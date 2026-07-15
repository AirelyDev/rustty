//! Inspector widgets module hub + shared ImGui helpers.
//!
//! PORTS: ghostty src/inspector/widgets.zig
//!
//! PORT STATUS: the module wiring + the shared ImGui helpers (`helpMarker`,
//! `DetachableHeader`, `DetachableHeaderState`, `detachableHeader`) are LIVE
//! (Task 7). The remaining widget sub-modules fill in as their own Phase-3
//! tasks land, in place.

// const cimgui = @import("dcimgui");
use core::ffi::CStr;

// pub const page = @import("widgets/page.zig");
//
// PORT NOTE (Task 12): the page-information widget is fully LIVE — pure
// display-only reads of a `terminal.Page` under the surface mutex (INV-1); the
// dirty reads (`page.isDirty()` / `row.dirty`) are pure observation (INV-2).
pub mod page;
// pub const pagelist = @import("widgets/pagelist.zig");
//
// PORT NOTE (Task 13): the PageList inspector widget (`Inspector` /
// `CellChooser` / `CellInfo` + the summary/scrollbar/regions/tracked-pins/pages
// tables) is fully LIVE — display-only reads of a `terminal.PageList` under the
// surface mutex (INV-1); the `pin.isDirty()` read is pure observation (INV-2).
// The §3 comptime obligations (`std.meta.tags(point.Tag)`, the `combo_width`
// comptime block, `regionHelpText`/`regionRow` per-tag) are hand-expanded in the
// file. `screen.rs`'s `Info` embeds `pagelist.{Inspector, CellChooser}` when
// Task 15/17 folds the aggregate together.
pub mod pagelist;
// pub const key = @import("widgets/key.zig");
pub mod key;
// pub const renderer = @import("widgets/renderer.zig");
//
// PORT NOTE (Task 5): the R2 acyclic accessor-trait half of renderer.zig is
// LIVE — `Info` + `overlayFeatures` (impl'd as the renderer-owned
// `InspectorOverlay` trait, INV-7). The ImGui `draw` half is Task 10.
pub mod renderer;
// pub const screen = @import("widgets/screen.zig");
//
// PORT NOTE (Task 11): the six free-function tables (cursorTable / cursorStyle
// / keyboardTable / kittyGraphicsTable / internalStateTable /
// semanticPromptTable — the S6 mechanical seam) are LIVE. The `Info` aggregate
// + its `draw`/`createDockSpace` embed `widgets.pagelist.{Inspector,
// CellChooser}` and inhabit when pagelist (Task 13) lands (grows-in-place).
pub mod screen;
// pub const style = @import("widgets/style.zig");
pub mod style;
// pub const surface = @import("widgets/surface.zig");      (Phase 3)
// pub const terminal = @import("widgets/terminal.zig");
//
// PORT NOTE (Task 14): the seven free-function tables (`miscTable`,
// `layoutTable` [INV-3 scrolling_region write-back], `mouseTable`, `colorTable`,
// `modesTable` [INV-4 comptime modes hand-expansion], `dynamicRGB`, `palette`)
// are LIVE. The `Info` aggregate + `ScreenMap` + `draw`/`drawOpen`/`screensTable`
// name `widgets.screen.Info` (the deferred Task-11 aggregate, itself gated on
// unbound dcimgui-sys DockBuilder symbols) and are carried as the verbatim
// reference body; they inhabit in place once `screen::Info` lands
// (grows-in-place, the screen.rs precedent).
pub mod terminal;
// pub const termio = @import("widgets/termio.zig");        (Phase 3)

/// Minimal `defer`-style scope guard — runs `f` on drop. Mirrors Zig's `defer`
/// for the ImGui `PushID`/`PopID`, `Begin`/`End`, and `PushStyleVar`/`PopStyleVar`
/// pairings below, including firing on the early `return` paths the reference
/// relies on. panic=abort means no unwinding ever crosses these FFI calls, so
/// drop-on-scope-exit is the sole runtime path (no double-fire, no leak).
struct Defer<F: FnMut()>(F);

impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

