#![doc = include_str!("../README.md")]
#![warn(clippy::pedantic)]

use std::num::NonZeroU8;

mod arc;
mod channel;
mod dynamic;
mod external;
mod image;
mod pixel;
mod pixel_elements;
mod shared_vec;
mod vec;

pub use channel::{ImageChannel, ImageChannelVTable, UnsafeImageChannel};
pub use dynamic::{DynamicImage, DynamicImageChannel, IncompatibleImageError};
#[cfg(feature = "image_0_25")]
pub use external::*;
pub use image::Image;
pub use pixel::{DynamicSize, PixelType, PixelTypePrimitive};

pub type LumaImage<T> = Image<T, 1>;
pub type RgbImageInterleaved<T> = Image<[T; 3], 1>;
pub type RgbaImageInterleaved<T> = Image<[T; 4], 1>;
pub type RgbImagePlanar<T> = Image<T, 3>;
pub type RgbaImagePlanar<T> = Image<T, 4>;

const fn unwrap_usize_to_nonzero_u8(value: usize) -> NonZeroU8 {
    assert!(value <= 255, "usize must be less than 256");
    #[allow(clippy::cast_possible_truncation)]
    NonZeroU8::new(value as u8).unwrap()
}

mod seal {
    /// Allows to forbid external implementations to add new primitives
    /// This crate heavily relies on casting between primitive pointers
    pub trait SealedPrimitive {}
    impl SealedPrimitive for u8 {}
    impl SealedPrimitive for u16 {}
    impl SealedPrimitive for f32 {}
}
