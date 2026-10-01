//! PORTS: ghostty src/config/Config.zig — `MouseScrollMultiplier`,
//! `QuickTerminalSize`, `Theme`, `Duration` and `WindowPadding`.

use super::*;

// pub const MouseScrollMultiplier = struct { precision: f64 = 1, discrete: f64 = 3; pub const default = .{} };
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseScrollMultiplier {
    pub precision: f64,
    pub discrete: f64,
}

impl Default for MouseScrollMultiplier {
    fn default() -> Self {
        Self {
            precision: 1.0,
            discrete: 3.0,
        }
    }
}

impl MouseScrollMultiplier {
    // pub const default: MouseScrollMultiplier = .{};
    pub fn default_const() -> MouseScrollMultiplier {
        MouseScrollMultiplier::default()
    }

    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // self.* = cli.args.parseAutoStruct(MouseScrollMultiplier, alloc, input, self.*) catch |err| switch (err) {
        //     error.InvalidValue => bare: {
        //         const v = std.fmt.parseFloat(f64, input) catch return error.InvalidValue;
        //         break :bare .{ .precision = v, .discrete = v };
        //     },
        //     else => return err,
        // };
        //
        // PORT NOTE: `parseAutoStruct` mutates a destination in place; we run it on
        // a copy seeded with the current value (the reference passes `self.*` as the
        // `default_` to inherit unset fields), then commit on success.
        let mut tmp = *self;
        match parse_auto_struct(&mut tmp, input) {
            Ok(()) => *self = tmp,
            Err(Error::InvalidValue) => {
                // bare: the whole value is a single float applied to both fields.
                let v = parse_float_f64(input).map_err(|_| Error::InvalidValue)?;
                *self = MouseScrollMultiplier {
                    precision: v,
                    discrete: v,
                };
            }
            Err(err) => return Err(err),
        }
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> MouseScrollMultiplier {
        // _ = alloc; return self.*;
        *self
    }

    /// Compare if two of our value are equal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &MouseScrollMultiplier) -> bool {
        // return self.precision == other.precision and self.discrete == other.discrete;
        self.precision == other.precision && self.discrete == other.discrete
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // var buf: [4096]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        // writer.print("precision:{d},discrete:{d}", .{ self.precision, self.discrete }) catch ...;
        // try formatter.formatEntry([]const u8, writer.buffered());
        let s = format!(
            "precision:{},discrete:{}",
            fmt_float(self.precision),
            fmt_float(self.discrete)
        );
        format_entry_string(name, s.as_bytes(), writer)
    }
}

// PORTS: the auto-struct field reflection of `MouseScrollMultiplier` used by
// `cli.args.parseAutoStruct` — fields `precision`, `discrete` (both `f64`).
impl AutoStruct for MouseScrollMultiplier {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"precision", b"discrete"]
    }

    fn set_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
        let v = value.ok_or(Error::ValueRequired)?;
        match key {
            b"precision" => self.precision = parse_float_f64(v).map_err(|_| Error::InvalidValue)?,
            b"discrete" => self.discrete = parse_float_f64(v).map_err(|_| Error::InvalidValue)?,
            _ => return Err(Error::InvalidField),
        }
        Ok(())
    }

    fn apply_default(&mut self, _i: usize) -> Result<(), ParseError> {
        // Both fields keep whatever the seeded destination already holds (the
        // reference inherits unset fields from the provided `default_`).
        Ok(())
    }
}

impl FormatEntry for MouseScrollMultiplier {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        MouseScrollMultiplier::format_entry(self, name, writer)
    }
}

/// PORTS: `std.fmt.parseFloat(f64, input)`.
//
// PORT NOTE (S75-F1; NEW-FINDINGS N120, task T-C3): this used to be
// `core::str::from_utf8(input).ok().and_then(|s| s.parse::<f64>().ok())`, and
// Rust's grammar differs from Zig's: `std.fmt.parseFloat` accepts `_` digit
// separators between two digits (`wght=1_000`, `precision:1_0`) and hex floats
// with an OPTIONAL `p` exponent (`0x1p8`, `0x1.8p0`, `0x1.8`), and Rust rejects
// both. S75-F1 then ported the grammar here as a private `pf_*` copy; it now
// calls the single port, `rustty_zigstd::fmt::parse_float`, which carries the
// same grammar, the ported `convert_hex`, and Zig 0.16.0's std test tables.
pub(super) fn parse_float_f64(input: &[u8]) -> Result<f64, ParseError> {
    rustty_zigstd::fmt::parse_float::<f64>(input).map_err(|_| Error::InvalidValue)
}

