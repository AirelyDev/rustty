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
//! PORT NOTE: DEFLATE decompression. The reference's `decompressZlib` decodes
//! through `wuffs.zlib.decode` (`4b86e3594`; `const wuffs = @import("wuffs");`),
//! ported as `rustty_wuffs::zlib::decode` over the same vendored Wuffs, which
//! the `kitty-graphics` feature links on every target, wasm32 included
//! (DECISIONS B8). PNG decoding remains the optional `sys.decode_png` hook
//! (seam), faithfully reproducing the `decode_png == null` save-effort path.
//!
//! `const kitty_windows = @import("windows.zig");` (`0b5446364`) is
//! `crate::kitty::windows`.

#![allow(non_snake_case)]

use crate::alloc::{ABox, Allocator, Bytes, CopyVecExt};
use crate::kitty::graphics_command as command;
// const kitty_windows = @import("windows.zig");
//
// PORT NOTE: the reference imports it on every target and calls it only under
// `comptime builtin.os.tag == .windows`; the port imports it where it is
// called (its own tests run on every host).
#[cfg(windows)]
use crate::kitty::windows as kitty_windows;
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
//
// PORT NOTE: the reference's scope tag, carried through the `log` facade as
// `target: LOG_SCOPE` (DECISION-logging.md §6r4). NOTE that two of this file's
// calls are `std.log.warn` — the UNSCOPED logger, i.e. the `.default` scope,
// whose carrier is the EMPTY target, not this constant.
const LOG_SCOPE: &str = "kitty_gfx";

/// Maximum width or height of an image. Taken directly from Kitty.
const MAX_DIMENSION: u32 = 10000;

/// Maximum size in bytes, taken from Kitty.
const MAX_SIZE: usize = 400 * 1024 * 1024; // 400MB

// PORT NOTE: `std.fs.max_path_bytes`, the size of the canonical path buffers
// `readFile` and `isPathInTempDir` allocate (`0b5446364`): `PATH_MAX` on the
// unix targets and `PATH_MAX_WIDE * 3 + 1` on Windows (`rustty_os::path`),
// and 4096 elsewhere (Zig's wasi value; on freestanding wasm the file media
// never open a file).
#[cfg(any(unix, windows))]
const MAX_PATH_BYTES: usize = rustty_os::path::MAX_PATH_BYTES;
#[cfg(not(any(unix, windows)))]
const MAX_PATH_BYTES: usize = 4096;

/// The error set of `LoadingImage::validatedFilePath`.
// PORT NOTE: the reference's is inferred (`![]const u8`): `File.realPath`'s
// errors, the blocklist's `error.InvalidData`, and on Windows
// `kitty_windows.Error` from `checkCanonicalPath` (`0b5446364`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidatedFilePathError {
    /// The blocklist refused the canonical path.
    #[cfg_attr(windows, allow(dead_code))]
    InvalidData,
    /// The canonical path does not fit the buffer (`error.NameTooLong`).
    NameTooLong,
    /// `File.realPath` failed.
    RealPath(std::io::ErrorKind),
    /// `kitty_windows.checkCanonicalPath` refused the canonical path.
    #[cfg(windows)]
    Windows(kitty_windows::Error),
}

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
    // PORT NOTE (parity tail S7-4): `std.ArrayListUnmanaged(u8) = .{}`, grown
    // through the allocator `init` was given (every later `alloc` argument is
    // that one: the storage's).
    pub data: Bytes,

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

/// The result of `LoadingImage::pread_range` (R13, UD61).
#[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
enum PreadOutcome {
    /// All requested bytes were read.
    Read,
    /// The descriptor cannot be read with `pread`; map it instead.
    Unsupported,
    /// A short read (the object shrank) or a read error.
    Failed,
    /// The destination could not grow (parity tail S7-4).
    OutOfMemory,
}

