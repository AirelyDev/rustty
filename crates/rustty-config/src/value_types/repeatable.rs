//! PORTS: ghostty src/config/Config.zig — the repeatable/list value types:
//! `RepeatableString`, `SelectionWordChars`, `RepeatableFontVariation`,
//! `RepeatableCodepointMap`, `RepeatableClipboardCodepointMap`, `RepeatableLink`
//! and `RepeatableCommand`.

use super::*;

/// RepeatableString is a string value that can be repeated to accumulate
/// a list of strings. This isn't called "StringList" because I find that
/// sometimes leads to confusion that it _accepts_ a list such as
/// comma-separated values.
// pub const RepeatableString = struct { list: ArrayListUnmanaged([:0]const u8), overwrite_next: bool = false };
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepeatableString {
    // Allocator for the list is the arena for the parent config.
    pub list: Vec<Vec<u8>>,
    // If true, then the next value will clear the list and start over
    // rather than append. This is a bit of a hack but is here to make
    // the font-family set of configurations work with CLI parsing.
    pub overwrite_next: bool,
}

impl RepeatableString {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(Error::ValueRequired)?;

        // Empty value resets the list
        // if (value.len == 0) { self.list.clearRetainingCapacity(); return; }
        if value.is_empty() {
            self.list.clear();
            return Ok(());
        }

        // If we're overwriting then we clear before appending
        // if (self.overwrite_next) {
        //     self.list.clearRetainingCapacity();
        //     self.overwrite_next = false;
        // }
        if self.overwrite_next {
            self.list.clear();
            self.overwrite_next = false;
        }

        // const copy = try alloc.dupeZ(u8, value);
        // try self.list.append(alloc, copy);
        self.list.push(value.to_vec());
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableString {
        // Copy the list and all the strings in the list.
        // var list = try std.ArrayListUnmanaged([:0]const u8).initCapacity(alloc, self.list.items.len);
        // for (self.list.items) |item| { const copy = try alloc.dupeZ(u8, item); list.appendAssumeCapacity(copy); }
        // return .{ .list = list };
        RepeatableString {
            list: self.list.clone(),
            overwrite_next: false,
        }
    }

    /// The number of items in the list
    // pub fn count(self: Self) usize
    pub fn count(&self) -> usize {
        // return self.list.items.len;
        self.list.len()
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &RepeatableString) -> bool {
        // const itemsA = self.list.items;
        // const itemsB = other.list.items;
        // if (itemsA.len != itemsB.len) return false;
        if self.list.len() != other.list.len() {
            return false;
        }
        // for (itemsA, itemsB) |a, b| { if (!std.mem.eql(u8, a, b)) return false; } else return true;
        for (a, b) in self.list.iter().zip(other.list.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // If no items, we want to render an empty field.
        // if (self.list.items.len == 0) { try formatter.formatEntry(void, {}); return; }
        if self.list.is_empty() {
            return format_entry_void(name, writer);
        }

        // for (self.list.items) |value| { try formatter.formatEntry([]const u8, value); }
        for value in &self.list {
            format_entry_string(name, value, writer)?;
        }
        Ok(())
    }
}

impl FormatEntry for RepeatableString {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        RepeatableString::format_entry(self, name, writer)
    }
}

/// SelectionWordChars stores the parsed codepoints for word boundary
/// characters used during text selection. The string is parsed once
/// during configuration and stored as u21 codepoints for efficient
/// lookup during selection operations.
// pub const SelectionWordChars = struct { codepoints: []const u21 = &default_word_boundaries };
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionWordChars {
    /// The parsed codepoints. Always includes null (U+0000) at index 0.
    pub codepoints: Vec<u32>,
}

impl Default for SelectionWordChars {
    fn default() -> Self {
        Self {
            codepoints: rustty_vt::selection_codepoints::DEFAULT_WORD_BOUNDARIES.to_vec(),
        }
    }
}

impl SelectionWordChars {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(Error::ValueRequired)?;

        // Parse string with Zig escape sequence support into codepoints
        // var list: std.ArrayList(u21) = .empty;
        let mut list: Vec<u32> = Vec::new();

        // Always include null as first boundary
        // try list.append(alloc, 0);
        list.push(0);

        // var it = string.codepointIterator(value);
        // while (it.next() catch return error.InvalidValue) |codepoint| {
        //     try list.append(alloc, codepoint);
        // }
        let mut it = crate::string::codepoint_iterator(value);
        loop {
            match it.next() {
                Ok(Some(codepoint)) => list.push(codepoint),
                Ok(None) => break,
                Err(_) => return Err(Error::InvalidValue),
            }
        }

        // self.codepoints = try list.toOwnedSlice(alloc);
        self.codepoints = list;
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> SelectionWordChars {
        // const copy = try alloc.dupe(u21, self.codepoints);
        // return .{ .codepoints = copy };
        SelectionWordChars {
            codepoints: self.codepoints.clone(),
        }
    }

    /// Compare if two values are equal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &SelectionWordChars) -> bool {
        // return std.mem.eql(u21, self.codepoints, other.codepoints);
        self.codepoints == other.codepoints
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // Convert codepoints back to UTF-8 string for display
        // var buf: [4096]u8 = undefined;
        // var pos: usize = 0;
        let mut buf = [0u8; 4096];
        let mut pos: usize = 0;

        // Skip the null character at index 0
        // for (self.codepoints[1..]) |codepoint| {
        for &codepoint in &self.codepoints[1..] {
            // var utf8_buf: [4]u8 = undefined;
            // const len = std.unicode.utf8Encode(codepoint, &utf8_buf) catch continue;
            let mut utf8_buf = [0u8; 4];
            let ch = match char::from_u32(codepoint) {
                Some(c) => c,
                None => continue,
            };
            let len = ch.encode_utf8(&mut utf8_buf).len();
            // if (pos + len > buf.len) break;
            if pos + len > buf.len() {
                break;
            }
            // @memcpy(buf[pos..][0..len], utf8_buf[0..len]);
            buf[pos..pos + len].copy_from_slice(&utf8_buf[0..len]);
            // pos += len;
            pos += len;
        }

