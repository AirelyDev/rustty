//! rustty-renderer
//!
//! PORTS: ghostty src/renderer/ minus size.zig (renderer.zig, Metal, OpenGL, WebGL, Options,
//! Overlay, State, Thread, backend, cell, cursor, generic, ...); links
//! glslang/spirv-cross/wuffs/libpng/zlib-sys; apprt via surface trait (breaks #10, #11).
//!
//! PORT STATUS: in progress. Two slices are complete; substantial surface deferred.
//!
//! DONE — the R-a headless slice: cell (renderer/cell.zig, 5 tests), cursor
//!   (renderer/cursor.zig, 4 tests), state (renderer/State.zig, 2 tests), shaders
//!   (the CPU-side `CellText`/`CellBg`/`Uniforms` vertex-data structs), shadertoy
//!   (renderer/shadertoy.zig, 4 tests — the GLSL -> glslang -> SPIR-V -> spirv-cross
//!   -> MSL/GLSL transpile), link (renderer/link.zig, 3 tests — the `oni.Regex`
//!   renderer link `Set`/`renderCellMap`), Health (the crate-root `renderer.zig`
//!   `Health` enum, 1 test), message (renderer/message.zig — the cross-thread
//!   `Message` union), thread (renderer/Thread.zig — the MINIMAL cross-thread
//!   surface only: `Thread::Mailbox` + the `DerivedConfig` seam placeholder), and
//!   size (renderer/size.zig, 6 tests — the A1 hoist in `rustty-render-types`,
//!   re-exported below).
//!
//! DONE — the GPU R-d OFFSCREEN slice (macOS/Metal, Chunks 0-4): the monomorphic
//!   `Renderer` over the concrete `Metal` backend RENDERS a terminal state offscreen.
//!   The whole `metal/*` backend (device/queue/buffer/texture/pipeline/render-pass/
//!   target/frame), `SwapChain`/`FrameState`, `update_frame`, the `rebuild_cells` CPU
//!   cell-fill, the uniforms math, and `draw_frame` (Metal encode+submit -> IOSurface
//!   readback) are ported and gated by the golden test
//!   `generic::macos_impl::tests::golden_renders_cursor_fg_bg` ("Ab" + cursor, real
//!   pixel asserts). The `draw_frame` raw-ptr aliasing is proven clean under Stacked
//!   AND Tree Borrows by `.port/phase-2/mirihar-chunk3b`.
//!
//! DONE — the macOS-windowing W0–W5 slice (sub-project #1): `IOSurfaceLayer`
//!   (W0), `Metal::init(view)` + the sync/async on-screen present halves (W1),
//!   `CVDisplayLink` (W2), `Metal::loop_enter` (the display-callback trampoline,
//!   W5) — the formerly deferred windowing-host tier is LIVE in this crate. The
//!   one residual windowed gap is CORE-side (renderer_seam::Renderer::init never
//!   routes the host view into `Metal::init(view)` — DEFERRALS.md "W-ATTACH").
//!
//! DONE — the renderer<->config tier via the `RendererConfig` accessor trait
//!   (defined here, impl'd IN rustty-config; no config edge, G7 acyclic):
//!   `Renderer.DerivedConfig::init`, `Message::initChangeConfig`,
//!   `Thread.DerivedConfig::init`, `shadertoy::{load_from_files,load_from_file}`,
//!   `prep_background_image` — all live. The Kitty-pixel path (`image`
//!   `kitty_update`/`prep_kitty_*`/`convert`, re-homed pure-Rust swizzle) and the
//!   z2d `Overlay` `highlight_*` draws are live too (R-d P1 fill).
//!
//! DEFERRED (the honest residual — see DEFERRALS.md "rustty-renderer R-d
//! [RECONCILED]"):
//!   - rustty-inspector: `overlayFeatures` — RESOLVED (Task 5): the renderer now
//!     reads overlay features through the renderer-owned `InspectorOverlay`
//!     accessor trait (state.rs; the `RendererConfig`-precedent acyclic shape),
//!     `state.inspector` being the `*mut dyn InspectorOverlay` fat pointer. No
//!     `unimplemented!()` remains for this seam.
//!   - GTK — out of the monomorphic Metal scope (a whole-backend follow-up port).
//!     The **OpenGL backend (Linux)** is NOT in this list any more: L4 ported the
//!     `crate::opengl` leaves + the `generic::linux_impl` twin, and wave 2 closed
//!     the last draw-path elisions — custom shaders (F-013), kitty graphics
//!     (F-005), background image (F-012) and the inspector overlay (F-020) — so
//!     `linux_impl` carries no `MILESTONE-SCOPE-CUT` markers (DEFERRALS.md
//!     "rustty-renderer L4 (OpenGL `linux_impl`) draw-path scope cuts").
//!     The **WebGL** backend is NOT in this list
//!     any more: WS14 established that the reference `src/renderer/WebGL.zig` is a
//!     2-line placeholder (no fields, no methods, no shaders, no tests, identical
//!     on the oracle's `origin/main`), and it is ported COMPLETE at
//!     [`crate::webgl`].
//!
//! G3b comment-coverage SKIPS this crate while it is marked `in progress` (only a
//! `done` status activates the whole-crate fuzzy match); it stays `in progress`
//! while the GL/GTK backends are unported (honest). R1 fidelity for the
//! ported slices is backstopped by the adversary + verify panel + the golden gate
//! (heavy-tier doctrine), and MAY be spot-checked with a scoped
//! `check_comment_coverage.py --port-glob` run.
//! See .port/phase-2/DEFERRALS.md and .port/phase-2/GPU_RD_CHUNK5_DESIGN.md.

