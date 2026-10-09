#!/usr/bin/env bash
# ci_sched.sh — the gate scheduler, change scoping and timing that ci.sh sources.
#
# ci.sh owns the gate REGISTRY and the pass/BLOCKED(3)/RED contract; this file
# owns HOW the registered gates are run (CI speed-up, P8). Nothing here
# changes what a gate checks or how its exit code is read: every gate of a full
# run still runs, and its verdict goes through the same classification as
# before (`ci_classify`, the body of the old `run_gate`).
#
# REGISTRY ENTRY: "ID|KIND|RES|TRAITS|runner..."
#   KIND    HARD | PROGRESS (| PERF-FENCE | ARMED: never dispatched, see ci.sh)
#   RES     light | heavy. Heavy = long or machine-filling (Miri, sanitizers,
#           Valgrind, release / clean-clone / feature-matrix builds, G4/G5/G8).
#           At most CI_HEAVY_CAP heavy gates run at once.
#   TRAITS  `-`, or comma-separated tokens, declared per entry, never inferred:
#     <lock>          a bare word. Two gates naming the same lock never overlap.
#                     `tgt` = the gate builds in, or reads artifacts uplifted
#                     into, the DEFAULT target dir (target/{debug,release}/
#                     lib*.{a,dylib,so}, xtask, examples) or writes untracked
#                     files into the tree (dist/, zig-out/, target/fuzz-*).
#                     Cargo's own lock keeps concurrent builds correct, but not
#                     an uplifted librustty_vt_c.* rebuilt with other features
#                     while another gate loads it (G-VTCLIB's publish rule).
#     dir=<name>      in a parallel run, CARGO_TARGET_DIR=$CI_TARGET_ROOT/<name>
#                     for this gate: its flag set differs from the default build
#                     and it must not share a dir with a gate running beside it.
#                     Gates naming the same dir are serialized.
#     after=<ID>      start only once gate ID has finished (or is not run).
#     shard           the runner takes `--shard K/N` (G-VALGRIND): in a parallel
#                     run it is split into CI_VALGRIND_SHARDS heavy units.
#     scope=<how>     what `--changed-since` passes to the gate (see below).
#
# --jobs 1 runs the gates through ci.sh's streaming `run_gate`, in registry
# order, exactly as before; --jobs N (N > 1) runs this scheduler. Bash 3.2
# (macOS /bin/bash): no `wait -n`, no associative arrays, no bare expansion of
# a possibly empty array under `set -u`.

# ---- small helpers -----------------------------------------------------------
ci_ncpu() {
  local n
  n="$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 2)"
  case "$n" in ''|*[!0-9]*) n=2 ;; esac
  echo "$n"
}
# Conservative: half the cores, 2..8. GitHub-hosted runs (GITHUB_ACTIONS set)
# stay at 1, i.e. exactly the old serial run.
ci_jobs_auto() {
  local n
  if [ -n "${GITHUB_ACTIONS:-}" ]; then echo 1; return 0; fi
  n=$(( $(ci_ncpu) / 2 ))
  [ "$n" -lt 2 ] && n=2
  [ "$n" -gt 8 ] && n=8
  echo "$n"
}
ci_now() { date +%s; }
ci_free_gb() { df -Pk "$1" 2>/dev/null | awk 'NR==2 {printf "%d", $4/1048576}'; }
# trait_val TRAITS KEY -> the value of KEY=... ; non-zero when absent
trait_val() {
  local t
  local IFS=,
  for t in $1; do case "$t" in "$2="*) echo "${t#*=}"; return 0 ;; esac; done
  return 1
}
# trait_has TRAITS TOKEN
trait_has() { case ",$1," in *",$2,"*) return 0 ;; esac; return 1; }
# trait_mutex TRAITS -> one mutual-exclusion key per line: bare locks and dir=
trait_mutex() {
  local t
  local IFS=,
  for t in $1; do
    case "$t" in
      -|shard|after=*|scope=*) ;;
      dir=*) echo "dir:${t#dir=}" ;;
      *) echo "$t" ;;
    esac
  done
}
in_words() { case " $2 " in *" $1 "*) return 0 ;; esac; return 1; }

# ---- timings -----------------------------------------------------------------
# ci-timings.tsv: gate id, kind, rc, seconds. rc is the exit code, or
# `skipped` (--fast) / `scoped-out` (--changed-since). One row per cargo step
# (kind CARGO), per gate, and per Valgrind shard (`G-VALGRIND[k/N]`, kind
# SHARD) next to its gate's own row.
ci_timing_init() {
  mkdir -p "$CI_RUN_DIR/logs"
  printf 'gate\tkind\trc\tseconds\n' > "$CI_TIMINGS"
  CI_T_START=$(ci_now)
}
ci_timing_row() { printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >> "$CI_TIMINGS"; }
ci_timing_summary() {
  local wall=$(( $(ci_now) - CI_T_START ))
  echo "TIMINGS — wall ${wall}s ($((wall / 60)) min), --jobs $CI_JOBS, heavy cap $CI_HEAVY_CAP; slowest first (all rows: $CI_TIMINGS):"
  awk -F'\t' 'NR > 1 && $3 != "skipped" && $3 != "scoped-out" && $2 != "SHARD" {printf "%8d  %s\n", $4, $1 "  [" $2 ", rc " $3 "]"}' "$CI_TIMINGS" \
    | sort -rn | head -"${CI_TIMINGS_TOP:-25}" | sed 's/^/  /'
}

# ---- change scoping (--changed-since) ----------------------------------------
# SCOPE_MODE: full (no --changed-since) | all (every crate selected: a full
# run) | none (docs/records only: every scoped long gate is scoped out) |
# crates (SCOPE_CRATES = the changed crates' reverse-dependency closure).
SCOPE_MODE=full
SCOPE_CRATES=""
SCOPE_CHANGED=""
SCOPE_REASON=""

# ci_scope_from_list LISTFILE — classify the changed paths in LISTFILE (one per
# line, repo-relative) against `cargo metadata`, and set SCOPE_*. Conservative:
#   * a path under crates/<c>/ or xtask/ selects that crate (any file type: a
#     README can be a doctest through include_str!);
#   * .port/tools/** (gates, ci.sh, this file, miri-run.sh, the supp files),
#     .port/phase-2/** (Miri harnesses, sanitizer suppressions and their
#     ledgers), crates-sys/**, any build.rs, Cargo.lock, the root Cargo.toml,
#     rust-toolchain*, .cargo/** and valgrind.supp select ALL crates;
#   * docs and records (*.md / *.txt outside the above, docs/, .superpowers/,
#     .port/** outside tools/ and phase-2/) select nothing;
#   * any other path selects ALL crates (unknown is never narrowed).
# The crate set is then closed over reverse dependencies of every kind
# (normal, build, dev), transitively.
ci_scope_from_list() {
  local list="$1" meta out
  mkdir -p "$CI_RUN_DIR" || return 2
  meta="$CI_RUN_DIR/scope-metadata.json"
  if ! (cd "$ROOT" && cargo metadata --no-deps --format-version 1 --offline) > "$meta" 2>/dev/null; then
    echo "ci.sh: cargo metadata failed; cannot compute the change scope" >&2; return 2
  fi
  out="$("$PY" - "$meta" "$list" <<'PY'
import json, os, re, sys
meta = json.load(open(sys.argv[1]))
root = meta["workspace_root"]
members = set(meta["workspace_members"])
pkgs = [p for p in meta["packages"] if p["id"] in members]
names = {p["name"] for p in pkgs}
dirs = sorted(((os.path.relpath(os.path.dirname(p["manifest_path"]), root) + "/", p["name"]) for p in pkgs),
              key=lambda d: -len(d[0]))
rdeps = {n: set() for n in names}
for p in pkgs:
    for d in p["dependencies"]:
        if d["name"] in names and d.get("path"):
            rdeps[d["name"]].add(p["name"])
files = [l.strip() for l in open(sys.argv[2]) if l.strip()]
GLOBAL_PREFIX = (".port/tools/", ".port/phase-2/", "crates-sys/", ".cargo/")
GLOBAL_FILE = {"Cargo.lock", "Cargo.toml", "rust-toolchain", "rust-toolchain.toml", "valgrind.supp"}
def is_doc(f):
    # .port/tools/ and .port/phase-2/ are GLOBAL and were matched before this.
    if f.startswith(("docs/", ".superpowers/", ".port/")):
        return True
    return re.search(r"\.(md|txt)$", f) is not None
glob, changed, docs = [], set(), 0
for f in files:
    if f.startswith(GLOBAL_PREFIX) or f in GLOBAL_FILE or f.startswith("rust-toolchain") \
            or re.search(r"(^|/)build\.rs$", f):
        glob.append(f); continue
    hit = next((n for d, n in dirs if f.startswith(d)), None)
    if hit is not None and (f.startswith("crates/") or f.startswith("xtask/")):
        changed.add(hit); continue
    if is_doc(f):
        docs += 1; continue
    glob.append(f + " (unclassified)")
if glob:
    print("mode=all")
    print("reason=%d path(s) select every crate: %s%s" % (len(glob), ", ".join(glob[:4]), " ..." if len(glob) > 4 else ""))
    print("changed=" + " ".join(sorted(names)))
    print("closure=" + " ".join(sorted(names)))
    sys.exit(0)
closure, todo = set(changed), list(changed)
while todo:
    for r in rdeps.get(todo.pop(), ()):
        if r not in closure:
            closure.add(r); todo.append(r)
if not closure:
    print("mode=none")
    print("reason=%s" % ("no changed path" if not files else "docs/records only (%d path(s))" % docs))
    print("changed=")
    print("closure=")
    sys.exit(0)
print("mode=crates")
print("reason=%d crate(s) changed (%d docs/records path(s) ignored); closure over reverse dependencies: %d crate(s)" % (len(changed), docs, len(closure)))
print("changed=" + " ".join(sorted(changed)))
print("closure=" + " ".join(sorted(closure)))
PY
)" || return 2
  SCOPE_MODE="$(printf '%s\n' "$out" | sed -n 's/^mode=//p')"
  SCOPE_REASON="$(printf '%s\n' "$out" | sed -n 's/^reason=//p')"
  SCOPE_CHANGED="$(printf '%s\n' "$out" | sed -n 's/^changed=//p')"
  SCOPE_CRATES="$(printf '%s\n' "$out" | sed -n 's/^closure=//p')"
  [ -n "$SCOPE_MODE" ] || return 2
  return 0
}

