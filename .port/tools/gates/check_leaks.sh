#!/usr/bin/env bash
# check_leaks.sh — GATE G6: memory safety (UB + leaks).
#
# RIGHT TOOL PER JOB:
#   * pure-Rust crates  -> Miri  (`cargo +nightly miri test -p <crate>`):
#       catches UB (use-after-free, OOB, uninit, invalid aliasing) AND leaks.
#       This is the PRIMARY gate and supersedes any tracking-allocator approach.
#   * Rust<->C FFI crates -> AddressSanitizer (`-Zsanitizer=address`):
#       Miri cannot execute foreign C, so FFI crates use ASan for UB across the
#       boundary. (FFI *leak* detection: LSan/valgrind are unavailable on arm64
#       macOS, so FFI leak checks are deferred to Linux CI — see DoP §G6.)
#   * live-syscall test binaries in otherwise-pure crates -> ASan:
#       a test whose PURPOSE is a real syscall loop (kqueue/mach-port) cannot
#       execute under Miri at all ("can't call foreign function `kqueue`");
#       it is ROUTED to ASan, never skipped (R5/R6). Per-binary list +
#       4-criterion justification: .port/phase-2/MIRI_EXCEPTIONS.md
#       ("Live-syscall test binaries").
#
# Done-detection (H5 fix): shared with G1/G2/G3b via _gatelib (PORT_STATUS_RE +
# DONE_STATUSES = done|complete|parity|ported|final) — a `PORT STATUS: ported`
# crate is gated exactly like a `done` one. The enumerator visits BOTH
# crates/*/Cargo.toml and crates-sys/*/Cargo.toml (sys statuses are currently
# 'vendored'/'stub'/... which are not completion claims, so none run today —
# but they are enumerated, listed by --list, and gate automatically the moment
# one claims a done-class status).
#
# Usage:
#   check_leaks.sh                    same as --all-done (the G6 gate run)
#   check_leaks.sh --all-done         run Miri/ASan for every claimed-done crate
#   check_leaks.sh --crate <name>     run the right tool for that crate
#   check_leaks.sh --list             enumerate crates + status + lane (no run)
#   check_leaks.sh --self-test        prove Miri catches a leak and a UB
# Exit 0 = GREEN (clean) ; non-zero = RED (UB/leak) or tool genuinely missing.
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
cd "$RUSTTY_ROOT"

# Crates whose tests cross into C (depend on a crates-sys/* crate) -> ASan, not Miri.
# rustty-wuffs wraps wuffs-sys C FFI (H5: it was absent from this list while its
# 'ported' status also escaped the old literal-'done' grep — doubly unreachable).
FFI_CRATES="rustty-simd rustty-font rustty-renderer rustty-config rustty-crash rustty-termio rustty-inspector rustty-font-shaper rustty-oniguruma rustty-input rustty-wuffs"
# Every crates-sys/* crate is FFI by construction (it exists to vendor C/C++),
# so the *-sys pattern routes the whole tree to ASan without a hand list.
is_ffi() {
  case "$1" in *-sys) return 0;; esac
  case " $FFI_CRATES " in *" $1 "*) return 0;; *) return 1;; esac
}

# Live-FFI integration-test binaries Miri cannot execute (real syscalls).
# These are NOT skipped (R5): the crate's Miri lane covers its unit surface
# (--lib) and each listed binary runs under ASan instead (R6 right-tool).
# Any addition REQUIRES a matching 4-criterion entry in
# .port/phase-2/MIRI_EXCEPTIONS.md ("Live-syscall test binaries").
# run_crate() enforces completeness: an unlisted tests/*.rs binary in a crate
# named here is a G6 RED (it would otherwise escape both lanes).
live_ffi_tests() {
  case "$1" in
    rustty-os) echo "cf_release_live";;  # real kqueue/mach-port xev loop
    *) echo "";;
  esac
}

# Unified done-detection (H5): defer to _gatelib's PORT_STATUS_RE + DONE_STATUSES
# so G6 can never disagree with G1/G2/G3b about what "claimed done" means.
crate_status_of() {  # $1 = crate directory (crates/... or crates-sys/...)
  python3 -c '
import sys, os
sys.path.insert(0, sys.argv[2])
import _gatelib as G
print(G.dir_status(sys.argv[1]))
' "$1" "$GATES_DIR"
}
claimed_done() {  # $1 = crate directory; exit 0 iff status is a completion claim
  python3 -c '
import sys, os
sys.path.insert(0, sys.argv[2])
import _gatelib as G
sys.exit(0 if G.dir_status(sys.argv[1]) in G.DONE_STATUSES else 1)
' "$1" "$GATES_DIR"
}

require_miri() {
  if ! cargo +nightly miri --version >/dev/null 2>&1; then
    echo "G6 RED: cargo miri not available (rustup component add miri --toolchain nightly)"; exit 2
  fi
}

