//! Tests for `class::surface` (the GTK `Surface` widget).

use super::*;
use crate::gtk::gresource;

/// S87-F4: the `font-size-request` boxed type is registered under the
/// reference's name, `GhosttyFontDesiredSize` (`src/font/face.zig:63-68`).
/// Headless: registering a boxed GType needs no display.
#[test]
fn font_size_request_gtype_name() {
    use glib::prelude::StaticType as _;
    assert_eq!(
        FontSizeRequest::static_type().name(),
        "GhosttyFontDesiredSize"
    );
}

/// NEGATIVE CONTROL (`P3-W11 T5` = `P3-W8 T5` Step 3, oracle `5ea45919d`).
///
/// The accumulator arm and the forwarding arm are selected by one
/// predicate; a test of only the accumulator arm passes against the
/// pre-`5ea45919d` code, which ran the accumulator for EVERY `.surface`
/// scroll and dropped every other unit. This asserts the full table,
/// including both new `false` rows (option off with a touchpad; option on
/// with a wheel) that select the forwarding arm.
///
/// Headless: `ScrollUnit` is a plain `gdk4` enum, so no display or
/// `gtk4::init()` is involved (gate `G-GTKINIT`).
#[test]
fn horizontal_tab_scroll_predicate() {
    use gdk4::ScrollUnit;

    // The only combination that switches tabs.
    assert!(Surface::horizontal_tab_scroll_applies(
        true,
        ScrollUnit::Surface
    ));

    // Option OFF with a touchpad: NEW behaviour — forwarded, not swallowed.
    assert!(!Surface::horizontal_tab_scroll_applies(
        false,
        ScrollUnit::Surface
    ));

    // A mouse wheel: forwarded regardless of the option. Before
    // `5ea45919d` both of these returned early with `false` and the event
    // never reached the terminal.
    assert!(!Surface::horizontal_tab_scroll_applies(
        true,
        ScrollUnit::Wheel
    ));
    assert!(!Surface::horizontal_tab_scroll_applies(
        false,
        ScrollUnit::Wheel
    ));
}

/// PROVING TEST (W16-GTK-01): the `defaultTermioEnv` baseline is
/// Flatpak-aware (`surface.zig:1596-1599`, oracle `b988efcfe`). Under
/// Flatpak the child env starts EMPTY; otherwise it is the inherited
/// process env. Before the fix the baseline was unconditionally
/// `environ_map()`, so the `is_flatpak = true` arm below failed (the
/// sandbox env leaked to every child).
///
/// Headless: no widget, no display, no live Flatpak sandbox — the
/// predicate is an argument (gate `G-GTKINIT`).
#[test]
fn termio_env_baseline_is_flatpak_aware() {
    let host = Surface::termio_env_baseline(false);
    // The inherited env is non-empty under any test runner (PATH is always
    // set), which is what makes the Flatpak arm below a real assertion and
    // not a vacuous one.
    assert_eq!(host, rustty_global::environ_map());
    assert!(
        !host.is_empty(),
        "off Flatpak the baseline must be the inherited process env",
    );

    let flatpak = Surface::termio_env_baseline(true);
    assert!(
        flatpak.is_empty(),
        "under Flatpak the baseline env must be empty, got {} entries",
        flatpak.len()
    );
}

/// The `#[template]` literal must equal the const-folded resolver
/// (`surface.zig:3600-3604`), so the two never drift.
#[test]
fn template_resource_matches_blueprint() {
    assert_eq!(TEMPLATE_RESOURCE, gresource::blueprint(1, 2, "surface"));
}

/// PORTS: the `Surface` template instantiation (`surface.zig:3608-3620`).
/// Display-dependent (GTK + a registered `GhosttyApplication` default for the
/// config/allocator fallback in `init`); marked `#[ignore]`, run via
/// `cargo test -p rustty-apprt --features gtk -- --ignored`.
///
/// Instantiates a `Surface` and asserts all 13 template children bind
/// (non-null) and that `render_surface` is a `GhosttyRenderSurface`.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
fn surface_template_children_bind() {
    // F2: a silent skip reports `1 passed` for a test that never ran.
    crate::gtk::init_for_display_test("surface_template_children_bind");
    let _ = libadwaita::init();
    crate::gtk::register_resources();

    // A default `GhosttyApplication` must exist for `init`'s
    // `Application::default()` config/allocator fallback.
    let _app = crate::gtk::class::application::Application::build_default_for_test();

    let surface = Surface::new(SurfaceOverrides::default());
    let imp = surface.imp();

    // All 13 template children are bound (non-null).
    assert!(imp.render_surface.is_bound(), "render_surface");
    assert!(imp.url_left.is_bound(), "url_left");
    assert!(imp.url_right.is_bound(), "url_right");
    assert!(imp.child_exited_overlay.is_bound(), "child_exited_overlay");
    assert!(imp.context_menu.is_bound(), "context_menu");
    assert!(imp.error_page.is_bound(), "error_page");
    assert!(imp.progress_bar_overlay.is_bound(), "progress_bar_overlay");
    assert!(imp.resize_overlay.is_bound(), "resize_overlay");
    assert!(imp.search_overlay.is_bound(), "search_overlay");
    assert!(imp.key_state_overlay.is_bound(), "key_state_overlay");
    assert!(imp.terminal_page.is_bound(), "terminal_page");
    assert!(imp.drop_target.is_bound(), "drop_target");
    assert!(imp.im_context.is_bound(), "im_context");

    // `render_surface` is a `GhosttyRenderSurface` (`2f0b65346`).
    let render_surface: &RenderSurface = &imp.render_surface;
    assert_eq!(render_surface.type_(), RenderSurface::static_type());
    assert_eq!(RenderSurface::static_type().name(), "GhosttyRenderSurface");
}

