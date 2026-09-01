#!/usr/bin/env bash
# GATE G-I18N — the translation-catalogue lane.
#
# PORTS: ghostty `.github/scripts/check-translations.sh`, run by its
# `translations` CI job. That script does two things; rustty can honestly do
# one of them today, and this gate says which is which rather than pretending.
#
# ── WHAT THIS GATE CHECKS ────────────────────────────────────────────────────
# Every `po/*.po` catalogue agrees with `po/io.airely.rustty.pot`: `msgcmp
# --use-untranslated` each of the 34 catalogues against the template, so a
# msgid that was renamed or removed cannot leave a catalogue silently carrying
# a string that no longer exists, and a msgid added to the template cannot go
# unnoticed in a catalogue. `--use-untranslated` because an entry a translator
# has not filled in yet is not an error — only a MISSING or EXTRA msgid is.
#
# This has had NO coverage. `cargo test -p rustty-apprt --test
# i18n_msgid_coverage` checks the msgid SET against the sources; it never runs
# msgcmp and never looks at the 34 catalogues. `xtask/src/translations.rs`
# mentions msgcmp only in a doc comment.
#
# ── WHAT IT DELIBERATELY DOES NOT CHECK, AND WHY ─────────────────────────────
# The reference's script also regenerates the .pot and diffs it against the
# committed one, to catch a template that has drifted from the sources. That
# check CANNOT run here yet, and running it would be actively destructive:
#
#   `cargo run -p xtask -- update-translations` extracts only the GTK leg
#   (crates/rustty-apprt/{src/gtk,assets/ui}) and the Nautilus Python leg. The
#   committed template additionally carries 180 command-palette msgids
#   MIRRORED from the oracle's src/input/command.zig — their call sites are in
#   crates/rustty-input/src/command.rs, which does not carry `i18n::N_`
#   markers yet (that lands with the input wave, P4-E43). So a regeneration
#   drops all 180: measured, 253 msgids -> 83.
#
# That hazard is already guarded, and well: i18n_msgid_coverage.rs pins the
# command block at exactly 180 and fails loudly if a regeneration drops it.
# Adding a second, weaker check here would not improve on it. When command.rs
# gains its `N_()` markers and `xtask update-translations` walks
# crates/rustty-input, restore the reference's first arm here.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

POT="po/io.airely.rustty.pot"

blocked() { echo "G-I18N BLOCKED: $*"; exit 3; }
red()     { echo "G-I18N RED: $*" >&2; exit 1; }

# macOS puts the gettext tools in a keg-only Homebrew prefix.
if ! command -v msgcmp >/dev/null 2>&1; then
  for p in /opt/homebrew/opt/gettext/bin /usr/local/opt/gettext/bin; do
    [ -x "$p/msgcmp" ] && PATH="$p:$PATH" && export PATH && break
  done
fi
command -v msgcmp >/dev/null 2>&1 || blocked "msgcmp not installed (gettext)"
[ -f "$ROOT/$POT" ] || red "$POT is missing"

npo="$(find "$ROOT/po" -name '*.po' | wc -l | tr -d ' ')"
# Anti-vacuity: a glob matching nothing would make this a silent pass.
[ "$npo" -gt 0 ] || red "no po/*.po catalogues found — this gate would check nothing"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "=== G-I18N :: every catalogue matches $POT ($npo files) ==="
bad=0
for f in "$ROOT"/po/*.po; do
  name="$(basename "$f")"
  if ! msgcmp --use-untranslated "$f" "$ROOT/$POT" > "$WORK/$name.log" 2>&1; then
    echo "    FAIL    $name"
    head -8 "$WORK/$name.log" | sed 's/^/      /'
    bad=1
  fi
done
if [ "$bad" -ne 0 ]; then
  echo "    fix with: cargo run -p xtask -- update-translations"
  echo "    NOTE: that command currently drops the 180 mirrored command-palette"
  echo "    msgids (see this gate's header). Re-add them before committing."
  red "at least one catalogue disagrees with $POT (see above)"
fi
echo "    ok      all $npo catalogues agree with $POT"

echo "G-I18N GREEN: all $npo catalogues match $POT."
echo "  Not checked: whether $POT itself is current — see the gate header."
exit 0
