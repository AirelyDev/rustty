// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const simd = @import("../../simd/main.zig");
// const lib = @import("../lib.zig");
use rustty_lib::r#enum::EnumType;
use rustty_lib::target::Target;
use rustty_simd as simd;
use rustty_util::quirks::inlineAssert as assert;

// const log = std.log.scoped(.kitty_gfx);

/// PORT NOTE: The reference's parsing surface returns Zig errors
/// (`error.InvalidFormat`, `error.Overflow`, `error.OutOfMemory`,
/// `error.InvalidData`). This enum models that error set, one variant per
/// reference error, so every `return error.X` and `try` propagation maps
/// assertion-for-assertion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidFormat,
    Overflow,
    OutOfMemory,
    InvalidData,
}

/// The key-value pairs for the control information for a command. The
/// recognized keys are always single ASCII letters and the values are either
/// single characters or 32-bit unsigned integers. Unknown non-letter keys are
/// ignored, matching how unknown letter keys are ignored when building a
/// command.
///
/// This is deliberately a dense value table plus a presence bitmap. There are
/// only 52 possible keys(technically KIP has fever than 52 possible .. at the moment of writing), so a lookup is one machine-word bit test followed by
/// an indexed load. The values are undefined until their presence bit is set.
///
/// Other alternatives were considered and should probably be revisited in the
/// future. Some of them were:
/// A `[52]?u32` table is 416 bytes versus 216 bytes for this layout, and
/// its larger copies erased the smaller lookup code.
/// `std.EnumMap` added optional-return lowering,
/// `std.StaticBitSet(52)` kept the same size but generated larger code
/// for the packed mask. Passing the table by pointer removed the copies
/// but did not improve end-to-end throughput. Keep this representation
/// unless a new benchmark shows an improvement in the zig compilers codegen.
///
/// For the value itself: if it is a single printable ASCII character it is the
/// ASCII code. Otherwise, it is parsed as a 32-bit unsigned integer.
///
/// PORT NOTE: the alternatives and byte/codegen measurements above are the
/// reference's, taken against the Zig compiler's codegen (`?u32`,
/// `std.EnumMap`, `std.StaticBitSet` are Zig constructs); the port keeps the
/// chosen representation: a `[u32; 52]` dense table plus a `u64` presence
/// bitmap.
// const KV = struct {
//     values: [52]u32 = undefined,
//     present: u64 = 0,
struct Kv {
    /// PORT NOTE: `= undefined` in the reference; zero-initialized here (same
    /// treatment as `Parser.kv_temp`), which is never observable because
    /// `get` refuses to read a slot whose presence bit is clear.
    values: [u32; 52],
    present: u64,
}

impl Kv {
    // fn index(key: u8) ?u6 {
    //     return switch (key) {
    //         'a'...'z' => @intCast(key - 'a'),
    //         'A'...'Z' => @intCast(26 + key - 'A'),
    //         else => null,
    //     };
    // }
    /// PORT NOTE: the reference returns `?u6`; Rust has no `u6`, so `u8`
    /// carries the 0..=51 index (the `@intCast` narrowing has no work to do
    /// either way, both range arms are already in-bounds).
    fn index(key: u8) -> Option<u8> {
        match key {
            b'a'..=b'z' => Some(key - b'a'),
            b'A'..=b'Z' => Some(26 + key - b'A'),
            _ => None,
        }
    }

    // fn get(self: *const KV, key: u8) ?u32 {
    //     const idx = index(key) orelse return null;
    //     if (self.present & (@as(u64, 1) << idx) == 0) return null;
    //     return self.values[idx];
    // }
    fn get(&self, key: u8) -> Option<u32> {
        let idx = Self::index(key)?;
        if self.present & (1u64 << idx) == 0 {
            return None;
        }
        Some(self.values[idx as usize])
    }

    // fn put(self: *KV, key: u8, value: u32) void {
    //     const idx = index(key) orelse return;
    //     self.values[idx] = value;
    //     self.present |= @as(u64, 1) << idx;
    // }
    fn put(&mut self, key: u8, value: u32) {
        let idx = match Self::index(key) {
            Some(idx) => idx,
            None => return,
        };
        self.values[idx as usize] = value;
        self.present |= 1u64 << idx;
    }
}

/// PORT NOTE: the reference initializes the parser's table with `.kv = .{}`
/// (field defaults: `values` undefined, `present` zero).
impl Default for Kv {
    fn default() -> Kv {
        Kv {
            // values: [52]u32 = undefined,
            values: [0; 52],
            // present: u64 = 0,
            present: 0,
        }
    }
}

/// Command parser parses the Kitty graphics protocol escape sequence.
///
/// PORT NOTE: The reference keeps `alloc: Allocator` to grow `data` (the `kv`
/// table is a plain value, no allocation). Rust's `Vec` carries its own
/// allocator, so the field is elided; `kv`, `max_bytes`, and the parser
/// `state` are preserved verbatim.
pub struct Parser {
    // alloc: Allocator,
    /// This is the list of KV pairs that we're building up.
    kv: Kv,

    /// This is used as a buffer to store the key/value of a KV pair. The value
    /// of a KV pair is at most a 32-bit integer which at most is 10 characters
    /// (4294967295), plus one character for the sign bit on signed ints.
    kv_temp: [u8; 11],
    kv_temp_len: u8,
    kv_current: u8, // Current kv key

    /// This is the list we use to collect the bytes from the data payload.
    /// The Kitty Graphics protocol specification seems to imply that the
    /// payload content of a single command should never exceed 4096 bytes,
    /// but Kitty itself supports larger payloads, so we use an ArrayList
    /// here instead of a fixed buffer so that we can too.
    data: Vec<u8>,

    /// Maximum bytes the data payload can take. This is to prevent
    /// malicious input from causing us to allocate too much memory.
    max_bytes: usize,

    /// Internal state for parsing.
    state: State,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Parsing k/v pairs. The "ignore" variants are in that state
    /// but ignore any data because we know they're invalid.
    ControlKey,
    ControlKeyIgnore,
    ControlValue,
    ControlValueIgnore,

    /// Collecting the data payload blob.
    Data,
}

impl Parser {
    /// Initialize the parser. The allocator given will be used for both
    /// temporary data and long-lived values such as the final image blob.
    pub fn init(max_bytes: usize) -> Parser {
        let result: Parser = Parser {
            // .alloc = alloc,
            data: Vec::new(),
            // .kv = .{},
            kv: Kv::default(),
            kv_temp_len: 0,
            kv_current: 0,
            max_bytes,
            state: State::ControlKey,

            // .kv_temp = undefined,
            kv_temp: [0; 11],
        };
        // if (std.valgrind.runningOnValgrind() > 0) {
        //     // Initialize our undefined fields so Valgrind can catch it.
        //     // https://github.com/ziglang/zig/issues/19148
        //     result.kv_temp = undefined;
        // }
        result
    }

    // PORT NOTE: `deinit` is the `Drop` glue in Zig. Rust drops `kv`/`data`
    // automatically, so there is no explicit `deinit`.
    // pub fn deinit(self: *Parser) void {
    //     self.data.deinit(self.alloc);
    // }

    /// Parse a complete command string.
    pub fn parse_string(data: &[u8]) -> Result<Command, Error> {
        let mut parser = Parser::init(1024 * 1024);
        // defer parser.deinit();
        for c in data.iter() {
            parser.feed(*c)?;
        }
        parser.complete()
    }

    /// Feed a single byte to the parser.
    ///
    /// The first byte to start parsing should be the byte immediately following
    /// the "G" in the APC sequence, i.e. "\x1b_G123" the first byte should
    /// be "1".
    pub fn feed(self: &mut Parser, c: u8) -> Result<(), Error> {
        match self.state {
            State::ControlKey => match c {
                // '=' means the key is complete and we're moving to the value.
                b'=' => {
                    if self.kv_temp_len != 1 {
                        // All control keys are a single character right now so
                        // if we're not a single character just ignore follow-up
                        // data.
                        self.state = State::ControlValueIgnore;
                        self.kv_temp_len = 0;
                    } else {
                        self.kv_current = self.kv_temp[0];
                        self.kv_temp_len = 0;
                        self.state = State::ControlValue;
                    }
                }

                // This can be encountered if we have a sequence with no
                // control data, only payload data (i.e. "\x1b_G;<data>").
                //
                // Kitty treats this as valid so we do as well.
                b';' => self.state = State::Data,

                _ => self.accumulate_value(c, State::ControlKeyIgnore)?,
            },

            // PARITY: faithful port of the reference `switch (c)` with a single
            // matched case; kept as a `match` to mirror the source structure.
            #[allow(clippy::single_match)]
            State::ControlKeyIgnore => match c {
                b'=' => self.state = State::ControlValueIgnore,
                _ => {}
            },

            State::ControlValue => match c {
                b',' => self.finish_value(State::ControlKey)?, // move to next key
                b';' => self.finish_value(State::Data)?,       // move to data
                _ => self.accumulate_value(c, State::ControlValueIgnore)?,
            },

            State::ControlValueIgnore => match c {
                b',' => self.state = State::ControlKeyIgnore,
                b';' => self.state = State::Data,
                _ => {}
            },

            State::Data => {
                if self.data.len() >= self.max_bytes {
                    return Err(Error::OutOfMemory);
                }
                self.data.push(c);
            }
        }

        Ok(())
    }

    /// Feed a slice of bytes to the parser. This is equivalent to
    /// calling feed for each byte in order, but once we're in the data
    /// state the remainder of the slice is appended in bulk, avoiding
    /// per-byte overhead for large payloads.
    // pub fn feedSlice(self: *Parser, bytes: []const u8) !void
    pub fn feed_slice(&mut self, bytes: &[u8]) -> Result<(), Error> {
        // var rem = bytes;
        let mut rem = bytes;
        // while (rem.len > 0) {
        while !rem.is_empty() {
            // if (self.state == .data) {
            if self.state == State::Data {
                // if (self.data.items.len + rem.len > self.max_bytes) return error.OutOfMemory;
                if self.data.len() + rem.len() > self.max_bytes {
                    return Err(Error::OutOfMemory);
                }
                // try self.data.appendSlice(self.alloc, rem);
                self.data.extend_from_slice(rem);
                return Ok(());
            }

            // try self.feed(rem[0]);
            self.feed(rem[0])?;
            // rem = rem[1..];
            rem = &rem[1..];
        }
        Ok(())
    }

