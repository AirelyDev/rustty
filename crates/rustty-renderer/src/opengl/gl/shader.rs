//! PORTS: ghostty `pkg/opengl/Shader.zig` (56 loc), in full.
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): see `draw.rs`'s module note — the
//! reference force-unwraps every glad slot, the `gl` crate panics identically on
//! an unloaded entry point, so the calls go straight through.
//!
//! PORT NOTE (by-value `self`): the reference's methods take `s: Shader` BY VALUE
//! — it is a single-`GLuint` plain struct. [`Shader`] is therefore `Copy` and its
//! methods take `self` by value, matching `Program.zig:30-36`'s `vs`/`fs` usage.
//!
//! PORT NOTE (`pub inline fn`): every fn in this file is `inline`. That is a Zig
//! codegen directive with no semantic content; `#[inline]` is deliberately NOT
//! added, since these are one-call FFI shims that LLVM inlines on its own and the
//! reference's `inline` carries no observable behaviour to port.

use std::ffi::CStr;
use std::fmt;

use gl::types::{GLchar, GLenum, GLint, GLsizei, GLuint};

use super::{errors, log_debug, log_err};

/// The size of the stack buffer [`Shader::get_info_log`] reads into.
///
/// PORTS: the `512` of `var msg: [512]u8 = undefined;` (`Shader.zig:47`).
const INFO_LOG_LEN: usize = 512;

/// The error [`Shader::set_source_and_compile`] can produce.
///
/// PORTS: the INFERRED error set of `setSourceAndCompile` (`Shader.zig:25`),
/// whose sole member is `error.CompileFailed` (`Shader.zig:37`) — the fn calls no
/// `getError`, so no `errors.Error` member can reach it. The one-variant enum is
/// that set exactly.
///
/// PORT NOTE: deliberately NOT given a `Gl(errors::Error)` variant of the kind
/// `sys::Error` carries. `sys::Error` needs one because `prepareContext`'s
/// inferred set genuinely IS wider than its own members; this one is not, and an
/// unconstructible variant would oblige every caller to match an arm the
/// reference can never produce.
///
/// [`Shader::create`] is explicitly `errors.Error!Shader` (`Shader.zig:13`), so it
/// returns [`errors::Error`] directly rather than this.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// PORTS: `Shader.zig:37` `return error.CompileFailed;`.
    ///
    /// PORT NOTE: Zig error names are globally interned, so this IS the same
    /// error value as `Program.zig:72`'s `error.CompileFailed`. Rust has no such
    /// interning, so it appears once per module's error enum and
    /// [`program::Error`](super::program::Error) maps it across.
    CompileFailed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // `@errorName`'s rendering, minus Zig's `error.` prefix — the same
            // convention `errors::Error`'s Display uses.
            Self::CompileFailed => f.write_str("CompileFailed"),
        }
    }
}

impl std::error::Error for Error {}

/// PORTS: `Shader.zig:1-11` — `const Shader = @This(); … id: c.GLuint`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shader {
    pub id: GLuint,
}

impl Shader {
    /// PORTS: `Shader.zig:13-22`.
    ///
    /// ```zig
    /// pub inline fn create(typ: c.GLenum) errors.Error!Shader {
    ///     const id = glad.context.CreateShader.?(typ);
    ///     if (id == 0) {
    ///         try errors.mustError();
    ///         unreachable;
    ///     }
    ///
    ///     log.debug("shader created id={}", .{id});
    ///     return Shader{ .id = id };
    /// }
    /// ```
    ///
    /// PORT NOTE (the `unreachable`): `errors.mustError()` ALWAYS returns an
    /// error (`errors.zig:28-33`), so `try` always propagates and the line after
    /// it is genuinely unreachable. [`errors::must_error`] has the same total
    /// behaviour, so the `?` always propagates here too — but the Rust compiler
    /// cannot know that from the `Result` type alone, so the `unreachable!()` is
    /// spelled out exactly as the reference spells it.
    ///
    /// Called with `c.GL_VERTEX_SHADER`/`c.GL_FRAGMENT_SHADER`
    /// (`Program.zig:30,34`) — the only two call sites in the reference.
    pub fn create(typ: GLenum) -> Result<Self, errors::Error> {
        // SAFETY: `glCreateShader` takes an enum and returns a name. An
        // unrecognized type is GL_INVALID_ENUM + a 0 return, handled below.
        let id = unsafe { gl::CreateShader(typ) };
        if id == 0 {
            errors::must_error()?;
            unreachable!("errors::must_error returned Ok");
        }

        log_debug!("shader created id={id}");
        Ok(Shader { id })
    }

