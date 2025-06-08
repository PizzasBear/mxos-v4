#![no_std] // don't link the Rust standard library
#![no_main] // disable all Rust-level entry points
#![feature(abi_x86_interrupt)]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

pub mod bitmap;
pub mod gdt;
pub mod interrupts;
pub mod memory;
pub mod output;
pub mod psf;

use bootloader_api::{BootInfo, BootloaderConfig, entry_point};

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

    gdt::init();
    interrupts::init_idt();

    memory::init(boot_info);

    if let Some(framebuffer) = boot_info.framebuffer.take() {
        output::console::init(&PSF_FONT, framebuffer);
    }
    log::info!("BOOT_INFO: {boot_info:#?}");

    // log::info!(
    //     "MEMORY_REGIONS: [{}\n]",
    //     boot_info.memory_regions.iter().format_with(",", |r, f| {
    //         f(&format_args!(
    //             "\n\tMemoryRegion {{ range: 0x{:X}..0x{:X}, kind: {:?} }}",
    //             r.start, r.end, r.kind
    //         ))
    //     })
    // );

    x86_64::instructions::interrupts::int3(); // test interrupts

    unsafe { interrupts::init_apic() };

    let rsdp = boot_info.rsdp_addr.into_option().expect("No RSDP address");

    log::info!("rsdp addr=0x{rsdp:x}");

    let acpi_tables = unsafe {
        acpi::AcpiTables::from_rsdp(memory::vmm::GlobalVmmApicHandler, rsdp as _).unwrap()
    };

    let acpi_platform_info = acpi_tables.platform_info().unwrap();

    let acpi::InterruptModel::Apic(acpi_apic) = acpi_platform_info.interrupt_model else {
        panic!(
            "Unknown interrupt model: {:?}",
            acpi_platform_info.interrupt_model
        );
    };

    log::info!("acpi_apic = {acpi_apic:#?}");

    let pci_config_regions = acpi::PciConfigRegions::new(&acpi_tables).unwrap();

    log::info!("pci region:");
    for region in pci_config_regions.iter() {
        log::info!(
            "  {{\n    segment_group={},\n    bus_range={:?},\n    phys_addr=0x{:X}\n}}",
            region.segment_group,
            region.bus_range,
            region.physical_address,
        );
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
        output::force_unlock();
    }

    println!();
    println!("{info}");

    loop {
        x86_64::instructions::interrupts::disable();
        x86_64::instructions::hlt();
    }
}