    /// Complete the parsing. This must be called after all the
    /// bytes have been fed to the parser.
    ///
    /// The allocator given will be used for the long-lived data
    /// of the final command.
    pub fn complete(self: &mut Parser) -> Result<Command, Error> {
        match self.state {
            // We can't ever end in the control key state and be valid.
            // This means the command looked something like "a=1,b"
            State::ControlKey | State::ControlKeyIgnore => return Err(Error::InvalidFormat),

            // Some commands (i.e. placements) end without extra data so
            // we end in the value state. i.e. "a=1,b=2"
            State::ControlValue => self.finish_value(State::Data)?,
            State::ControlValueIgnore => {}

            // Most commands end in data, i.e. "a=1,b=2;1234"
            State::Data => {}
        }

        // Determine our action, which is always a single character.
        let action: u8 = {
            let value = match self.kv.get(b'a') {
                Some(v) => v,
                None => b't' as u32,
            };
            match u8::try_from(value) {
                Ok(c) => c,
                Err(_) => return Err(Error::InvalidFormat),
            }
        };
        let control: Control = match action {
            b'q' => Control::Query(Transmission::parse(&self.kv)?),
            b't' => Control::Transmit(Transmission::parse(&self.kv)?),
            b'T' => Control::TransmitAndDisplay {
                transmission: Transmission::parse(&self.kv)?,
                display: Display::parse(&self.kv)?,
            },
            b'p' => Control::Display(Display::parse(&self.kv)?),
            b'd' => Control::Delete(Delete::parse(&self.kv)?),
            b'f' => Control::TransmitAnimationFrame(AnimationFrameLoading::parse(&self.kv)?),
            b'a' => Control::ControlAnimation(AnimationControl::parse(&self.kv)),
            b'c' => Control::ComposeAnimation(AnimationFrameComposition::parse(&self.kv)),
            _ => return Err(Error::InvalidFormat),
        };

        // Determine our quiet value. The spec specifies 0 and 1 but Kitty
        // allows anything greater than 1 to suppress.
        let quiet: Quiet = if let Some(v) = self.kv.get(b'q') {
            match v {
                0 => Quiet::No,
                1 => Quiet::Ok,
                _ => Quiet::Failures,
            }
        } else {
            Quiet::No
        };

        Ok(Command {
            control,
            quiet,
            data: self.decode_data()?,
        })
    }

    /// Decodes the payload data from base64 and returns it as a slice.
    /// This function will destroy the contents of self.data, it should
    /// only be used once we are done collecting payload bytes.
    fn decode_data(self: &mut Parser) -> Result<Vec<u8>, Error> {
        if self.data.is_empty() {
            return Ok(Vec::new());
        }

        let max_len = simd::base64::max_len(&self.data);
        assert(max_len <= self.data.len());

        // This is kinda cursed, but we can decode the base64 on top of
        // itself, since it's guaranteed that the encoded size is larger,
        // and any bytes in areas that are written to will have already
        // been used (assuming scalar decoding).
        //
        // PORT NOTE: the reference decodes in place, slicing `self.data.items`
        // as both source and destination. Rust's borrow checker forbids
        // aliasing the same `Vec` as both `&` source and `&mut` destination,
        // so we copy the source bytes first, decode into the original buffer's
        // first `max_len` bytes, then truncate. The observable result (decoded
        // bytes, length, and the InvalidData error on failure) is identical.
        let src = self.data.clone();
        let decoded_len = match simd::base64::decode(&src, &mut self.data[0..max_len]) {
            Ok(decoded) => decoded.len(),
            Err(_err) => {
                // log.warn("failed to decode base64 payload data: {}", .{err});
                return Err(Error::InvalidData);
            }
        };
        assert(decoded_len <= max_len);

        // Remove the extra bytes.
        self.data.truncate(decoded_len);

        Ok(std::mem::take(&mut self.data))
    }

    fn accumulate_value(self: &mut Parser, c: u8, overflow_state: State) -> Result<(), Error> {
        let idx = self.kv_temp_len;
        self.kv_temp_len += 1;
        if self.kv_temp_len as usize > self.kv_temp.len() {
            self.state = overflow_state;
            self.kv_temp_len = 0;
            return Ok(());
        }
        self.kv_temp[idx as usize] = c;
        Ok(())
    }

    fn finish_value(self: &mut Parser, next_state: State) -> Result<(), Error> {
        // We can move states right away, we don't use it.
        self.state = next_state;

        // Check for ASCII chars first
        if self.kv_temp_len == 1 {
            let c = self.kv_temp[0];
            // PARITY: faithful port of the reference `c < '0' or c > '9'`.
            #[allow(clippy::manual_range_contains)]
            if c < b'0' || c > b'9' {
                self.kv.put(self.kv_current, c as u32);
                self.kv_temp_len = 0;
                return Ok(());
            }
        }

        // Handle integer fields, parsing signed fields accordingly. We still
        // store the fields as u32 as they can be bitcast back later during
        // building of the higher-level command tree.
        let slice = &self.kv_temp[0..self.kv_temp_len as usize];
        let v: u32 = match self.kv_current {
            b'z' | b'H' | b'V' => parse_int_i32(slice)? as u32,
            _ => parse_int_u32(slice)?,
        };
        self.kv.put(self.kv_current, v);

        // Clear our temp buffer
        self.kv_temp_len = 0;
        Ok(())
    }
}

/// PORT NOTE: `std.fmt.parseInt(i32, bytes, 10)`. Returns `error.Overflow` on
/// out-of-range and `error.InvalidCharacter` (mapped to `InvalidFormat`) on a
/// non-numeric byte, mirroring the reference's `try`.
fn parse_int_i32(bytes: &[u8]) -> Result<i32, Error> {
    let s = std::str::from_utf8(bytes).map_err(|_| Error::InvalidFormat)?;
    match s.parse::<i32>() {
        Ok(v) => Ok(v),
        Err(e) => match e.kind() {
            std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow => {
                Err(Error::Overflow)
            }
            _ => Err(Error::InvalidFormat),
        },
    }
}

/// PORT NOTE: `std.fmt.parseInt(u32, bytes, 10)`. See `parse_int_i32`.
///
/// PORT NOTE: a leading `-` needs explicit handling because Zig and Rust
/// classify it differently for unsigned types. The reference's
/// `parseIntWithGenericCharacter` peels exactly one sign byte and then runs
/// `parseIntWithSign(u32, u8, rest, 10, .neg)`, which accumulates by
/// *subtracting* each digit from zero; for an unsigned accumulator the very
/// first nonzero digit fails the `math.cast(u32, -digit)` and yields
/// `error.Overflow`, while an all-zero magnitude (`"-0"`, `"-000"`) succeeds
/// with `0`. Rust's `"-1".parse::<u32>()` instead reports `InvalidDigit`
/// (which would map to `InvalidFormat`) and rejects `"-0"` outright, so the
/// negative case is hand-ported byte-for-byte in
/// `parse_int_u32_negative_magnitude` below. The non-negative case still
/// delegates to Rust's parser, which agrees with the reference on it.
fn parse_int_u32(bytes: &[u8]) -> Result<u32, Error> {
    // `if (buf[0] == '-') return parseIntWithSign(..., .neg);`
    if let [b'-', magnitude @ ..] = bytes {
        return parse_int_u32_negative_magnitude(magnitude);
    }

    let s = std::str::from_utf8(bytes).map_err(|_| Error::InvalidFormat)?;
    match s.parse::<u32>() {
        Ok(v) => Ok(v),
        Err(e) => match e.kind() {
            std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow => {
                Err(Error::Overflow)
            }
            _ => Err(Error::InvalidFormat),
        },
    }
}

/// PORT NOTE: byte-for-byte port of `std.fmt.parseIntWithSign(u32, u8, buf,
/// 10, .neg)` (zig 0.16.0 `std/fmt.zig:394`) for the magnitude that follows a
/// `-` sign byte. Only the ordering-relevant parts of the reference survive
/// specialization to `Result = Accumulate = u32` and `base = 10`:
///
///   * an empty magnitude (a bare `"-"`) is `error.InvalidCharacter`;
///   * a leading or trailing `'_'` is `error.InvalidCharacter`, checked
///     *before* any digit is looked at;
///   * then, left to right: `'_'` is skipped, a non-digit is
///     `error.InvalidCharacter` (`charToDigit`), and a digit hits the
///     `accumulate == 0 and sign == .neg` branch, which does
///     `math.cast(u32, -@as(i8, digit)) orelse return error.Overflow` and
///     `continue`.
///
/// Because that cast fails for every nonzero digit, the accumulator can never
/// leave zero, so the multiply/subtract tail of the reference loop is
/// unreachable for an unsigned result: the *first* nonzero digit returns
/// `error.Overflow` before the scan ever inspects the rest of the buffer.
/// That ordering is observable — the reference reports `Overflow` (not an
/// invalid-digit error) for `"-1a"`, `"-1_2"` and `"-1 "`, whereas deferring
/// to `"1a".parse::<u32>()` would report `InvalidDigit`.
///
/// PORT NOTE: the reference also accepts `'_'` digit separators on the
/// *non*-negative path (`parseInt(u32, "1_0", 10) == 10`) where this port
/// still returns `InvalidFormat`; that divergence is out of scope here (it
/// affects `parse_int_i32` identically) and is tracked separately.
fn parse_int_u32_negative_magnitude(buf: &[u8]) -> Result<u32, Error> {
    // `if (buf.len == 0) return error.InvalidCharacter;`
    if buf.is_empty() {
        return Err(Error::InvalidFormat);
    }

    // `if (buf_start[0] == '_' or buf_start[buf_start.len - 1] == '_')
    //     return error.InvalidCharacter;`
    if buf[0] == b'_' || buf[buf.len() - 1] == b'_' {
        return Err(Error::InvalidFormat);
    }

    for &c in buf {
        // `if (c == '_') continue;`
        if c == b'_' {
            continue;
        }

        // `const digit = try charToDigit(c, buf_base);`
        if !c.is_ascii_digit() {
            return Err(Error::InvalidFormat);
        }

        // `accumulate` is still 0 here (see the doc comment), so this is the
        // reference's first-digit `.neg` branch:
        // `accumulate = math.cast(Accumulate, -@as(i8, digit)) orelse
        //     return error.Overflow;`
        if c != b'0' {
            return Err(Error::Overflow);
        }
    }

    Ok(0)
}

