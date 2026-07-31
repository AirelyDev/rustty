#!/usr/bin/env bash
# P3-W9 T6 step 1 — zf 0.10.3 -> 0.11.0 diff over the files rustty ports
# (`src/zf/{filter,zf,clib}.zig` -> crates/rustty-zf/src/{filter,zf,clib}.rs).
D=/tmp/w9deps
O=$D/zf-3c52637b7e937c5ae61fd679717da3e276765b23/zf-3c52637b7e937c5ae61fd679717da3e276765b23/src/zf
N=$D/zf-c35c421f84895193246db06c40683c1a30e616ef/zf-c35c421f84895193246db06c40683c1a30e616ef/src/zf
for f in "${@:-filter.zig zf.zig clib.zig}"; do
  echo "########## $f"
  diff -u "$O/$f" "$N/$f" | tail -n +3
done
