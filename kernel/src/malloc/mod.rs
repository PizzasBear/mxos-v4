use core::{
    alloc::{GlobalAlloc, Layout},
    array,
    cell::UnsafeCell,
    hint::unreachable_unchecked,
    mem::{self, MaybeUninit},
    ops,
    ptr::{self, NonNull},
    sync::atomic::{self, AtomicPtr, AtomicU32, AtomicUsize, Ordering::SeqCst},
};

macro_rules! cfor {
    ($ident:ident in range($end:expr) $block:block) => {
        cfor!($ident in range(0, $end) $block);
    };
    ($ident:ident in range($start:expr, $end:expr) $block:block) => {{
        let mut $ident = $start..$end;
        while $ident.start < $ident.end {
            let $ident = {
                $ident.start += 1;
                $ident.start - 1
            };
            $block
        }
    }};
}

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
const NUM_SIZE_CLASSES: usize = SMALL_SIZE_CLASSES.len() + LARGE_SIZE_CLASSES.len();
const LARGE_SIZE_CLASS_PAGE_STARTS: [usize; LARGE_SIZE_CLASSES.len()] = {
    let mut a = [0; LARGE_SIZE_CLASSES.len()];
    cfor!(i in range(LARGE_SIZE_CLASSES.len()) {
        a[i] = SEGMENT_SIZE % LARGE_SIZE_CLASSES[i];
        if a[i] == 0 {
            a[i] = LARGE_SIZE_CLASSES[i];
        }
    });
    a
};

const fn size_class(size: usize) -> usize {
    if size <= 64 {
        [0, 0, 1, 2, 2, 3, 3, 4, 4][size + 7 >> 3]
    } else {
        let bits = usize::BITS - size.leading_zeros();
        4 * bits as usize + (size - 1 >> bits - 3) - 27
    }
}

#[repr(transparent)]
struct ThreadOwned<T>(UnsafeCell<T>);

impl<T> ThreadOwned<T> {
    fn from_mut(inner: &mut T) -> &Self {
        unsafe { &*(inner as *mut _ as *const _) }
    }
    unsafe fn from_ref(inner: &T) -> &Self {
        unsafe { &*(inner as *const _ as *const _) }
    }

    unsafe fn upgrade_exclusive(&self) -> &mut T {
        unsafe { &mut *self.0.get() }
    }
}

impl<'a, T> From<&'a mut T> for &'a ThreadOwned<T> {
    fn from(value: &'a mut T) -> Self {
        ThreadOwned::from_mut(value)
    }
}

impl<T> ops::Deref for ThreadOwned<T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.0.get() }
    }
}

impl<T> ops::DerefMut for ThreadOwned<T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.0.get() }
    }
}

#[repr(u8)]
enum ThreadFreeState {
    Normal = 0,
    Delaying = 1,
    Delayed = 3,
}

struct PageMeta {
    next: UnsafeCell<ThreadPagePtr>,
    prev_next: UnsafeCell<NonNull<ThreadPagePtr>>,
    free: UnsafeCell<*mut FreeList>,
    local_free: UnsafeCell<*mut FreeList>,
    thread_free: AtomicUsize,
    used: UnsafeCell<u32>,
    thread_freed: AtomicU32,
    is_full: UnsafeCell<bool>,
}

unsafe impl Sync for PageMeta {}

impl PageMeta {
    const fn new(prev_next: NonNull<ThreadPagePtr>) -> Self {
        Self {
            next: UnsafeCell::new(None),
            prev_next: UnsafeCell::new(prev_next),
            free: UnsafeCell::new(ptr::null_mut()),
            local_free: UnsafeCell::new(ptr::null_mut()),
            thread_free: AtomicUsize::new(0),
            used: UnsafeCell::new(0),
            thread_freed: AtomicU32::new(0),
            is_full: UnsafeCell::new(false),
        }
    }

    fn thread_free(&self) -> (usize, ThreadFreeState, *mut FreeList) {
        let thread_free = self.thread_free.load(SeqCst);
        Self::split_thread_free(thread_free)
    }
    fn split_thread_free(thread_free: usize) -> (usize, ThreadFreeState, *mut FreeList) {
        (
            thread_free,
            match thread_free & 3 {
                0 => ThreadFreeState::Normal,
                3 => ThreadFreeState::Delayed,
                1 => ThreadFreeState::Delaying,
                _ => unsafe { unreachable_unchecked() },
            },
            (thread_free & !7) as _,
        )
    }
}

struct SegmentMeta {
    thread_id: u32,
    class: u8,
    used: UnsafeCell<u8>,
}

