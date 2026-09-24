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
// PORT NOTE: `face/freetype.zig` (the FreeType backend `Face`) is ported in the
// `freetype` submodule below. `face/coretext.zig` / `face/web_canvas.zig` land
// in later F3 subtasks. The `Face = switch(options.backend)` selector still
// resolves to the scaffold placeholder (below) until those backends land; the
// concrete `freetype::Face` is reachable directly as `face::freetype::Face`.
// WASM DIVERGENCE: the FreeType backend face is not compiled on the `wasm32`
// web_canvas backend (no FreeType on freestanding wasm).
// EMBEDDER DIVERGENCE (features `freetype` + `harfbuzz`, both DEFAULT-ON): the
// FreeType backend face also drops out when an embedding application turns the
// vendored-C font backends off. It needs BOTH features — it rasterizes through
// `crate::ft` and shapes through `crate::hb` (`hasHarfbuzz()` is true for every
// freetype-active backend) — and the `Face` selector below resolves to
// `coretext::Face` on the only target where that is allowed (macOS, whose
// default backend is `.coretext`; see this crate's `Cargo.toml` `[features]`).
#[cfg(all(
    not(target_arch = "wasm32"),
    feature = "freetype",
    feature = "harfbuzz"
))]
pub mod freetype;
//
// PORT NOTE: `face/web_canvas.zig` (the browser Canvas backend `Face`) is ported
// in the `web_canvas` submodule below, cfg-gated to wasm — the active `Face` on
// the `web_canvas` backend (`.web_canvas => web_canvas.Face`).
#[cfg(target_arch = "wasm32")]
pub mod web_canvas;
//
// PORT NOTE: `face/coretext.zig` (the CoreText backend `Face`) is ported in the
// `coretext` submodule below (macOS only). Like `freetype::Face`, the concrete
// `coretext::Face` is reachable directly as `face::coretext::Face`; the
// `Face = switch(options.backend)` selector still resolves to the scaffold
// placeholder until the selector is re-pointed at the real backends.
#[cfg(target_os = "macos")]
pub mod coretext;
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
// PORT NOTE: the concrete backend `Face` impls (`face/coretext.zig`,
// `face/freetype.zig`) are ported in the `coretext`/`freetype` submodules, so the
// selector is now re-pointed at the real backend `Face` type. The `switch` above
// is preserved verbatim as a reference; it is modeled here with `cfg(target_os)`
// matching `backend.zig`'s `Backend.default`: macOS default `coretext` →
// `coretext::Face`; Linux default `fontconfig_freetype` → `freetype::Face`. The
// `web_canvas => web_canvas.Face` arm lands with the wasm backend.
#[cfg(target_os = "macos")]
pub type Face = coretext::Face;
// `.web_canvas => web_canvas.Face` (wasm browser backend).
#[cfg(target_arch = "wasm32")]
pub type Face = web_canvas::Face;
#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
pub type Face = freetype::Face;

/// The error type of the active backend `Face` (`coretext::Error` /
/// `freetype::Error`). Introduced so higher-tier callers (Collection,
/// DeferredFace) can compose the backend-face error into their own error sets
/// (the reference threads the backend's inferred error set through `!` unions).
#[cfg(target_os = "macos")]
pub type FaceError = coretext::Error;
#[cfg(target_arch = "wasm32")]
pub type FaceError = web_canvas::Error;
#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
pub type FaceError = freetype::Error;

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
// PORT NOTE (3-way split): `hasFreetype()` is FALSE only on the macOS default
// `coretext` backend and the wasm `web_canvas` backend; it is TRUE on the Linux
// default `fontconfig_freetype`. The three arms below therefore are:
//   * macOS  (`coretext`)            → `void` (Rust `()`), per the reference.
//   * wasm32 (`web_canvas`)          → the local placeholder struct. The
//     `freetype` module is not compiled on wasm (no FreeType on freestanding
//     wasm, see the `mod freetype` gate above) and `face/web_canvas.rs` names
//     `super::Options`/`super::FreetypeLoadFlags` directly, so this arm must keep
//     declaring its own types.
//   * everyone else (`fontconfig_freetype`) → RE-EXPORTS the freetype backend's
//     own `config::FreetypeLoadFlags` stand-in (`face/freetype.rs`). This is a
//     `pub use`, NOT a redeclaration, so `face::FreetypeLoadFlags` and
//     `face::freetype::FreetypeLoadFlags` are the SAME type — which is what lets
//     `face::Options` flow directly into `face::Face::init`/`::init_file` (on this
//     target class `face::Face` IS `freetype::Face`, which takes
//     `freetype::Options`).
#[cfg(target_os = "macos")]
pub type FreetypeLoadFlags = ();

/// Scaffold stand-in for `config::FreetypeLoadFlags` on the wasm `web_canvas`
/// backend (break #9: config injected via DI, so `rustty-config` is not a dep of
/// this tier). `hasFreetype()` is false on `web_canvas`, so this carries no flags.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FreetypeLoadFlags {
    _private: (),
}

/// On the freetype-active backends, `face::FreetypeLoadFlags` IS the freetype
/// backend's `config::FreetypeLoadFlags` stand-in (`face/freetype.rs`).
#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
pub use freetype::FreetypeLoadFlags;

