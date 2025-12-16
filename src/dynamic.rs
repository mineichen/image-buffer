use std::{fmt::Debug, mem::MaybeUninit, num::NonZeroU8};

use crate::{
    Image, ImageChannel, PixelType,
    pixel::{DynamicPixelKind, PixelTypePrimitive, RuntimePixelType},
};

/// Trait that extends `Any` with a method to clone the boxed value.
// trait DynamicImageBackend: std::any::Any + Debug + Send + Sync {
//     fn boxed_clone(&self) -> Box<dyn DynamicImageBackend>;
// }

// impl<T: PixelType, const CHANNELS: usize> DynamicImageBackend for Image<T, CHANNELS> {
//     fn boxed_clone(&self) -> Box<dyn DynamicImageBackend> {
//         Box::new(self.clone())
//     }
// }

/// Image with unknown number of channels and their types
///
/// The public interface is designed, so it can be extended to support images, which cannot be represented with Image (e.g. 1 Channel U8 and the other f32) in the future
/// It currently only allows casting back to Image to access the data
#[derive(Debug, Clone)]
pub struct DynamicImage {
    channels: Vec<DynamicImageChannel>,
}

#[derive(Debug, Clone)]
pub enum DynamicImageChannel {
    U8(ImageChannel<RuntimePixelType<u8>>),
    U16(ImageChannel<RuntimePixelType<u16>>),
    F32(ImageChannel<RuntimePixelType<f32>>),
}

impl DynamicImage {
    // pub fn channel_infos(&self) -> impl Iterator<Item = (NonZeroU8, DynamicPixelKind)> {
    //     (0..self.layout.buffer_dimensions.get())
    //         .map(|_| (self.layout.pixel_dimensions, self.layout.pixel_kind))
    // }

    // pub fn buffer_dimensions(&self) -> NonZeroUsize {
    //     self.layout.buffer_dimensions
    // }
}

impl<TPixel: PixelType + Send + Sync + Clone, const CHANNELS: usize> From<Image<TPixel, CHANNELS>>
    for DynamicImage
{
    fn from(value: Image<TPixel, CHANNELS>) -> Self {
        DynamicImage {
            channels: value
                .0
                .into_iter()
                .map(|channel| {
                    let runtime_channel = channel.into_runtime();
                    DynamicImageChannel::from(
                        <TPixel::Primitive as PixelTypePrimitive>::into_runtime_channel(
                            ImageChannel::<TPixel::Primitive>::from_runtime_wrapper(
                                runtime_channel,
                            ),
                        ),
                    )
                })
                .collect(),
        }
    }
}

#[non_exhaustive]
#[derive(Debug)]
#[allow(dead_code)]
pub struct IncompatibleImageError<TInput> {
    pub image: TInput,
    expected: ImageLayout<usize>,
}

impl<T: PixelType, const CHANNELS: usize> TryFrom<DynamicImage> for Image<T, CHANNELS> {
    type Error = IncompatibleImageError<DynamicImage>;

    fn try_from(value: DynamicImage) -> Result<Self, Self::Error> {
        let mut same_type = value
            .channels
            .into_iter()
            .filter_map(|c| {
                <T::Primitive as PixelTypePrimitive>::try_from_dynamic_image(c)?
                    .try_into_comptime::<T>()
            })
            .fuse();

        let mut count_ok = 0;

        let all: [_; CHANNELS] = std::array::from_fn(|_| {
            if let Some(channel) = same_type.next() {
                count_ok += 1;
                MaybeUninit::new(channel)
            } else {
                MaybeUninit::uninit()
            }
        });

        if count_ok != CHANNELS {
            todo!("Implement cleanup");
        }

        Ok(Image(all.map(|x| unsafe { x.assume_init() })))
    }
}

/// This is a temporary workaround until VTables are per layer instead of per image
// impl<'a, T: PixelType + Send + Sync + Clone, const CHANNELS: usize> TryFrom<&'a DynamicImage>
//     for &'a Image<T, CHANNELS>
// {
//     type Error = IncompatibleImageError<&'a DynamicImage>;

