//! rustty-vt search/thread
//!
//! PORTS: ghostty src/terminal/search/Thread.zig
//!
//! PORT STATUS: complete (vt-15). Faithful port of the background search
//! `Thread`: the dedicated thread that searches a terminal for a string match,
//! interleaving incremental search work with an event loop that drains a
//! mailbox, refreshes terminal state on a timer, and emits search events.
//
//! Search thread that handles searching a terminal for a string match.
//! This is expected to run on a dedicated thread to try to prevent too much
//! overhead to other terminal read/write operations.
//!
//! The current architecture of search does acquire global locks for accessing
//! terminal data, so there's still added contention, but we do our best to
//! minimize this by trading off memory usage (copying data to minimize lock
//! time).
// pub const Thread = @This();
//
// const std = @import("std");
// const builtin = @import("builtin");
// const testing = std.testing;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const Mutex = std.Thread.Mutex;
// const xev = @import("../../global.zig").xev;
// const internal_os = @import("../../os/main.zig");
// const BlockingQueue = @import("../../datastruct/main.zig").BlockingQueue;
// const MessageData = @import("../../datastruct/main.zig").MessageData;
// const point = @import("../point.zig");
// const FlattenedHighlight = @import("../highlight.zig").Flattened;
// const UntrackedHighlight = @import("../highlight.zig").Untracked;
// const ScreenSet = @import("../ScreenSet.zig");
// const Selection = @import("../Selection.zig");
// const Terminal = @import("../Terminal.zig");
//
// const ScreenSearch = @import("screen.zig").ScreenSearch;
// const ViewportSearch = @import("viewport.zig").ViewportSearch;
//
// PORT NOTE: the reference imports the event-loop backend via
// `@import("../../global.zig").xev` — a re-export of `@import("xev").Dynamic`.
// In Rustty that re-export is a DEFERRED SEAM (`rustty_global::seam::Xev = ()`):
// xev (libxev) is an upward dependency that is not yet ported, which is exactly
// why the reference's `search.zig` gates `Thread` OUT of the `lib` artifact
// ("The search thread is not available in libghostty due to the xev dep for
// now."). Because the rustty-vt crate builds with `ARTIFACT == Artifact::Ghostty`
// (the application artifact, which DOES include `Thread`), we faithfully port the
// thread's runtime behavior by mirroring the small slice of the xev API the
// `Thread` actually uses (`Loop` / `Async` / `Timer` / `Completion` /
// `CallbackAction` / wait/notify/run/stop) in the in-crate `xev` module below.
// It is backed by real OS threading (`std::thread`) and std synchronization
// (`Mutex`/`Condvar`), matching the task directive to "port faithfully via
// std::thread / the rustty-os threading seam". Every reference statement is
// preserved verbatim as a comment above its translation; the seam reproduces
// xev's observable semantics (level-triggered async waits, one-shot timers,
// `.rearm`/`.disarm` callback actions, `run(.once)`/`run(.no_wait)`, and
// `loop.stop()`/`loop.stopped()`).
use super::screen::ScreenSearch;
use super::viewport::ViewportSearch;
use crate::highlight::Flattened as FlattenedHighlight;
use crate::highlight::Untracked as UntrackedHighlight;
use crate::point;
use crate::screen_set::Key;
use crate::screen_set::Key as ScreenSetKey;
use crate::terminal::Terminal;
use allocator_api2::alloc::Global as Allocator;
use rustty_datastruct::message_data::MessageData;
use rustty_datastruct::BlockingQueue;
// PORT NOTE: the shared search/renderer mutex is now the guard-less
// `rustty_zigstd::thread::Mutex` (Options.mutex, C0 model); std's `Mutex`
// remains ONLY for the test `TestUserData` fields (`reset_done`/`selected`/
// `viewport`), so this import is `#[cfg(test)]`-gated to avoid an unused-import
// warning in non-test builds.
#[cfg(test)]
use std::sync::Mutex;

// const log = std.log.scoped(.search_thread);
//
// PORT NOTE: the reference `log.debug`/`log.warn` calls are diagnostics on
// non-fatal paths; we mirror them with `eprintln!` behind the same message
// strings (the convention used elsewhere in this crate, e.g. search/screen.rs).

// TODO: Some stuff that could be improved:
// - pause the refresh timer when the terminal isn't focused
// - we probably want to know our progress through the search
//   for viewport matches so we can show n/total UI.
// - notifications should be coalesced to avoid spamming a massive
//   amount of events if the terminal is changing rapidly.

/// The interval at which we refresh the terminal state to check if
/// there are any changes that require us to re-search. This should be
/// balanced to be fast enough to be responsive but not so fast that
/// we hold the terminal lock too often.
// const REFRESH_INTERVAL = 24; // 40 FPS
const REFRESH_INTERVAL: u64 = 24; // 40 FPS

// Additive infra (R1 — necessary glue): realizes the reference's
// `self.opts.mutex.lock(); defer self.opts.mutex.unlock();` for the guard-less
// zigstd `Mutex` (which returns NO RAII guard). A tiny local `defer`-unlock
// guard keeps the existing early-`return`/`?` exits inside the locked regions
// (e.g. `select`) transparent — the unlock fires on the guard's `Drop`, exactly
// like the Zig `defer`. This is the same `defer`-unlock convention core uses for
// the shared renderer mutex.
struct MutexUnlock<'a>(&'a rustty_zigstd::thread::Mutex);
impl Drop for MutexUnlock<'_> {
    fn drop(&mut self) {
        self.0.unlock();
    }
}
#[inline]
fn lock_deferred(m: *const rustty_zigstd::thread::Mutex) -> MutexUnlock<'static> {
    // SAFETY: `m` is the shared renderer mutex (`self.opts.mutex`), valid for the
    // thread's whole lifetime (the reference's shared `*std.Thread.Mutex`).
    // Locked here; unlocked on the returned guard's drop.
    unsafe {
        (*m).lock();
        MutexUnlock(&*m)
    }
}

/// The background search thread.
//
// PORT NOTE: the reference `Thread = @This()` makes the file itself the struct;
// the fields below are its members. `alloc`, `loop`, `wakeup`, `stop`, and
// `refresh` are reproduced via the in-crate `xev` seam. The completion handles
// (`wakeup_c`/`stop_c`/`refresh_c`) carry per-handle state exactly as the
// reference `xev.Completion` defaults to `.{}` (idle).
pub struct Thread {
    /// Allocator used for some state
    // alloc: std.mem.Allocator,
    pub alloc: Allocator,

    /// The mailbox that can be used to send this thread messages. Note
    /// this is a blocking queue so if it is full you will get errors (or block).
    // mailbox: *Mailbox,
    pub mailbox: Box<Mailbox>,

    /// The event loop for the search thread.
    // loop: xev.Loop,
    pub loop_: xev::Loop,

    /// This can be used to wake up the renderer and force a render safely from
    /// any thread.
    // wakeup: xev.Async,
    // wakeup_c: xev.Completion = .{},
    pub wakeup: xev::Async,
    pub wakeup_c: xev::Completion,

    /// This can be used to stop the thread on the next loop iteration.
    // stop: xev.Async,
    // stop_c: xev.Completion = .{},
    pub stop: xev::Async,
    pub stop_c: xev::Completion,

    /// The timer used for refreshing the terminal state to determine if
    /// we have a stale active area, viewport, screen change, etc. This is
    /// CPU intensive so we stop doing this under certain conditions.
    // refresh: xev.Timer,
    // refresh_c: xev.Completion = .{},
    // refresh_active: bool = false,
    pub refresh: xev::Timer,
    pub refresh_c: xev::Completion,
    pub refresh_active: bool,

    /// Search state. Starts as null and is populated when a search is
    /// started (a needle is given).
    // search: ?Search = null,
    pub search: Option<Search>,

    /// The options used to initialize this thread.
    // opts: Options,
    pub opts: Options,
}

impl Thread {
    /// Initialize the thread. This does not START the thread. This only sets
    /// up all the internal state necessary prior to starting the thread. It
    /// is up to the caller to start the thread with the threadMain entrypoint.
    // pub fn init(alloc: Allocator, opts: Options) !Thread {
    pub fn init(alloc: Allocator, opts: Options) -> Result<Thread, xev::Error> {
        // The mailbox for messaging this thread
        // var mailbox = try Mailbox.create(alloc);
        // errdefer mailbox.destroy(alloc);
        let mailbox = Mailbox::create();

        // Create our event loop.
        // var loop = try xev.Loop.init(.{});
        // errdefer loop.deinit();
        let loop_ = xev::Loop::init()?;

        // This async handle is used to "wake up" the renderer and force a render.
        // var wakeup_h = try xev.Async.init();
        // errdefer wakeup_h.deinit();
        let wakeup_h = xev::Async::init()?;

        // This async handle is used to stop the loop and force the thread to end.
        // var stop_h = try xev.Async.init();
        // errdefer stop_h.deinit();
        let stop_h = xev::Async::init()?;

        // Refresh timer, see comments.
        // var refresh_h = try xev.Timer.init();
        // errdefer refresh_h.deinit();
        let refresh_h = xev::Timer::init()?;

        // return .{
        //     .alloc = alloc,
        //     .mailbox = mailbox,
        //     .loop = loop,
        //     .wakeup = wakeup_h,
        //     .stop = stop_h,
        //     .refresh = refresh_h,
        //     .opts = opts,
        // };
        Ok(Thread {
            alloc,
            mailbox,
            loop_,
            wakeup: wakeup_h,
            wakeup_c: xev::Completion::new(),
            stop: stop_h,
            stop_c: xev::Completion::new(),
            refresh: refresh_h,
            refresh_c: xev::Completion::new(),
            refresh_active: false,
            search: None,
            opts,
        })
    }

    /// Clean up the thread. This is only safe to call once the thread
    /// completes executing; the caller must join prior to this.
    // pub fn deinit(self: *Thread) void {
    pub fn deinit(&mut self) {
        // self.refresh.deinit();
        self.refresh.deinit();
        // self.wakeup.deinit();
        self.wakeup.deinit();
        // self.stop.deinit();
        self.stop.deinit();
        // self.loop.deinit();
        self.loop_.deinit();
        // Nothing can possibly access the mailbox anymore, destroy it.
        // self.mailbox.destroy(self.alloc);
        //
        // PORT NOTE: the reference `mailbox.destroy(alloc)` frees the heap
        // allocation; the Rust `Box<Mailbox>` is dropped here to mirror it.
        Mailbox::destroy(core::mem::replace(&mut self.mailbox, Mailbox::create()));

        // if (self.search) |*s| {
        //     self.opts.mutex.lock();
        //     defer self.opts.mutex.unlock();
        //     s.deinit(self.opts.terminal);
        // }
        //
        // Cleanup must hold the terminal lock: `Search::deinit` reads the live
        // ScreenSet to decide which searchers may still untrack their pins.
        let mutex = self.opts.mutex;
        let terminal = self.opts.terminal;
        if let Some(s) = self.search.as_mut() {
            let _guard = lock_deferred(mutex);
            s.deinit(terminal);
        }
    }

