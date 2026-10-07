#!/usr/bin/env bash
# GATE G-VALGRIND — the workspace's test binaries under Valgrind memcheck.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `valgrind` job
# (`zig build test-valgrind`, `build.zig:411-422`), and it CLOSES A DEFERRAL
# rustty already wrote down. check_leaks.sh said, in its header and again at
# line ~201 (until P7-W16a, which found the premise wrong for LSan):
#
#   "FFI *leak* detection: LSan/valgrind are unavailable on arm64 macOS, so
#    FFI leak checks are deferred to Linux CI — see DoP §G6."
#
# G6 covers UB across the FFI boundary with AddressSanitizer, which on macOS is
# built with `detect_leaks=0` (Apple clang's ASan runtime has no leak checker;
# rustc's does, which is how G-LSAN runs on macOS since P7-W16a). So the
# crates that cross into C had NO leak coverage anywhere. This is that lane,
# and Linux CI is where it runs.
#
# SCOPE. The reference runs its WHOLE unit-test executable (`src/main.zig`,
# every module of the app) under `valgrind --leak-check=full
# --error-exitcode=1 --num-callers=50 --suppressions=valgrind.supp`. The port's
# equivalent is every workspace crate's test binaries, which is the default
# set below (NEW-FINDINGS N413; it was 3 FFI crates until L-5 and 6 after it).
# The one exclusion is listed under DEFAULT_SCOPE with its reason, and the gate
# fails when a workspace member is missing from the list.
#
# WHAT IT RUNS. For each crate in scope, `cargo build -p` and
# `cargo test -p <crate> --no-run`, then every test binary cargo reports, under
# `valgrind --leak-check=full --error-exitcode=1` with the repo's valgrind.supp
# (the reference's file) and valgrind-llvmpipe.supp (the port's GL-driver
# entries). `--error-exitcode=1` is what makes it a gate rather than a report:
# without it valgrind exits 0 no matter what it finds.
#
# `--errors-for-leak-kinds=definite` deliberately (the reference uses
# valgrind's default, definite + possible): every Rust test binary has a 48-byte
# *possibly lost* std thread handle (`std::thread::current::init_current`), and
# `possible` also fires on interior pointers into Vec/Box allocations, so the
# default kinds fail every binary on std itself (N404). A DEFINITE leak — a
# block with no remaining pointer to it — is unambiguous.
#
# FORKED CHILDREN. Valgrind follows fork(2) (and, with the default
# `--trace-children=no`, stops at exec), exactly as under the reference's
# command line: a child that runs Rust code and exits prints its own memcheck
# summary into the log. Every process's `ERROR SUMMARY` is checked, not only
# the test binary's, so an error inside a child is a finding even when the
# test never looks at the child's exit status. (A forked child inherits its
# parent's error count, so a parent error shows up in its children too; the
# parent's own summary already makes that run red, N410.)
#
# SHARDS AND JOBS. The whole scope is hours of memcheck on one core (rustty-vt's
# 3,223 lib tests alone take about 10 minutes natively in a debug build, and
# memcheck is 30-50x slower), so CI runs it as N jobs of J processes each.
# `--shard K/N` runs, from EVERY test binary, the tests whose index in the
# binary's `--list` is K-1 modulo N (round-robin, so a binary's slow neighbours
# spread out), each slice as its own memcheck process; `--jobs J` splits the
# shard again into J parts run side by side (part (K-1)*J+j of N*J). The parts
# partition every binary's list; nothing is left out. A binary that lists no
# test (`harness = false`) runs whole, in the one part its name hashes to.
# Without either option every binary runs whole, one after another.
#
# BLOCKED (exit 3), never green, without valgrind. That is every macOS host
# (valgrind has no working arm64-Darwin port) and any Linux box without it
# installed; `nix shell nixpkgs#valgrind` provides it, and so does
# `apt install valgrind libc6-dbg`. Also BLOCKED where
# RUSTTY_VALGRIND_OWN_JOB is set: CI's `ci` job sets it because the gate runs in
# ci.yml's `valgrind` job instead (sharded, as the reference runs valgrind in a
# job of its own), and a BLOCKED is reported as a WARNING there, not as a pass.
#
# WITHOUT ROOT (the Fedora VM, 2026-09-27, N405): `dnf download valgrind`,
# `rpm2cpio valgrind-*.rpm | cpio -idm` into a user dir, and put a two-line
# `valgrind` wrapper first on PATH that sets
# VALGRIND_LIB=<dir>/usr/libexec/valgrind and execs <dir>/usr/bin/valgrind
# (the launcher otherwise looks for its tools under /usr/libexec). No
# glibc debuginfo was needed there.
#
# SELF-TEST. `--self-test` (G-VALGRIND-SELF in ci.sh) needs no valgrind and
# runs on every leg: it runs copies of this script end to end against a fake
# `valgrind` and a fake `cargo` on PATH, over fixture test binaries and canned
# memcheck logs, and checks each acceptance arm GREEN and RED (see
# `self_test` below). The script is kept to bash 3.2 (no `mapfile`, no bare
# expansion of a possibly empty array under `set -u`) because macOS ships
# nothing newer and the self-test runs there.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
#
# Usage:
#   bash .port/tools/gates/check_valgrind.sh                    # the default scope
#   bash .port/tools/gates/check_valgrind.sh --shard 3/4 --jobs 4   # one CI job
#   bash .port/tools/gates/check_valgrind.sh rustty-oniguruma   # named crates
#   bash .port/tools/gates/check_valgrind.sh rustty-apprt:gtk,wayland,x11
#   bash .port/tools/gates/check_valgrind.sh --list-scope
#   bash .port/tools/gates/check_valgrind.sh --self-test
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

SUPP="$ROOT/valgrind.supp"
PORT_SUPP="$ROOT/.port/tools/gates/valgrind-llvmpipe.supp"

