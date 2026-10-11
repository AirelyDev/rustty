#!/usr/bin/env bash
# check_perf_fence.sh — GATE G8b (paired same-session perf fence) [R5]
#
# DoP id:  G8b (companion to G8; DEFINITION_OF_PARITY §1 perf axis)
# Reads:   the pinned oracle bench binary (~/rustty-oracle-artifacts or
#          $RUSTTY_ORACLE_ARTIFACTS, checked
#          against ws10_suite.py's ORACLE_SHA256) + the four corpora (BOTH
#          UNTRACKED; the corpora live in the shared checkout only), a candidate
#          rustty-bench, and the stored entry binary + its manifest in the MAIN
#          checkout's .perf-bins/ (see "THE ENTRY BINARY" below).
# Pass:    rustty beats the pinned oracle on all 14 configs (Fence 1) and has
#          not self-regressed against the entry binary (Fence 2).
#
# WHY G8b EXISTS ALONGSIDE G8
# ---------------------------
# G8 compares rustty against the ORACLE's frozen June numbers with a +/-7% band.
# Because rustty is 7-60% FASTER than the oracle, G8 spends that entire margin
# as headroom: measured on 2026-07-30, G8 would not fire until rustty got
#   terminal-parser      60.4% slower
#   screen-clone.clone  110.6% slower
#   terminal-stream      40.3% slower
# than it is today. G8 is therefore an ORACLE-PARITY fence, not a regression
# fence, and fixing its (id, optimize) indexing bug in WS10 T2 did not change
# that. G8b is the real regression fence: it re-measures a STORED BINARY in the
# same process as the candidate, which removes session variance (the same
# unchanged oracle binary spans 11.8% across sessions) and lets the band be +5%.
#
# THE ENTRY BINARY (Fence 2's baseline; CI hygiene E)
# ---------------------------------------------------
# Until CI hygiene E, Fence 2 had never run in any checkout. Three causes:
#   - ENTRY was the RELATIVE path .perf-bins/..., so from a linked worktree it
#     named that worktree's own (git-ignored, never created) .perf-bins/.
#   - Nothing stored the entry binary or said which commit it came from (WS10
#     stashed it by hand).
#   - With no entry binary G8b printed a NOTE and could go GREEN on Fence 1
#     alone, so the absence was invisible in a ci.sh verdict.
# Now:
#   - The entry lives in the MAIN checkout's .perf-bins/, derived by
#     _gatelib.main_checkout (git's common dir), the same helper ws10_suite.py
#     uses for the corpora, so the gate and the suite agree from any worktree.
#     --self PATH is still an explicit override (its manifest is PATH.json).
#   - `--store-entry` builds (or with --no-build reuses) the candidate, copies it
#     there and writes rustty-bench-ws10-entry-RF.json beside it: commit, clean/
#     dirty, sha256, UTC date, build command, rustc version. It refuses a dirty
#     tree unless --allow-dirty, prints the manifest it replaces, and never
#     measures. Policy (perf README): store or refresh the entry only ON
#     PURPOSE, at a phase close-out after a GREEN perf verdict, so the next
#     cycle is fenced against it. Never automatically, and never from ci.sh.
#   - Before measuring, Fence 2's entry is checked against its manifest and the
#     manifest is printed, so the output says what the candidate was compared
#     against. A binary whose sha256 is not its manifest's is RED (the stored
#     binary is not what it claims to be); a binary with no (readable)
#     manifest is BLOCKED ("re-store it"); no binary is BLOCKED.
#   - When Fence 2 cannot run, Fence 1 still runs and --emit-metrics is still
#     written (G8 keeps its input), but the verdict is never GREEN.
#
# EXIT CODES (deliberately four-valued)
#   0 GREEN    both fences held
#   1 RED      real regression (Fence 1 or Fence 2), a failed build, or a
#              Fence 2 entry whose sha256 does not match its manifest. Any RED
#              wins over any BLOCKED: Fence 1 RED with no entry is exit 1.
#   2 RED      ws10_suite.py's own exit 2: a measurement that turned out
#              provisional (load rose above IDLE_MAX while it ran), or an
#              argparse usage error. ci.sh reads it as RED (only 0 and 3 are
#              not), and it stays RED: a fence verdict is never overridden.
#   3 BLOCKED  the dedicated PRE-MEASUREMENT load check (load_check below):
#              the 1-minute load stayed above IDLE_MAX through a bounded
#              cooldown (G8B_COOLDOWN_S, default 600 s), checked before G8b's
#              own builds and again before measuring. Nothing was measured, so
#              no metrics file exists for G8. Its own reason line says so.
#   other      a crash or a kill of the harness: RED, never BLOCKED.
#   3 BLOCKED  measurement substrate absent (oracle binary / corpora missing --
#              they are untracked: the corpora live only in the main checkout,
#              which ws10_suite.py derives from git so a linked worktree finds
#              them too, and the oracle binary in ~/rustty-oracle-artifacts or
#              $RUSTTY_ORACLE_ARTIFACTS; so this is the normal state on a fresh
#              clone or on CI).
#   3 BLOCKED  Fence 1 held but Fence 2 did not run: no entry binary, or one
#              with no readable manifest. Fence 1 was measured and the metrics
#              file was written; the reason line says how to store an entry.
#
# Usage:
#   check_perf_fence.sh [--build] [--rust PATH] [--self PATH]
#                       [--emit-metrics PATH] [-n N]
#   check_perf_fence.sh --store-entry [--no-build] [--allow-dirty] [--rust PATH]
#                       (store the candidate as Fence 2's entry; no measurement)
#   check_perf_fence.sh --self-test   (G8b-SELF: the exit mapping, against a
#                       fake harness; no oracle, no measurement)
#
# --build (what ci.sh passes; P7-W22 review): once the substrate is known to be
# present, run every size label's `rustty_build` from BASELINE.json (ws10_suite
# --print-size-builds: the bench, rustty-gen and the vt-c release libraries, the
# perf README's list) at the repo root, so a fresh tree measures fresh artifacts
# and a stale one is never measured silently. CARGO_TARGET_DIR is unset for
# those builds because the counterparts are measured under <root>/target. A
# failed build is RED (exit 1). CI_SKIP_CARGO=1 skips them with a NOTE.
# --store-entry runs the same builds (unless --no-build), with no substrate or
# load check: it measures nothing.
# --emit-metrics PATH is removed first, so a BLOCKED run leaves no earlier run's
# file behind for G8 to read.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
SUITE="$HERE/../perf/ws10_suite.py"
RUST="${RUSTTY_BENCH_BIN:-$ROOT/target/release-fast/rustty-bench}"
ENTRY_NAME="rustty-bench-ws10-entry-RF"
ENTRY=""
BUILD=0
STORE=0
NO_BUILD=0
ALLOW_DIRTY=0
EMIT=""
ARGS=()

