#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

pub mod bitmap;
pub mod console;
pub mod malloc;
pub mod memory;
pub mod pmm;
pub mod psf;
pub mod serial;
pub mod vmm;

use bootloader_api::{entry_point, info::MemoryRegionKind, BootInfo, BootloaderConfig};

use psf::PsfFile;
use x86_64::VirtAddr;

const KENREL_START: u64 = 0xFFFF_8000_0000_0000;

pub const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.dynamic_range_start = Some(KENREL_START);
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

static PSF_FONT: spin::Lazy<PsfFile<'static>> =
    spin::Lazy::new(|| PsfFile::parse(include_bytes!("../LatKaCyrHeb-14.psfu")).unwrap());

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init_logger();

    log::info!("boot_info={boot_info:#?}");

    boot_info.memory_regions.sort_unstable_by_key(|r| r.start);
    let memory_size = (boot_info.memory_regions.iter())
        .filter_map(|r| (r.kind == MemoryRegionKind::Usable).then_some(r.end))
        .last()
        .unwrap();
    let phys_offset = VirtAddr::new(boot_info.physical_memory_offset.into_option().unwrap());
    let mapper = unsafe { memory::offset_page_table(phys_offset) };

    vmm::init(
        mapper,
        VirtAddr::new(KENREL_START),
        &*boot_info.memory_regions,
        memory_size,
    );

    let framebuffer = boot_info.framebuffer.as_mut().unwrap();

    console::init(&*PSF_FONT, framebuffer);

    for i in 1..=80 {
        cprintln!("Hello, World! {}", i);
    }

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

    sprintln!();
    sprintln!("{info}");

    loop {
        x86_64::instructions::interrupts::disable();
        x86_64::instructions::hlt();
    }
}
