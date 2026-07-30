#!/bin/sh
# xev SCOPE-BOUNDARY gate (WS19 T7).
#
# `crates/rustty-xev/src/lib.rs`'s `unsupported_target_seam` carries 10 panicking
# handle methods for the `iocp` (Windows) and `wasi_poll` (WASI/wasm) backends.
# Those are recorded as a permanent Justified R1 Divergence (out of scope by
# platform), and that classification rests on ONE falsifiable claim:
#
#     no artifact rustty actually builds can reach those bodies.
#
# Windows needs no check — the port emits no Windows artifact at all. wasm DOES:
# `backend.rs` selects `Backend::WasiPoll` for `target_family = "wasm"`, so if any
# wasm deliverable ever grew an xev edge, the seam would stop being a scope
# boundary and become real unported work. This gate is that check: it asserts
# `rustty-xev` is absent from the dependency graph of every wasm artifact, and
# that `rustty-os`'s single xev edge stays macOS-gated.
#
# Exit 0 = pass (scope boundary holds); 1 = FAIL (a wasm artifact links xev — the
# divergence entry is void and the backend must be ported); 3 = environment
# missing (cargo absent), never a silent green.
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
    echo "FAIL: $pkg ($target) DEPENDS ON rustty-xev — the wasi_poll seam is reachable."
    echo "      The 'out of scope by platform' divergence for wasi_poll is VOID;"
    echo "      port the backend or drop the edge."
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

if [ "$fail" -ne 0 ]; then
  echo
  echo ">> xev scope-boundary gate: RED"
  exit 1
fi

echo
echo ">> xev scope-boundary gate: GREEN — no wasm artifact links rustty-xev;"
echo "   the iocp/wasi_poll seam bodies are unreachable in every shipped build."
exit 0
