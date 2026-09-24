//! Kitty clipboard protocol (OSC 5522) request decoding.
//!
//! OSC 5522 format: `<OSC>5522;metadata;payload<ST>`
//!
//! Per the spec: "metadata is a colon separated list of key-value
//! pairs and payload is base64 encoded data."
//!
//! This contains the logic for parsing this.
//!
//! PORTS: ghostty src/terminal/kitty/clipboard_command.zig (B5 end state:
//! `25b1170d4` command parsing, amended by `928c7f0e7` readPromptExempt,
//! `e8d8945b5` text-input validation to spec, `bc2f7d7d2` strict base64)

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const Allocator = std.mem.Allocator;
//   const simd = @import("../../simd/main.zig");
//   const clipboard = @import("../clipboard.zig");
//   const protocol = @import("../osc/parsers/kitty_clipboard_protocol.zig");
// `std` facilities are reproduced inline. `Allocator` is the runtime allocator
// handle (parity tail S7-4): every decoded value is allocated through the
// allocator given to `parse`/`init`, fallibly.
#![allow(non_upper_case_globals)]

use crate::alloc::{self as valloc, Allocator, Bytes, OutOfMemory, TryPushExt};
use crate::clipboard;
use crate::osc::parsers::kitty_clipboard_protocol as protocol;
use rustty_simd as simd;

// const Operation = protocol.Operation;
pub(crate) use protocol::Operation;

/// Maximum id length, nothing specified but this is the limit used
/// in Kitty's source so we'll match it.
pub const max_id_len: usize = 512;

/// Maximum decoded password length. Kitty has no limit. Passwords are
/// UUID-sized in practice so anything longer simply never matches a
/// stored grant anyway.
pub const max_pw_len: usize = 128;

/// Maximum decoded MIME type length. Kitty has no limit but real MIME
/// types are tiny; anything longer drops the packet.
pub const max_mime_len: usize = 256;

/// Maximum decoded name length. Kitty has no limit but names are shown
/// in permission prompts so anything longer drops the packet.
pub const max_name_len: usize = 256;

/// `std.base64.standard.Encoder.calcSize(n)` — the encoded length of `n`
/// raw bytes under the padded standard alphabet.
// PORT NOTE: reproduced inline (the reference reaches it through std).
pub(crate) const fn b64_calc_size(n: usize) -> usize {
    n.div_ceil(3) * 4
}

/// `Metadata.parse`'s error set: `error{ OutOfMemory, InvalidValue }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    OutOfMemory,
    InvalidValue,
}

/// The decoded, validated metadata of one OSC 5522 sequence.
///
/// All slice values are allocated from the allocator given to parse and
/// are sized to their contents. Callers are expected to pass an arena
/// scoped to handling the packet. There is no deinit.
// PORT NOTE (parity tail S7-4): the `[]const u8` values are owned `Bytes`
// allocated through the allocator given to `parse` (the stream handler's
// packet-scoped arena over the terminal's `gpa()` in the reference; each
// value is its own allocation here, a T3 call-shape residual). They free on
// drop, so "no deinit" holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Metadata {
    pub op: Operation,

    /// The clipboard this operation targets. Per the spec: "To read
    /// from the primary selection instead of the clipboard, add the
    /// key `loc=primary` to the metadata section." Any other value
    /// means the clipboard, so only standard and primary are possible
    /// here.
    // loc: clipboard.Location = .standard,
    pub loc: clipboard::Location,

    /// Sanitized id: invalid characters stripped, truncated to
    /// max_id_len. Empty means no id.
    // id: []const u8 = "",
    pub id: Bytes,

    /// Decoded mime metadata value. Empty means absent; kitty treats an
    /// empty mime the same as a missing one everywhere it matters (a
    /// wdata packet with either commits the transaction).
    // mime: []const u8 = "",
    pub mime: Bytes,

    /// Decoded password. Empty means absent. Per the spec:
    /// "Specifying a password without a human friendly name is
    /// equivalent to not specifying a password and the terminal must
    /// treat the request as though it had no password."
    // pw: []const u8 = "",
    pub pw: Bytes,

    /// Decoded human friendly name of the requesting program, shown in
    /// permission prompts. Empty means absent. Its presence opts into
    /// password grants.
    // name: []const u8 = "",
    pub name: Bytes,
}