// pub const freetype_load_flags_default: FreetypeLoadFlags = if (FreetypeLoadFlags != void) .{} else {};
#[cfg(target_os = "macos")]
pub const FREETYPE_LOAD_FLAGS_DEFAULT: FreetypeLoadFlags = ();
#[cfg(target_arch = "wasm32")]
pub const FREETYPE_LOAD_FLAGS_DEFAULT: FreetypeLoadFlags = FreetypeLoadFlags { _private: () };
#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
pub const FREETYPE_LOAD_FLAGS_DEFAULT: FreetypeLoadFlags = freetype::FreetypeLoadFlags::DEFAULT;

/// Test-only instrumentation proving the manual-destructor discipline on the
/// recoverable construction/error paths (W9F-02). The backend `Face` types own
/// raw native handles (HarfBuzz font; FT face/mutex on FreeType) that `Drop`
/// does NOT free — teardown must go through `deinit`. These thread-local probes
/// let unit tests assert that an error path actually ran `deinit` on a moved
/// face it refused ownership of (thread-local so parallel tests don't observe
/// each other's faces). Not part of the reference (Zig tests rely on
/// `std.testing.allocator` leak detection instead, which has no Rust analog for
/// non-allocator native handles).
#[cfg(test)]
pub(crate) mod test_probe {
    use core::cell::Cell;

    thread_local! {
        /// Bumped by every backend `Face::deinit` on this thread.
        static FACE_DEINITS: Cell<usize> = const { Cell::new(0) };
        /// Fail-point for the backend `Face::set_size`: `None` = disarmed;
        /// `Some(n)` = the (n+1)-th `set_size` call on this thread fails.
        static FAIL_SET_SIZE: Cell<Option<usize>> = const { Cell::new(None) };
        /// Fail-point for the backend `Face::set_variations`: when set, the
        /// next non-empty `set_variations` call on this thread fails (S85-F3).
        static FAIL_SET_VARIATIONS: Cell<bool> = const { Cell::new(false) };
    }

    /// Arm the `set_variations` fail-point for the next non-empty call.
    pub fn arm_set_variations_failpoint() {
        FAIL_SET_VARIATIONS.with(|c| c.set(true));
    }

    /// Called by the backend `Face::set_variations` impls once past their
    /// empty-list early return; disarms and reports whether this call must fail.
    pub fn set_variations_failpoint_should_fail() -> bool {
        FAIL_SET_VARIATIONS.with(|c| c.replace(false))
    }

    /// Number of `Face::deinit` calls seen on this thread.
    pub fn face_deinits() -> usize {
        FACE_DEINITS.with(|c| c.get())
    }

    /// Called by the backend `Face::deinit` impls.
    pub fn note_face_deinit() {
        FACE_DEINITS.with(|c| c.set(c.get() + 1));
    }

    /// Arm the `set_size` fail-point: the call after `after` successful
    /// countdowns fails (0 = the very next call fails).
    pub fn arm_set_size_failpoint(after: usize) {
        FAIL_SET_SIZE.with(|c| c.set(Some(after)));
    }

    /// Disarm the `set_size` fail-point.
    pub fn disarm_set_size_failpoint() {
        FAIL_SET_SIZE.with(|c| c.set(None));
    }

    /// Called at the top of the backend `Face::set_size` impls; counts down and
    /// reports whether this call must fail.
    pub fn set_size_failpoint_should_fail() -> bool {
        FAIL_SET_SIZE.with(|c| match c.get() {
            None => false,
            Some(0) => true,
            Some(n) => {
                c.set(Some(n - 1));
                false
            }
        })
    }
}

/// Options for initializing a font face.
// pub const Options = struct {
//     size: DesiredSize,
//     freetype_load_flags: FreetypeLoadFlags = freetype_load_flags_default,
// };
#[cfg(any(target_os = "macos", target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub size: DesiredSize,
    // PORT NOTE: Rust structs have no per-field defaults; the reference default
    // of `freetype_load_flags` is `FREETYPE_LOAD_FLAGS_DEFAULT` (callers that
    // omit it supply that constant).
    pub freetype_load_flags: FreetypeLoadFlags,
}

/// On the freetype-active backends, `face::Options` IS `freetype::Options` — the
/// reference's single `font.face.Options` type, which `freetype.Face.init` takes
/// directly. Re-exported (not redeclared) so the two never diverge.
#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
pub use freetype::Options;

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
    // PORT NOTE: this crate has no GTK edge, so it keeps the `.none` arm
    // (`void`, Rust `()`). The `.gtk` arm's boxed type is registered in
    // rustty-apprt as `gtk::class::surface::FontSizeRequest`, under the
    // reference's name `GhosttyFontDesiredSize` (S87-F4). CORRECTION (task
    // T-F): this said "GTK is deferred; the `.gtk` arm's gobject boxed-type
    // registration is ported when the apprt-gtk layer lands, retargeting the
    // `GhosttyFontDesiredSize` name to the rustty identity"; the GTK layer has
    // landed, and GType names keep the `Ghostty` prefix like every other boxed
    // type in rustty-apprt.
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
