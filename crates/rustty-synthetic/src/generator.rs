//! A common interface for all generators.
//!
//! PORT NOTE: `synthetic/Generator.zig`. The Zig `Generator` is a `{ptr, nextFn}`
//! vtable that type-erases the concrete generator so `Bytes`/`Osc`/`Utf8` share
//! one type. We reproduce the vtable as a `ptr + next_fn` pair, generic over the
//! writer `W` (Zig's `*std.Io.Writer` is a single runtime interface type; in
//! Rust we keep it generic so the same `Generator` works with any writer). The
//! `init` asserts about pointer kind become compile-time guarantees of the Rust
//! type system (no-ops here).

use std::marker::PhantomData;

pub use crate::io::{Writer, WriterExt};

/// For generators, this is the only error that is allowed to be
/// returned by the next function.
///
/// PORT NOTE: `pub const Error = error{WriteFailed}`. In the generic port the error
/// is the writer's own `Error`; for the fixed-buffer writers the generators run
/// against, that error IS `WriteFailed`.
pub type Error = crate::io::WriteFailed;

/// The vtable for the generator.
pub struct Generator<'a, W: Writer> {
    ptr: *mut (),
    next_fn: fn(*mut (), &mut W, usize) -> Result<(), W::Error>,
    _marker: PhantomData<&'a mut ()>,
}

impl<'a, W: Writer> Generator<'a, W> {
    /// Create a new generator from a pointer and a function pointer.
    /// This usually is only called by generator implementations, not
    /// generator users.
    ///
    /// PORT NOTE: the `comptime nextFn` closure that `@ptrCast`s `ptr` back to the
    /// concrete generator is the caller-supplied `next_fn` trampoline here. The
    /// Zig `init` asserts on the pointer become compile-time guarantees:
    // assert(@typeInfo(Ptr) == .pointer); // Must be a pointer
    // assert(@typeInfo(Ptr).pointer.size == .one); // Must be a single-item pointer
    // assert(@typeInfo(@typeInfo(Ptr).pointer.child) == .@"struct"); // Must point to a struct
    pub fn init(ptr: *mut (), next_fn: fn(*mut (), &mut W, usize) -> Result<(), W::Error>) -> Self {
        Generator {
            ptr,
            next_fn,
            _marker: PhantomData,
        }
    }

    /// Get the next value from the generator. Returns the data written.
    pub fn next(&self, writer: &mut W, max_size: usize) -> Result<(), W::Error> {
        (self.next_fn)(self.ptr, writer, max_size)
    }
}
