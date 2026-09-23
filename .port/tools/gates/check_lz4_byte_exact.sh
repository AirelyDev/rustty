#!/usr/bin/env bash
# G-LZ4 — the LZ4 encoder BYTE STREAM matches the oracle's, not merely round-trips.
#
# WHY THIS EXISTS
# ---------------
# `compress::lz4_differential::lz4_differential_light` is a real, load-bearing
# guard: assessment B injected seven faults and it caught four (`LAST_LITERALS`
# 5->4, `MATCH_FIND_LIMIT` 12->11, and both decoder fast-path margins). But it is
# structurally blind to the match-finder's *heuristics*, because every fault in
# that class preserves round-trip identity AND format validity. Measured: with
# `HASH_LOG` 12 -> 11 the whole rustty suite stays green — reproduced here at
# P3-W0 T1 step 1, `cargo test -p rustty-vt --lib --release compress::` 19/19
# pass and `cargo test -p rustty-bench --lib --release` 18/18 pass — while
# 1,631 of this gate's 7,976 differential points diverge from the oracle's byte
# stream.
#
# Upstream mandates exactly this proof and rustty does not inherit the mandate
# automatically: `src/terminal/compress/AGENTS.md` — "When a change should not
# alter compressor output, prove it: compare encoded sizes (or a sequence-count
# fingerprint) on the same corpus before and after. Ratio drift is a functional
# change, not noise." That file is ported to
# `crates/rustty-vt/src/compress/AGENTS.md`; this script is its realization.
#
# CONTRACT
# --------
#  1. Extract `lz4.zig` (+ `lz4_differential.zig`, which `lz4.zig` imports in its
#     test block) from the ghostty oracle with `git show origin/main:<path>` into
#     a temp dir. READ-ONLY. This script never runs `git checkout` in the oracle,
#     and never reads the oracle working tree, so it is correct both before and
#     after P3-W10 advances the pin.
#  2. Build the oracle arm: `zig build-exe harness.zig -O ReleaseSafe -lc`. The
#     codec depends only on `std` (upstream's compress/AGENTS.md documents
#     standalone compilation as supported). `-lc` is needed only because Zig
#     0.16 removed `std.posix.write` and routes `File.writer` through an `Io`
#     instance; the harness writes stdout through `extern "c" write`.
#  3. Build the rustty arm: `rustc --edition 2021 -O -C debug-assertions=on` over
#     a two-line generated `main.rs` that `#[path]`-includes
#     `crates/rustty-vt/src/compress/lz4.rs` VERBATIM (it has no dependency
#     beyond `core`/`std`) and `include!`s `lz4_differential/harness_body.rs`.
#     Run from the repo root so rustup resolves the pinned toolchain.
#  4. Both harnesses implement the identical deterministic corpus specified by
#     `lz4_differential/CORPUS.md` (8 families x 35 boundary sizes = 280;
#     4,000 randomized triples; every match period 1..96; 60 valid blocks x 60
#     corruptions = 3,600; total 7,976) and print one line per point:
#     `family,size,seed,encoded_len,fnv1a(encoded)` for phases 1-3, and for
#     phase 4 the exact error-variant name or `decoded_len,fnv1a(output buffer)`.
#  5. Compare. Non-empty diff => exit 1, printing the diverging rows and the
#     total count. Oracle checkout absent => exit 3 BLOCKED, never a silent green.
#  6. ANTI-VACUITY: if either arm emits zero rows, or fewer than the expected
#     7,976, FAIL LOUDLY. A gate whose corpus generator silently produced nothing
#     would otherwise report a clean diff.
#
# NEGATIVE CONTROL
# ----------------
#   bash .port/tools/gates/check_lz4_byte_exact.sh --self-test
# patches `HASH_LOG` 12 -> 11 in a COPY of `lz4.rs` in the temp dir (the working
# tree is never touched) and requires the gate to exit non-zero reporting exactly
# $EXPECTED_SELFTEST_ROWS diverging rows out of 7,976. See CORPUS.md
# "Deviation from the plan's expected self-test count" for why this number is
# 1,631 and not the plan's 1,929 (assessment B's harnesses were transient and
# never committed, so there is no artifact to converge on).
#
# The oracle path is hard-coded on purpose. An env override would let the gate
# pass against a fake tree — the same spoof hole `ci.sh:114-129` documents for
# the test-count gate.

set -uo pipefail

ORACLE="/Users/gael/projects/ghostty"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
GATEDIR="$ROOT/.port/tools/gates/lz4_differential"
LZ4_RS="$ROOT/crates/rustty-vt/src/compress/lz4.rs"

EXPECTED_POINTS=7976
EXPECTED_SELFTEST_ROWS=1631

SELF_TEST=0
SELF_TEST_ORACLE=0
case "${1:-}" in
  --self-test) SELF_TEST=1 ;;
  --self-test-oracle) SELF_TEST_ORACLE=1 ;;
  "") ;;
  *) echo "usage: $0 [--self-test | --self-test-oracle]" >&2; exit 2 ;;
