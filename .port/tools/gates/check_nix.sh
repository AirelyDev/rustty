#!/usr/bin/env bash
# GATE G-NIX — the Nix flake lane.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `build-nix` job, plus the
# `alejandra` job (the flake's own `formatter` output).
#
# Runs `nix flake check`, which for rustty builds `librustty-vt` and its five
# passthru tests:
#   sanity-check        the exported symbols are present in BOTH artifacts, and
#                       (simd builds) the vendored simdutf/highway objects
#                       really landed in the archive
#   pkg-config          testers.hasPkgConfigModules over the installed .pc pair
#   pkg-config-libs     the .pc files advertise the right -l / archive path
#   build-with-shared   a C program compiled, linked and RUN against the shared
#                       library via pkg-config
#   build-with-static   the same program against the static archive, asserted
#                       NOT to depend on the shared library
#
# That overlaps G-CCONSUMER on purpose and from a different angle: G-CCONSUMER
# proves the CMake install works on the developer's own machine, this proves it
# works in a hermetic sandbox with a pinned toolchain and no ambient system
# libraries. The `_libintl_locale_name_canonicalize` defect in
# crates/rustty-os/src/i18n.rs was found by THIS lane's build-with-static, not
# by the host one, because the host had a Homebrew gettext lying around.
#
# BLOCKED (exit 3) — never green — when nix is unavailable, which is the normal
# case on a developer machine without it and on the macOS CI leg.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

if ! command -v nix >/dev/null 2>&1; then
  echo "G-NIX BLOCKED: nix is not installed."
  echo "  The flake is verified on a Linux host or in a container; see"
  echo "  docs/superpowers/specs/2026-07-15-batch4-nix-flake-design.md."
  exit 3
fi

[ -f "$ROOT/flake.nix" ] || { echo "G-NIX RED: flake.nix is missing" >&2; exit 1; }
[ -f "$ROOT/flake.lock" ] || { echo "G-NIX RED: flake.lock is missing (run 'nix flake lock')" >&2; exit 1; }

# Flakes + nix-command are still gated behind experimental-features on many
# installs; ask for them explicitly rather than depending on the host's
# nix.conf.
NIXFLAGS=(--extra-experimental-features "nix-command flakes")

LOG="$(mktemp)"
trap 'rm -f "$LOG"' EXIT

echo "=== G-NIX 1/2 :: nix flake check ==="
if ! nix "${NIXFLAGS[@]}" flake check 2>&1 | tee "$LOG" | tail -40; then
  echo "G-NIX RED: nix flake check failed (see above)" >&2
  exit 1
fi
# `nix flake check` can exit 0 having built nothing; require the explicit
# success line so an empty check set cannot read as a pass.
if ! grep -q "all checks passed" "$LOG"; then
  echo "G-NIX RED: nix flake check did not report 'all checks passed'" >&2
  exit 1
fi

echo "=== G-NIX 2/2 :: alejandra --check ==="
if ! nix "${NIXFLAGS[@]}" develop --command alejandra --check . 2>&1 | tail -5; then
  echo "G-NIX RED: Nix files are not alejandra-formatted (run 'nix develop -c alejandra .')" >&2
  exit 1
fi

echo "G-NIX GREEN: flake checks passed and the Nix files are formatted."
exit 0
