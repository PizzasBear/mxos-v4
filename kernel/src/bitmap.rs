use core::mem;

pub struct Bitmap<T: AsRef<[usize]> + ?Sized = [usize]>(pub T);

impl Bitmap {
    pub fn from_slice(slice: &[usize]) -> &Self {
        unsafe { mem::transmute(slice) }
    }

    pub fn from_slice_mut(slice: &mut [usize]) -> &mut Self {
        unsafe { mem::transmute(slice) }
    }
}

impl<T: ?Sized + AsRef<[usize]>> Bitmap<T> {
    /// Splits a bit into an index and a one-hot bitmask
    fn split_bit(bit: usize) -> (usize, usize) {
        (bit / usize::BITS as usize, 1 << bit as u32 % usize::BITS)
    }

    fn merge_bit(index: usize, bit: u32) -> usize {
        usize::BITS as usize * index + bit as usize
    }

    pub fn get(&self, bit: usize) -> bool {
        let (i, mask) = Self::split_bit(bit);
        self.0.as_ref()[i] & mask != 0
    }

    pub fn find_first_set(&self, start: usize) -> Option<usize> {
        let (i, bits) = (self.0.as_ref().iter())
            .enumerate()
            .skip(start / usize::BITS as usize)
            .find(|(_, &b)| b != 0)?;
        Some(Self::merge_bit(i, bits.trailing_zeros()))
    }

    pub fn find_first_unset(&self, start: usize) -> Option<usize> {
        let (i, bits) = (self.0.as_ref().iter())
            .enumerate()
            .skip(start / usize::BITS as usize)
            .find(|(_, &b)| b != !0)?;
        Some(Self::merge_bit(i, bits.trailing_ones()))
    }

    // pub fn find_last_set(&self) -> Option<usize> {
    //     let (i, bits) = self.0.as_ref().iter().enumerate().rfind(|(_, &b)| b != 0)?;
    //     Some(Self::merge_bit(i, usize::BITS - 1 - bits.leading_zeros()))
    // }

    // pub fn find_last_unset(&self) -> Option<usize> {
    //     let (i, bits) = self.0.as_ref().iter().enumerate().rfind(|(_, &b)| b != 0)?;
    //     Some(Self::merge_bit(i, usize::BITS - 1 - bits.leading_ones()))
    // }
}

impl<T: ?Sized + AsRef<[usize]> + AsMut<[usize]>> Bitmap<T> {
    pub fn toggle(&mut self, bit: usize) {
        let (i, mask) = Self::split_bit(bit);
        self.0.as_mut()[i] ^= mask;
    }

    pub fn set(&mut self, bit: usize) {
        let (i, mask) = Self::split_bit(bit);
        self.0.as_mut()[i] |= mask;
    }

    pub fn reset(&mut self, bit: usize) {
        let (i, mask) = Self::split_bit(bit);
        self.0.as_mut()[i] &= !mask;
    }

    pub fn assign(&mut self, bit: usize, value: bool) {
        match value {
            true => self.set(bit),
            false => self.reset(bit),
        }
    }
}

impl<'a> From<&'a [usize]> for &'a Bitmap {
    fn from(slice: &'a [usize]) -> Self {
        Bitmap::from_slice(slice)
    }
}

impl<'a> From<&'a mut [usize]> for &'a mut Bitmap {
    fn from(slice: &'a mut [usize]) -> Self {
        Bitmap::from_slice_mut(slice)
    }
}
