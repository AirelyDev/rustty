//! Represents the renderer thread logic. The renderer thread is able to
//! be woken up to render.
//!
//! PORTS: ghostty src/renderer/Thread.zig — MINIMAL cross-thread surface ONLY
//! (integration-tier stage 2): the `Mailbox` type (what termio's
//! `renderer_mailbox` points at) and the `DerivedConfig` placeholder. The
//! reference file's event loop (the `Thread` struct itself, its xev
//! `Loop`/`Async`/`Timer` handles, `threadMain`/`drainMailbox`/`renderCallback`
//! and the rest of the render/draw/cursor timer machinery) is NOT ported here —
//! it is deferred with the D6 xev decision + the GPU renderer tier (0 `test{}`
//! blocks — R1-safe; see .port/phase-2/DEFERRALS.md).

use rustty_datastruct::BlockingQueue;

use crate::message::Message;

/// The type used for sending messages to the IO thread. For now this is
/// hardcoded with a capacity. We can make this a comptime parameter in
/// the future if we want it configurable.
//
// PORT NOTE: reference `pub const Mailbox = BlockingQueue(rendererpkg.Message, 64);`
// (rustty-datastruct's already-ported `BlockingQueue`).
pub type Mailbox = BlockingQueue<Message, 64>;

/// Configuration we need derived from the main config.
//
// PORT NOTE (DEPENDENCY PLACEHOLDER — INHABITED, core C0 [ADV-FIX 2]): the
// reference is
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
//
// Its field and `init` take `configpkg` types, but `rustty-config` already
// depends on `rustty-renderer` so a renderer -> config edge would cycle (G7 —
// the same wall as `shadertoy::loadFromFile`). This placeholder was an
// UNINHABITED `enum {}` through the integration tier; core's renderer thread
// seam embeds it BY VALUE (renderer/Thread.zig `config: DerivedConfig`), and
// embedding an uninhabited type would make `Thread<A>` — and hence core's
// `Surface<A>` — uninhabited (references to them UB-adjacent, forcing every
// core test onto dangling pointers). Core C0 therefore retypes it to an
// INHABITED field-less struct with `unimplemented!()` methods (the xev_seam
// dependency-placeholder precedent — no reference logic is stubbed): the
// `custom_shader_animation` field + real `init` body land with the config-side
// placement (the renderer Thread event loop tier). See
// .port/phase-2/DEFERRALS.md.
pub struct DerivedConfig {
    // custom_shader_animation: configpkg.CustomShaderAnimation,
    // (config-typed field deferred WITH the placeholder — it would need the
    // G7-cycling renderer -> config edge.)
}

impl DerivedConfig {
    // pub fn init(config: *const configpkg.Config) DerivedConfig {
    //     return .{
    //         .custom_shader_animation = config.@"custom-shader-animation",
    //     };
    // }
    //
    // PORT NOTE: generic `C` solely because renderer -> config cycles (G7);
    // the concrete instantiation (`C` = `rustty_config::Config`) happens in
    // the crates above (core), same shape as `Message::initChangeConfig`.
    pub fn init<C>(config: &C) -> DerivedConfig {
        let _ = config;
        unimplemented!(
            "renderer.Thread.DerivedConfig.init deferred (config-coupled placeholder); \
             verbatim body above"
        )
    }
}
