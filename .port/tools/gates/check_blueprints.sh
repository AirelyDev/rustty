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
#   2. every .blp is already FORMATTED. `blueprint-compiler format` rewrites in
#      place and has no --check mode, so the only way to ask "was it already
#      formatted?" is to format and see whether anything changed. The reference
#      does this with `git diff --exit-code` in a following step.
#
# THIS GATE IS READ-ONLY. The reference's CI can afford to leave a reformatted
# worktree behind — it is a throwaway checkout. A developer's is not, so the
# formatting arm runs against a COPY and the real tree is never written to.
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
# Format a COPY, then diff against the originals. `blueprint-compiler format`
# has no --check mode and rewrites in place, so this is the only way to ask the
# question without touching the developer's tree.
cp -a "$ROOT/$UI_ROOT" "$WORK/ui"
find "$WORK/ui" -name '*.blp' -exec blueprint-compiler format {} \+ > "$WORK/fmt.log" 2>&1 || {
  tail -12 "$WORK/fmt.log" | sed 's/^/      /'
  red "blueprint-compiler format failed"
}
if ! diff -rq "$ROOT/$UI_ROOT" "$WORK/ui" > "$WORK/diff.log" 2>&1; then
  sed "s|$WORK/ui|<formatted>|g; s|$ROOT/$UI_ROOT|<repo>|g" "$WORK/diff.log" | head -12 | sed 's/^/      /'
  echo "    fix with: find $UI_ROOT -name '*.blp' -exec blueprint-compiler format {} +"
  red "at least one blueprint is not formatted (see above)"
fi
echo "    ok      all $n templates are already formatted"

echo "G-BLUEPRINT GREEN: $n templates compile and are formatted."
exit 0
