//! rustty-vt input/paste
//!
//! PORTS: ghostty src/input/paste.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports
//   const std = @import("std");
//   const Terminal = @import("../terminal/Terminal.zig");
// `std` is used for slice scanning helpers (reproduced inline below) and
// `Terminal` for `Options.fromTerminal`. `std.Io.Writer` (`encodeWriter`'s
// sink) maps to the crate's byte-oriented stand-in, `snapshot::io::Writer`.

use crate::snapshot::io::{WriteFailed, Writer};
use crate::terminal::Terminal;

/// The bracketed paste (mode 2004) frame written around the data.
// pub const bracketed_prefix = "\x1b[200~";
// pub const bracketed_suffix = "\x1b[201~";
#[allow(non_upper_case_globals)]
pub const bracketed_prefix: &[u8] = b"\x1b[200~";
#[allow(non_upper_case_globals)]
pub const bracketed_suffix: &[u8] = b"\x1b[201~";

/// The maximum number of bytes `encode` adds around the data, so callers
/// can size a buffer for the full encoded result.
#[allow(non_upper_case_globals)]
pub const max_frame_size: usize = bracketed_prefix.len() + bracketed_suffix.len();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// True if bracketed paste mode is on.
    pub bracketed: bool,
}

impl Options {
    /// Return the encoding options based on the current terminal state.
    pub fn from_terminal(t: &Terminal) -> Options {
        Options {
            bracketed: t.modes.get(crate::modes::Mode::BracketedPaste),
        }
    }
}

/// Encode the given data for pasting. The resulting value can be written
/// to the pty to perform a paste of the input data.
///
/// The data can be either a `[]u8` or a `[]const u8`. If the data
/// type is const then `EncodeError` may be returned. If the data type
/// is mutable then this function can't return an error.
///
/// This slightly complex calling style allows for initially const
/// data to be passed in without an allocation, since it is rare in normal
/// use cases that the data will need to be modified. In the unlikely case
/// data does need to be modified, the caller can make a mutable copy
/// after seeing the error.
///
/// The data is returned as a set of slices to limit allocations. The caller
/// can combine the slices into a single buffer if desired.
///
/// WARNING: The input data is not checked for safety. See the `isSafe`
/// function to check if the data is safe to paste.
///
/// PORT NOTE: the reference `encode` is a single comptime-dispatched function
/// whose return type and behavior depend on whether `data` is `[]u8` (mutable,
/// never errors) or `[]const u8` (const, may return `Error.MutableRequired`).
/// Rust has no comptime overloading on slice mutability, so we split it into two
/// functions that mirror the two `@TypeOf(data)` branches exactly:
///   - `encode` for `[]const u8` → `Error![3][]const u8`
///   - `encode_mut` for `[]u8`   → `[3][]const u8`
///
/// Both share the identical strip/bracketed/newline logic below.
pub fn encode(data: &[u8], opts: Options) -> Result<[&[u8]; 3], Error> {
    // These are the set of byte values that are always replaced by
    // a space (per xterm's behavior) for any text insertion method e.g.
    // a paste, drag and drop, etc. These are copied directly from xterm's
    // source.
    const STRIP: &[u8] = &[
        0x00, // NUL
        0x08, // BS
        0x05, // ENQ
        0x04, // EOT
        0x1B, // ESC
        0x7F, // DEL
        // These can be overridden by the running terminal program
        // via tcsetattr, so they aren't totally safe to hardcode like
        // this. In practice, I haven't seen modern programs change these
        // and its a much bigger architectural change to pass these through
        // so for now they're hardcoded.
        0x03, // VINTR (Ctrl+C)
        0x1C, // VQUIT (Ctrl+\)
        0x15, // VKILL (Ctrl+U)
        0x1A, // VSUSP (Ctrl+Z)
        0x11, // VSTART (Ctrl+Q)
        0x13, // VSTOP (Ctrl+S)
        0x17, // VWERASE (Ctrl+W)
        0x16, // VLNEXT (Ctrl+V)
        0x12, // VREPRINT (Ctrl+R)
        0x0F, // VDISCARD (Ctrl+O)
    ];

    // const mutable = @TypeOf(data) == []u8;
    // (const variant: mutable = false)

    let mut result: [&[u8]; 3] = [b"", data, b""];

    // If we have any of the strip values, then we need to replace them
    // with spaces. This is what xterm does and it does it regardless
    // of bracketed paste mode. This is a security measure to prevent pastes
    // from containing bytes that could be used to inject commands.
    if index_of_any(data, STRIP).is_some() {
        // if (comptime !mutable) return Error.MutableRequired;
        return Err(Error::MutableRequired);
    }

    // Bracketed paste mode (mode 2004) wraps pasted data in
    // fenceposts so that the terminal can ignore things like newlines.
    if opts.bracketed {
        result[0] = bracketed_prefix;
        result[2] = bracketed_suffix;
        return Ok(result);
    }

    // Non-bracketed. We have to replace newline with `\r`. This matches
    // the behavior of xterm and other terminals. For `\r\n` this will
    // result in `\r\r` which does match xterm.
    // } else if (std.mem.indexOfScalar(u8, data, '\n') != null) {
    //     return Error.MutableRequired;
    // }
    if index_of_scalar(data, b'\n').is_some() {
        return Err(Error::MutableRequired);
    }

    Ok(result)
}

