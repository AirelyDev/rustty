//! Kitty clipboard protocol (OSC 5522) response encoding.
//!
//! PORTS: ghostty src/terminal/kitty/clipboard_response.zig (B5/B7 end state:
//! `e28acd928` response encoding, `a5bb22e23` PasteEvent)

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const clipboard = @import("../clipboard.zig");
//   const oscpkg = @import("../osc.zig");
//   const protocol = @import("../osc/parsers/kitty_clipboard_protocol.zig");
// `*std.Io.Writer` maps to the crate's byte-oriented stand-in,
// `snapshot::io::Writer` (the paste-core precedent), and
// `std.base64.standard.Encoder` is reproduced inline (`b64_encode_writer`).
#![allow(non_upper_case_globals)]

use crate::clipboard;
use crate::osc::parsers::kitty_clipboard_protocol as protocol;
use crate::osc::Terminator;
use crate::snapshot::io::{FixedWriter, WriteFailed, Writer};

// const b64 = std.base64.standard.Encoder;
// const Content = clipboard.Content;
use clipboard::Content;
// const Operation = protocol.Operation;
// const Status = protocol.Status;
use protocol::{Operation, Status};

/// Maximum raw (pre-base64) bytes per DATA packet in read responses.
/// This is specified by the protocol.
pub const read_chunk_size: usize = 4096;

/// Maximum requested MIME types served by a single read request.
/// Requests beyond this simply see no DATA packets for the extras,
/// which is how the protocol communicates an unavailable type anyway.
pub const max_read_mimes: usize = 4;

/// The special MIME type that requests the list of available types.
pub const targets_mime: &[u8] = b".";

/// Maximum MIME types reported in a paste event's targets listing.
pub const max_listing_mimes: usize = 16;

/// The standard base64 alphabet (`std.base64.standard`).
const b64_alphabet: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `std.base64.standard.Encoder.encodeWriter` — encode `data` with required
/// padding, streaming into `writer`.
pub(crate) fn b64_encode_writer(writer: &mut dyn Writer, data: &[u8]) -> Result<(), WriteFailed> {
    for group in data.chunks(3) {
        let mut out = [b'='; 4];
        out[0] = b64_alphabet[(group[0] >> 2) as usize];
        match group.len() {
            1 => {
                out[1] = b64_alphabet[((group[0] & 0x03) << 4) as usize];
            }
            2 => {
                out[1] = b64_alphabet[(((group[0] & 0x03) << 4) | (group[1] >> 4)) as usize];
                out[2] = b64_alphabet[((group[1] & 0x0F) << 2) as usize];
            }
            _ => {
                out[1] = b64_alphabet[(((group[0] & 0x03) << 4) | (group[1] >> 4)) as usize];
                out[2] = b64_alphabet[(((group[1] & 0x0F) << 2) | (group[2] >> 6)) as usize];
                out[3] = b64_alphabet[(group[2] & 0x3F) as usize];
            }
        }
        writer.write_all(&out)?;
    }
    Ok(())
}

/// `std.base64.standard.Encoder.encode` into an owned buffer — the test-side
/// convenience over `b64_encode_writer` (the reference tests reach the std
/// Encoder directly).
#[cfg(test)]
pub(crate) fn b64_encode_to_vec(data: &[u8]) -> Vec<u8> {
    let mut aw = crate::snapshot::io::AllocatingWriter::new();
    b64_encode_writer(&mut aw, data).unwrap();
    aw.written().to_vec()
}

/// `@tagName` of an `Operation` (the `{t}` format in `encodeMetadata`).
fn op_tag_name(op: Operation) -> &'static [u8] {
    match op {
        Operation::Read => b"read",
        Operation::Walias => b"walias",
        Operation::Wdata => b"wdata",
        Operation::Write => b"write",
    }
}

