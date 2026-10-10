//! PORTS: ghostty src/termio/stream_handler.zig — the `test{}` blocks, plus the
//! port's KATs and the shared `StreamHandler` test doubles (`terminal_opts`,
//! `handler_for`) other termio tests use.

use super::*;
use allocator_api2::alloc::Global;

// A no-op `AppMailbox` double: the mode-12 guard returns before any mailbox
// is touched, so accepting every push is enough.
pub(crate) struct NullMailbox;

impl AppMailbox for NullMailbox {
    type Surface = ();

    #[allow(clippy::result_large_err)]
    fn push_or_return(
        &self,
        _msg: rustty_apprt_api::surface::Message<'static>,
        _timeout: Timeout,
    ) -> Result<rustty_datastruct::blocking_queue::Size, rustty_apprt_api::surface::Message<'static>>
    {
        Ok(1)
    }

    fn crash_surface(&self) -> *const dyn rustty_crash::sentry::ThreadStateSurface {
        &crate::TEST_CRASH_SURFACE
    }
}

pub(crate) fn terminal_opts(default_cursor_blink: Option<bool>) -> rustty_vt::terminal::Options {
    rustty_vt::terminal::Options {
        cols: 10,
        rows: 3,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        default_modes: rustty_vt::modes::ModePacked::default(),
        default_cursor_style: rustty_vt::screen::CursorStyle::Block,
        default_cursor_blink,
        default_xt_checksum: Default::default(),
        colors: rustty_vt::terminal::Colors {
            background: color::DynamicRGB::UNSET,
            foreground: color::DynamicRGB::UNSET,
            cursor: color::DynamicRGB::UNSET,
            palette: color::DynamicPalette::default_palette(),
        },
        kitty_image_storage_limit: 0,
        kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
    }
}

// A minimal StreamHandler over a caller-owned Terminal. Every field the
// mode-12 guard does not read is null / empty: the guard returns before the
// renderer state, mailboxes or size are touched.
pub(crate) fn handler_for(
    terminal: *mut Terminal,
    termio_mailbox: *mut Mailbox,
    renderer_state: *mut State,
) -> StreamHandler<NullMailbox> {
    StreamHandler::<NullMailbox> {
        alloc: Global,
        size: std::ptr::null_mut(),
        terminal,
        termio_mailbox,
        surface_mailbox: NullMailbox,
        renderer_state,
        renderer_mailbox: std::ptr::null_mut(),
        renderer_wakeup: xev::Async::init().expect("Async::init"),
        enquiry_response: Vec::new(),
        xt_checksum_report: false,
        osc_color_report_format: OSCColorReportFormat::Bit16,
        clipboard_write: ClipboardAccess::Deny,
        clipboard_write_limit: rustty_vt::kitty::clipboard::max_write_size,
        kitty_clipboard_grants: rustty_vt::kitty::clipboard::Grants::default(),
        kitty_clipboard_write: None,
        apc: rustty_vt::apc::Handler::default(),
        dcs: rustty_vt::dcs::Handler::default(),
        tmux_viewer: None,
        termio_messaged: false,
        seen_title: false,
    }
}

// NEGATIVE CONTROL (P3-W4 T4 Step 4): `c594031d5` MOVES the field that this
// rule reads (`default_cursor_blink`) from the handler onto `Terminal`. A
// mechanical port that re-points it at the wrong thing — or hard-codes it —
// silently breaks the "the user configured an explicit blink, so ignore DEC
// mode 12" rule, and no existing KAT covers it.
#[test]
fn stream_handler_mode_12_ignored_when_default_blink_is_explicit() {
    // (a) An EXPLICIT configured blink: DEC mode 12 must be ignored.
    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    assert!(!term.modes.get(modes::Mode::CursorBlinking));
    let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());
    h.setMode(modes::Mode::CursorBlinking, true).unwrap();
    assert!(
        !term.modes.get(modes::Mode::CursorBlinking),
        "DEC mode 12 must be ignored while an explicit default blink is set"
    );
    h.renderer_wakeup.deinit();
    term.deinit(Global.into());

    // (b) NO configured blink (null = emulator default): mode 12 applies.
    let mut term = Terminal::init(Global.into(), terminal_opts(None)).unwrap();
    // A null default resolves to the emulator default, which is blinking.
    assert!(term.modes.get(modes::Mode::CursorBlinking));
    let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());
    h.setMode(modes::Mode::CursorBlinking, false).unwrap();
    assert!(
        !term.modes.get(modes::Mode::CursorBlinking),
        "DEC mode 12 must apply when no explicit default blink is configured"
    );
    h.renderer_wakeup.deinit();
    term.deinit(Global.into());
}

// ---- task-14e-1: the restored `io_handler` diagnostics -----------------

/// `stream_handler.zig:186` — the `vt`/`vtFallible` funnel. This is the
/// highest-risk shape in the file: the port collapses the reference's
/// COMPTIME `switch (action)` into one trait method per action, so the
/// `action=` half of the message is not a value at the site and had been
/// DROPPED entirely. Each of the 38 callers now passes its own reference
/// `Action.Tag` field name; this pins the rendered form for two of them.
#[test]
fn vt_funnel_emits_the_action_name_and_the_error() {
    let cap = crate::log_capture::start();
    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());

    h.vt("print", Err(crate::Error));
    h.vt("kitty_keyboard_query", Err(crate::Error));
    // The SUCCESS arm must stay silent — the reference only logs in `catch`.
    h.vt("print", Ok(()));

    let messages = cap.messages();
    drop(cap);
    h.renderer_wakeup.deinit();
    term.deinit(Global.into());

    assert_eq!(
        vec![
            (
                log::Level::Warn,
                "io_handler".to_string(),
                "error handling VT action action=print err=Error".to_string(),
            ),
            (
                log::Level::Warn,
                "io_handler".to_string(),
                "error handling VT action action=kitty_keyboard_query err=Error".to_string(),
            ),
        ],
        messages
    );
}

