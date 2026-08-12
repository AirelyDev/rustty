//! WS20 CSS-PARSE-ERROR KAT — the opt-in `gtk-v4_16` byte-dump arm.
//!
//! PORTS the acceptance surface of `application.zig:1261-1274`: when GTK is at
//! least 4.16, `signalCssParsingError` dumps the OFFENDING CSS BYTES
//! (`css_section.getBytes()`) alongside the location/domain/code/message.
//!
//! Ghostty gates that arm on `if (comptime gtk_version.atLeast(4, 16, 0))`, i.e.
//! on its BUILD BOX's headers. rustty mirrors the fold as
//! `#[cfg(feature = "gtk-v4_16")]` (the compile-time API ceiling) plus a live
//! `gtk_version::at_least(4, 16, 0)` probe. This test only exists when the
//! feature is on, and it proves the two things a compile can't: that
//! `CssSection::bytes()` is actually reachable at that ceiling, and that a real
//! `CssProvider::parsing-error` hands us a section whose bytes are the source we
//! fed it.
//!
//! G-GTKINIT: exactly ONE headed `#[test]` lives in this binary.
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk-v4_16")`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk-v4_16"))]

use std::cell::RefCell;
use std::rc::Rc;

// NOTE: no `use gtk4::prelude::*` — `connect_parsing_error` and `load_from_string`
// are INHERENT methods on `CssProvider` in this gtk4-rs, so the glob resolved
// nothing and rustc reported it unused under `--features gtk,gtk-v4_16`. Removed
// rather than allow()'d; a glob reported unused genuinely brings in nothing.

#[test]
#[ignore = "headed GTK; run: cargo test -p rustty-apprt --features gtk,gtk-v4_16 \
            --test gtk_css_parse_error -- --ignored --exact css_parse_error_carries_source_bytes"]
fn css_parse_error_carries_source_bytes() {
    gtk4::init().expect("gtk4::init");

    // The runtime half of the two-part gate. On a < 4.16 shared library the arm
    // self-disables and there is nothing to prove.
    assert!(
        rustty_apprt::gtk::gtk_version::at_least(4, 16, 0),
        "this KAT needs a >= 4.16 GTK runtime; the arm self-disables below it"
    );

    const BAD_CSS: &str = "window { color: ###notacolor; }";

    let seen: Rc<RefCell<Vec<Vec<u8>>>> = Rc::new(RefCell::new(Vec::new()));
    let provider = gtk4::CssProvider::new();
    {
        let seen = seen.clone();
        provider.connect_parsing_error(move |_provider, section, _err| {
            // `const bytes = css_section.getBytes() orelse break :bytes;`
            // (`application.zig:1262`) — the symbol the `v4_14` ceiling hides.
            if let Some(bytes) = section.bytes() {
                seen.borrow_mut().push(bytes.to_vec());
            }
        });
    }

    provider.load_from_string(BAD_CSS);

    let seen = seen.borrow();
    assert!(
        !seen.is_empty(),
        "a malformed stylesheet must raise parsing-error WITH bytes at >= 4.16"
    );
    // Byte fidelity: the dump is the raw source, not a re-rendered string.
    assert!(
        seen.iter().any(|b| b == BAD_CSS.as_bytes()),
        "the section bytes must be the exact source we loaded; got {:?}",
        seen.iter()
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect::<Vec<_>>()
    );
}
