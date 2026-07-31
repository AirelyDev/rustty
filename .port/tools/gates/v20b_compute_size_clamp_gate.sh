#!/usr/bin/env bash
# v20b_compute_size_clamp_gate.sh — V20b (LINUX / VM ONLY)
#
# Discharges `.port/phase-2/VERIFICATION_DEBT.md` V20b: the GEOMETRY EFFECT of
# `004c88e41`'s `compute-size` handler (`class::window::imp::toplevel_compute_size`),
# which was filed ENVIRONMENT-BLOCKED on "needs two monitors".
#
# TWO MONITORS, BUT VIRTUAL. The handler only fires when the compositor's bounds
# SHRINK below the window's live size, which one fixed head can never produce. A
# **headless sway** (`WLR_BACKENDS=headless`) supplies two differently-sized heads with
# no DRM device, no seat, and its own `WAYLAND_DISPLAY` — so the primary GNOME session
# is never touched (the same discipline as `ws17_set_urgent_mutter_gate.sh`; see row V4
# for why that matters on this host). `swaymsg` then moves the window between heads
# from OUTSIDE the client, so the shrink decision comes from a real compositor.
#
# WHY SWAY AND NOT MUTTER: mutter's `--virtual-monitor` gives extra heads, but nothing
# on this host can move a window between mutter's heads without injecting input into a
# live session. sway exposes `swaymsg move workspace to output`, i.e. an external,
# scriptable, compositor-driven move. Both are real compositors implementing
# `xdg_toplevel.configure_bounds`; the seam under test is GDK's, not sway-specific.
#
# ARMS (both required):
#   1. `exceeds` — realized at 1400x1000 on the BIG head, moved to the 800x600 head,
#      the big head then switched off, and asked for its size again. The re-assertion
#      must beat GTK's clamp: the surface must still be larger than 800x600.
#      (The head is switched off because sway does not send
#      `xdg_toplevel.configure_bounds`, so the bounds GDK delivers are its fallback —
#      the bounding box of the ATTACHED monitors. Moving alone does not shrink them.
#      Established by probing the handler; see the row.)
#   2. `fits` (CONTRASTING CONTROL) — the same run at 640x480, which fits the small
#      head. The predicate returns false, the handler is a no-op, and the surface must
#      stay 640x480 rather than being inflated to the head. Without this arm, "the
#      window is not the monitor's size" would not distinguish a working handler from
#      a handler that never runs.
#
# A THIRD, SOURCE-MUTATION NEGATIVE CONTROL (delete the `set_size` re-assertion and
# watch arm 1 fail) is recorded in the debt row; it is not run here because it mutates
# tracked source.
#
# Exit 0 = GREEN.
set -uo pipefail

RUSTTY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORK="${TMPDIR:-/tmp}/v20b-gate"
SOCKET="wayland-v20bsway"
EVIDENCE="$RUSTTY_ROOT/.port/phase-2/evidence/v20b"

BIG_W=1920;  BIG_H=1200
SMALL_W=800; SMALL_H=600
BIG_OUT="HEADLESS-1"
SMALL_OUT="HEADLESS-2"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }

[ -n "${XDG_RUNTIME_DIR:-}" ] || fail "XDG_RUNTIME_DIR unset"
command -v sway    >/dev/null || fail "sway not installed"
command -v swaymsg >/dev/null || fail "swaymsg not installed"

# Scope every check to OUR socket. Never `pkill sway` / `pgrep -f` — that could match
# the primary session's compositor, which this gate must never disturb.
if [ -e "$XDG_RUNTIME_DIR/$SOCKET" ]; then
  fail "stale wayland socket $XDG_RUNTIME_DIR/$SOCKET from a previous run; remove it first"
fi

rm -rf "$WORK"; mkdir -p "$WORK" "$EVIDENCE"

UPTIME_BEFORE="$(uptime)"
echo "uptime before: $UPTIME_BEFORE"
echo "sway: $(sway --version 2>&1 | head -1)"
echo "primary session left alone: WAYLAND_DISPLAY=wayland-0 is NOT used by this gate"

# sway's own config. Windows must FLOAT: a tiled toplevel is sized by sway, so the
# client never gets to request a size and `compute-size` is not the deciding party.
cat > "$WORK/sway.conf" <<EOF
for_window [title=".*"] floating enable, border none
default_border none
focus_follows_mouse no
EOF

