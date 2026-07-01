//! PORTS: ghostty `src/apprt/structs.zig` — the plain apprt value types shared
//! across runtimes (content scale, sizes, cursor/IME position, clipboard, color
//! scheme, selection).
//!
//! PORT NOTE (acronyms): `IMEPos` keeps the reference name (identity/parity, R1);
//! the module-level allow silences clippy's `upper_case_acronyms` for it, exactly
//! as `rustty-vt`'s `color.rs` does for `RGB`.
#![allow(clippy::upper_case_acronyms)]

/// ContentScale is the ratio between the current DPI and the platform's
/// default DPI. This is used to determine how much certain rendered elements
/// need to be scaled up or down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContentScale {
    pub x: f32,
    pub y: f32,
}

/// The size of the surface in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

impl SurfaceSize {
    pub fn eql(&self, other: &SurfaceSize) -> bool {
        self.width == other.width && self.height == other.height
    }
}

/// The position of the cursor in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CursorPos {
    pub x: f32,
    pub y: f32,
}

/// Input Method Editor (IME) position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IMEPos {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The clipboard type.
///
/// If this is changed, you must also update rustty.h
//
// PORT NOTE: the reference is `enum(Backing)` where
// `Backing = switch (build_config.app_runtime) { .gtk => c_int, else => u2 }`.
// Rust has no `u2`; `#[repr(u8)]` is the smallest faithful backing (values
// 0/1/2). The GTK-only widening to `c_int` and the `getGObjectType` GObject
// registration are deferred to rustty-apprt's gtk backend (this crate defines
// the plain enum only; byte-exact C-ABI is part of the deferred embedded C-API).
// Reference `Backing`/`getGObjectType` comments preserved verbatim below.
//
//   Our backing isn't is as small as we can in Zig, but a full
//   C int if we're binding to C APIs.
//     const Backing = switch (build_config.app_runtime) { .gtk => c_int, else => u2 };
//
//   Make this a valid gobject if we're in a GTK environment.
//     pub const getGObjectType = switch (build_config.app_runtime) {
//         .gtk => @import("gobject").ext.defineEnum(Clipboard, .{ .name = "GhosttyApprtClipboard" }),
//         .none => void,
//     };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Clipboard {
    Standard = 0, // ctrl+c/v
    Selection = 1,
    Primary = 2,
}

/// PORT NOTE: `mime`/`data` are `[:0]const u8` (raw bytes, not validated UTF-8) —
/// modeled as borrowed `&[u8]` per the byte-slice convention (never a lossy
/// String), so `ClipboardContent` carries a lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipboardContent<'a> {
    pub mime: &'a [u8],
    pub data: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ClipboardRequestType {
    Paste,
    Osc52Read,
    Osc52Write,
}

/// Clipboard request. This is used to request clipboard contents and must
/// be sent as a response to a ClipboardRequest event.
//
// PORT NOTE: the reference `getGObjectType` (`gobject.ext.defineBoxed(...,
// "GhosttyClipboardRequest")`, gated on `app_runtime == .gtk`) is deferred to
// rustty-apprt's gtk backend; this crate defines the plain union only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardRequest {
    /// A direct paste of clipboard contents.
    Paste,

    /// A request to read clipboard contents via OSC 52.
    Osc52Read(Clipboard),

    /// A request to write clipboard contents via OSC 52.
    Osc52Write(Clipboard),
}

/// The color scheme in use (light vs dark).
//
// PORT NOTE: reference `enum(u2)`; Rust has no `u2`, `#[repr(u8)]` is the
// smallest faithful backing (values 0/1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ColorScheme {
    Light = 0,
    Dark = 1,
}

/// Selection information
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
    /// Top-left point of the selection in the viewport in scaled
    /// window pixels. (0,0) is the top-left of the window.
    pub tl_x_px: f64,
    pub tl_y_px: f64,

    /// The offset of the selection start in cells from the top-left
    /// of the viewport.
    ///
    /// This is a strange metric but its used by macOS.
    pub offset_start: u32,
    pub offset_len: u32,
}
