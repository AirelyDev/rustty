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
#     worktree names a missing directory.
#   * `rustty_oracle_root` only LOCATES the oracle; it never checks that it is
#     there. Each caller must turn a missing or off-pin oracle into BLOCKED
#     (exit 3), never into a "skipped" that still lets the gate go green.
#     `rustty_oracle_pin_status` is the shared check: G-EOL arm 3, G4's
#     staleness guard and D10 use it, and self-test arms assert their BLOCKED.
#   * The oracle pin is written in ONE place, `ORACLE_PIN` beside this file.
#     `rustty_oracle_pin` reads it, and so does `_gatelib.oracle_pin`.
#
# `bash _oracle_root.sh --self-test` (G-ORACLE-SH-SELF in ci.sh) builds a
# throwaway repository with a linked worktree and a pinned fixture oracle. It
# checks this resolver against the Python helper, and runs a COPY of every
# consumer end to end with each retired override variable and a GIT_DIR +
# GIT_WORK_TREE decoy set, from a working directory outside the fixture:
# `--print-oracle` for the location, D10's `--print-oracle-reads` for a real
# oracle read, and G-EOL in full for arm 3's BLOCKED paths. A structural arm
# scans the real consumers for a plain `git -C "$ORACLE"` and for a second
# assignment of the oracle variable. The gates that read the oracle at a fixed
# path (G4's guard, G-BENCH, G-LZ4, A5) run their own `--self-test-oracle`
# from here.

# `git "$@"` with every exported GIT_* variable removed — for any git call that
# locates a checkout or reads the oracle (`git -C "$ORACLE" show ...`), which
# GIT_DIR would otherwise answer from another repository. The body is a
# subshell, so the unsets never reach the caller.
rustty_oracle_git() (
  for _v in $(compgen -e); do
    case "$_v" in GIT_*) unset "$_v" ;; esac
  done
  git "$@"
)

# The directory this library lives in, and so where `ORACLE_PIN` is.
_RUSTTY_ORACLE_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Print the 40-hex oracle pin from `ORACLE_PIN`. Prints nothing and returns 1
# when the file is missing or malformed; callers treat that as BLOCKED.
rustty_oracle_pin() {
  local pin=""
  [ -r "$_RUSTTY_ORACLE_LIB_DIR/ORACLE_PIN" ] || return 1
  IFS= read -r pin <"$_RUSTTY_ORACLE_LIB_DIR/ORACLE_PIN" || [ -n "$pin" ] || return 1
  case "$pin" in
    *[!0-9a-f]* | "") return 1 ;;
  esac
  [ "${#pin}" -eq 40 ] || return 1
  printf '%s\n' "$pin"
}

# rustty_oracle_pin_status ORACLE PIN [PATHSPEC...]
# Returns 0 when ORACLE is a git checkout whose HEAD is exactly PIN and, when
# pathspecs are given, whose working tree is clean under them (no modified,
# staged or untracked file). Otherwise prints one line saying why and returns
# 1. Every git call goes through `rustty_oracle_git`.
rustty_oracle_pin_status() {
  local oracle="$1" pin="${2:-}" head dirty
  if [ "$#" -ge 2 ]; then shift 2; else shift "$#"; fi
  if [ ! -d "$oracle" ]; then
    echo "no oracle checkout at $oracle"
    return 1
  fi
  if [ -z "$pin" ]; then
    echo "the oracle pin file $_RUSTTY_ORACLE_LIB_DIR/ORACLE_PIN is missing or not a 40-hex commit"
    return 1
  fi
  if ! head="$(rustty_oracle_git -C "$oracle" rev-parse --verify -q HEAD 2>/dev/null)"; then
    echo "$oracle is not a git checkout with a HEAD"
    return 1
  fi
  if [ "$head" != "$pin" ]; then
    echo "the oracle at $oracle is at $head, not the pin $pin"
    return 1
  fi
  if [ "$#" -gt 0 ]; then
    if ! dirty="$(rustty_oracle_git -C "$oracle" status --porcelain --untracked-files=all -- "$@" 2>/dev/null)"; then
      echo "cannot read the oracle's working-tree status under $*"
      return 1
    fi
    if [ -n "$dirty" ]; then
      echo "the oracle's working tree under $* differs from the pin ($(printf '%s\n' "$dirty" | wc -l | tr -d ' ') path(s))"
      return 1
    fi
  fi
  return 0
}

