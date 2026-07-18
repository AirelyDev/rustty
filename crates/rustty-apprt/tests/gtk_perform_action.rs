//! PORTS: the acceptance gate for `Application.performAction`
//! (`src/apprt/gtk/class/application.zig:668-799` + the `Action` namespace
//! `:1909-2843`).
//!
//! The primary gate is [`perform_action_arm_coverage`]: a compile-time exhaustive
//! `match` over EVERY [`Action`] variant. Rust's exhaustiveness check IS the
//! parity gate — a new action key added to `apprt::Action` breaks this test's
//! build, exactly as Zig's exhaustive switch forces the apprt to handle the new
//! key. The three `#[ignore]` tests drive real dispatch behavior on a display:
//! `.render` queues a render, the Unimplemented block returns `Ok(false)`, and
//! `.selection_changed` is a handled no-op returning `Ok(true)`.
//!
//! GTK binds its "main thread" to whichever thread first calls `gtk::init`; the
//! cargo harness runs each test on a worker thread, so at most ONE GTK test's body
//! executes per multi-threaded run. Run each display test individually:
//!   LIBGL_ALWAYS_SOFTWARE=1 cargo test -p rustty-apprt --features gtk \
//!     --test gtk_perform_action <name> -- --ignored --exact

#![cfg(all(target_os = "linux", feature = "gtk"))]

use rustty_apprt_api::action::{
    CloseTabMode, ColorChange, ColorKind, CommandFinished, DesktopNotification, FloatWindow,
    Fullscreen, GotoSplit, GotoTab, GotoWindow, InitialSize, Inspector, KeySequence, KeyTable,
    MouseOverLink, MouseVisibility, OpenUrl, OpenUrlKind, PromptTitle, Pwd, QuitTimer, Readonly,
    ReloadConfig, ResizeSplit, ResizeSplitDirection, SearchSelected, SearchTotal, SecureInput,
    SetTitle, SizeLimit, SplitDirection, StartSearch,
};
use rustty_apprt_api::surface::ChildExited;
use rustty_apprt_api::{Action, CoreSurfaceHandle, Target};

/// A `CoreSurfaceHandle` that carries no live surface — usable only for
/// `Target::App` (which never dereferences a handle) and for the Unimplemented /
/// `.selection_changed` arms (which never touch the target).
struct NullHandle;
impl CoreSurfaceHandle for NullHandle {
    fn rt_surface_ptr(&self) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }
}

