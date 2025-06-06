use core::fmt;

use alloc::collections::{BTreeMap, BTreeSet};
use bootloader_api::info::MemoryRegion;
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageSize, PageTable, PageTableFlags,
        PhysFrame, Size2MiB, Size4KiB,
        mapper::{MapToError, MapperFlush},
        page_table::PageTableLevel,
    },
};

use super::{
    malloc::ALLOC,
    pmm::{self, BuddyAllocator},
};

const PAGE_SIZE: usize = Size4KiB::SIZE as _;
const HUGE_PAGE_SIZE: usize = Size2MiB::SIZE as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SizeAddr {
    size: usize,
    addr: usize,
}

impl SizeAddr {
    #[inline]
    pub const fn new(size: usize, addr: usize) -> Self {
        Self { size, addr }
    }
}

#[derive(Debug)]
struct TreeBestFitAlloc {
    addr_size_tree: BTreeMap<usize, usize>,
    size_addr_tree: BTreeSet<SizeAddr>,
}

impl TreeBestFitAlloc {
    pub fn new() -> Self {
        Self {
            addr_size_tree: BTreeMap::new(),
            size_addr_tree: BTreeSet::new(),
        }
    }

    fn alloc(&mut self, size: usize, align_order: u8) -> Option<SizeAddr> {
        let size = size + PAGE_SIZE - 1 & !(PAGE_SIZE - 1);
        let align = (1 << align_order).max(PAGE_SIZE);
        let free_size = size.max(align) + align - PAGE_SIZE;

        let entry = *(self.size_addr_tree)
            .range(SizeAddr::new(free_size, 0)..)
            .next()?;
        let addr = entry.addr + align - 1 & !(align - 1);
        self.size_addr_tree.remove(&entry);

        let before_entry = SizeAddr::new((addr - entry.addr) as _, entry.addr);

        if 0 < before_entry.size {
            self.size_addr_tree.insert(before_entry);
            *self.addr_size_tree.get_mut(&entry.addr).unwrap() = before_entry.size;
        } else {
            self.addr_size_tree.remove(&entry.addr);
        }

        let after_entry = SizeAddr::new(entry.size - before_entry.size - size, addr + size);

        if 0 < after_entry.size {
            self.size_addr_tree.insert(after_entry);
            self.addr_size_tree
                .insert(after_entry.addr, after_entry.size);
        }

        Some(SizeAddr { size, addr })
    }

    fn free(&mut self, mut addr: usize, mut size: usize) {
        // log::info!("We shall free: addr={addr:?} size={size}");
        addr &= !(PAGE_SIZE - 1);
        size = size + PAGE_SIZE - 1 & !(PAGE_SIZE - 1);

        // log::info!("JOE SHAV 1");

        if let Some((&prev_addr, &prev_size)) = self.addr_size_tree.range(..addr).next_back() {
            if addr <= prev_addr + prev_size {
                self.addr_size_tree.remove(&prev_addr);
                self.size_addr_tree
                    .remove(&SizeAddr::new(prev_size, prev_addr));
                size = prev_size.max((addr - prev_addr) + size);
                addr = prev_addr;
            }
        }

        // log::info!("JOE SHAV 2");

        if let Some((next_addr, next_size)) = (addr.checked_add(size))
            .and_then(|next_addr| self.addr_size_tree.remove_entry(&next_addr))
        {
            self.size_addr_tree
                .remove(&SizeAddr::new(next_size, next_addr));
            size += next_size;
        }

        // log::info!("New memory to be created");
        self.addr_size_tree.insert(addr as _, size);
        // log::info!("A thing");
        self.size_addr_tree.insert(SizeAddr::new(size, addr));
        // log::info!("Thou was freed: addr={addr:?} size={size}");
    }
}

pub struct VirtualMemoryManager<'a> {
    page_table: OffsetPageTable<'a>,
    frame_allocator: BuddyAllocator<'a>,
    kernel_alloc: TreeBestFitAlloc,
    user_alloc: TreeBestFitAlloc,
    kernel_start: VirtAddr,
}

impl<'a> fmt::Debug for VirtualMemoryManager<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VirtualMemoryManager")
            .field("page_table", &format_args!("OffsetPageTable {{ ... }}"))
            .field("frame_allocator", &format_args!("BuddyAllocator {{ ... }}"))
            .field("kernel_alloc", &self.kernel_alloc)
            .field("user_alloc", &self.user_alloc)
            .field("kernel_start", &self.kernel_start)
            .finish()
    }
}

