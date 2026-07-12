export const meta = {
  name: 'swiftapp-abi',
  description: 'Phase A of the macOS Swift-app milestone: symbol manifest + ghostty_* alias layer + ghostty.h ABI-layout gate + rt_surface pin mirihar proof',
  whenToUse: 'Executes docs/superpowers/plans/2026-07-12-macos-swift-app-milestone.md Phase A',
  phases: [
    { title: 'Independent', detail: 'A1 manifest+gate, A3 header+layout gate, A4 pin mirihar — parallel' },
    { title: 'A2 alias', detail: 'ghostty_* alias forwarders (needs A1 manifest)' },
  ],
}

const PLAN = 'docs/superpowers/plans/2026-07-12-macos-swift-app-milestone.md'
const PRE = '.port/phase-2/PREPORT_SWIFT_APP.md'

const OUT = {
  type: 'object',
  required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' },
    status: { type: 'string', enum: ['done', 'blocked'] },
    evidence: { type: 'string', description: 'exact commands + decisive output proving each step' },
    committed: { type: 'boolean' },
    commit_ref: { type: 'string' },
    blocker: { type: 'string' },
    notes: { type: 'string' },
  },
}

const COMMON = `
REPO: /Users/gael/projects/rustty. READ /Users/gael/projects/ghostty (oracle @ 4749c4e9) but NEVER modify it. Design: ${PLAN} (read your task's section) + ${PRE} (the authoritative pre-port artifact — §0 the 90-export surface, §1 LIFETIMES rows, §2 invariants I1-I16, §3 Branch-A alias mechanics, §4 abort policy, §5 comptime, §6 constants, §7 risks R-A..R-I). rustty-apprt-c already exports all 71 rustty_* C-ABI fns.
HARD RULES: R1 fidelity (byte-identical to upstream where Branch A requires; preserve header typos R-A/R-B/R-C verbatim); R2 (C-ABI names stay ghostty_*, canonical export is rustty_*); R5 new gates register PROGRESS not HARD; any SB fix PROVEN on a mirihar harness (never hand-reasoned). Git: explicit paths, index.lock retry (sleep 3, ≤10x), no push, no reset/stash. cargo (target/) vs miri (target/miri) separate. Return via StructuredOutput; COMMIT your own files.`

phase('Independent')
const a1 = agent(`${COMMON}
TASK A1 — symbol manifest + drift gate. Execute ${PLAN} Task A1 exactly: create .port/audits/CABI_SYMBOLS.tsv (one row per the 90 GHOSTTY_API exports in /Users/gael/projects/ghostty/include/ghostty.h: ghostty_name, rustty_name, signature, source_file, darwin_only) — the 6 Darwin-only are in ${PRE} §0. Create .port/tools/gates/check_symbol_manifest.sh (every ^GHOSTTY_API name has a TSV row + a live rustty_* export; drift = RED; self-test flips RED on an injected missing row). Verify grep -c '^GHOSTTY_API' == 90 == TSV rows. Commit with the plan's A1 message.`,
  { label: 'A1:manifest', phase: 'Independent', schema: OUT })

const a3 = agent(`${COMMON}
TASK A3 — ghostty.h ABI-layout gate. Execute ${PLAN} Task A3: create crates/rustty-apprt-c/tests/abi_layout.rs with const size_of/align_of + enum-ordinal asserts for every ABI struct/union/enum in ${PRE} §1/§6 (App.Options, Surface.Options, Platform union, EnvVar, Text, String, ClipboardContent, Target/Action unions, and the pinned-ordinal enums: PlatformTag macos=1/ios=2/0=invalid, Point.Tag, ColorScheme, ClipboardRequestType, Info.BuildMode, Target.Key, Action.Key positional). Read the header's struct field types to compute expected sizes. Create .port/tools/gates/check_ghostty_h_abi.sh (diff a staged copy of include/ghostty.h vs the pinned upstream — must be byte-identical, preserve R-A/R-B/R-C typos — then run the abi_layout test). Register it PROGRESS in ci.sh. Run cargo test -p rustty-apprt-c --test abi_layout; if a REAL layout mismatch surfaces, that's an R1 bug — fix the #[repr(C)] struct to match the header (report what you fixed). Commit with the plan's A3 message.`,
  { label: 'A3:abi-gate', phase: 'Independent', schema: OUT })

const a4 = agent(`${COMMON}
TASK A4 — prove the rt_surface pin (invariant I2, risk R-E; the sharpest UB trap). Execute ${PLAN} Task A4: FIRST read how ghostty_surface_new in crates/rustty-apprt-c currently allocates the embedded Surface and whether core_surface.init writes rt_surface = &container (self-referential). Build an isolated mirihar harness at .port/phase-2/mirihar-surface-pin/ (standalone cargo project, model on .port/phase-2/mirihar-chunk3b): a struct whose inner field stores a back-pointer to its container, Box it, assert the back-pointer stays valid across use; add a NEGATIVE candidate that MOVES the box after init and confirm Miri flags it UB. cargo +nightly miri run each candidate: negative = UB, shipped (boxed+no-move) = CLEAN under both stacked-borrows and -Zmiri-tree-borrows. If the current apprt-c code can move the Surface post-init, fix it (Pin<Box> or a documented no-move invariant with a note) and re-prove. Commit the harness (+ any fix) with the plan's A4 message. If the fix needs a larger refactor, keep it status=blocked with the precise harness finding (don't force it).`,
  { label: 'A4:pin-mirihar', phase: 'Independent', schema: OUT })

const [r1, r3, r4] = await parallel([() => a1, () => a3, () => a4])
log(`Independent: A1 ${r1 ? r1.status : 'fail'}, A3 ${r3 ? r3.status : 'fail'}, A4 ${r4 ? r4.status : 'fail'}`)

phase('A2 alias')
let a2 = null
if (r1 && r1.status === 'done') {
  a2 = await agent(`${COMMON}
TASK A2 — ghostty_* alias layer (Branch A). Execute ${PLAN} Task A2 using the manifest A1 just committed (.port/audits/CABI_SYMBOLS.tsv): create crates/rustty-apprt-c/src/ghostty_alias.rs with one thin forwarder per app export — #[no_mangle] pub unsafe extern "C" fn ghostty_<name>(<exact ported args>) -> <Ret> { crate::<rustty_name>(<args>) } — copying each signature from the live rustty_* decl; Darwin-only ones (${PRE} §0) gated #[cfg(any(target_os="macos", target_os="ios"))]. Add an "identity-ghostty" feature to Cargo.toml and #[cfg(feature = "identity-ghostty")] mod ghostty_alias; in lib.rs. Verify: cargo build -p rustty-apprt-c --features identity-ghostty green; nm -gU target/debug/librustty_apprt_c.a | grep -c '_ghostty_' equals the expected export count; run check_symbol_manifest.sh GREEN against the built lib. Commit with the plan's A2 message.`,
    { label: 'A2:alias', phase: 'A2 alias', schema: OUT })
  log(`A2 ${a2 ? a2.status : 'fail'}`)
} else {
  log('A2 SKIPPED — A1 manifest not done')
}

return {
  a1: r1 ? r1.status : 'fail', a3: r3 ? r3.status : 'fail', a4: r4 ? r4.status : 'fail', a2: a2 ? a2.status : 'skipped',
  summary: `A1 ${r1 ? r1.status : 'fail'}, A3 ${r3 ? r3.status : 'fail'}, A4 ${r4 ? r4.status : 'fail'}, A2 ${a2 ? a2.status : 'skipped'}${a2 && a2.commit_ref ? ' (' + a2.commit_ref + ')' : ''}`,
}
