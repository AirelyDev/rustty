#!/usr/bin/env bash
# GATE G-VERSION — rustty's version numbers are the pinned oracle's.
#
# A rustty version says which Ghostty it ports, so `VERSION` is the oracle's
# `build.zig.zon` `.version` and `VT_LIB_VERSION` its `build.zig` `lib_version`,
# both read AT THE PIN (`ORACLE_PIN`), not at the oracle's HEAD. Advancing the
# pin without moving the numbers (or the reverse) goes RED here.
#
# Arms:
#   1. the rustty arm of rustty-identity: VERSION, VT_LIB_VERSION
#   2. the identity-oracle arm (already the oracle's values; guards its edits)
#   3. Cargo.toml [workspace.package] version == VERSION without its pre-release
#      (a tag `vX.Y.Z` must equal it, see `xtask release-info`)
#   4. CMakeLists.txt project(rustty-vt VERSION x.y.z) == VT_LIB_VERSION's core
#
# Exit 0 green, 1 red, 3 BLOCKED (oracle absent or the pin is not in it): R3
# keeps the oracle off CI, and ci.sh reports exit 3 as a warning, never green.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
# shellcheck source=_oracle_root.sh
. "$HERE/_oracle_root.sh"

IDENT="${RUSTTY_VERSION_GATE_IDENTITY:-$ROOT/crates/rustty-identity/src/lib.rs}"
CARGO="${RUSTTY_VERSION_GATE_CARGO:-$ROOT/Cargo.toml}"
CMAKE="${RUSTTY_VERSION_GATE_CMAKE:-$ROOT/CMakeLists.txt}"

if ! pin="$(rustty_oracle_pin)"; then
  echo "G-VERSION: BLOCKED: ORACLE_PIN is missing or not a 40-hex commit" >&2
  exit 3
fi
oracle="$(rustty_oracle_root "$ROOT")"
if [ ! -d "$oracle" ] || ! rustty_oracle_git -C "$oracle" cat-file -e "$pin^{commit}" 2>/dev/null; then
  echo "G-VERSION: BLOCKED: pin $pin is not available in the oracle checkout at $oracle" >&2
  exit 3
fi

oshow() { rustty_oracle_git -C "$oracle" show "$pin:$1"; }
app="$(oshow build.zig.zon | sed -n 's/^[[:space:]]*\.version = "\(.*\)",$/\1/p' | head -1)"
lib="$(oshow build.zig | sed -n 's/^const lib_version = "\(.*\)";$/\1/p' | head -1)"
if [ -z "$app" ] || [ -z "$lib" ]; then
  echo "G-VERSION: RED: could not read .version / lib_version from the oracle at $pin" >&2
  exit 1
fi
core() { printf '%s' "${1%%[-+]*}"; }

rc=0
red() { echo "G-VERSION: RED: $*" >&2; rc=1; }

# Both arms of the identity crate, in file order: rustty first, oracle second.
mapfile_ver="$(sed -n 's/^[[:space:]]*pub const VERSION: &str = "\(.*\)";$/\1/p' "$IDENT")"
mapfile_lib="$(sed -n 's/^[[:space:]]*pub const VT_LIB_VERSION: &str = "\(.*\)";$/\1/p' "$IDENT")"
n_ver="$(printf '%s\n' "$mapfile_ver" | grep -c .)"
n_lib="$(printf '%s\n' "$mapfile_lib" | grep -c .)"
if [ "$n_ver" -ne 2 ] || [ "$n_lib" -ne 2 ]; then
  red "expected two VERSION and two VT_LIB_VERSION constants in $IDENT (rustty and identity-oracle arms), found $n_ver and $n_lib"
else
  while IFS= read -r v; do
    [ "$v" = "$app" ] || red "identity VERSION is \"$v\", the oracle at the pin has \"$app\""
  done <<<"$mapfile_ver"
  while IFS= read -r v; do
    [ "$v" = "$lib" ] || red "identity VT_LIB_VERSION is \"$v\", the oracle at the pin has \"$lib\""
  done <<<"$mapfile_lib"
fi

cargo_v="$(awk '/^\[/{p=($0=="[workspace.package]")} p&&/^version[[:space:]]*=/{gsub(/.*=[[:space:]]*"|".*/,"");print;exit}' "$CARGO")"
[ "$cargo_v" = "$(core "$app")" ] ||
  red "Cargo.toml [workspace.package] version is \"$cargo_v\", expected \"$(core "$app")\" (core of \"$app\")"

cmake_v="$(sed -n 's/^project(rustty-vt VERSION \([0-9.]*\) .*/\1/p' "$CMAKE" | head -1)"
[ "$cmake_v" = "$(core "$lib")" ] ||
  red "CMakeLists.txt project(rustty-vt VERSION) is \"$cmake_v\", expected \"$(core "$lib")\" (core of \"$lib\")"

if [ "$rc" -eq 0 ]; then
  echo "G-VERSION: ok: app $app, lib $lib (oracle $pin)"
fi
exit "$rc"
