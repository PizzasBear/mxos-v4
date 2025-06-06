#![allow(unused)]

use core::{array, iter, mem, ops, ptr::NonNull, slice};

use bootloader_api::info::{MemoryRegion, MemoryRegionKind};
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{
        FrameAllocator, FrameDeallocator, OffsetPageTable, PageSize, PhysFrame, Size2MiB, Size4KiB,
    },
};

use crate::bitmap::Bitmap;

struct FreeList {
    next: Option<NonNull<Self>>,
}

#[derive(Debug)]
struct Buddy<'a> {
    // top_level: bool,
    // phys_offset: usize,
    // /// log2 size
    // order: u8,
    free_list: Option<NonNull<FreeList>>,
    map: &'a mut Bitmap,
}

pub unsafe fn init(
    mapper: &OffsetPageTable,
    memory_regions: &[MemoryRegion],
    memory_size: u64,
) -> BuddyAllocator<'static> {
    let buddy_map_len = BuddyAllocator::buddy_map_len(memory_size as _);

    let mut start = 0;
    let mut end = 0;

    let mut phys_alloc = None;
    let mut buddy_map_start = 0;
    for r in &*memory_regions {
        if r.kind != MemoryRegionKind::Usable || r.start < 0x100000 {
            continue;
        }
        if end < r.start {
            start = r.start + 4095 & !4095;
        }
        end = r.end;

        if (mem::size_of::<usize>() * buddy_map_len + 4095) & !4095
            <= ((end & !4095) - start) as usize
        {
            buddy_map_start = start;
            let buddy_map_ptr = (mapper.phys_offset() + start).as_mut_ptr();
            phys_alloc = Some(BuddyAllocator::new(memory_size as _, &mapper, unsafe {
                slice::from_raw_parts_mut(buddy_map_ptr, buddy_map_len)
            }));
            break;
        }
    }
    let mut allocator = phys_alloc.unwrap();

    let mut start = 0;
    let mut end = 0;
    for r in memory_regions.iter() {
        if r.kind != MemoryRegionKind::Usable || r.start < 0x100000 {
            continue;
        }
        if end < r.start {
            if start == buddy_map_start {
                start += (mem::size_of::<usize>() * buddy_map_len) as u64 + 4095;
                start &= !4095;
            }
            // blue waffle
            allocator.free_region(PhysAddr::new(start)..PhysAddr::new(end));

            start = r.start + 4095 & !4095;
        }
        end = r.end;
    }

    if start == buddy_map_start {
        start += (mem::size_of::<usize>() * buddy_map_len) as u64 + 4095;
        start &= !4095;
    }
    allocator.free_region(PhysAddr::new(start)..PhysAddr::new(end));

    allocator
}

unsafe impl Send for Buddy<'_> {}

impl Buddy<'_> {
    // const fn map_size(memory_size: usize, order: u8) -> usize {
    //     const BITS: usize = usize::BITS as _;
    //     ((memory_size >> order + 1) + BITS - 1) / BITS
    // }

    fn toggle_chunk_pair(&mut self, bit: usize) {
        self.map.toggle(bit);
    }

    /// Returns `true` if chunks are different (one free and one allocated), and `false` if they are
    /// the same (both allocated or both free)
    fn is_chunk_pair_different(&self, bit: usize) -> bool {
        self.map.get(bit)
    }

    // fn ptr_to_bit(&self, addr: *const ()) -> usize {
    //     addr as usize - self.phys_offset >> self.order + 1
    // }

    // /// Returns true if merged
    // unsafe fn free(&mut self, ptr: NonNull<()>) -> bool {
    //     let bit = self.ptr_to_bit(ptr.as_ptr());
    //     self.toggle_chunk_pair(bit);
    //     if !self.top_level && !self.chunk_pair_xor(bit) {
    //         return true;
    //     }
    //     let next = self.free_list;
    //     unsafe {
    //         self.free_list
    //             .insert(ptr.cast())
    //             .as_ptr()
    //             .write(FreeList { next });
    //     }
    //     false
    // }
    unsafe fn push_free_list(&mut self, addr: VirtAddr) {
        let Some(ptr) = NonNull::new(addr.as_mut_ptr()) else {
            return;
        };
        let next = self.free_list;
        unsafe {
            self.free_list.insert(ptr).as_ptr().write(FreeList { next });
        }
    }

    fn pop_free_list(&mut self) -> Option<VirtAddr> {
        let mut free = self.free_list?;
        self.free_list = unsafe { free.as_mut().next };
        Some(VirtAddr::from_ptr(free.as_ptr()))
    }
}

