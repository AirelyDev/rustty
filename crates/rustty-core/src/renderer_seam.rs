//! Renderer seam — core-local dependency placeholders for the GPU renderer
//! and the renderer thread (CORE_BLUEPRINT §5).
//!
//! PORTS (PLACEHOLDER SHAPES ONLY — NOT the port): ghostty
//! src/renderer/generic.zig (`renderer.Renderer`, the comptime
//! `GenericRenderer(Metal|OpenGL)`), src/renderer/Options.zig
//! (`renderer.Options`) and src/renderer/Thread.zig (the `renderer.Thread`
//! field shape). The REAL ports are Phase-3 R-d (the GPU
//! Renderer/Metal/OpenGL tier) + D6 (the xev run loop) work; they cannot land
//! in rustty-renderer today because renderer/Thread.zig imports `configpkg`
//! AND core's `App.Mailbox`, neither of which rustty-renderer may dep
//! (rustty-config already depends on rustty-renderer — the G7 cycle — and a
//! renderer -> core edge is upside down). So core carries the SHAPES it
//! embeds (Surface.zig fields `renderer: renderer.Renderer` /
//! `renderer_thread: renderer.Thread`; Surface.zig:551 `Renderer.init(alloc,
//! .{...})`), exactly the termio crate-local `xev_seam` precedent: inhabited
//! structs, faithful field/signature shapes, `unimplemented!()` method bodies
//! — dependency placeholders, with NO reference logic stubbed or weakened
//! behind them. Runtime panics are confined to these placeholder leaves;
//! everything that calls them compiles and is reviewable.
//!
//! What is NOT here (lands WITH the real Phase-3/D6 ports, nothing of it is
//! stubbed): generic.zig's GraphicsAPI/SwapChain/FrameState/CustomShaderState
//! machinery and the rest of its method surface, and renderer/Thread.zig's
//! run loop (`threadMain_`, `drainMailbox`, the render/draw/cursor timer
//! callbacks, `DRAW_INTERVAL`/`CURSOR_BLINK_INTERVAL`,
//! `must_draw_from_app_thread`). Already REAL elsewhere:
//! `rustty_renderer::state::State` and `rustty_renderer::thread::Mailbox`
//! (this placeholder `Thread<A>` points at both), and now the renderer's
//! `DerivedConfig` — the config plane is landed, so this module re-exports the
//! real `rustty_renderer::DerivedConfig` (crate-root re-export) for the
//! `Options<A>.config` field below; only the windowing `Renderer`/xev `Thread`
//! shapes remain placeholders here.
//!
//! PORT STATUS: **LIVE** (core chunk C3; the R-d GPU tier W0–W5 and the xev
//! port Milestones A/B both landed, so the `Renderer`/`Thread<A>` bodies below
//! are real). Superseded status, kept for provenance:
//! ~~placeholder (core chunk C3; filled by Phase-3 R-d + D6)~~.
//!
//! WS19 T2 audit correction [2026-07-29, audit row W1]: `DEFERRALS.md` claimed
//! this file carried "15 `unimplemented!()` C3 dependency-placeholder bodies,
//! Target: Phase-3 R-d GPU tier / D6". It does not. All 15 `unimplemented!()`
//! in this file are inside `#[cfg(test)] mod compression_tests` (`:2013`) and
//! read `unimplemented!("test runtime: not driven by the compression tests")` —
//! test-runtime fakes, the same honest test glue `app.rs` uses. **Zero
//! production leaves.** The PORTS/what-is-NOT-here prose above still describes
//! the original placeholder intent and the crate-graph reason the real GPU
//! tier lives in `rustty-renderer`; read it as history, not as current status.

// PARITY: the reference's identifiers are camelCase (`surfaceInit`,
// `finalizeSurfaceInit`, `threadEnter`, `drawFrame`, `threadMain`); preserved
// verbatim per R1/R2 (termio thread.rs precedent).
#![allow(non_snake_case)]

// PORT NOTE: `alloc: Allocator` params mirror the termio/vt convention
// (`allocator_api2::alloc::Global as Allocator`).
use allocator_api2::alloc::Global as Allocator;
use rustty_apprt_api as apprt;
use rustty_font_shaper::SharedGrid;
use rustty_renderer::size::Size;
use rustty_renderer::state::State;
use rustty_renderer::thread;
// PORT NOTE: the real cross-platform `renderer.Renderer.DerivedConfig`
// (`rustty_renderer::DerivedConfig`, crate-root re-export of `generic.rs`) —
// swapped in for the former core-local placeholder (the config plane is landed;
// `Options<A>.config` below carries it). `init<C: RendererConfig>` runs headless
// with `C = rustty_config::Config`, the type core binds that the renderer crate
// cannot.
use rustty_renderer::generic::ExportedFrame;
use rustty_renderer::latest_frame::{DrawMutex, LatestFrame};
use rustty_renderer::message::Message;
use rustty_renderer::DerivedConfig;
use rustty_xev as xev;

// PORT NOTE (W3 E2): `const crash = @import("../crash/main.zig");` — the renderer
// thread sets its crash metadata (`crash.sentry.thread_state`) in `threadMain_`.
use rustty_crash as crash;
// PORT NOTE (W3 E2): `const internal_os = @import("../os/main.zig");` — used by
// `threadMain_` (pthread_setname_np) and `setQosClass`. Both uses sit behind the
// reference's `builtin.os.tag.isDarwin()` guards, which in Rust are real `cfg`s
// rather than comptime-dead branches, so the import carries the same cfg (an
// unconditional `use` is an unused-import warning on every non-Darwin target).
#[cfg(any(target_os = "macos", target_os = "ios"))]
use rustty_os as internal_os;

use crate::surface::SurfaceMailbox;

// PORT NOTE (surface-mailbox seam): the adapter below is owned by an `Arc` inside
// the renderer's `generic::SurfaceMailbox` handle, and it pushes through the
// `AppMailbox` trait (apprt-api's replacement for the reference
// `apprt.surface.Mailbox` struct) with the queue's own `Timeout` modes.
use apprt::surface::AppMailbox;
use rustty_datastruct::blocking_queue::Timeout;
use std::sync::Arc;

// Thread.zig:19-20
// PORT NOTE (`aee7bf347`): `DRAW_INTERVAL` is DELETED with the draw timer —
// the renderer computes the cadence itself (`generic`'s `DRAW_INTERVAL_MS`
// floor inside `animation_wake`).
const CURSOR_BLINK_INTERVAL: u64 = 600;

// P7-W7 T2 (`2f0b65346`): `apprt.must_draw_from_app_thread` (hoisted from
// renderer/Thread.zig by `c4e16970a`) is deleted with the GtkGLArea. Every apprt's
// renderer now draws on its own render thread; the `drawFrame` below has one arm.

/// PORT NOTE (Rust-only glue): the reference `threadMain_` returns an INFERRED
/// error union combining the xev handle errors (`loop.run`, `wakeup.notify`)
/// with the renderer's (`loopEnter`, `threadEnter`). Rust has no inferred
/// unions, so the three concrete error types are unified in this enum;
/// `threadMain` only logs+discards it (the reference `catch |err| log.warn`),
/// so the discriminant is never inspected at runtime.
// PORT NOTE: the payloads are the values the reference `threadMain`'s
// `log.warn("error in renderer err={}", .{err})` formats. CORRECTED (task
// 14d-1): that log is now LIVE, so the payloads are read — through the derived
// `Debug`, which rustc's dead-code analysis does not count as a use, hence the
// `allow(dead_code)` stays.
#[derive(Debug)]
#[allow(dead_code)]
enum ThreadMainError {
    /// A renderer entry point failed (`loopEnter`/`threadEnter`).
    Renderer(RendererError),
    /// `wakeup.notify()` failed.
    Notify(xev::NotifyError),
    /// `loop.run(.until_done)` failed.
    Xev(xev::Error),
}

impl From<RendererError> for ThreadMainError {
    fn from(err: RendererError) -> ThreadMainError {
        ThreadMainError::Renderer(err)
    }
}
impl From<xev::NotifyError> for ThreadMainError {
    fn from(err: xev::NotifyError) -> ThreadMainError {
        ThreadMainError::Notify(err)
    }
}
impl From<xev::Error> for ThreadMainError {
    fn from(err: xev::Error) -> ThreadMainError {
        ThreadMainError::Xev(err)
    }
}

/// PORT NOTE ([`ThreadStateGuard`] family, Rust-only glue): the reference
/// `defer crash.sentry.thread_state = null;` in `threadMain_` (Thread.zig:222)
/// — the `defer` clears the crash thread-local on EVERY exit path, declared
/// right after the set so it drops in the reference's LIFO defer position.
struct RendererThreadStateGuard;
impl Drop for RendererThreadStateGuard {
    fn drop(&mut self) {
        // defer crash.sentry.thread_state = null;
        crash::sentry::clearThreadState();
    }
}

/// PORT NOTE (Rust-only glue): the reference
/// `defer if (has_loop) self.renderer.loopExit();` (Thread.zig:230). Holds the
/// raw `*mut Renderer` (the child no longer races the main thread once the
/// guard drops — `loop.run` has returned). `@hasDecl(Renderer, "loopExit")` is
/// TRUE for Metal, so this fires unconditionally.
//
// N437: it also holds the thread's own `draw_mutex` handle; see `Thread::draw_mutex`.
struct LoopExitGuard(*mut Renderer, DrawMutex);
impl Drop for LoopExitGuard {
    fn drop(&mut self) {
        // self.renderer.loopExit();
        unsafe { rustty_renderer::Renderer::loop_exit_raw(&raw mut (*self.0).0, &self.1) };
    }
}

/// PORT NOTE (Rust-only glue): the reference `defer self.renderer.threadExit();`
/// (Thread.zig:236).
struct ThreadExitGuard(*mut Renderer, DrawMutex);
impl Drop for ThreadExitGuard {
    fn drop(&mut self) {
        // self.renderer.threadExit();
        unsafe { rustty_renderer::Renderer::thread_exit_raw(&raw mut (*self.0).0, &self.1) };
    }
}

/// The error type of the [`Renderer`] entry points.
//
// PORT NOTE: the reference error sets are INFERRED error unions (`!void` /
// `!Self`, from the concrete GraphicsAPI + shader/font failures). E1 (W3)
// aliases the REAL renderer error (`rustty_renderer::generic::Error` — the
// aggregate Metal-leaf + OOM enum, generic.zig's inferred `drawFrame`/`init`
// error set), so the real variant (`MetalFailed`/`OutOfMemory`/`Defunct`/
// `ImageDecode`) propagates out of `drawFrame` faithfully rather than an opaque
// marker. Its `Display`/`std::error::Error` impls live in
// `rustty_renderer::generic` (the `write!("{self:?}")` bump); `Surface::draw`'s
// `Result<(), RendererError>` and `InitError::Renderer`'s `From`/`Display`
// delegation are unchanged.
#[cfg(target_os = "macos")]
pub type RendererError = rustty_renderer::generic::Error;

// ---------------------------------------------------------------------------
// L4 SWAP LANDED (first light): the Linux renderer error is the REAL aggregate
// `rustty_renderer::generic::Error` — the OpenGL leaf + `OutOfMemory`/`Defunct`
// enum defined in `generic::linux_impl` and re-exported by generic.rs:7565's
// `#[cfg(target_os = "linux")] pub use linux_impl::*;`. This is exactly the macOS
// shape above (its `Display`/`std::error::Error` impls live in the renderer crate),
// so core's `InitError::Renderer` `From`/`Display` delegation stays
// target-independent. The former uninhabited `LinuxRendererErrorPlaceholder` is
// deleted now that `rustty_renderer::Renderer` (and its `Error`) resolve on Linux.
//
// The `cfg` is the exact COMPLEMENT of rustty-renderer's own gate
// (`pub use generic::Renderer;` on both `macos` and `linux`, renderer/src/lib.rs),
// so every target resolves exactly one of the two arms — never both, never neither.
// ---------------------------------------------------------------------------
#[cfg(not(target_os = "macos"))]
pub type RendererError = rustty_renderer::generic::Error;

// PORT NOTE (SWAP LANDED): the `renderer.Renderer.DerivedConfig`
// (generic.zig:536, `pub fn init` generic.zig:574, `pub fn deinit`
// generic.zig:651) is no longer a core-local placeholder — it is the REAL
// cross-platform `rustty_renderer::DerivedConfig` imported above (its
// `init<C>`/`deinit` bodies live in `rustty_renderer::generic`). Core
// constructs it directly at `Surface::init` (surface.rs) with
// `C = rustty_config::Config`. NOT to be confused with the renderer THREAD's
// own `rustty_renderer::thread::DerivedConfig` (`custom_shader_animation`),
// which `Thread<A>.config` below embeds.

/// The core side of the renderer's surface-mailbox seam: adapts core's real
/// `apprt.surface.Mailbox` (`crate::surface::SurfaceMailbox<A>`) to the
/// renderer-defined [`rustty_renderer::generic::SurfaceMailboxSink`] trait.
//
// PORT NOTE (surface-mailbox seam RESOLVED, GPU_RD_CHUNK3A_DESIGN §B.2): the
// reference simply copies `apprt.surface.Mailbox` BY VALUE into the renderer
// (`renderer/Options.zig` -> `generic.zig:704 .surface_mailbox =
// options.surface_mailbox`). rustty cannot: `rustty-apprt-api ->
// rustty-renderer`, so the renderer may not name an apprt type (check_acyclic).
// The design's prescribed resolution is this adapter — the renderer declares the
// trait + its own two-variant `SurfaceMessage`, core implements the trait and
// re-wraps into the full `apprt::surface::Message`. The apprt-typed side stays in
// core exactly as the `RendererConfig` accessor trait keeps the config-typed side
// in rustty-config.
//
// Owned by an `Arc` inside `generic::SurfaceMailbox`, so `push` takes `&self`
// (the reference's `push` receiver is a by-VALUE `Mailbox`, itself `Copy`).
struct SurfaceMailboxAdapter<A: apprt::App + 'static> {
    surface_mailbox: SurfaceMailbox<A>,
}

impl<A: apprt::App + 'static> rustty_renderer::generic::SurfaceMailboxSink
    for SurfaceMailboxAdapter<A>
{
    // _ = self.surface_mailbox.push(msg, timeout);
    fn push(
        &self,
        msg: rustty_renderer::generic::SurfaceMessage,
        timeout: rustty_renderer::generic::MailboxTimeout,
    ) -> usize {
        use rustty_renderer::generic::MailboxTimeout as RTimeout;
        use rustty_renderer::generic::SurfaceMessage as RMessage;

        // `.instant` (drawFrame :1454) / `.{ .forever = {} }` (frameCompleted
        // :1735) -> the queue's own timeout modes.
        let timeout = match timeout {
            RTimeout::Instant => Timeout::Instant,
            RTimeout::Forever => Timeout::Forever,
        };
        // Re-wrap the renderer's two-variant message into the full
        // `apprt.surface.Message` union the surface handles
        // (`Surface::handleMessage` -> `updateScrollbar`/`updateRendererHealth`).
        let msg = match msg {
            RMessage::Scrollbar(scrollbar) => apprt::surface::Message::Scrollbar(scrollbar),
            RMessage::RendererHealth(health) => apprt::surface::Message::RendererHealth(health),
            RMessage::Redraw => apprt::surface::Message::Redraw,
        };
        // `App.Mailbox.Queue.Size` (u32) -> the trait's `usize` count.
        usize::try_from(self.surface_mailbox.push(msg, timeout)).unwrap_or(0)
    }
}

/// The renderer implementation — the monomorphic `rustty_renderer::Renderer`,
/// wrapped so core keeps the camelCase seam names + the `Options<A>` ctor whose
/// apprt-typed fields the real `generic::Options` cannot hold (surface-mailbox
/// seam).
//
// PORT NOTE (CORE_BLUEPRINT §5, E1/W3 SWAP LANDED): the reference is
// comptime-chosen — `pub const Renderer = switch (build_config.renderer) {
// .metal => GenericRenderer(Metal), .opengl => GenericRenderer(OpenGL), ... }`
// (renderer.zig:38) — here the macOS/Metal monomorph. Newtype forwarder (NOT a
// bare `pub use`): core's callsites use the camelCase seam names
// (`surfaceInit`/`finalizeSurfaceInit`/`threadEnter`/`drawFrame`) and core's
// `Renderer::init` takes `Options<A>` (3 apprt-typed fields the real
// `generic::Options` refuses — naming them in rustty-renderer would cycle). `.0`
// is `pub` so the same-crate `Thread<A>` callbacks reach the full real method
// surface directly through `(*self.renderer).0`; out-of-module callers only
// touch the 6 forwarders below. Surface.zig embeds it by value
// (`renderer: renderer.Renderer`).
#[cfg(target_os = "macos")]
pub struct Renderer(pub rustty_renderer::Renderer, pub RendererHandles);

