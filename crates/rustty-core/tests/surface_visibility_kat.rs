//! `P3-W11 Task 6` — **V21**: the Surface-level visibility KAT
//! (`.port/phase-2/VERIFICATION_DEBT.md` row V21, oracle `6c8c07981`).
//!
//! `Surface::occlusionCallback`'s dedup early-return is an **observable change
//! to the renderer path**, not an additive feature: before `6c8c07981` every
//! occlusion callback pushed a renderer `Visible` message and queued a render.
//! Nothing in the tree exercised it, because `rustty-core` has no `Surface`
//! fixture — a real `Surface` owns a live `App`, a renderer thread, an io
//! thread and a pty.
//!
//! This file builds the `TestSurface` the V21 recipe describes: a heap-pinned
//! `Surface<TestApp>` in `MaybeUninit` with **only** the fields
//! `occlusionCallback` reads written, following the established
//! `app.rs::fabricateSurface` precedent (field-projected raw-pointer writes;
//! the untouched fields are never read and `MaybeUninit` never drops), plus a
//! real `Termio` assembled the way `rustty-termio`'s `exec.rs` `TestTermio`
//! assembles one, with its self-referential pointers re-seated at the final
//! in-Surface address.
//!
//! SCOPE, HONESTLY: this closes V21's part (a) — the dedup — end to end, and
//! the encoding half of part (b). The FINAL hop of part (b), "the queued
//! message is drained by `termio::Thread` and the bytes land in the pty write
//! queue", still needs a live pty + io thread and is NOT covered here; see the
//! V21 row for what remains.

use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use allocator_api2::alloc::Global as Allocator;
use rustty_apprt_api as apprt;
use rustty_core::app::App;
use rustty_core::surface::{Surface, SurfaceMailbox};
use rustty_termio as termio;
use rustty_vt::modes::Mode;

// ---------------------------------------------------------------------------
// The fake apprt runtime. Mirrors `app.rs`'s `test_runtime::TestApp`.
// ---------------------------------------------------------------------------

struct TestApp {
    config: rustty_config::Config,
    wakeups: AtomicUsize,
    actions: Mutex<Vec<apprt::action::ActionKey>>,
}

impl TestApp {
    fn new() -> TestApp {
        TestApp {
            config: rustty_config::Config::default_config(),
            wakeups: AtomicUsize::new(0),
            actions: Mutex::new(Vec::new()),
        }
    }
}

impl apprt::App for TestApp {
    type Surface = TestRtSurface;

    fn perform_action<C: apprt::CoreSurfaceHandle>(
        &self,
        _target: apprt::Target<'_, C>,
        action: apprt::Action<'_>,
    ) -> Result<bool, apprt::ApprtError> {
        self.actions.lock().unwrap().push(action.key());
        Ok(true)
    }

    fn perform_ipc(
        &self,
        _target: apprt::ipc::Target<'_>,
        _action: apprt::ipc::Action<'_>,
    ) -> Result<bool, apprt::ipc::Errors> {
        unimplemented!("V21 KAT: not driven")
    }

    fn wakeup(&self) {
        self.wakeups.fetch_add(1, Ordering::SeqCst);
    }

    fn config(&self) -> &rustty_config::Config {
        &self.config
    }

    fn keyboard_layout(&self) -> rustty_input::keyboard::Layout {
        rustty_input::keyboard::Layout::Unknown
    }
}

struct TestRtSurface {
    core: *mut Surface<TestApp>,
    app: *mut TestApp,
}

impl apprt::Surface for TestRtSurface {
    type App = TestApp;
    type Core = Surface<TestApp>;

