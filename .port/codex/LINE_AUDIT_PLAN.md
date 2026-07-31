# Exhaustive Line-Level Parity Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build and execute a hash-bound, bidirectional ledger proving that
every tracked Ghostty and Rustty line received the required non-macOS parity
review or an explicit verified scope disposition.

**Architecture:** A deterministic inventory binds both repositories to pinned
revisions and hashes. Primary semantic-range ledgers and independent review
ledgers cover that universe exactly once. A fail-closed verifier and mutation
self-tests prevent partial or stale work from appearing complete; a batch
selector drives parallel subsystem review until no uncovered range remains.

**Tech Stack:** Python 3 standard library, JSON Lines, `unittest`, Git plumbing,
existing Cargo/Zig/tooling, Markdown evidence under `.port/codex/`.

## Global Constraints

- Rustty product files and the entire Ghostty checkout are read-only.
- All created reports, ledgers, scripts, tests, fixtures, and evidence live
  below `/Users/gael/projects/rustty/.port/codex/`.
- Do not create issues, pull requests, commits, or staged changes.
- Scope is Linux, FreeBSD, Windows, WASM/browser, embedded C ABI, and shared
  behavior; pure macOS ranges receive explicit out-of-scope dispositions.
- Every tracked file in both pinned revisions enters the physical universe.
- Every first-party non-macOS/shared semantic line receives primary and
  independent secondary review.
- No uncertain range may be converted from `pending` to equivalence merely to
  improve the coverage percentage.
- No completion claim is permitted while the real verifier exits nonzero.

---

### Task 1: Define the ledger schema and canonical byte/range rules

**Files:**

- Create: `.port/codex/line-audit/scripts/model.py`
- Create: `.port/codex/line-audit/tests/test_model.py`
- Create: `.port/codex/line-audit/SCHEMA.md`

**Interfaces:**

- Produces `FileRecord`, `RangeRef`, `PrimaryRecord`, and `ReviewRecord`
  validation functions that accept decoded JSON dictionaries and return a list
  of diagnostic strings.
- Produces `split_physical_lines(data: bytes) -> list[bytes]` and
  `range_sha256(lines: list[bytes], start: int, end: int) -> str` using 1-based
  inclusive line coordinates and raw bytes including original line endings.
- Defines the complete enum sets for content kinds, scope classes,
  dispositions, risk tags, and secondary verdicts.

- [ ] **Step 1: Write failing schema and byte-boundary tests**

```python
class ModelTests(unittest.TestCase):
    def test_split_preserves_mixed_line_endings_and_final_unterminated_line(self):
        self.assertEqual(
            split_physical_lines(b"a\r\nb\nc"),
            [b"a\r\n", b"b\n", b"c"],
        )

    def test_range_hash_is_one_based_and_inclusive(self):
        lines = [b"a\n", b"b\n", b"c\n"]
        self.assertEqual(range_sha256(lines, 2, 3), sha256(b"b\nc\n").hexdigest())

    def test_primary_rejects_unknown_disposition(self):
        record = valid_primary_record()
        record["disposition"] = "looks_good"
        self.assertIn("invalid disposition", validate_primary(record)[0])
```

- [ ] **Step 2: Run the model tests and confirm missing imports/functions fail**

Run: `python3 -m unittest .port/codex/line-audit/tests/test_model.py -v`  
Expected: nonzero with `ModuleNotFoundError` or undefined model functions.

- [ ] **Step 3: Implement the model and document every field**

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

def split_physical_lines(data: bytes) -> list[bytes]:
    return data.splitlines(keepends=True)

def range_sha256(lines: list[bytes], start: int, end: int) -> str:
    if start < 1 or end < start or end > len(lines):
        raise ValueError("range outside physical lines")
    return hashlib.sha256(b"".join(lines[start - 1:end])).hexdigest()
