export const meta = {
  name: 'screen-tests-recovery',
  description: 'Recover the 12 Screen.zig reference tests dropped during the Screen-SCC (vt-11) + missed by vt-12c-screen (it ported its 23-deferred list but more were missing). screen.rs has 176 of Screen.zig\'s 188. All Screen methods are filled, so all 12 are portable. Verified by the test-count gate (screen drop=12).',
  phases: [
    { title: 'Port', detail: 'port the 12 missing Screen tests (scroll/scrollback/scroll_clear/select/hyperlink)' },
    { title: 'Verify', detail: 'test-count gate screen ok + tests + Miri-subset + ASan + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const TCG = 'python3 ' + GATES + '/check_test_counts.py screen'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1: test count is a CONTRACT (188 Screen test{} is non-negotiable). EVERY ref test{} as #[test], assertion-for-assertion, INDEPENDENT expected values, comments verbatim. Reference READ-ONLY ' + T + '/Screen.zig. All Screen methods ALREADY FILLED; 1417 tests pass (terminal recovery is concurrent on terminal.rs — you only touch screen.rs). SOUNDNESS RULE (memory): no &mut Page/Row/Cell args re-read across assert_integrity -> raw ptrs. '
  + 'MIRI: per MIRI_EXCEPTIONS.md Miri-subset on a couple new tests in ISOLATION. You ONLY ADD tests to screen.rs (no method-body changes). Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, screen_count: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, screen_count_ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Port')
const port = await robust(RULES +
  'TASK (recover the 12 dropped Screen tests). Port these Screen.zig test{} blocks (currently absent from crates/rustty-vt/src/screen.rs) as #[test], assertion-for-assertion: "scrolling with a single-row screen no scrollback", "scrolling across pages preserves style", "scrollback with multi-row delta", "scrollback empty", "scrolling moves selection", "scrolling moves viewport", "scroll and clear full screen", "scroll and clear partial screen", "scroll and clear empty screen", "scroll and clear ignore blank lines", "select untracked", "hyperlink start/end". Find each by grepping ' + T + '/Screen.zig for `^test "Screen: <name>"`. Verify each is genuinely absent (normalized fn-name diff) before porting — do NOT duplicate an existing test under a different name. '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt screen (your new tests pass + no regression); `' + TCG + '` => screen ok (rust >= 188); cargo +nightly miri test -p rustty-vt on 2 new tests (ISOLATED, 0 UB/leak — scrolling moves selection touches Selection tracked pins); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: recover the 12 dropped Screen tests (screen test-count -> 188)". Return ok+tests_added+screen_count+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'port', phase: 'Port' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Port: ' + JSON.stringify(port && { commit: port.commit, screen_count: port.screen_count, blockers: port.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; `' + TCG + '` => screen ok (rust >= 188 — the contract met); cargo test -p rustty-vt (no regression); cargo +nightly miri test -p rustty-vt on ~2 new tests (ISOLATED, 0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. Spot-check 3 recovered tests (a scroll-and-clear, scrolling-moves-selection, hyperlink-start/end) faithful vs Screen.zig, not tautological. Verdict GREEN only if screen test-count gate ok AND build + tests + Miri-subset + ASan + clippy + fmt green AND no regression. Report verdict + screen_count_ok + tests_passing + miri_subset_clean + asan_clean + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { port, verify }
