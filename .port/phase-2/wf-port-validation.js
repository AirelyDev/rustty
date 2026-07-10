export const meta = {
  name: 'rustty-port-validation',
  description: 'First full validation sweep of the rustty port: fidelity-contract audits, unsafe/SB soundness review, Zig→Rust semantic-mismatch hunt, logic drift vs the ghostty oracle — findings adversarially verified',
  whenToUse: 'Run after major porting milestones to catch fidelity drift and latent bugs',
  phases: [
    { title: 'Hunt', detail: '12 parallel read-only auditors over contracts, unsafe, semantics, logic' },
    { title: 'Verify', detail: 'adversarial refutation of every high+ finding' },
    { title: 'Synthesize', detail: 'write VALIDATION_REPORT.md' },
  ],
}

const FINDINGS = {
  type: 'object',
  required: ['auditor', 'coverage', 'findings'],
  properties: {
    auditor: { type: 'string' },
    coverage: { type: 'string', description: 'what you actually examined vs sampled vs skipped — be honest, silent truncation is forbidden' },
    findings: {
      type: 'array',
      items: {
        type: 'object',
        required: ['severity', 'title', 'file', 'detail', 'evidence'],
        properties: {
          severity: { type: 'string', enum: ['blocker', 'high', 'medium', 'low'] },
          title: { type: 'string' },
          file: { type: 'string', description: 'repo-relative path (rustty side), plus line if known' },
          detail: { type: 'string', description: 'the defect + concrete failure scenario (inputs/state → wrong behavior)' },
          evidence: { type: 'string', description: 'exact code/comment/count quoted from BOTH sides where a comparison is claimed' },
          fix_sketch: { type: 'string' },
        },
      },
    },
  },
}

const VERDICT = {
  type: 'object',
  required: ['refuted', 'reason'],
  properties: {
    refuted: { type: 'boolean' },
    reason: { type: 'string' },
    corrected_severity: { type: 'string', enum: ['blocker', 'high', 'medium', 'low'] },
  },
}

const COMMON = `
CONTEXT: rustty (/Users/gael/projects/rustty) is a full-fidelity Zig→Rust port of Ghostty. The READ-ONLY reference oracle is /Users/gael/projects/ghostty @ 4749c4e9 (Zig 0.15.2). Binding rules (.port/PORT_RULES.md): R1 every line/comment/test/assertion ported, no loss (Phase-0 contracts: 3234 test blocks, 11975 assertions floor, 42069 comment lines, 677 parity-surface ids — inventories in .port/phase-0/inventory/); R4 acyclic crates; R5 never weaken gates; R6 Miri primary + ASan for FFI (23 PageList Miri exceptions documented in .port/phase-2/MIRI_EXCEPTIONS.md).

HARD CONSTRAINTS for you:
- READ-ONLY: do not modify any file, do not run cargo build/test/clippy/miri (a workspace build holds the target-dir lock right now). Grep/read/wc only. Python3 for counting is fine.
- NEVER modify anything under /Users/gael/projects/ghostty.
- Cite exact file:line evidence for every finding. A comparison claim must quote BOTH the Zig and the Rust side.
- Report honest coverage: what you fully examined vs sampled vs skipped.
- No finding is too small if it violates R1 (a dropped comment/assertion IS a finding), but rank severity by user impact: blocker = wrong behavior/UB/contract breach, high = likely bug or fidelity loss, medium = drift/hygiene, low = nit.
Return via StructuredOutput.`

