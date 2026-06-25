export const meta = {
  name: 'vt12-tests-recovery',
  description: 'Recover the ~178 Terminal.zig reference test{} blocks dropped across vt-12b (terminal.rs has 200 of 376). All target methods are already filled, so all are portable. Port in method-family chunks (each agent diffs Terminal.zig vs terminal.rs to find its missing tests, ports them assertion-for-assertion), then a re-finale that HARD-ASSERTS Rust #[test] count == Zig test{} count = 376.',
  phases: [
    { title: 'Recover', detail: '6 family chunks: print / scroll / lines / cursor / erase / input+sweep' },
    { title: 'Refinale', detail: 'count==376 + Terminal G3b->0 + suite + ASan + Miri-pass -> complete-Terminal' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const TG3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/Terminal.zig --port-glob "' + R + '/crates/rustty-vt/src/terminal.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity: the test count is a CONTRACT (376 Terminal test{} is non-negotiable). EVERY ref test{} as #[test], assertion-for-assertion, INDEPENDENT expected values, comments verbatim. Reference READ-ONLY ' + T + '/Terminal.zig. All Terminal methods are ALREADY FILLED (vt-12b); 1417 tests pass. The dropped tests are listed in ' + R + '/.port/phase-2/vt12-terminal-missing-tests.txt (normalized names). '
  + 'KEY: to find YOUR chunk\'s missing tests, grep Terminal.zig for `^test "Terminal: <family> ...` and check each against crates/rustty-vt/src/terminal.rs (normalized fn-name match) — port every one not already present. Many are wide-char/spacer_head/spacer_tail/grapheme/VS15-16/Fitzpatrick/ZWJ + scroll-region + fuzzer-regression variants. SOUNDNESS RULE (memory): no &mut Page/Row/Cell args re-read across assert_integrity -> raw ptrs. [:0]const u8->Vec<u8>. '
  + 'MIRI: per MIRI_EXCEPTIONS.md run Miri-subset on a few new raw-ptr tests in ISOLATION. You ONLY ADD tests (no method-body changes) — so build/clippy stay green trivially; the risk is faithfulness + completeness. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_added: { type: 'integer' }, terminal_test_count: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const FINALE = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['PARITY', 'NOT_PARITY'] }, terminal_test_count: { type: 'integer' }, suite_tests: { type: 'integer' }, terminal_g3b_missing: { type: 'integer' }, residual_all_apc_deferred: { type: 'boolean' }, asan_clean: { type: 'boolean' }, miri_pass_clean: { type: 'boolean' }, tautological_or_weakened: { type: 'array', items: { type: 'string' } }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Recover')
const CHUNKS = [
  { label: 'print', fams: 'print, printRepeat, VS15, VS16, Fitzpatrick, overwrite, soft-wrap, default style, disabled-wraparound, grapheme/ZWJ/emoji print' },
  { label: 'scroll', fams: 'scrollUp, scrollDown, index, reverseIndex (incl bottom/top scroll-region, left-right margin, wide-char, scroll-region variants)' },
  { label: 'lines', fams: 'deleteLines, insertLines, insertBlanks, insert (incl left-right scroll-region, wide-char at margin, spacer_head/tail, bg-color, pending-wrap, page-boundary-dirty, legacy)' },
  { label: 'cursor', fams: 'cursorLeft (reverse-wrap/extended/margins), cursorUp, cursorDown (scroll-margin), saveCursor/restoreCursor, tabClear, keypad' },
  { label: 'erase', fams: 'deleteChars (wide-char/spacer/right-margin/resets-wrap), eraseChars, eraseDisplay, eraseLine, decaln (preserves-color/reset-margins/protected-graphemes)' },
  { label: 'input-sweep', fams: 'input (mouse/key encode), + a FINAL SWEEP: diff ALL Terminal.zig test{} vs terminal.rs and port EVERY remaining unported one regardless of family' },
]
const results = []
for (let i = 0; i < CHUNKS.length; i++) {
  const c = CHUNKS[i]
  const r = await robust(RULES +
    'TASK (recover Terminal test family "' + c.label + '": ' + c.fams + '). grep ' + T + '/Terminal.zig for the test{} blocks in this family, diff against crates/rustty-vt/src/terminal.rs, and PORT EVERY ONE NOT ALREADY PRESENT as a #[test], assertion-for-assertion (verbatim comments, independent expected values). '
    + (c.label === 'input-sweep' ? 'CRITICAL: after your family, do a FULL diff of all 376 Terminal.zig test names vs terminal.rs and port ANY still-missing test (catch mis-bucketed ones) so the count reaches 376. ' : '')
    + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (your new tests pass + no regression); cargo +nightly miri test -p rustty-vt on 1-2 new raw-ptr tests (ISOLATED, 0 UB/leak); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check; report the current `grep -cE "#\\[test\\]" terminal.rs` count. Commit "rustty: recover Terminal ' + c.label + ' tests (vt-12 recovery)". Return ok+tests_added+terminal_test_count+builds+commit+blockers.',
    { schema: STEP, effort: 'high', label: 'rec:' + c.label, phase: 'Recover' })
  results.push({ chunk: c.label, r })
  if (!r || (r.ok === false && r.blockers && r.blockers.length)) break
}

phase('Refinale')
const finale = await robust(RULES +
  'TASK (Terminal re-finale — adversarial, default NOT_PARITY). Recovery chunks: ' + JSON.stringify(results.map(x => x.r && { c: x.chunk, n: x.r.terminal_test_count })) + '\n'
  + 'HARD CONTRACT CHECK (the backstop that caught the original drop): `grep -cE "^test \\"" ' + T + '/Terminal.zig` (=376) MUST EQUAL the count of Terminal reference #[test] fns in crates/rustty-vt/src/terminal.rs (allowing rustty-only helper tests like terminal_pagepool_no_leak_* + the doc smoke test — but every one of the 376 Zig test{} blocks MUST have a counterpart; do a normalized name-diff and list ANY still missing as a blocker -> NOT_PARITY). '
  + 'Then FOREGROUND: cargo build --workspace; cargo test -p rustty-vt (full suite, report count, no failures); `' + TG3B + '` => Terminal missing residual ALL maps to the apc font-block (glyph_protocol/glyph_glossary) — confirm residual_all_apc_deferred (the test-body missing should now be 0); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` + `... stream_terminal address` (asan_clean); broader Miri pass in ISOLATION on ~10 representative Terminal tests across families incl the recovered wide-char/scroll-region ones + the 2 prior-UB paths (miri_pass_clean, 0 UB); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. ADVERSARIAL: sample 5 recovered tests (a print wide-char/Fitzpatrick, a deleteLines left-right-margin, an index bottom-of-scroll-region, a reverseIndex, a decaln) — assertion-for-assertion vs Terminal.zig, none tautological/weakened. '
  + 'Verdict PARITY only if: Terminal #[test] count covers all 376 Zig test{} + workspace builds + full suite green + Terminal G3b residual all-apc-deferred + asan_clean + miri_pass_clean + clippy/fmt/acyclic green + no tautological/weakened. On PARITY commit "rustty: complete Terminal (vt-12) — 376 tests + stream_terminal + reactivated formatter/kitty/Screen". Return verdict + terminal_test_count + suite_tests + terminal_g3b_missing + residual_all_apc_deferred + asan_clean + miri_pass_clean + tautological_or_weakened + commit + blockers.',
  { schema: FINALE, effort: 'high', label: 'refinale', phase: 'Refinale' })
return { results, finale }
