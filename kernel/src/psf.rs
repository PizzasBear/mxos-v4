use core::{cmp::Ordering, fmt, iter::FusedIterator, str};

mod ucs2;

use ucs2::Ucs2Str;

// should be less than (255 / 4)
// const MAX_UNICODE_CHAR_NUM: usize = 4;

#[derive(Debug)]
pub enum Error {
    UnexpectedEnd,
    UnknownPsf2Version(u32),
    InvalidMagic,
    InvalidGlyphSize,
    // EmptyUnicodeTableEntry,
    // UnicodeTooLong,
    Utf8Error(str::Utf8Error),
    Ucs2Error,
    UnexpectedUnicodeTable,
    InvalidUnicodeTableSize { num_glyphs: u32, num_entries: usize },
    UnterminatedUnicodeTable,
}

type Result<T, E = Error> = core::result::Result<T, E>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => write!(f, "PSF file unexpectedly ended"),
            Self::InvalidMagic => write!(f, "Invalid magic bytes"),
            Self::UnknownPsf2Version(ver) => write!(f, "Unsupported version PSF2.{ver}"),
            Self::InvalidGlyphSize => {
                write!(f, "The provided PSF2 glyph size doesn't equal the calculated size (`height * ((width + 7) / 8)`)")
            }
            // Self::UnicodeTooLong => {
            //     write!(
            //         f,
            //         "A glyph's unicode string cannot be longer {MAX_UNICODE_CHAR_NUM} characters",
            //     )
            // }
            Self::Utf8Error(err) => {
                write!(f, "Unicode table UTF-8 string parsing errored: {err}")
            }
            Self::Ucs2Error => {
                write!(f, "Unicode table UCS-2 LE string parsing errored")
            }
            // Self::EmptyUnicodeTableEntry => {
            //     write!(f, "Unicode table entry is empty")
            // }
            Self::UnexpectedUnicodeTable => {
                write!(f, "Encountered an unexpected unicode table")
            }
            &Self::InvalidUnicodeTableSize {
                num_glyphs,
                num_entries,
            } => {
                write!(
                    f,
                    "Unicode table has {} entries than there are glyphs",
                    match (num_glyphs as usize) < num_entries {
                        true => "more",
                        false => "less",
                    },
                )
            }
            Self::UnterminatedUnicodeTable => write!(f, "Unicode table wasn't properly terminated"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PsfVersion {
    Psf1,
    Psf2,
}

#[derive(Clone, Copy)]
pub struct PsfFile<'a> {
    raw_bytes: &'a [u8],
    version: PsfVersion,
    header_size: u32,
    has_unicode_table: bool,
    glyph_size: u32,
    glyph_width: u32,
    glyph_height: u32,
    num_glyphs: u32,

    longest_glyph: u8,
}

impl fmt::Debug for PsfFile<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PsfFile")
            .field("version", &self.version)
            .field("header_size", &self.header_size)
            .field("has_unicode_table", &self.has_unicode_table)
            .field("glyph_size", &self.glyph_size)
            .field("glyph_width", &self.glyph_width)
            .field("glyph_height", &self.glyph_height)
            .field("num_glyphs", &self.num_glyphs)
            .field("longest_glyph", &self.longest_glyph)
            .finish()
    }
}

impl<'a> PsfFile<'a> {
    pub const fn num_glyphs(&self) -> u32 {
        self.num_glyphs
    }
    pub const fn glyph_width(&self) -> u32 {
        self.glyph_width
    }
    pub const fn glyph_height(&self) -> u32 {
        self.glyph_height
    }

    pub fn parse1(bytes: &'a [u8]) -> Result<Self> {
        let header_bytes = bytes.get(..4).ok_or(Error::UnexpectedEnd)?;
        let header: &[u8; 4] = header_bytes.try_into().unwrap();

        if header[0] != 0x36 || header[1] != 0x04 {
            return Err(Error::InvalidMagic);
        }

        let mut slf = Self {
            raw_bytes: bytes,
            version: PsfVersion::Psf1,
            header_size: 4,
            num_glyphs: if header[2] & 1 == 0 { 256 } else { 512 },
            has_unicode_table: header[2] & 6 != 0,
            glyph_size: header[3] as u32,
            glyph_height: header[3] as u32,
            glyph_width: 8,
            longest_glyph: 0,
        };
        slf.process_unicode_table()?;
        Ok(slf)
    }

