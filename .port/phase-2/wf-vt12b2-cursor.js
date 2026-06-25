export const meta = {
  name: 'vt12b2-cursor',
  description: 'rustty-vt vt-12b-2: fill Terminal cursor/charset/tabs methods (cursor_up/down/left[104]/right, carriage_return, linefeed, backspace, horizontal_tab[_back], tab_clear*/tab_set/tab_reset, configure_charset/invoke_charset, set_cursor_pos, save_cursor/restore_cursor) + their ~58 tests + the print tests deferred from 12b-1 that these unblock. Scope: leave erase/scroll/index/SGR/modes/semantic/Handler todo!().',
  phases: [
    { title: 'Fill', detail: 'cursor/charset/tab methods + their tests + unblocked deferred print tests' },
    { title: 'Verify', detail: 'scoped G3b + tests + Miri-subset + ASan + faithfulness + no over-reach' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const SCOPED_G3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/Terminal.zig --port-glob "' + R + '/crates/rustty-vt/src/terminal.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/Terminal.zig. vt-12b-1 (print foundation) landed; 1004 tests pass. Plan: ' + R + '/.port/phase-2/TERMINAL_PLAN.md. '
  + 'KEY: Terminal derefs active Screen via active_mut(); Screen cursor API (cursor_up/down/left/right/cursor_absolute/cursor_horizontal_absolute/cursor_reload/save+restore via SavedCursor) all ported. raw *mut via addr_of_mut!. [:0]const u8->Vec<u8>. linefeed calls index() (still todo!() in 12b-3b) — that COMPILES; only linefeed/tab tests that trigger scroll DEFER to 12b-3b (port the non-scrolling ones now). '
  + 'MIRI: per MIRI_EXCEPTIONS.md (restored) run Miri on a SMALL subset of new cursor tests in ISOLATION. PROCESS LESSON (memory): gate on SCOPED G3b of JUST the methods filled this batch (diff vs prior baseline). '
  + 'IMPORTANT: fill ONLY cursor/charset/tab methods (+ any tiny helper they need like Colors::default if a test needs it); leave erase/scroll/index/insert*/delete*/SGR/modes/resize/semantic/Handler-dispatch todo!(). Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, methods_filled: { type: 'array', items: { type: 'string' } }, scoped_g3b_clean: { type: 'boolean' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, scoped_g3b_clean: { type: 'boolean' }, no_overreach: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill Terminal cursor/charset/tabs). In crates/rustty-vt/src/terminal.rs, replace the todo!() bodies of ONLY: cursor_up, cursor_down, cursor_left (ref ~104 loc), cursor_right, carriage_return, linefeed, backspace, horizontal_tab, horizontal_tab_back, tab_clear (current/all variants), tab_set, tab_reset, configure_charset, invoke_charset, set_cursor_pos, save_cursor, restore_cursor (+ Colors::default if a foundation test needs it) — faithful ports from ' + T + '/Terminal.zig. Then PORT: the ~58 cursor/charset/tab tests + the print tests deferred from 12b-1 that these unblock (setCursorPos/cursorRight-using ones). DEFER tests that still need a todo!() method (erase/scroll/index/SGR/modes/semantic) to their sub-batch. Leave all other methods todo!(). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (your tests pass + 1004 prior no regression); SCOPED G3b `' + SCOPED_G3B + '` — confirm 0 missing attributable to the methods you filled (diff vs the vt-12b-1 baseline; the remaining missing are still-todo!() methods + deferred tests); cargo +nightly miri test -p rustty-vt on 2-3 new cursor tests (ISOLATED, 0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` (ASan clean); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: Terminal cursor/charset/tabs + tests (vt-12b-2)". Return ok+tests_passing+methods_filled+scoped_g3b_clean+builds+commit+notes+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, methods_filled: fill.methods_filled, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (cursor/charset/tab tests pass + no regression vs 1004); cargo +nightly miri test -p rustty-vt on ~3 cursor tests (ISOLATED, 0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. SCOPED G3b (`' + SCOPED_G3B + '`): confirm the newly-filled cursor/charset/tab methods drop 0 comments (scoped_g3b_clean); remaining missing must be still-todo!() methods, NOT dropped comments in the filled ones. NO OVER-REACH: only cursor/charset/tab filled; erase/scroll/index/SGR/modes/semantic/Handler still todo!() (no_overreach). Spot-check 3 tests (a cursor_left with reverse-wrap, a tab-stop, a charset-invoke) faithful vs Terminal.zig, not tautological. Verdict GREEN only if build + tests + Miri-subset + ASan + scoped_g3b_clean + clippy + fmt green AND no_overreach AND no regression. Report verdict + tests_passing + miri_subset_clean + asan_clean + scoped_g3b_clean + no_overreach + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
