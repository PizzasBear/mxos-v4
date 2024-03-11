pub mod console;
pub mod serial;

/// Print to both the serial port and the console.
#[macro_export]
macro_rules! print {
    () => {{
        $crate::serial::_sprint(format_args!($($arg)*));
        _ = $crate::console::_cprint(format_args!($($arg)*));
    }};
}

/// Print to both the serial port and the console with newline.
macro_rules! println {
    () => {{
        $crate::output::serial::_sprintln(format_args!(""));
        _ = $crate::output::console::_cprintln(format_args!(""));
    }};
    ($($arg:tt)+) => {{
        $crate::output::serial::_sprintln(format_args!($($arg)*));
        _ = $crate::output::console::_cprintln(format_args!($($arg)*));
    }};
}

/// `SerialLogger` implements `log::Log`, it logs to the serial port with the format: `"LEVEL: MSG"`
pub struct Logger {
    _private: (),
}

pub static LOGGER: Logger = Logger { _private: () };

impl Logger {
    /// Forces the unlock the spinlock on the logger.
    pub unsafe fn force_unlock(&self) {
        unsafe { serial::SERIAL1.force_unlock() };
        unsafe { console::CONSOLE.force_unlock() };
    }
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