    /// The main entrypoint for the thread.
    // pub fn threadMain(self: *Thread) void {
    pub fn thread_main(&mut self) {
        // Call child function so we can use errors...
        // self.threadMain_() catch |err| {
        //     // In the future, we should expose this on the thread struct.
        //     log.warn("search thread err={}", .{err});
        // };
        if let Err(err) = self.thread_main_() {
            // In the future, we should expose this on the thread struct.
            eprintln!("warn(search_thread): search thread err={:?}", err);
        }
    }

    // fn threadMain_(self: *Thread) !void {
    fn thread_main_(&mut self) -> Result<(), xev::Error> {
        // defer log.debug("search thread exited", .{});
        // PORT NOTE: the `defer` debug log is emitted at every return; mirrored
        // by the `ThreadExitedLog` drop guard below.
        let _exited_log = ThreadExitedLog;

        // Right now, on Darwin, `std.Thread.setName` can only name the current
        // thread, and we have no way to get the current thread from within it,
        // so instead we use this code to name the thread instead.
        // if (comptime builtin.os.tag.isDarwin()) {
        #[cfg(target_os = "macos")]
        {
            // internal_os.macos.pthread_setname_np(&"search".*);
            unsafe {
                rustty_os::macos::pthread_setname_np(c"search".as_ptr());
            }

            // We can run with lower priority than other threads.
            // const class: internal_os.macos.QosClass = .utility;
            let class: rustty_os::macos::QosClass = rustty_os::macos::QosClass::utility;
            // if (internal_os.macos.setQosClass(class)) {
            //     log.debug("thread QoS class set class={}", .{class});
            // } else |err| {
            //     log.warn("error setting QoS class err={}", .{err});
            // }
            match rustty_os::macos::setQosClass(class) {
                Ok(()) => {
                    // PORT NOTE: `QosClass` does not derive `Debug`; format the
                    // raw discriminant (the reference prints the enum tag).
                    eprintln!(
                        "debug(search_thread): thread QoS class set class={}",
                        class as u32
                    );
                }
                Err(err) => {
                    eprintln!("warn(search_thread): error setting QoS class err={:?}", err);
                }
            }
        }

        // Start the async handlers
        // self.wakeup.wait(&self.loop, &self.wakeup_c, Thread, self, wakeupCallback);
        // self.stop.wait(&self.loop, &self.stop_c, Thread, self, stopCallback);
        //
        // PORT NOTE: in the reference, xev callbacks receive a `?*Thread`
        // userdata. To avoid creating an aliasing `*mut Thread` while we also
        // hold `&mut self`, we register the wait handlers by HANDLE (an opaque
        // id) and a callback discriminant, then dispatch the corresponding
        // method on `self` after the loop run reports which completions fired.
        // This preserves the level-triggered async-wait + `.rearm`/`.disarm`
        // semantics exactly.
        self.wakeup
            .wait(&self.loop_, &mut self.wakeup_c, xev::Callback::Wakeup);
        self.stop
            .wait(&self.loop_, &mut self.stop_c, xev::Callback::Stop);

        // Send an initial wakeup so we drain our mailbox immediately.
        // try self.wakeup.notify();
        self.wakeup.notify()?;

        // Start the refresh timer
        // self.startRefreshTimer();
        self.start_refresh_timer();

        // Run
        // log.debug("starting search thread", .{});
        eprintln!("debug(search_thread): starting search thread");
        // defer {
        //     log.debug("starting search thread shutdown", .{});
        //     // Send the quit message
        //     if (self.opts.event_cb) |cb| {
        //         cb(.quit, self.opts.event_userdata);
        //     }
        // }
        //
        // PORT NOTE: this `defer` runs the shutdown quit-callback at every
        // return from `thread_main_`. The body of the loop is `loop_body`; the
        // shutdown block is run once, unconditionally, after it returns.
        let result = self.thread_main_loop();

        // -- begin deferred shutdown block --
        eprintln!("debug(search_thread): starting search thread shutdown");
        // Send the quit message
        if let Some(cb) = self.opts.event_cb {
            cb(Event::Quit, self.opts.event_userdata);
        }
        // -- end deferred shutdown block --

        result
    }

    // The `while (true)` body of `threadMain_`, split out so the reference's
    // shutdown `defer` (the quit callback) runs at every exit.
    fn thread_main_loop(&mut self) -> Result<(), xev::Error> {
        // Unlike some of our other threads, we interleave search work
        // with our xev loop so that we can try to make forward search progress
        // while also listening for messages.
        // while (true) {
        loop {
            // If our loop is canceled then we drain our messages and quit.
            // if (self.loop.stopped()) {
            if self.loop_.stopped() {
                // while (self.mailbox.pop()) |message| {
                //     log.debug("mailbox message ignored during shutdown={}", .{message});
                // }
                while let Some(message) = self.mailbox.pop() {
                    eprintln!(
                        "debug(search_thread): mailbox message ignored during shutdown={:?}",
                        message
                    );
                }

                // return;
                return Ok(());
            }

            // const s: *Search = if (self.search) |*s| s else {
            //     // If we're not actively searching, we can block the loop
            //     // until it does some work.
            //     try self.loop.run(.once);
            //     continue;
            // };
            if self.search.is_none() {
                // If we're not actively searching, we can block the loop
                // until it does some work.
                self.run_loop(xev::RunMode::Once)?;
                continue;
            }

            // If we have an active search, we always send any pending
            // notifications. Even if the search is complete, there may be
            // notifications to send.
            // if (self.opts.event_cb) |cb| {
            //     s.notify(
            //         self.alloc,
            //         cb,
            //         self.opts.event_userdata,
            //     );
            // }
            if let Some(cb) = self.opts.event_cb {
                let alloc = self.alloc;
                let ud = self.opts.event_userdata;
                self.search.as_mut().unwrap().notify(alloc, cb, ud);
            }

            // if (s.isComplete()) {
            //     // If our search is complete, there's no more work to do, we
            //     // can block until we have an xev action.
            //     try self.loop.run(.once);
            //     continue;
            // }
            if self.search.as_mut().unwrap().is_complete() {
                // If our search is complete, there's no more work to do, we
                // can block until we have an xev action.
                self.run_loop(xev::RunMode::Once)?;
                continue;
            }

            // Tick the search. This will trigger any event callbacks, lock
            // for data loading, etc.
            // switch (s.tick()) {
            match self.search.as_mut().unwrap().tick() {
                // We're complete now when we were not before. Notify!
                // .complete => {},
                Search_Tick::Complete => {}

                // Forward progress was made.
                // .progress => {},
                Search_Tick::Progress => {}

                // All searches are blocked. Let's grab the lock and feed data.
                // .blocked => {
                //     self.opts.mutex.lock();
                //     defer self.opts.mutex.unlock();
                //     s.feed(self.alloc, self.opts.terminal);
                // },
                Search_Tick::Blocked => {
                    let _guard = lock_deferred(self.opts.mutex);
                    let alloc = self.alloc;
                    let terminal = self.opts.terminal;
                    self.search.as_mut().unwrap().feed(alloc, terminal);
                }
            }

            // We have an active search, so we only want to process messages
            // we have but otherwise return immediately so we can continue the
            // search. If the above completed the search, we still want to
            // go around the loop as quickly as possible to send notifications,
            // and then we'll block on the loop next time.
            // try self.loop.run(.no_wait);
            self.run_loop(xev::RunMode::NoWait)?;
        }
    }

    // Run the event loop in the given mode and dispatch any completions that
    // fired during the run. This is the Rustty analogue of `self.loop.run(mode)`
    // for the xev seam: xev would invoke the registered callbacks inline, but to
    // avoid aliasing `self`, the seam records which callbacks fired and we
    // dispatch them here against `&mut self`.
    fn run_loop(&mut self, mode: xev::RunMode) -> Result<(), xev::Error> {
        // const fired = self.loop.run(mode);  (collected callbacks)
        let fired = self.loop_.run(mode)?;
        for cb in fired {
            match cb {
                // wakeupCallback
                xev::Callback::Wakeup => {
                    let action = self.wakeup_callback();
                    self.loop_.apply_action(&mut self.wakeup_c, action);
                }
                // stopCallback
                xev::Callback::Stop => {
                    let action = self.stop_callback();
                    self.loop_.apply_action(&mut self.stop_c, action);
                }
                // refreshCallback
                xev::Callback::Refresh => {
                    let action = self.refresh_callback();
                    self.loop_.apply_action(&mut self.refresh_c, action);
                }
            }
        }
        Ok(())
    }

    /// Drain the mailbox.
    // fn drainMailbox(self: *Thread) !void {
    fn drain_mailbox(&mut self) -> Result<(), DrainError> {
        // while (self.mailbox.pop()) |message| {
        while let Some(message) = self.mailbox.pop() {
            // log.debug("mailbox message={}", .{message});
            eprintln!("debug(search_thread): mailbox message={:?}", message);
            // switch (message) {
            match message {
                // .change_needle => |v| {
                //     defer v.deinit();
                //     try self.changeNeedle(v.slice());
                // },
                Message::ChangeNeedle(v) => {
                    // PORT NOTE: the reference `v.deinit()` frees the
                    // `MessageData` alloc variant on scope exit; the Rust
                    // `MessageData`'s owned `Box` drops at the end of this arm,
                    // so we copy the slice out first (the borrow ends before
                    // `change_needle` mutates `self`).
                    let needle: Vec<u8> = v.slice().to_vec();
                    self.change_needle(&needle)?;
                }
                // .select => |v| try self.select(v),
                Message::Select(v) => self.select(v)?,
            }
        }
        Ok(())
    }