# ---- the pre-measurement load check (the only BLOCKED for load) ------------
# IDLE_MAX is ws10_suite.py's own threshold, read from it (one source).
G8B_COOLDOWN_S="${G8B_COOLDOWN_S:-600}"
G8B_COOLDOWN_STEP_S="${G8B_COOLDOWN_STEP_S:-15}"
load1() { python3 -c 'import os; print("%.2f" % os.getloadavg()[0])'; }
load_check() {  # load_check PHASE — returns when idle, exits 3 after the cooldown
  local phase="$1" waited=0 l idle
  idle="$(sed -n 's/^IDLE_MAX = \([0-9.]*\).*/\1/p' "$SUITE")"
  [ -n "$idle" ] || { echo "RED: cannot read IDLE_MAX from $SUITE"; exit 1; }
  while :; do
    l="$(load1)"
    if python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) <= float(sys.argv[2]) else 1)' "$l" "$idle"; then
      echo "load check ($phase): 1-minute load $l <= IDLE_MAX $idle (after ${waited}s)"
      return 0
    fi
    if [ "$waited" -ge "$G8B_COOLDOWN_S" ]; then
      echo "BLOCKED: G8b pre-measurement load check ($phase): 1-minute load $l > IDLE_MAX $idle after a ${waited}s cooldown; nothing was measured"
      exit 3
    fi
    sleep "$G8B_COOLDOWN_STEP_S"; waited=$((waited + G8B_COOLDOWN_STEP_S))
  done
}

