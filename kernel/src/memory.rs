use x86_64::{registers::control::Cr3, structures::paging::OffsetPageTable, VirtAddr};

pub unsafe fn offset_page_table(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let (lvl4_table, _) = Cr3::read();
    let lvl4_table_addr = physical_memory_offset + lvl4_table.start_address().as_u64();
    unsafe { OffsetPageTable::new(&mut *lvl4_table_addr.as_mut_ptr(), physical_memory_offset) }
}

// pub unsafe fn translate_addr(addr: VirtAddr, physical_memory_offset: VirtAddr) -> Option<PhysAddr> {
//     let (mut frame, _) = Cr3::read();
//
//     let table_indexes = [
//         (addr.p4_index(), (1 << 12 + 3 * 9) - 1),
//         (addr.p3_index(), (1 << 12 + 2 * 9) - 1),
//         (addr.p2_index(), (1 << 12 + 1 * 9) - 1),
//         (addr.p1_index(), (1 << 12 + 0 * 9) - 1),
//     ];
//     for (index, huge_mask) in table_indexes {
//         let table_frame_addr = physical_memory_offset + frame.start_address().as_u64();
//         let table: &PageTable = unsafe { &*table_frame_addr.as_ptr() };
//
//         let entry = &table[index];
//         frame = match entry.frame() {
//             Ok(frame) => frame,
//             Err(FrameError::FrameNotPresent) => return None,
//             Err(FrameError::HugeFrame) => {
//                 return Some(frame.start_address() + (addr.as_u64() & huge_mask))
//             }
//         };
//     }
//
//     Some(frame.start_address() + u64::from(addr.page_offset()))
// }
