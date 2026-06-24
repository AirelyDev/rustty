export const meta = {
  name: 'vt11f-clone-clear',
  description: 'rustty-vt vt-11f: fill Screen clone (with tracked-pin remap for cursor/selection/saved_cursor via TrackedPinsRemap — now Selection is done) + clearing (clear_rows, erase_history, erase_active, split_cell_boundary) + their ~31 tests (clone ~19, clear-history ~5, clearRows/eraseRows ~7). Leaves only semantic/prompt_click* + StringMap + ScreenSet for vt-11g.',
  phases: [
    { title: 'Fill', detail: 'Screen clone + clearing methods + ~31 tests' },
    { title: 'Verify', detail: 'ported tests + ASan + clippy/fmt + faithfulness + no over-reach' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/Screen.zig. Screen core/dump/resize/select* + the full Selection module landed; 946 tests pass. '
  + 'KEY: clone deep-copies the screen + REMAPS tracked pins (cursor.page_pin, selection Tracked pins, saved_cursor) via a TrackedPinsRemap = HashMap<*mut Pin, *mut Pin> (pagelist access.rs:721 has the analogue; PageList::clone gives the old->new pin map). Selection is done so clone can remap its tracked bounds. clearing uses pages + clearCells (filled in vt-11a). RULES: *mut Pin faithful; addr_of_mut! for raw page ptrs; quirks.inlineAssert->debug_assert!. '
  + 'MIRI: per MIRI_EXCEPTIONS.md rustty-vt Miri compile is intractable — gate on ASan + addr_of_mut! discipline; do NOT run per-sub-batch Miri. clone is the highest raw-pointer-remap risk -> lean on ASan + careful addr_of_mut!. '
  + 'IMPORTANT: fill ONLY clone + clear_rows + erase_history + erase_active + split_cell_boundary; leave semantic/prompt_click_move/prompt_click_line todo!() (vt-11g). Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, asan_clean: { type: 'boolean' }, no_overreach: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill Screen clone + clearing). In crates/rustty-vt/src/screen.rs, replace the todo!() bodies of clone, clear_rows, erase_history, erase_active, split_cell_boundary with faithful ports from ' + T + '/Screen.zig. clone: deep-clone via pages.clone, then remap cursor.page_pin / selection Tracked pins / saved_cursor through the TrackedPinsRemap the PageList clone produces; faithfully rebuild the cloned cursor pointers (page_row/page_cell) via cursorReload-style logic. Then PORT the tests: clone (~19, incl. the 9 that exercise selection/cursor pin remap — now Selection is done), clear-history (~5), clearRows/eraseRows (~7). Leave semantic/prompt_click* todo!(). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt screen (your tests pass + 946 prior no regression); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` (ASan clean — clone is the highest pointer-remap risk); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: Screen clone + clearing + tests (vt-11f)". Return ok+tests_passing+builds+commit+notes(tests count, clone remap approach)+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (clone+clearing tests pass + no regression vs 946); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` (ASan clean — SCRUTINIZE clone: the tracked-pin remap is the top UAF/dangling-pointer risk; confirm cloned cursor/selection pins point into the CLONED pages, not the original); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. NO OVER-REACH: only clone + clearing filled; semantic/prompt_click* still todo!() (no_overreach). Spot-check 3 tests (a clone-with-selection-remap, a clone-with-cursor-on-later-page, a clearRows/eraseHistory) faithful vs Screen.zig (cloned content + pin identity), not tautological. (Per MIRI_EXCEPTIONS.md do NOT run per-sub-batch Miri.) Verdict GREEN only if build + tests + ASan + clippy + fmt green AND no_overreach AND no regression. Report verdict + tests_passing + asan_clean + no_overreach + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