    /// Set the source and compile a shader.
    //
    // PORTS: `Shader.zig:24-38`.
    //
    // ```zig
    // pub inline fn setSourceAndCompile(s: Shader, source: [:0]const u8) !void {
    //     glad.context.ShaderSource.?(s.id, 1, &@as([*c]const u8, @ptrCast(source)), null);
    //     glad.context.CompileShader.?(s.id);
    //
    //     // Check if compilation succeeded
    //     var success: c_int = undefined;
    //     glad.context.GetShaderiv.?(s.id, c.GL_COMPILE_STATUS, &success);
    //     if (success == c.GL_TRUE) return;
    //     log.err("shader compilation failure id={} message={s}", .{
    //         s.id,
    //         std.mem.sliceTo(&s.getInfoLog(), 0),
    //     });
    //     return error.CompileFailed;
    // }
    // ```
    //
    // PORT NOTE (`source: [:0]const u8` -> `source: &CStr`): a Zig
    // sentinel-terminated slice IS a NUL-terminated string, which is `&CStr` in
    // Rust. This matters beyond spelling: the `null` `length` argument tells GL to
    // read each source string up to its NUL, so the NUL-termination is REQUIRED
    // for correctness, and `&CStr` makes that a type-level guarantee rather than a
    // convention. Task 7's GLSL assets become `c"…"` literals accordingly.
    //
    // PORT NOTE (`success == c.GL_TRUE`): compared against GL_TRUE, NOT `!= 0`.
    // Ported exactly — `glGetShaderiv(GL_COMPILE_STATUS)` is specified to write
    // GL_TRUE or GL_FALSE, so the two agree, but the reference's spelling is kept.
    pub fn set_source_and_compile(self, source: &CStr) -> Result<(), Error> {
        // `&@as([*c]const u8, @ptrCast(source))`: the ADDRESS of a one-element
        // array of `const char *`, paired with the count 1 — i.e. GL's
        // (count, array-of-strings, array-of-lengths) triple with a single string.
        let source_ptr: *const GLchar = source.as_ptr();
        // SAFETY: `count` (= 1) matches the one-element array `&source_ptr` points
        // at, and `source_ptr` is NUL-terminated and live for the call. A null
        // `length` is explicitly legal and means "each string is NUL-terminated".
        // GL copies the source before returning; it retains nothing.
        unsafe { gl::ShaderSource(self.id, 1, &source_ptr, std::ptr::null()) };
        // SAFETY: compiles the source just attached to a name this `Shader` owns.
        unsafe { gl::CompileShader(self.id) };

        // Check if compilation succeeded
        //
        // PORT NOTE (`var success: c_int = undefined`): Rust has no safe
        // `undefined`, so the slot is zero-initialized. Behaviourally identical —
        // `glGetShaderiv` unconditionally overwrites it — and unobservable, since
        // it is read only after the call. (Same handling as `Buffer::create`'s.)
        let mut success: GLint = 0;
        // SAFETY: `GL_COMPILE_STATUS` writes exactly one GLint through the
        // pointer; `&mut success` is one live, aligned GLint.
        unsafe { gl::GetShaderiv(self.id, gl::COMPILE_STATUS, &mut success) };
        if success == GLint::from(gl::TRUE) {
            return Ok(());
        }
        log_err!(
            "shader compilation failure id={} message={}",
            self.id,
            // `std.mem.sliceTo(&s.getInfoLog(), 0)` + `{s}`.
            display_info_log(&self.get_info_log()),
        );
        Err(Error::CompileFailed)
    }

