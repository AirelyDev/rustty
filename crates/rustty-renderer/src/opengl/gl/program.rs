//! PORTS: ghostty `pkg/opengl/Program.zig` (137 loc).
//!
//! PORT NOTE (`glad.context.<Fn>.?(…)`): see `draw.rs`'s module note — the
//! reference force-unwraps every glad slot, the `gl` crate panics identically on
//! an unloaded entry point, so the calls go straight through.
//!
//! PORT NOTE (by-value `self`): the reference's methods take `p: Program` BY VALUE
//! — it is a single-`GLuint` plain struct. [`Program`] is therefore `Copy` and its
//! methods take `self` by value, so `Pipeline.zig:42-49`'s `program.use()` after
//! `errdefer program.destroy()` ports with its spelling intact.
//!
//! PORT NOTE (`setUniform`, `Program.zig:90-125`): **NOT PORTED**, deliberately.
//! Its `value: anytype` is dispatched by a `switch (@TypeOf(value))` over SEVEN
//! distinct types (`bool`, `comptime_int`, `f32`, `@Vector(2|3|4, f32)`,
//! `[4]@Vector(4, f32)`), each calling a different `glUniform*`. Unlike
//! `Buffer.zig`'s `dataInfo` — whose `anytype` collapsed to the single `.slice`
//! arm every call site actually used (`buffer.rs`'s note) — there is no single
//! reachable arm to collapse to here, and Rust cannot express "any one of seven
//! unrelated types" as one parameter without inventing a `UniformValue` trait,
//! which the plan forbids. Crucially there is also nothing to validate an invented
//! shape against: `setUniform` has **zero call sites** in the entire reference
//! tree (`grep -rn setUniform --include=*.zig` finds only its own definition) —
//! the live uniform path is the UBO one, `RenderPass.zig:95`'s
//! `ubo.bindBase(.uniform, 1)`, which `buffer.rs` already ports. So porting it now
//! would mean INVENTING an abstraction AND an API surface with no oracle for
//! either. It is recoverable with zero churn to everything else the moment a call
//! site exists to name the arms it needs. Same disposition, and for a closely
//! related reason, as `Buffer.Binding.attribute` (see `buffer.rs`'s module note).
//!
//! PORT NOTE (`uniformBlockBinding`, `Program.zig:81-88`): likewise uncalled in
//! the reference, but its parameters are concrete, so it IS ported — "port whole
//! files", per `errors.rs`'s `must_error` and `buffer.rs`'s `set_data_null`.

use std::ffi::CStr;
use std::fmt;

use gl::types::{GLchar, GLint, GLsizei, GLuint};

use super::shader::{self, display_info_log, Shader};
use super::{errors, log_debug, log_err};

/// The size of the stack buffer [`Program::get_info_log`] reads into.
///
/// PORTS: the `512` of `var msg: [512]u8 = undefined;` (`Program.zig:134`).
const INFO_LOG_LEN: usize = 512;