impl<'a> VirtualMemoryManager<'a> {
    pub fn new(
        kernel_start: VirtAddr,
        page_table: OffsetPageTable<'a>,
        frame_allocator: BuddyAllocator<'a>,
    ) -> Self {
        Self {
            page_table,
            kernel_start,
            frame_allocator,
            kernel_alloc: TreeBestFitAlloc::new(),
            user_alloc: TreeBestFitAlloc::new(),
        }
    }

    unsafe fn page_map<S: PageSize + fmt::Debug>(
        &mut self,
        addr: VirtAddr,
        frame: PhysFrame<S>,
        page_flags: PageTableFlags,
    ) -> Result<MapperFlush<S>, MapToError<S>>
    where
        OffsetPageTable<'a>: Mapper<S>,
    {
        unsafe {
            self.page_table.map_to(
                Page::from_start_address(addr).unwrap(),
                frame,
                page_flags,
                &mut self.frame_allocator,
            )
        }
    }

    /// Make sure that `phys_addr` is not mapped to any virtual address.
    pub unsafe fn map(
        &mut self,
        kernel: bool,
        mut size: usize,
        align_order: u8,
        mut phys_addr: PhysAddr,
    ) -> Option<VirtAddr> {
        let addr_offset = phys_addr.as_u64() as usize & (PAGE_SIZE - 1);
        phys_addr -= addr_offset as u64;
        size += addr_offset;

        let SizeAddr { mut size, addr } = match kernel {
            true => self.kernel_alloc.alloc(size, align_order)?,
            false => self.user_alloc.alloc(size, align_order)?,
        };
        let mut addr = VirtAddr::new(addr as _);
        let return_addr = addr + addr_offset as u64;

        let mut page_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        page_flags.set(PageTableFlags::USER_ACCESSIBLE, !kernel);
        while 0 < size && !addr.is_aligned(HUGE_PAGE_SIZE as u64) {
            let frame = unsafe { PhysFrame::<Size4KiB>::from_start_address_unchecked(phys_addr) };
            unsafe { self.page_map(addr, frame, page_flags).unwrap().flush() };
            phys_addr += PAGE_SIZE as u64;
            addr += PAGE_SIZE as u64;
            size -= PAGE_SIZE;
        }
        while HUGE_PAGE_SIZE <= size {
            let frame = unsafe { PhysFrame::<Size2MiB>::from_start_address_unchecked(phys_addr) };
            unsafe { self.page_map(addr, frame, page_flags).unwrap().flush() };
            phys_addr += HUGE_PAGE_SIZE as u64;
            addr += HUGE_PAGE_SIZE as u64;
            size -= HUGE_PAGE_SIZE;
        }
        while 0 < size {
            let frame = unsafe { PhysFrame::<Size4KiB>::from_start_address_unchecked(phys_addr) };
            unsafe { self.page_map(addr, frame, page_flags).unwrap().flush() };
            phys_addr += PAGE_SIZE as u64;
            addr += PAGE_SIZE as u64;
            size -= PAGE_SIZE;
        }

        Some(return_addr)
    }

    pub fn alloc(&mut self, kernel: bool, size: usize, align_order: u8) -> Option<VirtAddr> {
        let SizeAddr { addr, mut size } = match kernel {
            true => self.kernel_alloc.alloc(size, align_order)?,
            false => self.user_alloc.alloc(size, align_order)?,
        };
        let return_addr = VirtAddr::new(addr as _);
        let mut addr = return_addr;
        log::info!(
            "VMM_BEGIN_ALLOC: addr={return_addr:?} layout={:?} kernel={kernel}",
            core::alloc::Layout::from_size_align(size, 1 << align_order),
        );

        let mut page_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        page_flags.set(PageTableFlags::USER_ACCESSIBLE, !kernel);
        while 0 < size && !addr.is_aligned(HUGE_PAGE_SIZE as u64) {
            let frame: PhysFrame<Size4KiB> = self.frame_allocator.allocate_frame()?;
            unsafe { self.page_map(addr, frame, page_flags).unwrap().flush() };
            addr += PAGE_SIZE as u64;
            size -= PAGE_SIZE;
        }
        while HUGE_PAGE_SIZE <= size {
            let frame: PhysFrame<Size2MiB> = self.frame_allocator.allocate_frame()?;
            unsafe { self.page_map(addr, frame, page_flags).unwrap().flush() };
            addr += HUGE_PAGE_SIZE as u64;
            size -= HUGE_PAGE_SIZE;
        }
        while 0 < size {
            let frame: PhysFrame<Size4KiB> = self.frame_allocator.allocate_frame()?;
            unsafe { self.page_map(addr, frame, page_flags).unwrap().flush() };
            addr += PAGE_SIZE as u64;
            size -= PAGE_SIZE;
        }

        log::info!(
            "VMM_END_ALLOC: addr={return_addr:?} layout={:?} kernel={kernel}",
            core::alloc::Layout::from_size_align(size, 1 << align_order),
        );

        Some(return_addr)
    }