    /// getInfoLog returns the info log for this shader. This attempts to
    /// keep the log fully stack allocated and is therefore limited to a max
    /// amount of elements.
    //
    // NOTE(mitchellh): we can add a dynamic version that uses an allocator
    // if we ever need it.
    //
    // (Both comments above are the reference's own, `Shader.zig:40-45`.)
    //
    // PORTS: `Shader.zig:46-50`.
    //
    // ```zig
    // pub inline fn getInfoLog(s: Shader) [512]u8 {
    //     var msg: [512]u8 = undefined;
    //     glad.context.GetShaderInfoLog.?(s.id, msg.len, null, &msg);
    //     return msg;
    // }
    // ```
    //
    // PORT NOTE (`var msg: [512]u8 = undefined`): zero-initialized, as Rust has no
    // safe `undefined`. Unlike the other `undefined` slots in this port this is
    // NOT merely equivalent, it is strictly safer: `glGetShaderInfoLog` writes a
    // NUL terminator whenever `maxLength > 0`, so the reference is fine on a
    // conformant driver — but if a driver ever wrote nothing, the reference would
    // hand back 512 undefined bytes and `sliceTo(…, 0)` would run off into
    // garbage. Zeroing makes the returned buffer NUL-terminated unconditionally.
    pub fn get_info_log(self) -> [u8; INFO_LOG_LEN] {
        let mut msg = [0u8; INFO_LOG_LEN];
        // SAFETY: `maxLength` is `msg`'s true length, so GL writes at most that
        // many bytes into it (NUL terminator included). A null `length` out-param
        // is explicitly legal and simply means "don't report the length written".
        unsafe {
            gl::GetShaderInfoLog(
                self.id,
                INFO_LOG_LEN as GLsizei,
                std::ptr::null_mut(),
                msg.as_mut_ptr().cast::<GLchar>(),
            );
        }
        msg
    }

    /// PORTS: `Shader.zig:52-56`.
    ///
    /// ```zig
    /// pub inline fn destroy(s: Shader) void {
    ///     assert(s.id != 0);
    ///     glad.context.DeleteShader.?(s.id);
    ///     log.debug("shader destroyed id={}", .{s.id});
    /// }
    /// ```
    ///
    /// PORT NOTE: takes `self` BY VALUE, mirroring the reference — this is NOT a
    /// `Drop` impl, for the same reasons `Buffer::destroy` is not (the reference
    /// manages GL object lifetimes explicitly via `defer`, and `Shader` is `Copy`,
    /// so an RAII `Drop` would double-delete).
    ///
    /// PORT NOTE (`assert(s.id != 0)`): `std.debug.assert` is compiled out in
    /// ReleaseFast; `debug_assert!` is compiled out in release. Same contract.
    pub fn destroy(self) {
        debug_assert!(self.id != 0);
        // SAFETY: deletes a name this `Shader` owns. Deleting an already-deleted
        // or unknown name is silently ignored by GL, not UB.
        unsafe { gl::DeleteShader(self.id) };
        log_debug!("shader destroyed id={}", self.id);
    }
}

/// Renders a `[512]u8` info log for `{s}`, stopping at the first NUL.
///
/// PORTS: `std.mem.sliceTo(&s.getInfoLog(), 0)` (`Shader.zig:35`,
/// `Program.zig:70`) composed with `{s}`'s formatting.
///
/// PORT NOTE: `{s}` on a `[]u8` prints the bytes as-is. `from_utf8_lossy` is the
/// nearest total equivalent — driver info logs are ASCII in practice, and a
/// non-UTF-8 byte must not be allowed to panic a diagnostic path.
pub(super) fn display_info_log(log: &[u8]) -> std::borrow::Cow<'_, str> {
    // `sliceTo(…, 0)`: everything before the first NUL. `GetShaderInfoLog` always
    // NUL-terminates, but an unterminated buffer must still not read out of
    // bounds, so a missing NUL falls back to the whole buffer — which is exactly
    // what `sliceTo` does.
    let end = log.iter().position(|&b| b == 0).unwrap_or(log.len());
    String::from_utf8_lossy(&log[..end])
}

#[cfg(test)]
mod tests {
    use super::display_info_log;

    /// RUSTTY-ONLY. `sliceTo(&msg, 0)` is what turns the fixed 512-byte buffer
    /// into the message the reference logs; getting it wrong prints NUL padding
    /// or truncates the driver's diagnostic. No GL context needed.
    #[test]
    fn info_log_display_stops_at_the_first_nul() {
        let mut log = [0u8; 16];
        log[..5].copy_from_slice(b"boom!");
        assert_eq!(display_info_log(&log), "boom!");

        // An all-NUL buffer (a driver with nothing to say) is the empty string,
        // not 16 NULs.
        assert_eq!(display_info_log(&[0u8; 16]), "");

        // No NUL at all must not read past the end.
        assert_eq!(display_info_log(b"unterminated"), "unterminated");
    }
}
