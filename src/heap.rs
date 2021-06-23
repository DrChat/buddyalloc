//! A simple heap based on a buddy allocator.  For the theory of buddy
//! allocators, see <https://en.wikipedia.org/wiki/Buddy_memory_allocation>
//!
//! The basic idea is that our heap size is a power of two, and the heap
//! starts out as one giant free block.  When a memory allocation request
//! is received, we round the requested size up to a power of two, and find
//! the smallest available block we can use.  If the smallest free block is
//! too big (more than twice as big as the memory we want to allocate), we
//! split the smallest free block in half recursively until it's the right
//! size.  This simplifies a lot of bookkeeping, because all our block
//! sizes are a power of 2, which makes it easy to have one free list per
//! block size.
use core::alloc::Layout;
use core::cmp::{max, min};
use core::mem::size_of;
use core::ptr::{self, NonNull};
use core::result::Result;

use crate::math::log2;

const MIN_HEAP_ALIGN: usize = 4096;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AllocationSizeError {
    BadAlignment,
    TooLarge,
}

/// Represents the reason for an allocation error.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AllocationError {
    HeapExhausted,
    InvalidSize(AllocationSizeError),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeapError {
    BadBaseAlignment,
    BadSizeAlignment,
    BadHeapSize,
    MinBlockTooSmall,
}

/// A free block in our heap.  This is actually a header that we store at
/// the start of the block.  We don't store any size information in the
/// header, because we allocate a separate free block list for each block
/// size.
struct FreeBlock {
    /// The next block in the free list, or NULL if this is the final
    /// block.
    next: *mut FreeBlock,
}

impl FreeBlock {
    /// Construct a `FreeBlock` header pointing at `next`.
    const fn new(next: *mut FreeBlock) -> FreeBlock {
        FreeBlock { next }
    }
}

/// The interface to a heap.  This data structure is stored _outside_ the
/// heap somewhere, typically in a static variable, because every single
/// byte of our heap is potentially available for allocation.
///
/// The generic parameter N specifies the number of steps to divide the
/// available heap size by two. This will be the minimum allocable block size.
#[derive(Debug)]
pub struct Heap<const N: usize> {
    /// The base address of our heap.  This must be aligned on a
    /// `MIN_HEAP_ALIGN` boundary.
    heap_base: *mut u8,

    /// The space available in our heap.  This must be a power of 2.
    heap_size: usize,

    /// The free lists for our heap.  The list at `free_lists[0]` contains
    /// the smallest block size we can allocate, and the list at the end
    /// can only contain a single free block the size of our entire heap,
    /// and only when no memory is allocated.
    free_lists: [*mut FreeBlock; N],

    /// Our minimum block size.  This is calculated based on `heap_size`
    /// and the generic parameter N, and it must be
    /// big enough to contain a `FreeBlock` header object.
    min_block_size: usize,

    /// The log base 2 of our block size.  Cached here so we don't have to
    /// recompute it on every allocation (but we haven't benchmarked the
    /// performance gain).
    min_block_size_log2: u8,
}

// This structure can safely be sent between threads.
unsafe impl<const N: usize> Send for Heap<N> {}