        // try formatter.formatEntry([]const u8, buf[0..pos]);
        format_entry_string(name, &buf[0..pos], writer)
    }
}

impl FormatEntry for SelectionWordChars {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        SelectionWordChars::format_entry(self, name, writer)
    }
}

/// FontVariation is a repeatable configuration value that sets a single
/// font variation value. Font variations are configurations for what
/// are often called "variable fonts." The font files usually end in
/// "-VF.ttf."
///
/// The value for this is in the format of `id=value` where `id` is the
/// 4-character font variation axis identifier and `value` is the
/// floating point value for that axis. For more details on font variations
/// see the MDN font-variation-settings documentation since this copies that
/// behavior almost exactly:
///
/// https://developer.mozilla.org/en-US/docs/Web/CSS/font-variation-settings
// pub const RepeatableFontVariation = struct { list: ArrayListUnmanaged(fontpkg.face.Variation) };
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RepeatableFontVariation {
    // Allocator for the list is the arena for the parent config.
    pub list: Vec<FaceVariation>,
}

impl RepeatableFontVariation {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // const eql_idx = std.mem.indexOf(u8, input, "=") orelse return error.InvalidValue;
        let eql_idx = index_of(input, b'=').ok_or(Error::InvalidValue)?;
        // const whitespace = " \t";
        // const key = std.mem.trim(u8, input[0..eql_idx], whitespace);
        let key = trim_ws(&input[0..eql_idx]);
        // const value = std.mem.trim(u8, input[eql_idx + 1 ..], whitespace);
        let value = trim_ws(&input[eql_idx + 1..]);
        // if (key.len != 4) return error.InvalidValue;
        if key.len() != 4 {
            return Err(Error::InvalidValue);
        }
        // try self.list.append(alloc, .{
        //     .id = fontpkg.face.Variation.Id.init(@ptrCast(key.ptr)),
        //     .value = std.fmt.parseFloat(f64, value) catch return error.InvalidValue,
        // });
        let key4: [u8; 4] = [key[0], key[1], key[2], key[3]];
        self.list.push(FaceVariation {
            id: face_variation_id_init(&key4),
            value: parse_float_f64(value).map_err(|_| Error::InvalidValue)?,
        });
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableFontVariation {
        // return .{ .list = try self.list.clone(alloc) };
        RepeatableFontVariation {
            list: self.list.clone(),
        }
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &RepeatableFontVariation) -> bool {
        // const itemsA = self.list.items;
        // const itemsB = other.list.items;
        // if (itemsA.len != itemsB.len) return false;
        if self.list.len() != other.list.len() {
            return false;
        }
        // for (itemsA, itemsB) |a, b| { if (!std.meta.eql(a, b)) return false; } else return true;
        for (a, b) in self.list.iter().zip(other.list.iter()) {
            if a != b {
                return false;
            }
        }
        true
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: anytype) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // if (self.list.items.len == 0) { try formatter.formatEntry(void, {}); return; }
        if self.list.is_empty() {
            return format_entry_void(name, writer);
        }

        // var buf: [128]u8 = undefined;
        // for (self.list.items) |value| {
        //     const str = std.fmt.bufPrint(&buf, "{s}={d}", .{ value.id.str(), value.value }) catch ...;
        //     try formatter.formatEntry([]const u8, str);
        // }
        for value in &self.list {
            let id_str = face_variation_id_str(value.id);
            let mut s: Vec<u8> = Vec::new();
            s.extend_from_slice(&id_str);
            s.push(b'=');
            s.extend_from_slice(fmt_float(value.value).as_bytes());
            format_entry_string(name, &s, writer)?;
        }
        Ok(())
    }
}

impl FormatEntry for RepeatableFontVariation {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        RepeatableFontVariation::format_entry(self, name, writer)
    }
}

// pub const RepeatableCodepointMap = struct { map: fontpkg.CodepointMap };
#[derive(Debug, Default)]
pub struct RepeatableCodepointMap {
    pub map: CodepointMap,
}

// PORT NOTE (R4): `RepeatableCodepointMap` wraps `fontpkg.CodepointMap`
// (`font/CodepointMap.zig`), ported PURE in rustty-font. Its `parseCLI`/`clone`/
// `equal`/`formatEntry` operate on that font map's `add`/`entries`/`descriptor`
// API. The shared `UnicodeRangeParser` is hoisted to module scope (also consumed
// by `RepeatableClipboardCodepointMap`). rustty-font's `CodepointMap` carries no
// C/vt edge (the fontconfig/CoreText discovery lands later in the shaper tier);
// the discovery-descriptor seam here only stores `family` + `monospace`.
impl RepeatableCodepointMap {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        use rustty_font::codepoint_map::{Descriptor, Entry, Global};
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // const eql_idx = std.mem.indexOf(u8, input, "=") orelse return error.InvalidValue;
        let eql_idx = index_of(input, b'=').ok_or(Error::InvalidValue)?;
        // const whitespace = " \t";
        // const key = std.mem.trim(u8, input[0..eql_idx], whitespace);
        let key = trim_ws(&input[0..eql_idx]);
        // const value = std.mem.trim(u8, input[eql_idx + 1 ..], whitespace);
        let value = trim_ws(&input[eql_idx + 1..]);
        // const valueZ = try alloc.dupeZ(u8, value);
        //
        // PORT NOTE: the reference dupes `value` once and shares the SAME `valueZ`
        // pointer across every range's descriptor. The seam's `family` is an owned
        // `Vec<u8>` (byte-slice fidelity — no sentinel), so each entry gets its own
        // content-identical copy; the shared-pointer identity is unobservable here
        // (it only fed `std.meta.eql`'s pointer compare in `equal`, ported by
        // content below).
        let value_z: std::vec::Vec<u8> = value.to_vec();

