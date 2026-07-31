//! PORTS: ghostty `src/apprt/gtk/portal/OpenURI.zig` — use D-Bus to call the XDG
//! Desktop Portal to open a URI.
//!
//! See <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.OpenURI.html#org-freedesktop-portal-openuri-openuri>
//!
//! PROTOCOL NOTE (the ordering that is load-bearing): the documented flow is "call
//! `OpenURI`, it returns a `Request` object path, then listen for that object's
//! `Response` signal". That flow is racy — a portal implementation may emit
//! `Response` before the caller has subscribed, and the signal is delivered exactly
//! once, so the response would be lost. The remedy (the same one
//! `class/global_shortcuts.rs` documents, and the one the reference implements here)
//! is to CONSTRUCT the expected request object path from our own unique bus name plus
//! the `handle_token` we are about to send, subscribe to `Response` on that path
//! FIRST, and only then issue the method call. Unlike `GlobalShortcuts` — where the
//! reference carries an unimplemented TODO — `OpenURI.zig` DOES additionally re-point
//! the subscription if the portal's reply names a different path (`:425-438`, for
//! older portals), and so does this port.
//!
//! # Threading / interior mutability
//!
//! The reference guards `entries` + `cleanup_timer` with a `std.Thread.Mutex`
//! (`OpenURI.zig:26`) and asserts it is held at each internal entry point. Every
//! caller here is the GTK main loop (the `gio` async callback, the `Response` signal
//! handler, and the `glib` timeout all dispatch on the thread that owns the main
//! context), so the port uses `RefCell` instead. Where the reference holds the mutex
//! ACROSS an outbound call (`openUrlFallback`, which spawns a process), this port
//! takes the entry out of the map and DROPS the borrow first: holding a `RefCell`
//! borrow across a re-entrant callback would panic where a re-entrant mutex lock
//! would deadlock, and neither is a behavior we want. The observable sequence is
//! unchanged on a single-threaded main loop.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

// `DBusConnection::{call, subscribe_to_signal}` are inherent; the prelude brings in
// `ToVariant` for the payload builder.
use gio::prelude::*;

use rustty_apprt_api::action::{OpenUrl, OpenUrlKind};

use super::{format_token, generate_token, parse_request_path_token, request_path};

/// PORTS: `OpenURI.zig:337-339` — the portal bus name, object path and interface.
const PORTAL_BUS_NAME: &str = "org.freedesktop.portal.Desktop";
const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const OPEN_URI_IFACE: &str = "org.freedesktop.portal.OpenURI";
/// PORTS: `OpenURI.zig:203` — the `Request` interface that carries `Response`.
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";

/// PORTS: `OpenURI.zig:502` `const cleanup_timeout = 30` — wait this many seconds
/// and then clean up any orphaned entries.
const CLEANUP_TIMEOUT_SECS: u64 = 30;

/// PORTS: `OpenURI.zig:103-111` `pub const Errors`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// Could not get a D-Bus connection (`OpenURI.zig:105`).
    DBusConnectionRequired,
    /// The D-Bus connection did not have a unique name. This _should_ be
    /// impossible, but is handled for safety's sake (`OpenURI.zig:108`).
    NoDBusUniqueName,
    /// A request with this token is already in flight — the reference's
    /// `putNoClobber` error (`OpenURI.zig:175`).
    DuplicateToken,
    /// DELIBERATE, DOCUMENTED DIVERGENCE (no reference counterpart): the URI is not
    /// valid UTF-8. The reference `dupeZ`s the raw bytes and hands the pointer to a
    /// `"s"` variant; D-Bus strings are REQUIRED to be UTF-8, so non-UTF-8 bytes
    /// would produce an invalid message rather than an open. Reporting it as an
    /// error routes the URL to the OS opener — which takes raw bytes — via the
    /// caller's existing error arm (`application.zig:2359-2362`), i.e. the byte
    /// string still gets opened, just not through the portal.
    UriNotUtf8,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::DBusConnectionRequired => "DBusConnectionRequired",
            Self::NoDBusUniqueName => "NoDBusUniqueName",
            Self::DuplicateToken => "DuplicateToken",
            Self::UriNotUtf8 => "UriNotUtf8",
        };
        f.write_str(s)
    }
}

