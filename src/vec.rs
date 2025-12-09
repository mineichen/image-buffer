use std::num::NonZeroU32;

use crate::{Factory, ImageVtable, UnsafeImage};

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

pub(crate) extern "C" fn clear_vec<T, const CHANNELS: usize>(
    image: &mut UnsafeImage<T, CHANNELS>,
) {
    unsafe {
        Vec::from_raw_parts(
            image.ptrs[0] as *mut T,
            (image.width.get() * image.height.get()) as usize * CHANNELS,
            image.data,
        )
    };
}
