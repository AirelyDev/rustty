#!/usr/bin/env bash
# P7-W11 T1 — bats-style script (no bats dependency; self-contained assertions
# so it runs the same under `bats` or invoked directly).
#
# Oracle range `5d6615fc4`, `4dfa44ecd`, `ed7f046ee`, `591ccacbd`, `a4031c8e3`
# (bash-preexec 0.7.0 + `ghostty.bash`, `.port/phase-7/delta-review/R3-lib-capi-input-core.md`
# §3.9): with an ARRAY-form `PROMPT_COMMAND` (Bash 5.1+), each array entry is
# invoked with `$?` already set to the exit status of the command just run at
# the prompt. The pin's `__rustty_hook` captured that correctly
# (`builtin local ret=$?`), but then called `__rustty_precmd "$ret"`, and
# `__rustty_precmd` itself did `local ret="$?"` — reading `$?` again at that
# point, which is the exit status of the `local` assignment inside
# `__rustty_hook` (always 0), not the argument it was just passed. The
# forwarded status was silently discarded. The end-state script fixes this by
# having `__rustty_precmd` read `ret="${1:-$?}"` (use the argument when given)
# and `__rustty_hook` read `ret="${__rustty_status:-$?}"` (the scalar-PROMPT_COMMAND
# capture/restore path, new `__rustty_status`/`__rustty_restore_status`, or
# else the array path's own `$?`).
#
# (The end-state script also widens the hook-present guard's array match from
# the literal prefix `"declare -a "` — which a `declare -ax`-attributed array
# fails, since `declare -p` renders it `declare -ax ...` with no space before
# the `x` — to the non-anchored `"declare -a"*`. That fix matters once
# `PROMPT_COMMAND` is a NON-EMPTY array before this script runs, a scenario
# this test does not construct; it is covered by 0-diff parity against the
# oracle instead, not by this test.)
#
# This test proves the observable symptom of the `$?`-forwarding bug: with
# `PROMPT_COMMAND` pre-declared as an exported array (`declare -ax`), a
# failing command must report exit status 1 through the OSC 133 `D` mark
# (`ESC ] 133 ; D ; <status>`).
#
# FAIL-FIRST: run against the pin's (pre-port) `rustty.bash` and this reports
# `133;D;0;` (wrong) instead of `133;D;1;`. Run against the ported end-state
# file and it reports `133;D;1;` (correct). Negative control re-verified
# (P7-W11 fix round 1) by reverting `__rustty_precmd`'s `ret="${1:-$?}"` back
# to `ret="$?"` in the ported file: this test, and `cargo test -p rustty-extra`
# through it, both fail; reverting the fix undo restores both to green.
set -u

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
script_under_test="${1:-$here/../shell-integration/bash/rustty.bash}"

if [[ ! -f "$script_under_test" ]]; then
  echo "FAIL: no such script: $script_under_test" >&2
  exit 1
fi

# Find a bash >= 4.4: the hook path under test only exists on that branch
# (older bash sources bash-preexec.sh instead, a separate code path). macOS
# ships bash 3.2 as /bin/bash, so search common Homebrew locations first.
find_bash44() {
  local candidates=(
    "${RUSTTY_TEST_BASH:-}"
    /opt/homebrew/bin/bash
    /usr/local/bin/bash
    /bin/bash
  )
  local c
  for c in "${candidates[@]}"; do
    [[ -n "$c" && -x "$c" ]] || continue
    local ver
    ver="$("$c" -c 'echo "${BASH_VERSINFO[0]}.${BASH_VERSINFO[1]}"' 2>/dev/null)" || continue
    local major="${ver%%.*}" minor="${ver##*.}"
    if (( major > 4 || (major == 4 && minor >= 4) )); then
      echo "$c"
      return 0
    fi
  done
  return 1
}

bash_bin="$(find_bash44)" || {
  # R14: no silent skip. Print a clear, visible skip line and exit with the
  # automake-convention SKIP code (77) rather than 0, so a caller (the
  # `rustty-extra` `#[test]` wrapper, `crates/rustty-extra/tests/bash_shell_integration_gate.rs`)
  # can tell "this never ran" apart from "this ran and passed", and — under
  # RUSTTY_REQUIRE_BASH44=1 — turn that skip into a hard failure instead of a
  # silent green.
  echo "SKIP: no bash >= 4.4 found on this host (need one to exercise the" \
    "array-PROMPT_COMMAND hook path); checked \$RUSTTY_TEST_BASH," \
    "/opt/homebrew/bin/bash, /usr/local/bin/bash, /bin/bash" >&2
  exit 77
}

run_case() {
  local target="$1"
  "$bash_bin" --noprofile --norc -i 2>&1 <<EOF
declare -ax PROMPT_COMMAND
source "$target"
false
exit
EOF
}

out="$(run_case "$script_under_test")"

# Pull the status field out of the OSC 133 D mark that follows the `false`
# invocation: ESC ] 133 ; D ; <status> ; aid=<pid> BEL
mark="$(printf '%s' "$out" | grep -o '133;D;[0-9]*' | tail -n1)"
status="${mark##*;}"

if [[ "$status" == "1" ]]; then
  echo "PASS: declare -ax PROMPT_COMMAND + failing command reports OSC 133 D status 1 ($script_under_test)"
  exit 0
else
  echo "FAIL: expected OSC 133 D status 1, got '${mark:-<no D mark found>}' ($script_under_test)" >&2
  echo "--- raw output ---" >&2
  printf '%s\n' "$out" | cat -v >&2
  exit 1
fi