/// task-14e-1 fix round 1: pin the caller→tag mapping for ALL 38 `vt`
/// callers, not just the two the funnel test renders.
///
/// WHY THIS IS STATIC. The regression this guards against is a caller
/// passing the wrong tag, or a new caller passing none. Driving it
/// dynamically would need each trait method's inner call to FAIL, and every
/// one of them is fallible only on `OutOfMemory` (`Terminal::print`,
/// `::index`, `Decrqss::encode` …) — which Rust's global allocator aborts on
/// rather than returning. There is no input that makes them fail, so there
/// is no dynamic drive for 36 of the 38. Reading the mapping out of the
/// source covers all 38 and catches the exact defect; `vt_funnel_…` above
/// pins the RENDERING, and `vt_callers_are_wired_to_the_funnel` below proves
/// the wiring is live rather than dead source text.
///
/// Two independent assertions:
///   1. every `self.vt("<tag>", …)` tag equals its enclosing `fn` name — the
///      port's trait methods are named for the reference's own union fields,
///      so this is what makes a copy-paste caller with a stale tag fail;
///   2. every tag is in the reference's `Action` union
///      (`src/terminal/stream.zig:35-140` at pin 20abdb50a), transcribed
///      below the way this tree's other `@tagName` ports are.
#[test]
fn vt_caller_tags_match_the_reference_action_union() {
    // `pub const Action = union(Key) { … }`, src/terminal/stream.zig:35.
    const REFERENCE_ACTION_TAGS: &[&str] = &[
        "print",
        "print_slice",
        "print_repeat",
        "bell",
        "backspace",
        "horizontal_tab",
        "horizontal_tab_back",
        "linefeed",
        "carriage_return",
        "enquiry",
        "invoke_charset",
        "cursor_up",
        "cursor_down",
        "cursor_left",
        "cursor_right",
        "cursor_col",
        "cursor_row",
        "cursor_col_relative",
        "cursor_row_relative",
        "cursor_pos",
        "cursor_style",
        "erase_display_below",
        "erase_display_above",
        "erase_display_complete",
        "erase_display_scrollback",
        "erase_display_scroll_complete",
        "erase_line_right",
        "erase_line_left",
        "erase_line_complete",
        "erase_line_right_unless_pending_wrap",
        "delete_chars",
        "erase_chars",
        "insert_lines",
        "insert_blanks",
        "delete_lines",
        "scroll_up",
        "scroll_down",
        "tab_clear_current",
        "tab_clear_all",
        "tab_set",
        "tab_reset",
        "index",
        "next_line",
        "reverse_index",
        "full_reset",
        "set_mode",
        "reset_mode",
        "save_mode",
        "restore_mode",
        "request_mode",
        "request_mode_unknown",
        "top_and_bottom_margin",
        "left_and_right_margin",
        "left_and_right_margin_ambiguous",
        "save_cursor",
        "restore_cursor",
        "modify_key_format",
        "mouse_shift_capture",
        "protected_mode_off",
        "protected_mode_iso",
        "protected_mode_dec",
        "size_report",
        "title_push",
        "title_pop",
        "xtversion",
        "device_attributes",
        "device_status",
        "kitty_keyboard_query",
        "kitty_keyboard_push",
        "kitty_keyboard_pop",
        "kitty_keyboard_set",
        "kitty_keyboard_set_or",
        "kitty_keyboard_set_not",
        "dcs_hook",
        "dcs_put",
        "dcs_unhook",
        "apc_start",
        "apc_end",
        "apc_put",
        "apc_put_slice",
        "end_hyperlink",
        "active_status_display",
        "decaln",
        "window_title",
        "report_pwd",
        "show_desktop_notification",
        "progress_report",
        "start_hyperlink",
        "clipboard_contents",
        "mouse_shape",
        "configure_charset",
        "set_attribute",
        "kitty_color_report",
        "color_operation",
        "semantic_prompt",
        "kitty_clipboard",
        "kitty_dnd",
        "resize_window",
        // 7b11f3dca, b1bfd1d9c, bae2c3cdb (oracle a60e9e2a5)
        "osc_unknown",
        "mouse_shape_reset",
        "program_status",
    ];

    let src = include_str!("handler_impl.rs");
    let mut enclosing: Option<&str> = None;
    let mut seen = 0usize;
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix("    fn ") {
            let end = rest.find(['(', '<']).unwrap_or(0);
            enclosing = Some(&rest[..end]);
        }
        let Some(rest) = line.trim_start().strip_prefix("self.vt(\"") else {
            continue;
        };
        let tag = &rest[..rest.find('"').expect("the tag is a string literal")];
        let owner = enclosing.expect("a vt() call outside any fn");
        seen += 1;
        assert_eq!(
            owner, tag,
            "`self.vt({tag:?}, …)` sits in `fn {owner}` — the tag must be the \
             enclosing trait method's own name, because the port names each \
             method for the reference Action field it dispatches"
        );
        assert!(
            REFERENCE_ACTION_TAGS.contains(&tag),
            "{tag:?} is not a field of the reference's `Action` union"
        );
    }
    assert_eq!(
        39, seen,
        "expected 39 `self.vt(…)` callers; a change in the count means an \
         action was added or removed and this pin needs revisiting"
    );
}

/// The wiring half of the test above: the tags are not dead source text.
///
/// Drives two REAL trait methods (`Handler::print` and
/// `Handler::kitty_keyboard_query`, chosen because they take different
/// funnel paths) and asserts (a) each produced its own observable side
/// effect, so the method body really ran and really reached `self.vt(…)`
/// at its tail, and (b) the funnel stayed SILENT on the success arm — the
/// reference only logs in `catch`.
///
/// The expected vector is not empty: `queryKittyKeyboard` emits
/// `stream_handler.zig:909`'s own `log.debug("querying kitty keyboard mode")`
/// unconditionally, which is reference behaviour and is pinned here too. So
/// this asserts exactly one thing fired and it was NOT the funnel — a
/// stronger statement than an empty vector would have been, and the reason
/// this is an `assert_eq!` on the whole vector rather than a count.
#[test]
fn vt_callers_are_wired_to_the_funnel() {
    use rustty_vt::stream::Handler as _;

    let cap = crate::log_capture::start();
    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    let mailbox: *mut Mailbox = Box::into_raw(Box::new(
        crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
    ));
    // `messageWriter`'s slow path unlocks the renderer mutex, so this test
    // needs a real `State` (the visibility-report KAT's shape).
    let mutex: *mut rustty_zigstd::thread::Mutex =
        Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
    let state: *mut State = Box::into_raw(Box::new(State {
        mutex,
        terminal: &raw mut term,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: rustty_renderer::state::Mouse::default(),
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    }));
    let mut h = handler_for(&raw mut term, mailbox, state);

    // `.print => try self.terminal.print(value.cp)`
    h.print(u32::from('Z'));
    // `.kitty_keyboard_query => try self.queryKittyKeyboard()`
    h.kitty_keyboard_query();

    let messages = cap.messages();
    // POSITIVE (a): `print` reached the terminal.
    let cell_printed = unsafe { (*term.screens.active).cursor.x } == 1;
    // POSITIVE (b): `queryKittyKeyboard` pushed its reply into the mailbox.
    let replied = mailbox_pop(mailbox).is_some();
    drop(cap);
    h.renderer_wakeup.deinit();
    term.deinit(Global.into());
    drop(unsafe { Box::from_raw(state) });
    drop(unsafe { Box::from_raw(mutex) });
    drop(unsafe { Box::from_raw(mailbox) });

    assert!(
        cell_printed,
        "Handler::print did not reach Terminal::print, so this test never \
         reached the `self.vt(\"print\", …)` it is exercising"
    );
    assert!(
        replied,
        "Handler::kitty_keyboard_query did not reach the mailbox, so this \
         test never reached its `self.vt(…)`"
    );
    assert_eq!(
        vec![(
            log::Level::Debug,
            "io_handler".to_string(),
            "querying kitty keyboard mode".to_string(),
        )],
        messages,
        "the only record must be queryKittyKeyboard's own reference debug \
         line; anything from the `vt` funnel means a success arm is logging"
    );
}