# ci_scope_from_rev REV — the changed paths: `git diff --name-only REV` (REV
# against the working tree, so uncommitted edits count) plus untracked files.
ci_scope_from_rev() {
  local rev="$1" list
  if ! git -C "$ROOT" rev-parse --verify --quiet "$rev^{commit}" >/dev/null; then
    echo "ci.sh: --changed-since: '$rev' is not a commit" >&2; return 2
  fi
  mkdir -p "$CI_RUN_DIR" || return 2
  list="$CI_RUN_DIR/scope-changed-paths.txt"
  { git -C "$ROOT" diff --name-only "$rev" -- && git -C "$ROOT" ls-files --others --exclude-standard; } > "$list" \
    || { echo "ci.sh: git diff failed" >&2; return 2; }
  SCOPE_NFILES="$(grep -c . "$list" | tr -d ' ')"
  ci_scope_from_list "$list"
}

ci_scope_print() {
  echo "CHANGE SCOPE (--changed-since $CI_CHANGED_SINCE; ${SCOPE_NFILES:-?} changed path(s)):"
  echo "  mode: $SCOPE_MODE — $SCOPE_REASON"
  if [ "$SCOPE_MODE" = crates ]; then
    echo "  changed crates: $SCOPE_CHANGED"
    echo "  closure (changed + every reverse dependent): $SCOPE_CRATES"
  fi
  echo "  gates with a scope= trait (the long gates) follow it; every other gate runs in full."
}
# After ci_plan: what --changed-since does to each gate with a scope= trait.
ci_scope_table() {
  local i how
  for i in ${CI_ORDER[@]+"${CI_ORDER[@]}"}; do
    how="$(trait_val "${GS_TRAITS[$i]}" scope)" || continue
    case "${GS_STATE[$i]}" in
      out) printf '  %-12s scoped out — %s\n' "${GS_ID[$i]}" "${GS_NOTE[$i]}" ;;
      skip) printf '  %-12s skipped by --fast\n' "${GS_ID[$i]}" ;;
      *) if [ "${GS_NARROWED[$i]}" = 1 ]; then
           printf '  %-12s narrowed — %s%s\n' "${GS_ID[$i]}" "${GS_NOTE[$i]}" "${GS_ARGS[$i]:+ (args: ${GS_ARGS[$i]})}"
         else printf '  %-12s full run%s\n' "${GS_ID[$i]}" "${GS_NOTE[$i]:+ — ${GS_NOTE[$i]}}"; fi ;;
    esac
  done
}

# ci_gate_scope TRAITS RUNNER — what --changed-since does to one gate. Sets
# SC_ACTION (run | narrow | out), SC_ENV ("VAR=value" or ""), SC_ARGS (extra
# runner arguments) and SC_NOTE. A gate without scope= always runs in full.
ci_gate_scope() {
  local traits="$1" runner="$2" how e c entries=""
  SC_ACTION=run; SC_ENV=""; SC_ARGS=""; SC_NOTE=""
  how="$(trait_val "$traits" scope)" || return 0
  case "$SCOPE_MODE" in full|all) return 0 ;; esac
  if [ "$SCOPE_MODE" = none ]; then SC_ACTION=out; SC_NOTE="no crate in scope: $SCOPE_REASON"; return 0; fi
  case "$how" in
    env:*)
      SC_ACTION=narrow; SC_ENV="${how#env:}=$SCOPE_CRATES"
      SC_NOTE="narrowed (${how#env:}) to the $(printf '%s\n' $SCOPE_CRATES | grep -c .) crate(s) of the change closure" ;;
    crate:*)
      if in_words "${how#crate:}" "$SCOPE_CRATES"; then SC_NOTE="${how#crate:} is in the change closure: runs in full"
      else SC_ACTION=out; SC_NOTE="${how#crate:} is not in the change closure"; fi ;;
    args)
      # The gate's own default scope (`--list-scope`), kept where the crate is
      # in the closure: crate names as positional arguments (its selector).
      # shellcheck disable=SC2086  # the runner is a word-split command line
      for e in $( $runner --list-scope 2>/dev/null ); do
        c="${e%%:*}"; in_words "$c" "$SCOPE_CRATES" && entries="$entries $e"
      done
      if [ -z "$entries" ]; then SC_ACTION=out; SC_NOTE="no crate of its scope is in the change closure"
      else SC_ACTION=narrow; SC_ARGS="${entries# }"; SC_NOTE="narrowed to $(printf '%s\n' $entries | grep -c .) scope entr(y/ies) in the change closure"; fi ;;
    *) echo "ci.sh: unknown scope trait '$how'" >&2; SC_ACTION=run ;;
  esac
  return 0
}

