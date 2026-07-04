export const meta = {
  name: 'phase2-batch1-leaf-crates-miri',
  description: 'Phase 2 batch 1 (Miri gate): port pure leaf crates rustty-util, build-config, lib, datastruct assertion-for-assertion + comment-for-comment, with Miri (UB+leaks) as the memory gate, per-crate gates + adversarial parity review, loop-until-green',
  phases: [
    { title: 'Port leaves', detail: 'serial: util -> build-config -> lib -> datastruct; each ported then adversarially reviewed, loop-until-green; Miri memory gate' },
    { title: 'Integrate', detail: 'full-workspace gate + commit + report' },
  ],
}

const G = '/Users/gael/projects/ghostty'
const R = '/Users/gael/projects/rustty'
const INV = R + '/.port/phase-0/inventory'
const RULES = 'PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md govern everything. R1 = 100% FULL FIDELITY: port EVERY line of logic; preserve EVERY comment (//!, ///, //, license headers, TODO/FIXME, ASCII diagrams, spec citations) verbatim except retarget ghostty-specific identifiers to rustty (R2); port EVERY `test{}` block as a Rust #[test], ASSERTION-FOR-ASSERTION (same checks/values/edge-cases), NOTHING dropped or weakened. No-loss is law; you MAY add minimal Rust glue only as needed to compile/link. Keep the SAME algorithms + data layouts (byte-identical where layouts are compared). TESTS use STANDARD Rust assertions: std.testing.expectEqual->assert_eq!, expect->assert!, expectError->assert!(matches!(x, Err(..))) / is_err, expectEqualStrings->assert_eq! on &str, expectEqualSlices->assert_eq! on slices, expectApproxEqAbs->approx compare. NO custom tracking-allocator/testutil. R6 MEMORY GATE: memory safety + leaks are checked by MIRI (these are pure-Rust crates): `bash ' + R + '/.port/tools/gates/check_leaks.sh --crate <crate>` runs `cargo +nightly miri test -p <crate>` and MUST be clean (zero UB, zero leaks). Miri is installed + validated. The ghostty tree ' + G + ' is READ-ONLY (pinned 4749c4e, Zig 0.15.2) — NEVER modify; rustty must NOT depend on it. Per-crate test/assertion contract: ' + INV + '/tests-summary.json (crate_map) + tests.jsonl (full bodies); comment baseline: ' + INV + '/comments-census.json. Work in ' + R + '. Be exact; never claim a green you did not run. '

const PORT_SCHEMA = {
  type: 'object', additionalProperties: true,
  required: ['crate','tests_ported','tests_contract','all_gates_green','blockers'],
  properties: {
    crate: { type: 'string' },
    files_ported: { type: 'array', items: { type: 'string' } },
    tests_ported: { type: 'integer' }, tests_contract: { type: 'integer' },
    assertions_ported: { type: 'integer' },
    comments_ported_lines: { type: 'integer' }, comments_contract_lines: { type: 'integer' },
    gates: { type: 'object', additionalProperties: true },
    miri_clean: { type: 'boolean' },
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
    gate_reproduced: { type: 'boolean' }, miri_reproduced: { type: 'boolean' },
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
    miri_all_done_green: { type: 'boolean' },
    total_tests_ported_so_far: { type: 'integer' },
    per_crate: { type: 'array', items: { type: 'object', additionalProperties: true } },
    commit: { type: 'string' },
    blockers: { type: 'array', items: { type: 'string' } },
  },
}

phase('Port leaves')
const SPECS = [
  { name: 'rustty-util', files: 'quirks.zig, math.zig, fastmem.zig, file_type.zig, tripwire.zig (the src/ root util files)',
    notes: 'BREAK #2: quirks.zig imports font/main.zig in ONE place (a font-related quirk). Port quirks fully but make that single font-dependent path a hook/trait/feature-gated seam wired when rustty-font lands — do NOT pull font into util (it stays a leaf, Miri-able). Record the deferred wiring in deferred_hooks. math/fastmem/file_type/tripwire are pure.' },
  { name: 'rustty-build-config', files: 'build_config.zig',
    notes: 'BREAK #3: DEFINE the backend enums (RuntimeType/app-runtime, FontBackend, RendererType, etc.) HERE so apprt/font/renderer import them from build-config (not the reverse). Pull version/identity from rustty-identity (exists). 0 test blocks expected — port all comptime logic + comments faithfully. No upward deps.' },
  { name: 'rustty-lib', files: 'lib/ (allocator.zig, allocator/convenience.zig, main.zig, and the rest of src/lib/)',
    notes: 'Allocator utilities exposed by libghostty-vt. lib/allocator.zig + allocator/convenience.zig form a 2-file cycle (same crate, fine). Preserve the allocator vtable/ABI semantics exactly.' },
  { name: 'rustty-datastruct', files: 'datastruct/ (all of src/datastruct/)',
    notes: 'Pure data structures (intrusive linked list, circular buffer, LRU, cache_table, ref_counted_set, bitmap_allocator, segmented pool, array_list_collection, comparison, split_tree, etc.). Depends on rustty-util (fastmem, quirks). Preserve packed/bit layouts EXACTLY where the reference compares bytes. 76 test blocks — the largest in this batch.' },
]

