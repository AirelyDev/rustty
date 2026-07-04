export const meta = {
  name: 'phase2-inc1-datastruct',
  description: 'Phase 2 increment 1: port the datastruct module (76 tests) + its util fn deps, assertion-for-assertion, loop-until-green, adversarial parity review, per-module gate',
  phases: [
    { title: 'Scout', detail: 'inventory datastruct files/tests/deps; pre-stage crate; list util fns needed' },
    { title: 'Util', detail: 'implement the rustty-util functions datastruct depends on (faithful, 0-test files)' },
    { title: 'Port', detail: 'parallel porters: each file group -> Rust src + tests with PARITY provenance' },
    { title: 'Green', detail: 'integrate + loop-until-green: all 76 datastruct tests pass + leak-clean' },
    { title: 'Review', detail: 'adversarial parity: every test ported, no dropped assertions, no UB; flip status + run gate' },
  ],
}

const REF = '/Users/gael/projects/ghostty'
const PORT = '/Users/gael/projects/rustty'
const DS_CRATE = PORT + '/crates/rustty-datastruct'
const UTIL_CRATE = PORT + '/crates/rustty-util'

const FACTS = `
GROUND-TRUTH (verified; do not re-derive):
- Reference (READ-ONLY, never modify): ${REF} @ 4749c4e93731067049bfbf2e4572061cef2bdd17. Module to port: src/datastruct/ (11 files).
- Port workspace (git, on branch with Phases 0+1 committed): ${PORT}. Work DIRECTLY in the main workspace (this is a single-module increment; no worktree). Target crate: ${DS_CRATE} (rustty-datastruct). Util deps go in ${UTIL_CRATE} (rustty-util). rustty-datastruct already depends on rustty-util? if not, the scout adds it to Cargo.toml.
- datastruct test contract (from .port/MODULE_STATUS.json + PARITY_LEDGER.json): EXACTLY 76 test blocks across these files (file: test count):
    comparison.zig=24, circ_buf.zig=21, split_tree.zig=18, lru.zig=4, message_data.zig=3, blocking_queue.zig=2, cache_table.zig=1, intrusive_linked_list.zig=1, segmented_pool.zig=1, main.zig=1, array_list_collection.zig=0.
  Sum = 76. This count is the CONTRACT: every one of the 76 reference test blocks MUST become a Rust test, assertion-for-assertion, with NO exceptions. (.port/phase-0/PARITY_LEDGER.json 'tests' array, filtered to file startswith 'src/datastruct/', is the authoritative per-test list with line numbers and assertion kinds.)
- PROVENANCE CONVENTION (gate_tests.sh enforces this): immediately above each ported Rust #[test], put a comment line:  // PARITY: src/datastruct/<file>.zig:<line> <reference test name>  . Each ported reference test carries exactly one such marker. gate_tests counts these and cross-checks against 'cargo test -- --list'; a mismatch FAILs.
- PARITY RULES: keep the SAME algorithms (circ_buf growth, split_tree structure, lru eviction, etc.). Idiomatic Rust is fine (ownership/Drop instead of Zig allocators) AS LONG AS behavior matches the ported assertions exactly. Do NOT substitute std collections that change observable behavior/complexity. No 'unsafe' unless the reference is inherently unsafe (intrusive list / segmented pool may need it) — and then it must be sound. Preserve panic/error semantics where a test asserts them.
- Leak parity: tests must be leak-clean (gate_leaks runs macOS 'leaks --atExit'); no leaked allocations.
- Do NOT modify the reference repo. Do NOT git commit (orchestrator commits). Do NOT weaken/skip/xfail any test or gate. If a reference test cannot be ported faithfully, STOP and surface it as a blocker (do not drop it).
`

