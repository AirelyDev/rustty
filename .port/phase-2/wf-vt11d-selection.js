export const meta = {
  name: 'vt11d-selection',
  description: 'rustty-vt vt-11d: finish the Selection module (the remaining methods beyond the 6 dump-path ones vt-11b filled: deinit/eql/start_ptr/end_ptr/track/tracked/ordered/contains/contained_row/contained_row_cached/adjust) + port the 18 Selection tests. Unblocks Screen select* (vt-11e) + clone selection-remap + SelectionGesture (vt-13).',
  phases: [
    { title: 'Fill', detail: 'finish Selection methods + 18 Selection tests' },
    { title: 'Verify', detail: 'ported tests + ASan + clippy/fmt + G3b(Selection.zig) + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const G3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/Selection.zig --port-glob "' + R + '/crates/rustty-vt/src/selection.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/Selection.zig. Screen foundation/dump/resize landed; 892 tests pass. vt-11b already filled Selection init/start/end/order/top_left/bottom_right. '
  + 'KEY: Selection methods take `&Screen`/`&mut Screen` but only read `screen.pages.*` (PageList — done): pin tracking via crate::pagelist track_pin->*mut Pin (stable), untrack_pin, point_from_pin. Bounds::Tracked{start,end:*mut Pin} is faithful. The Selection tests construct a Screen (Screen::init + test_write_string — both done) and assert selection bounds/order/contains/adjust. RULES: *mut Pin faithful; quirks.inlineAssert->debug_assert!. '
  + 'MIRI: per MIRI_EXCEPTIONS.md rustty-vt Miri compile is intractable — gate on ASan (sanitizers.sh) + the addr_of_mut! raw-pointer discipline; do NOT run per-sub-batch Miri. '
  + 'IMPORTANT: finish ONLY Selection (selection.rs); leave Screen select*/selection_string/line_iterator todo!() (vt-11e). Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, selection_tests_passing: { type: 'integer' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, no_overreach: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (finish Selection). In crates/rustty-vt/src/selection.rs, replace the remaining todo!() bodies with faithful ports from ' + T + '/Selection.zig: deinit, eql, start_ptr, end_ptr, track, tracked, ordered, contains, contained_row, contained_row_cached, and adjust (the recursive Adjustment dispatch — port all 10 Adjustment variants faithfully). Then PORT all 18 Selection test{} blocks as #[test], assertion-for-assertion (they build a Screen via Screen::init + test_write_string and assert bounds/order/containment/adjust). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt selection (18 pass + 892 prior no regression); `bash ' + GATES + '/sanitizers.sh rustty-vt selection address` (ASan clean — tracked pins are *mut Pin); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check; ' + G3B + ' => missing=0 (Selection is now fully ported). Commit "rustty: finish Selection module + 18 tests (vt-11d)". Return ok+tests_passing+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (Selection 18 pass + no regression vs 892); `bash ' + GATES + '/sanitizers.sh rustty-vt selection address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. ' + G3B + ' => missing=0 (Selection FULLY ported now — this IS gateable, unlike partial Screen). NO OVER-REACH: Selection complete; Screen select*/selection_string/line_iterator still todo!() (no_overreach). Spot-check 3 Selection tests (an order/ordered, a contains/contained_row, an adjust) faithful vs Selection.zig, not tautological. (Per MIRI_EXCEPTIONS.md do NOT run per-sub-batch Miri.) Verdict GREEN only if build + 18 tests + ASan + clippy + fmt green AND g3b_missing=0 AND no_overreach AND no regression. Report verdict + selection_tests_passing + asan_clean + g3b_missing + no_overreach + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
