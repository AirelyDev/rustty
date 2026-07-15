//! Screen-information inspector widget.
//!
//! PORTS: ghostty src/inspector/widgets/screen.zig
//!
//! PORT STATUS: the six free-function tables (`cursorTable`, `cursorStyle`,
//! `keyboardTable`, `kittyGraphicsTable`, `internalStateTable`,
//! `semanticPromptTable` — the S6 mechanical seam) are LIVE (Task 11). They are
//! pure display-only reads of a `terminal.Screen` / `Cursor` /
//! `SemanticPrompt` / `ImageStorage` obtained per-frame under the surface's
//! `renderer_state.mutex` (INV-1) — the `*Screen`/`*const DynamicPalette` are
//! call-scoped `BORROW_PARAM`s (LIFETIMES §1), NEVER cached across frames
//! (INV-6 re-fetch each frame). No live mutation: the two `ImGui_Checkbox`
//! calls (`##pending_wrap`, `##protected`) and the semantic-prompt `##seen`
//! checkbox feed a stack-local `var value` that is discarded — INV-3, only
//! terminal.rs's `scrolling_region` write-back mutates live state.
//!
//! The `Info` aggregate struct + its `draw`/`createDockSpace` are carried as a
//! preserved-verbatim reference body below (they embed
//! `widgets.pagelist.{Inspector,CellChooser}`, which is the Phase-3 pagelist
//! port, Task 13); they inhabit once that module lands (placeholder-leaf
//! grows-in-place doctrine, the renderer.rs / lib.rs precedent).
//!
//! All bodies run on the main thread with the surface's ImGui context current
//! (INV-11).

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const cimgui = @import("dcimgui");
// const widgets = @import("../widgets.zig");
// const units = @import("../units.zig");
// const terminal = @import("../../terminal/main.zig");
// const stylepkg = @import("../../terminal/style.zig");
//
// PORT NOTE (key.rs/style.rs precedent): `dcimgui` is the vendored Dear ImGui
// C API, reached through `dcimgui-sys` (full `dcimgui_sys::` paths). The
// `terminal.*` types live in rustty-vt: `terminal.Screen`/`.Screen.Cursor`/
// `.Screen.SemanticPrompt`/`.Screen.SemanticClick` in its `screen` module,
// `terminal.color.Palette` in `color`, `terminal.kitty.graphics.ImageStorage`
// in `kitty::graphics`, the `PageList.Viewport` tag in `pagelist`, and the
// semantic-prompt `ClickEvents`/`Click` enums (for `@tagName`) in
// `osc::semantic_prompt`. `widgets.style`/`widgets.helpMarker` are the parent
// module's Task 7/9 helpers; `units.toKibiBytes` is the crate leaf.
use core::ffi::{c_char, c_int, CStr};

use rustty_vt::color::Palette;
use rustty_vt::kitty::graphics::ImageStorage;
use rustty_vt::osc::semantic_prompt::{Click, ClickEvents};
use rustty_vt::pagelist::Viewport;
use rustty_vt::screen::{Cursor, Screen, SemanticClick, SemanticPrompt};

use super::helpMarker;
use crate::units;

/// Minimal `defer`-style scope guard — runs `f` on drop (mirrors the private
/// `Defer` in `widgets.rs`/`style.rs`). Backs the ImGui `BeginTable`/`EndTable`
/// and `Begin`/`End` pairings below so they fire on every exit path. panic=abort
/// means no unwinding ever crosses these FFI calls, so drop-on-scope-exit is the
/// sole runtime path (no double-fire, no leak).
struct Defer<F: FnMut()>(F);

impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

