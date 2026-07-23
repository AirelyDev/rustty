//! rustty-vt clipboard
//!
//! PORTS: ghostty src/terminal/clipboard.zig (BL-5 634ef7198)
//!
//! PORT NOTE: the intermediate `clipboard_set(kind, base64)` form (BL-5
//! 0a410f18e) is subsumed by this protocol-neutral shape — a normalized
//! `Location` plus decoded MIME `Content` representations.

/// The clipboard destination for a write.
// pub const Location = enum(c_int) { standard = 0, selection = 1, primary = 2, _ };
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Standard = 0,
    Selection = 1,
    Primary = 2,
}

/// A single representation of clipboard data.
///
/// The MIME type and data are borrowed and only valid for the duration of a
/// clipboard write callback. Data is binary-safe.
// pub const Content = struct { mime: []const u8, data: []const u8 };
#[derive(Clone, Copy, Debug)]
pub struct Content<'a> {
    pub mime: &'a [u8],
    pub data: &'a [u8],
}

/// One atomic clipboard write.
///
/// Contents are borrowed and only valid for the duration of a clipboard write
/// callback. An empty contents slice clears the destination.
// pub const Write = struct { location: Location, contents: []const Content };
#[derive(Clone, Copy, Debug)]
pub struct Write<'a> {
    pub location: Location,
    pub contents: &'a [Content<'a>],
}

/// The result of a clipboard write.
// pub const WriteResult = enum(c_int) { success..io_error, _ };
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteResult {
    Success = 0,
    Denied = 1,
    Unsupported = 2,
    Busy = 3,
    InvalidData = 4,
    IoError = 5,
}
