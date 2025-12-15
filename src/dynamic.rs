use std::{
    fmt::Debug,
    num::{NonZeroU8, NonZeroUsize},
};

use crate::{Image, PixelType, pixel::DynamicPixelKind};

/// Trait that extends `Any` with a method to clone the boxed value.
trait DynamicImageBackend: std::any::Any + Debug + Send + Sync {
    fn boxed_clone(&self) -> Box<dyn DynamicImageBackend>;
}

impl<T: Clone + Send + Sync, const CHANNELS: usize> DynamicImageBackend for Image<T, CHANNELS> {
    fn boxed_clone(&self) -> Box<dyn DynamicImageBackend> {
        Box::new(self.clone())
    }
}

/// Image with unknown number of channels and their types
///
/// The public interface is designed, so it can be extended to support images, which cannot be represented with Image (e.g. 1 Channel U8 and the other f32) in the future
/// It currently only allows casting back to Image to access the data
#[derive(Debug)]
pub struct DynamicImage {
    data: Box<dyn DynamicImageBackend>,
    layout: ImageLayout<NonZeroUsize>,
}

impl Clone for DynamicImage {
    fn clone(&self) -> Self {
        DynamicImage {
            data: self.data.boxed_clone(),
            layout: self.layout,
        }
    }
}

impl DynamicImage {
    pub fn channel_infos(&self) -> impl Iterator<Item = (NonZeroU8, DynamicPixelKind)> {
        (0..self.layout.buffer_dimensions.get())
            .map(|_| (self.layout.pixel_dimensions, self.layout.pixel_kind))
    }

    pub fn buffer_dimensions(&self) -> NonZeroUsize {
        self.layout.buffer_dimensions
    }
}

impl<TPixel: PixelType + Send + Sync + Clone, const CHANNELS: usize> From<Image<TPixel, CHANNELS>>
    for DynamicImage
{
    fn from(value: Image<TPixel, CHANNELS>) -> Self {
        DynamicImage {
            data: Box::new(value) as _,
            layout: ImageLayout {
                pixel_dimensions: const { NonZeroU8::new(TPixel::PIXEL_CHANNELS).unwrap() },
                pixel_kind: TPixel::KIND,
                buffer_dimensions: NonZeroUsize::try_from(CHANNELS)
                    .expect("Checked during construction"),
            },
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
        match (value.data.as_ref() as &dyn std::any::Any).downcast_ref::<Self>() {
            Some(_) => Ok(*(value.data as Box<dyn std::any::Any>)
                .downcast::<Self>()
                .expect("Checked during construction")),
            None => Err(IncompatibleImageError {
                image: value,
                expected: ImageLayout {
                    pixel_dimensions: const { NonZeroU8::new(T::PIXEL_CHANNELS).unwrap() },
                    pixel_kind: T::KIND,
                    buffer_dimensions: CHANNELS,
                },
            }),
        }
    }
}

/// This is a temporary workaround until VTables are per layer instead of per image
impl<'a, T: PixelType + Send + Sync + Clone, const CHANNELS: usize> TryFrom<&'a DynamicImage>
    for &'a Image<T, CHANNELS>
{
    type Error = IncompatibleImageError<&'a DynamicImage>;

    fn try_from(value: &'a DynamicImage) -> Result<Self, Self::Error> {
        (value.data.as_ref() as &dyn std::any::Any)
            .downcast_ref::<Image<T, CHANNELS>>()
            .ok_or(IncompatibleImageError {
                image: value,
                expected: ImageLayout {
                    pixel_dimensions: const { NonZeroU8::new(T::PIXEL_CHANNELS).unwrap() },
                    pixel_kind: T::KIND,
                    buffer_dimensions: CHANNELS,
                },
            })
    }
}

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
        assert_eq!(1, dynamic.buffer_dimensions().get());
        assert_eq!(
            vec![(NonZeroU8::MIN, DynamicPixelKind::U8)],
            dynamic.channel_infos().collect::<Vec<_>>()
        );
        let luma_back: LumaImage<u8> = dynamic.try_into().unwrap();
        assert_eq!(luma_back.into_vec(), vec![1]);
    }

    #[test]
    fn create_from_luma_rgb8_interleaved() {
        let luma =
            LumaImage::<[u8; 3]>::new_vec(vec![[1u8, 2, 3]], NonZeroU32::MIN, NonZeroU32::MIN);
        let dynamic = DynamicImage::from(luma);
        assert_eq!(1, dynamic.buffer_dimensions().get());
        assert_eq!(
            vec![(NonZeroU8::new(3).unwrap(), DynamicPixelKind::U8)],
            dynamic.channel_infos().collect::<Vec<_>>()
        );
        let luma_back: LumaImage<[u8; 3]> = dynamic.try_into().unwrap();
        assert_eq!(luma_back.into_vec(), vec![[1u8, 2, 3]]);
    }

    #[test]
    fn create_from_luma_rgb8_planar() {
        let luma = Image::<u8, 3>::new_vec(vec![1u8, 2, 3], NonZeroU32::MIN, NonZeroU32::MIN);
        let dynamic = DynamicImage::from(luma);
        assert_eq!(3, dynamic.buffer_dimensions().get());
        assert_eq!(
            std::iter::repeat((NonZeroU8::MIN, DynamicPixelKind::U8))
                .take(3)
                .collect::<Vec<_>>(),
            dynamic.channel_infos().collect::<Vec<_>>()
        );
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
            let ref_luma: &Image<u8, 1> = (&cloned).try_into().unwrap();
            assert_eq!(ref_luma.dimensions(), (width, height));
        }
        let luma_cloned: LumaImage<u8> = cloned.try_into().unwrap();
        let vec_back = luma_back.into_vec();
        let vec_cloned = luma_cloned.into_vec();
        assert_eq!(vec_back, vec_cloned);
        assert_eq!(vec_cloned, vec![1, 2, 3, 4]);
    }

    #[test]
    fn create_from_rgb8_interleaved() {
        let rgb = Image::<[u8; 3], 1>::new_vec(vec![[1u8, 2, 3]], NonZeroU32::MIN, NonZeroU32::MIN);
        let dynamic = DynamicImage::from(rgb);
        assert_eq!(1, dynamic.buffer_dimensions().get());
        assert_eq!(
            vec![(const { NonZeroU8::new(3).unwrap() }, DynamicPixelKind::U8)],
            dynamic.channel_infos().collect::<Vec<_>>()
        );
    }
}