        // var p: UnicodeRangeParser = .{ .input = key };
        let mut p = UnicodeRangeParser { input: key, i: 0 };
        // while (try p.next()) |range| {
        while let Some(range) = p.next()? {
            // try self.map.add(alloc, .{
            //     .range = range,
            //     .descriptor = .{ .family = valueZ, .monospace = false },
            // });
            //
            // PORT NOTE: `add` returns `OutOfMemory`; per the config allocation
            // model (Global aborts on OOM — `OutOfMemory` is not a recoverable
            // config error), this is `.expect`-ed rather than surfaced.
            self.map
                .add(
                    Global,
                    Entry {
                        range,
                        descriptor: Descriptor {
                            family: Some(value_z.clone()),
                            // we allow any font
                            monospace: false,
                        },
                    },
                )
                .expect("CodepointMap.add: Global allocator aborts on OOM");
        }
        Ok(())
    }

    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self {
    //     return .{ .map = try self.map.clone(alloc) };
    // }
    //
    // PORT NOTE: the reference returns a deep copy of `self.map` (the font
    // `CodepointMap`). Required by `Config.clone` (`cloneValue`, dispatched via
    // the `ci!`/`@hasDecl(T, "clone")` arm). The font map's `clone` returns
    // `OutOfMemory`; per the config allocation model (Global aborts on OOM) it is
    // `.expect`-ed, matching every other infallible `clone_in`.
    pub fn clone_in(&self) -> RepeatableCodepointMap {
        use rustty_font::codepoint_map::Global;
        // return .{ .map = try self.map.clone(alloc) };
        RepeatableCodepointMap {
            map: self
                .map
                .clone(Global)
                .expect("CodepointMap.clone: Global allocator aborts on OOM"),
        }
    }

    // pub fn equal(self: Self, other: Self) bool {
    //     const itemsA = self.map.list.slice();
    //     const itemsB = other.map.list.slice();
    //     if (itemsA.len != itemsB.len) return false;
    //     for (0..itemsA.len) |i| {
    //         const a = itemsA.get(i);
    //         const b = itemsB.get(i);
    //         if (!std.meta.eql(a, b)) return false;
    //     } else return true;
    // }
    //
    // PORT NOTE: the reference compares the two maps' entry lists (length, then
    // `std.meta.eql` per entry). Required by `Config.changed` (`deepEqual`).
    //
    // std.meta.eql on the `Entry`/`Descriptor` structs compares `range` element-
    // wise, `descriptor.monospace` directly, and `descriptor.family` (a slice) by
    // POINTER+LEN identity — NOT by content. Reproducing Zig's slice-pointer
    // identity across the port is neither possible nor meaningful (the seam's
    // `family` is an owned `Vec<u8>`); we compare `family` by CONTENT, which is the
    // semantically-intended equality for `changed`/`deepEqual` (two configs with
    // the same codepoint map should compare equal). This is a deliberate,
    // documented divergence from `std.meta.eql`'s pointer semantics.
    pub fn equal(&self, other: &RepeatableCodepointMap) -> bool {
        // const itemsA = self.map.list.slice();
        // const itemsB = other.map.list.slice();
        let items_a = self.map.entries();
        let items_b = other.map.entries();
        // if (itemsA.len != itemsB.len) return false;
        if items_a.len() != items_b.len() {
            return false;
        }
        // for (0..itemsA.len) |i| {
        for i in 0..items_a.len() {
            // const a = itemsA.get(i);
            // const b = itemsB.get(i);
            let a = &items_a[i];
            let b = &items_b[i];
            // if (!std.meta.eql(a, b)) return false;
            if a.range != b.range
                || a.descriptor.family != b.descriptor.family
                || a.descriptor.monospace != b.descriptor.monospace
            {
                return false;
            }
        }
        // } else return true;
        true
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // if (self.map.list.len == 0) {
        if self.map.is_empty() {
            // try formatter.formatEntry(void, {});
            // return;
            return format_entry_void(name, writer);
        }

        // var buf: [1024]u8 = undefined;
        // const ranges = self.map.list.items(.range);
        // const descriptors = self.map.list.items(.descriptor);
        // for (ranges, descriptors) |range, descriptor| {
        for entry in self.map.entries() {
            let range = entry.range;
            // descriptor.family orelse ""
            let family: &[u8] = entry.descriptor.family.as_deref().unwrap_or(b"");

            // if (range[0] == range[1]) {
            //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "U+{X:0>4}={s}", .{ range[0], descriptor.family orelse "" }) catch ...);
            // } else {
            //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "U+{X:0>4}-U+{X:0>4}={s}", .{ range[0], range[1], descriptor.family orelse "" }) catch ...);
            // }
            // PORT NOTE (fidelity): each entry is serialized through the
            // reference's FIXED `var buf: [1024]u8`, so a `descriptor.family`
            // long enough to push the rendered entry past 1024 bytes makes
            // `bufPrint` fail with `error.NoSpaceLeft`, which the reference
            // catches and returns as `error.OutOfMemory` — the whole config
            // dump fails. A growable `Vec` here silently accepted those
            // overlong entries; the fixed scratch reproduces the reference
            // bound exactly, matching the sibling
            // `RepeatableClipboardCodepointMap::format_entry` treatment
            // (boundary: 1017 family bytes for a 4-hex-digit single codepoint,
            // 1010 for the range form).
            let mut buf = [0u8; 1024];
            let prefix = if range[0] == range[1] {
                format!("U+{:04X}=", range[0])
            } else {
                format!("U+{:04X}-U+{:04X}=", range[0], range[1])
            };
            let s = buf_print_parts(&mut buf, &[prefix.as_bytes(), family])
                .ok_or(FormatError::OutOfMemory)?;
            format_entry_string(name, s, writer)?;
        }
        Ok(())
    }
}

impl FormatEntry for RepeatableCodepointMap {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        RepeatableCodepointMap::format_entry(self, name, writer)
    }
}

// pub const RepeatableClipboardCodepointMap = struct { map: ClipboardCodepointMap };
#[derive(Default)]
pub struct RepeatableClipboardCodepointMap {
    pub map: crate::ClipboardCodepointMap,
}

impl RepeatableClipboardCodepointMap {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        use rustty_vt::formatter::Replacement;
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // const eql_idx = std.mem.indexOf(u8, input, "=") orelse return error.InvalidValue;
        let eql_idx = index_of(input, b'=').ok_or(Error::InvalidValue)?;
        // const whitespace = " \t";
        // const key = std.mem.trim(u8, input[0..eql_idx], whitespace);
        let key = trim_ws(&input[0..eql_idx]);
        // const value = std.mem.trim(u8, input[eql_idx + 1 ..], whitespace);
        let value = trim_ws(&input[eql_idx + 1..]);