#[repr(C, align(0x400000))]
struct Segment {
    meta: SegmentMeta,
    pages: [MaybeUninit<PageMeta>; SEGMENT_SIZE / SMALL_PAGE_SIZE - 1],
    end_marker: (),
}

unsafe impl Sync for Segment {}

const _: () = {
    assert!(
        mem::size_of::<Segment>() == SEGMENT_SIZE && mem::align_of::<Segment>() == SEGMENT_SIZE
    );
    assert!(mem::offset_of!(Segment, end_marker) <= SMALL_PAGE_SIZE);
    cfor!(i in range(LARGE_SIZE_CLASS_PAGE_STARTS.len()) {
        assert!(mem::offset_of!(Segment, end_marker) <= LARGE_SIZE_CLASS_PAGE_STARTS[i]);
    });
    assert!(SEGMENT_SIZE & SEGMENT_SIZE - 1 == 0);
};

impl Segment {
    fn from_ptr<T>(ptr: *const T) -> *const Self {
        (ptr as usize & !(SEGMENT_SIZE - 1)) as *const _
    }

    fn small_page_id(page: *const PageMeta) -> usize {
        ((page as usize & SEGMENT_SIZE - 1) - mem::offset_of!(Segment, pages))
            / mem::size_of::<PageMeta>()
    }
    fn small_page_start(page: *mut PageMeta) -> *mut u8 {
        ((page as usize & !(SEGMENT_SIZE - 1)) + SMALL_PAGE_SIZE * (1 + Self::small_page_id(page)))
            as _
    }
    fn block_small_page_id(block: *const FreeList) -> usize {
        (block as usize & SEGMENT_SIZE - 1) / SMALL_PAGE_SIZE - 1
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

#[repr(transparent)]
struct FreeList {
    next: *mut Self,
}

// #[repr(transparent)]
// struct AtomicFreeList {
//     next: AtomicPtr<Self>,
// }

type ThreadPagePtr = Option<NonNull<ThreadOwned<PageMeta>>>;

struct ThreadAllocator {
    thread_id: u32,
    /// Accessed only locally
    pages: [UnsafeCell<ThreadPagePtr>; NUM_SIZE_CLASSES],
    /// Accessed only locally
    free_small_pages: UnsafeCell<ThreadPagePtr>,
    /// Accessed only locally
    full_pages: UnsafeCell<ThreadPagePtr>,
    delayed_free: AtomicPtr<FreeList>,
}

unsafe impl Sync for ThreadAllocator {}

impl ThreadAllocator {
    pub fn new(thread_id: u32) -> Self {
        Self {
            thread_id,
            pages: array::from_fn(|_| UnsafeCell::new(None)),
            full_pages: UnsafeCell::new(None),
            free_small_pages: UnsafeCell::new(None),
            delayed_free: AtomicPtr::new(ptr::null_mut()),
        }
    }
}

unsafe fn push_page(list: &mut ThreadPagePtr, page: &ThreadOwned<PageMeta>) {
    if let Some(list) = list {
        unsafe { *list.as_ref().prev_next.get() = NonNull::new_unchecked(page.next.get()) };
    }
    unsafe { *page.next.get() = *list };
    unsafe { *page.prev_next.get() = list.into() };
    *list = Some(page.into());
}

unsafe fn remove_page(page: &ThreadOwned<PageMeta>) {
    unsafe { *(*page.prev_next.get()).as_mut() = *page.next.get() };
    if let Some(next_page) = unsafe { *page.next.get() } {
        unsafe { *next_page.as_ref().prev_next.get() = *page.prev_next.get() };
    }
}

// unsafe fn pop_page(list: &mut ThreadPagePtr) -> Option<&ThreadOwned<PageMeta>> {
//     let page = unsafe { list.as_mut()?.as_ref() };
//     *list = unsafe { *page.next.get() };
//     if let Some(page_next) = *list {
//         unsafe { *page_next.as_ref().prev_next.get() = list.into() };
//     }
//     Some(page)
// }

impl ThreadOwned<ThreadAllocator> {
    unsafe fn free_small_page(&self, page: &mut PageMeta) {
        let seg = unsafe { ThreadOwned::from_ref(&*Segment::from_ptr(page)) };
        let seg_used = unsafe { &mut *seg.used.get() };
        *seg_used -= 1;
        if *seg_used == 0 {
            // Free segment
            todo!();
        } else {
            let free_pages = unsafe { &mut *self.free_small_pages.get() };
            unsafe { push_page(free_pages, page.into()) };
        }
    }

    unsafe fn find_page(&self, class: usize) -> Option<&ThreadOwned<PageMeta>> {
        while let Some(page) = unsafe { *self.pages[class].get() } {
            let page = unsafe { page.as_ref() };
            let next_page = unsafe { *page.next.get() };
            if unsafe { *page.used.get() } == page.thread_freed.load(atomic::Ordering::Relaxed)
                && next_page.is_some()
            {
                unsafe { remove_page(page) };
                unsafe { self.free_small_page(page.upgrade_exclusive()) };
            } else {
                let seg = unsafe { ThreadOwned::from_ref(&*Segment::from_ptr(page)) };
                unsafe { *seg.used.get() += 1 };
                return Some(page);
            }
        }
        None
    }

    unsafe fn alloc_small_page(&self, class: usize) -> &ThreadOwned<PageMeta> {
        let free_small_pages = unsafe { &mut *self.free_small_pages.get() };
        let page = match *free_small_pages {
            Some(mut page) => unsafe { &mut **page.as_mut() },
            None => {
                let segment: &mut MaybeUninit<Segment> = (|| todo!())();

                let segment = segment.write(Segment {
                    meta: SegmentMeta {
                        thread_id: self.thread_id,
                        class: class as _,
                        used: UnsafeCell::new(1),
                    },
                    pages: array::from_fn(|_| MaybeUninit::new(PageMeta::new(NonNull::dangling()))),
                    end_marker: (),
                });
                for page in &mut segment.pages {
                    let page = unsafe { page.assume_init_mut() };
                    unsafe { push_page(free_small_pages, page.into()) };
                }
                unsafe { &mut **free_small_pages.unwrap_unchecked().as_mut() }
            }
        };

        unsafe { remove_page(page.into()) };
        unsafe { push_page(free_small_pages, page.into()) };

        let page_start = Segment::small_page_start(page as _);

        // page.capacity = SMALL_PAGE_SIZE as u32 / SMALL_SIZE_CLASSES[class] as u32;

        let free = page.free.get_mut();
        for offset in (0..SMALL_PAGE_SIZE).step_by(SMALL_SIZE_CLASSES[class]) {
            let node: *mut FreeList = unsafe { page_start.add(offset).cast() };
            unsafe { node.write(FreeList { next: *free }) };
            *free = node.cast();
        }

        page.into()
    }

    unsafe fn alloc_large_page(&self, class: usize) -> &mut PageMeta {
        let large_class = class - SMALL_SIZE_CLASSES.len();
        let segment: &mut MaybeUninit<Segment> = (|| todo!())();

        let segment = segment.write(Segment {
            meta: SegmentMeta {
                thread_id: self.thread_id,
                class: class as _,
                used: UnsafeCell::new(1),
            },
            pages: array::from_fn(|_| MaybeUninit::uninit()),
            end_marker: (),
        });
        let seg_ptr = ptr::from_mut(segment);

        let page = segment.pages[0].write(PageMeta::new(NonNull::dangling()));

        let free = page.free.get_mut();
        for offset in (LARGE_SIZE_CLASS_PAGE_STARTS[large_class]..SEGMENT_SIZE)
            .step_by(LARGE_SIZE_CLASSES[large_class])
        {
            let node: *mut FreeList = unsafe { seg_ptr.add(offset).cast() };
            unsafe { node.write(FreeList { next: *free }) };
            *free = node.cast();
        }

        unsafe { push_page(&mut *self.pages[class].get(), page.into()) };

        page
    }

    unsafe fn local_free(
        &self,
        class: usize,
        page: &ThreadOwned<PageMeta>,
        mut free: NonNull<FreeList>,
    ) {
        if unsafe { page.is_full.get().replace(false) } {
            unsafe { remove_page(page) };
            unsafe { push_page(&mut *self.pages[class].get(), page) };
        }

        let local_free = unsafe { &mut *page.local_free.get() };
        unsafe { free.as_mut().next = *local_free };
        *local_free = free.as_ptr();
    }

    pub unsafe fn alloc(&self, size: usize) -> *mut u8 {
        if *LARGE_SIZE_CLASSES.last().unwrap() < size {
            // huge allocation
            todo!();
        }

        let class = size_class(size);

        if let Some(page) = unsafe { *self.pages[class].get() } {
            let page = unsafe { page.as_ref() };
            let page_free = unsafe { &mut *page.free.get() };
            if let Some(free) = unsafe { page_free.as_mut() } {
                unsafe { *page.used.get() += 1 };
                *page_free = free.next;
                return free as *mut _ as _;
            }
        }

        let mut delayed_free = self.delayed_free.swap(ptr::null_mut(), SeqCst);
        while let Some(free) = NonNull::new(delayed_free) {
            delayed_free = unsafe { free.as_ref().next };

            let seg = unsafe { ThreadOwned::from_ref(&*Segment::from_ptr(free.as_ptr())) };
            let page_id = match (seg.class as usize) < SMALL_SIZE_CLASSES.len() {
                true => Segment::block_small_page_id(free.as_ptr() as _),
                false => 0,
            };
            let page = unsafe { ThreadOwned::from_ref(seg.pages[page_id].assume_init_ref()) };
            unsafe { self.local_free(seg.class as _, page, free) };
        }

        loop {
            let page = unsafe {
                self.find_page(class)
                    .unwrap_or_else(|| match class < SMALL_SIZE_CLASSES.len() {
                        true => self.alloc_small_page(class),
                        false => ThreadOwned::from_mut(self.alloc_large_page(class)),
                    })
            };

            match NonNull::new(unsafe { *page.free.get() })
                .or_else(|| NonNull::new(unsafe { page.local_free.get().replace(ptr::null_mut()) }))
                .or_else(|| {
                    page.thread_free
                        .compare_exchange(
                            ThreadFreeState::Normal as _,
                            ThreadFreeState::Delayed as _,
                            SeqCst,
                            SeqCst,
                        )
                        .err()
                        .map(|_| unsafe {
                            // SAFETY: We checked that it isn't zero and other threads won't zero it
                            NonNull::new_unchecked(
                                (page.thread_free.swap(ThreadFreeState::Normal as _, SeqCst) & !7)
                                    as _,
                            )
                        })
                }) {
                Some(free) => unsafe {
                    *page.used.get() += 1;
                    *page.free.get() = free.as_ref().next;
                    break free.as_ptr() as _;
                },
                None => unsafe {
                    remove_page(page);
                    *page.is_full.get() = true;
                    push_page(&mut *self.full_pages.get(), page);
                },
            }
        }
    }
}

struct Allocator {
    thread_allocs: [ThreadAllocator; 1],
}

unsafe impl Sync for Allocator {}
unsafe impl Send for Allocator {}

impl Allocator {
    pub fn new() -> Self {
        Self {
            thread_allocs: array::from_fn(|thread_id| ThreadAllocator::new(thread_id as _)),
        }
    }
}

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let thread_id = 0;

        // Get this thread's id
        unsafe {
            ThreadOwned::from_ref(&self.thread_allocs[thread_id])
                .alloc(layout.align_to(8).unwrap().pad_to_align().size())
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let thread_id = 0;

        let size = layout.align_to(8).unwrap().pad_to_align().size();
        if *LARGE_SIZE_CLASSES.last().unwrap() < size {
            todo!();
        }

        let ptr = unsafe { &mut *ptr.cast() };

        let seg = unsafe { &*Segment::from_ptr(ptr) };
        let page_id = match (seg.class as usize) < SMALL_SIZE_CLASSES.len() {
            true => Segment::block_small_page_id(ptr),
            false => 0,
        };
        let page = unsafe { seg.pages[page_id].assume_init_ref() };

        if thread_id == seg.thread_id {
            let page = unsafe { ThreadOwned::from_ref(page) };
            let local_free = unsafe { &mut *page.local_free.get() };
            ptr.next = *local_free;
            *local_free = ptr;
        } else {
            let (mut cur, mut state, mut thread_free) = page.thread_free();
            let mut delaying_counter = 0;
            loop {
                match state {
                    ThreadFreeState::Normal => {
                        ptr.next = thread_free;
                        match page.thread_free.compare_exchange(
                            cur,
                            ptr as *mut _ as _,
                            SeqCst,
                            SeqCst,
                        ) {
                            Ok(_) => break,
                            Err(new) => {
                                (cur, state, thread_free) = PageMeta::split_thread_free(new);
                                continue;
                            }
                        }
                    }
                    ThreadFreeState::Delaying if delaying_counter < 4 => {
                        delaying_counter += 1;
                        (cur, state, thread_free) = page.thread_free();
                        continue;
                    }
                    ThreadFreeState::Delayed | ThreadFreeState::Delaying => {
                        match page.thread_free.compare_exchange(
                            cur,
                            ThreadFreeState::Delaying as _,
                            SeqCst,
                            SeqCst,
                        ) {
                            Ok(_) => {
                                let alloc = &self.thread_allocs[seg.thread_id as usize];
                                ptr.next = alloc.delayed_free.load(SeqCst);
                                while let Err(new_next) = (alloc.delayed_free)
                                    .compare_exchange(ptr.next, ptr, SeqCst, SeqCst)
                                {
                                    ptr.next = new_next;
                                }
                                page.thread_free.store(ThreadFreeState::Normal as _, SeqCst);
                                break;
                            }
                            Err(new) => {
                                (cur, state, thread_free) = PageMeta::split_thread_free(new);
                                continue;
                            }
                        }
                    }
                }
            }
        }
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
