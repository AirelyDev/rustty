#!/usr/bin/env bash
# GATE G-LINT — the non-Rust linter lane.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `prettier`, `typos`,
# `shellcheck`, `swiftlint`, `alejandra`, and `pinact` jobs. The oracle runs
# each as its own CI job through `nix develop -c <tool>`; rustty runs them all
# from one gate so `.port/tools/ci.sh` stays the single source of truth for the
# check sequence (see `.github/workflows/ci.yml`'s header) and so a contributor
# gets the identical result locally.
#
# Rust's own formatter/linter are NOT here: `cargo fmt --check` and `cargo
# clippy -D warnings` are steps 1 and 2 of ci.sh proper (they are the analog of
# the oracle's `zig-fmt` job).
#
# TOOL AVAILABILITY IS EXPLICIT, NEVER SILENT. Each linter is one of:
#     ok       ran and passed
#     FAIL     ran and found something  -> gate exits 1
#     skipped  the tool is not installed, or does not apply on this host
# A skipped linter is reported by name in the summary. If EVERY linter is
# skipped the gate exits 3 (BLOCKED) rather than 0, because a green over an
# empty set is not a pass. Once the Nix flake lands (Batch 4,
# docs/superpowers/specs/2026-07-15-batch4-nix-flake-design.md) `nix develop`
# supplies all of them and `skipped` should disappear from CI.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
#
# Usage:
#   bash .port/tools/gates/check_lints.sh              # all linters
#   bash .port/tools/gates/check_lints.sh shellcheck   # only the named ones
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

WANT="$*"
want() { [ -z "$WANT" ] || printf '%s\n' $WANT | grep -qx "$1"; }

RAN=0
FAILED=0
SKIPPED=""

pass()  { echo "  ok      $1"; RAN=$((RAN+1)); }
fail()  { echo "  FAIL    $1"; RAN=$((RAN+1)); FAILED=1; }
skip()  { echo "  skipped $1 ($2)"; SKIPPED="$SKIPPED $1"; }

# ---------------------------------------------------------------------------
# ShellCheck  (capital S on purpose: a comment line that begins with the exact
# token `shellcheck` is parsed as a directive and errors out with SC1073.)
#
# File list mirrors the oracle's `find` plus the exclusions rustty needs:
# cargo/zig output trees, the Xcode build dir, and the vendored C sources under
# crates-sys/*/vendor. zsh-shebang scripts are excluded because shellcheck
# cannot parse zsh at all (it silently analyses them as bash and invents
# findings); they are listed in the summary so the exclusion is never silent.
# ---------------------------------------------------------------------------
run_shellcheck() {
  want shellcheck || return 0
  if ! command -v shellcheck >/dev/null 2>&1; then
    skip shellcheck "not installed"; return 0
  fi

  local list zsh_list
  list="$(mktemp)"; zsh_list="$(mktemp)"
  find . \( -name '*.sh' -o -name '*.bash' \) -type f \
    ! -path './target/*' ! -path './zig-out/*' ! -path './macos/build/*' \
    ! -path './.git/*' ! -path '*/vendor/*' ! -path './build-cmake/*' \
    | sort > "$list"

  # Split out anything whose shebang names zsh.
  : > "$zsh_list"
  local f
  while IFS= read -r f; do
    case "$(head -1 "$f")" in *zsh*) echo "$f" >> "$zsh_list" ;; esac
  done < "$list"
  if [ -s "$zsh_list" ]; then
    grep -Fxv -f "$zsh_list" "$list" > "$list.tmp" && mv "$list.tmp" "$list"
  fi

  local n out rc
  n="$(wc -l < "$list" | tr -d ' ')"
  if [ "$n" -eq 0 ]; then
    skip shellcheck "no shell scripts matched"; rm -f "$list" "$zsh_list"; return 0
  fi
  # `xargs < file`, not `xargs -a` — BSD xargs (macOS) has no -a.
  out="$(xargs shellcheck --check-sourced --severity=warning < "$list" 2>&1)"; rc=$?
  if [ $rc -eq 0 ]; then
    pass "shellcheck ($n files)"
  else
    printf '%s\n' "$out" | sed 's/^/          /'
    fail "shellcheck ($n files)"
  fi
  if [ -s "$zsh_list" ]; then
    echo "          note: not linted (zsh, unsupported by shellcheck):"
    sed 's/^/            /' "$zsh_list"
  fi
  rm -f "$list" "$zsh_list"
}