/// Represents a possible response to a command.
#[derive(Debug, Clone)]
pub struct Response {
    pub id: u32,
    pub image_number: u32,
    pub placement_id: u32,
    /// The 1-based animation frame number, echoed as "r=" for the
    /// animation frame actions. This is how clients learn the frame
    /// number assigned to a newly created frame.
    pub frame: u32,
    pub message: Vec<u8>,
}

impl Default for Response {
    fn default() -> Self {
        Response {
            id: 0,
            image_number: 0,
            placement_id: 0,
            frame: 0,
            message: b"OK".to_vec(),
        }
    }
}

impl Response {
    /// PORT NOTE: the reference writes to a `std.Io.Writer`. The Rust port
    /// writes to anything implementing `std::io::Write`; the byte sequence
    /// produced is identical. `std.fmt`'s `{}` for integers is decimal, matched
    /// by Rust's `{}` `Display`.
    pub fn encode<W: std::io::Write>(self: &Response, writer: &mut W) -> std::io::Result<()> {
        // We only encode a result if we have either an id or an image number.
        if self.id == 0 && self.image_number == 0 {
            return Ok(());
        }

        // Used to cheaply keep track if we need to add a comma before
        // the next key-value pair.
        let mut prior: bool = false;

        writer.write_all(b"\x1b_G")?;
        if self.id > 0 {
            prior = true;
            write!(writer, "i={}", self.id)?;
        }
        if self.image_number > 0 {
            if prior {
                writer.write_all(b",")?;
            } else {
                prior = true;
            }
            write!(writer, "I={}", self.image_number)?;
        }
        if self.placement_id > 0 {
            if prior {
                writer.write_all(b",")?;
            } else {
                prior = true;
            }
            write!(writer, "p={}", self.placement_id)?;
        }
        if self.frame > 0 {
            if prior {
                writer.write_all(b",")?;
            } else {
                #[allow(unused_assignments)]
                {
                    prior = true;
                }
            }
            write!(writer, "r={}", self.frame)?;
        }
        writer.write_all(b";")?;
        writer.write_all(&self.message)?;
        writer.write_all(b"\x1b\\")?;
        Ok(())
    }

    /// Returns true if this response is not an error.
    pub fn ok(self: &Response) -> bool {
        self.message == b"OK"
    }

    /// Empty response
    pub fn empty(self: &Response) -> bool {
        self.id == 0 && self.image_number == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub control: Control,
    pub quiet: Quiet,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Query,                  // q
    Transmit,               // t
    TransmitAndDisplay,     // T
    Display,                // p
    Delete,                 // d
    TransmitAnimationFrame, // f
    ControlAnimation,       // a
    ComposeAnimation,       // c
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    No,       // 0
    Ok,       // 1
    Failures, // 2
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Query(Transmission),
    Transmit(Transmission),
    TransmitAndDisplay {
        transmission: Transmission,
        display: Display,
    },
    Display(Display),
    Delete(Delete),
    TransmitAnimationFrame(AnimationFrameLoading),
    ControlAnimation(AnimationControl),
    ComposeAnimation(AnimationFrameComposition),
}

/// PORT NOTE: the reference nests this as `Control.Identifiers`; a
/// module-level sibling here (Rust cannot nest types inside enums).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Identifiers {
    pub image_id: u32,
    pub image_number: u32,
    pub placement_id: u32,
}

impl Control {
    /// Returns the image and placement identifiers for any action.
    pub fn identifiers(&self) -> Identifiers {
        match self {
            Control::Query(t) | Control::Transmit(t) => Identifiers {
                image_id: t.image_id,
                image_number: t.image_number,
                placement_id: t.placement_id,
            },
            Control::TransmitAndDisplay { transmission: t, .. } => Identifiers {
                image_id: t.image_id,
                image_number: t.image_number,
                placement_id: t.placement_id,
            },
            Control::Display(d) => Identifiers {
                image_id: d.image_id,
                image_number: d.image_number,
                placement_id: d.placement_id,
            },
            Control::Delete(d) => Identifiers {
                image_id: d.image_id,
                image_number: d.image_number,
                placement_id: d.placement_id,
            },
            Control::TransmitAnimationFrame(f) => Identifiers {
                image_id: f.transmission.image_id,
                image_number: f.transmission.image_number,
                placement_id: f.transmission.placement_id,
            },
            Control::ControlAnimation(a) => Identifiers {
                image_id: a.image_id,
                image_number: a.image_number,
                placement_id: a.placement_id,
            },
            Control::ComposeAnimation(c) => Identifiers {
                image_id: c.image_id,
                image_number: c.image_number,
                placement_id: c.placement_id,
            },
        }
    }
}

impl Command {
    /// Take ownership over the data in this command. If the returned value
    /// has a length of zero, then the data was empty and need not be freed.
    pub fn to_owned_data(self: &mut Command) -> Vec<u8> {
        std::mem::take(&mut self.data)
    }

    /// Returns the transmission data if it has any.
    pub fn transmission(self: &Command) -> Option<Transmission> {
        match &self.control {
            Control::Query(t) => Some(*t),
            Control::Transmit(t) => Some(*t),
            Control::TransmitAndDisplay { transmission, .. } => Some(*transmission),
            Control::TransmitAnimationFrame(f) => Some(f.transmission),
            _ => None,
        }
    }

    /// Returns the display data if it has any.
    pub fn display(self: &Command) -> Option<Display> {
        match &self.control {
            Control::Display(d) => Some(*d),
            Control::TransmitAndDisplay { display, .. } => Some(*display),
            _ => None,
        }
    }

    // PORT NOTE: the reference frees `self.data` if non-empty. Rust drops the
    // owning `Vec<u8>` automatically, so there is no explicit `deinit`.
    // pub fn deinit(self: Command, alloc: Allocator) void {
    //     if (self.data.len > 0) alloc.free(self.data);
    // }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transmission {
    pub format: Format, // f
    /// Set when "f" carried a value we don't recognize. Kitty rejects these
    /// while handling the command so the EINVAL response can carry the image
    /// id, so we defer to `LoadingImage.init`. `format` is meaningless when
    /// this is set.
    pub format_unknown: bool,
    pub medium: Medium,           // t
    pub width: u32,               // s
    pub height: u32,              // v
    pub size: u32,                // S
    pub offset: u32,              // O
    pub image_id: u32,            // i
    pub image_number: u32,        // I
    pub placement_id: u32,        // p
    pub compression: Compression, // o
    pub more_chunks: bool,        // m
    pub usage: Usage,             // N
}

impl Default for Transmission {
    fn default() -> Self {
        Transmission {
            format: Format::Rgba, // f
            format_unknown: false,
            medium: Medium::Direct,         // t
            width: 0,                       // s
            height: 0,                      // v
            size: 0,                        // S
            offset: 0,                      // O
            image_id: 0,                    // i
            image_number: 0,                // I
            placement_id: 0,                // p
            compression: Compression::None, // o
            more_chunks: false,             // m
            usage: Usage::DEFAULT,          // N
        }
    }
}

/// Usage hints allow for optimising resource consumption strategies.
///
/// https://sw.kovidgoyal.net/kitty/graphics-protocol/#usage-hints
//
// PORT NOTE (oracle `a65e11cc9`): the reference is
// `packed struct(u32) { transient: bool = false, _padding: u31 = 0 }`, and it
// IS bit-cast (`result.usage = @bitCast(v)` over the raw `N` value), so the
// layout is observable — unlike `graphics_image.Limits`, which is only ever
// read field-wise. Rust has no `u31`, so the padding is dropped and the
// bit-cast is spelled out in [`Usage::from_bits`]/[`Usage::to_bits`]: Zig lays
// a `packed struct` out least-significant-field-first, so `transient` is bit 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    /// Image with this usage hint is assumed to be used for only a short
    /// time, so may be evicted before other images if memory pressure is
    /// encountered.
    pub transient: bool,
}

impl Usage {
    // pub const default: Usage = .{};
    pub const DEFAULT: Usage = Usage { transient: false };

    /// `@bitCast(v)` where `v` is the `u32` value of the `N` key.
    pub fn from_bits(v: u32) -> Usage {
        Usage {
            transient: (v & 0b1) != 0,
        }
    }

    /// The inverse of [`Usage::from_bits`]. Not used by the reference (which
    /// bit-casts in one direction only), but required to keep the two halves
    /// of the projection provably consistent — see the round-trip KAT.
    pub fn to_bits(self) -> u32 {
        u32::from(self.transient)
    }
}

/// C: RusttyKittyImageFormat (R2) — checked by the A5 enum-discriminant arm.
/// PORT NOTE: `lib.Enum(lib.target, &.{ "rgb", "rgba", "png", "gray_alpha",
/// "gray" })` synthesizes a C-ABI-aware enum. Like the `osc` port, we model the
/// nominal enum with named variants in declaration order and expose the
/// `EnumType` descriptor (the only observable the reference reflects over) via
/// `descriptor()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Rgb,  // 24
    Rgba, // 32
    Png,  // 100
    // The following are not supported directly via the protocol
    // but they are formats that a png may decode to that we
    // support.
    GrayAlpha,
    Gray,
}

