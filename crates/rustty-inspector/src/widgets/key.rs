//! Keyboard-event inspector widget data (`Event` / `EventRing` / `label`).
//!
//! PORTS: ghostty src/inspector/widgets/key.zig
//!
//! PORT STATUS: LIVE (full-file port). The pure-data `Event` + `EventRing` +
//! `label()` surface is threaded through core's `keyCallback`/
//! `maybeHandleBinding`/`encodeKey` signatures; the ImGui-dependent section
//! (`Event.render`/`renderUtf8`/`renderPty`, `modsTooltip`, the `Stream`
//! widget, `actionColor`/`keyColor`/`hsvToRgba`) was ported in Task 8 against
//! `dcimgui-sys`, filled in place below.

// const std = @import("std");
// const Allocator = std.mem.Allocator;
use allocator_api2::alloc::Global as Allocator;
// const input = @import("../../input.zig");
// PORT NOTE: the `input` hub splits across two crates here — `input.KeyEvent`
// (+ its `Action`/`Key`/`Mods`) lives in rustty-vt's input slice
// (input/key.rs), while `input.Binding.Action` lives in rustty-input
// (binding.rs).
use rustty_vt::input::key::{Action as KeyAction, Key, KeyEvent, Mods};
// const CircBuf = @import("../../datastruct/main.zig").CircBuf;
use rustty_datastruct::CircBuf;
use rustty_datastruct::circ_buf::Direction;
// const cimgui = @import("dcimgui");
// PORT NOTE (Task 8): `dcimgui` is the vendored Dear ImGui C API; the port
// calls it through `dcimgui-sys` (full `dcimgui_sys::` paths, matching
// `widgets.rs`). Every ImGui-consuming body below runs on the main thread with
// the surface's ImGui context current (INV-11).
use core::ffi::{c_char, c_void};
use core::fmt::Write as _;

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

// ---------------------------------------------------------------------------
// ImGui draw half (Task 8) — the dcimgui-consuming section of key.zig: the
// `Event` render methods, the `modsTooltip` helper, the `Stream` widget, and the
// row-color helpers (`actionColor`/`keyColor`/`hsvToRgba`). Ported line-for-line
// (R1) against `dcimgui-sys`; verbatim Zig is carried as ported comments.

/// Minimal `defer`-style scope guard — runs `f` on drop (mirrors the private
/// `Defer` in `widgets.rs`). Backs the ImGui `Begin*`/`End*` and `Push*`/`Pop*`
/// pairings below so they fire on every exit path (including the labeled-block
/// early exits). panic=abort means no unwinding ever crosses these FFI calls, so
/// drop-on-scope-exit is the sole runtime path (no double-fire, no leak).
struct Defer<F: FnMut()>(F);

impl<F: FnMut()> Drop for Defer<F> {
    fn drop(&mut self) {
        (self.0)();
    }
}

/// Fixed-buffer byte writer — the port of the reference's
/// `std.io.fixedBufferStream(&buf).writer()` backing the scratch buffers in
/// `renderUtf8`/`renderPty`/`modsTooltip` and the `Stream` draw. `write_all`/
/// `write_byte` return `Err(())` on overflow (the reference's
/// `error.NoSpaceLeft`), which callers either propagate (the `!void` methods) or
/// swallow (`catch break`/`catch {}`). Implements `core::fmt::Write` so the
/// `writer.print("U+{X} ", .{cp})` / `print("0x{X} ", .{byte})` sites port as
/// `write!(..)`.
struct FixedBuf<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> FixedBuf<'a> {
    fn new(buf: &'a mut [u8]) -> FixedBuf<'a> {
        FixedBuf { buf, pos: 0 }
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ()> {
        if self.pos + bytes.len() > self.buf.len() {
            return Err(());
        }
        self.buf[self.pos..self.pos + bytes.len()].copy_from_slice(bytes);
        self.pos += bytes.len();
        Ok(())
    }

    fn write_byte(&mut self, b: u8) -> Result<(), ()> {
        self.write_all(&[b])
    }

    /// `buf_stream.getWritten()` — the filled prefix.
    fn written(&self) -> &[u8] {
        &self.buf[..self.pos]
    }
}

impl core::fmt::Write for FixedBuf<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.write_all(s.as_bytes()).map_err(|()| core::fmt::Error)
    }
}

/// Copy `bytes` + a trailing NUL into `buf` and return a `*const c_char` to it.
/// Ports the null-terminated `@tagName(...).ptr` / `utf8.ptr` handed to the ImGui
/// `"%s"` text calls: Zig `@tagName` and string literals are NUL-terminated, a
/// Rust `&str`/`&[u8]` is not. Truncates to fit — the scratch buffers here are
/// sized well above any key/tag name.
fn cstr_ptr(buf: &mut [u8], bytes: &[u8]) -> *const c_char {
    let n = bytes.len().min(buf.len().saturating_sub(1));
    buf[..n].copy_from_slice(&bytes[..n]);
    buf[n] = 0;
    buf.as_ptr() as *const c_char
}

/// The reference's inline `@tagName(self.event.action)` — the NUL-terminated
/// lowercase tag string of the key action (Zig enum fields
/// `release`/`press`/`repeat`). Rust has no `@tagName` for this `#[repr(C)]`
/// enum, so it is hand-expanded to the matching C literals.
fn action_tag_name(action: KeyAction) -> &'static core::ffi::CStr {
    match action {
        KeyAction::Release => c"release",
        KeyAction::Press => c"press",
        KeyAction::Repeat => c"repeat",
    }
}

