export const meta = {
  name: 'vt4-leaves',
  description: 'rustty-vt sub-batch vt-4: port the trivial UNBLOCKED leaf modules (no cycles, all deps already ported): selection_codepoints, cursor, focus, build_options+options, mouse (strip GTK/gobject), UTF8Decoder, Tabstops, dcs. ~21 tests. Faithful (every line/comment, tests assertion-for-assertion), per-module gates + commits, then verify.',
  phases: [
    { title: 'Port', detail: 'one leaf module per step: faithful port + tests + gates + commit' },
    { title: 'Verify', detail: 'all module tests + Miri + ASan + G3b + clippy/fmt + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity: EVERY line ported; EVERY comment VERBATIM (retarget ghostty->rustty only, R2); EVERY test{} as #[test] ASSERTION-FOR-ASSERTION, INDEPENDENT expected values (no tautology); nothing dropped/weakened/stubbed. Reference READ-ONLY. These are LEAF modules — all in-crate deps are already ported (point/style/page/PageList/ansi/charsets/sgr/csi/modes/lib/sys/size — see crates/rustty-vt/src/). Wire each into crates/rustty-vt/src/lib.rs (the stub mod decls likely exist). Keep lib.rs PORT STATUS: stub. '
  + 'Run gates FOREGROUND (capture+inspect). Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, modules_done: { type: 'array', items: { type: 'string' } }, tests_passing: { type: 'integer' }, miri_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Port')
const MODS = [
  { label: 'selection_codepoints', task: 'Port ' + T + '/selection_codepoints.zig (31 loc, 0 tests — const tables) into crates/rustty-vt/src/selection_codepoints.rs. Pure data; faithful.' },
  { label: 'cursor', task: 'Port ' + T + '/cursor.zig (15 loc, 0 tests — the cursor Style enum) into crates/rustty-vt/src/cursor.rs.' },
  { label: 'focus', task: 'Port ' + T + '/focus.zig (39 loc, 2 tests) into crates/rustty-vt/src/focus.rs.' },
  { label: 'options', task: 'Port ' + T + '/build_options.zig (94 loc) into crates/rustty-vt/src/build_options.rs and finalize options.rs (the build-time terminal options: kitty_graphics, oniguruma, tmux_control_mode, etc.). Faithful to the reference build_options + terminal_options. These become the feature/cfg flags later sub-batches gate on.' },
  { label: 'mouse', task: 'Port ' + T + '/mouse.zig (168 loc, 3 tests) into crates/rustty-vt/src/mouse.rs. STRIP the GTK/gobject bits (the gobject import + any GTK-type conversion) — those belong to the apprt layer, not vt (R3: no GTK in vt). Port the mouse protocol/format/button types + the 3 tests faithfully; seam any GTK conversion with a documented PORT NOTE.' },
  { label: 'utf8_decoder', task: 'Port ' + T + '/UTF8Decoder.zig (142 loc, 3 tests) into crates/rustty-vt/src/utf8_decoder.rs (wire as mod utf8_decoder). Faithful DFA decoder + 3 tests. If a UTF8 decoder already exists in the stream path, reconcile to one faithful copy.' },
  { label: 'tabstops', task: 'Port ' + T + '/Tabstops.zig (271 loc, 5 tests) into crates/rustty-vt/src/tabstops.rs. Faithful bitset tabstops + 5 tests.' },
  { label: 'dcs', task: 'Finish ' + T + '/dcs.zig (430 loc, 8 tests) in crates/rustty-vt/src/dcs.rs (currently stub). It re-exports through lib/main and is near-leaf. Faithful + 8 tests.' },
]
const results = []
for (let i = 0; i < MODS.length; i++) {
  const m = MODS[i]
  const r = await robust(RULES +
    'TASK (' + m.label + '): ' + m.task + ' FULLY PORT (faithful; every comment verbatim; tests assertion-for-assertion, independent expected values). Wire into lib.rs. GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt ' + m.label + ' (its tests pass); cargo clippy -p rustty-vt --all-targets -- -D warnings; cargo fmt -p rustty-vt --check; scoped comment coverage `python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/' + (m.label === 'options' ? 'build_options.zig' : (m.label === 'utf8_decoder' ? 'UTF8Decoder.zig' : (m.label === 'tabstops' ? 'Tabstops.zig' : m.label + '.zig'))) + ' --port-glob "' + R + '/crates/rustty-vt/src/' + m.label + '.rs"` => missing=0 (for this module). Commit "rustty: vt port ' + m.label + ' (vt-4 leaf)" + trailer. Return ok+tests_added+builds+commit+blockers. If a module genuinely needs an un-ported sibling, surface it (these should all be leaves).',
    { schema: STEP, effort: 'high', label: 'm:' + m.label, phase: 'Port' })
  results.push({ mod: m.label, r })
  if (!r || (r.ok === false && r.blockers && r.blockers.length)) break
}

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (the vt-4 module tests pass: selection_codepoints/cursor/focus/build_options/mouse/utf8_decoder/tabstops/dcs — ~21 new + the existing 210 PageList still green); cargo +nightly miri test -p rustty-vt on the new leaf tests (0 UB/leak — they are tiny, tractable); `bash ' + GATES + '/sanitizers.sh rustty-vt "" address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. For each ported module, G3b comment coverage (its ref file) missing=0. Spot-check 2-3 ported tests are faithful (not tautological) vs the reference. Verdict GREEN only if all build/test/Miri/ASan/clippy/fmt green AND no module left stub AND g3b_missing=0. Report modules_done + tests_passing + the gate results.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { results, verify }
