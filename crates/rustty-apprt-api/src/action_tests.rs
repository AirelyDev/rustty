//! PORTS: the `test {}` blocks of ghostty `src/apprt/action.zig` — the 17
//! `checkGhosttyHEnum` C-enum-sync tests + the top-level `refAllDeclsRecursive`
//! smoke. Split into its own file (mirrors rustty-font-shaper's
//! `harfbuzz.rs`/`harfbuzz_tests.rs` convention).
//!
//! The 2 disabled reference tests (`GotoTab` line 567, `ColorKind` line 858,
//! commented out by `// TODO: check non-exhaustive enums`) are preserved as
//! commented-out code at their type sites in `action.rs`, NOT counted toward G1.

use rustty_lib::c_header::header_decls;
use rustty_lib::r#enum::{check_ghostty_henum, enum_type_values, EnumType};

use crate::action::*;

/// The checked-in public C header, embedded at compile time.
///
/// PORT NOTE: the reference `lib.checkGhosttyHEnum` does
/// `const c = @import("ghostty.h")` — it reflects over the REAL generated
/// header. Rust has no `@import` for a C header, so these tests read the
/// checked-in header's text and parse the named enum block out of it
/// (`rustty_lib::c_header`). The point is the same: the C declarations the
/// check compares against come from the header, never from the descriptor
/// under test.
const HEADER_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../include/rustty-embed.h"
));

/// PORT NOTE: the shared `checkGhosttyHEnum` driver. The reference reflects
/// over the enum type itself (`@typeInfo(T).@"enum".fields`), so the keys AND
/// their backing ints both come from `T`. Rust has no such reflection, so
/// [`henum!`] below writes the pairing once — `("quit", ActionKey::Quit)` — and
/// the VALUE half is `<ActionKey>::Quit as i64`, read out of the enum. The
/// header supplies the C side. Every link is therefore a real comparison:
/// header <-> Rust enum discriminant.
///
/// FIX (S45-F1): the `c` closure previously returned each field's own value out
/// of the descriptor being checked, so all 17 tests below compared the enum to
/// itself and could not fail.
///
/// FIX (N29): the descriptor was then built by `enum_type`, the `lib.Enum`
/// FACTORY model, which assigns each key its INDEX in the key list. That made
/// the check `header <-> position in a hand-typed literal`, which still could
/// not observe the Rust enums — transposing two `ActionKey` variants left all
/// 27 tests and all seven binary-interface gates green. The values now come
/// from the enum.
fn run_henum(header_enum: &str, prefix: &str, t: &EnumType) {
    run_henum_src(HEADER_SRC, header_enum, prefix, t).unwrap();
}

fn run_henum_src(src: &str, header_enum: &str, prefix: &str, t: &EnumType) -> Result<(), String> {
    let decls = header_decls(src, header_enum);
    check_ghostty_henum(t, prefix, |name| decls.get(name).copied())
}

/// `("zig field name", RustEnum::Variant)` -> `("zig field name", <value>)`.
///
/// The variant is named, so the value is READ OUT OF THE ENUM rather than
/// re-derived from the list's order. A reordered enum, a variant given an
/// explicit discriminant, or an inserted variant all change what this expands
/// to, and the header comparison then fails.
///
/// FIX (N36): the KEY LIST was hand-typed, so the check was blind in the other
/// direction — a variant ADDED to the Rust enum with no `RUSTTY_*` enumerator
/// left all 17 tests green, where the reference's
/// `inline for (@typeInfo(T).@"enum".fields)` fails. Expanding through
/// `rustty_lib::henum_type!` emits an exhaustive `match` over the enum
/// beside the list, so that is now a COMPILE error. (It also observes the tag
/// type and binds each key to its variant — task 19 fix round 1.)
macro_rules! henum {
    ($header:literal, $prefix:literal, $E:ident, [ $( ($key:literal, $V:ident) ),* $(,)? ]) => {
        run_henum(
            $header,
            $prefix,
            &rustty_lib::henum_type!($E, [ $( ($key, $V) ),* ]),
        )
    };
}