impl Event {
    /// Render this event in the inspector GUI.
    // pub fn render(self: *const Event) void {
    #[allow(non_snake_case)] // faithful Zig name (R1)
    pub fn render(&self) {
        // _ = cimgui.c.ImGui_BeginTable(
        //     "##event",
        //     2,
        //     cimgui.c.ImGuiTableFlags_None,
        // );
        let _ = unsafe {
            dcimgui_sys::ImGui_BeginTable(
                c"##event".as_ptr(),
                2,
                dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_None as dcimgui_sys::ImGuiTableFlags,
            )
        };
        // defer cimgui.c.ImGui_EndTable();
        let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

        // if (self.binding.len > 0) {
        if !self.binding.is_empty() {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Triggered Binding");
            unsafe { dcimgui_sys::ImGui_Text(c"Triggered Binding".as_ptr()) };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };

            // const height: f32 = height: {
            //     const item_count: f32 = @floatFromInt(@min(self.binding.len, 5));
            //     const padding = cimgui.c.ImGui_GetStyle().*.FramePadding.y * 2;
            //     break :height cimgui.c.ImGui_GetTextLineHeightWithSpacing() * item_count + padding;
            // };
            let height: f32 = {
                let item_count: f32 = self.binding.len().min(5) as f32;
                let padding = unsafe { (*dcimgui_sys::ImGui_GetStyle()).FramePadding.y } * 2.0;
                (unsafe { dcimgui_sys::ImGui_GetTextLineHeightWithSpacing() }) * item_count + padding
            };
            // if (cimgui.c.ImGui_BeginListBox("##bindings", .{ .x = 0, .y = height })) {
            if unsafe {
                dcimgui_sys::ImGui_BeginListBox(
                    c"##bindings".as_ptr(),
                    dcimgui_sys::ImVec2 { x: 0.0, y: height },
                )
            } {
                // defer cimgui.c.ImGui_EndListBox();
                let _end_listbox = Defer(|| unsafe { dcimgui_sys::ImGui_EndListBox() });
                // for (self.binding) |action| {
                for action in &self.binding {
                    // _ = cimgui.c.ImGui_SelectableEx(
                    //     @tagName(action).ptr,
                    //     false,
                    //     cimgui.c.ImGuiSelectableFlags_None,
                    //     .{ .x = 0, .y = 0 },
                    // );
                    let mut name_buf = [0u8; 256];
                    let _ = unsafe {
                        dcimgui_sys::ImGui_SelectableEx(
                            cstr_ptr(&mut name_buf, action.tag_name().as_bytes()),
                            false,
                            dcimgui_sys::ImGuiSelectableFlags__ImGuiSelectableFlags_None
                                as dcimgui_sys::ImGuiSelectableFlags,
                            dcimgui_sys::ImVec2 { x: 0.0, y: 0.0 },
                        )
                    };
                }
            }
        }

        // pty: {
        'pty: {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Encoding to Pty");
            unsafe { dcimgui_sys::ImGui_Text(c"Encoding to Pty".as_ptr()) };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // if (self.pty.len == 0) {
            //     cimgui.c.ImGui_TextDisabled("(no data)");
            //     break :pty;
            // }
            if self.pty.is_empty() {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(no data)".as_ptr()) };
                break 'pty;
            }

