//! #Coordinate System
//! There are different interpretations of x and y in imageprocessing. Most work with the top left corner as (0,0)
//! - Web: x=col, y=row https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/translate
//! - Halcon-Image: x=row + 0.5, y=col + 0.5
//! - Matlab: x=col + 0.5, y=row + 0.5 https://ch.mathworks.com/help/images/image-coordinate-systems.html
//!
//! There are just a few exceptions starting in the lower left corner:
//! - OpenGL: x=col, y=row
//!
//! Pilatus is choosing top left corner (0,0) with x=col, y=row semantics for the following reasons:
//! - All processing-libraries use the top-left corner
//! - Z Points away from the plane. Dept images would have negative pixelvalues otherwise.
//! - Horizontal is commonly described by X and vertical by Y, which leads to less confusion
//! - We need two different formats (web and halcon) anyway. Conversions cannot be avoided
//! - x=row y=col is more widely used in the analyzed examples
//!
//! # Genericity
//! Gray images can easily be shared as Arc<GenericImage<1>>, as there is no confusion how the pixels are aligned
//! Color images are shared via Arc<dyn RgbImage>,
use std::{
    fmt::{self, Debug, Formatter},
    mem::MaybeUninit,
    num::NonZeroU32,
    sync::Arc,
};

mod arc;
mod vec;

pub type LumaImage<T> = GenericImage<T, 1>;
pub type RgbImageInterleaved<T> = GenericImage<[T; 3], 1>;
pub type RgbaImageInterleaved<T> = GenericImage<[T; 4], 1>;
pub type RgbImagePlanar<T> = GenericImage<T, 3>;
pub type RgbaImagePlanar<T> = GenericImage<T, 4>;

#[repr(transparent)]
pub struct GenericImage<T: 'static, const CHANNELS: usize>(UnsafeGenericImage<T, CHANNELS>);

impl<T, const CHANNELS: usize> Clone for GenericImage<T, CHANNELS> {
    fn clone(&self) -> Self {
        Self(unsafe { (self.0.vtable.clone)(&self.0) })
    }
}

// Todo: Fixme, this is not correct
impl<T: std::cmp::PartialEq, const CHANNELS: usize> PartialEq for GenericImage<T, CHANNELS> {
    fn eq(&self, other: &Self) -> bool {
        self.0.width == other.0.width
            && self.0.height == other.0.height
            && self.buffers() == other.buffers()
    }
}

#[allow(clippy::len_without_is_empty)]
impl<const CHANNELS: usize, T: 'static> GenericImage<T, CHANNELS> {
    #[deprecated = "Use eigher new_vec or new_arc"]
    pub fn new(input: Vec<T>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        Self::new_vec(input, width, height)
    }

    pub fn new_vec(input: Vec<T>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        Self(UnsafeGenericImage::new_vec(input, width, height))
    }

    pub fn new_arc(input: Arc<[T]>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        Self(UnsafeGenericImage::new_arc(input, width, height))
    }

    /// Don't use this method unless you need a custom image.
    ///
    /// Use/provide methods like new_vec() and new_arc() for safe construction
    ///
    /// # Safety
    /// The vtable must be able to cleanup the fields
    pub unsafe fn new_with_vtable(
        ptrs: [*const T; CHANNELS],
        width: NonZeroU32,
        height: NonZeroU32,
        vtable: &'static ImageVtable<T, CHANNELS>,
        generic_field: usize,
    ) -> Self
    where
        T: Send + Sync,
    {
        Self(UnsafeGenericImage::new_with_vtable(
            ptrs,
            width,
            height,
            vtable,
            generic_field,
        ))
    }

    pub const fn len(&self) -> usize {
        assert!(self.0.width.get() <= usize::MAX as u32);
        assert!(self.0.height.get() <= usize::MAX as u32);
        self.0.width.get() as usize * self.0.height.get() as usize * CHANNELS
    }

    pub const fn buffers(&self) -> [&[T]; CHANNELS] {
        let len_per_channel = self.0.width.get() as usize * self.0.height.get() as usize;
        let mut result = [[].as_slice(); CHANNELS];
        let mut i = 0;
        while i < CHANNELS {
            result[i] = unsafe { std::slice::from_raw_parts(self.0.ptrs[i], len_per_channel) };
            i += 1;
        }
        result
    }

    pub fn make_mut(&mut self) -> &mut [T] {
        unsafe {
            let ptr = (self.0.vtable.make_mut)(&mut self.0);
            let len = self.len();
            std::slice::from_raw_parts_mut(ptr, len)
        }
    }

    pub fn into_vec(self) -> Vec<T>
    where
        T: Clone,
    {
        if self.0.vtable.drop as usize == vec::clear_vec::<T, CHANNELS> as usize {
            let size = self.len();
            let result =
                unsafe { Vec::from_raw_parts(self.0.ptrs[0] as *mut _, size, self.0.data) };
            std::mem::forget(self);
            result
        } else {
            let buffers = self.buffers();
            let mut result = Vec::with_capacity(self.len());
            for buf in buffers {
                result.extend_from_slice(buf);
            }
            result
        }
    }
    pub fn dimensions(&self) -> (NonZeroU32, NonZeroU32) {
        (self.0.width, self.0.height)
    }

    pub fn from_interleaved(i: &GenericImage<[T; CHANNELS], 1>) -> Self
    where
        T: Copy,
    {
        let (width, height) = i.dimensions();
        Self::from_flat_interleaved(i.flat_buffer(), (width, height))
    }

    pub fn from_flat_interleaved(v: &[T], (width, height): (NonZeroU32, NonZeroU32)) -> Self
    where
        T: Copy,
    {
        let len = width.get() as usize * height.get() as usize;
        let mut write_buf_container = Arc::new_uninit_slice(len * CHANNELS);
        let write_buf = Arc::get_mut(&mut write_buf_container).unwrap();
        let mut next_read = 0;

        let area = (width.get() * height.get()) as usize;
        let write_offsets: [_; CHANNELS] = std::array::from_fn(|i| i * area);

        for channel in 0..len {
            for (i, write_offset) in write_offsets.iter().enumerate() {
                unsafe {
                    write_buf
                        .get_unchecked_mut(channel + write_offset)
                        .write(*v.get_unchecked(next_read + i));
                }
            }
            next_read += CHANNELS;
        }
        GenericImage::<T, CHANNELS>::new_arc(
            unsafe { write_buf_container.assume_init() },
            width,
            height,
        )
    }
}

