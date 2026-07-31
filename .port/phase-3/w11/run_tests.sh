#!/bin/bash
# G3b-REST helper: run `cargo test` for each crate this lane touched.
cd "$(dirname "$0")/../../.." || exit 2
for c in "$@"; do
  echo "=== $c"
  cargo test -p "$c" --no-fail-fast 2>&1 | grep -E '^test result|^error' | head -12
done