```

- [ ] **Step 4: Run the model tests**

Run: `python3 -m unittest .port/codex/line-audit/tests/test_model.py -v`  
Expected: all model tests pass.

- [ ] **Step 5: Inspect repository boundaries**

Run: `git status --short --untracked-files=no`  
Expected: no tracked product modification.

---

### Task 2: Build the pinned bidirectional physical universe

**Files:**

- Create: `.port/codex/line-audit/scripts/build_universe.py`
- Create: `.port/codex/line-audit/tests/test_build_universe.py`
- Create: `.port/codex/line-audit/tests/fixtures/universe-repo/`
- Generate: `.port/codex/line-audit/universe.jsonl`

**Interfaces:**

- Consumes the model byte rules from Task 1.
- CLI:
  `build_universe.py --rustty PATH --ghostty PATH --output FILE`
- Emits one sorted JSON object per `git ls-files` path with `repo`, `revision`,
  `path`, `byte_length`, `line_count`, `sha256`, `content_kind`, and initial
  `scope_class: semantic_required`.
- Binary detection is byte-based: NUL in the first 8192 bytes or strict UTF-8
  decode failure. Detection never drops a file.

- [ ] **Step 1: Write failing universe tests**

```python
def test_inventory_is_sorted_and_includes_text_binary_empty(tmp_path):
    repo = make_git_fixture(tmp_path, {
        "z.txt": b"z\n", "a.bin": b"\x00\xff", "empty": b"",
    })
    records = build_records(repo, "ghostty")
    assert [r["path"] for r in records] == ["a.bin", "empty", "z.txt"]
    assert [r["content_kind"] for r in records] == ["binary", "empty", "text"]

def test_inventory_revision_is_head_commit(tmp_path):
    repo = make_git_fixture(tmp_path, {"a": b"x\n"})
    assert build_records(repo, "rustty")[0]["revision"] == git_head(repo)
```

- [ ] **Step 2: Run the universe tests and confirm failure**

Run: `python3 -m unittest .port/codex/line-audit/tests/test_build_universe.py -v`  
Expected: nonzero because `build_records` and the CLI do not exist.

- [ ] **Step 3: Implement deterministic Git inventory generation**

```python
def tracked_paths(repo: Path) -> list[str]:
    raw = subprocess.check_output(["git", "ls-files", "-z"], cwd=repo)
    return sorted(p.decode("utf-8", "surrogateescape") for p in raw.split(b"\0") if p)

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

- [ ] **Step 4: Run unit tests and determinism check**

Run: `python3 -m unittest .port/codex/line-audit/tests/test_build_universe.py -v`  
Expected: all tests pass.  
Run the CLI twice to two temporary files and compare them with `cmp`; expected:
exit 0.

- [ ] **Step 5: Generate the real pinned universe and record totals**

Run:
`python3 .port/codex/line-audit/scripts/build_universe.py --rustty /Users/gael/projects/rustty --ghostty /Users/gael/projects/ghostty --output .port/codex/line-audit/universe.jsonl`  
Expected: both baseline revisions match the design and every tracked path is
represented.

---

### Task 3: Implement the fail-closed coverage and review verifier

**Files:**

- Create: `.port/codex/line-audit/scripts/verify_ledger.py`
- Create: `.port/codex/line-audit/tests/test_verify_ledger.py`
- Create: `.port/codex/line-audit/tests/fixtures/ledger/`

**Interfaces:**

- CLI:
  `verify_ledger.py --universe FILE --ledger DIR --reviews DIR --findings DIR
  [--allow-incomplete]`
- Exit 0 only for a complete valid universe. `--allow-incomplete` still validates
  authored records but exits 2 and prints exact uncovered/pending/review totals.
- Diagnostics use stable codes including `E_UNCOVERED`, `E_OVERLAP`,
  `E_STALE_HASH`, `E_PENDING`, `E_REVIEW_MISSING`, `E_SELF_REVIEW`,
  `E_EXCLUSION_EVIDENCE`, `E_FINDING_LINK`, and `E_HIGH_RISK_PROOF`.

- [ ] **Step 1: Write the clean fixture and mutation tests**

```python
MUTATIONS = {
    "uncovered_first": remove_line_coverage(1),
    "uncovered_middle": remove_line_coverage(2),
    "uncovered_last": remove_line_coverage(3),
    "overlap": duplicate_range(2, 3),
    "stale_hash": replace_range_hash("0" * 64),
    "pending": replace_disposition("pending"),
    "missing_review": delete_review(),
    "self_review": replace_secondary_reviewer("primary"),
    "gap_without_finding": remove_finding_link(),
    "high_risk_without_control": clear_control_evidence(),
}

def test_each_mutation_is_rejected(self):
    for name, mutate in MUTATIONS.items():
        with self.subTest(name=name):
            result = verify_fixture(mutate(clean_fixture()))
            self.assertNotEqual(result.returncode, 0)
```

- [ ] **Step 2: Run verifier tests and confirm failure**

Run: `python3 -m unittest .port/codex/line-audit/tests/test_verify_ledger.py -v`  
Expected: nonzero because the verifier does not exist.

- [ ] **Step 3: Implement exact coverage, hash, evidence, and review checks**

