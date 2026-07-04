export const meta = {
  name: 'phase0-reference-oracle',
  description: 'Phase 0: build reference, run full test suite, extract fidelity inventories, assemble golden corpora, stand up differential harness, capture BASELINE.json, emit PORT_PLAN + DEFINITION_OF_PARITY',
  phases: [
    { title: 'Build & Test', detail: 'serial: build all reference artifacts + run full Zig test suite' },
    { title: 'Inventory & Plan', detail: 'parallel read-only: test/comment/assertion + parity-surface inventories, golden corpora, diff harness, port plan, definition of parity' },
    { title: 'Baseline', detail: 'isolated: throughput/size/startup -> BASELINE.json' },
    { title: 'Critic', detail: 'adversarial completeness audit of all Phase-0 deliverables' },
  ],
}

const G = '/Users/gael/projects/ghostty'        // REFERENCE, READ-ONLY
const R = '/Users/gael/projects/rustty'
const OUT = R + '/.port/phase-0'
const RULES = 'Obey ' + R + '/.port/PORT_RULES.md and ' + R + '/docs/superpowers/specs/2026-06-21-rustty-crate-structure-design.md. The ghostty tree at ' + G + ' is the READ-ONLY reference pinned at 4749c4e (Zig 0.15.2) — NEVER modify it. Write ALL outputs under ' + OUT + '. Build/run reliably: for long commands use run_in_background and poll. Report exact numbers and any blocker precisely; never fabricate. '

const BUILD_SCHEMA = {
  type: 'object', additionalProperties: true,
  required: ['built_ok','test_total','test_passed','test_failed','blockers'],
  properties: {
    built_ok: { type: 'boolean' },
    commands_run: { type: 'array', items: { type: 'string' } },
    test_total: { type: 'integer' }, test_passed: { type: 'integer' },
    test_failed: { type: 'integer' }, test_skipped: { type: 'integer' },
    lib_vt_test_summary: { type: 'string' },
    artifacts: { type: 'object', additionalProperties: true },
    cvt_examples_built: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
    blockers: { type: 'array', items: { type: 'string' } },
  },
}
const INV_SCHEMA = {
  type: 'object', additionalProperties: true,
  required: ['test_block_count','assertion_total','files_with_tests','inventory_path'],
  properties: {
    test_block_count: { type: 'integer' }, test_named_count: { type: 'integer' },
    test_anon_count: { type: 'integer' }, assertion_total: { type: 'integer' },
    files_with_tests: { type: 'integer' }, inventory_path: { type: 'string' },
    assertion_breakdown: { type: 'object', additionalProperties: true },
    method: { type: 'string' }, discrepancies: { type: 'array', items: { type: 'string' } },
  },
}
const BASELINE_SCHEMA = {
  type: 'object', additionalProperties: true,
  required: ['baseline_json_path','binary_sizes','methodology'],
  properties: {
    benchmarks: { type: 'array', items: { type: 'object', additionalProperties: true } },
    binary_sizes: { type: 'object', additionalProperties: true },
    startup_ms: { type: 'number' }, peak_rss_mb: { type: 'number' },
    methodology: { type: 'string' }, baseline_json_path: { type: 'string' },
    blockers: { type: 'array', items: { type: 'string' } },
  },
}
const CRITIC_SCHEMA = {
  type: 'object', additionalProperties: true,
  required: ['verdict','test_count_matches','blockers'],
  properties: {
    verdict: { type: 'string', enum: ['GREEN','RED'] },
    test_count_matches: { type: 'boolean' },
    raw_test_line_count: { type: 'integer' }, extracted_test_count: { type: 'integer' },
    gaps: { type: 'array', items: { type: 'string' } },
    blockers: { type: 'array', items: { type: 'string' } },
    summary: { type: 'string' },
  },
}

