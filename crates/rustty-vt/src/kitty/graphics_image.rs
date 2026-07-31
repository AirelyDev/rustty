//! rustty-vt kitty/graphics_image
//!
//! PORTS: ghostty src/terminal/kitty/graphics_image.zig
//!
//! PORT STATUS: done
//!
//! PORT NOTE: the reference imports are
//!   const std = @import("std");
//!   const builtin = @import("builtin");
//!   const assert = @import("../../quirks.zig").inlineAssert;
//!   const Allocator = std.mem.Allocator;
//!   const ArenaAllocator = std.heap.ArenaAllocator;
//!   const posix = std.posix;
//!   const fastmem = @import("../../fastmem.zig");
//!   const command = @import("graphics_command.zig");
//!   const PageList = @import("../PageList.zig");
//!   const sys = @import("../sys.zig");
//!   const temp_dir = struct {
//!       const TempDir = @import("../../os/TempDir.zig");
//!       const allocTmpDir = @import("../../os/file.zig").allocTmpDir;
//!       const freeTmpDir = @import("../../os/file.zig").freeTmpDir;
//!   };
//! The allocator is implicit in Rust (the global allocator backs `Vec`), so the
//! `Allocator`/`ArenaAllocator` parameters are dropped. `posix` operations are
//! reproduced over `std`/`libc`; `fastmem`/`command`/`PageList`/`sys` map to the
//! ported sibling modules, and the temp-file helpers come from `rustty_os`.
//!
//! PORT NOTE: DEFLATE decompression. The reference's `decompressZlib` uses
//! `std.compress.flate.Decompress` (zlib mode). We bind the SAME C library the
//! reference's std wraps (vendored zlib 1.3.1 via `zlib-sys`, R3) and call its
//! streaming inflate. PNG decoding remains the optional `sys.decode_png` hook
//! (seam), faithfully reproducing the `decode_png == null` save-effort path.

#![allow(non_snake_case)]

use crate::kitty::graphics_command as command;
use crate::pagelist as PageList;
use crate::sys;
use rustty_util::fastmem;
use rustty_util::quirks::inlineAssert as assert;

// PORT NOTE: temp-file helpers. The reference's `temp_dir` is a small namespace
// over `os/TempDir.zig` + `os/file.zig`; here they come from `rustty_os`.
mod temp_dir {
    #[cfg(test)]
    pub use rustty_os::tempdir::TempDir;

    // PORT NOTE (P3-W11 T1, oracle `e8525c0fd`(iii)): the reference's
    // `allocTmpDir`/`freeTmpDir` imports are **DELETED** by `e8525c0fd`. The
    // temp directory `isPathInTempDir` validates against is now a PARAMETER
    // threaded down from `Limits.temporary_file.enabled.directory`, which
    // `termio` fills with `global.tmpDirPath()`.
    //
    // This is deliberate security hardening: the previous body consulted the
    // process environment (`TMPDIR`/`TMP`) at validation time, so which
    // directories a remote-controlled kitty `f=` transmission could read from
    // depended on mutable process state. It no longer does.
    //
    // It also removes the reason this crate — the libghostty-vt artifact, which
    // upstream FORBIDS from importing `global.zig` — needed a self-contained
    // copy of the discovery logic (see the `P3-W7 T3` note this replaces).
}

// const log = std.log.scoped(.kitty_gfx);

/// Maximum width or height of an image. Taken directly from Kitty.
const MAX_DIMENSION: u32 = 10000;

/// Maximum size in bytes, taken from Kitty.
const MAX_SIZE: usize = 400 * 1024 * 1024; // 400MB

/// PORT NOTE: the reference returns the Zig error set `Image.Error` (plus
/// `Allocator.Error`/`InvalidData` propagated from helpers). This enum models
/// that error set so every `return error.X` maps assertion-for-assertion. The
/// `OutOfMemory` variant carries the implicit `Allocator.Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidData,
    DecompressionFailed,
    DimensionsRequired,
    DimensionsTooLarge,
    FilePathTooLong,
    TemporaryFileNotInTempDir,
    TemporaryFileNotNamedCorrectly,
    UnsupportedFormat,
    UnsupportedMedium,
    UnsupportedDepth,
    OutOfMemory,
}

/// An image that is still being loaded. The image should be initialized
/// using init on the first chunk and then addData for each subsequent
/// chunk. Once all chunks have been added, complete should be called
/// to finalize the image.
pub struct LoadingImage {
    /// The in-progress image. The first chunk must have all the metadata
    /// so this comes from that initially.
    pub image: Image,

    /// The data that is being built up.
    // PORT NOTE: `std.ArrayListUnmanaged(u8) = .{}` → an owning `Vec<u8>`.
    pub data: Vec<u8>,

    /// This is non-null when a transmit and display command is given
    /// so that we display the image after it is fully loaded.
    pub display: Option<command::Display>,

    /// Quiet is the quiet settings for the initial load command. This is
    /// used if q isn't set on subsequent chunks.
    pub quiet: command::Quiet,

    /// The temporary directory for file transmission (null means that
    /// temporary directory transmission is disabled).
    // PORT NOTE (oracle `e8525c0fd`): `temporary_directory: ?[]const u8`. Zig
    // stores a BORROW of `Limits.temporary_file.enabled.directory`, which in
    // turn borrows terminal-owned storage. Rust owns the bytes here instead:
    // tying `LoadingImage`'s lifetime to the `Limits` it was built from would
    // infect `ImageStorage`, `Screen` and `Terminal` with a lifetime parameter
    // for no observable gain. Byte-slice rule: `Vec<u8>`, never `String`.
    pub temporary_directory: Option<Vec<u8>>,
}

/// PORT NOTE (oracle `e8525c0fd`): the reference's anonymous
/// `union(enum) { enabled: struct { directory: []const u8 }, disabled: void }`
/// inside `Limits`. Named here because Rust has no anonymous enums.
/// Byte-slice rule: the directory is `Vec<u8>`, never `String`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemporaryFileLimit {
    Enabled {
        /// The directory to expect temporary files in.
        directory: Vec<u8>,
    },
    Disabled,
}