# ---- the entry binary's helpers (one Python, _gatelib's git rules) ----------
# entry_py main ROOT          -> the main checkout of ROOT (_gatelib.main_checkout)
# entry_py state ROOT         -> "clean <sha>" | "dirty <sha>" | "unknown -"
# entry_py verify ENTRY       -> prints the manifest; exit 0 ok, 1 sha mismatch,
#                                4 no / unreadable manifest
# entry_py show ENTRY         -> prints the manifest a store would replace
# entry_py store SRC ENTRY ROOT BUILD_CMD BUILT
#                             -> copies SRC to ENTRY and writes ENTRY.json
# git runs with every GIT_* variable removed (_gatelib.git_env, R5), and
# `status --untracked-files=no`: an untracked file is not compiled unless a
# tracked file names it, and the main checkout always carries untracked notes.
entry_py() {
  python3 - "$HERE" "$@" <<'PY'
import datetime, hashlib, json, os, shutil, subprocess, sys
sys.path.insert(0, sys.argv[1])
import _gatelib
cmd, a = sys.argv[2], sys.argv[3:]

def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()

def git(root, *args):
    try:
        r = subprocess.run(["git", *args], cwd=root, capture_output=True,
                           text=True, env=_gatelib.git_env())
    except OSError:
        return None
    return r.stdout.strip() if r.returncode == 0 else None

def state(root):
    head = git(root, "rev-parse", "HEAD")
    st = git(root, "status", "--porcelain", "--untracked-files=no")
    if head is None or st is None:
        return "unknown", "-"
    return ("dirty" if st else "clean"), head

def describe(m):
    return (f"commit {m.get('commit', '?')} "
            f"({'DIRTY' if m.get('dirty') else 'clean'}), stored "
            f"{m.get('date_utc', '?')}, sha256 {str(m.get('sha256', '?'))[:12]}..., "
            f"built by `{m.get('build_command', '?')}` "
            f"({'built' if m.get('built') else 'reused'}), {m.get('rustc', '?')}")

def load(man):
    try:
        with open(man, encoding="utf-8") as fh:
            m = json.load(fh)
        return m if isinstance(m, dict) and m.get("sha256") else None
    except (OSError, ValueError):
        return None

if cmd == "main":
    print(_gatelib.main_checkout(a[0]))
elif cmd == "state":
    print(*state(a[0]))
elif cmd == "verify":
    entry = a[0]
    man = entry + ".json"
    if not os.path.isfile(man):
        print(f"no manifest {man}")
        sys.exit(4)
    m = load(man)
    if m is None:
        print(f"unreadable manifest {man} (not a JSON object with a sha256)")
        sys.exit(4)
    print(f"Fence 2 entry: {entry}")
    print(f"   manifest: {describe(m)}")
    got, want = sha256(entry), m["sha256"]
    if got != want:
        print(f"   sha256 of the binary is {got}, the manifest says {want}")
        sys.exit(1)
elif cmd == "show":
    entry = a[0]
    man = entry + ".json"
    m = load(man) if os.path.isfile(man) else None
    if m is not None:
        print(f"previous manifest ({man}):")
        print(f"   {describe(m)}")
    elif os.path.exists(entry):
        print(f"previous entry {entry} had no readable manifest")
    else:
        print(f"no previous entry at {entry}")
elif cmd == "store":
    src, entry, root, build_cmd, built = a
    d = os.path.dirname(entry)
    os.makedirs(d, exist_ok=True)
    clean, head = state(root)
    try:
        rustc = subprocess.run(["rustc", "--version"], cwd=root,
                               capture_output=True, text=True).stdout.strip()
    except OSError:
        rustc = ""
    m = {
        "entry": os.path.basename(entry),
        "commit": head,
        "dirty": clean != "clean",
        "tree_state": clean,
        "sha256": sha256(src),
        "date_utc": datetime.datetime.now(datetime.timezone.utc)
                    .strftime("%Y-%m-%dT%H:%M:%SZ"),
        "build_command": build_cmd,
        "built": built == "1",
        "source": os.path.relpath(os.path.abspath(src), root),
        "rustc": rustc or "unknown",
    }
    # Temp files in the same directory, then rename: a reader sees the old
    # pair, no manifest (BLOCKED), or the new pair; never a half-copied binary.
    tb = os.path.join(d, f".{os.path.basename(entry)}.tmp.{os.getpid()}")
    tm = tb + ".json"
    shutil.copyfile(src, tb)
    os.chmod(tb, 0o755)
    if sha256(tb) != m["sha256"]:
        os.unlink(tb)
        sys.exit(f"RED: the copy of {src} does not hash like its source")
    with open(tm, "w", encoding="utf-8") as fh:
        json.dump(m, fh, indent=2)
        fh.write("\n")
    if os.path.exists(entry + ".json"):
        os.unlink(entry + ".json")
    os.replace(tb, entry)
    os.replace(tm, entry + ".json")
    print(f"stored {entry}")
    print(f"   {describe(m)}")
else:
    sys.exit(f"entry_py: unknown command {cmd}")
PY
}

# The size labels' builds (--build, and --store-entry without --no-build).
run_size_builds() {
  local builds cmd argv
  if [ "${CI_SKIP_CARGO:-0}" = "1" ]; then
    echo "NOTE: CI_SKIP_CARGO=1 -> size counterparts NOT rebuilt; measuring what is on disk"
    return 0
  fi
  builds="$(python3 "$SUITE" --print-size-builds)"
  while IFS= read -r cmd; do
    [ -n "$cmd" ] || continue
    read -r -a argv <<<"$cmd"
    if [ "${argv[0]}" != "cargo" ] || [ "${argv[1]:-}" != "build" ]; then
      echo "RED: BASELINE.json rustty_build is not a cargo build: $cmd"
      exit 1
    fi
    echo "+ $cmd"
    if ! (cd "$ROOT" && env -u CARGO_TARGET_DIR "${argv[@]}"); then
      echo "RED: size counterpart build failed: $cmd"
      exit 1
    fi
  done <<<"$builds"
}

