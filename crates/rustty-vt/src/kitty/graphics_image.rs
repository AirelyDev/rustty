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
// const LimitedAllocator = @import("../../datastruct/main.zig").LimitedAllocator;
use rustty_datastruct::limited_allocator::LimitedAllocator;
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
    InsufficientData,
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

    /// This is non-null when this load is an animation frame
    /// transmission (a=f) rather than a new image. On completion the
    /// data is composed into the target image's animation instead of
    /// being stored as an image.
    pub frame: Option<FrameContext>,

    /// Quiet is the quiet settings for the initial load command. This is
    /// used if q isn't set on subsequent chunks.
    pub quiet: command::Quiet,

    /// Response identifiers from the initial load command. Subsequent chunks
    /// omit these, so completion responses must use the saved values.
    pub response: command::Response,

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

/// PORT NOTE: nested `pub const FrameContext = struct { ... }` inside
/// `LoadingImage`; hoisted to module level (the `SharedMemoryRange`
/// precedent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameContext {
    /// The frame parameters from the initial a=f command. Chunked
    /// continuations only contribute payload bytes; all parameters
    /// come from the command that started the load, matching the
    /// protocol's requirement that chunks repeat a=f.
    pub cmd: command::AnimationFrameLoading,

    /// The generation of the target image when the load began.
    /// A different generation at completion means the image was
    /// replaced or evicted mid-transmission and the frame must be
    /// discarded rather than composed onto the wrong image.
    pub image_generation: u64,
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

/// PORT NOTE: the reference nests `const SharedMemoryRange = struct { start:
/// usize, end: usize };` inside `LoadingImage`; hoisted to module level because
/// Rust has no nested type namespacing on structs.
struct SharedMemoryRange {
    start: usize,
    end: usize,
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

