//! rustty-vt apc/glyph/request
//!
//! PORTS: ghostty src/terminal/apc/glyph/request.zig
//!
//! PORT STATUS: done

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const assert = std.debug.assert;
// const Glyf = @import("../../../font/opentype/glyf.zig").Glyf;
// PORT NOTE: `Allocator` is not threaded through the port. Zig's `CommandParser`
// buffers bytes via an allocator-backed `std.ArrayList(u8)` and the parsed
// `Request` variants own their `raw` bytes via `alloc`; here the buffer is an
// owned `Vec<u8>` and each request variant owns its `raw: Vec<u8>`, freed on
// drop. The reference `deinit`/`alloc.free(raw)` calls therefore have no
// port-side equivalent (handled by `Drop`). `Glyf` is `rustty_font::opentype::
// glyf::Glyf` (D1 Option C: rustty-font is a direct, mandatory dependency). The
// reference's `Glyf.Entry` / `Glyf.Outline` (nested types in Zig) are the
// sibling types `Entry` / `Outline` in the port.
use rustty_font::opentype::glyf::{DecodeError as GlyfDecodeError, Entry as GlyfEntry, Outline};

/// Maximum decoded glyph payload size accepted by the protocol.
/// This is documented in the spec.
const MAX_PAYLOAD_SIZE: usize = 64 * 1024; // 64 KiB

/// Error set for [`CommandParser`] operations.
// PORT NOTE: reference `pub const Error = Allocator.Error || error{InvalidFormat}`.
// `Allocator.Error` is `error{OutOfMemory}`; the `max_bytes` cap raises
// `OutOfMemory` exactly as the reference does (it is not a real allocation
// failure, just the protocol byte cap), so both members are preserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandParserError {
    /// The buffered payload reached the `max_bytes` cap.
    OutOfMemory,

    /// The completed payload did not parse as a valid request.
    InvalidFormat,
}

/// Stateful parser for a single glyph APC payload after the `25a1;` prefix.
// PORT NOTE: reference `pub const CommandParser = struct { alloc, data: ArrayList,
// max_bytes }`. The `alloc` field and the `data: std.ArrayList(u8) = .empty`
// allocator-backed buffer collapse to an owned `Vec<u8>` here.
pub struct CommandParser {
    data: Vec<u8>,

    /// Maximum bytes the data payload can buffer. This is to prevent
    /// malicious input from causing us to allocate too much memory.
    max_bytes: usize,
}

impl CommandParser {
    /// Create a glyph APC parser that buffers the raw command bytes.
    // PORT NOTE: reference `init(alloc, max_bytes)`; the `alloc` parameter is
    // dropped (no allocator threading) and `data` starts empty.
    pub fn init(max_bytes: usize) -> CommandParser {
        CommandParser {
            data: Vec::new(),
            max_bytes,
        }
    }

    /// Release any buffered command bytes owned by the parser.
    // PORT NOTE: reference `deinit` frees `self.data`; here the `Vec<u8>` is
    // freed on drop, so this is a no-op retained for call-site fidelity.
    pub fn deinit(&mut self) {
        // self.data.deinit(self.alloc);
    }

    /// Append one more byte of APC payload to the buffered command.
    // PORT NOTE: reference returns `Allocator.Error!void`; the only error is the
    // `max_bytes` cap surfaced as `OutOfMemory`.
    pub fn feed(&mut self, byte: u8) -> Result<(), CommandParserError> {
        // if (self.data.items.len >= self.max_bytes) return error.OutOfMemory;
        if self.data.len() >= self.max_bytes {
            return Err(CommandParserError::OutOfMemory);
        }
        // try self.data.append(self.alloc, byte);
        self.data.push(byte);
        Ok(())
    }

    /// Append a slice of APC payload bytes to the buffered command.
    /// Equivalent to calling feed for each byte, but appends in bulk.
    // pub fn feedSlice(self: *CommandParser, bytes: []const u8) Allocator.Error!void
    pub fn feed_slice(&mut self, bytes: &[u8]) -> Result<(), CommandParserError> {
        // if (self.data.items.len + bytes.len > self.max_bytes) return error.OutOfMemory;
        if self.data.len() + bytes.len() > self.max_bytes {
            return Err(CommandParserError::OutOfMemory);
        }
        // try self.data.appendSlice(self.alloc, bytes);
        self.data.extend_from_slice(bytes);
        Ok(())
    }

    /// Finish parsing and return an owned request that can outlive the parser.
    pub fn complete(&mut self) -> Result<Request, CommandParserError> {
        // Normalize bare single-byte verbs like `s` into `s;` so the parsed
        // command always has the standard `verb;...` layout.
        // if (self.data.items.len == 1) try self.data.append(self.alloc, ';');
        if self.data.len() == 1 {
            self.data.push(b';');
        }

        // const raw = try self.data.toOwnedSlice(alloc);
        //
        // Ownership of the buffered bytes has moved to `raw`, so clear the
        // array list before we build the final command value.
        // self.data = .empty;
        // errdefer alloc.free(raw);
        // PORT NOTE: `toOwnedSlice` transfers the buffer out and leaves the list
        // empty; `std::mem::take` reproduces this exactly. `Request::parse` takes
        // ownership of `raw`, so the reference `errdefer alloc.free(raw)` (which
        // only runs on the `'s'` free or an error before ownership transfer) is
        // subsumed by Rust ownership/drop.
        let raw = std::mem::take(&mut self.data);
        Request::parse(raw).map_err(|_| CommandParserError::InvalidFormat)
    }
}

/// Parsed glyph APC request with the verb classified eagerly.
// PORT NOTE: reference `pub const Request = union(enum)`. Modeled as a Rust enum;
// the `support` tag carries no payload. The nested option/value types (`Format`,
// `Reply`, `Width`, `Size`, `Align`, `Pad`) and the `optionValue` helper are
// hoisted to module scope (Rust has no nested type decls).
#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    /// Support query (bare `s` verb, no options).
    Support,

    /// Codepoint coverage query.
    Query(Query),

    /// Glyph registration request.
    Register(Register),

    /// Registration clear request.
    Clear(Clear),
}

/// Error returned by [`Request::parse`].
// PORT NOTE: reference `error{InvalidFormat}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidFormat;

impl Request {
    /// Parse an owned glyph APC payload into its eagerly-classified request
    /// form.
    ///
    /// The raw format here is strict on its requirements to avoid
    /// edge cases: it must contain the request AND the request must
    /// end in a semicolon (even if there are no options). The spec itself
    /// does not require this but we artificially insert it in our parser
    /// to simplify parsing later.
    pub fn parse(raw: Vec<u8>) -> Result<Request, InvalidFormat> {
        // if (raw.len < 2) return error.InvalidFormat;
        if raw.len() < 2 {
            return Err(InvalidFormat);
        }
        // if (raw[1] != ';') return error.InvalidFormat;
        if raw[1] != b';' {
            return Err(InvalidFormat);
        }

        match raw[0] {
            // 's' => { alloc.free(raw); return .support; }
            // PORT NOTE: the reference frees `raw` here because `.support`
            // carries no bytes; `raw` is dropped at end of scope.
            b's' => {
                drop(raw);
                Ok(Request::Support)
            }
            // 'q' => .{ .query = .init(raw) },
            b'q' => Ok(Request::Query(Query::init(raw))),
            // 'r' => .{ .register = Register.init(raw) orelse return error.InvalidFormat },
            b'r' => match Register::init(raw) {
                Some(reg) => Ok(Request::Register(reg)),
                None => Err(InvalidFormat),
            },
            // 'c' => .{ .clear = .init(raw) },
            b'c' => Ok(Request::Clear(Clear::init(raw))),
            // else => error.InvalidFormat,
            _ => Err(InvalidFormat),
        }
    }

