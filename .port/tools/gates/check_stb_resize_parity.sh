#!/usr/bin/env bash
# check_stb_resize_parity.sh — GATE D10: the vendored `stb_image_resize` glyph
# resampler must produce the same bytes as ghostty's OWN build of the same header.
#
# WHAT IT KILLS. `crates/rustty-font-shaper/src/face/freetype.rs` scales
# bitmap-strike glyphs with `stbir_resize_uint8`, C vendored byte-identically from
# the oracle (`src/stb/stb_image_resize.h`, MD5 7381a7070120fd5a36d8fb9e3179dd7c at
# the pin AND at origin/main). Identical C is NOT identical output: the resampler
# sums floats, so a build-flag or codegen change moves the last bit. Before WS11 the
# port had NO check on that at all, and — worse — every in-crate test that reached
# the call did so at scale 1.0, where any resampler agrees (substituting a
# nearest-neighbour loop left all 9 `face::freetype` tests green). D10 closes the
# build-variance half; the in-crate KATs
# (`face::freetype::tests::stb_resize_kat_*`) close the pipeline half.
#
# HOW. `.port/tools/stb-resize-oracle/run.sh`:
#   ARM 1  our vendored header via host `cc` at -O0/-O1/-O2/-O3/-Os. -O2 is the
#          reference; the other levels must be within the codegen envelope below. When
#          a `--expect` fixture is given it must be reproduced EXACTLY by all five.
#   ARM 2  ghostty's own recipe (`src/build/SharedDeps.zig`: -I src/stb + stb.c,
#          NO extra flags on non-MSVC), read with `git show` — NEVER a checkout,
#          several lanes depend on that tree staying pinned — built at all four Zig
#          optimize modes and diffed against the reference. Debug/ReleaseSafe/
#          ReleaseSmall must be EXACT; ReleaseFast gets the envelope.
# then `.port/tools/stb-resize-oracle/negative_control.py`, which proves the
# tolerance envelope still REJECTS a wrong resampler.
#
# THE ENVELOPE, AND WHY IT IS NOT A FUDGE FACTOR. It is `max delta 1 on <= 3 % of
# bytes`, and both numbers come from measurement, not from taste.
#
# The delta bound of 1 is the whole observed spread of the +/-1 codegen axis. The
# cause is identified, not guessed: whether the compiler vectorizes the resampler's
# float summation loop. `zig cc -O2 -fno-vectorize` restores the reference EXACTLY;
# `zig cc -O2`/`-O3` are byte-identical to ghostty's ReleaseFast; `-fno-slp-vectorize`
# alone and `-ffp-contract=off` do NOT restore it. The same +/-1 appears (a) between
# ghostty's own Debug and ReleaseFast builds of the identical header — so "bit-exact
# with the oracle" has no unique referent; (b) between aarch64 and x86_64 builds of
# the same compiler at the same -O; and (c) between Fedora gcc 16 -O0/-O1 and its own
# -O2/-O3/-Os. It is a property of the resampler's float summation order, not a port
# defect. See the permanent-divergence entry in `.port/phase-2/DEFERRALS.md`.
#
# The 3 % fraction bound is the smallest round number above the largest observed rate.
# Measured: 9/512 = 1.76 % (ghostty ReleaseFast, and Fedora gcc -O0 vs -O2, 16x32) and
# 3/136 = 2.21 % (Fedora gcc -O0 vs -O2, 8x17 — a small buffer, so one extra differing
# byte moves the rate a lot). A hard 2.00 % bound would have made this gate
# permanently and falsely RED on any Linux box whose `cc` is gcc, i.e. CI.
#
# The envelope stays ~18-20x away from a wrong resampler in BOTH dimensions:
#   stb vs nearest  501/512 (98 %) maxdelta 89  |  124/136 (91 %) maxdelta 192
#   stb vs bilinear 374/512 (73 %) maxdelta 28  |   75/136 (55 %) maxdelta 14
# `negative_control.py` asserts that machine-checkably against the SAME bound this
# gate enforces (and its own `--self-test` proves its RED arm is reachable), so the
# gate cannot degrade into "any resampler passes" the way the pre-WS11 tests had.
#
# WHY NO FLAGS ARE PINNED (locked decision, WS11 Task 3): the only construction that
# would match ReleaseFast is compiling the C with `zig cc -O3`, i.e. making the Zig
# toolchain a hard build dependency of the Rust port (against R3). Adding
# `-ffp-contract=off`/`-ffast-math` to `build.rs` would CREATE divergence, and since
# the seam is live on macOS too (`cfg(not(target_arch = "wasm32"))`, not
# `not(target_os = "macos")`) that would be an R5 violation. rustty's `cc` build
# already sits on the consensus side on every platform we ship.
#
# EXIT CONTRACT (`.port/tools/gates/README.md` — never a silent green)
#   0  GREEN     consensus internally exact, ghostty modes within contract,
#                negative control discriminating
#   3  BLOCKED   `zig` or the oracle repo absent, so the ghostty-recipe arm could
#                not run (the consensus arm and the negative control still ran and
#                can still turn it RED)
#   1  RED       anything else
#
# Registered in ci.sh as PROGRESS, not HARD, matching the rest of the D-series: it
# reports BLOCKED on any box without `zig` or the oracle checkout. R5 is NOT
# weakened — run_gate's "PROGRESS went RED" branch escalates a genuine mismatch to a
# HARD failure.
#
# USAGE
#   check_stb_resize_parity.sh              # the gate
#   check_stb_resize_parity.sh --self-test  # prove the RED flip is reachable:
#                                           # rebuild our header with -ffast-math
#                                           # and require the gate to go RED

