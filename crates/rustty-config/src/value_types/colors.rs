//! PORTS: ghostty src/config/Config.zig — the color value types: `Color`,
//! `TerminalColor`, `BoldColor`, `ColorList` and `Palette`.

use super::*;

/// Color represents a color using RGB.
///
/// This is a packed struct so that the C API to read color values just
/// works by setting it to a C integer.
// pub const Color = struct { r: u8, g: u8, b: u8 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// ghostty_config_color_s -> rustty_config_color_s (R2)
// pub const C = extern struct { r: u8, g: u8, b: u8 };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct ColorC {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    // pub fn cval(self: Color) Color.C
    pub fn cval(self: Color) -> ColorC {
        // return .{ .r = self.r, .g = self.g, .b = self.b };
        ColorC {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }

    /// Convert this to the terminal RGB struct
    // pub fn toTerminalRGB(self: Color) terminal.color.RGB
    pub fn to_terminal_rgb(self: Color) -> rustty_vt::color::RGB {
        // return .{ .r = self.r, .g = self.g, .b = self.b };
        rustty_vt::color::RGB {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }

    // pub fn parseCLI(input_: ?[]const u8) !Color
    // PORT NOTE (oracle 2970e9a2a Config.zig hunk): delegates to the shared
    // `terminal.color.RGB.parse` so CLI/config and the C ABI stay in lockstep
    // (was: local trim + x11-map lookup + fromHex).
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<Color, ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // const rgb: terminal.color.RGB = terminal.color.RGB.parse(input) catch return error.InvalidValue;
        //
        // PORT NOTE: `RGB::parse` takes `&[u8]`, matching the reference's
        // `parse(value: []const u8)`, so the CLI bytes are passed through
        // unconverted; non-UTF-8 input has no valid parse in any accepted form
        // (hex/name/rgb:) and maps to InvalidValue, as in the reference.
        let rgb = rustty_vt::color::RGB::parse(input).map_err(|_| Error::InvalidValue)?;
        // return .{ .r = rgb.r, .g = rgb.g, .b = rgb.b };
        Ok(Color {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        })
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: Color, _: Allocator) error{}!Color
    pub fn clone_in(self: Color) -> Color {
        self
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Color, other: Color) bool
    pub fn equal(self: Color, other: Color) -> bool {
        self == other
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Color, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        self: Color,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // var buf: [128]u8 = undefined;
        // try formatter.formatEntry([]const u8, try self.formatBuf(&buf));
        let mut buf = [0u8; 128];
        let s = self.format_buf(&mut buf);
        format_entry_string(name, &s, writer)
    }

    /// Format the color as a string.
    // pub fn formatBuf(self: Color, buf: []u8) Allocator.Error![]const u8
    pub fn format_buf(self: Color, _buf: &mut [u8]) -> Vec<u8> {
        // return std.fmt.bufPrint(buf, "#{x:0>2}{x:0>2}{x:0>2}", .{ self.r, self.g, self.b })
        //     catch error.OutOfMemory;
        //
        // PORT NOTE: the reference writes into the caller `buf`; the rustty
        // EntryFormatter consumes a byte slice, so we return an owned `Vec<u8>`
        // carrying the same bytes (`#rrggbb`, lowercase, zero-padded). `_buf` is
        // retained for call-site parity.
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b).into_bytes()
    }

    // PORT NOTE (oracle 2970e9a2a): `fromHex` was deleted upstream — `parseCLI`
    // now delegates to `terminal.color.RGB.parse`, which accepts the same hex
    // forms (with/without leading '#', 3 or 6 digits) plus more. See
    // `test "parseCLI hex"` below (was `test "fromHex"`).
}

impl FormatEntry for Color {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        Color::format_entry(*self, name, writer)
    }
}

/// Represents color values that can also reference special color
/// values such as "cell-foreground" or "cell-background".
// pub const TerminalColor = union(enum) { color: Color, "cell-foreground", "cell-background" };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalColor {
    Color(Color),
    CellForeground,
    CellBackground,
}