        // Parse the replacement value - either a codepoint or string
        // const replacement: ClipboardCodepointMap.Replacement = if (std.mem.startsWith(u8, value, "U+")) blk: {
        let replacement: Replacement = if value.starts_with(b"U+") {
            // Parse as codepoint
            // const cp_str = value[2..]; // Skip "U+"
            let cp_str = &value[2..];
            // const cp = std.fmt.parseInt(u21, cp_str, 16) catch return error.InvalidValue;
            let cp = parse_int_u21_base16(cp_str).map_err(|_| Error::InvalidValue)?;
            // break :blk .{ .codepoint = cp };
            Replacement::Codepoint(cp)
        } else {
            // Parse as UTF-8 string - validate it's valid UTF-8
            // if (!std.unicode.utf8ValidateSlice(value)) return error.InvalidValue;
            if core::str::from_utf8(value).is_err() {
                return Err(Error::InvalidValue);
            }
            // const value_copy = try alloc.dupe(u8, value);
            // break :blk .{ .string = value_copy };
            Replacement::String(value.to_vec())
        };

        // var p: UnicodeRangeParser = .{ .input = key };
        // while (try p.next()) |range| {
        //     try self.map.add(alloc, .{ .range = range, .replacement = replacement });
        // }
        let mut p = UnicodeRangeParser { input: key, i: 0 };
        while let Some(range) = p.next()? {
            self.map.add(crate::clipboard_codepoint_map::Entry {
                range,
                // PORT NOTE: `Replacement` is not `Clone`; clone its bytes by hand.
                replacement: match &replacement {
                    Replacement::Codepoint(cp) => Replacement::Codepoint(*cp),
                    Replacement::String(s) => Replacement::String(s.clone()),
                },
            });
        }
        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableClipboardCodepointMap {
        // return .{ .map = try self.map.clone(alloc) };
        RepeatableClipboardCodepointMap {
            map: self.map.clone(),
        }
    }

    /// Compare if two of our value are equal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &RepeatableClipboardCodepointMap) -> bool {
        use rustty_vt::formatter::Replacement;
        // const itemsA = self.map.list.slice();
        // const itemsB = other.map.list.slice();
        let items_a = &self.map.list;
        let items_b = &other.map.list;
        // if (itemsA.len != itemsB.len) return false;
        if items_a.len() != items_b.len() {
            return false;
        }
        // for (0..itemsA.len) |i| {
        for i in 0..items_a.len() {
            // const a = itemsA.get(i);
            // const b = itemsB.get(i);
            let a = &items_a[i];
            let b = &items_b[i];
            // if (!std.meta.eql(a.range, b.range)) return false;
            if a.range != b.range {
                return false;
            }
            // switch (a.replacement) {
            match &a.replacement {
                // .codepoint => |cp_a| switch (b.replacement) {
                //     .codepoint => |cp_b| if (cp_a != cp_b) return false,
                //     .string => return false,
                // },
                Replacement::Codepoint(cp_a) => match &b.replacement {
                    Replacement::Codepoint(cp_b) => {
                        if cp_a != cp_b {
                            return false;
                        }
                    }
                    Replacement::String(_) => return false,
                },
                // .string => |str_a| switch (b.replacement) {
                //     .string => |str_b| if (!std.mem.eql(u8, str_a, str_b)) return false,
                //     .codepoint => return false,
                // },
                Replacement::String(str_a) => match &b.replacement {
                    Replacement::String(str_b) => {
                        if str_a != str_b {
                            return false;
                        }
                    }
                    Replacement::Codepoint(_) => return false,
                },
            }
        }
        // return true;
        true
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: anytype) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        use rustty_vt::formatter::Replacement;
        // if (self.map.list.len == 0) { try formatter.formatEntry(void, {}); return; }
        if self.map.list.is_empty() {
            return format_entry_void(name, writer);
        }

        // var buf: [1024]u8 = undefined;
        // var value_buf: [32]u8 = undefined;
        // const ranges = self.map.list.items(.range);
        // const replacements = self.map.list.items(.replacement);
        // for (ranges, replacements) |range, replacement| {
        for entry in &self.map.list {
            let range = entry.range;
            // const value_str = switch (replacement) {
            //     .codepoint => |cp| try std.fmt.bufPrint(&value_buf, "U+{X:0>4}", .{cp}),
            //     .string => |s| s,
            // };
            let value_str: Vec<u8> = match &entry.replacement {
                Replacement::Codepoint(cp) => format!("U+{:04X}", cp).into_bytes(),
                Replacement::String(s) => s.clone(),
            };

            // if (range[0] == range[1]) {
            //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "U+{X:0>4}={s}", .{ range[0], value_str }) catch return error.OutOfMemory);
            // } else {
            //     try formatter.formatEntry([]const u8, std.fmt.bufPrint(&buf, "U+{X:0>4}-U+{X:0>4}={s}", .{ range[0], range[1], value_str }) catch return error.OutOfMemory);
            // }
            //
            // PORT NOTE (fidelity): each entry is serialized through the
            // reference's FIXED `var buf: [1024]u8`, so a `.string` replacement
            // long enough to push the rendered entry past 1024 bytes makes
            // `bufPrint` fail with `error.NoSpaceLeft`, which the reference
            // catches and returns as `error.OutOfMemory` — the whole config dump
            // fails. A growable `Vec` here silently accepted those overlong
            // entries; the fixed scratch below reproduces the reference bound
            // exactly (boundary: 1017 replacement bytes for a 4-hex-digit single
            // codepoint, 1010 for the range form). `value_buf: [32]u8` needs no
            // such treatment: a `u21` renders at most `U+1FFFFF` (8 bytes).
            let mut buf = [0u8; 1024];
            let prefix = if range[0] == range[1] {
                format!("U+{:04X}=", range[0])
            } else {
                format!("U+{:04X}-U+{:04X}=", range[0], range[1])
            };
            let s = buf_print_parts(&mut buf, &[prefix.as_bytes(), &value_str])
                .ok_or(FormatError::OutOfMemory)?;
            format_entry_string(name, s, writer)?;
        }
        Ok(())
    }
}

