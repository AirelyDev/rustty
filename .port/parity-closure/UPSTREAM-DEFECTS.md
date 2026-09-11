# Upstream defects — Ghostty bugs found while porting

Defects **in the Ghostty reference itself** (pin `20abdb50a`), recorded so fixes can be
proposed upstream. Governed by `.port/PORT_RULES.md` **R13**: a defect reachable from real
input is never ported; the port handles it safely and the defect is recorded here.

This is not the parity ledger. `LEDGER.tsv` tracks where the **port** diverges from the
reference; this file tracks where the **reference** is wrong. A row here usually has a matching
parity row, cross-referenced both ways.

## Status of this file

Created 2026-09-11 when the user ruled that reference crashes are not ported. **The Candidates
section below is unverified.** Each candidate was assembled from this plan's progress notes and
must be re-verified against the pin, and triaged, before it becomes an entry. A retroactive sweep
of past work — code, findings, adjudications, and session logs — is in progress to find every
instance this plan and earlier phases ported faithfully.

## Entry format

```
### UD<n> — <one-line title>
- kind:            crash | ub | leak | deadlock | latent | test-weakness
- severity:        high | medium | low   (reachability-weighted, see below)
- reference:       <path>:<lines> at 20abdb50a
- defect:          what the reference does wrong
- ReleaseSafe:     consequence in a safety-checked build
- ReleaseFast:     consequence in the shipped build
- reachability:    who or what can trigger it (peer process / file / VT stream / config / kernel / none)
- port handling:   what rustty does instead, with the file:line of the PORT NOTE
- parity row(s):   LEDGER ids and/or NEW-FINDINGS ids
- test:            the test proving the port's safe handling, and its negative control
- proposed fix:    a sentence or two suitable for an upstream issue or PR
- upstream status: not reported | reported <link> | fixed upstream <commit>
```

**Severity** weighs reachability first: a crash any same-user process can trigger outranks a
crash that needs a corrupted config file, which outranks one that needs kernel misbehaviour.
`latent` and `test-weakness` entries are `low` unless they hide a live defect.

## Entries

*(none yet verified)*

## Candidates pending verification

Assembled from this plan's progress notes on 2026-09-11. **Not verified** — do not cite as fact.

| # | reference | suspected defect | kind | port today | source |
|---|---|---|---|---|---|
| C1 | `src/apprt/gtk/class/global_shortcuts.zig` `responded`, subscription at `:481-486` | `vardict.?` on a malformed `(u@a{sv})` body; `Response` subscribed with a null sender, so any session-bus peer can trigger it | crash | being fixed safely by task 20 (S51-F7) | task 20 |
| C2 | `src/terminal/tmux/Viewer.zig:1160-1161` | `@intCast` of tmux-supplied layout width/height traps on out-of-range values | crash | **ported as a panic** (task 15, S209-F1) — must be re-addressed | task 15 |
| C3 | `src/renderer/Thread.zig:698-723` `cursorCancelCallback` | `unreachable` on cancel errors other than Canceled/NotFound; reachability depends on what the event loop can return | crash? | **ported as `unreachable!()`** (task 14d-1, S114-F2) — reachability must be decided | task 14d-1 |
| C4 | `src/termio/Exec.zig:1045`; `src/apprt/gtk/pre_exec.zig:39,58` | `log.err` between fork and exec: the log function allocates and locks, so a child forked from a multi-threaded parent can deadlock silently | deadlock | handled safely (raw `write`, tasks 14d-3 and 14e-1) | tasks 14d-3, 14e-1 |
| C5 | `src/font/DeferredFace.zig:154-157, 186-197` | zero-copy `cstringPtr` path returns into a +1 CFString that is never released | leak | rows S85-F6 / S90-F3 left OPEN | task 16 |
| C6 | `src/apprt/gtk/noop.zig` `updateConfigEvent` | references `ApprtWindow.DerivedConfig`, which does not exist; compiles only because nothing calls it | latent | N32 | task 18 |
| C7 | `src/renderer/Thread.zig:672` `cursorTimerCallback` | log message says "render callback" — copy-pasted from the render callback | latent | kept verbatim (R1) | task 14d-1 review |
| C8 | `src/renderer/OpenGL.zig:316-327` `present` | `BlitFramebuffer` unchecked; its GL error surfaces later inside the SRGB re-enable, misattributed | latent | faithful; restored log is the only report | task 14c-3 |
| C9 | `src/font/shaper/coretext.zig`, `harfbuzz.zig` `debugPositions` | dead private functions, only callers commented out | latent | N22 dead-site ratchet | task 14e-2 |
| C10 | `src/terminal/PageList.zig` reset test (~`:18963`) | the `!nodeIsValid` assertion is defended by two guards, so breaking either alone stays green | test-weakness | ported verbatim (S131-F1) | task 17 |
| C11 | `src/font/face/coretext.zig:1023-1036`; `src/terminal/StringMap` tests | assertions cannot distinguish the fixture under test | test-weakness | ported verbatim (S90-F5, S140-F1) | task 17 |
| C12 | `src/lib/enum.zig:126` `checkGhosttyHEnum` | iterates only the enum's fields, so a header enumerator the enum lacks is never caught | test-weakness | faithful (N36) | task 19 review |
| C13 | `src/apprt/gtk/media.zig` `bellMediaFile` | a GstPlay teardown race required `mem::forget` in the port's test; whether production code races is unknown | crash? | test-only workaround (S48-F1) | task 19 |
