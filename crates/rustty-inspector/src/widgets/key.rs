//! Keyboard-event inspector widget data (`Event` / `EventRing` / `label`).
//!
//! PORTS: ghostty src/inspector/widgets/key.zig
//!
//! PORT STATUS: partial — PARTIAL-FILE port (CORE_BLUEPRINT §4 / row C1,
//! [ADV-FIX 7]): the pure-data `Event` + `EventRing` + `label()` surface is
//! LIVE (it is threaded through core's `keyCallback`/`maybeHandleBinding`/
//! `encodeKey` signatures and cannot be deferred without gutting the input hot
//! path); the ImGui-dependent section (`Event.render`/`renderUtf8`/`renderPty`,
//! `modsTooltip`, the `Stream` widget, `actionColor`/`keyColor`/`hsvToRgba`)
//! is DEFERRED to the Phase-3 inspector port with verbatim comment bodies
//! below (DEFERRALS.md "rustty-inspector C1"). dcimgui is NOT wired into this
//! crate.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
use allocator_api2::alloc::Global as Allocator;
// const input = @import("../../input.zig");
// PORT NOTE: the `input` hub splits across two crates here — `input.KeyEvent`
// (+ its `Action`/`Key`/`Mods`) lives in rustty-vt's input slice
// (input/key.rs), while `input.Binding.Action` lives in rustty-input
// (binding.rs).
use rustty_vt::input::key::{Action as KeyAction, Key, KeyEvent};
// const CircBuf = @import("../../datastruct/main.zig").CircBuf;
use rustty_datastruct::CircBuf;
// const cimgui = @import("dcimgui");
// PORT NOTE (DEFERRED, Phase 3): `dcimgui` is consumed only by the deferred
// ImGui section at the bottom of this file; the dcimgui-sys binding is not
// linked by rustty-inspector until the Phase-3 GUI port.

/// Error set for this file's fallible functions, mirroring the reference's
/// inferred error sets: `Event.init`'s `!Event` is `Allocator.Error`
/// (`error.OutOfMemory` from `alloc.dupe`) and `Event.label`'s
/// `![:0]const u8` is the fixed-buffer writer's `error.NoSpaceLeft`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `Allocator.Error` — never constructed under Rust's infallible global
    /// allocator; preserved for call-shape parity.
    OutOfMemory,
    /// `error.NoSpaceLeft` — `label`'s buffer was exhausted.
    NoSpaceLeft,
}

/// Circular buffer of key events.
// PORT NOTE: `CircBuf(Event, undefined)` — the comptime `undefined` fill value
// maps to the `default: T` supplied at `CircBuf::init` (see the datastruct
// circ_buf.rs PORT NOTE); the Phase-3 `Stream.init` passes `Event::default()`.
pub type EventRing = CircBuf<Event>;

/// Represents a recorded keyboard event.
// PORT NOTE: `Clone` is required by the `CircBuf<T: Clone>` element bound (the
// Zig CircBuf memcpy-copies elements). `Default` reifies the reference field
// defaults (`binding = &.{}`, `pty = ""`, `imgui_state = .{}`) plus
// `KeyEvent`'s own default for the defaultless `event` field, standing in for
// the `undefined` CircBuf fill above.
#[derive(Clone, Debug, Default)]
pub struct Event {
    /// The input event.
    pub event: KeyEvent,

    /// The binding that was triggered as a result of this event.
    /// Multiple bindings are possible if they are chained.
    // (reference field default: `&.{}`)
    pub binding: Vec<rustty_input::binding::Action>,

    /// The data sent to the pty as a result of this keyboard event.
    /// This is allocated using the inspector allocator.
    // (reference field default: `""`; byte slice -> owned `Vec<u8>`)
    pub pty: Vec<u8>,

    /// State for the inspector GUI. Do not set this unless you're the inspector.
    // PORT NOTE: the reference type is an anonymous inline struct
    // (`struct { selected: bool = false }`); Rust requires the named
    // `ImguiState` below (field default: `.{}`).
    pub imgui_state: ImguiState,
}

