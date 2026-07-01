//! rustty-font-shaper::face
//!
//! PORTS: ghostty `src/font/face.zig` (112 loc, 3 tests) — the backend selector.
//! `Face` is aliased to the active backend `Face` type per the build backend;
//! this module also owns the face `Options`, `DesiredSize`, and `Variation`
//! types, `default_dpi`, and the `FreetypeLoadFlags` seam.

// const std = @import("std");
// const builtin = @import("builtin");
// const build_config = @import("../build_config.zig");
// const options = @import("main.zig").options;
// const config = @import("../config.zig");
// const freetype = @import("face/freetype.zig");
// const coretext = @import("face/coretext.zig");
// pub const web_canvas = @import("face/web_canvas.zig");
//
// PORT NOTE: the reference imports the two build seams `options` (from
// `main.zig`, providing `options.backend`) and `config` (providing
// `config.FreetypeLoadFlags`) plus the three concrete backend faces. In rustty:
//   * `options.backend` — the build backend — is modeled by `cfg(target_os)`
//     below, matching `backend.zig`'s `Backend.default` (macOS default=`coretext`,
//     Linux default=`fontconfig_freetype`); parity targets both defaults.
//   * `config.FreetypeLoadFlags` lives in the higher `rustty-config` crate; per
//     break #9 (config injected via DI) it is NOT a dependency of this tier, so
//     the freetype-active arm is seamed (see `FreetypeLoadFlags` below).
//   * `web_canvas` is the wasm face (feature-gated OFF); its port lands with the
//     wasm/web_canvas backend and is recorded here as a reference import.

/// Face implementation for the compile options.
// pub const Face = switch (options.backend) {
//     .freetype,
//     .freetype_windows,
//     .fontconfig_freetype,
//     .coretext_freetype,
//     => freetype.Face,
//
//     .coretext,
//     .coretext_harfbuzz,
//     .coretext_noshape,
//     => coretext.Face,
//
//     .web_canvas => web_canvas.Face,
// };
//
// PORT NOTE (F3 scaffold): the concrete backend `Face` impls (`face/coretext.zig`,
// `face/freetype.zig`, `face/web_canvas.zig`) are ported in later F3 subtasks.
// Until they land, this selector is wired to the scaffold placeholder
// `BackendFace` so the backend matrix is expressed and gated now. No reference
// logic is lost: the `switch` above is preserved verbatim and is re-pointed at
// the real `coretext::Face` / `freetype::Face` / `web_canvas::Face` when those
// files are ported. Every arm currently resolves to the same placeholder.
pub type Face = BackendFace;

/// Scaffold placeholder for the not-yet-ported backend `Face` implementations.
/// Replaced by `coretext::Face` / `freetype::Face` / `web_canvas::Face` (per the
/// build backend) as those files are ported in later F3 subtasks.
#[derive(Debug)]
pub struct BackendFace {
    // Private: the placeholder holds no state; it exists only so the backend
    // selector `Face` alias resolves and the scaffold gates green.
    _private: (),
}

/// If a DPI can't be calculated, this DPI is used. This is probably
/// wrong on modern devices so it is highly recommended you get the DPI
/// using whatever platform method you can.
// pub const default_dpi = if (builtin.os.tag == .macos) 72 else 96;
#[cfg(target_os = "macos")]
pub const DEFAULT_DPI: u16 = 72;
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_DPI: u16 = 96;

/// These are the flags to customize how freetype loads fonts. This is
/// only non-void if the freetype backend is enabled.
// pub const FreetypeLoadFlags = if (options.backend.hasFreetype())
//     config.FreetypeLoadFlags
// else
//     void;
//
// PORT NOTE: `options.backend.hasFreetype()` (backend.zig) is true for the
// freetype/freetype_windows/fontconfig_freetype/coretext_freetype backends. The
// build backend drives it: macOS default `coretext` → false → `void` (Rust `()`);
// Linux default `fontconfig_freetype` → true → `config.FreetypeLoadFlags`. That
// config type lives in the higher `rustty-config` crate (break #9, injected via
// DI), so it is NOT a dependency here; the freetype-active arm uses the local
// `FreetypeLoadFlags` placeholder — a faithful stand-in for
// `config::FreetypeLoadFlags`, wired to the real type when the Linux freetype
// backend + config DI land.
#[cfg(target_os = "macos")]
pub type FreetypeLoadFlags = ();

/// Scaffold stand-in for `config::FreetypeLoadFlags` on freetype-active backends
/// (break #9: config injected via DI, so `rustty-config` is not a dep of this
/// tier). Replaced by the real config type when the Linux freetype backend lands.
#[cfg(not(target_os = "macos"))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FreetypeLoadFlags {
    _private: (),
}

// pub const freetype_load_flags_default: FreetypeLoadFlags = if (FreetypeLoadFlags != void) .{} else {};
#[cfg(target_os = "macos")]
pub const FREETYPE_LOAD_FLAGS_DEFAULT: FreetypeLoadFlags = ();
#[cfg(not(target_os = "macos"))]
pub const FREETYPE_LOAD_FLAGS_DEFAULT: FreetypeLoadFlags = FreetypeLoadFlags { _private: () };

/// Options for initializing a font face.
// pub const Options = struct {
//     size: DesiredSize,
//     freetype_load_flags: FreetypeLoadFlags = freetype_load_flags_default,
// };
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub size: DesiredSize,
    // PORT NOTE: Rust structs have no per-field defaults; the reference default
    // of `freetype_load_flags` is `FREETYPE_LOAD_FLAGS_DEFAULT` (callers that
    // omit it supply that constant).
    pub freetype_load_flags: FreetypeLoadFlags,
}

