export const meta = {
  name: 'realign-phase3',
  description: 'Phase 3 of the realignment plan: ratify R7-R12, build + audit GHOSTTY_LIFETIMES.tsv, unsafe census + UNSOUND_SAFE ledger, upstream main-parity sweep, marker migration (detectors T18 deferred until ci.sh is free)',
  whenToUse: 'After phases 0-2; runs concurrently with the G6 exception-routing agent (disjoint files)',
  phases: [
    { title: 'Ratify', detail: 'T15: R7-R12 into PORT_RULES.md as BINDING' },
    { title: 'Lifetimes build', detail: 'T16: enumerate -> classify shards -> adversarial verify -> assemble TSV' },
    { title: 'Lifetimes audit', detail: 'T16 step 3: ported types vs TSV; UB-class findings via mirihar' },
    { title: 'Census', detail: 'T17: count-unsafe.sh + snapshot + UNSOUND_SAFE.md seed' },
    { title: 'Main parity', detail: 'T19: upstream ghostty commits since 4749c4e9 -> verdict ledger + small ports' },
    { title: 'Markers', detail: 'T20: marker taxonomy census + migration' },
  ],
}

const PLAN = 'docs/superpowers/plans/2026-07-10-port-realignment-and-validation-hardening.md'
const LEARN = '.port/phase-2/BUN_PORT_LEARNINGS.md'

const OUT = {
  type: 'object',
  required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' },
    status: { type: 'string', enum: ['done', 'blocked'] },
    evidence: { type: 'string' },
    blocker: { type: 'string' },
    notes: { type: 'string' },
    commit_refs: { type: 'array', items: { type: 'string' } },
    data: { type: 'string', description: 'machine-readable payload when the task produces one (JSON string)' },
  },
}

const COMMON = `
REPO: /Users/gael/projects/rustty. Plan: ${PLAN}. Learnings: ${LEARN} (read your task's cited sections).
CONTEXT: a sibling agent is currently running long Miri lanes and owns .port/tools/gates/check_leaks.sh, .port/phase-2/MIRI_EXCEPTIONS.md and .port/phase-2/ci-phase12-close.log — do NOT edit or commit those three files, and do NOT run ci.sh or heavy cargo builds unless your task requires a targeted test run.
HARD RULES: /Users/gael/projects/ghostty is READ-ONLY (git fetch on it is the ONE sanctioned write — remote refs only, never checkout/pull/reset). R5: never weaken anything. Git: explicit paths, index.lock retry (sleep 3, up to 10x), no push, no reset/stash. Return via StructuredOutput.`

phase('Ratify')
const t15 = await agent(`${COMMON}
TASK T15 (DG1 resolved as: ratify all six). Append ${LEARN} §5's PROPOSED R7-R12 to .port/PORT_RULES.md as BINDING rules dated 2026-07-10, with these edits: (a) strip the word PROPOSED, add a provenance line citing BUN_PORT_LEARNINGS §5 + the ratification date; (b) where a rule references artifacts that now exist (release profiles from T10, MIRI_EXCEPTIONS routing, the mirihar doctrine), cite the in-repo reality; (c) add to R11 the LIFETIMES prospective prong: "no pointer-heavy tier ports without its GHOSTTY_LIFETIMES.tsv rows landing first (2-reviewer consensus)" (memory adopt-lifetimes-tsv / plan Task 16 Step 4 + Task 22 Step 1). Also add the plan's Task 22 pre-port checklist as a PORT_RULES section (6 items, copy from the plan) since it is pure rule text. Commit PORT_RULES.md alone: "PORT_RULES: ratify R7-R12 (semantic fidelity, comptime/build-switch, unsafe accounting, markers, adversarial verification + LIFETIMES, upstream parity) + pre-port tier checklist (T15/T22, DG1)".`,
  { label: 'T15:ratify', phase: 'Ratify', schema: OUT })
log(`T15: ${t15 ? t15.status : 'failed'}`)