set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
HARNESS="$RUSTTY_ROOT/.port/tools/stb-resize-oracle"

if [[ "${1:-}" == "--self-test" ]]; then
  echo "=== D10 --self-test: the detector must go RED on a perturbed build ==="
  echo
  echo "--- (a) run.sh with -ffast-math on the consensus build (must be RED)"
  bash "$HARNESS/run.sh" --cflags "-ffast-math"
  rc_a=$?
  echo "    run.sh --cflags -ffast-math exit=$rc_a"
  echo
  echo "--- (b) negative_control.py --self-test: its RED arm must be reachable"
  python3 "$HARNESS/negative_control.py" --self-test
  rc_b=$?
  echo "    negative_control.py --self-test exit=$rc_b"
  echo
  if [[ "$rc_a" == "1" && "$rc_b" == "0" ]]; then
    echo "D10 --self-test: PASS — both RED arms are reachable"
    exit 0
  fi
  echo "D10 --self-test: FAIL — expected run.sh(-ffast-math)=1 and negative_control(--self-test)=0,"
  echo "                 got $rc_a and $rc_b. The detector may be vacuous."
  exit 1
fi

echo "=== D10 stb_image_resize parity: vendored C vs ghostty's own build recipe ==="
bash "$HARNESS/run.sh"
rc_run=$?
echo
python3 "$HARNESS/negative_control.py"
rc_nc=$?
echo

if [[ "$rc_nc" != "0" ]]; then
  echo "D10: RED — the negative control failed: the tolerance envelope no longer"
  echo "     discriminates a wrong resampler (or the consensus could not be built)."
  exit 1
fi

case "$rc_run" in
  0) echo "D10: GREEN — fixtures exact at every opt level; ghostty Debug/ReleaseSafe/"
     echo "     ReleaseSmall byte-exact; ReleaseFast within the measured +/-1-on-3 %"
     echo "     envelope; negative control discriminating."
     exit 0 ;;
  3) echo "D10: BLOCKED — the ghostty-recipe arm could not run (no zig, or no oracle repo"
     echo "     holding the pin commit)."
     echo "     The consensus arm and the negative control ran GREEN. Never a silent green."
     exit 3 ;;
  *) echo "D10: RED — see run.sh output above."
     exit 1 ;;
esac
