#!/usr/bin/env bash
# run.sh — WS11 stb_image_resize toolchain matrix.
#
# WHAT THIS MEASURES. `crates/rustty-font-shaper/vendor/stb_image_resize.h` is
# byte-identical to the oracle's `src/stb/stb_image_resize.h`, but identical C does
# not imply identical bytes out: the resampler sums floats, so codegen differences
# move the last bit. This script quantifies that, in two arms:
#
#   ARM 1 — CONSENSUS. Build OUR vendored header standalone with the host `cc` at
#   -O0 -O1 -O2 -O3 -Os and require all five outputs to be byte-identical. That
#   common value is the "consensus": the number the in-crate KAT fixtures encode,
#   and the number Fedora gcc/clang independently reproduce.
#
#   ARM 2 — GHOSTTY'S OWN RECIPE. Read the oracle's `src/stb/{stb.c,stb_image.h,
#   stb_image_resize.h}` with `git show` (NEVER a checkout — several lanes depend on
#   that tree staying pinned) and compile stb.c exactly the way
#   `src/build/SharedDeps.zig` does (`addIncludePath("src/stb")` +
#   `addCSourceFiles(.{ .files = &.{"src/stb/stb.c"} })` with NO extra flags on
#   non-MSVC), at all four Zig optimize modes, then diff each against the consensus.
#
# THE ASSERTION. Debug/ReleaseSafe/ReleaseSmall must be EXACTLY equal to the
# consensus. ReleaseFast is allowed max delta 1 on <= 2% of bytes — that envelope is
# not a fudge factor, it is a measured property of ReleaseFast's loop vectorizer
# (`zig cc -O2 -fno-vectorize` restores the consensus exactly; `zig cc -O3` is
# byte-identical to the ReleaseFast recipe). `negative_control.py` proves the
# envelope still rejects a wrong resampler: nearest neighbour and bilinear move
# 70-98% of bytes at max delta 14-192, a ~14x margin in delta.
#
# MODES
#   run.sh                       # the two built-in ramp cases: 16x32 and 8x17
#   run.sh --case IN IW IH C OW OH [--expect FIXTURE]
#                                # replay an arbitrary raw input through the whole
#                                # matrix; with --expect, require every consensus
#                                # toolchain AND ghostty Debug/ReleaseSafe/
#                                # ReleaseSmall to reproduce FIXTURE exactly.
#                                # Used to validate the in-crate KAT fixtures.
#   run.sh --cflags "-ffast-math"  # perturb the consensus build (gate --self-test)
#   run.sh ... --expect-scale 255 # multiply the harness output by N before comparing
#                                # to --expect. Models the pipeline's post-resize
#                                # num_grays normalisation: a MONO bitmap strike is
#                                # densified to num_grays=2 (values 0/1) BEFORE the
#                                # resize, and freetype.rs multiplies by
#                                # `factor = 255/(num_grays-1)` AFTER it. So the
#                                # in-crate fixture is 255x the raw resize output.
#
# EXIT CONTRACT (matches .port/tools/gates/README.md — never a silent green)
#   0  GREEN     every assertion held
#   1  RED       a genuine mismatch
#   3  BLOCKED   `zig` or the oracle repo absent, so arm 2 could not run
#                (arm 1 still runs and can still turn the result RED)
#
# GOTCHAS baked in, do not remove:
#   * `zig build-obj -ODebug` needs -fno-sanitize-c or the link fails with
#     `undefined symbol: ___ubsan_handle_type_mismatch_v1`. `-fno-sanitize=undefined`
#     is NOT a valid `zig build-obj` flag. It gates integer/pointer UB traps only,
#     never FP codegen, so it cannot perturb the measurement.
#   * Linking stb_image_resize on Linux needs -lm (`pow`, from the sRGB paths).
#   * `cc` may be shell-aliased; this script always uses an absolute/`command -v`
#     compiler path.

set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../.." && pwd)"
VENDOR="$ROOT/crates/rustty-font-shaper/vendor"
ORACLE_REPO="${GHOSTTY_ORACLE:-/Users/gael/projects/ghostty}"

CC="${CC:-}"
if [[ -z "$CC" ]]; then
  if [[ -x /usr/bin/cc ]]; then CC=/usr/bin/cc; else CC="$(command -v cc || command -v gcc || command -v clang)"; fi
fi
LIBM=()
[[ "$(uname -s)" != "Darwin" ]] && LIBM=(-lm)

