#!/usr/bin/env bash
# GATE G-NIX — the Nix flake lane.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `build-nix` job, plus the
# `alejandra` job (the flake's own `formatter` output).
#
# Runs `nix flake check` plus, under RUSTTY_NIX_VM_TESTS=1, the NixOS VM
# integration tests in the flake's `vmChecks` output (see flake.nix for why
# they are a separate output rather than inside `checks`).
#
# `nix flake check` for rustty builds `librustty-vt`, the `rustty`
# GTK app (on Linux, where nix/package.nix applies — its own installCheckPhase
# runs `rustty +version`), and librustty-vt's five passthru tests:
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

echo "=== G-NIX 1/3 :: nix flake check ==="
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

echo "=== G-NIX 2/3 :: NixOS VM integration tests ==="
if [ "${RUSTTY_NIX_VM_TESTS:-0}" != "1" ]; then
  echo "    skipped vmChecks (set RUSTTY_NIX_VM_TESTS=1 — they boot a NixOS guest)"
else
  sys="$(nix "${NIXFLAGS[@]}" eval --raw --impure --expr 'builtins.currentSystem')"
  names="$(nix "${NIXFLAGS[@]}" eval --json ".#vmChecks.$sys" --apply 'builtins.attrNames' 2>/dev/null \
            | tr -d '[]"' | tr ',' ' ')"
  [ -n "$names" ] || red "vmChecks.$sys exposes no tests"
  for t in $names; do
    echo "    --- $t ---"
    if ! nix "${NIXFLAGS[@]}" build ".#vmChecks.$sys.$t" --no-link 2>&1 | tail -20; then
      echo "G-NIX RED: NixOS VM test $t failed" >&2
      exit 1
    fi
    echo "    ok      $t"
  done
fi

echo "=== G-NIX 3/3 :: alejandra --check ==="
# `nix fmt` runs the flake's own `formatter` output (alejandra). It is used instead of
# `nix develop --command alejandra`, which realises the WHOLE dev shell (python wasmtime,
# the GTK toolchain, ...): a dev-shell dependency failing to build reddened a formatting
# check that never needed it (N488). Same tool, same files, same exit status.
if ! nix "${NIXFLAGS[@]}" fmt -- --check . 2>&1 | tail -5; then
  echo "G-NIX RED: Nix files are not alejandra-formatted (run 'nix fmt')" >&2
  exit 1
fi

echo "G-NIX GREEN: flake checks passed and the Nix files are formatted."
exit 0
