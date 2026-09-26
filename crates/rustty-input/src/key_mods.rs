//! rustty-input input/key_mods (config-seam slice)
//!
//! PORTS: ghostty src/input/key_mods.zig — the `RemapSet` + `Mod`/`alias` and
//! `Mods.translation` slice that the `rustty-vt` partial port DEFERRED (it needs
//! the apprt-layer `config.OptionAsAlt` + the config `formatter`, both now
//! available as `rustty-input`'s `config::OptionAsAlt` and `rustty-cfg-fmt`).
//!
//! PORT STATUS: ported
//!
//! The `Mods` packed struct (+ `Mod`/`ALIAS`/`ModSide`/`Side`/`Keys`/`SIDE_MASK`
//! and the VT-relevant methods) lives in `rustty-vt`'s `input/key_mods.rs` and is
//! reused here (the VT slice is the leaf; this is its config-dependent peer). This
//! module ports the remaining declarations — `Mods.translation` (as a free fn,
//! since `Mods` lives in vt and `translation` needs `OptionAsAlt`), the `RemapSet`
//! type with its `parseCLI`/`parse`/`finalize`/`clone`/`equal`/`formatEntry`/
//! `apply`/`Mask` machinery, and ALL of the reference's `RemapSet:` + `translation
//! macos-option-as-alt` `test{}` blocks (R1: assertion-for-assertion).
//
// PORT NOTE: the reference imports
//   const std = @import("std");
//   const assert = @import("../quirks.zig").inlineAssert;
//   const Allocator = std.mem.Allocator;
//   const builtin = @import("builtin");
//   const OptionAsAlt = @import("config.zig").OptionAsAlt;
// `std`/`Allocator` map to the Rust system allocator (the `AutoArrayHashMap` from
// `rustty-zigstd` owns its allocations via the global allocator, so the `alloc:
// Allocator` seam threaded through `parse`/`clone`/`deinit` is dropped exactly as
// the `Binding.zig` port did). `builtin` (target OS, for `translation`) maps to
// `cfg!(target_vendor = "apple")`. `OptionAsAlt` maps to `config::OptionAsAlt`.

use crate::config::OptionAsAlt;
use rustty_cfg_fmt::{
    format_entry_string, format_entry_void, EntryFormatter, FormatEntry, FormatError, Writer,
};
use rustty_vt::input::key_mods::{Keys, Mod, ModSide, Mods, Side, ALIAS};
use rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged;

/// Return the mods to use for key translation. This handles settings
/// like macos-option-as-alt. The translation mods should be used for
/// translation but never sent back in for the key callback.
//
// PORT NOTE: the reference is a method `Mods.translation(self, option_as_alt)`.
// `Mods` lives in `rustty-vt` (the VT leaf) and this transform needs the
// apprt-layer `OptionAsAlt`, so it is ported here as a free fn taking the `Mods`
// by value — a config-seam peer of the vt `Mods` methods. The body is otherwise
// assertion-for-assertion the reference.
// pub fn translation(self: Mods, option_as_alt: OptionAsAlt) Mods {
pub fn translation(self_: Mods, option_as_alt: OptionAsAlt) -> Mods {
    // var result = self;
    let mut result = self_;

    // macos-option-as-alt for darwin
    // if (comptime builtin.target.os.tag.isDarwin()) alt: {
    if cfg!(target_vendor = "apple") {
        // The `alt:` labeled block's `break :alt` is reproduced with an inner
        // closure returning whether to unset alt (Rust has no labeled break out
        // of a non-loop block).
        let unset_alt = || -> bool {
            // Alt has to be set only on the correct side
            // switch (option_as_alt) {
            match option_as_alt {
                // .false => break :alt,
                OptionAsAlt::False => false,
                // .true => {},
                OptionAsAlt::True => true,
                // .left => if (self.sides.alt == .right) break :alt,
                OptionAsAlt::Left => self_.sides.alt != ModSide::Right,
                // .right => if (self.sides.alt == .left) break :alt,
                OptionAsAlt::Right => self_.sides.alt != ModSide::Left,
            }
        };

        // Unset alt
        // result.alt = false;
        if unset_alt() {
            result.alt = false;
        }
    }

    // return result;
    result
}

/// PORT NOTE: `RemapSet.ParseError = Allocator.Error || error{MissingAssignment,
/// InvalidMod}`. `Allocator.Error` (OutOfMemory) is unreachable in this port (the
/// global allocator aborts on OOM), so the error set is exactly the two parse
/// errors. Carried as a unit enum so the tests can `expectError` on each variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    MissingAssignment,
    InvalidMod,
}

/// PORT NOTE: `parseCLI` maps the `parse` errors to `error.InvalidValue` (the
/// config CLI error). Modeled as a dedicated unit enum so `parseCLI`'s tests can
/// `expectError(error.InvalidValue, ...)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseCliError {
    InvalidValue,
}

/// PORT NOTE: `parseMod` returns `error{InvalidMod}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InvalidModError;

/// Modifier remapping. See `key-remap` in Config.zig for detailed docs.
// pub const RemapSet = struct {
pub struct RemapSet {
    /// Available mappings.
    // map: std.AutoArrayHashMapUnmanaged(Mods, Mods),
    pub map: AutoArrayHashMapUnmanaged<Mods, Mods>,

    /// The mask of remapped modifiers that can be used to quickly
    /// check if some input mods need remapping.
    // mask: Mask,
    pub mask: Mask,
}

impl RemapSet {
    // pub const empty: RemapSet = .{ .map = .{}, .mask = .{} };
    pub fn empty() -> RemapSet {
        RemapSet {
            map: AutoArrayHashMapUnmanaged::new(),
            mask: Mask::default(),
        }
    }