# ---- target dirs and --prune-targets -----------------------------------------
# Only dirs this script created: $CI_TARGET_ROOT/<name>, <name> a plain word,
# holding the marker file written when the script created it. A dir without
# the marker (made by hand, or by anything else) is never deleted.
CI_TARGET_MARK=".created-by-ci-sh"
ci_target_dir() {  # ci_target_dir NAME -> the path, created with its marker
  local d="$CI_TARGET_ROOT/$1"
  if [ ! -d "$d" ]; then mkdir -p "$d" && : > "$d/$CI_TARGET_MARK"; fi
  echo "$d"
}
ci_prune_target() {  # ci_prune_target NAME
  local name="$1" d
  case "$name" in ''|*[!a-z0-9-]*) echo "ci.sh: --prune-targets: refusing '$name' (not a plain dir name)" >&2; return 1 ;; esac
  case "$CI_TARGET_ROOT" in /*/target/ci-targets) ;; *) echo "ci.sh: --prune-targets: refusing target root '$CI_TARGET_ROOT' (not <checkout>/target/ci-targets)" >&2; return 1 ;; esac
  d="$CI_TARGET_ROOT/$name"
  [ -d "$d" ] || return 0
  if [ ! -f "$d/$CI_TARGET_MARK" ]; then echo "ci.sh: --prune-targets: refusing '$d' (no $CI_TARGET_MARK: not created by ci.sh)" >&2; return 1; fi
  rm -rf -- "$d" && echo "   (--prune-targets: deleted $d)"
}

# ---- classification (the old run_gate verdict, unchanged) ---------------------
# ci_classify IDX RC LOG — prints the verdict line and records the gate's
# summary row in GS_CAT/GS_TEXT, which ci_collect turns into the summary lists
# in registry order. NARROWED (GS_NARROWED) is --changed-since's: a pass of a
# narrowed gate is SCOPED, never PASSED; a narrowed gate that answers exit 3
# with its SCOPED-GREEN marker is SCOPED too; any other 3 is BLOCKED and any
# RED is RED, exactly as in a full run.
ci_classify() {
  local i="$1" rc="$2" log="$3" id kind
  id="${GS_ID[$i]}"; kind="${GS_KIND[$i]}"
  if [ "${GS_NARROWED[$i]}" = 1 ] && { [ "$rc" -eq 0 ] || { [ "$rc" -eq 3 ] && grep -q 'SCOPED-GREEN' "$log" 2>/dev/null; }; }; then
    echo ">> SCOPED ($rc): $id [ran only the change scope — not a full verdict]"
    GS_CAT[$i]=SCOPED; GS_TEXT[$i]="$id — ${GS_NOTE[$i]}"
  elif [ "$rc" -eq 0 ]; then
    echo ">> PASS: $id"
    if [ "$kind" = "PROGRESS" ]; then GS_CAT[$i]=PROGRESS; GS_TEXT[$i]="$id (progress/green)"; else GS_CAT[$i]=PASS; GS_TEXT[$i]="$id"; fi
  elif [ "$rc" -eq 3 ]; then
    echo ">> BLOCKED ($rc): $id [pending contract/tooling — surfaced, not greened]" >&2
    GS_CAT[$i]=WARN; GS_TEXT[$i]="$id BLOCKED (exit 3)$(blocked_reasons "$log")"
  else
    # Non-zero, non-BLOCKED => a real RED.
    if [ "$kind" = "HARD" ]; then
      echo ">> FAIL ($rc): $id [HARD-FAIL gate]" >&2
      GS_CAT[$i]=FAIL; GS_TEXT[$i]="$id (exit $rc)"
    else
      echo ">> FAIL ($rc): $id [PROGRESS gate went RED — a claimed-done crate regressed; R5]" >&2
      GS_CAT[$i]=FAIL; GS_TEXT[$i]="$id (claimed-done regression, exit $rc)"
    fi
  fi
}

# Summary lists, in dispatch (registry) order whatever the completion order.
ci_collect() {
  local i
  for i in ${CI_ORDER[@]+"${CI_ORDER[@]}"}; do
    case "${GS_CAT[$i]:-}" in
      PASS) PASSED+=("${GS_TEXT[$i]}") ;;
      PROGRESS) PROGRESS_NOTES+=("${GS_TEXT[$i]}") ;;
      WARN) WARNINGS+=("${GS_TEXT[$i]}") ;;
      FAIL) HARD_FAILURES+=("${GS_TEXT[$i]}") ;;
      SKIP) SKIPPED_GATES+=("${GS_ID[$i]}") ;;
      SCOPED) SCOPED_GATES+=("${GS_TEXT[$i]}") ;;
      OUT) SCOPED_OUT+=("${GS_TEXT[$i]}") ;;
    esac
  done
}

# ---- the plan ----------------------------------------------------------------
# Parse the registry into GS_* (one row per entry), CI_ORDER (dispatch order:
# HARD entries, then PROGRESS, as the serial loop always ran them) and the
# units (U_*: one per gate, or one per shard).
ci_plan() {
  local entry gid gkind gres gtraits grun n=0 phase i k nsh
  GS_ID=(); GS_KIND=(); GS_RES=(); GS_TRAITS=(); GS_RUN=(); GS_CAT=(); GS_TEXT=()
  GS_NARROWED=(); GS_NOTE=(); GS_ENV=(); GS_ARGS=(); GS_LEFT=(); GS_T0=(); GS_RCS=(); GS_STATE=()
  GS_UNITS=(); GS_UNITLIST=(); GS_MUTEX=(); GS_AFTER=()
  CI_ORDER=()
  for entry in "${GATE_REGISTRY[@]}"; do
    IFS='|' read -r gid gkind gres gtraits grun <<< "$entry"
    case "$gres" in light|heavy) ;; *) echo "ci.sh: registry entry '$gid' has RES '$gres' (want light|heavy)" >&2; exit 2 ;; esac
    [ -n "$gtraits" ] && [ -n "$grun" ] || { echo "ci.sh: registry entry '$gid' is not ID|KIND|RES|TRAITS|runner" >&2; exit 2; }
    GS_ID[$n]="$gid"; GS_KIND[$n]="$gkind"; GS_RES[$n]="$gres"; GS_TRAITS[$n]="$gtraits"; GS_RUN[$n]="$grun"
    GS_NARROWED[$n]=0; GS_NOTE[$n]=""; GS_ENV[$n]=""; GS_ARGS[$n]=""; GS_LEFT[$n]=0; GS_RCS[$n]=""; GS_STATE[$n]=pending
    GS_UNITS[$n]=0; GS_UNITLIST[$n]=""
    # Computed once here: the poll loop then needs no subshell per check.
    GS_MUTEX[$n]=" $(trait_mutex "$gtraits" | tr '\n' ' ')"
    GS_AFTER[$n]="$(printf '%s' "$gtraits" | tr ',' '\n' | sed -n 's/^after=//p' | tr '\n' ' ')"
    n=$((n + 1))
  done
  for phase in HARD PROGRESS; do
    i=0
    while [ "$i" -lt "$n" ]; do
      [ "${GS_KIND[$i]}" = "$phase" ] && CI_ORDER+=("$i")
      i=$((i + 1))
    done
  done
  U_GATE=(); U_LABEL=(); U_XARGS=(); U_STATE=(); U_PID=(); U_T0=()
  local u=0
  for i in ${CI_ORDER[@]+"${CI_ORDER[@]}"}; do
    if skip_in_fast "${GS_ID[$i]}"; then GS_STATE[$i]=skip; continue; fi
    ci_gate_scope "${GS_TRAITS[$i]}" "${GS_RUN[$i]}"
    GS_NOTE[$i]="$SC_NOTE"; GS_ENV[$i]="$SC_ENV"; GS_ARGS[$i]="$SC_ARGS"
    case "$SC_ACTION" in
      out) GS_STATE[$i]=out; continue ;;
      narrow) GS_NARROWED[$i]=1 ;;
    esac
    GS_UNITS[$i]=0
    nsh=1
    if [ "$CI_JOBS" -gt 1 ] && trait_has "${GS_TRAITS[$i]}" shard; then nsh="$CI_VALGRIND_SHARDS"; fi
    k=1
    while [ "$k" -le "$nsh" ]; do
      U_GATE[$u]="$i"; U_STATE[$u]=pending; U_PID[$u]=""; U_T0[$u]=0
      if [ "$nsh" -gt 1 ]; then
        U_LABEL[$u]="${GS_ID[$i]}[$k/$nsh]"
        U_XARGS[$u]="--shard $k/$nsh${CI_VALGRIND_JOBS:+ --jobs $CI_VALGRIND_JOBS}"
      else
        U_LABEL[$u]="${GS_ID[$i]}"; U_XARGS[$u]=""
      fi
      GS_LEFT[$i]=$(( GS_LEFT[$i] + 1 ))
      GS_UNITLIST[$i]="${GS_UNITLIST[$i]} $u"
      u=$((u + 1)); k=$((k + 1))
    done
    GS_UNITS[$i]="$nsh"
  done
}

# ci_unit_exec U — run one unit in the CURRENT (sub)shell: the gate's env, its
# dir, then its runner (a word-split command line, as the serial loop ran it).
ci_unit_exec() {
  local u="$1" i dir
  i="${U_GATE[$u]}"
  cd "$ROOT" || return 2
  if [ "$CI_JOBS" -gt 1 ] && dir="$(trait_val "${GS_TRAITS[$i]}" dir)"; then
    CARGO_TARGET_DIR="$(ci_target_dir "$dir")"; export CARGO_TARGET_DIR
  fi
  # shellcheck disable=SC2163  # GS_ENV is "NAME=value"
  [ -n "${GS_ENV[$i]}" ] && export "${GS_ENV[$i]}"
  # shellcheck disable=SC2086  # an intentional word-split command line
  ${GS_RUN[$i]} ${U_XARGS[$u]} ${GS_ARGS[$i]}
}

# ---- the parallel scheduler ----------------------------------------------------
# Poll loop (bash 3.2 has no `wait -n`): every CI_POLL seconds, finished units
# are reaped (each writes its exit code to its own .rc file), a gate whose
# units have all finished is printed WHOLE (its section, its log, its verdict,
# its duration) and classified, and then pending units are launched in
# dispatch order while: fewer than CI_JOBS units run; a heavy unit finds fewer
# than CI_HEAVY_CAP heavy units running (and, when one already runs, at least
# CI_MIN_FREE_GB free on the target volume); none of its locks/dirs is held by
# a running unit of ANOTHER gate (a gate's own shards share theirs); every
# after= gate has finished.
CI_POLL="${CI_POLL:-0.2}"
ci_unit_launch() {
  local u="$1" i log rcf
  i="${U_GATE[$u]}"
  log="$CI_RUN_DIR/logs/$(printf '%s' "${U_LABEL[$u]}" | tr '/[]' '___').log"; rcf="$log.rc"
  U_RCF[$u]="$rcf"
  : > "$log"; : > "$rcf"   # an EMPTY .rc means running (no stale rc from a past run)
  # The inner subshell: even a runner that calls `exit` leaves the .rc file.
  ( ( ci_unit_exec "$u" ) > "$log" 2>&1 < /dev/null; echo $? > "$rcf.tmp"; mv -f "$rcf.tmp" "$rcf" ) &
  U_PID[$u]=$!; U_STATE[$u]=running; U_T0[$u]=$(ci_now)
  [ "${GS_STATE[$i]}" = pending ] && { GS_STATE[$i]=running; GS_T0[$i]=${U_T0[$u]}; }
}
ci_unit_ready() {  # 0 when unit U may start now
  local u="$1" i v o j dep di
  i="${U_GATE[$u]}"
  for dep in ${GS_AFTER[$i]}; do
    for di in ${CI_ORDER[@]+"${CI_ORDER[@]}"}; do
      [ "${GS_ID[$di]}" = "$dep" ] || continue
      case "${GS_STATE[$di]}" in done|skip|out) ;; *) return 1 ;; esac
    done
  done
  [ "${GS_MUTEX[$i]}" != " " ] || return 0
  for o in ${CI_RUNNING[@]+"${CI_RUNNING[@]}"}; do
    j="${U_GATE[$o]}"; [ "$j" = "$i" ] && continue
    for v in ${GS_MUTEX[$j]}; do
      case "${GS_MUTEX[$i]} " in *" $v "*) return 1 ;; esac
    done
  done
  return 0
}
ci_gate_finish() {  # print gate I whole, classify it, record timings, prune
  local i="$1" rc=0 r u log secs d dirname other
  secs=$(( $(ci_now) - GS_T0[$i] ))
  section "gate ${GS_ID[$i]}  [${GS_KIND[$i]}]  ->  ${GS_RUN[$i]}${GS_ARGS[$i]:+ ${GS_ARGS[$i]}}"
  [ -n "${GS_NOTE[$i]}" ] && echo "(--changed-since: ${GS_NOTE[$i]}${GS_ENV[$i]:+; ${GS_ENV[$i]%%=*} set})"
  log="$CI_RUN_DIR/logs/$(printf '%s' "${GS_ID[$i]}" | tr '/[]' '___').log"
  if [ "${GS_UNITS[$i]}" -gt 1 ]; then
    # Shards: each shard's log whole, in shard order, also kept as the gate's.
    log="$CI_RUN_DIR/logs/$(printf '%s' "${GS_ID[$i]}" | tr '/[]' '___').gate.log"
    : > "$log"
    for u in ${GS_UNITLIST[$i]}; do
      d="$CI_RUN_DIR/logs/$(printf '%s' "${U_LABEL[$u]}" | tr '/[]' '___').log"
      { echo "--- ${U_LABEL[$u]} ($(( U_END[$u] - U_T0[$u] ))s, exit ${U_RC[$u]}) ---"; cat "$d"; } >> "$log"
    done
  fi
  cat "$log"
  # A sharded gate: any RED shard is the gate's RED; else any BLOCKED shard is
  # BLOCKED; only all-green shards are green. Never the last shard's rc alone.
  for r in ${GS_RCS[$i]}; do
    case "$r" in
      0) ;;
      3) [ "$rc" -eq 0 ] && rc=3 ;;
      *) if [ "$rc" -eq 0 ] || [ "$rc" -eq 3 ]; then rc="$r"; fi ;;
    esac
  done
  ci_classify "$i" "$rc" "$log"
  echo "   (gate ${GS_ID[$i]}: ${secs}s)"
  ci_timing_row "${GS_ID[$i]}" "${GS_KIND[$i]}" "$rc" "$secs"
  GS_STATE[$i]="done"
  if [ "$CI_PRUNE" = 1 ] && dirname="$(trait_val "${GS_TRAITS[$i]}" dir)"; then
    # Only when no gate still to run uses the same dir.
    for other in ${CI_ORDER[@]+"${CI_ORDER[@]}"}; do
      case "${GS_STATE[$other]}" in pending|running) ;; *) continue ;; esac
      [ "$(trait_val "${GS_TRAITS[$other]}" dir 2>/dev/null)" = "$dirname" ] && return 0
    done
    ci_prune_target "$dirname" || true
  fi
}
ci_run_parallel() {
  local u i nrun nheavy progressed rcv fg
  CI_RUNNING=(); U_RC=(); U_END=(); U_RCF=()
  local scan=1 last_scan=0
  # --fast / --changed-since lines first, in order: they run nothing.
  for i in ${CI_ORDER[@]+"${CI_ORDER[@]}"}; do
    case "${GS_STATE[$i]}" in
      skip) echo "### gate ${GS_ID[$i]}  [SKIPPED by --fast]"; GS_CAT[$i]=SKIP; ci_timing_row "${GS_ID[$i]}" "${GS_KIND[$i]}" skipped 0 ;;
      out) echo "### gate ${GS_ID[$i]}  [SCOPED OUT by --changed-since: ${GS_NOTE[$i]}]"; GS_CAT[$i]=OUT; GS_TEXT[$i]="${GS_ID[$i]} — ${GS_NOTE[$i]}"; ci_timing_row "${GS_ID[$i]}" "${GS_KIND[$i]}" scoped-out 0 ;;
    esac
  done
  echo "(parallel: --jobs $CI_JOBS, heavy cap $CI_HEAVY_CAP, ${#U_GATE[@]} unit(s); each gate prints whole when it ends; logs in $CI_RUN_DIR/logs)"
  while :; do
    # reap
    progressed=0
    local -a still=()
    for u in ${CI_RUNNING[@]+"${CI_RUNNING[@]}"}; do
      local rcf="${U_RCF[$u]}"
      if [ -s "$rcf" ]; then
        wait "${U_PID[$u]}" 2>/dev/null
        rcv="$(cat "$rcf")"; case "$rcv" in ''|*[!0-9]*) rcv=2 ;; esac
        U_RC[$u]="$rcv"; U_END[$u]=$(ci_now); U_STATE[$u]="done"; progressed=1
        i="${U_GATE[$u]}"
        GS_RCS[$i]="${GS_RCS[$i]} $rcv"
        GS_LEFT[$i]=$(( GS_LEFT[$i] - 1 ))
        [ "${U_LABEL[$u]}" != "${GS_ID[$i]}" ] && ci_timing_row "${U_LABEL[$u]}" SHARD "$rcv" $(( U_END[$u] - U_T0[$u] ))
        [ "${GS_LEFT[$i]}" -eq 0 ] && ci_gate_finish "$i"
      else
        still+=("$u")
      fi
    done
    CI_RUNNING=(${still[@]+"${still[@]}"})
    # launch: only when a slot may have opened (a unit ended, or the first
    # pass), or every 5 s for the free-disk condition on heavy gates.
    [ "$progressed" -eq 1 ] && scan=1
    [ $(( SECONDS - last_scan )) -ge 5 ] && scan=1
    if [ "$scan" -eq 0 ]; then sleep "$CI_POLL"; continue; fi
    scan=0; last_scan=$SECONDS; fg=""
    nrun=${#CI_RUNNING[@]}
    nheavy=0
    for u in ${CI_RUNNING[@]+"${CI_RUNNING[@]}"}; do [ "${GS_RES[${U_GATE[$u]}]}" = heavy ] && nheavy=$((nheavy + 1)); done
    local pending=0
    u=0
    while [ "$u" -lt "${#U_GATE[@]}" ]; do
      if [ "${U_STATE[$u]}" = pending ]; then
        pending=$((pending + 1))
        if [ "$nrun" -lt "$CI_JOBS" ]; then
          i="${U_GATE[$u]}"
          if [ "${GS_RES[$i]}" = heavy ]; then
            if [ "$nheavy" -ge "$CI_HEAVY_CAP" ]; then u=$((u + 1)); continue; fi
            if [ "$nheavy" -gt 0 ]; then
              [ -n "$fg" ] || fg="$(ci_free_gb "$ROOT")"
              if [ -n "$fg" ] && [ "$fg" -lt "$CI_MIN_FREE_GB" ]; then u=$((u + 1)); continue; fi
            fi
          fi
          if ci_unit_ready "$u"; then
            ci_unit_launch "$u"
            CI_RUNNING+=("$u"); nrun=$((nrun + 1)); progressed=1
            [ "${GS_RES[$i]}" = heavy ] && nheavy=$((nheavy + 1))
            pending=$((pending - 1))
          fi
        fi
      fi
      u=$((u + 1))
    done
    [ "$pending" -eq 0 ] && [ "${#CI_RUNNING[@]}" -eq 0 ] && break
    [ "$progressed" -eq 1 ] || sleep "$CI_POLL"
  done
}

# Kill what this run started (explicit PIDs and their descendants) on INT/TERM.
ci_killtree() { local p="$1" c; for c in $(pgrep -P "$p" 2>/dev/null); do ci_killtree "$c"; done; kill "$p" 2>/dev/null; }
ci_on_signal() {
  local u
  echo "ci.sh: interrupted — stopping the running gate(s)" >&2
  for u in ${CI_RUNNING[@]+"${CI_RUNNING[@]}"}; do [ -n "${U_PID[$u]}" ] && ci_killtree "${U_PID[$u]}"; done
  exit 130
}

# ---- G-CI-SELF (ci.sh --self-test) ------------------------------------------
# Runs COPIES of ci.sh + this file, each in its own fixture root under
# $CI_RUN_DIR/selftest/ (overwritten each run, never deleted), with
# CI_SKIP_CARGO=1 and a fixture GATE_REGISTRY of fake gates (fake.sh: append
# `S id` / `E id` to an events file around a sleep, then exit with the asked
# code). Each arm has its GREEN case and a negative control: a copy MUTATED to
# reintroduce exactly the regression the arm exists to catch, which the arm
# must notice.
#   A  --jobs 1: HARD gates then PROGRESS, in registry order; one timing row
#      per gate. Control: dispatch in plain registry order (no phases).
#   B  a RED gate inside a parallel batch fails the run (exit 1, named), each
#      gate's output printed whole. Control: the finisher classifies rc 0.
#   C  heavy cap, locks, after=: never more than --heavy-cap heavy gates at
#      once (and the cap is reached), two gates sharing a lock or a dir never
#      overlap, after= waits. Controls: cap check dropped; lock check dropped.
#   D  sharding: a RED shard makes the gate RED; shards share their dir and
#      run side by side. Control: the gate takes the last shard's rc.
#   E  change scope over the REAL workspace: a crate change selects every
#      transitive reverse dependent; tooling / Cargo.lock / build.rs /
#      crates-sys / unknown paths select ALL; docs/records select none.
#      Controls: one level of reverse deps only; .port/tools/ not global.
#   F  --changed-since end to end in a fixture git repo + cargo workspace:
#      narrowed gates get the closure, a crate:-scoped gate outside it is
#      scoped out, exit 5 and no PASS banner; a RED in scope is exit 1; a
#      tooling change is a full run (exit 0); SCOPED-GREEN vs plain BLOCKED.
#      Control: the scoped banner removed (the run falls through to PASS).
#   G  --fast still exits 4 and names its skips, serial and parallel.
#   H  --prune-targets deletes only marked dirs under target/ci-targets.
CST_PASS=0; CST_FAIL=0
cst_ok()  { echo "  ok    $1"; CST_PASS=$((CST_PASS + 1)); }
cst_bad() { echo "  FAIL  $1"; printf '%s\n' "${CST_OUT:-}" | tail -15 | sed 's/^/        /'; CST_FAIL=$((CST_FAIL + 1)); }
# cst_make NAME REGISTRY — a fixture root with copies of ci.sh (its registry
# replaced) and this file; CST_MUT_FILE/CST_MUT_OLD/CST_MUT_NEW mutate exactly
# one occurrence of one file's own code (never this self-test's text).
cst_make() {
  local d="$CST_BASE/$1"
  mkdir -p "$d/.port/tools/gates" "$d/target"
  CST_REG="$2" "$PY" - "$ROOT/.port/tools" "$d/.port/tools" <<'PY' || return 1
import os, sys
src, dst = sys.argv[1], sys.argv[2]
for f in ("ci.sh", "ci_sched.sh"):
    s = open(os.path.join(src, f)).read()
    if f == "ci.sh":
        a = s.index("\nGATE_REGISTRY=(\n") + 1
        b = s.index("\n)\n", a) + 3
        reg = "".join('  "%s"\n' % l for l in os.environ["CST_REG"].split("\n") if l.strip())
        s = s[:a] + "GATE_REGISTRY=(\n" + reg + ")\n" + s[b:]
    if os.environ.get("CST_MUT_FILE") == f:
        old, new = os.environ["CST_MUT_OLD"], os.environ["CST_MUT_NEW"]
        a = s.find("\n# ---- G-CI-SELF (ci.sh --self-test)")
        body = s if a < 0 else s[:a]
        assert body.count(old) == 1, "mutation target occurs %d times: %r" % (body.count(old), old)
        s = body.replace(old, new) + ("" if a < 0 else s[a:])
    open(os.path.join(dst, f), "w").write(s)
PY
  cat > "$d/.port/tools/gates/fake.sh" <<'FAKE'
#!/usr/bin/env bash
# fake.sh ID RC SECONDS [MARKER-LINE]
ev="$(cd "$(dirname "$0")/../../.." && pwd)/events"
echo "S $1" >> "$ev"
echo "BEGIN $1"
[ -n "${G6_ONLY:-}" ] && echo "G6_ONLY=$G6_ONLY"
[ -n "${CARGO_TARGET_DIR:-}" ] && echo "CTD=$CARGO_TARGET_DIR"
sleep "$3"
[ "$#" -gt 3 ] && echo "${*:4}"
echo "END $1"
echo "E $1" >> "$ev"
exit "$2"
FAKE
  cat > "$d/.port/tools/gates/check_valgrind.sh" <<'FAKE'
#!/usr/bin/env bash
# A stand-in G-VALGRIND: `--list-scope`, `--shard K/N`, crate arguments. The
# shard named in <root>/vg_fail (if any) exits 1.
root="$(cd "$(dirname "$0")/../../.." && pwd)"
k=1; n=1; crates=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --list-scope) printf '%s\n' a b c "b:feat"; exit 0 ;;
    --shard) k="${2%/*}"; n="${2#*/}"; shift 2 ;;
    *) crates="$crates $1"; shift ;;
  esac
