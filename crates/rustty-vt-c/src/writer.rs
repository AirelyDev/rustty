//! rustty-vt-c writer adaptors
//!
//! PORT NOTE (Rust-only glue, R1 "no-loss, not no-addition"): the reference C
//! shims encode into a `std.Io.Writer` and distinguish two sinks:
//!
//!   - `std.Io.Writer.fixed(buf)` — writes into a caller buffer and returns
//!     `error.WriteFailed` the moment the buffer would overflow.
//!   - `std.Io.Writer.Discarding` — discards bytes but counts how many were
//!     written (`.count`), used to compute the required size for `out_of_space`.
//!
//! The rustty-vt encoders are generic over `core::fmt::Write` and return
//! `core::fmt::Result` (a write either fully succeeds or returns `Err`). These
//! two adaptors reproduce the SAME two sinks exactly: `FixedWriter` errors on
//! the first byte that would overflow `buf` (mirroring `WriteFailed`) and tracks
//! `end` (bytes written), and `CountingWriter` never fails and tracks `count`.
//! No reference logic/behavior is changed; this is the minimal glue that lets
//! the ported encoders drive the C out-buffer protocol.

use core::fmt::Write;

/// Mirrors `std.Io.Writer.fixed(buf)`: writes into `buf`, returning
/// `core::fmt::Error` (the rustty analog of `error.WriteFailed`) as soon as a
/// write would overflow. `end` is the number of bytes written so far.
pub struct FixedWriter<'a> {
    buf: &'a mut [u8],
    pub end: usize,
}

impl<'a> FixedWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> FixedWriter<'a> {
        FixedWriter { buf, end: 0 }
    }
}

impl Write for FixedWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        // WriteFailed the moment the buffer would overflow.
        if self.end + bytes.len() > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.end..self.end + bytes.len()].copy_from_slice(bytes);
        self.end += bytes.len();
        Ok(())
    }
}

/// Mirrors `std.Io.Writer.Discarding`: discards all bytes but counts how many
/// were written (`count`). Never fails.
#[derive(Default)]
pub struct CountingWriter {
    pub count: usize,
}

impl CountingWriter {
    pub fn new() -> CountingWriter {
        CountingWriter { count: 0 }
    }
}

impl Write for CountingWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.count += s.len();
        Ok(())
    }
}
