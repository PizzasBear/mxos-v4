use core::{cmp::Ordering, fmt, mem, ops, str};

// should be less than (255 / 4)
const MAX_UNICODE_CHAR_NUM: usize = 4;

#[derive(Debug)]
pub enum Error {
    UnexpectedEnd,
    UnknownPsf2Version(u32),
    InvalidMagic,
    InvalidGlyphSize,
    EmptyUnicodeTableEntry,
    UnicodeTooLong,
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
            Self::UnicodeTooLong => {
                write!(
                    f,
                    "A glyph's unicode string cannot be longer {MAX_UNICODE_CHAR_NUM} characters",
                )
            }
            Self::Utf8Error(err) => {
                write!(f, "Unicode table UTF-8 string parsing errored: {err}")
            }
            Self::Ucs2Error => {
                write!(f, "Unicode table UCS-2 LE string parsing errored")
            }
            Self::EmptyUnicodeTableEntry => {
                write!(f, "Unicode table entry is empty")
            }
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

// #[derive(Debug, Default, Clone, Copy)]
// pub struct CacheEntry {
//     pos: usize,
//     glyph: u32,
// }

// struct Cache<const N: usize> {
//     max_lens: [u8; 255],
//     map: [CacheEntry; N],
// }

#[derive(Debug, Clone, Copy)]
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

pub struct UnicodeTableEntries<'a> {
    file: &'a PsfFile<'a>,
    entry: u32,
    index: usize,
}

pub struct Ucs2Str<T: ?Sized = [u8]>(T);

impl Ucs2Str {
    pub const EMPTY: &'static Self = &Ucs2Str([]);

    fn verify(bytes: &[u8]) -> bool {
        bytes.chunks(2).all(|c| {
            char::from_u32(u16::from_le_bytes(match c.try_into() {
                Ok(b) => b,
                Err(_) => return false,
            }) as _)
            .is_some()
        })
    }
    pub fn from_bytes(bytes: &[u8]) -> Option<&Self> {
        match Self::verify(bytes) {
            true => Some(unsafe { Self::from_bytes_unchecked(bytes) }),
            false => None,
        }
    }
    pub fn from_bytes_mut(bytes: &mut [u8]) -> Option<&mut Self> {
        match Self::verify(bytes) {
            true => Some(unsafe { Self::from_bytes_mut_unchecked(bytes) }),
            false => None,
        }
    }
    pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> &Self {
        unsafe { mem::transmute(bytes) }
    }
    pub unsafe fn from_bytes_mut_unchecked(bytes: &mut [u8]) -> &mut Self {
        unsafe { mem::transmute(bytes) }
    }

    pub fn len(&self) -> usize {
        self.0.len() / 2
    }

    pub fn get(&self, index: usize) -> Option<char> {
        unsafe {
            Some(char::from_u32_unchecked(u16::from_le_bytes(
                self.0
                    .get(2 * index..2 * index + 2)?
                    .try_into()
                    .unwrap_unchecked(),
            ) as _))
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl<'a> TryFrom<&'a [u8]> for &'a Ucs2Str {
    type Error = ();
    fn try_from(bytes: &'a [u8]) -> Result<Self, ()> {
        Ucs2Str::from_bytes(bytes).ok_or(())
    }
}
impl<'a> TryFrom<&'a mut [u8]> for &'a mut Ucs2Str {
    type Error = ();
    fn try_from(bytes: &'a mut [u8]) -> Result<Self, ()> {
        Ucs2Str::from_bytes_mut(bytes).ok_or(())
    }
}

impl<R: ops::RangeBounds<usize>> ops::Index<R> for Ucs2Str {
    type Output = Self;

    fn index(&self, range: R) -> &Self {
        let start = match range.start_bound() {
            ops::Bound::Included(start) => 2 * start,
            ops::Bound::Excluded(start) => 2 * (start + 1),
            ops::Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            ops::Bound::Included(_) => todo!(),
            ops::Bound::Excluded(_) => todo!(),
            ops::Bound::Unbounded => self.0.len(),
        };
        unsafe { mem::transmute(&self.0[start..end]) }
    }
}

pub enum UnicodeTableEntryValue<'a> {
    Utf8(&'a str),
    Ucs2(&'a Ucs2Str),
}

pub struct UnicodeTableEntry<'a> {
    pub entry: u32,
    pub value: UnicodeTableEntryValue<'a>,
}

impl<'a> Iterator for UnicodeTableEntries<'a> {
    type Item = UnicodeTableEntry<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        const ERROR_MSG: &str = "PSF unicode table is invalid";

        if self.index == self.file.raw_bytes.len() {
            return None;
        }

        let (entry, str_start) = (self.entry, self.index);
        let str_end;
        match self.file.version {
            PsfVersion::Psf1 => {
                str_end = 2 * self.file.raw_bytes[self.index..]
                    .chunks(2)
                    .position(|ch| matches!(ch, [0xFF | 0xFE, 0xFF]))
                    .expect(ERROR_MSG);
                self.index = str_end + 2;
            }
            PsfVersion::Psf2 => {
                str_end = self.file.raw_bytes[self.index..]
                    .iter()
                    .position(|&ch| 0xFD < ch)
                    .expect(ERROR_MSG);
                self.index = str_end + 1;
            }
        }
        if self.file.raw_bytes[str_end] == 0xFF {
            self.entry += 1;
        }
        let bytes = &self.file.raw_bytes[str_start..str_end];
        if bytes.is_empty() {
            panic!("{ERROR_MSG}");
        }
        Some(UnicodeTableEntry {
            entry,
            value: match self.file.version {
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
        let header_nums: &[u32; 8] =
            bytemuck::cast_ref(<&[u8; 32]>::try_from(header_bytes).unwrap());
        if header_nums[0] != 0x864ab572 {
            return Err(Error::InvalidMagic);
        }
        if header_nums[1] != 0 {
            return Err(Error::UnknownPsf2Version(header_nums[1]));
        }
        let mut slf = Self {
            raw_bytes: bytes,
            version: PsfVersion::Psf2,
            header_size: header_nums[2].max(32),
            has_unicode_table: header_nums[3] & 1 != 0,
            num_glyphs: header_nums[4],
            glyph_height: header_nums[6],
            glyph_width: header_nums[7],
            glyph_size: {
                let size = header_nums[6] * ((header_nums[7] + 7) / 8);
                if header_nums[5] != size {
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

    pub fn unicode_table_start(&self) -> usize {
        self.header_size as usize + self.num_glyphs as usize * self.glyph_size as usize
    }

    fn process_unicode_table(&mut self) -> Result<()> {
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
                            if len == 0 {
                                return Err(Error::EmptyUnicodeTableEntry);
                            }
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
                    if len == 0 {
                        return Err(Error::EmptyUnicodeTableEntry);
                    }
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
        if MAX_UNICODE_CHAR_NUM < max {
            return Err(Error::UnicodeTooLong);
        }
        self.longest_glyph = max as _;

        Ok(())
    }

    pub fn unicode_table_entries(&self) -> UnicodeTableEntries {
        UnicodeTableEntries {
            file: self,
            entry: 0,
            index: self.unicode_table_start(),
        }
    }

    pub fn find_glyph(&self, s: &str) -> Option<usize> {
        let _table = &self.raw_bytes[self.unicode_table_start()..];

        let substr = &s[..s
            .char_indices()
            .take(self.longest_glyph as _)
            .last()
            .map(|(i, ch)| i + ch.len_utf8())?];

        for (i, ch) in substr.char_indices().rev() {
            let _substr = &substr[..i + ch.len_utf8()];
        }

        todo!()
    }
}
