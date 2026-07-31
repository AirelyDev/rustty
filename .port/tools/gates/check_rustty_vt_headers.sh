#!/bin/sh
# ABI-fidelity gate (A5) for the published rustty C header family.
# Spec: docs/superpowers/specs/2026-07-15-rustty-c-header-family-design.md
set -e
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
lib=target/release/librustty_vt_c.dylib
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

echo "== (a) headers compile =="
printf '#include <rustty/vt.h>\nint main(void){return 0;}\n' > "$T/vt_probe.c"
"$CC" -fsyntax-only -Wall -Iinclude "$T/vt_probe.c"
echo "  OK"

# WS19 T3 (audit row N2). The (a) probe above compiles only the CURATED
# `rustty/vt.h`. `include/rustty.h` — the FULL cbindgen header we publish — had
# NEVER been compiled by any gate, and did not compile at all: cbindgen emitted
# `Opaque *rustty_wasm_alloc_opaque(void);` against an undeclared `Opaque`
# (cross-crate alias in rustty-lib, invisible with parse_deps=false), so
# `clang -fsyntax-only -I include` failed with 2 errors at :2455 and :2457.
# A header we publish but never compile is an unproven artifact. Every published
# header is now compiled as BOTH C11 and C++17 with -Wall -Wextra.
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

echo "== (b) every declared+cbindgen rustty_* symbol is a real export =="
grep -rhoE 'rustty_[a-z0-9_]+' include/rustty/vt.h include/rustty/vt | sort -u > "$T/vt_decl.txt"
wasm=target/wasm32-unknown-unknown/release/rustty_vt_c.wasm
{ nm -gjU "$lib" | sed 's/^_//' | grep '^rustty_'; \
  node -e 'const{readFileSync}=require("fs");WebAssembly.compile(readFileSync(process.argv[1])).then(m=>WebAssembly.Module.exports(m).forEach(e=>console.log(e.name)))' "$wasm"; \
} | grep '^rustty_' | sort -u > "$T/vt_exp.txt"
grep -oE 'rustty_[a-z0-9_]+' include/rustty.h | sort -u > "$T/vt_cbindgen.txt"
missing=$(comm -23 "$(comm -12 "$T/vt_decl.txt" "$T/vt_cbindgen.txt" > "$T/vt_both.txt"; echo "$T/vt_both.txt")" "$T/vt_exp.txt")
if [ -n "$missing" ]; then echo "  MISMATCH — declared+cbindgen but NOT exported:"; echo "$missing" | sed 's/^/    /'; exit 1; fi

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
VT_BOTH_FLOOR=185
both_n=$(wc -l < "$T/vt_both.txt" | tr -d ' ')
if [ "$both_n" -lt "$VT_BOTH_FLOOR" ]; then
  echo "  VACUOUS SHRINK — the (vt.h ∩ cbindgen) intersection is $both_n, below the"
  echo "  recorded floor of $VT_BOTH_FLOOR. Either a declaration was deleted from a"
  echo "  curated header, or include/rustty.h is stale (re-run"
  echo "  \`cargo run -p xtask -- header-gen\`). Raising the floor is only correct"
  echo "  when the surface genuinely grew."
  exit 1
fi
# The 5 tokens in the curated headers that are NOT cbindgen exports:
#   rustty_mode_new / rustty_mode_ansi / rustty_mode_value
#     function-like MACROS (include/rustty/vt/modes.h), not symbols.
#   rustty_osc_
#     a prose fragment in a doc comment (include/rustty/vt/osc.h:88).
#   rustty_vt_osc_free
#     a doc TYPO inherited verbatim from upstream's `ghostty_vt_osc_free`
#     (include/rustty/vt/osc.h:99). R1-FAITHFUL — do NOT "fix" it; the port
#     reproduces the oracle's prose, typos included.
cat > "$T/vt_nonabi.txt" <<'NONABI'
rustty_mode_ansi
rustty_mode_new
rustty_mode_value
rustty_osc_
rustty_vt_osc_free
NONABI
sort -u "$T/vt_nonabi.txt" -o "$T/vt_nonabi.txt"
comm -23 "$T/vt_decl.txt" "$T/vt_both.txt" > "$T/vt_declonly.txt"
if ! diff -q "$T/vt_nonabi.txt" "$T/vt_declonly.txt" >/dev/null; then
  echo "  MISMATCH — the set of curated-header tokens that are NOT cbindgen"
  echo "  exports changed. Expected exactly the 5 documented non-ABI tokens."
  echo "  '<' = expected but gone, '>' = unexpected new token:"
  diff "$T/vt_nonabi.txt" "$T/vt_declonly.txt" | grep '^[<>]' | sed 's/^/    /'
  echo "  A NEW '>' row usually means include/rustty.h is stale: re-run"
  echo "  \`cargo run -p xtask -- header-gen\`. Do not delete a declaration."
  exit 1
fi
echo "  OK ($both_n symbols cross-checked, floor $VT_BOTH_FLOOR; \
$(wc -l < "$T/vt_nonabi.txt" | tr -d ' ') documented non-ABI tokens)"

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
# NEVER `git checkout` in the oracle repo — `git show` only.
echo "== (d) every ORACLE lib_vt.zig export has a rustty_* counterpart =="
GH="/Users/gael/projects/ghostty"
if [ ! -d "$GH/.git" ]; then
  echo "  BLOCKED: ghostty oracle checkout not found at $GH (R3 — the external"
  echo "           oracle never ships to CI); the bidirectional export-parity arm"
  echo "           cannot bind here. This is NOT a pass."
  exit 3
fi
git -C "$GH" show origin/main:src/lib_vt.zig 2>/dev/null \
  | grep -oE '\.name = "ghostty_[a-z0-9_]+"' \
  | sed 's/.*"ghostty_/rustty_/; s/"$//' | sort -u > "$T/a5_oracle.txt"
if [ ! -s "$T/a5_oracle.txt" ]; then
  echo "  BLOCKED: read 0 exports from oracle origin/main:src/lib_vt.zig."
  echo "           An empty reference side would make this arm VACUOUS (R5), so it"
  echo "           is BLOCKED, not green. Fetch the oracle remote and retry."
  exit 3
fi
# The published artifact surface = native exports ∪ wasm exports.
cp "$T/vt_exp.txt" "$T/a5_have.txt"
# Documented residuals. Each MUST carry an owner and a trigger; the gate FAILS on
# anything not listed here, so this list can only shrink. Do NOT add a row to make
# a red gate green — that is R5. Adding a row requires a ledger entry naming the
# owner (`.port/phase-2/DEFERRALS.md` / `.port/phase-3/README.md`).
#
# The list is currently EMPTY, and that is the point: every oracle export has a
# rustty_* counterpart.
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
$(wc -l < "$T/a5_allow.txt" | tr -d ' ') documented residual(s), owner: Phase 3 BL-5 re-vet P3-RV5)"

echo "== (c) struct layouts match rustty_type_json() =="
node .port/tools/check-vt-struct-layouts.mjs

# P3-W0 T2 step 5. The missing analogue of upstream's `checkGhosttyHEnum`
# (`src/lib/enum.zig`): nothing in this port compared a Rust enum's
# discriminants to the header's enumerator values. Arm (b) compares symbol
# NAMES; arm (c) compares struct LAYOUTS; neither looks at an enum's values, so
# a header/Rust skew is a silent mis-dispatch rather than a compile error.
echo "== (e) enum discriminants match the curated headers =="
node .port/tools/check-vt-enum-discriminants.mjs

echo "VT-HEADERS: GREEN"
