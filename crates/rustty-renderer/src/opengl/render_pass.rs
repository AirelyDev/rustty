//! Wrapper for handling render passes.
//!
//! PORTS: ghostty `src/renderer/opengl/RenderPass.zig` (144 loc). This is the
//! OpenGL twin of `metal/render_pass.rs`.
//!
//! PORT NOTE (MONOMORPHIC): the reference's `Step`/`Attachment`/`Draw` are concrete
//! structs, ported 1:1. As in `metal/render_pass.rs`, the `Step`/`Attachment` fields
//! that name a `Texture`/`Sampler`/`Target` are **borrows** (`&…`) rather than owned
//! copies: the renderer-facing `opengl::{texture,sampler}` wrappers are deliberately
//! `!Copy` (a copy + double `deinit` would double-delete the shared `gl` handle), so
//! a by-value field would move them. The `gl.Buffer` handles (`uniforms`/`buffers`)
//! ARE `Copy` `GLuint`s, so those stay by value exactly as the reference's
//! `?gl.Buffer` / `[]const ?gl.Buffer` are.
//!
//! PORT NOTE (`catch return` as early bail): `RenderPass.step`'s many `catch return`
//! sites silently swallow a GL error and stop the step (`RenderPass.zig:61`'s own
//! TODO flags this). Each is ported as `if …is_err() { … return; }`, and the Zig
//! `defer`s that must still fire on those bails (the `unbind`s + `step_number += 1`)
//! are reproduced by the [`step`](RenderPass::step)/[`step_bound`] split — see
//! `step`'s note.
//!
//! PORT NOTE (uniform-buffer-at-binding-1, `RenderPass.zig:93-96`) and the blend
//! func `GL_ONE, GL_ONE_MINUS_SRC_ALPHA` (`RenderPass.zig:124-129`) are load-bearing
//! pixel-correctness details ("align with Metal"), ported verbatim below.

use gl::types::GLsizei;

use super::gl::buffer::{Buffer, Target as BufTarget};
use super::gl::framebuffer::{Attachment as FbAttachment, Target as FbTarget};
use super::gl::primitives::Primitive;
use super::gl::texture::Texture as GlTexture;
use super::gl::{blend_func, clear, clear_color, disable, draw_arrays_instanced, enable};
use super::pipeline::Pipeline;
use super::sampler::Sampler;
use super::target::Target;
use super::texture::Texture;

/// Where a color attachment renders to (`RenderPass.zig`'s `Options.Attachment.target`
/// `union(enum)`). The variants borrow (`&…`) rather than owning a by-value
/// `Texture`/`Target` — see the module note.
pub enum AttachmentTarget<'a> {
    Texture(&'a Texture),
    Target(&'a Target),
}

/// Describes a color attachment.
//
// PORTS: `RenderPass.zig:19-27` — flattened to a module-scope `Attachment` (see
// `opengl::buffer` for the flat-naming rationale).
pub struct Attachment<'a> {
    pub target: AttachmentTarget<'a>,
    // Zig `?[4]f32 = null`; callers pass it explicitly.
    pub clear_color: Option<[f32; 4]>,
}

/// Describes the draw call for this step.
//
// PORTS: `RenderPass.zig:38-43` — `Step.Draw`.
//
// PORT NOTE: Zig's field name `type` is a Rust keyword; renamed `kind` (value and
// semantics unchanged), as `metal::render_pass::Draw` does.
pub struct Draw {
    pub kind: Primitive,
    pub vertex_count: usize,
    // Zig default 1; callers pass it explicitly.
    pub instance_count: usize,
}

/// Describes a step in a render pass. Fields are borrows / `Copy` handles (see the
/// module note).
//
// PORTS: `RenderPass.zig:29-44` — `Step`.
pub struct Step<'a> {
    pub pipeline: &'a Pipeline,
    /// `gl.Buffer` — a `Copy` handle, so by value (Zig's `?gl.Buffer`).
    pub uniforms: Option<Buffer>,
    /// `gl.Buffer`s — index 0 is the vertex buffer, 1.. are storage buffers.
    pub buffers: &'a [Option<Buffer>],
    pub textures: &'a [Option<&'a Texture>],
    pub samplers: &'a [Option<&'a Sampler>],
    pub draw: Draw,
}