phase('Lifetimes build')
const scout = await agent(`${COMMON}
TASK: T16 Step 1a — enumerate the RETROACTIVE GHOSTTY_LIFETIMES scope. In the ORACLE (/Users/gael/projects/ghostty/src, read-only), enumerate every struct field whose type is/contains a pointer (*T, ?*T, []T is a slice not counted unless it aliases long-lived state, [*]T, ?[*]T, *anyopaque, function pointers) in: terminal/{page.zig,PageList.zig,Screen.zig,Terminal.zig,Parser.zig,stream.zig,kitty/*}, the lib_vt C-API surface (src/lib_vt.zig or equivalent that rustty-vt-c ports), input/{Keymap*,Binding*}, font/ (the shaper hook surfaces rustty-font/font-shaper port), termio/*, plus the VENDORED deps rustty ports: libvaxis (Tty/Vaxis/Loop), libxev (intrusive nodes). For each: {file, struct, field, zig_type, decl_line}. Also read .port/audits/ if GHOSTTY_LIFETIMES.tsv already exists (don't duplicate). Return via StructuredOutput with data = JSON array of shard objects: group fields into 6-9 shards by area (vt-page, vt-screen-terminal, vt-kitty-parser, vt-c-api, input-font, termio, vaxis, xev), each shard {name, fields:[...]}. Aim for completeness over speed; cite file:line per field.`,
  { label: 'T16:scout', phase: 'Lifetimes build', schema: OUT })

let shards = []
try { shards = JSON.parse(scout.data || '[]') } catch (e) { log('scout data parse failed: ' + e.message) }
log(`T16 scout: ${shards.length} shards, ${shards.reduce((n, s) => n + (s.fields ? s.fields.length : 0), 0)} pointer fields`)
if (!shards.length) throw new Error('T16 scout returned no shards — aborting lifetimes phases')

const TSV_SCHEMA_NOTE = `TSV schema (Bun heritage, BUN_PORT_LEARNINGS §3.1): file<TAB>struct<TAB>field<TAB>zig_type<TAB>class<TAB>rust_type<TAB>evidence. Classes: OWNED, SHARED, BORROW_PARAM, BORROW_FIELD, STATIC, BACKREF, INTRUSIVE, FFI, ARENA, GHOSTTY_C, UNKNOWN. Classification oracle = the type's deinit + who frees ("deinit-as-ownership-oracle"); evidence = "file:line — what you saw" citing the deinit/free/alloc site. rust_type = the exact Rust type/discipline the class prescribes (Box<T>/Rc/&'static/raw *mut with owner note/Option<NonNull<T>>+TODO(port) for UNKNOWN).`

const classified = await pipeline(
  shards,
  s => agent(`${COMMON}
TASK: T16 classify shard "${s.name}" (${s.fields.length} pointer fields). ${TSV_SCHEMA_NOTE}
Fields (from the scout, verify each against the oracle source yourself):
${JSON.stringify(s.fields, null, 1)}
For EACH field: read the owning struct's deinit/destroy in the oracle, trace who allocates and who frees, classify, cite evidence. UNKNOWN is allowed and honest — never guess a class without a deinit/free citation. Return data = JSON array of TSV row objects {file,struct,field,zig_type,class,rust_type,evidence,confidence:"high"|"low"}.`,
    { label: `classify:${s.name}`, phase: 'Lifetimes build', schema: OUT }),
  (res, s) => {
    let rows = []
    try { rows = JSON.parse(res.data || '[]') } catch (e) { return { shard: s.name, rows: [], error: 'parse: ' + e.message } }
    return agent(`${COMMON}
TASK: T16 adversarial verify for shard "${s.name}". Another agent classified these pointer fields (rows below). Default-refute: re-check EVERY row with confidence "low" or class UNKNOWN, plus at least 15% of the "high" rows (pick spread across structs), against the oracle source. A row is wrong if the deinit/free evidence does not support the class, or rust_type contradicts the class. Return data = JSON {verified_rows:[corrected full row list, ALL rows incl. untouched ones], corrections: <count>, agreement: <fraction 0-1 of sampled rows that were correct>}.
ROWS: ${JSON.stringify(rows)}`,
      { label: `verify:${s.name}`, phase: 'Lifetimes build', schema: OUT })
      .then(v => {
        let d = null
        try { d = JSON.parse(v.data || 'null') } catch (e) { /* fall through */ }
        return { shard: s.name, rows: d ? d.verified_rows : rows, corrections: d ? d.corrections : -1, agreement: d ? d.agreement : -1 }
      })
  }
)