impl FormatEntry for RepeatableClipboardCodepointMap {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        RepeatableClipboardCodepointMap::format_entry(self, name, writer)
    }
}

/// PORTS: `std.fmt.bufPrint(&buf, fmt, args)` over already-rendered byte
/// segments: copies `parts` into `buf` in order and returns the written prefix,
/// or `None` for the reference's `error.NoSpaceLeft` when they do not fit. Used
/// where the reference's formatting bound is a fixed stack buffer whose overflow
/// is observable (`catch return error.OutOfMemory`).
pub(super) fn buf_print_parts<'a>(buf: &'a mut [u8], parts: &[&[u8]]) -> Option<&'a [u8]> {
    let mut n: usize = 0;
    for part in parts {
        let end = n.checked_add(part.len())?;
        if end > buf.len() {
            return None;
        }
        buf[n..end].copy_from_slice(part);
        n = end;
    }
    Some(&buf[..n])
}

/// PORTS: `std.fmt.parseInt(u21, str, 16)` — a 21-bit unsigned hex parse.
//
// PORT NOTE (NEW-FINDINGS N120, task tail-s9): Rust has no `u21`, so this calls
// the one port of `std.fmt.parseInt` at the reference's width
// (`rustty_zigstd::fmt::parse_int_type` with `IntType::unsigned(21)`). It was a
// local copy (optional sign, `_` separators, `-` only at magnitude zero) that
// agreed with Zig on every input task tail-s9 probed. All error paths collapse
// to `error.InvalidValue` at the call sites, which do `parseInt(...) catch
// return error.InvalidValue`.
fn parse_int_u21_base16(input: &[u8]) -> Result<u21, ParseError> {
    use rustty_zigstd::fmt::{parse_int_type, IntType};
    const U21: IntType = IntType::unsigned(21);
    parse_int_type(U21, input, 16)
        // A `u21` value always fits the `u32`-backed `u21` alias.
        .map(|v| v as u21)
        .map_err(|_| Error::InvalidValue)
}

/// Parses the list of Unicode codepoint ranges. Valid syntax:
///
///   "" (empty returns null)
///   U+1234
///   U+1234-5678
///   U+1234,U+5678
///   U+1234-5678,U+5678
///   U+1234,U+5678-U+9ABC
///
/// etc.
//
// Reuse the same UnicodeRangeParser from RepeatableCodepointMap
//
// PORT NOTE: this is `RepeatableCodepointMap.UnicodeRangeParser` (the reference
// defines it inside `RepeatableCodepointMap` and `RepeatableClipboardCodepointMap`
// reuses it via `const UnicodeRangeParser = RepeatableCodepointMap.UnicodeRangeParser;`).
// `RepeatableCodepointMap` itself is a deferred DI seam (it needs the not-yet-
// ported `fontpkg.CodepointMap`), so the shared parser is hoisted to module
// scope here and consumed by `RepeatableClipboardCodepointMap`.
// const UnicodeRangeParser = struct { input: []const u8, i: usize = 0 };
struct UnicodeRangeParser<'a> {
    input: &'a [u8],
    i: usize,
}

impl UnicodeRangeParser<'_> {
    // pub fn next(self: *UnicodeRangeParser) !?[2]u21 {
    fn next(&mut self) -> Result<Option<[u21; 2]>, ParseError> {
        // Once we're EOF then we're done without an error.
        // if (self.eof()) return null;
        if self.eof() {
            return Ok(None);
        }

        // One codepoint no matter what
        // const start = try self.parseCodepoint();
        let start = self.parse_codepoint()?;
        // if (self.eof()) return .{ start, start };
        if self.eof() {
            return Ok(Some([start, start]));
        }

        // We're allowed to have any whitespace here
        // self.consumeWhitespace();
        self.consume_whitespace();

        // Otherwise we expect either a range or a comma
        // switch (self.input[self.i]) {
        match self.input[self.i] {
            // Comma means we have another codepoint but in a different
            // range so we return our current codepoint.
            // ',' => {
            //     self.advance();
            //     self.consumeWhitespace();
            //     if (self.eof()) return error.InvalidValue;
            //     return .{ start, start };
            // },
            b',' => {
                self.advance();
                self.consume_whitespace();
                if self.eof() {
                    return Err(Error::InvalidValue);
                }
                Ok(Some([start, start]))
            }

            // Hyphen means we have a range.
            // '-' => {
            //     self.advance();
            //     self.consumeWhitespace();
            //     if (self.eof()) return error.InvalidValue;
            //     const end = try self.parseCodepoint();
            //     self.consumeWhitespace();
            //     if (!self.eof() and self.input[self.i] != ',') return error.InvalidValue;
            //     self.advance();
            //     self.consumeWhitespace();
            //     if (start > end) return error.InvalidValue;
            //     return .{ start, end };
            // },
            b'-' => {
                self.advance();
                self.consume_whitespace();
                if self.eof() {
                    return Err(Error::InvalidValue);
                }
                let end = self.parse_codepoint()?;
                self.consume_whitespace();
                if !self.eof() && self.input[self.i] != b',' {
                    return Err(Error::InvalidValue);
                }
                self.advance();
                self.consume_whitespace();
                if start > end {
                    return Err(Error::InvalidValue);
                }
                Ok(Some([start, end]))
            }

            // else => return error.InvalidValue,
            _ => Err(Error::InvalidValue),
        }
    }

    // fn consumeWhitespace(self: *UnicodeRangeParser) void {
    fn consume_whitespace(&mut self) {
        // while (!self.eof()) {
        while !self.eof() {
            // switch (self.input[self.i]) {
            match self.input[self.i] {
                // ' ', '\t' => self.advance(),
                b' ' | b'\t' => self.advance(),
                // else => return,
                _ => return,
            }
        }
    }

    // fn parseCodepoint(self: *UnicodeRangeParser) !u21 {
    fn parse_codepoint(&mut self) -> Result<u21, ParseError> {
        // if (self.input[self.i] != 'U') return error.InvalidValue;
        if self.input[self.i] != b'U' {
            return Err(Error::InvalidValue);
        }
        // self.advance();
        self.advance();
        // if (self.eof()) return error.InvalidValue;
        if self.eof() {
            return Err(Error::InvalidValue);
        }
        // if (self.input[self.i] != '+') return error.InvalidValue;
        if self.input[self.i] != b'+' {
            return Err(Error::InvalidValue);
        }
        // self.advance();
        self.advance();
        // if (self.eof()) return error.InvalidValue;
        if self.eof() {
            return Err(Error::InvalidValue);
        }

        // const start_i = self.i;
        let start_i = self.i;
        // while (true) {
        loop {
            // const current = self.input[self.i];
            let current = self.input[self.i];
            // const is_hex = (current >= '0' and current <= '9') or (current >= 'A' and current <= 'F') or (current >= 'a' and current <= 'f');
            let is_hex = current.is_ascii_digit()
                || (b'A'..=b'F').contains(&current)
                || (b'a'..=b'f').contains(&current);
            // if (!is_hex) break;
            if !is_hex {
                break;
            }

            // Advance but break on EOF
            // self.advance();
            self.advance();
            // if (self.eof()) break;
            if self.eof() {
                break;
            }
        }

        // If we didn't consume a single character, we have an error.
        // if (start_i == self.i) return error.InvalidValue;
        if start_i == self.i {
            return Err(Error::InvalidValue);
        }

        // return std.fmt.parseInt(u21, self.input[start_i..self.i], 16) catch return error.InvalidValue;
        parse_int_u21_base16(&self.input[start_i..self.i]).map_err(|_| Error::InvalidValue)
    }

    // fn advance(self: *UnicodeRangeParser) void { self.i += 1; }
    fn advance(&mut self) {
        self.i += 1;
    }

    // fn eof(self: *const UnicodeRangeParser) bool { return self.i >= self.input.len; }
    fn eof(&self) -> bool {
        self.i >= self.input.len()
    }
}

