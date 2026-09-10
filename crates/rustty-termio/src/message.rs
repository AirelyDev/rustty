//! The messages that can be sent to an IO thread.
//!
//! PORTS: ghostty src/termio/message.zig (2 anonymous `test{}` blocks — both
//! ported below).
//!
//! PORT STATUS: ported (integration-tier stage 2).

// PARITY: the reference's identifiers are camelCase (`writeReq`); preserved
// verbatim per R1/R2 (crash-tier precedent).
#![allow(non_snake_case)]

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const renderer = @import("../renderer.zig");
// const terminal = @import("../terminal/main.zig");
// const termio = @import("../termio.zig");
// const MessageData = @import("../datastruct/main.zig").MessageData;
use rustty_datastruct::message_data::{Alloc, Small};
use rustty_datastruct::MessageData;
use rustty_renderer::size::Size;
use rustty_vt::size_report;
use rustty_vt::terminal::ScrollViewport;

use crate::termio_impl::DerivedConfig;

/// Represents a write request. Magic number comes from the largest
/// other union value. It can be upped if we add a larger union member
/// in the future.
//
// PORT NOTE: reference `pub const WriteReq = MessageData(u8, 38)` nested in
// `Message`. Hoisted to a module-level type alias (Rust cannot nest a generic
// type alias in an enum's namespace — same hoist as apprt-api's `WriteReq`).
// Carries `MessageData`'s borrow lifetime for the `Stable` variant.
pub type WriteReq<'a> = MessageData<'a, u8, 38>;

/// The messages that can be sent to an IO thread.
///
/// This is not a tiny structure (~40 bytes at the time of writing this comment),
/// but the number of messages we send to the IO thread is also very small.
/// At the current size we can queue 26,000 messages before consuming a MB of RAM.
//
// PORT NOTE: reference `Message = union(enum)`. The lifetime `'a` carries the
// `write_stable` borrow (`WriteReq.Stable` == `[]const Elem`); the cross-thread
// mailbox stores the `'static` instantiation (see `mailbox::Queue`).
pub enum Message<'a> {
    /// Request a color scheme report is sent to the PTY.
    ColorSchemeReport {
        /// Force write the current color scheme
        force: bool,
    },

    /// Request a visibility report is sent to the PTY.
    // visibility_report: struct { visible: bool, force: bool },
    VisibilityReport {
        /// The visibility state to report.
        visible: bool,

        /// Send the report even when mode 2033 is disabled.
        force: bool,
    },

    /// Purposely crash the renderer. This is used for testing and debugging.
    /// See the "crash" binding action.
    //
    // PORT NOTE: reference `crash: void`.
    Crash,

    /// The derived configuration to update the implementation with. This
    /// is allocated via the allocator and is expected to be freed when done.
    //
    // PORT NOTE: reference payload `struct { alloc: Allocator, ptr:
    // *termio.Termio.DerivedConfig }`. The `alloc` field collapses into `Box`
    // ownership (the reference `alloc.create`s the pointer and the consumer
    // `defer config.alloc.destroy(config.ptr)`s it — precedent: rustty-os
    // cf_release_thread's `Message.Release` alloc collapse).
    ChangeConfig {
        ptr: Box<DerivedConfig>,
    },

    /// Activate or deactivate the inspector.
    Inspector(bool),

    /// Resize the window.
    Resize(Size),

    /// Request a size report is sent to the PTY ([in-band
    /// size report, mode 2048](https://gist.github.com/rockorager/e695fb2924d36b2bcf1fff4a3704bd83) and
    /// [XTWINOPS](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h4-Functions-using-CSI-_-ordered-by-the-final-character-lparen-s-rparen:CSI-Ps;Ps;Ps-t.1EB0)).
    SizeReport(SizeReport),

    /// Clear the screen.
    ClearScreen {
        /// Include clearing the history
        history: bool,
    },

    /// Scroll the viewport
    ScrollViewport(ScrollViewport),

    /// Selection scrolling. If this is set to true then the termio
    /// thread starts a timer that will trigger a `selection_scroll_tick`
    /// message back to the surface. This ping/pong is because the
    /// surface thread doesn't have access to an event loop from libghostty.
    SelectionScroll(bool),

    /// Jump forward/backward n prompts.
    JumpToPrompt(isize),

    /// Send this when a synchronized output mode is started. This will
    /// start the timer so that the output mode is disabled after a
    /// period of time so that a bad actor can't hang the terminal.
    //
    // PORT NOTE: reference `start_synchronized_output: void`.
    StartSynchronizedOutput,

    /// Enable or disable linefeed mode (mode 20).
    LinefeedMode(bool),

    /// The surface gained or lost focus.
    Focused(bool),

    /// Record a Kitty clipboard protocol session grant for a password
    /// so future requests carrying it skip the permission prompt, for
    /// reads and writes respectively.
    // kitty_clipboard_grant_read: KittyClipboardGrant,
    // kitty_clipboard_grant_write: KittyClipboardGrant,
    KittyClipboardGrantRead(KittyClipboardGrant),
    KittyClipboardGrantWrite(KittyClipboardGrant),

    /// Write where the data fits in the union.
    //
    // PORT NOTE: reference `write_small: WriteReq.Small`.
    WriteSmall(Small<u8, 38>),

    /// Write where the data pointer is stable.
    //
    // PORT NOTE: reference `write_stable: WriteReq.Stable` (= `[]const u8`).
    WriteStable(&'a [u8]),

    /// Write where the data is allocated and must be freed.
    //
    // PORT NOTE: reference `write_alloc: WriteReq.Alloc`.
    WriteAlloc(Alloc<u8>),
}

