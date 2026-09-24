#!/usr/bin/env bash
# GATE G-FONTBACKEND — the `-Dfont-backend` build option, built and tested per
# backend.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `build-macos-freetype` job
# (pin 20abdb50a, lines 1383-1428):
#
#   zig build test -Drenderer=metal -Dfont-backend=coretext_freetype -Dtest-filter=""
#   zig build -Demit-macos-app=false -Drenderer=metal -Dfont-backend=coretext_freetype
#
# i.e. the non-default macOS backend is TESTED and the whole tree BUILT with it.
# rustty's `-Dfont-backend` is the `rustty-build-config/font-backend-*` feature
# set (N266, S-14). The workspace lanes only ever build the per-OS default, so
# without this lane a non-default arm (a `Face`/`Shaper`/`Library`/`Discover`
# selector, `loadCoreTextFreetype`, the CoreText face's `harfbuzz_shaper` arms,
# the noop shaper, the "no discovery" arms) could rot unseen.
#
# Beyond the reference's one job, every OTHER backend the port builds on macOS
# is tested too (`coretext_harfbuzz`, `coretext_noshape`, `freetype`): they are
# as selectable as `coretext_freetype`, and a selectable arm nobody compiles is
# the fold this gate exists to prevent.
#
# Per backend: `cargo test -p rustty-font -p rustty-font-shaper -p
# rustty-renderer` with the feature, under `RUSTTY_EXPECT_FONT_BACKEND=<tag>`,
# which `rustty-font-shaper`'s `selectors_follow_options_backend` test asserts —
# so a feature that did not reach the build is RED, not a silent re-run of the
# default. `coretext_freetype` also builds the whole workspace (the reference's
# "Build All"). Arm 0 proves that assertion can fail: the expectation
# `coretext_freetype` against a DEFAULT build must be RED.
#
# macOS only (the reference's job is macOS-only, and the CoreText backends
# cannot link elsewhere): BLOCKED (exit 3) on any other host, never green.
# Linux/Windows arms (`freetype` there) are type-checked by S-14's cross
# checks, not by this lane — see `.port/parity-closure/NEW-FINDINGS.md` N266.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

command -v cargo >/dev/null 2>&1 || { echo "G-FONTBACKEND BLOCKED: cargo not installed"; exit 3; }
if [ "$(uname -s)" != "Darwin" ]; then
  echo "G-FONTBACKEND BLOCKED: macOS-only lane (the reference's build-macos-freetype job; CoreText backends)"
  exit 3
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

FAILED=0
FAILED_NAMES=""
ok()  { echo "  ok      $1"; }
bad() { echo "  FAIL    $1"; FAILED=1; FAILED_NAMES="$FAILED_NAMES [$1]"; }

# run LABEL ENV_EXPECT cargo-args...
run() {
  local label="$1" expect="$2"; shift 2
  if RUSTTY_EXPECT_FONT_BACKEND="$expect" cargo "$@" > "$WORK/out.log" 2>&1; then
    grep -E '^test result' "$WORK/out.log" | sed 's/^/          /'
    ok "$label"
  else
    grep -E '^test result|FAILED|panicked at|^error' "$WORK/out.log" | head -20 | sed 's/^/          /'
    bad "$label"
  fi
}

echo "=== G-FONTBACKEND 0 :: the expectation check can fail ==="
if RUSTTY_EXPECT_FONT_BACKEND=coretext_freetype \
  cargo test -p rustty-font-shaper --lib -- backend_selection_tests > "$WORK/neg.log" 2>&1; then
  bad "arm 0: a default build passed RUSTTY_EXPECT_FONT_BACKEND=coretext_freetype (the lane check is vacuous)"
else
  if grep -q "the lane's -Dfont-backend reached the build" "$WORK/neg.log"; then
    ok "arm 0: default build vs expectation coretext_freetype is RED, for the stated reason"
  else
    tail -20 "$WORK/neg.log" | sed 's/^/          /'
    bad "arm 0: RED, but not for the expectation mismatch"
  fi
fi

# (feature suffix, @tagName)
BACKENDS=(
  "coretext-freetype|coretext_freetype"
  "coretext-harfbuzz|coretext_harfbuzz"
  "coretext-noshape|coretext_noshape"
  "freetype|freetype"
)

i=0
for entry in "${BACKENDS[@]}"; do
  i=$((i+1))
  IFS='|' read -r feat tag <<< "$entry"
  echo "=== G-FONTBACKEND $i :: -Dfont-backend=$tag ==="
  run "test font, font-shaper, renderer ($tag)" "$tag" \
    test -p rustty-font -p rustty-font-shaper -p rustty-renderer --no-fail-fast \
    --features "rustty-build-config/font-backend-$feat"
  if [ "$tag" = "coretext_freetype" ]; then
    run "build --workspace ($tag)" "$tag" \
      build --workspace --features "rustty-build-config/font-backend-$feat"
  fi
done

if [ "$FAILED" -ne 0 ]; then
  echo "G-FONTBACKEND RED:$FAILED_NAMES"
  exit 1
fi
echo "G-FONTBACKEND GREEN"
exit 0
