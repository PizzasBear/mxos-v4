use core::fmt;

use bootloader_api::info::FrameBuffer;
use hashbrown::HashMap;

use crate::psf::{self, PsfFile};

pub static CONSOLE: spin::Mutex<Option<ConsoleGraphics>> = spin::Mutex::new(None);

pub fn init(font: &'static PsfFile, framebuffer: &'static mut FrameBuffer) {
    (CONSOLE.lock())
        .insert(ConsoleGraphics::new(font, framebuffer))
        .clear();
}

pub fn deinit() -> Option<&'static mut FrameBuffer> {
    Some((CONSOLE.lock()).take()?.framebuffer)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
struct Point {
    x: usize,
    y: usize,
}

impl Point {
    fn new(x: usize, y: usize) -> Self {
        Self { x, y }
    }
}

pub struct ConsoleGraphics<'a> {
    font: &'a PsfFile<'a>,
    framebuffer: &'a mut FrameBuffer,
    table: HashMap<char, u32>,
    cursor: Point,
}

impl<'a> ConsoleGraphics<'a> {
    fn new(font: &'a PsfFile<'a>, framebuffer: &'a mut FrameBuffer) -> Self {
        let mut table = HashMap::new();
        for entry in font.unicode_table_entries() {
            match entry.value {
                psf::UnicodeTableEntryValue::Utf8(s) => {
                    for ch in s.chars() {
                        table.insert(ch, entry.index);
                    }
                }
                psf::UnicodeTableEntryValue::Ucs2(s) => {
                    for ch in s.chars() {
                        table.insert(ch, entry.index);
                    }
                }
            }
        }
        Self {
            font,
            framebuffer,
            table,
            cursor: Point::new(0, 0),
        }
    }

    pub fn clear(&mut self) {
        let buf = self.framebuffer.buffer_mut();
        buf.fill(0);
        self.cursor = Point::new(0, 0);
    }

    pub fn move_right(&mut self, n: usize) {
        self.cursor.x += n * self.font.glyph_width() as usize;
        if self.framebuffer.info().width <= self.cursor.x + self.font.glyph_width() as usize {
            self.cursor.x = 0;
            self.move_down();
        }
    }

    pub fn move_down(&mut self) {
        self.cursor.y += self.font.glyph_height() as usize;
        if self.framebuffer.info().height <= self.cursor.y + self.font.glyph_height() as usize {
            self.scrollup(1);
        }
    }

    pub fn scrollup(&mut self, lines: usize) {
        let info = self.framebuffer.info();
        let buf = self.framebuffer.buffer_mut();
        let buf_len = buf.len();
        let y_offset = info.height.min(self.font.glyph_height() as usize * lines);
        let offset = info.bytes_per_pixel * info.stride * y_offset;

        buf.copy_within(offset.., 0);
        buf[buf_len - offset..].fill(0);
        self.cursor.y = self.cursor.y.saturating_sub(y_offset);
    }

    pub fn putchar(&mut self, ch: char) -> bool {
        let mut status = true;
        if ch == '\r' {
            self.cursor.x = 0;
            return status;
        } else if ch == '\n' {
            self.cursor.x = 0;
            self.move_down();
            return status;
        }

        let Some(&glyph_id) = self.table.get(&ch).or_else(|| {
            status = false;
            self.table.get(&'\u{FFFD}')
        }) else {
            return false;
        };
        let glyph = self.font.get_glyph(glyph_id).unwrap();

        let info = self.framebuffer.info();
        let buf = self.framebuffer.buffer_mut();

        for (y, row) in (self.cursor.y..).zip(glyph.rows()) {
            for (x, pixel) in (self.cursor.x..).zip(row) {
                let idx = info.bytes_per_pixel * (info.stride * y + x);
                let pixel_buf = &mut buf[idx..idx + info.bytes_per_pixel];
                match pixel {
                    true => pixel_buf.fill(255),
                    false => pixel_buf.fill(0),
                }
            }
        }

        self.move_right(1);

        status
    }
}

impl fmt::Write for ConsoleGraphics<'_> {
    fn write_char(&mut self, ch: char) -> fmt::Result {
        self.putchar(ch).then_some(()).ok_or(fmt::Error)
    }
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for ch in s.chars() {
            self.write_char(ch)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum Error {
    Uninitialized,
    FormatError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uninitialized => write!(f, "Console is uninitialized"),
            Self::FormatError => write!(f, "Formatting error"),
        }
    }
}

impl From<fmt::Error> for Error {
    fn from(_: fmt::Error) -> Self {
        Self::FormatError
    }
}

/// Prints to the serial port. Don't use directly, use `sprint!()` instead.
#[doc(hidden)]
pub fn _cprint(args: core::fmt::Arguments) -> Result<(), Error> {
    fmt::write(CONSOLE.lock().as_mut().ok_or(Error::Uninitialized)?, args)?;
    Ok(())
}
/// Prints to the serial port. Don't use directly, use `sprintln!()` instead.
#[doc(hidden)]
pub fn _cprintln(args: core::fmt::Arguments) -> Result<(), Error> {
    let mut binding = CONSOLE.lock();
    let console = binding.as_mut().ok_or(Error::Uninitialized)?;
    fmt::write(console, args)?;
    console.putchar('\n');
    Ok(())
}

/// Print to console.
#[macro_export]
macro_rules! cprint {
    ($($arg:tt)*) => {{
        $crate::console::_cprint(format_args!($($arg)*)).expect("Printing to console failed");
    }};
}

/// Print to console with newline.
#[macro_export]
macro_rules! cprintln {
    () => {{
        $crate::console::_cprintln(format_args!("")).expect("Printing to console failed");
    }};
    ($($arg:tt)+) => {{
        $crate::console::_cprintln(format_args!($($arg)*)).expect("Printing to console failed");
    }};
}
