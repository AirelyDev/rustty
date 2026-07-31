#!/usr/bin/env bash
# P3-W7 T2 Step 1 — the non-UTF-8 environment reproducer.
#
# Runs an arbitrary command with a NON-UTF-8 environment variable set
# (FOO=<0xff>), which is legal on POSIX and which `std::env::vars()`
# documents as a PANIC.
#
# Usage: w7-nonutf8-repro.sh <command> [args...]
set -uo pipefail
export FOO=$'\xff'
export RUSTTY_W7_BAD=$'\xfe\xff'
"$@"
echo "EXIT=$?"
