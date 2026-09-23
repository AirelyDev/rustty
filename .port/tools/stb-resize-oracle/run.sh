#!/usr/bin/env bash
# run.sh — WS11 stb_image_resize toolchain matrix.
#
# WHAT THIS MEASURES. `crates/rustty-font-shaper/vendor/stb_image_resize.h` is
# byte-identical to the oracle's `src/stb/stb_image_resize.h`, but identical C does
# not imply identical bytes out: the resampler sums floats, so codegen differences
# move the last bit. This script quantifies that, in two arms:
#
#   ARM 1 — CONSENSUS. Build OUR vendored header standalone with the host `cc` at
#   -O0 -O1 -O2 -O3 -Os. The -O2 output is the reference; the others must be within
#   the same measured +/-1-on-<=3% codegen envelope as every other axis here (see the
#   long comment at the check itself -- Fedora gcc -O0/-O1 form a third variant, so
#   demanding byte-identity across opt levels would make this gate permanently RED on
#   a gcc host and would assert something the C does not guarantee). A --expect
#   fixture, however, must be reproduced EXACTLY by ALL FIVE levels -- that is the
#   load-bearing assertion, and it holds on macOS Apple clang and Fedora gcc+clang.
#
#   ARM 2 — GHOSTTY'S OWN RECIPE. Read the oracle's `src/stb/{stb.c,stb_image.h,
#   stb_image_resize.h}` AT THE PIN with `git show "$PIN:..."` (NEVER a checkout —
#   several lanes depend on that tree staying pinned; and never `HEAD:`, so an
#   oracle on another commit cannot change what is read) and compile stb.c exactly the way
#   `src/build/SharedDeps.zig` does (`addIncludePath("src/stb")` +
#   `addCSourceFiles(.{ .files = &.{"src/stb/stb.c"} })` with NO extra flags on
#   non-MSVC), at all four Zig optimize modes, then diff each against the consensus.
#
# THE ASSERTION. Debug/ReleaseSafe/ReleaseSmall must be EXACTLY equal to the
# reference. ReleaseFast is allowed max delta 1 on <= 3% of bytes — both bounds are
# measured, not chosen; the full basis is in the envelope discussion in
# .port/tools/gates/check_stb_resize_parity.sh (cause: the loop vectorizer;
# `zig cc -O2 -fno-vectorize` restores the reference exactly, `zig cc -O2`/`-O3` are
# byte-identical to the ReleaseFast recipe). `negative_control.py` proves the envelope
# still rejects a wrong resampler against the SAME bound: nearest neighbour and
# bilinear move 55-98% of bytes at max delta 14-192.
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
#   run.sh --print-oracle        # print the oracle repo arm 2 reads, and exit
#   run.sh --print-oracle-reads  # do arm 2's oracle read, print each file's cksum,
#                                # and exit (0 read / 3 BLOCKED); for the self-test
#
# EXIT CONTRACT (matches .port/tools/gates/README.md — never a silent green)
#   0  GREEN     every assertion held
#   1  RED       a genuine mismatch
#   3  BLOCKED   `zig` absent, or the oracle repo absent or without the pin commit,
#                so arm 2 could not run
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
# The oracle repository for arm 2: `ghostty` beside the MAIN checkout, resolved
# by the shared shell resolver (the twin of `_gatelib.oracle_root`). D10 runs
# this script, so R5 applies: the former `GHOSTTY_ORACLE` override is gone, and
# the oracle's git reads go through `rustty_oracle_git` (GIT_* stripped) so a
# GIT_DIR decoy cannot answer `git show "$PIN:src/stb/..."` either.
# `--print-oracle` prints the result and exits; G-ORACLE-SH-SELF
# (`_oracle_root.sh --self-test`) runs a copy of this script that way with the
# old override and a GIT_DIR decoy set.
# shellcheck source=.port/tools/gates/_oracle_root.sh
source "$ROOT/.port/tools/gates/_oracle_root.sh" || { echo "BLOCKED: cannot source _oracle_root.sh"; exit 3; }
ORACLE_REPO="$(rustty_oracle_root "$ROOT")"
if [[ "${1:-}" == "--print-oracle" ]]; then
  echo "$ORACLE_REPO"
  exit 0
