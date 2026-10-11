#!/bin/sh
# ABI-fidelity gate (A5) for the published rustty C header family.
# Spec: docs/superpowers/specs/2026-07-15-rustty-c-header-family-design.md
set -e
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"

# Every read of the oracle goes through `oracle_git`: the shared
# `rustty_oracle_git` from `_oracle_root.sh` (every GIT_* variable removed),
# reached through bash because this gate runs under `sh`. A plain
# `git -C "$GH" show` honours GIT_DIR, so a decoy repository could answer
# arm (d) with any export list (R5).
ORACLE_LIB="$root/.port/tools/gates/_oracle_root.sh"
oracle_git() { bash -c '. "$1" || exit 3; shift; rustty_oracle_git "$@"' oracle_git "$ORACLE_LIB" "$@"; }
# oracle_exports ORACLE — the ONLY read of the oracle in this script: the
# R2-renamed `rustty_*` names of origin/main:src/lib_vt.zig's exports, sorted.
oracle_exports() {
  oracle_git -C "$1" show origin/main:src/lib_vt.zig 2>/dev/null \
    | grep -oE '\.name = "ghostty_[a-z0-9_]+"' \
    | sed 's/.*"ghostty_/rustty_/; s/"$//' | sort -u
}

# --self-test-oracle (hermetic; run by G-ORACLE-SH-SELF): a fixture oracle and
# a decoy, each exporting one different name at origin/main. With GIT_DIR +
# GIT_WORK_TREE naming the decoy, `oracle_exports` on the fixture must return
# the fixture's name. Then the same for the Node registry reader of arm (c).
if [ "${1:-}" = "--self-test-oracle" ]; then
  st=$(mktemp -d "${TMPDIR:-/tmp}/a5_oracle.XXXXXX")
  if ! bash -c '. "$1" || exit 1
      rustty_fixture_repo "$2/ghostty" src/lib_vt.zig "@export(&f, .{ .name = \"ghostty_fixture_real\" });" >/dev/null &&
      rustty_fixture_repo "$2/decoy" src/lib_vt.zig "@export(&f, .{ .name = \"ghostty_decoy_only\" });" >/dev/null' \
      a5_fixture "$ORACLE_LIB" "$st"; then
    echo "SELF-TEST-ORACLE: FAIL (could not build the fixture repositories)"; rm -rf "$st"; exit 1
  fi
  got=$(GIT_DIR="$st/decoy/.git"; GIT_WORK_TREE="$st/decoy"; export GIT_DIR GIT_WORK_TREE
        cd "$st/decoy" && oracle_exports "$st/ghostty") || got="<read failed>"
  rm -rf "$st"
  rc=0
  if [ "$got" = "rustty_fixture_real" ]; then
    echo "  ok   A5 arm (d): the oracle read ignores a GIT_DIR + GIT_WORK_TREE decoy"
  else
    echo "  FAIL A5 arm (d): the oracle read returned '$got', want 'rustty_fixture_real'"
    rc=1
  fi
  if command -v node >/dev/null 2>&1; then
    node .port/tools/check-vt-struct-layouts.mjs --self-test-oracle || rc=1
  else
    echo "  BLOCKED A5 arm (c): node is not installed, so its oracle read cannot be checked"
    [ "$rc" -eq 0 ] && rc=3
  fi
  case "$rc" in
    0) echo "SELF-TEST-ORACLE: PASS" ;;
    3) echo "SELF-TEST-ORACLE: BLOCKED (never a pass)" ;;
    *) echo "SELF-TEST-ORACLE: FAIL" ;;
  esac
  exit "$rc"
fi

# --self-test-masking (P7-W12 T0, R5 negative control for the every-arm-runs
# verdict below): run THIS gate for real with a `cbindgen` shim first on PATH
# that always fails, so arms (a3) and (a3b) fail. Under the old
# straight-line `set -e` the run ended there and printed nothing of (a4), (b),
# (d), (c) or (e). The control passes only if every later arm still printed its
# header, the summary names (a3) as failed, and the gate exits 1.
if [ "${1:-}" = "--self-test-masking" ]; then
  shim=$(mktemp -d "${TMPDIR:-/tmp}/a5_mask.XXXXXX")
  printf '#!/bin/sh\necho "self-test-masking shim: cbindgen always fails" >&2\nexit 1\n' > "$shim/cbindgen"
  chmod +x "$shim/cbindgen"
  grc=0
  out=$(PATH="$shim:$PATH" sh "$0" 2>&1) || grc=$?
  rm -rf "$shim"
  rc=0
  for arm in "(a3b)" "(a4)" "(b)" "(d)" "(c)" "(e)" "(f)"; do
    if printf '%s\n' "$out" | grep -qF "== $arm "; then
      echo "  ok   arm $arm still ran after (a3) failed"
    else
      echo "  FAIL arm $arm did not run: an earlier failure masked it"; rc=1
    fi
  done
  for arm in "(a3)" "(a3b)"; do
    if printf '%s\n' "$out" | grep -qF "  -> arm $arm FAILED"; then
      echo "  ok   arm $arm reported as FAILED"
    else
      echo "  FAIL arm $arm was not reported as FAILED"; rc=1
    fi
  done
  last=$(printf '%s\n' "$out" | tail -1)
  case "$last" in
    "VT-HEADERS: RED (failed arm(s):"*"(a3)"*) echo "  ok   the verdict line is RED and names (a3)" ;;
    *) echo "  FAIL the verdict line is '$last', want a RED line naming (a3)"; rc=1 ;;
  esac
  if [ "$grc" -eq 1 ]; then
    echo "  ok   the gate exits 1"
  else
    echo "  FAIL the gate exited $grc, want 1"; rc=1
  fi
  if [ "$rc" -eq 0 ]; then echo "SELF-TEST-MASKING: PASS"; else echo "SELF-TEST-MASKING: FAIL"; fi
  exit "$rc"
