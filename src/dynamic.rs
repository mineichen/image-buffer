use std::{
    fmt::Debug,
    mem::MaybeUninit,
    num::{NonZeroU8, NonZeroU32, NonZeroUsize},
};

use crate::{Image, ImageChannel, PixelType, pixel::DynamicSize};

/// Image with number of channels and their types only known at runtime
///
/// The public interface is designed, so it can be extended to support images, which cannot be represented with Image (e.g. 1 Channel u8 and the other f32)
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicImage {
    channels: Vec<DynamicImageChannel>,
}

impl DynamicImage {
    pub fn from_channels(
        first: DynamicImageChannel,
        rest: impl IntoIterator<Item = DynamicImageChannel>,
    ) -> Self {
        Self {
            channels: std::iter::once(first).chain(rest).collect(),
        }
    }
    /// `DynamicImage` always has at least one channel, so this never panics
    #[must_use]
    pub fn first(&self) -> &DynamicImageChannel {
        &self.channels[0]
    }

    /// `DynamicImage` always has at least one channel, so this never panics
    #[must_use]
    pub fn last(&self) -> &DynamicImageChannel {
        &self.channels[self.len() - 1]
    }
}

// Deref slice only, to make sure, noone can create a DynamicImage with a empty Vec
impl std::ops::Deref for DynamicImage {
    type Target = [DynamicImageChannel];

    fn deref(&self) -> &Self::Target {
        &self.channels
    }
}

impl IntoIterator for DynamicImage {
    type Item = DynamicImageChannel;
    type IntoIter = std::vec::IntoIter<DynamicImageChannel>;

    fn into_iter(self) -> Self::IntoIter {
        self.channels.into_iter()
    }
}

impl std::ops::DerefMut for DynamicImage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.channels
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DynamicImageChannel {
    U8(ImageChannel<DynamicSize<u8>>),
    U16(ImageChannel<DynamicSize<u16>>),
    F32(ImageChannel<DynamicSize<f32>>),
}

impl DynamicImageChannel {
    #[must_use]
    pub fn pixel_elements(&self) -> NonZeroU8 {
        match self {
            DynamicImageChannel::U8(x) => x.pixel_elements(),
            DynamicImageChannel::U16(x) => x.pixel_elements(),
            DynamicImageChannel::F32(x) => x.pixel_elements(),
        }
    }

    #[must_use]
    pub fn dimensions(&self) -> (NonZeroU32, NonZeroU32) {
        match self {
            DynamicImageChannel::U8(x) => x.dimensions(),
            DynamicImageChannel::U16(x) => x.dimensions(),
            DynamicImageChannel::F32(x) => x.dimensions(),
        }
    }

    #[must_use]
    pub fn width(&self) -> NonZeroU32 {
        match self {
            DynamicImageChannel::U8(x) => x.width(),
            DynamicImageChannel::U16(x) => x.width(),
            DynamicImageChannel::F32(x) => x.width(),
        }
    }

    #[must_use]
    pub fn height(&self) -> NonZeroU32 {
        match self {
            DynamicImageChannel::U8(x) => x.height(),
            DynamicImageChannel::U16(x) => x.height(),
            DynamicImageChannel::F32(x) => x.height(),
        }
    }
}

impl<TPixel: PixelType + Send + Sync + Clone, const CHANNELS: usize> From<Image<TPixel, CHANNELS>>
    for DynamicImage
{
    fn from(value: Image<TPixel, CHANNELS>) -> Self {
        DynamicImage {
            channels: <[ImageChannel<TPixel>; CHANNELS]>::from(value)
                .into_iter()
                .map(ImageChannel::into)
                .collect(),
        }
    }
}

#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
#[error("IncompatibleImageError: {image:?} {reason:?}")]
pub struct IncompatibleImageError<TInput> {
    pub image: TInput,
    #[allow(dead_code)]
    pub(crate) reason: IncompatibleImageErrorReason,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum IncompatibleImageErrorReason {
    Comptime {
        pixel_dimensions: NonZeroU8,
        pixel_kind: &'static str,
        buffer_dimensions: NonZeroUsize,
    },
    MixedImageSizes {
        a: (NonZeroU32, NonZeroU32),
        b: (NonZeroU32, NonZeroU32),
    },
    RequiresMoreChannels {
        expected: NonZeroU8,
        actual: NonZeroU8,
    },
}

impl<T: PixelType, const CHANNELS: usize> TryFrom<DynamicImage> for Image<T, CHANNELS> {
    type Error = IncompatibleImageError<DynamicImage>;

    fn try_from(value: DynamicImage) -> Result<Self, Self::Error> {
        from_image_iter(value.channels.into_iter())
    }
}

impl<'a, T: PixelType + Send + Sync + Clone, const CHANNELS: usize> TryFrom<&'a DynamicImage>
    for Image<T, CHANNELS>
{
    type Error = IncompatibleImageError<DynamicImage>;

    fn try_from(value: &'a DynamicImage) -> Result<Self, Self::Error> {
        from_image_iter(value.clone().channels.into_iter())
    }
}

fn from_image_iter<T: PixelType, const CHANNELS: usize>(
    mut value: impl Iterator<Item = DynamicImageChannel>,
) -> Result<Image<T, CHANNELS>, IncompatibleImageError<DynamicImage>> {
    let mut incompatible_image = Ok(());
    let mut prev_image_size = None;

    let all: [_; CHANNELS] = std::array::from_fn(|i| {
        if incompatible_image.is_ok() {
            incompatible_image = Err((
                i,
                match value.next() {
                    Some(dynamic) => match ImageChannel::try_from(dynamic) {
                        Ok(typed) => {
                            let b = typed.dimensions();
                            match prev_image_size {
                                Some(a) if a != b => (
                                    Some(typed.into()),
                                    IncompatibleImageErrorReason::MixedImageSizes { a, b },
                                ),
                                _ => {
                                    prev_image_size = Some(typed.dimensions());
                                    return MaybeUninit::new(typed);
                                }
                            }
                        }
                        Err(dynamic) => (
                            Some(dynamic),
                            IncompatibleImageErrorReason::Comptime {
                                pixel_dimensions: T::ELEMENTS,
                                pixel_kind: std::any::type_name::<T>(),
                                buffer_dimensions: const { NonZeroUsize::new(CHANNELS).unwrap() },
                            },
                        ),
                    },
                    None => (
                        None,
                        IncompatibleImageErrorReason::RequiresMoreChannels {
                            expected: const { crate::unwrap_usize_to_nonzero_u8(CHANNELS) },
                            actual: crate::unwrap_usize_to_nonzero_u8(i),
                        },
                    ),
                },
            ));
        }
        MaybeUninit::uninit()
    });
    match incompatible_image {
        Ok(()) => Ok(all
            .map(|x| unsafe { x.assume_init() })
            .try_into()
            .expect("Preconditions are checked already")),
        Err((initialized_indices, (error_image, reason))) => Err(IncompatibleImageError {
            image: DynamicImage {
                channels: all
                    .into_iter()
                    .take(initialized_indices)
                    .map(|x| unsafe { x.assume_init() }.into())
                    .chain(error_image)
                    .chain(value)
                    .collect(),
            },
            reason,
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
