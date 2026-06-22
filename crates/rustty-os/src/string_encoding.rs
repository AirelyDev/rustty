//! rustty-os::string_encoding
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/string_encoding.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).

// PORT: the Zig reference imports only `std`. The decode/encode routines write to
// a `*std.Io.Writer` and may fail with `std.Io.Writer.Error` or `DecodeError`.
// We model the writer as the `Writer` trait below (the `std.Io.Writer`
// interface's `writeByte`/`writeAll` surface used here) and the error union as
// `EncodeError` (Writer.Error || error{DecodeError}). The `Allocating` writer
// used in tests is `AllocatingWriter`, whose `written()` returns the buffer.

use std::convert::Infallible;

/// PORT: `std.Io.Writer` — the subset (`writeByte`, `writeAll`) used by this
/// module, with the associated `Writer.Error`.
pub trait Writer {
    type Error;
    #[allow(non_snake_case)]
    fn writeByte(&mut self, byte: u8) -> Result<(), Self::Error>;
    #[allow(non_snake_case)]
    fn writeAll(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;
}

/// PORT: the error union `std.Io.Writer.Error || error{DecodeError}`. `W` is the
/// concrete writer's `Error` type; `DecodeError` is the reference's added error.
#[derive(Debug)]
pub enum WriteOrDecodeError<W> {
    Writer(W),
    DecodeError,
}

/// PORT: `std.Io.Writer.Allocating` — an allocating writer accumulating into an
/// owned buffer; `written()` returns the accumulated bytes.
#[derive(Default)]
pub struct AllocatingWriter {
    buf: Vec<u8>,
}

impl AllocatingWriter {
    pub fn init() -> AllocatingWriter {
        AllocatingWriter { buf: Vec::new() }
    }
    pub fn written(&self) -> &[u8] {
        &self.buf
    }
}

impl Writer for AllocatingWriter {
    type Error = Infallible;
    fn writeByte(&mut self, byte: u8) -> Result<(), Infallible> {
        self.buf.push(byte);
        Ok(())
    }
    fn writeAll(&mut self, bytes: &[u8]) -> Result<(), Infallible> {
        self.buf.extend_from_slice(bytes);
        Ok(())
    }
}

/// PORT: `std.Io.Writer.fixed(&buf)` — a writer over a fixed-size buffer.
/// `buffered()` returns the bytes written so far.
pub struct FixedWriter<'a> {
    buf: &'a mut [u8],
    end: usize,
}

/// PORT: the writer overflowing its fixed buffer maps to `Writer.Error`'s
/// `WriteFailed` (here, a unit error).
#[derive(Debug)]
pub struct WriteFailed;

impl<'a> FixedWriter<'a> {
    pub fn fixed(buf: &'a mut [u8]) -> FixedWriter<'a> {
        FixedWriter { buf, end: 0 }
    }
    pub fn buffered(&self) -> &[u8] {
        &self.buf[0..self.end]
    }
}

impl Writer for FixedWriter<'_> {
    type Error = WriteFailed;
    fn writeByte(&mut self, byte: u8) -> Result<(), WriteFailed> {
        if self.end >= self.buf.len() {
            return Err(WriteFailed);
        }
        self.buf[self.end] = byte;
        self.end += 1;
        Ok(())
    }
    fn writeAll(&mut self, bytes: &[u8]) -> Result<(), WriteFailed> {
        if self.end + bytes.len() > self.buf.len() {
            return Err(WriteFailed);
        }
        self.buf[self.end..self.end + bytes.len()].copy_from_slice(bytes);
        self.end += bytes.len();
        Ok(())
    }
}

