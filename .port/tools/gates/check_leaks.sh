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
#
# Usage:
#   check_leaks.sh --crate <name>     run the right tool for that crate
#   check_leaks.sh --all-done         run Miri/ASan for every PORT STATUS: done crate
#   check_leaks.sh --self-test        prove Miri catches a leak and a UB
# Exit 0 = GREEN (clean) ; non-zero = RED (UB/leak) or tool genuinely missing.
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
cd "$RUSTTY_ROOT"

# Crates whose tests cross into C (depend on a crates-sys/* crate) -> ASan, not Miri.
FFI_CRATES="rustty-simd rustty-font rustty-renderer rustty-config rustty-crash rustty-inspector rustty-font-shaper rustty-oniguruma rustty-input wuffs-sys glslang-sys"
is_ffi() { case " $FFI_CRATES " in *" $1 "*) return 0;; *) return 1;; esac; }

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
  else
    echo "=== G6 [$c] :: Miri (UB + leaks) ==="
    require_miri
    cargo +nightly miri test -p "$c" 2>&1
    return $?
  fi
}

case "${1:-}" in
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
  --all-done)
    rc=0; ran=0
    for cargo_toml in crates/*/Cargo.toml; do
      c="$(basename "$(dirname "$cargo_toml")")"
      librs="crates/$c/src/lib.rs"
      [ -f "$librs" ] || continue
      grep -qiE 'PORT STATUS:[[:space:]]*done' "$librs" || continue
      ran=1; run_crate "$c" || rc=1
    done
    [ "$ran" = 0 ] && echo "G6: no PORT STATUS: done crates yet (progress)"
    [ "$rc" = 0 ] && echo "G6 GREEN (all done crates clean)"; exit $rc ;;
  *)
    echo "usage: check_leaks.sh [--crate <name> | --all-done | --self-test]"; exit 2 ;;
esac