/// F-011 KAT (`surface.zig:1602-1606`): the default termio env keeps the
/// launch-time `LANG` reported by `Application::saved_language`
/// (`application.zig:475-478`) and removes `LANG` only when it was absent at
/// launch. Against the pre-fix hardcoded-`None` accessor the `Some` arm is
/// unreachable — every child env lost `LANG` — and this fails.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
fn default_termio_env_keeps_launch_lang() {
    use rustty_global::EnvMapExt as _;

    // F2: a silent skip reports `1 passed` for a test that never ran.
    crate::gtk::init_for_display_test("surface (display-requiring test)");
    let _ = libadwaita::init();
    crate::gtk::register_resources();

    let app = crate::gtk::class::application::Application::build_default_for_test();
    let surface = Surface::new(SurfaceOverrides::default());

    // `LANG` captured at launch → pinned into the child env.
    app.imp()
        .saved_language
        .replace(Some(std::ffi::CString::new("de_DE.UTF-8").unwrap()));
    let env = surface.default_termio_env();
    assert_eq!(
        env.getv(b"LANG"),
        Some(b"de_DE.UTF-8".as_slice()),
        "the launch-time LANG is kept in the default child env"
    );

    // `LANG` absent at launch → removed from the child env.
    app.imp().saved_language.replace(None);
    let env = surface.default_termio_env();
    assert_eq!(
        env.getv(b"LANG"),
        None,
        "LANG absent at launch is removed from the default child env"
    );
}

/// PORTS: `surface.zig:4122-4127` `test "computeFraction"`. Display-free.
#[test]
fn surface_compute_fraction() {
    assert_eq!(compute_fraction(100), 1.0);
    assert_eq!(compute_fraction(255), 1.0);
    assert_eq!(compute_fraction(0), 0.0);
    assert_eq!(compute_fraction(50), 0.5);
}

/// PORTS: `surface.zig:4419-4448` `test "command and shell integration
/// overrides"` (3650bef4e; 7 `expectEqual` assertions, carried
/// assertion-for-assertion). Display-free — pure over `Config`.
///
/// NEGATIVE-CONTROL NOTE (assessment §5): the gtk modules have no G1b row,
/// so this test is INVISIBLE to the gate — it is named here explicitly as
/// the B5 rider test and runs in the `--features gtk` lane.
#[test]
fn command_and_shell_integration_overrides() {
    // var config = try configpkg.Config.default(testing.allocator);
    // defer config.deinit();
    let mut config = rustty_config::Config::default_config();

    // config.@"shell-integration" = .nushell;
    // try applyCommandOverrides(&config, .{ .shell = "vim" }, null);
    // try testing.expectEqual(.detect, config.@"shell-integration");
    config.shell_integration = ShellIntegration::Nushell;
    apply_command_overrides(&mut config, Some(&Command::Shell(b"vim".to_vec())), None);
    assert_eq!(ShellIntegration::Detect, config.shell_integration);

    // config.@"shell-integration" = .none;
    // try applyCommandOverrides(&config, .{ .shell = "vim" }, null);
    // try testing.expectEqual(.none, config.@"shell-integration");
    config.shell_integration = ShellIntegration::None;
    apply_command_overrides(&mut config, Some(&Command::Shell(b"vim".to_vec())), None);
    assert_eq!(ShellIntegration::None, config.shell_integration);

    // try applyCommandOverrides(&config, .{ .shell = "nu" }, .nushell);
    // try testing.expectEqual(.nushell, config.@"shell-integration");
    apply_command_overrides(
        &mut config,
        Some(&Command::Shell(b"nu".to_vec())),
        Some(ShellIntegration::Nushell),
    );
    assert_eq!(ShellIntegration::Nushell, config.shell_integration);

    // try applyCommandOverrides(&config, .{ .shell = "vim" }, .none);
    // try testing.expectEqual(.none, config.@"shell-integration");
    apply_command_overrides(
        &mut config,
        Some(&Command::Shell(b"vim".to_vec())),
        Some(ShellIntegration::None),
    );
    assert_eq!(ShellIntegration::None, config.shell_integration);

    // config.@"shell-integration" = .nushell;
    // try applyCommandOverrides(&config, null, null);
    // try testing.expectEqual(.nushell, config.@"shell-integration");
    config.shell_integration = ShellIntegration::Nushell;
    apply_command_overrides(&mut config, None, None);
    assert_eq!(ShellIntegration::Nushell, config.shell_integration);

    // config.@"shell-integration" = .none;
    // try applyCommandOverrides(&config, null, .nushell);
    // try testing.expectEqual(.nushell, config.@"shell-integration");
    config.shell_integration = ShellIntegration::None;
    apply_command_overrides(&mut config, None, Some(ShellIntegration::Nushell));
    assert_eq!(ShellIntegration::Nushell, config.shell_integration);

    // try applyCommandOverrides(&config, null, .none);
    // try testing.expectEqual(.none, config.@"shell-integration");
    apply_command_overrides(&mut config, None, Some(ShellIntegration::None));
    assert_eq!(ShellIntegration::None, config.shell_integration);
}

/// **Invariant 9 KAT** — a key event routes through the IM context FIRST, and a
/// simple commit that arrives DURING a keypress (`in_keyevent == .not_composing`,
/// spelled [`ImKeyEvent::True`]) is BUFFERED for the key encoder rather than
/// committed as IME text (`surface.zig:1303,3159-3191`). This is the ordering
/// contract at the heart of `keyEvent`: `filterKeypress` runs before any core
/// `keyCallback`, and its re-entrant `commit` associates the text with the key
/// event. Display-gated for the real `IMMulticontext`; core surface is `None`
/// so the non-keyevent path is a safe no-op.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
fn key_press_routes_through_im_context_first() {
    let surface = display_surface();
    let imp = surface.imp();

    // During a keypress that is NOT composing, a simple IM commit ("a") is
    // buffered for the key encoder — NOT sent to the core surface — so the
    // key event can carry the text alongside the keycode.
    imp.ime.borrow_mut().in_keyevent = ImKeyEvent::True;
    Surface::im_commit(&imp.im_context, b"a", &surface);
    {
        let ime = imp.ime.borrow();
        assert_eq!(
            ime.im_len, 1,
            "commit buffered exactly one byte during keyevent"
        );
        assert_eq!(&ime.im_buf[..1], b"a", "buffered the committed text");
    }

    // Outside a keypress the same commit is NOT buffered (it would be routed
    // to the core surface, which is `None` here, so it is a no-op): the buffer
    // length stays 0.
    imp.ime.borrow_mut().im_len = 0;
    imp.ime.borrow_mut().in_keyevent = ImKeyEvent::False;
    Surface::im_commit(&imp.im_context, b"b", &surface);
    assert_eq!(
        imp.ime.borrow().im_len,
        0,
        "a commit outside a keyevent is not buffered for the encoder"
    );
}

