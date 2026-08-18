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

// PORT NOTE: re-exported (`pub use`) so seam consumers (rustty-vt-c's C
// decode wrapper) can name the allocator interface through `sys` without a
// direct rustty-datastruct edge.
pub use rustty_datastruct::limited_allocator::{Alignment, RawAllocator, RawSlice};
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
// The `Allocator` parameter is the datastruct `RawAllocator` interface (the
// same `std.mem.Allocator` analog `LimitedAllocator` wraps): the kitty decode
// path (`graphics_image.rs decodePng`, oracle `590d669c4`) hands the decoder a
// `LimitedAllocator` so every decoder allocation is capped BEFORE the memory
// is requested; a decoder must perform its allocations through this parameter.
pub type DecodePngFn = fn(&dyn RawAllocator, &[u8]) -> Result<Image, DecodeError>;

/// The `RawAllocator` over the Rust global allocator — the rustty analog of the
/// plain `std.mem.Allocator` callers pass where the reference threads `alloc`
/// (the datastruct copy of this shape lives in its tests; this is the shared
/// production instance backing the decode seam).
pub struct GlobalRawAllocator;

impl RawAllocator for GlobalRawAllocator {
    fn raw_alloc(&self, len: usize, alignment: Alignment, _ra: usize) -> *mut u8 {
        if len == 0 {
            // Zig: a zero-length alloc returns a valid, aligned,
            // dereference-free pointer; model it with a non-null aligned
            // sentinel so the global allocator never sees a 0-size layout.
            return alignment.to_byte_units() as *mut u8;
        }
        let layout = std::alloc::Layout::from_size_align(len, alignment.to_byte_units()).unwrap();
        // SAFETY(zig-port-idiom/sys): `layout` has a nonzero size (guarded
        // above) and a power-of-two alignment (`Alignment::to_byte_units`), the
        // contract for `std::alloc::alloc`.
        unsafe { std::alloc::alloc(layout) }
    }

    fn raw_resize(
        &self,
        memory: RawSlice,
        _alignment: Alignment,
        new_len: usize,
        _ra: usize,
    ) -> bool {
        // Conservative in-place resize: only succeed when shrinking-or-equal.
        new_len <= memory.len
    }

    fn raw_remap(
        &self,
        memory: RawSlice,
        alignment: Alignment,
        new_len: usize,
        _ra: usize,
    ) -> *mut u8 {
        if memory.len == 0 {
            return self.raw_alloc(new_len, alignment, 0);
        }
        let old_layout =
            std::alloc::Layout::from_size_align(memory.len, alignment.to_byte_units()).unwrap();
        // SAFETY(zig-port-idiom/sys): `memory.ptr` came from `raw_alloc` with
        // `old_layout` (nonzero len guarded above), the contract for
        // `std::alloc::realloc`.
        unsafe { std::alloc::realloc(memory.ptr, old_layout, new_len) }
    }

    fn raw_free(&self, memory: RawSlice, alignment: Alignment, _ra: usize) {
        if memory.len == 0 {
            return;
        }
        let layout =
            std::alloc::Layout::from_size_align(memory.len, alignment.to_byte_units()).unwrap();
        // SAFETY(zig-port-idiom/sys): `memory.ptr` came from
        // `raw_alloc`/`raw_remap` with this exact `layout` (nonzero len guarded
        // above), the contract for `std::alloc::dealloc`.
        unsafe { std::alloc::dealloc(memory.ptr, layout) }
    }
}

/// rustty-only test synchronization for the swappable `decode_png` slot: the
/// reference's tests run single-threaded, `cargo test` does not, so every test
/// that installs into the slot — or depends on which decoder is installed —
/// holds this lock for its whole body.
#[cfg(test)]
pub static decode_png_test_lock: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
//
// CAVEAT (codex-vet W36-B03): that split holds PER CARGO SELECTION, not
// per artifact — the reference compiles lib-vt separately (GhosttyZig.zig
// `vt_options.artifact = .lib`), but a COMBINED Cargo selection containing
// both tiers (`--workspace`, or `-p rustty-vt-c -p rustty-core`) feature-
// unifies `wuffs` into the single compiled rustty-vt, flipping the vt-c
// artifact's default to `Some`. Publish rule: librustty-vt artifacts come
// from ISOLATED `-p rustty-vt-c` invocations only; gated by G-VTCLIB
// (.port/tools/gates/check_vtc_lib_isolation.sh) and ledgered in
// DEFERRALS.md §"decode_png" ADDENDUM.
//
// We use a `RwLock` so the global is safely mutable across threads (the
// reference relies on Zig's `var` being mutated only at startup).
pub static decode_png: RwLock<Option<DecodePngFn>> = RwLock::new(DECODE_PNG_DEFAULT);

