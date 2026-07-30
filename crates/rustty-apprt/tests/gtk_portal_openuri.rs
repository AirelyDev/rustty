//! PORTAL `OpenURI` KAT — the pre-subscription ordering, live on a real D-Bus.
//!
//! PORTS the verification of ghostty `src/apprt/gtk/portal/OpenURI.zig` +
//! `src/apprt/gtk/portal.zig`, ported as [`rustty_apprt::gtk::portal`].
//!
//! # What is asserted, and why it cannot pass vacuously
//!
//! The load-bearing property of the port is an ORDERING: the `Response` subscription
//! must be installed on the SELF-COMPUTED request object path BEFORE the `OpenURI`
//! method call is issued, because `Response` is delivered exactly once and an eager
//! portal can emit it before the method's reply is even dispatched
//! (`OpenURI.zig:185-186`; the module docs of `portal/open_uri.rs` explain the race).
//!
//! This KAT stands up a STUB portal that is deliberately maximally eager: it emits
//! `Response` FIRST and returns the `(o)` reply SECOND. So:
//!
//!   * if the port pre-subscribes (correct), the `Response` is received and
//!     `Observed::responses` becomes 1;
//!   * if the port subscribed after the reply (the racy documented flow), the signal
//!     is already gone and `responses` stays 0 — the assertion FAILS.
//!
//! DEMONSTRATED FAIL ARM (measured on the VM, WS19 T8). The counterfactual that
//! actually reproduces the bug is "subscribe only once the method REPLY names the
//! path" — the literal documented flow. Applying it (drop the `subscribe_to_response`
//! in `start`, subscribe inside `request_callback` instead) fails this test with
//!
//! ```text
//! no Response was delivered: ... observed=Observed { responses: 0, call_replies: 1, .. },
//!   stub saw=Seen { calls: 1, response_path: Some(".../request/1_0/3389bbd4ce3e102a"), .. }
//! ```
//!
//! i.e. the stub proves it emitted on the right path and the client still never saw it.
//!
//! A WEAKER counterfactual does NOT reproduce it, and this is worth recording because
//! it was tried first and passed: merely moving `subscribe_to_response` to the line
//! AFTER `send_request` still passes. `gio`'s `call` is asynchronous and the portal's
//! `Response` is a UNICAST signal, so the client-side match table is consulted at
//! DISPATCH time, and no main-loop iteration happens between the two statements. The
//! ordering that matters is therefore "before any main-loop iteration", not "before
//! the send" — which is exactly why the subscription must not wait for the reply
//! callback, and why the reference's own comment frames it that way.
//!
//! The stub ALSO records the payload it received, so the test asserts the on-the-wire
//! shape (parent window, uri, and the three options `handle_token`/`writable`/`ask`),
//! and that the `handle_token` matches the token embedded in the request path we
//! subscribed to. A port that computed the path from one token and sent another would
//! pass a "did we get a response" check on a lenient portal but fails here.
//!
//! NEGATIVE CONTROL, in the same test after the positive arm: the stub RELEASES the
//! portal bus name, and a second `start` is issued. Nothing owns
//! `org.freedesktop.portal.Desktop`, so the call errors, and the assertion is that the
//! port reached the OS-opener fallback exactly once and dropped the entry — i.e. it
//! neither hangs nor leaks an in-flight request. Without the fallback arm this half
//! would time out.
//!
//! No `gtk4::init()` here: this exercises the D-Bus/GLib layer only, so the
//! G-GTKINIT one-headed-test-per-binary rule does not apply (and no display is
//! needed).
//!
//! # The bus this runs on
//!
//! The test spawns its OWN `dbus-daemon` from a generated config with NO
//! `<servicedir>`, and never touches the ambient session bus. Both properties are
//! load-bearing:
//!
//!   * it owns `org.freedesktop.portal.Desktop`, which would be impossible (and rude)
//!     on a desktop session bus;
//!   * with no service directory the bus CANNOT activate anything. The first attempt
//!     at this test ran under `dbus-run-session`, which inherits the system service
//!     files: releasing the stub's name made D-Bus start the REAL
//!     `xdg-desktop-portal`, the "no portal" call succeeded, and the negative control
//!     reported `fallbacks: 0` — i.e. the control was not merely weak, it was wrong.
//!     An activation-free bus is what makes "nothing owns the name" actually mean it.
//!
//! The daemon is killed by a `Drop` guard on every exit path, including panics.
//!
//! RUN (Fedora VM; needs `dbus-daemon` on PATH):
//!
//! ```sh
//! cargo test -p rustty-apprt --features gtk \
//!   --test gtk_portal_openuri -- --ignored --nocapture
//! ```
//!
//! R5: gated `all(target_os = "linux", feature = "gtk")`.
#![cfg(all(target_os = "linux", feature = "gtk"))]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