/// PORTS: the `{d}` float formatter — shortest round-trippable decimal. Rust's
/// default float `Display` produces the same shortest decimal for every
/// finite value and for `inf`/`-inf` (MEASURED against zig 0.16.0 `{d}`:
/// 1e100, 1e-7, 5e-324, f64::MAX, -0.0 and others print identically).
///
/// PORT NOTE (LEDGER S75-F3): NaN is the one difference. Zig's `{d}` prints
/// `nan` (`-nan` with the sign bit set, `std/fmt/float.zig` `copySpecialStr`);
/// Rust's `Display` prints `NaN` for both.
pub(super) fn fmt_float(v: f64) -> String {
    if v.is_nan() {
        return if v.is_sign_negative() { "-nan" } else { "nan" }.to_string();
    }
    format!("{v}")
}

// pub const QuickTerminalSize = struct { primary: ?Size, secondary: ?Size; Size; Dimensions; C; ... };
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct QuickTerminalSize {
    pub primary: Option<QuickTerminalSizeSize>,
    pub secondary: Option<QuickTerminalSizeSize>,
}

// pub const Size = union(enum) { percentage: f32, pixels: u32 };
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuickTerminalSizeSize {
    Percentage(f32),
    Pixels(u32),
}

/// PORTS: the error set of `QuickTerminalSize.parseCLI`/`Size.parse` —
/// `error{ ValueRequired, InvalidValue, MissingUnit, TooManyArguments }`. The
/// distinguishable `MissingUnit`/`TooManyArguments` errors are asserted by
/// `test "parse QuickTerminalSize"`, so they are preserved (the base args
/// `Error` set has neither).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickTerminalSizeParseError {
    ValueRequired,
    InvalidValue,
    MissingUnit,
    TooManyArguments,
}

impl QuickTerminalSizeSize {
    // pub fn toPixels(self: Size, parent_dimensions: u32) u32
    pub fn to_pixels(self: QuickTerminalSizeSize, parent_dimensions: u32) -> u32 {
        // switch (self) {
        match self {
            // .percentage => |v| {
            //     const dim: f32 = @floatFromInt(parent_dimensions);
            //     return @intFromFloat(v / 100.0 * dim);
            // },
            QuickTerminalSizeSize::Percentage(v) => {
                let dim: f32 = parent_dimensions as f32;
                // PORT NOTE: reference defect (R13), UPSTREAM-DEFECTS.md UD96.
                // `parse` accepts any non-negative float, including `inf%` and
                // `1e10%` (and `nan%`, since `nan < 0` is false), and the
                // reference's `@intFromFloat` traps in ReleaseSafe on a result
                // outside `u32` (measured, Zig 0.16.0; ReleaseFast gives
                // 4294967295, NaN gives 0 in every mode). Rust's `as` saturates
                // to those same ReleaseFast values, so the port computes the
                // reference's shipped result without the trap.
                (v / 100.0 * dim) as u32
            }
            // .pixels => |v| return v,
            QuickTerminalSizeSize::Pixels(v) => v,
        }
    }

    // pub fn parse(input: []const u8) !Size
    pub fn parse(input: &[u8]) -> Result<QuickTerminalSizeSize, QuickTerminalSizeParseError> {
        // if (input.len == 0) return error.ValueRequired;
        if input.is_empty() {
            return Err(QuickTerminalSizeParseError::ValueRequired);
        }

        // if (std.mem.endsWith(u8, input, "px")) {
        if input.ends_with(b"px") {
            // return .{ .pixels = std.fmt.parseInt(u32, input[0 .. input.len - "px".len], 10)
            //     catch return error.InvalidValue };
            let v = parse_int_u32_base10(&input[0..input.len() - 2])
                .map_err(|_| QuickTerminalSizeParseError::InvalidValue)?;
            return Ok(QuickTerminalSizeSize::Pixels(v));
        }

        // if (std.mem.endsWith(u8, input, "%")) {
        if input.ends_with(b"%") {
            // const percentage = std.fmt.parseFloat(f32, input[0 .. input.len - "%".len])
            //     catch return error.InvalidValue;
            let percentage = parse_float_f32(&input[0..input.len() - 1])
                .map_err(|_| QuickTerminalSizeParseError::InvalidValue)?;

            // if (percentage < 0) return error.InvalidValue;
            if percentage < 0.0 {
                return Err(QuickTerminalSizeParseError::InvalidValue);
            }
            // return .{ .percentage = percentage };
            return Ok(QuickTerminalSizeSize::Percentage(percentage));
        }

        // return error.MissingUnit;
        Err(QuickTerminalSizeParseError::MissingUnit)
    }

    // fn format(self: Size, writer: *std.Io.Writer) !void
    fn format(self: QuickTerminalSizeSize, buf: &mut Vec<u8>) {
        // switch (self) {
        match self {
            // .percentage => |v| try writer.print("{d}%", .{v}),
            QuickTerminalSizeSize::Percentage(v) => {
                buf.extend_from_slice(format!("{v}").as_bytes());
                buf.push(b'%');
            }
            // .pixels => |v| try writer.print("{}px", .{v}),
            QuickTerminalSizeSize::Pixels(v) => {
                buf.extend_from_slice(format!("{v}").as_bytes());
                buf.extend_from_slice(b"px");
            }
        }
    }
}

