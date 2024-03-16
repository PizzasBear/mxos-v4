use core::fmt::{self, Write};

pub mod console;
pub mod serial;

use console::CONSOLE;
use serial::SERIAL1;
use x86_64::instructions::interrupts::without_interrupts;

#[doc(hidden)]
pub struct _MultiWriter;

impl Write for _MultiWriter {
    fn write_char(&mut self, c: char) -> fmt::Result {
        without_interrupts(|| {
            SERIAL1.lock().write_char(c)?;
            if let Some(console) = CONSOLE.lock().as_mut() {
                console.write_char(c)?;
            }
            Ok(())
        })
    }
    fn write_str(&mut self, s: &str) -> fmt::Result {
        without_interrupts(|| {
            SERIAL1.lock().write_str(s)?;
            if let Some(console) = CONSOLE.lock().as_mut() {
                console.write_str(s)?;
            }
            Ok(())
        })
    }
}

/// Print to both the serial port and the console.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {{
        ::core::fmt::write(&mut $crate::output::_MultiWriter, format_args!($($arg)*))
            .expect("Printing failed");
    }};
}

/// Print to both the serial port and the console with newline.
#[macro_export]
macro_rules! println {
    () => {{
        $crate::print!("\n");
    }};
    ($($arg:tt)+) => {{
        $crate::print!("{}\n", format_args!($($arg)+));
    }};
}

/// `SerialLogger` implements `log::Log`, it logs to the serial port with the format: `"LEVEL: MSG"`
pub struct Logger {
    _private: (),
}

pub static LOGGER: Logger = Logger { _private: () };

/// Force unlock the serial port and the console.
pub unsafe fn force_unlock() {
    unsafe { serial::SERIAL1.force_unlock() };
    unsafe { console::CONSOLE.force_unlock() };
}

impl log::Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            println!("{}: {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

/// The function initiates the serial port and the serial logger, `SERIAL_LOGGER`,
/// and `init_logger` sets the default logger to serial.
pub fn init_logger() {
    log::set_logger(&LOGGER).expect("Failed to set logger");
    log::set_max_level(log::LevelFilter::Info);
}
