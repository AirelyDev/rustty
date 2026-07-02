//! PORTS: ghostty `src/apprt/surface.zig` — the surface `Message` union, the
//! `ReportTitleStyle`/`ChildExited`/`NewSurfaceContext` types,
//! `shouldInheritWorkingDirectory`, and the surface-mailbox seam.
//!
//! A5 SEAM: the reference `Mailbox` struct names core's `*Surface` and
//! `App.Mailbox`, so it cannot live in this interface crate. It is replaced by
//! the [`AppMailbox`] trait (implemented by rustty-core) + the
//! [`SurfaceMessageEnvelope`] DTO — the surface half of core's `App.Message`.
//!
//! DEFERRED: `newConfig` (reference `pub fn newConfig(app, config, context)`)
//! reads `app.focusedSurface()` and `Surface.pwd()`, i.e. it needs concrete
//! rustty-core `App`/`Surface`. It is NOT part of the interface seam and lands in
//! rustty-apprt/rustty-core when those crates arrive (its only apprt-api-visible
//! dependency, [`should_inherit_working_directory`], IS ported here). This is an
//! additive placement deferral, not a drop (R1-safe).

use rustty_config::Config;
use rustty_datastruct::MessageData;
use rustty_renderer::Health;
use rustty_vt::mouse::Shape as MouseShape;
use rustty_vt::osc::color::ColoredTarget;
use rustty_vt::osc::ProgressReport;
use rustty_vt::pagelist::grow::Scrollbar;

use crate::structs::Clipboard;

/// Represents a write request. Magic number comes from the max size
/// we want this union to be.
//
// PORT NOTE: reference `pub const WriteReq = MessageData(u8, 255)` nested in
// `Message`. Hoisted to a module-level type alias (Rust cannot nest a generic
// type alias in an enum's namespace). Carries `MessageData`'s borrow lifetime.
pub type WriteReq<'a> = MessageData<'a, u8, 255>;

