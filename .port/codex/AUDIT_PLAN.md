# Rustty Non-macOS Port Parity Audit Plan

> **For agentic workers:** Treat both `/Users/gael/projects/rustty` and
> `/Users/gael/projects/ghostty` as read-only. Write only beneath
> `/Users/gael/projects/rustty/.port/codex/`. Do not fix parity gaps.

**Goal:** Adversarially compare the entire Rustty port with the pinned Ghostty
reference, identify non-macOS functional and test-contract gaps, and report only
claims backed by reproducible evidence.

**Reference:** `/Users/gael/projects/ghostty` at its checked-out revision, plus
the Rustty repository's declared oracle revision and parity inventory. Record
the exact revisions used in the final report.

**Scope:** Linux, FreeBSD, Windows, WebAssembly/browser, embedded/C ABI, and
platform-neutral code that those targets consume. macOS execution may be used
as a practical proving ground, but macOS-only product gaps are excluded.

**Output:** Evidence, scripts, raw logs, agent reports, and the final finding
catalog all live in `.port/codex/`. Production code is never changed.

## Evidence standard

A reportable finding must include:

1. the Ghostty behavior or contract, cited to exact files and lines;
2. the Rustty behavior, cited to exact files and lines;
3. a concrete user-visible or contract-level consequence on a non-macOS target;
4. a verification method and its observed result;
5. an adversarial counter-check that tries to disprove the finding;
6. scope, severity, confidence, and any environmental limitation.

Textual similarity, TODO markers, test-count parity, or an uncompiled target are
leads, not findings. Intentional identity changes and documented non-ports must
be separated from defects. A suspected issue that cannot be verified is listed
as an unresolved lead, never promoted to the verified catalog.

## Workstreams

### 1. Freeze the audit universe and baseline

- [ ] Record both repository revisions, toolchain versions, installed targets,
      and existing worktree state.
- [ ] Map Ghostty source, tests, build targets, packaging, and supported-platform
      declarations to Rustty crates and artifacts.
- [ ] Read the existing parity inventories, deferral reports, reachability
      reports, and test-contract tooling; do not assume their conclusions.
- [ ] Establish finding IDs, severity definitions, and evidence templates.

### 2. Build, target, distribution, and dependency parity

- [ ] Compare Ghostty build target matrices with Cargo features, target gates,
      native dependencies, generated resources, packaging, and install layout.
- [ ] Verify feasible host builds and compile/check non-host targets when the
      toolchain is installed; distinguish source analysis from execution.
- [ ] Audit Linux, FreeBSD, Windows, and WASM target-specific branches for
      missing modules, unconditional host dependencies, and inert features.
- [ ] Inspect vendored or re-homed dependencies for narrowed functionality that
      is reachable from Ghostty product paths.

### 3. Linux and FreeBSD application-runtime parity

- [ ] Compare GTK application/runtime, window, surface, clipboard, DnD, input,
      notifications, portals, desktop integration, and Wayland/X11 behavior.
- [ ] Trace every reference callback and action through Rustty to a live runtime
      consumer; detect value-only types or handlers with no execution path.
- [ ] Verify executable entry points and packaging resources where feasible.
- [ ] Audit FreeBSD-specific compile/runtime branches separately from Linux.

### 4. Windows and browser/WASM parity

- [ ] Compare Windows command execution, ConPTY/terminal I/O, filesystem and
      environment behavior, renderer/font dependencies, build artifacts, and
      any Ghostty platform tests.
- [ ] Compare WASM exports, JS/browser glue, canvas/WebGL rendering, memory/table
      assumptions, examples, and generated bindings.
- [ ] Compile or inspect emitted symbols/artifacts where possible; record any
      target/toolchain blockers as limitations rather than findings.

### 5. Shared runtime and rendering stack

- [ ] Audit `core`, `termio`, `os`, `font`, `font-shaper`, `renderer`, `input`,
      `config`, `cli`, `terminfo`, `crash`, and `inspector` against reachable
      Ghostty behavior on non-macOS platforms.
- [ ] Search for placeholders, stubs, swallowed errors, simplified state
      machines, no-op callbacks, fake handles, unsupported returns, and tests
      that exercise only seams rather than shipped behavior.
- [ ] Compare configuration fields, defaults, validation, actions, keybindings,
      shell integration, locale/i18n, resources, and generated output.
- [ ] Test suspected gaps with focused reproducers or existing differential
      harnesses, then devise a negative/control case to challenge each result.

### 6. Terminal engine, public ABI, and protocol parity

- [ ] Differentially audit terminal parsing/execution, grids, selection, modes,
      OSC/DCS/APC, kitty graphics, Unicode, regex, compression, and formatting.
- [ ] Compare public C headers, symbols, layouts, enum values, ownership, error
      behavior, WASM exports, examples, and downstream build integration.
- [ ] Sample beyond existing corpora using generated/adversarial inputs, seeded
      runs, and minimized counterexamples for every divergence.
- [ ] Confirm whether each divergence is a defect, intentional rename, undefined
      behavior, oracle evolution, or harness artifact.

### 7. Test and assertion fidelity

- [ ] Recompute Ghostty-to-Rustty test mapping independently of repository gates.
- [ ] Detect missing tests, ignored/conditional tests, weakened assertions,
      reduced input domains, deleted failure-path checks, snapshot drift, and
      Rust tests that count multiple times without equivalent coverage.
- [ ] Inspect tests excluded on non-macOS targets and code that only compiles or
      runs under `cfg(test)`.
- [ ] Mutation-test selected high-risk areas to prove assertions fail when the
      behavior is deliberately perturbed; use a temporary worktree only when a
      code edit is required and remove it afterward.

### 8. Cross-review and synthesis

- [ ] Independently re-check every candidate finding against the opposite repo
      and current reachability.
- [ ] Attempt to falsify each claim with alternative configuration, feature,
      platform gate, test, or runtime path.
- [ ] De-duplicate root causes while preserving distinct affected platforms and
      user consequences.
- [ ] Publish `FINDINGS.md`, `UNRESOLVED_LEADS.md`, `COVERAGE.md`, and a machine-
      readable finding index with exact evidence paths.
- [ ] Confirm no files outside `.port/codex/` changed and no temporary worktrees
      remain.

## Finding severities

- **P0:** data loss, security boundary failure, or broad product unusability.
- **P1:** major supported feature absent/broken or a supported target cannot be
  built/launched through its intended path.
- **P2:** material behavior, protocol, configuration, or integration mismatch.
- **P3:** narrow edge-case mismatch or meaningful test-contract weakness without
  a demonstrated common-path failure.
- **P4:** verified diagnostic, packaging, documentation-generated, or low-impact
  parity defect.

## Artifact layout

- `agents/`: independent workstream reports.
- `evidence/`: raw command output and reduced repro inputs.
- `scripts/`: reusable read-only analyzers and differential reproducers.
- `findings/`: one detailed Markdown file per verified finding.
- `FINDINGS.md`: severity-ordered index and executive synthesis.
- `UNRESOLVED_LEADS.md`: plausible but unverified issues and exact blockers.
- `COVERAGE.md`: reviewed surface, methods, executions, and known blind spots.
