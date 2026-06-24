export const meta = {
  name: 'vt11g-restore-finale',
  description: 'Fix the vt-11 finale NOT_PARITY: restore the ~29 verbatim reference doc/inline comments dropped in fully-ported Screen production methods (comment-drift accumulated across vt-11a..e because whole-Screen G3b was deferred to the finale), then re-run the Screen-SCC finale -> complete Screen-SCC.',
  phases: [
    { title: 'Restore', detail: 'restore dropped Screen production comments verbatim + verify test-region missing all deferred + scoped G3b' },
    { title: 'Finale', detail: 're-run whole-cluster G3b + suite + ASan + faithfulness -> complete Screen-SCC' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const T = G + '/src/terminal'
const GATES = R + '/.port/tools/gates'
const SCREEN_G3B = 'python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files ' + T + '/Screen.zig --port-glob "' + R + '/crates/rustty-vt/src/screen.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = EVERY reference comment VERBATIM (retarget ghostty->rustty only). Reference READ-ONLY ' + T + '/Screen.zig. Screen is method-complete (992 tests pass); the issue is DROPPED COMMENTS in ported production methods, NOT logic. Deferrals tracked in ' + R + '/.port/phase-2/DEFERRALS.md. '
  + 'MIRI: per MIRI_EXCEPTIONS.md do NOT run per-sub-batch Miri (compile intractable); ASan + addr_of_mut! discipline is the gate. Run gates FOREGROUND. Commit trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, comments_restored: { type: 'integer' }, screen_g3b_missing: { type: 'integer' }, residual_all_deferred: { type: 'boolean' }, builds: { type: 'boolean' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const FINALE = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['PARITY', 'NOT_PARITY'] }, suite_tests: { type: 'integer' }, asan_clean: { type: 'boolean' }, residual_all_deferred: { type: 'boolean' }, tautological_or_weakened: { type: 'array', items: { type: 'string' } }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Restore')
const restore = await robust(RULES +
  'TASK (restore dropped Screen production comments — R1 fidelity). In crates/rustty-vt/src/screen.rs, restore the VERBATIM reference comments that were dropped/summarized on these FULLY-PORTED production methods (the finale identified these; restore each exactly from Screen.zig): '
  + '(1) split_cell_boundary — the full 32-line doc block (Screen.zig:1492-1523: boundary-condition enumeration, the "DOES NOT MODIFY ROW WRAP STATE! See cursorResetWrap" note, the spacer-tail/spacer-head cases). '
  + '(2) blank_cell — doc (ref 1620-1621). (3) set_attribute — doc (ref 1836-1839). (4) manual_style_update — the full lazy-print "Note that we COULD lazily do this in print..." comment (ref 2038-2041) + the `// std.log.warn("active styles=...")` debug comment (ref 2016). (5) select_all — doc (ref 2694-2695). (6) select_output — doc (ref 2873-2879). (7) dump_string — doc (ref 3143-3145). (8) Scroll enum — restore the full 3-line doc (ref 1258-1260), replacing the terse "/// See PageList.Scroll.". '
  + 'THEN SWEEP the whole screen.rs vs Screen.zig for ANY OTHER dropped production-method comments (G3b production-region missing) and restore them verbatim too. Also VERIFY the ~117 test-region G3b-missing all belong to DEFERRED tests (kitty image tests, the 5 TerminalFormatter Screen tests -> vt-12) — if any are dropped comments in a PORTED test, restore those verbatim; if they are deferred-test comments, leave them (they come with the deferred tests). No logic changes. '
  + 'GATES FOREGROUND: cargo build -p rustty-vt; cargo test -p rustty-vt (992 still pass, no regression); clippy --all-targets -D; fmt --check; `' + SCREEN_G3B + '` — report missing count + confirm EVERY production-region missing now maps to a documented kitty-ImageStorage/valgrind deferral PORT NOTE (residual_all_deferred). Commit "rustty: restore dropped Screen production doc comments (vt-11 fidelity fix)". Return ok+comments_restored+screen_g3b_missing+residual_all_deferred+builds+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'restore', phase: 'Restore' })

phase('Finale')
const finale = await robust(RULES +
  'TASK (Screen-SCC FINALE re-run — adversarial, default NOT_PARITY). Restore: ' + JSON.stringify(restore && { commit: restore.commit, screen_g3b_missing: restore.screen_g3b_missing, residual_all_deferred: restore.residual_all_deferred, blockers: restore.blockers }) + '\n'
  + 'Confirm FOREGROUND: cargo build --workspace; cargo test -p rustty-vt (full suite, no regression vs 992); `bash ' + GATES + '/sanitizers.sh rustty-vt screen address` + `... selection address` + `... screen_set address` (ASan clean); clippy --all-targets -D + fmt --check + ' + GATES + '/check_acyclic.sh. '
  + 'WHOLE-CLUSTER G3b: Selection/StringMap/ScreenSet missing=0; for Screen + formatter, EVERY residual missing must map to a DEFERRALS.md entry (kitty ImageStorage hooks/render_placement, TerminalFormatter + 100 formatter tests -> vt-12, valgrind PORT NOTE) — confirm residual_all_deferred=true and list any NOT covered as a blocker. ADVERSARIAL: re-sample 4 hard cluster tests (clone-selection-remap, resize-reflow-cross-page, selectLine semantic-prompt, selectionString rectangle) — assertion-for-assertion vs reference, none tautological/weakened. '
  + 'Verdict PARITY only if: workspace builds, full suite green (no regression), ASan clean, clippy/fmt/acyclic green, Selection/StringMap/ScreenSet G3b=0, EVERY Screen/formatter G3b-missing maps to a DEFERRALS.md entry, no tautological/weakened tests. On PARITY commit "rustty: complete Screen-SCC (vt-11) — Screen+Selection+formatter-prod+StringMap+ScreenSet". Return verdict + suite_tests + asan_clean + residual_all_deferred + tautological_or_weakened + commit + blockers + notes(what remains deferred to vt-12).',
  { schema: FINALE, effort: 'high', label: 'finale', phase: 'Finale' })
return { restore, finale }