// 2**12 bytes = 4 KiB
// 2**21 bytes = 2 MiB
// 2**30 bytes = 1 GiB

const ORDERS: ops::Range<u8> = 12..22;
// const MIN_ORDER: u8 = 21;
// const MAX_ORDER: u8 = 30;

#[derive(Debug)]
struct Buddies<'a>([Buddy<'a>; (ORDERS.end - ORDERS.start) as _]);

impl<'a> ops::Index<(ops::Bound<u8>, ops::Bound<u8>)> for Buddies<'a> {
    type Output = [Buddy<'a>];
    fn index(&self, (start, end): (ops::Bound<u8>, ops::Bound<u8>)) -> &[Buddy<'a>] {
        let map = |s| (s - ORDERS.start) as _;
        &self.0[(start.map(map), end.map(map))]
    }
}
impl<'a> ops::IndexMut<(ops::Bound<u8>, ops::Bound<u8>)> for Buddies<'a> {
    fn index_mut(&mut self, (start, end): (ops::Bound<u8>, ops::Bound<u8>)) -> &mut [Buddy<'a>] {
        let map = |s| (s - ORDERS.start) as _;
        &mut self.0[(start.map(map), end.map(map))]
    }
}
macro_rules! impl_index {
    ($ty:ty) => {
        impl<'a> ops::Index<$ty> for Buddies<'a> {
            type Output = [Buddy<'a>];
            fn index(&self, range: $ty) -> &[Buddy<'a>] {
                &self[(
                    ops::RangeBounds::start_bound(&range).cloned(),
                    ops::RangeBounds::end_bound(&range).cloned(),
                )]
            }
        }
        impl<'a> ops::IndexMut<$ty> for Buddies<'a> {
            fn index_mut(&mut self, range: $ty) -> &mut [Buddy<'a>] {
                &mut self[(
                    ops::RangeBounds::start_bound(&range).cloned(),
                    ops::RangeBounds::end_bound(&range).cloned(),
                )]
            }
        }
    };
}
impl_index!(ops::Range<u8>);
impl_index!(ops::RangeFrom<u8>);
impl_index!(ops::RangeInclusive<u8>);
impl_index!(ops::RangeToInclusive<u8>);
impl_index!(ops::RangeFull);
impl<'a> ops::Deref for Buddies<'a> {
    type Target = [Buddy<'a>; (ORDERS.end - ORDERS.start) as _];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<'a> ops::DerefMut for Buddies<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug)]
pub struct BuddyAllocator<'a> {
    buddies: Buddies<'a>,
    phys_offset: VirtAddr,
}

impl<'a> BuddyAllocator<'a> {
    const fn order_map_size(memory_size: usize, order: u8) -> usize {
        const BITS: usize = usize::BITS as _;
        ((memory_size >> order + 1) + BITS - 1) / BITS
    }

    /// Required length of the `buddy_map` slice.
    pub const fn buddy_map_len(memory_size: usize) -> usize {
        let mut sum = 0;
        let mut order = ORDERS.start;
        while order < ORDERS.end {
            sum += Self::order_map_size(memory_size, order);
            order += 1;
        }
        sum
    }

