//! rustty-config::value_types
//!
//! PORTS: the config value-types defined *in-file* inside ghostty
//! `src/config/Config.zig` (i.e. the `pub const X = enum/struct/union/packed`
//! definitions that the giant `Config` struct composes). The C1 leaf value-types
//! that live in their own ghostty files (string/path/io/command/conditional/
//! RepeatableStringMap/ClipboardCodepointMap/...) are ported in their own modules
//! and reused here.
//!
//! PORT STATUS: C2 SKELETON. Every type is laid down byte-faithfully (exact enum
//! variants, struct/union fields, defaults, and doc comments per R1). The simple
//! enums are complete; the ~20 types with real `parseCLI`/`clone`/`equal`/
//! `formatEntry` carry their field/variant definitions plus method SIGNATURES as
//! `todo!()`. The method bodies + every `test {}` (R1, count is a contract) are
//! filled in the subsequent fill sub-batches under scoped G3b.
//!
//! Modeling follows the panel-signed-off Decision B: the four comptime passes
//! (PARSE/FORMAT/CLONE/EQUAL-DIFF) are threaded dynamically through the
//! rustty-args `Target` trait + per-type inherent impls; here we lay the data.

#![allow(dead_code)]
#![allow(clippy::enum_variant_names)]

use rustty_args::args::{
    parse_auto_struct, parse_bool, AutoStruct, Error, PackedStruct, ParseError, WHITESPACE,
};
use rustty_cfg_fmt::{
    format_entry_display, format_entry_packed_struct, format_entry_string, format_entry_void,
    FormatEntry, FormatError, Writer,
};
// PORTS: `u21` (Zig's 21-bit codepoint type) -> `rustty_vt::page::u21` (= u32).
use rustty_vt::page::u21;

// PORT NOTE (R1): the value-type parse/format/clone/equal bodies below are the
// fill of the C2 skeleton (sub-batch). Each reproduces the reference method
// assertion-for-assertion; the four comptime passes are threaded via the
// rustty-args `Target`/`AutoStruct` traits and the rustty-cfg-fmt `FormatEntry`
// trait + standalone helpers, exactly as the leaf value-types (string/path/
// RepeatableStringMap/...) already do.

// PORTS: ghostty src/config/limit.zig (1092204df).
//
// PORT NOTE: the reference is `pub fn Limit(comptime T: type, comptime
// default_value: T) type` — a comptime type constructor whose default lives in
// the type. Rust const generics cannot be generic over their own type parameter
// (`const DEFAULT: T` is rejected), so the default is carried as a `u128` const
// parameter and converted back through [`LimitInt`]; `T` remains a real type
// parameter, so the reference's `Limit(u16, 42)` is spelled `Limit<u16, 42>`.
// `comptime std.debug.assert(@typeInfo(T) == .int)` is enforced by the
// [`LimitInt`] bound, which is only implemented for integer types.

/// `Limit(usize, 50_000_000)` — the `scrollback-limit-bytes` config type.
pub type ScrollbackLimitBytes = Limit<usize, 50_000_000>;

/// `Limit(usize, std.math.maxInt(usize))` — the `scrollback-limit-lines`
/// config type. Its declared default IS the unlimited sentinel.
pub type ScrollbackLimitLines = Limit<usize, { usize::MAX as u128 }>;

/// `Limit(usize, 64 * 1024 * 1024)` — the `clipboard-write-limit-bytes`
/// config type (`5350d4a5f`, default raised to the 64 MiB protocol minimum by
/// `70f006575`).
pub type ClipboardWriteLimitBytes = Limit<usize, { 64 * 1024 * 1024 }>;

/// The integer capability [`Limit`] needs: its maximum value (the `unlimited`
/// sentinel), conversion from the `u128` const default, and CLI parsing.
pub trait LimitInt: Copy + PartialEq + core::fmt::Display {
    const LIMIT_MAX: Self;
    fn from_default(v: u128) -> Self;
    fn parse_limit(input: &[u8]) -> Result<Self, ParseError>;
}