/// L4 SWAP LANDED (first light): the Linux renderer — the monomorphic
/// `rustty_renderer::Renderer` = `GenericRenderer(OpenGL)` (renderer.zig:38's
/// `.opengl` arm; `generic::linux_impl::Renderer`), re-exported on Linux by
/// renderer/src/lib.rs (mirroring the macOS `.metal` line). Same newtype forwarder
/// shape as macOS: core keeps the camelCase seam names + the `Options<A>` ctor whose
/// apprt-typed fields the real `generic::Options` cannot hold, and `.0` is `pub` so
/// the same-crate `Thread<A>` callbacks reach the full real method surface directly
/// through `(*self.renderer).0`. The former `LinuxRendererPlaceholder` (and its
/// `unimplemented!()` leaves) are deleted — the `Thread<A>` call sites now type-check
/// against the real `GenericRenderer(OpenGL)` on both targets.
#[cfg(not(target_os = "macos"))]
pub struct Renderer(pub rustty_renderer::Renderer, pub RendererHandles);

/// PORT-ONLY (N437): the main thread's handles to the renderer's cross-thread
/// state, taken when the renderer is built (before the render thread starts). They
/// sit beside the renderer (the wrapper's `.1`), not in it, so the main thread
/// reaches the draw mutex and the latest-frame slot without touching the bytes the
/// render thread's `&mut` covers; see `rustty_renderer::latest_frame`.
pub struct RendererHandles {
    /// The renderer's `draw_mutex`.
    pub draw_mutex: DrawMutex,
    /// The renderer's `latest_frame` slot.
    pub latest_frame: Arc<LatestFrame<ExportedFrame>>,
}

impl Renderer {
    // P7-W7 T2 (`2f0b65346`): `surfaceInit` (`Surface.zig:493` at the pin) is
    // deleted from `generic.zig` and its caller.

    // PORT NOTE ([ADV-FIX 3]): the reference init takes `(alloc, options)` —
    // `pub fn init(alloc: Allocator, options: renderer.Options) !Self`
    // (generic.zig:658) — so the alloc STAYS on the forwarder. Callsite
    // Surface.zig:551. Maps `Options<A>` -> the real `generic::Options`
    // (`config`/`font_grid`/`size`); the apprt-typed `surface_mailbox`/
    // `rt_surface`/`thread` stay in core (surface-mailbox seam: the real
    // renderer does not store them).
    //
    // ERRDEFER PARITY (discharged by the real init): `options.config` is the
    // REAL `rustty_renderer::DerivedConfig` (moved in). The reference frees a
    // partially-built config via its own errdefer on the `init` error path;
    // that errdefer lives INSIDE the real `Renderer::init` (it owns the moved
    // `DerivedConfig`), so the forwarder does NOT re-run `config.deinit()` on
    // error — doing so would be a double-free.
    pub fn init<A: apprt::App + 'static>(
        alloc: Allocator,
        options: Options<A>,
    ) -> Result<Renderer, RendererError> {
        let Options {
            config,
            font_grid,
            size,
            surface_mailbox,
            rt_surface,
            thread,
        } = options;
        let _ = (rt_surface, thread); // held-in-core seam bits

        // SURFACE-MAILBOX SEAM RESOLVED (GPU_RD_CHUNK3A_DESIGN §B.2): the real
        // `apprt.surface.Mailbox` is no longer DISCARDED here — it is wrapped in
        // the renderer-defined `SurfaceMailboxSink` and handed to the renderer, so
        // `drawFrame`'s `.scrollbar` push (generic.zig:1452-1454) and
        // `frameCompleted`'s `.renderer_health` push (:1735-1737) reach
        // `Surface::handleMessage` for real.
        let surface_mailbox =
            rustty_renderer::generic::SurfaceMailbox::new(Arc::new(SurfaceMailboxAdapter {
                surface_mailbox,
            }));
        let inner = rustty_renderer::Renderer::init(
            alloc,
            rustty_renderer::generic::Options {
                config,
                font_grid,
                size,
                surface_mailbox,
            },
        )?;
        // N437: the main thread's handles, taken before the render thread starts.
        let handles = RendererHandles {
            draw_mutex: inner.draw_mutex.clone(),
            latest_frame: inner.latest_frame_handle(),
        };
        Ok(Renderer(inner, handles))
    }

    // P7-W7 T2 (`2f0b65346`): `finalizeSurfaceInit` (`Surface.zig:712` at the pin)
    // is deleted from `generic.zig` and its caller.

    /// W-ATTACH: route the host NSView into the renderer's windowed layer-attach.
    /// Forwards the raw view pointer + scale (core reads them from the apprt
    /// surface's `host_view()`/`host_content_scale()`); the renderer never names an
    /// apprt type (R4). Called from `Surface::init` right after
    /// `finalizeSurfaceInit`, before the render thread spawns.
    pub fn attachHostView(
        &mut self,
        view: *mut core::ffi::c_void,
        scale: f64,
    ) -> Result<(), RendererError> {
        #[cfg(target_os = "macos")]
        {
            self.0.attach_host_view(view, scale)
        }
        #[cfg(not(target_os = "macos"))]
        {
            // OpenGL: no windowed host-attach. The renderer owns a surfaceless EGL
            // context and exports its frames to the apprt (`2f0b65346`); the real
            // leaf is a no-op on a null/typeless view (linux_impl
            // `attach_host_view`), matching the reference's non-macOS `attach_host_view`.
            self.0.attach_host_view(view, scale)
        }
    }

    // P7-W7 T2 (`2f0b65346`): the surface-deinit `threadEnter` forwarder is gone
    // with its caller ("We need to become the active rendering thread again",
    // `Surface.zig:810-811` at the pin): the render thread owns the GL context.

    /// Draw the frame to the screen.
    ///
    /// If `sync` is true, this will synchronously block until
    /// the frame is finished drawing and has been presented.
    //
    // PORT NOTE: `pub fn drawFrame(self: *Self, sync: bool) !void`
    // (generic.zig:1430); callsite Surface.zig:884 (`draw` — "Renderers are
    // required to support `drawFrame` being called from the main thread").
    //
    // W5 (`W5_DESIGN §2.5` caller 2 / §3.4b): RAW-self, per the Zig `self: *Self`
    // receiver. A `&mut self` receiver would be a protected `&mut` over the same
    // bytes as `rustty_renderer::Renderer` (newtype at offset 0), UB-concurrent
    // with the render thread's `draw_frame_raw`. The port's prior `&mut self` was
    // the divergence; raw-self is the faithful shape.
    //
    /// # Safety
    /// `self_` must be a valid, stable `*mut Renderer` (the Surface-embedded renderer,
    /// aliased raw by the render thread). No protected `&mut` may span the call.
    //
    // N437: the lock is the main thread's own handle, `(*self_).1.draw_mutex`
    // (beside the renderer, not in it).
    pub unsafe fn drawFrame(self_: *mut Renderer, sync: bool) -> Result<(), RendererError> {
        // self.0.draw_frame(sync)  →  raw-self draw (no protector spans the call)
        let lock: &DrawMutex = unsafe { &(*self_).1.draw_mutex };
        unsafe { rustty_renderer::Renderer::draw_frame_raw(&raw mut (*self_).0, lock, sync) }
    }

    /// Take the latest completed frame for the apprt to composite.
    /// Returns null if no frame is available. The caller takes
    /// ownership of the returned frame. Called on the main thread.
    //
    // PORT NOTE: `pub fn takeFrame(self: *Self) ?ExportedFrame` (generic.zig:1036);
    // callsite `render_surface.zig:156` (`core.renderer.takeFrame()`). N437: through
    // the main thread's handle to the slot, touching no renderer byte.
    ///
    /// # Safety
    /// `self_` is the Surface-embedded renderer.
    pub unsafe fn takeFrame(self_: *const Renderer) -> Option<ExportedFrame> {
        unsafe { (*self_).1.latest_frame.take() }
    }

    // PORT NOTE: `pub fn displayRealized(self: *Self) !void` (generic.zig:947-968);
    // callsite `Surface.displayRealized` (`Surface.zig:2544`), from the GTK apprt's
    // `renderSurfaceRealize`. Only flags change (`2f0b65346`); the render thread
    // rebuilds its GPU resources on the next draw.
    //
    // Raw-self, per the `drawFrame` precedent (W5): the renderer is aliased raw by
    // the render thread, so no protected `&mut` may span the call; the lock is the
    // main thread's own handle (N437).
    //
    /// # Safety
    /// `self_` must be a valid, stable `*mut Renderer` (the Surface-embedded
    /// renderer, aliased raw by the render thread). No protected `&mut` may span.
    pub unsafe fn displayRealized(self_: *mut Renderer) -> Result<(), RendererError> {
        let lock: &DrawMutex = unsafe { &(*self_).1.draw_mutex };
        unsafe { rustty_renderer::Renderer::display_realized_raw(&raw mut (*self_).0, lock) }
    }

    // PORT NOTE: `pub fn displayUnrealized(self: *Self) void` (generic.zig:970-985);
    // callsite `Surface.displayUnrealized` (`Surface.zig:2550`), from
    // `renderSurfaceUnrealize` and `initSurface`. Raw-self per `drawFrame`.
    //
    /// # Safety
    /// As [`Renderer::displayRealized`].
    pub unsafe fn displayUnrealized(self_: *mut Renderer) {
        let lock: &DrawMutex = unsafe { &(*self_).1.draw_mutex };
        unsafe { rustty_renderer::Renderer::display_unrealized_raw(&raw mut (*self_).0, lock) }
    }

    // PORT NOTE: `pub fn deinit(self: *Self) void` (generic.zig:800).
    pub fn deinit(&mut self) {
        #[cfg(target_os = "macos")]
        {
            self.0.deinit();
        }
        #[cfg(not(target_os = "macos"))]
        {
            self.0.deinit();
        }
    }
}

/// The options that are used to configure a renderer.
//
// PORT NOTE: renderer/Options.zig (the file IS the struct -> named `Options`;
// termio options.rs precedent). Generic over the apprt backend `A`
// (CORE_BLUEPRINT §5): `surface_mailbox`/`rt_surface`/`thread` name
// backend-parameterized types. The blueprint's §5 sketch wrote
// `Options<'_, A>`, but the reference file has NO borrowed field (`config` is
// by value, the rest are raw pointers), so a lifetime parameter would be
// unused (E0392) and is dropped — faithful to Options.zig.
pub struct Options<A: apprt::App + 'static> {
    /// The derived configuration for this renderer implementation.
    //
    // PORT NOTE: `config: renderer.Renderer.DerivedConfig` — now the real
    // `rustty_renderer::DerivedConfig` (crate-root re-export, imported above);
    // the seam `Options<A>` still carries it because the real
    // `rustty_renderer::Options` cannot hold the 3 apprt-typed fields below
    // (`rustty-apprt-api -> rustty-renderer` would cycle).
    pub config: DerivedConfig,

    /// The font grid that should be used along with the key for deref-ing.
    //
    // PORT NOTE: `font_grid: *font.SharedGrid` — rustty-font-shaper's
    // `SharedGrid` (the D1 3-tier shaper crate ABOVE vt+font;
    // `SharedGridSet::ref` returns exactly this `*mut SharedGrid`).
    pub font_grid: *mut SharedGrid,

    /// The size of everything.
    pub size: Size,

    /// The mailbox for sending the surface messages. This is only valid
    /// once the thread has started and should not be used outside of the thread.
    //
    // PORT NOTE: `surface_mailbox: apprt.surface.Mailbox` — core's
    // `SurfaceMailbox<A>` (CORE_BLUEPRINT §1; declared in `crate::surface`,
    // C3 skeleton).
    pub surface_mailbox: SurfaceMailbox<A>,

    /// The apprt surface.
    pub rt_surface: *mut A::Surface,

    /// The renderer thread.
    pub thread: *mut Thread<A>,
}

// const log = std.log.scoped(.renderer_thread);   (renderer/Thread.zig:19)
//
// PORT NOTE (task 14d-1): the reference `log.debug/info/warn/err` sites in
// `renderer/Thread.zig` are preserved verbatim as comments at each site, with
// the LIVE emission added directly beneath each one
// (`.port/parity-closure/DECISION-logging.md` §6 call shape 2 — `rustty-core`
// is below `rustty-vt-c` in R4's layering, so restored sites go through the
// `log` facade). `LOG_SCOPE` is the `@tagName` of the scope above. Note that
// `renderer/Thread.zig` ports into `rustty-core`, not `rustty-renderer` — a
// deliberate layering decision — so this scope lives here.
const LOG_SCOPE: &str = "renderer_thread";

/// The `{}` image of a renderer `Message`, for `drainMailbox`'s restored
/// `log.debug("mailbox message={}", .{message})` site (`Thread.zig:308`).
///
/// PORT NOTE (task 14d-1): Zig's default `{}` on a `union(enum)` renders the
/// ACTIVE TAG AND ITS PAYLOAD, so a tag name alone would not be this site. The
/// port cannot get there through `{:?}`: `renderer::Message` has no `Debug`
/// impl, and deriving one cascades into `rustty-font-shaper`
/// (`shared_grid_set::Key`), `renderer::thread::DerivedConfig`,
/// `renderer::generic::DerivedConfig`, `SearchMatches` and `SearchMatch` — five
/// types across two other crates, none of them this row's subject. So the
/// formatting lives HERE, at the one call site that needs it, and renders the
/// reference's own union syntax (`.tag = payload`) with the reference's own
/// field names.
///
/// Two knowing divergences, both documented rather than hidden:
///  * Zig prefixes the union's `@typeName`; this prints the braces alone.
///  * Four payload types are not printable without the cascade above, so they
///    render as an elision (`..`) or a cheap projection (a pointer, a count)
///    instead of their fields. The TAG — which is what the trace is for — is
///    exact for every variant.
struct MessageDbg<'a>(&'a rustty_renderer::Message);

impl core::fmt::Display for MessageDbg<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use rustty_renderer::Message as M;
        match self.0 {
            M::Crash => write!(f, "{{ .crash = void }}"),
            M::Focus(v) => write!(f, "{{ .focus = {v} }}"),
            M::Visible(v) => write!(f, "{{ .visible = {v} }}"),
            M::ResetCursorBlink => write!(f, "{{ .reset_cursor_blink = void }}"),
            M::FontGrid { grid, set, .. } => write!(
                f,
                "{{ .font_grid = {{ .grid = {grid:?}, .set = {set:?}, .new_key = .., .old_key = .. }} }}"
            ),
            M::Resize(v) => write!(f, "{{ .resize = {v:?} }}"),
            M::ChangeConfig { .. } => {
                write!(f, "{{ .change_config = {{ .thread = .., .impl = .. }} }}")
            }
            M::SearchViewportMatches(v) => write!(
                f,
                "{{ .search_viewport_matches = {{ .matches = ..[{}] }} }}",
                v.matches.len()
            ),
            M::SearchSelectedMatch(v) => write!(
                f,
                "{{ .search_selected_match = {} }}",
                if v.is_some() { ".." } else { "null" }
            ),
            M::Inspector(v) => write!(f, "{{ .inspector = {v} }}"),
            M::MacosDisplayId(v) => write!(f, "{{ .macos_display_id = {v} }}"),
            // Zig's `{}` on an enum payload renders `.tag`.
            M::PresentationHealth(v) => write!(
                f,
                "{{ .presentation_health = {} }}",
                match v {
                    rustty_renderer::Health::Healthy => ".healthy",
                    rustty_renderer::Health::Unhealthy => ".unhealthy",
                }
            ),
        }
    }
}

