# Exhaustive Line-Level Parity Audit Design

**Date:** 2026-07-31  
**Rustty baseline:** `fa02b36cc21eadcd8eaa8f68ae0bf1ec759ddc83`  
**Ghostty baseline:** `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`

The Rustty baseline advanced during Task 2 from the earlier audit revision
`5243303fbb2f2cfd0832edfb8fa49d72ce322d81`. The first generated universe was
immediately invalidated rather than credited. This baseline records the new
authoritative `HEAD`; later drift must trigger the same invalidation rule.

## Objective

Create a mechanically complete, independently reviewed audit of every tracked
line in the Rustty port and the Ghostty oracle that can affect non-macOS or
shared behavior. The audit must either prove the reviewed Rustty behavior is
equivalent at that line range or produce a verified parity finding. No line may
disappear through a directory skip, filename heuristic, stale source revision,
or undocumented exception.

The audit documents gaps; it does not fix Rustty or modify Ghostty. All audit
artifacts live under `.port/codex/`.

## Why the existing coverage is insufficient

The current live file audit covers 498 `src/**/*.zig` files while explicitly
skipping `src/build/` and `src/bench/`. It proves that each included file has a
plausible Rust counterpart or written non-port, but it does not prove that each
line, branch, assertion, error path, or side effect was reviewed.

The current Ghostty checkout has 5,781 tracked files and approximately 516,218
text lines. Important surfaces outside the old file inventory include build
logic, public headers, C/C++ helpers, examples, CI matrices, packaging,
resources, shaders, fuzz targets, and generated interfaces. A complete parity
audit must also examine Rustty-only lines because a port-specific no-op,
fallback, weakened assertion, or platform gate has no oracle line from which a
one-way inventory could discover it.

## Scope

### Physical universe

The inventory contains every path returned by `git ls-files` at the pinned
revision of both repositories.

- Text files are decoded losslessly where possible and split using physical
  line boundaries. Empty files receive a file-level record.
- Binary files receive a file-level content-hash record because they have no
  meaningful text lines.
- Every file record stores repository, revision, path, byte length, line count,
  whole-file SHA-256, detected content kind, and scope class.
- The inventory is regenerated only by an explicit baseline update. Any source
  hash drift invalidates the affected review records rather than silently
  carrying them forward.

### Semantic-review universe

Every first-party line that is executable, consumed, tested, built, packaged,
or used to define behavior on Linux, FreeBSD, Windows, WASM/browser, embedded C
ABI, or platform-neutral/shared code requires semantic parity review. This
includes:

- production source and inline platform branches;
- tests, test inputs, expected values, assertions, skips, and target gates;
- build scripts, dependency selection, feature defaults, generated-code rules,
  CI target matrices, packaging, and install logic;
- public headers, examples, foreign-language bindings, and ABI metadata;
- GTK resources, shaders, shell integration, terminfo, configuration data, and
  other runtime-consumed text assets;
- comments that state invariants, unsupported behavior, port status, safety
  requirements, or expected error semantics.

Blank lines, formatting-only delimiters, and prose without a behavioral claim
are still physically covered. They may share a range with the surrounding
semantic unit and do not require invented behavior.

### Explicitly dispositioned classes

Nothing is silently excluded. Lines outside full semantic comparison receive
one of these dispositions with evidence:

1. `pure_macos_out_of_scope`: the line is reachable only from a macOS/Darwin
   product path. Mixed shared/macOS files are split so shared and non-macOS
   ranges remain semantic-review required. References to macOS code from a
   non-macOS selector or interface remain in scope.
2. `vendored_identical`: third-party source matches the pinned dependency
   content or Ghostty copy. Review covers hashes, patches, build flags, exported
   interfaces, and non-macOS integration boundaries. Any modified/forked line
   becomes semantic-review required.
3. `generated_reproducible`: output is proven reproducible from reviewed inputs
   and generator code. Review covers provenance, regeneration, committed-output
   drift, and every consumed interface. Hand-edited or unreproducible output is
   semantic-review required.
4. `binary_asset`: no text line exists. Review covers identity/provenance and
   non-macOS consumers where the asset can affect behavior.
5. `documentation_nonbehavioral`: prose that defines no build, API, test,
   support, or behavioral contract. Support tables, examples, documented
   defaults, and platform promises do not qualify for this disposition.

