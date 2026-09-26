//! Use DBus to call the XDG Desktop Portal to open an URI.
//!
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

/// `const log = std.log.scoped(.openuri)` (`OpenURI.zig:18`).
///
/// S59-F6: every emission in this file previously printed `(gtk)` — the wrong
/// scope entirely — through a raw `eprintln!`. Carried as the `log::` facade's
/// `target:` (`DECISION-logging.md` §6r4). No brand token, nothing to rebrand.
const LOG_SCOPE: &str = "openuri";

/// PORTS: `OpenURI.zig:337-339` — the portal bus name, object path and interface.
const PORTAL_BUS_NAME: &str = "org.freedesktop.portal.Desktop";
const PORTAL_OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
const OPEN_URI_IFACE: &str = "org.freedesktop.portal.OpenURI";
/// PORTS: `OpenURI.zig:203` — the `Request` interface that carries `Response`.
const REQUEST_IFACE: &str = "org.freedesktop.portal.Request";

/// Wait this number of seconds and then clean up any orphaned entries.
///
/// PORTS: `OpenURI.zig:502` `const cleanup_timeout = 30`.
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
    /// The URI is not valid UTF-8 (no reference counterpart).
    ///
    /// PORT NOTE: reference defect (R13), `UPSTREAM-DEFECTS.md` UD171. The
    /// reference `dupeZ`s the URL's raw bytes and passes them to
    /// `builder.add("s", entry.uri.ptr)` (`OpenURI.zig:279`), i.e.
    /// `g_variant_new_string`, whose precondition is valid UTF-8. A non-UTF-8 URL
    /// (a terminal hyperlink, OSC 8, carries arbitrary bytes) violates it:
    /// MEASURED on GLib 2.88.2 with a C probe that builds `sendRequest`'s payload
    /// the reference's way, GLib logs `GLib-CRITICAL: g_variant_new_string():
    /// requires valid UTF-8` and substitutes the string `[Invalid UTF-8]`, so the
    /// portal is asked to open `[Invalid UTF-8]`. A failed `g_return_*_if_fail` is
    /// a programmer error after which, in GLib's own words, the function "should be
    /// considered to have undefined behaviour"; with the checks compiled out
    /// (`G_DISABLE_CHECKS`) the invalid string would reach the D-Bus serializer,
    /// whose UTF-8 `g_assert` aborts the process (measured for the IPC path, UD170).
    /// The port checks the bytes first and reports this error, which the caller
    /// handles with the reference's own failure path for `start`: `log.err("unable
    /// to open uri err={}")` then `openUrlFallback` (`application.zig:2845-2849`),
    /// so the URL is still opened, by the OS opener, with its raw bytes. It is
    /// checked BEFORE any state is mutated, so nothing is left behind (no entry, no
    /// subscription, no cleanup timer, no portal call).
    UriNotUtf8,
    // PORT NOTE: the reference's `TimerUnavailable` (`OpenURI.zig:110-111`) has no
    // variant here: `Instant::now()` cannot fail, where `std.Io.Timestamp` can.
    // Its doc, verbatim:
    // The system was unable to give us the time.
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

/// Data about any in-flight calls to the portal.
///
/// PORTS: `OpenURI.zig:79-101` `pub const Entry`.
struct Entry {
    /// When the request started.
    start: Instant,
    /// A token used by the portal to identify requests and responses. The
    /// actual format of the token does not really matter as long as it can be
    /// used as part of a D-Bus object path. `usize` was chosen since it's easy
    /// to hash and to generate random tokens.
    token: usize,
    /// The "kind" of URI. Unused here, but we may need to pass it on to the
    /// fallback URL opener if the D-Bus method fails.
    kind: OpenUrlKind,
    /// A copy of the URI that we are opening. We need our own copy since the
    /// method calls are asynchronous and the original may have been freed by
    /// the time we need it.
    uri: Vec<u8>,
    /// Used to manage a subscription to a D-Bus signal, which is how the XDG
    /// Portal reports results of the method call.
    ///
    /// RAII: dropping it unsubscribes, which is the port's spelling of
    /// `signalUnsubscribe` (`:227`).
    subscription: Option<gio::SignalSubscription>,
}

