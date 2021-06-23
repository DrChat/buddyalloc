#![feature(allocator_api, const_mut_refs, const_ptr_offset)]

use buddyalloc::Heap;
use std::{
    alloc::{AllocError, Allocator, Layout},
    ptr::NonNull,
    sync::Mutex,
};

/// Declare a simple heap locked behind a Mutex.
#[derive(Debug)]
struct LockedHeap<const N: usize>(Mutex<Heap<N>>);

/// Implement Rust's [Allocator] trait for the locked heap.
unsafe impl<const N: usize> Allocator for LockedHeap<N> {
    fn allocate(&self, layout: std::alloc::Layout) -> Result<NonNull<[u8]>, AllocError> {
        let mut heap = self.0.lock().map_err(|_| AllocError)?;

        let ptr = heap.allocate(layout).map_err(|_| AllocError)?;

        // SAFETY: The pointer is guaranteed to not be NULL if the heap didn't return an error.
        Ok(unsafe { NonNull::new_unchecked(std::slice::from_raw_parts_mut(ptr, layout.size())) })
    }

    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: std::alloc::Layout) {
        let mut heap = match self.0.lock() {
            Ok(h) => h,
            Err(_) => return,
        };

        heap.deallocate(ptr.as_ptr(), layout);
    }
}

fn main() {
    // Allocate the backing memory for our heap. This memory _MUST_
    // be aligned by at least 4096.
    let layout = Layout::from_size_align(16384, 4096).unwrap();
    let mem = unsafe { std::alloc::alloc(layout) };

    // Construct our locked heap, with a minimum block size of 16 (16384 >> 10).
    let heap: LockedHeap<10> = LockedHeap(Mutex::new(
        unsafe { Heap::new(NonNull::new(mem).unwrap(), 16384) }.unwrap(),
    ));
    let mut vec = Vec::with_capacity_in(16, &heap);

    vec.push(0usize);
    vec.push(1usize);
    vec.push(2usize);
    vec.push(3usize);

    println!("{:?}", vec);

    // Drop the heap and vector before freeing the backing memory.
    drop(vec);
    drop(heap);

    unsafe {
        std::alloc::dealloc(mem, layout);
    }
}