/// Draws a "(?)" disabled text marker that shows some help text
/// on hover.
//
// pub fn helpMarker(text: [:0]const u8) void {
//
// PORT NOTE: the `[:0]const u8` (null-terminated byte slice) crosses as
// `&CStr`; `.as_ptr()` yields the `*const c_char` the ImGui text calls want.
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn helpMarker(text: &CStr) {
    // cimgui.c.ImGui_TextDisabled("(?)");
    unsafe { dcimgui_sys::ImGui_TextDisabled(c"(?)".as_ptr()) };

    // if (!cimgui.c.ImGui_BeginItemTooltip()) return;
    if !unsafe { dcimgui_sys::ImGui_BeginItemTooltip() } {
        return;
    }
    // defer cimgui.c.ImGui_EndTooltip();
    let _end_tooltip = Defer(|| unsafe { dcimgui_sys::ImGui_EndTooltip() });

    // cimgui.c.ImGui_PushTextWrapPos(cimgui.c.ImGui_GetFontSize() * 35.0);
    unsafe { dcimgui_sys::ImGui_PushTextWrapPos(dcimgui_sys::ImGui_GetFontSize() * 35.0) };
    // defer cimgui.c.ImGui_PopTextWrapPos();
    let _pop_wrap = Defer(|| unsafe { dcimgui_sys::ImGui_PopTextWrapPos() });

    // cimgui.c.ImGui_TextUnformatted(text.ptr);
    unsafe { dcimgui_sys::ImGui_TextUnformatted(text.as_ptr()) };
}

/// DetachableHeader allows rendering a collapsing header that can be
/// detached into its own window.
pub struct DetachableHeader {
    /// Set whether the window is detached.
    pub detached: bool,

    /// If true, detaching will move the item into a docking position
    /// to the right.
    pub dock: bool,

    // Internal state do not touch.
    window_first: bool,
}

impl Default for DetachableHeader {
    // pub const DetachableHeader = struct {
    //     detached: bool = false,
    //     dock: bool = true,
    //     window_first: bool = true,
    // };
    fn default() -> DetachableHeader {
        DetachableHeader {
            detached: false,
            dock: true,
            window_first: true,
        }
    }
}

impl DetachableHeader {
    // pub fn windowEnd(self: *DetachableHeader) void {
    #[allow(non_snake_case)] // faithful Zig name (R1)
    pub fn windowEnd(&mut self) {
        // _ = self;

        // If we started the window, we need to end it.
        // cimgui.c.ImGui_End();
        unsafe { dcimgui_sys::ImGui_End() };
    }

    /// Returns null if there is no window created (not detached).
    /// Otherwise returns whether the window is open.
    //
    // pub fn window(self: *DetachableHeader, label: [:0]const u8) ?bool {
    pub fn window(&mut self, label: &CStr) -> Option<bool> {
        // If we're not detached, we don't create a window.
        // if (!self.detached) {
        if !self.detached {
            self.window_first = true;
            return None;
        }

        // If this is our first time showing the window then we need to
        // setup docking. We only do this on the first time because we
        // don't want to reset a user's docking behavior later.
        // if (self.window_first) dock: {
        if self.window_first {
            self.window_first = false;
            'dock: {
                // if (!self.dock) break :dock;
                if !self.dock {
                    break 'dock;
                }
                // const dock_id = cimgui.c.ImGui_GetWindowDockID();
                let dock_id = unsafe { dcimgui_sys::ImGui_GetWindowDockID() };
                // if (dock_id == 0) break :dock;
                if dock_id == 0 {
                    break 'dock;
                }
                // var dock_id_right: cimgui.c.ImGuiID = 0;
                // var dock_id_left: cimgui.c.ImGuiID = 0;
                let mut dock_id_right: dcimgui_sys::ImGuiID = 0;
                let mut dock_id_left: dcimgui_sys::ImGuiID = 0;
                // _ = cimgui.ImGui_DockBuilderSplitNode(
                //     dock_id,
                //     cimgui.c.ImGuiDir_Right,
                //     0.4,
                //     &dock_id_right,
                //     &dock_id_left,
                // );
                let _ = unsafe {
                    dcimgui_sys::ImGui_DockBuilderSplitNode(
                        dock_id,
                        dcimgui_sys::ImGuiDir_Right as dcimgui_sys::ImGuiDir,
                        0.4,
                        &mut dock_id_right,
                        &mut dock_id_left,
                    )
                };
                // cimgui.ImGui_DockBuilderDockWindow(label, dock_id_right);
                unsafe { dcimgui_sys::ImGui_DockBuilderDockWindow(label.as_ptr(), dock_id_right) };
                // cimgui.ImGui_DockBuilderFinish(dock_id);
                unsafe { dcimgui_sys::ImGui_DockBuilderFinish(dock_id) };
            }
        }

