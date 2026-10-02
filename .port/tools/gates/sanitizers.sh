#!/usr/bin/env bash
# sanitizers.sh — sanitizer matrix gate. Complements Miri (R6: right tool per job).
#
# Miri catches Stacked-Borrows/provenance/uninit/leak on pure Rust but is ~100-400x
# native (a few tests are intractable). The sanitizers run at ~2x native and catch
# real memory CORRUPTION (UAF/OOB/double-free) + races across ALL tests incl. the
# Miri-intractable ones, and cover the C-FFI boundary Miri can't model.
#
# Platform support (re-verified P7-W14, 2026-10-01; aarch64-apple-darwin dev box
# and the aarch64 Fedora VM `rvm`):
#   address (ASan)  ✅ macOS + Linux                         -> G-ASAN
#   leak    (LSan)  ❌ macOS (rustc-rejected; Apple ASan has no leak checker)
#                   ✅ Linux, run as ASan with detect_leaks=1 -> G-LSAN
#   memory  (MSan)  ❌ macOS ; Linux, needs -Zbuild-std + instrumented C -> G-MSAN
#                   (aarch64: see MSAN_AARCH64_NOTE below)
#   thread  (TSan)  ✅ macOS + Linux  (needs -Zbuild-std)  -> G-TSAN, not yet a gate
#   hwaddress(HWASan)❌ macOS ; aarch64-Linux/Android only -> not a gate
#   cfi             mitigation, not a bug-checker; needs -Clto -> optional Goal-2 hardening
#
# Design: docs/superpowers/specs/2026-10-01-memory-safety-lanes-design.md, section B.
#
# MODES
#   sanitizers.sh <crate> [filter] [sanitizer ...]
#       LEGACY single-crate run (check_leaks.sh's G6 ASan companion arms call it
#       this way). Sanitizers are rustc names: address thread leak memory hwaddress.
#       Prints "SANITIZERS: GREEN|PROBLEM"; exit 0 / 1.
#   sanitizers.sh --matrix --san <asan|lsan|msan> (--crates <a,b c> | --workspace)
#                 [--filter <f>] [--exclude <crate>]...
#       The MATRIX runner: one crate at a time, each in its OWN target dir
#       (target/san-<san>-<crate>), deleted after that crate, so the disk holds
#       one sanitizer build at a time. A crate spec may carry features:
#       `rustty-apprt:gtk,wayland,x11`. Per-crate logs stay in target/san-logs/<san>/.
#   sanitizers.sh --gate <asan|lsan|msan>
#       The ci.sh gates G-ASAN / G-LSAN / G-MSAN: the matrix over the workspace
#       minus that sanitizer's cited exclusions, plus (asan, lsan) the Miri-
#       exception cross-check (every `cfg_attr(miri, ignore = "...")` test whose
#       reason claims ASan coverage must have RUN and PASSED here).
#   sanitizers.sh --self-test
#       G-SAN-SELF: a synthetic crate with a deliberate heap overflow (RED under
#       ASan), leak (RED under LSan), and uninitialized read (RED under MSan); a
#       clean control (GREEN); an empty filter (vacuous RED); and the log
#       classifier and cross-check over canned logs.
#
# EXIT CONTRACT (ci.sh): 0 GREEN ; 1 RED (a sanitizer finding, a test failure,
# zero tests run, a stale exclusion, an unverified ASan claim) ; 3 BLOCKED (no
# nightly, wrong host OS for the sanitizer, a crate that failed to BUILD, or the
# disk floor reached). RED dominates BLOCKED. A build failure is BLOCKED, never
# GREEN (design §B "common rules").
#
# DISK. Every sanitizer build is a full dependency rebuild (5-15 GB for the
# heavy crates). Before each crate the runner checks the free space on the
# target dir's volume against SAN_MIN_FREE_GB (default 15) and stops BLOCKED
# below it, rather than filling the disk mid-build.
#
# Bash 3.2 compatible (macOS /bin/bash): no mapfile, no assoc arrays, no bare
# expansion of a possibly-empty array under `set -u`.
set -u

GATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$GATES_DIR/../../.." && pwd)"
OS="$(uname -s)"
ARCH="$(uname -m)"
PY="${PYTHON:-python3}"
SUPP_DIR="$ROOT/.port/phase-2"
LSAN_SUPP="$SUPP_DIR/lsan.supp"
SAN_TARGET_ROOT="${SAN_TARGET_ROOT:-$ROOT/target}"
SAN_LOG_ROOT="${SAN_LOG_ROOT:-$ROOT/target/san-logs}"
SAN_MIN_FREE_GB="${SAN_MIN_FREE_GB:-15}"
# SAN_MANIFEST_PATH: run against another workspace (the self-test's synthetic
# crate). Empty = this repository.
SAN_MANIFEST_PATH="${SAN_MANIFEST_PATH:-}"

host_target() { rustc +nightly -vV 2>/dev/null | sed -n 's/host: //p'; }

# ---------------------------------------------------------------------------
# EXCLUSIONS. Format: "<crate>|<reason>". Only crates that CANNOT run under the
# sanitizer, each with a cited reason. An entry naming a crate that is not a
# workspace member is RED (a rename would otherwise leave an exclusion that
# excludes nothing while the renamed crate silently joins the run — or worse,
# hides that it never did).
# ---------------------------------------------------------------------------
ASAN_EXCLUDES=(
)
LSAN_EXCLUDES=(
)
# Feature lanes G-ASAN and G-LSAN add on top of the members (a plain
# `cargo test -p` never turns them on). Format: "<crate>:<features>|<reason>".
FEATURE_LANES=(
  "rustty-vt:wuffs|the PNG decode tests (\`image_load_png_zlib_compressed_direct\`, \`sys::tests::default_decoder_*\`) are ignored without the wuffs feature, and one of them is a Miri exception whose stated compensating control is ASan; found by G-ASAN's claim cross-check on its first run (P7-W14)"
)
# Linux-only feature lanes: the same set G-VALGRIND keeps (N413).
LINUX_FEATURE_LANES=(
  "rustty-apprt:gtk,wayland,x11|GTK is the reference's Linux runtime, so its test executable there compiles the GTK apprt; cargo test --workspace does not"
  "rustty-os:flatpak|the Flatpak arm of rustty-os (links gio-2.0, Linux/FreeBSD only); cargo test --workspace does not build it"
)
# MSan phase-1 scope is an ALLOW-list, not workspace-minus-excludes: MSan
# reports any read of memory written by uninstrumented code, so a crate whose
# test binary links an uninstrumented C/C++ library cannot run under it at all.
# Every workspace member must be either in MSAN_SCOPE or in MSAN_EXCLUDES (with
# its reason); one in neither is RED.
MSAN_SCOPE=(
)
MSAN_EXCLUDES=(
)

