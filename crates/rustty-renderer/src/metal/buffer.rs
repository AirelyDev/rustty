//! PORTS: ghostty `src/renderer/metal/buffer.zig` — the `Buffer(comptime T)`
//! Metal data-storage wrapper. Per the GPU R-d sub-blueprint
//! (`GPU_RD_TRAIT_DESIGN.md` §B), the Zig comptime `Buffer(T)` maps 1:1 onto a
//! concrete `pub struct Buffer<T: Copy>` (the one place in the port where a Zig
//! comptime generic becomes a true Rust generic with zero friction — it is a
//! leaf value type, not a trait-object seam). `T: Copy` is load-bearing: the
//! `sync` memcpy copies `T`'s raw bytes.
//!
//! PORT NOTE (R3/objc2, ARC): ghostty's manual reference counting dissolves
//! into `Retained` ownership + `Drop`. Specifically:
//!   * `deinit()` (`self.buffer.msgSend(void, "release")`) → no explicit `Drop`
//!     impl is needed; the `Retained<ProtocolObject<dyn MTLBuffer>>` field
//!     releases automatically when the `Buffer` is dropped.
//!   * `sync`/`syncFromArrayLists`'s explicit `release` before reallocating →
//!     dissolves into the `self.buffer = …` field assignment (the old
//!     `Retained`'s `Drop` releases the previous buffer).
//!
//! PORT NOTE (R1, Option vs raw Object): objc2's `newBufferWith*` return
//! `Option<Retained<…>>` where ghostty's `objc.Object` `msgSend` returns a raw
//! (possibly-nil) `Object` that it stores without a null check. The nil case
//! (a Metal allocation failure) is surfaced here as `Error::MetalFailed` rather
//! than a deferred nil-pointer crash — the conservative faithful handling of
//! the nil that ghostty would have stored.

use std::marker::PhantomData;

use core::ffi::c_void;
use core::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSRange;
use objc2_metal::{MTLBuffer, MTLDevice};

use crate::metal::api;

// const log = std.log.scoped(.metal);
// PORT NOTE: no logging facade is wired in the port yet, so the `log.warn` call
// site (in `sync`) is preserved as a comment (mirrors the tree-wide
// `std.log.scoped` handling in e.g. `shadertoy.rs`).

/// The error set for buffer operations.
//
// PORT NOTE: `buffer.zig` has no explicit `pub const Error`; its `sync`/
// `syncFromArrayLists`/`init`/`initFill` use inferred error unions whose only
// member is `error.MetalFailed`. Collected here into one enum, matching the
// explicit `Error` sets in `Texture.zig`/`Sampler.zig`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// A Metal API call failed.
    MetalFailed,
}

/// Options for initializing a buffer.
//
// PORT NOTE (§C): ghostty's `buffer.Options` is named `BufferOptions` here so
// the three leaf option types (`BufferOptions`/`TextureOptions`/
// `SamplerOptions`) are distinct at `metal::` scope (Rust lacks Zig's
// per-file `buffer.Options`/`Texture.Options` namespacing).
pub struct BufferOptions {
    /// MTLDevice
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub resource_options: api::MTLResourceOptions,
}

/// Metal data storage for a certain set of equal types. This is usually
/// used for vertex buffers, etc. This helpful wrapper makes it easy to
/// prealloc, shrink, grow, sync, buffers with Metal.
pub struct Buffer<T: Copy> {
    /// The options this buffer was initialized with.
    pub opts: BufferOptions,

    /// The underlying MTLBuffer object.
    pub buffer: Retained<ProtocolObject<dyn MTLBuffer>>,

    /// The allocated length of the buffer.
    /// Note that this is the number
    /// of `T`s not the size in bytes.
    pub len: usize,

    // PORT NOTE: Zig carries `T` in the `Buffer(T)` type itself; Rust needs a
    // `PhantomData<T>` because `T` appears in no field.
    _marker: PhantomData<T>,
}

