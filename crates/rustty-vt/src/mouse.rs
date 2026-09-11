//! rustty-vt mouse
//!
//! PORTS: ghostty src/terminal/mouse.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const build_options = @import("terminal_options");
//   const lib = @import("lib.zig");
// `lib.Enum` for the `.zig` target is a plain enum with the keys in order;
// `lib.checkGhosttyHEnum` (used by the `Shape` test) lives in `rustty-lib` and
// is reached here via `rustty_lib`. `build_options` is the ported `options`
// module (the generated `terminal_options`).
//
// PORT NOTE (R3): the reference `Shape.getGObjectType` comptime decl, and the
// `@import("gobject")` machinery it depends on, are STRIPPED. They register
// `Shape` as a GTK `gobject` enum ("GhosttyMouseShape") when the GTK app runtime
// is selected — that is an apprt-layer concern, NOT a vt-layer one, so it does
// not belong in `rustty-vt`. The GTK conversion will be seamed in by the apprt
// crate when it is carved; this leaf ports only the mouse protocol/format/shape
// types and their tests.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The event types that can be reported for mouse-related activities.
/// These are all mutually exclusive (hence in a single enum).
// PORT NOTE: `Event = lib.Enum(lib.target, &.{ "none", "x10", "normal",
// "button", "any" })`. For the `.zig` target this is a plain enum with the keys
// in order; ported directly as a Rust enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    None,
    X10,    // 9
    Normal, // 1000
    Button, // 1002
    Any,    // 1003
}

/// Returns true if this event sends motion events.
pub fn event_sends_motion(event: Event) -> bool {
    event == Event::Button || event == Event::Any
}

/// The format of mouse events when enabled.
/// These are all mutually exclusive (hence in a single enum).
// PORT NOTE: `Format = lib.Enum(lib.target, &.{ "x10", "utf8", "sgr", "urxvt",
// "sgr_pixels" })`. For the `.zig` target this is a plain enum with the keys in
// order; ported directly as a Rust enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    X10,
    Utf8,      // 1005
    Sgr,       // 1006
    Urxvt,     // 1015
    SgrPixels, // 1016
}

/// The possible cursor shapes. Not all app runtimes support these shapes.
/// The shapes are always based on the W3C supported cursor styles so we
/// can have a cross platform list.
//
// Must be kept in sync with ghostty_cursor_shape_e
// PORT NOTE: the reference is `enum(c_int)`; modeled here as `#[repr(i32)]`
// (C `int`) so the discriminants match the C ABI ordering used by the
// `checkGhosttyHEnum` test below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Shape {
    Default,
    ContextMenu,
    Help,
    Pointer,
    Progress,
    Wait,
    Cell,
    Crosshair,
    Text,
    VerticalText,
    Alias,
    Copy,
    Move,
    NoDrop,
    NotAllowed,
    Grab,
    Grabbing,
    AllScroll,
    ColResize,
    RowResize,
    NResize,
    EResize,
    SResize,
    WResize,
    NeResize,
    NwResize,
    SeResize,
    SwResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ZoomIn,
    ZoomOut,
}

impl Shape {
    /// Build cursor shape from string or null if its unknown.
    pub fn from_string(v: &[u8]) -> Option<Shape> {
        string_map().get(v).copied()
    }

    // Make this a valid gobject if we're in a GTK environment.
    // PORT NOTE (R3): `pub const getGObjectType = gtk: { ... }` STRIPPED — see the
    // module-level PORT NOTE. It made `Shape` a GTK `gobject` enum and is an
    // apprt-layer concern. The reference doc comment above is preserved verbatim
    // so the stripped decl's intent is not lost (R1).
}

#[cfg(test)]
mod shape_tests {
    use super::Shape;
    use crate::options as build_options;
    use rustty_lib::c_header::{header_decls as parse_header_decls, parse_header_enum};
    use rustty_lib::r#enum::{check_ghostty_henum, enum_type_values, EnumType};
    use std::collections::HashMap;

    /// The `@hasDecl`/`@field` stand-in: the header's declarations as a lookup.
    ///
    /// PORT NOTE (S45-F1): the header parser this used to carry inline now
    /// lives in `rustty_lib::c_header`, shared with every other
    /// `checkGhosttyHEnum` port, all of which were synthesizing their C side
    /// from the descriptor under test.
    fn header_decls(src: &str) -> HashMap<String, i64> {
        parse_header_decls(src, HEADER_ENUM)
    }

