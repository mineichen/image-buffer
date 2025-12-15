use std::{
    fmt::{self, Debug, Formatter},
    num::{NonZeroU8, NonZeroU32},
    sync::Arc,
};

use crate::vec;

pub(crate) trait ChannelSize:
    Sized + PartialEq + Clone + Copy + Send + Sync + 'static
{
    fn get_pixel_channels(&self) -> NonZeroU8;
}

// PIXEL_CHANNELS is usize, because it's also used to define array lengths. Casting in const is not currently possible
#[derive(Clone, Copy, PartialEq, Default)]
pub struct ComptimeChannelSize<const PIXEL_CHANNELS: usize>();

impl<const PIXEL_CHANNELS: usize> ComptimeChannelSize<PIXEL_CHANNELS> {
    pub const fn get_pixel_channels_const(&self) -> NonZeroU8 {
        NonZeroU8::new(PIXEL_CHANNELS as u8).unwrap()
    }
}

impl<const PIXEL_CHANNELS: usize> ChannelSize for ComptimeChannelSize<PIXEL_CHANNELS> {
    fn get_pixel_channels(&self) -> NonZeroU8 {
        const {
            if PIXEL_CHANNELS > 255 {
                panic!("PIXEL_CHANNELS must be less than 256");
            }
            NonZeroU8::new(PIXEL_CHANNELS as u8).unwrap()
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct RuntimeChannelSize(pub(crate) NonZeroU8);

impl ChannelSize for RuntimeChannelSize {
    fn get_pixel_channels(&self) -> NonZeroU8 {
        self.0
    }
}

pub struct ImageChannel<T: 'static, TS: ChannelSize>(UnsafeImageChannel<T, TS>);

impl<T, TS: ChannelSize> Clone for ImageChannel<T, TS> {
    fn clone(&self) -> Self {
        Self(unsafe { (self.0.vtable.clone)(&self.0) })
    }
}

impl<T: std::cmp::PartialEq, TS: ChannelSize> PartialEq for ImageChannel<T, TS> {
    fn eq(&self, other: &Self) -> bool {
        self.0.width == other.0.width
            && self.0.height == other.0.height
            && self.0.channel_size == other.0.channel_size
            && self.buffer() == other.buffer()
    }
}

impl<T: Clone, TS: ChannelSize> ImageChannel<T, TS> {
    pub fn new_vec(input: Vec<T>, width: NonZeroU32, height: NonZeroU32, channel_size: TS) -> Self {
        assert_eq!(
            input.len(),
            UnsafeImageChannel::<T, TS>::calc_ptr_len_from_parts(width, height, channel_size),
            "Incompatible Buffer-Size"
        );

        Self(UnsafeImageChannel::new_vec(
            input,
            width,
            height,
            channel_size,
        ))
    }

    pub fn new_arc(
        input: Arc<[T]>,
        width: NonZeroU32,
        height: NonZeroU32,
        channel_size: TS,
    ) -> Self {
        Self(UnsafeImageChannel::new_arc(
            input,
            width,
            height,
            channel_size,
        ))
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
        vtable: &'static ImageChannelVTable<T, TS>,
        generic_field: *mut (),
        channel_size: TS,
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
                channel_size,
            ))
        }
    }
}
impl<T: 'static, TS: ChannelSize> ImageChannel<T, TS> {
    /// Create an ImageChannel from an UnsafeImageChannel (used internally)
    pub(crate) fn from_unsafe_internal(unsafe_channel: UnsafeImageChannel<T, TS>) -> Self {
        Self(unsafe_channel)
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.0.calc_ptr_len()
    }

    pub fn buffer(&self) -> &[T] {
        let len = self.len();
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
        let vec_drop: unsafe extern "C" fn(&mut UnsafeImageChannel<T, TS>) =
            vec::clear_vec_channel::<T, TS>;
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

    pub const fn width(&self) -> NonZeroU32 {
        self.0.width
    }

    pub const fn height(&self) -> NonZeroU32 {
        self.0.height
    }

    pub const fn dimensions(&self) -> (NonZeroU32, NonZeroU32) {
        (self.0.width, self.0.height)
    }
}

impl<TP: std::any::Any, S: ChannelSize> Debug for ImageChannel<TP, S> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageChannel")
            .field("width", &self.0.width)
            .field("height", &self.0.height)
            .field("pixel", &std::any::type_name::<TP>())
            .field("pixel_channels", &self.0.channel_size.get_pixel_channels())
            .finish()
    }
}

unsafe impl<TP: Send, S: ChannelSize> Send for ImageChannel<TP, S> {}
unsafe impl<TP: Sync, S: ChannelSize> Sync for ImageChannel<TP, S> {}