EXTRA_CFLAGS=()
CASE_IN="" CASE_IW=0 CASE_IH=0 CASE_C=1 CASE_OW=0 CASE_OH=0 EXPECT="" EXPECT_SCALE=1
while [[ $# -gt 0 ]]; do
  case "$1" in
    --cflags) IFS=' ' read -r -a EXTRA_CFLAGS <<< "$2"; shift 2 ;;
    --case) CASE_IN="$2" CASE_IW="$3" CASE_IH="$4" CASE_C="$5" CASE_OW="$6" CASE_OH="$7"; shift 7 ;;
    --expect) EXPECT="$2"; shift 2 ;;
    --expect-scale) EXPECT_SCALE="$2"; shift 2 ;;
    -h|--help) sed -n '2,60p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "run.sh: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

RED=0
BLOCKED=0

# bindiff A B  ->  prints "<n>/<total> differ, maxdelta=<d>"; sets DIFF_N/DIFF_MAX
bindiff() {
  local out
  out="$(python3 "$DIR/bindiff.py" "$1" "$2")" || return 1
  DIFF_N="${out%% *}"; DIFF_N="${DIFF_N%%/*}"
  DIFF_TOTAL="$(python3 -c "print('$out'.split()[0].split('/')[1])")"
  DIFF_MAX="${out##*maxdelta=}"
  DIFF_TEXT="$out"
}