// ============================================================================
// PORT NOTE (Task 11 — DEFERRED aggregate, pagelist Task 13):
//
// `/// Window names for the screen dockspace.`
// const window_info = "Info";
// const window_cell = "Cell";
// const window_pagelist = "PageList";
//
// The `Info` aggregate + its `draw`/`createDockSpace` embed
// `widgets.pagelist.Inspector` and `widgets.pagelist.CellChooser`, which are
// the Phase-3 `pagelist.rs` port (Task 13, not yet landed). They are carried
// here as the preserved verbatim reference body (R1 comment fidelity) and
// inhabit in place once `widgets::pagelist` exists — exactly the
// placeholder-leaf grows-in-place doctrine used by renderer.rs (draw half) and
// lib.rs (the `gui` aggregate). The six free-function tables it calls into
// (`cursorTable`/`cursorStyle`/`keyboardTable`/`semanticPromptTable`/
// `kittyGraphicsTable`/`internalStateTable`) ARE ported LIVE below.
//
// The reference draw obtains its `screen: *terminal.Screen`,
// `color_palette: *const terminal.color.DynamicPalette`, `key`/`active_key:
// terminal.ScreenSet.Key`, and `modify_other_keys_2: bool` as an anonymous
// `data` params struct — all BORROW_PARAM, call-scoped under the surface mutex
// (INV-1); the `*Screen`/`*DynamicPalette` are never cached (INV-6 re-fetch).
// When inhabited, that anonymous `data` struct becomes a named `DrawData`
// params struct (the S6 note).
//
// /// Screen information inspector widget.
// pub const Info = struct {
//     pagelist: widgets.pagelist.Inspector,
//     cell_chooser: widgets.pagelist.CellChooser,
//
//     pub const empty: Info = .{
//         .pagelist = .empty,
//         .cell_chooser = .empty,
//     };
//
//     /// Draw the screen info contents.
//     pub fn draw(self: *Info, open: bool, data: struct {
//         /// The screen that we're inspecting.
//         screen: *terminal.Screen,
//         /// Which screen key we're viewing.
//         key: terminal.ScreenSet.Key,
//         /// Which screen is active (primary or alternate).
//         active_key: terminal.ScreenSet.Key,
//         /// Whether xterm modify other keys mode 2 is enabled.
//         modify_other_keys_2: bool,
//         /// Color palette for cursor color resolution.
//         color_palette: *const terminal.color.DynamicPalette,
//     }) void {
//         // Create the dockspace for this screen
//         const dockspace_id = cimgui.c.ImGui_GetID("Screen Dockspace");
//         _ = createDockSpace(dockspace_id);
//
//         const screen = data.screen;
//
//         // Info window
//         info: {
//             defer cimgui.c.ImGui_End();
//             if (!cimgui.c.ImGui_Begin(
//                 window_info,
//                 null,
//                 cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
//             )) break :info;
//
//             if (cimgui.c.ImGui_CollapsingHeader(
//                 "Cursor",
//                 cimgui.c.ImGuiTreeNodeFlags_DefaultOpen,
//             )) {
//                 cursorTable(&screen.cursor);
//                 cimgui.c.ImGui_Separator();
//                 cursorStyle(
//                     &screen.cursor,
//                     &data.color_palette.current,
//                 );
//             }
//
//             if (cimgui.c.ImGui_CollapsingHeader(
//                 "Keyboard",
//                 cimgui.c.ImGuiTreeNodeFlags_DefaultOpen,
//             )) keyboardTable(
//                 screen,
//                 data.modify_other_keys_2,
//             );
//
//             if (cimgui.c.ImGui_CollapsingHeader(
//                 "Semantic Prompt",
//                 cimgui.c.ImGuiTreeNodeFlags_DefaultOpen,
//             )) semanticPromptTable(&screen.semantic_prompt);
//
//             if (cimgui.c.ImGui_CollapsingHeader(
//                 "Kitty Graphics",
//                 cimgui.c.ImGuiTreeNodeFlags_DefaultOpen,
//             )) kittyGraphicsTable(&screen.kitty_images);
//
//             if (cimgui.c.ImGui_CollapsingHeader(
//                 "Other Screen State",
//                 cimgui.c.ImGuiTreeNodeFlags_DefaultOpen,
//             )) internalStateTable(screen);
//         }
//
//         // Cell window
//         cell: {
//             defer cimgui.c.ImGui_End();
//             if (!cimgui.c.ImGui_Begin(
//                 window_cell,
//                 null,
//                 cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
//             )) break :cell;
//             self.cell_chooser.draw(&screen.pages);
//         }
//
//         // PageList window
//         pagelist: {
//             defer cimgui.c.ImGui_End();
//             if (!cimgui.c.ImGui_Begin(
//                 window_pagelist,
//                 null,
//                 cimgui.c.ImGuiWindowFlags_NoFocusOnAppearing,
//             )) break :pagelist;
//             self.pagelist.draw(&screen.pages);
//         }
//
//         // The remainder is the open state
//         if (!open) return;
//
//         // Show warning if viewing an inactive screen
//         if (data.key != data.active_key) {
//             cimgui.c.ImGui_TextColored(
//                 .{ .x = 1.0, .y = 0.8, .z = 0.0, .w = 1.0 },
//                 "⚠ Viewing inactive screen",
//             );
//             cimgui.c.ImGui_Separator();
//         }
//     }
//
//     /// Create the dock space for the screen inspector. This creates
//     /// a dedicated dock space for the screen inspector windows. But they
//     /// can of course be undocked and moved around as desired.
//     fn createDockSpace(dockspace_id: cimgui.c.ImGuiID) bool {
//         // Check if we need to set up the dockspace
//         const setup = cimgui.ImGui_DockBuilderGetNode(dockspace_id) == null;
//
//         if (setup) {
//             // Register our dockspace node
//             assert(cimgui.ImGui_DockBuilderAddNodeEx(
//                 dockspace_id,
//                 cimgui.ImGuiDockNodeFlagsPrivate.DockSpace,
//             ) == dockspace_id);
//
//             // Dock windows into the space
//             cimgui.ImGui_DockBuilderDockWindow(window_info, dockspace_id);
//             cimgui.ImGui_DockBuilderDockWindow(window_cell, dockspace_id);
//             cimgui.ImGui_DockBuilderDockWindow(window_pagelist, dockspace_id);
//             cimgui.ImGui_DockBuilderFinish(dockspace_id);
//         }
//
//         // Create the dockspace
//         assert(cimgui.c.ImGui_DockSpaceEx(
//             dockspace_id,
//             .{ .x = 0, .y = 0 },
//             cimgui.c.ImGuiDockNodeFlags_None,
//             null,
//         ) == dockspace_id);
//         return setup;
//     }
// };
// ============================================================================