/// The errors this module can produce.
///
/// PORT NOTE: the union of the inferred error sets of `Program.zig`'s fns —
/// `errors.Error` (via `mustError`/`getError`), `error.CompileFailed`
/// (`Program.zig:72`) and `error.UniformNameInvalid` (`Program.zig:101`). Zig
/// merges these automatically because its error names are globally interned; Rust
/// has no such interning, so the union is spelled out once here. Same shape as
/// `sys::Error` and [`shader::Error`].
///
/// The fns whose reference signature is explicitly `errors.Error!T`, or whose
/// inferred set is exactly `errors.Error`, return [`errors::Error`] directly
/// rather than this — see [`Program::create`], [`Program::attach_shader`],
/// [`Program::use_`], [`Program::uniform_block_binding`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// PORTS: an `errors.Error` member arriving through `errors.mustError()`
    /// (`Program.zig:22`) or `errors.getError()` (`Program.zig:54,77,87,103,124`).
    Gl(errors::Error),
    /// PORTS: `Program.zig:72` `return error.CompileFailed;` — the LINK failure.
    ///
    /// PORT NOTE: the reference reuses `error.CompileFailed` for a link failure
    /// rather than declaring an `error.LinkFailed`; Zig's global interning makes
    /// it literally the same error value as `Shader.zig:37`'s. That conflation is
    /// ported as-is: a `CompileFailed` out of [`Program::create_vf`] means "a
    /// stage failed to compile, OR the program failed to link", exactly as it does
    /// in the reference. See [`From<shader::Error>`](Error::from).
    CompileFailed,
    /// PORTS: `Program.zig:101` `return error.UniformNameInvalid;`.
    ///
    /// PORT NOTE: unconstructible here, because its only source is the unported
    /// `setUniform` (see the module docs). Carried so that the module's error
    /// type IS the reference's inferred union, and so that porting `setUniform`
    /// later adds no variant.
    UniformNameInvalid,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gl(err) => write!(f, "OpenGL error: {err}"),
            // `@errorName`'s rendering, minus Zig's `error.` prefix.
            Self::CompileFailed => f.write_str("CompileFailed"),
            Self::UniformNameInvalid => f.write_str("UniformNameInvalid"),
        }
    }
}

impl std::error::Error for Error {}

impl From<errors::Error> for Error {
    fn from(err: errors::Error) -> Self {
        Self::Gl(err)
    }
}

impl From<shader::Error> for Error {
    /// PORT NOTE: no reference analogue — Zig needs no conversion, since
    /// `Shader.setSourceAndCompile`'s errors are already members of `createVF`'s
    /// inferred set (same globally-interned values). This is that merge, spelled
    /// out: each member maps to the identically-named one.
    fn from(err: shader::Error) -> Self {
        match err {
            shader::Error::CompileFailed => Self::CompileFailed,
        }
    }
}

/// PORTS: `Program.zig:1-12` — `const Program = @This(); … id: c.GLuint`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Program {
    pub id: GLuint,
}

/// PORTS: `Program.zig:14-18`.
///
/// ```zig
/// pub const Binding = struct {
///     pub fn unbind(_: Binding) void {
///         glad.context.UseProgram.?(0);
///     }
/// };
/// ```
///
/// PORT NOTE: a FIELDLESS struct — unlike `Buffer.Binding` it carries no state at
/// all, because unbinding a program needs no target and no name. It is purely the
/// token that makes `defer pbind.unbind()` (`Pipeline.zig:49`) expressible.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Binding;

impl Binding {
    /// PORTS: `Program.zig:15-17`.
    ///
    /// PORT NOTE (`_: Binding`): the reference DISCARDS the receiver — see
    /// `buffer.rs`'s note on `Binding::attribute_divisor`. Rust only grants
    /// method-call syntax to a receiver literally named `self`, so it is named and
    /// simply goes unused.
    pub fn unbind(self) {
        // SAFETY: `glUseProgram(0)` unbinds whatever program is current and always
        // succeeds outside of a transform-feedback block.
        unsafe { gl::UseProgram(0) };
    }
}

impl Program {
    /// PORTS: `Program.zig:20-25`.
    ///
    /// ```zig
    /// pub fn create() !Program {
    ///     const id = glad.context.CreateProgram.?();
    ///     if (id == 0) try errors.mustError();
    ///     log.debug("program created id={}", .{id});
    ///     return .{ .id = id };
    /// }
    /// ```
    ///
    /// PORT NOTE (no `unreachable`): unlike `Shader.create` (`Shader.zig:16-17`),
    /// the reference does NOT write `unreachable` after `try errors.mustError()`
    /// here — it simply falls through to the `log.debug`. The fall-through is
    /// dead either way, since `mustError` always returns an error, so `try`
    /// always propagates. Ported with the same shape: `must_error()?` always
    /// returns `Err`, so nothing below it runs when `id == 0`. This asymmetry is
    /// the reference's, not an omission.
    ///
    /// PORT NOTE (`!Program`): the inferred set here is exactly `errors.Error`
    /// (`mustError` is the only fallible call), so the return type is
    /// [`errors::Error`] rather than this module's wider [`Error`].
    pub fn create() -> Result<Self, errors::Error> {
        // SAFETY: `glCreateProgram` takes no arguments and returns a name, or 0 on
        // failure (handled below).
        let id = unsafe { gl::CreateProgram() };
        if id == 0 {
            errors::must_error()?;
        }
        log_debug!("program created id={id}");
        Ok(Program { id })
    }

