//! Kitty drag and drop protocol (OSC 72) response encoding.
//!
//! rustty-vt kitty/dnd_response
//!
//! PORTS: ghostty src/terminal/kitty/dnd_response.zig
//!
//! PORT STATUS: ported (phase-5 B6 end state at 683d8db64: 38746b8c1)

// const std = @import("std");
// const Terminator = @import("../osc.zig").Terminator;
use crate::osc::Terminator;

/// The maximum raw bytes per base64-encoded chunk, chosen by kitty so a
/// chunk is exactly 4096 base64 characters (the protocol's chunk limit).
pub const MAX_CHUNK_RAW: usize = 3072;

/// The maximum bytes per plain-text (non-base64) chunk.
pub const MAX_CHUNK_PLAIN: usize = 4096;

/// Error names used in protocol error payloads. The wire encoding is
/// the tag name itself. This matches kitty's get_errno_name vocabulary,
/// which extends the spec's list with EISDIR, ENOSPC, and OK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Errno {
    OK,
    EPERM,
    ENOENT,
    EIO,
    EINVAL,
    EMFILE,
    ENOMEM,
    EFBIG,
    EISDIR,
    ENOSPC,
    EUNKNOWN,
}

impl Errno {
    /// PORT NOTE: `@tagName(errno)` — the wire encoding is the tag name.
    pub fn name(self) -> &'static str {
        match self {
            Errno::OK => "OK",
            Errno::EPERM => "EPERM",
            Errno::ENOENT => "ENOENT",
            Errno::EIO => "EIO",
            Errno::EINVAL => "EINVAL",
            Errno::EMFILE => "EMFILE",
            Errno::ENOMEM => "ENOMEM",
            Errno::EFBIG => "EFBIG",
            Errno::EISDIR => "EISDIR",
            Errno::ENOSPC => "ENOSPC",
            Errno::EUNKNOWN => "EUNKNOWN",
        }
    }
}

/// The payload encoding for a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// Payload bytes are sent as-is (MIME lists, error strings).
    Plain,

    /// Payload bytes are base64-encoded (all binary data).
    Base64,
}

/// The kind of error response. Selects the header: t=R for drop data
/// request errors, t=E for drag offer errors.
///
/// PORT NOTE: the reference uses an anonymous parameter enum
/// `enum { drop, drag }` on `encodeError`; Rust requires a named type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Drop,
    Drag,
}

/// The `x`/`y`/`Y` keys of the data request currently being answered,
/// echoed in responses and errors so the client can match them up. Only
/// non-zero keys are written, matching kitty's drop_append_request_keys.
///
/// PORT NOTE: the reference field `Y: i32` is spelled `cap_y` (Rust field
/// names cannot differ only by case conventions here without tripping
/// non-snake-case lints; matches the `CapX`/`CapY` naming precedent from
/// the pin-era OSC 72 parser). The wire key is still `Y`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RequestKeys {
    pub x: i32,
    pub y: i32,
    pub cap_y: i32,
}

// pub fn format(self: RequestKeys, writer: *std.Io.Writer) !void
// PORT NOTE: Zig's `{f}` format hook is ported as `core::fmt::Display`.
impl core::fmt::Display for RequestKeys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.x != 0 {
            write!(f, ":x={}", self.x)?;
        }
        if self.y != 0 {
            write!(f, ":y={}", self.y)?;
        }
        if self.cap_y != 0 {
            write!(f, ":Y={}", self.cap_y)?;
        }
        Ok(())
    }
}

/// Encode a complete protocol message: one bare OSC when `data` is
/// empty, otherwise one complete OSC per chunk of `data`, each
/// repeating the header. The final chunk carries `m=0`, earlier
/// chunks `m=1`.
///
/// `header` is the metadata without the OSC introducer, e.g. "t=q" or
/// "t=m:x=5:y=3". The client ID is appended as `:i=N` when non-zero.
///
/// PORT NOTE: `writer: *std.Io.Writer` → generic `std::io::Write`;
/// `std.Io.Writer.Error` → `std::io::Result`.
pub fn encode<W: std::io::Write>(
    writer: &mut W,
    header: &[u8],
    client_id: u32,
    data: &[u8],
    encoding: Encoding,
    terminator: Terminator,
) -> std::io::Result<()> {
    // The client ID is part of the repeated header.
    // PORT NOTE: the reference formats into a fixed `[16]u8` stack buffer
    // (`catch unreachable`: ":i=" + u32 always fits); a small String is the
    // Rust shape, same bytes.
    let id: String = if client_id != 0 {
        format!(":i={}", client_id)
    } else {
        String::new()
    };

    if data.is_empty() {
        // try writer.print("\x1b]72;{s}{s}{s}", .{ header, id, terminator.string() });
        writer.write_all(b"\x1b]72;")?;
        writer.write_all(header)?;
        writer.write_all(id.as_bytes())?;
        writer.write_all(terminator.string())?;
        return Ok(());
    }

    let limit: usize = match encoding {
        Encoding::Base64 => MAX_CHUNK_RAW,
        Encoding::Plain => MAX_CHUNK_PLAIN,
    };

    let mut offset: usize = 0;
    while offset < data.len() {
        let chunk_len = (data.len() - offset).min(limit);
        let chunk = &data[offset..offset + chunk_len];
        offset += chunk_len;
        let last: u8 = if offset >= data.len() { b'0' } else { b'1' };

        // try writer.print("\x1b]72;{s}{s}:m={c};", .{ header, id, last });
        writer.write_all(b"\x1b]72;")?;
        writer.write_all(header)?;
        writer.write_all(id.as_bytes())?;
        writer.write_all(&[b':', b'm', b'=', last, b';'])?;
        match encoding {
            Encoding::Plain => writer.write_all(chunk)?,
            Encoding::Base64 => {
                // var b64_buf: [std.base64.standard.Encoder.calcSize(max_chunk_raw)]u8 = undefined;
                // try writer.writeAll(std.base64.standard.Encoder.encode(&b64_buf, chunk));
                let mut b64_buf = [0u8; base64_calc_size(MAX_CHUNK_RAW)];
                writer.write_all(base64_standard_encode(&mut b64_buf, chunk))?;
            }
        }
        writer.write_all(terminator.string())?;
    }

    Ok(())
}