pub mod cell;
pub mod config;
pub mod cursor;
pub mod generic;
pub mod link;
pub mod message;
pub mod row;
pub mod shaders;
pub mod shadertoy;
pub mod state;
pub mod thread;

// Shared `#[cfg(test)]` mock `C` (impl `RendererConfig`) exercising the
// `RendererConfig`-bounded seams (`DerivedConfig::init`,
// `thread::DerivedConfig::init`, `Message::initChangeConfig`) without depping
// `rustty-config` (that IS the R4 cycle).
#[cfg(test)]
mod test_support;

// Minimal `#[cfg(test)]` `log::Log` backend so tests can observe the restored
// `log::` diagnostics (DECISION-logging.md §6r call shape 3). Test-only — never
// compiled into the production library.
#[cfg(test)]
pub(crate) mod log_capture;

// PORT STATUS (GPU R-d, macOS-only): the Metal graphics-API backend. Chunk 0
// lands only the Metal API vocabulary (`metal::api`, ghostty `metal/api.zig`
// re-exported over `objc2-metal`) + the shader build infra (`build.rs` compiles
// `src/shaders/shaders.metal` into `$OUT_DIR/rustty.metallib`). The `metal/*`
// leaf wrappers + the monomorphic `GenericRenderer(Metal)` draw path are the
// golden-image-gated Chunk 1+ work (GPU_RD_BLUEPRINT.md / GPU_RD_TRAIT_DESIGN.md).
// `pub` so the Chunk-0 vocabulary is reachable public API (nothing consumes it
// yet — the draw path lands in Chunk 1).
#[cfg(target_os = "macos")]
pub mod metal;

// PORT STATUS (L4, Linux/FreeBSD): the OpenGL graphics-API backend, the second
// concrete graphics API alongside `metal` (R5: purely additive — not one byte of
// the Metal lane changes). Task 1 lands only the headless-EGL context + the raw
// `gl` loader (`opengl::gl::sys`), i.e. the `OpenGL.zig::prepareContext` half; the
// `pkg/opengl` leaf wrappers and the monomorphic `GenericRenderer(OpenGL)`
// (`generic::linux_impl`) are later tasks.
// (docs/superpowers/plans/2026-07-17-L4-opengl-renderer.md)
//
// PORT NOTE (W2D/F-003): the gate is `any(linux, freebsd)`, not `linux`, because
// `renderer/backend.zig:20-21` picks `.metal` iff `target.os.tag.isDarwin()` and
// `.opengl` for EVERY other native target — and FreeBSD is a real windowed target
// at the pin (`apprt/runtime.zig:18` `.linux, .freebsd => .gtk`). The L4 milestone
// scoped itself to Linux only because the macOS-first plan sequenced the GTK tier
// last; the OpenGL arm itself names nothing Linux-specific (EGL via a dlopen'd
// `libEGL.so.1`, and `gl` is pure generated bindings loaded through
// `gl::load_with`), so FreeBSD gets the identical arm rather than a second twin.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub mod opengl;

