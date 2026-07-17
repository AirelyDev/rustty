//! PORT STATUS (L4, Linux-only): the OpenGL graphics-API backend — the second
//! concrete graphics API alongside `crate::metal`.
//!
//! PORTS: ghostty `src/renderer/OpenGL.zig` + `src/renderer/opengl/*.zig`, layered
//! over [`gl`] — this crate's hand-port of ghostty's own low-level `pkg/opengl`
//! wrapper package. That two-layer shape is deliberate and mirrors the reference:
//! `pkg/opengl` is a thin hand-rolled wrapper over a RAW glad function-pointer
//! context, so the Rust port keeps a hand-ported `pkg/opengl` twin over the raw
//! `gl` crate (glad's analogue). See the plan's "Tech Stack / GL loader decision".
//!
//! Task 1 lands only [`gl::sys`] — the headless-EGL context plus the
//! `OpenGL.zig::prepareContext` version/debug-output gate. The rest of
//! `OpenGL.zig`, the `pkg/opengl` leaves, the `renderer/opengl/*` leaves and the
//! monomorphic `GenericRenderer(OpenGL)` are later tasks.
//! (docs/superpowers/plans/2026-07-17-L4-opengl-renderer.md)

pub mod gl;