/// `@tagName` of a `Status` (the `{t}` format in `encodeMetadata`).
fn status_tag_name(status: Status) -> &'static [u8] {
    match status {
        Status::Data => b"DATA",
        Status::Done => b"DONE",
        Status::Ebusy => b"EBUSY",
        Status::Efbig => b"EFBIG",
        Status::Einval => b"EINVAL",
        Status::Eio => b"EIO",
        Status::Enosys => b"ENOSYS",
        Status::Eperm => b"EPERM",
        Status::Ok => b"OK",
    }
}

/// A single response packet.
#[derive(Clone, Copy, Debug)]
pub struct Response<'a> {
    pub op: Operation,
    pub status: Status,
    // primary: bool = false,
    pub primary: bool,
    // id: []const u8 = "",
    pub id: &'a [u8],
    // mime: ?[]const u8 = null,
    pub mime: Option<&'a [u8]>,
    // pw: ?[]const u8 = null,
    pub pw: Option<&'a [u8]>,
    /// The raw payload; the encoder base64-encodes it. An empty payload
    /// emits no payload section at all (no ';').
    // payload: []const u8 = "",
    pub payload: &'a [u8],
    // terminator: Terminator = .st,
    pub terminator: Terminator,
}

impl<'a> Response<'a> {
    /// The reference's struct-literal defaults (every field but `op` and
    /// `status`); callers use struct-update syntax over this.
    pub fn new(op: Operation, status: Status) -> Response<'a> {
        Response {
            op,
            status,
            primary: false,
            id: b"",
            mime: None,
            pw: None,
            payload: b"",
            terminator: Terminator::St,
        }
    }

    /// Encode the response. Errors may result in partially written data
    /// so it is up to callers to buffer it if they need to.
    // pub fn encode(self: *const Response, writer: *std.Io.Writer) std.Io.Writer.Error!void {
    pub fn encode(&self, writer: &mut dyn Writer) -> Result<(), WriteFailed> {
        self.encode_metadata(writer)?;
        if !self.payload.is_empty() {
            writer.write_all(b";")?;
            b64_encode_writer(writer, self.payload)?;
        }
        writer.write_all(self.terminator.string())
    }

    /// Encode the escape prefix and metadata section only: everything
    /// up to (and not including) the payload section and terminator.
    fn encode_metadata(&self, writer: &mut dyn Writer) -> Result<(), WriteFailed> {
        // The exact order of fields here matches what Kitty does.
        // try writer.print("\x1b]5522;type={t}:status={t}", .{ self.op, self.status });
        writer.write_all(b"\x1b]5522;type=")?;
        writer.write_all(op_tag_name(self.op))?;
        writer.write_all(b":status=")?;
        writer.write_all(status_tag_name(self.status))?;
        if self.primary {
            writer.write_all(b":loc=primary")?;
        }
        if !self.id.is_empty() {
            // try writer.print(":id={s}", .{self.id});
            writer.write_all(b":id=")?;
            writer.write_all(self.id)?;
        }
        if let Some(mime) = self.mime {
            writer.write_all(b":mime=")?;
            b64_encode_writer(writer, mime)?;
        }
        if let Some(pw) = self.pw {
            writer.write_all(b":pw=")?;
            b64_encode_writer(writer, pw)?;
        }
        Ok(())
    }
}

/// Encode a full successful read response: the OK packet, the targets
/// listing if requested, DATA chunks for each served representation,
/// and the final DONE packet. This is also the shape of an unsolicited
/// paste event (list=true, pw set to the one-time password).
#[derive(Clone, Copy, Debug)]
pub struct ReadSuccess<'a> {
    // primary: bool = false,
    pub primary: bool,
    // id: []const u8 = "",
    pub id: &'a [u8],

    /// One-time password echoed in every packet. Only used for
    /// terminal-initiated paste events.
    // pw: ?[]const u8 = null,
    pub pw: Option<&'a [u8]>,

    /// True when the targets ('.') listing was requested.
    // list: bool = false,
    pub list: bool,

    /// The MIME types available on the clipboard, reported by the
    /// targets listing. Kitty reports these space-separated in one
    /// packet with a trailing newline when non-empty.
    // available: []const []const u8 = &.{},
    pub available: &'a [&'a [u8]],

    /// The representations to serve, in request order. Each entry's
    /// data is chunked into DATA packets under its own MIME type. An
    /// entry with empty data produces no packets, which is how the
    /// protocol communicates an unavailable type.
    // contents: []const Content = &.{},
    pub contents: &'a [Content<'a>],

    // terminator: Terminator = .st,
    pub terminator: Terminator,
}