The verifier rejects an out-of-scope range without a concrete classification
reason and evidence path.

## Review unit and ledger model

The unit of human review is a semantic line range, normally one declaration,
function, branch, test, data table, resource block, or build rule. A large file
is split into ranges small enough to understand independently. Ranges may not
overlap and must cover every physical line exactly once.

Each primary-review record contains:

- stable batch ID and range ID;
- repository, revision, path, start line, end line, and SHA-256 of the exact raw
  range bytes;
- scope class and semantic-risk tags;
- Ghostty and Rustty counterpart ranges, including one-to-many and many-to-one
  mappings;
- disposition;
- concise behavioral contract;
- equivalence or divergence analysis;
- reachable non-macOS producer/consumer trace;
- verification command, fixture, artifact, or source proof;
- observed result and environmental limitation;
- candidate or canonical finding IDs;
- primary reviewer identity and review timestamp.

Allowed semantic dispositions are:

- `exact_equivalent`: mechanically identical after a documented rename or
  representation normalization;
- `semantic_equivalent`: different implementation with the same verified
  contract and observable behavior;
- `port_specific_valid`: Rustty-only implementation necessary for the Rust
  architecture without narrowing Ghostty behavior;
- `oracle_not_applicable`: behavior is genuinely unreachable or unsupported on
  the audited targets in both projects;
- `intentional_divergence`: a user-authorized product difference, cited to its
  authority and tested consequence;
- `parity_gap`: verified functional, platform, ABI, build, resource, or test
  mismatch;
- `pending`: not yet reviewed or evidence insufficient.

`unsupported`, `TODO`, `DEFERRED`, a passing unrelated test, source similarity,
and line-count parity are leads, not valid final dispositions.

## Bidirectional mapping

The ledger has two complete views:

1. **Oracle view:** every Ghostty line maps to Rustty behavior or an explicit
   disposition. This catches absent or narrowed functionality.
2. **Port view:** every Rustty line maps back to its Ghostty contract, a valid
   Rust-specific adaptation, or a finding. This catches invented fallbacks,
   inert features, swallowed errors, weakened tests, and dead compatibility
   seams that a Ghostty-driven pass cannot see.

Split and merged ports are represented by explicit range arrays. A filename or
PORT NOTE is useful discovery evidence but never sufficient equivalence proof.

## Risk model and evidence requirements

Every semantic range receives manual primary review and independent secondary
review. Risk determines how much executable evidence is required, not whether a
range is reviewed.

High-risk tags include platform `cfg`/target selection, unsafe/raw-pointer/FFI,
allocator ownership, error conversion, state machines, parser dispatch,
compression, rendering, concurrency, subprocess/environment behavior, public
ABI, serialization, test skips, and assertion weakening.

- High-risk `semantic_equivalent` and every `parity_gap` require a focused
  behavioral probe, target compiler check, artifact inspection, or exact
  source-derived proof plus a negative/control case.
- Lower-risk equivalence may use paired source proof when executable behavior is
  unavailable, but must state the limitation and trace a live consumer.
- Exact generated/vendor matches require full content hashes and reviewed build
  integration, not line-by-line paraphrase.
- No finding is canonical until an independent reviewer attempts to falsify its
  reachability, oracle contract, target scope, severity, and proposed
  verification.

## Independent review

Every completed semantic range receives a secondary record from a reviewer
other than the primary reviewer. The secondary reviewer may return:

- `upheld`;
- `upheld_with_correction`;
- `rejected` with counterevidence;
- `needs_more_evidence`.

The verifier treats all but the first two as incomplete. Reviewers must inspect
the underlying source ranges and evidence; merely endorsing the primary prose
is invalid. Exact-hash vendor/generated ranges may be batch-reviewed, but their
integration boundaries receive ordinary semantic secondary review.

## Batch workflow

The audit runs continuously in bounded subsystem batches:

1. Generate or validate the pinned physical universe.
2. Select the next uncovered batch, normally 300–1,000 semantic lines grouped
   by a coherent subsystem or call path.
3. Read every source line in the batch plus the required producers, consumers,
   tests, and paired implementation.
4. Create primary range records; leave uncertainty as `pending` rather than
   guessing.
5. For each candidate mismatch, devise and execute a falsification/control
   strategy before promotion.
