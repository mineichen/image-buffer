#![doc = include_str!("../README.md")]
use std::{
    fmt::{self, Debug, Formatter},
    mem::MaybeUninit,
    num::NonZeroU32,
    sync::Arc,
};

mod arc;
mod channel;
mod dynamic;
mod shared_vec;
mod vec;

pub use channel::ImageChannel;
pub use dynamic::{DynamicImage, DynamicPixelKind, IncompatibleImageError};

pub type LumaImage<T> = Image<T, 1>;
pub type RgbImageInterleaved<T> = Image<[T; 3], 1>;
pub type RgbaImageInterleaved<T> = Image<[T; 4], 1>;
pub type RgbImagePlanar<T> = Image<T, 3>;
pub type RgbaImagePlanar<T> = Image<T, 4>;

#[derive(Clone)]
pub struct Image<T: 'static, const CHANNELS: usize>([ImageChannel<T>; CHANNELS]);

impl<T: std::cmp::PartialEq, const CHANNELS: usize> PartialEq for Image<T, CHANNELS> {
    fn eq(&self, other: &Self) -> bool {
        self.0.iter().zip(other.0.iter()).all(|(a, b)| a == b)
    }
}

#[allow(clippy::len_without_is_empty)]
impl<const CHANNELS: usize, T: 'static> Image<T, CHANNELS> {
    #[deprecated = "Use eigher new_vec or new_arc"]
    pub fn new(input: Vec<T>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        let _assert_not_zero = const { CHANNELS.checked_sub(1).unwrap() };
        if CHANNELS == 1 {
            Image::new_arc(input.into(), width, height)
        } else {
            Image::new_vec(input.to_vec(), width, height)
        }
    }

    pub fn new_vec(input: Vec<T>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        let _assert_not_zero = const { CHANNELS.checked_sub(1).unwrap() };
        assert_eq!(
            input.len(),
            width.get() as usize * height.get() as usize * CHANNELS,
            "Incompatible Buffer-Size"
        );

        if CHANNELS == 1 {
            // For single channel, use Vec directly to preserve pointer reuse
            let channel = ImageChannel::new_vec(input, width, height);
            unsafe {
                let mut arr = std::mem::MaybeUninit::<[ImageChannel<T>; CHANNELS]>::uninit();
                std::ptr::write(arr.as_mut_ptr() as *mut ImageChannel<T>, channel);
                Self(arr.assume_init())
            }
        } else {
            Self(shared_vec::create_shared_channels(
                input,
                [(width, height); CHANNELS],
            ))
        }
    }

    pub fn new_arc(input: Arc<[T]>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        let _ = const { CHANNELS.checked_sub(1).unwrap() };
        let len_per_channel = (width.get() * height.get()) as usize;
        assert_eq!(
            input.len(),
            len_per_channel * CHANNELS,
            "Incompatible Buffer-Size"
        );

        // Create CHANNELS ImageChannels, each pointing to a different slice of the same Arc
        if CHANNELS == 1 {
            // For single channel, use the Arc directly to preserve pointer
            let channel = ImageChannel::new_arc(input, width, height);
            // SAFETY: When CHANNELS == 1, we know the array has exactly one element
            unsafe {
                let mut arr = std::mem::MaybeUninit::<[ImageChannel<T>; CHANNELS]>::uninit();
                std::ptr::write(arr.as_mut_ptr() as *mut ImageChannel<T>, channel);
                Self(arr.assume_init())
            }
        } else {
            // For multiple channels, create slices
            let channels = std::array::from_fn(|i| {
                let start = i * len_per_channel;
                let end = start + len_per_channel;
                let slice = Arc::clone(&input);
                // Create a new Arc pointing to the slice
                let channel_slice: Arc<[T]> = Arc::from(&slice[start..end]);
                ImageChannel::new_arc(channel_slice, width, height)
            });
            Self(channels)
        }
    }

    pub const fn len(&self) -> usize {
        if CHANNELS > 0 { self.0[0].len() } else { 0 }
    }

    pub fn buffers(&self) -> [&[T]; CHANNELS] {
        std::array::from_fn(|i| self.0[i].buffer())
    }

    pub fn make_mut(&mut self) -> [&mut [T]; CHANNELS] {
        let mut iter = self.0.iter_mut();
        std::array::from_fn(|_| iter.next().unwrap().make_mut())
    }

    pub fn into_vec(self) -> Vec<T>
    where
        T: Clone,
    {
        if CHANNELS == 1 {
            // For single channel, use ImageChannel::into_vec which preserves pointer reuse
            // SAFETY: When CHANNELS == 1, we know the array has exactly one element
            unsafe {
                let channel = std::ptr::read(self.0.as_ptr());
                std::mem::forget(self);
                channel.into_vec()
            }
        } else {
            // For multiple channels, concatenate them
            let mut result = Vec::with_capacity(self.len() * CHANNELS);
            for channel in self.0 {
                result.extend_from_slice(channel.buffer());
            }
            result
        }
    }

    pub fn width(&self) -> NonZeroU32 {
        // All channels have the same width (validated at construction)
        if CHANNELS > 0 {
            self.0[0].width()
        } else {
            NonZeroU32::MIN
        }
    }

    pub fn height(&self) -> NonZeroU32 {
        // All channels have the same height (validated at construction)
        if CHANNELS > 0 {
            self.0[0].height()
        } else {
            NonZeroU32::MIN
        }
    }

    pub fn dimensions(&self) -> (NonZeroU32, NonZeroU32) {
        // All channels have the same dimensions (validated at construction)
        if CHANNELS > 0 {
            self.0[0].dimensions()
        } else {
            (NonZeroU32::MIN, NonZeroU32::MIN)
        }
    }

    pub fn from_interleaved(i: &Image<[T; CHANNELS], 1>) -> Self
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
        Image::<T, CHANNELS>::new_arc(unsafe { write_buf_container.assume_init() }, width, height)
    }
}