fi

# N486 (P7-W24): the native library and its export list, per host. Arms (b)
# and (d) read the exports, arm (c) links the library. macOS: the `.dylib`,
# whose external defined symbols `nm -gjU` lists with Mach-O's leading `_`.
# Linux and FreeBSD (ELF): the `.so`, whose dynamic symbol table
# `nm -D --defined-only` lists (`<addr> <type> <name>`, no prefix). The same
# set on both: the library's defined, exported symbols, filtered to `rustty_*`
# in arm (b). Any other host is unverified, so arm (b), and arm (d) with it,
# is BLOCKED there (exit 3), never a pass.
# The target dir is cargo's own (CARGO_TARGET_DIR honoured): _target_dir.sh.
# shellcheck source=.port/tools/gates/_target_dir.sh
. "$(dirname "$0")/_target_dir.sh"
TDIR="$(rustty_target_dir "$(cd "$(dirname "$0")/../../.." && pwd)")"
case "$(uname -s)" in
  Darwin) lib="$TDIR/release/librustty_vt_c.dylib" ;;
  Linux|FreeBSD) lib="$TDIR/release/librustty_vt_c.so" ;;
  *) lib="" ;;
esac
# native_exports: the native library's defined, exported symbol names, one per
# line, without Mach-O's `_` prefix.
native_exports() {
  case "$(uname -s)" in
    Darwin) nm -gjU "$lib" | sed 's/^_//' ;;
    *) nm -D --defined-only "$lib" | awk 'NF >= 3 {print $3}' ;;
  esac
}
# P3-W6: UNCONDITIONAL, same reason as the wasm build below. This was
# `[ -f "$lib" ] || cargo build …`, which is the SAME stale-artifact vacuity —
# and it bit immediately: arm (c) reads the struct registry by running
# `rustty_type_json()` OUT OF THIS DYLIB, so with a stale one present the gate
# reported the pre-change registry and its verdict described a library that was
# no longer the source tree. WS19 T16 made exactly this change to all 23
# example/*/build.sh for the same reason; the gates had not been swept.
if ! cargo build -p rustty-vt-c --release >/dev/null 2>&1; then
  echo "A5: FAIL — \`cargo build -p rustty-vt-c --release\` does not build." >&2
  cargo build -p rustty-vt-c --release 2>&1 | tail -40 >&2
  exit 1
fi
# P3-W6 T1: this build is UNCONDITIONAL, and the failure is not swallowed.
#
# It used to be `[ -f …rustty_vt_c.wasm ] || cargo build …>/dev/null 2>&1`, which
# made the whole wasm arm VACUOUS for anyone with a warm target/: the 32-bit
# build had been a hard compile error since main @ 9d8fafb (two `const _`
# layout asserts written as literal `== 16` / `== 8`, false where `usize` is 4
# bytes), and A5 still reported GREEN because the stale `.wasm` satisfied the
# `-f` test. In a clean tree the gate instead died with exit 101 before printing
# its first line — a build failure wearing a gate failure's clothes. Both halves
# of that are fixed here: always build, and say so when it breaks.
if ! cargo build -p rustty-vt-c --target wasm32-unknown-unknown --release >/dev/null 2>&1; then
  echo "A5: FAIL — \`cargo build -p rustty-vt-c --target wasm32-unknown-unknown --release\` does not build." >&2
  echo "A5: arm (b) reads the export list out of that .wasm; without it this gate is vacuous." >&2
  cargo build -p rustty-vt-c --target wasm32-unknown-unknown --release 2>&1 | tail -40 >&2
  exit 1