/// Render cursor state with a table of cursor-specific fields.
// pub fn cursorTable(
//     cursor: *const terminal.Screen.Cursor,
// ) void {
//
// PORT NOTE (INV-1): `*const terminal.Screen.Cursor` is a call-scoped shared
// borrow obtained under the surface mutex; ports as `&Cursor`.
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn cursorTable(cursor: &Cursor) {
    // if (!cimgui.c.ImGui_BeginTable(
    //     "table_cursor",
    //     2,
    //     cimgui.c.ImGuiTableFlags_None,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"table_cursor".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    } {
        return;
    }
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // cimgui.c.ImGui_Text("Position (x, y)");
    unsafe { dcimgui_sys::ImGui_Text(c"Position (x, y)".as_ptr()) };
    // cimgui.c.ImGui_SameLine();
    unsafe { dcimgui_sys::ImGui_SameLine() };
    // widgets.helpMarker("The current cursor position in the terminal grid (0-indexed).");
    helpMarker(c"The current cursor position in the terminal grid (0-indexed).");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // cimgui.c.ImGui_Text("(%d, %d)", cursor.x, cursor.y);
    unsafe {
        dcimgui_sys::ImGui_Text(c"(%d, %d)".as_ptr(), cursor.x as c_int, cursor.y as c_int)
    };

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // cimgui.c.ImGui_Text("Hyperlink");
    unsafe { dcimgui_sys::ImGui_Text(c"Hyperlink".as_ptr()) };
    // cimgui.c.ImGui_SameLine();
    unsafe { dcimgui_sys::ImGui_SameLine() };
    // widgets.helpMarker("The active OSC8 hyperlink for newly printed characters.");
    helpMarker(c"The active OSC8 hyperlink for newly printed characters.");
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // if (cursor.hyperlink) |link| {
    //     cimgui.c.ImGui_Text("%.*s", link.uri.len, link.uri.ptr);
    // } else {
    //     cimgui.c.ImGui_TextDisabled("(none)");
    // }
    //
    // PORT NOTE (INV-1): `cursor.hyperlink` is a raw `Option<*mut Hyperlink>`
    // (a call-scoped back-reference into screen state, deref'd under the
    // surface mutex). `link.uri` is a `Vec<u8>` byte slice; `%.*s` takes
    // `(int precision, char* ptr)` so the (possibly non-NUL-terminated) slice
    // is bounded by its length — `uri.len()` + `uri.as_ptr()`.
    if let Some(link) = cursor.hyperlink {
        let uri = unsafe { &(*link).uri };
        unsafe {
            dcimgui_sys::ImGui_Text(
                c"%.*s".as_ptr(),
                uri.len() as c_int,
                uri.as_ptr() as *const c_char,
            )
        };
    } else {
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"(none)".as_ptr()) };
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Pending Wrap");
        unsafe { dcimgui_sys::ImGui_Text(c"Pending Wrap".as_ptr()) };
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // widgets.helpMarker("The 'last column flag' (LCF). If set, the next character will force a soft-wrap to the next line.");
        helpMarker(c"The 'last column flag' (LCF). If set, the next character will force a soft-wrap to the next line.");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // var value: bool = cursor.pending_wrap;
        // _ = cimgui.c.ImGui_Checkbox("##pending_wrap", &value);
        //
        // PORT NOTE (INV-3 display-only): the checkbox toggles a stack-local
        // `value` that is discarded — never a write-back to live cursor state.
        let mut value: bool = cursor.pending_wrap;
        let _ = unsafe { dcimgui_sys::ImGui_Checkbox(c"##pending_wrap".as_ptr(), &mut value) };
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Protected");
        unsafe { dcimgui_sys::ImGui_Text(c"Protected".as_ptr()) };
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // widgets.helpMarker("If enabled, new characters will have the protected attribute set, preventing erasure by certain sequences.");
        helpMarker(c"If enabled, new characters will have the protected attribute set, preventing erasure by certain sequences.");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // var value: bool = cursor.protected;
        // _ = cimgui.c.ImGui_Checkbox("##protected", &value);
        //
        // PORT NOTE (INV-3 display-only): stack-local, discarded (as above).
        let mut value: bool = cursor.protected;
        let _ = unsafe { dcimgui_sys::ImGui_Checkbox(c"##protected".as_ptr(), &mut value) };
    }
}

