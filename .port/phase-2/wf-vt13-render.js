export const meta = {
  name: 'vt13-render',
  description: 'rustty-vt vt-13: render (15 tests) + SelectionGesture (31 tests) + finalize highlight Tracked/track (deferred seam) — all now unblocked by the completed Screen/Terminal cluster. ~46 tests. Per-module: port ALL reference tests (test-count gate enforces), full gate set.',
  phases: [
    { title: 'Render', detail: 'render.zig -> render.rs + highlight Tracked/track + 15 tests' },
    { title: 'SelectionGesture', detail: 'SelectionGesture.zig -> selection_gesture.rs + 31 tests' },
    { title: 'Verify', detail: 'count-gate (render+gesture) + tests + Miri-subset + ASan + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const G3B = (zig, rs) => 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/' + zig + ' --port-glob "' + R + '/crates/rustty-vt/src/' + rs + '"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity: PORT EVERY test{} block (the count is a CONTRACT — check_test_counts.py enforces rust>=zig); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. Reference READ-ONLY. The full Screen/Terminal/Selection/ScreenSet/PageList cluster is DONE; 1608 tests pass. Plan: ' + R + '/.port/phase-2/VT_SUBBATCH_PLAN.md. '
  + 'KEY: render formats the screen for the renderer (uses Screen/PageList/Selection/color/cursor/highlight/page/point/size — all done); finalize highlight.rs Tracked/track (the deferred seam — needs Screen, now available). SelectionGesture handles click/drag selection (uses Screen/ScreenSet/Selection/Terminal — all done). SOUNDNESS RULE (memory): no &mut Page/Row/Cell args re-read across assert_integrity -> raw ptrs. [:0]const u8->Vec<u8>. '
  + 'MIRI: per MIRI_EXCEPTIONS.md (restored) Miri-subset on new tests in ISOLATION. PROCESS LESSONS (memory): port ALL tests (not representative) — the count gate enforces; scoped-G3b of just-filled methods. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, count_ok: { type: 'boolean' }, g3b_missing: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_gate_ok: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Render')
const render = await robust(RULES +
  'TASK (port render). Port ' + T + '/render.zig (1465 loc, 15 tests) into crates/rustty-vt/src/render.rs (currently stub) FULLY — every method + ALL 15 test{} blocks. Also finalize the DEFERRED highlight Tracked/track in crates/rustty-vt/src/highlight.rs (needs Screen, now available). GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt render (15 pass + no regression vs 1608); cargo +nightly miri test -p rustty-vt on 2 render tests (ISOLATED, 0 UB/leak); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check; ' + G3B('render.zig', 'render.rs') + ' missing=0. Commit "rustty: port render + highlight Tracked (vt-13)". Return ok+tests_added+count_ok+g3b_missing+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'render', phase: 'Render' })

phase('SelectionGesture')
const gesture = await robust(RULES + 'Render landed: ' + JSON.stringify(render && { commit: render.commit, blockers: render.blockers }) + '\n'
  + 'TASK (port SelectionGesture). Port ' + T + '/SelectionGesture.zig (2024 loc, 31 tests) into crates/rustty-vt/src/selection_gesture.rs (stub) FULLY — every method + ALL 31 test{} blocks (click/drag/word/line selection gestures over Screen+Selection). GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt selection_gesture (31 pass + no regression); cargo +nightly miri test -p rustty-vt on 2 gesture tests (ISOLATED, 0 UB/leak); clippy --all-targets -D; fmt --check; ' + G3B('SelectionGesture.zig', 'selection_gesture.rs') + ' missing=0. Commit "rustty: port SelectionGesture (vt-13)". Return ok+tests_added+count_ok+g3b_missing+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'gesture', phase: 'SelectionGesture' })

phase('Verify')
const verify = await robust(RULES + 'render: ' + JSON.stringify(render && render.commit) + ' gesture: ' + JSON.stringify(gesture && gesture.commit) + '\n'
  + 'TASK (independent verify — default RED). Confirm FOREGROUND: cargo build -p rustty-vt; `python3 ' + GATES + '/check_test_counts.py` => render + selection_gesture (+ all) rust>=zig (count_gate_ok — add render/SelectionGesture mappings if absent, or count directly: render.zig=15 vs render.rs #[test]; SelectionGesture.zig=31 vs selection_gesture.rs #[test]); cargo test -p rustty-vt (render 15 + gesture 31 pass + no regression vs 1608); cargo +nightly miri test -p rustty-vt on ~3 new tests (ISOLATED, 0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt render address` + `... selection_gesture address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. ' + G3B('render.zig', 'render.rs') + ' + ' + G3B('SelectionGesture.zig', 'selection_gesture.rs') + ' => missing=0 each. Spot-check 3 tests (a render selection/cursor, a gesture drag, a gesture word-select) faithful, not tautological. Verdict GREEN only if count_gate_ok (render=15, gesture=31) AND build + tests + Miri-subset + ASan + g3b=0 + clippy + fmt green AND no regression. Report verdict + tests_passing + count_gate_ok + miri_subset_clean + asan_clean + g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { render, gesture, verify }