/// Test/gate instrumentation: what the object has observed. The reference leaves no
/// observable at any of these points, so — following the `tick_count` /
/// `global_shortcut_trigger_count` precedent in `class/application.rs` — the port
/// adds counters. These are ALWAYS compiled (not `#[cfg(test)]`) because the live
/// portal gate runs the SHIPPING binary and needs them to prove it is not vacuous;
/// the cost is five `Cell`s per `Application`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Observed {
    /// `Response` signals received on a request path we subscribed to.
    pub responses: u64,
    /// The `response` code of the most recent `Response` (`0` ok, `1` cancelled,
    /// `2` ended unexpectedly).
    pub last_response_code: Option<u32>,
    /// How many times the non-portal OS opener was reached as a fallback.
    pub fallbacks: u64,
    /// How many `OpenURI` method-call replies were processed.
    pub call_replies: u64,
    /// How many times the reply named a different request path than the one we
    /// computed, so the subscription was re-pointed (`OpenURI.zig:433-438`).
    pub resubscribes: u64,
}

/// PORTS: `OpenURI.zig:79-101` `pub const Entry` — data about an in-flight call.
struct Entry {
    /// When the request started (`:81`).
    start: Instant,
    /// The token the portal uses to identify requests and responses (`:86`).
    token: usize,
    /// The "kind" of URI, needed to pass on to the fallback opener (`:89`).
    kind: OpenUrlKind,
    /// Our own copy of the URI: the method calls are asynchronous and the caller's
    /// slice may be gone by the time we need it (`:93`).
    uri: Vec<u8>,
    /// The `Response` subscription (`:96`). RAII: dropping it unsubscribes, which is
    /// the port's spelling of `signalUnsubscribe` (`:227`).
    subscription: Option<gio::SignalSubscription>,
}

/// The mutex-guarded half of the reference struct (`OpenURI.zig:26-35`).
#[derive(Default)]
struct State {
    /// `entries` (`:29`). PORT NOTE: the reference uses an
    /// `AutoArrayHashMapUnmanaged`; nothing depends on its iteration order (the
    /// cleanup sweep `swapRemove`s, which reorders anyway), so a `HashMap` is used.
    entries: HashMap<usize, Entry>,
    /// `cleanup_timer` (`:32`).
    cleanup_timer: Option<glib::SourceId>,
}

struct Inner {
    /// `dbus` (`OpenURI.zig:23`).
    dbus: RefCell<Option<gio::DBusConnection>>,
    state: RefCell<State>,
    /// `alive` (`OpenURI.zig:35`) — set false during shutdown so callbacks stop
    /// touching internal state.
    alive: Cell<bool>,
    observed: Cell<Observed>,
}

/// PORTS: `OpenURI.zig` — the whole struct.
///
/// PORT NOTE (no `app` backref): the reference holds `app: *App` (`:20`) for exactly
/// two purposes — the allocator, and `app.app.openUrlFallback`. The port needs no
/// allocator, and its fallback ([`crate::gtk::class::application::open_url_fallback`])
/// is app-independent (it reaches `rustty_os::open`), so no backref is stored. That
/// also avoids an `Rc` cycle between the `Application` and a `'static` glib closure.
#[derive(Clone)]
pub struct OpenUri {
    inner: Rc<Inner>,
}

impl Default for OpenUri {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenUri {
    /// PORTS: `OpenURI.zig:113-117` `init`.
    pub fn new() -> Self {
        Self {
            inner: Rc::new(Inner {
                dbus: RefCell::new(None),
                state: RefCell::new(State::default()),
                alive: Cell::new(true),
                observed: Cell::new(Observed::default()),
            }),
        }
    }

    /// PORTS: `OpenURI.zig:119-121` `setDbusConnection`.
    pub fn set_dbus_connection(&self, dbus: Option<&gio::DBusConnection>) {
        self.inner.dbus.replace(dbus.cloned());
    }

    /// Test/gate instrumentation — see [`Observed`].
    pub fn observed(&self) -> Observed {
        self.inner.observed.get()
    }

    /// How many requests are currently in flight. Used by the KAT to prove an entry
    /// was actually created (and later reaped), which the counters alone do not show.
    pub fn in_flight(&self) -> usize {
        self.inner.state.borrow().entries.len()
    }