impl<const N: usize> Heap<N> {
    /// Create a new heap.
    ///
    /// # Safety
    /// `heap_base` must be aligned on a
    /// `MIN_HEAP_ALIGN` boundary, `heap_size` must be a power of 2, and
    /// `heap_size / 2.pow(free_lists.len()-1)` must be greater than or
    /// equal to `size_of::<FreeBlock>()`.  Passing in invalid parameters
    /// may do horrible things.
    pub const unsafe fn new(heap_base: NonNull<u8>, heap_size: usize) -> Result<Self, HeapError> {
        // Calculate our minimum block size based on the number of free
        // lists we have available.
        let min_block_size = heap_size >> (N - 1);

        // The heap must be aligned on a 4K bounday.
        if heap_base.as_ptr() as usize & (MIN_HEAP_ALIGN - 1) != 0 {
            return Err(HeapError::BadBaseAlignment);
        }

        // The heap must be big enough to contain at least one block.
        if heap_size < min_block_size {
            return Err(HeapError::BadHeapSize);
        }

        // The smallest possible heap block must be big enough to contain
        // the block header.
        if min_block_size < size_of::<FreeBlock>() {
            return Err(HeapError::MinBlockTooSmall);
        }

        // The heap size must be a power of 2.
        if !heap_size.is_power_of_two() {
            return Err(HeapError::BadSizeAlignment);
        }

        // We must have one free list per possible heap block size.
        // FIXME: Can this assertion even be hit?
        // assert_eq!(
        //     min_block_size * (2u32.pow(N as u32 - 1)) as usize,
        //     heap_size
        // );

        // assert!(N > 0);
        let mut free_lists: [*mut FreeBlock; N] = [core::ptr::null_mut(); N];

        // Initialize the heap data as a single free block.
        let free_block = heap_base.as_ptr() as *mut FreeBlock;
        *free_block = FreeBlock::new(ptr::null_mut());

        // Insert the entire heap into the last free list.
        // See the documentation for `free_lists` - the last entry contains
        // the entire heap iff no memory is allocated.
        free_lists[N - 1] = free_block;

        // Store all the info about our heap in our struct.
        Ok(Self {
            heap_base: heap_base.as_ptr(),
            heap_size,
            free_lists,
            min_block_size,
            min_block_size_log2: log2(min_block_size),
        })
    }

    /// Figure out what size block we'll need to fulfill an allocation
    /// request.  This is deterministic, and it does not depend on what
    /// we've already allocated.  In particular, it's important to be able
    /// to calculate the same `allocation_size` when freeing memory as we
    /// did when allocating it, or everything will break horribly.
    fn allocation_size(&self, mut size: usize, align: usize) -> Result<usize, AllocationSizeError> {
        // Sorry, we don't support weird alignments.
        if !align.is_power_of_two() {
            return Err(AllocationSizeError::BadAlignment);
        }

        // We can't align any more precisely than our heap base alignment
        // without getting much too clever, so don't bother.
        if align > MIN_HEAP_ALIGN {
            return Err(AllocationSizeError::BadAlignment);
        }

        // We're automatically aligned to `size` because of how our heap is
        // sub-divided, but if we need a larger alignment, we can only do
        // it be allocating more memory.
        if align > size {
            size = align;
        }

        // We can't allocate blocks smaller than `min_block_size`.
        size = max(size, self.min_block_size);

        // Round up to the next power of two.
        size = size.next_power_of_two();

        // We can't allocate a block bigger than our heap.
        if size > self.heap_size {
            return Err(AllocationSizeError::TooLarge);
        }

        Ok(size)
    }

    /// The "order" of an allocation is how many times we need to double
    /// `min_block_size` in order to get a large enough block, as well as
    /// the index we use into `free_lists`.
    fn allocation_order(&self, size: usize, align: usize) -> Result<usize, AllocationSizeError> {
        self.allocation_size(size, align)
            .map(|s| (log2(s) - self.min_block_size_log2) as usize)
    }

    /// The size of the blocks we allocate for a given order.
    const fn order_size(&self, order: usize) -> usize {
        1 << (self.min_block_size_log2 as usize + order)
    }

    /// Pop a block off the appropriate free list.
    fn free_list_pop(&mut self, order: usize) -> Option<*mut u8> {
        let candidate = self.free_lists[order];
        if !candidate.is_null() {
            self.free_lists[order] = unsafe { (*candidate).next };
            Some(candidate as *mut u8)
        } else {
            None
        }
    }

    /// Insert `block` of order `order` onto the appropriate free list.
    unsafe fn free_list_insert(&mut self, order: usize, block: *mut u8) {
        let free_block_ptr = block as *mut FreeBlock;
        *free_block_ptr = FreeBlock::new(self.free_lists[order]);
        self.free_lists[order] = free_block_ptr;
    }