const SCOUT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['files', 'intra_deps', 'util_fns_needed', 'crate_prestaged', 'total_tests_seen', 'notes'],
  properties: {
    files: { type: 'array', items: { type: 'object', additionalProperties: true }, description: 'per file: {name, tests, summary, rust_module}' },
    intra_deps: { type: 'array', items: { type: 'string' }, description: 'intra-datastruct dependencies (file -> file)' },
    util_fns_needed: { type: 'array', items: { type: 'string' }, description: 'rustty-util functions/symbols datastruct requires' },
    crate_prestaged: { type: 'boolean', description: 'lib.rs (pub mod stubs) + Cargo.toml (rustty-util dep) staged' },
    total_tests_seen: { type: 'integer', description: 'must be 76' },
    notes: { type: 'string' },
  },
}
const UTIL_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['util_builds', 'fns_implemented', 'notes'],
  properties: { util_builds: { type: 'boolean' }, fns_implemented: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' } },
}
const PORT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['files_ported', 'tests_ported', 'provenance_added', 'notes'],
  properties: {
    files_ported: { type: 'array', items: { type: 'string' } },
    tests_ported: { type: 'integer' },
    provenance_added: { type: 'boolean' },
    notes: { type: 'string' },
  },
}
const GREEN_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['build_ok', 'all_tests_pass', 'datastruct_test_count', 'leak_clean', 'test_output_tail', 'notes'],
  properties: {
    build_ok: { type: 'boolean' },
    all_tests_pass: { type: 'boolean' },
    datastruct_test_count: { type: 'integer', description: 'number of Rust tests in rustty-datastruct that ran/passed; must be >= 76' },
    leak_clean: { type: 'boolean' },
    test_output_tail: { type: 'string' },
    notes: { type: 'string' },
  },
}
const CRITIC_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['check', 'passed', 'blocker', 'findings'],
  properties: {
    check: { type: 'string' }, passed: { type: 'boolean' }, blocker: { type: 'boolean' },
    findings: { type: 'array', items: { type: 'string' } },
  },
}

phase('Scout')
const scout = await agent(`You are the SCOUT for Phase 2 increment 1 (porting datastruct).
${FACTS}
TASK:
1. Read every file in ${REF}/src/datastruct/*.zig and ${REF}/src/datastruct.zig. For each: summarize what it is, its public API, and its tests. Read ${PORT}/.port/phase-0/PARITY_LEDGER.json filtered to src/datastruct/ for the authoritative per-test list (names + lines).
2. Determine intra-datastruct dependencies (which file imports which) so porters can be ordered/grouped; note any file that depends on another datastruct file.
3. Determine which rustty-util functions/symbols datastruct needs (e.g. fastmem copy/move, math helpers). List them precisely (Zig source location in src/*.zig).
4. PRE-STAGE the crate so parallel porters only touch their own src files:
   - ${DS_CRATE}/Cargo.toml: ensure it depends on rustty-util (path dep) and builds.
   - ${DS_CRATE}/src/lib.rs: write '#![allow(dead_code)]' + a 'pub mod <x>;' line for EACH datastruct file's Rust module (comparison, circ_buf, split_tree, lru, message_data, blocking_queue, cache_table, intrusive_linked_list, segmented_pool, array_list_collection; main.zig's re-exports become lib.rs re-exports). Create empty placeholder src/<x>.rs files (each with a //! doc naming its reference file) so the crate has a known module set. It need not compile yet.
5. Confirm total tests seen across datastruct == 76.
Return SCOUT_SCHEMA. Do NOT port logic yet (porters do that). Do NOT modify the reference.`, { label: 'scout', phase: 'Scout', schema: SCOUT_SCHEMA })

phase('Util')
const util = await agent(`You are the UTIL-PORT agent for Phase 2 increment 1.
${FACTS}
The scout listed the rustty-util functions datastruct needs: ${'${SEE scout.util_fns_needed}'} -- read the scout's notes and ${PORT}/.port/phase-0 if needed; authoritative source is the reference src/*.zig (fastmem.zig, math.zig, etc.).
TASK: Implement those functions faithfully in ${UTIL_CRATE} (rustty-util), matching the reference algorithms (e.g. fastmem.copy/move map to ptr::copy/copy_nonoverlapping with the SAME overlap semantics). Add '// PARITY: src/<file>.zig:<line> <decl>' provenance comments on ported items. These util files have 0 tests in the contract, so you are porting CODE only (no tests to port), but correctness matters — datastruct's tests will exercise them. Ensure 'cargo build -p rustty-util' exits 0.
Return UTIL_SCHEMA. Do NOT change <root> status in MODULE_STATUS.json (util fns are 0-test deps; <root> test-porting is a later increment). Do NOT modify the reference.`, { label: 'util', phase: 'Util', schema: UTIL_SCHEMA })

