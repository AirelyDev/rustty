#!/usr/bin/env bash
# citriage-bench-miri-probe.sh — per-test Miri probe for rustty-bench.
#
# Miri aborts the WHOLE test binary at the first foreign call, so a single lane
# run only ever names one offender. This runs each test in its own Miri process
# and classifies it, so the exception set is MEASURED rather than guessed.
# macOS has no `timeout`; `perl -e 'alarm N; exec @ARGV'` is the stand-in.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.." || exit 1
CRATE="${1:-rustty-bench}"
LIST="${2:-/tmp/bench_tests.txt}"
while read -r t; do
  [ -n "$t" ] || continue
  out=$(perl -e 'alarm 600; exec @ARGV' -- cargo +nightly miri test -p "$CRATE" --lib -- --exact "$t" 2>&1)
  if printf '%s' "$out" | grep -q "can't call foreign function"; then
    fn=$(printf '%s' "$out" | grep -o "foreign function \`[A-Za-z_0-9]*\`" | head -1)
    echo "FFI    $t  $fn"
  elif printf '%s' "$out" | grep -q "test result: ok"; then
    echo "ok     $t"
  else
    echo "OTHER  $t"
    printf '%s\n' "$out" | tail -5 | sed 's/^/         /'
  fi
done < "$LIST"
