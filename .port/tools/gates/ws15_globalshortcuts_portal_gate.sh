#!/usr/bin/env bash
# ws15_globalshortcuts_portal_gate.sh — WS15 live gate (LINUX / VM ONLY)
#
# Proves the XDG GlobalShortcuts portal round-trip actually completes against
# the running desktop, and that an `Activated` signal is dispatched all the way
# through to the bound action's side effect. Specifically it asserts, in order:
#   1. `CreateSession` got `Response` 0 (a session handle exists);
#   2. `BindShortcuts` completed AND its results name the bound shortcut;
#   3. the injected `Activated` reached the app's signal handler;
#   4. the shortcut id RESOLVED in the map (`dispatch map-hit`);
#   5. `performAllAction` was reached and returned Ok (`action performed ok`) —
#      which requires the `trigger` emission and an installed dispatch thunk;
#   6. a new toplevel window actually appeared — counted out-of-process from
#      GtkApplication's own D-Bus window export — the bound action being
#      `new_window`.
# Assertions 4-6 are what make this more than a signal-delivery test: the older
# "global shortcut activated=" line is logged BEFORE dispatch and so passes even
# when the map lookup misses or the action fails.
#
# Run ON THE VM inside the GNOME session's environment:
#   ssh rvm 'cd ~/projects/rustty && \
#     XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-0 \
#     DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus \
#     .port/tools/gates/ws15_globalshortcuts_portal_gate.sh'
#
# Exit 0 = GREEN.
#
# ---------------------------------------------------------------------------
# Two environment facts, established empirically against ghostty 1.3.1 in WS15
# T1 (see that commit message for the dbus-monitor transcript), that this gate
# has to satisfy before the portal will talk to us at all:
#
#  1. AN APP ID IS REQUIRED. xdg-desktop-portal answers `CreateSession` with
#     `org.freedesktop.portal.Error.NotAllowed: "An app id is required"` unless
#     it can derive an app id for the calling process. For a non-sandboxed host
#     process it derives one from the process's systemd unit under `app.slice`,
#     and requires a matching `<appid>.desktop` to exist. A desktop-session
#     launch satisfies this implicitly; a launch from an ssh shell does not, so
#     the gate installs a user-level .desktop file and runs the binary under
#     `systemd-run --user --scope --slice=app.slice --unit=app-<appid>-N.scope`.
#     This is environment setup, NOT a workaround for anything in the port: the
#     oracle needs exactly the same treatment to complete the same handshake.
#
#  2. GNOME 50 GATES BindShortcuts ON A USER DIALOG. gnome-control-center's
#     GlobalShortcutsProvider shows an "Add Keyboard Shortcuts" dialog and no
#     `Response` is emitted until it is answered. A physical click cannot be
#     synthesized on this VM (mutter ignores wtype's virtual-keyboard protocol;
#     ydotool/uinput needs root), but the dialog's "Add" button exposes an
#     AT-SPI `click` action — the same GtkButton activation a mouse click
#     produces. The gate drives that. GNOME then remembers the grant per app id
#     in `org.gnome.settings-daemon.global-shortcuts applications`, so later
#     runs complete with no dialog at all.
# ---------------------------------------------------------------------------
set -uo pipefail

RUSTTY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORK="${TMPDIR:-/tmp}/ws15-gate"
APP_LOG="$WORK/app.log"
BUS_LOG="$WORK/bus.log"
SHORTCUT="SHIFT+CTRL+ALT+F12"
BIN="$RUSTTY_ROOT/target/debug/rustty"
# `build_info::APPLICATION_ID` — BUNDLE_ID plus the debug_assertions `-debug`
# suffix (`info.zig:7-10`).
APP_ID="io.airely.rustty-debug"
UNIT="app-${APP_ID}-$$.scope"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }

# --- preconditions -----------------------------------------------------------
[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ] || fail "DBUS_SESSION_BUS_ADDRESS unset"
[ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ] || fail "no WAYLAND_DISPLAY/DISPLAY"
[ -x "$BIN" ] || fail "missing $BIN (run: cargo build -p rustty --features gtk)"

version="$(gdbus introspect --session --dest org.freedesktop.portal.Desktop \
  --object-path /org/freedesktop/portal/desktop 2>/dev/null \
  | awk '/interface org.freedesktop.portal.GlobalShortcuts/,/^  };/' \
  | sed -n 's/.*readonly u version = \([0-9]*\);.*/\1/p')"
[ -n "$version" ] || fail "org.freedesktop.portal.GlobalShortcuts not present on the session bus"
echo "portal GlobalShortcuts version=$version"

# --- (1) app id: a .desktop file the portal can resolve ----------------------
DESKTOP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$DESKTOP_DIR"
cat > "$DESKTOP_DIR/$APP_ID.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Rustty (debug)
Exec=$BIN
Terminal=false
NoDisplay=true
DESKTOP
update-desktop-database "$DESKTOP_DIR" 2>/dev/null

# --- (2) the AT-SPI grant clicker ---------------------------------------------
GRANT_PY="$WORK/grant.py"