    /// Create a program from a vertex and fragment shader source. This will
    /// compile and link the vertex and fragment shader.
    //
    // PORTS: `Program.zig:27-44`.
    //
    // ```zig
    // pub fn createVF(vsrc: [:0]const u8, fsrc: [:0]const u8) !Program {
    //     const vs = try Shader.create(c.GL_VERTEX_SHADER);
    //     try vs.setSourceAndCompile(vsrc);
    //     defer vs.destroy();
    //
    //     const fs = try Shader.create(c.GL_FRAGMENT_SHADER);
    //     try fs.setSourceAndCompile(fsrc);
    //     defer fs.destroy();
    //
    //     const p = try create();
    //     try p.attachShader(vs);
    //     try p.attachShader(fs);
    //     try p.link();
    //
    //     return p;
    // }
    // ```
    //
    // The reference's only call site is `Pipeline.zig:42` (Task 6).
    //
    // PORT NOTE (NO `glDetachShader` — the brief asks this be verified and
    // followed exactly): the reference does NOT detach the shaders after linking.
    // It only `defer`s `vs.destroy()`/`fs.destroy()` (i.e. `glDeleteShader`), and
    // there is no `glDetachShader` ANYWHERE in `pkg/opengl` — the entry point is
    // never even bound. The common "detach then delete" idiom is therefore NOT
    // added: `glDeleteShader` on an attached shader only FLAGS it for deletion,
    // and GL frees it once the program that owns it is deleted, so the reference's
    // sequence leaks nothing and behaves identically. Adding a detach would be
    // inventing behaviour the oracle does not have.
    //
    // PORT NOTE (`defer` -> immediately-invoked closures): Rust has no `defer`, and
    // R3 permits no `scopeguard` dependency. Each `defer` is expressed as "run the
    // rest of the scope, then destroy" — which is what `defer` means — by scoping
    // the remainder into a closure. A `Drop` guard was rejected: it would ALSO run
    // during a panic unwind, whereas a Zig `defer` does not run on panic (Zig
    // panics abort). The closures match the reference on both paths.
    //
    // PORT NOTE (the shader leak on a compile failure): note WHERE the reference
    // registers each `defer` — AFTER the corresponding `setSourceAndCompile`. So
    // if the vertex stage fails to compile, `defer vs.destroy()` was never
    // registered and `vs` leaks; if the fragment stage fails, `fs` leaks (`vs`
    // does not). Likewise there is no `errdefer p.destroy()`, so a link failure
    // leaks the program object. All three are the reference's behaviour and are
    // ported as-is rather than quietly fixed: this is a faithful port, and
    // `createVF` failing means shader compilation is broken, which aborts renderer
    // init anyway. Flagged upward rather than patched here
    // (`.port/parity-closure/UPSTREAM-DEFECTS.md` UD247).
    pub fn create_vf(vsrc: &CStr, fsrc: &CStr) -> Result<Self, Error> {
        let vs = Shader::create(gl::VERTEX_SHADER)?;
        // `vs` leaks if this fails — see the note above; the reference's `defer`
        // is registered only on the next line.
        vs.set_source_and_compile(vsrc)?;

        // defer vs.destroy();
        let result = (|| -> Result<Self, Error> {
            let fs = Shader::create(gl::FRAGMENT_SHADER)?;
            // `fs` leaks if this fails — see the note above.
            fs.set_source_and_compile(fsrc)?;

            // defer fs.destroy();
            let result = (|| -> Result<Self, Error> {
                let p = Self::create()?;
                p.attach_shader(vs)?;
                p.attach_shader(fs)?;
                p.link()?;

                Ok(p)
            })();
            fs.destroy();
            result
        })();
        vs.destroy();
        result
    }