    /// Free the raw bytes retained by any request variant.
    // PORT NOTE: reference `deinit` frees the `raw` slice of each non-support
    // variant; here those bytes are owned `Vec<u8>` freed on drop, so this is a
    // no-op retained for call-site fidelity.
    pub fn deinit(&mut self) {
        // switch (self.*) { .support => {}, inline else => |*cmd|
        //     if (cmd.raw.len > 0) alloc.free(cmd.raw) }
    }
}

/// Query verb payload with lazily-decoded options.
#[derive(Debug, PartialEq, Eq)]
pub struct Query {
    raw: Vec<u8>,
}

impl Query {
    /// Initialize a query command from owned raw command bytes.
    pub fn init(raw: Vec<u8>) -> Query {
        Query { raw }
    }

    /// Lazily decode a query option on demand.
    // PORT NOTE: reference `get(self, comptime option: Option) ?option.Type()`.
    // The only query option is `cp` (a `u21`), so `get` is specialized to the
    // single `cp` reader rather than a comptime-dispatched generic.
    pub fn get_cp(&self) -> Option<u32> {
        QueryOption::Cp.read(self.raw_options())
    }

    /// Return the raw option portion of a valid query command.
    fn raw_options(&self) -> &[u8] {
        // assert(self.raw.len >= 2);
        assert!(self.raw.len() >= 2);
        // assert(self.raw[0] == 'q');
        assert!(self.raw[0] == b'q');
        // assert(self.raw[1] == ';');
        assert!(self.raw[1] == b';');
        &self.raw[2..]
    }
}

/// Options recognized for the glyph query request.
// PORT NOTE: reference nested `Query.Option = enum { cp }` with comptime `Type`,
// `key`, and `read`. Hoisted to a module enum; the comptime `Type` mapping
// (`cp => u21`) is reflected directly in `read`'s `Option<u32>` return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryOption {
    /// Target Unicode codepoint encoded in hexadecimal.
    Cp,
}

impl QueryOption {
    /// Return the wire-format option key for this query option.
    fn key(self) -> &'static [u8] {
        match self {
            // .cp => @tagName(self),
            QueryOption::Cp => b"cp",
        }
    }

    /// Read and decode a query option from the raw option string.
    fn read(self, raw: &[u8]) -> Option<u32> {
        // const value = optionValue(raw, self.key()) orelse return null;
        let value = option_value(raw, self.key())?;
        match self {
            // .cp => std.fmt.parseInt(u21, value, 16) catch null,
            QueryOption::Cp => parse_u21(value, 16),
        }
    }
}

/// Register verb payload with lazily-decoded options and optional base64 data.
#[derive(Debug, PartialEq, Eq)]
pub struct Register {
    raw: Vec<u8>,
    payload_idx: usize,
}

impl Register {
    /// Initialize a register command from owned raw command bytes.
    pub fn init(raw: Vec<u8>) -> Option<Register> {
        // if (raw.len < 2) return null;
        if raw.len() < 2 {
            return None;
        }
        // if (raw[0] != 'r') return null;
        if raw[0] != b'r' {
            return None;
        }
        // if (raw[1] != ';') return null;
        if raw[1] != b';' {
            return None;
        }
        // const payload_idx = std.mem.lastIndexOfScalar(u8, raw, ';') orelse return null;
        let payload_idx = raw.iter().rposition(|&b| b == b';')?;
        // if (payload_idx <= 1) return null;
        if payload_idx <= 1 {
            return None;
        }

        Some(Register { raw, payload_idx })
    }

    /// Lazily decode a register option on demand, applying protocol
    /// defaults when the option is omitted.
    // PORT NOTE: reference `get(self, comptime option: Option) ?option.Type()`
    // returns a different payload type per option, so the comptime-generic `get`
    // is expanded into one typed accessor per option here. Each reproduces the
    // reference body: if the option is absent in `rawOptions`, fall back to
    // `self.get(.upm)` for `aw`/`lh` or to `Option.default()` otherwise;
    // otherwise decode via `Option.read`.
    fn raw_options(&self) -> &[u8] {
        // assert(self.raw.len >= 2);
        assert!(self.raw.len() >= 2);
        // assert(self.raw[0] == 'r');
        assert!(self.raw[0] == b'r');
        // assert(self.raw[1] == ';');
        assert!(self.raw[1] == b';');
        // assert(self.payload_idx >= 2);
        assert!(self.payload_idx >= 2);
        // assert(self.payload_idx <= self.raw.len);
        assert!(self.payload_idx <= self.raw.len());
        &self.raw[2..self.payload_idx]
    }

    /// `get(.cp)`: target codepoint, no default.
    // PORT NOTE: the `if option absent { return <default>; } read(raw)` shape is
    // the faithful expansion of the reference `get`'s `if (optionValue == null)
    // return default;` branch. For `cp` the default is `null` (`None`); `#[allow(
    // clippy::question_mark)]` keeps the explicit default branch rather than
    // collapsing it into `?`, which would hide the reference's default handling.
    #[allow(clippy::question_mark)]
    pub fn get_cp(&self) -> Option<u32> {
        let raw = self.raw_options();
        if option_value(raw, RegisterOption::Cp.key()).is_none() {
            // .cp default is null
            return None;
        }
        RegisterOption::Cp.read_cp(raw)
    }

    /// `get(.fmt)`: payload format, default `.glyf`.
    pub fn get_fmt(&self) -> Option<Format> {
        let raw = self.raw_options();
        if option_value(raw, RegisterOption::Fmt.key()).is_none() {
            return Some(Format::Glyf);
        }
        RegisterOption::Fmt.read_fmt(raw)
    }

    /// `get(.reply)`: reply verbosity, default `.all`.
    pub fn get_reply(&self) -> Option<Reply> {
        let raw = self.raw_options();
        if option_value(raw, RegisterOption::Reply.key()).is_none() {
            return Some(Reply::All);
        }
        RegisterOption::Reply.read_reply(raw)
    }

    /// `get(.upm)`: units-per-em, default 1000.
    pub fn get_upm(&self) -> Option<u32> {
        let raw = self.raw_options();
        if option_value(raw, RegisterOption::Upm.key()).is_none() {
            return Some(1000);
        }
        RegisterOption::Upm.read_u32(raw, RegisterOption::Upm)
    }

    /// `get(.aw)`: advance width, default = `get(.upm)`.
    pub fn get_aw(&self) -> Option<u32> {
        let raw = self.raw_options();
        if option_value(raw, RegisterOption::Aw.key()).is_none() {
            // .aw, .lh => self.get(.upm)
            return self.get_upm();
        }
        RegisterOption::Aw.read_u32(raw, RegisterOption::Aw)
    }

    /// `get(.lh)`: line height, default = `get(.upm)`.
    pub fn get_lh(&self) -> Option<u32> {
        let raw = self.raw_options();
        if option_value(raw, RegisterOption::Lh.key()).is_none() {
            // .aw, .lh => self.get(.upm)
            return self.get_upm();
        }
        RegisterOption::Lh.read_u32(raw, RegisterOption::Lh)
    }

    /// `get(.width)`: cell width, default `.narrow`.
    pub fn get_width(&self) -> Option<Width> {
        let raw = self.raw_options();
        if option_value(raw, RegisterOption::Width.key()).is_none() {
            return Some(Width::Narrow);
        }
        RegisterOption::Width.read_width(raw)
    }

    /// `get(.size)`: scale policy, default `.height`.
    pub fn get_size(&self) -> Option<Size> {
        let raw = self.raw_options();
        if option_value(raw, RegisterOption::Size.key()).is_none() {
            return Some(Size::Height);
        }
        RegisterOption::Size.read_size(raw)
    }

