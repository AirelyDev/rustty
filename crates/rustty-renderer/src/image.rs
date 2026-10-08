//! PORTS: ghostty `src/renderer/image.zig` — the generic Kitty-graphics +
//! overlay image render state (`State`, `Placement`, `Id`, `ImageMap`, and the
//! per-image `Image` upload state machine).
//!
//! DISPATCHER ONLY. `image.zig` is `Image(GraphicsAPI)`-shaped: every texture it
//! owns comes from `Renderer.API`, so the port is a MONOMORPHIC TWIN PAIR, exactly
//! like [`crate::generic`]'s (`macos_impl` over `crate::metal`, `linux_impl` over
//! `crate::opengl`). The two arms are mutually exclusive by target and expose the
//! IDENTICAL public surface (`State`, `Image`, `Pending`, `PixelFormat`, `Id`,
//! `Placement`, `DrawPlacements`, …), so every consumer — `generic::macos_impl`,
//! `generic::linux_impl`, `overlay::Overlay::pending_image` — names
//! `crate::image::*` unqualified and resolves against its own backend.
//!
//! `linux_impl` is COPY-ADAPTED from the already-reviewed `macos_impl`
//! (READ-ONLY reference) with the `crate::metal::*` graphics-API glue swapped for
//! `crate::opengl::*`; per the repo's locked twin doctrine (the module header of
//! `generic/linux_impl.rs`) we duplicate wholesale rather than factoring a shared
//! trait-generic core, because ghostty's own `Image(GraphicsAPI)` is a comptime
//! instantiation, not a vtable.
//!
//! The cfg predicate here MUST track [`crate::generic`]'s dispatcher arms: an
//! `image` twin exists exactly where a `generic` twin does. On every other target
//! (Windows / wasm32) the module is not compiled at all.
//!
//! PORT NOTE (W2D/F-003): the OpenGL arm's gate is `any(linux, freebsd)`, kept in
//! lock-step with `generic`'s `linux_impl` gate and the crate root's `opengl`
//! module — `renderer/backend.zig:20-21` selects `.opengl` for every non-Darwin
//! native target, and FreeBSD is a real windowed target at the pin
//! (`apprt/runtime.zig:18` `.linux, .freebsd => .gtk`). FreeBSD therefore gets the
//! SAME `linux_impl` twin, not a third one.

#[cfg(target_os = "macos")]
mod macos_impl;
#[cfg(target_os = "macos")]
pub use macos_impl::*;

// PORT NOTE (task W-2, N342): Windows is the third `.opengl` target (`backend.zig:20-21`);
// its runtime is `.none`, so it builds the `apprt.embedded` arms of `OpenGL.zig`.
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
mod linux_impl;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
pub use linux_impl::*;