run_crate() {
  local c="$1"
  if is_ffi "$c"; then
    echo "=== G6 [$c] :: ASan (FFI crate; Miri can't run C) ==="
    RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" \
      cargo +nightly test -p "$c" --target "$(rustc -vV | sed -n 's/host: //p')" 2>&1
    local rc=$?
    echo "(FFI leak detection for $c is deferred to Linux CI: LSan/valgrind unavailable on arm64 macOS)"
    return $rc
  fi
  local live
  live="$(live_ffi_tests "$c")"
  if [ -n "$live" ]; then
    # GUARD (tightening): with the Miri lane narrowed to --lib, EVERY
    # integration-test binary (tests/*.rs) of this crate must be explicitly
    # routed in live_ffi_tests(), otherwise a future unlisted binary would run
    # under neither Miri nor ASan in G6 — that is a RED, not a silent escape.
    local tf tb unrouted=""
    for tf in "crates/$c"/tests/*.rs "crates-sys/$c"/tests/*.rs; do
      [ -f "$tf" ] || continue
      tb="$(basename "$tf" .rs)"
      case " $live " in *" $tb "*) ;; *) unrouted="$unrouted $tb";; esac
    done
    if [ -n "$unrouted" ]; then
      echo "G6 RED [$c]: unrouted integration-test binaries:$unrouted"
      echo "  (this crate's Miri lane is --lib only; add each binary to live_ffi_tests()"
      echo "   WITH a 4-criterion MIRI_EXCEPTIONS.md entry, or it escapes both lanes)"
      return 1
    fi
    # ROUTED, not skipped (R5): Miri proves the crate's unit surface; each
    # live-syscall test binary (which Miri hard-errors on: "can't call foreign
    # function \`kqueue\`") runs under ASan. MIRI_EXCEPTIONS.md has the
    # 4-criterion justification per binary.
    echo "=== G6 [$c] :: Miri (unit surface --lib; live-FFI test binaries routed to ASan per MIRI_EXCEPTIONS.md) ==="
    require_miri
    local rc=0
    cargo +nightly miri test -p "$c" --lib 2>&1 || rc=1
    local t
    for t in $live; do
      echo "=== G6 [$c --test $t] :: ASan (live syscalls; Miri cannot execute them) ==="
      RUSTFLAGS="-Zsanitizer=address" RUSTDOCFLAGS="-Zsanitizer=address" \
        cargo +nightly test -p "$c" --test "$t" --target "$(rustc -vV | sed -n 's/host: //p')" 2>&1 || rc=1
    done
    return $rc
  fi
  echo "=== G6 [$c] :: Miri (UB + leaks) ==="
  require_miri
  cargo +nightly miri test -p "$c" 2>&1
  return $?
}

enumerate_crate_dirs() {
  local cargo_toml
  for cargo_toml in crates/*/Cargo.toml crates-sys/*/Cargo.toml; do
    [ -f "$cargo_toml" ] || continue
    dirname "$cargo_toml"
  done
}

case "${1:---all-done}" in
  --self-test)
    require_miri
    D="$(mktemp -d)"
    ( cd "$D" && cargo init --lib --name g6scratch -q 2>/dev/null
      printf '%s\n' '#[test]' 'fn lk(){let _=Box::leak(Box::new(1u32));}' \
        '#[test]' 'fn ub(){let v=[0u8;3];let p=v.as_ptr();let _=unsafe{*p.add(9)};}' > src/lib.rs
      # NOTE: capture output first — under `set -o pipefail`, `miri(exit 1)|grep`
      # returns miri's non-zero status even when grep matches, which would
      # falsely invert the check.
      lk_out="$(cargo +nightly miri test lk 2>&1 || true)"
      printf '%s' "$lk_out" | grep -qiE 'memory leaked' && echo "self-test: leak DETECTED ok" || { echo "self-test FAIL: leak not detected"; exit 1; }
      ub_out="$(cargo +nightly miri test ub 2>&1 || true)"
      printf '%s' "$ub_out" | grep -qiE 'Undefined Behavior' && echo "self-test: UB DETECTED ok" || { echo "self-test FAIL: UB not detected"; exit 1; } )
    rc=$?; rm -rf "$D"; exit $rc ;;
  --crate)
    [ -n "${2:-}" ] || { echo "usage: --crate <name>"; exit 2; }
    run_crate "$2"; exit $? ;;
  --list)
    printf '%-24s %-28s %-8s %s\n' "crate" "status" "gated" "lane"
    while IFS= read -r dir; do
      c="$(basename "$dir")"
      status="$(crate_status_of "$dir")"
      if claimed_done "$dir"; then gated="YES"; else gated="no"; fi
      if is_ffi "$c"; then lane="ASan"
      elif [ -n "$(live_ffi_tests "$c")" ]; then lane="Miri(--lib)+ASan(live tests)"
      else lane="Miri"; fi
      printf '%-24s %-28s %-8s %s\n' "$c" "$status" "$gated" "$lane"
    done < <(enumerate_crate_dirs)
    exit 0 ;;
  --all-done)
    rc=0; ran=0
    while IFS= read -r dir; do
      c="$(basename "$dir")"
      claimed_done "$dir" || continue
      ran=1; run_crate "$c" || rc=1
    done < <(enumerate_crate_dirs)
    [ "$ran" = 0 ] && echo "G6: no claimed-done crates yet (progress)"
    [ "$rc" = 0 ] && echo "G6 GREEN (all done crates clean)"; exit $rc ;;
  *)
    echo "usage: check_leaks.sh [--crate <name> | --all-done (default) | --list | --self-test]"; exit 2 ;;
esac