    /// `get(.align)`: placement, default `Align{}`.
    pub fn get_align(&self) -> Option<Align> {
        let raw = self.raw_options();
        if option_value(raw, RegisterOption::Align.key()).is_none() {
            return Some(Align::default());
        }
        RegisterOption::Align.read_align(raw)
    }

    /// `get(.pad)`: padding, default `Pad{}`.
    pub fn get_pad(&self) -> Option<Pad> {
        let raw = self.raw_options();
        if option_value(raw, RegisterOption::Pad.key()).is_none() {
            return Some(Pad::default());
        }
        RegisterOption::Pad.read_pad(raw)
    }

    /// Return the base64 payload carried by a register request.
    ///
    /// If no payload is present, this returns an empty slice. The returned
    /// bytes may still be invalid base64; this function only exposes the raw
    /// payload segment and does not validate or decode it.
    pub fn payload(&self) -> &[u8] {
        // assert(self.raw.len >= 2);
        assert!(self.raw.len() >= 2);
        // assert(self.raw[0] == 'r');
        assert!(self.raw[0] == b'r');
        // assert(self.raw[1] == ';');
        assert!(self.raw[1] == b';');
        // return if (self.payload_idx == self.raw.len) "" else self.raw[self.payload_idx + 1 ..];
        if self.payload_idx == self.raw.len() {
            b""
        } else {
            &self.raw[self.payload_idx + 1..]
        }
    }

    /// Decode this request's base64 glyf payload into an owned outline.
    // PORT NOTE: reference `decodeGlyfPayload(self, alloc, max_allocation_bytes)
    // DecodeError!Glyf.Outline`.
    // The reference uses a `stackFallback(max_payload_size, alloc)` allocator for
    // the decode scratch buffer (stack-first, heap-fallback); since `size` is
    // capped at `MAX_PAYLOAD_SIZE`, an owned `Vec<u8>` scratch is the faithful
    // equivalent (the choice of stack vs heap is an allocation-strategy detail,
    // not observable behavior). The std-base64 decoder is `std.base64.standard.
    // Decoder` (alphabet + `'='` pad). `Glyf.Entry` borrows `data` only for the
    // `decode` call; `Glyf.Entry::decode` returns an owned `Outline` (Rust:
    // owned `Vec`s), so the scratch buffer is freed after `decode` returns.
    pub fn decode_glyf_payload(&self, max_allocation_bytes: usize) -> Result<Outline, DecodeError> {
        // Prep base64 decoding, initial validation.
        // const Decoder = std.base64.standard.Decoder;
        let decoder = std_base64_standard_decoder();
        // const payload_bytes = self.payload();
        let payload_bytes = self.payload();
        // const size = Decoder.calcSizeForSlice(payload_bytes) catch return error.MalformedPayload;
        let size = match decoder.calc_size_for_slice(payload_bytes) {
            Ok(size) => size,
            Err(_) => return Err(DecodeError::MalformedPayload),
        };
        // if (size > max_payload_size) return error.PayloadTooLarge;
        if size > MAX_PAYLOAD_SIZE {
            return Err(DecodeError::PayloadTooLarge);
        }

        // Max payload size is reasonable for stack and its likely
        // we'll have stack space. We don't use much stack space in
        // the future function calls either, so try a stack allocator
        // here and fallback to heap as necessary.
        // var data_stack = std.heap.stackFallback(max_payload_size, alloc);
        // const data_alloc = data_stack.get();
        // const data = try data_alloc.alloc(u8, size);
        // defer data_alloc.free(data);
        let mut data = vec![0u8; size];

        // Base64 decode
        // Decoder.decode(data, payload_bytes) catch return error.MalformedPayload;
        if decoder.decode(&mut data, payload_bytes).is_err() {
            return Err(DecodeError::MalformedPayload);
        }

        // Glyf.Entry borrows from `data`, but only for the duration of the
        // decode call below. Glyf.Entry.decode returns an owned Outline, so
        // it is safe to free `data` before returning that outline.
        // const glyf_entry = Glyf.Entry.init(data) catch return error.MalformedPayload;
        let glyf_entry = match GlyfEntry::init(&data) {
            Ok(entry) => entry,
            Err(_) => return Err(DecodeError::MalformedPayload),
        };
        // The points slice is the dominant decoded allocation. Limiting
        // every allocation made by the decoder prevents a compact record
        // from expanding into an unbounded persistent outline.
        // var limited: LimitedAllocator = .init(alloc, max_allocation_bytes);
        // return glyf_entry.decode(limited.allocator()) catch |err| switch (err) { ... };
        match glyf_entry.decode_limited(max_allocation_bytes) {
            Ok(outline) => Ok(outline),
            Err(err) => Err(match err {
                // error.OutOfMemory => if (limited.limit_exceeded)
                //     error.PayloadTooLarge
                // else
                //     error.OutOfMemory,
                // PORT NOTE: `decode_limited` only produces `OutOfMemory` when
                // the per-allocation limit is exceeded (real OOM aborts), so
                // the reference's `limit_exceeded` discriminant collapses to
                // the `PayloadTooLarge` arm.
                GlyfDecodeError::OutOfMemory => DecodeError::PayloadTooLarge,
                // error.CompositeNotSupported => error.CompositeUnsupported,
                GlyfDecodeError::CompositeNotSupported => DecodeError::CompositeUnsupported,
                // error.InstructionsNotSupported => error.HintingUnsupported,
                GlyfDecodeError::InstructionsNotSupported => DecodeError::HintingUnsupported,
                // error.EndOfStream, error.EndPointsOutOfOrder,
                // error.TooManyPoints, error.CoordinateOverflow => error.MalformedPayload,
                GlyfDecodeError::EndOfStream
                | GlyfDecodeError::EndPointsOutOfOrder
                | GlyfDecodeError::TooManyPoints
                | GlyfDecodeError::CoordinateOverflow => DecodeError::MalformedPayload,
            }),
        }
    }
}

/// Errors that can occur while decoding a register glyph payload.
// PORT NOTE: reference `pub const DecodeError = Allocator.Error || error{...}`.
// `Allocator.Error` (`OutOfMemory`) is preserved as a member for fidelity even
// though `decode_glyf_payload` cannot surface it (see its mapping note).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Allocation failure while decoding.
    OutOfMemory,

    /// The decoded payload exceeds the protocol limit.
    PayloadTooLarge,

    /// The payload could not be decoded or parsed as the declared format.
    MalformedPayload,

    /// The glyf payload is composite, which the protocol forbids.
    CompositeUnsupported,

    /// The glyf payload contains hinting instructions, which the
    /// protocol forbids.
    HintingUnsupported,
}

/// Options recognized for the glyph register verb.
// PORT NOTE: reference nested `Register.Option = enum {...}` with comptime
// `Type`, `default`, `key`, `read`. Hoisted to a module enum; the comptime
// per-option `Type` is reflected by the typed `read_*` helpers below, and the
// `default()` mapping is inlined into each `Register::get_*` accessor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterOption {
    /// Target Unicode codepoint encoded in hexadecimal.
    Cp,
    /// Glyph payload format.
    Fmt,
    /// Requested reply verbosity for registration.
    Reply,
    /// Units-per-em for the glyph coordinate system.
    Upm,
    /// Authored advance width in units-per-em units.
    Aw,
    /// Authored line height in units-per-em units.
    Lh,
    /// Unicode cell width for terminal layout.
    Width,
    /// Glyph scale policy.
    Size,
    /// Glyph placement within the render span.
    Align,
    /// Fractional insets from the render span edges.
    Pad,
}

