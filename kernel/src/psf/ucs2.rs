use core::{fmt, iter::FusedIterator, mem, ops};

use alloc::string::String;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
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

    pub fn chars(
        &self,
    ) -> impl '_ + Iterator<Item = char> + DoubleEndedIterator + FusedIterator + ExactSizeIterator
    {
        self.0.chunks(2).map(|c| unsafe {
            char::from_u32(u16::from_le_bytes(c.try_into().unwrap_unchecked()) as _)
                .unwrap_unchecked()
        })
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

impl PartialEq<str> for Ucs2Str {
    fn eq(&self, other: &str) -> bool {
        self.chars().eq(other.chars())
    }
}
impl PartialEq<Ucs2Str> for str {
    fn eq(&self, other: &Ucs2Str) -> bool {
        self.chars().eq(other.chars())
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
impl<'a> From<&Ucs2Str> for String {
    fn from(s: &Ucs2Str) -> Self {
        s.chars().collect()
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

impl fmt::Display for Ucs2Str {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for ch in self.chars() {
            write!(f, "{ch}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for Ucs2Str {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct Adapter<'a>(&'a Ucs2Str);

        impl fmt::Debug for Adapter<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("\"")?;
                for ch in self.0.chars() {
                    write!(f, "{}", ch.escape_debug())?;
                }
                f.write_str("\"")?;
                Ok(())
            }
        }

        f.debug_tuple("Ucs2Str").field(&Adapter(self)).finish()
    }
}