phase('Port')
// Parallel porters; disjoint files; lib.rs pre-staged by scout so no shared-file writes.
const GROUPS = [
  { label: 'comparison', files: 'comparison.zig (24 tests)' },
  { label: 'circ_buf', files: 'circ_buf.zig (21 tests)' },
  { label: 'split_tree', files: 'split_tree.zig (18 tests)' },
  { label: 'small-A', files: 'lru.zig (4), message_data.zig (3), blocking_queue.zig (2)' },
  { label: 'small-B', files: 'cache_table.zig (1), intrusive_linked_list.zig (1), segmented_pool.zig (1), array_list_collection.zig (0), main.zig (1 - aggregator: fold its 1 test + any re-exports into lib.rs)' },
]
const ported = await parallel(GROUPS.map(g => () => agent(`You are a PORTER for Phase 2 increment 1. Port ONLY these datastruct file(s): ${g.files}
${FACTS}
TASK: For each assigned reference file ${REF}/src/datastruct/<file>.zig:
1. Port the implementation to its Rust module ${DS_CRATE}/src/<file>.rs (the module is already declared in lib.rs by the scout). Match the reference algorithm/structure. Use rustty-util for any util fns.
2. Port EVERY test block in that file to a Rust #[test] in a '#[cfg(test)] mod tests { use super::*; ... }' in the SAME src/<file>.rs, assertion-for-assertion (every expect/expectEqual/expectError/etc. becomes the equivalent assert!/assert_eq!/#[should_panic]/Result). Put a '// PARITY: src/datastruct/<file>.zig:<line> <test name>' comment immediately above each #[test]. Anonymous 'test {' -> name it test_anon_<line>.
3. Match leak semantics (no leaks). Match error/panic behavior where asserted.
ONLY write to ${DS_CRATE}/src/<your files>.rs (and for the 'small-B' aggregator, you MAY add re-export lines to lib.rs for main.zig's public surface — coordinate by ONLY appending clearly-marked re-exports). Do NOT touch other porters' files, Cargo.toml, or other crates. The crate may not fully compile until all porters finish (the Green agent integrates) — but make YOUR file as correct and self-consistent as possible; run 'cargo build -p rustty-datastruct 2>&1 | grep <yourfile>' to catch your own errors.
Return PORT_SCHEMA (tests_ported = number of #[test] you wrote; must equal your files' contract counts). If a test cannot be faithfully ported, note it as a blocker in notes (do NOT drop it). Do NOT modify the reference.`, { label: 'port:' + g.label, phase: 'Port', schema: PORT_SCHEMA })))

phase('Green')
const green = await agent(`You are the GREEN (integration + loop-until-green) agent for Phase 2 increment 1.
${FACTS}
The util fns and all datastruct files have been ported by parallel porters. Now make the crate fully green.
TASK:
1. 'cd ${PORT} && cargo build -p rustty-datastruct' -- fix ALL compile errors (integration issues between modules, lib.rs wiring, missing re-exports from main.zig, util signatures). Do not change test ASSERTIONS to make them pass; fix real port bugs in the implementation. If a test legitimately reveals a port bug, fix the implementation to match reference behavior.
2. 'cargo test -p rustty-datastruct' -- ALL tests must pass. Count them. The count MUST be >= 76 (the contract). If fewer than 76 tests exist, a porter dropped tests -> find the missing reference tests (compare provenance markers against PARITY_LEDGER datastruct entries) and port them. loop until every test passes AND count >= 76.
3. Leak check: run gate_leaks for this crate (or 'cargo test -p rustty-datastruct' under 'leaks --atExit' / the gate's mechanism). Must be leak-clean.
4. Also confirm 'cargo build --workspace' still succeeds (no breakage to other crates).
Return GREEN_SCHEMA with the real test count (datastruct_test_count), pass status, leak status, and the tail of the test output. Do NOT modify the reference. Do NOT weaken any test.`, { label: 'green', phase: 'Green', schema: GREEN_SCHEMA })