impl RegisterOption {
    /// Return the wire-format option key for this register option.
    fn key(self) -> &'static [u8] {
        match self {
            // @tagName(self)
            RegisterOption::Cp => b"cp",
            RegisterOption::Fmt => b"fmt",
            RegisterOption::Reply => b"reply",
            RegisterOption::Upm => b"upm",
            RegisterOption::Aw => b"aw",
            RegisterOption::Lh => b"lh",
            RegisterOption::Width => b"width",
            RegisterOption::Size => b"size",
            // .@"align" => @tagName(self)
            RegisterOption::Align => b"align",
            RegisterOption::Pad => b"pad",
        }
    }

    // PORT NOTE: the reference's single comptime `read` returns `option.Type()`;
    // it is split into typed helpers, one per distinct return type, each
    // reproducing the matching `read` arm.

    // .cp => std.fmt.parseInt(u21, value, 16) catch null,
    fn read_cp(self, raw: &[u8]) -> Option<u32> {
        let value = option_value(raw, self.key())?;
        parse_u21(value, 16)
    }

    // .fmt => Format.init(value),
    fn read_fmt(self, raw: &[u8]) -> Option<Format> {
        let value = option_value(raw, self.key())?;
        Format::init(value)
    }

    // .reply => Reply.init(value) orelse .all,
    fn read_reply(self, raw: &[u8]) -> Option<Reply> {
        let value = option_value(raw, self.key())?;
        Some(Reply::init(value).unwrap_or(Reply::All))
    }

    // .upm, .aw, .lh => std.fmt.parseInt(u32, value, 10) catch null,
    fn read_u32(self, raw: &[u8], opt: RegisterOption) -> Option<u32> {
        let value = option_value(raw, opt.key())?;
        parse_u32(value, 10)
    }

    // .width => Width.init(value),
    fn read_width(self, raw: &[u8]) -> Option<Width> {
        let value = option_value(raw, self.key())?;
        Width::init(value)
    }

    // .size => Size.init(value),
    fn read_size(self, raw: &[u8]) -> Option<Size> {
        let value = option_value(raw, self.key())?;
        Size::init(value)
    }

    // .@"align" => Align.init(value),
    fn read_align(self, raw: &[u8]) -> Option<Align> {
        let value = option_value(raw, self.key())?;
        Align::init(value)
    }

    // .pad => Pad.init(value),
    fn read_pad(self, raw: &[u8]) -> Option<Pad> {
        let value = option_value(raw, self.key())?;
        Pad::init(value)
    }

    /// Return the protocol default value for this option, used by the
    /// `default()`-coverage test. The typed defaults are inlined into
    /// `Register::get_*` for the live decode path.
    // PORT NOTE: reference `default(comptime self) ?self.Type()`. Exposed as a
    // set of constant accessors mirroring each arm so the `register option
    // defaults` test can assert them.
    pub fn default_cp() -> Option<u32> {
        None
    }
    pub fn default_fmt() -> Option<Format> {
        Some(Format::Glyf)
    }
    pub fn default_upm() -> Option<u32> {
        Some(1000)
    }
    pub fn default_aw() -> Option<u32> {
        None
    }
    pub fn default_lh() -> Option<u32> {
        None
    }
    pub fn default_width() -> Option<Width> {
        Some(Width::Narrow)
    }
    pub fn default_size() -> Option<Size> {
        Some(Size::Height)
    }
    pub fn default_align() -> Option<Align> {
        Some(Align::default())
    }
    pub fn default_pad() -> Option<Pad> {
        Some(Pad::default())
    }
    pub fn default_reply() -> Option<Reply> {
        Some(Reply::All)
    }
}

/// Clear verb payload with lazily-decoded options.
#[derive(Debug, PartialEq, Eq)]
pub struct Clear {
    raw: Vec<u8>,
}

impl Clear {
    /// Initialize a clear command from owned raw command bytes.
    pub fn init(raw: Vec<u8>) -> Clear {
        Clear { raw }
    }

    /// Lazily decode a clear option on demand.
    // PORT NOTE: the only clear option is `cp`; `get` is specialized.
    pub fn get_cp(&self) -> Option<u32> {
        ClearOption::Cp.read(self.raw_options())
    }

    /// Return whether a clear option was provided, even if malformed.
    pub fn has_cp(&self) -> bool {
        ClearOption::Cp.present(self.raw_options())
    }

    /// Return the raw option portion of a valid clear command.
    fn raw_options(&self) -> &[u8] {
        // assert(self.raw.len >= 2);
        assert!(self.raw.len() >= 2);
        // assert(self.raw[0] == 'c');
        assert!(self.raw[0] == b'c');
        // assert(self.raw[1] == ';');
        assert!(self.raw[1] == b';');
        &self.raw[2..]
    }
}

/// Options recognized for the glyph clear request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearOption {
    /// Target Unicode codepoint encoded in hexadecimal.
    Cp,
}

impl ClearOption {
    /// Return the wire-format option key for this clear option.
    fn key(self) -> &'static [u8] {
        match self {
            ClearOption::Cp => b"cp",
        }
    }

    /// Read and decode a clear option from the raw option string.
    fn read(self, raw: &[u8]) -> Option<u32> {
        let value = option_value(raw, self.key())?;
        match self {
            // .cp => std.fmt.parseInt(u21, value, 16) catch null,
            ClearOption::Cp => parse_u21(value, 16),
        }
    }

    /// Return whether the option is present in the raw option string,
    /// independent of whether its value can be decoded.
    fn present(self, raw: &[u8]) -> bool {
        // return optionValue(raw, self.key()) != null;
        option_value(raw, self.key()).is_some()
    }
}

/// Glyph payload formats named by the protocol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// TrueType simple glyph outline data.
    Glyf,

    /// OpenType COLR version 0 layered color glyph data.
    Colrv0,

    /// OpenType COLR version 1 paint graph glyph data.
    Colrv1,
}

impl Format {
    /// Parse a glyph payload format name.
    // PORT NOTE: reference `std.meta.stringToEnum(Format, value)` matches the
    // exact `@tagName` of each variant.
    pub fn init(value: &[u8]) -> Option<Format> {
        match value {
            b"glyf" => Some(Format::Glyf),
            b"colrv0" => Some(Format::Colrv0),
            b"colrv1" => Some(Format::Colrv1),
            _ => None,
        }
    }
}

/// Register command reply verbosity.
// PORT NOTE: reference `pub const Reply = enum(u2) { none = 0, all = 1,
// failures = 2 }`; the explicit discriminants are preserved via `repr(u8)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Reply {
    /// Suppress both success and failure replies.
    None = 0,

    /// Emit replies for both success and failure cases.
    All = 1,

    /// Emit replies only for failure cases.
    Failures = 2,
}

impl Reply {
    /// Parse the register command reply mode from its single-digit encoding.
    pub fn init(value: &[u8]) -> Option<Reply> {
        // if (value.len != 1) return null;
        if value.len() != 1 {
            return None;
        }
        match value[0] {
            b'0' => Some(Reply::None),
            b'1' => Some(Reply::All),
            b'2' => Some(Reply::Failures),
            _ => None,
        }
    }
}

/// Register command width override for terminal layout.
// PORT NOTE: reference `pub const Width = enum(u2) { narrow = 1, wide = 2 }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Width {
    /// One terminal cell.
    Narrow = 1,

    /// Two terminal cells.
    Wide = 2,
}

impl Width {
    /// Parse the register command width from its single-digit encoding.
    pub fn init(value: &[u8]) -> Option<Width> {
        // if (value.len != 1) return null;
        if value.len() != 1 {
            return None;
        }
        match value[0] {
            b'1' => Some(Width::Narrow),
            b'2' => Some(Width::Wide),
            _ => None,
        }
    }
}

/// Register command glyph scale policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Height,
    Advance,
    Contain,
    Cover,
    Stretch,
}

impl Size {
    /// Parse a glyph scale policy name.
    pub fn init(value: &[u8]) -> Option<Size> {
        match value {
            b"height" => Some(Size::Height),
            b"advance" => Some(Size::Advance),
            b"contain" => Some(Size::Contain),
            b"cover" => Some(Size::Cover),
            b"stretch" => Some(Size::Stretch),
            _ => None,
        }
    }
}