fi
# Arm 2 reads the oracle's files AT THE PIN (`git show "$PIN:src/stb/..."`), not
# at whatever the oracle's HEAD happens to be. The pin is the one checked-in
# `ORACLE_PIN` (empty when that file is unreadable -> BLOCKED).
ORACLE_PIN="$(rustty_oracle_pin)" || ORACLE_PIN=""

# read_oracle_stb DEST — the ONLY place arm 2 reads the oracle. Copies
# src/stb/{stb.c,stb_image.h,stb_image_resize.h} at the pin into DEST through
# `rustty_oracle_git` (GIT_* stripped). Prints why and returns 3 when it cannot.
read_oracle_stb() {
  local dest="$1" f
  if [[ -z "$ORACLE_PIN" ]]; then
    echo "the oracle pin file is missing or malformed"; return 3
  fi
  if ! rustty_oracle_git -C "$ORACLE_REPO" rev-parse --verify -q "$ORACLE_PIN^{commit}" >/dev/null 2>&1; then
    echo "oracle repo $ORACLE_REPO absent, or it does not have the pin ${ORACLE_PIN:0:9}"; return 3
  fi
  mkdir -p "$dest" || return 3
  for f in stb.c stb_image.h stb_image_resize.h; do
    rustty_oracle_git -C "$ORACLE_REPO" show "$ORACLE_PIN:src/stb/$f" > "$dest/$f" 2>/dev/null \
      || { echo "git show ${ORACLE_PIN:0:9}:src/stb/$f failed"; return 3; }
  done
  return 0
}

# --print-oracle-reads: perform arm 2's oracle read and print what was read
# (`<file> <cksum> <bytes>`), then exit — 0 when read, 3 when BLOCKED.
# G-ORACLE-SH-SELF runs a copy this way under a GIT_DIR decoy, to prove the
# read itself (not only the location) ignores GIT_*.
if [[ "${1:-}" == "--print-oracle-reads" ]]; then
  _reads="$(mktemp -d)" || exit 3
  if ! _why="$(read_oracle_stb "$_reads")"; then
    echo "BLOCKED: $_why"; rm -rf "$_reads"; exit 3
  fi
  for _f in stb.c stb_image.h stb_image_resize.h; do
    printf '%s %s\n' "$_f" "$(cksum < "$_reads/$_f" | tr -s ' \t' ' ')"
  done
  rm -rf "$_reads"
  exit 0