/// UD90 (R13, task R13-D): an input-method commit of exactly 128 bytes
/// during a keypress. The reference's `im_len: u7` cannot hold 128, which its
/// own `str.len > im_buf.len` guard admits, so `@intCast` traps (ReleaseFast:
/// the length becomes 0 and the text is lost). The port buffers all 128
/// bytes; 129 still takes the reference's warn-and-return. Display-gated for
/// the real `IMMulticontext`, as the test above.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
fn a_128_byte_commit_during_a_keypress_is_buffered_whole() {
    let surface = display_surface();
    let imp = surface.imp();

    // 64 two-byte characters: exactly the 128-byte buffer.
    let text = "\u{e9}".repeat(64);
    assert_eq!(128, text.len());
    imp.ime.borrow_mut().in_keyevent = ImKeyEvent::True;
    Surface::im_commit(&imp.im_context, text.as_bytes(), &surface);
    {
        let ime = imp.ime.borrow();
        assert_eq!(128, ime.im_len, "the whole commit is buffered");
        assert_eq!(text.as_bytes(), &ime.im_buf[..128]);
    }

    // One byte more is the reference's own "not enough buffer space" arm.
    imp.ime.borrow_mut().im_len = 0;
    let long = format!("{text}a");
    Surface::im_commit(&imp.im_context, long.as_bytes(), &surface);
    assert_eq!(0, imp.ime.borrow().im_len, "129 bytes are not buffered");
}

/// Sets up GTK + a default `GhosttyApplication` and returns a fresh `Surface`.
/// Returns `None` when no display is available (the caller returns early).
#[cfg(test)]
/// F2: this used to return `None` when GTK could not init, and every caller
/// spelled `let Some(surface) = display_surface() else { return; }` — nine
/// `#[ignore]`d tests that each reported `1 passed` without running. It now
/// returns the `Surface` and FAILS loudly instead.
fn display_surface() -> Surface {
    crate::gtk::init_for_display_test("surface (display_surface)");
    let _ = libadwaita::init();
    crate::gtk::register_resources();
    let _app = crate::gtk::class::application::Application::build_default_for_test();
    Surface::new(SurfaceOverrides::default())
}

