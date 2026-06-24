export const meta = {
  name: 'vt11a-screen-core',
  description: 'rustty-vt vt-11a: fill Screen CORE FOUNDATION methods (init/deinit/reset, page+cursor accessors, test_write_string, cursor movement, cursorCopy, setAttribute/style) replacing their vt-10 todo!() bodies, + port the tests that exercise only those (read/write, cursor movement, cursor-across-pages, cursorCopy, style). Leaves clearing/scrolling/clone/resize/selection/semantic as todo!() for later vt-11 sub-batches. Whole-Screen G3b deferred to the vt-11 finale.',
  phases: [
    { title: 'Fill', detail: 'Screen foundation methods + their tests; other methods stay todo!()' },
    { title: 'Verify', detail: 'ported tests + Miri + ASan + clippy/fmt + faithfulness + no over-reach' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim on what you port; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/Screen.zig. The Screen-SCC skeleton (vt-10) is landed: all types + todo!() method sigs compile (819 tests pass). '
  + 'Plan: ' + R + '/.port/phase-2/SCREEN_SCC_PLAN.md. Deps ready: PageList (pin tracking: track_pin->*mut Pin stable, point_from_pin, pin), page/style/point/size/ansi/charsets/hyperlink/cursor/osc/rustty-unicode. RULES: Zig [:0]const u8/[]const u8 -> Vec<u8> (never lossy String); []const u21 -> &[u32]; *mut Pin faithful (PinPool stable); fastmem::rotateOnceR -> in-place rotate; quirks.inlineAssert -> debug_assert!; unicode::table::get(cp).width via rustty-unicode. '
  + 'IMPORTANT: fill ONLY the foundation methods named below; leave all OTHER Screen methods as todo!() (later sub-batches) — do not over-reach. Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, methods_filled: { type: 'array', items: { type: 'string' } }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, screen_tests_passing: { type: 'integer' }, miri_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, no_overreach: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill Screen foundation). In crates/rustty-vt/src/screen.rs, replace the todo!() bodies of ONLY these foundation methods with faithful ports from ' + T + '/Screen.zig: '
  + 'init, deinit, reset; the page/cursor accessors + cursor pointer management (cursorReload, cursorCellRight/cursorCellLeft/cursorCellEndOfPrev, the page_pin/page_row/page_cell maintenance, cursorChangePin/cursorSwitchPageList helpers as needed); test_write_string (test-only, ports testWriteString — needs unicode width); cursor movement (cursorUp, cursorDown, cursorLeft, cursorRight, cursorAbsolute, cursorColAbsolute, cursorDownScroll, cursorDownOrScroll, cursorScrollAbove etc.); cursorCopy; setAttribute + the style/cursor-style helpers it needs. '
  + 'Then PORT the tests that pass with ONLY these methods (read/write, cursor movement, cursor-across-pages, cursorCopy [incl the hyperlink subgroup if those helpers are filled], style/setAttribute) as #[test], assertion-for-assertion. Leave clearing/scrolling/clone/resize/select*/selectionString/semantic/promptClickMove/dumpString as todo!() (later sub-batches) — DO NOT fill or test them. '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt screen (your ported tests pass + full suite no regression — 819 prior); cargo +nightly miri test -p rustty-vt on ~3 of your screen tests (0 UB/leak — watch the raw page pointers); clippy -p rustty-vt --all-targets -- -D warnings (the skeleton #[allow]s shrink as methods fill); fmt --check. Commit "rustty: Screen core foundation — init/cursor/accessors + tests (vt-11a)". Return ok+tests_passing+methods_filled+builds+commit+notes+blockers. (Whole-Screen G3b is deferred to the vt-11 finale; do not gate on it now.)',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill result: ' + JSON.stringify(fill && { commit: fill.commit, methods_filled: fill.methods_filled, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (the new Screen foundation tests pass + full suite no regression vs 819 prior); cargo +nightly miri test -p rustty-vt on ~4 screen foundation tests (0 UB/leak — the Cursor page_pin/page_row/page_cell raw pointers + cursorReload are the risk); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. NO OVER-REACH: confirm only foundation methods were filled (clearing/scrolling/clone/resize/select*/semantic still todo!()) — no half-baked method bodies (no_overreach). Spot-check 3 ported tests faithful, not tautological, vs Screen.zig. (Whole-Screen G3b deferred to vt-11 finale.) Verdict GREEN only if build+ported tests+Miri+ASan+clippy+fmt green AND no_overreach AND no regression. Report verdict + screen_tests_passing + miri_clean + asan_clean + no_overreach + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
