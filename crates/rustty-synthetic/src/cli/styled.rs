//! Generates styled terminal content: lines of printable text
//! interspersed with SGR sequences, optional multi-byte UTF-8
//! codepoints, combining marks (multi-codepoint graphemes), and
//! OSC 8 hyperlinks.
//!
//! This exists primarily to build corpora for benchmarking code that
//! consumes terminal *contents* (e.g. the terminal formatter used for
//! clipboard copy and dumps), where we want workloads with controlled
//! amounts of styling and Unicode rather than raw random bytes.
//!
//! Examples:
//!
//!   # Plain ASCII lines (equivalent to `ascii` with lines)
//!   rustty-gen styled --seed=42
//!
//!   # Heavily styled ASCII
//!   rustty-gen styled --seed=42 --style-rate=0.8
//!
//!   # Unicode-heavy, no styling
//!   rustty-gen styled --seed=42 --weight-two=1 --weight-three=1 \
//!     --weight-four=0.5 --grapheme-rate=0.1
//!
//!   # Mixed: styles, Unicode, and hyperlinks
//!   rustty-gen styled --seed=42 --style-rate=0.3 --weight-two=0.5 \
//!     --weight-three=0.25 --weight-four=0.1 --grapheme-rate=0.05 \
//!     --osc8-rate=0.1
//!
//! PORT NOTE: `synthetic/cli/Styled.zig` (b11d60818).

use crate::io::{WriteFailed, Writer, WriterExt as _};
use crate::rand::{DefaultPrng, Rng, U21};

/// PORT NOTE: the `error.InvalidValue` returned by `create` on a bad option.
#[derive(Debug)]
pub struct InvalidValue;

/// PORT NOTE: `pub const Options = struct { ... }`.
pub struct Options {
    /// Seed to use for deterministic generation. If unset, a time-based
    /// seed is used by the generic synthetic CLI.
    pub seed: Option<u64>,

    /// Emit lines whose printable length (in characters, not columns)
    /// is uniformly distributed in `[line-min, line-max]`. Each line is
    /// terminated by CR LF.
    // @"line-min": usize = 40,
    // @"line-max": usize = 80,
    pub line_min: usize,
    pub line_max: usize,

    /// Probability that a style change (SGR sequence) is emitted at
    /// each run boundary. Zero (default) produces unstyled output.
    // @"style-rate": f64 = 0.0,
    pub style_rate: f64,

    /// The length in characters of a "run": a sequence of characters
    /// that share styling. Style/hyperlink changes only happen on run
    /// boundaries.
    // @"run-min": usize = 4,
    // @"run-max": usize = 16,
    pub run_min: usize,
    pub run_max: usize,

    /// Relative weights for choosing the UTF-8 encoding length of each
    /// generated character. Unlike the raw `utf8` generator, characters
    /// are drawn from curated printable ranges so the output never
    /// contains control characters:
    ///
    ///   one:   printable ASCII (with occasional spaces)
    ///   two:   Latin-1 Supplement / Latin Extended (narrow)
    ///   three: Hiragana/Katakana and CJK ideographs (wide)
    ///   four:  emoji (wide)
    // @"weight-one": f64 = 1.0,
    // @"weight-two": f64 = 0.0,
    // @"weight-three": f64 = 0.0,
    // @"weight-four": f64 = 0.0,
    pub weight_one: f64,
    pub weight_two: f64,
    pub weight_three: f64,
    pub weight_four: f64,

    /// Probability that a generated character is followed by a
    /// combining diacritical mark, producing a multi-codepoint
    /// grapheme cluster.
    // @"grapheme-rate": f64 = 0.0,
    pub grapheme_rate: f64,

    /// Probability that an OSC 8 hyperlink is toggled (opened or
    /// closed) at each run boundary.
    // @"osc8-rate": f64 = 0.0,
    pub osc8_rate: f64,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            seed: None,
            line_min: 40,
            line_max: 80,
            style_rate: 0.0,
            run_min: 4,
            run_max: 16,
            weight_one: 1.0,
            weight_two: 0.0,
            weight_three: 0.0,
            weight_four: 0.0,
            grapheme_rate: 0.0,
            osc8_rate: 0.0,
        }
    }
}