    fn core(&self) -> *mut Surface<TestApp> {
        self.core
    }
    fn rt_app(&self) -> *mut TestApp {
        self.app
    }
    fn close(&self, _process_alive: bool) {}
    fn deinit(&mut self) {}
    fn get_content_scale(&self) -> Result<apprt::ContentScale, apprt::ApprtError> {
        unimplemented!("V21 KAT: not driven")
    }
    fn get_size(&self) -> Result<apprt::SurfaceSize, apprt::ApprtError> {
        unimplemented!("V21 KAT: not driven")
    }
    fn get_title(&self) -> Option<&std::ffi::CStr> {
        None
    }
    fn supports_clipboard(&self, _clipboard_type: apprt::Clipboard) -> bool {
        unimplemented!("V21 KAT: not driven")
    }
    fn clipboard_request(
        &self,
        _clipboard_type: apprt::Clipboard,
        _state: apprt::ClipboardRequest,
    ) -> Result<bool, apprt::ApprtError> {
        unimplemented!("V21 KAT: not driven")
    }
    fn set_clipboard(
        &self,
        _clipboard_type: apprt::Clipboard,
        _contents: &[apprt::ClipboardContent<'_>],
        _confirm: bool,
    ) -> Result<(), apprt::ApprtError> {
        unimplemented!("V21 KAT: not driven")
    }
    fn get_cursor_pos(&self) -> Result<apprt::CursorPos, apprt::ApprtError> {
        unimplemented!("V21 KAT: not driven")
    }
    fn default_termio_env(&self) -> Result<rustty_global::EnvMap, apprt::ApprtError> {
        unimplemented!("V21 KAT: not driven")
    }
}

// ---------------------------------------------------------------------------
// The fixture.
// ---------------------------------------------------------------------------

struct TestSurface {
    surface: *mut Surface<TestApp>,
    /// The termio mailbox `queueIo` pushes into (a raw alias of `surface.io.mailbox`).
    termio_mailbox: *mut termio::mailbox::Mailbox,
    /// The renderer mailbox `occlusionCallback` pushes into.
    renderer_mailbox: *mut rustty_renderer::thread::Mailbox,
    /// Owned allocations to release at the end.
    mu: Box<MaybeUninit<Surface<TestApp>>>,
    app: *mut App<TestApp>,
    rt_app: Box<TestApp>,
    rt_surface: Box<TestRtSurface>,
    mutex: *mut rustty_zigstd::thread::Mutex,
}

impl TestSurface {
    fn build() -> TestSurface {
        let mut rt_app = Box::new(TestApp::new());
        let app: *mut App<TestApp> = Box::into_raw(App::<TestApp>::create(Allocator).unwrap());

        let mut mu: Box<MaybeUninit<Surface<TestApp>>> = Box::new(MaybeUninit::uninit());
        let sp = mu.as_mut_ptr();

        let mut rt_surface = Box::new(TestRtSurface {
            core: sp,
            app: &raw mut *rt_app,
        });

        let mutex: *mut rustty_zigstd::thread::Mutex =
            Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
        let renderer_mailbox: *mut rustty_renderer::thread::Mailbox =
            Box::into_raw(rustty_renderer::thread::Mailbox::create());
        let renderer_wakeup = rustty_xev::Async::init().expect("Async::init");

        // The terminal (80x24) that lives inside the Termio.
        let terminal = rustty_vt::terminal::Terminal::init(
            Allocator,
            rustty_vt::terminal::Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                colors: rustty_vt::terminal::Colors::default(),
                default_modes: rustty_vt::modes::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .expect("Terminal::init");

        let full_config = rustty_config::Config::default_config();
        let derived = termio::termio_impl::DerivedConfig::init(&full_config);

        let exec_cfg = termio::exec::Config {
            command: Some(rustty_config::command::Command::Direct(vec![
                b"/bin/cat".to_vec()
            ])),
            env: rustty_global::EnvMap::new(),
            env_override: rustty_config::RepeatableStringMap::default(),
            shell_integration: rustty_config::value_types::ShellIntegration::Detect,
            shell_integration_features:
                rustty_config::value_types::ShellIntegrationFeatures::default(),
            cursor_blink: None,
            working_directory: None,
            resources_dir: None,
            term: b"xterm-256color".to_vec(),
            rt_pre_exec_info: Default::default(),
            rt_post_fork_info: Default::default(),
        };
        let backend = termio::backend::Backend::Exec(
            termio::exec::Exec::init(Allocator, exec_cfg).expect("Exec::init"),
        );

        let mailbox = termio::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC");

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

        let surface_mailbox = SurfaceMailbox::<TestApp> {
            surface: sp,
            app: rustty_core::app::Mailbox::<TestApp> {
                rt_app: &raw mut *rt_app,
                mailbox: unsafe { &raw mut (*app).mailbox },
            },
        };

        // The renderer State lives BY VALUE inside the Surface; both the
        // Surface and the Termio reach it, the latter through a raw pointer, so
        // it is written into the Surface first and pointed at afterwards.
        unsafe {
            std::ptr::addr_of_mut!((*sp).renderer_state).write(rustty_renderer::state::State {
                mutex,
                terminal: std::ptr::null_mut(),
                inspector: None,
                inspector_ptr: None,
                preedit: None,
                mouse: rustty_renderer::state::Mouse::default(),
                demand: Default::default(),
                handoff_gen: Default::default(),
                handoff_park: Default::default(),
            });
        }
        let state: *mut rustty_renderer::state::State =
            unsafe { std::ptr::addr_of_mut!((*sp).renderer_state) };

        let handler = termio::stream_handler::StreamHandler::<SurfaceMailbox<TestApp>> {
            alloc: Allocator,
            size: std::ptr::null_mut(),
            terminal: std::ptr::null_mut(),
            termio_mailbox: std::ptr::null_mut(),
            surface_mailbox,
            renderer_state: state,
            renderer_mailbox,
            renderer_wakeup,
            enquiry_response: derived.enquiry_response.clone(),
            osc_color_report_format: derived.osc_color_report_format,
            clipboard_write: derived.clipboard_write,
            apc: rustty_vt::apc::Handler::default(),
            dcs: rustty_vt::dcs::Handler::default(),
            tmux_viewer: None,
            termio_messaged: false,
            seen_title: false,
        };
        let terminal_stream = termio::stream_handler::Stream::init(handler);

        unsafe {
            std::ptr::addr_of_mut!((*sp).io).write(termio::Termio {
                alloc: Allocator,
                backend,
                config: derived,
                terminal,
                renderer_state: state,
                renderer_wakeup,
                renderer_mailbox,
                surface_mailbox,
                size,
                mailbox,
                terminal_stream,
                last_cursor_reset: None,
                thread_enter_state: None,
            });

            // Re-seat the self-referential pointers at the FINAL address.
            let io = std::ptr::addr_of_mut!((*sp).io);
            (*io).terminal_stream.handler.terminal = std::ptr::addr_of_mut!((*io).terminal);
            (*io).terminal_stream.handler.size = std::ptr::addr_of_mut!((*io).size);
            (*io).terminal_stream.handler.termio_mailbox = std::ptr::addr_of_mut!((*io).mailbox);
            (*state).terminal = std::ptr::addr_of_mut!((*io).terminal);

            // The remaining fields `occlusionCallback` reaches.
            std::ptr::addr_of_mut!((*sp).id).write(1);
            std::ptr::addr_of_mut!((*sp).app).write(app);
            std::ptr::addr_of_mut!((*sp).rt_app).write(&raw mut *rt_app);
            std::ptr::addr_of_mut!((*sp).rt_surface).write(&raw mut *rt_surface);
            std::ptr::addr_of_mut!((*sp).visible).write(true);
            std::ptr::addr_of_mut!((*sp).readonly).write(false);
            std::ptr::addr_of_mut!((*sp).renderer_thread.mailbox).write(renderer_mailbox);
            std::ptr::addr_of_mut!((*sp).renderer_thread.wakeup)
                .write(rustty_xev::Async::init().expect("Async::init"));
        }

        TestSurface {
            surface: sp,
            termio_mailbox: unsafe { std::ptr::addr_of_mut!((*sp).io.mailbox) },
            renderer_mailbox,
            mu,
            app,
            rt_app,
            rt_surface,
            mutex,
        }
    }

    fn surface(&mut self) -> &mut Surface<TestApp> {
        unsafe { &mut *self.surface }
    }

    fn termio_pop(&self) -> Option<termio::Message<'static>> {
        match unsafe { &*self.termio_mailbox } {
            termio::mailbox::Mailbox::Spsc { queue, .. } => queue.pop(),
        }
    }

