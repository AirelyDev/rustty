//! Represents the renderer thread logic. The renderer thread is able to
//! be woken up to render.
//!
//! PORTS: ghostty src/renderer/Thread.zig — MINIMAL cross-thread surface ONLY
//! (integration-tier stage 2): the `Mailbox` type (what termio's
//! `renderer_mailbox` points at) and the `DerivedConfig` (now the real
//! single-field struct + `init`). The reference file's event loop (the `Thread`
//! struct itself, its xev `Loop`/`Async`/`Timer` handles,
//! `threadMain`/`drainMailbox`/`renderCallback` and the rest of the
//! render/draw/cursor timer machinery) is NOT ported here — it is deferred with
//! the D6 xev decision + the GPU renderer tier (0 `test{}` blocks — R1-safe; see
//! .port/phase-2/DEFERRALS.md).

use rustty_datastruct::BlockingQueue;

use crate::config::RendererConfig;
use crate::message::Message;

/// The type used for sending messages to the IO thread. For now this is
/// hardcoded with a capacity. We can make this a comptime parameter in
/// the future if we want it configurable.
//
// PORT NOTE: reference `pub const Mailbox = BlockingQueue(rendererpkg.Message, 64);`
// (rustty-datastruct's already-ported `BlockingQueue`).
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
// PORT NOTE: reference `Thread.DerivedConfig { custom_shader_animation }`. The
// field-less [ADV-FIX 2] placeholder is now the real single-field struct; core
// embeds it BY VALUE (renderer_seam.rs) and constructs it nowhere (`Thread::init`
// is a placeholder), so promoting it breaks no core site (verified: no
// `thread::DerivedConfig { … }` construction exists outside this crate). The
// `custom_shader_animation` field is read through the renderer-defined
// `RendererConfig::custom_shader_animation` accessor (R4/G7 — the renderer may
// not name `rustty_config::CustomShaderAnimation`), which maps the config's own
// enum into the renderer-local mirror above.
//
// The reference declaration this ports:
//
// pub const DerivedConfig = struct {
//     custom_shader_animation: configpkg.CustomShaderAnimation,
//
//     pub fn init(config: *const configpkg.Config) DerivedConfig {
//         return .{
//             .custom_shader_animation = config.@"custom-shader-animation",
//         };
//     }
// };
pub struct DerivedConfig {
    // custom_shader_animation: configpkg.CustomShaderAnimation,
    pub custom_shader_animation: CustomShaderAnimation,
}

impl DerivedConfig {
    // pub fn init(config: *const configpkg.Config) DerivedConfig {
    //     return .{
    //         .custom_shader_animation = config.@"custom-shader-animation",
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
            custom_shader_animation: config.custom_shader_animation(),
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

    #[test]
    fn derived_config_extracts_custom_shader_animation() {
        let d = DerivedConfig::init(&MockConfig::default());
        assert_eq!(d.custom_shader_animation, CustomShaderAnimation::True);
    }
}
