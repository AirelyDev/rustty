//! App — the core app state shared by all terminal surfaces.
//!
//! PORTS: ghostty src/App.zig
//!
//! PORT STATUS: SKELETON (core chunk C3) — CORE_BLUEPRINT §6 row C4 owns this
//! file (ALL of App.zig: the `App<A>` struct, create..performAllAction, the
//! full `Message` union, `Mailbox.push`, the Wasm block, + the
//! `#[cfg(test)] test_runtime` seam tests). Chunk C3 declares ONLY the
//! minimal `Mailbox<A>`/`Queue<A>`/`Message<A>` type surface, because
//! `renderer_seam::Thread<A>` embeds `app_mailbox: app::Mailbox<A>`
//! (renderer/Thread.zig:89 `app_mailbox: App.Mailbox`) one chunk before this
//! file's owner lands. The split is per the blueprint's C3 row ("declare the
//! minimal app module skeleton (struct Mailbox<A>) ... and let C4 fill it,
//! documenting the split"); C4 fills IN PLACE — the paths below never change.

use rustty_apprt_api as apprt;

/// The message types that can be sent to the app thread.
//
// PORT NOTE (C3 SKELETON — C4 fills): the reference is `pub const Message =
// union(enum) { open_config, new_window, close, quit, surface_message,
// redraw_surface }` (App.zig:527). The variants name `*Surface` and
// `*apprt.Surface`, hence the `A` parameter (CORE_BLUEPRINT §1); they land
// with C4's full App.zig port (together with the documented
// `unsafe impl Send` cross-thread contract). Until then this is an inhabited
// placeholder shell so that `Queue<A>` below is a real queue type — nothing
// constructs a `Message` before C4.
pub struct Message<A: apprt::App + 'static> {
    _skeleton: core::marker::PhantomData<A>,
}

/// The type used for sending messages to the app thread.
//
// PORT NOTE: reference `pub const Queue = BlockingQueue(Message, 64);` —
// declared INSIDE `Mailbox` in the reference (App.zig:562), hoisted to the
// module per the CORE_BLUEPRINT §1 signature (`pub type Queue<A>`).
// `Queue.Timeout`/`Queue.Size` resolve to the module-level
// `rustty_datastruct::blocking_queue::{Timeout, Size}` (the C0 `AppMailbox`
// concretization).
pub type Queue<A> = rustty_datastruct::BlockingQueue<Message<A>, 64>;

/// Mailbox is the way that other threads send the app thread messages.
//
// PORT NOTE: App.zig:560 `pub const Mailbox = struct { rt_app: *apprt.App,
// mailbox: *Queue, ... }` — raw pointers held across threads (the reference
// threading contract; CORE_BLUEPRINT §1). The `push` method (push + "Wake up
// our app loop" via `rt_app.wakeup()`) lands with C4.
pub struct Mailbox<A: apprt::App + 'static> {
    pub rt_app: *mut A,
    pub mailbox: *mut Queue<A>,
}

// PORT NOTE ([ADV-FIX 6a]): the reference copies `App.Mailbox` BY VALUE (Zig
// struct assignment — Surface.zig:550 `const app_mailbox: App.Mailbox =
// .{...}` then `.app = app_mailbox` / `rendererpkg.Thread.init(...,
// app_mailbox)`). A `#[derive(Clone, Copy)]` on a `Mailbox<A>` would wrongly
// demand `A: Copy`, so the impls are MANUAL with no bound on `A` (the fields
// are two raw pointers, always bitwise-copyable).
impl<A: apprt::App + 'static> Clone for Mailbox<A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<A: apprt::App + 'static> Copy for Mailbox<A> {}