// The bytes the last `Surface::paste` call received (recorded before its
// empty-text and no-core returns), so the `drop` tests can assert which
// branch pasted what (N337, UD321).
thread_local! {
    pub(super) static LAST_PASTE: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

/// N337: a dropped `G_TYPE_STRING` whose bytes are not UTF-8 reaches `paste`
/// verbatim, and the handler answers `true` (the reference's `return 1`).
/// Through `value.get::<String>()` the same value made glib-rs panic inside
/// the C callback and abort the process.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib drop_pastes_a_non_utf8 -- --ignored"]
fn drop_pastes_a_non_utf8_string_verbatim() {
    use glib::translate::ToGlibPtrMut;
    let surface = display_surface();
    let target = surface.imp().drop_target.get();
    let raw: &[u8] = b"caf\xe9 \xff\xfe";
    let mut value = glib::Value::from_type(glib::Type::STRING);
    let owned = CString::new(raw).expect("no interior NUL");
    // SAFETY: `value` is a `G_TYPE_STRING` value; `g_value_set_string`
    // copies the NUL-terminated buffer.
    unsafe { glib::gobject_ffi::g_value_set_string(value.to_glib_none_mut().0, owned.as_ptr()) };
    LAST_PASTE.with(|l| *l.borrow_mut() = None);
    let accepted = target.emit_by_name::<bool>(
        "drop",
        &[&glib::value::BoxedValue(value), &10.0f64, &10.0f64],
    );
    assert!(accepted, "the string branch answers true");
    let pasted = LAST_PASTE.with(|l| l.borrow_mut().take());
    assert_eq!(
        pasted.as_deref(),
        Some(raw),
        "the string branch ran and passed the bytes verbatim"
    );
}

/// UD321 (R13): a dropped single `GFile` pastes its shell-escaped path and a
/// newline, as the file-list branch does. The reference builds that text and
/// returns without pasting it, so the drop is accepted and lost.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib drop_pastes_a_single_file -- --ignored"]
fn drop_pastes_a_single_file_path_shell_escaped() {
    let surface = display_surface();
    let target = surface.imp().drop_target.get();
    let file = gio::File::for_path("/tmp/rustty l4b/it's.txt");
    LAST_PASTE.with(|l| *l.borrow_mut() = None);
    let accepted = target.emit_by_name::<bool>(
        "drop",
        &[
            &glib::value::BoxedValue(file.to_value()),
            &10.0f64,
            &10.0f64,
        ],
    );
    assert!(accepted, "the file branch answers true");
    let pasted = LAST_PASTE.with(|l| l.borrow_mut().take());
    assert_eq!(
        pasted.as_deref(),
        Some(&b"/tmp/rustty\\ l4b/it\\'s.txt\n"[..]),
        "the file branch pasted the shell-escaped path and a newline"
    );
}

/// UD295 (R13): `surface_drop` answers `GtkDropTarget::drop`'s `gboolean`.
/// A `()` handler made glib-rs panic inside the C callback and abort the
/// process on every surface drop. A value the handler does not accept (not a
/// surface id) must come back `false`, the rejected-drop answer, without
/// aborting.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib surface_drop_answers -- --ignored"]
fn surface_drop_answers_the_gboolean_drop_signal() {
    let surface = display_surface();
    let target = surface.imp().surface_drop_target.get();
    let accepted = target.emit_by_name::<bool>(
        "drop",
        &[
            &glib::value::BoxedValue("not-a-surface-id".to_value()),
            &10.0f64,
            &10.0f64,
        ],
    );
    assert!(
        !accepted,
        "a value that is not a surface id is not accepted"
    );
}

/// S54-F3: the `mouse-shape` / `focused` pspecs and the `clipboard-write`
/// signal carry the reference's types and defaults (`surface.zig:154-170,
/// 222-240, 480-492`): `mouse-shape` is `GhosttyMouseShape` defaulting to
/// `.text`, `focused` defaults to `false` (the FIELD still starts `true`), and
/// `clipboard-write`'s first parameter is `GhosttyApprtClipboard`.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib surface_pspec -- --ignored"]
fn surface_pspec_types_and_defaults_match_the_reference() {
    let surface = display_surface();

    let shape = surface.find_property("mouse-shape").expect("mouse-shape");
    assert_eq!(shape.value_type(), MouseShapeEnum::static_type());
    assert_eq!(
        shape.default_value().get::<MouseShapeEnum>().ok(),
        Some(MouseShapeEnum::Text)
    );

    let focused = surface.find_property("focused").expect("focused");
    assert_eq!(focused.default_value().get::<bool>().ok(), Some(false));
    assert!(surface.focused(), "the private field still starts true");

    let id = glib::subclass::signal::SignalId::lookup("clipboard-write", Surface::static_type())
        .expect("clipboard-write is registered");
    let params: Vec<glib::Type> = id.query().param_types().iter().map(|t| t.type_()).collect();
    assert_eq!(
        params,
        [ClipboardEnum::static_type(), String::static_type()]
    );
}

/// S56-F3: a non-confirmed clipboard write emits `clipboard-write` with the
/// payload's RAW bytes (`surface.zig:4117-4122`, `text.ptr`); a non-UTF-8
/// OSC 52 payload is not U+FFFD-substituted. The handler reads the argument
/// the way `window.rs`'s does, and must fire exactly once.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib clipboard_write_signal -- --ignored"]
fn clipboard_write_signal_carries_the_raw_payload() {
    let surface = display_surface();
    let seen: std::rc::Rc<RefCell<Vec<Vec<u8>>>> = std::rc::Rc::default();
    let sink = seen.clone();
    surface.connect_local("clipboard-write", false, move |args| {
        sink.borrow_mut()
            .push(raw_string_bytes(&args[2]).expect("a non-NULL text"));
        None
    });

    surface.set_clipboard(
        Clipboard::Standard,
        &[ClipboardContent {
            mime: b"text/plain",
            data: b"caf\xe9",
        }],
        false,
    );
    assert_eq!(*seen.borrow(), [b"caf\xe9".to_vec()]);
}

/// UD172 (R13), the confirmation half of S56-F3: a non-UTF-8 OSC 52 payload
/// that needs confirmation must preview as its U+FFFD-substituted text. The
/// reference inserts the raw bytes into the `GtkTextBuffer`, whose UTF-8
/// precondition fails (a `Gtk-CRITICAL`, measured), leaving the preview empty
/// and a confirm that writes an empty clipboard.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib clipboard_confirmation_preview -- --ignored"]
fn clipboard_confirmation_preview_substitutes_invalid_utf8() {
    let surface = display_surface();
    let window = gtk4::Window::new();
    window.set_child(Some(&surface));

    surface.set_clipboard(
        Clipboard::Standard,
        &[ClipboardContent {
            mime: b"text/plain",
            data: b"caf\xe9",
        }],
        true,
    );
    let dialog = LAST_CLIPBOARD_CONFIRMATION
        .with(|last| last.borrow_mut().take())
        .expect("confirm = true builds a confirmation dialog");
    let buffer = dialog
        .get_clipboard_contents()
        .expect("the dialog carries the preview buffer");
    let text: String = buffer.property("text");
    assert_eq!(text, "caf\u{FFFD}");
    window.destroy();
}

/// S56-F4, updated for `2f0b65346`: `renderSurfaceResize`'s debug line carries
/// the (fractional) GdkSurface scale and the widget's integer scale factor
/// (`surface.zig:3386-3396`); the former `window_scale` is gone upstream.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib render_surface_resize_debug -- --ignored"]
fn render_surface_resize_debug_log_carries_the_surface_scale() {
    let surface = display_surface();
    let widget = surface
        .imp()
        .render_surface
        .upcast_ref::<gtk4::Widget>()
        .clone();

    let cap = crate::log_capture::start();
    surface.render_surface_resize_debug_log(10, 20);
    let unrooted = cap.messages();
    drop(cap);
    assert_eq!(
        unrooted,
        [(
            log::Level::Debug,
            LOG_SCOPE.to_owned(),
            format!(
                "gl resize width=10 height=20 scale={} scale_factor={}",
                scale::widget_surface_scale(&widget),
                widget.scale_factor()
            ),
        )]
    );

    // In a realized window the scale is the GdkSurface's own.
    let window = gtk4::Window::new();
    window.set_child(Some(&surface));
    WidgetExt::realize(&window);
    let gdk_surface = window
        .upcast_ref::<gtk4::Native>()
        .surface()
        .expect("a realized window has a GdkSurface");
    assert!(
        gdk_surface.scale() > 0.0,
        "a real GdkSurface scale is positive"
    );
    assert_eq!(scale::widget_surface_scale(&widget), gdk_surface.scale());

    let cap = crate::log_capture::start();
    surface.render_surface_resize_debug_log(30, 40);
    let rooted = cap.messages();
    drop(cap);
    assert_eq!(
        rooted,
        [(
            log::Level::Debug,
            LOG_SCOPE.to_owned(),
            format!(
                "gl resize width=30 height=40 scale={} scale_factor={}",
                gdk_surface.scale(),
                widget.scale_factor()
            ),
        )]
    );
    window.destroy();
}

/// S55-F4: `propMouseShape`'s switch (`surface.zig:2482-2517`), every arm, the
/// pairs copied from the reference in its arm order. Display-free: the map is
/// a pure function of the typed shape; the property itself is pinned by
/// `surface_pspec_types_and_defaults_match_the_reference`.
#[test]
fn mouse_shape_cursor_names_match_the_reference_switch() {
    let reference: [(MouseShape, &str); 34] = [
        (MouseShape::Default, "default"),
        (MouseShape::Help, "help"),
        (MouseShape::Pointer, "pointer"),
        (MouseShape::ContextMenu, "context-menu"),
        (MouseShape::Progress, "progress"),
        (MouseShape::Wait, "wait"),
        (MouseShape::Cell, "cell"),
        (MouseShape::Crosshair, "crosshair"),
        (MouseShape::Text, "text"),
        (MouseShape::VerticalText, "vertical-text"),
        (MouseShape::Alias, "alias"),
        (MouseShape::Copy, "copy"),
        (MouseShape::NoDrop, "no-drop"),
        (MouseShape::Move, "move"),
        (MouseShape::NotAllowed, "not-allowed"),
        (MouseShape::Grab, "grab"),
        (MouseShape::Grabbing, "grabbing"),
        (MouseShape::AllScroll, "all-scroll"),
        (MouseShape::ColResize, "col-resize"),
        (MouseShape::RowResize, "row-resize"),
        (MouseShape::NResize, "n-resize"),
        (MouseShape::EResize, "e-resize"),
        (MouseShape::SResize, "s-resize"),
        (MouseShape::WResize, "w-resize"),
        (MouseShape::NeResize, "ne-resize"),
        (MouseShape::NwResize, "nw-resize"),
        (MouseShape::SeResize, "se-resize"),
        (MouseShape::SwResize, "sw-resize"),
        (MouseShape::EwResize, "ew-resize"),
        (MouseShape::NsResize, "ns-resize"),
        (MouseShape::NeswResize, "nesw-resize"),
        (MouseShape::NwseResize, "nwse-resize"),
        (MouseShape::ZoomIn, "zoom-in"),
        (MouseShape::ZoomOut, "zoom-out"),
    ];
    for (shape, name) in reference {
        assert_eq!(mouse_shape_cursor_name(shape), name, "{shape:?}");
        // And through the GObject enum the property carries.
        assert_eq!(
            mouse_shape_cursor_name(MouseShape::from(MouseShapeEnum::from(shape))),
            name
        );
    }
}

/// S55-F1: `filterSnapPaths` (`surface.zig:1667-1740`) over a synthetic env.
/// Covers every branch: `SNAP_*` removed, the `SNAP` var itself removed (its own
/// value is a snap path), a colon list partially filtered and re-joined in
/// order, a list filtered to nothing removed, an empty snap var skipped rather
/// than matching everything, and `TERMINFO` / product-prefixed keys untouched
/// even when their value is a snap path. Display-free.
#[test]
fn filter_snap_paths_matches_the_reference() {
    use rustty_global::EnvMap;

    let mut env = EnvMap::default();
    let product_key = format!("{}_RESOURCES_DIR", rustty_identity::ENV_PREFIX);
    for (k, v) in [
        ("SNAP", "/snap/rustty/12"),
        ("SNAP_USER_DATA", "/home/u/snap/rustty/12"),
        ("SNAP_DATA", ""),
        (
            "PATH",
            "/snap/rustty/12/bin:/usr/bin:/home/u/snap/rustty/12/x:/bin",
        ),
        ("LD_LIBRARY_PATH", "/snap/rustty/12/lib"),
        ("HOME", "/home/u"),
        ("TERMINFO", "/snap/rustty/12/share/terminfo"),
        (product_key.as_str(), "/snap/rustty/12/share/rustty"),
    ] {
        env.put(k.as_bytes(), v.as_bytes());
    }

    filter_snap_paths(&mut env);

    let mut got: Vec<(String, String)> = env
        .iter()
        .map(|(k, v)| {
            (
                String::from_utf8(k.clone()).unwrap(),
                String::from_utf8(v.clone()).unwrap(),
            )
        })
        .collect();
    got.sort();
    let mut want = vec![
        ("HOME".to_string(), "/home/u".to_string()),
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        (
            "TERMINFO".to_string(),
            "/snap/rustty/12/share/terminfo".to_string(),
        ),
        (
            product_key.clone(),
            "/snap/rustty/12/share/rustty".to_string(),
        ),
    ];
    want.sort();
    assert_eq!(got, want);
}

/// S54-F1: `setParent` inherits `parent.rt_surface.surface.getPwd()` — the
/// parent WIDGET's `pwd` property (`surface.zig:812`), reached from the core
/// surface's `rt_surface`. A live core surface cannot be built in this tier, so
/// the lookup `set_parent` makes is driven directly with a real widget's pinned
/// apprt surface: the pwd comes back, and a second widget with none gives None.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib parent_widget_pwd -- --ignored"]
fn parent_widget_pwd_reads_the_parent_widget_property() {
    let parent = display_surface();
    let other = Surface::new(SurfaceOverrides::default());
    parent.set_pwd(Some("/srv/inherited"));

    // SAFETY: both `rt()` pointers are the widgets' pinned apprt surfaces, alive
    // for this scope.
    let got = unsafe { parent_widget_pwd(parent.rt().cast_mut()) };
    assert_eq!(got.as_deref(), Some("/srv/inherited"));
    let none = unsafe { parent_widget_pwd(other.rt().cast_mut()) };
    assert_eq!(none, None);
}

/// S55-F2: `setBellRinging` (`surface.zig:2296-2316`). Pins the four things the
/// generated setter lost: the change gate (no `notify::bell-ringing` for a write
/// that does not change the value), the focused guard, the per-call `ringBell`
/// for every `true` even while already ringing, and the freeze that holds back
/// notifications raised by `bell` handlers until the write is done.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib set_bell_ringing -- --ignored"]
fn set_bell_ringing_gates_notify_and_freezes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let surface = display_surface();
    let events: Rc<RefCell<Vec<&'static str>>> = Rc::default();

    let sink = events.clone();
    surface.connect_notify_local(Some("bell-ringing"), move |_, _| {
        sink.borrow_mut().push("notify::bell-ringing");
    });
    let sink = events.clone();
    surface.connect_notify_local(Some("zoom"), move |_, _| {
        sink.borrow_mut().push("notify::zoom");
    });
    let sink = events.clone();
    surface.connect_local("bell", false, move |args| {
        let this = args[0]
            .get::<Surface>()
            .expect("bell is emitted on a Surface");
        // A notification raised from inside a bell handler...
        this.set_zoom(!this.zoom());
        // ...must not be delivered until `setBellRinging` thaws.
        sink.borrow_mut().push("bell handler returned");
        None
    });
    let take = || std::mem::take(&mut *events.borrow_mut());

    // Unchanged value: no notify at all.
    surface.set_bell_ringing_inner(false);
    surface.set_bell_ringing_inner(false);
    assert_eq!(take(), Vec::<&str>::new());

    // Focused (the struct default): the bell rings, the state does not change.
    assert!(surface.focused());
    surface.set_bell_ringing_inner(true);
    assert_eq!(take(), ["bell handler returned", "notify::zoom"]);
    assert!(!surface.bell_ringing());

    // Unfocused: one state change, and a second ring notifies nothing new.
    surface.imp().focused.set(false);
    surface.set_bell_ringing_inner(true);
    // Both notifications come after the handler returned; their relative order
    // is GLib's notify-queue dispatch order at thaw, not the port's.
    let mut got = take();
    got[1..].sort_unstable();
    assert_eq!(
        got,
        [
            "bell handler returned",
            "notify::bell-ringing",
            "notify::zoom"
        ]
    );
    surface.set_bell_ringing_inner(true);
    assert_eq!(take(), ["bell handler returned", "notify::zoom"]);
    assert!(surface.bell_ringing());

    surface.set_bell_ringing_inner(false);
    assert_eq!(take(), ["notify::bell-ringing"]);
}

