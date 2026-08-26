//! Surface-information inspector widget + the top-level surface `Inspector`.
//!
//! PORTS: ghostty src/inspector/widgets/surface.zig
//!
//! PORT STATUS: all blockers landed (`widgets::termio::Stream` Task 16,
//! `widgets::terminal::Info` Task 14, the DockBuilder FFI set), so the whole
//! file is now INHABITED:
//!
//!   * the window-name constants (`window_imgui_demo` … `window_renderer`),
//!   * **`Mouse`** — the persisted inspector mouse state whose `last_point:
//!     Option<Pin>` is the **R3 hazard field** (a raw, non-owning `Pin` into
//!     PageList memory that a reflow invalidates). Its SB/TB-clean handling is
//!     PROVEN on the isolated `.port/phase-2/mirihar-inspector-pin` Miri harness
//!     (never hand-reasoned, R5): the stored `Pin` is a weak handle, re-resolved
//!     by the address-only `PageList::point_from_pin` walk (which never
//!     dereferences the pin's own node), `None` on failure — never a live borrow,
//!   * the top-level `Inspector` aggregate (`surface_info` / `key_stream` /
//!     `terminal_info` / `vt_stream` / `renderer_info` / the demo-window flag)
//!     with `init`/`deinit`/`draw`/`createDockSpace`,
//!   * the surface `Info` widget with its `draw`, and
//!   * the `dimensionsTable`/`fontTable`/`mouseTable` free functions.
//!
//! **R4 acyclic Surface seam.** The reference's `Inspector.draw` (a `*Surface`
//! since `ad96613a8` — the termio Copy/Export buttons reach
//! `surface.rt_surface`/`surface.rt_app` through it) / `Info.draw` / the three
//! tables (still `*const Surface`) take the surface pointer (LIFETIMES row:
//! `Inspector.draw.surface`, BORROW_PARAM — every terminal read under
//! `surface.renderer_state.mutex`, INV-1). `Surface` lives in rustty-core,
//! which ALREADY depends on rustty-inspector (`Option<*mut inspectorpkg::
//! Inspector>`), so a direct `rustty_core::Surface` name here would CYCLE.
//! Unlike the inspector -> renderer overlay edge (INV-7, a genuine
//! bidirectional coupling needing a renderer-owned trait object), every field
//! the deferred bodies below actually touch turns out to be a plain `Copy`
//! value or raw pointer from a crate this module ALREADY depends on
//! (`rustty_vt`) or the renderer (`rustty_renderer::size::{Size,Coordinate}`,
//! `rustty_renderer::state::State` — both already reachable via the INV-7
//! edge). So the seam is the SAME "named params struct in place of a whole
//! object pointer" shape `screen::DrawData`/`terminal::screensTable` already
//! use (the S6 note), just one tier up: [`SurfaceData`] replaces `*const
//! Surface`, and `Inspector::render`'s wakeup handle (Inspector.rs) crosses as
//! a bare `&rustty_xev::Async` (also a leaf below core) rather than the whole
//! core-local `renderer_thread: Thread<A>`. No new rustty-inspector -> core (or
//! -> apprt) dependency edge is added; `check_acyclic.sh` stays green.
//!
//! All bodies run on the main thread with the surface's ImGui context current
//! (INV-11); the whole `draw` holds ONE `renderer_state.mutex` lock across all
//! five sub-windows (INV-1).

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const cimgui = @import("dcimgui");
// const inspector = @import("../main.zig");
// const widgets = @import("../widgets.zig");
// const input = @import("../../input.zig");
// const renderer = @import("../../renderer.zig");
// const terminal = @import("../../terminal/main.zig");
// const Surface = @import("../../Surface.zig");
//
// PORT NOTE (screen.rs/pagelist.rs precedent): the `terminal.*` types live in
// rustty-vt — `terminal.Pin` is `rustty_vt::pagelist::Pin`, `terminal.point.
// Coordinate` is `rustty_vt::point::Coordinate`. `dcimgui` is the vendored Dear
// ImGui C API reached through `dcimgui-sys`. `input.MouseButton` (mouseTable's
// button labels) is `rustty_vt::input::mouse::Button` (the same enum the core
// `Surface.Mouse.click_state` array is keyed by); `renderer.Coordinate`
// (mouseTable's px conversion) in rustty-renderer; `Surface`/`inspector`/
// `widgets.{terminal,termio,key,renderer}` are the sibling modules named below.
use core::ffi::{c_int, c_void, CStr};

use allocator_api2::alloc::Global as Allocator;

use rustty_vt::input::mouse::{Button, ButtonState};
use rustty_vt::pagelist::Pin;
use rustty_vt::point::{Coordinate as PointCoordinate, Tag as PointTag};
use rustty_vt::selection_gesture::SelectionGesture;
use rustty_vt::terminal::Terminal;