phase('Build & Test')
const build = await agent(
  RULES +
  'TASK: Build every reference artifact needed as a parity oracle and run the FULL test suite. SERIALIZE all zig/cmake builds (shared .zig-cache — no concurrent zig invocations). Steps:\n' +
  '1. cd ' + G + '. Confirm `zig version` is 0.15.2 and HEAD is 4749c4e.\n' +
  '2. Run the full unit test suite: `zig build test` — capture ALL output to ' + OUT + '/test-run.log. Zig`s test runner prints a summary; parse total/passed/failed/skipped. If it runs long, run in background and poll the log.\n' +
  '3. Run `zig build test-lib-vt` -> ' + OUT + '/test-lib-vt.log; summarize.\n' +
  '4. Build libghostty-vt: `zig build -Demit-lib-vt=true`; note the produced lib paths (zig-out/lib/libghostty-vt.a etc.).\n' +
  '5. Build benchmarks: `zig build -Demit-bench=true`; list the produced zig-out/bin/ghostty-bench-* binaries.\n' +
  '6. Build the example/c-vt-* programs that build cleanly here (they link libghostty-vt; inspect each dir for build.zig or CMakeLists). Build as many as build without network. List which built and their run commands. Do NOT run them (the corpora agent will).\n' +
  'Write a human summary to ' + OUT + '/build-summary.md. Return the structured result. If `zig build test` reports ANY failing test, that is a blocker — record the failing test names from the log.',
  { schema: BUILD_SCHEMA, effort: 'high', label: 'build+test', phase: 'Build & Test' }
)

