#!/usr/bin/env bash
# ws19_portal_openuri_gate.sh — WS19 T8 live gate (LINUX / VM ONLY)
#
# Proves the ported XDG `OpenURI` portal call site (`crates/rustty-apprt/src/gtk/portal/`,
# PORTS ghostty `src/apprt/gtk/portal.zig` + `portal/OpenURI.zig`) completes a real
# round-trip against the session's own `xdg-desktop-portal`, and that the traffic it
# puts on the bus has the shape the reference specifies.
#
# Three layers, each with a stated negative control:
#
#   1. STUB-PORTAL ORDERING (private, activation-free bus)
#      `--test gtk_portal_openuri`. The stub emits `Response` BEFORE returning the
#      method reply, so it can only be observed by a client that pre-subscribed.
#      Negative control: the same test's second arm releases the portal bus name and
#      asserts the OS-opener fallback fires exactly once instead of hanging.
#      Demonstrated FAIL arm (recorded in the test's module docs): subscribing from the
#      reply callback instead yields `responses: 0`.
#
#   2. LIVE ROUND-TRIP (the real session bus)
#      `--test gtk_portal_openuri_live`. Asserts a `Response` with code 0 arrives on the
#      request object path the port computed for itself, and the fallback was NOT used.
#      Negative control: `NO_HANDLER=1` (exercised below) removes the scheme handler, so
#      the portal cannot resolve the URI and the `code == 0` assertion fails.
#
#   3. WIRE SHAPE (dbus-monitor)
#      The captured traffic must contain the `OpenURI` method call to
#      `org.freedesktop.portal.Desktop` carrying OUR `handle_token`, and the `Response`
#      signal on the matching request path.
#      Anti-vacuity: the token is read out of the app's own log and grepped for in the
#      capture, so an empty or truncated capture fails rather than passing silently.
#
# WHAT THIS GATE DELIBERATELY DOES NOT DO — and why (a plan correction).
# The WS19 plan's Task 8 Step 4 asks for the capture to be compared against "the
# installed ghostty 1.3.1-2.fc44 doing the same handshake". That is IMPOSSIBLE: that
# build has no OpenURI portal path at all —
#   strings /usr/bin/ghostty | grep -c org.freedesktop.portal.OpenURI  ->  0
# (it carries only `org.freedesktop.portal.GlobalShortcuts` + `.Request`). The portal
# `OpenURI` call site landed upstream AFTER the 1.3.1 tag, so there is no installed
# oracle to diff against. The substitute is stronger than a log diff anyway: layer 1's
# stub asserts the payload FIELD BY FIELD against the reference source at the pin
# (parent window "", the uri verbatim, and `handle_token`/`writable`/`ask` —
# `OpenURI.zig:266-330`), which a transcript comparison would only do by eye.
#
# RUN ON THE VM inside the live session's environment:
#   ssh rvm 'cd ~/rustty-vmwork && \
#     XDG_RUNTIME_DIR=/run/user/1000 \
#     DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus \
#     .port/tools/gates/ws19_portal_openuri_gate.sh'
#
# Exit 0 = GREEN.
set -uo pipefail

RUSTTY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORK="${TMPDIR:-/tmp}/ws19-t8-gate"
BUS_LOG="$WORK/bus.log"
LIVE_LOG="$WORK/live.log"
NOHANDLER_LOG="$WORK/nohandler.log"
SCHEME="x-rustty-openuri-gate"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }

[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ] || fail "DBUS_SESSION_BUS_ADDRESS unset"
command -v dbus-daemon >/dev/null || fail "dbus-daemon is required by the stub-bus KAT"
command -v dbus-monitor >/dev/null || fail "dbus-monitor is required for the wire capture"

rm -rf "$WORK"; mkdir -p "$WORK"

# The scheme handler that makes response code 0 reachable without launching anything
# heavy into the live session. Installed and removed by this gate.
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
DESKTOP_FILE="$DESKTOP_DIR/rustty-openuri-probe.desktop"
install_handler() {
  mkdir -p "$DESKTOP_DIR"
  cat > "$DESKTOP_FILE" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Rustty OpenURI probe
Exec=/bin/true %u
Terminal=false
NoDisplay=true
MimeType=x-scheme-handler/$SCHEME;
DESKTOP
  update-desktop-database "$DESKTOP_DIR" 2>/dev/null
  xdg-mime default rustty-openuri-probe.desktop "x-scheme-handler/$SCHEME" 2>/dev/null
}
remove_handler() {
  rm -f "$DESKTOP_FILE"
  update-desktop-database "$DESKTOP_DIR" 2>/dev/null
}

