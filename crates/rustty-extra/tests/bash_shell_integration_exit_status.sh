#!/usr/bin/env bash
# P7-W11 T1 — bats-style script (no bats dependency; self-contained assertions
# so it runs the same under `bats` or invoked directly).
#
# Oracle range `5d6615fc4`, `4dfa44ecd`, `ed7f046ee`, `591ccacbd`, `a4031c8e3`
# (bash-preexec 0.7.0 + `ghostty.bash`, `.port/phase-7/delta-review/R3-lib-capi-input-core.md`
# §3.9). Two independent case groups, each run in its own bash session:
#
## Case 1: exit-status forwarding through an ARRAY PROMPT_COMMAND
#
# With an ARRAY-form `PROMPT_COMMAND` (Bash 5.1+), each array entry is
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
# This proves the observable symptom: with `PROMPT_COMMAND` pre-declared as
# an exported array (`declare -ax`), a failing command must report exit
# status 1 through the OSC 133 `D` mark (`ESC ] 133 ; D ; <status>`).
#
# FAIL-FIRST: run against the pin's (pre-port) `rustty.bash` and this reports
# `133;D;0;` (wrong) instead of `133;D;1;`. Run against the ported end-state
# file and it reports `133;D;1;` (correct). Negative control re-verified
# (P7-W11 fix round 1) by reverting `__rustty_precmd`'s `ret="${1:-$?}"` back
# to `ret="$?"` in the ported file: this test, and `cargo test -p rustty-extra`
# through it, both fail; reverting the fix undo restores both to green.
#
## Case 2/3: recognising and not clobbering a PRE-POPULATED attributed array
# (`ed7f046ee` "recognise declare -ax / attributed PROMPT_COMMAND arrays",
# `a4031c8e3` "detect our hook inside PROMPT_COMMAND arrays") — added P7-W11
# fix round 2, review finding: these two commits had no behavioural test in
# the wired gate, only the manually-run `#[ignore]`d byte-parity KAT.
#
# Setup: `declare -ax PROMPT_COMMAND=(__prior_marker_hook)` before sourcing —
# an array PROMPT_COMMAND that already has an entry and the export attribute,
# exactly the shape `ed7f046ee`'s widened `"declare -a"*` match (from the
# narrower `"declare -a "`, trailing space) exists for. On the PIN, `-z
# "${PROMPT_COMMAND[*]}"` is false (one entry already), and `declare -p`
# prints `declare -ax PROMPT_COMMAND=(...)` — which does NOT start with the
# pin's literal `"declare -a "` (space before the value, none before `x`) —
# so the pin's installer falls into the SCALAR branch. `"${PROMPT_COMMAND}"`
# without a subscript reads element 0 only, so the scalar branch's
# concatenation reads, wraps, and reassigns element 0 alone, MERGING
# `__prior_marker_hook` and the new hook invocation into one array element.
#
# Case 2 assertions (single pre-existing element), FAIL-FIRST against the
# pin, PASS against the ported end state:
#   (1) `declare -p PROMPT_COMMAND` still reports an array after sourcing.
#       NOTE: this assertion PASSES on the pin too, in this exact scenario —
#       bash's unsubscripted `arr=value` assigns element 0 without discarding
#       the variable's array-ness, so the pin's mis-handling never actually
#       strips the `-a` attribute here. It stays in this test as a permanent
#       regression guard against a DIFFERENT hypothetical failure (e.g. a
#       future edit that did `unset -v PROMPT_COMMAND; PROMPT_COMMAND=...`),
#       not as fail-first evidence for `ed7f046ee`/`a4031c8e3` — see (2)/(3).
#   (2) `__prior_marker_hook` is still an array element, BYTE-IDENTICAL to
#       before sourcing (not merged into a bigger string). FAILS on the pin
#       (element 0 becomes `"__prior_marker_hook;__rustty_hook 2>/dev/null"`,
#       not `"__prior_marker_hook"`); PASSES on the ported file (element 0 is
#       untouched, `+=("__rustty_hook 2>/dev/null")` appends a new element
#       1). This is the direct fail-first proof of `ed7f046ee`.
#   (3) Sourcing a second time does not add the rustty hook entry twice
#       (measured as: the total number of occurrences of the literal string
#       `__rustty_hook 2>/dev/null` across all of `${PROMPT_COMMAND[@]}` is
#       the same after the 2nd source as after the 1st). NOTE: this
#       assertion ALSO passes on the pin in this exact one-prior-element
#       scenario — after the first source, element 0 is exactly
#       `"__prior_marker_hook;__rustty_hook 2>/dev/null"`, and the pin's
#       hook-present guard (`";${PROMPT_COMMAND[*]}now;" == *";__rustty_hook
#       2>/dev/null;"*`) happens to find that trailing `;__rustty_hook
#       2>/dev/null;` by coincidence, because there is only one array element
#       so `${PROMPT_COMMAND[*]}` (space-joined) degenerates to that single
#       string. It stays in this test as a permanent regression guard on the
#       ported file. See Case 3 for fail-first proof of `a4031c8e3`.
#
# Case 3 (fail-first proof of `a4031c8e3`, TWO pre-existing elements —
# `declare -ax PROMPT_COMMAND=(__prior_marker_hook __second_marker_hook)`):
# with a second, untouched element after the merged one, `${PROMPT_COMMAND[*]}`
# after the first source is `"<merged elem0> __second_marker_hook"` — the
# elements are joined by a SPACE (default IFS), not a semicolon, so the pin's
# semicolon-anchored guard `*";__rustty_hook 2>/dev/null;"*` no longer finds a
# trailing `;` right after the hook invocation and re-installs it on the
# second source. FAILS on the pin (hook string appears TWICE, concatenated,
# inside element 0); PASSES on the ported file (`"${PROMPT_COMMAND[*]}" !=
# *"__rustty_hook 2>/dev/null"*` is a plain substring search, unaffected by
# element boundaries or the join character).
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

