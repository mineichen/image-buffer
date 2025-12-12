use std::num::NonZeroU32;

use crate::{channel::ChannelFactory, channel::ImageChannelVTable, channel::UnsafeImageChannel};

struct VecFactory;

impl<T: 'static> UnsafeImageChannel<T> {
    pub fn new_vec(input: Vec<T>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        let cap = input.capacity();
        let ptr = input.as_ptr();
        assert_eq!(
            input.len() as u32,
            width.get() * height.get(),
            "Incompatible Buffer-Size"
        );

        std::mem::forget(input);
        let vtable = <VecFactory as ChannelFactory<T>>::VTABLE;
        unsafe {
            Self::new_with_vtable(
                ptr,
                width,
                height,
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
            image.ptr as *mut T,
            (image.width.get() * image.height.get()) as usize,
            image.data as usize,
        )
    };
}