# --- isolated config with one global keybind ---------------------------------
rm -rf "$WORK"; mkdir -p "$WORK/cfg/rustty"
printf 'keybind = global:ctrl+alt+shift+f12=new_window\n' > "$WORK/cfg/rustty/config.rustty"

cat > "$GRANT_PY" <<'PYEOF'
# Single-shot: click every button named $1 inside GNOME's global-shortcuts
# permission dialog. Exit 0 if at least one was clicked, 1 otherwise. One
# process per attempt on purpose: an AT-SPI client only refreshes its cached
# view of the desktop while a GLib main loop runs, so a long-lived polling
# process keeps seeing a stale tree.
import sys
import gi
gi.require_version("Atspi", "2.0")
from gi.repository import Atspi

WANT = sys.argv[1] if len(sys.argv) > 1 else "Add"
Atspi.init()

def find(node, out, depth=0):
    if depth > 25:
        return
    try:
        if node.get_role_name() in ("push button", "button") and node.get_name() == WANT:
            out.append(node)
        n = node.get_child_count()
    except Exception:
        return
    for i in range(n):
        c = node.get_child_at_index(i)
        if c:
            find(c, out, depth + 1)

clicked = 0
desktop = Atspi.get_desktop(0)
for i in range(desktop.get_child_count()):
    app = desktop.get_child_at_index(i)
    if not app:
        continue
    try:
        name = app.get_name() or ""
    except Exception:
        continue
    if "shortcut" not in name.lower():
        continue
    buttons = []
    find(app, buttons)
    for b in buttons:
        try:
            Atspi.Action.do_action(b, 0)
            clicked += 1
        except Exception as e:
            print(f"click failed: {e}", file=sys.stderr)

print(f"clicked {WANT!r} x{clicked}")
sys.exit(0 if clicked else 1)
PYEOF

# Count the rustty application's toplevel windows. This is the external,
# out-of-process observable for `new_window` — the gate uses it to prove the
# dispatched action actually did something, not merely that the code path ran.
#
# The source is `GtkApplication`'s own D-Bus export: a registered GTK app owns
# its app id on the session bus and exports one object per toplevel at
# `<app-path>/window/N`. That is authoritative and needs no accessibility stack
# (AT-SPI was tried first and reports rustty as an 'Unnamed' application, so it
# cannot be matched reliably). Prints the count on stdout; exits 2 if the app is
# not on the bus at all, so "app missing" can never masquerade as "0 windows".
APP_OBJ_PATH="/$(echo "$APP_ID" | sed 's/\./\//g; s/-/_/g')"
wincount() {
  local out
  out="$(gdbus introspect --session --dest "$APP_ID" \
        --object-path "$APP_OBJ_PATH/window" --recurse 2>/dev/null)" || return 2
  [ -n "$out" ] || return 2
  printf '%s\n' "$out" | grep -cE "^ *node $APP_OBJ_PATH/window/[0-9]+ \{"
  return 0
}

# Is the AT-SPI clicker itself usable? Distinguished up-front so that "no dialog
# appeared" can never be confused with "python3/gi/Atspi is missing", which would
# otherwise silently downgrade a broken grant path into a benign-looking message.
ATSPI_OK=0
if python3 -c 'import gi; gi.require_version("Atspi","2.0"); from gi.repository import Atspi' 2>/dev/null; then
  ATSPI_OK=1
else
  echo "WARNING: python3 gi/Atspi unavailable — the permission dialog CANNOT be answered by this gate"
fi

grant() { # grant <button-label> <tries>
  [ "$ATSPI_OK" -eq 1 ] || return 2
  for _ in $(seq 1 "${2:-40}"); do
    if python3 "$GRANT_PY" "$1" 2>/dev/null | grep -qv 'x0'; then
      echo "granted via AT-SPI ($1)"
      return 0
    fi
    sleep 0.5
  done
  return 1
}

# Installed and torn down by this gate; tracked so cleanup removes it rather than
# leaving a stray entry in the real user profile.
DESKTOP_FILE="$DESKTOP_DIR/$APP_ID.desktop"
cleanup() {
  [ -n "${UNIT:-}" ] && systemctl --user stop "$UNIT" 2>/dev/null
  [ -n "${APP_PID:-}" ] && kill "$APP_PID" 2>/dev/null
  [ -n "${MON_PID:-}" ] && kill "$MON_PID" 2>/dev/null
  [ -n "${APP_PID:-}" ] && wait "$APP_PID" 2>/dev/null
  rm -f "$DESKTOP_FILE"
  update-desktop-database "$DESKTOP_DIR" 2>/dev/null
  return 0
}
# Installed BEFORE anything is spawned so no failure can leak dbus-monitor or the
# scope unit, and armed for signals as well as normal exit.
trap cleanup EXIT INT TERM HUP

dbus-monitor --session "path_namespace=/org/freedesktop/portal/desktop" > "$BUS_LOG" 2>&1 &
MON_PID=$!
sleep 1

XDG_CONFIG_HOME="$WORK/cfg" systemd-run --user --scope --slice=app.slice \
  --unit="$UNIT" -- "$BIN" > "$APP_LOG" 2>&1 &
