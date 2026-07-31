# Task 3 implementation report — fail-closed coverage and review verifier

## Status

Implemented and verified. The clean real-Git fixture completes with exit 0;
invalid mutations fail with exit 1; incomplete mutations fail with exit 2. The
real empty ledger is `INCOMPLETE` with exit 2 and cannot become green from an
authored subset.

No Rustty product file or Ghostty file was changed. Nothing was staged or
committed.

## Strict TDD evidence

### Observed red

The complete clean-fixture and mutation suite was written before the verifier
module existed. The required command failed at import with:

```text
ModuleNotFoundError: No module named 'verify_ledger'
Ran 1 test in 0.000s
FAILED (errors=1)
```

This was the expected missing-production-module failure. After the initial
implementation reached green, a missing-input-directory regression test was
added. It failed because `E_INPUT` was absent and a missing ledger directory
was being interpreted through downstream finding/review/coverage diagnostics.
The minimal directory-input guard made that regression green.

### Final green

Fresh required command:

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_verify_ledger.py' -v
```

Result:

```text
Ran 25 tests in 14.307s
OK
```

The suite uses two temporary real Git repositories and committed blob bytes;
it does not mock Git trees. It also exercises CLI exits 0, 1, and 2 and parses
every emitted JSON diagnostic/summary object.

## Mutation results

Each named mutation was applied independently to the clean fixture. The clean
fixture itself exited 0 with all 7 text lines covered, final, and independently
upheld.

| Mutation | Required diagnostic | Observed exit |
| --- | --- | ---: |
| Empty universe | `E_UNIVERSE_EMPTY` | 1 |
| Deleted universe file record | `E_SOURCE_FILESET` | 1 |
| Duplicate universe record | `E_UNIVERSE_DUPLICATE` | 1 |
| Current repository revision drift | `E_SOURCE_REVISION` | 1 |
| Stale universe file hash | `E_SOURCE_FILESET` | 1 |
| Uncovered first line | `E_UNCOVERED` | 2 |
| Uncovered middle line | `E_UNCOVERED` | 2 |
| Uncovered last line | `E_UNCOVERED` | 2 |
| Overlapping primary range | `E_OVERLAP` | 1 |
| Out-of-bounds primary range | `E_BOUNDS` | 1 |
| Stale primary range hash | `E_STALE_HASH` | 1 |
| Absent counterpart | `E_COUNTERPART` | 1 |
| Malformed counterpart | `E_COUNTERPART` | 1 |
| Mismatched counterpart hash | `E_COUNTERPART` | 1 |
| Pending disposition | `E_PENDING` | 2 |
| High-risk equivalence without control evidence | `E_HIGH_RISK_PROOF` | 1 |
| Intentional divergence without authority evidence | `E_AUTHORITY_EVIDENCE` | 1 |
| Missing latest review | `E_REVIEW_MISSING` | 2 |
| Latest review needs more evidence | `E_REVIEW_MISSING` | 2 |
| Latest self-review | `E_SELF_REVIEW` | 1 |
| Broken/noncontiguous review chain | `E_REVIEW_CHAIN` | 1 |
| Review omits counterpart source | `E_REVIEW_SOURCES` | 1 |
| Exclusion lacks file evidence | `E_EXCLUSION_EVIDENCE` | 1 |
| Gap without finding | `E_FINDING_LINK` | 1 |
| Finding names a non-gap | `E_FINDING_LINK` | 1 |
| Finding names an unknown range | `E_FINDING_LINK` | 1 |
| Duplicate primary range ID | `E_RANGE_DUPLICATE` | 1 |
| Non-gap record cites a finding | `E_FINDING_LINK` | 1 |
| Summary-tampering text in ignored README/nested files | no diagnostic; derived totals remain 7 | 0 |

Additional stable-code tests cover `E_UNIVERSE_SCHEMA`, `E_LEDGER_SCHEMA`,
`E_REVIEW_SCHEMA`, `E_REVIEW_UNKNOWN`, duplicate review revisions through
`E_REVIEW_CHAIN`, and missing input paths through `E_INPUT`.

## Real incomplete run

Executed the exact required command against the independently approved Task 2
round-2 universe:

```text
python3 .port/codex/line-audit/scripts/verify_ledger.py \
  --universe .port/codex/line-audit/universe.jsonl \
  --ledger .port/codex/line-audit/ledger \
  --reviews .port/codex/line-audit/reviews \
  --findings .port/codex/line-audit/findings \
  --rustty /Users/gael/projects/rustty \
  --ghostty /Users/gael/projects/ghostty \
  --allow-incomplete