/// PORTS: `std.fmt.parseInt(u32, input, 10)`, through the single port of
/// `std.fmt.parseInt` (`rustty_zigstd::fmt`, NEW-FINDINGS N120).
//
// PORT NOTE: Rust's `str::parse::<u32>()` is NOT a faithful stand-in: the
// reference accepts a leading `+`, accepts `_` digit separators anywhere except
// the first/last position of the post-sign digit run (doubled separators
// included — `1__0` is 10 for `parseInt`), and treats a `-` sign on an unsigned
// type as `error.Overflow` for any non-zero magnitude while `-0` parses to 0.
// All call sites collapse both `Overflow` and `InvalidCharacter` into
// `error.InvalidValue`, so only the accept/reject boundary is observable.
fn parse_int_u32_base10(input: &[u8]) -> Result<u32, ParseIntError> {
    rustty_zigstd::fmt::parse_int::<u32>(input, 10)
}

/// PORTS: `std.fmt.parseFloat(f32, input)`.
//
// PORT NOTE (NEW-FINDINGS N123, task T-C3): this validated and stripped `_`
// separators and then called Rust's `str::parse::<f32>()`, which has no hex
// float grammar, so a hex-float percentage (`quick-terminal-size = 0x1p4%`)
// that the reference loads was rejected. It now calls the single port of
// `std.fmt.parseFloat` (`rustty_zigstd::fmt`, NEW-FINDINGS N120).
fn parse_float_f32(input: &[u8]) -> Result<f32, ParseError> {
    rustty_zigstd::fmt::parse_float::<f32>(input).map_err(|_| Error::InvalidValue)
}

// pub const Dimensions = struct { width: u32, height: u32 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuickTerminalSizeDimensions {
    pub width: u32,
    pub height: u32,
}

/// C API structure for QuickTerminalSize
// pub const C = extern struct { primary: C.Size, secondary: C.Size };
#[repr(C)]
pub struct QuickTerminalSizeC {
    pub primary: QuickTerminalSizeCSize,
    pub secondary: QuickTerminalSizeCSize,
}

// pub const Size = extern struct { tag: Tag, value: Value };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct QuickTerminalSizeCSize {
    pub tag: QuickTerminalSizeCTag,
    pub value: QuickTerminalSizeCValue,
}

/// c_int because it needs to be extern compatible
// pub const Tag = enum(c_int) { none, percentage, pixels };
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum QuickTerminalSizeCTag {
    None,
    Percentage,
    Pixels,
}

// pub const Value = extern union { percentage: f32, pixels: u32 };
#[repr(C)]
#[derive(Clone, Copy)]
pub union QuickTerminalSizeCValue {
    pub percentage: f32,
    pub pixels: u32,
}

impl QuickTerminalSizeCSize {
    // pub const none: C.Size = .{ .tag = .none, .value = undefined };
    pub fn none() -> QuickTerminalSizeCSize {
        QuickTerminalSizeCSize {
            tag: QuickTerminalSizeCTag::None,
            value: QuickTerminalSizeCValue { pixels: 0 },
        }
    }

    // pub fn percentage(v: f32) C.Size
    pub fn percentage(v: f32) -> QuickTerminalSizeCSize {
        // return .{ .tag = .percentage, .value = .{ .percentage = v } };
        QuickTerminalSizeCSize {
            tag: QuickTerminalSizeCTag::Percentage,
            value: QuickTerminalSizeCValue { percentage: v },
        }
    }

    // pub fn pixels(v: u32) C.Size
    pub fn pixels(v: u32) -> QuickTerminalSizeCSize {
        // return .{ .tag = .pixels, .value = .{ .pixels = v } };
        QuickTerminalSizeCSize {
            tag: QuickTerminalSizeCTag::Pixels,
            value: QuickTerminalSizeCValue { pixels: v },
        }
    }
}

impl QuickTerminalSize {
    // pub fn cval(self: QuickTerminalSize) C
    pub fn cval(self: QuickTerminalSize) -> QuickTerminalSizeC {
        // return .{
        //     .primary = if (self.primary) |p| switch (p) { .percentage => |v| .percentage(v), .pixels => |v| .pixels(v) } else .none,
        //     .secondary = if (self.secondary) |s| switch (s) { .percentage => |v| .percentage(v), .pixels => |v| .pixels(v) } else .none,
        // };
        QuickTerminalSizeC {
            primary: match self.primary {
                Some(QuickTerminalSizeSize::Percentage(v)) => QuickTerminalSizeCSize::percentage(v),
                Some(QuickTerminalSizeSize::Pixels(v)) => QuickTerminalSizeCSize::pixels(v),
                None => QuickTerminalSizeCSize::none(),
            },
            secondary: match self.secondary {
                Some(QuickTerminalSizeSize::Percentage(v)) => QuickTerminalSizeCSize::percentage(v),
                Some(QuickTerminalSizeSize::Pixels(v)) => QuickTerminalSizeCSize::pixels(v),
                None => QuickTerminalSizeCSize::none(),
            },
        }
    }