#[repr(C)]
pub struct ImageChannelVTable<T: 'static, TS: ChannelSize> {
    pub clone: unsafe extern "C" fn(&UnsafeImageChannel<T, TS>) -> UnsafeImageChannel<T, TS>,
    pub make_mut: unsafe extern "C" fn(&mut UnsafeImageChannel<T, TS>),
    pub drop: unsafe extern "C" fn(&mut UnsafeImageChannel<T, TS>),
}

#[repr(C)]
pub struct UnsafeImageChannel<T: 'static, TS: ChannelSize> {
    pub ptr: *const T,
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    pub vtable: &'static ImageChannelVTable<T, TS>,
    // Has to be cleaned up by clear proc too
    pub data: *mut (),
    pub channel_size: TS,
}

impl<T: 'static, TS: ChannelSize> UnsafeImageChannel<T, TS> {
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
        vtable: &'static ImageChannelVTable<T, TS>,
        generic_field: *mut (),
        channel_size: TS,
    ) -> Self {
        UnsafeImageChannel {
            ptr,
            width,
            height,
            vtable,
            data: generic_field,
            channel_size,
        }
    }

    pub fn calc_ptr_len(&self) -> usize {
        Self::calc_ptr_len_from_parts(self.width, self.height, self.channel_size)
    }

    pub(crate) fn calc_ptr_len_from_parts(
        width: NonZeroU32,
        height: NonZeroU32,
        channel_size: impl ChannelSize,
    ) -> usize {
        assert!(width.get() <= usize::MAX as u32);
        assert!(height.get() <= usize::MAX as u32);

        width.get() as usize
            * height.get() as usize
            * channel_size.get_pixel_channels().get() as usize
    }
}

impl<T, TS: ChannelSize> Drop for UnsafeImageChannel<T, TS> {
    fn drop(&mut self) {
        if self.ptr as usize != 0 {
            unsafe { (self.vtable.drop)(self) };
        }
    }
}

// Workaround inability to have static which uses Outer Generics
pub(crate) trait ChannelFactory<T: 'static, TS: ChannelSize> {
    const VTABLE: &'static ImageChannelVTable<T, TS>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miri_create_and_clear_vec_image_channel() {
        let size = 2.try_into().unwrap();
        let image = ImageChannel::new_vec(
            vec![0u8, 64u8, 128u8, 192u8],
            size,
            size,
            ComptimeChannelSize::<1>(),
        );
        assert_eq!(image.buffer(), &[0u8, 64u8, 128u8, 192u8]);
    }

    #[test]
    fn miri_to_vec_reuses_pointer() {
        let raw = vec![0u8, 64u8, 128u8, 192u8];
        let pointer = raw[..].as_ptr();
        let size = 2.try_into().unwrap();
        let image = ImageChannel::new_vec(raw, size, size, RuntimeChannelSize(NonZeroU8::MIN));
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
        let mut image = ImageChannel::new_arc(raw, size, size, RuntimeChannelSize(NonZeroU8::MIN));
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
        let mut image = ImageChannel::new_arc(raw, size, size, RuntimeChannelSize(NonZeroU8::MIN));
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
        let image = ImageChannel::new_arc(raw, size, size, RuntimeChannelSize(NonZeroU8::MIN));
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
        let image = ImageChannel::new_vec(raw, size, size, ComptimeChannelSize::<1>());
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
        test_entire_vtable(ImageChannel::new_arc(
            arc,
            NonZeroU32::MIN,
            NonZeroU32::MIN,
            RuntimeChannelSize(NonZeroU8::MIN),
        ));
    }

    #[test]
    fn miri_test_exclusive_arc_u16_channel() {
        test_entire_vtable(ImageChannel::new_arc(
            vec![1u16].into(),
            NonZeroU32::MIN,
            NonZeroU32::MIN,
            RuntimeChannelSize(NonZeroU8::MIN),
        ));
    }

    #[test]
    fn miri_test_vec_u16_channel() {
        test_entire_vtable(ImageChannel::new_vec(
            vec![1u16],
            NonZeroU32::MIN,
            NonZeroU32::MIN,
            ComptimeChannelSize::<1>(),
        ));
    }

    #[test]
    fn miri_test_vec_rgb16_channel() {
        test_entire_vtable(ImageChannel::new_vec(
            vec![1u16, 2u16, 3u16],
            NonZeroU32::MIN,
            NonZeroU32::MIN,
            RuntimeChannelSize(const { NonZeroU8::new(3).unwrap() }),
        ));
    }

    fn test_entire_vtable<T: 'static + Default + Eq + Debug, TS: ChannelSize>(
        mut image: ImageChannel<T, TS>,
    ) {
        image.make_mut()[0] = T::default();
        let clone = image.clone();
        assert_eq!(image.make_mut()[0], T::default());
        image.make_mut()[0] = T::default();

        assert_eq!(image, clone);
    }
}
