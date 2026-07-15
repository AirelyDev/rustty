//! Surface-information inspector widget + the top-level surface `Inspector`.
//!
//! PORTS: ghostty src/inspector/widgets/surface.zig
//!
//! PORT STATUS (Task 15): partial — the two pieces that do NOT touch the cyclic
//! core `Surface` edge are LIVE:
//!
//!   * the window-name constants (`window_imgui_demo` … `window_renderer`), and
//!   * **`Mouse`** — the persisted inspector mouse state whose `last_point:
//!     Option<Pin>` is the **R3 hazard field** (a raw, non-owning `Pin` into
//!     PageList memory that a reflow invalidates). Its SB/TB-clean handling is
//!     PROVEN on the isolated `.port/phase-2/mirihar-inspector-pin` Miri harness
//!     (never hand-reasoned, R5): the stored `Pin` is a weak handle, re-resolved
//!     by the address-only `PageList::point_from_pin` walk (which never
//!     dereferences the pin's own node), `None` on failure — never a live borrow.
//!
//! The rest of the file — the top-level `Inspector` aggregate (`surface_info` /
//! `key_stream` / `terminal_info` / `vt_stream` / `renderer_info` / the
//! demo-window flag) with `init`/`deinit`/`draw`/`createDockSpace`, the surface
//! `Info` widget with its `draw`, and the `dimensionsTable`/`fontTable`/
//! `mouseTable` free functions — is carried as the preserved-verbatim reference
//! body (R1) in the DEFERRED block below, and inhabits in place (the
//! placeholder-leaf grows-in-place doctrine, the exact shape `terminal.rs` /
//! `screen.rs` use for their own deferred aggregates) once its blockers land:
//!
//!   1. **The core `Surface` edge (R4 acyclicity).** `Inspector.draw` / `Info.draw`
//!      / the three tables all take `*const Surface` (LIFETIMES row: `Inspector.
//!      draw.surface`, BORROW_PARAM — every terminal read under
//!      `surface.renderer_state.mutex`, INV-1). `Surface` lives in rustty-core,
//!      which ALREADY depends on rustty-inspector (`Option<*mut inspectorpkg::
//!      Inspector>`), so a direct `rustty_core::Surface` name here would CYCLE.
//!      The edge is crossed by a renderer/core-defined **accessor trait** impl'd
//!      for the inspector (the `RendererConfig`/`InspectorOverlay` precedent) —
//!      that Surface-accessor seam is defined in Task 6/17, not here.
//!   2. **`widgets::termio::Stream`** (the `vt_stream` field + `Terminal IO`
//!      window) is Task 16 — not yet created.
//!   3. **`widgets::terminal::Info`** (the `terminal_info` field + `Terminal`
//!      window) is `terminal.rs`'s own DEFERRED aggregate (gated on
//!      `widgets::screen::Info`).
//!   4. **`createDockSpace`** needs the same three unbound dcimgui-sys DockBuilder
//!      symbols `terminal.rs` names — `ImGui_DockBuilderGetNode`,
//!      `ImGui_DockBuilderAddNodeEx`, and the private
//!      `ImGuiDockNodeFlagsPrivate.DockSpace` flag (only `SplitNode`/`DockWindow`/
//!      `Finish` are wired today).
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
// button labels) is in rustty-input; `renderer.Coordinate` (mouseTable's px
// conversion) in rustty-renderer; `Surface`/`inspector`/`widgets.{terminal,
// termio,key,renderer}` are the DEFERRED-block dependencies noted in the module
// doc. The LIVE surface below (`Mouse` + constants) needs only `Pin`.
use core::ffi::CStr;

use rustty_vt::pagelist::Pin;

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
/// **weak/raw handle, never a live borrow**: `mouseTable` (in the DEFERRED block)
/// re-resolves it null-guarded via `pages.point_from_pin(.active, p)`
/// (`pagelist/pin.rs:121`), which locates the node **by address comparison only**
/// (`core::ptr::eq`, never dereferencing `p.node`) and returns `None` when the pin
/// no longer fits — SB/TB-clean across the reflow. The `Copy`/no-`Drop` derive is
/// load-bearing: storing/dropping a stale `Mouse` never touches `*last_point.node`.
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

