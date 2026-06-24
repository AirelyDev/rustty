export const meta = {
  name: 'vt9-stream',
  description: 'rustty-vt sub-batch vt-9: stream.zig (3449 loc, 38 tests) — the VT byte->action Stream processor, generic Stream<Handler>. Skeleton-first: Handler trait (default no-op methods mirroring Zig @hasDecl optional dispatch) + Stream + action router with dispatch stubbed -> fill csi/osc/esc/dcs/apc dispatch -> TestHandler + 38 tests -> verify. All deps (Parser/osc/kitty/mouse/UTF8Decoder/sgr/ansi/csi/modes/charsets/device_*) done.',
  phases: [
    { title: 'Skeleton', detail: 'Handler trait + Stream<H> + next/nextSlice + action router (dispatch stubbed) -> builds' },
    { title: 'Dispatch', detail: 'fill csiDispatch + oscDispatch + escDispatch + dcs/apc handling' },
    { title: 'Tests+Verify', detail: 'TestHandler + 38 tests, then Miri + ASan + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const G3BS = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/stream.zig --port-glob "' + R + '/crates/rustty-vt/src/stream.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity: EVERY line; EVERY comment VERBATIM (retarget only, R2); EVERY test{} as #[test] ASSERTION-FOR-ASSERTION, INDEPENDENT expected values (no tautology); nothing dropped/weakened. Reference READ-ONLY ' + T + '/stream.zig (stream.rs is a stub). All deps DONE: crate::parser::Parser (+Action), crate::osc::Command, crate::{ansi,charsets,csi,device_attributes,device_status,modes,sgr,mouse}, crate::kitty, crate::utf8_decoder. '
  + 'KEY ADAPTATION: Zig `Stream(comptime Handler)` calls handler methods guarded by `@hasDecl(Handler, "x")` (optional dispatch). Port as a Rust `trait Handler` with a DEFAULT no-op impl for EVERY callback (so a Handler overrides only what it handles, faithfully mirroring @hasDecl). Stream<H: Handler>. Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, stream_tests_passing: { type: 'integer' }, miri_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Skeleton')
const skel = await robust(RULES +
  'TASK (skeleton). In crates/rustty-vt/src/stream.rs: define the `Handler` trait with a DEFAULT no-op method for EVERY handler callback stream.zig references via @hasDecl (print, setActivePwd, csiDispatch-targets like setCursorLeft/eraseDisplay/.., escDispatch-targets, oscDispatch-targets, dcsHook/dcsPut/dcsUnhook, apc*, etc. — read stream.zig + list them ALL faithfully). Define `Stream<H: Handler>` (handler + parser: crate::parser::Parser fields), init/deinit, and next(c)/nextSlice(bytes) which drive the Parser and route each Action to a dispatch method. Stub csiDispatch/oscDispatch/escDispatch/dcsHook bodies as `todo!()`/minimal so the crate BUILDS (the Dispatch phase fills them). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt (green, dispatch stubbed); clippy -p rustty-vt -- -D warnings (allow todo!); fmt --check. Commit "rustty: vt stream skeleton — Handler trait + Stream + router (vt-9)". Return ok+builds+commit+notes(the full Handler method list so the dispatch phase matches it)+blockers.',
  { schema: STEP, effort: 'high', label: 'skeleton', phase: 'Skeleton' })

phase('Dispatch')
const disp = await robust(RULES +
  'Skeleton landed: ' + JSON.stringify(skel && skel.notes ? skel.notes.slice(0, 500) : '') + '\n'
  + 'TASK (fill dispatch). Replace the stubbed dispatch in stream.rs with the FAITHFUL ports from stream.zig: csiDispatch (the full CSI-command -> Handler-method switch — every final byte + intermediates + params mapping), oscDispatch (osc::Command variant -> Handler mapping), escDispatch (ESC final-byte mapping), and dcsHook/dcsPut/dcsUnhook + apcStart/apcPut/apcEnd + sosPmApcString. EVERY comment verbatim. No test{} yet. '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; clippy -p rustty-vt --all-targets -- -D warnings (no remaining todo! in dispatch); fmt --check; ' + G3BS + ' => missing=0 (production comments). Commit "rustty: vt stream dispatch — csi/osc/esc/dcs/apc (vt-9)". Return ok+builds+commit+blockers+notes.',
  { schema: STEP, effort: 'high', label: 'dispatch', phase: 'Dispatch' })

phase('Tests+Verify')
const tests = await robust(RULES +
  'TASK (tests). Port stream.zig\'s TestHandler (the test handler struct capturing dispatched calls) + ALL 38 test{} blocks into stream.rs as #[test], assertion-for-assertion with INDEPENDENT expected values. The TestHandler impls the Handler trait overriding the methods the tests observe. GATES FOREGROUND: cargo build; cargo test -p rustty-vt stream => 38 pass + no regression; clippy --all-targets -D; fmt --check; ' + G3BS + ' => missing=0. Commit "rustty: vt stream TestHandler + 38 tests (vt-9)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'tests', phase: 'Tests+Verify' })

const verify = await robust(RULES +
  'TASK (independent verify — default RED). Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (stream 38 pass; full suite no regression — 781 + 38); cargo +nightly miri test -p rustty-vt on ~4 stream tests (0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt stream address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. ' + G3BS + ' => missing=0. FAITHFULNESS: the Handler trait has a default for every @hasDecl-optional method (no callback silently unhandled); spot-check 3 tests (a csiDispatch, an oscDispatch, an escDispatch) assert what stream.zig asserts, not tautological. Verdict GREEN only if all green AND no dispatch stub/todo left AND g3b_missing=0. Report verdict + stream_tests_passing + miri_clean + asan_clean + g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Tests+Verify' })
return { skel, disp, tests, verify }