fi
CC="$(xcrun --find clang 2>/dev/null || echo /usr/bin/clang)"
CXX="$(xcrun --find clang++ 2>/dev/null || echo /usr/bin/clang++)"
T="${TMPDIR:-/tmp}"
# `xcrun --find` returns the raw driver, which does NOT infer the macOS SDK, so a
# header that includes <stdlib.h> (rustty.h:20 does) fails to find it. Pass the SDK
# sysroot explicitly. Empty on non-Darwin, where the system include paths are found
# without help.
SYSROOT=""
sdk="$(xcrun --show-sdk-path 2>/dev/null || true)"
[ -n "$sdk" ] && SYSROOT="-isysroot $sdk"

# P7-W12 T0: EVERY ARM RUNS AND REPORTS. This script used to run its arms
# straight-line under `set -e`, so the first failing arm ended the run and hid
# every arm after it: a red (c) on the phase branch masked (e) entirely, and
# (d) whenever an earlier arm failed. Each arm now runs in its own `set -e`
# subshell; `record` notes its exit status, and the verdict at the end is RED
# (exit 1) if ANY arm failed, else BLOCKED (exit 3) if any arm was blocked,
# else GREEN. The two library builds above stay fatal: every arm from (b) on
# reads their artifacts, so a failed build is not an arm result.
# `--self-test-masking` (below) is the negative control.
set +e
FAILED_ARMS=""
BLOCKED_ARMS=""
record() {
  case "$2" in
    0) ;;
    3) BLOCKED_ARMS="$BLOCKED_ARMS $1"; echo "  -> arm $1 BLOCKED (exit 3); continuing with the remaining arms" ;;
    *) FAILED_ARMS="$FAILED_ARMS $1"; echo "  -> arm $1 FAILED (exit $2); continuing with the remaining arms" ;;
  esac
}

( set -e
echo "== (a) headers compile =="
printf '#include <rustty/vt.h>\nint main(void){return 0;}\n' > "$T/vt_probe.c"
"$CC" -fsyntax-only -Wall -Iinclude "$T/vt_probe.c"
echo "  OK"
)
record "(a)" $?

# WS19 T3 (audit row N2). The (a) probe above compiles only the CURATED
# `rustty/vt.h`. `include/rustty.h` — the FULL cbindgen header we publish — had
# NEVER been compiled by any gate, and did not compile at all: cbindgen emitted
# `Opaque *rustty_wasm_alloc_opaque(void);` against an undeclared `Opaque`
# (cross-crate alias in rustty-lib, invisible with parse_deps=false), so
# `clang -fsyntax-only -I include` failed with 2 errors at :2455 and :2457.
# A header we publish but never compile is an unproven artifact. Every published
# header is now compiled as BOTH C11 and C++17 with -Wall -Wextra.
( set -e
echo "== (a2) EVERY published header compiles standalone as C11 and C++17 =="
for h in rustty.h rustty-apprt.h rustty-embed.h; do
  printf '#include "%s"\nint main(void){return 0;}\n' "$h" > "$T/a2.c"
  "$CC"  -std=c11   -fsyntax-only -Wall -Wextra $SYSROOT -Iinclude "$T/a2.c"
  printf '#include "%s"\nint main(){return 0;}\n' "$h" > "$T/a2.cpp"
  "$CXX" -std=c++17 -fsyntax-only -Wall -Wextra $SYSROOT -Iinclude "$T/a2.cpp"
  echo "  OK  $h (c11 + c++17)"
done
printf '#include <rustty/vt.h>\nint main(){return 0;}\n' > "$T/a2vt.cpp"
"$CXX" -std=c++17 -fsyntax-only -Wall -Wextra $SYSROOT -Iinclude "$T/a2vt.cpp"
echo "  OK  rustty/vt.h (c++17; the (a) probe covers C)"
)
record "(a2)" $?

# --- (a3) the checked-in cbindgen header is not STALE (W12-02) ---------------
# `include/rustty.h` is GENERATED, but nothing regenerated it here, so it drifted
# silently: on main @ d34cc154 the checked file predated the whole D11 pass (the
# `c_int`-narrowed enum parameters/fields) and was missing
# `RUSTTY_TERMINAL_DATA_VT_PROCESSING_ERROR = 33`. Arm (b') catches only the
# staleness class that DROPS a `rustty_*` NAME token; a stale enum variant, a
# stale parameter type or a stale struct field is invisible to a name-token
# intersection. The design contract this gate implements
# (docs/superpowers/specs/2026-07-15-rustty-c-header-family-design.md:54-66) asks
# for a compatible SIGNATURE, not a compatible name list. Regenerate and diff:
# that is the only check that binds every future drift at once.
( set -e
echo "== (a3) include/rustty.h is byte-identical to a fresh cbindgen run =="
if ! command -v cbindgen >/dev/null 2>&1; then
  echo "  BLOCKED: cbindgen is not on PATH, so the generated header cannot be"
  echo "           re-derived and this arm cannot bind. Install it with"
  echo "           \`cargo install cbindgen\`. This is NOT a pass."
  exit 3
fi
# Run from the workspace root — the invocation `xtask header-gen` uses.
if ! cbindgen --config crates/rustty-vt-c/cbindgen.toml \
              --crate rustty-vt-c \
              --output "$T/rustty_regen.h" 2>"$T/rustty_regen.err"; then
  echo "  FAIL — cbindgen could not generate the header:" >&2
  grep -v '^WARN: Skip' "$T/rustty_regen.err" | tail -20 >&2
  exit 1
fi
if ! diff -u include/rustty.h "$T/rustty_regen.h" > "$T/rustty_regen.diff"; then
  echo "  STALE — include/rustty.h does not match what cbindgen generates from"
  echo "  the current rustty-vt-c sources. Re-run \`cargo run -p xtask -- header-gen\`"
  echo "  and commit the result. Do NOT hand-edit the generated header."
  sed -n '1,60p' "$T/rustty_regen.diff" | sed 's/^/    /'
  exit 1
fi
echo "  OK (regenerated and byte-identical)"
)
record "(a3)" $?