done
echo "S vg$k" >> "$root/events"
echo "shard $k/$n crates:${crates:- <default>} CTD=${CARGO_TARGET_DIR:-unset}"
sleep 1
echo "E vg$k" >> "$root/events"
[ "$(cat "$root/vg_fail" 2>/dev/null)" = "$k" ] && { echo "G-VALGRIND RED: shard $k"; exit 1; }
echo "G-VALGRIND GREEN (shard $k/$n)"
exit 0
FAKE
  chmod +x "$d/.port/tools/gates/fake.sh" "$d/.port/tools/gates/check_valgrind.sh"
  : > "$d/events"; : > "$d/vg_fail"
}
# cst_run NAME ARGS... — CST_OUT, CST_RC; the outer run's knobs never leak in.
cst_run() {
  local d="$CST_BASE/$1"; shift
  : > "$d/events"
  CST_OUT="$(cd "$d" && env -u CI_RUN_DIR -u CI_VALGRIND_SHARDS -u CI_VALGRIND_JOBS -u CARGO_TARGET_DIR \
    -u G6_ONLY -u SAN_ONLY -u GITHUB_ACTIONS -u CI_MIN_FREE_GB CI_SKIP_CARGO=1 CI_POLL=0.1 \
    ${CST_ENV:-} bash "$d/.port/tools/ci.sh" "$@" 2>&1)"
  CST_RC=$?
}
cst_has() { printf '%s\n' "$CST_OUT" | grep -qF -- "$1"; }
# cst_events NAME CHECK ARGS — python over the fixture's events file:
#   maxpar <ids...>      prints the max number of those ids running at once
#   overlap <a> <b>      exit 0 when a and b ever ran at the same time
#   before <a> <b>       exit 0 when a ended before b started
cst_events() {
  "$PY" - "$CST_BASE/$1/events" "${@:2}" <<'PY'
import sys
ev = [l.split() for l in open(sys.argv[1]) if l.strip()]
op, args = sys.argv[2], sys.argv[3:]
if op == "maxpar":
    run = mx = 0
    for k, i in ev:
        if i in args:
            run += 1 if k == "S" else -1
            mx = max(mx, run)
    print(mx); sys.exit(0)
if op == "overlap":
    live = set()
    for k, i in ev:
        if k == "S":
            if (i == args[0] and args[1] in live) or (i == args[1] and args[0] in live):
                sys.exit(0)
            live.add(i)
        else:
            live.discard(i)
    sys.exit(1)
if op == "before":
    pos = {(k, i): n for n, (k, i) in enumerate(ev)}
    sys.exit(0 if ("E", args[0]) in pos and ("S", args[1]) in pos and pos[("E", args[0])] < pos[("S", args[1])] else 1)
PY
}
cst_headers() { printf '%s\n' "$CST_OUT" | sed -n 's/^### gate \([^ ]*\) .*/\1/p' | tr '\n' ' '; }

