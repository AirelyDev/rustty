//! PORTS: ghostty `src/apprt/surface.zig` — the surface `Message` union, the
//! `ReportTitleStyle`/`ChildExited`/`NewSurfaceContext` types,
//! `shouldInheritWorkingDirectory`, and the surface-mailbox seam.
//!
//! A5 SEAM: the reference `Mailbox` struct names core's `*Surface` and
//! `App.Mailbox`, so it cannot live in this interface crate. It is replaced by
//! the [`AppMailbox`] trait (implemented by rustty-core) + the
//! [`SurfaceMessageEnvelope`] DTO — the surface half of core's `App.Message`.
//!
//! RESOLVED (core C-T): `newConfig` (reference `pub fn newConfig(app, config,
//! context)`) reads `app.focusedSurface()` and `Surface.pwd()`, i.e. it needs
//! concrete rustty-core `App`/`Surface`, so it is NOT part of this interface
//! seam. Per CORE_BLUEPRINT §6 (C-T) it now lands as `rustty_core::app::new_config`
//! (core app glue), built on this crate's [`should_inherit_working_directory`]
//! and [`NewSurfaceContext`]. The verbatim reference body is retained below for
//! provenance. See .port/phase-2/DEFERRALS.md ("core C-T").

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

/// A fixed-size desktop notification payload sent to the app thread.
//
// PORT NOTE: reference `pub const DesktopNotification = struct { ... }` nested
// in `Message` (`Message.DesktopNotification`). Hoisted to module level per the
// `WriteReq` precedent (Rust cannot nest types in an enum's namespace).
// Reference `title: [63:0]u8, body: [255:0]u8` — sentinel-terminated arrays
// (63/255 usable bytes + a NUL sentinel), so `@sizeOf` is 64/256. Modeled as
// `[u8; 64]`/`[u8; 256]`.
pub struct DesktopNotification {
    /// Desktop notification title.
    pub title: [u8; 64],

    /// Desktop notification body.
    pub body: [u8; 256],
}

impl DesktopNotification {
    // pub fn init(title: []const u8, body: []const u8) DesktopNotification {
    pub fn init(title: &[u8], body: &[u8]) -> DesktopNotification {
        // var result: DesktopNotification = undefined;
        //
        // PORT NOTE: Rust has no `undefined`; the buffers are zero-initialized.
        // Bytes past the NUL sentinel are never read (readers slice with
        // `std.mem.sliceTo(&.., 0)`), so the zero fill adds nothing observable.
        let mut result = DesktopNotification {
            title: [0; 64],
            body: [0; 256],
        };
        // copyUtf8Z(result.title.len, &result.title, title);
        // copyUtf8Z(result.body.len, &result.body, body);
        //
        // (`.len` of a Zig `[N:0]u8` is N — the sentinel-EXCLUSIVE capacity —
        // so the Rust `[u8; N + 1]` model passes `len() - 1`.)
        Self::copy_utf8_z(result.title.len() - 1, &mut result.title, title);
        Self::copy_utf8_z(result.body.len() - 1, &mut result.body, body);
        result
    }

    /// UTF-8 continuation bytes occupy the range 0x80 through 0xBF.
    fn is_utf8_continuation_byte(byte: u8) -> bool {
        // return switch (byte) { 0x80...0xBF => true, else => false };
        matches!(byte, 0x80..=0xBF)
    }

