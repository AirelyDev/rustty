//! PORTS: ghostty src/terminal/apc/glyph/response.zig
//!
//! PORT STATUS: done

// const std = @import("std");

/// Query response coverage state for a codepoint.
// PORT NOTE: reference `pub const Coverage = packed struct(u2)` with two bool
// fields defaulting to false. Zig packs `system` into bit 0 and `glossary` into
// bit 1. Rust has no bitfield structs, so the layout is modeled with explicit
// bool fields plus `from_bits`/`to_bits` helpers reproducing `@bitCast`
// to/from the backing u2. `Default` provides the `= false` field defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Coverage {
    /// A system font covers the codepoint.
    pub system: bool,

    /// A session glyph registration covers the codepoint.
    pub glossary: bool,
}

impl Coverage {
    /// No system font or registered glyph covers the codepoint.
    // PORT NOTE: `pub const free: Coverage = .{};` — the all-default value.
    pub const FREE: Coverage = Coverage {
        system: false,
        glossary: false,
    };

    /// `@bitCast` to the backing u2: system in bit 0, glossary in bit 1.
    pub fn to_bits(self) -> u8 {
        (self.system as u8) | ((self.glossary as u8) << 1)
    }

    /// `@bitCast` from the backing u2.
    pub fn from_bits(int: u8) -> Coverage {
        Coverage {
            system: int & 1 == 1,
            glossary: (int >> 1) & 1 == 1,
        }
    }

    /// Parse the query response coverage list from its comma-separated form.
    /// Unknown coverage names are ignored for forward compatibility.
    pub fn init(value: &[u8]) -> Option<Coverage> {
        // var result: Coverage = .free;
        let mut result: Coverage = Coverage::FREE;
        // var it = std.mem.splitScalar(u8, value, ',');
        // while (it.next()) |name| {
        // PORT NOTE: `std.mem.splitScalar(u8, value, ',')` splits on every ','
        // and yields one (possibly empty) field per separator-delimited run,
        // including a leading empty field for the empty input. Rust's
        // `split(b',')` on a byte slice yields the same field sequence.
        for name in value.split(|&b| b == b',') {
            // if (std.mem.eql(u8, name, "system")) {
            if name == b"system" {
                result.system = true;
            // } else if (std.mem.eql(u8, name, "glossary")) {
            } else if name == b"glossary" {
                result.glossary = true;
            }
        }

        Some(result)
    }
}

/// Response to a glyph APC request, formatted for the wire protocol.
// PORT NOTE: reference `pub const Response = union(enum)`. Each tag carries a
// payload struct, modeled as a Rust enum with struct-payload variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    /// Support query response listing supported payload formats.
    Support(ResponseSupport),

    /// Codepoint coverage query response.
    Query(ResponseQuery),

    /// Glyph registration response (success or error).
    Register(ResponseRegister),

    /// Registration clear response.
    Clear(ResponseClear),
}

impl Response {
    /// Recommended fixed buffer size for formatting a Glyph Protocol response.
    ///
    /// Glyph Protocol responses contain only framing plus bounded scalar fields:
    /// a u21 codepoint as hex, a decimal u8 status, a small fixed set of
    /// supported format names, coverage names, and the reason names produced by
    /// the executor. 1024 bytes is therefore far above the longest response we
    /// can emit today, while still being small enough for stack allocation in
    /// stream handlers. If callers construct responses with arbitrary `.other`
    /// or clear reason strings, they must ensure those strings fit or handle the
    /// writer error from `formatWire`.
    pub const MAX_WIRE_BYTES: usize = 1024;

