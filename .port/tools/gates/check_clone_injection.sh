#!/usr/bin/env bash
# check_clone_injection.sh — DETECTOR D8 (clone-injection audit) [ADVISORY]
#
# Spec: .port/phase-2/BUN_PORT_LEARNINGS.md §4 "D8 — clone-injection audit"
#       (and §3.4 "Clone-injection silently destroys perf", HN item 48023207,
#       the "selfmodruntime" failure mode: an agent that prefers COMPILING code
#       over FAITHFUL code silently sprinkles owning copies to escape the borrow
#       checker, and every one is a quiet perf/allocation regression the tests
#       never catch).
#
# WHAT IT DOES
#   Diff-based, per-change grep of NEWLY-ADDED owning-copy calls in the Rust
#   port —  `.clone()` | `.to_vec()` | `.to_owned()` | `.into_owned()` — over a
#   git range (default HEAD~1..HEAD; --staged for the pre-commit index). Each new
#   copy is cross-checked against the Zig side's copy behaviour and classified:
#
#     EXPLAINED  — the added copy carries an adjacent justifying comment that
#                  ties it to the reference's copy semantics: a `// PORT NOTE`
#                  citing `alloc.dupe`/`@memcpy`/`toOwnedSlice`, an `Arc`/`Rc`/
#                  `Cow` cheap-clone note, a trait-`bound`/`derive(Clone)`
#                  requirement, or a `PERF`/`SAFETY` rationale. This is exactly
#                  how the port already documents faithful copies (e.g.
#                  crates/rustty-inspector/.../key.rs "alloc.dupe maps to
#                  clone()").
#     UNEXPLAINED — a new owning copy with NO adjacent rationale. Flagged into a
#                  review-warning list. Not a proof of a bug: the reviewer must
#                  confirm the Zig side really copies here (dupe/@memcpy/value
#                  return) rather than aliasing — and, if it does not, either
#                  drop the copy or add the one-line rationale that makes it
#                  EXPLAINED next time.
#
#   The output is a REVIEW WARNING LIST (file:line, method, context), never an
#   opinion on correctness — the human review gate adjudicates. Detector D8's
#   documented hook is exactly "review-gate checklist + bench lane at tier
#   finales", so this script is ADVISORY by default: it prints and exits 0. It
#   never fails CI on its own (R5-safe: it cannot go RED and block a green port),
#   which is why it is wired into ci.sh as a PROGRESS entry that always prints
#   without failing (see ci_sh_line in this agent's report), OR run standalone.
#
# QUANTITATIVE BACKSTOP (the other half of the D8 spec)
#   Grep is the cheap qualitative net; it says WHERE a copy was injected, not
#   whether it cost anything. The quantitative backstop is the phase-0 benchmark
#   corpora:
#       .port/phase-0/baseline/BASELINE.json      (pinned perf/alloc/size/startup)
#       gate G8  check_baseline_regression.py     (the bench-lane regression gate)
#   A run prints a NOTE pointing there: every UNEXPLAINED copy is a hypothesis
#   that the bench lane at the tier finale must confirm did NOT regress the
#   pinned numbers. `--check-baseline` verifies the corpus is present so the
#   backstop is real (and warns, never fails, if it is absent).
#
# USAGE
#   check_clone_injection.sh                     # HEAD~1..HEAD, advisory (exit 0)
#   check_clone_injection.sh <git-range>         # e.g. main..HEAD, v0.1..HEAD
#   check_clone_injection.sh --since 5           # HEAD~5..HEAD
#   check_clone_injection.sh --staged            # staged (index) diff — pre-commit
#   check_clone_injection.sh --strict [range]    # exit 1 if any UNEXPLAINED
#                                                #   (for tier-finale enforcement)
#   check_clone_injection.sh --check-baseline    # assert phase-0 corpus present
#   check_clone_injection.sh --self-test         # prove the detector flips
#   check_clone_injection.sh --help
#
# EXIT CODES
#   0  advisory run completed (default), OR --strict found zero UNEXPLAINED
#   1  --strict found >=1 UNEXPLAINED copy (tier-finale enforcement only)
#   2  bad usage
#   3  BLOCKED (git range unresolvable / not a repo) — surfaced, never greened
set -uo pipefail

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TOOLS_DIR="$(cd "$GATES_DIR/.." && pwd)"
PORT_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
ROOT="$(cd "$PORT_DIR/.." && pwd)"
PY="${PYTHON:-python3}"
BASELINE_JSON="$PORT_DIR/phase-0/baseline/BASELINE.json"