// ===========================================================================
// DEFERRED (Task 15 blockers — preserved-verbatim reference body, R1). Inhabits
// in place once the core `Surface` accessor-trait seam (Task 6/17), `widgets::
// termio::Stream` (Task 16), `widgets::terminal::Info`, and the three unbound
// dcimgui-sys DockBuilder symbols land. See the module doc for the full blocker
// list. The bodies below are the faithful `surface.zig` source (lines 21-504).
// ===========================================================================
//
// pub const Inspector = struct {
//     /// Internal GUI state
//     surface_info: Info,
//     key_stream: widgets.key.Stream,
//     terminal_info: widgets.terminal.Info,
//     vt_stream: widgets.termio.Stream,
//     renderer_info: widgets.renderer.Info,
//     show_demo_window: bool,
//
//     pub fn init(alloc: Allocator) !Inspector {
//         return .{
//             .surface_info = .empty,
//             .key_stream = try .init(alloc),
//             .terminal_info = .empty,
//             .vt_stream = try .init(alloc),
//             .renderer_info = .empty,
//             .show_demo_window = true,
//         };
//     }
//
//     pub fn deinit(self: *Inspector, alloc: Allocator) void {
//         self.key_stream.deinit(alloc);
//         self.vt_stream.deinit(alloc);
//         self.renderer_info.deinit(alloc);
//     }
//
//     pub fn draw(
//         self: *Inspector,
//         surface: *const Surface,
//         mouse: Mouse,
//     ) void {
//         // Create our dockspace first. If we had to setup our dockspace,
//         // then it is a first render.
//         const dockspace_id = cimgui.c.ImGui_GetID("Main Dockspace");
//         const first_render = createDockSpace(dockspace_id);
//
//         // Draw everything that requires the terminal state mutex.
//         {
//             surface.renderer_state.mutex.lock();
//             defer surface.renderer_state.mutex.unlock();
//             const t = surface.renderer_state.terminal;
//
//             // Terminal info window
//             {
//                 const open = cimgui.c.ImGui_Begin(
//                     window_terminal,
//                     null,
//                     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
//                 );
//                 defer cimgui.c.ImGui_End();
//                 self.terminal_info.draw(open, t);
//             }
//
//             // Surface info window
//             {
//                 const open = cimgui.c.ImGui_Begin(
//                     window_surface,
//                     null,
//                     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
//                 );
//                 defer cimgui.c.ImGui_End();
//                 self.surface_info.draw(
//                     open,
//                     surface,
//                     mouse,
//                 );
//             }
//
//             // Keyboard info window
//             {
//                 const open = cimgui.c.ImGui_Begin(
//                     window_keyboard,
//                     null,
//                     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
//                 );
//                 defer cimgui.c.ImGui_End();
//                 self.key_stream.draw(
//                     open,
//                     surface.alloc,
//                 );
//             }
//
//             // Terminal IO window
//             {
//                 const open = cimgui.c.ImGui_Begin(
//                     window_termio,
//                     null,
//                     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
//                 );
//                 defer cimgui.c.ImGui_End();
//                 if (open) {
//                     self.vt_stream.draw(
//                         surface.alloc,
//                         &t.colors.palette.current,
//                     );
//                 }
//             }
//
//             // Renderer info window
//             {
//                 const open = cimgui.c.ImGui_Begin(
//                     window_renderer,
//                     null,
//                     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
//                 );
//                 defer cimgui.c.ImGui_End();
//                 self.renderer_info.draw(
//                     surface.alloc,
//                     open,
//                 );
//             }
//         }
//
//         // In debug we show the ImGui demo window so we can easily view
//         // available widgets and such.
//         if (comptime builtin.mode == .Debug) {
//             if (self.show_demo_window) {
//                 cimgui.c.ImGui_ShowDemoWindow(&self.show_demo_window);
//             }
//         }
//
//         if (first_render) {
//             // On first render, setup our initial focus state. We only
//             // do this on first render so that we can let the user change
//             // focus afterward without it snapping back.
//             cimgui.c.ImGui_SetWindowFocusStr(window_terminal);
//         }
//     }
//
//     /// Create the global dock space for the inspector. A dock space
//     /// is a special area where windows can be docked into. The global
//     /// dock space fills the entire main viewport.
//     ///
//     /// Returns true if this was the first time the dock space was created.
//     fn createDockSpace(dockspace_id: cimgui.c.ImGuiID) bool {
//         const viewport: *cimgui.c.ImGuiViewport = cimgui.c.ImGui_GetMainViewport();
//
//         // Initial Docking setup
//         const setup = cimgui.ImGui_DockBuilderGetNode(dockspace_id) == null;
//         if (setup) {
//             // Register our dockspace node
//             assert(cimgui.ImGui_DockBuilderAddNodeEx(
//                 dockspace_id,
//                 cimgui.ImGuiDockNodeFlagsPrivate.DockSpace,
//             ) == dockspace_id);
//
//             // Ensure it is the full size of the viewport
//             cimgui.ImGui_DockBuilderSetNodeSize(
//                 dockspace_id,
//                 viewport.Size,
//             );
//
//             // We only initialize one central docking point now but
//             // this is the point we'd pre-split and so on for the initial
//             // layout.
//             const dock_id_main: cimgui.c.ImGuiID = dockspace_id;
//             cimgui.ImGui_DockBuilderDockWindow(window_imgui_demo, dock_id_main);
//             cimgui.ImGui_DockBuilderDockWindow(window_terminal, dock_id_main);
//             cimgui.ImGui_DockBuilderDockWindow(window_surface, dock_id_main);
//             cimgui.ImGui_DockBuilderDockWindow(window_keyboard, dock_id_main);
//             cimgui.ImGui_DockBuilderDockWindow(window_termio, dock_id_main);
//             cimgui.ImGui_DockBuilderDockWindow(window_renderer, dock_id_main);
//             cimgui.ImGui_DockBuilderFinish(dockspace_id);
//         }
//
//         // Put the dockspace over the viewport.
//         assert(cimgui.c.ImGui_DockSpaceOverViewportEx(
//             dockspace_id,
//             viewport,
//             cimgui.c.ImGuiDockNodeFlags_PassthruCentralNode,
//             null,
//         ) == dockspace_id);
//         return setup;
//     }
// };
//
// (Mouse is ported LIVE above.)
//
// /// Surface information inspector widget.
// pub const Info = struct {
//     pub const empty: Info = .{};
//
//     /// Draw the surface info window.
//     pub fn draw(
//         self: *Info,
//         open: bool,
//         surface: *const Surface,
//         mouse: Mouse,
//     ) void {
//         _ = self;
//         if (!open) return;
//
//         if (cimgui.c.ImGui_CollapsingHeader(
//             "Help",
//             cimgui.c.ImGuiTreeNodeFlags_None,
//         )) {
//             cimgui.c.ImGui_TextWrapped(
//                 "This window displays information about the surface (window). " ++
//                     "A surface is the graphical area that displays the terminal " ++
//                     "content. It includes dimensions, font sizing, and mouse state " ++
//                     "information specific to this window instance.",
//             );
//         }
//
//         cimgui.c.ImGui_SeparatorText("Dimensions");
//         dimensionsTable(surface);
//
//         cimgui.c.ImGui_SeparatorText("Font");
//         fontTable(surface);
//
//         cimgui.c.ImGui_SeparatorText("Mouse");
//         mouseTable(surface, mouse);
//     }
// };
//
// fn dimensionsTable(surface: *const Surface) void {
//     _ = cimgui.c.ImGui_BeginTable(
//         "table_size",
//         2,
//         cimgui.c.ImGuiTableFlags_None,
//     );
//     defer cimgui.c.ImGui_EndTable();
//
//     // Screen Size
//     {
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Screen Size");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             cimgui.c.ImGui_Text(
//                 "%dpx x %dpx",
//                 surface.size.screen.width,
//                 surface.size.screen.height,
//             );
//         }
//     }
//
//     // Grid Size
//     {
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Grid Size");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             const grid_size = surface.size.grid();
//             cimgui.c.ImGui_Text(
//                 "%dc x %dr",
//                 grid_size.columns,
//                 grid_size.rows,
//             );
//         }
//     }
//
//     // Cell Size
//     {
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Cell Size");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             cimgui.c.ImGui_Text(
//                 "%dpx x %dpx",
//                 surface.size.cell.width,
//                 surface.size.cell.height,
//             );
//         }
//     }
//
//     // Padding
//     {
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Window Padding");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             cimgui.c.ImGui_Text(
//                 "T=%d B=%d L=%d R=%d px",
//                 surface.size.padding.top,
//                 surface.size.padding.bottom,
//                 surface.size.padding.left,
//                 surface.size.padding.right,
//             );
//         }
//     }
// }
//
// fn fontTable(surface: *const Surface) void {
//     _ = cimgui.c.ImGui_BeginTable(
//         "table_font",
//         2,
//         cimgui.c.ImGuiTableFlags_None,
//     );
//     defer cimgui.c.ImGui_EndTable();
//
//     {
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Size (Points)");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             cimgui.c.ImGui_Text(
//                 "%.2f pt",
//                 surface.font_size.points,
//             );
//         }
//     }
//
//     {
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Size (Pixels)");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             cimgui.c.ImGui_Text(
//                 "%.2f px",
//                 surface.font_size.pixels(),
//             );
//         }
//     }
// }
//
// fn mouseTable(
//     surface: *const Surface,
//     mouse: Mouse,
// ) void {
//     _ = cimgui.c.ImGui_BeginTable(
//         "table_mouse",
//         2,
//         cimgui.c.ImGuiTableFlags_None,
//     );
//     defer cimgui.c.ImGui_EndTable();
//
//     const surface_mouse = &surface.mouse;
//     const t = surface.renderer_state.terminal;
//
//     {
//         // R3 (PROVEN, mirihar-inspector-pin): `mouse.last_point` is a stale-safe
//         // weak Pin. It is fed to `pointFromPin` (address-only walk over live
//         // nodes, null when it no longer fits) and NEVER dereferenced directly —
//         // so a reflow between the mouse-move and this draw yields `.{}` (origin),
//         // not a use-after-free.
//         const hover_point: terminal.point.Coordinate = pt: {
//             const p = mouse.last_point orelse break :pt .{};
//             const pt = t.screens.active.pages.pointFromPin(
//                 .active,
//                 p,
//             ) orelse break :pt .{};
//             break :pt pt.coord();
//         };
//
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Hover Grid");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             cimgui.c.ImGui_Text(
//                 "row=%d, col=%d",
//                 hover_point.y,
//                 hover_point.x,
//             );
//         }
//     }
//
//     {
//         const coord: renderer.Coordinate.Terminal = (renderer.Coordinate{
//             .surface = .{
//                 .x = mouse.last_xpos,
//                 .y = mouse.last_ypos,
//             },
//         }).convert(.terminal, surface.size).terminal;
//
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Hover Point");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             cimgui.c.ImGui_Text(
//                 "(%dpx, %dpx)",
//                 @as(i64, @intFromFloat(coord.x)),
//                 @as(i64, @intFromFloat(coord.y)),
//             );
//         }
//     }
//
//     const any_click = for (surface_mouse.click_state) |state| {
//         if (state == .press) break true;
//     } else false;
//
//     click: {
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Click State");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             if (!any_click) {
//                 cimgui.c.ImGui_Text("none");
//                 break :click;
//             }
//
//             for (surface_mouse.click_state, 0..) |state, i| {
//                 if (state != .press) continue;
//                 const button: input.MouseButton = @enumFromInt(i);
//                 cimgui.c.ImGui_SameLine();
//                 cimgui.c.ImGui_Text("%s", (switch (button) {
//                     .unknown => "?",
//                     .left => "L",
//                     .middle => "M",
//                     .right => "R",
//                     .four => "{4}",
//                     .five => "{5}",
//                     .six => "{6}",
//                     .seven => "{7}",
//                     .eight => "{8}",
//                     .nine => "{9}",
//                     .ten => "{10}",
//                     .eleven => "{11}",
//                 }).ptr);
//             }
//         }
//     }
//
//     {
//         const left_click_point: terminal.point.Coordinate = pt: {
//             const p = surface_mouse.selection_gesture.validatedLeftClickPin(&t.screens) orelse
//                 break :pt .{};
//             const pt = t.screens.active.pages.pointFromPin(
//                 .active,
//                 p.*,
//             ) orelse break :pt .{};
//             break :pt pt.coord();
//         };
//
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Click Grid");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             cimgui.c.ImGui_Text(
//                 "row=%d, col=%d",
//                 left_click_point.y,
//                 left_click_point.x,
//             );
//         }
//     }
//
//     {
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("Click Point");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             cimgui.c.ImGui_Text(
//                 "(%dpx, %dpx)",
//                 @as(u32, @intFromFloat(surface_mouse.selection_gesture.left_click_xpos)),
//                 @as(u32, @intFromFloat(surface_mouse.selection_gesture.left_click_ypos)),
//             );
//         }
//     }
// }