/// `stream_handler.zig:946` — the over-long window title warn. Drives a real
/// call path rather than the funnel, and pins the exact record vector.
#[test]
fn window_title_warns_when_the_title_exceeds_the_buffer() {
    let cap = crate::log_capture::start();
    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());

    // The reference guards on `title.len >= buf.len` with `buf: [256]u8`.
    let too_long = vec![b'x'; 256];
    h.windowTitle(&too_long).unwrap();

    let messages = cap.messages();
    drop(cap);
    h.renderer_wakeup.deinit();
    term.deinit(Global.into());

    assert_eq!(
        vec![(
            log::Level::Warn,
            "io_handler".to_string(),
            "change title requested larger than our buffer size, ignoring".to_string(),
        )],
        messages
    );
}

/// The ABSENCE arm of the test above, with the positive assertion
/// PROCEDURE.md requires: a title that FITS must produce NO record, and the
/// test must prove it actually reached the code the absence is about rather
/// than bailing out somewhere earlier.
///
/// The positive proof is the terminal's title itself. `windowTitle` sets it
/// on the way past the guard, so a set title is evidence the guard was
/// evaluated and NOT taken — which an assertion on the empty record vector
/// alone cannot distinguish from "the function returned before the guard".
///
/// NEGATIVE CONTROL RUN (task-14e-1): moving the `log::warn!` out of the
/// `if title.len() >= buf.len()` block — the exact "diagnostic on a success
/// path" bug this project shipped once — fails THIS test with
/// `left: [] right: [(Warn, "io_handler", "change title requested ...")]`,
/// and leaves `window_title_warns_when_the_title_exceeds_the_buffer`
/// PASSING. So the control fails here, and it fails for the stated reason.
#[test]
fn window_title_is_silent_when_the_title_fits() {
    let cap = crate::log_capture::start();
    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());

    h.windowTitle(b"a short title").unwrap();

    let messages = cap.messages();
    // POSITIVE: we got past the length guard and through `set_title`.
    let title_after = term.get_title().map(<[u8]>::to_vec);
    drop(cap);
    h.renderer_wakeup.deinit();
    term.deinit(Global.into());

    assert_eq!(
        Some(b"a short title".to_vec()),
        title_after,
        "the title was not set, so this test never reached the guard whose \
         silence it is asserting"
    );
    assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
}

// Pops one message from the SPSC termio mailbox (the queue the handler's
// `messageWriter` pushes into).
fn mailbox_pop(mailbox: *mut Mailbox) -> Option<Message<'static>> {
    match unsafe { &*mailbox } {
        Mailbox::Spsc { queue, .. } => queue.pop(),
    }
}

// NEGATIVE CONTROL (P3-W4 T7 Step 5), termio half. `6c8c07981` adds TWO
// independently droppable termio arms: the mode-2033 ENABLE arm and the
// CSI ? 998 n DSR arm. Both must push a FORCED `VisibilityReport` carrying
// the terminal's current `flags.visible`. Dropping either is invisible to
// every existing termio KAT.
#[test]
fn stream_handler_visibility_report_arms_push_forced_messages() {
    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    term.flags.visible = false;

    let mailbox: *mut Mailbox = Box::into_raw(Box::new(
        crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
    ));
    let mutex: *mut rustty_zigstd::thread::Mutex =
        Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
    let state: *mut State = Box::into_raw(Box::new(State {
        mutex,
        terminal: &raw mut term,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: rustty_renderer::state::Mouse::default(),
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    }));
    let mut h = handler_for(&raw mut term, mailbox, state);

    // (a) Enabling mode 2033 pushes a forced report with the current state.
    h.setMode(modes::Mode::ReportVisibility, true).unwrap();
    match mailbox_pop(mailbox) {
        Some(Message::VisibilityReport { visible, force }) => {
            assert!(!visible, "the report must carry terminal.flags.visible");
            assert!(force, "a mode-2033 enable is a FORCED report");
        }
        other => panic!("expected a VisibilityReport, got {:?}", other.is_some()),
    }

    // (b) DISABLING pushes nothing.
    h.setMode(modes::Mode::ReportVisibility, false).unwrap();
    assert!(
        mailbox_pop(mailbox).is_none(),
        "disabling mode 2033 must not push a report"
    );

    // (c) An explicit CSI ? 998 n query pushes a forced report too.
    term.flags.visible = true;
    h.deviceStatusReport(rustty_vt::device_status::Request::Visibility)
        .unwrap();
    match mailbox_pop(mailbox) {
        Some(Message::VisibilityReport { visible, force }) => {
            assert!(visible);
            assert!(force, "an explicit query is answered unconditionally");
        }
        other => panic!("expected a VisibilityReport, got {:?}", other.is_some()),
    }

    h.renderer_wakeup.deinit();
    unsafe {
        drop(Box::from_raw(state));
        drop(Box::from_raw(mutex));
        drop(Box::from_raw(mailbox));
    }
    term.deinit(Global.into());
}

// P7-W6 T1 (714fe9b90): a program's `CSI 8 ; rows ; cols t` is forwarded
// to the surface as a `resize_window` message, unchanged (the surface
// decides whether it is allowed). Driven through the real vt stream.
#[test]
fn stream_handler_forwards_csi_8_t_as_a_resize_window_surface_message() {
    struct RecordingMailbox(std::sync::Mutex<Vec<stream::ResizeWindow>>);
    impl AppMailbox for RecordingMailbox {
        type Surface = ();
        #[allow(clippy::result_large_err)]
        fn push_or_return(
            &self,
            msg: SurfaceMessage<'static>,
            _timeout: Timeout,
        ) -> Result<rustty_datastruct::blocking_queue::Size, SurfaceMessage<'static>> {
            if let SurfaceMessage::ResizeWindow(v) = msg {
                self.0.lock().unwrap().push(v);
            }
            Ok(1)
        }
        fn crash_surface(&self) -> *const dyn rustty_crash::sentry::ThreadStateSurface {
            &crate::TEST_CRASH_SURFACE
        }
    }

    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    let n = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());
    let h = StreamHandler::<RecordingMailbox> {
        alloc: n.alloc,
        size: n.size,
        terminal: n.terminal,
        termio_mailbox: n.termio_mailbox,
        surface_mailbox: RecordingMailbox(std::sync::Mutex::new(Vec::new())),
        renderer_state: n.renderer_state,
        renderer_mailbox: n.renderer_mailbox,
        renderer_wakeup: n.renderer_wakeup,
        enquiry_response: n.enquiry_response,
        xt_checksum_report: n.xt_checksum_report,
        osc_color_report_format: n.osc_color_report_format,
        clipboard_write: n.clipboard_write,
        clipboard_write_limit: n.clipboard_write_limit,
        kitty_clipboard_grants: n.kitty_clipboard_grants,
        kitty_clipboard_write: n.kitty_clipboard_write,
        apc: n.apc,
        dcs: n.dcs,
        tmux_viewer: n.tmux_viewer,
        termio_messaged: n.termio_messaged,
        seen_title: n.seen_title,
    };
    let mut s = stream::Stream::init(stream::Options::with_handler(h));
    s.next_slice(b"\x1b[8;30;100t\x1b[8;;120t\x1b[8;30;100;1t");
    let got = s.handler.surface_mailbox.0.lock().unwrap().clone();
    s.handler.renderer_wakeup.deinit();
    drop(s);
    term.deinit(Global.into());

    assert_eq!(
        vec![
            stream::ResizeWindow {
                rows: 30,
                columns: 100
            },
            stream::ResizeWindow {
                rows: 0,
                columns: 120
            },
        ],
        got,
        "each valid CSI 8 t is forwarded once, as parsed; the one with extra \
         parameters is not"
    );
}