/// Render cursor style information using the shared style table.
// pub fn cursorStyle(cursor: *const terminal.Screen.Cursor, palette: ?*const terminal.color.Palette) void {
//     widgets.style.table(cursor.style, palette);
// }
//
// PORT NOTE: `cursor.style` is a `Copy` `terminal.Style` (passed by value into
// `style::table`); `palette: ?*const terminal.color.Palette` is the call-scoped
// borrowed palette (the caller hands `&data.color_palette.current`).
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn cursorStyle(cursor: &Cursor, palette: Option<&Palette>) {
    // widgets.style.table(cursor.style, palette);
    super::style::table(cursor.style, palette);
}

/// Render keyboard information with a table.
// fn keyboardTable(
//     screen: *const terminal.Screen,
//     modify_other_keys_2: bool,
// ) void {
//
// PORT NOTE: `pub` (not the reference's file-private `fn`) so the DEFERRED
// `Info.draw` (which calls it) still reaches it once inhabited; `*const Screen`
// is a call-scoped borrow (INV-1).
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn keyboardTable(screen: &Screen, modify_other_keys_2: bool) {
    // if (!cimgui.c.ImGui_BeginTable(
    //     "table_keyboard",
    //     2,
    //     cimgui.c.ImGuiTableFlags_None,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"table_keyboard".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    } {
        return;
    }
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // const kitty_flags = screen.kitty_keyboard.current();
    //
    // PORT NOTE (INV-4): read-only fetch of the current kitty-keyboard flags
    // (a `Copy` `Flags` value); display-only.
    let kitty_flags = screen.kitty_keyboard.current();

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Mode");
            unsafe { dcimgui_sys::ImGui_Text(c"Mode".as_ptr()) };
        }
        {
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // const mode = if (kitty_flags.int() != 0) "kitty" else "legacy";
            let mode: &CStr = if kitty_flags.int() != 0 {
                c"kitty"
            } else {
                c"legacy"
            };
            // cimgui.c.ImGui_Text("%s", mode.ptr);
            unsafe { dcimgui_sys::ImGui_Text(c"%s".as_ptr(), mode.as_ptr()) };
        }
    }

    // if (kitty_flags.int() != 0) {
    if kitty_flags.int() != 0 {
        // const Flags = @TypeOf(kitty_flags);
        // inline for (@typeInfo(Flags).@"struct".fields) |field| {
        //     const value = @field(kitty_flags, field.name);
        //     cimgui.c.ImGui_TableNextRow();
        //     {
        //         _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        //         const field_name = std.fmt.comptimePrint("{s}", .{field.name});
        //         cimgui.c.ImGui_Text("%s", field_name.ptr);
        //     }
        //     {
        //         _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        //         cimgui.c.ImGui_Text(
        //             "%s",
        //             if (value) "true".ptr else "false".ptr,
        //         );
        //     }
        // }
        //
        // PORT NOTE (§3 `@typeInfo(Flags).@"struct".fields` + `comptimePrint`):
        // Rust has no `@typeInfo`/`@field`, so the comptime iteration over the
        // packed `Flags` struct's fields is hand-expanded, one row per field in
        // exact declaration order (kitty/key.rs `Flags`: disambiguate,
        // report_events, report_alternates, report_all, report_associated). The
        // `comptimePrint("{s}", .{field.name})` field-name string collapses to
        // the C string literal. `kitty_row!` factors the identical 6-call body
        // (a literal inlining of the reference block; no runtime iteration).
        macro_rules! kitty_row {
            ($value:expr, $field_name:expr) => {{
                let value: bool = $value;
                // cimgui.c.ImGui_TableNextRow();
                unsafe { dcimgui_sys::ImGui_TableNextRow() };
                {
                    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
                    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
                    // const field_name = std.fmt.comptimePrint("{s}", .{field.name});
                    // cimgui.c.ImGui_Text("%s", field_name.ptr);
                    unsafe { dcimgui_sys::ImGui_Text(c"%s".as_ptr(), $field_name.as_ptr()) };
                }
                {
                    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
                    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
                    // cimgui.c.ImGui_Text("%s", if (value) "true".ptr else "false".ptr);
                    unsafe {
                        dcimgui_sys::ImGui_Text(
                            c"%s".as_ptr(),
                            if value { c"true".as_ptr() } else { c"false".as_ptr() },
                        )
                    };
                }
            }};
        }
        kitty_row!(kitty_flags.disambiguate, c"disambiguate");
        kitty_row!(kitty_flags.report_events, c"report_events");
        kitty_row!(kitty_flags.report_alternates, c"report_alternates");
        kitty_row!(kitty_flags.report_all, c"report_all");
        kitty_row!(kitty_flags.report_associated, c"report_associated");
    } else {
        {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            {
                // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
                let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
                // cimgui.c.ImGui_Text("Xterm modify keys");
                unsafe { dcimgui_sys::ImGui_Text(c"Xterm modify keys".as_ptr()) };
            }
            {
                // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
                let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
                // cimgui.c.ImGui_Text(
                //     "%s",
                //     if (modify_other_keys_2) "true".ptr else "false".ptr,
                // );
                unsafe {
                    dcimgui_sys::ImGui_Text(
                        c"%s".as_ptr(),
                        if modify_other_keys_2 {
                            c"true".as_ptr()
                        } else {
                            c"false".as_ptr()
                        },
                    )
                };
            }
        }
    } // keyboard mode info
}