```python
for file_record in universe:
    ranges = sorted(primary_by_file[file_key(file_record)], key=lambda r: r["start"])
    expected = 1
    for record in ranges:
        if record["start"] != expected:
            error("E_UNCOVERED", file_record, expected, record["start"] - 1)
        expected = record["end"] + 1
    if expected != file_record["line_count"] + 1:
        error("E_UNCOVERED", file_record, expected, file_record["line_count"])
```

Implement empty/binary file records separately, recompute raw range hashes from
the pinned repository, validate counterpart coordinates, require independent
acceptable secondary verdicts, and cross-link finding Markdown front matter.

- [ ] **Step 4: Run every verifier mutation test**

Run: `python3 -m unittest .port/codex/line-audit/tests/test_verify_ledger.py -v`  
Expected: the clean fixture passes; every named mutation fails with its expected
diagnostic.

- [ ] **Step 5: Run the verifier on the real empty ledger**

Run:
`python3 .port/codex/line-audit/scripts/verify_ledger.py --universe .port/codex/line-audit/universe.jsonl --ledger .port/codex/line-audit/ledger --reviews .port/codex/line-audit/reviews --findings .port/codex/line-audit/findings --allow-incomplete`  
Expected: exit 2 and nonzero uncovered totals for both repositories. A green or
zero-universe result fails this task.

---

### Task 4: Implement batch selection and derived progress reporting

**Files:**

- Create: `.port/codex/line-audit/scripts/next_batch.py`
- Create: `.port/codex/line-audit/scripts/render_progress.py`
- Create: `.port/codex/line-audit/tests/test_next_batch.py`
- Create: `.port/codex/line-audit/batches.json`
- Generate: `.port/codex/line-audit/PROGRESS.md`

**Interfaces:**

- `next_batch.py --universe FILE --ledger DIR --max-lines N --domain PREFIX`
  emits stable uncovered ranges without skipping difficult files.
- `render_progress.py` derives all counts from universe, ledgers, and reviews.
- `batches.json` stores `id`, `domain`, `oracle_paths`, `rustty_paths`, `owner`,
  `reviewer`, `state`, `semantic_lines`, and `dependencies`.

- [ ] **Step 1: Write failing selection tests**

```python
def test_selector_returns_first_uncovered_lines_in_path_order():
    result = select(universe_three_files(), ledger_covering_middle_file(), 1000, None)
    assert [(r.path, r.start, r.end) for r in result] == [
        ("a.zig", 1, 3), ("c.zig", 1, 2),
    ]

def test_progress_is_derived_not_hand_entered():
    report = render_progress(universe_lines=10, primary_lines=4, reviewed_lines=2)
    assert "40.00% primary" in report
    assert "20.00% secondary" in report
```

- [ ] **Step 2: Run selector tests and confirm failure**

Run: `python3 -m unittest .port/codex/line-audit/tests/test_next_batch.py -v`  
Expected: nonzero because selector/report functions do not exist.

- [ ] **Step 3: Implement deterministic selection and reporting**

```python
def uncovered_intervals(line_count: int, covered: list[tuple[int, int]]):
    cursor = 1
    for start, end in sorted(covered):
        if cursor < start:
            yield cursor, start - 1
        cursor = max(cursor, end + 1)
    if cursor <= line_count:
        yield cursor, line_count
```

The batch selector must report an oversized semantic unit rather than truncate
it without explanation. Progress distinguishes physical, primary semantic,
secondary semantic, exclusions, pending, invalidated, and finding-linked lines.

- [ ] **Step 4: Run tests and render the initial real progress report**

Run: `python3 -m unittest .port/codex/line-audit/tests/test_next_batch.py -v`  
Expected: all tests pass.  
Run `render_progress.py` on the real universe; expected: 100% physical inventory
and 0% semantic ledger coverage before batch execution.

---

### Task 5: Classify the physical universe without hiding semantic work

**Files:**

- Create: `.port/codex/line-audit/scripts/classify_universe.py`
- Create: `.port/codex/line-audit/tests/test_classify_universe.py`
- Modify: `.port/codex/line-audit/universe.jsonl`
- Create: `.port/codex/line-audit/evidence/scope/`

**Interfaces:**

- Classification rules may propose but never finalize exclusions based solely
  on path names.
- Each non-semantic file/range stores `classification_reason` and
  `classification_evidence`.
- Mixed files remain `semantic_required` until line-level ranges split their
  macOS-only parts.

- [ ] **Step 1: Write false-exclusion tests**