// PORT NOTE (R4 acyclic Surface seam): `rustty_renderer::size` re-exports the
// A1-hoisted `rustty-render-types` leaf (renderer.zig's `Size`/`Coordinate`),
// and `rustty_renderer::state::State` is the renderer `State.zig` port
// (mutex + terminal raw pointer) — both ALREADY nameable here (this crate
// already depends on rustty-renderer for the INV-7 `InspectorOverlay` edge).
// `SurfaceData` below replaces the reference's `surface: *const Surface`
// parameter with the flat set of these already-nameable values/pointers a
// caller obtains from a `Surface` — the same "anonymous `data` struct in place
// of a whole object pointer" shape `screen::DrawData`/`terminal::screensTable`
// already use, one tier up: `rustty_core::Surface` cannot be named from this
// crate without an inspector -> core dependency cycle (core already depends on
// inspector).
use rustty_renderer::size::{
    Coordinate as RenderCoordinate, CoordinateSurface, CoordinateTag, Size,
};
use rustty_renderer::state::State as RendererState;

use super::{key, renderer, terminal, termio};

/// This is discovered via the hardcoded string in the ImGui demo window.
// const window_imgui_demo = "Dear ImGui Demo";
pub const WINDOW_IMGUI_DEMO: &CStr = c"Dear ImGui Demo";
// const window_keyboard = "Keyboard";
pub const WINDOW_KEYBOARD: &CStr = c"Keyboard";
// const window_terminal = "Terminal";
pub const WINDOW_TERMINAL: &CStr = c"Terminal";
// const window_surface = "Surface";
pub const WINDOW_SURFACE: &CStr = c"Surface";
// const window_termio = "Terminal IO";
pub const WINDOW_TERMIO: &CStr = c"Terminal IO";
// const window_renderer = "Renderer";
pub const WINDOW_RENDERER: &CStr = c"Renderer";

// pub const Mouse = struct {
//     /// Last hovered x/y
//     last_xpos: f64 = 0,
//     last_ypos: f64 = 0,
//
//     // Last hovered screen point
//     last_point: ?terminal.Pin = null,
// };
//
/// Extra mouse state the inspector tracks per-surface (the `Inspector.mouse`
/// field, Inspector.zig:21). LIVE.
///
/// PORT NOTE (R3 — `last_point` Pin invalidation, PROVEN on
/// `.port/phase-2/mirihar-inspector-pin`): `last_point` is a persisted, NON-owning,
/// `Copy` [`Pin`] raw-aliasing PageList `Node` memory (`pagelist/mod.rs:630`; the
/// LIFETIMES `Mouse.last_point` BORROW_FIELD row). A reflow / page-prune between the
/// mouse-move that records it and the draw that reads it FREES the pinned node,
/// leaving `last_point` a DANGLING handle. The port must treat it as a
/// **weak/raw handle, never a live borrow**: `mouseTable` (below) re-resolves it
/// null-guarded via `pages.point_from_pin(.active, p)` (`pagelist/pin.rs:121`),
/// which locates the node **by address comparison only** (`core::ptr::eq`, never
/// dereferencing `p.node`) and returns `None` when the pin no longer fits —
/// SB/TB-clean across the reflow. The `Copy`/no-`Drop` derive is load-bearing:
/// storing/dropping a stale `Mouse` never touches `*last_point.node`.
#[derive(Clone, Copy)]
pub struct Mouse {
    /// Last hovered x/y
    pub last_xpos: f64,
    pub last_ypos: f64,

    // Last hovered screen point
    pub last_point: Option<Pin>,
}

impl Default for Mouse {
    // Mouse = struct { last_xpos: f64 = 0, last_ypos: f64 = 0,
    //                  last_point: ?terminal.Pin = null };
    fn default() -> Mouse {
        Mouse {
            last_xpos: 0.0,
            last_ypos: 0.0,
            last_point: None,
        }
    }
}

/// Minimal `defer`-style scope guard — runs `f` on drop (mirrors the private
/// `Defer` in `widgets.rs`/`screen.rs`/`terminal.rs`). Backs the ImGui
/// `Begin`/`End` and `BeginTable`/`EndTable` pairings below so they fire on
/// every exit path. panic=abort means no unwinding ever crosses these FFI
/// calls, so drop-on-scope-exit is the sole runtime path (no double-fire, no
/// leak).
struct Defer<F: FnMut()>(F);

impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

/// The `rt_surface.setClipboard` seam trampoline type — see
/// [`SurfaceData::rt_set_clipboard`].
pub type RtSetClipboardFn =
    unsafe fn(surface: *mut c_void, mime: &[u8], data: &[u8], confirm: bool) -> Result<(), ()>;

/// The `rt_app.performAction(..., .export_terminal_io, ...)` seam trampoline
/// type — see [`SurfaceData::rt_export_terminal_io`].
pub type RtExportTerminalIOFn =
    unsafe fn(surface: *mut c_void, contents: &[u8]) -> Result<bool, ()>;