/// The mutex-guarded half of the reference struct (`OpenURI.zig:26-35`).
///
/// PORT NOTE: the reference's `mutex` field is this `RefCell` (see the module's
/// threading note). Its doc, verbatim:
/// Mutex to protect modification of the entries map or the cleanup timer.
#[derive(Default)]
struct State {
    /// Map to store data about any in-flight calls to the portal.
    ///
    /// `entries` (`:29`). PORT NOTE: the reference uses an
    /// `AutoArrayHashMapUnmanaged`; nothing depends on its iteration order (the
    /// cleanup sweep `swapRemove`s, which reorders anyway), so a `HashMap` is used.
    entries: HashMap<usize, Entry>,
    /// Used to manage a timer to clean up any orphan entries in the map.
    cleanup_timer: Option<glib::SourceId>,
}

struct Inner {
    /// Connection to the D-Bus session bus that we'll use for all of our messaging.
    dbus: RefCell<Option<gio::DBusConnection>>,
    state: RefCell<State>,
    /// Set to false during shutdown so callbacks stop touching internal state.
    alive: Cell<bool>,
    observed: Cell<Observed>,
}

/// PORTS: `OpenURI.zig` — the whole struct.
///
/// PORT NOTE (no `app` backref): the reference holds `app: *App` (`:20`), documented
/// as:
/// The GTK app that we "belong" to.
/// It holds it for exactly
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

    /// Send the D-Bus method call to the XDG Desktop portal. The result of the
    /// method call will be reported asynchronously.
    ///
    /// PORTS: `OpenURI.zig:146-187` `start`.
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

            // Create an entry that is used to track the results of the D-Bus method
            // call.
            //
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

/// Subscribe to the D-Bus signal that will contain the results of our method
/// call to the portal. This must be called with the mutex locked.
///
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

/// Send the D-Bus method call to the portal. The mutex must be locked when this
/// is called.
///
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
    //
    // parent window - empty string means we have no window
    // URI to open
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

/// UNRESTORED SITES in this function (task-14d-3), all structural:
/// * `:369` `Open URI request finished without a D-Bus source object` and
///   `:373` `... with an unexpected source object` — the reference recovers the
///   connection by downcasting an untyped `?*gobject.Object` handed to a C
///   callback. This port's closure captures the typed connection directly, so
///   neither failure can be expressed.
/// * `:393` `D-Bus method call returned a null value!` — the async call's
///   `Result` is `Ok(Variant)`; there is no null reply to test for.
///
/// Process the result of the original method call. Receiving this result does
/// not indicate that the that the method call succeeded but it may contain an
/// error message that is useful to log for debugging purposes.
///
/// PORTS: `OpenURI.zig:354-439` `requestCallback`. It also names the portal's
/// actual request path.
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
            // log.err("Open URI request failed={s} ({})", .{ err.f_message orelse "(unknown)", err.f_code });
            //
            // S59-F6: the code was dropped. glib::Error's Display IS the
            // message (`err.f_message`), and `.kind()`/the raw code is exposed
            // by `glib::Error::code()`.
            log::error!(
                target: LOG_SCOPE,
                "Open URI request failed={} ({})",
                err.message(),
                err.code(),
            );
            fail_request_and_fallback(inner, token);
            return;
        }
    };

    // `if (reply.isOfType("(o)") == 0)` (`:398-405`).
    let reply_ty = glib::VariantTy::new("(o)").expect("(o) is a valid variant type");
    if !reply.is_type(reply_ty) {
        // log.warn("Reply from D-Bus method call does not contain an object path!", .{});
        log::warn!(
            target: LOG_SCOPE,
            "Reply from D-Bus method call does not contain an object path!"
        );
        fail_request_and_fallback(inner, token);
        return;
    }

    // `reply.get("(&o)", &object_path)` (`:407-408`).
    let object_path = reply.child_value(0);
    let Some(object_path) = object_path.str() else {
        // PORT NOTE: the reference reads the object path with
        // `reply.get("(&o)", &object_path)`, which cannot fail once
        // `isOfType("(o)")` has passed; `glib::Variant::str()` is `Option`, so
        // this arm is Rust-only. It keeps the reference's disposition for a
        // malformed reply (fail + fall back) and is deliberately NOT given the
        // text of a reference site it does not correspond to.
        log::warn!(target: LOG_SCOPE, "open uri reply object path is not a string");
        fail_request_and_fallback(inner, token);
        return;
    };

    // `portal.parseRequestPathToken(..) orelse { .. }` (`:410-414`).
    let Some(reply_token) = parse_request_path_token(object_path) else {
        // log.warn("Unable to parse token from the object path {s}", .{object_path});
        log::warn!(
            target: LOG_SCOPE,
            "Unable to parse token from the object path {object_path}"
        );
        fail_request_and_fallback(inner, token);
        return;
    };

    // `if (token != request.token)` (`:416-423`).
    if reply_token != token {
        // log.warn("Open URI request returned mismatched token expected={x} actual={x}", .{ request.token, token });
        log::warn!(
            target: LOG_SCOPE,
            "Open URI request returned mismatched token expected={token:x} actual={reply_token:x}"
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

        // log.debug("updating open uri request path old={s} new={s}", .{ request.request_path, object_path });
        log::debug!(
            target: LOG_SCOPE,
            "updating open uri request path old={request_path} new={object_path}"
        );
        // `self.unsubscribeFromResponse(entry); self.subscribeToResponse(..)`
        // (`:437-438`). Dropping the old subscription IS the unsubscribe.
        // `unsubscribeFromResponse`'s doc (`:215-217`), verbatim:
        // Unsubscribe to the D-Bus signal that contains the result of the method call.
        // This will prevent a response from being processed multiple times. This must
        // be called when the mutex is locked.
        //
        // UNRESTORED SITE `OpenURI.zig:225` (task-14d-3):
        // `unsubscribeFromResponse` warns `unable to unsubscribe open uri
        // response without dbus connection` when `self.dbus` is null, because
        // the reference needs the connection to call `signalUnsubscribe`. Here
        // the subscription is RAII — dropping it unsubscribes — so no
        // connection is needed and the failure cannot occur.
        if let Some(entry) = state.entries.get_mut(&token) {
            entry.subscription = None;
        }
        subscribe_to_response(inner, &mut state, token, &dbus, object_path);
    }

    let mut observed = inner.observed.get();
    observed.resubscribes += 1;
    inner.observed.set(observed);
}