/// S54-F2: `commandFinished`'s bell goes through `setBellRinging(true)`
/// (`surface.zig:1190`), so a second long command finishing while the bell is
/// already ringing rings again but does not re-notify `bell-ringing`.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk,x11,wayland --lib command_finished_bell -- --ignored"]
fn command_finished_bell_goes_through_set_bell_ringing() {
    use std::cell::Cell;
    use std::rc::Rc;

    use rustty_config::value_types::{Duration, NotifyOnCommandFinish};

    let surface = display_surface();
    let mut cfg = rustty_config::Config::default_config();
    cfg.notify_on_command_finish = NotifyOnCommandFinish::Always;
    cfg.notify_on_command_finish_action.bell = true;
    cfg.notify_on_command_finish_action.notify = false;
    cfg.notify_on_command_finish_after = Duration { duration: 0 };
    surface.imp().config.replace(Some(Config::new(&cfg)));
    surface.imp().focused.set(false);

    let bells = Rc::new(Cell::new(0u32));
    let notifies = Rc::new(Cell::new(0u32));
    let b = bells.clone();
    surface.connect_local("bell", false, move |_| {
        b.set(b.get() + 1);
        None
    });
    let n = notifies.clone();
    surface.connect_notify_local(Some("bell-ringing"), move |_, _| n.set(n.get() + 1));

    let finished = rustty_apprt_api::action::CommandFinished {
        exit_code: Some(0),
        duration: Duration {
            duration: 5_000_000_000,
        },
    };
    assert!(surface.command_finished(finished));
    assert!(surface.command_finished(finished));
    assert_eq!(bells.get(), 2, "every finished command rings");
    assert_eq!(notifies.get(), 1, "only the first is a state change");
    assert!(surface.bell_ringing());
}

