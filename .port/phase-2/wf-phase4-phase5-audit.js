export const meta = {
  name: 'phase4-phase5-audit',
  description: 'Read-only: validation sweep #2 over sweep-#1 blind spots (Phase 4) + pre-port LIFETIMES/invariant/comptime analysis for the unported tiers (Phase 5). Adversarially verified, synthesized to reports.',
  whenToUse: 'After the vt/vt-c Miri close; runs concurrently with the source-editing fix workflow (no build, no commits until synth)',
  phases: [
    { title: 'Hunt', detail: '8 blind-spot auditors + 3 pre-port tier analysts, all read-only' },
    { title: 'Verify', detail: 'adversarial refutation of blocker/high sweep-2 findings' },
    { title: 'Synthesize', detail: 'VALIDATION_REPORT_2.md + per-tier pre-port artifacts' },
  ],
}

const FINDINGS = {
  type: 'object',
  required: ['auditor', 'coverage', 'findings'],
  properties: {
    auditor: { type: 'string' },
    coverage: { type: 'string', description: 'what was fully examined vs sampled vs skipped — honest; silent truncation forbidden' },
    findings: {
      type: 'array',
      items: {
        type: 'object',
        required: ['severity', 'title', 'file', 'detail', 'evidence'],
        properties: {
          severity: { type: 'string', enum: ['blocker', 'high', 'medium', 'low'] },
          title: { type: 'string' },
          file: { type: 'string' },
          detail: { type: 'string', description: 'defect + concrete failure scenario (inputs/state -> wrong behavior)' },
          evidence: { type: 'string', description: 'exact code/comment/count quoted from BOTH rustty and ghostty where a comparison is claimed' },
          fix_sketch: { type: 'string' },
        },
      },
    },
  },
}

const PREPORT = {
  type: 'object',
  required: ['tier', 'summary', 'markdown'],
  properties: {
    tier: { type: 'string' },
    summary: { type: 'string' },
    markdown: { type: 'string', description: 'the full pre-port artifact markdown for this tier (LIFETIMES rows table + invariant catalog + comptime inventory + constants list + risk notes)' },
  },
}

const VERDICT = {
  type: 'object',
  required: ['refuted', 'reason'],
  properties: { refuted: { type: 'boolean' }, reason: { type: 'string' }, corrected_severity: { type: 'string', enum: ['blocker', 'high', 'medium', 'low'] } },
}

const COMMON = `
CONTEXT: rustty (/Users/gael/projects/rustty) is a full-fidelity Zig->Rust port of Ghostty. READ-ONLY oracle: /Users/gael/projects/ghostty @ 4749c4e9 (NEVER modify it, never modify anything — this whole workflow is read-only; return findings, do not edit or commit). Rules .port/PORT_RULES.md (R1 fidelity: every line/comment/test/assertion; contracts 3234 tests/11975 assertions/42069 comments/677 surface ids). Prior work: .port/phase-2/VALIDATION_REPORT.md (sweep #1 — §5 refuted appendix lists what NOT to re-raise), .port/audits/GHOSTTY_LIFETIMES.tsv (pointer-field ledger), .port/audits/UNSOUND_SAFE.md, BUN_PORT_LEARNINGS.md.
HARD: read-only (grep/read/python-count only; NO cargo build/test/miri — a sibling workflow holds build locks). Cite exact file:line; a comparison claim quotes BOTH sides. Honest coverage. Do NOT re-raise anything in VALIDATION_REPORT.md §5. Return via StructuredOutput.`

