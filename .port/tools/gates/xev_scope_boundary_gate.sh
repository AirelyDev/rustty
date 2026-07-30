#!/bin/sh
# xev REACHABILITY gate — G-XEVREACH.
#
# ── WHAT CHANGED, AND WHY (WS20, 2026-07-30) ────────────────────────────────
# This file used to be the "xev SCOPE-BOUNDARY gate" (G-XEVSCOPE). It asserted a
# FACT — no wasm artifact links `rustty-xev` — and then drew a CONCLUSION from it:
# "therefore the iocp/wasi_poll seam bodies are unreachable in every shipped
# build", which was the load-bearing leg of a DEFERRALS.md entry classifying both
# backends as OUT OF SCOPE BY PLATFORM.
#
# That conclusion is VOID. The rule for this port is that everything the
# reference supports gets ported; an inability to build or run a target on the
# porting host is verification debt, never a scope exclusion. Both backends are
# now ported in full (`crates/rustty-xev/src/{iocp,windows,wasi_poll,wasi}.rs`),
# and the DEFERRALS entry that rested on this gate is RETRACTED.
#
# The gate is REPURPOSED rather than retired because its FACT is still worth
# holding, for a reason that has nothing to do with scope:
#
#   * R4 (acyclic, minimal crate graph) and the wasm artifacts' size/shape budget.
#     `rustty-wasm`, `rustty-web-canvas` and `rustty-vt-c` run the SYNCHRONOUS
#     C-ABI with no event loop. An xev edge appearing in one of their trees would
#     be an unintended dependency, not a scope violation — the same class of
#     regression as any other accidental edge, and this is the cheapest check for
#     it.
#   * The `rustty-os -> rustty-xev` edge must stay macOS-gated, or every wasm
#     artifact inherits it transitively.
#
# So: the assertions are unchanged, the CONCLUSION is deleted, and the messages no
# longer say anything about scope. If a wasm artifact grows an xev edge, this gate
# goes RED and someone must decide whether that edge is wanted — it is no longer a
# claim that any code became "real work".
#
# Exit 0 = pass; 1 = FAIL (an unintended xev edge); 3 = environment missing
# (cargo absent), never a silent green.
#
# Run:  sh .port/tools/gates/xev_scope_boundary_gate.sh
set -u
ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
cd "$ROOT"

command -v cargo >/dev/null 2>&1 || {
  echo "BLOCKED: cargo not on PATH." >&2
  exit 3
}

fail=0

check() {
  pkg=$1
  target=$2
  out=$(cargo tree -p "$pkg" --target "$target" -e normal 2>/dev/null)
  if [ -z "$out" ]; then
    echo "BLOCKED: 'cargo tree -p $pkg --target $target' produced nothing." >&2
    exit 3
  fi
  # Self-check: a tree that does not even name its own package is not a tree.
  echo "$out" | grep -q "^$pkg " || {
    echo "BLOCKED: 'cargo tree -p $pkg --target $target' did not name $pkg." >&2
    exit 3
  }
  if echo "$out" | grep -q 'rustty-xev'; then
    echo "FAIL: $pkg ($target) DEPENDS ON rustty-xev — an unintended edge."
    echo "      These artifacts run the synchronous C-ABI with no event loop;"
    echo "      pulling the event-loop crate into a wasm tree is a dependency"
    echo "      regression. Drop the edge, or update this gate deliberately."
    fail=1
  else
    echo "ok: $pkg ($target) — no rustty-xev edge"
  fi
}

# --self-test: prove the FAIL arm is REACHABLE, so a green run means something.
# A gate that cannot fail is worthless (this program has found eight vacuous ones).
# The negative control is real, not synthetic: `rustty-os` genuinely DOES link
# rustty-xev on macOS, so pointing the same `check` at that target must trip the
# very grep the wasm arms rely on. If this arm ever prints "ok", the detection has
# stopped working and every GREEN this gate ever printed is void.
if [ "${1:-}" = "--self-test" ]; then
  echo "== self-test: the FAIL arm must be reachable =="
  fail=0
  check rustty-os aarch64-apple-darwin
  if [ "$fail" -eq 0 ]; then
    echo "SELF-TEST FAILED: rustty-os DOES link rustty-xev on macOS, but the check" >&2
    echo "                  reported ok — the detection is vacuous." >&2
    exit 1
  fi
  echo "self-test: FAIL arm REACHABLE (detection fired on a real xev edge)"
  fail=0
  echo
  echo "== self-test: the PASS arm must still hold =="
fi

check rustty-wasm       wasm32-unknown-unknown
check rustty-web-canvas wasm32-unknown-unknown
check rustty-vt-c       wasm32-unknown-unknown
check rustty-vt-c       wasm32-wasip1

# The one xev edge that exists below the app tier must stay macOS-gated, or a
# future wasm artifact would inherit it through rustty-os.
if grep -q '^rustty-xev = { workspace = true }' crates/rustty-os/Cargo.toml; then
  ctx=$(grep -B 12 '^rustty-xev = { workspace = true }' crates/rustty-os/Cargo.toml |
    grep '^\[target\.' | tail -1)
  case "$ctx" in
  "[target.'cfg(target_os = \"macos\")'.dependencies]")
    echo "ok: rustty-os -> rustty-xev is macOS-gated ($ctx)"
    ;;
  *)
    echo "FAIL: rustty-os -> rustty-xev is NOT macOS-gated (section: ${ctx:-<none>})."
    fail=1
    ;;
  esac
else
  echo "FAIL: could not locate the rustty-os -> rustty-xev edge (Cargo.toml changed?)."
  fail=1
fi

# WS20 added a SECOND rustty-os -> rustty-xev edge: `os/flatpak.zig`'s
# FlatpakHostCommand names `xev.Loop`/`xev.Timer`/`xev.Completion` for `waitXev`.
# It lives in the top-level [dependencies] (flatpak is a Linux tier, not macOS), so
# it cannot be macOS-gated — it must instead be OPTIONAL and reachable only via the
# `flatpak` feature, which is OFF by default. Otherwise every wasm artifact
# inherits it, exactly as the macOS gate above prevents.
if grep -q '^rustty-xev = { workspace = true, optional = true }' crates/rustty-os/Cargo.toml; then
  if grep -q '^flatpak = \["dep:rustty-xev"\]' crates/rustty-os/Cargo.toml; then
    echo "ok: rustty-os -> rustty-xev (flatpak) is optional + feature-gated, default-off"
  else
    echo "FAIL: rustty-os's optional rustty-xev dep is not reached via the flatpak feature."
    fail=1
  fi
else
  echo "note: rustty-os declares no optional rustty-xev dep (nothing to check)"
fi

if [ "$fail" -ne 0 ]; then
  echo
  echo ">> xev reachability gate (G-XEVREACH): RED"
  exit 1
fi

echo
echo ">> xev reachability gate (G-XEVREACH): GREEN — no wasm artifact links"
echo "   rustty-xev, and the one sub-app-tier xev edge stays macOS-gated."
echo "   (This gate draws NO scope conclusion: the iocp and wasi_poll backends"
echo "    are PORTED — see the RETRACTED entry in .port/phase-2/DEFERRALS.md.)"
exit 0