/// See "link" for documentation.
// pub const RepeatableLink = struct { links: ArrayListUnmanaged(inputpkg.Link) };
#[derive(Debug, Clone, Default)]
pub struct RepeatableLink {
    pub links: Vec<rustty_input::link::Link>,
}

impl RepeatableLink {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, _input: Option<&[u8]>) -> Result<(), ParseError> {
        // _ = self; _ = alloc; _ = input_;
        // return error.NotImplemented;
        //
        // PORT NOTE: the reference's `parseCLI` is an explicit stub that always
        // returns `error.NotImplemented` ("This currently can't be set").
        //
        // CORRECTION (LEDGER S76-F2): this used to say "The base args `Error`
        // set has no `NotImplemented` ... so the error collapses to
        // `InvalidValue`". That changed the recorded diagnostic from the
        // reference's `unknown error error.NotImplemented` to the invalid-value
        // message; `rustty-args` now carries `Error::NotImplemented`.
        Err(Error::NotImplemented)
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> RepeatableLink {
        // Note: we don't do any errdefers below since the allocation
        // is expected to be arena allocated.
        // var list = try std.ArrayListUnmanaged(inputpkg.Link).initCapacity(alloc, self.links.items.len);
        // for (self.links.items) |item| { const copy = try item.clone(alloc); list.appendAssumeCapacity(copy); }
        // return .{ .links = list };
        RepeatableLink {
            links: self.links.iter().map(|item| item.clone_in()).collect(),
        }
    }

    /// Compare if two of our value are requal. Required by Config.
    // pub fn equal(self: Self, other: Self) bool
    pub fn equal(&self, other: &RepeatableLink) -> bool {
        // const itemsA = self.links.items;
        // const itemsB = other.links.items;
        // if (itemsA.len != itemsB.len) return false;
        if self.links.len() != other.links.len() {
            return false;
        }
        // for (itemsA, itemsB) |*a, *b| { if (!a.equal(b)) return false; } else return true;
        for (a, b) in self.links.iter().zip(other.links.iter()) {
            if !a.equal(b) {
                return false;
            }
        }
        true
    }

    /// Used by Formatter
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        _name: &[u8],
        _writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // This currently can't be set so we don't format anything.
        // _ = self; _ = formatter;
        Ok(())
    }
}

impl FormatEntry for RepeatableLink {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        RepeatableLink::format_entry(self, name, writer)
    }
}

/// See "command-palette-entry" for documentation.
// pub const RepeatableCommand = struct { value: ArrayListUnmanaged(inputpkg.Command), value_c: ArrayListUnmanaged(inputpkg.Command.C) };
//
// PORT NOTE (ABI FIX, S1-F1/S103-F1): `rustty_input::command::C` is now a bare
// `#[repr(C)]` struct of raw `*const c_char` pointers (matching
// `rustty_command_s` in `include/rustty-embed.h`) with nothing to own the
// strings it points into. `storage` holds the owning `CommandCStorage` for
// every `value_c` entry, index-for-index; it MUST be rebuilt in the same
// motion as `value_c` at every mutation site (`init`, `parse_cli`, `clone_in`)
// so a `C` in `value_c` never outlives the storage its pointers borrow.
//
// Deliberately NOT `#[derive(Clone)]`: a derived field-by-field clone would
// clone `storage` (deep-copying the `CString`s into fresh allocations) and
// `value_c` (a bitwise pointer copy, since `C: Copy`) independently, leaving
// the clone's `value_c` pointers aliasing the ORIGINAL's `storage`, not its
// own. Use `clone_in()`, which rebuilds `value_c`/`storage` together from the
// cloned `value`.
#[derive(Debug, Default)]
pub struct RepeatableCommand {
    pub value: Vec<rustty_input::command::Command>,
    pub value_c: Vec<rustty_input::command::C>,
    storage: Vec<rustty_input::command::CommandCStorage>,
}

/// ghostty_config_command_palette_entry_s -> rustty_config_command_palette_entry_s (R2) (the extern C view).
// pub const C = extern struct { ... };
#[repr(C)]
pub struct RepeatableCommandC {
    pub commands: *const rustty_input::command::C,
    pub len: usize,
}