# --- (a3b) the OTHER checked-in cbindgen header is not STALE (W92-B02-02) ----
# Exactly the (a3) argument, for `include/rustty-apprt.h`: it is GENERATED but
# nothing regenerated it here either, and it HAD drifted — it predated
# `rustty_info`/`rustty_translate`/`rustty_cli_try_action` (landed 33da59bc) and
# the D11 `c_int` narrowing of `Point.tag`/`Point.coord_tag`/`KeyEvent.action`,
# so the published header declared a surface the crate no longer had. Arm (a2)
# only compiles it; a compile cannot see a MISSING declaration.
( set -e
echo "== (a3b) include/rustty-apprt.h is byte-identical to a fresh cbindgen run =="
if ! cbindgen --config crates/rustty-apprt-c/cbindgen.toml \
              --crate rustty-apprt-c \
              --output "$T/rustty_apprt_regen.h" 2>"$T/rustty_apprt_regen.err"; then
  echo "  FAIL — cbindgen could not generate the header:" >&2
  grep -v '^WARN: ' "$T/rustty_apprt_regen.err" | tail -20 >&2
  exit 1
fi
if ! diff -u include/rustty-apprt.h "$T/rustty_apprt_regen.h" > "$T/rustty_apprt_regen.diff"; then
  echo "  STALE — include/rustty-apprt.h does not match what cbindgen generates"
  echo "  from the current rustty-apprt-c sources. Re-run"
  echo "  \`cargo run -p xtask -- header-gen-apprt\` and commit the result."
  echo "  Do NOT hand-edit the generated header."
  sed -n '1,60p' "$T/rustty_apprt_regen.diff" | sed 's/^/    /'
  exit 1
fi
echo "  OK (regenerated and byte-identical)"
)
record "(a3b)" $?

# --- (a4) the generated header's `Style` pointee is the SIZED STRUCT (W12-02) -
# C has ONE flat type namespace. `rustty-vt-c` exported TWO Rust types named
# `Style` — `size_report::Style` (a 4-byte `#[repr(C)]` enum) and `style::Style`
# (the 72-byte sized `RusttyStyle` struct) — and cbindgen SILENTLY emitted only
# the enum, so the published `include/rustty.h` declared
# `void rustty_style_default(Style *result)` against a 4-byte pointee. A C caller
# writing through that pointer overruns its object by 68 bytes; the callee writes
# the full struct regardless. Nothing above sees it: (a2) compiles the header
# (both spellings compile), (b)/(b') compare NAME tokens, (c)/(e) read the
# CURATED headers, which are correct. The fix renamed the Rust enum to
# `SizeReportStyle`; this arm is the regression test for the whole
# same-name-collision class — it re-derives the pointee type FROM the generated
# header and requires it to be a struct whose size matches the curated
# `RusttyStyle`.
( set -e
echo "== (a4) generated rustty_style_default pointee == curated RusttyStyle =="
cat > "$T/a4_gen.cpp" <<'A4GEN'
#include <cstdio>
#include "rustty.h"
// Recover the first parameter's pointee type straight out of the declaration.
template <class T> struct rustty_param_of;
template <class T> struct rustty_param_of<void (*)(T *)> { using type = T; };
using StyleParam = rustty_param_of<decltype(&rustty_style_default)>::type;
static_assert(!__is_enum(StyleParam),
              "include/rustty.h: rustty_style_default's pointee is an ENUM. Two "
              "Rust types collided on the C name `Style`; rename one.");
static_assert(__is_class(StyleParam) || __is_union(StyleParam),
              "include/rustty.h: rustty_style_default's pointee is not a struct.");
int main(void) { std::printf("%zu\n", sizeof(StyleParam)); return 0; }
A4GEN
cat > "$T/a4_cur.c" <<'A4CUR'
#include <stdio.h>
#include <rustty/vt.h>
int main(void) { printf("%zu\n", sizeof(RusttyStyle)); return 0; }
A4CUR
"$CXX" -std=c++17 -Wall -Wextra $SYSROOT -Iinclude "$T/a4_gen.cpp" -o "$T/a4_gen"
"$CC"  -std=c11   -Wall -Wextra $SYSROOT -Iinclude "$T/a4_cur.c"   -o "$T/a4_cur"
a4_gen_size=$("$T/a4_gen")
a4_cur_size=$("$T/a4_cur")
if [ "$a4_gen_size" != "$a4_cur_size" ]; then
  echo "  MISMATCH — the generated header's rustty_style_default pointee is"
  echo "  $a4_gen_size bytes; the curated RusttyStyle is $a4_cur_size bytes."
  echo "  A caller using include/rustty.h would overrun/underrun the style object."
  exit 1
fi
echo "  OK (pointee is a $a4_gen_size-byte struct, == sizeof(RusttyStyle))"
)
record "(a4)" $?

