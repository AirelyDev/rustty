export const meta = {
  name: 'p4-par1-platform',
  description: 'Phase-4 parallel tier 1: cli ssh-cache, config bundle, po/ i18n wave, .github mirrors, crash restructure',
  phases: [{ title: 'Port', detail: 'worktree-isolated, patch harvest; crates disjoint from the in-flight vt work' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
You are a Phase-4 porter for the rustty port (full-fidelity Zig->Rust port of ghostty).
You run in an ISOLATED git worktree of ${REPO} at current main HEAD with its own build dir.
Oracle (read-only, AT the target pin 12967b68f): ${REF}; working-tree files ARE the end state.
Range: 6ad1fe7d8..12967b68f. Intermediates: git -C ${REF} show <hash>:<path>.

READ FIRST: your area assessment in ${REPO}/.port/phase-4/assessments/ (named in your task) —
its rows are the spec; the plan ${REPO}/docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md
(Global Constraints); ${REPO}/.port/PORT_RULES.md.

BINDING: R1 (every line/comment/test, end-state only, superseded intermediates NOT ported,
their tests carried); R2 per-token renames incl. inside strings/msgids; R7 arithmetic
(saturating/checked + debug_assert!); R9 SAFETY tags; zig byte-slices -> Vec<u8>/&[u8] never
lossy String; match surrounding rustty idiom.

VERIFY: cargo build -p <crate> + cargo test -p <crate> --lib green (0 failed) for every crate
you touch; note the final "test result:" lines.

HARVEST from the worktree repo root:
  git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch
Do NOT git commit. Confirm non-empty. Report per schema with every upstream test carried.
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

const TASKS = [
  {
    label: 'w6-clissh',
    prompt: `PATCHNAME w6-clissh. Assessment E cluster "ssh-terminfo-cache (atomic bundle A)"
[.port/phase-4/assessments/E-os-termio-cli-app.md] — 4 ABSENT commits, ~280 lines, owns G1b
cli_ssh_cache_diskcache drop=1. Port the END STATE of the versioned SSH terminfo cache into
rustty-cli (crates/rustty-cli): versioned cache entries (comptime Wyhash of the encoded
terminfo -> in Rust a const/computed-at-build hash with the same value semantics), isFailure
classification, cache-path-naming errors, infocmp short-circuit removal. Signatures interlock —
land ALL FOUR commits' end state together (find them: git -C ${REF} log --oneline
6ad1fe7d8..12967b68f -- src/cli/ssh_cache/ src/cli/ssh-cache — and read the assessment rows).
Carry the dropped DiskCache isFailure test + every other new test. Also (same cluster set, tiny):
the list-themes g/G aliases row and the LineIterator 2 regression sub-cases as identical-output
assertions (see the E assessment "cli-small" cluster for exact rows).`,
  },
  {
    label: 'w8-config',
    prompt: `PATCHNAME w8-config. Assessment G clusters 1-3 [.port/phase-4/assessments/
G-config-i18n-datastruct.md] into crates/rustty-config:
(1) 29b82dd80 hex-escape BYTE preservation in string.rs (zig-byteslice rule: \\xNN escapes must
produce raw bytes, not UTF-8 encoding) + the new test G1b masks.
(2) Theme tilde/HOME END STATE (4 ABSENT commits, 2 superseded intermediates NOT ported):
Config-level expandHome helper + Theme.finalize + Config.finalize call + WorkingDirectory
refactor + the rewritten test.
(3) Config tail: drag-handle option, link-osc8 option (option only — the runtime consumer is
another wave), edit.zig paths row (rustty-os::edit landed already on main — wire the config
side if the assessment row says so), open_config default, the 4 doc rewrites. SKIP the
cmd-translation loop row (blocked on input translated(), another wave) and note it.
Verify with cargo test -p rustty-config --lib. Config doc rewrites must keep G3b happy (the
oracle sentences must appear in the port).`,
  },
  {
    label: 'w8-po',
    prompt: `PATCHNAME w8-po. Assessment G cluster "po/ i18n wave" — 31 ABSENT commits,
~27k lines, MECHANICAL END-STATE MIRROR. Method (state it in your report):
For each of the oracle's po files (33 existing catalogs + NEW po/da.po + the pot template):
copy the ORACLE END STATE (${REF}/po/*) over rustty's po/, then apply R2 per-token renames
(Ghostty->Rustty, ghostty->rustty; the oracle pot has 9 'Ghostty' occurrences; memory
rename-casing-fidelity) to msgids AND msgstrs AND header comments. rustty's catalogs are
byte-frozen at the range-start state (79 msgids) — the end state is 253 msgids uniformly.
Check first how rustty's existing po/ differs from the oracle's at range-START (e.g. is it
already renamed? diff po/xx.po between the two trees for one file to learn the transform),
then reproduce exactly that transform on the END state. e40a5475d governance-section fork is
N-A per the assessment — preserve rustty's forked section if one exists.
Verify: msgfmt --check on every catalog if available (else python polib or a structural count),
report per-file msgid counts (all should be 253-ish), and grep 0 remaining 'ghostty'/'Ghostty'
tokens (case-sensitive both) outside deliberate references. Also the src/os/i18n.zig N_ +
README pairing row if the assessment G/E rows place it here. No cargo crates touched unless
the xtask translations check needs a fixture update — run cargo test -p xtask if it exists.`,
  },
  {
    label: 'w8-github',
    prompt: `PATCHNAME w8-github. Assessment G cluster ".github" — 3 ABSENT rows:
(1) DELETE .github/ISSUE_TEMPLATE/config.yml and .github/ISSUE_TEMPLATE/preapproved.md
(upstream deleted them; rustty still carries both).
(2) issue-triage.yml: add the new required doc-search checkbox per the upstream hunk.
(3) release-tip.yml (or wherever rustty pins paths-filter): bump dorny/paths-filter to v4.0.3
matching upstream.
Read the exact upstream hunks: git -C ${REF} log --oneline 6ad1fe7d8..12967b68f -- .github
then git -C ${REF} show <hash> -- .github/... . Mirror faithfully with R2 renames. The 35 N-A
rows (VOUCHED.td, Zig-build CI) are NOT ported — do not touch them. No build to run; validate
YAML syntax (python -c 'import yaml,sys;yaml.safe_load(open(...))') for each edited workflow.`,
  },
  {
    label: 'w6-crash',
    prompt: `PATCHNAME w6-crash. Assessment E cluster "crash-no-global-state (bundle G)" — 2
ABSENT commits (15c50c1db crash/sentry substance + a82637b53), ~320 lines, into
crates/rustty-crash (+ the global-lane caller if it lives in rustty-global/rustty-core —
follow the assessment rows). END STATE: sentry.init takes an OWNED env-map snapshot; ALL dir
resolution happens on the init thread; statics for state/cache dirs; Transport stops using the
global alloc(-equivalent). rustty's initThread currently reads the global env live (setenv
race class) — that is the bug being fixed.
ESCALATION (binding): upstream 15c50c1db INVERTED the macOS XDG_CACHE_HOME cacheDir test
(comment contradicts code; a82637b53 kept the original). Do NOT port the inversion: port the
a82637b53 behavior, and APPEND a row to ${REPO}/.port/phase-4/UPSTREAM_BUGS.md (create the
file if absent, modeled on .port/phase-3/UPSTREAM_BUGS.md) recording the contradiction with
the exact upstream lines quoted. Carry every portable test; cargo test -p rustty-crash --lib
plus any touched caller crate lane.`,
  },
]

phase('Port')
const results = await parallel(TASKS.map(t => () =>
  agent(COMMON + '\nYOUR TASK: ' + t.prompt,
    { label: t.label, phase: 'Port', schema: SCHEMA, isolation: 'worktree' })
    .then(r => r ? { ...r, _label: t.label } : null)
))

return { tasks: results.filter(Boolean) }