// PORT: the reference `renderer.zig`'s own module doc comment (`:1-8`). The crate
// root above carries rustty's PORT STATUS narrative rather than the reference's
// prose, so these eight lines had no home and were dropped — a scoped G3b RED that
// predated WS14 and surfaced when T2 brought this file under a scoped run. They are
// restored verbatim here, at the file's first `renderer.zig` decl (as quoted `//`
// lines, not `//!` — an inner doc comment is only legal at the top of a module):
//
//     //! Renderer implementation and utilities. The renderer is responsible for
//     //! taking the internal screen state and turning into some output format,
//     //! usually for a screen.
//     //!
//     //! The renderer is closely tied to the windowing system which usually
//     //! has to prepare the window for the given renderer using system-specific
//     //! APIs. The renderers in this package assume that the renderer is already
//     //! setup (OpenGL has a context, Vulkan has a surface, etc.)

// PORT: `pub const WebGL = @import("renderer/WebGL.zig");` (renderer.zig:20).
// NOT target-gated: the reference re-export is unconditional, and the type is a
// zero-sized placeholder that names nothing target-specific (WS14 T2).
pub mod webgl;
pub use webgl::WebGL;

// GPU R-d Chunk-3 prerequisites: the Kitty-graphics + overlay image render state
// (`image.zig`) and the CPU/z2d debug overlay (`Overlay.zig`). Neither names
// `GenericRenderer`, so both compile standalone before Chunk 3 (GPU_RD_P1_DESIGN.md
// §1.1). The new `rustty-renderer -> rustty-z2d` edge (Overlay's z2d) is a
// downward, acyclic edge to a pure-Rust sink (check_acyclic.sh stays GREEN).
//
// `image` is a MONOMORPHIC TWIN PAIR, gated exactly where `generic`'s renderer
// arms are: `image::Image` embeds the backend's `Texture` (the `Ready` variant),
// `image::Replace`/`ImageEntry` carry it, and `image::State`'s draw path is
// written against the backend leaves — `crate::metal` on macOS,
// `crate::opengl` on Linux/FreeBSD (see the `image.rs` dispatcher). On every other
// target there is no `generic` renderer arm, so there is no `image` twin either.
//
// PORT NOTE (W2D/F-003): this predicate tracks `generic`'s dispatcher arms exactly
// — it gained `freebsd` in lock-step with the `opengl`/`linux_impl` widening above
// (`backend.zig:20-21` gives every non-Darwin native target `.opengl`).
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "freebsd"))]
pub mod image;
// `overlay` is NOT gated: `Overlay` renders on the CPU via z2d (`Overlay.zig`'s
// whole point — see the module docs) and names no backend type. Its ONLY
// `image`-typed leaf, `pending_image() -> image::Pending`, is gated WITHIN
// `overlay.rs` instead, so the backend-free surface resolves on every target. That
// surface is load-bearing on the backend-less targets:
// `state::InspectorOverlay::overlay_features` names [`overlay::Feature`]
// UNCONDITIONALLY (it is the acyclic inspector seam's only return type), and
// `overlay::InitError` is named by both `generic` twins. The `pending_image` leaf's
// own gate now tracks the `image` dispatcher's `any(macos, linux, freebsd)`: the L4
// OpenGL image twin (docs/superpowers/plans/2026-07-17-L4-opengl-renderer.md) landed
// it on Linux, so the overlay's Kitty-independent draw path is live on both
// backends — and on every target the OpenGL backend serves (W2D/F-003: FreeBSD).
pub mod overlay;
// The GPU_RD_TRAIT_DESIGN §D field names `overlay: Option<Overlay>` via
// `crate::Overlay`.
pub use overlay::Overlay;