cleanup() {
  [ -n "${MON_PID:-}" ] && kill "$MON_PID" 2>/dev/null
  remove_handler
  return 0
}
# Armed BEFORE anything is spawned or installed, and for signals as well as exit, so a
# failed run leaves neither a dbus-monitor nor a stray handler in the user profile.
trap cleanup EXIT INT TERM HUP

cd "$RUSTTY_ROOT" || fail "cannot cd to $RUSTTY_ROOT"

# --- layer 1: stub-portal ordering, on a private activation-free bus ----------
echo "== layer 1: stub-portal ordering + no-portal fallback (private bus) =="
if ! cargo test -p rustty-apprt --features gtk --test gtk_portal_openuri \
      -- --ignored --nocapture > "$WORK/stub.log" 2>&1; then
  tail -30 "$WORK/stub.log"
  fail "the stub-portal ordering KAT failed"
fi
grep -q "KAT PASS" "$WORK/stub.log" \
  || { tail -30 "$WORK/stub.log"; fail "stub KAT did not report PASS (did it get filtered out?)"; }
grep -qE "^test result: ok\. 1 passed" "$WORK/stub.log" \
  || { tail -20 "$WORK/stub.log"; fail "stub KAT did not run exactly one test — a filtered-out test would 'pass' vacuously"; }
echo "  OK"

# --- layer 2: live round-trip against the real portal ------------------------
echo "== layer 2: live round-trip against the session's xdg-desktop-portal =="
install_handler

dbus-monitor --session \
  "path_namespace=/org/freedesktop/portal/desktop" \
  "type='signal',interface='org.freedesktop.portal.Request'" > "$BUS_LOG" 2>&1 &
MON_PID=$!
sleep 1

if ! cargo test -p rustty-apprt --features gtk --test gtk_portal_openuri_live \
      -- --ignored --nocapture > "$LIVE_LOG" 2>&1; then
  tail -30 "$LIVE_LOG"
  fail "the live portal KAT failed"
fi
grep -q "LIVE KAT PASS" "$LIVE_LOG" \
  || { tail -30 "$LIVE_LOG"; fail "live KAT did not report PASS"; }
grep -qE "^test result: ok\. 1 passed" "$LIVE_LOG" \
  || { tail -20 "$LIVE_LOG"; fail "live KAT did not run exactly one test"; }
echo "  OK"

# --- layer 3: the wire shape -------------------------------------------------
echo "== layer 3: dbus-monitor capture shape =="
sleep 1
kill "$MON_PID" 2>/dev/null; MON_PID=""

# The token the app actually used, straight out of its own log line. Reading it from
# the app rather than hardcoding a pattern is what stops layer 3 from passing on an
# unrelated capture.
TOKEN="$(sed -n 's/.*open uri successful token=\([0-9a-f]*\).*/\1/p' "$LIVE_LOG" | head -1)"
[ -n "$TOKEN" ] || { tail -20 "$LIVE_LOG"; fail "could not read the request token from the app log; layer 3 would be vacuous"; }
echo "  request token=$TOKEN"

grep -q "member=OpenURI" "$BUS_LOG" \
  || { grep -c '' "$BUS_LOG"; fail "no OpenURI method call in the capture ($BUS_LOG)"; }
grep -q "org.freedesktop.portal.OpenURI" "$BUS_LOG" \
  || fail "the capture has no org.freedesktop.portal.OpenURI interface reference"
grep -q "$TOKEN" "$BUS_LOG" \
  || fail "the capture does not contain the handle_token $TOKEN the app used — the capture and the run do not correspond"
grep -q "member=Response" "$BUS_LOG" \
  || fail "no Request.Response signal in the capture"
echo "  OK (OpenURI call + Response signal + matching handle_token all present)"

# --- negative control for layer 2 --------------------------------------------
# Remove the scheme handler: the portal can no longer resolve the URI, so the
# `last_response_code == Some(0)` assertion MUST fail. If it still passes, layer 2 was
# not actually measuring the portal's resolution of our URI.
echo "== negative control: same live KAT with NO scheme handler (must FAIL) =="
remove_handler
if cargo test -p rustty-apprt --features gtk --test gtk_portal_openuri_live \
     -- --ignored --nocapture > "$NOHANDLER_LOG" 2>&1; then
  tail -20 "$NOHANDLER_LOG"
  fail "the live KAT PASSED with no scheme handler installed: it is not measuring the portal's response code"
fi
echo "  OK — it failed as required:"
grep -E "panicked|the portal did not report success|test result:" "$NOHANDLER_LOG" | sed 's/^/    /' | head -6

echo
echo "GATE PASS: portal OpenURI — stub ordering + live round-trip + wire shape, with both negative controls exercised"