            // self.renderPty() catch {
            //     cimgui.c.ImGui_TextDisabled("(error rendering pty data)");
            //     break :pty;
            // };
            if self.renderPty().is_err() {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(error rendering pty data)".as_ptr()) };
                break 'pty;
            }
        }

        {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Action");
            unsafe { dcimgui_sys::ImGui_Text(c"Action".as_ptr()) };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text("%s", @tagName(self.event.action).ptr);
            unsafe {
                dcimgui_sys::ImGui_Text(c"%s".as_ptr(), action_tag_name(self.event.action).as_ptr())
            };
        }
        {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Key");
            unsafe { dcimgui_sys::ImGui_Text(c"Key".as_ptr()) };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text("%s", @tagName(self.event.key).ptr);
            let mut key_buf = [0u8; 256];
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"%s".as_ptr(),
                    cstr_ptr(&mut key_buf, self.event.key.tag_name().as_bytes()),
                )
            };
        }
        // if (!self.event.mods.empty()) {
        if !self.event.mods.empty() {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Mods");
            unsafe { dcimgui_sys::ImGui_Text(c"Mods".as_ptr()) };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // if (self.event.mods.shift) cimgui.c.ImGui_Text("shift ");
            if self.event.mods.shift {
                unsafe { dcimgui_sys::ImGui_Text(c"shift ".as_ptr()) };
            }
            // if (self.event.mods.ctrl) cimgui.c.ImGui_Text("ctrl ");
            if self.event.mods.ctrl {
                unsafe { dcimgui_sys::ImGui_Text(c"ctrl ".as_ptr()) };
            }
            // if (self.event.mods.alt) cimgui.c.ImGui_Text("alt ");
            if self.event.mods.alt {
                unsafe { dcimgui_sys::ImGui_Text(c"alt ".as_ptr()) };
            }
            // if (self.event.mods.super) cimgui.c.ImGui_Text("super ");
            if self.event.mods.super_ {
                unsafe { dcimgui_sys::ImGui_Text(c"super ".as_ptr()) };
            }
        }
        // if (self.event.composing) {
        if self.event.composing {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("Composing");
            unsafe { dcimgui_sys::ImGui_Text(c"Composing".as_ptr()) };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // cimgui.c.ImGui_Text("true");
            unsafe { dcimgui_sys::ImGui_Text(c"true".as_ptr()) };
        }
        // utf8: {
        'utf8: {
            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // cimgui.c.ImGui_Text("UTF-8");
            unsafe { dcimgui_sys::ImGui_Text(c"UTF-8".as_ptr()) };
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // if (self.event.utf8.len == 0) {
            //     cimgui.c.ImGui_TextDisabled("(empty)");
            //     break :utf8;
            // }
            if self.event.utf8.is_empty() {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(empty)".as_ptr()) };
                break 'utf8;
            }

            // self.renderUtf8(self.event.utf8) catch {
            //     cimgui.c.ImGui_TextDisabled("(error rendering utf-8)");
            //     break :utf8;
            // };
            if self.renderUtf8(&self.event.utf8).is_err() {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"(error rendering utf-8)".as_ptr()) };
                break 'utf8;
            }
        }
    }

    // fn renderUtf8(self: *const Event, utf8: []const u8) !void {
    #[allow(non_snake_case)] // faithful Zig name (R1)
    fn renderUtf8(&self, utf8: &[u8]) -> Result<(), Error> {
        // _ = self;

        // Format the codepoint sequence
        // var buf: [1024]u8 = undefined;
        // var buf_stream = std.io.fixedBufferStream(&buf);
        // const writer = buf_stream.writer();
        let mut buf = [0u8; 1024];
        let written_len;
        {
            let mut stream = FixedBuf::new(&mut buf);
            // if (std.unicode.Utf8View.init(utf8)) |view| {
            //     var it = view.iterator();
            //     while (it.nextCodepoint()) |cp| {
            //         try writer.print("U+{X} ", .{cp});
            //     }
            // } else |_| {
            //     try writer.writeAll("(invalid utf-8)");
            // }
            match core::str::from_utf8(utf8) {
                Ok(view) => {
                    for cp in view.chars() {
                        write!(stream, "U+{:X} ", cp as u32).map_err(|_| Error::NoSpaceLeft)?;
                    }
                }
                Err(_) => {
                    stream
                        .write_all(b"(invalid utf-8)")
                        .map_err(|()| Error::NoSpaceLeft)?;
                }
            }
            // try writer.writeByte(0);
            stream.write_byte(0).map_err(|()| Error::NoSpaceLeft)?;
            written_len = stream.written().len();
        }

        // Render as a textbox
        // _ = cimgui.c.ImGui_InputText(
        //     "##utf8",
        //     &buf,
        //     buf_stream.getWritten().len - 1,
        //     cimgui.c.ImGuiInputTextFlags_ReadOnly,
        // );
        let _ = unsafe {
            dcimgui_sys::ImGui_InputText(
                c"##utf8".as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                written_len - 1,
                dcimgui_sys::ImGuiInputTextFlags__ImGuiInputTextFlags_ReadOnly
                    as dcimgui_sys::ImGuiInputTextFlags,
            )
        };
        Ok(())
    }

    // fn renderPty(self: *const Event) !void {
    #[allow(non_snake_case)] // faithful Zig name (R1)
    fn renderPty(&self) -> Result<(), Error> {
        // Format the codepoint sequence
        // var buf: [1024]u8 = undefined;
        // var buf_stream = std.io.fixedBufferStream(&buf);
        // const writer = buf_stream.writer();
        let mut buf = [0u8; 1024];
        let written_len;
        {
            let mut stream = FixedBuf::new(&mut buf);
            // for (self.pty) |byte| {
            for &byte in &self.pty {
                // Print ESC special because its so common
                // if (byte == 0x1B) {
                //     try writer.writeAll("ESC ");
                //     continue;
                // }
                if byte == 0x1B {
                    stream.write_all(b"ESC ").map_err(|()| Error::NoSpaceLeft)?;
                    continue;
                }

                // Print ASCII as-is
                // if (byte > 0x20 and byte < 0x7F) {
                //     try writer.writeByte(byte);
                //     continue;
                // }
                if byte > 0x20 && byte < 0x7F {
                    stream.write_byte(byte).map_err(|()| Error::NoSpaceLeft)?;
                    continue;
                }

                // Everything else as a hex byte
                // try writer.print("0x{X} ", .{byte});
                write!(stream, "0x{:X} ", byte).map_err(|_| Error::NoSpaceLeft)?;
            }

            // try writer.writeByte(0);
            stream.write_byte(0).map_err(|()| Error::NoSpaceLeft)?;
            written_len = stream.written().len();
        }

        // Render as a textbox
        // _ = cimgui.c.ImGui_InputText(
        //     "##pty",
        //     &buf,
        //     buf_stream.getWritten().len - 1,
        //     cimgui.c.ImGuiInputTextFlags_ReadOnly,
        // );
        let _ = unsafe {
            dcimgui_sys::ImGui_InputText(
                c"##pty".as_ptr(),
                buf.as_mut_ptr() as *mut c_char,
                written_len - 1,
                dcimgui_sys::ImGuiInputTextFlags__ImGuiInputTextFlags_ReadOnly
                    as dcimgui_sys::ImGuiInputTextFlags,
            )
        };
        Ok(())
    }
}

