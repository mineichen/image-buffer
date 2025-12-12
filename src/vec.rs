use std::num::NonZeroU32;

use crate::{
    Factory, ImageVtable, UnsafeImage, channel::ChannelFactory, channel::ImageChannelVTable,
    channel::UnsafeImageChannel,
};

impl<const CHANNELS: usize, T: 'static> UnsafeImage<T, CHANNELS> {
    pub fn new_vec(input: Vec<T>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        let cap = input.capacity();
        let base_ptr = input.as_ptr();
        assert_eq!(
            input.len() as u32,
            width.get() * height.get() * CHANNELS as u32,
            "Incompatible Buffer-Size"
        );

        let len_per_channel = (width.get() * height.get()) as usize;
        let ptrs = std::array::from_fn(|i| unsafe { base_ptr.add(i * len_per_channel) });
        std::mem::forget(input);
        let vtable = <VecFactory as Factory<T, CHANNELS>>::VTABLE;
        unsafe { Self::new_with_vtable(ptrs, width, height, vtable, cap) }
    }
}

struct VecFactory;

impl<T: 'static + Clone, const CHANNELS: usize> Factory<T, CHANNELS> for VecFactory {
    const VTABLE: &'static ImageVtable<T, CHANNELS> = {
        unsafe extern "C" fn make_mut<T: Clone, const CHANNELS: usize>(
            _image: &mut UnsafeImage<T, CHANNELS>,
        ) {
            // Do nothing, as ptrs are exclusive if i have &mut ImutableVtable
        }
        &ImageVtable {
            make_mut,
            drop: clear_vec,
            clone: crate::arc::clone_slice_into_arc,
        }
    };
}

pub(crate) extern "C" fn clear_vec<T, const CHANNELS: usize>(image: &mut UnsafeImage<T, CHANNELS>) {
    unsafe {
        Vec::from_raw_parts(
            image.ptrs[0] as *mut T,
            (image.width.get() * image.height.get()) as usize * CHANNELS,
            image.data,
        )
    };
}

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

// Helper to get Vec vtable (needed by shared_vec)
pub(crate) fn get_vec_channel_vtable<T: 'static + Clone>() -> &'static ImageChannelVTable<T> {
    <VecFactory as ChannelFactory<T>>::VTABLE
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