const goodShards = classified.filter(Boolean).filter(c => c.rows && c.rows.length)
log(`T16 classify+verify: ${goodShards.length}/${shards.length} shards, ${goodShards.reduce((n, c) => n + c.rows.length, 0)} rows, agreements: ${goodShards.map(c => `${c.shard}:${c.agreement}`).join(' ')}`)

const assembled = await agent(`${COMMON}
TASK: T16 assemble. Write /Users/gael/projects/rustty/.port/audits/GHOSTTY_LIFETIMES.tsv (create .port/audits/ if needed): header comment lines (#) documenting the schema, the 11-class taxonomy, the ratified R11 "trust it over local guessing / UNKNOWN => Option<NonNull<T>>+TODO(port)" doctrine, per-shard inter-classifier agreement stats (below), oracle pin 4749c4e9, date; then all rows sorted by file,struct,field. Then commit it: "audits: GHOSTTY_LIFETIMES.tsv — retroactive pointer-field ownership ledger (T16; Bun lifetime-classify heritage; R11)".
SHARD DATA: ${JSON.stringify(goodShards.map(c => ({ shard: c.shard, agreement: c.agreement, corrections: c.corrections, rows: c.rows })))}`,
  { label: 'T16:assemble', phase: 'Lifetimes build', schema: OUT })
log(`T16 TSV: ${assembled ? assembled.status : 'failed'}`)

phase('Lifetimes audit')
const audits = await parallel(goodShards.map(c => () =>
  agent(`${COMMON}
TASK: T16 Step 3 — audit shard "${c.shard}": for each TSV row below, find the PORTED Rust counterpart field (crates/rustty-vt, rustty-vt-c, rustty-vaxis, rustty-xev, rustty-font*, rustty-termio, rustty-input) and check its actual type/discipline against the row's rust_type prescription. Mismatch classes: (a) benign representational difference — note only; (b) discipline violation with UB potential (e.g. a BACKREF held as &mut, an INTRUSIVE node using references, an OWNED ptr never freed on a path) — a FINDING with file:line both sides; (c) row itself wrong because the port already restructured faithfully — correct the row (return corrected rows). Do NOT edit code in this pass; findings only. Return data = JSON {findings:[{severity:"high"|"medium"|"low",rust_file,tsv_row,detail}], corrected_rows:[...]}.
ROWS: ${JSON.stringify(c.rows)}`,
    { label: `audit:${c.shard}`, phase: 'Lifetimes audit', schema: OUT })
))
const findings = audits.filter(Boolean).flatMap(a => {
  try { return (JSON.parse(a.data || '{}').findings) || [] } catch (e) { return [] }
})
log(`T16 audit: ${findings.length} findings (${findings.filter(f => f.severity === 'high').length} high)`)

const auditClose = await agent(`${COMMON}
TASK: T16 audit close. Findings from the LIFETIMES audit below. (1) Adversarially re-check every HIGH finding against both sources; (2) fix confirmed HIGH ones surgically ONLY if the fix is local and provable — any SB/TB-class fix must reproduce-then-prove on a mirihar harness per doctrine (if that is too large for this pass, FILE it instead: append to .port/phase-2/DEFERRALS.md under a "LIFETIMES-AUDIT findings (2026-07-10)" section with full detail); (3) apply corrected_rows + a "# audited-against-port: 2026-07-10" stamp to .port/audits/GHOSTTY_LIFETIMES.tsv; (4) write the findings ledger to .port/audits/LIFETIMES_AUDIT_2026-07-10.md (all severities). Commit the touched files: "audits: LIFETIMES retroactive audit — <n> findings (<m> fixed, <k> filed) (T16 step 3)".
FINDINGS: ${JSON.stringify(findings)}`,
  { label: 'T16:audit-close', phase: 'Lifetimes audit', schema: OUT })
log(`T16 audit close: ${auditClose ? auditClose.status : 'failed'}`)

