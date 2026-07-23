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
//! The default `decode_png` impl uses the `rustty-wuffs` binding under the
//! `wuffs` cargo feature (the reference `@import("wuffs")`); the default is
//! `None` only for the `.lib` artifact / featureless builds, exactly as the
//! reference comptime block reads.

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
// PORT NOTE: the reference is a `pub var` with a comptime default:
//     pub var decode_png: ?DecodePngFn = png: {
//         if (build_options.artifact == .lib) break :png null;
//         break :png &decodePngWuffs;
//     };
// Mapped 1:1: `options::ARTIFACT` carries the artifact (options.rs:32,
// `Ghostty`; the `.lib` arm is kept for the later lib-artifact seam), and the
// wuffs binding rides the `wuffs` cargo feature (rustty-core — the SharedDeps
// `.ghostty` analog — enables it for the application tier; rustty-vt-c — the
// GhosttyZig `.lib` analog — does not, keeping the reference `null` default).
// We use a `RwLock` so the global is safely mutable across threads (the
// reference relies on Zig's `var` being mutated only at startup).
pub static decode_png: RwLock<Option<DecodePngFn>> = RwLock::new(DECODE_PNG_DEFAULT);

#[cfg(feature = "wuffs")]
const DECODE_PNG_DEFAULT: Option<DecodePngFn> =
    if matches!(crate::options::ARTIFACT, crate::build_options::Artifact::Lib) {
        None
    } else {
        Some(decode_png_wuffs as DecodePngFn)
    };
/// Without the wuffs binding no default decoder exists (embedders install one
/// at runtime via the swappable slot — the vt-c `set(.decode_png, ..)` path).
#[cfg(not(feature = "wuffs"))]
const DECODE_PNG_DEFAULT: Option<DecodePngFn> = None;

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
#[cfg(feature = "wuffs")]
fn decode_png_wuffs(data: &[u8]) -> Result<Image, DecodeError> {
    // const wuffs = @import("wuffs");
    use allocator_api2::alloc::Global;
    // const result = wuffs.png.decode(
    //     alloc,
    //     data,
    // ) catch |err| switch (err) {
    //     error.WuffsError => return error.InvalidData,
    //     error.OutOfMemory => return error.OutOfMemory,
    //     error.Overflow => return error.InvalidData,
    // };
    let result = match rustty_wuffs::png::decode(Global, data) {
        Ok(result) => result,
        Err(rustty_wuffs::Error::WuffsError) => return Err(DecodeError::InvalidData),
        Err(rustty_wuffs::Error::OutOfMemory) => return Err(DecodeError::OutOfMemory),
        Err(rustty_wuffs::Error::Overflow) => return Err(DecodeError::InvalidData),
    };

    // return .{
    //     .width = result.width,
    //     .height = result.height,
    //     .data = result.data,
    // };
    //
    // PORT NOTE: the reference moves the alloc-owned `[]u8` into `Image.data`;
    // here ownership transfers via `into_raw` into the `Vec` that carries it
    // (sys.rs `Image.data: Vec<u8>`).
    let (width, height, ptr, len) = result.into_raw();
    // SAFETY: `into_raw` yields sole ownership of a `Global` (std-allocator)
    // align-1 allocation of exactly `len` bytes, matching `Vec<u8>`'s
    // `Layout::array::<u8>(len)` free layout with capacity == len.
    let data = unsafe { Vec::from_raw_parts(ptr.as_ptr(), len, len) };
    Ok(Image {
        width,
        height,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIFETIMES-AUDIT #2 un-defer: the reference comptime default
    /// (sys.zig:18-24) is `&decodePngWuffs` for every non-`.lib` artifact and
    /// `null` otherwise. This crate's `options::ARTIFACT` is `Ghostty`
    /// (options.rs:32), so with the `wuffs` feature (the application tier) the
    /// default MUST be installed; without it (the vt-c `.lib` analog) it stays
    /// `None`.
    #[test]
    fn default_decoder_matches_artifact_and_feature() {
        let installed = decode_png.read().unwrap().is_some();
        assert_eq!(cfg!(feature = "wuffs"), installed);
    }

    /// The installed default actually decodes a real PNG: the kitty testdata
    /// 50x76 fixture -> 50*76*4 RGBA bytes (wuffs decodes to RGBA_NONPREMUL).
    #[cfg(feature = "wuffs")]
    #[test]
    fn default_decoder_decodes_png() {
        // Deref-copy out of the guard (`Option<fn>` is Copy — the
        // graphics_image.rs:720 `match *sys::decode_png.read().unwrap()` idiom).
        let f = (*decode_png.read().unwrap()).expect("wuffs default decoder installed");
        let img = f(include_bytes!(
            "kitty/testdata/image-png-none-50x76-2147483647-raw.data"
        ))
        .expect("PNG decodes");
        assert_eq!(50, img.width);
        assert_eq!(76, img.height);
        assert_eq!(50 * 76 * 4, img.data.len());
    }
}
