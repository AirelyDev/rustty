// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const simd = @import("../../simd/main.zig");
// const lib = @import("../lib.zig");
use rustty_lib::r#enum::EnumType;
use rustty_lib::target::Target;
use rustty_simd as simd;
use rustty_util::quirks::inlineAssert as assert;
use std::collections::HashMap;

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
/// keys are always single characters and the values are either single
/// characters or 32-bit unsigned integers.
///
/// For the value of this: if the value is a single printable ASCII character
/// it is the ASCII code. Otherwise, it is parsed as a 32-bit unsigned integer.
///
/// PORT NOTE: `std.AutoHashMapUnmanaged(u8, u32)`. In the reference the map is
/// arena-allocated and freed wholesale with the arena; here the map owns its
/// storage and is dropped with the `Parser`.
type Kv = HashMap<u8, u32>;

/// Command parser parses the Kitty graphics protocol escape sequence.
///
/// PORT NOTE: The reference stores temporary parser state in an
/// `ArenaAllocator` (the `kv` map plus parse-int scratch) and the long-lived
/// payload in `data`. Rust ownership makes the arena unnecessary: `kv` is an
/// owning `HashMap` and `data` is an owning `Vec<u8>`, both dropped with the
/// `Parser`. `max_bytes` and the parser `state` are preserved verbatim.
pub struct Parser {
    // /// The memory used by the parser is stored in an arena because it is
    // /// all freed at the end of the command.
    // arena: ArenaAllocator,
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
        // var arena = ArenaAllocator.init(alloc);
        // errdefer arena.deinit();
        let result: Parser = Parser {
            // .arena = arena,
            data: Vec::new(),
            kv: HashMap::new(),
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
    //     // We don't free the hash map because its in the arena
    //     self.data.deinit(self.arena.child_allocator);
    //     self.arena.deinit();
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
            let value = match self.kv.get(&b'a') {
                Some(v) => *v,
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
            b'a' => Control::ControlAnimation(AnimationControl::parse(&self.kv)?),
            b'c' => Control::ComposeAnimation(AnimationFrameComposition::parse(&self.kv)?),
            _ => return Err(Error::InvalidFormat),
        };

        // Determine our quiet value
        let quiet: Quiet = if let Some(v) = self.kv.get(&b'q') {
            match *v {
                0 => Quiet::No,
                1 => Quiet::Ok,
                2 => Quiet::Failures,
                _ => return Err(Error::InvalidFormat),
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
        // const alloc = self.arena.allocator();

        // We can move states right away, we don't use it.
        self.state = next_state;

        // Check for ASCII chars first
        if self.kv_temp_len == 1 {
            let c = self.kv_temp[0];
            // PARITY: faithful port of the reference `c < '0' or c > '9'`.
            #[allow(clippy::manual_range_contains)]
            if c < b'0' || c > b'9' {
                self.kv.insert(self.kv_current, c as u32);
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
        self.kv.insert(self.kv_current, v);

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
fn parse_int_u32(bytes: &[u8]) -> Result<u32, Error> {
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

/// Represents a possible response to a command.
#[derive(Debug, Clone)]
pub struct Response {
    pub id: u32,
    pub image_number: u32,
    pub placement_id: u32,
    pub message: Vec<u8>,
}

impl Default for Response {
    fn default() -> Self {
        Response {
            id: 0,
            image_number: 0,
            placement_id: 0,
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
                #[allow(unused_assignments)]
                {
                    prior = true;
                }
            }
            write!(writer, "p={}", self.placement_id)?;
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
    pub format: Format,           // f
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
}

impl Default for Transmission {
    fn default() -> Self {
        Transmission {
            format: Format::Rgba,           // f
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
        }
    }
}

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
        if let Some(v) = kv.get(&b'f') {
            result.format = match *v {
                24 => Format::Rgb,
                32 => Format::Rgba,
                100 => Format::Png,
                _ => return Err(Error::InvalidFormat),
            };
        }

        if let Some(v) = kv.get(&b't') {
            let c = match u8::try_from(*v) {
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

        if let Some(v) = kv.get(&b's') {
            result.width = *v;
        }

        if let Some(v) = kv.get(&b'v') {
            result.height = *v;
        }

        if let Some(v) = kv.get(&b'S') {
            result.size = *v;
        }

        if let Some(v) = kv.get(&b'O') {
            result.offset = *v;
        }

        if let Some(v) = kv.get(&b'i') {
            result.image_id = *v;
        }

        if let Some(v) = kv.get(&b'I') {
            result.image_number = *v;
        }

        if let Some(v) = kv.get(&b'p') {
            result.placement_id = *v;
        }

        if let Some(v) = kv.get(&b'o') {
            let c = match u8::try_from(*v) {
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
            if let Some(v) = kv.get(&b'm') {
                result.more_chunks = *v > 0;
            }
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

        if let Some(v) = kv.get(&b'i') {
            result.image_id = *v;
        }

        if let Some(v) = kv.get(&b'I') {
            result.image_number = *v;
        }

        if let Some(v) = kv.get(&b'p') {
            result.placement_id = *v;
        }

        if let Some(v) = kv.get(&b'x') {
            result.x = *v;
        }

        if let Some(v) = kv.get(&b'y') {
            result.y = *v;
        }

        if let Some(v) = kv.get(&b'w') {
            result.width = *v;
        }

        if let Some(v) = kv.get(&b'h') {
            result.height = *v;
        }

        if let Some(v) = kv.get(&b'X') {
            result.x_offset = *v;
        }

        if let Some(v) = kv.get(&b'Y') {
            result.y_offset = *v;
        }

        if let Some(v) = kv.get(&b'c') {
            result.columns = *v;
        }

        if let Some(v) = kv.get(&b'r') {
            result.rows = *v;
        }

        if let Some(v) = kv.get(&b'C') {
            result.cursor_movement = match *v {
                0 => CursorMovement::After,
                1 => CursorMovement::None,
                _ => return Err(Error::InvalidFormat),
            };
        }

        if let Some(v) = kv.get(&b'U') {
            result.virtual_placement = match *v {
                0 => false,
                1 => true,
                _ => return Err(Error::InvalidFormat),
            };
        }

        if let Some(v) = kv.get(&b'z') {
            // We can bitcast here because of how we parse it earlier.
            result.z = *v as i32;
        }

        if let Some(v) = kv.get(&b'P') {
            result.parent_id = *v;
        }

        if let Some(v) = kv.get(&b'Q') {
            result.parent_placement_id = *v;
        }

        if let Some(v) = kv.get(&b'H') {
            // We can bitcast here because of how we parse it earlier.
            result.horizontal_offset = *v as i32;
        }

        if let Some(v) = kv.get(&b'V') {
            // We can bitcast here because of how we parse it earlier.
            result.vertical_offset = *v as i32;
        }

        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnimationFrameLoading {
    pub x: u32,                            // x
    pub y: u32,                            // y
    pub create_frame: u32,                 // c
    pub edit_frame: u32,                   // r
    pub gap_ms: u32,                       // z
    pub composition_mode: CompositionMode, // X
    pub background: Background,            // Y
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Background {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Background {
    /// PORT NOTE: `@bitCast(u32)` of a `packed struct(u32)` whose fields are
    /// `r, g, b, a` in declaration order. `r` occupies the least significant
    /// byte and `a` the most significant, which this reproduces.
    pub fn from_int(v: u32) -> Background {
        Background {
            r: (v & 0xff) as u8,
            g: ((v >> 8) & 0xff) as u8,
            b: ((v >> 16) & 0xff) as u8,
            a: ((v >> 24) & 0xff) as u8,
        }
    }
}

impl AnimationFrameLoading {
    fn parse(kv: &Kv) -> Result<AnimationFrameLoading, Error> {
        let mut result: AnimationFrameLoading = AnimationFrameLoading::default();

        if let Some(v) = kv.get(&b'x') {
            result.x = *v;
        }

        if let Some(v) = kv.get(&b'y') {
            result.y = *v;
        }

        if let Some(v) = kv.get(&b'c') {
            result.create_frame = *v;
        }

        if let Some(v) = kv.get(&b'r') {
            result.edit_frame = *v;
        }

        if let Some(v) = kv.get(&b'z') {
            result.gap_ms = *v;
        }

        if let Some(v) = kv.get(&b'X') {
            result.composition_mode = match *v {
                0 => CompositionMode::AlphaBlend,
                1 => CompositionMode::Overwrite,
                _ => return Err(Error::InvalidFormat),
            };
        }

        if let Some(v) = kv.get(&b'Y') {
            result.background = Background::from_int(*v);
        }

        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnimationFrameComposition {
    pub frame: u32,                        // c
    pub edit_frame: u32,                   // r
    pub x: u32,                            // x
    pub y: u32,                            // y
    pub width: u32,                        // w
    pub height: u32,                       // h
    pub left_edge: u32,                    // X
    pub top_edge: u32,                     // Y
    pub composition_mode: CompositionMode, // C
}

impl AnimationFrameComposition {
    fn parse(kv: &Kv) -> Result<AnimationFrameComposition, Error> {
        let mut result: AnimationFrameComposition = AnimationFrameComposition::default();

        if let Some(v) = kv.get(&b'c') {
            result.frame = *v;
        }

        if let Some(v) = kv.get(&b'r') {
            result.edit_frame = *v;
        }

        if let Some(v) = kv.get(&b'x') {
            result.x = *v;
        }

        if let Some(v) = kv.get(&b'y') {
            result.y = *v;
        }

        if let Some(v) = kv.get(&b'w') {
            result.width = *v;
        }

        if let Some(v) = kv.get(&b'h') {
            result.height = *v;
        }

        if let Some(v) = kv.get(&b'X') {
            result.left_edge = *v;
        }

        if let Some(v) = kv.get(&b'Y') {
            result.top_edge = *v;
        }

        if let Some(v) = kv.get(&b'C') {
            result.composition_mode = match *v {
                0 => CompositionMode::AlphaBlend,
                1 => CompositionMode::Overwrite,
                _ => return Err(Error::InvalidFormat),
            };
        }

        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AnimationControl {
    pub action: AnimationAction, // s
    pub frame: u32,              // r
    pub gap_ms: u32,             // z
    pub current_frame: u32,      // c
    pub loops: u32,              // v
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
    fn parse(kv: &Kv) -> Result<AnimationControl, Error> {
        let mut result: AnimationControl = AnimationControl::default();

        if let Some(v) = kv.get(&b's') {
            result.action = match *v {
                0 => AnimationAction::Invalid,
                1 => AnimationAction::Stop,
                2 => AnimationAction::RunWait,
                3 => AnimationAction::Run,
                _ => return Err(Error::InvalidFormat),
            };
        }

        if let Some(v) = kv.get(&b'r') {
            result.frame = *v;
        }

        if let Some(v) = kv.get(&b'z') {
            result.gap_ms = *v;
        }

        if let Some(v) = kv.get(&b'c') {
            result.current_frame = *v;
        }

        if let Some(v) = kv.get(&b'v') {
            result.loops = *v;
        }

        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delete {
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
    AnimationFrames(bool),

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

impl Delete {
    fn parse(kv: &Kv) -> Result<Delete, Error> {
        let what: u8 = {
            let value = match kv.get(&b'd') {
                Some(v) => *v,
                None => b'a' as u32,
            };
            match u8::try_from(value) {
                Ok(c) => c,
                Err(_) => return Err(Error::InvalidFormat),
            }
        };

        Ok(match what {
            b'a' | b'A' => Delete::All(what == b'A'),

            b'i' | b'I' => {
                let mut result: Delete = Delete::Id {
                    delete: what == b'I',
                    image_id: 0,
                    placement_id: 0,
                };
                if let Some(v) = kv.get(&b'i') {
                    if let Delete::Id { image_id, .. } = &mut result {
                        *image_id = *v;
                    }
                }
                if let Some(v) = kv.get(&b'p') {
                    if let Delete::Id { placement_id, .. } = &mut result {
                        *placement_id = *v;
                    }
                }

                result
            }

            b'n' | b'N' => {
                let mut result: Delete = Delete::Newest {
                    delete: what == b'N',
                    image_number: 0,
                    placement_id: 0,
                };
                if let Some(v) = kv.get(&b'I') {
                    if let Delete::Newest { image_number, .. } = &mut result {
                        *image_number = *v;
                    }
                }
                if let Some(v) = kv.get(&b'p') {
                    if let Delete::Newest { placement_id, .. } = &mut result {
                        *placement_id = *v;
                    }
                }

                result
            }

            b'c' | b'C' => Delete::IntersectCursor(what == b'C'),

            b'f' | b'F' => Delete::AnimationFrames(what == b'F'),

            b'p' | b'P' => {
                let mut result: Delete = Delete::IntersectCell {
                    delete: what == b'P',
                    x: 0,
                    y: 0,
                };
                if let Some(v) = kv.get(&b'x') {
                    if let Delete::IntersectCell { x, .. } = &mut result {
                        *x = *v;
                    }
                }
                if let Some(v) = kv.get(&b'y') {
                    if let Delete::IntersectCell { y, .. } = &mut result {
                        *y = *v;
                    }
                }

                result
            }

            b'q' | b'Q' => {
                let mut result: Delete = Delete::IntersectCellZ {
                    delete: what == b'Q',
                    x: 0,
                    y: 0,
                    z: 0,
                };
                if let Some(v) = kv.get(&b'x') {
                    if let Delete::IntersectCellZ { x, .. } = &mut result {
                        *x = *v;
                    }
                }
                if let Some(v) = kv.get(&b'y') {
                    if let Delete::IntersectCellZ { y, .. } = &mut result {
                        *y = *v;
                    }
                }
                if let Some(v) = kv.get(&b'z') {
                    // We can bitcast here because of how we parse it earlier.
                    if let Delete::IntersectCellZ { z, .. } = &mut result {
                        *z = *v as i32;
                    }
                }

                result
            }

            b'r' | b'R' => {
                let x = match kv.get(&b'x') {
                    Some(v) => *v,
                    None => return Err(Error::InvalidFormat),
                };
                let y = match kv.get(&b'y') {
                    Some(v) => *v,
                    None => return Err(Error::InvalidFormat),
                };
                if x > y {
                    return Err(Error::InvalidFormat);
                }
                Delete::Range {
                    delete: what == b'R',
                    first: x,
                    last: y,
                }
            }

            b'x' | b'X' => {
                let mut result: Delete = Delete::Column {
                    delete: what == b'X',
                    x: 0,
                };
                if let Some(v) = kv.get(&b'x') {
                    if let Delete::Column { x, .. } = &mut result {
                        *x = *v;
                    }
                }

                result
            }

            b'y' | b'Y' => {
                let mut result: Delete = Delete::Row {
                    delete: what == b'Y',
                    y: 0,
                };
                if let Some(v) = kv.get(&b'y') {
                    if let Delete::Row { y, .. } = &mut result {
                        *y = *v;
                    }
                }

                result
            }

            b'z' | b'Z' => {
                let mut result: Delete = Delete::Z {
                    delete: what == b'Z',
                    z: 0,
                };
                if let Some(v) = kv.get(&b'z') {
                    // We can bitcast here because of how we parse it earlier.
                    if let Delete::Z { z, .. } = &mut result {
                        *z = *v as i32;
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
            Control::Delete(v) => v,
            _ => unreachable!(),
        };
        assert!(matches!(v, Delete::IntersectCell { .. }));
        let dv = match v {
            Delete::IntersectCell { delete, x, y } => (delete, x, y),
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
            Control::Delete(v) => v,
            _ => unreachable!(),
        };
        assert!(matches!(v, Delete::Range { .. }));
        let range = match v {
            Delete::Range {
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
            Control::Delete(v) => v,
            _ => unreachable!(),
        };
        assert!(matches!(v, Delete::Range { .. }));
        let range = match v {
            Delete::Range {
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
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=d,d=R,x=5,y=4";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        assert_eq!(Err(Error::InvalidFormat), p.complete());
    }

    #[test]
    fn delete_range_command_4() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=d,d=R,x=5";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        assert_eq!(Err(Error::InvalidFormat), p.complete());
    }

    #[test]
    fn delete_range_command_5() {
        let mut p = Parser::init(1024 * 1024);

        let input = b"a=d,d=R,y=5";
        for c in input.iter() {
            p.feed(*c).unwrap();
        }
        assert_eq!(Err(Error::InvalidFormat), p.complete());
    }
}