    // pub fn calculate(self: QuickTerminalSize, position: QuickTerminalPosition, dims: Dimensions) Dimensions
    pub fn calculate(
        self: QuickTerminalSize,
        position: QuickTerminalPosition,
        dims: QuickTerminalSizeDimensions,
    ) -> QuickTerminalSizeDimensions {
        // switch (position) {
        match position {
            // .left, .right => return .{
            //     .width = if (self.primary) |v| v.toPixels(dims.width) else 400,
            //     .height = if (self.secondary) |v| v.toPixels(dims.height) else dims.height,
            // },
            QuickTerminalPosition::Left | QuickTerminalPosition::Right => {
                QuickTerminalSizeDimensions {
                    width: match self.primary {
                        Some(v) => v.to_pixels(dims.width),
                        None => 400,
                    },
                    height: match self.secondary {
                        Some(v) => v.to_pixels(dims.height),
                        None => dims.height,
                    },
                }
            }
            // .top, .bottom => return .{
            //     .width = if (self.secondary) |v| v.toPixels(dims.width) else dims.width,
            //     .height = if (self.primary) |v| v.toPixels(dims.height) else 400,
            // },
            QuickTerminalPosition::Top | QuickTerminalPosition::Bottom => {
                QuickTerminalSizeDimensions {
                    width: match self.secondary {
                        Some(v) => v.to_pixels(dims.width),
                        None => dims.width,
                    },
                    height: match self.primary {
                        Some(v) => v.to_pixels(dims.height),
                        None => 400,
                    },
                }
            }
            // .center => if (dims.width >= dims.height) { ... } else { ... },
            QuickTerminalPosition::Center => {
                if dims.width >= dims.height {
                    QuickTerminalSizeDimensions {
                        width: match self.primary {
                            Some(v) => v.to_pixels(dims.width),
                            None => 800,
                        },
                        height: match self.secondary {
                            Some(v) => v.to_pixels(dims.height),
                            None => 400,
                        },
                    }
                } else {
                    QuickTerminalSizeDimensions {
                        width: match self.secondary {
                            Some(v) => v.to_pixels(dims.width),
                            None => 400,
                        },
                        height: match self.primary {
                            Some(v) => v.to_pixels(dims.height),
                            None => 800,
                        },
                    }
                }
            }
        }
    }

    // pub fn parseCLI(self: *QuickTerminalSize, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), QuickTerminalSizeParseError> {
        // const input_ = input orelse return error.ValueRequired;
        let input_ = input.ok_or(QuickTerminalSizeParseError::ValueRequired)?;
        // var it = std.mem.splitScalar(u8, input_, ',');
        let mut it = split_scalar(input_, b',');

        // const primary = std.mem.trim(u8, it.next() orelse return error.ValueRequired, cli.args.whitespace);
        let primary = trim_ws_args(
            it.next()
                .ok_or(QuickTerminalSizeParseError::ValueRequired)?,
        );
        // self.primary = try .parse(primary);
        self.primary = Some(QuickTerminalSizeSize::parse(primary)?);

        // self.secondary = secondary: {
        //     const secondary = std.mem.trim(u8, it.next() orelse break :secondary null, cli.args.whitespace);
        //     break :secondary try .parse(secondary);
        // };
        self.secondary = match it.next() {
            Some(raw) => Some(QuickTerminalSizeSize::parse(trim_ws_args(raw))?),
            None => None,
        };

        // if (it.next()) |_| return error.TooManyArguments;
        if it.next().is_some() {
            return Err(QuickTerminalSizeParseError::TooManyArguments);
        }
        Ok(())
    }

    // pub fn clone(self: *const QuickTerminalSize, _: Allocator) Allocator.Error!QuickTerminalSize
    pub fn clone_in(&self) -> QuickTerminalSize {
        // return .{ .primary = self.primary, .secondary = self.secondary };
        *self
    }

    // pub fn formatEntry(self: QuickTerminalSize, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // const primary = self.primary orelse return;
        let primary = match self.primary {
            Some(p) => p,
            None => return Ok(()),
        };

        // var buf: [4096]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        let mut buf: Vec<u8> = Vec::new();

        // primary.format(&writer) catch return error.OutOfMemory;
        primary.format(&mut buf);
        // if (self.secondary) |secondary| {
        //     writer.writeByte(',') catch return error.OutOfMemory;
        //     secondary.format(&writer) catch return error.OutOfMemory;
        // }
        if let Some(secondary) = self.secondary {
            buf.push(b',');
            secondary.format(&mut buf);
        }

        // try formatter.formatEntry([]const u8, writer.buffered());
        format_entry_string(name, &buf, writer)
    }
}

impl FormatEntry for QuickTerminalSize {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        QuickTerminalSize::format_entry(self, name, writer)
    }
}