esac

say() { echo "[G-LZ4] $*"; }

# The stripped git runner: every GIT_* variable removed, so GIT_DIR cannot
# answer `git -C "$ORACLE" show` from another repository (R5).
# shellcheck source=.port/tools/gates/_oracle_root.sh
source "$ROOT/.port/tools/gates/_oracle_root.sh" || { say "BLOCKED: cannot source _oracle_root.sh"; exit 3; }

# read_oracle_codec ORACLE DEST — the ONLY read of the oracle: lz4.zig and
# lz4_differential.zig at origin/main, into DEST. Returns non-zero (with git's
# error in DEST/<file>.err) when a file cannot be read.
read_oracle_codec() {
  local f
  for f in lz4.zig lz4_differential.zig; do
    rustty_oracle_git -C "$1" show "origin/main:src/terminal/compress/$f" > "$2/$f" 2>"$2/$f.err" \
      || { READ_FAILED="$f"; return 1; }
  done
}

# --self-test-oracle (hermetic; run by G-ORACLE-SH-SELF): a fixture oracle and
# a decoy, each with its own origin/main lz4.zig. With GIT_DIR + GIT_WORK_TREE
# naming the decoy, `read_oracle_codec` on the fixture must return the
# fixture's bytes.
if [ "$SELF_TEST_ORACLE" -eq 1 ]; then
  st="$(mktemp -d "${TMPDIR:-/tmp}/glz4_oracle.XXXXXX")" || exit 1
  ok=1
  for repo in ghostty decoy; do
    if ! rustty_fixture_repo "$st/$repo" src/terminal/compress/lz4.zig "// $repo lz4" >/dev/null; then ok=0; fi
    printf '// %s differential' "$repo" > "$st/$repo/src/terminal/compress/lz4_differential.zig"
    rustty_fixture_git -C "$st/$repo" add -A >/dev/null 2>&1 &&
      rustty_fixture_git -C "$st/$repo" commit -q -m two >/dev/null 2>&1 &&
      rustty_fixture_git -C "$st/$repo" update-ref refs/remotes/origin/main HEAD >/dev/null 2>&1 || ok=0
  done
  mkdir -p "$st/out"
  if [ "$ok" -eq 1 ] && ( export GIT_DIR="$st/decoy/.git" GIT_WORK_TREE="$st/decoy"
                          cd "$st/decoy" && read_oracle_codec "$st/ghostty" "$st/out" ) &&
     [ "$(cat "$st/out/lz4.zig")" = "// ghostty lz4" ] &&
     [ "$(cat "$st/out/lz4_differential.zig")" = "// ghostty differential" ]; then
    rm -rf "$st"
    say "  ok   the oracle read ignores a GIT_DIR + GIT_WORK_TREE decoy"
    say "SELF-TEST-ORACLE: PASS"; exit 0
  fi
  say "  FAIL the oracle read did not return the fixture oracle's files under a GIT_DIR decoy"
  say "       (lz4.zig: '$(cat "$st/out/lz4.zig" 2>/dev/null)')"
  rm -rf "$st"
  say "SELF-TEST-ORACLE: FAIL"; exit 1
fi

# ---- environment ----------------------------------------------------------
if [ ! -d "$ORACLE/.git" ]; then
  say "BLOCKED: ghostty oracle checkout not found at $ORACLE."
  say "         The oracle is an EXTERNAL reference (R3) and never ships to CI."
  say "         Exiting 3 (environment missing) rather than passing vacuously."
  exit 3
fi
for tool in zig rustc git diff; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    say "BLOCKED: '$tool' is not on PATH; cannot build both arms."
    exit 3
  fi
done
if [ ! -f "$LZ4_RS" ]; then
  say "BLOCKED: $LZ4_RS is missing."
  exit 3
fi
for f in harness.zig harness_body.rs CORPUS.md; do
  if [ ! -f "$GATEDIR/$f" ]; then
    say "BLOCKED: $GATEDIR/$f is missing."
    exit 3
  fi
done

TMP="$(mktemp -d "${TMPDIR:-/tmp}/glz4.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/z" "$TMP/r"

# ---- oracle arm -----------------------------------------------------------
# `git show` only; never `git checkout`. Reading origin/main here is deliberate:
# the gate must track upstream's codec, not the pinned working tree, so no pin
# applies. The read goes through `read_oracle_codec` (GIT_* stripped).
READ_FAILED=""
if ! read_oracle_codec "$ORACLE" "$TMP/z"; then
  say "BLOCKED: cannot read origin/main:src/terminal/compress/$READ_FAILED from the oracle."
  sed 's/^/[G-LZ4]   /' "$TMP/z/$READ_FAILED.err" >&2
  exit 3
