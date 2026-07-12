export const meta = {
  name: 'xev-detectors',
  description: 'Source-editing: fix the xev MPSC latent SB UB (mirihar-proven) + implement the D1-D9 semantic-mismatch detectors (D7 already done) and wire them into ci.sh',
  whenToUse: 'After the vt/vt-c Miri close; runs concurrently with the read-only audit workflow',
  phases: [
    { title: 'xev-fix', detail: 'reproduce the dead-branch MPSC UB on a mirihar harness, fix, prove, un-defer or keep-filed honestly' },
    { title: 'Detectors', detail: 'D1-D6,D8,D9 each design+implement+self-test as its own disjoint gate script/tests' },
    { title: 'Land', detail: 'single agent wires all detectors into ci.sh + commits' },
  ],
}

const OUT = {
  type: 'object',
  required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' },
    status: { type: 'string', enum: ['done', 'blocked', 'filed'] },
    evidence: { type: 'string' },
    committed: { type: 'boolean' },
    commit_ref: { type: 'string' },
    ci_sh_line: { type: 'string', description: 'the exact registry line to add to ci.sh for this detector (or empty)' },
    new_files: { type: 'array', items: { type: 'string' } },
    notes: { type: 'string' },
    blocker: { type: 'string' },
  },
}

const COMMON = `
REPO: /Users/gael/projects/rustty. READ /Users/gael/projects/ghostty (oracle @ 4749c4e9) but NEVER modify it. Rules .port/PORT_RULES.md — R1 fidelity; R5 never weaken a gate/test (blocked > bent); R6 Miri pure-Rust / ASan FFI; every SB/TB fix PROVEN on an isolated mirihar harness (.port/phase-2/mirihar-*), never hand-reasoned. Detector specs: BUN_PORT_LEARNINGS.md §4 (D1-D9) — read your detector's paragraph. A CONCURRENT read-only audit workflow is running; it does not edit — but expect the git index busy occasionally (index.lock: sleep 3, retry <=10x). Git: explicit paths only, no push, no reset/stash. cargo (target/) and miri (target/miri) use separate dirs. Return via StructuredOutput.`

phase('xev-fix')
const xev = await agent(`${COMMON}
TASK: fix the xev MPSC latent SB UB (LIFETIMES-audit findings M1+M2, filed in .port/phase-2/DEFERRALS.md "LIFETIMES-AUDIT findings (2026-07-10)"; full detail in .port/audits/LIFETIMES_AUDIT_2026-07-10.md).
THE BUG: crates/rustty-xev/src/kqueue.rs — the MPSC completion queue (queue_mpsc.rs, self-referential: head/tail point at the embedded stub, methods take *mut Self per its PORT NOTE lines 14-17) is fed by callers that form &mut self.thread_pool_completions coerced to *mut Self (kqueue.rs:630 init, :740 pop, :1394 thread_perform). Each &mut retag invalidates the stub self-pointers written at init -> SB UB (M2: even single-threaded; M1: also cross-thread). Zig reference is clean (*Self method calls). CRITICAL: this branch is DEAD (thread_pool always None — Loop::init hardcodes Options::default() at kqueue.rs:321; ThreadPool::schedule is unimplemented!()), so NO existing test reaches it and a normal Miri run won't flag it.
THEREFORE you MUST build a mirihar harness that SYNTHESIZES the init->pop cycle (and the thread_perform cross-thread path) to reproduce the UB under Miri FIRST, then fix, then re-prove clean. Model on .port/phase-2/mirihar-chunk3b (a standalone cargo project run with cargo +nightly miri run). The audit warns the one-line addr_of_mut! fix is NOT trivially provable — the stored stub pointers carry the init-tick provenance across later tick(&mut self) retags, so the harness must settle whether the tick receiver needs a raw seam or head/tail need UnsafeCell-class fields. Iterate on the harness until the fix is Miri-clean under BOTH stacked-borrows and -Zmiri-tree-borrows.
Then apply the proven fix to kqueue.rs (and queue_mpsc.rs if the harness shows the fields need it), preserving R1 (the Zig *Self semantics). Run cargo build -p rustty-xev + cargo test -p rustty-xev + the tractable cargo +nightly miri test -p rustty-xev to confirm no regression. Update LIFETIMES_AUDIT + DEFERRALS + the TSV rows to RESOLVED, and un-defer the §6 ThreadPool-body gate note if applicable.
IF after genuine effort the fix cannot be proven clean without a larger redesign (e.g. it needs the ThreadPool body ported first): do NOT force a wrong fix (R5). Keep it FILED, but UPGRADE the DEFERRALS entry with exactly what the harness established (the minimal provable fix shape + why it's blocked), and commit the mirihar harness itself as the reproduction. status="filed" in that case.
Commit your files (mirihar harness, kqueue.rs/queue_mpsc.rs if fixed, audit/DEFERRALS/TSV updates) with an accurate message. Report status done|filed|blocked with the harness evidence (paste the Miri UB repro + the post-fix clean result).`,
  { label: 'xev-mpsc-ub', phase: 'xev-fix', schema: OUT })