/// Encode an error response. `kind` selects the header: t=R for drop
/// data request errors, t=E for drag offer errors. The payload is
/// "NAME" or "NAME:description", sent plain (not base64).
pub fn encode_error<W: std::io::Write>(
    writer: &mut W,
    kind: ErrorKind,
    keys: RequestKeys,
    client_id: u32,
    errno: Errno,
    desc: &[u8],
    terminator: Terminator,
) -> std::io::Result<()> {
    // var header_buf: [64]u8 = undefined;
    // const header = std.fmt.bufPrint(&header_buf, "t={c}{f}", .{ ..., keys })
    //     catch unreachable;
    let header = format!(
        "t={}{}",
        match kind {
            ErrorKind::Drop => 'R',
            ErrorKind::Drag => 'E',
        },
        keys,
    );

    // Description strings are short static messages; size the buffer
    // for the longest error name plus a generous description.
    // PORT NOTE: the reference's fixed `[256]u8` buffer (`catch
    // unreachable`) becomes a Vec; same bytes, no truncation either way.
    let payload: Vec<u8> = if !desc.is_empty() {
        // "{t}:{s}"
        let mut p = Vec::with_capacity(errno.name().len() + 1 + desc.len());
        p.extend_from_slice(errno.name().as_bytes());
        p.push(b':');
        p.extend_from_slice(desc);
        p
    } else {
        errno.name().as_bytes().to_vec()
    };

    encode(
        writer,
        header.as_bytes(),
        client_id,
        &payload,
        Encoding::Plain,
        terminator,
    )
}

/// PORT NOTE: `std.base64.standard.Encoder.calcSize(n)` — the padded
/// standard-alphabet encoded length.
const fn base64_calc_size(n: usize) -> usize {
    n.div_ceil(3) * 4
}

/// PORT NOTE: faithful port of `std.base64.standard.Encoder.encode`
/// (standard RFC 4648 alphabet, `'='` padding). The workspace has no
/// base64 *encoder* (rustty-simd only decodes), matching the precedent of
/// local encoders in `apc/glyph/glossary.rs` and `rustty-core/surface.rs`.
fn base64_standard_encode<'a>(dest: &'a mut [u8], source: &[u8]) -> &'a [u8] {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let out_len = base64_calc_size(source.len());
    debug_assert!(dest.len() >= out_len);

    let mut di: usize = 0;
    let mut chunks = source.chunks_exact(3);
    for chunk in &mut chunks {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        dest[di] = ALPHABET[(n >> 18) as usize & 63];
        dest[di + 1] = ALPHABET[(n >> 12) as usize & 63];
        dest[di + 2] = ALPHABET[(n >> 6) as usize & 63];
        dest[di + 3] = ALPHABET[n as usize & 63];
        di += 4;
    }
    let rem = chunks.remainder();
    match rem.len() {
        0 => {}
        1 => {
            let n = (rem[0] as u32) << 16;
            dest[di] = ALPHABET[(n >> 18) as usize & 63];
            dest[di + 1] = ALPHABET[(n >> 12) as usize & 63];
            dest[di + 2] = b'=';
            dest[di + 3] = b'=';
            di += 4;
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            dest[di] = ALPHABET[(n >> 18) as usize & 63];
            dest[di + 1] = ALPHABET[(n >> 12) as usize & 63];
            dest[di + 2] = ALPHABET[(n >> 6) as usize & 63];
            dest[di + 3] = b'=';
            di += 4;
        }
        _ => unreachable!(),
    }
    debug_assert_eq!(out_len, di);
    &dest[0..di]
}