    pub fn parse2(bytes: &'a [u8]) -> Result<Self> {
        let header_bytes = bytes.get(..32).ok_or(Error::UnexpectedEnd)?;
        let header_num = {
            let header_nums: &[u8; 32] = header_bytes.try_into().unwrap();
            |i: usize| u32::from_le_bytes(header_nums[4 * i..4 * i + 4].try_into().unwrap())
        };

        if header_num(0) != 0x864ab572 {
            return Err(Error::InvalidMagic);
        }
        if header_num(1) != 0 {
            return Err(Error::UnknownPsf2Version(header_num(1)));
        }
        let mut slf = Self {
            raw_bytes: bytes,
            version: PsfVersion::Psf2,
            header_size: header_num(2).max(32),
            has_unicode_table: header_num(3) & 1 != 0,
            num_glyphs: header_num(4),
            glyph_height: header_num(6),
            glyph_width: header_num(7),
            glyph_size: {
                let size = header_num(6) * ((header_num(7) + 7) / 8);
                if header_num(5) != size {
                    return Err(Error::InvalidGlyphSize);
                }
                size
            },
            longest_glyph: 0,
        };
        slf.process_unicode_table()?;
        Ok(slf)
    }

    pub fn parse(bytes: &'a [u8]) -> Result<Self> {
        if bytes.starts_with(&0x864ab572u32.to_le_bytes()) {
            Self::parse2(bytes)
        } else if bytes.starts_with(&0x0436u16.to_le_bytes()) {
            Self::parse1(bytes)
        } else {
            Err(Error::InvalidMagic)
        }
    }

    fn unicode_table_start(&self) -> usize {
        self.header_size as usize + self.num_glyphs as usize * self.glyph_size as usize
    }

    fn process_unicode_table(&mut self) -> Result<()> {
        log::info!("I AM {self:#?}");
        let table_start = self.unicode_table_start();
        if !self.has_unicode_table {
            return match self.raw_bytes.len().cmp(&table_start) {
                Ordering::Less => Err(Error::UnexpectedEnd),
                Ordering::Equal => Ok(()),
                Ordering::Greater => Err(Error::UnexpectedUnicodeTable),
            };
        }

        let mut num_entries = 0usize;
        let mut max = 0;
        match self.version {
            PsfVersion::Psf1 => {
                let mut len = 0;
                for ch in self.raw_bytes[table_start..].chunks(2) {
                    match u16::from_le_bytes(ch.try_into().map_err(|_| Error::Ucs2Error)?) {
                        n @ (0xFFFE | 0xFFFF) => {
                            // if len == 0 {
                            //     return Err(Error::EmptyUnicodeTableEntry);
                            // }
                            num_entries += n as usize & 1;
                            max = max.max(len);
                            len = 0;
                        }
                        ch if char::from_u32(ch as _).is_none() => return Err(Error::Ucs2Error),
                        _ => len += 1,
                    }
                }
                if !self.raw_bytes.ends_with(&[0xFF; 2]) {
                    return Err(Error::UnterminatedUnicodeTable);
                }
            }
            PsfVersion::Psf2 => {
                for s in self.raw_bytes[table_start..].split_inclusive(|&b| 0xFD < b) {
                    let Some((&sep, s)) = s.split_last() else {
                        return Err(Error::UnterminatedUnicodeTable);
                    };
                    let len = str::from_utf8(s).map_err(Error::Utf8Error)?.chars().count();
                    // if len == 0 {
                    //     return Err(Error::EmptyUnicodeTableEntry);
                    // }
                    max = max.max(len);
                    num_entries += sep as usize & 1;
                }
                if !self.raw_bytes.last().is_some_and(|&b| b == 0xFF) {
                    return Err(Error::UnterminatedUnicodeTable);
                }
            }
        }
        if num_entries != self.num_glyphs as usize {
            return Err(Error::InvalidUnicodeTableSize {
                num_glyphs: self.num_glyphs,
                num_entries,
            });
        }
        // if MAX_UNICODE_CHAR_NUM < max {
        //     return Err(Error::UnicodeTooLong);
        // }
        self.longest_glyph = max as _;

        Ok(())
    }

    pub fn unicode_table_entries(&self) -> UnicodeTableEntries<'a> {
        UnicodeTableEntries {
            version: self.version,
            raw_bytes: self.raw_bytes,
            entry_index: 0,
            index: self.unicode_table_start(),
        }
    }

    pub fn get_glyph(&self, entry: u32) -> Option<Glyph<'a>> {
        let start = (self.header_size + entry * self.glyph_size) as usize;
        let bytes = self
            .raw_bytes
            .get(start..start + self.glyph_size as usize)?;
        Some(Glyph {
            bytes,
            width: self.glyph_width,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Glyph<'a> {
    pub bytes: &'a [u8],
    width: u32,
}

impl Glyph<'_> {
    pub fn width(&self) -> u32 {
        self.width
    }
}

#[derive(Debug, Clone)]
pub struct GlyphRowIter<'a> {
    bytes: &'a [u8],
    indices: u8,
}

impl<'a> GlyphRowIter<'a> {
    fn split_indices(&self) -> (u8, u8) {
        (self.indices & 7, self.indices >> 4)
    }
}