phase('Census')
const t17 = await agent(`${COMMON}
TASK T17 (plan Task 17 steps 1+3 — step 2's bulk SAFETY-tagging and step 4's ci.sh wiring are DEFERRED to the detectors pass; note that in your commit body). (1) Create .port/audits/count-unsafe.sh per ${LEARN} §4-D7: the exact committed rg command (unsafe \\{|unsafe fn|unsafe extern|unsafe impl|unsafe trait, vendored C source and crates-sys excluded from the pure-Rust tier), per-crate density per kLOC split by R6 tier (pure-Miri vs FFI-ASan vs -sys), block line-span distribution (single-line vs structural), JSON snapshot to .port/audits/unsafe-census-<shortsha>.json with categories reconciling exactly to the total. Run it, commit script + first snapshot. (2) Seed .port/audits/UNSOUND_SAFE.md: grep the workspace for safe pub fns wrapping raw-ptr derefs, &self->&mut accessors, lifetime-less pointer-holding pub structs — seed from the LIFETIMES audit artifacts (.port/audits/LIFETIMES_AUDIT_2026-07-10.md if present) + your own targeted greps of rustty-vt page/pagelist + rustty-vt-c; triage each entry reachable-from-VT-stream / internal-only / theoretical; cite file:line. Commit: "audits: unsafe census v1 + UNSOUND_SAFE ledger seed (T17, R9)".`,
  { label: 'T17:census', phase: 'Census', schema: OUT })
log(`T17: ${t17 ? t17.status : 'failed'}`)

phase('Main parity')
const t19 = await agent(`${COMMON}
TASK T19 (plan Task 19). (1) git -C /Users/gael/projects/ghostty fetch origin (sanctioned; refs only). (2) Enumerate git -C /Users/gael/projects/ghostty log --oneline 4749c4e9..origin/main -- src/ (and include/ if it exists). (3) For EACH commit (batch sensibly if >40: triage by touched-file overlap with ported code first): read the diff, map to the rustty file(s), verdict ported/missing/partial/not-applicable with file:line evidence. Doctrine verbatim: "a merge does NOT port the fix; only an explicit .rs change does" and "ALSO port the test hunks from the same commit". (4) Write .port/phase-2/MAIN_PARITY.md: pin range, per-commit verdict table, evidence. (5) Port ONLY the small clear ones (single-function fixes with tests) assertion-for-assertion; anything structural gets a MAIN_PARITY backlog entry instead. Run the touched crates' tests for anything you port. Commit ledger + ports separately: "parity: MAIN_PARITY ledger — upstream 4749c4e9..origin/main triaged (T19, R12)" / "rustty-*: upstream parity ports — <list> (T19, R12)".`,
  { label: 'T19:main-parity', phase: 'Main parity', schema: OUT })
log(`T19: ${t19 ? t19.status : 'failed'}`)

phase('Markers')
const t20 = await agent(`${COMMON}
TASK T20 (plan Task 20). (1) Census existing markers: rg -c "TODO|FIXME|PORT NOTE|PERF|unimplemented!|todo!" crates/ per crate + classify the free-form ones. (2) Migrate to the ratified R10 taxonomy (TODO(port): <reason> / PERF(port): <zig idiom> / PORT NOTE: <reshape reason>): mechanical rewrites ONLY where intent is unambiguous from the existing text/ledger; ambiguous ones get TODO(port): + a pointer to their DEFERRALS entry. Do NOT invent new deferrals and do NOT touch ported reference comments (R1) — only rustty-authored markers. (3) Write the census (before/after counts per category per crate) to .port/audits/MARKERS.md. Commit: "markers: R10 taxonomy migration + census (T20)". Verify cargo build --workspace still green after (comment-only edits, but prove it).`,
  { label: 'T20:markers', phase: 'Markers', schema: OUT })

return {
  t15, scoutShards: shards.length, rows: goodShards.reduce((n, c) => n + c.rows.length, 0),
  tsv: assembled ? assembled.status : 'failed',
  auditFindings: findings.length, auditClose: auditClose ? auditClose.status : 'failed',
  t17: t17 ? t17.status : 'failed', t19: t19 ? t19.status : 'failed', t20: t20 ? t20.status : 'failed',
  summary: `T15 ${t15 ? t15.status : 'fail'}; TSV ${goodShards.reduce((n, c) => n + c.rows.length, 0)} rows (${assembled ? assembled.status : 'fail'}); audit ${findings.length} findings (${auditClose ? auditClose.status : 'fail'}); census ${t17 ? t17.status : 'fail'}; parity ${t19 ? t19.status : 'fail'}; markers ${t20 ? t20.status : 'fail'}`,
}
