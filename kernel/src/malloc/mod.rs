use core::{
    alloc::{GlobalAlloc, Layout},
    array,
    cell::UnsafeCell,
    hint::unreachable_unchecked,
    mem::{align_of, offset_of, size_of, MaybeUninit},
    ops,
    ptr::{self, NonNull},
    slice,
    sync::atomic::{self, AtomicPtr, AtomicU32, AtomicUsize},
};

mod thread_owned;

use thread_owned::ThreadOwned;

const SMALL_PAGE_SIZE: usize = 64 << 10;
const SEGMENT_SIZE: usize = 4 << 20;

const SMALL_SIZE_CLASSES: [usize; 33] = [
    0x8, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0xA0, 0xC0, 0xE0, 0x100, 0x140, 0x180,
    0x1C0, 0x200, 0x280, 0x300, 0x380, 0x400, 0x500, 0x600, 0x700, 0x800, 0xA00, 0xC00, 0xE00,
    0x1000, 0x1400, 0x1800, 0x1C00, 0x2000,
];
const LARGE_SIZE_CLASSES: [usize; 24] = [
    0x2800, 0x3000, 0x3800, 0x4000, 0x5000, 0x6000, 0x7000, 0x8000, 0xA000, 0xC000, 0xE000,
    0x10000, 0x14000, 0x18000, 0x1C000, 0x20000, 0x28000, 0x30000, 0x38000, 0x40000, 0x50000,
    0x60000, 0x70000, 0x80000,
];

const fn size_class(size: usize) -> usize {
    if size <= 64 {
        [0, 0, 1, 2, 2, 3, 3, 4, 4][size + 7 >> 3]
    } else {
        let bits = usize::BITS - size.leading_zeros();
        4 * bits as usize + (size - 1 >> bits - 3) - 27
    }
}

enum PageKind {
    /// 64 KiB pages
    Small,
    Large,
}

#[repr(u8)]
enum ThreadFreeState {
    Normal = 0,
    Delaying = 1,
    Delayed = 3,
}

struct PageMeta {
    /// Accessed only locally
    next_page: UnsafeCell<Option<NonNull<PageMeta>>>,
    /// Accessed only locally
    free: UnsafeCell<*mut FreeList>,
    /// Accessed only locally
    local_free: UnsafeCell<*mut FreeList>,
    thread_free: AtomicUsize,
    used: UnsafeCell<u32>,
    thread_freed: AtomicU32,
}

impl PageMeta {
    const fn new() -> Self {
        Self {
            next_page: UnsafeCell::new(None),
            free: UnsafeCell::new(ptr::null_mut()),
            local_free: UnsafeCell::new(ptr::null_mut()),
            thread_free: AtomicUsize::new(0),
            used: UnsafeCell::new(0),
            thread_freed: AtomicU32::new(0),
        }
    }

    fn set_thread_state_normal(&self) {
        self.thread_free.fetch_and(!3, atomic::Ordering::SeqCst);
    }
    fn set_thread_state_delayed(&self) {
        self.thread_free.fetch_or(3, atomic::Ordering::SeqCst);
    }
    /// Changes `self.thread_free` from `Delayed` to `Delaying`.
    /// Doesn't do anything if `self.thread_free` isn't `Delayed`.
    fn transition_to_delaying(&self) {
        self.thread_free.fetch_and(1, atomic::Ordering::SeqCst);
    }
    fn thread_free(&self) -> (ThreadFreeState, Option<NonNull<Self>>) {
        let thread_free = self.thread_free.load(atomic::Ordering::Relaxed);
        (
            match thread_free & 3 {
                0 => ThreadFreeState::Normal,
                3 => ThreadFreeState::Delayed,
                1 => ThreadFreeState::Delaying,
                _ => unsafe { unreachable_unchecked() },
            },
            NonNull::new((thread_free & !7) as _),
        )
    }
}

impl ThreadOwned<'_, PageMeta> {
    #[inline]
    fn next_page(&mut self) -> &mut Option<NonNull<PageMeta>> {
        unsafe { &mut *self.next_page.get() }
    }
    #[inline]
    fn free(&mut self) -> &mut *mut FreeList {
        unsafe { &mut *self.free.get() }
    }
    #[inline]
    fn local_free(&mut self) -> &mut *mut FreeList {
        unsafe { &mut *self.local_free.get() }
    }
    #[inline]
    fn used(&mut self) -> &mut u32 {
        unsafe { &mut *self.used.get() }
    }
}

