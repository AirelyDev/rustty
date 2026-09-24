//! LEDGER S212-F1 and S212-F2: the io thread's entry point, `Thread::threadMain`,
//! run for real on its own thread over a real `Termio`, as the surface runs it.
//!
//! - S212-F1: when `threadMain_` fails, `threadMain` paints a failure screen
//!   chosen by the error (`Thread.zig:148-216`). A configured `input` path that
//!   does not exist fails `Termio.threadEnter` with `error.InputNotFound` before
//!   anything is spawned, and must paint the reference's `input` message, not the
//!   general "error starting IO thread" one.
//! - S212-F2: `threadMain_` tags the io thread's crash metadata as an `.io`
//!   thread of `io.surface_mailbox.surface` (`Thread.zig:234-240`). The child's
//!   exit is reported from the io thread (`processExit` pushes `child_exited`),
//!   and the test mailbox records that thread's crash `thread_state`.
//!
//! An integration test (its own process) for the reasons `live_shell.rs` gives:
//! it forks a real shell and installs no global state the unit suite shares.
//! The two tests serialize on `SERIAL` (each forks or holds pty fds).

// Real fork/exec + openpty + kqueue/epoll; unix only, like `live_shell.rs`.
#![cfg(unix)]
#![allow(non_snake_case)]

use std::sync::Mutex;
use std::time::{Duration, Instant};

use allocator_api2::alloc::Global;

use rustty_termio::exec::{Config, Exec};
use rustty_termio::termio_impl::DerivedConfig;
use rustty_termio::xev_seam as xev;
use rustty_termio::{Backend, Mailbox, Options, Termio, Thread};

static SERIAL: Mutex<()> = Mutex::new(());

/// The surface the test mailbox reports from `crash_surface`.
struct TestSurface;

impl rustty_crash::sentry::ThreadStateSurface for TestSurface {
    fn screen_size(&self) -> (u32, u32) {
        (0, 0)
    }
    fn grid_size(&self) -> (u32, u32) {
        (0, 0)
    }
    fn cell_size(&self) -> (u32, u32) {
        (0, 0)
    }
}

static TEST_SURFACE: TestSurface = TestSurface;

/// For each `child_exited` pushed: the pushing thread's crash `thread_state`,
/// as `(type, surface is TEST_SURFACE)`, or `None`.
#[allow(clippy::type_complexity)]
static CHILD_EXITED_STATES: Mutex<Vec<Option<(rustty_crash::sentry::Type, bool)>>> =
    Mutex::new(Vec::new());

#[derive(Clone, Copy)]
struct TestMailbox;

impl rustty_apprt_api::surface::AppMailbox for TestMailbox {
    type Surface = ();

    #[allow(clippy::result_large_err)]
    fn push_or_return(
        &self,
        msg: rustty_apprt_api::surface::Message<'static>,
        _timeout: rustty_datastruct::blocking_queue::Timeout,
    ) -> Result<rustty_datastruct::blocking_queue::Size, rustty_apprt_api::surface::Message<'static>>
    {
        if let rustty_apprt_api::surface::Message::ChildExited(_) = &msg {
            let state = rustty_crash::sentry::threadState();
            CHILD_EXITED_STATES
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(state.map(|(ty, surface)| {
                    (
                        ty,
                        std::ptr::addr_eq(surface, &TEST_SURFACE as *const TestSurface),
                    )
                }));
        }
        Ok(1)
    }

    fn crash_surface(&self) -> *const dyn rustty_crash::sentry::ThreadStateSurface {
        &TEST_SURFACE
    }
}

/// A heap-pinned `Termio<TestMailbox>` built with the public `Termio::init`,
/// plus the render state it points into (the `live_shell.rs` harness, with the
/// full config supplied by the test).
struct Harness {
    io: *mut Termio<TestMailbox>,
    state: *mut rustty_renderer::state::State,
    mutex: *mut rustty_zigstd::thread::Mutex,
    renderer_mailbox: *mut rustty_renderer::thread::Mailbox,
    renderer_wakeup: xev::Async,
}

impl Harness {
    fn build(
        command: rustty_config::command::Command,
        full_config: &rustty_config::Config,
    ) -> Harness {
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
                demand: Default::default(),
                handoff_gen: Default::default(),
                handoff_park: Default::default(),
            }));
        let renderer_mailbox: *mut rustty_renderer::thread::Mailbox =
            Box::into_raw(rustty_renderer::thread::Mailbox::create());
        let renderer_wakeup = xev::Async::init().expect("Async::init");

        let exec_cfg = Config {
            command: Some(command),
            env: rustty_termio::command::EnvMap::new(),
            env_override: rustty_config::RepeatableStringMap::default(),
            shell_integration: rustty_config::value_types::ShellIntegration::Detect,
            shell_integration_features:
                rustty_config::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: rustty_termio::command::RtPreExecInfo::default(),
            rt_post_fork_info: rustty_termio::command::RtPostForkInfo::default(),
        };
        let backend = Backend::Exec(Exec::init(Global, exec_cfg).expect("Exec::init"));
        let derived = DerivedConfig::init(full_config);
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
        let mailbox = Mailbox::initSPSC().expect("Mailbox::initSPSC");
        let opts = Options {
            size,
            full_config,
            config: derived,
            backend,
            mailbox,
            renderer_state: state,
            renderer_wakeup,
            renderer_mailbox,
            surface_mailbox: TestMailbox,
        };
        let io: *mut Termio<TestMailbox> = Box::into_raw(Box::new(std::mem::MaybeUninit::<
            Termio<TestMailbox>,
        >::uninit()))
        .cast::<Termio<TestMailbox>>();
        // SAFETY: fresh `Termio`-sized storage that `init` writes whole.
        unsafe { Termio::init(io, Global, opts) }.expect("Termio::init");
        unsafe {
            (*state).terminal = std::ptr::addr_of_mut!((*io).terminal);
        }
        Harness {
            io,
            state,
            mutex,
            renderer_mailbox,
            renderer_wakeup,
        }
    }

    fn screen_dump(&self) -> Vec<u8> {
        unsafe { (*self.mutex).lock() };
        let dump = unsafe {
            (*(*self.io).terminal.screens.active).dump_string_alloc(
                Global,
                rustty_vt::point::Point::Active(rustty_vt::point::Coordinate::default()),
            )
        };
        unsafe { (*self.mutex).unlock() };
        dump.unwrap_or_default()
    }

    /// After the io thread is joined (it ran `threadExit` itself).
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