# --- (a5) rustty.h's rustty_size_report_encode takes the REAL size_report Size -
# P8-W5 T5 (NEW-FINDINGS N529): `rustty_size_report_encode(int, Size, ...)` was
# declared over `mouse_encode::Size` (a different layout) because cbindgen keys
# types by bare name. Arm (f) bars the name collision; this arm pins the ABI
# end to end: a C program built from the generated `include/rustty.h` passes a
# `SizeReportSize` by value to the native library and checks the bytes.
( set -e
echo "== (a5) rustty.h: rustty_size_report_encode(SizeReportSize) encodes the reference vector =="
if [ -z "$lib" ]; then
  echo "  BLOCKED: no native library for $(uname -s). This is NOT a pass."
  exit 3
fi
"$CC" -std=c11 -Wall -Wextra -Werror $SYSROOT -Iinclude test/rustty-h/size_report_abi.c \
  "$lib" -Wl,-rpath,"$PWD/$(dirname "$lib")" -o "$T/a5_size_report"
"$T/a5_size_report"
echo "  OK (rustty.h's SizeReportSize round-trips through the library)"
)
record "(a5)" $?

# Arm (d) reads the export list arm (b) writes: drop any list an earlier run
# left in $T, so (d) never reads a stale one when (b) stops before writing it.
rm -f "$T/vt_exp.txt" "$T/vt_native_exp.txt"
( set -e
echo "== (b) every declared+cbindgen rustty_* symbol is a real export =="
if [ -z "$lib" ]; then
  echo "  BLOCKED: no native-library export reader for $(uname -s) (verified on"
  echo "           macOS and Linux only); arm (d) reads this arm's list. This is NOT a pass."
  exit 3
fi
if [ ! -f "$lib" ]; then
  echo "  FAIL — $lib is missing after \`cargo build -p rustty-vt-c --release\`."
  exit 1
fi
native_exports | grep '^rustty_' | sort -u > "$T/vt_native_exp.txt"
if [ ! -s "$T/vt_native_exp.txt" ]; then
  echo "  FAIL — read 0 rustty_* exports from $lib; an empty native side would make"
  echo "  this arm and arm (d) vacuous (R5)."
  exit 1
fi
grep -rhoE 'rustty_[a-z0-9_]+' include/rustty/vt.h include/rustty/vt | sort -u > "$T/vt_decl.txt"
wasm="$TDIR/wasm32-unknown-unknown/release/rustty_vt_c.wasm"
{ cat "$T/vt_native_exp.txt"; \
  node -e 'const{readFileSync}=require("fs");WebAssembly.compile(readFileSync(process.argv[1])).then(m=>WebAssembly.Module.exports(m).forEach(e=>console.log(e.name)))' "$wasm"; \
} | grep '^rustty_' | sort -u > "$T/vt_exp.txt"
grep -oE 'rustty_[a-z0-9_]+' include/rustty.h | sort -u > "$T/vt_cbindgen.txt"
missing=$(comm -23 "$(comm -12 "$T/vt_decl.txt" "$T/vt_cbindgen.txt" > "$T/vt_both.txt"; echo "$T/vt_both.txt")" "$T/vt_exp.txt")
if [ -n "$missing" ]; then echo "  MISMATCH — declared+cbindgen but NOT exported:"; printf '%s\n' "$missing" | sed 's/^/    /'; exit 1; fi

# --- (b') the vacuous-shrink guard (P3-W0 T2 step 3) ------------------------
# Arm (b) asserts `(vt.h ∩ cbindgen) ⊆ exports`, and an INTERSECTION can be made
# to shrink. Two ways: delete a declaration from a curated header, or let the
# cbindgen header go stale. The second one was LIVE when this guard was written:
# `include/rustty.h` was missing rustty_render_state_begin_update/_end_update
# (WS10 T19 ported the exports but never re-ran `xtask header-gen`), so arm (b)
# quietly cross-checked 183 symbols instead of 185 and reported OK. Regenerating
# the header added exactly those two declarations and nothing else.
#
# So pin BOTH sides of the shrink:
#   * a FLOOR on the intersection's cardinality, and
#   * an EXACT identity for `vt_decl \ intersection` — the tokens the curated
#     headers mention that are deliberately not cbindgen exports.
# Neither can be satisfied by deleting anything.
# P4-W5 (bundles R+M): 185 -> 184. The oracle range REMOVED three exports
# (`terminal_mode_get`/`terminal_mode_set`, cfc19e805; `render_state_colors_get`,
# b4079f00c) and rustty deleted them with it, while ADDING two
# (`render_state_clean` + `render_state_row_iterator_next_dirty`, 0d37f2d34):
# net -1. This is a deliberate, oracle-driven shrink, not a vacuous one — the
# removed names no longer exist upstream, and arm (d) tracks the still-missing
# oracle exports. Re-float upward at W5 T8 as the remaining bundles land.
# P4-W5 Bundle S: +12 snapshot exports, +3 terminal_continuation_*,
# +terminal_vt_write_until_ground, +formatter_format (= +17).
#
# P4-W5 (bundle W, a8e9b413f): the oracle DELETED the 12 per-type
# wasm convenience exports (`wasm_alloc/free_{u8,u8_array,u16_array,usize,
# sgr_attribute}`) for the byte-based API, and ADDED `wasm_alloc`/`wasm_free`/
# `wasm_take_opaque` (+3; the opaque pair survives, already counted); the
# 184-era measurement also predated 2 tokens that only entered the intersection
# via the regen. Oracle-driven, not vacuous: the curated `wasm.h`/`sgr.h` are
# byte-identical to the pin, the regenerated `include/rustty.h` matches the
# crate, and the decl-only identity below still pins the exact non-ABI tokens.
# regen the checked-in `include/rustty.h` was STALE (intersection 174) — the
# exact failure class this arm documents.
# P4-W5 combined re-float (S +17, W net -7): measured 194 on 2026-08-25 with
# every bundle landed; floor pinned to the measurement.
# P5-W6 Bundle P (+1): `rustty_terminal_paste` (oracle 60a1ae2df) — measured
# 195 on 2026-08-25 with the phase-5 C-API bundles landed; floor re-pinned to
# the measurement (arm (d) tracks the oracle side, which also reads 195).
# P6-W3 (+8): `rustty_search_{new,free,tick,feed,run,set,get,get_multi}`
# (oracle `f9202919f`/`76d9fcefe`) — measured 203 on 2026-09-01 with the
# search C API landed; floor re-pinned to the measurement (arm (d) reads 203).
# P8-W9 T1 (+1): the Phase 8 libvt C API (P8-W2 T2, P8-W3 T3, P8-W5 T3) —
# measured 204 at the Phase 8 finale (2026-10-08); floor re-pinned to the
# measurement (the P8-W5 T3 review's deferred minor).
VT_BOTH_FLOOR=204
both_n=$(wc -l < "$T/vt_both.txt" | tr -d ' ')
if [ "$both_n" -lt "$VT_BOTH_FLOOR" ]; then
  echo "  VACUOUS SHRINK — the (vt.h ∩ cbindgen) intersection is $both_n, below the"
  echo "  recorded floor of $VT_BOTH_FLOOR. Either a declaration was deleted from a"
  echo "  curated header, or include/rustty.h is stale (re-run"
  echo "  \`cargo run -p xtask -- header-gen\`). Raising the floor is only correct"
  echo "  when the surface genuinely grew."
  exit 1
fi
# The 6 tokens in the curated headers that are NOT cbindgen exports:
#   rustty_mode_new / rustty_mode_ansi / rustty_mode_value
#     function-like MACROS (include/rustty/vt/modes.h), not symbols.
#   rustty_osc_
#     a prose fragment in a doc comment (include/rustty/vt/osc.h:88).
#   rustty_terminal_continuation_
#     a prose fragment ("the existing rustty_terminal_continuation_* APIs") in
#     snapshot.h's RETAIN_CONTINUATION doc, inherited from the oracle's
#     `ghostty_terminal_continuation_*` wildcard (P4-W5 Bundle S). The three
#     REAL rustty_terminal_continuation_{write,buf,alloc} exports are checked
#     by the intersection and arm (d) as usual.
#   rustty_vt_osc_free
#     a doc TYPO inherited verbatim from upstream's `ghostty_vt_osc_free`
#     (include/rustty/vt/osc.h:99). R1-FAITHFUL — do NOT "fix" it; the port
#     reproduces the oracle's prose, typos included.
cat > "$T/vt_nonabi.txt" <<'NONABI'
rustty_mode_ansi
rustty_mode_new
rustty_mode_value
rustty_osc_
rustty_terminal_continuation_
rustty_vt_osc_free
NONABI
sort -u "$T/vt_nonabi.txt" -o "$T/vt_nonabi.txt"
comm -23 "$T/vt_decl.txt" "$T/vt_both.txt" > "$T/vt_declonly.txt"
if ! diff -q "$T/vt_nonabi.txt" "$T/vt_declonly.txt" >/dev/null; then
  echo "  MISMATCH — the set of curated-header tokens that are NOT cbindgen"
  echo "  exports changed. Expected exactly the 6 documented non-ABI tokens."
  echo "  '<' = expected but gone, '>' = unexpected new token:"
  diff "$T/vt_nonabi.txt" "$T/vt_declonly.txt" | grep '^[<>]' | sed 's/^/    /'
  echo "  A NEW '>' row usually means include/rustty.h is stale: re-run"
  echo "  \`cargo run -p xtask -- header-gen\`. Do not delete a declaration."
  exit 1
fi
echo "  OK ($both_n symbols cross-checked, floor $VT_BOTH_FLOOR; \
$(wc -l < "$T/vt_nonabi.txt" | tr -d ' ') documented non-ABI tokens)"
)
record "(b)" $?