/// **Invariant 6 / §5.2** — `dispose` is idempotent (the null-guards). Populates
/// an OWNED row (`config`) and a glib source id, calls `dispose()` twice, and
/// asserts no panic/double-free and that every touched field is nulled. Before
/// the T14 free-sites landed, `dispose` left these fields `Some` — RED.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
fn surface_dispose_is_idempotent() {
    let surface = display_surface();
    let imp = surface.imp();

    // `config` is populated by `init`'s app fallback; also install a source id.
    assert!(imp.config.borrow().is_some(), "config seeded by init");
    let id = glib::timeout_add_seconds_local(3600, || glib::ControlFlow::Break);
    imp.progress_bar_timer.replace(Some(id));

    // Dispose twice — the second call must not double-free or panic.
    imp.dispose();
    imp.dispose();

    assert!(imp.config.borrow().is_none(), "config nulled after dispose");
    assert!(
        imp.progress_bar_timer.borrow().is_none(),
        "progress_bar_timer nulled after dispose"
    );
}

/// **Invariant 11** — `dispose` removes every glib source id. Installs all three
/// (`progress_bar_timer`, `idle_rechild`, `pending_horizontal_scroll_reset`) and
/// asserts they are nulled after a single `dispose`. Before T14, `dispose` never
/// touched them — RED.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
fn surface_dispose_removes_all_source_ids() {
    let surface = display_surface();
    let imp = surface.imp();

    imp.progress_bar_timer
        .replace(Some(glib::timeout_add_seconds_local(3600, || {
            glib::ControlFlow::Break
        })));
    imp.idle_rechild
        .replace(Some(glib::timeout_add_seconds_local(3600, || {
            glib::ControlFlow::Break
        })));
    imp.pending_horizontal_scroll_reset
        .replace(Some(glib::timeout_add_seconds_local(3600, || {
            glib::ControlFlow::Break
        })));

    imp.dispose();

    assert!(
        imp.progress_bar_timer.borrow().is_none(),
        "progress_bar_timer"
    );
    assert!(imp.idle_rechild.borrow().is_none(), "idle_rechild");
    assert!(
        imp.pending_horizontal_scroll_reset.borrow().is_none(),
        "pending_horizontal_scroll_reset"
    );
}

/// **Invariant 3** — `core_surface` realize ordering. The core surface is NOT
/// constructed eagerly at widget-construction time: [`Surface::core`] is `None`
/// until the lazy `initSurface` runs on the FIRST render-surface resize
/// (`surface.zig:3247-3257,3400-3403`), never on construction or realize. The
/// live realize → lazy-init drive (which needs the T16 input/render signal
/// handlers, a bound core app, a pty, and a GL context) is exercised
/// end-to-end by T19 first-light; here we pin the structural ordering — nothing
/// builds a core surface before that resize.
///
/// A default `GhosttyApplication` is registered (for `init`'s config fallback);
/// display-gated only for that. Before T15 the field simply didn't exist; this
/// pins that the lifecycle never constructs it eagerly.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
fn surface_core_is_none_before_first_resize() {
    let surface = display_surface();

    // No core surface exists before the first resize (**invariant 3**). The
    // apprt seam agrees: `core()` on the widget is `None`.
    assert!(
        surface.core().is_none(),
        "core None before the first resize (not eagerly constructed)"
    );
}

