#!/usr/bin/env bash
# GATE G-VALGRIND — FFI leak detection under Valgrind memcheck.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `valgrind` job
# (`zig build test-valgrind`), and it CLOSES A DEFERRAL rustty already wrote
# down. check_leaks.sh says, in its header and again at line ~201:
#
#   "FFI *leak* detection: LSan/valgrind are unavailable on arm64 macOS, so
#    FFI leak checks are deferred to Linux CI — see DoP §G6."
#
# G6 covers UB across the FFI boundary with AddressSanitizer, which on macOS is
# built with `detect_leaks=0` because Apple's ASan has no leak detector. So the
# crates that cross into C have had NO leak coverage anywhere. This is that
# lane, and Linux CI is where it runs.
#
# WHAT IT RUNS. The compiled test binary of each FFI crate, under
# `valgrind --leak-check=full --error-exitcode=1` with the repo's
# valgrind.supp. `--error-exitcode=1` is what makes it a gate rather than a
# report: without it valgrind exits 0 no matter what it finds.
#
# `--errors-for-leak-kinds=definite` deliberately: `possible` leaks fire
# constantly on Rust's thread-locals and on interior pointers into
# Vec/Box allocations, which would make this gate noise. A DEFINITE leak — a
# block with no remaining pointer to it — is unambiguous.
#
# BLOCKED (exit 3), never green, without valgrind. That is every macOS host
# (valgrind has no working arm64-Darwin port) and any Linux box without it
# installed; `nix shell nixpkgs#valgrind` provides it, and so does
# `apt install valgrind libc6-dbg`.
#
# WITHOUT ROOT (the Fedora VM, 2026-09-27, N405): `dnf download valgrind`,
# `rpm2cpio valgrind-*.rpm | cpio -idm` into a user dir, and put a two-line
# `valgrind` wrapper first on PATH that sets
# VALGRIND_LIB=<dir>/usr/libexec/valgrind and execs <dir>/usr/bin/valgrind
# (the launcher otherwise looks for its tools under /usr/libexec). No
# glibc debuginfo was needed there. The default crate set (11 test binaries)
# took 33 min there, 25 of them in rustty-vt-c's 553 lib tests.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
#
# Usage:
#   bash .port/tools/gates/check_valgrind.sh              # the default crate set
#   bash .port/tools/gates/check_valgrind.sh rustty-oniguruma
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

SUPP="$ROOT/valgrind.supp"

blocked() { echo "G-VALGRIND BLOCKED: $*"; exit 3; }
red()     { echo "G-VALGRIND RED: $*" >&2; exit 1; }

command -v valgrind >/dev/null 2>&1 \
  || blocked "valgrind not installed (nix shell nixpkgs#valgrind, or apt install valgrind libc6-dbg)"
command -v cargo >/dev/null 2>&1 || blocked "cargo not installed"
[ -f "$SUPP" ] || red "valgrind.supp is missing"

# The crates whose tests cross into vendored C: the set check_leaks.sh hands
# to ASan and then notes it cannot leak-check on macOS, plus (N404,
# 2026-09-27) rustty-font-shaper (FreeType, HarfBuzz, Fontconfig; its first
# memcheck run found 88 definite leaks, now fixed), rustty-simd (simdutf,
# highway; clean) and rustty-xev (the event loop under every termio thread;
# N406's leaked loop box, now fixed). The reference's `test-valgrind` runs its
# WHOLE unit-test executable under memcheck; the crates not yet listed here
# are recorded in NEW-FINDINGS N404.
CRATES=("rustty-oniguruma" "rustty-wuffs" "rustty-vt-c" "rustty-font-shaper" "rustty-simd" "rustty-xev")
if [ "$#" -gt 0 ]; then
  CRATES=("$@")
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

RAN=0
FAILED=0
FAILED_NAMES=""
SKIPPED=""