fi

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

  for o in "${OPTS[@]}"; do
    if ! run_case "$WORK/cons$o" "$WORK/$label$o.bin" "$ow" "$oh" "$in" "$iw" "$ih" "$c"; then
      echo "  RED: consensus harness $o crashed on case $label"; RED=1; continue
    fi
  done
  CONS="$WORK/$label-O2.bin"
  # The reference is the host cc at -O2. The other opt levels are held to the SAME
  # measured envelope as every other codegen axis (max delta 1 on <= 3 % of bytes),
  # for the SAME identified reason: whether the compiler vectorizes the resampler's
  # float summation loop. This is not a fudge factor and it is not a relaxation of
  # anything -- it is a measured property of the C, demonstrated on two compiler
  # families and two operating systems:
  #   Apple clang aarch64 -O0/-O1/-O2/-O3/-Os  ] all byte-identical: the consensus
  #   Fedora clang -O0/-O1/-O2/-O3/-Os         ]  (16x32 md5 5f18d21d..., 8x17 0e2fb9f0...)
  #   Fedora gcc 16 -O2/-O3/-Os                ]
  #   Fedora gcc 16 -O0/-O1  -> a THIRD variant (16x32 md5 770a168d..., 8x17 4dc5c919...),
  #                             9/512 and 3/136 bytes off the consensus at max delta 1
  # Requiring all five levels to be byte-identical would therefore make this gate
  # permanently RED on any Linux box where `cc` is gcc -- i.e. CI -- while telling us
  # nothing we did not already know about the +/-1 axis.
  # WHAT STAYS EXACT, and is the load-bearing assertion: a --expect fixture must be
  # reproduced byte-for-byte by EVERY opt level (see below). That is strictly stronger
  # than the -O2-only comparison this replaced, and it holds on both platforms today.
  for o in "${OPTS[@]}"; do
    bindiff "$CONS" "$WORK/$label$o.bin"
    [[ "$o" == "-O2" ]] && continue
    if [[ "$DIFF_N" == "0" ]]; then
      printf '  host cc %-4s vs host cc -O2: %s [EXACT]\n' "$o" "$DIFF_TEXT"
    elif [[ "$DIFF_MAX" -le 1 ]] && python3 -c "import sys; sys.exit(0 if $DIFF_N <= 0.03*$DIFF_TOTAL else 1)"; then
      printf '  host cc %-4s vs host cc -O2: %s [within the +/-1-on-3%% codegen envelope]\n' "$o" "$DIFF_TEXT"
    else
      echo "  RED: host cc $o vs host cc -O2: $DIFF_TEXT (OUTSIDE the +/-1-on-3% envelope)"
      RED=1
    fi
  done
  echo "  reference (host cc -O2): md5 $(md5sum < "$CONS" 2>/dev/null | cut -d' ' -f1 || md5 -q "$CONS")"

  # fixture equality, if asked -- required EXACT at EVERY opt level
  if [[ -n "$EXPECT" ]]; then
    for o in "${OPTS[@]}"; do
      bindiff "$WORK/$label$o.bin" "$EXPECT"
      if [[ "$DIFF_N" == "0" ]]; then
        printf '  fixture %s vs host cc %-4s: %s [EXACT]\n' "$(basename "$EXPECT")" "$o" "$DIFF_TEXT"
      else
        echo "  RED: fixture $(basename "$EXPECT") vs host cc $o: $DIFF_TEXT"
        RED=1
      fi
    done
  fi

  # ------------------------------------------------- arm 2: ghostty's own recipe
  if [[ -z "${ZIG_OK+x}" ]]; then
    if ! command -v zig >/dev/null 2>&1; then
      echo "  BLOCKED: zig not installed — ghostty-recipe arm skipped"
      BLOCKED=1; ZIG_OK=0
    elif ! why="$(read_oracle_stb "$WORK/oracle")"; then
      echo "  BLOCKED: $why — ghostty-recipe arm skipped"
      BLOCKED=1; ZIG_OK=0
    else
      ZIG_OK=1
      echo "  oracle: $ORACLE_REPO @ ${ORACLE_PIN:0:9} (the pin) ; zig $(zig version)"
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
        # measured envelope: max delta 1 on <= 3% of bytes -- the bound is set from
        # the measured rate of the +/-1 codegen axis, which is ~2 % of bytes but is
# NOT bounded by exactly 2.00 %: measured 9/512 = 1.76 % (zig ReleaseFast and Fedora
# gcc -O0 vs -O2, 16x32 case) and 3/136 = 2.21 % (Fedora gcc -O0 vs -O2, 8x17 case).
# 3 % is the smallest round bound above the measured maximum. The discrimination margin
# is unaffected and remains ~20x: a wrong resampler moves 55-98 % of bytes (see
# negative_control.py, which validates THIS bound, and the in-crate KATs).
        if [[ "$DIFF_MAX" -le 1 ]] && python3 -c "import sys; sys.exit(0 if $DIFF_N <= 0.03*$DIFF_TOTAL else 1)"; then
          verdict="[within envelope: maxdelta<=1 on ${pct}% <= 3%]"
        else
          verdict="[RED: OUTSIDE the +/-1-on-3% envelope]"; RED=1
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
