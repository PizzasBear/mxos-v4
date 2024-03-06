use core::{
    borrow::{Borrow, BorrowMut},
    ops,
};

macro_rules! def_align {
    ($name:ident, $make:ident, $align:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
        #[repr(C, align($align))]
        pub struct $name<T>(pub T);

        #[inline]
        #[must_use]
        pub const fn $make<T>(x: T) -> $name<T> {
            $name::new(x)
        }

        impl<T> $name<T> {
            #[inline]
            #[must_use]
            pub const fn new(x: T) -> Self {
                Self(x)
            }

            #[inline]
            #[must_use]
            pub fn into_inner(Self(x): Self) -> T {
                x
            }

            #[inline]
            #[must_use]
            pub const fn inner(&self) -> &T {
                &self.0
            }

            #[inline]
            #[must_use]
            pub fn inner_mut(&mut self) -> &mut T {
                &mut self.0
            }
        }

        impl<T> From<T> for $name<T> {
            #[inline]
            #[must_use]
            fn from(x: T) -> Self {
                Self(x)
            }
        }

        impl<T> ops::Deref for $name<T> {
            type Target = T;

            #[inline]
            #[must_use]
            fn deref(&self) -> &T {
                &self.0
            }
        }

        impl<T> ops::DerefMut for $name<T> {
            #[inline]
            #[must_use]
            fn deref_mut(&mut self) -> &mut T {
                &mut self.0
            }
        }

        impl<T> AsRef<T> for $name<T> {
            #[inline]
            #[must_use]
            fn as_ref(&self) -> &T {
                self
            }
        }

        impl<T> AsMut<T> for $name<T> {
            #[inline]
            #[must_use]
            fn as_mut(&mut self) -> &mut T {
                self
            }
        }

        impl<T> Borrow<T> for $name<T> {
            #[inline]
            #[must_use]
            fn borrow(&self) -> &T {
                self
            }
        }

        impl<T> BorrowMut<T> for $name<T> {
            #[inline]
            #[must_use]
            fn borrow_mut(&mut self) -> &mut T {
                self
            }
        }
    };
}

def_align!(Align2, align2, 2);
def_align!(Align4, align4, 4);
def_align!(Align8, align8, 8);
def_align!(Align16, align16, 16);
def_align!(Align32, align32, 32);
def_align!(Align64, align64, 64);
def_align!(Align128, align128, 128);
def_align!(Align256, align256, 256);
def_align!(Align512, align512, 512);
def_align!(Align1024, align1024, 1024);
def_align!(Align2048, align2048, 2048);
def_align!(Align4096, align4096, 4096);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
#[repr(C, align(8))]
pub struct U32Pair(pub u32, pub u32);

pub const fn u32_pair(a: u32, b: u32) -> U32Pair {
    U32Pair(a, b)
}