    /// PORTS: `OpenURI.zig:123-142` `deinit`.
    pub fn deinit(&self) {
        let inner = &self.inner;

        // `if (!self.alive) return; self.alive = false;` (`:129-130`).
        if !inner.alive.get() {
            return;
        }
        inner.alive.set(false);

        {
            let mut state = inner.state.borrow_mut();
            // `self.stopCleanupTimer()` (`:132`).
            stop_cleanup_timer(&mut state);
            // `for (entries) |entry| { unsubscribe; destroy }` (`:134-137`) — each
            // `Entry`'s `Drop` unsubscribes and frees the uri copy.
            state.entries.clear();
        }

        // `self.dbus = null` (`:141`).
        inner.dbus.replace(None);
    }

    /// PORTS: `OpenURI.zig:146-187` `start`. Send the D-Bus method call to the XDG
    /// Desktop portal; the result is reported asynchronously.
    pub fn start(&self, value: OpenUrl<'_>) -> Result<(), Error> {
        let inner = &self.inner;

        // `const dbus = self.dbus orelse return error.DBusConnectionRequired` (`:148`).
        let dbus = inner.dbus.borrow().clone();
        let Some(dbus) = dbus else {
            return Err(Error::DBusConnectionRequired);
        };

        // See `Error::UriNotUtf8`. Checked BEFORE any state is mutated so the error
        // path leaves nothing behind (the reference's `errdefer` discipline).
        let uri = std::str::from_utf8(value.url).map_err(|_| Error::UriNotUtf8)?;

        // `portal.generateToken()` / `portal.getRequestPath(..)` (`:150-151`).
        let token = generate_token();
        let request_path = request_path(&dbus, token).ok_or(Error::NoDBusUniqueName)?;

        {
            let mut state = inner.state.borrow_mut();

            // `try self.entries.putNoClobber(alloc, token, entry)` (`:165-177`).
            if state.entries.contains_key(&token) {
                return Err(Error::DuplicateToken);
            }
            state.entries.insert(
                token,
                Entry {
                    start: Instant::now(),
                    token,
                    kind: value.kind,
                    uri: value.url.to_vec(),
                    subscription: None,
                },
            );

            // `self.startCleanupTimer()` (`:184`).
            start_cleanup_timer(inner, &mut state);

            // `self.subscribeToResponse(entry, dbus, request_path.ptr)` (`:185`).
            // BEFORE the method call — see the module's PROTOCOL NOTE.
            subscribe_to_response(inner, &mut state, token, &dbus, &request_path);
        }

        // `self.sendRequest(entry, dbus, request)` (`:186`).
        send_request(inner, &dbus, token, uri, &request_path);

        Ok(())
    }
}

/// PORTS: `OpenURI.zig:191-212` `subscribeToResponse`.
fn subscribe_to_response(
    inner: &Rc<Inner>,
    state: &mut State,
    token: usize,
    dbus: &gio::DBusConnection,
    request_path: &str,
) {
    // `if (entry.subscription != null) return` (`:199`).
    let Some(entry) = state.entries.get(&token) else {
        return;
    };
    if entry.subscription.is_some() {
        return;
    }

    let weak = Rc::downgrade(inner);
    let subscription = dbus.subscribe_to_signal(
        None,
        Some(REQUEST_IFACE),
        Some("Response"),
        Some(request_path),
        None,
        gio::DBusSignalFlags::NONE,
        move |signal| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            response_received(&inner, signal.object_path, signal.parameters);
        },
    );

    if let Some(entry) = state.entries.get_mut(&token) {
        entry.subscription = Some(subscription);
    }
}

/// PORTS: `OpenURI.zig:237-246` `failRequest` + `:248-254` `failRequestAndFallback`.
///
/// Takes the entry OUT of the map (so its `Drop` unsubscribes) and then reaches the
/// fallback opener with the borrow already released — see the module's threading note.
fn fail_request_and_fallback(inner: &Rc<Inner>, token: usize) {
    // `if (!self.alive) return null` (`:241`).
    if !inner.alive.get() {
        return;
    }
    let entry = inner.state.borrow_mut().entries.remove(&token);
    let Some(entry) = entry else {
        return;
    };
    fallback(inner, entry.kind, &entry.uri);
}

/// `self.app.app.openUrlFallback(kind, uri)` (`OpenURI.zig:253`).
fn fallback(inner: &Rc<Inner>, kind: OpenUrlKind, uri: &[u8]) {
    let mut observed = inner.observed.get();
    observed.fallbacks += 1;
    inner.observed.set(observed);
    crate::gtk::class::application::open_url_fallback(kind, uri);
}