/// UNRESTORED SITE `OpenURI.zig:454` (task-14d-3): `OpenURI response received
/// with null userdata` guards the `@ptrCast(ud orelse ...)` of a C callback's
/// user-data pointer. This handler is a Rust closure over an `Rc<Inner>`, so
/// there is no nullable user-data to check. `:532`'s `cleanup called with null
/// userdata` is the same shape in [`cleanup`].
///
/// Handle the response signal from the portal. This should contain the actual
/// results of the method call (success or failure).
///
/// PORTS: `OpenURI.zig:443-499` `responseReceived`.
fn response_received(inner: &Rc<Inner>, object_path: &str, params: &glib::Variant) {
    // `portal.parseRequestPathToken(..) orelse { log; return }` (`:459-462`).
    let Some(token) = parse_request_path_token(object_path) else {
        // log.warn("invalid object path: {s}", .{std.mem.span(object_path)});
        log::warn!(target: LOG_SCOPE, "invalid object path: {object_path}");
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
        // log.warn("no entry for token {x}", .{token});
        log::warn!(target: LOG_SCOPE, "no entry for token {token:x}");
        return;
    };

    // var response: u32 = 0;
    // var results: ?*glib.Variant = null;
    // defer if (results) |variant| variant.unref();
    // params.get("(u@a{sv})", &response, &results);
    //
    // CORRECTION (task 20, S59-F3): this used to read
    // `params.child_value(0).get().unwrap_or(2)` under a note that said "DELIBERATE
    // DIVERGENCE (the same one `global_shortcuts.rs` documents): the fallback code is
    // 2 ("ended unexpectedly"), not 0". The divergence did not only change a default:
    // glib-rs's `child_value` ASSERTS `index < n_children()`, so a `Response` signal
    // with an empty `()` body — which any bus peer can send, the subscription has no
    // sender filter (`OpenURI.zig:202-212`) — panicked inside the D-Bus dispatch on
    // the GTK main loop. The reference's `params.get` is `g_variant_get`, which on a
    // type mismatch emits a GLib critical and writes nothing, leaving `response` at
    // 0. The same C call is made here, so both the critical and the zero are the
    // reference's own.
    let mut response: u32 = 0;
    let mut results: *mut glib::ffi::GVariant = std::ptr::null_mut();
    // SAFETY: `params` is a live `GVariant` for the call. The format string is a
    // valid NUL-terminated `g_variant_get` format whose two conversions (`u`, `@a{sv}`)
    // take a `*mut u32` and a `*mut *mut GVariant` respectively, which is what is
    // passed. On a mismatch GLib writes neither.
    unsafe {
        glib::ffi::g_variant_get(
            glib::translate::ToGlibPtr::<*mut glib::ffi::GVariant>::to_glib_none(params).0,
            c"(u@a{sv})".as_ptr(),
            &mut response as *mut u32,
            &mut results as *mut *mut glib::ffi::GVariant,
        );
    }
    // `defer if (results) |variant| variant.unref()`: `@a{sv}` hands back a new
    // reference, which this handler does not otherwise use.
    if !results.is_null() {
        // SAFETY: `results` is the owned reference `g_variant_get` returned.
        unsafe { glib::ffi::g_variant_unref(results) };
    }

    {
        let mut observed = inner.observed.get();
        observed.responses += 1;
        observed.last_response_code = Some(response);
        inner.observed.set(observed);
    }

    match response {
        // `0 => log.debug("open uri successful")` (`:484-486`).
        // log.debug("open uri successful", .{});
        //
        // S59-F6: this and the `cancelled` line below were printed
        // UNCONDITIONALLY. Zig's `std.log` machinery erases `log.debug` at
        // comptime in a non-Debug build; `log::debug!` is gated by the
        // installed backend's `log::max_level()`, which
        // `install_app_log_backend` sets to `Info` off a debug build — the
        // direct analogue of the reference's `.log_level` default. The `token=`
        // suffix each of these four carried is gone too: it is not in the
        // reference's message text.
        0 => log::debug!(target: LOG_SCOPE, "open uri successful"),
        // `1 => log.debug("open uri request was cancelled by the user", .{})`
        // (`:488-490`). Quoted VERBATIM: an abbreviated reproduction here made
        // G-LOGFILE's content check pair this emission with the `0 =>` arm's
        // leftover comment and report a phantom divergence.
        1 => log::debug!(target: LOG_SCOPE, "open uri request was cancelled by the user"),
        // `2 => { log.warn; openUrlFallback }` (`:490-493`).
        2 => {
            // log.warn("open uri request ended unexpectedly", .{});
            log::warn!(target: LOG_SCOPE, "open uri request ended unexpectedly");
            fallback(inner, entry.kind, &entry.uri);
        }
        // `else => { log.err; openUrlFallback }` (`:494-497`).
        other => {
            // log.err("unrecognized response code={}", .{response});
            log::error!(target: LOG_SCOPE, "unrecognized response code={other}");
            fallback(inner, entry.kind, &entry.uri);
        }
    }
}

