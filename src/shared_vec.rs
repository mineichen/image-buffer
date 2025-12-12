use std::{
    marker::PhantomData,
    num::NonZeroU32,
    sync::atomic::{AtomicUsize, Ordering},
    vec::Vec,
};

use crate::channel::{ChannelFactory, ImageChannelVTable, UnsafeImageChannel};

/// Internal structure that holds a Vec (as raw parts) and reference counts
/// This allows multiple ImageChannels to share the same Vec
#[repr(C)]
pub struct SharedVecData<T, const CHANNELS: usize> {
    /// Pointer to the start of the Vec data
    vec: Vec<T>,
    /// Total number of ImageChannels using this SharedVec (global atomic)
    total_refs: AtomicUsize,
    /// Per-slice reference counts (one per slice/channel) - used to detect if mutual borrowing is ok
    slice_refs: [AtomicUsize; CHANNELS],
}

impl<T, const CHANNELS: usize> SharedVecData<T, CHANNELS> {
    fn new(vec: Vec<T>) -> Self {
        Self {
            vec,
            total_refs: AtomicUsize::new(CHANNELS),
            slice_refs: std::array::from_fn(|_| AtomicUsize::new(1)),
        }
    }

    fn increment_total(&self) -> usize {
        self.total_refs.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn decrement_total(&self) -> bool {
        self.total_refs.fetch_sub(1, Ordering::AcqRel) == 1
    }

    fn increment_slice(&self, slice_idx: usize) -> usize {
        self.slice_refs[slice_idx].fetch_add(1, Ordering::AcqRel) + 1
    }

    fn decrement_slice(&self, slice_idx: usize) -> usize {
        self.slice_refs[slice_idx].fetch_sub(1, Ordering::AcqRel) - 1
    }

    fn is_slice_unique(&self, slice_idx: usize) -> bool {
        self.slice_refs[slice_idx].load(Ordering::Acquire) == 1
    }
}

/// Metadata stored in a Box, with pointer stored in UnsafeImageChannel.data field
/// Minimal: only stores what's needed to access the shared data
#[repr(C)]
struct SharedVecMetadata<T, const CHANNELS: usize> {
    /// Pointer to the SharedVecData
    data_ptr: *mut SharedVecData<T, CHANNELS>,
    /// Index of this slice (to access the correct slice_refs in SharedVecData)
    slice_idx: usize,
    /// Start offset in the Vec for this slice
    start: usize,
}

// Single generic extern "C" functions with const CHANNELS
// These are instantiated when added to the vtable
unsafe extern "C" fn clone_shared_vec<T: 'static, const CHANNELS: usize>(
    image: &UnsafeImageChannel<T>,
) -> UnsafeImageChannel<T> {
    let metadata = unsafe { &mut *(image.data.cast::<SharedVecMetadata<T, CHANNELS>>()) };

    let data = metadata.data_ptr;
    let start = metadata.start;
    let slice_idx = metadata.slice_idx;

    // Increment slice reference count
    unsafe {
        (*data).increment_slice(slice_idx);
        (*data).increment_total();
    }

    // Create new metadata Box for the clone
    let metadata_clone = Box::new(SharedVecMetadata::<T, CHANNELS> {
        data_ptr: data,
        slice_idx,
        start,
    });
    let metadata_clone_ptr = Box::into_raw(metadata_clone);

    UnsafeImageChannel {
        ptr: image.ptr,
        width: image.width,
        height: image.height,
        vtable: image.vtable,
        data: metadata_clone_ptr.cast(),
    }
}

unsafe extern "C" fn make_mut_shared_vec<T: 'static + Clone, const CHANNELS: usize>(
    image: &mut UnsafeImageChannel<T>,
) {
    let metadata = unsafe { &mut *(image.data.cast::<SharedVecMetadata<T, CHANNELS>>()) };
    let data = metadata.data_ptr;
    let slice_idx = metadata.slice_idx;

    // Check if this slice is unique (count == 1)
    let is_unique = unsafe { (*data).is_slice_unique(slice_idx) };

    if !is_unique {
        // Clone the slice into a new Vec
        let slice = unsafe {
            std::slice::from_raw_parts(image.ptr, (image.width.get() * image.height.get()) as usize)
        };
        let new_vec: Vec<T> = slice.to_vec();
        let new_ptr = new_vec.as_ptr();
        let new_cap = new_vec.capacity();

        // Decrement old references
        unsafe {
            (*data).decrement_slice(slice_idx);
            if (*data).decrement_total() {
                let shared_data = Box::from_raw(data);
                drop(shared_data);
            }
        }
        // Drop the metadata Box
        let metadata_box =
            unsafe { Box::from_raw(image.data.cast::<SharedVecMetadata<T, CHANNELS>>()) };
        drop(metadata_box);

        // Update to point to new Vec
        image.ptr = new_ptr;
        image.data = (new_cap as usize) as *mut (); // Store capacity for Vec cleanup
        // Switch to Vec vtable
        use crate::vec;
        image.vtable = vec::get_vec_channel_vtable::<T>();

        // Leak the Vec - it will be cleaned up by the Vec vtable
        std::mem::forget(new_vec);
    }
    // If unique, we can mutate in place - no changes needed
}

