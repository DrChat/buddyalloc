//! A simple heap based on a buddy allocator.  For the theory of buddy
//! allocators, see <https://en.wikipedia.org/wiki/Buddy_memory_allocation>
//!
//! This can either be used as a standalone library, or as a replacement
//! for Rust's system allocator.  It runs on top of `libcore`, so it can be
//! used on bare metal or in kernel space.
//!
//! Note that our `Heap` API is unstable.

#![no_std]
#![feature(const_mut_refs, const_raw_ptr_deref, const_raw_ptr_to_usize_cast)]

pub use heap::*;

mod heap;
mod math;
