# Task 2 brief — pinned bidirectional physical universe

## Context

Task 1 provides the canonical raw-line and file-record model. This task must
inventory every tracked path from the committed trees of both repositories,
including symlinks by their Git blob bytes, without reading untracked audit
files or following working-tree symlink targets.

## Binding constraints

- Read `/Users/gael/projects/rustty/.port/codex/line-audit/SCHEMA.md` and reuse
  `scripts/model.py`; do not duplicate its splitting/hash rules.
- Write only below `.port/codex/line-audit/`.
- Do not modify Rustty product files or Ghostty.
- Do not stage or commit anything.
- Python 3 standard library only.
- Strict TDD: new tests must be observed red before implementation and green
  afterward.

## Files

- Create `.port/codex/line-audit/scripts/build_universe.py`.
- Create `.port/codex/line-audit/tests/test_build_universe.py`.
- Generate `.port/codex/line-audit/universe.jsonl` only after tests pass.
- Write the implementation report to
  `.port/codex/line-audit/sdd/task-2-report.md`.

## Required interfaces

```python
def classify_content(data: bytes) -> str: ...
def read_committed_blobs(repo: pathlib.Path, revision: str) -> list[tuple[str, bytes]]: ...
def build_records(repo: pathlib.Path, repo_name: str, expected_revision: str | None = None) -> list[dict]: ...
def write_jsonl(records: list[dict], output: pathlib.Path) -> None: ...
```

- `read_committed_blobs` enumerates the full recursive committed tree at the
  supplied revision in Git path order and returns exact blob bytes. It must not
  follow working-tree symlinks. A non-blob tree entry is rejected explicitly;
  no tracked entry is silently skipped.
- `build_records` resolves `HEAD` to a full object ID, compares it to the
  optional expected revision, and fails on mismatch before emitting records.
- Repository names are exactly `ghostty` and `rustty`.
- Records sort by repository order (`ghostty` before `rustty`) and path bytes
  represented as valid UTF-8 POSIX paths. A non-UTF-8 Git path fails loudly
  rather than being omitted or mangled.
- Every emitted record must pass `validate_file` from Task 1.

## Content classification

```python
def classify_content(data: bytes) -> str:
    if not data:
        return "empty"
    if b"\0" in data[:8192]:
        return "binary"
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        return "binary"
    return "text"
```

- `text`: `line_count = len(split_physical_lines(data))`.
- `empty`: `line_count = 0`.
- `binary`: `line_count = 0` and initial `scope_class = "binary_asset"` with
  reason `"binary content has no physical text lines"` and evidence
  `"content classification: NUL prefix or invalid UTF-8"`.
- Text and empty files initially use `scope_class = "semantic_required"` with
  empty classification reason/evidence. Task 5 will refine classifications.
- `byte_length` is `len(data)`; `sha256` is the lowercase SHA-256 of the exact
  blob bytes.

## CLI

```text
build_universe.py \
  --rustty PATH --ghostty PATH --output FILE \
  --expect-rustty-revision HEX --expect-ghostty-revision HEX
```

The expectation options are optional for fixture use but both are required by
the real generation command. The CLI writes atomically by creating a sibling
temporary file and replacing the destination after all records validate.
JSON uses UTF-8, one compact object per line, sorted keys, and a final newline.
Running twice on unchanged commits must produce byte-identical output.

## Required tests

Create temporary Git repositories using real commits and local test-only author
configuration. At minimum test:

1. deterministic path order;
2. text, binary-by-NUL, binary-by-invalid-UTF8, and empty classification;
3. CRLF/LF/bare-CR line counts through Task 1's splitter;
4. exact byte lengths and content hashes;
5. revision equals committed `HEAD` and accepts the expected pin;
6. expected-revision mismatch fails without output;
7. a committed symlink is inventoried from its link-target blob bytes even when
   its working-tree target contains different bytes;
8. untracked files do not enter the universe;
9. output records pass `validate_file`;
10. two generated outputs are byte-identical;
11. atomic failure leaves an existing output unchanged;
12. both repository record sets are ordered `ghostty`, then `rustty`.

## Real generation

After green tests, run from `/Users/gael/projects/rustty`:

```text
python3 .port/codex/line-audit/scripts/build_universe.py \
  --rustty /Users/gael/projects/rustty \
  --ghostty /Users/gael/projects/ghostty \
  --output .port/codex/line-audit/universe.jsonl \
  --expect-rustty-revision fa02b36cc21eadcd8eaa8f68ae0bf1ec759ddc83 \
  --expect-ghostty-revision 6ad1fe7d8cbda36c77b337a96c9bea8a77883699
```

Report exact file counts, text/binary/empty counts, line totals, output SHA-256,
and whether both repositories remained clean.

## Test command and report contract

```text
python3 -m unittest discover -s .port/codex/line-audit/tests -p 'test_build_universe.py' -v
```

The report records status, observed red failure, files created, green command
and exact count, real generation command/result/totals, self-review, concerns,
and confirmation no file outside `.port/codex/line-audit/` changed.