// PORT NOTE (R4 SEAM, RESOLVED): `RepeatableCommand.parseCLI` calls
// `cli.args.parseAutoStruct(inputpkg.Command, ...)`, i.e. it needs the *external*
// `rustty_input::command::Command` type to implement the `rustty_args::AutoStruct`
// trait. Both types are foreign to this crate, so the orphan rule forbids the impl
// here; it lives in rustty-input (alongside `Command`, in command.rs). With that
// impl landed, `init`/`parseCLI`/`formatEntry` are filled here: they consume
// `inputpkg.command.defaults`/`defaultsC`, `Command.cval`, and `Command.format`
// (all ported). `clone`/`equal`/`cval` were already filled.
impl RepeatableCommand {
    // pub fn cval(self: *const Self) C
    pub fn cval(&self) -> RepeatableCommandC {
        // return .{ .commands = self.value_c.items.ptr, .len = self.value_c.items.len };
        RepeatableCommandC {
            commands: self.value_c.as_ptr(),
            len: self.value_c.len(),
        }
    }

    // pub fn init(self: *Self, alloc: Allocator) !void
    //
    // PORT NOTE: the reference `!void` return is the `Allocator.Error` (OOM) from
    // the two `appendSlice`s; the Rust port aborts on allocation failure (no
    // recoverable OOM), so `init` is infallible here. `errdefer { deinit; deinit }`
    // has no analog (owned `Vec`s free on drop).
    pub fn init(&mut self) {
        // self.value = .empty;
        // self.value_c = .empty;
        self.value = Vec::new();
        self.value_c = Vec::new();
        self.storage = Vec::new();
        // try self.value.ensureUnusedCapacity(alloc, inputpkg.command.defaults.len);
        // try self.value_c.ensureUnusedCapacity(alloc, inputpkg.command.defaults.len);
        let defaults = rustty_input::command::defaults();
        self.value.reserve(defaults.len());
        self.value_c.reserve(defaults.len());
        self.storage.reserve(defaults.len());
        // for (inputpkg.command.defaults) |cmd| {
        for cmd in defaults {
            // Translation is currently a GTK-only feature. In particular,
            // translating these shared strings for the embedded runtime gives
            // the macOS app a localized command palette in an otherwise
            // unlocalized UI.
            // const localized = if (comptime build_config.app_runtime == .gtk)
            //     cmd.translated()
            // else
            //     cmd;
            let localized = if matches!(
                rustty_build_config::app_runtime(),
                rustty_build_config::Runtime::Gtk
            ) {
                cmd.translated()
            } else {
                cmd
            };
            // self.value_c.appendAssumeCapacity(try localized.cval(alloc));
            //
            // PORT NOTE: `cval` is `!C` (OOM only); the Rust port aborts on
            // allocation failure, so the unit error is unreachable here.
            //
            // PORT NOTE (ABI FIX, S1-F1/S103-F1): `cval()` now returns the
            // owning `CommandCStorage`, not a bare `C`. Push the storage into
            // `self.storage` in the SAME motion as pushing its `C` view into
            // `self.value_c`, so the pointers in `value_c[i]` are always
            // backed by `storage[i]`.
            let localized_storage = localized.cval().expect("cval is infallible");
            let localized_c = localized_storage.as_c();
            // self.value.appendAssumeCapacity(localized);
            self.value.push(localized);
            self.value_c.push(localized_c);
            self.storage.push(localized_storage);
        }
    }

    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // Unset or empty input clears the list
        // const input = input_ orelse "";
        let input = input_.unwrap_or(b"");
        // if (input.len == 0) {
        if input.is_empty() {
            // log.info("config has 'command-palette-entry =', using default entries", .{});
            crate::config_struct::log_info(
                "config has 'command-palette-entry =', using default entries".to_string(),
            );
            // try self.init(alloc);
            self.init();
            // return;
            return Ok(());
        }

        // if (std.mem.eql(u8, input, "clear")) {
        if input == b"clear" {
            // log.info("config has 'command-palette-entry = clear', all command entries cleared", .{});
            crate::config_struct::log_info(
                "config has 'command-palette-entry = clear', all command entries cleared"
                    .to_string(),
            );
            // self.value.clearRetainingCapacity();
            self.value.clear();
            // self.value_c.clearRetainingCapacity();
            self.value_c.clear();
            // PORT NOTE (ABI FIX, S1-F1/S103-F1): `storage` backs `value_c`
            // index-for-index; clear it alongside so no stale entries remain.
            self.storage.clear();
            // return;
            return Ok(());
        }

        // Reserve space in our lists
        // try self.value.ensureUnusedCapacity(alloc, 1);
        self.value.reserve(1);
        // try self.value_c.ensureUnusedCapacity(alloc, 1);
        self.value_c.reserve(1);
        self.storage.reserve(1);

        // const cmd = try cli.args.parseAutoStruct(inputpkg.Command, alloc, input, null);
        //
        // PORT NOTE: `parseAutoStruct(Command, ...)` is `parse_auto_struct` over the
        // `AutoStruct for Command` impl (in rustty-input). The result is built into a
        // fresh `Command`; `parse_auto_struct` fills set fields, applies the
        // `description = ""` default, and errors on an unset required field.
        let mut cmd = rustty_input::command::Command {
            action: rustty_input::binding::Action::Ignore,
            title: Vec::new(),
            description: Vec::new(),
        };
        parse_auto_struct(&mut cmd, input)?;
        // const cmd_c = try cmd.cval(alloc);
        //
        // PORT NOTE: `cmd.cval(alloc)` is `!C` (the OOM path); the Rust `cval`
        // returns `Result<C, ()>` carrying that fallible signature. `catch return
        // error.OutOfMemory` -> map the unit error into the parse error set.
        //
        // PORT NOTE (ABI FIX, S1-F1/S103-F1): `cval()` now returns the owning
        // `CommandCStorage`; push it into `self.storage` alongside its `C`
        // view in `self.value_c` in the same motion.
        let cmd_storage = cmd.cval().map_err(|()| Error::InvalidValue)?;
        let cmd_c = cmd_storage.as_c();
        // self.value.appendAssumeCapacity(cmd);
        self.value.push(cmd);
        // self.value_c.appendAssumeCapacity(cmd_c);
        self.value_c.push(cmd_c);
        self.storage.push(cmd_storage);