overall_status=0

fail() {
  echo "FAIL: $*" >&2
  overall_status=1
}

pass() {
  echo "PASS: $*"
}

# ---------------------------------------------------------------------------
# Case 1: exit status through an empty exported array PROMPT_COMMAND.
# ---------------------------------------------------------------------------

run_case1() {
  local target="$1"
  "$bash_bin" --noprofile --norc -i 2>&1 <<EOF
declare -ax PROMPT_COMMAND
source "$target"
false
exit
EOF
}

out="$(run_case1 "$script_under_test")"
mark="$(printf '%s' "$out" | grep -o '133;D;[0-9]*' | tail -n1)"
status="${mark##*;}"
if [[ "$status" == "1" ]]; then
  pass "declare -ax PROMPT_COMMAND (empty) + failing command reports OSC 133 D status 1 ($script_under_test)"
else
  fail "expected OSC 133 D status 1, got '${mark:-<no D mark found>}' ($script_under_test)"
  echo "--- raw output (case 1) ---" >&2
  printf '%s\n' "$out" | cat -v >&2
fi

# ---------------------------------------------------------------------------
# Cases 2/3: a pre-populated, exported (`-ax`) array PROMPT_COMMAND.
# ---------------------------------------------------------------------------

# Runs `target` twice against a bash session whose PROMPT_COMMAND starts as
# `declare -ax PROMPT_COMMAND=($init_elements)`, printing `declare -p
# PROMPT_COMMAND`'s output after each source, each tagged on one line so it
# survives being interleaved with the OSC noise the hooks themselves print:
# `{ printf 'MARK<n>='; builtin declare -p PROMPT_COMMAND; }` is a single
# compound command, so the precmd/preexec hooks only fire before and after
# the whole group, never in the middle of it.
run_array_case() {
  local target="$1" init_elements="$2"
  "$bash_bin" --noprofile --norc -i 2>&1 <<EOF
__prior_marker_hook() { :; }
__second_marker_hook() { :; }
declare -ax PROMPT_COMMAND=($init_elements)
source "$target"
{ printf 'MARK1='; builtin declare -p PROMPT_COMMAND; }
source "$target"
{ printf 'MARK2='; builtin declare -p PROMPT_COMMAND; }
exit
EOF
}