/// Surface-level state `Inspector::draw`/`Info::draw`/the three tables need —
/// see the module doc's R4 acyclic Surface seam note. Every field is a plain
/// `Copy` value, raw pointer, or monomorphized trampoline fn pointer, all
/// call-scoped (INV-1), obtained by the (future) core caller from its
/// `Surface` (under `renderer_state`'s mutex where the reference reads a live
/// terminal).
pub struct SurfaceData {
    /// `surface.renderer_state` (mutex + terminal raw pointer). `Inspector::
    /// draw` locks/unlocks this itself, exactly mirroring the reference's
    /// `surface.renderer_state.mutex.lock()`/`.unlock()` (INV-1).
    pub renderer_state: *mut RendererState,
    /// `surface.alloc`.
    pub alloc: Allocator,
    /// `surface.size` (screen/cell/padding + `.grid()`).
    pub size: Size,
    /// `surface.font_size.points`.
    pub font_size_points: f32,
    /// `surface.font_size.pixels()`.
    pub font_size_pixels: f32,
    /// `surface.mouse.click_state`.
    pub mouse_click_state: [ButtonState; Button::MAX as usize],
    /// `&surface.mouse.selection_gesture` — raw, call-scoped (INV-1).
    pub selection_gesture: *const SelectionGesture,

    /// The reference `surface: *Surface` ITSELF, type-erased (`ad96613a8` made
    /// the widget signatures non-const so termio's Copy/Export buttons can
    /// reach `surface.rt_surface`/`surface.rt_app` through it): the context
    /// pointer the two seam trampolines below recover their `*mut Surface`
    /// from. Valid for the duration of `draw` (INV-1), like every other field
    /// here.
    pub surface: *mut c_void,

    /// `surface.rt_surface.setClipboard(.standard, &.{.{ .mime, .data }},
    /// confirm)` (termio `copyEvents`, `ad96613a8`), monomorphized by the
    /// apprt caller. The `.standard` clipboard target is fixed inside the
    /// trampoline (the only kind the widget names); `mime`/`data`/`confirm`
    /// cross exactly as the widget passes them, with `data`'s backing memory
    /// carrying a NUL one past the slice (the `[:0]const u8` convention of
    /// core's own `setClipboard` callsites). The concrete backend error erases
    /// to `Err(())` — consumed by the widget's `catch log.err` arm.
    pub rt_set_clipboard: RtSetClipboardFn,

    /// `surface.rt_app.performAction(.{ .surface = surface },
    /// .export_terminal_io, .{ .contents = contents })` (termio
    /// `exportEvents`, `ad96613a8`), monomorphized by the apprt caller.
    /// Returns the reference `!bool` ("handled"); the concrete backend error
    /// erases to `Err(())`.
    pub rt_export_terminal_io: RtExportTerminalIOFn,
}

// pub const Inspector = struct {
/// The top-level surface inspector — owns/draws the terminal + surface +
/// keyboard + termio + renderer sub-windows under ONE `renderer_state.mutex`
/// lock (INV-1).
pub struct Inspector {
    /// Internal GUI state
    // surface_info: Info,
    surface_info: Info,
    // key_stream: widgets.key.Stream,
    pub key_stream: key::Stream,
    // terminal_info: widgets.terminal.Info,
    terminal_info: terminal::Info,
    // vt_stream: widgets.termio.Stream,
    pub vt_stream: termio::Stream,
    // renderer_info: widgets.renderer.Info,
    pub renderer_info: renderer::Info,
    // show_demo_window: bool,
    //
    // PORT NOTE: the field is declared UNCONDITIONALLY, exactly as the oracle
    // does (`surface.zig:29`), even though its only read is the
    // `debug_assertions`-gated demo window below. Cfg-gating the field to match
    // its use would change the struct's shape between profiles and diverge from
    // the reference; allowing dead_code only in the profile where it is
    // genuinely unread keeps the shape and silences a warning that a
    // `clippy --release -D warnings` lane would otherwise trip on.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    show_demo_window: bool,
}

impl Inspector {
    // pub fn init(alloc: Allocator) !Inspector {
    //     return .{
    //         .surface_info = .empty,
    //         .key_stream = try .init(alloc),
    //         .terminal_info = .empty,
    //         .vt_stream = try .init(alloc),
    //         .renderer_info = .empty,
    //         .show_demo_window = true,
    //     };
    // }
    //
    // PORT NOTE: `widgets::termio::Stream::init` is infallible in the port
    // (Task 16 design decision — collapses under the global allocator), unlike
    // the reference's `try .init(alloc)`; only `key::Stream::init` remains
    // fallible (the CircBuf grow-then-evict OOM signal, `key::Error`).
    pub fn init(alloc: Allocator) -> Result<Inspector, key::Error> {
        Ok(Inspector {
            surface_info: Info::empty(),
            key_stream: key::Stream::init(alloc)?,
            terminal_info: terminal::Info::empty(),
            vt_stream: termio::Stream::init(alloc),
            renderer_info: renderer::Info::empty(),
            show_demo_window: true,
        })
    }

