//! D5 — paired-invariant catalog: integration tests through the in-crate seams.
//!
//! Detector D5 (`.port/phase-2/BUN_PORT_LEARNINGS.md` §4, adapts Bun Class 4 /
//! oven-sh/bun#30678: "coupled-emission-site invariant drift"). The rule: every
//! predicate/flag consulted at 2+ producer/consumer sites is a NAMED paired
//! invariant with (producer assert, consumer graceful-degrade, ONE integration
//! test through the seam). The catalog is `.port/phase-2/VT_INVARIANTS.md`.
//!
//! Two of the four seeded pairs have BOTH ends inside `rustty-vt`, so their seam
//! is testable here as a real integration test (public API only, no reach into
//! private internals beyond the already-`pub` raw-pointer fields the port
//! exposes):
//!
//!   * mode-state ↔ key-encoder      (modes.set  →  Options::from_terminal → encode)
//!   * style-id refcount Page ↔ style set (set_attribute → styles.add/release; count)
//!
//! The other two seeded pairs (dirty-tracking ↔ renderer damage; kitty
//! placements ↔ renderer atlas) cross the rustty-vt → rustty-renderer boundary;
//! their seam test needs cross-crate wiring and is tracked in the catalog, not
//! here. See VT_INVARIANTS.md.
//!
//! This file is OWNED by detector D5 and is disjoint from every other detector's
//! files (naming convention: `d5_*`). It edits no shared source.

use allocator_api2::alloc::Global;

use rustty_vt::color::{DynamicPalette, DynamicRGB};
use rustty_vt::input::key::{Action, Key, KeyEvent, Mods};
use rustty_vt::input::key_encode::{self, Options as EncOptions};
use rustty_vt::kitty::graphics_image::Limits;
use rustty_vt::modes::{Mode, ModePacked};
use rustty_vt::sgr::Attribute;
use rustty_vt::terminal::{Colors, Options, Terminal};

/// Build a minimal `Terminal` the same way the in-crate `terminal.rs` test
/// helper `opts()` does (that helper is not exported from the crate, so the
/// fields are spelled out). Small grid — these tests only exercise the seam,
/// not layout.
fn terminal(cols: u16, rows: u16) -> Terminal {
    let opts = Options {
        cols,
        rows,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        colors: Colors {
            background: DynamicRGB::UNSET,
            foreground: DynamicRGB::UNSET,
            cursor: DynamicRGB::UNSET,
            palette: DynamicPalette::default_palette(),
        },
        default_modes: ModePacked::default(),
        default_cursor_style: rustty_vt::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        kitty_image_storage_limit: 320 * 1000 * 1000,
        kitty_image_loading_limits: Limits::DIRECT,
    };
    Terminal::init(Global, opts).expect("Terminal::init")
}

// ---------------------------------------------------------------------------
// INVARIANT #4 — mode-state ↔ key-encoder
//
// PREDICATE: the key encoder's cursor-key encoding matches DECCKM (Mode
// CursorKeys) as the terminal currently holds it. PRODUCER: `terminal.modes`
// (set by the VT stream). CONSUMER: `key_encode::Options::from_terminal`, which
// reads `modes.get(Mode::CursorKeys)` into `cursor_key_application`, then
// `key_encode::encode` selects the SS3 (`ESC O A`) vs CSI (`ESC [ A`) arrow form.
//
// The Bun-class failure this guards: two sites computing "is application cursor
// mode active" differently — here the producer (modes) and the consumer
// (encoder) MUST agree. This test drives the real seam end to end.
// ---------------------------------------------------------------------------
#[test]
fn mode_state_key_encoder_seam() {
    let up = KeyEvent {
        action: Action::Press,
        key: Key::ArrowUp,
        mods: Mods::default(),
        ..Default::default()
    };

    // Normal cursor mode (DECCKM reset) → CSI form `ESC [ A`.
    {
        let mut t = terminal(80, 24);
        assert!(
            !t.modes.get(Mode::CursorKeys),
            "DECCKM defaults to reset (normal cursor keys)"
        );
        let opts = EncOptions::from_terminal(&t);
        assert!(
            !opts.cursor_key_application,
            "seam: consumer must read the producer's reset mode"
        );
        let mut out: Vec<u8> = Vec::new();
        key_encode::encode(&mut out, &up, opts).unwrap();
        assert_eq!(out, b"\x1b[A", "normal cursor keys → CSI A");
        t.deinit(Global);
    }

    // Application cursor mode (DECCKM set) → SS3 form `ESC O A`.
    {
        let mut t = terminal(80, 24);
        t.modes.set(Mode::CursorKeys, true);
        let opts = EncOptions::from_terminal(&t);
        assert!(
            opts.cursor_key_application,
            "seam: consumer must read the producer's set mode"
        );
        let mut out: Vec<u8> = Vec::new();
        key_encode::encode(&mut out, &up, opts).unwrap();
        assert_eq!(out, b"\x1bOA", "application cursor keys → SS3 A");
        t.deinit(Global);
    }
}

// ---------------------------------------------------------------------------
// INVARIANT #2 — style-id refcount: Page ↔ style set
//
// PREDICATE: every non-default style id stored on the page has a matching live
// refcount in that page's `RefCountedSet` (`page.styles`); releasing the last
// reference drops the living count to 0. PRODUCER: `screen.set_attribute` →
// `manual_style_update` → `page.styles.add`/`.release` (the `ref_counted_set`
// asserts `ref_count > 0` on release). CONSUMER: any style lookup + the
// page-integrity / clone paths that walk `styles.count()`.
//
// The Bun-class failure this guards: the id written into a cell and the refcount
// held for it drifting apart (a leaked or double-freed style id). This test
// drives set_attribute through the Terminal and reads the living style count off
// the active page (public raw-pointer fields, mirroring the in-crate
// screen.rs style-refcount tests).
// ---------------------------------------------------------------------------
#[test]
fn style_id_refcount_page_style_set_seam() {
    let mut t = terminal(10, 10);

    // Reach the active page via the crate's public raw-pointer chain:
    //   screens.active : *mut Screen  →  cursor.page_pin : *mut Pin
    //   (*pin).node : *mut Node       →  Node::page(node) : *mut Page
    let count = |t: &Terminal| -> usize {
        unsafe {
            let screen = &*t.screens.active;
            let pin = &*screen.cursor.page_pin;
            let page = rustty_vt::pagelist::Node::page(pin.node);
            (*page).styles.count()
        }
    };

    // Default cursor style is never stored: living count starts at 0.
    assert_eq!(count(&t), 0, "no styles registered before any attribute");

    // Bold registers exactly one style (producer add).
    t.set_attribute(Attribute::Bold).unwrap();
    assert_eq!(count(&t), 1, "seam: Bold produces one refcounted style id");

    // Unsetting all attributes returns to the default style (producer release):
    // the previously-registered style's last reference is dropped.
    t.set_attribute(Attribute::Unset).unwrap();
    assert_eq!(count(&t), 0, "seam: Unset releases the style id (no leak)");

    t.deinit(Global);
}