impl Default for ReadSuccess<'_> {
    // (Zig struct-literal field defaults.)
    fn default() -> Self {
        ReadSuccess {
            primary: false,
            id: b"",
            pw: None,
            list: false,
            available: &[],
            contents: &[],
            terminator: Terminator::St,
        }
    }
}

impl ReadSuccess<'_> {
    pub fn encode(&self, writer: &mut dyn Writer) -> Result<(), WriteFailed> {
        // Initial read response
        Response {
            primary: self.primary,
            id: self.id,
            pw: self.pw,
            terminator: self.terminator,
            ..Response::new(Operation::Read, Status::Ok)
        }
        .encode(writer)?;

        // Listing of mimes if requested
        if self.list {
            self.encode_listing(writer)?;
        }

        // Encoding of each mime-type + content
        for content in self.contents {
            let mut i: usize = 0;
            while i < content.data.len() {
                // const n = @min(content.data.len - i, read_chunk_size);
                let n = (content.data.len() - i).min(read_chunk_size);
                Response {
                    id: self.id,
                    mime: Some(content.mime),
                    pw: self.pw,
                    payload: &content.data[i..][0..n],
                    terminator: self.terminator,
                    ..Response::new(Operation::Read, Status::Data)
                }
                .encode(writer)?;
                i += n;
            }
        }

        // Trailing done.
        Response {
            id: self.id,
            pw: self.pw,
            terminator: self.terminator,
            ..Response::new(Operation::Read, Status::Done)
        }
        .encode(writer)?;
        Ok(())
    }

    /// Encode the targets ('.') listing packet. The listing gets a
    /// trailing newline when non-empty.
    fn encode_listing(&self, writer: &mut dyn Writer) -> Result<(), WriteFailed> {
        let listing = Response {
            id: self.id,
            mime: Some(targets_mime),
            pw: self.pw,
            terminator: self.terminator,
            ..Response::new(Operation::Read, Status::Data)
        };
        listing.encode_metadata(writer)?;
        if !self.available.is_empty() {
            writer.write_all(b";")?;

            // Join the types into one chunk before encoding. The
            // listing is a DATA packet, so it shares the pre-encoding
            // chunk size bound of any other data packet; types that
            // don't fit are dropped (a listing that large doesn't
            // happen in practice).
            // var raw: [read_chunk_size]u8 = undefined;
            // var fixed: std.Io.Writer = .fixed(&raw);
            let mut raw = [0u8; read_chunk_size];
            let mut fixed = FixedWriter::new(&mut raw);
            for (i, mime) in self.available.iter().enumerate() {
                let sep: usize = if i > 0 { 1 } else { 0 };
                // if (fixed.end + sep + mime.len + "\n".len > raw.len) break;
                if fixed.end + sep + mime.len() + b"\n".len() > read_chunk_size {
                    break;
                }
                if i > 0 {
                    // fixed.writeAll(" ") catch unreachable;
                    fixed.write_all(b" ").unwrap();
                }
                fixed.write_all(mime).unwrap();
            }
            fixed.write_all(b"\n").unwrap();
            b64_encode_writer(writer, fixed.buffered())?;
        }
        writer.write_all(self.terminator.string())
    }
}

