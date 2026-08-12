//! rustty-config::theme
//!
//! PORTS: ghostty src/config/theme.zig
//!
//! PORT STATUS: ported. Every line/comment/test{} (R1) is ported here. The only
//! retarget is product identity (R2): the "Ghostty resources dir" comments keep
//! their meaning but name the rustty resources dir.

// const std = @import("std");
// const builtin = @import("builtin");
// const Allocator = std.mem.Allocator;
// const global_state = &@import("../global.zig").state;
// const internal_os = @import("../os/main.zig");
// const cli = @import("../cli.zig");
//
// PORT NOTE (R4 cycle-break, BREAK #1): the reference reaches the process-global
// resources dir via `global_state.resources_dir`. `rustty-global` is a leaf and
// the concrete `ResourcesDir` type lives in `rustty-os` (above `rustty-global` in
// the port order), so the global `state` is generic over it. `rustty-config`
// cannot depend on `rustty-global` without re-introducing the cycle, so the
// `.resources` arm reads the resources dir through a leaf-safe injection seam
// (`set_resources_dir`/`resources_dir`) that `rustty-core` populates from the
// process-global `GlobalState.resources_dir`. The seam carries the REAL
// `rustty_os::ResourcesDir` value, so the `.resources` arm is faithful (not a
// stub): when wired, `resources_dir().app()` is exactly the reference's
// `global_state.resources_dir.app()`.

use crate::path::{basename, is_absolute};
use rustty_args::diagnostics::{Diagnostic, DiagnosticList};
#[cfg_attr(target_family = "wasm", allow(unused_imports))]
use rustty_os as internal_os;
// WS14 T6 (wasm32 arm): a browser has no filesystem, no XDG base dirs, no user
// database and no home directory, and the reference's own wasm build never
// reaches this path (Zig analyses `config.zig.Wasm` LAZILY, so it never forces
// these decls; Rust compiles whole crates, so an arm must exist). The wasm arm
// takes the SAME branch this code already takes when the lookup fails.
#[cfg(not(target_family = "wasm"))]
use rustty_os::ResourcesDir;
#[cfg_attr(target_family = "wasm", allow(unused_imports))]
use std::sync::OnceLock;

/// PORT NOTE (BREAK #1 seam): the process-global resources directory the
/// reference reads via `global_state.resources_dir`. `rustty-core` sets this once
/// from the real `GlobalState.resources_dir` (built by `rustty_os::resourcesDir`).
/// Until set, the `.resources` location reports no directory — identical to the
/// reference behavior when `resources_dir.app()` is null (a perfectly valid state
/// per `Location.dir`'s own docs).
#[cfg(not(target_family = "wasm"))]
static RESOURCES_DIR: OnceLock<ResourcesDir> = OnceLock::new();

/// Inject the process-global resources directory (BREAK #1 seam). Called once by
/// `rustty-core` during global-state init. Mirrors the reference's
/// `global_state.resources_dir` being populated in `GlobalState.init`.
#[cfg(not(target_family = "wasm"))]
pub fn set_resources_dir(dir: ResourcesDir) -> Result<(), ResourcesDir> {
    RESOURCES_DIR.set(dir)
}

/// Read the process-global resources directory's `app()` path, or `None` if the
/// seam is unset or the dir has no app path. Equivalent to the reference's
/// `global_state.resources_dir.app()`.
fn resources_dir_app() -> Option<String> {
    // WS14 T6 (wasm32 arm): no resources directory exists in a browser — the same
    // "seam unset" state the doc comment above already describes as valid.
    #[cfg(target_family = "wasm")]
    {
        None
    }
    #[cfg(not(target_family = "wasm"))]
    {
        RESOURCES_DIR.get().and_then(|d| d.app().map(String::from))
    }
}

/// Location of possible themes. The order of this enum matters because it
/// defines the priority of theme search (from top to bottom).
// pub const Location = enum {
//     user, // XDG config dir
//     resources, // Ghostty resources dir
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// XDG config dir
    User,
    /// rustty resources dir
    Resources,
}

impl Location {
    /// PORT NOTE: the reference iterates `Location` via `@enumFromInt(self.i)`.
    /// This is the declaration-order list of variants used by `LocationIterator`.
    const ALL: [Location; 2] = [Location::User, Location::Resources];