        // return cimgui.c.ImGui_Begin(
        //     label,
        //     &self.detached,
        //     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
        // );
        Some(unsafe {
            dcimgui_sys::ImGui_Begin(
                label.as_ptr(),
                &mut self.detached,
                dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing
                    as dcimgui_sys::ImGuiWindowFlags,
            )
        })
    }

    // pub fn header(self: *DetachableHeader, label: [:0]const u8) bool {
    pub fn header(&mut self, label: &CStr) -> bool {
        // If we're detached, create a separate window.
        // if (self.detached) return false;
        if self.detached {
            return false;
        }

        // Make sure all headers have a unique ID in the stack. We only
        // need to do this for the header side because creating a window
        // automatically creates an ID.
        // cimgui.c.ImGui_PushID(label);
        unsafe { dcimgui_sys::ImGui_PushID(label.as_ptr()) };
        // defer cimgui.c.ImGui_PopID();
        let _pop_id = Defer(|| unsafe { dcimgui_sys::ImGui_PopID() });

        // Create the collapsing header with the pop out button overlaid.
        // cimgui.c.ImGui_SetNextItemAllowOverlap();
        unsafe { dcimgui_sys::ImGui_SetNextItemAllowOverlap() };
        // const is_open = cimgui.c.ImGui_CollapsingHeader(
        //     label,
        //     cimgui.c.ImGuiTreeNodeFlags_None,
        // );
        let is_open = unsafe {
            dcimgui_sys::ImGui_CollapsingHeader(
                label.as_ptr(),
                dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_None
                    as dcimgui_sys::ImGuiTreeNodeFlags,
            )
        };

        // Place pop-out button inside the header bar
        // const header_max = cimgui.c.ImGui_GetItemRectMax();
        // const header_min = cimgui.c.ImGui_GetItemRectMin();
        // const frame_height = cimgui.c.ImGui_GetFrameHeight();
        // const button_size = frame_height - 4;
        // const padding = 4;
        let header_max = unsafe { dcimgui_sys::ImGui_GetItemRectMax() };
        let header_min = unsafe { dcimgui_sys::ImGui_GetItemRectMin() };
        let frame_height = unsafe { dcimgui_sys::ImGui_GetFrameHeight() };
        let button_size = frame_height - 4.0;
        let padding = 4.0;

        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // cimgui.c.ImGui_SetCursorScreenPos(.{
        //     .x = header_max.x - button_size - padding,
        //     .y = header_min.y + 2,
        // });
        unsafe {
            dcimgui_sys::ImGui_SetCursorScreenPos(dcimgui_sys::ImVec2 {
                x: header_max.x - button_size - padding,
                y: header_min.y + 2.0,
            })
        };
        {
            // cimgui.c.ImGui_PushStyleVarImVec2(
            //     cimgui.c.ImGuiStyleVar_FramePadding,
            //     .{ .x = 0, .y = 0 },
            // );
            unsafe {
                dcimgui_sys::ImGui_PushStyleVarImVec2(
                    dcimgui_sys::ImGuiStyleVar__ImGuiStyleVar_FramePadding
                        as dcimgui_sys::ImGuiStyleVar,
                    dcimgui_sys::ImVec2 { x: 0.0, y: 0.0 },
                )
            };
            // defer cimgui.c.ImGui_PopStyleVar();
            let _pop_style = Defer(|| unsafe { dcimgui_sys::ImGui_PopStyleVar() });
            // if (cimgui.c.ImGui_ButtonEx(
            //     ">>##detach",
            //     .{ .x = button_size, .y = button_size },
            // )) {
            //     self.detached = true;
            // }
            if unsafe {
                dcimgui_sys::ImGui_ButtonEx(
                    c">>##detach".as_ptr(),
                    dcimgui_sys::ImVec2 {
                        x: button_size,
                        y: button_size,
                    },
                )
            } {
                self.detached = true;
            }
        }

        // if (cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_DelayShort)) {
        //     cimgui.c.ImGui_SetTooltip("Detach into separate window");
        // }
        if unsafe {
            dcimgui_sys::ImGui_IsItemHovered(
                dcimgui_sys::ImGuiHoveredFlags__ImGuiHoveredFlags_DelayShort
                    as dcimgui_sys::ImGuiHoveredFlags,
            )
        } {
            unsafe { dcimgui_sys::ImGui_SetTooltip(c"Detach into separate window".as_ptr()) };
        }

        // return is_open;
        is_open
    }
}