phase('Hunt')
const auditors = [
  { key: 'terminal-print', prompt: `${COMMON}
AUDIT (sweep-1 blind spot #2): Terminal.print / printCell — the highest-value un-walked hot path. Walk ghostty src/terminal/Terminal.zig print/printCell/printWrap/print-with-grapheme vs crates/rustty-vt/src/terminal.rs line-by-line. Hunt: grapheme-cluster attach-on-overwrite, wide-char (spacer head/tail) placement at line ends and across wrap, pending-wrap state, cursor advance, combining marks, zero-width, tab expansion in print. Quote both sides for every suspected drift. Cover every function in the print path; honest coverage.` },
  { key: 'selection-fns', prompt: `${COMMON}
AUDIT (sweep-1 blind spot #3): selection + dump. Walk ghostty Selection/selectLine/selectWord/selectOutput/promptClickMove and Screen.dumpString/testWriteString vs their rustty-vt counterparts. Hunt: boundary conditions at page edges, wide-char/grapheme boundaries in word selection, rectangular vs linear, off-by-one in ranges, prompt-click semantic-prompt detection. Quote both sides per drift.` },
  { key: 'comment-census', prompt: `${COMMON}
AUDIT (sweep-1 blind spot #6, R1): comment-fidelity CONTENT census. G3/G3b only check counts+distinctive-match; nobody has content-verified the 42069-line comment contract. Sample: pick 6 high-comment-density ported files across crates (vt terminal.rs/screen.rs, config, input, font), and for ~15 comment blocks each, diff the ported comment TEXT against the Zig source comment at the mapped site. Flag: dropped meaning, summarized-away detail, stale text describing the OLD Zig primitive where a Rust primitive with different semantics replaced it (detector D9 class), citations/spec-refs lost. Report a defect rate estimate + concrete examples with both sides quoted.` },
  { key: 'apprt-c-bodies', prompt: `${COMMON}
AUDIT (sweep-1 blind spot #7): the ~60% of rustty-apprt-c export bodies + vt-c render/selection full bodies never reviewed. For every extern "C" fn in rustty-apprt-c not covered by sweep #1: panic-across-FFI (indexing/unwrap/assert without catch_unwind = UB), null-parity vs ghostty, CString/buffer lifetime, container_of/offset math. Also vt-c render.rs + selection full bodies. List violators with file:line; sample against ghostty where a parity claim is made.` },
  { key: 'inspector-bodies', prompt: `${COMMON}
AUDIT (sweep-1 blind spot #7): rustty-inspector widget bodies + dcimgui FFI. Review the ported Dear ImGui widget render sections and the imgui_impl FFI: panic-across-FFI, pointer lifetime of imgui draw data, the 13 unimplemented!() seams (are they tracked in DEFERRALS?), any &mut aliasing in the render loop. Note which are live vs deferred-leaf.` },
  { key: 'bulk-unsafe', prompt: `${COMMON}
AUDIT (sweep-1 blind spot #4): the bulk unsafe bodies only sampled in sweep #1 — terminal.rs (~894 unsafe blocks), screen.rs (~529), formatter.rs (~247). Guided by .port/audits/GHOSTTY_LIFETIMES.tsv + UNSOUND_SAFE.md, review for the project's known hazard class: long-lived raw alias into a field invalidated by an interleaved &mut through self (the search/screen.rs + ref_counted_set bugs just fixed are the template). Prioritize blocks that materialize &mut from raw or hold a raw across a &mut-self method call. This is static (no Miri) — flag CANDIDATES with the alias/retag reasoning + file:line, ranked by confidence; note which the recent full Miri sweep already cleared (those tests passed, so live-exercised paths are proven — focus on paths the 84 Miri-excepted/less-exercised tests cover).` },
  { key: 'unicode-tables', prompt: `${COMMON}
AUDIT (sweep-1 blind spot #8, §3g): rustty-unicode generated tables provenance. props/grapheme/symbols stage tables are embedded generator output, verified by provenance header only — the one comptime-artifact class R1 takes on faith. Read the generator provenance, locate ghostty's generated arrays (src/unicode/ + the uucode/ziglyph gen), and design the concrete one-time verification: how to dump ghostty's arrays and numerically diff vs the embedded Rust tables + a checksum test to add. You cannot build here — deliver the exact commands/script + the checksum-test spec as a finding (fix_sketch), and spot-check a handful of codepoints by reading both tables directly if feasible.` },
  { key: 'cross-target-cfg', prompt: `${COMMON}
AUDIT (sweep-1 blind spot #9): cfg-gated code not type-checked unless the target is built (Bun CI learning). Enumerate every #[cfg(...)] / cfg!( ) gate in the workspace guarding platform-specific code (linux/windows/wasm/fontconfig/ConPTY/flatpak/gtk). For each: does the gated code even parse/typecheck (obvious type errors, references to items that don't exist, stale APIs)? Since macOS-first is locked, these are un-type-checked — flag any that look broken. Also list which -sys crates won't cross-compile (host C toolchain dependency). Deliver the cargo-check matrix recommendation.` },
]

