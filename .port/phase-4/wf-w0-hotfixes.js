export const meta = {
  name: 'p4-w0-hotfixes',
  description: 'Phase-4 W0: fail-first fixes for live bugs shipping in rustty today',
  phases: [{ title: 'Fix', detail: 'worktree-isolated: prove RED, fix, prove GREEN, report exact edits' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'

const COMMON = `
You are a Phase-4 live-bug fixer for the rustty port (full-fidelity Zig->Rust port of ghostty).
You run in an ISOLATED git worktree — a private copy of ${REPO} at current main, with its own
build dir. Edit freely and run cargo here; your changes do NOT touch other agents.

Oracle (read-only reference, AT the target pin already): ${REF}. Read upstream with
git -C ${REF} show <hash> -- <path>  (or the working tree files directly — they ARE the target).

BINDING RULES (${REPO}/.port/PORT_RULES.md): R1 fidelity (port the upstream fix AND its test
assertion-for-assertion, preserve comments); R2 renames (ghostty->rustty, Ghostty->Rustty per
token, incl. inside test strings); R7 arithmetic (saturating_*/checked_* + debug_assert!, never
bare ops for Zig +|/-|/@abs); DG5 (extern "C" abort). SB/UB: never take &mut Page/Row/Cell/Screen
re-read across a mutating call — use raw pointers (memory rust-mut-args-strongly-protected).

METHOD (TDD, mandatory):
1. Read the upstream commit(s) for your bug and the cited rustty code.
2. Write a FAIL-FIRST test that reproduces the bug (port the upstream test, R2-renamed). Run it,
   CONFIRM IT FAILS (panic/wrong value/Miri error) on the unmodified rustty code. If it does NOT
   fail, your understanding is wrong — investigate, do not proceed with a vacuous test.
3. Apply the minimal fix (the upstream end state).
4. Run the test again, CONFIRM GREEN. Then run the crate's fast test lane
   (cargo test -p <crate> --lib) and confirm no regressions you introduced.
5. Report the EXACT edits so the orchestrator can replay them in the main tree.

Your report (structured) must let the orchestrator apply your change WITHOUT re-deriving it:
for each file, give exact old_string->new_string pairs (unique, with enough context to match
once) and any new test code as a full block with its insertion anchor (the exact existing line
your new code goes after). Include the verify command and the observed RED and GREEN evidence.
Do NOT git commit — the orchestrator commits in the main tree.
`

const SCHEMA = {
  type: 'object',
  required: ['bug', 'crate', 'red_evidence', 'green_evidence', 'verify_cmd', 'files'],
  properties: {
    bug: { type: 'string' },
    crate: { type: 'string' },
    red_evidence: { type: 'string', description: 'the observed failure BEFORE the fix' },
    green_evidence: { type: 'string', description: 'the observed pass AFTER the fix' },
    verify_cmd: { type: 'string' },
    files: {
      type: 'array',
      items: {
        type: 'object',
        required: ['path', 'edits'],
        properties: {
          path: { type: 'string', description: 'repo-relative' },
          edits: {
            type: 'array',
            items: {
              type: 'object',
              required: ['old_string', 'new_string'],
              properties: {
                old_string: { type: 'string' },
                new_string: { type: 'string' },
                note: { type: 'string' },
              },
            },
          },
        },
      },
    },
    caveats: { type: 'string' },
  },
}

const BUGS = [
  {
    label: 't1-title-report',
    crate: 'rustty-vt',
    prompt: `BUG: CSI 21 t title report is answered UNCONDITIONALLY when write_pty is set — a
command-injection exposure upstream fixed in 38e891e6c ("gate title report behind opt-in").
Upstream: git -C ${REF} show 38e891e6c. Read it fully — it adds a bool option (default OFF) and
only emits the title report when enabled, and it also re-partitions try/log for several report
sites (setAttribute/windowTitle/reportPwd promoted to errors; colorOperation/kittyColorOperation/
clipboardContents stay logged). For W0, port the VT-SIDE END STATE: the title-report path gated
behind the new option, DEFAULT OFF, plus upstream's amended test asserting no response without
opt-in. rustty side: crates/rustty-vt/src/stream_terminal.rs (see the size_report_csi_21_t_title
test ~line 4789 and the report emission path). The config/C-API wiring lands in a later
workstream — here, add the Terminal-level flag (mirroring upstream's field) defaulting to the
safe (no-report) behavior, and the guard + test. If the title report in rustty currently routes
through size_report, gate exactly that emission. Preserve every comment.`,
  },
  {
    label: 't2t4-terminal-vs15-osc-grapheme',
    crate: 'rustty-vt',
    prompt: `TWO independent bugs in the same crate (report BOTH, files may include terminal.rs and
osc/mod.rs):
(A) 33d34cf5c: VS15 on a wide base under the cursor -> cursor_cell_left(prev_left - 1) UNDERFLOWS
at crates/rustty-vt/src/terminal.rs ~line 2208 (debug panic; release OOB cell pointer). Read
git -C ${REF} show 33d34cf5c. Port the fix + its test.
(B) 727b8a02f: two allocation-cap DoS fixes. (i) The OSC capture path has NO bound — add the
8 MiB Capture.max_allocating_bytes cap in crates/rustty-vt/src/osc/mod.rs (grep max_allocating ->
0 today). (ii) Unbounded zero-width codepoints per cell force unbounded grapheme-capacity
doublings — add the grapheme_max_len = 64 cap on the grapheme append path. Read
git -C ${REF} show 727b8a02f -- src/terminal/osc.zig src/terminal/page.zig src/terminal/Terminal.zig
and port both caps + their tests. (Note: the parser call sites in apc/other files are a LATER
task — here port only the osc.zig cap machinery and the grapheme cap.) R7: caps use saturating/
checked forms. Preserve comments.`,
  },
  {
    label: 't3-hyperlink-reflow-crash',
    crate: 'rustty-vt',
    prompt: `BUG d5c7e54ae: hyperlink reflow string-capacity check at crates/rustty-vt/src/pagelist/
resize.rs ~line 1652 uses the COMBINED-alloc test, but the dup performs TWO chunk-rounded
allocations that can fail after the combined check passes (ghostty #13522 -> an unreachable/crash
on resize). Read git -C ${REF} show d5c7e54ae fully (it splits the capacity check to match the two
actual allocations). Port the end-state fix + its regression test. Verify the test reproduces the
crash on unmodified rustty (it should panic/hit the unreachable). Preserve comments.`,
  },
  {
    label: 't5-kitty-range-delete',
    crate: 'rustty-vt',
    prompt: `BUG d866fa455: Kitty graphics RANGE delete joins the image-id bounds with OR, matching
EVERY placement for any valid range (a targeted delete wipes all graphics). At
crates/rustty-vt/src/kitty/graphics_storage.rs ~line 673:
  if key.image_id >= first || key.image_id <= last {   // WRONG: || matches everything
must become  &&  (inclusive bounds). Read git -C ${REF} show d866fa455 — it also UPDATES the
lowercase and uppercase range tests to use first=2,last=2 with placements kept below (1) and
above (3). Port the fix AND the amended test assertions (this is a gate-invisible class: the
fix lives inside existing tests, so you MUST port the amended test bodies). Verify the amended
test fails on the || code and passes on &&. Preserve comments.`,
  },
  {
    label: 't6-color-reset',
    crate: 'rustty-vt',
    prompt: `BUG 7cd2f65f5: OSC color reset pins the override to the CURRENT default instead of
clearing it. At crates/rustty-vt/src/color.rs ~line 687:
  pub fn reset(&mut self) { self.override_ = self.default; }   // WRONG
Read git -C ${REF} show 7cd2f65f5 — reset must set the override to NONE (clear it), and there is a
stream_terminal regression-test half. Port the fix + the regression test (R2-renamed). Verify the
test fails today (reset leaves the old default pinned) and passes after. Preserve comments.
Check color.rs's reset semantics against the upstream Color/PaletteColor reset carefully — match
the exact field the upstream clears.`,
  },
  {
    label: 't7-renderer-framecounter-uaf',
    crate: 'rustty-renderer',
    prompt: `TWO renderer bugs (report both):
(A) 9e6e2ea96: rustty MISSES the frame-counter reset — after ~100k frames the terminal render
state deinits+rebuilds the FULL state every frame (perf collapse after ~12min at 120Hz). Read
git -C ${REF} show 9e6e2ea96. One-line fix (reset terminal_state_frame_count) + a unit test that
drives the counter past the threshold and asserts no full rebuild.
(B) 4b4a5b241: rustty's IOSurfaceLayer release/Drop NEVER clears the display callback, so a
host-retained layer can invoke the callback on a FREED renderer context (R9 callback-ctx
use-after-free). Read git -C ${REF} show 4b4a5b241. Port the fix (clear the callback on
release/Drop) + an assertion that Drop clears it. This is macOS-only (#[cfg(target_os="macos")]).
If a full runtime UAF test is not feasible headless, assert the callback pointer is nulled on
Drop. Preserve comments and any FAITHFULNESS/PORT NOTE that becomes wrong (delete, don't amend).`,
  },
  {
    label: 't8-circbuf-seqset-hotkeys-skill',
    crate: 'multi',
    prompt: `FOUR small independent fixes across crates (report all; files span 4 crates):
(A) CircBuf resize-shrink corruption [assessment G]: crates/rustty-datastruct/src/circ_buf.rs —
resize-shrink leaves head/tail/full inconsistent (live: callers are vt search sliding_window,
tmux viewer, apc glossary, inspector). Upstream measured 24 zig tests vs 21 rust (3 dropped).
Find the upstream circ_buf commit(s) in range: git -C ${REF} log --oneline 6ad1fe7d8..12967b68f --
src/datastruct/circ_buf.zig  and git -C ${REF} log --oneline 6ad1fe7d8..12967b68f -- src/circ_buf.zig .
Port the shrink fix + the 3 dropped tests. Verify a shrink-then-access test fails today.
(B) 0060d89b5: crates/rustty-core/src/surface.rs ~line 6743 assigns keyboard.sequence_set BEFORE
the fallible encodeKey — on encode error the state is corrupted. Read git -C ${REF} show 0060d89b5.
Reorder so sequence_set is assigned only AFTER encodeKey succeeds. Add/port the regression test.
(C) fd47b15cd: the gtk Hotkeys deinit LEAK fix — rustty still calls self.clear() where upstream
deinits. Find crates/rustty-apprt/.../hotkeys.rs ~line 265. Read git -C ${REF} show fd47b15cd and
port the deinit (full teardown, not clear). If a leak test is Miri/ASan-only, note it.
(D) fde9e281c: .agents/skills/writing-commit-messages/SKILL.md carries a double negative
("without no filler phrases") upstream fixed. Read git -C ${REF} show fde9e281c and apply the
one-word fix.
For each, prove RED where a test is feasible; (C)/(D) may be assertion-free — say so in caveats.`,
  },
]

phase('Fix')
const results = await parallel(BUGS.map(b => () =>
  agent(COMMON + '\n\nYOUR BUG:\n' + b.prompt,
    { label: b.label, phase: 'Fix', schema: SCHEMA, isolation: 'worktree' })
    .then(r => r ? { ...r, _label: b.label } : null)
))

return { fixes: results.filter(Boolean) }