```

Observed result: exit 2, `INCOMPLETE`. Runtime was `real 151.61s`,
`user 53.91s`, `sys 85.69s`.

The CLI emitted 101 JSON objects: exactly 100 individual `E_UNCOVERED`
diagnostics plus the final summary. The summary retains all 6,925 uncovered
file intervals rather than the output cap.

| Repository | Files | Text lines | Covered | Uncovered | Pending | Primary final | Secondary upheld | Invalid diagnostics | Incomplete diagnostics |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ghostty | 5,781 | 515,483 | 0 | 515,483 | 0 | 0 | 0 | 0 | 1,761 |
| rustty | 5,369 | 2,588,106 | 0 | 2,588,106 | 0 | 0 | 0 | 0 | 5,164 |
| total | 11,150 | 3,103,589 | 0 | 3,103,589 | 0 | 0 | 0 | 0 | 6,925 |

`--allow-incomplete` did not weaken the exit status.

### Fail-closed drift observation

Before Task 2 round 2, Rustty `HEAD` advanced while the universe remained at
its prior pin. The exact real command correctly exited 1 with
`E_SOURCE_REVISION` and `E_SOURCE_FILESET`; it did not misclassify stale source
binding as ordinary incompleteness. That run still derived the prior universe's
3,082,944 uncovered text lines. After the universe was regenerated and
independently approved, the exact command produced the exit-2 result above.

## Self-review

- Universe records are schema-validated, nonempty, unique, and rebound to both
  supplied full `HEAD^{commit}` revisions and complete Task 2 filesets.
- Primary and counterpart hashes are recomputed from committed blob bytes with
  Task 1's physical-line splitter and range hasher; working-tree bytes are not
  evidence.
- Coverage, pending, final, and upheld counts use interval unions. Overlaps are
  invalid, while every exact uncovered interval remains represented in the
  summary even when CLI diagnostics are capped.
- High-risk control evidence, divergence authority, cross-repository semantic
  counterparts, review chains, reviewer independence, exact inspected source
  sets, and bidirectional finding links all fail closed.
- Only direct sorted `*.jsonl` ledger/review files and promoted direct `*.md`
  finding files count. Placeholder READMEs and nested files cannot create
  records or alter derived totals.
- The verifier uses only the Python 3 standard library and existing Task 1/2
  modules.

## Files created

- `scripts/verify_ledger.py`
- `tests/test_verify_ledger.py`
- `ledger/README.md`
- `reviews/README.md`
- `findings/README.md`
- `sdd/task-3-report.md`

## Repository-boundary confirmation

Final pins used by the successful incomplete run:

```text
rustty   2bd45ccc79ae4ab6bdcb8cb95d5a99d0d8d44816
ghostty  6ad1fe7d8cbda36c77b337a96c9bea8a77883699
universe d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3
```

All Task 3 writes are below `.port/codex/line-audit/`. No Rustty product file
or Ghostty file was modified, and no staging or commit was performed.

## Concerns

The complete committed-tree rebuild is intentionally expensive because Task 2
retrieves every committed blob independently; the real verification took
151.61 seconds. Correctness is unaffected, and the verifier avoids a redundant
second blob traversal when the ledger has no primary/counterpart ranges.

## Fix round 1 — malformed inputs, exact diagnostic partition, and stale metadata

### Review findings addressed

Both Important findings in `sdd/task-3-review.md` are addressed:

1. Parsed JSON objects enter semantic processing only after their canonical
   schema validator completes without diagnostics. Validator execution uses a
   deep copied shape that substitutes only unhashable enum values, allowing the
   canonical validator to preserve every independent diagnostic without ever
   trusting or admitting the original malformed object.
2. Record-level schema diagnostics carry a valid repository whenever the
   record identifies one; review diagnostics resolve the repository through a
   valid `range_id`. The summary exposes an explicit `unattributed` bucket, and
   for both diagnostic classes `ghostty + rustty + unattributed == total`.

An additional adversarial probe discovered that a schema-valid stale universe
`line_count` could admit coordinates beyond the actual committed blob. Both
committed-range hash sites now check the actual physical-line array before
calling Task 1's range hasher. Primary mismatches emit `E_STALE_HASH` and
counterpart mismatches emit `E_COUNTERPART`, always followed by an `INVALID`
summary.

### Regression RED evidence

The six-stage malformed-type CLI matrix was added first. The focused run
executed 27 tests and failed six assertions: universe, primary, counterpart,
review, and checked-source stages each raised `TypeError` with empty stdout;
the partition test showed ledger schema diagnostics had no repository.

A separate preservation regression combined a missing review field with an
unhashable checked-source enum. It failed because the first safety boundary
returned only a generic validator failure rather than both canonical schema
diagnostics.

The stale-metadata regression then changed the fixture universe's Rustty text
line count from 3 to 4 and extended a primary to line 4. It failed with the
reviewed `ValueError: end exceeds line count`, empty stdout, and no summary.
The paired counterpart mutation exercised the analogous hash site.

### Restored GREEN evidence

Fresh required focused command:

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_verify_ledger.py' -v
```