/// Decode date from the buffer that has been encoded in the same way that
/// `bash`'s `printf %q` encodes a string and write it to the writer. If an
/// error is returned garbage may have been written to the buffer.
#[allow(non_snake_case)]
pub fn printfQDecode<Wr: Writer>(
    writer: &mut Wr,
    buf: &[u8],
) -> Result<(), WriteOrDecodeError<Wr::Error>> {
    let data: &[u8] = 'data: {
        // Strip off `$''` quoting.
        if buf.starts_with(b"$'") {
            if buf.len() < 3 || !buf.ends_with(b"'") {
                return Err(WriteOrDecodeError::DecodeError);
            }
            break 'data &buf[2..buf.len() - 1];
        }
        // Strip off `''` quoting.
        if buf.starts_with(b"'") {
            if buf.len() < 2 || !buf.ends_with(b"'") {
                return Err(WriteOrDecodeError::DecodeError);
            }
            break 'data &buf[1..buf.len() - 1];
        }
        buf
    };

    let mut src: usize = 0;

    while src < data.len() {
        match data[src] {
            b'\\' => {
                if src + 1 >= data.len() {
                    return Err(WriteOrDecodeError::DecodeError);
                }
                match data[src + 1] {
                    c @ (b' ' | b'\\' | b'"' | b'\'' | b'$') => {
                        writer.writeByte(c).map_err(WriteOrDecodeError::Writer)?;
                        src += 2;
                    }
                    b'e' => {
                        writer
                            .writeByte(control_code::ESC)
                            .map_err(WriteOrDecodeError::Writer)?;
                        src += 2;
                    }
                    b'n' => {
                        writer
                            .writeByte(control_code::LF)
                            .map_err(WriteOrDecodeError::Writer)?;
                        src += 2;
                    }
                    b'r' => {
                        writer
                            .writeByte(control_code::CR)
                            .map_err(WriteOrDecodeError::Writer)?;
                        src += 2;
                    }
                    b't' => {
                        writer
                            .writeByte(control_code::HT)
                            .map_err(WriteOrDecodeError::Writer)?;
                        src += 2;
                    }
                    b'v' => {
                        writer
                            .writeByte(control_code::VT)
                            .map_err(WriteOrDecodeError::Writer)?;
                        src += 2;
                    }
                    _ => return Err(WriteOrDecodeError::DecodeError),
                }
            }
            _ => {
                writer
                    .writeByte(data[src])
                    .map_err(WriteOrDecodeError::Writer)?;
                src += 1;
            }
        }
    }
    Ok(())
}

// PORT: `std.ascii.control_code.{esc,lf,cr,ht,vt}` — the ASCII control byte
// values used by `printfQDecode`.
mod control_code {
    pub const ESC: u8 = 0x1b;
    pub const LF: u8 = 0x0a;
    pub const CR: u8 = 0x0d;
    pub const HT: u8 = 0x09;
    pub const VT: u8 = 0x0b;
}

/// Decode data from the buffer that has been URL percent encoded and write
/// it to the given buffer. If an error is returned the garbage may have been
/// written to the writer.
#[allow(non_snake_case)]
pub fn urlPercentDecode<Wr: Writer>(
    writer: &mut Wr,
    buf: &[u8],
) -> Result<(), WriteOrDecodeError<Wr::Error>> {
    let mut src: usize = 0;
    while src < buf.len() {
        match buf[src] {
            b'%' => {
                if src + 2 >= buf.len() {
                    return Err(WriteOrDecodeError::DecodeError);
                }
                match buf[src + 1] {
                    b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => match buf[src + 2] {
                        b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => {
                            writer
                                .writeByte(shl(hex(buf[src + 1]), 4) | hex(buf[src + 2]))
                                .map_err(WriteOrDecodeError::Writer)?;
                            src += 3;
                        }
                        _ => return Err(WriteOrDecodeError::DecodeError),
                    },
                    _ => return Err(WriteOrDecodeError::DecodeError),
                }
            }
            _ => {
                writer
                    .writeByte(buf[src])
                    .map_err(WriteOrDecodeError::Writer)?;
                src += 1;
            }
        }
    }
    Ok(())
}

// PORT: `std.math.shl(u8, x, 4)` — left shift saturating to 0 on overshift.
// Here the shift is always 4 on a `u4`-widened value, so it is a plain `<< 4`.
fn shl(x: u8, amt: u32) -> u8 {
    x.checked_shl(amt).unwrap_or(0)
}

fn hex(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => unreachable!(),
    }
}

/// Is the given character valid in URI percent encoding?
#[allow(non_snake_case)]
fn isValidChar(c: u8) -> bool {
    // PORT: `std.ascii.isPrint(c)` is true for printable ASCII 0x20..=0x7e (space
    // through `~`). Space is already excluded by the first arm (returns false),
    // so on the `else` arm `c` is never space; thus `is_ascii_graphic()`
    // (0x21..=0x7e) computes exactly `isPrint(c)` for every byte reaching it.
    match c {
        b' ' | b';' | b'=' => false,
        _ => c.is_ascii_graphic(),
    }
}

