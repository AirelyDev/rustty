//! A noop implementation of the keymap interface so that the embedded
//! library can compile on non-macOS platforms.
//
// PORTS: ghostty src/input/KeymapNoop.zig
//
// PORT STATUS: done
//
// PORT NOTE: the reference file IS the `KeymapNoop` struct via
//   const KeymapNoop = @This();
// and imports
//   const Mods = @import("key.zig").Mods;
// `Mods` maps to the ported `rustty_vt::input::key_mods::Mods`. The empty
// `State`/zero-field `Translation` default map directly. `init`/`reload`
// are infallible here (the `!` error sets carry no error tags); the `out`,
// `state`, `code`, `mods` params to `translate` are unused (`_ = ...`), so
// they are prefixed with `_`.

use rustty_vt::input::key_mods::Mods;

/// PORT NOTE: the reference is a zero-field struct.
pub struct KeymapNoop;

pub struct State {}

//
// PORT NOTE: the reference field defaults are
//   text: []const u8 = "", composing: bool = false, mods: Mods = .{}
// which are exactly the Rust-derived defaults, so `Default` is derived.
#[derive(Default)]
pub struct Translation {
    pub text: Vec<u8>,
    pub composing: bool,
    pub mods: Mods,
}

// PORT NOTE: the reference `init`/`reload`/`translate` return Zig error unions
// (`!KeymapNoop`, `!void`, `!Translation`) whose error set carries no tags here
// (the noop never fails). The signature shape is preserved as `Result<_, ()>`
// to mirror the interface implemented by `KeymapDarwin`; the unit error is
// intentional, hence the per-occurrence `clippy::result_unit_err` allow.
#[allow(clippy::result_unit_err)]
impl KeymapNoop {
    pub fn init() -> Result<KeymapNoop, ()> {
        Ok(KeymapNoop {})
    }

    pub fn deinit(&self) {}

    pub fn reload(&mut self) -> Result<(), ()> {
        Ok(())
    }

    pub fn translate(
        &self,
        _out: &mut [u8],
        _state: &mut State,
        _code: u16,
        _mods: Mods,
    ) -> Result<Translation, ()> {
        // return .{ .text = "", .composing = false };
        Ok(Translation {
            text: Vec::new(),
            composing: false,
            ..Default::default()
        })
    }
}
