# `buddyalloc`: A simple "buddy allocator" for bare-metal Rust

Are you using Rust on bare metal with `#[no_std]`?  Do you lack even a
working `malloc` and `free`?  Would you like to have a Rust-compatible
allocator that works with `core::alloc`?

This is a simple [buddy allocator][] that you can use a drop-in replacement
for Rust's regular allocators.  It's highly experimental and may corrupt
your data, panic your machine, etc.  But it appears to be enough to make
`Vec::push` work, at least in _extremely_ limited testing.

There is a test suite which attempts to allocate and deallocate a bunch of
memory, and which tries to make sure everything winds up at the expected
location in memory each time.

This library was originally based on the work done in the [toyos][]
repository.

[buddy allocator]: https://en.wikipedia.org/wiki/Buddy_memory_allocation
[toyos]: https://github.com/emk/toyos-rs

## Using this allocator
```rust
// This can be a block of free system memory on your microcontroller.
const HEAP_MEM: usize  = 0xFFF0_0000;
const HEAP_SIZE: usize = 0x0008_0000;

let mut heap: Heap<16> = unsafe {
    Heap::new(NonNull::new(HEAP_MEM as *mut u8).unwrap(), HEAP_SIZE).unwrap()
};
let mem = heap.allocate(Layout::from_size_align(16, 16).unwrap()).unwrap();

// Yay! We have a 16-byte block of memory from the heap.
```

### Static initialization
This allocator does not have to be initialized at runtime!

```rust
const HEAP_MEM: usize  = 0xFFF0_0000;
const HEAP_SIZE: usize = 0x0008_0000;

// You'll want to wrap this heap in a lock abstraction for real-world use.
static mut ALLOCATOR: Heap<16> = unsafe {
    Heap::new_unchecked(HEAP_MEM as *mut u8, HEAP_SIZE)
};

pub fn some_func() {
  let mem = unsafe {
    ALLOCATOR.allocate(Layout::from_size_align(16, 16).unwrap()).unwrap()
  };

  // Yay! We now have a 16-byte block from the heap without initializing it!
}
```

See the [allocator][] example for a more complete idea of how to use this heap.

[allocator]: examples/allocator.rs

## Why this crate over the original buddy allocator?
The last change made to the [original][] crate was back in 2016. It uses
more unsafe code than I felt comfortable with, does not have detailed
error reporting, and does not have an interface compatible with Rust's
allocator trait.

The new version minimizes the amount of unsafe code, preferring to return
error codes if the API is misused (as well as providing unchecked variants).

In addition, every possible error condition now has an associated error
enum, ensuring reliable error reporting (which is important when you
can't attach a debugger to an embedded system).

And finally, since we report all possible error conditions, I've
removed all panicking statements from this crate.

[original]: https://github.com/emk/toyos-rs/tree/master/crates/alloc_buddy_simple

## Licensing

Licensed under the [Apache License, Version 2.0][LICENSE-APACHE] or the
[MIT license][LICENSE-MIT], at your option.  This is HIGHLY EXPERIMENTAL
CODE PROVIDED "AS IS", AND IT MAY DO HORRIBLE THINGS TO YOUR COMPUTER OR
DATA.

[LICENSE-APACHE]: http://www.apache.org/licenses/LICENSE-2.0
[LICENSE-MIT]: http://opensource.org/licenses/MIT
