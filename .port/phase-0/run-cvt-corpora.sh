#!/usr/bin/env bash
# Phase-0 golden corpora generator for the c-vt-* / cpp-vt-* example consumers.
#
# Each example is a self-contained program: it has hardcoded ("representative")
# VT input baked into its source (no stdin, no argv), and writes deterministic
# output to stdout (a few also write to stderr). We capture stdout+stderr
# COMBINED into <name>.golden (this is what the differential harness will diff,
# byte-for-byte) and ALSO keep stdout/stderr separately plus the exit code, so a
# later reviewer can see exactly where each byte came from.
#
# Build the binaries first with .port/phase-0/build-examples.sh (the supported
# method, and why the examples' own `zig build` is not used: see that script).
#
# Paths (N525): the repository root is derived from this script's location, so
# a worktree reads its own examples-out/ (git-ignored) and writes its own
# corpora/cvt/; nothing outside this checkout is written.
#
# Reproduce a single golden (from the repository root):
#   .port/phase-0/examples-out/<dir>/zig-out/bin/<exe> \
#     > .port/phase-0/corpora/cvt/<name>.golden 2>&1
#
# c-vt-paste (N395): the reference draws a fresh OSC 5522 clipboard-read
# password (`pw=<base64>`) on every run, so its output never repeats byte for
# byte. The comparisons below mask exactly that token (the same `sed` as
# `cvt_normalize` in .port/tools/gates/run_differential.sh, G4 leg B), and a
# capture that equals the stored file under the mask keeps the stored bytes,
# so a re-run does not churn the committed corpus.
#
# NOTE: this script only RUNS the prebuilt binaries; it never touches the
# ghostty reference tree.

set -u

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EX="$ROOT/.port/phase-0/examples-out"
OUT="$ROOT/.port/phase-0/corpora/cvt"
mkdir -p "$OUT"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/run-cvt-corpora.XXXXXX")" || exit 3
trap 'rm -rf "$TMP"' EXIT

# The N395 mask (run_differential.sh `cvt_normalize`), file -> stdout.
mask() { LC_ALL=C sed -E 's#(pw=)[A-Za-z0-9+/]+=*#\1<PW>#g' "$1"; }
# Move a fresh capture into place unless the stored file equals it under the mask.
store() { # store <fresh> <stored>
  if [ -f "$2" ] && cmp -s <(mask "$1") <(mask "$2"); then
    rm -f "$1"
  else
    mv "$1" "$2"
  fi
}

# dir:exe pairs (exe basename under <dir>/zig-out/bin/)
PAIRS=(
  "c-vt:c_vt"
  "c-vt-build-info:c_vt_build_info"
  "c-vt-colors:c_vt_colors"
  "c-vt-effects:c_vt_effects"
  "c-vt-encode-focus:c_vt_encode_focus"
  "c-vt-encode-key:c_vt_encode_key"
  "c-vt-encode-mouse:c_vt_encode_mouse"
  "c-vt-formatter:c_vt_formatter"
  "c-vt-grid-ref-tracked:c_vt_grid_ref_tracked"
  "c-vt-grid-traverse:c_vt_grid_traverse"
  "c-vt-kitty-graphics:c_vt_kitty_graphics"
  "c-vt-modes:c_vt_modes"
  "c-vt-paste:c_vt_paste"
  "c-vt-render:c_vt_render"
  "c-vt-selection:c_vt_selection"
  "c-vt-selection-gesture:c_vt_selection_gesture"
  "c-vt-sgr:c_vt_sgr"
  "c-vt-size-report:c_vt_size_report"
  "c-vt-static:c_vt_static"
  "c-vt-stream:c_vt_stream"
  "cpp-vt-stream:cpp_vt_stream"
  "c-vt-search:c_vt_search"
  "c-vt-snapshot:c_vt_snapshot"
)

printf '%-26s %6s %8s %8s %8s\n' "name" "exit" "comb_B" "out_B" "err_B"
printf '%-26s %6s %8s %8s %8s\n' "----" "----" "------" "-----" "-----"

fail=0
for p in "${PAIRS[@]}"; do
  dir="${p%%:*}"
  exe="${p##*:}"
  bin="$EX/$dir/zig-out/bin/$exe"
  name="$dir"
  if [ ! -x "$bin" ]; then
    printf '%-26s %6s %8s %8s %8s  MISSING:%s\n' "$name" "NA" "-" "-" "-" "$bin"
    fail=1
    continue
  fi
  # Combined golden (stdout+stderr, in arrival order) — the diff target.
  "$bin" > "$TMP/$name.golden" 2>&1
  ec=$?
  # Separate streams for provenance.
  "$bin" > "$TMP/$name.stdout" 2>"$TMP/$name.stderr"
  ec2=$?
  for ext in golden stdout stderr; do
    store "$TMP/$name.$ext" "$OUT/$name.$ext"
  done
  comb=$(wc -c < "$OUT/$name.golden" | tr -d ' ')
  ob=$(wc -c < "$OUT/$name.stdout" | tr -d ' ')
  eb=$(wc -c < "$OUT/$name.stderr" | tr -d ' ')
  echo "$ec" > "$OUT/$name.exit"
  printf '%-26s %6s %8s %8s %8s\n' "$name" "$ec" "$comb" "$ob" "$eb"
  if [ "$ec" != "$ec2" ]; then
    echo "  WARN: non-deterministic exit code ($ec vs $ec2) for $name" >&2
    fail=1
  fi
done

# Determinism check: run again, compare combined goldens via sha256 (N395 mask applied).
echo ""
echo "=== determinism re-run (sha256 of masked combined golden, run #2 vs stored) ==="
for p in "${PAIRS[@]}"; do
  dir="${p%%:*}"; exe="${p##*:}"
  bin="$EX/$dir/zig-out/bin/$exe"
  [ -x "$bin" ] || continue
  "$bin" > "$TMP/rerun" 2>&1
  h1=$(mask "$OUT/$dir.golden" | shasum -a 256 | awk '{print $1}')
  h2=$(mask "$TMP/rerun" | shasum -a 256 | awk '{print $1}')
  if [ "$h1" == "$h2" ]; then st="OK"; else st="MISMATCH"; fail=1; fi
  printf '%-26s %s %s\n' "$dir" "$st" "$h1"
done

exit $fail