        // Validated here rather than while parsing so the response can
        // carry the image id, matching Kitty's initialize_load_data.
        if t.format_unknown {
            return Err(Error::UnsupportedFormat);
        }
        let mut result: LoadingImage = LoadingImage {
            image: Image {
                id: t.image_id,
                number: t.image_number,
                width: t.width,
                height: t.height,
                compression: t.compression,
                format: t.format,
                // .metadata = .{ .transient = t.usage.transient },
                metadata: Metadata {
                    transient: t.usage.transient,
                    ..Metadata::default()
                },
                ..Image::default()
            },

            data: Vec::new(),
            display: cmd.display(),
            frame: None,
            quiet: cmd.quiet,
            // .response = .{
            //     .id = t.image_id,
            //     .image_number = t.image_number,
            //     .placement_id = t.placement_id,
            // },
            response: command::Response {
                id: t.image_id,
                image_number: t.image_number,
                placement_id: t.placement_id,
                ..Default::default()
            },
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
                // POSIX paths cannot contain internal nulls.
                // log.warn("invalid image path: BadPathName", .{});
                return Err(Error::InvalidData);
            }
        }

        // Depending on the medium, load the data from the path.
        match t.medium {
            command::Medium::Direct => unreachable!(), // handled above
            command::Medium::File => result.readFile(command::Medium::File, &t, &cmd.data)?,
            command::Medium::TemporaryFile => {
                result.readFile(command::Medium::TemporaryFile, &t, &cmd.data)?
            }
            command::Medium::SharedMemory => result.readSharedMemory(&t, &cmd.data)?,
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
            // break :stat std.math.cast(usize, stat.size) orelse
            //     return error.InvalidData;
            match usize::try_from(st.st_size) {
                Ok(v) => v,
                Err(_) => return Err(Error::InvalidData),
            }
        };

        // Get the memory range we'll read. Validate it to make sure
        // it doesn't overflow.
        let range = self.sharedMemoryRange(t, stat_size)?;

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

        assert(self.data.is_empty());
        self.data.extend_from_slice(&map[range.start..range.end]);
        Ok(())
    }

    /// Returns the byte range to copy from a shared memory object.
    // PORT NOTE: the reference return type is the inline error set
    // `error{ InvalidData, DimensionsTooLarge }!SharedMemoryRange`; both members
    // exist on the module `Error` enum, so it is used directly.
    fn sharedMemoryRange(
        &self,
        t: &command::Transmission,
        stat_size: usize,
    ) -> Result<SharedMemoryRange, Error> {
        let expected_size: Option<usize> = match self.image.format {
            // PNG dimensions come from the decoded data.
            command::Format::Png => None,

            // Validate before multiplying because protocol dimensions are
            // u32 values and may otherwise overflow in safe builds.
            command::Format::Gray
            | command::Format::GrayAlpha
            | command::Format::Rgb
            | command::Format::Rgba => {
                if self.image.width > MAX_DIMENSION || self.image.height > MAX_DIMENSION {
                    return Err(Error::DimensionsTooLarge);
                }

                let bpp: usize = command::Transmission::format_bpp(self.image.format) as usize;
                Some(self.image.width as usize * self.image.height as usize * bpp)
            }
        };

        // Get our start offset and validate its within the range of
        // the statted data.
        // const start = std.math.cast(usize, t.offset) orelse
        //     return error.InvalidData;
        let start = match usize::try_from(t.offset) {
            Ok(v) => v,
            Err(_) => return Err(Error::InvalidData),
        };
        if start > stat_size {
            return Err(Error::InvalidData);
        }

        // Validate that our length is within the stat range too.
        let available = stat_size - start;
        let data_size: usize = if t.size > 0 {
            // std.math.cast(usize, t.size) orelse return error.InvalidData
            match usize::try_from(t.size) {
                Ok(v) => v,
                Err(_) => return Err(Error::InvalidData),
            }
        } else if self.image.compression == command::Compression::None && expected_size.is_some() {
            expected_size.unwrap()
        } else {
            available
        };
        if data_size > MAX_SIZE || data_size > available {
            return Err(Error::InvalidData);
        }

        // data_size <= available guarantees this addition cannot overflow.
        Ok(SharedMemoryRange {
            start,
            end: start + data_size,
        })
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

        // We'll populate a delete path if this is a temporary file.
        // PORT NOTE: the reference's single `defer { file.close(io);
        // if (delete_path) |p| ... deleteFile(p) ... }` runs close-then-delete
        // on every exit path. Rust drop order is reverse declaration order, so
        // the delete guard is declared BEFORE `file`: at scope exit `file`
        // drops first (close), then the guard best-effort deletes.
        struct DeletePathGuard {
            // var delete_path: ?[]const u8 = null;
            delete_path: Option<Vec<u8>>,
        }
        impl Drop for DeletePathGuard {
            fn drop(&mut self) {
                if let Some(p) = &self.delete_path {
                    if unlink(p).is_err() {
                        // log.warn("failed to delete temporary file: {}", .{err});
                    }
                }
            }
        }
        let mut delete_guard = DeletePathGuard { delete_path: None };

        // Open our file right away before we do validation. This avoids
        // TOCTOU issues.
        let mut file = match open_file(path) {
            Ok(f) => f,
            Err(_err) => {
                // log.warn("failed to open image file: {}", .{err});
                return Err(Error::InvalidData);
            }
        };
        // defer file.close(); — handled by `File` Drop (see guard note above).

        // Derive the path from the open handle so the file we validate is the
        // exact file we read. Resolving a path before opening it would allow a
        // cooperating process to swap a symlink or directory entry in between.
        let abs_path = match Self::validatedFilePath(&file) {
            Ok(p) => p,
            Err(_err) => {
                // log.warn("failed to validate image file path: {}", .{err});
                return Err(Error::InvalidData);
            }
        };

        // Temporary file logic
        if medium == command::Medium::TemporaryFile {
            // assert(self.temporary_directory != null);
            assert(self.temporary_directory.is_some());
            // PORT NOTE: the borrow is taken before the call so the
            // `&self.temporary_directory` read and the `&[u8]` argument do not
            // overlap a `&mut self` — `isPathInTempDir` is an associated
            // function, so no `self` borrow is live inside it.
            let dir = self.temporary_directory.as_deref().unwrap();
            if !Self::isPathInTempDir(dir, &abs_path) {
                return Err(Error::TemporaryFileNotInTempDir);
            }
            if index_of(&abs_path, b"tty-graphics-protocol").is_none() {
                return Err(Error::TemporaryFileNotNamedCorrectly);
            }
            // delete_path = abs_path;
            delete_guard.delete_path = Some(abs_path);
        }

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

        // PORT NOTE: the reference reads through a 4096-byte buffered reader.
        // We read with the same semantics into a `Vec<u8>`.
        // Read the file
        let mut managed: Vec<u8> = Vec::new();
        if t.size > 0 {
            // S is the exact number of bytes to read, not a maximum:
            // https://sw.kovidgoyal.net/kitty/graphics-protocol/#local-client
            let size: usize = match usize::try_from(t.size) {
                Ok(s) => s,
                Err(_) => return Err(Error::InvalidData),
            };
            if size > MAX_SIZE {
                return Err(Error::InvalidData);
            }

            // Read exact size
            // PORT NOTE: `reader.readAlloc(alloc, size)` reads EXACTLY `size`
            // bytes or fails (a short read is `error.EndOfStream`); every
            // failure other than OOM maps to `error.InvalidData`. `read_exact`
            // reproduces that contract (Vec growth aborts on OOM, which is the
            // Rust-global OOM policy).
            use std::io::Read;
            managed.resize(size, 0);
            if file.read_exact(&mut managed).is_err() {
                // log.warn("failed to read image file: {}", .{err});
                return Err(Error::InvalidData);
            }
        } else {
            use std::io::Read;
            // `.limited(max_size)`: read at most `max_size` bytes.
            let mut limited = (&mut file).take(MAX_SIZE as u64);
            if limited.read_to_end(&mut managed).is_err() {
                // log.warn("failed to read image file: {?}", .{buf_reader.err});
                return Err(Error::InvalidData);
            }
        }

        // Set our data
        assert(self.data.is_empty());
        self.data = managed;
        Ok(())
    }

    /// Returns the canonical path of an open file after applying the file
    /// transmission blocklist.
    // PORT NOTE: the reference is `fn validatedFilePath(io, file, buf) ![]const
    // u8` where `file.realPath` derives the canonical path from the open
    // HANDLE. Rust std has no handle-based realpath, so `realpath_of_handle`
    // below reproduces it per platform (macOS `fcntl(F_GETPATH)`, Linux
    // `/proc/self/fd/N` readlink) — never by re-resolving the submitted path,
    // which would reintroduce the TOCTOU this exists to close. Any handle-path
    // failure maps to `InvalidData` (the caller logs then maps every error to
    // `InvalidData` anyway).
    fn validatedFilePath(file: &std::fs::File) -> Result<Vec<u8>, Error> {
        // const path = buf[0..try file.realPath(io, buf)];
        let path = match realpath_of_handle(file) {
            Ok(p) => p,
            Err(_) => return Err(Error::InvalidData),
        };

        // This is logic copied directly from Kitty, mostly. This is really
        // rough but it will catch obvious bad actors.
        if starts_with(&path, b"/proc/")
            || starts_with(&path, b"/sys/")
            || (starts_with(&path, b"/dev/") && !starts_with(&path, b"/dev/shm/"))
        {
            return Err(Error::InvalidData);
        }

        Ok(path)
    }

    /// Returns true if path appears to be in a temporary directory.
    /// Copies logic from Kitty.
    // PORT NOTE (oracle `e8525c0fd`(iii), `b988efcfe`): `dir` is now a
    // PARAMETER. The previous body called `temp_dir.allocTmpDir()`, which read
    // `TMPDIR`/`TMP` from the process environment at validation time; that
    // lookup is DELETED. See the `temp_dir` module note above.
    fn isPathInTempDir(dir: &[u8], path: &[u8]) -> bool {
        if isPathInDir(b"/tmp", path) {
            return true;
        }
        if isPathInDir(b"/dev/shm", path) {
            return true;
        }
        if isPathInDir(dir, path) {
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
        if isPathInDir(&real_dir, path) {
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

        // Data length must be what we expect.
        let bpp = command::Transmission::format_bpp(self.image.format) as usize;
        let expected_len = self.image.width as usize * self.image.height as usize * bpp;
        let actual_len = self.data.len();
        if self.frame.is_some() {
            // Kitty allows animation frames to exceed their expected length
            // and just truncates it. Not sure if thats expected but lets
            // allow it too.
            if actual_len < expected_len {
                // std.log.warn(
                //     "insufficient frame data image id={} expected_len={} actual_len={}",
                //     .{ img.id, expected_len, actual_len },
                // );
                return Err(Error::InsufficientData);
            }
            // self.data.items.len = expected_len;
            self.data.truncate(expected_len);
        } else if actual_len != expected_len {
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
        // PORT NOTE: `result.data = .{ .complete = try self.data.toOwnedSlice(alloc) };`
        // then `self.image = .{}`. We move our data into the result image and
        // reset `self.image` to its default so a later drop frees nothing twice.
        let mut result = std::mem::take(&mut self.image);
        result.data = Data::Complete(std::mem::take(&mut self.data));
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

        // var limited: LimitedAllocator = .init(alloc, max_size);
        // const decode_alloc = limited.allocator();
        //
        // PORT NOTE: the reference's `alloc` (the caller's allocator) is the
        // Rust global allocator here, wrapped by the same
        // `datastruct/limited_allocator` the reference wraps. The decoder
        // performs every allocation through the limited allocator, so an
        // oversized decode is rejected BEFORE the memory is requested from the
        // child.
        let limited = LimitedAllocator::init(sys::GlobalRawAllocator, MAX_SIZE);
        let decode_alloc = limited.allocator();
        let result = match decode_png_fn(decode_alloc, &self.data) {
            Ok(r) => r,
            Err(err) => match err {
                sys::DecodeError::InvalidData => return Err(Error::InvalidData),
                // error.OutOfMemory => if (limited.limit_exceeded)
                //     return error.InvalidData
                // else
                //     return error.OutOfMemory,
                sys::DecodeError::OutOfMemory => {
                    return Err(if limited.limit_exceeded.get() {
                        Error::InvalidData
                    } else {
                        Error::OutOfMemory
                    })
                }
            },
        };
        // defer decode_alloc.free(result.data); — the decoder's
        // limited-allocator buffer was already freed inside the seam after
        // copying into the global-allocator `Vec` carried by `result` (see
        // `sys::decode_png` PORT NOTE).

        if result.data.len() > MAX_SIZE {
            // log.warn("png image too large size={} max_size={}", .{ result.data.len, max_size });
            return Err(Error::InvalidData);
        }

        // Replace our data
        // PORT NOTE: the reference re-allocates `self.data` from the parent
        // allocator and `appendSlice`s the decode-allocator bytes in (then the
        // deferred free releases the decode buffer). The seam already produced
        // `result.data` as a parent-(global-)allocator `Vec` — that copy IS the
        // `appendSlice` copy (see `sys::decode_png` PORT NOTE) — so it is moved
        // in here rather than copied a second time.
        self.data = result.data;

        // Store updated image dimensions
        self.image.width = result.width;
        self.image.height = result.height;
        self.image.format = command::Format::Rgba;
        Ok(())
    }
}

/// Image represents a single image whose metadata is fully known.
///
/// Complete image data is always fully decoded raw pixels: loading inflates
/// any zlib-compressed payload and decodes PNG into RGBA before an image is
/// completed, so `compression` is always `None` and `format` is never `Png`
/// for a stored image. Pending image data reserves the exact decoded byte
/// length that will be attached later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub id: u32,
    pub number: u32,
    pub width: u32,
    pub height: u32,
    pub format: command::Format,
    pub compression: command::Compression,
    pub data: Data,
    pub metadata: Metadata,

    /// Unique, monotonically increasing stamp assigned each time an
    /// image is added to (or replaced in) an ImageStorage. A changed
    /// generation for a given image ID means the image contents may
    /// have changed, even if the dimensions and byte length are the
    /// same (e.g. a retransmission of the same ID). Stamps order by
    /// transmission time. Zero means "never stored".
    ///
    /// For animated images this also changes whenever the frame that
    /// should be displayed changes (advance, edit, or delete of the
    /// current frame), since consumers key texture caches off it.
    //
    // PORT NOTE: replaces the pre-`bdc0b6c19` `transmit_time: std.time.Instant`.
    pub generation: u64,

    /// Animation state, non-null once any animation command (a=f,
    /// a=a) has attached animation state to this image. Owned by the
    /// image; replaced/retransmitted images drop it, which implements
    /// the protocol's "retransmission resets the animation" rule.
    ///
    /// This is only ever attached to images stored in an ImageStorage
    /// and must only be mutated through the storage's own pointer
    /// (Image values are copied around freely; copies share this
    /// pointer and never own it).
    //
    // PORT NOTE: the reference is `?*animation.Animation` — a raw heap
    // pointer shared by Image value-copies and freed once by the stored
    // image's `deinit`. Rustty's `Image` clones already deep-copy the pixel
    // `data`, so this is `Option<Box<Animation>>` under the same sanctioned
    // shape: `Drop` frees it at every removal site and clones deep-copy —
    // observably identical for every read (see graphics_animation.rs).
    pub animation: Option<Box<crate::kitty::graphics_animation::Animation>>,
}

/// PORT NOTE: the reference is an anonymous `metadata: packed struct(u32) {
/// transient: bool, implicit_id: bool, placement_count: u30 } = .{}` field on
/// `Image`. Named here (Rust has no anonymous structs) and carried as a plain
/// struct: it is only ever read/written field-wise, never bitcast, so the
/// packing is unobservable. `placement_count: u30` rides a `u32` carrier
/// bounded by [`Metadata::PLACEMENT_COUNT_MAX`] (`maxInt(u30)`), enforced at
/// the single increment site (`ImageStorage::add_placement`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Metadata {
    /// The image's transient usage hint, used to prioritize eviction.
    pub transient: bool,

    /// Set this if the image was loaded without an ID or number. Such
    /// images must not receive responses. Kitty gives these client ID
    /// 0 (unaddressable); our storage keys everything by one public
    /// u32 ID, so they get an ID from the upper half of the range
    /// that is guaranteed unused at assignment time, but a client
    /// that explicitly transmits that ID later can still replace
    /// them.
    pub implicit_id: bool,

    /// Number of placements referencing this image.
    pub placement_count: u32,
}