/// **Invariant 3 — the resize DRIVER.** Emits a real `resize` on the child
/// `RenderSurface`, exercising the template-callback trampoline end to end
/// (`surface.blp` → [`Surface::render_surface_resize`]). This is the regression
/// guard for the argument-order defect: the callback is connected NON-swapped,
/// as in the reference, so GTK's C scope calls it with `(RenderSurface, width,
/// height, Surface)`; a trampoline that read the arguments in another order
/// would cache the wrong size or crash (before this test, under gtk4-rs's
/// scope, any real GTK resize aborted first-light). Here we assert (a) the
/// signal reaches the handler at all and (b) the emitted dimensions are cached,
/// proving width/height arrive in the right slots.
///
/// On first resize with no core surface, `render_surface_resize` falls through to
/// `init_surface` (invariant 3 — lazy on first RESIZE, not realize). This
/// structural harness does not `bind_core_app` (`build_default_for_test`), so
/// `init_surface` bails at the unbound-app guard and `core()` stays `None`; the
/// LIVE construction (bound core app + pty + GL renderer) is T19 first-light,
/// blocked on the renderer-seam un-gating. The point pinned here is that a real
/// resize reaches the lazy-init leg without aborting.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
fn surface_init_on_first_resize() {
    let surface = display_surface();
    let imp = surface.imp();

    assert!(surface.core().is_none(), "no core surface before resize");

    // Drive the real signal through the C-scope trampoline (gtk4-rs's scope
    // SIGABRTed here with a mis-ordered handler).
    let (w, h) = (640_i32, 480_i32);
    imp.render_surface.emit_by_name::<()>("resize", &[&w, &h]);

    // The handler cached exactly what we emitted — width/height came from
    // the signal's width/height arguments, not the emitting RenderSurface.
    assert_eq!(
        imp.size.get(),
        SurfaceSize {
            width: 640,
            height: 480
        },
        "render_surface_resize cached the emitted width/height (correct argument order)"
    );

    // Lazy init was reached but bails (no core app bound in this harness), so
    // the core surface stays None. Live construction is T19.
    assert!(
        surface.core().is_none(),
        "init bails at the unbound-app guard in the structural harness"
    );
    // UD356: the failed initialization shows the error page.
    assert!(surface.error(), "a failed surface init sets `error`");
}

// (`2f0b65346` deleted the GLArea `render` signal and `glareaRender`; the
// former `surface_render_signal_does_not_abort` KAT drove that trampoline.)

/// **T17 apprt-shim forwarders** — the apprt `Surface` (`gtk::surface::Surface`)
/// backref forwards the read accessors to the owning widget: `get_size` returns
/// the widget's CACHED size (`Surface.zig:52-54` → `surface.zig:1578-1586`, the
/// value `render_surface_resize` stamps), and `get_content_scale` / `get_cursor_pos`
/// resolve too. Reads them back through the seam pointer (`self.rt()`), proving
/// the whole `apprt::Surface -> class::Surface` forward and the `constructed`
/// backref wiring (`surface.zig:1793`). Before T15/T17 the shim
/// `unimplemented!()`d every one of these — RED.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
fn surface_shim_read_accessors_forward() {
    use rustty_apprt_api::Surface as _;

    let surface = display_surface();

    // Stamp a known size into the cache (what `render_surface_resize` does), then read it
    // back through the apprt-surface seam pointer.
    let sentinel = SurfaceSize {
        width: 321,
        height: 123,
    };
    surface.imp().size.set(sentinel);
    let cursor = CursorPos { x: 4.0, y: 7.0 };
    surface.imp().cursor_pos.set(cursor);

    // SAFETY: `rt()` points at the widget's pinned embedded apprt surface,
    // whose backref was wired in `constructed`; the widget is alive here.
    let rt: &ApprtSurface = unsafe { &*surface.rt() };

    let size = rt
        .get_size()
        .expect("get_size forwards through the backref");
    assert_eq!(
        size, sentinel,
        "apprt Surface::get_size returns cached size"
    );

    let pos = rt.get_cursor_pos().expect("get_cursor_pos forwards");
    assert_eq!(
        pos, cursor,
        "apprt Surface::get_cursor_pos returns cached pos"
    );

    // The content-scale forwarder resolves (value is display-dependent).
    assert!(rt.get_content_scale().is_ok(), "get_content_scale forwards");

    // The clipboard-support forwarder is `true` for all three variants
    // (`Surface.zig:60-71`).
    assert!(rt.supports_clipboard(Clipboard::Standard));
    assert!(rt.supports_clipboard(Clipboard::Selection));
    assert!(rt.supports_clipboard(Clipboard::Primary));
}