/// If there is an active cleanup timer, cancel it. This must be called with the
/// mutex locked
///
/// PORTS: `OpenURI.zig:506-515` `stopCleanupTimer`.
//
// PORT NOTE (task-14d-3, UNRESTORED SITE `OpenURI.zig:512`): the reference tests
// `if (glib.Source.remove(timer) == 0) log.warn("unable to remove cleanup timer
// source={d}", .{timer})`. glib-rs's `SourceId::remove` returns `()` — it has no
// failure report — so there is no arm for the warning.
fn stop_cleanup_timer(state: &mut State) {
    if let Some(timer) = state.cleanup_timer.take() {
        timer.remove();
    }
}

/// Start a timer to clean up any entries that have not received a timely
/// response. If there is already a timer it will be stopped and replaced with a
/// new one. This must be called with the mutex locked.
///
/// PORTS: `OpenURI.zig:520-525` `startCleanupTimer`.
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

/// The cleanup timer is used to free up any entries that may have failed
/// to get a response in a timely manner.
///
/// PORTS: `OpenURI.zig:529-565` `cleanup`, reaching the fallback opener for each.
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
                // log.warn("open uri request timed out token={x}", .{entry.token});
                log::warn!(target: LOG_SCOPE, "open uri request timed out token={token:x}");
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

    use crate::log_capture;

    /// Build the `(u@a{sv})` payload the portal's `Response` signal carries
    /// (`OpenURI.zig:481`).
    fn response_params(code: u32) -> glib::Variant {
        glib::Variant::tuple_from_iter([code.to_variant(), glib::VariantDict::new(None).end()])
    }

    /// S59-F6, the SCOPE half. Every emission in this file printed `(gtk)`;
    /// the reference's scope is `openuri` (`OpenURI.zig:18`). The message text
    /// had drifted too — the port had prefixed it with `open uri `.
    ///
    /// Asserts the FULL captured record vector, so an extra record, a missing
    /// one, a wrong level or a wrong scope all fail.
    #[test]
    fn response_with_an_unparseable_object_path_warns_under_the_openuri_scope() {
        let cap = log_capture::start();
        let open_uri = OpenUri::new();
        response_received(
            &open_uri.inner,
            "/not/a/request/path/zz",
            &response_params(0),
        );
        let messages = cap.messages();
        drop(cap);
        assert_eq!(
            vec![(
                log::Level::Warn,
                "openuri".to_string(),
                "invalid object path: /not/a/request/path/zz".to_string(),
            )],
            messages
        );
    }

    /// S59-F6, the LEVEL half. `OpenURI.zig:484-497` uses FOUR different
    /// levels for the four response codes; the port printed all of them
    /// through `eprintln!`, i.e. unconditionally and with the level baked into
    /// the text. Codes 0 and 1 are `log.debug`; this pins both.
    ///
    /// Codes 2 and `else` are deliberately NOT driven here: their reference
    /// arms call `openUrlFallback`, which spawns the OS opener. Their levels
    /// (warn / err) are pinned by reading, not by launching a browser from a
    /// unit test — stated rather than quietly skipped.
    #[test]
    fn successful_and_cancelled_responses_log_at_debug() {
        let cap = log_capture::start();
        let open_uri = OpenUri::new();

        for (token, code) in [(0x11usize, 0u32), (0x22usize, 1u32)] {
            open_uri.inner.state.borrow_mut().entries.insert(
                token,
                Entry {
                    start: Instant::now(),
                    token,
                    kind: OpenUrlKind::Unknown,
                    uri: b"https://example.invalid/".to_vec(),
                    subscription: None,
                },
            );
            let path = super::super::build_request_path("1_1", token);
            response_received(&open_uri.inner, &path, &response_params(code));
        }

        let messages = cap.messages();
        drop(cap);
        assert_eq!(
            vec![
                (
                    log::Level::Debug,
                    "openuri".to_string(),
                    "open uri successful".to_string(),
                ),
                (
                    log::Level::Debug,
                    "openuri".to_string(),
                    "open uri request was cancelled by the user".to_string(),
                ),
            ],
            messages
        );
    }

    /// S59-F3. A `Response` whose body is not `(ua{sv})` must not take the main loop
    /// down. The reference reads it with `params.get("(u@a{sv})", ..)`
    /// (`OpenURI.zig:478-481`): `g_variant_get` rejects the mismatched value, writes
    /// nothing, and `response` keeps its initial 0 — so the handler logs
    /// `open uri successful` and returns, with no fallback.
    ///
    /// Two malformed shapes, because they failed differently before the fix: the
    /// empty tuple `()` PANICKED in `child_value(0)`, and a wrong-typed first child
    /// `(sa{sv})` took the port-only `unwrap_or(2)` fallback arm. The positive
    /// assertion is the full log vector plus `last_response_code`, so a run that
    /// never reached the switch cannot pass.
    #[test]
    fn malformed_response_bodies_read_as_code_zero_without_panicking() {
        let cap = log_capture::start();
        let open_uri = OpenUri::new();

        let empty = glib::Variant::tuple_from_iter(std::iter::empty::<glib::Variant>());
        let wrong_first =
            glib::Variant::tuple_from_iter(["x".to_variant(), glib::VariantDict::new(None).end()]);
        for (token, params) in [(0x31usize, empty), (0x32usize, wrong_first)] {
            open_uri.inner.state.borrow_mut().entries.insert(
                token,
                Entry {
                    start: Instant::now(),
                    token,
                    kind: OpenUrlKind::Unknown,
                    uri: b"https://example.invalid/".to_vec(),
                    subscription: None,
                },
            );
            let path = super::super::build_request_path("1_1", token);
            response_received(&open_uri.inner, &path, &params);
            assert_eq!(open_uri.observed().last_response_code, Some(0));
        }

        let messages = cap.messages();
        drop(cap);
        assert_eq!(open_uri.observed().responses, 2);
        assert_eq!(open_uri.observed().fallbacks, 0);
        assert_eq!(
            vec![
                (
                    log::Level::Debug,
                    "openuri".to_string(),
                    "open uri successful".to_string(),
                ),
                (
                    log::Level::Debug,
                    "openuri".to_string(),
                    "open uri successful".to_string(),
                ),
            ],
            messages
        );
    }

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
    /// must run first") had no oracle basis.
    ///
    /// CORRECTION (task T-D, S59-F5): this doc went on "The `UriNotUtf8` path
    /// itself is exercised by the live-gated portal KAT, where a connection
    /// exists." No test or gate in the tree named `UriNotUtf8` (grep of
    /// `crates/*/tests` and `.port/tools`, 2026-09-23). It is driven now by
    /// `open_uri_rejects_a_non_utf8_uri_before_any_state`, below.
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

    /// A private message bus for one test: `dbus-daemon --session --nofork
    /// --print-address` on its own socket, connected WITHOUT touching the process
    /// environment (`DBUS_SESSION_BUS_ADDRESS`), which other tests in the same
    /// binary read concurrently. The daemon is killed when the guard drops.
    struct PrivateBus {
        daemon: std::process::Child,
        conn: gio::DBusConnection,
    }

    impl PrivateBus {
        fn start() -> PrivateBus {
            use std::io::BufRead;
            let mut daemon = std::process::Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address"])
                .stdout(std::process::Stdio::piped())
                .spawn()
                .expect("dbus-daemon must be installed for this test");
            let mut address = String::new();
            std::io::BufReader::new(daemon.stdout.as_mut().expect("piped stdout"))
                .read_line(&mut address)
                .expect("dbus-daemon prints its address");
            let conn = gio::DBusConnection::for_address_sync(
                address.trim(),
                gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                    | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                None,
                gio::Cancellable::NONE,
            )
            .expect("connect to the private bus");
            assert!(
                conn.unique_name().is_some(),
                "a message-bus connection has a unique name"
            );
            PrivateBus { daemon, conn }
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.conn.close_sync(gio::Cancellable::NONE);
            let _ = self.daemon.kill();
            let _ = self.daemon.wait();
        }
    }

    /// S59-F5 / UD171 (R13): with a connection, a non-UTF-8 URL is refused with
    /// `UriNotUtf8` BEFORE any state exists: no entry, no request made, no
    /// cleanup timer. The reference would call `g_variant_new_string` on the bytes
    /// (a GLib precondition violation, measured: `GLib-CRITICAL` and the portal
    /// asked to open `[Invalid UTF-8]`). Positive control on the same connection:
    /// a UTF-8 URL does start a request (one entry in flight).
    #[test]
    fn open_uri_rejects_a_non_utf8_uri_before_any_state() {
        // CORRECTION (task L-2): the positive control's request is an ASYNC
        // `DBusConnection::call` whose reply callback attaches to the calling
        // thread's thread-default main context. With none pushed that was the
        // process-global default context, so another test iterating it
        // (`hotkeys_wire_path_bound_pressed_denied_revoked`) dispatched this
        // test's callback on its own thread: glib-rs's thread guard panicked in the
        // C callback and aborted the test binary, in about one full run in two on
        // the VM. The test now runs on a private context of its own.
        let ctx = glib::MainContext::new();
        ctx.with_thread_default(|| {
            let bus = PrivateBus::start();
            let open_uri = OpenUri::new();
            open_uri.set_dbus_connection(Some(&bus.conn));

            assert_eq!(
                open_uri.start(OpenUrl {
                    kind: OpenUrlKind::Unknown,
                    url: b"https://example.invalid/caf\xe9",
                }),
                Err(Error::UriNotUtf8),
            );
            assert_eq!(open_uri.in_flight(), 0, "no entry for the refused URL");
            assert!(
                open_uri.inner.state.borrow().cleanup_timer.is_none(),
                "no cleanup timer for the refused URL"
            );
            assert_eq!(open_uri.observed(), Observed::default());

            // Positive control: the same object and connection DO start a request for
            // a UTF-8 URL, so the refusal above is the UTF-8 check, not a dead object.
            assert_eq!(
                open_uri.start(OpenUrl {
                    kind: OpenUrlKind::Unknown,
                    url: b"https://example.invalid/cafe",
                }),
                Ok(()),
            );
            assert_eq!(open_uri.in_flight(), 1);
            open_uri.deinit();
        })
        .expect("acquire the test's own main context");
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