ci_self_test() {
  CST_BASE="$CI_RUN_DIR/selftest"
  mkdir -p "$CST_BASE" || { echo "G-CI-SELF BLOCKED: cannot create $CST_BASE"; return 3; }
  command -v "$PY" >/dev/null 2>&1 || { echo "G-CI-SELF BLOCKED: no $PY"; return 3; }
  command -v git >/dev/null 2>&1 || { echo "G-CI-SELF BLOCKED: no git (arm F)"; return 3; }
  local R h
  echo "=== G-CI-SELF :: ci.sh's scheduler, scoping and sharding, on copies of ci.sh ==="

  # A — the serial order.
  R='H1|HARD|light|-|bash $GATES/fake.sh H1 0 0
P1|PROGRESS|light|-|bash $GATES/fake.sh P1 0 0
H2|HARD|heavy|-|bash $GATES/fake.sh H2 0 0'
  cst_make order "$R"; cst_run order --jobs 1
  h="$(cst_headers)"
  if [ "$CST_RC" = 0 ] && [ "$h" = "H1 H2 P1 " ] && cst_has "CI RESULT: PASS" \
     && [ "$(awk -F'\t' '$3 == 0 && ($1 == "H1" || $1 == "H2" || $1 == "P1")' "$CST_BASE/order/target/ci-run/ci-timings.tsv" | grep -c .)" = 3 ]; then
    cst_ok "A --jobs 1: HARD then PROGRESS in registry order ($h), one ci-timings.tsv row each"
  else cst_bad "A --jobs 1 order/timings (exit $CST_RC, order '$h')"; fi
  CST_MUT_FILE=ci_sched.sh CST_MUT_OLD='[ "${GS_KIND[$i]}" = "$phase" ] && CI_ORDER+=("$i")' \
    CST_MUT_NEW='[ "$phase" = HARD ] && CI_ORDER+=("$i")' cst_make order-mut "$R"
  cst_run order-mut --jobs 1; h="$(cst_headers)"
  if [ "$h" != "H1 H2 P1 " ]; then cst_ok "A control: plain registry order ($h) is noticed"
  else cst_bad "A control: a phase-less dispatch went unnoticed"; fi

  # B — a RED in a parallel batch.
  R='B1|HARD|light|-|bash $GATES/fake.sh B1 0 1
B2|HARD|light|-|bash $GATES/fake.sh B2 1 1
B3|PROGRESS|light|-|bash $GATES/fake.sh B3 0 1
B4|HARD|light|-|bash $GATES/fake.sh B4 3 1 BLOCKED: fixture reason'
  cst_make red "$R"; cst_run red --jobs 4
  local whole=1 g
  for g in B1 B2 B3 B4; do
    # Between this gate's section header and its END line, nothing of another gate.
    printf '%s\n' "$CST_OUT" | awk -v g="$g" '$0 ~ "^### gate " g " " {on=1; next} on && /^END / {exit (($2 == g) ? 0 : 1)} on && /^(BEGIN|END) / && $2 != g {exit 1}' || whole=0
  done
  if [ "$CST_RC" = 1 ] && cst_has "X B2 (exit 1)" && cst_has "HARD FAILURES (1)" && cst_has "! B4 BLOCKED (exit 3)" \
     && cst_has "reason: BLOCKED: fixture reason" && [ "$whole" = 1 ] && [ "$(cst_events red maxpar B1 B2 B3 B4)" -ge 3 ]; then
    cst_ok "B a RED gate inside a parallel batch of 4 fails the run (exit 1), BLOCKED stays a WARNING, each gate printed whole"
  else cst_bad "B RED in a parallel batch (exit $CST_RC, whole=$whole)"; fi
  CST_MUT_FILE=ci_sched.sh CST_MUT_OLD='ci_classify "$i" "$rc" "$log"' CST_MUT_NEW='ci_classify "$i" 0 "$log"' cst_make red-mut "$R"
  cst_run red-mut --jobs 4
  if [ "$CST_RC" != 1 ]; then cst_ok "B control: a finisher that drops the gate's rc is noticed (exit $CST_RC)"
  else cst_bad "B control: a finisher that drops the rc went unnoticed"; fi

  # C — heavy cap, locks, dirs, after=.
  R='V1|HARD|heavy|-|bash $GATES/fake.sh V1 0 1
V2|HARD|heavy|-|bash $GATES/fake.sh V2 0 1
V3|HARD|heavy|-|bash $GATES/fake.sh V3 0 1
V4|HARD|heavy|-|bash $GATES/fake.sh V4 0 1
L1|HARD|light|tgt|bash $GATES/fake.sh L1 0 1
L2|HARD|light|tgt|bash $GATES/fake.sh L2 0 1
D1|PROGRESS|light|dir=shared|bash $GATES/fake.sh D1 0 1
D2|PROGRESS|light|dir=shared|bash $GATES/fake.sh D2 0 1
A1|HARD|light|-|bash $GATES/fake.sh A1 0 1
A2|HARD|light|after=A1|bash $GATES/fake.sh A2 0 0'
  cst_make cap "$R"; cst_run cap --jobs 8 --heavy-cap 2
  local mh; mh="$(cst_events cap maxpar V1 V2 V3 V4)"
  if [ "$CST_RC" = 0 ] && [ "$mh" = 2 ] && ! cst_events cap overlap L1 L2 && ! cst_events cap overlap D1 D2 \
     && cst_events cap before A1 A2 && cst_has "CTD=$CST_BASE/cap/target/ci-targets/shared"; then
    cst_ok "C heavy cap 2 reached and held (max $mh), tgt and dir=shared serialized, after= waits, dir= sets CARGO_TARGET_DIR"
  else cst_bad "C cap/locks/after (exit $CST_RC, max heavy $mh)"; fi
  CST_MUT_FILE=ci_sched.sh CST_MUT_OLD='if [ "$nheavy" -ge "$CI_HEAVY_CAP" ]; then' CST_MUT_NEW='if false; then' cst_make cap-mut "$R"
  cst_run cap-mut --jobs 8 --heavy-cap 2; mh="$(cst_events cap-mut maxpar V1 V2 V3 V4)"
  if [ "$mh" -gt 2 ]; then cst_ok "C control: a dropped heavy-cap check is noticed (max $mh heavy at once)"
  else cst_bad "C control: a dropped heavy-cap check went unnoticed (max $mh)"; fi
  CST_MUT_FILE=ci_sched.sh CST_MUT_OLD='  [ -n "$m" ] || return 0' CST_MUT_NEW='  return 0' cst_make lock-mut "$R"
  cst_run lock-mut --jobs 8 --heavy-cap 2
  if cst_events lock-mut overlap L1 L2; then cst_ok "C control: a dropped lock check is noticed (L1 and L2 overlapped)"
  else cst_bad "C control: a dropped lock check went unnoticed"; fi

  # D — sharding.
  R='G-VALGRIND|HARD|heavy|dir=valgrind,shard,scope=args|bash $GATES/check_valgrind.sh'
  cst_make shard "$R"; echo 2 > "$CST_BASE/shard/vg_fail"
  CST_ENV="CI_VALGRIND_SHARDS=3" cst_run shard --jobs 4 --heavy-cap 3
  if [ "$CST_RC" = 1 ] && cst_has "X G-VALGRIND (exit 1)" && cst_has "--- G-VALGRIND[2/3]" && cst_has "shard 3/3" \
     && [ "$(cst_events shard maxpar vg1 vg2 vg3)" -ge 2 ] && [ "$(printf '%s\n' "$CST_OUT" | grep -c "CTD=$CST_BASE/shard/target/ci-targets/valgrind")" = 3 ] \
     && awk -F'\t' '$1 == "G-VALGRIND[2/3]" && $2 == "SHARD" && $3 == 1 {f=1} END {exit !f}' "$CST_BASE/shard/target/ci-run/ci-timings.tsv"; then
    cst_ok "D a RED shard (2/3) fails G-VALGRIND; 3 shards side by side in one dir, a timing row per shard"
  else cst_bad "D shard failure (exit $CST_RC)"; fi
  : > "$CST_BASE/shard/vg_fail"
  CST_ENV="CI_VALGRIND_SHARDS=3" cst_run shard --jobs 4 --heavy-cap 3
  if [ "$CST_RC" = 0 ] && cst_has "+ G-VALGRIND"; then cst_ok "D control: all shards green, the gate is green"
  else cst_bad "D all-green shards (exit $CST_RC)"; fi
  # shellcheck disable=SC2016  # the scheduler's code, literally
  CST_MUT_FILE=ci_sched.sh CST_MUT_OLD='  for r in ${GS_RCS[$i]}; do' CST_MUT_NEW='  rc="${GS_RCS[$i]##* }"; for r in; do' cst_make shard-mut "$R"
  echo 2 > "$CST_BASE/shard-mut/vg_fail"
  CST_ENV="CI_VALGRIND_SHARDS=3" cst_run shard-mut --jobs 4 --heavy-cap 3
  if [ "$CST_RC" != 1 ]; then cst_ok "D control: taking the last shard's rc hides shard 2's RED, noticed"
  else cst_bad "D control: a last-shard aggregator went unnoticed"; fi

  # E — the change scope over the real workspace (functions, no run).
  cst_scope_arms

  # F — --changed-since end to end in a fixture repository.
  cst_scope_e2e

  # G — --fast keeps its exit 4 and names what it skipped.
  R='G6|PROGRESS|heavy|-|bash $GATES/fake.sh G6 0 0
K1|HARD|light|-|bash $GATES/fake.sh K1 0 0'
  cst_make fast "$R"
  for h in 1 3; do
    cst_run fast --fast --jobs "$h"
    if [ "$CST_RC" = 4 ] && cst_has "### gate G6  [SKIPPED by --fast]" && cst_has "    - G6" && cst_has "FAST-SUBSET GREEN" && ! cst_has "CI RESULT: PASS"; then
      cst_ok "G --fast --jobs $h: exit 4, G6 named as skipped, no PASS banner"
    else cst_bad "G --fast --jobs $h (exit $CST_RC)"; fi
  done

  # H — --prune-targets.
  R='P1|HARD|light|dir=pruneme|bash $GATES/fake.sh P1 0 0
P2|HARD|light|dir=second|bash $GATES/fake.sh P2 0 0'
  cst_make prune "$R"
  local tr="$CST_BASE/prune/target/ci-targets"
  mkdir -p "$tr/handmade"
  cst_run prune --jobs 2
  if [ -f "$tr/pruneme/$CI_TARGET_MARK" ] && [ -d "$tr/second" ]; then cst_ok "H without --prune-targets the dirs stay"
  else cst_bad "H dirs without --prune-targets"; fi
  cst_run prune --jobs 2 --prune-targets
  if [ ! -d "$tr/pruneme" ] && [ ! -d "$tr/second" ] && [ -d "$tr/handmade" ] && cst_has "(--prune-targets: deleted $tr/pruneme)"; then
    cst_ok "H --prune-targets deletes each gate's marked dir when it ends"
  else cst_bad "H --prune-targets"; fi
  CST_OUT="$( (CI_TARGET_ROOT="$tr"; ci_prune_target handmade; echo "rc=$?"; ci_prune_target '../x'; echo "rc=$?"
              CI_TARGET_ROOT="$CST_BASE/prune/elsewhere"; ci_prune_target handmade; echo "rc=$?") 2>&1)"
  if [ -d "$tr/handmade" ] && [ "$(printf '%s\n' "$CST_OUT" | grep -c '^rc=1$')" = 3 ] && cst_has "no $CI_TARGET_MARK"; then
    cst_ok "H control: an unmarked dir, a non-plain name and a foreign root are each refused"
  else cst_bad "H prune refusals"; fi

  echo "--- G-CI-SELF: $CST_PASS ok, $CST_FAIL failed (fixtures in $CST_BASE) ---"
  [ "$CST_FAIL" -eq 0 ] || { echo "G-CI-SELF RED"; return 1; }
  echo "G-CI-SELF GREEN"
  return 0
}

