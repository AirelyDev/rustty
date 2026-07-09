//! PORT: the `*std.Io.Writer` surface the synthetic package writes through.
//!
//! We reuse `rustty_os::string_encoding`'s `Writer` trait (so the generators can
//! hand their writer straight to `urlPercentEncode`, which is generic over it)
//! and its `FixedWriter`/`WriteFailed` (`std.Io.Writer.fixed`). `WriterExt` adds
//! the `writer.print("…{d}…", …)` / `writeInt` conveniences the generators use.

pub use rustty_os::string_encoding::{FixedWriter, WriteFailed, Writer};

/// PORT: the `writer.print`/`writer.writeInt` helpers layered on `std.Io.Writer`.
/// Blanket-implemented for every `Writer`.
pub trait WriterExt: Writer {
    /// PORT: `writer.print("{d}", .{v})` for an unsigned integer — decimal,
    /// no leading zeros.
    #[allow(non_snake_case)]
    fn writeUint(&mut self, v: u64) -> Result<(), Self::Error> {
        let mut buf = [0u8; 20];
        let mut i = buf.len();
        let mut n = v;
        loop {
            i -= 1;
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        self.writeAll(&buf[i..])
    }

    /// PORT: `writer.writeInt(usize, v, .little)` — a fixed 8-byte LE encoding
    /// (`usize` is 64-bit on the target).
    #[allow(non_snake_case)]
    fn writeIntUsizeLittle(&mut self, v: usize) -> Result<(), Self::Error> {
        self.writeAll(&(v as u64).to_le_bytes())
    }
}

impl<W: Writer> WriterExt for W {}