struct SegmentMeta {
    thread_id: u32,
    page_kind: PageKind,
    num_pages: usize,
    used: UnsafeCell<u8>,
}

#[repr(C, align(0x400000))]
struct Segment {
    meta: SegmentMeta,
    page: MaybeUninit<PageMeta>,
}

const _: () = {
    assert!(size_of::<Segment>() == SEGMENT_SIZE && align_of::<Segment>() == SEGMENT_SIZE);
    assert!(
        offset_of!(Segment, page) + (SEGMENT_SIZE / SMALL_PAGE_SIZE - 1) * size_of::<PageMeta>()
            <= SMALL_PAGE_SIZE
    );
    assert!(SEGMENT_SIZE & SEGMENT_SIZE - 1 == 0);
};

impl Segment {
    const PAGES_OFFSET: usize =
        size_of::<SegmentMeta>() + align_of::<PageMeta>() - 1 & !(align_of::<PageMeta>() - 1);

    #[must_use]
    fn pages(&self) -> &[PageMeta] {
        unsafe { slice::from_raw_parts(self.page.as_ptr(), self.meta.num_pages) }
    }

    #[must_use]
    fn pages_mut(&mut self) -> &mut [PageMeta] {
        unsafe { slice::from_raw_parts_mut(self.page.as_mut_ptr(), self.meta.num_pages) }
    }

    fn from_ptr<T>(ptr: *const T) -> *const Self {
        (ptr as usize & !(SEGMENT_SIZE - 1)) as *const _
    }

    fn small_page_id(page: *const PageMeta) -> usize {
        ((page as usize & SEGMENT_SIZE - 1) - offset_of!(Self, page)) / size_of::<PageMeta>()
    }
    fn small_page_start(page: *mut PageMeta) -> *mut u8 {
        ((page as usize & !(SEGMENT_SIZE - 1)) + SMALL_PAGE_SIZE * (1 + Self::small_page_id(page)))
            as _
    }
}

impl ThreadOwned<'_, Segment> {
    fn meta(&mut self) -> ThreadOwned<'_, SegmentMeta> {
        unsafe { ThreadOwned::new(&self.meta) }
    }
}

impl ops::Deref for Segment {
    type Target = SegmentMeta;
    fn deref(&self) -> &SegmentMeta {
        &self.meta
    }
}

impl ops::DerefMut for Segment {
    fn deref_mut(&mut self) -> &mut SegmentMeta {
        &mut self.meta
    }
}

#[repr(C)]
struct FreeList {
    next: *mut Self,
}

#[repr(C)]
struct AtomicFreeList {
    next: AtomicPtr<Self>,
}

struct ThreadAllocator {
    thread_id: u32,
    /// Accessed only locally
    small_pages: [UnsafeCell<Option<NonNull<PageMeta>>>; SMALL_SIZE_CLASSES.len()],
    /// Accessed only locally
    large_pages: [UnsafeCell<Option<NonNull<PageMeta>>>; LARGE_SIZE_CLASSES.len()],
    /// Accessed only locally
    free_small_pages: UnsafeCell<Option<NonNull<PageMeta>>>,
    /// Accessed only locally
    full_pages: UnsafeCell<Option<NonNull<PageMeta>>>,
}

impl ThreadAllocator {
    pub fn new(thread_id: u32) -> Self {
        Self {
            thread_id,
            small_pages: SMALL_SIZE_CLASSES.map(|_| UnsafeCell::new(None)),
            large_pages: LARGE_SIZE_CLASSES.map(|_| UnsafeCell::new(None)),
            full_pages: UnsafeCell::new(None),
            free_small_pages: UnsafeCell::new(None),
        }
    }
}

