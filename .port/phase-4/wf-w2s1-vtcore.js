export const meta = {
  name: 'p4-w2s1-vtcore',
  description: 'Phase-4 W2 stage 1: stream-continuation bundle + PageList Builder/allocatePage (parallel, disjoint files)',
  phases: [{ title: 'Port', detail: 'worktree-isolated, patch harvest' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
You are a Phase-4 vt-core porter for the rustty port (full-fidelity Zig->Rust port of ghostty).
You run in an ISOLATED git worktree of ${REPO} at current main HEAD with its own build dir.

Oracle (read-only, AT the target pin 12967b68f): ${REF}. Working-tree files ARE the target end
state. Intermediate states: git -C ${REF} show <hash>:<path>. Range: 6ad1fe7d8..12967b68f.

READ FIRST:
- ${REPO}/.port/phase-4/assessments/A-terminal-core.md — YOUR cluster rows are the spec
  (per-commit verdicts, evidence, test lists). Follow them.
- ${REPO}/docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md (Global Constraints).
- ${REPO}/.port/PORT_RULES.md.

BINDING:
- R1: every line, comment, and test{} assertion-for-assertion. Port the RANGE-DIFF END STATE,
  never intermediates; carry every superseded commit's regression tests against the end state.
- R2 renames per token. R7 arithmetic (saturating/checked + debug_assert!). R9 SAFETY tags.
- SB/UB: raw-pointer args for Page/Row/Cell/Screen/PageList re-read seams (memory
  rust-mut-args-strongly-protected). Long-lived raw alias interleaved with &mut = prove on an
  isolated Miri harness, never hand-reason (memory shaper-hook-sb-clean-interleave).
- Match the surrounding rustty idiom exactly (look at neighboring ported functions first).
- cargo test -p rustty-vt --lib takes ~90s; run targeted filters while iterating, and ONE full
  lane at the end (must be 0 failed).

HARVEST when done, from the worktree repo root:
  git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch
Do NOT git commit. Confirm the patch is non-empty. Report per the schema, including the full
list of upstream tests you carried (name-by-name) so the orchestrator can audit against G1b.
`

const SCHEMA = {
  type: 'object',
  required: ['patch_path', 'summary', 'test_evidence', 'tests_carried', 'caveats'],
  properties: {
    patch_path: { type: 'string' },
    summary: { type: 'string' },
    files_created: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    tests_carried: { type: 'array', items: { type: 'string' }, description: 'upstream test names ported, one per test' },
    test_evidence: { type: 'string' },
    caveats: { type: 'string' },
  },
}

phase('Port')
const results = await parallel([
  () => agent(COMMON + `
YOUR TASK (patch name: w2-stream): assessment A cluster **A-3 — stream continuation /
ground-state / APC exit** (commits 68beeeb3f, 70e41e96d, a69a591af, d7bb4b863, afb351f83,
b53728241; ~2,000 lines; 26 upstream tests; this is the G1b module 'stream' drop=8 owner).

Scope, per the assessment rows (re-read them):
- NEW ${REF}/src/terminal/stream_continuation.zig (606 lines) -> crates/rustty-vt/src/stream_continuation.rs
  with ALL its tests (8), pub mod wiring, and the Stream.init options rework it implies.
- The ground-state/continuation rework in ${REF}/src/terminal/stream.zig (read the end state;
  the diff for those commits) -> crates/rustty-vt/src/stream.rs. ALL 16 new stream.zig tests.
- b53728241: apc_end now carries ApcEnd{terminated} — port the signature change through
  crates/rustty-vt/src/stream.rs AND the apc handler (crates/rustty-vt/src/apc/mod.rs or
  equivalent — the apc half is IN SCOPE here per the plan; its 4 new tests are G1b apc drop=4)
  AND stream_terminal.rs's arm (the .unknown/UnknownSequence action stub may reference an
  enum variant that termio consumes later — add the vt-side end state only).
  Also the 2 apc Glossary tests the assessment names.
- The inspector's 68beeeb3f init-API slice: crates/rustty-inspector (grep for the Stream init
  it constructs) must keep compiling — update its call site to the new init options in the SAME
  patch (assessment F cluster 'inspector' names this lockstep break).
- DO NOT touch pagelist/ or page*.rs (a sibling agent owns those files this stage).
Cross-check when done: every upstream test name from the 6 commits appears in your
tests_carried list. Full lane green.`,
    { label: 'w2-stream', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),

  () => agent(COMMON + `
YOUR TASK (patch name: w2-builder): assessment A cluster **A-1 — PageList Builder/allocatePage**
(commits fc1bd06a1, 35db32078, 457c5a0a6 END STATE; ~660 lines; 7 upstream tests) plus the two
snapshot-enabler riders the assessment couples to it: b61fd5fbb (RefCountedSet iterator over
raw [*]Item — SAFETY tag + narrow scope) and d44baa914 (main.zig re-exports -> the crate's
lib.rs pub use surface).

Scope, per the assessment rows (re-read them):
- ${REF}/src/terminal/PageList.zig end state: the Builder type, allocatePage (renamed from
  addPage), cancel->deinit rename, and their call-site updates INSIDE pagelist/*.rs only.
- PageAllocation holds *mut PageList — raw-ptr shape + LIFETIMES-style SAFETY comments.
- ${REF}/src/terminal/ref_counted_set.zig iterator -> crates/rustty-vt/src/ref_counted_set.rs.
- All 7 Builder tests + the iterator tests, assertion-for-assertion.
- DO NOT touch stream.rs / stream_continuation / apc / inspector (a sibling agent owns those).
  You own crates/rustty-vt/src/pagelist/**, ref_counted_set.rs, page*.rs if needed, and the vt
  lib.rs re-export lines for YOUR types only (append-only, single line each, to minimize patch
  overlap — the orchestrator resolves lib.rs overlap if any).
Full lane green.`,
    { label: 'w2-builder', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),
])

return { tasks: results.filter(Boolean) }
