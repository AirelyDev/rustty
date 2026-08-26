#!/usr/bin/env bash
# xev_ios_kqueue_gate.sh — the rustty-xev Darwin-gate parity gate.
#
# WHY THIS EXISTS. `libxev`'s `backend.zig:20,38` maps `.ios` and `.visionos`
# onto `.kqueue` exactly as it maps `.macos`, and the mach paths inside
# `kqueue.zig` are gated `isDarwin()`, which covers all three. rustty's port
# gated its kqueue family on `macos + freebsd` only, while `backend.rs` already
# answered `Backend::Kqueue` for iOS. The two halves disagreed, and the crate
# handed an iOS build the `no_backend_seam` placeholders: every `Loop`, `Async`,
# `Timer`, `Process` and `Stream` entry point was an `unimplemented!()` panic on
# a target the reference runs for real (ghostty ships a libghostty iOS build).
#
# Nothing caught it, because nothing had ever built the crate for iOS. This gate
# does, so the Darwin gates cannot silently narrow again.
#
# WHAT THIS GATE DOES NOT DO — the limit stated rather than left implicit: it
# type-checks only. It never links, never runs a binary, and no iOS device or
# simulator lane exists in this repo, so the widened mach/kqueue arms are proven
# for iOS up to codegen and no further. `visionos` is a tier-3 target with no
# rustup `rust-std`, so that arm is not type-checked even here and remains
# codegen debt. A real iOS-simulator test lane is the honest follow-up.
#
# EXIT 3 = BLOCKED (a warning, never a pass): the aarch64-apple-ios rust-std is
# not installed on this box. Same convention as w2f001_windows_check_gate.sh.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

TARGET=aarch64-apple-ios
LIB=crates/rustty-xev/src/lib.rs
GATES="$(git rev-parse --show-toplevel)/.port/tools/gates"

if ! rustc --print target-libdir --target "$TARGET" >/dev/null 2>&1; then
  echo "BLOCKED: rust-std for $TARGET is not installed." >&2
  echo "         install with: rustup target add $TARGET" >&2
  exit 3
fi

# --------------------------------------------------------------------------
# --self-test: prove the FAIL arm is REACHABLE.
#
# A gate that has only ever been observed green is bookkeeping. This arm drops
# `target_os = "ios"` from the seam's two `not(any(...))` lists — reintroducing
# the exact bug that was fixed — and REQUIRES the build to go red ON THE SEAM
# FENCE specifically (a red build for any other reason is reported as a broken
# self-test, not a pass). The mutation is applied to the working tree and undone
# by an EXIT trap; `git diff --quiet` afterwards is the belt-and-braces check.
# --------------------------------------------------------------------------
if [ "${1:-}" = "--self-test" ]; then
  if ! git diff --quiet -- "$LIB"; then
    echo "SELF-TEST REFUSED: $LIB has uncommitted changes, so the mutation is" >&2
    echo "                   not safely reversible. Commit or stash first." >&2
    exit 2
  fi
  restore() { git checkout -- "$LIB"; }
  trap restore EXIT

  python3 - "$LIB" <<'PY'
import io, sys
p = sys.argv[1]
s = io.open(p, encoding="utf-8").read()
# Match the seam's FULL cfg block, not just the ios/visionos/linux run: that
# shorter run also appears in the `thread_pool::ThreadPool` re-export gate, so
# matching on it would mutate an unrelated cfg and test the wrong thing.
needle = (
    '#[cfg(not(any(\n'
    '    target_os = "macos",\n'
    '    target_os = "ios",\n'
    '    target_os = "visionos",\n'
    '    target_os = "linux",\n'
    '    target_os = "freebsd",\n'
    '    target_os = "windows",\n'
    '    target_os = "wasi"\n'
    ')))]\n'
)
n = s.count(needle)
if n != 2:
    sys.exit(
        "SELF-TEST BROKEN: expected exactly 2 seam cfg blocks (the `pub use` and "
        "the `mod`), found %d. Has the seam been reshaped or reformatted?" % n
    )
io.open(p, "w", encoding="utf-8").write(
    s.replace(needle, needle.replace('    target_os = "ios",\n', ""))
)
PY

  echo "== self-test: ios dropped from the seam lists; the build MUST go red =="
  if out=$(cargo check -p rustty-xev --target "$TARGET" 2>&1); then
    echo "SELF-TEST FAILED: the build stayed GREEN with the bug reintroduced;" >&2
    echo "                  the seam fence is not gating." >&2
    exit 1
  fi
  if ! printf '%s' "$out" | grep -q "no-backend seam is active for a NON-wasm target"; then
    echo "SELF-TEST FAILED: the build went red, but NOT on the seam fence:" >&2
    printf '%s\n' "$out" | tail -20 >&2
    exit 1
  fi
  echo "self-test OK: the seam fence fired, as designed."

  restore
  trap - EXIT
  git diff --quiet -- "$LIB" || { echo "SELF-TEST: restore FAILED!" >&2; exit 1; }
  echo "self-test: working tree restored."
  exit 0
fi

echo "== 1/3: rustty-xev type-checks for $TARGET (check-only; no linking) =="
cargo check -p rustty-xev --target "$TARGET"

# Step 1 passing is NOT by itself proof the REAL backend is compiled: the
# placeholder seam type-checks too, which is exactly how the bug hid. The
# in-crate seam fence now makes that specific confusion a build error, and this
# step covers the other half — a cfg that narrows the Darwin set without
# activating the seam at all.
echo "== 2/3: the Darwin gates name the reference's full set =="
python3 "$GATES/check_xev_darwin_set.py"

echo "== 3/3: host lane must not regress =="
cargo test -p rustty-xev