    /// Returns the directory for the given theme based on this location type.
    ///
    /// This will return null with no error if the directory type doesn't exist
    /// or is invalid for any reason. For example, it is perfectly valid to
    /// install and run Ghostty without the resources directory.
    ///
    /// Due to the way allocations are handled, an Arena allocator (or another
    /// similar allocator implementation) should be used. It may not be safe to
    /// free the returned allocations.
    // pub fn dir(
    //     self: Location,
    //     arena_alloc: Allocator,
    // ) error{OutOfMemory}!?[]const u8 {
    //
    // PORT NOTE: allocation in Rust is infallible for the owned `String`/`Vec`
    // collections used here, so the reference's `error{OutOfMemory}` set collapses
    // away — `dir` returns `Option<Vec<u8>>` directly. The `BufferTooSmall`/iOS
    // comptime error juggling in the `.user` arm exists only to remap the
    // platform-specific `xdg.config` error set into `OutOfMemory`; in Rust
    // `xdg::config` returns the single `DirError::NoHomeDir`, which we treat as
    // "the XDG directory doesn't exist" (the reference's `else => return null`).
    pub fn dir(self) -> Option<Vec<u8>> {
        // return switch (self) {
        match self {
            // .user => user: {
            Location::User => {
                // const subdir = std.fs.path.join(arena_alloc, &.{
                //     "ghostty", "themes",
                // }) catch return error.OutOfMemory;
                //
                // R2/Task 18: the subdir name carries the product identity
                // (was hardcoded to the literal `"ghostty"`, an obsolete leak
                // from the "reused macOS app hardcodes ghostty" rationale) —
                // sourced from `rustty-identity::CONFIG_DIR` so the user
                // themes dir is `~/.config/rustty/themes` by default (matching
                // where `rustty-config` looks for the main config file), and
                // `~/.config/ghostty/themes` under the `identity-oracle`
                // parity profile.
                let subdir = join(&[rustty_identity::CONFIG_DIR.as_bytes(), b"themes"]);

                // break :user internal_os.xdg.config(
                //     arena_alloc,
                //     .{ .subdir = subdir },
                // ) catch |err| {
                //     // We need to do some comptime tricks to get the right
                //     // error set since some platforms don't support some
                //     // error types.
                //     const Error = @TypeOf(err) || switch (builtin.os.tag) {
                //         .ios => error{BufferTooSmall},
                //         else => error{},
                //     };
                //
                //     switch (@as(Error, err)) {
                //         error.OutOfMemory => return error.OutOfMemory,
                //         error.BufferTooSmall => return error.OutOfMemory,
                //
                //         // Any other error we treat as the XDG directory not
                //         // existing. Windows in particularly can return a LOT
                //         // of errors here.
                //         else => return null,
                //     }
                // };
                // (W94-B02-01: `xdg::config` is byte-typed — the joined subdir
                // bytes feed it directly; the former `from_utf8_lossy` shim is
                // gone.)
                // WS14 T6 (wasm32 arm): no XDG config directory in a browser — the
                // same `None` the error arm below already yields.
                #[cfg(target_family = "wasm")]
                {
                    let _ = subdir;
                    None
                }
                // Any other error we treat as the XDG directory not
                // existing. Windows in particularly can return a LOT
                // of errors here — hence the blanket `.ok()`.
                #[cfg(not(target_family = "wasm"))]
                internal_os::xdg::config(
                    &rustty_global::environ_map(),
                    internal_os::xdg::Options {
                        subdir: Some(&subdir),
                        ..Default::default()
                    },
                )
                .ok()
            }

            // .resources => try std.fs.path.join(arena_alloc, &.{
            //     global_state.resources_dir.app() orelse return null,
            //     "themes",
            // }),
            Location::Resources => {
                let app = resources_dir_app()?;
                Some(join(&[app.as_bytes(), b"themes"]))
            }
        }
    }
}

