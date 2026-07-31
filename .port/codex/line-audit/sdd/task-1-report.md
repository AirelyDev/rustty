# Task 1 implementation report

## Status

DONE

## TDD evidence

- Red command: `python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_model.py' -v`
- Red exit status: `1`
- Expected failure: the test module could not import `model`, because
  `scripts/model.py` had not yet been created.
- Green command: `python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_model.py' -v`
- Green exit status: `0`
- Green result: exactly 17 tests ran and passed.

## Files created

- `.port/codex/line-audit/scripts/model.py`
- `.port/codex/line-audit/tests/test_model.py`
- `.port/codex/line-audit/SCHEMA.md`
- `.port/codex/line-audit/sdd/task-1-report.md`

## Self-review

- Confirmed raw hashes select 1-based inclusive physical-line slices and
  include untouched CRLF/LF bytes.
- Confirmed all required enum, integer, path/hash, evidence, timestamp,
  parity-gap, risk-tag, and review-supersession cases have independent tests.
- Confirmed validators accumulate independent diagnostics rather than stopping
  after a first failure.
- Ran `git diff --check`; it reported no whitespace errors.

## Remaining concerns

- A primary record has no embedded or referenced file-record identifier beyond
  its repository/revision/path fields. Therefore `validate_primary` can require
  its nonsemantic `analysis` rationale but cannot, by itself, prove that the
  separate file record carries classification evidence; that cross-record join
  belongs to a later ledger verifier.
- The brief requires nonsemantic records to use their scope-class disposition
  without enumerating the mapping. The model and schema make the chosen mapping
  explicit: excluded/binary records are `oracle_not_applicable`, identical and
  reproducible records are `exact_equivalent`, and nonbehavioral documentation
  is `port_specific_valid`.

## Scope confirmation

No file outside `.port/codex/line-audit/` changed. Nothing was staged or
committed.

## Fix round 1 — independent review corrections

- Red command: `python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_model.py' -v`
- Red exit status: `1`
- Expected failure: the new regression tests could not import
  `validate_primary_with_file`, because the canonical cross-record validator
  did not yet exist.
- Implemented LF-only physical-line splitting, Git object-ID revision
  validation (40 or 64 lowercase hexadecimal characters), positive text byte
  length, and `validate_primary_with_file` cross-record enforcement.
- Updated the brief and schema to state the binding bare-CR, revision, text,
  and cross-record rules.
- Green command: `python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_model.py' -v`
- Green exit status: `0`
- Green result: exactly 24 tests ran and passed.
- Re-ran `git diff --check`; it reported no whitespace errors. No file outside
  `.port/codex/line-audit/` changed, and nothing was staged or committed.

The cross-record evidence concern in the initial report is resolved by
`validate_primary_with_file`. The nonsemantic disposition mapping remains an
explicit documented policy because the source brief does not enumerate it.
