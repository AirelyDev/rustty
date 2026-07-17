//! Wrapper for handling render pipelines.
//!
//! PORTS: ghostty `src/renderer/opengl/Pipeline.zig` (164 loc) — the renderer-facing
//! pipeline layered over the `gl` package's [`Program`](super::gl::Program),
//! [`Framebuffer`](super::gl::Framebuffer) and [`VertexArray`](super::gl::VertexArray)
//! leaves. This is the OpenGL twin of `metal/pipeline.rs`.
//!
//! PORT NOTE (Copy handles, no ARC): the `gl` leaves are `Copy` `GLuint` handles
//! with explicit `create`/`destroy`, so the reference's `errdefer program.destroy()`
//! etc. become manual cleanup on the error path (there is no `Drop` to lean on, and
//! adding one would be a deviation — see `opengl::texture`). Each Zig `defer`/
//! `errdefer` is reproduced by scoping the remainder of its region into a closure,
//! exactly as `opengl::gl::program::create_vf` does (which carries the full
//! `defer`-as-closure rationale). `deinit` takes `&self` and destroys ONLY the
//! program (`Pipeline.zig:72-74`), leaking the fbo/vao — the reference's behaviour,
//! ported as-is.
//!
//! PORT NOTE (HARD SPOT, autoAttribute): ghostty's `autoAttribute` (`Pipeline.zig:76-164`)
//! uses comptime field reflection (`@typeInfo(T).@"struct".fields`) to build the
//! vertex-attribute format/binding calls from a vertex struct. Rust has no comptime
//! field reflection, so the comptime `?type` `VertexAttributes` becomes a runtime
//! [`Option<VertexLayout>`] — a hand-written per-struct attribute table
//! ([`AttribDesc`] rows + stride) whose entries pre-compute the reference's `FT ->
//! format` switch and `@offsetOf`. This is EXACTLY the shape `metal/pipeline.rs`
//! established (its `VertexAttribute`/`VertexLayoutRef` + `auto_attribute`), with the
//! per-struct `CellText`/`Image`/`BgImage` tables living in the shaders module
//! (`metal::shaders`) beside the pipeline descriptions that select them and the
//! `gpu_abi` structs they pin against. The OpenGL tables likewise belong in
//! `opengl::shaders` (Task 7, which lands `opengl::gpu_abi` — the structs the tables'
//! offsets must be pinned to by `const{}` drift guards, and which do NOT exist yet):
//! this file provides only the MECHANISM ([`AttribDesc`], [`VertexLayout`],
//! [`auto_attribute`], [`Pipeline::init`]), and [`auto_attribute`] reproduces
//! `Pipeline.zig:76-164`'s descriptor-setting loop verbatim.

use gl::types::{GLenum, GLint, GLuint};

use std::ffi::CStr;

use super::gl::framebuffer::{Framebuffer, Target as FbTarget};
use super::gl::program::{self, Program};
use super::gl::vertex_array::{Binding as VaoBinding, VertexArray};

// const log = std.log.scoped(.opengl);
// PORT NOTE: `Pipeline.zig` declares the shared `.opengl` scoped logger but has no
// `log` call sites; preserved as this comment for inventory parity.

/// PORTS: the union of `Pipeline.init`'s inferred error set — `errors.Error` (via
/// the `gl` leaves) plus `error.CompileFailed`/`error.UniformNameInvalid` arriving
/// through `gl.Program.createVF` ([`program::Error`]). Rust has no globally-interned
/// error names, so the merge is spelled out: [`Pipeline::init`] returns exactly the
/// program module's error, which already IS that union.
pub type Error = program::Error;

/// Vertex step function.
//
// PORTS: `Pipeline.zig:23-27` — `Options.StepFunction`. Flattened to a module-scope
// enum (Rust lacks Zig's in-struct namespacing; see `opengl::buffer` for the
// flat-naming rationale).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepFunction {
    Constant,
    PerVertex,
    PerInstance,
}

/// Options for initializing a render pipeline.
//
// PORTS: `Pipeline.zig:10-28`.
//
// PORT NOTE (`vertex_fn`/`fragment_fn: [:0]const u8` -> `&CStr`): the reference
// takes NUL-terminated GLSL source; `gl.Program.createVF` ([`Program::create_vf`])
// takes `&CStr`, so the options carry `&CStr` and port unchanged.
pub struct Options<'a> {
    /// GLSL source of the vertex function
    pub vertex_fn: &'a CStr,
    /// GLSL source of the fragment function
    pub fragment_fn: &'a CStr,

    /// Vertex step function.
    // Zig default `.per_vertex`; callers pass it explicitly.
    pub step_fn: StepFunction,

    /// Whether to enable blending.
    // Zig default `true`; callers pass it explicitly.
    pub blending_enabled: bool,
}