# ---- --self-test (G8b-SELF) -------------------------------------------------
# Copies of this script against a fake ws10_suite.py (preflight and builds no-op,
# then crash / usage error / fence RED / green as asked) and a stubbed load1.
# A crash and a usage error stay RED, a fence RED stays RED, only the load
# check is BLOCKED (and measures nothing). Control: the reviewed blanket remap
# (`*) ... exit 3`) turns the crash BLOCKED, and the crash arm notices.
# CI hygiene E adds the entry binary's arms, each with a control that reverts
# its fix in a mutated copy and must turn the arm's verdict:
#   - entry found in the MAIN checkout from a linked worktree (control: the
#     root-relative lookup it replaced, which BLOCKs there);
#   - no entry -> BLOCKED with the metrics file still written (control: the
#     old silent narrowing, exit 0);
#   - sha256 mismatch -> RED (control: no comparison, exit 0);
#   - no manifest -> BLOCKED (control: the old "any binary will do", exit 0);
#   - --store-entry writes binary + manifest that the fence then accepts,
#     refuses a dirty tree, takes --allow-dirty (controls: no refusal; no
#     --store-entry mode);
#   - Fence 1 RED with no entry -> exit 1 (control: BLOCKED overriding RED).
# The mutations below are literal source text, hence single quotes.
# shellcheck disable=SC2016
self_test() {
  local st fail=0 mode want before
  set +e   # the arms run copies that exit non-zero on purpose
  st="$(mktemp -d "${TMPDIR:-/tmp}/g8bself.XXXXXX")"
  st="$(cd "$st" && pwd -P)"   # git reports /private/var, not /var, on macOS
  g() {  # git with no GIT_* redirection, hooks or signing from the host
    env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE -u GIT_COMMON_DIR \
      git -c user.name=g8bself -c user.email=g8bself@invalid -c commit.gpgsign=false \
      -c core.hooksPath=/dev/null -c init.defaultBranch=main "$@"
  }
  mk() {  # mk DIR [MUTATION-OLD MUTATION-NEW]
    mkdir -p "$1/.port/tools/gates" "$1/.port/tools/perf"
    cp "$HERE/_gatelib.py" "$1/.port/tools/gates/_gatelib.py"
    python3 - "$HERE/check_perf_fence.sh" "$1/.port/tools/gates/check_perf_fence.sh" "${2:-}" "${3:-}" <<'PY'
import sys
s = open(sys.argv[1]).read()
old = "load1() { python3 -c 'import os; print(\"%.2f\" % os.getloadavg()[0])'; }"
assert s.count(old) == 1
s = s.replace(old, 'load1() { cat "$ROOT/load"; }')
if sys.argv[3]:
    # Mutate the gate's own code, never this self-test's text.
    a = s.index("\n# ---- --self-test (G8b-SELF)")
    b = s.rindex('if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi')
    rest = s[:a] + s[b:]
    assert rest.count(sys.argv[3]) == 1, sys.argv[3]
    s = s[:a].replace(sys.argv[3], sys.argv[4]) + s[a:b] + s[b:].replace(sys.argv[3], sys.argv[4])
open(sys.argv[2], "w").write(s)
PY
    # The fake harness records its argv (so an arm can see whether Fence 2
    # was passed an entry) and writes --emit-metrics as Fence 1 would.
    cat > "$1/.port/tools/perf/ws10_suite.py" <<'PY'
import os, sys
root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
IDLE_MAX = 2.0
if "--print-size-builds" in sys.argv:
    print("cargo build -p rustty-bench --profile release-fast --bin rustty-bench")
    sys.exit(0)
if "--preflight" in sys.argv:
    sys.exit(0)
open(os.path.join(root, "measured"), "a").write(" ".join(sys.argv[1:]) + "\n")
if "--emit-metrics" in sys.argv:
    open(sys.argv[sys.argv.index("--emit-metrics") + 1], "w").write("{}\n")
mode = open(os.path.join(root, "mode")).read().strip()
if mode == "crash":
    os.kill(os.getpid(), 11)
if mode == "usage":
    print("usage: ws10_suite.py [-h]\nws10_suite.py: error: unrecognized arguments", file=sys.stderr); sys.exit(2)
sys.exit(1 if mode == "red" else 0)
PY
    printf '#!/bin/sh\nexit 0\n' > "$1/bench"; chmod +x "$1/bench"
    printf 'tracked\n' > "$1/tracked.txt"
  }
  seed() {  # seed MAIN-DIR -> a valid entry + manifest, as a store would leave
    mkdir -p "$1/.perf-bins"
    cp "$1/bench" "$1/.perf-bins/$ENTRY_NAME"
    python3 - "$1/.perf-bins/$ENTRY_NAME" <<'PY'
import hashlib, json, sys
p = sys.argv[1]
json.dump({"commit": "seeded", "dirty": False, "date_utc": "2026-10-10T00:00:00Z",
           "sha256": hashlib.sha256(open(p, "rb").read()).hexdigest(),
           "build_command": "seeded by G8b-SELF", "built": False, "rustc": "n/a"},
          open(p + ".json", "w"))
PY
  }
  mkrepo() {  # mkrepo NAME [MUTATION-OLD MUTATION-NEW] -> $st/NAME + worktree $st/NAME-wt
    mk "$st/$1" "${2:-}" "${3:-}"
    g -C "$st/$1" init -q && g -C "$st/$1" add -A && g -C "$st/$1" commit -qm fixture \
      && g -C "$st/$1" worktree add -q "$st/$1-wt" 2>/dev/null
  }
  gate() {  # gate DIR ARGS... -> rc in RC, output in OUT
    local d="$1"; shift
    OUT="$(CI_SKIP_CARGO=1 G8B_COOLDOWN_S=0 bash "$d/.port/tools/gates/check_perf_fence.sh" "$@" 2>&1)"; RC=$?
  }
  run() {  # run DIR MODE LOAD -> rc in RC, output in OUT
    printf '%s\n' "$2" > "$1/mode"; printf '%s\n' "$3" > "$1/load"; : > "$1/measured"
    gate "$1" --build --rust "$1/bench" ${G8B_ST_ARGS:-}
  }
  ran_f2() { grep -q -- "--self $1/.perf-bins/$ENTRY_NAME" "$2/measured"; }  # ran_f2 MAIN WT
  man() { python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get(sys.argv[2]))' "$1/.perf-bins/$ENTRY_NAME.json" "$2"; }
  ok() { echo "  ok    $*"; }
  bad() { echo "  FAIL  $* (exit $RC)"; printf '%s\n' "$OUT" | tail -4 | sed 's/^/        /'; fail=1; }

  mk "$st/base"; seed "$st/base"
  for mode in green:0 red:1 usage:2 crash:139; do
    want="${mode#*:}"; mode="${mode%%:*}"
    run "$st/base" "$mode" 0.10
    if [ "$RC" = "$want" ] && ran_f2 "$st/base" "$st/base"; then ok "harness $mode -> exit $RC ($( [ "$RC" = 0 ] && echo green || echo RED in ci.sh)), Fence 2 given the entry"
    else bad "harness $mode -> want $want with Fence 2 run"; fi
  done
  G8B_ST_ARGS="--emit-metrics $st/base/m.json" run "$st/base" green 0.10
  if [ "$RC" = 0 ]; then ok "with --emit-metrics (as ci.sh runs it) -> exit 0"; else bad "with --emit-metrics"; fi
  run "$st/base" green 9.90
  if [ "$RC" = 3 ] && printf '%s\n' "$OUT" | grep -q "BLOCKED: G8b pre-measurement load check" && [ ! -s "$st/base/measured" ]; then
    ok "loaded host -> exit 3 (pre-measurement load check), nothing measured"
  else bad "loaded host"; fi
  mk "$st/remap" '  *) echo "RED: unexpected harness exit $rc (a crash or a kill)" ;;' '  *) echo "BLOCKED: unexpected harness exit $rc"; exit 3 ;;'
  seed "$st/remap"; run "$st/remap" crash 0.10
  if [ "$RC" = 3 ]; then ok "control: the blanket remap turns a harness crash into BLOCKED (exit 3); the crash arm above requires RED"
  else bad "control: the remapped copy did not turn the crash BLOCKED"; fi

  # -- the entry is found in the MAIN checkout from a linked worktree --------
  mkrepo wt; seed "$st/wt"
  run "$st/wt-wt" green 0.10
  if [ "$RC" = 0 ] && ran_f2 "$st/wt" "$st/wt-wt" && printf '%s\n' "$OUT" | grep -q "manifest: commit seeded"; then
    ok "linked worktree -> entry resolved in the main checkout, manifest printed, exit 0"
  else bad "linked worktree -> want exit 0 with the main checkout's entry"; fi
  mkrepo wtc 'MAIN="$(entry_py main "$ROOT")"' 'MAIN="$ROOT"'; seed "$st/wtc"
  run "$st/wtc-wt" green 0.10
  if [ "$RC" = 3 ]; then ok "control: the worktree-relative lookup misses the main checkout's entry (exit 3); the arm above requires 0"
  else bad "control: the worktree-relative copy still found the entry"; fi

  # -- no entry: BLOCKED, Fence 1 measured, metrics still written ------------
  mk "$st/none"
  G8B_ST_ARGS="--emit-metrics $st/none/m.json" run "$st/none" green 0.10
  if [ "$RC" = 3 ] && [ -s "$st/none/m.json" ] && printf '%s\n' "$OUT" | grep -q "BLOCKED: Fence 2 did not run" \
     && printf '%s\n' "$OUT" | grep -q -- "--store-entry" && ! grep -q -- "--self" "$st/none/measured"; then
    ok "no entry -> exit 3 (Fence 2 did not run, how to store one), Fence 1 measured, metrics written"
  else bad "no entry -> want exit 3 with metrics"; fi
  mk "$st/nonec" '  if [ "$rc" -eq 0 ]; then rc=3; fi' '  :'
  run "$st/nonec" green 0.10
  if [ "$RC" = 0 ]; then ok "control: the old silent narrowing is GREEN on Fence 1 alone (exit 0); the arm above requires 3"
  else bad "control: the narrowing copy was not GREEN"; fi
  # -- Fence 1 RED with no entry: RED wins ----------------------------------
  run "$st/none" red 0.10
  if [ "$RC" = 1 ]; then ok "Fence 1 RED with no entry -> exit 1 (RED wins over BLOCKED)"
  else bad "Fence 1 RED with no entry -> want 1"; fi
  mk "$st/redc" '  if [ "$rc" -eq 0 ]; then rc=3; fi' '  rc=3'
  run "$st/redc" red 0.10
  if [ "$RC" = 3 ]; then ok "control: BLOCKED overriding the verdict turns Fence 1 RED into exit 3; the arm above requires 1"
  else bad "control: the overriding copy did not exit 3"; fi

  # -- sha256 mismatch: RED ---------------------------------------------------
  mk "$st/sha"; seed "$st/sha"; printf '# tampered\n' >> "$st/sha/.perf-bins/$ENTRY_NAME"
  run "$st/sha" green 0.10
  if [ "$RC" = 1 ] && printf '%s\n' "$OUT" | grep -q "RED: Fence 2's entry is not what its manifest claims" && ! grep -q -- "--self" "$st/sha/measured"; then
    ok "entry sha256 != manifest -> exit 1, Fence 2 not run against it"
  else bad "sha256 mismatch -> want 1"; fi
  mk "$st/shac" '    if got != want:' '    if False:'; seed "$st/shac"; printf '# tampered\n' >> "$st/shac/.perf-bins/$ENTRY_NAME"
  run "$st/shac" green 0.10
  if [ "$RC" = 0 ]; then ok "control: without the comparison the tampered entry passes (exit 0); the arm above requires 1"
  else bad "control: the uncompared copy did not pass"; fi

  # -- no manifest: BLOCKED ---------------------------------------------------
  mk "$st/nom"; seed "$st/nom"; rm -f "$st/nom/.perf-bins/$ENTRY_NAME.json"
  run "$st/nom" green 0.10
  if [ "$RC" = 3 ] && printf '%s\n' "$OUT" | grep -q "BLOCKED: Fence 2 did not run.*re-store it"; then
    ok "entry with no manifest -> exit 3 (re-store it)"
  else bad "no manifest -> want 3"; fi
  mk "$st/nomc" '    4) F2_STATE=BLOCKED' '    4) ARGS+=("--self" "$ENTRY"); F2_STATE=""'; seed "$st/nomc"; rm -f "$st/nomc/.perf-bins/$ENTRY_NAME.json"
  run "$st/nomc" green 0.10
  if [ "$RC" = 0 ]; then ok "control: accepting any binary passes the manifest-less entry (exit 0); the arm above requires 3"
  else bad "control: the any-binary copy did not pass"; fi

  # -- --store-entry: writes, refuses dirty, takes --allow-dirty -------------
  mkrepo store; seed "$st/store"
  gate "$st/store-wt" --store-entry --no-build --rust "$st/store-wt/bench"
  if [ "$RC" = 0 ] && [ -x "$st/store/.perf-bins/$ENTRY_NAME" ] && printf '%s\n' "$OUT" | grep -q "previous manifest" \
     && [ "$(man "$st/store" commit)" = "$(g -C "$st/store-wt" rev-parse HEAD)" ] && [ "$(man "$st/store" dirty)" = False ] \
     && ! [ -e "$st/store-wt/.perf-bins" ] && [ ! -e "$st/store-wt/measured" ]; then
    ok "--store-entry from a worktree -> main checkout's .perf-bins/ (binary + manifest, HEAD, clean), previous manifest printed, nothing measured"
  else bad "--store-entry -> want the entry stored in the main checkout"; fi
  run "$st/store-wt" green 0.10
  if [ "$RC" = 0 ] && ran_f2 "$st/store" "$st/store-wt"; then ok "a stored entry is accepted by the fence (exit 0, Fence 2 run)"
  else bad "the stored entry was not accepted"; fi
  printf 'edit\n' >> "$st/store-wt/tracked.txt"; before="$(cksum < "$st/store/.perf-bins/$ENTRY_NAME.json")"
  gate "$st/store-wt" --store-entry --no-build --rust "$st/store-wt/bench"
  if [ "$RC" = 1 ] && printf '%s\n' "$OUT" | grep -q "REFUSED" && [ "$(cksum < "$st/store/.perf-bins/$ENTRY_NAME.json")" = "$before" ]; then
    ok "--store-entry on a dirty tree -> refused (exit 1), entry untouched"
  else bad "--store-entry dirty -> want refused"; fi
  gate "$st/store-wt" --store-entry --no-build --allow-dirty --rust "$st/store-wt/bench"
  if [ "$RC" = 0 ] && [ "$(man "$st/store" dirty)" = True ]; then ok "--store-entry --allow-dirty -> stored, manifest says dirty"
  else bad "--allow-dirty -> want stored with dirty=true"; fi
  mkrepo storec 'if [ "$ALLOW_DIRTY" = 0 ]; then' 'if false; then'; seed "$st/storec"
  printf 'edit\n' >> "$st/storec-wt/tracked.txt"
  gate "$st/storec-wt" --store-entry --no-build --rust "$st/storec-wt/bench"
  if [ "$RC" = 0 ]; then ok "control: without the refusal a dirty tree is stored (exit 0); the dirty arm above requires 1"
  else bad "control: the no-refusal copy still refused"; fi
  mkrepo stores '    --store-entry) STORE=1; shift ;;' '    --store-entry-off) STORE=1; shift ;;'; seed "$st/stores"
  printf 'green\n' > "$st/stores-wt/mode"; printf '0.10\n' > "$st/stores-wt/load"
  gate "$st/stores-wt" --store-entry --no-build --rust "$st/stores-wt/bench"
  if [ "$(man "$st/stores" commit)" = seeded ]; then ok "control: with no store mode the entry is never written (manifest still the seed); the store arm above requires HEAD"
  else bad "control: the no-store copy wrote a manifest"; fi

  if [ "$fail" = 0 ]; then echo "G8b-SELF GREEN"; return 0; fi
  echo "G8b-SELF RED (fixtures in $st)"; return 1
}
if [ "${1:-}" = "--self-test" ]; then self_test; exit $?; fi