/// Register command glyph placement within the render span.
// PORT NOTE: reference `pub const Align = struct { horizontal: Horizontal =
// .center, vertical: Vertical = .center }`; the field defaults are reproduced
// by `Default`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Align {
    pub horizontal: AlignHorizontal,
    pub vertical: AlignVertical,
}

impl Default for Align {
    fn default() -> Align {
        Align {
            horizontal: AlignHorizontal::Center,
            vertical: AlignVertical::Center,
        }
    }
}

/// Horizontal alignment names.
// PORT NOTE: reference nested `Align.Horizontal = enum { start, center, end }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignHorizontal {
    Start,
    Center,
    End,
}

impl AlignHorizontal {
    fn init(value: &[u8]) -> Option<AlignHorizontal> {
        match value {
            b"start" => Some(AlignHorizontal::Start),
            b"center" => Some(AlignHorizontal::Center),
            b"end" => Some(AlignHorizontal::End),
            _ => None,
        }
    }
}

/// Vertical alignment names.
// PORT NOTE: reference nested `Align.Vertical = enum { start, center, end,
// baseline }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignVertical {
    Start,
    Center,
    End,
    Baseline,
}

impl AlignVertical {
    fn init(value: &[u8]) -> Option<AlignVertical> {
        match value {
            b"start" => Some(AlignVertical::Start),
            b"center" => Some(AlignVertical::Center),
            b"end" => Some(AlignVertical::End),
            b"baseline" => Some(AlignVertical::Baseline),
            _ => None,
        }
    }
}

impl Align {
    /// Parse an align value in `<horizontal>,<vertical>` form.
    pub fn init(value: &[u8]) -> Option<Align> {
        // var it = std.mem.splitScalar(u8, value, ',');
        let mut it = value.split(|&b| b == b',');
        // const horizontal = Horizontal.init(it.next() orelse return null) orelse return null;
        let horizontal = AlignHorizontal::init(it.next()?)?;
        // const vertical = Vertical.init(it.next() orelse return null) orelse return null;
        let vertical = AlignVertical::init(it.next()?)?;
        // if (it.next() != null) return null;
        if it.next().is_some() {
            return None;
        }

        Some(Align {
            horizontal,
            vertical,
        })
    }
}

/// Register command fractional insets from the render span edges.
// PORT NOTE: reference `pub const Pad = struct { top/right/bottom/left: f64 = 0 }`.
// `f64` precludes `Eq`/`Hash`; tests compare via `expectEqual` on exact f64
// values parsed from the same literal strings, so `PartialEq` suffices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pad {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Default for Pad {
    fn default() -> Pad {
        Pad {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }
}

impl Pad {
    /// Parse a pad value in `<top>,<right>,<bottom>,<left>` form.
    pub fn init(value: &[u8]) -> Option<Pad> {
        // var it = std.mem.splitScalar(u8, value, ',');
        let mut it = value.split(|&b| b == b',');
        // const top = parseFraction(it.next() orelse return null) orelse return null;
        let top = parse_fraction(it.next()?)?;
        // const right = parseFraction(it.next() orelse return null) orelse return null;
        let right = parse_fraction(it.next()?)?;
        // const bottom = parseFraction(it.next() orelse return null) orelse return null;
        let bottom = parse_fraction(it.next()?)?;
        // const left = parseFraction(it.next() orelse return null) orelse return null;
        let left = parse_fraction(it.next()?)?;
        // if (it.next() != null) return null;
        if it.next().is_some() {
            return None;
        }

        // Glyph Protocol §8.5.2: "If `l + r ≥ 1` or `t + b ≥ 1`
        // the terminal MUST treat the request as if `pad=0,0,0,0`."
        // if (left + right >= 1 or top + bottom >= 1) return .{};
        if left + right >= 1.0 || top + bottom >= 1.0 {
            return Some(Pad::default());
        }

        Some(Pad {
            top,
            right,
            bottom,
            left,
        })
    }
}

/// Parse one pad component from the spec's `0.0`–`1.0` fractional range.
/// Top/bottom fractions are relative to cell height; left/right fractions
/// are relative to render span width.
// PORT NOTE (`946422dbc`): `std.fmt.parseFloat` + the explicit `[0, 1]`
// range check became `return fraction.parse(value);` — the fraction parser
// carries the range check (and rejects exponent/hex-float syntax the old
// parser accepted).
fn parse_fraction(value: &[u8]) -> Option<f64> {
    // return fraction.parse(value);
    crate::fraction::parse(value)
}

/// Find the last occurrence of `key=value` for a lazily-parsed option list.
// PORT NOTE: reference `optionValue(raw, comptime key: []const u8) ?[]const u8`.
// The `comptime key` is a runtime `&[u8]` here; behavior is identical.
fn option_value<'a>(raw: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    // var remaining = raw;
    let mut remaining = raw;
    // var result: ?[]const u8 = null;
    let mut result: Option<&[u8]> = None;
    // while (remaining.len > 0) {
    while !remaining.is_empty() {
        // Options are semicolon-delimited, so each loop peels off one segment
        // and checks whether it matches the requested key.
        // const len = std.mem.indexOfScalar(u8, remaining, ';') orelse remaining.len;
        let len = remaining
            .iter()
            .position(|&b| b == b';')
            .unwrap_or(remaining.len());
        // const full = remaining[0..len];
        let full = &remaining[0..len];

        // if (std.mem.indexOfScalar(u8, full, '=')) |eql_idx| {
        if let Some(eql_idx) = full.iter().position(|&b| b == b'=') {
            // if (std.mem.eql(u8, full[0..eql_idx], key)) {
            if &full[0..eql_idx] == key {
                // result = full[eql_idx + 1 ..];
                result = Some(&full[eql_idx + 1..]);
            }
        }

        // if (len == remaining.len) break;
        if len == remaining.len() {
            break;
        }
        // remaining = remaining[len + 1 ..];
        remaining = &remaining[len + 1..];
    }

    result
}

/// `std.fmt.parseInt(u21, value, radix) catch null` — parse and bound to u21.
// PORT NOTE: Rust has no `u21`; parse into `u32` and reject values that exceed
// the u21 range (`> 0x1F_FFFF`), reproducing the reference's `error.Overflow`
// (→ `null`). `from_str_radix` rejects empty input and non-`[0-9A-Fa-f]` bytes
// (→ `null`), matching `error.InvalidCharacter`. A leading `+` is rejected here
// (Zig accepts it), but no protocol path exercises a signed codepoint.
fn parse_u21(value: &[u8], radix: u32) -> Option<u32> {
    let s = std::str::from_utf8(value).ok()?;
    let parsed = u32::from_str_radix(s, radix).ok()?;
    if parsed > 0x1F_FFFF {
        return None;
    }
    Some(parsed)
}

/// `std.fmt.parseInt(u32, value, radix) catch null`.
fn parse_u32(value: &[u8], radix: u32) -> Option<u32> {
    let s = std::str::from_utf8(value).ok()?;
    u32::from_str_radix(s, radix).ok()
}

// PORT NOTE (`946422dbc`): the `parse_float` helper (`std.fmt.parseFloat(f64,
// value) catch null`) is deleted with its only caller's switch to
// `crate::fraction::parse` — the reference removed its `parseFloat` dependency
// entirely.

