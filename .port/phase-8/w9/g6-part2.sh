#!/usr/bin/env bash
cd /Users/gael/projects/rustty/.claude/worktrees/p8-w9t2m
date -u +%FT%TZ > logs/g6p2.start
rc=0
for c in rustty-vt rustty-wasm rustty-wuffs rustty-xev rustty-z2d rustty-zf rustty-zigstd; do
  bash .port/tools/gates/check_leaks.sh --crate $c || { echo "CRATE-RC-NONZERO $c"; rc=1; }
done
echo $rc > logs/g6p2.rc
date -u +%FT%TZ > logs/g6p2.end
