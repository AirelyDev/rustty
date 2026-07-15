//! rustty-wuffs — R1 safe wrapper over the vendored Wuffs image codecs.
//!
//! PORTS: ghostty pkg/wuffs/src/main.zig (+ the `png`/`jpeg`/`error`/`swizzle`
//! submodules). The SAFE `decode` API over the raw `wuffs-sys` FFI leaf.
//!
//! PORT STATUS: ported (main/error/png/jpeg/swizzle every line/comment, R1;
//! swizzle has no reference test{} blocks — new byte-exact KATs added instead).

// const std = @import("std");
use allocator_api2::alloc::Allocator;
use core::alloc::Layout;
use core::ptr::NonNull;

// pub const png = @import("png.zig");
pub mod png;
// pub const jpeg = @import("jpeg.zig");
pub mod jpeg;
// pub const swizzle = @import("swizzle.zig");
pub mod swizzle;
// pub const Error = @import("error.zig").Error;
pub mod error;
pub use error::Error;

// Test-only fixture helpers (the reference `1x1#*`/`too_big` @embedFiles are
// absent from the checkout — F5; PNG fixtures are generated, not checked in).
#[cfg(test)]
pub(crate) mod test_fixtures;

/// The maximum image size, based on the 4G limit of Ghostty's
/// `image-storage-limit` config.
// pub const maximum_image_size = 4 * 1024 * 1024 * 1024;
pub const MAXIMUM_IMAGE_SIZE: usize = 4 * 1024 * 1024 * 1024;

/// Reference `ImageData { width: u32, height: u32, data: []u8 }`.
///
/// PORT NOTE (zig-byteslice-to-rust-rawbytes): the reference `data: []u8` is a
/// slice OWNED by the passed allocator (align 1). Here it is a [`RawBuf`] that owns
/// the same allocation so the caller can either read it (`bytes()`) + let it free
/// on `Drop`, or transfer the raw pointer into the renderer's `image::Pending`
/// (`into_raw`, which forgets the `Drop`) and later free it with the SAME
/// allocator + `Layout::from_size_align(len, 1)`.
// pub const ImageData = struct {
//     width: u32,
//     height: u32,
//     data: []u8,
// };
pub struct ImageData<A: Allocator> {
    pub width: u32,
    pub height: u32,
    pub data: RawBuf<A>,
}

impl<A: Allocator> ImageData<A> {
    /// The decoded RGBA_NONPREMUL bytes (`width * height * 4`).
    pub fn bytes(&self) -> &[u8] {
        self.data.as_slice()
    }

    /// Transfer OWNERSHIP of the decoded buffer out of this `ImageData` (forgets
    /// the `RawBuf` `Drop`): returns `(width, height, ptr, len)`. The caller
    /// becomes responsible for freeing `ptr` with `Layout::from_size_align(len, 1)`
    /// over the SAME allocator (the renderer's `image::Pending` free path). This is
    /// the faithful map of the reference moving `image_data.data.ptr` into
    /// `imagepkg.Image.pending.data` (the `defer alloc.free` frees only `contents`,
    /// never the decoded `destination`).
    pub fn into_raw(self) -> (u32, u32, NonNull<u8>, usize) {
        let ImageData {
            width,
            height,
            data,
        } = self;
        let (ptr, len) = data.into_raw();
        (width, height, ptr, len)
    }
}

/// An owned, align-1 heap buffer allocated over `A` (the decoded RGBA data). The
/// align-1 `Layout` matches the renderer's `image::Pending` free layout exactly
/// (`free_data`: `Layout::from_size_align(p.len(), 1)`), so the pointer can be
/// transferred (`ImageData::into_raw`) and freed later without a layout mismatch.
pub struct RawBuf<A: Allocator> {
    ptr: NonNull<u8>,
    len: usize,
    alloc: A,
}

impl<A: Allocator> RawBuf<A> {
    /// `alloc.alloc(u8, len)` — allocate `len` bytes (align 1).
    pub(crate) fn alloc(alloc: A, len: usize) -> Result<Self, Error> {
        // align 1: matches the renderer's `Pending` free layout (the load-bearing
        // allocator invariant — RENDERER_BG_IMAGE_DESIGN §3.2 / R-2).
        let layout = Layout::from_size_align(len, 1).map_err(|_| Error::OutOfMemory)?;
        let ptr = alloc.allocate(layout).map_err(|_| Error::OutOfMemory)?;
        Ok(RawBuf {
            ptr: ptr.cast(),
            len,
            alloc,
        })
    }