impl rustty_args::args::Target for Options {
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        match key {
            // seed: ?u64 = null
            b"seed" => {
                self.seed = Some(rustty_args::args::parse_field_int::<u64>(value)?);
                Ok(())
            }
            // @"line-min": usize = 40
            b"line-min" => {
                self.line_min = rustty_args::args::parse_field_int::<usize>(value)?;
                Ok(())
            }
            // @"line-max": usize = 80
            b"line-max" => {
                self.line_max = rustty_args::args::parse_field_int::<usize>(value)?;
                Ok(())
            }
            // @"style-rate": f64 = 0.0
            b"style-rate" => {
                self.style_rate = rustty_args::args::parse_field_float::<f64>(value)?;
                Ok(())
            }
            // @"run-min": usize = 4
            b"run-min" => {
                self.run_min = rustty_args::args::parse_field_int::<usize>(value)?;
                Ok(())
            }
            // @"run-max": usize = 16
            b"run-max" => {
                self.run_max = rustty_args::args::parse_field_int::<usize>(value)?;
                Ok(())
            }
            // @"weight-one": f64 = 1.0
            b"weight-one" => {
                self.weight_one = rustty_args::args::parse_field_float::<f64>(value)?;
                Ok(())
            }
            // @"weight-two": f64 = 0.0
            b"weight-two" => {
                self.weight_two = rustty_args::args::parse_field_float::<f64>(value)?;
                Ok(())
            }
            // @"weight-three": f64 = 0.0
            b"weight-three" => {
                self.weight_three = rustty_args::args::parse_field_float::<f64>(value)?;
                Ok(())
            }
            // @"weight-four": f64 = 0.0
            b"weight-four" => {
                self.weight_four = rustty_args::args::parse_field_float::<f64>(value)?;
                Ok(())
            }
            // @"grapheme-rate": f64 = 0.0
            b"grapheme-rate" => {
                self.grapheme_rate = rustty_args::args::parse_field_float::<f64>(value)?;
                Ok(())
            }
            // @"osc8-rate": f64 = 0.0
            b"osc8-rate" => {
                self.osc8_rate = rustty_args::args::parse_field_float::<f64>(value)?;
                Ok(())
            }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}

/// PORT NOTE: `const Styled = @This()`.
pub struct Styled {
    opts: Options,
}

/// PORT NOTE: `const State = struct { ... }` (file-scope in the reference).
#[derive(Default)]
struct State {
    /// True when a non-default SGR style is currently active.
    styled: bool,

    /// True when an OSC 8 hyperlink is currently open.
    link: bool,
}

impl Styled {
    // pub fn create(alloc: Allocator, opts: Options) !*Styled {
    pub fn create(opts: Options) -> Result<Styled, InvalidValue> {
        // for ([_]f64{ opts.@"style-rate", opts.@"grapheme-rate", opts.@"osc8-rate" }) |rate| {
        //     if (rate < 0 or rate > 1) return error.InvalidValue;
        // }
        for rate in [opts.style_rate, opts.grapheme_rate, opts.osc8_rate] {
            if !(0.0..=1.0).contains(&rate) {
                return Err(InvalidValue);
            }
        }

        let weights = [
            opts.weight_one,
            opts.weight_two,
            opts.weight_three,
            opts.weight_four,
        ];
        let mut weight_sum: f64 = 0.0;
        for weight in weights {
            if weight < 0.0 {
                return Err(InvalidValue);
            }
            weight_sum += weight;
        }
        if weight_sum <= 0.0 {
            return Err(InvalidValue);
        }

        if opts.line_min == 0 {
            return Err(InvalidValue);
        }
        if opts.run_min == 0 {
            return Err(InvalidValue);
        }

        Ok(Styled { opts })
    }

    // pub fn destroy(self: *Styled, alloc: Allocator) void { alloc.destroy(self); }
    pub fn destroy(self) {}