/// PORT NOTE: `std.fs.path.join` for the in-tree uses here joins the components
/// with the platform path separator. On the POSIX targets the parity harness
/// runs, that is '/'. Empty components are skipped (matching `std.fs.path.join`),
/// and no trailing separator is added.
fn join(parts: &[&[u8]]) -> Vec<u8> {
    let sep = std::path::MAIN_SEPARATOR as u8;
    let mut out: Vec<u8> = Vec::new();
    for part in parts {
        if part.is_empty() {
            continue;
        }
        if !out.is_empty() && *out.last().unwrap() != sep {
            out.push(sep);
        }
        out.extend_from_slice(part);
    }
    out
}

/// An iterator that returns all possible directories for finding themes in
/// order of priority.
// pub const LocationIterator = struct {
pub struct LocationIterator {
    /// Due to the way allocations are handled, an Arena allocator (or another
    /// similar allocator implementation) should be used. It may not be safe to
    /// free the returned allocations.
    // arena_alloc: Allocator,
    //
    // PORT NOTE: the reference threads `arena_alloc` through `next`; the Rust
    // `dir`/`join` own their results, so there is no allocator field to carry.
    // i: usize = 0,
    pub i: usize,
}

/// The item yielded by `LocationIterator.next` — the matched location and its
/// resolved directory.
// pub fn next(self: *LocationIterator) !?struct {
//     location: Location,
//     dir: []const u8,
// }
pub struct LocationItem {
    pub location: Location,
    pub dir: Vec<u8>,
}

impl LocationIterator {
    pub fn new() -> LocationIterator {
        LocationIterator { i: 0 }
    }

    // pub fn next(self: *LocationIterator) !?struct {
    //     location: Location,
    //     dir: []const u8,
    // } {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<LocationItem> {
        // const max = @typeInfo(Location).@"enum".fields.len;
        let max = Location::ALL.len();
        // while (self.i < max) {
        while self.i < max {
            // const location: Location = @enumFromInt(self.i);
            let location: Location = Location::ALL[self.i];
            // self.i += 1;
            self.i += 1;
            // if (try location.dir(self.arena_alloc)) |dir|
            //     return .{
            //         .location = location,
            //         .dir = dir,
            //     };
            if let Some(dir) = location.dir() {
                return Some(LocationItem { location, dir });
            }
        }
        // return null;
        None
    }

    // pub fn reset(self: *LocationIterator) void {
    pub fn reset(&mut self) {
        // self.i = 0;
        self.i = 0;
    }
}

impl Default for LocationIterator {
    fn default() -> LocationIterator {
        LocationIterator::new()
    }
}

/// The resolved theme: its path and the open file handle. Returned by `open`.
// returns: ?struct { path: []const u8, file: std.fs.File }
pub struct ThemeFile {
    pub path: Vec<u8>,
    pub file: std::fs::File,
}

