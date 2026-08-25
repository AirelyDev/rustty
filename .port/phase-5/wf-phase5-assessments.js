export const meta = {
  name: 'phase5-assessments',
  description: 'Phase-5 upstream resync: per-area re-vetting assessments of 12967b68f..683d8db64',
  phases: [
    { title: 'Assess', detail: 'five parallel area agents' },
    { title: 'Reconcile', detail: 'coverage critic' },
  ],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const RANGE = '12967b68f..683d8db64'

const COMMON = `
You are a Phase-5 re-vetting assessment agent for the rustty port. Repo: ${REPO}.
Oracle at ${REF}, working tree AT THE NEW PIN 683d8db64 (= origin/main) — read target files
directly; intermediates via git -C ${REF} show <hash>:<path>. NEVER write to the oracle.

READ FIRST: ${REPO}/.port/phase-5/README.md; ${REPO}/.port/phase-4/README.md ("Binding
rules"); ${REPO}/.port/PORT_RULES.md; a Phase-4 assessment in
${REPO}/.port/phase-4/assessments/ for format precedent; the G1b debt in
${REPO}/.port/phase-5/w0/g1b-after-at-683d8db64.txt.

CONTEXT: Phase 4 just absorbed everything through 12967b68f (finale commit d63dd9fc, all
gates green) — so ABSENT is the expected verdict for most range work; verify PORTED claims
with file:line + guard-test evidence anyway (hash citations are not evidence).

YOUR JOB — for YOUR AREA's file set:
1. git -C ${REF} log --oneline --no-merges ${RANGE} -- <paths> ; verdict EVERY commit:
   PORTED / PARTIAL / ABSENT / N-A with evidence (reverted-in-range = N-A with the hash).
2. Assess the RANGE-DIFF END STATE where commits rewrite the same code; list every
   commit's tests against the end state.
3. Name ATOMIC BUNDLES, cross-area couplings, SB/UB hazards (raw-ptr Page/Row/Cell/Screen
   discipline; decoder/iterator aliasing), and the G1b dropped-test mapping for your area.
4. COVERAGE HONESTY: exhaustive vs sampled, with the filter stated.
5. Write ${REPO}/.port/phase-5/assessments/<FILE> in the Phase-4 format.
Do NOT modify code.
`

const SCHEMA = {
  type: 'object',
  required: ['area', 'file', 'commits_total', 'ported', 'partial', 'absent', 'na', 'clusters', 'couplings', 'risks'],
  properties: {
    area: { type: 'string' }, file: { type: 'string' },
    commits_total: { type: 'integer' }, ported: { type: 'integer' },
    partial: { type: 'integer' }, absent: { type: 'integer' }, na: { type: 'integer' },
    clusters: { type: 'array', items: { type: 'object', required: ['name', 'verdict_mix', 'approx_lines', 'commits'], properties: {
      name: { type: 'string' }, verdict_mix: { type: 'string' }, approx_lines: { type: 'integer' },
      commits: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' } } } },
    couplings: { type: 'array', items: { type: 'string' } },
    risks: { type: 'array', items: { type: 'string' } },
  },
}

const AREAS = [
  {
    key: 'A-kitty-subsystems', file: 'A-kitty-subsystems.md',
    paths: 'src/terminal/kitty',
    extra: `The dominant mass: NEW kitty clipboard protocol (clipboard{,_command,_grants,_response,_write}.zig),
NEW kitty DnD protocol (dnd{,_command,_drop,_response,_test}.zig), NEW graphics_animation.zig +
graphics_pixel.zig, plus churn to the existing graphics_* files Phase 4 just ported (re-vet those
hunks against rustty's current end state — some may already match if upstream backported). G1b
kitty drop=202 — map every dropped test to its commit. rustty side: crates/rustty-vt/src/kitty/.`,
  },
  {
    key: 'B-terminal-core', file: 'B-terminal-core.md',
    paths: 'src/terminal src/Surface.zig src/simd',
    extra: `EXCLUDING src/terminal/kitty (area A owns it). Majors: NEW src/terminal/paste.zig
(input_paste drop=7), NEW osc/kitty_metadata.zig + osc/parsers/kitty_desktop_notification.zig
(osc drop=47), stream_terminal churn (drop=38), parser (drop=1), snapshot/formatter/page churn
against Phase-4's fresh ports, simd hunks, Surface.zig. rustty: crates/rustty-vt/, rustty-simd,
rustty-core surface arms.`,
  },
  {
    key: 'C-capi-lib', file: 'C-capi-lib.md',
    paths: 'include src/lib src/lib_vt.zig src/terminal/c example src/build src/global.zig',
    extra: `C-API/headers/lib tier on top of Phase-4's fully-green A5 surface: every new export or
type will flip A5 arm (d) RED — enumerate the exact delta (oracle lib_vt.zig exports at the new
pin vs rustty's 194). Include the examples and build hunks. rustty: crates/rustty-vt-c,
crates/rustty-lib, include/rustty*, the A5/A3 gates.`,
  },
  {
    key: 'D-ui-tiers', file: 'D-ui-tiers.md',
    paths: 'macos src/apprt src/apprt.zig src/renderer src/font src/input pkg dist',
    extra: `macOS wave (13 files: NEW ScriptKeyEventCommandTests + KeyboardLayoutTests — AppleScript
key-event feature?, read the commits), apprt (6), renderer (4, incl. head commit 683d8db64 "release
GPU resources for hidden surfaces (macOS)"), font (1), input (1), pkg/wuffs vendored C shim headers
(stdlib.h/string.h — rustty's wuffs-sys vendors the release-C artifact; check coupling), pkg/afl.
rustty: macos/, crates/rustty-{apprt,apprt-api,renderer,font*,input}, crates-sys/wuffs-sys.`,
  },
  {
    key: 'E-platform-tail', file: 'E-platform-tail.md',
    paths: 'src/termio src/cli src/os src/config po .github typos.toml valgrind.supp src/App.zig src/main_ghostty.zig',
    extra: `termio (5), cli (4), os (2), config (2), po (4 — end-state mirror method per Phase-4
par-1), .github (2), typos, valgrind.supp. rustty: crates/rustty-{termio,cli,os,config}, po/,
.github/.`,
  },
]

phase('Assess')
const results = await parallel(AREAS.map(a => () =>
  agent(COMMON + `\nYOUR AREA: ${a.key}. Assessment file: .port/phase-5/assessments/${a.file}\nFile-set (oracle paths): ${a.paths}\n\n` + a.extra,
    { label: `assess:${a.key}`, phase: 'Assess', schema: SCHEMA })
))

phase('Reconcile')
const summary = JSON.stringify(results.filter(Boolean).map(r => ({ area: r.area, absent: r.absent, partial: r.partial })))
const critic = await agent(
  `Coverage-reconciliation critic, Phase-5 (range ${RANGE}). Repo ${REPO}, oracle ${REF} at the
new pin. Assessments: ` + AREAS.map(a => a.file).join(', ') + ` in .port/phase-5/assessments/.
Summaries: ${summary}
1. Map EVERY file in .port/phase-5/UPSTREAM_DIFF_STAT.txt (155 files) to its assessing area;
   orphans must be assessed inline by you.
2. Spot-check 8 randomized non-ABSENT verdicts (R11 default-refute) against the upstream hunk
   and cited rustty code.
3. Write .port/phase-5/assessments/RECONCILIATION.md (orphans, double-claims, spot-checks,
   global verdict table). Do not modify code or other assessments.`,
  { label: 'reconcile', phase: 'Reconcile', schema: {
    type: 'object', required: ['orphans', 'double_claims', 'spot_check_failures', 'file'],
    properties: { orphans: { type: 'array', items: { type: 'string' } },
      double_claims: { type: 'array', items: { type: 'string' } },
      spot_check_failures: { type: 'array', items: { type: 'string' } },
      file: { type: 'string' } } } },
)

return { areas: results.filter(Boolean), reconciliation: critic }