blocked() { echo "G-VALGRIND BLOCKED: $*"; exit 3; }
red()     { echo "G-VALGRIND RED: $*" >&2; exit 1; }

# ---- --self-test (G-VALGRIND-SELF) -------------------------------------------
# Runs COPIES of this script end to end, each in its own fixture root, with a
# fake `valgrind` and a fake `cargo` first on PATH (no valgrind needed, so it
# runs on macOS too). The fake cargo reports fixture test binaries (shell
# scripts that answer `--list --format terse`) and fails the build of any crate
# marked broken; the fake valgrind records which binary and which tests every
# memcheck process was given, then prints a canned memcheck log. Each arm has
# its GREEN case and a RED control; a control that runs a MUTATED copy of the
# script proves the assertion notices exactly that regression.
#   A  shard round-robin: shards 1..3 x 2 jobs run every listed test exactly
#      once, the named skip never, the no-list binary whole exactly once; RED
#      control: an off-by-one modulus in the copy leaves tests out, and a copy
#      that ignores the skip runs it.
#   B  skips: the summary line counts and names the skip; a skip naming a test
#      the binary no longer lists, or a binary the crate no longer builds, is RED.
#   C  memcheck summaries: a forked child's `ERROR SUMMARY: 1` under a parent's
#      `0` (valgrind exits 0) is RED; `definitely lost: 0 bytes` with
#      `ERROR SUMMARY: 0` is GREEN; control: a copy that reads only the last
#      summary (the test binary's own) goes GREEN on the child error.
#   D  a crate that fails to build is RED, also next to a clean crate.
#   E  RUSTTY_VALGRIND_OWN_JOB is BLOCKED (exit 3); a bad --shard is RED.
#   F  crate-list completeness on the REAL rows: the real DEFAULT_SCOPE against
#      the real `cargo metadata` has nothing missing; dropping rustty-zf, the GTK
#      apprt lane or the flatpak lane from it is RED, each by name.
#   G  every memcheck process gets --error-exitcode=1, --leak-check=full,
#      --errors-for-leak-kinds=definite, --num-callers=50 and both
#      suppression files.
self_test() {
  local st fx pass=0 fail=0 k
  command -v python3 >/dev/null 2>&1 || blocked "--self-test needs python3"
  command -v cargo >/dev/null 2>&1 || blocked "--self-test needs cargo (arm F reads the real cargo metadata)"
  st="$(mktemp -d)"
  # Expanded now: `st` is local to this function, and the trap runs at exit.
  # shellcheck disable=SC2064
  trap "rm -rf '$st'" EXIT
  fx="$st/fix"
  mkdir -p "$fx/bin" "$fx/lists" "$fx/logs" "$fx/buildfail"
  (cd "$ROOT" && cargo metadata --no-deps --format-version 1) > "$fx/meta.json" 2>"$st/meta.log" \
    || { tail -3 "$st/meta.log"; red "--self-test: cargo metadata failed"; }

  cat > "$fx/bin/valgrind" <<'FAKE'
#!/usr/bin/env bash
fx="$(cd "$(dirname "$0")/.." && pwd)"
opts=""
while [ "$#" -gt 0 ]; do case "$1" in --*) opts="$opts $1"; shift ;; *) break ;; esac; done
printf '%s\n' "$opts" >> "$fx/vgopts"
bin="$(basename "$1")"; bin="${bin%-*}"; shift
n=0
for a in "$@"; do
  case "$a" in --*) ;; *) printf '%s\t%s\n' "$bin" "$a" >> "$fx/record"; n=$((n+1)) ;; esac
done
[ "$n" -gt 0 ] || printf '%s\tWHOLE\n' "$bin" >> "$fx/record"
if [ -f "$fx/logs/$bin.log" ]; then
  cat "$fx/logs/$bin.log"
else
  printf '==4242== Memcheck, a memory error detector\ntest result: ok. %s passed\n==4242== ERROR SUMMARY: 0 errors from 0 contexts (suppressed: 0 from 0)\n' "$n"
fi
exit 0
FAKE
  cat > "$fx/bin/cargo" <<'FAKE'