    // pub fn deinit(self: *Inspector, alloc: Allocator) void {
    //     self.key_stream.deinit(alloc);
    //     self.vt_stream.deinit(alloc);
    //     self.renderer_info.deinit(alloc);
    // }
    pub fn deinit(&mut self, alloc: Allocator) {
        self.key_stream.deinit(alloc);
        self.vt_stream.deinit(alloc);
        self.renderer_info.deinit(alloc);
    }

    // pub fn draw(
    //     self: *Inspector,
    //     surface: *Surface,
    //     mouse: Mouse,
    // ) void {
    //
    // PORT NOTE (R4 acyclic Surface seam, module doc): `data: SurfaceData`
    // replaces the reference's `surface: *Surface` (non-const since
    // `ad96613a8` for the termio Copy/Export buttons).
    #[allow(non_snake_case)] // faithful Zig name (R1)
    pub fn draw(&mut self, data: SurfaceData, mouse: Mouse) {
        // Create our dockspace first. If we had to setup our dockspace,
        // then it is a first render.
        // const dockspace_id = cimgui.c.ImGui_GetID("Main Dockspace");
        let dockspace_id = unsafe { dcimgui_sys::ImGui_GetID(c"Main Dockspace".as_ptr()) };
        // const first_render = createDockSpace(dockspace_id);
        let first_render = Self::createDockSpace(dockspace_id);

        // Draw everything that requires the terminal state mutex.
        // {
        {
            // surface.renderer_state.mutex.lock();
            unsafe { (*(*data.renderer_state).mutex).lock() };
            // defer surface.renderer_state.mutex.unlock();
            let _unlock = Defer(|| unsafe { (*(*data.renderer_state).mutex).unlock() });
            // const t = surface.renderer_state.terminal;
            let t: *mut Terminal = unsafe { (*data.renderer_state).terminal };

            // Terminal info window
            // {
            {
                // const open = cimgui.c.ImGui_Begin(
                //     window_terminal,
                //     null,
                //     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
                // );
                let open = unsafe {
                    dcimgui_sys::ImGui_Begin(
                        WINDOW_TERMINAL.as_ptr(),
                        core::ptr::null_mut(),
                        dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing
                            as dcimgui_sys::ImGuiWindowFlags,
                    )
                };
                // defer cimgui.c.ImGui_End();
                let _end = Defer(|| unsafe { dcimgui_sys::ImGui_End() });
                // self.terminal_info.draw(open, t);
                self.terminal_info.draw(open, unsafe { &mut *t });
            }

            // Surface info window
            // {
            {
                // const open = cimgui.c.ImGui_Begin(
                //     window_surface,
                //     null,
                //     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
                // );
                let open = unsafe {
                    dcimgui_sys::ImGui_Begin(
                        WINDOW_SURFACE.as_ptr(),
                        core::ptr::null_mut(),
                        dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing
                            as dcimgui_sys::ImGuiWindowFlags,
                    )
                };
                // defer cimgui.c.ImGui_End();
                let _end = Defer(|| unsafe { dcimgui_sys::ImGui_End() });
                // self.surface_info.draw(
                //     open,
                //     surface,
                //     mouse,
                // );
                self.surface_info.draw(open, &data, mouse);
            }

            // Keyboard info window
            // {
            {
                // const open = cimgui.c.ImGui_Begin(
                //     window_keyboard,
                //     null,
                //     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
                // );
                let open = unsafe {
                    dcimgui_sys::ImGui_Begin(
                        WINDOW_KEYBOARD.as_ptr(),
                        core::ptr::null_mut(),
                        dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing
                            as dcimgui_sys::ImGuiWindowFlags,
                    )
                };
                // defer cimgui.c.ImGui_End();
                let _end = Defer(|| unsafe { dcimgui_sys::ImGui_End() });
                // self.key_stream.draw(
                //     open,
                //     surface.alloc,
                // );
                self.key_stream.draw(open, data.alloc);
            }

            // Terminal IO window
            // {
            {
                // const open = cimgui.c.ImGui_Begin(
                //     window_termio,
                //     null,
                //     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
                // );
                let open = unsafe {
                    dcimgui_sys::ImGui_Begin(
                        WINDOW_TERMIO.as_ptr(),
                        core::ptr::null_mut(),
                        dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing
                            as dcimgui_sys::ImGuiWindowFlags,
                    )
                };
                // defer cimgui.c.ImGui_End();
                let _end = Defer(|| unsafe { dcimgui_sys::ImGui_End() });
                // if (open) {
                if open {
                    //     self.vt_stream.draw(
                    //         surface,
                    //         &t.colors.palette.current,
                    //     );
                    self.vt_stream
                        .draw(&data, unsafe { &(*t).colors.palette.current });
                }
            }

            // Renderer info window
            // {
            {
                // const open = cimgui.c.ImGui_Begin(
                //     window_renderer,
                //     null,
                //     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
                // );
                let open = unsafe {
                    dcimgui_sys::ImGui_Begin(
                        WINDOW_RENDERER.as_ptr(),
                        core::ptr::null_mut(),
                        dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing
                            as dcimgui_sys::ImGuiWindowFlags,
                    )
                };
                // defer cimgui.c.ImGui_End();
                let _end = Defer(|| unsafe { dcimgui_sys::ImGui_End() });
                // self.renderer_info.draw(
                //     surface.alloc,
                //     open,
                // );
                self.renderer_info.draw(data.alloc, open);
            }
        }

        // In debug we show the ImGui demo window so we can easily view
        // available widgets and such.
        // if (comptime builtin.mode == .Debug) {
        //     if (self.show_demo_window) {
        //         cimgui.c.ImGui_ShowDemoWindow(&self.show_demo_window);
        //     }
        // }
        #[cfg(debug_assertions)]
        if self.show_demo_window {
            unsafe { dcimgui_sys::ImGui_ShowDemoWindow(&mut self.show_demo_window) };
        }

        // if (first_render) {
        if first_render {
            //     // On first render, setup our initial focus state. We only
            //     // do this on first render so that we can let the user change
            //     // focus afterward without it snapping back.
            //     cimgui.c.ImGui_SetWindowFocusStr(window_terminal);
            unsafe { dcimgui_sys::ImGui_SetWindowFocusStr(WINDOW_TERMINAL.as_ptr()) };
        }
    }