fi
cp "$GATEDIR/harness.zig" "$TMP/z/harness.zig"
if ! ( cd "$TMP/z" && zig build-exe harness.zig -O ReleaseSafe -lc ) > "$TMP/z/build.log" 2>&1; then
  say "FAIL: the oracle (Zig) arm did not build."
  sed 's/^/[G-LZ4]   /' "$TMP/z/build.log" >&2
  exit 1
fi
if ! "$TMP/z/harness" > "$TMP/zig.out" 2>"$TMP/z/run.log"; then
  say "FAIL: the oracle (Zig) arm did not run to completion."
  sed 's/^/[G-LZ4]   /' "$TMP/z/run.log" >&2
  exit 1
fi

# ---- rustty arm -----------------------------------------------------------
RS_SRC="$LZ4_RS"
if [ "$SELF_TEST" -eq 1 ]; then
  RS_SRC="$TMP/r/lz4.rs"
  sed 's/const HASH_LOG: usize = 12/const HASH_LOG: usize = 11/' "$LZ4_RS" > "$RS_SRC"
  if ! grep -q 'const HASH_LOG: usize = 11' "$RS_SRC"; then
    say "FAIL(self-test): could not patch HASH_LOG in the temp copy — the"
    say "                 declaration in lz4.rs changed shape. The negative"
    say "                 control is not running; refusing to report a result."
    exit 1
  fi
  say "self-test: HASH_LOG patched 12 -> 11 in $RS_SRC (working tree untouched)"
fi

printf '#[path = "%s"]\nmod lz4;\ninclude!("%s");\n' "$RS_SRC" "$GATEDIR/harness_body.rs" > "$TMP/r/main.rs"
# cd to the repo root so rustup picks up rust-toolchain.toml's pinned channel.
if ! ( cd "$ROOT" && rustc --edition 2021 -O -C debug-assertions=on -A warnings \
        -o "$TMP/r/harness" "$TMP/r/main.rs" ) > "$TMP/r/build.log" 2>&1; then
  say "FAIL: the rustty (Rust) arm did not build."
  sed 's/^/[G-LZ4]   /' "$TMP/r/build.log" >&2
  exit 1
fi
if ! "$TMP/r/harness" > "$TMP/rust.out" 2>"$TMP/r/run.log"; then
  say "FAIL: the rustty (Rust) arm did not run to completion."
  sed 's/^/[G-LZ4]   /' "$TMP/r/run.log" >&2
  exit 1
fi

# ---- anti-vacuity ---------------------------------------------------------
zig_rows=$(wc -l < "$TMP/zig.out" | tr -d ' ')
rust_rows=$(wc -l < "$TMP/rust.out" | tr -d ' ')
if [ "$zig_rows" -ne "$EXPECTED_POINTS" ] || [ "$rust_rows" -ne "$EXPECTED_POINTS" ]; then
  say "FAIL (VACUOUS): expected $EXPECTED_POINTS corpus points from each arm;"
  say "                got oracle=$zig_rows rustty=$rust_rows."
  say "                A gate whose generator emits nothing reports a clean diff."
  exit 1
fi

# ---- compare --------------------------------------------------------------
diff "$TMP/zig.out" "$TMP/rust.out" > "$TMP/diff.txt"
diverging=$(grep -c '^<' "$TMP/diff.txt" 2>/dev/null || true)
diverging=${diverging:-0}

if [ "$SELF_TEST" -eq 1 ]; then
  say "self-test: $diverging / $EXPECTED_POINTS points diverge"
  if [ "$diverging" -ne "$EXPECTED_SELFTEST_ROWS" ]; then
    say "FAIL(self-test): expected exactly $EXPECTED_SELFTEST_ROWS diverging rows."
    say "                 A different count means the corpus generator changed and"
    say "                 the gate's sensitivity is now unknown. Fix the generator"
    say "                 (or, if the change was deliberate, re-derive and re-pin"
    say "                 this number together with CORPUS.md's sensitivity table)."
    exit 1
  fi
  say "PASS(self-test): the FAIL arm is reachable and its reach is pinned."
  say "                 rc must be non-zero -> exiting 1 as the negative control."
  exit 1
fi

if [ "$diverging" -ne 0 ]; then
  say "FAIL: the rustty LZ4 byte stream differs from the oracle's on"
  say "      $diverging of $EXPECTED_POINTS points. Diverging rows (oracle '<',"
  say "      rustty '>'), first 40:"
  grep '^[<>]' "$TMP/diff.txt" | head -40 | sed 's/^/[G-LZ4]   /'
  say "      Row key: phase,family|block,size|corruption,seed|outbuf,len,fnv1a."
  say "      See .port/tools/gates/lz4_differential/CORPUS.md."
  exit 1
fi

say "OK: encoder byte stream and decoder behaviour identical to the oracle on"
say "    all $EXPECTED_POINTS differential points (oracle origin/main:src/terminal/compress/lz4.zig)."
exit 0