# ---------------------------------------------------------------------------
# Log classification. classify LOG RC -> prints one of
#   FINDING | BUILD | FAIL | VACUOUS | GREEN  and, for GREEN, the test count.
# ---------------------------------------------------------------------------
FINDING_RE='ERROR: (Address|Thread|Leak|Memory|HWAddress)Sanitizer|WARNING: (MemorySanitizer|ThreadSanitizer)|SUMMARY: [A-Za-z]*Sanitizer|runtime error:'
tests_ran() { awk '/^running [0-9]+ tests?$/ {s+=$2} END {print s+0}' "$1"; }
classify() {
  local log="$1" rc="$2" ran
  if grep -qE "$FINDING_RE" "$log"; then echo FINDING; return; fi
  ran="$(tests_ran "$log")"
  if [ "$rc" -ne 0 ]; then
    # Tests that ran and failed are RED; a run that never reached a test
    # binary (compile/link error) is BLOCKED.
    if grep -qE '^test result: FAILED|^error: test failed|^failures:$|process didn.t exit successfully: .*(signal|exit status)' "$log" && [ "$ran" -gt 0 ]; then
      echo FAIL; return
    fi
    if grep -qE 'error(\[E[0-9]+\])?: |could not compile|linking with .* failed' "$log"; then
      echo BUILD; return
    fi
    echo FAIL; return
  fi
  if [ "$ran" -eq 0 ]; then echo VACUOUS; return; fi
  echo "GREEN $ran"
}

# ---------------------------------------------------------------------------
# One crate under one sanitizer. run_crate SAN SPEC FILTER TD LOG
# SAN is a matrix id (asan|lsan|msan) or a legacy rustc name.
# ---------------------------------------------------------------------------
run_crate() {
  local san="$1" spec="$2" filter="$3" td="$4" log="$5"
  local crate="${spec%%:*}" feats="" tgt rflags="" build_std="" rc
  case "$spec" in *:*) feats="${spec#*:}" ;; esac
  tgt="$(host_target)"
  local -a envs=() cargo_args=()
  # allocator_may_return_null=1: ASan's malloc otherwise ABORTS on a request
  # larger than its 1 TiB maximum, where the system malloc returns NULL. The
  # port's out-of-memory tests rely on that NULL (`Vec::try_reserve_exact`
  # reporting failure, the faithful port of Zig's error.OutOfMemory paths; e.g.
  # rustty-vt's hyperlink `try_dupe_bytes_reports_allocation_failure` asks for
  # isize::MAX bytes). It changes no detector: every memory error is still
  # reported. First seen as G-ASAN's first-run "allocation-size-too-big" (P7-W14).
  case "$san" in
    asan|address)
      rflags="-Zsanitizer=address"
      envs+=("ASAN_OPTIONS=detect_leaks=0:allocator_may_return_null=1") ;;  # WHY: Apple ASan has no leak checker (rustc rejects -Zsanitizer=leak on Darwin); on Linux, G-LSAN is the leak lane, so G-ASAN keeps its verdict to memory errors only
    lsan)
      rflags="-Zsanitizer=address"
      envs+=("ASAN_OPTIONS=detect_leaks=1:allocator_may_return_null=1")      # WHY: not suppressive — turns the leak checker ON; this IS G-LSAN (design §B: LSan run as ASan with detect_leaks=1)
      envs+=("LSAN_OPTIONS=suppressions=$LSAN_SUPP:print_suppressions=1") ;;  # WHY: every entry in lsan.supp is a third-party leak with a cited reason in .port/phase-2/SANITIZER_SUPPRESSIONS.md; print_suppressions=1 lists every match in the log so a suppression is never silent
    msan|memory)
      rflags="-Zsanitizer=memory -Zsanitizer-memory-track-origins"
      build_std="-Zbuild-std"
      # The C we compile ourselves through `cc` is instrumented too; anything
      # uninstrumented (system libraries) is out of MSan scope by construction.
      envs+=("CC=clang" "CXX=clang++" "CFLAGS=-fsanitize=memory -fsanitize-memory-track-origins -fno-omit-frame-pointer -g")
      envs+=("CXXFLAGS=-fsanitize=memory -fsanitize-memory-track-origins -fno-omit-frame-pointer -g")
      envs+=("MSAN_OPTIONS=poison_in_dtor=1:allocator_may_return_null=1") ;;
    thread)
      rflags="-Zsanitizer=thread"; build_std="-Zbuild-std" ;;
    leak)
      rflags="-Zsanitizer=leak" ;;
    hwaddress)
      rflags="-Zsanitizer=hwaddress" ;;
    *) echo "sanitizers.sh: unknown sanitizer '$san'" >&2; return 2 ;;
  esac
  # RUST_MIN_STACK (std's default spawn stack, 2 MiB otherwise): ASan and MSan
  # put redzones and shadow bookkeeping in every frame, so instrumented debug
  # frames are 2-3x their native size. A test that checks a recursion bound
  # FITS the default stack is checking native frame sizes; under ASan it
  # overflows without any memory error. Measured (P7-W14): rustty-vt's
  # `deeply_nested_layout_is_a_syntax_error_beyond_max_depth` parses MAX_DEPTH
  # (1000) levels in 1.25-1.5 MiB natively (debug) and overflows the 2 MiB stack
  # under ASan. 8 MiB keeps that headroom; unbounded recursion still overflows,
  # and the native bound stays checked by `cargo test --workspace`. Threads
  # that set an explicit stack size are unaffected.
  if [ "$san" != leak ] && [ "$san" != hwaddress ]; then envs+=("RUST_MIN_STACK=8388608"); fi
  if [ -n "$SAN_MANIFEST_PATH" ]; then cargo_args+=(--manifest-path "$SAN_MANIFEST_PATH"); fi
  cargo_args+=(-p "$crate" --target "$tgt" --no-fail-fast)
  if [ -n "$feats" ]; then cargo_args+=(--features "$feats"); fi
  if [ -n "$build_std" ]; then cargo_args+=("$build_std"); fi
  mkdir -p "$(dirname "$log")"
  # shellcheck disable=SC2086  # FILTER is an intentional word list (libtest takes several)
  env "${envs[@]}" RUSTFLAGS="$rflags" RUSTDOCFLAGS="$rflags" CARGO_TARGET_DIR="$td" \
    cargo +nightly test "${cargo_args[@]}" -- $filter >"$log" 2>&1
  rc=$?
  return $rc
}