    // pub fn run(self: *Styled, writer: *std.Io.Writer, rand: std.Random) !void {
    pub fn run<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        rand: Rng,
    ) -> Result<(), WriteFailed> {
        // var prng: ?std.Random.DefaultPrng = null;
        // var gen_rand = rand;
        // if (self.opts.seed) |seed| {
        //     prng = std.Random.DefaultPrng.init(seed);
        //     gen_rand = prng.?.random();
        // }
        let seeded;
        let gen_rand = match self.opts.seed {
            Some(seed) => {
                seeded = DefaultPrng::init(seed);
                seeded.random()
            }
            None => rand,
        };

        // var state: State = .{};
        let mut state = State::default();
        loop {
            // self.writeLine(writer, gen_rand, &state) catch |err| {
            //     ... error.BrokenPipe => return, // stdout closed
            //         error.WriteFailed => return, // fixed buffer full
            // };
            if self.write_line(writer, gen_rand, &mut state).is_err() {
                return Ok(());
            }
        }
    }

    // fn writeLine(self, writer, rand, state) std.Io.Writer.Error!void {
    fn write_line<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        rand: Rng,
        state: &mut State,
    ) -> Result<(), WriteFailed> {
        let line_min = self.opts.line_min;
        // const line_max = @max(line_min, self.opts.@"line-max");
        let line_max = line_min.max(self.opts.line_max);
        let run_min = self.opts.run_min;
        let run_max = run_min.max(self.opts.run_max);

        // const line_len = rand.intRangeAtMostBiased(usize, line_min, line_max);
        let line_len = rand.int_range_at_most_biased::<usize>(line_min, line_max);
        let mut remaining = line_len;
        while remaining > 0 {
            if self.opts.style_rate > 0.0 && rand.float_f64() < self.opts.style_rate {
                self.write_sgr(writer, rand, state)?;
            }

            if self.opts.osc8_rate > 0.0 && rand.float_f64() < self.opts.osc8_rate {
                self.toggle_link(writer, rand, state)?;
            }

            // const run_len = @min(remaining, rand.intRangeAtMostBiased(usize, run_min, run_max));
            let run_len = remaining.min(rand.int_range_at_most_biased::<usize>(run_min, run_max));
            for _ in 0..run_len {
                self.write_char(writer, rand)?;
            }
            remaining -= run_len;
        }

        // Close any open styling/hyperlink so state never bleeds across
        // lines. This mirrors what well-behaved programs do.
        if state.styled {
            writer.writeAll(b"\x1b[0m")?;
            state.styled = false;
        }
        if state.link {
            writer.writeAll(b"\x1b]8;;\x1b\\")?;
            state.link = false;
        }

        writer.writeAll(b"\r\n")
    }

    fn write_char<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        rand: Rng,
    ) -> Result<(), WriteFailed> {
        let weights = [
            self.opts.weight_one,
            self.opts.weight_two,
            self.opts.weight_three,
            self.opts.weight_four,
        ];

        // const cp: u21 = switch (rand.weightedIndex(f64, &weights)) { ... };
        let cp: u32 = match rand.weighted_index_f64(&weights) {
            // Printable ASCII with occasional word-ish spacing.
            0 => {
                if rand.float_f64() < 0.15 {
                    u32::from(b' ')
                } else {
                    rand.int_range_at_most_biased::<U21>(U21(0x21), U21(0x7E)).0 as u32
                }
            }

            // Latin-1 Supplement and Latin Extended-A/B (narrow).
            1 => {
                rand.int_range_at_most_biased::<U21>(U21(0xC0), U21(0x24F))
                    .0 as u32
            }

            // Kana or CJK ideographs (wide).
            2 => {
                if rand.boolean() {
                    rand.int_range_at_most_biased::<U21>(U21(0x3041), U21(0x30FE))
                        .0 as u32
                } else {
                    rand.int_range_at_most_biased::<U21>(U21(0x4E00), U21(0x9FFF))
                        .0 as u32
                }
            }

            // Emoji (wide).
            3 => {
                rand.int_range_at_most_biased::<U21>(U21(0x1F600), U21(0x1F64F))
                    .0 as u32
            }

            // else => unreachable,
            _ => unreachable!(),
        };

        // try writer.print("{u}", .{cp});
        write_codepoint(writer, cp)?;

        // Optionally follow with a combining diacritical mark to form a
        // multi-codepoint grapheme cluster.
        if self.opts.grapheme_rate > 0.0 && rand.float_f64() < self.opts.grapheme_rate {
            // const mark = rand.intRangeAtMostBiased(u21, 0x300, 0x36F);
            let mark = rand
                .int_range_at_most_biased::<U21>(U21(0x300), U21(0x36F))
                .0 as u32;
            write_codepoint(writer, mark)?;
        }
        Ok(())
    }

    fn write_sgr<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        rand: Rng,
        state: &mut State,
    ) -> Result<(), WriteFailed> {
        // _ = self;
        let _ = self;

        // switch (rand.uintLessThan(u8, 8)) {
        match rand.uint_less_than::<u8>(8) {
            // Reset.
            0 => {
                writer.writeAll(b"\x1b[0m")?;
                state.styled = false;
                return Ok(());
            }

            // 16-color foreground.
            1 => {
                // const base: u8 = if (rand.boolean()) 30 else 90;
                let base: u8 = if rand.boolean() { 30 } else { 90 };
                // try writer.print("\x1b[{d}m", .{base + rand.uintLessThan(u8, 8)});
                writer.writeAll(b"\x1b[")?;
                writer.writeUint(u64::from(base + rand.uint_less_than::<u8>(8)))?;
                writer.writeAll(b"m")?;
            }

            // 16-color background.
            2 => {
                let base: u8 = if rand.boolean() { 40 } else { 100 };
                writer.writeAll(b"\x1b[")?;
                writer.writeUint(u64::from(base + rand.uint_less_than::<u8>(8)))?;
                writer.writeAll(b"m")?;
            }

            // 256-color foreground/background.
            // 3 => try writer.print("\x1b[38;5;{d}m", .{rand.int(u8)}),
            3 => {
                writer.writeAll(b"\x1b[38;5;")?;
                writer.writeUint(u64::from(rand.int::<u8>()))?;
                writer.writeAll(b"m")?;
            }
            // 4 => try writer.print("\x1b[48;5;{d}m", .{rand.int(u8)}),
            4 => {
                writer.writeAll(b"\x1b[48;5;")?;
                writer.writeUint(u64::from(rand.int::<u8>()))?;
                writer.writeAll(b"m")?;
            }

            // RGB foreground/background. Components are quantized so the
            // number of unique styles stays bounded (6^3 per channel pair)
            // rather than pathologically unique per cell.
            // 5, 6 => |kind| { ... }
            kind @ (5 | 6) => {
                let layer: u8 = if kind == 5 { 38 } else { 48 };
                // try writer.print("\x1b[{d};2;{d};{d};{d}m", .{ layer, r, g, b });
                //   (the three components are drawn left-to-right, preserving
                //   the reference's argument evaluation order)
                let r = rand.uint_less_than::<u8>(6) * 51;
                let g = rand.uint_less_than::<u8>(6) * 51;
                let b = rand.uint_less_than::<u8>(6) * 51;
                writer.writeAll(b"\x1b[")?;
                writer.writeUint(u64::from(layer))?;
                writer.writeAll(b";2;")?;
                writer.writeUint(u64::from(r))?;
                writer.writeAll(b";")?;
                writer.writeUint(u64::from(g))?;
                writer.writeAll(b";")?;
                writer.writeUint(u64::from(b))?;
                writer.writeAll(b"m")?;
            }

            // Attributes: bold, italic, underline, inverse, strikethrough.
            7 => {
                let attrs: [u8; 5] = [1, 3, 4, 7, 9];
                writer.writeAll(b"\x1b[")?;
                writer.writeUint(u64::from(attrs[rand.uint_less_than::<usize>(attrs.len())]))?;
                writer.writeAll(b"m")?;
            }

            // else => unreachable,
            _ => unreachable!(),
        }

        state.styled = true;
        Ok(())
    }

    fn toggle_link<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        rand: Rng,
        state: &mut State,
    ) -> Result<(), WriteFailed> {
        // _ = self;
        let _ = self;

        if state.link {
            writer.writeAll(b"\x1b]8;;\x1b\\")?;
            state.link = false;
            return Ok(());
        }

        // try writer.print("\x1b]8;;http://example.com/{d}\x1b\\", .{rand.uintLessThan(u16, 1024)});
        writer.writeAll(b"\x1b]8;;http://example.com/")?;
        writer.writeUint(u64::from(rand.uint_less_than::<u16>(1024)))?;
        writer.writeAll(b"\x1b\\")?;
        state.link = true;
        Ok(())
    }
}