/// PORTS: `OpenURI.zig:258-349` `sendRequest` (the payload builder + the call).
fn send_request(
    inner: &Rc<Inner>,
    dbus: &gio::DBusConnection,
    token: usize,
    uri: &str,
    request_path: &str,
) {
    // `(ssa{sv})`: parent window (empty = we have no window), the URI, and the
    // options vardict (`:266-330`).
    let options = glib::VariantDict::new(None);
    options.insert_value("handle_token", &format_token(token).to_variant());
    options.insert_value("writable", &false.to_variant());
    options.insert_value("ask", &false.to_variant());
    let payload =
        glib::Variant::tuple_from_iter(["".to_variant(), uri.to_variant(), options.end()]);

    // We're expecting an object path back from the method call (`:333`).
    let reply_ty = glib::VariantTy::new("(o)").expect("(o) is a valid variant type");

    let weak = Rc::downgrade(inner);
    let request_path = request_path.to_owned();
    dbus.call(
        Some(PORTAL_BUS_NAME),
        PORTAL_OBJECT_PATH,
        OPEN_URI_IFACE,
        "OpenURI",
        Some(&payload),
        Some(reply_ty),
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        move |res| {
            let Some(inner) = weak.upgrade() else {
                return;
            };
            request_callback(&inner, token, &request_path, res);
        },
    );
}

/// PORTS: `OpenURI.zig:354-439` `requestCallback`. Process the result of the original
/// method call. Receiving this result does not indicate the call SUCCEEDED, but it
/// may carry an error worth logging, and it names the portal's actual request path.
fn request_callback(
    inner: &Rc<Inner>,
    token: usize,
    request_path: &str,
    res: Result<glib::Variant, glib::Error>,
) {
    {
        let mut observed = inner.observed.get();
        observed.call_replies += 1;
        inner.observed.set(observed);
    }

    // `if (err_) |err| { log; failRequestAndFallback }` (`:382-389`).
    let reply = match res {
        Ok(reply) => reply,
        Err(err) => {
            eprintln!("error(gtk): open uri request failed={err}");
            fail_request_and_fallback(inner, token);
            return;
        }
    };

    // `if (reply.isOfType("(o)") == 0)` (`:398-405`).
    let reply_ty = glib::VariantTy::new("(o)").expect("(o) is a valid variant type");
    if !reply.is_type(reply_ty) {
        eprintln!("warn(gtk): reply from D-Bus method call does not contain an object path!");
        fail_request_and_fallback(inner, token);
        return;
    }

    // `reply.get("(&o)", &object_path)` (`:407-408`).
    let object_path = reply.child_value(0);
    let Some(object_path) = object_path.str() else {
        eprintln!("warn(gtk): open uri reply object path is not a string");
        fail_request_and_fallback(inner, token);
        return;
    };

    // `portal.parseRequestPathToken(..) orelse { .. }` (`:410-414`).
    let Some(reply_token) = parse_request_path_token(object_path) else {
        eprintln!("warn(gtk): unable to parse token from the object path {object_path}");
        fail_request_and_fallback(inner, token);
        return;
    };

    // `if (token != request.token)` (`:416-423`).
    if reply_token != token {
        eprintln!(
            "warn(gtk): open uri request returned mismatched token expected={token:x} actual={reply_token:x}"
        );
        fail_request_and_fallback(inner, token);
        return;
    }

    // `if (!self.alive) return` (`:428`).
    if !inner.alive.get() {
        return;
    }

    // `const entry = self.entries.get(token) orelse return` (`:430`) and
    // `if (mem.eql(request.request_path, object_path)) return` (`:431`).
    let dbus = inner.dbus.borrow().clone();
    let Some(dbus) = dbus else {
        return;
    };
    {
        let mut state = inner.state.borrow_mut();
        if !state.entries.contains_key(&token) {
            return;
        }
        if request_path == object_path {
            return;
        }

        eprintln!(
            "debug(gtk): updating open uri request path old={request_path} new={object_path}"
        );
        // `self.unsubscribeFromResponse(entry); self.subscribeToResponse(..)`
        // (`:437-438`). Dropping the old subscription IS the unsubscribe.
        if let Some(entry) = state.entries.get_mut(&token) {
            entry.subscription = None;
        }
        subscribe_to_response(inner, &mut state, token, &dbus, object_path);
    }

    let mut observed = inner.observed.get();
    observed.resubscribes += 1;
    inner.observed.set(observed);
}