phase('Hunt')
const auditors = [
  {
    key: 'contracts-vt',
    prompt: `${COMMON}
AUDIT: fidelity contracts for rustty-vt (the crown jewel, ~136k LOC, ~2165 tests) vs ghostty src/terminal/*.
1. Reconstruct the per-file test-block inventory: count 'test "' and 'test {' blocks per ghostty src/terminal/*.zig file (and terminal subdirs), then find each file's rustty counterpart under crates/rustty-vt/src and count #[test] fns. Produce a per-file delta table. Cross-check against .port/phase-0/inventory/ if it has per-file granularity.
2. For the 5 files with the largest Zig test counts, spot-check 10 random tests each: does the Rust test exist AND carry the same assertions (assertion-for-assertion per R1)? Quote one concrete example of any dropped/weakened assertion.
3. Check .port/phase-2/vt12-terminal-missing-tests.txt — is it resolved or still open? Verify a sample of its entries against current code.
Every per-file test deficit = one finding (group small ones).`,
  },
  {
    key: 'contracts-breadth',
    prompt: `${COMMON}
AUDIT: fidelity contracts breadth-first over the NON-vt ported crates: rustty-{config,input,font,font-shaper,os,datastruct,unicode,simd,terminfo,util,args,cli,termio,renderer,vaxis,zf,z2d,wuffs,xev}.
1. For each, identify its ghostty/vendored-Zig source counterpart (config→src/config, input→src/input, font→src/font, os→src/os, vaxis→vendored libvaxis, zf→vendored zf, z2d→vendored z2d, xev→libxev, etc. — check .port docs / crate lib.rs headers for the mapping). Count Zig test blocks vs Rust #[test] per crate; delta table.
2. Spot-check 3 tests per crate for assertion fidelity.
3. Grep all these crates for #[ignore], assert!(true), empty test bodies { }, and tests whose body is only a construction with no assertion — each is a potential R1/R5 violation.
Report per-crate deltas as findings.`,
  },
  {
    key: 'headers-ledger',
    prompt: `${COMMON}
AUDIT: status-header and ledger integrity — this directly gates G1/G2/G3/G9 which only turn binding when a crate's lib.rs PORT STATUS says done.
1. For all 39 crates: extract the PORT STATUS header from lib.rs (grep -A3 'PORT STATUS' or similar). Compare each claimed status against reality (test counts, unimplemented!/todo! counts, .port/phase-2 batch reports). Known-suspect stale ones: rustty-vt says stub, rustty-input says stub, rustty-cfg-fmt says stub. List every mismatch in BOTH directions (understated AND overstated).
2. Enumerate every live unimplemented!()/todo!() in the workspace with file:line and one-line context; verify each maps to an entry in the deferral ledger (find DEFERRALS.md — likely .port/ or repo root) or a tracked design doc. Untracked seam = high finding.
3. Audit DEFERRALS.md itself for drift: entries claiming done that aren't, and landed work (D6 runtime-thread, macOS windowing W0-W5, TUI-CLI, synthetic/bench/extra) missing or stale in the ledger.
4. Check .port/phase-2/GAP_ANALYSIS.md staleness: list its claims contradicted by git log since 2026-07-08.`,
  },
  {
    key: 'unsafe-vt',
    prompt: `${COMMON}
AUDIT: unsafe soundness in the pure-Rust hot core: rustty-vt (page.rs, pagelist, screen, terminal, style, hyperlink/kitty if raw-ptr-touched) + rustty-datastruct + rustty-zigstd + rustty-vaxis.
Known past UB classes fixed in this codebase (recurrence-hunt for the PATTERN, not the fixed instances): (a) &mut Page/Row/[Cell] fn args re-read across assert_integrity → Stacked Borrows UB, fixed via raw-ptr args; (b) long-lived raw alias into a field + interleaved &mut through self → SB/TB UB, fixed via raw-self helpers with narrow access.
1. Inventory: count unsafe blocks/fns per file (top 20 files).
2. For the 12 highest-density files, review each unsafe block: does it re-materialize &mut while a raw alias may live? ptr arithmetic against the right allocation? correct provenance through offset_of!/container_of patterns? slice::from_raw_parts lifetimes? MaybeUninit init-before-assume? Any Vec/Box aliasing its own interior?
3. Scrutinize .port/phase-2/MIRI_EXCEPTIONS.md: for 5 of the 23 excepted PageList tests, check the claimed compensating coverage (lighter Miri tests + ASan) actually exists in-tree.
4. Grep for mem::transmute, from_raw_parts, as *mut through & (shared→mut casts), static mut — justify or flag each hit in these crates.`,
  },
  {
    key: 'unsafe-ffi',
    prompt: `${COMMON}
AUDIT: FFI-boundary soundness: rustty-vt-c, rustty-apprt-c, rustty-oniguruma, rustty-simd, rustty-font/-sys crates, rustty-crash, rustty-inspector, rustty-wuffs, plus objc2 usage in renderer/xev/os.
1. Panic-across-FFI: every extern "C" fn exported to C — does it catch_unwind or provably not panic (indexing, unwrap, assert can panic)? Unwinding across extern "C" is immediate UB/abort. List violators.
2. The container_of/offset_of! trampoline seam in vt-c (TerminalWrapper recovery): verify offset math, repr assumptions (is the struct repr(C)?), and that the alias rules hold.
3. CString/CStr lifetimes: any as_ptr() on a temporary CString? Any returned pointer into a Rust buffer that can move/drop?
4. Null-handling parity: do rustty_* C-API fns null-check like ghostty's counterparts? Sample 10 exported fns against ghostty's src/lib_vt or include/ghostty behavior.
5. Global C init thread-safety: onig_initialize is OnceLock'd (past fix) — audit OTHER C libs (freetype, harfbuzz, fontconfig, sentry/breakpad, dcimgui, Metal/objc2 main-thread requirements) for unsynchronized global init or thread-affinity violations.
6. Callbacks passed to C: Send/Sync soundness, catch_unwind, user-data pointer lifetime.`,
  },
  {
    key: 'semantics-numeric',
    prompt: `${COMMON}
AUDIT: Zig→Rust NUMERIC semantic mismatches — the class behind several Bun post-rewrite panics. Ghostty compiles Debug/ReleaseSafe (overflow+bounds PANIC in Zig); Rust release wraps silently on overflow and always bounds-checks. So:
1. Overflow drift: Zig +%/-%/*% (wrapping) must map to wrapping_*; plain +/-/* in Zig (which traps in ReleaseSafe) maps to plain Rust ops which PANIC in debug but WRAP in release — meaning a Zig crash-on-overflow becomes silent corruption in rustty release builds. Grep the hot arithmetic paths (cursor math, page offsets, grid coords, SGR params, scrollback sizes) for suspicious plain arithmetic ported from trapping Zig; sample 10 sites, compare with the Zig source, and assess: should rustty enforce overflow-checks=true in release for parity (R1) — check what Cargo.toml profiles currently say.
2. Cast drift: Zig @intCast (safety-checked trap) vs Rust 'as' (silent truncate/wrap). Grep for ' as u8', ' as u16', ' as u32', ' as usize', ' as i32' in rustty-vt/config/input hot paths; sample 15 sites against the Zig source; flag any where Zig would trap but Rust silently truncates. Same for @floatToInt vs 'as'.
3. Divide/shift: Zig @divTrunc/@divFloor/@mod/@rem distinctions vs Rust %/ (Rust % is rem, not mod!) — grep for % on possibly-negative ints and compare each against the Zig operator used.
4. Slice casting parity: bytemuck/align_to/from_raw_parts reinterpret sites — odd-length handling: does Zig side assume exact divisibility (@ptrCast slice) where Rust code truncates or vice versa?
Deliver a table of confirmed drift sites.`,
  },
  {
    key: 'semantics-assert-fmt',
    prompt: `${COMMON}
AUDIT: assertion/format/comptime semantic mismatches (Bun regression classes 1 and 4).
1. Side-effectful debug_assert!: grep every debug_assert!/debug_assert_eq!/debug_assert_ne! in the workspace whose argument contains a function call; classify each call as pure or side-effecting (read the callee). In Zig, assert(expr) ALWAYS evaluates expr in ReleaseSafe; Rust debug_assert! compiles the whole expression OUT in release — a ported side-effecting assert silently drops the side effect. Also the inverse: Zig std.debug.assert compiled out in ReleaseFast but ghostty uses ReleaseSafe — check which Zig assert semantics each ported site had and whether assert! vs debug_assert! was chosen correctly (R1: preserve behavior).
2. unreachable drift: Zig 'unreachable' is a safety-checked trap in ReleaseSafe but UB in ReleaseFast; Rust unreachable!() panics, unreachable_unchecked() is UB. Grep for unreachable_unchecked — every hit must be provably unreachable; flag any that map to Zig ReleaseSafe checked-unreachable.
3. Comptime artifacts bit-for-bit (R1): find the ported parse/transition tables (VT parser state machine, UTF-8 decode tables, key encode tables). Verify at least the VT parser table matches ghostty's comptime-generated one (compare table contents or the generation logic assertion-by-assertion). Any generation-logic paraphrase that could differ in an edge state = blocker.
4. Format strings: Zig comptime std.fmt vs Rust format!/write! — grep ported formatter code (dump formatter, cfg-fmt, help strings) for {} placeholder-count or escaping drift ({{ vs {) and OSC/CSI sequences built via format strings where a literal { or ESC byte matters.`,
  },
  {
    key: 'logic-stream',
    prompt: `${COMMON}
AUDIT: line-level logic drift vs the oracle — VT stream/parser front half.
Pick these ghostty files and their rustty-vt counterparts: src/terminal/stream.zig, src/terminal/Parser.zig, src/terminal/csi.zig, src/terminal/osc.zig, src/terminal/dcs.zig (or nearest equivalents; locate via crate layout).
For EACH: walk the Zig file top-to-bottom against the Rust port and hunt for logic drift: inverted conditions, off-by-one in param defaults (CSI params default to 1 vs 0 per-command!), missing match arms for action/mode values, early-return vs fallthrough differences, changed saturation/clamping, missed 'intermediates' handling, C0-in-CSI interaction, OSC terminator (BEL vs ST) handling, UTF-8 boundary handling in the stream. This is a REAL read of both sides, not a spot-check — cover every public function in these files; state coverage honestly if a file is too large and you sampled.
Quote both sides for every suspected drift.`,
  },
  {
    key: 'logic-screen',
    prompt: `${COMMON}
AUDIT: line-level logic drift vs the oracle — screen/page back half.
Ghostty files: src/terminal/Screen.zig, src/terminal/Terminal.zig (the op layer: eraseDisplay/eraseLine/deleteChars/insertLines/scrollUp/scrollDown/setCursorPos/reverseIndex/index), src/terminal/PageList.zig (grow/scroll/reflow entry points), src/terminal/page.zig (cell/row management, styles, grapheme map) vs their rustty-vt counterparts.
Hunt specifically: scroll-region (DECSTBM) boundary conditions incl. left/right margins (DECSLRM), cursor pending-wrap state across ops, wide-char (spacer head/tail) handling at line ends and during erase/insert/delete, grapheme cluster attach/detach on overwrite, style ref-counting on erase vs clear, dirty-tracking parity, reflow edge cases (wrapped rows, spacer at wrap point), selection adjustment on scroll. These are where terminal emulators actually break. Walk each listed op function fully on both sides; quote both sides for every suspected drift; honest coverage statement.`,
  },
  {
    key: 'tests-quality',
    prompt: `${COMMON}
AUDIT: quality of the PORTED TEST SUITE itself — do the 4000+ Rust tests actually verify what the Zig tests verified?
1. Grep the workspace tests for: #[ignore], #[should_panic] without expected=, tests with zero assert/expect calls, tests that construct-and-drop only, commented-out assertions, .unwrap() as the only 'assertion', TODO/FIXME inside test bodies, assertions comparing a value to itself.
2. Randomly sample 25 test functions across rustty-vt/config/input/font and diff each against its named Zig original (tests usually carry the Zig test name in a comment or fn name): same assertion COUNT and same expected VALUES? Watch for expected-value transcription errors (0x07 vs 0x1b, row/col swapped, 0-based vs 1-based).
3. Check test-support fidelity: the Zig tests use std.testing.expectEqual with exact types — look for Rust ports that assert_eq! on a LOSSY conversion of the value (e.g. comparing .len() instead of contents, or a Debug-string compare that hides field drift).
4. Verify the counting basis of G1: how does gate G1 count 'ported tests' (read .port/tools/gates/) — could it double-count or count non-parity tests (zigstd infra tests) toward 3234?`,
  },
  {
    key: 'deps-hygiene',
    prompt: `${COMMON}
AUDIT: workspace/dependency/build hygiene against R3 (self-contained, same C libs) and R4 (acyclic layering).
1. cargo metadata is OK to run (read-only, no build lock): dump the workspace graph; check no crate depends on anything referencing /Users/gael/projects/ghostty (also grep all build.rs and .cargo/config* for that path); confirm zero path/git deps escaping the repo.
2. Pure-Rust substitution creep (R3 violation): list all crates.io deps of parity crates and flag any that REPLACE a C lib the reference uses (e.g. a pure-Rust png/freetype/regex instead of vendored C). Rust bindings to the same C lib are fine.
3. Feature/profile audit: workspace Cargo.toml profiles — debug-assertions, overflow-checks, panic strategy per profile; cdylib/staticlib crate-types on the C-ABI crates; does identity-ghostty feature exist and flip cleanly (grep its cfg usage)?
4. R4: run python3 .port/tools/cycle_check.py (read-only) and report; also sanity-check the layering intent: does rustty-font depend on rustty-vt or vice versa correctly (memory says font must stay BELOW vt with no vt edge)?
5. Vendored C sources: verify checked-in vendored code identity where feasible (oniguruma/wuffs/etc. version headers vs what ghostty pins in build.zig.zon).`,
  },
  {
    key: 'gates-machinery',
    prompt: `${COMMON}
AUDIT: the gate machinery itself — validate the validators.
1. Read every script in .port/tools/gates/ + ci.sh: for each gate G1-G9, determine exactly what it measures, its failure mode, and whether it can silently pass-by-construction (e.g. G4 differential in harness-control mode passing while checking NOTHING real — confirm and document what's needed to arm it; G1/G2/G3/G9 progress-mode semantics: exactly which header string flips them binding, and which currently-complete crates are therefore ungraded).
2. Self-test integrity: do the --self-test modes still flip RED on injection (read the injection logic; don't execute anything that mutates files — if self-test writes temp copies only, you may run it).
3. Look for gate-weakening drift since phase-1 (git log on .port/tools/gates/ and diff anything suspicious; R5 forbids weakening).
4. Miri/ASan runner scripts: check miri-run.sh + sanitizers.sh cover ALL pure-Rust crates (list any crate absent from both runners = uncovered by R6).
5. Report the precise minimal action list to get: (a) G4 armed for real, (b) all complete crates graded by G1/G2/G3/G9, (c) one honest full-green ci.sh run.`,
  },
]