#[cfg(feature = "wuffs")]
const DECODE_PNG_DEFAULT: Option<DecodePngFn> = if matches!(
    crate::options::ARTIFACT,
    crate::build_options::Artifact::Lib
) {
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

/// PORT NOTE (`590d669c4` seam): the wuffs binding's `decode` is generic over
/// an `allocator_api2::alloc::Allocator`; this adapter views the seam's
/// `&dyn RawAllocator` (e.g. the kitty path's `LimitedAllocator`) as one, so
/// every wuffs allocation — decoder struct, work buffer, and the decoded pixel
/// buffer — is requested through the seam allocator and therefore capped.
#[cfg(feature = "wuffs")]
#[derive(Clone, Copy)]
struct WuffsRawAlloc<'a>(&'a dyn RawAllocator);

#[cfg(feature = "wuffs")]
// SAFETY(zig-port-idiom/sys): delegates allocate/deallocate 1:1 to the
// `RawAllocator` vtable (the `std.mem.Allocator` analog), which upholds the
// same validity/ownership contract `Allocator` requires.
unsafe impl allocator_api2::alloc::Allocator for WuffsRawAlloc<'_> {
    fn allocate(
        &self,
        layout: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, allocator_api2::alloc::AllocError> {
        let ptr = self
            .0
            .raw_alloc(layout.size(), Alignment::from_byte_units(layout.align()), 0);
        match std::ptr::NonNull::new(ptr) {
            Some(p) => Ok(std::ptr::NonNull::slice_from_raw_parts(p, layout.size())),
            None => Err(allocator_api2::alloc::AllocError),
        }
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout) {
        self.0.raw_free(
            RawSlice::new(ptr.as_ptr(), layout.size()),
            Alignment::from_byte_units(layout.align()),
            0,
        );
    }
}

// fn decodePngWuffs(
//     alloc: Allocator,
//     data: []const u8,
// ) DecodeError!Image {
#[cfg(feature = "wuffs")]
fn decode_png_wuffs(alloc: &dyn RawAllocator, data: &[u8]) -> Result<Image, DecodeError> {
    // const wuffs = @import("wuffs");
    // const result = wuffs.png.decode(
    //     alloc,
    //     data,
    // ) catch |err| switch (err) {
    //     error.WuffsError => return error.InvalidData,
    //     error.OutOfMemory => return error.OutOfMemory,
    //     error.Overflow => return error.InvalidData,
    // };
    let result = match rustty_wuffs::png::decode(WuffsRawAlloc(alloc), data) {
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
    // PORT NOTE: the reference moves the alloc-owned `[]u8` into `Image.data`
    // and the CALLER frees it through the same allocator after copying
    // (`decodePng`'s `defer decode_alloc.free(result.data)` + `appendSlice`).
    // `sys::Image.data` is a global-allocator `Vec<u8>`, so the copy happens
    // here instead and the decode-allocator buffer is freed on `result`'s drop
    // — same single copy out of the decode allocation, relocated.
    // PERF(port): the reference's zero-copy handoff into the caller's
    // `appendSlice` source; one extra decoded-image copy per PNG decode.
    let data = result.bytes().to_vec();
    Ok(Image {
        width: result.width,
        height: result.height,
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
        let _png_lock = decode_png_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let installed = decode_png.read().unwrap().is_some();
        assert_eq!(cfg!(feature = "wuffs"), installed);
    }

    /// The installed default actually decodes a real PNG: the kitty testdata
    /// 50x76 fixture -> 50*76*4 RGBA bytes (wuffs decodes to RGBA_NONPREMUL).
    #[cfg(feature = "wuffs")]
    #[test]
    fn default_decoder_decodes_png() {
        let _png_lock = decode_png_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // Deref-copy out of the guard (`Option<fn>` is Copy — the
        // graphics_image.rs:720 `match *sys::decode_png.read().unwrap()` idiom).
        let f = (*decode_png.read().unwrap()).expect("wuffs default decoder installed");
        let img = f(
            &GlobalRawAllocator,
            include_bytes!("kitty/testdata/image-png-none-50x76-2147483647-raw.data"),
        )
        .expect("PNG decodes");
        assert_eq!(50, img.width);
        assert_eq!(76, img.height);
        assert_eq!(50 * 76 * 4, img.data.len());
    }
}
