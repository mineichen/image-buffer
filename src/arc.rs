use std::{mem::ManuallyDrop, num::NonZeroU32, sync::Arc};

use crate::{Factory, GenericImage, ImageVtable, UnsafeGenericImage};

impl<const CHANNELS: usize, T: 'static> UnsafeGenericImage<T, CHANNELS> {
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
            image: &mut UnsafeGenericImage<T, CHANNELS>,
        ) -> *mut T {
            let mut arc = ManuallyDrop::new(unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptrs[0], image.data);
                Arc::<[T]>::from_raw(ptr)
            });

            if let Some(ptr) = Arc::get_mut(&mut arc) {
                ptr.as_mut_ptr()
            } else {
                let mut new_data = Arc::<[T]>::from(&arc[..]);
                ManuallyDrop::into_inner(arc);

                let ptr = Arc::get_mut(&mut new_data).expect("Just created, must be unique");
                let r = ptr.as_mut_ptr();
                let base_ptr = Arc::into_raw(new_data).cast::<T>();
                let len_per_channel = (image.width.get() * image.height.get()) as usize;
                image.ptrs = std::array::from_fn(|i| base_ptr.add(i * len_per_channel));
                r
            }
        }
        extern "C" fn clear_arc<T: Clone, const CHANNELS: usize>(
            image: &mut UnsafeGenericImage<T, CHANNELS>,
        ) {
            unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptrs[0], image.data);
                Arc::<[T]>::from_raw(ptr);
            }
        }

        extern "C" fn clone_arc<T: Clone, const CHANNELS: usize>(
            image: &UnsafeGenericImage<T, CHANNELS>,
        ) -> UnsafeGenericImage<T, CHANNELS> {
            let arc = ManuallyDrop::new(unsafe {
                let ptr = std::ptr::slice_from_raw_parts(image.ptrs[0], image.data);
                Arc::<[T]>::from_raw(ptr)
            });
            GenericImage::new_arc((*arc).clone(), image.width, image.height).0
        }

        &ImageVtable {
            drop: clear_arc,
            clone: clone_arc,
            make_mut,
        }
    };
}
pub(crate) extern "C" fn clone_slice_into_arc<T: Clone, const CHANNELS: usize>(
    image: &UnsafeGenericImage<T, CHANNELS>,
) -> UnsafeGenericImage<T, CHANNELS> {
    let buffer = unsafe {
        std::slice::from_raw_parts(
            image.ptrs[0],
            image.width.get() as usize * image.height.get() as usize * CHANNELS,
        )
    };
    UnsafeGenericImage::new_arc(Arc::from(buffer), image.width, image.height)
}