#!/usr/bin/env bash
fx="$(cd "$(dirname "$0")/.." && pwd)"
sub="$1"; shift
crate=""
while [ "$#" -gt 0 ]; do [ "$1" = -p ] && crate="${2:-}"; shift; done
case "$sub" in
  metadata) cat "$fx/meta.json" ;;
  build|test)
    if [ -e "$fx/buildfail/$crate" ]; then echo "error: fixture build failure of $crate" >&2; exit 101; fi
    if [ "$sub" = test ] && [ -d "$fx/tbin/$crate" ]; then
      for b in "$fx/tbin/$crate"/*; do
        printf '{"reason":"compiler-artifact","profile":{"test":true},"executable":"%s","manifest_path":"%s/crates/%s/Cargo.toml"}\n' "$b" "$fx" "$crate"
      done
    fi ;;
esac
exit 0
FAKE
  chmod +x "$fx/bin/valgrind" "$fx/bin/cargo"

  # Fixture test binaries: `tbin/<crate>/<binary>-<hash>`, listing
  # `lists/<binary>` (no list file: a `harness = false` binary).
  st_bin() {
    mkdir -p "$fx/tbin/$1" "$fx/crates/$1"
    cat > "$fx/tbin/$1/$2-0123456789abcdef" <<'FAKE'
#!/usr/bin/env bash
fx="$(cd "$(dirname "$0")/../.." && pwd)"
bin="$(basename "$0")"; bin="${bin%-*}"
if [ "${1:-}" = --list ] && [ -f "$fx/lists/$bin" ]; then sed 's/$/: test/' "$fx/lists/$bin"; fi
exit 0
FAKE
    chmod +x "$fx/tbin/$1/$2-0123456789abcdef"
  }
  st_bin fixa fixa_lib; for k in $(seq 0 22); do echo "mod_a::t$k"; done > "$fx/lists/fixa_lib"
  st_bin fixa fixa_it;  for k in $(seq 0 4); do echo "it_$k"; done > "$fx/lists/fixa_it"
  st_bin fixa fixa_hf
  st_bin fixb fixb_lib; for k in $(seq 0 8); do echo "mod_b::t$k"; done > "$fx/lists/fixb_lib"
  echo "mod_x::b_skipme" >> "$fx/lists/fixb_lib"
  st_bin fixerr childerr; echo "forks_a_child" > "$fx/lists/childerr"
  cat > "$fx/logs/childerr.log" <<'LOG'
==100== Memcheck, a memory error detector
==101== 4,096 bytes in 1 blocks are definitely lost in loss record 1 of 1
==101==    at 0x4885F54: malloc (vg_replace_malloc.c:447)
==101==    by 0x10A2B3: fixture::child (fixture.rs:1)
==101== ERROR SUMMARY: 1 errors from 1 contexts (suppressed: 0 from 0)
test forks_a_child ... ok
test result: ok. 1 passed; 0 failed
==100== HEAP SUMMARY:
==100==     in use at exit: 0 bytes in 0 blocks
==100== ERROR SUMMARY: 0 errors from 0 contexts (suppressed: 0 from 0)
LOG
  st_bin fixclean leakzero; echo "holds_memory_at_exit" > "$fx/lists/leakzero"
  cat > "$fx/logs/leakzero.log" <<'LOG'
==200== Memcheck, a memory error detector
test result: ok. 1 passed; 0 failed
==200== LEAK SUMMARY:
==200==    definitely lost: 0 bytes in 0 blocks
==200==    indirectly lost: 0 bytes in 0 blocks
==200==      possibly lost: 48 bytes in 1 blocks
==200== ERROR SUMMARY: 0 errors from 0 contexts (suppressed: 0 from 0)
LOG
  : > "$fx/buildfail/fixbroken"

  # A copy of this script under $st/<name>. ST_<ARRAY> (newline-separated)
  # replaces DEFAULT_SCOPE / VALGRIND_SKIPS / REQUIRED_FEATURE_LANES, and
  # ST_<ARRAY>_DROP removes one entry from the real list; ST_MUT_OLD ->
  # ST_MUT_NEW mutates exactly one occurrence.
  st_copy() {
    local d="$st/$1"
    mkdir -p "$d/.port/tools/gates"
    : > "$d/valgrind.supp"; : > "$d/.port/tools/gates/valgrind-llvmpipe.supp"
    python3 - "$ROOT/.port/tools/gates/check_valgrind.sh" "$d/.port/tools/gates/check_valgrind.sh" <<'PY'
import os, shlex, sys
s = open(sys.argv[1]).read()
for name in ("DEFAULT_SCOPE", "VALGRIND_SKIPS", "REQUIRED_FEATURE_LANES"):
    head = "\n" + name + "=(\n"
    i = s.index(head) + 1
    j = s.index("\n)\n", i) + 3
    items = shlex.split(s[i + len(head) - 1:j - 2], comments=True)
    if ("ST_" + name) in os.environ:
        items = [x for x in os.environ["ST_" + name].split("\n") if x]
    drop = os.environ.get("ST_" + name + "_DROP")
    if drop:
        assert drop in items, drop
        items.remove(drop)
    s = s[:i] + name + "=(\n" + "".join("  %s\n" % shlex.quote(x) for x in items) + ")\n" + s[j:]
old = os.environ.get("ST_MUT_OLD")
if old:
    # The gate's own code only: this self-test quotes the mutated text too.
    a = s.index("\n# ---- --self-test (G-VALGRIND-SELF)")
    b = s.index("\n}\n", s.index("\nself_test() {", a)) + 3
    rest = s[:a] + s[b:]
    assert rest.count(old) == 1, old
    s = rest.replace(old, os.environ["ST_MUT_NEW"])
open(sys.argv[2], "w").write(s)
PY
  }
  # Run copy $1 with the rest as arguments: ST_OUT, ST_RC.
  st_run() {
    : > "$fx/record"
    ST_OUT="$(cd "$st" && env -u RUSTTY_VALGRIND_OWN_JOB PATH="$fx/bin:$PATH" bash "$st/$1/.port/tools/gates/check_valgrind.sh" "${@:2}" 2>&1)"
    ST_RC=$?
  }
  st_ok()  { echo "  ok    $1"; pass=$((pass+1)); }
  st_bad() { echo "  FAIL  $1"; printf '%s\n' "$ST_OUT" | tail -12 | sed 's/^/        /'; fail=$((fail+1)); }
  # st_expect <arm> <rc> <text> [<text absent>]
  st_expect() {
    if [ "$ST_RC" = "$2" ] && printf '%s\n' "$ST_OUT" | grep -qF -- "$3" \
       && { [ -z "${4:-}" ] || ! printf '%s\n' "$ST_OUT" | grep -qF -- "$4"; }; then
      st_ok "$1"
    else
      st_bad "$1 (wanted exit $2 with \"$3\"${4:+ and without \"$4\"}; got exit $ST_RC)"
    fi
  }
  # The record of shards 1..3 x 2 jobs, checked against the fixture lists.
  st_partition() {
    local rec="$st/partition.rec" k
    : > "$rec"
    for k in 1 2 3; do
      st_run "$1" --shard "$k/3" --jobs 2 fixa fixb
      [ "$ST_RC" = 0 ] || { echo "        shard $k/3 exit $ST_RC"; return 1; }
      cat "$fx/record" >> "$rec"
    done
    python3 - "$fx" "$rec" <<'PY'
import collections, sys
fx, rec = sys.argv[1], sys.argv[2]
got = collections.Counter(l.rstrip("\n") for l in open(rec))
want = collections.Counter()
for b in ("fixa_lib", "fixa_it", "fixb_lib"):
    for t in open("%s/lists/%s" % (fx, b)).read().split():
        if t != "mod_x::b_skipme":
            want[b + "\t" + t] += 1
want["fixa_hf\tWHOLE"] += 1
bad = sorted((k, got[k], want[k]) for k in set(got) | set(want) if got[k] != want[k])
for k, g, w in bad[:6]:
    print("        %s ran %d time(s), want %d" % (k.replace("\t", "::"), g, w))
sys.exit(1 if bad else 0)
PY
  }

  echo "=== G-VALGRIND-SELF :: check_valgrind.sh against a fake valgrind and cargo ==="
  export ST_VALGRIND_SKIPS="fixb/fixb_lib|mod_x::b_skipme|fixture skip"
  # Never equal to a named-crate list below, so the member check (arm F) stays
  # out of these runs.
  export ST_DEFAULT_SCOPE="fixture-default-scope"
  export ST_REQUIRED_FEATURE_LANES="fixa|fixture lane"
  st_copy base
  # A
  if st_partition base; then st_ok "A shards 1-3/3 x 2 jobs: every test once, the skip never, the no-list binary once"
  else ST_OUT=""; st_bad "A shard partition"; fi
  st_run base fixa fixb
  if [ "$ST_RC" = 0 ] && [ "$(grep -c WHOLE "$fx/record")" = 3 ] && [ "$(grep -c '^fixb_lib' "$fx/record")" = 9 ]; then
    st_ok "A unsharded: three binaries whole, the one with a skip by name without it"
  else st_bad "A unsharded run"; fi
  # shellcheck disable=SC2016 # the gate's code, literally
  ST_MUT_OLD='[ $(( j % n )) -eq $(( k - 1 )) ]' ST_MUT_NEW='[ $(( j % (n + 1) )) -eq $(( k - 1 )) ]' st_copy mut-modulus
  if st_partition mut-modulus >/dev/null; then ST_OUT=""; st_bad "A control: an off-by-one modulus went unnoticed"
  else st_ok "A control: an off-by-one modulus leaves tests out, noticed"; fi
  # shellcheck disable=SC2016 # the gate's code, literally
  ST_MUT_OLD='[ "${all[$j]}" = "$s" ] && continue 2' ST_MUT_NEW=':' st_copy mut-noskip
  if st_partition mut-noskip >/dev/null; then ST_OUT=""; st_bad "A control: a copy that runs the skipped test went unnoticed"
  else st_ok "A control: a copy that runs the skipped test, noticed"; fi
  # B
  st_run base --shard 2/3 --jobs 2 fixa fixb
  st_expect "B the summary counts and names the skip" 0 "1 named test skip(s): fixb/fixb_lib::mod_x::b_skipme"
  ST_VALGRIND_SKIPS="fixb/fixb_lib|mod_x::renamed_away|fixture skip" st_copy stale-test
  st_run stale-test fixa fixb
  st_expect "B a skip naming a test the binary does not list is RED" 1 "stale VALGRIND_SKIPS entry: fixb/fixb_lib lists no test mod_x::renamed_away"
  ST_VALGRIND_SKIPS="fixb/gone_bin|mod_x::b_skipme|fixture skip" st_copy stale-bin
  st_run stale-bin fixa fixb
  st_expect "B a skip naming a binary the crate does not build is RED" 1 "stale VALGRIND_SKIPS entry: fixb builds no test binary gone_bin"
  st_run stale-bin fixa
  st_expect "B control: the same entry, its crate not in this run, is not RED" 0 "G-VALGRIND GREEN" "stale"
  # C
  st_run base fixerr
  st_expect "C a forked child's ERROR SUMMARY 1 under a parent's 0 is RED" 1 "valgrind finding: 1 error(s)"
  if printf '%s\n' "$ST_OUT" | grep -qF "definitely lost in loss record"; then st_ok "C the child's leak record is printed"
  else st_bad "C the child's leak record is printed"; fi
  st_run base fixclean
  st_expect "C 'definitely lost: 0 bytes' with ERROR SUMMARY 0 is GREEN" 0 "G-VALGRIND GREEN"
  ST_MUT_OLD='| tr -d , | sort -n | tail -1)' ST_MUT_NEW='| tr -d , | tail -1)' st_copy mut-lastsummary
  st_run mut-lastsummary fixerr
  st_expect "C control: a copy reading only the test binary's own (last) summary misses the child" 0 "G-VALGRIND GREEN"
  # D
  st_run base fixbroken
  st_expect "D a build failure is RED" 1 "RED: fixbroken(build)"
  st_run base fixclean fixbroken
  st_expect "D a build failure next to a clean crate is RED" 1 "fixbroken(build)"
  # E
  ST_OUT="$(cd "$st" && RUSTTY_VALGRIND_OWN_JOB="the fixture job" PATH="$fx/bin:$PATH" bash "$st/base/.port/tools/gates/check_valgrind.sh" fixclean 2>&1)"; ST_RC=$?
  st_expect "E RUSTTY_VALGRIND_OWN_JOB is BLOCKED" 3 "BLOCKED: this job defers to the fixture job"
  st_run base --shard 4/3 fixclean
  st_expect "E --shard 4/3 is RED" 1 "--shard wants 1 <= K <= N"
  # F, on the real DEFAULT_SCOPE and REQUIRED_FEATURE_LANES (no fixture
  # binaries for them: the GREEN case ends BLOCKED, nothing ran).
  unset ST_DEFAULT_SCOPE ST_REQUIRED_FEATURE_LANES
  export ST_VALGRIND_SKIPS=""
  st_copy real
  st_run real
  st_expect "F the real DEFAULT_SCOPE covers every real workspace member but xtask" 3 "no test binary was run" "missing"
  ST_DEFAULT_SCOPE_DROP="rustty-zf" st_copy real-nozf
  st_run real-nozf
  st_expect "F control: rustty-zf dropped from the real list is RED" 1 "workspace members missing from DEFAULT_SCOPE (add them, or record why not): rustty-zf"
  ST_DEFAULT_SCOPE_DROP="rustty-apprt:gtk,wayland,x11" st_copy real-nogtk
  st_run real-nogtk
  st_expect "F control: the GTK apprt lane dropped is RED" 1 "feature lane missing from DEFAULT_SCOPE: rustty-apprt:gtk,wayland,x11"
  ST_DEFAULT_SCOPE_DROP="rustty-os:flatpak" st_copy real-noflatpak
  st_run real-noflatpak
  st_expect "F control: the flatpak lane dropped is RED" 1 "feature lane missing from DEFAULT_SCOPE: rustty-os:flatpak"
  # G
  : > "$fx/vgopts"
  st_run base fixclean
  local want_opt missing_opt=""
  for want_opt in --leak-check=full --errors-for-leak-kinds=definite --error-exitcode=1 --num-callers=50 \
      "--suppressions=$st/base/valgrind.supp" "--suppressions=$st/base/.port/tools/gates/valgrind-llvmpipe.supp"; do # WHY: not a toggle: arm G asserts the gate passes both suppression files, each justified at vg_opts.
    grep -qF -- " $want_opt" "$fx/vgopts" || missing_opt="$missing_opt $want_opt"
  done
  if [ "$ST_RC" = 0 ] && [ -s "$fx/vgopts" ] && [ -z "$missing_opt" ]; then st_ok "G memcheck gets the gate's options and both suppression files"
  else st_bad "G memcheck options (missing:$missing_opt)"; fi

  echo "--- G-VALGRIND-SELF: $pass ok, $fail failed ---"
  [ "$fail" -eq 0 ] || { echo "G-VALGRIND-SELF RED"; return 1; }
  echo "G-VALGRIND-SELF GREEN"
  return 0
}

# The default scope: every workspace member, as `crate` or `crate:features`,
# in the workspace's port order, plus two feature lanes `cargo test
# --workspace` does not build: the GTK apprt (the reference's default runtime
# on Linux, so its test executable there includes it) and rustty-os's Flatpak
# support (ci.sh also tests it on its own). Measured by L-6 on the Fedora VM
# (NEW-FINDINGS N413). One member is not in scope: xtask, the build driver (the
# port of `build.zig` and its steps), which is not part of the reference's test
# executable either; its tests drive cmake, cc and xgettext.
DEFAULT_SCOPE=(
  rustty-util rustty-build-config rustty-identity rustty-version
  rustty-datastruct rustty-unicode rustty-simd rustty-lib rustty-global
  rustty-render-types rustty-os rustty-os:flatpak rustty-xev rustty-zigstd
  rustty-vt rustty-vt-c rustty-crash rustty-cfg-fmt rustty-args
  rustty-apprt-api rustty-font rustty-font-shaper rustty-oniguruma
  rustty-wuffs rustty-z2d rustty-zigimg rustty-zf rustty-vaxis rustty-input
  rustty-inspector rustty-renderer rustty-config rustty-terminfo
  rustty-termio rustty-core rustty-apprt "rustty-apprt:gtk,wayland,x11"
  rustty-apprt-c rustty-cli rustty-extra rustty rustty-bench
  rustty-synthetic rustty-helpgen rustty-wasm rustty-web-canvas
  breakpad-sys dcimgui-sys fontconfig-sys freetype-sys gettext-sys
  glslang-sys harfbuzz-sys highway-sys libpng-sys libxml2-sys oniguruma-sys
  sentry-sys simdutf-sys spirv-cross-sys wuffs-sys zlib-sys
)

# The feature lanes DEFAULT_SCOPE must keep. The member check below compares
# crate names only, so without this list either lane could be dropped and
# nothing would say so (review L-6 F6). Format: `<entry>|<reason>`.
REQUIRED_FEATURE_LANES=(
  "rustty-apprt:gtk,wayland,x11|GTK is the reference's default runtime on Linux, so its test executable there compiles the GTK apprt; cargo test --workspace does not"
  "rustty-os:flatpak|the Flatpak arm of rustty-os, which ci.sh also tests on its own; cargo test --workspace does not build it"
)

# Tests memcheck cannot run, by exact name, each with its reason. Printed by
# name on every run and counted in the summary line, never dropped silently;
# each still runs in every `cargo test` lane. An entry whose test is no longer
# in its binary's `--list`, or whose binary the crate no longer builds, is RED
# (review L-6 F5): a rename would otherwise leave an entry that skips nothing.
# Format: `<crate>/<binary>|<test path>|<reason>`.
VALGRIND_SKIPS=(
  "rustty-cli/rustty_cli|edit_config::exec_tests::exec_failure_report_prints_the_raw_command_bytes|the test makes execve(2) fail with E2BIG in its own process (a 4 MiB command); valgrind cannot survive a failed execve of the program it runs (\"EXEC FAILED: I can't recover from execve() failing, so I'm dying\"), which kills the whole test binary (N416)"
)

SHARD_K=1
SHARD_N=1
JOBS=1
LIST_ONLY=0
SELF_TEST=0
CRATES=()
while [ "$#" -gt 0 ]; do
  case "$1" in
    --shard)
      [ "$#" -ge 2 ] || red "--shard needs K/N"
      SHARD_K="${2%/*}"; SHARD_N="${2#*/}"; shift 2 ;;
    --shard=*)
      v="${1#--shard=}"; SHARD_K="${v%/*}"; SHARD_N="${v#*/}"; shift ;;
    --jobs)
      [ "$#" -ge 2 ] || red "--jobs needs a count"
      JOBS="$2"; shift 2 ;;
    --jobs=*) JOBS="${1#--jobs=}"; shift ;;
    --list-scope) LIST_ONLY=1; shift ;;
    --self-test) SELF_TEST=1; shift ;;
    -*) red "unknown option $1" ;;
    *) CRATES+=("$1"); shift ;;
  esac
