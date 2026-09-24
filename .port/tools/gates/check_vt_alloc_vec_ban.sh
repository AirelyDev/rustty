#!/usr/bin/env bash
# check_vt_alloc_vec_ban.sh — GATE G-VTALLOC (parity tail S7-1)
#
# WHY THIS GATE EXISTS
# --------------------
# The S-7 allocator work (`.port/parity-closure/S7-ALLOCATOR-DESIGN.md`) moves
# the vt crates' caller-routed containers onto allocator_api2's `Vec<T, A>`.
# Its stable `Vec` has no `Copy` specializations: `extend_from_slice` is a
# per-element `extend(iter.cloned())` loop and `resize` a per-element write
# loop, where std lowers both to one memcpy / fill. The naive swap cost +81%
# on `screen-clone.render` (N261). The fix is `rustty_vt::alloc::CopyVecExt`;
# the guard is a clippy `disallowed-methods` ban on the two allocator_api2
# methods in `crates/rustty-vt/clippy.toml` and `crates/rustty-vt-c/clippy.toml`,
# enforced by ci.sh's `cargo clippy --workspace --all-targets -- -D warnings`.
#
# A clippy config ban can silently stop working: the file can be deleted or
# edited, the two copies can drift, a path can stop resolving after an
# allocator_api2 upgrade (clippy then only warns about the config, and the ban
# matches nothing), or a site can `#[allow]` it. This gate pins all of that:
#
#   1. CONFIG arm: both clippy.toml files exist, are byte-identical, and name
#      both banned paths.
#   2. ESCAPE arm (`check_vt_alloc_vec_ban_escape.py`): nothing may switch the
#      lint off. RED on any mention of `disallowed_methods`, `clippy::style` or
#      `clippy::all` in either crate's .rs files (allow/expect, combined lists,
#      multi-line and inner attributes, cfg_attr), on allow/expect(warnings), on
#      a crate `[lints]` table other than `workspace = true`, and on the
#      workspace `[workspace.lints.clippy]` allowing disallowed_methods, style or
#      all. The ban itself also cannot see `Extend::extend` (see clippy.toml).
#   3. --self-test: the ESCAPE arm's own self-test (a clean synthetic tree
#      passes; eleven planted spellings and a tree without the crates each go
#      RED), then the FIRES arm: a throwaway crate outside the workspace
#      (`fixtures/vt_alloc_vec_ban.rs`) is linted with each crate's config
#      (CLIPPY_CONF_DIR). EXACTLY the two `BANNED` lines must be reported, at
#      their line numbers, and std `Vec`'s same-named methods must not be.
#      Negative control: with an empty config dir the same fixture must report
#      ZERO, so the hits come from the config and not from anything else.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
#
# Usage:
#   check_vt_alloc_vec_ban.sh              # arms 1-2 (no compile)
#   check_vt_alloc_vec_ban.sh --self-test  # arm 3 (builds the fixture)
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

VT_CONF="crates/rustty-vt/clippy.toml"
VTC_CONF="crates/rustty-vt-c/clippy.toml"
FIXTURE=".port/tools/gates/fixtures/vt_alloc_vec_ban.rs"
BANNED_PATHS=(
  "allocator_api2::vec::Vec::extend_from_slice"
  "allocator_api2::vec::Vec::resize"
)

fail() { echo "G-VTALLOC: FAIL: $*"; exit 1; }
blocked() { echo "G-VTALLOC: BLOCKED: $*"; exit 3; }

config_arm() {
  for f in "$VT_CONF" "$VTC_CONF"; do
    [ -f "$f" ] || fail "$f is missing"
    for p in "${BANNED_PATHS[@]}"; do
      grep -q "path = \"$p\"" "$f" || fail "$f does not ban $p"
    done
  done
  cmp -s "$VT_CONF" "$VTC_CONF" || fail "$VT_CONF and $VTC_CONF differ"
  echo "  ok      config: both clippy.toml files ban ${#BANNED_PATHS[@]} paths, identical"
}

ESCAPE_PY=".port/tools/gates/check_vt_alloc_vec_ban_escape.py"

