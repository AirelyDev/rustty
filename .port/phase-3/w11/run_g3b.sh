#!/bin/bash
# G3b-REST helper: run check_comment_coverage.py over the non-vt crates in scope.
# Usage: run_g3b.sh [crate ...]   (default: the ten P3-W11 lane crates)
cd "$(dirname "$0")/../../.." || exit 2
crates=("$@")
if [ ${#crates[@]} -eq 0 ]; then
  crates=(global unicode termio os cli vt-c bench lib config simd)
fi
for c in "${crates[@]}"; do
  python3 .port/tools/gates/check_comment_coverage.py --crate "$c" 2>&1 | grep -E '^G3b|MISSING'
done
