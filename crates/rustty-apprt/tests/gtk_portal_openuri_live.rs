//! PORTAL `OpenURI` LIVE KAT — a real round-trip against the session's own
//! `xdg-desktop-portal`.
//!
//! Companion to `gtk_portal_openuri.rs`, which proves the ORDERING against a stub
//! portal on a private bus. This one proves the port talks to the REAL thing: the
//! ported payload is accepted by `xdg-desktop-portal` and its `Response` lands on the
//! request path the port computed for itself.
//!
//! # Why this is not vacuous
//!
//! `Observed::responses` can only become non-zero if a `Response` signal arrived on
//! the object path THIS PROCESS derived from its own unique bus name and the
//! `handle_token` it sent. Nothing in the test writes that counter, and no local code
//! path fabricates a `Response` — the signal has to come off the bus. The test also
//! asserts the response CODE is 0 (the portal actually resolved the URI) and that the
//! OS-opener fallback was NOT used, so a silently-failed portal open cannot pass.
//!
//! NEGATIVE CONTROLS:
//!   1. Precondition, asserted, not assumed: `org.freedesktop.portal.Desktop` must
//!      have an owner. Without this the test would be measuring nothing.
//!   2. The scheme handler installed by the gate is what makes response code 0
//!      reachable. Run the gate with `NO_HANDLER=1` and the portal answers a NON-zero
//!      code (or errors), and the `last_response_code == Some(0)` assertion fails —
//!      the gate exercises that arm and records it.
//!   3. The companion stub KAT's no-portal arm covers the "portal absent" behavior.
//!
//! The probed URI uses a private scheme, `x-rustty-openuri-gate://`, whose handler the
//! gate points at `/bin/true`. Nothing heavy is launched into the live session — which
//! matters: this file is run against the primary desktop session.
//!
//! ONE HEADED TEST PER BINARY: its own file (G-GTKINIT). No `gtk4::init()` is needed
//! (D-Bus/GLib only), but keeping one test per file preserves the rule's shape.
//!
//! RUN (Fedora VM, inside the live session's environment) via
//! `.port/tools/gates/ws19_portal_openuri_gate.sh`, which installs the scheme handler
//! and captures the bus traffic. Directly:
//!
//! ```sh
//! export XDG_RUNTIME_DIR=/run/user/$(id -u)
//! export DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus
//! cargo test -p rustty-apprt --features gtk --test gtk_portal_openuri_live \
//!   -- --ignored --nocapture
//! ```
//!
//! R5: gated `all(any(linux, freebsd), feature = "gtk")`.
#![cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]

use std::time::{Duration, Instant};

use gio::prelude::*;

use rustty_apprt::gtk::portal::OpenUri;
use rustty_apprt_api::action::{OpenUrl, OpenUrlKind};

const PORTAL_NAME: &str = "org.freedesktop.portal.Desktop";

/// The URI the gate installs a `/bin/true` handler for.
const PROBE_URI: &[u8] = b"x-rustty-openuri-gate://ws19-t8";

#[test]
#[ignore = "needs the live session's D-Bus + a running xdg-desktop-portal — see the module docs"]
fn portal_open_uri_round_trips_against_the_real_portal() {
    let client = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .expect("the session bus (export DBUS_SESSION_BUS_ADDRESS)");

    // NEGATIVE CONTROL 1: the portal must actually be there.
    let reply = client
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&glib::Variant::tuple_from_iter([PORTAL_NAME.to_variant()])),
            Some(glib::VariantTy::new("(b)").unwrap()),
            gio::DBusCallFlags::NONE,
            5_000,
            gio::Cancellable::NONE,
        )
        .expect("NameHasOwner");
    let has_owner: bool = reply.child_value(0).get().unwrap_or(false);
    assert!(
        has_owner,
        "{PORTAL_NAME} has no owner: there is no portal to round-trip against, so \
         this test would pass on nothing"
    );

    let open_uri = OpenUri::new();
    open_uri.set_dbus_connection(Some(&client));

    open_uri
        .start(OpenUrl {
            kind: OpenUrlKind::Unknown,
            url: PROBE_URI,
        })
        .expect("the request is issued");
    assert_eq!(
        open_uri.in_flight(),
        1,
        "start must register the in-flight entry"
    );

    // Pump the main loop until the real portal answers.
    let ctx = glib::MainContext::default();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(30) && open_uri.observed().responses == 0 {
        while ctx.iteration(false) {}
        std::thread::sleep(Duration::from_millis(20));
    }

    let observed = open_uri.observed();
    assert_eq!(
        observed.responses, 1,
        "the real portal's Response never reached our self-computed request path \
         (observed={observed:?})"
    );
    assert_eq!(
        observed.last_response_code,
        Some(0),
        "the portal did not report success; is the x-scheme-handler installed? \
         (observed={observed:?})"
    );
    assert_eq!(
        observed.fallbacks, 0,
        "the OS-opener fallback was used, so the portal path did not carry the open \
         (observed={observed:?})"
    );
    assert_eq!(
        open_uri.in_flight(),
        0,
        "the entry must be released once its Response arrived"
    );

    open_uri.deinit();
    eprintln!("LIVE KAT PASS: real xdg-desktop-portal OpenURI Response code=0 on our request path");
}