    /// PORTS: `Program.zig:46-50`.
    ///
    /// ```zig
    /// pub fn destroy(p: Program) void {
    ///     assert(p.id != 0);
    ///     glad.context.DeleteProgram.?(p.id);
    ///     log.debug("program destroyed id={}", .{p.id});
    /// }
    /// ```
    ///
    /// PORT NOTE: takes `self` BY VALUE and is NOT a `Drop` impl — see
    /// `Shader::destroy`. `Pipeline.zig:46`'s `errdefer program.destroy()` is the
    /// reference's own lifetime management.
    ///
    /// PORT NOTE (`assert(p.id != 0)`): `std.debug.assert` is compiled out in
    /// ReleaseFast; `debug_assert!` is compiled out in release. Same contract.
    pub fn destroy(self) {
        debug_assert!(self.id != 0);
        // SAFETY: deletes a name this `Program` owns. Deleting an already-deleted
        // or unknown name is silently ignored by GL, not UB. Any shader still
        // attached is freed with it.
        unsafe { gl::DeleteProgram(self.id) };
        log_debug!("program destroyed id={}", self.id);
    }

    /// PORTS: `Program.zig:52-55`.
    ///
    /// ```zig
    /// pub fn attachShader(p: Program, s: Shader) !void {
    ///     glad.context.AttachShader.?(p.id, s.id);
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE (`!void`): the inferred set is exactly `errors.Error`.
    pub fn attach_shader(self, s: Shader) -> Result<(), errors::Error> {
        // SAFETY: attaches a name `s` owns to a name this `Program` owns.
        // Attaching twice, or a non-shader name, is a defined GL error, collected
        // below.
        unsafe { gl::AttachShader(self.id, s.id) };
        errors::get_error()
    }

    /// PORTS: `Program.zig:57-73`.
    ///
    /// ```zig
    /// pub fn link(p: Program) !void {
    ///     glad.context.LinkProgram.?(p.id);
    ///
    ///     // Check if linking succeeded
    ///     var success: c_int = undefined;
    ///     glad.context.GetProgramiv.?(p.id, c.GL_LINK_STATUS, &success);
    ///     if (success == c.GL_TRUE) {
    ///         log.debug("program linked id={}", .{p.id});
    ///         return;
    ///     }
    ///
    ///     log.err("program link failure id={} message={s}", .{
    ///         p.id,
    ///         std.mem.sliceTo(&p.getInfoLog(), 0),
    ///     });
    ///     return error.CompileFailed;
    /// }
    /// ```
    ///
    /// PORT NOTE (`!void`): the inferred set is exactly `error{CompileFailed}` —
    /// `link` calls no `getError`. It returns this module's [`Error`] rather than a
    /// one-member type; only `CompileFailed` is ever constructed here.
    ///
    /// PORT NOTE (`var success: c_int = undefined`): zero-initialized — see
    /// `Shader::set_source_and_compile`.
    pub fn link(self) -> Result<(), Error> {
        // SAFETY: links a name this `Program` owns. A link failure is reported via
        // GL_LINK_STATUS, not as a GL error.
        unsafe { gl::LinkProgram(self.id) };

        // Check if linking succeeded
        let mut success: GLint = 0;
        // SAFETY: `GL_LINK_STATUS` writes exactly one GLint through the pointer;
        // `&mut success` is one live, aligned GLint.
        unsafe { gl::GetProgramiv(self.id, gl::LINK_STATUS, &mut success) };
        if success == GLint::from(gl::TRUE) {
            log_debug!("program linked id={}", self.id);
            return Ok(());
        }

        log_err!(
            "program link failure id={} message={}",
            self.id,
            // `std.mem.sliceTo(&p.getInfoLog(), 0)` + `{s}`.
            display_info_log(&self.get_info_log()),
        );
        Err(Error::CompileFailed)
    }