echo "=== G-VALGRIND :: memcheck over the FFI crates ==="
for c in "${CRATES[@]}"; do
  # `--no-run` and parse the JSON so the exact test binary is known, rather
  # than globbing target/debug/deps and hoping. Same reasoning as the
  # xcframework archive discovery in xtask.
  if ! cargo test -p "$c" --no-run --message-format=json > "$WORK/$c.json" 2>"$WORK/$c.build.log"; then
    tail -15 "$WORK/$c.build.log" | sed 's/^/      /'
    echo "  FAIL    $c (build)"
    FAILED=1; FAILED_NAMES="$FAILED_NAMES $c(build)"
    continue
  fi
  # Test executables appear as `"test":true` compiler-artifact entries.
  # Each line is `<executable>\t<package dir>`: `cargo test` runs a test
  # binary with its package directory as the working directory and
  # CARGO_MANIFEST_DIR set, and tests rely on both (rustty-vt's
  # `image_load_rgb_not_compressed_relative_regular_file` asserts the CWD), so
  # each binary runs the same way here (N407).
  mapfile -t bins < <(python3 - "$WORK/$c.json" <<'PY'
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
    echo "  skipped $c (no test binary produced)"
    SKIPPED="$SKIPPED $c"
    continue
  fi

  for entry in "${bins[@]}"; do
    b="${entry%%$'\t'*}"
    pkg_dir="${entry#*$'\t'}"
    name="$c/$(basename "$b" | sed 's/-[0-9a-f]*$//')"
    # An ARRAY rather than a `\`-continued command line so each option can
    # carry its reason on its OWN line — D6's suppression audit (rule c)
    # requires the WHY to sit on the same physical line as the toggle, and a
    # backslash continuation makes a trailing `#` comment impossible.
    vg_opts=(
      --leak-check=full
      --errors-for-leak-kinds=definite
      --error-exitcode=1
      --num-callers=50
      --suppressions="$SUPP" # WHY: valgrind.supp is the reference's file: its 233 entries name GTK/GLib/GDK/fontconfig frames of the GUI app, Mesa DRI driver objects, or DRD-only kinds (audited 2026-09-27, N403); none names a frame these crates execute, so it cannot hide a leak in rustty or vendored-C code.
    )
    if (cd "$pkg_dir" && CARGO_MANIFEST_DIR="$pkg_dir" valgrind "${vg_opts[@]}" "$b" --test-threads=1) > "$WORK/run.log" 2>&1; then
      ok_line="$(grep -m1 '^test result:' "$WORK/run.log" || echo 'ran')"
      echo "  ok      $name — $ok_line"
      RAN=$((RAN+1))
    else
      # Distinguish a TEST failure from a valgrind finding: a red gate should
      # say which, because the fixes are entirely different. The deciding
      # line is memcheck's own `ERROR SUMMARY: N errors` (what
      # --error-exitcode acts on), not a text match: every run that ends with
      # unfreed memory prints `definitely lost: 0 bytes` in its LEAK SUMMARY,
      # so matching "definitely lost" called a plain test failure a finding
      # (N403).
      vg_errors="$(sed -n 's/^==[0-9]*== ERROR SUMMARY: \([0-9,]*\) errors.*/\1/p' "$WORK/run.log" | tail -1 | tr -d ,)"
      if [ "${vg_errors:-0}" -gt 0 ]; then
        # Print only the ERROR records: every record with a stack before
        # HEAP SUMMARY (memcheck prints stacks there only for errors), and
        # after it only the `definitely lost` leak records. A plain
        # at/by grep showed whichever stack came first — measured on the
        # negative control, a `possibly lost` thread-local record (N403).
        awk '
          /HEAP SUMMARY:/ { heap = 1 }
          /^==[0-9]+== [^ ]/ {
            hdr = $0; pending = 0
            if (heap) { keep = ($0 ~ /are definitely lost in loss record/); if (keep) print }
            else { keep = 1; pending = 1 }
            next
          }
          /^==[0-9]+==    (at|by) / {
            if (pending) { print hdr; pending = 0 }
            if (keep) print
            next
          }
          { keep = 0; pending = 0 }
        ' "$WORK/run.log" | head -40 | sed 's/^/      /'
        echo "  FAIL    $name (valgrind finding: $vg_errors error(s))"
      else
        # The run log ENDS with memcheck's own summary, so its tail hides the
        # failing test; show libtest's failure lines instead (N403).
        if grep -qE "panicked at|^test .* FAILED$" "$WORK/run.log"; then
          grep -E -A2 "panicked at|^test .* FAILED$" "$WORK/run.log" | head -20 | sed 's/^/      /'
        else
          tail -14 "$WORK/run.log" | sed 's/^/      /'
        fi
        echo "  FAIL    $name (test failure, not a valgrind finding)"
      fi
      FAILED=1; FAILED_NAMES="$FAILED_NAMES $name"
      RAN=$((RAN+1))
    fi
  done
done

echo "--- G-VALGRIND summary: $RAN binary/binaries run,$(printf '%s' "${SKIPPED:- none}") skipped ---"
if [ "$RAN" -eq 0 ]; then
  echo "G-VALGRIND BLOCKED: no test binary was run; nothing was checked."
  exit 3
fi
if [ "$FAILED" -ne 0 ]; then
  echo "G-VALGRIND RED:$FAILED_NAMES"
  exit 1
fi
echo "G-VALGRIND GREEN ($RAN binary/binaries clean under memcheck)."
exit 0