    // fn select(self: *Thread, sel: ScreenSearch.Select) !void {
    // PORT NOTE: dereferences the raw `self.opts.terminal`/`screen` pointers
    // under the held mutex; lint allowed per the crate `*Terminal`/`*Screen`
    // convention. The `for i in 0..hl_chunks.len()` mirrors the reference
    // `for (0..hl_chunks.len) |i|` verbatim.
    #[allow(clippy::not_unsafe_ptr_arg_deref, clippy::needless_range_loop)]
    fn select(&mut self, sel: super::screen::Select) -> Result<(), DrainError> {
        // const s = if (self.search) |*s| s else return;
        if self.search.is_none() {
            return Ok(());
        }
        // self.opts.mutex.lock();
        // defer self.opts.mutex.unlock();
        let _guard = lock_deferred(self.opts.mutex);

        // A screen can be removed or replaced between refresh ticks. Reconcile
        // while holding the terminal lock before touching any ScreenSearch pins.
        // s.feed(self.alloc, self.opts.terminal);
        let alloc = self.alloc;
        let terminal = self.opts.terminal;
        self.search.as_mut().unwrap().feed(alloc, terminal);

        // const screen_search = s.screens.getPtr(s.last_screen.key) orelse return;
        let last_key = self.search.as_ref().unwrap().last_screen.key;
        if self
            .search
            .as_mut()
            .unwrap()
            .screens
            .get_ptr(last_key)
            .is_none()
        {
            return Ok(());
        }

        // Make the selection. Ignore the result because we don't
        // care if the selection didn't change.
        // _ = try screen_search.select(sel);
        let screen_search = self
            .search
            .as_mut()
            .unwrap()
            .screens
            .get_ptr(last_key)
            .unwrap();
        screen_search.select(sel)?;

        // Grab our match if we have one. If we don't have a selection
        // then we do nothing.
        // const flattened = screen_search.selectedMatch() orelse return;
        let flattened = match screen_search.selected_match() {
            Some(f) => f,
            None => return Ok(()),
        };

        // No matter what we reset our selected match cache. This will
        // trigger a callback which will trigger the renderer to wake up
        // so it can be notified the screen scrolled.
        // s.last_screen.selected = null;
        self.search.as_mut().unwrap().last_screen.selected = None;

        // Grab the current screen and see if this match is visible within
        // the viewport already. If it is, we do nothing.
        // const screen = self.opts.terminal.screens.get(
        //     s.last_screen.key,
        // ) orelse return;
        let screen: *mut crate::screen::Screen =
            match unsafe { (*self.opts.terminal).screens.get(last_key) } {
                Some(s) => s,
                None => return Ok(()),
            };

        // Grab the viewport. Viewports and selections are usually small
        // so this check isn't very expensive, despite appearing O(N^2),
        // both Ns are usually equal to 1.
        // var it = screen.pages.pageIterator(
        //     .right_down,
        //     .{ .viewport = .{} },
        //     null,
        // );
        let mut it = unsafe {
            (*screen).pages.page_iterator(
                crate::pagelist::iter::Direction::RightDown,
                point::Point::Viewport(point::Coordinate { x: 0, y: 0 }),
                None,
            )
        };
        // const hl_chunks = flattened.chunks.slice();
        let hl_chunks = &flattened.chunks;
        // while (it.next()) |chunk| {
        while let Some(chunk) = it.next() {
            // for (0..hl_chunks.len) |i| {
            for i in 0..hl_chunks.len() {
                // const hl_chunk = hl_chunks.get(i);
                let hl_chunk = &hl_chunks[i];
                // if (chunk.overlaps(.{
                //     .node = hl_chunk.node,
                //     .start = hl_chunk.start,
                //     .end = hl_chunk.end,
                // })) return;
                if chunk.overlaps(crate::pagelist::iter::Chunk {
                    node: hl_chunk.node,
                    start: hl_chunk.start,
                    end: hl_chunk.end,
                }) {
                    return Ok(());
                }
            }
        }

        // screen.scroll(.{ .pin = flattened.startPin() });
        unsafe { (*screen).scroll(crate::screen::Scroll::Pin(flattened.start_pin())) };
        Ok(())
    }

    /// Change the search term to the given value.
    // fn changeNeedle(self: *Thread, needle: []const u8) !void {
    fn change_needle(&mut self, needle: &[u8]) -> Result<(), DrainError> {
        // log.debug("changing search needle to '{s}'", .{needle});
        eprintln!(
            "debug(search_thread): changing search needle to '{}'",
            String::from_utf8_lossy(needle)
        );

        // Stop the previous search
        // if (self.search) |*s| {
        if self.search.is_some() {
            // If our search is unchanged, do nothing.
            // if (std.ascii.eqlIgnoreCase(s.viewport.needle(), needle)) return;
            if ascii_eql_ignore_case(self.search.as_ref().unwrap().viewport.needle(), needle) {
                return Ok(());
            }

            // {
            //     self.opts.mutex.lock();
            //     defer self.opts.mutex.unlock();
            //     s.deinit(self.opts.terminal);
            // }
            // self.search = null;
            {
                let mutex = self.opts.mutex;
                let terminal = self.opts.terminal;
                let _guard = lock_deferred(mutex);
                self.search.as_mut().unwrap().deinit(terminal);
            }
            self.search = None;

            // When the search changes then we need to emit that it stopped.
            // if (self.opts.event_cb) |cb| {
            if let Some(cb) = self.opts.event_cb {
                // cb(
                //     .{ .total_matches = 0 },
                //     self.opts.event_userdata,
                // );
                cb(Event::TotalMatches(0), self.opts.event_userdata);
                // cb(
                //     .{ .selected_match = null },
                //     self.opts.event_userdata,
                // );
                cb(Event::SelectedMatch(None), self.opts.event_userdata);
                // cb(
                //     .{ .viewport_matches = &.{} },
                //     self.opts.event_userdata,
                // );
                cb(Event::ViewportMatches(&[]), self.opts.event_userdata);
            }
        }

        // No needle means stop the search.
        // if (needle.len == 0) return;
        if needle.is_empty() {
            return Ok(());
        }

        // Setup our search state.
        // self.search = try .init(self.alloc, needle);
        self.search = Some(Search::init(self.alloc, needle)?);

        // We need to grab the terminal lock and do an initial feed.
        // self.opts.mutex.lock();
        // defer self.opts.mutex.unlock();
        let _guard = lock_deferred(self.opts.mutex);
        // self.search.?.feed(self.alloc, self.opts.terminal);
        let alloc = self.alloc;
        let terminal = self.opts.terminal;
        self.search.as_mut().unwrap().feed(alloc, terminal);
        Ok(())
    }

    // fn startRefreshTimer(self: *Thread) void {
    fn start_refresh_timer(&mut self) {
        // Set our active state so it knows we're running. We set this before
        // even checking the active state in case we have a pending shutdown.
        // self.refresh_active = true;
        self.refresh_active = true;

        // If our timer is already active, then we don't have to do anything.
        // if (self.refresh_c.state() == .active) return;
        if self.refresh_c.state() == xev::CompletionState::Active {
            return;
        }

        // Start the timer which loops
        // self.refresh.run(
        //     &self.loop,
        //     &self.refresh_c,
        //     REFRESH_INTERVAL,
        //     Thread,
        //     self,
        //     refreshCallback,
        // );
        self.refresh.run(
            &self.loop_,
            &mut self.refresh_c,
            REFRESH_INTERVAL,
            xev::Callback::Refresh,
        );
    }

    // fn stopRefreshTimer(self: *Thread) void {
    #[allow(dead_code)]
    fn stop_refresh_timer(&mut self) {
        // This will stop the refresh on the next iteration.
        // self.refresh_active = false;
        self.refresh_active = false;
    }

    // fn wakeupCallback(
    //     self_: ?*Thread,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Async.WaitError!void,
    // ) xev.CallbackAction {
    fn wakeup_callback(&mut self) -> xev::CallbackAction {
        // _ = r catch |err| {
        //     log.warn("error in wakeup err={}", .{err});
        //     return .rearm;
        // };
        //
        // PORT NOTE: the seam's async wait never yields an error to the
        // callback (notify is infallible once the handle is initialized), so
        // the reference's error branch is unreachable; we keep the success path.

        // const self = self_.?;

        // When we wake up, we drain the mailbox. Mailbox producers should
        // wake up our thread after publishing.
        // self.drainMailbox() catch |err|
        //     log.warn("error draining mailbox err={}", .{err});
        if let Err(err) = self.drain_mailbox() {
            eprintln!("warn(search_thread): error draining mailbox err={:?}", err);
        }

        // return .rearm;
        xev::CallbackAction::Rearm
    }

    // fn stopCallback(
    //     self_: ?*Thread,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Async.WaitError!void,
    // ) xev.CallbackAction {
    fn stop_callback(&mut self) -> xev::CallbackAction {
        // _ = r catch unreachable;
        // self_.?.loop.stop();
        self.loop_.stop();
        // return .disarm;
        xev::CallbackAction::Disarm
    }

    // fn refreshCallback(
    //     self_: ?*Thread,
    //     _: *xev.Loop,
    //     _: *xev.Completion,
    //     r: xev.Timer.RunError!void,
    // ) xev.CallbackAction {
    fn refresh_callback(&mut self) -> xev::CallbackAction {
        // _ = r catch unreachable;
        // const self: *Thread = self_ orelse {
        //     // This shouldn't happen so we log it.
        //     log.warn("refresh callback fired without data set", .{});
        //     return .disarm;
        // };
        //
        // PORT NOTE: `self` is always present in the seam (dispatched against
        // `&mut self`), so the reference's null branch is unreachable here.

        // Run our feed if we have a search active.
        // if (self.search) |*s| {
        //     self.opts.mutex.lock();
        //     defer self.opts.mutex.unlock();
        //     s.feed(self.alloc, self.opts.terminal);
        // }
        let alloc = self.alloc;
        let terminal = self.opts.terminal;
        let mutex = self.opts.mutex;
        if let Some(s) = self.search.as_mut() {
            let _guard = lock_deferred(mutex);
            s.feed(alloc, terminal);
        }

        // Only continue if we're still active
        // if (self.refresh_active) self.refresh.run(
        //     &self.loop,
        //     &self.refresh_c,
        //     REFRESH_INTERVAL,
        //     Thread,
        //     self,
        //     refreshCallback,
        // );
        if self.refresh_active {
            self.refresh.run(
                &self.loop_,
                &mut self.refresh_c,
                REFRESH_INTERVAL,
                xev::Callback::Refresh,
            );
        }

        // return .disarm;
        xev::CallbackAction::Disarm
    }
}

// `defer log.debug("search thread exited", .{});` — emitted at every return
// from `thread_main_`.
struct ThreadExitedLog;
impl Drop for ThreadExitedLog {
    fn drop(&mut self) {
        eprintln!("debug(search_thread): search thread exited");
    }
}

