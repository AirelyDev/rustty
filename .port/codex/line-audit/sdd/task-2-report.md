# Task 2 implementation report — pinned bidirectional physical universe

## Status

Implemented and verified. No Rustty product file or Ghostty file was changed,
and nothing was staged or committed.

## Strict TDD evidence

### Red

After adding the new contract tests, the required command was run before
`build_universe.py` existed:

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_build_universe.py' -v
```

It failed as expected with `ModuleNotFoundError: No module named
'build_universe'`. This was a missing-production-module failure, not a test
setup failure.

### Green

After the implementation, the same command was run again:

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_build_universe.py' -v
```

Result: `Ran 8 tests in 1.422s` and `OK`.

The eight real-Git integration tests cover all required behaviors: stable Git
path order; the three content kinds; CRLF/LF/bare-CR physical line counting;
exact bytes and hashes; `HEAD` pinning and mismatch failure; committed
symlink-blob handling; exclusion of untracked files; schema validation;
byte-identical output; atomic failure preservation; and ghostty-before-rustty
ordering.

## Files created

- `scripts/build_universe.py`
- `tests/test_build_universe.py`
- `universe.jsonl`
- `sdd/task-2-report.md`

## Real pinned generation

Executed from `/Users/gael/projects/rustty` after green tests:

```text
python3 .port/codex/line-audit/scripts/build_universe.py \
  --rustty /Users/gael/projects/rustty \
  --ghostty /Users/gael/projects/ghostty \
  --output .port/codex/line-audit/universe.jsonl \
  --expect-rustty-revision 5243303fbb2f2cfd0832edfb8fa49d72ce322d81 \
  --expect-ghostty-revision 6ad1fe7d8cbda36c77b337a96c9bea8a77883699
```

The command completed successfully. A fresh parse of the generated JSONL
validated every record with Task 1's `validate_file`; total diagnostics: `0`.

| Repository | Files | Text | Binary | Empty | Physical text lines |
| --- | ---: | ---: | ---: | ---: | ---: |
| ghostty | 5,781 | 1,761 | 4,014 | 6 | 515,483 |
| rustty | 5,290 | 5,091 | 167 | 32 | 2,567,294 |
| total | 11,071 | 6,852 | 4,181 | 38 | 3,082,777 |

`universe.jsonl` SHA-256:

```text
81598efefa28edc7c2bc2b264d45f538add9f24f7a1b7b1668033f86ef718740
```

## Self-review

- The implementation imports Task 1's splitter and validator rather than
  reproducing their byte or hash rules.
- Recursive enumeration uses committed Git tree entries and `git cat-file`
  blob bytes; it never reads a working-tree file or follows a symlink target.
- A non-blob tree entry and a non-UTF-8 Git path fail explicitly.
- Records are ordered by fixed repository order and UTF-8 path bytes; all are
  validated before the temporary output replaces its destination.
- The JSONL output uses compact sorted-key objects, UTF-8, and a final newline.

## Scope and cleanliness

`git diff --name-only` was empty for both repositories. Ghostty's
`git status --short` was empty. Rustty's status contains only
`?? .port/codex/`, the audit directory that was already untracked before this
task and is the sole allowed write scope; no path outside
`.port/codex/line-audit/` changed. Nothing was staged or committed.

## Concerns

None. The universe is intentionally a snapshot of the two supplied committed
revisions; regenerating it after either `HEAD` changes will correctly reject
the stale expected revision.

## Fix round 1 — atomic-write failure coverage and repin

### Review finding addressed

Independent review finding `T2-R1` identified that the original atomic-failure
test only failed before temporary-output creation. A new test,
`test_replace_failure_preserves_destination_and_removes_sibling_temporary_file`,
uses a valid committed record, an existing destination, and a patched
`os.replace` that raises `OSError("injected replace failure")`. It asserts that
the existing destination bytes are unchanged and that no
`.universe.jsonl.*.tmp` sibling remains.

