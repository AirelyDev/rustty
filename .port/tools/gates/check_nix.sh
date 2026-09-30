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
# Step 4 builds the dev shell (`devShells.<system>.default`), which upstream's
# alejandra job realises through `nix develop` and step 3 no longer does (N488).
# Its failure is classified from the build log (`classify_devshell_log`):
# success is GREEN; a failure whose ONLY failing derivation is nixpkgs'
# `python3.N-wasmtime` on aarch64-linux is BLOCKED (N489: that package fails its
# own pytest phase there; upstream's devShell.nix carries the same dependency);
# any other failure is RED. `--self-test` runs the classifier over fixture logs
# and needs no nix (G-NIX-SELF).
#
# BLOCKED (exit 3) — never green — when nix is unavailable, which is the normal
# case on a developer machine without it and on the macOS CI leg.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

red() { echo "G-NIX RED: $*" >&2; exit 1; }

# classify_devshell_log LOG SYSTEM RC
# Prints one verdict line and returns 0 (GREEN), 1 (RED) or 3 (BLOCKED, N489).
# A failing derivation is a LEAF when nix reports it failed for its own reason
# (new format: `error: Cannot build '<drv>'.` then a `Reason:` other than a
# failed dependency; old format: `error: builder for '<drv>' failed ...`).
# Dependency-propagation errors are not leaves. Any other `error:` line
# (evaluation, hash mismatch, ...) is RED on its own.
classify_devshell_log() {
  local log="$1" sys="$2" rc="$3" out leaves others nonwasm
  if [ "$rc" = "0" ]; then
    echo "devshell GREEN: devShells.$sys.default built"
    return 0
  fi
  out="$(awk -v q="'" '
    function drv_of(line,   i, rest, j) {
      i = index(line, q "/nix/store/")
      if (i == 0) return ""
      rest = substr(line, i + 1)
      j = index(rest, q)
      if (j == 0) return ""
      rest = substr(rest, 1, j - 1)
      sub(/^.*\//, "", rest)
      return rest
    }
    /^error: Cannot build / {
      if (pending != "") print "OTHER no Reason line for " pending
      pending = drv_of($0)
      if (pending == "") print "OTHER " $0
      next
    }
    /^[[:space:]]+Reason: / {
      if (pending != "") {
        if ($0 !~ /Reason: [0-9]+ dependenc(y|ies) failed/) print "LEAF " pending
        pending = ""
      }
      next
    }
    /^error: builder for / { d = drv_of($0); if (d == "") print "OTHER " $0; else print "LEAF " d; next }
    /^error: [0-9]+ dependencies of derivation / { next }
    /^error: build of / { next }
    /^error: / { print "OTHER " $0 }
    END { if (pending != "") print "OTHER no Reason line for " pending }
  ' "$log")"
  leaves="$(printf '%s\n' "$out" | sed -n 's/^LEAF //p')"
  others="$(printf '%s\n' "$out" | sed -n 's/^OTHER //p')"
  if [ -n "$others" ]; then
    echo "devshell RED: a non-build error: $(printf '%s\n' "$others" | head -1)"
    return 1
  fi
  if [ -z "$leaves" ]; then
    echo "devshell RED: the build failed (rc $rc) and no failing derivation was identified"
    return 1
  fi
  nonwasm="$(printf '%s\n' "$leaves" | grep -Ev '^[a-z0-9]{32}-python3\.[0-9]+-wasmtime-[0-9][0-9.]*\.drv$' || true)"
  if [ -n "$nonwasm" ]; then
    echo "devshell RED: failing derivation(s) other than nixpkgs' wasmtime: $(printf '%s' "$nonwasm" | tr '\n' ' ')"
    return 1
  fi
  if [ "$sys" != "aarch64-linux" ]; then
    echo "devshell RED: nixpkgs' wasmtime failed on $sys (N489 covers aarch64-linux only): $(printf '%s' "$leaves" | tr '\n' ' ')"
    return 1
  fi
  echo "devshell BLOCKED (N489): only nixpkgs' wasmtime failed on $sys: $(printf '%s' "$leaves" | tr '\n' ' ')"
  return 3
}

# G-NIX-SELF: the classifier over fixture logs; builds nothing, needs no nix.
self_test() {
  local tmp fails=0
  tmp="$(mktemp -d)"
  # Derivation names from the real aarch64-linux failure (rvm, nix 2.35.2, P7-W12 T5 round 1).
  local W='/nix/store/s7g8hsh2cfl3piimil4jxqsks6gndw3b-python3.14-wasmtime-47.0.1.drv'
  local ENV='/nix/store/7fjg87dmhgbcm3c8dj1r16rlm5qw355y-python3-3.14.7-env.drv'
  local SH='/nix/store/kaim4lqrcfyq6jiriaraywn8l0pdkgk9-rustty.drv'
  local U='/nix/store/0123456789abcdfghijklmnpqrsvwxyz-python3.14-ucs-detect-1.0.8.drv'
  cat >"$tmp/wasm_only" <<LOG
building '$W'...
error: Cannot build '$W'.
       Reason: builder failed with exit code 2.
       Output paths:
         /nix/store/yhl8wv22z9rgrmpgn76jkl3j8n6xbdbf-python3.14-wasmtime-47.0.1
       > ERROR ci/cbindgen.py - RuntimeError: bindings need an update, run this script
error: Cannot build '$ENV'.
       Reason: 1 dependency failed.
error: Cannot build '$SH'.
       Reason: 1 dependency failed.
LOG
  {
    cat "$tmp/wasm_only"
    printf "error: Cannot build '%s'.\n       Reason: builder failed with exit code 1.\n" "$U"
  } >"$tmp/wasm_plus"
  cat >"$tmp/other_only" <<LOG
error: Cannot build '$U'.
       Reason: builder failed with exit code 1.
error: Cannot build '$SH'.
       Reason: 1 dependency failed.
LOG
  cat >"$tmp/old_fmt" <<LOG
error: builder for '$W' failed with exit code 2;
       last 10 log lines:
error: 1 dependencies of derivation '$ENV' failed to build
error: build of '$SH' failed
LOG
  {
    cat "$tmp/wasm_only"
    echo "error: hash mismatch in fixed-output derivation '$U':"
  } >"$tmp/wasm_hash"
  {
    cat "$tmp/wasm_only"
    printf "error: Cannot build '%s'.\n       Reason: timed out after 3600 seconds.\n" "$U"
  } >"$tmp/wasm_timeout"
  printf "error: flake 'git+file:///x' does not provide attribute 'devShells.aarch64-linux.default'\n" >"$tmp/eval"
  : >"$tmp/empty"
  # name | log | system | rc | expected return
  local cases=(
    "success|empty|aarch64-linux|0|0"
    "wasmtime-only-aarch64|wasm_only|aarch64-linux|1|3"
    "wasmtime-only-old-format|old_fmt|aarch64-linux|1|3"
    "wasmtime-only-x86_64|wasm_only|x86_64-linux|1|1"
    "wasmtime-only-darwin|wasm_only|aarch64-darwin|1|1"
    "wasmtime-plus-another|wasm_plus|aarch64-linux|1|1"
    "another-only|other_only|aarch64-linux|1|1"
    "wasmtime-plus-hash-mismatch|wasm_hash|aarch64-linux|1|1"
    "wasmtime-plus-timeout|wasm_timeout|aarch64-linux|1|1"
    "eval-error|eval|aarch64-linux|1|1"
    "failed-without-errors|empty|aarch64-linux|1|1"
  )
  local c name log sys rc want got line
  for c in "${cases[@]}"; do
    IFS='|' read -r name log sys rc want <<<"$c"
    line="$(classify_devshell_log "$tmp/$log" "$sys" "$rc")"
    got=$?
    if [ "$got" = "$want" ]; then
      echo "  ok   $name -> $got ($line)"
    else
      echo "  FAIL $name: expected $want, got $got ($line)"
      fails=$((fails + 1))
    fi
  done
  rm -rf "$tmp"
  if [ "$fails" -ne 0 ]; then
    echo "G-NIX-SELF RED: $fails classifier case(s) wrong" >&2
    return 1
  fi
  echo "G-NIX-SELF GREEN: ${#cases[@]} dev-shell classifier cases"
  return 0
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
  exit $?
fi

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

echo "=== G-NIX 1/4 :: nix flake check ==="
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

echo "=== G-NIX 2/4 :: NixOS VM integration tests ==="
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

echo "=== G-NIX 3/4 :: alejandra --check ==="
# PORT NOTE: upstream runs `nix develop -c alejandra --check .` (test.yml:1920). This
# runs `nix fmt -- --check .`, the flake's own `formatter` output (alejandra): same tool,
# same files, same exit status. `nix develop` realises the WHOLE dev shell (python
# wasmtime, the GTK toolchain, ...), and nixpkgs' python3.14-wasmtime does not build on
# aarch64-linux (N489), which reddened a formatting check that never needed it (N488).
# The dev-shell build that `nix develop` implied is kept as its own step, 4/4.
if ! nix "${NIXFLAGS[@]}" fmt -- --check . 2>&1 | tail -5; then
  echo "G-NIX RED: Nix files are not alejandra-formatted (run 'nix fmt')" >&2
  exit 1
fi

echo "=== G-NIX 4/4 :: dev shell build ==="
sys="$(nix "${NIXFLAGS[@]}" eval --raw --impure --expr 'builtins.currentSystem')" \
  || red "cannot determine the current nix system"
# --keep-going so every failing derivation is reported, not only the first:
# the BLOCKED arm needs the complete set.
nix "${NIXFLAGS[@]}" build ".#devShells.$sys.default" --no-link --keep-going >"$LOG" 2>&1
rc=$?
tail -30 "$LOG"
verdict="$(classify_devshell_log "$LOG" "$sys" "$rc")"
vrc=$?
echo "    $verdict"
case "$vrc" in
  0) ;;
  3)
    echo "G-NIX BLOCKED: flake checks passed and the Nix files are formatted, but the dev shell"
    echo "  cannot be built on $sys: nixpkgs' python3.N-wasmtime fails (N489)."
    exit 3
    ;;
  *) red "the dev shell devShells.$sys.default did not build (see above)" ;;
esac

echo "G-NIX GREEN: flake checks passed, the Nix files are formatted and the dev shell builds."
exit 0