/// `std.base64.standard.Decoder`.
// PORT NOTE: reference `std.base64.standard.Decoder` is `Base64Decoder.init(
// standard_alphabet_chars, '=')`. `rustty_simd`'s `Base64Decoder` is a verbatim
// port of the same `std/base64.zig` `Base64Decoder` but is a private submodule,
// so the standard decoder is re-derived here from `std/base64.zig` (Zig 0.15.2)
// with the standard alphabet and `'='` pad char — the exact decoder the
// reference's `decodeGlyfPayload` uses.
// PORT NOTE: exposed `pub(crate)` so the protocol-neutral clipboard_write
// effect (stream_terminal, BL-5 634ef7198) can share the exact same strict
// `std.base64.standard.Decoder` the reference `clipboardContents` uses.
pub(crate) fn std_base64_standard_decoder() -> Base64Decoder {
    Base64Decoder::init(std_base64_standard_alphabet_chars(), Some(b'='))
}

/// Zig's `std.base64.standard_alphabet_chars`.
fn std_base64_standard_alphabet_chars() -> [u8; 64] {
    *b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
}

/// Decode error of the std base64 decoder.
// PORT NOTE: `std/base64.zig` `Error = error{ InvalidCharacter, InvalidPadding,
// NoSpaceLeft }`. The decode paths below only construct `InvalidCharacter` /
// `InvalidPadding`; `NoSpaceLeft` is preserved for fidelity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Base64Error {
    InvalidCharacter,
    InvalidPadding,
    #[allow(dead_code)]
    NoSpaceLeft,
}

/// Faithful port of `std/base64.zig`'s `Base64Decoder` (Zig 0.15.2).
pub(crate) struct Base64Decoder {
    char_to_index: [u8; 256],
    fast_char_to_index: [[u32; 256]; 4],
    pad_char: Option<u8>,
}

impl Base64Decoder {
    const INVALID_CHAR: u8 = 0xff;
    const INVALID_CHAR_TST: u32 = 0xff000000;

    fn init(alphabet_chars: [u8; 64], pad_char: Option<u8>) -> Base64Decoder {
        let mut result = Base64Decoder {
            char_to_index: [Self::INVALID_CHAR; 256],
            fast_char_to_index: [[Self::INVALID_CHAR_TST; 256]; 4],
            pad_char,
        };

        let mut char_in_alphabet = [false; 256];
        for (i, &c) in alphabet_chars.iter().enumerate() {
            // assert(!char_in_alphabet[c]);
            assert!(!char_in_alphabet[c as usize]);
            // assert(pad_char == null or c != pad_char.?);
            assert!(match pad_char {
                Some(p) => c != p,
                None => true,
            });

            let ci = i as u32;
            result.fast_char_to_index[0][c as usize] = ci << 2;
            result.fast_char_to_index[1][c as usize] = (ci >> 4) | ((ci & 0x0f) << 12);
            result.fast_char_to_index[2][c as usize] = ((ci & 0x3) << 22) | ((ci & 0x3c) << 6);
            result.fast_char_to_index[3][c as usize] = ci << 16;

            result.char_to_index[c as usize] = i as u8;
            char_in_alphabet[c as usize] = true;
        }
        result
    }

    // PORT NOTE: `% 4 != 0` / `% 4 == 1` mirror `std/base64.zig`'s modulo checks;
    // `#[allow(clippy::manual_is_multiple_of)]` keeps them verbatim.
    #[allow(clippy::manual_is_multiple_of)]
    fn calc_size_upper_bound(&self, source_len: usize) -> Result<usize, Base64Error> {
        let mut result = source_len / 4 * 3;
        let leftover = source_len % 4;
        if self.pad_char.is_some() {
            // if (leftover % 4 != 0) return error.InvalidPadding;
            if leftover % 4 != 0 {
                return Err(Base64Error::InvalidPadding);
            }
        } else {
            if leftover % 4 == 1 {
                return Err(Base64Error::InvalidPadding);
            }
            result += leftover * 3 / 4;
        }
        Ok(result)
    }

    pub(crate) fn calc_size_for_slice(&self, source: &[u8]) -> Result<usize, Base64Error> {
        let source_len = source.len();
        let mut result = self.calc_size_upper_bound(source_len)?;
        if let Some(pad_char) = self.pad_char {
            if source_len >= 1 && source[source_len - 1] == pad_char {
                result -= 1;
            }
            if source_len >= 2 && source[source_len - 2] == pad_char {
                result -= 1;
            }
        }
        Ok(result)
    }