impl TerminalColor {
    // pub fn parseCLI(input_: ?[]const u8) !TerminalColor
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<TerminalColor, ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // if (std.mem.eql(u8, input, "cell-foreground")) return .@"cell-foreground";
        if input == b"cell-foreground" {
            return Ok(TerminalColor::CellForeground);
        }
        // if (std.mem.eql(u8, input, "cell-background")) return .@"cell-background";
        if input == b"cell-background" {
            return Ok(TerminalColor::CellBackground);
        }
        // return .{ .color = try Color.parseCLI(input) };
        Ok(TerminalColor::Color(Color::parse_cli(Some(input))?))
    }

    // pub fn toTerminalRGB(self: TerminalColor) ?terminal.color.RGB
    pub fn to_terminal_rgb(self: TerminalColor) -> Option<rustty_vt::color::RGB> {
        // return switch (self) {
        //     .color => |v| v.toTerminalRGB(),
        //     .@"cell-foreground", .@"cell-background" => null,
        // };
        match self {
            TerminalColor::Color(v) => Some(v.to_terminal_rgb()),
            TerminalColor::CellForeground | TerminalColor::CellBackground => None,
        }
    }

    /// Used by Formatter
    // pub fn formatEntry(self: TerminalColor, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        self: TerminalColor,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // switch (self) {
        match self {
            // .color => try self.color.formatEntry(formatter),
            TerminalColor::Color(v) => v.format_entry(name, writer),
            // .@"cell-foreground", .@"cell-background"
            // => try formatter.formatEntry([:0]const u8, @tagName(self)),
            TerminalColor::CellForeground => format_entry_string(name, b"cell-foreground", writer),
            TerminalColor::CellBackground => format_entry_string(name, b"cell-background", writer),
        }
    }
}

impl FormatEntry for TerminalColor {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        TerminalColor::format_entry(*self, name, writer)
    }
}

/// Represents color values that can be used for bold. See `bold-color`.
// pub const BoldColor = union(enum) { color: Color, bright };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoldColor {
    Color(Color),
    Bright,
}

impl BoldColor {
    /// Convert to the terminal-native BoldColor type.
    // pub fn toTerminal(self: BoldColor) terminal.style.Style.BoldColor
    pub fn to_terminal(self: BoldColor) -> rustty_vt::style::BoldColor {
        // return switch (self) {
        //     .color => |col| .{ .color = col.toTerminalRGB() },
        //     .bright => .bright,
        // };
        match self {
            BoldColor::Color(col) => rustty_vt::style::BoldColor::Color(col.to_terminal_rgb()),
            BoldColor::Bright => rustty_vt::style::BoldColor::Bright,
        }
    }

    // pub fn parseCLI(input_: ?[]const u8) !BoldColor
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<BoldColor, ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // if (std.mem.eql(u8, input, "bright")) return .bright;
        if input == b"bright" {
            return Ok(BoldColor::Bright);
        }
        // return .{ .color = try Color.parseCLI(input) };
        Ok(BoldColor::Color(Color::parse_cli(Some(input))?))
    }

    /// Used by Formatter
    // pub fn formatEntry(self: BoldColor, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        self: BoldColor,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // switch (self) {
        match self {
            // .color => try self.color.formatEntry(formatter),
            BoldColor::Color(v) => v.format_entry(name, writer),
            // .bright => try formatter.formatEntry([:0]const u8, @tagName(self)),
            BoldColor::Bright => format_entry_string(name, b"bright", writer),
        }
    }
}

impl FormatEntry for BoldColor {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        BoldColor::format_entry(*self, name, writer)
    }
}

// ===========================================================================
// Repeatable/list and aggregate structs (with method sigs).
// ===========================================================================

// pub const ColorList = struct { colors: ArrayListUnmanaged(Color), colors_c: ArrayListUnmanaged(Color.C) };
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorList {
    pub colors: Vec<Color>,
    pub colors_c: Vec<ColorC>,
}

/// ghostty_config_color_list_s -> rustty_config_color_list_s (R2)
// pub const C = extern struct { colors: [*]Color.C, len: usize };
#[repr(C)]
pub struct ColorListC {
    pub colors: *const ColorC,
    pub len: usize,
}