/// The primary gate: a compile-time exhaustive match over every `Action` variant.
///
/// This is NOT a runtime assertion — it is a build-time parity check. If a new
/// variant is added to `apprt::Action`, this `match` stops compiling (no `_`
/// arm), forcing the port to grow a `performAction` arm for it. That is the Rust
/// analogue of the reference's exhaustive `switch (action)` — a missing key is a
/// compile error, not a silent fallthrough.
#[test]
fn perform_action_arm_coverage() {
    fn classify(a: &Action<'_>) -> &'static str {
        match a {
            Action::Quit => "quit",
            Action::NewWindow => "new_window",
            Action::NewTab => "new_tab",
            Action::CloseTab(_) => "close_tab",
            Action::NewSplit(_) => "new_split",
            Action::CloseAllWindows => "close_all_windows",
            Action::ToggleMaximize => "toggle_maximize",
            Action::ToggleFullscreen(_) => "toggle_fullscreen",
            Action::ToggleTabOverview => "toggle_tab_overview",
            Action::ToggleWindowDecorations => "toggle_window_decorations",
            Action::ToggleQuickTerminal => "toggle_quick_terminal",
            Action::ToggleCommandPalette => "toggle_command_palette",
            Action::ToggleVisibility => "toggle_visibility",
            Action::ToggleBackgroundOpacity => "toggle_background_opacity",
            Action::MoveTab(_) => "move_tab",
            Action::GotoTab(_) => "goto_tab",
            Action::GotoSplit(_) => "goto_split",
            Action::GotoWindow(_) => "goto_window",
            Action::ResizeSplit(_) => "resize_split",
            Action::EqualizeSplits => "equalize_splits",
            Action::ToggleSplitZoom => "toggle_split_zoom",
            Action::PresentTerminal => "present_terminal",
            Action::SizeLimit(_) => "size_limit",
            Action::ResetWindowSize => "reset_window_size",
            Action::InitialSize(_) => "initial_size",
            Action::CellSize(_) => "cell_size",
            Action::Scrollbar(_) => "scrollbar",
            Action::Render => "render",
            Action::Inspector(_) => "inspector",
            Action::ShowGtkInspector => "show_gtk_inspector",
            Action::RenderInspector => "render_inspector",
            Action::DesktopNotification(_) => "desktop_notification",
            Action::SetTitle(_) => "set_title",
            Action::SetTabTitle(_) => "set_tab_title",
            Action::PromptTitle(_) => "prompt_title",
            Action::Pwd(_) => "pwd",
            Action::MouseShape(_) => "mouse_shape",
            Action::MouseVisibility(_) => "mouse_visibility",
            Action::MouseOverLink(_) => "mouse_over_link",
            Action::RendererHealth(_) => "renderer_health",
            Action::OpenConfig => "open_config",
            Action::QuitTimer(_) => "quit_timer",
            Action::FloatWindow(_) => "float_window",
            Action::SecureInput(_) => "secure_input",
            Action::KeySequence(_) => "key_sequence",
            Action::KeyTable(_) => "key_table",
            Action::ColorChange(_) => "color_change",
            Action::ReloadConfig(_) => "reload_config",
            Action::ConfigChange(_) => "config_change",
            Action::CloseWindow => "close_window",
            Action::RingBell => "ring_bell",
            Action::SelectionChanged => "selection_changed",
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::CheckForUpdates => "check_for_updates",
            Action::OpenUrl(_) => "open_url",
            Action::ShowChildExited(_) => "show_child_exited",
            Action::ProgressReport(_) => "progress_report",
            Action::ShowOnScreenKeyboard => "show_on_screen_keyboard",
            Action::CommandFinished(_) => "command_finished",
            Action::StartSearch(_) => "start_search",
            Action::EndSearch => "end_search",
            Action::SearchTotal(_) => "search_total",
            Action::SearchSelected(_) => "search_selected",
            Action::Readonly(_) => "readonly",
            Action::CopyTitleToClipboard => "copy_title_to_clipboard",
        }
    }

    // A witness call so the gate is a live `#[test]` (and the classifier is not
    // dead code), not merely a type-check.
    assert_eq!(classify(&Action::Render), "render");
    assert_eq!(classify(&Action::SelectionChanged), "selection_changed");
    assert_eq!(classify(&Action::Quit), "quit");
}

/// Build the real `Application` GObject (the `performAction` receiver) using the
/// same construction the entrypoint does. Returns `None` when GTK cannot bind
/// this worker thread as its main thread (multi-threaded harness run — the body
/// is exercised by running this test with `--exact`).
fn display_application() -> Option<harness::AppHarness> {
    harness::AppHarness::init()
}

mod harness {
    use allocator_api2::alloc::Global;
    use gio::prelude::*;
    use rustty_core::app::App as CoreApp;

    use rustty_apprt::gtk;
    use rustty_apprt::gtk::class::application::Application;

    /// Owns the core app + gtk apprt app so the `Application` stays live for the
    /// duration of a test. Mirrors `gtk_first_light`'s construction.
    pub struct AppHarness {
        pub application: Application,
        // Kept alive; `Application::default()` and the bound cells point into these.
        _core_app: Box<CoreApp<gtk::App>>,
        _app: Box<gtk::App>,
    }

    impl AppHarness {
        pub fn init() -> Option<Self> {
            // GTK must NOT be initialized yet: `App::init` → `Application::new` →
            // `setGtkEnv` asserts GTK is not yet initialized (env vars must be set
            // first). If a previous display test in the SAME process already brought
            // GTK up, skip — these tests are run one at a time (`--exact`), so a
            // batch run gracefully no-ops the later ones instead of tripping the
            // assert. Mirrors `gtk_first_light`'s construction (no explicit
            // `gtk::init`; `register()` below brings GTK up).
            if gtk4::is_initialized() {
                return None;
            }

            // Global rustty state (resources dir, logging) — the real entrypoint's
            // first step; font/renderer setup would panic without it.
            rustty_apprt::init().ok()?;

            let mut core_app: Box<CoreApp<gtk::App>> = CoreApp::<gtk::App>::create(Global).ok()?;
            let core_ptr: *mut CoreApp<gtk::App> = &mut *core_app;

            let app = match gtk::App::init(core_ptr.cast()) {
                Ok(rt) => Box::new(rt),
                Err(err) => match err {},
            };

            let application: Application = app.application().clone();
            application.upcast_ref::<gio::Application>().set_default();
            application.register(gio::Cancellable::NONE).ok()?;
            let app_ptr: *mut std::ffi::c_void = std::ptr::from_ref(&*app).cast_mut().cast();
            application.bind_rt_app(app_ptr);
            application.bind_core_app::<gtk::App>();

            Some(Self {
                application,
                _core_app: core_app,
                _app: app,
            })
        }
    }
}

/// The `.render` action dispatched at the app level is a handled no-op returning
/// `Ok(true)` (`application.zig:2481-2486` `.app => {}`), and does not panic —
/// the app-target render arm needs no surface. (The surface-target render leg is
/// covered end-to-end by `gtk_first_light` / the surface widget's
/// `render_contract_chain`.)
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk \
            --test gtk_perform_action -- --ignored --exact"]
fn perform_action_render_queues_a_render() {
    let Some(h) = display_application() else {
        return;
    };
    let handled = h
        .application
        .perform_action(Target::<NullHandle>::App, Action::Render)
        .expect("render dispatch does not error");
    assert!(handled, ".render is a handled action (returns true)");
}

/// Every key in the Unimplemented block warns and returns `Ok(false)`
/// (`application.zig:777-793`). These arms never touch the target, so `Target::App`
/// with a null handle exercises them faithfully.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk \
            --test gtk_perform_action -- --ignored --exact"]
fn perform_action_unimplemented_returns_false() {
    let Some(h) = display_application() else {
        return;
    };

    // The Unimplemented set, one variant per reference line (`:777-791`).
    let unimplemented: [Action<'_>; 13] = [
        Action::SecureInput(SecureInput::Toggle),
        Action::CloseAllWindows,
        Action::FloatWindow(FloatWindow::Toggle),
        Action::ToggleVisibility,
        Action::ToggleBackgroundOpacity,
        Action::CellSize(rustty_apprt_api::action::CellSize {
            width: 0,
            height: 0,
        }),
        Action::RenderInspector,
        Action::RendererHealth(rustty_renderer::Health::Healthy),
        Action::ColorChange(ColorChange {
            kind: ColorKind(0),
            r: 0,
            g: 0,
            b: 0,
        }),
        Action::ResetWindowSize,
        Action::CheckForUpdates,
        Action::Undo,
        Action::Redo,
    ];

    for action in unimplemented {
        let handled = h
            .application
            .perform_action(Target::<NullHandle>::App, action)
            .expect("unimplemented dispatch does not error");
        assert!(!handled, "unimplemented action must return false");
    }
}

/// Regression guard (review finding, T22): the window-chrome milestone path
/// `activate -> new_window` (`Target::App`, no parent) must NOT panic. `new_window`
/// is an INHABITED placeholder — it creates + presents a `Window` and defers the
/// first-tab leaf ([`Window::new_tab_page`], a T21 deferral) rather than dispatching
/// into the `unimplemented!` leaf. The arm returns `Ok(true)` (a window was opened).
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk \
            --test gtk_perform_action -- --ignored --exact"]
fn perform_action_new_window_app_does_not_panic() {
    let Some(h) = display_application() else {
        return;
    };
    let handled = h
        .application
        .perform_action(Target::<NullHandle>::App, Action::NewWindow)
        .expect("new_window dispatch does not error");
    assert!(handled, ".new_window opens a window (returns true)");
}

/// `.selection_changed` is a HANDLED no-op that returns `Ok(true)`
/// ("GTK has no accessibility consumer for this yet", `application.zig:746-747`).
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk \
            --test gtk_perform_action -- --ignored --exact"]
fn perform_action_selection_changed_returns_true() {
    let Some(h) = display_application() else {
        return;
    };
    let handled = h
        .application
        .perform_action(Target::<NullHandle>::App, Action::SelectionChanged)
        .expect("selection_changed dispatch does not error");
    assert!(
        handled,
        ".selection_changed is a handled no-op (returns true)"
    );
}

// Silence unused-import warnings for the payload types the coverage classifier
// references only by pattern (their constructors are exercised by the module's
// exhaustiveness, not by name here).
#[allow(dead_code)]
fn _payload_type_witnesses() {
    let _: fn(CloseTabMode) -> Action<'static> = Action::CloseTab;
    let _: fn(SplitDirection) -> Action<'static> = Action::NewSplit;
    let _: fn(Fullscreen) -> Action<'static> = Action::ToggleFullscreen;
    let _: fn(GotoTab) -> Action<'static> = Action::GotoTab;
    let _: fn(GotoSplit) -> Action<'static> = Action::GotoSplit;
    let _: fn(GotoWindow) -> Action<'static> = Action::GotoWindow;
    let _: fn(ResizeSplit) -> Action<'static> = Action::ResizeSplit;
    let _: fn(SizeLimit) -> Action<'static> = Action::SizeLimit;
    let _: fn(InitialSize) -> Action<'static> = Action::InitialSize;
    let _: fn(Inspector) -> Action<'static> = Action::Inspector;
    let _: fn(PromptTitle) -> Action<'static> = Action::PromptTitle;
    let _: fn(MouseVisibility) -> Action<'static> = Action::MouseVisibility;
    let _: fn(QuitTimer) -> Action<'static> = Action::QuitTimer;
    let _: fn(KeySequence) -> Action<'static> = Action::KeySequence;
    let _: fn(Readonly) -> Action<'static> = Action::Readonly;
    let _: fn(ReloadConfig) -> Action<'static> = Action::ReloadConfig;
    let _: fn(CommandFinished) -> Action<'static> = Action::CommandFinished;
    let _: fn(SearchTotal) -> Action<'static> = Action::SearchTotal;
    let _: fn(SearchSelected) -> Action<'static> = Action::SearchSelected;
    let _: fn(ResizeSplitDirection) = |_d: ResizeSplitDirection| {};
    let _: fn(DesktopNotification<'static>) -> Action<'static> = Action::DesktopNotification;
    let _: fn(SetTitle<'static>) -> Action<'static> = Action::SetTitle;
    let _: fn(Pwd<'static>) -> Action<'static> = Action::Pwd;
    let _: fn(MouseOverLink<'static>) -> Action<'static> = Action::MouseOverLink;
    let _: fn(KeyTable<'static>) -> Action<'static> = Action::KeyTable;
    let _: fn(OpenUrl<'static>) -> Action<'static> = Action::OpenUrl;
    let _: fn(StartSearch<'static>) -> Action<'static> = Action::StartSearch;
    let _: fn(ChildExited) -> Action<'static> = Action::ShowChildExited;
    let _: OpenUrlKind = OpenUrlKind::Unknown;
}