    // PORT NOTE: `source.len() % 4 != 0` mirrors `std/base64.zig`'s modulo check;
    // `#[allow(clippy::manual_is_multiple_of)]` keeps it verbatim.
    #[allow(clippy::manual_is_multiple_of)]
    pub(crate) fn decode(&self, dest: &mut [u8], source: &[u8]) -> Result<(), Base64Error> {
        // if (decoder.pad_char != null and source.len % 4 != 0) return error.InvalidPadding;
        if self.pad_char.is_some() && source.len() % 4 != 0 {
            return Err(Base64Error::InvalidPadding);
        }
        let mut dest_idx: usize = 0;
        let mut fast_src_idx: usize = 0;
        let mut acc: u16 = 0; // u12
        let mut acc_len: u8 = 0; // u4
        let mut leftover_idx: Option<usize> = None;
        while fast_src_idx + 16 < source.len() && dest_idx + 15 < dest.len() {
            let mut bits: u128 = 0;
            let mut i = 0;
            while i < 4 {
                let mut new_bits: u128 =
                    self.fast_char_to_index[0][source[fast_src_idx + i * 4] as usize] as u128;
                new_bits |=
                    self.fast_char_to_index[1][source[fast_src_idx + 1 + i * 4] as usize] as u128;
                new_bits |=
                    self.fast_char_to_index[2][source[fast_src_idx + 2 + i * 4] as usize] as u128;
                new_bits |=
                    self.fast_char_to_index[3][source[fast_src_idx + 3 + i * 4] as usize] as u128;
                if (new_bits & Self::INVALID_CHAR_TST as u128) != 0 {
                    return Err(Base64Error::InvalidCharacter);
                }
                bits |= new_bits << (24 * i);
                i += 1;
            }
            dest[dest_idx..][0..16].copy_from_slice(&bits.to_le_bytes());
            fast_src_idx += 16;
            dest_idx += 12;
        }
        while fast_src_idx + 4 < source.len() && dest_idx + 3 < dest.len() {
            let mut bits = self.fast_char_to_index[0][source[fast_src_idx] as usize];
            bits |= self.fast_char_to_index[1][source[fast_src_idx + 1] as usize];
            bits |= self.fast_char_to_index[2][source[fast_src_idx + 2] as usize];
            bits |= self.fast_char_to_index[3][source[fast_src_idx + 3] as usize];
            if (bits & Self::INVALID_CHAR_TST) != 0 {
                return Err(Base64Error::InvalidCharacter);
            }
            dest[dest_idx..][0..4].copy_from_slice(&bits.to_le_bytes());
            fast_src_idx += 4;
            dest_idx += 3;
        }
        let remaining = &source[fast_src_idx..];
        for (offset, &c) in remaining.iter().enumerate() {
            let src_idx = fast_src_idx + offset;
            let d = self.char_to_index[c as usize];
            // if (d == invalid_char) {
            if d == Self::INVALID_CHAR {
                // if (decoder.pad_char == null or c != decoder.pad_char.?) return error.InvalidCharacter;
                if self.pad_char.is_none() || Some(c) != self.pad_char {
                    return Err(Base64Error::InvalidCharacter);
                }
                leftover_idx = Some(src_idx);
                break;
            }
            // acc = (acc << 6) + d;
            acc = (acc << 6) + d as u16;
            acc_len += 6;
            if acc_len >= 8 {
                acc_len -= 8;
                // dest[dest_idx] = @truncate(acc >> acc_len);
                dest[dest_idx] = (acc >> acc_len) as u8;
                dest_idx += 1;
            }
        }
        // if (acc_len > 4 or (acc & (@as(u12, 1) << acc_len) - 1) != 0) return error.InvalidPadding;
        // PORT NOTE: the u12 mask `(1 << acc_len) - 1` keeps only the low 12
        // bits; `acc` is held in a `u16` and never exceeds 12 significant bits,
        // so masking with `(1u16 << acc_len) - 1` reproduces the u12 arithmetic.
        if acc_len > 4 || (acc & ((1u16 << acc_len) - 1)) != 0 {
            return Err(Base64Error::InvalidPadding);
        }
        // if (leftover_idx == null) return;
        let leftover_idx = match leftover_idx {
            Some(idx) => idx,
            None => return Ok(()),
        };
        let leftover = &source[leftover_idx..];
        if let Some(pad_char) = self.pad_char {
            let padding_len = acc_len / 2;
            let mut padding_chars: usize = 0;
            for &c in leftover {
                if c != pad_char {
                    // return if (c == invalid_char) error.InvalidCharacter else error.InvalidPadding;
                    return Err(if c == Base64Decoder::INVALID_CHAR {
                        Base64Error::InvalidCharacter
                    } else {
                        Base64Error::InvalidPadding
                    });
                }
                padding_chars += 1;
            }
            // if (padding_chars != padding_len) return error.InvalidPadding;
            if padding_chars != padding_len as usize {
                return Err(Base64Error::InvalidPadding);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: reference `testParse(alloc, data) CommandParser.Error!Request`
    // feeds each byte through a `CommandParser` and returns the completed
    // `Request`. The `alloc` parameter is dropped (no allocator threading).
    fn test_parse(data: &[u8]) -> Result<Request, CommandParserError> {
        // var parser = CommandParser.init(alloc, 1024 * 1024);
        let mut parser = CommandParser::init(1024 * 1024);
        // defer parser.deinit();
        // for (data) |byte| try parser.feed(byte);
        for &byte in data {
            parser.feed(byte)?;
        }
        // return try parser.complete(alloc);
        parser.complete()
    }

    #[test]
    fn support_command() {
        // var cmd = try testParse(testing.allocator, "s");
        let cmd = test_parse(b"s").unwrap();
        // try testing.expect(cmd == .support);
        assert!(matches!(cmd, Request::Support));
    }

    #[test]
    fn query_command() {
        // var cmd = try testParse(testing.allocator, "q;cp=E0A0");
        let cmd = test_parse(b"q;cp=E0A0").unwrap();
        // try testing.expect(cmd == .query);
        let Request::Query(qry) = &cmd else {
            panic!("expected query");
        };
        // try testing.expectEqual(@as(u21, 0xE0A0), cmd.query.get(.cp).?);
        assert_eq!(0xE0A0u32, qry.get_cp().unwrap());
    }

    #[test]
    fn register_command_with_payload() {
        // var cmd = try testParse(..., "r;cp=e0a0;fmt=glyf;upm=1000;reply=2;QQ==");
        let cmd = test_parse(b"r;cp=e0a0;fmt=glyf;upm=1000;reply=2;QQ==").unwrap();

        // try testing.expect(cmd == .register);
        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        // try testing.expectEqual(@as(u21, 0xE0A0), cmd.register.get(.cp).?);
        assert_eq!(0xE0A0u32, reg.get_cp().unwrap());
        // try testing.expectEqual(Format.glyf, cmd.register.get(.fmt).?);
        assert_eq!(Format::Glyf, reg.get_fmt().unwrap());
        // try testing.expectEqual(@as(u32, 1000), cmd.register.get(.upm).?);
        assert_eq!(1000u32, reg.get_upm().unwrap());
        // try testing.expectEqual(Reply.failures, cmd.register.get(.reply).?);
        assert_eq!(Reply::Failures, reg.get_reply().unwrap());
        // try testing.expectEqualStrings("QQ==", cmd.register.payload());
        assert_eq!(b"QQ==", reg.payload());
    }

    #[test]
    fn register_command_with_sizing_and_placement_options() {
        let cmd = test_parse(
            b"r;cp=e0a0;upm=2048;aw=1024;lh=1536;width=2;size=contain;align=end,baseline;pad=0.1,0.2,0.3,0.4;QQ==",
        )
        .unwrap();

        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        // try testing.expectEqual(@as(u32, 2048), cmd.register.get(.upm).?);
        assert_eq!(2048u32, reg.get_upm().unwrap());
        // try testing.expectEqual(@as(u32, 1024), cmd.register.get(.aw).?);
        assert_eq!(1024u32, reg.get_aw().unwrap());
        // try testing.expectEqual(@as(u32, 1536), cmd.register.get(.lh).?);
        assert_eq!(1536u32, reg.get_lh().unwrap());
        // try testing.expectEqual(Width.wide, cmd.register.get(.width).?);
        assert_eq!(Width::Wide, reg.get_width().unwrap());
        // try testing.expectEqual(Size.contain, cmd.register.get(.size).?);
        assert_eq!(Size::Contain, reg.get_size().unwrap());
        // try testing.expectEqual(Align{ .horizontal = .end, .vertical = .baseline }, ...);
        assert_eq!(
            Align {
                horizontal: AlignHorizontal::End,
                vertical: AlignVertical::Baseline,
            },
            reg.get_align().unwrap()
        );
        // try testing.expectEqual(Pad{ .top=0.1, .right=0.2, .bottom=0.3, .left=0.4 }, ...);
        assert_eq!(
            Pad {
                top: 0.1,
                right: 0.2,
                bottom: 0.3,
                left: 0.4,
            },
            reg.get_pad().unwrap()
        );
        // try testing.expectEqualStrings("QQ==", cmd.register.payload());
        assert_eq!(b"QQ==", reg.payload());
    }

    #[test]
    fn register_option_defaults() {
        // const Option = Request.Register.Option;
        // try testing.expect(Option.cp.default() == null);
        assert!(RegisterOption::default_cp().is_none());
        // try testing.expectEqual(Format.glyf, Option.fmt.default().?);
        assert_eq!(Format::Glyf, RegisterOption::default_fmt().unwrap());
        // try testing.expectEqual(@as(u32, 1000), Option.upm.default().?);
        assert_eq!(1000u32, RegisterOption::default_upm().unwrap());
        // try testing.expect(Option.aw.default() == null);
        assert!(RegisterOption::default_aw().is_none());
        // try testing.expect(Option.lh.default() == null);
        assert!(RegisterOption::default_lh().is_none());
        // try testing.expectEqual(Width.narrow, Option.width.default().?);
        assert_eq!(Width::Narrow, RegisterOption::default_width().unwrap());
        // try testing.expectEqual(Size.height, Option.size.default().?);
        assert_eq!(Size::Height, RegisterOption::default_size().unwrap());
        // try testing.expectEqual(Align{}, Option.@"align".default().?);
        assert_eq!(Align::default(), RegisterOption::default_align().unwrap());
        // try testing.expectEqual(Pad{}, Option.pad.default().?);
        assert_eq!(Pad::default(), RegisterOption::default_pad().unwrap());
        // try testing.expectEqual(Reply.all, Option.reply.default().?);
        assert_eq!(Reply::All, RegisterOption::default_reply().unwrap());
    }

    #[test]
    fn register_command_defaults() {
        // var cmd = try testParse(testing.allocator, "r;cp=e0a0;QQ==");
        let cmd = test_parse(b"r;cp=e0a0;QQ==").unwrap();

        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        assert_eq!(0xE0A0u32, reg.get_cp().unwrap());
        assert_eq!(Format::Glyf, reg.get_fmt().unwrap());
        assert_eq!(1000u32, reg.get_upm().unwrap());
        assert_eq!(1000u32, reg.get_aw().unwrap());
        assert_eq!(1000u32, reg.get_lh().unwrap());
        assert_eq!(Width::Narrow, reg.get_width().unwrap());
        assert_eq!(Size::Height, reg.get_size().unwrap());
        assert_eq!(Align::default(), reg.get_align().unwrap());
        assert_eq!(Pad::default(), reg.get_pad().unwrap());
        assert_eq!(Reply::All, reg.get_reply().unwrap());
    }

    #[test]
    fn register_command_aw_and_lh_default_to_upm() {
        // var cmd = try testParse(testing.allocator, "r;cp=e0a0;upm=2048;QQ==");
        let cmd = test_parse(b"r;cp=e0a0;upm=2048;QQ==").unwrap();

        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        // try testing.expectEqual(@as(u32, 2048), cmd.register.get(.aw).?);
        assert_eq!(2048u32, reg.get_aw().unwrap());
        // try testing.expectEqual(@as(u32, 2048), cmd.register.get(.lh).?);
        assert_eq!(2048u32, reg.get_lh().unwrap());
    }

    #[test]
    fn register_command_invalid_sizing_and_placement_options() {
        let cmd =
            test_parse(b"r;cp=e0a0;width=3;size=invalid;align=center,middle;pad=0,1.2,0,0;QQ==")
                .unwrap();

        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        // try testing.expect(cmd.register.get(.width) == null);
        assert!(reg.get_width().is_none());
        // try testing.expect(cmd.register.get(.size) == null);
        assert!(reg.get_size().is_none());
        // try testing.expect(cmd.register.get(.@"align") == null);
        assert!(reg.get_align().is_none());
        // try testing.expect(cmd.register.get(.pad) == null);
        assert!(reg.get_pad().is_none());
    }

    #[test]
    fn register_command_degenerate_padding_defaults_to_no_padding() {
        // var cmd = try testParse(..., "r;cp=e0a0;pad=0.4,0.2,0.6,0.1;QQ==");
        let cmd = test_parse(b"r;cp=e0a0;pad=0.4,0.2,0.6,0.1;QQ==").unwrap();

        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        // try testing.expectEqual(Pad{}, cmd.register.get(.pad).?);
        assert_eq!(Pad::default(), reg.get_pad().unwrap());
    }

    #[test]
    fn register_command_invalid_reply_falls_back_to_reply_1() {
        // var cmd = try testParse(testing.allocator, "r;cp=e0a0;reply=9;QQ==");
        let cmd = test_parse(b"r;cp=e0a0;reply=9;QQ==").unwrap();

        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        // try testing.expectEqual(Reply.all, cmd.register.get(.reply).?);
        assert_eq!(Reply::All, reg.get_reply().unwrap());
    }

    #[test]
    fn register_command_duplicate_options_use_the_last_value() {
        // var cmd = try testParse(..., "r;cp=e0a0;reply=1;reply=2;QQ==");
        let cmd = test_parse(b"r;cp=e0a0;reply=1;reply=2;QQ==").unwrap();

        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        // try testing.expectEqual(Reply.failures, cmd.register.get(.reply).?);
        assert_eq!(Reply::Failures, reg.get_reply().unwrap());
    }

    #[test]
    fn register_command_with_invalid_payload() {
        // var cmd = try testParse(..., "r;cp=e0a0;fmt=glyf;%%%not-base64%%%");
        let cmd = test_parse(b"r;cp=e0a0;fmt=glyf;%%%not-base64%%%").unwrap();

        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        assert_eq!(0xE0A0u32, reg.get_cp().unwrap());
        assert_eq!(Format::Glyf, reg.get_fmt().unwrap());
        // try testing.expectEqualStrings("%%%not-base64%%%", cmd.register.payload());
        assert_eq!(b"%%%not-base64%%%", reg.payload());
    }

    #[test]
    fn register_command_rejects_missing_payload_separator() {
        // for ([_][]const u8{ "r", "r;cp=e0a0", "r;foo" }) |data| {
        for data in [
            b"r".as_slice(),
            b"r;cp=e0a0".as_slice(),
            b"r;foo".as_slice(),
        ] {
            // try testing.expectError(error.InvalidFormat, testParse(...));
            assert_eq!(Err(CommandParserError::InvalidFormat), test_parse(data));
        }
    }

    #[test]
    fn register_decodes_glyf_payload() {
        // var cmd = try testParse(..., "r;cp=e0a0;AAAAAAAAAAAAAA==");
        let cmd = test_parse(b"r;cp=e0a0;AAAAAAAAAAAAAA==").unwrap();

        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        // var outline = try cmd.register.decodeGlyfPayload(
        //     testing.allocator,
        //     std.math.maxInt(usize),
        // );
        let outline = reg.decode_glyf_payload(usize::MAX).unwrap();
        // try testing.expectEqual(@as(usize, 0), outline.points.len);
        assert_eq!(0usize, outline.points.len());
        // try testing.expectEqual(@as(usize, 0), outline.contours.len);
        assert_eq!(0usize, outline.contours.len());
    }

    #[test]
    fn register_rejects_malformed_glyf_payload() {
        // var cmd = try testParse(..., "r;cp=e0a0;%%%not-base64%%%");
        let cmd = test_parse(b"r;cp=e0a0;%%%not-base64%%%").unwrap();

        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        // try testing.expectError(
        //     error.MalformedPayload,
        //     cmd.register.decodeGlyfPayload(testing.allocator, std.math.maxInt(usize)),
        // );
        // PORT NOTE: `Outline` (the Ok payload) has no `PartialEq`/`Debug`, so
        // the result is mapped to `()` before the `assert_eq!` error comparison.
        assert_eq!(
            Err(DecodeError::MalformedPayload),
            reg.decode_glyf_payload(usize::MAX).map(|_| ())
        );
    }

    #[test]
    fn register_response_without_payload() {
        // var cmd = try testParse(..., "r;cp=E0A0;status=4;reason=out_of_namespace");
        let cmd = test_parse(b"r;cp=E0A0;status=4;reason=out_of_namespace").unwrap();

        // Register parsing is request-only, so the final segment is always treated
        // as payload rather than as a response field.
        let Request::Register(reg) = &cmd else {
            panic!("expected register");
        };
        assert_eq!(0xE0A0u32, reg.get_cp().unwrap());
        // try testing.expectEqualStrings("reason=out_of_namespace", cmd.register.payload());
        assert_eq!(b"reason=out_of_namespace", reg.payload());
    }

    #[test]
    fn clear_command() {
        // var cmd = try testParse(testing.allocator, "c;cp=e0a0");
        let cmd = test_parse(b"c;cp=e0a0").unwrap();

        // try testing.expect(cmd == .clear);
        let Request::Clear(clr) = &cmd else {
            panic!("expected clear");
        };
        // try testing.expect(cmd.clear.has(.cp));
        assert!(clr.has_cp());
        // try testing.expectEqual(@as(u21, 0xE0A0), cmd.clear.get(.cp).?);
        assert_eq!(0xE0A0u32, clr.get_cp().unwrap());
    }

    #[test]
    fn clear_command_tracks_malformed_cp_presence() {
        // for ([_][]const u8{ "c;cp=zz", "c;cp=", "c;cp=200000" }) |data| {
        for data in [
            b"c;cp=zz".as_slice(),
            b"c;cp=".as_slice(),
            b"c;cp=200000".as_slice(),
        ] {
            let cmd = test_parse(data).unwrap();

            let Request::Clear(clr) = &cmd else {
                panic!("expected clear");
            };
            // try testing.expect(cmd.clear.has(.cp));
            assert!(clr.has_cp());
            // try testing.expect(cmd.clear.get(.cp) == null);
            assert!(clr.get_cp().is_none());
        }
    }

    #[test]
    fn invalid_command() {
        // try testing.expectError(error.InvalidFormat, testParse(testing.allocator, "x"));
        assert_eq!(Err(CommandParserError::InvalidFormat), test_parse(b"x"));
    }
}