impl Format {
    pub fn descriptor() -> EnumType {
        rustty_lib::Enum(
            Target::Zig,
            &[
                Some("rgb"),  // 24
                Some("rgba"), // 32
                Some("png"),  // 100
                // The following are not supported directly via the protocol
                // but they are formats that a png may decode to that we
                // support.
                Some("gray_alpha"),
                Some("gray"),
            ],
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Medium {
    Direct,        // d
    File,          // f
    TemporaryFile, // t
    SharedMemory,  // s
}

impl Medium {
    pub fn descriptor() -> EnumType {
        rustty_lib::Enum(
            Target::Zig,
            &[
                Some("direct"),         // d
                Some("file"),           // f
                Some("temporary_file"), // t
                Some("shared_memory"),  // s
            ],
        )
    }
}

/// C: RusttyKittyImageCompression (R2) — checked by the A5 enum-discriminant arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    None,
    ZlibDeflate, // z
}

impl Compression {
    pub fn descriptor() -> EnumType {
        rustty_lib::Enum(
            Target::Zig,
            &[
                Some("none"),
                Some("zlib_deflate"), // z
            ],
        )
    }
}

impl Transmission {
    pub fn format_bpp(format: Format) -> u8 {
        match format {
            Format::Gray => 1,
            Format::GrayAlpha => 2,
            Format::Rgb => 3,
            Format::Rgba => 4,
            Format::Png => unreachable!(), // Must be validated before
        }
    }

    fn parse(kv: &Kv) -> Result<Transmission, Error> {
        let mut result: Transmission = Transmission::default();
        if let Some(v) = kv.get(b'f') {
            result.format = match v {
                24 => Format::Rgb,
                0 | 32 => Format::Rgba,
                100 => Format::Png,
                _ => {
                    // Defer returning an error until the image is processed.
                    result.format_unknown = true;
                    Format::Rgba
                }
            };
        }

        if let Some(v) = kv.get(b't') {
            let c = match u8::try_from(v) {
                Ok(c) => c,
                Err(_) => return Err(Error::InvalidFormat),
            };
            result.medium = match c {
                b'd' => Medium::Direct,
                b'f' => Medium::File,
                b't' => Medium::TemporaryFile,
                b's' => Medium::SharedMemory,
                _ => return Err(Error::InvalidFormat),
            };
        }

        if let Some(v) = kv.get(b's') {
            result.width = v;
        }

        if let Some(v) = kv.get(b'v') {
            result.height = v;
        }

        if let Some(v) = kv.get(b'S') {
            result.size = v;
        }

        if let Some(v) = kv.get(b'O') {
            result.offset = v;
        }

        if let Some(v) = kv.get(b'i') {
            result.image_id = v;
        }

        if let Some(v) = kv.get(b'I') {
            result.image_number = v;
        }

        if let Some(v) = kv.get(b'p') {
            result.placement_id = v;
        }

        if let Some(v) = kv.get(b'o') {
            let c = match u8::try_from(v) {
                Ok(c) => c,
                Err(_) => return Err(Error::InvalidFormat),
            };
            result.compression = match c {
                b'z' => Compression::ZlibDeflate,
                _ => return Err(Error::InvalidFormat),
            };
        }

        // If the transmission medium is a local-only medium, ignore the "m"
        // key. The Kitty graphics protocol specification does not explicitly
        // call out this behavior (although the "m" key is only mentioned in
        // connection with remote clients) but that's how it's implemented in
        // Kitty and at least one client (mpv) relies on this behavior when
        // using the shared memory transmission medium.
        //
        // https://sw.kovidgoyal.net/kitty/graphics-protocol/#the-transmission-medium
        // https://github.com/kovidgoyal/kitty/blob/ccc3bee9af794f332b4e9adcd714a649f639c397/kitty/graphics.c#L547-L592
        if result.medium == Medium::Direct {
            if let Some(v) = kv.get(b'm') {
                result.more_chunks = v > 0;
            }
        }

        // if (kv.get('N')) |v| {
        //     result.usage = @bitCast(v);
        // }
        if let Some(v) = kv.get(b'N') {
            result.usage = Usage::from_bits(v);
        }

        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Display {
    pub image_id: u32,                   // i
    pub image_number: u32,               // I
    pub placement_id: u32,               // p
    pub x: u32,                          // x
    pub y: u32,                          // y
    pub width: u32,                      // w
    pub height: u32,                     // h
    pub x_offset: u32,                   // X
    pub y_offset: u32,                   // Y
    pub columns: u32,                    // c
    pub rows: u32,                       // r
    pub cursor_movement: CursorMovement, // C
    pub virtual_placement: bool,         // U
    pub parent_id: u32,                  // P
    pub parent_placement_id: u32,        // Q
    pub horizontal_offset: i32,          // H
    pub vertical_offset: i32,            // V
    pub z: i32,                          // z
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorMovement {
    #[default]
    After, // 0
    None, // 1
}

impl Display {
    fn parse(kv: &Kv) -> Result<Display, Error> {
        let mut result: Display = Display::default();

        if let Some(v) = kv.get(b'i') {
            result.image_id = v;
        }

        if let Some(v) = kv.get(b'I') {
            result.image_number = v;
        }

        if let Some(v) = kv.get(b'p') {
            result.placement_id = v;
        }

        if let Some(v) = kv.get(b'x') {
            result.x = v;
        }

        if let Some(v) = kv.get(b'y') {
            result.y = v;
        }

        if let Some(v) = kv.get(b'w') {
            result.width = v;
        }

        if let Some(v) = kv.get(b'h') {
            result.height = v;
        }

        if let Some(v) = kv.get(b'X') {
            result.x_offset = v;
        }

        if let Some(v) = kv.get(b'Y') {
            result.y_offset = v;
        }

        if let Some(v) = kv.get(b'c') {
            result.columns = v;
        }

        if let Some(v) = kv.get(b'r') {
            result.rows = v;
        }

        if let Some(v) = kv.get(b'C') {
            // Kitty only tests this against 1 so any other value moves the cursor.
            result.cursor_movement = if v == 1 {
                CursorMovement::None
            } else {
                CursorMovement::After
            };
        }

        if let Some(v) = kv.get(b'U') {
            // Kitty stores this in a bool, so any non-zero value is virtual.
            result.virtual_placement = v != 0;
        }

        if let Some(v) = kv.get(b'z') {
            // We can bitcast here because of how we parse it earlier.
            result.z = v as i32;
        }

        if let Some(v) = kv.get(b'P') {
            result.parent_id = v;
        }

        if let Some(v) = kv.get(b'Q') {
            result.parent_placement_id = v;
        }

        if let Some(v) = kv.get(b'H') {
            // We can bitcast here because of how we parse it earlier.
            result.horizontal_offset = v as i32;
        }

        if let Some(v) = kv.get(b'V') {
            // We can bitcast here because of how we parse it earlier.
            result.vertical_offset = v as i32;
        }

        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnimationFrameLoading {
    /// Frame data is transmitted exactly like image data, so an a=f
    /// command carries a full set of transmission keys: medium (t),
    /// format (f), the frame rectangle size (s/v), chunking (m), etc.
    /// The image id/number here identify the (existing) image the
    /// frame belongs to.
    pub transmission: Transmission,

    /// The left/top edge (pixels) of the rectangle within the frame
    /// that the transmitted data updates.
    pub x: u32, // x
    pub y: u32, // y

    /// The 1-based frame number whose pixels serve as the base canvas
    /// for a newly created frame. Zero means no base frame: the
    /// canvas is filled with `background` instead.
    pub create_frame: u32, // c

    /// The 1-based frame number of an existing frame to edit. Zero
    /// (or any value past the next frame number) creates a new frame.
    pub edit_frame: u32, // r

    /// The gap in milliseconds before the next frame is shown. For
    /// new frames, zero selects the default gap and a negative value
    /// creates a gapless (never displayed) frame. For edits, zero
    /// leaves the gap unchanged.
    pub gap_ms: i32, // z

    pub composition_mode: CompositionMode, // X
    pub background: Background,            // Y
}

/// The canvas background color as a 32-bit RGBA value where R is
/// the most significant byte (0xRRGGBBAA), e.g. Y=4278190335
/// (0xff0000ff) is opaque red. Bit-cast from the little-endian
/// u32 value, so the field order is reversed from the byte order.
//
// PORT NOTE: the reference is `packed struct(u32) { a, b, g, r }` — Zig lays a
// packed struct out least-significant-field-first, so `a` is the low byte and
// `r` the high byte of the little-endian u32. The bit-cast is spelled out in
// [`Background::from_int`] and pinned by the
// `background_bit_cast_is_0x_rrggbbaa` golden-byte KAT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Background {
    pub a: u8,
    pub b: u8,
    pub g: u8,
    pub r: u8,
}

impl Background {
    /// PORT NOTE: `@bitCast(u32)` of a `packed struct(u32)` whose fields are
    /// `a, b, g, r` in declaration order. `a` occupies the least significant
    /// byte and `r` the most significant, which this reproduces (so the u32
    /// value reads 0xRRGGBBAA, matching the doc comment above).
    pub fn from_int(v: u32) -> Background {
        Background {
            a: (v & 0xff) as u8,
            b: ((v >> 8) & 0xff) as u8,
            g: ((v >> 16) & 0xff) as u8,
            r: ((v >> 24) & 0xff) as u8,
        }
    }

    /// The inverse of [`Background::from_int`]. Not used by the reference
    /// (which bit-casts in one direction only), but required to keep the two
    /// halves of the projection provably consistent — see the round-trip KAT
    /// (the `Usage::to_bits` precedent).
    pub fn to_int(self) -> u32 {
        (self.a as u32) | ((self.b as u32) << 8) | ((self.g as u32) << 16) | ((self.r as u32) << 24)
    }
}

impl AnimationFrameLoading {
    fn parse(kv: &Kv) -> Result<AnimationFrameLoading, Error> {
        let mut result: AnimationFrameLoading = AnimationFrameLoading {
            transmission: Transmission::parse(kv)?,
            ..Default::default()
        };

        if let Some(v) = kv.get(b'x') {
            result.x = v;
        }

        if let Some(v) = kv.get(b'y') {
            result.y = v;
        }

        if let Some(v) = kv.get(b'c') {
            result.create_frame = v;
        }

        if let Some(v) = kv.get(b'r') {
            result.edit_frame = v;
        }

        if let Some(v) = kv.get(b'z') {
            // 'z' is one of the keys Parser.finishValue parses as an
            // i32, so the u32 in the KV table is the bit pattern of
            // that i32 and this reconstructs it exactly.
            //
            // PORT NOTE (R7): `@bitCast` — this is the intentional
            // truncation-free bit-pattern reinterpretation, NOT a checked
            // narrowing; pinned by the `gap_ms_is_a_bit_pattern_cast` KAT.
            result.gap_ms = i32::from_ne_bytes(v.to_ne_bytes());
        }

        // Tested only against 1 like Kitty.
        if let Some(v) = kv.get(b'X') {
            result.composition_mode = if v == 1 {
                CompositionMode::Overwrite
            } else {
                CompositionMode::AlphaBlend
            };
        }

        if let Some(v) = kv.get(b'Y') {
            result.background = Background::from_int(v);
        }

        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnimationFrameComposition {
    pub image_id: u32,     // i
    pub image_number: u32, // I
    pub placement_id: u32, // p

    /// The 1-based frame number being edited (the destination).
    pub dest_frame: u32, // c

    /// The 1-based frame number whose pixels are composed onto the
    /// destination (the source).
    pub source_frame: u32, // r

    /// The left/top edge (pixels) of the destination rectangle.
    pub x: u32, // x
    pub y: u32, // y

    /// The size of the rectangle to compose. Zero selects the full
    /// image width/height.
    pub width: u32, // w
    pub height: u32, // h

    /// The left/top edge (pixels) of the source rectangle.
    pub left_edge: u32, // X
    pub top_edge: u32, // Y

    pub composition_mode: CompositionMode, // C
}

impl AnimationFrameComposition {
    // Cannot fail: every key is stored as parsed, with no range
    // validation (Kitty range-checks these while handling the
    // command; see the comment above the execution-deferral tests).
    fn parse(kv: &Kv) -> AnimationFrameComposition {
        let mut result: AnimationFrameComposition = AnimationFrameComposition::default();

        if let Some(v) = kv.get(b'i') {
            result.image_id = v;
        }

        if let Some(v) = kv.get(b'I') {
            result.image_number = v;
        }

        if let Some(v) = kv.get(b'p') {
            result.placement_id = v;
        }

        if let Some(v) = kv.get(b'c') {
            result.dest_frame = v;
        }

        if let Some(v) = kv.get(b'r') {
            result.source_frame = v;
        }

        if let Some(v) = kv.get(b'x') {
            result.x = v;
        }

        if let Some(v) = kv.get(b'y') {
            result.y = v;
        }

        if let Some(v) = kv.get(b'w') {
            result.width = v;
        }

        if let Some(v) = kv.get(b'h') {
            result.height = v;
        }

        if let Some(v) = kv.get(b'X') {
            result.left_edge = v;
        }

        if let Some(v) = kv.get(b'Y') {
            result.top_edge = v;
        }

        if let Some(v) = kv.get(b'C') {
            // Kitty tests this against zero for compose so any non-zero overwrites
            result.composition_mode = if v == 0 {
                CompositionMode::AlphaBlend
            } else {
                CompositionMode::Overwrite
            };
        }

        result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnimationControl {
    pub image_id: u32,           // i
    pub image_number: u32,       // I
    pub placement_id: u32,       // p
    pub action: AnimationAction, // s

    /// The 1-based frame number whose gap is set from gap_ms.
    pub frame: u32, // r

    /// The new gap for `frame` in milliseconds. Zero is ignored and a
    /// negative value makes the frame gapless.
    pub gap_ms: i32, // z

    /// The 1-based frame number to make current (client-driven
    /// animation). Zero is ignored.
    pub current_frame: u32, // c

    /// The loop count: zero is ignored, 1 loops forever, and any
    /// larger value plays that many minus one loops.
    pub loops: u32, // v
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationAction {
    #[default]
    Invalid, // 0
    Stop,    // 1
    RunWait, // 2
    Run,     // 3
}

impl AnimationControl {
    // Cannot fail: every key is stored as parsed. Out-of-range values
    // (frame numbers, states) are silently ignored during execution,
    // matching Kitty.
    fn parse(kv: &Kv) -> AnimationControl {
        let mut result: AnimationControl = AnimationControl::default();

        if let Some(v) = kv.get(b'i') {
            result.image_id = v;
        }

        if let Some(v) = kv.get(b'I') {
            result.image_number = v;
        }

        if let Some(v) = kv.get(b'p') {
            result.placement_id = v;
        }

        if let Some(v) = kv.get(b's') {
            // Kitty ignores values it doesn't know, leaving the animation
            // state untouched, which is what `.invalid` means here.
            result.action = match v {
                1 => AnimationAction::Stop,
                2 => AnimationAction::RunWait,
                3 => AnimationAction::Run,
                _ => AnimationAction::Invalid,
            };
        }

        if let Some(v) = kv.get(b'r') {
            result.frame = v;
        }

        if let Some(v) = kv.get(b'z') {
            // 'z' is one of the keys Parser.finishValue parses as an
            // i32, so the u32 in the KV table is the bit pattern of
            // that i32 and this reconstructs it exactly.
            //
            // PORT NOTE (R7): `@bitCast` — see `AnimationFrameLoading::parse`.
            result.gap_ms = i32::from_ne_bytes(v.to_ne_bytes());
        }

        if let Some(v) = kv.get(b'c') {
            result.current_frame = v;
        }

        if let Some(v) = kv.get(b'v') {
            result.loops = v;
        }

        result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Delete {
    pub image_id: u32,      // i
    pub image_number: u32,  // I
    pub placement_id: u32,  // p
    pub action: DeleteAction,
}

impl Delete {
    fn parse(kv: &Kv) -> Result<Delete, Error> {
        Ok(Delete {
            image_id: kv.get(b'i').unwrap_or(0),
            image_number: kv.get(b'I').unwrap_or(0),
            placement_id: kv.get(b'p').unwrap_or(0),
            action: DeleteAction::parse(kv)?,
        })
    }
}

/// PORT NOTE: the reference nests this as `Delete.Action`; Rust cannot nest a
/// type inside a struct, so it is a sibling named `DeleteAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteAction {
    // a/A
    All(bool),

    // i/I
    Id {
        delete: bool,      // uppercase
        image_id: u32,     // i
        placement_id: u32, // p
    },

    // n/N
    Newest {
        delete: bool,      // uppercase
        image_number: u32, // I
        placement_id: u32, // p
    },

    // c/C,
    IntersectCursor(bool),

    // f/F
    AnimationFrames(DeleteAnimationFrames),

    // p/P
    IntersectCell {
        delete: bool, // uppercase
        x: u32,       // x
        y: u32,       // y
    },

    // q/Q
    IntersectCellZ {
        delete: bool, // uppercase
        x: u32,       // x
        y: u32,       // y
        z: i32,       // z
    },

    // r/R
    Range {
        delete: bool, // uppercase
        first: u32,   // x
        last: u32,    // y
    },

    // x/X
    Column {
        delete: bool, // uppercase
        x: u32,       // x
    },

    // y/Y
    Row {
        delete: bool, // uppercase
        y: u32,       // y
    },

    // z/Z
    Z {
        delete: bool, // uppercase
        z: i32,       // z
    },
}

/// PORT NOTE: the reference nests this as `Delete.Action.AnimationFrames`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeleteAnimationFrames {
    pub delete: bool,      // uppercase
    pub image_id: u32,     // i
    pub image_number: u32, // I
    pub frame: u32,        // r
}

impl DeleteAction {
    fn parse(kv: &Kv) -> Result<DeleteAction, Error> {
        let what: u8 = {
            let value = match kv.get(b'd') {
                Some(v) => v,
                None => b'a' as u32,
            };
            match u8::try_from(value) {
                Ok(c) => c,
                Err(_) => return Err(Error::InvalidFormat),
            }
        };

        Ok(match what {
            b'a' | b'A' => DeleteAction::All(what == b'A'),

            b'i' | b'I' => {
                let mut result: DeleteAction = DeleteAction::Id {
                    delete: what == b'I',
                    image_id: 0,
                    placement_id: 0,
                };
                if let Some(v) = kv.get(b'i') {
                    if let DeleteAction::Id { image_id, .. } = &mut result {
                        *image_id = v;
                    }
                }
                if let Some(v) = kv.get(b'p') {
                    if let DeleteAction::Id { placement_id, .. } = &mut result {
                        *placement_id = v;
                    }
                }

                result
            }

            b'n' | b'N' => {
                let mut result: DeleteAction = DeleteAction::Newest {
                    delete: what == b'N',
                    image_number: 0,
                    placement_id: 0,
                };
                if let Some(v) = kv.get(b'I') {
                    if let DeleteAction::Newest { image_number, .. } = &mut result {
                        *image_number = v;
                    }
                }
                if let Some(v) = kv.get(b'p') {
                    if let DeleteAction::Newest { placement_id, .. } = &mut result {
                        *placement_id = v;
                    }
                }

                result
            }

            b'c' | b'C' => DeleteAction::IntersectCursor(what == b'C'),

            b'f' | b'F' => {
                let mut result: DeleteAnimationFrames = DeleteAnimationFrames {
                    delete: what == b'F',
                    ..Default::default()
                };
                if let Some(v) = kv.get(b'i') {
                    result.image_id = v;
                }
                if let Some(v) = kv.get(b'I') {
                    result.image_number = v;
                }
                if let Some(v) = kv.get(b'r') {
                    result.frame = v;
                }

                DeleteAction::AnimationFrames(result)
            }

            b'p' | b'P' => {
                let mut result: DeleteAction = DeleteAction::IntersectCell {
                    delete: what == b'P',
                    x: 0,
                    y: 0,
                };
                if let Some(v) = kv.get(b'x') {
                    if let DeleteAction::IntersectCell { x, .. } = &mut result {
                        *x = v;
                    }
                }
                if let Some(v) = kv.get(b'y') {
                    if let DeleteAction::IntersectCell { y, .. } = &mut result {
                        *y = v;
                    }
                }

                result
            }

            b'q' | b'Q' => {
                let mut result: DeleteAction = DeleteAction::IntersectCellZ {
                    delete: what == b'Q',
                    x: 0,
                    y: 0,
                    z: 0,
                };
                if let Some(v) = kv.get(b'x') {
                    if let DeleteAction::IntersectCellZ { x, .. } = &mut result {
                        *x = v;
                    }
                }
                if let Some(v) = kv.get(b'y') {
                    if let DeleteAction::IntersectCellZ { y, .. } = &mut result {
                        *y = v;
                    }
                }
                if let Some(v) = kv.get(b'z') {
                    // We can bitcast here because of how we parse it earlier.
                    if let DeleteAction::IntersectCellZ { z, .. } = &mut result {
                        *z = v as i32;
                    }
                }

                result
            }

            b'r' | b'R' => {
                // Both bounds default to zero when omitted and no
                // validation is performed; an inverted or zero range
                // simply matches no images.
                let mut result: DeleteAction = DeleteAction::Range {
                    delete: what == b'R',
                    first: 0,
                    last: 0,
                };
                if let Some(v) = kv.get(b'x') {
                    if let DeleteAction::Range { first, .. } = &mut result {
                        *first = v;
                    }
                }
                if let Some(v) = kv.get(b'y') {
                    if let DeleteAction::Range { last, .. } = &mut result {
                        *last = v;
                    }
                }

                result
            }

            b'x' | b'X' => {
                let mut result: DeleteAction = DeleteAction::Column {
                    delete: what == b'X',
                    x: 0,
                };
                if let Some(v) = kv.get(b'x') {
                    if let DeleteAction::Column { x, .. } = &mut result {
                        *x = v;
                    }
                }

                result
            }

            b'y' | b'Y' => {
                let mut result: DeleteAction = DeleteAction::Row {
                    delete: what == b'Y',
                    y: 0,
                };
                if let Some(v) = kv.get(b'y') {
                    if let DeleteAction::Row { y, .. } = &mut result {
                        *y = v;
                    }
                }

                result
            }

            b'z' | b'Z' => {
                let mut result: DeleteAction = DeleteAction::Z {
                    delete: what == b'Z',
                    z: 0,
                };
                if let Some(v) = kv.get(b'z') {
                    // We can bitcast here because of how we parse it earlier.
                    if let DeleteAction::Z { z, .. } = &mut result {
                        *z = v as i32;
                    }
                }

                result
            }

            _ => return Err(Error::InvalidFormat),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompositionMode {
    #[default]
    AlphaBlend, // 0
    Overwrite, // 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transmission_command() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"f=24,s=10,v=20";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));
        let v = match command.control {
            Control::Transmit(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(Format::Rgb, v.format);
        assert_eq!(10u32, v.width);
        assert_eq!(20u32, v.height);
        assert!(!v.usage.transient);
    }

    // test "transmission command with transient hint"
    #[test]
    fn transmission_command_with_transient_hint() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"f=24,s=10,v=20,N=1";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));
        let v = match command.control {
            Control::Transmit(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(Format::Rgb, v.format);
        assert_eq!(10u32, v.width);
        assert_eq!(20u32, v.height);
        assert!(v.usage.transient);
    }

    /// NEGATIVE CONTROL (rustty-only, `P3-W11 T2`): the reference bit-casts a
    /// raw `u32` into `Usage`, so the BIT POSITION of `transient` is part of
    /// the wire format. Upstream's single `N=1` case cannot distinguish bit 0
    /// from "any non-zero value sets it". These cases can.
    #[test]
    fn transmission_usage_transient_is_bit_zero() {
        fn usage_of(input: &[u8]) -> Usage {
            let mut p = Parser::init(1024 * 1024);
            for c in input.iter() {
                p.feed(*c).unwrap();
            }
            match p.complete().unwrap().control {
                Control::Transmit(v) => v.usage,
                _ => unreachable!(),
            }
        }

        // Bit 0 clear: not transient, even though the value is non-zero.
        assert!(!usage_of(b"f=24,s=10,v=20,N=2").transient);
        assert!(!usage_of(b"f=24,s=10,v=20,N=4").transient);
        assert!(!usage_of(b"f=24,s=10,v=20,N=0").transient);
        // Bit 0 set.
        assert!(usage_of(b"f=24,s=10,v=20,N=1").transient);
        assert!(usage_of(b"f=24,s=10,v=20,N=3").transient);

        // The `from_bits`/`to_bits` projection round-trips.
        assert_eq!(0u32, Usage::from_bits(0).to_bits());
        assert_eq!(1u32, Usage::from_bits(1).to_bits());
        assert_eq!(1u32, Usage::from_bits(0xffff_ffff).to_bits());
        assert_eq!(Usage::DEFAULT, Usage::from_bits(0));
    }

    // test "feedSlice matches per-byte feed"
    #[test]
    fn feed_slice_matches_per_byte_feed() {
        let input = b"f=24,s=10,v=20;aGVsbG8gd29ybGQ=";

        let mut p1 = Parser::init(1024 * 1024);
        for c in input.iter() {
            p1.feed(*c).unwrap();
        }
        let c1 = p1.complete().unwrap();

        let mut p2 = Parser::init(1024 * 1024);
        p2.feed_slice(input).unwrap();
        let c2 = p2.complete().unwrap();

        assert!(matches!(c1.control, Control::Transmit(_)));
        assert!(matches!(c2.control, Control::Transmit(_)));
        assert_eq!(c1.data, c2.data);
    }

    // test "feedSlice across slice boundaries"
    #[test]
    fn feed_slice_across_slice_boundaries() {
        let mut p = Parser::init(1024 * 1024);

        p.feed_slice(b"f=24,s=10").unwrap();
        p.feed_slice(b",v=20;aGVsbG8g").unwrap();
        p.feed_slice(b"d29ybGQ=").unwrap();
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));

        // The payload is base64-decoded on completion.
        assert_eq!(b"hello world".as_slice(), command.data.as_slice());
    }

    // test "feedSlice respects max_bytes"
    #[test]
    fn feed_slice_respects_max_bytes() {
        let mut p = Parser::init(4);

        p.feed_slice(b"f=24;ab").unwrap();
        assert_eq!(Err(Error::OutOfMemory), p.feed_slice(b"cde"));
    }

    #[test]
    fn transmission_ignores_m_if_medium_is_not_direct() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=t,t=t,m=1";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));
        let v = match command.control {
            Control::Transmit(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(Medium::TemporaryFile, v.medium);
        assert!(!v.more_chunks);
    }

    #[test]
    fn transmission_respects_m_if_medium_is_direct() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=t,t=d,m=1";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));
        let v = match command.control {
            Control::Transmit(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(Medium::Direct, v.medium);
        assert!(v.more_chunks);
    }

    #[test]
    fn query_command() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"i=31,s=1,v=1,a=q,t=d,f=24;QUFBQQ";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Query(_)));
        let v = match command.control {
            Control::Query(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(Medium::Direct, v.medium);
        assert_eq!(1u32, v.width);
        assert_eq!(1u32, v.height);
        assert_eq!(31u32, v.image_id);
        assert_eq!(b"AAAA".as_slice(), command.data.as_slice());
    }

    #[test]
    fn display_command() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=p,U=1,i=31,c=80,r=120";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Display(_)));
        let v = match command.control {
            Control::Display(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(80u32, v.columns);
        assert_eq!(120u32, v.rows);
        assert_eq!(31u32, v.image_id);
    }

    #[test]
    fn delete_command() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=d,d=p,x=3,y=4";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Delete(_)));
        let v = match command.control {
            Control::Delete(v) => v.action,
            _ => unreachable!(),
        };
        assert!(matches!(v, DeleteAction::IntersectCell { .. }));
        let dv = match v {
            DeleteAction::IntersectCell { delete, x, y } => (delete, x, y),
            _ => unreachable!(),
        };
        assert!(!dv.0);
        assert_eq!(3u32, dv.1);
        assert_eq!(4u32, dv.2);
    }

    #[test]
    fn no_control_data() {
        let mut p = Parser::init(1024 * 1024);

        let input = b";QUFBQQ";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));
        assert_eq!(b"AAAA".as_slice(), command.data.as_slice());
    }

    #[test]
    fn ignore_unknown_keys_long() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"f=24,s=10,v=20,hello=world";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));
        let v = match command.control {
            Control::Transmit(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(Format::Rgb, v.format);
        assert_eq!(10u32, v.width);
        assert_eq!(20u32, v.height);
    }

    // test "ignore unknown keys (non-letter)"
    #[test]
    fn ignore_unknown_keys_non_letter() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"f=24,s=10,v=20,!=1";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));
        let v = match command.control {
            Control::Transmit(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(Format::Rgb, v.format);
        assert_eq!(10u32, v.width);
        assert_eq!(20u32, v.height);
    }

    #[test]
    fn ignore_very_long_values() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"f=24,s=10,v=2000000000000000000000000000000000000000";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));
        let v = match command.control {
            Control::Transmit(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(Format::Rgb, v.format);
        assert_eq!(10u32, v.width);
        assert_eq!(0u32, v.height);
    }

    #[test]
    fn ensure_very_large_negative_values_dont_get_skipped() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=p,i=1,z=-2000000000";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Display(_)));
        let v = match command.control {
            Control::Display(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(1, v.image_id);
        assert_eq!(-2000000000, v.z);
    }

    #[test]
    fn ensure_proper_overflow_error_for_u32() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=p,i=10000000000";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        assert_eq!(Err(Error::Overflow), p.complete());
    }

    /// PORT NOTE: the unsigned control fields go through
    /// `std.fmt.parseInt(u32, ...)`, whose `.neg` path subtracts each digit
    /// from zero. This table is the *measured* output of the reference — every
    /// row was produced by running `std.fmt.parseInt(u32, s, 10)` on zig
    /// 0.16.0 (`std/fmt.zig:394` `parseIntWithSign`).
    ///
    /// The rows that pin the error *ordering* are the ones with a nonzero
    /// leading digit followed by a non-digit byte (`"-1a"`, `"-1_2"`, `"-1 "`):
    /// the reference returns `Overflow` from the first digit, before the scan
    /// ever reaches the offending byte, so an implementation that classified
    /// the whole magnitude first would report an invalid-digit error instead.
    #[test]
    fn parse_int_u32_matches_reference_parse_int_with_sign() {
        // (input, expected)
        let cases: &[(&[u8], Result<u32, Error>)] = &[
            // Nonzero magnitude: the first digit fails `math.cast(u32, -digit)`.
            (b"-1", Err(Error::Overflow)),
            (b"-01", Err(Error::Overflow)),
            (b"-0000000001", Err(Error::Overflow)),
            (b"-2147483648", Err(Error::Overflow)),
            (b"-4294967295", Err(Error::Overflow)),
            // Magnitude that itself overflows u32: still the first digit.
            (b"-9999999999", Err(Error::Overflow)),
            // ORDERING: the first nonzero digit wins over the later garbage.
            (b"-1a", Err(Error::Overflow)),
            (b"-1_2", Err(Error::Overflow)),
            (b"-1 ", Err(Error::Overflow)),
            // All-zero magnitudes survive the negation and yield 0.
            (b"-0", Ok(0)),
            (b"-000", Ok(0)),
            (b"-0_0", Ok(0)),
            (b"-00_00", Ok(0)),
            // `charToDigit` fires first when no nonzero digit precedes.
            (b"-0a", Err(Error::InvalidFormat)),
            (b"-a1", Err(Error::InvalidFormat)),
            (b"- 1", Err(Error::InvalidFormat)),
            // Exactly one sign byte is consumed, so a second sign is a digit
            // error, not an overflow.
            (b"--1", Err(Error::InvalidFormat)),
            (b"-+1", Err(Error::InvalidFormat)),
            // Empty magnitude (bare `-`).
            (b"-", Err(Error::InvalidFormat)),
            // Leading/trailing `_` is rejected before the digit loop runs,
            // so it beats the first-digit overflow.
            (b"-_1", Err(Error::InvalidFormat)),
            (b"-1_", Err(Error::InvalidFormat)),
            (b"-0_", Err(Error::InvalidFormat)),
            // Non-negative path, unchanged.
            (b"10", Ok(10)),
            (b"+10", Ok(10)),
            (b"4294967296", Err(Error::Overflow)),
            (b"", Err(Error::InvalidFormat)),
        ];

        for (input, want) in cases {
            assert_eq!(
                *want,
                parse_int_u32(input),
                "input: {:?}",
                std::str::from_utf8(input)
            );
        }
    }

    /// The same negative-magnitude classification, observed through the real
    /// parser (values are capped at `kv_temp`'s 11 bytes).
    #[test]
    fn ensure_proper_overflow_error_for_negative_u32() {
        for input in [
            b"a=p,i=-1".as_slice(),
            b"a=p,p=-1".as_slice(),
            // 11 bytes, the exact `kv_temp` capacity (longer values are
            // dropped by `accumulate_value` before they ever reach the parse).
            b"a=p,i=-4294967295".as_slice(),
            // Magnitude that itself overflows u32: same reported error.
            b"a=p,i=-9999999999".as_slice(),
            // ORDERING: `Overflow` from the leading `1`, not `InvalidFormat`
            // from the trailing non-digit.
            b"a=p,i=-1a".as_slice(),
            b"a=p,i=-1_2".as_slice(),
            b"a=p,i=-1 ".as_slice(),
        ] {
            let mut p = Parser::init(1024 * 1024);
            for c in input.iter() {
                p.feed(*c).unwrap();
            }
            assert_eq!(Err(Error::Overflow), p.complete(), "input: {input:?}");
        }
    }

    /// PORT NOTE: a negative zero is accepted by the reference's unsigned
    /// parser (`parseInt(u8, "-0", 10) == 0`), so it must not be rejected.
    #[test]
    fn negative_zero_is_accepted_for_u32() {
        for input in [
            b"a=p,i=-0,q=2".as_slice(),
            b"a=p,i=-000,q=2".as_slice(),
            b"a=p,i=-0_0,q=2".as_slice(),
        ] {
            let mut p = Parser::init(1024 * 1024);
            for c in input.iter() {
                p.feed(*c).unwrap();
            }
            let command = p.complete().unwrap();
            let v = match command.control {
                Control::Display(v) => v,
                _ => unreachable!(),
            };
            assert_eq!(0, v.image_id, "input: {input:?}");
        }
    }

    /// PORT NOTE: the reference consumes exactly one sign byte, so a second
    /// sign is a digit error (`error.InvalidCharacter`), not an overflow; and
    /// a leading/trailing `_` is rejected before any digit is inspected.
    #[test]
    fn invalid_format_cases_for_negative_u32() {
        for input in [
            b"a=p,i=--1".as_slice(),
            b"a=p,i=-+1".as_slice(),
            b"a=p,i=-a1".as_slice(),
            b"a=p,i=-0a".as_slice(),
            b"a=p,i=- 1".as_slice(),
            // NOTE: a bare `i=-` never reaches the integer parse at all: like
            // the reference, `finish_value` short-circuits a single non-digit
            // byte and stores it verbatim. `parse_int_u32(b"-")` is covered by
            // the table test above instead.
            b"a=p,i=-_1".as_slice(),
            b"a=p,i=-1_".as_slice(),
        ] {
            let mut p = Parser::init(1024 * 1024);
            for c in input.iter() {
                p.feed(*c).unwrap();
            }
            assert_eq!(Err(Error::InvalidFormat), p.complete(), "input: {input:?}");
        }
    }

    #[test]
    fn ensure_proper_overflow_error_for_i32() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=p,i=1,z=-9999999999";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        assert_eq!(Err(Error::Overflow), p.complete());
    }

    #[test]
    fn all_i32_values() {
        {
            // 'z' (usually z-axis values)
            let mut p = Parser::init(1024 * 1024);
            let input = b"a=p,i=1,z=-1";
            for c in input.iter() {
                p.feed(*c).unwrap();
            }
            let command = p.complete().unwrap();

            assert!(matches!(command.control, Control::Display(_)));
            let v = match command.control {
                Control::Display(v) => v,
                _ => unreachable!(),
            };
            assert_eq!(1, v.image_id);
            assert_eq!(-1, v.z);
        }

        {
            // 'H' (relative placement, horizontal offset)
            let mut p = Parser::init(1024 * 1024);
            let input = b"a=p,i=1,H=-1";
            for c in input.iter() {
                p.feed(*c).unwrap();
            }
            let command = p.complete().unwrap();

            assert!(matches!(command.control, Control::Display(_)));
            let v = match command.control {
                Control::Display(v) => v,
                _ => unreachable!(),
            };
            assert_eq!(1, v.image_id);
            assert_eq!(-1, v.horizontal_offset);
        }

        {
            // 'V' (relative placement, vertical offset)
            let mut p = Parser::init(1024 * 1024);
            let input = b"a=p,i=1,V=-1";
            for c in input.iter() {
                p.feed(*c).unwrap();
            }
            let command = p.complete().unwrap();

            assert!(matches!(command.control, Control::Display(_)));
            let v = match command.control {
                Control::Display(v) => v,
                _ => unreachable!(),
            };
            assert_eq!(1, v.image_id);
            assert_eq!(-1, v.vertical_offset);
        }
    }

    #[test]
    fn response_encode_nothing_without_id_or_image_number() {
        let mut buf: Vec<u8> = Vec::new();

        let r: Response = Response::default();
        r.encode(&mut buf).unwrap();
        assert_eq!(b"".as_slice(), buf.as_slice());
    }

    #[test]
    fn response_encode_with_only_image_id() {
        let mut buf: Vec<u8> = Vec::new();

        let r: Response = Response {
            id: 4,
            ..Default::default()
        };
        r.encode(&mut buf).unwrap();
        assert_eq!(b"\x1b_Gi=4;OK\x1b\\".as_slice(), buf.as_slice());
    }

    #[test]
    fn response_encode_with_only_image_number() {
        let mut buf: Vec<u8> = Vec::new();

        let r: Response = Response {
            image_number: 4,
            ..Default::default()
        };
        r.encode(&mut buf).unwrap();
        assert_eq!(b"\x1b_GI=4;OK\x1b\\".as_slice(), buf.as_slice());
    }

    #[test]
    fn response_encode_with_image_id_and_number() {
        let mut buf: Vec<u8> = Vec::new();

        let r: Response = Response {
            id: 12,
            image_number: 4,
            ..Default::default()
        };
        r.encode(&mut buf).unwrap();
        assert_eq!(b"\x1b_Gi=12,I=4;OK\x1b\\".as_slice(), buf.as_slice());
    }

    #[test]
    fn delete_range_command_1() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=d,d=r,x=3,y=4";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Delete(_)));
        let v = match command.control {
            Control::Delete(v) => v.action,
            _ => unreachable!(),
        };
        assert!(matches!(v, DeleteAction::Range { .. }));
        let range = match v {
            DeleteAction::Range {
                delete,
                first,
                last,
            } => (delete, first, last),
            _ => unreachable!(),
        };
        assert!(!range.0);
        assert_eq!(3u32, range.1);
        assert_eq!(4u32, range.2);
    }

    #[test]
    fn delete_range_command_2() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=d,d=R,x=5,y=11";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Delete(_)));
        let v = match command.control {
            Control::Delete(v) => v.action,
            _ => unreachable!(),
        };
        assert!(matches!(v, DeleteAction::Range { .. }));
        let range = match v {
            DeleteAction::Range {
                delete,
                first,
                last,
            } => (delete, first, last),
            _ => unreachable!(),
        };
        assert!(range.0);
        assert_eq!(5u32, range.1);
        assert_eq!(11u32, range.2);
    }

    #[test]
    fn delete_range_command_3() {
        // An inverted range is accepted; it simply matches no images. This
        // matches kitty, which does no validation and filters with
        // `x <= image_id <= y`.
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=d,d=R,x=5,y=4";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Delete(_)));
        let v = match command.control {
            Control::Delete(v) => v.action,
            _ => unreachable!(),
        };
        assert!(matches!(v, DeleteAction::Range { .. }));
        let range = match v {
            DeleteAction::Range {
                delete,
                first,
                last,
            } => (delete, first, last),
            _ => unreachable!(),
        };
        assert!(range.0);
        assert_eq!(5u32, range.1);
        assert_eq!(4u32, range.2);
    }

    #[test]
    fn delete_range_command_4() {
        // An omitted upper bound defaults to zero, which matches no images
        // since image IDs are always non-zero.
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=d,d=R,x=5";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Delete(_)));
        let v = match command.control {
            Control::Delete(v) => v.action,
            _ => unreachable!(),
        };
        assert!(matches!(v, DeleteAction::Range { .. }));
        let range = match v {
            DeleteAction::Range {
                delete,
                first,
                last,
            } => (delete, first, last),
            _ => unreachable!(),
        };
        assert!(range.0);
        assert_eq!(5u32, range.1);
        assert_eq!(0u32, range.2);
    }

    #[test]
    fn delete_range_command_5() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=d,d=R,y=5";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Delete(_)));
        let v = match command.control {
            Control::Delete(v) => v.action,
            _ => unreachable!(),
        };
        assert!(matches!(v, DeleteAction::Range { .. }));
        let range = match v {
            DeleteAction::Range {
                delete,
                first,
                last,
            } => (delete, first, last),
            _ => unreachable!(),
        };
        assert!(range.0);
        assert_eq!(0u32, range.1);
        assert_eq!(5u32, range.2);
    }

    #[test]
    fn delete_range_command_6() {
        // Both bounds omitted defaults to the empty range [0, 0].
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=d,d=r";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        let command = p.complete().unwrap();

        assert!(matches!(command.control, Control::Delete(_)));
        let v = match command.control {
            Control::Delete(v) => v.action,
            _ => unreachable!(),
        };
        assert!(matches!(v, DeleteAction::Range { .. }));
        let range = match v {
            DeleteAction::Range {
                delete,
                first,
                last,
            } => (delete, first, last),
            _ => unreachable!(),
        };
        assert!(!range.0);
        assert_eq!(0u32, range.1);
        assert_eq!(0u32, range.2);
    }

    // Kitty range-checks non-flag keys while handling the command, not while
    // parsing, so a bad value never drops the command (which would make a
    // response impossible even when the client gave an id). The flag keys
    // "a", "d", "t" and "o" are the exception.

    #[test]
    fn unknown_format_value_is_deferred_to_execution() {
        let command = Parser::parse_string(b"a=t,f=42,i=31,s=1,v=1").unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));
        let v = match command.control {
            Control::Transmit(v) => v,
            _ => unreachable!(),
        };
        assert!(v.format_unknown);
        assert_eq!(31u32, v.image_id);
    }

    #[test]
    fn zero_format_value_is_rgba() {
        let command = Parser::parse_string(b"a=t,f=0,i=31,s=1,v=1").unwrap();

        assert!(matches!(command.control, Control::Transmit(_)));
        let v = match command.control {
            Control::Transmit(v) => v,
            _ => unreachable!(),
        };
        assert!(!v.format_unknown);
        assert_eq!(Format::Rgba, v.format);
    }

    #[test]
    fn known_format_values_are_not_unknown() {
        for (input, want) in [
            (b"f=24".as_slice(), Format::Rgb),
            (b"f=32".as_slice(), Format::Rgba),
            (b"f=100".as_slice(), Format::Png),
        ] {
            let command = Parser::parse_string(input).unwrap();

            let v = match command.control {
                Control::Transmit(v) => v,
                _ => unreachable!(),
            };
            assert!(!v.format_unknown);
            assert_eq!(want, v.format);
        }
    }

    #[test]
    fn quiet_value_above_two_suppresses_all_responses() {
        let command = Parser::parse_string(b"a=t,q=3,i=31,s=1,v=1").unwrap();

        assert_eq!(Quiet::Failures, command.quiet);
    }

    #[test]
    fn cursor_movement_value_above_one_moves_the_cursor() {
        let command = Parser::parse_string(b"a=p,i=31,C=2").unwrap();

        assert!(matches!(command.control, Control::Display(_)));
        let v = match command.control {
            Control::Display(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(CursorMovement::After, v.cursor_movement);
    }

    #[test]
    fn virtual_placement_value_above_one_is_virtual() {
        let command = Parser::parse_string(b"a=p,i=31,U=2").unwrap();

        assert!(matches!(command.control, Control::Display(_)));
        let v = match command.control {
            Control::Display(v) => v,
            _ => unreachable!(),
        };
        assert!(v.virtual_placement);
    }

    #[test]
    fn animation_frame_composition_value_above_one_alpha_blends() {
        let command = Parser::parse_string(b"a=f,i=31,X=2").unwrap();

        assert!(matches!(command.control, Control::TransmitAnimationFrame(_)));
        let v = match command.control {
            Control::TransmitAnimationFrame(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(CompositionMode::AlphaBlend, v.composition_mode);
    }

    #[test]
    fn animation_compose_value_above_one_overwrites() {
        let command = Parser::parse_string(b"a=c,i=31,C=2").unwrap();

        assert!(matches!(command.control, Control::ComposeAnimation(_)));
        let v = match command.control {
            Control::ComposeAnimation(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(CompositionMode::Overwrite, v.composition_mode);
    }

    #[test]
    fn animation_control_action_above_three_is_ignored() {
        let command = Parser::parse_string(b"a=a,i=31,s=4").unwrap();

        assert!(matches!(command.control, Control::ControlAnimation(_)));
        let v = match command.control {
            Control::ControlAnimation(v) => v,
            _ => unreachable!(),
        };
        assert_eq!(AnimationAction::Invalid, v.action);
    }

    #[test]
    fn unknown_flag_key_values_still_fail_parsing() {
        // Kitty rejects these while parsing too. See gen/apc_parsers.py.
        for input in [
            b"a=z,i=31".as_slice(),     // action
            b"a=d,d=w,i=31".as_slice(), // delete action
            b"a=t,t=q,i=31".as_slice(), // transmission medium
            b"a=t,o=q,i=31".as_slice(), // compression
        ] {
            assert_eq!(
                Err(Error::InvalidFormat),
                Parser::parse_string(input),
                "input: {input:?}"
            );
        }
    }

    // Golden-byte KAT (rustty addition, not a reference test; `4b915e3bc`
    // amends existing parse arms without adding `^test` blocks, so this pins
    // the amended semantics directly): the a=f/a=a `z` key is parsed by
    // `finishValue` as an i32 whose BIT PATTERN is stored in the u32 KV table;
    // `gap_ms` reconstructs it with a bit cast, never a checked narrowing.
    #[test]
    fn gap_ms_is_a_bit_pattern_cast() {
        // z=-5 → parse_int_i32 = -5i32 → stored as 0xFFFF_FFFB.
        let cmd = Parser::parse_string(b"a=f,z=-5").unwrap();
        let f = match cmd.control {
            Control::TransmitAnimationFrame(f) => f,
            _ => unreachable!(),
        };
        assert_eq!(-5i32, f.gap_ms);
        assert_eq!(0xFFFF_FFFBu32, f.gap_ms as u32);

        let cmd = Parser::parse_string(b"a=a,i=1,z=-1").unwrap();
        let a = match cmd.control {
            Control::ControlAnimation(a) => a,
            _ => unreachable!(),
        };
        assert_eq!(-1i32, a.gap_ms);
        assert_eq!(u32::MAX, a.gap_ms as u32);
    }

    // Golden-byte KAT (rustty addition, see above): the `Y` background value
    // is a `packed struct(u32) { a, b, g, r }` bit-cast — `a` in the least
    // significant byte, `r` in the most significant, i.e. Y=0xRRGGBBAA.
    // Y=4278190335 (0xff0000ff) is opaque red per the reference doc comment.
    #[test]
    fn background_bit_cast_is_0x_rrggbbaa() {
        let bg = Background::from_int(4278190335);
        assert_eq!(0xff, bg.r);
        assert_eq!(0x00, bg.g);
        assert_eq!(0x00, bg.b);
        assert_eq!(0xff, bg.a);
        // Round-trip: the projection halves stay consistent.
        assert_eq!(4278190335, bg.to_int());

        // A distinct-byte pattern pins each lane: 0x11223344 → r=0x11 g=0x22
        // b=0x33 a=0x44.
        let bg = Background::from_int(0x11223344);
        assert_eq!(
            (0x11, 0x22, 0x33, 0x44),
            (bg.r, bg.g, bg.b, bg.a),
        );
        assert_eq!(0x11223344, bg.to_int());

        // Parsed through the real KV path.
        let cmd = Parser::parse_string(b"a=f,Y=4278190335").unwrap();
        let f = match cmd.control {
            Control::TransmitAnimationFrame(f) => f,
            _ => unreachable!(),
        };
        assert_eq!(
            Background {
                r: 255,
                g: 0,
                b: 0,
                a: 255
            },
            f.background
        );
    }
}