/// PORT NOTE: the reference nests `const SharedMemoryRange = struct { start:
/// usize, end: usize };` inside `LoadingImage`; hoisted to module level because
/// Rust has no nested type namespacing on structs.
// (Used only by the POSIX shared-memory medium, unsupported on Windows,
// Android and wasm32; see `readSharedMemory`.)
#[cfg_attr(
    any(windows, target_os = "android", target_arch = "wasm32"),
    allow(dead_code)
)]
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
    pub fn init(
        alloc: Allocator,
        cmd: &command::Command,
        limits: &Limits,
    ) -> Result<LoadingImage, Error> {
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

            data: Bytes::new_in(alloc),
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
            result.addData(alloc, &cmd.data)?;
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
                log::warn!(target: LOG_SCOPE, "invalid image path: BadPathName");
                return Err(Error::InvalidData);
            }
        }

        // Depending on the medium, load the data from the path.
        match t.medium {
            command::Medium::Direct => unreachable!(), // handled above
            command::Medium::File => {
                result.readFile(command::Medium::File, alloc, &t, &cmd.data)?
            }
            command::Medium::TemporaryFile => {
                result.readFile(command::Medium::TemporaryFile, alloc, &t, &cmd.data)?
            }
            command::Medium::SharedMemory => result.readSharedMemory(alloc, &t, &cmd.data)?,
        }

        Ok(result)
    }

    /// Reads the data from a shared memory segment.
    fn readSharedMemory(
        &mut self,
        alloc: Allocator,
        t: &command::Transmission,
        path: &[u8],
    ) -> Result<(), Error> {
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
            let _ = (alloc, t, path);
            Err(Error::UnsupportedMedium)
        }
        #[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
        {
            self.readSharedMemoryPosix(alloc, t, path)
        }
    }

    #[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
    fn readSharedMemoryPosix(
        &mut self,
        alloc: Allocator,
        t: &command::Transmission,
        path: &[u8],
    ) -> Result<(), Error> {
        self.data.allocator().debug_assert_same(alloc);
        use std::os::raw::c_int;

        // POSIX shared memory names must begin with a slash, contain at
        // least one character after it, contain no other slashes, and fit
        // within NAME_MAX. Some shm_open implementations accept names
        // without the leading slash, but the Kitty protocol does not.
        if !validSharedMemoryName(path, NAME_MAX) {
            // log.warn("invalid shared memory name", .{});
            log::warn!(target: LOG_SCOPE, "invalid shared memory name");
            return Err(Error::InvalidData);
        }

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
            //
            // PORT NOTE (N11): the reference renders a Zig `error.<Name>`; the
            // port's failure is a libc errno, rendered through `io::Error`.
            log::warn!(
                target: LOG_SCOPE,
                "unable to open shared memory {}: {}",
                String::from_utf8_lossy(path),
                std::io::Error::last_os_error(),
            );
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
                log::warn!(
                    target: LOG_SCOPE,
                    "unable to fstat shared memory {}: {}",
                    String::from_utf8_lossy(path),
                    std::io::Error::last_os_error(),
                );
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

        #[cfg(test)]
        Self::test_after_fstat_hook(|hook| {
            if let Some(f) = hook.borrow_mut().as_mut() {
                f();
            }
        });

        // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD61), plus the
        // port's own P6. The reference maps `stat_size` bytes `MAP_SHARED` and
        // copies `map[range.start..range.end]`. The object belongs to the peer
        // that sent `t=s`. If that peer shrinks it with `ftruncate` after the
        // `fstat` above, the first touch of a page past the new end raises SIGBUS
        // and the terminal dies (measured on Linux; a signal, so every build mode).
        // Separately, the port formed a `&[u8]` over memory the peer can write
        // while it is borrowed, which is undefined behaviour in Rust even with no
        // truncation.
        //
        // The port reads the range with `pread`, which reports a truncated
        // object as a short read. A short read or a read error fails the load
        // with `InvalidData`, the error every other shared-memory failure here
        // returns; no message is logged, because the reference has none for
        // this case. Where `pread` cannot read a shared-memory object (macOS
        // answers ESPIPE, measured; macOS also refuses to shrink one, measured,
        // so the SIGBUS cannot happen there), the port keeps the reference's
        // mapping but copies out of it through a raw pointer, never a reference.
        assert(self.data.is_empty());
        match Self::pread_range(
            guard.fd,
            range.start,
            range.end - range.start,
            &mut self.data,
        ) {
            PreadOutcome::Read => return Ok(()),
            PreadOutcome::Failed => {
                self.data.clear();
                return Err(Error::InvalidData);
            }
            // The allocation `appendSlice` would make failed.
            PreadOutcome::OutOfMemory => return Err(Error::OutOfMemory),
            PreadOutcome::Unsupported => {}
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
            log::warn!(
                target: LOG_SCOPE,
                "unable to mmap shared memory {}: {}",
                String::from_utf8_lossy(path),
                std::io::Error::last_os_error(),
            );
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
        // P6: no `&[u8]` over the peer-writable mapping; copy through a raw
        // pointer. `sharedMemoryRange` guarantees `range.end <= stat_size`.
        let len = range.end - range.start;
        // try self.data.appendSlice(alloc, map[range.start..range.end]);
        self.data
            .try_reserve_exact(len)
            .map_err(|_| Error::OutOfMemory)?;
        // SAFETY: the mapping is `stat_size` readable bytes and
        // `range.start + len == range.end <= stat_size`; `self.data` is empty
        // with at least `len` bytes of capacity, and the two do not overlap.
        unsafe {
            std::ptr::copy_nonoverlapping(
                (map_ptr as *const u8).add(range.start),
                self.data.as_mut_ptr(),
                len,
            );
            self.data.set_len(len);
        }
        Ok(())
    }

    /// Reads `len` bytes at `start` from `fd` into the empty `dst` with
    /// `pread` (R13, UD61). `Failed` covers a short read, which is how a
    /// truncated object shows up, and any read error. `Unsupported` means the
    /// descriptor cannot be read this way at all (macOS POSIX shared memory
    /// answers ESPIPE), so the caller falls back to mapping it.
    #[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
    fn pread_range(
        fd: std::os::raw::c_int,
        start: usize,
        len: usize,
        dst: &mut Bytes,
    ) -> PreadOutcome {
        if dst.try_reserve_exact(len).is_err() {
            return PreadOutcome::OutOfMemory;
        }
        let mut done: usize = 0;
        while done < len {
            let Ok(offset) = libc::off_t::try_from(start + done) else {
                return PreadOutcome::Failed;
            };
            // SAFETY: `dst` has at least `len` bytes of capacity and
            // `done < len`, so `len - done` bytes from `done` are writable.
            let n =
                unsafe { libc::pread(fd, dst.as_mut_ptr().add(done).cast(), len - done, offset) };
            if n < 0 {
                match std::io::Error::last_os_error().raw_os_error() {
                    Some(libc::EINTR) => continue,
                    Some(libc::ESPIPE | libc::ENXIO | libc::ENODEV | libc::EOPNOTSUPP)
                        if done == 0 =>
                    {
                        return PreadOutcome::Unsupported;
                    }
                    _ => return PreadOutcome::Failed,
                }
            }
            if n == 0 {
                // End of file before `len` bytes: the object shrank.
                return PreadOutcome::Failed;
            }
            done += n as usize;
        }
        // SAFETY: `pread` initialised the first `len` bytes.
        unsafe { dst.set_len(len) };
        PreadOutcome::Read
    }

    /// Test seam for UD61: runs `f` on the hook invoked between the `fstat`
    /// and the read in `readSharedMemoryPosix`, where the reference's
    /// truncation window is. Test builds only.
    #[cfg(all(test, not(any(windows, target_os = "android", target_arch = "wasm32"))))]
    pub(crate) fn test_after_fstat_hook<R>(
        f: impl FnOnce(&std::cell::RefCell<Option<Box<dyn FnMut()>>>) -> R,
    ) -> R {
        thread_local! {
            static HOOK: std::cell::RefCell<Option<Box<dyn FnMut()>>> =
                const { std::cell::RefCell::new(None) };
        }
        HOOK.with(f)
    }

    /// Returns the byte range to copy from a shared memory object.
    // PORT NOTE: the reference return type is the inline error set
    // `error{ InvalidData, DimensionsTooLarge }!SharedMemoryRange`; both members
    // exist on the module `Error` enum, so it is used directly.
    #[cfg_attr(
        any(windows, target_os = "android", target_arch = "wasm32"),
        allow(dead_code)
    )]
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
        alloc: Allocator,
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

        // Some Windows paths are dangerous to even open, so the raw path
        // is checked before the open. The canonical path of the opened
        // file is checked again in validatedFilePath. See kitty_windows
        // for what is refused and why.
        #[cfg(windows)]
        if let Err(err) = kitty_windows::check_path(path) {
            // log.warn("invalid image path: {}", .{err});
            log::warn!(target: LOG_SCOPE, "invalid image path: {err:?}");
            return Err(Error::InvalidData);
        }

        // The canonical path buffer is sized for the longest path the
        // platform allows, which is about 96 KiB on Windows, so it is
        // heap allocated rather than placed on the stack of whichever
        // host thread feeds the stream. It is allocated before the open
        // so the deferred temporary file deletion below can still use it.
        // const abs_buf = try alloc.alloc(u8, std.fs.max_path_bytes);
        // defer alloc.free(abs_buf);
        //
        // PORT NOTE (`0b5446364`, N360): one `alloc` of `max_path_bytes`
        // through the caller's allocator, freed on every exit path. The buffer
        // is owned by the delete guard below, because the delete path is a
        // prefix of it (the reference's `delete_path = abs_path`).
        let mut abs_buf = Bytes::new_in(alloc);
        if abs_buf.try_reserve_exact(MAX_PATH_BYTES).is_err() {
            return Err(Error::OutOfMemory);
        }
        // The capacity is reserved, so this never allocates.
        abs_buf.resize_copy(MAX_PATH_BYTES, 0);

        // We'll populate a delete path if this is a temporary file.
        // PORT NOTE: the reference's `defer { file.close(io);
        // if (delete_path) |p| ... deleteFile(p) ... }` runs close-then-delete
        // on every exit path, and the earlier `defer alloc.free(abs_buf)` runs
        // after it. Rust drop order is reverse declaration order, so the guard
        // is declared BEFORE `file`: at scope exit `file` drops first (close),
        // then the guard best-effort deletes, then its buffer is freed.
        struct DeletePathGuard {
            abs_buf: Bytes,
            // var delete_path: ?[]const u8 = null;
            //
            // (the length of the `abs_buf` prefix that is the delete path)
            delete_path: Option<usize>,
        }
        impl Drop for DeletePathGuard {
            fn drop(&mut self) {
                if let Some(len) = self.delete_path {
                    if let Err(err) = unlink(&self.abs_buf[..len]) {
                        // log.warn("failed to delete temporary file: {}", .{err});
                        log::warn!(target: LOG_SCOPE, "failed to delete temporary file: {err}");
                    }
                }
            }
        }
        let mut delete_guard = DeletePathGuard {
            abs_buf,
            delete_path: None,
        };

        // Open our file right away before we do validation. This avoids
        // TOCTOU issues.
        let mut file = match open_file(path) {
            Ok(f) => f,
            Err(err) => {
                // log.warn("failed to open image file: {}", .{err});
                log::warn!(target: LOG_SCOPE, "failed to open image file: {err}");
                return Err(Error::InvalidData);
            }
        };
        // defer file.close(); — handled by `File` Drop (see guard note above).

        // Derive the path from the open handle so the file we validate is the
        // exact file we read. Resolving a path before opening it would allow a
        // cooperating process to swap a symlink or directory entry in between.
        let abs_path = match Self::validatedFilePath(&file, &mut delete_guard.abs_buf) {
            Ok(p) => p,
            Err(err) => {
                // log.warn("failed to validate image file path: {}", .{err});
                log::warn!(target: LOG_SCOPE, "failed to validate image file path: {err:?}");
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
            if !Self::isPathInTempDir(alloc, dir, abs_path)? {
                return Err(Error::TemporaryFileNotInTempDir);
            }
            if index_of(abs_path, b"tty-graphics-protocol").is_none() {
                return Err(Error::TemporaryFileNotNamedCorrectly);
            }
            // delete_path = abs_path;
            delete_guard.delete_path = Some(abs_path.len());
        }

        // File must be a regular file
        match file.metadata() {
            Ok(md) => {
                if !md.is_file() {
                    // log.warn("file is not a regular file kind={}", .{stat.kind});
                    //
                    // PORT NOTE: `stat.kind` is Zig's `File.Kind` enum; the
                    // Rust counterpart is `std::fs::FileType`, rendered by its
                    // own Debug.
                    log::warn!(
                        target: LOG_SCOPE,
                        "file is not a regular file kind={:?}",
                        md.file_type()
                    );
                    return Err(Error::InvalidData);
                }
            }
            Err(err) => {
                // log.warn("failed to stat file: {}", .{err});
                log::warn!(target: LOG_SCOPE, "failed to stat file: {err}");
                return Err(Error::InvalidData);
            }
        }

        if t.offset > 0 {
            use std::io::Seek;
            if let Err(err) = file.seek(std::io::SeekFrom::Start(t.offset as u64)) {
                // log.warn("failed to seek to offset {}: {}", .{ t.offset, err });
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to seek to offset {}: {err}",
                    t.offset
                );
                return Err(Error::InvalidData);
            }
        }

        // PORT NOTE: the reference reads through a 4096-byte buffered reader.
        // We read with the same semantics into a byte list on `alloc`.
        // Read the file
        // var managed: std.ArrayList(u8) = .empty;
        // errdefer managed.deinit(alloc);
        let mut managed: Bytes = Bytes::new_in(alloc);
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
            // reproduces that contract; the buffer is reserved up front through
            // `alloc`, fallibly (`error.OutOfMemory`).
            // const data = reader.readAlloc(alloc, size) catch |err| {
            //     log.warn("failed to read image file: {}", .{err});
            //     return switch (err) {
            //         error.OutOfMemory => error.OutOfMemory,
            //         else => error.InvalidData,
            //     };
            // };
            use std::io::Read;
            if managed.try_reserve_exact(size).is_err() {
                log::warn!(target: LOG_SCOPE, "failed to read image file: OutOfMemory");
                return Err(Error::OutOfMemory);
            }
            // The capacity is reserved, so this never allocates.
            managed.resize_copy(size, 0);
            if let Err(err) = file.read_exact(&mut managed) {
                // log.warn("failed to read image file: {}", .{err});
                log::warn!(target: LOG_SCOPE, "failed to read image file: {err}");
                return Err(Error::InvalidData);
            }
        } else {
            // reader.appendRemaining(alloc, &managed, .limited(max_size)) catch {
            //
            // PORT NOTE: reference defect (R13), UD140 (parity S189-F2).
            // `.limited(max_size)` fails "if `limit` is reached or exceeded",
            // so the reference rejects a file of EXACTLY `max_size` bytes,
            // while every other `max_size` check in this file is inclusive
            // (`> max_size`: the `s=` size above, the direct/chunked data
            // accumulation, the PNG result), and the same 400 MB file loads
            // when sent with `s=`. Reachable by any program that can send a
            // file-medium transmission without `s=`. The port reads with a
            // limit of `max_size + 1`, so exactly `max_size` bytes load and
            // anything longer still fails with `InvalidData`.
            if let Err(err) = append_remaining_limited(&mut file, &mut managed, MAX_SIZE + 1) {
                // log.warn("failed to read image file: {?}", .{buf_reader.err});
                //
                // PORT NOTE: the reference renders the buffered reader's
                // OPTIONAL stored error (`{?}`): the file's read error, or
                // `null` when the failure is `error.StreamTooLong`, which
                // leaves it unset.
                let err: String = match err {
                    AppendRemainingError::ReadFailed(err) => err.to_string(),
                    AppendRemainingError::StreamTooLong => String::from("null"),
                    AppendRemainingError::OutOfMemory => String::from("OutOfMemory"),
                };
                log::warn!(target: LOG_SCOPE, "failed to read image file: {err}");
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
    // which would reintroduce the TOCTOU this exists to close. The error set
    // is the reference's inferred one: `realPath`'s failures, the blocklist's
    // `error.InvalidData`, and on Windows `kitty_windows.Error`
    // ([`ValidatedFilePathError`]); the caller logs it and maps every error to
    // `InvalidData`.
    //
    // PORT NOTE (N360): `realpath_of_handle` returns the path in a transient
    // Rust-heap `Vec` (the reference writes it straight into `buf`), which is
    // copied into `buf`; a path longer than `buf` is `error.NameTooLong`, as
    // `realPath` reports it. The caller's allocator sees only `buf`.
    fn validatedFilePath<'b>(
        file: &std::fs::File,
        buf: &'b mut [u8],
    ) -> Result<&'b [u8], ValidatedFilePathError> {
        // const path = buf[0..try file.realPath(io, buf)];
        let real = match realpath_of_handle(file) {
            Ok(p) => p,
            Err(err) => return Err(ValidatedFilePathError::RealPath(err.kind())),
        };
        if real.len() > buf.len() {
            return Err(ValidatedFilePathError::NameTooLong);
        }
        buf[..real.len()].copy_from_slice(&real);
        let path: &'b [u8] = &buf[..real.len()];

        // if (comptime builtin.os.tag == .windows) {
        //     try kitty_windows.checkCanonicalPath(path);
        //     return path;
        // }
        #[cfg(windows)]
        {
            kitty_windows::check_canonical_path(path).map_err(ValidatedFilePathError::Windows)?;
            Ok(path)
        }

        #[cfg(not(windows))]
        {
            // This is logic copied directly from Kitty, mostly. This is really
            // rough but it will catch obvious bad actors.
            if starts_with(path, b"/proc/")
                || starts_with(path, b"/sys/")
                || (starts_with(path, b"/dev/") && !starts_with(path, b"/dev/shm/"))
            {
                return Err(ValidatedFilePathError::InvalidData);
            }

            Ok(path)
        }
    }

    /// Returns true if path appears to be in a temporary directory.
    /// Copies logic from Kitty.
    // PORT NOTE (oracle `e8525c0fd`(iii), `b988efcfe`): `dir` is now a
    // PARAMETER. The previous body called `temp_dir.allocTmpDir()`, which read
    // `TMPDIR`/`TMP` from the process environment at validation time; that
    // lookup is DELETED. See the `temp_dir` module note above.
    // PORT NOTE (`0b5446364`): now `Allocator.Error!bool`: the real-path
    // buffer below is allocated through `alloc`, and only that allocation's
    // failure is an error (`Error::OutOfMemory`).
    fn isPathInTempDir(alloc: Allocator, dir: &[u8], path: &[u8]) -> Result<bool, Error> {
        if isPathInDir(b"/tmp", path) {
            return Ok(true);
        }
        if isPathInDir(b"/dev/shm", path) {
            return Ok(true);
        }
        if isPathInDir(dir, path) {
            return Ok(true);
        }

        // The temporary dir is sometimes a symlink. On macOS for
        // example /tmp is /private/var/... On Windows the directory a
        // host passes (typically GetTempPath output) regularly differs
        // from the canonical path in case or uses 8.3 short names, and
        // resolving it covers both. The buffer is heap allocated for
        // the reason given in readFile.
        // const buf = try alloc.alloc(u8, std.fs.max_path_bytes);
        // defer alloc.free(buf);
        let mut buf = Bytes::new_in(alloc);
        if buf.try_reserve_exact(MAX_PATH_BYTES).is_err() {
            return Err(Error::OutOfMemory);
        }
        // The capacity is reserved, so this never allocates.
        buf.resize_copy(MAX_PATH_BYTES, 0);
        // PORT NOTE: `std.Io.Dir.cwd().realPathFile(io, dir, buf) catch return
        // false`. `e8525c0fd` turned the old `if (posix.realpath(...)) |…| …
        // else |_| {}` fall-through into an early `return false`; with nothing
        // after the check in either version the two are equivalent, and we
        // follow the new shape. `realpath` returns a transient Rust-heap `Vec`
        // (N360), copied into `buf`; one longer than `buf` is `realPathFile`'s
        // `error.NameTooLong`, so `false` too.
        let Ok(real) = realpath(dir) else {
            return Ok(false);
        };
        if real.len() > buf.len() {
            return Ok(false);
        }
        buf[..real.len()].copy_from_slice(&real);
        let real_dir = &buf[..real.len()];
        if isPathInDir(real_dir, path) {
            return Ok(true);
        }

        Ok(false)
    }

    // pub fn deinit(self: *LoadingImage, alloc: Allocator) void {
    //     self.image.deinit(alloc);
    //     self.data.deinit(alloc);
    // }
    // PORT NOTE: `self.image` and `self.data` free through the allocator they
    // were allocated with when dropped; `deinit` releases them now and checks
    // that allocator.
    pub fn deinit(&mut self, alloc: Allocator) {
        self.image.deinit(alloc);
        self.data.allocator().debug_assert_same(alloc);
        self.data = Bytes::new_in(alloc);
    }

    // pub fn destroy(self: *LoadingImage, alloc: Allocator) void {
    //     self.deinit(alloc);
    //     alloc.destroy(self);
    // }
    pub fn destroy(this: ABox<LoadingImage>, alloc: Allocator) {
        ABox::allocator(&this).debug_assert_same(alloc);
        let mut loading = ABox::into_inner(this);
        loading.deinit(alloc);
    }

    /// Adds a chunk of data to the image. Use this if the image
    /// is coming in chunks (the "m" parameter in the protocol).
    pub fn addData(&mut self, alloc: Allocator, data: &[u8]) -> Result<(), Error> {
        // If no data, skip
        if data.is_empty() {
            return Ok(());
        }

        // If our data would get too big, return an error
        if self.data.len() + data.len() > MAX_SIZE {
            // log.warn("image data too large max_size={}", .{max_size});
            log::warn!(target: LOG_SCOPE, "image data too large max_size={}", MAX_SIZE);
            return Err(Error::InvalidData);
        }

        // Ensure we have enough room to add the data
        // to the end of the ArrayList before doing so.
        // try self.data.ensureUnusedCapacity(alloc, data.len);
        self.data.allocator().debug_assert_same(alloc);
        self.data
            .try_reserve(data.len())
            .map_err(|_| Error::OutOfMemory)?;

        // PORT NOTE: `fastmem.copy(u8, self.data.items[start_i..], data)` after
        // bumping the length. The capacity is reserved, so growing the list
        // never allocates; then memcpy into the tail.
        let start_i = self.data.len();
        self.data.resize_copy(start_i + data.len(), 0);
        fastmem::copy(&mut self.data[start_i..], data);
        Ok(())
    }

    /// Complete the chunked image, returning a completed image.
    pub fn complete(&mut self, alloc: Allocator) -> Result<Image, Error> {
        // Decompress the data if it is compressed.
        self.decompress(alloc)?;

        // Decode the png if we have to
        if self.image.format == command::Format::Png {
            self.decodePng(alloc)?;
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
                //
                // PORT NOTE: `std.log.warn` is the UNSCOPED logger — the
                // reference's `.default` scope, whose facade carrier is the
                // EMPTY target (§6r4), NOT this file's `.kitty_gfx` scope.
                log::warn!(
                    target: "",
                    "insufficient frame data image id={} expected_len={expected_len} actual_len={actual_len}",
                    self.image.id
                );
                return Err(Error::InsufficientData);
            }
            // self.data.items.len = expected_len;
            self.data.truncate(expected_len);
        } else if actual_len != expected_len {
            // std.log.warn(
            //     "unexpected length image id={} width={} height={} bpp={} expected_len={} actual_len={}",
            //     .{ img.id, img.width, img.height, bpp, expected_len, actual_len },
            // );
            //
            // PORT NOTE: `std.log.warn` — the `.default` scope; see the
            // sibling above.
            log::warn!(
                target: "",
                "unexpected length image id={} width={} height={} bpp={bpp} expected_len={expected_len} actual_len={actual_len}",
                self.image.id,
                self.image.width,
                self.image.height
            );
            return Err(Error::InvalidData);
        }

        // (deleted — bdc0b6c19: `self.image.transmit_time = Instant::now()` and
        // its `InternalError` failure path are gone; the stamp is now assigned
        // by `ImageStorage::add_image`.)

        // Everything looks good, copy the image data over.
        // PORT NOTE: `result.data = .{ .complete = try self.data.toOwnedSlice(alloc) };`
        // then `self.image = .{}`. We move our data into the result image (the
        // list itself, not a shrunk copy: a T3 call-shape residual) and reset
        // `self.image` to its default so a later drop frees nothing twice.
        self.data.allocator().debug_assert_same(alloc);
        let mut result = std::mem::take(&mut self.image);
        result.data = Data::Complete(core::mem::replace(&mut self.data, Bytes::new_in(alloc)));
        Ok(result)
    }

    /// Debug function to write the data to a file. This is useful for
    /// capturing some test data for unit tests.
    //
    // pub fn debugDump(io: std.Io, self: LoadingImage) !void {
    //     if (comptime builtin.mode != .Debug) @compileError("debugDump in non-debug");
    //
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
    //     const cwd = std.Io.Dir.cwd();
    //     const f = try cwd.createFile(io, filename, .{});
    //     defer f.close(io);
    //
    //     const writer = f.writer();
    //     try writer.writeAll(self.data.items);
    // }
    //
    // PORT NOTE: the reference's `if (comptime builtin.mode != .Debug)
    // @compileError(...)` makes this function EXIST only in a Debug build;
    // `#[cfg(debug_assertions)]` is the exact analogue (a release build has no
    // such item, so a caller added in release fails to compile, as it does
    // upstream). The `io: std.Io` parameter is the reference's explicit IO
    // handle — `std::fs` carries none, so it has no counterpart. The 1024-byte
    // `bufPrint` scratch buffer is an allocation detail of Zig's fixed-buffer
    // formatting, not observable output.
    #[cfg(debug_assertions)]
    pub fn debug_dump(&self) -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(self.debug_dump_filename())?;
        f.write_all(&self.data)
    }

    /// The `bufPrint`ed name `debug_dump` writes to.
    //
    // PORT NOTE: split out of `debug_dump` ONLY so the name is assertable
    // without a test writing into the process-wide current directory (the
    // reference's `Dir.cwd()`); the bytes produced are the reference's.
    #[cfg(debug_assertions)]
    pub(crate) fn debug_dump_filename(&self) -> String {
        // @tagName(self.image.format) / @tagName(self.image.compression) —
        // the Zig field names (see `Format::descriptor`/`Compression::descriptor`).
        let format = match self.image.format {
            command::Format::Rgb => "rgb",
            command::Format::Rgba => "rgba",
            command::Format::Png => "png",
            command::Format::GrayAlpha => "gray_alpha",
            command::Format::Gray => "gray",
        };
        let compression = match self.image.compression {
            command::Compression::None => "none",
            command::Compression::ZlibDeflate => "zlib_deflate",
        };
        format!(
            "image-{}-{}-{}x{}-{}.data",
            format, compression, self.image.width, self.image.height, self.image.id,
        )
    }

    /// Decompress the data in-place.
    fn decompress(&mut self, alloc: Allocator) -> Result<(), Error> {
        match self.image.compression {
            command::Compression::None => Ok(()),
            command::Compression::ZlibDeflate => self.decompressZlib(alloc),
        }
    }

    #[cfg(feature = "kitty-graphics")]
    fn decompressZlib(&mut self, alloc: Allocator) -> Result<(), Error> {
        // Provide a bounded size hint based on image metadata for the initial allocation
        // const size_hint: ?usize = hint: { ... };
        let size_hint: Option<usize> = 'hint: {
            if self.image.format == command::Format::Png
                || self.image.width == 0
                || self.image.height == 0
                || self.image.width > MAX_DIMENSION
                || self.image.height > MAX_DIMENSION
            {
                break 'hint None;
            }
            // const pixels = std.math.mul(usize, self.image.width, self.image.height) catch break :hint null;
            let Some(pixels) = (self.image.width as usize).checked_mul(self.image.height as usize)
            else {
                break 'hint None;
            };
            // const bytes = std.math.mul(usize, pixels, command.Transmission.formatBpp(self.image.format)) catch break :hint null;
            let Some(bytes) = pixels.checked_mul(usize::from(command::Transmission::format_bpp(
                self.image.format,
            ))) else {
                break 'hint None;
            };
            // break :hint if (bytes <= max_size) bytes else null;
            if bytes <= MAX_SIZE {
                Some(bytes)
            } else {
                None
            }
        };
        // const decompressed = wuffs.zlib.decode(
        //     alloc,
        //     self.data.items,
        //     max_size,
        //     size_hint,
        // ) catch |err| switch (err) {
        //     error.OutOfMemory => return error.OutOfMemory,
        //     error.WuffsError, error.Overflow => return error.DecompressionFailed,
        // };
        let decompressed = match rustty_wuffs::zlib::decode(alloc, &self.data, MAX_SIZE, size_hint)
        {
            Ok(buf) => buf,
            Err(rustty_wuffs::Error::OutOfMemory) => return Err(Error::OutOfMemory),
            Err(rustty_wuffs::Error::WuffsError | rustty_wuffs::Error::Overflow) => {
                return Err(Error::DecompressionFailed)
            }
        };

        // self.data.deinit(alloc);
        // self.data = .{ .items = decompressed, .capacity = decompressed.len };
        //
        // PORT NOTE: the decoded buffer (allocated through `alloc`, align 1,
        // exactly `len` bytes after `decode`'s final `realloc`) becomes the
        // list's storage with `capacity == len`, as the reference's does; the
        // old list is freed by the assignment.
        self.data.allocator().debug_assert_same(alloc);
        let (ptr, len) = decompressed.into_raw();
        // SAFETY: `ptr` is a live `len`-byte, align-1 allocation made through
        // `alloc` (`RawBuf`), which is exactly `Layout::array::<u8>(len)`;
        // ownership moves to the list.
        self.data = unsafe { Bytes::from_raw_parts_in(ptr.as_ptr(), len, len, alloc) };

        // Make sure we note that our image is no longer compressed
        self.image.compression = command::Compression::None;
        Ok(())
    }

    // PORT NOTE: without the `kitty-graphics` feature the reference compiles
    // no graphics module (`graphics = struct {}`) and links no wuffs, while
    // the port always compiles this one; nothing dispatches into it then
    // (`options::KITTY_GRAPHICS` is false at every call site), so this arm is
    // never reached.
    #[cfg(not(feature = "kitty-graphics"))]
    fn decompressZlib(&mut self, _alloc: Allocator) -> Result<(), Error> {
        unreachable!("kitty graphics is compiled out")
    }

    /// Decode the data as PNG. This will also updated the image dimensions.
    fn decodePng(&mut self, alloc: Allocator) -> Result<(), Error> {
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
        // PORT NOTE (parity tail S7-4): the child is the caller's allocator
        // (the handle's raw vtable), wrapped by the same
        // `datastruct/limited_allocator` the reference wraps. The decoder
        // performs every allocation through the limited allocator, so an
        // oversized decode is rejected BEFORE the memory is requested from the
        // child.
        let limited = LimitedAllocator::init(alloc, MAX_SIZE);
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
        // defer decode_alloc.free(result.data);
        struct FreeDecoded<'a>(&'a dyn sys::RawAllocator, sys::RawSlice);
        impl Drop for FreeDecoded<'_> {
            fn drop(&mut self) {
                self.0.free_bytes(self.1);
            }
        }
        let _free = FreeDecoded(decode_alloc, result.data);
        // SAFETY: the decoder's live buffer, freed by `_free` at scope end.
        let decoded = unsafe { result.bytes() };

        if decoded.len() > MAX_SIZE {
            // log.warn("png image too large size={} max_size={}", .{ result.data.len, max_size });
            log::warn!(
                target: LOG_SCOPE,
                "png image too large size={} max_size={}",
                decoded.len(),
                MAX_SIZE
            );
            return Err(Error::InvalidData);
        }

        // Replace our data
        // self.data.deinit(alloc);
        // self.data = .empty;
        // try self.data.ensureUnusedCapacity(alloc, result.data.len);
        // try self.data.appendSlice(alloc, result.data[0..result.data.len]);
        // (parity tail S7-4b) The new data is allocated through `alloc`,
        // fallibly, and the decoded pixels are copied straight out of the
        // decoder's buffer, as the reference does.
        self.data = Bytes::new_in(alloc);
        self.data
            .try_reserve_exact(decoded.len())
            .map_err(|_| Error::OutOfMemory)?;
        self.data.extend_copy(decoded);

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
    pub animation: Option<ABox<crate::kitty::graphics_animation::Animation>>,
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
    // PORT NOTE (parity tail S7-4): allocated through the storage's
    // allocator (freed by `deinit(alloc)`, or on drop).
    Complete(Bytes),

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

    // pub fn deinit(self: *Data, alloc: Allocator) void {
    //     switch (self.*) {
    //         .complete => |data| if (data.len > 0) alloc.free(data),
    //         .pending => {},
    //     }
    // }
    // PORT NOTE: frees the complete bytes (through the allocator they were
    // allocated with, checked against `alloc`) and leaves the empty default.
    pub fn deinit(&mut self, alloc: Allocator) {
        if let Data::Complete(data) = self {
            if data.capacity() != 0 {
                data.allocator().debug_assert_same(alloc);
            }
            *data = Bytes::new_in(alloc);
        }
    }
}

