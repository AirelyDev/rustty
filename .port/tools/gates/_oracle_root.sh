#!/usr/bin/env bash
# _oracle_root.sh — the shell twin of `_gatelib.oracle_root` (R5).
#
# SOURCE this from a shell gate or harness that reads the read-only ghostty
# oracle checkout, then call `rustty_oracle_root "$ROOT"`. It prints the oracle
# path and never fails: the caller keeps its own absent/pin handling.
#
# The rule is the Python helper's, restated exactly so both languages agree:
#
#   * The oracle is `ghostty` BESIDE THE MAIN CHECKOUT. `$ROOT/../ghostty` is
#     wrong inside a git worktree: from `.claude/worktrees/<wt>` it names
#     `.claude/worktrees/ghostty`, which does not exist.
#     `git rev-parse --git-common-dir` names the MAIN repository's `.git` from
#     inside any worktree, so its parent is the real checkout.
#   * Nothing is caller-settable. There is no environment override, and git runs
#     with every exported `GIT_*` variable removed: `GIT_DIR` / `GIT_WORK_TREE`
#     would otherwise redirect `--git-common-dir` to a decoy repository and,
#     with it, the oracle.
#   * git's answer is trusted only when git exits 0. Otherwise the fallback is
#     the checkout's own sibling — the pre-worktree behaviour, which from a
#     worktree names a missing directory and so reaches the caller's
#     absent/BLOCKED path rather than a vacuous green.
#
# `bash _oracle_root.sh --self-test` (G-ORACLE-SH-SELF in ci.sh) builds a
# throwaway repository with a linked worktree, checks this function against the
# Python helper, and runs a COPY of every consumer end to end
# (`--print-oracle`) with each retired override variable and a GIT_DIR +
# GIT_WORK_TREE decoy set, from a working directory outside the fixture.

# `--git-common-dir` for $1, asked of git with every exported GIT_* removed.
# The body is a subshell, so the unsets never reach the caller.
_rustty_git_common_dir() (
  for _v in $(compgen -e); do
    case "$_v" in GIT_*) unset "$_v" ;; esac
  done
  git -C "$1" rev-parse --path-format=absolute --git-common-dir 2>/dev/null
)

rustty_oracle_root() {
  local checkout="$1" common="" rc=0 main_checkout
  common="$(_rustty_git_common_dir "$checkout")" || rc=$?
  if [ "$rc" -eq 0 ] && [ -n "$common" ]; then
    main_checkout="$(dirname "$common")"
  else
    main_checkout="$checkout"
  fi
  # `abspath(join(main_checkout, "..", "ghostty"))`, lexically: both inputs are
  # absolute and normalized, so `..` is exactly one `dirname`.
  printf '%s/ghostty\n' "$(dirname "$main_checkout")"
}

# The consumers, as paths relative to the checkout root. Each accepts
# `--print-oracle`, prints the oracle it would read, and exits 0.
_RUSTTY_ORACLE_CONSUMERS=(
  ".port/tools/gates/check_eol_fidelity.sh"
  ".port/tools/gates/run_differential.sh"
  ".port/tools/macos-parity/menu-shortcut-parity.sh"
)