pub(crate) extern "C" fn drop_shared_vec<T: 'static, const CHANNELS: usize>(
    image: &mut UnsafeImageChannel<T>,
) {
    unsafe {
        let metadata = Box::from_raw(image.data as *mut SharedVecMetadata<T, CHANNELS>);
        let data = metadata.data_ptr;
        let slice_idx = metadata.slice_idx;

        (*data).decrement_slice(slice_idx);
        if (*data).decrement_total() {
            let shared_data = Box::from_raw(data);
            // Vec will be dropped automatically when the Box is dropped
            drop(shared_data);
        }
    };
}

struct SharedVecFactory<T: 'static, const CHANNELS: usize>(PhantomData<(T, [(); CHANNELS])>);

// Implement ChannelFactory with const VTABLE using associated const
// PhantomData makes this type unique for each T and CHANNELS combination
impl<T: 'static + Clone, const CHANNELS: usize> ChannelFactory<T>
    for SharedVecFactory<T, CHANNELS>
{
    const VTABLE: &'static ImageChannelVTable<T> = {
        &ImageChannelVTable {
            clone: clone_shared_vec::<T, CHANNELS>,
            make_mut: make_mut_shared_vec::<T, CHANNELS>,
            drop: drop_shared_vec::<T, CHANNELS>,
        }
    };
}

/// Create ImageChannels from a Vec, sharing the underlying storage
/// Note: T: Clone is required for vtable creation, but clone/drop don't actually need it
pub fn create_shared_channels<T: 'static + Clone, const CHANNELS: usize>(
    vec: Vec<T>,
    width: NonZeroU32,
    height: NonZeroU32,
    offsets: [usize; CHANNELS],
) -> [UnsafeImageChannel<T>; CHANNELS] {
    // Create SharedVecData
    let base = vec.as_ptr();
    let shared_data = Box::new(SharedVecData::<T, CHANNELS>::new(vec));
    let data_ptr = Box::into_raw(shared_data);

    // Create ImageChannels for each slice
    std::array::from_fn(|i| {
        let start = offsets[i];

        // Create metadata in a Box (only one allocation per channel)
        let metadata = Box::new(SharedVecMetadata::<T, CHANNELS> {
            data_ptr,
            slice_idx: i,
            start,
        });
        let metadata_ptr = Box::into_raw(metadata);

        let vtable = <SharedVecFactory<T, CHANNELS> as ChannelFactory<T>>::VTABLE;
        unsafe {
            UnsafeImageChannel::new_with_vtable(
                base.add(start),
                width,
                height,
                vtable,
                metadata_ptr.cast(),
            )
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_vec_basic() {
        let vec = vec![0u8, 1u8, 2u8, 3u8, 4u8, 5u8];
        // Get the address of the original Vec's data before it's moved
        let original_ptr = vec.as_ptr();

        let width = NonZeroU32::new(2).unwrap();
        let height = NonZeroU32::new(1).unwrap();
        let len_per_channel = 2;

        let channels = create_shared_channels::<u8, 3>(vec, width, height, [0, 2, 4]);
        assert_eq!(channels.len(), 3);

        // Verify that channels point to the correct offsets in the original Vec
        assert_eq!(
            channels[0].ptr as *const u8, original_ptr,
            "Channel 0 should point to the start of the original Vec"
        );
        assert_eq!(
            channels[1].ptr as *const u8,
            unsafe { original_ptr.add(len_per_channel) },
            "Channel 1 should point to offset 2 in the original Vec"
        );
        assert_eq!(
            channels[2].ptr as *const u8,
            unsafe { original_ptr.add(len_per_channel * 2) },
            "Channel 2 should point to offset 4 in the original Vec"
        );

        // Check that channels point to correct data
        let buf0 = unsafe { std::slice::from_raw_parts(channels[0].ptr, 2) };
        let buf1 = unsafe { std::slice::from_raw_parts(channels[1].ptr, 2) };
        let buf2 = unsafe { std::slice::from_raw_parts(channels[2].ptr, 2) };

        assert_eq!(buf0, &[0u8, 1u8]);
        assert_eq!(buf1, &[2u8, 3u8]);
        assert_eq!(buf2, &[4u8, 5u8]);
    }

    #[test]
    fn test_shared_vec_clone() {
        let vec = vec![0u8, 1u8, 2u8, 3u8];
        let width = NonZeroU32::new(2).unwrap();
        let height = NonZeroU32::new(1).unwrap();

        let channels = create_shared_channels::<u8, 2>(vec, width, height, [0, 2]);
        let channel1_clone = unsafe { (channels[0].vtable.clone)(&channels[0]) };

        // Both should point to same data
        let buf_orig = unsafe { std::slice::from_raw_parts(channels[0].ptr, 2) };
        let buf_clone = unsafe { std::slice::from_raw_parts(channel1_clone.ptr, 2) };
        assert_eq!(buf_orig.as_ptr(), buf_clone.as_ptr());
    }
}