RANGE=""
STAGED=0
STRICT=0
SELFTEST=0
CHECK_BASELINE=0
# Restrict the scan to the actual Rust port sources (R3: never the oracle).
PATHSPEC=('crates/' 'src/')

usage() { sed -n '2,60p' "$GATES_DIR/check_clone_injection.sh" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
  case "$1" in
    --staged)         STAGED=1 ;;
    --strict)         STRICT=1 ;;
    --self-test)      SELFTEST=1 ;;
    --check-baseline) CHECK_BASELINE=1 ;;
    --since)          shift; RANGE="HEAD~${1:?--since needs N}..HEAD" ;;
    -h|--help)        usage; exit 0 ;;
    --*)              echo "check_clone_injection.sh: unknown flag '$1'" >&2; exit 2 ;;
    *)                RANGE="$1" ;;
  esac
  shift
done

cd "$ROOT" || { echo "BLOCKED: cannot cd to repo root $ROOT" >&2; exit 3; }

# ---------------------------------------------------------------------------
# The diff analyser. Reads a unified `git diff` on stdin, tracks the +NEW line
# numbers, finds added owning-copy calls, classifies EXPLAINED/UNEXPLAINED by an
# adjacent-added-line justification window, and prints the review warning list.
# Prints "UNEXPLAINED=<n>" as its last line for the shell to read.
#
# The python program is materialised to a temp file (NOT fed via `python3 - <<`,
# which would make the heredoc itself stdin and swallow the diff).
# ---------------------------------------------------------------------------
D8_PYPROG="$(mktemp "${TMPDIR:-/tmp}/d8_analyse.XXXXXX.py")"
trap 'rm -f "$D8_PYPROG"' EXIT
cat > "$D8_PYPROG" <<'PYEOF'
import sys, re

COPY_RE = re.compile(r'\.(clone|to_vec|to_owned|into_owned)\(\)')
# An adjacent comment counts as a justification if it references the reference's
# copy behaviour or a legitimate cheap/necessary-clone rationale.
JUSTIFY_RE = re.compile(
    r'(?i)(port note|safety|perf|@memcpy|\balloc\.dupe\b|\.dupe\(|toownedslice'
    r'|\bcow\b|\barc\b|\brc<|\brc\b|cheap|required by|trait bound|\bbound\b'
    r'|derive\(clone|faithful|reference (copies|dupe|clone|owns)|zig)')

def is_comment_only(code):
    s = code.lstrip()
    return s.startswith('//') or s.startswith('/*') or s.startswith('*')

def is_attr(code):
    return code.lstrip().startswith('#[')

