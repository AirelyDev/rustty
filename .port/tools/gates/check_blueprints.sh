#!/usr/bin/env bash
# GATE G-BLUEPRINT — the GTK Blueprint lane.
#
# PORTS: ghostty `.github/workflows/test.yml`'s `blueprint-compiler` job, which
# runs `nix/build-support/check-blueprints.sh` and then `git diff --exit-code`.
#
# rustty ported that script during the Nix work and then WIRED IT TO NOTHING —
# exactly the "config checked in, nothing runs it" shape this whole gate suite
# exists to eliminate. This is what runs it.
#
# TWO PROPERTIES, and the second is the one that needs the git dance:
#   1. every .blp COMPILES (`blueprint-compiler compile`), so a template that
#      would fail at build time fails here instead;
#   2. every .blp is already FORMATTED. `blueprint-compiler format` WITHOUT
#      `-f/--fix` is the check: it prints the diff it would apply and exits 1
#      when any file is unformatted (measured on 0.20.4, N400: a mis-indented
#      template gave "1 file would be formatted" and exit 1, the file
#      untouched). Only `--fix` rewrites. The reference's job follows its
#      script with `git diff --exit-code`, a second net for a compiler that
#      rewrites.
#
# THIS GATE IS READ-ONLY. The formatting arm runs against a COPY and then also
# diffs the copy against the tree, so even a compiler version that rewrote
# could never modify a developer's tree, and such a rewrite is still caught.
#
# BLOCKED (exit 3), never green, without blueprint-compiler — which is a GTK
# tool and is not packaged for macOS. The Nix dev shell provides it, and so
# does `dnf install blueprint-compiler` on the Linux side.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

UI_ROOT="crates/rustty-apprt/assets/ui"

blocked() { echo "G-BLUEPRINT BLOCKED: $*"; exit 3; }
red()     { echo "G-BLUEPRINT RED: $*" >&2; exit 1; }

command -v blueprint-compiler >/dev/null 2>&1 \
  || blocked "blueprint-compiler not installed (nix develop, or dnf/apt install blueprint-compiler)"
[ -d "$ROOT/$UI_ROOT" ] || red "$UI_ROOT does not exist"

n="$(find "$ROOT/$UI_ROOT" -name '*.blp' | wc -l | tr -d ' ')"
# Anti-vacuity: an empty match set would make both arms silent passes.
[ "$n" -gt 0 ] || red "no .blp files under $UI_ROOT — this gate would check nothing"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "=== G-BLUEPRINT 1/2 :: compile ($n templates) ==="
bad=0
while IFS= read -r f; do
  if ! blueprint-compiler compile --output=/dev/null "$f" > "$WORK/compile.log" 2>&1; then
    echo "    FAIL    ${f#"$ROOT"/}"
    head -12 "$WORK/compile.log" | sed 's/^/      /'
    bad=1
  fi
done < <(find "$ROOT/$UI_ROOT" -name '*.blp' | sort)
[ "$bad" -eq 0 ] || red "at least one blueprint does not compile (see above)"
echo "    ok      all $n templates compile"

echo "=== G-BLUEPRINT 2/2 :: formatting ==="
# Check a COPY, then also diff it against the originals. `blueprint-compiler
# format` (no `--fix`) exits 1 on an unformatted file and prints the diff; the
# diff arm below catches a compiler that rewrote instead.
cp -a "$ROOT/$UI_ROOT" "$WORK/ui"
fix_hint="    fix with: find $UI_ROOT -name '*.blp' -exec blueprint-compiler format --fix {} +"
find "$WORK/ui" -name '*.blp' -exec blueprint-compiler format {} \+ > "$WORK/fmt.log" 2>&1 || {
  tail -12 "$WORK/fmt.log" | sed 's/^/      /'
  echo "$fix_hint"
  red "blueprint-compiler format reported unformatted templates or failed (see above)"
}
if ! diff -rq "$ROOT/$UI_ROOT" "$WORK/ui" > "$WORK/diff.log" 2>&1; then
  sed "s|$WORK/ui|<formatted>|g; s|$ROOT/$UI_ROOT|<repo>|g" "$WORK/diff.log" | head -12 | sed 's/^/      /'
  echo "$fix_hint"
  red "at least one blueprint is not formatted (see above)"
fi
echo "    ok      all $n templates are already formatted"

echo "G-BLUEPRINT GREEN: $n templates compile and are formatted."
exit 0
