//! Cell-style inspector widget (`table` / `color`).
//!
//! PORTS: ghostty src/inspector/widgets/style.zig
//!
//! PORT STATUS: LIVE (full-file port, Task 9). Pure display-only reads of a
//! `terminal.Style` value (passed by value — `Style` is `Copy`) with an
//! optional borrowed palette. No live mutation (INV-3: every editable widget
//! is read-into-local-and-discard; this one has no editable state at all — the
//! `ImGui_ColorEdit3` swatches feed a stack-local `[3]f32` that is dropped).
//! All bodies run on the main thread with the surface's ImGui context current
//! (INV-11).

// const std = @import("std");
// const cimgui = @import("dcimgui");
// PORT NOTE (Task 8/9 precedent, key.rs): `dcimgui` is the vendored Dear ImGui
// C API; the port calls it through `dcimgui-sys` (full `dcimgui_sys::` paths,
// matching `widgets.rs`).
use core::ffi::{c_int, CStr};
// const terminal = @import("../../terminal/main.zig");
// PORT NOTE: `terminal.Style` / `terminal.Style.Color` live in rustty-vt's
// `style` module; `terminal.color.Palette` in its `color` module.
use rustty_vt::color::Palette;
use rustty_vt::style::{Color, Style};
// const widgets = @import("../widgets.zig");
// PORT NOTE: `widgets.helpMarker` is the parent module's shared helper
// (`super::helpMarker`, Task 7).
use super::helpMarker;

/// Minimal `defer`-style scope guard — runs `f` on drop (mirrors the private
/// `Defer` in `widgets.rs`/`key.rs`). Backs the ImGui `BeginTable`/`EndTable`
/// and `PushID`/`PopID` pairings below so they fire on every exit path.
/// panic=abort means no unwinding ever crosses these FFI calls, so
/// drop-on-scope-exit is the sole runtime path (no double-fire, no leak).
struct Defer<F: FnMut()>(F);

impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