/// Self-check that the probe above can actually FAIL — i.e. that it reads the
/// header and not the descriptor. Each mutation is one of the drifts
/// `checkGhosttyHEnum` exists to catch.
///
/// PORT NOTE: no reference analogue; the oracle gets this for free because
/// `@import("ghostty.h")` is the real header.
#[test]
fn ghostty_h_probe_can_fail() {
    let fields = enum_type_values(&[
        ("app", TargetKey::App as i64),
        ("surface", TargetKey::Surface as i64),
    ]);
    let run = |src: &str| run_henum_src(src, "rustty_target_tag_e", "RUSTTY_TARGET_", &fields);

    // Baseline: the real header passes.
    run(HEADER_SRC).unwrap();

    // A dropped declaration leaves an enum value unmatched.
    let dropped = HEADER_SRC.replace("  RUSTTY_TARGET_SURFACE,\n", "");
    assert_ne!(dropped, HEADER_SRC);
    assert!(run(&dropped).is_err(), "dropped enumerator must fail");

    // A renamed declaration is equally a drop, from the check's view.
    let renamed = HEADER_SRC.replace("RUSTTY_TARGET_SURFACE", "RUSTTY_TARGET_SURFACES");
    assert_ne!(renamed, HEADER_SRC);
    assert!(run(&renamed).is_err(), "renamed enumerator must fail");

    // A revalued declaration mismatches the field's backing int.
    let revalued = HEADER_SRC.replace("  RUSTTY_TARGET_APP,\n", "  RUSTTY_TARGET_APP = 7,\n");
    assert_ne!(revalued, HEADER_SRC);
    assert!(run(&revalued).is_err(), "revalued enumerator must fail");
}

// test "ghostty.h Target.Key" {
//     try lib.checkGhosttyHEnum(Key, "GHOSTTY_TARGET_");
// }
#[test]
fn ghostty_h_target_key() {
    let _ = (TargetKey::App, TargetKey::Surface);
    henum!(
        "rustty_target_tag_e",
        "RUSTTY_TARGET_",
        TargetKey,
        [("app", App), ("surface", Surface),]
    );
}