/// Open the given named theme. If there are any errors then messages
/// will be appended to the given error list and null is returned. If
/// a non-null return value is returned, there are never any errors added.
///
/// One error that is not recoverable and may be returned is OOM. This is
/// always a critical error for configuration loading so it is returned.
///
/// Due to the way allocations are handled, an Arena allocator (or another
/// similar allocator implementation) should be used. It may not be safe to
/// free the returned allocations.
///
/// This will never return anything other than a handle to a regular file. If
/// the theme resolves to something other than a regular file a diagnostic entry
/// will be added to the list and null will be returned.
// pub fn open(
//     arena_alloc: Allocator,
//     theme: []const u8,
//     diags: *cli.DiagnosticList,
// ) error{OutOfMemory}!?struct {
//     path: []const u8,
//     file: std.fs.File,
// } {
pub fn open(theme: &[u8], diags: &mut DiagnosticList) -> Option<ThemeFile> {
    // Absolute themes are loaded a different path.
    // if (std.fs.path.isAbsolute(theme)) {
    if is_absolute(theme) {
        // const file: std.fs.File = try openAbsolute(
        //     arena_alloc,
        //     theme,
        //     diags,
        // ) orelse return null;
        let file: std::fs::File = open_absolute(theme, diags)?;
        // const stat = file.stat() catch |err| {
        //     try diags.append(arena_alloc, .{
        //         .message = try std.fmt.allocPrintSentinel(
        //             arena_alloc,
        //             "not reading theme from \"{s}\": {}",
        //             .{ theme, err },
        //             0,
        //         ),
        //     });
        //     return null;
        // };
        let stat = match file.metadata() {
            Ok(m) => m,
            Err(err) => {
                let err_str = err.to_string();
                diags.append(Diagnostic {
                    message: alloc_print(&[
                        b"not reading theme from \"",
                        theme,
                        b"\": ",
                        err_str.as_bytes(),
                    ]),
                    ..Default::default()
                });
                return None;
            }
        };
        // switch (stat.kind) {
        //     .file => {},
        //     else => {
        //         try diags.append(arena_alloc, .{
        //             .message = try std.fmt.allocPrintSentinel(
        //                 arena_alloc,
        //                 "not reading theme from \"{s}\": it is a {s}",
        //                 .{ theme, @tagName(stat.kind) },
        //                 0,
        //             ),
        //         });
        //         return null;
        //     },
        // }
        if !stat.is_file() {
            diags.append(Diagnostic {
                message: alloc_print(&[
                    b"not reading theme from \"",
                    theme,
                    b"\": it is a ",
                    kind_name(&stat).as_bytes(),
                ]),
                ..Default::default()
            });
            return None;
        }
        // return .{ .path = theme, .file = file };
        return Some(ThemeFile {
            path: theme.to_vec(),
            file,
        });
    }

    // const basename = std.fs.path.basename(theme);
    let basename = basename(theme);
    // if (!std.mem.eql(u8, theme, basename)) {
    if theme != basename {
        // try diags.append(arena_alloc, .{
        //     .message = try std.fmt.allocPrintSentinel(
        //         arena_alloc,
        //         "theme \"{s}\" cannot include path separators unless it is an absolute path",
        //         .{theme},
        //         0,
        //     ),
        // });
        // return null;
        diags.append(Diagnostic {
            message: alloc_print(&[
                b"theme \"",
                theme,
                b"\" cannot include path separators unless it is an absolute path",
            ]),
            ..Default::default()
        });
        return None;
    }

    // Iterate over the possible locations to try to find the
    // one that exists.
    // var it: LocationIterator = .{ .arena_alloc = arena_alloc };
    let mut it = LocationIterator::new();
    // const cwd = std.fs.cwd();
    // while (try it.next()) |loc| {
    while let Some(loc) = it.next() {
        // const path = try std.fs.path.join(arena_alloc, &.{ loc.dir, theme });
        let path = join(&[&loc.dir, theme]);
        // if (cwd.openFile(path, .{})) |file| {
        match std::fs::File::open(os_str_from_bytes(&path)) {
            Ok(file) => {
                // const stat = file.stat() catch |err| {
                //     try diags.append(arena_alloc, .{
                //         .message = try std.fmt.allocPrintSentinel(
                //             arena_alloc,
                //             "not reading theme from \"{s}\": {}",
                //             .{ theme, err },
                //             0,
                //         ),
                //     });
                //     return null;
                // };
                let stat = match file.metadata() {
                    Ok(m) => m,
                    Err(err) => {
                        let err_str = err.to_string();
                        diags.append(Diagnostic {
                            message: alloc_print(&[
                                b"not reading theme from \"",
                                theme,
                                b"\": ",
                                err_str.as_bytes(),
                            ]),
                            ..Default::default()
                        });
                        return None;
                    }
                };
                // switch (stat.kind) {
                //     .file => {},
                //     else => {
                //         try diags.append(arena_alloc, .{
                //             .message = ... "not reading theme from \"{s}\": it is a {s}" ...
                //         });
                //         return null;
                //     },
                // }
                if !stat.is_file() {
                    diags.append(Diagnostic {
                        message: alloc_print(&[
                            b"not reading theme from \"",
                            theme,
                            b"\": it is a ",
                            kind_name(&stat).as_bytes(),
                        ]),
                        ..Default::default()
                    });
                    return None;
                }
                // return .{
                //     .path = path,
                //     .file = file,
                // };
                return Some(ThemeFile { path, file });
            }
            // } else |err| switch (err) {
            Err(err) => match err.kind() {
                // Not an error, just continue to the next location.
                // error.FileNotFound => {},
                std::io::ErrorKind::NotFound => {}
                // Anything else is an error we log and give up on.
                // else => {
                //     try diags.append(arena_alloc, .{
                //         .message = try std.fmt.allocPrintSentinel(
                //             arena_alloc,
                //             "failed to load theme \"{s}\" from the file \"{s}\": {}",
                //             .{ theme, path, err },
                //             0,
                //         ),
                //     });
                //
                //     return null;
                // },
                _ => {
                    let err_str = err.to_string();
                    diags.append(Diagnostic {
                        message: alloc_print(&[
                            b"failed to load theme \"",
                            theme,
                            b"\" from the file \"",
                            &path,
                            b"\": ",
                            err_str.as_bytes(),
                        ]),
                        ..Default::default()
                    });
                    return None;
                }
            },
        }
    }

    // Unlikely scenario: the theme doesn't exist. In this case, we reset
    // our iterator, reiterate over in order to build a better error message.
    // This does double allocate some memory but for errors I think that's
    // fine.
    // it.reset();
    it.reset();
    // while (try it.next()) |loc| {
    while let Some(loc) = it.next() {
        // const path = try std.fs.path.join(arena_alloc, &.{ loc.dir, theme });
        let path = join(&[&loc.dir, theme]);
        // try diags.append(arena_alloc, .{
        //     .message = try std.fmt.allocPrintSentinel(
        //         arena_alloc,
        //         "theme \"{s}\" not found, tried path \"{s}\"",
        //         .{ theme, path },
        //         0,
        //     ),
        // });
        diags.append(Diagnostic {
            message: alloc_print(&[
                b"theme \"",
                theme,
                b"\" not found, tried path \"",
                &path,
                b"\"",
            ]),
            ..Default::default()
        });
    }

    // return null;
    None
}

