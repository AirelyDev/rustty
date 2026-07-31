# Task 3 brief — fail-closed coverage and review verifier

## Context

Tasks 1–2 define the record model and exact pinned physical universe. This task
is the audit's load-bearing gate: it must reject partial, stale, overlapping,
weakly evidenced, or self-reviewed ledgers and must never turn an authored
subset into a green result.

## Binding constraints

- Read `.port/codex/line-audit/SCHEMA.md`, `scripts/model.py`, and
  `scripts/build_universe.py` first.
- Write only below `.port/codex/line-audit/`.
- Do not modify Rustty product files or Ghostty; do not stage or commit.
- Python 3 standard library only.
- Follow strict TDD. Every mutation named below must be observed to fail while
  the clean fixture passes.
- The real repository verification must report `INCOMPLETE` with exit 2 until
  the exhaustive ledger is actually finished.

## Files

- Create `.port/codex/line-audit/scripts/verify_ledger.py`.
- Create `.port/codex/line-audit/tests/test_verify_ledger.py`.
- Create placeholder documentation files (not fake ledger records) in
  `.port/codex/line-audit/ledger/`, `reviews/`, and `findings/` explaining that
  only JSONL/Markdown records named by batches count.
- Write `.port/codex/line-audit/sdd/task-3-report.md`.

## CLI

```text
verify_ledger.py \
  --universe FILE --ledger DIR --reviews DIR --findings DIR \
  --rustty PATH --ghostty PATH [--allow-incomplete]
```

Exit codes:

- `0`: fully complete and valid only;
- `1`: invalid schema, source drift, overlap, stale hash, evidence violation,
  broken review/finding link, or tool/input error;
- `2`: structurally valid but incomplete (`uncovered`, `pending`, missing final
  acceptable review, or a latest review that is rejected/needs evidence).

`--allow-incomplete` means continue gathering all diagnostics and print the
incomplete summary; it never changes exit 2 to exit 0.

Machine-readable diagnostics go to stdout as one JSON object per line followed
by a final summary object. Human-readable stderr is allowed but not required.
Each diagnostic has `code`, `message`, and relevant location keys.

## Input loading

- Universe: every nonblank JSONL line is one file record, validated with
  `validate_file`. Reject an empty universe and duplicate
  `(repo, revision, path)` keys.
- Primary ledgers: load only `*.jsonl` directly inside `--ledger`, sorted by
  filename and line number. Validate every object with
  `validate_primary_with_file`. Reject duplicate `range_id` values.
- Reviews: load only `*.jsonl` directly inside `--reviews`, sorted. Validate
  every object with `validate_review`.
- Findings: load only `*.md` directly inside `--findings`. Each promoted
  finding begins exactly with:

```text
---
{"id":"F-LA-001","range_ids":["range-a","range-b"]}
---
```

Parse the second physical line as compact JSON. Reject duplicate finding IDs,
empty range arrays, unknown range IDs, duplicate range IDs within one finding,
or missing closing delimiter.

Schema-invalid parsed values produce all applicable schema diagnostics but are
never admitted to key construction, sorting, interval math, reference
resolution, finding linking, or review grouping. Malformed-but-valid JSON must
therefore end with a machine-readable `INVALID` summary rather than aborting
the CLI.

Review-diagnostic attribution uses a separate identity-only index of parsed
primaries with individually valid nonempty string `range_id` and valid `repo`.
This index never supplies semantic records. Attribution requires exactly one
parsed occurrence. Every repeated individually valid range ID produces one
`E_RANGE_DUPLICATE`; repeated same- or cross-repository identities are
ambiguous and their review diagnostics remain unattributed.

## Repository and source binding

- Every universe repository has exactly one revision, equal to the current full
  `HEAD^{commit}` of its supplied path. Mismatch is `E_SOURCE_REVISION` and
  invalid, not incomplete.
- Independently rebuild the file records from each pinned committed tree using
  Task 2 and compare the complete `(path, byte_length, line_count, sha256,
  content_kind)` set. Missing/extra/mismatched files are invalid.
- Recompute each primary source range and every counterpart range from committed
  blob bytes using Task 1. Range bounds must be within a text file and exact
  hashes must match.
- Empty/binary files have no primary line range. Their physical existence and
  bytes are covered by the universe. Their non-macOS integration boundary will
  be represented by semantic ranges in consuming text files.

## Exact coverage

For every text file with `line_count > 0`, sort its primary ranges by start/end:

- first start must be 1;
- each next start must equal previous end plus 1;
- final end must equal file line count;
- overlaps, gaps, reversed ranges, and out-of-bounds coordinates are invalid or
  incomplete as specified below.

`E_OVERLAP`, `E_BOUNDS`, and duplicate range IDs are invalid (exit 1).
Uncovered intervals are `E_UNCOVERED` and incomplete (exit 2). Coverage counts
are calculated from the interval union, never by summing possibly overlapping
range lengths.

## Primary evidence rules

- `pending` is incomplete.
- Every non-pending `semantic_required` record already satisfies Task 1's
  nonempty contract/analysis/reachability/verification/result rules.
- The following exact tags are high risk:

```python
HIGH_RISK_TAGS = {
    "platform_cfg", "unsafe", "raw_pointer", "ffi", "allocator",
    "error_conversion", "state_machine", "parser_dispatch", "compression",
    "rendering", "concurrency", "subprocess_environment", "public_abi",
    "serialization", "test_skip", "assertion_weakening",
}
```

- A high-risk final equivalence (`exact_equivalent`, `semantic_equivalent`, or
  `port_specific_valid`) and every `parity_gap` must carry an extra nonempty
  string field `control_evidence`. The Task 1 model permits extra fields; this
  verifier makes the field mandatory for these cases. Missing evidence is
  `E_HIGH_RISK_PROOF` and invalid.
- `parity_gap` finding IDs must resolve bidirectionally as described below.
- `intentional_divergence` must carry extra nonempty string field
  `authority_evidence`; missing authority is invalid.
- Nonsemantic source ranges must use the file's scope and evidence through
  `validate_primary_with_file`.

## Counterpart rules

- Every counterpart resolves to an existing universe text file at the exact
  revision, within bounds, with matching range hash.
- For `exact_equivalent` and `semantic_equivalent`, `counterparts` is nonempty.
- For every other disposition it may be empty when analysis explains why.
- For semantic equivalence, every counterpart repository must differ from the
  primary repository. A same-repository helper may be cited in prose but does
  not satisfy the cross-port counterpart requirement.

## Independent review rules

- Group reviews by `range_id`. Reject reviews of unknown ranges and duplicate
  `(range_id, review_revision)` pairs.
- Revisions must form a contiguous chain starting at 1. Revision 1 has null
  `supersedes`; every later revision supersedes exactly the immediately prior
  revision.
- The latest review's `secondary_reviewer` must differ from the primary
  reviewer (`E_SELF_REVIEW` invalid).
- `source_ranges_checked` must include exact references for the primary range
  and every counterpart range. Missing inspected sources are invalid.
- Latest `upheld` or `upheld_with_correction` completes secondary review.
  Missing latest review, `rejected`, or `needs_more_evidence` is incomplete.

## Finding-link rules

- Every primary `parity_gap` finding ID names a parsed finding file whose
  `range_ids` contains that range.
- Every finding `range_id` names a primary `parity_gap` record whose
  `finding_ids` contains that finding ID.
- No non-gap primary may cite a finding.
- Broken links use `E_FINDING_LINK` and are invalid.

## Stable diagnostic codes

At minimum implement and test:

```text
E_UNIVERSE_EMPTY E_UNIVERSE_SCHEMA E_UNIVERSE_DUPLICATE
E_SOURCE_REVISION E_SOURCE_FILESET E_LEDGER_SCHEMA E_RANGE_DUPLICATE
E_BOUNDS E_OVERLAP E_UNCOVERED E_STALE_HASH E_PENDING
E_COUNTERPART E_HIGH_RISK_PROOF E_AUTHORITY_EVIDENCE
E_REVIEW_SCHEMA E_REVIEW_UNKNOWN E_REVIEW_CHAIN E_REVIEW_MISSING
E_SELF_REVIEW E_REVIEW_SOURCES E_EXCLUSION_EVIDENCE E_FINDING_LINK
```

The final summary contains exact `files`, `text_lines`, `covered_lines`,
`uncovered_lines`, `pending_lines`, `primary_final_lines`,
`secondary_upheld_lines`, `invalid_diagnostics`, and
`incomplete_diagnostics`, split by repository and totaled.
It also contains an explicit `unattributed` diagnostic bucket for errors that
cannot be assigned to either repository. For both diagnostic classes, the two
repository counts plus `unattributed` equal the total exactly.

## Required clean fixture and mutation tests

Use two temporary real Git repositories with at least one text file on each
side. Generate their universe with Task 2. The clean fixture must include exact
cross-repository primary records for every line, acceptable independent reviews
checking every source/counterpart, and a correctly linked parity-gap finding
for one range. It must exit 0.

Independently mutate the clean fixture and assert the exact diagnostic and
nonzero class for:

1. empty universe;
2. deleted universe file record;
3. duplicate universe record;
4. current repository revision drift;
5. stale universe file content/hash;
6. uncovered first, middle, and last lines;
7. overlap and out-of-bounds range;
8. stale primary range hash;
9. absent/malformed/mismatched counterpart;
10. `pending` disposition;
11. high-risk equivalence without `control_evidence`;
12. intentional divergence without `authority_evidence`;
13. missing review;
14. self-review;
15. broken/noncontiguous review revision chain;
16. review omitting a source or counterpart range;
17. exclusion with missing file evidence;
18. gap without finding, finding without gap, and unknown range;
19. duplicate range ID;
20. non-gap record that cites a finding;
21. zero-line count summary tampering is impossible because summary is derived.

The test helper may call verifier functions directly but must also test CLI exit
0/1/2 and JSON diagnostic/summary output.

## Real incomplete run

After tests pass, run:

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

Expected: exit 2, `INCOMPLETE`, zero invalid diagnostics, and all 3,103,589 text
lines uncovered. Cap individual uncovered diagnostics at 100 in output while
the summary retains exact totals, so an empty ledger does not emit megabytes.

## Report contract

Run:

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_verify_ledger.py' -v
```

The report records status, observed red, green test count, every mutation and
expected diagnostic/exit class, real incomplete command/result/exact summary,
runtime, self-review, concerns, and repository-boundary confirmation.
