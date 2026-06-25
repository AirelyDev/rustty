export const meta = {
  name: 'vt16-tmux',
  description: 'rustty-vt vt-16 (LAST module): terminal/tmux/* control-mode (output 47, layout 40, control 18, viewer 9 = 114 tests). Deps ready: Terminal/Screen/cursor/size (done); oniguruma gated like StringMap; siblings. Gated by options.tmux_control_mode. Dep order: output+layout (parsers) -> control -> viewer (the big integrator, 2283 loc). Completes all of Ghostty terminal/.',
  phases: [
    { title: 'Parsers', detail: 'tmux/output.rs (47) + tmux/layout.rs (40) — the control-mode parsers' },
    { title: 'Control', detail: 'tmux/control.rs (18) — uses output/layout' },
    { title: 'Viewer', detail: 'tmux/viewer.rs (9, 2283 loc) — the tmux multiplexer/viewer' },
    { title: 'Verify', detail: 'count-gate(tmux) + tests + Miri-subset + ASan + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const TM = G + '/src/terminal/tmux'
const GATES = R + '/.port/tools/gates'
const G3B = (zig, rs) => 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + TM + '/' + zig + ' --port-glob "' + R + '/crates/rustty-vt/src/tmux/' + rs + '"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT — check_test_counts.py "tmux" must end rust>=zig); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. Reference READ-ONLY ' + TM + '/. The VT cluster + search are DONE; 1842 tests pass. Rust tmux slice = crates/rustty-vt/src/tmux/{output,layout,control,viewer}.rs (stubs). Plan: ' + R + '/.port/phase-2/VT_SUBBATCH_PLAN.md. '
  + 'KEY/SEAMS: Terminal/Screen/cursor/size (done); quirks->rustty-util; oniguruma -> gate the regex-using parts behind #[cfg(feature="oniguruma")] like StringMap (OFF by default; their tests SkipZigTest when off). tmux is gated by options.tmux_control_mode — mirror with a tmux cargo feature OR the options gate (whichever the reference uses). [:0]const u8->Vec<u8>. SOUNDNESS RULE (memory): raw-ptr discipline. '
  + 'MIRI: per MIRI_EXCEPTIONS.md Miri-subset in ISOLATION (viewer drives Terminals — may be heavy/thread-y -> ASan-cover the heavy ones + note). PROCESS LESSONS (memory): port ALL tests (count gate enforces), scoped-G3b. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_gate_ok: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Parsers')
const parsers = await robust(RULES +
  'TASK (tmux parsers). Port FULLY (every test): ' + TM + '/output.zig (47t, tmux control-mode output parser) -> tmux/output.rs; ' + TM + '/layout.zig (40t, tmux layout parser) -> tmux/layout.rs. GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt tmux (the 87 pass + no regression vs 1842); Miri-subset 2 tests ISOLATED (0 UB/leak); clippy --all-targets -D; fmt; ' + G3B('output.zig', 'output.rs') + ' + ' + G3B('layout.zig', 'layout.rs') + ' missing=0. Commit "rustty: vt tmux output + layout parsers + 87 tests (vt-16)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'parsers', phase: 'Parsers' })

phase('Control')
const control = await robust(RULES + 'Parsers landed: ' + JSON.stringify(parsers && parsers.commit) + '\n'
  + 'TASK (tmux control). Port ' + TM + '/control.zig (18t, the tmux -CC control-mode protocol; uses output/layout) FULLY into tmux/control.rs. GATES FOREGROUND: build; cargo test -p rustty-vt tmux (18 new pass + no regression); Miri-subset 2 tests ISOLATED; clippy -D; fmt; ' + G3B('control.zig', 'control.rs') + ' missing=0. Commit "rustty: vt tmux control + 18 tests (vt-16)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'control', phase: 'Control' })

phase('Viewer')
const viewer = await robust(RULES + 'Prior: ' + JSON.stringify([parsers, control].map(s => s && s.commit)) + '\n'
  + 'TASK (tmux viewer — the big integrator). Port ' + TM + '/viewer.zig (9t, 2283 loc, the tmux multiplexer/viewer driving per-pane Terminals) FULLY into tmux/viewer.rs. GATES FOREGROUND: build; cargo test -p rustty-vt tmux (9 new pass + no regression); Miri-subset (light tests; heavy/Terminal-driving ones ASan-cover + note); clippy -D; fmt; ' + G3B('viewer.zig', 'viewer.rs') + ' missing=0. Commit "rustty: vt tmux viewer + 9 tests (vt-16)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'viewer', phase: 'Viewer' })

phase('Verify')
const verify = await robust(RULES + 'Phases: ' + JSON.stringify([parsers, control, viewer].map(s => s && s.commit)) + '\n'
  + 'TASK (independent verify — default RED; this COMPLETES rustty-vt terminal/). Confirm FOREGROUND: cargo build --workspace; add a "tmux" mapping to ' + GATES + '/check_test_counts.py (output+layout+control+viewer.zig vs tmux/*.rs, oniguruma-gated count as deferred if applicable) then `python3 ' + GATES + '/check_test_counts.py` => ALL modules GREEN incl tmux (count_gate_ok); cargo test -p rustty-vt (tmux ~114 pass + no regression vs 1842); cargo +nightly miri test -p rustty-vt on ~4 tmux parser tests ISOLATED (0 UB/leak; viewer heavy ones ASan-only — note); `bash ' + GATES + '/sanitizers.sh rustty-vt tmux address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. G3b on each tmux .zig missing=0 (modulo oniguruma-gated). Spot-check 3 tests (an output parse, a layout parse, a control command) faithful, not tautological. Verdict GREEN only if count_gate_ok (tmux) + ALL-modules-green + build + tests + Miri/ASan + g3b=0 + clippy + fmt green AND no regression. On GREEN, also note: this completes all of Ghostty terminal/ in rustty-vt (remaining deferrals: apc/font, key_mods/config, ScrollViewport C-ABI->rustty-vt-c, stream SIMD seam, oniguruma feature). Return verdict + tests_passing + count_gate_ok + miri_subset_clean + asan_clean + g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { parsers, control, viewer, verify }
