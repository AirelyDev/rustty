//! PORTS: ghostty pkg/wuffs/src/zlib.zig — the safe zlib `decode`
//! (`4b86e3594`, decoder alignment `42049f1c4`).
//!
//! PORT STATUS: ported (every line/comment/test{}, R1).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
use allocator_api2::alloc::Allocator;
// const c = @import("wuffs_c");
use wuffs_sys::{
    sizeof__wuffs_zlib__decoder, wuffs_base__io_buffer, wuffs_base__io_buffer_meta,
    wuffs_base__slice_u8, wuffs_base__suspension__short_write, wuffs_zlib__decoder,
    wuffs_zlib__decoder__initialize, wuffs_zlib__decoder__transform_io, WUFFS_VERSION,
};
// const Error = @import("error.zig").Error;
// const check = @import("error.zig").check;
use crate::error::{check, Error};
use crate::{RawBuf, Scratch};

// const log = std.log.scoped(.wuffs_zlib);
//
// PORT NOTE: the reference's scope tag, carried through the `log` facade as
// `target: LOG_SCOPE` (DECISION-logging.md §6r4) and threaded into
// `error::check` where the reference threads its `log`.
pub(crate) const LOG_SCOPE: &str = "wuffs_zlib";

/// Decode a complete RFC 1950 zlib stream. `size_hint` is the expected output
/// size when known; the output may grow up to `limit` bytes.
// pub fn decode(
//     alloc: Allocator,
//     data: []const u8,
//     limit: usize,
//     size_hint: ?usize,
// ) Error![]u8 {
//
// PORT NOTE (zig-byteslice-to-rust-rawbytes): the returned `[]u8` is owned by
// `alloc` (align 1); here it is a [`RawBuf`] over the same allocation, which the
// caller reads, drops (frees), or takes over with `RawBuf::into_raw`.
pub fn decode<A: Allocator + Copy>(
    alloc: A,
    data: &[u8],
    limit: usize,
    size_hint: Option<usize>,
) -> Result<RawBuf<A>, Error> {
    // if (limit == 0) return error.Overflow;
    if limit == 0 {
        return Err(Error::Overflow);
    }

    // Wuffs' translated decoder is opaque, so allocate its storage with Zig.
    // Request alignment explicitly since a byte allocation need not satisfy
    // the alignment of the underlying C struct.
    // const decoder_buf = try alloc.alignedAlloc(u8, .@"16", c.sizeof__wuffs_zlib__decoder());
    // defer alloc.free(decoder_buf);
    // SAFETY: FFI call to a pure size accessor (no receiver).
    let decoder_size = unsafe { sizeof__wuffs_zlib__decoder() };
    let decoder_buf = Scratch::alloc(alloc, decoder_size, 16)?;
    // const decoder: ?*c.wuffs_zlib__decoder = @ptrCast(decoder_buf);
    let decoder = decoder_buf.ptr() as *mut wuffs_zlib__decoder;
    {
        // const status = c.wuffs_zlib__decoder__initialize(
        //     decoder,
        //     c.sizeof__wuffs_zlib__decoder(),
        //     c.WUFFS_VERSION,
        //     0,
        // );
        // SAFETY: `decoder` points at `decoder_size` 16-aligned bytes it owns;
        // `initialize` (options 0) zeroes and sets up the whole struct.
        let status = unsafe {
            wuffs_zlib__decoder__initialize(decoder, decoder_size, WUFFS_VERSION as u64, 0)
        };
        // try check(log, &status);
        check(LOG_SCOPE, &status)?;
    }

    // var source: c.wuffs_base__io_buffer = .{
    //     .data = .{ .ptr = @ptrCast(@constCast(data.ptr)), .len = data.len },
    //     .meta = .{ .wi = data.len, .ri = 0, .pos = 0, .closed = true },
    // };
    //
    // PORT NOTE: the `@constCast` is the same here: Wuffs only reads a source
    // buffer (it advances `meta.ri`, never writes `data`).
    let mut source = wuffs_base__io_buffer {
        data: wuffs_base__slice_u8 {
            ptr: data.as_ptr() as *mut u8,
            len: data.len(),
        },
        meta: wuffs_base__io_buffer_meta {
            wi: data.len(),
            ri: 0,
            pos: 0,
            closed: true,
        },
    };

    // const initial_size = @max(1, @min(limit, size_hint orelse
    //     std.math.mul(usize, data.len, 2) catch limit));
    let initial_size = 1usize
        .max(limit.min(size_hint.unwrap_or_else(|| data.len().checked_mul(2).unwrap_or(limit))));
    // var destination = try alloc.alloc(u8, initial_size);
    // errdefer alloc.free(destination);
    //
    // PORT NOTE: the `errdefer` is `RawBuf`'s `Drop`: every early return below
    // drops `destination`, and the success path moves it out.
    let mut destination = RawBuf::alloc(alloc, initial_size)?;

    // var output: c.wuffs_base__io_buffer = .{
    //     .data = .{ .ptr = @ptrCast(destination.ptr), .len = destination.len },
    //     .meta = .{ .wi = 0, .ri = 0, .pos = 0, .closed = false },
    // };
    let mut output = wuffs_base__io_buffer {
        data: wuffs_base__slice_u8 {
            ptr: destination.ptr(),
            len: destination.len(),
        },
        meta: wuffs_base__io_buffer_meta {
            wi: 0,
            ri: 0,
            pos: 0,
            closed: false,
        },
    };
    loop {
        // const status = c.wuffs_zlib__decoder__transform_io(
        //     decoder,
        //     &output,
        //     &source,
        //     c.wuffs_base__empty_slice_u8(),
        // );
        //
        // PORT NOTE: `wuffs_base__empty_slice_u8` is a `static inline` (no
        // linkable symbol); it is the 2-field struct `{ NULL, 0 }`, built by
        // hand as the png/jpeg ports build `make_slice_u8`.
        // SAFETY: `decoder` is initialized; `output` covers `destination`'s
        // owned bytes and `source` covers `data`, both live across the call.
        let status = unsafe {
            wuffs_zlib__decoder__transform_io(
                decoder,
                &mut output,
                &mut source,
                wuffs_base__slice_u8 {
                    ptr: core::ptr::null_mut(),
                    len: 0,
                },
            )
        };
        // if (c.wuffs_base__status__is_ok(&status)) {
        //
        // PORT NOTE: `wuffs_base__status__is_ok` is a `static inline`: a status
        // is OK iff its `repr` is NULL (read directly, as `error::check` does).
        if status.repr.is_null() {
            // if (source.meta.ri != data.len) return error.WuffsError;
            if source.meta.ri != data.len() {
                return Err(Error::WuffsError);
            }
            // return try alloc.realloc(destination, output.meta.wi);
            destination.realloc(output.meta.wi)?;
            return Ok(destination);
        }
        // if (status.repr != c.wuffs_base__suspension__short_write) {
        //     try check(log, &status);
        //     unreachable;
        // }
        //
        // PORT NOTE: an address comparison, as in the reference: Wuffs statuses
        // are identified by the address of their message string.
        // SAFETY: taking the address of an extern static array; nothing is read.
        let short_write = unsafe { wuffs_base__suspension__short_write.as_ptr() };
        if status.repr != short_write {
            check(LOG_SCOPE, &status)?;
            unreachable!();
        }
        // if (output.meta.wi != destination.len) return error.WuffsError;
        if output.meta.wi != destination.len() {
            return Err(Error::WuffsError);
        }
        // if (destination.len == limit) return error.Overflow;
        if destination.len() == limit {
            return Err(Error::Overflow);
        }

        // const next_size = destination.len + @min(destination.len, limit - destination.len);
        let next_size = destination.len() + destination.len().min(limit - destination.len());
        // destination = try alloc.realloc(destination, next_size);
        destination.realloc(next_size)?;
        // output.data = .{ .ptr = @ptrCast(destination.ptr), .len = destination.len };
        output.data = wuffs_base__slice_u8 {
            ptr: destination.ptr(),
            len: destination.len(),
        };
    }
}

