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

use alloc::{boxed::Box, vec::Vec};
use bootloader_api::{entry_point, info::MemoryRegionKind, BootInfo, BootloaderConfig};

use hashbrown::HashMap;
use psf::PsfFile;
// use psf::PsfFile;
use x86_64::VirtAddr;

const KENREL_START: u64 = 0xFFFF_8000_0000_0000;

pub const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.dynamic_range_start = Some(KENREL_START);
    config.mappings.physical_memory = Some(bootloader_api::config::Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

const PSF_FONT: spin::Lazy<PsfFile<'static>> =
    spin::Lazy::new(|| PsfFile::parse(include_bytes!("../LatKaCyrHeb-14.psfu")).unwrap());

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init_logger();

    boot_info.memory_regions.sort_unstable_by_key(|r| r.start);
    let memory_size = (boot_info.memory_regions.iter())
        .filter_map(|r| (r.kind == MemoryRegionKind::Usable).then_some(r.end))
        .last()
        .unwrap();
    let phys_offset = VirtAddr::new(boot_info.physical_memory_offset.into_option().unwrap());
    let mapper = unsafe { memory::offset_page_table(phys_offset) };

    // unsafe { malloc::ALLOC.0.free_segments.push_bytes(ptr) };
    log::info!("boot_info={boot_info:#?}");

    // let idx = fb_info.bytes_per_pixel * (fb_info.stride * y as usize + x as usize);
    // framebuffer.buffer_mut()[idx..idx + fb_info.bytes_per_pixel].fill((255. * c) as u8)

    log::info!("STACK_PTR={:?}", &() as *const _);
    vmm::init(
        mapper,
        VirtAddr::new(KENREL_START),
        &*boot_info.memory_regions,
        memory_size,
    );

    log::info!("DONE");

    let x = Box::new(5);
    log::info!("ALLOC WORKS: {}", x);
    drop(x);
    log::info!("DEALLOC WORKS");

    let v = (0..2048).collect::<Vec<u64>>();
    log::info!("Large WORKS: {}", v.len());
    drop(v);
    log::info!("Large DEALLOC WORKS");

    let v = (0..1 << 17).collect::<Vec<u64>>();
    log::info!("HUGE WORKS: {}", v.len());
    drop(v);
    log::info!("HUGE DEALLOC WORKS");

    let mut unicode_table = HashMap::new();
    for entry in PSF_FONT.unicode_table_entries() {
        log::info!("FONT ENTRY: {entry:?}");
        match entry.value {
            psf::UnicodeTableEntryValue::Utf8(s) => {
                for ch in s.chars() {
                    unicode_table.insert(ch, entry.index);
                }
            }
            psf::UnicodeTableEntryValue::Ucs2(s) => {
                for ch in s.chars() {
                    unicode_table.insert(ch, entry.index);
                }
            }
        }
    }

    let _framebuffer = boot_info.framebuffer.take().unwrap();
    sprintln!();
    let glyph_rows = |ch| {
        PSF_FONT
            .get_glyph(*unicode_table.get(&ch).unwrap())
            .unwrap()
            .rows()
    };
    for (_y, (row0, row1, row2, row3, row4)) in itertools::izip!(
        glyph_rows('I'),
        glyph_rows(' '),
        glyph_rows('w'),
        glyph_rows('o'),
        glyph_rows('n'),
    )
    .enumerate()
    {
        for (_x, pixel) in row0
            .chain(row1)
            .chain(row2)
            .chain(row3)
            .chain(row4)
            .enumerate()
        {
            match pixel {
                true => sprint!("[]"),
                false => sprint!("  "),
            }
        }
        sprintln!();
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
