# Task 2 fix round 1 scoped re-review

## Verdict

- T2-R1: **ADDRESSED**
- New Critical/Important breakage: **none**
- Approval: **APPROVED**

## Reviewed scope

The implementation is unchanged and retains the previously reviewed hash:

```text
665cea4d4c56cfb2e638791b5cab5dbb535b37edefd2bd2912789e8b8fdb6309  scripts/build_universe.py
```

The new test and regenerated universe match the fix-round package:

```text
c4d9fec725c7c890403a1e20cb7b6bc975b6874b1ce6e2dbb40d637bbc37f79a  tests/test_build_universe.py
19d3d7270015d39b08a2b9c3e2fe0c0af3325526f3453effb55ecfc8b1599782  universe.jsonl
```

## T2-R1 closure

The new
`test_replace_failure_preserves_destination_and_removes_sibling_temporary_file`
uses a valid committed record and existing output, injects an `OSError` from
`os.replace`, then asserts both that the destination retains its original
bytes and that the sibling temporary file is gone.  This exercises the
production `finally` cleanup after the temporary output has been written and
before replacement completes—the failure phase absent from the prior test.

I independently ran the required suite:

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_build_universe.py' -v
Ran 9 tests in 1.666s
OK
```

I also replayed the report's exact cleanup-guard mutation in an isolated
temporary fixture (`if temporary_name is not None:` changed to
`if False and temporary_name is not None:`), with the same injected replace
failure.  The destination remained intact and exactly one
`.universe.jsonl.*.tmp` sibling remained.  That is the failure asserted by the
new test, so its reported mutation-red evidence is substantiated.

## Regenerated-universe binding

I parsed all 11,071 JSONL records, checked the final newline and required
repository/path-byte ordering, and ran Task 1 `validate_file` on each record:
0 diagnostics and 0 duplicate `(repo, revision, path)` keys.

I then independently enumerated each pinned tree with
`git ls-tree -r -z --full-tree` and retrieved its exact blob bytes through
`git cat-file --batch`.  For every path I compared membership, byte length,
SHA-256, content classification, physical line count, scope class, and
classification reason/evidence to a fresh calculation.

| Repository pin | Tree entries | Universe records | Missing | Extra | Field mismatches | Text / binary / empty | Physical lines |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ghostty @ `6ad1fe7d8cbda36c77b337a96c9bea8a77883699` | 5,781 | 5,781 | 0 | 0 | 0 | 1,761 / 4,014 / 6 | 515,483 |
| rustty @ `fa02b36cc21eadcd8eaa8f68ae0bf1ec759ddc83` | 5,290 | 5,290 | 0 | 0 | 0 | 5,091 / 167 / 32 | 2,567,461 |
| total | 11,071 | 11,071 | 0 | 0 | 0 | 6,852 / 4,181 / 38 | 3,082,944 |

Both trees contained only blob entries.  The regenerated universe is therefore
non-vacuous and exactly binds every tracked committed path and blob byte for
the requested Rustty and Ghostty pins, with no path omission or substitution.

## Conclusion

T2-R1 is addressed.  The prior required-test gap is closed, the mutation-red
claim is independently reproducible, the current focused suite is green, and
the repinned universe is complete and exact.  No new Critical or Important
breakage was found.