impl<T> Image<T, 1> {
    pub const fn buffer(&self) -> &[T] {
        self.0[0].buffer()
    }
}

impl<const CHANNELS: usize, T: Copy> Image<[T; CHANNELS], 1> {
    pub fn flat_buffer(&self) -> &[T] {
        // SAFETY: [u8; 3] has the same layout as 3 consecutive u8 values
        unsafe {
            std::slice::from_raw_parts(
                self.buffers()[0].as_ptr() as *const T,
                self.len() * CHANNELS,
            )
        }
    }

    pub fn from_planar_image(i: &Image<T, CHANNELS>) -> Self {
        let (width, height) = i.dimensions();
        Self::from_planar(i.buffers(), width, height)
    }

    pub fn from_planar(channels: [&[T]; CHANNELS], width: NonZeroU32, height: NonZeroU32) -> Self {
        let _ = const { CHANNELS.checked_sub(1).unwrap() };

        let len = width.get() as usize * height.get() as usize;
        let mut channels = channels.map(|c| c.iter());

        let mut data: Vec<[T; CHANNELS]> = Vec::with_capacity(len * CHANNELS);
        for _ in 0..len {
            let mut value = [MaybeUninit::<T>::uninit(); CHANNELS];

            for (src, dst) in channels
                .iter_mut()
                .map(|c| c.next().unwrap())
                .zip(value.iter_mut())
            {
                dst.write(*src);
            }

            data.push(value.map(|x| unsafe { x.assume_init() }));
        }

        Image::<[T; CHANNELS], 1>::new_vec(data, width, height)
    }
}

impl<TP: std::any::Any, const CHANNELS: usize> Debug for Image<TP, CHANNELS> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width())
            .field("height", &self.height())
            .field("channels", &CHANNELS)
            .field("pixel", &std::any::type_name::<TP>())
            .finish()
    }
}

unsafe impl<TP: Send, const T: usize> Send for Image<TP, T> {}
unsafe impl<TP: Sync, const T: usize> Sync for Image<TP, T> {}

impl<'a, T> From<&'a Image<T, 1>> for (&'a [T], NonZeroU32, NonZeroU32) {
    fn from(that: &'a LumaImage<T>) -> Self {
        let (width, height) = that.dimensions();
        let buf = that.buffer();
        (buf, width, height)
    }
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
    fn from_planar_image() {
        let two = NonZeroU32::new(2).unwrap();
        let image = RgbImagePlanar::new_vec((0..12).collect(), two, two);
        let planar_image = Image::from_planar_image(&image);
        assert_eq!(
            planar_image.buffer(),
            &[[0u8, 4, 8], [1, 5, 9,], [2, 6, 10,], [3, 7, 11]]
        );
        assert_eq!(planar_image.dimensions(), (two, two));
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
        // VTable is not possible, as Image is ABI-Stable and multiple dylibs use their own allocator for Vecs
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
            ptr_mut[0][..].as_ptr(),
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
            ptr_mut[0][..].as_ptr(),
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
        test_entire_vtable(Image::<u16, 1>::new_arc(
            arc,
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }
    #[test]
    fn miri_test_exclusive_arc_u16_luma() {
        test_entire_vtable(Image::<u16, 1>::new_arc(
            vec![1].into(),
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }
    #[test]
    fn miri_test_vec_u16_luma() {
        test_entire_vtable(Image::<u16, 1>::new_vec(
            vec![1],
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        ));
    }

    fn test_entire_vtable<T: 'static + Default + Eq + Debug + Clone, const SIZE: usize>(
        mut image: Image<T, SIZE>,
    ) {
        for channel in image.make_mut() {
            channel[0] = T::default();
        }
        let clone = image.clone();
        for channel in image.make_mut() {
            assert_eq!(channel[0], T::default());
            channel[0] = T::default();
        }

        assert_eq!(image, clone);
    }
}
