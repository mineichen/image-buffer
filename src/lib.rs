#![doc = include_str!("../README.md")]

use std::{
    fmt::{self, Debug, Formatter},
    num::NonZeroU32,
};

use pixel::PixelType;

mod arc;
mod channel;
mod dynamic;
mod pixel;
mod shared_vec;
mod vec;

pub use channel::ImageChannel;
//pub use dynamic::{DynamicImage, IncompatibleImageError};

use crate::{channel::ComptimeChannelSize, pixel::PixelTypePrimitive};
//pub use pixel::PixelType;

pub type LumaImage<T> = Image<T, 1>;
pub type RgbImageInterleaved<T> = Image<[T; 3], 1>;
pub type RgbaImageInterleaved<T> = Image<[T; 4], 1>;
pub type RgbImagePlanar<T> = Image<T, 3>;
pub type RgbaImagePlanar<T> = Image<T, 4>;

#[derive(Clone)]
#[repr(transparent)]
pub struct Image<T: PixelType, const CHANNELS: usize>(
    [ImageChannel<T::Primitive, T::ChannelSize>; CHANNELS],
);

impl<T: PixelType, const CHANNELS: usize> PartialEq for Image<T, CHANNELS> {
    fn eq(&self, other: &Self) -> bool {
        self.0.iter().zip(other.0.iter()).all(|(a, b)| a == b)
    }
}

#[allow(clippy::len_without_is_empty)]
impl<const CHANNELS: usize, T: PixelType> Image<T, CHANNELS> {
    pub fn new_vec(mut input: Vec<T>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        assert_non_zero_channels::<CHANNELS>();
        assert_eq!(
            input.len(),
            width.get() as usize * height.get() as usize * CHANNELS,
            "Incompatible Buffer-Size"
        );

        let ptr = input.as_mut_ptr();
        let len = input.len();
        let cap = input.capacity();

        let ptr = ptr as *mut T::Primitive;
        let len = len * T::PIXEL_CHANNELS.get() as usize;
        let cap = cap * T::PIXEL_CHANNELS.get() as usize;
        std::mem::forget(input);

        // Safety: T::Primitive is expected to be a aligned fraction of T...
        let cast_input = unsafe { Vec::from_raw_parts(ptr, len, cap) };

        if CHANNELS == 1 {
            let channel =
                ImageChannel::new_vec(cast_input, width, height, T::ChannelSize::default());
            unsafe {
                let mut arr = std::mem::MaybeUninit::<
                    [ImageChannel<T::Primitive, T::ChannelSize>; CHANNELS],
                >::uninit();
                std::ptr::write(
                    arr.as_mut_ptr() as *mut ImageChannel<T::Primitive, T::ChannelSize>,
                    channel,
                );
                Self(arr.assume_init())
            }
        } else {
            Self(shared_vec::create_shared_channels(
                cast_input,
                [(width, height, T::ChannelSize::default()); CHANNELS],
            ))
        }
    }

    pub fn into_channels(self) -> [ImageChannel<T::Primitive, T::ChannelSize>; CHANNELS] {
        self.0
    }

    pub const fn len(&self) -> usize {
        let (width, height) = self.0[0].dimensions();
        assert!(width.get() <= usize::MAX as u32);
        assert!(height.get() <= usize::MAX as u32);

        width.get() as usize * height.get() as usize
    }