// The `changeConfig` half of the same move: the handler no longer owns the
// defaults, so a config change must reach `Terminal`'s setters (and must
// apply immediately only while the cursor still follows its defaults).
#[test]
fn stream_handler_change_config_routes_cursor_defaults_to_the_terminal() {
    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    // `changeConfig` ends with a `messageWriter` (the mode-2031 color-scheme
    // report), so this handler needs a real termio mailbox.
    let mailbox: *mut Mailbox = Box::into_raw(Box::new(
        crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
    ));
    let mutex: *mut rustty_zigstd::thread::Mutex =
        Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
    let state: *mut State = Box::into_raw(Box::new(State {
        mutex,
        terminal: &raw mut term,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: rustty_renderer::state::Mouse::default(),
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    }));
    let mut h = handler_for(&raw mut term, mailbox, state);

    let mut cfg = DerivedConfig::init(&rustty_config::Config::default());
    cfg.cursor_style = rustty_vt::screen::CursorStyle::Underline;
    cfg.cursor_blink = Some(true);
    h.changeConfig(&mut cfg);

    assert_eq!(
        rustty_vt::screen::CursorStyle::Underline,
        term.cursor.default_style
    );
    assert_eq!(Some(true), term.cursor.default_blink);
    assert!(term.cursor.is_default);
    assert_eq!(rustty_vt::screen::CursorStyle::Underline, unsafe {
        (*term.screens.active).cursor.cursor_style
    });
    assert!(term.modes.get(modes::Mode::CursorBlinking));

    h.renderer_wakeup.deinit();
    unsafe {
        drop(Box::from_raw(state));
        drop(Box::from_raw(mutex));
        drop(Box::from_raw(mailbox));
    }
    term.deinit(Global.into());
}

// The bytes of a `write_small`/`write_alloc` termio message.
fn write_bytes(msg: &Message<'static>) -> Vec<u8> {
    match msg {
        Message::WriteSmall(v) => v.data[0..v.len].to_vec(),
        Message::WriteStable(v) => v.to_vec(),
        Message::WriteAlloc(v) => v.data.to_vec(),
        _ => panic!("expected a write message"),
    }
}

/// S214-F1 / UD205 (no reference counterpart): an OSC 4 that queries all
/// 256 palette entries and then sets one gets every report, in one write,
/// and the set after the reports is applied. The reference's response
/// buffer holds 25 16-bit reports for indices 0.. (640 bytes with BEL,
/// 665 with ST) and fails on the 26th (measured, zig 0.16.0 aarch64-macos;
/// 24th to 27th depending on the indices' digit count), sends nothing,
/// and skips the set.
#[test]
fn color_operation_reports_every_query_and_applies_later_requests() {
    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    let mailbox: *mut Mailbox = Box::into_raw(Box::new(
        crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
    ));
    let mutex: *mut rustty_zigstd::thread::Mutex =
        Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
    let state: *mut State = Box::into_raw(Box::new(State {
        mutex,
        terminal: &raw mut term,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: rustty_renderer::state::Mouse::default(),
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    }));
    let mut h = handler_for(&raw mut term, mailbox, state);

    let mut requests = osc::color::List::new_in(rustty_vt::alloc::Global);
    for i in 0..=255u8 {
        requests
            .append(osc::color::Request::Query(osc::color::Target::Palette(i)))
            .unwrap();
    }
    let new_color = color::RGB {
        r: 0x12,
        g: 0x34,
        b: 0x56,
    };
    requests
        .append(osc::color::Request::Set(osc::color::ColoredTarget {
            target: osc::color::Target::Palette(7),
            color: new_color,
        }))
        .unwrap();

    let mut expected: Vec<u8> = Vec::new();
    for i in 0..=255u8 {
        let c = term.colors.palette.current[usize::from(i)];
        expected.extend_from_slice(
            format!(
                "\x1b]4;{};rgb:{:0>4x}/{:0>4x}/{:0>4x}",
                i,
                u16::from(c.r) * 257,
                u16::from(c.g) * 257,
                u16::from(c.b) * 257,
            )
            .as_bytes(),
        );
        expected.extend_from_slice(b"\x1b\\");
    }
    assert!(
        expected.len() > 1024,
        "the case exceeds the reference's buffer"
    );

    h.colorOperation(osc::color::Operation::osc_4, &requests, osc::Terminator::St)
        .expect("colorOperation");

    let msg = mailbox_pop(mailbox).expect("one report message");
    assert_eq!(write_bytes(&msg), expected, "all 256 reports, in order");
    assert!(mailbox_pop(mailbox).is_none(), "exactly one message");
    assert_eq!(
        term.colors.palette.current[7], new_color,
        "the set after the reports is applied"
    );

    h.renderer_wakeup.deinit();
    unsafe {
        drop(Box::from_raw(state));
        drop(Box::from_raw(mutex));
        drop(Box::from_raw(mailbox));
    }
    term.deinit(Global.into());
}

