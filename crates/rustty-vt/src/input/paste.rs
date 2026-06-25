//! rustty-vt input/paste
//!
//! PORTS: ghostty src/input/paste.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports
//   const std = @import("std");
//   const Terminal = @import("../terminal/Terminal.zig");
// `std` is used for slice scanning helpers (reproduced inline below) and
// `Terminal` for `Options.fromTerminal`.

use crate::terminal::Terminal;

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
        result[0] = b"\x1b[200~";
        result[2] = b"\x1b[201~";
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
        // `.{ "\x1b[200~", data, "\x1b[201~" }`. We reborrow `data` immutably
        // here because the mutation above is already complete.
        return [b"\x1b[200~", data, b"\x1b[201~"];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_safe_test() {
        assert!(is_safe(b"hello"));
        assert!(!is_safe(b"hello\n"));
        assert!(!is_safe(b"hello\nworld"));
        assert!(!is_safe(b"he\x1b[201~llo"));
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