    pub fn new(
        memory_size: usize,
        page_table: &OffsetPageTable,
        mut buddy_map: &'a mut [usize],
    ) -> Self {
        assert!(Self::buddy_map_len(memory_size) <= buddy_map.len());

        log::info!("PRE_FILLED_BUDDY_MAP");
        buddy_map.fill(0);

        log::info!("FILLED_BUDDY_MAP");

        let mut buddies = array::from_fn(|i| {
            let order = ORDERS.start + i as u8;
            let top_level = order + 1 == ORDERS.end;

            Buddy {
                // top_level,
                // phys_offset,
                // order,
                free_list: None,
                map: Bitmap::from_slice_mut(&mut []),
            }
        });
        for (order, buddy) in ORDERS.zip(&mut buddies) {
            let map;
            (map, buddy_map) = buddy_map.split_at_mut(Self::order_map_size(memory_size, order));
            buddy.map = map.into();
        }

        Self {
            buddies: Buddies(buddies),
            phys_offset: page_table.phys_offset(),
        }
    }

    pub fn free_region(&mut self, range: ops::Range<PhysAddr>) {
        log::info!("free_region: {range:?}");

        let ops::Range { mut start, mut end } = range;
        assert!(start.is_aligned(1u64 << ORDERS.start));
        assert!(end.is_aligned(1u64 << ORDERS.start));

        let mut start = (start.as_u64() >> ORDERS.start - 1) as usize;
        let mut end = (end.as_u64() >> ORDERS.start - 1) as usize;

        // [........][........][........][........]
        // [...][...][...][...][...][...][...][...]

        for order in ORDERS {
            start /= 2;
            end /= 2;

            if end <= start {
                break;
            }

            if start & 1 != 0 {
                self.free(order, PhysAddr::new((start << order) as _));
                start += 1;
            }
            if end & 1 != 0 {
                end -= 1;
                self.free(order, PhysAddr::new((end << order) as _));
            }
        }

        let order = ORDERS.end - 1;
        for i in start..end {
            self.free(order, PhysAddr::new((i << order) as _));
        }
    }

    pub fn free(&mut self, order: u8, addr: PhysAddr) {
        // log::info!(
        //     "free: order={order} range={:?}",
        //     addr..addr + (1u64 << order)
        // );

        assert!(addr.is_aligned(1u64 << order));

        let mut pair = (addr.as_u64() >> order) as usize;
        for (order, buddy) in (order..).zip(&mut self.buddies[order..]) {
            pair /= 2;
            buddy.toggle_chunk_pair(pair);
            if buddy.is_chunk_pair_different(pair) {
                unsafe { buddy.push_free_list(self.phys_offset + addr.as_u64()) };
                return;
            }
        }
        unsafe {
            (self.buddies.last_mut().unwrap()).push_free_list(self.phys_offset + addr.as_u64());
        }
    }

    pub fn alloc(&mut self, order: u8) -> Option<PhysAddr> {
        if 12 < order {
            log::info!("alloc: order={order}");
        }

        assert!(ORDERS.contains(&order));

        for (buddy_order, buddy) in (order..).zip(&mut self.buddies[order..]) {
            let Some(addr) = buddy.pop_free_list() else {
                continue;
            };
            let addr = PhysAddr::new(addr - self.phys_offset);

            for (buddy_order, buddy) in (order..).zip(&mut self.buddies[order..=buddy_order]) {
                buddy.toggle_chunk_pair((addr.as_u64() >> buddy_order + 1) as _);
            }
            return Some(addr);
        }

        None
    }
}

unsafe impl FrameAllocator<Size4KiB> for BuddyAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        Some(PhysFrame::from_start_address(self.alloc(12)?).unwrap())
    }
}

impl FrameDeallocator<Size4KiB> for BuddyAllocator<'_> {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame) {
        self.free(12, frame.start_address());
    }
}

unsafe impl FrameAllocator<Size2MiB> for BuddyAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size2MiB>> {
        Some(PhysFrame::from_start_address(self.alloc(21)?).unwrap())
    }
}

impl FrameDeallocator<Size2MiB> for BuddyAllocator<'_> {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size2MiB>) {
        self.free(21, frame.start_address());
    }
}
