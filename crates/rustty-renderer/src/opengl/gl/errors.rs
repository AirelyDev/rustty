//! PORTS: ghostty `pkg/opengl/errors.zig` (33 loc), in full.

use std::fmt;

/// PORTS: `errors.zig:5-13`.
///
/// ```zig
/// pub const Error = error{
///     InvalidEnum,
///     InvalidValue,
///     InvalidOperation,
///     InvalidFramebufferOperation,
///     OutOfMemory,
///
///     Unknown,
/// };
/// ```
///
/// PORT NOTE: a Zig error SET becomes a plain fieldless Rust enum, and every
/// `!void`/`Error!void` return in `pkg/opengl` becomes `Result<_, Error>` (the
/// brief's stated mapping). The variant names are carried over verbatim.
///
/// `Copy` because a Zig error is a bare integer tag that callers pass around
/// freely; `PartialEq`/`Eq` so tests can name the expected variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    InvalidEnum,
    InvalidValue,
    InvalidOperation,
    InvalidFramebufferOperation,
    OutOfMemory,

    Unknown,
}

impl fmt::Display for Error {
    /// PORT NOTE: no reference analogue — `pkg/opengl` never formats these. The
    /// rendering is `@errorName`'s (i.e. the variant's own spelling), so that a
    /// call site which logs one prints what the reference's `{}` on an error
    /// would print, minus Zig's `error.` prefix.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidEnum => "InvalidEnum",
            Self::InvalidValue => "InvalidValue",
            Self::InvalidOperation => "InvalidOperation",
            Self::InvalidFramebufferOperation => "InvalidFramebufferOperation",
            Self::OutOfMemory => "OutOfMemory",
            Self::Unknown => "Unknown",
        })
    }
}

impl std::error::Error for Error {}

/// getError returns the error (if any) from the last OpenGL operation.
//
// PORTS: `errors.zig:15-26`.
//
// ```zig
// pub fn getError() Error!void {
//     return switch (glad.context.GetError.?()) {
//         c.GL_NO_ERROR => {},
//         c.GL_INVALID_ENUM => Error.InvalidEnum,
//         c.GL_INVALID_VALUE => Error.InvalidValue,
//         c.GL_INVALID_OPERATION => Error.InvalidOperation,
//         c.GL_INVALID_FRAMEBUFFER_OPERATION => Error.InvalidFramebufferOperation,
//         c.GL_OUT_OF_MEMORY => Error.OutOfMemory,
//         else => Error.Unknown,
//     };
// }
// ```
//
// PORT NOTE (`glad.context.GetError.?()`): the `.?` force-unwraps glad's slot,
// i.e. the reference PANICS if `glGetError` is absent. The `gl` crate's unloaded
// entry points panic identically ("glGetError was not loaded"), so calling it
// straight through — with no `is_loaded()` guard — is the faithful port, not a
// gap. `glGetError` is core GL 1.0: any context that loaded at all has it.
//
// PORT NOTE (the `else` arm): `GL_STACK_OVERFLOW`/`GL_STACK_UNDERFLOW`/
// `GL_CONTEXT_LOST` are real `glGetError` returns that the reference deliberately
// does not name; they land on `else => Error.Unknown`, and do so here too.
pub fn get_error() -> Result<(), Error> {
    // SAFETY: `glGetError` takes no arguments, returns an enum, and is valid to
    // call on any current context. It is UB only with no current context, which
    // is the same precondition the reference (and all of `pkg/opengl`) assumes.
    match unsafe { gl::GetError() } {
        gl::NO_ERROR => Ok(()),
        gl::INVALID_ENUM => Err(Error::InvalidEnum),
        gl::INVALID_VALUE => Err(Error::InvalidValue),
        gl::INVALID_OPERATION => Err(Error::InvalidOperation),
        gl::INVALID_FRAMEBUFFER_OPERATION => Err(Error::InvalidFramebufferOperation),
        gl::OUT_OF_MEMORY => Err(Error::OutOfMemory),
        _ => Err(Error::Unknown),
    }
}

/// mustError just calls getError but always results in an error being returned.
/// If getError has no error, then Unknown is returned.
//
// PORTS: `errors.zig:28-33`.
//
// ```zig
// pub fn mustError() Error!void {
//     try getError();
//     return Error.Unknown;
// }
// ```
//
// PORT NOTE: unused by the reference (no call site in `pkg/opengl` or `src/`),
// carried over anyway per the plan's "port whole files" convention. `pub` in a
// `pub` module, so it is public API rather than dead code.
pub fn must_error() -> Result<(), Error> {
    get_error()?;
    Err(Error::Unknown)
}