//     fn try_from(value: &'a DynamicImage) -> Result<Self, Self::Error> {
//         (value.channels.as_ref() as &dyn std::any::Any)
//             .downcast_ref::<Image<T, CHANNELS>>()
//             .ok_or(IncompatibleImageError {
//                 image: value,
//                 expected: ImageLayout {
//                     pixel_dimensions: T::PIXEL_CHANNELS,
//                     pixel_kind: T::KIND,
//                     buffer_dimensions: CHANNELS,
//                 },
//             })
//     }
// }

#[derive(Copy, Debug, Clone)]
struct ImageLayout<T> {
    pub pixel_dimensions: NonZeroU8,
    pub pixel_kind: DynamicPixelKind,
    pub buffer_dimensions: T,
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use crate::LumaImage;

    use super::*;

    #[test]
    fn create_from_luma_u8() {
        let luma = LumaImage::<u8>::new_vec(vec![1], NonZeroU32::MIN, NonZeroU32::MIN);
        let dynamic = DynamicImage::from(luma);
        assert_eq!(1, dynamic.channels.len());
        let luma_back: LumaImage<u8> = dynamic.try_into().unwrap();
        assert_eq!(luma_back.into_vec(), vec![1]);
    }

    #[test]
    fn create_from_luma_rgb16_interleaved() {
        let luma = LumaImage::new_vec(vec![[1u16, 2, 3]], NonZeroU32::MIN, NonZeroU32::MIN);
        let dynamic = DynamicImage::from(luma);
        assert_eq!(1, dynamic.channels.len());
        let luma_back: LumaImage<[u16; 3]> = dynamic.try_into().unwrap();
        assert_eq!(luma_back.into_vec(), vec![[1u16, 2, 3]]);
    }

    #[test]
    fn create_from_rgb8_interleaved() {
        let rgb = Image::<[u8; 3], 1>::new_vec(vec![[1u8, 2, 3]], NonZeroU32::MIN, NonZeroU32::MIN);
        let dynamic = DynamicImage::from(rgb);
        assert_eq!(1, dynamic.channels.len());
        // assert_eq!(
        //     vec![(const { NonZeroU8::new(3).unwrap() }, DynamicPixelKind::U8)],
        //     dynamic.channel_infos().collect::<Vec<_>>()
        // );
        let rgb_back: Image<[u8; 3], 1> = dynamic.try_into().unwrap();
        assert_eq!(rgb_back.into_vec(), vec![[1u8, 2, 3]]);
    }
    #[test]
    fn create_from_luma_rgb8_planar() {
        let luma = Image::<u8, 3>::new_vec(vec![1u8, 2, 3], NonZeroU32::MIN, NonZeroU32::MIN);
        let dynamic = DynamicImage::from(luma);
        assert_eq!(3, dynamic.channels.len());
        let luma_back: Image<u8, 3> = dynamic.try_into().unwrap();
        assert_eq!(luma_back.into_vec(), vec![1u8, 2, 3]);
    }

    // #[test]
    // fn clone_dynamic_image() {
    //     let width = NonZeroU32::new(2).unwrap();
    //     let height = NonZeroU32::new(2).unwrap();
    //     let luma = LumaImage::<u8>::new_vec(vec![1, 2, 3, 4], width, height);
    //     let dynamic = DynamicImage::from(luma);
    //     let cloned = dynamic.clone();

    //     // Verify both can be converted back to the same image
    //     let luma_back: LumaImage<u8> = dynamic.try_into().unwrap();
    //     {
    //         let ref_luma: &Image<u8, 1> = (&cloned).try_into().unwrap();
    //         assert_eq!(ref_luma.dimensions(), (width, height));
    //     }
    //     let luma_cloned: LumaImage<u8> = cloned.try_into().unwrap();
    //     let vec_back = luma_back.into_vec();
    //     let vec_cloned = luma_cloned.into_vec();
    //     assert_eq!(vec_back, vec_cloned);
    //     assert_eq!(vec_cloned, vec![1, 2, 3, 4]);
    // }
}