/// Options for beginning a render pass.
//
// PORTS: `RenderPass.zig:14-27` — `Options`.
pub struct Options<'a> {
    /// Color attachments for this render pass.
    pub attachments: &'a [Attachment<'a>],
}

/// PORTS: `RenderPass.zig:46-48` — the top-level `attachments`/`step_number` fields
/// ARE the struct.
pub struct RenderPass<'a> {
    pub attachments: &'a [Attachment<'a>],
    pub step_number: usize,
}

impl<'a> RenderPass<'a> {
    /// PORTS: `RenderPass.zig:50-57`.
    ///
    /// ```zig
    /// pub fn begin(opts: Options) Self {
    ///     return .{ .attachments = opts.attachments };
    /// }
    /// ```
    pub fn begin(opts: Options<'a>) -> Self {
        Self {
            attachments: opts.attachments,
            step_number: 0,
        }
    }

    /// PORTS: `RenderPass.zig:59-137`.
    ///
    /// Add a step to this render pass.
    ///
    /// TODO: Errors are silently ignored in this function, maybe they shouldn't be?
    ///
    /// PORT NOTE (the `defer` fan-out): the Zig body registers, in order,
    /// `defer pbind.unbind()` (`:66`), `defer vaobind.unbind()` (`:69`),
    /// `defer fbobind.unbind()` (`:82`) and `defer self.step_number += 1` (`:84`),
    /// all of which fire on the normal return AND on every intervening
    /// `catch return`. The `pbind` half lives here; everything from `vaobind`
    /// onward — including the `step_number` bump, which only fires once we get past
    /// `fbobind` acquisition — lives in [`step_bound`], which returns whether it
    /// advanced that far so this fn can do the bump under the released borrow of
    /// `self`.
    pub fn step(&mut self, s: Step<'_>) {
        // if (s.draw.instance_count == 0) return;
        if s.draw.instance_count == 0 {
            return;
        }

        // const pbind = s.pipeline.program.use() catch return;
        let Ok(pbind) = s.pipeline.program.use_() else {
            return;
        };

        // Read what `step_bound` needs from `self` up front so its borrow ends
        // before the `self.step_number += 1` bump below (the `defer` at `:84`).
        let attachment0 = &self.attachments[0];
        let is_first_step = self.step_number == 0;

        // defer pbind.unbind() — fires on both the advanced and the bailed path.
        let advanced = step_bound(&s, attachment0, is_first_step);
        pbind.unbind();

        // defer self.step_number += 1 — registered only AFTER fbobind is bound
        // (`:84`), so it fires exactly when `step_bound` advanced past that point.
        if advanced {
            self.step_number += 1;
        }
    }

    /// Complete this render pass.
    /// This struct can no longer be used after calling this.
    ///
    /// PORTS: `RenderPass.zig:139-144`.
    ///
    /// ```zig
    /// pub fn complete(self: *const Self) void {
    ///     _ = self;
    ///     gl.flush();
    /// }
    /// ```
    pub fn complete(&self) {
        super::gl::flush();
    }
}

