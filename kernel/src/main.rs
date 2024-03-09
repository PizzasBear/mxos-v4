#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

pub mod bitmap;
pub mod malloc;
pub mod memory;
pub mod pmm;
pub mod psf;
pub mod serial;
pub mod vmm;

use core::{mem, slice};

use alloc::boxed::Box;
use bootloader_api::{entry_point, info::MemoryRegionKind, BootInfo, BootloaderConfig};

// use psf::PsfFile;
use x86_64::{PhysAddr, VirtAddr};

use crate::pmm::BuddyAllocator;

const KENREL_START: u64 = 0xFFFF_8000_0000_0000;

pub const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.dynamic_range_start = Some(KENREL_START);
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

// const PSF_FONT: Lazy<PsfFile> =
//     Lazy::new(|| PsfFile::parse(include_bytes!("../LatKaCyrHeb-14.psfu")).unwrap());

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init_logger();

    boot_info.memory_regions.sort_unstable_by_key(|r| r.start);
    let phys_offset = VirtAddr::new(boot_info.physical_memory_offset.into_option().unwrap());
    let memory_size = boot_info
        .memory_regions
        .iter()
        .filter(|r| r.kind == MemoryRegionKind::Usable)
        .last()
        .unwrap()
        .end;
    let mapper = unsafe { memory::offset_page_table(phys_offset) };

    let buddy_map_len = BuddyAllocator::buddy_map_len(memory_size as _);

    let mut start = 0;
    let mut end = 0;

    let mut phys_alloc = None;
    let mut buddy_map_start = 0;
    for r in &*boot_info.memory_regions {
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
    let mut phys_alloc = phys_alloc.unwrap();

    let mut start = 0;
    let mut end = 0;
    for r in &*boot_info.memory_regions {
        if r.kind != MemoryRegionKind::Usable || r.start < 0x100000 {
            continue;
        }
        if end < r.start {
            if start == buddy_map_start {
                start += (mem::size_of::<usize>() * buddy_map_len) as u64 + 4095;
                start &= !4095;
            }
            // blue waffle
            phys_alloc.free_region(PhysAddr::new(start)..PhysAddr::new(end));

            start = r.start + 4095 & !4095;
        }
        end = r.end;
    }

    // unsafe { malloc::ALLOC.0.free_segments.push_bytes(ptr) };
    log::info!("boot_info={boot_info:#?}");
    log::info!("VARS: memory_size={memory_size} buddy_map_size={buddy_map_len}");

    // let idx = fb_info.bytes_per_pixel * (fb_info.stride * y as usize + x as usize);
    // framebuffer.buffer_mut()[idx..idx + fb_info.bytes_per_pixel].fill((255. * c) as u8)

    log::info!("STACK_PTR={:?}", &() as *const _);
    vmm::init(VirtAddr::new(KENREL_START), mapper, phys_alloc);

    log::info!("DONE");

    log::info!("ALLOC WORKS: {}", Box::new(5));

    loop {
        x86_64::instructions::hlt();
    }
}

#[cfg_attr(not(test), panic_handler)]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    #[allow(dead_code)]
    const _: &dyn core::any::Any = &panic_handler;

    unsafe {
        serial::SERIAL_LOGGER.force_unlock();
    }

    sprintln!("");
    sprintln!("{info}");

    loop {
        x86_64::instructions::interrupts::disable();
        x86_64::instructions::hlt();
    }
}
