# Task 3 scoped re-review — fix round 1

Spec compliance: FAIL

Task quality: CHANGES_REQUIRED

## Defect status

### 1. Malformed JSON types caused uncaught exceptions and no final summary — ADDRESSED

The corrected loader now keeps schema-invalid objects out of universe keys,
fileset sorting, primary semantic structures, counterpart resolution, review
groups, and checked-source sets. I independently exercised JSON arrays/objects
in all of these positions through the CLI:

- universe `repo`, `revision`, `path`, `line_count`, and `content_kind`;
- primary `repo`, `path`, `range_id`, `risk_tags`, and `finding_ids`;
- counterpart `repo`, `path`, and `start`;
- review `range_id` and `review_revision`;
- checked-source `repo`, `path`, and `start`;
- finding-header `id` and `range_ids` entries.

All 20 mutations exited 1, emitted parseable `E_*` diagnostics, ended with an
`INVALID` summary, had empty stderr, and reconciled both invalid and incomplete
partitions. No traceback or missing summary was observed. The key safety gates
at `scripts/verify_ledger.py:285-315`, `357-384`, and `552-580` therefore
address the original crash defect.

### 2. Per-repository diagnostic counts omitted attributable failures — OPEN

The arithmetic part is improved: `scripts/verify_ledger.py:686-713` adds an
explicit unattributed bucket, and every probe satisfied
`ghostty + rustty + unattributed == total` for both diagnostic classes. Direct
primary and review schema errors also carry the expected repository when their
records otherwise resolve normally.

However, the repository attribution is still not exact for a review whose
primary was rejected by schema validation. A clean Ghostty primary with only
`contract` removed emits two Ghostty `E_LEDGER_SCHEMA` diagnostics and is then
excluded from `primary_by_id`. Its otherwise valid review consequently emits:

```text
E_REVIEW_UNKNOWN  range_id=g-first  repo=<absent>
```

The resulting invalid partition is Ghostty 2, Rustty 0, unattributed 1, total
3. The third diagnostic is not genuinely unattributed: the parsed primary has
an individually valid `range_id="g-first"` and `repo="ghostty"`. This remains
the same class of incorrect repository-derived count identified in round 0.

Required correction: build a separate attribution-only map from parsed
primaries whose `range_id` and `repo` fields are individually valid, without
admitting those records to any semantic/coverage structure. Use it only for
review diagnostic attribution, and leave conflicting duplicate IDs
unattributed unless their repository is unambiguous. Add a regression asserting
that a review of a schema-rejected Ghostty primary is counted under Ghostty,
not the global bucket.

### 3. Stale universe coordinates could raise during committed-range hashing — ADDRESSED

Both range-hash sites now guard against actual committed-blob line counts before
calling `range_sha256` (`scripts/verify_ledger.py:453-466` and `503-510`). I
independently increased a schema-valid universe `line_count` from three to four
and cited nonexistent actual line four in each direction:

| Mutation | Exit | Required diagnostic | Final summary |
| --- | ---: | --- | --- |
| primary range `4..4` | 1 | `E_STALE_HASH` | `INVALID` |
| counterpart range `4..4` | 1 | `E_COUNTERPART` | `INVALID` |

Both also emitted the expected `E_SOURCE_FILESET`; neither emitted stderr or a
traceback. Adjacent exact-EOF coordinates continued through normal hash
verification. The additional stale-metadata crash is addressed.

## Remaining Important finding

### Important — the safety wrapper still discards independent canonical diagnostics for an adjacent malformed type

- Location: `scripts/verify_ledger.py:68-88`, reached from primary validation
  at `359-374`.
- Counterexample: on a valid primary, remove the required `contract` field and
  set `risk_tags` to `[{}]`. The risk-tag object is valid JSON and should
  independently produce an invalid-risk-tag diagnostic, while the missing
  contract should retain both canonical missing/nonempty diagnostics.
- Observed output: the model validator accumulates the missing-field
  diagnostics, then raises when it attempts to add the dictionary risk tag to
  its internal set. `_safe_validation` catches that exception and replaces the
  complete diagnostic list with only:

  ```text
  E_LEDGER_SCHEMA: schema validator rejected malformed field types:
  TypeError: cannot use 'dict' as a set element (unhashable type: 'dict')
  ```

  Neither `missing required field: contract` nor `contract must be nonempty`
  survives. The record is safely excluded and a final summary is emitted, so
  this is not a crash; it is a fail-closed evidence-loss defect.
- Why the new regression misses it: the preservation test combines a missing
  review field with an unhashable enum in `source_ranges_checked`. Enum fields
  are specially replaced at lines 81-82 before validation, so that case keeps
  both diagnostics. Unhashable risk-tag entries are not covered by that
  substitution and take the broad exception path at lines 72-73.
- Exhaustive-audit consequence: malformed records can suppress other
  independent schema failures, contradicting the canonical validators'
  all-diagnostics contract and the fix-round review requirement that canonical
  diagnostics not be lost. At ledger scale, this makes remediation reports
  incomplete even though the overall status remains invalid.
- Required correction: make canonical validation robust to every JSON value
  the schema permits the parser to deliver, preferably by fixing the risk-tag
  validator to avoid inserting invalid non-string values into its set. If the
  Task 3 defensive-copy boundary remains, it must preserve the canonical
  missing-field/type diagnostics rather than replacing them with one generic
  exception. Add the exact combined regression above and adjacent unhashable
  array/object risk-tag cases.

## Verification evidence

The review package hashes matched the scoped snapshot:

```text
35e16d61e5955a3823ca0987353ad3b544b8785580b9a8057cf2107ae9a3d64d  scripts/verify_ledger.py
7ef6ab1ad560a5c66edd57739b3b9fa5e24b27bdcb365e61c593db96883ece29  tests/test_verify_ledger.py
d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3  universe.jsonl
```

Repository pins remained:

```text
rustty   2bd45ccc79ae4ab6bdcb8cb95d5a99d0d8d44816
ghostty  6ad1fe7d8cbda36c77b337a96c9bea8a77883699
```

Fresh test results:

```text
Focused verifier suite:  Ran 29 tests in 21.141s — OK
Complete audit suite:    Ran 62 tests in 20.939s — OK
py_compile:              OK
```

The implementer/controller real-tree control for this unchanged snapshot
reported exit 2, zero invalid diagnostics, 6,925 incomplete intervals, and all
3,103,589 lines uncovered. Per controller direction, the scoped reviewer did
not duplicate that long control after confirming the exact hashes and pins.

## Decision

CHANGES_REQUIRED. The malformed-input crash and actual-byte bounds defects are
addressed, but canonical diagnostic preservation and exact repository
attribution still have Important gaps. Task 4 must not begin until the combined
counterexamples above pass and receive another scoped independent review.
