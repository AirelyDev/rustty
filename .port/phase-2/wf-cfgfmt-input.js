export const meta = {
  name: 'cfgfmt-input',
  description: 'Config-cluster C0 + the next chokepoint: rustty-cfg-fmt (config/formatter.zig 8t leaf) then rustty-input (PORT_PLAN #18): Binding 85t (keybind grammar) + KeymapDarwin/command/keycodes/Link/KeymapNoop/keyboard/helpgen_actions + OptionAsAlt, and complete the deferred key_mods RemapSet/translation (frees the 23 deferred vt key_mods tests). Deps ready: vt/datastruct/util/cfg-fmt/render-types.',
  phases: [
    { title: 'CfgFmt', detail: 'config/formatter.zig (8t) -> rustty-cfg-fmt (leaf below config+input)' },
    { title: 'Binding', detail: 'input/Binding.zig (85t, 4882loc — the keybind grammar) -> rustty-input' },
    { title: 'InputRest', detail: 'KeymapDarwin(2)+command(1)+keycodes(1)+Link+KeymapNoop+keyboard+helpgen_actions+OptionAsAlt' },
    { title: 'KeyMods', detail: 'complete key_mods RemapSet/translation (the 23 deferred vt tests) in rustty-input' },
    { title: 'Verify', detail: 'count-gate + tests + Miri + G3b + faithfulness; key_mods deferral resolved' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const IN = G + '/src/input', CF = G + '/src/config'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. R2 = rustty identity. Reference READ-ONLY. Plan: ' + R + '/.port/phase-2/NEXT_PHASE_PLAN.md + the config-scoping panel (Decision B: model comptime-reflection via the rustty-args Target-trait pattern, crates/rustty-args/src/args.rs; NO proc-macro). '
  + 'CRATES: rustty-cfg-fmt (crates/rustty-cfg-fmt/, lib.rs stub — formatter.zig`s key.zig import is DEAD code, Key lives with Config later, so cfg-fmt is a near-leaf); rustty-input (crates/rustty-input/, lib.rs stub; deps rustty-vt/datastruct/util/cfg-fmt/render-types — all PORTED). The vt input slice (key/key_encode/mouse/mouse_encode/paste/function_keys) is DONE in rustty-vt; rustty-input is the REST of input/. [:0]const u8->Vec<u8>; byte-slices &[u8]/Vec<u8>. SOUNDNESS (memory): raw-ptr discipline. comptime field-reflection (parseIntoField/formatEntry) -> the Target-trait dynamic-dispatch pattern (rustty-args precedent), NO proc-macro. '
  + 'MIRI per MIRI_EXCEPTIONS.md ISOLATED. PROCESS LESSONS (memory): port ALL tests (count enforces), scoped-G3b. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_ok: { type: 'boolean' }, key_mods_resolved: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('CfgFmt')
const cf = await robust(RULES +
  'TASK (port cfg-fmt). Port ' + CF + '/formatter.zig (8t, 232loc — the config-value EntryFormatter/formatEntry) FULLY into rustty-cfg-fmt (src/formatter.rs + wire lib.rs). The key.zig import is dead (Key only on the import line) — do not pull Config/Key in. GATES FOREGROUND: cargo build -p rustty-cfg-fmt; cargo test -p rustty-cfg-fmt (8 pass); cargo +nightly miri test on 2 ISOLATED (0 UB/leak); clippy -p rustty-cfg-fmt --all-targets -D; fmt; check_comment_coverage.py scoped to formatter.zig missing=0. Commit "rustty: port rustty-cfg-fmt (formatter, 8 tests)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'cfgfmt', phase: 'CfgFmt' })

phase('Binding')
const bind = await robust(RULES + 'cfg-fmt landed: ' + JSON.stringify(cf && cf.commit) + '\n'
  + 'TASK (the keybind grammar — the big one). Port ' + IN + '/Binding.zig (85t, 4882loc — Trigger/Action/Set parsing of e.g. `ctrl+a>n=new_window`, the full keybind grammar) FULLY into rustty-input (src/binding.rs + sub-modules as needed; wire lib.rs). Needs key/Mods (rustty-vt input — done), command/Action. If Binding is internally mutually-dependent, use the struct-first skeleton->fill discipline (lay types+sigs so it compiles, then fill). GATES FOREGROUND: cargo build -p rustty-input; cargo test -p rustty-input binding (85 pass); Miri-subset 2 ISOLATED; clippy --all-targets -D; fmt; check_comment_coverage.py Binding.zig missing=0. Commit "rustty: port input Binding keybind grammar + 85 tests". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'binding', phase: 'Binding' })

phase('InputRest')
const rest = await robust(RULES + 'Prior: ' + JSON.stringify([cf, bind].map(s => s && s.commit)) + '\n'
  + 'TASK (input leaves). Port FULLY (every test) into rustty-input: ' + IN + '/{KeymapDarwin.zig 2t, command.zig 1t (739loc), keycodes.zig 1t (673loc), Link.zig 0t, KeymapNoop.zig 0t, keyboard.zig 0t, helpgen_actions.zig 0t, config.zig (OptionAsAlt enum, needed by key_mods)}. GATES FOREGROUND: cargo build -p rustty-input; cargo test -p rustty-input (the new pass + no regression); Miri-subset 2 ISOLATED; clippy --all-targets -D; fmt; G3b each missing=0. Commit "rustty: port input KeymapDarwin/command/keycodes/Link/OptionAsAlt + tests". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'inputrest', phase: 'InputRest' })

phase('KeyMods')
const km = await robust(RULES + 'Prior: ' + JSON.stringify([cf, bind, rest].map(s => s && s.commit)) + '\n'
  + 'TASK (complete key_mods — frees the 23 deferred vt tests). The vt key_mods.rs (crates/rustty-vt/src/input/key_mods.rs) is a PARTIAL port: the Mod/alias/RemapSet (parseCLI/parse/finalize/clone/equal/formatEntry/apply/Mask) + Mods.translation (the 23 deferred tests in ' + IN + '/key_mods.zig, lines ~555-899 + the macos-option-as-alt test ~169) were deferred because they need config.OptionAsAlt + the config formatter (cfg-fmt) — now BOTH available. Port the RemapSet + Mod+alias + Mods.translation into rustty-input (src/key_mods.rs, deps OptionAsAlt + cfg-fmt) with ALL 23 deferred tests. Update check_test_counts.py: vt input_key_mods apc_deferred 23->0 split (vt keeps the VT-Mods tests; rustty-input gets the 23 RemapSet/translation). Update DEFERRALS.md (key_mods config-seam RESOLVED). GATES FOREGROUND: cargo build -p rustty-input + -p rustty-vt; cargo test -p rustty-input (23 new pass); cargo test -p rustty-vt (2019, no regression); Miri-subset 2 ISOLATED; clippy -D; fmt; G3b key_mods.zig missing=0. Commit "rustty: complete key_mods RemapSet/translation in rustty-input (frees 23 deferred vt tests)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'keymods', phase: 'KeyMods' })

phase('Verify')
const verify = await robust(RULES + 'Phases: ' + JSON.stringify([cf, bind, rest, km].map(s => s && s.commit)) + '\n'
  + 'TASK (independent verify — default RED). Add rustty-cfg-fmt + rustty-input module mappings to ' + GATES + '/check_test_counts.py (GH bases: src/config for formatter, src/input for the input files). Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py` => cfg-fmt + input modules rust>=zig + key_mods now fully covered (vt + input) + ALL GREEN (count_ok); cargo test -p rustty-cfg-fmt + -p rustty-input (all pass) + -p rustty-vt (2019, no regression); cargo +nightly miri test on ~4 ISOLATED (0 UB/leak); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh (no cycle: input deps vt/cfg-fmt, cfg-fmt is a leaf below); G3b on the ported .zig files missing=0. Confirm the 23 key_mods tests now RUN (key_mods_resolved). Spot-check 3 tests (a Binding trigger>action parse, a RemapSet apply, a formatter entry) faithful, not tautological. Verdict GREEN only if count_ok + key_mods_resolved + build + tests + Miri-subset + g3b=0 + clippy + fmt + acyclic green AND no vt regression. Report verdict + tests_passing + count_ok + key_mods_resolved + miri_subset_clean + g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { cf, bind, rest, km, verify }
