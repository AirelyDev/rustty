export const meta = {
  name: 'vt11e-screen-select',
  description: 'rustty-vt vt-11e: fill Screen selection methods (select, select_line, select_all, select_word, select_word_between, select_output, selection_string, line_iterator, LineIterator::next) now that Selection (vt-11d) + formatter dump (vt-11b) are done. Port the ~41 selection tests (selectLine/selectWord/selectAll/selectOutput/selectionString/lineIterator). Leaves clone/clearing/semantic/StringMap/ScreenSet for vt-11f/g.',
  phases: [
    { title: 'Fill', detail: 'Screen select*/selection_string/line_iterator + ~41 selection tests' },
    { title: 'Verify', detail: 'ported tests + ASan + clippy/fmt + faithfulness + no over-reach' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/Screen.zig. Screen foundation/dump/resize + the FULL Selection module landed; 910 tests pass. '
  + 'KEY: select* return Option<Selection> (Selection done); selection_string uses ScreenFormatter (done) + Selection + builds a StringMap via STRUCT LITERAL (no StringMap method calls — StringMap type exists from vt-10); line_iterator yields Selections per line; select_word uses selection_codepoints (done) + unicode width. RULES: [:0]const u8 selectionString return -> Vec<u8> (NUL invariant); []const u21 boundary codepoints -> &[u32]; *mut Pin faithful; quirks.inlineAssert->debug_assert!. '
  + 'MIRI: per MIRI_EXCEPTIONS.md rustty-vt Miri compile is intractable — gate on ASan + addr_of_mut! discipline; do NOT run per-sub-batch Miri. '
  + 'IMPORTANT: fill ONLY select*/selection_string/line_iterator/LineIterator::next; leave Screen clone/clearRows/erase_*/split_cell_boundary/semantic/prompt_click* todo!() (vt-11f/g). If the ONE selectionString map-alloc-failure test needs StringMap methods + the selectionString_tw tripwire, DEFER that single test to vt-11g (note it). Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, asan_clean: { type: 'boolean' }, no_overreach: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill Screen selection methods). In crates/rustty-vt/src/screen.rs, replace the todo!() bodies of select, select_line, select_all, select_word, select_word_between, select_output, selection_string, line_iterator, and LineIterator::next with faithful ports from ' + T + '/Screen.zig. Then PORT the selection tests (selectLine ~17, selectWord ~4, selectAll ~1, selectOutput ~1, selectionString ~16 [defer the 1 map-alloc-failure test to vt-11g if it needs StringMap+tripwire], lineIterator ~2) as #[test], assertion-for-assertion (assert selection bounds + selectionString content via dump). Leave clone/clearRows/erase_*/semantic/prompt_click* todo!(). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt screen (your select tests pass + 910 prior no regression); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` (ASan clean); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: Screen select* + selection_string + line_iterator + tests (vt-11e)". Return ok+tests_passing+builds+commit+notes(tests count + any deferred)+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (select tests pass + no regression vs 910); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. NO OVER-REACH: only select*/selection_string/line_iterator filled; clone/clearRows/erase_*/semantic/prompt_click* still todo!() (no_overreach). Spot-check 3 tests (a selectLine, a selectWord, a selectionString) faithful vs Screen.zig (bounds + dumped content), not tautological. (Per MIRI_EXCEPTIONS.md do NOT run per-sub-batch Miri.) Verdict GREEN only if build + select tests + ASan + clippy + fmt green AND no_overreach AND no regression. Report verdict + tests_passing + asan_clean + no_overreach + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