    /// Write the response in the glyph APC wire format to `writer`.
    ///
    /// The framing is: `ESC _ 25a1 ; <verb> ; <key=value>* ESC \`
    // PORT NOTE: reference `formatWire(self, writer: *std.Io.Writer)
    // std.Io.Writer.Error!void`. The faithful Rust equivalent writes through a
    // generic `&mut impl std::fmt::Write` and propagates the writer error via
    // `std::fmt::Result`. `writeAll`/`writeByte` -> `write_str`/`write_char`;
    // `writer.print("...{x}...{d}...", .{ a, b })` -> `write!` with `{:x}`
    // (lowercase hex) and `{}` (decimal Display) format specs.
    pub fn format_wire(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        writer.write_str("\x1b_25a1;")?;
        match self {
            Response::Support(r) => {
                // From the spec:
                // Order is not significant; clients MUST treat the value as a
                // set. An empty fmt= value means the terminal recognises
                // Glyph Protocol but currently advertises no payload formats
                // — every r will be rejected. Clients MUST ignore names they
                // do not recognise rather than failing the reply, so future
                // format names are forward- compatible.
                writer.write_str("s;fmt=")?;
                let mut first = true;
                if r.fmt.glyf {
                    first = false;
                    writer.write_str("glyf")?;
                }
                if r.fmt.colrv0 {
                    if !first {
                        writer.write_char(',')?;
                    }
                    first = false;
                    writer.write_str("colrv0")?;
                }
                if r.fmt.colrv1 {
                    if !first {
                        writer.write_char(',')?;
                    }
                    first = false;
                    // PORT NOTE: the reference sets `first = false;` here even
                    // though it is the final branch and `first` is never read
                    // again; the faithful assignment is kept and the trailing
                    // `let _ = first;` silences Rust's unused-assignment lint.
                    let _ = first;
                    writer.write_str("colrv1")?;
                }
            }
            Response::Query(r) => {
                // status is a comma-separated list of coverage names — the
                // set of sources that can render cp in this session. Order is
                // not significant; clients MUST treat the value as a set.
                write!(writer, "q;cp={:x};status=", r.cp)?;
                let mut first = true;
                if r.status.system {
                    first = false;
                    writer.write_str("system")?;
                }
                if r.status.glossary {
                    if !first {
                        writer.write_char(',')?;
                    }
                    writer.write_str("glossary")?;
                }
            }
            Response::Register(r) => {
                write!(
                    writer,
                    "r;cp={:x};status={}",
                    r.cp,
                    r.status.int_from_enum()
                )?;
                if let Some(reason) = &r.reason {
                    writer.write_str(";reason=")?;
                    // PORT NOTE: `reason.name()` returns `[]const u8`; here the
                    // byte-slice is written through as bytes-as-text via the
                    // wire writer (all reason names are ASCII).
                    writer.write_str(reason.name())?;
                }
            }
            Response::Clear(r) => {
                write!(writer, "c;status={}", r.status.int_from_enum())?;
                if let Some(reason) = &r.reason {
                    writer.write_str(";reason=")?;
                    // PORT NOTE: reference `writer.writeAll(reason)` writes the
                    // `?[]const u8` reason bytes through; the stored bytes are
                    // ASCII reason names, surfaced as `&str` for the wire writer.
                    writer.write_str(
                        std::str::from_utf8(reason).expect("clear reason is valid UTF-8"),
                    )?;
                }
            }
        }
        writer.write_str("\x1b\\")?;
        Ok(())
    }
}

/// Support query response fields.
// PORT NOTE: reference `pub const Support = struct { fmt: Formats }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResponseSupport {
    /// Supported payload formats.
    pub fmt: ResponseSupportFormats,
}

/// Supported payload formats.
// PORT NOTE: reference `pub const Formats = packed struct(u8)` with three bool
// fields (bits 0,1,2) defaulting to false and a `_padding: u5 = 0` filling the
// high 5 bits. Modeled with explicit bools + `from_bits`/`to_bits` reproducing
// `@bitCast` to/from the backing u8 (padding bits are always zero). `Default`
// gives the `= false` field defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ResponseSupportFormats {
    /// TrueType simple glyph outlines (required in v1).
    pub glyf: bool,

    /// COLR v0 layered flat-colour glyphs.
    pub colrv0: bool,

    /// COLR v1 paint-graph glyphs.
    pub colrv1: bool,
    // _padding: u5 = 0,
}

impl ResponseSupportFormats {
    /// `@bitCast` to the backing u8: glyf bit 0, colrv0 bit 1, colrv1 bit 2,
    /// the high 5 padding bits always zero.
    pub fn to_bits(self) -> u8 {
        (self.glyf as u8) | ((self.colrv0 as u8) << 1) | ((self.colrv1 as u8) << 2)
    }

    /// `@bitCast` from the backing u8.
    pub fn from_bits(int: u8) -> ResponseSupportFormats {
        ResponseSupportFormats {
            glyf: int & 1 == 1,
            colrv0: (int >> 1) & 1 == 1,
            colrv1: (int >> 2) & 1 == 1,
        }
    }
}

/// Codepoint query response fields.
// PORT NOTE: reference `pub const Query = struct { cp: u21, status: Coverage }`.
// `u21` is modeled as `u32` (no native u21 in Rust); the queried codepoint is
// always a valid scalar value in range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResponseQuery {
    /// The queried codepoint.
    pub cp: u32,

    /// Coverage status for the codepoint.
    pub status: Coverage,
}

/// Register response fields.
// PORT NOTE: reference `pub const Register = struct` — `status` defaults to
// `.ok` and `reason` defaults to `null`. `cp: u21` -> `u32`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseRegister {
    /// The target codepoint of the registration.
    pub cp: u32,

    /// Result status of the registration encoded as a decimal u8.
    pub status: Status,

    /// Optional symbolic error reason.
    pub reason: Option<ResponseRegisterReason>,
}