    /// Copy as much of `src` as fits, backing up from a UTF-8 continuation
    /// byte so valid input is never truncated in the middle of a codepoint.
    //
    // PORT NOTE: reference signature `copyUtf8Z(comptime capacity: usize,
    // dst: *[capacity:0]u8, src: []const u8)` — `capacity` is the
    // sentinel-exclusive length of `dst`. The Rust sentinel-array model is a
    // `&mut [u8]` of `capacity + 1` bytes with `capacity` as a runtime
    // parameter (asserted, since Rust cannot tie a const generic to `N + 1`).
    fn copy_utf8_z(capacity: usize, dst: &mut [u8], src: &[u8]) {
        debug_assert!(dst.len() == capacity + 1);
        // var len = @min(src.len, capacity);
        let mut len = src.len().min(capacity);
        // while (len > 0 and
        //     len < src.len and
        //     isUtf8ContinuationByte(src[len])) : (len -= 1)
        // {}
        while len > 0 && len < src.len() && Self::is_utf8_continuation_byte(src[len]) {
            len -= 1;
        }

        // @memcpy(dst[0..len], src[0..len]);
        dst[..len].copy_from_slice(&src[..len]);
        // dst[len] = 0;
        dst[len] = 0;
    }
}

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

    /// A Kitty clipboard protocol (OSC 5522) read request. The receiver
    /// takes ownership of the request state and must eventually destroy
    /// it.
    // kitty_clipboard_read: *apprt.ClipboardRequest.KittyRead,
    // PORT NOTE: `*KittyRead` (arena-owned, destroy on the receiver) ->
    // `Box<KittyRead>` moved by value — ownership makes double-free
    // unrepresentable (assessment E hazard #1).
    KittyClipboardRead(Box<crate::structs::KittyRead>),

    /// A committed Kitty clipboard protocol (OSC 5522) write
    /// transaction. The receiver takes ownership of the request state
    /// and must eventually destroy it.
    // kitty_clipboard_write: *apprt.ClipboardRequest.KittyWrite,
    KittyClipboardWrite(Box<crate::structs::KittyWrite>),

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
    // PORT NOTE: reference `desktop_notification: DesktopNotification` — the
    // named payload struct (hoisted to module level above, `WriteReq`
    // precedent).
    DesktopNotification(DesktopNotification),

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
    //
    // PORT NOTE (`push_or_return`, core C-T): `StreamHandler.surfaceMessageWriter`
    // (src/termio/stream_handler.zig) does the reference's by-COPY retry —
    // `if (surface_mailbox.push(msg, .instant) == 0) { unlock; push(msg,
    // .forever); }`. Rust's `push` MOVES `msg`, so the retry needs the message
    // handed BACK on the failed instant push. [`push_or_return`] is that
    // primitive (mirrors `rustty_datastruct::BlockingQueue::push_or_return`);
    // the plain [`push`](Self::push) is the drop-on-failure default expressed
    // in terms of it, so existing callers are unchanged.
    //
    // The `Err` carries a full `Message` (large); handing it back is the point.
    #[allow(clippy::result_large_err)]
    fn push_or_return(
        &self,
        msg: Message<'static>,
        timeout: rustty_datastruct::blocking_queue::Timeout,
    ) -> Result<rustty_datastruct::blocking_queue::Size, Message<'static>>;

    /// Send a message to the surface, dropping it if the queue push fails.
    fn push(
        &self,
        msg: Message<'static>,
        timeout: rustty_datastruct::blocking_queue::Timeout,
    ) -> rustty_datastruct::blocking_queue::Size {
        // On failure the returned message is dropped (the plain `push`
        // contract mirrors the reference's `_ = ...push(msg, ...)`).
        self.push_or_return(msg, timeout).unwrap_or(0)
    }

    /// `Mailbox.surface`, as the crash reporter sees it.
    //
    // PORT NOTE (LEDGER S211-F2, S212-F2): the reference's `Mailbox` is the
    // struct `{ surface: *Surface, app: App.Mailbox }`, and termio's io and
    // io-reader threads read the field for their crash metadata:
    // `crash.sentry.thread_state = .{ .type = .io, .surface =
    // io.surface_mailbox.surface }`. `Surface` is core's and apprt-api cannot
    // name it, so the field is exposed through crash's seam trait. There is
    // no default: every mailbox has a surface in the reference.
    fn crash_surface(&self) -> *const dyn rustty_crash::sentry::ThreadStateSurface;
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

#[cfg(test)]
mod tests {
    use super::*;

    // std.mem.sliceTo(&buf, 0) — slice up to (excluding) the NUL sentinel.
    fn slice_to_nul(buf: &[u8]) -> &[u8] {
        &buf[..buf.iter().position(|&b| b == 0).unwrap_or(buf.len())]
    }

    // PORT NOTE (all four tests): a Zig `[N:0]u8` destination is modeled as
    // `[u8; N + 1]`, so the reference's `dst.len` (the sentinel-exclusive
    // capacity N) is expressed as `dst.len() - 1`, and `expectEqualStrings`
    // over `sliceTo(&dst, 0)` becomes a byte-slice `assert_eq!` over
    // `slice_to_nul(&dst)`.