# ---------------------------------------------------------------------------
# LEGACY single-crate mode (kept byte-compatible for check_leaks.sh).
# ---------------------------------------------------------------------------
legacy_main() {
  local CRATE="$1" FILTER="${2:-}"
  shift 2 2>/dev/null || shift $# 2>/dev/null
  local SANS
  if [ "$#" -gt 0 ]; then SANS="$*"
  elif [ "$OS" = "Darwin" ]; then SANS="address thread"
  else SANS="address thread leak"; fi
  local fail=0 s
  for s in $SANS; do
    case "$s" in
      leak|hwaddress) if [ "$OS" = "Darwin" ]; then echo "  ⏭  $s: skipped (Linux-only)"; continue; fi ;;
    esac
    local td="/tmp/san-gate-$CRATE-$s" log="/tmp/san-gate-$CRATE-$s.log" rc v
    echo "### sanitizer=$s crate=$CRATE ###"
    run_crate "$s" "$CRATE" "$FILTER" "$td" "$log"; rc=$?
    v="$(classify "$log" "$rc")"
    # NON-VACUITY + HONEST REPORTING (miri-reaudit, 2026-07-31): the green line
    # once echoed the LAST `test result` (the empty Doc-tests summary), so a
    # filter matching nothing looked identical to a real pass. The count is the
    # sum of every target's `running N tests` header; zero is RED.
    case "$v" in
      FINDING) echo "  ❌ $s: SANITIZER FINDING"; grep -E 'ERROR:|SUMMARY:|runtime error:' "$log" | head -4; fail=1 ;;
      BUILD|FAIL) echo "  ⚠️  $s: build/test failed rc=$rc — $(grep -m1 -iE 'error(\[|:)' "$log" | head -c 100)"; fail=1 ;;
      VACUOUS) echo "  ❌ $s: VACUOUS — 0 tests ran (filter '${FILTER:-<none>}' matched nothing); a green over an empty set is not a pass"; fail=1 ;;
      GREEN*) echo "  ✅ $s: ${v#GREEN } test(s) ran — $(grep 'test result' "$log" | tr '\n' ';' | sed 's/;$//')" ;;
    esac
  done
  if [ $fail -eq 0 ]; then echo "SANITIZERS: GREEN ($CRATE: $SANS)"; else echo "SANITIZERS: PROBLEM ($CRATE)"; fi
  return $fail
}

# ---------------------------------------------------------------------------
# Matrix helpers.
# ---------------------------------------------------------------------------
free_gb() { df -Pk "$1" 2>/dev/null | awk 'NR==2 {printf "%d", $4/1048576}'; }

# Delete one crate's sanitizer target dir. Guarded: only a path of the exact
# shape <SAN_TARGET_ROOT>/san-<san>-<crate> is ever removed (no variable can
# widen this to the repository or the shared target dir).
drop_target() {
  local td="$1"
  case "$td" in
    "$SAN_TARGET_ROOT"/san-asan-*|"$SAN_TARGET_ROOT"/san-lsan-*|"$SAN_TARGET_ROOT"/san-msan-*) ;;
    *) echo "sanitizers.sh: refusing to delete '$td' (not a san-<san>-<crate> dir)" >&2; return 1 ;;
  esac
  case "$td" in *..*|*' '*) echo "sanitizers.sh: refusing to delete '$td'" >&2; return 1 ;; esac
  [ -d "$td" ] && rm -rf -- "$td"
  return 0
}

workspace_members() {
  local mp=()
  if [ -n "$SAN_MANIFEST_PATH" ]; then mp=(--manifest-path "$SAN_MANIFEST_PATH"); fi
  (cd "$ROOT" && cargo metadata --no-deps --format-version 1 ${mp[@]+"${mp[@]}"} 2>/dev/null) |
    "$PY" -c 'import json,sys; print("\n".join(sorted(p["name"] for p in json.load(sys.stdin)["packages"])))'
}

