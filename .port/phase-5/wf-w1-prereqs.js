export const meta = {
  name: 'p5-w1-prereqs',
  description: 'Phase-5 W1: leaf prerequisites — simd base64 streaming, wuffs swizzle, paste core, sys randomSecure, config option',
  phases: [{ title: 'Port', detail: 'worktree-isolated, patch harvest' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
You are a Phase-5 porter for the rustty port (full-fidelity Zig->Rust port of ghostty).
ISOLATED git worktree of ${REPO}; FIRST: git log --oneline -1 — if not cc8c6530 or newer,
git reset --hard main. Oracle AT the pin 683d8db64: ${REF} (working-tree files ARE the end
state; range 12967b68f..683d8db64; intermediates via git show).

READ FIRST: your assessment rows in ${REPO}/.port/phase-5/assessments/ (named per task);
${REPO}/docs/superpowers/plans/2026-08-25-phase5-upstream-resync.md (traps + hazards are
binding); ${REPO}/.port/PORT_RULES.md. Phase 4 just landed everything through 12967b68f —
build on it (git log --oneline -16 shows the phase-4 series).

BINDING: R1 (every line/comment/test assertion-for-assertion; END STATE only — the named
reverted intermediates are NOT ported, their surviving tests are); R2 renames; R7
arithmetic; R9 SAFETY tags; DG5; SB raw-ptr discipline. Match surrounding idiom.
VERIFY: cargo build + cargo test --lib green per touched crate.
HARVEST: git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch (non-empty).
Report per schema; every upstream test carried by name.
`

const SCHEMA = {
  type: 'object',
  required: ['patch_path', 'summary', 'test_evidence', 'tests_carried', 'caveats'],
  properties: {
    patch_path: { type: 'string' }, summary: { type: 'string' },
    files_created: { type: 'array', items: { type: 'string' } },
    files_modified: { type: 'array', items: { type: 'string' } },
    tests_carried: { type: 'array', items: { type: 'string' } },
    test_evidence: { type: 'string' }, caveats: { type: 'string' },
  },
}

const TASKS = [
  {
    label: 'w1-simd-b64', prompt: `PATCHNAME p5-simd-b64. Assessment B cluster B-3: the NEW
simd base64 work — ${REF}/src/simd/base64.zig Streaming decoder (+349) + strict/streaming
split + OSC bulk-reading hunks. Port into crates/rustty-simd (rustty-simd already has base64
from Phase 4's c992658b2 wiring — extend to the end state). The Streaming decoder's
EOF-at-padding stream-restart is load-bearing tested behavior (clipboard WriteState hazard #5
in the plan). Every upstream test. cargo test -p rustty-simd --lib green.`,
  },
  {
    label: 'w1-wuffs', prompt: `PATCHNAME p5-wuffs. Assessment A B4's wuffs prerequisite + D's
pkg/wuffs rows: NEW ${REF}/pkg/wuffs/src/swizzle.zig (rgbToRgba/gToRgba/gaToRgba/rgbaSrcOver)
and the vendored C shim headers ${REF}/pkg/wuffs/include/{stdlib.h,string.h} into rustty's
wuffs vendoring (crates-sys/wuffs-sys — read its build.rs/vendor layout first; the swizzle
functions are Zig wrappers over wuffs' C swizzler — port them where graphics_pixel can reach
them WITHOUT a new crate edge; rustty-vt already depends on the sys crate? verify with
cargo tree -p rustty-vt -i wuffs-sys or the sys::DecodePngFn seam from Phase 4 — if vt
reaches wuffs only via the vt-c seam, put the swizzle wrappers behind the same sys hook
pattern and note it). Every upstream test. Builds + tests green.`,
  },
  {
    label: 'w1-paste', prompt: `PATCHNAME p5-paste. Assessment A B7 + B's paste rows: NEW
${REF}/src/terminal/paste.zig (shared paste core) + ${REF}/src/input/paste.zig end state
(G1b input_paste drop=7). Port into crates/rustty-vt/src/paste.rs + rustty-input's paste
module (follow where rustty-input's existing paste code lives — the drop=7 module is
input_paste). Also the a5bb22e23 PasteEvent slice IF it is separable from the clipboard
bundle (check the assessment row; if it is clipboard-coupled, note it for W3 instead).
Every upstream test. vt + input lanes green.`,
  },
  {
    label: 'w1-sys-config', prompt: `PATCHNAME p5-sys-config. Two leaf rows:
(1) dda8e6f31 randomSecure: NEW ${REF}/src/terminal/sys.zig override surface (secure-random
entropy for clipboard OTP) — rustty-vt already has a sys.rs runtime-hook module (Phase 4
extended it for decode_png); add the randomSecure hook the same way + the os-side default
impl (getrandom/SecRandomCopyBytes per platform, hand-rolled extern per the mem.rs no-libc
precedent — check what rustty-os already wraps). The c/sys.zig + vt/sys.h C surface is W6's.
(2) The config clipboard write-limit option (assessment E bundle KC's config half,
5350d4a5f/70f006575 END STATE — the limit plumbing survives the revert): rustty-config
option + docs + the goldens/pins regeneration (bash .port/phase-3/w9-regen-goldens.sh; bump
the mdgen/extra field-count pins as Phase 4 did — grep 208). Every upstream test; config +
os + vt + extra lanes green.`,
  },
]

phase('Port')
const results = await parallel(TASKS.map(t => () =>
  agent(COMMON + '\nYOUR TASK: ' + t.prompt,
    { label: t.label, phase: 'Port', schema: SCHEMA, isolation: 'worktree' })
    .then(r => r ? { ...r, _label: t.label } : null)
))

return { tasks: results.filter(Boolean) }
