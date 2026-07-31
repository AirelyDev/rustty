# Task 3 independent review — fail-closed coverage and review verifier

Spec compliance: FAIL

Task quality: CHANGES_REQUIRED

## Findings

### Important — schema-invalid JSON objects continue into unsafe semantic processing and crash the CLI

- Location: `scripts/verify_ledger.py:241-257`, with analogous unsafe uses at
  `307-315`, `335-356`, `391-415`, `476-495`, and `528-534`.
- Root cause: the loader records schema diagnostics but continues processing any
  JSON object merely because it is a `dict`. Fields already proven to have the
  wrong type are then used as set elements, dictionary keys, ordered tuple
  members, numeric bounds, or set-intersection inputs. JSON arrays and objects
  are valid JSON values but are unhashable in these positions.
- Reproducible counterexample: start with the valid complete fixture, replace
  the first universe record's valid string `path` with `[]`, and invoke the CLI.
  This is a JSON-valid malformed field type which `validate_file` correctly
  diagnoses. Instead of emitting `E_UNIVERSE_SCHEMA` and a final `INVALID`
  summary, execution reaches line 252 and raises:

  ```text
  TypeError: cannot use 'tuple' as a set element (unhashable type: 'list')
  ```

  The observed process return code was 1, but stdout was exactly empty: there
  was no machine-readable diagnostic and no final summary.
- Independent breadth probe: the same failure pattern was reproduced with
  universe `repo=[]`, `revision=[]`, and `line_count={}`; primary `path=[]`,
  `repo=[]`, and `risk_tags=[{}]`; review `range_id=[]`,
  `review_revision=[]`, and a `source_ranges_checked` reference containing
  `repo=[]`. The observed exceptions were `TypeError` at the corresponding set,
  dictionary, comparison, or intersection operation. This is therefore not a
  single-field special case.
- Exhaustive-audit consequence: one accidental or adversarial malformed JSONL
  record aborts the load-bearing gate before it can report what is invalid or
  produce exact totals. That violates the brief's explicit input-error and
  machine-readable-output contract and prevents dependable fail-closed
  operation over the eventual multi-million-line ledger.
- Required correction: separate parsed records from schema-valid records and
  never pass a record into key construction, sorting, interval math, reference
  resolution, finding linking, or review grouping when its canonical validator
  returned errors. Preserve the schema diagnostic, then continue safely so the
  CLI always emits the final `INVALID` summary. Add CLI regression tests for
  malformed-but-valid-JSON types at every input stage (universe, primary,
  counterpart, review, checked source, and finding header), asserting exit 1,
  parseable JSON diagnostics, and a final summary rather than merely asserting
  a Python exception or nonzero status.

### Important — per-repository diagnostic counts omit attributable schema failures

- Location: `scripts/verify_ledger.py:245-247`, `337-339`, `477-479`, and
  `592-609`.
- Root cause: schema diagnostics are emitted with only their input file and
  line. Even when the parsed record has a valid `repo`, or a review's valid
  `range_id` resolves to a primary repository, the diagnostic is not tagged
  with that repository. Lines 592-600 count only diagnostics carrying `repo`,
  while lines 602-609 separately count every diagnostic for the total.
- Reproducible counterexample: remove `contract` from the valid Ghostty primary
  `g-first`. The verifier correctly exits 1 and emits two `E_LEDGER_SCHEMA`
  diagnostics. The observed summary is internally inconsistent:

  ```text
  repositories.ghostty.invalid_diagnostics = 0
  repositories.rustty.invalid_diagnostics  = 0
  total.invalid_diagnostics                = 2
  ```

  Removing `falsification` from that range's review similarly produces
  Ghostty 0, Rustty 0, total 1. Both records unambiguously identify a Ghostty
  range.
- Exhaustive-audit consequence: the required exact diagnostic totals are not
  actually “split by repository and totaled.” Batch-level and repository-level
  progress reports will understate which side is invalid, and the repository
  counts cannot be reconciled to the total. The review contract explicitly
  classifies an incorrect derived count as Important.
- Required correction: attach the resolved repository to every record-level
  diagnostic whenever it is knowable (including schema diagnostics and review
  diagnostics resolved through `range_id`). Define and expose an explicit
  global/unattributed bucket for genuinely repository-independent input errors,
  or otherwise specify a reconciliation rule, then derive the total from the
  complete partition. Add assertions that known-repository schema failures are
  reflected in that repository and that the partition reconciles exactly.

## Verification performed

### Snapshot and pins

The supplied package hashes all matched at review start, including:

```text
698dfd7dc8372aaba668fbf61e0435073ee62af61d002f7b6a3fffa152e2812a  scripts/verify_ledger.py
b5f16ffa28ca42993979bcd64234047e81395313796cb7d370563465d22855ca  tests/test_verify_ledger.py
d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3  universe.jsonl
```

Current repository and universe pins were independently confirmed after the
real run:

```text
rustty   2bd45ccc79ae4ab6bdcb8cb95d5a99d0d8d44816
ghostty  6ad1fe7d8cbda36c77b337a96c9bea8a77883699
```

The universe contains exactly 11,150 records and 3,103,589 text lines:
Ghostty has 5,781 files / 515,483 text lines; Rustty has 5,369 files /
2,588,106 text lines.

### Tests and independent fixtures

The required focused suite passed unchanged:

```text
Ran 25 tests in 14.954s
OK
```

I also constructed a separate two-repository real-Git fixture without using
the suite's clean-fixture records. It contained four text lines, adjacent exact
coverage, cross-repository exact counterparts, one bidirectionally linked
parity-gap finding, and independent upheld reviews checking the exact primary
and counterpart ranges. It exited 0 with no diagnostics and exact totals of
four covered/final/upheld lines.

Independent interval mutations produced these derived totals:

| Case | Exit | Covered | Uncovered | Pending | Primary final | Secondary upheld |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| clean adjacent ranges | 0 | 7 | 0 | 0 | 7 | 7 |
| one-line gap | 2 | 6 | 1 | 0 | 6 | 6 |
| overlapping ranges | 1 | 7 | 0 | 0 | 7 | 7 |
| one pending line | 2 | 7 | 0 | 1 | 6 | 7 |
| one missing review | 2 | 7 | 0 | 0 | 7 | 6 |

This confirms interval-union counting for the tested valid-shape inputs,
including overlap not inflating covered lines, while the two Important findings
above remain open.

### Real incomplete run

The exact real command was rerun against the current pins. It exited 2 and
emitted exactly 100 printed `E_UNCOVERED` diagnostics plus the final summary.
The summary remained uncapped and reported:

```text
status                   INCOMPLETE
invalid_diagnostics      0
incomplete_diagnostics   6925
covered_lines            0
uncovered_lines          3103589
```

The repository split was Ghostty 1,761 uncovered intervals / 515,483 lines and
Rustty 5,164 uncovered intervals / 2,588,106 lines. This verifies the real empty
ledger remains fail-closed at the reviewed pins; it does not resolve the
malformed-input and diagnostic-partition defects.

## Decision

CHANGES_REQUIRED. Task 4 must not begin while the two Important findings are
open. After correction, rerun the entire focused suite, the malformed-type
matrix through the CLI, the repository diagnostic reconciliation probes, and
the exact real incomplete command.