# E: the classification and closure on the real workspace metadata.
cst_scope_arms() {
  local list="$CST_BASE/scope-paths.txt" tri x y z
  # A crate X with a direct dependent Y and a dependent Z of Y that does not
  # depend on X directly: Z must be in X's closure.
  tri="$( (cd "$ROOT" && cargo metadata --no-deps --format-version 1 --offline 2>/dev/null) | "$PY" -c '
import json, sys
d = json.load(sys.stdin); m = set(d["workspace_members"])
pk = [p for p in d["packages"] if p["id"] in m and "/crates/" in p["manifest_path"]]; names = {p["name"] for p in pk}
deps = {p["name"]: {x["name"] for x in p["dependencies"] if x["name"] in names and x.get("path")} for p in pk}
for x in sorted(names):
    for y in sorted(n for n in names if x in deps[n]):
        for z in sorted(n for n in names if y in deps[n] and x not in deps[n]):
            if z != x: print(x, y, z); sys.exit(0)
')"
  read -r x y z <<< "$tri"
  if [ -z "$z" ]; then CST_OUT="$tri"; cst_bad "E no transitive dependency chain found in cargo metadata"; return; fi
  printf 'crates/%s/src/lib.rs\n' "$x" > "$list"
  ci_scope_from_list "$list"
  if [ "$SCOPE_MODE" = crates ] && in_words "$x" "$SCOPE_CRATES" && in_words "$y" "$SCOPE_CRATES" && in_words "$z" "$SCOPE_CRATES"; then
    cst_ok "E a change in $x selects $y (direct) and $z (through $y)"
  else CST_OUT="$SCOPE_MODE: $SCOPE_CRATES"; cst_bad "E transitive closure of $x"; fi
  # Control: a copy of this file whose closure stops after one level.
  local mutd="$CST_BASE/scope-mut"; mkdir -p "$mutd"
  CST_OUT="$("$PY" - "$ROOT/.port/tools/ci_sched.sh" "$mutd/ci_sched.sh" <<'PY'
import sys
s = open(sys.argv[1]).read()
s = s[:s.index("\n# ---- G-CI-SELF (ci.sh --self-test)")]
old = "            closure.add(r); todo.append(r)"
assert s.count(old) == 1
open(sys.argv[2], "w").write(s.replace(old, "            closure.add(r)"))
PY
)" || { cst_bad "E control: could not build the mutated copy"; return; }
  CST_OUT="$( (. "$mutd/ci_sched.sh"; ci_scope_from_list "$list"; echo "$SCOPE_CRATES") 2>&1)"
  if ! in_words "$z" "$CST_OUT"; then cst_ok "E control: a one-level closure misses $z, noticed"
  else cst_bad "E control: a one-level closure went unnoticed"; fi
  # Conservative widening, and docs/records selecting nothing.
  local p want
  for p in ".port/tools/ci.sh|all" ".port/tools/gates/check_leaks.sh|all" "Cargo.lock|all" "rust-toolchain.toml|all" \
           "crates/$x/build.rs|all" "crates-sys/zlib-sys/Cargo.toml|all" ".port/phase-2/lsan.supp|all" "include/rustty/vt.h|all" \
           "valgrind.supp|all" "docs/ghostty-bug-fixes.md|none" ".port/parity-closure/NEW-FINDINGS.md|none" \
           ".port/phase-8/DECISIONS.md|none" "README.md|none" "crates/$x/README.md|crates"; do
    want="${p#*|}"; printf '%s\n' "${p%|*}" > "$list"
    ci_scope_from_list "$list"
    if [ "$SCOPE_MODE" = "$want" ]; then cst_ok "E ${p%|*} -> $want"
    else CST_OUT="$SCOPE_MODE: $SCOPE_REASON"; cst_bad "E ${p%|*} -> $SCOPE_MODE, want $want"; fi
  done
  printf '%s\n' docs/a.md crates/$x/src/lib.rs > "$list"; ci_scope_from_list "$list"
  if [ "$SCOPE_MODE" = crates ]; then cst_ok "E docs next to a crate change: the crate's closure"
  else cst_bad "E docs + crate"; fi
  "$PY" - "$ROOT/.port/tools/ci_sched.sh" "$mutd/ci_sched.sh" <<'PY'