macro_rules! impl_limit_int {
    ($($t:ty),*) => {$(
        impl LimitInt for $t {
            const LIMIT_MAX: $t = <$t>::MAX;
            fn from_default(v: u128) -> $t {
                v as $t
            }
            // PORT NOTE (LEDGER S78-F2): `std.fmt.parseInt(T, input, 0)`. This
            // was `parse_field_int` (rustty-args' `from_str_radix` path), which
            // rejects the `_` separators and `-0` that `parseInt` accepts, so
            // `scrollback-limit-bytes = 50_000_000` was a config error.
            // It calls `rustty_zigstd::fmt::parse_int`, the port of
            // `std.fmt.parseInt` itself (NEW-FINDINGS N120).
            fn parse_limit(input: &[u8]) -> Result<$t, ParseError> {
                // .value = std.fmt.parseInt(T, input, 0) catch return error.InvalidValue,
                rustty_zigstd::fmt::parse_int::<$t>(input, 0).map_err(|_| Error::InvalidValue)
            }
        }
    )*};
}
impl_limit_int!(u8, u16, u32, u64, usize);

/// A configurable integer limit with a declared default and support for the
/// special value `unlimited`.
///
/// Unlimited is stored as the maximum value of the integer type so the value
/// remains directly comparable without optional unwrapping.
// pub fn Limit(comptime T: type, comptime default_value: T) type {
//     return struct { value: T = default_value; ... };
// }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limit<T: LimitInt, const DEFAULT: u128> {
    // value: T = default_value,
    pub value: T,
}

impl<T: LimitInt, const DEFAULT: u128> Limit<T, DEFAULT> {
    /// The limit initialized with its declared default value.
    // pub const default: Self = .{};
    pub fn default_value() -> Self {
        Limit {
            value: T::from_default(DEFAULT),
        }
    }

    /// Parses an integer limit or the special value "unlimited".
    // pub fn parseCLI(input_: ?[]const u8) !Self
    pub fn parse_cli(input_: Option<&[u8]>) -> Result<Self, ParseError> {
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(Error::ValueRequired)?;
        // if (std.mem.eql(u8, input, "unlimited")) {
        //     return .{ .value = std.math.maxInt(T) };
        // }
        if input == b"unlimited" {
            return Ok(Limit {
                value: T::LIMIT_MAX,
            });
        }

        // return .{ .value = std.fmt.parseInt(T, input, 0) catch
        //     return error.InvalidValue };
        Ok(Limit {
            value: T::parse_limit(input).map_err(|_| Error::InvalidValue)?,
        })
    }

    /// Formats the maximum integer value as "unlimited" and all other
    /// values as integers.
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // if (self.value == std.math.maxInt(T)) {
        //     try formatter.formatEntry([]const u8, "unlimited");
        // } else {
        //     try formatter.formatEntry(T, self.value);
        // }
        if self.value == T::LIMIT_MAX {
            format_entry_string(name, b"unlimited", writer)
        } else {
            format_entry_display(name, &self.value, writer)
        }
    }

    /// Returns the configured value, or null for the unlimited sentinel.
    // pub fn optional(self: Self) ?T
    pub fn optional(&self) -> Option<T> {
        // if (self.value == std.math.maxInt(T)) return null;
        // return self.value;
        if self.value == T::LIMIT_MAX {
            None
        } else {
            Some(self.value)
        }
    }

    /// Returns an independent copy of this value for Config cloning.
    // pub fn clone(self: Self, _: std.mem.Allocator) std.mem.Allocator.Error!Self
    pub fn clone_in(&self) -> Self {
        *self
    }
}

impl<T: LimitInt, const DEFAULT: u128> Default for Limit<T, DEFAULT> {
    fn default() -> Self {
        Self::default_value()
    }
}

impl<T: LimitInt, const DEFAULT: u128> FormatEntry for Limit<T, DEFAULT> {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        Limit::format_entry(self, name, writer)
    }
}

/// PORTS: `std.fs.max_path_bytes` — the platform maximum path length used to
/// size stack path buffers (4096 on Linux/macOS).
const MAX_PATH_BYTES: usize = 4096;

/// PORTS: `std.mem.trim(u8, s, " \t")` — the per-method whitespace trim used
/// throughout the reference value-type parsers.
fn trim_ws(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && (s[start] == b' ' || s[start] == b'\t') {
        start += 1;
    }
    while end > start && (s[end - 1] == b' ' || s[end - 1] == b'\t') {
        end -= 1;
    }
    &s[start..end]
}

