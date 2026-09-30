#!/usr/bin/env bash
# gtk_live_headless_sway.sh — the CANONICAL launch recipe for the live GTK KATs
# (`crates/rustty-apprt/tests/gtk_*.rs`, `#[ignore]`d, `--features gtk`) on a
# Linux host with no desktop session, e.g. rvm (P7-W7 T6).
#
# Usage (from anywhere; the command runs in the repo root):
#   .port/tools/gtk_live_headless_sway.sh cargo test -p rustty-apprt --features gtk \
#       --test gtk_inspector_dump -- --ignored --exact inspector_dump --nocapture
#
# What it does: it starts a headless sway on its own socket, runs the command
# against it, and tears sway down again. The primary session's compositor is
# never touched, because sway is addressed by its own pid and socket and the
# script never uses `pkill`.
#
# THE RECIPE, and why each part is there:
#   - WLR_BACKENDS=headless and WLR_HEADLESS_OUTPUTS=1: one 1280x720 virtual
#     head, with no DRM device and no seat.
#   - WLR_RENDERER=pixman: this is REQUIRED on rvm. The default GLES renderer
#     allocates through GBM and fails with `DRM_IOCTL_MODE_CREATE_DUMB` there
#     (V20b).
#   - LIBGL_ALWAYS_SOFTWARE=1 for the client: Mesa llvmpipe.
#   - TILING. The sway config has NO `floating enable` rule; this is required,
#     not cosmetic. The v20b gate floats every window, because it needs
#     client-chosen sizes. Do not copy that rule here. sway centers floating
#     windows, so the 1000x600 inspector window fully covers both terminal
#     windows in `gtk_inspector_dump`. A fully covered window gets no
#     `wl_surface.frame` callbacks, so GTK never snapshots it and its
#     RenderSurface texture never changes. Assertion (c), "both terminals kept
#     presenting frames alongside the inspector", then fails every time. With
#     tiling, no window covers another. This one config difference explains why
#     the test passed for T5b and failed for T5c at the same commit.
#   - The test binary's argv (`--ignored --exact ...`) reaches
#     `Config::load_cli_args`, as the reference's `std.os.argv` does. It raises
#     config diagnostics, so a "Configuration Errors" dialog also opens. That is
#     expected, and it is harmless under tiling.
set -uo pipefail

[ "$#" -ge 1 ] || { echo "usage: $0 <command...>" >&2; exit 2; }
[ -n "${XDG_RUNTIME_DIR:-}" ] || { echo "XDG_RUNTIME_DIR unset" >&2; exit 2; }
command -v sway >/dev/null || { echo "sway not installed" >&2; exit 2; }

RUSTTY_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/gtk-live-sway.XXXXXX")"

cat > "$WORK/sway.conf" <<'EOF'
default_border none
focus_follows_mouse no
EOF

SWAY_PID=""
WL=""
cleanup() {
  if [ -n "$SWAY_PID" ]; then
    kill "$SWAY_PID" 2>/dev/null
    for _ in $(seq 1 25); do kill -0 "$SWAY_PID" 2>/dev/null || break; sleep 0.2; done
    kill -9 "$SWAY_PID" 2>/dev/null
    wait "$SWAY_PID" 2>/dev/null
    rm -f "$XDG_RUNTIME_DIR/sway-ipc.$(id -u).$SWAY_PID.sock"
  fi
  [ -n "$WL" ] && rm -f "$XDG_RUNTIME_DIR/$WL" "$XDG_RUNTIME_DIR/$WL.lock"
  rm -f "$WORK/sway.conf" "$WORK/sway.log"
  rmdir "$WORK" 2>/dev/null
  return 0
}
trap cleanup EXIT INT TERM HUP

env -u WAYLAND_DISPLAY WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_HEADLESS_OUTPUTS=1 \
    WLR_LIBINPUT_NO_DEVICES=1 XDG_SESSION_TYPE=wayland \
  setsid nohup sway --config "$WORK/sway.conf" -d > "$WORK/sway.log" 2>&1 < /dev/null &
SWAY_PID=$!

# sway's IPC socket is named after its own pid; never glob `sway-ipc.*.sock`.
SWAYSOCK="$XDG_RUNTIME_DIR/sway-ipc.$(id -u).$SWAY_PID.sock"
for _ in $(seq 1 150); do
  kill -0 "$SWAY_PID" 2>/dev/null || { tail -20 "$WORK/sway.log" >&2; echo "sway exited during startup" >&2; exit 1; }
  [ -S "$SWAYSOCK" ] && break
  sleep 0.2
done
[ -S "$SWAYSOCK" ] || { tail -30 "$WORK/sway.log" >&2; echo "sway never created $SWAYSOCK" >&2; exit 1; }

# sway 1.11 does not export WAYLAND_DISPLAY into its log environment; it logs
# "Running compositor on wayland display 'wayland-N'". That line can land after the
# IPC socket appears, so wait for it too.
for _ in $(seq 1 150); do
  WL="$(grep -oE "compositor on wayland display 'wayland-[0-9]+'" "$WORK/sway.log" | tail -1 | grep -oE 'wayland-[0-9]+')"
  [ -n "$WL" ] && break
  sleep 0.2
done
[ -n "$WL" ] || { tail -30 "$WORK/sway.log" >&2; echo "could not determine sway's WAYLAND_DISPLAY" >&2; exit 1; }
sleep 1
echo "headless sway: pid $SWAY_PID WAYLAND_DISPLAY=$WL (tiling, pixman)"

cd "$RUSTTY_ROOT" || exit 1
env WAYLAND_DISPLAY="$WL" SWAYSOCK="$SWAYSOCK" LIBGL_ALWAYS_SOFTWARE=1 "$@"
