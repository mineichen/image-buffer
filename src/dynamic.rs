use std::num::{NonZeroU8, NonZeroUsize};

use crate::Image;

/// Dynamic version of image buffers.
/// Image<[u8; PIXEL_DIMENSIONS], BUFFER_DIMENSIONS>
///
/// The public interface is designed, so it can be extended to support images, which cannot be represented with Image (e.g. 1 Channel U8 and the other f32) in the future
/// It currently only allows casting back to Image to access the data
pub struct DynamicImage {
    data: Box<dyn std::any::Any>,
    layout: ImageLayout<NonZeroUsize>,
}

impl DynamicImage {
    pub fn pixel_kinds(&self) -> impl Iterator<Item = (NonZeroU8, DynamicPixelKind)> {
        (0..self.layout.buffer_dimensions.get())
            .map(|_| (self.layout.pixel_dimensions, self.layout.pixel_kind))
    }

    pub fn buffer_dimensions(&self) -> NonZeroUsize {
        self.layout.buffer_dimensions
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum DynamicPixelKind {
    U8,
    U16,
    F32,
}

impl<const CHANNELS: usize> From<Image<u8, CHANNELS>> for DynamicImage {
    fn from(value: Image<u8, CHANNELS>) -> Self {
        DynamicImage {
            data: Box::new(value) as _,
            layout: ImageLayout {
                pixel_dimensions: NonZeroU8::MIN,
                pixel_kind: DynamicPixelKind::U8,
                buffer_dimensions: NonZeroUsize::try_from(CHANNELS)
                    .expect("Checked during construction"),
            },
        }
    }
}

#[non_exhaustive]
#[derive(Debug)]
#[allow(dead_code)]
pub struct IncompatibleImageError {
    actual: ImageLayout<NonZeroUsize>,
    expected: ImageLayout<usize>,
}

impl<T: PixelType, const CHANNELS: usize, const PIXEL_CHANNELS: usize>
    From<Image<[T; PIXEL_CHANNELS], CHANNELS>> for DynamicImage
{
    fn from(value: Image<[T; PIXEL_CHANNELS], CHANNELS>) -> Self {
        let _non_zero = const { CHANNELS.checked_sub(1).unwrap() };
        let _non_one = const { CHANNELS.checked_sub(1).unwrap() };

        DynamicImage {
            data: Box::new(value) as _,
            layout: ImageLayout {
                pixel_kind: DynamicPixelKind::U8,
                pixel_dimensions: NonZeroU8::MIN,
                buffer_dimensions: NonZeroUsize::try_from(CHANNELS)
                    .expect("Checked during construction"),
            },
        }
    }
}

trait PixelTypePrimitive {
    const KIND: DynamicPixelKind;
}

impl PixelTypePrimitive for u8 {
    const KIND: DynamicPixelKind = DynamicPixelKind::U8;
}

impl PixelTypePrimitive for u16 {
    const KIND: DynamicPixelKind = DynamicPixelKind::U8;
}

impl PixelTypePrimitive for f32 {
    const KIND: DynamicPixelKind = DynamicPixelKind::U8;
}

pub trait PixelType {
    const PIXEL_CHANNELS: NonZeroU8;
    const KIND: DynamicPixelKind;
}

impl<T: PixelTypePrimitive> PixelType for T {
    const PIXEL_CHANNELS: NonZeroU8 = NonZeroU8::MIN;
    const KIND: DynamicPixelKind = T::KIND;
}
impl<T: PixelTypePrimitive> PixelType for [T; 2] {
    const PIXEL_CHANNELS: NonZeroU8 = NonZeroU8::new(2).unwrap();
    const KIND: DynamicPixelKind = T::KIND;
}
impl<T: PixelTypePrimitive> PixelType for [T; 3] {
    const PIXEL_CHANNELS: NonZeroU8 = NonZeroU8::new(3).unwrap();
    const KIND: DynamicPixelKind = T::KIND;
}
impl<T: PixelTypePrimitive> PixelType for [T; 4] {
    const PIXEL_CHANNELS: NonZeroU8 = NonZeroU8::new(4).unwrap();
    const KIND: DynamicPixelKind = T::KIND;
}

impl<T: PixelType, const CHANNELS: usize> TryFrom<DynamicImage> for Image<T, CHANNELS> {
    type Error = IncompatibleImageError;

    fn try_from(value: DynamicImage) -> Result<Self, Self::Error> {
        if let Ok(x) = value.data.downcast::<Self>() {
            Ok(*x)
        } else {
            Err(IncompatibleImageError {
                actual: value.layout,
                expected: ImageLayout {
                    pixel_dimensions: T::PIXEL_CHANNELS,
                    pixel_kind: T::KIND,
                    buffer_dimensions: CHANNELS,
                },
            })
        }
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
            dynamic.pixel_kinds().collect::<Vec<_>>()
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
            vec![(NonZeroU8::MIN, DynamicPixelKind::U8)],
            dynamic.pixel_kinds().collect::<Vec<_>>()
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
            dynamic.pixel_kinds().collect::<Vec<_>>()
        );
        let luma_back: Image<u8, 3> = dynamic.try_into().unwrap();
        assert_eq!(luma_back.into_vec(), vec![1u8, 2, 3]);
    }
}
