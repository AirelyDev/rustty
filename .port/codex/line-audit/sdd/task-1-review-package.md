# Task 1 review package

## Inputs

- Requirements brief:
  `/Users/gael/projects/rustty/.port/codex/line-audit/sdd/task-1-brief.md`
- Implementer report:
  `/Users/gael/projects/rustty/.port/codex/line-audit/sdd/task-1-report.md`
- Implementation:
  `/Users/gael/projects/rustty/.port/codex/line-audit/scripts/model.py`
- Tests:
  `/Users/gael/projects/rustty/.port/codex/line-audit/tests/test_model.py`
- Schema documentation:
  `/Users/gael/projects/rustty/.port/codex/line-audit/SCHEMA.md`

## Immutable snapshot hashes presented for review

```text
5519cb2bf38a9c0e1945c9a11ec51a1f7fbaab4262d2eb060c0f13207aec1022  scripts/model.py
1916043030ca12f9f2a67a55a51928a44bbd089901d747034369aa7c5740957e  tests/test_model.py
1e6926f4917848d8fb22fd51bfd06574a660aaefb3280ba54111638e3973b33f  SCHEMA.md
```

All files are new audit-only artifacts, so there is no product-source base
version. Review the complete file contents rather than a truncated diff.

## Fix round 1 snapshot

The initial review identified four findings. The implementer added regression
tests first, observed red, then changed the model/schema/brief. Re-review the
four findings in `task-1-review.md` against these complete-file hashes:

```text
0b43466a2943e49442cd22f8049715de5d6c9e6f68854015541fb60e68094ac2  scripts/model.py
b509e190272bbfe6b4c100d3c904cbd04cc2c6d3408b7b780004125ef1710efe  tests/test_model.py
b309c1cd39888de647e387f8b31ef09b70b5c202c7fcfb2bc0d0c7bb94b014ef  SCHEMA.md
26d626643af476c2f32f5b20662037f298ad36b4b26995e073ec1e2ecdfd8684  sdd/task-1-brief.md
```

Write the scoped re-review to
`/Users/gael/projects/rustty/.port/codex/line-audit/sdd/task-1-r1-review.md`.

## Review output contract

Write the full review to
`/Users/gael/projects/rustty/.port/codex/line-audit/sdd/task-1-review.md`.

The review must contain two explicit verdicts:

1. `Spec compliance: PASS` or `FAIL`.
2. `Task quality: APPROVED` or `CHANGES_REQUIRED`.

For each finding, include severity (`Critical`, `Important`, or `Minor`), exact
file/line, violated requirement or risk, and a concrete correction. Inspect the
actual source and tests. Do not accept the implementer's prose as proof.