/// **T16 — the render contract** (`2f0b65346`). The app-thread leg the
/// render thread triggers once it has pushed a frame:
///
///   render thread `.redraw` → core `Surface::redraw` →
///   `performAction(.render)` → [`action_render`](crate::gtk::class::application::action_render)
///   → [`ApprtSurface::redraw`] → `render_surface.queue_draw()` → GTK
///   snapshots the `RenderSurface`, which takes the frame.
///
/// This test drives the apprt hop: a `.render` action on a surface target
/// whose opaque `rt_surface` handle points at THIS widget's pinned apprt
/// surface — the shape core's `Surface::redraw` hands `performAction` — and
/// runs every line of `action_render` → `redraw()` → `queue_draw()` on a live
/// widget, on the app/main thread. The live frame delivery is the
/// `gtk_first_light` KAT.
#[test]
#[ignore = "requires a display; run via cargo test -p rustty-apprt --features gtk -- --ignored"]
fn render_contract_chain() {
    use rustty_apprt_api::{CoreSurfaceHandle, Surface as _, Target};

    let surface = display_surface();
    let app_thread = std::thread::current().id();

    // A surface-target handle whose `rt_surface` points at this widget's pinned
    // apprt surface.
    struct FakeCore {
        rt: *mut std::ffi::c_void,
    }
    impl CoreSurfaceHandle for FakeCore {
        fn rt_surface_ptr(&self) -> *mut std::ffi::c_void {
            self.rt
        }
    }
    let handle = FakeCore {
        rt: surface.rt().cast_mut().cast::<std::ffi::c_void>(),
    };

    // The `.app` arm is a no-op (`application.zig:2483`); it must not touch a
    // surface (or panic) with no widget in play.
    crate::gtk::class::application::action_render(Target::<FakeCore>::App);

    // The `.surface` arm reaches through `rt_surface_ptr` to the apprt surface
    // and calls `redraw()` → `render_surface.queue_draw()` (a safe no-op on an
    // unrealized widget).
    crate::gtk::class::application::action_render(Target::surface_ref(&handle));

    // Confirm the handle's `rt_surface` pointer — the one `action_render`
    // dereferenced — resolves to THIS widget.
    let sentinel = SurfaceSize {
        width: 333,
        height: 111,
    };
    surface.imp().size.set(sentinel);
    // SAFETY: `handle.rt` is `surface.rt()`, the widget's pinned embedded apprt
    // surface, alive for this scope.
    let rt: &ApprtSurface = unsafe { &*handle.rt.cast::<ApprtSurface>() };
    assert_eq!(
        rt.get_size().expect("backref resolves"),
        sentinel,
        "action_render's rt_surface handle resolves to this widget"
    );
    assert_eq!(
        std::thread::current().id(),
        app_thread,
        "the redraw leg stayed on the app/main thread"
    );
}

/// **Invariant 5** — `finalize` removes the surface from the app (`deleteSurface`)
/// BEFORE `deinit`ing the core surface (`surface.zig:1925-1940`). Exercises the
/// extracted ordering directly (a live `CoreSurface` cannot be built in this
/// tier); both closures record into a shared log and we assert the order. No
/// display needed. Before the helper existed the test did not compile — RED.
#[test]
fn surface_finalize_deletes_from_app_before_deinit() {
    use std::cell::RefCell;
    let order: RefCell<Vec<&'static str>> = RefCell::new(Vec::new());

    finalize_core_surface(
        true,
        || order.borrow_mut().push("delete"),
        || order.borrow_mut().push("deinit"),
    );
    assert_eq!(*order.borrow(), ["delete", "deinit"]);

    // Absent core surface ⇒ neither runs (the `if (priv.core_surface)` guard).
    order.borrow_mut().clear();
    finalize_core_surface(
        false,
        || order.borrow_mut().push("delete"),
        || order.borrow_mut().push("deinit"),
    );
    assert!(
        order.borrow().is_empty(),
        "no teardown when core surface absent"
    );
}

/// **Cross-thread SOUNDNESS regression — the render-thread panic (invariant 5).**
///
/// Reproduces, DETERMINISTICALLY and without a live GL renderer, the exact
/// teardown hazard the [`AtomicPtr<CoreSurface>`] storage + join-before-clear
/// finalize ordering (this file's imp `Drop`) prevents. A background "renderer
/// thread" hammers a published `core_surface` pointer — the widget [`Surface::core`]
/// cross-thread read is a single atomic load, feeding the apprt seam's non-null
/// assert (`crate::gtk::surface::Surface::core`, `Surface.zig:24-28`). The main
/// thread runs the finalize sequence: the `deinit_and_destroy` closure must STOP +
/// JOIN the reader (as `CoreSurface::deinit` joins the render/io threads) BEFORE
/// clearing the pointer to null.
///
/// The ORIGINAL bug cleared first: `self.core_surface.borrow_mut().take()` (→
/// `None`) and only THEN `boxed.deinit()` (the join) — so the still-running
/// renderer thread saw a cleared pointer and hit
/// `expect("...uninitialized surface")` (the `thread 'renderer' panicked` line).
/// Here the reader records whether it EVER observed null; with the corrected
/// order it never does. Reversing the two lines of the closure below makes this
/// test fail — it is a live regression guard, not a tautology.
#[test]
fn finalize_joins_threads_before_clearing_core_surface() {
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    // A non-null sentinel standing in for the published, heap-stable core surface
    // pointer. NEVER dereferenced — the reader only null-checks it, exactly as the
    // widget `core()` atomic load feeds the apprt seam's non-null assert.
    let sentinel: *mut CoreSurface = std::ptr::NonNull::<CoreSurface>::dangling().as_ptr();
    let core_surface = Arc::new(AtomicPtr::new(sentinel));
    let stop = Arc::new(AtomicBool::new(false));
    let saw_null = Arc::new(AtomicBool::new(false));

    let reader = {
        let core_surface = Arc::clone(&core_surface);
        let stop = Arc::clone(&stop);
        let saw_null = Arc::clone(&saw_null);
        std::thread::spawn(move || {
            // The renderer thread's loop: read `core()` continuously until it is
            // told to stop and joined (mirroring `renderer_thr.join()`).
            while !stop.load(Ordering::Acquire) {
                if core_surface.load(Ordering::Acquire).is_null() {
                    saw_null.store(true, Ordering::Release);
                }
            }
        })
    };

    // Let the reader spin on the published pointer before teardown starts.
    std::thread::sleep(Duration::from_millis(20));

    // The finalize ordering under test (mirrors the imp `Drop` deinit closure):
    // JOIN the reader FIRST, THEN clear the pointer.
    finalize_core_surface(
        !core_surface.load(Ordering::Acquire).is_null(),
        || { /* `deleteSurface`: reads core() (still non-null); no clear */ },
        || {
            // `CoreSurface::deinit`: stop + JOIN the render thread FIRST …
            stop.store(true, Ordering::Release);
            reader.join().unwrap();
            // … and ONLY NOW clear the pointer (`alloc.destroy` +
            // `priv.core_surface = null`, `surface.zig:1937-1939`).
            core_surface.store(std::ptr::null_mut(), Ordering::Release);
        },
    );

    assert!(
        !saw_null.load(Ordering::Acquire),
        "renderer thread observed a cleared core_surface before being joined — \
         the teardown cleared the pointer before joining (invariant 5 violated)"
    );
}
