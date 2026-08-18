export const meta = {
  name: 'p4-par3-reflow-snapshot-gtk',
  description: 'Phase-4 parallel tier 3: reflow chain, terminal tail, snapshot scaffold+records, GTK wave, E-tail, themes re-vendor',
  phases: [{ title: 'Port', detail: 'worktree-isolated, patch harvest; disjoint ownership' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
You are a Phase-4 porter for the rustty port (full-fidelity Zig->Rust port of ghostty).
You run in an ISOLATED git worktree of ${REPO}; FIRST ACTION: run git log --oneline -1 — if it
is not 95beca86 or newer, run git reset --hard main. Your own build dir; edit freely.
Oracle (read-only, AT target pin 12967b68f): ${REF}; working-tree files ARE the end state.
Range: 6ad1fe7d8..12967b68f. Intermediates: git -C ${REF} show <hash>:<path>.

READ FIRST: your assessment in ${REPO}/.port/phase-4/assessments/ (named per task);
${REPO}/docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md Global Constraints;
${REPO}/.port/PORT_RULES.md. NOTE what has ALREADY LANDED on main (W0 hotfixes, W1
foundations incl. crc32c/limited_allocator/fraction/fastprint/os edit+stderr, W2
builder+stream bundles, par-1 cli/config/po/crash, par-2 kitty/formatter/input/modes/
termio-pool/renderer-cell/notifications/macos/font) — do not re-port or revert any of it;
build on it.

BINDING: R1 (every line/comment/test assertion-for-assertion; END STATE only; superseded
intermediates' tests carried); R2 per-token renames; R7 arithmetic (saturating/checked +
debug_assert!); R9 SAFETY tags; DG5; SB/UB raw-ptr discipline (memories
rust-mut-args-strongly-protected, shaper-hook-sb-clean-interleave — prove restructured
aliasing on an isolated Miri harness or run Miri on the real lanes). Match surrounding idiom.

VERIFY: cargo build + cargo test --lib green (0 failed) for every crate touched; note the
final "test result:" lines. The vt full lane takes ~90-110s — run targeted filters while
iterating, ONE full lane at the end.

HARVEST from the worktree repo root:
  git add -A && git diff --cached > ${PATCHDIR}/<PATCHNAME>.patch
Do NOT git commit. Confirm non-empty. Report per schema; list every upstream test carried.
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
    label: 'w2-reflow',
    prompt: `PATCHNAME w2-reflow. Assessment A cluster **A-2 — reflow bugfix + bulk-copy perf
chain** (the biggest single mass; d5c7e54ae landed in W0 — do not re-port; the remaining 9:
4a88cc594, c5ca2db1b, c249b9de3, 179161c08, 46276d046, 0fb3565c7, d4e446c48, ec5b36961,
88ed6bebf) as ONE end-state port of PageList reflow/bulk-copy in
crates/rustty-vt/src/pagelist/ (+ page.rs Mask if 0fb3565c7's eql_any lives there):
bulkRunLength/copyRun FINAL shape (88ed6bebf supersedes c249b9de3's), page::Mask eql_any +
nested paths extension, 46276d046 src-page-memory-recycled-as-dst (the TOP SB/Miri hazard:
copyRun mutates dst styles while a src cell slice is live — raw-pointer discipline,
and PROVE the aliasing on an isolated non-FFI Miri harness at
${REPO}-worktree-local first, then include the harness as .port/phase-4/mirihar-reflow-recycle/
(copy the layout of .port/phase-2/mirihar-chunk3b) in your patch, plus run
cargo +nightly miri test -p rustty-vt --lib on the reflow test filters if miri is available).
12 upstream tests; the test "Pin row movement clamps across mixed-width pages" only MOVED
in the diff — it is already in rustty, do NOT double-port it. You own pagelist/** and
page*.rs ONLY — do not touch terminal.rs/stream*/kitty (another agent owns terminal.rs).
Full vt lane green at the end.`,
  },
  {
    label: 'w2-tail',
    prompt: `PATCHNAME w2-tail. Assessment A clusters "Terminal/stream behavior + security fixes"
(minus what landed: 38e891e6c/33d34cf5c/727b8a02f/d866fa455/7cd2f65f5 in W0; 4371871bc
setLimit + f973bd53b overline + c092b2bcf modes landed in par-2 — verify, don't re-port) and
"render-state / wasm / ABI-metadata / misc perf" native arms. You own
crates/rustty-vt/src/terminal.rs, stream_terminal.rs, render.rs (or render-state files) and
build_options.rs. Port: bfd40c84b (CSI 2 K wrap-state behavior + test), c992658b2
(rustty-simd base64 is ready), 5b70f208b, 51a4311ef (inlining cleanup), ac4dc1205,
946422dbc call sites (rustty-vt fraction.rs landed on main — switch the parse call sites to
it), 3d9b2b483 NATIVE arms (grapheme capacity projection, bitmap search_start, printSlice
check-hoist; SKIP wasm hash arm — blocked on wasm pool wiring, note it), e3056658d/
74a233b54/0d37f2d34 CORE-side render-state work (dirty-row iteration vt-side machinery;
the C exports are area C's later bundle), 0e8b7bea6 packed-cell layout CORE half,
1fdbb8c91 build_options.glyph_protocol gate sites + the O4 build_options.zig Features parse
machinery -> crates/rustty-vt/src/build_options.rs, and 74efadb44 XTGETTCAP source side —
RUN bash .port/tools/gates/check_acyclic.sh FIRST for the new terminal->terminfo edge; if
it creates a cycle, mirror by moving the table per the plan and record the decision. Also
the E-cluster terminfo bundle rows if in reach: Smol/Rmol overline entries in
crates/rustty-terminfo (regenerate the encode golden + note the ssh-cache version() hash
auto-updates — that is BY DESIGN, cite the par-1 caveat). Full vt lane green.`,
  },
  {
    label: 'wsnap-1',
    prompt: `PATCHNAME wsnap-1. Assessment B clusters C1+C2 [.port/phase-4/assessments/
B-snapshot-io.md] — the snapshot subsystem SCAFFOLD + page/style/hyperlink RECORDS, as
crates/rustty-vt/src/snapshot/ (the pub mod seam + main.zig-parity re-export line already
exist on main from the builder bundle — wire them live). Port the END STATE (NO BLAKE3;
decode vs decodeExact; Decoder=init/ready/next) of: snapshot/{main,snapshot,record,envelope,
checkpoint,continuation,io}.zig and snapshot/{page,style,hyperlink}.zig + AGENTS.md
(R2-renamed) + the .ksy spec file + ALL testdata hex goldens for these modules copied
BIT-FOR-BIT from ${REF}/src/terminal/snapshot/testdata/ (R1 comptime artifacts; copy the
files verbatim). Dependencies already on main: rustty-lib crc32c, PageList Builder/
allocate_page, stream_continuation, RefCountedSet iterator, hyperlink zero-len free guards.
SB DISCIPLINE: decode-directly-into-Page is the search-subsystem UB shape — raw-ptr
decode targets with narrow &mut scopes; transactional rollback must release style/hyperlink
refs on EVERY error path (carry every release-on-error test). If you restructure aliasing,
prove on an isolated Miri harness .port/phase-4/mirihar-snapshot-decode/ included in the
patch. The grid codec + screen/terminal records are the NEXT stage (another agent) — write
your module boundaries so grid.rs/screen.rs/terminal.rs drop in beside you (stub nothing;
just don't port them, and keep snapshot/main's re-export list commented for the absent
modules with a PORT NOTE). Every test{} in the files you port, assertion-for-assertion.
Full vt lane green.`,
  },
  {
    label: 'w7-gtk',
    prompt: `PATCHNAME w7-gtk. Assessment F cluster "GTK wave" (18 ABSENT, ~2,900 lines) +
its E pairings, in crates/rustty-apprt (gtk) + crates/rustty-cli:
B5 +new-tab: NEW ${REF}/src/apprt/gtk/ipc/new_tab.zig -> gtk ipc module + NEW
${REF}/src/cli/new_tab.zig -> crates/rustty-cli/src/new_tab.rs (from the new_window.rs
template; E cluster row) + performIpc .new_tab arm + the applyCommandOverrides
shell-integration overrides fix (3650bef4e) WITH its 7-assertion test (G1b-invisible —
must be carried explicitly). B6 split drag-and-drop + flicker fix (1d053bd6e SplitTree.locate
landed? check crates/rustty-datastruct split_tree — the G assessment says SplitTree.locate is
portable now; port it if absent, it is datastruct-local). B7 bell relocation. B8 native blur
pair + a6fc3e0e2 blur-region set-path leak fix. B9 export-terminal-io GTK side (the
inspector copy/export actions; the C-ABI enum halves are W5's — keep apprt-api compiling
with PORT NOTE arms as the input agent did). Window-title renaming pair + open-config-in-
window (os::edit landed on main; wire the GTK payload consumer). Teardown fixes: 1dbc8ca30
WeakRef.deinit — VERIFY rustty's gtk4-rs WeakRef wrapper semantics BEFORE porting (the
assessment warns a blind copy risks double-clear; record the verification either way in a
comment). Blueprints/CSS land in crates/rustty-apprt/assets/. hotkeys deinit landed in W0 —
don't re-port. VERIFY: cargo build -p rustty-apprt + cargo test -p rustty-apprt --lib (+ the
gtk feature lane if buildable on macOS — .port/tools/gates/check_gtk_feature_lane.sh will
tell you; if GTK cannot build on this host, get as far as cargo check allows, note precisely
what remains unverified for the Linux finale).`,
  },
  {
    label: 'w6-tail',
    prompt: `PATCHNAME w6-tail. The remaining assessment-E clusters not yet landed, in
rustty-core/rustty-os/rustty-cli/rustty-renderer:
(1) metal-warmup chain END STATE (4 commits, threadedWarmup dissolved): App.create spawns
detached font Discover.warmup (font side landed in par-2) + Renderer.API.warmup threads;
Metal.warmup primes device query, command queue, both shader pipeline formats.
(2) surface-write-lifecycle remnants: OSC52/long-key buffer transfer, pwd stackFallback,
readonly-queueIo test.
(3) surface-misc: 2b32b5b75 tempdir close(CloseMode) API + 2 tests; 77537c806 OSC8
fail-closed CORE+OS side (os/open Kind::Osc8 guard + Surface .osc8 switch LAND TOGETHER;
the C-ABI Kind + macOS mapping are seamed — the macOS Swift handler landed in par-2 with a
PORT-SEAM marker; keep that contract, i.e. core produces the kind internally and the apprt-
api enum gets its arm WITH a PORT NOTE citing W5-E for the header KAT).
(4) edit-config: cli edit_config rewrite over rustty-os::edit (landed) + App.Message
.OpenConfig payload consumers in rustty-core (the input agent landed the Action payload).
(5) prompt/set window-title + move_tab_to_new_window + mouseReportingActive core arms
(input Actions landed in par-2; wire the core Surface/App handling per the E rows).
Do NOT touch rustty-vt (other agents own it this wave). cargo test green per crate.`,
  },
  {
    label: 'w8-themes',
    prompt: `PATCHNAME w8-themes. RECONCILIATION orphan O7: the iTerm2 themes vendor pin.
Upstream now pins ghostty-themes-release-20260810-152212-0173c3c.tgz (see
${REF}/build.zig.zon — verify the exact name/hash); rustty's vendored corpus
(crates/rustty-extra/themes/ with provenance in its README.md) is at
20260720-153658-97e244c (one release behind). Re-vendor: download the new release tarball
from the ghostty-org/ghostty-themes GitHub release matching that tag (curl -L; the URL
pattern is in the README provenance or the oracle's build.zig.zon), verify its integrity
hash against the oracle pin if the .zon records one, replace the corpus, update the README
provenance, and re-run the themes gates: .port/tools/gates named G-THEMES or w9-themes
(grep .port/tools/ci.sh for THEMES to find the runner) — the theme COUNT may change from
592; if it does, update the gate's expected number with the measurement recorded in the
gate file comment (tighten-only, cite the release tag). Also re-run
cargo test -p rustty-extra (list-themes goldens may reference theme names) and
cargo test -p rustty-config --lib theme. If the network is unavailable, STOP and report
BLOCKED in caveats — do not fake the corpus.`,
  },
]

phase('Port')
const results = await parallel(TASKS.map(t => () =>
  agent(COMMON + '\nYOUR TASK: ' + t.prompt,
    { label: t.label, phase: 'Port', schema: SCHEMA, isolation: 'worktree' })
    .then(r => r ? { ...r, _label: t.label } : null)
))

return { tasks: results.filter(Boolean) }