escape_arm() {
  command -v python3 >/dev/null 2>&1 || blocked "python3 not on PATH"
  python3 "$ESCAPE_PY" "$ROOT" || fail "the ban is suppressed (see ESCAPE lines above)"
  echo "  ok      escape: nothing in the vt crates or the workspace lints switches the ban off"
}

# lint_fixture <conf-dir> -> prints the line numbers clippy flags as
# disallowed_methods in the fixture, one per line. Exits 3 if clippy cannot
# build the fixture (a broken environment, not a pass).
lint_fixture() {
  local conf="$1" out
  # Each config lints into its own target dir, so cargo never replays another
  # config's cached diagnostics.
  local tdir="$WORK/target-$(printf '%s' "$conf" | tr -c 'A-Za-z0-9' '_')"
  out="$(cd "$WORK/crate" && CLIPPY_CONF_DIR="$conf" CARGO_TARGET_DIR="$tdir" \
    cargo clippy --offline --quiet --message-format=short -- \
    -W clippy::disallowed_methods 2>&1)"
  local rc=$?
  if [ $rc -ne 0 ]; then
    echo "$out" >&2
    return 3
  fi
  printf '%s\n' "$out" | sed -nE 's#^src/lib\.rs:([0-9]+):[0-9]+: warning: use of a disallowed method.*#\1#p'
}

self_test() {
  python3 "$ESCAPE_PY" --self-test || fail "the ESCAPE arm's self-test failed"
  command -v cargo >/dev/null 2>&1 || blocked "cargo not on PATH"
  [ -f "$FIXTURE" ] || fail "$FIXTURE is missing"
  WORK="$(mktemp -d "${TMPDIR:-/tmp}/g-vtalloc.XXXXXX")" || blocked "mktemp failed"
  trap 'rm -rf "$WORK"' EXIT
  mkdir -p "$WORK/crate/src" "$WORK/empty-conf"
  cp "$FIXTURE" "$WORK/crate/src/lib.rs"
  # The allocator_api2 version the workspace resolves (Cargo.lock), so the ban
  # is proven against the paths the vt crates actually compile.
  local ver
  ver="$(awk '/^name = "allocator-api2"$/ { getline; gsub(/version = |"/, ""); print; exit }' Cargo.lock)"
  [ -n "$ver" ] || fail "allocator-api2 is not in Cargo.lock"
  printf '[package]\nname = "g_vtalloc_fixture"\nversion = "0.0.0"\nedition = "2021"\npublish = false\n\n[dependencies]\nallocator-api2 = "=%s"\n\n[workspace]\n' \
    "$ver" > "$WORK/crate/Cargo.toml"

  local want got
  want="$(grep -n 'BANNED' "$WORK/crate/src/lib.rs" | grep -v '^[0-9]*://' | cut -d: -f1 | sort -n | tr '\n' ' ')"
  [ "$(echo "$want" | wc -w | tr -d ' ')" = "${#BANNED_PATHS[@]}" ] \
    || fail "fixture must mark exactly ${#BANNED_PATHS[@]} BANNED lines (got: $want)"

  for conf in crates/rustty-vt crates/rustty-vt-c; do
    got="$(lint_fixture "$ROOT/$conf")" || blocked "clippy could not build the fixture under $conf"
    got="$(printf '%s\n' "$got" | sed '/^$/d' | sort -n | tr '\n' ' ')"
    [ "$got" = "$want" ] || fail "under $conf/clippy.toml the ban flagged lines [$got], want [$want]"
    echo "  ok      fires: $conf/clippy.toml flags exactly lines [$want]"
  done

  got="$(lint_fixture "$WORK/empty-conf")" || blocked "clippy could not build the fixture with no config"
  got="$(printf '%s\n' "$got" | sed '/^$/d' | tr '\n' ' ')"
  [ -z "$got" ] || fail "negative control: with no config clippy still flagged [$got]"
  echo "  ok      control: with no config the fixture flags nothing"
}

echo "G-VTALLOC: allocator_api2 Vec slow-method ban (N261)"
config_arm
escape_arm
if [ "${1:-}" = "--self-test" ]; then
  self_test
fi
echo "G-VTALLOC: PASS"
exit 0
