use std::{
    fmt::Debug,
    mem::MaybeUninit,
    num::{NonZeroU8, NonZeroUsize},
};

use crate::{Image, ImageChannel, PixelType, pixel::DynamicSize};

/// Image with number of channels and their types only known at runtime
///
/// The public interface is designed, so it can be extended to support images, which cannot be represented with Image (e.g. 1 Channel U8 and the other f32) in the future
/// It currently only allows casting back to Image to access the data
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicImage {
    channels: Vec<DynamicImageChannel>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DynamicImageChannel {
    U8(ImageChannel<DynamicSize<u8>>),
    U16(ImageChannel<DynamicSize<u16>>),
    F32(ImageChannel<DynamicSize<f32>>),
}

impl<TPixel: PixelType + Send + Sync + Clone, const CHANNELS: usize> From<Image<TPixel, CHANNELS>>
    for DynamicImage
{
    fn from(value: Image<TPixel, CHANNELS>) -> Self {
        DynamicImage {
            channels: value
                .0
                .into_iter()
                .map(ImageChannel::into_runtime)
                .collect(),
        }
    }
}

#[non_exhaustive]
#[derive(Debug)]
pub struct IncompatibleImageError {
    pub image: DynamicImage,
    #[allow(dead_code)]
    pixel_dimensions: NonZeroU8,
    #[allow(dead_code)]
    pixel_kind: &'static str,
    #[allow(dead_code)]
    buffer_dimensions: NonZeroUsize,
}

impl<T: PixelType, const CHANNELS: usize> TryFrom<DynamicImage> for Image<T, CHANNELS> {
    type Error = IncompatibleImageError;

    fn try_from(value: DynamicImage) -> Result<Self, Self::Error> {
        from_image_iter(value.channels.into_iter())
    }
}

impl<'a, T: PixelType + Send + Sync + Clone, const CHANNELS: usize> TryFrom<&'a DynamicImage>
    for Image<T, CHANNELS>
{
    type Error = IncompatibleImageError;

    fn try_from(value: &'a DynamicImage) -> Result<Self, Self::Error> {
        from_image_iter(value.clone().channels.into_iter())
    }
}

fn from_image_iter<T: PixelType, const CHANNELS: usize>(
    mut value: impl Iterator<Item = DynamicImageChannel>,
) -> Result<Image<T, CHANNELS>, IncompatibleImageError> {
    let mut incompatible_image = Ok(());

    let all: [_; CHANNELS] = std::array::from_fn(|i| {
        if incompatible_image.is_ok() {
            incompatible_image = Err(match value.next() {
                Some(dynamic) => match ImageChannel::try_from(dynamic) {
                    Ok(typed) => return MaybeUninit::new(typed),
                    Err(dynamic) => (i, Some(dynamic)),
                },
                None => (i, None),
            })
        }
        MaybeUninit::uninit()
    });
    match incompatible_image {
        Ok(_) => Ok(Image(all.map(|x| unsafe { x.assume_init() }))),
        Err((initialized_indices, error_image)) => Err(IncompatibleImageError {
            image: DynamicImage {
                channels: all
                    .into_iter()
                    .take(initialized_indices)
                    .map(|x| unsafe { x.assume_init() }.into_runtime())
                    .chain(error_image)
                    .chain(value)
                    .collect(),
            },
            pixel_dimensions: T::PIXEL_CHANNELS,
            pixel_kind: std::any::type_name::<T>(),
            buffer_dimensions: const { NonZeroUsize::new(CHANNELS).unwrap() },
        }),
    }
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

    #[test]
    fn clone_dynamic_image() {
        let width = NonZeroU32::new(2).unwrap();
        let height = NonZeroU32::new(2).unwrap();
        let luma = LumaImage::<u8>::new_vec(vec![1, 2, 3, 4], width, height);
        let dynamic = DynamicImage::from(luma);
        let cloned = dynamic.clone();

        // Verify both can be converted back to the same image
        let luma_back: LumaImage<u8> = dynamic.try_into().unwrap();
        {
            let ref_luma: Image<u8, 1> = (&cloned).try_into().unwrap();
            assert_eq!(ref_luma.dimensions(), (width, height));
        }
        let luma_cloned: LumaImage<u8> = cloned.try_into().unwrap();
        let vec_back = luma_back.into_vec();
        let vec_cloned = luma_cloned.into_vec();
        assert_eq!(vec_back, vec_cloned);
        assert_eq!(vec_cloned, vec![1, 2, 3, 4]);
    }

    #[test]
    fn create_from_incompatible_image() {
        let luma = LumaImage::<u8>::new_vec(vec![42], NonZeroU32::MIN, NonZeroU32::MIN);
        let dynamic = DynamicImage::from(luma.clone());
        let incompatible = Image::<u16, 1>::try_from(dynamic).unwrap_err();
        assert_eq!(incompatible.image, DynamicImage::from(luma));
    }
}