    /// Parse from CLI input. Required by Config.
    // pub fn parseCLI(self: *RemapSet, alloc: Allocator, input: ?[]const u8) !void {
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseCliError> {
        // const value = input orelse "";
        let value = input.unwrap_or(b"");

        // Empty value resets the set
        // if (value.len == 0) {
        if value.is_empty() {
            // self.map.clearRetainingCapacity();
            self.map.clear_retaining_capacity();
            // self.mask = .{};
            self.mask = Mask::default();
            // return;
            return Ok(());
        }

        // self.parse(alloc, value) catch |err| switch (err) {
        //     error.OutOfMemory => return error.OutOfMemory,
        //     error.MissingAssignment, error.InvalidMod => return error.InvalidValue,
        // };
        self.parse(value).map_err(|err| match err {
            ParseError::MissingAssignment | ParseError::InvalidMod => ParseCliError::InvalidValue,
        })
    }

    /// Parse a modifier remap and add it to the set.
    // pub fn parse(self: *RemapSet, alloc: Allocator, input: []const u8) ParseError!void {
    pub fn parse(&mut self, input: &[u8]) -> Result<(), ParseError> {
        // Find the assignment point ('=')
        // const eql_idx = std.mem.indexOfScalar(u8, input, '=') orelse return error.MissingAssignment;
        let eql_idx = input
            .iter()
            .position(|&b| b == b'=')
            .ok_or(ParseError::MissingAssignment)?;

        // The to side defaults to "left" if no explicit side is given.
        // This is because this is the default unsided value provided by
        // the apprts in the current Mods layout.
        // const to: Mods = to: {
        //     const raw = try parseMod(input[eql_idx + 1 ..]);
        //     break :to initMods(raw[0], raw[1] orelse .left);
        // };
        let to: Mods = {
            let raw = Self::parse_mod(&input[eql_idx + 1..]).map_err(|_| ParseError::InvalidMod)?;
            Self::init_mods(raw.0, raw.1.unwrap_or(ModSide::Left))
        };

        // The from side, if sided, is easy and we put it directly into
        // the map.
        // const from_raw = try parseMod(input[0..eql_idx]);
        let from_raw = Self::parse_mod(&input[0..eql_idx]).map_err(|_| ParseError::InvalidMod)?;
        // if (from_raw[1]) |from_side| {
        if let Some(from_side) = from_raw.1 {
            // const from: Mods = initMods(from_raw[0], from_side);
            let from: Mods = Self::init_mods(from_raw.0, from_side);
            // try self.map.put(alloc, from, to);
            self.map.put(from, to);
            // errdefer comptime unreachable;
            // self.mask.update(from);
            self.mask.update(from);
            // return;
            return Ok(());
        }

        // We need to do some combinatorial explosion here for unsided
        // from in order to assign all possible sides.
        // const from_left = initMods(from_raw[0], .left);
        let from_left = Self::init_mods(from_raw.0, ModSide::Left);
        // const from_right = initMods(from_raw[0], .right);
        let from_right = Self::init_mods(from_raw.0, ModSide::Right);
        // try self.map.put(alloc, from_left, to);
        self.map.put(from_left, to);
        // errdefer _ = self.map.swapRemove(from_left);
        // try self.map.put(alloc, from_right, to);
        self.map.put(from_right, to);
        // errdefer _ = self.map.swapRemove(from_right);

        // errdefer comptime unreachable;
        // self.mask.update(from_left);
        self.mask.update(from_left);
        // self.mask.update(from_right);
        self.mask.update(from_right);

        Ok(())
    }

    // pub fn deinit(self: *RemapSet, alloc: Allocator) void {
    //     self.map.deinit(alloc);
    // }
    //
    // PORT NOTE: `deinit` frees the map's allocations; in Rust this is `Drop`
    // (the `AutoArrayHashMapUnmanaged`'s `Vec`/`HashMap` drop automatically). The
    // explicit `deinit(alloc)` seam — and the tests' `defer set.deinit(alloc)` —
    // are therefore dropped (RAII), exactly as the `Binding.zig` port did.

    /// Must be called prior to any remappings so that the mapping
    /// is sorted properly. Otherwise, you will get invalid results.
    // pub fn finalize(self: *RemapSet) void {
    pub fn finalize(&mut self) {
        // const Context = struct {
        //     keys: []const Mods,
        //     pub fn lessThan(ctx, a_index, b_index) bool {
        //         _ = b_index;
        //         // Mods with any right sides prioritize
        //         const side_mask = comptime Mods.side_mask.int();
        //         const a = ctx.keys[a_index];
        //         return a.int() & side_mask != 0;
        //     }
        // };
        // self.map.sort(Context{ .keys = self.map.keys() });
        //
        // PORT NOTE: the comparator captures the keys array and reads `keys[a_index]`
        // (ignoring `b_index`), returning true iff `a` has any right-side bit set.
        // The keys are snapshotted before sorting so the closure does not borrow
        // `self.map` while it is being sorted.
        let side_mask = Mods::SIDE_MASK.int();
        let keys: Vec<Mods> = self.map.keys().copied().collect();
        self.map.sort(|a_index, _b_index| {
            let a = keys[a_index];
            a.int() & side_mask != 0
        });
    }

    /// Deep copy of the struct. Required by Config.
    // pub fn clone(self: *const RemapSet, alloc: Allocator) Allocator.Error!RemapSet {
    //     return .{ .map = try self.map.clone(alloc), .mask = self.mask };
    // }
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> RemapSet {
        RemapSet {
            map: self.map.clone(),
            mask: self.mask,
        }
    }

    /// Compare if two RemapSets are equal. Required by Config.
    // pub fn equal(self: RemapSet, other: RemapSet) bool {
    pub fn equal(&self, other: &RemapSet) -> bool {
        // if (self.map.count() != other.map.count()) return false;
        if self.map.count() != other.map.count() {
            return false;
        }

        // var it = self.map.iterator();
        // while (it.next()) |entry| {
        for (key, value) in self.map.iter() {
            // const other_value = other.map.get(entry.key_ptr.*) orelse return false;
            let other_value = match other.map.get(key) {
                Some(v) => v,
                None => return false,
            };
            // if (!entry.value_ptr.equal(other_value)) return false;
            if !value.equal(*other_value) {
                return false;
            }
        }

        // return true;
        true
    }

