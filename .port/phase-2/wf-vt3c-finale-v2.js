export const meta = {
  name: 'vt3c-finale-v2',
  description: 'Finalize PageList (vt-3c) on the faithful re-architected allocator. Integrate gate (210 tests + ASan-all + Miri-subset + full G3b + tracking-leak + clippy/fmt/acyclic + ci.sh) -> the complete-PageList commit; then an adversarial parity review of the tests + the allocator re-architecture + the tracking allocator.',
  phases: [
    { title: 'Integrate', detail: '210 + ASan-all + Miri-subset + G3b + tracking + ci.sh -> complete-PageList commit' },
    { title: 'Review', detail: 'adversarial parity review (tests + allocator + tracking)' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const GATES = R + '/.port/tools/gates'
const G3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + G + '/src/terminal/PageList.zig --port-glob "' + R + '/crates/rustty-vt/src/pagelist/*.rs"'
const SAN = 'bash ' + GATES + '/sanitizers.sh rustty-vt pagelist address'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. Reference READ-ONLY ' + G + '/src/terminal/PageList.zig. Run gates FOREGROUND (capture+inspect). The allocator was re-architected faithfully (audit task #7: threaded allocator-api2, mmap pageAllocator, DynamicMemoryPoolAligned-over-arena, fallible OOM, lazy-zero, PAGE_SIZE_MIN). Miri exceptions are documented in .port/phase-2/MIRI_EXCEPTIONS.md (23 heavy tests, ASan+tracking-covered). Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const INTEG = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, pagelist_tests: { type: 'integer' }, g3b_missing: { type: 'integer' }, asan_clean: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, tracking_clean: { type: 'boolean' }, ci_pass: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const REVIEW = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['PARITY', 'NOT_PARITY'] }, tautological_or_weakened_tests: { type: 'array', items: { type: 'string' } }, allocator_faithful: { type: 'boolean' }, tracking_sound: { type: 'boolean' }, unsound_unsafe_or_ub: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Integrate')
const integ = await robust(RULES +
  'TASK (integrate PageList vt-3c). LOOP-to-green, all FOREGROUND with captured exit codes: (1) cargo test -p rustty-vt pagelist => all pass (>=210; verify the 205 reference test{} blocks are present — count vs PageList.zig). (2) `' + SAN + '` => ASan on all PageList tests, 0 findings (asan_clean) — confirm the real count ran (>=210 in the lib-test binary, not the doctest 0). (3) cargo +nightly miri test -p rustty-vt on a representative TRACTABLE subset (reset, clone_partial_trimmed_left_reclaims_styles, active_after_grow, the 4 resize_no_reflow UB-fixed tests, a pin + iter + scroll + init_error test) => 0 UB/leak (miri_subset_clean); the 23 heavy tests are exception-documented (do NOT run them under Miri). (4) the page tracking-allocator tests (pagelist_pagepool_no_leak_*) pass (tracking_clean). (5) `' + G3B + '` => missing=0 (g3b_missing). (6) clippy -p rustty-vt --all-targets -D warnings + fmt --check + ' + GATES + '/check_acyclic.sh + ' + R + '/.port/tools/ci.sh (note if ci.sh runs whole-repo Miri and is impractical, run its non-Miri portions + state Miri/ASan verified separately). Keep lib.rs PORT STATUS: stub. Commit "rustty: complete PageList (vt-3c) — 205 reference tests + faithful allocator". GREEN only if pass + g3b_missing=0 + asan_clean + miri_subset_clean + tracking_clean. Return all fields + commit.',
  { schema: INTEG, effort: 'high', label: 'integrate', phase: 'Integrate' })

phase('Review')
const review = await robust(RULES +
  'TASK (adversarial parity review — default NOT_PARITY when uncertain). Integrate: ' + JSON.stringify(integ) + '\n'
  + '1. TESTS: all 205 reference test{} ported, NONE tautological (expected values independent of code under test) or weakened. Sample the hardest: a resize-reflow, scroll, pin-tracking, iterator, clone-reclaims-styles, init_error, an erase point-range test — confirm each asserts what PageList.zig asserts.\n'
  + '2. ALLOCATOR RE-ARCHITECTURE (audit task #7) faithfulness vs ' + G + '/src/terminal/PageList.zig + Zig std.heap.MemoryPoolAligned/ArenaAllocator/page_allocator: is the page pool a faithful DynamicMemoryPoolAligned-over-arena on the mmap pageAllocator, threaded like MemoryPool.init(gen_alloc,page_alloc)? Is OOM fallible (no handle_alloc_error on the buffer path; OutOfMemory propagates + reference errdefer cleanup)? Is fresh-page zeroing lazy (mmap, no eager memset) while reused/retained stays zeroed? Is PAGE_SIZE_MIN target-correct? allocator_faithful.\n'
  + '3. TRACKING ALLOCATOR: is the cfg(test) TrackingAllocator a sound, observation-only counting wrapper that genuinely catches page leaks (it was proven by injection)? tracking_sound.\n'
  + '4. Any unsound unsafe / UB across pagelist + the new allocator code (beyond the benign term_size int-to-ptr provenance warning). Re-spot-check Miri on 2-3 tests + read the new unsafe.\n'
  + 'Any tautology/weakening/unfaithful-allocator/unsound-unsafe => NOT_PARITY with specifics. Return verdict + tautological_or_weakened_tests + allocator_faithful + tracking_sound + unsound_unsafe_or_ub + blockers.',
  { schema: REVIEW, effort: 'high', label: 'review', phase: 'Review' })
return { integ, review }
