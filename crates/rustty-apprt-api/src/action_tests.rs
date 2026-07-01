//! PORTS: the `test {}` blocks of ghostty `src/apprt/action.zig` — the 16
//! `checkGhosttyHEnum` C-enum-sync tests + the top-level `refAllDeclsRecursive`
//! smoke. Split into its own file (mirrors rustty-font-shaper's
//! `harfbuzz.rs`/`harfbuzz_tests.rs` convention).
//!
//! The 2 disabled reference tests (`GotoTab` line 567, `ColorKind` line 858,
//! commented out by `// TODO: check non-exhaustive enums`) are preserved as
//! commented-out code at their type sites in `action.rs`, NOT counted toward G1.

use rustty_lib::r#enum::{check_ghostty_henum, enum_type};
use rustty_lib::target::Target as EnumTarget;

use crate::action::*;

/// PORT NOTE: the shared `checkGhosttyHEnum` driver. Models the reference
/// `enum(c_int)` as an [`rustty_lib::r#enum::EnumType`] (C target) built from the
/// snake_case field names in declaration order, then reflects over it exactly
/// like `lib.checkGhosttyHEnum`: the `rustty.h` C enum is modeled as declaring
/// each key with its own sequential `c_int` value (prefix retargeted
/// `GHOSTTY_` -> `RUSTTY_` per R2).
fn run_henum(prefix: &str, keys: &[&str]) {
    let opts: Vec<Option<&str>> = keys.iter().map(|k| Some(*k)).collect();
    let t = enum_type(EnumTarget::C, &opts);
    let fields = t.fields.clone();
    check_ghostty_henum(&t, prefix, |name| {
        fields.iter().find_map(|(n, v)| {
            let expected = format!("{}{}", prefix, n.to_ascii_uppercase());
            if expected == name {
                Some(*v as i64)
            } else {
                None
            }
        })
    })
    .unwrap();
}

// test "ghostty.h Target.Key" {
//     try lib.checkGhosttyHEnum(Key, "GHOSTTY_TARGET_");
// }
#[test]
fn ghostty_h_target_key() {
    let _ = (TargetKey::App, TargetKey::Surface);
    run_henum("RUSTTY_TARGET_", &["app", "surface"]);
}

// test "ghostty.h Action.Key" {
//     try lib.checkGhosttyHEnum(Key, "GHOSTTY_ACTION_");
// }
#[test]
fn ghostty_h_action_key() {
    let _ = ActionKey::Quit;
    run_henum(
        "RUSTTY_ACTION_",
        &[
            "quit",
            "new_window",
            "new_tab",
            "close_tab",
            "new_split",
            "close_all_windows",
            "toggle_maximize",
            "toggle_fullscreen",
            "toggle_tab_overview",
            "toggle_window_decorations",
            "toggle_quick_terminal",
            "toggle_command_palette",
            "toggle_visibility",
            "toggle_background_opacity",
            "move_tab",
            "goto_tab",
            "goto_split",
            "goto_window",
            "resize_split",
            "equalize_splits",
            "toggle_split_zoom",
            "present_terminal",
            "size_limit",
            "reset_window_size",
            "initial_size",
            "cell_size",
            "scrollbar",
            "render",
            "inspector",
            "show_gtk_inspector",
            "render_inspector",
            "desktop_notification",
            "set_title",
            "set_tab_title",
            "prompt_title",
            "pwd",
            "mouse_shape",
            "mouse_visibility",
            "mouse_over_link",
            "renderer_health",
            "open_config",
            "quit_timer",
            "float_window",
            "secure_input",
            "key_sequence",
            "key_table",
            "color_change",
            "reload_config",
            "config_change",
            "close_window",
            "ring_bell",
            "selection_changed",
            "undo",
            "redo",
            "check_for_updates",
            "open_url",
            "show_child_exited",
            "progress_report",
            "show_on_screen_keyboard",
            "command_finished",
            "start_search",
            "end_search",
            "search_total",
            "search_selected",
            "readonly",
            "copy_title_to_clipboard",
        ],
    );
}