impl Default for Data {
    // data: Data = .{ .complete = "" },
    fn default() -> Self {
        Data::Complete(Bytes::new_in(crate::alloc::Global))
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
            data: Data::default(),
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
    // pub fn deinit(self: *Image, alloc: Allocator) void {
    //     self.data.deinit(alloc);
    //     if (self.animation) |anim| {
    //         anim.deinit(alloc);
    //         alloc.destroy(anim);
    //         self.animation = null;
    //     }
    // }
    // PORT NOTE: the owning `Data` and animation box free through the
    // allocator they were allocated with; `deinit` releases them now (leaving
    // the empty default) and checks that allocator.
    pub fn deinit(&mut self, alloc: Allocator) {
        self.data.deinit(alloc);
        if let Some(anim) = self.animation.take() {
            ABox::allocator(&anim).debug_assert_same(alloc);
            let mut anim = ABox::into_inner(anim);
            anim.deinit(alloc);
        }
    }

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
    // PORT NOTE (parity tail S7-4): the reference copies the struct by value
    // (the copy shares the animation pointer and allocates nothing). The copy
    // here is field by field: no pixel data and no animation (the port's
    // animation is owned, so a copy cannot share it), so it allocates nothing
    // either. Callers read only the metadata.
    pub fn withoutData(&self) -> Image {
        Image {
            id: self.id,
            number: self.number,
            width: self.width,
            height: self.height,
            format: self.format,
            compression: self.compression,
            // if (copy.data == .complete) copy.data = .{ .complete = "" };
            data: match self.data {
                Data::Complete(_) => Data::default(),
                Data::Pending(len) => Data::Pending(len),
            },
            metadata: self.metadata,
            generation: self.generation,
            animation: None,
        }
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

/// `posix.NAME_MAX` — the longest single path component the shared memory
/// name may be. Zig's `std.posix.NAME_MAX` is 255 on every POSIX target we
/// reach `readSharedMemoryPosix` on (Linux, the BSDs and the Darwin family
/// all define it as 255; `std/c.zig`), and the `libc` crate does not expose it
/// uniformly across those targets, so the value is pinned here.
#[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
const NAME_MAX: usize = 255;

/// The failures of [`append_remaining_limited`].
enum AppendRemainingError {
    /// `error.StreamTooLong`: `limit` bytes were read.
    StreamTooLong,
    /// `error.OutOfMemory`: the list could not grow.
    OutOfMemory,
    /// `error.ReadFailed`, with the reader's error.
    ReadFailed(std::io::Error),
}

/// `reader.appendRemaining(gpa, list, .limited(limit))` (zig 0.16
/// `std/Io/Reader.zig` `appendRemainingAligned`): appends the rest of the
/// stream to `list`, but "if `limit` is reached or exceeded,
/// `error.StreamTooLong` is returned instead", so a stream of exactly `limit`
/// bytes is an error too. (Allocation failure aborts, the Rust global OOM
/// policy.)
// CORRECTION (parity S189-F2): `readFile` used `take(MAX_SIZE).read_to_end`,
// which stops silently at the limit, so a file of `max_size` bytes or more
// loaded truncated instead of failing with `InvalidData`.
//
// PORT NOTE (parity tail S7-4): `list` grows through its own allocator (the
// caller's `gpa`), fallibly: a refused growth is `error.OutOfMemory`, which
// the caller's catch-all reports like any other failure. The growth pattern
// (one reserve per 4 KiB read) is not Zig's (a T3 call-shape residual).
fn append_remaining_limited(
    reader: &mut impl std::io::Read,
    list: &mut Bytes,
    limit: usize,
) -> Result<(), AppendRemainingError> {
    let start = list.len();
    let mut buf = [0u8; 4096];
    loop {
        // Read at most up to the limit (and one past it is never needed:
        // reaching the limit is already the error).
        let room = limit - (list.len() - start);
        if room == 0 {
            return Err(AppendRemainingError::StreamTooLong);
        }
        let want = room.min(buf.len());
        let n = match reader.read(&mut buf[..want]) {
            Ok(0) => return Ok(()),
            Ok(n) => n,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(AppendRemainingError::ReadFailed(err)),
        };
        list.try_extend_copy(&buf[..n])
            .map_err(|_| AppendRemainingError::OutOfMemory)?;
    }
}

/// Returns whether a name follows the POSIX shared memory name format.
#[cfg_attr(
    any(windows, target_os = "android", target_arch = "wasm32"),
    allow(dead_code)
)]
fn validSharedMemoryName(name: &[u8], name_max: usize) -> bool {
    if name.len() < 2 || name.len() > name_max || name[0] != b'/' {
        return false;
    }
    for &c in &name[1..] {
        if c == b'/' || c == 0 {
            return false;
        }
    }

    true
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

// Windows (task W-2): Zig's `realpath` converts the WTF-8 path to WTF-16 (not
// WTF-8 is `error.InvalidWtf8`), opens it and returns
// `GetFinalPathNameByHandle(.Dos)` as WTF-8 — the DOS form, the same form
// `realpath_of_handle` returns, so the temp-directory prefix check compares like
// with like. CORRECTION: this arm required UTF-8 and returned `canonicalize`'s
// verbatim `\\?\C:\..` form, so every temporary-file transmission failed the
// prefix check against the handle's DOS path (`TemporaryFileNotInTempDir`,
// measured on Windows).
#[cfg(windows)]
fn realpath(path: &[u8]) -> std::io::Result<Vec<u8>> {
    let p = rustty_os::path::os_string_from_bytes(path)
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
    let canon = rustty_os::path::realpath(std::path::Path::new(&p))?;
    Ok(canon.into_os_string().into_encoded_bytes())
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

// Windows: Zig's `File.realPath` is `realPathWindows` (std/Io/Threaded.zig):
// `GetFinalPathNameByHandle(h_file, .{ .volume_name = .Dos }, buf)`, then
// `std.unicode.wtf16LeToWtf8`. Zig builds the DOS path from the NT object name
// and the mount manager and does NOT prepend `\\?\`; a UNC path comes back as
// `\\server\share\...`. The Win32 `GetFinalPathNameByHandleW` with
// `VOLUME_NAME_DOS` gives the same path with the `\\?\` (or `\\?\UNC\`) prefix,
// which `final_path_to_zig_dos` strips.
// CORRECTION (parity S189-F4): this arm used to return `Unsupported`, so the
// kitty file and temporary-file media failed closed on Windows.
#[cfg(windows)]
fn realpath_of_handle(file: &std::fs::File) -> std::io::Result<Vec<u8>> {
    use std::os::windows::io::AsRawHandle;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetFinalPathNameByHandleW(
            hfile: *mut core::ffi::c_void,
            lpszfilepath: *mut u16,
            cchfilepath: u32,
            dwflags: u32,
        ) -> u32;
    }
    // FILE_NAME_NORMALIZED | VOLUME_NAME_DOS
    const FLAGS: u32 = 0;

    // `var wide_buf: [windows.PATH_MAX_WIDE]u16`; a longer path is
    // `error.NameTooLong` there, and an error here.
    const PATH_MAX_WIDE: usize = 32767;
    let mut buf = vec![0u16; PATH_MAX_WIDE];
    // SAFETY(ffi-boundary/graphics_image): `buf` holds `buf.len()` u16s and
    // the call writes at most that many, NUL included; `file` keeps the
    // handle open for the duration of the call.
    let n = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buf.as_mut_ptr(),
            buf.len() as u32,
            FLAGS,
        )
    } as usize;
    if n == 0 {
        return Err(std::io::Error::last_os_error());
    }
    // A return value not below the buffer length is the size needed.
    if n >= buf.len() {
        return Err(std::io::Error::from(std::io::ErrorKind::InvalidFilename));
    }
    Ok(wtf16le_to_wtf8(&final_path_to_zig_dos(&buf[..n])))
}