impl ResponseRegister {
    /// Construct a register response with the reference field defaults
    /// (`status = .ok`, `reason = null`).
    pub fn new(cp: u32) -> ResponseRegister {
        ResponseRegister {
            cp,
            status: Status::OK,
            reason: None,
        }
    }
}

/// Register error reason codes defined by Glyph Protocol §6.2.
// PORT NOTE: reference `pub const Reason = union(enum)` — the `other` tag
// carries a `[]const u8`, ported as an owned `Vec<u8>` per the byte-slice
// fidelity rule (never a lossy String).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResponseRegisterReason {
    /// `cp` is not in any PUA range.
    OutOfNamespace,

    /// Payload contains composite glyphs.
    CompositeUnsupported,

    /// Payload contains hinting instructions.
    HintingUnsupported,

    /// Payload failed to parse as the declared `fmt`.
    MalformedPayload,

    /// Payload exceeds 64 KiB after base64 decoding.
    PayloadTooLarge,

    /// A reason code not known by this version of Ghostty.
    Other(Vec<u8>),
}

impl ResponseRegisterReason {
    /// Return the wire-format reason name.
    // PORT NOTE: reference `name(self) []const u8`. The fixed variants return
    // static names; `.other` returns its stored byte-slice value verbatim.
    pub fn name(&self) -> &str {
        match self {
            ResponseRegisterReason::OutOfNamespace => "out_of_namespace",
            ResponseRegisterReason::CompositeUnsupported => "composite_unsupported",
            ResponseRegisterReason::HintingUnsupported => "hinting_unsupported",
            ResponseRegisterReason::MalformedPayload => "malformed_payload",
            ResponseRegisterReason::PayloadTooLarge => "payload_too_large",
            // .other => |value| value,
            // PORT NOTE: the stored bytes are ASCII reason names; surfaced as
            // a `&str` to feed the wire writer. (Callers construct `.other`
            // from known ASCII reason strings.)
            ResponseRegisterReason::Other(value) => {
                std::str::from_utf8(value).expect("reason name is valid UTF-8")
            }
        }
    }
}

/// Clear response fields.
// PORT NOTE: reference `pub const Clear = struct` — `status` defaults to `.ok`,
// `reason` defaults to `null`. `reason: ?[]const u8` -> `Option<Vec<u8>>` per
// the byte-slice fidelity rule.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ResponseClear {
    /// Result status of the clear operation encoded as a decimal u8.
    pub status: Status,

    /// Optional symbolic error reason.
    pub reason: Option<Vec<u8>>,
}

/// Status code for register and clear responses.
// PORT NOTE: reference `pub const Status = enum(u8) { ok = 0, err = 1, _ }`.
// The trailing `_` makes it a non-exhaustive enum that may hold ANY u8 value
// (e.g. `@enumFromInt(37)`), so it is modeled as a transparent newtype over
// `u8` with `OK`/`ERR` constants. `@intFromEnum` -> `int_from_enum`. Defaults
// to `.ok` (= 0) to honour the reference struct field default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status(pub u8);

impl Status {
    /// The operation completed successfully.
    pub const OK: Status = Status(0);

    /// A generic or unspecified error occurred.
    pub const ERR: Status = Status(1);

    /// `@enumFromInt(value)` — construct a Status from an arbitrary u8.
    pub fn from_int(value: u8) -> Status {
        Status(value)
    }

    /// `@intFromEnum(self)` — the backing u8 value.
    pub fn int_from_enum(self) -> u8 {
        self.0
    }
}

