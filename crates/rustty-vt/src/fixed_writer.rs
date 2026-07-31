//! A fixed-capacity `core::fmt::Write` sink.
//!
//! PORT NOTE: this has no single upstream file. It is the faithful stand-in for
//! Zig's `std.Io.Writer.fixed(&buf)`, which the reference uses wherever a VT
//! response is encoded into a stack buffer of a statically-known maximum size
//! (`stream_terminal.zig`'s mode-2048 resize report, `dcs.zig`'s DECRQSS
//! response, ...). rustty's encoders write through `core::fmt::Write`, so the
//! sink is expressed as a `&mut [u8]` plus a write cursor.
//!
//! The buffer stays a byte buffer end to end: nothing here goes through
//! `String`, matching the reference's `[]u8`.

/// A writer over a caller-supplied byte buffer.
///
/// `write_str` fails (`core::fmt::Error`) when the buffer would overflow,
/// mirroring `std.Io.Writer.fixed`'s `error.WriteFailed`.
pub struct FixedWriter<'a> {
    buf: &'a mut [u8],
    /// Number of bytes written so far. (Zig: `std.Io.Writer.end`.)
    pub end: usize,
}

impl<'a> FixedWriter<'a> {
    /// `std.Io.Writer.fixed(buf)`
    pub fn new(buf: &'a mut [u8]) -> FixedWriter<'a> {
        FixedWriter { buf, end: 0 }
    }

    /// `writer.buffered()`
    pub fn buffered(&self) -> &[u8] {
        &self.buf[..self.end]
    }
}

impl core::fmt::Write for FixedWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let next = match self.end.checked_add(bytes.len()) {
            Some(v) => v,
            None => return Err(core::fmt::Error),
        };
        if next > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[self.end..next].copy_from_slice(bytes);
        self.end = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write as _;

    #[test]
    fn writes_and_reports_buffered() {
        let mut buf = [0u8; 8];
        let mut w = FixedWriter::new(&mut buf);
        w.write_str("abc").unwrap();
        w.write_str("de").unwrap();
        assert_eq!(b"abcde".as_slice(), w.buffered());
        assert_eq!(5, w.end);
    }

    #[test]
    fn overflow_is_an_error_and_writes_nothing() {
        let mut buf = [0u8; 4];
        let mut w = FixedWriter::new(&mut buf);
        w.write_str("abc").unwrap();
        assert!(w.write_str("de").is_err());
        // The partial write is rejected whole, so the cursor did not move.
        assert_eq!(b"abc".as_slice(), w.buffered());
    }
}