impl<T> GenericImage<T, 1> {
    pub const fn buffer(&self) -> &[T] {
        let len = self.0.width.get() as usize * self.0.height.get() as usize;
        unsafe { std::slice::from_raw_parts(self.0.ptrs[0], len) }
    }
}

impl<const CHANNELS: usize, T: Copy> GenericImage<[T; CHANNELS], 1> {
    pub fn flat_buffer(&self) -> &[T] {
        // SAFETY: [u8; 3] has the same layout as 3 consecutive u8 values
        unsafe {
            std::slice::from_raw_parts(
                self.buffers()[0].as_ptr() as *const T,
                self.len() * CHANNELS,
            )
        }
    }

    pub fn from_planar_image(i: &GenericImage<T, CHANNELS>) -> Self {
        let (width, height) = i.dimensions();
        Self::from_planar(i.buffers(), width, height)
    }

    pub fn from_planar(channels: [&[T]; CHANNELS], width: NonZeroU32, height: NonZeroU32) -> Self {
        let len = width.get() as usize * height.get() as usize;
        let mut channels = channels.map(|c| c.iter());

        let mut data = Arc::new_uninit_slice(len);
        let data_ptr = Arc::get_mut(&mut data).unwrap();
        for i in 0..len {
            let mut value = [MaybeUninit::<T>::uninit(); CHANNELS];

            for (src, dst) in channels
                .iter_mut()
                .map(|c| c.next().unwrap())
                .zip(value.iter_mut())
            {
                dst.write(*src);
            }

            data_ptr[i].write(value.map(|x| unsafe { x.assume_init() }));
        }
        let data = unsafe { data.assume_init() };

        GenericImage::<[T; CHANNELS], 1>::new_arc(data, width, height)
    }
}

#[repr(C)]
pub struct ImageVtable<T: 'static, const CHANNELS: usize> {
    pub clone:
        unsafe extern "C" fn(&UnsafeGenericImage<T, CHANNELS>) -> UnsafeGenericImage<T, CHANNELS>,
    pub make_mut: unsafe extern "C" fn(&mut UnsafeGenericImage<T, CHANNELS>) -> *mut T,
    pub drop: unsafe extern "C" fn(&mut UnsafeGenericImage<T, CHANNELS>),
}

impl<TP: std::any::Any, const CHANNELS: usize> Debug for GenericImage<TP, CHANNELS> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("GenericImage")
            .field("width", &self.0.width)
            .field("height", &self.0.height)
            .field("channels", &CHANNELS)
            .field("pixel", &std::any::type_name::<TP>())
            .finish()
    }
}

unsafe impl<TP: Send, const T: usize> Send for GenericImage<TP, T> {}
unsafe impl<TP: Sync, const T: usize> Sync for GenericImage<TP, T> {}

impl<'a, T> From<&'a GenericImage<T, 1>> for (&'a [T], NonZeroU32, NonZeroU32) {
    fn from(that: &'a LumaImage<T>) -> Self {
        let (width, height) = that.dimensions();
        let buf = that.buffer();
        (buf, width, height)
    }
}