    /// Used by Formatter. Required by Config.
    // pub fn formatEntry(self: RemapSet, formatter: anytype) !void {
    pub fn format_entry<W: Writer>(
        &self,
        formatter: &mut EntryFormatter<W>,
    ) -> Result<(), FormatError<W::Error>> {
        // if (self.map.count() == 0) {
        if self.map.count() == 0 {
            // try formatter.formatEntry(void, {});
            formatter.format_entry(&VoidValue)?;
            // return;
            return Ok(());
        }

        // var it = self.map.iterator();
        // while (it.next()) |entry| {
        for (key, value) in self.map.iter() {
            // const from = entry.key_ptr.*;
            let from = *key;
            // const to = entry.value_ptr.*;
            let to = *value;

            // var buf: [64]u8 = undefined;
            // var fbs = std.io.fixedBufferStream(&buf);
            // const writer = fbs.writer();
            let mut fbs: Vec<u8> = Vec::new();
            // formatMod(writer, from) catch return error.OutOfMemory;
            Self::format_mod(&mut fbs, from);
            // writer.writeByte('=') catch return error.OutOfMemory;
            fbs.push(b'=');
            // formatMod(writer, to) catch return error.OutOfMemory;
            Self::format_mod(&mut fbs, to);
            // try formatter.formatEntry([]const u8, fbs.getWritten());
            formatter.format_entry(&ByteSlice(fbs))?;
        }

        Ok(())
    }

    // fn formatMod(writer: anytype, mods: Mods) !void {
    fn format_mod(writer: &mut Vec<u8>, mods: Mods) {
        // Check which mod is set and format it with optional side prefix
        // inline for (.{ "shift", "ctrl", "alt", "super" }) |name| {
        for name in [
            (b"shift".as_slice(), mods.shift, mods.sides.shift),
            (b"ctrl".as_slice(), mods.ctrl, mods.sides.ctrl),
            (b"alt".as_slice(), mods.alt, mods.sides.alt),
            (b"super".as_slice(), mods.super_, mods.sides.super_),
        ] {
            let (name_bytes, set, side) = name;
            // if (@field(mods, name)) {
            if set {
                // const side = @field(mods.sides, name);
                // if (side == .right) {
                if side == ModSide::Right {
                    // try writer.writeAll("right_");
                    writer.extend_from_slice(b"right_");
                } else {
                    // Only write left_ if we need to distinguish
                    // For now, always write left_ if it's a sided mapping
                    // try writer.writeAll("left_");
                    writer.extend_from_slice(b"left_");
                }
                // try writer.writeAll(name);
                writer.extend_from_slice(name_bytes);
                // return;
                return;
            }
        }
    }

    /// Parses a single mode in a single remapping string. E.g.
    /// `ctrl` or `left_shift`.
    // fn parseMod(input: []const u8) error{InvalidMod}!struct { Mod, ?Mod.Side } {
    fn parse_mod(input: &[u8]) -> Result<(Mod, Option<ModSide>), InvalidModError> {
        // const side_str, const mod_str = if (std.mem.indexOfScalar(u8, input, '_')) |idx| .{
        //     input[0..idx],
        //     input[idx + 1 ..],
        // } else .{ "", input };
        let (side_str, mod_str): (&[u8], &[u8]) = match input.iter().position(|&b| b == b'_') {
            Some(idx) => (&input[0..idx], &input[idx + 1..]),
            None => (b"".as_slice(), input),
        };

        // const mod: Mod = if (std.meta.stringToEnum(Mod, mod_str)) |mod| mod else mod: {
        //     inline for (alias) |pair| {
        //         if (std.mem.eql(u8, mod_str, pair[0])) { break :mod pair[1]; }
        //     }
        //     return error.InvalidMod;
        // };
        // `mod` is a Rust keyword; ported as the raw identifier `r#mod`.
        let r#mod = match string_to_mod(mod_str) {
            Some(m) => m,
            None => {
                let mut found: Option<Mod> = None;
                for &(name, m) in ALIAS {
                    if mod_str == name {
                        found = Some(m);
                        break;
                    }
                }
                match found {
                    Some(m) => m,
                    None => return Err(InvalidModError),
                }
            }
        };

        // return .{
        //     mod,
        //     if (side_str.len > 0) std.meta.stringToEnum(Mod.Side, side_str) orelse return error.InvalidMod else null,
        // };
        let side = if !side_str.is_empty() {
            match string_to_side(side_str) {
                Some(s) => Some(s),
                None => return Err(InvalidModError),
            }
        } else {
            None
        };

        Ok((r#mod, side))
    }

    // fn initMods(mod: Mod, side: Mod.Side) Mods {
    fn init_mods(r#mod: Mod, side: ModSide) -> Mods {
        // switch (mod) {
        //     inline else => |tag| {
        //         var mods: Mods = .{};
        //         @field(mods, @tagName(tag)) = true;
        //         @field(mods.sides, @tagName(tag)) = side;
        //         return mods;
        //     },
        // }
        let mut mods = Mods::default();
        match r#mod {
            Mod::Shift => {
                mods.shift = true;
                mods.sides.shift = side;
            }
            Mod::Ctrl => {
                mods.ctrl = true;
                mods.sides.ctrl = side;
            }
            Mod::Alt => {
                mods.alt = true;
                mods.sides.alt = side;
            }
            Mod::Super => {
                mods.super_ = true;
                mods.sides.super_ = side;
            }
        }
        mods
    }

    /// Returns true if the given mods need remapping.
    // pub fn isRemapped(self: *const RemapSet, mods: Mods) bool {
    //     return self.mask.match(mods);
    // }
    pub fn is_remapped(&self, mods: Mods) -> bool {
        self.mask.match_(mods)
    }