// `dcsCommand`'s tmux control-mode arm (`fix-freetype-tmux` lane). This arm
// was `Command::Tmux(_) => {}` — a silent no-op — while `TMUX_ENABLED` is
// TRUE in the shipped binary, so entering tmux control mode built no viewer,
// sent tmux no commands, and never tore the viewer down.
//
// Drives the reference's three prongs end to end (`stream_handler.zig:384`):
//   `.enter` -> allocate the viewer
//   fallthrough -> `viewer.next(.{ .tmux = tmux })`, and every `.command`
//                  action goes out the termio mailbox via `messageWriter`
//   `.exit`  -> tear the viewer down
//
// Runs in BOTH build configurations, asserting the reference's behaviour in
// each: with `tmux_control_mode` OFF the whole block is `break :tmux` — a
// no-op — and with it ON (the shipped binary; `cargo test -p rustty-termio
// --features oniguruma` here) the three prongs above must all fire.
#[test]
fn stream_handler_dcs_tmux_arm_drives_the_viewer() {
    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    let mailbox: *mut Mailbox = Box::into_raw(Box::new(
        crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
    ));
    let mutex: *mut rustty_zigstd::thread::Mutex =
        Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
    let state: *mut State = Box::into_raw(Box::new(State {
        mutex,
        terminal: &raw mut term,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: rustty_renderer::state::Mouse::default(),
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    }));
    let mut h = handler_for(&raw mut term, mailbox, state);

    let feed = |h: &mut StreamHandler<NullMailbox>, n: rustty_vt::tmux::ControlNotification| {
        let mut cmd: rustty_vt::dcs::Command = rustty_vt::dcs::Command::Tmux(n);
        h.dcsCommand(&mut cmd).expect("dcsCommand");
    };

    // (a) `.enter` allocates the viewer.
    assert!(h.tmux_viewer.is_none());
    feed(&mut h, rustty_vt::tmux::ControlNotification::Enter);

    if !TMUX_ENABLED {
        // `if (comptime !tmux_enabled) break :tmux;` — the entire block is
        // inert in a build without the option, so nothing was allocated.
        assert!(
            h.tmux_viewer.is_none(),
            "with tmux_control_mode off the arm must be a no-op"
        );
        assert!(mailbox_pop(mailbox).is_none());

        h.deinit();
        h.renderer_wakeup.deinit();
        unsafe {
            drop(Box::from_raw(state));
            drop(Box::from_raw(mutex));
            drop(Box::from_raw(mailbox));
        }
        term.deinit(Global.into());
        return;
    }

    assert!(
        h.tmux_viewer.is_some(),
        "the .enter prong must build the tmux viewer"
    );
    assert!(
        mailbox_pop(mailbox).is_none(),
        ".enter emits no tmux command"
    );

    // (b) The fallthrough prong routes the notification into the viewer.
    // A block end moves the viewer startup_block -> startup_session and
    // yields no action.
    feed(
        &mut h,
        rustty_vt::tmux::ControlNotification::BlockEnd(Vec::new()),
    );
    assert!(
        mailbox_pop(mailbox).is_none(),
        "the startup block end yields no action"
    );

    // ...and the session-changed notification makes the viewer enter its
    // command queue, whose first command MUST reach the termio mailbox.
    feed(
        &mut h,
        rustty_vt::tmux::ControlNotification::SessionChanged {
            id: 1,
            name: b"rustty".to_vec(),
        },
    );
    let msg =
        mailbox_pop(mailbox).expect("the viewer's first queued command must be written to tmux");
    let bytes = write_bytes(&msg);
    assert!(!bytes.is_empty());
    assert_eq!(
        b'\n',
        bytes[bytes.len() - 1],
        "the reference asserts the command carries its trailing newline"
    );
    assert!(
        h.termio_messaged,
        "messageWriter must mark the termio thread as needing a wakeup"
    );

    // (c) `.exit` tears the viewer down.
    feed(&mut h, rustty_vt::tmux::ControlNotification::Exit);
    assert!(
        h.tmux_viewer.is_none(),
        "the .exit prong must free the viewer"
    );

    h.deinit();
    h.renderer_wakeup.deinit();
    unsafe {
        drop(Box::from_raw(state));
        drop(Box::from_raw(mutex));
        drop(Box::from_raw(mailbox));
    }
    term.deinit(Global.into());
}

// R13 (task R13-A, UPSTREAM-DEFECTS UD6): a tmux control-mode block larger
// than the parser's 1 MiB limit. The reference's `dcs.Handler.put` catches
// the parser's `OutOfMemory`, discards the state and returns null, so no
// `.exit` reaches `dcsCommand` and the viewer is never freed; the next
// `ESC P 1000 p` then trips `assert(self.tmux_viewer == null)`
// (`stream_handler.zig:410`). These two tests drive the real bytes through
// the VT stream into this handler.
//
// A real tmux reaches it: `tmux -CC attach` to a pane with more than 1 MiB
// of history, detach, then attach again in the same surface.
#[cfg(feature = "oniguruma")]
fn tmux_stream_fixture() -> (
    Box<Terminal>,
    *mut Mailbox,
    *mut rustty_zigstd::thread::Mutex,
    *mut State,
) {
    let mut term = Box::new(Terminal::init(Global, terminal_opts(Some(false))).unwrap());
    let mailbox: *mut Mailbox = Box::into_raw(Box::new(
        crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
    ));
    let mutex: *mut rustty_zigstd::thread::Mutex =
        Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
    let state: *mut State = Box::into_raw(Box::new(State {
        mutex,
        terminal: &raw mut *term,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: rustty_renderer::state::Mouse::default(),
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    }));
    (term, mailbox, mutex, state)
}

// A tmux control-mode block whose payload is one byte over the parser's
// 1 MiB buffer limit, including the `ESC \` that ends the DCS.
#[cfg(feature = "oniguruma")]
fn oversized_tmux_block() -> Vec<u8> {
    let mut bytes = b"\x1bP1000p%begin 1 1 1\n".to_vec();
    let mut line = vec![b'a'; 79];
    line.push(b'\n');
    let payload = 1024 * 1024 + 1;
    while bytes.len() < payload + 32 {
        bytes.extend_from_slice(&line);
    }
    bytes.extend_from_slice(b"\x1b\\");
    bytes
}

#[test]
#[cfg(feature = "oniguruma")]
fn tmux_oversized_control_block_ends_the_viewer() {
    let (mut term, mailbox, mutex, state) = tmux_stream_fixture();
    let h = handler_for(&raw mut *term, mailbox, state);
    let mut s = stream::Stream::init(stream::Options::with_handler(h));

    // Reached: `ESC P 1000 p` builds the viewer.
    s.next_slice(b"\x1bP1000p");
    assert!(
        s.handler.tmux_viewer.is_some(),
        ".enter must build the viewer"
    );

    // The oversized block, up to but not including the DCS terminator.
    let block = oversized_tmux_block();
    let body = &block[7..block.len() - 2];
    s.next_slice(body);
    assert!(
        s.handler.tmux_viewer.is_none(),
        "a control-mode block over the 1 MiB limit must end the session and free the viewer"
    );

    // The rest of the DCS is dropped, and the terminator is harmless.
    s.next_slice(b"%begin 2 2 2\n%end 2 2 2\n\x1b\\");
    assert!(s.handler.tmux_viewer.is_none());
    assert!(mailbox_pop(mailbox).is_none());

    s.deinit();
    s.handler.renderer_wakeup.deinit();
    unsafe {
        drop(Box::from_raw(state));
        drop(Box::from_raw(mutex));
        drop(Box::from_raw(mailbox));
    }
    term.deinit(Global);
}