// fn modsTooltip(
//     mods: *const input.Mods,
//     buf: []u8,
// ) ![:0]const u8 {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn modsTooltip<'a>(mods: &Mods, buf: &'a mut [u8]) -> Result<&'a [u8], Error> {
    // var stream = std.io.fixedBufferStream(buf);
    // const writer = stream.writer();
    // var first = true;
    let n;
    {
        let mut stream = FixedBuf::new(&mut *buf);
        let mut first = true;
        // if (mods.shift) {
        //     try writer.writeAll("Shift");
        //     first = false;
        // }
        if mods.shift {
            stream.write_all(b"Shift").map_err(|()| Error::NoSpaceLeft)?;
            first = false;
        }
        // if (mods.ctrl) {
        //     if (!first) try writer.writeAll("+");
        //     try writer.writeAll("Ctrl");
        //     first = false;
        // }
        if mods.ctrl {
            if !first {
                stream.write_all(b"+").map_err(|()| Error::NoSpaceLeft)?;
            }
            stream.write_all(b"Ctrl").map_err(|()| Error::NoSpaceLeft)?;
            first = false;
        }
        // if (mods.alt) {
        //     if (!first) try writer.writeAll("+");
        //     try writer.writeAll("Alt");
        //     first = false;
        // }
        if mods.alt {
            if !first {
                stream.write_all(b"+").map_err(|()| Error::NoSpaceLeft)?;
            }
            stream.write_all(b"Alt").map_err(|()| Error::NoSpaceLeft)?;
            first = false;
        }
        // if (mods.super) {
        //     if (!first) try writer.writeAll("+");
        //     try writer.writeAll("Super");
        // }
        if mods.super_ {
            if !first {
                stream.write_all(b"+").map_err(|()| Error::NoSpaceLeft)?;
            }
            stream.write_all(b"Super").map_err(|()| Error::NoSpaceLeft)?;
        }
        // try writer.writeByte(0);
        stream.write_byte(0).map_err(|()| Error::NoSpaceLeft)?;
        // const written = stream.getWritten();
        n = stream.written().len();
    }
    // return written[0 .. written.len - 1 :0];
    Ok(&buf[0..n - 1])
}

/// Keyboard event stream inspector widget.
// pub const Stream = struct {
pub struct Stream {
    // events: EventRing,
    pub events: EventRing,
}

impl Stream {
    // pub fn init(alloc: Allocator) !Stream {
    pub fn init(_alloc: Allocator) -> Result<Stream, Error> {
        // var events: EventRing = try .init(alloc, 2);
        // errdefer events.deinit(alloc);
        let events = EventRing::init(2, Event::default()).map_err(|_| Error::OutOfMemory)?;
        // return .{ .events = events };
        Ok(Stream { events })
    }

    // pub fn deinit(self: *Stream, alloc: Allocator) void {
    //     var it = self.events.iterator(.forward);
    //     while (it.next()) |v| v.deinit(alloc);
    //     self.events.deinit(alloc);
    // }
    //
    // PORT NOTE: the reference frees each Event (its utf8/binding/pty) then the
    // ring. Under Rust ownership this collapses into Drop: `Stream` owns
    // `events` by value, so dropping `Stream` drops the `CircBuf<Event>`, which
    // frees the storage `Vec`, dropping each `Event` and freeing its owned
    // `Vec`s. The forward-mut per-element free loop is elided (the `CircBuf`
    // iterator yields `&Event`; a mutable iterator is scoped to termio, R4 /
    // Task 16) — Drop is the equivalent, leak-free path. `alloc` retained for
    // call-shape parity.
    pub fn deinit(&mut self, _alloc: Allocator) {}