done
case "$SHARD_K$SHARD_N$JOBS" in *[!0-9]*|"") red "--shard wants K/N and --jobs a count, all integers" ;; esac
[ "$SHARD_N" -ge 1 ] && [ "$SHARD_K" -ge 1 ] && [ "$SHARD_K" -le "$SHARD_N" ] \
  || red "--shard wants 1 <= K <= N, got $SHARD_K/$SHARD_N"
[ "$JOBS" -ge 1 ] || red "--jobs wants at least 1"
[ "${#CRATES[@]}" -gt 0 ] || CRATES=("${DEFAULT_SCOPE[@]}")

if [ "$SELF_TEST" = 1 ]; then
  self_test
  exit $?
fi

if [ "$LIST_ONLY" = 1 ]; then
  printf '%s\n' "${CRATES[@]}"
  exit 0
fi

if [ -n "${RUSTTY_VALGRIND_OWN_JOB:-}" ]; then
  blocked "this job defers to $RUSTTY_VALGRIND_OWN_JOB (RUSTTY_VALGRIND_OWN_JOB is set), which runs the full scope sharded."
fi
command -v valgrind >/dev/null 2>&1 \
  || blocked "valgrind not installed (nix shell nixpkgs#valgrind, or apt install valgrind libc6-dbg)"
