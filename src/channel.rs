use std::{
    fmt::{self, Debug, Formatter},
    num::{NonZeroU8, NonZeroU32},
    sync::Arc,
};

use crate::{
    dynamic::DynamicImageChannel,
    pixel::{DynamicSize, PixelType, PixelTypePrimitive, RuntimePixelType},
    unwrap_usize_to_nonzero_u8, vec,
};

pub trait PixelChannels: Sized + PartialEq + Clone + Copy + Send + Sync + 'static {
    fn get(&self) -> NonZeroU8;
}

// PIXEL_CHANNELS is usize, because it's also used to define array lengths. Casting in const is not currently possible
#[derive(Clone, Copy, PartialEq, Default)]
pub struct ComptimeChannelSize<const PIXEL_CHANNELS: usize>();

impl<const PIXEL_CHANNELS: usize> PixelChannels for ComptimeChannelSize<PIXEL_CHANNELS> {
    fn get(&self) -> NonZeroU8 {
        const { unwrap_usize_to_nonzero_u8(PIXEL_CHANNELS) }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub struct RuntimeChannelSize(pub(crate) NonZeroU8);

impl Default for RuntimeChannelSize {
    fn default() -> Self {
        Self(NonZeroU8::MIN)
    }
}

impl PixelChannels for RuntimeChannelSize {
    fn get(&self) -> NonZeroU8 {
        self.0
    }
}

pub struct ImageChannel<TP: RuntimePixelType>(UnsafeImageChannel<TP::Primitive>);

impl<TP: RuntimePixelType> Clone for ImageChannel<TP> {
    fn clone(&self) -> Self {
        Self(unsafe { (self.0.vtable.clone)(&self.0) })
    }
}

impl<TP: RuntimePixelType> PartialEq for ImageChannel<TP>
where
    TP::Primitive: std::cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.0.width == other.0.width
            && self.0.height == other.0.height
            && self.0.channel_size == other.0.channel_size
            && self.flat_buffer() == other.flat_buffer()
    }
}

impl<TP: PixelType> ImageChannel<TP>
where
    TP: Clone,
    TP::Primitive: Clone,
{
    /// # Panics
    /// Panics if the buffer size is not compatible with the width and height.
    #[must_use]
    pub fn new_vec(mut input: Vec<TP>, width: NonZeroU32, height: NonZeroU32) -> Self {
        let channel_size = TP::ChannelSize::default();
        let expected_len = width.get() as usize * height.get() as usize;
        assert_eq!(input.len(), expected_len, "Incompatible Buffer-Size");

        // Cast Vec<TP> to Vec<TP::Primitive>
        let len = input.len();
        let cap = input.capacity();

        let ptr = input.as_mut_ptr().cast::<TP::Primitive>();
        let len = len * TP::PIXEL_CHANNELS.get() as usize;
        let cap = cap * TP::PIXEL_CHANNELS.get() as usize;
        std::mem::forget(input);

        // Safety: TP::Primitive is expected to be an aligned fraction of TP
        let cast_input = unsafe { Vec::from_raw_parts(ptr, len, cap) };

        Self(UnsafeImageChannel::new_vec(
            cast_input,
            width,
            height,
            channel_size.get(),
        ))
    }

    pub fn new_arc(input: Arc<[TP]>, width: NonZeroU32, height: NonZeroU32) -> Self {
        let channel_size = TP::ChannelSize::default();
        let len = input.len();
        let ptr = Arc::into_raw(input).cast::<TP::Primitive>();
        let len = len * TP::PIXEL_CHANNELS.get() as usize;

        // Safety: TP::Primitive is expected to be an aligned fraction of TP
        let cast_input = unsafe { Arc::from_raw(std::ptr::slice_from_raw_parts(ptr, len)) };

        Self(UnsafeImageChannel::new_arc(
            cast_input,
            width,
            height,
            channel_size.get(),
        ))
    }

    #[must_use]
    pub fn buffer(&self) -> &[TP] {
        let len = self.len();
        let buf = unsafe { std::slice::from_raw_parts(self.0.ptr, len) };
        let len = len / TP::PIXEL_CHANNELS.get() as usize;
        unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<TP>(), len) }
    }

    pub fn make_mut(&mut self) -> &mut [TP] {
        unsafe {
            (self.0.vtable.make_mut)(&mut self.0);
            let len = self.len();
            let len = len / TP::PIXEL_CHANNELS.get() as usize;
            std::slice::from_raw_parts_mut(self.0.ptr as *mut TP, len)
        }
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<TP>
    where
        TP: Clone,
    {
        // Get Vec<TP::Primitive> using the base implementation
        let vec_drop: unsafe extern "C" fn(&mut UnsafeImageChannel<TP::Primitive>) =
            vec::clear_vec_channel::<TP::Primitive>;
        let mut vec = if std::ptr::fn_addr_eq(self.0.vtable.drop, vec_drop) {
            let size = self.len();
            let result =
                unsafe { Vec::from_raw_parts(self.0.ptr.cast_mut(), size, self.0.data as usize) };
            std::mem::forget(self);
            result
        } else {
            let len = self.len();
            let buf = unsafe { std::slice::from_raw_parts(self.0.ptr, len) };
            buf.to_vec()
        };

        // Cast Vec<TP::Primitive> back to Vec<TP>
        let ptr = vec.as_mut_ptr().cast::<TP>();
        let len = vec.len() / TP::PIXEL_CHANNELS.get() as usize;
        let cap = vec.capacity() / TP::PIXEL_CHANNELS.get() as usize;
        std::mem::forget(vec);

        unsafe { Vec::from_raw_parts(ptr, len, cap) }
    }

    /// Don't use this method unless you need a custom image.
    ///
    /// Use/provide methods like `new_vec()` and `new_arc()` for safe construction
    ///
    /// # Safety
    /// The vtable must be able to cleanup the fields
    pub unsafe fn new_with_vtable(
        ptr: *const TP::Primitive,
        width: NonZeroU32,
        height: NonZeroU32,
        vtable: &'static ImageChannelVTable<TP::Primitive>,
        generic_field: *mut (),
    ) -> Self
    where
        TP::Primitive: Send + Sync,
    {
        let channel_size = TP::ChannelSize::default();
        unsafe {
            Self(UnsafeImageChannel::new_with_vtable(
                ptr,
                width,
                height,
                vtable,
                generic_field,
                channel_size.get(),
            ))
        }
    }
}