```python
def test_darwin_named_file_with_linux_consumer_remains_semantic():
    record = file_record("src/darwin_adapter.zig")
    evidence = consumer_evidence("src/linux/main.zig", "darwin_adapter")
    assert classify(record, evidence).scope_class == "semantic_required"

def test_vendor_file_modified_from_reference_is_not_identical():
    record = file_record("vendor/x.c", sha256="a")
    assert classify_vendor(record, upstream_sha256="b").scope_class == "semantic_required"
```

- [ ] **Step 2: Run classification tests and confirm failure**

Run: `python3 -m unittest .port/codex/line-audit/tests/test_classify_universe.py -v`  
Expected: nonzero because classification functions do not exist.

- [ ] **Step 3: Implement proposal rules and evidence schema**

Use Git attributes, extension/content, known generated manifests, dependency
metadata, target cfg searches, and exact hashes. Emit `needs_manual_scope_review`
for ambiguous files rather than selecting an exclusion.

- [ ] **Step 4: Manually review every proposed non-semantic classification**

For each proposal, inspect generators/upstream hashes or target consumers and
write evidence under `evidence/scope/`. Reclassify any mixed or uncertain file
as `semantic_required`.

- [ ] **Step 5: Verify scope completeness**

Run classification tests and the real verifier. Expected: tests pass; verifier
remains exit 2 because semantic ledgers are incomplete but reports zero files
with missing scope classification.

---

### Task 6: Execute the first four independent primary-review domains

**Files:**

- Create: `.port/codex/line-audit/ledger/build-distribution-001.jsonl`
- Create: `.port/codex/line-audit/ledger/gtk-bsd-001.jsonl`
- Create: `.port/codex/line-audit/ledger/platform-targets-001.jsonl`
- Create: `.port/codex/line-audit/ledger/vt-renderer-001.jsonl`
- Create evidence below corresponding `.port/codex/line-audit/evidence/<batch>/`
- Modify only batch state in `.port/codex/line-audit/batches.json`

**Interfaces:**

- Each agent owns exactly one ledger/evidence subtree and does not edit the
  canonical findings or another batch.
- Each range conforms to Task 1 schema and cites raw range hashes produced by
  the helper.
- Candidate findings use IDs `C-<batch-id>-NNN` until cross-review.

- [ ] **Step 1: Assign `build-distribution-001`**

Review first-party build configuration, CI target matrices, packaging, install,
desktop resources, examples, and public build entry points line-by-line. Pair
every Ghostty range with Cargo/xtask/CI/dist behavior and reverse-audit the
corresponding Rustty lines.

- [ ] **Step 2: Assign `gtk-bsd-001`**

Review GTK application/runtime selection, lifecycle, windows, surfaces,
resources, Wayland/X11, FreeBSD gates, clipboard, DnD, notifications, and
portals line-by-line.

- [ ] **Step 3: Assign `platform-targets-001`**

Review Windows OS/ConPTY/page allocation, WASM/browser builds and ABI bridges,
FreeBSD event-loop and renderer selection, and target dependency gates
line-by-line.

- [ ] **Step 4: Audit `vt-renderer-001` locally**

Review the next deterministic uncovered VT C ABI, allocator, renderer/OpenGL,
Kitty/background/custom-shader/scrollbar/overlay ranges and reverse Rustty
counterparts line-by-line.

- [ ] **Step 5: Validate authored primary records without claiming completion**

Run the verifier with `--allow-incomplete`. Expected: exit 2, zero schema/hash/
overlap errors for authored ranges, and reduced uncovered totals.

---

### Task 7: Independently cross-review every first-wave semantic range

**Files:**

- Create: `.port/codex/line-audit/reviews/build-distribution-001.jsonl`
- Create: `.port/codex/line-audit/reviews/gtk-bsd-001.jsonl`
- Create: `.port/codex/line-audit/reviews/platform-targets-001.jsonl`
- Create: `.port/codex/line-audit/reviews/vt-renderer-001.jsonl`
- Create: `.port/codex/line-audit/findings/<promoted-id>.md` for upheld gaps
- Modify: `.port/codex/FINDINGS.md` only after canonical promotion

**Interfaces:**

- A reviewer cannot review a batch they authored.
- Every primary semantic range receives one acceptable review or returns to the
  author with `needs_more_evidence`/`rejected`.
- Promoted findings retain bidirectional links to all affected range IDs and
  deduplicate against F-001 through F-020.

- [ ] **Step 1: Rotate the four batches to different reviewers**

Use a derangement: build reviewer checks GTK, GTK reviewer checks platform,
platform reviewer checks VT/renderer, and the primary agent checks build. No
author/reviewer identity may match.