# Extracts the `declare -p PROMPT_COMMAND` line tagged `MARK<n>=` (`1` or
# `2`) out of a case's raw transcript.
extract_mark() {
  local raw="$1" n="$2" line
  line="$(printf '%s\n' "$raw" | grep -o "MARK${n}=declare.*" | tail -n1)"
  printf '%s' "${line#MARK"${n}"=}"
}

# True (0) iff `$1` (a `declare -p PROMPT_COMMAND` line) reports an array.
is_array_declare() {
  [[ "$1" == "declare -a"* ]]
}

# True (0) iff `$1` (a `declare -p PROMPT_COMMAND` line) has an array element
# whose value is EXACTLY `__prior_marker_hook`, unmodified.
has_untouched_prior_marker() {
  [[ "$1" == *'[0]="__prior_marker_hook"'* || "$1" == *'="__prior_marker_hook"'* ]]
}

# Counts occurrences of the literal hook-install string across the whole
# `declare -p` line (i.e. across every array element, and even within one
# merged element — this is what lets Case 3 see the pin double-install it).
count_hook_occurrences() {
  printf '%s' "$1" | grep -o -F '__rustty_hook 2>/dev/null' | wc -l | tr -d ' '
}

# --- Case 2: one pre-existing element ---
case2_out="$(run_array_case "$script_under_test" '__prior_marker_hook')"
case2_mark1="$(extract_mark "$case2_out" 1)"
case2_mark2="$(extract_mark "$case2_out" 2)"

if [[ -z "$case2_mark1" || -z "$case2_mark2" ]]; then
  fail "case 2: could not find both declare -p PROMPT_COMMAND marks in the transcript"
  echo "--- raw output (case 2) ---" >&2
  printf '%s\n' "$case2_out" | cat -v >&2
else
  # Assertion (1): still an array after sourcing.
  if is_array_declare "$case2_mark1"; then
    pass "case 2 (1): PROMPT_COMMAND is still an array after sourcing ($script_under_test)"
  else
    fail "case 2 (1): PROMPT_COMMAND is no longer an array after sourcing: $case2_mark1"
  fi

  # Assertion (2): the prior element survives untouched.
  if has_untouched_prior_marker "$case2_mark1"; then
    pass "case 2 (2): __prior_marker_hook survives as its own untouched array element ($script_under_test)"
  else
    fail "case 2 (2): __prior_marker_hook was not found as an untouched array element: $case2_mark1"
  fi

  # Assertion (3): sourcing twice does not add the hook twice.
  case2_count1="$(count_hook_occurrences "$case2_mark1")"
  case2_count2="$(count_hook_occurrences "$case2_mark2")"
  if [[ "$case2_count1" == "1" && "$case2_count2" == "$case2_count1" ]]; then
    pass "case 2 (3): sourcing twice installs the hook exactly once ($script_under_test)"
  else
    fail "case 2 (3): hook occurrence count changed across a second source (1st: $case2_count1, 2nd: $case2_count2): $case2_mark1 | $case2_mark2"
  fi
fi

# --- Case 3: two pre-existing elements (fail-first proof of a4031c8e3) ---
case3_out="$(run_array_case "$script_under_test" '__prior_marker_hook __second_marker_hook')"
case3_mark1="$(extract_mark "$case3_out" 1)"
case3_mark2="$(extract_mark "$case3_out" 2)"

if [[ -z "$case3_mark1" || -z "$case3_mark2" ]]; then
  fail "case 3: could not find both declare -p PROMPT_COMMAND marks in the transcript"
  echo "--- raw output (case 3) ---" >&2
  printf '%s\n' "$case3_out" | cat -v >&2
else
  case3_count1="$(count_hook_occurrences "$case3_mark1")"
  case3_count2="$(count_hook_occurrences "$case3_mark2")"
  if [[ "$case3_count1" == "1" && "$case3_count2" == "$case3_count1" ]]; then
    pass "case 3: sourcing twice with two pre-existing array elements installs the hook exactly once ($script_under_test)"
  else
    fail "case 3: hook occurrence count changed across a second source with two pre-existing elements (1st: $case3_count1, 2nd: $case3_count2): $case3_mark1 | $case3_mark2"
  fi
fi

exit "$overall_status"
