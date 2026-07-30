#!/usr/bin/env bash
# ws17_kde_globals_gate.sh — WS17 V3 (LINUX / VM ONLY)
#
# Discharges `.port/phase-2/VERIFICATION_DEBT.md` V3's central claim: that the KDE-only
# Wayland globals the port binds (`org_kde_kwin_server_decoration_manager`,
# `org_kde_kwin_slide_manager`) have never been exercised against a real KWin.
#
# TWO ARMS, and the second is the whole point:
#   1. POSITIVE — `kde_globals_bind_under_kwin` under a NESTED KWin must PASS.
#   2. NEGATIVE CONTROL — the SAME test on the GNOME/mutter session must FAIL, because
#      mutter advertises neither KDE global. If it passed there, the test would be
#      asserting nothing about KWin.
#
# NESTED, never the primary session: kwin runs on its own socket with a throwaway
# XDG_CONFIG_HOME, so GDM, the default systemd target and the tty2 GNOME session are
# untouched.
#
# ROOTLESS KWIN: `sudo` on this VM requires a password, so `dnf install kwin` is not
# available to a non-interactive runner. Use a user-prefix kwin extracted from
# downloaded RPMs (no root at any point):
#   mkdir -p /tmp/kwinrpm && cd /tmp/kwinrpm
#   dnf download --disablerepo=cursor --resolve kwin
#   mkdir root && cd root && for f in ../*.rpm; do rpm2cpio "$f" | cpio -idmu --quiet; done
# Override the location with KWIN_PREFIX.
#
# LEDGER CORRECTION encoded here: V3's text said KWin blur rides `org_kde_kwin_blur`. It
# does not — the oracle uses `ext_background_effect_v1` (`wayland.zig:156`) and so does
# rustty (`Tag::ExtBackgroundEffect`). `org_kde_kwin_blur` is in neither tree. The gate
# asserts that, statically, below.
#
# Exit 0 = GREEN.
set -uo pipefail

RUSTTY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
KWIN_PREFIX="${KWIN_PREFIX:-/tmp/kwinrpm/root}"
WORK="${TMPDIR:-/tmp}/ws17-v3-gate"
SOCKET="wayland-kwinv3"
EVIDENCE="$RUSTTY_ROOT/.port/phase-2/evidence/ws17"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }

[ -n "${XDG_RUNTIME_DIR:-}" ] || fail "XDG_RUNTIME_DIR unset"
PARENT_WAYLAND_DISPLAY="${PARENT_WAYLAND_DISPLAY:-wayland-0}"
[ -S "$XDG_RUNTIME_DIR/$PARENT_WAYLAND_DISPLAY" ] \
  || fail "no parent compositor socket at $XDG_RUNTIME_DIR/$PARENT_WAYLAND_DISPLAY; kwin cannot nest and the mutter control has nothing to run against"
[ -x "$KWIN_PREFIX/usr/bin/kwin_wayland" ] || fail "no kwin_wayland at $KWIN_PREFIX/usr/bin (see the rootless recipe in this file's header)"