    /// Create the global dock space for the inspector. A dock space
    /// is a special area where windows can be docked into. The global
    /// dock space fills the entire main viewport.
    ///
    /// Returns true if this was the first time the dock space was created.
    // fn createDockSpace(dockspace_id: cimgui.c.ImGuiID) bool {
    #[allow(non_snake_case)] // faithful Zig name (R1)
    fn createDockSpace(dockspace_id: dcimgui_sys::ImGuiID) -> bool {
        // const viewport: *cimgui.c.ImGuiViewport = cimgui.c.ImGui_GetMainViewport();
        let viewport = unsafe { dcimgui_sys::ImGui_GetMainViewport() };

        // Initial Docking setup
        // const setup = cimgui.ImGui_DockBuilderGetNode(dockspace_id) == null;
        let setup = unsafe { dcimgui_sys::ImGui_DockBuilderGetNode(dockspace_id) }.is_null();
        // if (setup) {
        if setup {
            // Register our dockspace node
            // assert(cimgui.ImGui_DockBuilderAddNodeEx(
            //     dockspace_id,
            //     cimgui.ImGuiDockNodeFlagsPrivate.DockSpace,
            // ) == dockspace_id);
            assert_eq!(
                unsafe {
                    dcimgui_sys::ImGui_DockBuilderAddNodeEx(
                        dockspace_id,
                        dcimgui_sys::IM_GUI_DOCK_NODE_FLAGS_PRIVATE_DOCK_SPACE,
                    )
                },
                dockspace_id
            );

            // Ensure it is the full size of the viewport
            // cimgui.ImGui_DockBuilderSetNodeSize(
            //     dockspace_id,
            //     viewport.Size,
            // );
            unsafe { dcimgui_sys::ImGui_DockBuilderSetNodeSize(dockspace_id, (*viewport).Size) };

            // We only initialize one central docking point now but
            // this is the point we'd pre-split and so on for the initial
            // layout.
            // const dock_id_main: cimgui.c.ImGuiID = dockspace_id;
            let dock_id_main: dcimgui_sys::ImGuiID = dockspace_id;
            // cimgui.ImGui_DockBuilderDockWindow(window_imgui_demo, dock_id_main);
            unsafe {
                dcimgui_sys::ImGui_DockBuilderDockWindow(WINDOW_IMGUI_DEMO.as_ptr(), dock_id_main)
            };
            // cimgui.ImGui_DockBuilderDockWindow(window_terminal, dock_id_main);
            unsafe {
                dcimgui_sys::ImGui_DockBuilderDockWindow(WINDOW_TERMINAL.as_ptr(), dock_id_main)
            };
            // cimgui.ImGui_DockBuilderDockWindow(window_surface, dock_id_main);
            unsafe {
                dcimgui_sys::ImGui_DockBuilderDockWindow(WINDOW_SURFACE.as_ptr(), dock_id_main)
            };
            // cimgui.ImGui_DockBuilderDockWindow(window_keyboard, dock_id_main);
            unsafe {
                dcimgui_sys::ImGui_DockBuilderDockWindow(WINDOW_KEYBOARD.as_ptr(), dock_id_main)
            };
            // cimgui.ImGui_DockBuilderDockWindow(window_termio, dock_id_main);
            unsafe {
                dcimgui_sys::ImGui_DockBuilderDockWindow(WINDOW_TERMIO.as_ptr(), dock_id_main)
            };
            // cimgui.ImGui_DockBuilderDockWindow(window_renderer, dock_id_main);
            unsafe {
                dcimgui_sys::ImGui_DockBuilderDockWindow(WINDOW_RENDERER.as_ptr(), dock_id_main)
            };
            // cimgui.ImGui_DockBuilderFinish(dockspace_id);
            unsafe { dcimgui_sys::ImGui_DockBuilderFinish(dockspace_id) };
        }

        // Put the dockspace over the viewport.
        // assert(cimgui.c.ImGui_DockSpaceOverViewportEx(
        //     dockspace_id,
        //     viewport,
        //     cimgui.c.ImGuiDockNodeFlags_PassthruCentralNode,
        //     null,
        // ) == dockspace_id);
        assert_eq!(
            unsafe {
                dcimgui_sys::ImGui_DockSpaceOverViewportEx(
                    dockspace_id,
                    viewport,
                    dcimgui_sys::ImGuiDockNodeFlags__ImGuiDockNodeFlags_PassthruCentralNode
                        as dcimgui_sys::ImGuiDockNodeFlags,
                    core::ptr::null(),
                )
            },
            dockspace_id
        );
        // return setup;
        setup
    }
}

