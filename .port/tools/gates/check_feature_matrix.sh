#!/usr/bin/env bash
# GATE G-FEATMATRIX — standalone crate builds and single-feature matrices.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `test-pkg-linux`
# (`matrix: pkg: ["wuffs"]` — build a pkg/ subproject on its own) and
# `test-sentry-linux` (`matrix: sentry: ["true","false"]` — build both sides of
# a build option).
#
# WHY STANDALONE BUILDS ARE NOT REDUNDANT WITH `cargo build --workspace`.
# Cargo UNIFIES features across a workspace build: if any member enables a
# feature on a shared dependency, every member sees it. So a crate whose own
# Cargo.toml forgets a feature or a dependency still compiles in the workspace
# — and breaks the moment someone depends on it alone, which is exactly what
# `cargo build -p <crate>` (and any downstream consumer) does. This branch has
# already seen that class of bug twice: the lib-vt tarball slice and the
# `--no-default-features` CMake path.
#
# The crates checked are rustty's analogs of the reference's `pkg/*` wrapper
# subprojects — the safe Rust wrappers over vendored C, and the ports of
# ghostty's third-party Zig dependencies.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

command -v cargo >/dev/null 2>&1 || { echo "G-FEATMATRIX BLOCKED: cargo not installed"; exit 3; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

RAN=0
FAILED=0
FAILED_NAMES=""

ok()  { echo "  ok      $1"; RAN=$((RAN+1)); }
bad() { echo "  FAIL    $1"; RAN=$((RAN+1)); FAILED=1; FAILED_NAMES="$FAILED_NAMES $1"; }

run() {
  local label="$1"; shift
  if cargo "$@" > "$WORK/out.log" 2>&1; then
    ok "$label"
  else
    tail -20 "$WORK/out.log" | sed 's/^/          /'
    bad "$label"
  fi
}

# ── 1. standalone crate builds (the reference's test-pkg-linux) ──────────────
# The `pkg/*` analogs: safe wrappers over vendored C, and the ports of
# ghostty's third-party Zig dependencies.
STANDALONE=(
  rustty-wuffs      # the reference's own pkg/wuffs, the one it actually tests
  rustty-oniguruma  # pkg/oniguruma
  rustty-z2d        # vancluever/z2d
  rustty-zf         # natecraddock/zf
  rustty-vaxis      # libvaxis
  rustty-xev        # libxev
  rustty-zigimg     # zigimg
)

echo "=== G-FEATMATRIX 1/2 :: standalone crate builds ==="
for c in "${STANDALONE[@]}"; do
  [ -d "$ROOT/crates/$c" ] || { echo "  skipped $c (not present)"; continue; }
  run "$c (alone)" build -p "$c"
done

# ── 2. single-feature matrices (the reference's test-sentry-linux) ───────────
# crate|feature — built BOTH ways, because a feature that only ever builds in
# one position is a feature nobody has compiled the other half of.
echo "=== G-FEATMATRIX 2/2 :: feature on/off matrices ==="
MATRIX=(
  "rustty-crash|sentry"
  # The packaging build options — the ports of the reference's `-Dflatpak=true`
  # and `-Dsnap`, which its own manifests pass and which its `test-sentry-linux`
  # job models for `-Dsentry`.
  #
  # These belong here more than sentry does: both were hardcoded `false` with no
  # seam at all, so the Flatpak shipped with its portal/DBus integration,
  # `flatpak-spawn --host` and host passwd lookup inert. Building BOTH sides is
  # what keeps that from silently returning — and the first build with the
  # option ON found that the path had never compiled (a lossy `EnvMap` alias)
  # and then that it had never linked (`G_VARIANT_TYPE` is a glib MACRO, not an
  # exported symbol).
  "rustty-build-config|snap"
)

# `flatpak` is Linux/FreeBSD-only, in rustty exactly as in the reference: it
# implies GTK and links gio-2.0 (`SharedDeps.zig:663-685` — "when we're
# targeting flatpak we ALWAYS link GTK"), neither of which exists on macOS.
# Reported skipped BY NAME elsewhere rather than quietly dropped.
case "$(uname -s)" in
  Linux|FreeBSD) MATRIX+=("rustty-build-config|flatpak") ;;
  *) FLATPAK_SKIP=1 ;;
esac
for spec in "${MATRIX[@]}"; do
  crate="${spec%%|*}"
  feat="${spec##*|}"
  [ -d "$ROOT/crates/$crate" ] || { echo "  skipped $crate (not present)"; continue; }
  run "$crate --features $feat" build -p "$crate" --features "$feat"
  run "$crate --no-default-features" build -p "$crate" --no-default-features
done

echo "--- G-FEATMATRIX summary: $RAN build(s) ---"
if [ "$RAN" -eq 0 ]; then
  echo "G-FEATMATRIX BLOCKED: nothing was built."
  exit 3
fi
if [ "$FAILED" -ne 0 ]; then
  echo "G-FEATMATRIX RED:$FAILED_NAMES"
  exit 1
fi
echo "G-FEATMATRIX GREEN ($RAN build(s) clean)."
exit 0