pub struct DetachableHeaderState {
    pub show_window: bool,

    /// Internal state. Don't touch.
    first_show: bool,
}

impl Default for DetachableHeaderState {
    // pub const DetachableHeaderState = struct {
    //     show_window: bool = false,
    //     first_show: bool = false,
    // };
    fn default() -> DetachableHeaderState {
        DetachableHeaderState {
            show_window: false,
            first_show: false,
        }
    }
}

/// Render a collapsing header that can be detached into its own window.
/// When detached, renders as a separate window with a close button.
/// When attached, renders as a collapsing header with a pop-out button.
//
// pub fn detachableHeader(
//     label: [:0]const u8,
//     state: *DetachableHeaderState,
//     ctx: anytype,
//     comptime contentFn: fn (@TypeOf(ctx)) void,
// ) void {
//
// PORT NOTE (§3 comptime callback): the `ctx: anytype` + `comptime contentFn:
// fn(@TypeOf(ctx)) void` pair — a compile-time monomorphized content callback —
// ports as a generic `Ctx` value plus an `impl FnOnce(Ctx)` closure (Rust
// monomorphizes it per call site, exactly as Zig's comptime instantiation does).
// `contentFn` is invoked at most once per call (the window-`Begin` branch and
// the `is_open` branch are mutually exclusive via the early `return`), so
// `FnOnce` — which moves `ctx` in — is the precise bound.
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn detachableHeader<Ctx>(
    label: &CStr,
    state: &mut DetachableHeaderState,
    ctx: Ctx,
    content_fn: impl FnOnce(Ctx),
) {
    // cimgui.c.ImGui_PushID(label);
    unsafe { dcimgui_sys::ImGui_PushID(label.as_ptr()) };
    // defer cimgui.c.ImGui_PopID();
    let _pop_id = Defer(|| unsafe { dcimgui_sys::ImGui_PopID() });

    // if (state.show_window) {
    if state.show_window {
        // On first show, dock this window to the right of the parent window's dock.
        // We only do this once so the user can freely reposition the window afterward
        // without it snapping back to the right on every frame.
        // if (!state.first_show) {
        if !state.first_show {
            state.first_show = true;
            // const current_dock_id = cimgui.c.ImGui_GetWindowDockID();
            let current_dock_id = unsafe { dcimgui_sys::ImGui_GetWindowDockID() };
            // if (current_dock_id != 0) {
            if current_dock_id != 0 {
                // var dock_id_right: cimgui.c.ImGuiID = 0;
                // var dock_id_left: cimgui.c.ImGuiID = 0;
                let mut dock_id_right: dcimgui_sys::ImGuiID = 0;
                let mut dock_id_left: dcimgui_sys::ImGuiID = 0;
                // _ = cimgui.ImGui_DockBuilderSplitNode(
                //     current_dock_id,
                //     cimgui.c.ImGuiDir_Right,
                //     0.3,
                //     &dock_id_right,
                //     &dock_id_left,
                // );
                let _ = unsafe {
                    dcimgui_sys::ImGui_DockBuilderSplitNode(
                        current_dock_id,
                        dcimgui_sys::ImGuiDir_Right as dcimgui_sys::ImGuiDir,
                        0.3,
                        &mut dock_id_right,
                        &mut dock_id_left,
                    )
                };
                // cimgui.ImGui_DockBuilderDockWindow(label, dock_id_right);
                unsafe {
                    dcimgui_sys::ImGui_DockBuilderDockWindow(label.as_ptr(), dock_id_right)
                };
                // cimgui.ImGui_DockBuilderFinish(current_dock_id);
                unsafe { dcimgui_sys::ImGui_DockBuilderFinish(current_dock_id) };
            }
        }

        // defer cimgui.c.ImGui_End();
        let _end = Defer(|| unsafe { dcimgui_sys::ImGui_End() });
        // if (cimgui.c.ImGui_Begin(
        //     label,
        //     &state.show_window,
        //     cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
        // )) contentFn(ctx);
        if unsafe {
            dcimgui_sys::ImGui_Begin(
                label.as_ptr(),
                &mut state.show_window,
                dcimgui_sys::ImGuiWindowFlags__ImGuiWindowFlags_NoFocusOnAppearing
                    as dcimgui_sys::ImGuiWindowFlags,
            )
        } {
            content_fn(ctx);
        }
        // return;
        return;
    }

    // Reset first_show when window is closed so next open docks again
    // state.first_show = false;
    state.first_show = false;

    // cimgui.c.ImGui_SetNextItemAllowOverlap();
    unsafe { dcimgui_sys::ImGui_SetNextItemAllowOverlap() };
    // const is_open = cimgui.c.ImGui_CollapsingHeader(
    //     label,
    //     cimgui.c.ImGuiTreeNodeFlags_None,
    // );
    let is_open = unsafe {
        dcimgui_sys::ImGui_CollapsingHeader(
            label.as_ptr(),
            dcimgui_sys::ImGuiTreeNodeFlags__ImGuiTreeNodeFlags_None
                as dcimgui_sys::ImGuiTreeNodeFlags,
        )
    };

    // Place pop-out button inside the header bar
    // const header_max = cimgui.c.ImGui_GetItemRectMax();
    // const header_min = cimgui.c.ImGui_GetItemRectMin();
    // const frame_height = cimgui.c.ImGui_GetFrameHeight();
    // const button_size = frame_height - 4;
    // const padding = 4;
    let header_max = unsafe { dcimgui_sys::ImGui_GetItemRectMax() };
    let header_min = unsafe { dcimgui_sys::ImGui_GetItemRectMin() };
    let frame_height = unsafe { dcimgui_sys::ImGui_GetFrameHeight() };
    let button_size = frame_height - 4.0;
    let padding = 4.0;

    // cimgui.c.ImGui_SameLine();
    unsafe { dcimgui_sys::ImGui_SameLine() };
    // cimgui.c.ImGui_SetCursorScreenPos(.{
    //     .x = header_max.x - button_size - padding,
    //     .y = header_min.y + 2,
    // });
    unsafe {
        dcimgui_sys::ImGui_SetCursorScreenPos(dcimgui_sys::ImVec2 {
            x: header_max.x - button_size - padding,
            y: header_min.y + 2.0,
        })
    };
    // cimgui.c.ImGui_PushStyleVarImVec2(
    //     cimgui.c.ImGuiStyleVar_FramePadding,
    //     .{ .x = 0, .y = 0 },
    // );
    unsafe {
        dcimgui_sys::ImGui_PushStyleVarImVec2(
            dcimgui_sys::ImGuiStyleVar__ImGuiStyleVar_FramePadding as dcimgui_sys::ImGuiStyleVar,
            dcimgui_sys::ImVec2 { x: 0.0, y: 0.0 },
        )
    };
    // if (cimgui.c.ImGui_ButtonEx(
    //     ">>##detach",
    //     .{ .x = button_size, .y = button_size },
    // )) {
    //     state.show_window = true;
    // }
    if unsafe {
        dcimgui_sys::ImGui_ButtonEx(
            c">>##detach".as_ptr(),
            dcimgui_sys::ImVec2 {
                x: button_size,
                y: button_size,
            },
        )
    } {
        state.show_window = true;
    }
    // cimgui.c.ImGui_PopStyleVar();
    unsafe { dcimgui_sys::ImGui_PopStyleVar() };
    // if (cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_DelayShort)) {
    //     cimgui.c.ImGui_SetTooltip("Pop out into separate window");
    // }
    if unsafe {
        dcimgui_sys::ImGui_IsItemHovered(
            dcimgui_sys::ImGuiHoveredFlags__ImGuiHoveredFlags_DelayShort
                as dcimgui_sys::ImGuiHoveredFlags,
        )
    } {
        unsafe { dcimgui_sys::ImGui_SetTooltip(c"Pop out into separate window".as_ptr()) };
    }

    // if (is_open) contentFn(ctx);
    if is_open {
        content_fn(ctx);
    }
}
