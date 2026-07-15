#!/bin/sh
# c-vt example parity gate: diff a rustty c-vt example's stdout against its
# ghostty twin (`zig build run`). Usage: .port/tools/cvt-parity.sh <example-name>
set -e
name="$1"; [ -n "$name" ] || { echo "usage: cvt-parity.sh <c-vt-example>"; exit 2; }
root=$(cd "$(dirname "$0")/../.." && pwd)
gdir="/Users/gael/projects/ghostty/example/$name"
rdir="$root/example/$name"
T="${TMPDIR:-/tmp}"
[ -d "$gdir" ] || { echo "BLOCKED: ghostty example $gdir absent"; exit 3; }
[ -d "$rdir" ] || { echo "$name: rustty port missing ($rdir)"; exit 1; }
( cd "$gdir" && zig build run 2>/dev/null ) > "$T/g-$name.txt" || { echo "$name: ghostty ref build/run failed"; exit 1; }
sh "$rdir/build.sh" > "$T/r-$name.txt" 2>/dev/null || { echo "$name: rustty build/run failed"; exit 1; }
if diff "$T/g-$name.txt" "$T/r-$name.txt" >/dev/null; then
  echo "$name: BYTE-IDENTICAL ✓ ($(wc -l < "$T/r-$name.txt" | tr -d ' ') lines)"
else
  echo "$name: DIFFERS"; diff "$T/g-$name.txt" "$T/r-$name.txt"; exit 1
fi
