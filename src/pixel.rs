use std::num::NonZeroU8;

use crate::{
    ImageChannel,
    channel::{ComptimeSize, PixelSize, RuntimeSize},
    dynamic::DynamicImageChannel,
};

/// Removes all compile time hints, of how many channels a pixel persists
/// This is primarily used in `DynamicImageChannel`
#[derive(Clone, Copy, Default)]
pub struct DynamicSize<T: PixelTypePrimitive>(std::marker::PhantomData<T>);

impl<T: PixelTypePrimitive> RuntimePixelType for DynamicSize<T> {
    type Primitive = T;
    type PixelSize = RuntimeSize;
}

pub trait PixelTypePrimitive: Clone + PartialEq + Send + Sync + 'static {
    fn into_runtime_channel(i: ImageChannel<DynamicSize<Self>>) -> DynamicImageChannel;
    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Result<ImageChannel<DynamicSize<Self>>, DynamicImageChannel>;
}

impl PixelTypePrimitive for u8 {
    fn into_runtime_channel(i: ImageChannel<DynamicSize<Self>>) -> DynamicImageChannel {
        DynamicImageChannel::U8(i)
    }

    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Result<ImageChannel<DynamicSize<Self>>, DynamicImageChannel> {
        if let DynamicImageChannel::U8(channel) = channel {
            Ok(channel)
        } else {
            Err(channel)
        }
    }
}

impl PixelTypePrimitive for u16 {
    fn into_runtime_channel(i: ImageChannel<DynamicSize<Self>>) -> DynamicImageChannel {
        DynamicImageChannel::U16(i)
    }
    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Result<ImageChannel<DynamicSize<Self>>, DynamicImageChannel> {
        if let DynamicImageChannel::U16(channel) = channel {
            Ok(channel)
        } else {
            Err(channel)
        }
    }
}

impl PixelTypePrimitive for f32 {
    fn into_runtime_channel(i: ImageChannel<DynamicSize<Self>>) -> DynamicImageChannel {
        DynamicImageChannel::F32(i)
    }
    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Result<ImageChannel<DynamicSize<Self>>, DynamicImageChannel> {
        if let DynamicImageChannel::F32(channel) = channel {
            Ok(channel)
        } else {
            Err(channel)
        }
    }
}

pub trait RuntimePixelType: Clone + Sized + 'static {
    type Primitive: PixelTypePrimitive;
    type PixelSize: PixelSize + Default;
}

pub trait PixelType: RuntimePixelType + Clone + Sized + 'static {
    const PIXEL_CHANNELS: NonZeroU8;
}

impl<T: PixelTypePrimitive> RuntimePixelType for T {
    type Primitive = T;
    type PixelSize = ComptimeSize<1>;
}

impl<T: PixelTypePrimitive, const PIXEL_CHANNELS: usize> RuntimePixelType for [T; PIXEL_CHANNELS] {
    type Primitive = T;
    type PixelSize = ComptimeSize<PIXEL_CHANNELS>;
}

impl<T: PixelTypePrimitive> PixelType for T {
    const PIXEL_CHANNELS: NonZeroU8 = NonZeroU8::MIN;
}

impl<T: PixelTypePrimitive, const PIXEL_CHANNELS: usize> PixelType for [T; PIXEL_CHANNELS] {
    const PIXEL_CHANNELS: NonZeroU8 = {
        const {
            assert!(
                PIXEL_CHANNELS <= 255,
                "PIXEL_CHANNELS must be less than 256"
            );
            #[allow(clippy::cast_possible_truncation)]
            NonZeroU8::new(PIXEL_CHANNELS as u8).unwrap()
        }
    };
}