// `gio::prelude` re-exports `glib::prelude`, which is where `ToVariant` lives.
use gio::prelude::*;

use rustty_apprt::gtk::portal::{self, OpenUri};
use rustty_apprt_api::action::{OpenUrl, OpenUrlKind};

const PORTAL_NAME: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const OPEN_URI_IFACE: &str = "org.freedesktop.portal.OpenURI";
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";

/// The stub portal's introspection XML — the real `OpenURI` signature.
const STUB_XML: &str = r#"
<node>
  <interface name="org.freedesktop.portal.OpenURI">
    <method name="OpenURI">
      <arg type="s" name="parent_window" direction="in"/>
      <arg type="s" name="uri" direction="in"/>
      <arg type="a{sv}" name="options" direction="in"/>
      <arg type="o" name="handle" direction="out"/>
    </method>
  </interface>
</node>
"#;

/// What the stub portal saw on the wire, for the payload-shape assertions.
#[derive(Debug, Default, Clone)]
struct Seen {
    calls: u32,
    parent_window: Option<String>,
    uri: Option<String>,
    handle_token: Option<String>,
    writable: Option<bool>,
    ask: Option<bool>,
    /// The request object path the stub emitted `Response` on.
    response_path: Option<String>,
}

/// A private, ACTIVATION-FREE `dbus-daemon`. Killed on drop so no daemon can survive
/// the test — including when an assertion panics.
struct PrivateBus {
    child: std::process::Child,
    address: String,
    _dir: std::path::PathBuf,
}