/// The named type for `Event.imgui_state`'s anonymous inline struct
/// (`struct { selected: bool = false }`). Plain data — no FFI.
#[derive(Clone, Copy, Debug, Default)]
pub struct ImguiState {
    // selected: bool = false,
    pub selected: bool,
}

impl Event {
    // pub fn init(alloc: Allocator, ev: input.KeyEvent) !Event {
    // PORT NOTE: Zig passes `ev` by value (a shallow copy; the caller keeps
    // its `utf8` slice). The Rust `KeyEvent` OWNS `utf8: Vec<u8>`, so `ev`
    // comes in by reference and the "copy with empty utf8, then dupe" shape
    // is preserved line-for-line below. `alloc.dupe` maps to `clone()` under
    // the infallible global allocator (`_alloc` retained for signature
    // parity — termio command.rs precedent).
    pub fn init(_alloc: Allocator, ev: &KeyEvent) -> Result<Event, Error> {
        // var copy = ev;
        // copy.utf8 = "";
        let mut copy = ev.clone();
        copy.utf8 = Vec::new();
        // if (ev.utf8.len > 0) copy.utf8 = try alloc.dupe(u8, ev.utf8);
        if !ev.utf8.is_empty() {
            copy.utf8 = ev.utf8.clone();
        }
        // return .{ .event = copy };
        Ok(Event {
            event: copy,
            ..Default::default()
        })
    }

    // pub fn deinit(self: *const Event, alloc: Allocator) void {
    // PORT NOTE: the reference frees its slices through a `*const Event` (Zig
    // frees slice values regardless of pointer constness); the owned `Vec`s
    // require `&mut self` here, and each `alloc.free` is the drop of the
    // replaced `Vec`.
    pub fn deinit(&mut self, _alloc: Allocator) {
        // alloc.free(self.binding);
        self.binding = Vec::new();
        // if (self.event.utf8.len > 0) alloc.free(self.event.utf8);
        if !self.event.utf8.is_empty() {
            self.event.utf8 = Vec::new();
        }
        // if (self.pty.len > 0) alloc.free(self.pty);
        if !self.pty.is_empty() {
            self.pty = Vec::new();
        }
    }

    /// Returns a label that can be used for this event. This is null-terminated
    /// so it can be easily used with C APIs.
    // pub fn label(self: *const Event, buf: []u8) ![:0]const u8 {
    // PORT NOTE: `[:0]const u8` -> the written bytes (WITHOUT the trailing
    // NUL) sliced out of `buf`, with the NUL written into `buf` for C-API
    // parity (rustty-os locale.rs sentinel precedent). The reference's
    // fixed-buffer writer partially fills `buf` before `error.NoSpaceLeft`;
    // this port's writer checks the whole write first — the on-error `buf`
    // contents are never observed by any caller.
    pub fn label<'a>(&self, buf: &'a mut [u8]) -> Result<&'a [u8], Error> {
        // var buf_stream = std.io.fixedBufferStream(buf);
        // const writer = buf_stream.writer();
        // (PORT NOTE: the fixed-buffer writer — `pos` tracks `buf_stream.pos`.)
        let mut pos: usize = 0;
        fn write_all(buf: &mut [u8], pos: &mut usize, bytes: &[u8]) -> Result<(), Error> {
            if *pos + bytes.len() > buf.len() {
                return Err(Error::NoSpaceLeft);
            }
            buf[*pos..*pos + bytes.len()].copy_from_slice(bytes);
            *pos += bytes.len();
            Ok(())
        }

        // switch (self.event.action) {
        //     .press => try writer.writeAll("Press: "),
        //     .release => try writer.writeAll("Release: "),
        //     .repeat => try writer.writeAll("Repeat: "),
        // }
        match self.event.action {
            KeyAction::Press => write_all(buf, &mut pos, b"Press: ")?,
            KeyAction::Release => write_all(buf, &mut pos, b"Release: ")?,
            KeyAction::Repeat => write_all(buf, &mut pos, b"Repeat: ")?,
        }

        // if (self.event.mods.shift) try writer.writeAll("Shift+");
        if self.event.mods.shift {
            write_all(buf, &mut pos, b"Shift+")?;
        }
        // if (self.event.mods.ctrl) try writer.writeAll("Ctrl+");
        if self.event.mods.ctrl {
            write_all(buf, &mut pos, b"Ctrl+")?;
        }
        // if (self.event.mods.alt) try writer.writeAll("Alt+");
        if self.event.mods.alt {
            write_all(buf, &mut pos, b"Alt+")?;
        }
        // if (self.event.mods.super) try writer.writeAll("Super+");
        if self.event.mods.super_ {
            write_all(buf, &mut pos, b"Super+")?;
        }

        // Write our key. If we have an invalid key we attempt to write
        // the utf8 associated with it if we have it to handle non-ascii.
        // try writer.writeAll(switch (self.event.key) {
        //     .unidentified => if (self.event.utf8.len > 0) self.event.utf8 else @tagName(self.event.key),
        //     else => @tagName(self.event.key),
        // });
        write_all(
            buf,
            &mut pos,
            match self.event.key {
                Key::Unidentified => {
                    if !self.event.utf8.is_empty() {
                        self.event.utf8.as_slice()
                    } else {
                        self.event.key.tag_name().as_bytes()
                    }
                }
                _ => self.event.key.tag_name().as_bytes(),
            },
        )?;

        // Deadkey
        // if (self.event.composing) try writer.writeAll(" (composing)");
        if self.event.composing {
            write_all(buf, &mut pos, b" (composing)")?;
        }

        // Null-terminator
        // try writer.writeByte(0);
        write_all(buf, &mut pos, &[0])?;
        // return buf[0..(buf_stream.getWritten().len - 1) :0];
        Ok(&buf[0..pos - 1])
    }
}

