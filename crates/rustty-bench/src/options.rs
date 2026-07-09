//! This file contains helpers for CLI options.

// const std = @import("std");

use std::io::{self, Read};

/// A data source opened by [`data_file`]: either stdin or a file handle. This is
/// the Rust map of the reference's `std.fs.File` return (which may be
/// `File.stdin()` or an `openFile` handle). The `errdefer file.close()` in the
/// reference is Rust's automatic drop.
pub enum DataFile {
    // if (std.mem.eql(u8, path, "-")) return .stdin();
    Stdin(io::Stdin),
    // const file = try std.fs.cwd().openFile(path, .{});
    File(std::fs::File),
}

impl Read for DataFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            DataFile::Stdin(s) => s.read(buf),
            DataFile::File(f) => f.read(buf),
        }
    }
}

/// Returns the data file for the given path in a way that is consistent
/// across our CLI. If the path is not set then no file is returned.
/// If the path is "-", then we will return stdin. If the path is
/// a file then we will open and return the handle.
// pub fn dataFile(path_: ?[]const u8) !?std.fs.File {
//
// PORT NOTE (§8): the path is bytes (`[]const u8`); we form an `OsStr` from the
// bytes (`OsStrExt`) rather than lossily converting to `String`, faithful to the
// Zig byte-slice path.
pub fn data_file(path_: Option<&[u8]>) -> io::Result<Option<DataFile>> {
    // const path = path_ orelse return null;
    let path = match path_ {
        Some(p) => p,
        None => return Ok(None),
    };

    // Stdin
    // if (std.mem.eql(u8, path, "-")) return .stdin();
    if path == b"-" {
        return Ok(Some(DataFile::Stdin(io::stdin())));
    }

    // Normal file
    // const file = try std.fs.cwd().openFile(path, .{});
    // errdefer file.close();
    #[cfg(unix)]
    let os_path: &std::ffi::OsStr = {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(path)
    };
    #[cfg(not(unix))]
    let os_path: std::ffi::OsString = {
        // On non-Unix the byte-slice path is decoded as UTF-8 (lossless for the
        // ASCII paths the benchmark uses); the Unix arm above is the faithful
        // byte-exact path.
        std::ffi::OsString::from(String::from_utf8_lossy(path).into_owned())
    };
    let file = std::fs::File::open(os_path)?;

    // return file;
    Ok(Some(DataFile::File(file)))
}

/// A cache-line-aligned 4096-byte scratch buffer.
///
/// PORT NOTE (§8): reproduces `[4096]u8 align(std.atomic.cache_line)`. `64` is
/// the `std.atomic.cache_line` value on the port's targets (x86-64 / aarch64).
/// The alignment is a perf-fidelity detail, not correctness; the non-aligned
/// `[4096]u8` scratch buffers stay plain arrays.
#[repr(align(64))]
pub struct CacheLineBuf(pub [u8; 4096]);

impl CacheLineBuf {
    pub fn new() -> CacheLineBuf {
        CacheLineBuf([0u8; 4096])
    }
}

impl Default for CacheLineBuf {
    fn default() -> Self {
        CacheLineBuf::new()
    }
}

/// The reference's `reader.readSliceShort(&buf)`: read into `buf`, returning the
/// number of bytes read; a short read (`n < buf.len`) happens only at EOF, and
/// `n == 0` means EOF was reached. This loops `Read::read` to fill `buf` until it
/// is full or the source is exhausted, matching that "n==0 ⇒ break" contract.
pub fn read_slice_short<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        let got = r.read(&mut buf[n..])?;
        if got == 0 {
            break;
        }
        n += got;
    }
    Ok(n)
}