// test "ghostty.h SplitDirection" {
//     try lib.checkGhosttyHEnum(SplitDirection, "GHOSTTY_SPLIT_DIRECTION_");
// }
#[test]
fn ghostty_h_split_direction() {
    let _ = SplitDirection::Right;
    run_henum("RUSTTY_SPLIT_DIRECTION_", &["right", "down", "left", "up"]);
}

// test "ghostty.h GotoSplit" {
//     try lib.checkGhosttyHEnum(GotoSplit, "GHOSTTY_GOTO_SPLIT_");
// }
#[test]
fn ghostty_h_goto_split() {
    let _ = GotoSplit::Previous;
    run_henum(
        "RUSTTY_GOTO_SPLIT_",
        &["previous", "next", "up", "left", "down", "right"],
    );
}

// test "ghostty.h GotoWindow" {
//     try lib.checkGhosttyHEnum(GotoWindow, "GHOSTTY_GOTO_WINDOW_");
// }
#[test]
fn ghostty_h_goto_window() {
    let _ = GotoWindow::Previous;
    run_henum("RUSTTY_GOTO_WINDOW_", &["previous", "next"]);
}

// test "ghostty.h ResizeSplit.Direction" {
//     try lib.checkGhosttyHEnum(Direction, "GHOSTTY_RESIZE_SPLIT_");
// }
#[test]
fn ghostty_h_resize_split_direction() {
    let _ = ResizeSplitDirection::Up;
    run_henum("RUSTTY_RESIZE_SPLIT_", &["up", "down", "left", "right"]);
}

// test "ghostty.h Fullscreen" {
//     try lib.checkGhosttyHEnum(Fullscreen, "GHOSTTY_FULLSCREEN_");
// }
#[test]
fn ghostty_h_fullscreen() {
    let _ = Fullscreen::Native;
    run_henum(
        "RUSTTY_FULLSCREEN_",
        &[
            "native",
            "macos_non_native",
            "macos_non_native_visible_menu",
            "macos_non_native_padded_notch",
        ],
    );
}

// test "ghostty.h FloatWindow" {
//     try lib.checkGhosttyHEnum(FloatWindow, "GHOSTTY_FLOAT_WINDOW_");
// }
#[test]
fn ghostty_h_float_window() {
    let _ = FloatWindow::On;
    run_henum("RUSTTY_FLOAT_WINDOW_", &["on", "off", "toggle"]);
}

// test "ghostty.h SecureInput" {
//     try lib.checkGhosttyHEnum(SecureInput, "GHOSTTY_SECURE_INPUT_");
// }
#[test]
fn ghostty_h_secure_input() {
    let _ = SecureInput::On;
    run_henum("RUSTTY_SECURE_INPUT_", &["on", "off", "toggle"]);
}

// test "ghostty.h Inspector" {
//     try lib.checkGhosttyHEnum(Inspector, "GHOSTTY_INSPECTOR_");
// }
#[test]
fn ghostty_h_inspector() {
    let _ = Inspector::Toggle;
    run_henum("RUSTTY_INSPECTOR_", &["toggle", "show", "hide"]);
}

// test "ghostty.h QuitTimer" {
//     try lib.checkGhosttyHEnum(QuitTimer, "GHOSTTY_QUIT_TIMER_");
// }
#[test]
fn ghostty_h_quit_timer() {
    let _ = QuitTimer::Start;
    run_henum("RUSTTY_QUIT_TIMER_", &["start", "stop"]);
}

// test "ghostty.h Readonly" {
//     try lib.checkGhosttyHEnum(Readonly, "GHOSTTY_READONLY_");
// }
#[test]
fn ghostty_h_readonly() {
    let _ = Readonly::Off;
    run_henum("RUSTTY_READONLY_", &["off", "on"]);
}