APP_PID=$!

# --- wait for the round-trip -------------------------------------------------
handle=""
for _ in $(seq 1 40); do
  sleep 0.5
  handle="$(sed -n 's/.*global shortcuts session_handle=\(.*\)$/\1/p' "$APP_LOG" | head -1)"
  [ -n "$handle" ] && break
done
[ -n "$handle" ] || {
  echo "--- app log ---"; cat "$APP_LOG"
  echo "--- bus log (portal) ---"; grep -E "member=(CreateSession|BindShortcuts|Response)|NotAllowed" "$BUS_LOG" | head -20
  fail "no portal session handle: CreateSession never got Response 0"
}
echo "session_handle=$handle"

# Answer the one-time GNOME permission dialog if it is up. A second run needs no
# dialog at all (the grant is remembered), so a miss here is not fatal.
grant Add 40
case $? in
  0) ;;
  2) echo "grant SKIPPED: AT-SPI clicker unavailable — if BindShortcuts now stalls, that is why, NOT an absent dialog" ;;
  *) echo "no permission dialog appeared (AT-SPI is working and found no 'Add' button — grant already remembered)" ;;
esac

bound=0
for _ in $(seq 1 60); do
  if grep -q "global shortcuts bound results=" "$APP_LOG"; then bound=1; break; fi
  sleep 0.5
done
[ "$bound" -eq 1 ] || {
  echo "--- app log ---"; cat "$APP_LOG"
  fail "BindShortcuts never completed (the GNOME permission dialog was not answered)"
}
grep -q "global shortcuts bound results=.*$SHORTCUT" "$APP_LOG" \
  || fail "BindShortcuts results do not mention $SHORTCUT"
echo "BindShortcuts accepted $SHORTCUT"

# --- activation dispatch -----------------------------------------------------
# Baseline window count BEFORE the activation, so the post-activation count is a
# delta rather than an absolute (the app may legitimately start with one window).
windows_before="$(wincount)"
rc=$?
[ "$rc" -eq 0 ] && [ -n "$windows_before" ] || {
  echo "--- app log ---"; cat "$APP_LOG"
  fail "cannot enumerate rustty's windows on the session bus at $APP_ID$APP_OBJ_PATH/window (rc=$rc); the new-window side-effect assertion below would be vacuous"
}
echo "windows before activation: $windows_before"

gdbus emit --session \
  --object-path /org/freedesktop/portal/desktop \
  --signal org.freedesktop.portal.GlobalShortcuts.Activated \
  "objectpath '$handle'" "'$SHORTCUT'" "uint64 0" "@a{sv} {}" \
  || fail "gdbus emit failed"

activated=0
for _ in $(seq 1 20); do
  if grep -q "global shortcut activated=$SHORTCUT" "$APP_LOG"; then activated=1; break; fi
  sleep 0.5
done
[ "$activated" -eq 1 ] || {
  echo "--- app log ---"; cat "$APP_LOG"
  fail "Activated signal was not received (the signal never reached the app's handler)"
}
echo "Activated signal received"

# The line above is emitted BEFORE `dispatch_activation` runs, so on its own it
# proves only signal delivery. The two assertions below are the load-bearing
# ones: they are reachable only after the map lookup HIT (`dispatch map-hit`),
# and only after `trigger` was emitted, the `perform_all_action_thunk` was
# installed, and `performAllAction` returned Ok (`action performed ok`).
dispatched=0
for _ in $(seq 1 20); do
  if grep -q "global shortcut dispatch map-hit id=$SHORTCUT" "$APP_LOG"; then dispatched=1; break; fi
  sleep 0.5
done
[ "$dispatched" -eq 1 ] || {
  echo "--- app log ---"; cat "$APP_LOG"
  fail "Activated arrived but did not resolve to a bound action (map lookup missed)"
}
echo "map lookup resolved $SHORTCUT to its bound action"

performed=0
for _ in $(seq 1 20); do
  if grep -q "global shortcut action performed ok" "$APP_LOG"; then performed=1; break; fi
  sleep 0.5
done
[ "$performed" -eq 1 ] || {
  echo "--- app log ---"; cat "$APP_LOG"
  fail "the bound action was never performed (trigger not emitted, thunk not installed, or performAllAction returned Err)"
}
echo "performAllAction completed Ok"

# --- the observable side effect ----------------------------------------------
# The bound action is `new_window`, so a window must actually have appeared.
windows_after="$windows_before"
for _ in $(seq 1 20); do
  windows_after="$(wincount)" || windows_after="$windows_before"
  [ "${windows_after:-0}" -gt "${windows_before:-0}" ] && break
  sleep 0.5
done
[ "${windows_after:-0}" -gt "${windows_before:-0}" ] || {
  echo "--- app log ---"; cat "$APP_LOG"
  fail "no new window appeared (before=$windows_before after=$windows_after): the action was reported performed but had no effect"
}
echo "new window observed on the session bus: $windows_before -> $windows_after"

echo "GATE PASS: portal round-trip + dispatch through performAllAction + new window confirmed"
