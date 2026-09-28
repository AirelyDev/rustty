#!/usr/bin/env bash
# GATE G-VALGRIND — the workspace's test binaries under Valgrind memcheck.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `valgrind` job
# (`zig build test-valgrind`, `build.zig:411-422`), and it CLOSES A DEFERRAL
# rustty already wrote down. check_leaks.sh says, in its header and again at
# line ~201:
#
#   "FFI *leak* detection: LSan/valgrind are unavailable on arm64 macOS, so
#    FFI leak checks are deferred to Linux CI — see DoP §G6."
#
# G6 covers UB across the FFI boundary with AddressSanitizer, which on macOS is
# built with `detect_leaks=0` because Apple's ASan has no leak detector. So the
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
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
#
# Usage:
#   bash .port/tools/gates/check_valgrind.sh                    # the default scope
#   bash .port/tools/gates/check_valgrind.sh --shard 3/4 --jobs 4   # one CI job
#   bash .port/tools/gates/check_valgrind.sh rustty-oniguruma   # named crates
#   bash .port/tools/gates/check_valgrind.sh rustty-apprt:gtk,wayland,x11
#   bash .port/tools/gates/check_valgrind.sh --list-scope
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

SUPP="$ROOT/valgrind.supp"
PORT_SUPP="$ROOT/.port/tools/gates/valgrind-llvmpipe.supp"

blocked() { echo "G-VALGRIND BLOCKED: $*"; exit 3; }
red()     { echo "G-VALGRIND RED: $*" >&2; exit 1; }

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

# Tests memcheck cannot run, by exact name, each with its reason. Printed by
# name on every run, never dropped silently; each still runs in every
# `cargo test` lane. Format: `<crate>/<binary>|<test path>|<reason>`.
VALGRIND_SKIPS=(
  "rustty-cli/rustty_cli|edit_config::exec_tests::exec_failure_report_prints_the_raw_command_bytes|the test makes execve(2) fail with E2BIG in its own process (a 4 MiB command); valgrind cannot survive a failed execve of the program it runs (\"EXEC FAILED: I can't recover from execve() failing, so I'm dying\"), which kills the whole test binary (N416)"
)

SHARD_K=1
SHARD_N=1
JOBS=1
LIST_ONLY=0
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
    -*) red "unknown option $1" ;;
    *) CRATES+=("$1"); shift ;;
  esac
done
case "$SHARD_K$SHARD_N$JOBS" in *[!0-9]*|"") red "--shard wants K/N and --jobs a count, all integers" ;; esac
[ "$SHARD_N" -ge 1 ] && [ "$SHARD_K" -ge 1 ] && [ "$SHARD_K" -le "$SHARD_N" ] \
  || red "--shard wants 1 <= K <= N, got $SHARD_K/$SHARD_N"
[ "$JOBS" -ge 1 ] || red "--jobs wants at least 1"
[ "${#CRATES[@]}" -gt 0 ] || CRATES=("${DEFAULT_SCOPE[@]}")

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
fi

RAN=0
FAILED=0
FAILED_NAMES=""
SKIPPED=""
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
  --suppressions="$SUPP" # WHY: valgrind.supp is the reference's file: its 233 entries name GTK/GLib/GDK/fontconfig frames of the GUI app, Mesa DRI driver objects, or DRD-only kinds (audited 2026-09-27, N403); none names a frame these crates execute, so it cannot hide a leak in rustty or vendored-C code.
  --suppressions="$PORT_SUPP" # WHY: llvmpipe (Mesa's software GL) noise only, each entry justified in the file (N412): LLVM codegen frames under gallivm_jit_function, and the two golden tests' own comparisons of llvmpipe-blended pixels; a rustty function below those frames is still checked.
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
  if ! cargo build -p "$c" "${feat[@]}" > "$WORK/build.log" 2>&1; then
    tail -15 "$WORK/build.log" | sed 's/^/      /'
    echo "  FAIL    $entry (build)"
    FAILED=1; FAILED_NAMES="$FAILED_NAMES $entry(build)"
    continue
  fi
  # `--no-run` and parse the JSON so the exact test binary is known, rather
  # than globbing target/debug/deps and hoping. Same reasoning as the
  # xcframework archive discovery in xtask.
  if ! cargo test -p "$c" "${feat[@]}" --no-run --message-format=json > "$WORK/build.json" 2>"$WORK/build.log"; then
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
  mapfile -t bins < <(python3 - "$WORK/build.json" <<'PY'
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
  if [ "${#bins[@]}" -eq 0 ]; then
    echo "  skipped $entry (no test binary produced)"
    SKIPPED="$SKIPPED $entry"
    continue
  fi
  for line in "${bins[@]}"; do
    b="${line%%$'\t'*}"
    bin_name="$(basename "$b" | sed 's/-[0-9a-f]*$//')"
    # This binary's VALGRIND_SKIPS, printed by name.
    skip_list=""
    for s in "${VALGRIND_SKIPS[@]}"; do
      IFS='|' read -r s_bin s_test s_why <<< "$s"
      if [ "$s_bin" = "$c/$bin_name" ]; then
        skip_list="$skip_list|$s_test"
        echo "  skip    $c/$bin_name::$s_test — $s_why"
      fi
    done
    BIN_NAME+=("$entry/$bin_name")
    BIN_PKG+=("${line#*$'\t'}")
    BIN_EXE+=("$b")
    BIN_SKIPS+=("${skip_list#|}")
  done
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
    mapfile -t all < <(cd "$pkg_dir" && CARGO_MANIFEST_DIR="$pkg_dir" "$b" --list --format terse 2>/dev/null | sed -n 's/: test$//p')
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
      for s in "${skips[@]}"; do
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
        mapfile -t st < "$WORK/w$j/status"
        RAN=$(( RAN + st[0] ))
        if [ "${st[1]}" != 0 ]; then
          FAILED=1; FAILED_NAMES="$FAILED_NAMES${st[2]:-}"
        fi
      else
        FAILED=1; FAILED_NAMES="$FAILED_NAMES part-$j(no-status)"
      fi
    done
  fi
fi

T_ALL=$(( $(date +%s) - T0 ))
echo "--- G-VALGRIND summary: $RAN memcheck run(s) in ${T_ALL}s,$(printf '%s' "${SKIPPED:- none}") skipped ---"
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