    /// Apply a remap to the given mods.
    // pub fn apply(self: *const RemapSet, mods: Mods) Mods {
    pub fn apply(&self, mods: Mods) -> Mods {
        // if (!self.isRemapped(mods)) return mods;
        if !self.is_remapped(mods) {
            return mods;
        }

        // const mods_binding: Mods.Keys.Backing = @truncate(mods.int());
        let mods_binding: u8 = (mods.int() & 0b1111) as u8;
        // const mods_sides: Mods.Side.Backing = @bitCast(mods.sides);
        let mods_sides: u8 = mods.sides.int();

        // var it = self.map.iterator();
        // while (it.next()) |entry| {
        for (key, value) in self.map.iter() {
            // const from = entry.key_ptr.*;
            let from = *key;
            // const from_binding: Mods.Keys.Backing = @truncate(from.int());
            let from_binding: u8 = (from.int() & 0b1111) as u8;
            // if (mods_binding & from_binding != from_binding) continue;
            if mods_binding & from_binding != from_binding {
                continue;
            }
            // const from_sides: Mods.Side.Backing = @bitCast(from.sides);
            let from_sides: u8 = from.sides.int();
            // if ((mods_sides ^ from_sides) & from_binding != 0) continue;
            if (mods_sides ^ from_sides) & from_binding != 0 {
                continue;
            }

            // var mods_int = mods.int();
            let mut mods_int = mods.int();
            // mods_int &= ~from.int();
            mods_int &= !from.int();
            // mods_int |= entry.value_ptr.int();
            mods_int |= value.int();
            // return @bitCast(mods_int);
            return Mods::from_int(mods_int);
        }

        // unreachable;
        unreachable!()
    }
}

// `RemapSet` (the `KeyRemapSet`) is used as the `key-remap` config field type.
// It has an `equal` decl, so the reference's `Config.changed` resolves its
// `deepEqual` via the `@hasDecl(T, "equal")` fast path (`old.equal(new)`). The
// orphan rule requires this `DeepEqual` impl to live in `RemapSet`'s home crate
// (rustty-input) rather than rustty-config.
impl rustty_datastruct::comparison::DeepEqual for RemapSet {
    fn deep_equal(&self, other: &Self) -> bool {
        self.equal(other)
    }
}

/// PORT NOTE: a `void` value implementing `FormatEntry` so `RemapSet.formatEntry`
/// can pass it to `EntryFormatter::format_entry` — the reference's
/// `formatter.formatEntry(void, {})` (`.void` arm, `format_entry_void`).
struct VoidValue;
impl FormatEntry for VoidValue {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_void(name, writer)
    }
}

/// PORT NOTE: a `[]const u8` value wrapper implementing `FormatEntry` — the
/// reference's `formatter.formatEntry([]const u8, bytes)` (`.pointer` `[]const u8`
/// arm, `format_entry_string`). Mirrors the `Binding.zig` port's `ByteSlice`.
struct ByteSlice(Vec<u8>);
impl FormatEntry for ByteSlice {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_string(name, &self.0, writer)
    }
}

/// PORT NOTE: `std.meta.stringToEnum(Mod, name)` — exact byte-slice match of the
/// `Mod` tag names (`shift`/`ctrl`/`alt`/`super`).
fn string_to_mod(name: &[u8]) -> Option<Mod> {
    match name {
        b"shift" => Some(Mod::Shift),
        b"ctrl" => Some(Mod::Ctrl),
        b"alt" => Some(Mod::Alt),
        b"super" => Some(Mod::Super),
        _ => None,
    }
}

/// PORT NOTE: `std.meta.stringToEnum(Mod.Side, name)` — exact byte-slice match of
/// the `Mod.Side` tag names (`left`/`right`).
fn string_to_side(name: &[u8]) -> Option<ModSide> {
    match name {
        b"left" => Some(ModSide::Left),
        b"right" => Some(ModSide::Right),
        _ => None,
    }
}

/// Tracks which modifier keys and sides have remappings registered.
/// Used as a fast pre-check before doing expensive map lookups.
///
/// The mask uses separate tracking for left and right sides because
/// remappings can be side-specific (e.g., only remap left_ctrl).
///
/// Note: `left_sides` uses inverted logic where 1 means "left is remapped"
/// even though `Mod.Side.left = 0`. This allows efficient bitwise matching
/// since we can AND directly with the side bits.
// pub const Mask = packed struct(u12) {
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Mask {
    /// Which modifier keys (shift/ctrl/alt/super) have any remapping.
    // keys: Mods.Keys = .{},
    pub keys: Keys,
    /// Which modifiers have left-side remappings (inverted: 1 = left remapped).
    // left_sides: Mods.Side = .{},
    pub left_sides: Side,
    /// Which modifiers have right-side remappings (1 = right remapped).
    // right_sides: Mods.Side = .{},
    pub right_sides: Side,
}

impl Mask {
    /// Adds a modifier to the mask, marking it as having a remapping.
    // pub fn update(self: *Mask, mods: Mods) void {
    pub fn update(&mut self, mods: Mods) {
        // const keys_int: Mods.Keys.Backing = mods.keys().int();
        let keys_int: u8 = mods.keys().int();

        // OR the new keys into our existing keys mask.
        // Example: keys=0b0000, new ctrl → keys=0b0010
        // self.keys = @bitCast(self.keys.int() | keys_int);
        self.keys = Keys::from_int(self.keys.int() | keys_int);

        // Both Keys and Side are u4 with matching bit positions.
        // This lets us use keys_int to select which side bits to update.
        // const sides: Mods.Side.Backing = @bitCast(mods.sides);
        let sides: u8 = mods.sides.int();
        // const left_int: Mods.Side.Backing = @bitCast(self.left_sides);
        let left_int: u8 = self.left_sides.int();
        // const right_int: Mods.Side.Backing = @bitCast(self.right_sides);
        let right_int: u8 = self.right_sides.int();

        // Update left_sides: set bit if this key is active AND side is left.
        // Since Side.left=0, we invert sides (~sides) so left becomes 1.
        // keys_int masks to only affect the modifier being added.
        // Example: left_ctrl → keys_int=0b0010, ~sides=0b1111 (left=0 inverted)
        //          result: left_int | (0b0010 & 0b1111) = left_int | 0b0010
        // self.left_sides = @bitCast(left_int | (keys_int & ~sides));
        //
        // PORT NOTE: `~sides` is a `u4` bit-complement; masked with the `u4`
        // `keys_int` so only the low 4 bits matter. `Side::from_int` reads only
        // bits 0..=3, so the upper bits of the `u8` `!sides` are irrelevant.
        self.left_sides = Side::from_int(left_int | (keys_int & !sides));

        // Update right_sides: set bit if this key is active AND side is right.
        // Since Side.right=1, we use sides directly.
        // Example: right_ctrl → keys_int=0b0010, sides=0b0010 (right=1)
        //          result: right_int | (0b0010 & 0b0010) = right_int | 0b0010
        // self.right_sides = @bitCast(right_int | (keys_int & sides));
        self.right_sides = Side::from_int(right_int | (keys_int & sides));
    }

