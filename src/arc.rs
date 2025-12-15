use std::{mem::ManuallyDrop, num::NonZeroU32, sync::Arc};

use crate::{
    ImageChannel,
    channel::{ChannelFactory, ChannelSize, ImageChannelVTable, UnsafeImageChannel},
};

struct ArcFactory;

impl<T: 'static, TS: ChannelSize> UnsafeImageChannel<T, TS> {
    pub fn new_arc(input: Arc<[T]>, width: NonZeroU32, height: NonZeroU32, channel_size: TS) -> Self
    where
        T: Clone,
    {
        let len = input.len();
        assert_eq!(
            len,
            Self::calc_ptr_len_from_parts(width, height, channel_size),
            "Incompatible Buffer-Size"
        );

        let ptr = Arc::into_raw(input).cast::<T>();
        let vtable = <ArcFactory as ChannelFactory<T, TS>>::VTABLE;
        unsafe {
            Self::new_with_vtable(
                ptr,
                width,
                height,
                vtable,
                std::ptr::without_provenance_mut(len),
                channel_size,
            )
        }
    }
}

impl<T: 'static + Clone, TS: ChannelSize> ChannelFactory<T, TS> for ArcFactory {
    const VTABLE: &'static ImageChannelVTable<T, TS> = {
        unsafe extern "C" fn make_mut<T: Clone, TS: ChannelSize>(
            image: &mut UnsafeImageChannel<T, TS>,
        ) {
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
        extern "C" fn clear_arc_channel<T: Clone, TS: ChannelSize>(
            image: &mut UnsafeImageChannel<T, TS>,
        ) {
            unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptr, image.data as usize);
                Arc::<[T]>::from_raw(ptr);
            }
        }

        extern "C" fn clone_arc_channel<T: Clone, TS: ChannelSize>(
            image: &UnsafeImageChannel<T, TS>,
        ) -> UnsafeImageChannel<T, TS> {
            let arc = ManuallyDrop::new(unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptr, image.data as usize);
                Arc::<[T]>::from_raw(ptr)
            });
            UnsafeImageChannel::new_arc(
                (*arc).clone(),
                image.width,
                image.height,
                image.channel_size.clone(),
            )
        }

        &ImageChannelVTable {
            drop: clear_arc_channel,
            clone: clone_arc_channel,
            make_mut,
        }
    };
}

pub(crate) extern "C" fn clone_slice_into_arc_channel<T: Clone, TS: ChannelSize>(
    image: &UnsafeImageChannel<T, TS>,
) -> UnsafeImageChannel<T, TS> {
    let buffer = unsafe {
        std::slice::from_raw_parts(
            image.ptr,
            UnsafeImageChannel::<T, TS>::calc_ptr_len_from_parts(
                image.width,
                image.height,
                image.channel_size,
            ),
        )
    };
    UnsafeImageChannel::new_arc(
        Arc::from(buffer),
        image.width,
        image.height,
        image.channel_size.clone(),
    )
}
