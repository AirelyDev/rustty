#!/usr/bin/env bash
# P3-W9 T6 step 1 — print the vaxis 0.5.1 -> 0.6.0 diff for the named src files.
D=/tmp/w9deps
O=$D/vaxis-7dbb9fd3122e4ffad262dd7c151d80d863b68558/libvaxis-7dbb9fd3122e4ffad262dd7c151d80d863b68558/src
N=$D/vaxis-1dbbe575dff4586fe51e3217aa5c3fecdcbb6089/libvaxis-1dbbe575dff4586fe51e3217aa5c3fecdcbb6089/src
for f in "$@"; do
  echo "########## $f"
  diff -u "$O/$f" "$N/$f" | tail -n +3
done
