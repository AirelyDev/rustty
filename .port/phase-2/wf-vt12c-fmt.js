export const meta = {
  name: 'vt12c-fmt',
  description: 'rustty-vt vt-12c-fmt: reactivate the formatter deferral now that Terminal exists. Fill TerminalFormatter (formatter.rs init/format + Terminal Extra: palette/modes/scrolling_region/tabstops/pwd/keyboard) + port the ~100 formatter test{} blocks (all build fixtures via Terminal::init + vt_stream) + the 5 TerminalFormatter Screen tests. +~105 tests. Drives formatter.zig G3b -> 0.',
  phases: [
    { title: 'Fill', detail: 'TerminalFormatter + 100 formatter tests + 5 Screen TerminalFormatter tests' },
    { title: 'Verify', detail: 'formatter G3b->0 + tests + Miri-subset + ASan + faithfulness' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const FMT_G3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/formatter.zig --port-glob "' + R + '/crates/rustty-vt/src/formatter.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/formatter.zig + Terminal.zig (TerminalFormatter Screen tests). vt-12b done — Terminal + stream_terminal fully ported; 1253 tests pass. The formatter PRODUCTION code (PageFormatter/PageListFormatter/ScreenFormatter) is already ported (vt-11b); ONLY TerminalFormatter (init/format = todo!() stub) + the 100 formatter tests + 5 TerminalFormatter Screen tests were deferred (DEFERRALS.md, Terminal-gated). Plan: ' + R + '/.port/phase-2/TERMINAL_PLAN.md. '
  + 'KEY: TerminalFormatter (formatter.rs:~285) formats a *const Terminal — its Extra emits palette/modes/scrolling_region/tabstops/pwd/kitty_keyboard via the format machinery; it composes the already-ported ScreenFormatter/PageListFormatter. The 100 tests build a Terminal (Terminal::init), feed input via t.vt_stream().next_slice(bytes) (now available), then format + assert the round-trip / output. Writer seam: &mut dyn io::Write. [:0]const u8->Vec<u8>. '
  + 'MIRI: per MIRI_EXCEPTIONS.md (restored) Miri-subset on a few formatter tests in ISOLATION. PROCESS LESSON: SCOPED G3b — after this, formatter.zig G3b should be 0 missing (the TerminalFormatter + its tests were the residual). '
  + 'IMPORTANT: fill TerminalFormatter; do NOT touch the already-ported production formatters except to wire TerminalFormatter into them. Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, formatter_tests_ported: { type: 'integer' }, fmt_g3b_missing: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, fmt_g3b_missing: { type: 'integer' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill TerminalFormatter + port the formatter tests). (1) In crates/rustty-vt/src/formatter.rs, fill TerminalFormatter::init + format (the todo!() stub) from ' + T + '/formatter.zig — it formats a Terminal (palette/modes/scrolling_region/tabstops/pwd/kitty_keyboard Extra) composing the already-ported ScreenFormatter/PageListFormatter/PageFormatter. (2) PORT all ~100 formatter test{} blocks from formatter.zig (they Terminal::init, feed input via t.vt_stream().next_slice, then format + assert) as #[test], assertion-for-assertion. (3) PORT the 5 TerminalFormatter-driven Screen tests from Screen.zig. '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (your ~105 new tests pass + 1253 prior no regression); SCOPED G3b `' + FMT_G3B + '` => missing=0 (TerminalFormatter + tests were the residual); cargo +nightly miri test -p rustty-vt on 3 formatter tests (ISOLATED, 0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt formatter address` (ASan clean); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: TerminalFormatter + 100 formatter tests + 5 Screen tests (vt-12c-fmt)". Update DEFERRALS.md (remove the formatter-tests + TerminalFormatter deferral entries — now resolved). Return ok+tests_passing+formatter_tests_ported+fmt_g3b_missing+builds+commit+notes+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, formatter_tests_ported: fill.formatter_tests_ported, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (the ~105 new formatter tests pass + no regression vs 1253); cargo +nightly miri test -p rustty-vt on ~3 formatter tests (ISOLATED, 0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt formatter address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. SCOPED G3b `' + FMT_G3B + '` => missing=0 (formatter.zig FULLY ported now — gateable). Confirm DEFERRALS.md no longer lists the formatter-tests/TerminalFormatter deferral. Spot-check 3 formatter tests (a plain dump, a VT round-trip, an HTML format) faithful vs formatter.zig, not tautological. Verdict GREEN only if build + tests + Miri-subset + ASan + fmt_g3b_missing=0 + clippy + fmt green AND no regression. Report verdict + tests_passing + miri_subset_clean + asan_clean + fmt_g3b_missing + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
