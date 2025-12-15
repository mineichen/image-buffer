use std::num::NonZeroU32;

use crate::channel::{ChannelFactory, ChannelSize, ImageChannelVTable, UnsafeImageChannel};

struct VecFactory;

impl<T: 'static, TS: ChannelSize> UnsafeImageChannel<T, TS> {
    pub fn new_vec(input: Vec<T>, width: NonZeroU32, height: NonZeroU32, channel_size: TS) -> Self
    where
        T: Clone,
    {
        let cap = input.capacity();
        let ptr = input.as_ptr();
        assert_eq!(
            input.len() as u32,
            width.get() * height.get() * channel_size.get_pixel_channels().get() as u32,
            "Incompatible Buffer-Size"
        );

        std::mem::forget(input);
        let vtable = <VecFactory as ChannelFactory<T, TS>>::VTABLE;
        unsafe {
            Self::new_with_vtable(
                ptr,
                width,
                height,
                vtable,
                std::ptr::without_provenance_mut(cap),
                channel_size,
            )
        }
    }
}

impl<T: 'static + Clone, TS: ChannelSize> ChannelFactory<T, TS> for VecFactory {
    const VTABLE: &'static ImageChannelVTable<T, TS> = {
        unsafe extern "C" fn make_mut<T: Clone, TS: ChannelSize>(
            _image: &mut UnsafeImageChannel<T, TS>,
        ) {
            // Do nothing, as ptr is exclusive if i have &mut ImutableVTable
        }
        &ImageChannelVTable {
            make_mut,
            drop: clear_vec_channel,
            clone: crate::arc::clone_slice_into_arc_channel,
        }
    };
}

pub(crate) extern "C" fn clear_vec_channel<T, TS: ChannelSize>(
    image: &mut UnsafeImageChannel<T, TS>,
) {
    unsafe {
        Vec::from_raw_parts(
            image.ptr as *mut T,
            (image.width.get() * image.height.get()) as usize
                * image.channel_size.get_pixel_channels().get() as usize,
            image.data as usize,
        )
    };
}
