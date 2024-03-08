use alloc::collections::{BTreeMap, BTreeSet};
use x86_64::{
    structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTableFlags},
    VirtAddr,
};

use crate::pmm::BuddyAllocator;

const PAGE_SIZE: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SizeAddr {
    size: usize,
    addr: VirtAddr,
}

struct VirtualMemoryManager<'a> {
    page_table: OffsetPageTable<'a>,
    frame_allocator: BuddyAllocator<'a>,
    addr_size_tree: BTreeMap<VirtAddr, usize>,
    size_addr_tree: BTreeSet<SizeAddr>,
    segment_buffer: [VirtAddr; 2],
}

impl<'a> VirtualMemoryManager<'a> {
    pub fn new(page_table: OffsetPageTable<'a>, pmm: BuddyAllocator<'a>) -> Self {
        Self {
            page_table,
            frame_allocator: pmm,
            addr_size_tree: BTreeMap::new(),
            size_addr_tree: BTreeSet::new(),
            segment_buffer: [VirtAddr::zero(); 2],
        }
    }

    fn alloc(&mut self, size: usize, order: u8) -> Option<VirtAddr> {
        let size = size + PAGE_SIZE - 1 & PAGE_SIZE - 1;
        let align = (1 << order).max(PAGE_SIZE);
        let free_size = size.max(align) + align - PAGE_SIZE;

        let entry = *(self.size_addr_tree)
            .range(
                SizeAddr {
                    size: free_size,
                    addr: VirtAddr::zero(),
                }..,
            )
            .next()?;
        let begin_addr = entry.addr.align_up(align as u64);
        self.size_addr_tree.remove(&entry);

        let before_entry = SizeAddr {
            size: (begin_addr - entry.addr) as _,
            addr: entry.addr,
        };

        if 0 < before_entry.size {
            self.size_addr_tree.insert(before_entry);
            *self.addr_size_tree.get_mut(&entry.addr).unwrap() = before_entry.size;
        } else {
            self.addr_size_tree.remove(&entry.addr);
        }

        let after_entry = SizeAddr {
            size: entry.size - before_entry.size - size,
            addr: begin_addr + size,
        };

        if 0 < after_entry.size {
            self.size_addr_tree.insert(after_entry);
            self.addr_size_tree
                .insert(after_entry.addr, after_entry.size);
        }

        unsafe {
            let frame = self.frame_allocator.allocate_frame()?;
            self.page_table.map_to(
                Page::from_start_address(begin_addr).unwrap(),
                frame,
                PageTableFlags::PRESENT,
                &mut self.frame_allocator,
            )
        };

        Some(begin_addr)
    }

    fn free(&mut self, virt: VirtAddr, size: usize, order: u8) {}
}