impl Metadata {
    /// `std.math.maxInt(u30)` — the reference field is a `u30`.
    pub const PLACEMENT_COUNT_MAX: u32 = (1 << 30) - 1;
}

/// `pub const Data = union(enum) { complete, pending }` — the payload state of
/// a stored image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Data {
    /// Owned, decoded image bytes. The empty default is not allocated.
    Complete(Vec<u8>),

    /// Expected decoded byte length for a payload that has not arrived.
    Pending(usize),
}

impl Data {
    /// Bytes reserved against the storage limit.
    // PORT NOTE: clippy wants an `is_empty` companion for `len`; the reference
    // has none and `len` here means "reserved bytes", not container length.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        match self {
            Data::Complete(data) => data.len(),
            Data::Pending(expected_len) => *expected_len,
        }
    }

    /// Returns decoded bytes when the payload is complete.
    pub fn bytes(&self) -> Option<&[u8]> {
        match self {
            Data::Complete(data) => Some(data),
            Data::Pending(_) => None,
        }
    }

    pub fn is_pending(&self) -> bool {
        matches!(self, Data::Pending(_))
    }

    // PORT NOTE: `deinit` frees the complete bytes if non-empty; Rust drops the
    // owning `Vec<u8>` automatically.
    // pub fn deinit(self: *Data, alloc: Allocator) void {
    //     switch (self.*) {
    //         .complete => |data| if (data.len > 0) alloc.free(data),
    //         .pending => {},
    //     }
    // }
}

