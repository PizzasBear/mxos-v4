#![no_std]
#![no_main]

// extern crate alloc;
use core::{
    fmt::{self, Write},
    panic::PanicInfo,
};

use rand::prelude::*;
use uefi::{
    data_types::EqStrUntilNul,
    prelude::*,
    proto::{
        console::{gop::GraphicsOutput, text::Output},
        media::fs::SimpleFileSystem,
        rng::Rng as RngProto,
    },
};

pub mod align;
pub mod serial;

type Result<T, E = Error> = core::result::Result<T, E>;

#[derive(Debug)]
enum Error {
    Uefi(uefi::Status),
    Fmt(fmt::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Uefi(err) => fmt::Display::fmt(err, f),
            Error::Fmt(err) => fmt::Display::fmt(err, f),
        }
    }
}

impl<T: fmt::Debug> From<uefi::Error<T>> for Error {
    fn from(err: uefi::Error<T>) -> Self {
        Self::Uefi(err.status())
    }
}

impl From<fmt::Error> for Error {
    fn from(err: fmt::Error) -> Self {
        Self::Fmt(err)
    }
}

fn halt() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

fn init_rng(bt: &BootServices) -> uefi::Result<StdRng> {
    let rng_handle = bt.get_handle_for_protocol::<RngProto>()?;
    let mut rng_proto = bt.open_protocol_exclusive::<RngProto>(rng_handle)?;

    let mut seed = <StdRng as rand::SeedableRng>::Seed::default();
    rng_proto.get_rng(None, &mut seed)?;
    Ok(StdRng::from_seed(seed))
}

fn main(_image: Handle, st: SystemTable<Boot>) -> Result<()> {
    serial::init_logger();

    let bt = st.boot_services();

    let cout_handle = bt.get_handle_for_protocol::<Output>()?;
    let mut cout = bt.open_protocol_exclusive::<Output>(cout_handle)?;
    cout.clear()?;

    sprintln!("Hello, world!");
    writeln!(cout, "Hello, world!")?;

    let fs_handle = bt.get_handle_for_protocol::<SimpleFileSystem>()?;
    let mut fs = bt.open_protocol_exclusive::<SimpleFileSystem>(fs_handle)?;

    let mut dir = fs.open_volume()?;
    loop {
        let mut buf = align::Align8([0; 80 + 2 * 256]);
        let Some(file) = dir.read_entry(&mut *buf)? else {
            break;
        };
    }

    if (|| true)() {
        halt();
    }

    let gop_handle = bt.get_handle_for_protocol::<GraphicsOutput>()?;
    let mut gop = bt.open_protocol_exclusive::<GraphicsOutput>(gop_handle)?;

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let stride = mode.stride();

    // match mode.pixel_format() {
    //     uefi::proto::console::gop::PixelFormat::Rgb => todo!(),
    //     uefi::proto::console::gop::PixelFormat::Bgr => todo!(),
    //     uefi::proto::console::gop::PixelFormat::Bitmask => {
    //         let bitmask = mode.pixel_bitmask().unwrap();
    //     }
    //     uefi::proto::console::gop::PixelFormat::BltOnly => unimplemented!(),
    // }

    let mut framebuffer = gop.frame_buffer();
    for offset in (0..4 * stride * height).step_by(4 * stride) {
        for i in (offset..offset + 4 * width).step_by(4) {
            unsafe {
                framebuffer.write_value(i, (!0u32).to_ne_bytes());
            }
        }
    }

    Ok(())
}

#[entry]
fn efi_start(image: Handle, st: SystemTable<Boot>) -> Status {
    main(image, st).unwrap();
    halt();
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // unsafe {
    //     serial::SERIAL_LOGGER.force_unlock();
    //     sprintln!();
    // }

    // log::error!("Kernel panic: `{}`", info);

    // log::error!("PANIC: {}", info);
    halt();
}