// renderer/size.zig lives in the rustty-render-types leaf (A1 hoist). The
// reference `renderer.zig` re-exports these size types (`GridSize`, `Size`,
// `ScreenSize`, `CellSize`, `Padding`, `Coordinate`, ...); mirror that re-export.
pub use rustty_render_types as size;

// pub const Message = message.Message;
pub use message::Message;

// PORT NOTE: the reference crate root comptime-selects the GPU renderer
// implementation:
//
//     /// The implementation to use for the renderer. This is comptime chosen
//     /// so that every build has exactly one renderer implementation.
//     pub const Renderer = switch (build_config.renderer) {
//         .metal => GenericRenderer(Metal),
//         .opengl => GenericRenderer(OpenGL),
//         .webgl => WebGL,
//     };
//
// Ported MONOMORPHICALLY (GPU_RD_TRAIT_DESIGN.md §D): a set of one per target —
// the concrete `generic::Renderer` over the concrete `Metal` leaf types.
// macOS-gated because `Renderer` names `crate::metal::Metal`; the GPU draw path
// (the big CPU methods) lands in a later chunk (GPU_RD_CHUNK3A_DESIGN §B).
#[cfg(target_os = "macos")]
pub use generic::Renderer;

// L4 SWAP (first light): the Linux renderer is `GenericRenderer(OpenGL)` — the
// monomorphic `generic::linux_impl::Renderer` (renderer.zig:38's `.opengl` arm),
// re-exported by `generic`'s own `#[cfg(any(target_os = "linux", target_os =
// "freebsd"))] pub use linux_impl::*;`. This mirrors the macOS line above (the
// `.metal` arm) so `rustty_renderer::Renderer` resolves on Linux; core's
// `renderer_seam::Renderer` newtype now forwards to the REAL renderer on both
// targets. Additive Linux `pub use` — the macOS export is unchanged.
//
// PORT NOTE (W2D/F-003): widened to `any(linux, freebsd)`. `renderer.zig:37-41`
// switches on `build_config.renderer`, which `backend.zig:20-21` resolves to
// `.opengl` for every non-Darwin native target, so the `GenericRenderer(OpenGL)`
// arm is what FreeBSD selects too. core's `renderer_seam` needs NO change: its
// `Renderer`/`RendererError` arms are the EXACT COMPLEMENT of the macOS export
// (`#[cfg(not(target_os = "macos"))]`, renderer_seam.rs), an invariant that
// comment already pins — it starts resolving on FreeBSD the moment this line does.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub use generic::Renderer;

// PORT: `.webgl => WebGL` (renderer.zig:41) — the ONLY arm that is NOT
// `GenericRenderer(Backend)`: the reference selects the bare `WebGL` type. With
// this arm the port's switch is complete for all three reference arms
// (`.metal` macOS, `.opengl` Linux, `.webgl` wasm32). `renderer/backend.zig`
// returns `.webgl` for `wasm32` + `WasmTarget.browser`, which is why the cfg is
// `target_arch = "wasm32"` (WS14 T2).
#[cfg(target_arch = "wasm32")]
pub use webgl::WebGL as Renderer;

// The `renderer.Renderer.DerivedConfig` nested config, named by the cross-thread
// message surface (`Message::ChangeConfig`). It names no Metal type (its 9
// config-typed fields use inhabited placeholders — GPU_RD_CHUNK3A_DESIGN §B.1), so
// it is NOT macOS-gated. This replaces the earlier uninhabited
// `RendererDerivedConfig` placeholder (now that the real, inhabited struct exists);
// `DerivedConfig::init` remains a deferred generic-`C` seam (config-coupled, G7).
pub use generic::DerivedConfig;

/// The health status of a renderer. These must be shared across all
/// renderers even if some states aren't reachable so that our API users
/// can use the same enum for all renderers.
//
// PORT NOTE: reference `src/renderer.zig` crate-root `pub const Health =
// enum(c_int) { healthy, unhealthy }`. Hoisted into the rustty-renderer crate
// root here (mirroring the render-types A1 `size` hoist precedent) so that
// `rustty-apprt-api`'s `Action::renderer_health` variant can name it WITHOUT
// pulling in the GPU renderer. `enum(c_int)` -> `#[repr(i32)]` (C `int`) so the
// discriminants match the C ABI ordering used by the `checkGhosttyHEnum` test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Health {
    Healthy,
    Unhealthy,
}