# WS19 T3. Arm (b) is ONE-DIRECTIONAL: it asserts
#   (vt.h ∩ cbindgen) ⊆ exports
# so a symbol missing from the header AND the library is INVISIBLE to it — and it
# is vacuous-shrink-prone (delete a declaration and the intersection shrinks, so
# the gate gets *easier*). That is exactly how `rustty_render_state_begin_update`
# and `rustty_render_state_end_update` hid.
#
# Arm (d) closes it by driving parity from the side that CANNOT go vacuous: the
# ORACLE's own `@export(..., .{ .name = "ghostty_*" })` list in `src/lib_vt.zig`.
# Every oracle export must have its R2-renamed `rustty_*` counterpart in the
# published artifacts (native dylib for portable exports, the wasm module for the
# `#[cfg(target_arch = "wasm32")]` ones). Deleting rustty declarations cannot make
# this arm pass; only porting can.
#
# R3: the oracle is an EXTERNAL read-only reference and never ships to CI, so an
# absent oracle is BLOCKED (exit 3), never a silent green — the same convention
# `check_test_counts.py` uses. The reference is read at `origin/main` rather than
# the pinned working tree because the lib-vt C-API series (BL-5) that defines this
# surface lives BEYOND the pin and WS5 ported it from there; reading the pin would
# under-count by 17 exports and make the arm weaker than the code it checks.
# NEVER `git checkout` in the oracle repo — `git show` only, through
# `oracle_exports` (GIT_* stripped). `origin/main` rather than the pin is
# deliberate (above), so no pin check applies to this arm.
( set -e
echo "== (d) every ORACLE lib_vt.zig export has a rustty_* counterpart =="
# N483: the oracle is `ghostty` beside the main checkout (`rustty_oracle_root`), not a fixed path.
GH="$(bash -c '. "$1" || exit 3; rustty_oracle_root "$2"' oracle_root "$ORACLE_LIB" "$root" || true)"
if [ ! -d "$GH/.git" ]; then
  echo "  BLOCKED: ghostty oracle checkout not found at $GH (R3 — the external"
  echo "           oracle never ships to CI); the bidirectional export-parity arm"
  echo "           cannot bind here. This is NOT a pass."
  exit 3
fi
oracle_exports "$GH" > "$T/a5_oracle.txt"
if [ ! -s "$T/a5_oracle.txt" ]; then
  echo "  BLOCKED: read 0 exports from oracle origin/main:src/lib_vt.zig."
  echo "           An empty reference side would make this arm VACUOUS (R5), so it"
  echo "           is BLOCKED, not green. Fetch the oracle remote and retry."
  exit 3
fi
# The published artifact surface = native exports ∪ wasm exports.
if [ ! -s "$T/vt_exp.txt" ]; then
  echo "  BLOCKED: arm (b) wrote no export list (it stopped before reading the"
  echo "           library), so this arm has no rustty side to compare. NOT a pass."
  exit 3
fi
cp "$T/vt_exp.txt" "$T/a5_have.txt"
# Documented residuals. Each MUST carry an owner and a trigger; the gate FAILS on
# anything not listed here, so this list can only shrink. Do NOT add a row to make
# a red gate green — that is R5. Adding a row requires a ledger entry naming the
# owner (`.port/phase-2/DEFERRALS.md` / `.port/phase-3/README.md`).
#
# P4-W5 Bundle S emptied the snapshot/io/continuation/formatter rows by PORTING
# them (17 exports). The 3 rows below are the LAST residuals: the byte-based
# wasm allocator exports added by a8e9b413f, owned by P4-W5 T6 Bundle W (the
# wasm-allocator break is atomic: new exports + DELETION of the 12 old per-type
# exports + wasm.h rewrite + example/index.html rewrites + ws14 sweep — landing
# the names alone would skew that ABI unit). Trigger: Bundle W lands and MUST
# delete these rows (the stale check below enforces it).
#
#   STRUCK (WS10 T19): rustty_render_state_begin_update / _end_update
#     Found by THIS arm on the day it landed (WS19 T3). The oracle split
#     `RenderState.update` into `beginUpdate` + `endUpdate` beyond the pin
#     (446f80f4e), adding a `pending_styles` field and deferring per-cell style
#     denormalization so a locking caller can shorten its lock hold. That was a
#     FEATURE port, not a signature split. WS10 T19 ported the feature — the
#     field, the two phases, the `page.Mask` group scans and the chunked update —
#     so both exports now exist and the rows are struck rather than inherited by
#     the Phase 3 BL-5 re-vet.
# (P4-W5 Bundle W landed the wasm alloc trio 2026-08-25; the residual list is
# now EMPTY — every oracle lib_vt export has a rustty counterpart.)
cat > "$T/a5_allow.txt" <<'ALLOW'
ALLOW
sort -u "$T/a5_allow.txt" -o "$T/a5_allow.txt"
absent=$(comm -23 "$T/a5_oracle.txt" "$T/a5_have.txt")
unexpected=$(printf '%s\n' "$absent" | grep -v '^$' | comm -23 - "$T/a5_allow.txt" || true)
if [ -n "$unexpected" ]; then
  echo "  MISMATCH — oracle exports with NO rustty_* counterpart, and NOT in the"
  echo "  documented-residual list (see the comment above this arm):"
  printf '%s\n' "$unexpected" | sed 's/^/    /'
  echo "  Port them, or add a row to the residual list WITH an owner and a trigger"
  echo "  in the ledger. Do not delete a declaration to make this pass — this arm"
  echo "  is driven by the ORACLE, so shrinking rustty cannot satisfy it."
  exit 1
fi
# Anti-vacuity: a stale allowlist entry (a residual that has since been ported)
# must be REMOVED, otherwise the list rots into permanent permission.
stale=$(comm -13 "$(printf '%s\n' "$absent" | grep -v '^$' | sort -u > "$T/a5_absent.txt"; echo "$T/a5_absent.txt")" "$T/a5_allow.txt")
if [ -n "$stale" ]; then
  echo "  STALE RESIDUAL LIST — these are listed as missing but ARE now exported;"
  echo "  delete them from the list in this script (a rotting allowlist is how a"
  echo "  gate stops binding):"
  printf '%s\n' "$stale" | sed 's/^/    /'
  exit 1
fi
echo "  OK ($(wc -l < "$T/a5_oracle.txt" | tr -d ' ') oracle exports checked; \
$(wc -l < "$T/a5_allow.txt" | tr -d ' ') documented residual(s), owner: P4-W5 T6 Bundle W)"
)
record "(d)" $?

