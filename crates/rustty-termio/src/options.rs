//! The options that are used to configure a terminal IO implementation.
//!
//! PORTS: ghostty src/termio/Options.zig (0 `test{}` blocks).
//!
//! PORT STATUS: struct shape ported (integration-tier stage 2).

// const xev = @import("../global.zig").xev;
// const apprt = @import("../apprt.zig");
// const renderer = @import("../renderer.zig");
// const Config = @import("../config.zig").Config;
// const termio = @import("../termio.zig");
use rustty_apprt_api::surface::AppMailbox;
use rustty_config::Config;
use rustty_renderer::size::Size;
use rustty_renderer::state::State;
use rustty_renderer::thread;

use crate::backend::Backend;
use crate::mailbox::Mailbox;
use crate::termio_impl::DerivedConfig;
use crate::xev_seam as xev;

// PORT NOTE: the reference is a top-level file-struct (`Options.zig`); the Rust
// port names it `Options`. `M` is the `apprt.surface.Mailbox` seam
// (`rustty_apprt_api::surface::AppMailbox`, implemented by rustty-core later);
// `'a` carries the `*const Config` borrow.
pub struct Options<'a, M: AppMailbox> {
    /// All size metrics for the terminal.
    pub size: Size,

    /// The full app configuration. This is only available during initialization.
    /// The memory it points to is NOT stable after the init call so any values
    /// in here must be copied.
    //
    // PORT NOTE: reference `full_config: *const Config` -> borrowed
    // `&'a Config` (apprt-api `Message::ChangeConfig` precedent).
    pub full_config: &'a Config,

    /// The derived configuration for this termio implementation.
    pub config: DerivedConfig,

    /// The backend for termio that implements where reads/writes are sourced.
    pub backend: Backend,

    /// The mailbox for the terminal. This is how messages are delivered.
    /// If you're using termio.Thread this MUST be "mailbox".
    pub mailbox: Mailbox,

    /// The render state. The IO implementation can modify anything here. The
    /// surface thread will setup the initial "terminal" pointer but the IO impl
    /// is free to change that if that is useful (i.e. doing some sort of dual
    /// terminal implementation.)
    //
    // PORT NOTE: reference `renderer_state: *renderer.State` (a shared mutable
    // pointer across threads) -> raw `*mut State` (renderer state.rs precedent).
    pub renderer_state: *mut State,

    /// A handle to wake up the renderer. This hints to the renderer that
    /// a repaint should happen.
    //
    // PORT NOTE: `xev.Async` — the D6 deferred seam (crate::xev_seam).
    pub renderer_wakeup: xev::Async,

    /// The mailbox for renderer messages.
    //
    // PORT NOTE: reference `renderer_mailbox: *renderer.Thread.Mailbox` — the
    // minimal renderer cross-thread surface landed with this stage
    // (rustty_renderer::thread::Mailbox = BlockingQueue<renderer::Message, 64>).
    pub renderer_mailbox: *mut thread::Mailbox,

    /// The mailbox for sending the surface messages.
    //
    // PORT NOTE: reference `surface_mailbox: apprt.surface.Mailbox` — the
    // already-landed AppMailbox seam; termio is a generic consumer (`M`).
    pub surface_mailbox: M,
}
