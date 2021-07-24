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

## Using this allocator
Pull this allocator using Cargo, and see the [allocator.rs][] example for a good idea of how to use this heap.

[allocator.rs]: examples/allocator.rs

## Warning

This has only been run in the "low half" of memory, and if you store your
heap in the upper half of your memory range, you may run into some issues
with `isize` versus `usize`.

## Licensing

Licensed under the [Apache License, Version 2.0][LICENSE-APACHE] or the
[MIT license][LICENSE-MIT], at your option.  This is HIGHLY EXPERIMENTAL
CODE PROVIDED "AS IS", AND IT MAY DO HORRIBLE THINGS TO YOUR COMPUTER OR
DATA.  But if you're using random unsafe, unstable Rust libraries in
implementing a panicking version of `malloc` in kernel space, you probably
knew that already.

[LICENSE-APACHE]: http://www.apache.org/licenses/LICENSE-2.0
[LICENSE-MIT]: http://opensource.org/licenses/MIT
