//! PORTS: ghostty `pkg/opengl/extensions.zig` (32 loc), in full.
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): see `draw.rs`'s module note.
//!
//! PORT NOTE (unused by the milestone): nothing in `src/renderer` calls this file
//! — it is carried over per the plan's "port whole files" convention, the same
//! basis on which `errors::must_error` and `buffer::Binding::set_data_null` are
//! ported. Everything here is `pub` in a `pub` module, so it is public API rather
//! than dead code.

use std::ffi::CStr;

use gl::types::{GLint, GLuint};

use super::errors;
use super::glad;

/// Returns the number of extensions.
//
// PORTS: `extensions.zig:6-12`.
//
// ```zig
// pub fn len() !u32 {
//     var n: c.GLint = undefined;
//     glad.context.GetIntegerv.?(c.GL_NUM_EXTENSIONS, &n);
//     try errors.getError();
//     return @intCast(n);
// }
// ```
//
// PORT NOTE (`var n: c.GLint = undefined`): zero-initialized here; see
// `buffer::Buffer::create`'s note.
pub fn len() -> Result<u32, errors::Error> {
    let mut n: GLint = 0;
    // SAFETY: `GL_NUM_EXTENSIONS` is a single-valued query, so `glGetIntegerv`
    // writes exactly one GLint through the pointer; `&mut n` is one live, aligned
    // GLint.
    glad::context(|gl| unsafe { gl.GetIntegerv(gl::NUM_EXTENSIONS, &mut n) });
    errors::get_error()?;
    // `@intCast(n)`: GLint -> u32, trapping on a negative in Zig. `try_from`
    // panics identically. A count is never negative.
    Ok(u32::try_from(n).expect("GL_NUM_EXTENSIONS returned a negative count"))
}

/// Returns an iterator for the extensions.
//
// PORTS: `extensions.zig:14-17`.
//
// ```zig
// pub fn iterator() !Iterator {
//     return Iterator{ .len = try len() };
// }
// ```
//
// PORT NOTE (`.i` defaulted): the reference relies on `i: c.GLuint = 0`'s field
// default (`extensions.zig:23`); the initializer is explicit here.
pub fn iterator() -> Result<Iterator, errors::Error> {
    Ok(Iterator { len: len()?, i: 0 })
}

/// Iterator for the available extensions.
//
// PORTS: `extensions.zig:19-32`.
//
// ```zig
// pub const Iterator = struct {
//     /// The total number of extensions.
//     len: c.GLuint = 0,
//     i: c.GLuint = 0,
//     …
// };
// ```
//
// PORT NOTE (the NAME `Iterator`): kept verbatim, even though it shadows the
// `std::iter::Iterator` trait's name inside this module. It is `extensions::
// Iterator` at every use site, exactly as it is `extensions.Iterator` in the
// reference, and the trait is reachable as `std::iter::Iterator` regardless.
//
// PORT NOTE (why NOT `impl std::iter::Iterator`): `next` is FALLIBLE here —
// `!?[]const u8`, i.e. `Result<Option<…>>` — and the std trait's `next` returns a
// bare `Option`. Implementing it would mean either swallowing the `getError` the
// reference performs on every step, or inventing a `Result`-yielding item type
// whose `?` behaviour differs from the reference's `try`. An inherent `next`
// keeps the reference's control flow one-for-one; a `for` loop becomes
// `while let Some(x) = it.next()?`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Iterator {
    /// The total number of extensions.
    pub len: GLuint,
    pub i: GLuint,
}

impl Iterator {
    /// PORTS: `extensions.zig:25-31`.
    ///
    /// ```zig
    /// pub fn next(self: *Iterator) !?[]const u8 {
    ///     if (self.i >= self.len) return null;
    ///     const res = glad.context.GetStringi.?(c.GL_EXTENSIONS, self.i);
    ///     try errors.getError();
    ///     self.i += 1;
    ///     return std.mem.sliceTo(res, 0);
    /// }
    /// ```
    ///
    /// PORT NOTE (`self: *Iterator` -> `&mut self`): the ONE type in this package
    /// whose receiver is genuinely mutable — it advances `self.i`. Unlike
    /// `texture::Binding::unbind`'s incidental `*const`, this pointer is
    /// load-bearing and ports to `&mut self`.
    ///
    /// PORT NOTE (`std.mem.sliceTo(res, 0)` -> `CStr::from_ptr(…).to_bytes()`):
    /// both walk to the first NUL and yield the bytes BEFORE it, exclusive.
    ///
    /// PORT NOTE (the `'static` lifetime): the returned slice borrows a string the
    /// DRIVER owns, which lives as long as the GL context. Zig's `[]const u8`
    /// carries no lifetime at all, so there is nothing to port here and no
    /// borrow to tie it to — `Iterator` holds no reference. `'static` is the
    /// honest description of the only lifetime the caller can observe, and it
    /// inherits the same context-outlives-the-slice precondition that every
    /// function in this package already assumes (see `errors::get_error`'s note).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<&'static [u8]>, errors::Error> {
        if self.i >= self.len {
            return Ok(None);
        }
        // SAFETY: `glGetStringi` with an in-range index returns a pointer to a
        // NUL-terminated string owned by the driver. The bounds check above is what
        // makes the index in-range; an out-of-range one would return null and raise
        // GL_INVALID_VALUE, which is why the `getError` below precedes any read.
        let res = glad::context(|gl| unsafe { gl.GetStringi(gl::EXTENSIONS, self.i) });
        errors::get_error()?;
        self.i += 1;
        // SAFETY: `res` is a valid, NUL-terminated, driver-owned C string — the
        // `getError` above rules out the null/error case before it is read.
        Ok(Some(unsafe { CStr::from_ptr(res.cast()) }.to_bytes()))
    }
}
