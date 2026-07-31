# Task 2 review package

## Inputs

- Requirements:
  `/Users/gael/projects/rustty/.port/codex/line-audit/sdd/task-2-brief.md`
- Implementer report:
  `/Users/gael/projects/rustty/.port/codex/line-audit/sdd/task-2-report.md`
- Prior model/schema:
  `/Users/gael/projects/rustty/.port/codex/line-audit/scripts/model.py`
  and `/Users/gael/projects/rustty/.port/codex/line-audit/SCHEMA.md`
- Implementation:
  `/Users/gael/projects/rustty/.port/codex/line-audit/scripts/build_universe.py`
- Tests:
  `/Users/gael/projects/rustty/.port/codex/line-audit/tests/test_build_universe.py`
- Generated universe:
  `/Users/gael/projects/rustty/.port/codex/line-audit/universe.jsonl`

## Snapshot hashes

```text
665cea4d4c56cfb2e638791b5cab5dbb535b37edefd2bd2912789e8b8fdb6309  scripts/build_universe.py
62672e96753fddea1baa8aa2c7a20f1bf8f439754402df9f0186110d42459a69  tests/test_build_universe.py
81598efefa28edc7c2bc2b264d45f538add9f24f7a1b7b1668033f86ef718740  universe.jsonl
```

The universe contains 11,071 JSONL records and is 4.3 MiB. Review the complete
implementation and tests; independently interrogate the universe rather than
loading it into the review report.

## Review contract

Write the full review to
`/Users/gael/projects/rustty/.port/codex/line-audit/sdd/task-2-review.md`.

Give both `Spec compliance: PASS|FAIL` and
`Task quality: APPROVED|CHANGES_REQUIRED`. Findings require severity, exact
location, proof, consequence for exhaustive coverage, and correction. Verify
that the generated universe truly binds both committed trees, is non-vacuous,
and cannot silently omit or substitute a tracked path.

## Fix round 1 snapshot

The implementation source is unchanged. A mutation-proven atomic failure test
was added and the universe was regenerated after Rustty advanced to
`fa02b36cc21eadcd8eaa8f68ae0bf1ec759ddc83`:

```text
665cea4d4c56cfb2e638791b5cab5dbb535b37edefd2bd2912789e8b8fdb6309  scripts/build_universe.py
c4d9fec725c7c890403a1e20cb7b6bc975b6874b1ce6e2dbb40d637bbc37f79a  tests/test_build_universe.py
19d3d7270015d39b08a2b9c3e2fe0c0af3325526f3453effb55ecfc8b1599782  universe.jsonl
```

Write the scoped re-review to
`/Users/gael/projects/rustty/.port/codex/line-audit/sdd/task-2-r1-review.md`.

## Repin validation round 2 snapshot

Rustty advanced again, so the earlier generated artifact is no longer
authoritative. The implementation and tests remain unchanged. Review the new
universe against these exact committed revisions:

```text
rustty   2bd45ccc79ae4ab6bdcb8cb95d5a99d0d8d44816
ghostty  6ad1fe7d8cbda36c77b337a96c9bea8a77883699
```

Snapshot hashes and expected totals:

```text
665cea4d4c56cfb2e638791b5cab5dbb535b37edefd2bd2912789e8b8fdb6309  scripts/build_universe.py
c4d9fec725c7c890403a1e20cb7b6bc975b6874b1ce6e2dbb40d637bbc37f79a  tests/test_build_universe.py
cb6ab83addcfe20c8153c9d008ee1c547c3a314695143800c60db50a18809fef  sdd/task-2-report.md
d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3  universe.jsonl
```

- 11,150 files total: 5,781 Ghostty and 5,369 Rustty.
- 6,925 text, 4,182 binary, and 43 empty records.
- 3,103,589 physical text lines total.

Independently enumerate both committed trees and compare every emitted record,
including path, blob bytes/hash, content classification, byte length, and line
count. Confirm there are zero missing, extra, or mismatched records and that
the pins still equal current `HEAD` at review time. Write the scoped re-review
to
`/Users/gael/projects/rustty/.port/codex/line-audit/sdd/task-2-r2-review.md`.