command -v cargo >/dev/null 2>&1 || blocked "cargo not installed"
[ -f "$SUPP" ] || red "valgrind.supp is missing"
[ -f "$PORT_SUPP" ] || red "$PORT_SUPP is missing"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# The default scope must name every workspace member but xtask: a crate added
# to the workspace without being added here would otherwise never run under
# memcheck, and nothing would say so.
if [ "${CRATES[*]}" = "${DEFAULT_SCOPE[*]}" ]; then
  cargo metadata --no-deps --format-version 1 > "$WORK/meta.json" 2>"$WORK/meta.log" \
    || red "cargo metadata failed: $(tail -3 "$WORK/meta.log")"
  missing="$(python3 - "$WORK/meta.json" "${DEFAULT_SCOPE[@]}" <<'PY'
import json, sys
meta = json.load(open(sys.argv[1]))
scope = {e.split(":", 1)[0] for e in sys.argv[2:]}
members = set(meta["workspace_members"])
names = sorted(p["name"] for p in meta["packages"] if p["id"] in members)
print(" ".join(n for n in names if n not in scope and n != "xtask"))
PY
)"
  [ -z "$missing" ] || red "workspace members missing from DEFAULT_SCOPE (add them, or record why not): $missing"
  for req in "${REQUIRED_FEATURE_LANES[@]}"; do
    lane="${req%%|*}"
    found=0
    for e in "${DEFAULT_SCOPE[@]}"; do
      [ "$e" = "$lane" ] && found=1
    done
    [ "$found" = 1 ] || red "feature lane missing from DEFAULT_SCOPE: $lane (${req#*|})"
  done