#[cfg(test)]
mod health_tests {
    use super::Health;
    use rustty_lib::c_header::header_decls;
    use rustty_lib::r#enum::{check_ghostty_henum, enum_type_values, EnumType};

    /// The checked-in public C header, embedded at compile time.
    ///
    /// PORT NOTE: the reference `lib.checkGhosttyHEnum` does
    /// `const c = @import("ghostty.h")` — it reflects over the REAL generated
    /// header. Rust has no `@import` for a C header, so this test parses the
    /// named enum block out of the checked-in header's text. The C declarations
    /// come from the header, never from the descriptor under test (S45-F1).
    const HEADER_SRC: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../include/rustty-embed.h"
    ));

    /// The header's enum tag for `renderer.Health`.
    const HEADER_ENUM: &str = "rustty_action_renderer_health_e";

    fn run(src: &str) -> Result<(), String> {
        let t = health_enum_descriptor();
        let decls = header_decls(src, HEADER_ENUM);
        check_ghostty_henum(&t, "RUSTTY_RENDERER_HEALTH_", |name| {
            decls.get(name).copied()
        })
    }

    /// Self-check that the probe below can actually FAIL — i.e. that it reads
    /// the header and not the descriptor.
    ///
    /// PORT NOTE: no reference analogue; the oracle gets this for free because
    /// `@import("ghostty.h")` is the real header.
    #[test]
    fn ghostty_h_health_probe_can_fail() {
        run(HEADER_SRC).unwrap();

        let dropped = HEADER_SRC.replace("  RUSTTY_RENDERER_HEALTH_UNHEALTHY,\n", "");
        assert_ne!(dropped, HEADER_SRC);
        assert!(run(&dropped).is_err(), "dropped enumerator must fail");

        let renamed = HEADER_SRC.replace(
            "RUSTTY_RENDERER_HEALTH_UNHEALTHY",
            "RUSTTY_RENDERER_HEALTH_UNHEALTHIER",
        );
        assert_ne!(renamed, HEADER_SRC);
        assert!(run(&renamed).is_err(), "renamed enumerator must fail");

        let revalued = HEADER_SRC.replace(
            "  RUSTTY_RENDERER_HEALTH_HEALTHY,\n",
            "  RUSTTY_RENDERER_HEALTH_HEALTHY = 7,\n",
        );
        assert_ne!(revalued, HEADER_SRC);
        assert!(run(&revalued).is_err(), "revalued enumerator must fail");
    }

    /// PORT NOTE: models the reference `enum(c_int)` `Health` as an [`EnumType`]
    /// so the `checkGhosttyHEnum` test can reflect over its keys and backing
    /// int. The keys are the snake_case forms of the variants, in declaration
    /// order; the VALUES are `Health::Variant as i64`, read out of the enum.
    ///
    /// PORT NOTE (N29): this used to call `enum_type(Target::C, ..)`, the
    /// `lib.Enum` FACTORY model, which assigns each key its INDEX — so the
    /// check compared the header to a hand-typed list's ORDER and could not
    /// observe `Health` at all.
    fn health_enum_descriptor() -> EnumType {
        enum_type_values(&[
            ("healthy", Health::Healthy as i64),
            ("unhealthy", Health::Unhealthy as i64),
        ])
    }

    // test "ghostty.h Health" {
    //     try lib.checkGhosttyHEnum(Health, "GHOSTTY_RENDERER_HEALTH_");
    // }
    #[test]
    fn ghostty_h_health() {
        // PORT NOTE: `try lib.checkGhosttyHEnum(Health, "GHOSTTY_RENDERER_HEALTH_");`,
        // retargeted to `RUSTTY_RENDERER_HEALTH_` per R2. The declarations come
        // from `rustty-embed.h`'s `rustty_action_renderer_health_e` block,
        // exactly as the reference's come from the generated header.
        run(HEADER_SRC).unwrap();
    }
}
