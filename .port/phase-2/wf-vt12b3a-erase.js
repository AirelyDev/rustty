export const meta = {
  name: 'vt12b3a-erase',
  description: 'rustty-vt vt-12b-3a: fill Terminal erase methods (erase_chars, erase_line, erase_display[132], decaln, delete_chars) + their tests. Raw *mut Cell/Row + tracked-pin RAII -> Miri + ASan + tracking-allocator gates. Leaves scroll/index/insert*/delete_lines/margins for 12b-3b.',
  phases: [
    { title: 'Fill', detail: 'Terminal erase methods + their tests' },
    { title: 'Verify', detail: 'scoped G3b + tests + Miri-subset + ASan + tracking-alloc + faithfulness + no over-reach' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const SCOPED_G3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/Terminal.zig --port-glob "' + R + '/crates/rustty-vt/src/terminal.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/Terminal.zig. vt-12b-1/2 (print + cursor/charset/tabs) landed; 1034 tests pass. Plan: ' + R + '/.port/phase-2/TERMINAL_PLAN.md. '
  + 'KEY: erase methods deref active Screen via active_mut() + use Screen clear_cells/clear_unprotected_cells/clear_rows + page row/cell access; raw *mut Cell/*mut Row/*mut Page via core::ptr::addr_of_mut! (SB-safe idiom); tracked-pin invalidation across operations -> faithful track_pin/untrack_pin (Drop-guard where Zig uses defer). [:0]const u8->Vec<u8>. decaln fills the screen with E. '
  + 'MIRI: per MIRI_EXCEPTIONS.md (restored) run Miri on a SMALL subset of new erase tests in ISOLATION (raw-ptr coverage). PROCESS LESSON (memory): gate on SCOPED G3b of JUST the methods filled this batch (diff vs prior baseline). '
  + 'IMPORTANT: fill ONLY erase_chars/erase_line/erase_display/decaln/delete_chars (+ tiny helpers they need); leave index/reverse_index/scroll_up/scroll_down/scroll_viewport/insert_lines/delete_lines/insert_blanks/margins/SGR/modes/semantic/Handler todo!(). Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, methods_filled: { type: 'array', items: { type: 'string' } }, scoped_g3b_clean: { type: 'boolean' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, tracking_clean: { type: 'boolean' }, scoped_g3b_clean: { type: 'boolean' }, no_overreach: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill Terminal erase). In crates/rustty-vt/src/terminal.rs, replace the todo!() bodies of ONLY: erase_chars, erase_line, erase_display (ref ~132 loc), decaln, delete_chars — faithful ports from ' + T + '/Terminal.zig (raw *mut Cell/Row via addr_of_mut!; tracked-pin RAII where Zig uses defer/errdefer). Then PORT the erase tests (eraseChars/eraseLine/eraseDisplay/decaln/deleteChars groups). DEFER tests needing still-todo!() methods (scroll/insert*/delete_lines/margins) to 12b-3b. Leave all other methods todo!(). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (your erase tests pass + 1034 prior no regression); SCOPED G3b `' + SCOPED_G3B + '` — 0 missing attributable to the filled erase methods (diff vs baseline); cargo +nightly miri test -p rustty-vt on 2-3 erase tests (ISOLATED, 0 UB/leak — erase touches *mut Cell/Row); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` (ASan clean); the page-pool tracking allocator if an erase test drives multi-page erase (all pages returned); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: Terminal erase methods + tests (vt-12b-3a)". Return ok+tests_passing+methods_filled+scoped_g3b_clean+builds+commit+notes+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, methods_filled: fill.methods_filled, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (erase tests pass + no regression vs 1034); cargo +nightly miri test -p rustty-vt on ~3 erase tests (ISOLATED, 0 UB/leak — scrutinize the *mut Cell/Row + tracked-pin handling); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. SCOPED G3b: filled erase methods drop 0 comments (scoped_g3b_clean). NO OVER-REACH: only erase filled; scroll/index/insert*/delete_lines/margins/SGR/modes/semantic/Handler still todo!() (no_overreach). Spot-check 3 erase tests (an eraseDisplay protected, an eraseLine, a deleteChars wide-char) faithful vs Terminal.zig, not tautological. Verdict GREEN only if build + erase tests + Miri-subset + ASan + scoped_g3b_clean + clippy + fmt green AND no_overreach AND no regression. Report verdict + tests_passing + miri_subset_clean + asan_clean + tracking_clean + scoped_g3b_clean + no_overreach + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
