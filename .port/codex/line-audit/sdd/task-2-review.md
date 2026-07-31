# Task 2 independent review — pinned bidirectional physical universe

## Verdict

- Spec compliance: **FAIL**
- Task quality: **CHANGES_REQUIRED**
- Findings: **1**

## Scope and artifact identity

Reviewed only the Task 2 audit artifacts named in the review package.  The
reviewed SHA-256 values match the package exactly:

```text
665cea4d4c56cfb2e638791b5cab5dbb535b37edefd2bd2912789e8b8fdb6309  scripts/build_universe.py
62672e96753fddea1baa8aa2c7a20f1bf8f439754402df9f0186110d42459a69  tests/test_build_universe.py
81598efefa28edc7c2bc2b264d45f538add9f24f7a1b7b1668033f86ef718740  universe.jsonl
```

No product or Ghostty file was written, staged, or committed by this review.

## Positive verification

The implementation conforms to the substantive inventory requirements:

- It resolves `HEAD^{commit}`, compares an optional pin before iterating, and
  rejects a mismatch before records can reach `write_jsonl`.
- It uses `git ls-tree -r -z --full-tree` plus `git cat-file blob`; therefore
  it reads committed blob objects, not working-tree files, and cannot follow a
  working-tree symlink.  It rejects any emitted non-blob entry and fails on a
  non-UTF-8 path.
- Classification is byte-for-byte the required algorithm.  It reuses Task 1's
  physical-line splitter and `validate_file`, uses exact blob byte lengths and
  SHA-256 values, and applies the required binary scope/reason/evidence.
- JSONL validation precedes creation of the sibling temporary file; output is
  compact, key-sorted UTF-8 JSON with a final newline, and replacement uses
  `os.replace`.

I ran the required test command independently:

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_build_universe.py' -v
Ran 8 tests in 1.739s
OK
```

I also independently parsed every generated JSONL record and validated each
with Task 1's `validate_file`: 0 diagnostics, 0 duplicate
`(repo, revision, path)` keys, and required repository/path-byte ordering.
Against `git ls-tree -r -z --full-tree` at each stated revision, with exact
blob bytes read through `git cat-file --batch`, the universe has the following
complete equality results:

| Pinned repository | Git entries | Universe records | Missing | Extra | Byte/metadata mismatches | Text / binary / empty | Physical lines |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ghostty @ `6ad1fe7d8cbda36c77b337a96c9bea8a77883699` | 5,781 | 5,781 | 0 | 0 | 0 | 1,761 / 4,014 / 6 | 515,483 |
| rustty @ `5243303fbb2f2cfd0832edfb8fa49d72ce322d81` | 5,290 | 5,290 | 0 | 0 | 0 | 5,091 / 167 / 32 | 2,567,294 |
| total | 11,071 | 11,071 | 0 | 0 | 0 | 6,852 / 4,181 / 38 | 3,082,777 |

Both recursive trees emitted only `blob` entries.  For every path, the review
compared the emitted `byte_length`, SHA-256, classification, physical line
count, scope class, reason, and evidence to a fresh calculation from the
pinned blob bytes.  This establishes non-vacuous, bidirectional path and byte
binding for the supplied universe; it does not rely on the current Rustty
`HEAD`, which has advanced since the pinned snapshot.

## Finding

### T2-R1 — Required atomic-write failure test does not exercise atomic writing

- **Severity:** Medium
- **Location:** `tests/test_build_universe.py:157-176`
- **Proof:** The test named
  `test_cli_mismatch_and_validation_failure_leave_existing_output_unchanged`
  checks (1) a revision mismatch in `build_records`, which occurs before
  `write_jsonl` is called, and (2) an invalid record at
  `write_jsonl`'s preflight validation.  Production code performs that
  validation at `scripts/build_universe.py:121-125`, before it creates the
  temporary file at lines 129-137 or calls `os.replace` at line 142.  Thus no
  test causes an error after temporary-output creation and before replacement;
  the assertion at line 175 cannot establish the required atomic-write failure
  property.
- **Consequence for exhaustive coverage:** A regression that truncates or
  replaces an existing universe after beginning a write can pass all eight
  tests.  The audit universe is its coverage boundary, so a damaged partial
  output could silently reduce the subsequent line-audit population rather
  than preserving the prior complete snapshot.
- **Correction:** Add a test with valid records and an existing destination
  that injects an error during temporary-file writing or at `os.replace`.
  Assert that the destination retains its original bytes and that the sibling
  temporary file is removed.  Keep the existing preflight-validation test as a
  separate validation-failure case.

## Conclusion

The generated universe itself is complete and exactly bound to both required
pinned committed trees.  The implementation is sound on direct inspection,
but Task 2 explicitly requires a test that an atomic failure preserves an
existing output.  The present test only covers failures before atomic writing
begins, so the required test contract is not met.