/// Which of the three `glVertexAttrib*Format` entry points an attribute uses. The
/// runtime pre-computation of `autoAttribute`'s `FT`-driven format `switch`
/// (`Pipeline.zig:100-162`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttribClass {
    /// `glVertexAttribIFormat` — integer attributes (`u8`/`u16`/`u32`/`i8`/`i16`/
    /// `i32`), no normalize/convert-to-float step (`Pipeline.zig:105-141`).
    Integer,
    /// `glVertexAttribFormat` with `normalized = false` — `f16`/`f32`
    /// (`Pipeline.zig:142-155`; the reference always passes `false`).
    Float,
    /// `glVertexAttribLFormat` — `f64`, whose type is hard-coded `GL_DOUBLE`
    /// (`Pipeline.zig:156-160`).
    Long,
}

/// One attribute row: the format entry point + its `size` (array len, else 1), GL
/// component `typ`, and byte `offset` within the vertex struct. The pre-computed
/// per-field result of `autoAttribute`'s reflection — the OpenGL analogue of
/// `metal::pipeline::VertexAttribute`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttribDesc {
    pub class: AttribClass,
    /// `size` argument (`a.len` for an array field, else `1`).
    pub size: GLint,
    /// GL component type (`GL_UNSIGNED_BYTE`, `GL_FLOAT`, …). Ignored for
    /// [`AttribClass::Long`], where `glVertexAttribLFormat` hard-codes `GL_DOUBLE`.
    pub typ: GLenum,
    /// `@offsetOf(T, field)`.
    pub offset: GLuint,
}

/// A vertex struct's attribute table + its stride (`@sizeOf(VA)`). The runtime
/// stand-in for `Pipeline.init`'s comptime `?type` `VertexAttributes` — the OpenGL
/// analogue of `metal::pipeline::VertexLayoutRef`. `attrs` is `&'static` so the
/// per-struct tables (Task 7's `opengl::shaders`) are plain consts.
#[derive(Clone, Copy)]
pub struct VertexLayout {
    pub attrs: &'static [AttribDesc],
    pub stride: usize,
}

/// PORTS: `Pipeline.zig:1-38` — the top-level `program`/`fbo`/`vao`/`stride`/
/// `blending_enabled` fields ARE the struct.
//
// PORT NOTE (not `Copy`): the fields are all `Copy` `gl` handles, but a `Pipeline`
// OWNS its program/fbo/vao — a `Copy` + double `deinit` would double-delete. Left
// non-`Copy` like `metal::pipeline::Pipeline`; `render_pass::Step` borrows it
// (`&Pipeline`).
pub struct Pipeline {
    pub program: Program,
    pub fbo: Framebuffer,
    pub vao: VertexArray,
    pub stride: usize,
    pub blending_enabled: bool,
}

impl Pipeline {
    /// PORTS: `Pipeline.zig:40-70`.
    ///
    /// ```zig
    /// pub fn init(comptime VertexAttributes: ?type, opts: Options) !Self {
    ///     const program = try gl.Program.createVF(opts.vertex_fn, opts.fragment_fn);
    ///     errdefer program.destroy();
    ///     const pbind = try program.use();
    ///     defer pbind.unbind();
    ///     const fbo = try gl.Framebuffer.create();
    ///     errdefer fbo.destroy();
    ///     const fbobind = try fbo.bind(.framebuffer);
    ///     defer fbobind.unbind();
    ///     const vao = try gl.VertexArray.create();
    ///     errdefer vao.destroy();
    ///     const vaobind = try vao.bind();
    ///     defer vaobind.unbind();
    ///     if (VertexAttributes) |VA| try autoAttribute(VA, vaobind, opts.step_fn);
    ///     return .{ .program = program, .fbo = fbo, .vao = vao,
    ///         .stride = if (VertexAttributes) |VA| @sizeOf(VA) else 0,
    ///         .blending_enabled = opts.blending_enabled };
    /// }
    /// ```
    ///
    /// The comptime `?type` `VertexAttributes` becomes `Option<VertexLayout>`:
    /// `Some(table)` drives [`auto_attribute`] and sets `stride = table.stride`;
    /// `None` = no vertex input (`stride = 0` — the full-screen-triangle pipelines).
    /// Each `defer`/`errdefer` is a closure boundary — see the module note.
    pub fn init(vertex_attributes: Option<VertexLayout>, opts: Options<'_>) -> Result<Self, Error> {
        let stride = vertex_attributes.map_or(0, |l| l.stride);

        // const program = try gl.Program.createVF(...); errdefer program.destroy();
        let program = Program::create_vf(opts.vertex_fn, opts.fragment_fn)?;

        // const pbind = try program.use(); defer pbind.unbind();
        let with_program = (|| -> Result<Self, Error> {
            let pbind = program.use_()?;
            let result = Self::init_with_fbo(vertex_attributes, &opts, program, stride);
            pbind.unbind();
            result
        })();

        if with_program.is_err() {
            program.destroy(); // errdefer program.destroy()
        }
        with_program
    }