    /// Returns true if the given mods match any remapping in this mask.
    /// This is a fast check to avoid expensive map lookups when no
    /// remapping could possibly apply.
    ///
    /// Checks both that the modifier key is remapped AND that the
    /// specific side (left/right) being pressed has a remapping.
    // pub fn match(self: *const Mask, mods: Mods) bool {
    //
    // PORT NOTE: `match` is a Rust keyword; the method is named `match_`.
    pub fn match_(&self, mods: Mods) -> bool {
        // Find which pressed keys have remappings registered.
        // Example: pressed={ctrl,alt}, mask={ctrl} → active=0b0010 (just ctrl)
        // const active = mods.keys().int() & self.keys.int();
        let active = mods.keys().int() & self.keys.int();
        // if (active == 0) return false;
        if active == 0 {
            return false;
        }

        // Check if the pressed side matches a remapped side.
        // For left (sides bit = 0): check against left_int (where 1 = left remapped)
        //   ~sides inverts so left becomes 1, then AND with left_int
        // For right (sides bit = 1): check against right_int directly
        //
        // Example: pressing left_ctrl (sides.ctrl=0, left_int.ctrl=1)
        //   ~sides = 0b1111, left_int = 0b0010
        //   (~sides & left_int) = 0b0010 ✓ matches
        //
        // Example: pressing right_ctrl but only left_ctrl is remapped
        //   sides = 0b0010, left_int = 0b0010, right_int = 0b0000
        //   (~0b0010 & 0b0010) | (0b0010 & 0b0000) = 0b0000 ✗ no match
        // const sides: Mods.Side.Backing = @bitCast(mods.sides);
        let sides: u8 = mods.sides.int();
        // const left_int: Mods.Side.Backing = @bitCast(self.left_sides);
        let left_int: u8 = self.left_sides.int();
        // const right_int: Mods.Side.Backing = @bitCast(self.right_sides);
        let right_int: u8 = self.right_sides.int();
        // const side_match = (~sides & left_int) | (sides & right_int);
        let side_match = (!sides & left_int) | (sides & right_int);

        // Final check: is any active (pressed + remapped) key also side-matched?
        // return (active & side_match) != 0;
        (active & side_match) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_cfg_fmt::{entry_formatter, AllocatingWriter};

    // test "translation macos-option-as-alt"
    #[test]
    #[cfg_attr(
        not(target_vendor = "apple"),
        ignore = "SkipZigTest: darwin-only transform (R14)"
    )]
    fn translation_macos_option_as_alt() {
        // if (comptime !builtin.target.os.tag.isDarwin()) return error.SkipZigTest;
        //
        // PORT NOTE: the reference skips on non-darwin. We reproduce the skip with
        // the `cfg_attr(.., ignore)` above: on non-darwin the test reports
        // `ignored` (the Rust analog of `error.SkipZigTest`, R14), since
        // `translation` is a darwin-gated transform.

        // Unset
        {
            // const mods: Mods = .{};
            let mods: Mods = Mods::default();
            // const result = mods.translation(.true);
            let result = translation(mods, OptionAsAlt::True);
            // try testing.expectEqual(result, mods);
            assert_eq!(result, mods);
        }

        // Set
        {
            // const mods: Mods = .{ .alt = true };
            let mods: Mods = Mods {
                alt: true,
                ..Default::default()
            };
            // const result = mods.translation(.true);
            let result = translation(mods, OptionAsAlt::True);
            // try testing.expectEqual(Mods{}, result);
            assert_eq!(Mods::default(), result);
        }

        // Set but disabled
        {
            // const mods: Mods = .{ .alt = true };
            let mods: Mods = Mods {
                alt: true,
                ..Default::default()
            };
            // const result = mods.translation(.false);
            let result = translation(mods, OptionAsAlt::False);
            // try testing.expectEqual(result, mods);
            assert_eq!(result, mods);
        }

        // Set wrong side
        {
            // const mods: Mods = .{ .alt = true, .sides = .{ .alt = .right } };
            let mods: Mods = Mods {
                alt: true,
                sides: Side {
                    alt: ModSide::Right,
                    ..Default::default()
                },
                ..Default::default()
            };
            // const result = mods.translation(.left);
            let result = translation(mods, OptionAsAlt::Left);
            // try testing.expectEqual(result, mods);
            assert_eq!(result, mods);
        }
        {
            // const mods: Mods = .{ .alt = true, .sides = .{ .alt = .left } };
            let mods: Mods = Mods {
                alt: true,
                sides: Side {
                    alt: ModSide::Left,
                    ..Default::default()
                },
                ..Default::default()
            };
            // const result = mods.translation(.right);
            let result = translation(mods, OptionAsAlt::Right);
            // try testing.expectEqual(result, mods);
            assert_eq!(result, mods);
        }

        // Set with other mods
        {
            // const mods: Mods = .{ .alt = true, .shift = true };
            let mods: Mods = Mods {
                alt: true,
                shift: true,
                ..Default::default()
            };
            // const result = mods.translation(.true);
            let result = translation(mods, OptionAsAlt::True);
            // try testing.expectEqual(Mods{ .shift = true }, result);
            assert_eq!(
                Mods {
                    shift: true,
                    ..Default::default()
                },
                result
            );
        }
    }

    // test "RemapSet: unsided remap creates both left and right mappings"
    #[test]
    fn remap_set_unsided_remap_creates_both_left_and_right_mappings() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();
        // try set.parse(alloc, "ctrl=super");
        set.parse(b"ctrl=super").unwrap();
        // set.finalize();
        set.finalize();
        // try testing.expectEqual(
        //     Mods{ .super = true, .sides = .{ .super = .left } },
        //     set.apply(.{ .ctrl = true, .sides = .{ .ctrl = .left } }),
        // );
        assert_eq!(
            Mods {
                super_: true,
                sides: Side {
                    super_: ModSide::Left,
                    ..Default::default()
                },
                ..Default::default()
            },
            set.apply(Mods {
                ctrl: true,
                sides: Side {
                    ctrl: ModSide::Left,
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
        // try testing.expectEqual(
        //     Mods{ .super = true, .sides = .{ .super = .left } },
        //     set.apply(.{ .ctrl = true, .sides = .{ .ctrl = .right } }),
        // );
        assert_eq!(
            Mods {
                super_: true,
                sides: Side {
                    super_: ModSide::Left,
                    ..Default::default()
                },
                ..Default::default()
            },
            set.apply(Mods {
                ctrl: true,
                sides: Side {
                    ctrl: ModSide::Right,
                    ..Default::default()
                },
                ..Default::default()
            }),
        );
    }

    // test "RemapSet: sided from only maps that side"
    #[test]
    fn remap_set_sided_from_only_maps_that_side() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "left_alt=ctrl");
        set.parse(b"left_alt=ctrl").unwrap();
        // set.finalize();
        set.finalize();

        // const left_alt: Mods = .{ .alt = true, .sides = .{ .alt = .left } };
        let left_alt: Mods = Mods {
            alt: true,
            sides: Side {
                alt: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // const left_ctrl: Mods = .{ .ctrl = true, .sides = .{ .ctrl = .left } };
        let left_ctrl: Mods = Mods {
            ctrl: true,
            sides: Side {
                ctrl: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(left_ctrl, set.apply(left_alt));
        assert_eq!(left_ctrl, set.apply(left_alt));

        // const right_alt: Mods = .{ .alt = true, .sides = .{ .alt = .right } };
        let right_alt: Mods = Mods {
            alt: true,
            sides: Side {
                alt: ModSide::Right,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(right_alt, set.apply(right_alt));
        assert_eq!(right_alt, set.apply(right_alt));
    }

    // test "RemapSet: sided to"
    #[test]
    fn remap_set_sided_to() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "ctrl=right_super");
        set.parse(b"ctrl=right_super").unwrap();
        // set.finalize();
        set.finalize();

        // const left_ctrl: Mods = .{ .ctrl = true, .sides = .{ .ctrl = .left } };
        let left_ctrl: Mods = Mods {
            ctrl: true,
            sides: Side {
                ctrl: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // const right_super: Mods = .{ .super = true, .sides = .{ .super = .right } };
        let right_super: Mods = Mods {
            super_: true,
            sides: Side {
                super_: ModSide::Right,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(right_super, set.apply(left_ctrl));
        assert_eq!(right_super, set.apply(left_ctrl));
    }

    // test "RemapSet: both sides specified"
    #[test]
    fn remap_set_both_sides_specified() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "left_shift=right_ctrl");
        set.parse(b"left_shift=right_ctrl").unwrap();
        // set.finalize();
        set.finalize();

        // const left_shift: Mods = .{ .shift = true, .sides = .{ .shift = .left } };
        let left_shift: Mods = Mods {
            shift: true,
            sides: Side {
                shift: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // const right_ctrl: Mods = .{ .ctrl = true, .sides = .{ .ctrl = .right } };
        let right_ctrl: Mods = Mods {
            ctrl: true,
            sides: Side {
                ctrl: ModSide::Right,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(right_ctrl, set.apply(left_shift));
        assert_eq!(right_ctrl, set.apply(left_shift));
    }

    // test "RemapSet: multiple parses accumulate"
    #[test]
    fn remap_set_multiple_parses_accumulate() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "left_ctrl=super");
        set.parse(b"left_ctrl=super").unwrap();
        // try set.parse(alloc, "left_alt=ctrl");
        set.parse(b"left_alt=ctrl").unwrap();
        // set.finalize();
        set.finalize();

        // const left_ctrl: Mods = .{ .ctrl = true, .sides = .{ .ctrl = .left } };
        let left_ctrl: Mods = Mods {
            ctrl: true,
            sides: Side {
                ctrl: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // const left_super: Mods = .{ .super = true, .sides = .{ .super = .left } };
        let left_super: Mods = Mods {
            super_: true,
            sides: Side {
                super_: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(left_super, set.apply(left_ctrl));
        assert_eq!(left_super, set.apply(left_ctrl));

        // const left_alt: Mods = .{ .alt = true, .sides = .{ .alt = .left } };
        let left_alt: Mods = Mods {
            alt: true,
            sides: Side {
                alt: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // const left_ctrl_result: Mods = .{ .ctrl = true, .sides = .{ .ctrl = .left } };
        let left_ctrl_result: Mods = Mods {
            ctrl: true,
            sides: Side {
                ctrl: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(left_ctrl_result, set.apply(left_alt));
        assert_eq!(left_ctrl_result, set.apply(left_alt));
    }

    // test "RemapSet: error on missing assignment"
    #[test]
    fn remap_set_error_on_missing_assignment() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try testing.expectError(error.MissingAssignment, set.parse(alloc, "ctrl"));
        assert_eq!(set.parse(b"ctrl"), Err(ParseError::MissingAssignment));
        // try testing.expectError(error.MissingAssignment, set.parse(alloc, ""));
        assert_eq!(set.parse(b""), Err(ParseError::MissingAssignment));
    }

    // test "RemapSet: error on invalid modifier"
    #[test]
    fn remap_set_error_on_invalid_modifier() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try testing.expectError(error.InvalidMod, set.parse(alloc, "invalid=ctrl"));
        assert_eq!(set.parse(b"invalid=ctrl"), Err(ParseError::InvalidMod));
        // try testing.expectError(error.InvalidMod, set.parse(alloc, "ctrl=invalid"));
        assert_eq!(set.parse(b"ctrl=invalid"), Err(ParseError::InvalidMod));
        // try testing.expectError(error.InvalidMod, set.parse(alloc, "middle_ctrl=super"));
        assert_eq!(set.parse(b"middle_ctrl=super"), Err(ParseError::InvalidMod));
    }

    // test "RemapSet: isRemapped checks mask"
    #[test]
    fn remap_set_is_remapped_checks_mask() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "ctrl=super");
        set.parse(b"ctrl=super").unwrap();
        // set.finalize();
        set.finalize();

        // try testing.expect(set.isRemapped(.{ .ctrl = true }));
        assert!(set.is_remapped(Mods {
            ctrl: true,
            ..Default::default()
        }));
        // try testing.expect(!set.isRemapped(.{ .alt = true }));
        assert!(!set.is_remapped(Mods {
            alt: true,
            ..Default::default()
        }));
        // try testing.expect(!set.isRemapped(.{ .shift = true }));
        assert!(!set.is_remapped(Mods {
            shift: true,
            ..Default::default()
        }));
    }

    // test "RemapSet: clone creates independent copy"
    #[test]
    fn remap_set_clone_creates_independent_copy() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "ctrl=super");
        set.parse(b"ctrl=super").unwrap();
        // set.finalize();
        set.finalize();

        // var cloned = try set.clone(alloc);
        let cloned = set.clone();

        // try testing.expect(set.equal(cloned));
        assert!(set.equal(&cloned));
        // try testing.expect(cloned.isRemapped(.{ .ctrl = true }));
        assert!(cloned.is_remapped(Mods {
            ctrl: true,
            ..Default::default()
        }));
    }

    // test "RemapSet: equal compares correctly"
    #[test]
    fn remap_set_equal_compares_correctly() {
        // var set1: RemapSet = .empty;
        let mut set1: RemapSet = RemapSet::empty();

        // var set2: RemapSet = .empty;
        let mut set2: RemapSet = RemapSet::empty();

        // try testing.expect(set1.equal(set2));
        assert!(set1.equal(&set2));

        // try set1.parse(alloc, "ctrl=super");
        set1.parse(b"ctrl=super").unwrap();
        // try testing.expect(!set1.equal(set2));
        assert!(!set1.equal(&set2));

        // try set2.parse(alloc, "ctrl=super");
        set2.parse(b"ctrl=super").unwrap();
        // try testing.expect(set1.equal(set2));
        assert!(set1.equal(&set2));

        // try set1.parse(alloc, "alt=shift");
        set1.parse(b"alt=shift").unwrap();
        // try testing.expect(!set1.equal(set2));
        assert!(!set1.equal(&set2));
    }

    // test "RemapSet: parseCLI basic"
    #[test]
    fn remap_set_parse_cli_basic() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parseCLI(alloc, "ctrl=super");
        set.parse_cli(Some(b"ctrl=super")).unwrap();
        // try testing.expectEqual(@as(usize, 2), set.map.count());
        assert_eq!(2usize, set.map.count());
    }

    // test "RemapSet: parseCLI empty clears"
    #[test]
    fn remap_set_parse_cli_empty_clears() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parseCLI(alloc, "ctrl=super");
        set.parse_cli(Some(b"ctrl=super")).unwrap();
        // try testing.expectEqual(@as(usize, 2), set.map.count());
        assert_eq!(2usize, set.map.count());

        // try set.parseCLI(alloc, "");
        set.parse_cli(Some(b"")).unwrap();
        // try testing.expectEqual(@as(usize, 0), set.map.count());
        assert_eq!(0usize, set.map.count());
    }

    // test "RemapSet: parseCLI invalid"
    #[test]
    fn remap_set_parse_cli_invalid() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try testing.expectError(error.InvalidValue, set.parseCLI(alloc, "foo=bar"));
        assert_eq!(
            set.parse_cli(Some(b"foo=bar")),
            Err(ParseCliError::InvalidValue)
        );
        // try testing.expectError(error.InvalidValue, set.parseCLI(alloc, "ctrl"));
        assert_eq!(
            set.parse_cli(Some(b"ctrl")),
            Err(ParseCliError::InvalidValue)
        );
    }

    // test "RemapSet: parse aliased modifiers"
    #[test]
    fn remap_set_parse_aliased_modifiers() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "cmd=ctrl");
        set.parse(b"cmd=ctrl").unwrap();
        // set.finalize();
        set.finalize();

        // const left_super: Mods = .{ .super = true, .sides = .{ .super = .left } };
        let left_super: Mods = Mods {
            super_: true,
            sides: Side {
                super_: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // const left_ctrl: Mods = .{ .ctrl = true, .sides = .{ .ctrl = .left } };
        let left_ctrl: Mods = Mods {
            ctrl: true,
            sides: Side {
                ctrl: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(left_ctrl, set.apply(left_super));
        assert_eq!(left_ctrl, set.apply(left_super));
    }

    // test "RemapSet: parse aliased modifiers command"
    #[test]
    fn remap_set_parse_aliased_modifiers_command() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "command=alt");
        set.parse(b"command=alt").unwrap();
        // set.finalize();
        set.finalize();

        // const left_super: Mods = .{ .super = true, .sides = .{ .super = .left } };
        let left_super: Mods = Mods {
            super_: true,
            sides: Side {
                super_: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // const left_alt: Mods = .{ .alt = true, .sides = .{ .alt = .left } };
        let left_alt: Mods = Mods {
            alt: true,
            sides: Side {
                alt: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(left_alt, set.apply(left_super));
        assert_eq!(left_alt, set.apply(left_super));
    }

    // test "RemapSet: parse aliased modifiers opt and option"
    #[test]
    fn remap_set_parse_aliased_modifiers_opt_and_option() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "opt=super");
        set.parse(b"opt=super").unwrap();
        // set.finalize();
        set.finalize();

        // const left_alt: Mods = .{ .alt = true, .sides = .{ .alt = .left } };
        let left_alt: Mods = Mods {
            alt: true,
            sides: Side {
                alt: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // const left_super: Mods = .{ .super = true, .sides = .{ .super = .left } };
        let left_super: Mods = Mods {
            super_: true,
            sides: Side {
                super_: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(left_super, set.apply(left_alt));
        assert_eq!(left_super, set.apply(left_alt));

        // set.deinit(alloc);
        // set = .empty;
        set = RemapSet::empty();

        // try set.parse(alloc, "option=shift");
        set.parse(b"option=shift").unwrap();
        // set.finalize();
        set.finalize();

        // const left_shift: Mods = .{ .shift = true, .sides = .{ .shift = .left } };
        let left_shift: Mods = Mods {
            shift: true,
            sides: Side {
                shift: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(left_shift, set.apply(left_alt));
        assert_eq!(left_shift, set.apply(left_alt));
    }

    // test "RemapSet: parse aliased modifiers control"
    #[test]
    fn remap_set_parse_aliased_modifiers_control() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "control=super");
        set.parse(b"control=super").unwrap();
        // set.finalize();
        set.finalize();

        // const left_ctrl: Mods = .{ .ctrl = true, .sides = .{ .ctrl = .left } };
        let left_ctrl: Mods = Mods {
            ctrl: true,
            sides: Side {
                ctrl: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // const left_super: Mods = .{ .super = true, .sides = .{ .super = .left } };
        let left_super: Mods = Mods {
            super_: true,
            sides: Side {
                super_: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(left_super, set.apply(left_ctrl));
        assert_eq!(left_super, set.apply(left_ctrl));
    }

    // test "RemapSet: parse aliased modifiers on target side"
    #[test]
    fn remap_set_parse_aliased_modifiers_on_target_side() {
        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(alloc, "alt=cmd");
        set.parse(b"alt=cmd").unwrap();
        // set.finalize();
        set.finalize();

        // const left_alt: Mods = .{ .alt = true, .sides = .{ .alt = .left } };
        let left_alt: Mods = Mods {
            alt: true,
            sides: Side {
                alt: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // const left_super: Mods = .{ .super = true, .sides = .{ .super = .left } };
        let left_super: Mods = Mods {
            super_: true,
            sides: Side {
                super_: ModSide::Left,
                ..Default::default()
            },
            ..Default::default()
        };
        // try testing.expectEqual(left_super, set.apply(left_alt));
        assert_eq!(left_super, set.apply(left_alt));
    }

    // test "RemapSet: formatEntry empty"
    #[test]
    fn remap_set_format_entry_empty() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();

        // const set: RemapSet = .empty;
        let set: RemapSet = RemapSet::empty();
        // try set.formatEntry(formatterpkg.entryFormatter("key-remap", &buf.writer));
        let mut formatter = entry_formatter(b"key-remap", &mut buf);
        set.format_entry(&mut formatter).unwrap();
        // try testing.expectEqualSlices(u8, "key-remap = \n", buf.written());
        assert_eq!(buf.written(), b"key-remap = \n");
    }

    // test "RemapSet: formatEntry single sided"
    #[test]
    fn remap_set_format_entry_single_sided() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();

        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(testing.allocator, "left_ctrl=super");
        set.parse(b"left_ctrl=super").unwrap();
        // set.finalize();
        set.finalize();

        // try set.formatEntry(formatterpkg.entryFormatter("key-remap", &buf.writer));
        let mut formatter = entry_formatter(b"key-remap", &mut buf);
        set.format_entry(&mut formatter).unwrap();
        // try testing.expectEqualSlices(u8, "key-remap = left_ctrl=left_super\n", buf.written());
        assert_eq!(buf.written(), b"key-remap = left_ctrl=left_super\n");
    }

    // test "RemapSet: formatEntry unsided creates two entries"
    #[test]
    fn remap_set_format_entry_unsided_creates_two_entries() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();

        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(testing.allocator, "ctrl=super");
        set.parse(b"ctrl=super").unwrap();
        // set.finalize();
        set.finalize();

        // try set.formatEntry(formatterpkg.entryFormatter("key-remap", &buf.writer));
        let mut formatter = entry_formatter(b"key-remap", &mut buf);
        set.format_entry(&mut formatter).unwrap();
        // Unsided creates both left and right mappings
        // const written = buf.written();
        let written = buf.written();
        // try testing.expect(std.mem.indexOf(u8, written, "left_ctrl=left_super") != null);
        assert!(index_of(written, b"left_ctrl=left_super").is_some());
        // try testing.expect(std.mem.indexOf(u8, written, "right_ctrl=left_super") != null);
        assert!(index_of(written, b"right_ctrl=left_super").is_some());
    }

    // test "RemapSet: formatEntry right sided"
    #[test]
    fn remap_set_format_entry_right_sided() {
        // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
        let mut buf = AllocatingWriter::init();

        // var set: RemapSet = .empty;
        let mut set: RemapSet = RemapSet::empty();

        // try set.parse(testing.allocator, "left_alt=right_ctrl");
        set.parse(b"left_alt=right_ctrl").unwrap();
        // set.finalize();
        set.finalize();

        // try set.formatEntry(formatterpkg.entryFormatter("key-remap", &buf.writer));
        let mut formatter = entry_formatter(b"key-remap", &mut buf);
        set.format_entry(&mut formatter).unwrap();
        // try testing.expectEqualSlices(u8, "key-remap = left_alt=right_ctrl\n", buf.written());
        assert_eq!(buf.written(), b"key-remap = left_alt=right_ctrl\n");
    }

    /// PORT NOTE: `std.mem.indexOf(u8, haystack, needle)` — first index of the
    /// subslice, or `None`. Used by the unsided `formatEntry` test.
    fn index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }
}