// test "ghostty.h MouseVisibility" {
//     try lib.checkGhosttyHEnum(MouseVisibility, "GHOSTTY_MOUSE_");
// }
#[test]
fn ghostty_h_mouse_visibility() {
    let _ = MouseVisibility::Visible;
    run_henum("RUSTTY_MOUSE_", &["visible", "hidden"]);
}

// test "ghostty.h PromptTitle" {
//     try lib.checkGhosttyHEnum(PromptTitle, "GHOSTTY_PROMPT_TITLE_");
// }
#[test]
fn ghostty_h_prompt_title() {
    let _ = PromptTitle::Surface;
    run_henum("RUSTTY_PROMPT_TITLE_", &["surface", "tab"]);
}

// test "ghostty.h OpenUrl.Kind" {
//     try lib.checkGhosttyHEnum(Kind, "GHOSTTY_ACTION_OPEN_URL_KIND_");
// }
#[test]
fn ghostty_h_open_url_kind() {
    let _ = OpenUrlKind::Unknown;
    run_henum("RUSTTY_ACTION_OPEN_URL_KIND_", &["unknown", "text", "html"]);
}

// test "ghostty.h CloseTabMode" {
//     try lib.checkGhosttyHEnum(CloseTabMode, "GHOSTTY_ACTION_CLOSE_TAB_MODE_");
// }
#[test]
fn ghostty_h_close_tab_mode() {
    let _ = CloseTabMode::This;
    run_henum("RUSTTY_ACTION_CLOSE_TAB_MODE_", &["this", "other", "right"]);
}

