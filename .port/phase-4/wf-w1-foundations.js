export const meta = {
  name: 'p4-w1-foundations',
  description: 'Phase-4 W1: leaf foundations (lib enum/TinyIo/crc32c, datastruct allocators, fraction/fastprint, os, simd)',
  phases: [{ title: 'Port', detail: 'worktree-isolated: port new modules + tests, verify GREEN, emit a git patch' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
You are a Phase-4 foundations porter for the rustty port (full-fidelity Zig->Rust port of ghostty).
You run in an ISOLATED git worktree — a private copy of ${REPO} at current main HEAD, with its own
build dir. Edit freely and run cargo here; you do NOT affect other agents or the main tree.

Oracle (read-only, AT the target pin): ${REF}. Working-tree files ARE the target end state; read
them directly, and read intermediate states with git -C ${REF} show <hash>:<path>.

BINDING (${REPO}/.port/PORT_RULES.md, and the plan
${REPO}/docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md):
- R1 fidelity: port EVERY line, comment (doc/inline/license/ASCII-diagram/spec-citation), and
  test{} block assertion-for-assertion. Comptime artifacts bit-for-bit.
- R2 renames per token (ghostty->rustty, Ghostty->Rustty), incl. in comments and test strings.
- R4 acyclicity: the crate graph MUST stay acyclic; run bash .port/tools/gates/check_acyclic.sh
  before finishing. Place new modules so no new cross-crate cycle forms.
- R7 arithmetic: saturating_*/checked_*/unsigned_abs + debug_assert!, never bare ops for Zig
  +|/-|/@abs/std.math.add-catch.
- R9 unsafe: every unsafe block carries // SAFETY(<origin>/<dest>): <contract>; density
  non-increasing; new externs get one safe wrapper.
- SB/UB: raw-pointer args for Page/Row/Cell/Screen re-read across mutation (memory
  rust-mut-args-strongly-protected). Where a raw alias interleaves with &mut to the same field,
  PROVE it on an isolated non-FFI Miri harness — do not hand-reason.
- Match the idiom/naming of the surrounding rustty code and neighboring ported modules.

METHOD:
1. Read the upstream source (the .zig files for your task) IN FULL and any cited commits.
2. Port to idiomatic parity-preserving Rust: create the new module file(s), wire them into the
   crate (lib.rs / mod.rs pub mod), port EVERY test{} block as #[test] (or a #[cfg(test)] mod).
3. Build: cargo build -p <crate>. Test: cargo test -p <crate> --lib (your new tests + no
   regressions). For pure-Rust crates, note Miri-readiness (do not necessarily run full Miri —
   it is slow — but the code must be Miri-clean by construction).
4. Run bash .port/tools/gates/check_acyclic.sh — must pass.
5. HARVEST: from your worktree repo root run exactly:
     git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch
   (PATCHNAME is given in your task). Do NOT git commit. Confirm the patch file exists and is
   non-empty (wc -l).

Report: the patch path, the crate(s), what you ported (files created/modified, test count),
the build+test evidence (paste the final "test result:" line), the acyclic-gate result, and any
caveat (a hunk you could not port, a dependency on another wave, a Miri concern).
`

const SCHEMA = {
  type: 'object',
  required: ['patch_path', 'crates', 'summary', 'test_evidence', 'acyclic_ok'],
  properties: {
    patch_path: { type: 'string' },
    crates: { type: 'array', items: { type: 'string' } },
    summary: { type: 'string' },
    files_created: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    test_count: { type: 'integer' },
    test_evidence: { type: 'string' },
    acyclic_ok: { type: 'boolean' },
    caveats: { type: 'string' },
  },
}

const TASKS = [
  {
    label: 'lib',
    patch: 'w1-lib',
    prompt: `Port THREE things into crate rustty-lib (all leaf, all in ${REPO}/crates/rustty-lib):
(1) e77c2612e — enum STABLE VALUES. Read git -C ${REF} show e77c2612e. rustty-lib/src/enum.rs
still has the OLD value semantics (the assessment: "enum.rs:98 still has old value semantics").
Port the stable-value end state + its tests. This is the precondition for the later C bundle-T
type-metadata work, so get the value assignment exactly right.
(2) src/lib/TinyIo.zig (1,522 lines) -> rustty-lib/src/tiny_io.rs (or the file name matching the
crate's convention). Read ${REF}/src/lib/TinyIo.zig in full. It is a small std.Io-shim used by the
streaming C APIs; port every type, method, comment, and test. Add pub use in lib.rs. Also port the
one-line module export from 82df79ec8 if it names TinyIo.
(3) src/crc32c.zig -> the CRC32C codec (slicing-by-16 software CRC + the interleaved-stream end
state of 36d8e3f77 + eb09bf829). Read ${REF}/src/crc32c.zig. Place it where it is a leaf reachable
by rustty-vt (snapshot) without a cycle — rustty-lib is fine (or rustty-util if that is the better
FIRM leaf; check check_acyclic.sh either way). Port the KAT/differential tests bit-exactly.
Keep the three independent; if any single one is genuinely blocked, port the other two and say so.`,
  },
  {
    label: 'datastruct',
    patch: 'w1-datastruct',
    prompt: `Crate rustty-datastruct (${REPO}/crates/rustty-datastruct):
(1) NEW src/datastruct/limited_allocator.zig (115 lines) -> rustty-datastruct/src/limited_allocator.rs.
Read ${REF}/src/datastruct/limited_allocator.zig. It caps total bytes an inner allocator may hand
out; port with every test. This unblocks kitty png caps + apc glyf caps (later waves).
(2) NEW src/datastruct/wasm_page_pool.zig -> rustty-datastruct/src/wasm_page_pool.rs. Read
${REF}/src/datastruct/wasm_page_pool.zig. It feeds PageList raw-ptr pages — apply R9 SAFETY tags
and note the LIFETIMES concern (the consumer wiring in rustty-vt is a later wave; here just port
the pool + tests standalone).
(3) DELETE src/lru.rs — upstream deleted src/datastruct/lru.zig and the assessment verified
rustty-datastruct/src/lru.rs has ZERO real callers (every "lru" hit elsewhere is z2d FillRule or a
link flag). Remove the file and its pub mod line. Confirm cargo build -p rustty-datastruct still
builds after removal.
Wire (1),(2) into lib.rs (pub mod). One agent owns datastruct/lib.rs so there is no race.`,
  },
  {
    label: 'vt-newfiles',
    patch: 'w1-vt-newfiles',
    prompt: `Crate rustty-vt (${REPO}/crates/rustty-vt) — TWO new leaf files (do NOT touch existing
vt modules beyond adding the two pub mod lines):
(1) src/terminal/fraction.zig -> rustty-vt/src/fraction.rs. Read ${REF}/src/terminal/fraction.zig
(the custom fraction parser from 946422dbc that REPLACED std.fmt.parseFloat). CRITICAL: its parse
REJECTS exponent and hex-float syntax that Rust's f64::parse ACCEPTS — port the exact accepted
grammar and every test, including the rejection cases. The call sites that switch to it are a later
wave; here only create the module + tests + pub mod.
(2) src/fastprint.zig -> rustty-vt/src/fastprint.rs. Read ${REF}/src/fastprint.zig (the fast print
path). Port fully with tests + pub mod.
Build cargo build -p rustty-vt and cargo test -p rustty-vt --lib fraction:: fastprint:: to confirm
your new tests pass; a full-lane run is not required (the crate is large) but must at least build.`,
  },
  {
    label: 'os',
    patch: 'w1-os',
    prompt: `Crate rustty-os (${REPO}/crates/rustty-os) — TWO new files:
(1) NEW src/os/stderr.zig -> rustty-os/src/stderr.rs. Read ${REF}/src/os/stderr.zig. Joint
decision (assessment E + C): the motive is a Zig-toolchain detail but the primitive is a useful
signal-safe raw-write panic-stderr seam — port the POSIX arm (hand-rolled extern "C" write(2), NO
new libc edge, per the mem.rs precedent). Keep it cfg-switchable (R8), do not fold to one platform.
(2) NEW src/os/edit.zig -> rustty-os/src/edit.rs. Read ${REF}/src/os/edit.zig. getConfigEditCommand
resolves VISUAL/EDITOR + a default_editor enum. Port with tests. The App.Message.OpenConfig payload
consumers are later waves; here port the os/edit function + enum + tests + pub mod.
Wire both into rustty-os lib.rs. cargo build -p rustty-os + cargo test -p rustty-os --lib.`,
  },
  {
    label: 'simd',
    patch: 'w1-simd',
    prompt: `Crate rustty-simd (${REPO}/crates/rustty-simd) — the scalar UTF-8 decoder rewrite.
The assessment: "vt.rs still has the deleted utf8_seq_info + per-byte ASCII path; scalar rewrite
is standalone-safe (existing differential test is the regression check)." Find the upstream
commit(s) touching src/simd's scalar UTF-8 path in range:
  git -C ${REF} log --oneline 6ad1fe7d8..12967b68f -- src/simd
Read them, and read ${REF}/src/simd/ scalar utf8 decoder end state. Port the rewrite into
rustty-simd/src/vt.rs (remove the deleted utf8_seq_info + per-byte ASCII path, add the new scalar
path). The EXISTING differential test in rustty-simd is your regression check — it must still pass.
cargo build -p rustty-simd + cargo test -p rustty-simd --lib. If the range has NO src/simd scalar
utf8 changes (double-check the log), report that as "nothing to port" with the evidence and emit an
empty patch.`,
  },
]

phase('Port')
const results = await parallel(TASKS.map(t => () =>
  agent(COMMON + `\n\nYOUR TASK (patch name: ${t.patch}):\n` + t.prompt,
    { label: t.label, phase: 'Port', schema: SCHEMA, isolation: 'worktree' })
    .then(r => r ? { ...r, _label: t.label } : null)
))

return { tasks: results.filter(Boolean) }