/// Represents the renderer thread logic. The renderer thread is able to
/// be woken up to render.
//
// PORT NOTE (DEPENDENCY PLACEHOLDER, CORE_BLUEPRINT §5): renderer/Thread.zig
// — the FAITHFUL field shape (what Surface.zig embeds and wires:
// `renderer_thread: renderer.Thread`, the `.renderer_wakeup =
// render_thread.wakeup` handle copy at :675, `renderer_thread.mailbox` pushes,
// `renderer_thread.stop.notify()` at :796), with `init`/`threadMain`/`deinit`
// as `unimplemented!()` placeholders. Generic over the apprt backend `A`
// (reference fields `surface: *apprt.Surface`, `app_mailbox: App.Mailbox`).
// The reference's first field `alloc: std.mem.Allocator`
// ("Allocator used for some state") collapses — Rust `Box` ownership carries
// the mailbox cleanup (termio thread.rs precedent). Field defaults
// (`= .{}` / `= false`) are documented at each field for the Phase-3 fill.
pub struct Thread<A: apprt::App + 'static> {
    /// The main event loop for the application. The user data of this loop
    /// is always the allocator used to create the loop. This is a convenience
    /// so that users of the loop always have an allocator.
    //
    // PORT NOTE: reference field name `loop` is a Rust keyword -> `loop_`
    // (termio thread.rs precedent).
    pub loop_: xev::Loop,

    /// This can be used to wake up the renderer and force a render safely from
    /// any thread.
    pub wakeup: xev::Async,
    // (reference default: `= .{}`)
    pub wakeup_c: xev::Completion,

    /// This can be used to stop the renderer on the next loop iteration.
    pub stop: xev::Async,
    // (reference default: `= .{}`)
    pub stop_c: xev::Completion,

    /// The timer used for animations (custom shaders, Kitty graphics).
    /// Normal rendering is driven by wakeup messages instead.
    pub render_h: xev::Timer,
    // (reference default: `= .{}`)
    pub render_c: xev::Completion,
    // (reference default: `= .{}`)
    pub render_c_cancel: xev::Completion,

    /// The kind of work the currently scheduled animation wake needs,
    /// stored when the timer is armed.
    //
    // animation_wake: rendererpkg.Renderer.AnimationWake.Kind = .draw,
    pub animation_wake: rustty_renderer::generic::AnimationWakeKind,

    /// This async is used to force a draw immediately. This does not
    /// coalesce like the wakeup does.
    pub draw_now: xev::Async,
    // (reference default: `= .{}`)
    pub draw_now_c: xev::Completion,

    /// The timer used for cursor blinking
    pub cursor_h: xev::Timer,
    // (reference default: `= .{}`)
    pub cursor_c: xev::Completion,
    // (reference default: `= .{}`)
    pub cursor_c_cancel: xev::Completion,

    /// Incremental scrollback compression scheduling.
    //
    // PORT NOTE (WS7 D3): reference `compression: Compression = undefined` — see
    // [`Compression`] for the `undefined`/gating deviation. `pub(crate)` (not
    // `pub`) because `Compression` itself is crate-private, mirroring the
    // reference's file-private `const Compression = struct`.
    pub(crate) compression: Compression,

    /// The surface we're rendering to.
    //
    // PORT NOTE: `surface: *apprt.Surface` -> the backend surface pointer.
    pub surface: *mut A::Surface,

    /// The underlying renderer implementation.
    //
    // PORT NOTE: `renderer: *rendererpkg.Renderer` -> the placeholder above.
    pub renderer: *mut Renderer,

    /// Pointer to the shared state that is used to generate the final render.
    //
    // PORT NOTE: `state: *rendererpkg.State` — REAL, already landed
    // (`rustty_renderer::state::State`).
    pub state: *mut State,

    /// The mailbox that can be used to send this thread messages. Note
    /// this is a blocking queue so if it is full you will get errors (or block).
    //
    // PORT NOTE: `mailbox: *Mailbox` — REAL, already landed
    // (`rustty_renderer::thread::Mailbox` =
    // `BlockingQueue<renderer::Message, 64>`).
    pub mailbox: *mut thread::Mailbox,

    // P7-W7 T2 (`2f0b65346`): `app_mailbox: App.Mailbox` ("Mailbox to send
    // messages to the app thread") is deleted with `must_draw_from_app_thread`'s
    // `.redraw_surface` push, its only use.
    /// PORT-ONLY (N437): the render thread's own handle to the renderer's
    /// `draw_mutex`, taken in `init` (before the thread starts). Every call into
    /// the renderer locks through it before forming a reference to the renderer;
    /// see `rustty_renderer::latest_frame`.
    pub draw_mutex: DrawMutex,

    /// Configuration we need derived from the main config.
    //
    // PORT NOTE: `config: DerivedConfig` — renderer/Thread.zig's OWN
    // `DerivedConfig` (`custom_shader_animation`), the [ADV-FIX 2] INHABITED
    // placeholder in `rustty_renderer::thread` — NOT this module's
    // `renderer.Renderer.DerivedConfig` placeholder above.
    pub config: thread::DerivedConfig,

    // (reference default: `= .{}`)
    pub flags: Flags,
}

/// PORT NOTE: the reference's inline `flags: packed struct { ... } = .{}`
/// (renderer/Thread.zig:94).
pub struct Flags {
    /// This is true when a blinking cursor should be visible and false
    /// when it should not be visible. This is toggled on a timer by the
    /// thread automatically.
    // (reference default: `= false`)
    pub cursor_blink_visible: bool,

    /// This is true when the inspector is active.
    // (reference default: `= false`)
    pub has_inspector: bool,

    /// This is true when the view is visible. This is used to determine
    /// if we should be rendering or not.
    // (reference default: `= true`)
    pub visible: bool,

    /// This is true when the view is focused. This defaults to true
    /// and it is up to the apprt to set the correct value.
    // (reference default: `= true`)
    pub focused: bool,
}

// PORT NOTE: MANUAL `Default` — the reference defaults are not all-false
// (`visible = true`, `focused = true`), so `#[derive(Default)]` would be
// unfaithful.
impl Default for Flags {
    fn default() -> Flags {
        Flags {
            cursor_blink_visible: false,
            has_inspector: false,
            visible: true,
            focused: true,
        }
    }
}

impl<A: apprt::App + 'static> Thread<A> {
    /// Initialize the thread. This does not START the thread. This only sets
    /// up all the internal state necessary prior to starting the thread. It
    /// is up to the caller to start the thread with the threadMain entrypoint.
    //
    // PORT NOTE: `pub fn init(alloc: Allocator, config: *const
    // configpkg.Config, surface: *apprt.Surface, renderer_impl:
    // *rendererpkg.Renderer, state: *rendererpkg.State) !Thread`
    // (renderer/Thread.zig:116 at `12752b2ac`; `2f0b65346` dropped the
    // `app_mailbox` parameter); callsite Surface.zig:577. Error type: the xev handle-init failures (termio
    // `Thread::init` precedent; `Mailbox.create`'s alloc failure collapses
    // into Rust's infallible `Box`). The `alloc` param is kept on the
    // placeholder ([ADV-FIX 3] doctrine).
    pub fn init(
        alloc: Allocator,
        config: &rustty_config::Config,
        surface: *mut A::Surface,
        renderer_impl: *mut Renderer,
        state: *mut State,
        draw_mutex: DrawMutex,
    ) -> Result<Thread<A>, xev::Error> {
        // PORT NOTE ([ADV-FIX 3]): the `alloc` param is kept on the placeholder
        // signature, but the reference `Mailbox.create(alloc)` collapses to
        // Rust `Box` ownership, so `alloc` is unused here.
        let _ = alloc;

        // PORT NOTE (errdefer parity, CONFIRMED): rustty-xev has NO `impl Drop`
        // for `Loop`/`Timer`/`Async` — every teardown is an explicit `deinit()`
        // syscall. So the reference `errdefer h.deinit()` ordering
        // (Thread.zig:135-163) does NOT happen for free on the `?` early return;
        // it is reproduced explicitly: on each `Err`, `deinit()` the
        // earlier-initialized handles in LIFO order, then return the error (the
        // surface.rs errdefer-chain precedent). Pre-spawn + single-threaded, so
        // no aliasing concern.

        // Create our event loop.
        // var loop = try xev.Loop.init(.{});
        // errdefer loop.deinit();
        let mut loop_ = xev::Loop::init(xev::Options::default())?;

        // This async handle is used to "wake up" the renderer and force a render.
        // var wakeup_h = try xev.Async.init();
        // errdefer wakeup_h.deinit();
        let mut wakeup_h = match xev::Async::init() {
            Ok(v) => v,
            Err(e) => {
                loop_.deinit();
                return Err(e);
            }
        };

        // This async handle is used to stop the loop and force the thread to end.
        // var stop_h = try xev.Async.init();
        // errdefer stop_h.deinit();
        let mut stop_h = match xev::Async::init() {
            Ok(v) => v,
            Err(e) => {
                wakeup_h.deinit();
                loop_.deinit();
                return Err(e);
            }
        };

        // The primary timer for rendering.
        // var render_h = try xev.Timer.init();
        // errdefer render_h.deinit();
        let render_h = match xev::Timer::init() {
            Ok(v) => v,
            Err(e) => {
                stop_h.deinit();
                wakeup_h.deinit();
                loop_.deinit();
                return Err(e);
            }
        };

        // (PORT NOTE `aee7bf347`: the draw timer is DELETED — `render_h` is the
        // animation timer now.)

        // Draw now async, see comments.
        // var draw_now = try xev.Async.init();
        // errdefer draw_now.deinit();
        let mut draw_now = match xev::Async::init() {
            Ok(v) => v,
            Err(e) => {
                render_h.deinit();
                stop_h.deinit();
                wakeup_h.deinit();
                loop_.deinit();
                return Err(e);
            }
        };

        // Setup a timer for blinking the cursor
        // var cursor_timer = try xev.Timer.init();
        // errdefer cursor_timer.deinit();
        let cursor_timer = match xev::Timer::init() {
            Ok(v) => v,
            Err(e) => {
                draw_now.deinit();
                render_h.deinit();
                stop_h.deinit();
                wakeup_h.deinit();
                loop_.deinit();
                return Err(e);
            }
        };

        // The mailbox for messaging this thread
        // var mailbox = try Mailbox.create(alloc);
        // errdefer mailbox.destroy(alloc);
        //
        // (PORT NOTE: `Mailbox.create(alloc)` -> the REAL infallible
        // `BlockingQueue::create()` returning `Box<Self>`; `Box::into_raw` for
        // the `*mut Mailbox` field. No `try`/errdefer — infallible.)
        let mailbox: *mut thread::Mailbox = Box::into_raw(thread::Mailbox::create());

        // Only enable compression if we have it enabled... save some
        // minor resources.
        // if (comptime terminalpkg.compression_enabled) {
        //     result.compression = try .init();
        // }
        //
        // PORT NOTE (WS7 D3, the ONE deviation): the reference declares the field
        // `compression: Compression = undefined` and assigns it only inside the
        // comptime gate, leaving it UNDEFINED on unsupported targets. Rust has no
        // `undefined`, and the honest analogue (`MaybeUninit` + `assume_init` at
        // every touch) would trade a real UB hazard for nothing: `xev::Timer` is a
        // zero-sized handle on every backend rustty supports (kqueue/epoll/
        // io_uring all keep timer state in the loop, not the watcher), so
        // `Compression::init()` allocates and syscalls nothing. So the field is
        // built unconditionally here, and the `compression_enabled()` gate is
        // preserved at every BEHAVIORAL site verbatim (`deinit`, `changeConfig`,
        // `Compression::wake`) — nothing is scheduled or stepped on an unsupported
        // target. The reference's ordering (construct `result`, then assign) is
        // flattened into the struct literal below; the errdefer chain is the same.
        let compression = match Compression::init() {
            Ok(v) => v,
            Err(e) => {
                // (the still-live errdefers, LIFO — plus the mailbox's)
                unsafe { thread::Mailbox::destroy(Box::from_raw(mailbox)) };
                cursor_timer.deinit();
                draw_now.deinit();
                render_h.deinit();
                stop_h.deinit();
                wakeup_h.deinit();
                loop_.deinit();
                return Err(e);
            }
        };

        // return .{ ... };
        Ok(Thread {
            // .alloc = alloc,  (collapsed — Box ownership)
            // .config = .init(config),
            //
            // (PORT NOTE: `rustty_config::Config` impls the renderer-defined
            // `RendererConfig` accessor, so the thread `DerivedConfig::init` runs
            // with `C = rustty_config::Config`.)
            config: thread::DerivedConfig::init(config),
            // .loop = loop,
            loop_,
            // .wakeup = wakeup_h,
            wakeup: wakeup_h,
            wakeup_c: xev::Completion::default(),
            // .stop = stop_h,
            stop: stop_h,
            stop_c: xev::Completion::default(),
            // .render_h = render_h,
            render_h,
            render_c: xev::Completion::default(),
            render_c_cancel: xev::Completion::default(),
            // (reference struct-decl default: `animation_wake = .draw`)
            animation_wake: rustty_renderer::generic::AnimationWakeKind::Draw,
            // .draw_now = draw_now,
            draw_now,
            draw_now_c: xev::Completion::default(),
            // .cursor_h = cursor_timer,
            cursor_h: cursor_timer,
            cursor_c: xev::Completion::default(),
            cursor_c_cancel: xev::Completion::default(),
            // (see the PORT NOTE above — the reference's post-construction
            // `result.compression = try .init()` under the comptime gate)
            compression,
            // .surface = surface,
            surface,
            // .renderer = renderer_impl,
            renderer: renderer_impl,
            // .state = state,
            state,
            // .mailbox = mailbox,
            mailbox,
            // N437: the render thread's handle (Rust-only parameter).
            draw_mutex,
            flags: Flags::default(),
        })
    }

    /// Clean up the thread. This is only safe to call once the thread
    /// completes executing; the caller must join prior to this.
    //
    // PORT NOTE: `pub fn deinit(self: *Thread) void` -> `&mut self` (NOT
    // consuming): Surface.zig calls it on the still-owned field
    // (`self.renderer_thread.deinit()`, Surface.zig:813 + the :575 errdefer).
    pub fn deinit(&mut self) {
        // self.stop.deinit();
        self.stop.deinit();
        // self.wakeup.deinit();
        self.wakeup.deinit();
        // self.render_h.deinit();
        self.render_h.deinit();
        // self.draw_now.deinit();
        self.draw_now.deinit();
        // self.cursor_h.deinit();
        self.cursor_h.deinit();
        // if (comptime terminalpkg.compression_enabled)
        //     self.compression.deinit();
        if rustty_vt::compression_enabled() {
            self.compression.deinit();
        }
        // self.loop.deinit();
        self.loop_.deinit();

        // Nothing can possibly access the mailbox anymore, destroy it.
        // self.mailbox.destroy(self.alloc);
        //
        // (PORT NOTE: `Mailbox.destroy(alloc)` -> `BlockingQueue::destroy(Box<Self>)`;
        // reconstitute the owning `Box` from the raw pointer. `deinit` runs
        // post-join — the child is dead, no aliasing.)
        unsafe { thread::Mailbox::destroy(Box::from_raw(self.mailbox)) };
    }
}

