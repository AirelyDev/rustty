# Synthetic fixture — NOT a real ledger. Proves check_phase2_closure.py's FAIL arm.

This file reproduces the exact defect the WS19 Phase 2 closure audit was created by:
items filed into a workstream that has since MERGED (so nobody will ever pick them up),
and items with no owner at all. Both must be REPORTED, with the file:line and the reason.

If this fixture ever stops producing exit 1, the gate has stopped binding — do not
"fix" the fixture, fix the gate.

## Deferred to WS8 upstream sweep — synthetic orphan

**OPEN.** A synthetic item filed to WS8, which is merged and closed.
**Target: WS8.**

## Synthetic item with no owner at all

**OPEN.** No target line anywhere in this entry.