# ---------------------------------------------------------------- arm 1: consensus
echo "host cc: $($CC --version 2>&1 | head -1)"
if [[ ${#EXTRA_CFLAGS[@]} -gt 0 ]]; then
  echo "consensus build PERTURBED with: ${EXTRA_CFLAGS[*]}"
fi

# case list: "label out_w out_h in_file in_w in_h channels"
CASES=()
if [[ -n "$CASE_IN" ]]; then
  CASES+=("${CASE_OW}x${CASE_OH} $CASE_OW $CASE_OH $CASE_IN $CASE_IW $CASE_IH $CASE_C")
else
  CASES+=("16x32 16 32 - 8 16 1")
  CASES+=("8x17 8 17 - 8 16 1")
fi

OPTS=(-O0 -O1 -O2 -O3 -Os)
for o in "${OPTS[@]}"; do
  if ! "$CC" "$o" "${EXTRA_CFLAGS[@]+"${EXTRA_CFLAGS[@]}"}" -I"$VENDOR" -o "$WORK/cons$o" \
        "$DIR/oracle.c" "$VENDOR/stb_impl.c" "${LIBM[@]+"${LIBM[@]}"}" 2>"$WORK/cc.log"; then
    echo "RED: host cc $o failed to build the consensus harness"; sed -n '1,10p' "$WORK/cc.log"
    exit 1
  fi
done

run_case() { # $1=binary $2=out $3..= case fields (ow oh in iw ih c)
  local bin="$1" out="$2" ow="$3" oh="$4" in="$5" iw="$6" ih="$7" c="$8"
  if [[ "$in" == "-" ]]; then
    "$bin" "$ow" "$oh" > "$out"
  else
    "$bin" "$ow" "$oh" "$in" "$iw" "$ih" "$c" > "$out"
  fi
  local rc=$?
  # Model the pipeline's post-resize num_grays normalisation (see --expect-scale).
  if [[ "$EXPECT_SCALE" != "1" ]]; then
    python3 -c "import sys;p=sys.argv[1];n=int(sys.argv[2]);d=open(p,'rb').read();open(p,'wb').write(bytes(min(255,b*n) for b in d))" "$out" "$EXPECT_SCALE"
  fi
  return $rc
}

for spec in "${CASES[@]}"; do
  read -r label ow oh in iw ih c <<< "$spec"
  echo "=== case $label (in ${iw}x${ih}, ${c}ch)"

  # all five opt levels must agree
  for o in "${OPTS[@]}"; do
    if ! run_case "$WORK/cons$o" "$WORK/$label$o.bin" "$ow" "$oh" "$in" "$iw" "$ih" "$c"; then
      echo "  RED: consensus harness $o crashed on case $label"; RED=1; continue
    fi
  done
  CONS="$WORK/$label-O2.bin"
  for o in "${OPTS[@]}"; do
    bindiff "$CONS" "$WORK/$label$o.bin"
    if [[ "$DIFF_N" != "0" ]]; then
      echo "  RED: host cc $o vs host cc -O2: $DIFF_TEXT (the five opt levels must agree)"
      RED=1
    fi
  done
  echo "  consensus (host cc -O0/-O1/-O2/-O3/-Os, all identical): md5 $(md5sum < "$CONS" 2>/dev/null | cut -d' ' -f1 || md5 -q "$CONS")"

  # fixture equality, if asked
  if [[ -n "$EXPECT" ]]; then
    bindiff "$CONS" "$EXPECT"
    if [[ "$DIFF_N" == "0" ]]; then
      echo "  fixture $(basename "$EXPECT") vs consensus: $DIFF_TEXT  [EXACT]"
    else
      echo "  RED: fixture $(basename "$EXPECT") vs consensus: $DIFF_TEXT"
      RED=1
    fi
  fi

  # ------------------------------------------------- arm 2: ghostty's own recipe
  if [[ -z "${ZIG_OK+x}" ]]; then
    if ! command -v zig >/dev/null 2>&1; then
      echo "  BLOCKED: zig not installed — ghostty-recipe arm skipped"
      BLOCKED=1; ZIG_OK=0
    elif ! git -C "$ORACLE_REPO" rev-parse HEAD >/dev/null 2>&1; then
      echo "  BLOCKED: oracle repo $ORACLE_REPO absent — ghostty-recipe arm skipped"
      BLOCKED=1; ZIG_OK=0
    else
      ZIG_OK=1
      ORACLE_REV="$(git -C "$ORACLE_REPO" rev-parse --short HEAD)"
      mkdir -p "$WORK/oracle"
      for f in stb.c stb_image.h stb_image_resize.h; do
        git -C "$ORACLE_REPO" show "HEAD:src/stb/$f" > "$WORK/oracle/$f" || { echo "  BLOCKED: git show src/stb/$f failed"; BLOCKED=1; ZIG_OK=0; }
      done
      echo "  oracle: $ORACLE_REPO @ $ORACLE_REV ; zig $(zig version)"
      if [[ "$ZIG_OK" == "1" ]]; then
        for MODE in Debug ReleaseFast ReleaseSafe ReleaseSmall; do
          # EXACTLY ghostty's recipe: -I<src/stb> + stb.c, no extra flags.
          # -fno-sanitize-c only defeats the UB-trap runtime (link-only), never FP codegen.
          if ! (cd "$WORK/oracle" && zig build-obj "-O$MODE" -lc -I. -fno-sanitize-c \
                  -femit-bin="$WORK/oracle/stb-$MODE.o" stb.c) >"$WORK/zig.log" 2>&1; then
            echo "  RED: zig build-obj -O$MODE failed"; sed -n '1,10p' "$WORK/zig.log"; RED=1; continue
          fi
          if ! "$CC" -O2 -o "$WORK/oracle_$MODE" "$DIR/oracle.c" "$WORK/oracle/stb-$MODE.o" \
                "${LIBM[@]+"${LIBM[@]}"}" 2>"$WORK/link.log"; then
            echo "  RED: linking ghostty-recipe $MODE failed"; sed -n '1,10p' "$WORK/link.log"; RED=1; continue
          fi
        done
      fi
    fi
  fi

  if [[ "${ZIG_OK:-0}" == "1" ]]; then
    for MODE in Debug ReleaseFast ReleaseSafe ReleaseSmall; do
      [[ -x "$WORK/oracle_$MODE" ]] || continue
      run_case "$WORK/oracle_$MODE" "$WORK/$label-g-$MODE.bin" "$ow" "$oh" "$in" "$iw" "$ih" "$c"
      bindiff "$CONS" "$WORK/$label-g-$MODE.bin"
      pct="$(python3 -c "print(f'{100.0*$DIFF_N/$DIFF_TOTAL:.2f}')")"
      verdict=""
      if [[ "$MODE" == "ReleaseFast" ]]; then
        # measured envelope: max delta 1 on <= 2% of bytes
        if [[ "$DIFF_MAX" -le 1 ]] && python3 -c "import sys; sys.exit(0 if $DIFF_N <= 0.02*$DIFF_TOTAL else 1)"; then
          verdict="[within envelope: maxdelta<=1 on ${pct}% <= 2%]"
        else
          verdict="[RED: OUTSIDE the +/-1-on-2% envelope]"; RED=1
        fi
      else
        if [[ "$DIFF_N" == "0" ]]; then
          verdict="[EXACT]"
        else
          verdict="[RED: $MODE must be byte-exact with the consensus]"; RED=1
        fi
      fi
      printf '  ghostty-recipe %-12s vs consensus: %s %s\n' "$MODE" "$DIFF_TEXT" "$verdict"
      if [[ -n "$EXPECT" && "$MODE" != "ReleaseFast" ]]; then
        bindiff "$EXPECT" "$WORK/$label-g-$MODE.bin"
        if [[ "$DIFF_N" != "0" ]]; then
          echo "    RED: fixture vs ghostty-recipe $MODE: $DIFF_TEXT"; RED=1
        fi
      fi
    done
  fi
done

if [[ "$RED" != "0" ]]; then echo "run.sh: RED"; exit 1; fi
if [[ "$BLOCKED" != "0" ]]; then echo "run.sh: GREEN on the consensus arm; BLOCKED on the ghostty-recipe arm"; exit 3; fi
echo "run.sh: GREEN"
exit 0
