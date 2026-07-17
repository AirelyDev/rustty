//! PORTS: ghostty's `pkg/opengl` package — the low-level OpenGL wrapper ghostty
//! hand-wrote over glad (`pkg/opengl/main.zig`'s module docs: "purpose-built for
//! usage within this program … not meant to have 100% API coverage"). It is the GL
//! analogue of what `objc2-metal` is to the Metal lane, except that ghostty
//! hand-rolled it, so it is hand-ported here rather than taken off crates.io.
//!
//! PORT NOTE: this module is `crate::opengl::gl`; the raw `gl` CRATE (glad's
//! analogue, i.e. `pkg/opengl/c.zig` + `glad.zig`) is `::gl`.
//!
//! Task 1 lands [`sys`] only. The leaf wrappers (`Buffer`/`Framebuffer`/`Program`/
//! `Shader`/`Texture`/`VertexArray`/`draw`/`errors`/`extensions`/…) are Tasks 2-4.

pub mod sys;