// test "ghostty.h Action.Key" {
//     try lib.checkGhosttyHEnum(Key, "GHOSTTY_ACTION_");
// }
#[test]
fn ghostty_h_action_key() {
    let _ = ActionKey::Quit;
    henum!(
        "rustty_action_tag_e",
        "RUSTTY_ACTION_",
        ActionKey,
        [
            ("quit", Quit),
            ("new_window", NewWindow),
            ("new_tab", NewTab),
            ("close_tab", CloseTab),
            ("new_split", NewSplit),
            ("close_all_windows", CloseAllWindows),
            ("toggle_maximize", ToggleMaximize),
            ("toggle_fullscreen", ToggleFullscreen),
            ("toggle_tab_overview", ToggleTabOverview),
            ("toggle_window_decorations", ToggleWindowDecorations),
            ("toggle_quick_terminal", ToggleQuickTerminal),
            ("toggle_command_palette", ToggleCommandPalette),
            ("toggle_visibility", ToggleVisibility),
            ("toggle_background_opacity", ToggleBackgroundOpacity),
            ("move_tab", MoveTab),
            ("goto_tab", GotoTab),
            ("goto_split", GotoSplit),
            ("goto_window", GotoWindow),
            ("resize_split", ResizeSplit),
            ("equalize_splits", EqualizeSplits),
            ("toggle_split_zoom", ToggleSplitZoom),
            ("present_terminal", PresentTerminal),
            ("size_limit", SizeLimit),
            ("reset_window_size", ResetWindowSize),
            ("initial_size", InitialSize),
            ("cell_size", CellSize),
            ("scrollbar", Scrollbar),
            ("render", Render),
            ("inspector", Inspector),
            ("show_gtk_inspector", ShowGtkInspector),
            ("render_inspector", RenderInspector),
            ("export_terminal_io", ExportTerminalIO),
            ("desktop_notification", DesktopNotification),
            ("set_title", SetTitle),
            ("set_tab_title", SetTabTitle),
            ("set_window_title", SetWindowTitle),
            ("prompt_title", PromptTitle),
            ("pwd", Pwd),
            ("mouse_shape", MouseShape),
            ("mouse_visibility", MouseVisibility),
            ("mouse_over_link", MouseOverLink),
            ("renderer_health", RendererHealth),
            ("open_config", OpenConfig),
            ("quit_timer", QuitTimer),
            ("float_window", FloatWindow),
            ("secure_input", SecureInput),
            ("key_sequence", KeySequence),
            ("key_table", KeyTable),
            ("color_change", ColorChange),
            ("reload_config", ReloadConfig),
            ("config_change", ConfigChange),
            ("close_window", CloseWindow),
            ("ring_bell", RingBell),
            ("selection_changed", SelectionChanged),
            ("undo", Undo),
            ("redo", Redo),
            ("check_for_updates", CheckForUpdates),
            ("open_url", OpenUrl),
            ("show_child_exited", ShowChildExited),
            ("progress_report", ProgressReport),
            ("show_on_screen_keyboard", ShowOnScreenKeyboard),
            ("command_finished", CommandFinished),
            ("start_search", StartSearch),
            ("end_search", EndSearch),
            ("search_total", SearchTotal),
            ("search_selected", SearchSelected),
            ("readonly", Readonly),
            ("copy_title_to_clipboard", CopyTitleToClipboard),
            ("move_tab_to_new_window", MoveTabToNewWindow),
        ]
    );
}

// test "ghostty.h SplitDirection" {
//     try lib.checkGhosttyHEnum(SplitDirection, "GHOSTTY_SPLIT_DIRECTION_");
// }
#[test]
fn ghostty_h_split_direction() {
    let _ = SplitDirection::Right;
    henum!(
        "rustty_action_split_direction_e",
        "RUSTTY_SPLIT_DIRECTION_",
        SplitDirection,
        [("right", Right), ("down", Down), ("left", Left), ("up", Up),]
    );
}

// test "ghostty.h GotoSplit" {
//     try lib.checkGhosttyHEnum(GotoSplit, "GHOSTTY_GOTO_SPLIT_");
// }
#[test]
fn ghostty_h_goto_split() {
    let _ = GotoSplit::Previous;
    henum!(
        "rustty_action_goto_split_e",
        "RUSTTY_GOTO_SPLIT_",
        GotoSplit,
        [
            ("previous", Previous),
            ("next", Next),
            ("up", Up),
            ("left", Left),
            ("down", Down),
            ("right", Right),
        ]
    );
}

// test "ghostty.h GotoWindow" {
//     try lib.checkGhosttyHEnum(GotoWindow, "GHOSTTY_GOTO_WINDOW_");
// }
#[test]
fn ghostty_h_goto_window() {
    let _ = GotoWindow::Previous;
    henum!(
        "rustty_action_goto_window_e",
        "RUSTTY_GOTO_WINDOW_",
        GotoWindow,
        [("previous", Previous), ("next", Next),]
    );
}

// test "ghostty.h ResizeSplit.Direction" {
//     try lib.checkGhosttyHEnum(Direction, "GHOSTTY_RESIZE_SPLIT_");
// }
#[test]
fn ghostty_h_resize_split_direction() {
    let _ = ResizeSplitDirection::Up;
    henum!(
        "rustty_action_resize_split_direction_e",
        "RUSTTY_RESIZE_SPLIT_",
        ResizeSplitDirection,
        [("up", Up), ("down", Down), ("left", Left), ("right", Right),]
    );
}

