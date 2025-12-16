use std::num::NonZeroU8;

use crate::{
    ImageChannel,
    channel::{ComptimeChannelSize, PixelChannels, RuntimeChannelSize},
    dynamic::DynamicImageChannel,
};

pub(crate) trait PixelTypePrimitive: Clone + PartialEq + Send + Sync + 'static {
    const KIND: DynamicPixelKind;
    fn into_runtime_channel<TS: PixelChannels>(i: ImageChannel<Self, TS>) -> DynamicImageChannel;
    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Option<ImageChannel<Self, RuntimeChannelSize>>;
}

impl PixelTypePrimitive for u8 {
    const KIND: DynamicPixelKind = DynamicPixelKind::U8;

    fn into_runtime_channel<TS: PixelChannels>(i: ImageChannel<Self, TS>) -> DynamicImageChannel {
        DynamicImageChannel::U8(i.into_runtime())
    }

    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Option<ImageChannel<Self, RuntimeChannelSize>> {
        if let DynamicImageChannel::U8(channel) = channel {
            Some(channel)
        } else {
            None
        }
    }
}

impl PixelTypePrimitive for u16 {
    const KIND: DynamicPixelKind = DynamicPixelKind::U16;
    fn into_runtime_channel<TS: PixelChannels>(i: ImageChannel<Self, TS>) -> DynamicImageChannel {
        DynamicImageChannel::U16(i.into_runtime())
    }
    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Option<ImageChannel<Self, RuntimeChannelSize>> {
        if let DynamicImageChannel::U16(channel) = channel {
            Some(channel)
        } else {
            None
        }
    }
}

impl PixelTypePrimitive for f32 {
    const KIND: DynamicPixelKind = DynamicPixelKind::F32;
    fn into_runtime_channel<TS: PixelChannels>(i: ImageChannel<Self, TS>) -> DynamicImageChannel {
        DynamicImageChannel::F32(i.into_runtime())
    }
    fn try_from_dynamic_image(
        channel: DynamicImageChannel,
    ) -> Option<ImageChannel<Self, RuntimeChannelSize>> {
        if let DynamicImageChannel::F32(channel) = channel {
            Some(channel)
        } else {
            None
        }
    }
}

pub trait PixelType: Clone + Sized + 'static {
    type Primitive: PixelTypePrimitive;
    type ChannelSize: PixelChannels + Default;

    const PIXEL_CHANNELS: NonZeroU8;
    const KIND: DynamicPixelKind;
}

impl<T: PixelTypePrimitive> PixelType for T {
    type Primitive = T;
    type ChannelSize = ComptimeChannelSize<1>;

    const PIXEL_CHANNELS: NonZeroU8 = NonZeroU8::MIN;
    const KIND: DynamicPixelKind = T::KIND;
}

impl<T: PixelTypePrimitive, const PIXEL_CHANNELS: usize> PixelType for [T; PIXEL_CHANNELS] {
    type Primitive = T;
    type ChannelSize = ComptimeChannelSize<PIXEL_CHANNELS>;
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
    const KIND: DynamicPixelKind = T::KIND;
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum DynamicPixelKind {
    U8,
    U16,
    F32,
}