- [ ] **Step 2: Require source/evidence inspection per review record**

Each review records checked counterpart ranges, attempted falsification,
counterevidence, corrected disposition/severity, and verdict.

- [ ] **Step 3: Return non-upheld records for correction**

Authors add evidence or change disposition; reviewers issue a new superseding
record. History remains append-only by `review_revision`.

- [ ] **Step 4: Promote and deduplicate upheld gaps**

Create detailed finding documents and update the canonical index without
double-counting an existing root cause.

- [ ] **Step 5: Run the verifier**

Expected: exit 2 because the global universe remains incomplete; first-wave
semantic records have zero missing/invalid secondary reviews.

---

### Task 8: Iterate all remaining semantic domains

**Files:**

- Create additional `.port/codex/line-audit/ledger/<domain>-NNN.jsonl`
- Create matching review, evidence, and finding files
- Modify: `.port/codex/line-audit/batches.json`
- Regenerate: `.port/codex/line-audit/PROGRESS.md`

**Interfaces:**

- The batch selector, not reviewer preference, determines uncovered work inside
  each domain.
- Required domains are:
  `terminal-parser-protocols`, `grid-screen-selection`,
  `pagelist-compression-search`, `core-renderer-inspector`,
  `termio-os-subprocess`, `config-cli-input`, `font-shaping-discovery`,
  `shell-terminfo-crash`, `headers-examples-bindings`,
  `tests-fuzz-assertions`, `resources-shaders-generated`, and
  `rustty-reverse-remainder`.

- [ ] **Step 1: Fill at most four independent primary batches concurrently**

Each batch covers 300–1,000 semantic lines or one indivisible larger semantic
unit and records every physical line in its assigned interval.

- [ ] **Step 2: Validate primary batches in partial mode**

Reject schema, stale-hash, gap, overlap, weak-evidence, and invalid-counterpart
errors before secondary assignment.

- [ ] **Step 3: Rotate every batch to an independent reviewer**

No semantic range advances without `upheld` or `upheld_with_correction`.

- [ ] **Step 4: Reconcile findings and regenerate progress**

Progress must monotonically reduce valid uncovered lines; invalidated lines
return to the uncovered pool.

- [ ] **Step 5: Repeat until the selector emits no uncovered interval**

Do not stop at file, function, symbol, or test-count parity. Stop batch creation
only when both repository views have zero uncovered physical lines and zero
unreviewed semantic lines.

---

### Task 9: Final adversarial completion audit

**Files:**

- Create: `.port/codex/line-audit/FINAL_VERIFICATION.md`
- Regenerate: `.port/codex/line-audit/PROGRESS.md`
- Update: `.port/codex/FINDINGS.md`, `.port/codex/findings.json`, and
  `.port/codex/VERIFICATION.md`

**Interfaces:**

- Consumes the full pinned universe, all primary/review ledgers, evidence, and
  canonical findings.
- Produces the only completion evidence accepted by this plan.

- [ ] **Step 1: Rebuild the universe independently and compare byte-for-byte**

Generate to a temporary file and use `cmp` against `universe.jsonl`. Expected:
exit 0 and pinned repository revisions unchanged.

- [ ] **Step 2: Run all audit-tool unit and mutation tests**

Run: `python3 -m unittest discover .port/codex/line-audit/tests -v`  
Expected: every clean test passes and every mutation is observed to make its
fixture fail.

- [ ] **Step 3: Run the full verifier without partial mode**

Run the Task 3 CLI without `--allow-incomplete`. Expected: exit 0, zero
uncovered/pending/invalidated ranges, 100% independent review, and all findings
bidirectionally linked.

- [ ] **Step 4: Re-run behavioral and target evidence for every finding**

Use each finding's recorded verification command. Expected results must match
the recorded observations; otherwise invalidate and reopen its ranges.

- [ ] **Step 5: Verify repository boundaries and worktrees**

Run `git status --short --untracked-files=no` in Rustty, full status in Ghostty,
filter Rustty untracked paths outside `.port/codex/`, and inspect `git worktree
list --porcelain`. Expected: no product changes, no Ghostty changes, no audit
worktree remains.

- [ ] **Step 6: Publish final derived counts and completion status**

Write exact file/line totals, scope-class totals, primary/secondary totals,
finding counts, rejected candidates, environmental limits, and fresh command
outputs to `FINAL_VERIFICATION.md`. Mark the persistent goal complete only after
all preceding evidence is current and green.

