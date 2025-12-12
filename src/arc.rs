use std::{mem::ManuallyDrop, num::NonZeroU32, sync::Arc};

use crate::{
    Factory, ImageVtable, UnsafeImage,
    channel::{ChannelFactory, ImageChannelVTable, UnsafeImageChannel},
};

impl<const CHANNELS: usize, T: 'static> UnsafeImage<T, CHANNELS> {
    pub fn new_arc(input: Arc<[T]>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        assert_eq!(
            input.len() as u32,
            width.get() * height.get() * CHANNELS as u32,
            "Incompatible Buffer-Size"
        );

        let len = input.len();
        let base_ptr = Arc::into_raw(input).cast::<T>();
        let len_per_channel = (width.get() * height.get()) as usize;
        let ptrs = std::array::from_fn(|i| unsafe { base_ptr.add(i * len_per_channel) });
        let vtable = <ArcFactory as Factory<T, CHANNELS>>::VTABLE;
        unsafe { Self::new_with_vtable(ptrs, width, height, vtable, len) }
    }
}

struct ArcFactory;

impl<T: 'static + Clone, const CHANNELS: usize> Factory<T, CHANNELS> for ArcFactory {
    const VTABLE: &'static ImageVtable<T, CHANNELS> = {
        unsafe extern "C" fn make_mut<T: Clone, const CHANNELS: usize>(
            image: &mut UnsafeImage<T, CHANNELS>,
        ) {
            let mut arc = ManuallyDrop::new(unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptrs[0], image.data);
                Arc::<[T]>::from_raw(ptr)
            });

            if Arc::get_mut(&mut arc).is_none() {
                let new_data = Arc::<[T]>::from(&arc[..]);
                ManuallyDrop::into_inner(arc);

                let base_ptr = Arc::into_raw(new_data).cast::<T>();
                let len_per_channel = (image.width.get() * image.height.get()) as usize;

                image.ptrs = std::array::from_fn(|i| unsafe { base_ptr.add(i * len_per_channel) });
            }
        }
        extern "C" fn clear_arc<T: Clone, const CHANNELS: usize>(
            image: &mut UnsafeImage<T, CHANNELS>,
        ) {
            unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptrs[0], image.data);
                Arc::<[T]>::from_raw(ptr);
            }
        }

        extern "C" fn clone_arc<T: Clone, const CHANNELS: usize>(
            image: &UnsafeImage<T, CHANNELS>,
        ) -> UnsafeImage<T, CHANNELS> {
            let arc = ManuallyDrop::new(unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptrs[0], image.data);
                Arc::<[T]>::from_raw(ptr)
            });
            // Create UnsafeImage directly since Image no longer uses UnsafeImage
            UnsafeImage::new_arc((*arc).clone(), image.width, image.height)
        }

        &ImageVtable {
            drop: clear_arc,
            clone: clone_arc,
            make_mut,
        }
    };
}
pub(crate) extern "C" fn clone_slice_into_arc<T: Clone, const CHANNELS: usize>(
    image: &UnsafeImage<T, CHANNELS>,
) -> UnsafeImage<T, CHANNELS> {
    let buffer = unsafe {
        std::slice::from_raw_parts(
            image.ptrs[0],
            image.width.get() as usize * image.height.get() as usize * CHANNELS,
        )
    };
    UnsafeImage::new_arc(Arc::from(buffer), image.width, image.height)
}

impl<T: 'static> UnsafeImageChannel<T> {
    pub fn new_arc(input: Arc<[T]>, width: NonZeroU32, height: NonZeroU32) -> Self
    where
        T: Clone,
    {
        assert_eq!(
            input.len() as u32,
            width.get() * height.get(),
            "Incompatible Buffer-Size"
        );

        let len = input.len();
        let ptr = Arc::into_raw(input).cast::<T>();
        let vtable = <ArcFactory as ChannelFactory<T>>::VTABLE;
        unsafe {
            Self::new_with_vtable(
                ptr,
                width,
                height,
                vtable,
                std::ptr::without_provenance_mut(len),
            )
        }
    }
}

impl<T: 'static + Clone> ChannelFactory<T> for ArcFactory {
    const VTABLE: &'static ImageChannelVTable<T> = {
        unsafe extern "C" fn make_mut<T: Clone>(image: &mut UnsafeImageChannel<T>) {
            let mut arc = ManuallyDrop::new(unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptr, image.data as usize);
                Arc::<[T]>::from_raw(ptr)
            });

            if Arc::get_mut(&mut arc).is_none() {
                let new_data = Arc::<[T]>::from(&arc[..]);
                ManuallyDrop::into_inner(arc);

                image.ptr = Arc::into_raw(new_data).cast::<T>();
            }
        }
        extern "C" fn clear_arc_channel<T: Clone>(image: &mut UnsafeImageChannel<T>) {
            unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptr, image.data as usize);
                Arc::<[T]>::from_raw(ptr);
            }
        }

        extern "C" fn clone_arc_channel<T: Clone>(
            image: &UnsafeImageChannel<T>,
        ) -> UnsafeImageChannel<T> {
            let arc = ManuallyDrop::new(unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptr, image.data as usize);
                Arc::<[T]>::from_raw(ptr)
            });
            UnsafeImageChannel::new_arc((*arc).clone(), image.width, image.height)
        }

        &ImageChannelVTable {
            drop: clear_arc_channel,
            clone: clone_arc_channel,
            make_mut,
        }
    };
}

pub(crate) extern "C" fn clone_slice_into_arc_channel<T: Clone>(
    image: &UnsafeImageChannel<T>,
) -> UnsafeImageChannel<T> {
    let buffer = unsafe {
        std::slice::from_raw_parts(
            image.ptr,
            image.width.get() as usize * image.height.get() as usize,
        )
    };
    UnsafeImageChannel::new_arc(Arc::from(buffer), image.width, image.height)
}