#[test]
#[cfg(feature = "oniguruma")]
fn tmux_reattach_after_an_oversized_control_block_does_not_crash() {
    let (mut term, mailbox, mutex, state) = tmux_stream_fixture();
    let h = handler_for(&raw mut *term, mailbox, state);
    let mut s = stream::Stream::init(stream::Options::with_handler(h));

    // First attach: the viewer exists, then the oversized history block.
    s.next_slice(b"\x1bP1000p");
    assert!(
        s.handler.tmux_viewer.is_some(),
        ".enter must build the viewer"
    );
    let block = oversized_tmux_block();
    s.next_slice(&block[7..]);

    // Second attach in the same surface. The reference asserts here.
    s.next_slice(b"\x1bP1000p");
    assert!(
        s.handler.tmux_viewer.is_some(),
        "the second attach must build a new viewer"
    );

    // The new viewer is live: its startup block and the session change make
    // it send its first command to tmux.
    s.next_slice(b"%begin 1 1 1\n%end 1 1 1\n%session-changed $1 rustty\n");
    let msg = mailbox_pop(mailbox).expect("the new viewer must send its first command");
    let bytes = write_bytes(&msg);
    assert_eq!(Some(&b'\n'), bytes.last());

    s.next_slice(b"\x1b\\");
    assert!(
        s.handler.tmux_viewer.is_none(),
        "ESC \\ ends the second session"
    );

    s.deinit();
    s.handler.renderer_wakeup.deinit();
    unsafe {
        drop(Box::from_raw(state));
        drop(Box::from_raw(mutex));
        drop(Box::from_raw(mailbox));
    }
    term.deinit(Global);
}

// test "kitty clipboard read: targets-only never consumes a one-time grant"
#[test]
fn kitty_clipboard_read_targets_only_never_consumes_a_one_time_grant() {
    // var handler: StreamHandler = undefined;
    // handler.alloc = testing.allocator;
    // handler.kitty_clipboard_grants = .{};
    //
    // PORT NOTE: Rust cannot mirror `undefined`; the minimal constructed
    // handler double (`handler_for`, the module's established seam) stands
    // in — only `kitty_clipboard_grants` is read by the code under test.
    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    let mut h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());
    h.kitty_clipboard_grants
        .grant(
            Global.into(),
            b"otp",
            rustty_vt::kitty::clipboard::Direction::Read,
            true,
        )
        .unwrap();

    // A listing request must not burn the one-time paste password...
    assert!(!h.kittyClipboardReadGranted(b"otp", 0));
    // ...so the follow-up data read is still granted, exactly once.
    assert!(h.kittyClipboardReadGranted(b"otp", 1));
    assert!(!h.kittyClipboardReadGranted(b"otp", 1));

    h.renderer_wakeup.deinit();
    term.deinit(Global.into());
}

// test "kitty clipboard write: oversized text replies EFBIG"
#[test]
fn kitty_clipboard_write_oversized_text_replies_efbig() {
    use rustty_vt::kitty::clipboard as kitty_clipboard;
    use rustty_vt::osc::parsers::kitty_clipboard_protocol::Operation;

    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();

    // var mailbox = try termio.Mailbox.initSPSC(testing.allocator);
    let mailbox: *mut Mailbox = Box::into_raw(Box::new(
        crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
    ));
    // var mutex: std.Io.Mutex = .init; mutex.lockUncancelable(global.io());
    let mutex: *mut rustty_zigstd::thread::Mutex =
        Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
    // var renderer_state: renderer.State = .{ .mutex = &mutex, .terminal = undefined };
    let state: *mut State = Box::into_raw(Box::new(State {
        mutex,
        terminal: &raw mut term,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: rustty_renderer::state::Mouse::default(),
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    }));

    // var handler: StreamHandler = undefined; (minimal constructed double)
    let mut h = handler_for(&raw mut term, mailbox, state);
    h.clipboard_write = ClipboardAccess::Allow;
    h.clipboard_write_limit = 4;
    h.kitty_clipboard_write = None;

    // const begin: Metadata = .{ .op = .write, .id = "macos" };
    let mut begin = kitty_clipboard::Metadata::new(Operation::Write);
    begin.id = rustty_vt::alloc::try_dupe(Global.into(), b"macos").unwrap();
    h.kittyClipboardWriteBegin(&begin, rustty_vt::osc::Terminator::St)
        .unwrap();

    // const state = handler.kitty_clipboard_write.?;
    // try state.data(..., "SGVsbA==") // "Hell"
    // try testing.expectError(error.TooLarge, state.data(..., "bw==")) // "o"
    let mut wdata = kitty_clipboard::Metadata::new(Operation::Wdata);
    wdata.mime = rustty_vt::alloc::try_dupe(Global.into(), b"text/plain").unwrap();
    {
        let write_state = h.kitty_clipboard_write.as_mut().unwrap();
        write_state
            .data(Global.into(), &wdata, b"SGVsbA==")
            .unwrap();
        assert_eq!(
            Err(kitty_clipboard::DataError::TooLarge),
            write_state.data(Global.into(), &wdata, b"bw==")
        );
    }
    // try handler.kittyClipboardWriteFinish(state, .EFBIG, .st);
    let write_state = h.kitty_clipboard_write.take().unwrap();
    h.kittyClipboardWriteFinish(
        &write_state,
        kitty_clipboard::Status::Efbig,
        rustty_vt::osc::Terminator::St,
    )
    .unwrap();
    drop(write_state);
    assert!(h.kitty_clipboard_write.is_none());

    // const response = mailbox.spsc.queue.pop(global.io());
    let msg = mailbox_pop(mailbox).expect("expected the EFBIG status reply");
    match msg {
        Message::WriteAlloc(v) => assert_eq!(
            b"\x1B]5522;type=write:status=EFBIG:id=macos\x1B\\".as_slice(),
            &*v.data,
        ),
        _ => panic!("expected a write_alloc message"),
    }

    // Teardown leaves no transaction that could be committed and
    // forwarded to the macOS clipboard path.
    assert!(mailbox_pop(mailbox).is_none());

    h.deinit();
    h.renderer_wakeup.deinit();
    unsafe {
        drop(Box::from_raw(state));
        drop(Box::from_raw(mutex));
        drop(Box::from_raw(mailbox));
    }
    term.deinit(Global.into());
}

// UD351 (R13, parity tail U-1): an OSC 7 whose host contains a `%` and
// percent-decodes to more than `std.Io.net.HostName.max_len` (255) bytes
// reaches `unreachable` in Zig 0.16.0 `std.Uri.getHost` in the reference.
// Each case drives the full path (VT parser -> OSC 7 -> `reportPwd`) and
// returns the terminal pwd afterwards.
fn osc7_pwd_after(url: &[u8]) -> Option<Vec<u8>> {
    let mut term = Terminal::init(Global.into(), terminal_opts(None)).unwrap();
    let h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());
    let mut s = rustty_vt::stream::Stream::init(rustty_vt::stream::Options::with_handler(h));
    let mut seq = b"\x1b]7;".to_vec();
    seq.extend_from_slice(url);
    seq.extend_from_slice(b"\x1b\\");
    s.next_slice(&seq);
    s.handler.renderer_wakeup.deinit();
    drop(s);
    let pwd = term.get_pwd().map(<[u8]>::to_vec);
    term.deinit(Global.into());
    pwd
}

fn file_url(host: &[u8], path: &[u8]) -> Vec<u8> {
    let mut url = b"file://".to_vec();
    url.extend_from_slice(host);
    url.extend_from_slice(path);
    url
}