# Fixture helpers for the self-tests (this file's, and the consumers'
# `--self-test-oracle`). `rustty_fixture_git` is git with GIT_* removed and a
# fixed identity. `rustty_fixture_repo DIR PATH CONTENT` makes DIR a repository
# whose one commit holds CONTENT at PATH, points `origin/main` at that commit,
# and prints it.
rustty_fixture_git() {
  rustty_oracle_git -c user.name=selftest -c user.email=selftest@invalid \
    -c commit.gpgsign=false -c core.hooksPath=/dev/null \
    -c init.defaultBranch=main "$@"
}
rustty_fixture_repo() {
  local dir="$1" path="$2" content="$3"
  mkdir -p "$dir/$(dirname "$path")" || return 1
  printf '%s' "$content" >"$dir/$path" || return 1
  { rustty_fixture_git -C "$dir" init -q &&
    rustty_fixture_git -C "$dir" add -- "$path" &&
    rustty_fixture_git -C "$dir" commit -q -m fixture &&
    rustty_fixture_git -C "$dir" update-ref refs/remotes/origin/main HEAD; } >/dev/null 2>&1 || return 1
  rustty_fixture_git -C "$dir" rev-parse HEAD
}

rustty_oracle_root() {
  local checkout="$1" common="" rc=0 main_checkout
  common="$(rustty_oracle_git -C "$checkout" rev-parse --path-format=absolute \
    --git-common-dir 2>/dev/null)" || rc=$?
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
  ".port/tools/stb-resize-oracle/run.sh"
)