log(`xev: ${xev ? xev.status : 'failed'} ${xev && xev.commit_ref ? '(' + xev.commit_ref + ')' : ''}`)

phase('Detectors')
const detectors = [
  { key: 'D1', name: 'slice-reinterpret audit', spec: `D1 (BUN_PORT_LEARNINGS §4). Create .port/tools/gates/check_slice_reinterpret.sh + .port/audits/slice-reinterpret.tsv. Grep the workspace for slice reinterprets (bytemuck::cast_slice, align_to, from_raw_parts.*as *(const|mut) u(16|32|64), bytes_of, cast_slice_mut). Build the allowlist: every hit gets a row (file:line, decision truncate|error|exact-len-proven, the Zig origin line it ports, and — for truncate/error — the name of its odd/misaligned-length unit test). The gate FAILS on any unlisted hit. Where a truncate/error site lacks an odd-length test, WRITE that test (in the owning crate, a new #[test]) — kills Bun class 1 (#31188). Self-test: an injected new reinterpret with no allowlist row must make the gate RED.` },
  { key: 'D2', name: 'capacity-saturation tests', spec: `D2 (§4). One-time inventory of every fixed [T; N] + monotonic-counter (write-then-increment) pair in rustty-vt hot code (CSI param buffers, page capacities, kitty quotas). For each, derive N from the same expression the Zig counter-range implies (IntFittingRange(0,max+1) => [T; MAX+1]) and confirm the port matches; write a #[test] <name>_capacity_saturation that drives every bounded store to its ceiling. Put these in a NEW disjoint module/file to avoid racing other agents (e.g. crates/rustty-vt/src/capacity_saturation_tests.rs as a #[cfg(test)] mod, or tests/capacity_saturation.rs). Gate = an inventory file .port/audits/capacity-saturation.tsv where each [T;N]+counter pair names its test. Kills Bun class 2a (#31503). Triage note in the doc: a Rust panic where Zig "worked" => re-run oracle under ReleaseSafe first (R3).` },
  { key: 'D3', name: 'constants-by-expression parity', spec: `D3 (§4). Create .port/tools/gates/check_constants_parity.py: (a) grep gate — zero TODO(port)|FIXME(port)|placeholder markers survive per crate at its sub-batch gate; (b) extract numeric consts from the Zig reference files (phase-0 inventory has paths) and diff vs the Rust twin — derived constants MUST appear as the same const EXPRESSION, not a folded literal (kills Bun class 2b, the 64-vs-count/4 shrink). Self-test: inject a folded-literal divergence -> RED.` },
  { key: 'D4', name: 'comptime-phase + golden VT bytes', spec: `D4 (§4) — the VERY HIGH likelihood class for this codebase (Ghostty comptime-builds CSI/SGR responses; data domain IS escape sequences; killed Bun class 3 #30693). (a) Write an ST-integrity assertion helper + golden byte tests: for ALL emitted escape/format output in rustty-vt (device_status/device_attributes/kitty responses/OSC replies/DCS), byte-exact expected strings + assert every OSC/DCS/APC opener is followed by a correct terminator (ST = ESC backslash or BEL), no stray bytes. Put in a NEW disjoint test module. (b) Checklist item (write .port/tools/gates/check_comptime_phase.md): every Zig comptime string-processing param must map to a Rust const/generic/macro; a runtime-fn port is a flagged deviation needing a written grammar-collision argument. Run your new golden tests to confirm they pass (native).` },
  { key: 'D5', name: 'paired-invariant catalog', spec: `D5 (§4, kills Bun class 4 #30678). Create .port/phase-2/VT_INVARIANTS.md seeded with the 4 known cross-site pairs: dirty-tracking<->renderer damage; style-id refcounts Page<->style set; kitty placements<->renderer atlas; mode state<->key encoder. For each: the predicate, its producer site (add a debug_assert there if missing — but only if it's a faithful no-op addition, else just document), its consumer site (graceful-degrade), and ONE integration test through the seam (write it in a disjoint new test module if a real seam exists in-crate; else note it needs cross-crate wiring). Gate = a review-checklist rule that the catalog is updated when a chunk touches either end.` },
  { key: 'D6', name: 'sanitizer canary + cfg-mapping', spec: `D6 (§4, kills Bun class 5 #30875). (a) Sanitizer canary tests compiled only in sanitizer builds: one deliberately leaks, one heap-overflows; the ASan/LSan lane must FAIL on them (proving the gate can SEE the port's allocations), then they're filtered from the roll-up. Add to sanitizers.sh / the ASan harness + a canary test file. (b) Grep audit (in check_constants_parity.py or a new check): every Zig builtin.mode/build-option/comptime-switch maps to a cfg, never constant-folded. (c) suppression-list entries need a per-entry WHY. Self-test: the canary must make the sanitizer lane RED when the gate is armed.` },
  { key: 'D8', name: 'clone-injection audit', spec: `D8 (§4, HN selfmodruntime). Create .port/tools/gates/check_clone_injection.sh: per-diff grep of NEW .clone()/.to_vec()/.to_owned()/.into_owned() vs the Zig side's copy behavior; unexplained copies flagged. Since it's diff-based, implement it to run against a git range (e.g. HEAD~N..HEAD or a staged diff) and emit a review warning list; wire a baseline against the phase-0 benchmark corpora as the quantitative backstop note. Not a hard gate (advisory), but committed + documented.` },
  { key: 'D9', name: 'stale-comment contract check', spec: `D9 (§4). Create .port/tools/gates/check_stale_comments.sh (or extend the G3b comment gate): wherever a Zig builtin was replaced by a Rust primitive with different failure semantics (reinterpretSlice->cast_slice, catch unreachable->expect, wrapping/saturating ops, &mut->raw), the adjacent ported comment must state the RUST primitive's contract; any "len/align is guaranteed X" comment near a panicking cast needs a test asserting X. Implement as a grep heuristic that flags candidate stale comments for review + a checklist rule. Spot-run it and report the candidate count.` },
]