    /// The buffer as a byte slice.
    pub fn as_slice(&self) -> &[u8] {
        // SAFETY: `ptr` points at `len` initialized (decoded) bytes owned by self.
        unsafe { core::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// The byte length (`width * height * 4` for a decoded RGBA image).
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.len
    }

    /// The raw base pointer (for handing to a Wuffs `slice_u8`).
    pub(crate) fn ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Consume the buffer WITHOUT freeing (`mem::forget` the `Drop`), returning the
    /// raw `(ptr, len)`. The caller owns the allocation from here.
    fn into_raw(self) -> (NonNull<u8>, usize) {
        let out = (self.ptr, self.len);
        core::mem::forget(self);
        out
    }
}

impl<A: Allocator> Drop for RawBuf<A> {
    fn drop(&mut self) {
        // The align-1 layout that `alloc` used; freeing with the matched layout.
        let layout =
            Layout::from_size_align(self.len, 1).expect("valid layout (matched a prior alloc)");
        // SAFETY: `ptr`/`layout` were produced by `RawBuf::alloc` via `self.alloc`;
        // `self` owns the buffer (only owned `RawBuf`s reach `Drop` — `into_raw`
        // forgets).
        unsafe {
            self.alloc.deallocate(self.ptr, layout);
        }
    }
}

/// A transient, self-freeing scratch buffer over `A` (the Wuffs decoder struct
/// backing + the intermediate work buffer). Freed on scope exit — the faithful map
/// of the reference `defer alloc.free(decoder_buf)` / `defer alloc.free(work_buffer)`.
/// Its `Layout` (size + align) is stored so `Drop` frees with the exact layout the
/// alloc used; unlike [`RawBuf`] it is NEVER transferred, so any alignment is fine
/// (the decoder backing is u64-aligned — the wuffs-sys KAT precedent — since the
/// opaque decoder struct holds u64 fields).
pub(crate) struct Scratch<A: Allocator> {
    ptr: NonNull<u8>,
    layout: Layout,
    alloc: A,
}

impl<A: Allocator> Scratch<A> {
    pub(crate) fn alloc(alloc: A, size: usize, align: usize) -> Result<Self, Error> {
        let layout = Layout::from_size_align(size, align).map_err(|_| Error::OutOfMemory)?;
        let ptr = alloc.allocate(layout).map_err(|_| Error::OutOfMemory)?;
        Ok(Scratch {
            ptr: ptr.cast(),
            layout,
            alloc,
        })
    }

    pub(crate) fn ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    pub(crate) fn len(&self) -> usize {
        self.layout.size()
    }
}

impl<A: Allocator> Drop for Scratch<A> {
    fn drop(&mut self) {
        // SAFETY: `ptr`/`layout` were produced by `Scratch::alloc` via `self.alloc`.
        unsafe {
            self.alloc.deallocate(self.ptr, self.layout);
        }
    }
}

// test {
//     std.testing.refAllDeclsRecursive(@This());
// }
//
// PORT NOTE: Zig's `refAllDeclsRecursive` forces every public decl to be
// semantically analyzed (a compile-touch). The faithful Rust map is a test that
// references the crate's public surface so a regression that drops/renames a decl
// fails to compile.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_all_decls() {
        assert_eq!(MAXIMUM_IMAGE_SIZE, 4 * 1024 * 1024 * 1024);
        // Touch each public decode entrypoint + the error type.
        let _decode_png: fn(
            allocator_api2::alloc::Global,
            &[u8],
        ) -> Result<ImageData<allocator_api2::alloc::Global>, Error> = png::decode;
        let _decode_jpeg: fn(
            allocator_api2::alloc::Global,
            &[u8],
        ) -> Result<ImageData<allocator_api2::alloc::Global>, Error> = jpeg::decode;
        let _e = Error::WuffsError;
    }
}