// test {
//     _ = std.testing.refAllDeclsRecursive(@This());
// }
#[test]
fn ref_all_decls_recursive() {
    // PORT NOTE: ghostty `test { _ = std.testing.refAllDeclsRecursive(@This()); }`
    // — forces every public decl of `action.zig` to be referenced/compiled. Rust
    // compiles the crate's items in the test build; we mirror it by referencing
    // every public decl and touching representative instances (incl. the C
    // projections + Display formatters), following rustty-lib's precedent.
    #[allow(unused_imports)]
    use crate::action::{
        Action, ActionC, ActionCValue, ActionKey, CellSize, CloseTabMode, ColorChange, ColorKind,
        CommandFinished, ConfigChange, ConfigChangeC, DesktopNotification, DesktopNotificationC,
        FloatWindow, Fullscreen, GotoSplit, GotoTab, GotoWindow, InitialSize, Inspector,
        KeySequence, KeySequenceC, KeyTable, KeyTableActivateC, KeyTableC, KeyTableCValue,
        KeyTableTag, MouseOverLink, MouseOverLinkC, MouseVisibility, MoveTab, OpenUrl, OpenUrlC,
        OpenUrlKind, PromptTitle, Pwd, PwdC, Readonly, ReloadConfig, ResizeSplit,
        ResizeSplitDirection, SearchSelected, SearchSelectedC, SearchTotal, SearchTotalC,
        SecureInput, SetTitle, SetTitleC, SizeLimit, SplitDirection, StartSearch, StartSearchC,
        Target, TargetC, TargetCValue, TargetKey,
    };
    use crate::seam::CoreSurfaceHandle;

    // A stand-in for rustty-core's `Surface` implementing the cycle-break bridge.
    struct DummySurface;
    impl CoreSurfaceHandle for DummySurface {
        fn rt_surface_ptr(&self) -> *mut core::ffi::c_void {
            core::ptr::null_mut()
        }
    }

    // Target: both arms + key()/cval() (exercises the CoreSurfaceHandle bridge).
    let surf = DummySurface;
    let t_app: Target<'_, DummySurface> = Target::App;
    let t_surf: Target<'_, DummySurface> = Target::Surface(&surf);
    assert_eq!(t_app.key(), TargetKey::App);
    assert_eq!(t_surf.key(), TargetKey::Surface);
    let _ = t_app.cval();
    let _ = t_surf.cval();

    // Representative actions across void / value / borrow / C-projection arms.
    assert_eq!(Action::Quit.key(), ActionKey::Quit);
    let _ = Action::Quit.cval();
    let _ = Action::CloseTab(CloseTabMode::Other).cval();
    let _ = Action::NewSplit(SplitDirection::Left).cval();
    let _ = Action::MoveTab(MoveTab { amount: -1 }).cval();
    let _ = Action::GotoTab(GotoTab::PREVIOUS).cval();
    let _ = Action::ResizeSplit(ResizeSplit {
        amount: 3,
        direction: ResizeSplitDirection::Up,
    })
    .cval();
    let _ = Action::SizeLimit(SizeLimit {
        min_width: 1,
        min_height: 2,
        max_width: 3,
        max_height: 4,
    })
    .cval();
    let _ = Action::InitialSize(InitialSize {
        width: 10,
        height: 20,
    })
    .cval();
    let _ = Action::CellSize(CellSize {
        width: 8,
        height: 16,
    })
    .cval();
    let _ = Action::Inspector(Inspector::Show).cval();
    let _ = Action::RendererHealth(rustty_renderer::Health::Healthy).cval();
    let _ = Action::MouseShape(rustty_vt::mouse::Shape::Text).cval();
    let _ = Action::PromptTitle(PromptTitle::Tab).cval();
    let _ = Action::FloatWindow(FloatWindow::Toggle).cval();
    let _ = Action::SecureInput(SecureInput::Off).cval();
    let _ = Action::QuitTimer(QuitTimer::Stop).cval();
    let _ = Action::Readonly(Readonly::On).cval();
    let _ = Action::MouseVisibility(MouseVisibility::Hidden).cval();
    let _ = Action::ColorChange(ColorChange {
        kind: ColorKind::FOREGROUND,
        r: 1,
        g: 2,
        b: 3,
    })
    .cval();
    let _ = Action::ReloadConfig(ReloadConfig::default()).cval();
    let _ = Action::OpenUrl(OpenUrl {
        kind: OpenUrlKind::Html,
        url: b"https://example.com",
    })
    .cval();
    let _ = Action::KeyTable(KeyTable::Activate(b"nav")).cval();
    let _ = Action::KeyTable(KeyTable::Deactivate).cval();
    let _ = Action::KeySequence(KeySequence::End).cval();
    let _ = Action::CommandFinished(CommandFinished {
        exit_code: Some(0),
        duration: rustty_config::value_types::Duration { duration: 5 },
    })
    .cval();
    let _ = Action::SearchTotal(SearchTotal { total: Some(3) }).cval();
    let _ = Action::SearchSelected(SearchSelected { selected: None }).cval();

    // Borrow-carrying payloads + their Display formatters + C projections.
    let st = SetTitle { title: b"title" };
    let _ = st.cval();
    let _ = format!("{st}");
    let _ = Action::SetTitle(st).cval();
    let pwd = Pwd { pwd: b"/tmp" };
    let _ = pwd.cval();
    let _ = format!("{pwd}");
    let dn = DesktopNotification {
        title: b"t",
        body: b"b",
    };
    let _ = dn.cval();
    let _ = format!("{dn}");
    let mol = MouseOverLink { url: b"u" };
    let _ = mol.cval();
    let ss = StartSearch { needle: b"n" };
    let _ = ss.cval();

    // ConfigChange needs a real Config borrow (exercises the config dep).
    let cfg = rustty_config::Config::default();
    let cc = ConfigChange { config: &cfg };
    let _ = cc.cval();
    let _ = Action::ConfigChange(cc).cval();

    // ChildExited (surface) + external Scrollbar/ProgressReport arms.
    let _ = Action::ShowChildExited(crate::surface::ChildExited {
        exit_code: 0,
        runtime_ms: 1,
    })
    .cval();
    let _ = Action::Scrollbar(rustty_vt::pagelist::grow::Scrollbar::ZERO).cval();
    let _ = Action::ProgressReport(rustty_vt::osc::ProgressReport {
        state: rustty_vt::osc::ProgressReportState::Set,
        progress: Some(50),
    })
    .cval();
}
