export const meta = {
  name: 'p4-par5-snapshot3-sw',
  description: 'Phase-4 final porting stage: snapshot screen/terminal+drivers -> C-API bundle S; wasm bundle W; benches',
  phases: [{ title: 'Port', detail: '(wsnap-3 -> w5-S pipeline) ∥ w5-W ∥ wbench' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
You are a Phase-4 porter for the rustty port (full-fidelity Zig->Rust port of ghostty).
You run in an ISOLATED git worktree of ${REPO}; FIRST ACTION: git log --oneline -1 — if not
7a93446e or newer, run: git reset --hard main. Own build dir; edit freely.
Oracle (read-only, AT target pin 12967b68f): ${REF}; working-tree files ARE the end state.
Range: 6ad1fe7d8..12967b68f.

READ FIRST: the relevant assessment in ${REPO}/.port/phase-4/assessments/ (B-snapshot-io.md /
C-capi-lib-wasm.md); the plan docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md;
.port/PORT_RULES.md. ALREADY LANDED (git log --oneline -14; build on it): everything through
par-4 — snapshot grid/page/history live + codec.rs documenting the deferred drivers, C-API
bundles T/R/M/K/E, kitty/render/mode C surfaces, embed header @ pin.

BINDING: R1 end-state fidelity (every line/comment/test); R2 renames; R7 arithmetic; R9
SAFETY; DG5; closed-enum c_int narrowing; SB/UB raw-ptr discipline (decoder objects aliasing
Terminal/Screen across C calls = the named UB shape; Miri-prove). Match surrounding idiom.

VERIFY: cargo build + relevant test lanes green (vt lane ~90s); gates you touch never weakened.

HARVEST from the worktree repo root:
  git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch
Do NOT git commit. Confirm non-empty. Report per schema; every upstream test carried by name.
`

const SCHEMA = {
  type: 'object',
  required: ['patch_path', 'summary', 'test_evidence', 'tests_carried', 'caveats'],
  properties: {
    patch_path: { type: 'string' },
    summary: { type: 'string' },
    files_created: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    tests_carried: { type: 'array', items: { type: 'string' } },
    test_evidence: { type: 'string' },
    caveats: { type: 'string' },
  },
}

phase('Port')
const results = await parallel([
  // Lane 1: snapshot completion, then the snapshot/io C API on top of it
  () => agent(COMMON + `
YOUR TASK (PATCHNAME wsnap-3): COMPLETE the snapshot subsystem — assessment B cluster C4's
screen/terminal halves + the codec drivers. Port the END STATE of
${REF}/src/terminal/snapshot/screen.zig (2,460 L), terminal.zig (1,840 L), and snapshot.zig's
drivers (EncodeError/DecodeError/DecodeExactError, encode, decode, decodeExact, Decoded,
Decoder=init/ready/next — crates/rustty-vt/src/snapshot/codec.rs documents exactly this
deferred surface incl. the deferred upstream test list; make it ALL live), plus the ONE
deferred history test ('HISTORY encodes newest first and restores complete history' —
history.rs PORT STATUS names it), 9be6c2ea2 retain-continuations decode option, b867a0f59's
snapshot-screen half, and the kitty 1-line hunks if any remain. decode-into-Screen/Terminal
follows the transactional release-on-error precedent in page.rs; raw-ptr discipline; run the
full snapshot Miri lane at the end (MIRIFLAGS=-Zmiri-disable-isolation cargo +nightly miri
test -p rustty-vt --lib snapshot -- --test-threads=1) — zero exceptions expected, per the
wsnap-2 precedent. Every upstream test{} in those files assertion-for-assertion. Full vt lane
green. Report your WORKTREE path for the chained S agent.`,
    { label: 'wsnap-3', phase: 'Port', schema: SCHEMA, isolation: 'worktree' })
    .then(async s => {
      if (!s) return [null, null]
      const bundleS = await agent(COMMON + `
YOUR TASK (PATCHNAME w5-S): assessment C **Bundle S — snapshot/io/streaming C API**, on TOP
of the completed snapshot subsystem. FIRST: git apply --3way ${s.patch_path} (built on the
same base; resolve trivially if needed). Then port: include/ghostty/vt/snapshot.h (525 L) ->
include/rustty/vt/snapshot.h + io.h (107 L) -> include/rustty/vt/io.h;
${REF}/src/terminal/c/snapshot.zig (1,533 L) -> crates/rustty-vt-c/src/snapshot.rs +
c/io.zig -> io.rs; the 15 new exports + rustty_terminal_continuation_* +
rustty_vt_write_until_ground + rustty_formatter_format streaming writer (846d24e12 buffered
writer adapter + 924c8a90d streaming C API — re-read .port/phase-4/NON_PORTS.md's TinyIo row
FIRST and either extend its argument with your Rust-std shape, or port the needed subset;
record the decision on the row); example/c-vt-snapshot (build.zig-based example -> the
rustty example/ convention — look at example/ neighbors for the cc-based build.sh pattern);
types registry rows Bundle T deferred to S (REGISTRY_RESIDUALS names them with owner S —
land them and CLEAR the residual rows, the stale-allowlist guard requires it); a69a591af's 4
vt_write_until_ground tests. SB: the C Decoder object holds a raw Terminal alias across
calls — raw-ptr + SAFETY contract, Miri on the new vt-c seam tests. After: bash
.port/tools/gates/check_rustty_vt_headers.sh — arm (d) must LOSE all its snapshot/io/
continuation/formatter rows (only wasm_* may remain, owned by the sibling W agent; report
the exact residual list). vt-c + vt lanes green.`,
        { label: 'w5-S', phase: 'Port', schema: SCHEMA, isolation: 'worktree' })
      return [s, bundleS]
    }),

  // Lane 2: wasm allocator bundle W
  () => agent(COMMON + `
YOUR TASK (PATCHNAME w5-W): assessment C **Bundle W — the wasm allocator break** (ATOMIC) +
the deferred wasm arms: 492c26067 custom memory pool (rustty-datastruct wasm_page_pool
landed in W1 — wire it: src/lib/allocator/wasm.zig -> the rustty wasm allocator seam),
a8e9b413f simplified alloc API + DELETE crates/rustty-lib/src/allocator/convenience.rs (or
wherever convenience.zig's port lives — grep; upstream deleted it) + REMOVE the 12 stale
per-type wasm exports and ADD the new alloc API exports per
${REF}/src/lib_vt.zig + wasm.h at the pin; REWRITE the 3 wasm examples
(example/{wasm-vt,wasm-sgr,wasm-key-encode}/index.html call the old names — verified in the
assessment); port test/wasm-alloc.mjs -> the rustty test location (check
.port/tools/gates/ws14_webgl_browser_gate.sh for how wasm artifacts are exercised and keep
that gate honest — run it if runnable); 5364e5158 wasm stack reservation + d61920d80 wasm
release log gating (R8 cfg-switchable); 87f69a12e + 3d9b2b483 wasm hash/Style.eql arms (the
PORT NOTE anchor at hash_map.rs AutoContext from par-3 names them — land them WITH the pool
wiring, delete the anchor note); sweep the rustty-wasm/web-canvas crates for the renamed
alloc API. Build the wasm target: cargo build -p <the wasm cdylib crate> --target
wasm32-unknown-unknown (find the crate ci.sh's wasm lane builds; restore_zig_deps not
needed). A5 arm (d)'s wasm_* rows must clear; run the gate and report the residual. All
touched native lanes green too.`,
    { label: 'w5-W', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),

  // Lane 3: benches (C8) + verify-kaitai wiring
  () => agent(COMMON + `
YOUR TASK (PATCHNAME wbench): assessment B cluster C8 + the bench-gate parity RED. Port
${REF}/src/benchmark/{TerminalSnapshot,TerminalFormatter,TerminalResize}.zig ->
crates/rustty-bench/src/{terminal_snapshot,terminal_formatter,terminal_resize}.rs with cli
Action registrations (the G-BENCH gate check_bench_tests.sh is currently RED missing exactly
these three actions — it must go GREEN; run it before and after and report both), plus the
synthetic Styled generator if the benches need it and it is absent (check
crates/rustty-synthetic or the synthetic module location; assessment B lists
src/synthetic/{Kitty,cli/Kitty,Styled}). Snapshot drivers may still be in-flight in a
sibling agent's worktree — if TerminalSnapshot needs snapshot::encode/decode that is not on
main yet, write the bench against the API DOCUMENTED in
crates/rustty-vt/src/snapshot/codec.rs (the oracle's exact driver signatures) and mark the
bench module cfg-gated with a PORT NOTE naming wsnap-3 — the orchestrator will un-gate on
merge; if the drivers ARE on main by the time you start (check first), wire them live.
Also wire src/terminal/c/types-schema-verify.py's kaitai sibling: verify-kaitai.py
(cluster C5) — port it beside the .ksy in crates/rustty-vt/src/snapshot/ and add a ci.sh
PROGRESS row IF python3+kaitai are available (probe; if kaitai is unavailable, the gate must
exit 3 BLOCKED, never silent-green — follow the check_test_counts.py oracle-absent
precedent). cargo test -p rustty-bench green; report the G-BENCH before/after lines.`,
    { label: 'wbench', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),
])

const flat = []
for (const r of results) {
  if (Array.isArray(r)) flat.push(...r.filter(Boolean))
  else if (r) flat.push(r)
}
return { tasks: flat }