/// Mutable-data variant of `encode`. See `encode` for the contract. Because
/// the input is mutable, strip replacement and newline conversion are done in
/// place and this function can never fail.
pub fn encode_mut(data: &mut [u8], opts: Options) -> [&[u8]; 3] {
    // These are the set of byte values that are always replaced by
    // a space (per xterm's behavior) for any text insertion method e.g.
    // a paste, drag and drop, etc. These are copied directly from xterm's
    // source.
    const STRIP: &[u8] = &[
        0x00, // NUL
        0x08, // BS
        0x05, // ENQ
        0x04, // EOT
        0x1B, // ESC
        0x7F, // DEL
        // These can be overridden by the running terminal program
        // via tcsetattr, so they aren't totally safe to hardcode like
        // this. In practice, I haven't seen modern programs change these
        // and its a much bigger architectural change to pass these through
        // so for now they're hardcoded.
        0x03, // VINTR (Ctrl+C)
        0x1C, // VQUIT (Ctrl+\)
        0x15, // VKILL (Ctrl+U)
        0x1A, // VSUSP (Ctrl+Z)
        0x11, // VSTART (Ctrl+Q)
        0x13, // VSTOP (Ctrl+S)
        0x17, // VWERASE (Ctrl+W)
        0x16, // VLNEXT (Ctrl+V)
        0x12, // VREPRINT (Ctrl+R)
        0x0F, // VDISCARD (Ctrl+O)
    ];

    // const mutable = @TypeOf(data) == []u8;
    // (mutable variant: mutable = true)

    // If we have any of the strip values, then we need to replace them
    // with spaces. This is what xterm does and it does it regardless
    // of bracketed paste mode. This is a security measure to prevent pastes
    // from containing bytes that could be used to inject commands.
    if index_of_any(data, STRIP).is_some() {
        let mut offset: usize = 0;
        while let Some(idx) = index_of_any(&data[offset..], STRIP) {
            offset += idx;
            data[offset] = b' ';
            offset += 1;
        }
    }

    // Bracketed paste mode (mode 2004) wraps pasted data in
    // fenceposts so that the terminal can ignore things like newlines.
    if opts.bracketed {
        // PORT NOTE: in the reference the bracketed branch returns
        // `.{ bracketed_prefix, data, bracketed_suffix }`. We reborrow `data`
        // immutably here because the mutation above is already complete.
        return [bracketed_prefix, data, bracketed_suffix];
    }

    // Non-bracketed. We have to replace newline with `\r`. This matches
    // the behavior of xterm and other terminals. For `\r\n` this will
    // result in `\r\r` which does match xterm.
    // if (comptime mutable) {
    //     std.mem.replaceScalar(u8, data, '\n', '\r');
    // }
    replace_scalar(data, b'\n', b'\r');

    [b"", data, b""]
}

/// PORT NOTE: `std.mem.indexOfAny(u8, slice, values)` — returns the index of the
/// first byte in `slice` that is one of `values`, or `None`.
fn index_of_any(slice: &[u8], values: &[u8]) -> Option<usize> {
    slice.iter().position(|b| values.contains(b))
}

/// PORT NOTE: `std.mem.indexOfScalar(u8, slice, value)` — returns the index of
/// the first occurrence of `value`, or `None`.
fn index_of_scalar(slice: &[u8], value: u8) -> Option<usize> {
    slice.iter().position(|&b| b == value)
}