// (Mouse is ported LIVE above.)

/// Surface information inspector widget.
// pub const Info = struct {
pub struct Info;

impl Info {
    // pub const empty: Info = .{};
    pub fn empty() -> Info {
        Info
    }

    /// Draw the surface info window.
    // pub fn draw(
    //     self: *Info,
    //     open: bool,
    //     surface: *const Surface,
    //     mouse: Mouse,
    // ) void {
    //     _ = self;
    #[allow(non_snake_case)] // faithful Zig name (R1)
    pub fn draw(&mut self, open: bool, data: &SurfaceData, mouse: Mouse) {
        // if (!open) return;
        if !open {
            return;
        }

        // if (cimgui.c.ImGui_CollapsingHeader(
        //     "Help",
        //     cimgui.c.ImGuiTreeNodeFlags_None,
        // )) {
        if unsafe {
            dcimgui_sys::ImGui_CollapsingHeader(
                c"Help".as_ptr(),
                dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_None
                    as dcimgui_sys::ImGuiTreeNodeFlags,
            )
        } {
            // cimgui.c.ImGui_TextWrapped(
            //     "This window displays information about the surface (window). " ++
            //         "A surface is the graphical area that displays the terminal " ++
            //         "content. It includes dimensions, font sizing, and mouse state " ++
            //         "information specific to this window instance.",
            // );
            unsafe {
                dcimgui_sys::ImGui_TextWrapped(
                    c"This window displays information about the surface (window). A surface is the graphical area that displays the terminal content. It includes dimensions, font sizing, and mouse state information specific to this window instance."
                        .as_ptr(),
                )
            };
        }

        // cimgui.c.ImGui_SeparatorText("Dimensions");
        unsafe { dcimgui_sys::ImGui_SeparatorText(c"Dimensions".as_ptr()) };
        // dimensionsTable(surface);
        dimensionsTable(data);

        // cimgui.c.ImGui_SeparatorText("Font");
        unsafe { dcimgui_sys::ImGui_SeparatorText(c"Font".as_ptr()) };
        // fontTable(surface);
        fontTable(data);

        // cimgui.c.ImGui_SeparatorText("Mouse");
        unsafe { dcimgui_sys::ImGui_SeparatorText(c"Mouse".as_ptr()) };
        // mouseTable(surface, mouse);
        mouseTable(data, mouse);
    }
}

// fn dimensionsTable(surface: *const Surface) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn dimensionsTable(data: &SurfaceData) {
    // _ = cimgui.c.ImGui_BeginTable(
    //     "table_size",
    //     2,
    //     cimgui.c.ImGuiTableFlags_None,
    // );
    let _ = unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"table_size".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    };
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // Screen Size
    // {
    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Screen Size");
            unsafe { dcimgui_sys::ImGui_Text(c"Screen Size".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text(
            //     "%dpx x %dpx",
            //     surface.size.screen.width,
            //     surface.size.screen.height,
            // );
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"%dpx x %dpx".as_ptr(),
                    data.size.screen.width as c_int,
                    data.size.screen.height as c_int,
                )
            };
        }
    }

    // Grid Size
    // {
    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Grid Size");
            unsafe { dcimgui_sys::ImGui_Text(c"Grid Size".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // const grid_size = surface.size.grid();
            let grid_size = data.size.grid();
            // cimgui.c.ImGui_Text(
            //     "%dc x %dr",
            //     grid_size.columns,
            //     grid_size.rows,
            // );
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"%dc x %dr".as_ptr(),
                    grid_size.columns as c_int,
                    grid_size.rows as c_int,
                )
            };
        }
    }

    // Cell Size
    // {
    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Cell Size");
            unsafe { dcimgui_sys::ImGui_Text(c"Cell Size".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text(
            //     "%dpx x %dpx",
            //     surface.size.cell.width,
            //     surface.size.cell.height,
            // );
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"%dpx x %dpx".as_ptr(),
                    data.size.cell.width as c_int,
                    data.size.cell.height as c_int,
                )
            };
        }
    }

    // Padding
    // {
    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Window Padding");
            unsafe { dcimgui_sys::ImGui_Text(c"Window Padding".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text(
            //     "T=%d B=%d L=%d R=%d px",
            //     surface.size.padding.top,
            //     surface.size.padding.bottom,
            //     surface.size.padding.left,
            //     surface.size.padding.right,
            // );
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"T=%d B=%d L=%d R=%d px".as_ptr(),
                    data.size.padding.top as c_int,
                    data.size.padding.bottom as c_int,
                    data.size.padding.left as c_int,
                    data.size.padding.right as c_int,
                )
            };
        }
    }
}