SWAY_PID=""
cleanup() {
  if [ -n "$SWAY_PID" ]; then
    kill "$SWAY_PID" 2>/dev/null
    for _ in $(seq 1 25); do kill -0 "$SWAY_PID" 2>/dev/null || break; sleep 0.2; done
    kill -9 "$SWAY_PID" 2>/dev/null
    wait "$SWAY_PID" 2>/dev/null
  fi
  rm -f "$XDG_RUNTIME_DIR/$SOCKET" "$XDG_RUNTIME_DIR/$SOCKET.lock"
  [ -n "$SWAY_PID" ] && rm -f "$XDG_RUNTIME_DIR/sway-ipc.$(id -u).$SWAY_PID.sock"
  return 0
}
# Armed BEFORE anything is spawned, and for signals: a killed gate must not leave a
# compositor reparented to init.
trap cleanup EXIT INT TERM HUP

# Two headless outputs. `WLR_HEADLESS_OUTPUTS=2` creates HEADLESS-1/HEADLESS-2 at
# startup; the config above gives them their differing modes.
# WLR_RENDERER=pixman is REQUIRED, not cosmetic: with the default GLES renderer the
# headless backend still allocates through GBM, which fails `Permission denied` on this
# VM ("DRM_IOCTL_MODE_CREATE_DUMB failed"). The failure is silent at the IPC layer —
# `swaymsg output ... resolution` returns `success: true` while both heads stay at
# wlroots' 1280x720 default. The provisioning assertion below is what caught it.
env -u WAYLAND_DISPLAY WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_HEADLESS_OUTPUTS=2 \
    WLR_LIBINPUT_NO_DEVICES=1 XDG_SESSION_TYPE=wayland \
  setsid nohup sway --config "$WORK/sway.conf" -d \
  > "$WORK/sway.log" 2>&1 < /dev/null &
SWAY_PID=$!

# sway's IPC socket is named after its OWN pid — address it exactly, never by globbing
# `sway-ipc.*.sock`. A stale socket from an earlier run sorts first and swaymsg would
# then be talking to a compositor this gate does not control (that happened once).
SWAYSOCK="$XDG_RUNTIME_DIR/sway-ipc.$(id -u).$SWAY_PID.sock"
ready=0
for _ in $(seq 1 150); do
  kill -0 "$SWAY_PID" 2>/dev/null || { tail -20 "$WORK/sway.log"; fail "sway exited during startup"; }
  [ -S "$SWAYSOCK" ] && { ready=1; break; }
  sleep 0.2
done
[ "$ready" -eq 1 ] || { tail -30 "$WORK/sway.log"; fail "sway never created $SWAYSOCK"; }
export SWAYSOCK
WL_DISPLAY="$(grep -oE 'WAYLAND_DISPLAY=wayland-[0-9]+' "$WORK/sway.log" | tail -1 | cut -d= -f2)"
[ -n "$WL_DISPLAY" ] || WL_DISPLAY="$(swaymsg -t get_version --raw >/dev/null 2>&1 && echo wayland-1)"
[ -n "$WL_DISPLAY" ] || { tail -30 "$WORK/sway.log"; fail "could not determine sway's WAYLAND_DISPLAY"; }
sleep 2
echo "headless sway up (pid $SWAY_PID) SWAYSOCK=$SWAYSOCK WAYLAND_DISPLAY=$WL_DISPLAY"

# PROVE the provisioning took effect before trusting any arm. A gate that silently
# ran on one head, or on two heads of the SAME size, would pass for the wrong reason.
# Give the two headless outputs their DIFFERING modes. Done over IPC AFTER startup,
# not in the config file: the headless outputs do not exist when sway parses its
# config, so an `output ... mode` line there is silently dropped and both heads stay
# at wlroots' 1280x720 default — which would make both arms meaningless. The
# provisioning check below is what caught that.
# The SMALL head sits at the origin: GDK's bounds fallback is the bounding box of the
# attached monitors, so with the small head offset to the right the box would keep the
# big head's width even after it is switched off.
swaymsg "output $SMALL_OUT resolution ${SMALL_W}x${SMALL_H} position 0 0" >/dev/null 2>&1
swaymsg "output $BIG_OUT resolution ${BIG_W}x${BIG_H} position ${SMALL_W} 0" >/dev/null 2>&1
sleep 1

swaymsg -t get_outputs --raw > "$WORK/outputs.json" 2>&1 \
  || { cat "$WORK/outputs.json"; fail "swaymsg could not talk to sway"; }
python3 - "$WORK/outputs.json" "$BIG_OUT" "$BIG_W" "$BIG_H" "$SMALL_OUT" "$SMALL_W" "$SMALL_H" <<'PY' \
  || fail "the two heads are not provisioned as requested (see $WORK/outputs.json)"