/// The body under `defer pbind.unbind()` (`RenderPass.zig:68-137`): bind the vao,
/// resolve + bind the target framebuffer, run the clear + the resource binds + the
/// draw, then fire the `vaobind`/`fbobind` `unbind` `defer`s. Returns `true` iff we
/// bound the framebuffer (`RenderPass.zig:71-82`) — i.e. reached the point where the
/// caller's `defer self.step_number += 1` was registered.
fn step_bound(s: &Step<'_>, attachment0: &Attachment<'_>, is_first_step: bool) -> bool {
    // const vaobind = s.pipeline.vao.bind() catch return;
    let Ok(vaobind) = s.pipeline.vao.bind() else {
        return false;
    };

    // const fbobind = switch (self.attachments[0].target) { … };
    // On any bail here, `defer vaobind.unbind()` still fires — reproduced inline.
    let fbobind = match &attachment0.target {
        AttachmentTarget::Target(t) => {
            let Ok(b) = t.framebuffer.bind(FbTarget::FRAMEBUFFER) else {
                vaobind.unbind();
                return false;
            };
            b
        }
        AttachmentTarget::Texture(t) => {
            let Ok(b) = s.pipeline.fbo.bind(FbTarget::FRAMEBUFFER) else {
                vaobind.unbind();
                return false;
            };
            // fbobind.texture2D(.color0, t.target, t.texture, 0) catch { fbobind.unbind(); return; };
            if b.texture_2d(FbAttachment::COLOR0, t.target, t.texture, 0)
                .is_err()
            {
                b.unbind();
                vaobind.unbind();
                return false;
            }
            b
        }
    };

    // From here the Zig `defer self.step_number += 1` is registered, so it fires no
    // matter how the body below bails; all of `defer fbobind.unbind()` /
    // `defer vaobind.unbind()` fire too. `step_body` reproduces every `catch return`
    // as an early return of its own (they all land on the same `defer` fan-out), so
    // the caller always advances once we are here.
    step_body(s, vaobind, is_first_step, attachment0.clear_color);
    fbobind.unbind();
    vaobind.unbind();
    true
}

/// The clear + resource-bind + draw tail (`RenderPass.zig:86-136`). Every reference
/// `catch return` becomes an early `return` here — they all share the same `defer`
/// cleanup, run by [`step_bound`] after this returns, so a bail and a completion are
/// indistinguishable to the caller (both advance `step_number`).
fn step_body(
    s: &Step<'_>,
    vaobind: super::gl::vertex_array::Binding,
    is_first_step: bool,
    clear_col: Option<[f32; 4]>,
) {
    // If we have a clear color and this is the
    // first step in the pass, go ahead and clear.
    if is_first_step {
        if let Some(c) = clear_col {
            clear_color(c[0], c[1], c[2], c[3]);
            clear(gl::COLOR_BUFFER_BIT);
        }
    }

    // Bind the uniform buffer we bind at index 1 to align with Metal.
    if let Some(ubo) = s.uniforms {
        if ubo.bind_base(BufTarget::UNIFORM, 1).is_err() {
            return;
        }
    }

    // Bind relevant texture units.
    for (i, t) in s.textures.iter().enumerate() {
        if let Some(tex) = t {
            if GlTexture::active(i as gl::types::GLuint).is_err() {
                return;
            }
            if tex.texture.bind(tex.target).is_err() {
                return;
            }
        }
    }

    // Bind relevant samplers.
    for (i, samp) in s.samplers.iter().enumerate() {
        if let Some(sampler) = samp {
            if sampler.sampler.bind(i as gl::types::GLuint).is_err() {
                return;
            }
        }
    }

    // Bind 0th buffer as the vertex buffer,
    // and bind the rest as storage buffers.
    if !s.buffers.is_empty() {
        if let Some(vbo) = s.buffers[0] {
            let stride = GLsizei::try_from(s.pipeline.stride).unwrap_or(GLsizei::MAX);
            if vaobind.bind_vertex_buffer(0, vbo.id, 0, stride).is_err() {
                return;
            }
        }

        // for (s.buffers[1..], 1..) |b, i| … buf.bindBase(.storage, i)
        for (i, b) in s.buffers[1..].iter().enumerate() {
            if let Some(buf) = b {
                let idx = (i + 1) as gl::types::GLuint;
                if buf.bind_base(BufTarget::STORAGE, idx).is_err() {
                    return;
                }
            }
        }
    }

    if s.pipeline.blending_enabled {
        if enable(gl::BLEND).is_err() {
            return;
        }
        if blend_func(gl::ONE, gl::ONE_MINUS_SRC_ALPHA).is_err() {
            return;
        }
    } else if disable(gl::BLEND).is_err() {
        return;
    }

    let vertex_count = GLsizei::try_from(s.draw.vertex_count).unwrap_or(GLsizei::MAX);
    let instance_count = GLsizei::try_from(s.draw.instance_count).unwrap_or(GLsizei::MAX);
    let _ = draw_arrays_instanced(s.draw.kind, 0, vertex_count, instance_count);
}