impl Metadata {
    /// The reference's struct-literal defaults (every field but `op`); Rust
    /// callers spell `.{ .op = .write, .id = "42" }` as
    /// `Metadata { id: bytes(b"42"), ..Metadata::new(Operation::Write) }`.
    /// The empty defaults allocate nothing.
    pub fn new(op: Operation) -> Metadata {
        Metadata {
            op,
            loc: clipboard::Location::Standard,
            id: Bytes::new_in(valloc::Global),
            mime: Bytes::new_in(valloc::Global),
            pw: Bytes::new_in(valloc::Global),
            name: Bytes::new_in(valloc::Global),
        }
    }

    /// Parse the metadata field. The raw value is expected to be exactly
    /// the metadata (prefix and payload and separators stripped out).
    ///
    /// A null result means the packet should be silently dropped. An
    /// InvalidValue error means a decoded textual value was not valid
    /// UTF-8 or exceeded a local safety limit. Callers use the raw
    /// operation to decide whether that invalid value aborts an in-flight
    /// write transaction.
    // pub fn parse(
    //     alloc: Allocator,
    //     raw: []const u8,
    // ) error{ OutOfMemory, InvalidValue }!?Metadata {
    pub fn parse(alloc: Allocator, raw: &[u8]) -> Result<Option<Metadata>, ParseError> {
        // const fields = Raw.parse(raw) orelse return null;
        let fields = match Raw::parse(raw) {
            Some(fields) => fields,
            None => return Ok(None),
        };
        // var result: Metadata = .{
        //     .op = Operation.init(fields.op orelse return null) orelse return null,
        //     .loc = ...,
        //     .id = try sanitizeId(alloc, fields.id),
        // };
        let op = match fields.op {
            Some(op_bytes) => match Operation::init(op_bytes) {
                Some(op) => op,
                None => return Ok(None),
            },
            None => return Ok(None),
        };
        let mut result = Metadata {
            op,
            loc: if fields.loc == b"primary" {
                clipboard::Location::Primary
            } else {
                clipboard::Location::Standard
            },
            id: sanitize_id(alloc, fields.id).map_err(|_| ParseError::OutOfMemory)?,
            mime: Bytes::new_in(alloc),
            pw: Bytes::new_in(alloc),
            name: Bytes::new_in(alloc),
        };

        // result.mime = decodeValue(alloc, fields.mime, max_mime_len) catch ...
        result.mime = match decode_value(alloc, fields.mime, max_mime_len) {
            Ok(v) => v,
            Err(DecodeValueError::OutOfMemory) => return Err(ParseError::OutOfMemory),
            Err(
                DecodeValueError::Overflow
                | DecodeValueError::InvalidBase64
                | DecodeValueError::InvalidUtf8,
            ) => return Err(ParseError::InvalidValue),
        };
        result.pw = match decode_value(alloc, fields.pw, max_pw_len) {
            Ok(v) => v,
            Err(DecodeValueError::OutOfMemory) => return Err(ParseError::OutOfMemory),
            Err(DecodeValueError::InvalidBase64 | DecodeValueError::InvalidUtf8) => {
                return Err(ParseError::InvalidValue)
            }
            // An over-long password behaves as if none was given: it can
            // never match a stored grant.
            Err(DecodeValueError::Overflow) => Bytes::new_in(alloc),
        };
        result.name = match decode_value(alloc, fields.name, max_name_len) {
            Ok(v) => v,
            Err(DecodeValueError::OutOfMemory) => return Err(ParseError::OutOfMemory),
            Err(
                DecodeValueError::Overflow
                | DecodeValueError::InvalidBase64
                | DecodeValueError::InvalidUtf8,
            ) => return Err(ParseError::InvalidValue),
        };
        Ok(Some(result))
    }

    /// Return the last recognized operation from syntactically valid raw
    /// metadata. This intentionally does not decode any values, so callers
    /// can still classify an InvalidValue parse error.
    pub fn operation(raw: &[u8]) -> Option<Operation> {
        // const fields = Raw.parse(raw) orelse return null;
        // return Operation.init(fields.op orelse return null);
        let fields = Raw::parse(raw)?;
        Operation::init(fields.op?)
    }
}

// const Raw = struct { ... };
#[derive(Default)]
struct Raw<'a> {
    op: Option<&'a [u8]>,
    loc: &'a [u8],
    id: &'a [u8],
    mime: &'a [u8],
    pw: &'a [u8],
    name: &'a [u8],
}