/// The message types that can be sent to a single surface.
//
// PORT NOTE: reference `Message = union(enum)`. Carries the `WriteReq`/
// `change_config` borrow lifetime `'a`.
pub enum Message<'a> {
    /// Set the title of the surface.
    /// TODO: we should change this to a "WriteReq" style structure in
    /// the termio message so that we can more efficiently send strings
    /// of any length
    SetTitle([u8; 256]),

    /// Report the window title back to the terminal
    ReportTitle(ReportTitleStyle),

    /// Set the mouse shape.
    SetMouseShape(MouseShape),

    /// Read the clipboard and write to the pty.
    ClipboardRead(Clipboard),

    /// Write the clipboard contents.
    ClipboardWrite {
        clipboard_type: Clipboard,
        req: WriteReq<'a>,
    },

    /// Change the configuration to the given configuration. The pointer is
    /// not valid after receiving this message so any config must be used
    /// and derived immediately.
    //
    // PORT NOTE: reference `*const Config` -> borrowed `&'a Config`.
    ChangeConfig(&'a Config),

    /// Close the surface. This will only close the current surface that
    /// receives this, not the full application.
    Close,

    /// The child process running in the surface has exited. This may trigger
    /// a surface close, it may not. Additional details about the child
    /// command are given in the `ChildExited` struct.
    ChildExited(ChildExited),

    /// Show a desktop notification.
    //
    // PORT NOTE: reference `title: [63:0]u8, body: [255:0]u8` — sentinel-
    // terminated arrays (63/255 usable bytes + a NUL sentinel), so `@sizeOf` is
    // 64/256. Modeled as `[u8; 64]`/`[u8; 256]`.
    DesktopNotification { title: [u8; 64], body: [u8; 256] },

    /// Health status change for the renderer.
    RendererHealth(Health),

    /// Tell the surface to present itself to the user. This may require raising
    /// a window and switching tabs.
    PresentSurface,

    /// Notifies the surface that password input has started within
    /// the terminal. This should always be followed by a false value
    /// unless the surface exits.
    PasswordInput(bool),

    /// A terminal color was changed using OSC sequences.
    ColorChange(ColoredTarget),

    /// Notifies the surface that a tick of the timer that is timing
    /// out selection scrolling has occurred. "selection scrolling"
    /// is when the user has clicked and dragged the mouse outside
    /// the viewport of the terminal and the terminal is scrolling
    /// the viewport to follow the mouse cursor.
    SelectionScrollTick(bool),

    /// The terminal has reported a change in the working directory.
    PwdChange(WriteReq<'a>),

    /// The terminal encountered a bell character.
    RingBell,

    /// Report the progress of an action using a GUI element
    ProgressReport(ProgressReport),

    /// A command has started in the shell, start a timer.
    StartCommand,

    /// A command has finished in the shell, stop the timer and send out
    /// notifications as appropriate. The optional u8 is the exit code
    /// of the command.
    StopCommand(Option<u8>),

    /// The scrollbar state changed for the surface.
    Scrollbar(Scrollbar),

    /// Search progress update
    SearchTotal(Option<usize>),

    /// Selected search index change
    SearchSelected(Option<usize>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportTitleStyle {
    Csi21t,
    // This enum is a placeholder for future title styles.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct ChildExited {
    pub exit_code: u32,
    pub runtime_ms: u64,
    // PORT NOTE: reference `getGObjectType` (`gobject.ext.defineBoxed(...,
    // "GhosttyApprtChildExited")`, gated on `app_runtime == .gtk`) is deferred to
    // rustty-apprt's gtk backend; this crate defines the plain struct only.
}

/// Context for new surface creation to determine inheritance behavior
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum NewSurfaceContext {
    Window = 0,
    Tab = 1,
    Split = 2,
}

pub fn should_inherit_working_directory(context: NewSurfaceContext, config: &Config) -> bool {
    match context {
        NewSurfaceContext::Window => config.window_inherit_working_directory,
        NewSurfaceContext::Tab => config.tab_inherit_working_directory,
        NewSurfaceContext::Split => config.split_inherit_working_directory,
    }
}

/// The surface half of core's `App.Message.surface_message` envelope.
//
// PORT NOTE: reference `Mailbox.push` builds `.surface_message = .{ .surface =
// self.surface, .message = msg }`. The concrete `App.Message` union lives in
// rustty-core; this exposes the envelope shape so core's [`AppMailbox`] impl can
// construct it without apprt-api naming core. `S` is the opaque core surface
// handle.
pub struct SurfaceMessageEnvelope<'a, S> {
    pub surface: S,
    pub message: Message<'a>,
}

/// A surface mailbox seam.
//
// PORT NOTE: replaces the reference `apprt.surface.Mailbox` struct
// (`{ surface: *Surface, app: App.Mailbox }`), which names rustty-core. The send
// path is exposed as a trait implemented BY core so apprt-api never depends on
// core. The `Timeout`/`Size` types trace to core's queue module (`App.Mailbox.
// Queue.Timeout`/`.Size`); they were opaque associated-type growth placeholders
// until core C0 RESOLVED them concrete (CORE_BLUEPRINT §3): `App.Mailbox.Queue
// = BlockingQueue(App.Message, 64)`, and rustty-datastruct defines
// `blocking_queue::Timeout`/`Size` at MODULE level (not per-instantiation), so
// the faithful `Queue.Timeout`/`Queue.Size` ARE the concrete datastruct types —
// which also lets generic termio code CONSTRUCT timeouts (`.{ .instant = {} }`).
pub trait AppMailbox {
    /// Core's `*Surface` handle (opaque here).
    type Surface;

    /// Send a message to the surface.
    //
    // PORT NOTE: mirrors `Mailbox.push`. Reference body comment preserved verbatim:
    //   Surface message sending is actually implemented on the app
    //   thread, so we have to rewrap the message with our surface
    //   pointer and send it to the app thread.
    // i.e. the impl rewraps `msg` into a [`SurfaceMessageEnvelope`] with its own
    // surface pointer and pushes it via `self.app.push(.{ .surface_message = ... })`.
    //
    // PORT NOTE (`Message<'static>`): a STORING push cannot accept an arbitrary
    // borrow lifetime — the cross-thread queue stores the `'static`
    // instantiation (termio `mailbox::Queue` precedent; every queued payload is
    // owned). The one borrowed variant, `ChangeConfig(&Config)`, is only ever
    // DIRECT-dispatched (`App.updateConfig` -> `surface.core().handleMessage`),
    // never queued, so `handleMessage` keeps `Message<'_>` while pushes are
    // `'static`.
    fn push(
        &self,
        msg: Message<'static>,
        timeout: rustty_datastruct::blocking_queue::Timeout,
    ) -> rustty_datastruct::blocking_queue::Size;
}

// DEFERRED (needs rustty-core `App`/`Surface`; see module doc). The reference
// `newConfig` function + its comments are preserved verbatim here so nothing is
// lost (R1); the implementation lands in rustty-apprt/rustty-core.
//
// /// Returns a new config for a surface for the given app that should be
// /// used for any new surfaces. The resulting config should be deinitialized
// /// after the surface is initialized.
// pub fn newConfig(
//     app: *const App,
//     config: *const Config,
//     context: NewSurfaceContext,
// ) Allocator.Error!Config {
//     // Create a shallow clone
//     var copy = config.shallowClone(app.alloc);
//
//     // Our allocator is our config's arena
//     const alloc = copy._arena.?.allocator();
//
//     // Get our previously focused surface for some inherited values.
//     const prev = app.focusedSurface();
//     if (prev) |p| {
//         if (shouldInheritWorkingDirectory(context, config)) {
//             if (try p.pwd(alloc)) |pwd| {
//                 copy.@"working-directory" = .{ .path = pwd };
//             }
//         }
//     }
//
//     return copy;
// }