impl<TP: PixelType> TryFrom<DynamicImageChannel> for ImageChannel<TP> {
    type Error = DynamicImageChannel;

    fn try_from(value: DynamicImageChannel) -> Result<Self, Self::Error> {
        let typed = <TP::Primitive as PixelTypePrimitive>::try_from_dynamic_image(value)?;

        if typed.0.channel_size == TP::PIXEL_CHANNELS {
            Ok(ImageChannel(typed.0))
        } else {
            Err(<TP::Primitive as PixelTypePrimitive>::into_runtime_channel(
                typed,
            ))
        }
    }
}

impl<TP: PixelType> From<ImageChannel<TP>> for DynamicImageChannel {
    fn from(value: ImageChannel<TP>) -> Self {
        let flat_channel: ImageChannel<DynamicSize<TP::Primitive>> = ImageChannel(value.0);
        <TP::Primitive as PixelTypePrimitive>::into_runtime_channel(flat_channel)
    }
}

impl<TP: RuntimePixelType> ImageChannel<TP> {
    /// Create an `ImageChannel` from an `UnsafeImageChannel` (used internally)
    #[must_use]
    pub(crate) fn from_unsafe_internal(unsafe_channel: UnsafeImageChannel<TP::Primitive>) -> Self {
        Self(unsafe_channel)
    }

    #[allow(clippy::len_without_is_empty)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.calc_len_flat()
    }

    #[must_use]
    pub fn flat_buffer(&self) -> &[TP::Primitive] {
        let len = self.len();
        unsafe { std::slice::from_raw_parts(self.0.ptr, len) }
    }

    pub fn primitive_make_mut(&mut self) -> &mut [TP::Primitive] {
        unsafe {
            (self.0.vtable.make_mut)(&mut self.0);
            let len = self.len();
            std::slice::from_raw_parts_mut(self.0.ptr.cast_mut(), len)
        }
    }

    #[must_use]
    pub fn primitive_into_vec(self) -> Vec<TP::Primitive>
    where
        TP::Primitive: Clone,
    {
        let vec_drop: unsafe extern "C" fn(&mut UnsafeImageChannel<TP::Primitive>) =
            vec::clear_vec_channel::<TP::Primitive>;
        // Check if this is a Vec-backed channel
        if std::ptr::fn_addr_eq(self.0.vtable.drop, vec_drop) {
            let size = self.len();
            let result =
                unsafe { Vec::from_raw_parts(self.0.ptr.cast_mut(), size, self.0.data as usize) };
            std::mem::forget(self);
            result
        } else {
            self.flat_buffer().to_vec()
        }
    }

    #[must_use]
    pub const fn width(&self) -> NonZeroU32 {
        self.0.width
    }

    #[must_use]
    pub const fn height(&self) -> NonZeroU32 {
        self.0.height
    }

    #[must_use]
    pub const fn dimensions(&self) -> (NonZeroU32, NonZeroU32) {
        (self.0.width, self.0.height)
    }
}

