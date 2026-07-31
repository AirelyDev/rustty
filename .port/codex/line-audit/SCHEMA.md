# Line-audit ledger schema

`scripts/model.py` is the canonical standard-library implementation of these
rules. Its validators return every applicable deterministic diagnostic as a
list of strings; consumers must treat a nonempty list as an invalid record.

## Raw bytes and ranges

`split_physical_lines(data)` splits only at `LF`, preserving each raw `LF` or
`CRLF` terminator and a final unterminated physical line. A bare `CR` is content,
not a physical-line terminator. Empty data has no physical lines.

`range_sha256(lines, start, end)` uses one-based inclusive line coordinates.
It hashes the direct concatenation of the selected raw line bytes, including
their original terminators. `start < 1`, `end < start`, and `end > len(lines)`
raise `ValueError`.

## Shared value rules

- Repositories are `ghostty` or `rustty`.
- `revision` is a 40- or 64-character lowercase hexadecimal Git object ID.
  `sha256` and `range_sha256` values are exactly 64 lowercase hexadecimal
  characters.
- Paths are nonempty relative POSIX paths; absolute paths, backslashes, empty
  components, and `.` or `..` components are rejected.
- Integer length, count, coordinate, and revision fields reject booleans.
- Timestamps are valid RFC 3339 UTC date-times ending in `Z` (fractional
  seconds are allowed).
- A range reference contains `repo`, `revision`, `path`, `start`, `end`, and
  `range_sha256`, with positive inclusive coordinates.

## File records

Required fields:

```text
repo, revision, path, byte_length, line_count, sha256, content_kind,
scope_class, classification_reason, classification_evidence
```

`content_kind` is one of `text`, `binary`, or `empty`.

- Text has positive `byte_length` and `line_count`.
- Empty has zero `byte_length` and zero `line_count`.
- Binary has zero `line_count`; it is handled at file level rather than with
  text ranges.

`scope_class` is one of:

```text
semantic_required, pure_macos_out_of_scope, vendored_identical,
generated_reproducible, binary_asset, documentation_nonbehavioral
```

For every nonsemantic scope class, `classification_reason` and
`classification_evidence` are nonempty strings. They are still required
string fields for semantic records, but may be empty there.

## Primary records

Required fields:

```text
batch_id, range_id, repo, revision, path, start, end, range_sha256,
scope_class, risk_tags, counterparts, disposition, contract, analysis,
reachability, verification, observed_result, limitations, finding_ids,
primary_reviewer, reviewed_at
```

`batch_id`, `range_id`, and `primary_reviewer` are nonempty strings.
`counterparts` is an array of range references. `risk_tags` is an array of
unique, nonempty lowercase snake-case strings. `finding_ids` is an array of
nonempty strings.

`disposition` is one of:

```text
exact_equivalent, semantic_equivalent, port_specific_valid,
oracle_not_applicable, intentional_divergence, parity_gap, pending
```

For `semantic_required` records whose disposition is final (anything except
`pending`), `contract`, `analysis`, `reachability`, `verification`, and
`observed_result` must all be nonempty. `parity_gap` also requires at least
one nonempty finding ID.

Nonsemantic records carry the matching file record's classification reason and
evidence and must give nonempty rationale in `analysis`. Use
`validate_primary_with_file(primary, file_record)` as the canonical validator
when a primary is paired with its inventory record. It includes all
`validate_primary` diagnostics, validates the file record, requires exact
`(repo, revision, path)` and `scope_class` agreement, and requires the matched
file record to provide nonempty classification reason and evidence. Their
required dispositions are:

| Scope class | Disposition |
| --- | --- |
| `pure_macos_out_of_scope` | `oracle_not_applicable` |
| `vendored_identical` | `exact_equivalent` |
| `generated_reproducible` | `exact_equivalent` |
| `binary_asset` | `oracle_not_applicable` |
| `documentation_nonbehavioral` | `port_specific_valid` |

## Secondary review records

Required fields:

```text
batch_id, range_id, review_revision, secondary_reviewer, reviewed_at, verdict,
source_ranges_checked, falsification, counterevidence, corrections, supersedes
```

`review_revision` is a positive integer. `secondary_reviewer` and
`falsification` are nonempty strings. `source_ranges_checked` is a nonempty
array of range references. `counterevidence` and `corrections` are required
strings and may be empty when none exists.

`verdict` is one of `upheld`, `upheld_with_correction`, `rejected`, or
`needs_more_evidence`. `supersedes` is `null` for revision 1; later revisions
must name a positive revision strictly lower than `review_revision`.