while [ $# -gt 0 ]; do
  case "$1" in
    --rust) RUST="$2"; shift 2 ;;
    --self) ENTRY="$2"; shift 2 ;;
    --build) BUILD=1; shift ;;
    --store-entry) STORE=1; shift ;;
    --no-build) NO_BUILD=1; shift ;;
    --allow-dirty) ALLOW_DIRTY=1; shift ;;
    --emit-metrics) EMIT="$2"; ARGS+=("$1" "$2"); shift 2 ;;
    *) ARGS+=("$1"); shift ;;
  esac
done
if [ "$STORE" = 0 ] && { [ "$NO_BUILD" = 1 ] || [ "$ALLOW_DIRTY" = 1 ]; }; then
  echo "RED (usage): --no-build and --allow-dirty only apply to --store-entry"
  exit 2
fi

# The entry lives in the MAIN checkout (see "THE ENTRY BINARY"); --self wins.
MAIN="$(entry_py main "$ROOT")"
if [ -z "$ENTRY" ]; then
  ENTRY="$MAIN/.perf-bins/$ENTRY_NAME"
fi

# ---- --store-entry: store only, never measure --------------------------------
if [ "$STORE" = 1 ]; then
  echo "=== G8b --store-entry :: store the candidate as Fence 2's entry (no measurement) ==="
  read -r tree head <<<"$(entry_py state "$ROOT")"
  echo "tree: $ROOT at $head ($tree)"
  if [ "$tree" != clean ]; then
    if [ "$ALLOW_DIRTY" = 0 ]; then
      echo "REFUSED: the tree is $tree (git status --untracked-files=no), so the manifest's commit would not name what was built."
      echo "   commit first, or pass --allow-dirty to store it marked dirty"
      exit 1
    fi
    echo "NOTE: --allow-dirty: storing from a $tree tree; the manifest records it"
  fi
  built=0
  if [ "$NO_BUILD" = 0 ]; then
    run_size_builds
    [ "${CI_SKIP_CARGO:-0}" = "1" ] || built=1
  fi
  if [ ! -x "$RUST" ]; then
    echo "RED: candidate bench binary not found: $RUST"
    echo "   build it with: cargo build -p rustty-bench --profile release-fast (or drop --no-build)"
    exit 1
  fi
  bench_cmd="$(python3 "$SUITE" --print-size-builds | grep -e '-p rustty-bench ' || true)"
  entry_py show "$ENTRY"
  entry_py store "$RUST" "$ENTRY" "$ROOT" "${bench_cmd:-unknown}" "$built"
  exit 0