/// Render a style as a table.
// pub fn table(
//     st: terminal.Style,
//     palette: ?*const terminal.color.Palette,
// ) void {
pub fn table(st: Style, palette: Option<&Palette>) {
    {
        // _ = cimgui.c.ImGui_BeginTable(
        //     "style",
        //     2,
        //     cimgui.c.ImGuiTableFlags_None,
        // );
        let _ = unsafe {
            dcimgui_sys::ImGui_BeginTable(
                c"style".as_ptr(),
                2,
                dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
            )
        };
        // defer cimgui.c.ImGui_EndTable();
        let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });
        {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Foreground");
            unsafe { dcimgui_sys::ImGui_Text(c"Foreground".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The foreground (text) color");
            helpMarker(c"The foreground (text) color");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // color("fg", st.fg_color, palette);
            color(c"fg", st.fg_color, palette);
        }

        {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Background");
            unsafe { dcimgui_sys::ImGui_Text(c"Background".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The background (cell) color");
            helpMarker(c"The background (cell) color");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // color("bg", st.bg_color, palette);
            color(c"bg", st.bg_color, palette);
        }

        {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Underline");
            unsafe { dcimgui_sys::ImGui_Text(c"Underline".as_ptr()) };
            // cimgui.c.ImGui_SameLine();
            unsafe { dcimgui_sys::ImGui_SameLine() };
            // widgets.helpMarker("The underline color, if underlines are enabled.");
            helpMarker(c"The underline color, if underlines are enabled.");
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // color("underline", st.underline_color, palette);
            color(c"underline", st.underline_color, palette);
        }

        // const style_flags = .{
        //     .{ "bold", "Text will be rendered with bold weight." },
        //     .{ "italic", "Text will be rendered in italic style." },
        //     .{ "faint", "Text will be rendered with reduced intensity." },
        //     .{ "blink", "Text will blink." },
        //     .{ "inverse", "Foreground and background colors are swapped." },
        //     .{ "invisible", "Text will be invisible (hidden)." },
        //     .{ "strikethrough", "Text will have a line through it." },
        // };
        // inline for (style_flags) |entry| entry: {
        //     const style = entry[0];
        //     const help = entry[1];
        //     if (!@field(st.flags, style)) break :entry;
        //
        //     cimgui.c.ImGui_TableNextRow();
        //     {
        //         _ = cimgui.c.ImGui_TableSetColumnIndex(0);
        //         cimgui.c.ImGui_Text(style.ptr);
        //         cimgui.c.ImGui_SameLine();
        //         widgets.helpMarker(help);
        //     }
        //     {
        //         _ = cimgui.c.ImGui_TableSetColumnIndex(1);
        //         cimgui.c.ImGui_Text("true");
        //     }
        // }
        //
        // PORT NOTE (§3): Rust has no `inline for` / `@field(st.flags, style)`,
        // so the comptime loop over the `style_flags` `(name, help)` tuple is
        // hand-expanded (monomorphized) into one block per flag, in the exact
        // declaration order. `style.ptr` — the null-terminated flag-name string
        // literal — is handed to `ImGui_Text` as its FORMAT string (faithful to
        // the reference; no `%` in any flag name so this is safe). The labeled
        // `break :entry` on `!@field(...)` becomes an `if <flag> { ... }` guard.
        // `style_row!` factors the identical 6-call body; each expansion is a
        // literal inlining of the block above (no runtime tuple, no dispatch).
        macro_rules! style_row {
            ($cond:expr, $name:expr, $help:expr) => {
                if $cond {
                    // cimgui.c.ImGui_TableNextRow();
                    unsafe { dcimgui_sys::ImGui_TableNextRow() };
                    {
                        // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
                        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
                        // cimgui.c.ImGui_Text(style.ptr);
                        unsafe { dcimgui_sys::ImGui_Text($name.as_ptr()) };
                        // cimgui.c.ImGui_SameLine();
                        unsafe { dcimgui_sys::ImGui_SameLine() };
                        // widgets.helpMarker(help);
                        helpMarker($help);
                    }
                    {
                        // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
                        let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
                        // cimgui.c.ImGui_Text("true");
                        unsafe { dcimgui_sys::ImGui_Text(c"true".as_ptr()) };
                    }
                }
            };
        }
        // .{ "bold", "Text will be rendered with bold weight." },
        style_row!(
            st.flags.bold,
            c"bold",
            c"Text will be rendered with bold weight."
        );
        // .{ "italic", "Text will be rendered in italic style." },
        style_row!(
            st.flags.italic,
            c"italic",
            c"Text will be rendered in italic style."
        );
        // .{ "faint", "Text will be rendered with reduced intensity." },
        style_row!(
            st.flags.faint,
            c"faint",
            c"Text will be rendered with reduced intensity."
        );
        // .{ "blink", "Text will blink." },
        style_row!(st.flags.blink, c"blink", c"Text will blink.");
        // .{ "inverse", "Foreground and background colors are swapped." },
        style_row!(
            st.flags.inverse,
            c"inverse",
            c"Foreground and background colors are swapped."
        );
        // .{ "invisible", "Text will be invisible (hidden)." },
        style_row!(
            st.flags.invisible,
            c"invisible",
            c"Text will be invisible (hidden)."
        );
        // .{ "strikethrough", "Text will have a line through it." },
        style_row!(
            st.flags.strikethrough,
            c"strikethrough",
            c"Text will have a line through it."
        );
    }

    // cimgui.c.ImGui_TextDisabled("(Any styles not shown are not currently set)");
    unsafe {
        dcimgui_sys::ImGui_TextDisabled(c"(Any styles not shown are not currently set)".as_ptr())
    };
}

/// Render a style color.
// pub fn color(
//     id: [:0]const u8,
//     c: terminal.Style.Color,
//     palette: ?*const terminal.color.Palette,
// ) void {
// PORT NOTE: `id: [:0]const u8` (null-terminated byte slice) crosses as
// `&CStr`; `.as_ptr()` yields the `*const c_char` `ImGui_PushID` wants.
pub fn color(id: &CStr, c: Color, palette: Option<&Palette>) {
    // cimgui.c.ImGui_PushID(id);
    unsafe { dcimgui_sys::ImGui_PushID(id.as_ptr()) };
    // defer cimgui.c.ImGui_PopID();
    let _pop_id = Defer(|| unsafe { dcimgui_sys::ImGui_PopID() });

    // switch (c) {
    match c {
        // .none => cimgui.c.ImGui_Text("default"),
        Color::None => unsafe { dcimgui_sys::ImGui_Text(c"default".as_ptr()) },

        // .palette => |idx| {
        Color::Palette(idx) => {
            // cimgui.c.ImGui_Text("Palette %d", idx);
            unsafe { dcimgui_sys::ImGui_Text(c"Palette %d".as_ptr(), idx as c_int) };
            // if (palette) |p| {
            if let Some(p) = palette {
                // const rgb = p[idx];
                let rgb = p[idx as usize];
                // var data: [3]f32 = .{
                //     @as(f32, @floatFromInt(rgb.r)) / 255,
                //     @as(f32, @floatFromInt(rgb.g)) / 255,
                //     @as(f32, @floatFromInt(rgb.b)) / 255,
                // };
                let mut data: [f32; 3] = [
                    rgb.r as f32 / 255.0,
                    rgb.g as f32 / 255.0,
                    rgb.b as f32 / 255.0,
                ];
                // _ = cimgui.c.ImGui_ColorEdit3(
                //     "color_fg",
                //     &data,
                //     cimgui.c.ImGuiColorEditFlags_DisplayHex |
                //         cimgui.c.ImGuiColorEditFlags_NoPicker |
                //         cimgui.c.ImGuiColorEditFlags_NoLabel,
                // );
                let _ = unsafe {
                    dcimgui_sys::ImGui_ColorEdit3(
                        c"color_fg".as_ptr(),
                        data.as_mut_ptr(),
                        (dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_DisplayHex
                            | dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_NoPicker
                            | dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_NoLabel)
                            as dcimgui_sys::ImGuiColorEditFlags,
                    )
                };
            }
        }

        // .rgb => |rgb| {
        Color::Rgb(rgb) => {
            // var data: [3]f32 = .{
            //     @as(f32, @floatFromInt(rgb.r)) / 255,
            //     @as(f32, @floatFromInt(rgb.g)) / 255,
            //     @as(f32, @floatFromInt(rgb.b)) / 255,
            // };
            let mut data: [f32; 3] = [
                rgb.r as f32 / 255.0,
                rgb.g as f32 / 255.0,
                rgb.b as f32 / 255.0,
            ];
            // _ = cimgui.c.ImGui_ColorEdit3(
            //     "color_fg",
            //     &data,
            //     cimgui.c.ImGuiColorEditFlags_DisplayHex |
            //         cimgui.c.ImGuiColorEditFlags_NoPicker |
            //         cimgui.c.ImGuiColorEditFlags_NoLabel,
            // );
            let _ = unsafe {
                dcimgui_sys::ImGui_ColorEdit3(
                    c"color_fg".as_ptr(),
                    data.as_mut_ptr(),
                    (dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_DisplayHex
                        | dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_NoPicker
                        | dcimgui_sys::ImGuiColorEditFlags__ImGuiColorEditFlags_NoLabel)
                        as dcimgui_sys::ImGuiColorEditFlags,
                )
            };
        }
    }
}