// PORT NOTE (W3 E2b, THE ALIASING DISCIPLINE): after the spawn
// (surface.rs) the child thread holds `*mut Thread<A>` for its whole life; the
// xev loop hands THAT SAME raw pointer back into every callback as `userdata:
// *mut Thread<A>`. So the faithful port is raw-self throughout — `threadMain`
// takes `self_: *mut Thread<A>` (NOT `&mut self`); there is NO
// `let t = &mut *self_;` whole-struct reborrow anywhere. Every callback is a
// SAFE fn (raw-ptr derefs in explicit `unsafe` blocks — an `unsafe fn` item
// does NOT impl `Fn`, so xev's ZST-callback bound forces safe fns; the termio
// io_thread precedent) taking raw `self_` with only narrow per-field
// projections `(*self_).<field>`. xev registration args stay raw: the loop
// re-enters the completion `c` across ticks, so a `&mut` re-read would be UB.
// This shape is proven on the mirihar-chunk3b `renderthread_shipped` (CLEAN
// under SB + TB) vs `renderthread_naive` (UB) harness.
//
// The `where A::Surface: apprt::Surface<Core = Surface<A>>` bound gives
// `threadMain_` the typed `rt_surface.core()` edge (the crash-metadata target);
// every callsite (Surface::init's spawn, in surface.rs) is inside a Surface
// impl block carrying the same bound.
impl<A: apprt::App + 'static> Thread<A>
where
    A::Surface: apprt::Surface<App = A, Core = crate::surface::Surface<A>>,
{
    /// The main entrypoint for the thread.
    //
    // PORT NOTE (E2b: RAW self, was `&mut self`): `pub fn threadMain(self:
    // *Thread) void` (Thread.zig:199). NO whole reborrow — see the impl-block
    // aliasing note above.
    ///
    /// # Safety
    ///
    /// `self_` must point at a fully-initialized, frame-pinned `Thread<A>` that
    /// the child thread is the sole user of until the `deinit` join (the
    /// `RendererThreadArg` contract, surface.rs).
    pub unsafe fn threadMain(self_: *mut Thread<A>) {
        // Call child function so we can use errors...
        // self.threadMain_() catch |err| {
        //     // In the future, we should expose this on the thread struct.
        //     log.warn("error in renderer err={}", .{err});
        // };
        if let Err(err) = unsafe { Self::threadMain_(self_) } {
            // In the future, we should expose this on the thread struct.
            // log.warn("error in renderer err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error in renderer err={err:?}");
        }
    }

    // fn threadMain_(self: *Thread) !void {
    unsafe fn threadMain_(self_: *mut Thread<A>) -> Result<(), ThreadMainError> {
        // defer log.debug("renderer thread exited", .{});
        //
        // PORT NOTE (task 14d-1): a Zig `defer` at function scope runs on
        // EVERY exit, including the error returns below, so the emission goes
        // in a `Drop` guard rather than at the tail — a tail call would be
        // skipped on exactly the failure paths the diagnostic exists to
        // bracket.
        struct ThreadExited;
        impl Drop for ThreadExited {
            fn drop(&mut self) {
                log::debug!(target: LOG_SCOPE, "renderer thread exited");
            }
        }
        let _thread_exited = ThreadExited;

        // Right now, on Darwin, `std.Thread.setName` can only name the current
        // thread, and we have no way to get the current thread from within it,
        // so instead we use this code to name the thread instead.
        // if (builtin.os.tag.isDarwin()) {
        //     internal_os.macos.pthread_setname_np(&"renderer".*);
        // }
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        unsafe {
            internal_os::macos::pthread_setname_np(c"renderer".as_ptr());
        }

        // Setup our crash metadata
        // crash.sentry.thread_state = .{
        //     .type = .renderer,
        //     .surface = self.renderer.surface_mailbox.surface,
        // };
        // defer crash.sentry.thread_state = null;
        //
        // (PORT NOTE: the real Renderer dropped `surface_mailbox` (the surface-mailbox
        // seam), so the `ThreadStateSurface` target — the core `Surface<A>` — is
        // obtained from the Thread's apprt surface via `.core()`. Core's
        // `Surface<A>` impls `crash::sentry::ThreadStateSurface`.)
        let core_surface: *mut crate::surface::Surface<A> =
            unsafe { <A::Surface as apprt::Surface>::core(&*(*self_).surface) };
        crash::sentry::setThreadState(crash::sentry::ThreadState {
            type_: crash::sentry::Type::Renderer,
            surface: core_surface as *const crate::surface::Surface<A>,
        });
        let _thread_state = RendererThreadStateGuard;

        // Setup our thread QoS
        // self.setQosClass();
        Self::setQosClass(self_);

        // Run our loop start/end callbacks if the renderer cares.
        // const has_loop = @hasDecl(rendererpkg.Renderer, "loopEnter");
        // if (has_loop) try self.renderer.loopEnter(self);
        // defer if (has_loop) self.renderer.loopExit();
        //
        // (`@hasDecl(Renderer, "loopEnter")` is TRUE for Metal ⇒ the real
        // `loop_enter`. R4: the reference passes `self` and reads `&thr.draw_now`;
        // the real signature takes the Copy `draw_now: Async` directly.)
        //
        // Windows (task W-2, N342): on IOCP `xev.Async` is a value, not a Copy
        // handle, so the by-value argument is Zig's value copy (`.clone()`); the
        // OpenGL leaf has no `loopEnter` and ignores it.
        #[allow(clippy::clone_on_copy)]
        let draw_now = unsafe { (*self_).draw_now.clone() };
        // N437: through the thread's own lock handle; see `Thread::draw_mutex`.
        let lock: DrawMutex = unsafe { (*self_).draw_mutex.clone() };
        unsafe {
            rustty_renderer::Renderer::loop_enter_raw(
                &raw mut (*(*self_).renderer).0,
                &lock,
                draw_now,
            )
        }?;
        let _loop_exit = LoopExitGuard(unsafe { (*self_).renderer }, lock.clone());

        // Run our thread start/end callbacks. This is important because some
        // renderers have to do per-thread setup. For example, OpenGL has to set
        // some thread-local state since that is how it works.
        // try self.renderer.threadEnter(self.surface);
        // defer self.renderer.threadExit();
        unsafe {
            rustty_renderer::Renderer::thread_enter_raw(
                &raw mut (*(*self_).renderer).0,
                &lock,
                (*self_).surface,
            )
        }?;
        let _thread_exit = ThreadExitGuard(unsafe { (*self_).renderer }, lock);

        // Start the async handlers
        // self.wakeup.wait(&self.loop, &self.wakeup_c, Thread, self, wakeupCallback);
        // self.stop.wait(&self.loop, &self.stop_c, Thread, self, stopCallback);
        // self.draw_now.wait(&self.loop, &self.draw_now_c, Thread, self, drawNowCallback);
        unsafe {
            (*self_).wakeup.wait(
                &raw mut (*self_).loop_,
                &raw mut (*self_).wakeup_c,
                self_,
                Self::wakeupCallback,
            );
            (*self_).stop.wait(
                &raw mut (*self_).loop_,
                &raw mut (*self_).stop_c,
                self_,
                Self::stopCallback,
            );
            (*self_).draw_now.wait(
                &raw mut (*self_).loop_,
                &raw mut (*self_).draw_now_c,
                self_,
                Self::drawNowCallback,
            );
        }

        // Send an initial wakeup message so that we render right away.
        // try self.wakeup.notify();
        unsafe { (*self_).wakeup.notify() }?;

        // Start blinking the cursor.
        // self.cursor_h.run(&self.loop, &self.cursor_c, cursorBlinkInterval(), Thread, self, cursorTimerCallback);
        unsafe {
            (*self_).cursor_h.run(
                &raw mut (*self_).loop_,
                &raw mut (*self_).cursor_c,
                cursorBlinkInterval(),
                self_,
                Self::cursorTimerCallback,
            );
        }

        // Arm the animation timer in case the renderer already needs
        // animation wakes (e.g. custom shaders loaded at startup).
        // self.armAnimationTimer();
        Self::armAnimationTimer(self_);

        // Run
        // log.debug("starting renderer thread", .{});
        // defer log.debug("starting renderer thread shutdown", .{});
        // _ = try self.loop.run(.until_done);
        log::debug!(target: LOG_SCOPE, "starting renderer thread");
        // Same `defer`-is-not-a-tail-call reasoning as `ThreadExited` above:
        // `loop.run` is a `try`, so a tail emission would be skipped on the
        // exact path this line is meant to bracket.
        struct ThreadShutdown;
        impl Drop for ThreadShutdown {
            fn drop(&mut self) {
                log::debug!(target: LOG_SCOPE, "starting renderer thread shutdown");
            }
        }
        let _thread_shutdown = ThreadShutdown;
        // PORT NOTE (N431): entered through the loop pointer its callbacks re-enter
        // through, never a `&mut Loop` (see `xev::Loop::run`).
        unsafe { xev::Loop::run_raw(&raw mut (*self_).loop_, xev::RunMode::UntilDone) }?;
        Ok(())
    }

    // fn setQosClass(self: *const Thread) void {
    fn setQosClass(self_: *mut Thread<A>) {
        // Thread QoS classes are only relevant on macOS.
        // if (comptime !builtin.target.os.tag.isDarwin()) return;
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            let class: internal_os::macos::QosClass = {
                // If we aren't visible (our view is fully occluded) then we
                // always drop our rendering priority down because it's just
                // mostly wasted work.
                //
                // The renderer itself should be doing this as well (for example
                // Metal will stop our DisplayLink) but this also helps with
                // general forced updates and CPU usage i.e. a rebuild cells call.
                // if (!self.flags.visible) break :class .utility;
                if !unsafe { (*self_).flags.visible } {
                    internal_os::macos::QosClass::utility
                }
                // If we're not focused, but we're visible, then we set a higher
                // than default priority because framerates still matter but it
                // isn't as important as when we're focused.
                // if (!self.flags.focused) break :class .user_initiated;
                else if !unsafe { (*self_).flags.focused } {
                    internal_os::macos::QosClass::user_initiated
                }
                // We are focused and visible, we are the definition of user interactive.
                // break :class .user_interactive;
                else {
                    internal_os::macos::QosClass::user_interactive
                }
            };

            // if (internal_os.macos.setQosClass(class)) { log.debug } else |err| { log.warn }
            match internal_os::macos::setQosClass(class) {
                // log.debug("thread QoS class set class={}", .{class});
                Ok(()) => log::debug!(target: LOG_SCOPE, "thread QoS class set class={class:?}"),
                // log.warn("error setting QoS class err={}", .{err});
                Err(err) => log::warn!(target: LOG_SCOPE, "error setting QoS class err={err:?}"),
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        {
            let _ = self_;
        }
    }

    /// Schedule the animation timer for the renderer's next animation
    /// wake, if it needs one.
    ///
    /// This is called after every frame update or animation draw and
    /// whenever the wake inputs change (focus, config, visibility
    /// regain). Resetting a pending timer is always safe: every call
    /// recomputes the wake, so the deadline only ever moves toward the
    /// actual next wake.
    //
    // fn armAnimationTimer(self: *Thread) void {
    //     const wake = self.renderer.animationWake() orelse return;
    //     self.animation_wake = wake.kind;
    //     self.render_h.reset(
    //         &self.loop,
    //         &self.render_c,
    //         &self.render_c_cancel,
    //         wake.delay_ms,
    //         Thread,
    //         self,
    //         animationTimerCallback,
    //     );
    // }
    // (`aee7bf347`: REPLACES `syncDrawTimer` — deleted with the draw timer.)
    fn armAnimationTimer(self_: *mut Thread<A>) {
        // const wake = self.renderer.animationWake() orelse return;
        let wake = match unsafe {
            rustty_renderer::Renderer::animation_wake_raw(
                &raw mut (*(*self_).renderer).0,
                &(*self_).draw_mutex,
            )
        } {
            Some(wake) => wake,
            None => return,
        };
        unsafe { (*self_).animation_wake = wake.kind };
        unsafe {
            (*self_).render_h.reset(
                &raw mut (*self_).loop_,
                &raw mut (*self_).render_c,
                &raw mut (*self_).render_c_cancel,
                wake.delay_ms,
                self_,
                Self::animationTimerCallback,
            );
        }
    }

    // /// Drain the mailbox.
    // fn drainMailbox(self: *Thread) !void {
    fn drainMailbox(self_: *mut Thread<A>) -> Result<(), RendererError> {
        // There's probably a more elegant way to do this...
        //
        // This is effectively an @autoreleasepool{} block, which we need in
        // order to ensure that autoreleased objects are properly released.
        // const pool = if (builtin.os.tag.isDarwin())
        //     @import("objc").AutoreleasePool.init()
        // else
        //     void;
        // defer if (builtin.os.tag.isDarwin()) pool.deinit();
        //
        // REALIZED (WS2-C, closes the W3 deferral): the pool is live via the
        // `rustty_os::autorelease::autoreleasepool` shim (objc2-backed on macOS,
        // identity elsewhere — the exact re-arm path the deferral named; core
        // still has NO objc2 edge). The drain body is hoisted to
        // `drainMailboxBody` so the pool scopes the whole drain; the closure
        // exit IS the reference `defer pool.deinit()`.
        rustty_os::autorelease::autoreleasepool(|| Self::drainMailboxBody(self_))
    }

    /// The reference `drainMailbox` body (see the pool wrapper above).
    fn drainMailboxBody(self_: *mut Thread<A>) -> Result<(), RendererError> {
        // while (self.mailbox.pop()) |message| {
        while let Some(message) = unsafe { (*(*self_).mailbox).pop() } {
            // log.debug("mailbox message={}", .{message});
            log::debug!(target: LOG_SCOPE, "mailbox message={}", MessageDbg(&message));
            // switch (message) {
            match message {
                // .crash => @panic("crash request, crashing intentionally"),
                Message::Crash => panic!("crash request, crashing intentionally"),

                // .visible => |v| visible: {
                Message::Visible(v) => 'visible: {
                    // If our state didn't change we do nothing.
                    // if (self.flags.visible == v) break :visible;
                    if unsafe { (*self_).flags.visible } == v {
                        break 'visible;
                    }

                    // Set our visible state
                    // self.flags.visible = v;
                    unsafe { (*self_).flags.visible = v };

                    // Visibility affects our QoS class
                    // self.setQosClass();
                    Self::setQosClass(self_);

                    // If we became visible then we immediately rebuild cells
                    // (renderCallback skips updateFrame while invisible) and
                    // draw. Going through renderCallback also reschedules
                    // any Kitty graphics animation wakeup that lapsed
                    // while we were invisible.
                    // if (v) _ = renderCallback(self, undefined, undefined, {});
                    if v {
                        let _ = Self::renderCallback(
                            self_,
                            core::ptr::null_mut(),
                            core::ptr::null_mut(),
                            Ok(()),
                        );
                    }

                    // Notify the renderer so it can update any state.
                    // self.renderer.setVisible(v);
                    unsafe {
                        rustty_renderer::Renderer::set_visible_raw(
                            &raw mut (*(*self_).renderer).0,
                            &(*self_).draw_mutex,
                            v,
                        )
                    };

                    // Note that we're explicitly today not stopping any
                    // cursor timers, draw timers, etc. These things have very
                    // little resource cost and properly maintaining their active
                    // state across different transitions is going to be bug-prone,
                    // so its easier to just let them keep firing and have them
                    // check the visible state themselves to control their behavior.
                }

                // .focus => |v| focus: {
                Message::Focus(v) => 'focus: {
                    // If our state didn't change we do nothing.
                    // if (self.flags.focused == v) break :focus;
                    if unsafe { (*self_).flags.focused } == v {
                        break 'focus;
                    }

                    // Set our state
                    // self.flags.focused = v;
                    unsafe { (*self_).flags.focused = v };

                    // Focus affects our QoS class
                    // self.setQosClass();
                    Self::setQosClass(self_);

                    // Set it on the renderer
                    // try self.renderer.setFocus(v);
                    unsafe {
                        rustty_renderer::Renderer::set_focus_raw(
                            &raw mut (*(*self_).renderer).0,
                            &(*self_).draw_mutex,
                            v,
                        )
                    }?;

                    // Focus gates custom shader animation, so re-arm
                    // the animation timer for the new state.
                    // self.armAnimationTimer();
                    Self::armAnimationTimer(self_);

                    // if (!v) {
                    if !v {
                        // If we're not focused, then we stop the cursor blink
                        // if (self.cursor_c.state() == .active and
                        //     self.cursor_c_cancel.state() == .dead)
                        if xev::Completion::state_of(unsafe { &(*self_).cursor_c })
                            == xev::CompletionState::Active
                            && xev::Completion::state_of(unsafe { &(*self_).cursor_c_cancel })
                                == xev::CompletionState::Dead
                        {
                            // self.cursor_h.cancel(&self.loop, &self.cursor_c,
                            //     &self.cursor_c_cancel, void, null, cursorCancelCallback);
                            unsafe {
                                (*self_).cursor_h.cancel(
                                    &raw mut (*self_).loop_,
                                    &raw mut (*self_).cursor_c,
                                    &raw mut (*self_).cursor_c_cancel,
                                    core::ptr::null_mut::<()>(),
                                    Self::cursorCancelCallback,
                                );
                            }
                        }
                    } else {
                        // If we're focused, we immediately show the cursor again
                        // and then restart the timer.
                        // if (self.cursor_c.state() != .active) {
                        if xev::Completion::state_of(unsafe { &(*self_).cursor_c })
                            != xev::CompletionState::Active
                        {
                            // self.flags.cursor_blink_visible = true;
                            unsafe { (*self_).flags.cursor_blink_visible = true };
                            // self.cursor_h.run(&self.loop, &self.cursor_c,
                            //     cursorBlinkInterval(), Thread, self, cursorTimerCallback);
                            unsafe {
                                (*self_).cursor_h.run(
                                    &raw mut (*self_).loop_,
                                    &raw mut (*self_).cursor_c,
                                    cursorBlinkInterval(),
                                    self_,
                                    Self::cursorTimerCallback,
                                );
                            }
                        }
                    }
                }

                // .reset_cursor_blink => {
                Message::ResetCursorBlink => {
                    // self.flags.cursor_blink_visible = true;
                    unsafe { (*self_).flags.cursor_blink_visible = true };
                    // if (self.cursor_c.state() == .active) {
                    if xev::Completion::state_of(unsafe { &(*self_).cursor_c })
                        == xev::CompletionState::Active
                    {
                        // self.cursor_h.reset(&self.loop, &self.cursor_c,
                        //     &self.cursor_c_cancel, cursorBlinkInterval(), Thread, self,
                        //     cursorTimerCallback);
                        unsafe {
                            (*self_).cursor_h.reset(
                                &raw mut (*self_).loop_,
                                &raw mut (*self_).cursor_c,
                                &raw mut (*self_).cursor_c_cancel,
                                cursorBlinkInterval(),
                                self_,
                                Self::cursorTimerCallback,
                            );
                        }
                    }
                }

                // .font_grid => |grid| {
                Message::FontGrid {
                    grid,
                    set,
                    new_key,
                    old_key,
                } => {
                    // self.renderer.setFontGrid(grid.grid);
                    unsafe {
                        rustty_renderer::Renderer::with_locked(
                            &raw mut (*(*self_).renderer).0,
                            &(*self_).draw_mutex,
                            |r| r.set_font_grid_locked(grid),
                        )
                    };
                    // grid.set.deref(grid.old_key);
                    unsafe { (*set).deref(old_key) };
                    // (PORT NOTE: `new_key` is an OWNED `Key` copy the reference discards
                    // here; drop it to free its descriptors and avoid a leak — the
                    // surface.rs `font_grid_key` owned-copy precedent.)
                    drop(new_key);
                }

                // .presentation_health => |v| self.renderer.setPresentationHealth(v),
                Message::PresentationHealth(v) => unsafe {
                    rustty_renderer::Renderer::with_locked(
                        &raw mut (*(*self_).renderer).0,
                        &(*self_).draw_mutex,
                        |r| r.set_presentation_health(v),
                    )
                },

                // .resize => |v| self.renderer.setScreenSize(v),
                Message::Resize(v) => unsafe {
                    rustty_renderer::Renderer::with_locked(
                        &raw mut (*(*self_).renderer).0,
                        &(*self_).draw_mutex,
                        |r| r.set_screen_size_locked(v),
                    )
                },

                // .change_config => |config| {
                Message::ChangeConfig { thread, r#impl } => {
                    // defer config.alloc.destroy(config.thread);
                    // defer config.alloc.destroy(config.impl);
                    // (PORT NOTE: both `alloc.destroy`s collapse into the `Box` drops at
                    // the end of this arm — the `Message::ChangeConfig` owns them.)
                    // try self.changeConfig(config.thread);
                    Self::changeConfig(self_, *thread);
                    // try self.renderer.changeConfig(config.impl);
                    unsafe {
                        rustty_renderer::Renderer::with_locked(
                            &raw mut (*(*self_).renderer).0,
                            &(*self_).draw_mutex,
                            |r| r.change_config_locked(*r#impl),
                        )
                    }?;

                    // The config affects what animation wakes the
                    // renderer needs (custom shaders, animation mode).
                    // self.armAnimationTimer();
                    Self::armAnimationTimer(self_);
                }

                // .search_viewport_matches => |v| {
                Message::SearchViewportMatches(v) => {
                    // Note we don't free the new value because we expect our
                    // allocators to match.
                    // if (self.renderer.search_matches) |*m| m.arena.deinit();
                    // (PORT NOTE: the `arena.deinit()` collapses into the drop of the old
                    // `Option`'s owned `Vec` when the field is reassigned below.)
                    // self.renderer.search_matches = v;
                    // self.renderer.search_matches_dirty = true;
                    // (N437: under the thread's lock handle; see `Thread::draw_mutex`.)
                    unsafe {
                        rustty_renderer::Renderer::with_locked(
                            &raw mut (*(*self_).renderer).0,
                            &(*self_).draw_mutex,
                            |r| {
                                r.search_matches = Some(v);
                                r.search_matches_dirty = true;
                            },
                        )
                    };
                }

                // .search_selected_match => |v| {
                Message::SearchSelectedMatch(v) => {
                    // if (self.renderer.search_selected_match) |*m| m.arena.deinit();
                    // (same arena collapse as above.)
                    // self.renderer.search_selected_match = v;
                    // self.renderer.search_matches_dirty = true;
                    unsafe {
                        rustty_renderer::Renderer::with_locked(
                            &raw mut (*(*self_).renderer).0,
                            &(*self_).draw_mutex,
                            |r| {
                                r.search_selected_match = v;
                                r.search_matches_dirty = true;
                            },
                        )
                    };
                }

                // .inspector => |v| self.flags.has_inspector = v,
                Message::Inspector(v) => unsafe { (*self_).flags.has_inspector = v },

                // .macos_display_id => |v| {
                //     if (@hasDecl(rendererpkg.Renderer, "setMacOSDisplayID")) {
                //         try self.renderer.setMacOSDisplayID(v, &self.draw_now);
                //     }
                // },  (@hasDecl TRUE for Metal)
                //
                // PORT NOTE (`a177ba90a`): the `&self.draw_now` argument is new —
                // a display change may now CREATE the display link ("when a
                // display becomes available it re-adds itself"), and creating it
                // needs the output-callback userdata this thread owns. `Async` is
                // a Copy+Send handle, so the R4 seam passes it by value where the
                // reference passes `*xev.Async`; the READ of the field happens
                // before the `&mut` call on the renderer, so the two raw-pointer
                // accesses never overlap.
                Message::MacosDisplayId(v) => {
                    // (Windows, task W-2: `.clone()` is the value copy; see
                    // `threadMain_`'s `loop_enter`.)
                    #[allow(clippy::clone_on_copy)]
                    let draw_now = unsafe { (*self_).draw_now.clone() };
                    unsafe {
                        rustty_renderer::Renderer::set_macos_display_id_raw(
                            &raw mut (*(*self_).renderer).0,
                            &(*self_).draw_mutex,
                            v,
                            draw_now,
                        )
                    }?;
                }
            }
        }

        Ok(())
    }

    // fn changeConfig(self: *Thread, config: *const DerivedConfig) !void {
    //
    // (PORT NOTE: the reference takes a `*const DerivedConfig` and copies `config.*`;
    // the port takes the value BY MOVE — the caller owns the `Box<DerivedConfig>`
    // and unboxes it. The `!void` is infallible.)
    fn changeConfig(self_: *mut Thread<A>, config: thread::DerivedConfig) {
        // A newly enabled scheduler must reconsider existing history even when no
        // terminal activity occurred while compression was disabled.
        // if (comptime terminalpkg.compression_enabled) {
        //     if (!self.config.scrollback_compression and
        //         config.scrollback_compression)
        //     {
        //         self.compression.activity = null;
        //     }
        // }
        if rustty_vt::compression_enabled()
            && !unsafe { (*self_).config.scrollback_compression }
            && config.scrollback_compression
        {
            unsafe { (*self_).compression.activity = None };
        }

        // self.config = config.*;
        unsafe { (*self_).config = config };
    }

    /// Trigger a draw. This will not update frame data or anything, it will
    /// just trigger a draw/paint.
    // fn drawFrame(self: *Thread, now: bool) void {
    fn drawFrame(self_: *mut Thread<A>, now: bool) {
        // If we're invisible, we do not draw.
        // if (!self.flags.visible) return;
        if !unsafe { (*self_).flags.visible } {
            return;
        }

        // If the renderer is managing a vsync on its own, we only draw
        // when we're forced to via `now`.
        // if (!now and self.renderer.hasVsync()) return;
        if !now
            && unsafe {
                rustty_renderer::Renderer::has_vsync_raw(
                    &raw mut (*(*self_).renderer).0,
                    &(*self_).draw_mutex,
                )
            }
        {
            return;
        }

        // self.renderer.drawFrame(false) catch |err|
        //     log.warn("error drawing err={}", .{err});
        //
        // (`2f0b65346`: the `apprt.must_draw_from_app_thread` arm, which pushed
        // `.redraw_surface` to the app thread instead, is deleted.)
        //
        // W5 (`W5_DESIGN §2.5` caller 1): RAW-self draw — the render thread must
        // NOT establish a protected whole-`Renderer` `&mut` (it would race the
        // main thread's `draw_frame_raw` under `draw_mutex`; the mutex cannot
        // rescue a protector established before the lock). `&raw mut
        // (*(*self_).renderer).0` is a raw place projection off the already-raw
        // `renderer: *mut Renderer` — no reborrow, no protector. N437: locked
        // through the thread's own handle.
        let rp: *mut rustty_renderer::Renderer = unsafe { &raw mut (*(*self_).renderer).0 };
        if let Err(err) =
            unsafe { rustty_renderer::Renderer::draw_frame_raw(rp, &(*self_).draw_mutex, false) }
        {
            // log.warn("error drawing err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error drawing err={err:?}");
        }
    }

    // fn wakeupCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Async.WaitError!void) xev.CallbackAction {
    fn wakeupCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::WaitError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| { log.err("error in wakeup err={}", .{err}); return .rearm; };
        if let Err(err) = r {
            // log.err("error in wakeup err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in wakeup err={err:?}");
            return xev::CallbackAction::Rearm;
        }

        // const t = self_.?;
        //
        // PORT NOTE (R7, NEW-FINDINGS N13): Zig's `.?` is a checked unwrap
        // (a safe build panics "attempt to use null value"); `rustty-xev` hands
        // userdata over as a raw pointer that is null for `()` userdata, so the
        // check is kept as an assertion instead of an unchecked dereference.
        assert!(!self_.is_null(), "attempt to use null value");

        // When we wake up, we check the mailbox. Mailbox producers should
        // wake up our thread after publishing.
        // t.drainMailbox() catch |err| log.err("error draining mailbox err={}", .{err});
        if let Err(err) = Self::drainMailbox(self_) {
            // log.err("error draining mailbox err={}", .{err});
            log::error!(target: LOG_SCOPE, "error draining mailbox err={err:?}");
        }

        // Render immediately
        // _ = renderCallback(t, undefined, undefined, {});
        let _ = Self::renderCallback(self_, core::ptr::null_mut(), core::ptr::null_mut(), Ok(()));

        // PageList mutations maintain their own compression dirty state. Checking
        // it here covers output, resize, and viewport scrolling uniformly.
        // t.compression.wake(t);
        //
        // (PORT NOTE: the reference's `self`/`thread` pair — `self` IS
        // `&thread.compression` — is two raw pointers into the SAME object here;
        // see [`Compression`]'s aliasing note.)
        Compression::wake(unsafe { &raw mut (*self_).compression }, self_);

        // The below is not used anymore but if we ever want to introduce a
        // configuration to introduce a delay to coalesce renders, we can use this.
        //
        // // If the timer is already active then we don't have to do anything.
        // if (t.render_c.state() == .active) return .rearm;
        //
        // // Timer is not active, let's start it
        // t.render_h.run(&t.loop, &t.render_c, 10, Thread, t, renderCallback);

        // return .rearm;
        xev::CallbackAction::Rearm
    }

    // fn drawNowCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Async.WaitError!void) xev.CallbackAction {
    fn drawNowCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::WaitError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| { log.err("error in draw now err={}", .{err}); return .rearm; };
        if let Err(err) = r {
            // log.err("error in draw now err={}", .{err});
            log::error!(target: LOG_SCOPE, "error in draw now err={err:?}");
            return xev::CallbackAction::Rearm;
        }

        // Draw immediately
        // const t = self_.?;
        // R7, NEW-FINDINGS N13: the checked unwrap, as above.
        assert!(!self_.is_null(), "attempt to use null value");
        // t.drawFrame(true);
        Self::drawFrame(self_, true);

        // return .rearm;
        xev::CallbackAction::Rearm
    }

    // PORT NOTE (`aee7bf347`): `drawCallback` is DELETED with the draw timer;
    // `animationTimerCallback` below is the animation timer's callback.

    // fn renderCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Timer.RunError!void) xev.CallbackAction {
    fn renderCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| switch (err) {
        //     // Sent when a scheduled animation wakeup is superseded by a
        //     // newer one (Timer.reset cancels the pending run). Nothing to
        //     // do; the replacement timer carries on.
        //     error.Canceled => return .disarm,
        //     else => unreachable,
        // };
        //
        // PORT NOTE: reference defect (R13), inventory M16 (with
        // `cursorCancelCallback`, LEDGER S114-F2). The `else => unreachable`
        // is reachable on Linux io_uring, where a timer SQE can complete with
        // an errno libxev maps to `error.Unexpected` (the kernel path is not
        // verified; see S114-F2). The reference has no diagnostic here, so the
        // port adds none: any error takes the `Canceled` arm (disarm — the
        // next wakeup re-arms the render timer). On kqueue the arm is dead
        // (timers never reach a syscall).
        if r.is_err() {
            return xev::CallbackAction::Disarm;
        }
        // const t: *Thread = self_ orelse {
        //     // This shouldn't happen so we log it.
        //     log.warn("render callback fired without data set", .{});
        //     return .disarm;
        // };
        //
        // PORT NOTE (task 14d-1): this arm was elided as "non-null by
        // construction". It is not structurally absent: `rustty-xev` hands a
        // callback its userdata as a RAW `*mut Userdata`
        // (`common::userdata_value`), which is exactly as nullable as the
        // reference's `?*Thread` — so the reference's guard ports directly,
        // and eliding it turned a disarm-and-warn into a null dereference.
        if self_.is_null() {
            // This shouldn't happen so we log it.
            log::warn!(target: LOG_SCOPE, "render callback fired without data set");
            return xev::CallbackAction::Disarm;
        }

        // If the display is now unrealized, release GPU resources now
        // we're on the render thread, and do not try to update and draw
        // this frame.
        // if (!t.renderer.display_realized) {
        //     t.renderer.draw_mutex.lockUncancelable(global.io());
        //     defer t.renderer.draw_mutex.unlock(global.io());
        //
        //     t.renderer.releaseGpuResources();
        //     return .disarm;
        // }
        //
        // (N437: `display_realized` is read under the lock; see
        // `rustty_renderer::Renderer::release_if_unrealized_raw`.)
        if unsafe {
            rustty_renderer::Renderer::release_if_unrealized_raw(
                &raw mut (*(*self_).renderer).0,
                &(*self_).draw_mutex,
            )
        } {
            return xev::CallbackAction::Disarm;
        }

        // If we're not visible there's no point spending CPU rebuilding cells —
        // we'll catch up when the .visible mailbox message flips us back on.
        // Kitty graphics animations pause with us and resume on visibility.
        // if (!t.flags.visible) return .disarm;
        if !unsafe { (*self_).flags.visible } {
            return xev::CallbackAction::Disarm;
        }

        // Update our frame data
        // t.renderer.updateFrame(t.state, t.flags.cursor_blink_visible)
        //     catch |err| log.warn("error rendering err={}", .{err});
        //
        // (PORT NOTE: `update_frame` takes a SHARED `&State`; the error is
        // `VtOutOfMemory`, logged not propagated.)
        // (N437 / ruling 32: the update takes the thread's lock handle around
        // the Kitty image update and the GPU-cell rebuild only, as the
        // reference does; see `rustty_renderer::Renderer::update_frame_raw`.)
        if let Err(err) = unsafe {
            rustty_renderer::Renderer::update_frame_raw(
                &raw mut (*(*self_).renderer).0,
                &(*self_).draw_mutex,
                &*(*self_).state,
                (*self_).flags.cursor_blink_visible,
            )
        } {
            // log.warn("error rendering err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error rendering err={err:?}");
        }

        // Draw
        // t.drawFrame(false);
        Self::drawFrame(self_, false);

        // Schedule the next animation wake, if the renderer needs one.
        // t.armAnimationTimer();
        Self::armAnimationTimer(self_);

        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn animationTimerCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Timer.RunError!void) xev.CallbackAction {
    fn animationTimerCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| switch (err) {
        //     // Sent when a scheduled animation wake is superseded by a
        //     // newer one (Timer.reset cancels the pending run). Nothing to
        //     // do; the replacement timer carries on.
        //     error.Canceled => return .disarm,
        //     else => unreachable,
        // };
        //
        // CORRECTED (task 14d-1): the `else` arm carried a
        // `// log.warn("animation callback fired without data set")`
        // comment. That is a MIS-TRANSCRIPTION — the reference's `else` arm
        // here is a bare `unreachable` with no diagnostic (`Thread.zig:621`);
        // the "fired without data set" warning belongs to the `self_ orelse`
        // arm below (`Thread.zig:623-627`), where it is now restored.
        //
        // PORT NOTE: reference defect (R13), inventory M16 — as in
        // `renderCallback`: any error disarms, with no diagnostic because the
        // reference has none.
        if r.is_err() {
            return xev::CallbackAction::Disarm;
        }
        // const t: *Thread = self_ orelse {
        //     // This shouldn't happen so we log it.
        //     log.warn("animation callback fired without data set", .{});
        //     return .disarm;
        // };
        // (same nullable-raw-userdata reasoning as `renderCallback` above.)
        if self_.is_null() {
            // This shouldn't happen so we log it.
            log::warn!(target: LOG_SCOPE, "animation callback fired without data set");
            return xev::CallbackAction::Disarm;
        }

        // Animations pause entirely while we're invisible; the .visible
        // mailbox message re-arms us when we can be seen again.
        // if (!t.flags.visible) return .disarm;
        if !unsafe { (*self_).flags.visible } {
            return xev::CallbackAction::Disarm;
        }

        // switch (t.animation_wake) {
        match unsafe { (*self_).animation_wake } {
            // Frame data must be updated (a Kitty animation frame is
            // due). renderCallback updates, draws, and re-arms us.
            // .update => return renderCallback(t, undefined, undefined, {}),
            rustty_renderer::generic::AnimationWakeKind::Update => {
                Self::renderCallback(self_, core::ptr::null_mut(), core::ptr::null_mut(), Ok(()))
            }

            // A redraw alone suffices (custom shader time uniform).
            // Draw calls don't update from the terminal state so they
            // are much cheaper than a frame update.
            // .draw => { t.drawFrame(false); t.armAnimationTimer(); return .disarm; },
            rustty_renderer::generic::AnimationWakeKind::Draw => {
                Self::drawFrame(self_, false);
                Self::armAnimationTimer(self_);
                xev::CallbackAction::Disarm
            }
        }
    }

    // fn cursorTimerCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Timer.RunError!void) xev.CallbackAction {
    fn cursorTimerCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // _ = r catch |err| switch (err) {
        //     error.Canceled => return .disarm,
        //     else => { log.warn(...); unreachable; },
        // };
        if let Err(err) = r {
            match err {
                // This is sent when our timer is canceled. That's fine.
                xev::TimerRunError::Canceled => return xev::CallbackAction::Disarm,
                // PORT NOTE: reference defect (R13), inventory M16 — as in
                // `renderCallback`, but this arm has the reference's own
                // warning, which is kept; then disarm instead of
                // `unreachable`. The blink then stops until the next focus
                // GAIN: `reset_cursor_blink` re-runs the timer only while
                // `cursor_c` is active (`Thread.zig:390`, the
                // `ResetCursorBlink` arm above), so a keypress only makes the
                // cursor visible, and the `.focus = true` arm
                // (`Thread.zig:371-384`) is what runs a dead timer again.
                // While unfocused a stopped timer is the reference's own
                // intended state (UD73, corrected by task R13-C2).
                _ => {
                    // log.warn("error in cursor timer callback err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "error in cursor timer callback err={err:?}"
                    );
                    return xev::CallbackAction::Disarm;
                }
            }
        }

        // const t: *Thread = self_ orelse {
        //     // This shouldn't happen so we log it.
        //     log.warn("render callback fired without data set", .{});
        //     return .disarm;
        // };
        // (same nullable-raw-userdata reasoning as `renderCallback` above. The
        // message text really is "render callback" here — `Thread.zig:672`
        // copy-pastes it into `cursorTimerCallback`; kept verbatim, typo and
        // all, per R1.)
        if self_.is_null() {
            // This shouldn't happen so we log it.
            log::warn!(target: LOG_SCOPE, "render callback fired without data set");
            return xev::CallbackAction::Disarm;
        }

        // t.flags.cursor_blink_visible = !t.flags.cursor_blink_visible;
        unsafe { (*self_).flags.cursor_blink_visible = !(*self_).flags.cursor_blink_visible };
        // t.wakeup.notify() catch {};
        let _ = unsafe { (*self_).wakeup.notify() };

        // t.cursor_h.run(&t.loop, &t.cursor_c, cursorBlinkInterval(), Thread, t, cursorTimerCallback);
        unsafe {
            (*self_).cursor_h.run(
                &raw mut (*self_).loop_,
                &raw mut (*self_).cursor_c,
                cursorBlinkInterval(),
                self_,
                Self::cursorTimerCallback,
            );
        }
        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn cursorCancelCallback(_: ?*void, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Timer.CancelError!void) xev.CallbackAction {
    //
    // (PORT NOTE: the userdata is `?*void` — a `*mut ()` here.)
    fn cursorCancelCallback(
        _ud: *mut (),
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::CancelError>,
    ) -> xev::CallbackAction {
        // This makes it easier to work across platforms where different platforms
        // support different sets of errors, so we just unify it.
        // const CancelError = xev.Timer.CancelError || error{ Canceled, NotFound, Unexpected };
        // _ = r catch |err| switch (@as(CancelError, @errorCast(err))) {
        //     error.Canceled => {}, // success
        //     error.NotFound => {}, // completed before it could cancel
        //     else => { log.warn(...); unreachable; },
        // };
        //
        // CORRECTED (task 14d-1, closes S114-F2): the note that stood here
        // ("Rustty's `CancelError` set is `error{Canceled}` ... the
        // NotFound/unreachable arms have no counterpart yet") was true of the
        // macOS/kqueue set ONLY, and the code it justified — `if let
        // Err(_err) = r {}` — swallowed EVERY cancel error on every target.
        // `rustty-xev`'s `CancelError` is per-target, exactly as the
        // reference's `xev.Timer.CancelError` is:
        //
        //   * kqueue (macOS/iOS/visionOS/FreeBSD): `error{Canceled}`. Its one
        //     variant IS the reference's success arm, so the `NotFound` and
        //     `else` arms are UNINHABITED there — nothing was being swallowed
        //     on that target, and there is no failure arm to log from.
        //   * dynamic (Linux): `error{NotFound, ExpirationInProgress,
        //     Unexpected}`. `NotFound` is the reference's second success arm
        //     ("completed before it could cancel"); the other two fell into
        //     the reference's `else`, which WARNS AND THEN HITS
        //     `unreachable` — i.e. the reference treats them as impossible
        //     and crashes a safe build. The port continued silently.
        //
        // Faithful means the reference's own control flow, not a log line
        // added to a swallow: warn, then `unreachable!()` — Rust's
        // `unreachable!()` panics the way Zig's `unreachable` does in a safe
        // build, and in a release build both are UB-on-reach by contract.
        // The diagnostic is kept because the reference emits it BEFORE
        // trapping, so the operator sees which error caused the crash.
        //
        // PORT NOTE: reference defect (R13), UD73 (task R13-C reopened and
        // re-closed S114-F2). The paragraph above is superseded: these errors
        // are not impossible. On io_uring the timer cancel is a
        // `TIMEOUT_REMOVE`, and a remove that races the timer firing completes
        // with `-EALREADY` (the kernel's `io_timeout_extract` when
        // `hrtimer_try_to_cancel` fails), which libxev's `timer_remove`
        // mapping (NOENT, BUSY, TIME) does not name, so it arrives here as
        // `Unexpected`; `-EBUSY` arrives as `ExpirationInProgress`. Both hit
        // the reference's `unreachable`.
        // The port keeps the reference's warning and then disarms, which is
        // what the success arms do: the timer has fired or is firing, so
        // there is nothing left to cancel.
        //
        // Trigger (corrected by task R13-C2): this callback is passed only by
        // the `.focus = false` arm (`Thread.zig:357-370`), so the race is
        // losing focus while the blink timer fires. `reset_cursor_blink`'s
        // `Timer.reset` queues its own `TIMEOUT_REMOVE` with the loop's no-op
        // callback (libxev `io_uring.zig:289-324`) and never reaches here.
        // After the failed cancel the timer's own completion arrives as an
        // expiration and `cursorTimerCallback` re-runs it, so the blink timer
        // keeps ticking while unfocused. That is invisible (an unfocused
        // cursor is a hollow block whatever the blink state,
        // `cursor.zig:58-60`) and the next focus loss cancels it again.
        if let Err(err) = r {
            #[cfg(not(any(target_os = "linux", windows)))]
            match err {
                // error.Canceled => {}, // success
                xev::CancelError::Canceled => {}
            }
            // Windows (task W-2, N342): libxev's IOCP `CancelError` is
            // `error{Unexpected}` (`backend/iocp.zig:1372-1374`), which the
            // reference's unified set sends to its `else` arm. IOCP's timer
            // cancel never fills a cancel error (`stop_completion`'s `.timer` arm,
            // `iocp.zig:801-826`), so the arm is not reached there; it takes the
            // same handling as the Linux arm below (UD73) rather than a second
            // policy for one reference `else`.
            #[cfg(windows)]
            match err {
                // else => { log.warn(...); unreachable; },
                xev::CancelError::Unexpected => {
                    // log.warn("error in cursor cancel callback err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "error in cursor cancel callback err={err:?}"
                    );
                }
            }
            #[cfg(target_os = "linux")]
            match err {
                // error.NotFound => {}, // completed before it could cancel
                xev::CancelError::NotFound => {}
                // else => { log.warn(...); unreachable; },
                //
                // (R13, UD73: warn, then fall through to `.disarm`.)
                _ => {
                    // log.warn("error in cursor cancel callback err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "error in cursor cancel callback err={err:?}"
                    );
                }
            }
        }

        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // The reference keeps a commented-out libuv-era callback here
    // (`Thread.zig:725-729`), preserved verbatim (R1) — it is dead in the
    // reference too, so there is nothing to port beyond the text:
    //
    // fn prepFrameCallback(h: *libuv.Prepare) void {
    //     _ = h;
    //
    //     tracy.frameMark();
    // }

    // fn stopCallback(self_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     r: xev.Async.WaitError!void) xev.CallbackAction {
    fn stopCallback(
        self_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        r: Result<(), xev::WaitError>,
    ) -> xev::CallbackAction {
        // _ = r catch unreachable;
        //
        // PORT NOTE: reference defect (R13), UD75 (task R13-C2). The wait
        // error is not impossible: on Linux the stop `Async` is an eventfd
        // `poll`, and the epoll backend (Ghostty's `xev.Dynamic` falls back to
        // it when io_uring is unavailable) starts it with
        // `epoll_ctl(EPOLL_CTL_ADD)`. If the user's per-user
        // `fs.epoll.max_user_watches` is already used up at thread start, the
        // loop's own eventfd registration fails first and `run` errors out, so
        // this callback is not reached; it is reached when the limit is
        // crossed between that registration and this one (another of the
        // user's processes can use it up), or on a transient `ENOMEM`. That
        // arrives here as an error, and the reference traps. Disarming would
        // leave the thread with no stop watcher, so `deinit`'s join would hang
        // forever; stopping the loop would stop the renderer on an error that
        // is not a stop request. The port re-arms instead, which is what the
        // same file's `wakeupCallback` does for the same `Async.WaitError`
        // (without its message, which names the wakeup; the reference has
        // none here). The eventfd (a mach port on macOS) keeps the
        // notification, so a stop notified while the wait cannot register is
        // delivered once it does. The retry happens once per tick, paced by
        // the loop's other work: the wakeup wait or the cursor timer is always
        // active here, so it does not spin.
        if r.is_err() {
            return xev::CallbackAction::Rearm;
        }
        // self_.?.loop.stop();
        // R7, NEW-FINDINGS N13: the checked unwrap, as above.
        assert!(!self_.is_null(), "attempt to use null value");
        unsafe { (*self_).loop_.stop() };
        // return .disarm;
        xev::CallbackAction::Disarm
    }
}