impl PrivateBus {
    fn spawn() -> Self {
        use std::io::{BufRead, BufReader};

        let dir = std::env::temp_dir().join(format!("ws19-t8-bus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let conf = dir.join("session.conf");
        // No <servicedir>: this bus can never activate a service, so "the name has no
        // owner" cannot be quietly undone by activation.
        std::fs::write(
            &conf,
            format!(
                r#"<busconfig>
  <type>session</type>
  <listen>unix:tmpdir={}</listen>
  <policy context="default">
    <allow own="*"/>
    <allow send_destination="*"/>
    <allow receive_sender="*"/>
  </policy>
</busconfig>
"#,
                dir.display()
            ),
        )
        .expect("write bus config");

        let mut child = std::process::Command::new("dbus-daemon")
            .arg("--nofork")
            .arg("--print-address")
            .arg(format!("--config-file={}", conf.display()))
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("dbus-daemon must be installed to run this KAT");

        let mut line = String::new();
        BufReader::new(child.stdout.take().expect("piped stdout"))
            .read_line(&mut line)
            .expect("dbus-daemon printed no address");
        let address = line.trim().to_owned();
        assert!(
            address.starts_with("unix:"),
            "unexpected bus address {address:?}"
        );

        Self {
            child,
            address,
            _dir: dir,
        }
    }

    fn connect(&self) -> gio::DBusConnection {
        gio::DBusConnection::for_address_sync(
            &self.address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None::<&gio::DBusAuthObserver>,
            gio::Cancellable::NONE,
        )
        .expect("connect to the private bus")
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Ask the bus whether the portal name currently has an owner. Used both to refuse
/// to run against the real portal and to prove the negative control's precondition.
fn portal_name_has_owner(client: &gio::DBusConnection) -> Result<bool, glib::Error> {
    let reply = client.call_sync(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "NameHasOwner",
        Some(&glib::Variant::tuple_from_iter([PORTAL_NAME.to_variant()])),
        Some(glib::VariantTy::new("(b)").unwrap()),
        gio::DBusCallFlags::NONE,
        5_000,
        gio::Cancellable::NONE,
    )?;
    Ok(reply.child_value(0).get().unwrap_or(true))
}

/// Pump the default main context until `done` or `limit` elapses. Returns whether
/// `done` became true, so a timeout is always reported as a failed assertion rather
/// than as a silent pass.
fn pump_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let ctx = glib::MainContext::default();
    let start = Instant::now();
    while start.elapsed() < limit {
        if done() {
            return true;
        }
        // Non-blocking iteration plus a short sleep: a blocking iteration would
        // wedge if no source is ever readied (exactly the hang this test rules out).
        while ctx.iteration(false) {}
        std::thread::sleep(Duration::from_millis(10));
    }
    done()
}

#[test]
#[ignore = "spawns a private dbus-daemon; Linux + the gtk feature — see the module docs"]
fn portal_open_uri_presubscribes_then_falls_back_without_a_portal() {
    let bus = PrivateBus::spawn();
    let client = bus.connect();

    // --- the bus must be genuinely portal-free -------------------------------
    // If this were ever true the negative control below would be measuring the real
    // portal instead of the absence of one.
    let has_owner = portal_name_has_owner(&client).expect("NameHasOwner");
    assert!(
        !has_owner,
        "{PORTAL_NAME} already has an owner on the private bus"
    );

    // --- the stub portal, on its own connection ------------------------------
    let stub = bus.connect();

    let node = gio::DBusNodeInfo::for_xml(STUB_XML).expect("stub xml");
    let iface = node
        .lookup_interface(OPEN_URI_IFACE)
        .expect("the stub declares OpenURI");

    let seen: Rc<RefCell<Seen>> = Rc::new(RefCell::new(Seen::default()));
    let seen_cb = seen.clone();
    let _registration = stub
        .register_object(PORTAL_PATH, &iface)
        .method_call(
            move |conn, sender, _object_path, _iface_name, method, params, invocation| {
                assert_eq!(method, "OpenURI", "the stub exports only OpenURI");
                let sender = sender
                    .expect("a unicast method call has a sender")
                    .to_owned();

                let parent_window = params.child_value(0).str().map(str::to_owned);
                let uri = params.child_value(1).str().map(str::to_owned);
                let options = glib::VariantDict::new(Some(&params.child_value(2)));
                let handle_token = options
                    .lookup_value("handle_token", None)
                    .and_then(|v| v.str().map(str::to_owned));
                let writable = options
                    .lookup_value("writable", None)
                    .and_then(|v| v.get::<bool>());
                let ask = options
                    .lookup_value("ask", None)
                    .and_then(|v| v.get::<bool>());

                // The request path a real portal derives: the caller's unique name
                // with `:` stripped and `.` -> `_`, plus the handle token.
                let token = handle_token.clone().expect("handle_token option");
                let request_path = format!(
                    "{PORTAL_PATH}/request/{}/{token}",
                    sender.trim_start_matches(':').replace('.', "_"),
                );

                // *** THE ORDERING UNDER TEST ***
                // Emit `Response` BEFORE returning the reply. A client that only
                // subscribes after seeing the reply misses this signal forever.
                conn.emit_signal(
                    Some(&sender),
                    &request_path,
                    REQUEST_IFACE,
                    "Response",
                    Some(&glib::Variant::tuple_from_iter([
                        0u32.to_variant(),
                        glib::VariantDict::new(None).end(),
                    ])),
                )
                .expect("emit Response");

                {
                    let mut s = seen_cb.borrow_mut();
                    s.calls += 1;
                    s.parent_window = parent_window;
                    s.uri = uri;
                    s.handle_token = handle_token;
                    s.writable = writable;
                    s.ask = ask;
                    s.response_path = Some(request_path.clone());
                }

                let handle = glib::variant::ObjectPath::try_from(request_path)
                    .expect("the derived request path is a valid object path");
                invocation
                    .return_value(Some(&glib::Variant::tuple_from_iter([handle.to_variant()])));
            },
        )
        .build()
        .expect("register the stub portal object");

    let acquired = Rc::new(std::cell::Cell::new(false));
    let acquired_cb = acquired.clone();
    let owner_id = gio::bus_own_name_on_connection(
        &stub,
        PORTAL_NAME,
        gio::BusNameOwnerFlags::NONE,
        move |_, _| acquired_cb.set(true),
        |_, _| {},
    );
    assert!(
        pump_until(Duration::from_secs(5), || acquired.get()),
        "the stub never acquired {PORTAL_NAME}"
    );

    // --- POSITIVE ARM: the pre-subscription receives the eager Response -------
    let open_uri = OpenUri::new();
    open_uri.set_dbus_connection(Some(&client));

    const URL: &[u8] = b"https://example.invalid/ws19-t8-kat";
    open_uri
        .start(OpenUrl {
            kind: OpenUrlKind::Html,
            url: URL,
        })
        .expect("start issues the request");

    // The entry exists BEFORE any response: proves `start` registered it, so the
    // "entry dropped" assertion below is a real transition and not a no-op.
    assert_eq!(
        open_uri.in_flight(),
        1,
        "start must register exactly one in-flight entry"
    );

    assert!(
        pump_until(Duration::from_secs(10), || open_uri.observed().responses
            >= 1),
        "no Response was delivered: the subscription was not installed before the \
         method call (observed={:?}, stub saw={:?})",
        open_uri.observed(),
        seen.borrow(),
    );

    let observed = open_uri.observed();
    assert_eq!(observed.responses, 1, "exactly one Response");
    assert_eq!(
        observed.last_response_code,
        Some(0),
        "the stub answered 0 = success"
    );
    assert_eq!(
        observed.fallbacks, 0,
        "a successful portal open must NOT reach the OS opener"
    );
    assert_eq!(
        observed.resubscribes, 0,
        "the stub returned the same path we computed, so no re-point was needed"
    );
    assert_eq!(
        open_uri.in_flight(),
        0,
        "the entry must be dropped once its Response arrived"
    );

    // --- the on-the-wire payload shape ---------------------------------------
    let s = seen.borrow().clone();
    assert_eq!(s.calls, 1, "exactly one OpenURI method call");
    assert_eq!(
        s.parent_window.as_deref(),
        Some(""),
        "parent window is the empty string (we have no window) — OpenURI.zig:274-276"
    );
    assert_eq!(
        s.uri.as_deref(),
        Some(std::str::from_utf8(URL).unwrap()),
        "the uri is sent verbatim"
    );
    assert_eq!(
        s.writable,
        Some(false),
        "`writable` option — OpenURI.zig:310-313"
    );
    assert_eq!(s.ask, Some(false), "`ask` option — OpenURI.zig:322-325");

    // The token in the payload MUST be the token in the path we subscribed to:
    // `handle_token` is what ties the two together (OpenURI.zig:295-301).
    let token = s.handle_token.expect("handle_token was sent");
    assert_eq!(
        token.len(),
        portal::TOKEN_HEX_LEN,
        "handle_token is zero-padded to a fixed width: {token}"
    );
    let path = s.response_path.expect("the stub emitted on a request path");
    assert_eq!(
        portal::parse_request_path_token(&path),
        usize::from_str_radix(&token, 16).ok(),
        "the request path's token and the payload's handle_token must agree \
         (path={path} token={token})"
    );

    // --- NEGATIVE CONTROL: no portal on the bus -> fallback, no hang ----------
    // Release the name. Now nothing owns org.freedesktop.portal.Desktop and there is
    // no .service file to activate, so the method call must error out and the port
    // must reach the OS opener instead of waiting forever.
    gio::bus_unown_name(owner_id);
    assert!(
        pump_until(Duration::from_secs(5), || {
            !portal_name_has_owner(&client).unwrap_or(true)
        }),
        "the stub never released {PORTAL_NAME}, so the negative control would be vacuous"
    );

    // A scheme with no handler, so the fallback's `xdg-open` cannot launch anything
    // heavy in the session; it exits immediately and is reaped by the opener's own
    // thread.
    open_uri
        .start(OpenUrl {
            kind: OpenUrlKind::Unknown,
            url: b"x-rustty-openuri-kat://no-portal",
        })
        .expect("start still issues the request; the failure is asynchronous");

    assert!(
        pump_until(Duration::from_secs(15), || open_uri.observed().fallbacks
            >= 1),
        "with no portal on the bus the OS-opener fallback was never reached \
         (observed={:?})",
        open_uri.observed(),
    );
    let observed = open_uri.observed();
    assert_eq!(
        observed.fallbacks, 1,
        "exactly one fallback for the one failed request"
    );
    assert_eq!(
        observed.responses, 1,
        "no second Response: the failed call produced no Response signal"
    );
    assert_eq!(
        open_uri.in_flight(),
        0,
        "the failed request's entry must be dropped, not leaked"
    );
    assert_eq!(
        s.calls, 1,
        "the stub saw no second call (it no longer owns the name)"
    );

    // `deinit` after the arms so the cleanup timer source cannot outlive the test.
    open_uri.deinit();
    assert_eq!(open_uri.in_flight(), 0);

    eprintln!(
        "KAT PASS: pre-subscription received the eager Response; no-portal arm fell back once"
    );
}