const analysts = [
  { key: 'preport-gtk', prompt: `${COMMON}
PHASE-5 PRE-PORT ANALYSIS for the apprt-gtk tier (~55 .zig / ~23k LOC GTK4/libadwaita + OpenGL, the largest unported chunk; macOS-first means it lands LAST but the pre-port gate is required by R11/PORT_RULES pre-port checklist). Produce the pre-port artifact (read ghostty src/apprt/gtk*/): (1) LIFETIMES.tsv rows — every pointer-typed struct field in the gtk sources (GObject refs, widget pointers, signal-handler user-data, GL context) classified per the 11-class taxonomy (OWNED/SHARED/BORROW_*/STATIC/BACKREF/INTRUSIVE/FFI/ARENA/GHOSTTY_C/UNKNOWN) with file:line evidence; (2) paired-invariant catalog (cross-crate predicates the tier introduces); (3) comptime-parameter inventory (each maps to Rust const/generic/macro); (4) constants extraction; (5) risk notes (gtk4-rs binding stack, GObject lifetime hazards, the .blp/.css/glsl reuse-verbatim decision). Return markdown for .port/phase-2/PREPORT_GTK.md.` },
  { key: 'preport-swift', prompt: `${COMMON}
PHASE-5 PRE-PORT ANALYSIS for the macos-swift-app tier (the nearest shippable milestone: C-ABI glue linking the reused ~32k-LOC Swift AppKit/SwiftUI frontend — ghostty_*->rustty_* symbol bridge, modulemap/umbrella header, .xcframework packaging). Read ghostty src/apprt/embedded.zig + the macOS/ C-API surface + include/ghostty.h. Produce (1) LIFETIMES.tsv rows for the C-ABI boundary structs (surface/app handles, callback user-data, the objc/AppKit object pointers crossing the boundary) — flag every FFI/GHOSTTY_C field and the drop-on-wrong-thread hazard (AppKit/Metal main-thread affinity); (2) the paired invariants across the Swift<->Rust seam; (3) the symbol-identity decision surface (this milestone is gated on open decision #1 — the ghostty_* symbol fork — DO NOT resolve it, just lay out what the pre-port needs either way); (4) catch_unwind/abort policy for the 271 exported fns. Return markdown for .port/phase-2/PREPORT_SWIFT_APP.md.` },
  { key: 'preport-inspector', prompt: `${COMMON}
PHASE-5 PRE-PORT ANALYSIS for the gui-inspector tier (~4.4k LOC Dear ImGui widgets + imgui_impl_metal backend; dcimgui FFI leaf done, 13 widget seams open in rustty-inspector). Read ghostty src/inspector/*. Produce (1) LIFETIMES.tsv rows for the inspector's pointer fields (imgui context, draw-data, the Surface/Terminal back-references it renders); (2) invariant catalog (inspector<->surface state coupling — the D5 dirty-tracking/mode-state pairs it consumes); (3) comptime inventory; (4) which of the 13 seams are mechanical vs need design. Return markdown for .port/phase-2/PREPORT_INSPECTOR.md.` },
]