// fn fontTable(surface: *const Surface) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn fontTable(data: &SurfaceData) {
    // _ = cimgui.c.ImGui_BeginTable(
    //     "table_font",
    //     2,
    //     cimgui.c.ImGuiTableFlags_None,
    // );
    let _ = unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"table_font".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    };
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // {
    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Size (Points)");
            unsafe { dcimgui_sys::ImGui_Text(c"Size (Points)".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text(
            //     "%.2f pt",
            //     surface.font_size.points,
            // );
            unsafe { dcimgui_sys::ImGui_Text(c"%.2f pt".as_ptr(), data.font_size_points as f64) };
        }
    }

    // {
    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Size (Pixels)");
            unsafe { dcimgui_sys::ImGui_Text(c"Size (Pixels)".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text(
            //     "%.2f px",
            //     surface.font_size.pixels(),
            // );
            unsafe { dcimgui_sys::ImGui_Text(c"%.2f px".as_ptr(), data.font_size_pixels as f64) };
        }
    }
}

/// `input.MouseButton` label hand-mapping (mouseTable's click-state row; no
/// `@tagName` for this one — it's a short display abbreviation, not the enum's
/// field name).
fn mouse_button_label(button: Button) -> &'static CStr {
    match button {
        Button::Unknown => c"?",
        Button::Left => c"L",
        Button::Middle => c"M",
        Button::Right => c"R",
        Button::Four => c"{4}",
        Button::Five => c"{5}",
        Button::Six => c"{6}",
        Button::Seven => c"{7}",
        Button::Eight => c"{8}",
        Button::Nine => c"{9}",
        Button::Ten => c"{10}",
        Button::Eleven => c"{11}",
    }
}

/// `@enumFromInt(i)` hand-mapping (§3) — `Button`'s discriminants are dense
/// 0..=11 (`input/mouse.rs`), so the click-state array index recovers the
/// `Button` directly.
fn button_from_click_state_index(i: usize) -> Button {
    match i {
        0 => Button::Unknown,
        1 => Button::Left,
        2 => Button::Right,
        3 => Button::Middle,
        4 => Button::Four,
        5 => Button::Five,
        6 => Button::Six,
        7 => Button::Seven,
        8 => Button::Eight,
        9 => Button::Nine,
        10 => Button::Ten,
        _ => Button::Eleven,
    }
}