fi

RAN=0
FAILED=0
FAILED_NAMES=""
SKIPPED=""
SKIP_APPLIED=""
BUILT_CRATES=" "
LOGDIR="$WORK"
T0=$(date +%s)

# An ARRAY rather than a `\`-continued command line so each option can carry
# its reason on its OWN line — D6's suppression audit (rule c) requires the WHY
# to sit on the same physical line as the toggle, and a backslash continuation
# makes a trailing `#` comment impossible.
vg_opts=(
  --leak-check=full
  --errors-for-leak-kinds=definite
  --error-exitcode=1
  --num-callers=50
  --suppressions="$SUPP" # WHY: valgrind.supp is the reference's file, applied as the reference applies it: its 233 entries name GTK/GLib/GDK/fontconfig frames, Mesa DRI driver objects, or DRD-only kinds (audited 2026-09-27, N403), and the GTK apprt lane runs the same GTK/GLib code the reference's test executable runs them over, so there they can match, exactly as in the reference (review L-6 F4); none names a Rust or vendored-C frame of the port.
  --suppressions="$PORT_SUPP" # WHY: Mesa (llvmpipe software GL and EGL) noise only, justified in the file: one entry for LLVM codegen frames under gallivm_jit_function (N412), two for the never-terminated EGL display's driver data Mesa drops at exit, every frame Mesa's up to eglInitialize, definite leaks only (N520); the tests' read-back pixels are declared defined in the tests instead (make_mem_defined, review L-6 F2, N521).
)