/// PORTS: `std.mem.splitScalar(u8, input, delim)` — yields all delimiter-
/// separated slices including empty ones (unlike `tokenizeScalar`).
fn split_scalar(input: &[u8], delim: u8) -> impl Iterator<Item = &[u8]> {
    input.split(move |&c| c == delim)
}

// pub const Theme = struct { light: []const u8, dark: []const u8 };
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Theme {
    pub light: Vec<u8>,
    pub dark: Vec<u8>,
}

impl Theme {
    // pub fn parseCLI(self: *Theme, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // if (input.len == 0) return error.ValueRequired;
        if input.is_empty() {
            return Err(Error::ValueRequired);
        }

        // If there is a comma, equal sign, or colon, then we assume that
        // we're parsing a light/dark mode theme pair. Note that "=" isn't
        // actually valid for setting a light/dark mode pair but I anticipate
        // it'll be a common typo.
        //
        // On Windows, a colon at index 1 is a drive letter (e.g. C:\...)
        // and should not trigger light/dark pair parsing.
        // const has_colon = if (comptime builtin.os.tag == .windows)
        //     if (std.mem.indexOf(u8, input, ":")) |idx| idx != 1 else false
        // else
        //     std.mem.indexOf(u8, input, ":") != null;
        let has_colon = if cfg!(windows) {
            match index_of(input, b':') {
                Some(idx) => idx != 1,
                None => false,
            }
        } else {
            index_of(input, b':').is_some()
        };
        // if (std.mem.indexOf(u8, input, ",") != null or std.mem.indexOf(u8, input, "=") != null or has_colon) {
        if index_of(input, b',').is_some() || index_of(input, b'=').is_some() || has_colon {
            // self.* = try cli.args.parseAutoStruct(Theme, alloc, input, null);
            let mut tmp = Theme::default();
            parse_auto_struct(&mut tmp, input)?;
            *self = tmp;
            // return;
            return Ok(());
        }

        // Trim our value
        // const trimmed = std.mem.trim(u8, input, cli.args.whitespace);
        let trimmed = trim_ws_args(input);

        // Set the value to the specified value directly.
        // self.* = .{ .light = try alloc.dupeZ(u8, trimmed), .dark = self.light };
        let light = trimmed.to_vec();
        *self = Theme {
            dark: light.clone(),
            light,
        };
        Ok(())
    }

    /// Expand tilde paths in light/dark theme values.
    // pub fn finalize(self: *Theme, alloc: Allocator) Allocator.Error!void {
    pub fn finalize(&mut self) {
        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        let mut buf = [0u8; MAX_PATH_BYTES];

        // const light = expandHome(self.light, &buf);
        // if (!std.mem.eql(u8, light, self.light))
        //     self.light = try alloc.dupeZ(u8, light);
        let light = crate::config_struct::expand_home(&self.light, &mut buf);
        if light != &self.light[..] {
            let light = light.to_vec();
            self.light = light;
        }

        // const dark = expandHome(self.dark, &buf);
        // if (!std.mem.eql(u8, dark, self.dark))
        //     self.dark = try alloc.dupeZ(u8, dark);
        let dark = crate::config_struct::expand_home(&self.dark, &mut buf);
        if dark != &self.dark[..] {
            let dark = dark.to_vec();
            self.dark = dark;
        }
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Theme, alloc: Allocator) Allocator.Error!Theme
    pub fn clone_in(&self) -> Theme {
        // return .{ .light = try alloc.dupeZ(u8, self.light), .dark = try alloc.dupeZ(u8, self.dark) };
        Theme {
            light: self.light.clone(),
            dark: self.dark.clone(),
        }
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Theme, formatter: anytype) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // var buf: [4096]u8 = undefined;
        // if (std.mem.eql(u8, self.light, self.dark)) { try formatter.formatEntry([]const u8, self.light); return; }
        if self.light == self.dark {
            return format_entry_string(name, &self.light, writer);
        }

        // const str = std.fmt.bufPrint(&buf, "light:{s},dark:{s}", .{ self.light, self.dark }) catch return error.OutOfMemory;
        // try formatter.formatEntry([]const u8, str);
        //
        // PORT NOTE (fidelity): the pair is rendered through the reference's
        // FIXED `var buf: [4096]u8`, so `bufPrint` fails with
        // `error.NoSpaceLeft` — caught and returned as `error.OutOfMemory` —
        // once `light:<light>,dark:<dark>` passes 4096 bytes, failing the whole
        // config dump. `parseCLI` bounds neither component, so the case is
        // reachable; a growable `Vec` silently accepted it (parity S77-F1).
        // The equal-value early return above uses no buffer, matching the
        // reference.
        let mut buf = [0u8; 4096];
        let s = buf_print_parts(&mut buf, &[b"light:", &self.light, b",dark:", &self.dark])
            .ok_or(FormatError::OutOfMemory)?;
        format_entry_string(name, s, writer)
    }
}

