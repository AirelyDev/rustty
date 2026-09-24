#!/usr/bin/env bash
# GATE G-FONTBACKEND — the `-Dfont-backend` build option, built and tested per
# backend.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `build-macos-freetype` job
# (at the oracle pin, `ORACLE_PIN`; lines 1383-1428):
#
#   zig build test -Drenderer=metal -Dfont-backend=coretext_freetype -Dtest-filter=""
#   zig build -Demit-macos-app=false -Drenderer=metal -Dfont-backend=coretext_freetype
#
# i.e. the non-default macOS backend is TESTED and the whole tree BUILT with it.
# rustty's `-Dfont-backend` is the `rustty-build-config/font-backend-*` feature
# set (N266, S-14). The workspace lanes only ever build the per-OS default, so
# without this lane a non-default arm (a `Face`/`Shaper`/`Library`/`Discover`
# selector, `loadCoreTextFreetype`, the CoreText face's `harfbuzz_shaper` arms,
# the noop shaper, the "no discovery" arms, the Fontconfig and Windows
# discoverers on macOS) could rot unseen.
#
# Beyond the reference's one job, every OTHER backend the port builds on macOS
# is tested too: they are as selectable as `coretext_freetype` (Ghostty accepts
# any backend on any target, `Config.zig:191-195`), and a selectable arm nobody
# compiles is the fold this gate exists to prevent.
#
# Arms:
#   L  (any host) lint: no `Cargo.toml` may name a `font-backend-*` feature
#      outside its `[features]` table. One dependent that enables
#      `rustty-build-config/font-backend-X` in a `[dependencies]` entry would
#      silently flip the backend for EVERY crate in its build (feature
#      unification); only `[features]` pass-throughs are allowed. The arm
#      self-tests first on synthetic manifests (a clean one, and one with the
#      feature in `[dependencies]` / `[target.*.dependencies]`).
#   0  a DEFAULT build tested against `RUSTTY_EXPECT_FONT_BACKEND=coretext_freetype`
#      must be RED for the stated reason (the per-backend check can fail).
#   1+ per backend: `cargo clippy -p rustty-font-shaper --all-targets -D
#      warnings`, then `cargo test -p rustty-font -p rustty-font-shaper -p
#      rustty-renderer` with the feature, under
#      `RUSTTY_EXPECT_FONT_BACKEND=<tag>` (asserted by
#      `selectors_follow_options_backend`, so a feature that did not reach the
#      build is RED, not a silent re-run of the default). `coretext_freetype` and
#      `freetype` also test the crates above the font tier with backend-keyed
#      code (`rustty-core`, `rustty-cli`, `rustty-apprt-c`), and
#      `coretext_freetype` builds the whole workspace (the reference's "Build
#      All").
#
# Fixtures, because the reference's discoverers read the host:
#   * `fontconfig_freetype`: the vendored fontconfig is configured exactly as
#     the reference's (`FONTCONFIG_PATH=/etc/fonts`), which macOS lacks, so the
#     lane points `FONTCONFIG_FILE` at a generated fonts.conf listing the macOS
#     font directories.
#   * `freetype_windows`: the scanner reads `%SYSTEMROOT%\Fonts` and opens
#     `<dir>\<name>` — backslashes, as in the reference. The lane builds a
#     `SYSTEMROOT` whose `root\Fonts` directory (and `root\Fonts\<name>` files)
#     hold Arial, Arial Unicode and Noto Sans Chakma copied from the system.
#
# macOS only (the reference's job is macOS-only, and the CoreText backends
# cannot link elsewhere): after arm L, BLOCKED (exit 3) on any other host, never
# green. Linux/Windows `freetype` arms are type-checked by S-14's cross checks,
# not by this lane — see `.port/parity-closure/NEW-FINDINGS.md` N266.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

FAILED=0
FAILED_NAMES=""
ok()  { echo "  ok      $1"; }
bad() { echo "  FAIL    $1"; FAILED=1; FAILED_NAMES="$FAILED_NAMES [$1]"; }

# lint_manifests ROOT...: print every Cargo.toml under the roots that names
# `font-backend-` outside its `[features]` table; exit 1 if any.
lint_manifests() {
  python3 - "$@" <<'PY'
import pathlib, sys, tomllib
bad = []
def walk(node, path):
    if isinstance(node, dict):
        for k, v in node.items():
            if path == () and k == "features":
                continue
            if "font-backend-" in str(k):
                bad.append((path + (k,)))
            walk(v, path + (k,))
    elif isinstance(node, list):
        for v in node:
            walk(v, path)
    elif isinstance(node, str) and "font-backend-" in node:
        bad.append(path)
for root in sys.argv[1:]:
    for p in sorted(pathlib.Path(root).rglob("Cargo.toml")):
        if "target" in p.parts or ".git" in p.parts:
            continue
        before = len(bad)
        walk(tomllib.loads(p.read_text()), ())
        for b in bad[before:]:
            print(f"{p}: `font-backend-*` outside [features] at {'.'.join(b)}")
sys.exit(1 if bad else 0)
PY
}

echo "=== G-FONTBACKEND L :: font-backend-* only in [features] ==="
mkdir -p "$WORK/lint/good" "$WORK/lint/bad1" "$WORK/lint/bad2"
printf '[package]\nname="a"\n[features]\nfont-backend-freetype = ["rustty-build-config/font-backend-freetype"]\n' \
  > "$WORK/lint/good/Cargo.toml"