    /// Attempt to remove a block from our free list, returning true
    /// success, and false if the block wasn't on our free list.  This is
    /// the slowest part of a primitive buddy allocator, because it runs in
    /// O(log N) time where N is the number of blocks of a given size.
    ///
    /// We could perhaps improve this by keeping our free lists sorted,
    /// because then "nursery generation" allocations would probably tend
    /// to occur at lower addresses and then be faster to find / rule out
    /// finding.
    fn free_list_remove(&mut self, order: usize, block: *mut u8) -> bool {
        let block_ptr = block as *mut FreeBlock;

        // Yuck, list traversals are gross without recursion.  Here,
        // `*checking` is the pointer we want to check, and `checking` is
        // the memory location we found it at, which we'll need if we want
        // to replace the value `*checking` with a new value.
        let mut checking: &mut *mut FreeBlock = &mut self.free_lists[order];

        // Loop until we run out of free blocks.
        while !(*checking).is_null() {
            // Is this the pointer we want to remove from the free list?
            if *checking == block_ptr {
                // Yup, this is the one, so overwrite the value we used to
                // get here with the next one in the sequence.
                *checking = unsafe { (*(*checking)).next };
                return true;
            }

            // Haven't found it yet, so point `checking` at the address
            // containing our `next` field.  (Once again, this is so we'll
            // be able to reach back and overwrite it later if necessary.)
            checking = unsafe { &mut ((*(*checking)).next) };
        }
        false
    }

    /// Split a `block` of order `order` down into a block of order
    /// `order_needed`, placing any unused chunks on the free list.
    ///
    /// # Safety
    /// The block must be owned by this heap, otherwise bad things
    /// will happen.
    unsafe fn split_free_block(&mut self, block: *mut u8, mut order: usize, order_needed: usize) {
        // Get the size of our starting block.
        let mut size_to_split = self.order_size(order);

        // Progressively cut our block down to size.
        while order > order_needed {
            // Update our loop counters to describe a block half the size.
            size_to_split >>= 1;
            order -= 1;

            // Insert the "upper half" of the block into the free list.
            let split = block.add(size_to_split);
            self.free_list_insert(order, split);
        }
    }

    /// Given a `block` with the specified `order`, find the "buddy" block,
    /// that is, the other half of the block we originally split it from,
    /// and also the block we could potentially merge it with.
    fn buddy(&self, order: usize, block: *mut u8) -> Option<*mut u8> {
        assert!(block >= self.heap_base);

        let relative = unsafe { block.offset_from(self.heap_base) } as usize;
        let size = self.order_size(order);
        if size >= self.heap_size {
            // The main heap itself does not have a budy.
            None
        } else {
            // Fun: We can find our buddy by xoring the right bit in our
            // offset from the base of the heap.
            Some(unsafe { self.heap_base.add(relative ^ size) })
        }
    }

    /// Allocate a block of memory large enough to contain `layout`,
    /// and aligned to `layout`.  This will return an [`AllocationError`]
    /// if the alignment is greater than `MIN_HEAP_ALIGN`, or if
    /// we can't find enough memory.
    ///
    /// All allocated memory must be passed to `deallocate` with the same
    /// `size` and `align` parameter, or else horrible things will happen.
    pub fn allocate(&mut self, layout: Layout) -> Result<*mut u8, AllocationError> {
        // Figure out which order block we need.
        match self.allocation_order(layout.size(), layout.align()) {
            Ok(order_needed) => {
                // Start with the smallest acceptable block size, and search
                // upwards until we reach blocks the size of the entire heap.
                for order in order_needed..self.free_lists.len() {
                    // Do we have a block of this size?
                    if let Some(block) = self.free_list_pop(order) {
                        // If the block is too big, break it up.  This leaves
                        // the address unchanged, because we always allocate at
                        // the head of a block.
                        if order > order_needed {
                            // SAFETY: The block came from the heap.
                            unsafe { self.split_free_block(block, order, order_needed) };
                        }

                        // We have an allocation, so quit now.
                        return Ok(block);
                    }
                }

                // We couldn't find a large enough block for this allocation.
                Err(AllocationError::HeapExhausted)
            }

            // We can't allocate a block with the specified size and
            // alignment.
            Err(e) => Err(AllocationError::InvalidSize(e)),
        }
    }