def main():
    cur_file = None
    newno = 0
    # Per hunk: list of (lineno, code, is_added) for added lines only, plus we
    # keep a flat record of added lines with their code for the justify window.
    added = []        # (file, lineno, code)
    hits = []         # (file, lineno, method, code, idx_into_added)

    for raw in sys.stdin.buffer.read().decode('utf-8', 'replace').splitlines():
        if raw.startswith('+++ '):
            m = re.match(r'\+\+\+ b/(.*)', raw)
            cur_file = m.group(1) if m else None
            continue
        if raw.startswith('--- ') or raw.startswith('diff --git'):
            continue
        if raw.startswith('@@'):
            m = re.search(r'\+(\d+)', raw)
            newno = int(m.group(1)) if m else 0
            continue
        if not raw:
            continue
        tag, code = raw[0], raw[1:]
        if tag == '+':
            idx = len(added)
            added.append((cur_file, newno, code))
            if not is_comment_only(code) and not is_attr(code):
                for mm in COPY_RE.finditer(code):
                    hits.append((cur_file, newno, mm.group(1), code.strip(), idx))
            newno += 1
        elif tag == '-':
            pass  # removed line: does not advance the +NEW counter
        else:
            newno += 1  # ' ' context (and any stray) advances the new counter

    # Classify: EXPLAINED if any added line within +/-2 added-lines (same file)
    # of the hit carries a justifying comment/keyword.
    WINDOW = 2
    unexplained = []
    explained = 0
    for (f, ln, meth, code, idx) in hits:
        just = False
        lo, hi = max(0, idx - WINDOW), min(len(added), idx + WINDOW + 1)
        for j in range(lo, hi):
            af, _, ac = added[j]
            if af != f:
                continue
            # A justification is a COMMENT that cites the copy rationale — never
            # bare code (e.g. an adjacent `#[derive(Clone)]` attribute or an
            # `Arc`-typed field must NOT silently excuse a to_owned()).
            if '//' not in ac:
                continue
            comment = ac[ac.index('//'):]
            if JUSTIFY_RE.search(comment):
                just = True
                break
        if just:
            explained += 1
        else:
            unexplained.append((f, ln, meth, code))

    if unexplained:
        print("  UNEXPLAINED new owning-copies (review each vs the Zig side's copy behaviour):")
        for (f, ln, meth, code) in unexplained:
            snippet = code if len(code) <= 100 else code[:97] + '...'
            print(f"    {f}:{ln}: .{meth}()   |  {snippet}")
    if explained:
        print(f"  ({explained} new owning-copy call(s) EXPLAINED by an adjacent rationale — accepted.)")
    if not hits:
        print("  (no new owning-copy calls in this diff.)")
    print(f"UNEXPLAINED={len(unexplained)}")

main()
PYEOF

analyse() { "$PY" "$D8_PYPROG"; }