/// PORTS: `OpenURI.zig:443-499` `responseReceived`. Handles the `Response` signal
/// from the portal, which carries the actual result of the method call.
fn response_received(inner: &Rc<Inner>, object_path: &str, params: &glib::Variant) {
    // `portal.parseRequestPathToken(..) orelse { log; return }` (`:459-462`).
    let Some(token) = parse_request_path_token(object_path) else {
        eprintln!("warn(gtk): open uri invalid object path: {object_path}");
        return;
    };

    // `if (!self.alive) return` (`:467`).
    if !inner.alive.get() {
        return;
    }

    // `const entry = self.entries.fetchSwapRemove(token) orelse { log; return }`
    // (`:469-472`), and `self.unsubscribeFromResponse(entry)` (`:476`) — taking the
    // entry out drops its subscription. The borrow is released before any fallback.
    let entry = inner.state.borrow_mut().entries.remove(&token);
    let Some(entry) = entry else {
        eprintln!("warn(gtk): open uri no entry for token {token:x}");
        return;
    };

    // `params.get("(u@a{sv})", &response, &results)` (`:481`).
    //
    // DELIBERATE DIVERGENCE (the same one `global_shortcuts.rs` documents): the
    // fallback code is 2 ("ended unexpectedly"), not 0. The oracle's `params.get`
    // leaves `response` at its zero value on a malformed payload, which reads as
    // 0 = success; treating an unparseable Response as success would report an open
    // that never happened.
    let response: u32 = params.child_value(0).get().unwrap_or(2);

    {
        let mut observed = inner.observed.get();
        observed.responses += 1;
        observed.last_response_code = Some(response);
        inner.observed.set(observed);
    }

    match response {
        // `0 => log.debug("open uri successful")` (`:484-486`).
        0 => eprintln!("debug(gtk): open uri successful token={token:x}"),
        // `1 => log.debug("cancelled by the user")` (`:487-489`).
        1 => eprintln!("debug(gtk): open uri request was cancelled by the user token={token:x}"),
        // `2 => { log.warn; openUrlFallback }` (`:490-493`).
        2 => {
            eprintln!("warn(gtk): open uri request ended unexpectedly token={token:x}");
            fallback(inner, entry.kind, &entry.uri);
        }
        // `else => { log.err; openUrlFallback }` (`:494-497`).
        other => {
            eprintln!("error(gtk): open uri unrecognized response code={other}");
            fallback(inner, entry.kind, &entry.uri);
        }
    }
}

/// PORTS: `OpenURI.zig:506-515` `stopCleanupTimer`.
fn stop_cleanup_timer(state: &mut State) {
    if let Some(timer) = state.cleanup_timer.take() {
        timer.remove();
    }
}

/// PORTS: `OpenURI.zig:520-525` `startCleanupTimer`. If there is already a timer it
/// is stopped and replaced.
fn start_cleanup_timer(inner: &Rc<Inner>, state: &mut State) {
    stop_cleanup_timer(state);

    let weak = Rc::downgrade(inner);
    // `glib.timeoutAddSeconds(cleanup_timeout + 1, cleanup, self)` (`:524`) — one
    // second past the timeout so an entry created just now is definitely stale.
    let source =
        glib::timeout_add_seconds_local_once((CLEANUP_TIMEOUT_SECS + 1) as u32, move || {
            if let Some(inner) = weak.upgrade() {
                cleanup(&inner);
            }
        });
    state.cleanup_timer = Some(source);
}