/// `eqlIgnoreCase` for the `change_needle` short-circuit.
// PORT NOTE: reference `std.ascii.eqlIgnoreCase`.
fn ascii_eql_ignore_case(a: &[u8], b: &[u8]) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Error type for `drain_mailbox`/`select`/`change_needle` (the reference's
/// `!void` with `Allocator.Error` + `ScreenSearch` errors). All the fallible
/// callees produce `OutOfMemory`.
#[derive(Clone, Copy, Debug)]
pub enum DrainError {
    OutOfMemory,
}

impl From<crate::pagelist::OutOfMemory> for DrainError {
    fn from(_: crate::pagelist::OutOfMemory) -> Self {
        DrainError::OutOfMemory
    }
}

// pub const Options = struct {
pub struct Options {
    /// Mutex that must be held while reading/writing the terminal.
    // mutex: *Mutex,   where  Mutex = std.Thread.Mutex
    //
    // PORT NOTE: the shared renderer/search mutex is the guard-less Zig-shaped
    // `rustty_zigstd::thread::Mutex` (the core C0 mutex-model decision). The raw
    // pointer mirrors the reference's shared `*std.Thread.Mutex`; it is locked
    // under `unsafe` at the same sites the reference calls `mutex.lock()`.
    pub mutex: *const rustty_zigstd::thread::Mutex,

    /// The terminal data to search.
    // terminal: *Terminal,
    pub terminal: *mut Terminal,

    /// The callback for events from the search thread along with optional
    /// userdata. This can be null if you don't want to receive events,
    /// which could be useful for a one-time search (although, odd, you
    /// should use our search structures directly then).
    // event_cb: ?EventCallback = null,
    // event_userdata: ?*anyopaque = null,
    pub event_cb: Option<EventCallback>,
    pub event_userdata: *mut core::ffi::c_void,
}

// pub const EventCallback = *const fn (event: Event, userdata: ?*anyopaque) void;
pub type EventCallback = fn(event: Event, userdata: *mut core::ffi::c_void);

/// The type used for sending messages to the thread.
// pub const Mailbox = BlockingQueue(Message, 64);
pub type Mailbox = BlockingQueue<Message, 64>;

/// The messages that can be sent to the thread.
// pub const Message = union(enum) {
//
// PORT NOTE: the reference union's largest variant is `WriteReq`
// (`MessageData(u8, 255)`, the 256-byte small buffer) — the magic number is
// chosen precisely so the union is that size. The Rust enum has the same large
// `ChangeNeedle` variant; the size disparity with `Select` is by design, so the
// `large_enum_variant` lint is allowed (boxing would change the layout the
// reference deliberately fixes).
#[allow(clippy::large_enum_variant)]
pub enum Message {
    /// Represents a write request. Magic number comes from the max size
    /// we want this union to be.
    // pub const WriteReq = MessageData(u8, 255);

    /// Change the search term. If no prior search term is given this
    /// will start a search. If an existing search term is given this will
    /// stop the prior search and start a new one.
    // change_needle: WriteReq,
    ChangeNeedle(WriteReq),

    /// Select a search result.
    // select: ScreenSearch.Select,
    Select(super::screen::Select),
}

/// `WriteReq = MessageData(u8, 255)`.
pub type WriteReq = MessageData<'static, u8, 255>;

impl core::fmt::Debug for Message {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Message::ChangeNeedle(_) => f.write_str("change_needle"),
            Message::Select(_) => f.write_str("select"),
        }
    }
}

/// Events that can be emitted from the search thread. The caller
/// chooses to handle these as they see fit.
// pub const Event = union(enum) {
pub enum Event<'a> {
    /// Search is quitting. The search thread is exiting.
    // quit,
    Quit,

    /// Search is complete for the given needle on all screens.
    // complete,
    Complete,

    /// Total matches on the current active screen have changed.
    // total_matches: usize,
    TotalMatches(usize),

    /// Selected match changed.
    // selected_match: ?SelectedMatch,
    SelectedMatch(Option<SelectedMatch>),

    /// Matches in the viewport have changed. The memory is owned by the
    /// search thread and is only valid during the callback.
    // viewport_matches: []const FlattenedHighlight,
    ViewportMatches(&'a [FlattenedHighlight]),
}

// pub const SelectedMatch = struct {
//     idx: usize,
//     highlight: FlattenedHighlight,
// };
pub struct SelectedMatch {
    pub idx: usize,
    pub highlight: FlattenedHighlight,
}

/// Search state.
// const Search = struct {
pub struct Search {
    /// Active viewport search for the active screen.
    // viewport: ViewportSearch,
    pub viewport: ViewportSearch,

    /// The searchers for all the screens.
    // screens: std.EnumMap(ScreenSet.Key, ScreenSearch),
    //
    // PORT NOTE: the reference `std.EnumMap(ScreenSet.Key, ScreenSearch)` is
    // ported to the per-key `Option<ScreenSearch>` array used elsewhere in this
    // crate for `EnumMap` (see `ScreenSet.all`), keyed by `Key as usize`.
    pub screens: ScreenSearchMap,

    /// ScreenSet generations captured when each searcher was initialized.
    /// Allocators may reuse a destroyed Screen address, so pointer equality
    /// alone cannot distinguish replacement screens from stale handles.
    // screen_generations: std.EnumMap(ScreenSet.Key, usize),
    //
    // PORT NOTE: `std.EnumMap(ScreenSet.Key, usize)` — an EnumMap entry can be
    // ABSENT, and `screenIsValid` distinguishes absent from present, so this is
    // a per-key `Option<usize>` array (NOT `[usize; COUNT]` like
    // `ScreenSet.generations`, which is always populated).
    pub screen_generations: [Option<usize>; Key::COUNT],

    /// All state related to screen switches, collected so that when
    /// we switch screens it makes everything related stale, too.
    // last_screen: ScreenState,
    pub last_screen: ScreenState,

    /// True if we sent the complete notification yet.
    // last_complete: bool,
    pub last_complete: bool,

    /// The last viewport matches we found.
    // stale_viewport_matches: bool,
    pub stale_viewport_matches: bool,
}

// const ScreenState = struct {
pub struct ScreenState {
    /// Last active screen key
    // key: ScreenSet.Key,
    pub key: ScreenSetKey,

    /// Last notified total matches count
    // total: ?usize = null,
    pub total: Option<usize>,

    /// Last notified selected match index
    // selected: ?SelectedMatch = null,
    pub selected: Option<ScreenStateSelectedMatch>,
}

// const SelectedMatch = struct {
//     idx: usize,
//     highlight: UntrackedHighlight,
// };
pub struct ScreenStateSelectedMatch {
    pub idx: usize,
    pub highlight: UntrackedHighlight,
}

/// `std.EnumMap(ScreenSet.Key, ScreenSearch)` ported as a per-key array.
pub struct ScreenSearchMap {
    entries: [Option<ScreenSearch>; Key::COUNT],
}

impl ScreenSearchMap {
    // .init(.{})
    fn init() -> ScreenSearchMap {
        ScreenSearchMap {
            entries: [None, None],
        }
    }

    // self.screens.getPtr(key)
    fn get_ptr(&mut self, key: ScreenSetKey) -> Option<&mut ScreenSearch> {
        self.entries[key as usize].as_mut()
    }

    // self.screens.get(key)
    fn get(&self, key: ScreenSetKey) -> Option<&ScreenSearch> {
        self.entries[key as usize].as_ref()
    }

    // self.screens.contains(key)
    fn contains(&self, key: ScreenSetKey) -> bool {
        self.entries[key as usize].is_some()
    }

    // self.screens.put(key, value)
    fn put(&mut self, key: ScreenSetKey, value: ScreenSearch) {
        self.entries[key as usize] = Some(value);
    }

    // self.screens.remove(key)
    fn remove(&mut self, key: ScreenSetKey) {
        self.entries[key as usize] = None;
    }
}

/// Result of `Search.tick`.
// pub const Tick = enum {
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum Search_Tick {
    /// All searches are complete.
    // complete,
    Complete,

    /// Progress was made on at least one screen.
    // progress,
    Progress,

    /// All incomplete searches are blocked on feed.
    // blocked,
    Blocked,
}

impl Search {
    // pub fn init(
    //     alloc: Allocator,
    //     needle: []const u8,
    // ) Allocator.Error!Search {
    pub fn init(alloc: Allocator, needle: &[u8]) -> Result<Search, crate::pagelist::OutOfMemory> {
        // var vp: ViewportSearch = try .init(alloc, needle);
        // errdefer vp.deinit();
        let mut vp: ViewportSearch = ViewportSearch::init(alloc, needle)?;

        // We use dirty tracking for active area changes. Start with it
        // dirty so the first change is re-searched.
        // vp.active_dirty = true;
        vp.active_dirty = Some(true);

        // return .{
        //     .viewport = vp,
        //     .screens = .init(.{}),
        //     .screen_generations = .init(.{}),
        //     .last_screen = .{ .key = .primary },
        //     .last_complete = false,
        //     .stale_viewport_matches = true,
        // };
        Ok(Search {
            viewport: vp,
            screens: ScreenSearchMap::init(),
            screen_generations: [None, None],
            last_screen: ScreenState {
                key: ScreenSetKey::Primary,
                total: None,
                selected: None,
            },
            last_complete: false,
            stale_viewport_matches: true,
        })
    }