const hunted = await parallel(auditors.map(a => () =>
  agent(a.prompt, { label: `hunt:${a.key}`, phase: 'Hunt', schema: FINDINGS })
))

const reports = hunted.filter(Boolean)
const all = reports.flatMap(r => r.findings.map(f => ({ ...f, auditor: r.auditor })))
log(`Hunt done: ${reports.length}/${auditors.length} auditors reported, ${all.length} raw findings`)

phase('Verify')
const needVerify = all.filter(f => f.severity === 'blocker' || f.severity === 'high')
const rest = all.filter(f => !(f.severity === 'blocker' || f.severity === 'high'))
log(`Adversarially verifying ${needVerify.length} blocker/high findings (${rest.length} medium/low pass through unverified but labeled)`)

const verified = await parallel(needVerify.map(f => () =>
  agent(`${COMMON}
You are an adversarial VERIFIER. Another auditor claims the following finding about the rustty port. Your job is to REFUTE it: re-read the actual code (both rustty and ghostty sides where relevant), re-run the counts, and prove it wrong, exaggerated, or already handled elsewhere (compensating code, documented deferral in DEFERRALS.md/design docs, MIRI_EXCEPTIONS.md, or intentional per PORT_RULES). Default to refuted=true if the evidence does not decisively hold. If it survives, you may correct its severity.

FINDING (from auditor ${f.auditor}):
title: ${f.title}
severity: ${f.severity}
file: ${f.file}
detail: ${f.detail}
evidence: ${f.evidence}`,
    { label: `verify:${f.title.slice(0, 40)}`, phase: 'Verify', schema: VERDICT })
    .then(v => ({ ...f, verdict: v }))
))