#[repr(C)]
pub struct UnsafeGenericImage<T: 'static, const CHANNELS: usize> {
    pub ptrs: [*const T; CHANNELS],
    pub width: NonZeroU32,
    pub height: NonZeroU32,
    pub vtable: &'static ImageVtable<T, CHANNELS>,
    // Has to be cleaned up by clear proc too
    pub data: usize,
}
impl<const CHANNELS: usize, T: 'static> UnsafeGenericImage<T, CHANNELS> {
    /// Don't use this method unless you need a custom image.
    ///
    /// Use/provide methods like new_vec() and new_arc() for safe construction
    ///
    /// # Safety
    /// The vtable must be able to cleanup the fields
    pub unsafe fn new_with_vtable(
        ptrs: [*const T; CHANNELS],
        width: NonZeroU32,
        height: NonZeroU32,
        vtable: &'static ImageVtable<T, CHANNELS>,
        generic_field: usize,
    ) -> Self {
        assert!(matches!(CHANNELS, 1 | 3 | 4));

        UnsafeGenericImage {
            ptrs,
            width,
            height,
            vtable,
            data: generic_field,
        }
    }
}

impl<T, const CHANNELS: usize> Drop for UnsafeGenericImage<T, CHANNELS> {
    fn drop(&mut self) {
        if self.ptrs[0] as usize != 0 {
            unsafe { (self.vtable.drop)(self) };
        }
    }
}

// Workaroung inability to have static which uses Outer Generics
trait Factory<T: 'static, const CHANNELS: usize> {
    const VTABLE: &'static ImageVtable<T, CHANNELS>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miri_create_and_clear_vec_image() {
        let size = 2.try_into().unwrap();
        let image = LumaImage::new_vec(vec![0u8, 64u8, 128u8, 192u8], size, size);
        assert_eq!(image.buffers()[0], &[0u8, 64u8, 128u8, 192u8]);
    }
    #[test]
    fn miri_to_vec_reuses_pointer() {
        let raw = vec![0u8, 64u8, 128u8, 192u8];
        let pointer = raw[..].as_ptr();
        let size = 2.try_into().unwrap();
        let image = LumaImage::new_vec(raw, size, size);
        let to_vec = image.into_vec();

        // Miri seems to generate clear_vec::<const u8> for each call
        // It works on native x86. Because it's only an optimization, this is good enough
        // VTable is not possible, as GenericImage is ABI-Stable and multiple dylibs use their own allocator for Vecs
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
        let mut image = LumaImage::new_arc(raw, size, size);
        let ptr_mut = image.make_mut();

        assert_eq!(
            ptr_mut[..].as_ptr(),
            pointer,
            "Should reuse the buffer if it was created by vec"
        );
    }

    #[test]
    fn miri_make_mut_doesnt_reuse_arc_pointer_if_not_unique() {
        let raw = Arc::<[u8]>::from([0u8, 64u8, 128u8, 192u8].as_slice());
        let _raw2 = raw.clone();
        let pointer = raw[..].as_ptr();
        let size = 2.try_into().unwrap();
        let mut image = LumaImage::new_arc(raw, size, size);
        let ptr_mut = image.make_mut();

        assert_ne!(
            ptr_mut[..].as_ptr(),
            pointer,
            "Should reuse the buffer if it was created by vec"
        );
    }

    #[test]
    fn miri_clone_arc_backed_shares_memory() {
        let raw = Arc::<[u8]>::from([0u8, 64u8, 128u8, 192u8].as_slice());
        let pointer = raw[..].as_ptr();
        let size = 2.try_into().unwrap();
        let image = LumaImage::new_arc(raw, size, size);
        let image2 = image.clone();

        assert_eq!(
            image2.buffer().as_ptr(),
            pointer,
            "Should reuse the buffer if it was created by vec"
        );
    }

    #[test]
    fn miri_clone_from_box() {
        let raw = vec![0u8, 64u8, 128u8, 192u8];
        let size = 2.try_into().unwrap();
        let image = LumaImage::new_vec(raw, size, size);
        let image2 = image.clone();
        let to_vec = image.into_vec();
        let to_vec2 = image2.into_vec();

        assert_ne!(
            to_vec[..].as_ptr(),
            to_vec2[..].as_ptr(),
            "Should reuse the buffer if it was created by vec"
        );
    }

    #[test]
    fn miri_test_shared_arc_u16_luma() {
        let arc: Arc<[u16]> = vec![1].into();
        test_entire_vtable(GenericImage::<u16, 1>::new_arc(
            arc,
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }
    #[test]
    fn miri_test_exclusive_arc_u16_luma() {
        test_entire_vtable(GenericImage::<u16, 1>::new_arc(
            vec![1].into(),
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }
    #[test]
    fn miri_test_vec_u16_luma() {
        test_entire_vtable(GenericImage::<u16, 1>::new_vec(
            vec![1],
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }

    fn test_entire_vtable<T: 'static + Default + Eq, const SIZE: usize>(
        mut image: GenericImage<T, SIZE>,
    ) {
        image.make_mut()[0] = T::default();
        let mut clone = image.clone();
        clone.make_mut()[0] = T::default();
        assert_eq!(image, clone);
    }
}
