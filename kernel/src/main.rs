#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

pub mod bitmap;
pub mod memory;
pub mod output;
pub mod psf;

use bootloader_api::{entry_point, BootInfo, BootloaderConfig};

use psf::PsfFile;

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
    output::init_logger();

    log::info!("boot_info={boot_info:#?}");

    memory::init(boot_info);

    let framebuffer = boot_info.framebuffer.as_mut().unwrap();

    output::console::init(&*PSF_FONT, framebuffer);

    log::info!("Halting");

    loop {
        x86_64::instructions::hlt();
    }
}

#[cfg_attr(not(test), panic_handler)]
fn panic_handler(info: &core::panic::PanicInfo) -> ! {
    #[allow(dead_code)]
    const _: &dyn core::any::Any = &panic_handler;

    unsafe {
        output::LOGGER.force_unlock();
    }
    output::console::CONSOLE.lock().take();

    sprintln!();
    sprintln!("{info}");

    loop {
        x86_64::instructions::interrupts::disable();
        x86_64::instructions::hlt();
    }
}
