//! PORTS: ghostty src/termio/Exec.zig — the `test{}` blocks and the port's KATs,
//! split by topic under `tests/`; the shared test instrumentation and harness
//! (`TestMailbox`, `TestTermio`) stay here.

use super::*;
use rustty_os::passwd::Entry as PasswdEntry;

// Test-only instrumentation of the `threadEnter` read-thread spawn error
// arm (S211-F5 / UD102). Thread-local, so it only affects the test thread
// that sets it (threadEnter and its error arm run on the calling thread).
thread_local! {
    /// Calls of `drain_pty_master_once` on this thread.
    pub(super) static DRAIN_CALLS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
    /// Calls of `Subprocess::stop` on this thread.
    pub(super) static STOP_CALLS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
    /// The value of `STOP_CALLS` when `drain_pty_master_once` last ran on
    /// this thread (`None` if it has not run since the last reset).
    pub(super) static STOPS_BEFORE_DRAIN: std::cell::Cell<Option<usize>> =
        const { std::cell::Cell::new(None) };
    /// Make `threadEnter`'s read-thread spawn fail on this thread.
    pub(super) static FAIL_READ_THREAD_SPAWN: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
    /// The `threadEnter` steps that failed on this thread, innermost
    /// first, with their errors (`note_thread_enter_failure!`, N354).
    pub(super) static THREAD_ENTER_FAILURE: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Make `threadMainPosix`'s gather-thread spawn fail on this thread
    /// (S211-F6).
    pub(super) static FAIL_GATHER_SPAWN: std::cell::Cell<bool> =
        const { std::cell::Cell::new(false) };
}

/// Test-only hook run by `gatherMainPosix` right before its outer
/// `poll(-1)`, for the one fd it was installed for (the gather stage runs
/// on its own thread, so a thread-local cannot reach it; keying on the fd
/// keeps concurrent tests out of it). UD19.
#[cfg(not(windows))]
type OuterPollHook = (std::os::fd::RawFd, Box<dyn FnMut() + Send>);
#[cfg(not(windows))]
static BEFORE_OUTER_POLL: std::sync::Mutex<Option<OuterPollHook>> = std::sync::Mutex::new(None);

#[cfg(not(windows))]
pub(super) fn before_outer_poll(fd: std::os::fd::RawFd) {
    let mut hook = BEFORE_OUTER_POLL.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((hook_fd, f)) = hook.as_mut() {
        if *hook_fd == fd {
            f();
        }
    }
}

// Serialize the pty/fork-spawning KATs so two do not `fork()` concurrently in
// the same test binary (a `fork` snapshots every open fd; concurrent
// pty-spawning tests can leak each other's master fd into a child — the
// parallel-cargo-test isolation lesson). Every KAT that spawns a real process
// takes this lock for its duration. (T8: hoisted to the crate root so
// `command::tests`' fork KATs share it — see the doc there.)
#[cfg(not(windows))]
use crate::FORK_KAT_SERIAL;

// A trivial `AppMailbox` test double: it accepts every push (returns a
// non-zero queue size) and records nothing. Enough for the exit / termios /
// write callbacks, which only `push(...)` to it.
//
// The one thing it does keep is each `child_exited` exit code, in a
// thread-local (the exit callback runs on the thread that drives the loop,
// which in these KATs is the test thread), so the process-exit KATs can see
// which arm of `processExit` produced it.
#[cfg(any(target_os = "macos", windows))]
struct TestMailbox;

#[cfg(any(target_os = "macos", windows))]
thread_local! {
    static CHILD_EXITED_CODES: std::cell::RefCell<Vec<u32>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(any(target_os = "macos", windows))]
impl rustty_apprt_api::surface::AppMailbox for TestMailbox {
    type Surface = ();

    #[allow(clippy::result_large_err)]
    fn push_or_return(
        &self,
        msg: rustty_apprt_api::surface::Message<'static>,
        _timeout: rustty_datastruct::blocking_queue::Timeout,
    ) -> Result<rustty_datastruct::blocking_queue::Size, rustty_apprt_api::surface::Message<'static>>
    {
        if let rustty_apprt_api::surface::Message::ChildExited(info) = &msg {
            CHILD_EXITED_CODES.with(|v| v.borrow_mut().push(info.exit_code));
        }
        // S211-F2: a bell rung by the pty output is pushed from the
        // io-reader thread; record that thread's crash `thread_state`.
        if let rustty_apprt_api::surface::Message::RingBell = &msg {
            let state = rustty_crash::sentry::threadState();
            BELL_THREAD_STATES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(state.map(|(ty, surface)| {
                    (
                        ty,
                        std::ptr::addr_eq(surface, &crate::TEST_CRASH_SURFACE as *const _),
                    )
                }));
        }
        // Non-zero => "accepted" (matches a live queue's push return).
        Ok(1)
    }