import json, sys
data = json.load(open(sys.argv[1]))
want = {sys.argv[2]: (int(sys.argv[3]), int(sys.argv[4])),
        sys.argv[5]: (int(sys.argv[6]), int(sys.argv[7]))}
got = {o["name"]: (o["rect"]["width"], o["rect"]["height"]) for o in data}
print("outputs:", got)
sys.exit(0 if all(got.get(k) == v for k, v in want.items()) else 1)
PY
echo "  two heads confirmed: $BIG_OUT ${BIG_W}x${BIG_H}, $SMALL_OUT ${SMALL_W}x${SMALL_H}"

cd "$RUSTTY_ROOT" || fail "cannot cd to $RUSTTY_ROOT"

run_arm() {
  # Split: bash expands every word of a `local` BEFORE assigning any of them, so
  # `log="$WORK/$arm.log"` on the same line is an unbound-variable error under `set -u`.
  local arm="$1" w="$2" h="$3"
  local log="$WORK/$arm.log"
  echo "== arm \`$arm\`: request ${w}x${h}, move to $SMALL_OUT (${SMALL_W}x${SMALL_H}) =="
  env WAYLAND_DISPLAY="$WL_DISPLAY" SWAYSOCK="$SWAYSOCK" LIBGL_ALWAYS_SOFTWARE=1 \
      RUSTTY_V20B_ARM="$arm" RUSTTY_V20B_W="$w" RUSTTY_V20B_H="$h" \
      RUSTTY_V20B_SMALL_W="$SMALL_W" RUSTTY_V20B_SMALL_H="$SMALL_H" \
      RUSTTY_V20B_SMALL_OUTPUT="$SMALL_OUT" RUSTTY_V20B_BIG_OUTPUT="$BIG_OUT" \
    cargo test -p rustty-apprt --features gtk,wayland --test gtk_wayland_compute_size \
      -- --ignored --exact compute_size_reasserts_size_on_a_smaller_monitor --nocapture \
      > "$log" 2>&1
  local rc=$?
  grep -E "^V20B (arm|heads|before|after|outputs)" "$log" | sed 's/^/    /'
  # The arm switched the big head off and parked the workspace on the small one;
  # put both back so the next arm starts from the same two-head state.
  swaymsg "output $BIG_OUT enable" >/dev/null 2>&1
  swaymsg "output $BIG_OUT resolution ${BIG_W}x${BIG_H} position ${SMALL_W} 0" >/dev/null 2>&1
  swaymsg "move workspace to output $BIG_OUT" >/dev/null 2>&1
  sleep 1
  return $rc
}

run_arm exceeds 1400 1000 \
  || { tail -25 "$WORK/exceeds.log"; fail "arm \`exceeds\`: the clamp was NOT defeated"; }
grep -qE "^test result: ok\. 1 passed" "$WORK/exceeds.log" || fail "arm \`exceeds\` did not report a pass"
echo "  OK"

run_arm fits 640 480 \
  || { tail -25 "$WORK/fits.log"; fail "arm \`fits\` (control): a within-bounds window was disturbed"; }
grep -qE "^test result: ok\. 1 passed" "$WORK/fits.log" || fail "arm \`fits\` did not report a pass"
echo "  OK"

UPTIME_AFTER="$(uptime)"

{
  echo "V20b — compute-size monitor clamp, GEOMETRY EFFECT, on a real compositor"
  echo "captured: $(date -Is)"
  echo "sway:     $(sway --version 2>&1 | head -1)"
  echo "compositor: a headless sway (WLR_BACKENDS=headless, WLR_HEADLESS_OUTPUTS=2) on"
  echo "            its own socket. The primary GNOME/mutter session was NOT touched."
  echo "heads: $BIG_OUT ${BIG_W}x${BIG_H} + $SMALL_OUT ${SMALL_W}x${SMALL_H}"
  echo "uptime before: $UPTIME_BEFORE"
  echo "uptime after : $UPTIME_AFTER"
  echo
  echo "--- arm 1 \`exceeds\` (clamp must be defeated) ---"
  grep -E "^V20B |test result:" "$WORK/exceeds.log"
  echo
  echo "--- arm 2 \`fits\` (contrasting control: handler must be a no-op) ---"
  grep -E "^V20B |test result:" "$WORK/fits.log"
} > "$EVIDENCE/v20b-compute-size-two-heads.txt"

echo "uptime after: $UPTIME_AFTER"
echo "evidence: $EVIDENCE/v20b-compute-size-two-heads.txt"
echo
echo "GATE PASS: the compute-size handler re-asserts the window size against a real"
echo "           smaller head's bounds; the within-bounds control is untouched"
