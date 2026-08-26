export const meta = {
  name: 'p5-missed-six',
  description: 'Phase-5 finale: port the 6 upstream commits the assessments missed (exposed by the G3b sweep)',
  phases: [{ title: 'Port', detail: 'two agents: the big row-reset/cursor/print trio, and the small trio' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
Phase-5 gap-closure porter. ISOLATED worktree of ${REPO}; FIRST: git log --oneline -1 — if
not 4e9d5e10 or newer, git reset --hard main (main also carries an uncommitted-then-landed
comment sweep — your base includes ghost comments for some of YOUR code; make them real).
Oracle AT pin 683d8db64: ${REF}.

CONTEXT: the G3b comment sweep exposed six upstream commits (2026-08-18..25, inside the
pin) that the area-B assessment missed — vt correctness fixes. The sweep's per-commit
oracle file:line attributions are in the p5-g3b-sweep workflow result; the sentences it
could not restore are comments on YOUR code. Binding: .port/PORT_RULES.md (R1 end-state,
R2, R7 saturate-and-assert, R9, SB raw-ptr discipline — 33cda4dc5 is ABOUT a stale-pointer
bug class; its port must be Miri-proven on the touched paths).

METHOD: fail-first where the commit fixes a bug reproducible in rustty (bed20eb36 tabstop
XOR-toggle IS a live rustty bug at tabstops.rs:195,201 — prove RED first); END STATE; every
test hunk assertion-for-assertion incl. the AFL-fuzz regression of 9313d580c; the restored/
ghost comments become live comments on the ported code (G3b vt must reach missing=0 — run
python3 .port/tools/gates/check_comment_coverage.py --crate vt at the end and report).
VERIFY: full vt lane green; Miri on touched filters.
HARVEST: git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch (non-empty).
Report per schema; every upstream test carried.
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

phase('Port')
const results = await parallel([
  () => agent(COMMON + `
YOUR TASK (PATCHNAME p5-missed-big): the pagelist/screen/terminal trio —
(1) b31fbc846 'fully reset row metadata when recycling row storage': Page::resetRow/
Row::reset (page.zig:1294-1300, 2057-2070) + ALL call sites across PageList/Screen/Terminal
+ every test (93 attributed sentences map the surface — the largest of the six).
(2) 9313d580c 'fix stale cursor style/hyperlink state after scroll clear': cursor_change_pin
routing in cursor_reload, early hyperlink_id zeroing, assert_integrity cursor style/
hyperlink refcount asserts, the AFL-fuzz regression test (Screen.zig:383-4253,
Terminal.zig:7441-7480).
(3) 33cda4dc5 'reload cell pointers when print grows a page': print must re-derive the cell
pointer via node+serial identity across the spacer-tail write and append_grapheme; the
failed-hyperlink debug_assert must not read the stale cell; both regression tests
(Terminal.zig:1342-1396, 6196-6255). This is the SB-hazard one — Miri-prove the print
filters after porting.
You own page*.rs, pagelist/**, screen.rs, terminal.rs.`,
    { label: 'p5-missed-big', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),

  () => agent(COMMON + `
YOUR TASK (PATCHNAME p5-missed-small): the small trio —
(1) bed20eb36 'clear tabstop bit on unset instead of toggling': tabstops.rs:195,201 XOR ->
clear (LIVE BUG — prove RED first with the upstream no-op-not-a-toggle test additions,
Tabstops.zig:202,221).
(2) eeab85b96: the RIS tail of the 'progress_report effect callback' test in
stream_terminal.rs (handler code already ported — test extension only,
stream_terminal.zig:3340).
(3) f1948d505 'transfer selection text into string maps': the selectionStringMap transfer
path in screen.rs (Screen.zig:2941 region) replacing the opts.map duplicate-copy shape +
its tests. COORDINATE: the sibling owns screen.rs for cursor work — your f1948d505 touch is
the selection_string region only; if you collide, the orchestrator 3-ways (keep your hunks
minimal and localized).
You own tabstops.rs, stream_terminal.rs (that one test), and screen.rs selection_string
region.`,
    { label: 'p5-missed-small', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),
])

return { tasks: results.filter(Boolean) }