# Matrix result accumulators (globals: bash 3.2 has no namerefs).
M_RED=0; M_BLOCKED=0; M_TOTAL=0; M_ROWS=""
matrix_run() {  # matrix_run SAN FILTER SPEC...
  local san="$1" filter="$2"; shift 2
  local spec crate td log rc v t0 t1 safe fg
  echo "=== sanitizer matrix: $san over $# crate spec(s); target dirs under $SAN_TARGET_ROOT/san-$san-<crate>, deleted after each ==="
  for spec in "$@"; do
    crate="${spec%%:*}"
    safe="$(printf '%s' "$spec" | tr ':,' '__')"
    td="$SAN_TARGET_ROOT/san-$san-$safe"
    log="$SAN_LOG_ROOT/$san/$safe.log"
    mkdir -p "$SAN_TARGET_ROOT"
    fg="$(free_gb "$SAN_TARGET_ROOT")"
    if [ -n "$fg" ] && [ "$fg" -lt "$SAN_MIN_FREE_GB" ]; then
      echo "  ⛔ BLOCKED: $fg GB free under $SAN_TARGET_ROOT, below the SAN_MIN_FREE_GB=$SAN_MIN_FREE_GB floor; stopping before $spec"
      M_BLOCKED=1
      M_ROWS="$M_ROWS$spec|BLOCKED(disk)|0|0"$'\n'
      return
    fi
    t0=$(date +%s)
    run_crate "$san" "$spec" "$filter" "$td" "$log"; rc=$?
    t1=$(date +%s)
    v="$(classify "$log" "$rc")"
    drop_target "$td"
    case "$v" in
      GREEN*)
        M_TOTAL=$((M_TOTAL + ${v#GREEN }))
        echo "  ✅ $san $spec: ${v#GREEN } test(s) ran, $((t1 - t0))s — $(grep '^test result' "$log" | awk '{p+=$4; i+=$8} END {printf "%d passed, %d ignored", p, i}')"
        M_ROWS="$M_ROWS$spec|GREEN|${v#GREEN }|$((t1 - t0))"$'\n' ;;
      FINDING)
        M_RED=1
        echo "  ❌ $san $spec: SANITIZER FINDING ($((t1 - t0))s) — log $log"
        grep -E "$FINDING_RE" "$log" | sort -u | head -6 | sed 's/^/       /'
        M_ROWS="$M_ROWS$spec|FINDING|$(tests_ran "$log")|$((t1 - t0))"$'\n' ;;
      FAIL)
        M_RED=1
        echo "  ❌ $san $spec: tests FAILED under the sanitizer, rc=$rc ($((t1 - t0))s) — log $log"
        grep -E '^test .* FAILED$|^error: test failed|signal:' "$log" | head -6 | sed 's/^/       /'
        M_ROWS="$M_ROWS$spec|FAIL|$(tests_ran "$log")|$((t1 - t0))"$'\n' ;;
      VACUOUS)
        M_RED=1
        echo "  ❌ $san $spec: VACUOUS — 0 tests ran (filter '${filter:-<none>}'); a green over an empty set is not a pass"
        M_ROWS="$M_ROWS$spec|VACUOUS|0|$((t1 - t0))"$'\n' ;;
      BUILD)
        M_BLOCKED=1
        echo "  ⛔ $san $spec: BLOCKED — did not build under the sanitizer, rc=$rc — log $log"
        grep -m3 -E 'error(\[E[0-9]+\])?: |linking with' "$log" | sed 's/^/       /'
        M_ROWS="$M_ROWS$spec|BLOCKED(build)|0|$((t1 - t0))"$'\n' ;;
    esac
  done
}

matrix_summary() {  # matrix_summary SAN
  echo "--- $1 matrix summary: $M_TOTAL test(s) ran ---"
  printf '%s' "$M_ROWS" | awk -F'|' 'NF {printf "  %-36s %-15s %6s tests %6ss\n", $1, $2, $3, $4}'
}

# Excluded-or-scoped completeness. check_members SAN SPEC_LIST EXCL_LIST
# Prints RED lines; returns 1 on a stale exclusion or (msan) an unlisted member.
check_members() {
  local mode="$1" members bad=0 e c
  members="$(workspace_members)"
  if [ -z "$members" ]; then echo "  ❌ cargo metadata returned no workspace members"; return 1; fi
  shift
  for e in "$@"; do
    c="${e%%|*}"; c="${c%%:*}"
    if ! printf '%s\n' "$members" | grep -qx "$c"; then
      echo "  ❌ $mode: '$c' is listed but is not a workspace member (stale entry; a rename must update it)"
      bad=1
    fi
  done
  return $bad
}

# The Miri-exception cross-check: every test whose `cfg_attr(miri, ignore =
# "...")` reason claims ASan coverage must appear as `test <path> ... ok` in
# this sanitizer's logs. Ignored or failed = RED; a claimed crate whose log is
# missing = RED; a test absent from a log that ran (cfg-gated off this host OS,
# e.g. kqueue tests on Linux) is listed BY NAME, and RED if it is every claim
# of its crate (a broken parse, not a platform gate).
asan_claims_crosscheck() {  # SAN
  ROOT="$ROOT" LOGDIR="$SAN_LOG_ROOT/$1" "$PY" - <<'PYEOF'
import glob, json, os, re, subprocess, sys
root, logdir = os.environ["ROOT"], os.environ["LOGDIR"]
src_root = os.environ.get("SAN_CLAIMS_SRC", root)
attr = re.compile(r'#\[cfg_attr\(\s*miri\s*,\s*ignore\s*=\s*"((?:[^"\\]|\\.)*)"\s*\)\]', re.S)
claim = re.compile(r'ASan|AddressSanitizer|sanitizer', re.I)
fnre = re.compile(r'\bfn\s+([A-Za-z0-9_]+)')
# crate dir -> package name, from the manifests (the dir name is not always the package name).
dir2pkg = {}
for man in glob.glob(os.path.join(src_root, "crates", "**", "Cargo.toml"), recursive=True):
    if "/vendor/" in man or "/target/" in man:
        continue
    m = re.search(r'^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', open(man, encoding="utf-8").read(), re.M | re.S)
    if m:
        dir2pkg[os.path.dirname(man)] = m.group(1)
def pkg_of(path):
    d = os.path.dirname(path)
    while d.startswith(src_root) and d != src_root:
        if d in dir2pkg:
            return dir2pkg[d]
        d = os.path.dirname(d)
    return None
claims = []
for f in sorted(glob.glob(os.path.join(src_root, "crates", "**", "*.rs"), recursive=True)):
    if "/vendor/" in f or "/target/" in f:
        continue
    s = open(f, encoding="utf-8", errors="replace").read()
    for m in attr.finditer(s):
        if not claim.search(m.group(1)):
            continue
        fm = fnre.search(s, m.end())
        if fm:
            claims.append((pkg_of(f), os.path.relpath(f, src_root), fm.group(1)))
# Per-crate test outcomes from the logs (feature-lane logs fold into their crate).
outcome = {}
logs_seen = set()
for lg in glob.glob(os.path.join(logdir, "*.log")):
    # Log names are `<crate>` or `<crate>_<features>` (':' and ',' became '_');
    # workspace crate names use '-' only, so the first '_' starts the features.
    crate = os.path.basename(lg)[:-4].split("_", 1)[0]
    logs_seen.add(crate)
    for line in open(lg, encoding="utf-8", errors="replace"):
        m = re.match(r'^test (\S+) \.\.\. (ok|ignored|FAILED)', line)
        if m:
            name = m.group(1).rsplit("::", 1)[-1]
            prev = outcome.setdefault(crate, {}).get(name)
            if prev != "ok":
                outcome[crate][name] = m.group(2)
red, ok, absent = [], 0, []
by_crate = {}
for pkg, path, fn in claims:
    by_crate.setdefault(pkg, []).append((path, fn))
for pkg, items in sorted(by_crate.items(), key=lambda kv: str(kv[0])):
    if pkg not in logs_seen:
        red.append(f"{pkg}: {len(items)} ASan-claiming Miri exception(s) but the crate has no {os.path.basename(logdir)} log (excluded or not run): e.g. {items[0][0]}::{items[0][1]}")
        continue
    seen_here = 0
    for path, fn in items:
        st = outcome.get(pkg, {}).get(fn)
        if st == "ok":
            ok += 1; seen_here += 1
        elif st is None:
            absent.append(f"{pkg} {path}::{fn}")
        else:
            seen_here += 1
            red.append(f"{pkg} {path}::{fn} claims ASan coverage but was {st} under {os.path.basename(logdir)}")
    if seen_here == 0:
        red.append(f"{pkg}: none of its {len(items)} ASan-claiming tests appear in its log (a broken claim parse, or every claim cfg-gated off this host)")
print(f"  ASan-claim cross-check: {len(claims)} cfg_attr(miri, ignore) annotation(s) claim ASan coverage; {ok} ran and passed here; {len(absent)} not compiled on this host OS; {len(red)} problem(s)")
for a in absent:
    print(f"     not compiled on this host OS (cfg-gated; checked on the other OS's lane): {a}")
for r in red:
    print(f"  ❌ {r}")
if not claims:
    print("  ❌ no ASan-claiming annotations parsed at all (the annotation grammar changed?)")
    sys.exit(1)
sys.exit(1 if red else 0)
PYEOF
}

