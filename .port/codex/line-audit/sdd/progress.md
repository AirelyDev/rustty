# SDD ledger — plan: .port/codex/LINE_AUDIT_PLAN.md

Task 1: in progress — fresh implementer dispatched for schema/model TDD.
Task 1: fix round 1/5 — 4 open (bare-CR splitting; Git revision width; zero-byte text; cross-record exclusion evidence).
Task 1: fix round 1/5 complete — 4 addressed, 0 open; scoped re-review approved.
Task 1: complete (audit-only uncommitted files; review clean after round 1; 24 tests).

Task 2: in progress — fresh implementer dispatched for pinned universe TDD.
Task 2: fix round 1/5 — 1 review finding open (atomic post-temp failure test); Rustty baseline drift to fa02b36 invalidated the generated universe and requires regeneration.
Task 2: fix round 1/5 complete — T2-R1 addressed, 0 open; repinned universe independently matched both committed trees.
Task 2: complete (audit-only uncommitted files; review clean after round 1; 9 tests; 11,071 files / 3,082,944 physical text lines).

Task 3: in progress — fresh implementer dispatched for fail-closed verifier TDD.
Task 3: real-run checkpoint — correctly rejected second Rustty baseline drift (E_SOURCE_REVISION/E_SOURCE_FILESET); verifier code remains at 25-test green state while Task 2 repins.
Task 2: repin validation round 2 — Rustty advanced to 2bd45ccc; regenerate and independently bind the new committed tree before Task 3 resumes.
Task 2: repin validation round 2 complete — independently approved with 0 missing, 0 extra, and 0 mismatched records (11,150 files / 3,103,589 physical text lines).
Task 3: implementation checkpoint complete — 25 tests green; real empty-ledger control exits 2 with 3,103,589 uncovered, 0 covered, and 0 invalid diagnostics; fresh independent review in progress.
Task 3: fix round 1/5 — 2 Important findings open (malformed JSON types can crash without a summary; attributable repository diagnostics do not reconcile with totals).
Task 3: fix round 1/5 adversarial extension — schema-valid stale line metadata plus an actual-blob out-of-bounds range also reproduced an uncaught hash exception; regression/fix added to the same round before re-review.
Task 3: fix round 1/5 re-review — crash safety and stale actual-byte bounds addressed; 2 Important edges remain (risk-tag validator loses independent diagnostics; reviews of schema-rejected primaries lose otherwise knowable repository attribution).
Task 3: fix round 2/5 — original implementer assigned the 2 remaining bounded corrections with red-first regressions.
Task 3: fix round 2/5 re-review — risk-tag case and cross-repository ambiguity improved, but 2 Important gaps remain (other canonical enum/membership JSON types still raise; duplicate same-repository parsed IDs collapse, remain attributable, and miss E_RANGE_DUPLICATE).
Task 3: fix round 3/5 — original implementer assigned exhaustive canonical enum-type guards and occurrence-aware duplicate identity handling.