/// `*mut` pair handed to the io thread, as the surface's `IoThreadArgs`.
struct IoArgs(*mut Thread, *mut Termio<TestMailbox>);
// SAFETY: the io thread is the sole user of both until it is joined, except
// for `stop.notify()`, which the surface also calls from its own thread.
unsafe impl Send for IoArgs {}

/// Run `Thread::threadMain` on a new thread until `done` holds, then stop it
/// and join. Returns whether `done` held before the deadline.
fn run_io_thread(h: &Harness, done: impl Fn() -> bool) -> bool {
    let thread: *mut Thread = Box::into_raw(Box::new(Thread::init().expect("Thread::init")));
    let args = IoArgs(thread, h.io);
    let handle = std::thread::Builder::new()
        .name("io".to_string())
        .spawn(move || {
            let args = args;
            // SAFETY: both pointers outlive the join below.
            unsafe { Thread::threadMain(args.0, args.1) }
        })
        .expect("spawn io thread");

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut ok = false;
    while Instant::now() < deadline {
        if done() {
            ok = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }

    // The surface's stop: `io_thread.stop.notify()`, then join.
    unsafe { (*thread).stop.notify() }.expect("stop.notify");
    handle.join().expect("io thread join");
    let thread = unsafe { *Box::from_raw(thread) };
    thread.deinit();
    ok
}

/// S212-F1: a missing `input` path paints the reference's `input` screen.
#[test]
fn missing_input_path_paints_the_input_failure_screen() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let mut full_config = rustty_config::Config::default();
    full_config
        .input
        .list
        .push(rustty_config::io::ReadableIO::Path(
            b"/nonexistent-rustty-t-g/input.txt".to_vec(),
        ));
    let h = Harness::build(
        rustty_config::command::Command::Direct(vec![b"/bin/cat".to_vec()]),
        &full_config,
    );

    const NEEDLE: &[u8] = b"A configured `input` path was not found";
    let found = run_io_thread(&h, || {
        let dump = h.screen_dump();
        dump.windows(NEEDLE.len()).any(|w| w == NEEDLE)
    });
    let dump = h.screen_dump();
    h.teardown();

    assert!(
        found,
        "the input failure screen was not painted; screen: {:?}",
        String::from_utf8_lossy(&dump)
    );
    assert!(
        !dump
            .windows(b"error starting IO thread".len())
            .any(|w| w == b"error starting IO thread"),
        "the general failure screen must not be painted"
    );
}

/// S212-F1, the other `input` member: an `input` file larger than the 10 MiB
/// `readToEndAlloc` limit fails after the backend started, with
/// `error.InputFailed`, which paints the same `input` screen.
#[test]
fn oversized_input_file_paints_the_input_failure_screen() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());

    let path = std::env::temp_dir().join(format!("rustty-t-g-input-{}.bin", std::process::id()));
    std::fs::write(&path, vec![b'a'; 10 * 1024 * 1024 + 1]).expect("write input file");
    let mut full_config = rustty_config::Config::default();
    full_config
        .input
        .list
        .push(rustty_config::io::ReadableIO::Path(
            path.as_os_str().as_encoded_bytes().to_vec(),
        ));
    let h = Harness::build(
        rustty_config::command::Command::Direct(vec![b"/bin/cat".to_vec()]),
        &full_config,
    );

    const NEEDLE: &[u8] = b"A configured `input` path was not found";
    let found = run_io_thread(&h, || {
        let dump = h.screen_dump();
        dump.windows(NEEDLE.len()).any(|w| w == NEEDLE)
    });
    let dump = h.screen_dump();
    h.teardown();
    let _ = std::fs::remove_file(&path);

    assert!(
        found,
        "the input failure screen was not painted; screen: {:?}",
        String::from_utf8_lossy(&dump)
    );
}

/// S212-F2: the io thread's crash `thread_state` is `.io` with the mailbox's
/// surface when it reports the child's exit.
#[test]
fn io_thread_sets_io_crash_thread_state() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    CHILD_EXITED_STATES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();

    let full_config = rustty_config::Config::default();
    let h = Harness::build(
        rustty_config::command::Command::Direct(vec![
            b"/bin/sh".to_vec(),
            b"-c".to_vec(),
            b"exit 7".to_vec(),
        ]),
        &full_config,
    );

    let exited = run_io_thread(&h, || {
        !CHILD_EXITED_STATES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    });
    h.teardown();
    let states = CHILD_EXITED_STATES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();

    assert!(exited, "the child's exit was not reported");
    assert_eq!(
        states,
        vec![Some((rustty_crash::sentry::Type::Io, true))],
        "the io thread's thread_state is .io with the mailbox's surface"
    );
}
