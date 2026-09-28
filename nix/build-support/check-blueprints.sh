#!/usr/bin/env bash
#
# PORTS: ghostty nix/build-support/check-blueprints.sh.
#
# Two differences from the reference, both structural:
#   * the search root is crates/rustty-apprt/assets/ui, not src/ — that is
#     where rustty's 19 .blp files live (versioned 1.2 … 1.5 subdirectories,
#     same as the reference's layout under src/apprt/gtk-ng/ui);
#   * `blueprint-compiler format` without `--fix` is check-only: it exits 1 on
#     an unformatted file (measured on 0.20.4, N400). CI still pairs this
#     script with `git diff --exit-code` exactly as the reference's job does.

set -o nounset -o pipefail -o errexit

ui_root="crates/rustty-apprt/assets/ui"

if [ ! -d "$ui_root" ]; then
  echo "check-blueprints: $ui_root does not exist" >&2
  exit 1
fi

# Anti-vacuity: an empty match set would make this script exit 0 while
# checking nothing, which is exactly the failure mode a formatting gate must
# not have.
count=$(find "$ui_root" -name '*.blp' | wc -l)
if [ "$count" -eq 0 ]; then
  echo "check-blueprints: no .blp files under $ui_root — nothing was checked" >&2
  exit 1
fi
echo "check-blueprints: $count blueprint file(s)"

find "$ui_root" -name \*.blp -exec blueprint-compiler format {} \+
find "$ui_root" -name \*.blp -exec blueprint-compiler compile --output=/dev/null {} \;