// Returns the interval for the blinking cursor in milliseconds.
// fn cursorBlinkInterval() u64 {
fn cursorBlinkInterval() -> u64 {
    // CORRECTION (parity S114-F1): this branch used to be a comment, "inert"
    // for want of a valgrind probe; `rustty_zigstd::valgrind` ports
    // `std.valgrind.runningOnValgrind`, which, like the reference's, is live
    // only where `builtin.valgrind_support` is (debug builds, not macOS).
    cursor_blink_interval_for(rustty_zigstd::valgrind::running_on_valgrind())
}

/// The body of `cursorBlinkInterval` given `std.valgrind.runningOnValgrind()`.
fn cursor_blink_interval_for(running_on_valgrind: usize) -> u64 {
    // if (std.valgrind.runningOnValgrind() > 0) {
    if running_on_valgrind > 0 {
        // If we're running under Valgrind, the cursor blink adds enough
        // churn that it makes some stalls annoying unless you're on a
        // super powerful computer, so we delay it.
        //
        // This is a hack, we should change some of our cursor timer
        // logic to be more efficient:
        // https://github.com/ghostty-org/ghostty/issues/8003
        // return CURSOR_BLINK_INTERVAL * 5;
        return CURSOR_BLINK_INTERVAL * 5;
    }

    // return CURSOR_BLINK_INTERVAL;
    CURSOR_BLINK_INTERVAL
}