/// The limits of the Kitty Graphics protocol we should allow.
///
/// This can be used to restrict the type of images and other
/// parameters for resource or security reasons. Note that depending
/// on how libghostty is compiled, some of these may be fully unsupported
/// and ignored (e.g. "file" on wasm32-freestanding).
// PORT NOTE: the reference was a `packed struct` of three `bool`s;
// `e8525c0fd` widened `temporary_file` into a tagged union and dropped
// `packed`. The struct is only read field-wise, never bitcast, so a plain
// struct preserves all observable behavior. It is no longer `Copy`, because
// the enabled arm owns its directory bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub file: bool,
    pub temporary_file: TemporaryFileLimit,
    pub shared_memory: bool,
}

impl Limits {
    /// Enables all filesystem-related image transmission mediums. `path`
    /// is the temporary directory to expect files in when files are
    /// transmitted using said medium.
    // PORT NOTE (oracle `e8525c0fd`): replaces `pub const all: Limits`, which
    // is DELETED upstream — there is no longer a way to enable the temporary
    // file medium without naming the directory it is restricted to.
    pub fn all_with_temp_dir(path: &[u8]) -> Limits {
        Limits {
            file: true,
            temporary_file: TemporaryFileLimit::Enabled {
                directory: path.to_vec(),
            },
            shared_memory: true,
        }
    }

    pub const DIRECT: Limits = Limits {
        file: false,
        temporary_file: TemporaryFileLimit::Disabled,
        shared_memory: false,
    };
}

impl LoadingImage {
    /// Initialize a chunked immage from the first image transmission.
    /// If this is a multi-chunk image, this should only be the FIRST
    /// chunk.
    // PORT NOTE (oracle `e8525c0fd`): `limits: Limits` is taken by REFERENCE
    // here. `Limits` stopped being `Copy` when `temporary_file` gained its
    // owned directory bytes; upstream's by-value pass is free because the Zig
    // union holds a borrowed slice.
    pub fn init(cmd: &command::Command, limits: &Limits) -> Result<LoadingImage, Error> {
        // Build our initial image from the properties sent via the control.
        // These can be overwritten by the data loading process. For example,
        // PNG loading sets the width/height from the data.
        let t = cmd.transmission().unwrap();
        let mut result: LoadingImage = LoadingImage {
            image: Image {
                id: t.image_id,
                number: t.image_number,
                width: t.width,
                height: t.height,
                compression: t.compression,
                format: t.format,
                usage: t.usage,
                ..Image::default()
            },

            data: Vec::new(),
            display: cmd.display(),
            quiet: cmd.quiet,
            // .temporary_directory = switch (limits.temporary_file) {
            //     .enabled => |d| d.directory,
            //     .disabled => null,
            // },
            temporary_directory: match &limits.temporary_file {
                TemporaryFileLimit::Enabled { directory } => Some(directory.clone()),
                TemporaryFileLimit::Disabled => None,
            },
        };

        // Special case for the direct medium, we just add the chunk directly.
        if t.medium == command::Medium::Direct {
            result.addData(&cmd.data)?;
            return Ok(result);
        }

        // Verify our capabilities and limits allow this.
        {
            // Special case if we don't support decoding PNGs and the format
            // is a PNG we can save a lot of memory/effort buffering the
            // data but failing up front.
            if t.format == command::Format::Png && sys::decode_png.read().unwrap().is_none() {
                return Err(Error::UnsupportedMedium);
            }

            // Verify the medium is allowed
            match t.medium {
                command::Medium::Direct => unreachable!(),
                command::Medium::File => {
                    if !limits.file {
                        return Err(Error::UnsupportedMedium);
                    }
                }
                // .temporary_file => if (limits.temporary_file == .disabled)
                //     return error.UnsupportedMedium,
                command::Medium::TemporaryFile => {
                    if limits.temporary_file == TemporaryFileLimit::Disabled {
                        return Err(Error::UnsupportedMedium);
                    }
                }
                command::Medium::SharedMemory => {
                    if !limits.shared_memory {
                        return Err(Error::UnsupportedMedium);
                    }
                }
            }
        }

        // Otherwise, the payload data is guaranteed to be a path.

        // PORT NOTE: `if (comptime builtin.os.tag != .windows)`.
        #[cfg(not(windows))]
        {
            if cmd.data.contains(&0) {
                // posix.realpath *asserts* that the path does not have
                // internal nulls instead of erroring.
                // log.warn("failed to get absolute path: BadPathName", .{});
                return Err(Error::InvalidData);
            }
        }

        // PORT NOTE: `var abs_buf: [std.fs.max_path_bytes]u8 = undefined;` and
        // `posix.realpath(cmd.data, &abs_buf)`. In Rust `realpath` returns an
        // owning path; we resolve below per medium.
        let path: Vec<u8> = match t.medium {
            command::Medium::Direct => unreachable!(), // handled above
            command::Medium::File | command::Medium::TemporaryFile => match realpath(&cmd.data) {
                Ok(p) => p,
                Err(_err) => {
                    // log.warn("failed to get absolute path: {}", .{err});
                    return Err(Error::InvalidData);
                }
            },
            command::Medium::SharedMemory => cmd.data.clone(),
        };

        // Depending on the medium, load the data from the path.
        match t.medium {
            command::Medium::Direct => unreachable!(), // handled above
            command::Medium::File => result.readFile(command::Medium::File, &t, &path)?,
            command::Medium::TemporaryFile => {
                result.readFile(command::Medium::TemporaryFile, &t, &path)?
            }
            command::Medium::SharedMemory => result.readSharedMemory(&t, &path)?,
        }

        Ok(result)
    }

    /// Reads the data from a shared memory segment.
    fn readSharedMemory(&mut self, t: &command::Transmission, path: &[u8]) -> Result<(), Error> {
        // android does not support POSIX shared memory.
        // windows is currently unsupported, does it support shm?
        // PORT NOTE: `if (comptime builtin.abi.isAndroid() or
        // builtin.target.os.tag == .windows) return error.UnsupportedMedium;`
        // and `if (comptime !builtin.link_libc) return error.UnsupportedMedium;`.
        // We gate the same way: anything that is not a libc POSIX target (here,
        // non-windows / non-android) supports shm; otherwise unsupported.
        //
        // libc is required for shm_open. PORT NOTE (wasm32): freestanding wasm has
        // no POSIX shared memory (the reference's `!builtin.link_libc` →
        // `UnsupportedMedium` gate), so wasm takes the unsupported arm too.
        #[cfg(any(windows, target_os = "android", target_arch = "wasm32"))]
        {
            let _ = (t, path);
            return Err(Error::UnsupportedMedium);
        }
        #[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
        {
            self.readSharedMemoryPosix(t, path)
        }
    }