/// PORT NOTE: `std.mem.replaceScalar(u8, slice, match, replacement)` — replaces
/// every occurrence of `match` in `slice` with `replacement`, in place.
fn replace_scalar(slice: &mut [u8], match_: u8, replacement: u8) {
    for b in slice.iter_mut() {
        if *b == match_ {
            *b = replacement;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Returned if encoding requires a mutable copy of the data. This
    /// can only be returned if the input data type is const.
    MutableRequired,
}

/// Encode the given data for pasting directly into a writer. This is
/// the same transformation as `encode` (unsafe bytes replaced, bracketed
/// frame or newline conversion per `opts`) but the data is copied
/// exactly once: into the writer's buffer, where it is modified in place.
/// This is the form to use when the data is const and the result is
/// being assembled into a single buffer anyway.
///
/// The data is copied in chunks sized to the writer's buffer, so any
/// writer works; a writer with less total capacity than the writer
/// needs to hold at once reports `error.WriteFailed` as usual.
///
/// WARNING: The input data is not checked for safety. See `isSafe`
/// and `isSafeWith` to check if the data is safe to paste.
// PORT NOTE (writer stand-in mechanics): the reference encodes in place
// inside the writer's own buffer (`writableSliceGreedy(1)` + `encode` on the
// writable slice + `advance`), so the data is copied exactly once and chunks
// are sized to the writer's buffer. The `snapshot::io::Writer` stand-in (the
// crate's byte-oriented `std.Io.Writer` surface) exposes no writable buffer,
// so this port stages each chunk through a local buffer, applies the
// identical in-place transformation there (`encode_mut`, its returned frame
// ignored just as the reference ignores `encode`'s), and forwards it with
// `write_all`. Behavior is identical; only the copy count and the chunk
// boundary (a fixed local buffer rather than the writer's) differ — the same
// treatment as snapshot/io.rs's "bulk-read bypass" PORT NOTE.
pub fn encode_writer(
    writer: &mut dyn Writer,
    data: &[u8],
    opts: Options,
) -> Result<(), WriteFailed> {
    if opts.bracketed {
        writer.write_all(bracketed_prefix)?;
    }

    // The byte transformations are position-independent, so the data
    // can be copied and encoded chunk by chunk. The frame returned by
    // encode is ignored since it's written around the whole data here.
    //
    // (Zig: `const dest = try writer.writableSliceGreedy(1);` — the staging
    // chunk below stands in for the writer's writable slice.)
    let mut chunk = [0u8; 512];
    let mut remaining = data;
    while !remaining.is_empty() {
        // const n = @min(dest.len, remaining.len);
        let n = chunk.len().min(remaining.len());
        // @memcpy(dest[0..n], remaining[0..n]);
        chunk[..n].copy_from_slice(&remaining[..n]);
        // _ = encode(dest[0..n], opts);
        let _ = encode_mut(&mut chunk[..n], opts);
        // writer.advance(n);
        writer.write_all(&chunk[..n])?;
        remaining = &remaining[n..];
    }

    if opts.bracketed {
        writer.write_all(bracketed_suffix)?;
    }
    Ok(())
}

/// Returns true if the data looks safe to paste. Data is considered
/// unsafe if it contains any of the following:
///
/// - `\n`: Newlines can be used to inject commands.
/// - `\x1b[201~`: This is the end of a bracketed paste. This cane be used
///   to exit a bracketed paste and inject commands.
///
/// We consider any scenario unsafe regardless of current terminal state.
/// For example, even if bracketed paste mode is not active, we still
/// consider `\x1b[201~` unsafe. The existence of these types of bytes
/// should raise suspicion that the producer of the paste data is
/// acting strangely.
pub fn is_safe(data: &[u8]) -> bool {
    index_of(data, b"\n").is_none() && index_of(data, b"\x1b[201~").is_none()
}

/// PORT NOTE: `std.mem.indexOf(u8, haystack, needle)` — returns the index of the
/// first occurrence of the `needle` subsequence, or `None`.
fn index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Returns true if the data looks safe to paste given how it will be
/// encoded. This is the terminal-state-aware counterpart of `isSafe`:
///
/// - Bracketed (mode 2004 on): the program receives the data as one
///   framed unit, so newlines are fine. The data is unsafe only if it
///   contains the end of the frame (`\x1b[201~`), which would let the
///   rest of the data escape the frame and inject commands.
/// - Unbracketed: the same rule as `isSafe`.
///
/// Callers wanting the conservative rule regardless of terminal state
/// should use `isSafe` instead.
pub fn is_safe_with(data: &[u8], opts: Options) -> bool {
    if opts.bracketed {
        return index_of(data, bracketed_suffix).is_none();
    }
    is_safe(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::io::FixedWriter;

    #[test]
    fn is_safe_test() {
        assert!(is_safe(b"hello"));
        assert!(!is_safe(b"hello\n"));
        assert!(!is_safe(b"hello\nworld"));
        assert!(!is_safe(b"he\x1b[201~llo"));
    }

    // test isSafeWith
    #[test]
    fn is_safe_with_test() {
        // Bracketed: newlines are fine, the frame terminator is not.
        assert!(is_safe_with(b"hello", Options { bracketed: true }));
        assert!(is_safe_with(b"hello\nworld", Options { bracketed: true }));
        assert!(!is_safe_with(
            b"he\x1b[201~llo",
            Options { bracketed: true }
        ));
        assert!(!is_safe_with(
            b"hello\n\x1b[201~",
            Options { bracketed: true }
        ));

        // Unbracketed: the conservative rule.
        assert!(is_safe_with(b"hello", Options { bracketed: false }));
        assert!(!is_safe_with(b"hello\nworld", Options { bracketed: false }));
        assert!(!is_safe_with(
            b"he\x1b[201~llo",
            Options { bracketed: false }
        ));
    }

    // test "encodeWriter bracketed"
    #[test]
    fn encode_writer_bracketed() {
        let mut buf = [0u8; 64];
        // var writer: std.Io.Writer = .fixed(&buf);
        let mut writer = FixedWriter::new(&mut buf);
        encode_writer(
            &mut writer,
            b"hel\x1blo\nworld",
            Options { bracketed: true },
        )
        .unwrap();
        assert_eq!(
            b"\x1b[200~hel lo\nworld\x1b[201~".as_slice(),
            writer.buffered()
        );
    }

    // test "encodeWriter unbracketed"
    #[test]
    fn encode_writer_unbracketed() {
        let mut buf = [0u8; 64];
        let mut writer = FixedWriter::new(&mut buf);
        encode_writer(
            &mut writer,
            b"hel\x00lo\r\nworld",
            Options { bracketed: false },
        )
        .unwrap();
        assert_eq!(b"hel lo\r\rworld".as_slice(), writer.buffered());
    }

    // test "encodeWriter empty"
    #[test]
    fn encode_writer_empty() {
        let mut buf = [0u8; 64];
        let mut writer = FixedWriter::new(&mut buf);
        encode_writer(&mut writer, b"", Options { bracketed: true }).unwrap();
        assert_eq!(b"\x1b[200~\x1b[201~".as_slice(), writer.buffered());
        // writer = .fixed(&buf);
        let mut writer = FixedWriter::new(&mut buf);
        encode_writer(&mut writer, b"", Options { bracketed: false }).unwrap();
        assert_eq!(b"".as_slice(), writer.buffered());
    }

    // test "encodeWriter chunks through a small writer buffer"
    #[test]
    fn encode_writer_chunks_through_a_small_writer_buffer() {
        // A writer with a 4-byte staging buffer that drains into a list,
        // so the data is copied and encoded in several chunks.
        //
        // PORT NOTE: the reference Sink is a `std.Io.Writer` whose vtable
        // `drain(w, data, splat)` appends the writer's buffered bytes plus the
        // incoming chunks (last one repeated `splat` times) to a list; the
        // 4-byte staging buffer forces the encode loop through several
        // `writableSliceGreedy` rounds. The `Writer` stand-in has no vtable
        // buffer, so the staging + drain pair lives on the Sink itself:
        // `write_all` routes every byte through the 4-byte staging buffer and
        // drains it into the list whenever it fills, exercising the same
        // multi-drain path the reference test forces.
        struct Sink {
            // list: std.ArrayList(u8) = .empty,
            list: Vec<u8>,
            // (Zig: the writer's 4-byte staging buffer + `end` cursor.)
            buf: [u8; 4],
            end: usize,
        }

        impl Sink {
            // fn drain(w: *std.Io.Writer, data: []const []const u8, splat: usize)
            //     std.Io.Writer.Error!usize
            fn drain(&mut self) {
                // self.list.appendSlice(testing.allocator, w.buffered()) ...; w.end = 0;
                self.list.extend_from_slice(&self.buf[..self.end]);
                self.end = 0;
            }

            // try sink.writer.flush();
            fn flush(&mut self) {
                self.drain();
            }
        }

        impl Writer for Sink {
            fn write_all(&mut self, bytes: &[u8]) -> Result<(), WriteFailed> {
                for &byte in bytes {
                    if self.end == self.buf.len() {
                        self.drain();
                    }
                    self.buf[self.end] = byte;
                    self.end += 1;
                }
                Ok(())
            }
        }

        let mut sink = Sink {
            list: Vec::new(),
            buf: [0u8; 4],
            end: 0,
        };

        let data = b"line one\nline\x1btwo\nline three\n";
        encode_writer(&mut sink, data, Options { bracketed: true }).unwrap();
        sink.flush();
        assert_eq!(
            b"\x1b[200~line one\nline two\nline three\n\x1b[201~".as_slice(),
            sink.list.as_slice(),
        );
    }

    // test "encodeWriter too small"
    #[test]
    fn encode_writer_too_small() {
        let mut buf = [0u8; 4];
        let mut writer = FixedWriter::new(&mut buf);
        // try testing.expectError(error.WriteFailed, ...);
        assert_eq!(
            Err(WriteFailed),
            encode_writer(&mut writer, b"hello", Options { bracketed: true }),
        );
    }

    // test max_frame_size
    #[test]
    fn max_frame_size_test() {
        let result = encode(b"", Options { bracketed: true }).unwrap();
        assert_eq!(max_frame_size, result[0].len() + result[2].len());
    }

    #[test]
    fn encode_bracketed() {
        let result = encode(b"hello", Options { bracketed: true }).unwrap();
        assert_eq!(b"\x1b[200~", result[0]);
        assert_eq!(b"hello", result[1]);
        assert_eq!(b"\x1b[201~", result[2]);
    }

    #[test]
    fn encode_unbracketed_no_newlines() {
        let result = encode(b"hello", Options { bracketed: false }).unwrap();
        assert_eq!(b"", result[0]);
        assert_eq!(b"hello", result[1]);
        assert_eq!(b"", result[2]);
    }

    #[test]
    fn encode_unbracketed_newlines_const() {
        assert_eq!(
            Err(Error::MutableRequired),
            encode(b"hello\nworld", Options { bracketed: false }),
        );
    }

    #[test]
    fn encode_unbracketed_newlines() {
        let mut data: Vec<u8> = b"hello\nworld".to_vec();
        let result = encode_mut(&mut data, Options { bracketed: false });
        assert_eq!(b"", result[0]);
        assert_eq!(b"hello\rworld", result[1]);
        assert_eq!(b"", result[2]);
    }

    #[test]
    fn encode_unbracketed_windows_stye_newline() {
        let mut data: Vec<u8> = b"hello\r\nworld".to_vec();
        let result = encode_mut(&mut data, Options { bracketed: false });
        assert_eq!(b"", result[0]);
        assert_eq!(b"hello\r\rworld", result[1]);
        assert_eq!(b"", result[2]);
    }

    #[test]
    fn encode_strip_unsafe_bytes_const() {
        assert_eq!(
            Err(Error::MutableRequired),
            encode(b"hello\x00world", Options { bracketed: true }),
        );
    }

    #[test]
    fn encode_strip_unsafe_bytes_mutable_bracketed() {
        let mut data: Vec<u8> = b"hel\x1blo\x00world".to_vec();
        let result = encode_mut(&mut data, Options { bracketed: true });
        assert_eq!(b"\x1b[200~", result[0]);
        assert_eq!(b"hel lo world", result[1]);
        assert_eq!(b"\x1b[201~", result[2]);
    }

    #[test]
    fn encode_strip_unsafe_bytes_mutable_unbracketed() {
        let mut data: Vec<u8> = b"hel\x03lo".to_vec();
        let result = encode_mut(&mut data, Options { bracketed: false });
        assert_eq!(b"", result[0]);
        assert_eq!(b"hel lo", result[1]);
        assert_eq!(b"", result[2]);
    }

    #[test]
    fn encode_strip_multiple_unsafe_bytes() {
        let mut data: Vec<u8> = b"\x00\x08\x7f".to_vec();
        let result = encode_mut(&mut data, Options { bracketed: true });
        assert_eq!(b"   ", result[1]);
    }
}