/// PORTS: `OpenURI.zig:529-565` `cleanup`. Frees any entries that failed to get a
/// response in a timely manner, reaching the fallback opener for each.
///
/// The reference returns `SOURCE_REMOVE`, so this is a one-shot; the port uses
/// `timeout_add_seconds_local_once`, whose source removes itself. The `cleanup_timer`
/// field is cleared first (`:540`) so a later `start` installs a fresh timer instead
/// of trying to remove a dead source id.
fn cleanup(inner: &Rc<Inner>) {
    let timed_out: Vec<(OpenUrlKind, Vec<u8>)> = {
        let mut state = inner.state.borrow_mut();
        // `self.cleanup_timer = null` (`:540`). The source has already removed
        // itself (one-shot), so the id is simply dropped — `glib::SourceId` has no
        // `Drop`, so dropping it detaches nothing and cannot double-remove.
        state.cleanup_timer = None;

        // `if (!self.alive) return SOURCE_REMOVE` (`:541`).
        if !inner.alive.get() {
            return;
        }

        let now = Instant::now();
        let timeout = Duration::from_secs(CLEANUP_TIMEOUT_SECS);
        // `while (true) { for (entries) |entry| if (stale) { .. } }` (`:550-562`) —
        // the reference restarts the scan after each `swapRemove` because removing
        // during iteration invalidates the position. Collecting first is the same
        // set of entries, and lets the fallback run with the borrow released.
        let stale: Vec<usize> = state
            .entries
            .values()
            .filter(|e| now.duration_since(e.start) > timeout)
            .map(|e| e.token)
            .collect();

        stale
            .into_iter()
            .filter_map(|token| {
                eprintln!("warn(gtk): open uri request timed out token={token:x}");
                state
                    .entries
                    .remove(&token)
                    .map(|entry| (entry.kind, entry.uri))
            })
            .collect()
    };

    for (kind, uri) in timed_out {
        fallback(inner, kind, &uri);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With no D-Bus connection set, `start` reports `DBusConnectionRequired` — the
    /// reference's `self.dbus orelse return error.DBusConnectionRequired`
    /// (`OpenURI.zig:148`), which is the arm `application.zig:2359`'s `catch` turns
    /// into the OS-opener fallback.
    ///
    /// Falsifiable: it asserts the specific error AND that no entry was created, so
    /// a `start` that silently succeeded, or that registered an entry before
    /// checking the connection, fails.
    #[test]
    fn open_uri_start_without_dbus_is_an_error() {
        let open_uri = OpenUri::new();
        let err = open_uri
            .start(OpenUrl {
                kind: OpenUrlKind::Unknown,
                url: b"https://example.invalid/",
            })
            .expect_err("no dbus connection was set");
        assert_eq!(err, Error::DBusConnectionRequired);
        assert_eq!(open_uri.in_flight(), 0);
        assert_eq!(open_uri.observed(), Observed::default());
    }

    /// The connection check runs FIRST, before the URI is inspected at all — so a
    /// non-UTF-8 URI with no connection set reports `DBusConnectionRequired`, not
    /// `UriNotUtf8`.
    ///
    /// PORTS: `oracle@main:src/apprt/gtk/portal/OpenURI.zig:149` — `start` opens
    /// with `const dbus = self.dbus orelse return error.DBusConnectionRequired;`
    /// and performs NO UTF-8 validation anywhere (Zig carries `value.url` as
    /// `[]const u8` and passes the bytes straight through). [`Error::UriNotUtf8`]
    /// is therefore a rustty-only guard, added because the D-Bus `"s"` variant
    /// requires UTF-8; R1 pins it AFTER the connection check.
    ///
    /// An earlier version of this test asserted the opposite ordering and FAILED
    /// on Linux from the day it landed (WS19 T8) — its rationale ("the UTF-8 check
    /// must run first") had no oracle basis. The `UriNotUtf8` path itself is
    /// exercised by the live-gated portal KAT, where a connection exists.
    #[test]
    fn open_uri_connection_check_precedes_uri_validation() {
        let open_uri = OpenUri::new();
        let err = open_uri
            .start(OpenUrl {
                kind: OpenUrlKind::Unknown,
                url: b"https://example.invalid/\xff\xfe",
            })
            .expect_err("no dbus connection was set");
        assert_eq!(err, Error::DBusConnectionRequired);
        assert_eq!(open_uri.in_flight(), 0);
    }

    /// `deinit` is idempotent (`OpenURI.zig:129-130` `if (!self.alive) return`) and
    /// clears the connection, so a post-`deinit` `start` cannot issue a call.
    #[test]
    fn open_uri_deinit_is_idempotent_and_disarms() {
        let open_uri = OpenUri::new();
        open_uri.deinit();
        open_uri.deinit();
        assert_eq!(
            open_uri.start(OpenUrl {
                kind: OpenUrlKind::Unknown,
                url: b"https://example.invalid/",
            }),
            Err(Error::DBusConnectionRequired),
        );
    }
}