    #[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
    fn readSharedMemoryPosix(
        &mut self,
        t: &command::Transmission,
        path: &[u8],
    ) -> Result<(), Error> {
        use std::os::raw::c_int;

        // Since we're only supporting posix then max_path_bytes should
        // be enough to stack allocate the path.
        // PORT NOTE: `std.fmt.bufPrintZ(&buf, "{s}", .{path})` — a NUL-terminated
        // copy of `path`. We build a `CString`; a path with an interior NUL
        // (which `bufPrintZ` would not produce) maps to `error.InvalidData`.
        let pathz = match std::ffi::CString::new(path) {
            Ok(c) => c,
            Err(_) => return Err(Error::InvalidData),
        };

        // PORT NOTE: `std.c.shm_open(pathz, @bitCast(O{ .ACCMODE = .RDONLY }), 0)`.
        // SAFETY: pathz is a valid NUL-terminated C string.
        let fd = unsafe { libc::shm_open(pathz.as_ptr(), libc::O_RDONLY, 0) };
        if fd < 0 {
            // log.warn("unable to open shared memory {s}: {}", .{ path, err });
            return Err(Error::InvalidData);
        }

        // defer _ = std.c.close(fd);
        // defer _ = std.c.shm_unlink(pathz);
        struct ShmGuard {
            fd: c_int,
            pathz: std::ffi::CString,
        }
        impl Drop for ShmGuard {
            fn drop(&mut self) {
                // SAFETY: fd was returned by shm_open; pathz is valid.
                unsafe {
                    libc::close(self.fd);
                    libc::shm_unlink(self.pathz.as_ptr());
                }
            }
        }
        let guard = ShmGuard { fd, pathz };

        // The size from stat on may be larger than our expected size because
        // shared memory has to be a multiple of the page size.
        let stat_size: usize = {
            // SAFETY: guard.fd is an open file descriptor.
            let mut st: libc::stat = unsafe { std::mem::zeroed() };
            let rc = unsafe { libc::fstat(guard.fd, &mut st) };
            if rc != 0 {
                // log.warn("unable to fstat shared memory {s}: {}", .{ path, err });
                return Err(Error::InvalidData);
            }
            if st.st_size <= 0 {
                return Err(Error::InvalidData);
            }
            st.st_size as usize
        };

        let expected_size: usize = match self.image.format {
            // Png we decode the full data size because later decoding will
            // get the proper dimensions and assert validity.
            command::Format::Png => stat_size,

            // For these formats we have a size we must have.
            command::Format::Gray
            | command::Format::GrayAlpha
            | command::Format::Rgb
            | command::Format::Rgba => {
                let bpp = command::Transmission::format_bpp(self.image.format) as usize;
                self.image.width as usize * self.image.height as usize * bpp
            }
        };

        // Our stat size must be at least the expected size otherwise
        // the shared memory data is invalid.
        if stat_size < expected_size {
            // log.warn(
            //     "shared memory size too small expected={} actual={}",
            //     .{ expected_size, stat_size },
            // );
            return Err(Error::InvalidData);
        }

        // PORT NOTE: `std.posix.mmap(null, stat_size, PROT.READ,
        // MAP{ .TYPE = .SHARED }, fd, 0)`.
        // SAFETY: guard.fd is open and readable; stat_size > 0.
        let map_ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                stat_size, // mmap always uses the stat size
                libc::PROT_READ,
                libc::MAP_SHARED,
                guard.fd,
                0,
            )
        };
        if map_ptr == libc::MAP_FAILED {
            // log.warn("unable to mmap shared memory {s}: {}", .{ path, err });
            return Err(Error::InvalidData);
        }
        // defer std.posix.munmap(map);
        struct MmapGuard {
            ptr: *mut libc::c_void,
            len: usize,
        }
        impl Drop for MmapGuard {
            fn drop(&mut self) {
                // SAFETY: ptr/len came from the successful mmap above.
                unsafe {
                    libc::munmap(self.ptr, self.len);
                }
            }
        }
        let _mmap_guard = MmapGuard {
            ptr: map_ptr,
            len: stat_size,
        };
        // SAFETY: the mapping is stat_size bytes, readable.
        let map: &[u8] = unsafe { std::slice::from_raw_parts(map_ptr as *const u8, stat_size) };

        // Our end size always uses the expected size so we cut off the
        // padding for mmap alignment.
        let start: usize = t.offset as usize;
        let end: usize = if t.size > 0 {
            std::cmp::min(t.offset as usize + t.size as usize, expected_size)
        } else {
            expected_size
        };

        assert(self.data.is_empty());
        self.data.extend_from_slice(&map[start..end]);
        Ok(())
    }

    /// Reads the data from a temporary file and returns it. This allocates
    /// and does not free any of the data, so the caller must free it.
    ///
    /// This will also delete the temporary file if it is in a safe location.
    fn readFile(
        &mut self,
        medium: command::Medium,
        t: &command::Transmission,
        path: &[u8],
    ) -> Result<(), Error> {
        // PORT NOTE: the reference's `comptime medium` switch with a
        // `@compileError` on anything but file/temporary_file becomes a runtime
        // assert; callers only ever pass those two variants.
        match medium {
            command::Medium::File | command::Medium::TemporaryFile => {}
            _ => unreachable!("readFile only supports file and temporary_file"),
        }

        // Verify file seems "safe". This is logic copied directly from Kitty,
        // mostly. This is really rough but it will catch obvious bad actors.
        if starts_with(path, b"/proc/")
            || starts_with(path, b"/sys/")
            || (starts_with(path, b"/dev/") && !starts_with(path, b"/dev/shm/"))
        {
            return Err(Error::InvalidData);
        }

        // Temporary file logic
        if medium == command::Medium::TemporaryFile {
            // assert(self.temporary_directory != null);
            assert(self.temporary_directory.is_some());
            // PORT NOTE: the borrow is taken before the call so the
            // `&self.temporary_directory` read and the `&[u8]` argument do not
            // overlap a `&mut self` — `isPathInTempDir` is an associated
            // function, so no `self` borrow is live inside it.
            let dir = self.temporary_directory.as_deref().unwrap();
            if !Self::isPathInTempDir(dir, path) {
                return Err(Error::TemporaryFileNotInTempDir);
            }
            if index_of(path, b"tty-graphics-protocol").is_none() {
                return Err(Error::TemporaryFileNotNamedCorrectly);
            }
        }

        // PORT NOTE: `defer if (medium == .temporary_file) posix.unlink(path)`.
        // The guard runs on every exit path after this point (mirroring the
        // Zig `defer`), best-effort deleting the temporary file.
        struct UnlinkGuard<'a> {
            armed: bool,
            path: &'a [u8],
        }
        impl Drop for UnlinkGuard<'_> {
            fn drop(&mut self) {
                if self.armed && unlink(self.path).is_err() {
                    // log.warn("failed to delete temporary file: {}", .{err});
                }
            }
        }
        let _unlink_guard = UnlinkGuard {
            armed: medium == command::Medium::TemporaryFile,
            path,
        };

        let mut file = match open_file(path) {
            Ok(f) => f,
            Err(_err) => {
                // log.warn("failed to open temporary file: {}", .{err});
                return Err(Error::InvalidData);
            }
        };
        // defer file.close(); — handled by `File` Drop.

        // File must be a regular file
        match file.metadata() {
            Ok(md) => {
                if !md.is_file() {
                    // log.warn("file is not a regular file kind={}", .{stat.kind});
                    return Err(Error::InvalidData);
                }
            }
            Err(_err) => {
                // log.warn("failed to stat file: {}", .{err});
                return Err(Error::InvalidData);
            }
        }

        if t.offset > 0 {
            use std::io::Seek;
            if file
                .seek(std::io::SeekFrom::Start(t.offset as u64))
                .is_err()
            {
                // log.warn("failed to seek to offset {}: {}", .{ t.offset, err });
                return Err(Error::InvalidData);
            }
        }

        // PORT NOTE: the reference reads through a 4096-byte buffered reader,
        // appending the remaining bytes up to `size` into a managed ArrayList.
        // We read with the same `size` cap into a `Vec<u8>`.
        // Read the file
        let size: usize = if t.size > 0 {
            std::cmp::min(t.size as usize, MAX_SIZE)
        } else {
            MAX_SIZE
        };

        let mut managed: Vec<u8> = Vec::new();
        {
            use std::io::Read;
            // `.limited(size)`: read at most `size` bytes.
            let mut limited = (&mut file).take(size as u64);
            if limited.read_to_end(&mut managed).is_err() {
                // log.warn("failed to read temporary file: {?}", .{buf_reader.err});
                return Err(Error::InvalidData);
            }
        }

        // Set our data
        assert(self.data.is_empty());
        self.data = managed;
        Ok(())
    }

    /// Returns true if path appears to be in a temporary directory.
    /// Copies logic from Kitty.
    // PORT NOTE (oracle `e8525c0fd`(iii), `b988efcfe`): `dir` is now a
    // PARAMETER. The previous body called `temp_dir.allocTmpDir()`, which read
    // `TMPDIR`/`TMP` from the process environment at validation time; that
    // lookup is DELETED. See the `temp_dir` module note above.
    fn isPathInTempDir(dir: &[u8], path: &[u8]) -> bool {
        if starts_with(path, b"/tmp") {
            return true;
        }
        if starts_with(path, b"/dev/shm") {
            return true;
        }
        if starts_with(path, dir) {
            return true;
        }

        // The temporary dir is sometimes a symlink. On macOS for
        // example /tmp is /private/var/...
        // PORT NOTE: `std.Io.Dir.cwd().realPathFile(io, dir, &buf) catch return
        // false`. `e8525c0fd` turned the old `if (posix.realpath(...)) |…| …
        // else |_| {}` fall-through into an early `return false`; with nothing
        // after the check in either version the two are equivalent, and we
        // follow the new shape.
        let Ok(real_dir) = realpath(dir) else {
            return false;
        };
        if starts_with(path, &real_dir) {
            return true;
        }

        false
    }

    // PORT NOTE: `deinit` frees `self.image` and `self.data`; Rust drops both
    // (the image's `data` Vec and `self.data`) automatically. `destroy`
    // additionally `alloc.destroy(self)` — that is the caller dropping the box.
    // pub fn deinit(self: *LoadingImage, alloc: Allocator) void {
    //     self.image.deinit(alloc);
    //     self.data.deinit(alloc);
    // }
    // pub fn destroy(self: *LoadingImage, alloc: Allocator) void {
    //     self.deinit(alloc);
    //     alloc.destroy(self);
    // }

    /// Adds a chunk of data to the image. Use this if the image
    /// is coming in chunks (the "m" parameter in the protocol).
    pub fn addData(&mut self, data: &[u8]) -> Result<(), Error> {
        // If no data, skip
        if data.is_empty() {
            return Ok(());
        }

        // If our data would get too big, return an error
        if self.data.len() + data.len() > MAX_SIZE {
            // log.warn("image data too large max_size={}", .{max_size});
            return Err(Error::InvalidData);
        }

        // Ensure we have enough room to add the data
        // to the end of the ArrayList before doing so.
        self.data.reserve(data.len());

        // PORT NOTE: `fastmem.copy(u8, self.data.items[start_i..], data)` after
        // bumping the length. We grow the Vec then memcpy into the tail.
        let start_i = self.data.len();
        self.data.resize(start_i + data.len(), 0);
        fastmem::copy(&mut self.data[start_i..], data);
        Ok(())
    }

    /// Complete the chunked image, returning a completed image.
    pub fn complete(&mut self) -> Result<Image, Error> {
        // Decompress the data if it is compressed.
        self.decompress()?;

        // Decode the png if we have to
        if self.image.format == command::Format::Png {
            self.decodePng()?;
        }

        // Validate our dimensions.
        if self.image.width == 0 || self.image.height == 0 {
            return Err(Error::DimensionsRequired);
        }
        if self.image.width > MAX_DIMENSION || self.image.height > MAX_DIMENSION {
            return Err(Error::DimensionsTooLarge);
        }

        // Data length must be what we expect
        let bpp = command::Transmission::format_bpp(self.image.format) as usize;
        let expected_len = self.image.width as usize * self.image.height as usize * bpp;
        let actual_len = self.data.len();
        if actual_len != expected_len {
            // std.log.warn(
            //     "unexpected length image id={} width={} height={} bpp={} expected_len={} actual_len={}",
            //     .{ img.id, img.width, img.height, bpp, expected_len, actual_len },
            // );
            return Err(Error::InvalidData);
        }

        // (deleted — bdc0b6c19: `self.image.transmit_time = Instant::now()` and
        // its `InternalError` failure path are gone; the stamp is now assigned
        // by `ImageStorage::add_image`.)

        // Everything looks good, copy the image data over.
        // PORT NOTE: `result.data = try self.data.toOwnedSlice(alloc);` then
        // `self.image = .{}`. We move our data into the result image and reset
        // `self.image` to its default so a later drop frees nothing twice.
        let mut result = std::mem::take(&mut self.image);
        result.data = std::mem::take(&mut self.data);
        Ok(result)
    }

    // PORT NOTE: `debugDump` is a Debug-only helper for capturing test data; it
    // is `@compileError` outside Debug builds and is not exercised by any test.
    // It is reproduced as a comment to preserve the reference verbatim.
    //
    // Debug function to write the data to a file. This is useful for
    // capturing some test data for unit tests.
    // pub fn debugDump(self: LoadingImage) !void {
    //     if (comptime builtin.mode != .Debug) @compileError("debugDump in non-debug");
    //     var buf: [1024]u8 = undefined;
    //     const filename = try std.fmt.bufPrint(
    //         &buf,
    //         "image-{s}-{s}-{d}x{d}-{}.data",
    //         .{
    //             @tagName(self.image.format),
    //             @tagName(self.image.compression),
    //             self.image.width,
    //             self.image.height,
    //             self.image.id,
    //         },
    //     );
    //     const cwd = std.fs.cwd();
    //     const f = try cwd.createFile(filename, .{});
    //     defer f.close();
    //     const writer = f.writer();
    //     try writer.writeAll(self.data.items);
    // }

    /// Decompress the data in-place.
    fn decompress(&mut self) -> Result<(), Error> {
        match self.image.compression {
            command::Compression::None => Ok(()),
            command::Compression::ZlibDeflate => self.decompressZlib(),
        }
    }

    // WASM DIVERGENCE (`wasm32`): the kitty zlib-deflate image path binds the
    // vendored C `zlib-sys` library, which cannot link on freestanding
    // `wasm32-unknown-unknown` (no libc). The kitty PNG/zlib image-decode path is
    // not needed for a terminal-in-browser text demo, so on wasm this method is a
    // faithful `Unsupported` stub returning `DecompressionFailed` — mirroring how
    // the reference gates the wasm/lib image path (PNG decode is likewise a null
    // `sys::decode_png` seam). The `Compression::ZlibDeflate` match arm and its
    // signature are unchanged; only the body diverges. Native/other targets keep
    // the real C-zlib inflate below. (`zlib-sys` is `cfg(not(wasm32))` in Cargo.)
    #[cfg(target_arch = "wasm32")]
    fn decompressZlib(&mut self) -> Result<(), Error> {
        Err(Error::DecompressionFailed)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn decompressZlib(&mut self) -> Result<(), Error> {
        // PORT NOTE: the reference opens a `std.compress.flate.Decompress` in
        // zlib mode over `self.data.items`, then appends the remaining
        // decompressed bytes (limited to `max_size`) into a managed list. We
        // call the vendored zlib's streaming inflate (same C library) with the
        // same `max_size` cap; any failure maps to `DecompressionFailed`.
        //
        // Write it to an array list
        let list = match zlib_sys::inflate_zlib(&self.data, MAX_SIZE) {
            Ok(list) => list,
            Err(_) => {
                // log.warn("failed to read decompressed data: {?}", .{stream.err});
                return Err(Error::DecompressionFailed);
            }
        };

        // Empty our current data list, take ownership over managed array list
        self.data = list;

        // Make sure we note that our image is no longer compressed
        self.image.compression = command::Compression::None;
        Ok(())
    }

    /// Decode the data as PNG. This will also updated the image dimensions.
    fn decodePng(&mut self) -> Result<(), Error> {
        assert(self.image.format == command::Format::Png);

        // PORT NOTE: `const decode_png_fn = sys.decode_png orelse return
        // error.UnsupportedFormat;`. The swappable slot is a `RwLock<Option<..>>`.
        let decode_png_fn = match *sys::decode_png.read().unwrap() {
            Some(f) => f,
            None => return Err(Error::UnsupportedFormat),
        };
        let result = match decode_png_fn(&self.data) {
            Ok(r) => r,
            Err(err) => match err {
                sys::DecodeError::InvalidData => return Err(Error::InvalidData),
                sys::DecodeError::OutOfMemory => return Err(Error::OutOfMemory),
            },
        };
        // defer alloc.free(result.data); — `result` (and its Vec) drops at end.

        if result.data.len() > MAX_SIZE {
            // log.warn("png image too large size={} max_size={}", .{ result.data.len, max_size });
            return Err(Error::InvalidData);
        }

        // Replace our data
        self.data = Vec::new();
        self.data.reserve(result.data.len());
        self.data
            .extend_from_slice(&result.data[0..result.data.len()]);

        // Store updated image dimensions
        self.image.width = result.width;
        self.image.height = result.height;
        self.image.format = command::Format::Rgba;
        Ok(())
    }
}