/// Write data to the writer after URI percent encoding.
#[allow(non_snake_case)]
pub fn urlPercentEncode<Wr: Writer>(writer: &mut Wr, data: &[u8]) -> Result<(), Wr::Error> {
    crate::uri::Component::percentEncode(writer, data, isValidChar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printf_q_1() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr\\ kurwa";

        printfQDecode(&mut w, s).unwrap();
        assert_eq!(b"bobr kurwa".as_slice(), w.written());
    }

    #[test]
    fn printf_q_2() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr\\nkurwa";

        printfQDecode(&mut w, s).unwrap();
        assert_eq!(b"bobr\nkurwa".as_slice(), w.written());
    }

    #[test]
    fn printf_q_3() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr\\dkurwa";

        assert!(matches!(
            printfQDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn printf_q_4() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr kurwa\\";

        assert!(matches!(
            printfQDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn printf_q_5() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"$'bobr kurwa'";

        printfQDecode(&mut w, s).unwrap();
        assert_eq!(b"bobr kurwa".as_slice(), w.written());
    }

    #[test]
    fn printf_q_6() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"'bobr kurwa'";

        printfQDecode(&mut w, s).unwrap();
        assert_eq!(b"bobr kurwa".as_slice(), w.written());
    }

    #[test]
    fn printf_q_7() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"$'bobr kurwa";

        assert!(matches!(
            printfQDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn printf_q_8() {
        let mut w = AllocatingWriter::init();
        let s: &[u8] = b"$'";
        // PORT: the reference copies `s` into a sentinel-terminated `src` buffer
        // (`var src: [s.len:0]u8`) that is otherwise unused before calling
        // `printfQDecode(&w.writer, s)`. The decode is performed on `s` itself.
        let mut src = [0u8; 2];
        src.copy_from_slice(s);
        let _ = src;
        assert!(matches!(
            printfQDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn printf_q_9() {
        let mut w = AllocatingWriter::init();
        let s: &[u8] = b"'bobr kurwa";
        let mut src = [0u8; 11];
        src.copy_from_slice(s);
        let _ = src;
        assert!(matches!(
            printfQDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn printf_q_10() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"'";
        let mut src = [0u8; 1];
        src.copy_from_slice(s);
        let _ = src;
        assert!(matches!(
            printfQDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn singles_percent() {
        for c in 0..255usize {
            let mut w = AllocatingWriter::init();

            // PORT: `std.fmt.bufPrintZ(&buf, "%{x:0>2}", .{c})` formats one
            // lowercase-hex `%XX` escape. The expected decoded byte is `c`
            // itself (an independent oracle: encode the literal value, decode it
            // back, expect the same value).
            let buf = format!("%{:02x}", c);

            urlPercentDecode(&mut w, buf.as_bytes()).unwrap();
            let decoded = w.written();

            assert_eq!(1, decoded.len());
            assert_eq!(c as u8, decoded[0]);
        }
        for c in 0..255usize {
            let mut w = AllocatingWriter::init();

            let buf = format!("%{:02X}", c);

            urlPercentDecode(&mut w, buf.as_bytes()).unwrap();
            let decoded = w.written();

            assert_eq!(1, decoded.len());
            assert_eq!(c as u8, decoded[0]);
        }
    }

    #[test]
    fn percent_1() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr%20kurwa";

        urlPercentDecode(&mut w, s).unwrap();
        assert_eq!(b"bobr kurwa".as_slice(), w.written());
    }

    #[test]
    fn percent_2() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr%2kurwa";

        assert!(matches!(
            urlPercentDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn percent_3() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr%kurwa";

        assert!(matches!(
            urlPercentDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn percent_4() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr%%kurwa";

        assert!(matches!(
            urlPercentDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn percent_5() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr%20kurwa%20";

        urlPercentDecode(&mut w, s).unwrap();
        assert_eq!(b"bobr kurwa ".as_slice(), w.written());
    }

    #[test]
    fn percent_6() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr%20kurwa%2";

        assert!(matches!(
            urlPercentDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }

    #[test]
    fn percent_7() {
        let mut w = AllocatingWriter::init();

        let s: &[u8] = b"bobr%20kurwa%";

        assert!(matches!(
            urlPercentDecode(&mut w, s),
            Err(WriteOrDecodeError::DecodeError)
        ));
    }
}