    pub unsafe fn free(&mut self, mut addr: VirtAddr, mut size: usize) {
        let kernel = self.kernel_start <= addr;

        if !kernel && self.kernel_start < addr + size as u64 {
            unsafe {
                self.free(
                    self.kernel_start,
                    (addr + size as u64 - self.kernel_start) as _,
                );
            }
            size = (self.kernel_start - addr) as _;
        }

        match kernel {
            true => self.kernel_alloc.free(addr.as_u64() as _, size),
            false => self.user_alloc.free(addr.as_u64() as _, size),
        }

        while 0 < size && !addr.is_aligned(HUGE_PAGE_SIZE as u64) {
            self.page_table
                .unmap(Page::<Size4KiB>::from_start_address(addr).unwrap())
                .unwrap()
                .1
                .flush();
            addr += PAGE_SIZE as u64;
            size -= PAGE_SIZE;
        }
        while HUGE_PAGE_SIZE <= size {
            self.page_table
                .unmap(Page::<Size2MiB>::from_start_address(addr).unwrap())
                .unwrap()
                .1
                .flush();
            addr += HUGE_PAGE_SIZE as u64;
            size -= HUGE_PAGE_SIZE;
        }
        while 0 < size {
            self.page_table
                .unmap(Page::<Size4KiB>::from_start_address(addr).unwrap())
                .unwrap()
                .1
                .flush();
            addr += PAGE_SIZE as u64;
            size -= PAGE_SIZE;
        }
    }
}

pub static VMM: spin::Once<spin::Mutex<VirtualMemoryManager<'static>>> = spin::Once::new();

