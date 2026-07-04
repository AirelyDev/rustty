//! Wrapper for handling render passes.
//!
//! PORTS: ghostty `src/renderer/metal/RenderPass.zig`.
//!
//! PORT NOTE (§A.3, MONOMORPHIC): ghostty's `Step`/`Attachment`/`Draw` are
//! concrete structs (no `GraphicsApi` generics), ported 1:1. Per §A.3 the `Step`
//! fields are **borrows** (`&…`), not owned `Retained` clones, to preserve Zig's
//! zero-refcount by-value copy semantics (a `Retained` clone would bump the
//! refcount per step per frame). Zig's erased `[]const ?objc.Object` buffer slice
//! becomes `&[Option<&ProtocolObject<dyn MTLBuffer>>]` — already `T`-erased,
//! exactly what `buffer::Buffer::<T>::raw()` returns. ghostty's untyped
//! `setProperty`/`msgSend` sends map to objc2's typed setters (identical Apple
//! selectors); `renderCommandEncoderWithDescriptor:` returns an `Option` where
//! ghostty stores the (unchecked) encoder directly.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLBuffer, MTLCommandBuffer, MTLCommandEncoder, MTLLoadAction, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLStoreAction, MTLTexture,
};

use crate::metal::api;
use crate::metal::pipeline::Pipeline;
use crate::metal::sampler::Sampler;
use crate::metal::target::Target;
use crate::metal::texture::Texture;

// const log = std.log.scoped(.metal);
// PORT NOTE: no logging facade is wired in the port yet (see `buffer.rs`);
// `RenderPass.zig` has no `log` call sites, but the scoped-log declaration is
// preserved as this comment for inventory parity.

/// Where a color attachment renders to (RenderPass.zig `Options.Attachment.target`
/// `union(enum)`). Per §A.3 the variants borrow (`&…`) rather than owning a
/// by-value `Texture`/`Target`.
pub enum AttachmentTarget<'a> {
    Texture(&'a Texture),
    Target(&'a Target),
}

/// Describes a color attachment.
//
// PORT NOTE (§C): ghostty's `RenderPass.Options.Attachment` is flattened to a
// module-scope `Attachment` (see `buffer.rs` for the flat-naming rationale).
pub struct Attachment<'a> {
    pub target: AttachmentTarget<'a>,
    // Zig `?[4]f64 = null`; callers pass it explicitly.
    pub clear_color: Option<[f64; 4]>,
}

/// Describes the draw call for this step.
//
// PORT NOTE: Zig's field name `type` is a Rust keyword; renamed `kind` (the
// value/semantics are unchanged).
pub struct Draw {
    pub kind: api::MTLPrimitiveType,
    pub vertex_count: usize,
    // Zig default 1; callers pass it explicitly.
    pub instance_count: usize,
}

/// Describes a step in a render pass. Fields are borrows (§A.3).
pub struct Step<'a> {
    pub pipeline: &'a Pipeline,
    /// MTLBuffer
    pub uniforms: Option<&'a ProtocolObject<dyn MTLBuffer>>,
    /// MTLBuffer
    pub buffers: &'a [Option<&'a ProtocolObject<dyn MTLBuffer>>],
    pub textures: &'a [Option<&'a Texture>],
    /// Set of samplers to use for this step. The index maps to an index
    /// of a fragment texture, set via setFragmentSamplerState(_:index:).
    pub samplers: &'a [Option<&'a Sampler>],
    pub draw: Draw,
}

/// Options for beginning a render pass.
pub struct Options<'a> {
    /// MTLCommandBuffer
    pub command_buffer: &'a ProtocolObject<dyn MTLCommandBuffer>,
    /// Color attachments for this render pass.
    pub attachments: &'a [Attachment<'a>],
}

/// MTLRenderCommandEncoder
pub struct RenderPass {
    pub encoder: Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>,
}