fi

echo "=== G8b :: paired same-session perf fence (the pinned oracle only) ==="

if [ -n "$EMIT" ]; then
  rm -f -- "$EMIT"
fi

if [ "$BUILD" = 1 ]; then
  # Substrate first: on CI (no oracle) this BLOCKs without paying for builds.
  set +e
  python3 "$SUITE" --preflight
  rc=$?
  set -e
  if [ "$rc" -ne 0 ]; then
    [ "$rc" -eq 3 ] && echo "BLOCKED: measurement substrate absent (untracked oracle binary/corpora)"
    exit "$rc"
  fi
  load_check "before the builds"
  run_size_builds
fi

if [ ! -x "$RUST" ]; then
  echo "BLOCKED: candidate bench binary not found: $RUST"
  echo "   build it with: cargo build -p rustty-bench --profile release-fast"
  exit 3
fi

# Fence 2: say what the candidate is compared against, or why it is not. When
# it cannot run, Fence 1 still runs (and still writes --emit-metrics for G8),
# but the verdict below is never GREEN.
F2_STATE=""
F2_WHY=""
STORE_HINT="store one on purpose (perf README, Fence 2): .port/tools/gates/check_perf_fence.sh --store-entry"
if [ ! -e "$ENTRY" ]; then
  F2_STATE=BLOCKED
  F2_WHY="Fence 2 did not run: no entry binary at $ENTRY; $STORE_HINT"
