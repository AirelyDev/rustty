#!/usr/bin/env bash
# _target_dir.sh — where cargo really puts this workspace's artifacts.
#
# SOURCE it from a gate that reads a built artifact (librustty_vt_c.*, a .wasm)
# and call `rustty_target_dir "$ROOT"`. It prints cargo's own answer,
# `cargo metadata`'s `target_directory`, which honours CARGO_TARGET_DIR,
# CARGO_BUILD_TARGET_DIR and any `build.target-dir` config, so the gate reads
# the artifact its own `cargo build` just wrote. A literal `$ROOT/target/...`
# reads a different (possibly stale, possibly absent) file as soon as ci.sh or
# a caller sets CARGO_TARGET_DIR. If cargo metadata fails, the default
# `$ROOT/target` is printed (the caller's own build has already failed then).
# G-TGTDIR (check_target_dir.sh) keeps the registered gates on this helper.
rustty_target_dir() {
  local root="$1" d
  d="$(cd "$root" && cargo metadata --no-deps --format-version 1 --offline 2>/dev/null \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])' 2>/dev/null)"
  [ -n "$d" ] || d="$root/target"
  printf '%s\n' "$d"
}