/// Image represents a single fully loaded image.
///
/// The image data is always fully decoded raw pixels: loading inflates any
/// zlib-compressed payload and decodes PNG into RGBA before an image is
/// completed, so `compression` is always `None` and `format` is never `Png` for
/// a stored image, and `data.len()` always equals
/// `width * height * bytes-per-pixel`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub id: u32,
    pub number: u32,
    pub width: u32,
    pub height: u32,
    pub format: command::Format,
    pub compression: command::Compression,
    pub data: Vec<u8>,
    /// Usage hints transmitted with the image (oracle `a65e11cc9`); see
    /// `command::Usage`. `transient` prioritises this image for eviction.
    pub usage: command::Usage,
    /// Unique, monotonically increasing stamp assigned each time an image is
    /// added to (or replaced in) an `ImageStorage`. A changed generation for a
    /// given image ID means the image contents may have changed, even if the
    /// dimensions and byte length are the same (e.g. a retransmission of the
    /// same ID). Stamps order by transmission time. Zero means "never stored".
    //
    // PORT NOTE: replaces the pre-`bdc0b6c19` `transmit_time: std.time.Instant`.
    pub generation: u64,

    /// Set this to true if this image was loaded by a command that
    /// doesn't specify an ID or number, since such commands should
    /// not be responded to, even though we do currently give them
    /// IDs in the public range (which is bad!).
    pub implicit_id: bool,
}