gate_verdict() {  # gate_verdict ID EXTRA_RED
  local id="$1" extra="$2"
  if [ "$M_RED" -ne 0 ] || [ "$extra" -ne 0 ]; then echo "$id: RED"; return 1; fi
  if [ "$M_BLOCKED" -ne 0 ]; then echo "$id: BLOCKED (a crate did not build, or the disk floor was reached; see above)"; return 3; fi
  if [ "$M_TOTAL" -eq 0 ]; then echo "$id: RED — VACUOUS, 0 tests ran"; return 1; fi
  echo "$id: GREEN ($M_TOTAL test(s) under the sanitizer, 0 findings)"
  return 0
}

# ci.yml's `ci` job sets RUSTTY_SANITIZERS_OWN_JOB: the gates run in its
# `sanitizers` job instead (one job per sanitizer), as G-VALGRIND runs in the
# `valgrind` job. BLOCKED here is a WARNING naming that job, never a pass.
own_job_blocked() {  # own_job_blocked ID
  if [ -n "${RUSTTY_SANITIZERS_OWN_JOB:-}" ]; then
    echo "BLOCKED: $1 — this job defers to $RUSTTY_SANITIZERS_OWN_JOB (RUSTTY_SANITIZERS_OWN_JOB is set), which runs it"
    exit 3
  fi
}

need_nightly() {
  if ! cargo +nightly --version >/dev/null 2>&1; then
    echo "BLOCKED: the +nightly toolchain is not installed (-Zsanitizer needs it)"; exit 3
  fi
}

# Build the spec list for a workspace gate: members minus EXCL (+ EXTRA lanes).
scope_minus() {  # scope_minus "excl entries" -> stdout specs
  local excl="$1" m
  for m in $(workspace_members); do
    printf '%s\n' "$excl" | grep -q "^$m|" && continue
    echo "$m"
  done
}

gate_asan_like() {  # gate_asan_like asan|lsan
  local san="$1" id excl extra=0 e
  if [ "$san" = asan ]; then id="G-ASAN"; else id="G-LSAN"; fi
  own_job_blocked "$id"
  need_nightly
  if [ "$san" = lsan ] && [ "$OS" != "Linux" ]; then
    echo "BLOCKED: $id needs LeakSanitizer, which exists on Linux only (rustc rejects -Zsanitizer=leak on $OS and Apple's ASan has no leak checker). It runs on the Linux VM (rvm) and in ci.yml's sanitizers job."
    exit 3
  fi
  if [ "$san" = lsan ] && [ ! -r "$LSAN_SUPP" ]; then
    echo "$id: RED — $LSAN_SUPP is missing (SANITIZER_SUPPRESSIONS.md's file)"; exit 1
  fi
  excl=""
  if [ "$san" = asan ]; then
    for e in ${ASAN_EXCLUDES[@]+"${ASAN_EXCLUDES[@]}"}; do excl="$excl$e"$'\n'; done
  else
    for e in ${LSAN_EXCLUDES[@]+"${LSAN_EXCLUDES[@]}"}; do excl="$excl$e"$'\n'; done
  fi
  echo "### $id: AddressSanitizer$( [ "$san" = lsan ] && echo ' + LeakSanitizer (detect_leaks=1)') over the workspace test suites ($OS/$ARCH) ###"
  if [ -n "$excl" ]; then
    echo "excluded (cannot run under the sanitizer):"
    printf '%s' "$excl" | awk -F'|' 'NF {printf "  - %s: %s\n", $1, $2}'
  fi
  local -a check=()
  while IFS= read -r e; do [ -n "$e" ] && check+=("$e"); done <<EOF
$excl
EOF
  check_members "$id exclusion" ${check[@]+"${check[@]}"} || extra=1
  rm -rf -- "${SAN_LOG_ROOT:?}/$san"
  local -a specs_a=()
  while IFS= read -r e; do [ -n "$e" ] && specs_a+=("$e"); done <<EOF
$(scope_minus "$excl")
EOF
  for e in ${FEATURE_LANES[@]+"${FEATURE_LANES[@]}"}; do specs_a+=("${e%%|*}"); echo "feature lane: ${e%%|*} — ${e#*|}"; done
  if [ "$OS" = "Linux" ]; then
    for e in ${LINUX_FEATURE_LANES[@]+"${LINUX_FEATURE_LANES[@]}"}; do specs_a+=("${e%%|*}"); echo "feature lane: ${e%%|*} — ${e#*|}"; done
  fi
  check_members "$id feature lane" ${FEATURE_LANES[@]+"${FEATURE_LANES[@]}"} ${LINUX_FEATURE_LANES[@]+"${LINUX_FEATURE_LANES[@]}"} || extra=1
  if [ -n "${SAN_ONLY:-}" ]; then
    # Debug aid: SAN_ONLY="a b" narrows the run (the verdict says so: never a full green).
    local -a narrowed=()
    for e in ${specs_a[@]+"${specs_a[@]}"}; do case " $SAN_ONLY " in *" ${e%%:*} "*) narrowed+=("$e") ;; esac; done
    specs_a=(${narrowed[@]+"${narrowed[@]}"})
    echo "NOTE: SAN_ONLY narrows this run to: ${specs_a[*]:-<nothing>} — the verdict is BLOCKED, never GREEN"
  fi
  matrix_run "$san" "" ${specs_a[@]+"${specs_a[@]}"}
  matrix_summary "$san"
  asan_claims_crosscheck "$san" || extra=1
  if [ -n "${SAN_ONLY:-}" ]; then M_BLOCKED=1; fi
  gate_verdict "$id" "$extra"
}

