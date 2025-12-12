use std::{
    fmt::{self, Debug, Formatter},
    num::NonZeroU32,
    sync::Arc,
};

use crate::vec;

#[repr(transparent)]
pub struct ImageChannel<T: 'static>(UnsafeImageChannel<T>);

impl<T> Clone for ImageChannel<T> {
    fn clone(&self) -> Self {
        Self(unsafe { (self.0.vtable.clone)(&self.0) })
    }
}

impl<T: std::cmp::PartialEq> PartialEq for ImageChannel<T> {
    fn eq(&self, other: &Self) -> bool {
        self.0.width == other.0.width
            && self.0.height == other.0.height
            && self.buffer() == other.buffer()
    }
}

impl<T: 'static> ImageChannel<T> {
    pub fn new_vec(input: Vec<T>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        Self(UnsafeImageChannel::new_vec(input, width, height))
    }

    pub fn new_arc(input: Arc<[T]>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        Self(UnsafeImageChannel::new_arc(input, width, height))
    }

    /// Don't use this method unless you need a custom image.
    ///
    /// Use/provide methods like new_vec() and new_arc() for safe construction
    ///
    /// # Safety
    /// The vtable must be able to cleanup the fields
    pub unsafe fn new_with_vtable(
        ptr: *const T,
        width: NonZeroU32,
        height: NonZeroU32,
        vtable: &'static ImageChannelVTable<T>,
        generic_field: *mut (),
    ) -> Self
    where
        T: Send + Sync,
    {
        unsafe {
            Self(UnsafeImageChannel::new_with_vtable(
                ptr,
                width,
                height,
                vtable,
                generic_field,
            ))
        }
    }

    /// Create an ImageChannel from an UnsafeImageChannel (used internally)
    pub(crate) fn from_unsafe_internal(unsafe_channel: UnsafeImageChannel<T>) -> Self {
        Self(unsafe_channel)
    }

    #[allow(clippy::len_without_is_empty)]
    pub const fn len(&self) -> usize {
        assert!(self.0.width.get() <= usize::MAX as u32);
        assert!(self.0.height.get() <= usize::MAX as u32);
        self.0.width.get() as usize * self.0.height.get() as usize
    }

    pub const fn buffer(&self) -> &[T] {
        let len = self.0.width.get() as usize * self.0.height.get() as usize;
        unsafe { std::slice::from_raw_parts(self.0.ptr, len) }
    }

    pub fn make_mut(&mut self) -> &mut [T] {
        unsafe {
            (self.0.vtable.make_mut)(&mut self.0);
            let len = self.len();
            std::slice::from_raw_parts_mut(self.0.ptr as *mut T, len)
        }
    }

    pub fn into_vec(self) -> Vec<T>
    where
        T: Clone,
    {
        let vec_drop: unsafe extern "C" fn(&mut UnsafeImageChannel<T>) =
            vec::clear_vec_channel::<T>;
        // Check if this is a Vec-backed channel
        if std::ptr::fn_addr_eq(self.0.vtable.drop, vec_drop) {
            let size = self.len();
            let result =
                unsafe { Vec::from_raw_parts(self.0.ptr as *mut _, size, self.0.data as usize) };
            std::mem::forget(self);
            result
        } else {
            // Arc-backed, SharedVec, or other - clone the data
            let buffer = self.buffer();
            let mut result = Vec::with_capacity(self.len());
            result.extend_from_slice(buffer);
            result
        }
    }

    pub fn width(&self) -> NonZeroU32 {
        self.0.width
    }

    pub fn height(&self) -> NonZeroU32 {
        self.0.height
    }

    pub fn dimensions(&self) -> (NonZeroU32, NonZeroU32) {
        (self.0.width, self.0.height)
    }
}

impl<TP: std::any::Any> Debug for ImageChannel<TP> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageChannel")
            .field("width", &self.0.width)
            .field("height", &self.0.height)
            .field("pixel", &std::any::type_name::<TP>())
            .finish()
    }
}

unsafe impl<TP: Send> Send for ImageChannel<TP> {}
unsafe impl<TP: Sync> Sync for ImageChannel<TP> {}

#[repr(C)]
pub struct ImageChannelVTable<T: 'static> {
    pub clone: unsafe extern "C" fn(&UnsafeImageChannel<T>) -> UnsafeImageChannel<T>,
    pub make_mut: unsafe extern "C" fn(&mut UnsafeImageChannel<T>),
    pub drop: unsafe extern "C" fn(&mut UnsafeImageChannel<T>),
}

#[repr(C)]
pub struct UnsafeImageChannel<T: 'static> {
    pub ptr: *const T,
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    pub vtable: &'static ImageChannelVTable<T>,
    // Has to be cleaned up by clear proc too
    pub data: *mut (),
}