### Mutation-red and restored green evidence

The production cleanup guard was temporarily mutated from:

```python
if temporary_name is not None:
```

to:

```python
if False and temporary_name is not None:
```

The required test command then produced the expected red result: 9 tests ran,
with exactly `test_replace_failure_preserves_destination_and_removes_sibling_temporary_file`
failing because one `.universe.jsonl.*.tmp` sibling remained. The original
cleanup guard was restored with no production behavior change.

The same required command was rerun after restoration:

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_build_universe.py' -v
```

Result: `Ran 9 tests in 1.520s` and `OK`.

### New authoritative pin and generation

The previous universe was superseded because Rustty `HEAD` advanced. After the
green run, `universe.jsonl` was atomically regenerated with:

```text
python3 .port/codex/line-audit/scripts/build_universe.py \
  --rustty /Users/gael/projects/rustty \
  --ghostty /Users/gael/projects/ghostty \
  --output .port/codex/line-audit/universe.jsonl \
  --expect-rustty-revision fa02b36cc21eadcd8eaa8f68ae0bf1ec759ddc83 \
  --expect-ghostty-revision 6ad1fe7d8cbda36c77b337a96c9bea8a77883699
```

A fresh parse validated all records with Task 1's `validate_file`; total
diagnostics: `0`.

| Repository | Revision | Files | Text | Binary | Empty | Physical text lines |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| ghostty | `6ad1fe7d8cbda36c77b337a96c9bea8a77883699` | 5,781 | 1,761 | 4,014 | 6 | 515,483 |
| rustty | `fa02b36cc21eadcd8eaa8f68ae0bf1ec759ddc83` | 5,290 | 5,091 | 167 | 32 | 2,567,461 |
| total | — | 11,071 | 6,852 | 4,181 | 38 | 3,082,944 |

New `universe.jsonl` SHA-256:

```text
19d3d7270015d39b08a2b9c3e2fe0c0af3325526f3453effb55ecfc8b1599782
```

`git diff --name-only` remained empty for both repositories. Ghostty remained
fully clean. Rustty's only status entry remained the audit-scoped untracked
`.port/codex/` directory; no file outside `.port/codex/line-audit/` changed,
and nothing was staged or committed.

## Repin validation round 2

The authoritative committed pins for this round were:

```text
rustty   2bd45ccc79ae4ab6bdcb8cb95d5a99d0d8d44816
ghostty  6ad1fe7d8cbda36c77b337a96c9bea8a77883699
```

`universe.jsonl` was regenerated from those exact pins with the Task 2 CLI.
Measured generation runtime was `real 133.86s` (`user 51.53s`, `sys 71.90s`).
The Rustty committed tree includes a prior generated universe; this run treats
that file as an ordinary committed blob, so its new record hashes the prior
committed blob rather than attempting a self-referential fixed point.

A fresh parse of the resulting JSONL validated every record with Task 1's
`validate_file`; total diagnostics: `0`.

| Repository | Files | Text | Binary | Empty | Physical text lines |
| --- | ---: | ---: | ---: | ---: | ---: |
| ghostty | 5,781 | 1,761 | 4,014 | 6 | 515,483 |
| rustty | 5,369 | 5,164 | 168 | 37 | 2,588,106 |
| total | 11,150 | 6,925 | 4,182 | 43 | 3,103,589 |

New `universe.jsonl` SHA-256:

```text
d2ce9a50a8ca31bc5f3bab936bc34bc9b5898af59ab3d5f34c31c29af5c0eed3
```

The checked Ghostty `HEAD` remains the stated pin. Rustty `HEAD` also matches
the stated authoritative pin. No staging or commits were performed. The
shared Rustty worktree contains concurrent audit-artifact changes under
`.port/codex/`; this repin round modified only
`.port/codex/line-audit/universe.jsonl` and this report.
