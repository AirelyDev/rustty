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
//! `core::fmt::Result`. These two adaptors reproduce the SAME two sinks
//! exactly: `FixedWriter` fills `buf` as far as it goes and then reports
//! `WriteFailed` (see the byte-level note on `FixedWriter`), tracking `end`
//! (bytes written), and `CountingWriter` never fails and tracks `count`.
//! No reference logic/behavior is changed; this is the minimal glue that lets
//! the ported encoders drive the C out-buffer protocol.

use core::fmt::Write;

/// Mirrors `std.Io.Writer.fixed(buf)`: writes into `buf`, returning
/// `core::fmt::Error` (the rustty analog of `error.WriteFailed`) when a write
/// would overflow. `end` is the number of bytes written so far.
///
/// PORT NOTE (W81-B01-01): `std.Io.Writer.fixed(buf)` makes the CALLER buffer
/// the writer's own internal buffer, so an oversized `writeAll` copies the
/// FITTING PREFIX into it (advancing `end` to `buf.len()`) and only then fails
/// with `error.WriteFailed`. Probed on zig 0.16.0 (ghostty's pinned
/// `minimum_zig_version`):
///
/// ```text
/// var buf = [_]u8{ 'X', 'Y' };
/// var w = std.Io.Writer.fixed(buf[0..]);
/// w.writeAll("abc") => result=error.WriteFailed end=2 buf=ab
/// ```
///
/// This adaptor reproduces that byte-for-byte: copy what fits, then return
/// `Err`. (It previously rejected an overflowing write WHOLE, leaving the
/// caller buffer untouched — observable through the C ABI after
/// `RUSTTY_OUT_OF_SPACE`.) The error is still returned for the whole write, so
/// the encoders' `catch` path and their required-length recount through
/// `CountingWriter` are unchanged.
pub struct FixedWriter<'a> {
    buf: &'a mut [u8],
    pub end: usize,
}

impl<'a> FixedWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> FixedWriter<'a> {
        FixedWriter { buf, end: 0 }
    }

    /// Copy the fitting prefix of `bytes` into `buf`, returning `true` when the
    /// whole write fit. Mirrors `std.Io.Writer.fixed`'s drain: the prefix lands
    /// in the caller buffer either way.
    fn write_prefix(&mut self, bytes: &[u8]) -> bool {
        let avail = self.buf.len() - self.end;
        if bytes.len() <= avail {
            self.buf[self.end..self.end + bytes.len()].copy_from_slice(bytes);
            self.end += bytes.len();
            return true;
        }
        // WriteFailed — but only AFTER the prefix that fits is copied.
        self.buf[self.end..].copy_from_slice(&bytes[..avail]);
        self.end = self.buf.len();
        false
    }
}

impl Write for FixedWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        if self.write_prefix(s.as_bytes()) {
            Ok(())
        } else {
            Err(core::fmt::Error)
        }
    }
}

// PORT NOTE: several rustty-vt encoders (`formatter::*::format`,
// `input::key_encode::encode`, `input::mouse_encode::encode`) are generic over
// `std::io::Write` (returning `std::io::Result`) rather than `core::fmt::Write`.
// The same two sinks are reproduced here: `FixedWriter` returns
// `std::io::ErrorKind::WriteZero` (the rustty analog of `error.WriteFailed`)
// when a write would overflow, while `CountingWriter` never fails. An
// overflowing write copies the fitting prefix into the caller buffer first and
// then fails, matching `std.Io.Writer.fixed` (see the byte-level probe in the
// `FixedWriter` doc comment).
impl std::io::Write for FixedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.write_prefix(bytes) {
            Ok(bytes.len())
        } else {
            // WriteFailed — the prefix that fit has already been copied.
            Err(std::io::Error::from(std::io::ErrorKind::WriteZero))
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
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

impl std::io::Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.count += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;

    // W81-B01-01 (Rust-only proving test; the reference relies on
    // `std.Io.Writer.fixed`'s own semantics and has no such unit test). The
    // oracle probe on zig 0.16.0 is
    //     buf = "XY"; fixed(buf).writeAll("abc")
    //     => result=error.WriteFailed end=2 buf="ab"
    // i.e. the fitting PREFIX is copied before the failure. This adaptor used
    // to reject the overflowing write whole, leaving "XY" intact.
    #[test]
    fn overflowing_fmt_write_copies_the_fitting_prefix_then_fails() {
        let mut buf = *b"XY";
        let mut w = FixedWriter::new(&mut buf);
        assert!(core::fmt::Write::write_str(&mut w, "abc").is_err());
        assert_eq!(2, w.end);
        assert_eq!(b"ab", &buf);
    }

    #[test]
    fn overflowing_io_write_copies_the_fitting_prefix_then_fails() {
        let mut buf = *b"XY";
        let mut w = FixedWriter::new(&mut buf);
        let err = w.write(b"abc").unwrap_err();
        assert_eq!(std::io::ErrorKind::WriteZero, err.kind());
        assert_eq!(2, w.end);
        assert_eq!(b"ab", &buf);
    }

    // A write that fits is unaffected, and a partially-filled buffer still
    // takes only what remains (the `end` offset participates in the prefix).
    #[test]
    fn fitting_writes_are_unchanged_and_prefix_respects_end() {
        let mut buf = *b"XYZW";
        let mut w = FixedWriter::new(&mut buf);
        assert!(w.write(b"ab").is_ok());
        assert_eq!(2, w.end);
        assert!(w.write(b"cde").is_err());
        assert_eq!(4, w.end);
        assert_eq!(b"abcd", &buf);
    }

    // A zero-length write against a full buffer still succeeds, exactly like
    // `writeAll("")` on an exhausted `std.Io.Writer.fixed`.
    #[test]
    fn empty_write_on_a_full_buffer_succeeds() {
        let mut buf = *b"XY";
        let mut w = FixedWriter::new(&mut buf);
        assert!(w.write(b"ab").is_ok());
        assert!(w.write(b"").is_ok());
        assert_eq!(2, w.end);
    }
}