# Run one memcheck process and classify it. $1 = display name, $2 = package
# dir, $3 = binary, rest = libtest arguments.
run_one() {
  local name="$1" pkg_dir="$2" b="$3"
  shift 3
  local t_start log status secs vg_errors
  t_start=$(date +%s)
  log="$LOGDIR/run.log"
  if (cd "$pkg_dir" && CARGO_MANIFEST_DIR="$pkg_dir" valgrind "${vg_opts[@]}" "$b" --test-threads=1 "$@") > "$log" 2>&1; then
    status=0
  else
    status=1
  fi
  secs=$(( $(date +%s) - t_start ))
  # Every process's summary, the test binary's and any forked child's: the
  # deciding line is memcheck's own `ERROR SUMMARY: N errors` (what
  # --error-exitcode acts on), never a text match on "definitely lost", which
  # LEAK SUMMARY prints as `definitely lost: 0 bytes` on any run that ends with
  # unfreed memory (N403).
  vg_errors="$(sed -n 's/^==[0-9]*== ERROR SUMMARY: \([0-9,]*\) errors.*/\1/p' "$log" | tr -d , | sort -n | tail -1)"
  RAN=$((RAN+1))
  if [ "$status" -eq 0 ] && [ "${vg_errors:-0}" -eq 0 ]; then
    echo "  ok      $name — $(grep -m1 '^test result:' "$log" || echo 'ran') (${secs}s)"
    return
  fi
  if [ "${vg_errors:-0}" -gt 0 ]; then
    # Print only the ERROR records: every record with a stack before
    # HEAP SUMMARY (memcheck prints stacks there only for errors), and after
    # it only the `definitely lost` leak records. A plain at/by grep showed
    # whichever stack came first — measured on the negative control, a
    # `possibly lost` thread-local record (N403). Per process: a forked
    # child's HEAP SUMMARY does not end the parent's error section.
    awk '
      /^==[0-9]+== / { split($1, a, "=="); pid = a[2] }
      /HEAP SUMMARY:/ { heap[pid] = 1 }
      /^==[0-9]+== [^ ]/ {
        hdr = $0; pending = 0
        if (heap[pid]) { keep = ($0 ~ /are definitely lost in loss record/); if (keep) print }
        else { keep = 1; pending = 1 }
        next
      }
      /^==[0-9]+==    (at|by) / {
        if (pending) { print hdr; pending = 0 }
        if (keep) print
        next
      }
      { keep = 0; pending = 0 }
    ' "$log" | head -40 | sed 's/^/      /'
    echo "  FAIL    $name (valgrind finding: $vg_errors error(s)) (${secs}s)"
  else
    # The run log ENDS with memcheck's own summary, so its tail hides the
    # failing test; show libtest's failure lines instead (N403).
    if grep -qE "panicked at|^test .* FAILED$" "$log"; then
      grep -E -A2 "panicked at|^test .* FAILED$" "$log" | head -20 | sed 's/^/      /'
    else
      tail -14 "$log" | sed 's/^/      /'
    fi
    echo "  FAIL    $name (test failure, not a valgrind finding) (${secs}s)"
  fi
  FAILED=1; FAILED_NAMES="$FAILED_NAMES $name"
}

# ---- build every crate in scope and collect its test binaries --------------
BIN_NAME=()
BIN_PKG=()
BIN_EXE=()
BIN_SKIPS=()
echo "=== G-VALGRIND :: memcheck over ${#CRATES[@]} crate(s), shard $SHARD_K/$SHARD_N, $JOBS job(s) ==="
for entry in "${CRATES[@]}"; do
  c="${entry%%:*}"
  feat=()
  if [ "$entry" != "$c" ]; then
    feat=(--features "${entry#*:}")
  fi
  # The crate's own artifacts first: an integration test may load one that
  # `cargo test` does not build (rustty-apprt-c's C-ABI smoke links the
  # cdylib; ci.sh has it from its `cargo build --workspace`).
  if ! cargo build -p "$c" ${feat[@]+"${feat[@]}"} > "$WORK/build.log" 2>&1; then
    tail -15 "$WORK/build.log" | sed 's/^/      /'
    echo "  FAIL    $entry (build)"
    FAILED=1; FAILED_NAMES="$FAILED_NAMES $entry(build)"
    continue
  fi
  # `--no-run` and parse the JSON so the exact test binary is known, rather
  # than globbing target/debug/deps and hoping. Same reasoning as the
  # xcframework archive discovery in xtask.
  if ! cargo test -p "$c" ${feat[@]+"${feat[@]}"} --no-run --message-format=json > "$WORK/build.json" 2>"$WORK/build.log"; then
    tail -15 "$WORK/build.log" | sed 's/^/      /'
    echo "  FAIL    $entry (build)"
    FAILED=1; FAILED_NAMES="$FAILED_NAMES $entry(build)"
    continue
  fi
  # Test executables appear as `"test":true` compiler-artifact entries.
  # Each line is `<executable>\t<package dir>`: `cargo test` runs a test
  # binary with its package directory as the working directory and
  # CARGO_MANIFEST_DIR set, and tests rely on both (rustty-vt's
  # `image_load_rgb_not_compressed_relative_regular_file` asserts the CWD), so
  # each binary runs the same way here (N407). Only this package's binaries:
  # a path dependency's test artifacts are not reported with `-p`.
  bins=()
  while IFS= read -r l; do bins+=("$l"); done < <(python3 - "$WORK/build.json" <<'PY'
import json, os, sys
for line in open(sys.argv[1]):
    line = line.strip()
    if not line.startswith("{"):
        continue
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("profile", {}).get("test"):
        exe = m.get("executable")
        if exe:
            print(exe + "\t" + os.path.dirname(m["manifest_path"]))
PY
)
  BUILT_CRATES="$BUILT_CRATES$c "
  if [ "${#bins[@]}" -eq 0 ]; then
    echo "  skipped $entry (no test binary produced)"
    SKIPPED="$SKIPPED $entry"
    continue
  fi
  for line in "${bins[@]}"; do
    b="${line%%$'\t'*}"
    bin_name="$(basename "$b" | sed 's/-[0-9a-f]*$//')"
    # This binary's VALGRIND_SKIPS, printed by name. Each must name a test
    # the binary lists (listing runs no test).
    skip_list=""
    listing=""
    for s in ${VALGRIND_SKIPS[@]+"${VALGRIND_SKIPS[@]}"}; do
      IFS='|' read -r s_bin s_test s_why <<< "$s"
      if [ "$s_bin" = "$c/$bin_name" ]; then
        [ -n "$listing" ] || listing="$(cd "${line#*$'\t'}" && CARGO_MANIFEST_DIR="${line#*$'\t'}" "$b" --list --format terse 2>/dev/null | sed -n 's/: test$//p')"
        if printf '%s\n' "$listing" | grep -qxF -- "$s_test"; then
          skip_list="$skip_list|$s_test"
          SKIP_APPLIED="$SKIP_APPLIED $c/$bin_name::$s_test"
          echo "  skip    $c/$bin_name::$s_test — $s_why"
        else
          echo "  FAIL    stale VALGRIND_SKIPS entry: $c/$bin_name lists no test $s_test (renamed or removed? update or delete the entry)"
          FAILED=1; FAILED_NAMES="$FAILED_NAMES $c/$bin_name::$s_test(stale-skip)"
        fi
      fi
    done
    BIN_NAME+=("$entry/$bin_name")
    BIN_PKG+=("${line#*$'\t'}")
    BIN_EXE+=("$b")
    BIN_SKIPS+=("${skip_list#|}")
  done
done

# A VALGRIND_SKIPS entry whose crate built in this run but produced no binary
# of that name skips nothing: RED, as a stale test name is.
for s in ${VALGRIND_SKIPS[@]+"${VALGRIND_SKIPS[@]}"}; do
  s_bin="${s%%|*}"
  case "$BUILT_CRATES" in *" ${s_bin%%/*} "*) ;; *) continue ;; esac
  matched=0
  for n in ${BIN_NAME[@]+"${BIN_NAME[@]}"}; do
    n_entry="${n%/*}"
    [ "${n_entry%%:*}" = "${s_bin%%/*}" ] && [ "${n##*/}" = "${s_bin#*/}" ] && matched=1
  done
  if [ "$matched" = 0 ]; then
    echo "  FAIL    stale VALGRIND_SKIPS entry: ${s_bin%%/*} builds no test binary ${s_bin#*/}"
    FAILED=1; FAILED_NAMES="$FAILED_NAMES $s_bin(stale-skip)"
  fi
