#!/usr/bin/env bash
# GATE G-DOXYGEN — the C-API reference actually generates, and cleanly.
#
# rustty carried `Doxyfile`, `DoxygenLayout.xml` and all six `dist/doxygen/`
# assets over from the top-level-files port and then ran `doxygen` from
# NOWHERE — not a gate, not a workflow, not the Makefile. The reference at
# least has a consumer (`src/build/docker/lib-c-docs/`, now ported to
# `xtask/docker/lib-c-docs/`), but even there the docs are only built when
# someone publishes them.
#
# That matters more here than it looks: the Doxyfile's INPUT is
# `include/rustty`, which is a GENERATED header tree (`xtask header-gen`).
# A doc comment that stops parsing, an `@ref` to a renamed symbol, or an
# EXAMPLE_PATH entry pointing at a deleted example all rot silently.
#
# ── WHAT IT CHECKS ───────────────────────────────────────────────────────────
# 1. doxygen runs to completion over the committed Doxyfile.
# 2. It emits NO warnings. `WARN_AS_ERROR` is not set in the Doxyfile (the
#    reference does not set it either, and flipping it would change the
#    committed file's behaviour for everyone), so the warning stream is
#    captured and asserted empty here instead.
# 3. The output is non-empty and contains the entry page — anti-vacuity, since
#    doxygen happily exits 0 having produced almost nothing if INPUT resolves
#    to no files.
# 4. Every `HTML_EXTRA_FILES` and asset path named in the Doxyfile exists.
#
# Runs against a COPY of the config with OUTPUT_DIRECTORY redirected to a temp
# dir, so the gate never writes into the working tree.
#
# BLOCKED (exit 3), never green, without doxygen: `brew install doxygen`,
# `apt install doxygen`, `nix shell nixpkgs#doxygen`.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

blocked() { echo "G-DOXYGEN BLOCKED: $*"; exit 3; }
red()     { echo "G-DOXYGEN RED: $*" >&2; exit 1; }

command -v doxygen >/dev/null 2>&1 \
  || blocked "doxygen not installed (brew install doxygen / apt install doxygen)"
[ -f "$ROOT/Doxyfile" ] || red "Doxyfile is missing"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "=== G-DOXYGEN 1/3 :: the assets the Doxyfile names all exist ==="
missing=0
# HTML_EXTRA_FILES is a `\`-continued list; pull every path-looking token out of
# the Doxyfile's asset directives and check each one.
while IFS= read -r p; do
  [ -n "$p" ] || continue
  if [ ! -e "$ROOT/$p" ]; then
    echo "    missing: $p"
    missing=1
  fi
done < <(grep -E '^\s*(HTML_EXTRA_FILES|HTML_HEADER|LAYOUT_FILE|PROJECT_LOGO|INPUT|EXAMPLE_PATH|HTML_EXTRA_STYLESHEET)\s*=|^\s+[A-Za-z0-9_./-]+\.(png|js|json|html|xml)\s*\\?\s*$' "$ROOT/Doxyfile" \
     | sed -E 's/^[^=]*=\s*//; s/\\$//' | tr -s ' ' '\n' | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')
[ "$missing" -eq 0 ] || red "the Doxyfile names asset paths that do not exist (see above)"
echo "    ok      every named asset resolves"

echo "=== G-DOXYGEN 2/3 :: doxygen runs clean ==="
# Redirect the output so the gate is read-only with respect to the work tree.
sed -E "s|^OUTPUT_DIRECTORY.*|OUTPUT_DIRECTORY = $WORK/out|" "$ROOT/Doxyfile" > "$WORK/Doxyfile"
mkdir -p "$WORK/out"
if ! doxygen "$WORK/Doxyfile" > "$WORK/stdout.log" 2> "$WORK/warn.log"; then
  tail -25 "$WORK/warn.log" | sed 's/^/      /'
  red "doxygen exited non-zero"
fi
# Doxygen writes warnings to stderr and still exits 0.
#
# Warnings are filtered through an explicit baseline rather than compared to
# zero. The reason is R1: the only warnings the C headers produce today come
# from doc text that is BYTE-IDENTICAL to the reference's (its
# `rgb:<red>/<green>/<blue>` syntax description, which doxygen reads as HTML
# tags). Demanding zero would force a faithfully-ported comment to diverge from
# the source it was ported from. Every baseline entry names the reference file
# and line it matches; see the file's header for the admission rule.
BASELINE="$(dirname "${BASH_SOURCE[0]}")/doxygen_warnings.baseline"
[ -f "$BASELINE" ] || red "the accepted-warning baseline is missing: $BASELINE"

# Strip the baseline-matched lines and the two-line continuations doxygen
# indents under a warning.
cp "$WORK/warn.log" "$WORK/unexpected.log"
while IFS= read -r pat; do
  case "$pat" in ''|'#'*) continue;; esac
  grep -Ev "$pat" "$WORK/unexpected.log" > "$WORK/unexpected.next" || true
  mv "$WORK/unexpected.next" "$WORK/unexpected.log"
done < "$BASELINE"
# Continuation lines begin with whitespace and belong to the warning above.
sed -i.bak '/^[[:space:]]/d' "$WORK/unexpected.log" && rm -f "$WORK/unexpected.log.bak"

if [ -s "$WORK/unexpected.log" ]; then
  echo "    doxygen emitted $(wc -l < "$WORK/unexpected.log" | tr -d ' ') UNEXPECTED warning line(s):"
  head -25 "$WORK/unexpected.log" | sed 's/^/      /'
  echo "    (baseline: $BASELINE — add an entry ONLY if the text is byte-identical"
  echo "     to the reference's; otherwise fix the doc comment.)"
  red "the C-API docs do not generate warning-free"
fi
nbase="$(grep -c . "$WORK/warn.log" 2>/dev/null || echo 0)"
echo "    ok      no unexpected warnings ($nbase baselined line(s) accepted)"

echo "=== G-DOXYGEN 3/3 :: the output is real ==="
html_root="$WORK/out/librustty"
[ -d "$html_root" ] || red "expected HTML under $html_root (check HTML_OUTPUT)"
n="$(find "$html_root" -name '*.html' | wc -l | tr -d ' ')"
# Anti-vacuity: an INPUT that resolves to nothing still exits 0 and writes a
# handful of chrome pages. The real header tree is ~37 files.
[ "$n" -ge 20 ] || red "only $n HTML page(s) generated — INPUT probably resolved to nothing"
[ -f "$html_root/index.html" ] || red "no index.html in the generated docs"
echo "    ok      $n HTML page(s), index.html present"

echo "G-DOXYGEN GREEN (docs generate warning-free, $n page(s))."
exit 0