// DEFERRED (Phase-3 inspector GUI over dcimgui — CORE_BLUEPRINT §4 / C1
// [ADV-FIX 7], 0 tests — R1-safe): everything below this point is the
// ImGui-dependent section of key.zig (the `Event` render methods, the
// `modsTooltip` helper, the `Stream` widget, and the row-color helpers).
// It is preserved verbatim so the Phase-3 port fills it in place:
//
//    /// Render this event in the inspector GUI.
//    pub fn render(self: *const Event) void {
//        _ = cimgui.c.ImGui_BeginTable(
//            "##event",
//            2,
//            cimgui.c.ImGuiTableFlags_None,
//        );
//        defer cimgui.c.ImGui_EndTable();
//
//        if (self.binding.len > 0) {
//            cimgui.c.ImGui_TableNextRow();
//            _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//            cimgui.c.ImGui_Text("Triggered Binding");
//            _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//
//            const height: f32 = height: {
//                const item_count: f32 = @floatFromInt(@min(self.binding.len, 5));
//                const padding = cimgui.c.ImGui_GetStyle().*.FramePadding.y * 2;
//                break :height cimgui.c.ImGui_GetTextLineHeightWithSpacing() * item_count + padding;
//            };
//            if (cimgui.c.ImGui_BeginListBox("##bindings", .{ .x = 0, .y = height })) {
//                defer cimgui.c.ImGui_EndListBox();
//                for (self.binding) |action| {
//                    _ = cimgui.c.ImGui_SelectableEx(
//                        @tagName(action).ptr,
//                        false,
//                        cimgui.c.ImGuiSelectableFlags_None,
//                        .{ .x = 0, .y = 0 },
//                    );
//                }
//            }
//        }
//
//        pty: {
//            cimgui.c.ImGui_TableNextRow();
//            _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//            cimgui.c.ImGui_Text("Encoding to Pty");
//            _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//            if (self.pty.len == 0) {
//                cimgui.c.ImGui_TextDisabled("(no data)");
//                break :pty;
//            }
//
//            self.renderPty() catch {
//                cimgui.c.ImGui_TextDisabled("(error rendering pty data)");
//                break :pty;
//            };
//        }
//
//        {
//            cimgui.c.ImGui_TableNextRow();
//            _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//            cimgui.c.ImGui_Text("Action");
//            _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//            cimgui.c.ImGui_Text("%s", @tagName(self.event.action).ptr);
//        }
//        {
//            cimgui.c.ImGui_TableNextRow();
//            _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//            cimgui.c.ImGui_Text("Key");
//            _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//            cimgui.c.ImGui_Text("%s", @tagName(self.event.key).ptr);
//        }
//        if (!self.event.mods.empty()) {
//            cimgui.c.ImGui_TableNextRow();
//            _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//            cimgui.c.ImGui_Text("Mods");
//            _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//            if (self.event.mods.shift) cimgui.c.ImGui_Text("shift ");
//            if (self.event.mods.ctrl) cimgui.c.ImGui_Text("ctrl ");
//            if (self.event.mods.alt) cimgui.c.ImGui_Text("alt ");
//            if (self.event.mods.super) cimgui.c.ImGui_Text("super ");
//        }
//        if (self.event.composing) {
//            cimgui.c.ImGui_TableNextRow();
//            _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//            cimgui.c.ImGui_Text("Composing");
//            _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//            cimgui.c.ImGui_Text("true");
//        }
//        utf8: {
//            cimgui.c.ImGui_TableNextRow();
//            _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//            cimgui.c.ImGui_Text("UTF-8");
//            _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//            if (self.event.utf8.len == 0) {
//                cimgui.c.ImGui_TextDisabled("(empty)");
//                break :utf8;
//            }
//
//            self.renderUtf8(self.event.utf8) catch {
//                cimgui.c.ImGui_TextDisabled("(error rendering utf-8)");
//                break :utf8;
//            };
//        }
//    }
//
//    fn renderUtf8(self: *const Event, utf8: []const u8) !void {
//        _ = self;
//
//        // Format the codepoint sequence
//        var buf: [1024]u8 = undefined;
//        var buf_stream = std.io.fixedBufferStream(&buf);
//        const writer = buf_stream.writer();
//        if (std.unicode.Utf8View.init(utf8)) |view| {
//            var it = view.iterator();
//            while (it.nextCodepoint()) |cp| {
//                try writer.print("U+{X} ", .{cp});
//            }
//        } else |_| {
//            try writer.writeAll("(invalid utf-8)");
//        }
//        try writer.writeByte(0);
//
//        // Render as a textbox
//        _ = cimgui.c.ImGui_InputText(
//            "##utf8",
//            &buf,
//            buf_stream.getWritten().len - 1,
//            cimgui.c.ImGuiInputTextFlags_ReadOnly,
//        );
//    }
//
//    fn renderPty(self: *const Event) !void {
//        // Format the codepoint sequence
//        var buf: [1024]u8 = undefined;
//        var buf_stream = std.io.fixedBufferStream(&buf);
//        const writer = buf_stream.writer();
//
//        for (self.pty) |byte| {
//            // Print ESC special because its so common
//            if (byte == 0x1B) {
//                try writer.writeAll("ESC ");
//                continue;
//            }
//
//            // Print ASCII as-is
//            if (byte > 0x20 and byte < 0x7F) {
//                try writer.writeByte(byte);
//                continue;
//            }
//
//            // Everything else as a hex byte
//            try writer.print("0x{X} ", .{byte});
//        }
//
//        try writer.writeByte(0);
//
//        // Render as a textbox
//        _ = cimgui.c.ImGui_InputText(
//            "##pty",
//            &buf,
//            buf_stream.getWritten().len - 1,
//            cimgui.c.ImGuiInputTextFlags_ReadOnly,
//        );
//    }
//
// (the two functions above close the reference's `pub const Event = struct`)
//
// fn modsTooltip(
//     mods: *const input.Mods,
//     buf: []u8,
// ) ![:0]const u8 {
//     var stream = std.io.fixedBufferStream(buf);
//     const writer = stream.writer();
//     var first = true;
//     if (mods.shift) {
//         try writer.writeAll("Shift");
//         first = false;
//     }
//     if (mods.ctrl) {
//         if (!first) try writer.writeAll("+");
//         try writer.writeAll("Ctrl");
//         first = false;
//     }
//     if (mods.alt) {
//         if (!first) try writer.writeAll("+");
//         try writer.writeAll("Alt");
//         first = false;
//     }
//     if (mods.super) {
//         if (!first) try writer.writeAll("+");
//         try writer.writeAll("Super");
//     }
//     try writer.writeByte(0);
//     const written = stream.getWritten();
//     return written[0 .. written.len - 1 :0];
// }
//
// /// Keyboard event stream inspector widget.
// pub const Stream = struct {
//     events: EventRing,
//
//     pub fn init(alloc: Allocator) !Stream {
//         var events: EventRing = try .init(alloc, 2);
//         errdefer events.deinit(alloc);
//         return .{ .events = events };
//     }
//
//     pub fn deinit(self: *Stream, alloc: Allocator) void {
//         var it = self.events.iterator(.forward);
//         while (it.next()) |v| v.deinit(alloc);
//         self.events.deinit(alloc);
//     }
//
//     pub fn draw(
//         self: *Stream,
//         open: bool,
//         alloc: Allocator,
//     ) void {
//         if (!open) return;
//
//         if (self.events.empty()) {
//             cimgui.c.ImGui_Text("No recorded key events. Press a key with the " ++
//                 "terminal focused to record it.");
//             return;
//         }
//
//         if (cimgui.c.ImGui_Button("Clear")) {
//             var it = self.events.iterator(.forward);
//             while (it.next()) |v| v.deinit(alloc);
//             self.events.clear();
//         }
//
//         cimgui.c.ImGui_Separator();
//
//         const table_flags = cimgui.c.ImGuiTableFlags_Borders |
//             cimgui.c.ImGuiTableFlags_Resizable |
//             cimgui.c.ImGuiTableFlags_ScrollY |
//             cimgui.c.ImGuiTableFlags_SizingFixedFit;
//
//         if (!cimgui.c.ImGui_BeginTable("table_key_events", 6, table_flags)) return;
//         defer cimgui.c.ImGui_EndTable();
//
//         cimgui.c.ImGui_TableSetupScrollFreeze(0, 1);
//         cimgui.c.ImGui_TableSetupColumnEx("Action", cimgui.c.ImGuiTableColumnFlags_WidthFixed, 80, 0);
//         cimgui.c.ImGui_TableSetupColumnEx("Key", cimgui.c.ImGuiTableColumnFlags_WidthFixed, 160, 0);
//         cimgui.c.ImGui_TableSetupColumnEx("Mods", cimgui.c.ImGuiTableColumnFlags_WidthFixed, 150, 0);
//         cimgui.c.ImGui_TableSetupColumnEx("UTF-8", cimgui.c.ImGuiTableColumnFlags_WidthFixed, 80, 0);
//         cimgui.c.ImGui_TableSetupColumnEx("PTY Encoding", cimgui.c.ImGuiTableColumnFlags_WidthStretch, 0, 0);
//         cimgui.c.ImGui_TableSetupColumnEx("Binding", cimgui.c.ImGuiTableColumnFlags_WidthStretch, 0, 0);
//         cimgui.c.ImGui_TableHeadersRow();
//
//         var it = self.events.iterator(.reverse);
//         while (it.next()) |ev| {
//             cimgui.c.ImGui_PushIDPtr(ev);
//             defer cimgui.c.ImGui_PopID();
//
//             cimgui.c.ImGui_TableNextRow();
//             const row_min_y = cimgui.c.ImGui_GetCursorScreenPos().y;
//
//             // Set row background color based on action
//             cimgui.c.ImGui_TableSetBgColor(cimgui.c.ImGuiTableBgTarget_RowBg0, actionColor(ev.event.action), -1);
//
//             // Action column with colored text
//             _ = cimgui.c.ImGui_TableSetColumnIndex(0);
//             const action_text_color: cimgui.c.ImVec4 = switch (ev.event.action) {
//                 .press => .{ .x = 0.4, .y = 1.0, .z = 0.4, .w = 1.0 }, // Green
//                 .release => .{ .x = 0.6, .y = 0.6, .z = 1.0, .w = 1.0 }, // Blue
//                 .repeat => .{ .x = 1.0, .y = 1.0, .z = 0.4, .w = 1.0 }, // Yellow
//             };
//             cimgui.c.ImGui_TextColored(action_text_color, "%s", @tagName(ev.event.action).ptr);
//
//             // Key column with consistent key coloring
//             _ = cimgui.c.ImGui_TableSetColumnIndex(1);
//             const key_name = switch (ev.event.key) {
//                 .unidentified => if (ev.event.utf8.len > 0) ev.event.utf8 else @tagName(ev.event.key),
//                 else => @tagName(ev.event.key),
//             };
//             const key_rgba = keyColor(ev.event.key);
//             const key_color: cimgui.c.ImVec4 = .{
//                 .x = @as(f32, @floatFromInt(key_rgba & 0xFF)) / 255.0,
//                 .y = @as(f32, @floatFromInt((key_rgba >> 8) & 0xFF)) / 255.0,
//                 .z = @as(f32, @floatFromInt((key_rgba >> 16) & 0xFF)) / 255.0,
//                 .w = 1.0,
//             };
//             cimgui.c.ImGui_TextColored(key_color, "%s", key_name.ptr);
//
//             // Composing indicator
//             if (ev.event.composing) {
//                 cimgui.c.ImGui_SameLine();
//                 cimgui.c.ImGui_TextColored(.{ .x = 1.0, .y = 0.6, .z = 0.0, .w = 1.0 }, "*");
//                 if (cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_None)) {
//                     cimgui.c.ImGui_SetTooltip("Composing (dead key)");
//                 }
//             }
//
//             // Mods
//             _ = cimgui.c.ImGui_TableSetColumnIndex(2);
//             mods: {
//                 if (ev.event.mods.empty()) {
//                     cimgui.c.ImGui_TextDisabled("-");
//                     break :mods;
//                 }
//
//                 var any_hovered = false;
//                 if (ev.event.mods.shift) {
//                     _ = cimgui.c.ImGui_SmallButton("S");
//                     any_hovered = any_hovered or cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_None);
//                     cimgui.c.ImGui_SameLine();
//                 }
//                 if (ev.event.mods.ctrl) {
//                     _ = cimgui.c.ImGui_SmallButton("C");
//                     any_hovered = any_hovered or cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_None);
//                     cimgui.c.ImGui_SameLine();
//                 }
//                 if (ev.event.mods.alt) {
//                     _ = cimgui.c.ImGui_SmallButton("A");
//                     any_hovered = any_hovered or cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_None);
//                     cimgui.c.ImGui_SameLine();
//                 }
//                 if (ev.event.mods.super) {
//                     _ = cimgui.c.ImGui_SmallButton("M");
//                     any_hovered = any_hovered or cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_None);
//                     cimgui.c.ImGui_SameLine();
//                 }
//                 cimgui.c.ImGui_NewLine();
//
//                 if (any_hovered) tooltip: {
//                     var tooltip_buf: [64]u8 = undefined;
//                     const tooltip = modsTooltip(
//                         &ev.event.mods,
//                         &tooltip_buf,
//                     ) catch break :tooltip;
//                     cimgui.c.ImGui_SetTooltip("%s", tooltip.ptr);
//                 }
//             }
//
//             // UTF-8
//             _ = cimgui.c.ImGui_TableSetColumnIndex(3);
//             if (ev.event.utf8.len == 0) {
//                 cimgui.c.ImGui_TextDisabled("-");
//             } else {
//                 var utf8_buf: [128]u8 = undefined;
//                 var utf8_stream = std.io.fixedBufferStream(&utf8_buf);
//                 const utf8_writer = utf8_stream.writer();
//                 if (std.unicode.Utf8View.init(ev.event.utf8)) |view| {
//                     var utf8_it = view.iterator();
//                     while (utf8_it.nextCodepoint()) |cp| {
//                         utf8_writer.print("U+{X} ", .{cp}) catch break;
//                     }
//                 } else |_| {
//                     utf8_writer.writeAll("?") catch {};
//                 }
//                 utf8_writer.writeByte(0) catch {};
//                 cimgui.c.ImGui_Text("%s", &utf8_buf);
//             }
//
//             // PTY
//             _ = cimgui.c.ImGui_TableSetColumnIndex(4);
//             if (ev.pty.len == 0) {
//                 cimgui.c.ImGui_TextDisabled("-");
//             } else {
//                 var pty_buf: [256]u8 = undefined;
//                 var pty_stream = std.io.fixedBufferStream(&pty_buf);
//                 const pty_writer = pty_stream.writer();
//                 for (ev.pty) |byte| {
//                     if (byte == 0x1B) {
//                         pty_writer.writeAll("ESC ") catch break;
//                     } else if (byte > 0x20 and byte < 0x7F) {
//                         pty_writer.writeByte(byte) catch break;
//                     } else {
//                         pty_writer.print("0x{X} ", .{byte}) catch break;
//                     }
//                 }
//                 pty_writer.writeByte(0) catch {};
//                 cimgui.c.ImGui_Text("%s", &pty_buf);
//             }
//
//             // Binding
//             _ = cimgui.c.ImGui_TableSetColumnIndex(5);
//             if (ev.binding.len == 0) {
//                 cimgui.c.ImGui_TextDisabled("-");
//             } else {
//                 var binding_buf: [256]u8 = undefined;
//                 var binding_stream = std.io.fixedBufferStream(&binding_buf);
//                 const binding_writer = binding_stream.writer();
//                 for (ev.binding, 0..) |action, i| {
//                     if (i > 0) binding_writer.writeAll(", ") catch break;
//                     binding_writer.writeAll(@tagName(action)) catch break;
//                 }
//                 binding_writer.writeByte(0) catch {};
//                 cimgui.c.ImGui_Text("%s", &binding_buf);
//             }
//
//             // Row hover highlight
//             const row_max_y = cimgui.c.ImGui_GetCursorScreenPos().y;
//             const mouse_pos = cimgui.c.ImGui_GetMousePos();
//             if (mouse_pos.y >= row_min_y and mouse_pos.y < row_max_y) {
//                 cimgui.c.ImGui_TableSetBgColor(cimgui.c.ImGuiTableBgTarget_RowBg1, 0x1AFFFFFF, -1);
//             }
//         }
//     }
// };
//
// /// Returns row background color for an action (ABGR format for ImGui)
// fn actionColor(action: input.Action) u32 {
//     return switch (action) {
//         .press => 0x1A4A6F4A, // Muted sage green
//         .release => 0x1A6A5A5A, // Muted slate gray
//         .repeat => 0x1A4A5A6F, // Muted warm brown
//     };
// }
//
// /// Generate a consistent color for a key based on its enum value.
// /// Uses HSV color space with fixed saturation and value for pleasing colors.
// fn keyColor(key: input.Key) u32 {
//     const key_int: u32 = @intCast(@intFromEnum(key));
//     const hue: f32 = @as(f32, @floatFromInt(key_int *% 47)) / 256.0;
//     return hsvToRgba(hue, 0.5, 0.9, 1.0);
// }
//
// /// Convert HSV (hue 0-1, saturation 0-1, value 0-1) to RGBA u32.
// fn hsvToRgba(h: f32, s: f32, v: f32, a: f32) u32 {
//     var r: f32 = undefined;
//     var g: f32 = undefined;
//     var b: f32 = undefined;
//
//     const i: u32 = @intFromFloat(h * 6.0);
//     const f = h * 6.0 - @as(f32, @floatFromInt(i));
//     const p = v * (1.0 - s);
//     const q = v * (1.0 - f * s);
//     const t = v * (1.0 - (1.0 - f) * s);
//
//     switch (i % 6) {
//         0 => {
//             r = v;
//             g = t;
//             b = p;
//         },
//         1 => {
//             r = q;
//             g = v;
//             b = p;
//         },
//         2 => {
//             r = p;
//             g = v;
//             b = t;
//         },
//         3 => {
//             r = p;
//             g = q;
//             b = v;
//         },
//         4 => {
//             r = t;
//             g = p;
//             b = v;
//         },
//         else => {
//             r = v;
//             g = p;
//             b = q;
//         },
//     }
//
//     const ri: u32 = @intFromFloat(r * 255.0);
//     const gi: u32 = @intFromFloat(g * 255.0);
//     const bi: u32 = @intFromFloat(b * 255.0);
//     const ai: u32 = @intFromFloat(a * 255.0);
//
//     return (ai << 24) | (bi << 16) | (gi << 8) | ri;
// }