    fn renderer_drain(&self) -> Vec<rustty_renderer::message::Message> {
        let mb = unsafe { &*self.renderer_mailbox };
        let mut out = Vec::new();
        while let Some(m) = mb.pop() {
            out.push(m);
        }
        out
    }

    fn set_report_visibility(&mut self, on: bool) {
        unsafe {
            (*self.surface)
                .io
                .terminal
                .modes
                .set(Mode::ReportVisibility, on);
        }
    }
}

// ---------------------------------------------------------------------------
// The KAT.
// ---------------------------------------------------------------------------

#[test]
fn surface_occlusion_callback_dedups_and_reports() {
    // REBUILD SENTINEL: verified to fire when tripped (see the lane report).
    assert_eq!(2 + 2, 4, "REBUILD SENTINEL");

    let mut f = TestSurface::build();
    f.set_report_visibility(true);
    // `build` seeds `visible = true`, so the first callback below is a real
    // transition. Drain anything the setup queued.
    let _ = f.renderer_drain();
    while f.termio_pop().is_some() {}

    // (a) A state CHANGE pushes exactly one renderer `Visible(false)` and one
    //     unforced `VisibilityReport { visible: false }`.
    f.surface().occlusionCallback(false).unwrap();

    let msgs = f.renderer_drain();
    assert_eq!(
        1,
        msgs.len(),
        "a state change must push exactly one renderer message"
    );
    assert!(
        matches!(msgs[0], rustty_renderer::message::Message::Visible(false)),
        "the renderer message must be Visible(false), got {:?}",
        std::mem::discriminant(&msgs[0])
    );
    match f.termio_pop() {
        Some(termio::Message::VisibilityReport { visible, force }) => {
            assert!(!visible);
            assert!(!force, "an occlusion-driven report is UNFORCED");
        }
        other => panic!("expected a VisibilityReport, got some={}", other.is_some()),
    }
    assert!(f.termio_pop().is_none());
    assert!(
        !unsafe { (*f.surface).io.terminal.flags.visible },
        "the terminal's flags.visible must be updated under the renderer lock"
    );

    // (a, the dedup) A REPEAT callback in the same state must push NOTHING —
    // neither a renderer message nor a report. This is the assertion the V21
    // row names, and the one that fails if the early-return is deleted.
    f.surface().occlusionCallback(false).unwrap();
    assert_eq!(
        0,
        f.renderer_drain().len(),
        "a repeat same-state occlusion callback must not reach the renderer"
    );
    assert!(
        f.termio_pop().is_none(),
        "a repeat same-state occlusion callback must not queue a report"
    );

    // A transition BACK is not deduped.
    f.surface().occlusionCallback(true).unwrap();
    assert_eq!(1, f.renderer_drain().len());
    assert!(matches!(
        f.termio_pop(),
        Some(termio::Message::VisibilityReport {
            visible: true,
            force: false
        })
    ));

    // With mode 2033 DISABLED, the renderer message still goes out but no
    // report is queued — `occlusionCallback` reads the mode under the lock.
    f.set_report_visibility(false);
    f.surface().occlusionCallback(false).unwrap();
    assert_eq!(1, f.renderer_drain().len());
    assert!(
        f.termio_pop().is_none(),
        "no report when mode 2033 is disabled"
    );

    // (b, encoding half) The bytes the queued report turns into. The mailbox ->
    // pty hop needs a live io thread and is recorded as remaining debt in V21;
    // this pins the payload the hop carries.
    let mut buf = Vec::new();
    rustty_vt::device_status::encode_visibility_report(
        &mut buf,
        rustty_vt::device_status::Visibility::NotVisible,
    );
    assert_eq!(b"\x1b[?999;2n".to_vec(), buf);

    drop(f);
}

impl Drop for TestSurface {
    fn drop(&mut self) {
        unsafe {
            let sp = self.surface;
            (*sp).io.terminal.deinit(Allocator);
            (*sp).renderer_thread.wakeup.deinit();
            (*sp).io.renderer_wakeup.deinit();
            drop(Box::from_raw(self.mutex));
            Box::from_raw(self.renderer_mailbox).destroy();
            Box::from_raw(self.app).destroy();
        }
        let _ = &self.mu;
        let _ = &self.rt_app;
        let _ = &self.rt_surface;
    }
}
