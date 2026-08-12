#!/usr/bin/env bash
# G-WINPROTO-DEFAULT — F-007: the DEFAULT Linux build must compile BOTH native
# winproto arms (Wayland + X11), and the macOS default build must compile
# NEITHER.
#
# W2D/F-002 ADDED A THIRD ARM: FreeBSD. `apprt/runtime.zig:14-24` selects `.gtk`
# for `.linux, .freebsd` alike, so ghostty's default FreeBSD build IS a GTK app.
# rustty's gtk module, its gtk4-rs dep tables, the `main_rustty.rs` launch arm and
# the cli ipc arms were all `target_os = "linux"`-only, so a FreeBSD build had
# `app_runtime() == .gtk` and no compiled backend (deterministic `unimplemented!`).
# Every one of those gates is now `any(target_os = "linux", target_os =
# "freebsd")`, and the FreeBSD arm below is the machine-checkable predicate for
# it: the default FreeBSD dependency graph must contain the same GTK + winproto
# crates the Linux one does. `cargo tree --target` resolves the real cfg-tabled
# graph without building, so this arm runs on THIS macOS host even though a
# FreeBSD gtk `cargo check` cannot (pkg-config needs a FreeBSD GTK install).
#
# WHY THIS EXISTS
# ---------------
# ghostty's build derives its gtk-wayland/gtk-x11 defaults from
# `pkg-config --variable=targets gtk4` (src/build/gtk.zig:9-27 feeding
# src/build/Config.zig:208-218) — on a normal distro GTK both default ON.
# rustty's `wayland`/`x11` Cargo features used to be default-OFF, so the
# documented Linux package build (PACKAGING.md: `cargo build --release -p
# rustty` with no feature flags) compiled only the noop winproto arm and
# silently lost SSD decorations, blur, urgency, the quick terminal, and
# WINDOWID. F-007 made both arms default-ON in crates/rustty/Cargo.toml;
# this gate pins that default in BOTH directions.
#
# The probes are the exact pair used by the F-007 audit: resolve the `rustty`
# dependency graph per target and grep for the protocol crates
# (gdk4-wayland / gdk4-x11 / wayland-client). `cargo tree` resolves without
# building, so this gate is cheap and needs no Linux host.
#
# ANTI-VACUITY: if the feature plumbing broke entirely (rustty-apprt renamed
# its features, the target-gated dep table moved, ...), a bare "macOS graph has
# no protocol crates" check would stay green. So the Linux arm REQUIRES the
# protocol crates to appear (a broken probe fails loudly), and both arms first
# require `rustty-apprt` itself in the graph so an empty/errored `cargo tree`
# output can never pass.
set -euo pipefail

cd "$(dirname "$0")/../../.."

LINUX_TARGET=x86_64-unknown-linux-gnu
MACOS_TARGET=aarch64-apple-darwin
FREEBSD_TARGET=x86_64-unknown-freebsd
PROTO_CRATES=(gdk4-wayland gdk4-x11 wayland-client)
# The gtk apprt backend proper (W2D/F-002). Kept separate from PROTO_CRATES so a
# regression names which half fell out.
GTK_CRATES=(gtk4 libadwaita)

fail() {
  echo "G-WINPROTO-DEFAULT: FAIL: $*" >&2
  exit 1
}

linux_graph="$(cargo tree -p rustty --target "$LINUX_TARGET")" \
  || fail "cargo tree ($LINUX_TARGET) failed"
macos_graph="$(cargo tree -p rustty --target "$MACOS_TARGET")" \
  || fail "cargo tree ($MACOS_TARGET) failed"
freebsd_graph="$(cargo tree -p rustty --target "$FREEBSD_TARGET")" \
  || fail "cargo tree ($FREEBSD_TARGET) failed"

# Anti-vacuity: both graphs must actually contain the apprt crate.
grep -q "rustty-apprt" <<<"$linux_graph" \
  || fail "probe broken: rustty-apprt missing from the Linux graph"
grep -q "rustty-apprt" <<<"$macos_graph" \
  || fail "probe broken: rustty-apprt missing from the macOS graph"
grep -q "rustty-apprt" <<<"$freebsd_graph" \
  || fail "probe broken: rustty-apprt missing from the FreeBSD graph"

# Default Linux graph: BOTH native winproto arms present (ghostty's effective
# default on a normal distro GTK).
for dep in "${PROTO_CRATES[@]}"; do
  grep -q "$dep" <<<"$linux_graph" \
    || fail "default Linux graph is missing $dep — the wayland/x11 features fell out of the rustty default set (F-007 regression)"
done

# Default macOS graph: NEITHER arm (R5 — the features are Linux-target-gated
# no-ops; default-on must not leak protocol crates into macOS/embedded builds).
for dep in "${PROTO_CRATES[@]}"; do
  ! grep -q "$dep" <<<"$macos_graph" \
    || fail "macOS default graph contains $dep — the Linux target gate leaked (R5 regression)"
done

# Default FreeBSD graph (W2D/F-002): the GTK backend AND both winproto arms,
# byte-for-byte the Linux set — `Runtime.default()` is `.gtk` there too
# (`apprt/runtime.zig:18`), so anything the Linux GUI build compiles the FreeBSD
# GUI build must compile as well.
for dep in "${GTK_CRATES[@]}" "${PROTO_CRATES[@]}"; do
  grep -q "$dep" <<<"$freebsd_graph" \
    || fail "default FreeBSD graph is missing $dep — a gtk gate is still linux-only (F-002 regression)"
done

# ...and the macOS control must contain NEITHER set, so the FreeBSD arm above
# cannot be passing merely because the target gate stopped gating.
for dep in "${GTK_CRATES[@]}"; do
  ! grep -q "$dep" <<<"$macos_graph" \
    || fail "macOS default graph contains $dep — the gtk target gate leaked (R5 regression)"
done

echo "G-WINPROTO-DEFAULT: OK (Linux + FreeBSD defaults: gtk backend and both winproto arms; macOS default: neither)"