pub fn init(
    mut page_table: OffsetPageTable<'static>,
    kernel_start: VirtAddr,
    memory_regions: &[MemoryRegion],
    memory_size: u64,
) {
    fn free_page_table(
        alloc: &mut TreeBestFitAlloc,
        phys_offset: VirtAddr,
        addr: VirtAddr,
        table: &PageTable,
        level: PageTableLevel,
    ) {
        let lvl_alignment = level.entry_address_space_alignment();
        // if PageTableLevel::One < level {
        //     log::info!(
        //         "Hello there: level={level:?} addr={addr:?} lvl_alignment=0x{lvl_alignment:x}"
        //     );
        // }
        let mut run_start = None;
        for (i, entry) in table.iter().enumerate() {
            let addr = addr + i as u64 * lvl_alignment;
            if entry.is_unused() {
                run_start.get_or_insert(addr);
                continue;
            }
            if let Some(start) = run_start.take() {
                let size = (addr - start) as _;
                // log::info!(
                //     "Let's free this: addr={start:?} size=0x{size:x} \
                //      lvl_alignment=0x{lvl_alignment:x}"
                // );
                alloc.free(start.as_u64() as _, size);
            }
            if entry.flags().contains(PageTableFlags::HUGE_PAGE)
                || !entry.flags().contains(PageTableFlags::PRESENT)
            {
                continue;
            }
            if let Some(level) = level.next_lower_level() {
                let table = unsafe { &*(phys_offset + entry.addr().as_u64()).as_ptr() };
                free_page_table(alloc, phys_offset, addr, table, level);
            }
        }
        if let Some(start) = run_start {
            let size = (level.table_address_space_alignment() - (start - addr)) as _;
            // log::info!(
            //     "Let's free this: addr={start:?} size=0x{size:x} \
            //      lvl_alignment=0x{lvl_alignment:x}"
            // );
            alloc.free(start.as_u64() as _, size);
        }
    }

    VMM.call_once(move || {
        let mut frame_allocator = unsafe { pmm::init(&page_table, memory_regions, memory_size) };

        let phys_offset = page_table.phys_offset();

        const LVL4_ENTRY_ALIGN: usize = PageTableLevel::Four.entry_address_space_alignment() as _;
        const LVL3_ENTRY_ALIGN: usize = PageTableLevel::Three.entry_address_space_alignment() as _;
        const LVL2_ENTRY_ALIGN: usize = PageTableLevel::Two.entry_address_space_alignment() as _;

        assert!(kernel_start.as_u64() as usize % LVL4_ENTRY_ALIGN == 0);
        let pml4_kernel_start = kernel_start.as_u64() as usize / LVL4_ENTRY_ALIGN % 512;

        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        let page_ord = PAGE_SIZE.trailing_zeros() as _;
        let huge_page_ord = HUGE_PAGE_SIZE.trailing_zeros() as _;
        'tag: for i in pml4_kernel_start..512 {
            let entry = &mut page_table.level_4_table_mut()[i];

            if entry.is_unused() {
                entry.set_addr(frame_allocator.alloc(page_ord).unwrap(), flags);
            }

            let table: &mut PageTable =
                unsafe { &mut *(phys_offset + entry.addr().as_u64()).as_mut_ptr() };
            for (j, entry) in table.iter_mut().enumerate() {
                if entry.is_unused() {
                    entry.set_addr(frame_allocator.alloc(page_ord).unwrap(), flags);
                }

                let table: &mut PageTable =
                    unsafe { &mut *(phys_offset + entry.addr().as_u64()).as_mut_ptr() };

                for k in (0..512).step_by(2) {
                    if table[k].is_unused() && table[k + 1].is_unused() {
                        let flags = flags | PageTableFlags::HUGE_PAGE;
                        table[k].set_addr(frame_allocator.alloc(huge_page_ord).unwrap(), flags);
                        table[k + 1].set_addr(frame_allocator.alloc(huge_page_ord).unwrap(), flags);
                        let addr = x86_64::VirtAddr::new_truncate(
                            (i * LVL4_ENTRY_ALIGN + j * LVL3_ENTRY_ALIGN + k * LVL2_ENTRY_ALIGN)
                                as _,
                        );
                        x86_64::instructions::tlb::flush(addr);
                        x86_64::instructions::tlb::flush(addr + LVL2_ENTRY_ALIGN as u64);
                        // log::info!(
                        //     "ALLOC FREE SEG: {addr:?}:{i},{j},{k} pml4_start={pml4_kernel_start}",
                        // );
                        unsafe { ALLOC.free_segments.push_bytes(addr.as_mut_ptr()) };
                        if 4 <= ALLOC.free_segments.len() {
                            break 'tag;
                        }
                    }
                }
            }
        }

        // log::info!(
        //     "ALLOCATED A FEW SEGMENTS: pml4_kernel_start={pml4_kernel_start} free_segments={:?}",
        //     ALLOC.free_segments
        // );

        let mut vmm = VirtualMemoryManager::new(kernel_start, page_table, frame_allocator);

        for (i, entry) in vmm.page_table.level_4_table().iter().enumerate() {
            let alloc = match i < pml4_kernel_start {
                true => &mut vmm.user_alloc,
                false => {
                    // log::info!("Let's go kernel");
                    &mut vmm.kernel_alloc
                }
            };
            let addr = VirtAddr::new_truncate((i * LVL4_ENTRY_ALIGN) as _);
            if entry.is_unused() {
                alloc.free(addr.as_u64() as _, LVL4_ENTRY_ALIGN as _);
            } else {
                let table = unsafe { &*(phys_offset + entry.addr().as_u64()).as_ptr() };
                free_page_table(alloc, phys_offset, addr, table, PageTableLevel::Three);
            }
        }

        // log::info!("VMM INITIALIZED: pml4_kernel_start={pml4_kernel_start}");

        spin::Mutex::new(vmm)
    });
    ALLOC.vmm.call_once(|| VMM.get().unwrap());
}