impl Default for Image {
    fn default() -> Self {
        Image {
            id: 0,
            number: 0,
            width: 0,
            height: 0,
            format: command::Format::Rgb,
            compression: command::Compression::None,
            data: Vec::new(),
            usage: command::Usage::DEFAULT,
            generation: 0,
            implicit_id: false,
        }
    }
}

impl Image {
    // PORT NOTE: `deinit` frees `self.data` if non-empty; Rust drops the owning
    // `Vec<u8>` automatically.
    // pub fn deinit(self: *Image, alloc: Allocator) void {
    //     if (self.data.len > 0) alloc.free(self.data);
    // }

    /// Mostly for logging
    pub fn withoutData(&self) -> Image {
        let mut copy = self.clone();
        copy.data = Vec::new();
        copy
    }
}

/// The rect taken up by some image placement, in grid cells. This will
/// be rounded up to the nearest grid cell since we can't place images
/// in partial grid cells.
pub struct Rect {
    pub top_left: PageList::Pin,
    pub bottom_right: PageList::Pin,
}

// ---------------------------------------------------------------------------
// PORT NOTE: support shims for the std/posix facilities used above.
// ---------------------------------------------------------------------------

/// `std.mem.startsWith(u8, haystack, needle)`.
fn starts_with(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && &haystack[0..needle.len()] == needle
}