// Wuffs is C, which Miri cannot execute: every test below runs the vendored
// decoder, so each is ignored under Miri (R14) and runs natively.
#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::{AllocError, Global};
    use core::alloc::Layout;
    use core::cell::Cell;
    use core::ptr::NonNull;

    const COMPRESSED: [u8; 19] = [
        0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0x28, 0x2f, //
        0x4d, 0x4b, 0x2b, 0x06, 0x00, 0x1a, 0x02, 0x04, 0x60,
    ];

    // test "zlib decode grows output" {
    #[test]
    #[cfg_attr(miri, ignore = "calls the vendored Wuffs C decoder (R14)")]
    fn zlib_decode_grows_output() {
        // const compressed = [_]u8{ ... };
        // const output = try decode(std.testing.allocator, &compressed, 64, 1);
        // defer std.testing.allocator.free(output);
        let output = decode(Global, &COMPRESSED, 64, Some(1)).unwrap();
        // try std.testing.expectEqualStrings("hello wuffs", output);
        assert_eq!(b"hello wuffs", output.as_slice());
    }

    // test "zlib decode enforces output limit" {
    #[test]
    #[cfg_attr(miri, ignore = "calls the vendored Wuffs C decoder (R14)")]
    fn zlib_decode_enforces_output_limit() {
        // try std.testing.expectError(error.Overflow, decode(std.testing.allocator, &compressed, 10, null));
        assert_eq!(
            Some(Error::Overflow),
            decode(Global, &COMPRESSED, 10, None).err()
        );
    }

    // test "zlib decode accepts exact limit and rejects truncated input" {
    #[test]
    #[cfg_attr(miri, ignore = "calls the vendored Wuffs C decoder (R14)")]
    fn zlib_decode_accepts_exact_limit_and_rejects_truncated_input() {
        // const output = try decode(std.testing.allocator, &compressed, 11, 11);
        // defer std.testing.allocator.free(output);
        let output = decode(Global, &COMPRESSED, 11, Some(11)).unwrap();
        // try std.testing.expectEqualStrings("hello wuffs", output);
        assert_eq!(b"hello wuffs", output.as_slice());
        // try std.testing.expectError(error.WuffsError, decode(std.testing.allocator, compressed[0 .. compressed.len - 1], 64, null));
        assert_eq!(
            Some(Error::WuffsError),
            decode(Global, &COMPRESSED[..COMPRESSED.len() - 1], 64, None).err()
        );
    }

    /// `std.heap.FixedBufferAllocator`, for the alignment test below: a bump
    /// allocator over a caller buffer that honours the requested alignment
    /// (`alignPointerOffset` from the current end), resizes in place only the
    /// last allocation, and frees only the last allocation.
    #[derive(Clone, Copy)]
    struct FixedBufferAllocator<'a> {
        buffer: NonNull<u8>,
        len: usize,
        end_index: &'a Cell<usize>,
    }

    impl FixedBufferAllocator<'_> {
        fn is_last(&self, ptr: NonNull<u8>, len: usize) -> bool {
            ptr.as_ptr() as usize + len == self.buffer.as_ptr() as usize + self.end_index.get()
        }
    }

    // SAFETY: every block handed out is a distinct, in-bounds range of the
    // caller's buffer, aligned as the layout asks, and is only reused after
    // it is freed (the end index moves back only over the last block).
    unsafe impl Allocator for FixedBufferAllocator<'_> {
        fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
            let base = self.buffer.as_ptr() as usize;
            let cur = base + self.end_index.get();
            let start = cur.next_multiple_of(layout.align()) - base;
            let end = start.checked_add(layout.size()).ok_or(AllocError)?;
            if end > self.len {
                return Err(AllocError);
            }
            self.end_index.set(end);
            // SAFETY: `start <= end <= len`, inside the buffer.
            let ptr = unsafe { NonNull::new_unchecked(self.buffer.as_ptr().add(start)) };
            Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
        }

        unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
            if self.is_last(ptr, layout.size()) {
                self.end_index.set(self.end_index.get() - layout.size());
            }
        }

        unsafe fn grow(
            &self,
            ptr: NonNull<u8>,
            old: Layout,
            new: Layout,
        ) -> Result<NonNull<[u8]>, AllocError> {
            if !self.is_last(ptr, old.size()) {
                return Err(AllocError);
            }
            let end = self.end_index.get() + (new.size() - old.size());
            if end > self.len {
                return Err(AllocError);
            }
            self.end_index.set(end);
            Ok(NonNull::slice_from_raw_parts(ptr, new.size()))
        }

        unsafe fn shrink(
            &self,
            ptr: NonNull<u8>,
            old: Layout,
            new: Layout,
        ) -> Result<NonNull<[u8]>, AllocError> {
            if self.is_last(ptr, old.size()) {
                self.end_index
                    .set(self.end_index.get() - (old.size() - new.size()));
            }
            Ok(NonNull::slice_from_raw_parts(ptr, new.size()))
        }
    }

    // test "zlib decode aligns decoder in an unaligned allocator buffer" {
    #[test]
    #[cfg_attr(miri, ignore = "calls the vendored Wuffs C decoder (R14)")]
    fn zlib_decode_aligns_decoder_in_an_unaligned_allocator_buffer() {
        // const memory = try testing.allocator.alignedAlloc(
        //     u8,
        //     .@"16",
        //     c.sizeof__wuffs_zlib__decoder() + 128,
        // );
        // defer testing.allocator.free(memory);
        // SAFETY: FFI call to a pure size accessor (no receiver).
        let size = unsafe { sizeof__wuffs_zlib__decoder() } + 128;
        let layout = Layout::from_size_align(size, 16).unwrap();
        let memory = Global.allocate(layout).unwrap().cast::<u8>();

        // A byte allocation from this allocator starts at an unaligned address
        // unless the caller explicitly requests a stronger alignment.
        // var fba = std.heap.FixedBufferAllocator.init(memory[1..]);
        // const alloc = fba.allocator();
        let end_index = Cell::new(0);
        let alloc = FixedBufferAllocator {
            // SAFETY: `size >= 1`, so `memory + 1` is inside the block.
            buffer: unsafe { NonNull::new_unchecked(memory.as_ptr().add(1)) },
            len: size - 1,
            end_index: &end_index,
        };
        {
            // const output = try decode(alloc, &compressed, 64, 11);
            // defer alloc.free(output);
            let output = decode(alloc, &COMPRESSED, 64, Some(11)).unwrap();
            // try testing.expectEqualStrings("hello wuffs", output);
            assert_eq!(b"hello wuffs", output.as_slice());
        }
        // SAFETY: `memory` came from `Global.allocate(layout)`.
        unsafe { Global.deallocate(memory, layout) };
    }

    // test "zlib decode rejects corrupted checksum" {
    #[test]
    #[cfg_attr(miri, ignore = "calls the vendored Wuffs C decoder (R14)")]
    fn zlib_decode_rejects_corrupted_checksum() {
        let mut compressed = COMPRESSED;
        // Change only the Adler-32 trailer, leaving the deflate data intact.
        // compressed[compressed.len - 1] ^= 1;
        let last = compressed.len() - 1;
        compressed[last] ^= 1;
        // try std.testing.expectError(error.WuffsError, decode(std.testing.allocator, &compressed, 64, 11));
        assert_eq!(
            Some(Error::WuffsError),
            decode(Global, &compressed, 64, Some(11)).err()
        );
    }
}
