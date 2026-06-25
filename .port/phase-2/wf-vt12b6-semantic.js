export const meta = {
  name: 'vt12b6-semantic',
  description: 'rustty-vt vt-12b-6: fill the LAST Terminal-direct methods (semantic_prompt[121], semantic_prompt_fresh_line, cursor_is_at_prompt, set_pwd, get_pwd, set_title, get_title) + their ~19 tests. pwd/title use Vec<u8>+NUL sentinel (byte-slice rule). After this, only the stream_terminal Handler dispatch (12b-7) remains in 12b.',
  phases: [
    { title: 'Fill', detail: 'Terminal semantic/pwd/title methods + their tests' },
    { title: 'Verify', detail: 'scoped G3b + tests + Miri-subset + ASan + faithfulness + no over-reach' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const SCOPED_G3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/Terminal.zig --port-glob "' + R + '/crates/rustty-vt/src/terminal.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = 100% fidelity (every line/comment verbatim; tests assertion-for-assertion, independent expected values). Reference READ-ONLY ' + T + '/Terminal.zig. vt-12b-1..5 landed; 1172 tests pass. Plan: ' + R + '/.port/phase-2/TERMINAL_PLAN.md. '
  + 'KEY: semantic_prompt[~121] handles OSC133 prompt marking (uses osc::semantic_prompt — ported + the Screen semantic_prompt field); cursor_is_at_prompt scans for a prompt row; set_pwd/set_title clear then append bytes + a trailing 0 sentinel (Vec<u8>); get_pwd/get_title return None if empty else Some(&items[..len-1]) (NUL-stripped &[u8]) — NEVER lossy String. SOUNDNESS RULE (memory): no &mut Page/Row/[Cell] args across assert_integrity -> raw ptrs. '
  + 'MIRI: per MIRI_EXCEPTIONS.md (restored) Miri-subset on new tests in ISOLATION. PROCESS LESSON: SCOPED G3b of JUST the methods filled this batch (a single dropped comment fails). '
  + 'IMPORTANT: fill ONLY semantic_prompt/semantic_prompt_fresh_line/cursor_is_at_prompt/set_pwd/get_pwd/set_title/get_title; leave the stream_terminal Handler dispatch + kitty/glyph handlers (12b-7/12c) todo!(). Keep lib.rs PORT STATUS: stub. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, tests_passing: { type: 'integer' }, methods_filled: { type: 'array', items: { type: 'string' } }, scoped_g3b_clean: { type: 'boolean' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, tests_passing: { type: 'integer' }, miri_subset_clean: { type: 'boolean' }, asan_clean: { type: 'boolean' }, scoped_g3b_clean: { type: 'boolean' }, no_overreach: { type: 'boolean' }, tautological_tests: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fill')
const fill = await robust(RULES +
  'TASK (fill Terminal semantic/pwd/title). In crates/rustty-vt/src/terminal.rs, replace the todo!() bodies of ONLY: semantic_prompt, semantic_prompt_fresh_line, cursor_is_at_prompt, set_pwd, get_pwd, set_title, get_title — faithful ports from ' + T + '/Terminal.zig (pwd/title = Vec<u8> + trailing 0 sentinel; getters return Option<&[u8]> NUL-stripped). Then PORT the ~19 semantic/pwd/title tests. DEFER tests needing the still-todo!() Handler dispatch to 12b-7. Leave all other methods todo!(). '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (your tests pass + 1172 prior no regression); SCOPED G3b `' + SCOPED_G3B + '` — 0 missing attributable to the filled methods (diff vs baseline); cargo +nightly miri test -p rustty-vt on 2 new tests (ISOLATED, 0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` (ASan clean); clippy -p rustty-vt --all-targets -- -D warnings; fmt --check. Commit "rustty: Terminal semantic/pwd/title + tests (vt-12b-6)". Return ok+tests_passing+methods_filled+scoped_g3b_clean+builds+commit+notes+blockers.',
  { schema: STEP, effort: 'high', label: 'fill', phase: 'Fill' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED). Fill: ' + JSON.stringify(fill && { commit: fill.commit, methods_filled: fill.methods_filled, blockers: fill.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (no regression vs 1172); cargo +nightly miri test -p rustty-vt on ~2 new tests (ISOLATED, 0 UB/leak); `bash ' + GATES + '/sanitizers.sh rustty-vt terminal address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. SCOPED G3b: filled methods drop 0 comments (scoped_g3b_clean). Confirm pwd/title use Vec<u8>+NUL (NOT lossy String) + getters return Option<&[u8]> faithfully. NO OVER-REACH: only semantic/pwd/title filled; Handler dispatch + kitty/glyph still todo!() (no_overreach). Spot-check 3 tests (a semantic_prompt OSC133, a setPwd round-trip, a cursor_is_at_prompt) faithful vs Terminal.zig, not tautological. Verdict GREEN only if build + tests + Miri-subset + ASan + scoped_g3b_clean + clippy + fmt green AND no_overreach AND no regression. Report verdict + tests_passing + miri_subset_clean + asan_clean + scoped_g3b_clean + no_overreach + tautological_tests + blockers.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fill, verify }