/// An unsolicited Kitty paste event (mode 5522): a read response that
/// lists the clipboard's available MIME types and carries the one-time
/// password the program uses for its follow-up read.
#[derive(Clone, Copy, Debug)]
pub struct PasteEvent<'a> {
    /// True if the paste came from the primary selection, reported as
    /// `loc=primary` on the OK packet.
    // primary: bool = false,
    pub primary: bool,

    /// The one-time password, echoed in every packet.
    pub pw: &'a [u8],

    /// The MIME types available on the clipboard.
    pub available: &'a [&'a [u8]],

    // terminator: Terminator = .st,
    pub terminator: Terminator,
}

impl<'a> PasteEvent<'a> {
    /// The reference's struct-literal defaults (every field but `pw` and
    /// `available`); callers use struct-update syntax over this.
    pub fn new(pw: &'a [u8], available: &'a [&'a [u8]]) -> PasteEvent<'a> {
        PasteEvent {
            primary: false,
            pw,
            available,
            terminator: Terminator::St,
        }
    }

    pub fn encode(&self, writer: &mut dyn Writer) -> Result<(), WriteFailed> {
        ReadSuccess {
            primary: self.primary,
            pw: Some(self.pw),
            list: true,
            available: self.available,
            terminator: self.terminator,
            ..ReadSuccess::default()
        }
        .encode(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::io::AllocatingWriter;

    // test "response: basic status packet"
    #[test]
    fn response_basic_status_packet() {
        // var buf: [128]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        let mut buf = [0u8; 128];
        let mut writer = FixedWriter::new(&mut buf);
        Response::new(Operation::Write, Status::Done)
            .encode(&mut writer)
            .unwrap();
        assert_eq!(
            b"\x1b]5522;type=write:status=DONE\x1b\\".as_slice(),
            writer.buffered(),
        );
    }

    // test "response: id echo and terminator"
    #[test]
    fn response_id_echo_and_terminator() {
        let mut buf = [0u8; 128];
        let mut writer = FixedWriter::new(&mut buf);
        Response {
            id: b"42",
            terminator: Terminator::Bel,
            ..Response::new(Operation::Write, Status::Eperm)
        }
        .encode(&mut writer)
        .unwrap();
        assert_eq!(
            b"\x1b]5522;type=write:status=EPERM:id=42\x07".as_slice(),
            writer.buffered(),
        );
    }

    // test "response: key order type,status,loc,id,mime,pw and payload"
    #[test]
    fn response_key_order_type_status_loc_id_mime_pw_and_payload() {
        let mut buf = [0u8; 256];
        let mut writer = FixedWriter::new(&mut buf);
        Response {
            primary: true,
            id: b"x",
            mime: Some(b"text/plain"),
            pw: Some(b"otp"),
            payload: b"Ghostty",
            ..Response::new(Operation::Read, Status::Data)
        }
        .encode(&mut writer)
        .unwrap();
        assert_eq!(
            b"\x1b]5522;type=read:status=DATA:loc=primary:id=x\
              :mime=dGV4dC9wbGFpbg==:pw=b3Rw;R2hvc3R0eQ==\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "read success: empty request is OK then DONE"
    #[test]
    fn read_success_empty_request_is_ok_then_done() {
        let mut buf = [0u8; 256];
        let mut writer = FixedWriter::new(&mut buf);
        ReadSuccess {
            id: b"7",
            ..ReadSuccess::default()
        }
        .encode(&mut writer)
        .unwrap();
        assert_eq!(
            b"\x1b]5522;type=read:status=OK:id=7\x1b\\\
              \x1b]5522;type=read:status=DONE:id=7\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "read success: targets listing with text"
    #[test]
    fn read_success_targets_listing_with_text() {
        let mut buf = [0u8; 512];
        let mut writer = FixedWriter::new(&mut buf);
        ReadSuccess {
            list: true,
            available: &[b"text/plain"],
            ..ReadSuccess::default()
        }
        .encode(&mut writer)
        .unwrap();
        // "." => "Lg==", "text/plain\n" => "dGV4dC9wbGFpbgo="
        assert_eq!(
            b"\x1b]5522;type=read:status=OK\x1b\\\
              \x1b]5522;type=read:status=DATA:mime=Lg==;dGV4dC9wbGFpbgo=\x1b\\\
              \x1b]5522;type=read:status=DONE\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "read success: targets listing joins multiple types"
    #[test]
    fn read_success_targets_listing_joins_multiple_types() {
        let mut buf = [0u8; 512];
        let mut writer = FixedWriter::new(&mut buf);
        ReadSuccess {
            list: true,
            available: &[b"text/plain", b"image/png"],
            ..ReadSuccess::default()
        }
        .encode(&mut writer)
        .unwrap();
        // Payload is base64 of "text/plain image/png\n".
        assert_eq!(
            b"\x1b]5522;type=read:status=OK\x1b\\\
              \x1b]5522;type=read:status=DATA:mime=Lg==;dGV4dC9wbGFpbiBpbWFnZS9wbmcK\x1b\\\
              \x1b]5522;type=read:status=DONE\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "read success: empty targets listing packet still sent"
    #[test]
    fn read_success_empty_targets_listing_packet_still_sent() {
        let mut buf = [0u8; 512];
        let mut writer = FixedWriter::new(&mut buf);
        ReadSuccess {
            list: true,
            ..ReadSuccess::default()
        }
        .encode(&mut writer)
        .unwrap();
        assert_eq!(
            b"\x1b]5522;type=read:status=OK\x1b\\\
              \x1b]5522;type=read:status=DATA:mime=Lg==\x1b\\\
              \x1b]5522;type=read:status=DONE\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "read success: data chunks under requested mime"
    #[test]
    fn read_success_data_chunks_under_requested_mime() {
        let mut buf = [0u8; 512];
        let mut writer = FixedWriter::new(&mut buf);
        ReadSuccess {
            contents: &[Content {
                mime: b"text/plain",
                data: b"Ghostty",
            }],
            ..ReadSuccess::default()
        }
        .encode(&mut writer)
        .unwrap();
        assert_eq!(
            b"\x1b]5522;type=read:status=OK\x1b\\\
              \x1b]5522;type=read:status=DATA:mime=dGV4dC9wbGFpbg==;R2hvc3R0eQ==\x1b\\\
              \x1b]5522;type=read:status=DONE\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "read success: each representation carries its own data"
    #[test]
    fn read_success_each_representation_carries_its_own_data() {
        let mut buf = [0u8; 512];
        let mut writer = FixedWriter::new(&mut buf);
        ReadSuccess {
            contents: &[
                Content {
                    mime: b"text/plain",
                    data: b"hello",
                },
                Content {
                    mime: b"image/png",
                    data: b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a",
                },
            ],
            ..ReadSuccess::default()
        }
        .encode(&mut writer)
        .unwrap();
        assert_eq!(
            b"\x1b]5522;type=read:status=OK\x1b\\\
              \x1b]5522;type=read:status=DATA:mime=dGV4dC9wbGFpbg==;aGVsbG8=\x1b\\\
              \x1b]5522;type=read:status=DATA:mime=aW1hZ2UvcG5n;iVBORw0KGgo=\x1b\\\
              \x1b]5522;type=read:status=DONE\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "read success: chunking at read_chunk_size"
    #[test]
    fn read_success_chunking_at_read_chunk_size() {
        // const data = "z" ** (read_chunk_size + 1);
        let data = vec![b'z'; read_chunk_size + 1];
        // var aw: std.Io.Writer.Allocating = .init(alloc);
        let mut aw = AllocatingWriter::new();
        ReadSuccess {
            contents: &[Content {
                mime: b"text/plain",
                data: &data,
            }],
            ..ReadSuccess::default()
        }
        .encode(&mut aw)
        .unwrap();

        // OK + 2 DATA packets + DONE = 4 packets.
        // const count = std.mem.count(u8, aw.written(), "\x1b]5522;");
        let needle: &[u8] = b"\x1b]5522;";
        let count = aw
            .written()
            .windows(needle.len())
            .filter(|w| *w == needle)
            .count();
        assert_eq!(4usize, count);

        // The first chunk is exactly read_chunk_size bytes, base64 encoded
        // with padding.
        let first = b64_encode_to_vec(&data[0..read_chunk_size]);
        assert!(aw
            .written()
            .windows(first.len())
            .any(|w| w == first.as_slice()));
    }

    // test "read success: no data packets for empty clipboard"
    #[test]
    fn read_success_no_data_packets_for_empty_clipboard() {
        let mut buf = [0u8; 512];
        let mut writer = FixedWriter::new(&mut buf);
        ReadSuccess {
            contents: &[Content {
                mime: b"text/plain",
                data: b"",
            }],
            ..ReadSuccess::default()
        }
        .encode(&mut writer)
        .unwrap();
        assert_eq!(
            b"\x1b]5522;type=read:status=OK\x1b\\\
              \x1b]5522;type=read:status=DONE\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "read success: paste event carries pw in every packet"
    #[test]
    fn read_success_paste_event_carries_pw_in_every_packet() {
        let mut buf = [0u8; 512];
        let mut writer = FixedWriter::new(&mut buf);
        ReadSuccess {
            list: true,
            pw: Some(b"otp"),
            available: &[b"text/plain"],
            ..ReadSuccess::default()
        }
        .encode(&mut writer)
        .unwrap();
        assert_eq!(
            b"\x1b]5522;type=read:status=OK:pw=b3Rw\x1b\\\
              \x1b]5522;type=read:status=DATA:mime=Lg==:pw=b3Rw;dGV4dC9wbGFpbgo=\x1b\\\
              \x1b]5522;type=read:status=DONE:pw=b3Rw\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "paste event: pw in every packet, listing of every type"
    #[test]
    fn paste_event_pw_in_every_packet_listing_of_every_type() {
        let mut buf = [0u8; 512];
        let mut writer = FixedWriter::new(&mut buf);
        PasteEvent::new(b"otp", &[b"text/plain", b"image/png"])
            .encode(&mut writer)
            .unwrap();
        // Payload is base64 of "text/plain image/png\n".
        assert_eq!(
            b"\x1b]5522;type=read:status=OK:pw=b3Rw\x1b\\\
              \x1b]5522;type=read:status=DATA:mime=Lg==:pw=b3Rw;dGV4dC9wbGFpbiBpbWFnZS9wbmcK\x1b\\\
              \x1b]5522;type=read:status=DONE:pw=b3Rw\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "paste event: primary is reported only on the OK packet"
    #[test]
    fn paste_event_primary_is_reported_only_on_the_ok_packet() {
        let mut buf = [0u8; 512];
        let mut writer = FixedWriter::new(&mut buf);
        PasteEvent {
            primary: true,
            terminator: Terminator::Bel,
            ..PasteEvent::new(b"otp", &[b"text/plain"])
        }
        .encode(&mut writer)
        .unwrap();
        assert_eq!(
            b"\x1b]5522;type=read:status=OK:loc=primary:pw=b3Rw\x07\
              \x1b]5522;type=read:status=DATA:mime=Lg==:pw=b3Rw;dGV4dC9wbGFpbgo=\x07\
              \x1b]5522;type=read:status=DONE:pw=b3Rw\x07"
                .as_slice(),
            writer.buffered(),
        );
    }

    // test "paste event: empty listing packet is still sent"
    #[test]
    fn paste_event_empty_listing_packet_is_still_sent() {
        let mut buf = [0u8; 512];
        let mut writer = FixedWriter::new(&mut buf);
        PasteEvent::new(b"otp", &[]).encode(&mut writer).unwrap();
        assert_eq!(
            b"\x1b]5522;type=read:status=OK:pw=b3Rw\x1b\\\
              \x1b]5522;type=read:status=DATA:mime=Lg==:pw=b3Rw\x1b\\\
              \x1b]5522;type=read:status=DONE:pw=b3Rw\x1b\\"
                .as_slice(),
            writer.buffered(),
        );
    }
}
