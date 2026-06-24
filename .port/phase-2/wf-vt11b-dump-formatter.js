export const meta = {
  name: 'vt11b-dump-formatter',
  description: 'rustty-vt vt-11b: fill Screen dump_string/dump_string_alloc/dump_string_alloc_unwrapped + the formatter PRODUCTION code (ScreenFormatter/PageFormatter/PageListFormatter — NOT TerminalFormatter, which needs Terminal -> vt-12). High-leverage: unblocks content verification for all subsequent Screen tests. Port the read/write tests (deferred from vt-11a, need dumpStringAlloc) + the cursorCopy x/y tests.',
  phases: [
    { title: 'Fill', detail: 'formatter production (3 non-Terminal formatters) + Screen dump_string* + read/write tests' },
    { title: 'Verify', detail: 'ported tests + ASan + clippy/fmt + faithfulness + no over-reach' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/{formatter.zig,Screen.zig}. Screen-SCC skeleton (vt-10) + Screen foundation (vt-11a) landed; 833 tests pass; screen.rs has 22 todo!() left. '
  + 'Plan: ' + R + '/.port/phase-2/SCREEN_SCC_PLAN.md. RULES: Zig [:0]const u8/[]const u8 -> Vec<u8> (never lossy String); writer `*std.Io.Writer` -> `&mut dyn std::io::Write` (Discarding -> a counting sink; Allocating -> Vec<u8>); style emission delegates to style.rs (formatter_vt/html, already ported); quirks.inlineAssert -> debug_assert!. '
  + 'MIRI: per .port/phase-2/MIRI_EXCEPTIONS.md the rustty-vt Miri compile is intractable (66+ min) — use ASan (sanitizers.sh) + the addr_of_mut! raw-pointer discipline as the per-sub-batch memory-safety gate; full Miri is a finale deep-check (do NOT run per-sub-batch Miri). '
  + 'IMPORTANT: fill ONLY formatter production (ScreenFormatter/PageFormatter/PageListFormatter + helpers) + Screen dump_string*; TerminalFormatter stays a stub (needs Terminal, vt-12) and its tests + all ~100 formatter test{} blocks DEFER to vt-12 (record in DEFERRALS.md). Leave Screen clearRows/clone/resize/select*/semantic as todo!(). Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, asan_clean: { type: 'boolean' }, no_overreach: { type: 'boolean' }, formatter_tests_deferred: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill formatter production + Screen dump). (1) In crates/rustty-vt/src/formatter.rs, fill the production formatters from ' + T + '/formatter.zig: PageFormatter (incl the ~485-line formatWithState core loop at ref 877 + TrailingState), PageListFormatter (ref 696), ScreenFormatter (ref 424: init/format, Content/Extra handling, pin_map two-pass) + all their cell/writer/style helpers (formatStyled, Extra::isSet, etc.). TerminalFormatter (ref 141) stays a compiling stub (needs Terminal -> vt-12). DO NOT port the 100 formatter test{} blocks (they ALL parse via Terminal::vtStream -> DEFER to vt-12; record in DEFERRALS.md). '
  + '(2) In crates/rustty-vt/src/screen.rs, fill dump_string + dump_string_alloc + dump_string_alloc_unwrapped (they use ScreenFormatter). '
  + '(3) Port the Screen tests that now pass with dump available: the read/write tests (deferred from vt-11a) + the cursorCopy x/y tests (needed dumpStringAlloc). Leave clearRows/clone/resize/select*/semantic as todo!(). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (your new tests pass + 833 prior no regression); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` + `... formatter address` (ASan clean); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: formatter production + Screen dump_string + read/write tests (vt-11b)". Return ok+tests_passing+builds+commit+notes(what filled, what deferred)+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (new tests pass + no regression vs 833); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` + `... formatter address` (ASan clean — the formatter walks pages via pointers); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. NO OVER-REACH: only formatter production + Screen dump* filled; TerminalFormatter still stub; Screen clearRows/clone/resize/select*/semantic still todo!() (no_overreach). DEFERRAL DISCIPLINE: confirm the ~100 formatter test{} blocks are DEFERRED to vt-12 (recorded in DEFERRALS.md), not silently dropped (formatter_tests_deferred). Spot-check 3 ported read/write tests faithful (the dumped content matches Screen.zig assertions), not tautological. (Per MIRI_EXCEPTIONS.md, do NOT run per-sub-batch Miri; ASan + addr_of_mut! discipline is the gate.) Verdict GREEN only if build + ported tests + ASan + clippy + fmt green AND no_overreach AND formatter_tests_deferred AND no regression. Report verdict + tests_passing + asan_clean + no_overreach + formatter_tests_deferred + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