# ---------------------------------------------------------------------------
run_self_test() {
  echo "=== D8 SELF-TEST (prove the detector flags an injected clone, and"
  echo "    accepts an explained one) ==="
  local tmp; tmp="$(mktemp "${TMPDIR:-/tmp}/d8_selftest.XXXXXX")"
  # A synthetic unified diff: one UNEXPLAINED .clone(), one EXPLAINED (PORT NOTE
  # citing alloc.dupe), one comment-only line (must NOT count), one attribute.
  cat > "$tmp" <<'DIFF'
diff --git a/crates/rustty-demo/src/x.rs b/crates/rustty-demo/src/x.rs
--- a/crates/rustty-demo/src/x.rs
+++ b/crates/rustty-demo/src/x.rs
@@ -10,3 +10,16 @@ impl Foo {
     fn ok(&self) {}
+        let sneaky = self.buf.clone();
+        let more = compute();
+        let stuff = other_thing();
+        // PORT NOTE: alloc.dupe maps to clone() here — faithful copy.
+        let ok = self.buf.to_vec();
+        let filler_a = a();
+        let filler_b = b();
+        #[derive(Clone)]
+        struct Bar;
+        let v = other.to_owned();
     fn tail(&self) {}
DIFF
  local out; out="$(analyse < "$tmp")"
  echo "$out"
  local u; u="$(printf '%s\n' "$out" | sed -n 's/^UNEXPLAINED=//p')"
  rm -f "$tmp"
  # Expect: sneaky.clone() UNEXPLAINED; to_vec() EXPLAINED (PORT NOTE within
  # window); to_owned() UNEXPLAINED (no rationale). => UNEXPLAINED == 2.
  if [ "$u" = "2" ]; then
    echo ">> SELF-TEST PASS: detector flagged 2 UNEXPLAINED, accepted the"
    echo "   PORT-NOTE-explained copy, ignored the comment-only line. GREEN."
    return 0
  else
    echo ">> SELF-TEST FAIL: expected UNEXPLAINED=2, got '$u'." >&2
    return 1
  fi
}

check_baseline_note() {
  echo "------------------------------------------------------------------"
  echo "QUANTITATIVE BACKSTOP (D8 spec): grep says WHERE a copy was injected;"
  echo "the phase-0 benchmark corpora say whether it COST anything."
  if [ -f "$BASELINE_JSON" ]; then
    echo "  baseline present : $BASELINE_JSON"
  else
    echo "  baseline MISSING : $BASELINE_JSON (regenerable — see"
    echo "                     .port/phase-0/baseline/run_benchmarks.py)"
  fi
  echo "  bench lane / gate: G8 check_baseline_regression.py"
  echo "  contract         : every UNEXPLAINED copy above is a hypothesis the"
  echo "                     bench lane at the tier finale must confirm did NOT"
  echo "                     regress the pinned perf/alloc/size/startup numbers."
}

# ---- --check-baseline: assert the corpus backstop is present ----------------
if [ "$CHECK_BASELINE" = "1" ]; then
  check_baseline_note
  if [ -f "$BASELINE_JSON" ]; then exit 0; else
    echo ">> WARNING: phase-0 baseline corpus absent; quantitative backstop"
    echo "   cannot bind here (advisory — not failing)." >&2
    exit 0
  fi
fi

# ---- --self-test ------------------------------------------------------------
if [ "$SELFTEST" = "1" ]; then
  run_self_test; exit $?
fi

# ---- resolve the diff -------------------------------------------------------
if ! git -C "$ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "BLOCKED: $ROOT is not a git work tree." >&2; exit 3
fi

echo "=================================================================="
echo " D8 clone-injection audit  [ADVISORY]  root: $ROOT"

get_diff() {
  if [ "$STAGED" = "1" ]; then
    git -C "$ROOT" diff --cached --unified=3 -- "${PATHSPEC[@]}"
  else
    if [ -z "$RANGE" ]; then
      if git -C "$ROOT" rev-parse HEAD~1 >/dev/null 2>&1; then
        RANGE="HEAD~1..HEAD"
      else
        RANGE="HEAD"   # root commit: diff against empty tree below
      fi
    fi
    if [ "$RANGE" = "HEAD" ]; then
      git -C "$ROOT" diff --unified=3 \
        "$(git -C "$ROOT" hash-object -t tree /dev/null)" HEAD -- "${PATHSPEC[@]}"
    else
      git -C "$ROOT" diff --unified=3 "$RANGE" -- "${PATHSPEC[@]}"
    fi
  fi
}

if [ "$STAGED" = "1" ]; then
  echo " scope: staged (index) diff"
else
  echo " scope: range '${RANGE:-HEAD~1..HEAD}'"
fi
echo "------------------------------------------------------------------"

DIFF_OUT="$(get_diff 2>/tmp/d8_giterr.$$)" || {
  echo "BLOCKED: could not compute diff for '${RANGE:-staged}':" >&2
  cat /tmp/d8_giterr.$$ >&2; rm -f /tmp/d8_giterr.$$; exit 3
}
rm -f /tmp/d8_giterr.$$

RESULT="$(printf '%s\n' "$DIFF_OUT" | analyse)"
UNEXPLAINED="$(printf '%s\n' "$RESULT" | sed -n 's/^UNEXPLAINED=//p')"
# strip the machine-readable trailer from the human output
printf '%s\n' "$RESULT" | sed '/^UNEXPLAINED=[0-9]*$/d'

echo "------------------------------------------------------------------"
echo "SUMMARY: ${UNEXPLAINED:-0} UNEXPLAINED new owning-copy call(s) flagged for review."
check_baseline_note

if [ "$STRICT" = "1" ]; then
  if [ "${UNEXPLAINED:-0}" -gt 0 ]; then
    echo ">> STRICT (tier-finale enforcement): RED — ${UNEXPLAINED} unexplained copy(ies)." >&2
    exit 1
  fi
  echo ">> STRICT: GREEN — no unexplained new owning-copies."
  exit 0
fi

echo ">> ADVISORY: review list emitted; not failing CI (D8 is advisory per-commit,"
echo "   enforced by review + the bench lane at tier finales)."
exit 0