    /// The body under `defer pbind.unbind()`: create the fbo (`errdefer
    /// fbo.destroy()`) and bind it (`defer fbobind.unbind()`), then hand off to
    /// [`init_with_vao`](Self::init_with_vao). Split out so each Zig `defer`/
    /// `errdefer` region is a single closure (the module note).
    fn init_with_fbo(
        vertex_attributes: Option<VertexLayout>,
        opts: &Options<'_>,
        program: Program,
        stride: usize,
    ) -> Result<Self, Error> {
        // const fbo = try gl.Framebuffer.create(); errdefer fbo.destroy();
        let fbo = Framebuffer::create()?;

        // const fbobind = try fbo.bind(.framebuffer); defer fbobind.unbind();
        let with_fbo = (|| -> Result<Self, Error> {
            let fbobind = fbo.bind(FbTarget::FRAMEBUFFER)?;
            let result = Self::init_with_vao(vertex_attributes, opts, program, fbo, stride);
            fbobind.unbind();
            result
        })();

        if with_fbo.is_err() {
            fbo.destroy(); // errdefer fbo.destroy()
        }
        with_fbo
    }

    /// The body under `defer fbobind.unbind()`: create the vao (`errdefer
    /// vao.destroy()`) and bind it (`defer vaobind.unbind()`), run
    /// [`auto_attribute`] if we have a layout, and build the `Pipeline`.
    fn init_with_vao(
        vertex_attributes: Option<VertexLayout>,
        opts: &Options<'_>,
        program: Program,
        fbo: Framebuffer,
        stride: usize,
    ) -> Result<Self, Error> {
        // const vao = try gl.VertexArray.create(); errdefer vao.destroy();
        let vao = VertexArray::create()?;

        // const vaobind = try vao.bind(); defer vaobind.unbind();
        let with_vao = (|| -> Result<Self, Error> {
            let vaobind = vao.bind()?;
            let result = (|| -> Result<Self, Error> {
                // if (VertexAttributes) |VA| try autoAttribute(VA, vaobind, opts.step_fn);
                if let Some(layout) = vertex_attributes {
                    auto_attribute(layout, vaobind, opts.step_fn)?;
                }
                Ok(Pipeline {
                    program,
                    fbo,
                    vao,
                    stride,
                    blending_enabled: opts.blending_enabled,
                })
            })();
            vaobind.unbind();
            result
        })();

        if with_vao.is_err() {
            vao.destroy(); // errdefer vao.destroy()
        }
        with_vao
    }

    /// PORTS: `Pipeline.zig:72-74`.
    ///
    /// ```zig
    /// pub fn deinit(self: *const Self) void {
    ///     self.program.destroy();
    /// }
    /// ```
    ///
    /// PORT NOTE (only the program is destroyed): the reference's `deinit` destroys
    /// ONLY `program`, NOT `fbo`/`vao` — those GL objects leak for the process
    /// lifetime. That is the reference's behaviour (a pipeline outlives its uses and
    /// is torn down at shutdown), ported as-is rather than "corrected".
    pub fn deinit(&self) {
        self.program.destroy();
    }
}

/// PORTS: `Pipeline.zig:76-164` — the `autoAttribute` reflection loop, with the
/// comptime `FT -> format` `switch` pre-computed into the [`AttribDesc`] table
/// (`layout.attrs`) and this fn reproducing the descriptor-setting tail verbatim.
///
/// ```zig
/// fn autoAttribute(T: type, vaobind: gl.VertexArray.Binding, step_fn: Options.StepFunction) !void {
///     const divisor: gl.c.GLuint = switch (step_fn) {
///         .per_vertex => 0,
///         .per_instance => 1,
///         .constant => std.math.maxInt(gl.c.GLuint),
///     };
///     inline for (@typeInfo(T).@"struct".fields, 0..) |field, i| {
///         try vaobind.enableAttribArray(i);
///         try vaobind.attributeBinding(i, 0);
///         try vaobind.bindingDivisor(i, divisor);
///         const offset = @offsetOf(T, field.name);
///         // … FT-driven format switch → attributeIFormat / attributeFormat / attributeLFormat …
///     }
/// }
/// ```
fn auto_attribute(
    layout: VertexLayout,
    vaobind: VaoBinding,
    step_fn: StepFunction,
) -> Result<(), super::gl::errors::Error> {
    let divisor: GLuint = match step_fn {
        StepFunction::PerVertex => 0,
        StepFunction::PerInstance => 1,
        StepFunction::Constant => GLuint::MAX, // std.math.maxInt(GLuint)
    };

    for (i, a) in layout.attrs.iter().enumerate() {
        let idx = i as GLuint;
        vaobind.enable_attrib_array(idx)?;
        vaobind.attribute_binding(idx, 0)?;
        vaobind.binding_divisor(idx, divisor)?;

        match a.class {
            AttribClass::Integer => vaobind.attribute_i_format(idx, a.size, a.typ, a.offset)?,
            AttribClass::Float => vaobind.attribute_format(idx, a.size, a.typ, false, a.offset)?,
            AttribClass::Long => vaobind.attribute_l_format(idx, a.size, a.offset)?,
        }
    }

    Ok(())
}