    /// Deallocate a block allocated using `allocate`.
    ///
    /// # Safety
    /// `layout` must match what was passed to `allocate`,
    /// or our heap will be corrupted.
    pub unsafe fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        let initial_order = self
            .allocation_order(layout.size(), layout.align())
            .expect("Tried to dispose of invalid block");

        // The fun part: When deallocating a block, we also want to check
        // to see if its "buddy" is on the free list.  If the buddy block
        // is also free, we merge them and continue walking up.
        //
        // `block` is the biggest merged block we have so far.
        let mut block = ptr;
        for order in initial_order..self.free_lists.len() {
            // Would this block have a buddy?
            if let Some(buddy) = self.buddy(order, block) {
                // Is this block's buddy free?
                if self.free_list_remove(order, buddy) {
                    // Merge them!  The lower address of the two is the
                    // newly-merged block.  Then we want to try again.
                    block = min(block, buddy);
                    continue;
                }
            }

            // If we reach here, we didn't find a buddy block of this size,
            // so take what we've got and mark it as free.
            self.free_list_insert(order, block);
            return;
        }
    }
}

#[cfg(test)]
mod test {
    // Use std in tests.
    extern crate std;
    use super::*;

    #[test]
    fn test_allocation_size_and_order() {
        unsafe {
            let heap_size = 256;
            let layout = std::alloc::Layout::from_size_align(heap_size, 4096).unwrap();
            let mem = std::alloc::alloc(layout);
            let heap: Heap<5> = Heap::new(NonNull::new(mem).unwrap(), heap_size).unwrap();

            // Can't align beyond MIN_HEAP_ALIGN.
            assert_eq!(
                Err(AllocationSizeError::BadAlignment),
                heap.allocation_size(256, 8192)
            );

            // Can't align beyond heap_size.
            assert_eq!(
                Err(AllocationSizeError::TooLarge),
                heap.allocation_size(256, 256 * 2)
            );

            // Simple allocations just round up to next block size.
            assert_eq!(Ok(16), heap.allocation_size(0, 1));
            assert_eq!(Ok(16), heap.allocation_size(1, 1));
            assert_eq!(Ok(16), heap.allocation_size(16, 1));
            assert_eq!(Ok(32), heap.allocation_size(17, 1));
            assert_eq!(Ok(32), heap.allocation_size(32, 32));
            assert_eq!(Ok(256), heap.allocation_size(256, 256));

            // Aligned allocations use alignment as block size.
            assert_eq!(Ok(64), heap.allocation_size(16, 64));

            // Block orders.
            assert_eq!(Ok(0), heap.allocation_order(0, 1));
            assert_eq!(Ok(0), heap.allocation_order(1, 1));
            assert_eq!(Ok(0), heap.allocation_order(16, 16));
            assert_eq!(Ok(1), heap.allocation_order(32, 32));
            assert_eq!(Ok(2), heap.allocation_order(64, 64));
            assert_eq!(Ok(3), heap.allocation_order(128, 128));
            assert_eq!(Ok(4), heap.allocation_order(256, 256));
            assert_eq!(
                Err(AllocationSizeError::TooLarge),
                heap.allocation_order(512, 512)
            );

            std::alloc::dealloc(mem, layout);
        }
    }