    /// The checked-in public C header, embedded at compile time.
    ///
    /// PORT NOTE: the reference `lib.checkGhosttyHEnum` does
    /// `const c = @import("ghostty.h")` — it reflects over the REAL generated
    /// header. Rust has no `@import` for a C header, so the test reads the
    /// checked-in header's text and parses the enum block out of it (see
    /// [`rustty_lib::c_header::parse_header_enum`]). The point is the same: the C declarations the
    /// check compares against must come from the header, never from the
    /// descriptor under test.
    const HEADER_SRC: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../include/rustty-embed.h"
    ));

    /// The header's enum tag for `Shape` (`terminal.MouseShape`).
    const HEADER_ENUM: &str = "rustty_action_mouse_shape_e";

    /// PORT NOTE: models the reference `enum(c_int)` `Shape` as a [`EnumType`]
    /// (C target) so the `checkGhosttyHEnum` test can reflect over its keys and
    /// backing int. The keys are the snake_case forms of the variants, in
    /// declaration order, matching `@typeInfo(Shape).@"enum".fields`.
    fn enum_descriptor() -> EnumType {
        // PORT NOTE (N29): the VALUES are `Shape::Variant as i64`, read out of
        // the `#[repr(i32)]` enum. This used to call `enum_type(Target::C, ..)`,
        // the `lib.Enum` FACTORY model, which assigns each key its INDEX — so
        // the check compared the header to a hand-typed list's ORDER and could
        // not observe `Shape` at all.
        enum_type_values(&[
            ("default", Shape::Default as i64),
            ("context_menu", Shape::ContextMenu as i64),
            ("help", Shape::Help as i64),
            ("pointer", Shape::Pointer as i64),
            ("progress", Shape::Progress as i64),
            ("wait", Shape::Wait as i64),
            ("cell", Shape::Cell as i64),
            ("crosshair", Shape::Crosshair as i64),
            ("text", Shape::Text as i64),
            ("vertical_text", Shape::VerticalText as i64),
            ("alias", Shape::Alias as i64),
            ("copy", Shape::Copy as i64),
            ("move", Shape::Move as i64),
            ("no_drop", Shape::NoDrop as i64),
            ("not_allowed", Shape::NotAllowed as i64),
            ("grab", Shape::Grab as i64),
            ("grabbing", Shape::Grabbing as i64),
            ("all_scroll", Shape::AllScroll as i64),
            ("col_resize", Shape::ColResize as i64),
            ("row_resize", Shape::RowResize as i64),
            ("n_resize", Shape::NResize as i64),
            ("e_resize", Shape::EResize as i64),
            ("s_resize", Shape::SResize as i64),
            ("w_resize", Shape::WResize as i64),
            ("ne_resize", Shape::NeResize as i64),
            ("nw_resize", Shape::NwResize as i64),
            ("se_resize", Shape::SeResize as i64),
            ("sw_resize", Shape::SwResize as i64),
            ("ew_resize", Shape::EwResize as i64),
            ("ns_resize", Shape::NsResize as i64),
            ("nesw_resize", Shape::NeswResize as i64),
            ("nwse_resize", Shape::NwseResize as i64),
            ("zoom_in", Shape::ZoomIn as i64),
            ("zoom_out", Shape::ZoomOut as i64),
        ])
    }

    #[test]
    fn ghostty_h_mouse_shape() {
        // PORT NOTE: `if (comptime build_options.artifact == .lib) return
        // error.SkipZigTest;`. rustty-vt resolves `artifact` to the application
        // artifact (the reference `.ghostty`), so this skip path is not taken
        // and the check runs — preserved verbatim.
        if build_options::ARTIFACT == crate::build_options::Artifact::Lib {
            return;
        }

        // PORT NOTE: `try lib.checkGhosttyHEnum(Shape, "GHOSTTY_MOUSE_SHAPE_");`,
        // retargeted to `RUSTTY_MOUSE_SHAPE_` per R2. `checkGhosttyHEnum` takes a
        // lookup closure mapping an UPPER-CASE `<prefix><KEY>` name to its declared
        // C value (mirroring `@hasDecl`/`@field` on `@import("ghostty.h")`). The
        // declarations therefore come from the checked-in header text, exactly as
        // the reference's come from the generated header — never from the
        // descriptor under test.
        let t = enum_descriptor();
        let decls = header_decls(HEADER_SRC);
        check_ghostty_henum(&t, "RUSTTY_MOUSE_SHAPE_", |name| decls.get(name).copied()).unwrap();
    }

    /// Self-check that the probe above can actually FAIL — i.e. that it is
    /// reading the header and not the descriptor. Each mutation of the header
    /// text is one of the drifts `checkGhosttyHEnum` exists to catch.
    ///
    /// PORT NOTE: no reference analogue; the oracle gets this for free because
    /// `@import("ghostty.h")` is the real header. Added per PORT_RULES (prove
    /// the check even where Ghostty has none).
    #[test]
    fn ghostty_h_mouse_shape_probe_can_fail() {
        let t = enum_descriptor();
        let run = |src: &str| {
            let decls = header_decls(src);
            check_ghostty_henum(&t, "RUSTTY_MOUSE_SHAPE_", |name| decls.get(name).copied())
        };

        // Baseline: the real header passes.
        run(HEADER_SRC).unwrap();

        // A dropped declaration leaves a `Shape` value unmatched.
        let dropped = HEADER_SRC.replace("  RUSTTY_MOUSE_SHAPE_NWSE_RESIZE,\n", "");
        assert_ne!(dropped, HEADER_SRC);
        assert!(run(&dropped).is_err(), "dropped enumerator must fail");

        // A renamed declaration is equally a drop, from the check's view.
        let renamed = HEADER_SRC.replace(
            "RUSTTY_MOUSE_SHAPE_ZOOM_OUT",
            "RUSTTY_MOUSE_SHAPE_ZOOM_OUTER",
        );
        assert_ne!(renamed, HEADER_SRC);
        assert!(run(&renamed).is_err(), "renamed enumerator must fail");

        // A revalued declaration mismatches the field's backing int.
        let revalued =
            HEADER_SRC.replace("RUSTTY_MOUSE_SHAPE_HELP,", "RUSTTY_MOUSE_SHAPE_HELP = 99,");
        assert_ne!(revalued, HEADER_SRC);
        assert!(run(&revalued).is_err(), "revalued enumerator must fail");

        // Reordering shifts every implicit value from the swap point on.
        let reordered = HEADER_SRC.replace(
            "  RUSTTY_MOUSE_SHAPE_HELP,\n  RUSTTY_MOUSE_SHAPE_POINTER,\n",
            "  RUSTTY_MOUSE_SHAPE_POINTER,\n  RUSTTY_MOUSE_SHAPE_HELP,\n",
        );
        assert_ne!(reordered, HEADER_SRC);
        assert!(run(&reordered).is_err(), "reordered enumerators must fail");
    }

    /// The parser must refuse anything it cannot read rather than silently
    /// yielding an empty/partial declaration set (which would make the check
    /// above vacuously pass).
    #[test]
    fn header_enum_parser_rejects_garbage() {
        assert!(parse_header_enum(HEADER_SRC, "rustty_no_such_enum_e").is_err());
        assert!(parse_header_enum("typedef enum {\n} rustty_empty_e;", "rustty_empty_e").is_err());
        assert!(parse_header_enum(
            "typedef enum {\n  A = zzz,\n} rustty_bad_e;",
            "rustty_bad_e"
        )
        .is_err());
        assert!(
            parse_header_enum("typedef enum {\n  A B,\n} rustty_bad2_e;", "rustty_bad2_e").is_err()
        );

        // Explicit values and trailing-comma-less bodies parse as C does.
        assert_eq!(
            parse_header_enum(
                "typedef enum {\n  A,\n  B = 7,\n  C\n} rustty_ok_e;",
                "rustty_ok_e"
            )
            .unwrap(),
            vec![
                ("A".to_string(), 0),
                ("B".to_string(), 7),
                ("C".to_string(), 8),
            ]
        );
    }
}

