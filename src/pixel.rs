use std::num::NonZeroU8;

use crate::{
    ImageChannel,
    channel::{ComptimeChannelSize, PixelChannels, RuntimeChannelSize},
    dynamic::DynamicImageChannel,
};

// Wrapper type for runtime channel sizes. Its purpose is to disallow calling channels() on it ()
#[derive(Clone, Copy)]
pub struct FlatPixelType<T: PixelTypePrimitive>(std::marker::PhantomData<T>);

impl<T: PixelTypePrimitive> RuntimePixelTypeTrait for FlatPixelType<T> {
    type Primitive = T;
    type ChannelSize = RuntimeChannelSize;
    const KIND: DynamicPixelKind = T::KIND;
}

impl<T: PixelTypePrimitive> Default for FlatPixelType<T> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

pub(crate) trait PixelTypePrimitive: Clone + PartialEq + Send + Sync + 'static {
    const KIND: DynamicPixelKind;
    fn into_runtime_channel(i: ImageChannel<Self>) -> DynamicImageChannel;
    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Option<ImageChannel<FlatPixelType<Self>>>;
}

impl PixelTypePrimitive for u8 {
    const KIND: DynamicPixelKind = DynamicPixelKind::U8;

    fn into_runtime_channel(i: ImageChannel<Self>) -> DynamicImageChannel {
        DynamicImageChannel::U8(i.into_runtime())
    }

    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Option<ImageChannel<FlatPixelType<Self>>> {
        if let DynamicImageChannel::U8(channel) = channel {
            Some(channel)
        } else {
            None
        }
    }
}

impl PixelTypePrimitive for u16 {
    const KIND: DynamicPixelKind = DynamicPixelKind::U16;
    fn into_runtime_channel(i: ImageChannel<Self>) -> DynamicImageChannel {
        DynamicImageChannel::U16(i.into_runtime())
    }
    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Option<ImageChannel<FlatPixelType<Self>>> {
        if let DynamicImageChannel::U16(channel) = channel {
            Some(channel)
        } else {
            None
        }
    }
}

impl PixelTypePrimitive for f32 {
    const KIND: DynamicPixelKind = DynamicPixelKind::F32;
    fn into_runtime_channel(i: ImageChannel<Self>) -> DynamicImageChannel {
        DynamicImageChannel::F32(i.into_runtime())
    }
    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Option<ImageChannel<FlatPixelType<Self>>> {
        if let DynamicImageChannel::F32(channel) = channel {
            Some(channel)
        } else {
            None
        }
    }
}

pub trait RuntimePixelTypeTrait: Clone + Sized + 'static {
    type Primitive: PixelTypePrimitive;
    type ChannelSize: PixelChannels + Default;
    const KIND: DynamicPixelKind;
}

pub trait PixelTypeTrait: RuntimePixelTypeTrait + Clone + Sized + 'static {
    const PIXEL_CHANNELS: NonZeroU8;
}

impl<T: PixelTypePrimitive> RuntimePixelTypeTrait for T {
    type Primitive = T;
    type ChannelSize = ComptimeChannelSize<1>;
    const KIND: DynamicPixelKind = T::KIND;
}

impl<T: PixelTypePrimitive, const PIXEL_CHANNELS: usize> RuntimePixelTypeTrait
    for [T; PIXEL_CHANNELS]
{
    type Primitive = T;
    type ChannelSize = ComptimeChannelSize<PIXEL_CHANNELS>;
    const KIND: DynamicPixelKind = T::KIND;
}

impl<T: PixelTypePrimitive> PixelTypeTrait for T {
    const PIXEL_CHANNELS: NonZeroU8 = NonZeroU8::MIN;
}

impl<T: PixelTypePrimitive, const PIXEL_CHANNELS: usize> PixelTypeTrait for [T; PIXEL_CHANNELS] {
    const PIXEL_CHANNELS: NonZeroU8 = {
        let _ = const {
            if PIXEL_CHANNELS > 255 {
                panic!("PIXEL_CHANNELS must be less than 256");
            }
            if PIXEL_CHANNELS == 0 {
                panic!("PIXEL_CHANNELS must be greater than 0");
            }
        };
        NonZeroU8::new(PIXEL_CHANNELS as u8).unwrap()
    };
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum DynamicPixelKind {
    U8,
    U16,
    F32,
}