done

# ---- run part $1 of $2 over every collected binary ---------------------------
# The tests whose index in the binary's `--list` is $1-1 modulo $2 (listing
# runs no test), minus the binary's VALGRIND_SKIPS.
run_part() {
  local k="$1" n="$2" i j s owner name pkg_dir b
  local -a all slice skips
  for i in "${!BIN_EXE[@]}"; do
    name="${BIN_NAME[$i]}"; pkg_dir="${BIN_PKG[$i]}"; b="${BIN_EXE[$i]}"
    skips=()
    [ -z "${BIN_SKIPS[$i]}" ] || IFS='|' read -r -a skips <<< "${BIN_SKIPS[$i]}"
    all=()
    while IFS= read -r t; do all+=("$t"); done < <(cd "$pkg_dir" && CARGO_MANIFEST_DIR="$pkg_dir" "$b" --list --format terse 2>/dev/null | sed -n 's/: test$//p')
    if [ "${#all[@]}" -eq 0 ]; then
      # A binary that lists no test (a `harness = false` target, or one with
      # no tests at all) runs whole, in exactly one part, chosen by its name.
      owner=$(( $(printf '%s' "$name" | cksum | cut -d' ' -f1) % n + 1 ))
      if [ "$owner" -eq "$k" ]; then
        run_one "$name [lists no tests: whole binary]" "$pkg_dir" "$b"
      fi
      continue
    fi
    slice=()
    for j in "${!all[@]}"; do
      [ $(( j % n )) -eq $(( k - 1 )) ] || continue
      for s in ${skips[@]+"${skips[@]}"}; do
        [ "${all[$j]}" = "$s" ] && continue 2
      done
      slice+=("${all[$j]}")
    done
    [ "${#slice[@]}" -gt 0 ] || continue
    if [ "$n" -eq 1 ] && [ "${#skips[@]}" -eq 0 ]; then
      run_one "$name" "$pkg_dir" "$b"
    else
      run_one "$name [${#slice[@]}/${#all[@]}]" "$pkg_dir" "$b" --exact "${slice[@]}"
    fi
  done
}

if [ "${#BIN_EXE[@]}" -gt 0 ]; then
  if [ "$JOBS" -eq 1 ]; then
    run_part "$SHARD_K" "$SHARD_N"
  else
    # JOBS memcheck processes at once: part (K-1)*JOBS+j of N*JOBS for
    # j = 1..JOBS, each a worker with its own log directory. Their lines are
    # printed once all have finished, in part order.
    for j in $(seq 1 "$JOBS"); do
      mkdir -p "$WORK/w$j"
      (
        LOGDIR="$WORK/w$j"; RAN=0; FAILED=0; FAILED_NAMES=""
        run_part $(( (SHARD_K - 1) * JOBS + j )) $(( SHARD_N * JOBS ))
        printf '%s\n%s\n%s\n' "$RAN" "$FAILED" "$FAILED_NAMES" > "$WORK/w$j/status"
      ) > "$WORK/w$j/out" 2>&1 &
    done
    wait
    for j in $(seq 1 "$JOBS"); do
      echo "  -- part $(( (SHARD_K - 1) * JOBS + j ))/$(( SHARD_N * JOBS )):"
      cat "$WORK/w$j/out"
      if [ -f "$WORK/w$j/status" ]; then
        { IFS= read -r st_ran; IFS= read -r st_failed; IFS= read -r st_names; } < "$WORK/w$j/status"
        RAN=$(( RAN + st_ran ))
        if [ "$st_failed" != 0 ]; then
          FAILED=1; FAILED_NAMES="$FAILED_NAMES${st_names:-}"
        fi
      else
        FAILED=1; FAILED_NAMES="$FAILED_NAMES part-$j(no-status)"
      fi
    done
  fi
fi

T_ALL=$(( $(date +%s) - T0 ))
n_skip=$(printf '%s' "$SKIP_APPLIED" | wc -w | tr -d ' ')
echo "--- G-VALGRIND summary: $RAN memcheck run(s) in ${T_ALL}s; crates without a test binary:${SKIPPED:- none}; $n_skip named test skip(s)${SKIP_APPLIED:+:$SKIP_APPLIED} ---"
if [ "$RAN" -eq 0 ] && [ "$FAILED" -eq 0 ]; then
  echo "G-VALGRIND BLOCKED: no test binary was run; nothing was checked."
  exit 3
fi
if [ "$FAILED" -ne 0 ]; then
  echo "G-VALGRIND RED:$FAILED_NAMES"
  exit 1
fi
echo "G-VALGRIND GREEN ($RAN memcheck run(s) clean, shard $SHARD_K/$SHARD_N, $JOBS job(s))."
exit 0