# --- the ledger correction, checked statically -------------------------------
echo "== ledger correction: blur rides ext-background-effect, NOT org_kde_kwin_blur =="
# Look for CODE references only. Restricting to `*/src/` and excluding `//` lines keeps
# the check from matching the prose that explains the correction (including the module
# docs of the KAT this gate runs, which name the protocol in order to correct it).
blur_hits=$(grep -rn --include='*.rs' -i "org_kde_kwin_blur" "$RUSTTY_ROOT"/crates/*/src/ 2>/dev/null \
            | grep -vE ':[[:space:]]*(//|/\*|\*)' | wc -l | tr -d ' ')
if [ "$blur_hits" != "0" ]; then
  grep -rn --include='*.rs' -i "org_kde_kwin_blur" "$RUSTTY_ROOT"/crates/*/src/ 2>/dev/null | head -5
  fail "org_kde_kwin_blur appears in crate SOURCE ($blur_hits site(s)) — the correction below would be wrong"
fi
grep -rq "ExtBackgroundEffect" "$RUSTTY_ROOT/crates/rustty-apprt/src/gtk/winproto/wayland/globals.rs" \
  || fail "ExtBackgroundEffect not found in the wayland globals — this check has gone stale (never a silent pass)"
echo "  OK (no org_kde_kwin_blur anywhere in crates/; ExtBackgroundEffect is what binds)"

# --- leftover-detection pre-flight -------------------------------------------
if pgrep -x kwin_wayland >/dev/null 2>&1; then
  fail "a kwin_wayland is already running; stop it (pkill -x kwin_wayland) — this gate will not attach to state it did not create"
fi
[ -e "$XDG_RUNTIME_DIR/$SOCKET" ] && fail "stale wayland socket $XDG_RUNTIME_DIR/$SOCKET from a previous run; remove it first"

rm -rf "$WORK"; mkdir -p "$WORK/config" "$EVIDENCE"
cat > "$WORK/config/kwinrc" <<'KWINRC'
[org.kde.kdecoration2]
library=org.kde.breeze
theme=Breeze
KWINRC

echo "uptime before: $(uptime)"

KWIN_PID=""
cleanup() {
  if [ -n "$KWIN_PID" ]; then
    kill "$KWIN_PID" 2>/dev/null
    for _ in $(seq 1 20); do kill -0 "$KWIN_PID" 2>/dev/null || break; sleep 0.2; done
    kill -9 "$KWIN_PID" 2>/dev/null
  fi
  pkill -x kwin_wayland 2>/dev/null
  rm -f "$XDG_RUNTIME_DIR/$SOCKET" "$XDG_RUNTIME_DIR/$SOCKET.lock"
  return 0
}
trap cleanup EXIT INT TERM HUP

# kwin needs the PARENT WAYLAND_DISPLAY; without it it falls through to the DRM backend
# and exits 1 with an EMPTY log (a failure mode that reads as "kwin is broken").
XDG_CONFIG_HOME="$WORK/config" WAYLAND_DISPLAY="$PARENT_WAYLAND_DISPLAY" \
  LD_LIBRARY_PATH="$KWIN_PREFIX/usr/lib64" \
  QT_PLUGIN_PATH="$KWIN_PREFIX/usr/lib64/qt6/plugins" \
  QML2_IMPORT_PATH="$KWIN_PREFIX/usr/lib64/qt6/qml" \
  XDG_DATA_DIRS="$KWIN_PREFIX/usr/share:${XDG_DATA_DIRS:-/usr/share}" \
  setsid nohup "$KWIN_PREFIX/usr/bin/kwin_wayland" --socket "$SOCKET" \
  > "$WORK/kwin.log" 2>&1 < /dev/null &
KWIN_PID=$!

# Readiness, not mere socket existence: the socket file appears BEFORE kwin accepts
# clients, and connecting early fails as a bare "Failed to initialize GTK".
ready=0
for _ in $(seq 1 150); do
  kill -0 "$KWIN_PID" 2>/dev/null || { tail -20 "$WORK/kwin.log"; fail "nested kwin exited during startup"; }
  if gdbus call --session --dest org.kde.KWin --object-path /KWin \
       --method org.freedesktop.DBus.Peer.Ping >/dev/null 2>&1; then ready=1; break; fi
  sleep 0.2
done
[ "$ready" -eq 1 ] || { tail -20 "$WORK/kwin.log"; fail "nested kwin never took org.kde.KWin on the session bus"; }
echo "nested kwin up and ready on $SOCKET (pid $KWIN_PID)"

cd "$RUSTTY_ROOT" || fail "cannot cd to $RUSTTY_ROOT"

# --- arm 1: POSITIVE, under KWin ---------------------------------------------
echo "== arm 1: KDE globals must bind under KWin =="
env -u LD_LIBRARY_PATH -u QT_PLUGIN_PATH -u QML2_IMPORT_PATH -u XDG_DATA_DIRS \
    WAYLAND_DISPLAY="$SOCKET" XDG_CONFIG_HOME="$WORK/config" LIBGL_ALWAYS_SOFTWARE=1 \
  cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_kde_globals \
    -- --ignored --nocapture > "$WORK/kwin-arm.log" 2>&1
kwin_rc=$?
grep -qE "^test result: ok\. 1 passed" "$WORK/kwin-arm.log" \
  || { tail -25 "$WORK/kwin-arm.log"; fail "the KDE-globals KAT did not pass under KWin (rc=$kwin_rc)"; }
grep "bound globals:" "$WORK/kwin-arm.log" | sed 's/^/  /'
echo "  OK"

# --- arm 2: NEGATIVE CONTROL, on the GNOME/mutter session --------------------
# Same binary, same assertions, different compositor. mutter advertises neither KDE
# global, so this MUST fail. A pass here would mean arm 1 proved nothing.
echo "== arm 2 (negative control): the same KAT on GNOME/mutter must FAIL =="
env -u LD_LIBRARY_PATH -u QT_PLUGIN_PATH -u QML2_IMPORT_PATH -u XDG_DATA_DIRS \
    WAYLAND_DISPLAY="$PARENT_WAYLAND_DISPLAY" LIBGL_ALWAYS_SOFTWARE=1 \
  cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_kde_globals \
    -- --ignored --nocapture > "$WORK/mutter-arm.log" 2>&1
mutter_rc=$?
if [ "$mutter_rc" -eq 0 ]; then
  tail -25 "$WORK/mutter-arm.log"
  fail "the KDE-globals KAT PASSED on GNOME/mutter — it is not actually asserting anything KDE-specific"
fi
echo "  OK — it failed as required:"
grep -E "bound globals:|panicked|did not bind" "$WORK/mutter-arm.log" | sed 's/^/    /' | head -4

# --- evidence ----------------------------------------------------------------
{
  echo "WS17 V3 — KDE Wayland globals under a live KWin"
  echo "captured: $(date -Is)"
  echo "kwin: $("$KWIN_PREFIX/usr/bin/kwin_wayland" --version 2>/dev/null)"
  echo "parent compositor: $PARENT_WAYLAND_DISPLAY (GNOME/mutter session)"
  echo
  echo "--- arm 1: nested KWin (must pass) ---"
  grep -E "bound globals:|KAT PASS|test result:" "$WORK/kwin-arm.log"
  echo
  echo "--- arm 2: GNOME/mutter, NEGATIVE CONTROL (must fail) ---"
  grep -E "bound globals:|panicked|did not bind|test result:" "$WORK/mutter-arm.log"
  echo
  echo "--- ledger correction ---"
  echo "blur rides ext_background_effect_v1 (oracle wayland.zig:156; rustty Tag::ExtBackgroundEffect)."
  echo "org_kde_kwin_blur code sites in crates/*/src/: $blur_hits (prose that explains the correction is excluded)"
} > "$EVIDENCE/v3-kde-globals-under-kwin.txt"
echo "evidence: $EVIDENCE/v3-kde-globals-under-kwin.txt"

echo "uptime after: $(uptime)"
echo
echo "GATE PASS: KDE globals bind under a live KWin, and the same KAT fails on mutter"
