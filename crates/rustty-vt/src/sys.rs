//! System interface for the terminal package.
//!
//! This provides runtime-swappable function pointers for operations that
//! depend on external implementations (e.g. image decoding). Each function
//! pointer is initialized with a default implementation if available.
//!
//! This exists so that the terminal package doesn't have hard dependencies
//! on specific libraries and enables embedders of the terminal package to
//! swap out implementations as needed at startup to provide their own
//! implementations.
//!
//! PORTS: ghostty src/terminal/sys.zig
//!
//! PORT STATUS: done
//!
//! PORT NOTE: the reference imports are
//!   const std = @import("std");
//!   const Allocator = std.mem.Allocator;
//!   const build_options = @import("terminal_options");
//! The default `decode_png` impl uses `@import("wuffs")` — the wuffs C library
//! binding, which is NOT part of this leaf sub-batch (it rides on a later FFI
//! crate, R3). We port the function-pointer system and the `Image`/`DecodeError`
//! types verbatim; the default value is `None` here (the reference also defaults
//! to `null` when `build_options.artifact == .lib`), and the `decodePngWuffs`
//! seam is left as a documented deferred stub so no unported file is pulled in.

#![allow(non_upper_case_globals)]

use std::sync::RwLock;

/// The error type returned by a PNG decode.
// PORT NOTE: the reference is `pub const DecodeError = Allocator.Error ||
// error{InvalidData};`. The allocation-failure path is represented by
// `OutOfMemory`; `InvalidData` is the explicit reference error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    OutOfMemory,
    InvalidData,
}

/// The function pointer type for a PNG decoder.
// PORT NOTE: the reference is
// `pub const DecodePngFn = *const fn (Allocator, []const u8) DecodeError!Image;`.
// Allocation is handled by the Rust allocator, so the `Allocator` parameter is
// dropped; the decoder takes the encoded bytes and returns the decoded image or
// an error.
pub type DecodePngFn = fn(&[u8]) -> Result<Image, DecodeError>;

/// Decode PNG data into RGBA pixels. If null, PNG decoding is unsupported
/// and the exact semantics are up to callers. For example, the Kitty Graphics
/// Protocol will work but cannot accept PNG images.
// PORT NOTE: the reference is a `pub var` whose default is either `null` (when
// the build artifact is the C lib) or `&decodePngWuffs`. Since the wuffs binding
// is not in this leaf sub-batch, the default here is `None`; embedders (and the
// later FFI crate) install a decoder at startup via the swappable slot. We use a
// `RwLock` so the global is safely mutable across threads (the reference relies
// on Zig's `var` being mutated only at startup).
pub static decode_png: RwLock<Option<DecodePngFn>> = RwLock::new(None);

/// The result of decoding an image. The caller owns the returned data
/// and must free it with the same allocator that was passed to the
/// decode function.
// PORT NOTE: the reference `data` is a `[]u8` allocated by the decode function;
// in Rust ownership is carried by the `Vec<u8>`, which frees on drop.
#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

// fn decodePngWuffs(
//     alloc: Allocator,
//     data: []const u8,
// ) DecodeError!Image {
//     const wuffs = @import("wuffs");
//     const result = wuffs.png.decode(
//         alloc,
//         data,
//     ) catch |err| switch (err) {
//         error.WuffsError => return error.InvalidData,
//         error.OutOfMemory => return error.OutOfMemory,
//         error.Overflow => return error.InvalidData,
//     };
//
//     return .{
//         .width = result.width,
//         .height = result.height,
//         .data = result.data,
//     };
// }
//
// PORT NOTE (DEFERRED SEAM): `decodePngWuffs` is the default decoder backed by
// the wuffs C library. The wuffs binding is not part of this VT leaf sub-batch
// (R3: no unported sibling pulled in); it is installed by a later FFI crate.
// The function body is reproduced verbatim above as a comment so the reference
// logic is preserved when the seam is filled in.