    // pub fn draw(
    //     self: *Stream,
    //     open: bool,
    //     alloc: Allocator,
    // ) void {
    pub fn draw(&mut self, open: bool, _alloc: Allocator) {
        // if (!open) return;
        if !open {
            return;
        }

        // if (self.events.empty()) {
        //     cimgui.c.ImGui_Text("No recorded key events. Press a key with the " ++
        //         "terminal focused to record it.");
        //     return;
        // }
        if self.events.empty() {
            unsafe {
                dcimgui_sys::ImGui_Text(
                    c"No recorded key events. Press a key with the terminal focused to record it."
                        .as_ptr(),
                )
            };
            return;
        }

        // if (cimgui.c.ImGui_Button("Clear")) {
        if unsafe { dcimgui_sys::ImGui_Button(c"Clear".as_ptr()) } {
            // var it = self.events.iterator(.forward);
            // while (it.next()) |v| v.deinit(alloc);
            // self.events.clear();
            //
            // PORT NOTE: per-element `Event.deinit` (freeing utf8/binding/pty)
            // collapses into Rust ownership. `clear()` resets the ring's
            // head/tail so it reads empty; each stale `Event`'s owned `Vec`s are
            // freed when its storage slot is overwritten by the next append or
            // when the ring drops (bounded to `max_capacity` retained slots). No
            // mutable iterator is required (R4: scoped to termio, Task 16).
            self.events.clear();
        }

        // cimgui.c.ImGui_Separator();
        unsafe { dcimgui_sys::ImGui_Separator() };

        // const table_flags = cimgui.c.ImGuiTableFlags_Borders |
        //     cimgui.c.ImGuiTableFlags_Resizable |
        //     cimgui.c.ImGuiTableFlags_ScrollY |
        //     cimgui.c.ImGuiTableFlags_SizingFixedFit;
        let table_flags = (dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_Borders
            | dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_Resizable
            | dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_ScrollY
            | dcimgui_sys::ImGuiTableFlags__ImGuiTableFlags_SizingFixedFit)
            as dcimgui_sys::ImGuiTableFlags;

        // if (!cimgui.c.ImGui_BeginTable("table_key_events", 6, table_flags)) return;
        if !unsafe { dcimgui_sys::ImGui_BeginTable(c"table_key_events".as_ptr(), 6, table_flags) } {
            return;
        }
        // defer cimgui.c.ImGui_EndTable();
        let _end_table = Defer(|| unsafe { dcimgui_sys::ImGui_EndTable() });

        // cimgui.c.ImGui_TableSetupScrollFreeze(0, 1);
        unsafe { dcimgui_sys::ImGui_TableSetupScrollFreeze(0, 1) };
        // cimgui.c.ImGui_TableSetupColumnEx("Action", cimgui.c.ImGuiTableColumnFlags_WidthFixed, 80, 0);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumnEx(
                c"Action".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                    as dcimgui_sys::ImGuiTableColumnFlags,
                80.0,
                0,
            )
        };
        // cimgui.c.ImGui_TableSetupColumnEx("Key", cimgui.c.ImGuiTableColumnFlags_WidthFixed, 160, 0);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumnEx(
                c"Key".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                    as dcimgui_sys::ImGuiTableColumnFlags,
                160.0,
                0,
            )
        };
        // cimgui.c.ImGui_TableSetupColumnEx("Mods", cimgui.c.ImGuiTableColumnFlags_WidthFixed, 150, 0);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumnEx(
                c"Mods".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                    as dcimgui_sys::ImGuiTableColumnFlags,
                150.0,
                0,
            )
        };
        // cimgui.c.ImGui_TableSetupColumnEx("UTF-8", cimgui.c.ImGuiTableColumnFlags_WidthFixed, 80, 0);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumnEx(
                c"UTF-8".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthFixed
                    as dcimgui_sys::ImGuiTableColumnFlags,
                80.0,
                0,
            )
        };
        // cimgui.c.ImGui_TableSetupColumnEx("PTY Encoding", cimgui.c.ImGuiTableColumnFlags_WidthStretch, 0, 0);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumnEx(
                c"PTY Encoding".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthStretch
                    as dcimgui_sys::ImGuiTableColumnFlags,
                0.0,
                0,
            )
        };
        // cimgui.c.ImGui_TableSetupColumnEx("Binding", cimgui.c.ImGuiTableColumnFlags_WidthStretch, 0, 0);
        unsafe {
            dcimgui_sys::ImGui_TableSetupColumnEx(
                c"Binding".as_ptr(),
                dcimgui_sys::ImGuiTableColumnFlags__ImGuiTableColumnFlags_WidthStretch
                    as dcimgui_sys::ImGuiTableColumnFlags,
                0.0,
                0,
            )
        };
        // cimgui.c.ImGui_TableHeadersRow();
        unsafe { dcimgui_sys::ImGui_TableHeadersRow() };

        // var it = self.events.iterator(.reverse);
        let mut it = self.events.iterator(Direction::Reverse);
        // while (it.next()) |ev| {
        while let Some(ev) = it.next() {
            // cimgui.c.ImGui_PushIDPtr(ev);
            unsafe { dcimgui_sys::ImGui_PushIDPtr(ev as *const Event as *const c_void) };
            // defer cimgui.c.ImGui_PopID();
            let _pop_id = Defer(|| unsafe { dcimgui_sys::ImGui_PopID() });

            // cimgui.c.ImGui_TableNextRow();
            unsafe { dcimgui_sys::ImGui_TableNextRow() };
            // const row_min_y = cimgui.c.ImGui_GetCursorScreenPos().y;
            let row_min_y = unsafe { dcimgui_sys::ImGui_GetCursorScreenPos() }.y;

            // Set row background color based on action
            // cimgui.c.ImGui_TableSetBgColor(cimgui.c.ImGuiTableBgTarget_RowBg0, actionColor(ev.event.action), -1);
            unsafe {
                dcimgui_sys::ImGui_TableSetBgColor(
                    dcimgui_sys::ImGuiTableBgTarget__ImGuiTableBgTarget_RowBg0
                        as dcimgui_sys::ImGuiTableBgTarget,
                    actionColor(ev.event.action),
                    -1,
                )
            };

            // Action column with colored text
            // _ = cimgui.c.ImGui_TableSetColumnIndex(0);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(0) };
            // const action_text_color: cimgui.c.ImVec4 = switch (ev.event.action) {
            //     .press => .{ .x = 0.4, .y = 1.0, .z = 0.4, .w = 1.0 }, // Green
            //     .release => .{ .x = 0.6, .y = 0.6, .z = 1.0, .w = 1.0 }, // Blue
            //     .repeat => .{ .x = 1.0, .y = 1.0, .z = 0.4, .w = 1.0 }, // Yellow
            // };
            let action_text_color: dcimgui_sys::ImVec4 = match ev.event.action {
                KeyAction::Press => dcimgui_sys::ImVec4 {
                    x: 0.4,
                    y: 1.0,
                    z: 0.4,
                    w: 1.0,
                }, // Green
                KeyAction::Release => dcimgui_sys::ImVec4 {
                    x: 0.6,
                    y: 0.6,
                    z: 1.0,
                    w: 1.0,
                }, // Blue
                KeyAction::Repeat => dcimgui_sys::ImVec4 {
                    x: 1.0,
                    y: 1.0,
                    z: 0.4,
                    w: 1.0,
                }, // Yellow
            };
            // cimgui.c.ImGui_TextColored(action_text_color, "%s", @tagName(ev.event.action).ptr);
            unsafe {
                dcimgui_sys::ImGui_TextColored(
                    action_text_color,
                    c"%s".as_ptr(),
                    action_tag_name(ev.event.action).as_ptr(),
                )
            };

            // Key column with consistent key coloring
            // _ = cimgui.c.ImGui_TableSetColumnIndex(1);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(1) };
            // const key_name = switch (ev.event.key) {
            //     .unidentified => if (ev.event.utf8.len > 0) ev.event.utf8 else @tagName(ev.event.key),
            //     else => @tagName(ev.event.key),
            // };
            let key_name: &[u8] = match ev.event.key {
                Key::Unidentified => {
                    if !ev.event.utf8.is_empty() {
                        ev.event.utf8.as_slice()
                    } else {
                        ev.event.key.tag_name().as_bytes()
                    }
                }
                _ => ev.event.key.tag_name().as_bytes(),
            };
            // const key_rgba = keyColor(ev.event.key);
            let key_rgba = keyColor(ev.event.key);
            // const key_color: cimgui.c.ImVec4 = .{
            //     .x = @as(f32, @floatFromInt(key_rgba & 0xFF)) / 255.0,
            //     .y = @as(f32, @floatFromInt((key_rgba >> 8) & 0xFF)) / 255.0,
            //     .z = @as(f32, @floatFromInt((key_rgba >> 16) & 0xFF)) / 255.0,
            //     .w = 1.0,
            // };
            let key_color = dcimgui_sys::ImVec4 {
                x: (key_rgba & 0xFF) as f32 / 255.0,
                y: ((key_rgba >> 8) & 0xFF) as f32 / 255.0,
                z: ((key_rgba >> 16) & 0xFF) as f32 / 255.0,
                w: 1.0,
            };
            // cimgui.c.ImGui_TextColored(key_color, "%s", key_name.ptr);
            let mut key_name_buf = [0u8; 256];
            unsafe {
                dcimgui_sys::ImGui_TextColored(
                    key_color,
                    c"%s".as_ptr(),
                    cstr_ptr(&mut key_name_buf, key_name),
                )
            };

            // Composing indicator
            // if (ev.event.composing) {
            if ev.event.composing {
                // cimgui.c.ImGui_SameLine();
                unsafe { dcimgui_sys::ImGui_SameLine() };
                // cimgui.c.ImGui_TextColored(.{ .x = 1.0, .y = 0.6, .z = 0.0, .w = 1.0 }, "*");
                unsafe {
                    dcimgui_sys::ImGui_TextColored(
                        dcimgui_sys::ImVec4 {
                            x: 1.0,
                            y: 0.6,
                            z: 0.0,
                            w: 1.0,
                        },
                        c"*".as_ptr(),
                    )
                };
                // if (cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_None)) {
                //     cimgui.c.ImGui_SetTooltip("Composing (dead key)");
                // }
                if unsafe {
                    dcimgui_sys::ImGui_IsItemHovered(
                        dcimgui_sys::ImGuiHoveredFlags__ImGuiHoveredFlags_None
                            as dcimgui_sys::ImGuiHoveredFlags,
                    )
                } {
                    unsafe { dcimgui_sys::ImGui_SetTooltip(c"Composing (dead key)".as_ptr()) };
                }
            }

            // Mods
            // _ = cimgui.c.ImGui_TableSetColumnIndex(2);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(2) };
            // mods: {
            'mods: {
                // if (ev.event.mods.empty()) {
                //     cimgui.c.ImGui_TextDisabled("-");
                //     break :mods;
                // }
                if ev.event.mods.empty() {
                    unsafe { dcimgui_sys::ImGui_TextDisabled(c"-".as_ptr()) };
                    break 'mods;
                }

                // var any_hovered = false;
                let mut any_hovered = false;
                // if (ev.event.mods.shift) {
                //     _ = cimgui.c.ImGui_SmallButton("S");
                //     any_hovered = any_hovered or cimgui.c.ImGui_IsItemHovered(cimgui.c.ImGuiHoveredFlags_None);
                //     cimgui.c.ImGui_SameLine();
                // }
                if ev.event.mods.shift {
                    let _ = unsafe { dcimgui_sys::ImGui_SmallButton(c"S".as_ptr()) };
                    any_hovered = any_hovered
                        || unsafe {
                            dcimgui_sys::ImGui_IsItemHovered(
                                dcimgui_sys::ImGuiHoveredFlags__ImGuiHoveredFlags_None
                                    as dcimgui_sys::ImGuiHoveredFlags,
                            )
                        };
                    unsafe { dcimgui_sys::ImGui_SameLine() };
                }
                // if (ev.event.mods.ctrl) { ... "C" ... }
                if ev.event.mods.ctrl {
                    let _ = unsafe { dcimgui_sys::ImGui_SmallButton(c"C".as_ptr()) };
                    any_hovered = any_hovered
                        || unsafe {
                            dcimgui_sys::ImGui_IsItemHovered(
                                dcimgui_sys::ImGuiHoveredFlags__ImGuiHoveredFlags_None
                                    as dcimgui_sys::ImGuiHoveredFlags,
                            )
                        };
                    unsafe { dcimgui_sys::ImGui_SameLine() };
                }
                // if (ev.event.mods.alt) { ... "A" ... }
                if ev.event.mods.alt {
                    let _ = unsafe { dcimgui_sys::ImGui_SmallButton(c"A".as_ptr()) };
                    any_hovered = any_hovered
                        || unsafe {
                            dcimgui_sys::ImGui_IsItemHovered(
                                dcimgui_sys::ImGuiHoveredFlags__ImGuiHoveredFlags_None
                                    as dcimgui_sys::ImGuiHoveredFlags,
                            )
                        };
                    unsafe { dcimgui_sys::ImGui_SameLine() };
                }
                // if (ev.event.mods.super) { ... "M" ... }
                if ev.event.mods.super_ {
                    let _ = unsafe { dcimgui_sys::ImGui_SmallButton(c"M".as_ptr()) };
                    any_hovered = any_hovered
                        || unsafe {
                            dcimgui_sys::ImGui_IsItemHovered(
                                dcimgui_sys::ImGuiHoveredFlags__ImGuiHoveredFlags_None
                                    as dcimgui_sys::ImGuiHoveredFlags,
                            )
                        };
                    unsafe { dcimgui_sys::ImGui_SameLine() };
                }
                // cimgui.c.ImGui_NewLine();
                unsafe { dcimgui_sys::ImGui_NewLine() };

                // if (any_hovered) tooltip: {
                if any_hovered {
                    'tooltip: {
                        // var tooltip_buf: [64]u8 = undefined;
                        let mut tooltip_buf = [0u8; 64];
                        // const tooltip = modsTooltip(
                        //     &ev.event.mods,
                        //     &tooltip_buf,
                        // ) catch break :tooltip;
                        let tooltip = match modsTooltip(&ev.event.mods, &mut tooltip_buf) {
                            Ok(t) => t,
                            Err(_) => break 'tooltip,
                        };
                        // cimgui.c.ImGui_SetTooltip("%s", tooltip.ptr);
                        //
                        // PORT NOTE: `tooltip` excludes the trailing NUL (Zig
                        // `[:0]`), but the NUL sits in `tooltip_buf` immediately
                        // after it, so `tooltip.as_ptr()` is a valid C string.
                        unsafe {
                            dcimgui_sys::ImGui_SetTooltip(
                                c"%s".as_ptr(),
                                tooltip.as_ptr() as *const c_char,
                            )
                        };
                    }
                }
            }

            // UTF-8
            // _ = cimgui.c.ImGui_TableSetColumnIndex(3);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(3) };
            // if (ev.event.utf8.len == 0) {
            //     cimgui.c.ImGui_TextDisabled("-");
            // } else {
            if ev.event.utf8.is_empty() {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"-".as_ptr()) };
            } else {
                // var utf8_buf: [128]u8 = undefined;
                // var utf8_stream = std.io.fixedBufferStream(&utf8_buf);
                // const utf8_writer = utf8_stream.writer();
                let mut utf8_buf = [0u8; 128];
                {
                    let mut utf8_stream = FixedBuf::new(&mut utf8_buf);
                    // if (std.unicode.Utf8View.init(ev.event.utf8)) |view| {
                    //     var utf8_it = view.iterator();
                    //     while (utf8_it.nextCodepoint()) |cp| {
                    //         utf8_writer.print("U+{X} ", .{cp}) catch break;
                    //     }
                    // } else |_| {
                    //     utf8_writer.writeAll("?") catch {};
                    // }
                    match core::str::from_utf8(&ev.event.utf8) {
                        Ok(view) => {
                            for cp in view.chars() {
                                if write!(utf8_stream, "U+{:X} ", cp as u32).is_err() {
                                    break;
                                }
                            }
                        }
                        Err(_) => {
                            let _ = utf8_stream.write_all(b"?");
                        }
                    }
                    // utf8_writer.writeByte(0) catch {};
                    let _ = utf8_stream.write_byte(0);
                }
                // cimgui.c.ImGui_Text("%s", &utf8_buf);
                unsafe {
                    dcimgui_sys::ImGui_Text(c"%s".as_ptr(), utf8_buf.as_ptr() as *const c_char)
                };
            }

            // PTY
            // _ = cimgui.c.ImGui_TableSetColumnIndex(4);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(4) };
            // if (ev.pty.len == 0) {
            //     cimgui.c.ImGui_TextDisabled("-");
            // } else {
            if ev.pty.is_empty() {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"-".as_ptr()) };
            } else {
                // var pty_buf: [256]u8 = undefined;
                // var pty_stream = std.io.fixedBufferStream(&pty_buf);
                // const pty_writer = pty_stream.writer();
                let mut pty_buf = [0u8; 256];
                {
                    let mut pty_stream = FixedBuf::new(&mut pty_buf);
                    // for (ev.pty) |byte| {
                    //     if (byte == 0x1B) { pty_writer.writeAll("ESC ") catch break; }
                    //     else if (byte > 0x20 and byte < 0x7F) { pty_writer.writeByte(byte) catch break; }
                    //     else { pty_writer.print("0x{X} ", .{byte}) catch break; }
                    // }
                    for &byte in &ev.pty {
                        let r = if byte == 0x1B {
                            pty_stream.write_all(b"ESC ")
                        } else if byte > 0x20 && byte < 0x7F {
                            pty_stream.write_byte(byte)
                        } else {
                            write!(pty_stream, "0x{:X} ", byte).map_err(|_| ())
                        };
                        if r.is_err() {
                            break;
                        }
                    }
                    // pty_writer.writeByte(0) catch {};
                    let _ = pty_stream.write_byte(0);
                }
                // cimgui.c.ImGui_Text("%s", &pty_buf);
                unsafe {
                    dcimgui_sys::ImGui_Text(c"%s".as_ptr(), pty_buf.as_ptr() as *const c_char)
                };
            }

            // Binding
            // _ = cimgui.c.ImGui_TableSetColumnIndex(5);
            let _ = unsafe { dcimgui_sys::ImGui_TableSetColumnIndex(5) };
            // if (ev.binding.len == 0) {
            //     cimgui.c.ImGui_TextDisabled("-");
            // } else {
            if ev.binding.is_empty() {
                unsafe { dcimgui_sys::ImGui_TextDisabled(c"-".as_ptr()) };
            } else {
                // var binding_buf: [256]u8 = undefined;
                // var binding_stream = std.io.fixedBufferStream(&binding_buf);
                // const binding_writer = binding_stream.writer();
                let mut binding_buf = [0u8; 256];
                {
                    let mut binding_stream = FixedBuf::new(&mut binding_buf);
                    // for (ev.binding, 0..) |action, i| {
                    //     if (i > 0) binding_writer.writeAll(", ") catch break;
                    //     binding_writer.writeAll(@tagName(action)) catch break;
                    // }
                    for (i, action) in ev.binding.iter().enumerate() {
                        if i > 0 && binding_stream.write_all(b", ").is_err() {
                            break;
                        }
                        if binding_stream
                            .write_all(action.tag_name().as_bytes())
                            .is_err()
                        {
                            break;
                        }
                    }
                    // binding_writer.writeByte(0) catch {};
                    let _ = binding_stream.write_byte(0);
                }
                // cimgui.c.ImGui_Text("%s", &binding_buf);
                unsafe {
                    dcimgui_sys::ImGui_Text(
                        c"%s".as_ptr(),
                        binding_buf.as_ptr() as *const c_char,
                    )
                };
            }

            // Row hover highlight
            // const row_max_y = cimgui.c.ImGui_GetCursorScreenPos().y;
            let row_max_y = unsafe { dcimgui_sys::ImGui_GetCursorScreenPos() }.y;
            // const mouse_pos = cimgui.c.ImGui_GetMousePos();
            let mouse_pos = unsafe { dcimgui_sys::ImGui_GetMousePos() };
            // if (mouse_pos.y >= row_min_y and mouse_pos.y < row_max_y) {
            //     cimgui.c.ImGui_TableSetBgColor(cimgui.c.ImGuiTableBgTarget_RowBg1, 0x1AFFFFFF, -1);
            // }
            if mouse_pos.y >= row_min_y && mouse_pos.y < row_max_y {
                unsafe {
                    dcimgui_sys::ImGui_TableSetBgColor(
                        dcimgui_sys::ImGuiTableBgTarget__ImGuiTableBgTarget_RowBg1
                            as dcimgui_sys::ImGuiTableBgTarget,
                        0x1AFF_FFFF,
                        -1,
                    )
                };
            }
        }
    }
}