phase('Inventory & Plan')
const results = await parallel([
  () => agent(
    RULES +
    'TASK (fidelity contract): Extract EVERY inline Zig `test {...}` block from ALL .zig files under ' + G + '/src. Write a robust extractor SCRIPT (python) — do NOT eyeball files. For each test block capture: file (relative to src/), test name (or <anonymous>), start/end line, full body, and a per-test list+count of assertions (std.testing.expect, expectEqual, expectError, expectEqualStrings, expectEqualSlices, expectEqualDeep, expectFmt, expectApproxEq*, std.debug.assert, and any `try testing.*`). Handle nested braces and strings/comments correctly so the count is exact. Emit ' + OUT + '/inventory/tests.jsonl (one object per test) and ' + OUT + '/inventory/tests-summary.json (counts: total blocks, named vs anonymous, assertions total + breakdown by kind, files_with_tests, per-file counts). Also cross-check against a raw `rg` line count and explain any delta. The total test-block count is the NO-EXCEPTIONS contract — be exact. Keep the extractor at ' + OUT + '/inventory/extract_tests.py. Return the structured summary.',
    { schema: INV_SCHEMA, effort: 'high', label: 'test-inventory', phase: 'Inventory & Plan' }
  ),
  () => agent(
    RULES +
    'TASK (R1 comment fidelity contract): Write a python script that censuses EVERY comment in ' + G + '/src/**.zig: classify into doc-module (//!), doc-decl (///), line (//), and detect license/header blocks, TODO/FIXME/NOTE, and ASCII-diagram/citation comments (spec refs like ECMA-48, xterm, DEC). Emit ' + OUT + '/inventory/comments-census.json with: total comment lines, per-class counts, per-file counts, list of files with license headers, and total TODO/FIXME. This is the baseline the port`s fidelity critic will diff against (no comment may be dropped). Save the script at ' + OUT + '/inventory/census_comments.py. Return a concise text summary with the totals.',
    { label: 'comment-census', phase: 'Inventory & Plan' }
  ),
  () => agent(
    RULES +
    'TASK (parity surface census): Inventory the user-observable parity surface so nothing is missed later. Produce ' + OUT + '/inventory/parity-surface.json covering: (a) every config option — parse the fields of ' + G + '/src/config/Config.zig (name, type, default if obvious); (b) every keybind ACTION — from ' + G + '/src/input/Binding.zig / command.zig (the Action union); (c) every CLI action/subcommand — from ' + G + '/src/cli/ (the +action set); (d) terminfo capabilities — from the ghostty terminfo source (search src/terminfo and build/); (e) terminal modes (src/terminal/modes.zig), SGR attributes (sgr.zig), and the OSC/CSI/DCS handler sets (osc.zig, csi handlers). Give exact counts per category and write the full lists. Use scripts/grep, not guesswork. Return a text summary with all counts.',
    { label: 'parity-surface', phase: 'Inventory & Plan' }
  ),
  () => agent(
    RULES +
    'TASK (golden corpora): Assemble byte-exact golden outputs that the differential harness will diff against. Using the PREBUILT artifacts (zig-out) and any example/c-vt-* binaries the build stage produced (read ' + OUT + '/build-summary.md): (1) run each runnable c-vt-* / zig-vt example with representative input and capture stdout+stderr to ' + OUT + '/corpora/cvt/<name>.golden; (2) export the ghostty terminfo (try `infocmp -A zig-out/share/terminfo xterm-ghostty` and copy the terminfo source) to ' + OUT + '/corpora/terminfo/; (3) capture config parse goldens: run `zig-out/bin/ghostty +show-config --default=true` and a few sample configs -> ' + OUT + '/corpora/config/; (4) probe for vttest/esctest on PATH and record availability (TODO note if absent — do not fail). Write ' + OUT + '/corpora/MANIFEST.md listing every golden, how it was produced, and the exact reproduce command. Return a summary: count of goldens by category + any TODOs.',
    { label: 'golden-corpora', phase: 'Inventory & Plan' }
  ),
  () => agent(
    RULES +
    'TASK (differential harness scaffolding): Stand up the comparator that feeds identical input to reference and port and diffs canonical output, plus the GATE demonstrator. Deliver under ' + OUT + '/harness/: (1) DESIGN.md describing the comparator architecture — VT core via libghostty-vt formatters (Screen/PageList serialization, see ' + G + '/example/c-vt-formatter), config parse-then-serialize diff, input encode diff, render perceptual diff with an explicit tolerance; how identity tokens are matched via the identity-ghostty build profile (no canonicalization). (2) A working `diff_vt.sh`/python comparator skeleton that, given an input VT byte stream, runs it through the reference (zig-out libghostty-vt or a c-vt-stream/formatter binary) and through a PORT hook (currently a stub since rustty is empty), and diffs. (3) The EMPTY-WORKSPACE GATE: a script ' + OUT + '/harness/gate_empty_divergence.sh that runs the comparator against the (empty) rustty port and asserts it reports ~100% divergence (because the port produces nothing yet) — prove the harness correctly detects total divergence, and print the divergence %. (4) FUZZ-INVENTORY.md: list ghostty`s fuzz targets (grep src for fuzz harnesses / afl, e.g. terminal Parser/osc/page fuzzers) and describe the differential-fuzz wiring (shared corpus, reference-vs-port compare). Return a summary including the measured empty-divergence %.',
    { label: 'diff-harness', phase: 'Inventory & Plan' }
  ),
  () => agent(
    RULES +
    'TASK (PORT_PLAN): Produce ' + OUT + '/plan/PORT_PLAN.md — the strict topological, leaves-first port order over the rustty crate structure from the design spec. Use the existing cycle tools (' + R + '/.port/tools/cycle_check.py and reach_scc.py) and the design`s crate DAG. For EACH crate give: source files it owns (mapped from ghostty src/), its crate dependencies, the cycle-breaks it requires (cross-ref the 14-item break-list in ' + R + '/.port/cycle-analysis.md), the count of test blocks it owns (read ' + OUT + '/inventory/tests-summary.json if present, else estimate from src), and its per-crate Definition-of-Done. Confirm the order is a valid topological sort (no crate before its deps). Flag any crate where the file->crate mapping is still ambiguous. Return a summary: crate count, the ordered list, and any ambiguities.',
    { label: 'port-plan', phase: 'Inventory & Plan' }
  ),
  () => agent(
    RULES +
    'TASK (DEFINITION_OF_PARITY): Write ' + OUT + '/plan/DEFINITION_OF_PARITY.md — the machine-checkable global + per-crate parity gates. Cover: (1) GLOBAL gates: 100% test blocks ported (count == contract), assertion-for-assertion, 0 comment-inventory delta (R1), differential VT clean, differential fuzz clean, leak-free (tracking allocator/Miri), crate graph acyclic (cycle_check.py exit 0), perf/mem/size/startup not worse than BASELINE.json, every config option/keybind/action/terminfo cap covered (cross-ref parity-surface.json). (2) PER-CRATE gates template. (3) For each gate, the exact command/script that checks it and the pass condition. (4) The identity-rename caveat: which constitution identity-equivalence gates are dropped (R2) and what replaces them. Return a summary of the gate list.',
    { label: 'definition-of-parity', phase: 'Inventory & Plan' }
  ),
])