phase('Review')
const critics = await parallel([
  () => agent(`Adversarial COMPLETENESS critic, Phase 2 inc 1. Verify EVERY datastruct reference test was ported, assertion-for-assertion.
${FACTS}
- Build the authoritative list of the 76 datastruct reference test blocks from ${PORT}/.port/phase-0/PARITY_LEDGER.json (file startswith src/datastruct/). For EACH, confirm a Rust #[test] exists in rustty-datastruct with a matching '// PARITY: <file>:<line> <name>' marker. List any MISSING tests (BLOCKER).
- Spot-check 8+ ported tests (across comparison/circ_buf/split_tree/lru/the small files) by reading the reference test body and the Rust port side-by-side: confirm EVERY assertion was preserved (no expect dropped, no loosened comparison, no commented-out asserts, no assert!(true) placeholders). Report any dropped/weakened assertion as a BLOCKER.
- Confirm provenance count == 76 and 'cargo test -p rustty-datastruct -- --list' count == ported test count.
Return CRITIC_SCHEMA. check='all 76 datastruct tests ported assertion-for-assertion'.`, { label: 'critic:completeness', phase: 'Review', schema: CRITIC_SCHEMA }),

  () => agent(`Adversarial SOUNDNESS + FIDELITY critic, Phase 2 inc 1.
${FACTS}
- Read all rustty-datastruct src for 'unsafe' blocks: for each, confirm it is sound (no UB, no aliasing violations, bounds respected) and that the reference genuinely required unsafe (intrusive list / segmented pool may). Report unsound unsafe as BLOCKER.
- Algorithm fidelity: for circ_buf, split_tree, lru, compare the Rust algorithm to the reference (growth/eviction/rebalance strategy, capacity semantics, iteration order). Report any behavioral divergence (even if current tests pass) as a finding/BLOCKER.
- Confirm no test was made to pass by altering its assertion vs the reference. Confirm 'cargo test -p rustty-datastruct' actually passes (run it).
Return CRITIC_SCHEMA. check='datastruct port sound + algorithmically faithful'.`, { label: 'critic:soundness', phase: 'Review', schema: CRITIC_SCHEMA }),

  () => agent(`GATE agent, Phase 2 inc 1. Flip the module status and run the parity gate.
${FACTS}
- Confirm 'cargo test -p rustty-datastruct' passes with >= 76 tests and is leak-clean.
- Edit ${PORT}/.port/MODULE_STATUS.json: set the 'datastruct' module status to "ported" and ported_tests to the actual passing count (>=76). Leave all other modules unchanged (do NOT touch <root>; util fns are 0-test deps).
- Run ${PORT}/.port/gates/gate_tests.sh -> must report datastruct PASS (ported, REF=76, PORT>=76) and overall exit 0 (other modules PENDING).
- Run ${PORT}/.port/gates/gate_leaks.sh -> must be PASS/clean.
- Run ${PORT}/ci.sh -> must be GREEN (exit 0): build OK, tests OK, gate_all PASS (datastruct now a real PASS, others PENDING), FAIL=0 ERROR=0.
- Confirm reference repo 'git -C ${REF} status --porcelain' is empty (untouched).
Return CRITIC_SCHEMA with the observed exit codes in findings. check='datastruct gate green (status=ported, gate_tests PASS, ci.sh GREEN)'. blocker=true if ci.sh not green or status flip inconsistent.`, { label: 'gate', phase: 'Review', schema: CRITIC_SCHEMA }),
])

const blockers = critics.filter(Boolean).filter(c => c.blocker || !c.passed)
return {
  scout, util,
  ported_summary: ported.filter(Boolean).map(p => ({ files: p.files_ported, tests: p.tests_ported })),
  green,
  critics,
  blockers,
  gate_green: blockers.length === 0
    && !!green && green.build_ok === true && green.all_tests_pass === true
    && green.datastruct_test_count >= 76 && green.leak_clean === true,
}