/// Schedules incremental terminal compression after renderer activity stops.
///
/// This owns all renderer-specific compression state. The terminal decides
/// when compression-relevant activity changes and performs the actual work;
/// the renderer only provides idle scheduling and avoids waiting for the
/// terminal lock.
//
// PORTS: oracle@main renderer/Thread.zig `const Compression = struct` (WS7 D3).
// The `scrollback-compression` gating is oracle 0fb89f4ff; the contended-wake
// `else if` branch is oracle 25e624569.
//
// PORT NOTE (visibility): the reference declaration is file-private (`const
// Compression`, not `pub const`); `pub(crate)` is the closest Rust analogue that
// still lets the `pub struct Thread<A>` field name it.
//
// PORT NOTE (ALIASING, memory `rust-mut-args-strongly-protected`): every
// reference method takes `self: *Compression` AND `thread: *Thread`, where `self`
// IS `&thread.compression` — two live pointers into the same object, one nested in
// the other. In Rust that is only sound with RAW pointers, so the methods below
// are associated fns taking `self_: *mut Compression` + `thread: *mut Thread<A>`
// and projecting narrowly (`(*self_).activity`, `(*thread).config.…`); no `&mut
// Compression`/`&mut Thread` is ever minted, and the xev completions — which the
// loop re-enters across ticks — stay raw (`&raw mut (*self_).completion`).
// `deinit` is the one exception: it is called from `Thread::deinit(&mut self)`
// post-join, where nothing else aliases.
//
// PORT NOTE (the activity token): `compression_activity()` is an OPAQUE change
// token, not a sequence — its doc says "the direction of the change has no
// meaning". Only `==`/`!=` is used here, exactly as the reference does; never `<`
// or `>`.
pub(crate) struct Compression {
    // timer: xev.Timer,
    timer: xev::Timer,
    // completion: xev.Completion = .{},
    completion: xev::Completion,
    // reset_completion: xev.Completion = .{},
    reset_completion: xev::Completion,
    /// The last activity token we acted on. `None` (the reference `null`) forces
    /// the next wake to schedule a pass.
    // activity: ?u64 = null,
    activity: Option<u64>,
}

impl Compression {
    // const idle_interval = 250;
    const IDLE_INTERVAL: u64 = 250;
    // const step_interval = 1;
    const STEP_INTERVAL: u64 = 1;

    // fn init() !Compression {
    fn init() -> Result<Compression, xev::Error> {
        // return .{ .timer = try xev.Timer.init() };
        Ok(Compression {
            timer: xev::Timer::init()?,
            // (reference field defaults)
            completion: xev::Completion::default(),
            reset_completion: xev::Completion::default(),
            activity: None,
        })
    }

    // fn deinit(self: *Compression) void {
    fn deinit(&mut self) {
        // self.timer.deinit();
        self.timer.deinit();
    }

    /// Start or postpone compression after a renderer wake.
    // fn wake(self: *Compression, thread: *Thread) void {
    fn wake<A: apprt::App + 'static>(self_: *mut Compression, thread: *mut Thread<A>) {
        // If we have no compression then don't do anything.
        // if (comptime !terminalpkg.compression_enabled) return;
        if !rustty_vt::compression_enabled() {
            return;
        }
        // if (!thread.config.scrollback_compression) return;
        if !unsafe { (*thread).config.scrollback_compression } {
            return;
        }

        // PageList activity, rather than a generic renderer wake, restarts the
        // idle interval. In particular, the inspector wakes the renderer every
        // frame without changing terminal contents and must not starve this
        // timer indefinitely.
        //
        // (PORT NOTE: `state`/`mutex`/`terminal` are raw pointers in the port —
        // re-deref'd fresh at every use, never cached as a reference across the
        // interleaving `&mut Terminal` below, the generic.rs `updateFrame`
        // precedent.)
        let state: *mut State = unsafe { (*thread).state };
        // if (thread.state.mutex.tryLock()) {
        if unsafe { (*(*state).mutex).try_lock() } {
            // defer thread.state.mutex.unlock();
            //
            // (The `defer` runs AFTER the comparison and the early return, so the
            // unlock is hoisted to cover both exits below.)

            // const activity = thread.state.terminal.compressionActivity();
            let activity = unsafe { (*(*state).terminal).compression_activity() };
            // if (self.activity == activity) return;
            let same = unsafe { (*self_).activity } == Some(activity);
            // self.activity = activity;
            if !same {
                unsafe { (*self_).activity = Some(activity) };
            }
            unsafe { (*(*state).mutex).unlock() }; // `defer`
            if same {
                return;
            }
        }
        // } else if (self.completion.state() == .active) {
        else if xev::Completion::state_of(unsafe { &(*self_).completion })
            == xev::CompletionState::Active
        {
            // Contention doesn't prove that compression-relevant activity
            // changed. Keep an existing deadline so frequent inspector frames
            // cannot postpone compression forever. The timer rechecks both the
            // activity token and lock availability before doing any work.
            return;
        }

