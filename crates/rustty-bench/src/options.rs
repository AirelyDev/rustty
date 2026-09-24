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
///
/// An interrupted read (`EINTR`) is retried: Zig 0.16's file reader loops on
/// `.INTR` inside `fileReadStreamingPosix` (`std/Io/Threaded.zig:9641-9644`),
/// so it never reaches `readSliceShort`'s caller (parity S62-F6).
pub fn read_slice_short<R: Read>(r: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        let got = match r.read(&mut buf[n..]) {
            Ok(got) => got,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if got == 0 {
            break;
        }
        n += got;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::read_slice_short;
    use std::io::{self, Read};

    /// Returns `Interrupted` before every chunk, then the chunk.
    struct Interrupting<'a> {
        chunks: &'a [&'a [u8]],
        interrupt_next: bool,
        interrupts: usize,
    }

    impl Read for Interrupting<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let (first, rest) = match self.chunks.split_first() {
                Some(split) => split,
                None => return Ok(0),
            };
            if self.interrupt_next {
                self.interrupt_next = false;
                self.interrupts += 1;
                return Err(io::ErrorKind::Interrupted.into());
            }
            let n = first.len().min(buf.len());
            buf[..n].copy_from_slice(&first[..n]);
            self.chunks = rest;
            self.interrupt_next = true;
            Ok(n)
        }
    }

    /// Parity S62-F6: `EINTR` is retried, not a read failure.
    #[test]
    fn read_slice_short_retries_interrupted_reads() {
        let mut r = Interrupting {
            chunks: &[b"abc", b"de"],
            interrupt_next: true,
            interrupts: 0,
        };
        let mut buf = [0u8; 8];
        let n = read_slice_short(&mut r, &mut buf).unwrap();
        assert_eq!(b"abcde", &buf[..n]);
        // Both chunks were preceded by an interrupted read.
        assert_eq!(2, r.interrupts);
    }

    /// Any other error still fails the read.
    #[test]
    fn read_slice_short_propagates_other_errors() {
        struct Failing;
        impl Read for Failing {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
        }
        let err = read_slice_short(&mut Failing, &mut [0u8; 4]).unwrap_err();
        assert_eq!(io::ErrorKind::BrokenPipe, err.kind());
    }
}
