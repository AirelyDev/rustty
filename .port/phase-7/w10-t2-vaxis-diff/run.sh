#!/usr/bin/env bash
# run.sh — rustty-vaxis uucode differential (the S-10b recipe, NEW-FINDINGS
# N240, re-run for uucode 9d555245 / Unicode 18 by P7-W10 T2).
#
# The Zig probe (probe.zig) is built against uucode 9d555245 with ghostty's
# uucode_config.zig (the G4-uni harness copy, verbatim from the reference pin),
# as SharedDeps.zig wires the singleton uucode into libvaxis, and dumps the four
# vaxis fields over 0..=0x10FFFF plus uucode's `computeGraphemeBreak` over every
# (state row, gb1, gb2). `cargo run --example dump_uucode -p rustty-vaxis`
# dumps the port's tables and algorithm in the same layout; both must be
# byte-identical.
#
# Exit: 0 GREEN, 3 BLOCKED (no zig), other RED.
# R3: harness-only; builds in a temp dir, never in the rustty tree.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSTTY_ROOT="$(cd "$HERE/../../.." && pwd)"
WORK="$(mktemp -d)"

command -v zig >/dev/null 2>&1 || { echo ">> BLOCKED: zig not found" >&2; exit 3; }

cp "$HERE/build.zig" "$HERE/build.zig.zon" "$HERE/probe.zig" "$WORK/"
cp "$RUSTTY_ROOT/.port/phase-0/harness/unicode/uucode_config.zig" "$WORK/"

( cd "$WORK" && zig build && ./zig-out/bin/vaxis-probe ) || { echo ">> RED: probe failed" >&2; exit 1; }
( cd "$RUSTTY_ROOT" && cargo run --quiet --example dump_uucode -p rustty-vaxis -- "$WORK" ) \
  || { echo ">> RED: rust dumper failed" >&2; exit 1; }

rc=0
for pair in "zig_fields.bin rust_fields.bin" "zig_gb_table.bin rust_gb_table.bin"; do
  set -- $pair
  if cmp -s "$WORK/$1" "$WORK/$2"; then
    echo "OK    $1 == $2 ($(wc -c < "$WORK/$1" | tr -d ' ') bytes)"
  else
    echo "DIFF  $1 != $2"
    cmp -l "$WORK/$1" "$WORK/$2" | head -10
    rc=1
  fi
done
echo "WORK=$WORK"
exit $rc
