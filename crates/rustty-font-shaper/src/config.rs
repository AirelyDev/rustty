//! rustty-font-shaper::config — local faithful stand-ins for the handful of
//! `rustty-config` value types that `SharedGridSet` reads.
//!
//! PORT NOTE (break #9 — config injected via DI): the reference `SharedGridSet.zig`
//! imports `configpkg = @import("../config.zig")` and names `Config`,
//! `RepeatableString`, `FontStyle`, `RepeatableFontVariation`,
//! `RepeatableCodepointMap`, and `FontSyntheticStyle`. Those live in the HIGHER
//! `rustty-config` crate, which depends on this font tier — so this tier must NOT
//! depend on it (that would be a cycle, G7). We therefore model the exact subset
//! of those config types that `SharedGridSet` + its ported tests exercise, the
//! same DI-break pattern used for `collection::FontSyntheticStyle`. When the full
//! `rustty-config` port lands it supplies the real types via injection; the
//! observable behavior modeled here is identical.
//!
//! Only the surface actually touched is modeled: the `list`/`map` fields, the
//! `nameValue`/`clone` accessors, and the `default` constructor. The reference's
//! `parseCLI`/`equal`/`formatEntry`/`test` blocks belong to `rustty-config` and
//! are ported there, not duplicated into this stand-in.

use crate::face::Variation;
use rustty_font::codepoint_map::CodepointMap;

/// `configpkg.RepeatableString` — an ordered list of strings (the `font-family`
/// family of options). The reference stores `[:0]const u8` items; here they are
/// owned NUL-free byte strings (`Vec<u8>`), deep-cloned by `#[derive(Clone)]`.
// pub const RepeatableString = struct {
//     list: std.ArrayListUnmanaged([:0]const u8) = .{},
//     ...
// };
#[derive(Clone, Debug, Default)]
pub struct RepeatableString {
    /// The list of strings. (Reference: `list.items`.)
    pub list: Vec<Vec<u8>>,
}

/// `configpkg.FontStyle` — the `font-style*` options.
// pub const FontStyle = union(enum) { default: void, false: void, name: [:0]const u8 };
// PORT NOTE: the reference default for the `font-style*` fields is `.{ .default = {} }`
// → the `Default` variant (`#[default]`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum FontStyle {
    /// Use the default font style that font discovery finds.
    #[default]
    Default,
    /// Disable this font style completely. This will fall back to using
    /// the regular font when this style is encountered.
    False,
    /// A specific named font style to use for this style.
    Name(Vec<u8>),
}

impl FontStyle {
    /// Returns the string name value that can be used with a font
    /// descriptor.
    // pub fn nameValue(self: Self) ?[:0]const u8 {
    //     return switch (self) { .default, .false => null, .name => self.name };
    // }
    pub fn name_value(&self) -> Option<Vec<u8>> {
        match self {
            FontStyle::Default | FontStyle::False => None,
            FontStyle::Name(v) => Some(v.clone()),
        }
    }
}

/// `configpkg.RepeatableFontVariation` — the `font-variation*` options.
// pub const RepeatableFontVariation = struct {
//     list: std.ArrayListUnmanaged(fontpkg.face.Variation) = .{},
//     ...
// };
#[derive(Clone, Debug, Default)]
pub struct RepeatableFontVariation {
    /// The list of variations. (Reference: `list.items`.)
    pub list: Vec<Variation>,
}

/// `configpkg.RepeatableCodepointMap` — the `font-codepoint-map` option.
// pub const RepeatableCodepointMap = struct { map: fontpkg.CodepointMap = .{}, ... };
//
// PORT NOTE: `CodepointMap` deep-clones via its `clone(alloc)` method (it does not
// derive `Clone`), so `RepeatableCodepointMap` provides `clone_config` rather than
// a derived `Clone`.
#[derive(Debug, Default)]
pub struct RepeatableCodepointMap {
    /// The codepoint map. (Reference: `map`.)
    pub map: CodepointMap,
}

impl RepeatableCodepointMap {
    /// Deep copy. (Reference: `clone(self, alloc) !Self` → `.{ .map = try self.map.clone(alloc) }`.)
    // PORT NOTE: allocation aborts on OOM in this tier (the reference returns the
    // error); an empty map (the default) clones trivially.
    pub fn clone_config(&self) -> RepeatableCodepointMap {
        RepeatableCodepointMap {
            map: self.map.clone(allocator_api2::alloc::Global).unwrap(),
        }
    }
}

// PORT NOTE: the reference `Config` is the giant global terminal configuration
// (`config.zig`'s `Config`); `SharedGridSet` only reads its font-config subset via
// `DerivedConfig`. In rustty that modeled subset is exactly `DerivedConfig` (see
// `shared_grid_set.rs`), so `Config` is a type alias for it there — there is no
// separate `Config` struct to define here.
