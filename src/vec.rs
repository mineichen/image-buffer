use std::num::{NonZeroU8, NonZeroU32};

use crate::channel::{
    ChannelFactory, ImageChannelVTable, UnsafeImageChannel, calc_channel_len_flat,
};

struct VecFactory;

impl<T: 'static> UnsafeImageChannel<T> {
    pub fn new_vec(
        input: Vec<T>,
        width: NonZeroU32,
        height: NonZeroU32,
        channel_size: NonZeroU8,
    ) -> Self
    where
        T: Clone,
    {
        let cap = input.capacity();
        let ptr = input.as_ptr();

        assert_eq!(
            input.len(),
            calc_channel_len_flat(width, height, channel_size),
            "Incompatible Buffer-Size"
        );
        std::mem::forget(input);
        let vtable = <VecFactory as ChannelFactory<T>>::VTABLE;
        unsafe {
            Self::new_with_vtable(
                ptr,
                width,
                height,
                channel_size,
                vtable,
                std::ptr::without_provenance_mut(cap),
            )
        }
    }
}

impl<T: 'static + Clone> ChannelFactory<T> for VecFactory {
    const VTABLE: &'static ImageChannelVTable<T> = {
        unsafe extern "C" fn make_mut<T: Clone>(_image: &mut UnsafeImageChannel<T>) {
            // Do nothing, as ptr is exclusive if i have &mut ImutableVTable
        }
        &ImageChannelVTable {
            make_mut,
            drop: clear_vec_channel,
            clone: crate::arc::clone_slice_into_arc_channel,
        }
    };
}

pub(crate) extern "C" fn clear_vec_channel<T>(image: &mut UnsafeImageChannel<T>) {
    unsafe {
        Vec::from_raw_parts(
            image.ptr.cast_mut(),
            (image.width.get() * image.height.get()) as usize * image.channel_size.get() as usize,
            image.data as usize,
        )
    };
}
