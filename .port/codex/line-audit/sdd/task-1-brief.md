# Task 1 brief — ledger schema and canonical byte/range rules

## Context

This is the foundation of the exhaustive bidirectional Rustty/Ghostty line
audit. Later inventory, verifier, batch, and semantic-review tasks depend on
these exact byte and schema rules.

## Binding constraints

- Write only below `.port/codex/line-audit/`.
- Do not modify Rustty product files or Ghostty.
- Do not stage or commit anything.
- Use Python 3 standard library only.
- Follow strict TDD: author tests, run and observe the expected red result,
  implement minimally, then run and observe green.

## Files

- Create `.port/codex/line-audit/scripts/model.py`.
- Create `.port/codex/line-audit/tests/test_model.py`.
- Create `.port/codex/line-audit/SCHEMA.md`.
- Write the implementation report to
  `.port/codex/line-audit/sdd/task-1-report.md`.

## Required interfaces

- `split_physical_lines(data: bytes) -> list[bytes]` splits only at `LF`,
  preserves the raw `LF` or `CRLF` bytes and the final unterminated line, and
  treats a bare `CR` as content.
- `range_sha256(lines: list[bytes], start: int, end: int) -> str` uses 1-based
  inclusive coordinates and hashes concatenated raw line bytes. It raises
  `ValueError` for start below 1, end before start, and end beyond EOF.
- `validate_file(record: dict) -> list[str]`.
- `validate_primary(record: dict) -> list[str]`.
- `validate_primary_with_file(primary: dict, file_record: dict | None) -> list[str]`.
- `validate_review(record: dict) -> list[str]`.
- Validation returns all deterministic diagnostics rather than stopping at the
  first field error.

## Enum values

```python
CONTENT_KINDS = {"text", "binary", "empty"}
SCOPE_CLASSES = {
    "semantic_required", "pure_macos_out_of_scope", "vendored_identical",
    "generated_reproducible", "binary_asset", "documentation_nonbehavioral",
}
DISPOSITIONS = {
    "exact_equivalent", "semantic_equivalent", "port_specific_valid",
    "oracle_not_applicable", "intentional_divergence", "parity_gap", "pending",
}
SECONDARY_VERDICTS = {
    "upheld", "upheld_with_correction", "rejected", "needs_more_evidence",
}
```

Risk tags are an array of unique nonempty lowercase snake-case strings rather
than a closed enum, allowing later subsystem-specific tags without a schema
migration.

## Required file record fields

`repo`, `revision`, `path`, `byte_length`, `line_count`, `sha256`,
`content_kind`, `scope_class`, `classification_reason`, and
`classification_evidence`.

- `repo` is `ghostty` or `rustty`.
- Revision is a 40- or 64-character lowercase hexadecimal Git object ID;
  SHA-256 values are 64 lowercase hexadecimal characters.
- Paths are nonempty relative POSIX paths with no `.` or `..` component.
- Length/count values are nonnegative integers but not booleans.
- Text records have positive byte length and line count; empty records have zero bytes/lines;
  binary records have zero line count because they use file-level disposition.
- A non-`semantic_required` scope class requires nonempty reason and evidence.

## Required primary record fields

`batch_id`, `range_id`, `repo`, `revision`, `path`, `start`, `end`,
`range_sha256`, `scope_class`, `risk_tags`, `counterparts`, `disposition`,
`contract`, `analysis`, `reachability`, `verification`, `observed_result`,
`limitations`, `finding_ids`, `primary_reviewer`, and `reviewed_at`.

- `counterparts` is an array of objects containing `repo`, `revision`, `path`,
  `start`, `end`, and `range_sha256`.
- Text ranges use positive 1-based inclusive bounds.
- All semantic final dispositions except `pending` require nonempty contract,
  analysis, reachability, verification, and observed result.
- `parity_gap` requires at least one nonempty finding ID.
- Nonsemantic scope records use their scope-class disposition and require
  classification reason and evidence carried by their matching file record;
  `validate_primary_with_file` enforces that cross-record relationship and the
  primary schema still requires rationale in `analysis`.
- Timestamps are RFC 3339 UTC strings ending in `Z`.

## Required review record fields

`batch_id`, `range_id`, `review_revision`, `secondary_reviewer`, `reviewed_at`,
`verdict`, `source_ranges_checked`, `falsification`, `counterevidence`,
`corrections`, and `supersedes`.

- Review revision is a positive integer.
- Checked source ranges are a nonempty array of range references.
- Reviewer identity and falsification text are nonempty.
- `supersedes` is null for revision 1 and a positive prior revision otherwise.

## Required tests

At minimum, independently test:

1. mixed CRLF/LF and final unterminated splitting;
2. empty input;
3. one-based inclusive hash;
4. every invalid range-bound case;
5. valid file, primary, and review records produce no diagnostics;
6. invalid enums;
7. booleans rejected as integer fields;
8. invalid relative paths and hashes;
9. exclusion missing reason/evidence;
10. parity gap missing finding;
11. semantic final record missing evidence fields;
12. duplicate/invalid risk tags;
13. invalid timestamp;
14. review revision/supersedes inconsistency;
15. validation returns more than one diagnostic for a multiply-invalid record.

## Commands and report contract

Run tests from the Rustty root with:

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_model.py' -v
```

The report must record:

- status: `DONE`, `DONE_WITH_CONCERNS`, `NEEDS_CONTEXT`, or `BLOCKED`;
- the red command, exit status, and why the failure was expected;
- files created;
- the green command, exit status, and exact test count;
- self-review findings and remaining concerns;
- confirmation that no file outside `.port/codex/line-audit/` changed.