impl<'a> Raw<'a> {
    fn parse(raw: &'a [u8]) -> Option<Raw<'a>> {
        let mut result = Raw {
            op: None,
            loc: b"",
            id: b"",
            mime: b"",
            pw: b"",
            name: b"",
        };

        // This visits every record even though an empty raw string
        // yields one empty record. Validating the complete structure
        // before decoding also ensures the last duplicate value wins.
        // var it = std.mem.splitScalar(u8, raw, ':');
        for record in raw.split(|&b| b == b':') {
            // const eql_idx = std.mem.indexOfScalar(u8, record, '=') orelse return null;
            let eql_idx = record.iter().position(|&b| b == b'=')?;
            let key = &record[0..eql_idx];
            let value = &record[eql_idx + 1..];

            if key == b"type" {
                result.op = Some(value);
            } else if key == b"loc" {
                result.loc = value;
            } else if key == b"id" {
                result.id = value;
            } else if key == b"mime" {
                result.mime = value;
            } else if key == b"pw" {
                result.pw = value;
            } else if key == b"name" {
                result.name = value;
            }
            // Unknown keys are ignored.
        }

        Some(result)
    }
}

/// Sanitize the ID according to the spec:
///
/// Valid ids must include only characters from the set: [a-zA-Z0-9-_+.].
/// Any other characters must be stripped out from the id by the terminal
/// emulator before retransmitting it.
// fn sanitizeId(alloc: Allocator, value: []const u8) Allocator.Error![]const u8 {
// PORT NOTE: `defer list.deinit(alloc)` + `toOwnedSlice` hand the grown list
// over as the result (the list itself, not a shrunk copy: a T3 residual).
fn sanitize_id(alloc: Allocator, value: &[u8]) -> Result<Bytes, OutOfMemory> {
    let mut list: Bytes = Bytes::new_in(alloc);
    for &c in value {
        match c {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'+' | b'.' => {}
            _ => continue,
        }
        if list.len() >= max_id_len {
            break;
        }
        // try list.append(alloc, c);
        list.try_push(c)?;
    }
    Ok(list)
}

/// `decodeValue`'s error set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecodeValueError {
    OutOfMemory,
    Overflow,
    InvalidBase64,
    InvalidUtf8,
}

/// Base64-decode a metadata value. Per the spec these values "are
/// UTF-8 strings that are base64 encoded", so the decoded result
/// must be valid UTF-8, and the encoding is strict RFC 4648 with
/// required padding per the spec's "Encoding of payloads" section.
// fn decodeValue(alloc: Allocator, value: []const u8, max_len: usize) error{
//     OutOfMemory, Overflow, InvalidBase64, InvalidUtf8,
// }![]const u8 {
fn decode_value(alloc: Allocator, value: &[u8], max_len: usize) -> Result<Bytes, DecodeValueError> {
    // const Encoder = std.base64.standard.Encoder;
    //
    // Avoid hostile large payloads.
    // if (value.len > Encoder.calcSize(max_len)) return error.Overflow;
    if value.len() > b64_calc_size(max_len) {
        return Err(DecodeValueError::Overflow);
    }

    // Decode
    // const buf = try alloc.alloc(u8, simd.base64.maxLen(value));
    // errdefer alloc.free(buf);
    let mut buf = alloc_buf(alloc, simd::base64::max_len(value))
        .map_err(|_| DecodeValueError::OutOfMemory)?;
    let decoded_len = simd::base64::decode_strict(value, &mut buf, simd::base64::Padding::Required)
        .map_err(|_| DecodeValueError::InvalidBase64)?
        .len();
    buf.truncate(decoded_len);

    // Must be valid UTF-8
    // if (!std.unicode.utf8ValidateSlice(decoded)) return error.InvalidUtf8;
    if core::str::from_utf8(&buf).is_err() {
        return Err(DecodeValueError::InvalidUtf8);
    }
    if buf.len() > max_len {
        return Err(DecodeValueError::Overflow);
    }
    Ok(buf)
}

/// `alloc.alloc(u8, n)`: one exact-sized, fallible allocation through
/// `alloc`, zero-filled (Zig's is `undefined`; the decoders overwrite what
/// they report). `n == 0` allocates nothing.
pub(crate) fn alloc_buf(alloc: Allocator, n: usize) -> Result<Bytes, OutOfMemory> {
    use crate::alloc::CopyVecExt;
    let mut buf = Bytes::new_in(alloc);
    buf.try_reserve_exact(n).map_err(|_| OutOfMemory)?;
    // The capacity is reserved, so this never allocates.
    buf.resize_copy(n, 0);
    Ok(buf)
}

/// `Payload.init`'s error set: `error{ OutOfMemory, Invalid }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayloadError {
    OutOfMemory,
    Invalid,
}

