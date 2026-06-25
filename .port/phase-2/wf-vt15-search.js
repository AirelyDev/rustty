export const meta = {
  name: 'vt15-search',
  description: 'rustty-vt vt-15: terminal/search/* (sliding_window 25, screen 16, pagelist 7, viewport 4, active 2, Thread 1 = ~55 tests). Deps ready: Terminal/highlight/formatter (done), datastruct->rustty-datastruct, os->rustty-os, global->rustty-global. Dep order: sliding_window -> {screen,pagelist,viewport,active} -> Thread. Gated (optional in main.zig).',
  phases: [
    { title: 'Core', detail: 'search/sliding_window.rs (25t) — the search core' },
    { title: 'Consumers', detail: 'search/{screen(16),pagelist(7),viewport(4),active(2)}.rs' },
    { title: 'Thread', detail: 'search/thread.rs (1t) — search thread (global/os)' },
    { title: 'Verify', detail: 'count-gate(search) + tests + Miri-subset + ASan + G3b + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const SE = G + '/src/terminal/search'
const GATES = R + '/.port/tools/gates'
const G3B = (zig, rs) => 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + SE + '/' + zig + ' --port-glob "' + R + '/crates/rustty-vt/src/search/' + rs + '"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = PORT EVERY test{} (count is a CONTRACT — check_test_counts.py "search" enforces rust>=54); EVERY line/comment verbatim; assertion-for-assertion, INDEPENDENT expected values. Reference READ-ONLY ' + SE + '/. The VT cluster is DONE; 1787 tests pass. Rust search slice = crates/rustty-vt/src/search/{sliding_window,screen,pagelist,viewport,active,thread}.rs (stubs). Plan: ' + R + '/.port/phase-2/VT_SUBBATCH_PLAN.md. '
  + 'KEY/SEAMS: Terminal/highlight/formatter (done); datastruct/main->rustty-datastruct (circ_buf etc.); os/main->rustty-os; global->rustty-global; quirks/tripwire->rustty-util. search may be feature-gated in the reference (optional) — mirror with a cargo feature if the reference gates it, else always-compile. SOUNDNESS RULE (memory): no &mut Page/Row/Cell args re-read across assert_integrity -> raw ptrs. [:0]const u8->Vec<u8>. oniguruma-style regex: if search uses a regex engine, gate behind #[cfg(feature="oniguruma")] like StringMap (off by default). '
  + 'MIRI: per MIRI_EXCEPTIONS.md Miri-subset in ISOLATION. PROCESS LESSONS (memory): port ALL tests (count gate enforces), scoped-G3b. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, count_gate_ok: { type: 'boolean' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Core')
const core = await robust(RULES +
  'TASK (search core). Port ' + SE + '/sliding_window.zig (25 tests, the search ring-buffer/window core; needs datastruct + formatter) FULLY into crates/rustty-vt/src/search/sliding_window.rs. GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt sliding_window (25 pass + no regression vs 1787); Miri-subset 2 tests ISOLATED (0 UB/leak); clippy --all-targets -D; fmt; ' + G3B('sliding_window.zig', 'sliding_window.rs') + ' missing=0. Commit "rustty: vt search sliding_window + 25 tests (vt-15)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'core', phase: 'Core' })

phase('Consumers')
const cons = await robust(RULES + 'Core landed: ' + JSON.stringify(core && core.commit) + '\n'
  + 'TASK (search consumers). Port FULLY (every test): ' + SE + '/screen.zig (16t) -> search/screen.rs; ' + SE + '/pagelist.zig (7t) -> search/pagelist.rs; ' + SE + '/viewport.zig (4t) -> search/viewport.rs; ' + SE + '/active.zig (2t) -> search/active.rs. They build on sliding_window + Terminal/highlight. GATES FOREGROUND: build; cargo test -p rustty-vt search (the 29 pass + no regression); Miri-subset 2 tests ISOLATED; clippy -D; fmt; G3b each missing=0. Commit "rustty: vt search screen/pagelist/viewport/active + 29 tests (vt-15)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'consumers', phase: 'Consumers' })

phase('Thread')
const thread = await robust(RULES + 'Prior: ' + JSON.stringify([core, cons].map(s => s && s.commit)) + '\n'
  + 'TASK (search thread). Port ' + SE + '/Thread.zig (1t, the background search thread; needs global->rustty-global + os->rustty-os) FULLY into crates/rustty-vt/src/search/thread.rs. If it uses real OS threading, port faithfully via std::thread / the rustty-os threading seam. GATES FOREGROUND: build; cargo test -p rustty-vt search (no regression); Miri-subset (if tractable — threads may not be Miri-friendly; if so ASan-cover + note); clippy -D; fmt; ' + G3B('Thread.zig', 'thread.rs') + ' missing=0. Commit "rustty: vt search Thread (vt-15)". Return ok+tests_added+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'thread', phase: 'Thread' })

phase('Verify')
const verify = await robust(RULES + 'Phases: ' + JSON.stringify([core, cons, thread].map(s => s && s.commit)) + '\n'
  + 'TASK (independent verify — default RED). Confirm FOREGROUND: cargo build --workspace; `python3 ' + GATES + '/check_test_counts.py search` => search rust>=54 (count_gate_ok); cargo test -p rustty-vt (search ~55 pass + no regression vs 1787); cargo +nightly miri test -p rustty-vt on ~4 search tests ISOLATED (0 UB/leak; thread tests may be ASan-only — note); `bash ' + GATES + '/sanitizers.sh rustty-vt search address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. G3b on each search .zig missing=0. Spot-check 3 tests (a sliding_window match, a search/screen query, a viewport) faithful, not tautological. Verdict GREEN only if count_gate_ok (search 54+) + build + tests + Miri-subset/ASan + g3b=0 + clippy + fmt green AND no regression. Report verdict + tests_passing + count_gate_ok + miri_subset_clean + asan_clean + g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { core, cons, thread, verify }
