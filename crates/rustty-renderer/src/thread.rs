//! Represents the renderer thread logic. The renderer thread is able to
//! be woken up to render.
//!
//! PORTS: ghostty src/renderer/Thread.zig — the part of it that can live in
//! THIS crate: the `Mailbox` type (what termio's `renderer_mailbox` points at)
//! and the `DerivedConfig` (the real single-field struct + `init`).
//!
//! WHERE THE REST OF THE FILE IS — read this before concluding anything is
//! missing. The reference file's event loop (the `Thread` struct itself, its
//! xev `Loop`/`Async`/`Timer` handles, `threadMain`/`threadMain_`/
//! `drainMailbox`/`changeConfig`/`drawFrame`/`setQosClass`, the wakeup/
//! draw-now/render/animation/cursor/stop callbacks, `armAnimationTimer`,
//! `cursorBlinkInterval`, `CURSOR_BLINK_INTERVAL` and the `Compression`
//! scheduler) is FULLY PORTED — in `rustty-core`'s `renderer_seam.rs`, not
//! here. All 17 reference `fn`s plus `Compression`'s six are live there with
//! ZERO production `unimplemented!()`.
//!
//! It lives in core because renderer/Thread.zig imports `configpkg` AND core's
//! `App.Mailbox`, and BOTH edges cycle from `rustty-renderer` (rustty-config
//! already deps rustty-renderer — the G7 cycle — and a renderer -> core edge is
//! upside down). R4 therefore puts the loop at the lowest crate that may name
//! all three: core.
//!
//! (Superseded status, kept because it misled a 2026-08-26 parity audit into
//! reporting the loop as unported: ~~"NOT ported here — deferred with the D6
//! xev decision + the GPU renderer tier"~~. D6 Milestones A/B and the R-d GPU
//! tier W0–W5 have since landed; `.port/phase-2/DEFERRALS.md` carries the
//! matching RESOLVED row.)

use rustty_datastruct::BlockingQueue;

use crate::config::RendererConfig;
use crate::message::Message;

/// Whether calls to `drawFrame` must be done from the app thread.
///
/// The type used for sending messages to the IO thread. For now this is
/// hardcoded with a capacity. We can make this a comptime parameter in
/// the future if we want it configurable.
//
// PORT NOTE: reference `pub const Mailbox = BlockingQueue(rendererpkg.Message, 64);`
// (rustty-datastruct's already-ported `BlockingQueue`).
// The first doc line above is carried verbatim: in the reference it heads
// `Mailbox`'s doc block although it describes no decl of this file (the
// app-thread flag is `apprt.zig`'s `must_draw_from_app_thread`).
pub type Mailbox = BlockingQueue<Message, 64>;

/// PORTS `configpkg.CustomShaderAnimation` (Config.zig:5244,
/// `enum(c_int){false,true,always}`). Renderer-LOCAL mirror (R4/G7 — the renderer
/// may not name `rustty_config::CustomShaderAnimation`; config already deps
/// renderer). `Config` maps its own enum into this via
/// `RendererConfig::custom_shader_animation` (config-side, acyclic — the
/// `Color`/`TerminalColor` mirror precedent). `#[repr(C)]` preserves the
/// extern-C-ABI ordering (`false`=0, `true`=1, `always`=2).
//
// PORT NOTE: reference `pub const CustomShaderAnimation = enum(c_int) { false,
// true, always };`. The bare-word variants become `False`/`True`/`Always` (Rust
// reserves `false`/`true`); the config-side `map_custom_shader_animation` maps
// arm-for-arm with an exhaustive `match` (adding a variant to the config enum
// fails to compile until the mirror + map are updated — the drift backstop).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum CustomShaderAnimation {
    False,
    True,
    Always,
}

/// Configuration we need derived from the main config.
//
// PORT NOTE (`aee7bf347`): `custom_shader_animation` MOVED out of this struct
// into the renderer's own `DerivedConfig` (`generic::DerivedConfig`) — the
// renderer computes its own animation wakes now (`Renderer::animation_wake`).
// The renderer-local `CustomShaderAnimation` mirror above stays declared HERE
// (unchanged decl site; `generic` imports it) so the config-side accessor
// (`RendererConfig::custom_shader_animation`, impl'd in rustty-config) keeps
// its acyclic seam (R4/G7 — memory `renderer-config-accessor-trait-acyclic`).
//
// The reference declaration this ports:
//
// pub const DerivedConfig = struct {
//     scrollback_compression: bool,
//
//     pub fn init(config: *const configpkg.Config) DerivedConfig {
//         return .{
//             .scrollback_compression = config.@"scrollback-compression",
//         };
//     }
// };
pub struct DerivedConfig {
    /// Whether the renderer thread schedules idle scrollback compression.
    ///
    /// PORTS: oracle 0fb89f4ff (WS7 D3) — read by the renderer thread's
    /// `Compression` scheduler (core's `renderer_seam.rs`) on every wake and
    /// every timer step, and by `changeConfig` to restart a fresh idle pass on
    /// a disabled -> enabled transition.
    // scrollback_compression: bool,
    pub scrollback_compression: bool,
}

impl DerivedConfig {
    // pub fn init(config: *const configpkg.Config) DerivedConfig {
    //     return .{
    //         .scrollback_compression = config.@"scrollback-compression",
    //     };
    // }
    //
    // PORT NOTE: `C` is bounded by the renderer-defined `RendererConfig` (R4/G7 —
    // the renderer may not name `rustty_config::Config`); the concrete
    // instantiation (`C` = `rustty_config::Config`) happens in the crates above
    // (core, via `Message::initChangeConfig`), same shape as
    // `Renderer.DerivedConfig::init`. Infallible (reference returns
    // `DerivedConfig`, not `!DerivedConfig`).
    pub fn init<C: RendererConfig>(config: &C) -> DerivedConfig {
        DerivedConfig {
            scrollback_compression: config.scrollback_compression(),
        }
    }
}

// ===========================================================================
// thread::DerivedConfig::init<C> test — mock-`C` (§6.T3).
// ===========================================================================
//
// The renderer CANNOT dep rustty-config (that IS the cycle, R4), so the thread
// DerivedConfig extraction is exercised through the shared `#[cfg(test)]`
// `MockConfig` impl-ing [`RendererConfig`] (returns `CustomShaderAnimation::True`,
// the config default `.true`).
#[cfg(test)]
mod derived_config_tests {
    use super::*;
    use crate::test_support::MockConfig;

    // PORT NOTE (`aee7bf347`): the `custom_shader_animation` extraction test
    // moved with the field to `generic::DerivedConfig` (whose full `init<C>`
    // extraction is covered by the golden-renderer assembly + the
    // `change_config` tests); the renderer-side wake logic is what consumes it
    // now (`Renderer::animation_wake`).

    /// PORTS: oracle 0fb89f4ff (WS7 D3) — the renderer's derived config carries
    /// `scrollback-compression`, so a config reload changes what the renderer
    /// thread's idle scheduler does. BOTH arms are asserted: extracting only the
    /// `true` default would pass against a hardcoded `true`.
    #[test]
    fn derived_config_extracts_scrollback_compression() {
        // The mock mirrors the config default (`true`).
        let d = DerivedConfig::init(&MockConfig::default());
        assert!(d.scrollback_compression);

        let d = DerivedConfig::init(&MockConfig {
            scrollback_compression: false,
            ..MockConfig::default()
        });
        assert!(!d.scrollback_compression);
    }
}