        // Contention may mean parsing is active. Scheduling is a harmless
        // false positive when no compression work is actually pending, but is
        // necessary when no timer is already active.
        // self.schedule(thread, idle_interval);
        Self::schedule(self_, thread, Self::IDLE_INTERVAL);
    }

    /// Start the one-shot timer, or move its deadline if it is already active.
    // fn schedule(self: *Compression, thread: *Thread, delay_ms: u64) void {
    fn schedule<A: apprt::App + 'static>(
        self_: *mut Compression,
        thread: *mut Thread<A>,
        delay_ms: u64,
    ) {
        // self.timer.reset(
        //     &thread.loop,
        //     &self.completion,
        //     &self.reset_completion,
        //     delay_ms,
        //     Thread,
        //     thread,
        //     timerCallback,
        // );
        unsafe {
            (*self_).timer.reset(
                &raw mut (*thread).loop_,
                &raw mut (*self_).completion,
                &raw mut (*self_).reset_completion,
                delay_ms,
                thread,
                Self::timerCallback::<A>,
            );
        }
    }

    // fn timerCallback(thread_: ?*Thread, _: *xev.Loop, _: *xev.Completion,
    //     result: xev.Timer.RunError!void) xev.CallbackAction {
    fn timerCallback<A: apprt::App + 'static>(
        thread_: *mut Thread<A>,
        _l: *mut xev::Loop,
        _c: *mut xev::Completion,
        result: Result<(), xev::TimerRunError>,
    ) -> xev::CallbackAction {
        // _ = result catch |err| switch (err) {
        //     error.Canceled => return .disarm,
        //     else => {
        //         log.warn("error in compression timer err={}", .{err});
        //         return .disarm;
        //     },
        // };
        //
        // (Both arms `.disarm`; the non-`Canceled` arm differs only by the log.)
        if let Err(err) = result {
            if err != xev::TimerRunError::Canceled {
                // log.warn("error in compression timer err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error in compression timer err={err:?}");
            }
            return xev::CallbackAction::Disarm;
        }

        // const thread = thread_ orelse return .disarm;
        if thread_.is_null() {
            return xev::CallbackAction::Disarm;
        }
        let thread = thread_;
        // const self = &thread.compression;
        let self_: *mut Compression = unsafe { &raw mut (*thread).compression };

        // if (self.step(thread)) |delay| self.schedule(thread, delay);
        if let Some(delay) = Self::step(self_, thread) {
            Self::schedule(self_, thread, delay);
        }
        // return .disarm;
        xev::CallbackAction::Disarm
    }

    /// Try one bounded step without waiting for the terminal lock. The return
    /// value is the delay before another attempt, or null when work is done.
    // fn step(self: *Compression, thread: *Thread) ?u64 {
    fn step<A: apprt::App + 'static>(
        self_: *mut Compression,
        thread: *mut Thread<A>,
    ) -> Option<u64> {
        // if (!thread.config.scrollback_compression) return null;
        if !unsafe { (*thread).config.scrollback_compression } {
            return None;
        }

        // const state = thread.state;
        let state: *mut State = unsafe { (*thread).state };
        // if (!state.mutex.tryLock()) return idle_interval;
        if !unsafe { (*(*state).mutex).try_lock() } {
            return Some(Self::IDLE_INTERVAL);
        }
        // defer state.mutex.unlock();
        //
        // (Hand-rolled: the guard-less `rustty_zigstd::thread::Mutex` has no RAII
        // drop, and the lock is held across the two returns below. Neither arm can
        // panic before its unlock — `compression_activity`/`compress` are the only
        // calls and both are infallible.)

        // const activity = state.terminal.compressionActivity();
        let activity = unsafe { (*(*state).terminal).compression_activity() };
        // if (self.activity != activity) {
        if unsafe { (*self_).activity } != Some(activity) {
            // self.activity = activity;
            unsafe { (*self_).activity = Some(activity) };
            // `defer state.mutex.unlock()`
            unsafe { (*(*state).mutex).unlock() };
            // return idle_interval;
            return Some(Self::IDLE_INTERVAL);
        }

        // return switch (state.terminal.compress(.incremental)) {
        //     .pending => step_interval,
        //     .unsupported,
        //     .complete,
        //     => null,
        // };
        let result = unsafe {
            (*(*state).terminal).compress(rustty_vt::terminal::TerminalCompressionMode::Incremental)
        };
        unsafe { (*(*state).mutex).unlock() }; // `defer`
        match result {
            rustty_vt::terminal::TerminalCompressionResult::Pending => Some(Self::STEP_INTERVAL),
            rustty_vt::terminal::TerminalCompressionResult::Unsupported
            | rustty_vt::terminal::TerminalCompressionResult::Complete => None,
        }
    }
}

// ===========================================================================
// WS7 D3 — the renderer-thread idle compression scheduler, END TO END.
// ===========================================================================
//
// These are rustty-only tests (the reference file has no `test{}` blocks), and
// they are the FIRST place in the shipped app where `PageList::compress` actually
// runs: before D3 the only callers were rustty-bench, vt-internal search, and vt
// unit tests. Each test drives the REAL `Thread<A>` — a real `xev.Loop`, the real
// `Compression` timer chain, a real `Terminal` with real scrollback and the real
// guard-less render mutex — through the same entry point `wakeupCallback` uses,
// then runs the loop `.until_done` (the compression timer is the only registered
// completion, so the loop returns exactly when the idle pass finishes).
#[cfg(test)]
mod compression_tests {
    use super::*;
    use rustty_vt::terminal::{Options as TerminalOptions, Terminal};

    /// The fake apprt runtime. Only `App::Surface` matters here: no method on it
    /// is reachable from the compression paths.
    //
    // `pub(super)` so the sibling `surface_mailbox_seam_tests` module can reuse the
    // fake runtime instead of duplicating ~100 lines of `unimplemented!()` stubs.
    pub(super) struct TestApp;

    /// The fake runtime surface. `Core = crate::surface::Surface<TestApp>` is what
    /// satisfies the `changeConfig` impl block's bound.
    pub(super) struct TestSurface;

    impl apprt::App for TestApp {
        type Surface = TestSurface;

        fn perform_action<C: apprt::CoreSurfaceHandle>(
            &self,
            _target: apprt::Target<'_, C>,
            _action: apprt::Action<'_>,
        ) -> Result<bool, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn perform_ipc(
            &self,
            _target: apprt::ipc::Target<'_>,
            _action: apprt::ipc::Action<'_>,
        ) -> Result<bool, apprt::ipc::Errors> {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn wakeup(&self) {}

        fn config(&self) -> &rustty_config::Config {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn keyboard_layout(&self) -> rustty_input::keyboard::Layout {
            rustty_input::keyboard::Layout::Unknown
        }
    }

    impl apprt::Surface for TestSurface {
        type App = TestApp;
        type Core = crate::surface::Surface<TestApp>;

        fn core(&self) -> *mut crate::surface::Surface<TestApp> {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn rt_app(&self) -> *mut TestApp {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn close(&self, _process_alive: bool) {}

        fn deinit(&mut self) {}

        fn get_content_scale(&self) -> Result<apprt::ContentScale, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn get_size(&self) -> Result<apprt::SurfaceSize, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn get_title(&self) -> Option<&std::ffi::CStr> {
            None
        }

        fn supports_clipboard(&self, _clipboard_type: apprt::Clipboard) -> bool {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn clipboard_request(
            &self,
            _clipboard_type: apprt::Clipboard,
            _state: apprt::ClipboardRequest,
        ) -> Result<apprt::ClipboardReadResult, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn set_clipboard(
            &self,
            _clipboard_type: apprt::Clipboard,
            _contents: &[apprt::ClipboardContent<'_>],
            _confirm: bool,
        ) -> Result<(), apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn get_cursor_pos(&self) -> Result<apprt::CursorPos, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the compression tests")
        }

        fn default_termio_env(&self) -> Result<rustty_global::EnvMap, apprt::ApprtError> {
            unimplemented!("test runtime: not driven by the compression tests")
        }
    }

    /// A heap-pinned `Thread<TestApp>` over a real render `State`/`Terminal`.
    /// `surface`/`renderer` are null: no compression path dereferences them
    /// (`Compression` only ever touches `thread.config`, `thread.state` and
    /// `thread.loop_`).
    struct Harness {
        thread: *mut Thread<TestApp>,
        state: *mut State,
        mutex: *mut rustty_zigstd::thread::Mutex,
        terminal: *mut Terminal,
    }

    impl Harness {
        /// Build the harness with `scrollback-compression` set as given, and a
        /// primary screen carrying enough compressible cold history that an
        /// incremental pass has real work to do.
        fn build(scrollback_compression: bool) -> Harness {
            let mut terminal = Box::new(
                Terminal::init(
                    Allocator.into(),
                    TerminalOptions {
                        cols: 80,
                        rows: 24,
                        max_scrollback_bytes: Some(10 * 1024 * 1024),
                        max_scrollback_lines: None,
                        colors: rustty_vt::terminal::Colors::default(),
                        default_modes: rustty_vt::modes::ModePacked::default(),
                        default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                        default_cursor_blink: Some(false),
                        kitty_image_storage_limit: 0,
                        kitty_image_loading_limits:
                            rustty_vt::kitty::graphics_image::Limits::DIRECT,
                    },
                )
                .expect("Terminal::init"),
            );

            // Real terminal output, so the primary PageList grows real cold pages.
            {
                let mut stream = terminal.vt_stream();
                let line = b"repeated and compressible terminal history\r\n";
                for _ in 0..2_000 {
                    stream.next_slice(line);
                }
                stream.deinit();
            }

            let terminal: *mut Terminal = Box::into_raw(terminal);
            let mutex: *mut rustty_zigstd::thread::Mutex =
                Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
            let state: *mut State = Box::into_raw(Box::new(State {
                mutex,
                terminal,
                inspector: None,
                inspector_ptr: None,
                preedit: None,
                mouse: rustty_renderer::state::Mouse::default(),
                demand: Default::default(),
                handoff_gen: Default::default(),
                handoff_park: Default::default(),
            }));

            let config = rustty_config::Config {
                scrollback_compression,
                ..rustty_config::Config::default()
            };

            let thread = Box::into_raw(Box::new(
                Thread::<TestApp>::init(
                    Allocator,
                    &config,
                    core::ptr::null_mut(),
                    core::ptr::null_mut(),
                    state,
                    // (no renderer: the compression harness never locks it)
                    DrawMutex::new(),
                )
                .expect("Thread::init"),
            ));

            Harness {
                thread,
                state,
                mutex,
                terminal,
            }
        }

        /// Exactly what `wakeupCallback` does for compression.
        fn wake(&self) {
            Compression::wake(unsafe { &raw mut (*self.thread).compression }, self.thread);
        }

        /// Run the loop until the compression timer chain disarms itself. The
        /// compression timer is the ONLY completion this harness registers, so
        /// `.until_done` returns precisely when the idle pass is over (or
        /// immediately when nothing was scheduled).
        fn run(&self) {
            // N431: through the raw loop pointer the compression timer re-arms on.
            unsafe { xev::Loop::run_raw(&raw mut (*self.thread).loop_, xev::RunMode::UntilDone) }
                .expect("loop.run");
        }

        /// How many primary-screen pages are currently encoded (compressed) —
        /// the observable that proves compression really ran.
        fn compressed_pages(&self) -> usize {
            let primary = unsafe {
                (*self.terminal)
                    .screens
                    .get(rustty_vt::screen_set::Key::Primary)
            }
            .expect("primary screen");
            unsafe { (*primary).pages.memory_stats() }.compressed_pages
        }

        fn activity(&self) -> Option<u64> {
            unsafe { (*self.thread).compression.activity }
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            unsafe {
                (*self.thread).deinit();
                drop(Box::from_raw(self.thread));
                drop(Box::from_raw(self.state));
                drop(Box::from_raw(self.mutex));
                let mut t = Box::from_raw(self.terminal);
                t.deinit(Allocator.into());
            }
        }
    }

    /// THE POSITIVE ARM: with `scrollback-compression = true`, an idle renderer
    /// drives `Terminal::compress` all the way to `.complete` and real pages end
    /// up encoded.
    ///
    /// PORTS (behavior): oracle@main renderer/Thread.zig `Compression` — the
    /// reference has no test for this, so the assertions pin the observable
    /// contract: the idle pass runs to completion (`step` returned `null`, which
    /// is why `loop.run(.until_done)` returned at all) and it compressed pages.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-core, parity tail C-1): the harness's `Thread::init` opens a real xev event loop, whose `kqueue` Miri cannot call, and the pass is driven by that loop; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn compression_idle_pass_compresses_scrollback() {
        assert!(
            rustty_vt::compression_enabled(),
            "this target supports strict reclamation; the D3 scheduler needs it"
        );

        let h = Harness::build(true);
        assert_eq!(
            h.compressed_pages(),
            0,
            "nothing is compressed before the pass"
        );

        // First wake: the activity token differs from `None`, so a pass is armed.
        h.wake();
        assert!(
            h.activity().is_some(),
            "the wake latched the activity token"
        );

        h.run();

        // The loop returned, so the timer chain disarmed: `step` returned `null`,
        // i.e. `Terminal::compress` reported `.complete` (`.unsupported` is ruled
        // out by the `compression_enabled` assert above).
        assert!(
            h.compressed_pages() > 0,
            "the idle pass compressed cold scrollback pages"
        );

        // The scheduler's own timer is disarmed and was NOT rescheduled, which is
        // exactly `step` having returned `null` — i.e. `Terminal::compress`
        // reported `.complete`. (`.unsupported` cannot be the `null` reason here:
        // it would also mean zero compressed pages, contradicting the assert
        // above, and it is ruled out by `compression_enabled()`.)
        assert_eq!(
            xev::Completion::state_of(unsafe { &(*h.thread).compression.completion }),
            xev::CompletionState::Dead,
            "the idle pass reached `.complete` and stopped rescheduling"
        );
    }

    /// THE NEGATIVE ARM (what makes the positive arm non-vacuous): with
    /// `scrollback-compression = false` the very same harness schedules nothing,
    /// never touches the terminal, and compresses nothing.
    ///
    /// PORTS: oracle 0fb89f4ff — the `wake`/`step` config guards.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-core, parity tail C-1): the harness's `Thread::init` opens a real xev event loop, whose `kqueue` Miri cannot call, and the pass is driven by that loop; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn compression_is_inert_when_the_option_is_disabled() {
        let h = Harness::build(false);

        h.wake();
        assert!(
            h.activity().is_none(),
            "a disabled scheduler must not even read the activity token"
        );
        assert_eq!(
            xev::Completion::state_of(unsafe { &(*h.thread).compression.completion }),
            xev::CompletionState::Dead,
            "a disabled scheduler must not arm the idle timer"
        );

        // Nothing is registered, so this returns immediately; it would also give
        // the timer chain a chance to run if one had been armed.
        h.run();
        assert_eq!(h.compressed_pages(), 0);

        // `step`, called directly, is inert too (the second 0fb89f4ff guard).
        assert_eq!(
            Compression::step(unsafe { &raw mut (*h.thread).compression }, h.thread),
            None
        );
        assert_eq!(h.compressed_pages(), 0);
    }

    /// `changeConfig` clears the latched activity token on a disabled -> enabled
    /// transition so a fresh idle pass starts even though no terminal activity
    /// occurred while compression was off — and does NOT clear it otherwise.
    ///
    /// PORTS: oracle 0fb89f4ff `changeConfig` (:513-518).
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-core, parity tail C-1): the harness's `Thread::init` opens a real xev event loop, whose `kqueue` Miri cannot call, and the pass is driven by that loop; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn change_config_restarts_the_pass_only_on_disabled_to_enabled() {
        fn derived(scrollback_compression: bool) -> thread::DerivedConfig {
            // (PORT NOTE `aee7bf347`: `custom_shader_animation` moved to the
            // renderer's own DerivedConfig.)
            thread::DerivedConfig {
                scrollback_compression,
            }
        }

        // enabled -> enabled: the latched token survives (no needless restart).
        let h = Harness::build(true);
        h.wake();
        let latched = h.activity();
        assert!(latched.is_some());
        Thread::<TestApp>::changeConfig(h.thread, derived(true));
        assert_eq!(h.activity(), latched);

        // enabled -> disabled: also no clear (the guard is one-directional).
        Thread::<TestApp>::changeConfig(h.thread, derived(false));
        assert_eq!(h.activity(), latched);

        // disabled -> enabled: cleared, so the next wake arms a fresh pass.
        Thread::<TestApp>::changeConfig(h.thread, derived(true));
        assert_eq!(h.activity(), None);

        // Drain the pass the first wake armed so the loop has no live completion
        // when the harness tears down.
        h.run();
    }
}

// ===========================================================================
// F-014 PROVING TESTS — the core half of the surface-mailbox seam
// ===========================================================================
//
// Before the fix, `Renderer::init` DISCARDED the real `apprt.surface.Mailbox`
// (`let _ = (surface_mailbox, rt_surface, thread);`) and the renderer's mailbox
// was a unit struct whose `push` returned 0, so the renderer's `.scrollbar` /
// `.renderer_health` pushes reached NOTHING. These tests drive the shipped
// adapter and assert the messages land on the REAL app queue in the exact
// `apprt.surface.Message` shape `Surface::handleMessage` dispatches on.
#[cfg(test)]
mod surface_mailbox_seam_tests {
    use rustty_renderer::generic::{
        MailboxTimeout as RTimeout, SurfaceMailboxSink, SurfaceMessage as RMessage,
    };
    use rustty_renderer::Health;
    use rustty_vt::pagelist::grow::Scrollbar;

    use super::compression_tests::TestApp;
    use super::*;
    use crate::app::{Mailbox as AppMailboxHandle, Message as AppMessage, Queue};

    fn scrollbar() -> Scrollbar {
        Scrollbar {
            total: 120,
            offset: 7,
            len: 24,
        }
    }