( set -e
echo "== (c) struct layouts match rustty_type_json() =="
RUSTTY_TARGET_DIR="$TDIR" node .port/tools/check-vt-struct-layouts.mjs
)
record "(c)" $?

# P3-W0 T2 step 5. The missing analogue of upstream's `checkGhosttyHEnum`
# (`src/lib/enum.zig`): nothing in this port compared a Rust enum's
# discriminants to the header's enumerator values. Arm (b) compares symbol
# NAMES; arm (c) compares struct LAYOUTS; neither looks at an enum's values, so
# a header/Rust skew is a silent mis-dispatch rather than a compile error.
# NEW-FINDINGS N27: the arm also checks that each positional pair NAMES the
# same thing (a transposition of implicit-value enumerators keeps every value
# assert green); its self-test proves that on a fixture first.
( set -e
echo "== (e) enum discriminants and names match the curated headers =="
node .port/tools/check-vt-enum-discriminants.mjs --self-test
node .port/tools/check-vt-enum-discriminants.mjs
)
record "(e)" $?

# P8-W5 T3 fix 1 (NEW-FINDINGS N529). cbindgen keys types by bare name, so two
# rustty-vt-c types with one name make include/rustty.h declare one layout for
# both; (a3) regenerates that wrong header faithfully and (c) reads only the
# curated headers. This arm fails on any such collision not in the checker's
# baseline (and on a stale baseline row). Negative controls: its --self-test,
# and reintroducing the OSC `UnknownC` beside the SGR one turns it RED.
( set -e
echo "== (f) no two rustty-vt-c types share a name cbindgen emits =="
python3 .port/tools/check-vt-cbindgen-name-collisions.py --self-test
python3 .port/tools/check-vt-cbindgen-name-collisions.py
)
record "(f)" $?

if [ -n "$FAILED_ARMS" ]; then
  echo "VT-HEADERS: RED (failed arm(s):$FAILED_ARMS${BLOCKED_ARMS:+; blocked arm(s):$BLOCKED_ARMS})"
  exit 1
fi
if [ -n "$BLOCKED_ARMS" ]; then
  echo "VT-HEADERS: BLOCKED (arm(s):$BLOCKED_ARMS; never a pass)"
  exit 3
fi
echo "VT-HEADERS: GREEN"
