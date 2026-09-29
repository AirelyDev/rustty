#!/usr/bin/env bash
# P7-W11 T1 — bats-style script (no bats dependency; self-contained assertions
# so it runs the same under `bats` or invoked directly).
#
# Oracle range `5d6615fc4`, `4dfa44ecd`, `ed7f046ee`, `591ccacbd`, `a4031c8e3`
# (bash-preexec 0.7.0 + `ghostty.bash`, `.port/phase-7/delta-review/R3-lib-capi-input-core.md`
# §3.9): the bash >= 4.4 hook installer matched an array-form `PROMPT_COMMAND`
# with the literal prefix `"declare -a "` (note the trailing space). An
# exported array declared with `declare -ax PROMPT_COMMAND` renders as
# `declare -ax PROMPT_COMMAND=(...)` under `declare -p`, which does not start
# with `"declare -a "`, so the old script fell through to the *scalar* branch
# and clobbered the array with a wrapped string. The end-state script matches
# the non-anchored prefix `"declare -a"*` instead, so every `declare -a*`
# variant (`-a`, `-ax`, `-ai`, ...) is recognized as an array.
#
# This test proves the observable symptom: with `PROMPT_COMMAND` pre-declared
# as an exported array (`declare -ax`), a failing command must report exit
# status 1 through the OSC 133 `D` mark (`ESC ] 133 ; D ; <status>`).
#
# FAIL-FIRST: run against the pin's (pre-port) `rustty.bash` and this reports
# `133;D;0;` (wrong) instead of `133;D;1;`. Run against the ported end-state
# file and it reports `133;D;1;` (correct).
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
  # R14: no silent skip. Print a clear, visible skip and a non-zero-looking
  # marker in the output so a harness scanning for "PASS"/"FAIL" notices this
  # never ran, rather than mistaking silence for success.
  echo "SKIP: no bash >= 4.4 found on this host (need one to exercise the" \
    "array-PROMPT_COMMAND hook path); checked \$RUSTTY_TEST_BASH," \
    "/opt/homebrew/bin/bash, /usr/local/bin/bash, /bin/bash" >&2
  exit 0
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