#[test]
fn ud351_osc7_host_decoding_to_256_bytes_is_ignored() {
    // 255 `a` + `%41` decodes to 256 bytes: one past the buffer.
    let mut host = vec![b'a'; 255];
    host.extend_from_slice(b"%41");
    assert_eq!(None, osc7_pwd_after(&file_url(&host, b"/tmp")));
}

#[test]
fn ud351_osc7_host_of_300_bytes_with_a_percent_escape_is_ignored() {
    // T-1's probe shape: 300 `a` + `%41`.
    let mut host = vec![b'a'; 300];
    host.extend_from_slice(b"%41");
    assert_eq!(None, osc7_pwd_after(&file_url(&host, b"/tmp")));
}

#[test]
fn ud351_osc7_long_ipv6_bracketed_host_is_ignored() {
    // The brackets are part of the host component that `getHost` decodes.
    let mut host = b"[".to_vec();
    host.extend_from_slice(&[b'a'; 260]);
    host.extend_from_slice(b"%41]");
    assert_eq!(None, osc7_pwd_after(&file_url(&host, b"/tmp")));
}

#[test]
fn ud351_osc7_long_kitty_shell_cwd_host_is_ignored() {
    // kitty-shell-cwd takes the same `getHost` path.
    let mut url = b"kitty-shell-cwd://".to_vec();
    url.extend_from_slice(&[b'a'; 300]);
    url.extend_from_slice(b"%41/tmp");
    assert_eq!(None, osc7_pwd_after(&url));
}

#[test]
fn ud351_osc7_long_hosts_that_fit_or_need_no_decoding_are_rejected_as_non_local() {
    // 254 `a` + `%41` decodes to exactly 255 bytes: fits, not local.
    let mut host = vec![b'a'; 254];
    host.extend_from_slice(b"%41");
    assert_eq!(None, osc7_pwd_after(&file_url(&host, b"/tmp")));

    // No `%`: returned undecoded however long (the reference does not trap).
    assert_eq!(None, osc7_pwd_after(&file_url(&[b'a'; 300], b"/tmp")));
}

#[test]
fn ud351_osc7_percent_encoded_local_host_still_sets_the_pwd() {
    // Control: a short percent-encoded host that decodes to `localhost`
    // passes the same `getHost` decode and the pwd is set.
    //
    // PORT NOTE (N461): on Windows `reportPwd` returns before it parses
    // the URL ("reportPwd unimplemented on windows", the reference's
    // `stream_handler.zig:1486-1489`, ported as-is), so no OSC 7 sets the
    // pwd there and the control expects `None`. It pins that early
    // return; the `ud351_*` rejections above hold on Windows only through
    // it, not through `getHost`.
    let expected = if cfg!(windows) {
        None
    } else {
        Some(b"/tmp".to_vec())
    };
    assert_eq!(expected, osc7_pwd_after(b"file://local%68ost/tmp"));
}

// b1bfd1d9c: an empty OSC 22 resets the mouse shape. libghostty-vt's
// handler always resets to `text`; termio picks `default` while the
// program has mouse reporting on (the pointer is the program's, not a text
// cursor) and `text` otherwise:
//
//     .mouse_shape_reset => try self.setMouseShape(
//         if (self.terminal.flags.mouse_event != .none) .default else .text,
//     ),
//
// Rustty-only: upstream pins the libghostty-vt half in `c/terminal.zig`
// (`get mouse_shape`) and has no termio test for this arm.
#[test]
fn mouse_shape_reset_is_default_with_mouse_reporting_and_text_without() {
    use rustty_vt::mouse::{Event, Shape};

    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    let term_ptr: *mut Terminal = &raw mut term;
    let h = handler_for(term_ptr, std::ptr::null_mut(), std::ptr::null_mut());
    let mut s = stream::Stream::init(stream::Options::with_handler(h));

    // Mouse reporting on: an empty OSC 22 restores the default pointer.
    s.next_slice(b"\x1b]22;pointer\x07");
    // SAFETY: the stream's handler aliases `term`; read and write it through
    // that raw pointer while the stream is live.
    assert_eq!(Shape::Pointer, unsafe { (*term_ptr).mouse_shape });
    unsafe { (*term_ptr).flags.mouse_event = Event::Normal };
    s.next_slice(b"\x1b]22;\x07");
    assert_eq!(Shape::Default, unsafe { (*term_ptr).mouse_shape });

    // Mouse reporting off: an empty OSC 22 restores the text cursor, ended
    // with ST this time.
    s.next_slice(b"\x1b]22;pointer\x1b\\");
    assert_eq!(Shape::Pointer, unsafe { (*term_ptr).mouse_shape });
    unsafe { (*term_ptr).flags.mouse_event = Event::None };
    s.next_slice(b"\x1b]22;\x1b\\");
    assert_eq!(Shape::Text, unsafe { (*term_ptr).mouse_shape });

    s.handler.renderer_wakeup.deinit();
    drop(s);
    term.deinit(Global.into());
}

// Review Focus 2 (P8-W3 T2; rustty-only, the reference adds no termio test):
// a handler built from the DEFAULT `DerivedConfig` must never write a
// DECRQCRA reply, and must ignore XTCHECKSUM. A screen read-back is a
// disclosure channel, so reporting is opt-in via `vt-xt-checksum-report`.
fn xt_checksum_fixture() -> (
    Box<Terminal>,
    *mut Mailbox,
    *mut rustty_zigstd::thread::Mutex,
    *mut State,
) {
    let mut term = Box::new(Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap());
    let mailbox: *mut Mailbox = Box::into_raw(Box::new(
        crate::mailbox::Mailbox::initSPSC().expect("Mailbox::initSPSC"),
    ));
    let mutex: *mut rustty_zigstd::thread::Mutex =
        Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));
    let state: *mut State = Box::into_raw(Box::new(State {
        mutex,
        terminal: &raw mut *term,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: rustty_renderer::state::Mouse::default(),
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    }));
    (term, mailbox, mutex, state)
}

#[test]
fn xt_checksum_default_config_never_replies() {
    let (mut term, mailbox, mutex, state) = xt_checksum_fixture();
    let mut h = handler_for(&raw mut *term, mailbox, state);

    // The default config: reporting off, DEC checksum calculation.
    let mut cfg = DerivedConfig::init(&rustty_config::Config::default());
    assert!(!cfg.xt_checksum_report);
    h.changeConfig(&mut cfg);
    // `changeConfig` ends with the mode-2031 color scheme report.
    assert!(matches!(
        mailbox_pop(mailbox),
        Some(Message::ColorSchemeReport { .. })
    ));
    assert!(mailbox_pop(mailbox).is_none());

    let before = unsafe { (*h.terminal).flags.xt_checksum };
    let mut s = stream::Stream::init(stream::Options::with_handler(h));
    s.next_slice(b"hello");
    // DECRQCRA over the whole screen, one cell, and with an id; then
    // XTCHECKSUM with every bit set and with none.
    s.next_slice(b"\x1b[1;1;1;1;24;80*y");
    s.next_slice(b"\x1b[7;1;1;1;1;1*y");
    s.next_slice(b"\x1b[31#y");
    s.next_slice(b"\x1b[0#y");
    s.next_slice(b"\x1b[#y");

    assert!(
        mailbox_pop(mailbox).is_none(),
        "the default config must never answer DECRQCRA"
    );
    assert_eq!(
        before, term.flags.xt_checksum,
        "XTCHECKSUM is ignored while reporting is disabled"
    );

    s.handler.renderer_wakeup.deinit();
    drop(s);
    unsafe {
        drop(Box::from_raw(state));
        drop(Box::from_raw(mutex));
        drop(Box::from_raw(mailbox));
    }
    term.deinit(Global.into());
}