    // pub fn deinit(self: *Search, t: *Terminal) void {
    //
    // PORT NOTE: dereferences the raw `*Terminal` to read its `ScreenSet`; the
    // caller holds the shared mutex (see `Thread::deinit` /
    // `Thread::change_needle`), matching the reference.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn deinit(&mut self, t: *const Terminal) {
        // self.viewport.deinit();
        self.viewport.deinit();
        // var it = self.screens.iterator();
        // while (it.next()) |entry| {
        //     if (self.screenIsValid(&t.screens, entry.key, entry.value)) {
        //         entry.value.deinit();
        //     } else {
        //         entry.value.deinitScreenInvalid();
        //     }
        // }
        // SB: raw `addr_of!` (reference `&t.screens`), NOT `&(*t).screens` — the
        // searchers hold live raw aliases into these screens' PageLists, so no
        // reference into the Terminal is formed here
        // (rust-mut-args-strongly-protected).
        let screens: *const crate::screen_set::ScreenSet =
            unsafe { core::ptr::addr_of!((*t).screens) };
        for key_i in 0..Key::COUNT {
            let entry_key = Self::key_from_index(key_i);
            // The reference's `screenIsValid` takes `*const ScreenSearch` only
            // to read `search.screen`; passing that pointer directly keeps the
            // `&self` borrow disjoint from the `&mut` entry borrow below.
            let entry_screen = match self.screens.entries[key_i].as_ref() {
                Some(ss) => ss.screen,
                None => continue,
            };
            let valid = self.screen_is_valid(screens, entry_key, entry_screen);
            let entry = self.screens.entries[key_i].as_mut().unwrap();
            if valid {
                entry.deinit();
            } else {
                entry.deinit_screen_invalid();
            }
        }
    }

    /// `std.EnumMap` iteration order — key index to key.
    fn key_from_index(key_i: usize) -> ScreenSetKey {
        match key_i {
            0 => ScreenSetKey::Primary,
            _ => ScreenSetKey::Alternate,
        }
    }

    // fn screenIsValid(
    //     self: *const Search,
    //     screens: *const ScreenSet,
    //     key: ScreenSet.Key,
    //     search: *const ScreenSearch,
    // ) bool {
    //
    // PORT NOTE: takes the searcher's `screen` pointer rather than the whole
    // `*const ScreenSearch` — that field is all the reference reads, and this
    // keeps the borrow disjoint from the caller's `&mut` on the same entry.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    fn screen_is_valid(
        &self,
        screens: *const crate::screen_set::ScreenSet,
        key: ScreenSetKey,
        search_screen: *mut crate::screen::Screen,
    ) -> bool {
        // const generation = self.screen_generations.get(key) orelse return false;
        let generation = match self.screen_generations[key as usize] {
            Some(g) => g,
            None => return false,
        };
        // if (generation != screens.generation(key)) return false;
        if generation != unsafe { (*screens).generation(key) } {
            return false;
        }
        // const actual = screens.get(key) orelse return false;
        let actual = match unsafe { (*screens).get(key) } {
            Some(a) => a,
            None => return false,
        };
        // return actual == search.screen;
        core::ptr::eq(actual, search_screen)
    }

    /// Returns true if all searches on all screens are complete.
    // pub fn isComplete(self: *Search) bool {
    pub fn is_complete(&self) -> bool {
        // var it = self.screens.iterator();
        // while (it.next()) |entry| {
        //     if (!entry.value.state.isComplete()) return false;
        // }
        for s in self.screens.entries.iter().flatten() {
            if !s.state.is_complete() {
                return false;
            }
        }

        // return true;
        true
    }

    /// Tick the search forward as much as possible without acquiring
    /// the big lock. Returns the overall tick progress.
    // pub fn tick(self: *Search) Tick {
    pub fn tick(&mut self) -> Search_Tick {
        // var result: Tick = .complete;
        let mut result: Search_Tick = Search_Tick::Complete;
        // var it = self.screens.iterator();
        // while (it.next()) |entry| {
        for key_i in 0..Key::COUNT {
            let entry_key = match key_i {
                0 => ScreenSetKey::Primary,
                _ => ScreenSetKey::Alternate,
            };
            let entry = match self.screens.entries[key_i].as_mut() {
                Some(e) => e,
                None => continue,
            };
            // if (entry.value.tick()) {
            //     result = .progress;
            // } else |err| switch (err) {
            match entry.tick() {
                Ok(()) => {
                    result = Search_Tick::Progress;
                }
                // Ignore... nothing we can do.
                // error.OutOfMemory => log.warn(
                //     "error ticking screen search key={} err={}",
                //     .{ entry.key, err },
                // ),
                Err(super::screen::TickError::OutOfMemory) => {
                    eprintln!(
                        "warn(search_thread): error ticking screen search key={:?} err={:?}",
                        entry_key,
                        super::screen::TickError::OutOfMemory
                    );
                }

                // Ignore, good for us. State remains whatever it is.
                // error.SearchComplete => {},
                Err(super::screen::TickError::SearchComplete) => {}

                // Ignore, too, progressed
                // error.FeedRequired => switch (result) {
                Err(super::screen::TickError::FeedRequired) => match result {
                    // If we think we're complete, we're not because we're
                    // blocked now (nothing made progress).
                    // .complete => result = .blocked,
                    Search_Tick::Complete => result = Search_Tick::Blocked,

                    // If we made some progress, we remain in progress
                    // since blocked means no progress at all.
                    // .progress => {},
                    Search_Tick::Progress => {}

                    // If we're blocked already then we remain blocked.
                    // .blocked => {},
                    Search_Tick::Blocked => {}
                },
            }
        }

        // log.debug("tick result={}", .{result});
        // return result;
        result
    }

    /// Grab the mutex and update any state that requires it, such as
    /// feeding additional data to the searches or updating the active screen.
    // pub fn feed(
    //     self: *Search,
    //     alloc: Allocator,
    //     t: *Terminal,
    // ) void {
    // PORT NOTE: `t` is a raw pointer (reference `*Terminal`); the body
    // dereferences it under the held mutex, so the lint is allowed (the
    // convention used across this crate for `*Terminal`/`*Screen` args).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn feed(&mut self, alloc: Allocator, t: *mut Terminal) {
        // Update our active screen
        // if (t.screens.active_key != self.last_screen.key) {
        if unsafe { (*t).screens.active_key } != self.last_screen.key {
            // The default values will force resets of a bunch of other
            // state too to force recalculations and notifications.
            // self.last_screen = .{ .key = t.screens.active_key };
            self.last_screen = ScreenState {
                key: unsafe { (*t).screens.active_key },
                total: None,
                selected: None,
            };
        }

        // Reconcile our screens with the terminal screens. Remove
        // searchers for screens that no longer exist and add searchers
        // for screens that do exist but we don't have yet.
        {
            // Remove screens we have that no longer exist or changed.
            // var it = self.screens.iterator();
            // while (it.next()) |entry| {
            for key_i in 0..Key::COUNT {
                let entry_key = Self::key_from_index(key_i);
                let entry_screen = match self.screens.entries[key_i].as_ref() {
                    Some(ss) => ss.screen,
                    None => continue,
                };
                // const remove = !self.screenIsValid(
                //     &t.screens,
                //     entry.key,
                //     entry.value,
                // );
                // SB: raw `addr_of!`, NOT `&(*t).screens` — see `Search::deinit`.
                let remove: bool = !self.screen_is_valid(
                    unsafe { core::ptr::addr_of!((*t).screens) },
                    entry_key,
                    entry_screen,
                );

                // if (remove) {
                if remove {
                    // entry.value.deinitScreenInvalid();
                    // _ = self.screens.remove(entry.key);
                    // _ = self.screen_generations.remove(entry.key);
                    //
                    // The screen this searcher was built on is gone or was
                    // replaced, so its tracked pins were freed with that
                    // screen's PageList pool and must not be untracked.
                    self.screens.entries[key_i]
                        .as_mut()
                        .unwrap()
                        .deinit_screen_invalid();
                    self.screens.remove(entry_key);
                    self.screen_generations[key_i] = None;
                }
            }
        }
        {
            // Add screens that exist but we don't have yet.
            // var it = t.screens.all.iterator();
            // while (it.next()) |entry| {
            for key_i in 0..Key::COUNT {
                let entry_key = match key_i {
                    0 => ScreenSetKey::Primary,
                    _ => ScreenSetKey::Alternate,
                };
                let entry_value = match unsafe { (*t).screens.all[key_i] } {
                    Some(v) => v,
                    None => continue,
                };
                // if (self.screens.contains(entry.key)) continue;
                if self.screens.contains(entry_key) {
                    continue;
                }
                // const screen_search = ScreenSearch.init(
                //     alloc,
                //     entry.value.*,
                //     self.viewport.needle(),
                // ) catch |err| switch (err) {
                let needle_owned: Vec<u8> = self.viewport.needle().to_vec();
                match ScreenSearch::init(alloc, entry_value, &needle_owned) {
                    // self.screens.put(entry.key, screen_search);
                    // self.screen_generations.put(
                    //     entry.key,
                    //     t.screens.generation(entry.key),
                    // );
                    Ok(ss) => {
                        self.screens.put(entry_key, ss);
                        self.screen_generations[key_i] =
                            Some(unsafe { (*t).screens.generation(entry_key) });
                    }
                    // error.OutOfMemory => {
                    //     // OOM is probably going to sink the entire ship but
                    //     // we can just ignore it and wait on the next
                    //     // reconciliation to try again.
                    //     log.warn(
                    //         "error initializing screen search for key={} err={}",
                    //         .{ entry.key, err },
                    //     );
                    //     continue;
                    // },
                    Err(err) => {
                        eprintln!(
                            "warn(search_thread): error initializing screen search for key={:?} err={:?}",
                            entry_key, err
                        );
                        continue;
                    }
                }
            }
        }

        // See the `search_viewport_dirty` flag on the terminal to know
        // what exactly this is for. But, if this is set, we know the renderer
        // found the viewport/active area dirty, so we should mark it as
        // dirty in our viewport searcher so it forces a re-search.
        // if (t.flags.search_viewport_dirty) {
        if unsafe { (*t).flags.search_viewport_dirty } {
            // t.flags.search_viewport_dirty = false;
            unsafe { (*t).flags.search_viewport_dirty = false };

            // Mark our viewport dirty so it researches the active
            // self.viewport.active_dirty = true;
            self.viewport.active_dirty = Some(true);

            // Reload our active area for our active screen
            // if (self.screens.getPtr(t.screens.active_key)) |screen_search| {
            let active_key = unsafe { (*t).screens.active_key };
            if let Some(screen_search) = self.screens.get_ptr(active_key) {
                // screen_search.reloadActive() catch |err| switch (err) {
                //     error.OutOfMemory => log.warn(
                //         "error reloading active area for screen key={} err={}",
                //         .{ t.screens.active_key, err },
                //     ),
                // };
                if let Err(err) = screen_search.reload_active() {
                    eprintln!(
                        "warn(search_thread): error reloading active area for screen key={:?} err={:?}",
                        active_key, err
                    );
                }
            }
        }

        // Check our viewport for changes.
        // if (self.viewport.update(&t.screens.active.pages)) |updated| {
        //     if (updated) self.stale_viewport_matches = true;
        // } else |err| switch (err) {
        //     error.OutOfMemory => log.warn(
        //         "error updating viewport search err={}",
        //         .{err},
        //     ),
        // }
        let active_pages: *const crate::pagelist::PageList =
            unsafe { &(*(*t).screens.active).pages };
        match self.viewport.update(unsafe { &*active_pages }) {
            Ok(updated) => {
                if updated {
                    self.stale_viewport_matches = true;
                }
            }
            Err(err) => {
                eprintln!(
                    "warn(search_thread): error updating viewport search err={:?}",
                    err
                );
            }
        }

        // Feed data
        // var it = self.screens.iterator();
        // while (it.next()) |entry| {
        for key_i in 0..Key::COUNT {
            let entry_key = match key_i {
                0 => ScreenSetKey::Primary,
                _ => ScreenSetKey::Alternate,
            };
            let entry = match self.screens.entries[key_i].as_mut() {
                Some(e) => e,
                None => continue,
            };
            // if (entry.value.state.needsFeed()) {
            if entry.state.needs_feed() {
                // entry.value.feed() catch |err| switch (err) {
                //     error.OutOfMemory => log.warn(
                //         "error feeding screen search key={} err={}",
                //         .{ entry.key, err },
                //     ),
                // };
                if let Err(err) = entry.feed() {
                    eprintln!(
                        "warn(search_thread): error feeding screen search key={:?} err={:?}",
                        entry_key, err
                    );
                }
            }
        }
    }

    /// Notify about any changes to the search state.
    ///
    /// This doesn't require any locking as it only reads internal state.
    // pub fn notify(
    //     self: *Search,
    //     alloc: Allocator,
    //     cb: EventCallback,
    //     ud: ?*anyopaque,
    // ) void {
    pub fn notify(&mut self, alloc: Allocator, cb: EventCallback, ud: *mut core::ffi::c_void) {
        let _ = alloc;
        // const screen_search = self.screens.get(self.last_screen.key) orelse return;
        let last_key = self.last_screen.key;
        if self.screens.get(last_key).is_none() {
            return;
        }

        // Check our total match data
        // const total = screen_search.matchesLen();
        let total = self.screens.get(last_key).unwrap().matches_len();
        // if (total != self.last_screen.total) {
        if Some(total) != self.last_screen.total {
            // log.debug("notifying total matches={}", .{total});
            eprintln!("debug(search_thread): notifying total matches={}", total);
            // self.last_screen.total = total;
            self.last_screen.total = Some(total);
            // cb(.{ .total_matches = total }, ud);
            cb(Event::TotalMatches(total), ud);
        }

        // Check our viewport matches. If they're stale, we do the
        // viewport search now. We do this as part of notify and not
        // tick because the viewport search is very fast and doesn't
        // require ticked progress or feeds.
        // if (self.stale_viewport_matches) viewport: {
        if self.stale_viewport_matches {
            'viewport: {
                // We always make stale as false. Even if we fail below
                // we require a re-feed to re-search the viewport. The feed
                // process will make it stale again.
                // self.stale_viewport_matches = false;
                self.stale_viewport_matches = false;

                // var arena: ArenaAllocator = .init(alloc);
                // defer arena.deinit();
                // const arena_alloc = arena.allocator();
                // var results: std.ArrayList(FlattenedHighlight) = .empty;
                //
                // PORT NOTE: the reference allocates the cloned highlights in an
                // arena freed on scope exit. The Rust `Flattened` owns its
                // storage and drops, so the result `Vec` plays the arena's role;
                // the callback receives the slice, then the `Vec` (and each
                // `Flattened`) drops at scope exit — identical lifetime.
                let mut results: Vec<FlattenedHighlight> = Vec::new();
                // while (self.viewport.next()) |hl| {
                while let Some(hl) = self.viewport.next() {
                    // const hl_cloned = hl.clone(arena_alloc) catch continue;
                    let hl_cloned = match hl.clone() {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    // results.append(arena_alloc, hl_cloned) catch |err| switch (err) {
                    //     error.OutOfMemory => {
                    //         log.warn(
                    //             "error collecting viewport matches err={}",
                    //             .{err},
                    //         );
                    //         // Reset the viewport so we force an update on the
                    //         // next feed.
                    //         self.viewport.reset();
                    //         break :viewport;
                    //     },
                    // };
                    //
                    // PORT NOTE: `Vec::push` cannot fail-with-OOM in the std
                    // sense (it aborts on alloc failure), so the reference's
                    // error branch (log + `viewport.reset()` + break) is
                    // structurally unreachable; the success path is preserved.
                    results.push(hl_cloned);
                }

                // log.debug("notifying viewport matches len={}", .{results.items.len});
                eprintln!(
                    "debug(search_thread): notifying viewport matches len={}",
                    results.len()
                );
                // cb(.{ .viewport_matches = results.items }, ud);
                cb(Event::ViewportMatches(&results), ud);
                let _ = &mut results;
                break 'viewport;
            }
        }

        // Check our last selected match data.
        // if (screen_search.selected) |m| match: {
        if self.screens.get(last_key).unwrap().selected.is_some() {
            'select_match: {
                // const flattened = screen_search.selectedMatch() orelse break :match;
                let flattened = match self.screens.get(last_key).unwrap().selected_match() {
                    Some(f) => f,
                    None => break 'select_match,
                };
                // const untracked = flattened.untracked();
                let untracked = flattened.untracked();
                // if (self.last_screen.selected) |prev| {
                if let Some(prev) = self.last_screen.selected.as_ref() {
                    // if (prev.idx == m.idx and prev.highlight.eql(untracked)) {
                    //     // Same selection, don't update it.
                    //     break :match;
                    // }
                    //
                    // PORT NOTE: `Untracked::eql` consumes its operands; the Rust
                    // `Untracked` is not `Copy` (its `Pin`s use explicit
                    // `pin_copy()`), so we compare via fresh `Untracked` copies
                    // built from the pins rather than moving `prev.highlight`/
                    // `untracked` (both are needed afterwards).
                    let m_idx = self
                        .screens
                        .get(last_key)
                        .unwrap()
                        .selected
                        .as_ref()
                        .unwrap()
                        .idx;
                    let prev_copy = UntrackedHighlight {
                        start: prev.highlight.start.pin_copy(),
                        end: prev.highlight.end.pin_copy(),
                    };
                    let untracked_copy = UntrackedHighlight {
                        start: untracked.start.pin_copy(),
                        end: untracked.end.pin_copy(),
                    };
                    if prev.idx == m_idx && prev_copy.eql(untracked_copy) {
                        // Same selection, don't update it.
                        break 'select_match;
                    }
                }

                // New selection, notify!
                // self.last_screen.selected = .{
                //     .idx = m.idx,
                //     .highlight = untracked,
                // };
                let m_idx = self
                    .screens
                    .get(last_key)
                    .unwrap()
                    .selected
                    .as_ref()
                    .unwrap()
                    .idx;
                self.last_screen.selected = Some(ScreenStateSelectedMatch {
                    idx: m_idx,
                    highlight: untracked,
                });

                // log.debug("notifying selection updated idx={}", .{m.idx});
                eprintln!(
                    "debug(search_thread): notifying selection updated idx={}",
                    m_idx
                );
                // cb(
                //     .{ .selected_match = .{
                //         .idx = m.idx,
                //         .highlight = flattened,
                //     } },
                //     ud,
                // );
                cb(
                    Event::SelectedMatch(Some(SelectedMatch {
                        idx: m_idx,
                        highlight: flattened,
                    })),
                    ud,
                );
            }
        // } else if (self.last_screen.selected != null) {
        } else if self.last_screen.selected.is_some() {
            // log.debug("notifying selection cleared", .{});
            eprintln!("debug(search_thread): notifying selection cleared");
            // self.last_screen.selected = null;
            self.last_screen.selected = None;
            // cb(
            //     .{ .selected_match = null },
            //     ud,
            // );
            cb(Event::SelectedMatch(None), ud);
        }

        // Send our complete notification if we just completed.
        // if (!self.last_complete and self.isComplete()) {
        if !self.last_complete && self.is_complete() {
            // log.debug("notifying search complete", .{});
            eprintln!("debug(search_thread): notifying search complete");
            // self.last_complete = true;
            self.last_complete = true;
            // cb(.complete, ud);
            cb(Event::Complete, ud);
        }
    }
}

