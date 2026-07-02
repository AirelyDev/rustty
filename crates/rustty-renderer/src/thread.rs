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
// PORT NOTE (DEFERRED SEAM): the reference is
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
// the same wall as `shadertoy::loadFromFile`). Until the cycle-side placement
// is resolved with the renderer Thread event loop, this uninhabited placeholder
// keeps the type addressable for the cross-thread message surface
// (`Message::ChangeConfig`) without fabricating the struct (precedent:
// `state::Inspector`). See .port/phase-2/DEFERRALS.md.
pub enum DerivedConfig {}