impl Message<'_> {
    /// PORT NOTE (Zig-builtin shim, task-14e-1): `@tagName(message)` — the
    /// reference union's own snake_case FIELD name, as `Thread.zig:313`'s
    /// `log.debug("mailbox message={s}", .{@tagName(message)})` prints it. The
    /// Rust variant identifiers are PascalCase (R2 naming), so a derived
    /// `Debug` would emit the wrong bytes; the mapping is spelled out and
    /// pinned by a test (`rustty-xev` `Backend::tag_name` precedent).
    #[allow(non_snake_case)] // faithful Zig builtin name (R1)
    pub fn tagName(&self) -> &'static str {
        match self {
            Message::ColorSchemeReport { .. } => "color_scheme_report",
            Message::VisibilityReport { .. } => "visibility_report",
            Message::Crash => "crash",
            Message::ChangeConfig { .. } => "change_config",
            Message::Inspector(_) => "inspector",
            Message::Resize(_) => "resize",
            Message::SizeReport(_) => "size_report",
            Message::ClearScreen { .. } => "clear_screen",
            Message::ScrollViewport(_) => "scroll_viewport",
            Message::SelectionScroll(_) => "selection_scroll",
            Message::JumpToPrompt(_) => "jump_to_prompt",
            Message::StartSynchronizedOutput => "start_synchronized_output",
            Message::LinefeedMode(_) => "linefeed_mode",
            Message::Focused(_) => "focused",
            Message::KittyClipboardGrantRead(_) => "kitty_clipboard_grant_read",
            Message::KittyClipboardGrantWrite(_) => "kitty_clipboard_grant_write",
            Message::WriteSmall(_) => "write_small",
            Message::WriteStable(_) => "write_stable",
            Message::WriteAlloc(_) => "write_alloc",
        }
    }
}

impl Message<'_> {
    /// Return a write request for the given data. This will use
    /// write_small if it fits or write_alloc otherwise. This should NOT
    /// be used for stable pointers which can be manually set to write_stable.
    //
    // PORT NOTE: reference `pub fn writeReq(alloc: Allocator, data: anytype)
    // !Message`. The `alloc` param collapses (Rust global allocator) and with
    // it the `!` error (`WriteReq.init`'s only error is OOM); `data: anytype`
    // is the byte-slice `MessageData::init` consumes.
    pub fn writeReq(data: &[u8]) -> Message<'static> {
        match WriteReq::init(data) {
            // .stable => unreachable,
            MessageData::Stable(_) => unreachable!(),
            // .small => |v| Message{ .write_small = v },
            MessageData::Small(v) => Message::WriteSmall(v),
            // .alloc => |v| Message{ .write_alloc = v },
            MessageData::Alloc(v) => Message::WriteAlloc(v),
        }
    }

    // Free resources owned by a message that will not be processed.
    // The message is invalid after this call.
    //
    // PORT NOTE: reference `pub fn deinit(self: *const Message) void` frees the
    // `change_config` payload (`ptr.deinit()` + `alloc.destroy(ptr)`) and the
    // `write_alloc` buffer (`alloc.free(data)`). In the port both payloads are
    // owned values (`ChangeConfig`'s `Box<DerivedConfig>`; `Alloc<u8>`'s
    // `Box<[u8]>`), so dropping the `Message` performs exactly these frees —
    // no explicit method is needed.
}

/// The payload of the KittyClipboardGrant* messages.
// pub const KittyClipboardGrant = struct { alloc: Allocator, pw: []const u8 };
// PORT NOTE: the reference's `alloc` + manually-freed `pw` collapse to an
// owned `Vec` (freed on drop, including through `Message`'s drop-deinit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KittyClipboardGrant {
    pub pw: Vec<u8>,
}

/// The types of size reports that we support.
//
// PORT NOTE: reference `pub const SizeReport = terminal.size_report.Style`,
// nested in `Message`; hoisted to module level like `WriteReq`.
pub type SizeReport = size_report::Style;

#[cfg(test)]
mod tests {
    use super::*;

    // test {
    //     std.testing.refAllDecls(@This());
    // }
    #[test]
    fn ref_all_decls() {
        // Reify the module's public declarations (the refAllDecls analogue).
        let _ = std::any::type_name::<Message<'static>>();
        let _ = std::any::type_name::<WriteReq<'static>>();
        let _ = std::any::type_name::<SizeReport>();
        let _: fn(&[u8]) -> Message<'static> = Message::writeReq;
        // Exercise both writeReq outcomes (small fit + alloc overflow).
        assert!(matches!(
            Message::writeReq(b"hello!"),
            Message::WriteSmall(_)
        ));
        assert!(matches!(
            Message::writeReq(&[0u8; 64]),
            Message::WriteAlloc(_)
        ));
    }

    // test {
    //     // Ensure we don't grow our IO message size without explicitly wanting to.
    //     const testing = std.testing;
    //     try testing.expectEqual(@as(usize, 40), @sizeOf(Message));
    // }
    #[test]
    fn message_size() {
        // Ensure we don't grow our IO message size without explicitly wanting to.
        //
        // PORT NOTE: the reference pins `@sizeOf(Message) == 40` for the Zig
        // union layout. The Rust enum layout differs mechanically (the
        // `WriteReq.Small` `len` is a `usize` rather than Zig's
        // `IntFittingRange(0, 38)` u6, and the discriminant is not packed into
        // padding), so the anti-growth contract is pinned at the equivalent
        // Rust-layout constant instead. The assertion's intent — a build break
        // on accidental growth — is preserved assertion-for-assertion.
        assert_eq!(56, core::mem::size_of::<Message>());
    }
}
