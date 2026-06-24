export const meta = {
  name: 'vt11c-screen-resize',
  description: 'rustty-vt vt-11c: fill Screen resize + resize_without_reflow (+ the cursor/page coordination helpers; row/cell reflow delegates to the DONE PageList::resize) and port the ~52 resize tests (resize-no-reflow + resize-reflow), now verifiable via dump_string (vt-11b). The largest remaining self-contained Screen test group. Leaves clearRows/clone/select*/semantic todo!() for later.',
  phases: [
    { title: 'Fill', detail: 'Screen resize/resize_without_reflow + helpers + ~52 resize tests' },
    { title: 'Verify', detail: 'ported tests + ASan + clippy/fmt + faithfulness + no over-reach' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/Screen.zig. Screen foundation (vt-11a) + dump/formatter (vt-11b) landed; 839 tests pass. dump_string/dump_string_alloc are available for content assertions. '
  + 'KEY: Screen row/cell REFLOW delegates to crate::pagelist resize (DONE) — Screen::resize is the screen-level coordination (cursor reposition, viewport, selection-pin adjust if a selection is tracked). RULES: [:0]const u8->Vec<u8>; *mut Pin faithful; quirks.inlineAssert->debug_assert!. '
  + 'MIRI: per MIRI_EXCEPTIONS.md the rustty-vt Miri compile is intractable (66+ min) — gate on ASan (sanitizers.sh) + the addr_of_mut! raw-pointer discipline; do NOT run per-sub-batch Miri. '
  + 'IMPORTANT: fill ONLY resize + resize_without_reflow + the helpers they need; leave clearRows/erase_history/erase_active/split_cell_boundary/clone/select*/selection_string/line_iterator/semantic/prompt_click* as todo!(). If a resize test needs a Selection method (tracked-selection adjust), port the MINIMAL Selection method needed (note it) rather than over-reaching. Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, asan_clean: { type: 'boolean' }, no_overreach: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill Screen resize). In crates/rustty-vt/src/screen.rs, replace the todo!() bodies of resize + resize_without_reflow (+ the private helpers they call: cursor reposition after resize, reflowUpdateCursor / the resize cursor pin handling, etc.) with faithful ports from ' + T + '/Screen.zig. Row/cell reflow delegates to crate::pagelist resize (already ported) — port the Screen-level coordination faithfully. Then PORT the resize tests (resize-no-reflow group + resize-reflow group, ~52) as #[test], assertion-for-assertion (assert content via dump_string_alloc + cursor x/y + page structure). Leave all other todo!() Screen methods untouched. '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt screen (your resize tests pass + 839 prior no regression); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` (ASan clean — resize churns pages via pointers); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: Screen resize + reflow coordination + tests (vt-11c)". Return ok+tests_passing+builds+commit+notes(resize tests count + any minimal Selection method pulled in)+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (resize tests pass + no regression vs 839); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. NO OVER-REACH: only resize/resize_without_reflow (+ minimal needed helpers/Selection) filled; clearRows/clone/select*/semantic still todo!() (no_overreach). Spot-check 3 resize tests (a no-reflow shrink, a reflow grow-cols, a reflow that crosses pages) are faithful vs Screen.zig (content via dump matches), not tautological. (Per MIRI_EXCEPTIONS.md do NOT run per-sub-batch Miri.) Verdict GREEN only if build + resize tests + ASan + clippy + fmt green AND no_overreach AND no regression. Report verdict + tests_passing + asan_clean + no_overreach + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