/// Render kitty graphics information table.
// pub fn kittyGraphicsTable(
//     kitty_images: *const terminal.kitty.graphics.ImageStorage,
// ) void {
//
// PORT NOTE: `*const ImageStorage` is a call-scoped borrow (INV-1); all reads
// are pure observation.
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn kittyGraphicsTable(kitty_images: &ImageStorage) {
    // if (!kitty_images.enabled()) {
    //     cimgui.c.ImGui_TextDisabled("(Kitty graphics are disabled)");
    //     return;
    // }
    if !kitty_images.enabled() {
        unsafe { dcimgui_sys::ImGui_TextDisabled(c"(Kitty graphics are disabled)".as_ptr()) };
        return;
    }

    // if (!cimgui.c.ImGui_BeginTable(
    //     "##kitty_graphics",
    //     2,
    //     cimgui.c.ImGuiTableFlags_None,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"##kitty_graphics".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    } {
        return;
    }
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // cimgui.c.ImGui_Text("Memory Usage");
    unsafe { dcimgui_sys::ImGui_Text(c"Memory Usage".as_ptr()) };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // cimgui.c.ImGui_Text("%d bytes (%d KiB)", kitty_images.total_bytes, units.toKibiBytes(kitty_images.total_bytes));
    unsafe {
        dcimgui_sys::ImGui_Text(
            c"%d bytes (%d KiB)".as_ptr(),
            kitty_images.total_bytes as c_int,
            units::toKibiBytes(kitty_images.total_bytes) as c_int,
        )
    };

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // cimgui.c.ImGui_Text("Memory Limit");
    unsafe { dcimgui_sys::ImGui_Text(c"Memory Limit".as_ptr()) };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // cimgui.c.ImGui_Text("%d bytes (%d KiB)", kitty_images.total_limit, units.toKibiBytes(kitty_images.total_limit));
    unsafe {
        dcimgui_sys::ImGui_Text(
            c"%d bytes (%d KiB)".as_ptr(),
            kitty_images.total_limit as c_int,
            units::toKibiBytes(kitty_images.total_limit) as c_int,
        )
    };

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // cimgui.c.ImGui_Text("Image Count");
    unsafe { dcimgui_sys::ImGui_Text(c"Image Count".as_ptr()) };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // cimgui.c.ImGui_Text("%d", kitty_images.images.count());
    //
    // PORT NOTE: the reference `AutoHashMapUnmanaged.count()` maps to the ported
    // `std::collections::HashMap::len()`.
    unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), kitty_images.images.len() as c_int) };

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // cimgui.c.ImGui_Text("Placement Count");
    unsafe { dcimgui_sys::ImGui_Text(c"Placement Count".as_ptr()) };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // cimgui.c.ImGui_Text("%d", kitty_images.placements.count());
    unsafe { dcimgui_sys::ImGui_Text(c"%d".as_ptr(), kitty_images.placements.len() as c_int) };

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // cimgui.c.ImGui_Text("Image Loading");
    unsafe { dcimgui_sys::ImGui_Text(c"Image Loading".as_ptr()) };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // cimgui.c.ImGui_Text("%s", if (kitty_images.loading != null) "true".ptr else "false".ptr);
    unsafe {
        dcimgui_sys::ImGui_Text(
            c"%s".as_ptr(),
            if kitty_images.loading.is_some() {
                c"true".as_ptr()
            } else {
                c"false".as_ptr()
            },
        )
    };
}

