# Synthetic fixture — NOT a real ledger. Proves check_phase2_closure.py's PASS arm.

The POSITIVE control for `phase2-closure-orphan.md`. Same shape, same open markers, but
each item names an OPEN workstream. A gate with only a demonstrated FAIL is as untrustworthy
as one with only a demonstrated PASS: without this fixture we could not tell "the gate
rejects everything" from "the gate discriminates".

This file is also what proves the closed-workstream derivation is LIVE rather than
hardcoded: mark WS14 as merged in the ROADMAP and this fixture must FLIP to FAIL.

## Deferred to the wasm/WebGL workstream — synthetic OWNED item

**OPEN.** A synthetic item filed to a workstream that is open and unexecuted.
**Target: WS14.**

## Second synthetic owned item, different open marker

**Un-defer trigger:** the wasm artifact pipeline lands.
**Target: WS14.**