impl ColorList {
    // pub fn cval(self: *const Self) C
    pub fn cval(&self) -> ColorListC {
        // return .{ .colors = self.colors_c.items.ptr, .len = self.colors_c.items.len };
        ColorListC {
            colors: self.colors_c.as_ptr(),
            len: self.colors_c.len(),
        }
    }

    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // if (input.len == 0) return error.ValueRequired;
        if input.is_empty() {
            return Err(Error::ValueRequired);
        }

        // Always reset on parse
        // self.* = .{};
        *self = ColorList::default();

        // Split the input by commas and parse each color
        // var it = std.mem.tokenizeScalar(u8, input, ',');
        // var count: usize = 0;
        // while (it.next()) |raw| {
        let mut count: usize = 0;
        for raw in tokenize_scalar(input, b',') {
            // count += 1;
            count += 1;
            // if (count > 64) return error.InvalidValue;
            if count > 64 {
                return Err(Error::InvalidValue);
            }

            // Trim whitespace from each color value
            // const trimmed = std.mem.trim(u8, raw, " \t");
            let trimmed = trim_ws(raw);
            // const color = try Color.parseCLI(trimmed);
            let color = Color::parse_cli(Some(trimmed))?;
            // try self.colors.append(alloc, color);
            self.colors.push(color);
            // try self.colors_c.append(alloc, color.cval());
            self.colors_c.push(color.cval());
        }

        // If no colors were parsed, we need to return an error
        // if (self.colors.items.len == 0) return error.InvalidValue;
        if self.colors.is_empty() {
            return Err(Error::InvalidValue);
        }

        // assert(self.colors.items.len == self.colors_c.items.len);
        assert_eq!(self.colors.len(), self.colors_c.len());
        Ok(())
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> ColorList {
        // return .{ .colors = try self.colors.clone(alloc), .colors_c = try self.colors_c.clone(alloc) };
        ColorList {
            colors: self.colors.clone(),
            colors_c: self.colors_c.clone(),
        }
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &ColorList) -> bool {
        // const itemsA = self.colors.items;
        // const itemsB = other.colors.items;
        let items_a = &self.colors;
        let items_b = &other.colors;
        // if (itemsA.len != itemsB.len) return false;
        if items_a.len() != items_b.len() {
            return false;
        }
        // for (itemsA, itemsB) |a, b| { if (!a.equal(b)) return false; } else return true;
        for (a, b) in items_a.iter().zip(items_b.iter()) {
            if !a.equal(*b) {
                return false;
            }
        }
        true
    }

    // pub fn formatEntry(self: Self, formatter: anytype) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // If no items, we want to render an empty field.
        // if (self.colors.items.len == 0) { try formatter.formatEntry(void, {}); return; }
        if self.colors.is_empty() {
            return format_entry_void(name, writer);
        }

        // Build up the value of our config. Our buffer size should be
        // sized to contain all possible maximum values.
        // var buf: [1024]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        let mut buf: Vec<u8> = Vec::new();
        // for (self.colors.items, 0..) |color, i| {
        for (i, color) in self.colors.iter().enumerate() {
            // var color_buf: [128]u8 = undefined;
            let mut color_buf = [0u8; 128];
            // const color_str = try color.formatBuf(&color_buf);
            let color_str = color.format_buf(&mut color_buf);
            // if (i != 0) writer.writeByte(',') catch return error.OutOfMemory;
            if i != 0 {
                buf.push(b',');
            }
            // writer.writeAll(color_str) catch return error.OutOfMemory;
            buf.extend_from_slice(&color_str);
        }

        // try formatter.formatEntry([]const u8, writer.buffered());
        format_entry_string(name, &buf, writer)
    }
}

impl FormatEntry for ColorList {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        ColorList::format_entry(self, name, writer)
    }
}

/// PORTS: `std.mem.tokenizeScalar(u8, input, delim)` — yields non-empty
/// delimiter-separated tokens (consecutive delimiters and leading/trailing
/// delimiters produce no empty tokens), in order.
fn tokenize_scalar(input: &[u8], delim: u8) -> impl Iterator<Item = &[u8]> {
    input.split(move |&c| c == delim).filter(|s| !s.is_empty())
}