impl Default for Data {
    // data: Data = .{ .complete = "" },
    fn default() -> Self {
        Data::Complete(Vec::new())
    }
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
            data: Data::Complete(Vec::new()),
            metadata: Metadata::default(),
            generation: 0,
            animation: None,
        }
    }
}

/// Borrowed view of an image's displayable [`Data`], returned by
/// [`Image::render_data`].
//
// PORT NOTE: the reference `renderData` returns the `Data` union BY VALUE —
// in Zig a cheap slice share. Rustty's `Data` owns its bytes, so returning it
// would deep-copy the pixels; this borrowed mirror carries the same two arms
// and the same `bytes()` accessor the consumers use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderData<'a> {
    Complete(&'a [u8]),
    Pending(usize),
}

impl<'a> RenderData<'a> {
    /// Returns decoded bytes when the payload is complete (the borrowed
    /// mirror of [`Data::bytes`]).
    pub fn bytes(&self) -> Option<&'a [u8]> {
        match self {
            RenderData::Complete(data) => Some(data),
            RenderData::Pending(_) => None,
        }
    }

    /// Bytes reserved against the storage limit (the borrowed mirror of
    /// [`Data::len`]; the reference's `renderData()` returns `Data`, whose
    /// `len()` the C API's `.data_len` arm reads — 73903f76a).
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        match self {
            RenderData::Complete(data) => data.len(),
            RenderData::Pending(expected_len) => *expected_len,
        }
    }
}