6. Assign the batch to an independent reviewer, who records a verdict for every
   semantic range and adversarially reviews every candidate.
7. Reconcile corrections, update canonical findings, and run the global
   verifier.
8. Choose the next uncovered or invalidated batch and repeat.

Independent batches may run in parallel when they do not share ledger files.
One agent owns one batch file; the primary agent integrates reviews and the
canonical catalog to prevent concurrent-write ambiguity.

## Artifact layout

All paths are below `.port/codex/line-audit/`:

- `universe.jsonl`: immutable pinned file inventory for both repositories;
- `batches.json`: batch ownership, state, line totals, and dependencies;
- `ledger/<batch-id>.jsonl`: primary range records;
- `reviews/<batch-id>.jsonl`: independent secondary verdicts;
- `evidence/<batch-id>/`: command output, reduced fixtures, and artifact data;
- `findings/<finding-id>.md`: detailed promoted findings;
- `scripts/build_universe.py`: generate deterministic file metadata;
- `scripts/verify_ledger.py`: enforce hashes, coverage, schema, and review
  invariants;
- `scripts/next_batch.py`: report uncovered and invalidated ranges without
  silently prioritizing easy files;
- `PROGRESS.md`: generated human summary by repository, subsystem, scope class,
  risk, primary review, secondary review, and remaining lines.

The existing `.port/codex/FINDINGS.md` remains the canonical deduplicated
finding catalog until the line audit produces a replacement with equal or
stronger evidence.

## Verifier invariants

The global verifier fails unless all applicable conditions hold:

1. Repository revisions equal the pinned baselines.
2. Every tracked file appears exactly once in the physical universe.
3. Every text line is covered by exactly one primary range: no gaps, overlaps,
   reversed ranges, or ranges beyond EOF.
4. Every file and range hash matches current pinned content.
5. Every path and counterpart range exists at the cited revision.
6. Every semantic range has a final non-`pending` disposition.
7. Every semantic range has an independent acceptable secondary verdict.
8. Every exclusion has a permitted class, specific rationale, and evidence.
9. Every `parity_gap` resolves to a detailed finding; every finding resolves
   back to one or more ledger ranges.
10. Every finding records oracle contract, Rustty behavior, non-macOS impact,
    verification, observed result, adversarial counter-check, severity,
    confidence, and limitations.
11. Every high-risk equivalence records the required proof and control.
12. Summary counts are derived from ledger data rather than hand-maintained.
13. No audit output exists outside `.port/codex/`, no Rustty/Ghostty product
    file changed, and no temporary audit worktree remains.

The verifier supports partial progress, but its result must say `INCOMPLETE` and
exit nonzero until every completion invariant is satisfied. It may never report
green merely because all currently authored ledger files are internally valid.

## Testing the audit machinery

The tooling is tested with mutation/self-tests that prove it rejects:

- a deleted inventory file;
- an uncovered first, middle, and last line;
- overlapping and out-of-bounds ranges;
- a stale file or range hash;
- a missing counterpart path;
- an invalid or `pending` disposition;
- a semantic range without secondary review;
- self-review by the primary reviewer;
- an exclusion without evidence;
- a parity gap without a finding and a finding without a range;
- a high-risk equivalence without proof/control;
- a zero-file or zero-line universe;
- a summary whose counts differ from the ledger.

Each self-test must demonstrate red behavior before the clean fixture is allowed
to demonstrate green. The real repository remains incomplete until all ledger
batches are finished.

## Completion criteria

The line audit is complete only when:

- 100% of tracked files in both pinned repositories are physically
  dispositioned;
- 100% of first-party non-macOS/shared semantic lines have primary review;
- 100% of those semantic lines have independent secondary review;
- every vendored/generated/macOS-only/binary line or file has a verified scope
  disposition and all non-macOS integration boundaries are semantically
  reviewed;
- no `pending`, invalidated, uncovered, overlapping, or stale range remains;
- every discovered mismatch is either rejected with counterevidence or present
  in the canonical finding catalog;
- the full verifier and all mutation self-tests pass against the pinned source;
- final repository-boundary checks prove no forbidden changes or leftover audit
  worktrees.

This process can prove that every line received the defined audit and that all
discovered gaps are accounted for. It does not make the logically impossible
claim that finite review can prove the absence of every latent bug for every
input; uncertainty is represented as incomplete evidence rather than silently
converted into parity.