/// `std.mem.indexOf(u8, haystack, needle)`.
fn index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// `posix.realpath(path, &buf)`: resolve `path` to a canonical absolute path,
/// returning the path bytes. On platforms where paths are not UTF-8 this still
/// round-trips the raw bytes via `OsStr`.
#[cfg(all(not(windows), not(target_arch = "wasm32")))]
fn realpath(path: &[u8]) -> std::io::Result<Vec<u8>> {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let p = std::path::Path::new(std::ffi::OsStr::from_bytes(path));
    let canon = std::fs::canonicalize(p)?;
    Ok(canon.into_os_string().into_vec())
}

// PORT NOTE (wasm32-wasi): wasi exposes the same raw-bytes ↔ OsStr round-trip as
// unix via `std::os::wasi::ffi` (paths are not required to be UTF-8), so the
// helper is identical except for the extension trait's module path.
#[cfg(all(target_arch = "wasm32", target_os = "wasi"))]
fn realpath(path: &[u8]) -> std::io::Result<Vec<u8>> {
    use std::os::wasi::ffi::{OsStrExt, OsStringExt};
    let p = std::path::Path::new(std::ffi::OsStr::from_bytes(path));
    let canon = std::fs::canonicalize(p)?;
    Ok(canon.into_os_string().into_vec())
}

// WASM DIVERGENCE (freestanding `wasm32-unknown-unknown`): there is no
// filesystem and no `std::os::wasi`/`std::os::unix` OsStr byte round-trip. The
// kitty "file"/"tempfile" transmission media are unreachable in a browser
// terminal demo, so these path helpers are `Unsupported` stubs. Native + wasi
// keep the real implementations above.
#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
fn realpath(_path: &[u8]) -> std::io::Result<Vec<u8>> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

#[cfg(windows)]
fn realpath(path: &[u8]) -> std::io::Result<Vec<u8>> {
    let s = std::str::from_utf8(path)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let canon = std::fs::canonicalize(s)?;
    Ok(canon.to_string_lossy().into_owned().into_bytes())
}

/// `std.fs.cwd().openFile(path, .{})`.
#[cfg(all(not(windows), not(target_arch = "wasm32")))]
fn open_file(path: &[u8]) -> std::io::Result<std::fs::File> {
    use std::os::unix::ffi::OsStrExt;
    std::fs::File::open(std::path::Path::new(std::ffi::OsStr::from_bytes(path)))
}

#[cfg(all(target_arch = "wasm32", target_os = "wasi"))]
fn open_file(path: &[u8]) -> std::io::Result<std::fs::File> {
    use std::os::wasi::ffi::OsStrExt;
    std::fs::File::open(std::path::Path::new(std::ffi::OsStr::from_bytes(path)))
}

// WASM DIVERGENCE (freestanding wasm): no filesystem — see `realpath` note.
#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
fn open_file(_path: &[u8]) -> std::io::Result<std::fs::File> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

#[cfg(windows)]
fn open_file(path: &[u8]) -> std::io::Result<std::fs::File> {
    let s = std::str::from_utf8(path)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    std::fs::File::open(s)
}

/// `posix.unlink(path)`.
#[cfg(all(not(windows), not(target_arch = "wasm32")))]
fn unlink(path: &[u8]) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    std::fs::remove_file(std::path::Path::new(std::ffi::OsStr::from_bytes(path)))
}

#[cfg(all(target_arch = "wasm32", target_os = "wasi"))]
fn unlink(path: &[u8]) -> std::io::Result<()> {
    use std::os::wasi::ffi::OsStrExt;
    std::fs::remove_file(std::path::Path::new(std::ffi::OsStr::from_bytes(path)))
}

// WASM DIVERGENCE (freestanding wasm): no filesystem — see `realpath` note.
#[cfg(all(target_arch = "wasm32", not(target_os = "wasi")))]
fn unlink(_path: &[u8]) -> std::io::Result<()> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
}