/// The desired size for loading a font.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesiredSize {
    // Desired size in points
    pub points: f32,

    // The DPI of the screen so we can convert points to pixels.
    // (reference field defaults: `xdpi`/`ydpi` = `default_dpi`)
    pub xdpi: u16,
    pub ydpi: u16,
}

impl DesiredSize {
    // Converts points to pixels
    pub fn pixels(self: DesiredSize) -> f32 {
        // 1 point = 1/72 inch
        (self.points * self.ydpi as f32) / 72.0
    }

    /// Make this a valid gobject if we're in a GTK environment.
    // pub const getGObjectType = switch (build_config.app_runtime) {
    //     .gtk => @import("gobject").ext.defineBoxed(
    //         DesiredSize,
    //         .{ .name = "GhosttyFontDesiredSize" },
    //     ),
    //
    //     .none => void,
    // };
    //
    // PORT NOTE: `build_config.app_runtime` is `.none` in this build (GTK is
    // deferred; the `.gtk` arm's gobject boxed-type registration is ported when
    // the apprt-gtk layer lands, retargeting the `GhosttyFontDesiredSize` name to
    // the rustty identity). `.none` → `void` (Rust `()`).
    pub const GET_G_OBJECT_TYPE: () = ();
}

/// A font variation setting. The best documentation for this I know of
/// is actually the CSS font-variation-settings property on MDN:
/// https://developer.mozilla.org/en-US/docs/Web/CSS/font-variation-settings
// pub const Variation = struct {
//     id: Id,
//     value: f64,
//     ...
// };
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variation {
    pub id: Id,
    pub value: f64,
}

/// PORT NOTE: the reference nests this as `Variation.Id`; Rust has no nested
/// type declarations, so it is a sibling type (referenced as `Variation.Id` in
/// Zig, `Id` here).
// pub const Id = packed struct(u32) {
//     d: u8,
//     c: u8,
//     b: u8,
//     a: u8,
//     ...
// };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Id {
    pub d: u8,
    pub c: u8,
    pub b: u8,
    pub a: u8,
}

impl Id {
    // pub fn init(v: *const [4]u8) Id {
    //     return .{ .a = v[0], .b = v[1], .c = v[2], .d = v[3] };
    // }
    pub fn init(v: &[u8; 4]) -> Id {
        Id {
            a: v[0],
            b: v[1],
            c: v[2],
            d: v[3],
        }
    }

    /// Converts the ID to a string. The return value is only valid
    /// for the lifetime of the self pointer.
    // pub fn str(self: Id) [4]u8 {
    //     return .{ self.a, self.b, self.c, self.d };
    // }
    pub fn str(self: Id) -> [u8; 4] {
        [self.a, self.b, self.c, self.d]
    }

    // PORT NOTE: the reference tests use `@as(u32, @bitCast(id))` over the
    // `packed struct(u32) { d, c, b, a }`. In a Zig packed struct the FIRST
    // field occupies the least-significant bits, so `d` is the low byte and `a`
    // the high byte: value = (a<<24) | (b<<16) | (c<<8) | d. This method
    // reproduces that bit layout so the ported tests assert the same u32.
    pub fn to_u32(self: Id) -> u32 {
        ((self.a as u32) << 24) | ((self.b as u32) << 16) | ((self.c as u32) << 8) | (self.d as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test {
    //     @import("std").testing.refAllDecls(@This());
    // }
    //
    // PORT NOTE: `refAllDecls` is a Zig comptime reflection smoke test that forces
    // every declaration in the module to be analyzed. Rust type-checks every item
    // unconditionally, so the structural guarantee is automatic; we mirror it
    // concretely by touching each public decl so a removed/renamed item surfaces
    // as a test failure rather than only a compile error.
    #[test]
    fn ref_all_decls() {
        let _ = core::mem::size_of::<Face>();
        let _ = core::mem::size_of::<BackendFace>();
        let _ = DEFAULT_DPI;
        let _ = core::mem::size_of_val(&FREETYPE_LOAD_FLAGS_DEFAULT);
        let _ = core::mem::size_of::<FreetypeLoadFlags>();
        let _ = core::mem::size_of::<Options>();
        let _ = core::mem::size_of::<DesiredSize>();
        let _ = DesiredSize::pixels as fn(DesiredSize) -> f32;
        let _: () = DesiredSize::GET_G_OBJECT_TYPE;
        let _ = core::mem::size_of::<Variation>();
        let _ = Id::init as fn(&[u8; 4]) -> Id;
        let _ = Id::str as fn(Id) -> [u8; 4];
        let _ = Id::to_u32 as fn(Id) -> u32;
    }

    // test "Variation.Id: wght should be 2003265652" {
    //     const testing = std.testing;
    //     const id = Variation.Id.init("wght");
    //     try testing.expectEqual(@as(u32, 2003265652), @as(u32, @bitCast(id)));
    //     try testing.expectEqualStrings("wght", &(id.str()));
    // }
    #[test]
    fn variation_id_wght_should_be_2003265652() {
        let id = Id::init(b"wght");
        assert_eq!(2003265652u32, id.to_u32());
        assert_eq!(b"wght", &id.str());
    }

    // test "Variation.Id: slnt should be 1936486004" {
    //     const testing = std.testing;
    //     const id: Variation.Id = .{ .a = 's', .b = 'l', .c = 'n', .d = 't' };
    //     try testing.expectEqual(@as(u32, 1936486004), @as(u32, @bitCast(id)));
    //     try testing.expectEqualStrings("slnt", &(id.str()));
    // }
    #[test]
    fn variation_id_slnt_should_be_1936486004() {
        let id = Id {
            a: b's',
            b: b'l',
            c: b'n',
            d: b't',
        };
        assert_eq!(1936486004u32, id.to_u32());
        assert_eq!(b"slnt", &id.str());
    }
}