    // test "DesktopNotification init"
    #[test]
    fn desktop_notification_init() {
        // const notification = Message.DesktopNotification.init("Title", "Body");
        let notification = DesktopNotification::init(b"Title", b"Body");

        // try std.testing.expectEqualStrings("Title", std.mem.sliceTo(&notification.title, 0));
        assert_eq!(&b"Title"[..], slice_to_nul(&notification.title));
        // try std.testing.expectEqualStrings("Body", std.mem.sliceTo(&notification.body, 0));
        assert_eq!(&b"Body"[..], slice_to_nul(&notification.body));
    }

    // test "copyUtf8Z handles len at the final byte of every UTF-8 sequence length"
    #[test]
    fn copy_utf8_z_handles_len_at_the_final_byte_of_every_utf8_sequence_length() {
        // const DesktopNotification = Message.DesktopNotification;

        // var dst_1_byte: [1:0]u8 = undefined;
        let mut dst_1_byte = [0u8; 2];
        let src_ending_in_1_byte_codepoint = "ab".as_bytes();
        assert!(!DesktopNotification::is_utf8_continuation_byte(
            src_ending_in_1_byte_codepoint[dst_1_byte.len() - 1],
        ));
        DesktopNotification::copy_utf8_z(
            dst_1_byte.len() - 1,
            &mut dst_1_byte,
            src_ending_in_1_byte_codepoint,
        );
        assert_eq!(&b"a"[..], slice_to_nul(&dst_1_byte));

        // var dst_2_bytes: [2:0]u8 = undefined;
        let mut dst_2_bytes = [0u8; 3];
        let src_ending_in_2_byte_codepoint = "aЯ".as_bytes();
        assert!(DesktopNotification::is_utf8_continuation_byte(
            src_ending_in_2_byte_codepoint[dst_2_bytes.len() - 1],
        ));
        DesktopNotification::copy_utf8_z(
            dst_2_bytes.len() - 1,
            &mut dst_2_bytes,
            src_ending_in_2_byte_codepoint,
        );
        assert_eq!(&b"a"[..], slice_to_nul(&dst_2_bytes));

        // var dst_3_bytes: [3:0]u8 = undefined;
        let mut dst_3_bytes = [0u8; 4];
        let src_ending_in_3_byte_codepoint = "a€".as_bytes();
        assert!(DesktopNotification::is_utf8_continuation_byte(
            src_ending_in_3_byte_codepoint[dst_3_bytes.len() - 1],
        ));
        DesktopNotification::copy_utf8_z(
            dst_3_bytes.len() - 1,
            &mut dst_3_bytes,
            src_ending_in_3_byte_codepoint,
        );
        assert_eq!(&b"a"[..], slice_to_nul(&dst_3_bytes));

        // var dst_4_bytes: [4:0]u8 = undefined;
        let mut dst_4_bytes = [0u8; 5];
        let src_ending_in_4_byte_codepoint = "a😀".as_bytes();
        assert!(DesktopNotification::is_utf8_continuation_byte(
            src_ending_in_4_byte_codepoint[dst_4_bytes.len() - 1],
        ));
        DesktopNotification::copy_utf8_z(
            dst_4_bytes.len() - 1,
            &mut dst_4_bytes,
            src_ending_in_4_byte_codepoint,
        );
        assert_eq!(&b"a"[..], slice_to_nul(&dst_4_bytes));
    }

    // test "copyUtf8Z keeps a complete codepoint at the truncation boundary"
    #[test]
    fn copy_utf8_z_keeps_a_complete_codepoint_at_the_truncation_boundary() {
        // var dst: [5:0]u8 = undefined;
        let mut dst = [0u8; 6];

        DesktopNotification::copy_utf8_z(dst.len() - 1, &mut dst, "abcЯz".as_bytes());

        // try std.testing.expectEqualStrings("abcЯ", std.mem.sliceTo(&dst, 0));
        assert_eq!("abcЯ".as_bytes(), slice_to_nul(&dst));
    }

    // test "copyUtf8Z preserves UTF-8 that fits"
    #[test]
    fn copy_utf8_z_preserves_utf8_that_fits() {
        // var dst: [5:0]u8 = undefined;
        let mut dst = [0u8; 6];

        DesktopNotification::copy_utf8_z(dst.len() - 1, &mut dst, "abcЯ".as_bytes());

        // try std.testing.expectEqualStrings("abcЯ", std.mem.sliceTo(&dst, 0));
        assert_eq!("abcЯ".as_bytes(), slice_to_nul(&dst));
    }
}