// fn mouseTable(
//     surface: *const Surface,
//     mouse: Mouse,
// ) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn mouseTable(data: &SurfaceData, mouse: Mouse) {
    // _ = cimgui.c.ImGui_BeginTable(
    //     "table_mouse",
    //     2,
    //     cimgui.c.ImGuiTableFlags_None,
    // );
    let _ = unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"table_mouse".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    };
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // const surface_mouse = &surface.mouse;
    // const t = surface.renderer_state.terminal;
    let t: *mut Terminal = unsafe { (*data.renderer_state).terminal };

    // {
    {
        // // R3 (PROVEN, mirihar-inspector-pin): `mouse.last_point` is a stale-safe
        // // weak Pin. It is fed to `pointFromPin` (address-only walk over live
        // // nodes, null when it no longer fits) and NEVER dereferenced directly —
        // // so a reflow between the mouse-move and this draw yields `.{}` (origin),
        // // not a use-after-free.
        // const hover_point: terminal.point.Coordinate = pt: {
        //     const p = mouse.last_point orelse break :pt .{};
        //     const pt = t.screens.active.pages.pointFromPin(
        //         .active,
        //         p,
        //     ) orelse break :pt .{};
        //     break :pt pt.coord();
        // };
        let hover_point: PointCoordinate = 'pt: {
            let p = match mouse.last_point {
                Some(p) => p,
                None => break 'pt PointCoordinate { x: 0, y: 0 },
            };
            let pages = unsafe { &(*(*t).screens.active).pages };
            match pages.point_from_pin(PointTag::Active, p) {
                Some(pt) => pt.coord(),
                None => PointCoordinate { x: 0, y: 0 },
            }
        };

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Hover Grid");
            unsafe { dcimgui_sys::ImGui_Text(c"Hover Grid".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text(
            //     "row=%d, col=%d",
            //     hover_point.y,
            //     hover_point.x,
            // );
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"row=%d, col=%d".as_ptr(),
                    hover_point.y as c_int,
                    hover_point.x as c_int,
                )
            };
        }
    }

    // {
    {
        // const coord: renderer.Coordinate.Terminal = (renderer.Coordinate{
        //     .surface = .{
        //         .x = mouse.last_xpos,
        //         .y = mouse.last_ypos,
        //     },
        // }).convert(.terminal, surface.size).terminal;
        let coord = match (RenderCoordinate::Surface(CoordinateSurface {
            x: mouse.last_xpos,
            y: mouse.last_ypos,
        }))
        .convert(CoordinateTag::Terminal, data.size)
        {
            RenderCoordinate::Terminal(t) => t,
            _ => unreachable!(),
        };

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Hover Point");
            unsafe { dcimgui_sys::ImGui_Text(c"Hover Point".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text(
            //     "(%dpx, %dpx)",
            //     @as(i64, @intFromFloat(coord.x)),
            //     @as(i64, @intFromFloat(coord.y)),
            // );
            unsafe {
                dcimgui_sys::ImGui_Text(c"(%dpx, %dpx)".as_ptr(), coord.x as i64, coord.y as i64)
            };
        }
    }

    // const any_click = for (surface_mouse.click_state) |state| {
    //     if (state == .press) break true;
    // } else false;
    let any_click = data.mouse_click_state.contains(&ButtonState::Press);

    // click: {
    'click: {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Click State");
            unsafe { dcimgui_sys::ImGui_Text(c"Click State".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // if (!any_click) {
            //     cimgui.c.ImGui_Text("none");
            //     break :click;
            // }
            if !any_click {
                unsafe { dcimgui_sys::ImGui_Text(c"none".as_ptr()) };
                break 'click;
            }

            // for (surface_mouse.click_state, 0..) |state, i| {
            //     if (state != .press) continue;
            //     const button: input.MouseButton = @enumFromInt(i);
            //     cimgui.c.ImGui_SameLine();
            //     cimgui.c.ImGui_Text("%s", (switch (button) {...}).ptr);
            // }
            for (i, &state) in data.mouse_click_state.iter().enumerate() {
                if state != ButtonState::Press {
                    continue;
                }
                let button = button_from_click_state_index(i);
                unsafe { dcimgui_sys::ImGui_SameLine() };
                unsafe {
                    dcimgui_sys::ImGui_Text(c"%s".as_ptr(), mouse_button_label(button).as_ptr())
                };
            }
        }
    }

    // {
    {
        // const left_click_point: terminal.point.Coordinate = pt: {
        //     const p = surface_mouse.selection_gesture.validatedLeftClickPin(&t.screens) orelse
        //         break :pt .{};
        //     const pt = t.screens.active.pages.pointFromPin(
        //         .active,
        //         p.*,
        //     ) orelse break :pt .{};
        //     break :pt pt.coord();
        // };
        let left_click_point: PointCoordinate = 'pt: {
            let screens = unsafe { &(*t).screens };
            let p = match unsafe { &*data.selection_gesture }.validated_left_click_pin(screens) {
                Some(p) => p,
                None => break 'pt PointCoordinate { x: 0, y: 0 },
            };
            let pages = unsafe { &(*(*t).screens.active).pages };
            match pages.point_from_pin(PointTag::Active, unsafe { *p }) {
                Some(pt) => pt.coord(),
                None => PointCoordinate { x: 0, y: 0 },
            }
        };

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Click Grid");
            unsafe { dcimgui_sys::ImGui_Text(c"Click Grid".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text(
            //     "row=%d, col=%d",
            //     left_click_point.y,
            //     left_click_point.x,
            // );
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"row=%d, col=%d".as_ptr(),
                    left_click_point.y as c_int,
                    left_click_point.x as c_int,
                )
            };
        }
    }

    // {
    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Click Point");
            unsafe { dcimgui_sys::ImGui_Text(c"Click Point".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text(
            //     "(%dpx, %dpx)",
            //     @as(u32, @intFromFloat(surface_mouse.selection_gesture.left_click_xpos)),
            //     @as(u32, @intFromFloat(surface_mouse.selection_gesture.left_click_ypos)),
            // );
            let (left_click_xpos, left_click_ypos) = unsafe {
                (
                    (*data.selection_gesture).left_click_xpos,
                    (*data.selection_gesture).left_click_ypos,
                )
            };
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"(%dpx, %dpx)".as_ptr(),
                    left_click_xpos as u32 as c_int,
                    left_click_ypos as u32 as c_int,
                )
            };
        }
    }
}