/// Palette is the 256 color palette for 256-color mode.
// pub const Palette = struct { value: terminal.color.Palette = default, mask: PaletteMask = initEmpty };
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// The actual value that is updated as we parse.
    pub value: rustty_vt::color::Palette,
    /// Keep track of which indexes were manually set by the user.
    pub mask: rustty_vt::color::PaletteMask,
}

impl Default for Palette {
    fn default() -> Self {
        // value: terminal.color.default, mask: .initEmpty()
        Self {
            value: rustty_vt::color::DEFAULT,
            mask: rustty_vt::color::PaletteMask::default(),
        }
    }
}

/// ghostty_config_palette_s -> rustty_config_palette_s (R2)
// pub const C = extern struct { colors: [256]Color.C };
#[repr(C)]
pub struct PaletteC {
    pub colors: [ColorC; 256],
}

/// PORTS: the error set of `Palette.parseCLI` — `parseInt`'s `error.Overflow`/
/// `error.InvalidCharacter` plus `error.InvalidValue`/`error.ValueRequired`. The
/// reference's `parseCLI` is `!void`; the distinguishable `error.Overflow` is
/// asserted by `test "parseCLI overflow"`, so it is preserved here (the base
/// args `Error` set has no Overflow variant).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteParseError {
    ValueRequired,
    InvalidValue,
    Overflow,
    InvalidCharacter,
}

impl Palette {
    // pub fn cval(self: Self) Palette.C
    pub fn cval(&self) -> PaletteC {
        // var result: Palette.C = undefined;
        // for (self.value, 0..) |color, i| {
        //     result.colors[i] = Color.C{ .r = color.r, .g = color.g, .b = color.b };
        // }
        // return result;
        let mut colors = [ColorC { r: 0, g: 0, b: 0 }; 256];
        for (i, color) in self.value.iter().enumerate() {
            colors[i] = ColorC {
                r: color.r,
                g: color.g,
                b: color.b,
            };
        }
        PaletteC { colors }
    }

    // pub fn parseCLI(self: *Self, input: ?[]const u8) !void
    // PORT NOTE (oracle 2970e9a2a Config.zig hunk): delegates to the shared
    // `terminal.color.parsePaletteEntry` so CLI/config and the C ABI stay in
    // lockstep (was: local `=`-split + parseInt + Color.parseCLI).
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), PaletteParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(PaletteParseError::ValueRequired)?;
        // const entry = terminal.color.parsePaletteEntry(value) catch |err| switch (err) {
        //     error.Overflow => return error.Overflow,
        //     error.InvalidFormat => return error.InvalidValue,
        // };
        let entry = rustty_vt::color::parse_palette_entry(value).map_err(|e| match e {
            rustty_vt::color::ParsePaletteEntryError::Overflow => PaletteParseError::Overflow,
            rustty_vt::color::ParsePaletteEntryError::InvalidFormat => {
                PaletteParseError::InvalidValue
            }
        })?;
        // self.value[entry.index] = entry.color;
        self.value[entry.index as usize] = entry.color;
        // self.mask.set(entry.index);
        self.mask.set(entry.index as usize);
        Ok(())
    }

    // pub fn clone(self: Self, _: Allocator) error{}!Self
    pub fn clone_in(&self) -> Palette {
        self.clone()
    }

    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &Palette) -> bool {
        // return std.meta.eql(self, other);
        self == other
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // var buf: [128]u8 = undefined;
        // for (0.., self.value) |k, v| {
        //     try formatter.formatEntry([]const u8,
        //         std.fmt.bufPrint(&buf, "{d}=#{x:0>2}{x:0>2}{x:0>2}", .{ k, v.r, v.g, v.b })
        //             catch return error.OutOfMemory);
        // }
        for (k, v) in self.value.iter().enumerate() {
            let s = format!("{}=#{:02x}{:02x}{:02x}", k, v.r, v.g, v.b);
            format_entry_string(name, s.as_bytes(), writer)?;
        }
        Ok(())
    }
}

impl FormatEntry for Palette {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        Palette::format_entry(self, name, writer)
    }
}

/// PORTS: `std.mem.indexOf(u8, haystack, &[_]u8{needle})` (single-byte needle).
pub(super) fn index_of(haystack: &[u8], needle: u8) -> Option<usize> {
    haystack.iter().position(|&c| c == needle)
}