MSAN_AARCH64_NOTE=""
gate_msan() {
  own_job_blocked G-MSAN
  need_nightly
  if [ "$OS" != "Linux" ]; then
    echo "BLOCKED: G-MSAN needs MemorySanitizer, which exists on Linux only (not on $OS). It runs on the Linux VM (rvm) and in ci.yml's sanitizers job."
    exit 3
  fi
  if [ -n "$MSAN_AARCH64_NOTE" ] && [ "$ARCH" = "aarch64" ]; then
    echo "BLOCKED: $MSAN_AARCH64_NOTE"
    exit 3
  fi
  if ! rustup +nightly component list --installed 2>/dev/null | grep -q '^rust-src'; then
    echo "BLOCKED: G-MSAN needs -Zbuild-std, which needs the nightly rust-src component (rustup component add rust-src --toolchain nightly)"
    exit 3
  fi
  if ! command -v clang >/dev/null 2>&1; then
    echo "BLOCKED: G-MSAN compiles the vendored C with clang -fsanitize=memory; clang is not on PATH"
    exit 3
  fi
  echo "### G-MSAN: MemorySanitizer (-Zbuild-std, C via cc with -fsanitize=memory) over the phase-1 scope ($OS/$ARCH) ###"
  local e extra=0
  echo "excluded (link uninstrumented C/C++; MSan cannot run them):"
  for e in ${MSAN_EXCLUDES[@]+"${MSAN_EXCLUDES[@]}"}; do printf '  - %s: %s\n' "${e%%|*}" "${e#*|}"; done
  check_members "G-MSAN scope" ${MSAN_SCOPE[@]+"${MSAN_SCOPE[@]}"} || extra=1
  check_members "G-MSAN exclusion" ${MSAN_EXCLUDES[@]+"${MSAN_EXCLUDES[@]}"} || extra=1
  local m listed
  listed="$(printf '%s\n' ${MSAN_SCOPE[@]+"${MSAN_SCOPE[@]}"} ${MSAN_EXCLUDES[@]+"${MSAN_EXCLUDES[@]}"} | sed 's/[|:].*//')"
  for m in $(workspace_members); do
    printf '%s\n' "$listed" | grep -qx "$m" || { echo "  ❌ G-MSAN: workspace member '$m' is in neither MSAN_SCOPE nor MSAN_EXCLUDES (add it, or record why not)"; extra=1; }
  done
  rm -rf -- "${SAN_LOG_ROOT:?}/msan"
  matrix_run msan "" ${MSAN_SCOPE[@]+"${MSAN_SCOPE[@]}"}
  matrix_summary msan
  gate_verdict "G-MSAN" "$extra"
}

