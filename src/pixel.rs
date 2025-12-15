use std::num::NonZeroU8;

use crate::channel::{ChannelSize, ComptimeChannelSize};

pub(crate) trait PixelTypePrimitive: Clone + 'static + PartialEq {
    const KIND: DynamicPixelKind;
}

impl PixelTypePrimitive for u8 {
    const KIND: DynamicPixelKind = DynamicPixelKind::U8;
}

impl PixelTypePrimitive for u16 {
    const KIND: DynamicPixelKind = DynamicPixelKind::U16;
}

impl PixelTypePrimitive for f32 {
    const KIND: DynamicPixelKind = DynamicPixelKind::F32;
}

pub trait PixelType: Sized + 'static {
    type Primitive: PixelTypePrimitive;
    type ChannelSize: ChannelSize + Default;

    const PIXEL_CHANNELS: NonZeroU8;
    const KIND: DynamicPixelKind;

    // fn vec_remove_dimensions(vec: Vec<Self>) -> Vec<Self::Primitive>;
    // fn vec_add_dimensions(vec: Vec<Self::Primitive>) -> Vec<Self>;
}

impl<T: PixelTypePrimitive> PixelType for T {
    type Primitive = T;
    type ChannelSize = ComptimeChannelSize<1>;

    const PIXEL_CHANNELS: NonZeroU8 = NonZeroU8::MIN;
    const KIND: DynamicPixelKind = T::KIND;
    // fn vec_remove_dimensions(vec: Vec<Self>) -> Vec<Self::Primitive> {
    //     vec
    // }
    // fn vec_add_dimensions(vec: Vec<Self::Primitive>) -> Vec<Self> {
    //     vec
    // }
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
    // fn vec_remove_dimensions(vec: Vec<Self>) -> Vec<Self::Primitive> {
    //     debug_assert!(vec.len() % PIXEL_CHANNELS == 0);
    //     todo!()
    // }
    // fn vec_add_dimensions(vec: Vec<Self::Primitive>) -> Vec<Self> {
    //     todo!()
    // }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum DynamicPixelKind {
    U8,
    U16,
    F32,
}