printf '[package]\nname="b"\n[dependencies]\nrustty-build-config = { path = "x", features = ["font-backend-coretext-freetype"] }\n' \
  > "$WORK/lint/bad1/Cargo.toml"
printf '[package]\nname="c"\n[target.%s.dependencies]\nrustty-build-config = { path = "x", features = ["font-backend-freetype"] }\n' \
  "'cfg(unix)'" > "$WORK/lint/bad2/Cargo.toml"
if lint_manifests "$WORK/lint/good" > /dev/null \
  && ! lint_manifests "$WORK/lint/bad1" > /dev/null \
  && ! lint_manifests "$WORK/lint/bad2" > /dev/null; then
  ok "arm L self-test: [features] passes; [dependencies] and [target.*.dependencies] are RED"
else
  bad "arm L self-test: the lint did not tell [features] from dependency tables"
fi
if lint_manifests "$ROOT/Cargo.toml" "$ROOT/crates" "$ROOT/crates-sys" "$ROOT/xtask" > "$WORK/lint.log"; then
  ok "arm L: every font-backend-* is a [features] entry"
else
  sed 's/^/          /' "$WORK/lint.log"
  bad "arm L: a font-backend-* feature is enabled outside [features]"
fi

command -v cargo >/dev/null 2>&1 || { echo "G-FONTBACKEND BLOCKED: cargo not installed"; exit 3; }
if [ "$(uname -s)" != "Darwin" ]; then
  if [ "$FAILED" -ne 0 ]; then echo "G-FONTBACKEND RED:$FAILED_NAMES"; exit 1; fi
  echo "G-FONTBACKEND BLOCKED: macOS-only lane (the reference's build-macos-freetype job; CoreText backends)"
  exit 3
fi

# run LABEL ENV_EXPECT cargo-args...
run() {
  local label="$1" expect="$2"; shift 2
  if RUSTTY_EXPECT_FONT_BACKEND="$expect" cargo "$@" > "$WORK/out.log" 2>&1; then
    grep -E '^test result' "$WORK/out.log" | sed 's/^/          /'
    ok "$label"
  else
    grep -E '^test result|FAILED|panicked at|^error|^warning' "$WORK/out.log" | head -20 | sed 's/^/          /'
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

# Fixture: a fonts.conf over the macOS font directories (fontconfig_freetype).
mkdir -p "$WORK/fc-cache"
cat > "$WORK/fonts.conf" <<EOF
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <dir>/System/Library/Fonts</dir>
  <dir>/Library/Fonts</dir>
  <dir>~/Library/Fonts</dir>
  <cachedir>$WORK/fc-cache</cachedir>
  <alias><family>monospace</family><prefer><family>Menlo</family></prefer></alias>
</fontconfig>
EOF

# Fixture: a %SYSTEMROOT% for the Windows directory scanner (freetype_windows).
WINROOT="$WORK/win"
mkdir -p "$WINROOT/root\\Fonts"
for f in Arial.ttf "Arial Unicode.ttf" NotoSansChakma-Regular.ttf; do
  src="/System/Library/Fonts/Supplemental/$f"
  if [ ! -f "$src" ]; then bad "freetype_windows fixture: $src is missing"; continue; fi
  cp "$src" "$WINROOT/root\\Fonts/$f"
  ln "$WINROOT/root\\Fonts/$f" "$WINROOT/root\\Fonts\\$f"
done

# Both fixtures are read only by their own backend's discoverer.
export FONTCONFIG_FILE="$WORK/fonts.conf"
export SYSTEMROOT="$WINROOT/root"

# (feature suffix, @tagName)
BACKENDS=(
  "coretext-freetype|coretext_freetype"
  "coretext-harfbuzz|coretext_harfbuzz"
  "coretext-noshape|coretext_noshape"
  "freetype|freetype"
  "fontconfig-freetype|fontconfig_freetype"
  "freetype-windows|freetype_windows"
)

i=0
for entry in "${BACKENDS[@]}"; do
  i=$((i+1))
  IFS='|' read -r feat tag <<< "$entry"
  echo "=== G-FONTBACKEND $i :: -Dfont-backend=$tag ==="
  features=(--features "rustty-build-config/font-backend-$feat")
  run "clippy rustty-font-shaper -D warnings ($tag)" "$tag" \
    clippy -p rustty-font-shaper --all-targets "${features[@]}" -- -D warnings
  pkgs=(-p rustty-font -p rustty-font-shaper -p rustty-renderer)
  case "$tag" in
    coretext_freetype|freetype)
      pkgs+=(-p rustty-core -p rustty-cli -p rustty-apprt-c)
      # apprt-c's C-ABI smoke test links the uplifted `librustty_apprt_c`
      # cdylib from the target dir; build it with THIS backend first, so the
      # test neither misses it nor links another backend's copy.
      run "build rustty-apprt-c ($tag)" "$tag" build -p rustty-apprt-c "${features[@]}"
      ;;
  esac
  run "test ${pkgs[*]} ($tag)" "$tag" test "${pkgs[@]}" --no-fail-fast "${features[@]}"
  if [ "$tag" = "coretext_freetype" ]; then
    run "build --workspace ($tag)" "$tag" build --workspace "${features[@]}"
  fi
done

if [ "$FAILED" -ne 0 ]; then
  echo "G-FONTBACKEND RED:$FAILED_NAMES"
  exit 1
fi
echo "G-FONTBACKEND GREEN"
exit 0