// test "ghostty.h Fullscreen" {
//     try lib.checkGhosttyHEnum(Fullscreen, "GHOSTTY_FULLSCREEN_");
// }
#[test]
fn ghostty_h_fullscreen() {
    let _ = Fullscreen::Native;
    henum!(
        "rustty_action_fullscreen_e",
        "RUSTTY_FULLSCREEN_",
        Fullscreen,
        [
            ("native", Native),
            ("macos_non_native", MacosNonNative),
            ("macos_non_native_visible_menu", MacosNonNativeVisibleMenu),
            ("macos_non_native_padded_notch", MacosNonNativePaddedNotch),
        ]
    );
}

// test "ghostty.h FloatWindow" {
//     try lib.checkGhosttyHEnum(FloatWindow, "GHOSTTY_FLOAT_WINDOW_");
// }
#[test]
fn ghostty_h_float_window() {
    let _ = FloatWindow::On;
    henum!(
        "rustty_action_float_window_e",
        "RUSTTY_FLOAT_WINDOW_",
        FloatWindow,
        [("on", On), ("off", Off), ("toggle", Toggle),]
    );
}

// test "ghostty.h SecureInput" {
//     try lib.checkGhosttyHEnum(SecureInput, "GHOSTTY_SECURE_INPUT_");
// }
#[test]
fn ghostty_h_secure_input() {
    let _ = SecureInput::On;
    henum!(
        "rustty_action_secure_input_e",
        "RUSTTY_SECURE_INPUT_",
        SecureInput,
        [("on", On), ("off", Off), ("toggle", Toggle),]
    );
}

// test "ghostty.h Inspector" {
//     try lib.checkGhosttyHEnum(Inspector, "GHOSTTY_INSPECTOR_");
// }
#[test]
fn ghostty_h_inspector() {
    let _ = Inspector::Toggle;
    henum!(
        "rustty_action_inspector_e",
        "RUSTTY_INSPECTOR_",
        Inspector,
        [("toggle", Toggle), ("show", Show), ("hide", Hide),]
    );
}

// test "ghostty.h QuitTimer" {
//     try lib.checkGhosttyHEnum(QuitTimer, "GHOSTTY_QUIT_TIMER_");
// }
#[test]
fn ghostty_h_quit_timer() {
    let _ = QuitTimer::Start;
    henum!(
        "rustty_action_quit_timer_e",
        "RUSTTY_QUIT_TIMER_",
        QuitTimer,
        [("start", Start), ("stop", Stop),]
    );
}

// test "ghostty.h Readonly" {
//     try lib.checkGhosttyHEnum(Readonly, "GHOSTTY_READONLY_");
// }
#[test]
fn ghostty_h_readonly() {
    let _ = Readonly::Off;
    henum!(
        "rustty_action_readonly_e",
        "RUSTTY_READONLY_",
        Readonly,
        [("off", Off), ("on", On),]
    );
}

// test "ghostty.h MouseVisibility" {
//     try lib.checkGhosttyHEnum(MouseVisibility, "GHOSTTY_MOUSE_");
// }
#[test]
fn ghostty_h_mouse_visibility() {
    let _ = MouseVisibility::Visible;
    henum!(
        "rustty_action_mouse_visibility_e",
        "RUSTTY_MOUSE_",
        MouseVisibility,
        [("visible", Visible), ("hidden", Hidden),]
    );
}

// test "ghostty.h PromptTitle" {
//     try lib.checkGhosttyHEnum(PromptTitle, "GHOSTTY_PROMPT_TITLE_");
// }
#[test]
fn ghostty_h_prompt_title() {
    let _ = PromptTitle::Surface;
    henum!(
        "rustty_action_prompt_title_e",
        "RUSTTY_PROMPT_TITLE_",
        PromptTitle,
        [("surface", Surface), ("tab", Tab), ("window", Window),]
    );
}