impl Default for Status {
    fn default() -> Status {
        Status::OK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: each test uses a `String` (impl std::fmt::Write) in place of
    // the reference's `std.Io.Writer.fixed(&buf)` + `writer.buffered()`; the
    // accumulated string is the faithful equivalent of the buffered bytes.

    #[test]
    fn support_formats_default_to_no_advertised_formats() {
        // const Formats = Response.Support.Formats;
        // try testing.expectEqual(@as(u8, 0), @as(u8, @bitCast(Formats{})));
        assert_eq!(0u8, ResponseSupportFormats::default().to_bits());
    }

    #[test]
    fn response_support_format_wire() {
        let mut writer = String::new();

        let resp = Response::Support(ResponseSupport {
            fmt: ResponseSupportFormats {
                glyf: true,
                colrv0: true,
                colrv1: false,
            },
        });
        resp.format_wire(&mut writer).unwrap();
        assert_eq!("\x1b_25a1;s;fmt=glyf,colrv0\x1b\\", writer);
    }

    #[test]
    fn response_support_format_wire_with_no_formats() {
        let mut writer = String::new();

        let resp = Response::Support(ResponseSupport {
            fmt: ResponseSupportFormats::default(),
        });
        resp.format_wire(&mut writer).unwrap();
        assert_eq!("\x1b_25a1;s;fmt=\x1b\\", writer);
    }

    #[test]
    fn response_query_format_wire() {
        let mut writer = String::new();

        let resp = Response::Query(ResponseQuery {
            cp: 0xE0A0,
            status: Coverage {
                system: true,
                glossary: true,
            },
        });
        resp.format_wire(&mut writer).unwrap();
        assert_eq!("\x1b_25a1;q;cp=e0a0;status=system,glossary\x1b\\", writer);
    }

    #[test]
    fn response_query_format_wire_with_no_coverage() {
        let mut writer = String::new();

        let resp = Response::Query(ResponseQuery {
            cp: 0xE0A0,
            status: Coverage::default(),
        });
        resp.format_wire(&mut writer).unwrap();
        assert_eq!("\x1b_25a1;q;cp=e0a0;status=\x1b\\", writer);
    }

    #[test]
    fn coverage_parses_comma_separated_names() {
        assert_eq!(Coverage::default(), Coverage::init(b"").unwrap());
        assert_eq!(
            Coverage {
                system: true,
                glossary: false
            },
            Coverage::init(b"system").unwrap()
        );
        assert_eq!(
            Coverage {
                system: false,
                glossary: true
            },
            Coverage::init(b"glossary").unwrap()
        );
        assert_eq!(
            Coverage {
                system: true,
                glossary: true
            },
            Coverage::init(b"system,glossary").unwrap()
        );
        assert_eq!(
            Coverage {
                system: true,
                glossary: true
            },
            Coverage::init(b"glossary,system").unwrap()
        );
        assert_eq!(
            Coverage {
                system: true,
                glossary: false
            },
            Coverage::init(b"system,future").unwrap()
        );
        assert_eq!(Coverage::default(), Coverage::init(b"future").unwrap());
    }

    #[test]
    fn response_register_success_format_wire() {
        let mut writer = String::new();

        let resp = Response::Register(ResponseRegister::new(0xE0A0));
        resp.format_wire(&mut writer).unwrap();
        assert_eq!("\x1b_25a1;r;cp=e0a0;status=0\x1b\\", writer);
    }

    #[test]
    fn response_register_error_format_wire() {
        let mut writer = String::new();

        let resp = Response::Register(ResponseRegister {
            cp: 0xE0A0,
            status: Status::ERR,
            reason: Some(ResponseRegisterReason::OutOfNamespace),
        });
        resp.format_wire(&mut writer).unwrap();
        assert_eq!(
            "\x1b_25a1;r;cp=e0a0;status=1;reason=out_of_namespace\x1b\\",
            writer
        );
    }

    #[test]
    fn response_register_arbitrary_status_format_wire() {
        let mut writer = String::new();

        let resp = Response::Register(ResponseRegister {
            cp: 0xE0A0,
            status: Status::from_int(37),
            reason: Some(ResponseRegisterReason::PayloadTooLarge),
        });
        resp.format_wire(&mut writer).unwrap();
        assert_eq!(
            "\x1b_25a1;r;cp=e0a0;status=37;reason=payload_too_large\x1b\\",
            writer
        );
    }

    #[test]
    fn register_reason_names() {
        // const Reason = Response.Register.Reason;
        assert_eq!(
            "out_of_namespace",
            ResponseRegisterReason::OutOfNamespace.name()
        );
        assert_eq!(
            "composite_unsupported",
            ResponseRegisterReason::CompositeUnsupported.name()
        );
        assert_eq!(
            "hinting_unsupported",
            ResponseRegisterReason::HintingUnsupported.name()
        );
        assert_eq!(
            "malformed_payload",
            ResponseRegisterReason::MalformedPayload.name()
        );
        assert_eq!(
            "payload_too_large",
            ResponseRegisterReason::PayloadTooLarge.name()
        );
        assert_eq!(
            "future_reason",
            ResponseRegisterReason::Other(b"future_reason".to_vec()).name()
        );
    }

    #[test]
    fn response_clear_format_wire() {
        let mut writer = String::new();

        let resp = Response::Clear(ResponseClear::default());
        resp.format_wire(&mut writer).unwrap();
        assert_eq!("\x1b_25a1;c;status=0\x1b\\", writer);
    }

    #[test]
    fn response_clear_error_format_wire() {
        let mut writer = String::new();

        let resp = Response::Clear(ResponseClear {
            status: Status::ERR,
            reason: Some(b"out_of_namespace".to_vec()),
        });
        resp.format_wire(&mut writer).unwrap();
        assert_eq!("\x1b_25a1;c;status=1;reason=out_of_namespace\x1b\\", writer);
    }
}
