//! rustty-config::theme
//!
//! PORTS: ghostty src/config/theme.zig
//!
//! PORT STATUS: SKELETON (C2). The public surface (`Location`, `LocationIterator`,
//! `open`, `openAbsolute`) is laid down with `todo!()` bodies so the giant
//! `Config` struct (which references `theme.open`/`theme.Location`) compiles. The
//! full faithful port of every line/comment/test{} (R1) lands in a follow-up
//! sub-batch.

#![allow(dead_code)]

// const std = @import("std");
// const builtin = @import("builtin");
// const Allocator = std.mem.Allocator;
// const global_state = &@import("../global.zig").state;
// const internal_os = @import("../os/main.zig");
// const cli = @import("../cli.zig");

/// Location of possible themes. The order of this enum matters because it
/// defines the priority of theme search (from top to bottom).
// pub const Location = enum { user, resources, ... };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// XDG config dir
    User,
    /// Ghostty resources dir
    Resources,
}

impl Location {
    /// Returns the directory for the given theme based on this location type.
    ///
    /// This will return null with no error if the directory type doesn't exist
    /// or is invalid for any reason. For example, it is perfectly valid to
    /// install and run Ghostty without the resources directory.
    ///
    /// Due to the way allocations are handled, an Arena allocator (or another
    /// similar allocator implementation) should be used. It may not be safe to
    /// free the returned allocations.
    // pub fn dir(self: Location, arena_alloc: Allocator) error{OutOfMemory}!?[]const u8
    pub fn dir(self: Location) -> Option<Vec<u8>> {
        let _ = self;
        todo!("port theme.Location.dir")
    }
}

/// PORTS: theme.zig `LocationIterator`.
// pub const LocationIterator = struct { ... };
pub struct LocationIterator;

/// PORTS: theme.zig `Theme` — the resolved theme file (path + open file handle).
/// `open` returns this. Skeleton placeholder carrying the path the caller reads.
pub struct ThemeFile {
    pub path: Vec<u8>,
}

/// Open the given named theme. If there are any errors then messaging is logged
/// and null is returned. (`pub fn open`)
// pub fn open(arena_alloc, themes_dir, theme, errors) !?...
pub fn open(_theme: &[u8]) -> Option<ThemeFile> {
    todo!("port theme.open")
}

/// PORTS: theme.zig `openAbsolute`.
pub fn open_absolute(_path: &[u8]) -> Option<ThemeFile> {
    todo!("port theme.openAbsolute")
}