#[cfg(test)]
mod tests {
    use super::{encode, encode_error, Encoding, Errno, ErrorKind, RequestKeys, MAX_CHUNK_RAW};
    use crate::osc::Terminator;

    #[test]
    fn encode_bare_message() {
        let mut buf: Vec<u8> = Vec::new();
        encode(&mut buf, b"t=q", 0, b"", Encoding::Plain, Terminator::St).unwrap();
        assert_eq!(b"\x1b]72;t=q\x1b\\".as_slice(), buf.as_slice());
    }

    #[test]
    fn encode_bare_message_with_client_id() {
        let mut buf: Vec<u8> = Vec::new();
        encode(&mut buf, b"t=q", 7, b"", Encoding::Plain, Terminator::St).unwrap();
        assert_eq!(b"\x1b]72;t=q:i=7\x1b\\".as_slice(), buf.as_slice());
    }

    #[test]
    fn encode_plain_payload_single_chunk() {
        let mut buf: Vec<u8> = Vec::new();
        encode(
            &mut buf,
            b"t=m:x=1:y=2",
            0,
            b"text/plain ",
            Encoding::Plain,
            Terminator::St,
        )
        .unwrap();
        assert_eq!(
            b"\x1b]72;t=m:x=1:y=2:m=0;text/plain \x1b\\".as_slice(),
            buf.as_slice(),
        );
    }

    #[test]
    fn encode_base64_payload_chunking() {
        let mut out: Vec<u8> = Vec::new();

        // Exactly one byte more than a chunk to force two chunks.
        let data = [b'A'; MAX_CHUNK_RAW + 1];
        encode(
            &mut out,
            b"t=r:x=1",
            3,
            &data,
            Encoding::Base64,
            Terminator::St,
        )
        .unwrap();

        // First chunk: full header, m=1, 4096 base64 chars.
        let prefix = b"\x1b]72;t=r:x=1:i=3:m=1;";
        assert!(out.starts_with(prefix));
        let first_end = out.windows(2).position(|w| w == b"\x1b\\").unwrap();
        assert_eq!(prefix.len() + 4096, first_end);

        // Second chunk: m=0 with the single remaining byte.
        let rest = &out[first_end + 2..];
        assert!(rest.starts_with(b"\x1b]72;t=r:x=1:i=3:m=0;"));

        // Decodes back to the original data.
        let mut decoded: Vec<u8> = Vec::new();
        for osc in out
            .split(|&b| b == 0x1b)
            .map(|s| s.strip_prefix(b"]72;").unwrap_or(s))
        {
            // PORT NOTE: the reference splits on the two-byte "\x1b\\" ST;
            // splitting on ESC and stripping the "]72;"/"\\" remnants yields
            // the same payload extraction.
            let osc = osc.strip_prefix(b"\\").unwrap_or(osc);
            if osc.is_empty() {
                continue;
            }
            let payload_start = match osc.iter().position(|&b| b == b';') {
                Some(p) => p,
                None => continue,
            };
            let payload = &osc[payload_start + 1..];
            if payload.is_empty() {
                continue;
            }
            let max_len = rustty_simd::base64::max_len(payload);
            let mut buf = vec![0u8; max_len];
            let decoded_chunk = rustty_simd::base64::decode(payload, &mut buf).unwrap();
            decoded.extend_from_slice(decoded_chunk);
        }
        assert_eq!(data.as_slice(), decoded.as_slice());
    }

    #[test]
    fn encode_error_with_and_without_description() {
        {
            let mut buf: Vec<u8> = Vec::new();
            encode_error(
                &mut buf,
                ErrorKind::Drop,
                RequestKeys {
                    x: 2,
                    ..Default::default()
                },
                0,
                Errno::ENOENT,
                b"drop data request index out of bounds",
                Terminator::St,
            )
            .unwrap();
            assert_eq!(
                b"\x1b]72;t=R:x=2:m=0;ENOENT:drop data request index out of bounds\x1b\\"
                    .as_slice(),
                buf.as_slice(),
            );
        }
        {
            let mut buf: Vec<u8> = Vec::new();
            encode_error(
                &mut buf,
                ErrorKind::Drag,
                RequestKeys::default(),
                5,
                Errno::EPERM,
                b"",
                Terminator::St,
            )
            .unwrap();
            assert_eq!(
                b"\x1b]72;t=E:i=5:m=0;EPERM\x1b\\".as_slice(),
                buf.as_slice()
            );
        }
    }

    #[test]
    fn request_keys_only_non_zero_keys_written() {
        {
            let s = format!("{}", RequestKeys::default());
            assert_eq!("", s);
        }
        {
            let s = format!(
                "{}",
                RequestKeys {
                    x: 1,
                    y: 2,
                    cap_y: 3
                }
            );
            assert_eq!(":x=1:y=2:Y=3", s);
        }
        {
            let s = format!(
                "{}",
                RequestKeys {
                    cap_y: 4,
                    ..Default::default()
                }
            );
            assert_eq!(":Y=4", s);
        }
    }
}
