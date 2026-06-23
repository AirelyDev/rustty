export const meta = {
  name: 'vt3c-comment-fidelity',
  description: 'Restore the ~52 genuinely-dropped PageList METHOD doc comments flagged by the (now fast+scoped) G3b gate. R1 = no dropped reference comments. Mechanical, gate-driven loop-until-green, then commit. Method-only scope (tests are a separate workflow).',
  phases: [
    { title: 'Fix', detail: 'restore dropped method doc comments verbatim, loop until G3b methods missing=0' },
    { title: 'Verify', detail: 'independent re-check: G3b methods missing=0 + build/clippy/fmt' },
  ],
}
const G = '/Users/gael/projects/ghostty', R = '/Users/gael/projects/rustty'
const GATES = R + '/.port/tools/gates'
// Method-only G3b: strip the 205 test blocks (contiguous from line 5583) so we measure
// METHOD comment fidelity only; tests + their comments are a separate workflow.
const G3B_M = 'head -n 5582 ' + G + '/src/terminal/PageList.zig > /tmp/pl_methods_ref.zig && python3 ' + GATES + '/check_comment_coverage.py --crate vt --ref-files /tmp/pl_methods_ref.zig --port-glob "' + R + '/crates/rustty-vt/src/pagelist/*.rs"'
const RULES = 'OBEY: PARITY CONSTITUTION + ' + R + '/.port/PORT_RULES.md. R1 = EVERY reference comment preserved VERBATIM (retarget ghostty->rustty identifiers only per R2); a dropped reference comment is a hard violation. This is a COMMENT-RESTORATION task on already-ported methods — do NOT change code/logic, only restore dropped doc/inline comments to their correct location. Reference READ-ONLY ' + G + '/src/terminal/PageList.zig. Run all commands FOREGROUND and inspect captured output; NEVER background-and-poll an output file. '
async function robust(p, o) { let r = await agent(p, o); if (!r) r = await agent(p, { ...o, label: (o.label || 'a') + '-retry' }); return r }
const STEP = { type: 'object', additionalProperties: true, required: ['ok', 'blockers'], properties: { ok: { type: 'boolean' }, final_missing: { type: 'integer' }, restored: { type: 'integer' }, commit: { type: 'string' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }
const VERIFY = { type: 'object', additionalProperties: true, required: ['verdict', 'blockers'], properties: { verdict: { type: 'string', enum: ['GREEN', 'RED'] }, g3b_methods_missing: { type: 'integer' }, build_green: { type: 'boolean' }, notes: { type: 'string' }, blockers: { type: 'array', items: { type: 'string' } } } }

phase('Fix')
const fix = await robust(RULES +
  'TASK: restore EVERY dropped PageList method doc comment. STEP 1 — run FOREGROUND to get the missing list:\n  ' + G3B_M + ' 2>&1 | tee /tmp/g3b_m.txt\nThe "MISSING ref comment:" lines (currently ~52) are reference comments absent from pagelist/*.rs. STEP 2 — for EACH missing comment: locate it in PageList.zig (grep the distinctive phrase), identify which method/type/enum it documents, find that method in the matching pagelist/*.rs file, and re-insert the dropped comment line(s) VERBATIM at the correct position (same relative placement as the reference; retarget ghostty->rustty identifiers only). Many are multi-line doc blocks — restore the WHOLE block, not just the flagged line. A few may be REVIEW-level reworded comments (ratio 0.62-0.80) — re-align those to the reference wording too if cheap, but REVIEW items are soft (not RED). STEP 3 — re-run ' + G3B_M + ' and LOOP STEP 2 until its "missing=" count is 0. STEP 4 — confirm no code changed: cargo build -p rustty-vt + cargo clippy -p rustty-vt --all-targets -- -D warnings + cargo fmt -p rustty-vt --check all GREEN (foreground). Commit "rustty: restore dropped PageList method doc comments (vt-3c R1)" + trailer "Co-Authored-By: Claude Opus 4.8 (1M context)". If a flagged comment documents a genuinely Zig-only construct with NO Rust counterpart (rare), note it explicitly in notes rather than fabricating a home. Return ok+restored+final_missing+commit+blockers.',
  { schema: STEP, effort: 'high', label: 'comment-fix', phase: 'Fix' })

phase('Verify')
const verify = await robust(RULES +
  'TASK (independent verify — default RED if uncertain). Run FOREGROUND and inspect: (1) ' + G3B_M + '  -> report g3b_methods_missing = its "missing=" count. (2) cargo build -p rustty-vt + cargo clippy -p rustty-vt --all-targets -- -D warnings + cargo fmt -p rustty-vt --check -> build_green. Also spot-check 3 of the previously-missing comments (e.g. "fast-path function to erase exactly 1 row", "grow the number of rows available", "checks if a pin is valid") are now present verbatim in pagelist/*.rs. Verdict GREEN only if g3b_methods_missing == 0 AND build_green; else RED with specifics.',
  { schema: VERIFY, effort: 'high', label: 'verify', phase: 'Verify' })
return { fix, verify }