const audits = await parallel(auditors.map(a => () => agent(a.prompt, { label: `audit:${a.key}`, phase: 'Hunt', schema: FINDINGS })))
const preports = await parallel(analysts.map(a => () => agent(a.prompt, { label: `preport:${a.key}`, phase: 'Hunt', schema: PREPORT })))

const reports = audits.filter(Boolean)
const allF = reports.flatMap(r => r.findings.map(f => ({ ...f, auditor: r.auditor })))
log(`Hunt: ${reports.length}/${auditors.length} auditors, ${allF.length} findings; ${preports.filter(Boolean).length}/${analysts.length} pre-port artifacts`)

phase('Verify')
const needV = allF.filter(f => f.severity === 'blocker' || f.severity === 'high')
const rest = allF.filter(f => !(f.severity === 'blocker' || f.severity === 'high'))
const verified = await parallel(needV.map(f => () =>
  agent(`${COMMON}
ADVERSARIAL VERIFIER. Refute this sweep-2 finding: re-read both sides, re-run counts, prove it wrong/exaggerated/already-handled (compensating code, DEFERRALS, MIRI_EXCEPTIONS, intentional per PORT_RULES, or already in VALIDATION_REPORT.md §5 refuted). Default refuted=true if evidence doesn't decisively hold.
FINDING (${f.auditor}): ${f.title} [${f.severity}] ${f.file}\n${f.detail}\nEVIDENCE: ${f.evidence}`,
    { label: `verify:${f.title.slice(0, 36)}`, phase: 'Verify', schema: VERDICT }).then(v => ({ ...f, verdict: v }))))
const confirmed = verified.filter(Boolean).filter(v => v.verdict && !v.verdict.refuted).map(v => ({ ...v, severity: v.verdict.corrected_severity || v.severity }))
const refuted = verified.filter(Boolean).filter(v => v.verdict && v.verdict.refuted)
log(`Verify: ${confirmed.length} confirmed, ${refuted.length} refuted`)

phase('Synthesize')
const synth = await agent(`${COMMON}
Write TWO deliverables (you MAY write files — this synthesis step is the only writer; write via the Write tool, do NOT commit, the orchestrator handles commits):
1. /Users/gael/projects/rustty/.port/phase-2/VALIDATION_REPORT_2.md — sweep #2. Structure like VALIDATION_REPORT.md: Verdict; Confirmed findings (ranked, file:line, failure scenario, fix sketch, rule violated); Medium/low ledger; Coverage & blind spots (what THIS sweep still didn't reach); Refuted appendix.
2. The three pre-port artifacts already drafted (write each to its named path): .port/phase-2/PREPORT_GTK.md, PREPORT_SWIFT_APP.md, PREPORT_INSPECTOR.md — using the markdown from the analysts below verbatim (light cleanup only).

CONFIRMED (verified):
${JSON.stringify(confirmed, null, 1)}
MEDIUM/LOW (unverified):
${JSON.stringify(rest, null, 1)}
REFUTED (appendix, one line each):
${JSON.stringify(refuted.map(r => ({ t: r.title, why: r.verdict.reason })), null, 1)}
COVERAGE:
${JSON.stringify(reports.map(r => ({ a: r.auditor, c: r.coverage })), null, 1)}
PRE-PORT ARTIFACTS:
${JSON.stringify(preports.filter(Boolean), null, 1)}

After writing all four files, return a <=350-word executive summary: sweep-2 verdict, top findings, and the pre-port readiness of each tier.`,
  { label: 'synthesize', phase: 'Synthesize' })

return { summary: synth, confirmed: confirmed.length, refuted: refuted.length, preports: preports.filter(Boolean).map(p => p.tier),
  files: ['.port/phase-2/VALIDATION_REPORT_2.md', '.port/phase-2/PREPORT_GTK.md', '.port/phase-2/PREPORT_SWIFT_APP.md', '.port/phase-2/PREPORT_INSPECTOR.md'] }