// PORTS: the auto-struct field reflection of `Theme` — fields `light`, `dark`
// (both `[]const u8`, required: no struct default).
impl AutoStruct for Theme {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"light", b"dark"]
    }

    fn set_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
        let v = value.ok_or(Error::ValueRequired)?;
        match key {
            b"light" => self.light = v.to_vec(),
            b"dark" => self.dark = v.to_vec(),
            _ => return Err(Error::InvalidField),
        }
        Ok(())
    }

    fn apply_default(&mut self, _i: usize) -> Result<(), ParseError> {
        // light/dark are required (no struct default, `default_ == null`); an
        // unset field is `error.InvalidValue` (e.g. `light:foo` with no `dark`).
        Err(Error::InvalidValue)
    }
}

impl FormatEntry for Theme {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        Theme::format_entry(self, name, writer)
    }
}

/// PORTS: `std.mem.trim(u8, s, cli.args.whitespace)` where `cli.args.whitespace`
/// is `" \t"` (the args crate's `WHITESPACE`).
fn trim_ws_args(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && WHITESPACE.contains(&s[start]) {
        start += 1;
    }
    while end > start && WHITESPACE.contains(&s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

// pub const Duration = struct { duration: u64 = 0; const units = [...]; ... };
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Duration {
    /// Duration in nanoseconds
    pub duration: u64,
}

// const units = [_]struct { name: []const u8, factor: u64 }{ ... };
//
// The order is important as the first factor that matches will be the
// default unit that is used for formatting.
//
// PORT NOTE: `std.time.ns_per_*` constants are inlined as their literal
// nanosecond factors (ns_per_s = 1e9, ns_per_min = 60e9, etc.).
pub(super) const DURATION_UNITS: &[(&[u8], u64)] = &[
    // .{ .name = "y", .factor = 365 * std.time.ns_per_day },
    (b"y", 365 * NS_PER_DAY),
    // .{ .name = "w", .factor = std.time.ns_per_week },
    (b"w", NS_PER_WEEK),
    // .{ .name = "d", .factor = std.time.ns_per_day },
    (b"d", NS_PER_DAY),
    // .{ .name = "h", .factor = std.time.ns_per_hour },
    (b"h", NS_PER_HOUR),
    // .{ .name = "m", .factor = std.time.ns_per_min },
    (b"m", NS_PER_MIN),
    // .{ .name = "s", .factor = std.time.ns_per_s },
    (b"s", NS_PER_S),
    // .{ .name = "ms", .factor = std.time.ns_per_ms },
    (b"ms", NS_PER_MS),
    // .{ .name = "µs", .factor = std.time.ns_per_us },
    ("µs".as_bytes(), NS_PER_US),
    // .{ .name = "us", .factor = std.time.ns_per_us },
    (b"us", NS_PER_US),
    // .{ .name = "ns", .factor = 1 },
    (b"ns", 1),
];

const NS_PER_US: u64 = 1000;
const NS_PER_MS: u64 = 1000 * NS_PER_US;
pub(super) const NS_PER_S: u64 = 1000 * NS_PER_MS;
const NS_PER_MIN: u64 = 60 * NS_PER_S;
const NS_PER_HOUR: u64 = 60 * NS_PER_MIN;
const NS_PER_DAY: u64 = 24 * NS_PER_HOUR;
const NS_PER_WEEK: u64 = 7 * NS_PER_DAY;

impl Duration {
    // pub fn clone(self: *const Duration, _: Allocator) error{}!Duration
    pub fn clone_in(&self) -> Duration {
        // return .{ .duration = self.duration };
        *self
    }

    // pub fn equal(self: Duration, other: Duration) bool
    pub fn equal(self: Duration, other: Duration) -> bool {
        // return self.duration == other.duration;
        self == other
    }

    // pub fn round(self: Duration, to: u64) Duration
    pub fn round(self: Duration, to: u64) -> Duration {
        // return .{ .duration = self.duration / to * to };
        Duration {
            duration: self.duration / to * to,
        }
    }

    // pub fn lte(self: Duration, other: Duration) bool
    pub fn lte(self: Duration, other: Duration) -> bool {
        // return self.duration <= other.duration;
        self.duration <= other.duration
    }

    // pub fn parseCLI(input: ?[]const u8) !Duration
    pub fn parse_cli(input: Option<&[u8]>) -> Result<Duration, ParseError> {
        // var remaining = input orelse return error.ValueRequired;
        let mut remaining = input.ok_or(Error::ValueRequired)?;

        // var value: ?u64 = null;
        let mut value: Option<u64> = None;
        // while (remaining.len > 0) {
        while !remaining.is_empty() {
            // Skip over whitespace before the number
            // while (remaining.len > 0 and std.ascii.isWhitespace(remaining[0])) { remaining = remaining[1..]; }
            while !remaining.is_empty() && is_ascii_whitespace(remaining[0]) {
                remaining = &remaining[1..];
            }

            // There was whitespace at the end, that's OK
            // if (remaining.len == 0) break;
            if remaining.is_empty() {
                break;
            }

            // Find the longest number
            // const number: u64 = number: {
            //     var prev_number: ?u64 = null;
            //     var prev_remaining: ?[]const u8 = null;
            //     for (1..remaining.len + 1) |index| {
            //         prev_number = std.fmt.parseUnsigned(u64, remaining[0..index], 10) catch {
            //             if (prev_remaining) |prev| remaining = prev;
            //             break :number prev_number;
            //         };
            //         prev_remaining = remaining[index..];
            //     }
            //     if (prev_remaining) |prev| remaining = prev;
            //     break :number prev_number;
            // } orelse return error.InvalidValue;
            let number: u64 = {
                let mut prev_number: Option<u64> = None;
                let mut prev_remaining: Option<&[u8]> = None;
                let mut broke = false;
                for index in 1..remaining.len() + 1 {
                    match parse_unsigned_u64_base10(&remaining[0..index]) {
                        Ok(n) => {
                            prev_number = Some(n);
                            prev_remaining = Some(&remaining[index..]);
                        }
                        Err(_) => {
                            if let Some(prev) = prev_remaining {
                                remaining = prev;
                            }
                            broke = true;
                            break;
                        }
                    }
                }
                if !broke {
                    if let Some(prev) = prev_remaining {
                        remaining = prev;
                    }
                }
                match prev_number {
                    Some(n) => n,
                    None => return Err(Error::InvalidValue),
                }
            };

            // A number without a unit is invalid unless the number is
            // exactly zero. In that case, the unit is unambiguous since
            // its all the same.
            // if (remaining.len == 0) {
            //     if (number == 0) { value = 0; break; }
            //     return error.InvalidValue;
            // }
            if remaining.is_empty() {
                if number == 0 {
                    value = Some(0);
                    break;
                }
                return Err(Error::InvalidValue);
            }

            // Find the longest matching unit. Needs to be the longest matching
            // to distinguish 'm' from 'ms'.
            // const factor = factor: {
            //     var prev_factor: ?u64 = null;
            //     var prev_index: ?usize = null;
            //     for (1..remaining.len + 1) |index| {
            //         const next_factor = next: {
            //             for (units) |unit| { if (std.mem.eql(u8, unit.name, remaining[0..index])) break :next unit.factor; }
            //             break :next null;
            //         };
            //         if (next_factor) |next| { prev_factor = next; prev_index = index; }
            //     }
            //     if (prev_index) |index| { remaining = remaining[index..]; }
            //     break :factor prev_factor;
            // } orelse return error.InvalidValue;
            let factor: u64 = {
                let mut prev_factor: Option<u64> = None;
                let mut prev_index: Option<usize> = None;
                for index in 1..remaining.len() + 1 {
                    let next_factor = {
                        let mut found = None;
                        for &(uname, ufactor) in DURATION_UNITS {
                            if uname == &remaining[0..index] {
                                found = Some(ufactor);
                                break;
                            }
                        }
                        found
                    };
                    if let Some(next) = next_factor {
                        prev_factor = Some(next);
                        prev_index = Some(index);
                    }
                }
                if let Some(index) = prev_index {
                    remaining = &remaining[index..];
                }
                match prev_factor {
                    Some(f) => f,
                    None => return Err(Error::InvalidValue),
                }
            };

            // Add our time value to the total. Avoid overflow with saturating math.
            // const diff = std.math.mul(u64, number, factor) catch std.math.maxInt(u64);
            //
            // PORT NOTE: `std.math.mul(...) catch maxInt` is saturating-on-overflow;
            // `saturating_mul` is the exact equivalent.
            let diff = number.saturating_mul(factor);
            // value = (value orelse 0) +| diff;
            value = Some(value.unwrap_or(0).saturating_add(diff));
        }

        // return if (value) |v| .{ .duration = v } else error.ValueRequired;
        match value {
            Some(v) => Ok(Duration { duration: v }),
            None => Err(Error::ValueRequired),
        }
    }

    // pub fn formatEntry(self: Duration, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // var buf: [64]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        // try self.format(&writer);
        // try formatter.formatEntry([]const u8, writer.buffered());
        let mut buf: Vec<u8> = Vec::new();
        self.format(&mut buf);
        format_entry_string(name, &buf, writer)
    }

    // pub fn format(self: Duration, writer: *std.Io.Writer) !void
    pub fn format(self: &Duration, buf: &mut Vec<u8>) {
        // var value = self.duration;
        let mut value = self.duration;
        // var i: usize = 0;
        let mut i: usize = 0;
        // for (units) |unit| {
        for &(uname, ufactor) in DURATION_UNITS {
            // if (value >= unit.factor) {
            if value >= ufactor {
                // if (i > 0) writer.writeAll(" ") catch unreachable;
                if i > 0 {
                    buf.push(b' ');
                }
                // const remainder = value % unit.factor;
                let remainder = value % ufactor;
                // const quotient = (value - remainder) / unit.factor;
                let quotient = (value - remainder) / ufactor;
                // writer.print("{d}{s}", .{ quotient, unit.name }) catch unreachable;
                buf.extend_from_slice(format!("{quotient}").as_bytes());
                buf.extend_from_slice(uname);
                // value = remainder;
                value = remainder;
                // i += 1;
                i += 1;
            }
        }
    }

    // pub fn cval(self: Duration) usize
    pub fn cval(self: Duration) -> usize {
        // return @intCast(self.asMilliseconds());
        self.as_milliseconds() as usize
    }

    /// Convenience function to convert to milliseconds since many OS and
    /// library timing functions operate on that timescale.
    // pub fn asMilliseconds(self: Duration) c_uint
    pub fn as_milliseconds(self: Duration) -> std::os::raw::c_uint {
        // const ms: u64 = std.math.divTrunc(u64, self.duration, std.time.ns_per_ms) catch std.math.maxInt(c_uint);
        // PORT NOTE: `divTrunc` only errors on divide-by-zero / signed overflow,
        // neither possible here (constant non-zero divisor, unsigned); the
        // `catch` is unreachable, matching the reference's effective behavior.
        let ms: u64 = self.duration / NS_PER_MS;
        // return std.math.cast(c_uint, ms) orelse std.math.maxInt(c_uint);
        std::os::raw::c_uint::try_from(ms).unwrap_or(std::os::raw::c_uint::MAX)
    }
}

impl FormatEntry for Duration {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        Duration::format_entry(self, name, writer)
    }
}