impl<T: 'static> UnsafeImageChannel<T> {
    /// Don't use this method unless you need a custom image.
    ///
    /// Use/provide methods like new_vec() and new_arc() for safe construction
    ///
    /// # Safety
    /// The vtable must be able to cleanup the fields
    pub unsafe fn new_with_vtable(
        ptr: *const T,
        width: NonZeroU32,
        height: NonZeroU32,
        vtable: &'static ImageChannelVTable<T>,
        generic_field: *mut (),
    ) -> Self {
        UnsafeImageChannel {
            ptr,
            width,
            height,
            vtable,
            data: generic_field,
        }
    }
}

impl<T> Drop for UnsafeImageChannel<T> {
    fn drop(&mut self) {
        if self.ptr as usize != 0 {
            unsafe { (self.vtable.drop)(self) };
        }
    }
}

// Workaround inability to have static which uses Outer Generics
pub(crate) trait ChannelFactory<T: 'static> {
    const VTABLE: &'static ImageChannelVTable<T>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miri_create_and_clear_vec_image_channel() {
        let size = 2.try_into().unwrap();
        let image = ImageChannel::new_vec(vec![0u8, 64u8, 128u8, 192u8], size, size);
        assert_eq!(image.buffer(), &[0u8, 64u8, 128u8, 192u8]);
    }

    #[test]
    fn miri_to_vec_reuses_pointer() {
        let raw = vec![0u8, 64u8, 128u8, 192u8];
        let pointer = raw[..].as_ptr();
        let size = 2.try_into().unwrap();
        let image = ImageChannel::new_vec(raw, size, size);
        let to_vec = image.into_vec();

        // Miri seems to generate clear_vec_channel::<const u8> for each call
        // It works on native x86. Because it's only an optimization, this is good enough
        // VTable is not possible, as ImageChannel is ABI-Stable and multiple dylibs use their own allocator for Vecs
        if !cfg!(miri) {
            assert_eq!(
                to_vec[..].as_ptr(),
                pointer,
                "Should reuse the buffer if it was created by vec"
            );
        }
    }

    #[test]
    fn miri_make_mut_reuses_arc_pointer() {
        let raw = Arc::<[u8]>::from([0u8, 64u8, 128u8, 192u8].as_slice());
        let pointer = raw[..].as_ptr();
        let size = 2.try_into().unwrap();
        let mut image = ImageChannel::new_arc(raw, size, size);
        let ptr_mut = image.make_mut();

        assert_eq!(
            ptr_mut[..].as_ptr(),
            pointer,
            "Should reuse the buffer if it was created by arc"
        );
    }

    #[test]
    fn miri_make_mut_doesnt_reuse_arc_pointer_if_not_unique() {
        let raw = Arc::<[u8]>::from([0u8, 64u8, 128u8, 192u8].as_slice());
        let _raw2 = raw.clone();
        let pointer = raw[..].as_ptr();
        let size = 2.try_into().unwrap();
        let mut image = ImageChannel::new_arc(raw, size, size);
        let ptr_mut = image.make_mut();

        assert_ne!(
            ptr_mut[..].as_ptr(),
            pointer,
            "Should not reuse the buffer if arc is not unique"
        );
    }

    #[test]
    fn miri_clone_arc_backed_shares_memory() {
        let raw = Arc::<[u8]>::from([0u8, 64u8, 128u8, 192u8].as_slice());
        let pointer = raw[..].as_ptr();
        let size = 2.try_into().unwrap();
        let image = ImageChannel::new_arc(raw, size, size);
        let image2 = image.clone();

        assert_eq!(
            image2.buffer().as_ptr(),
            pointer,
            "Should share the buffer if it was created by arc"
        );
    }

    #[test]
    fn miri_clone_from_vec() {
        let raw = vec![0u8, 64u8, 128u8, 192u8];
        let size = 2.try_into().unwrap();
        let image = ImageChannel::new_vec(raw, size, size);
        let image2 = image.clone();
        let to_vec = image.into_vec();
        let to_vec2 = image2.into_vec();

        assert_ne!(
            to_vec[..].as_ptr(),
            to_vec2[..].as_ptr(),
            "Should not share the buffer if it was created by vec"
        );
    }

    #[test]
    fn miri_test_shared_arc_u16_channel() {
        let arc: Arc<[u16]> = vec![1].into();
        test_entire_vtable(ImageChannel::<u16>::new_arc(
            arc,
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }

    #[test]
    fn miri_test_exclusive_arc_u16_channel() {
        test_entire_vtable(ImageChannel::<u16>::new_arc(
            vec![1].into(),
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }

    #[test]
    fn miri_test_vec_u16_channel() {
        test_entire_vtable(ImageChannel::<u16>::new_vec(
            vec![1],
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }

    fn test_entire_vtable<T: 'static + Default + Eq + Debug>(mut image: ImageChannel<T>) {
        image.make_mut()[0] = T::default();
        let clone = image.clone();
        assert_eq!(image.make_mut()[0], T::default());
        image.make_mut()[0] = T::default();

        assert_eq!(image, clone);
    }
}