/// Open the given theme from an absolute path. If there are any errors
/// then messages will be appended to the given error list and null is
/// returned. If a non-null return value is returned, there are never any
/// errors added.
///
/// Due to the way allocations are handled, an Arena allocator (or another
/// similar allocator implementation) should be used. It may not be safe to
/// free the returned allocations.
// pub fn openAbsolute(
//     arena_alloc: Allocator,
//     theme: []const u8,
//     diags: *cli.DiagnosticList,
// ) error{OutOfMemory}!?std.fs.File {
pub fn open_absolute(theme: &[u8], diags: &mut DiagnosticList) -> Option<std::fs::File> {
    // return std.fs.openFileAbsolute(theme, .{}) catch |err| {
    match std::fs::File::open(os_str_from_bytes(theme)) {
        Ok(file) => Some(file),
        Err(err) => {
            // switch (err) {
            match err.kind() {
                // error.FileNotFound => try diags.append(arena_alloc, .{
                //     .message = try std.fmt.allocPrintSentinel(
                //         arena_alloc,
                //         "failed to load theme from the path \"{s}\"",
                //         .{theme},
                //         0,
                //     ),
                // }),
                std::io::ErrorKind::NotFound => diags.append(Diagnostic {
                    message: alloc_print(&[b"failed to load theme from the path \"", theme, b"\""]),
                    ..Default::default()
                }),
                // else => try diags.append(arena_alloc, .{
                //     .message = try std.fmt.allocPrintSentinel(
                //         arena_alloc,
                //         "failed to load theme from the path \"{s}\": {}",
                //         .{ theme, err },
                //         0,
                //     ),
                // }),
                _ => diags.append(Diagnostic {
                    message: alloc_print(&[
                        b"failed to load theme from the path \"",
                        theme,
                        b"\": ",
                        err.to_string().as_bytes(),
                    ]),
                    ..Default::default()
                }),
            }

            // return null;
            None
        }
    }
}

// ===========================================================================
// PORT NOTE: helpers — std.fs / std.fmt analogues used above. These add nothing the
// reference doesn't already do (R1): they are the minimal Rust glue for the std
// calls the reference makes inline.
// ===========================================================================

// `std.fs.path.isAbsolute` / `std.fs.path.basename` are the target-dispatched
// `std.fs.path` helpers ported once in `crate::path` (imported at the top of this
// module) and shared with `config/path.zig`'s `expand`.
//
// PORT NOTE: they were previously POSIX-shaped `MAIN_SEPARATOR` one-liners here,
// which on a Windows build accepted UNC (`\\server\...`, a leading `\`) while
// REJECTING drive-qualified themes (`C:\...`), and dropped the last component of a
// path with a trailing separator. `crate::path` now ports `std.fs.path`'s real
// `isAbsoluteWindows`/`basenameWindows`/`basenamePosix` bodies.