# ---------------------------------------------------------------------------
# prettier — `prettier --check .`, honoring .prettierignore.
#
# Falls back to `npx prettier@3` when the binary is not on PATH, so a plain
# Node install is enough. The Nix devShell (Batch 4) becomes the version pin;
# until then the major is pinned here so CI and local agree.
# ---------------------------------------------------------------------------
PRETTIER_PIN="prettier@3"
run_prettier() {
  want prettier || return 0
  local -a cmd
  if command -v prettier >/dev/null 2>&1; then
    cmd=(prettier)
  elif command -v npx >/dev/null 2>&1; then
    cmd=(npx --yes "$PRETTIER_PIN")
  else
    skip prettier "no prettier and no npx"; return 0
  fi
  local out rc
  out="$("${cmd[@]}" --check . 2>&1)"; rc=$?
  if [ $rc -eq 0 ]; then
    pass "prettier"
  else
    printf '%s\n' "$out" | grep -v '^npm notice' | sed 's/^/          /'
    fail "prettier"
  fi
}

# ---------------------------------------------------------------------------
# typos — configured by typos.toml at the repo root.
# ---------------------------------------------------------------------------
run_typos() {
  want typos || return 0
  if ! command -v typos >/dev/null 2>&1; then
    skip typos "not installed (brew install typos-cli)"; return 0
  fi
  local out rc
  out="$(typos 2>&1)"; rc=$?
  if [ $rc -eq 0 ]; then pass typos; else printf '%s\n' "$out" | sed 's/^/          /'; fail typos; fi
}

# ---------------------------------------------------------------------------
# swiftlint — macOS only; .swiftlint.yml scopes it to macos/.
# ---------------------------------------------------------------------------
run_swiftlint() {
  want swiftlint || return 0
  if [ "$(uname -s)" != "Darwin" ]; then
    skip swiftlint "macOS-only lane"; return 0
  fi
  if ! command -v swiftlint >/dev/null 2>&1; then
    skip swiftlint "not installed (brew install swiftlint)"; return 0
  fi
  local out rc
  out="$(swiftlint lint --strict 2>&1)"; rc=$?
  if [ $rc -eq 0 ]; then pass swiftlint; else printf '%s\n' "$out" | sed 's/^/          /'; fail swiftlint; fi
}

# ---------------------------------------------------------------------------
# alejandra — the Nix formatter. Applies only once Batch 4 lands *.nix files;
# until then there is nothing to format and the lane reports skipped rather
# than a vacuous green.
# ---------------------------------------------------------------------------
run_alejandra() {
  want alejandra || return 0
  if [ -z "$(find . -name '*.nix' -not -path './target/*' -not -path '*/vendor/*' -print -quit)" ]; then
    skip alejandra "no .nix files yet (Batch 4)"; return 0
  fi
  if ! command -v alejandra >/dev/null 2>&1; then
    skip alejandra "not installed"; return 0
  fi
  local out rc
  out="$(alejandra --check . 2>&1)"; rc=$?
  if [ $rc -eq 0 ]; then pass alejandra; else printf '%s\n' "$out" | sed 's/^/          /'; fail alejandra; fi
}

# ---------------------------------------------------------------------------
# pinact — verifies every GitHub Actions `uses:` is pinned to a full commit
# SHA (.github/pinact.yml). Needs a GITHUB_TOKEN to resolve refs against the
# API; without one it would rate-limit into a false red, so it skips.
# ---------------------------------------------------------------------------
run_pinact() {
  want pinact || return 0
  if ! command -v pinact >/dev/null 2>&1; then
    skip pinact "not installed (brew install pinact)"; return 0
  fi
  if [ -z "${GITHUB_TOKEN:-}" ]; then
    skip pinact "no GITHUB_TOKEN (would rate-limit)"; return 0
  fi
  local out rc
  out="$(pinact run --check 2>&1)"; rc=$?
  if [ $rc -eq 0 ]; then pass pinact; else printf '%s\n' "$out" | sed 's/^/          /'; fail pinact; fi
}

echo "=== G-LINT :: non-Rust linters ==="
run_shellcheck
run_prettier
run_typos
run_swiftlint
run_alejandra
run_pinact

echo "--- G-LINT summary: $RAN ran, $(printf '%s' "$SKIPPED" | wc -w | tr -d ' ') skipped ---"
if [ "$RAN" -eq 0 ]; then
  echo "G-LINT BLOCKED: no linter was available on this host; skipped:$SKIPPED"
  exit 3
fi
if [ "$FAILED" -ne 0 ]; then
  echo "G-LINT RED: at least one linter reported findings (see above)."
  exit 1
fi
echo "G-LINT GREEN ($RAN linter(s) clean).${SKIPPED:+ Not run:$SKIPPED}"
exit 0