// (`Config` has 200+ fields, so the field assignments stay.)
#[allow(clippy::field_reassign_with_default)]
#[test]
fn xt_checksum_report_enabled_replies_with_xterms_checksum() {
    let (mut term, mailbox, mutex, state) = xt_checksum_fixture();
    let mut h = handler_for(&raw mut *term, mailbox, state);

    // `vt-xt-checksum-report = true` with esctest's `--xterm-checksum`
    // extension (xterm's `checksumExtension: 23`).
    let mut config = rustty_config::Config::default();
    config.vt_xt_checksum_report = true;
    config.vt_xt_checksum_extension = rustty_config::XtChecksumExtension {
        negate: false,
        attributes: false,
        trim: false,
        undrawn: false,
        full: true,
    };
    let mut cfg = DerivedConfig::init(&config);
    assert!(cfg.xt_checksum_report);
    h.changeConfig(&mut cfg);
    let configured = rustty_vt::xt_checksum::Flags::default()
        .with_positive(true)
        .with_no_attributes(true)
        .with_no_trim(true)
        .with_full(true);
    assert_eq!(configured, cfg.xt_checksum_extension);
    // `changeConfig` applied it to the terminal (and to its RIS default).
    assert_eq!(configured, term.flags.xt_checksum);
    assert_eq!(configured, term.default_xt_checksum);
    assert!(matches!(
        mailbox_pop(mailbox),
        Some(Message::ColorSchemeReport { .. })
    ));

    let mut s = stream::Stream::init(stream::Options::with_handler(h));
    // Grapheme clustering (mode 2027), so the ZWJ family below is ONE cell.
    s.next_slice(b"\x1b[?2027h");
    // Row 1: wide characters; row 2: a ZWJ emoji cluster; row 3: a bold
    // red cell.
    s.next_slice("a橋a橋\r\n".as_bytes());
    s.next_slice("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}x\r\n".as_bytes());
    s.next_slice(b"\x1b[1;31mR\x1b[0m");

    // The expected values below are hard-coded, NOT computed by
    // `rect_xt_checksum`: they come from the reference's own
    // `xt_checksum.compute` (Ghostty a60e9e2a5), fed the same cells in a
    // scratch harness (see the P8-W3 T2 fix report for the source).
    //
    // The cells: `a`, wide `橋` (0x6A4B), `a`, wide `橋`; the family
    // emoji as one wide cluster cell (U+1F468) and `x`; bold red `R`.
    let ask = |s: &mut stream::Stream<StreamHandler<NullMailbox>>, seq: &[u8]| -> Vec<u8> {
        s.next_slice(seq);
        let msg = mailbox_pop(mailbox).expect("DECRQCRA is answered when reporting is on");
        let bytes = write_bytes(&msg);
        assert!(mailbox_pop(mailbox).is_none(), "exactly one reply");
        bytes
    };
    // esctest's `--xterm-checksum` flags (the configured extension): no
    // negation, no attributes, no trim, full codepoints.
    assert_eq!(
        b"\x1bP7!~CD0A\x1b\\".to_vec(),
        ask(&mut s, b"\x1b[7;1;1;1;3;10*y")
    );

    // XTCHECKSUM now takes effect: `CSI 0 # y` selects the DEC calculation
    // (negated, attributes counted, trimmed, DEC 8-bit values).
    s.next_slice(b"\x1b[0#y");
    assert_eq!(
        rustty_vt::xt_checksum::Flags::default(),
        term.flags.xt_checksum
    );
    let dec = ask(&mut s, b"\x1b[7;1;1;1;3;10*y");
    assert_eq!(b"\x1bP7!~FD52\x1b\\".to_vec(), dec);
    // Row 3 alone (the bold red cell).
    assert_eq!(
        b"\x1bP7!~FF2E\x1b\\".to_vec(),
        ask(&mut s, b"\x1b[7;1;3;1;3;10*y")
    );

    // `CSI 2 # y` is the same DEC calculation without attributes: the bold
    // cell stops counting, so the styled cell changes the reply.
    s.next_slice(b"\x1b[2#y");
    let no_attributes = ask(&mut s, b"\x1b[7;1;1;1;3;10*y");
    assert_eq!(b"\x1bP7!~FDD2\x1b\\".to_vec(), no_attributes);
    assert_ne!(dec, no_attributes, "the bold cell must count");
    assert_eq!(
        b"\x1bP7!~FFAE\x1b\\".to_vec(),
        ask(&mut s, b"\x1b[7;1;3;1;3;10*y")
    );

    s.handler.renderer_wakeup.deinit();
    drop(s);
    unsafe {
        drop(Box::from_raw(state));
        drop(Box::from_raw(mutex));
        drop(Box::from_raw(mailbox));
    }
    term.deinit(Global.into());
}

// PORT NOTE: no reference test (P8-W10 T1). Pins the termio half of
// f473f3109's wiring, `.soft_reset => self.terminal.softReset()`: DECSTR
// through the app's `StreamHandler` reaches `Terminal::soft_reset`.
#[test]
fn stream_handler_soft_reset_reaches_the_terminal() {
    use rustty_vt::modes::Mode;

    let mut term = Terminal::init(Global.into(), terminal_opts(Some(false))).unwrap();
    let h = handler_for(&raw mut term, std::ptr::null_mut(), std::ptr::null_mut());
    let mut s = stream::Stream::init(stream::Options::with_handler(h));
    // The region's bottom (1) differs from the full screen's (rows 3, so 2),
    // so the bottom assertion below fails unless DECSTR resets the region.
    s.next_slice(b"\x1b[4h\x1b[?6h\x1b[1;2r");
    assert!(s.handler.term().modes.get(Mode::Insert));
    assert_eq!(1, s.handler.term().scrolling_region.bottom);
    s.next_slice(b"\x1b[!p");
    s.handler.renderer_wakeup.deinit();
    drop(s);

    assert!(!term.modes.get(Mode::Insert));
    assert!(!term.modes.get(Mode::Origin));
    assert_eq!(0, term.scrolling_region.top);
    assert_eq!(2, term.scrolling_region.bottom);
    term.deinit(Global.into());
}