/// `std.fmt.allocPrintSentinel(.., 0)` — owned message bytes, built by
/// concatenating the literal text segments and the `{s}` byte payloads of the
/// reference's format string. The trailing NUL sentinel the reference appends is
/// implicit (`Vec<u8>` carries no sentinel); `Diagnostic.message` is the byte
/// payload.
///
/// PORT NOTE (fidelity): the `{s}` payloads here (theme name, joined path) are
/// arbitrary OS bytes and Zig's `{s}` copies them verbatim. This helper used to
/// take a `String` built by `format!` over a `String::from_utf8_lossy` view,
/// which replaced every invalid byte with U+FFFD (`EF BF BD`) — so a theme named
/// with e.g. a stray `0xFF` was reported with a different byte payload than the
/// reference reports. Appending the raw slices keeps the diagnostic byte-exact.
/// The `{}`-formatted parts (`std.fs.File.Kind` tag names, error renderings) are
/// ASCII and are passed in as their UTF-8 bytes.
fn alloc_print(parts: &[&[u8]]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for part in parts {
        out.extend_from_slice(part);
    }
    out
}

/// `@tagName(stat.kind)` for the non-file kinds the diagnostics report. Returns
/// the reference's `std.fs.File.Kind` tag name.
fn kind_name(meta: &std::fs::Metadata) -> &'static str {
    let ft = meta.file_type();
    if ft.is_dir() {
        "directory"
    } else if ft.is_symlink() {
        "sym_link"
    } else {
        // Block/char devices, fifos, sockets, etc. We only ever hit this arm for
        // non-regular files; report the generic "unknown" the reference's Kind
        // would render for anything not specifically named.
        "unknown"
    }
}

/// Build an `&OsStr` from raw path bytes without a lossy round-trip (R3/fidelity:
/// byte-slices stay bytes). POSIX-only, matching the parity harness targets.
#[cfg(unix)]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    // Non-unix fallback (not exercised by the POSIX parity harness).
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

// PORT NOTE: the reference has no `test {}` blocks in `theme.zig`. These cover
// the port's diagnostic-message seam only: the reference renders the theme name
// and the tried path with Zig's `{s}`, which copies the byte slice VERBATIM, so
// a theme name (or resolved path) that is not valid UTF-8 must be reported with
// its original bytes and not with U+FFFD replacements.
#[cfg(test)]
mod tests {
    use super::*;

    /// The U+FFFD replacement character `String::from_utf8_lossy` substitutes.
    const REPLACEMENT: &[u8] = &[0xEF, 0xBF, 0xBD];

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    // The separator-rejection branch: no filesystem access at all.
    #[test]
    fn diagnostic_preserves_theme_bytes_separator_rejection() {
        let theme = b"bad\xffname/x";
        let mut diags = DiagnosticList::default();
        assert!(open(theme, &mut diags).is_none());

        let items = diags.items();
        assert_eq!(1, items.len());
        let msg = &items[0].message;
        // The reference's `{s}` copies the name byte for byte.
        assert!(contains(msg, theme), "message lost the raw theme bytes");
        assert!(
            !contains(msg, REPLACEMENT),
            "message replaced invalid bytes with U+FFFD"
        );
        assert_eq!(
            b"theme \"bad\xffname/x\" cannot include path separators unless it is an absolute path"
                .as_slice(),
            msg.as_slice()
        );
    }

    // The absolute-path not-found branch of `openAbsolute`.
    #[test]
    fn diagnostic_preserves_theme_bytes_open_absolute_not_found() {
        let theme = b"/nonexistent-\xff-rustty-theme-parity";
        let mut diags = DiagnosticList::default();
        assert!(open_absolute(theme, &mut diags).is_none());

        let items = diags.items();
        assert_eq!(1, items.len());
        let msg = &items[0].message;
        assert!(contains(msg, theme), "message lost the raw theme bytes");
        assert!(
            !contains(msg, REPLACEMENT),
            "message replaced invalid bytes with U+FFFD"
        );
    }
}
