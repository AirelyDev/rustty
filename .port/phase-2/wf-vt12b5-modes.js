export const meta = {
  name: 'vt12b5-modes',
  description: 'rustty-vt vt-12b-5: fill Terminal modes/resize/screen-switch (deccolm, resize, switch_screen, switch_screen_mode[96], full_reset) + their ~26 tests + tests deferred earlier that these unblock. resize coordinates the done Screen::resize; switch_screen does primary<->alt swap. Leaves semantic/pwd/title + Handler-dispatch todo!().',
  phases: [
    { title: 'Fill', detail: 'Terminal modes/resize/switch methods + their tests' },
    { title: 'Verify', detail: 'scoped G3b + tests + Miri-subset + ASan + faithfulness + no over-reach' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const SCOPED_G3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/Terminal.zig --port-glob "' + R + '/crates/rustty-vt/src/terminal.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/Terminal.zig. vt-12b-1..4 landed; 1144 tests pass. Plan: ' + R + '/.port/phase-2/TERMINAL_PLAN.md. '
  + 'KEY: resize coordinates the active Screen resize (Screen::resize is DONE) + alt-screen + tabstops + scrolling_region reset; switch_screen swaps the ScreenSet active (primary<->alt) via screens (ScreenSet ported, active:*mut Screen); switch_screen_mode (~96 loc) handles 47/1047/1049 save/restore; deccolm 80<->132; full_reset is RIS (re-init most state). modes::ModeState ported. SOUNDNESS RULE (memory): no &mut Page/Row/[Cell] args held across assert_integrity -> raw ptrs. [:0]const u8->Vec<u8>. '
  + 'MIRI: per MIRI_EXCEPTIONS.md (restored) Miri-subset on new tests in ISOLATION (switch_screen swaps *mut Screen -> SB risk). PROCESS LESSON: SCOPED G3b of JUST the methods filled this batch (a single dropped comment fails). '
  + 'IMPORTANT: fill ONLY deccolm/resize/switch_screen/switch_screen_mode/full_reset; leave semantic_prompt/semantic_prompt_fresh_line/cursor_is_at_prompt/set+get_pwd/title/is_dirty/clear_dirty(if test-only already done)/kitty-glyph handlers/Handler-dispatch todo!(). Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, methods_filled: { type: 'array', items: { type: 'string' } }, scoped_g3b_clean: { type: 'boolean' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, scoped_g3b_clean: { type: 'boolean' }, no_overreach: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill Terminal modes/resize/switch). In crates/rustty-vt/src/terminal.rs, replace the todo!() bodies of ONLY: deccolm, resize, switch_screen, switch_screen_mode, full_reset — faithful ports from ' + T + '/Terminal.zig. Then PORT the modes/resize/switch tests + any tests deferred earlier these unblock. DEFER tests needing still-todo!() methods (semantic/pwd/Handler). Leave all other methods todo!(). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (your tests pass + 1144 prior no regression); SCOPED G3b `' + SCOPED_G3B + '` — 0 missing attributable to the filled methods (diff vs baseline); cargo +nightly miri test -p rustty-vt on 2-3 new tests (ISOLATED, 0 UB/leak — switch_screen swapping *mut Screen + resize page churn are the risks); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` (ASan clean); the page-pool tracking allocator on a resize-grow test; clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: Terminal modes/resize/switch + tests (vt-12b-5)". Return ok+tests_passing+methods_filled+scoped_g3b_clean+builds+commit+notes+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, methods_filled: fill.methods_filled, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (no regression vs 1144); cargo +nightly miri test -p rustty-vt on ~3 new tests (ISOLATED, 0 UB/leak — switch_screen *mut Screen swap + resize is the top risk; the "strongly-protected" SB class must be ABSENT); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. SCOPED G3b: filled methods drop 0 comments (scoped_g3b_clean). NO OVER-REACH: only modes/resize/switch filled; semantic/pwd/title/Handler still todo!() (no_overreach). Spot-check 3 tests (a resize-grow-with-reflow, a switch-to-alt-and-back, a full_reset) faithful vs Terminal.zig, not tautological. Verdict GREEN only if build + tests + Miri-subset + ASan + scoped_g3b_clean + clippy + fmt green AND no_overreach AND no regression. Report verdict + tests_passing + miri_subset_clean + asan_clean + scoped_g3b_clean + no_overreach + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