/// Render internal terminal state table.
// pub fn internalStateTable(
//     screen: *const terminal.Screen,
// ) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn internalStateTable(screen: &Screen) {
    // const pages = &screen.pages;
    let pages = &screen.pages;

    // if (!cimgui.c.ImGui_BeginTable(
    //     "##terminal_state",
    //     2,
    //     cimgui.c.ImGuiTableFlags_None,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"##terminal_state".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    } {
        return;
    }
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // cimgui.c.ImGui_Text("Memory Usage");
    unsafe { dcimgui_sys::ImGui_Text(c"Memory Usage".as_ptr()) };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // cimgui.c.ImGui_Text("%d bytes (%d KiB)", pages.page_size, units.toKibiBytes(pages.page_size));
    unsafe {
        dcimgui_sys::ImGui_Text(
            c"%d bytes (%d KiB)".as_ptr(),
            pages.page_size as c_int,
            units::toKibiBytes(pages.page_size) as c_int,
        )
    };

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // cimgui.c.ImGui_Text("Memory Limit");
    unsafe { dcimgui_sys::ImGui_Text(c"Memory Limit".as_ptr()) };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // cimgui.c.ImGui_Text("%d bytes (%d KiB)", pages.maxSize(), units.toKibiBytes(pages.maxSize()));
    //
    // PORT NOTE: `pages.maxSize()` -> the ported `PageList::max_size()`.
    unsafe {
        dcimgui_sys::ImGui_Text(
            c"%d bytes (%d KiB)".as_ptr(),
            pages.max_size() as c_int,
            units::toKibiBytes(pages.max_size()) as c_int,
        )
    };

    // cimgui.c.ImGui_TableNextRow();
    unsafe { dcimgui_sys::ImGui_TableNextRow() };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
    // cimgui.c.ImGui_Text("Viewport Location");
    unsafe { dcimgui_sys::ImGui_Text(c"Viewport Location".as_ptr()) };
    // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
    let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
    // cimgui.c.ImGui_Text("%s", @tagName(pages.viewport).ptr);
    //
    // PORT NOTE (§3 `@tagName`): Rust has no `@tagName`; the `PageList.Viewport`
    // tag name string is produced by a match over the (payload-free) variants,
    // reproducing the Zig field-name spelling (active / top / pin).
    let viewport_name: &CStr = match pages.viewport {
        Viewport::Active => c"active",
        Viewport::Top => c"top",
        Viewport::Pin => c"pin",
    };
    unsafe { dcimgui_sys::ImGui_Text(c"%s".as_ptr(), viewport_name.as_ptr()) };
}