/// PORTS: `std.ascii.isWhitespace(c)` — `{ ' ', '\t', '\n', '\r', VT, FF }`.
fn is_ascii_whitespace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// PORTS: `std.fmt.parseUnsigned(u64, buf, 10)`, through the single port
/// (`rustty_zigstd::fmt`, NEW-FINDINGS N120).
//
// PORT NOTE: the former copy rejected every `_`, where `parseUnsigned` skips an
// interior one. The only caller (`Duration.parseCLI`'s longest-number loop)
// stops at the first prefix that fails to parse, and a prefix ending in `_`
// always fails, so no `_` ever reaches a successful parse: the two agree on
// every input that caller can pass.
fn parse_unsigned_u64_base10(buf: &[u8]) -> Result<u64, ParseIntError> {
    rustty_zigstd::fmt::parse_unsigned::<u64>(buf, 10)
}

// pub const WindowPadding = struct { top_left: u32 = 0, bottom_right: u32 = 0 };
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowPadding {
    pub top_left: u32,
    pub bottom_right: u32,
}

impl WindowPadding {
    // pub fn clone(self: Self, _: Allocator) error{}!Self
    pub fn clone_in(&self) -> WindowPadding {
        // return self;
        *self
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(self: WindowPadding, other: WindowPadding) -> bool {
        // return std.meta.eql(self, other);
        self == other
    }

    // pub fn parseCLI(input_: ?[]const u8) !WindowPadding
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<WindowPadding, ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // const whitespace = " \t";

        // if (std.mem.indexOf(u8, input, ",")) |idx| {
        if let Some(idx) = index_of(input, b',') {
            // const input_left = std.mem.trim(u8, input[0..idx], whitespace);
            let input_left = trim_ws(&input[0..idx]);
            // const input_right = std.mem.trim(u8, input[idx + 1 ..], whitespace);
            let input_right = trim_ws(&input[idx + 1..]);
            // const left = std.fmt.parseInt(u32, input_left, 10) catch return error.InvalidValue;
            let left = parse_int_u32_base10(input_left).map_err(|_| Error::InvalidValue)?;
            // const right = std.fmt.parseInt(u32, input_right, 10) catch return error.InvalidValue;
            let right = parse_int_u32_base10(input_right).map_err(|_| Error::InvalidValue)?;
            // return .{ .top_left = left, .bottom_right = right };
            Ok(WindowPadding {
                top_left: left,
                bottom_right: right,
            })
        } else {
            // const value = std.fmt.parseInt(u32, std.mem.trim(u8, input, whitespace), 10) catch return error.InvalidValue;
            let value = parse_int_u32_base10(trim_ws(input)).map_err(|_| Error::InvalidValue)?;
            // return .{ .top_left = value, .bottom_right = value };
            Ok(WindowPadding {
                top_left: value,
                bottom_right: value,
            })
        }
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // var buf: [128]u8 = undefined;
        // if (self.top_left == self.bottom_right) {
        //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "{}", .{self.top_left}) catch ...);
        // } else {
        //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "{},{}", .{ self.top_left, self.bottom_right }) catch ...);
        // }
        if self.top_left == self.bottom_right {
            let s = format!("{}", self.top_left);
            format_entry_string(name, s.as_bytes(), writer)
        } else {
            let s = format!("{},{}", self.top_left, self.bottom_right);
            format_entry_string(name, s.as_bytes(), writer)
        }
    }
}

impl FormatEntry for WindowPadding {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        WindowPadding::format_entry(self, name, writer)
    }
}