/// PORTS: `std.mem.trim(u8, s, &std.ascii.whitespace)` — `std.ascii.whitespace`
/// is `{ ' ', '\t', '\n', '\r', 11, 12 }` (space, tab, LF, CR, VT, FF).
fn trim_ascii_ws(s: &[u8]) -> &[u8] {
    fn is_ws(c: u8) -> bool {
        matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
    }
    let mut start = 0;
    let mut end = s.len();
    while start < end && is_ws(s[start]) {
        start += 1;
    }
    while end > start && is_ws(s[end - 1]) {
        end -= 1;
    }
    &s[start..end]
}

// ===========================================================================
// External value-types referenced by Config fields.
//
// PORT NOTE (R3/DI seam): some font-coupled types are not yet ported in their
// home crate (rustty-font). Per the font crate's DI design and the task's "model
// the data here, no config->font hard type-dep" guidance, those are modeled as
// local skeleton types carrying their reference field shape, to be wired to the
// real font types when those land. `MetricModifier` IS available
// (`rustty_font::metrics::Modifier`) and is reused directly.
// ===========================================================================

// const MetricModifier = fontpkg.Metrics.Modifier;
pub use rustty_font::metrics::Modifier as MetricModifier;

// pub const WindowPaddingBalance = @import("../renderer/size.zig").PaddingBalance;
pub use rustty_render_types::PaddingBalance as WindowPaddingBalance;

// const KeyRemapSet = @import("../input/key_mods.zig").RemapSet;
pub use rustty_input::key_mods::RemapSet as KeyRemapSet;

// PORT NOTE: `fontpkg.face.Variation` is not yet ported in rustty-font. Skeleton
// placeholder modeling `pub const Variation = struct { id: u32, value: f64 }`
// (ghostty src/font/face.zig:74). Wired to the real type when face.zig lands.
//
// The reference `Id` is `packed struct(u32) { d: u8, c: u8, b: u8, a: u8 }` with
// `init(v: *const [4]u8) = .{ .a = v[0], .b = v[1], .c = v[2], .d = v[3] }`. As a
// little-endian packed struct the in-memory `u32` is `a<<24 | b<<16 | c<<8 | d`
// (e.g. "wght" -> 0x77676874 = 2003265652). The skeleton models `id` as that
// `u32`; `face_variation_id_init`/`face_variation_id_str` reproduce `Id.init`/
// `Id.str` against that representation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceVariation {
    pub id: u32,
    pub value: f64,
}

/// PORTS: `fontpkg.face.Variation.Id.init(v: *const [4]u8)`.
fn face_variation_id_init(v: &[u8; 4]) -> u32 {
    // .{ .a = v[0], .b = v[1], .c = v[2], .d = v[3] } -> a<<24 | b<<16 | c<<8 | d
    (v[0] as u32) << 24 | (v[1] as u32) << 16 | (v[2] as u32) << 8 | (v[3] as u32)
}

/// PORTS: `fontpkg.face.Variation.Id.str(self) [4]u8`.
fn face_variation_id_str(id: u32) -> [u8; 4] {
    // return .{ self.a, self.b, self.c, self.d };
    [
        (id >> 24) as u8,
        (id >> 16) as u8,
        (id >> 8) as u8,
        id as u8,
    ]
}

// PORT NOTE: `fontpkg.CodepointMap` (`font/CodepointMap.zig`) is ported PURE in
// rustty-font; re-export it under the reference name. Its discovery-descriptor
// seam (`Descriptor`) carries `family` + `monospace` (the two fields config's
// `RepeatableCodepointMap.parseCLI` sets); the full fontconfig/CoreText discovery
// lands later in the shaper tier — rustty-font stays pure (no C/vt edge).
pub use rustty_font::codepoint_map::CodepointMap;

// ===========================================================================
// Bare-enum name maps (the PARSE/FORMAT name tables).
//
// PORTS: for the config enums WITHOUT a custom `parseCLI`, the reference parses
// them in `cli.args.parseIntoField` via `std.meta.stringToEnum(Field, value)`
// and formats them via `@tagName(self)`. Both use the Zig field-name spelling,
// which for hyphenated variants is the HYPHENATED literal (e.g. `new-tab`,
// `linear-corrected`, `16-bit`) — NOT the Rust ident. Rust has no enum
// reflection, so each bare enum gets an explicit name map generated by the
// `bare_enum_names!` macro below:
//
//   - `string_to_enum(&[u8]) -> Option<Self>`  ==  `std.meta.stringToEnum`
//   - `to_name(self) -> &'static [u8]`          ==  `@tagName(self)`
//
// The two maps are inverse over the SAME literal table, so PARSE and FORMAT are
// guaranteed symmetric (the reference's `stringToEnum`/`@tagName` derive from the
// same comptime field names). Exhaustiveness of `to_name` is enforced by the
// match having one arm per variant (a missing variant is a compile error).
// ===========================================================================

