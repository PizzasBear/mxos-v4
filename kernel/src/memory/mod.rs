use bootloader_api::info::{BootInfo, MemoryRegionKind};
use x86_64::{registers::control::Cr3, structures::paging::OffsetPageTable, VirtAddr};

pub mod malloc;
pub mod pmm;
pub mod vmm;

unsafe fn offset_page_table(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let (lvl4_table, _) = Cr3::read();
    let lvl4_table_addr = physical_memory_offset + lvl4_table.start_address().as_u64();
    unsafe { OffsetPageTable::new(&mut *lvl4_table_addr.as_mut_ptr(), physical_memory_offset) }
}

pub fn init(boot_info: &mut BootInfo) {
    boot_info.memory_regions.sort_unstable_by_key(|r| r.start);
    let memory_size = (boot_info.memory_regions.iter())
        .filter_map(|r| (r.kind == MemoryRegionKind::Usable).then_some(r.end))
        .last()
        .unwrap();
    let phys_offset = VirtAddr::new(boot_info.physical_memory_offset.into_option().unwrap());
    let mapper = unsafe { offset_page_table(phys_offset) };

    vmm::init(
        mapper,
        VirtAddr::new(crate::KENREL_START),
        &*boot_info.memory_regions,
        memory_size,
    );
}