impl ThreadOwned<'_, ThreadAllocator> {
    #[inline]
    fn free_small_pages(&mut self) -> &mut Option<NonNull<PageMeta>> {
        unsafe { &mut *self.free_small_pages.get() }
    }
    #[inline]
    fn full_pages(&mut self) -> &mut Option<NonNull<PageMeta>> {
        unsafe { &mut *self.full_pages.get() }
    }
    #[inline]
    fn small_page(&mut self, class: usize) -> &mut Option<NonNull<PageMeta>> {
        unsafe { &mut *self.small_pages[class].get() }
    }
    #[inline]
    fn large_page(&mut self, class: usize) -> &mut Option<NonNull<PageMeta>> {
        unsafe { &mut *self.large_pages[class].get() }
    }

    /// Access locally
    unsafe fn free_small_page(&mut self, page: &mut PageMeta) {
        *page.next_page.get_mut() = *self.free_small_pages();
        *self.free_small_pages() = Some(page.into());

        let seg = unsafe { &*Segment::from_ptr(page) };
        let seg_used = unsafe { &mut *seg.used.get() };
        *seg_used -= 1;
        if *seg_used == 0 {
            // Free segment
            todo!();
        }
    }

    /// Access locally
    unsafe fn get_small_page(&mut self, class: usize) -> &PageMeta {
        while let Some(page) = self.small_page(class) {
            let page = unsafe { page.as_ref() };
            let next_page = unsafe { *page.next_page.get() };
            if unsafe { *page.used.get() } == page.thread_freed.load(atomic::Ordering::Relaxed)
                && next_page.is_some()
            {
                *self.small_page(class) = next_page;
            } else {
                return page;
            }
        }

        let mut page = match *free_pages {
            Some(page) => page,
            None => {
                let segment: &mut MaybeUninit<Segment> = (|| todo!())();

                let segment = segment.write(Segment {
                    meta: SegmentMeta {
                        thread_id: self.thread_id,
                        page_kind: PageKind::Small,
                        num_pages: SEGMENT_SIZE / SMALL_PAGE_SIZE - 1,
                        used: 1,
                    },
                    page: MaybeUninit::uninit(),
                });
                let mut free_pages = None;
                for page in
                    unsafe { slice::from_raw_parts_mut(&mut segment.page, segment.meta.num_pages) }
                {
                    let page = page.write(PageMeta::new());
                    *page.next_page.get_mut() = free_pages.take();
                    free_pages = Some(page.into());
                }
                unsafe { free_pages.unwrap_unchecked() }
            }
        };

        let page = unsafe { page.as_mut() };
        *free_pages = page.next_page.get_mut().take();
        *class_page = Some(NonNull::from(&*page));

        let page_start = Segment::small_page_start(page);

        let free = page.free.get_mut();
        for offset in (0..SMALL_PAGE_SIZE).step_by(SMALL_SIZE_CLASSES[class]) {
            let node: *mut FreeList = unsafe { page_start.add(offset).cast() };
            unsafe { node.write(FreeList { next: *free }) };
            *free = node.cast();
        }

        page
    }

    pub unsafe fn alloc(&self, size: usize) -> *mut u8 {
        let class = size_class(size);

        if class < self.small_pages.len() {
            if let Some(page) = unsafe { *self.small_pages[class].get() } {
                let page = unsafe { page.as_ref() };
                let page_free = unsafe { &mut *page.free.get() };
                if let Some(free) = unsafe { page_free.as_mut() } {
                    *page_free = free.next;
                    return free as *mut _ as _;
                }
            }

            let page = unsafe { self.get_small_page(class) };

            let free = loop {
                match NonNull::new(unsafe { *page.free.get() })
                    .or_else(|| {
                        NonNull::new(unsafe { page.local_free.get().replace(ptr::null_mut()) })
                    })
                    .or_else(|| {
                        NonNull::new(page.thread_free.swap(0, atomic::Ordering::Relaxed) as _)
                    }) {
                    Some(ptr) => break ptr,
                    None => {
                        let full_pages = unsafe { &mut *self.full_pages.get() };
                        *full_pages = full_pages.take();

                        unsafe { *self.small_pages[class].get() = (*page.next_page.get()).take() };
                        todo!();
                    }
                }
            };

            todo!();
        } else if class < SMALL_SIZE_CLASSES.len() + LARGE_SIZE_CLASSES.len() {
            let page = self.large_pages[class - SMALL_SIZE_CLASSES.len()];
            todo!();
        } else {
            todo!();
        }
    }
}

struct Allocator {
    thread_allocs: [UnsafeCell<ThreadAllocator>; 1],
}

unsafe impl Sync for Allocator {}
unsafe impl Send for Allocator {}

impl Allocator {
    pub fn new() -> Self {
        Self {
            thread_allocs: array::from_fn(|thread_id| {
                UnsafeCell::new(ThreadAllocator::new(thread_id as _))
            }),
        }
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Get this thread's id
        unsafe { (*self.thread_allocs[0].get()).alloc(layout.pad_to_align().size()) }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        todo!();
    }
}

struct LazyAllocator(spin::Lazy<Allocator>);

impl LazyAllocator {
    pub const fn new() -> Self {
        Self(spin::Lazy::new(Allocator::new))
    }
}

unsafe impl GlobalAlloc for LazyAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { self.0.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.0.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: LazyAllocator = LazyAllocator::new();