    pub fn buffers(&self) -> [&[T]; CHANNELS] {
        std::array::from_fn(|i| {
            let buf = self.0[i].buffer();
            unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const T, self.len()) }
        })
    }

    pub fn make_mut(&mut self) -> [&mut [T]; CHANNELS] {
        let mut iter = self.0.iter_mut();
        std::array::from_fn(|_| {
            let buf = iter.next().unwrap().make_mut();
            unsafe {
                std::slice::from_raw_parts_mut(
                    buf.as_mut_ptr() as *mut T,
                    buf.len() / T::PIXEL_CHANNELS.get() as usize,
                )
            }
        })
    }

    pub fn into_vec(mut self) -> Vec<T>
    where
        T: Clone,
    {
        if CHANNELS == 1 {
            // For single channel, use ImageChannel::into_vec which preserves pointer reuse
            // SAFETY: When CHANNELS == 1, we know the array has exactly one element
            let ch_ptr = self.0.as_mut_ptr();
            std::mem::forget(self);
            let channel = unsafe { std::ptr::read(ch_ptr) };

            let mut vec = channel.into_vec();

            let len = vec.len();
            let cap = vec.capacity();
            let ptr = vec.as_mut_ptr() as *mut T;
            let len = len / T::PIXEL_CHANNELS.get() as usize;
            let cap = cap / T::PIXEL_CHANNELS.get() as usize;
            std::mem::forget(vec);

            unsafe { Vec::from_raw_parts(ptr, len, cap) }
        } else {
            // For multiple channels, concatenate them
            let mut result =
                Vec::with_capacity(self.len() * CHANNELS * T::PIXEL_CHANNELS.get() as usize);
            for channel in self.0 {
                let buf = channel.buffer();
                let slice = unsafe {
                    std::slice::from_raw_parts(
                        buf.as_ptr() as *const T,
                        buf.len() / T::PIXEL_CHANNELS.get() as usize,
                    )
                };
                result.extend_from_slice(slice);
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

    // pub fn from_interleaved(i: &Image<T, CHANNELS>) -> Self
    // where
    //     T: PixelType,

    // {
    //     let (width, height) = i.dimensions();
    //     Self::from_flat_interleaved(i.flat_buffer(), (width, height))
    // }

    // pub fn from_flat_interleaved(v: &[T], (width, height): (NonZeroU32, NonZeroU32)) -> Self
    // where
    //     T: Copy,
    // {
    //     let len = width.get() as usize * height.get() as usize;
    //     if CHANNELS == 1 {
    //         return Self::new_vec(v.to_vec(), width, height);
    //     }

    //     assert_non_zero_channels::<CHANNELS>();
    //     assert_eq!(v.len(), len * CHANNELS);
    //     let mut write_buf_container = vec![MaybeUninit::<T>::uninit(); len * CHANNELS];

    //     let mut next_read = 0;

    //     let area = (width.get() * height.get()) as usize;
    //     let write_offsets: [_; CHANNELS] = std::array::from_fn(|i| i * area);

    //     for channel in 0..len {
    //         for (i, write_offset) in write_offsets.iter().enumerate() {
    //             unsafe {
    //                 write_buf_container
    //                     .get_unchecked_mut(channel + write_offset)
    //                     .write(*v.get_unchecked(next_read + i));
    //             }
    //         }
    //         next_read += CHANNELS;
    //     }
    //     let x = unsafe { std::mem::transmute::<Vec<MaybeUninit<T>>, Vec<T>>(write_buf_container) };
    //     Image::<T, CHANNELS>::new_vec(x, width, height)
    // }
}

impl<T> Image<T, 1>
where
    T: PixelType<ChannelSize = ComptimeChannelSize<1>>,
{
    pub fn buffer(&self) -> &[T] {
        let buf = self.0[0].buffer();

        unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const T, self.len()) }
    }
}

fn assert_non_zero_channels<const CHANNELS: usize>() {
    let _ = const {
        if CHANNELS == 0 {
            panic!("Image must have at least one channel");
        }
    };
}

impl<const PIXEL_CHANNELS: usize, T: PixelTypePrimitive> Image<[T; PIXEL_CHANNELS], 1> {
    pub fn flat_buffer(&self) -> &[T] {
        &self.0[0].buffer()
    }

    // pub fn from_planar_image(i: &Image<T, CHANNELS>) -> Self {
    //     let (width, height) = i.dimensions();
    //     Self::from_planar(i.buffers(), width, height)
    // }

    // pub fn from_planar(channels: [&[T]; CHANNELS], width: NonZeroU32, height: NonZeroU32) -> Self {
    //     if CHANNELS == 1 {
    //         let flat_buffer = unsafe {
    //             std::slice::from_raw_parts(
    //                 channels[0].as_ptr() as *const T,
    //                 channels[0].len() * CHANNELS,
    //             )
    //         };
    //         let channel = ImageChannel::new_vec(
    //             flat_buffer.to_vec(),
    //             width,
    //             height,
    //             ComptimeChannelSize::<CHANNELS>::default(),
    //         );