impl<T: Copy> Buffer<T> {
    /// Initialize a buffer with the given length pre-allocated.
    pub fn init(opts: BufferOptions, len: usize) -> Result<Self, Error> {
        let buffer = opts
            .device
            .newBufferWithLength_options(
                len * std::mem::size_of::<T>(),
                opts.resource_options.resolve(),
            )
            .ok_or(Error::MetalFailed)?;

        Ok(Self {
            buffer,
            opts,
            len,
            _marker: PhantomData,
        })
    }

    /// Init the buffer filled with the given data.
    pub fn init_fill(opts: BufferOptions, data: &[T]) -> Result<Self, Error> {
        let ptr = NonNull::new(data.as_ptr() as *mut c_void).ok_or(Error::MetalFailed)?;
        // SAFETY: `ptr` is derived from a valid `&[T]`; the length matches the
        // slice's byte extent.
        let buffer = unsafe {
            opts.device.newBufferWithBytes_length_options(
                ptr,
                // ghostty: `data.len * @sizeOf(T)` — for a `&[T]`,
                // `size_of_val` is definitionally that product (clippy).
                std::mem::size_of_val(data),
                opts.resource_options.resolve(),
            )
        }
        .ok_or(Error::MetalFailed)?;

        Ok(Self {
            buffer,
            opts,
            len: data.len(),
            _marker: PhantomData,
        })
    }

    // PORT NOTE (ARC): `deinit()` (`self.buffer.msgSend(void, "release")`)
    // dissolves into the `Retained` field's automatic `Drop` — no manual impl.

    /// Sync new contents to the buffer. The data is expected to be the
    /// complete contents of the buffer. If the amount of data is larger
    /// than the buffer length, the buffer will be reallocated.
    ///
    /// If the amount of data is smaller than the buffer length, the
    /// remaining data in the buffer is left untouched.
    pub fn sync(&mut self, data: &[T]) -> Result<(), Error> {
        // If we need more bytes than our buffer has, we need to reallocate.
        // (ghostty: `data.len * @sizeOf(T)`; `size_of_val` of a `&[T]` is that
        // product — clippy manual_slice_size_calculation.)
        let req_bytes = std::mem::size_of_val(data);
        let avail_bytes = self.buffer.length();
        if req_bytes > avail_bytes {
            // Deallocate previous buffer
            // PORT NOTE (ARC): the explicit `release` dissolves into the
            // `self.buffer = …` assignment below (old `Retained`'s `Drop`).

            // Allocate a new buffer with enough to hold double what we require.
            let size = req_bytes * 2;
            self.buffer = self
                .opts
                .device
                .newBufferWithLength_options(
                    // FAITHFULNESS QUIRK (R1): ghostty passes `size * @sizeOf(T)`
                    // === `req_bytes * 2 * size_of::<T>()`, over-allocating by a
                    // `size_of::<T>()` factor (a latent reference quirk; harmless
                    // — the buffer is always ≥ required). Reproduced byte-for-byte
                    // as the exact `size * size_of::<T>()` expression, NOT
                    // corrected.
                    size * std::mem::size_of::<T>(),
                    self.opts.resource_options.resolve(),
                )
                .ok_or(Error::MetalFailed)?;
        }

        // We can fit within the buffer so we can just replace bytes.
        let dst = {
            // PORT NOTE: objc2's `contents()` returns `NonNull<c_void>` (never
            // null), so ghostty's null-guard is structurally unreachable and is
            // preserved here as this comment:
            //     const ptr = self.buffer.msgSend(?[*]u8, objc.sel("contents"), .{}) orelse {
            //         log.warn("buffer contents ptr is null", .{});
            //         return error.MetalFailed;
            //     };
            self.buffer.contents().as_ptr() as *mut u8
        };

        let src = data.as_ptr() as *const u8;

        // SAFETY: `dst` points at `req_bytes` of Metal-owned storage (the buffer
        // is ≥ `req_bytes`); `src` is `req_bytes` of `data`'s bytes. Regions do
        // not overlap. (@memcpy of `dst[0..req_bytes]` from `src[0..req_bytes]`.)
        unsafe {
            std::ptr::copy_nonoverlapping(src, dst, req_bytes);
        }

        // If we're using the managed resource storage mode, then
        // we need to signal Metal to synchronize the buffer data.
        //
        // Ref: https://developer.apple.com/documentation/metal/synchronizing-a-managed-resource-in-macos?language=objc
        if self.opts.resource_options.storage_mode == api::StorageMode::Managed {
            self.buffer.didModifyRange(NSRange::new(0, req_bytes));
        }

        Ok(())
    }

