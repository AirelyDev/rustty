//! Terminal-information inspector widget.
//!
//! PORTS: ghostty src/inspector/widgets/terminal.zig
//!
//! PORT STATUS (Task 14): the seven free-function tables — `miscTable`,
//! `layoutTable`, `mouseTable`, `colorTable`, `modesTable`, `dynamicRGB`,
//! `palette` — are LIVE. They are display-only reads of a `terminal.Terminal`
//! obtained per-frame under the surface's `renderer_state.mutex` (INV-1). Two
//! of them carry the invariants this task centers on:
//!
//!   * `layoutTable` holds the **ONE** inspector -> live-terminal write-back
//!     (INV-3): `t.scrolling_region = override` guarded by `changed &&
//!     top<bottom && left<right`. EVERY other editable widget here (the
//!     `##focused` checkbox, the mode checkboxes, the `##override`/`##default`
//!     color edits) reads into a stack-local and DISCARDS the ImGui edit —
//!     display-only.
//!   * `modesTable` hand-expands the reference's `inline for
//!     (@typeInfo(terminal.Mode).@"enum".fields)` (§3) into the `MODES` table
//!     below (INV-4: `t.modes.get(mode)` is display-only).
//!
//! The `Info` aggregate struct + `ScreenMap = EnumMap(ScreenSet.Key,
//! screen.Info)` + its `draw`/`drawOpen`/`screensTable` are carried as the
//! preserved-verbatim reference body (R1) in the DEFERRED block below. Their
//! sole missing dependency is `widgets.screen.Info` — the Task-11 screen
//! aggregate, which is itself gated on three unbound dcimgui-sys DockBuilder
//! symbols (`ImGui_DockBuilderGetNode`, `ImGui_DockBuilderAddNodeEx`,
//! `ImGuiDockNodeFlagsPrivate::DockSpace`, used by `screen.Info.createDockSpace`
//! / `.draw`). Per the placeholder-leaf grows-in-place doctrine (the exact shape
//! `screen.rs` uses to carry ITS deferred `Info`), the `Info`/`ScreenMap`/screen
//! -window trio inhabits in place once `screen::Info` lands. The INV-6 per-frame
//! re-fetch/removal (`t.screens.get(entry.key) orelse remove`, `!screen_open =>
//! remove`) lives inside that deferred `draw` and is preserved verbatim.
//!
//! All bodies run on the main thread with the surface's ImGui context current
//! (INV-11).

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const cimgui = @import("dcimgui");
// const widgets = @import("../widgets.zig");
// const terminal = @import("../../terminal/main.zig");
// const modes = terminal.modes;
// const Terminal = terminal.Terminal;
//
// PORT NOTE (screen.rs/pagelist.rs precedent): `dcimgui` is the vendored Dear
// ImGui C API reached through `dcimgui-sys`. The `terminal.*` types live in
// rustty-vt: `terminal.Terminal`/`.ScrollingRegion`/`.MouseShiftCapture` in its
// `terminal` module, `terminal.Mode`/`modes.ModeTag` in `modes`, `terminal.
// color.DynamicRGB`/`.Palette`/`.RGB` in `color`, and `mouse.Event`/`.Format`/
// `.Shape` in `mouse`. `widgets.helpMarker` is the parent module's Task 7 helper.
use core::ffi::{c_char, c_int, c_uint, c_void, CStr};

use rustty_vt::color::{DynamicRGB, Palette};
use rustty_vt::modes::{Mode, ModeTag};
use rustty_vt::mouse::{Event as MouseEvent, Format as MouseFormat, Shape as MouseShape};
use rustty_vt::terminal::{MouseShiftCapture, ScrollingRegion, Terminal};

use super::helpMarker;

/// Minimal `defer`-style scope guard — runs `f` on drop (mirrors the private
/// `Defer` in `widgets.rs`/`screen.rs`). Backs the ImGui `BeginTable`/`EndTable`,
/// `PushID`/`PopID`, and `PushItemWidth`/`PopItemWidth` pairings below so they
/// fire on every exit path. panic=abort means no unwinding ever crosses these
/// FFI calls, so drop-on-scope-exit is the sole runtime path (no double-fire,
/// no leak).
struct Defer<F: FnMut()>(F);

impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