    /// PORTS: `Program.zig:75-79`.
    ///
    /// ```zig
    /// pub fn use(p: Program) !Binding {
    ///     glad.context.UseProgram.?(p.id);
    ///     try errors.getError();
    ///     return .{};
    /// }
    /// ```
    ///
    /// PORT NOTE (`use` -> `use_`): `use` is a Rust keyword. The trailing
    /// underscore is the brief's named spelling; `r#use` was the alternative and
    /// was not taken, since the brief names this API explicitly and `use_` reads
    /// at the call sites (`Pipeline.zig:48`, `RenderPass.zig:65`) without raw-ident
    /// noise.
    ///
    /// PORT NOTE (`!Binding`): the inferred set is exactly `errors.Error`.
    pub fn use_(self) -> Result<Binding, errors::Error> {
        // SAFETY: makes a name this `Program` owns current. An unlinked or unknown
        // name is a defined GL error, collected below.
        unsafe { gl::UseProgram(self.id) };
        errors::get_error()?;
        Ok(Binding)
    }

    /// PORTS: `Program.zig:81-88`.
    ///
    /// ```zig
    /// pub fn uniformBlockBinding(
    ///     self: Program,
    ///     index: c.GLuint,
    ///     binding: c.GLuint,
    /// ) !void {
    ///     glad.context.UniformBlockBinding.?(self.id, index, binding);
    ///     try errors.getError();
    /// }
    /// ```
    ///
    /// PORT NOTE: uncalled in the reference; carried over per "port whole files"
    /// (see the module docs).
    pub fn uniform_block_binding(
        self,
        index: GLuint,
        binding: GLuint,
    ) -> Result<(), errors::Error> {
        // SAFETY: acts on a name this `Program` owns. A bad block index or an
        // out-of-range binding point is a defined GL error, collected below.
        unsafe { gl::UniformBlockBinding(self.id, index, binding) };
        errors::get_error()
    }

    /// getInfoLog returns the info log for this program. This attempts to
    /// keep the log fully stack allocated and is therefore limited to a max
    /// amount of elements.
    //
    // NOTE(mitchellh): we can add a dynamic version that uses an allocator
    // if we ever need it.
    //
    // (Both comments above are the reference's own, `Program.zig:127-132`.)
    //
    // PORTS: `Program.zig:133-137`.
    //
    // ```zig
    // pub fn getInfoLog(s: Program) [512]u8 {
    //     var msg: [512]u8 = undefined;
    //     glad.context.GetProgramInfoLog.?(s.id, msg.len, null, &msg);
    //     return msg;
    // }
    // ```
    //
    // PORT NOTE (`var msg: [512]u8 = undefined`): zero-initialized — see
    // `Shader::get_info_log`, which carries the full rationale.
    //
    // PORT NOTE (the receiver is named `s`, not `p`, in the reference): a typo of
    // no consequence; named `self` here like every other method.
    pub fn get_info_log(self) -> [u8; INFO_LOG_LEN] {
        let mut msg = [0u8; INFO_LOG_LEN];
        // SAFETY: `maxLength` is `msg`'s true length, so GL writes at most that
        // many bytes into it (NUL terminator included). A null `length` out-param
        // is explicitly legal and simply means "don't report the length written".
        unsafe {
            gl::GetProgramInfoLog(
                self.id,
                INFO_LOG_LEN as GLsizei,
                std::ptr::null_mut(),
                msg.as_mut_ptr().cast::<GLchar>(),
            );
        }
        msg
    }
}