elif [ ! -x "$ENTRY" ]; then
  F2_STATE=BLOCKED
  F2_WHY="Fence 2 did not run: the entry $ENTRY is not executable; re-store it: $STORE_HINT"
else
  set +e
  why="$(entry_py verify "$ENTRY")"
  vrc=$?
  set -e
  printf '%s\n' "$why"
  case "$vrc" in
    0) ARGS+=("--self" "$ENTRY") ;;
    1) F2_STATE=RED
       F2_WHY="Fence 2's entry is not what its manifest claims (sha256 differs; see above); re-store it: $STORE_HINT" ;;
    4) F2_STATE=BLOCKED
       F2_WHY="Fence 2 did not run: $why; re-store it: $STORE_HINT" ;;
    *) echo "RED: cannot verify Fence 2's entry $ENTRY (exit $vrc)"; exit 1 ;;
  esac
fi
if [ -n "$F2_STATE" ]; then
  echo "NOTE: $F2_STATE pending: $F2_WHY -> Fence 1 (oracle) only, verdict not GREEN."
fi

load_check "before measuring"
set +e
# ${ARGS[@]+...}: an empty array under `set -u` is an unbound-variable error on
# bash 3.2 (macOS), which made this line exit 1 whenever no flag added an ARG.
python3 "$SUITE" --rust "$RUST" ${ARGS[@]+"${ARGS[@]}"}
rc=$?
set -e

case "$rc" in
  0) if [ -z "$F2_STATE" ]; then echo "GREEN: perf fence held (Fence 1 and Fence 2)"
     else echo "Fence 1 held; Fence 2 did not run (below)"; fi ;;
  1) echo "RED: perf fence regressed; see LOST / SELF-REGRESS above" ;;
  2) echo "RED (exit 2): the harness reported a provisional (loaded) measurement or a usage error; see above. Only the pre-measurement load check is BLOCKED." ;;
  3) echo "BLOCKED: measurement substrate absent (untracked oracle binary/corpora)" ;;
  *) echo "RED: unexpected harness exit $rc (a crash or a kill)" ;;
esac
# Fence 2's own verdict joins the harness's: any RED wins (a RED Fence 2 entry
# turns a GREEN or BLOCKED run into exit 1), and a Fence 2 that did not run
# turns only a GREEN into BLOCKED, never a RED.
if [ "$F2_STATE" = RED ]; then
  echo "RED: $F2_WHY"
  case "$rc" in 0|3) rc=1 ;; esac
elif [ "$F2_STATE" = BLOCKED ]; then
  echo "BLOCKED: $F2_WHY"
  if [ "$rc" -eq 0 ]; then rc=3; fi
fi
exit "$rc"
