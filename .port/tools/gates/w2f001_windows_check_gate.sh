#!/usr/bin/env bash
# w2f001_windows_check_gate.sh — W2 / F-001 gate.
#
# Proves that the shared OS layer plus VT and VT-C TYPE-CHECK for
# `x86_64-pc-windows-msvc` (the ConPTY / VirtualAlloc surface the oracle CI-tests
# via `build-libghostty-vt-windows`), and that the macOS/Linux host lane does not
# regress. Design: `.port/codex-vet/designs/W2D_WINDOWS_OS.md`.
#
# WHAT THIS GATE DOES NOT DO — stated so the limit is explicit, not silent:
# it never LINKS and never RUNS anything on Windows. No Windows host or CI lane
# exists in this repo, so the windows arms of `page_allocator.rs` (ntdll
# NtAllocate/NtFreeVirtualMemory), `pty.rs` (ConPTY) and `page.rs`
# (VirtualAlloc/VirtualFree) are proven only up to type-checking against
# hand-transcribed Win32/Zig-std signatures. A real windows build/test lane is
# the honest follow-up.
#
# `RUSTTY_SYS_CHECK_ONLY=1` makes the vendored-C `-sys` build scripts
# (highway/simdutf/zlib + rustty-simd's own cc build) skip their `cc`
# invocations: cross-compiling them to MSVC needs a sysroot this host does not
# have, and `cargo check` never links, so the objects are pure waste here. The
# switch is OPT-IN — step 2 below runs with it UNSET, so a wrongly-skipped native
# build would show up as a host failure rather than a silent green.
#
# EXIT 3 = BLOCKED (a warning, never a pass): the `x86_64-pc-windows-msvc`
# rust-std is not installed on this box, so step 1 cannot bind. Same convention
# as `check_gtk_feature_lane.sh` on non-Linux — a missing environment must not
# be reported as green.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if ! rustc --print target-libdir --target x86_64-pc-windows-msvc >/dev/null 2>&1; then
  echo "BLOCKED: rust-std for x86_64-pc-windows-msvc is not installed." >&2
  echo "         install with: rustup target add x86_64-pc-windows-msvc" >&2
  exit 3
fi

echo "== 1/3: windows-msvc type-check lane (check-only; no linking) =="
RUSTTY_SYS_CHECK_ONLY=1 cargo check -p rustty-os -p rustty-vt -p rustty-vt-c \
  --target x86_64-pc-windows-msvc

echo "== 2/3: host check must not regress (env UNSET: the real native path) =="
cargo check -p rustty-os -p rustty-vt -p rustty-vt-c

echo "== 3/3: host rustty-os unit tests =="
cargo test -p rustty-os --lib

echo "PASS: F-001 windows check gate"