/// Render semantic prompt state table.
// pub fn semanticPromptTable(
//     semantic_prompt: *const terminal.Screen.SemanticPrompt,
// ) void {
#[allow(non_snake_case)] // faithful Zig name (R1)
pub fn semanticPromptTable(semantic_prompt: &SemanticPrompt) {
    // if (!cimgui.c.ImGui_BeginTable(
    //     "##semantic_prompt",
    //     2,
    //     cimgui.c.ImGuiTableFlags_None,
    // )) return;
    if !unsafe {
        dcimgui_sys::ImGui_BeginTable(
            c"##semantic_prompt".as_ptr(),
            2,
            dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
        )
    } {
        return;
    }
    // defer cimgui.c.ImGui_EndTable();
    let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Seen");
        unsafe { dcimgui_sys::ImGui_Text(c"Seen".as_ptr()) };
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // widgets.helpMarker("Whether any semantic prompt markers (OSC 133) have been seen in this screen.");
        helpMarker(c"Whether any semantic prompt markers (OSC 133) have been seen in this screen.");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // var value: bool = semantic_prompt.seen;
        // _ = cimgui.c.ImGui_Checkbox("##seen", &value);
        //
        // PORT NOTE (INV-3 display-only): stack-local, discarded.
        let mut value: bool = semantic_prompt.seen;
        let _ = unsafe { dcimgui_sys::ImGui_Checkbox(c"##seen".as_ptr(), &mut value) };
    }

    {
        // cimgui.c.ImGui_TableNextRow();
        unsafe { dcimgui_sys::ImGui_TableNextRow() };
        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
        // cimgui.c.ImGui_Text("Click Handling");
        unsafe { dcimgui_sys::ImGui_Text(c"Click Handling".as_ptr()) };
        // cimgui.c.ImGui_SameLine();
        unsafe { dcimgui_sys::ImGui_SameLine() };
        // widgets.helpMarker("How click events are handled in prompts. Set via 'cl' or 'click_events' options in OSC 133.");
        helpMarker(c"How click events are handled in prompts. Set via 'cl' or 'click_events' options in OSC 133.");
        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
        // switch (semantic_prompt.click) {
        //     .none => cimgui.c.ImGui_TextDisabled("(none)"),
        //     .click_events => |click_events| cimgui.c.ImGui_Text("click_events=%s", @tagName(click_events).ptr),
        //     .cl => |cl| cimgui.c.ImGui_Text("cl=%s", @tagName(cl).ptr),
        // }
        //
        // PORT NOTE (§3 `@tagName`): the inner `ClickEvents`/`Click` tag names
        // are produced by a match over their (payload-free) variants, matching
        // the Zig field spelling (absolute/relative; line/multiple/
        // conservative_vertical/smart_vertical).
        match semantic_prompt.click {
            // .none => cimgui.c.ImGui_TextDisabled("(none)"),
            SemanticClick::None => unsafe {
                dcimgui_sys::ImGui_TextDisabled(c"(none)".as_ptr())
            },
            // .click_events => |click_events| cimgui.c.ImGui_Text("click_events=%s", @tagName(click_events).ptr),
            SemanticClick::ClickEvents(click_events) => {
                let name: &CStr = match click_events {
                    ClickEvents::Absolute => c"absolute",
                    ClickEvents::Relative => c"relative",
                };
                unsafe {
                    dcimgui_sys::ImGui_Text(c"click_events=%s".as_ptr(), name.as_ptr())
                };
            }
            // .cl => |cl| cimgui.c.ImGui_Text("cl=%s", @tagName(cl).ptr),
            SemanticClick::Cl(cl) => {
                let name: &CStr = match cl {
                    Click::Line => c"line",
                    Click::Multiple => c"multiple",
                    Click::ConservativeVertical => c"conservative_vertical",
                    Click::SmartVertical => c"smart_vertical",
                };
                unsafe { dcimgui_sys::ImGui_Text(c"cl=%s".as_ptr(), name.as_ptr()) };
            }
        }
    }
}