impl<TP: RuntimePixelType> Debug for ImageChannel<TP>
where
    TP::Primitive: std::any::Any,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageChannel")
            .field("width", &self.0.width)
            .field("height", &self.0.height)
            .field("pixel", &std::any::type_name::<TP::Primitive>())
            .field("pixel_channels", &self.0.channel_size.get())
            .finish()
    }
}

unsafe impl<TP: RuntimePixelType> Send for ImageChannel<TP> where TP::Primitive: Send {}
unsafe impl<TP: RuntimePixelType> Sync for ImageChannel<TP> where TP::Primitive: Sync {}

/// `VTable` for `ImageChannel`
/// Reasons for not using the Bytes crate:
/// - Ability to have non static Images (`Image<u8, 1>` could become `ImageRef<'static, u8, 1>` in the future)
/// - Beign ABI-Stable and thus sharable between dylibs
/// - Initial design was much different... If the two arguments above are not enough, refactor to Bytes might be a good choice
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
    pub channel_size: NonZeroU8,
}

impl<T: 'static> UnsafeImageChannel<T> {
    /// Don't use this method unless you need a custom image.
    ///
    /// Use/provide methods like `new_vec()` and `new_arc()` for safe construction
    ///
    /// # Safety
    /// The vtable must be able to cleanup the fields
    pub unsafe fn new_with_vtable(
        ptr: *const T,
        width: NonZeroU32,
        height: NonZeroU32,
        vtable: &'static ImageChannelVTable<T>,
        generic_field: *mut (),
        channel_size: NonZeroU8,
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

    pub(crate) const fn calc_len_flat(&self) -> usize {
        calc_image_channel_len_flat(self.width, self.height, self.channel_size)
    }
}
pub(crate) const fn calc_image_channel_len_flat(
    width: NonZeroU32,
    height: NonZeroU32,
    channel_size: NonZeroU8,
) -> usize {
    #[allow(clippy::cast_possible_truncation)]
    let width_usize = width.get() as usize;
    #[allow(clippy::cast_possible_truncation)]
    let height_usize = height.get() as usize;

    width_usize * height_usize * channel_size.get() as usize
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
        let image = ImageChannel::<u8>::new_vec(vec![0u8, 64u8, 128u8, 192u8], size, size);
        assert_eq!(image.buffer(), &[0u8, 64u8, 128u8, 192u8]);
    }

    #[test]
    fn miri_to_vec_reuses_pointer() {
        let raw = vec![0u8, 64u8, 128u8, 192u8];
        let pointer = raw[..].as_ptr();
        let size = 2.try_into().unwrap();
        let image = ImageChannel::<u8>::new_vec(raw, size, size);
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
        let mut image = ImageChannel::<u8>::new_arc(raw, size, size);
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
        let mut image = ImageChannel::<u8>::new_arc(raw, size, size);
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
        let image = ImageChannel::<u8>::new_arc(raw, size, size);
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
        let image = ImageChannel::<u8>::new_vec(raw, size, size);
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
            vec![1u16].into(),
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }

    #[test]
    fn miri_test_vec_u16_channel() {
        test_entire_vtable(ImageChannel::<u16>::new_vec(
            vec![1u16],
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }

    #[test]
    fn miri_test_vec_rgb16_channel() {
        test_entire_vtable(ImageChannel::<[u16; 3]>::new_vec(
            vec![[1u16, 2u16, 3u16]],
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }

    fn test_entire_vtable<TP: RuntimePixelType>(mut image: ImageChannel<TP>)
    where
        TP::Primitive: 'static + Default + Eq + Debug,
    {
        image.primitive_make_mut()[0] = TP::Primitive::default();
        let clone = image.clone();
        assert_eq!(image.primitive_make_mut()[0], TP::Primitive::default());
        image.primitive_make_mut()[0] = TP::Primitive::default();

        assert_eq!(image, clone);
    }
}
