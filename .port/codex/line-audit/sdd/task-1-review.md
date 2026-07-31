# Task 1 independent review

## Verdicts

Spec compliance: FAIL

Task quality: CHANGES_REQUIRED

## Evidence reviewed

- The three supplied immutable hashes match the complete reviewed files.
- `python3 -m unittest discover -s .port/codex/line-audit/tests -p
  'test_model.py' -v` passed: 17 tests.
- I independently exercised the relevant negative cases; the records and byte
  sequences below are accepted by the current model despite the stated ledger
  rules.

## Findings

### 1. Critical — physical-line splitting creates coordinates that do not match the canonical LF/CRLF byte model

**Location:** `scripts/model.py:52` (with the contradictory canonical rule at
`SCHEMA.md:9-10` and missing regression coverage at
`tests/test_model.py:100-104`).

`bytes.splitlines(keepends=True)` treats a bare `CR` as a line ending.  That is
not the documented raw terminator set of `LF` and `CRLF`.  For example,
`split_physical_lines(b"one\rtwo\n")` currently returns
`[b"one\r", b"two\n"]`; under the stated model the bare `CR` is raw content
and the one physical line is `b"one\rtwo\n"`.  The resulting line counts,
range coordinates, and range SHA-256 values can therefore be wrong, defeating
the raw-byte basis of the coverage proof.

**Correction:** implement splitting explicitly on `b"\n"`, retaining a
preceding `b"\r"` in that same line and omitting only the artificial empty
tail after a terminal LF.  Add tests for bare-CR content before an LF, bare-CR
content in an unterminated final line, and the normal CRLF/LF cases.

### 2. Critical — the schema cannot record the actual Git revision pin without inventing a surrogate

**Location:** `SCHEMA.md:20-21`; enforced by `scripts/model.py:100-103` and
used for every file/range reference at `scripts/model.py:127`, `147`, and
`188`.

The schema calls the field `revision` but requires a 64-character SHA-256.
The reviewed Rustty checkout is pinned by a normal 40-character Git object ID
(`5243303fbb2f2cfd0832edfb8fa49d72ce322d81`), which the validator rejects.
The schema provides neither a Git revision field nor an algorithm/derivation
for a permitted 64-character replacement.  A fabricated SHA-256 would make a
record pass while no longer pinning an auditable repository revision; this
makes the promised bidirectional proof vacuous for the pinned repositories.

**Correction:** distinguish the repository revision from content hashes.  For
example, retain 64-character SHA-256 validation for `sha256` and
`range_sha256`, while documenting and validating `revision` as the repository
object ID used by the pins (accepting the applicable 40-character SHA-1 and
64-character SHA-256 Git formats, or storing an explicit algorithm-qualified
identifier).  Update all record and reference tests to use a real valid Git
pin and add rejection tests for malformed revision identifiers.  This requires
reconciling the same incompatible rule in the task brief before implementation.

### 3. Important — file records admit an impossible empty byte stream as a text file with a range

**Location:** `scripts/model.py:157-158`; absent from
`tests/test_model.py:144-164`.

`validate_file` accepts a `text` record with `byte_length: 0` and
`line_count: 1`.  No byte sequence has this inventory: the documented splitter
returns zero physical lines for empty data (`SCHEMA.md:9-10`), while every text
physical line has at least one byte.  Such a record lets downstream tooling
claim a hashable text range for an empty file and weakens the coverage proof.

**Correction:** when `content_kind == "text"`, require both `line_count >= 1`
and `byte_length >= 1`; add a focused rejecting test.  Keep the existing empty
file rule for the only valid zero-byte/zero-line case.

### 4. Important — nonsemantic primary records validate without the required file-record evidence

**Location:** `scripts/model.py:233-239`; schema requirement at
`SCHEMA.md:85-86` (and brief at `sdd/task-1-brief.md:87-89`).

The only nonsemantic checks are the mapped disposition and nonempty `analysis`.
The primary record contains no file-record reference or evidence fields, and
`validate_primary` has no file-record input.  Consequently a
`binary_asset` primary with an empty `counterparts` array and only an analysis
string returns no diagnostics even if no matching file record exists or its
classification reason/evidence is empty.  This fails the required evidence
chain for exclusions; the implementation report acknowledges the gap but does
not provide the required enforcement.

**Correction:** add a canonical cross-record validator (or change the primary
validator to accept its file record) that resolves the exact `(repo, revision,
path)` file record and requires its nonempty classification reason and
evidence for every nonsemantic primary.  Make that validation part of the
schema contract and add both missing-file and empty-evidence tests.  If this is
deliberately deferred, the brief and schema must say it is not enforced in
Task 1 rather than claiming the primary rule is satisfied.