// PORT NOTE: the in-crate `xev` event-loop seam. See the module-header PORT NOTE
// for why this exists (xev is an unported, deferred upward dependency that gates
// `Thread` out of the `lib` artifact in the reference). This reproduces exactly
// the slice of xev's public API the `Thread` uses, backed by `std::thread`
// synchronization. Semantics mirrored: `Async.wait` is a level-triggered wait
// that re-arms (`.rearm`) until disarmed; `Async.notify` sets the async pending
// across threads; `Timer.run` is a one-shot timer; `Loop.run(.once)` blocks for
// at least one event, `Loop.run(.no_wait)` processes ready events without
// blocking; `Loop.stop`/`stopped` mark/observe cancellation.
pub mod xev {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant};

    /// `xev.Loop`/`xev.Async`/`xev.Timer` init never fails in this seam; the
    /// error type mirrors the reference's `!Loop`/`!Async`/`!Timer` so the
    /// `try`/`?` surface matches. (xev's real init can fail on fd exhaustion.)
    #[derive(Clone, Copy, Debug)]
    pub enum Error {
        InitFailed,
    }

    /// Which `Thread` callback a fired completion maps to. (In the reference
    /// these are distinct `*const fn` callbacks bound to each handle.)
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Callback {
        Wakeup,
        Stop,
        Refresh,
    }

    /// `xev.CallbackAction`.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum CallbackAction {
        /// Re-arm the wait so the callback fires again on the next event.
        Rearm,
        /// Disarm; the completion goes idle.
        Disarm,
    }

    /// `xev.Loop.RunMode`.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum RunMode {
        /// Block until at least one event is processed (`.once`).
        Once,
        /// Process all ready events without blocking (`.no_wait`).
        NoWait,
    }

    /// `xev.Completion` state — `.dead`/`.active`. We only distinguish the
    /// reference's observed `.active` from "not active".
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum CompletionState {
        Dead,
        Active,
    }

    /// `xev.Completion`. In the reference this carries the libxev completion
    /// node; here it tracks whether the associated handle wait/timer is armed
    /// and which callback it dispatches.
    pub struct Completion {
        active: bool,
        callback: Option<Callback>,
    }

    impl Completion {
        // `xev.Completion = .{}` — idle/dead default.
        pub fn new() -> Completion {
            Completion {
                active: false,
                callback: None,
            }
        }

        // self.refresh_c.state()
        pub fn state(&self) -> CompletionState {
            if self.active {
                CompletionState::Active
            } else {
                CompletionState::Dead
            }
        }
    }

    impl Default for Completion {
        fn default() -> Completion {
            Completion::new()
        }
    }

    /// Shared loop state, accessible across the producer thread (mailbox
    /// pushers calling `notify`) and the search thread (running the loop).
    struct Shared {
        /// Pending async notifications by callback id.
        wakeup_pending: AtomicBool,
        stop_pending: AtomicBool,
        /// One-shot timer deadline (ns since loop start); 0 = unarmed.
        timer_deadline: AtomicU64,
        timer_armed: AtomicBool,
        /// Loop cancellation flag (`loop.stop()` / `loop.stopped()`).
        stopped: AtomicBool,
        /// Whether the wakeup/stop async waits are armed.
        wakeup_armed: AtomicBool,
        stop_armed: AtomicBool,
        /// Condvar for blocking `run(.once)` until an event is ready.
        mutex: Mutex<()>,
        cond: Condvar,
        /// Monotonic loop epoch for timer math.
        start: Instant,
    }

    impl Shared {
        fn signal(&self) {
            let _g = self.mutex.lock().unwrap();
            self.cond.notify_all();
        }
    }

    /// `xev.Loop`.
    pub struct Loop {
        shared: Arc<Shared>,
    }

    impl Loop {
        // var loop = try xev.Loop.init(.{});
        pub fn init() -> Result<Loop, Error> {
            Ok(Loop {
                shared: Arc::new(Shared {
                    wakeup_pending: AtomicBool::new(false),
                    stop_pending: AtomicBool::new(false),
                    timer_deadline: AtomicU64::new(0),
                    timer_armed: AtomicBool::new(false),
                    stopped: AtomicBool::new(false),
                    wakeup_armed: AtomicBool::new(false),
                    stop_armed: AtomicBool::new(false),
                    mutex: Mutex::new(()),
                    cond: Condvar::new(),
                    start: Instant::now(),
                }),
            })
        }

        // self.loop.deinit();
        pub fn deinit(&mut self) {}

        // self.loop.stop();
        pub fn stop(&mut self) {
            self.shared.stopped.store(true, Ordering::SeqCst);
            self.shared.signal();
        }

        // self.loop.stopped()
        pub fn stopped(&self) -> bool {
            self.shared.stopped.load(Ordering::SeqCst)
        }

        // Run the loop, collecting the set of callbacks whose completions fired.
        // try self.loop.run(mode)
        pub fn run(&mut self, mode: RunMode) -> Result<Vec<Callback>, Error> {
            // For `.once`, block until at least one event is ready.
            if mode == RunMode::Once {
                loop {
                    if self.shared.stopped.load(Ordering::SeqCst) {
                        break;
                    }
                    if self.has_ready_event() {
                        break;
                    }
                    // Block until a notify/timer/stop wakes us, bounded by the
                    // nearest timer deadline so the one-shot timer can fire.
                    let timeout = self.next_timer_timeout();
                    let guard = self.shared.mutex.lock().unwrap();
                    match timeout {
                        Some(d) => {
                            let (g, _) = self.shared.cond.wait_timeout(guard, d).unwrap();
                            drop(g);
                        }
                        None => {
                            let g = self.shared.cond.wait(guard).unwrap();
                            drop(g);
                        }
                    }
                }
            }

            // Collect ready callbacks (both modes process ready events).
            let mut fired: Vec<Callback> = Vec::new();

            // Timer (one-shot): fires if armed and deadline elapsed.
            if self.shared.timer_armed.load(Ordering::SeqCst) {
                let deadline = self.shared.timer_deadline.load(Ordering::SeqCst);
                let elapsed = self.shared.start.elapsed().as_nanos() as u64;
                if elapsed >= deadline {
                    self.shared.timer_armed.store(false, Ordering::SeqCst);
                    fired.push(Callback::Refresh);
                }
            }

            // Stop async: level-triggered while armed + pending.
            if self.shared.stop_armed.load(Ordering::SeqCst)
                && self.shared.stop_pending.swap(false, Ordering::SeqCst)
            {
                fired.push(Callback::Stop);
            }

            // Wakeup async: level-triggered while armed + pending.
            if self.shared.wakeup_armed.load(Ordering::SeqCst)
                && self.shared.wakeup_pending.swap(false, Ordering::SeqCst)
            {
                fired.push(Callback::Wakeup);
            }

            Ok(fired)
        }

        // Apply the callback action to the completion (re-arm / disarm), the
        // analogue of xev consuming the `.rearm`/`.disarm` return value.
        pub fn apply_action(&mut self, c: &mut Completion, action: CallbackAction) {
            match action {
                CallbackAction::Rearm => {
                    // Stays armed; nothing to do (level-triggered wait persists).
                }
                CallbackAction::Disarm => {
                    c.active = false;
                    match c.callback {
                        Some(Callback::Wakeup) => {
                            self.shared.wakeup_armed.store(false, Ordering::SeqCst)
                        }
                        Some(Callback::Stop) => {
                            self.shared.stop_armed.store(false, Ordering::SeqCst)
                        }
                        Some(Callback::Refresh) => {
                            self.shared.timer_armed.store(false, Ordering::SeqCst)
                        }
                        None => {}
                    }
                }
            }
        }

        fn has_ready_event(&self) -> bool {
            if self.shared.wakeup_armed.load(Ordering::SeqCst)
                && self.shared.wakeup_pending.load(Ordering::SeqCst)
            {
                return true;
            }
            if self.shared.stop_armed.load(Ordering::SeqCst)
                && self.shared.stop_pending.load(Ordering::SeqCst)
            {
                return true;
            }
            if self.shared.timer_armed.load(Ordering::SeqCst) {
                let deadline = self.shared.timer_deadline.load(Ordering::SeqCst);
                let elapsed = self.shared.start.elapsed().as_nanos() as u64;
                if elapsed >= deadline {
                    return true;
                }
            }
            false
        }

        fn next_timer_timeout(&self) -> Option<Duration> {
            if !self.shared.timer_armed.load(Ordering::SeqCst) {
                return None;
            }
            let deadline = self.shared.timer_deadline.load(Ordering::SeqCst);
            let elapsed = self.shared.start.elapsed().as_nanos() as u64;
            if elapsed >= deadline {
                Some(Duration::from_millis(0))
            } else {
                Some(Duration::from_nanos(deadline - elapsed))
            }
        }
    }

    /// `xev.Async` — a cross-thread wakeup handle.
    pub struct Async {
        shared: Option<Arc<Shared>>,
        which: Option<Callback>,
        // Set when notify() is called before the handle is associated with a
        // loop (mirrors xev async being notifiable any time post-init).
        pre_pending: std::sync::atomic::AtomicBool,
    }

    impl Async {
        // var wakeup_h = try xev.Async.init();
        pub fn init() -> Result<Async, Error> {
            Ok(Async {
                shared: None,
                which: None,
                pre_pending: std::sync::atomic::AtomicBool::new(false),
            })
        }

        // self.wakeup.deinit();
        pub fn deinit(&mut self) {}

        // self.wakeup.wait(&self.loop, &self.wakeup_c, Thread, self, wakeupCallback);
        pub fn wait(&mut self, loop_: &Loop, c: &mut Completion, which: Callback) {
            self.shared = Some(loop_.shared.clone());
            self.which = Some(which);
            c.active = true;
            c.callback = Some(which);
            let shared = self.shared.as_ref().unwrap();
            match which {
                Callback::Wakeup => shared.wakeup_armed.store(true, Ordering::SeqCst),
                Callback::Stop => shared.stop_armed.store(true, Ordering::SeqCst),
                Callback::Refresh => {}
            }
            // Carry forward any pre-association notify.
            if self.pre_pending.swap(false, Ordering::SeqCst) {
                self.set_pending(which);
            }
        }

        // try self.wakeup.notify();
        pub fn notify(&self) -> Result<(), Error> {
            match (&self.shared, self.which) {
                (Some(shared), Some(which)) => {
                    Self::set_pending_on(shared, which);
                    shared.signal();
                    Ok(())
                }
                _ => {
                    self.pre_pending.store(true, Ordering::SeqCst);
                    Ok(())
                }
            }
        }

        fn set_pending(&self, which: Callback) {
            if let Some(shared) = &self.shared {
                Self::set_pending_on(shared, which);
                shared.signal();
            }
        }

        fn set_pending_on(shared: &Arc<Shared>, which: Callback) {
            match which {
                Callback::Wakeup => shared.wakeup_pending.store(true, Ordering::SeqCst),
                Callback::Stop => shared.stop_pending.store(true, Ordering::SeqCst),
                Callback::Refresh => {}
            }
        }

        /// A clonable notifier handle so producers on other threads can wake the
        /// loop. (The reference shares the `*xev.Async` directly; in Rust the
        /// `Thread` is owned by the search thread, so callers that need to
        /// notify across threads take a `Notifier` before spawning.)
        pub fn notifier(&self) -> Notifier {
            Notifier {
                shared: self.shared.clone(),
                which: self.which,
            }
        }
    }

    /// Cross-thread `Async.notify` handle.
    #[derive(Clone)]
    pub struct Notifier {
        shared: Option<Arc<Shared>>,
        which: Option<Callback>,
    }

    impl Notifier {
        pub fn notify(&self) -> Result<(), Error> {
            match (&self.shared, self.which) {
                (Some(shared), Some(which)) => {
                    Async::set_pending_on(shared, which);
                    shared.signal();
                    Ok(())
                }
                _ => Ok(()),
            }
        }
    }

    /// `xev.Timer` — a one-shot timer.
    pub struct Timer {
        shared: Option<Arc<Shared>>,
    }

    impl Timer {
        // var refresh_h = try xev.Timer.init();
        pub fn init() -> Result<Timer, Error> {
            Ok(Timer { shared: None })
        }

        // self.refresh.deinit();
        pub fn deinit(&mut self) {}

        // self.refresh.run(&self.loop, &self.refresh_c, REFRESH_INTERVAL, Thread, self, refreshCallback);
        pub fn run(&mut self, loop_: &Loop, c: &mut Completion, next_ms: u64, which: Callback) {
            self.shared = Some(loop_.shared.clone());
            let shared = self.shared.as_ref().unwrap();
            let deadline = shared.start.elapsed().as_nanos() as u64 + next_ms * 1_000_000;
            shared.timer_deadline.store(deadline, Ordering::SeqCst);
            shared.timer_armed.store(true, Ordering::SeqCst);
            c.active = true;
            c.callback = Some(which);
            shared.signal();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DynamicPalette, DynamicRGB};
    use crate::kitty;
    use crate::modes as modespkg;
    use crate::point::Point;
    use crate::terminal::{Colors, Options as TerminalOptions, Terminal};

    // PORT NOTE: see search/screen.rs — supplies the Zig `Options` field defaults
    // the reference relies on implicitly for `Terminal.init`.
    fn terminal_opts(cols: u16, rows: u16, max_scrollback_bytes: usize) -> TerminalOptions {
        TerminalOptions {
            cols,
            rows,
            max_scrollback_bytes: Some(max_scrollback_bytes),
            max_scrollback_lines: None,
            colors: Colors {
                background: DynamicRGB::UNSET,
                foreground: DynamicRGB::UNSET,
                cursor: DynamicRGB::UNSET,
                palette: DynamicPalette::default_palette(),
            },
            default_modes: modespkg::ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // const TestUserData = struct {
    //     const Self = @This();
    //     reset: std.Thread.ResetEvent = .{},
    //     total: usize = 0,
    //     selected: ?Event.SelectedMatch = null,
    //     viewport: []FlattenedHighlight = &.{},
    //     ...
    // };
    //
    // PORT NOTE: the reference `TestUserData` holds a `std.Thread.ResetEvent`
    // (waited on by the main thread, set by the callback when `.complete` is
    // observed) and accumulates the latest `total`/`selected`/`viewport`. The
    // callback runs on the SEARCH thread and `userdata` is `*anyopaque`, so we
    // mirror it with a `*mut TestUserData` carried through the `c_void` userdata.
    // The `reset` event is a `Mutex<bool>` + `Condvar` (the std analogue of
    // `ResetEvent` with `timedWait`). `selected` stores the idx (the reference's
    // `Event.SelectedMatch` carries an owning `FlattenedHighlight` we don't need
    // for the assertions); `viewport` clones each highlight, owned by the struct.
    struct TestUserData {
        reset_done: Mutex<bool>,
        reset_cond: std::sync::Condvar,
        total: std::sync::atomic::AtomicUsize,
        selected: Mutex<Option<usize>>,
        viewport: Mutex<Vec<FlattenedHighlight>>,
    }

    impl TestUserData {
        fn new() -> TestUserData {
            TestUserData {
                reset_done: Mutex::new(false),
                reset_cond: std::sync::Condvar::new(),
                total: std::sync::atomic::AtomicUsize::new(0),
                selected: Mutex::new(None),
                viewport: Mutex::new(Vec::new()),
            }
        }

        // fn deinit(self: *Self) void {
        //     for (self.viewport) |*hl| hl.deinit(testing.allocator);
        //     testing.allocator.free(self.viewport);
        // }
        //
        // PORT NOTE: the owned `Vec<FlattenedHighlight>` drops here (each
        // `Flattened` frees on drop), mirroring the reference's free.

        // fn callback(event: Event, userdata: ?*anyopaque) void {
        fn callback(event: Event, userdata: *mut core::ffi::c_void) {
            // const ud: *Self = @ptrCast(@alignCast(userdata.?));
            let ud: &TestUserData = unsafe { &*(userdata as *const TestUserData) };
            // switch (event) {
            match event {
                // .quit => {},
                Event::Quit => {}
                // .complete => ud.reset.set(),
                Event::Complete => {
                    let mut done = ud.reset_done.lock().unwrap();
                    *done = true;
                    ud.reset_cond.notify_all();
                }
                // .total_matches => |v| ud.total = v,
                Event::TotalMatches(v) => {
                    ud.total.store(v, std::sync::atomic::Ordering::SeqCst);
                }
                // .selected_match => |v| ud.selected = v,
                Event::SelectedMatch(v) => {
                    *ud.selected.lock().unwrap() = v.map(|m| m.idx);
                }
                // .viewport_matches => |v| {
                Event::ViewportMatches(v) => {
                    // for (ud.viewport) |*hl| hl.deinit(testing.allocator);
                    // testing.allocator.free(ud.viewport);
                    // ud.viewport = testing.allocator.alloc(FlattenedHighlight, v.len) catch unreachable;
                    // for (ud.viewport, v) |*dst, src| {
                    //     dst.* = src.clone(testing.allocator) catch unreachable;
                    // }
                    let mut vp = ud.viewport.lock().unwrap();
                    *vp = Vec::with_capacity(v.len());
                    for src in v {
                        vp.push(src.clone().unwrap());
                    }
                }
            }
        }
    }

    // test {
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (.port/phase-2/MIRI_EXCEPTIONS.md): rustc/Miri interpreter ICE (operand.rs:160, upstream bug), not a soundness signal; covered natively + by ASan"
    )]
    #[test]
    fn search_thread() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var mutex: std.Thread.Mutex = .{};
        let mutex = rustty_zigstd::thread::Mutex::new();
        // var t: Terminal = try .init(alloc, .{ .cols = 20, .rows = 2 });
        // defer t.deinit(alloc);
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(20, 2, 10_000)).unwrap();

        // var stream = t.vtStream();
        // defer stream.deinit();
        // stream.nextSlice("Hello, world");
        {
            let mut stream = t.vt_stream();
            stream.next_slice(b"Hello, world");
        }

        // var ud: TestUserData = .{};
        // defer ud.deinit();
        let ud: Box<TestUserData> = Box::new(TestUserData::new());
        let ud_ptr: *const TestUserData = &*ud;

        // var thread: Thread = try .init(alloc, .{
        //     .mutex = &mutex,
        //     .terminal = &t,
        //     .event_cb = &TestUserData.callback,
        //     .event_userdata = &ud,
        // });
        // defer thread.deinit();
        let mut thread: Thread = Thread::init(
            alloc,
            Options {
                mutex: &mutex as *const rustty_zigstd::thread::Mutex,
                terminal: &mut t as *mut Terminal,
                event_cb: Some(TestUserData::callback),
                event_userdata: ud_ptr as *mut core::ffi::c_void,
            },
        )
        .unwrap();

        // PORT NOTE: the reference spawns the OS thread with `&thread` (the search
        // thread owns the `*Thread`) and the main thread keeps pushing to
        // `thread.mailbox` + notifying `thread.wakeup`. To do that safely across
        // the std-thread boundary, we take the cross-thread handles (a `Notifier`
        // for `wakeup`, a `Notifier` for `stop`, and a raw `*mut Mailbox`) BEFORE
        // spawning, then move the `Thread` into the spawned closure. These handles
        // are exactly the producer surface the reference accesses from the main
        // thread (`thread.mailbox.push`, `thread.wakeup.notify`,
        // `thread.stop.notify`).
        //
        // We must associate the async waits with the loop before taking the
        // notifiers, so they carry the loop's shared state. The waits are armed at
        // the top of `thread_main_`; to obtain valid notifiers here we arm them
        // up-front (idempotent with the re-arm inside `thread_main_`).
        thread
            .wakeup
            .wait(&thread.loop_, &mut thread.wakeup_c, xev::Callback::Wakeup);
        thread
            .stop
            .wait(&thread.loop_, &mut thread.stop_c, xev::Callback::Stop);
        let wakeup_notifier = thread.wakeup.notifier();
        let stop_notifier = thread.stop.notifier();

        // PORT NOTE: the reference spawns the OS thread with `&thread` (a borrow,
        // not a move) and the main thread keeps using `thread.mailbox`/
        // `thread.wakeup`. The faithful Rust analogue is `std::thread::scope`,
        // which lets the spawned thread BORROW the stack-local `thread` without
        // moving it — so the mailbox's heap `Box` is never retagged/invalidated
        // (Stacked-Borrows clean under Miri). We pass a `*mut Thread` (the stable
        // address of the stack local) through a `Send` wrapper since `Thread`
        // carries raw `*mut`/`*const` fields and is thus `!Send`. The mailbox
        // pointer is the stable heap node address taken once; `push` takes `&self`
        // and is internally synchronized.
        let mailbox_ptr: *const Mailbox = core::ptr::addr_of!(*thread.mailbox);
        let thread_ptr: *mut Thread = &mut thread as *mut Thread;

        // SAFETY: the spawned thread is joined (scope end) before `t`/`ud`/`mutex`
        // and `thread` itself go out of scope, so the raw pointers stay valid for
        // the whole spawn lifetime (mirroring the reference's `&t`/`&ud`/`&thread`
        // shared with the search thread, joined before deinit).
        struct SendPtr(*mut Thread);
        unsafe impl Send for SendPtr {}
        struct SendMailbox(*const Mailbox);
        unsafe impl Send for SendMailbox {}

        let send_thread = SendPtr(thread_ptr);
        let send_mailbox = SendMailbox(mailbox_ptr);

        std::thread::scope(|scope| {
            // var os_thread = try std.Thread.spawn(.{}, threadMain, .{&thread});
            let os_thread = scope.spawn(move || {
                let st = send_thread;
                unsafe { (*st.0).thread_main() };
            });

            // Start our search
            // _ = thread.mailbox.push(
            //     .{ .change_needle = try .init(
            //         alloc,
            //         @as([]const u8, "world"),
            //     ) },
            //     .forever,
            // );
            let _ = unsafe {
                (*send_mailbox.0).push(
                    Message::ChangeNeedle(WriteReq::init(b"world")),
                    rustty_datastruct::blocking_queue::Timeout::Forever,
                )
            };
            // try thread.wakeup.notify();
            wakeup_notifier.notify().unwrap();

            // Wait for completion
            // try ud.reset.timedWait(100 * std.time.ns_per_ms);
            {
                let done = ud.reset_done.lock().unwrap();
                let (done, timeout) = ud
                    .reset_cond
                    .wait_timeout_while(done, std::time::Duration::from_millis(100), |d| !*d)
                    .unwrap();
                assert!(*done, "timed out waiting for search completion");
                let _ = timeout;
            }

            // Stop the thread
            // try thread.stop.notify();
            stop_notifier.notify().unwrap();
            // os_thread.join();
            os_thread.join().unwrap();
        });

        // 1 total matches
        // try testing.expectEqual(1, ud.total);
        assert_eq!(1, ud.total.load(std::sync::atomic::Ordering::SeqCst));
        // try testing.expectEqual(1, ud.viewport.len);
        let vp = ud.viewport.lock().unwrap();
        assert_eq!(1, vp.len());
        {
            // const sel = ud.viewport[0].untracked();
            let sel = vp[0].untracked();
            // try testing.expectEqual(point.Point{ .screen = .{ .x = 7, .y = 0 } },
            //     t.screens.active.pages.pointFromPin(.screen, sel.start).?);
            assert_eq!(
                Point::Screen(point::Coordinate { x: 7, y: 0 }),
                unsafe {
                    (*t.screens.active)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.start)
                }
                .unwrap()
            );
            // try testing.expectEqual(point.Point{ .screen = .{ .x = 11, .y = 0 } },
            //     t.screens.active.pages.pointFromPin(.screen, sel.end).?);
            assert_eq!(
                Point::Screen(point::Coordinate { x: 11, y: 0 }),
                unsafe {
                    (*t.screens.active)
                        .pages
                        .point_from_pin(point::Tag::Screen, sel.end)
                }
                .unwrap()
            );
        }

        // defer thread.deinit(); defer t.deinit(alloc); defer ud.deinit();
        drop(vp);
        thread.deinit();
        t.deinit(alloc);
        drop(ud);
    }

    /// PORTS: oracle 73ac36fa5
    /// `test "select after active screen removal"`
    #[test]
    fn select_after_active_screen_removal() {
        // const alloc = testing.allocator;
        let alloc = Allocator;
        // var mutex: std.Thread.Mutex = .{};
        let mutex = rustty_zigstd::thread::Mutex::new();
        // var t: Terminal = try .init(alloc, .{ .cols = 20, .rows = 2 });
        // defer t.deinit(alloc);
        let mut t: Terminal = Terminal::init(alloc, terminal_opts(20, 2, 10_000)).unwrap();
        // SB: the searcher and the `Thread` hold a raw `*mut Terminal` ACROSS the
        // screen mutations below. Every mutation must therefore go through THIS
        // pointer's own tag; a `&mut t` reborrow (`t.switch_screen(..)`) is a
        // Unique retag that invalidates the stored alias, which Miri proves is
        // Stacked-Borrows UB (memory: shaper-hook-sb-clean-interleave).
        let t_ptr: *mut Terminal = core::ptr::addr_of_mut!(t);

        // _ = try t.switchScreen(.alternate);
        unsafe { (*t_ptr).switch_screen(ScreenSetKey::Alternate) }.unwrap();

        // var search: Search = try .init(alloc, "needle");
        // search.feed(alloc, &t);
        let mut search: Search = Search::init(alloc, b"needle").unwrap();
        search.feed(alloc, t_ptr);
        // try testing.expectEqual(ScreenSet.Key.alternate, search.last_screen.key);
        assert_eq!(search.last_screen.key, ScreenSetKey::Alternate);
        // try testing.expect(search.screens.contains(.alternate));
        assert!(search.screens.contains(ScreenSetKey::Alternate));

        // var thread: Thread = undefined;
        // thread.search = search;
        // thread.opts = .{ .mutex = &mutex, .terminal = &t };
        // defer if (thread.search) |*active| active.deinit(&t);
        //
        // PORT NOTE: the reference leaves the rest of `Thread` `undefined` because
        // `select` only touches `search`/`opts`. Rust has no partially-initialized
        // struct, so we build a real `Thread` with `Thread::init` (which only adds
        // an idle xev loop + mailbox) and install the pre-fed `Search`. The extra
        // `thread.deinit()` at the end also exercises 73ac36fa5's
        // `Search::deinit(&t)` screen-validity split, which the reference's
        // `defer active.deinit(&t)` covers separately.
        let mut thread: Thread = Thread::init(
            alloc,
            Options {
                mutex: &mutex as *const rustty_zigstd::thread::Mutex,
                terminal: t_ptr,
                event_cb: None,
                event_userdata: core::ptr::null_mut(),
            },
        )
        .unwrap();
        thread.search = Some(search);

        // _ = try t.switchScreen(.primary);
        // t.screens.remove(alloc, .alternate);
        unsafe { (*t_ptr).switch_screen(ScreenSetKey::Primary) }.unwrap();
        unsafe { (*t_ptr).screens.remove(alloc, ScreenSetKey::Alternate) };

        // try thread.select(.next);
        thread.select(super::super::screen::Select::Next).unwrap();
        // try testing.expectEqual(
        //     ScreenSet.Key.primary,
        //     thread.search.?.last_screen.key,
        // );
        assert_eq!(
            thread.search.as_ref().unwrap().last_screen.key,
            ScreenSetKey::Primary
        );
        // try testing.expect(!thread.search.?.screens.contains(.alternate));
        assert!(!thread
            .search
            .as_ref()
            .unwrap()
            .screens
            .contains(ScreenSetKey::Alternate));

        thread.deinit();
        unsafe { (*t_ptr).deinit(alloc) };
    }
}
