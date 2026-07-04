export const meta = {
  name: 'phase2-batch1-leaf-crates',
  description: 'Phase 2 batch 1: port the pure leaf crates (rustty-util, build-config, lib, datastruct) assertion-for-assertion + comment-for-comment, with a tracking-allocator leak harness, per-crate gates, and adversarial parity review, loop-until-green',
  phases: [
    { title: 'Test harness', detail: 'create rustty-testutil: tracking allocator + leak assert + std.testing-equivalent helpers' },
    { title: 'Port leaves', detail: 'serial: util -> build-config -> lib -> datastruct; each ported then adversarially reviewed, loop-until-green' },
    { title: 'Integrate', detail: 'full-workspace gate + commit + report' },
  ],
}

const G = '/Users/gael/projects/ghostty'
const R = '/Users/gael/projects/rustty'
const INV = R + '/.port/phase-0/inventory'
const RULES = 'PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md govern everything. Critical: R1 = 100% FULL FIDELITY — port EVERY line of logic, preserve EVERY comment (doc //!, ///, //, license headers, TODO/FIXME, ASCII diagrams, spec citations) verbatim except retarget ghostty-specific identifiers to rustty per R2; port EVERY `test{}` block as a Rust #[test], ASSERTION-FOR-ASSERTION (same checks, same values, same edge cases), NOTHING dropped or weakened. No-loss is the law; you MAY add minimal Rust glue (only what is needed to compile/link). Keep the SAME algorithms and data layouts as the reference (byte-identical where layouts are compared). The ghostty tree at ' + G + ' is READ-ONLY (pinned 4749c4e, Zig 0.15.2) — NEVER modify it; the rustty crates must NOT depend on it. Per-crate test/assertion contract is in ' + INV + '/tests-summary.json (crate_map) and tests.jsonl (full bodies); comment baseline in ' + INV + '/comments-census.json. Work in ' + R + '. Be exact; never claim a green you did not run. '

const PORT_SCHEMA = {
  type: 'object', additionalProperties: true,
  required: ['crate','tests_ported','tests_contract','all_gates_green','blockers'],
  properties: {
    crate: { type: 'string' },
    files_ported: { type: 'array', items: { type: 'string' } },
    tests_ported: { type: 'integer' }, tests_contract: { type: 'integer' },
    assertions_ported: { type: 'integer' }, assertions_contract: { type: 'integer' },
    comments_ported_lines: { type: 'integer' }, comments_contract_lines: { type: 'integer' },
    gates: { type: 'object', additionalProperties: true },
    all_gates_green: { type: 'boolean' },
    breaks_handled: { type: 'array', items: { type: 'string' } },
    deferred_hooks: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
    blockers: { type: 'array', items: { type: 'string' } },
  },
}
const REVIEW_SCHEMA = {
  type: 'object', additionalProperties: true,
  required: ['verdict','blockers'],
  properties: {
    verdict: { type: 'string', enum: ['PARITY','NOT_PARITY'] },
    dropped_or_missing_tests: { type: 'array', items: { type: 'string' } },
    dropped_or_weakened_assertions: { type: 'array', items: { type: 'string' } },
    dropped_comments: { type: 'array', items: { type: 'string' } },
    unsound_unsafe_or_ub: { type: 'array', items: { type: 'string' } },
    behavior_or_layout_diffs: { type: 'array', items: { type: 'string' } },
    gate_reproduced: { type: 'boolean' },
    notes: { type: 'string' },
    blockers: { type: 'array', items: { type: 'string' } },
  },
}
const INTEG_SCHEMA = {
  type: 'object', additionalProperties: true,
  required: ['verdict','workspace_test_green','acyclic_green','blockers'],
  properties: {
    verdict: { type: 'string', enum: ['GREEN','RED'] },
    workspace_test_green: { type: 'boolean' }, acyclic_green: { type: 'boolean' },
    total_tests_ported_so_far: { type: 'integer' },
    per_crate: { type: 'array', items: { type: 'object', additionalProperties: true } },
    commit: { type: 'string' },
    blockers: { type: 'array', items: { type: 'string' } },
  },
}