const confirmed = verified.filter(Boolean).filter(v => v.verdict && !v.verdict.refuted)
  .map(v => ({ ...v, severity: v.verdict.corrected_severity || v.severity }))
const refuted = verified.filter(Boolean).filter(v => v.verdict && v.verdict.refuted)
log(`Verify done: ${confirmed.length} confirmed, ${refuted.length} refuted`)

phase('Synthesize')
const summary = await agent(`You are writing the first full validation report for the rustty port (full-fidelity Zig→Rust port of Ghostty; rules in .port/PORT_RULES.md). Inputs below: per-auditor coverage statements, CONFIRMED blocker/high findings (adversarially verified), REFUTED findings (with refutation reasons — include as an appendix so they aren't re-found later), and unverified medium/low findings.

COVERAGE:
${JSON.stringify(reports.map(r => ({ auditor: r.auditor, coverage: r.coverage })), null, 1)}

CONFIRMED (verified blocker/high):
${JSON.stringify(confirmed, null, 1)}

MEDIUM/LOW (unverified, labeled):
${JSON.stringify(rest, null, 1)}

REFUTED (appendix material):
${JSON.stringify(refuted.map(r => ({ title: r.title, auditor: r.auditor, reason: r.verdict.reason })), null, 1)}

Write /Users/gael/projects/rustty/.port/phase-2/VALIDATION_REPORT.md:
1. **Verdict** — one paragraph: overall port health, the single most important risk.
2. **Findings** — every confirmed finding, ranked, each with file:line, failure scenario, fix sketch, and which rule (R1-R6) it violates.
3. **Medium/low ledger** — grouped, compact.
4. **Coverage & blind spots** — per-auditor honest coverage + what NO auditor examined (so the next sweep targets it).
5. **Refuted appendix** — one line each, so future sweeps don't re-raise them.
Do not soften findings. Then return an executive summary (<=400 words): verdict, top 5 findings, coverage gaps.`,
  { label: 'report', phase: 'Synthesize' })

return {
  summary,
  report: '.port/phase-2/VALIDATION_REPORT.md',
  stats: { auditors: reports.length, raw: all.length, verified: needVerify.length, confirmed: confirmed.length, refuted: refuted.length },
}