    /// The renderer's two pushes, driven through the shipped
    /// [`SurfaceMailboxAdapter`], must land on the real `App.Mailbox.Queue` as
    /// `App.Message.surface_message` envelopes carrying the surface pointer and
    /// the matching `apprt.surface.Message` variant — the exact shape
    /// `App.surfaceMessage` -> `Surface::handleMessage` ->
    /// `updateScrollbar`/`updateRendererHealth` consumes.
    #[test]
    fn adapter_enqueues_scrollbar_and_health_onto_the_real_app_queue() {
        let mut rt_app = TestApp;
        let mut queue: Box<Queue<TestApp>> = Queue::create();

        // The envelope only CARRIES the surface pointer (the app thread dispatches
        // on it later); nothing on this path dereferences it, so a null stand-in
        // keeps the test free of a full `Surface` stand-up.
        let surface: *mut crate::surface::Surface<TestApp> = core::ptr::null_mut();
        let adapter = SurfaceMailboxAdapter {
            surface_mailbox: SurfaceMailbox::<TestApp> {
                surface,
                app: AppMailboxHandle::<TestApp> {
                    rt_app: &raw mut rt_app,
                    mailbox: &raw mut *queue,
                },
            },
        };

        // `push` returns the queue size AFTER the push (the reference's
        // `App.Mailbox.Queue.Size`) — the `> 0` the scrollbar `defer` tests.
        assert_eq!(
            adapter.push(RMessage::Scrollbar(scrollbar()), RTimeout::Instant),
            1
        );
        assert_eq!(
            adapter.push(
                RMessage::RendererHealth(Health::Unhealthy),
                RTimeout::Forever
            ),
            2
        );

        let mut drained: Vec<AppMessage<TestApp>> = Vec::new();
        while let Some(msg) = queue.pop() {
            drained.push(msg);
        }
        assert_eq!(drained.len(), 2, "both renderer pushes must be queued");

        match &drained[0] {
            AppMessage::SurfaceMessage(env) => {
                assert_eq!(env.surface, surface);
                match env.message {
                    apprt::surface::Message::Scrollbar(got) => assert_eq!(got, scrollbar()),
                    _ => panic!("expected apprt.surface.Message.scrollbar"),
                }
            }
            _ => panic!("expected App.Message.surface_message"),
        }
        match &drained[1] {
            AppMessage::SurfaceMessage(env) => {
                assert_eq!(env.surface, surface);
                match env.message {
                    apprt::surface::Message::RendererHealth(got) => {
                        assert_eq!(got, Health::Unhealthy)
                    }
                    _ => panic!("expected apprt.surface.Message.renderer_health"),
                }
            }
            _ => panic!("expected App.Message.surface_message"),
        }
    }

    /// A FULL queue makes the adapter report 0 — the value `drawFrame`'s
    /// scrollbar `defer` needs in order to KEEP `scrollbar_dirty` set ("Fail
    /// instantly if the surface mailbox if full, we'll just get it on the next
    /// frame", generic.zig:1450-1455).
    #[test]
    fn adapter_reports_zero_when_the_queue_is_full() {
        let mut rt_app = TestApp;
        let mut queue: Box<Queue<TestApp>> = Queue::create();
        let surface: *mut crate::surface::Surface<TestApp> = core::ptr::null_mut();
        let adapter = SurfaceMailboxAdapter {
            surface_mailbox: SurfaceMailbox::<TestApp> {
                surface,
                app: AppMailboxHandle::<TestApp> {
                    rt_app: &raw mut rt_app,
                    mailbox: &raw mut *queue,
                },
            },
        };

        // `Queue<A> = BlockingQueue<Message<A>, 64>`; fill it to capacity.
        for i in 0..64 {
            assert!(
                adapter.push(RMessage::Scrollbar(scrollbar()), RTimeout::Instant) > 0,
                "push {i} should still fit"
            );
        }
        assert_eq!(
            adapter.push(RMessage::Scrollbar(scrollbar()), RTimeout::Instant),
            0,
            "a full queue must refuse an .instant push"
        );

        while queue.pop().is_some() {}
    }
}

// ===========================================================================
// task 14d-1 (S114-F2, S114-F3): the restored `renderer/Thread.zig`
// diagnostics, and the cursor-cancel control flow.
//
// Chosen for shapes that break INDEPENDENTLY rather than one test per site:
// the two callbacks whose null-userdata guard was elided outright, the
// compression timer's Canceled-vs-other split, the cursor-cancel row's
// BEHAVIOUR (which is not a log line at all), and the mailbox message
// rendering. Every log assertion is a full-vector `assert_eq!` on the
// captured records.
//
// All five run without a renderer, a GPU or a live xev loop: each drives a
// callback directly on the calling thread, which is also why plain
// `log_capture::start` (thread-isolated) is enough here and the
// `start_admitting_all_threads` escape hatch is NOT used — nothing under test
// spawns.
// ===========================================================================
#[cfg(test)]
mod thread_diagnostics_tests {
    use super::compression_tests::TestApp;
    use super::*;
    use crate::log_capture;

    /// `Thread.zig:563` — the `self_ orelse` arm of `renderCallback`, elided by
    /// the port as "non-null by construction". `rustty-xev` hands userdata to a
    /// callback as a raw `*mut Userdata`, so the arm is reachable and the
    /// reference's disarm-and-warn is the correct behaviour; before this it was
    /// a null dereference.
    #[test]
    fn render_callback_warns_and_disarms_without_data_set() {
        let cap = log_capture::start();
        let action = Thread::<TestApp>::renderCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Ok(()),
        );
        let messages = cap.messages();
        drop(cap);

        assert_eq!(xev::CallbackAction::Disarm, action);
        assert_eq!(
            vec![(
                log::Level::Warn,
                "renderer_thread".to_string(),
                "render callback fired without data set".to_string()
            )],
            messages
        );
    }

    /// `Thread.zig:625` — the same shape in `animationTimerCallback`, with the
    /// OTHER message text. A single shared guard would pass the test above and
    /// fail this one.
    #[test]
    fn animation_timer_callback_warns_and_disarms_without_data_set() {
        let cap = log_capture::start();
        let action = Thread::<TestApp>::animationTimerCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Ok(()),
        );
        let messages = cap.messages();
        drop(cap);

        assert_eq!(xev::CallbackAction::Disarm, action);
        assert_eq!(
            vec![(
                log::Level::Warn,
                "renderer_thread".to_string(),
                "animation callback fired without data set".to_string()
            )],
            messages
        );
    }

    /// `Thread.zig:823` — the compression timer warns on any error EXCEPT
    /// `Canceled`, and disarms either way. The port previously returned
    /// `.disarm` for both with no diagnostic at all; a fix that logged
    /// unconditionally would fail the first half of this test, which is the
    /// success-path-diagnostic bug class (a canceled timer is routine).
    #[test]
    fn compression_timer_warns_only_on_a_non_canceled_error() {
        let cap = log_capture::start();
        let canceled = Compression::timerCallback::<TestApp>(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Err(xev::TimerRunError::Canceled),
        );
        let after_canceled = cap.messages();
        let unexpected = Compression::timerCallback::<TestApp>(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Err(xev::TimerRunError::Unexpected),
        );
        let after_unexpected = cap.messages();
        drop(cap);

        assert_eq!(xev::CallbackAction::Disarm, canceled);
        assert_eq!(xev::CallbackAction::Disarm, unexpected);
        assert_eq!(Vec::<log_capture::CapturedRecord>::new(), after_canceled);
        assert_eq!(
            vec![(
                log::Level::Warn,
                "renderer_thread".to_string(),
                "error in compression timer err=Unexpected".to_string()
            )],
            after_unexpected
        );
    }

    /// S114-F2, the BEHAVIOURAL half. On kqueue targets `xev::CancelError` has
    /// the single variant `Canceled`, which is the reference's own success arm
    /// — so the correct behaviour here is silence and `.disarm`, and there is
    /// no reachable `unreachable` branch on this target. This test exists to
    /// pin exactly that: a "fix" that warned on every cancel error would fire
    /// on every routine cursor-blink cancel.
    ///
    /// The other half of the row — `NotFound` silent, everything else warning
    /// and then trapping — is only reachable where `CancelError` has more than
    /// one variant (the Linux/dynamic set), and is asserted by the two tests
    /// below, which compile only there.
    #[test]
    #[cfg(not(any(target_os = "linux", windows)))]
    fn cursor_cancel_callback_is_silent_on_the_success_arm() {
        let cap = log_capture::start();
        let action = Thread::<TestApp>::cursorCancelCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Err(xev::CancelError::Canceled),
        );
        let messages = cap.messages();
        drop(cap);

        assert_eq!(xev::CallbackAction::Disarm, action);
        assert_eq!(Vec::<log_capture::CapturedRecord>::new(), messages);
    }

    /// S114-F2 on the target where the divergence was real: `NotFound` is the
    /// reference's second success arm ("completed before it could cancel") and
    /// must stay silent.
    #[test]
    #[cfg(target_os = "linux")]
    fn cursor_cancel_callback_is_silent_on_not_found() {
        let cap = log_capture::start();
        let action = Thread::<TestApp>::cursorCancelCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Err(xev::CancelError::NotFound),
        );
        let messages = cap.messages();
        drop(cap);

        assert_eq!(xev::CallbackAction::Disarm, action);
        assert_eq!(Vec::<log_capture::CapturedRecord>::new(), messages);
    }

    /// S114-F2, REVERSED by task R13-C under R13 (UD73). This test used to be
    /// `cursor_cancel_callback_traps_on_an_impossible_error` and pinned the
    /// reference's warn-then-`unreachable`. Those errors are kernel-reachable
    /// on io_uring, so each must now WARN (the reference's own diagnostic,
    /// which also proves the error arm was reached rather than a success arm)
    /// and then DISARM without panicking. The injected errors stand in for the
    /// kernel's `-EALREADY` race, which cannot be scheduled.
    #[test]
    #[cfg(target_os = "linux")]
    fn cursor_cancel_callback_warns_and_disarms_on_a_cancel_error() {
        for err in [
            xev::CancelError::ExpirationInProgress,
            xev::CancelError::Unexpected,
        ] {
            let cap = log_capture::start();
            let action = Thread::<TestApp>::cursorCancelCallback(
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                Err(err),
            );
            let messages = cap.messages();
            drop(cap);

            assert_eq!(xev::CallbackAction::Disarm, action, "{err:?}");
            assert_eq!(1, messages.len(), "{err:?}: {messages:?}");
            assert!(
                format!("{:?}", messages[0])
                    .contains(&format!("error in cursor cancel callback err={err:?}")),
                "{messages:?}"
            );
        }
    }

    /// The Windows (IOCP) arm (task W-2, N342): the only `CancelError` there is
    /// `Unexpected`, the reference's `else` arm; it warns and disarms as the
    /// Linux arm does (UD73).
    #[test]
    #[cfg(windows)]
    fn cursor_cancel_callback_warns_and_disarms_on_the_iocp_cancel_error() {
        let err = xev::CancelError::Unexpected;
        let cap = log_capture::start();
        let action = Thread::<TestApp>::cursorCancelCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Err(err),
        );
        let messages = cap.messages();
        drop(cap);

        assert_eq!(xev::CallbackAction::Disarm, action);
        assert_eq!(1, messages.len(), "{messages:?}");
        assert!(
            format!("{:?}", messages[0])
                .contains(&format!("error in cursor cancel callback err={err:?}")),
            "{messages:?}"
        );
    }

    /// Inventory M16 (R13): the three timer callbacks' `else => unreachable`
    /// arms, reachable on Linux io_uring. Driven by an injected
    /// `TimerRunError::Unexpected` (the kernel error cannot be forced), with a
    /// null thread: the error arm must return before the thread is touched,
    /// which a null pointer would expose. Each disarms; the cursor timer keeps
    /// the reference's warning, the other two stay silent as the reference is.
    #[test]
    fn timer_callbacks_disarm_on_an_unexpected_error() {
        let cap = log_capture::start();
        let render = Thread::<TestApp>::renderCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Err(xev::TimerRunError::Unexpected),
        );
        let animation = Thread::<TestApp>::animationTimerCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Err(xev::TimerRunError::Unexpected),
        );
        let silent = cap.messages();
        let cursor = Thread::<TestApp>::cursorTimerCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Err(xev::TimerRunError::Unexpected),
        );
        let messages = cap.messages();
        drop(cap);

        assert_eq!(xev::CallbackAction::Disarm, render);
        assert_eq!(xev::CallbackAction::Disarm, animation);
        assert_eq!(xev::CallbackAction::Disarm, cursor);
        assert_eq!(
            Vec::<log_capture::CapturedRecord>::new(),
            silent,
            "render and animation have no diagnostic in the reference"
        );
        assert_eq!(1, messages.len(), "{messages:?}");
        assert!(
            format!("{:?}", messages[0]).contains("error in cursor timer callback err=Unexpected"),
            "{messages:?}"
        );
    }

    /// UD75 (R13, task R13-C2): `stopCallback`'s `_ = r catch unreachable`.
    /// The wait error (an epoll `ENOSPC`/`ENOMEM` on the stop eventfd's
    /// registration) cannot be forced, so it is injected, with a null thread:
    /// the error arm must return before the thread is touched (a null pointer
    /// would expose it, and a stopped loop would need one). It re-arms, as
    /// `wakeupCallback` does for the same error type, and logs nothing, as
    /// the reference has no message here.
    #[test]
    fn stop_callback_rearms_on_a_wait_error() {
        let cap = log_capture::start();
        let action = Thread::<TestApp>::stopCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Err(xev::WaitError::Unexpected),
        );
        let messages = cap.messages();
        drop(cap);
        assert_eq!(xev::CallbackAction::Rearm, action);
        assert_eq!(Vec::<log_capture::CapturedRecord>::new(), messages);
    }

    /// `Thread.zig:308` — `mailbox message={}` renders the ACTIVE TAG AND ITS
    /// PAYLOAD, using the reference's own field names. A tag-only rendering
    /// (the obvious shortcut) fails on every payload-bearing arm here.
    #[test]
    fn mailbox_message_renders_the_reference_union_syntax() {
        use rustty_renderer::Message as M;
        assert_eq!("{ .crash = void }", MessageDbg(&M::Crash).to_string());
        assert_eq!("{ .focus = true }", MessageDbg(&M::Focus(true)).to_string());
        assert_eq!(
            "{ .visible = false }",
            MessageDbg(&M::Visible(false)).to_string()
        );
        assert_eq!(
            "{ .reset_cursor_blink = void }",
            MessageDbg(&M::ResetCursorBlink).to_string()
        );
        assert_eq!(
            "{ .inspector = true }",
            MessageDbg(&M::Inspector(true)).to_string()
        );
        assert_eq!(
            "{ .macos_display_id = 7 }",
            MessageDbg(&M::MacosDisplayId(7)).to_string()
        );
        assert_eq!(
            "{ .search_selected_match = null }",
            MessageDbg(&M::SearchSelectedMatch(None)).to_string()
        );
    }

    /// NEW-FINDINGS N13: `Thread.zig:496`/`:542`/`:729` unwrap `self_.?`, a
    /// checked unwrap. A null userdata must fail loudly at the unwrap, not
    /// dereference null further in. (R7: an invariant check, ported checked.)
    #[test]
    #[should_panic(expected = "attempt to use null value")]
    fn wakeup_callback_null_userdata_panics_at_the_unwrap() {
        Thread::<TestApp>::wakeupCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Ok(()),
        );
    }

    #[test]
    #[should_panic(expected = "attempt to use null value")]
    fn draw_now_callback_null_userdata_panics_at_the_unwrap() {
        Thread::<TestApp>::drawNowCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Ok(()),
        );
    }

    #[test]
    #[should_panic(expected = "attempt to use null value")]
    fn stop_callback_null_userdata_panics_at_the_unwrap() {
        Thread::<TestApp>::stopCallback(
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            Ok(()),
        );
    }
}

#[cfg(test)]
mod cursor_blink_interval_tests {
    use super::{cursorBlinkInterval, cursor_blink_interval_for};

    /// Parity S114-F1: under Valgrind (one or more) the interval is five
    /// times `CURSOR_BLINK_INTERVAL` (`Thread.zig:735-748`); natively it is
    /// the plain 600 ms.
    ///
    /// This test binary also runs under Valgrind (G-VALGRIND), where
    /// `cursorBlinkInterval()` is 3000 wherever client requests are compiled
    /// in; the test used to assert the native value unconditionally and failed
    /// there (NEW-FINDINGS N414).
    #[test]
    fn cursor_blink_interval_is_five_times_longer_under_valgrind() {
        assert_eq!(600, cursor_blink_interval_for(0));
        assert_eq!(3000, cursor_blink_interval_for(1));
        assert_eq!(3000, cursor_blink_interval_for(2));
        let answered = under_valgrind_by_maps() && rustty_zigstd::valgrind::VALGRIND_SUPPORT;
        assert_eq!(if answered { 3000 } else { 600 }, cursorBlinkInterval());
    }

    /// Whether this process runs under Valgrind, from evidence that does not
    /// go through the client request `cursorBlinkInterval` uses: Valgrind maps
    /// its `vgpreload_*.so` libraries into every program it runs (Linux; no
    /// other host the port tests on has a Valgrind).
    fn under_valgrind_by_maps() -> bool {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_to_string("/proc/self/maps")
                .expect("/proc/self/maps is readable on Linux")
                .contains("/vgpreload_")
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }
}