/// Returns row background color for an action (ABGR format for ImGui)
// fn actionColor(action: input.Action) u32 {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn actionColor(action: KeyAction) -> dcimgui_sys::ImU32 {
    // return switch (action) {
    match action {
        KeyAction::Press => 0x1A4A_6F4A,   // Muted sage green
        KeyAction::Release => 0x1A6A_5A5A, // Muted slate gray
        KeyAction::Repeat => 0x1A4A_5A6F,  // Muted warm brown
    }
}

/// Generate a consistent color for a key based on its enum value.
/// Uses HSV color space with fixed saturation and value for pleasing colors.
// fn keyColor(key: input.Key) u32 {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn keyColor(key: Key) -> dcimgui_sys::ImU32 {
    // const key_int: u32 = @intCast(@intFromEnum(key));
    //
    // PORT NOTE: `Key` is `#[repr(C)]` with data-less variants, so `key as u32`
    // is `@intFromEnum` (Unidentified = 0, incrementing in declaration order).
    let key_int: u32 = key as u32;
    // const hue: f32 = @as(f32, @floatFromInt(key_int *% 47)) / 256.0;
    let hue: f32 = key_int.wrapping_mul(47) as f32 / 256.0;
    // return hsvToRgba(hue, 0.5, 0.9, 1.0);
    hsvToRgba(hue, 0.5, 0.9, 1.0)
}