phase('Test harness')
const testutil = await agent(
  RULES +
  'TASK: Create a dev-only test-support crate `crates/rustty-testutil` (NOT a runtime dep — used only under [dev-dependencies]) that lets ported Zig tests keep their semantics, especially LEAK DETECTION (the constitution requires std.testing.allocator-equivalent leak checking; cargo miri is NOT installed so use a TRACKING GLOBAL ALLOCATOR approach):\n' +
  '1. A `TrackingAllocator` implementing GlobalAlloc that wraps System and counts live allocations/bytes, plus a scoped guard `assert_no_leaks(|| { ...test body... })` that fails if allocations are not balanced at scope end — mirroring how Zig`s testing.allocator detects leaks. Document how a test opts in.\n' +
  '2. Thin assertion helpers mirroring std.testing so ported tests read like the originals where helpful: expect, expect_equal, expect_error, expect_equal_slices, expect_equal_strings (map to Rust assert!/assert_eq! with good messages). These are conveniences; ported tests may also use plain assert_eq!.\n' +
  '3. A self-test: one test that leaks (Box::leak) and is DETECTED by the guard, and one clean test that passes — proving the leak harness works. Mark the leaking self-test #[ignore] or gate it so the normal suite is green, but prove detection in a doctest/explicit run.\n' +
  'Wire it into the workspace ([workspace.dependencies], dev-dependency for crates). Run `cargo test -p rustty-testutil`; must be green. Update ' + R + '/.port/tools/gates/check_leaks.sh to run `cargo test` with the tracking allocator path for done crates (no longer BLOCKED). Write ' + R + '/.port/phase-2/testutil.md. Return a short summary: does it build, does leak-detection work, how crates opt in.',
  { effort: 'high', label: 'rustty-testutil', phase: 'Test harness' }
)

phase('Port leaves')
const SPECS = [
  { name: 'rustty-util', files: 'quirks.zig, math.zig, fastmem.zig, file_type.zig, tripwire.zig (the src/ root util files)',
    notes: 'BREAK #2: quirks.zig imports font/main.zig in ONE place (a font-related quirk). Port quirks fully but make that single font-dependent path a hook/trait/feature-gated seam to be wired when rustty-font lands — do NOT pull font into util (it must stay a leaf). Record the deferred wiring in deferred_hooks. math/fastmem/file_type/tripwire are pure.' },
  { name: 'rustty-build-config', files: 'build_config.zig',
    notes: 'BREAK #3: DEFINE the backend enums (app-runtime/RuntimeType, FontBackend, RendererType, and any other backend enums build_config exposes) HERE so apprt/font/renderer import them from build-config (not the reverse). Pull version/identity strings from rustty-identity (already exists). 0 test blocks expected — but port any comptime logic + comments faithfully. No upward deps.' },
  { name: 'rustty-lib', files: 'lib/ (allocator.zig, allocator/convenience.zig, main.zig, and the rest of src/lib/)',
    notes: 'Allocator utilities exposed by libghostty-vt. lib/allocator.zig and allocator/convenience.zig form a 2-file cycle — same crate, fine. Preserve the allocator vtable/ABI semantics exactly.' },
  { name: 'rustty-datastruct', files: 'datastruct/ (all of src/datastruct/)',
    notes: 'Pure data structures (intrusive linked list, circular buffer, LRU, cache_table, ref_counted_set, bitmap_allocator, segmented pool, array_list_collection, comparison, split_tree, etc.). Depends on rustty-util (fastmem, quirks). Preserve packed/bit layouts EXACTLY where the reference compares bytes. 76 test blocks — the largest in this batch.' },
]