impl Image {
    // PORT NOTE: `deinit` forwards to `self.data.deinit(alloc)` and frees
    // `self.animation` (`anim.deinit(alloc); alloc.destroy(anim)`); Rust drops
    // the owning `Data` and `Option<Box<Animation>>` automatically.
    // pub fn deinit(self: *Image, alloc: Allocator) void {
    //     self.data.deinit(alloc);
    //     if (self.animation) |anim| {
    //         anim.deinit(alloc);
    //         alloc.destroy(anim);
    //         self.animation = null;
    //     }
    // }

    /// The pixel data that should be displayed for this image. For an
    /// animated image this is the current animation frame; otherwise
    /// (and for the root frame) it is the image's own data.
    pub fn render_data(&self) -> RenderData<'_> {
        if let Some(anim) = &self.animation {
            if anim.current_index > 0 {
                return RenderData::Complete(&anim.frames[(anim.current_index - 1) as usize].data);
            }
        }

        match &self.data {
            Data::Complete(data) => RenderData::Complete(data),
            Data::Pending(expected_len) => RenderData::Pending(*expected_len),
        }
    }

    /// The pixel data of the given 1-based animation frame number, or
    /// null if the frame doesn't exist. Frame 1 (the root frame)
    /// always exists as long as the image data is complete, even for
    /// images without animation state.
    ///
    /// The returned slice is owned by the image (or its animation)
    /// and remains valid until the image or frame is mutated.
    pub fn frame_data(&self, number: u32) -> Option<&[u8]> {
        match number {
            0 => None,
            1 => self.data.bytes(),
            _ => {
                let anim = self.animation.as_ref()?;
                // Minus 2 because frame is 1-based and frame 1 is the
                // image base data, so the animation frames start at frame 2.
                let idx = (number - 2) as usize;
                if idx >= anim.frames.len() {
                    return None;
                }
                Some(&anim.frames[idx].data)
            }
        }
    }

    /// Total bytes of pixel data reserved against the storage limit
    /// for this image: the base data plus any animation frames.
    pub fn storage_size(&self) -> usize {
        let mut total: usize = self.data.len();
        if let Some(anim) = &self.animation {
            total += anim.frame_bytes();
        }
        total
    }

    /// Mostly for logging
    pub fn withoutData(&self) -> Image {
        let mut copy = self.clone();
        // if (copy.data == .complete) copy.data = .{ .complete = "" };
        if matches!(copy.data, Data::Complete(_)) {
            copy.data = Data::Complete(Vec::new());
        }
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

impl Rect {
    /// Returns true if the grid cell is inside this rectangle. Pin.isBetween
    /// compares page order, so its interior rows intentionally do not constrain
    /// x. Check the column independently and use isBetween only for the row.
    pub fn contains(&self, cell: PageList::Pin) -> bool {
        if cell.x < self.top_left.x || cell.x > self.bottom_right.x {
            return false;
        }

        let mut row = cell;
        row.x = self.top_left.x;
        row.is_between(self.top_left, self.bottom_right)
    }
}

/// Returns true if `path` is `dir` or is contained within it, requiring a
/// path-separator boundary so similarly prefixed directories do not match.
fn isPathInDir(dir: &[u8], path: &[u8]) -> bool {
    if dir.is_empty() || !starts_with(path, dir) {
        return false;
    }
    if path.len() == dir.len() || is_sep(dir[dir.len() - 1]) {
        return true;
    }
    is_sep(path[dir.len()])
}

/// `std.fs.path.isSep(byte)` — platform path-separator test.
#[cfg(not(windows))]
fn is_sep(byte: u8) -> bool {
    byte == b'/'
}

#[cfg(windows)]
fn is_sep(byte: u8) -> bool {
    byte == b'/' || byte == b'\\'
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

/// `std.Io.File.realPath(io, buf)` — the canonical path of an OPEN handle.
///
/// PORT NOTE (TOCTOU, `ec04900ab`): this must be derived from the handle, never
/// by re-resolving the submitted path string (which a cooperating process could
/// swap in between). macOS: `fcntl(F_GETPATH)`; Linux/Android: readlink of
/// `/proc/self/fd/N` — the same mechanisms Zig's `File.realPath` uses.
#[cfg(target_os = "macos")]
fn realpath_of_handle(file: &std::fs::File) -> std::io::Result<Vec<u8>> {
    use std::os::fd::AsRawFd;
    let mut buf = [0u8; libc::PATH_MAX as usize];
    // SAFETY(ffi-boundary/graphics_image): `F_GETPATH` writes a NUL-terminated
    // path of at most PATH_MAX bytes into `buf`, which is exactly PATH_MAX
    // bytes; `file` keeps the fd open for the duration of the call.
    let rc = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, buf.as_mut_ptr()) };
    if rc == -1 {
        return Err(std::io::Error::last_os_error());
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    Ok(buf[..len].to_vec())
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn realpath_of_handle(file: &std::fs::File) -> std::io::Result<Vec<u8>> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStringExt;
    let link = format!("/proc/self/fd/{}", file.as_raw_fd());
    let target = std::fs::read_link(link)?;
    Ok(target.into_os_string().into_vec())
}

// PORT NOTE: remaining targets (windows, other unixes, wasm). Zig's
// `File.realPath` covers windows via the NT object path; the rustty windows
// tier has no handle-path binding yet, so the kitty file/temporary-file medium
// fails closed (`InvalidData` via `validatedFilePath`) there.
// TODO(port): windows GetFinalPathNameByHandleW once a windows tier lands.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
fn realpath_of_handle(_file: &std::fs::File) -> std::io::Result<Vec<u8>> {
    Err(std::io::Error::from(std::io::ErrorKind::Unsupported))
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

    // test "temporary file path must be inside directory"
    #[test]
    fn temporary_file_path_must_be_inside_directory() {
        assert!(isPathInDir(
            b"/tmp",
            b"/tmp/tty-graphics-protocol-image.data"
        ));
        assert!(isPathInDir(
            b"/tmp/",
            b"/tmp/tty-graphics-protocol-image.data"
        ));
        assert!(isPathInDir(b"/tmp", b"/tmp"));

        assert!(!isPathInDir(b"", b"/tmp/tty-graphics-protocol-image.data"));
        assert!(!isPathInDir(
            b"/tmp",
            b"/tmpX/tty-graphics-protocol-image.data"
        ));
        assert!(!isPathInDir(
            b"/dev/shm",
            b"/dev/shm-evil/tty-graphics-protocol-image.data"
        ));
        assert!(!isPathInDir(
            b"/custom/tmp",
            b"/custom/tmp-suffix/tty-graphics-protocol-image.data"
        ));
    }

    /// Test helper: the reference's `const loading: LoadingImage = .{ .image =
    /// .{...}, .quiet = .no, .temporary_directory = null };` literal (field
    /// defaults fill the rest).
    fn shm_loading(width: u32, height: u32, format: command::Format) -> LoadingImage {
        LoadingImage {
            image: Image {
                width,
                height,
                format,
                ..Image::default()
            },
            data: Vec::new(),
            display: None,
            frame: None,
            quiet: command::Quiet::No,
            response: command::Response::default(),
            temporary_directory: None,
        }
    }

    // test "shared memory range with offset and size"
    #[test]
    fn shared_memory_range_with_offset_and_size() {
        let loading = shm_loading(1, 1, command::Format::Rgb);

        let explicit = loading
            .sharedMemoryRange(
                &command::Transmission {
                    offset: 2,
                    size: 3,
                    ..Default::default()
                },
                5,
            )
            .unwrap();
        assert_eq!(2usize, explicit.start);
        assert_eq!(5usize, explicit.end);

        let implicit = loading
            .sharedMemoryRange(
                &command::Transmission {
                    offset: 2,
                    ..Default::default()
                },
                5,
            )
            .unwrap();
        assert_eq!(2usize, implicit.start);
        assert_eq!(5usize, implicit.end);
    }

    // test "shared memory range rejects out of bounds offset"
    #[test]
    fn shared_memory_range_rejects_out_of_bounds_offset() {
        let loading = shm_loading(1, 1, command::Format::Rgb);

        assert_eq!(
            Err(Error::InvalidData),
            loading
                .sharedMemoryRange(
                    &command::Transmission {
                        offset: 4,
                        ..Default::default()
                    },
                    3,
                )
                .map(|_| ())
        );
    }

    // test "shared memory range validates dimensions before multiplication"
    #[test]
    fn shared_memory_range_validates_dimensions_before_multiplication() {
        let loading = shm_loading(u32::MAX, u32::MAX, command::Format::Rgba);

        assert_eq!(
            Err(Error::DimensionsTooLarge),
            loading
                .sharedMemoryRange(&command::Transmission::default(), 1)
                .map(|_| ())
        );
    }

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

    // test "image load: temporary file outside directory prefix is rejected"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_temporary_file_outside_directory_prefix_is_rejected() {
        // PORT NOTE (Linux parity): the reference test's `testing.tmpDir` lives
        // under the CWD-relative `.zig-cache/tmp`, NOT the system temp dir —
        // and that placement is LOAD-BEARING here: `isPathInTempDir` accepts
        // anything under `/tmp`, and `rustty_os::temp_dir::TempDir` resolves
        // there on Linux, which would satisfy the `/tmp` arm and mask the
        // directory-prefix rejection this test exists to prove (green-on-macOS
        // only because its TempDir lands under `/var/folders`). Mirror the
        // reference with a target-dir scratch on both platforms.
        let base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!("kitty-prefix-scratch-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(&base).unwrap();
        // try tmp_dir.dir.createDir(io, "temp", .default_dir);
        // try tmp_dir.dir.createDir(io, "temp-suffix", .default_dir);
        std::fs::create_dir(base.join("temp")).unwrap();
        std::fs::create_dir(base.join("temp-suffix")).unwrap();

        let filename = "tty-graphics-protocol-image.data";
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        std::fs::write(base.join("temp-suffix").join(filename), data).unwrap();

        let trusted_path = std::fs::canonicalize(base.join("temp")).unwrap();
        let outside_path = std::fs::canonicalize(base.join("temp-suffix").join(filename)).unwrap();

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
            data: path_to_bytes(&outside_path),
        };
        assert_eq!(
            Err(Error::TemporaryFileNotInTempDir),
            LoadingImage::init(
                &cmd,
                &Limits::all_with_temp_dir(&path_to_bytes(&trusted_path))
            )
            .map(|_| ())
        );

        // Rejection must happen before temporary-file cleanup is armed.
        // (`tmp_dir.dir.access(...)` on the reference tmpDir; `metadata` is the
        // std::fs existence probe over the scratch dir.)
        std::fs::metadata(&outside_path).unwrap();
        std::fs::remove_dir_all(&base).unwrap();
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

    // test "image load: regular file size reads exactly requested bytes"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_regular_file_size_reads_exactly_requested_bytes() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        tmp_dir
            .dir()
            .writeFile("image.data", &[1, 2, 3, 4, 5, 6])
            .unwrap();

        let path = tmp_dir.dir().realpath("image.data").unwrap();
        let path_bytes = path_to_bytes(&path);

        struct Case {
            offset: u32,
            expected: [u8; 3],
        }
        let cases = [
            Case {
                offset: 0,
                expected: [1, 2, 3],
            },
            Case {
                offset: 3,
                expected: [4, 5, 6],
            },
        ];
        for case in cases.iter() {
            let cmd: command::Command = command::Command {
                control: command::Control::Transmit(command::Transmission {
                    format: command::Format::Rgb,
                    medium: command::Medium::File,
                    width: 1,
                    height: 1,
                    size: 3,
                    offset: case.offset,
                    image_id: 31,
                    ..Default::default()
                }),
                quiet: command::Quiet::No,
                data: path_bytes.clone(),
            };

            let mut loading = LoadingImage::init(
                &cmd,
                &Limits {
                    file: true,
                    temporary_file: TemporaryFileLimit::Disabled,
                    shared_memory: false,
                },
            )
            .unwrap();
            let img = loading.complete().unwrap();

            let Data::Complete(bytes) = &img.data else {
                unreachable!()
            };
            assert_eq!(case.expected.as_slice(), bytes.as_slice());
        }
        tmp_dir.deinit();
    }

    // test "image load: regular file size rejects short data"
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_regular_file_size_rejects_short_data() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        tmp_dir.dir().writeFile("image.data", &[1, 2]).unwrap();

        let path = tmp_dir.dir().realpath("image.data").unwrap();
        let path_bytes = path_to_bytes(&path);

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::File,
                width: 1,
                height: 1,
                size: 3,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: path_bytes,
        };

        assert_eq!(
            Err(Error::InvalidData),
            LoadingImage::init(
                &cmd,
                &Limits {
                    file: true,
                    temporary_file: TemporaryFileLimit::Disabled,
                    shared_memory: false,
                },
            )
            .map(|_| ())
        );
        tmp_dir.deinit();
    }

    // test "image load: blocklist applies to opened file after symlink swap"
    // (the reference starts with `if (builtin.os.tag == .windows) return
    // error.SkipZigTest;` — mapped to the cfg gate)
    #[cfg(not(windows))]
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_blocklist_applies_to_opened_file_after_symlink_swap() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let base = tmp_dir.dir().realpath(".").unwrap();
        tmp_dir.dir().writeFile("safe.data", b"safe").unwrap();
        // try tmp_dir.dir.symLink(io, "/dev/null", "image.data", .{});
        std::os::unix::fs::symlink("/dev/null", base.join("image.data")).unwrap();

        // Pin the blocked file, then simulate the cooperating process replacing
        // the path with a safe target before validation.
        let blocked_file = std::fs::File::open(base.join("image.data")).unwrap();
        // try tmp_dir.dir.symLinkAtomic(io, "safe.data", "image.data", .{});
        // PORT NOTE: `symLinkAtomic` = symlink to a temp name + rename over.
        std::os::unix::fs::symlink("safe.data", base.join("image.data.atomic")).unwrap();
        std::fs::rename(base.join("image.data.atomic"), base.join("image.data")).unwrap();

        assert_eq!(
            Err(Error::InvalidData),
            LoadingImage::validatedFilePath(&blocked_file).map(|_| ())
        );

        // The pathname now resolves to the safe replacement, demonstrating that
        // the rejection above came from the already-open file handle.
        let safe_file = std::fs::File::open(base.join("image.data")).unwrap();
        LoadingImage::validatedFilePath(&safe_file).unwrap();
        // (blocked_file/safe_file close on drop; the reference defers both.)
        drop(blocked_file);
        tmp_dir.deinit();
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_png_not_compressed_regular_file() {
        // PORT NOTE: `if (sys.decode_png == null) return error.SkipZigTest;`. The
        // decoder seam is not installed in this VT leaf crate (the wuffs binding
        // rides a later FFI crate), so this test is skipped, matching the
        // reference's skip when no decoder is present.
        let _png_lock = sys::decode_png_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
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

    // test "image load: png rejects oversized decoder allocation"
    //
    // PORT NOTE (restructured, same assertions): the reference threads the
    // testing `FailingAllocator` in as the allocator `complete(alloc)` hands to
    // `decodePng`, then asserts `error.InvalidData` AND
    // `!failing.has_induced_failure` (the limiter rejected the request BEFORE
    // it reached the child). rustty's `complete()` carries no allocator
    // parameter (the Rust global backs it), so the two assertions are split:
    // part A drives the SAME oversized decoder through a
    // `LimitedAllocator(FailingAllocator)` directly, proving the request is
    // rejected before the child; part B installs the decoder in the global slot
    // and asserts `loading.complete()` maps it to `InvalidData`.
    #[test]
    fn image_load_png_rejects_oversized_decoder_allocation() {
        use rustty_datastruct::limited_allocator::{Alignment, RawAllocator, RawSlice};

        // const oversized_decoder = struct { fn decode(alloc, _) { ... } }.decode;
        fn oversized_decoder(
            alloc: &dyn RawAllocator,
            _data: &[u8],
        ) -> core::result::Result<sys::Image, sys::DecodeError> {
            // const data = try alloc.alloc(u8, max_size + 1);
            let data = match alloc.alloc_bytes(MAX_SIZE + 1) {
                Ok(d) => d,
                Err(_) => return Err(sys::DecodeError::OutOfMemory),
            };
            // return .{ .width = 1, .height = 1, .data = data };
            // PORT NOTE: unreachable in practice (the limiter rejects first);
            // sys::Image owns a Vec, so copy-and-free the raw allocation.
            // SAFETY: `data` is a live `MAX_SIZE + 1`-byte allocation from `alloc`.
            let bytes = unsafe { std::slice::from_raw_parts(data.ptr, data.len) }.to_vec();
            alloc.free_bytes(data);
            Ok(sys::Image {
                width: 1,
                height: 1,
                data: bytes,
            })
        }

        // PORT NOTE: the rustty analog of `std.testing.FailingAllocator`
        // restricted to what this test observes (see the datastruct
        // limited_allocator tests for the canonical copy).
        struct FailingAllocator {
            fail_index: usize,
            index: std::cell::Cell<usize>,
            has_induced_failure: std::cell::Cell<bool>,
        }
        impl RawAllocator for FailingAllocator {
            fn raw_alloc(&self, len: usize, alignment: Alignment, ra: usize) -> *mut u8 {
                if self.index.get() >= self.fail_index {
                    self.has_induced_failure.set(true);
                    return std::ptr::null_mut();
                }
                self.index.set(self.index.get() + 1);
                sys::GlobalRawAllocator.raw_alloc(len, alignment, ra)
            }
            fn raw_resize(
                &self,
                memory: RawSlice,
                alignment: Alignment,
                new_len: usize,
                ra: usize,
            ) -> bool {
                sys::GlobalRawAllocator.raw_resize(memory, alignment, new_len, ra)
            }
            fn raw_remap(
                &self,
                memory: RawSlice,
                alignment: Alignment,
                new_len: usize,
                ra: usize,
            ) -> *mut u8 {
                sys::GlobalRawAllocator.raw_remap(memory, alignment, new_len, ra)
            }
            fn raw_free(&self, memory: RawSlice, alignment: Alignment, ra: usize) {
                sys::GlobalRawAllocator.raw_free(memory, alignment, ra);
            }
        }

        // Part A: fail any allocation which reaches the underlying allocator.
        // The size limiter should reject the decoder's request before it gets
        // that far.
        // var failing = testing.FailingAllocator.init(testing.allocator, .{ .fail_index = 0 });
        let failing = FailingAllocator {
            fail_index: 0,
            index: std::cell::Cell::new(0),
            has_induced_failure: std::cell::Cell::new(false),
        };
        let limited = LimitedAllocator::init(failing, MAX_SIZE);
        assert_eq!(
            Err(sys::DecodeError::OutOfMemory),
            oversized_decoder(limited.allocator(), b"").map(|_| ())
        );
        assert!(limited.limit_exceeded.get());
        // try testing.expect(!failing.has_induced_failure);
        assert!(!limited.child.has_induced_failure.get());

        // Part B: with the decoder installed, `complete()` maps the
        // limit-rejected decode to `InvalidData`.
        let _png_lock = sys::decode_png_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // const original_decode_png = sys.decode_png;
        // defer sys.decode_png = original_decode_png;
        // sys.decode_png = &oversized_decoder;
        let original_decode_png = *sys::decode_png.read().unwrap();
        *sys::decode_png.write().unwrap() = Some(oversized_decoder);

        let mut loading = shm_loading(0, 0, command::Format::Png);
        assert_eq!(Err(Error::InvalidData), loading.complete().map(|_| ()));

        *sys::decode_png.write().unwrap() = original_decode_png;
    }

    // test "image load: png rejects oversized Wuffs image before allocation"
    #[test]
    fn image_load_png_rejects_oversized_wuffs_image_before_allocation() {
        let _png_lock = sys::decode_png_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // if (sys.decode_png == null) return error.SkipZigTest;
        if sys::decode_png.read().unwrap().is_none() {
            return;
        }

        // Turn the small test PNG into a 32768x32767 image. Its decoded RGBA
        // size is just under Wuffs' 4 GiB package limit but over Kitty's 400 MiB
        // limit, which previously allowed the large allocation to happen first.
        let mut data = include_bytes!("testdata/image-png-none-50x76-2147483647-raw.data").to_vec();
        // std.mem.writeInt(u32, data[16..20], 32768, .big);
        data[16..20].copy_from_slice(&32768u32.to_be_bytes());
        // std.mem.writeInt(u32, data[20..24], 32767, .big);
        data[20..24].copy_from_slice(&32767u32.to_be_bytes());
        // std.mem.writeInt(u32, data[29..33], std.hash.Crc32.hash(data[12..29]), .big);
        let crc = crc32(&data[12..29]);
        data[29..33].copy_from_slice(&crc.to_be_bytes());

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Png,
                medium: command::Medium::Direct,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data,
        };
        let mut loading = LoadingImage::init(&cmd, &Limits::DIRECT).unwrap();

        assert_eq!(Err(Error::InvalidData), loading.complete().map(|_| ()));
    }

    /// PORT NOTE: `std.hash.Crc32.hash` (the IEEE polynomial, as PNG chunk CRCs
    /// use) — a small bitwise implementation, test-only.
    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
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
