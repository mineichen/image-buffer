use std::num::NonZeroU32;

use image_0_25::{DynamicImage, GenericImageView};

use crate::Image;

#[derive(thiserror::Error)]
#[non_exhaustive]
pub struct IntoDynamicImage0_25Error {
    pub image: DynamicImage,
    pub reason: IntoDynamicImage0_25ErrorReason,
}

impl std::fmt::Display for IntoDynamicImage0_25Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cannot convert {}x{} image into DynamicImage: {}",
            self.image.width(),
            self.image.height(),
            self.reason
        )
    }
}

impl std::fmt::Debug for IntoDynamicImage0_25Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum IntoDynamicImage0_25ErrorReason {
    #[error("Neither height nor length can be 0")]
    ZeroDimension(#[from] std::num::TryFromIntError),
    #[error("The image has a wrong length. Expected {expected}, got {actual}")]
    IncompatibleBufferSize { expected: usize, actual: usize },
    #[error(
        "::image::DynamicImage is non_exhaustive, so it could be extended in the future. Report this as a bug/extension request to imbuf"
    )]
    NonExhaustive,
}

/// Only fails, if image::Image.{width/height}() is 0
impl TryFrom<DynamicImage> for crate::DynamicImage {
    type Error = IntoDynamicImage0_25Error;

    fn try_from(value: DynamicImage) -> Result<Self, Self::Error> {
        let (width, height) = value.dimensions();
        let width_times_height = width as usize * height as usize;
        let width = match NonZeroU32::try_from(width) {
            Ok(width) => width,
            Err(e) => {
                return Err(IntoDynamicImage0_25Error {
                    image: value,
                    reason: e.into(),
                });
            }
        };

        let height = match NonZeroU32::try_from(height) {
            Ok(height) => height,
            Err(e) => {
                return Err(IntoDynamicImage0_25Error {
                    image: value,
                    reason: e.into(),
                });
            }
        };
        Ok(match value {
            DynamicImage::ImageLuma8(x) => {
                Image::<u8, 1>::new_vec(extract_vec(x, width_times_height)?, width, height).into()
            }
            DynamicImage::ImageLuma16(x) => {
                Image::<u16, 1>::new_vec(extract_vec(x, width_times_height)?, width, height).into()
            }
            DynamicImage::ImageLumaA8(x) => Image::<[u8; 2], 1>::new_vec_flat(
                extract_vec(x, width_times_height)?,
                width,
                height,
            )
            .into(),
            DynamicImage::ImageLumaA16(x) => Image::<[u16; 2], 1>::new_vec_flat(
                extract_vec(x, width_times_height)?,
                width,
                height,
            )
            .into(),
            DynamicImage::ImageRgb8(x) => Image::<[u8; 3], 1>::new_vec_flat(
                extract_vec(x, width_times_height)?,
                width,
                height,
            )
            .into(),
            DynamicImage::ImageRgb16(x) => Image::<[u16; 3], 1>::new_vec_flat(
                extract_vec(x, width_times_height)?,
                width,
                height,
            )
            .into(),
            DynamicImage::ImageRgb32F(x) => Image::<[f32; 3], 1>::new_vec_flat(
                extract_vec(x, width_times_height)?,
                width,
                height,
            )
            .into(),
            DynamicImage::ImageRgba8(x) => Image::<[u8; 4], 1>::new_vec_flat(
                extract_vec(x, width_times_height)?,
                width,
                height,
            )
            .into(),
            DynamicImage::ImageRgba16(x) => Image::<[u16; 4], 1>::new_vec_flat(
                extract_vec(x, width_times_height)?,
                width,
                height,
            )
            .into(),
            DynamicImage::ImageRgba32F(x) => Image::<[f32; 4], 1>::new_vec_flat(
                extract_vec(x, width_times_height)?,
                width,
                height,
            )
            .into(),
            _ => {
                return Err(IntoDynamicImage0_25Error {
                    image: value,
                    reason: IntoDynamicImage0_25ErrorReason::NonExhaustive,
                });
            }
        })
    }
}

fn extract_vec<TPixel: image_0_25::Pixel>(
    image: image_0_25::ImageBuffer<TPixel, Vec<TPixel::Subpixel>>,
    width_times_height: usize,
) -> Result<Vec<TPixel::Subpixel>, IntoDynamicImage0_25Error>
where
    DynamicImage: From<image_0_25::ImageBuffer<TPixel, Vec<TPixel::Subpixel>>>,
{
    let actual = image.len();
    let expected = width_times_height * TPixel::CHANNEL_COUNT as usize;
    if actual != expected {
        return Err(IntoDynamicImage0_25Error {
            image: image.into(),
            reason: IntoDynamicImage0_25ErrorReason::IncompatibleBufferSize {
                expected: width_times_height,
                actual,
            },
        });
    }
    let vec = image.into_vec();
    Ok(vec)
}

mod tests {

    #[allow(unused_imports)] // Bug, probably because of crate renaming
    use image_0_25::DynamicImage;

    #[test]
    fn test_try_from_dynamic_luma_image() {
        let image = DynamicImage::new_luma8(100, 100);
        let dynamic_image = crate::DynamicImage::try_from(image).unwrap();
        assert_eq!(dynamic_image[0].width().get(), 100);
        assert_eq!(dynamic_image[0].height().get(), 100);
    }

    #[test]
    fn test_try_from_dynamic_rgb_image() {
        let image = DynamicImage::new_rgb16(100, 100);
        let dynamic_image = crate::DynamicImage::try_from(image).unwrap();
        assert_eq!(dynamic_image[0].width().get(), 100);
        assert_eq!(dynamic_image[0].height().get(), 100);
    }

    #[test]
    fn create_image_from_zero_width_fails() {
        let image: image_0_25::ImageBuffer<image_0_25::Luma<u8>, Vec<u8>> =
            image_0_25::ImageBuffer::from_raw(0, 1, vec![]).unwrap();
        crate::DynamicImage::try_from(DynamicImage::from(image)).unwrap_err();
    }

    #[test]
    fn create_image_from_zero_height_fails() {
        let image: image_0_25::ImageBuffer<image_0_25::Luma<u8>, Vec<u8>> =
            image_0_25::ImageBuffer::from_raw(1, 0, vec![]).unwrap();

        crate::DynamicImage::try_from(DynamicImage::from(image)).unwrap_err();
    }

    #[test]
    fn create_image_from_wrong_vec_len() {
        let image: image_0_25::ImageBuffer<image_0_25::Luma<u8>, Vec<u8>> =
            image_0_25::ImageBuffer::from_raw(1, 1, vec![0, 1]).unwrap();

        crate::DynamicImage::try_from(DynamicImage::from(image)).unwrap_err();
    }
}
