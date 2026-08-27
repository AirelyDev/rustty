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

# The crates whose tests cross into vendored C — exactly the set check_leaks.sh
# hands to ASan and then notes it cannot leak-check on macOS.
CRATES=("rustty-oniguruma" "rustty-wuffs" "rustty-vt-c")
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
  mapfile -t bins < <(python3 - "$WORK/$c.json" <<'PY'
import json, sys
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
            print(exe)
PY
)
  if [ "${#bins[@]}" -eq 0 ]; then
    echo "  skipped $c (no test binary produced)"
    SKIPPED="$SKIPPED $c"
    continue
  fi

  for b in "${bins[@]}"; do
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
      --suppressions="$SUPP" # WHY: valgrind.supp suppresses ONLY dyld/libSystem frames that fire before main() on every process — no rustty frame is suppressed, and each entry carries its own reason in that file.
    )
    if valgrind "${vg_opts[@]}" "$b" --test-threads=1 > "$WORK/run.log" 2>&1; then
      ok_line="$(grep -m1 '^test result:' "$WORK/run.log" || echo 'ran')"
      echo "  ok      $name — $ok_line"
      RAN=$((RAN+1))
    else
      # Distinguish a TEST failure from a valgrind finding: a red gate should
      # say which, because the fixes are entirely different.
      if grep -q "definitely lost\|Invalid read\|Invalid write\|Invalid free" "$WORK/run.log"; then
        grep -E "definitely lost|Invalid (read|write|free)|^==[0-9]+==    (at|by)" "$WORK/run.log" \
          | head -14 | sed 's/^/      /'
        echo "  FAIL    $name (valgrind finding)"
      else
        tail -14 "$WORK/run.log" | sed 's/^/      /'
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