    fn crash_surface(&self) -> *const dyn rustty_crash::sentry::ThreadStateSurface {
        &crate::TEST_CRASH_SURFACE
    }
}

/// The `(type, is TEST_CRASH_SURFACE)` crash `thread_state` of the thread
/// that pushed each `RingBell` through `TestMailbox` (`None` = no state).
#[cfg(any(target_os = "macos", windows))]
#[allow(clippy::type_complexity)]
static BELL_THREAD_STATES: std::sync::Mutex<Vec<Option<(rustty_crash::sentry::Type, bool)>>> =
    std::sync::Mutex::new(Vec::new());

// A heap-pinned `Termio<TestMailbox>` test harness (blueprint §5 shape,
// constructed by hand because `Termio::init` is deferred to B4). It carries a
// live renderer `State` (guard-less zigstd mutex + a real owned `Terminal`), a
// real `StreamHandler` whose `terminal`/`size`/`termio_mailbox` self-pointers
// are re-seated at the FINAL heap address (the [ADV-FIX F1] discipline), real
// termio + renderer mailboxes, and a real renderer-wakeup `Async`. The backend
// `Exec` is a required field that the read path never touches (no subprocess is
// started through it). Shared by the B2 write-path + B3 read-path KATs — both
// now drive the REAL `Termio::processOutput` (B3) so `io` must be a valid,
// pinned `Termio`, not a placeholder.
#[cfg(any(target_os = "macos", windows))]
#[cfg_attr(windows, allow(dead_code))] // (the byte counters serve macOS KATs)
struct TestTermio {
    io: *mut crate::Termio<TestMailbox>,
    state: *mut rustty_renderer::state::State,
    mutex: *mut rustty_zigstd::thread::Mutex,
    renderer_mailbox: *mut rustty_renderer::thread::Mailbox,
    renderer_wakeup: xev::Async,
    /// The `PROCESSED_BYTES` value when this harness was built, so
    /// `processed_bytes` is per-harness even though the counter is global
    /// (the KATs are serialized by `FORK_KAT_SERIAL`).
    processed_base: usize,
    /// The `PROCESSED_CALLS` value when this harness was built.
    calls_base: usize,
}