// test "ghostty.h OpenUrl.Kind" {
//     try lib.checkGhosttyHEnum(Kind, "GHOSTTY_ACTION_OPEN_URL_KIND_");
// }
#[test]
fn ghostty_h_open_url_kind() {
    let _ = OpenUrlKind::Unknown;
    henum!(
        "rustty_action_open_url_kind_e",
        "RUSTTY_ACTION_OPEN_URL_KIND_",
        OpenUrlKind,
        [
            ("unknown", Unknown),
            ("text", Text),
            ("html", Html),
            ("osc8", Osc8),
        ]
    );
}

// test "ghostty.h CloseTabMode" {
//     try lib.checkGhosttyHEnum(CloseTabMode, "GHOSTTY_ACTION_CLOSE_TAB_MODE_");
// }
#[test]
fn ghostty_h_close_tab_mode() {
    let _ = CloseTabMode::This;
    henum!(
        "rustty_action_close_tab_mode_e",
        "RUSTTY_ACTION_CLOSE_TAB_MODE_",
        CloseTabMode,
        [("this", This), ("other", Other), ("right", Right),]
    );
}

// test "ghostty.h OpenConfig" {
//     try lib.checkGhosttyHEnum(OpenConfig, "GHOSTTY_ACTION_OPEN_CONFIG_");
// }
#[test]
fn ghostty_h_open_config() {
    let _ = OpenConfig::OsOpen;
    henum!(
        "rustty_action_open_config_e",
        "RUSTTY_ACTION_OPEN_CONFIG_",
        OpenConfig,
        [("os_open", OsOpen), ("new_window", NewWindow),]
    );
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
        ExportTerminalIO, ExportTerminalIOC, FloatWindow, Fullscreen, GotoSplit, GotoTab,
        GotoWindow, InitialSize, Inspector, KeySequence, KeySequenceC, KeyTable, KeyTableActivateC,
        KeyTableC, KeyTableCValue, KeyTableTag, MouseOverLink, MouseOverLinkC, MouseVisibility,
        MoveTab, OpenConfig, OpenUrl, OpenUrlC, OpenUrlKind, PromptTitle, Pwd, PwdC, Readonly,
        ReloadConfig, ResizeSplit, ResizeSplitDirection, SearchSelected, SearchSelectedC,
        SearchTotal, SearchTotalC, SecureInput, SetTitle, SetTitleC, SizeLimit, SplitDirection,
        StartSearch, StartSearchC, Target, TargetC, TargetCValue, TargetKey,
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
    let _ = Action::OpenConfig(OpenConfig::NewWindow).cval();
    assert_eq!(
        Action::MoveTabToNewWindow.key(),
        ActionKey::MoveTabToNewWindow
    );
    let _ = Action::MoveTabToNewWindow.cval();

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
    let etio = ExportTerminalIO {
        contents: b"Seq\tKind\tDescription\n",
    };
    let etio_c: ExportTerminalIOC = etio.cval();
    assert_eq!(etio_c.len, etio.contents.len());
    let _ = format!("{etio}");
    let _ = Action::ExportTerminalIO(etio).cval();
    let _ = Action::SetWindowTitle(SetTitle { title: b"title" }).cval();

    // S45-F2: the reference `format`s print `@typeName(@This())`, the qualified
    // `apprt.action.<Type>`, in front of each payload (`action.zig:645-654,
    // 754-760, 778-784, 806-817`).
    assert_eq!(format!("{st}"), "apprt.action.SetTitle{ title }");
    assert_eq!(format!("{pwd}"), "apprt.action.Pwd{ /tmp }");
    assert_eq!(
        format!("{dn}"),
        "apprt.action.DesktopNotification{ title: t, body: b }"
    );
    assert_eq!(
        format!("{etio}"),
        "apprt.action.ExportTerminalIO{ contents: 21 bytes }"
    );

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