import sys
s = open(sys.argv[1]).read()
s = s[:s.index("\n# ---- G-CI-SELF (ci.sh --self-test)")]
old = 'GLOBAL_PREFIX = (".port/tools/", '
assert s.count(old) == 1
open(sys.argv[2], "w").write(s.replace(old, 'GLOBAL_PREFIX = ('))
PY
  printf '%s\n' .port/tools/ci.sh > "$list"
  CST_OUT="$( (. "$mutd/ci_sched.sh"; ci_scope_from_list "$list"; echo "mode=$SCOPE_MODE") 2>&1)"
  if ! cst_has "mode=all"; then cst_ok "E control: .port/tools/ dropped from the global list is noticed ($(printf '%s' "$CST_OUT" | tail -1))"
  else cst_bad "E control: .port/tools/ not global went unnoticed"; fi
}

# F: a fixture git repo holding a 3-crate workspace (b -> a, c alone) and a
# ci.sh copy whose registry has each scope kind. Every scenario commits its
# ci.sh copy first, so only the scenario's own change is "changed since HEAD".
cst_e2e_setup() {  # cst_e2e_setup REGISTRY (CST_MUT_* pass through to cst_make)
  local d="$CST_BASE/e2e" c
  cst_make e2e "$1" || return 1
  printf '[workspace]\nmembers = ["crates/a", "crates/b", "crates/c"]\nresolver = "2"\n' > "$d/Cargo.toml"
  for c in a b c; do
    mkdir -p "$d/crates/$c/src"
    if [ "$c" = b ]; then printf '[package]\nname = "b"\nversion = "0.1.0"\nedition = "2021"\n[dependencies]\na = { path = "../a" }\n' > "$d/crates/$c/Cargo.toml"
    else printf '[package]\nname = "%s"\nversion = "0.1.0"\nedition = "2021"\n' "$c" > "$d/crates/$c/Cargo.toml"; fi
    printf '// fixture\n' > "$d/crates/$c/src/lib.rs"
  done
  mkdir -p "$d/docs"; printf 'fixture\n' > "$d/docs/notes.md"
  printf '/target/\n/events\n/vg_fail\n' > "$d/.gitignore"
  ( cd "$d" && { [ -d .git ] || git init -q; } && cargo generate-lockfile --offline -q \
      && git add -A && git -c user.name=ci -c user.email=ci@localhost commit -q --allow-empty -m scenario ) >/dev/null 2>&1
}
cst_e2e_revert() { ( cd "$CST_BASE/e2e" && git checkout -q -- . ) >/dev/null 2>&1; }