/// Copies `s` into a fixed `[u8; N]` whose tail byte stays `0` — the NUL
/// terminator. `N` must be `s.len() + 1`. This is the const builder behind the
/// `to_name_z` arms: the reference's `@tagName` is a `[:0]const u8` (a
/// sentinel-terminated string literal), so the C-API enum path needs a
/// NUL-terminated copy of the un-terminated `to_name` slice.
pub const fn name_z_array<const N: usize>(s: &[u8]) -> [u8; N] {
    let mut out = [0u8; N];
    let mut i = 0;
    while i < s.len() {
        out[i] = s[i];
        i += 1;
    }
    out
}

/// Generates `string_to_enum` (== `std.meta.stringToEnum`) and `to_name`
/// (== `@tagName`) for a bare config enum, over the exact Zig variant-name
/// literals. Each `Variant => b"name"` pair maps a Rust variant to its Zig
/// field-name spelling (hyphenated for hyphenated Zig variants).
macro_rules! bare_enum_names {
    ($ty:ty { $($variant:ident => $name:literal),+ $(,)? }) => {
        impl $ty {
            /// `@typeInfo(E).@"enum".fields |f| f.name` — the ordered list of Zig
            /// variant-name literals in declaration order. Feeds the shell-completion
            /// generators (`rustty-extra`): the `.@"enum"` arm of the reference's
            /// `writeBashCompletions`/`writeZshCompletions`/`writeCompletions` emits
            /// exactly these names, space-joined. The literal table is the SAME one
            /// `string_to_enum`/`to_name` use, so the completion bytes == `@tagName`.
            pub const VALUE_NAMES: &'static [&'static [u8]] = &[$($name),+];

            /// `std.meta.stringToEnum(Self, input)` — maps the Zig variant-name
            /// literal to the variant, `None` for an unknown name.
            pub fn string_to_enum(input: &[u8]) -> Option<$ty> {
                match input {
                    $($name => Some(<$ty>::$variant),)+
                    _ => None,
                }
            }

            /// `@tagName(self)` — the Zig variant-name literal for this variant.
            pub fn to_name(self) -> &'static [u8] {
                match self {
                    $(<$ty>::$variant => $name,)+
                }
            }

            /// `@tagName(self)` as a NUL-terminated `[*:0]const u8` for the C API
            /// (`c_get`'s `.@"enum"` arm). Each variant's name is materialized as a
            /// `'static` NUL-terminated array, matching the reference's sentinel
            /// `@tagName` pointer.
            pub fn to_name_z(self) -> *const std::os::raw::c_char {
                match self {
                    $(<$ty>::$variant => {
                        static Z: [u8; $name.len() + 1] =
                            $crate::value_types::name_z_array($name);
                        Z.as_ptr().cast()
                    })+
                }
            }
        }
    };
}

mod colors;
mod enums;
mod keybinds;
mod packed;
mod repeatable;
mod unions;
mod window;

pub use colors::*;
pub use enums::*;
pub use keybinds::*;
pub use packed::*;
pub use repeatable::*;
pub use unions::*;
pub use window::*;

// ===========================================================================
// Value-type tests (R1: every in-type `test {}` block ported, assertion-for-
// assertion, with INDEPENDENT expected values). The reference defines these
// inside each value type's struct; here they are collected in one module and
// named with the reference test's slug, prefixed by the type, to disambiguate.
//
// DEFERRED (not ported in this sub-batch): the `Keybinds` value-type tests, whose
// impl is a deferred DI/infra seam (see the PORT NOTE on that type).
// `RepeatableCommand`'s tests ARE ported below (the `Command: AutoStruct` seam is
// resolved); `RepeatableCodepointMap`'s tests ARE ported below (its
// `fontpkg.CodepointMap` seam is now ported PURE in rustty-font).
// ===========================================================================
#[cfg(test)]
mod tests;