# ---------------------------------------------------------------------------
# SELF-TEST (G-SAN-SELF).
# ---------------------------------------------------------------------------
self_test() {
  need_nightly
  local st fail=0 out rc
  st="$(mktemp -d "${TMPDIR:-/tmp}/san-self.XXXXXX")"
  echo "### G-SAN-SELF: sanitizers.sh --self-test (work dir $st) ###"
  # --- arm C: the log classifier over canned logs (no build) ---
  mkdir -p "$st/logs"
  printf 'running 3 tests\ntest a ... ok\n==1==ERROR: AddressSanitizer: heap-buffer-overflow\nSUMMARY: AddressSanitizer: heap-buffer-overflow\n' >"$st/logs/finding.log"
  # shellcheck disable=SC2016  # literal backticks, as cargo prints them
  printf 'error[E0425]: cannot find value `x`\nerror: could not compile `foo`\n' >"$st/logs/build.log"
  # shellcheck disable=SC2016  # literal backticks, as cargo prints them
  printf 'running 2 tests\ntest a ... ok\ntest b ... FAILED\nfailures:\ntest result: FAILED. 1 passed; 1 failed\nerror: test failed, to rerun pass `--lib`\n' >"$st/logs/fail.log"
  printf 'running 0 tests\ntest result: ok. 0 passed; 0 failed\n' >"$st/logs/vacuous.log"
  printf 'running 4 tests\ntest result: ok. 4 passed\nrunning 1 test\ntest result: ok. 1 passed\n' >"$st/logs/green.log"
  printf 'running 1 test\n==9==WARNING: MemorySanitizer: use-of-uninitialized-value\n' >"$st/logs/msan.log"
  printf 'running 2 tests\n=================================================================\n==7==ERROR: LeakSanitizer: detected memory leaks\nSUMMARY: AddressSanitizer: 64 byte(s) leaked in 1 allocation(s).\n' >"$st/logs/leak.log"
  ck() {  # ck NAME RC WANT
    local got; got="$(classify "$st/logs/$1.log" "$2")"
    if [ "$got" = "$3" ]; then echo "  ok   C classifier: $1 (rc=$2) -> $got"; else echo "  FAIL C classifier: $1 (rc=$2) -> '$got', want '$3'"; fail=1; fi
  }
  ck finding 1 FINDING; ck build 101 BUILD; ck fail 101 FAIL; ck vacuous 0 VACUOUS
  ck green 0 "GREEN 5"; ck msan 77 FINDING; ck leak 23 FINDING
  # --- arm D: drop_target refuses anything but a san-<san>-<crate> dir ---
  if ( SAN_TARGET_ROOT="$st/t"; drop_target "$st" ) 2>/dev/null; then echo "  FAIL D drop_target accepted a non-sanitizer dir"; fail=1
  else echo "  ok   D drop_target refuses a non-sanitizer dir"; fi
  # --- arm E: the ASan-claim cross-check over a synthetic source + logs ---
  mkdir -p "$st/src/crates/foo/src" "$st/xlogs/asan"
  printf '[package]\nname = "foo"\n' >"$st/src/crates/foo/Cargo.toml"
  cat >"$st/src/crates/foo/src/lib.rs" <<'RS'
#[test]
#[cfg_attr(miri, ignore = "foreign function -- runs natively + the G-ASAN lane")]
fn claimed_and_ran() {}
#[test]
#[cfg_attr(
    miri,
    ignore = "foreign function -- covered by AddressSanitizer"
)]
fn claimed_but_ignored() {}
#[test]
#[cfg_attr(miri, ignore = "reads a file")]
fn not_a_claim() {}
RS
  printf 'running 3 tests\ntest tests::claimed_and_ran ... ok\ntest claimed_but_ignored ... ignored\ntest not_a_claim ... ok\n' >"$st/xlogs/asan/foo.log"
  out="$(SAN_CLAIMS_SRC="$st/src" SAN_LOG_ROOT="$st/xlogs" asan_claims_crosscheck asan 2>&1)"; rc=$?
  if [ $rc -eq 1 ] && printf '%s' "$out" | grep -q 'claimed_but_ignored claims ASan coverage but was ignored' && printf '%s' "$out" | grep -q '2 cfg_attr(miri, ignore) annotation(s) claim ASan coverage; 1 ran and passed'; then
    echo "  ok   E cross-check: an ignored ASan-claiming test is RED; the multi-line attribute parses; a non-claim is not counted"
  else echo "  FAIL E cross-check (rc=$rc): $out"; fail=1; fi
  printf 'running 3 tests\ntest tests::claimed_and_ran ... ok\ntest claimed_but_ignored ... ok\n' >"$st/xlogs/asan/foo.log"
  out="$(SAN_CLAIMS_SRC="$st/src" SAN_LOG_ROOT="$st/xlogs" asan_claims_crosscheck asan 2>&1)"; rc=$?
  if [ $rc -eq 0 ]; then echo "  ok   E control: both claims ran and passed -> GREEN"; else echo "  FAIL E control (rc=$rc): $out"; fail=1; fi
  rm -f "$st/xlogs/asan/foo.log"
  out="$(SAN_CLAIMS_SRC="$st/src" SAN_LOG_ROOT="$st/xlogs" asan_claims_crosscheck asan 2>&1)"; rc=$?
  if [ $rc -eq 1 ] && printf '%s' "$out" | grep -q 'has no asan log'; then echo "  ok   E a claimed crate with no log (excluded) is RED"
  else echo "  FAIL E missing-log (rc=$rc): $out"; fail=1; fi

  # --- arm F: the BLOCKED arms (no build) ---
  out="$(RUSTTY_SANITIZERS_OWN_JOB="the fixture job" bash "$GATES_DIR/sanitizers.sh" --gate asan 2>&1)"; rc=$?
  if [ $rc -eq 3 ] && printf '%s' "$out" | grep -q 'defers to the fixture job'; then echo "  ok   F RUSTTY_SANITIZERS_OWN_JOB is BLOCKED (exit 3), naming the job"
  else echo "  FAIL F OWN_JOB (rc=$rc): $out"; fail=1; fi
  if [ "$OS" != "Linux" ]; then
    for g in lsan msan; do
      out="$(env -u RUSTTY_SANITIZERS_OWN_JOB bash "$GATES_DIR/sanitizers.sh" --gate "$g" 2>&1)"; rc=$?
      if [ $rc -eq 3 ] && printf '%s' "$out" | grep -q 'exists on Linux only'; then echo "  ok   F --gate $g on $OS is BLOCKED (exit 3), Linux-only"
      else echo "  FAIL F --gate $g on $OS (rc=$rc): $out"; fail=1; fi
    done
  fi

  # --- arms A/B: the synthetic crate under the real sanitizers ---
  local cr="$st/crate"
  mkdir -p "$cr/src"
  cat >"$cr/Cargo.toml" <<'TOML'
[package]
name = "san-selftest"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]
TOML
  cat >"$cr/src/lib.rs" <<'RS'
//! G-SAN-SELF's synthetic crate: one deliberate defect per sanitizer, plus a
//! clean control. Each arm selects its test by exact name.
#[cfg(test)]
mod tests {
    #[inline(never)]
    fn oob_read(v: &[u8]) -> u8 {
        // 8 bytes past a 16-byte heap allocation: inside ASan's redzone.
        unsafe { *v.as_ptr().add(v.len() + 8) }
    }
    #[test]
    fn defect_heap_overflow() {
        let v: Vec<u8> = (0u8..16).collect();
        std::hint::black_box(oob_read(std::hint::black_box(&v)));
    }

    #[inline(never)]
    fn leak_some() {
        // Many unreachable blocks, so no stale register or stack slot can keep
        // them all reachable when LeakSanitizer scans at exit.
        for i in 0..64u8 {
            let b = Box::new([i; 256]);
            std::mem::forget(std::hint::black_box(b));
        }
    }
    #[test]
    fn defect_leak() {
        leak_some();
    }