cst_scope_e2e() {
  local R d="$CST_BASE/e2e"
  R='G6|PROGRESS|heavy|scope=env:G6_ONLY|bash $GATES/fake.sh G6 0 0
G6c|PROGRESS|heavy|scope=crate:a|bash $GATES/fake.sh G6c 0 0
GC|HARD|heavy|scope=crate:c|bash $GATES/fake.sh GC 0 0
G-VALGRIND|HARD|heavy|scope=args|bash $GATES/check_valgrind.sh
K1|HARD|light|-|bash $GATES/fake.sh K1 0 0'
  cst_e2e_setup "$R" || { CST_OUT=""; cst_bad "F could not build the fixture repository"; return; }
  printf '// changed\n' >> "$d/crates/a/src/lib.rs"
  cst_run e2e --changed-since HEAD --jobs 3
  if [ "$CST_RC" = 5 ] && cst_has "G6_ONLY=a b" && cst_has "crates: a b b:feat" && cst_has "~ G6c (progress/green)" \
     && cst_has "[SCOPED OUT by --changed-since: c is not in the change closure]" && cst_has "CHANGE-SCOPED GREEN" \
     && cst_has "~ G6 — narrowed" && cst_has "~ G-VALGRIND — narrowed" && ! cst_has "CI RESULT: PASS" \
     && cst_has "closure (changed + every reverse dependent): a b"; then
    cst_ok "F a change in crate a: G6/G-VALGRIND narrowed to {a, b}, crate:a runs in full, crate:c scoped out, exit 5, no PASS banner"
  else cst_bad "F crate change (exit $CST_RC)"; fi
  cst_e2e_revert
  CST_MUT_FILE=ci.sh CST_MUT_OLD='if [ "$(( ${#SCOPED_GATES[@]} + ${#SCOPED_OUT[@]} ))" -ne 0 ]; then' CST_MUT_NEW='if false; then' \
    cst_e2e_setup "$R"
  printf '// changed\n' >> "$d/crates/a/src/lib.rs"
  cst_run e2e --changed-since HEAD --jobs 3
  if [ "$CST_RC" != 5 ] && cst_has "CI RESULT: PASS"; then cst_ok "F control: without the scoped banner the run reads PASS (exit $CST_RC), noticed"
  else cst_bad "F control: a missing scoped banner went unnoticed (exit $CST_RC)"; fi
  cst_e2e_revert
  # RED inside the scope is still RED.
  cst_e2e_setup "${R/G6 0 0/G6 1 0}"; printf '// changed\n' >> "$d/crates/a/src/lib.rs"
  cst_run e2e --changed-since HEAD --jobs 3
  if [ "$CST_RC" = 1 ] && cst_has "X G6 (claimed-done regression, exit 1)"; then cst_ok "F a RED long gate inside the scope fails the run (exit 1)"
  else cst_bad "F RED in scope (exit $CST_RC)"; fi
  cst_e2e_revert
  # SCOPED-GREEN (exit 3 + marker) is SCOPED; a plain 3 is a WARNING.
  cst_e2e_setup "${R/G6 0 0/G6 3 0 G6 SCOPED-GREEN (fixture)}"; printf '// changed\n' >> "$d/crates/a/src/lib.rs"
  cst_run e2e --changed-since HEAD --jobs 3
  if [ "$CST_RC" = 5 ] && cst_has "~ G6 — narrowed"; then cst_ok "F a narrowed gate's exit 3 with SCOPED-GREEN is SCOPED"
  else cst_bad "F SCOPED-GREEN (exit $CST_RC)"; fi
  cst_e2e_revert
  cst_e2e_setup "${R/G6 0 0/G6 3 0 BLOCKED: no miri}"; printf '// changed\n' >> "$d/crates/a/src/lib.rs"
  cst_run e2e --changed-since HEAD --jobs 3
  if cst_has "! G6 BLOCKED (exit 3)" && cst_has "reason: BLOCKED: no miri" && ! cst_has "~ G6 — narrowed"; then
    cst_ok "F control: a narrowed gate's plain exit 3 stays a BLOCKED WARNING"
  else cst_bad "F plain BLOCKED in scope (exit $CST_RC)"; fi
  cst_e2e_revert
  # docs only: every scoped gate out; tooling: a full run.
  cst_e2e_setup "$R"; printf 'more\n' >> "$d/docs/notes.md"
  cst_run e2e --changed-since HEAD --jobs 3
  if [ "$CST_RC" = 5 ] && cst_has "mode: none" && cst_has "4 had nothing in it" && cst_has "+ K1"; then
    cst_ok "F a docs-only change: the 4 scoped gates scoped out, K1 still runs, exit 5"
  else cst_bad "F docs-only (exit $CST_RC)"; fi
  cst_e2e_revert
  printf '# touched\n' >> "$d/.port/tools/gates/fake.sh"
  cst_run e2e --changed-since HEAD --jobs 3
  if [ "$CST_RC" = 0 ] && cst_has "mode: all" && cst_has "CI RESULT: PASS" && cst_has "this was a full run" && ! cst_has "G6_ONLY="; then
    cst_ok "F a change under .port/tools/: every crate, no narrowing, a full run (exit 0)"
  else cst_bad "F tooling change (exit $CST_RC)"; fi
  cst_e2e_revert
}
