//! rustty-config::error_list
//!
//! PORTS: ghostty src/config/ErrorList.zig — the config error accumulator
//! (`ErrorList` = `@This()`). C1 leaf value-type (0 tests).
//!
//! PORT STATUS: ported (every line/comment, R1).

// const ErrorList = @This();
//
// const std = @import("std");
// const Allocator = std.mem.Allocator;

/// PORTS: `pub const Error = struct { message: [:0]const u8 };`
//
// PORT NOTE: `[:0]const u8` (a sentinel-terminated byte slice) -> `Vec<u8>`
// (never a lossy `String`; config values carry raw bytes).
pub struct Error {
    // message: [:0]const u8,
    pub message: Vec<u8>,
}

/// PORTS: `ErrorList` (the `@This()` struct).
///
/// The list of errors. This will use the arena allocator associated
/// with the config structure (or whatever allocated used to call ErrorList
/// functions).
// list: std.ArrayListUnmanaged(Error) = .{},
//
// PORT NOTE: `std.ArrayListUnmanaged(Error)` -> `Vec<Error>`; the reference's
// explicit allocator threading is owned by the `Vec` here.
#[derive(Default)]
pub struct ErrorList {
    pub list: Vec<Error>,
}

impl ErrorList {
    /// True if there are no errors.
    // pub fn empty(self: ErrorList) bool {
    //     return self.list.items.len == 0;
    // }
    pub fn empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Add a new error to the list.
    // pub fn add(self: *ErrorList, alloc: Allocator, err: Error) !void {
    //     try self.list.append(alloc, err);
    // }
    pub fn add(&mut self, err: Error) {
        self.list.push(err);
    }
}