impl<'a> Iterator for GlyphRowIter<'a> {
    type Item = bool;
    fn next(&mut self) -> Option<bool> {
        let (first, rest) = self.bytes.split_first()?;
        let (start, end) = self.split_indices();
        if self.bytes.len() <= 1 && end < start {
            self.bytes = &[];
            return None;
        }

        if start == 7 {
            self.bytes = rest;
            self.indices ^= 7;
        } else {
            self.indices += 1;
        }
        Some(first >> 7 - start & 1 != 0)
    }

    fn nth(&mut self, n: usize) -> Option<bool> {
        self.bytes = &self.bytes[self.bytes.len().min(n / 8 + 1)..];
        self.indices += (n % 8) as u8;
        if self.indices & 8 != 0 {
            self.bytes = self.bytes.get(1..).unwrap_or(&[]);
            self.indices ^= 8;
        }
        self.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.bytes.len();
        let (start, end) = self.split_indices();
        let len = 8 * len - start as usize + end as usize - 8;
        (len, Some(len))
    }

    fn count(self) -> usize {
        self.size_hint().0
    }
}

impl ExactSizeIterator for GlyphRowIter<'_> {}
impl FusedIterator for GlyphRowIter<'_> {}

impl<'a> DoubleEndedIterator for GlyphRowIter<'a> {
    fn next_back(&mut self) -> Option<bool> {
        let (last, rest) = self.bytes.split_last()?;
        let (start, end) = self.split_indices();
        if self.bytes.len() <= 1 && end < start {
            self.bytes = &[];
            return None;
        }

        if end == 0 {
            self.bytes = rest;
            self.indices ^= 0x70;
        } else {
            self.indices -= 0x10;
        }
        Some(last >> 7 - end & 1 != 0)
    }

    fn nth_back(&mut self, n: usize) -> Option<bool> {
        self.bytes = &self.bytes[..self.bytes.len().saturating_sub(n / 8)];
        self.indices -= (n % 8 << 4) as u8;
        if self.indices & 0x80 != 0 {
            self.bytes = &self.bytes[..self.bytes.len().saturating_sub(1)];
            self.indices ^= 0x80;
        }
        self.next_back()
    }
}

impl<'a> Glyph<'a> {
    pub fn rows(
        &self,
    ) -> impl 'a
           + Iterator<Item = GlyphRowIter<'a>>
           + DoubleEndedIterator
           + FusedIterator
           + ExactSizeIterator {
        let indices = ((self.width + 7) % 8 << 4) as _;
        self.bytes
            .chunks((self.width + 7 >> 3) as _)
            .map(move |bytes| GlyphRowIter { bytes, indices })
    }
}

pub struct UnicodeTableEntries<'a> {
    raw_bytes: &'a [u8],
    version: PsfVersion,
    entry_index: u32,
    index: usize,
}

#[derive(Debug, Clone, Copy, Hash)]
pub enum UnicodeTableEntryValue<'a> {
    Utf8(&'a str),
    Ucs2(&'a Ucs2Str),
}

#[derive(Debug, Clone, Hash)]
pub struct UnicodeTableEntry<'a> {
    pub index: u32,
    pub value: UnicodeTableEntryValue<'a>,
}

impl<'a> Iterator for UnicodeTableEntries<'a> {
    type Item = UnicodeTableEntry<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        const ERROR_MSG: &str = "PSF unicode table is invalid";

        if self.index == self.raw_bytes.len() {
            return None;
        }

        let (entry_index, str_start) = (self.entry_index, self.index);
        let mut str_end = self.index;
        match self.version {
            PsfVersion::Psf1 => {
                str_end += 2 * self.raw_bytes[self.index..]
                    .chunks(2)
                    .position(|ch| matches!(ch, [0xFF | 0xFE, 0xFF]))
                    .expect(ERROR_MSG);
                self.index = str_end + 2;
            }
            PsfVersion::Psf2 => {
                str_end += self.raw_bytes[self.index..]
                    .iter()
                    .position(|&ch| 0xFD < ch)
                    .expect(ERROR_MSG);
                self.index = str_end + 1;
            }
        }
        if self.raw_bytes[str_end] == 0xFF {
            self.entry_index += 1;
        }
        let bytes = &self.raw_bytes[str_start..str_end];
        // if bytes.is_empty() {
        //     panic!("{ERROR_MSG}");
        // }
        Some(UnicodeTableEntry {
            index: entry_index,
            value: match self.version {
                PsfVersion::Psf1 => {
                    UnicodeTableEntryValue::Ucs2(Ucs2Str::from_bytes(bytes).expect(ERROR_MSG))
                }
                PsfVersion::Psf2 => {
                    UnicodeTableEntryValue::Utf8(str::from_utf8(bytes).expect(ERROR_MSG))
                }
            },
        })
    }
}