/// PORT NOTE: `const string_map = std.StaticStringMap(Shape).initComptime(...)`.
/// `std.StaticStringMap` is an exact (case-sensitive) byte-key map; modeled as a
/// lazily-built `HashMap<&[u8], Shape>` over the same `(key, shape)` pairs, in
/// the same order. `string_map().get(v)` mirrors `string_map.get(v)`.
fn string_map() -> &'static HashMap<Vec<u8>, Shape> {
    static MAP: OnceLock<HashMap<Vec<u8>, Shape>> = OnceLock::new();
    MAP.get_or_init(build_string_map)
}

fn build_string_map() -> HashMap<Vec<u8>, Shape> {
    let pairs: &[(&'static str, Shape)] = &[
        // W3C
        ("default", Shape::Default),
        ("context-menu", Shape::ContextMenu),
        ("help", Shape::Help),
        ("pointer", Shape::Pointer),
        ("progress", Shape::Progress),
        ("wait", Shape::Wait),
        ("cell", Shape::Cell),
        ("crosshair", Shape::Crosshair),
        ("text", Shape::Text),
        ("vertical-text", Shape::VerticalText),
        ("alias", Shape::Alias),
        ("copy", Shape::Copy),
        ("move", Shape::Move),
        ("no-drop", Shape::NoDrop),
        ("not-allowed", Shape::NotAllowed),
        ("grab", Shape::Grab),
        ("grabbing", Shape::Grabbing),
        ("all-scroll", Shape::AllScroll),
        ("col-resize", Shape::ColResize),
        ("row-resize", Shape::RowResize),
        ("n-resize", Shape::NResize),
        ("e-resize", Shape::EResize),
        ("s-resize", Shape::SResize),
        ("w-resize", Shape::WResize),
        ("ne-resize", Shape::NeResize),
        ("nw-resize", Shape::NwResize),
        ("se-resize", Shape::SeResize),
        ("sw-resize", Shape::SwResize),
        ("ew-resize", Shape::EwResize),
        ("ns-resize", Shape::NsResize),
        ("nesw-resize", Shape::NeswResize),
        ("nwse-resize", Shape::NwseResize),
        ("zoom-in", Shape::ZoomIn),
        ("zoom-out", Shape::ZoomOut),
        // xterm/foot
        ("left_ptr", Shape::Default),
        ("question_arrow", Shape::Help),
        ("hand", Shape::Pointer),
        ("left_ptr_watch", Shape::Progress),
        ("watch", Shape::Wait),
        ("cross", Shape::Crosshair),
        ("xterm", Shape::Text),
        ("dnd-link", Shape::Alias),
        ("dnd-copy", Shape::Copy),
        ("dnd-move", Shape::Move),
        ("dnd-no-drop", Shape::NoDrop),
        ("crossed_circle", Shape::NotAllowed),
        ("hand1", Shape::Grab),
        ("right_side", Shape::EResize),
        ("top_side", Shape::NResize),
        ("top_right_corner", Shape::NeResize),
        ("top_left_corner", Shape::NwResize),
        ("bottom_side", Shape::SResize),
        ("bottom_right_corner", Shape::SeResize),
        ("bottom_left_corner", Shape::SwResize),
        ("left_side", Shape::WResize),
        ("fleur", Shape::AllScroll),
    ];
    pairs
        .iter()
        .map(|(k, v)| (k.as_bytes().to_vec(), *v))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_shape_from_string() {
        // PORT NOTE: `const testing = std.testing;`
        assert_eq!(Shape::Default, Shape::from_string(b"default").unwrap());
    }

    // PORT NOTE: the reference root `test { _ = std.testing.refAllDeclsRecursive(
    // @This()); }` is a reflection smoke test that forces every declaration in the
    // module to be analyzed. Rust compiles/type-checks every item unconditionally,
    // so the structural guarantee is automatic; we mirror it concretely by
    // touching each public decl so a regression surfaces as a test failure.
    #[test]
    fn ref_all_decls() {
        assert!(event_sends_motion(Event::Button));
        assert!(event_sends_motion(Event::Any));
        assert!(!event_sends_motion(Event::None));
        assert!(!event_sends_motion(Event::X10));
        assert!(!event_sends_motion(Event::Normal));
        let _ = Format::X10;
        let _ = Format::Utf8;
        let _ = Format::Sgr;
        let _ = Format::Urxvt;
        let _ = Format::SgrPixels;
        assert_eq!(Some(Shape::ZoomOut), Shape::from_string(b"zoom-out"));
        assert_eq!(None, Shape::from_string(b"nonexistent"));
    }
}
