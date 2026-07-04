export const meta = {
  name: 'config-c3-capi',
  description: 'Config cluster C3 (the tail): config/CApi.zig (8t, the config C-ABI: rustty_config_new/free/load/finalize/get/diagnostics) + c_get.zig (6t, typed get) + Wasm.zig (0t, browser-gated). Builds on the done Config struct. ~14 tests. Completes the config cluster.',
  phases: [
    { title: 'Port', detail: 'CApi.zig + c_get.zig + Wasm.zig -> rustty-config (C-ABI exports)' },
    { title: 'Verify', detail: 'count-gate (config CApi/c_get) + tests + Miri/ASan + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const CF = G + '/src/config'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. R2 = rustty identity (exports rustty_config_*, NOT ghostty_config_*). R3 faithful. Reference READ-ONLY ' + CF + '/. Config C1+C2 DONE in rustty-config (the Config struct + 4 passes + machinery, 169 tests). Deps ready. Plan: ' + R + '/.port/phase-2/NEXT_PHASE_PLAN.md (config C3). '
  + 'KEY: CApi.zig (config C-ABI — config_new/free/load/finalize/get + DiagnosticList C iteration; the panel noted CApi stays IN rustty-config, NOT a separate -c crate) + c_get.zig (the typed config_get over the Key registry). Wasm.zig (0t, browser-gated via isWasm — feature-gate #[cfg(feature="wasm")] OFF or document-defer like the vt wasm exports). Exports = #[no_mangle] pub extern "C" fn rustty_config_* (mirror the lib_vt/lib pattern); cbindgen-visible if a config header is generated. [:0]const u8->Vec<u8>. SOUNDNESS (memory): raw-ptr discipline across the C boundary. '
  + 'MIRI per MIRI_EXCEPTIONS.md ISOLATED (C-boundary heavy -> ASan). Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_ok: { type: 'boolean' }, config_complete: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Port')
const port = await robust(RULES +
  'TASK (config C-ABI tail). Port FULLY (every test) into rustty-config: ' + CF + '/CApi.zig (8t — config_new/free/load/finalize/get + the C DiagnosticList iteration + the rustty_config_* extern fns) -> src/c_api.rs; ' + CF + '/c_get.zig (6t — the typed config_get over the Key registry) -> src/c_get.rs; ' + CF + '/Wasm.zig (0t, browser-gated) -> src/wasm.rs (feature-gate #[cfg(feature="wasm")] OFF + document, like the vt wasm exports). Wire lib.rs. GATES FOREGROUND: cargo build -p rustty-config; cargo test -p rustty-config (CApi 8 + c_get 6 pass + no regression vs 169); Miri-subset 2 ISOLATED (0 UB/leak; C-boundary heavy -> ASan-note); clippy -p rustty-config --all-targets -D; fmt; check_comment_coverage.py scoped to CApi.zig + c_get.zig missing=0. Commit "rustty: config C3 CApi + c_get C-ABI (14 tests) — config cluster complete". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'port', phase: 'Port' })

phase('Verify')
const verify = await robust(RULES + 'Port: ' + JSON.stringify(port && port.commit) + '\n'
  + 'TASK (independent verify — default RED; completes the config cluster). Add config_capi (CApi.zig 8) + config_c_get (c_get.zig 6) mappings to ' + GATES + '/check_test_counts.py (GH base src/config). Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py` => config CApi/c_get rust>=zig + WHOLE gate GREEN (count_ok); cargo test -p rustty-config (CApi 8 + c_get 6 + the rest pass) + no workspace regression (vt 2019); cargo +nightly miri test -p rustty-config ~3 ISOLATED (0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-config c_api address` (ASan clean if C-boundary); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh; G3b CApi.zig + c_get.zig missing=0. Confirm the config cluster is COMPLETE (C1+C2+C3, all config/*.zig ported modulo the documented font/oniguruma/help_strings deferrals) (config_complete). Update NEXT_PHASE_PLAN (config cluster DONE; renderer/apprt/cli/termio tier now unblocked). Spot-check 3 tests (a config_new/get round-trip, a c_get typed get, a diagnostics iteration) faithful, not tautological. Verdict GREEN only if count_ok + build + tests + Miri/ASan + g3b=0 + clippy + fmt + acyclic green AND no regression. Report verdict + tests_passing + count_ok + config_complete + miri_subset_clean + g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { port, verify }