/// A decoded base64 payload of one OSC 5522 sequence, e.g. the MIME
/// type list of a read request or the alias list of a walias packet.
/// The data slice aliases the allocated buf.
// pub const Payload = struct { buf: []u8, data: []const u8, ... };
// PORT NOTE: the reference keeps the oversized decode buffer (`buf`) and the
// decoded prefix (`data`) as two slices where `data` aliases `buf`; the port
// stores the owned buffer plus the decoded length and exposes the alias as
// the `data()` accessor — no self-referential borrow.
pub struct Payload {
    buf: Bytes,
    len: usize,
}

impl Payload {
    /// Decode a base64 payload into freshly allocated memory. The
    /// encoding is strict RFC 4648 with required padding per the
    /// spec's "Encoding of payloads" section; how an invalid payload
    /// is reported (or not) depends on the packet type.
    // pub fn init(alloc: Allocator, payload: []const u8) error{ OutOfMemory, Invalid }!Payload {
    pub fn init(alloc: Allocator, payload: &[u8]) -> Result<Payload, PayloadError> {
        // const buf = try alloc.alloc(u8, simd.base64.maxLen(payload));
        // errdefer alloc.free(buf);
        let mut buf = alloc_buf(alloc, simd::base64::max_len(payload))
            .map_err(|_| PayloadError::OutOfMemory)?;
        // const data = simd.base64.decodeStrict(payload, buf, .required) catch return error.Invalid;
        let len = simd::base64::decode_strict(payload, &mut buf, simd::base64::Padding::Required)
            .map_err(|_| PayloadError::Invalid)?
            .len();
        Ok(Payload { buf, len })
    }

    // pub fn deinit(self: *const Payload, alloc: Allocator) void { alloc.free(self.buf); }
    // PORT NOTE: `buf` frees through the allocator it was allocated with on
    // drop; `deinit` consumes the payload and checks that allocator.
    pub fn deinit(self, alloc: Allocator) {
        self.buf.allocator().debug_assert_same(alloc);
    }

    /// The decoded bytes (the reference's `data` field).
    pub fn data(&self) -> &[u8] {
        &self.buf[..self.len]
    }

    pub fn is_valid_utf8(&self) -> bool {
        // return std.unicode.utf8ValidateSlice(self.data);
        core::str::from_utf8(self.data()).is_ok()
    }

    /// Iterate the whitespace-separated MIME types of the payload.
    /// Matches Python str.split() used by kitty.
    // pub fn mimeIterator(self: *const Payload) std.mem.TokenIterator(u8, .any) {
    //     return std.mem.tokenizeAny(u8, self.data, &std.ascii.whitespace);
    // }
    pub fn mime_iterator(&self) -> MimeIterator<'_> {
        MimeIterator {
            data: self.data(),
            index: 0,
        }
    }
}

/// `std.ascii.whitespace` — note it includes vertical tab (0x0B), which
/// Rust's `u8::is_ascii_whitespace` does not; the set is spelled out for
/// fidelity.
const ascii_whitespace: &[u8] = b" \t\n\r\x0B\x0C";

/// `std.mem.TokenIterator(u8, .any)` over the payload bytes: skips runs of
/// delimiter bytes and yields the non-delimiter runs, with the reference's
/// `reset` rewind.
pub struct MimeIterator<'a> {
    data: &'a [u8],
    index: usize,
}

impl<'a> MimeIterator<'a> {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&'a [u8]> {
        // Skip delimiters.
        while self.index < self.data.len() && ascii_whitespace.contains(&self.data[self.index]) {
            self.index += 1;
        }
        if self.index >= self.data.len() {
            return None;
        }
        let start = self.index;
        while self.index < self.data.len() && !ascii_whitespace.contains(&self.data[self.index]) {
            self.index += 1;
        }
        Some(&self.data[start..self.index])
    }

    // pub fn reset(self: *TokenIterator) void
    pub fn reset(&mut self) {
        self.index = 0;
    }
}

