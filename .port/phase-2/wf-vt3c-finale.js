export const meta = {
  name: 'vt3c-finale',
  description: 'Finalize PageList (vt-3c): integrate gate (205 tests + full G3b + clippy/fmt/acyclic + ci.sh; full-pagelist Miri verified separately) then an adversarial parity review (scrutinize the PagePool arena refactor, the rustty-util tripwire enabled() change, init OOM failpoints, and no-tautology sampling). Commits "complete PageList (vt-3c)" on a green integrate.',
  phases: [
    { title: 'Integrate', detail: '205 pass + G3b 0 + clippy/fmt/acyclic + ci.sh -> complete-PageList commit' },
    { title: 'Review', detail: 'adversarial parity review of tests + arena refactor + tripwire change' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const GATES = R + '/.port/tools/gates'
const G3B_FULL = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + G + '/src/terminal/PageList.zig --port-glob "' + R + '/crates/rustty-vt/src/pagelist/*.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. Reference READ-ONLY ' + G + '/src/terminal/PageList.zig. Run gates FOREGROUND (capture+inspect), NEVER background-and-poll. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const INTEG = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, pagelist_tests: { type: 'integer' }, g3b_missing: { type: 'integer' }, ci_pass: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const REVIEW = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['PARITY', 'NOT_PARITY'] }, tautological_or_weakened_tests: { type: 'array', items: { type: 'string' } }, arena_refactor_faithful: { type: 'boolean' }, tripwire_change_sound: { type: 'boolean' }, behavior_or_layout_diffs: { type: 'array', items: { type: 'string' } }, unsound_unsafe_or_ub: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Integrate')
const integ = await robust(RULES +
  'TASK (integrate PageList vt-3c). The full-pagelist Miri gate (cargo +nightly miri test -p rustty-vt pagelist) was run separately and must be 0 UB/0 leak — if you can cheaply re-confirm a couple of pin/clone tests under Miri do so, else trust the separate run and note it. Run FOREGROUND and LOOP-to-green (real exit codes): (1) cargo test -p rustty-vt = exactly 205 pagelist tests present + passing (verify vs PageList.zig 205 test blocks; init_error runs, not ignored). (2) ' + G3B_FULL + ' => missing=0. (3) cargo clippy -p rustty-vt --all-targets -- -D warnings; cargo fmt -p rustty-vt --check; ' + GATES + '/check_acyclic.sh. (4) ' + R + '/.port/tools/ci.sh -> PASS (if ci.sh runs the slow whole-repo Miri and is impractical here, run the non-Miri portions and note Miri was verified separately). Keep lib.rs PORT STATUS: stub (rustty-vt is multi-sub-batch; PageList is one). Commit "rustty: complete PageList (vt-3c) — 205 tests". Return GREEN only if 205 pass + g3b_missing=0 + clippy/fmt/acyclic green; else RED + precise blockers. Report pagelist_tests + g3b_missing + ci_pass + commit.',
  { schema: INTEG, effort: 'high', label: 'integrate', phase: 'Integrate' })

phase('Review')
const review = await robust(RULES +
  'TASK (adversarial parity review of PageList vt-3c — default NOT_PARITY when uncertain). Integrate result: ' + JSON.stringify(integ) + '\n'
  + 'Scrutinize against ' + G + '/src/terminal/PageList.zig:\n'
  + '1. TESTS: all 205 test{} ported, NONE tautological (expected values independent of code under test) or weakened/neutered. Sample hard ones: a resize-reflow test, a scroll test, a pin-tracking test, an iterator test, the clone-reclaims-styles test, init_error, an erase point-range test. Confirm each asserts what the reference asserts.\n'
  + '2. PagePool ARENA REFACTOR (pagelist/mod.rs ~line 212, the `arena: Vec<NonNull<u8>>` model added by the init-OOM work): is it a FAITHFUL behavioral equivalent of the reference MemoryPoolAligned([std_size]u8, page_align) + arena.child_allocator (std_size page-aligned free-list reused LIFO; non-standard pages alloc/freed individually; reset retains up to retain_with_limit AND zeroes retained buffers)? Any leak, double-free, use-after-free, or unsound unsafe? Re-run cargo +nightly miri test -p rustty-vt for a couple of grow/reset/clone tests to spot-check.\n'
  + '3. TRIPWIRE CHANGE (crates/rustty-util/src/tripwire.rs enabled() cfg!(test)->cfg!(debug_assertions)): is this a sound cross-crate equivalent of Zig `enabled = builtin.is_test`? Confirm cargo test -p rustty-util passes (4 tripwire tests). Does it alter production behavior (failpoints are #[cfg(test)]-gated at call sites, so should be inert in non-test builds — verify)?\n'
  + '4. init OOM failpoints + init_pages non-std cleanup: faithful to reference init_tw/initPages_tw + the errdefer? G3b 0 missing (no dropped comments).\n'
  + 'Any tautology/weakening/behavior-diff/unsound-unsafe/UB/missing => NOT_PARITY with specifics. Return verdict + tautological_or_weakened_tests + arena_refactor_faithful + tripwire_change_sound + behavior_or_layout_diffs + unsound_unsafe_or_ub + blockers.',
  { schema: REVIEW, effort: 'high', label: 'review', phase: 'Review' })
return { integ, review }