        Ok(())
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const Self, alloc: Allocator) Allocator.Error!Self
    //
    // PORT NOTE (ABI FIX, S1-F1/S103-F1): the reference used to clone
    // `value_c` directly (`try self.value_c.clone(alloc)`), which copied
    // `Command.C` structs whose string pointers still referenced the source
    // config's memory — a use-after-free once the source config was freed
    // (embedded host: `rustty_config_clone` + `rustty_config_free` of the
    // old config), caught by ASan (`5909690de`). Upstream fixed this by
    // rebuilding `value_c` from the cloned commands via the same `cval`
    // path `parseCLI` uses. Rust's `C` is a bare pointer struct borrowing
    // `self.storage`, so this port never had a choice in the matter: cloning
    // `value_c` directly would copy pointers that alias INTO `self`'s
    // storage, which dangle the moment either `RepeatableCommand` is dropped
    // or reallocates. Clone `value` and rebuild `value_c`/`storage` from the
    // clone in the same motion, exactly like `init`/`parse_cli` do — this
    // already matches upstream's fixed end state, so there is no behavior
    // change here, only the corrected rationale.
    //
    // Reference comment, carried:
    //
    // Cloning value_c directly would copy Command.C structs
    // whose string pointers still reference the source config's
    // memory — the clone must stay valid after the source is
    // freed.
    pub fn clone_in(&self) -> RepeatableCommand {
        let value: Vec<rustty_input::command::Command> =
            self.value.iter().map(|item| item.clone_in()).collect();
        let mut value_c: Vec<rustty_input::command::C> = Vec::with_capacity(value.len());
        let mut storage: Vec<rustty_input::command::CommandCStorage> =
            Vec::with_capacity(value.len());
        for cmd in &value {
            let cmd_storage = cmd.cval().expect("cval is infallible");
            value_c.push(cmd_storage.as_c());
            storage.push(cmd_storage);
        }
        RepeatableCommand {
            value,
            value_c,
            storage,
        }
    }

    /// Compare if two of our value are equal. Required by Config.
    // pub fn equal(self: RepeatableCommand, other: RepeatableCommand) bool
    pub fn equal(&self, other: &RepeatableCommand) -> bool {
        // if (self.value.items.len != other.value.items.len) return false;
        if self.value.len() != other.value.len() {
            return false;
        }
        // for (self.value.items, other.value.items) |a, b| { if (!a.equal(b)) return false; }
        for (a, b) in self.value.iter().zip(other.value.iter()) {
            if !a.equal(b) {
                return false;
            }
        }
        // return true;
        true
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    //
    // PORT NOTE: the reference builds each entry's bytes into a 4096-byte fixed
    // buffer (`std.Io.Writer.fixed`), then hands `writer.buffered()` to
    // `formatter.formatEntry([]const u8, ...)`. The Rust port accumulates each
    // entry into an owned `Vec<u8>` and emits it via `format_entry_string`;
    // `std.zig.fmtString` -> `zig_string_escape`; `{f}` on the action ->
    // `Command.action.format`.
    //
    // The fixed buffer's BOUND is observable and is reproduced: every reference
    // `writer.print(...) catch return error.OutOfMemory` fails as soon as the
    // entry passes 4096 bytes, so the whole config dump fails on an overlong
    // title/description/action. `fixed_writer_cap` below is checked after each
    // `print`, at exactly the points the reference can fail (parity S76-F4).
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // if (self.value.items.len == 0) {
        if self.value.is_empty() {
            // try formatter.formatEntry(void, {});
            // return;
            return format_entry_void(name, writer);
        }

        // for (self.value.items) |item| {
        for item in self.value.iter() {
            // var buf: [4096]u8 = undefined;
            // var writer: std.Io.Writer = .fixed(&buf);
            let mut buf: Vec<u8> = Vec::new();

            // writer.print("title:\"{f}\"", .{std.zig.fmtString(item.title)}) catch return error.OutOfMemory;
            buf.extend_from_slice(b"title:\"");
            rustty_args::args::zig_string_escape(&mut buf, &item.title);
            buf.push(b'"');
            fixed_writer_cap(&buf, 4096)?;

            // if (item.description.len > 0) {
            if !item.description.is_empty() {
                // writer.print(",description:\"{f}\"", .{std.zig.fmtString(item.description)}) catch return error.OutOfMemory;
                buf.extend_from_slice(b",description:\"");
                rustty_args::args::zig_string_escape(&mut buf, &item.description);
                buf.push(b'"');
                fixed_writer_cap(&buf, 4096)?;
            }

            // writer.print(",action:\"{f}\"", .{item.action}) catch return error.OutOfMemory;
            buf.extend_from_slice(b",action:\"");
            // PORT NOTE: `{f}` on the action -> `Action::format` into a buffer; it is
            // Infallible (never errors), matching the reference's never-failing path.
            {
                let mut action_buf = rustty_cfg_fmt::AllocatingWriter::init();
                let _ = item.action.format(&mut action_buf);
                buf.extend_from_slice(action_buf.written());
            }
            buf.push(b'"');
            fixed_writer_cap(&buf, 4096)?;

            // try formatter.formatEntry([]const u8, writer.buffered());
            format_entry_string(name, &buf, writer)?;
        }

        Ok(())
    }
}

/// PORTS: the failure edge of `std.Io.Writer.fixed(&buf)` when the reference
/// accumulates an entry with `writer.print(...) catch return error.OutOfMemory`.
/// The port builds the same bytes into a `Vec` and calls this at each point the
/// reference's writer can overflow, so the observable bound is identical.
fn fixed_writer_cap<E>(buf: &[u8], cap: usize) -> Result<(), FormatError<E>> {
    if buf.len() > cap {
        return Err(FormatError::OutOfMemory);
    }
    Ok(())
}

impl FormatEntry for RepeatableCommand {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        RepeatableCommand::format_entry(self, name, writer)
    }
}
