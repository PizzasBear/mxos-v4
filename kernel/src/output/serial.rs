//! This module contains everithing related to the 16550 UART serial port logging.

use core::fmt;

use spin::{Lazy, Mutex};
use uart_16550::SerialPort;
use x86_64::instructions::interrupts::without_interrupts;

/// The serial port.
pub static SERIAL1: Lazy<Mutex<SerialPort>> = Lazy::new(|| {
    let mut serial_port = unsafe { SerialPort::new(0x3f8) };
    serial_port.init();
    Mutex::new(serial_port)
});

/// Prints to the serial port. Don't use directly, use `sprint!()` instead.
#[doc(hidden)]
pub fn _sprint(args: core::fmt::Arguments) {
    without_interrupts(|| {
        // SERIAL1.lock().write_fmt(args).expect("Printing to serial failed");
        fmt::write(&mut *SERIAL1.lock(), args).expect("Printing to serial failed");
    })
}
/// Prints to the serial port. Don't use directly, use `sprintln!()` instead.
#[doc(hidden)]
pub fn _sprintln(args: core::fmt::Arguments) {
    without_interrupts(|| {
        let serial1 = &mut *SERIAL1.lock();
        fmt::write(serial1, args).expect("Printing to serial failed");
        serial1.send(b'\n');
    })
}

/// Print to serial port.
#[macro_export]
macro_rules! sprint {
    ($($arg:tt)*) => {{
        $crate::serial::_sprint(format_args!($($arg)*));
    }};
}

/// Print to serial port with newline.
#[macro_export]
macro_rules! sprintln {
    () => {{
        $crate::output::serial::_sprintln(format_args!(""));
    }};
    ($($arg:tt)+) => {{
        $crate::output::serial::_sprintln(format_args!($($arg)*));
    }};
}
