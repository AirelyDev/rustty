# Task 1 fix round 1 — scoped re-review

## Result

Task 1 is now approved.

- Original findings addressed: 4
- Original findings open: 0
- New Critical/Important breakage introduced by this round: none

## Snapshot and verification

The current hashes match every fix-round hash in the review package:

```text
0b43466a2943e49442cd22f8049715de5d6c9e6f68854015541fb60e68094ac2  scripts/model.py
b509e190272bbfe6b4c100d3c904cbd04cc2c6d3408b7b780004125ef1710efe  tests/test_model.py
b309c1cd39888de647e387f8b31ef09b70b5c202c7fcfb2bc0d0c7bb94b014ef  SCHEMA.md
26d626643af476c2f32f5b20662037f298ad36b4b26995e073ec1e2ecdfd8684  sdd/task-1-brief.md
```

The required unittest command passed independently with 24 tests.  I also
checked the formerly failing bare-CR inputs and the impossible zero-byte text
record directly; they now produce the documented line lists and diagnostic.

## Original findings

### 1. Critical — physical-line splitting: ADDRESSED

`scripts/model.py:51-59` now explicitly splits only on `LF`; a bare `CR`
remains in raw content.  This matches the amended binding brief at
`sdd/task-1-brief.md:28-30` and schema at `SCHEMA.md:9-11`.
`tests/test_model.py:111-115` covers both a bare CR before an LF and in the
unterminated tail, in addition to the existing mixed CRLF/LF test.

### 2. Critical — repository revision identity: ADDRESSED

`scripts/model.py:37` and `113-116` accept 40- or 64-character lowercase Git
object IDs while retaining SHA-256 validation for content and range hashes.
The same distinction is documented in `SCHEMA.md:21-23` and the binding brief
at `sdd/task-1-brief.md:69-70`.  The test fixtures use the actual 40-character
Rustty Git ID (`tests/test_model.py:19`), and `tests/test_model.py:165-168`
also covers valid 64-character and invalid-length revisions.  The ledger can
therefore express the pinned checkout without inventing a content-hash
surrogate.

### 3. Important — impossible zero-byte text inventory: ADDRESSED

`scripts/model.py:170-174` requires both positive text byte length and line
count, exactly as stated in `SCHEMA.md:43-44` and
`sdd/task-1-brief.md:73-74`.  The regression at
`tests/test_model.py:170-172` rejects the prior zero-byte/one-line record.

### 4. Important — nonsemantic file-evidence chain: ADDRESSED

`validate_primary_with_file` at `scripts/model.py:259-278` composes primary
and file diagnostics, requires a file record, validates it, and checks exact
`(repo, revision, path)`, scope class, and nonempty classification reason and
evidence.  The new required interface and its use are explicit in the brief
(`sdd/task-1-brief.md:36`, `90-93`) and schema (`SCHEMA.md:87-93`).
`tests/test_model.py:203-241` covers matching acceptance plus absent,
mismatched, scope-mismatched, and empty-evidence failures.

## New breakage

None at Critical or Important severity found in the scoped fix-round changes.