phase('Baseline')
const baseline = await agent(
  RULES +
  'TASK (BASELINE.json — run ISOLATED for accuracy; no other heavy work runs now). Capture the performance baseline on this machine (Apple M4 Pro, Mac16,7, arm64, 14 cores, 48GB, macOS 26.5.1). (1) Run the benchmark binaries built in the build stage (zig-out/bin/ghostty-bench-* — read ' + OUT + '/build-summary.md for names; each typically takes a benchmark name + reads stdin or has flags, inspect ' + G + '/src/benchmark for usage). Record throughput/timing for each, multiple runs, report median. (2) Binary sizes: stat zig-out/bin/ghostty, zig-out/lib/libghostty-vt.* (and .a), the test exe if present. (3) Startup time: time `zig-out/bin/ghostty +version` or the lightest no-GUI invocation, median of N runs. (4) If a metric cannot be measured here (e.g. GUI frame timing needs a display, peak RSS needs instrumentation), record it as a documented TODO with the reason — do NOT fabricate. Write ' + OUT + '/baseline/BASELINE.json (structured, with a methodology field and per-metric reproduce commands) and ' + OUT + '/baseline/METHODOLOGY.md. Return the structured result.',
  { schema: BASELINE_SCHEMA, effort: 'high', label: 'baseline', phase: 'Baseline' }
)

phase('Critic')
const critic = await agent(
  RULES +
  'TASK (adversarial completeness critic — default to RED when uncertain). Audit ALL Phase-0 deliverables for gaps; findings become blockers. Specifically: (1) Re-derive the test-block count independently (your own grep/script over ' + G + '/src) and compare to the extractor`s count in ' + OUT + '/inventory/tests-summary.json — ANY mismatch is a blocker (this is the no-exceptions contract). (2) Verify the test extractor handled edge cases (tests in comments/strings, nested braces, conditional tests). (3) Verify the comment census covers all 279 test-bearing files and the full src tree. (4) Verify golden corpora MANIFEST has reproduce commands and at least the VT formatter golden exists. (5) Verify the diff harness gate actually ran and reported ~100% divergence vs the empty port. (6) Verify BASELINE.json has real numbers (not all TODO) for at least binary size + one throughput metric. (7) Verify PORT_PLAN.md is a valid topo sort and DEFINITION_OF_PARITY.md has a check command per gate. (8) Verify the full `zig build test` passed (read ' + OUT + '/test-run.log) — failing reference tests are a blocker. ' +
  'Here are the prior agents` summaries:\n' +
  'BUILD: ' + JSON.stringify(build) + '\n' +
  'INVENTORY/PLAN: ' + JSON.stringify(results) + '\n' +
  'BASELINE: ' + JSON.stringify(baseline) + '\n' +
  'Write ' + OUT + '/REPORT.md (human summary of Phase 0: what was produced, all key counts, gate status) and ' + OUT + '/report.json (machine-readable: every deliverable path, counts, and gate pass/fail). Return the structured verdict. The Phase-0 GATE is GREEN only if: full reference test suite passed, test-count contract matches, BASELINE.json populated, and the diff harness proved ~100% divergence vs the empty workspace.',
  { schema: CRITIC_SCHEMA, effort: 'high', label: 'completeness-critic', phase: 'Critic' }
)

return { build, results, baseline, critic }