    //         return {
    //             let mut arr = std::mem::MaybeUninit::<[ImageChannel<[T; CHANNELS]>; 1]>::uninit();
    //             unsafe {
    //                 std::ptr::write(arr.as_mut_ptr() as *mut ImageChannel<T>, channel);
    //                 Self(arr.assume_init())
    //             }
    //         };
    //     }
    //     assert_non_zero_channels::<CHANNELS>();

    //     let len = width.get() as usize * height.get() as usize;
    //     let mut channels = channels.map(|c| c.iter());

    //     let mut data = Arc::new_uninit_slice(len);
    //     let data_ptr = Arc::get_mut(&mut data).unwrap();
    //     for dst in data_ptr {
    //         let mut value = [MaybeUninit::<T>::uninit(); CHANNELS];

    //         for (src, dst) in channels
    //             .iter_mut()
    //             .map(|c| c.next().unwrap())
    //             .zip(value.iter_mut())
    //         {
    //             dst.write(*src);
    //         }

    //         dst.write(value.map(|x| unsafe { x.assume_init() }));
    //     }
    //     let data = unsafe { data.assume_init() };

    //     let image = ImageChannel::new_arc(data, width, height);
    //     Self([image])
    // }
}

impl<T: PixelType, const CHANNELS: usize> Debug for Image<T, CHANNELS> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Image")
            .field("width", &self.width())
            .field("height", &self.height())
            .field("channels", &CHANNELS)
            .field("pixel", &std::any::type_name::<T>())
            .finish()
    }
}

// impl<'a, T: PixelType> From<&'a Image<T, 1>> for (&'a [T], NonZeroU32, NonZeroU32) {
//     fn from(that: &'a LumaImage<T>) -> Self {
//         let (width, height) = that.dimensions();
//         let buf = that.buffer();
//         (buf, width, height)
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miri_create_and_clear_vec_image() {
        let size = 2.try_into().unwrap();
        let image = LumaImage::new_vec(vec![0u8, 64u8, 128u8, 192u8], size, size);
        assert_eq!(image.buffers()[0], &[0u8, 64u8, 128u8, 192u8]);
    }

    // #[test]
    // fn from_planar_image() {
    //     let two = NonZeroU32::new(2).unwrap();
    //     let image = RgbImagePlanar::new_vec((0..12).collect(), two, two);
    //     let interleaved_image = Image::from_planar_image(&image);
    //     assert_eq!(
    //         interleaved_image.buffer(),
    //         &[[0u8, 4, 8], [1, 5, 9,], [2, 6, 10,], [3, 7, 11]]
    //     );
    //     assert_eq!(interleaved_image.dimensions(), (two, two));
    // }

    // #[test]
    // fn luma_from_planar() {
    //     let two = NonZeroU32::new(2).unwrap();
    //     let image = LumaImage::new_vec(vec![0u8, 64u8, 128u8, 192u8], two, two);
    //     let planar_image = Image::from_planar_image(&image);
    //     assert_eq!(planar_image.buffer(), [[0u8], [64u8], [128u8], [192u8]]);
    // }

    // #[test]
    // fn luma_from_interleaved() {
    //     let two = NonZeroU32::new(2).unwrap();
    //     let interleaved_image =
    //         LumaImage::from_flat_interleaved(&[0u8, 64u8, 128u8, 192u8], (two, two));
    //     assert_eq!(interleaved_image.buffers(), [[0u8, 64u8, 128u8, 192u8]]);
    //     assert_eq!(interleaved_image.dimensions(), (two, two));
    // }
    // #[test]
    // fn from_flat_interleaved_image() {
    //     let two = NonZeroU32::new(2).unwrap();
    //     let image: RgbImagePlanar<u8> =
    //         Image::from_flat_interleaved((0..12).collect::<Vec<_>>().as_slice(), (two, two));
    //     assert_eq!(
    //         image.buffers(),
    //         [[0u8, 3, 6, 9], [1, 4, 7, 10], [2, 5, 8, 11]]
    //     );
    //     assert_eq!(image.dimensions(), (two, two));
    // }

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
}
