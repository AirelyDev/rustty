# Task 3 review package

## Inputs

- Requirements: `sdd/task-3-brief.md`
- Schema/model: `SCHEMA.md` and `scripts/model.py`
- Universe builder: `scripts/build_universe.py`
- Independently approved universe: `universe.jsonl`
- Implementation: `scripts/verify_ledger.py`
- Tests: `tests/test_verify_ledger.py`
- Implementer evidence: `sdd/task-3-report.md`
- Placeholder directories: `ledger/`, `reviews/`, and `findings/`

All paths above are below
`/Users/gael/projects/rustty/.port/codex/line-audit/`.

## Snapshot hashes

```text
c0ffc7ffeac3ade89b569ad24db5abbdbba22dffeb9d20487e571d6e0e57fe12  sdd/task-3-brief.md
0b43466a2943e49442cd22f8049715de5d6c9e6f68854015541fb60e68094ac2  scripts/model.py
665cea4d4c56cfb2e638791b5cab5dbb535b37edefd2bd2912789e8b8fdb6309  scripts/build_universe.py
d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3  universe.jsonl
698dfd7dc8372aaba668fbf61e0435073ee62af61d002f7b6a3fffa152e2812a  scripts/verify_ledger.py
b5f16ffa28ca42993979bcd64234047e81395313796cb7d370563465d22855ca  tests/test_verify_ledger.py
ec390e2a7e123e3c052080c20b006015b6b56b042aaadb11d3bf7d9b81a58c86  sdd/task-3-report.md
```

## Independent review contract

Review the full implementation and test suite against every requirement in the
brief. Re-run the focused suite. Independently construct at least one valid
complete fixture and adversarially mutate it beyond the listed tests. In
particular, probe malformed-but-valid-JSON field types, missing keys, corrupted
line counts/bounds, malformed review source references, malformed counterpart
references, duplicate records, missing paths, unreadable/missing inputs, and
repository drift. A bad input must produce machine-readable diagnostics and
exit 1; an uncaught exception, missing final summary, silently accepted bad
record, or incorrect derived count is an Important finding.

Recompute coverage totals independently for fixtures containing adjacent,
gapped, overlapping, pending, final, and upheld ranges. Verify that invalid
records cannot contribute to a false exit 0, that the latest independent
review checks the exact primary and all counterpart ranges, and that promoted
findings link bidirectionally. Confirm the CLI caps only printed uncovered
diagnostics, not totals.

Do not modify implementation or tests. Write the review to
`sdd/task-3-review.md` with:

- `Spec compliance: PASS|FAIL`
- `Task quality: APPROVED|CHANGES_REQUIRED`
- every finding classified Critical, Important, or Minor;
- exact file/line location, a reproducible counterexample, observed output,
  exhaustive-audit consequence, and required correction;
- explicit confirmation of test results and current repository/universe pins.

No Critical or Important finding may remain open before Task 4 begins.

## Fix round 1 snapshot and scoped re-review

The implementer addressed the two Important findings from
`sdd/task-3-review.md` and the controller's additional stale-metadata range-hash
crash. Review this exact snapshot:

```text
b067caae1df732902a569e3167df27aaab6c2d7dae96c7836932e51a654d176d  sdd/task-3-brief.md
d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3  universe.jsonl
35e16d61e5955a3823ca0987353ad3b544b8785580b9a8057cf2107ae9a3d64d  scripts/verify_ledger.py
7ef6ab1ad560a5c66edd57739b3b9fa5e24b27bdcb365e61c593db96883ece29  tests/test_verify_ledger.py
cf341b603833188e0a13e185bb373ea248eda48115804cb654d46f92563f274f  sdd/task-3-report.md
```

Controller verification: 29 focused tests and all 62 line-audit tests pass;
`py_compile` passes. The real control rerun at unchanged pins exited 2 with
3,103,589 uncovered, 0 covered, 0 invalid, 6,925 incomplete, and an exact
zero/zero unattributed diagnostic bucket.

Independently re-run the malformed-type matrix, repository/unattributed
partition probes, and stale-universe-line-count probes for both primary and
counterpart hashing. Check the implementation rather than trusting the new
tests: schema-invalid objects must be diagnosed but excluded from every
semantic structure; schema-valid source metadata drift must not permit actual
blob range hashing to raise; independent canonical diagnostics must not be
lost; all diagnostic partitions must reconcile. Probe adjacent variants for
unhandled types or actual-byte bounds.

Write the scoped result to `sdd/task-3-r1-review.md`, marking each original
Important finding and the additional stale-metadata crash as `ADDRESSED` or
`OPEN`, and give final `Spec compliance: PASS|FAIL` and
`Task quality: APPROVED|CHANGES_REQUIRED`. Do not modify implementation/tests.

## Fix round 2 snapshot and scoped re-review

Round 1's independent review left two bounded edges open. The implementer
changed the canonical model risk-tag validator and added a non-semantic
identity-only attribution index. Review this exact snapshot:

```text
505f8c338d03ea8eaab3249e49cecb4d953a2d99b77659ab222ae6f8d9eaa321  scripts/model.py
0067e1bc6f4c7f28c08ae48813e9bf7c09e4161db0929e31856f132f5f8e08da  tests/test_model.py
ec88b8dfaaaafc68d64a54b85e8974042feb18f6b129508775d63cff82551264  sdd/task-3-brief.md
80094241732f620d0ac90819257a1fb3840b5cf5f5342ce0651648914a4fe044  scripts/verify_ledger.py
e3e10f475f22c49a188c7bf72bfc8986ec1bb015045a3bc5d95b0f92baf61759  tests/test_verify_ledger.py
0fe45cd9e33bb96cc35d40f936db3e97bd8972f6b42fb8d066b1cf9e8f995476  sdd/task-3-report.md
d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3  universe.jsonl
```

Controller verification: all 63 line-audit tests pass and all changed Python
files compile. The model test exercises list/object/scalar/null/bool risk-tag
values while preserving independent diagnostics. The verifier test exercises
review attribution through a schema-rejected Ghostty primary and the control
where the same parsed range ID appears in both repositories and must remain
unattributed.

Independently verify that the canonical validator never raises and preserves
all independent errors; that the attribution-only index cannot admit a rejected
primary into coverage, review acceptance, findings, or counterpart resolution;
that only a unique valid `(range_id, repo)` identity is attributed; and that
duplicate/conflicting identities remain unattributed while the partition
reconciles. Re-run adjacent controls. Confirm the changes do not execute on the
empty real ledger path and therefore do not invalidate the already completed
real control at the same universe/code base.

Write `sdd/task-3-r2-review.md`, marking the two round-1 open edges
`ADDRESSED|OPEN`, and give final `Spec compliance: PASS|FAIL` and
`Task quality: APPROVED|CHANGES_REQUIRED`. Do not modify implementation/tests.
