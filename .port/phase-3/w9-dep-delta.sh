#!/usr/bin/env bash
# P3-W9 T6 — per-file numstat of a ported Zig library between the pin and
# oracle-head tarball, restricted to the files rustty actually ports.
# Usage: w9-dep-delta.sh <old-src-dir> <new-src-dir>
set -uo pipefail
OLD="$1"; NEW="$2"
diff -ruN "$OLD" "$NEW" > /tmp/w9-dep.diff
echo "TOTAL diff lines: $(wc -l < /tmp/w9-dep.diff)"
echo "--- per-file +/- ---"
awk '
  /^--- / { f=$2 }
  /^\+\+\+ / { g=$2 }
  /^\+/ && !/^\+\+\+/ { add[g]++ }
  /^-/  && !/^---/    { del[g]++ }
  END { for (k in add) printf "%6d +%-6d -%-6d %s\n", add[k]+del[k], add[k], del[k], k
        for (k in del) if (!(k in add)) printf "%6d +%-6d -%-6d %s\n", del[k], 0, del[k], k }
' /tmp/w9-dep.diff | sort -rn
