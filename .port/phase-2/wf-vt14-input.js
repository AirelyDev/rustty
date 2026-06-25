export const meta = {
  name: 'vt14-input',
  description: 'rustty-vt vt-14: the VT input slice (input/{mouse,paste,key,mouse_encode,key_encode} + function_keys). ~131 tests. Deps ready: Terminal/kitty/kitty.key (done), config->rustty-config, renderer/size->rustty-render-types. Seams: dcimgui Key::imguiKey -> #[cfg(feature="imgui")] (GUI-glue, deferred to apprt). Port ALL tests (count gate enforces). key_encode is the big one (92 tests, 2540 loc).',
  phases: [
    { title: 'Leaves', detail: 'input/mouse(1) + input/paste(10) + function_keys(1) + input/key(4, imguiKey feature-gated)' },
    { title: 'MouseEncode', detail: 'input/mouse_encode(23) (needs key/mouse/render-size)' },
    { title: 'KeyEncode', detail: 'input/key_encode(92) (needs key/kitty/Terminal/config/function_keys)' },
    { title: 'Verify', detail: 'count-gate(input) + tests + Miri-subset + ASan + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const IN = G + '/src/input'
const GATES = R + '/.port/tools/gates'
const G3B = (zig, rs) => 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + zig + ' --port-glob "' + R + '/crates/rustty-vt/src/input/' + rs + '"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT — check_test_counts.py enforces); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. Reference READ-ONLY ' + IN + '/. The VT cluster is DONE; 1654 tests pass. Rust input slice = crates/rustty-vt/src/input/{key,key_encode,mouse,mouse_encode,paste}.rs (stubs) + add function_keys.rs. Plan: ' + R + '/.port/phase-2/VT_SUBBATCH_PLAN.md. '
  + 'KEY/SEAMS: config.zig -> rustty-config (exists); renderer/size.zig -> rustty-render-types (exists); kitty + kitty/key (done); function_keys.zig -> new crates/rustty-vt/src/input/function_keys.rs (315 loc sibling, port it). dcimgui: Key::imguiKey (the Key->cimgui-constant mapping, GUI-glue per R3) -> gate behind #[cfg(feature="imgui")] (OFF by default; dcimgui binding is an apprt-layer dep, DEFERRED — record in DEFERRALS.md); its test (if any) becomes #[cfg(feature="imgui")] (SkipZigTest pattern). All OTHER key/key_encode/mouse_encode logic is VT-relevant — port it fully. [:0]const u8->Vec<u8>. SOUNDNESS RULE (memory): raw-ptr discipline. '
  + 'MIRI: per MIRI_EXCEPTIONS.md Miri-subset in ISOLATION. PROCESS LESSONS (memory): port ALL tests (count gate enforces), scoped-G3b. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_gate_ok: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Leaves')
const leaves = await robust(RULES +
  'TASK (input leaves + key). Port FULLY (every test): ' + IN + '/mouse.zig (1t) -> input/mouse.rs; ' + IN + '/paste.zig (10t, needs Terminal) -> input/paste.rs; ' + IN + '/function_keys.zig (315 loc, 1t) -> input/function_keys.rs (new, wire into input/mod.rs); ' + IN + '/key.zig (4t) -> input/key.rs — port the Key enum + mods + all VT-relevant methods; gate Key::imgui_key behind #[cfg(feature="imgui")] (add the cargo feature; dcimgui deferred to apprt — record in DEFERRALS.md) + its test #[cfg(feature="imgui")]. GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (new tests pass + no regression vs 1654); cargo +nightly miri test on 2 new tests ISOLATED (0 UB/leak); clippy --all-targets -D; fmt --check; ' + G3B(IN + '/mouse.zig', 'mouse.rs') + ' + key/paste/function_keys G3b missing=0. Commit "rustty: vt input leaves + key (vt-14)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'leaves', phase: 'Leaves' })

phase('MouseEncode')
const me = await robust(RULES + 'Leaves landed: ' + JSON.stringify(leaves && leaves.commit) + '\n'
  + 'TASK (input/mouse_encode, 23 tests). Port ' + IN + '/mouse_encode.zig FULLY into input/mouse_encode.rs (needs key/mouse [done] + renderer/size->rustty-render-types). GATES FOREGROUND: build; cargo test -p rustty-vt mouse_encode (23 pass + no regression); Miri-subset 2 tests ISOLATED; clippy -D; fmt; ' + G3B(IN + '/mouse_encode.zig', 'mouse_encode.rs') + ' missing=0. Commit "rustty: vt input mouse_encode (vt-14)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'mouse_encode', phase: 'MouseEncode' })

phase('KeyEncode')
const ke = await robust(RULES + 'Prior: ' + JSON.stringify([leaves, me].map(s => s && s.commit)) + '\n'
  + 'TASK (input/key_encode, 92 tests, 2540 loc — the big one). Port ' + IN + '/key_encode.zig FULLY into input/key_encode.rs (needs key/kitty/kitty.key/Terminal/config[rustty-config]/function_keys — all available). Port ALL 92 test{} blocks assertion-for-assertion. GATES FOREGROUND: build; cargo test -p rustty-vt key_encode (92 pass + no regression); Miri-subset 3 tests ISOLATED; clippy -D; fmt; ' + G3B(IN + '/key_encode.zig', 'key_encode.rs') + ' missing=0. Commit "rustty: vt input key_encode + 92 tests (vt-14)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'key_encode', phase: 'KeyEncode' })

phase('Verify')
const verify = await robust(RULES + 'Phases: ' + JSON.stringify([leaves, me, ke].map(s => s && s.commit)) + '\n'
  + 'TASK (independent verify — default RED). Confirm FOREGROUND: cargo build --workspace; cargo test -p rustty-vt (input mouse 1 + paste 10 + function_keys 1 + key 3-4 + mouse_encode 23 + key_encode 92 pass + no regression vs 1654); per-file COUNT check (each input .zig test{} count == its .rs #[test] count, modulo the imgui-gated key test) — add input modules to check_test_counts.py or count directly; cargo +nightly miri test on ~4 input tests ISOLATED (0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt key_encode address` + `... mouse_encode address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh; G3b on each input .zig missing=0 (modulo the imgui-gated imguiKey). Spot-check 3 tests (a key_encode kitty-protocol, a mouse_encode SGR, a paste bracketed) faithful, not tautological. Confirm imguiKey cleanly feature-gated (recorded in DEFERRALS). Verdict GREEN only if per-file counts met + build + tests + Miri-subset + ASan + g3b=0 + clippy + fmt green AND no regression. Report verdict + tests_passing + count_gate_ok + miri_subset_clean + asan_clean + g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { leaves, me, ke, verify }