_rustty_oracle_self_test() {
  local here root base main_co wt oracle stale decoy decoy_oracle fakebin
  local failed=0 got want rel copy r
  here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  root="$(cd "$here/../../.." && pwd)"

  arm() { # arm <ok:0|1> <label> [detail]
    if [ "$1" = "0" ]; then
      echo "  ok   $2"
    else
      echo "  FAIL $2"
      [ -n "${3:-}" ] && echo "       ($3)"
      failed=1
    fi
  }
  clean_git() {
    # shellcheck disable=SC2046  # one `-u NAME` word pair per GIT_* variable.
    env $(compgen -e | awk '/^GIT_/ { printf "-u %s ", $0 }') \
      git -c user.name=selftest -c user.email=selftest@invalid \
      -c commit.gpgsign=false -c core.hooksPath=/dev/null \
      -c init.defaultBranch=main "$@"
  }

  echo "=== G-ORACLE-SH-SELF :: shell oracle resolution ==="
  base="$(mktemp -d "${TMPDIR:-/tmp}/goracle_sh.XXXXXX")" || { echo "BLOCKED: mktemp"; return 3; }
  base="$(cd "$base" && pwd -P)"
  # shellcheck disable=SC2064  # expand $base now: it is the fixture to remove.
  trap "rm -rf '$base'" EXIT
  main_co="$base/rustty"
  wt="$main_co/.claude/worktrees/wt"
  oracle="$base/ghostty"
  stale="$main_co/.claude/worktrees/ghostty"
  decoy="$base/decoy/rustty"
  decoy_oracle="$base/decoy/ghostty"
  mkdir -p "$main_co" "$decoy" "$decoy_oracle" || return 3
  if ! { clean_git -C "$main_co" init -q &&
    clean_git -C "$main_co" commit -q --allow-empty -m fixture &&
    clean_git -C "$main_co" worktree add -q --detach "$wt" &&
    clean_git -C "$decoy" init -q; } >/dev/null 2>&1; then
    echo "BLOCKED: could not build the git fixture"
    return 3
  fi
  # The consumers plus what they source before resolving the oracle.
  for rel in .port/tools/gates/_oracle_root.sh .port/tools/parity_names.env \
    "${_RUSTTY_ORACLE_CONSUMERS[@]}"; do
    mkdir -p "$wt/$(dirname "$rel")"
    cp "$root/$rel" "$wt/$rel" || { echo "BLOCKED: cannot copy $rel"; return 3; }
  done

  # 0. no stopgap: the worktree-sibling path the old formula used is absent.
  if [ ! -e "$stale" ] && [ ! -L "$stale" ]; then r=0; else r=1; fi
  arm "$r" "fixture has no worktree-sibling oracle (no stopgap)" "$stale"

  # 1. from the worktree and from the main checkout -> the main sibling.
  got="$(rustty_oracle_root "$wt")"
  if [ "$got" = "$oracle" ]; then r=0; else r=1; fi
  arm "$r" "from a worktree names the MAIN checkout's sibling" "got $got, want $oracle"
  got="$(rustty_oracle_root "$main_co")"
  if [ "$got" = "$oracle" ]; then r=0; else r=1; fi
  arm "$r" "from the main checkout names its sibling" "got $got, want $oracle"

  # 2. the Python helper agrees (one rule, two languages).
  if command -v python3 >/dev/null 2>&1; then
    want="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import _gatelib as G; print(G.oracle_root(sys.argv[2]))' "$here" "$wt")"
    got="$(rustty_oracle_root "$wt")"
    if [ "$got" = "$want" ]; then r=0; else r=1; fi
    arm "$r" "agrees with _gatelib.oracle_root" "shell $got, python $want"
  else
    echo "  skip agrees with _gatelib.oracle_root (python3 not installed)"
  fi

  # 3. GIT_DIR + GIT_WORK_TREE cannot redirect it.
  got="$(GIT_DIR="$decoy/.git" GIT_WORK_TREE="$decoy" rustty_oracle_root "$wt")"
  if [ "$got" = "$oracle" ]; then r=0; else r=1; fi
  arm "$r" "GIT_DIR + GIT_WORK_TREE do not redirect the oracle" "got $got, want $oracle"

  # 4. git's stdout is trusted only on exit 0: a git that prints the decoy's
  #    common dir but fails must give the fallback, not the decoy's sibling.
  fakebin="$base/fakebin"
  mkdir -p "$fakebin"
  printf '#!/bin/sh\necho %s/.git\nexit 1\n' "$decoy" >"$fakebin/git"
  chmod +x "$fakebin/git"
  got="$(PATH="$fakebin:$PATH" rustty_oracle_root "$wt")"
  want="$(dirname "$wt")/ghostty"
  if [ "$got" = "$want" ]; then r=0; else r=1; fi
  arm "$r" "a failing git's output is ignored (fallback: the checkout's sibling)" "got $got, want $want"

  # 5. every consumer, end to end, from a cwd OUTSIDE the fixture checkout,
  #    with each retired override variable and the GIT_* decoy set.
  for rel in "${_RUSTTY_ORACLE_CONSUMERS[@]}"; do
    copy="$wt/$rel"
    got="$(cd "$decoy" && GHOSTTY_ORACLE="$decoy_oracle" GHOSTTY="$decoy_oracle" \
      GHOSTTY_ROOT="$decoy_oracle" ORACLE_TREE="$decoy_oracle" \
      GIT_DIR="$decoy/.git" GIT_WORK_TREE="$decoy" \
      bash "$copy" --print-oracle 2>&1)"
    if [ "$got" = "$oracle" ]; then r=0; else r=1; fi
    arm "$r" "$(basename "$rel") ignores env overrides and GIT_* (cwd outside the fixture)" \
      "got '$got', want '$oracle'"
  done

  # 6. the live run: inside a linked worktree, the resolved oracle is the main
  #    checkout's sibling, not the worktree's own (a string compare, so a
  #    stopgap symlink at the worktree sibling cannot satisfy it).
  local gd gc
  gd="$(clean_git -C "$root" rev-parse --path-format=absolute --git-dir 2>/dev/null)"
  gc="$(clean_git -C "$root" rev-parse --path-format=absolute --git-common-dir 2>/dev/null)"
  if [ -n "$gd" ] && [ -n "$gc" ] && [ "$gd" != "$gc" ]; then
    want="$(dirname "$(dirname "$gc")")/ghostty"
    got="$(rustty_oracle_root "$root")"
    if [ "$got" = "$want" ] && [ "$got" != "$(dirname "$root")/ghostty" ]; then r=0; else r=1; fi
    arm "$r" "live worktree run resolves to the main checkout's sibling" "got $got, want $want"
  else
    echo "  ok   live run is not in a linked worktree (the worktree case is the fixture above)"
  fi

  if [ "$failed" -ne 0 ]; then
    echo "G-ORACLE-SH-SELF RED: an arm above did not hold."
    return 1
  fi
  echo "G-ORACLE-SH-SELF GREEN: the shell resolver and its ${#_RUSTTY_ORACLE_CONSUMERS[@]} consumers hold."
  return 0
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  set -uo pipefail
  case "${1:-}" in
    --self-test) _rustty_oracle_self_test; exit $? ;;
    *) echo "usage: $0 --self-test   (otherwise, source this file)" >&2; exit 2 ;;
  esac
fi