Result:

```text
Ran 29 tests in 21.124s
OK
```

Fresh complete line-audit suite:

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_*.py' -v
Ran 62 tests in 23.392s
OK
```

The new tests prove:

- malformed-but-valid JSON at universe, primary, counterpart, review,
  checked-source, and finding-header stages exits 1 with parseable `E_*`
  objects and a final `INVALID` summary, never a traceback;
- independent schema diagnostics survive an unhashable enum in the same
  record;
- attributable primary/review schema errors appear in the correct repository;
- repository counts plus the explicit unattributed bucket exactly equal total
  invalid and incomplete diagnostics;
- stale universe coordinates beyond actual primary/counterpart blob lines emit
  `E_STALE_HASH`/`E_COUNTERPART` without exception.

### Real incomplete rerun

Pins and universe hash were unchanged, so the exact required real command was
rerun. It exited 2 with `INCOMPLETE` after `real 157.38s` (`user 56.39s`,
`sys 89.60s`). It emitted exactly 100 `E_UNCOVERED` diagnostics plus one final
summary.

| Repository | Files | Text lines | Covered | Uncovered | Invalid | Incomplete |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ghostty | 5,781 | 515,483 | 0 | 515,483 | 0 | 1,761 |
| rustty | 5,369 | 2,588,106 | 0 | 2,588,106 | 0 | 5,164 |
| unattributed | — | — | — | — | 0 | 0 |
| total | 11,150 | 3,103,589 | 0 | 3,103,589 | 0 | 6,925 |

Pending, primary-final, and secondary-upheld totals remained zero. Final pins
and universe hash remained:

```text
rustty   2bd45ccc79ae4ab6bdcb8cb95d5a99d0d8d44816
ghostty  6ad1fe7d8cbda36c77b337a96c9bea8a77883699
universe d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3
```

No product/Ghostty file was changed, and nothing was staged or committed.

## Fix round 2 — canonical risk tags and rejected-primary attribution

### Remaining Important edges addressed

The two findings in `sdd/task-3-r1-review.md` are addressed:

1. Task 1's canonical `validate_primary` now inserts only valid string risk
   tags into its duplicate-detection set. Every JSON list/object/scalar tag is
   diagnosed without raising, and independent missing/nonempty field
   diagnostics remain present.
2. The verifier builds a separate attribution-only index from every parsed
   primary with an individually valid nonempty string `range_id` and valid
   repository. Schema-invalid primaries remain excluded from all semantic,
   coverage, finding, and review structures. Reviews use the identity-only
   index solely to attribute diagnostics. IDs observed in both repositories are
   omitted as ambiguous and remain explicitly unattributed.

### Regression RED and GREEN evidence

The canonical model regression combined a missing `contract` with
`risk_tags=[{}]` and failed before the fix with:

```text
TypeError: cannot use 'dict' as a set element (unhashable type: 'dict')
Ran 1 test in 0.000s — FAILED (errors=1)
```

The attribution regression removed `contract` from `g-first`; its valid review
still emitted `E_REVIEW_UNKNOWN` without `repo`, so the focused targeted test
failed 1/1. A conflicting parsed Ghostty identity for the valid Rustty
`r-first` ID was added as the ambiguity control.

Fresh verification after the fixes:

```text
Task 1 model suite:       Ran 25 tests in 0.001s  — OK
Focused verifier suite:  Ran 29 tests in 19.529s — OK
Complete line-audit:     Ran 63 tests in 21.071s — OK
```

The model matrix includes adjacent `[]`, `{}`, integer, null, boolean, valid,
and duplicate string risk tags. The verifier test proves the valid review of a
schema-rejected Ghostty primary is charged to Ghostty, while a range ID parsed
for both repositories leaves review schema diagnostics unattributed. Both
cases reconcile repository plus unattributed counts exactly to the total.

### Real empty-ledger snapshot applicability

No second 157-second real traversal was run by controller direction. The code
changes execute only while validating or indexing parsed primary records; the
real ledger directory contains no JSONL records, so its execution path is
unchanged. The previously completed exact fix-round-1 snapshot remains the
applicable control: exit 2, `INCOMPLETE`, 11,150 files, 3,103,589 text and
uncovered lines, 0 covered/invalid/unattributed diagnostics, 6,925 incomplete
intervals, and 100 printed uncovered diagnostics plus the final summary.

Pins and universe hash were rechecked after the complete suite and remained:

```text
rustty   2bd45ccc79ae4ab6bdcb8cb95d5a99d0d8d44816
ghostty  6ad1fe7d8cbda36c77b337a96c9bea8a77883699
universe d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3
```

No product/Ghostty file was changed, and nothing was staged or committed.

## Fix round 3 — total enum validation and occurrence-aware identities

### Remaining Important edges addressed

The two findings in `sdd/task-3-r2-review.md` are addressed:

1. Task 1's canonical enum validator now checks for a string before set
   membership. Every subsequent scope/disposition dictionary or set lookup is
   likewise type-guarded. Direct canonical validation therefore returns
   diagnostics, rather than raising, for decoded JSON list/object values at
   every enum site: file repository, content kind, scope class, primary
   disposition, counterpart repository, review verdict, and checked-source
   repository. An independently missing required field remains diagnosed in
   every case.
2. The verifier's attribution index now retains every parsed occurrence with
   an individually valid nonempty string `range_id` and valid repository.
   Attribution is admitted only for exactly one occurrence. Every repeated ID
   emits exactly one stable `E_RANGE_DUPLICATE`, whether the pair is
   valid+rejected or rejected+rejected and whether it is same-repository or
   cross-repository. Rejected primaries remain absent from coverage, reviews,
   counterparts, and findings.

### Regression RED and GREEN evidence

The direct canonical model matrix initially produced 20 subtest errors from
unhashable list/object membership:

```text
Ran 1 test — FAILED (errors=20)
```

The occurrence matrix initially failed all four subtests because
`E_RANGE_DUPLICATE` was absent for parsed identities involving rejected
records:

```text
Ran 1 test — FAILED (failures=4)
```

Fresh verification after the fixes:

```text
Task 1 model suite:       Ran 26 tests in 0.001s  — OK
Focused verifier suite:  Ran 30 tests in 19.457s — OK
Complete line-audit:     Ran 65 tests in 22.464s — OK
Python syntax compile:   OK
```

The occurrence matrix covers valid+rejected and rejected+rejected pairs in
both same- and cross-repository forms. Each case asserts one duplicate
diagnostic, the expected attribution or ambiguity, exact diagnostic
partitioning, and unchanged covered/final semantic totals for the accepted
records only.

### Real empty-ledger snapshot applicability

No additional 157-second real traversal was run by controller direction. This
round changes only canonical record validation and parsed-primary identity
handling; the real ledger contains no JSONL records, so the empty-ledger path
is unchanged. The saved exact fix-round-1 output was reparsed successfully:
101 JSON objects, 100 `E_UNCOVERED` diagnostics and one `INCOMPLETE` summary;
11,150 files; 3,103,589 text and uncovered lines; 0 covered and invalid; 6,925
incomplete intervals; and 0 unattributed invalid/incomplete diagnostics.

The universe still contains exactly the audit pins below, both pinned commits
remain available, and its SHA-256 is unchanged:

```text
rustty   2bd45ccc79ae4ab6bdcb8cb95d5a99d0d8d44816
ghostty  6ad1fe7d8cbda36c77b337a96c9bea8a77883699
universe d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3
```

This round changed only Task 3 line-audit files. Ghostty remained clean, and
nothing was staged or committed by this task.