/// Convert HSV (hue 0-1, saturation 0-1, value 0-1) to RGBA u32.
// fn hsvToRgba(h: f32, s: f32, v: f32, a: f32) u32 {
#[allow(non_snake_case)] // faithful Zig name (R1)
fn hsvToRgba(h: f32, s: f32, v: f32, a: f32) -> dcimgui_sys::ImU32 {
    // var r: f32 = undefined;
    // var g: f32 = undefined;
    // var b: f32 = undefined;
    let r: f32;
    let g: f32;
    let b: f32;

    // const i: u32 = @intFromFloat(h * 6.0);
    let i: u32 = (h * 6.0) as u32;
    // const f = h * 6.0 - @as(f32, @floatFromInt(i));
    let f = h * 6.0 - i as f32;
    // const p = v * (1.0 - s);
    let p = v * (1.0 - s);
    // const q = v * (1.0 - f * s);
    let q = v * (1.0 - f * s);
    // const t = v * (1.0 - (1.0 - f) * s);
    let t = v * (1.0 - (1.0 - f) * s);

    // switch (i % 6) {
    match i % 6 {
        0 => {
            r = v;
            g = t;
            b = p;
        }
        1 => {
            r = q;
            g = v;
            b = p;
        }
        2 => {
            r = p;
            g = v;
            b = t;
        }
        3 => {
            r = p;
            g = q;
            b = v;
        }
        4 => {
            r = t;
            g = p;
            b = v;
        }
        _ => {
            r = v;
            g = p;
            b = q;
        }
    }

    // const ri: u32 = @intFromFloat(r * 255.0);
    // const gi: u32 = @intFromFloat(g * 255.0);
    // const bi: u32 = @intFromFloat(b * 255.0);
    // const ai: u32 = @intFromFloat(a * 255.0);
    let ri: u32 = (r * 255.0) as u32;
    let gi: u32 = (g * 255.0) as u32;
    let bi: u32 = (b * 255.0) as u32;
    let ai: u32 = (a * 255.0) as u32;

    // return (ai << 24) | (bi << 16) | (gi << 8) | ri;
    (ai << 24) | (bi << 16) | (gi << 8) | ri
}