    #[inline(never)]
    fn uninit_byte() -> u8 {
        let mut v: Vec<u8> = Vec::with_capacity(16);
        // SAFETY: deliberately NOT safe — reads uninitialized heap memory so
        // MemorySanitizer reports it. This crate exists only for the self-test.
        unsafe {
            v.set_len(16);
        }
        let b = v[std::hint::black_box(3)];
        std::hint::black_box(&v);
        b
    }
    #[test]
    fn defect_uninit_read() {
        if uninit_byte() == 7 {
            println!("seven");
        }
    }

    #[test]
    fn clean_control() {
        let v: Vec<u8> = (0u8..16).collect();
        assert_eq!(v.iter().map(|&b| b as u32).sum::<u32>(), 120);
    }
}
RS
  local sroot="$st/target"
  arm() {  # arm NAME SAN FILTER WANT(RED|GREEN) MUST_GREP
    local name="$1" san="$2" filter="$3" want="$4" must="$5" o r
    o="$(SAN_MANIFEST_PATH="$cr/Cargo.toml" SAN_TARGET_ROOT="$sroot" SAN_LOG_ROOT="$st/slogs-$name" SAN_MIN_FREE_GB=0 \
         M_RED=0 M_BLOCKED=0 M_TOTAL=0 M_ROWS="" bash -c '. "$0" --source-only; matrix_run "$1" "$2" san-selftest; matrix_summary "$1"; gate_verdict SELF 0' \
         "$GATES_DIR/sanitizers.sh" "$san" "$filter" 2>&1)"; r=$?
    local okv=0
    case "$want" in
      RED) [ $r -eq 1 ] && printf '%s' "$o" | grep -qE "$must" && okv=1 ;;
      GREEN) [ $r -eq 0 ] && printf '%s' "$o" | grep -qE "$must" && okv=1 ;;
    esac
    if [ $okv -eq 1 ]; then echo "  ok   $name: $san '$filter' -> $want ($(printf '%s' "$o" | grep -m1 -oE "$must"))"
    else echo "  FAIL $name: $san '$filter' wanted $want matching /$must/, got rc=$r:"; printf '%s\n' "$o" | sed 's/^/       /' | tail -15; fail=1; fi
  }
  arm "A1 heap overflow" asan "tests::defect_heap_overflow --exact" RED "heap-buffer-overflow"
  arm "A0 clean control" asan "tests::clean_control --exact" GREEN "1 test\\(s\\) ran"
  arm "B  empty filter" asan "no_such_test_anywhere" RED "VACUOUS"
  if [ "$OS" = "Linux" ]; then
    arm "A2 leak" lsan "tests::defect_leak --exact" RED "LeakSanitizer: detected memory leaks"
    arm "A2 control: the leak under G-ASAN (leak checker off)" asan "tests::defect_leak --exact" GREEN "1 test\\(s\\) ran"
    if [ -n "$MSAN_AARCH64_NOTE" ] && [ "$ARCH" = "aarch64" ]; then
      echo "  skip A3 uninit read under MSan: BLOCKED on $ARCH ($MSAN_AARCH64_NOTE)"
    else
      arm "A3 uninit read" msan "tests::defect_uninit_read --exact" RED "MemorySanitizer: use-of-uninitialized-value"
      arm "A3 control: clean under MSan" msan "tests::clean_control --exact" GREEN "1 test\\(s\\) ran"
    fi
  else
    echo "  skip A2 leak under LSan: Linux-only (rustc rejects -Zsanitizer=leak on $OS); runs on rvm / ci.yml"
    echo "  skip A3 uninit read under MSan: Linux-only; runs on rvm / ci.yml"
  fi
  # The synthetic crate's own build dirs (san-<san>-san-selftest) are dropped by
  # matrix_run; remove the work dir itself.
  rm -rf -- "${st:?}"
  if [ $fail -eq 0 ]; then echo "G-SAN-SELF: GREEN"; return 0; fi
  echo "G-SAN-SELF: RED"; return 1
}

# ---------------------------------------------------------------------------
# Dispatch.
# ---------------------------------------------------------------------------
# shellcheck disable=SC2317  # `exit` is reached only when run, not sourced
if [ "${1:-}" = "--source-only" ]; then return 0 2>/dev/null || exit 0; fi
case "${1:-}" in
  --self-test) self_test; exit $? ;;
  --crosscheck)
    # Re-run only the ASan-claim cross-check over an existing run's logs.
    case "${2:-}" in asan|lsan) asan_claims_crosscheck "$2"; exit $? ;; esac
    echo "usage: sanitizers.sh --crosscheck asan|lsan" >&2; exit 2 ;;
  --gate)
    case "${2:-}" in
      asan) gate_asan_like asan; exit $? ;;
      lsan) gate_asan_like lsan; exit $? ;;
      msan) gate_msan; exit $? ;;
      *) echo "usage: sanitizers.sh --gate asan|lsan|msan" >&2; exit 2 ;;
    esac ;;
  --matrix)
    shift
    need_nightly
    SAN=""; FILTER=""; WS=0; LIST=""; EXCL=""
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --san) SAN="$2"; shift 2 ;;
        --crates) LIST="$LIST $(printf '%s' "$2" | tr ',' ' ')"; shift 2 ;;
        --workspace) WS=1; shift ;;
        --filter) FILTER="$2"; shift 2 ;;
        --exclude) EXCL="$EXCL$2|"$'\n'; shift 2 ;;
        *) echo "sanitizers.sh --matrix: unknown argument '$1'" >&2; exit 2 ;;
      esac
    done
    case "$SAN" in asan|lsan|msan) ;; *) echo "sanitizers.sh --matrix: --san asan|lsan|msan" >&2; exit 2 ;; esac
    if [ "$WS" -eq 1 ]; then LIST="$LIST $(scope_minus "$EXCL" | tr '\n' ' ')"; fi
    # shellcheck disable=SC2086  # LIST is a word list of crate specs
    set -- $LIST
    [ "$#" -gt 0 ] || { echo "sanitizers.sh --matrix: no crates (use --crates or --workspace)" >&2; exit 2; }
    matrix_run "$SAN" "$FILTER" "$@"
    matrix_summary "$SAN"
    gate_verdict "MATRIX($SAN)" 0; exit $? ;;
  ""|-h|--help)
    sed -n '2,50p' "$0"; exit 2 ;;
  *) legacy_main "$@"; exit $? ;;
esac