    /// Like Buffer.sync but takes data from an array of ArrayLists,
    /// rather than a single array. Returns the number of items synced.
    //
    // PORT NOTE: Zig's `[]const std.ArrayListUnmanaged(T)` → `&[Vec<T>]` (the
    // already-ported `ArrayListCollection<T>.lists` is a `Vec<Vec<T>>`).
    pub fn sync_from_array_lists(&mut self, lists: &[Vec<T>]) -> Result<usize, Error> {
        let mut total_len: usize = 0;
        for list in lists {
            total_len += list.len();
        }

        // If we need more bytes than our buffer has, we need to reallocate.
        let req_bytes = total_len * std::mem::size_of::<T>();
        let avail_bytes = self.buffer.length();
        if req_bytes > avail_bytes {
            // Deallocate previous buffer
            // PORT NOTE (ARC): the explicit `release` dissolves into the
            // `self.buffer = …` assignment below (old `Retained`'s `Drop`).

            // Allocate a new buffer with enough to hold double what we require.
            let size = req_bytes * 2;
            self.buffer = self
                .opts
                .device
                .newBufferWithLength_options(
                    // FAITHFULNESS QUIRK (R1): see `sync` — the `size * @sizeOf(T)`
                    // over-allocation is reproduced verbatim, NOT corrected.
                    size * std::mem::size_of::<T>(),
                    self.opts.resource_options.resolve(),
                )
                .ok_or(Error::MetalFailed)?;
        }

        // We can fit within the buffer so we can just replace bytes.
        let dst = {
            // PORT NOTE: see `sync` — objc2's `contents()` is `NonNull`, so
            // ghostty's null-guard (`log.warn` + `error.MetalFailed`) is
            // structurally unreachable and preserved as this comment.
            self.buffer.contents().as_ptr() as *mut u8
        };

        let mut i: usize = 0;

        for list in lists {
            let src = list.as_ptr() as *const u8;
            let n = list.len() * std::mem::size_of::<T>();
            // SAFETY: `dst[i..][0..n]` lies within the ≥ `req_bytes` buffer; `src`
            // is `n` bytes of `list`'s items. Non-overlapping.
            // (@memcpy(dst[i..][0 .. list.items.len * @sizeOf(T)], ptr).)
            unsafe {
                std::ptr::copy_nonoverlapping(src, dst.add(i), n);
            }
            i += n;
        }

        // If we're using the managed resource storage mode, then
        // we need to signal Metal to synchronize the buffer data.
        //
        // Ref: https://developer.apple.com/documentation/metal/synchronizing-a-managed-resource-in-macos?language=objc
        if self.opts.resource_options.storage_mode == api::StorageMode::Managed {
            self.buffer.didModifyRange(NSRange::new(0, req_bytes));
        }

        Ok(total_len)
    }

    /// The underlying `MTLBuffer` handle, borrowed for a RenderPass step
    /// (ghostty reads the `buffer.buffer` field directly; §B).
    pub fn raw(&self) -> &ProtocolObject<dyn MTLBuffer> {
        &self.buffer
    }
}
