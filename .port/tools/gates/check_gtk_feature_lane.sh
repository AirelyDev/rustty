#!/usr/bin/env bash
# G-GTKFEAT — lint the code that ONLY compiles under `--features gtk,wayland,x11`.
#
# WHY THIS EXISTS
# ---------------
# `.port/tools/ci.sh` lints with `cargo clippy --workspace --all-targets --
# -D warnings`, which builds with DEFAULT features. `rustty-apprt`'s `gtk`,
# `wayland` and `x11` features are default-OFF (the macOS/embedded build never
# enables them), so the whole of `crates/rustty-apprt/src/gtk/**` — ~30k lines of
# ported GTK4/libadwaita/Wayland/X11 code — was linted by NO lane at all. Code
# that is never compiled cannot fail a lint, so that lane was green by
# construction rather than by verification.
#
# It had in fact been red for some time. When this gate was written the exact
# command below reported two errors on `main` (631aaf453):
#   * `unused_mut` at `winproto/wayland/queue_source.rs:103`
#   * `clippy::large_enum_variant` on `winproto::Window` (576-byte Wayland arm)
# Both were fixed in the commit preceding this one.
#
# This is the same shape as G-ONIG (`check_oniguruma_lane.sh`, 2026-07-30), which
# closed the identical hole for `--features oniguruma`. That gate is this one's
# model, including the anti-vacuity design below.
#
# ANTI-VACUITY — WHY THE CLIPPY RUN ALONE WOULD NOT BE ENOUGH
# -----------------------------------------------------------
# Ask the question this project keeps failing to ask: IF THE THING THIS GATE
# WATCHES DISAPPEARED ENTIRELY, WOULD IT STILL BE GREEN? For a bare
# `cargo clippy --features gtk,wayland,x11 -- -D warnings` the answer is YES —
# if a renamed `cfg`, a moved module or a dropped optional dependency stopped the
# features from pulling in the GTK code, clippy would lint an empty shell and
# exit 0. That is exactly the failure mode the gate exists to catch, so the gate
# also MEASURES how much gated code the feature set actually brings in.
#
# The measurement is a DELTA, not an absolute: `-p rustty-apprt --features ...`
# lists the crate's WHOLE test surface, so an absolute floor would still hold
# after every gated test vanished. `listed-with-features minus listed-without`
# IS the count of feature-gated tests and nothing else moves it.
#
# Two further deliberate choices:
#   * The delta counts LISTED tests (`-- --list`), not PASSED ones. Listing
#     counts what COMPILED; ~50 of these tests are `#[ignore]`d or self-skip
#     without a live display/compositor, so a passed-count floor would measure
#     the CI box's display rather than the code under lint.
#   * The gtk delta alone is too coarse to protect the `wayland` and `x11` arms:
#     losing either would move a 140-test delta by 3 or 4. So their gated
#     integration binaries (`gtk_wayland_winproto.rs`, `gtk_x11_winproto.rs`,
#     both `#![cfg(feature = ...)]`) are floored BY NAME as well. If either
#     feature stops compiling its backend, those binaries stop existing and this
#     arm goes RED even though the clippy run is still green.
#
# NON-GTK-TARGET HOST: BLOCKED, NEVER A SILENT GREEN
# --------------------------------------------------
# `src/gtk.rs` is
# `#[cfg(all(any(target_os = "linux", target_os = "freebsd"), feature = "gtk"))]`
# (W2D/F-002 widened the OS half; the oracle gives FreeBSD the same `.gtk`
# runtime, `apprt/runtime.zig:14-24`) and every GTK dependency is tabled on those
# same two targets, so on macOS `--features gtk` resolves to a no-op: the command
# would exit 0 while checking nothing, which is precisely the vacuity above
# wearing a different hat. On a host that is neither Linux nor FreeBSD — or on
# one of those without the gtk4/wayland/x11 development libraries — this gate
# exits 3 (BLOCKED), which `ci.sh`'s `run_gate` renders as a WARNING and never
# counts as a pass. Run it on the Linux VM (a FreeBSD host would work too; none
# exists in this project yet).
#
# EXIT CODES
#   0  clean
#   1  the clippy lane has a warning, or a gated-test floor was breached
#   3  BLOCKED — this host cannot build the GTK lane (not a pass)
set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

FEATURES="gtk,wayland,x11"

# Measured 2026-07-31 on the Fedora 44 VM (gtk4 4.22.4, wayland-client 1.25.0,
# libX11 1.8.13): `cargo test -p rustty-apprt --features gtk,wayland,x11 --
# --list` lists 153 tests, the default-feature arm lists 13 -> 140 are gated.
# Raise these floors when gated tests are added; never lower one without saying
# why in the same commit.
# 2026-08-11 (W2D/W16-GTK-02): +1 gated test —
# `tests/gtk_seam.rs::gtk_exports_resources_dir_and_global_init_routes_through_it`,
# the KAT that global init routes `resources_dir` through the gtk apprt's
# Flatpak-aware override. 140 -> 141.
GTK_DELTA_FLOOR=141
WAYLAND_NAME_FLOOR=3
X11_NAME_FLOOR=4