/// `GetFinalPathNameByHandleW(.., VOLUME_NAME_DOS)` output in the form Zig's
/// `GetFinalPathNameByHandle(.., .{ .volume_name = .Dos })` returns it:
/// `\\?\UNC\server\share\x` becomes `\\server\share\x` and `\\?\C:\x`
/// becomes `C:\x`.
#[cfg(any(windows, test))]
fn final_path_to_zig_dos(wide: &[u16]) -> Vec<u16> {
    let unc: Vec<u16> = "\\\\?\\UNC\\".encode_utf16().collect();
    let dos: Vec<u16> = "\\\\?\\".encode_utf16().collect();
    if wide.starts_with(&unc) {
        let mut out: Vec<u16> = "\\\\".encode_utf16().collect();
        out.extend_from_slice(&wide[unc.len()..]);
        out
    } else if wide.starts_with(&dos) {
        wide[dos.len()..].to_vec()
    } else {
        wide.to_vec()
    }
}

/// `std.unicode.wtf16LeToWtf8`: surrogate pairs combine, and an unpaired
/// surrogate is encoded as its own three-byte sequence (WTF-8), so no path is
/// lossy.
#[cfg(any(windows, test))]
fn wtf16le_to_wtf8(wide: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(wide.len());
    let mut i = 0;
    while i < wide.len() {
        let u = wide[i] as u32;
        let cp = if (0xD800..0xDC00).contains(&u)
            && i + 1 < wide.len()
            && (0xDC00..0xE000).contains(&(wide[i + 1] as u32))
        {
            i += 1;
            0x10000 + ((u - 0xD800) << 10) + (wide[i] as u32 - 0xDC00)
        } else {
            u
        };
        i += 1;
        if cp < 0x80 {
            out.push(cp as u8);
        } else if cp < 0x800 {
            out.push(0xC0 | (cp >> 6) as u8);
            out.push(0x80 | (cp & 0x3F) as u8);
        } else if cp < 0x10000 {
            out.push(0xE0 | (cp >> 12) as u8);
            out.push(0x80 | ((cp >> 6) & 0x3F) as u8);
            out.push(0x80 | (cp & 0x3F) as u8);
        } else {
            out.push(0xF0 | (cp >> 18) as u8);
            out.push(0x80 | ((cp >> 12) & 0x3F) as u8);
            out.push(0x80 | ((cp >> 6) & 0x3F) as u8);
            out.push(0x80 | (cp & 0x3F) as u8);
        }
    }
    out
}