/// PORT NOTE: `writer.print("{u}", .{cp})` — the UTF-8 encoding of one
/// codepoint. Every range drawn above is a valid Unicode scalar value (no
/// surrogates), so `char::from_u32` cannot fail.
fn write_codepoint<W: Writer<Error = WriteFailed>>(
    writer: &mut W,
    cp: u32,
) -> Result<(), WriteFailed> {
    let c = char::from_u32(cp).unwrap();
    let mut buf = [0u8; 4];
    writer.writeAll(c.encode_utf8(&mut buf).as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::FixedWriter;
    use crate::rand::DefaultPrng;

    // test Styled {
    #[test]
    fn styled() {
        let impl_ = Styled::create(Options {
            seed: Some(1),
            ..Options::default()
        })
        .unwrap();

        let prng = DefaultPrng::init(1);
        let rand = prng.random();

        let mut buf = [0u8; 4096];
        let mut writer = FixedWriter::fixed(&mut buf);
        impl_.run(&mut writer, rand).unwrap();
        let output = writer.buffered();
        assert!(!output.is_empty());

        // Default options: plain printable ASCII lines only.
        for &byte in output {
            assert!(byte == b'\r' || byte == b'\n' || (0x20..0x7F).contains(&byte));
        }
        impl_.destroy();
    }

    // test "Styled styled output" {
    #[test]
    fn styled_styled_output() {
        let impl_ = Styled::create(Options {
            seed: Some(1),
            style_rate: 0.5,
            osc8_rate: 0.2,
            ..Options::default()
        })
        .unwrap();

        let prng = DefaultPrng::init(1);
        let rand = prng.random();

        let mut buf = [0u8; 16384];
        let mut writer = FixedWriter::fixed(&mut buf);
        impl_.run(&mut writer, rand).unwrap();
        let output = writer.buffered();

        // Must contain SGR and OSC 8 sequences.
        // (std.mem.indexOf(u8, output, needle) != null)
        assert!(find(output, b"\x1b[").is_some());
        assert!(find(output, b"\x1b]8;;").is_some());
        impl_.destroy();
    }

    // test "Styled unicode output" {
    #[test]
    fn styled_unicode_output() {
        let impl_ = Styled::create(Options {
            seed: Some(1),
            weight_two: 1.0,
            weight_three: 1.0,
            weight_four: 1.0,
            grapheme_rate: 0.25,
            ..Options::default()
        })
        .unwrap();

        let prng = DefaultPrng::init(1);
        let rand = prng.random();

        let mut buf = [0u8; 16384];
        let mut writer = FixedWriter::fixed(&mut buf);
        impl_.run(&mut writer, rand).unwrap();
        let output = writer.buffered();

        // No escapes, valid UTF-8 (modulo a possibly truncated tail from
        // the fixed buffer filling up mid-sequence).
        // (std.mem.indexOfScalar(u8, output, 0x1B) == null)
        assert!(!output.contains(&0x1B));
        let mut end = output.len();
        while end > 0 && output[end - 1] & 0xC0 == 0x80 {
            end -= 1;
        }
        if end > 0 && output[end - 1] >= 0xC0 {
            end -= 1;
        }
        // PORT NOTE: `std.unicode.utf8ValidateSlice` -> `str::from_utf8` (the
        // identical UTF-8 well-formedness check).
        assert!(core::str::from_utf8(&output[..end]).is_ok());
        impl_.destroy();
    }

    // test "Styled invalid options" {
    #[test]
    fn styled_invalid_options() {
        // try testing.expectError(error.InvalidValue, Styled.create(alloc, .{ .@"style-rate" = 1.5 }));
        assert!(Styled::create(Options {
            style_rate: 1.5,
            ..Options::default()
        })
        .is_err());
        // try testing.expectError(error.InvalidValue, Styled.create(alloc, .{ .@"weight-one" = 0 }));
        assert!(Styled::create(Options {
            weight_one: 0.0,
            ..Options::default()
        })
        .is_err());
        // try testing.expectError(error.InvalidValue, Styled.create(alloc, .{ .@"line-min" = 0 }));
        assert!(Styled::create(Options {
            line_min: 0,
            ..Options::default()
        })
        .is_err());
    }

    /// PORT NOTE: `std.mem.indexOf(u8, haystack, needle)`.
    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }
}