const ported = []
for (let i = 0; i < SPECS.length; i++) {
  const spec = SPECS[i]
  const portPrompt =
    RULES +
    'TASK: Port the crate **' + spec.name + '**. Source: ' + spec.notes + '\n' +
    'STEPS (loop until ALL gates green):\n' +
    '1. Read the reference source under ' + G + '/src and this crate`s contract (' + INV + '/tests-summary.json crate_map -> ' + spec.name + ', tests.jsonl, comments-census.json).\n' +
    '2. Port ALL code to idiomatic, parity-preserving Rust in crates/' + spec.name + '/src/. Preserve EVERY comment (retarget ghostty->rustty only). Same algorithms + layouts.\n' +
    '3. Port EVERY test{} block as #[test], ASSERTION-FOR-ASSERTION, using standard Rust assertions. Anonymous tests -> a fixed naming convention (e.g. test_anon_<n>).\n' +
    '4. Set the crate lib.rs `//! PORT STATUS:` line to `done`.\n' +
    '5. Run per-crate gates and LOOP until all green: `cargo test -p ' + spec.name + '` (fast correctness, all pass); G1 strict (python3 ' + R + '/.port/tools/gates/check_tests_ported.py with ported==contract + names match); G2 `check_assertions.py --crate ' + spec.name + '`; G3 `check_comments_fidelity.py --crate ' + spec.name + '` (no comment drop); G7 `check_acyclic.sh` (graph stays acyclic); **G6 MEMORY: `bash ' + R + '/.port/tools/gates/check_leaks.sh --crate ' + spec.name + '` (Miri — zero UB, zero leaks)**. Handle the listed BREAK (record in breaks_handled/deferred_hooks).\n' +
    '6. Commit "rustty: port ' + spec.name + ' (phase 2)" ending with Co-Authored-By: Claude Opus 4.8 (1M context).\n' +
    'Return the structured result with EXACT counts and gate results incl. miri_clean. If a gate cannot be made green honestly, STOP with a precise blocker — NEVER weaken a test/gate.'
  let result = await agent(portPrompt, { schema: PORT_SCHEMA, effort: 'high', label: 'port:' + spec.name, phase: 'Port leaves' })

  const reviewPrompt =
    RULES +
    'TASK (adversarial parity review of **' + spec.name + '** — default NOT_PARITY when uncertain). Porter claims: ' + JSON.stringify(result) + '\n' +
    'Independently verify vs the reference (' + G + '/src) + contract (' + INV + '): (1) is EVERY reference test{} present as a Rust #[test] with the SAME assertions/values? List missing/weakened. (2) is EVERY comment preserved (counts + spot-check content)? (3) any non-idiomatic or UNSOUND unsafe / potential UB? (4) any behavior/memory-layout divergence (packed/bit layouts, integer widths, overflow/wrap, allocator semantics)? (5) RE-RUN the gates yourself: `cargo test -p ' + spec.name + '`, check_tests_ported (strict), check_assertions, check_comments_fidelity, check_acyclic, and **`check_leaks.sh --crate ' + spec.name + '` (Miri must be clean)** — confirm genuinely green. Return the verdict. Any dropped test/assertion/comment, any unsound unsafe, any Miri UB/leak, or any not-actually-green gate => NOT_PARITY with specifics.'
  let review = await agent(reviewPrompt, { schema: REVIEW_SCHEMA, effort: 'high', label: 'review:' + spec.name, phase: 'Port leaves' })

  if (review.verdict !== 'PARITY') {
    const fixPrompt = portPrompt + '\n\nA prior attempt FAILED adversarial review. FIX these (regress nothing else): ' + JSON.stringify(review)
    result = await agent(fixPrompt, { schema: PORT_SCHEMA, effort: 'high', label: 'refix:' + spec.name, phase: 'Port leaves' })
    review = await agent(reviewPrompt + '\n\n(Re-review after fixes.) New claim: ' + JSON.stringify(result), { schema: REVIEW_SCHEMA, effort: 'high', label: 'rereview:' + spec.name, phase: 'Port leaves' })
  }
  ported.push({ crate: spec.name, result, review })
}

phase('Integrate')
const integ = await agent(
  RULES +
  'TASK (integrate + full-workspace gate, batch 1). (1) Run ' + R + '/.port/tools/ci.sh: confirm cargo build/test --workspace, clippy -D warnings, fmt --check all green, G7 acyclic green, and per-crate G1/G2/G3 green for the 4 done crates. (2) Run **`bash ' + R + '/.port/tools/gates/check_leaks.sh --all-done`** — Miri must be clean for all done crates (zero UB/leaks). (3) Re-run G1 progress mode: report total tests ported so far vs 3234. (4) Verify no port crate depends on ghostty (R3). (5) Ensure git tree clean, all 4 crates committed (final batch commit if needed). (6) Write ' + R + '/.port/phase-2/batch1-REPORT.md + batch1-report.json (per-crate tests ported/contract, assertions, comments, Miri status, review verdicts). ' +
  'Per-crate results: ' + JSON.stringify(ported) + '\n' +
  'Return the structured verdict. GREEN only if all 4 crates: tests==contract, assertion-for-assertion, comment fidelity intact, Miri clean, gates green, AND full workspace builds+tests green + acyclic.',
  { schema: INTEG_SCHEMA, effort: 'high', label: 'integrate-batch1', phase: 'Integrate' }
)

return { ported, integ }