/// Whether a read request with this many data MIME types (the targets
/// type '.' excluded) is exempt from the user permission prompt.
// pub fn readPromptExempt(data_mimes: usize) bool {
pub fn read_prompt_exempt(data_mimes: usize) -> bool {
    data_mimes == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: the reference tests parse into a per-test arena; the port's
    // `Metadata` owns its values, so the arena scaffolding has no analog.

    // test "metadata: empty is dropped"
    #[test]
    fn metadata_empty_is_dropped() {
        assert!(Metadata::parse(crate::alloc::Global, b"")
            .unwrap()
            .is_none());
    }

    // test "metadata: record without = is dropped"
    #[test]
    fn metadata_record_without_eql_is_dropped() {
        assert!(Metadata::parse(crate::alloc::Global, b"type=read:bare")
            .unwrap()
            .is_none());
        assert!(Metadata::parse(crate::alloc::Global, b"bare:type=read")
            .unwrap()
            .is_none());
    }

    // test "metadata: missing or unknown type is dropped"
    #[test]
    fn metadata_missing_or_unknown_type_is_dropped() {
        assert!(Metadata::parse(crate::alloc::Global, b"loc=primary")
            .unwrap()
            .is_none());
        assert!(Metadata::parse(crate::alloc::Global, b"type=bobr")
            .unwrap()
            .is_none());
        assert!(Metadata::parse(crate::alloc::Global, b"type=")
            .unwrap()
            .is_none());
    }

    // test "metadata: duplicate keys keep the last occurrence"
    #[test]
    fn metadata_duplicate_keys_keep_the_last_occurrence() {
        assert_eq!(
            Operation::Read,
            Metadata::parse(crate::alloc::Global, b"type=bobr:type=read")
                .unwrap()
                .unwrap()
                .op,
        );
        assert!(
            Metadata::parse(crate::alloc::Global, b"type=read:type=bobr")
                .unwrap()
                .is_none()
        );
    }

    // test "metadata: basic read"
    #[test]
    fn metadata_basic_read() {
        let meta = Metadata::parse(crate::alloc::Global, b"type=read")
            .unwrap()
            .unwrap();
        assert_eq!(Operation::Read, meta.op);
        assert!(meta.loc == crate::clipboard::Location::Standard);
        assert_eq!(0usize, meta.id.len());
    }

    // test "metadata: unknown keys ignored"
    #[test]
    fn metadata_unknown_keys_ignored() {
        let meta = Metadata::parse(crate::alloc::Global, b"type=read:bobr=kurwa")
            .unwrap()
            .unwrap();
        assert_eq!(Operation::Read, meta.op);
    }

    // test "metadata: loc"
    #[test]
    fn metadata_loc() {
        assert!(
            Metadata::parse(crate::alloc::Global, b"type=read:loc=primary")
                .unwrap()
                .unwrap()
                .loc
                == crate::clipboard::Location::Primary
        );
        // Anything other than "primary" means the clipboard; it is not an
        // error.
        assert!(
            Metadata::parse(crate::alloc::Global, b"type=read:loc=bobr")
                .unwrap()
                .unwrap()
                .loc
                == crate::clipboard::Location::Standard
        );
    }

    // test "metadata: id sanitized"
    #[test]
    fn metadata_id_sanitized() {
        {
            let meta = Metadata::parse(crate::alloc::Global, b"type=read:id=abc-123_x.Y+z")
                .unwrap()
                .unwrap();
            assert_eq!(b"abc-123_x.Y+z".as_slice(), meta.id.as_slice());
        }
        {
            // Invalid characters are stripped, not rejected.
            let meta = Metadata::parse(crate::alloc::Global, b"type=read:id=*4 2*")
                .unwrap()
                .unwrap();
            assert_eq!(b"42".as_slice(), meta.id.as_slice());
        }
    }

    // test "metadata: id truncated to max"
    #[test]
    fn metadata_id_truncated_to_max() {
        // const raw = "type=read:id=" ++ "a" ** (max_id_len + 100);
        let mut raw = b"type=read:id=".to_vec();
        raw.extend(core::iter::repeat_n(b'a', max_id_len + 100));
        let meta = Metadata::parse(crate::alloc::Global, &raw)
            .unwrap()
            .unwrap();
        assert_eq!(max_id_len, meta.id.len());
    }

    // test "metadata: mime decoded"
    #[test]
    fn metadata_mime_decoded() {
        // "text/plain"
        let meta = Metadata::parse(crate::alloc::Global, b"type=wdata:mime=dGV4dC9wbGFpbg==")
            .unwrap()
            .unwrap();
        assert_eq!(b"text/plain".as_slice(), meta.mime.as_slice());
    }

    // test "metadata: invalid mime base64 reported"
    #[test]
    fn metadata_invalid_mime_base64_reported() {
        // Invalid base64 in a metadata value aborts an in-flight write
        // per the spec, so like invalid UTF-8 it is reported rather than
        // silently dropping the packet. Unpadded or whitespace-laced
        // values are invalid too: metadata values use the same strict
        // encoding as payloads.
        let cases: [&[u8]; 4] = [
            b"type=wdata:mime=!!!",
            // "text/plain" without its padding.
            b"type=wdata:mime=dGV4dC9wbGFpbg",
            // "text/plain" with its final byte replaced by '!'.
            b"type=wdata:mime=dGV4dC9wbGFpbg=!",
            // A newline inside otherwise valid base64.
            b"type=wdata:mime=dGV4dC9w\nbGFpbg==",
        ];
        for case in cases {
            assert_eq!(
                Err(ParseError::InvalidValue),
                Metadata::parse(crate::alloc::Global, case)
            );
            assert_eq!(Operation::Wdata, Metadata::operation(case).unwrap());
        }
    }

    // test "metadata: invalid mime utf8 reported"
    #[test]
    fn metadata_invalid_mime_utf8_reported() {
        // base64 of 0xff 0xfe
        assert_eq!(
            Err(ParseError::InvalidValue),
            Metadata::parse(crate::alloc::Global, b"type=wdata:mime=//4="),
        );
        assert_eq!(
            Operation::Wdata,
            Metadata::operation(b"type=wdata:mime=//4=").unwrap(),
        );
    }

    // test "metadata: pw and name"
    #[test]
    fn metadata_pw_and_name() {
        // pw="secret", name="app"
        let meta = Metadata::parse(crate::alloc::Global, b"type=read:pw=c2VjcmV0:name=YXBw")
            .unwrap()
            .unwrap();
        assert_eq!(b"secret".as_slice(), meta.pw.as_slice());
        assert_eq!(b"app".as_slice(), meta.name.as_slice());
    }

    // test "metadata: over-long name reported"
    #[test]
    fn metadata_over_long_name_reported() {
        // const Encoder = std.base64.standard.Encoder;
        // const long = "n" ** (max_name_len + 1);
        let long = vec![b'n'; max_name_len + 1];
        let mut raw = b"type=read:name=".to_vec();
        raw.extend_from_slice(&crate::kitty::clipboard_response::b64_encode_to_vec(&long));
        assert_eq!(
            Err(ParseError::InvalidValue),
            Metadata::parse(crate::alloc::Global, &raw)
        );
    }

    // test "metadata: empty name"
    #[test]
    fn metadata_empty_name() {
        let meta = Metadata::parse(crate::alloc::Global, b"type=read:pw=c2VjcmV0:name=")
            .unwrap()
            .unwrap();
        assert_eq!(0usize, meta.name.len());
    }

    // test "read prompt exemption: only requests without data types"
    #[test]
    fn read_prompt_exemption_only_requests_without_data_types() {
        // A targets-only ('.') read parses to zero data MIME types and is
        // served without a prompt; any data type, even alongside the
        // targets listing, still prompts.
        assert!(read_prompt_exempt(0));
        assert!(!read_prompt_exempt(1));
    }

    // test "payload: mime iterator"
    #[test]
    fn payload_mime_iterator() {
        // base64 of "text/plain  text/html\ntext/uri-list"
        let payload = Payload::init(
            crate::alloc::Global,
            b"dGV4dC9wbGFpbiAgdGV4dC9odG1sCnRleHQvdXJpLWxpc3Q=",
        )
        .unwrap();
        let mut it = payload.mime_iterator();
        assert_eq!(b"text/plain".as_slice(), it.next().unwrap());
        assert_eq!(b"text/html".as_slice(), it.next().unwrap());
        assert_eq!(b"text/uri-list".as_slice(), it.next().unwrap());
        assert!(it.next().is_none());
    }

    // test "payload: invalid base64"
    #[test]
    fn payload_invalid_base64() {
        let cases: [&[u8]; 3] = [
            b"!!!",
            // "text/plain" without its padding: payloads use the strict
            // encoding, so missing padding is rejected.
            b"dGV4dC9wbGFpbg",
            // A newline inside otherwise valid base64.
            b"dGV4dC9w\nbGFpbg==",
        ];
        for case in cases {
            assert!(matches!(
                Payload::init(crate::alloc::Global, case),
                Err(PayloadError::Invalid)
            ));
        }
    }

    // test "payload: decoded text must be valid utf8"
    #[test]
    fn payload_decoded_text_must_be_valid_utf8() {
        // Valid base64 encoding of a single 0xff byte.
        let payload = Payload::init(crate::alloc::Global, b"/w==").unwrap();
        assert!(!payload.is_valid_utf8());
    }
}
