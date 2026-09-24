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
    //
    // PERF(port): the reference reads through `f.reader(&read_buf)` — a
    // BUFFERED std.Io.File.Reader. The port read straight through, paying one
    // read(2) per 4 KiB `read_slice_short` (16384 syscalls on a 64 MiB
    // corpus, ~10 ms measured). 256 KiB is an internal choice (R10): the
    // bytes delivered are identical, only the syscall count changes.
    File(io::BufReader<std::fs::File>),
}

impl Read for DataFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            DataFile::Stdin(s) => s.read(buf),
            DataFile::File(f) => f.read(buf),
        }
    }
}

impl DataFile {
    /// The reference's `f.reader(io, &.{})`: an UNBUFFERED reader over the
    /// data file (the terminal-stream bench, `TerminalStream.zig` `step`).
    ///
    /// PORT NOTE: a file opened by [`data_file`] sits behind the 256 KiB
    /// `BufReader` above, which the bench's 64 KiB `read_slice_short` requests
    /// do NOT bypass (`BufReader` only skips its buffer for reads at least as
    /// large as its capacity), so every byte was copied buffer-to-buffer —
    /// the per-chunk memcpy the reference removed. This returns the inner
    /// `File` instead. The buffer must be empty (nothing read through it yet),
    /// or bytes it holds would be skipped. Stdin keeps `io::Stdin`, whose
    /// process-wide buffer Rust does not let a reader opt out of.
    pub fn unbuffered(&mut self) -> &mut dyn Read {
        match self {
            DataFile::Stdin(s) => s,
            DataFile::File(f) => {
                assert!(f.buffer().is_empty());
                f.get_mut()
            }
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
    Ok(Some(DataFile::File(io::BufReader::with_capacity(
        256 * 1024,
        file,
    ))))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// rustty-only (task S-4): `DataFile::unbuffered` is the reference's
    /// `f.reader(io, &.{})` for the terminal-stream bench. It must deliver the
    /// file's bytes exactly, and must NOT route them through the `BufReader`
    /// (whose buffer a 64 KiB request does not bypass): after each read
    /// through it, nothing is staged in the buffer.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "writes and opens a temp file; Miri isolation forbids `open`"
    )]
    fn unbuffered_reads_the_file_directly() {
        use std::io::Write as _;

        let mut path = std::env::temp_dir();
        path.push(format!(
            "rustty-s4-unbuffered-{}-{:?}.bin",
            std::process::id(),
            std::thread::current().id()
        ));
        // 3 x 64 KiB + 5 bytes, distinct per position.
        let data: Vec<u8> = (0..3 * 64 * 1024 + 5).map(|i| (i % 251) as u8).collect();
        std::fs::File::create(&path)
            .unwrap()
            .write_all(&data)
            .unwrap();

        let bytes = {
            use std::os::unix::ffi::OsStrExt;
            path.as_os_str().as_bytes().to_vec()
        };
        let mut f = data_file(Some(&bytes)).unwrap().unwrap();
        let mut got = Vec::new();
        let mut buf = [0u8; 64 * 1024];
        let mut reads = 0;
        loop {
            let mut r = f.unbuffered();
            let n = read_slice_short(&mut r, &mut buf).unwrap();
            if n == 0 {
                break;
            }
            reads += 1;
            got.extend_from_slice(&buf[..n]);
            // The BufReader was bypassed: nothing is staged in it.
            match &f {
                DataFile::File(b) => {
                    assert!(
                        b.buffer().is_empty(),
                        "read {reads} went through the buffer"
                    )
                }
                DataFile::Stdin(_) => unreachable!(),
            }
        }
        assert_eq!(4, reads);
        assert_eq!(data, got);
        let _ = std::fs::remove_file(&path);
    }
}