// PORT NOTE: remaining targets (other unixes, wasm). Zig's `File.realPath`
// has arms for more unixes than the port's rustty tiers build for; on these
// the kitty file/temporary-file medium fails closed (`InvalidData` via
// `validatedFilePath`).
#[cfg(not(any(
    target_os = "macos",
    target_os = "linux",
    target_os = "android",
    windows
)))]
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

    // Parity S189-F4 (no reference test): the host-independent half of the
    // Windows `realpath_of_handle` arm. The `GetFinalPathNameByHandleW` call
    // itself only runs on Windows.
    #[test]
    fn windows_final_path_takes_zig_dos_form() {
        let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        assert_eq!(
            w("C:\\Users\\a\\img.png"),
            final_path_to_zig_dos(&w("\\\\?\\C:\\Users\\a\\img.png"))
        );
        assert_eq!(
            w("\\\\server\\share\\img.png"),
            final_path_to_zig_dos(&w("\\\\?\\UNC\\server\\share\\img.png"))
        );
        assert_eq!(w("C:\\x"), final_path_to_zig_dos(&w("C:\\x")));
    }

    // Runs in CI's `windows` job (`cargo test -p rustty-vt`): the handle path
    // of a real file is `std::fs::canonicalize`'s answer without the `\\?\`
    // prefix.
    #[cfg(windows)]
    #[test]
    fn windows_realpath_of_handle_is_the_canonical_path() {
        use std::os::windows::ffi::OsStrExt;
        let path = std::env::temp_dir().join("rustty-s189-f4-realpath.txt");
        std::fs::write(&path, b"x").unwrap();
        let file = std::fs::File::open(&path).unwrap();
        let got = realpath_of_handle(&file).unwrap();
        drop(file);
        let canon: Vec<u16> = std::fs::canonicalize(&path)
            .unwrap()
            .as_os_str()
            .encode_wide()
            .collect();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(wtf16le_to_wtf8(&final_path_to_zig_dos(&canon)), got);
        assert!(!got.starts_with(b"\\\\?\\"));
    }

    #[test]
    fn wtf16le_to_wtf8_matches_zig() {
        let w = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        assert_eq!(b"C:\\a".to_vec(), wtf16le_to_wtf8(&w("C:\\a")));
        // Two-, three- and four-byte sequences (U+00E9, U+20AC, U+1F600).
        assert_eq!(
            "\u{e9}\u{20ac}\u{1f600}".as_bytes().to_vec(),
            wtf16le_to_wtf8(&w("\u{e9}\u{20ac}\u{1f600}"))
        );
        // An unpaired surrogate is kept as WTF-8, not replaced.
        assert_eq!(
            vec![b'a', 0xED, 0xA0, 0x80, b'b'],
            wtf16le_to_wtf8(&[0x61, 0xD800, 0x62])
        );
        assert_eq!(vec![0xED, 0xB0, 0x80], wtf16le_to_wtf8(&[0xDC00]));
    }

    // PORT NOTE: the reference's tests construct `command.Command` literals with
    // a `.control = .{ .transmit = ... }` and `.data` slice, then exercise
    // LoadingImage. We build the equivalent `Command` values via the ported
    // `command` module. The reference's `alloc.dupe(u8, ...)` (an owned copy the
    // command frees) maps to an owned `Vec<u8>` in `Command.data`.

    /// PORT NOTE: NOT a reference test — `debugDump` is a Debug-only capture
    /// helper the reference never calls. It pins the `bufPrint` filename shape
    /// (parity S189-F3), whose `@tagName`s are the Zig field names.
    #[cfg(debug_assertions)]
    #[test]
    fn debug_dump_filename_matches_the_reference_bufprint() {
        let mut li = LoadingImage {
            image: Image {
                width: 20,
                height: 10,
                id: 7,
                format: command::Format::Rgba,
                compression: command::Compression::ZlibDeflate,
                ..Image::default()
            },
            data: crate::alloc::Bytes::new_in(crate::alloc::Global),
            display: None,
            frame: None,
            quiet: command::Quiet::No,
            response: command::Response::default(),
            temporary_directory: None,
        };
        assert_eq!(
            "image-rgba-zlib_deflate-20x10-7.data",
            li.debug_dump_filename()
        );

        li.image.format = command::Format::GrayAlpha;
        li.image.compression = command::Compression::None;
        assert_eq!(
            "image-gray_alpha-none-20x10-7.data",
            li.debug_dump_filename()
        );
    }

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

    // test "shared memory names follow POSIX rules" (`090fca451`)
    //
    // NEGATIVE CONTROL: drop the `name[0] != b'/'` clause from
    // `validSharedMemoryName` — the `!validSharedMemoryName("kitty", 8)`
    // assertion fails.
    #[test]
    fn shared_memory_names_follow_POSIX_rules() {
        assert!(validSharedMemoryName(b"/kitty", 8));
        assert!(validSharedMemoryName(b"/1234567", 8));

        assert!(!validSharedMemoryName(b"", 8));
        assert!(!validSharedMemoryName(b"/", 8));
        assert!(!validSharedMemoryName(b"kitty", 8));
        assert!(!validSharedMemoryName(b"/kitty/image", 16));
        assert!(!validSharedMemoryName(b"/kitty\x00image", 16));
        assert!(!validSharedMemoryName(b"/12345678", 8));
    }

    // test "image load rejects invalid POSIX shared memory names" (`090fca451`)
    //
    // The reference skips on Android / Windows / no-libc (`return
    // error.SkipZigTest`); the port gates the whole test on the same targets
    // (`readSharedMemory`'s `UnsupportedMedium` arm — wasm stands in for
    // `!builtin.link_libc`).
    //
    // NEGATIVE CONTROL: remove the `validSharedMemoryName` guard from
    // `readSharedMemoryPosix` — `shm_open("kitty-without-leading-slash")`
    // then decides the outcome: on macOS it fails with a different `errno`
    // but still maps to `InvalidData` (so watch the name check itself via the
    // unit test above), while on Linux a relative name is accepted and the
    // load proceeds to `fstat`/size validation instead.
    #[test]
    #[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
    fn image_load_rejects_invalid_POSIX_shared_memory_names() {
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::SharedMemory,
                width: 1,
                height: 1,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: crate::alloc::heap_bytes(b"kitty-without-leading-slash"),
        };

        assert_eq!(
            Err(Error::InvalidData),
            LoadingImage::init(
                crate::alloc::Global,
                &cmd,
                &Limits {
                    file: false,
                    temporary_file: TemporaryFileLimit::Disabled,
                    shared_memory: true,
                },
            )
            .map(|_| ())
        );
    }

    // ---- task-14e-2: the restored `kitty/graphics_image.zig` diagnostics ----
    //
    // `assert_eq!` on the FULL captured record vector, never `.any()`.

    /// `graphics_image.zig:218` — a shared-memory name the protocol rejects.
    /// Rides the same driver as
    /// `image_load_rejects_invalid_POSIX_shared_memory_names` above.
    #[test]
    #[cfg(not(any(windows, target_os = "android", target_arch = "wasm32")))]
    fn an_invalid_shared_memory_name_warns() {
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::SharedMemory,
                width: 1,
                height: 1,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: crate::alloc::heap_bytes(b"kitty-without-leading-slash"),
        };

        let cap = crate::log_capture::start();
        let out = LoadingImage::init(
            crate::alloc::Global,
            &cmd,
            &Limits {
                file: false,
                temporary_file: TemporaryFileLimit::Disabled,
                shared_memory: true,
            },
        )
        .map(|_| ());
        let messages = cap.messages();
        drop(cap);

        // Positive reach: the guard's own return value.
        assert_eq!(Err(Error::InvalidData), out);
        assert_eq!(
            vec![(
                log::Level::Warn,
                "kitty_gfx".to_string(),
                "invalid shared memory name".to_string(),
            )],
            messages
        );
    }

    /// `graphics_image.zig:551` — the UNSCOPED `std.log.warn` (the `.default`
    /// scope, carried by the EMPTY target) when the accumulated data length
    /// does not match `width * height * bpp`.
    #[test]
    fn an_unexpected_data_length_warns_under_the_default_scope() {
        // 2x1 RGB => 6 bytes expected; supply 3.
        let mut loading = shm_loading(2, 1, command::Format::Rgb);
        loading.data = crate::alloc::heap_bytes(&[0u8; 3]);

        let cap = crate::log_capture::start();
        let out = loading.complete(crate::alloc::Global).map(|_| ());
        let messages = cap.messages();
        drop(cap);

        // Positive reach: the guard's own return value.
        assert_eq!(Err(Error::InvalidData), out);
        assert_eq!(
            vec![(
                log::Level::Warn,
                // `std.log.warn`, not `log.warn` — the `.default` scope.
                String::new(),
                "unexpected length image id=0 width=2 height=1 bpp=3 \
                 expected_len=6 actual_len=3"
                    .to_string(),
            )],
            messages
        );
    }

    /// The absence half: a correctly-sized image completes silently. Positive
    /// reach is the completed `Image`'s own data length.
    #[test]
    fn a_correctly_sized_image_completes_silently() {
        let mut loading = shm_loading(2, 1, command::Format::Rgb);
        loading.data = crate::alloc::heap_bytes(&[0u8; 6]);

        let cap = crate::log_capture::start();
        let out = loading.complete(crate::alloc::Global);
        let messages = cap.messages();
        drop(cap);

        let img = out.expect(
            "complete() failed, so this test never reached the code whose \
             silence it asserts",
        );
        match &img.data {
            Data::Complete(d) => assert_eq!(6, d.len()),
            other => panic!("unexpected image data: {other:?}"),
        }
        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
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
            data: crate::alloc::Bytes::new_in(crate::alloc::Global),
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
            data: crate::alloc::heap_bytes(b"AAAA"),
        };
        let _loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();
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
            data: crate::alloc::heap_bytes(b"AAAA"),
        };
        let mut loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();
        assert_eq!(
            Err(Error::DimensionsTooLarge),
            loading.complete(crate::alloc::Global)
        );
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
            data: crate::alloc::heap_bytes(b"AAAA"),
        };
        let mut loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();
        assert_eq!(
            Err(Error::DimensionsTooLarge),
            loading.complete(crate::alloc::Global)
        );
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
            data: crate::alloc::heap_bytes(b"AAAA"),
        };
        let mut loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();
        assert_eq!(
            Err(Error::DimensionsTooLarge),
            loading.complete(crate::alloc::Global)
        );
    }

    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md): Miri cannot call the vendored Wuffs zlib decoder (C FFI, R14) -- foreign function; covered natively + by ASan"
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
            data: crate::alloc::heap_bytes(include_bytes!(
                "testdata/image-rgb-zlib_deflate-128x96-2147483647-raw.data"
            )),
        };
        let mut loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();
        let img = loading.complete(crate::alloc::Global).unwrap();

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
            data: crate::alloc::heap_bytes(include_bytes!(
                "testdata/image-rgb-none-20x15-2147483647-raw.data"
            )),
        };
        let mut loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();
        let img = loading.complete(crate::alloc::Global).unwrap();

        // should be decompressed
        assert!(img.compression == command::Compression::None);
    }

    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md): Miri cannot call the vendored Wuffs zlib decoder (C FFI, R14) -- foreign function; covered natively + by ASan"
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
            data: crate::alloc::heap_bytes(&data[0..1024]),
        };
        let mut loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();

        // Read our remaining chunks
        let mut pos = 1024;
        loop {
            let end = std::cmp::min(pos + 1024, data.len());
            let size = end - pos;
            loading
                .addData(crate::alloc::Global, &data[pos..end])
                .unwrap();
            pos = end;
            if size < 1024 {
                break;
            }
        }

        // Complete
        let img = loading.complete(crate::alloc::Global).unwrap();
        assert!(img.compression == command::Compression::None);
    }

    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md): Miri cannot call the vendored Wuffs zlib decoder (C FFI, R14) -- foreign function; covered natively + by ASan"
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
            data: crate::alloc::Bytes::new_in(crate::alloc::Global),
        };
        let mut loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();

        // Read our remaining chunks
        let mut pos = 0;
        loop {
            let end = std::cmp::min(pos + 1024, data.len());
            let size = end - pos;
            loading
                .addData(crate::alloc::Global, &data[pos..end])
                .unwrap();
            pos = end;
            if size < 1024 {
                break;
            }
        }

        // Complete
        let img = loading.complete(crate::alloc::Global).unwrap();
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
            data: crate::alloc::heap_bytes(&path_bytes),
        };
        assert_eq!(
            Err(Error::TemporaryFileNotNamedCorrectly),
            LoadingImage::init(
                crate::alloc::Global,
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
        // `realPath`/`realPathFile` give the DOS form on Windows (`C:\..`), as
        // `rustty_os::path::realpath` does; `std::fs::canonicalize`'s verbatim
        // `\\?\C:\..` is a device-namespace spelling that `readFile` refuses
        // before the open since `0b5446364` (measured on rwin, P7-W5 T2).
        let base = rustty_os::path::realpath(&base).unwrap();
        // try tmp_dir.dir.createDir(io, "temp", .default_dir);
        // try tmp_dir.dir.createDir(io, "temp-suffix", .default_dir);
        std::fs::create_dir(base.join("temp")).unwrap();
        std::fs::create_dir(base.join("temp-suffix")).unwrap();

        let filename = "tty-graphics-protocol-image.data";
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        std::fs::write(base.join("temp-suffix").join(filename), data).unwrap();

        let trusted_path = rustty_os::path::realpath(&base.join("temp")).unwrap();
        let outside_path =
            rustty_os::path::realpath(&base.join("temp-suffix").join(filename)).unwrap();

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
            data: crate::alloc::heap_bytes(&path_to_bytes(&outside_path)),
        };
        assert_eq!(
            Err(Error::TemporaryFileNotInTempDir),
            LoadingImage::init(
                crate::alloc::Global,
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
            data: crate::alloc::heap_bytes(&path_bytes),
        };
        let mut loading = LoadingImage::init(
            crate::alloc::Global,
            &cmd,
            &Limits::all_with_temp_dir(&tmp_dir_realpath(&tmp_dir)),
        )
        .unwrap();
        let img = loading.complete(crate::alloc::Global).unwrap();
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
            data: crate::alloc::heap_bytes(&path_bytes),
        };
        let mut loading = LoadingImage::init(
            crate::alloc::Global,
            &cmd,
            &Limits::all_with_temp_dir(&tmp_dir_realpath(&tmp_dir)),
        )
        .unwrap();
        let img = loading.complete(crate::alloc::Global).unwrap();
        assert!(img.compression == command::Compression::None);
        tmp_dir.dir().access(&path).unwrap();
        tmp_dir.deinit();
    }

    // Parity S189-F2 (no reference test): `appendRemaining(.., .limited(n))`
    // fails with `error.StreamTooLong` once `n` bytes have been read, so a
    // stream of exactly `n` bytes fails as well.
    #[test]
    fn append_remaining_limited_matches_zig_limit_semantics() {
        let run = |input: &[u8], limit: usize| {
            let mut list = crate::alloc::heap_bytes(b"x");
            let mut reader = input;
            let r = append_remaining_limited(&mut reader, &mut list, limit);
            (
                r.map_err(|e| matches!(e, AppendRemainingError::StreamTooLong)),
                list.to_vec(),
            )
        };
        assert_eq!((Ok(()), b"xabc".to_vec()), run(b"abc", 4));
        assert_eq!(Err(true), run(b"abcd", 4).0);
        assert_eq!(Err(true), run(b"abcdef", 4).0);
        assert_eq!(Err(true), run(b"", 0).0);
    }

    /// A sparse file of `len` bytes (no disk space) in a fresh temp dir,
    /// loaded as a file-medium RGB transmission with no `s=` size.
    fn load_sparse_file_without_size(len: u64) -> Result<LoadingImage, Error> {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        tmp_dir.dir().writeFile("image.data", b"").unwrap();
        let path = tmp_dir.dir().realpath("image.data").unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(len)
            .unwrap();

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium: command::Medium::File,
                compression: command::Compression::None,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: crate::alloc::heap_bytes(&path_to_bytes(&path)),
        };
        let result = LoadingImage::init(
            crate::alloc::Global,
            &cmd,
            &Limits::all_with_temp_dir(&tmp_dir_realpath(&tmp_dir)),
        );
        tmp_dir.deinit();
        result
    }

    // Parity S189-F2 / UD140 (R13; no reference test): a file with no `s=`
    // size and EXACTLY `max_size` bytes loads, as it does with `s=`. The
    // reference's `.limited(max_size)` rejects it (reference defect). The
    // file is sparse, so it takes no disk space.
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_file_of_max_size_without_size_loads() {
        let result = load_sparse_file_without_size(MAX_SIZE as u64);
        let loading = match result {
            Ok(loading) => loading,
            Err(err) => panic!("a file of exactly max_size bytes must load: {err:?}"),
        };
        assert_eq!(MAX_SIZE, loading.data.len());
    }

    // Parity S189-F2 / UD140, the other side of the boundary: one byte more
    // than `max_size` is still `InvalidData`, rather than loaded truncated.
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_file_over_max_size_without_size_is_invalid_data() {
        let result = load_sparse_file_without_size(MAX_SIZE as u64 + 1);
        assert!(
            matches!(result, Err(Error::InvalidData)),
            "{:?}",
            result.as_ref().err()
        );
    }

    // Parity S189-F2 / UD140, the zlib path (NOT a reference test): at
    // 20abdb50a the reference's `decompressZlib` read the stream with
    // `.limited(max_size)` and rejected EXACTLY `max_size` output bytes.
    // `4b86e3594` fixed that upstream: `decompressZlib` now decodes through
    // `wuffs.zlib.decode`, which accepts exactly its limit (the upstream test
    // "zlib decode accepts exact limit and rejects truncated input",
    // `rustty-wuffs`). This keeps the pin on the kitty fixture, at a limit
    // equal to its decompressed length (the production call passes
    // `MAX_SIZE`) and one byte less, with no size hint as for a PNG payload.
    #[test]
    #[cfg_attr(miri, ignore = "calls the vendored Wuffs C decoder (R14)")]
    fn zlib_inflate_accepts_exactly_the_limit() {
        use rustty_wuffs::zlib::decode;
        let data = include_bytes!("testdata/image-rgb-zlib_deflate-128x96-2147483647-raw.data");
        let full = 128 * 96 * 3;
        let a = crate::alloc::Global;
        assert_eq!(full, decode(a, data, usize::MAX / 2, None).unwrap().len());
        assert_eq!(full, decode(a, data, full, None).unwrap().len());
        assert_eq!(
            Some(rustty_wuffs::Error::Overflow),
            decode(a, data, full - 1, None).err()
        );
    }

    // test "image load: rgb, not compressed, relative regular file"
    //
    // PORT NOTE: the reference's `testing.tmpDir` lives under the CWD-relative
    // `.zig-cache/tmp`, and this test's whole point is that `cmd.data` is a
    // RELATIVE path (`.zig-cache/tmp/{s}/image.data`) — the sibling
    // `image_load_rgb_not_compressed_regular_file` passes a realpath. The port
    // keeps the scratch under `target/` and asserts the CWD it is relative to,
    // so the path handed to the loader is relative exactly as the reference's
    // is.
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_rgb_not_compressed_relative_regular_file() {
        assert_eq!(
            std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap(),
            std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap(),
            "this test needs the cargo-set CWD to build a relative path from"
        );
        let rel_dir = format!("../../target/kitty-relative-scratch-{}", std::process::id());
        std::fs::create_dir_all(&rel_dir).unwrap();
        let rel_path = format!("{rel_dir}/image.data");
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        std::fs::write(&rel_path, data).unwrap();

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
            data: crate::alloc::heap_bytes(rel_path.as_bytes()),
        };

        // The scratch directory is removed on EVERY exit, including a panic —
        // a bare `remove_dir_all` at the end leaks it whenever an assertion
        // below fails, and this one lives under `target/`, where it would then
        // persist across runs.
        struct Scratch(String);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(rel_dir.clone());

        let mut loading = LoadingImage::init(
            crate::alloc::Global,
            &cmd,
            &Limits {
                file: true,
                temporary_file: TemporaryFileLimit::Disabled,
                shared_memory: false,
            },
        )
        .unwrap();
        let img = loading.complete(crate::alloc::Global).unwrap();
        assert!(img.compression == command::Compression::None);
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
                data: crate::alloc::heap_bytes(&path_bytes.clone()),
            };

            let mut loading = LoadingImage::init(
                crate::alloc::Global,
                &cmd,
                &Limits {
                    file: true,
                    temporary_file: TemporaryFileLimit::Disabled,
                    shared_memory: false,
                },
            )
            .unwrap();
            let img = loading.complete(crate::alloc::Global).unwrap();

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
            data: crate::alloc::heap_bytes(&path_bytes),
        };

        assert_eq!(
            Err(Error::InvalidData),
            LoadingImage::init(
                crate::alloc::Global,
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

        // var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        let mut path_buf = vec![0u8; MAX_PATH_BYTES];
        assert_eq!(
            Err(ValidatedFilePathError::InvalidData),
            LoadingImage::validatedFilePath(&blocked_file, &mut path_buf).map(|_| ())
        );

        // The pathname now resolves to the safe replacement, demonstrating that
        // the rejection above came from the already-open file handle.
        let safe_file = std::fs::File::open(base.join("image.data")).unwrap();
        LoadingImage::validatedFilePath(&safe_file, &mut path_buf).unwrap();
        // (blocked_file/safe_file close on drop; the reference defers both.)
        drop(blocked_file);
        tmp_dir.deinit();
    }

    // The seven `image load: windows …` tests (`0b5446364`). Each reference
    // test starts with `if (builtin.os.tag != .windows) return
    // error.SkipZigTest;`, mapped to `#[cfg(windows)]` (R14); they run in the
    // Windows lane (`cargo test -p rustty-vt kitty` on rwin).

    /// `LoadingImage.init(io, alloc, &cmd, limits)` for a `path` transmitted
    /// with `medium`, as the Windows tests build it (rgb, 20x15, id 31).
    #[cfg(windows)]
    fn windows_load(
        path: &[u8],
        medium: command::Medium,
        limits: &Limits,
    ) -> Result<LoadingImage, Error> {
        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Rgb,
                medium,
                compression: command::Compression::None,
                width: 20,
                height: 15,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            // .data = try alloc.dupe(u8, path),
            data: crate::alloc::heap_bytes(path),
        };
        LoadingImage::init(crate::alloc::Global, &cmd, limits)
    }

    /// `.{ .file = true, .temporary_file = .disabled, .shared_memory = false }`
    #[cfg(windows)]
    const WINDOWS_FILE_ONLY: Limits = Limits {
        file: true,
        temporary_file: TemporaryFileLimit::Disabled,
        shared_memory: false,
    };

    // test "image load: windows UNC path is rejected before open"
    #[cfg(windows)]
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS filesystem syscalls
    fn image_load_windows_unc_path_is_rejected_before_open() {
        // Opening a UNC path would resolve the host name and authenticate to
        // it over SMB, so the rejection has to happen on the raw path. The
        // host name is in the reserved .invalid TLD so it can never resolve.
        let paths: [&[u8]; 3] = [
            b"\\\\nonexistent-host.invalid\\share\\tty-graphics-protocol-image.data",
            b"//nonexistent-host.invalid/share/tty-graphics-protocol-image.data",
            b"\\\\?\\UNC\\nonexistent-host.invalid\\share\\image.data",
        ];
        for path in paths {
            assert_eq!(
                Some(Error::InvalidData),
                windows_load(path, command::Medium::File, &WINDOWS_FILE_ONLY).err(),
                "{}",
                String::from_utf8_lossy(path)
            );
        }
    }

    // test "image load: windows device namespace paths are rejected before open"
    #[cfg(windows)]
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_windows_device_namespace_paths_are_rejected_before_open() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let filename = "tty-graphics-protocol-image.data";
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        tmp_dir.dir().writeFile(filename, data).unwrap();

        let dir_path = tmp_dir_realpath(&tmp_dir);
        let real_path = path_to_bytes(&tmp_dir.dir().realpath(filename).unwrap());

        // The verbatim, local device and NT namespace spellings all resolve
        // to the same real file, so rejecting them proves the check runs on
        // the raw path rather than on the canonical one.
        let prefixes: [&[u8]; 3] = [b"\\\\?\\", b"\\\\.\\", b"\\??\\"];
        for prefix in prefixes {
            // const path = try std.mem.concat(alloc, u8, &.{ prefix, real_path });
            let path = [prefix, &real_path[..]].concat();

            let mediums = [command::Medium::File, command::Medium::TemporaryFile];
            for medium in mediums {
                assert_eq!(
                    Some(Error::InvalidData),
                    windows_load(&path, medium, &Limits::all_with_temp_dir(&dir_path)).err(),
                    "{} {medium:?}",
                    String::from_utf8_lossy(&path)
                );
            }
        }

        // A named pipe reached through the local device namespace.
        {
            assert_eq!(
                Some(Error::InvalidData),
                windows_load(
                    b"\\\\.\\pipe\\rustty-kitty-graphics-test",
                    command::Medium::File,
                    &Limits::all_with_temp_dir(&dir_path),
                )
                .err()
            );
        }

        // Nothing above reached the temporary file deletion.
        tmp_dir
            .dir()
            .access(std::path::Path::new(filename))
            .unwrap();
        tmp_dir.deinit();
    }

    // test "image load: windows reserved device names are rejected before open"
    #[cfg(windows)]
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_windows_reserved_device_names_are_rejected_before_open() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let dir_path = tmp_dir_realpath(&tmp_dir);

        let names: [&[u8]; 6] = [b"CON", b"NUL.data", b"com1", b"LPT9.png", b"AUX", b"PRN."];
        for name in names {
            // const path = try std.fs.path.join(alloc, &.{ dir_path, name });
            let path = [&dir_path[..], b"\\", name].concat();

            assert_eq!(
                Some(Error::InvalidData),
                windows_load(&path, command::Medium::File, &WINDOWS_FILE_ONLY).err(),
                "{}",
                String::from_utf8_lossy(&path)
            );
        }
        tmp_dir.deinit();
    }

    // test "image load: windows local file accepted in forward slash and upper case spellings"
    #[cfg(windows)]
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_windows_local_file_accepted_in_forward_slash_and_upper_case_spellings() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let filename = "image.data";
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        tmp_dir.dir().writeFile(filename, data).unwrap();

        let real_path = path_to_bytes(&tmp_dir.dir().realpath(filename).unwrap());

        // Both spellings open the same file; the canonical path the checks
        // see is the backslash, on-disk case form either way.
        // std.mem.replaceScalar(u8, forward, '\\', '/');
        let forward: Vec<u8> = real_path
            .iter()
            .map(|&c| if c == b'\\' { b'/' } else { c })
            .collect();
        // _ = std.ascii.upperString(upper, real_path);
        let upper = real_path.to_ascii_uppercase();

        let spellings = [forward, upper];
        for path in &spellings {
            let mut loading =
                windows_load(path, command::Medium::File, &WINDOWS_FILE_ONLY).unwrap();
            let img = loading.complete(crate::alloc::Global).unwrap();
            assert!(img.compression == command::Compression::None);
        }

        tmp_dir
            .dir()
            .access(std::path::Path::new(filename))
            .unwrap();
        tmp_dir.deinit();
    }

    // test "image load: windows temporary file with differently spelled directory"
    #[cfg(windows)]
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_windows_temporary_file_with_differently_spelled_directory() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let filename = "tty-graphics-protocol-image.data";
        let data = include_bytes!("testdata/image-rgb-none-20x15-2147483647-raw.data");
        tmp_dir.dir().writeFile(filename, data).unwrap();

        let dir_path = tmp_dir_realpath(&tmp_dir);
        let real_path = path_to_bytes(&tmp_dir.dir().realpath(filename).unwrap());

        // Hosts hand over GetTempPath output, which regularly differs from
        // the canonical path in case (`C:\WINDOWS\TEMP\`), so the directory
        // prefix check only passes through the real-path fallback.
        let dir_upper = dir_path.to_ascii_uppercase();
        let path_upper = real_path.to_ascii_uppercase();
        assert!(!isPathInDir(&dir_upper, &real_path) || dir_upper == dir_path);

        let mut loading = windows_load(
            &path_upper,
            command::Medium::TemporaryFile,
            &Limits::all_with_temp_dir(&dir_upper),
        )
        .unwrap();
        let img = loading.complete(crate::alloc::Global).unwrap();
        assert!(img.compression == command::Compression::None);

        // Temporary file should be gone
        assert_eq!(
            std::io::ErrorKind::NotFound,
            tmp_dir
                .dir()
                .access(std::path::Path::new(filename))
                .unwrap_err()
                .kind()
        );
        tmp_dir.deinit();
    }

    // test "image load: windows canonical path check accepts a local file opened through a device spelling"
    #[cfg(windows)]
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_windows_canonical_path_check_accepts_a_local_file_opened_through_a_device_spelling(
    ) {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let filename = "image.data";
        tmp_dir.dir().writeFile(filename, b"safe").unwrap();

        let real_path = path_to_bytes(&tmp_dir.dir().realpath(filename).unwrap());

        // readFile refuses this spelling before the open, but the post-open
        // check works from the canonical path and so does not care how the
        // file was reached: `\\.\C:\...` canonicalizes back to `C:\...`.
        let device_path = [&b"\\\\.\\"[..], &real_path[..]].concat();
        let file = open_file(&device_path).unwrap();

        // const canon_buf = try alloc.alloc(u8, std.fs.max_path_bytes);
        let mut canon_buf = vec![0u8; MAX_PATH_BYTES];
        let canon = LoadingImage::validatedFilePath(&file, &mut canon_buf).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&real_path),
            String::from_utf8_lossy(canon)
        );
        drop(file);
        tmp_dir.deinit();
    }

    // test "image load: windows canonical path check rejects a file reached through a UNC share"
    #[cfg(windows)]
    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    fn image_load_windows_canonical_path_check_rejects_a_file_reached_through_a_unc_share() {
        let mut tmp_dir = temp_dir::TempDir::init().unwrap();
        let filename = "image.data";
        tmp_dir.dir().writeFile(filename, b"safe").unwrap();

        let real_path = path_to_bytes(&tmp_dir.dir().realpath(filename).unwrap());
        assert!(kitty_windows::is_drive_absolute(&real_path));

        // Reach the same local file through the loopback administrative
        // share, which is how a junction or symlink into a share would look
        // to the post-open check. The share needs the server service and an
        // administrative token, so the test is skipped when the open fails.
        let unc_path = format!(
            "\\\\localhost\\{}$\\{}",
            real_path[0] as char,
            String::from_utf8_lossy(&real_path[3..])
        );
        // const file = std.Io.Dir.cwd().openFile(io, unc_path, .{}) catch
        //     return error.SkipZigTest;
        //
        // PORT NOTE (R14): a runtime skip, printed, since only the open's
        // outcome decides it.
        let file = match open_file(unc_path.as_bytes()) {
            Ok(f) => f,
            Err(err) => {
                eprintln!(
                    "SKIP image_load_windows_canonical_path_check_rejects_a_file_reached_through_a_unc_share: \
                     cannot open {unc_path} ({err}); the loopback administrative share needs the \
                     server service and an administrative token (R14)"
                );
                tmp_dir.deinit();
                return;
            }
        };

        let mut canon_buf = vec![0u8; MAX_PATH_BYTES];
        assert_eq!(
            Err(ValidatedFilePathError::Windows(
                kitty_windows::Error::NotDriveAbsolute
            )),
            LoadingImage::validatedFilePath(&file, &mut canon_buf).map(|_| ())
        );
        drop(file);
        tmp_dir.deinit();
    }

    #[test]
    #[cfg_attr(miri, ignore)] // real-OS temp-file filesystem syscalls
    #[cfg_attr(
        not(feature = "wuffs"),
        ignore = "SkipZigTest: no PNG decoder without the wuffs feature (R14)"
    )]
    fn image_load_png_not_compressed_regular_file() {
        // PORT NOTE: `if (sys.decode_png == null) return error.SkipZigTest;`. The
        // decoder seam is installed exactly when the `wuffs` feature is on
        // (`sys::tests::default_decoder_matches_artifact_and_feature`), so the
        // skip is the `cfg_attr(.., ignore)` above and a missing decoder here is
        // a failure, not an early-return `ok` (R14).
        let _png_lock = sys::decode_png_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert!(
            sys::decode_png.read().unwrap().is_some(),
            "the wuffs feature is on but no PNG decoder is installed"
        );

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
            data: crate::alloc::heap_bytes(&path_bytes),
        };
        let mut loading = LoadingImage::init(
            crate::alloc::Global,
            &cmd,
            &Limits::all_with_temp_dir(&tmp_dir_realpath(&tmp_dir)),
        )
        .unwrap();
        let img = loading.complete(crate::alloc::Global).unwrap();
        assert!(img.compression == command::Compression::None);
        assert!(img.format == command::Format::Rgba);
        tmp_dir.dir().access(&path).unwrap();
        tmp_dir.deinit();
    }

    // test "image load: png, zlib compressed, direct" (`4b86e3594`)
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md): Miri cannot call the vendored Wuffs zlib decoder (C FFI, R14) -- foreign function; covered natively + by ASan"
    )]
    #[cfg_attr(
        not(feature = "wuffs"),
        ignore = "SkipZigTest: no PNG decoder without the wuffs feature (R14)"
    )]
    fn image_load_png_zlib_compressed_direct() {
        // if (sys.decode_png == null) return error.SkipZigTest;
        //
        // PORT NOTE: as in `image_load_png_not_compressed_regular_file`: the
        // skip is the `cfg_attr(.., ignore)` above, and a missing decoder here
        // is a failure (R14).
        let _png_lock = sys::decode_png_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        assert!(
            sys::decode_png.read().unwrap().is_some(),
            "the wuffs feature is on but no PNG decoder is installed"
        );

        // const data = @embedFile("testdata/image-png-zlib_deflate-50x76-2147483647-raw.data");
        let data = include_bytes!("testdata/image-png-zlib_deflate-50x76-2147483647-raw.data");

        let cmd: command::Command = command::Command {
            control: command::Control::Transmit(command::Transmission {
                format: command::Format::Png,
                medium: command::Medium::Direct,
                compression: command::Compression::ZlibDeflate,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: crate::alloc::heap_bytes(data),
        };
        let mut loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();
        let img = loading.complete(crate::alloc::Global).unwrap();

        // try testing.expectEqual(command.Transmission.Compression.none, img.compression);
        assert_eq!(command::Compression::None, img.compression);
        // try testing.expectEqual(command.Transmission.Format.rgba, img.format);
        assert_eq!(command::Format::Rgba, img.format);
        // try testing.expectEqual(@as(u32, 50), img.width);
        assert_eq!(50u32, img.width);
        // try testing.expectEqual(@as(u32, 76), img.height);
        assert_eq!(76u32, img.height);
    }

    // NOT a reference test (P7-W5 T2): `4b86e3594` changed `decompressZlib`'s
    // error mapping so an allocation failure is `error.OutOfMemory`
    // (`error.OutOfMemory => return error.OutOfMemory`), where the stream reader
    // before it reported every failure as `DecompressionFailed`. Allocation 0
    // is the payload copy; allocation 1, the first of the decode (the decoder
    // storage), is refused.
    #[test]
    #[cfg_attr(miri, ignore = "calls the vendored Wuffs C decoder (R14)")]
    fn image_load_zlib_decompression_propagates_out_of_memory() {
        let failing =
            crate::alloc::testing::FailingAllocator::over_heap(crate::alloc::testing::Config {
                fail_index: 1,
                ..crate::alloc::testing::Config::default()
            });
        let failing_c = failing.allocator();
        let compressed =
            include_bytes!("testdata/image-rgb-zlib_deflate-128x96-2147483647-raw.data");
        crate::alloc::testing::with_handle(&failing_c, |alloc| {
            let mut loading = LoadingImage {
                data: crate::alloc::try_dupe(alloc, compressed).unwrap(),
                ..shm_loading(128, 96, command::Format::Rgb)
            };
            loading.image.compression = command::Compression::ZlibDeflate;
            assert_eq!(Err(Error::OutOfMemory), loading.decompress(alloc));
            assert!(failing.has_induced_failure.get());
            assert_eq!(command::Compression::ZlibDeflate, loading.image.compression);
            loading.deinit(alloc);
        });
    }

    // test "image load: png rejects oversized decoder allocation"
    //
    // PORT NOTE (parity tail S7-4): ported as written. The decode seam's
    // `LimitedAllocator` wraps the allocator `complete(alloc)` is given, so
    // the testing `FailingAllocator` sits under the limiter exactly as in the
    // reference.
    #[test]
    fn image_load_png_rejects_oversized_decoder_allocation() {
        use rustty_datastruct::limited_allocator::RawAllocator;

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
            Ok(sys::Image {
                width: 1,
                height: 1,
                data,
            })
        }

        let _png_lock = sys::decode_png_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // const original_decode_png = sys.decode_png;
        // defer sys.decode_png = original_decode_png;
        // sys.decode_png = &oversized_decoder;
        let original_decode_png = *sys::decode_png.read().unwrap();
        *sys::decode_png.write().unwrap() = Some(oversized_decoder);

        // Fail any allocation which reaches the underlying allocator. The size
        // limiter should reject the decoder's request before it gets that far.
        // var failing = testing.FailingAllocator.init(testing.allocator, .{
        //     .fail_index = 0,
        // });
        // const alloc = failing.allocator();
        let failing =
            crate::alloc::testing::FailingAllocator::over_heap(crate::alloc::testing::Config {
                fail_index: 0,
                ..crate::alloc::testing::Config::default()
            });
        let failing_c = failing.allocator();
        crate::alloc::testing::with_handle(&failing_c, |alloc| {
            // var loading: LoadingImage = .{
            //     .image = .{ .format = .png },
            //     .quiet = .no,
            //     .temporary_directory = null,
            // };
            // defer loading.deinit(alloc);
            let mut loading = LoadingImage {
                data: crate::alloc::Bytes::new_in(alloc),
                ..shm_loading(0, 0, command::Format::Png)
            };

            // try testing.expectError(error.InvalidData, loading.complete(alloc));
            assert_eq!(Err(Error::InvalidData), loading.complete(alloc).map(|_| ()));
            loading.deinit(alloc);
        });
        // try testing.expect(!failing.has_induced_failure);
        assert!(!failing.has_induced_failure.get());

        *sys::decode_png.write().unwrap() = original_decode_png;
    }

    // Parity tail S7-4 (routing evidence): the PNG decoder allocates through
    // the `LimitedAllocator` whose child is the allocator `complete` is given
    // (`LimitedAllocator.init(alloc, max_size)`), and the decoded pixels are
    // copied into data allocated through that same allocator. S7-4b: the copy
    // is taken straight from the decoder's buffer, which is freed through the
    // decode allocator only afterwards (`defer decode_alloc.free`), so both
    // are live on the handle at once. The input (3 bytes) is freed before the
    // copy (`self.data.deinit(alloc)`), so the peak is decoded 4 + copy 4 = 8;
    // a copy out of the seam first would peak at input 3 + decoded 4 = 7.
    #[test]
    fn image_load_png_decode_routes_through_the_allocator_handle() {
        use rustty_datastruct::limited_allocator::RawAllocator;
        fn small_decoder(
            alloc: &dyn RawAllocator,
            _data: &[u8],
        ) -> core::result::Result<sys::Image, sys::DecodeError> {
            // const data = try alloc.alloc(u8, 4);
            let data = alloc
                .alloc_bytes(4)
                .map_err(|_| sys::DecodeError::OutOfMemory)?;
            // SAFETY: a live 4-byte allocation from `alloc`.
            unsafe { core::ptr::write_bytes(data.ptr, 9, 4) };
            Ok(sys::Image {
                width: 1,
                height: 1,
                data,
            })
        }

        let _png_lock = sys::decode_png_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let original_decode_png = *sys::decode_png.read().unwrap();
        *sys::decode_png.write().unwrap() = Some(small_decoder);

        let counting = crate::alloc::testing::CountingAllocator::over_heap();
        let c = counting.allocator();
        crate::alloc::testing::with_handle(&c, |alloc| {
            let mut loading = LoadingImage {
                data: crate::alloc::try_dupe(alloc, b"png").unwrap(),
                ..shm_loading(0, 0, command::Format::Png)
            };
            let c0 = counting.alloc_calls.get();
            let mut img = loading.complete(alloc).unwrap();
            // The decoder's 4-byte buffer (through the limiter, then freed)
            // and the image data it is copied into.
            assert_eq!(c0 + 2, counting.alloc_calls.get());
            assert_eq!(Some((4, 0)), counting.last_alloc.get());
            assert_eq!(Some(&[9u8, 9, 9, 9][..]), img.data.bytes());
            assert_eq!(
                8,
                counting.peak_bytes.get(),
                "decoder buffer and copy both live"
            );
            img.deinit(alloc);
            loading.deinit(alloc);
        });
        assert_eq!(0, counting.live_bytes.get(), "every byte is returned");

        // The copy is fallible: the decoder's allocation succeeds, the copy
        // into `alloc` fails, and `complete` answers OutOfMemory with every
        // byte returned (the decoder's buffer through the decode allocator).
        let failing =
            crate::alloc::testing::FailingAllocator::over_heap(crate::alloc::testing::Config {
                fail_index: 2,
                ..crate::alloc::testing::Config::default()
            });
        let fc = failing.allocator();
        crate::alloc::testing::with_handle(&fc, |alloc| {
            let mut loading = LoadingImage {
                data: crate::alloc::try_dupe(alloc, b"png").unwrap(),
                ..shm_loading(0, 0, command::Format::Png)
            };
            assert_eq!(Err(Error::OutOfMemory), loading.complete(alloc).map(|_| ()));
            loading.deinit(alloc);
        });
        assert!(failing.has_induced_failure.get());
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());

        *sys::decode_png.write().unwrap() = original_decode_png;
    }

    // test "image load: png rejects oversized Wuffs image before allocation"
    #[test]
    #[cfg_attr(
        not(feature = "wuffs"),
        ignore = "SkipZigTest: no PNG decoder without the wuffs feature (R14)"
    )]
    fn image_load_png_rejects_oversized_wuffs_image_before_allocation() {
        let _png_lock = sys::decode_png_test_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // if (sys.decode_png == null) return error.SkipZigTest;
        // (that skip is the `cfg_attr(.., ignore)` above: the decoder is
        // installed exactly under `wuffs`; R14)
        assert!(
            sys::decode_png.read().unwrap().is_some(),
            "the wuffs feature is on but no PNG decoder is installed"
        );

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
            data: crate::alloc::heap_bytes(&data),
        };
        let mut loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();

        assert_eq!(
            Err(Error::InvalidData),
            loading.complete(crate::alloc::Global).map(|_| ())
        );
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
            data: crate::alloc::heap_bytes(b"AAAA"),
        };

        // Direct medium should work even with the most restrictive limits
        let _loading = LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).unwrap();
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
            data: crate::alloc::heap_bytes(&path_bytes),
        };
        assert_eq!(
            Err(Error::UnsupportedMedium),
            LoadingImage::init(crate::alloc::Global, &cmd, &Limits::DIRECT).map(|_| ())
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
            data: crate::alloc::heap_bytes(&path_bytes),
        };
        let _loading = LoadingImage::init(
            crate::alloc::Global,
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
            data: crate::alloc::heap_bytes(&path_bytes),
        };
        assert_eq!(
            Err(Error::UnsupportedMedium),
            LoadingImage::init(
                crate::alloc::Global,
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
            data: crate::alloc::heap_bytes(&path_bytes),
        };
        let _loading = LoadingImage::init(
            crate::alloc::Global,
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