#[cfg(any(target_os = "macos", windows))]
#[cfg_attr(windows, allow(dead_code))]
impl TestTermio {
    fn build() -> TestTermio {
        use allocator_api2::alloc::Global;

        // The shared render State (heap-pinned; `terminal` re-seated after the
        // Termio is pinned). `mutex` is the guard-less zigstd lock.
        let mutex: *mut rustty_zigstd::thread::Mutex =
            Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
        let state: *mut rustty_renderer::state::State =
            Box::into_raw(Box::new(rustty_renderer::state::State {
                mutex,
                terminal: std::ptr::null_mut(),
                inspector: None,
                inspector_ptr: None,
                preedit: None,
                mouse: rustty_renderer::state::Mouse::default(),
                // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
                demand: Default::default(),
                handoff_gen: Default::default(),
                handoff_park: Default::default(),
            }));

        // A throwaway renderer mailbox (pushes queue; notify fails-safe) and a
        // real renderer-wakeup Async (Copy; shared by the handler + Termio).
        let renderer_mailbox: *mut rustty_renderer::thread::Mailbox =
            Box::into_raw(rustty_renderer::thread::Mailbox::create());
        let renderer_wakeup = xev::Async::init().expect("Async::init");

        // The owned Terminal (80x24).
        let terminal = Terminal::init(
            Global.into(),
            rustty_vt::terminal::Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                colors: rustty_vt::terminal::Colors::default(),
                default_modes: rustty_vt::modes::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                default_xt_checksum: Default::default(),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .expect("Terminal::init");

        // DerivedConfig from a default Config (only the handler's cursor/OSC
        // fields are read from it; `config` itself is untouched by the read
        // path).
        let full_config = configpkg::Config::default();
        let derived = crate::termio_impl::DerivedConfig::init(&full_config);

        // Backend (Exec; a required Termio field, never used by the read path —
        // no subprocess is started through it).
        let exec_cfg = Config {
            command: Some(configpkg::command::Command::Direct(vec![
                b"/bin/cat".to_vec()
            ])),
            env: EnvMap::new(),
            env_override: configpkg::RepeatableStringMap::default(),
            shell_integration: configpkg::value_types::ShellIntegration::Detect,
            shell_integration_features: configpkg::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: RtPreExecInfo::default(),
            rt_post_fork_info: RtPostForkInfo::default(),
        };
        let backend =
            crate::backend::Backend::Exec(Exec::init(Global, exec_cfg).expect("Exec::init"));

        // The termio mailbox (SPSC; its own wakeup Async).
        let mailbox = crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC");

        // Size (a required field, unread by the read path).
        let size = rustty_renderer::size::Size {
            screen: rustty_renderer::size::ScreenSize {
                width: 800,
                height: 480,
            },
            cell: rustty_renderer::size::CellSize {
                width: 10,
                height: 20,
            },
            padding: rustty_renderer::size::Padding::default(),
        };

        // The StreamHandler. Its `terminal`/`size`/`termio_mailbox` are raw
        // pointers back into the owning Termio — null here, re-seated at the
        // FINAL address after the Termio is heap-pinned (the F1 discipline).
        let handler = crate::stream_handler::StreamHandler::<TestMailbox> {
            alloc: Global,
            size: std::ptr::null_mut(),
            terminal: std::ptr::null_mut(),
            termio_mailbox: std::ptr::null_mut(),
            surface_mailbox: TestMailbox,
            renderer_state: state,
            renderer_mailbox,
            // (`clone`: `Copy` everywhere but the Windows Async.)
            #[allow(clippy::clone_on_copy)]
            renderer_wakeup: renderer_wakeup.clone(),
            enquiry_response: derived.enquiry_response.clone(),
            osc_color_report_format: derived.osc_color_report_format,
            clipboard_write: derived.clipboard_write,
            clipboard_write_limit: derived.clipboard_write_limit,
            // (reference field defaults: `kitty_clipboard_grants = .{}`,
            // `kitty_clipboard_write = null`.)
            kitty_clipboard_grants: rustty_vt::kitty::clipboard::Grants::default(),
            kitty_clipboard_write: None,
            apc: rustty_vt::apc::Handler::default(),
            dcs: rustty_vt::dcs::Handler::default(),
            tmux_viewer: None,
            termio_messaged: false,
            seen_title: false,
        };
        let terminal_stream =
            crate::stream_handler::Stream::init(rustty_vt::stream::Options::with_handler(handler));

        // Assemble + heap-pin the Termio.
        let io: *mut crate::Termio<TestMailbox> = Box::into_raw(Box::new(crate::Termio {
            alloc: Global,
            backend,
            config: derived,
            terminal,
            renderer_state: state,
            #[allow(clippy::clone_on_copy)]
            renderer_wakeup: renderer_wakeup.clone(),
            renderer_mailbox,
            surface_mailbox: TestMailbox,
            size,
            mailbox,
            terminal_stream,
            last_cursor_reset: None,
            thread_enter_state: None,
        }));

        // Re-seat the self-referential pointers at the final address (F1).
        unsafe {
            (*io).terminal_stream.handler.terminal = std::ptr::addr_of_mut!((*io).terminal);
            (*io).terminal_stream.handler.size = std::ptr::addr_of_mut!((*io).size);
            (*io).terminal_stream.handler.termio_mailbox = std::ptr::addr_of_mut!((*io).mailbox);
            (*state).terminal = std::ptr::addr_of_mut!((*io).terminal);
        }

        TestTermio {
            io,
            state,
            mutex,
            renderer_mailbox,
            renderer_wakeup,
            processed_base: crate::PROCESSED_BYTES.load(core::sync::atomic::Ordering::Relaxed),
            calls_base: crate::PROCESSED_CALLS.load(core::sync::atomic::Ordering::Relaxed),
        }
    }

    // The active Screen text, read under the renderer mutex (matching the read
    // thread's `processOutput` lock discipline).
    /// The running tally of bytes handed to `Termio::processOutput` since
    /// this harness was built. See `crate::PROCESSED_BYTES`.
    fn processed_bytes(&self) -> usize {
        crate::PROCESSED_BYTES.load(core::sync::atomic::Ordering::Relaxed) - self.processed_base
    }

    /// The number of `processOutput` CALLS since this harness was built —
    /// i.e. batches delivered by the read pipeline. See
    /// `crate::PROCESSED_CALLS`.
    fn processed_calls(&self) -> usize {
        crate::PROCESSED_CALLS.load(core::sync::atomic::Ordering::Relaxed) - self.calls_base
    }

    fn screen_dump(&self) -> Vec<u8> {
        use allocator_api2::alloc::Global;
        unsafe { (*self.mutex).lock() };
        let dump = unsafe {
            (*(*self.io).terminal.screens.active).dump_string_alloc(
                Global.into(),
                rustty_vt::point::Point::Active(rustty_vt::point::Coordinate::default()),
            )
        };
        unsafe { (*self.mutex).unlock() };
        dump.unwrap_or_default()
    }

    // Deinit everything. MUST be called only after every thread touching `io`
    // is joined (nothing dereferences `io` anymore): moving the Termio out of
    // the box invalidates its self-pointers, which is sound only post-join.
    fn teardown(self) {
        let termio = unsafe { *Box::from_raw(self.io) };
        termio.deinit();
        unsafe {
            drop(Box::from_raw(self.state));
            drop(Box::from_raw(self.mutex));
            drop(Box::from_raw(self.renderer_mailbox));
        }
        let mut w = self.renderer_wakeup;
        w.deinit();
    }
}

#[cfg(not(windows))]
mod child_raw_errno;
mod exec_command;
mod read_thread;
mod subprocess;
mod thread_enter;