#[cfg(windows)]
fn unlink(path: &[u8]) -> std::io::Result<()> {
    let s = std::str::from_utf8(path)
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    std::fs::remove_file(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: the reference's tests construct `command.Command` literals with
    // a `.control = .{ .transmit = ... }` and `.data` slice, then exercise
    // LoadingImage. We build the equivalent `Command` values via the ported
    // `command` module. The reference's `alloc.dupe(u8, ...)` (an owned copy the
    // command frees) maps to an owned `Vec<u8>` in `Command.data`.

    // This specifically tests we ALLOW invalid RGB data because Kitty
    // documents that this should work.
    #[test]
    fn image_load_with_invalid_rgb_data() {
        // <ESC>_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA<ESC>\
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                width: 1,
                height: 1,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: b"AAAA".to_vec(),
        };
        let _loading = LoadingImage::init(&cmd, &Limits::DIRECT).unwrap();
    }

    #[test]
    fn image_load_with_image_too_wide() {
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                width: MAX_DIMENSION + 1,
                height: 1,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: b"AAAA".to_vec(),
        };
        let mut loading = LoadingImage::init(&cmd, &Limits::DIRECT).unwrap();
        assert_eq!(Err(Error::DimensionsTooLarge), loading.complete());
    }

    #[test]
    fn image_load_with_image_too_tall() {
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                height: MAX_DIMENSION + 1,
                width: 1,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: b"AAAA".to_vec(),
        };
        let mut loading = LoadingImage::init(&cmd, &Limits::DIRECT).unwrap();
        assert_eq!(Err(Error::DimensionsTooLarge), loading.complete());
    }

    // rustty-only capacity regression (no Zig test{} counterpart) for the DG2/
    // DG4 release-profile fork (PORT_RULES §DG2/DG4; VALIDATION_REPORT §3d
    // "Kitty graphics expected-size multiply"). Oracle behavior, mirrored
    // exactly: graphics_image.zig computes `expected_len = img.width *
    // img.height * bpp` (:394) in u32, where attacker-controlled dimensions
    // would overflow — a ReleaseSafe TRAP, ReleaseFast UB — but that multiply
    // is UNREACHABLE for out-of-range dimensions because the guard at
    // graphics_image.zig:390 returns error.DimensionsTooLarge first
    // (max_dimension = 10000). So for w=0xFFFF_FFFF, h=2, bpp=4 (rgba) the
    // oracle ERRORS — it does not trap — in every build mode, and so must we.
    // The ported multiply at complete() widens to usize (Rust contract: the
    // u32 operands cannot overflow usize after the MAX_DIMENSION guard, max
    // 10000*10000*4); [profile.release] overflow-checks = true restores
    // ReleaseSafe trap-parity should this guard ordering ever regress.
    #[test]
    fn image_load_expected_size_capacity_width_u32_max() {
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgba, // bpp = 4
                width: 0xFFFF_FFFF,
                height: 2,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: b"AAAA".to_vec(),
        };
        let mut loading = LoadingImage::init(&cmd, &Limits::DIRECT).unwrap();
        assert_eq!(Err(Error::DimensionsTooLarge), loading.complete());
    }

    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md): Miri cannot call the vendored zlib C FFI (inflateInit2_) -- foreign function; covered natively + by ASan"
    )]
    #[test]
    fn image_load_rgb_zlib_compressed_direct() {
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::Direct,
                compression: command::Compression::ZlibDeflate,
                height: 96,
                width: 128,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: include_bytes!("testdata/image-rgb-zlib_deflate-128x96-2147483647-raw.data")
                .to_vec(),
        };
        let mut loading = LoadingImage::init(&cmd, &Limits::DIRECT).unwrap();
        let img = loading.complete().unwrap();

        // should be decompressed
        assert!(img.compression == command::Compression::None);
    }

    #[test]
    fn image_load_rgb_not_compressed_direct() {
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::Direct,
                compression: command::Compression::None,
                width: 20,
                height: 15,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data").to_vec(),
        };
        let mut loading = LoadingImage::init(&cmd, &Limits::DIRECT).unwrap();
        let img = loading.complete().unwrap();

        // should be decompressed
        assert!(img.compression == command::Compression::None);
    }

    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md): Miri cannot call the vendored zlib C FFI (inflateInit2_) -- foreign function; covered natively + by ASan"
    )]
    #[test]
    fn image_load_rgb_zlib_compressed_direct_chunked() {
        let data = include_bytes!("testdata/image-rgb-zlib_deflate-128x96-2147483647-raw.data");

        // Setup our initial chunk
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::Direct,
                compression: command::Compression::ZlibDeflate,
                height: 96,
                width: 128,
                image_id: 31,
                more_chunks: true,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: data[0..1024].to_vec(),
        };
        let mut loading = LoadingImage::init(&cmd, &Limits::DIRECT).unwrap();

        // Read our remaining chunks
        let mut pos = 1024;
        loop {
            let end = std::cmp::min(pos + 1024, data.len());
            let size = end - pos;
            loading.addData(&data[pos..end]).unwrap();
            pos = end;
            if size < 1024 {
                break;
            }
        }

        // Complete
        let img = loading.complete().unwrap();
        assert!(img.compression == command::Compression::None);
    }

    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md): Miri cannot call the vendored zlib C FFI (inflateInit2_) -- foreign function; covered natively + by ASan"
    )]
    #[test]
    fn image_load_rgb_zlib_compressed_direct_chunked_with_zero_initial_chunk() {
        let data = include_bytes!("testdata/image-rgb-zlib_deflate-128x96-2147483647-raw.data");

        // Setup our initial chunk
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::Direct,
                compression: command::Compression::ZlibDeflate,
                height: 96,
                width: 128,
                image_id: 31,
                more_chunks: true,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: Vec::new(),
        };
        let mut loading = LoadingImage::init(&cmd, &Limits::DIRECT).unwrap();

        // Read our remaining chunks
        let mut pos = 0;
        loop {
            let end = std::cmp::min(pos + 1024, data.len());
            let size = end - pos;
            loading.addData(&data[pos..end]).unwrap();
            pos = end;
            if size < 1024 {
                break;
            }
        }

        // Complete
        let img = loading.complete().unwrap();
        assert!(img.compression == command::Compression::None);
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_temporary_file_without_correct_path() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        tmp_dir.dir().writeFile("image.data", data).unwrap();

        let path = tmp_dir.dir().realpath("image.data").unwrap();
        let path_bytes = path_to_bytes(&path);

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::TemporaryFile,
                compression: command::Compression::None,
                width: 20,
                height: 15,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: path_bytes,
        };
        assert_eq!(
            Err(Error::TemporaryFileNotNamedCorrectly),
            LoadingImage::init(
                &cmd,
                &Limits::all_with_temp_dir(&tmp_dir_realpath(&tmp_dir))
            )
            .map(|_| ())
        );

        // Temporary file should still be there
        tmp_dir.dir().access(&path).unwrap();
        tmp_dir.deinit();
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_rgb_not_compressed_temporary_file() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        tmp_dir
            .dir()
            .writeFile("tty-graphics-protocol-image.data", data)
            .unwrap();

        let path = tmp_dir
            .dir()
            .realpath("tty-graphics-protocol-image.data")
            .unwrap();
        let path_bytes = path_to_bytes(&path);

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::TemporaryFile,
                compression: command::Compression::None,
                width: 20,
                height: 15,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: path_bytes,
        };
        let mut loading = LoadingImage::init(
            &cmd,
            &Limits::all_with_temp_dir(&tmp_dir_realpath(&tmp_dir)),
        )
        .unwrap();
        let img = loading.complete().unwrap();
        assert!(img.compression == command::Compression::None);

        // Temporary file should be gone
        assert_eq!(
            std::io::ErrorKind::NotFound,
            tmp_dir.dir().access(&path).unwrap_err().kind()
        );
        tmp_dir.deinit();
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_rgb_not_compressed_regular_file() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        tmp_dir.dir().writeFile("image.data", data).unwrap();

        let path = tmp_dir.dir().realpath("image.data").unwrap();
        let path_bytes = path_to_bytes(&path);

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::File,
                compression: command::Compression::None,
                width: 20,
                height: 15,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: path_bytes,
        };
        let mut loading = LoadingImage::init(
            &cmd,
            &Limits::all_with_temp_dir(&tmp_dir_realpath(&tmp_dir)),
        )
        .unwrap();
        let img = loading.complete().unwrap();
        assert!(img.compression == command::Compression::None);
        tmp_dir.dir().access(&path).unwrap();
        tmp_dir.deinit();
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_png_not_compressed_regular_file() {
        // PORT NOTE: `if (sys.decode_png == null) return error.SkipZigTest;`. The
        // decoder seam is not installed in this VT leaf crate (the wuffs binding
        // rides a later FFI crate), so this test is skipped, matching the
        // reference's skip when no decoder is present.
        if sys::decode_png.read().unwrap().is_none() {
            return;
        }

        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let data = include_bytes!("testdata/image-png-none-50x76-2147483647-raw.data");
        tmp_dir
            .dir()
            .writeFile("tty-graphics-protocol-image.data", data)
            .unwrap();

        let path = tmp_dir
            .dir()
            .realpath("tty-graphics-protocol-image.data")
            .unwrap();
        let path_bytes = path_to_bytes(&path);

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Png,
                medium: command::Medium::File,
                compression: command::Compression::None,
                width: 0,
                height: 0,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: path_bytes,
        };
        let mut loading = LoadingImage::init(
            &cmd,
            &Limits::all_with_temp_dir(&tmp_dir_realpath(&tmp_dir)),
        )
        .unwrap();
        let img = loading.complete().unwrap();
        assert!(img.compression == command::Compression::None);
        assert!(img.format == command::Format::Rgba);
        tmp_dir.dir().access(&path).unwrap();
        tmp_dir.deinit();
    }

    #[test]
    fn limits_direct_medium_always_allowed() {
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::Direct,
                width: 1,
                height: 1,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: b"AAAA".to_vec(),
        };

        // Direct medium should work even with the most restrictive limits
        let _loading = LoadingImage::init(&cmd, &Limits::DIRECT).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn limits_file_medium_blocked_by_limits() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        tmp_dir.dir().writeFile("image.data", data).unwrap();

        let path = tmp_dir.dir().realpath("image.data").unwrap();
        let path_bytes = path_to_bytes(&path);

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::File,
                compression: command::Compression::None,
                width: 20,
                height: 15,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: path_bytes,
        };
        assert_eq!(
            Err(Error::UnsupportedMedium),
            LoadingImage::init(&cmd, &Limits::DIRECT).map(|_| ())
        );
        tmp_dir.deinit();
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn limits_file_medium_allowed_by_limits() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        tmp_dir.dir().writeFile("image.data", data).unwrap();

        let path = tmp_dir.dir().realpath("image.data").unwrap();
        let path_bytes = path_to_bytes(&path);

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::File,
                compression: command::Compression::None,
                width: 20,
                height: 15,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: path_bytes,
        };
        let _loading = LoadingImage::init(
            &cmd,
            &Limits {
                file: true,
                temporary_file: TemporaryFileLimit::Disabled,
                shared_memory: false,
            },
        )
        .unwrap();
        tmp_dir.deinit();
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn limits_temporary_file_medium_blocked_by_limits() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        tmp_dir
            .dir()
            .writeFile("tty-graphics-protocol-image.data", data)
            .unwrap();

        let path = tmp_dir
            .dir()
            .realpath("tty-graphics-protocol-image.data")
            .unwrap();
        let path_bytes = path_to_bytes(&path);

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::TemporaryFile,
                compression: command::Compression::None,
                width: 20,
                height: 15,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: path_bytes,
        };
        assert_eq!(
            Err(Error::UnsupportedMedium),
            LoadingImage::init(
                &cmd,
                &Limits {
                    file: true,
                    temporary_file: TemporaryFileLimit::Disabled,
                    shared_memory: true,
                },
            )
            .map(|_| ())
        );

        // File should still exist since we blocked before reading
        tmp_dir
            .dir()
            .access(std::path::Path::new("tty-graphics-protocol-image.data"))
            .unwrap();
        tmp_dir.deinit();
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn limits_temporary_file_medium_allowed_by_limits() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        tmp_dir
            .dir()
            .writeFile("tty-graphics-protocol-image.data", data)
            .unwrap();

        let path = tmp_dir
            .dir()
            .realpath("tty-graphics-protocol-image.data")
            .unwrap();
        let path_bytes = path_to_bytes(&path);

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::TemporaryFile,
                compression: command::Compression::None,
                width: 20,
                height: 15,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: path_bytes,
        };
        let _loading = LoadingImage::init(
            &cmd,
            &Limits {
                file: false,
                temporary_file: TemporaryFileLimit::Enabled {
                    directory: tmp_dir_realpath(&tmp_dir),
                },
                shared_memory: false,
            },
        )
        .unwrap();
        tmp_dir.deinit();
    }

    // Test helper: convert a resolved `PathBuf` to the raw bytes the command's
    // `data` payload carries (the absolute path string).
    #[cfg(not(windows))]
    fn path_to_bytes(p: &std::path::Path) -> Vec<u8> {
        use std::os::unix::ffi::OsStrExt;
        p.as_os_str().as_bytes().to_vec()
    }

    #[cfg(windows)]
    fn path_to_bytes(p: &std::path::Path) -> Vec<u8> {
        p.to_string_lossy().into_owned().into_bytes()
    }

    /// PORT NOTE (oracle `e8525c0fd`): upstream's ported temp-file tests build
    /// their limits with
    /// `.allWithTempDir(dir_path_buf[0..try tmp_dir.dir.realPath(io, &dir_path_buf)])`
    /// — i.e. the canonical path of the test's OWN temp directory, not the
    /// process `TMPDIR`. This is that expression.
    fn tmp_dir_realpath(tmp_dir: &temp_dir::TempDir) -> Vec<u8> {
        path_to_bytes(&tmp_dir.dir().realpath(".").unwrap())
    }
}