_rustty_oracle_self_test() {
  local here root base main_co wt oracle stale decoy decoy_oracle fakebin
  local failed=0 blocked=0 got want rel copy r
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
  blocked_arm() { # blocked_arm <label> <why>: the arm could not check (R5)
    echo "  BLOCKED $1 ($2)"
    blocked=1
  }
  # Run a command with every GIT_* variable removed, so the arms below are not
  # steered by an outer GIT_DIR (a hook, say) unless an arm sets one itself.
  clean_env() {
    # shellcheck disable=SC2046  # one `-u NAME` word pair per GIT_* variable.
    env $(compgen -e | awk '/^GIT_/ { printf "-u %s ", $0 }') "$@"
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
  for rel in .port/tools/gates/_oracle_root.sh .port/tools/gates/ORACLE_PIN \
    .port/tools/parity_names.env "${_RUSTTY_ORACLE_CONSUMERS[@]}"; do
    mkdir -p "$wt/$(dirname "$rel")"
    cp "$root/$rel" "$wt/$rel" || { echo "BLOCKED: cannot copy $rel"; return 3; }
  done

  # A pinned fixture ORACLE at the main checkout's sibling, for the read arms
  # (8, 9): D10's three stb files and one CRLF fuzz seed. The fixture checkout's
  # own ORACLE_PIN names this oracle's commit. The DECOY repository gets a
  # commit of its own (so a GIT_DIR pointing at it has a HEAD, and it is not
  # the pin), and the decoy "oracle" holds a DIFFERENT seed.
  local seed_rel="test/fuzz-libghostty/corpus/parser-initial/seed-crlf"
  local port_seed=".port/phase-0/corpora/cvt/parser-initial/seed-crlf"
  local fixture_pin decoy_head f
  mkdir -p "$oracle/src/stb" "$(dirname "$oracle/$seed_rel")" \
    "$(dirname "$decoy_oracle/$seed_rel")" "$(dirname "$wt/$port_seed")" "$decoy/src/stb"
  for f in stb.c stb_image.h stb_image_resize.h; do
    printf 'fixture oracle %s\n' "$f" >"$oracle/src/stb/$f"
  done
  printf 'a\r\nb\r\n' >"$oracle/$seed_rel"
  printf 'a\nb\n' >"$decoy_oracle/$seed_rel"
  printf 'decoy stb.c\n' >"$decoy/src/stb/stb.c"
  printf 'a\r\nb\r\n' >"$wt/$port_seed"
  printf '.port/phase-0/corpora/cvt/** -text\n' >"$wt/.gitattributes"
  if ! { clean_git -C "$oracle" init -q && clean_git -C "$oracle" add -A &&
    clean_git -C "$oracle" commit -q -m oracle &&
    clean_git -C "$decoy" add -A && clean_git -C "$decoy" commit -q -m decoy &&
    clean_git -C "$wt" add -- .gitattributes "$port_seed" &&
    clean_git -C "$wt" commit -q -m seed; } >/dev/null 2>&1; then
    echo "BLOCKED: could not build the fixture oracle"
    return 3
  fi
  fixture_pin="$(clean_git -C "$oracle" rev-parse HEAD)"
  decoy_head="$(clean_git -C "$decoy" rev-parse HEAD)"
  printf '%s\n' "$fixture_pin" >"$wt/.port/tools/gates/ORACLE_PIN"

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
    blocked_arm "agrees with _gatelib.oracle_root" "python3 not installed"
  fi

  # 3. GIT_DIR + GIT_WORK_TREE cannot redirect it.
  got="$(GIT_DIR="$decoy/.git" GIT_WORK_TREE="$decoy" rustty_oracle_root "$wt")"
  if [ "$got" = "$oracle" ]; then r=0; else r=1; fi
  arm "$r" "GIT_DIR + GIT_WORK_TREE do not redirect the oracle" "got $got, want $oracle"

  # 3b. oracle reads through rustty_oracle_git (D10's `git -C "$ORACLE" show`)
  #     are not answered by a GIT_DIR decoy.
  got="$(GIT_DIR="$decoy/.git" GIT_WORK_TREE="$decoy" \
    rustty_oracle_git -C "$main_co" rev-parse --absolute-git-dir 2>/dev/null)"
  want="$main_co/.git"
  if [ "$got" = "$want" ]; then r=0; else r=1; fi
  arm "$r" "rustty_oracle_git ignores GIT_DIR + GIT_WORK_TREE" "got $got, want $want"

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

  # 7. ONE pin. The shell and Python readers read the same file and agree on a
  #    40-hex commit, and no other file under .port/tools spells that commit.
  local sh_pin py_pin py_file others
  sh_pin="$(rustty_oracle_pin)"
  if [ -z "$sh_pin" ]; then
    arm 1 "the checked-in ORACLE_PIN is a 40-hex commit" "$_RUSTTY_ORACLE_LIB_DIR/ORACLE_PIN"
  elif command -v python3 >/dev/null 2>&1; then
    py_pin="$(python3 -c 'import sys; sys.path.insert(0, sys.argv[1]); import _gatelib as G; print(G.oracle_pin()); print(G.ORACLE_PIN_FILE)' "$here")"
    py_file="$(printf '%s\n' "$py_pin" | sed -n 2p)"
    py_pin="$(printf '%s\n' "$py_pin" | sed -n 1p)"
    if [ "$sh_pin" = "$py_pin" ] && [ "$py_file" = "$_RUSTTY_ORACLE_LIB_DIR/ORACLE_PIN" ]; then r=0; else r=1; fi
    arm "$r" "rustty_oracle_pin and _gatelib.oracle_pin read one file and agree" \
      "shell $sh_pin ($_RUSTTY_ORACLE_LIB_DIR/ORACLE_PIN), python $py_pin ($py_file)"
  else
    blocked_arm "rustty_oracle_pin and _gatelib.oracle_pin agree" "python3 not installed"
  fi
  if [ -n "$sh_pin" ]; then
    others="$(grep -rlF "$sh_pin" "$root/.port/tools" 2>/dev/null | grep -vxF "$_RUSTTY_ORACLE_LIB_DIR/ORACLE_PIN")"
    if [ -z "$others" ]; then r=0; else r=1; fi
    arm "$r" "no other .port/tools file hardcodes the pin" "$(printf '%s ' $others)"
  fi

  # 8. D10's oracle READ, end to end, under a GIT_DIR + GIT_WORK_TREE decoy (and
  #    the retired override), from a cwd outside the fixture: `--print-oracle-
  #    reads` must list the fixture oracle's three files at the fixture pin. A
  #    plain `git -C "$ORACLE_REPO"` would ask the decoy, which has no such
  #    commit (BLOCKED), and a `HEAD:` read would ignore the pin.
  copy="$wt/.port/tools/stb-resize-oracle/run.sh"
  want=""
  for f in stb.c stb_image.h stb_image_resize.h; do
    want="$want$f $(cksum <"$oracle/src/stb/$f" | tr -s ' \t' ' ')"$'\n'
  done
  want="${want%$'\n'}"
  r=0
  got="$(cd "$decoy" && clean_env GHOSTTY_ORACLE="$decoy_oracle" \
    GIT_DIR="$decoy/.git" GIT_WORK_TREE="$decoy" bash "$copy" --print-oracle-reads 2>&1)" || r=$?
  if [ "$r" = "0" ] && [ "$got" = "$want" ]; then r=0; else r=1; fi
  arm "$r" "run.sh reads the pinned fixture oracle under a GIT_DIR decoy (D10)" "got '$got'"
  printf '%s\n' "$decoy_head" >"$wt/.port/tools/gates/ORACLE_PIN"
  r=0
  got="$(cd "$decoy" && clean_env bash "$copy" --print-oracle-reads 2>&1)" || r=$?
  printf '%s\n' "$fixture_pin" >"$wt/.port/tools/gates/ORACLE_PIN"
  if [ "$r" = "3" ] && [[ "$got" == *"does not have the pin"* ]]; then r=0; else r=1; fi
  arm "$r" "run.sh is BLOCKED (3) when the oracle lacks the pin commit (D10)" "rc/out: $got"

  # 9. G-EOL end to end (the whole gate, a copy, cwd outside the fixture). Its
  #    arms 1-2 read the CHECKED repository's index, so no GIT_DIR here: that is
  #    the accepted pre-commit exception. GHOSTTY_ORACLE names the decoy oracle,
  #    whose seed differs, so an override honoured at ANY point is caught.
  copy="$wt/.port/tools/gates/check_eol_fidelity.sh"
  eol() { (cd "$decoy" && clean_env GHOSTTY_ORACLE="$decoy_oracle" bash "$copy" 2>&1); }
  r=0; got="$(eol)" || r=$?
  if [ "$r" = "0" ] && [[ "$got" == *"reference: $oracle @ ${fixture_pin:0:9}"* ]] &&
    [[ "$got" == *"ok      parser-initial (1 seed(s) byte-identical"* ]]; then r=0; else r=1; fi
  arm "$r" "G-EOL at the pin compares against the resolved oracle and is GREEN" "$(printf '%s' "$got" | tail -4 | tr '\n' '|')"
  mv "$oracle" "$oracle.away"
  r=0; got="$(eol)" || r=$?
  mv "$oracle.away" "$oracle"
  if [ "$r" = "3" ] && [[ "$got" == *"G-EOL BLOCKED"*"no oracle checkout at $oracle"* ]]; then r=0; else r=1; fi
  arm "$r" "G-EOL is BLOCKED (3), not GREEN, when the oracle is absent" "rc=$r: $(printf '%s' "$got" | tail -3 | tr '\n' '|')"
  printf '%s\n' "$decoy_head" >"$wt/.port/tools/gates/ORACLE_PIN"
  r=0; got="$(eol)" || r=$?
  printf '%s\n' "$fixture_pin" >"$wt/.port/tools/gates/ORACLE_PIN"
  if [ "$r" = "3" ] && [[ "$got" == *"not the pin $decoy_head"* ]]; then r=0; else r=1; fi
  arm "$r" "G-EOL is BLOCKED (3) when the oracle is off the pin" "$(printf '%s' "$got" | tail -3 | tr '\n' '|')"
  printf 'dirty\n' >>"$oracle/$seed_rel"
  r=0; got="$(eol)" || r=$?
  printf 'a\r\nb\r\n' >"$oracle/$seed_rel"
  if [ "$r" = "3" ] && [[ "$got" == *"differs from the pin"* ]]; then r=0; else r=1; fi
  arm "$r" "G-EOL is BLOCKED (3) when the oracle's corpus is dirty" "$(printf '%s' "$got" | tail -3 | tr '\n' '|')"
  mv "$oracle" "$oracle.away"
  printf '* text=auto\n' >"$wt/.gitattributes"
  r=0; got="$(eol)" || r=$?
  printf '.port/phase-0/corpora/cvt/** -text\n' >"$wt/.gitattributes"
  mv "$oracle.away" "$oracle"
  if [ "$r" = "1" ]; then r=0; else r=1; fi
  arm "$r" "G-EOL: a RED arm 1 wins over arm 3's BLOCKED (exit 1)" "$(printf '%s' "$got" | tail -3 | tr '\n' '|')"

  # 10. Structural scan of the REAL consumers. For each (file, oracle variable):
  #     the variable is assigned exactly once outside the lines marked
  #     `# R5-selftest-fixture` (whose number is fixed per file), and no line
  #     runs plain `git` with it — every oracle git call goes through the
  #     stripped runner. This catches a re-read override (`ORACLE="${X:-$ORACLE}"`)
  #     or a plain `git -C "$ORACLE"` anywhere in the file, not only where an
  #     end-to-end arm happens to look.
  local spec file var nmark body assigns plain marks
  for spec in \
    ".port/tools/gates/check_eol_fidelity.sh ORACLE 0" \
    ".port/tools/gates/run_differential.sh GHOSTTY 3" \
    ".port/tools/macos-parity/menu-shortcut-parity.sh GHOSTTY_ROOT 0" \
    ".port/tools/stb-resize-oracle/run.sh ORACLE_REPO 0" \
    ".port/tools/gates/check_bench_tests.sh ORACLE 0" \
    ".port/tools/gates/check_lz4_byte_exact.sh ORACLE 0" \
    ".port/tools/gates/check_rustty_vt_headers.sh GH 0"; do
    read -r file var nmark <<<"$spec"
    body="$(grep -vE '^[[:space:]]*#' "$root/$file")"
    marks="$(printf '%s\n' "$body" | grep -c 'R5-selftest-fixture')"
    assigns="$(printf '%s\n' "$body" | grep -v 'R5-selftest-fixture' |
      grep -cE "(^|[^A-Za-z0-9_\$])$var\+?=|(^|[^A-Za-z0-9_])read( -[A-Za-z]+)*( [A-Za-z_][A-Za-z0-9_]*)* $var([^A-Za-z0-9_]|\$)|\\\$\{$var:?=")"
    plain="$(printf '%s\n' "$body" | grep -E "(^|[^A-Za-z0-9_])git[[:space:]]" |
      grep -E "\\\$\{?$var([^A-Za-z0-9_]|\$)" | head -3)"
    if [ "$assigns" = "1" ] && [ "$marks" = "$nmark" ] && [ -z "$plain" ]; then r=0; else r=1; fi
    arm "$r" "$(basename "$file"): \$$var assigned once, read only through the stripped runner" \
      "assignments=$assigns (want 1), fixture-marked=$marks (want $nmark), plain git: ${plain:-none}"
  done
  body="$(grep -vE '^[[:space:]]*//' "$root/.port/tools/check-vt-struct-layouts.mjs")"
  assigns="$(printf '%s\n' "$body" | grep -cE '(^|[^A-Za-z0-9_])ORACLE[[:space:]]*=[^=]')"
  plain="$(printf '%s\n' "$body" | grep -cE '(execFileSync|execSync|spawnSync|spawn|exec)\(\s*"git"')"
  if [ "$assigns" = "1" ] && [ "$plain" = "1" ]; then r=0; else r=1; fi
  arm "$r" "check-vt-struct-layouts.mjs: ORACLE assigned once, git spawned only in oracleGit" \
    "assignments=$assigns (want 1), git spawn sites=$plain (want 1)"
  plain="$(grep -rnF --include='*.py' '"git", "-C"' "$root/.port/tools" | grep -vF "$here/_gatelib.py:")"
  others="$(grep -rnF --include='*.py' '"git", "-C"' "$here/_gatelib.py" | wc -l | tr -d ' ')"
  if [ -z "$plain" ] && [ "$others" = "1" ]; then r=0; else r=1; fi
  arm "$r" "Python: \`git -C <oracle>\` appears only in _gatelib.oracle_git" "${plain:-in _gatelib: $others}"
  plain="$(grep -rnE --include='*.py' 'cwd=ORACLE' "$root/.port/tools" | grep -v 'git_env()')"
  if [ -z "$plain" ]; then r=0; else r=1; fi
  arm "$r" "Python: every git call with cwd=ORACLE passes env=git_env()" "$plain"

  # 11. The gates that read the oracle at a FIXED path (and G4's guard) prove
  #     their own reads and BLOCKED paths on hermetic fixtures.
  for spec in "bash .port/tools/gates/run_differential.sh" \
    "bash .port/tools/gates/check_bench_tests.sh" \
    "bash .port/tools/gates/check_lz4_byte_exact.sh" \
    "sh .port/tools/gates/check_rustty_vt_headers.sh"; do
    read -r file rel <<<"$spec"
    r=0
    got="$(cd "$root" && clean_env "$file" "$rel" --self-test-oracle 2>&1)" || r=$?
    case "$r" in
      0) arm 0 "$(basename "$rel") --self-test-oracle" ;;
      3) blocked_arm "$(basename "$rel") --self-test-oracle" "$(printf '%s' "$got" | grep BLOCKED | head -1)" ;;
      *) arm 1 "$(basename "$rel") --self-test-oracle" "rc=$r: $(printf '%s' "$got" | grep -E 'FAIL|BLOCKED' | head -3 | tr '\n' '|')" ;;
    esac
  done

  if [ "$failed" -ne 0 ]; then
    echo "G-ORACLE-SH-SELF RED: an arm above did not hold."
    return 1
  fi
  if [ "$blocked" -ne 0 ]; then
    echo "G-ORACLE-SH-SELF BLOCKED: an arm above could not check (never a pass)."
    return 3
  fi
  echo "G-ORACLE-SH-SELF GREEN: the shell resolver, the pin, and every oracle reader hold."
  return 0
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  set -uo pipefail
  case "${1:-}" in
    --self-test) _rustty_oracle_self_test; exit $? ;;
    *) echo "usage: $0 --self-test   (otherwise, source this file)" >&2; exit 2 ;;
  esac
fi