impl RenderPass {
    /// Begin a render pass.
    pub fn begin(opts: Options<'_>) -> Self {
        // Create a pass descriptor
        let desc = MTLRenderPassDescriptor::renderPassDescriptor();

        // Set our color attachment to be our drawable surface.
        let attachments = desc.colorAttachments();
        for (i, at) in opts.attachments.iter().enumerate() {
            // SAFETY: `i` < `opts.attachments.len()`; objc2 marks the subscript
            // unsafe ("attachmentIndex might not be bounds-checked").
            let attachment = unsafe { attachments.objectAtIndexedSubscript(i) };

            attachment.setLoadAction(if at.clear_color.is_some() {
                MTLLoadAction::Clear
            } else {
                MTLLoadAction::Load
            });
            attachment.setStoreAction(MTLStoreAction::Store);
            let tex: &ProtocolObject<dyn MTLTexture> = match &at.target {
                AttachmentTarget::Texture(t) => &t.texture,
                AttachmentTarget::Target(t) => &t.texture, // Target's IOSurface-backed tex
            };
            attachment.setTexture(Some(tex));
            if let Some(c) = at.clear_color {
                attachment.setClearColor(api::MTLClearColor {
                    red: c[0],
                    green: c[1],
                    blue: c[2],
                    alpha: c[3],
                });
            }
        }

        // MTLRenderCommandEncoder
        let encoder = opts
            .command_buffer
            .renderCommandEncoderWithDescriptor(&desc)
            .expect("renderCommandEncoderWithDescriptor returned nil");

        Self { encoder }
    }

    /// Add a step to this render pass.
    pub fn step(&self, s: Step<'_>) {
        if s.draw.instance_count == 0 {
            return;
        }

        // Set pipeline state
        self.encoder.setRenderPipelineState(&s.pipeline.state);

        if !s.buffers.is_empty() {
            // We reserve index 0 for the vertex buffer, this isn't very
            // flexible but it lines up with the API we have for OpenGL.
            if let Some(buf) = s.buffers[0] {
                // SAFETY: objc2 marks the buffer setters unsafe (the offset/index
                // must be valid for the bound buffer, which they are).
                unsafe {
                    self.encoder.setVertexBuffer_offset_atIndex(Some(buf), 0, 0);
                    self.encoder
                        .setFragmentBuffer_offset_atIndex(Some(buf), 0, 0);
                }
            }

            // Set the rest of the buffers starting at index 2, this is
            // so that we can use index 1 for the uniforms if present.
            //
            // Also, we set buffers (and textures) for both stages.
            //
            // Again, not very flexible, but it's consistent and predictable,
            // and we need to treat the uniforms as special because of OpenGL.
            //
            // TODO: Maybe in the future add info to the pipeline struct which
            //       allows it to define a mapping between provided buffers and
            //       what index they get set at for the vertex / fragment stage.
            for (i, b) in s.buffers[1..].iter().enumerate() {
                if let Some(buf) = b {
                    let idx = i + 2;
                    // SAFETY: see above; `idx` is a monotonically increasing bind slot.
                    unsafe {
                        self.encoder
                            .setVertexBuffer_offset_atIndex(Some(buf), 0, idx);
                        self.encoder
                            .setFragmentBuffer_offset_atIndex(Some(buf), 0, idx);
                    }
                }
            }
        }

        // Set the uniforms as buffer index 1 if present.
        if let Some(buf) = s.uniforms {
            // SAFETY: see above; index 1 is the reserved uniforms slot.
            unsafe {
                self.encoder.setVertexBuffer_offset_atIndex(Some(buf), 0, 1);
                self.encoder
                    .setFragmentBuffer_offset_atIndex(Some(buf), 0, 1);
            }
        }

        // Set textures.
        for (i, t) in s.textures.iter().enumerate() {
            if let Some(tex) = t {
                // SAFETY: objc2 marks the texture setters unsafe; `i` is the bind slot.
                unsafe {
                    self.encoder.setVertexTexture_atIndex(Some(&tex.texture), i);
                    self.encoder
                        .setFragmentTexture_atIndex(Some(&tex.texture), i);
                }
            }
        }

        // Set samplers.
        for (i, samp) in s.samplers.iter().enumerate() {
            if let Some(sampler) = samp {
                // SAFETY: objc2 marks the sampler setter unsafe; `i` is the bind slot.
                unsafe {
                    self.encoder
                        .setFragmentSamplerState_atIndex(Some(&sampler.sampler), i);
                }
            }
        }

        // Draw!
        // SAFETY: objc2 marks `drawPrimitives:…` unsafe (the counts must be valid
        // for the bound buffers, which they are); `vertexStart` is 0.
        unsafe {
            self.encoder
                .drawPrimitives_vertexStart_vertexCount_instanceCount(
                    s.draw.kind,
                    0,
                    s.draw.vertex_count,
                    s.draw.instance_count,
                );
        }
    }

    /// Complete this render pass.
    /// This struct can no longer be used after calling this.
    pub fn complete(&self) {
        self.encoder.endEncoding();
    }
}