    #[test]
    fn test_buddy() {
        unsafe {
            let heap_size = 256;
            let layout = std::alloc::Layout::from_size_align(heap_size, 4096).unwrap();
            let mem = std::alloc::alloc(layout);
            let heap: Heap<5> = Heap::new(NonNull::new(mem).unwrap(), heap_size).unwrap();

            let block_16_0 = mem;
            let block_16_1 = mem.offset(16);
            assert_eq!(Some(block_16_1), heap.buddy(0, block_16_0));
            assert_eq!(Some(block_16_0), heap.buddy(0, block_16_1));

            let block_32_0 = mem;
            let block_32_1 = mem.offset(32);
            assert_eq!(Some(block_32_1), heap.buddy(1, block_32_0));
            assert_eq!(Some(block_32_0), heap.buddy(1, block_32_1));

            let block_32_2 = mem.offset(64);
            let block_32_3 = mem.offset(96);
            assert_eq!(Some(block_32_3), heap.buddy(1, block_32_2));
            assert_eq!(Some(block_32_2), heap.buddy(1, block_32_3));

            let block_256_0 = mem;
            assert_eq!(None, heap.buddy(4, block_256_0));

            std::alloc::dealloc(mem, layout);
        }
    }

    #[test]
    fn test_alloc_and_dealloc() {
        unsafe {
            let heap_size = 256;
            let layout = std::alloc::Layout::from_size_align(heap_size, 4096).unwrap();
            let mem = std::alloc::alloc(layout);
            let mut heap: Heap<5> = Heap::new(NonNull::new(mem).unwrap(), heap_size).unwrap();

            let block_16_0 = heap
                .allocate(Layout::from_size_align(8, 8).unwrap())
                .unwrap();
            assert_eq!(mem, block_16_0);

            let bigger_than_heap = heap.allocate(Layout::from_size_align(heap_size, 4096).unwrap());
            assert_eq!(
                Err(AllocationError::InvalidSize(AllocationSizeError::TooLarge)),
                bigger_than_heap
            );

            let bigger_than_free =
                heap.allocate(Layout::from_size_align(heap_size, heap_size).unwrap());
            assert_eq!(Err(AllocationError::HeapExhausted), bigger_than_free);

            let block_16_1 = heap
                .allocate(Layout::from_size_align(8, 8).unwrap())
                .unwrap();
            assert_eq!(mem.offset(16), block_16_1);

            let block_16_2 = heap
                .allocate(Layout::from_size_align(8, 8).unwrap())
                .unwrap();
            assert_eq!(mem.offset(32), block_16_2);

            let block_32_2 = heap
                .allocate(Layout::from_size_align(32, 32).unwrap())
                .unwrap();
            assert_eq!(mem.offset(64), block_32_2);

            let block_16_3 = heap
                .allocate(Layout::from_size_align(8, 8).unwrap())
                .unwrap();
            assert_eq!(mem.offset(48), block_16_3);

            let block_128_1 = heap
                .allocate(Layout::from_size_align(128, 128).unwrap())
                .unwrap();
            assert_eq!(mem.offset(128), block_128_1);

            let too_fragmented = heap.allocate(Layout::from_size_align(64, 64).unwrap());
            assert_eq!(Err(AllocationError::HeapExhausted), too_fragmented);

            heap.deallocate(block_32_2, Layout::from_size_align(32, 32).unwrap());
            heap.deallocate(block_16_0, Layout::from_size_align(8, 8).unwrap());
            heap.deallocate(block_16_3, Layout::from_size_align(8, 8).unwrap());
            heap.deallocate(block_16_1, Layout::from_size_align(8, 8).unwrap());
            heap.deallocate(block_16_2, Layout::from_size_align(8, 8).unwrap());

            let block_128_0 = heap
                .allocate(Layout::from_size_align(128, 128).unwrap())
                .unwrap();
            assert_eq!(mem.offset(0), block_128_0);

            heap.deallocate(block_128_1, Layout::from_size_align(128, 128).unwrap());
            heap.deallocate(block_128_0, Layout::from_size_align(128, 128).unwrap());

            // And allocate the whole heap, just to make sure everything
            // got cleaned up correctly.
            let block_256_0 = heap
                .allocate(Layout::from_size_align(256, 256).unwrap())
                .unwrap();
            assert_eq!(mem.offset(0), block_256_0);

            std::alloc::dealloc(mem, layout);
        }
    }
}
