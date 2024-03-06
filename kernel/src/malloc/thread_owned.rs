use core::{marker::PhantomData, mem::MaybeUninit, ops};

// #[repr(transparent)]
// pub struct ThreadOwned<'a, T: ?Sized> {
//     inner: &'a T,
//     phantom: PhantomData<&'a mut ()>,
// }
//
// impl<'a, T: ?Sized> ThreadOwned<'a, T> {
//     pub unsafe fn new(inner: &'a T) -> Self {
//         Self {
//             inner,
//             phantom: PhantomData,
//         }
//     }
//
//     pub fn borrow(&mut self) -> ThreadOwned<T> {
//         unsafe { Self::new(self.inner) }
//     }
// }
//
// impl<'a, T: ?Sized> ops::Deref for ThreadOwned<'a, T> {
//     type Target = T;
//     fn deref(&self) -> &T {
//         self.inner
//     }
// }

pub struct ThreadOwned<'a, T> {
    inner: MaybeUninit<T>,
    phantom: PhantomData<&'a mut ()>,
}

impl<T> ThreadOwned<'_, T> {}