const results = await parallel(detectors.map(d => () =>
  agent(`${COMMON}
TASK: implement detector ${d.key} — ${d.name}. ${d.spec}
Create ONLY your own new files (disjoint from other detector agents — do NOT edit ci.sh, another detector's files, or shared source files; if you must add tests, put them in a NEW module/file named for your detector). Self-test/run your gate or tests to prove they work (report the output). COMMIT your own new files (explicit paths, index.lock retry). Return ci_sh_line = the exact registry line ci.sh should add to run your gate (read how ci.sh registers gates first — the "Gxx|CLASS|script" format), or empty if advisory-only. new_files = the paths you created.`,
    { label: `detector:${d.key}`, phase: 'Detectors', schema: OUT })))

const good = results.filter(Boolean)
log(`Detectors: ${good.filter(r => r.status === 'done').length}/${detectors.length} done; ci lines: ${good.filter(r => r.ci_sh_line).length}`)

phase('Land')
const land = await agent(`${COMMON}
TASK: wire the completed detectors into .port/tools/ci.sh and finalize. The detector agents each committed their own new files and returned a ci_sh_line. Add each non-empty ci_sh_line to ci.sh's gate registry in the correct spot (read ci.sh's registry format + how it classifies HARD vs PROGRESS vs advisory — new detectors that could false-positive on existing code should register as PROGRESS/advisory, NOT HARD, until proven clean, to honor R5's "never make CI un-passable"). Update .port/tools/CI.md with the new gate rows. Then run the newly-wired gates once (bash the individual scripts, NOT a full ci.sh which would trigger the 5h Miri lane) to confirm they don't crash and report sensibly. Commit ci.sh + CI.md ("gates: wire D1-D9 semantic-mismatch detectors into ci.sh (advisory/progress) — <list>").
DETECTOR RESULTS: ${JSON.stringify(good.map(r => ({ task: r.task, status: r.status, ci_sh_line: r.ci_sh_line, new_files: r.new_files })), null, 1)}
Report which detectors landed, which are advisory vs progress, and any that couldn't be wired (with reason).`,
  { label: 'land-detectors', phase: 'Land', schema: OUT })

return { xev: xev ? { status: xev.status, ref: xev.commit_ref } : null,
  detectors: good.map(r => ({ d: r.task, status: r.status })),
  land: land ? land.status : 'failed',
  summary: `xev ${xev ? xev.status : 'fail'}; detectors ${good.filter(r => r.status === 'done').length}/${detectors.length} done; land ${land ? land.status : 'fail'}` }