// ============================================================================
// PORT NOTE (Task 14 — DEFERRED aggregate, gated on `widgets::screen::Info`):
//
// The `Info` aggregate + `ScreenMap = std.EnumMap(terminal.ScreenSet.Key,
// widgets.screen.Info)` + its `draw`/`drawOpen` + the `screensTable` free
// function all name `widgets.screen.Info` (the ScreenMap VALUE type, and
// `entry.value.draw(...)`). `screen::Info` is the deferred Task-11 aggregate —
// NOT yet inhabited because its `createDockSpace`/`draw` require three
// dcimgui-sys DockBuilder symbols that are NOT bound today:
// `ImGui_DockBuilderGetNode`, `ImGui_DockBuilderAddNodeEx`, and the private
// `ImGuiDockNodeFlagsPrivate.DockSpace` flag (only `SplitNode`/`DockWindow`/
// `Finish` are wired in `crates-sys/dcimgui-sys/src/lib.rs`). So per the
// placeholder-leaf grows-in-place doctrine (the exact shape `screen.rs` uses to
// carry its own deferred `Info`), the `Info`/`ScreenMap`/`screensTable` trio is
// carried here as the verbatim reference body (R1) and inhabits in place once
// `screen::Info` lands. The INV-6 per-frame re-fetch/removal is inside `draw`.
//
// pub const Info = struct {
//     /// True if we're showing the 256-color palette window.
//     show_palette: bool,
//
//     /// The various detachable headers.
//     misc_header: widgets.DetachableHeader,
//     layout_header: widgets.DetachableHeader,
//     mouse_header: widgets.DetachableHeader,
//     color_header: widgets.DetachableHeader,
//     modes_header: widgets.DetachableHeader,
//
//     /// Screen detail windows for each screen key.
//     screens: ScreenMap,
//
//     pub const empty: Info = .{
//         .show_palette = false,
//         .misc_header = .{},
//         .layout_header = .{},
//         .mouse_header = .{},
//         .color_header = .{},
//         .modes_header = .{},
//         .screens = .{},
//     };
//
//     /// Draw the terminal info window.
//     pub fn draw(
//         self: *Info,
//         open: bool,
//         t: *Terminal,
//     ) void {
//         // Draw our open state if we're open.
//         if (open) self.drawOpen(t);
//
//         // Draw our detached state that draws regardless of if
//         // we're open or not.
//         if (self.misc_header.window("Terminal Misc")) |visible| {
//             defer self.misc_header.windowEnd();
//             if (visible) miscTable(t);
//         }
//         if (self.layout_header.window("Terminal Layout")) |visible| {
//             defer self.layout_header.windowEnd();
//             if (visible) layoutTable(t);
//         }
//         if (self.mouse_header.window("Terminal Mouse")) |visible| {
//             defer self.mouse_header.windowEnd();
//             if (visible) mouseTable(t);
//         }
//         if (self.color_header.window("Terminal Color")) |visible| {
//             defer self.color_header.windowEnd();
//             if (visible) colorTable(t, &self.show_palette);
//         }
//         if (self.modes_header.window("Terminal Modes")) |visible| {
//             defer self.modes_header.windowEnd();
//             if (visible) modesTable(t);
//         }
//
//         // Palette pop-out window
//         if (self.show_palette) {
//             defer cimgui.c.ImGui_End();
//             if (cimgui.c.ImGui_Begin(
//                 "256-Color Palette",
//                 &self.show_palette,
//                 cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
//             )) {
//                 palette("palette", &t.colors.palette.current);
//             }
//         }
//
//         // Screen pop-out windows
//         var it = self.screens.iterator();
//         while (it.next()) |entry| {
//             const screen = t.screens.get(entry.key) orelse {
//                 // Could happen if we opened up a window for a screen
//                 // and that screen was subsequently deinitialized. In
//                 // this case, hide the window.
//                 self.screens.remove(entry.key);
//                 continue;
//             };
//
//             var title_buf: [128]u8 = undefined;
//             const title = std.fmt.bufPrintZ(
//                 &title_buf,
//                 "Screen: {t}",
//                 .{entry.key},
//             ) catch "Screen";
//
//             // Setup our next window so it has some size to it.
//             const viewport = cimgui.c.ImGui_GetMainViewport();
//             cimgui.c.ImGui_SetNextWindowSize(
//                 .{
//                     .x = @min(400, viewport.*.Size.x),
//                     .y = @min(300, viewport.*.Size.y),
//                 },
//                 cimgui.c.ImGuiCond_FirstUseEver,
//             );
//
//             var screen_open: bool = true;
//             defer cimgui.c.ImGui_End();
//             const screen_draw = cimgui.c.ImGui_Begin(
//                 title,
//                 &screen_open,
//                 cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
//             );
//             entry.value.draw(screen_draw, .{
//                 .screen = screen,
//                 .key = entry.key,
//                 .active_key = t.screens.active_key,
//                 .modify_other_keys_2 = t.flags.modify_other_keys_2,
//                 .color_palette = &t.colors.palette,
//             });
//
//             // If the window was closed, remove it from our map so future
//             // renders don't draw it.
//             if (!screen_open) self.screens.remove(entry.key);
//         }
//     }
//
//     fn drawOpen(self: *Info, t: *Terminal) void {
//         // Show our screens up top.
//         screensTable(t, &self.screens);
//
//         if (self.misc_header.header("Misc")) miscTable(t);
//         if (self.layout_header.header("Layout")) layoutTable(t);
//         if (self.mouse_header.header("Mouse")) mouseTable(t);
//         if (self.color_header.header("Color")) colorTable(t, &self.show_palette);
//         if (self.modes_header.header("Modes")) modesTable(t);
//     }
// };
//
// pub const ScreenMap = std.EnumMap(
//     terminal.ScreenSet.Key,
//     widgets.screen.Info,
// );
//
// /// Render the table of possible screens with various actions.
// fn screensTable(
//     t: *Terminal,
//     map: *ScreenMap,
// ) void {
//     if (!cimgui.c.ImGui_BeginTable(
//         "screens",
//         3,
//         cimgui.c.ImGuiTableFlags_Borders |
//             cimgui.c.ImGuiTableFlags_RowBg |
//             cimgui.c.ImGuiTableFlags_SizingFixedFit,
//     )) return;
//     defer cimgui.c.ImGui_EndTable();
//
//     cimgui.c.ImGui_TableSetupColumn("Screen", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
//     cimgui.c.ImGui_TableSetupColumn("Status", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
//     cimgui.c.ImGui_TableSetupColumn("", cimgui.c.ImGuiTableColumnFlags_WidthFixed);
//
//     // Custom header row to include help marker before "Screen"
//     {
//         cimgui.c.ImGui_TableNextRowEx(cimgui.c.ImGuiTableRowFlags_Headers, 0.0);
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_PushStyleVarImVec2(cimgui.c.ImGuiStyleVar_FramePadding, .{ .x = 0, .y = 0 });
//             widgets.helpMarker(
//                 "A terminal can have multiple screens, only one of which is active at " ++
//                     "a time. Each screen has its own grid, contents, and other state. " ++
//                     "This section allows you to inspect the different screens managed by " ++
//                     "the terminal.",
//             );
//             cimgui.c.ImGui_PopStyleVar();
//             cimgui.c.ImGui_SameLineEx(0.0, cimgui.c.ImGui_GetStyle().*.ItemInnerSpacing.x);
//             cimgui.c.ImGui_TableHeader("Screen");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             cimgui.c.ImGui_TableHeader("Status");
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(2);
//             cimgui.c.ImGui_TableHeader("");
//         }
//     }
//
//     for (std.meta.tags(terminal.ScreenSet.Key)) |key| {
//         const is_initialized = t.screens.get(key) != null;
//         const is_active = t.screens.active_key == key;
//
//         cimgui.c.ImGui_TableNextRow();
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             cimgui.c.ImGui_Text("%s", @tagName(key).ptr);
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             if (is_active) {
//                 cimgui.c.ImGui_TextColored(
//                     .{ .x = 0.4, .y = 1.0, .z = 0.4, .w = 1.0 },
//                     "active",
//                 );
//             } else if (is_initialized) {
//                 cimgui.c.ImGui_TextColored(
//                     .{ .x = 0.6, .y = 0.6, .z = 0.6, .w = 1.0 },
//                     "initialized",
//                 );
//             } else {
//                 cimgui.c.ImGui_TextColored(
//                     .{ .x = 0.4, .y = 0.4, .z = 0.4, .w = 1.0 },
//                     "(not initialized)",
//                 );
//             }
//         }
//         {
//             _ = cimgui.c.ImGui_TableSetColumnIndex(2);
//             cimgui.c.ImGui_PushIDInt(@intFromEnum(key));
//             defer cimgui.c.ImGui_PopID();
//             cimgui.c.ImGui_BeginDisabled(!is_initialized);
//             defer cimgui.c.ImGui_EndDisabled();
//             if (cimgui.c.ImGui_Button("View")) {
//                 if (!map.contains(key)) {
//                     map.put(key, .empty);
//                 }
//             }
//         }
//     }
// }
// ============================================================================

/// Table of miscellaneous terminal information.
// fn miscTable(t: *Terminal) void {
//
// PORT NOTE: `pub` (not the reference's file-private `fn`) so the DEFERRED
// `Info.draw`/`drawOpen` still reach it once inhabited (the screen.rs
// precedent). `*Terminal` is display-only here → `&Terminal` (INV-1).
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn miscTable(t: &Terminal) {
    // _ = cimgui.c.ImGui_BeginTable("table_misc", 2, cimgui.c.ImGuiTableFlags_None);
    let _ = unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"table_misc".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    };
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Working Directory");
            unsafe { dcimgui_sys::ImGui_Text(c"Working Directory".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The current working directory reported by the shell.");
            helpMarker(c"The current working directory reported by the shell.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // if (t.pwd.items.len > 0) {
            //     cimgui.c.ImGui_Text("%.*s", t.pwd.items.len, t.pwd.items.ptr);
            // } else {
            //     cimgui.c.ImGui_TextDisabled("(none)");
            // }
            //
            // PORT NOTE: `t.pwd` is a `Vec<u8>` byte buffer (the reference
            // `std.ArrayList(u8)`); `%.*s` takes `(int precision, char* ptr)`
            // so the (possibly non-NUL-terminated) buffer is bounded by its
            // length — `len()` + `as_ptr()`.
            if !t.pwd.is_empty() {
                unsafe {
                    dcimgui_sys::ImGui_Text(
                        c"%.*s".as_ptr(),
                        t.pwd.len() as c_int,
                        t.pwd.as_ptr() as *const c_char,
                    )
                };
            } else {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(none)".as_ptr()) };
            }
        }
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Focused");
            unsafe { dcimgui_sys::ImGui_Text(c"Focused".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("Whether the terminal itself is currently focused.");
            helpMarker(c"Whether the terminal itself is currently focused.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // var value: bool = t.flags.focused;
            // _ = cimgui.c.ImGui_Checkbox("##focused", &value);
            //
            // PORT NOTE (INV-3 display-only): the checkbox toggles a stack-local
            // `value` that is discarded — never a write-back to live flags.
            let mut value: bool = t.flags.focused;
            let _ = unsafe { dcimgui_sys::ImGui_Checkbox(c"##focused".as_ptr(), &mut value) };
        }
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Previous Char");
            unsafe { dcimgui_sys::ImGui_Text(c"Previous Char".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The previously printed character, used only for the REP sequence.");
            helpMarker(c"The previously printed character, used only for the REP sequence.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // if (t.previous_char) |c| {
            //     cimgui.c.ImGui_Text("U+%04X", @as(u32, c));
            // } else {
            //     cimgui.c.ImGui_TextDisabled("(none)");
            // }
            if let Some(c) = t.previous_char {
                unsafe { dcimgui_sys::ImGui_Text(c"U+%04X".as_ptr(), c as c_uint) };
            } else {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(none)".as_ptr()) };
            }
        }
    }
}

/// Table of terminal layout information.
// fn layoutTable(t: *Terminal) void {
//
// PORT NOTE (INV-3): this table holds the ONE inspector -> live-terminal
// write-back — `t.scrolling_region = override` — so it takes `&mut Terminal`
// (the reference `*Terminal`). Every other editable widget in this file reads
// into a discarded stack-local; ONLY the guarded assignment below mutates live
// terminal state.
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn layoutTable(t: &mut Terminal) {
    // _ = cimgui.c.ImGui_BeginTable("table_layout", 2, cimgui.c.ImGuiTableFlags_None);
    let _ = unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"table_layout".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    };
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Grid");
            unsafe { dcimgui_sys::ImGui_Text(c"Grid".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The size of the terminal grid in columns and rows.");
            helpMarker(c"The size of the terminal grid in columns and rows.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text("%dc x %dr", t.cols, t.rows);
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"%dc x %dr".as_ptr(),
                    t.cols as c_int,
                    t.rows as c_int,
                )
            };
        }
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Pixels");
            unsafe { dcimgui_sys::ImGui_Text(c"Pixels".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The size of the terminal grid in pixels.");
            helpMarker(c"The size of the terminal grid in pixels.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text("%dw x %dh", t.width_px, t.height_px);
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"%dw x %dh".as_ptr(),
                    t.width_px as c_int,
                    t.height_px as c_int,
                )
            };
        }
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Scroll Region");
            unsafe { dcimgui_sys::ImGui_Text(c"Scroll Region".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The scrolling region boundaries (top, bottom, left, right).");
            helpMarker(c"The scrolling region boundaries (top, bottom, left, right).");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_PushItemWidth(cimgui.c.ImGui_CalcTextSize("00000").x);
            unsafe {
                dcimgui_sys::ImGui_PushItemWidth(
                    dcimgui_sys::ImGui_CalcTextSize(c"00000".as_ptr()).x,
                )
            };
            // defer cimgui.c.ImGui_PopItemWidth();
            let _pop_item_width = Defer(|| unsafe { dcimgui_sys::ImGui_PopItemWidth() });

            // var override = t.scrolling_region;
            // var changed = false;
            //
            // PORT NOTE: the reference `var override = t.scrolling_region;` is a
            // value copy of the struct. `ScrollingRegion` is not `Copy` in the
            // port, so the copy is constructed field-by-field; the InputScalars
            // edit THIS stack-local, and only the guarded assignment below
            // writes it back (INV-3).
            let mut override_ = ScrollingRegion {
                top: t.scrolling_region.top,
                bottom: t.scrolling_region.bottom,
                left: t.scrolling_region.left,
                right: t.scrolling_region.right,
            };
            let mut changed = false;

            // cimgui.c.ImGui_AlignTextToFramePadding();
            unsafe { dcimgui_sys::ImGui_AlignTextToFramePadding() };
            // cimgui.c.ImGui_Text("T:");
            unsafe { dcimgui_sys::ImGui_Text(c"T:".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // if (cimgui.c.ImGui_InputScalar("##scroll_top", cimgui.c.ImGuiDataType_U16, &override.top)) {
            //     override.top = @min(override.top, t.rows -| 1);
            //     changed = true;
            // }
            if unsafe {
                dcimgui_sys::ImGui_InputScalar(
                    c"##scroll_top".as_ptr(),
                    dcimgui_sys::ImGuiDataType__ImGuiDataType_U16 as dcimgui_sys::ImGuiDataType,
                    &mut override_.top as *mut u16 as *mut c_void,
                )
            } {
                override_.top = core::cmp::min(override_.top, t.rows.saturating_sub(1));
                changed = true;
            }

            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // cimgui.c.ImGui_Text("B:");
            unsafe { dcimgui_sys::ImGui_Text(c"B:".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // if (cimgui.c.ImGui_InputScalar("##scroll_bottom", cimgui.c.ImGuiDataType_U16, &override.bottom)) {
            //     override.bottom = @min(override.bottom, t.rows -| 1);
            //     changed = true;
            // }
            if unsafe {
                dcimgui_sys::ImGui_InputScalar(
                    c"##scroll_bottom".as_ptr(),
                    dcimgui_sys::ImGuiDataType__ImGuiDataType_U16 as dcimgui_sys::ImGuiDataType,
                    &mut override_.bottom as *mut u16 as *mut c_void,
                )
            } {
                override_.bottom = core::cmp::min(override_.bottom, t.rows.saturating_sub(1));
                changed = true;
            }

            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // cimgui.c.ImGui_Text("L:");
            unsafe { dcimgui_sys::ImGui_Text(c"L:".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // if (cimgui.c.ImGui_InputScalar("##scroll_left", cimgui.c.ImGuiDataType_U16, &override.left)) {
            //     override.left = @min(override.left, t.cols -| 1);
            //     changed = true;
            // }
            if unsafe {
                dcimgui_sys::ImGui_InputScalar(
                    c"##scroll_left".as_ptr(),
                    dcimgui_sys::ImGuiDataType__ImGuiDataType_U16 as dcimgui_sys::ImGuiDataType,
                    &mut override_.left as *mut u16 as *mut c_void,
                )
            } {
                override_.left = core::cmp::min(override_.left, t.cols.saturating_sub(1));
                changed = true;
            }

            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // cimgui.c.ImGui_Text("R:");
            unsafe { dcimgui_sys::ImGui_Text(c"R:".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // if (cimgui.c.ImGui_InputScalar("##scroll_right", cimgui.c.ImGuiDataType_U16, &override.right)) {
            //     override.right = @min(override.right, t.cols -| 1);
            //     changed = true;
            // }
            if unsafe {
                dcimgui_sys::ImGui_InputScalar(
                    c"##scroll_right".as_ptr(),
                    dcimgui_sys::ImGuiDataType__ImGuiDataType_U16 as dcimgui_sys::ImGuiDataType,
                    &mut override_.right as *mut u16 as *mut c_void,
                )
            } {
                override_.right = core::cmp::min(override_.right, t.cols.saturating_sub(1));
                changed = true;
            }

            // if (changed and
            //     override.top < override.bottom and
            //     override.left < override.right)
            // {
            //     t.scrolling_region = override;
            // }
            //
            // PORT NOTE (INV-3): the SOLE inspector -> live terminal-state
            // write-back in the entire inspector. Guarded so a partial/invalid
            // edit never corrupts the region.
            if changed
                && override_.top < override_.bottom
                && override_.left < override_.right
            {
                t.scrolling_region = override_;
            }
        }
    }
}

/// Table of mouse-related terminal information.
// fn mouseTable(t: *Terminal) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn mouseTable(t: &Terminal) {
    // _ = cimgui.c.ImGui_BeginTable("table_mouse", 2, cimgui.c.ImGuiTableFlags_None);
    let _ = unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"table_mouse".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    };
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Event Mode");
            unsafe { dcimgui_sys::ImGui_Text(c"Event Mode".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The mouse event reporting mode set by the application.");
            helpMarker(c"The mouse event reporting mode set by the application.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text("%s", @tagName(t.flags.mouse_event).ptr);
            unsafe {
                dcimgui_sys::ImGui_Text(c"%s".as_ptr(), mouse_event_name(t.flags.mouse_event).as_ptr())
            };
        }
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Format");
            unsafe { dcimgui_sys::ImGui_Text(c"Format".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The mouse event encoding format.");
            helpMarker(c"The mouse event encoding format.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text("%s", @tagName(t.flags.mouse_format).ptr);
            unsafe {
                dcimgui_sys::ImGui_Text(c"%s".as_ptr(), mouse_format_name(t.flags.mouse_format).as_ptr())
            };
        }
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Shape");
            unsafe { dcimgui_sys::ImGui_Text(c"Shape".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The current mouse cursor shape set by the application.");
            helpMarker(c"The current mouse cursor shape set by the application.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text("%s", @tagName(t.mouse_shape).ptr);
            unsafe {
                dcimgui_sys::ImGui_Text(c"%s".as_ptr(), mouse_shape_name(t.mouse_shape).as_ptr())
            };
        }
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Shift Capture");
            unsafe { dcimgui_sys::ImGui_Text(c"Shift Capture".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("XTSHIFTESCAPE state for capturing shift in mouse protocol.");
            helpMarker(c"XTSHIFTESCAPE state for capturing shift in mouse protocol.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // if (t.flags.mouse_shift_capture == .null) {
            //     cimgui.c.ImGui_TextDisabled("(unset)");
            // } else {
            //     cimgui.c.ImGui_Text("%s", @tagName(t.flags.mouse_shift_capture).ptr);
            // }
            if t.flags.mouse_shift_capture == MouseShiftCapture::Null {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(unset)".as_ptr()) };
            } else {
                unsafe {
                    dcimgui_sys::ImGui_Text(
                        c"%s".as_ptr(),
                        mouse_shift_capture_name(t.flags.mouse_shift_capture).as_ptr(),
                    )
                };
            }
        }
    }
}

/// Table of color-related terminal information.
// fn colorTable(t: *Terminal, show_palette: *bool) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn colorTable(t: &Terminal, show_palette: &mut bool) {
    // cimgui.c.ImGui_TextWrapped(
    //     "Color state for the terminal. Note these colors only apply " ++
    //         "to the palette and unstyled colors. Many modern terminal " ++
    //         "applications use direct RGB colors which are not reflected here.",
    // );
    unsafe {
        dcimgui_sys::ImGui_TextWrapped(
            c"Color state for the terminal. Note these colors only apply to the palette and unstyled colors. Many modern terminal applications use direct RGB colors which are not reflected here."
                .as_ptr(),
        )
    };
    // cimgui.c.ImGui_Separator();
    unsafe { dcimgui_sys::ImGui_Separator() };

    // _ = cimgui.c.ImGui_BeginTable("table_color", 2, cimgui.c.ImGuiTableFlags_None);
    let _ = unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"table_color".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    };
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Background");
            unsafe { dcimgui_sys::ImGui_Text(c"Background".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("Unstyled cell background color.");
            helpMarker(c"Unstyled cell background color.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // _ = dynamicRGB("bg_color", &t.colors.background);
            let _ = dynamicRGB(c"bg_color", &t.colors.background);
        }
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Foreground");
            unsafe { dcimgui_sys::ImGui_Text(c"Foreground".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("Unstyled cell foreground color.");
            helpMarker(c"Unstyled cell foreground color.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // _ = dynamicRGB("fg_color", &t.colors.foreground);
            let _ = dynamicRGB(c"fg_color", &t.colors.foreground);
        }
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Cursor");
            unsafe { dcimgui_sys::ImGui_Text(c"Cursor".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("Cursor coloring set by escape sequences.");
            helpMarker(c"Cursor coloring set by escape sequences.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // _ = dynamicRGB("cursor_color", &t.colors.cursor);
            let _ = dynamicRGB(c"cursor_color", &t.colors.cursor);
        }
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Palette");
            unsafe { dcimgui_sys::ImGui_Text(c"Palette".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The 256-color palette.");
            helpMarker(c"The 256-color palette.");
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // if (cimgui.c.ImGui_Button("View")) {
            //     show_palette.* = true;
            // }
            if unsafe { dcimgui_sys::ImGui_Button(c"View".as_ptr()) } {
                *show_palette = true;
            }
        }
    }
}

/// The (Mode, name) pairs driving `modesTable`.
//
// PORT NOTE (§3 hand-expansion): the reference iterates
// `inline for (@typeInfo(terminal.Mode).@"enum".fields) |field|` with
// `@setEvalBranchQuota(6000)`. Rust has no `@typeInfo`/`@field`, so the
// comptime iteration over the `Mode` enum's fields is hand-expanded into this
// table, in exact declaration order (= the reference `ENTRIES` order the enum
// is generated from). Each row's `field.name` becomes the C-string literal, and
// `field.value` (the discriminant) is recovered at runtime as `mode as u16`.
// `@bitCast(@as(modes.ModeTag.Backing, field.value))` becomes
// `ModeTag::from_backing(mode as u16)`.
const MODES: &[(Mode, &CStr)] = &[
    // ANSI
    (Mode::DisableKeyboard, c"disable_keyboard"),
    (Mode::Insert, c"insert"),
    (Mode::SendReceiveMode, c"send_receive_mode"),
    (Mode::Linefeed, c"linefeed"),
    // DEC
    (Mode::CursorKeys, c"cursor_keys"),
    (Mode::Column132, c"132_column"),
    (Mode::SlowScroll, c"slow_scroll"),
    (Mode::ReverseColors, c"reverse_colors"),
    (Mode::Origin, c"origin"),
    (Mode::Wraparound, c"wraparound"),
    (Mode::Autorepeat, c"autorepeat"),
    (Mode::MouseEventX10, c"mouse_event_x10"),
    (Mode::CursorBlinking, c"cursor_blinking"),
    (Mode::CursorVisible, c"cursor_visible"),
    (Mode::EnableMode3, c"enable_mode_3"),
    (Mode::ReverseWrap, c"reverse_wrap"),
    (Mode::AltScreenLegacy, c"alt_screen_legacy"),
    (Mode::KeypadKeys, c"keypad_keys"),
    (Mode::BackarrowKeyMode, c"backarrow_key_mode"),
    (Mode::EnableLeftAndRightMargin, c"enable_left_and_right_margin"),
    (Mode::MouseEventNormal, c"mouse_event_normal"),
    (Mode::MouseEventButton, c"mouse_event_button"),
    (Mode::MouseEventAny, c"mouse_event_any"),
    (Mode::FocusEvent, c"focus_event"),
    (Mode::MouseFormatUtf8, c"mouse_format_utf8"),
    (Mode::MouseFormatSgr, c"mouse_format_sgr"),
    (Mode::MouseAlternateScroll, c"mouse_alternate_scroll"),
    (Mode::MouseFormatUrxvt, c"mouse_format_urxvt"),
    (Mode::MouseFormatSgrPixels, c"mouse_format_sgr_pixels"),
    (Mode::IgnoreKeypadWithNumlock, c"ignore_keypad_with_numlock"),
    (Mode::AltEscPrefix, c"alt_esc_prefix"),
    (Mode::AltSendsEscape, c"alt_sends_escape"),
    (Mode::ReverseWrapExtended, c"reverse_wrap_extended"),
    (Mode::AltScreen, c"alt_screen"),
    (Mode::SaveCursor, c"save_cursor"),
    (Mode::AltScreenSaveCursorClearEnter, c"alt_screen_save_cursor_clear_enter"),
    (Mode::BracketedPaste, c"bracketed_paste"),
    (Mode::SynchronizedOutput, c"synchronized_output"),
    (Mode::GraphemeCluster, c"grapheme_cluster"),
    (Mode::ReportColorScheme, c"report_color_scheme"),
    (Mode::InBandSizeReports, c"in_band_size_reports"),
];

/// Table of terminal modes.
// fn modesTable(t: *Terminal) void {
//
// PORT NOTE (INV-4): `t.modes.get(mode)` is a read-only fetch feeding the
// per-row checkbox stack-local (display-only); no mode is ever written back.
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn modesTable(t: &Terminal) {
    // _ = cimgui.c.ImGui_BeginTable(
    //     "table_modes",
    //     3,
    //     cimgui.c.ImGuiTableFlags_SizingFixedFit | cimgui.c.ImGuiTableFlags_RowBg,
    // );
    let _ = unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"table_modes".as_ptr(),
            3,
            (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_SizingFixedFit
                | dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_RowBg)
                as dcimgui_sys::ImGuiTableFlags,
        )
    };
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    {
        // cimgui.c.ImGui_TableSetupColumn("", cimgui.c.ImGuiTableColumnFlags_NoResize);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumn(
                c"".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_NoResize
                    as dcimgui_sys::ImGuiTableColumnFlags,
            )
        };
        // cimgui.c.ImGui_TableSetupColumn("Number", cimgui.c.ImGuiTableColumnFlags_PreferSortAscending);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumn(
                c"Number".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_PreferSortAscending
                    as dcimgui_sys::ImGuiTableColumnFlags,
            )
        };
        // cimgui.c.ImGui_TableSetupColumn("Name", cimgui.c.ImGuiTableColumnFlags_WidthStretch);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumn(
                c"Name".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthStretch
                    as dcimgui_sys::ImGuiTableColumnFlags,
            )
        };
        // cimgui.c.ImGui_TableHeadersRow();
        unsafe { dcimgui_sys::ImGui_TableHeadersRow() };
    }

    // inline for (@typeInfo(terminal.Mode).@"enum".fields) |field| {
    //     @setEvalBranchQuota(6000);
    //     const tag: modes.ModeTag = @bitCast(@as(modes.ModeTag.Backing, field.value));
    //     ...
    // }
    for &(mode, name) in MODES {
        // const tag: modes.ModeTag = @bitCast(@as(modes.ModeTag.Backing, field.value));
        let tag = ModeTag::from_backing(mode as u16);

        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // cimgui.c.ImGui_PushIDInt(@intCast(field.value));
        unsafe { dcimgui_sys::ImGui_PushIDInt(mode as u16 as c_int) };
        // defer cimgui.c.ImGui_PopID();
        let _pop_id = Defer(|| unsafe { dcimgui_sys::ImGui_PopID() });
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // var value: bool = t.modes.get(@field(terminal.Mode, field.name));
            // _ = cimgui.c.ImGui_Checkbox("##checkbox", &value);
            //
            // PORT NOTE (INV-3/INV-4 display-only): the checkbox toggles a
            // stack-local `value` that is discarded — no mode write-back.
            let mut value: bool = t.modes.get(mode);
            let _ = unsafe { dcimgui_sys::ImGui_Checkbox(c"##checkbox".as_ptr(), &mut value) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text(
            //     "%s%d",
            //     if (tag.ansi) "" else "?",
            //     @as(u32, @intCast(tag.value)),
            // );
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"%s%d".as_ptr(),
                    if tag.ansi { c"".as_ptr() } else { c"?".as_ptr() },
                    tag.value as c_int,
                )
            };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            // const name = std.fmt.comptimePrint("{s}", .{field.name});
            // cimgui.c.ImGui_Text("%s", name.ptr);
            unsafe { dcimgui_sys::ImGui_Text(c"%s".as_ptr(), name.as_ptr()) };
        }
    }
}

/// Render a DynamicRGB color.
// fn dynamicRGB(label: [:0]const u8, rgb: *terminal.color.DynamicRGB) bool {
//
// PORT NOTE: the reference `rgb: *terminal.color.DynamicRGB` is a mutable
// pointer, but the body only READS `rgb.override`/`rgb.default` (the ColorEdit3s
// write discarded stack-local `col` arrays — INV-3 display-only) so it ports as
// `&DynamicRGB`. `override` is `override_` in the port (Rust keyword).
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn dynamicRGB(label: &CStr, rgb: &DynamicRGB) -> bool {
    // _ = cimgui.c.ImGui_BeginTable(
    //     label,
    //     if (rgb.override != null) 2 else 1,
    //     cimgui.c.ImGuiTableFlags_SizingFixedFit,
    // );
    let _ = unsafe {
        dcimgui_sys::ImGui_BeginTable(
            label.as_ptr(),
            if rgb.override_.is_some() { 2 } else { 1 },
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_SizingFixedFit
                as dcimgui_sys::ImGuiTableFlags,
        )
    };
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // if (rgb.override != null) cimgui.c.ImGui_TableSetupColumn(
    //     "##label",
    //     cimgui.c.ImGuiTableColumnFlags_WidthFixed,
    // );
    if rgb.override_.is_some() {
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumn(
                c"##label".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                    as dcimgui_sys::ImGuiTableColumnFlags,
            )
        };
    }
    // cimgui.c.ImGui_TableSetupColumn(
    //     "##value",
    //     cimgui.c.ImGuiTableColumnFlags_WidthStretch,
    // );
    unsafe {
        dcimgui_sys::ImGui_TableSetupColumn(
            c"##value".as_ptr(),
            dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthStretch
                as dcimgui_sys::ImGuiTableColumnFlags,
        )
    };

    // if (rgb.override) |c| {
    if let Some(c) = rgb.override_ {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("override:");
        unsafe { dcimgui_sys::ImGui_Text(c"override:".as_ptr()) };
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // widgets.helpMarker("Overridden color set by escape sequences.");
        helpMarker(c"Overridden color set by escape sequences.");

        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // var col = [3]f32{ c.r/255.0, c.g/255.0, c.b/255.0 };
        let mut col = [
            c.r as f32 / 255.0,
            c.g as f32 / 255.0,
            c.b as f32 / 255.0,
        ];
        // _ = cimgui.c.ImGui_ColorEdit3("##override", &col, cimgui.c.ImGuiColorEditFlags_None);
        let _ = unsafe {
            dcimgui_sys::ImGui_ColorEdit3(
                c"##override".as_ptr(),
                col.as_mut_ptr(),
                dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_None
                    as dcimgui_sys::ImGuiColorEditFlags,
            )
        };
    }

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // if (rgb.default) |c| {
    if let Some(c) = rgb.default {
        // if (rgb.override != null) {
        if rgb.override_.is_some() {
            // cimgui.c.ImGui_Text("default:");
            unsafe { dcimgui_sys::ImGui_Text(c"default:".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("Default color from configuration.");
            helpMarker(c"Default color from configuration.");

            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        }

        // var col = [3]f32{ c.r/255.0, c.g/255.0, c.b/255.0 };
        let mut col = [
            c.r as f32 / 255.0,
            c.g as f32 / 255.0,
            c.b as f32 / 255.0,
        ];
        // _ = cimgui.c.ImGui_ColorEdit3("##default", &col, cimgui.c.ImGuiColorEditFlags_None);
        let _ = unsafe {
            dcimgui_sys::ImGui_ColorEdit3(
                c"##default".as_ptr(),
                col.as_mut_ptr(),
                dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_None
                    as dcimgui_sys::ImGuiColorEditFlags,
            )
        };
    } else {
        // cimgui.c.ImGui_TextDisabled("(unset)");
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"(unset)".as_ptr()) };
    }

    // return false;
    false
}

/// Render a color palette as a 16x16 grid of color buttons.
// fn palette(label: [:0]const u8, pal: *const terminal.color.Palette) void {
//
// PORT NOTE: `pal: *const terminal.color.Palette` (=`[RGB; 256]`) ports as
// `&Palette`; the ColorEdit3 writes a discarded stack-local `col_arr` (INV-3).
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn palette(label: &CStr, pal: &Palette) {
    // cimgui.c.ImGui_PushID(label);
    unsafe { dcimgui_sys::ImGui_PushID(label.as_ptr()) };
    // defer cimgui.c.ImGui_PopID();
    let _pop_id = Defer(|| unsafe { dcimgui_sys::ImGui_PopID() });

    // for (0..16) |row| {
    for row in 0..16usize {
        // for (0..16) |col| {
        for col in 0..16usize {
            // const idx = row * 16 + col;
            let idx = row * 16 + col;
            // const rgb = pal[idx];
            let rgb = pal[idx];
            // var col_arr = [3]f32{ rgb.r/255.0, rgb.g/255.0, rgb.b/255.0 };
            let mut col_arr = [
                rgb.r as f32 / 255.0,
                rgb.g as f32 / 255.0,
                rgb.b as f32 / 255.0,
            ];

            // if (col > 0) cimgui.c.ImGui_SameLine();
            if col > 0 {
                unsafe { dcimgui_sys::ImGui_SameLine() };
            }

            // cimgui.c.ImGui_PushIDInt(@intCast(idx));
            unsafe { dcimgui_sys::ImGui_PushIDInt(idx as c_int) };
            // _ = cimgui.c.ImGui_ColorEdit3("##color", &col_arr, cimgui.c.ImGuiColorEditFlags_NoInputs);
            let _ = unsafe {
                dcimgui_sys::ImGui_ColorEdit3(
                    c"##color".as_ptr(),
                    col_arr.as_mut_ptr(),
                    dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_NoInputs
                        as dcimgui_sys::ImGuiColorEditFlags,
                )
            };
            // if (cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_DelayShort)) {
            //     cimgui.c.ImGui_SetTooltip("%d: #%02X%02X%02X", idx, rgb.r, rgb.g, rgb.b);
            // }
            if unsafe {
                dcimgui_sys::ImGui_IsItemHovered(
                    dcimgui_sys::ImGuiHoveredFlags__ImGuiHoveredFlags_DelayShort
                        as dcimgui_sys::ImGuiHoveredFlags,
                )
            } {
                unsafe {
                    dcimgui_sys::ImGui_SetTooltip(
                        c"%d: #%02X%02X%02X".as_ptr(),
                        idx as c_int,
                        rgb.r as c_uint,
                        rgb.g as c_uint,
                        rgb.b as c_uint,
                    )
                };
            }
            // cimgui.c.ImGui_PopID();
            unsafe { dcimgui_sys::ImGui_PopID() };
        }
    }
}

// ============================================================================
// @tagName hand-expansions (§3). Rust has no `@tagName`; each mouse-state enum's
// tag string is produced by a match over its (payload-free) variants, matching
// the Zig field-name spelling used by the reference `@tagName(...).ptr`.
// ============================================================================

// @tagName(t.flags.mouse_event) — mouse.Event = { none, x10, normal, button, any }.
fn mouse_event_name(event: MouseEvent) -> &'static CStr {
    match event {
        MouseEvent::None => c"none",
        MouseEvent::X10 => c"x10",
        MouseEvent::Normal => c"normal",
        MouseEvent::Button => c"button",
        MouseEvent::Any => c"any",
    }
}

// @tagName(t.flags.mouse_format) — mouse.Format = { x10, utf8, sgr, urxvt, sgr_pixels }.
fn mouse_format_name(format: MouseFormat) -> &'static CStr {
    match format {
        MouseFormat::X10 => c"x10",
        MouseFormat::Utf8 => c"utf8",
        MouseFormat::Sgr => c"sgr",
        MouseFormat::Urxvt => c"urxvt",
        MouseFormat::SgrPixels => c"sgr_pixels",
    }
}

// @tagName(t.flags.mouse_shift_capture) — MouseShiftCapture = { null, false, true }.
fn mouse_shift_capture_name(capture: MouseShiftCapture) -> &'static CStr {
    match capture {
        MouseShiftCapture::Null => c"null",
        MouseShiftCapture::False => c"false",
        MouseShiftCapture::True => c"true",
    }
}

// @tagName(t.mouse_shape) — mouse.Shape (34 W3C cursor shapes, in declaration
// order; the snake_case forms match `@typeInfo(Shape).@"enum".fields`).
fn mouse_shape_name(shape: MouseShape) -> &'static CStr {
    match shape {
        MouseShape::Default => c"default",
        MouseShape::ContextMenu => c"context_menu",
        MouseShape::Help => c"help",
        MouseShape::Pointer => c"pointer",
        MouseShape::Progress => c"progress",
        MouseShape::Wait => c"wait",
        MouseShape::Cell => c"cell",
        MouseShape::Crosshair => c"crosshair",
        MouseShape::Text => c"text",
        MouseShape::VerticalText => c"vertical_text",
        MouseShape::Alias => c"alias",
        MouseShape::Copy => c"copy",
        MouseShape::Move => c"move",
        MouseShape::NoDrop => c"no_drop",
        MouseShape::NotAllowed => c"not_allowed",
        MouseShape::Grab => c"grab",
        MouseShape::Grabbing => c"grabbing",
        MouseShape::AllScroll => c"all_scroll",
        MouseShape::ColResize => c"col_resize",
        MouseShape::RowResize => c"row_resize",
        MouseShape::NResize => c"n_resize",
        MouseShape::EResize => c"e_resize",
        MouseShape::SResize => c"s_resize",
        MouseShape::WResize => c"w_resize",
        MouseShape::NeResize => c"ne_resize",
        MouseShape::NwResize => c"nw_resize",
        MouseShape::SeResize => c"se_resize",
        MouseShape::SwResize => c"sw_resize",
        MouseShape::EwResize => c"ew_resize",
        MouseShape::NsResize => c"ns_resize",
        MouseShape::NeswResize => c"nesw_resize",
        MouseShape::NwseResize => c"nwse_resize",
        MouseShape::ZoomIn => c"zoom_in",
        MouseShape::ZoomOut => c"zoom_out",
    }
}