const ported = []
for (let i = 0; i < SPECS.length; i++) {
  const spec = SPECS[i]
  const portPrompt =
    RULES +
    'TASK: Port the crate **' + spec.name + '** from the reference. Source: ' + spec.notes + '\n' +
    'STEPS (loop until ALL gates green):\n' +
    '1. Read the reference source files (under ' + G + '/src) and this crate`s contract in ' + INV + '/tests-summary.json (crate_map -> ' + spec.name + ') + tests.jsonl + comments-census.json.\n' +
    '2. Port ALL code to idiomatic, parity-preserving Rust in crates/' + spec.name + '/src/. Preserve EVERY comment (retarget ghostty->rustty identifiers per R2 only). Keep the same algorithms + data layouts.\n' +
    '3. Port EVERY test{} block as #[test], ASSERTION-FOR-ASSERTION. Use rustty-testutil (dev-dependency) for leak-checked tests (wrap allocating tests in the no-leak guard) and the assertion helpers. Anonymous tests -> a fixed naming convention.\n' +
    '4. Set the crate`s lib.rs `//! PORT STATUS:` line to `done`.\n' +
    '5. Run per-crate gates and LOOP until all green: `cargo test -p ' + spec.name + '` (all pass); G1 strict for this crate (python3 ' + R + '/.port/tools/gates/check_tests_ported.py ... ported==contract, names match); G2 assertions (>= ref per test); G3 comment fidelity (no drop); G7 acyclic (cargo graph still acyclic); leak gate (tracking allocator, no leaks). Handle the listed BREAK if any (record in breaks_handled / deferred_hooks).\n' +
    '6. Commit "rustty: port ' + spec.name + ' (phase 2)" with Co-Authored-By: Claude Opus 4.8 (1M context).\n' +
    'Return the structured result with EXACT counts (tests_ported vs tests_contract, assertions, comments) and the gate results. If a gate cannot be made green honestly, STOP and put a precise blocker (do NOT weaken any test/gate).'
  let result = await agent(portPrompt, { schema: PORT_SCHEMA, effort: 'high', label: 'port:' + spec.name, phase: 'Port leaves' })

  const reviewPrompt =
    RULES +
    'TASK (adversarial parity review of **' + spec.name + '** — default to NOT_PARITY when uncertain). The porter claims: ' + JSON.stringify(result) + '\n' +
    'Independently verify against the reference (' + G + '/src) and the contract (' + INV + '): (1) is EVERY reference test{} block present as a Rust #[test] with the SAME assertions and values? List any missing/weakened. (2) is EVERY comment preserved (compare counts + spot-check content; flag any dropped/over-summarized)? (3) any non-idiomatic or UNSOUND unsafe / potential UB? (4) any behavior or memory-layout divergence from the reference (esp. packed/bit layouts, integer widths, overflow/wrap semantics, allocator behavior)? (5) RE-RUN the gates yourself (cargo test -p ' + spec.name + ', G1 strict, G2, G3, G7, leak) and confirm they are genuinely green. Return the verdict. Any dropped test/assertion/comment, any unsound unsafe, or any gate that is not actually green => NOT_PARITY with specifics.'
  let review = await agent(reviewPrompt, { schema: REVIEW_SCHEMA, effort: 'high', label: 'review:' + spec.name, phase: 'Port leaves' })

  if (review.verdict !== 'PARITY') {
    const fixPrompt = portPrompt + '\n\nA prior port attempt FAILED adversarial review. Findings to FIX (do not regress anything else): ' + JSON.stringify(review)
    result = await agent(fixPrompt, { schema: PORT_SCHEMA, effort: 'high', label: 'report:' + spec.name, phase: 'Port leaves' })
    review = await agent(reviewPrompt + '\n\n(Re-review after fixes.) New port claim: ' + JSON.stringify(result), { schema: REVIEW_SCHEMA, effort: 'high', label: 'rereview:' + spec.name, phase: 'Port leaves' })
  }
  ported.push({ crate: spec.name, result, review })
}

phase('Integrate')
const integ = await agent(
  RULES +
  'TASK (integrate + full-workspace gate for batch 1). (1) Run the full CI: ' + R + '/.port/tools/ci.sh — confirm `cargo build --workspace`, `cargo test --workspace`, `cargo clippy -D warnings`, `cargo fmt --check` all green, G7 acyclic green, and the per-crate parity gates (G1/G2/G3) green for the 4 done crates (rustty-util, rustty-build-config, rustty-lib, rustty-datastruct). (2) Re-run G1 in progress mode and report total tests ported so far vs 3234. (3) Verify no port crate depends on ghostty (R3). (4) Ensure git tree clean and all 4 crates committed; make a final batch commit if needed. (5) Write ' + R + '/.port/phase-2/batch1-REPORT.md and batch1-report.json (per-crate: tests ported/contract, assertions, comments, gate status; the adversarial review verdicts). ' +
  'Per-crate port+review results: ' + JSON.stringify(ported) + '\n' +
  'Return the structured verdict. GREEN only if all 4 crates: tests ported == contract, assertion-for-assertion, comment fidelity intact, gates green, AND the full workspace builds+tests green and stays acyclic.',
  { schema: INTEG_SCHEMA, effort: 'high', label: 'integrate-batch1', phase: 'Integrate' }
)

return { testutil, ported, integ }