echo "G-GTKFEAT: the --features $FEATURES lane (no other CI lane compiles it)"

# ---- environment ---------------------------------------------------------
case "$(uname -s)" in
  Linux | FreeBSD) ;;
  *)
    echo "  BLOCKED: this host is $(uname -s), which is neither Linux nor FreeBSD."
    echo "  crates/rustty-apprt/src/gtk is cfg(any(target_os = \"linux\", target_os ="
    echo "  \"freebsd\")) and its deps are tabled on those two targets, so the lint"
    echo "  would pass here WITHOUT COMPILING ANY OF THE GATED CODE. Run this gate on"
    echo "  the Linux box; a skip here is a WARNING, not a pass."
    exit 3
    ;;
esac
missing=""
for pc in gtk4 wayland-client x11; do
  pkg-config --exists "$pc" 2>/dev/null || missing="$missing $pc"
done
if [ -n "$missing" ]; then
  echo "  BLOCKED: missing development packages:$missing"
  echo "  The GTK lane cannot be compiled on this host, so it cannot be linted."
  exit 3
fi

fail=0

# ---- the lane itself -----------------------------------------------------
# This is verbatim the command that was red on main. `--all-targets` matters:
# the gated integration tests under crates/rustty-apprt/tests/gtk_*.rs are lint
# surface too.
clippy_out="$(cargo clippy -p rustty-apprt --all-targets --features "$FEATURES" -- -D warnings 2>&1)"
clippy_rc=$?
if [ "$clippy_rc" -ne 0 ]; then
  echo "  RED: cargo clippy -p rustty-apprt --all-targets --features $FEATURES -- -D warnings"
  echo "       FAILED (exit $clippy_rc)"
  printf '%s\n' "$clippy_out" | grep -E "^(error|warning)" | head -20
  fail=1
else
  echo "  clippy -D warnings: clean"
fi

# ---- the anti-vacuity arm ------------------------------------------------
list_tests() { cargo test -p rustty-apprt "$@" -- --list 2>/dev/null | grep -c ': test$'; }

with_list="$(cargo test -p rustty-apprt --features "$FEATURES" -- --list 2>/dev/null)"
with_n="$(printf '%s\n' "$with_list" | grep -c ': test$')"
base_n="$(list_tests)"
delta=$(( with_n - base_n ))
wl_n="$(printf '%s\n' "$with_list" | grep ': test$' | grep -c '^wayland_')"
x11_n="$(printf '%s\n' "$with_list" | grep ': test$' | grep -c '^x11_')"
echo "  gated tests: $with_n listed with features, $base_n without -> $delta gated"
echo "               (wayland_* $wl_n, x11_* $x11_n)"

if [ "$delta" -lt "$GTK_DELTA_FLOOR" ]; then
  echo "  RED (VACUITY): the feature set adds only $delta tests, floor is $GTK_DELTA_FLOOR."
  echo "  Fewer GATED tests were COMPILED than when this gate was written. Either"
  echo "  tests were removed (lower the floor DELIBERATELY, in the same commit) or"
  echo "  the features stopped pulling in crates/rustty-apprt/src/gtk — in which"
  echo "  case the clippy run above linted an empty shell and was green while"
  echo "  verifying nothing. That is why this measures a DELTA and not a total."
  fail=1
fi
if [ "$wl_n" -lt "$WAYLAND_NAME_FLOOR" ]; then
  echo "  RED (VACUITY): $wl_n wayland_* tests compiled, floor is $WAYLAND_NAME_FLOOR."
  echo "  The wayland backend's gated integration binary is gone or renamed; the"
  echo "  clippy run above is no longer covering src/gtk/winproto/wayland*."
  fail=1
fi
if [ "$x11_n" -lt "$X11_NAME_FLOOR" ]; then
  echo "  RED (VACUITY): $x11_n x11_* tests compiled, floor is $X11_NAME_FLOOR."
  echo "  The x11 backend's gated integration binary is gone or renamed; the clippy"
  echo "  run above is no longer covering src/gtk/winproto/x11.rs."
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "G-GTKFEAT: RED"
  exit 1
fi
echo "G-GTKFEAT: GREEN (floors: gtk delta>=$GTK_DELTA_FLOOR, wayland_*>=$WAYLAND_NAME_FLOOR, x11_*>=$X11_NAME_FLOOR)"
