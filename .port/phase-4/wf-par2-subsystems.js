export const meta = {
  name: 'p4-par2-subsystems',
  description: 'Phase-4 parallel tier 2: kitty, formatter, input, modes tail, termio pool, renderer cell, notifications, macOS, font',
  phases: [{ title: 'Port', detail: 'worktree-isolated, patch harvest; disjoint from in-flight vt-core agents' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
You are a Phase-4 porter for the rustty port (full-fidelity Zig->Rust port of ghostty).
You run in an ISOLATED git worktree of ${REPO} at current main HEAD (verify with git log --oneline -1:
it must be 3d6fd16e or newer; if the worktree is stale, run: git reset --hard main) with its own build dir.
Oracle (read-only, AT target pin 12967b68f): ${REF}; its working-tree files ARE the end state.
Range: 6ad1fe7d8..12967b68f. Intermediates: git -C ${REF} show <hash>:<path>.

READ FIRST: your assessment file in ${REPO}/.port/phase-4/assessments/ (named per task) — its
rows are the spec; ${REPO}/docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md Global
Constraints; ${REPO}/.port/PORT_RULES.md.

BINDING: R1 (every line/comment/test assertion-for-assertion; END STATE only, intermediates'
tests carried); R2 per-token renames; R7 arithmetic (saturating/checked + debug_assert!); R9
SAFETY tags; DG5 extern "C"; SB/UB raw-ptr discipline for Page/Row/Cell/Screen params (memory
rust-mut-args-strongly-protected) — where upstream threads s:*Screen through an API whose
receiver lives INSIDE Screen, the param MUST be a raw pointer, never &mut Screen; prove
restructured aliasing on an isolated Miri harness. Match surrounding rustty idiom.

VERIFY: cargo build + cargo test --lib green (0 failed) for every crate touched; run targeted
filters while iterating and full crate lanes at the end.

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
    label: 'w3-kitty',
    prompt: `PATCHNAME w3-kitty. Assessment D clusters 1+2 (kitty pin-lifecycle + pending payloads;
kitty security hardening) [.port/phase-4/assessments/D-terminal-subsystems.md] + assessment E
"kitty-pending-images renderer side" rows that are pure rustty-vt (leave rustty-renderer's
image.zig isPending arm OUT if it depends on unported renderer state — note it instead).
Port the END STATE of ${REF}/src/terminal/kitty/graphics_{storage,exec,image}.zig as ONE unit
(~2,100 lines churn, ~23 new tests): pin lifecycle, pending payloads (ImageData pending union,
data.bytes()), infallible setLimit (its terminal.rs caller: rustty-vt/src/terminal.rs:1550
still handles a Result — update the caller), prune reclaim, pin release; and the security
series: TOCTOU open-first path validation via fcntl(F_GETPATH) on macOS / /proc/self/fd on
Linux (af2fa311's ordering BEFORE ec04900ab), /tmpX prefix-boundary fix, shm range validation,
png cap via rustty_datastruct::limited_allocator (landed on main), saturating geometry (R7).
Keep collect-keys-first where upstream removes-while-iterating. s:*Screen params: RAW pointers
following delete()'s existing shape (W0 already fixed the range-delete && bug — do not undo it).
NOTE the amended-assertion class: several fixes amend EXISTING tests — port the amended bodies.
G1b kitty drop=11 must close: run python3 .port/tools/gates/check_test_counts.py 2>/dev/null
| grep -E "kitty" at the end and report the row.`,
  },
  {
    label: 'w3-formatter',
    prompt: `PATCHNAME w3-formatter. Assessment D cluster 3 (formatter end-state bundle, 8 ABSENT,
~1,330 lines) + assessment A cluster A-4's formatter halves (pin-map redesign; fastprint landed
on main — wire it if the end state calls it). Port the END STATE of
${REF}/src/terminal/formatter.zig (+ its StringMap coupling ${REF}/src/terminal/StringMap.zig ->
rustty-vt/src/string_map.rs if that is where PinMap lives — follow the oracle):
pin-map RLE redesign (PinMap.Map replacing Vec<Pin> in StringMap), tabstops-before-screen +
cursor-home (e523cf810), pending-wrap cursor restore (997a2aff2 — ALSO DELETE the first-pass
extra.screen emission, or output doubles: the deletion is part of the port), html
trailing-newline fix, buffer-assembled VT/CSS style emission (2ed67cadd formatter half only).
DO NOT touch stream.rs / stream_continuation / apc / pagelist / ref_counted_set (other agents
own them). G1b formatter drop=2 must close; report the formatter row from
python3 .port/tools/gates/check_test_counts.py | grep formatter.`,
  },
  {
    label: 'w3-input',
    prompt: `PATCHNAME w3-input. Assessment D cluster 5 (input Binding/command/key_encode, 10 ABSENT)
+ the two config-side holdovers par-1 deferred (assessment G notes them compile-coupled):
(1) rustty-input: release-fallback guard at key_encode.rs:281 region (bd647035e; G1b
input_key_encode drop=3 — carry the 3 tests); 8cfbaf545+b67f8ef51 un-escape + round-trip pair
(R2: the test string 👻Ghostty becomes 👻Rustty); the three new Binding.Action variants
(prompt_window_title, set_window_title, move_tab_to_new_window) with their parse/format arms;
0a183c923 OpenConfig payload union (Action::OpenConfig void -> payload .default) INCLUDING its
config-side default-keybind hunk in rustty-config (binding.rs void_arm sites named by par-1's
caveat); df23bef0e translated()/N_ machinery on command.zig -> rustty-input/src/command.rs
INCLUDING rustty-config's cmd-translation loop (value_types.rs ~5971 extend_from_slice of
defaults()/defaults_c() -> the end-state translated() loop). The C-ABI halves of the new action
variants are a LATER bundle — vt-side/config-side only here; keep rustty-apprt-api compiling
(if an exhaustive match on Action breaks, add the minimal arms with a PORT NOTE citing the W5/W7
bundle). G1b input_key_encode drop=3 must close; report the row.`,
  },
  {
    label: 'w3-modes-tail',
    prompt: `PATCHNAME w3-modes-tail. Assessment D clusters 6+7 minus what other agents own:
(1) modes DECECM + configurable defaults (4 ABSENT): get_report 117/permanently-reset arm
(G1b modes drop=1), cfc19e805 setDefault/default_configurable flags on rustty-vt/src/modes.rs.
(2) color/dcs/hyperlink/KV tail: f719af00c dense KV table replacing HashMap<u8,u32> + arena
removal (kitty KV — coordinate: the kitty agent owns graphics_*.zig; the KV table lives in
${REF}/src/terminal/kitty/key_value... check the actual file; if it IS in graphics_*.zig, SKIP
it and say so — the kitty agent owns it); f973bd53b overline printAttributes half that lives in
terminal.rs:5375 + the AMENDED dcs test body; hyperlink zero-len free guards; remaining dcs/
color rows per the assessment (7cd2f65f5 landed in W0 — do not re-port).
DO NOT touch stream*/apc/pagelist/formatter/kitty graphics files (other agents own them).
G1b modes drop=1 must close; report the row.`,
  },
  {
    label: 'w6-termiopool',
    prompt: `PATCHNAME w6-termiopool. Assessment E cluster "termio-infra" + assessment G's B3 bundle:
e0ef934f7 SegmentedPool -> MemoryPool rewrite in rustty-termio (pooled Write{td,req,buf} with
intrusive xev.WriteRequest raw back-pointers) ATOMIC with DELETING
crates/rustty-datastruct/src/segmented_pool.rs (+ its pub mod / re-exports) in the same patch.
SB DISCIPLINE (binding): pooled nodes' ADDRESSES are held by the event loop across completion —
stable addresses required; raw-ptr handoff, NO &mut re-borrow across completion. Read the
existing termio Write pool code first (crates/rustty-termio/src/...) and the intrusive-ptr
doctrine (memory intrusive-ptr-and-placeholder-fill; rustty-zigstd MemoryPool exists — check
crates/rustty-zigstd). Add/extend LIFETIMES rows: append to .port/audits/GHOSTTY_LIFETIMES.tsv
if present (check its column format first). If you restructure aliasing, prove it on an
isolated non-FFI Miri harness at .port/phase-4/mirihar-termio-pool (copy the shape of
.port/phase-2/mirihar-* harnesses) and run cargo +nightly miri run there if nightly+miri are
available (check with cargo +nightly miri --version; if unavailable, say so in caveats and keep
the harness committed for the finale). Also the small termio rows the cluster names: apcEnd
.unknown arm ONLY IF it does not require the in-flight stream-continuation work (check whether
rustty-vt on main already has ApcEnd/Unknown — if not, SKIP with a note; the W2 stream agent
owns that signature), and the comment-only row. cargo test -p rustty-termio --lib green.`,
  },
  {
    label: 'w6-rendercell-notif',
    prompt: `PATCHNAME w6-rendercell-notif. TWO bundles from assessment E (+F coupling):
(1) renderer-cell-storage (cde7f9343; G1b renderer_cell drop=1): fg_rows drops
ArrayListCollection for owned []CellTextRow + in-place bg realloc in rustty-renderer's cell
storage (both metal and linux paths if both exist — grep ArrayListCollection usage), ATOMIC
with DELETING crates/rustty-datastruct/src/array_list_collection.rs (+ pub mod lines) in the
same patch. Carry the new 'Contents resize grows and shrinks' test.
(2) desktop-notification truncation END STATE (assessment E cluster + F §2b, G1b
apprt_api_surface drop=4): UTF-8-boundary truncation moves INTO
apprt.surface.Message.DesktopNotification.init (rustty-apprt-api or rustty-apprt — find
Message::DesktopNotification; the assessment says rustty-termio/src/stream_handler.rs:2194 is
the old truncation site — stream_handler now only forwards; DELETE the old site's truncation)
+ the 4 new src/apprt/surface.zig tests (DesktopNotification init + 3 copyUtf8Z). Report the
apprt_api_surface and renderer_cell G1b rows at the end.`,
  },
  {
    label: 'w7-macos',
    prompt: `PATCHNAME w7-macos. Assessment F cluster "macOS Swift wave" [.port/phase-4/assessments/
F-ui-tiers.md] — 44 ABSENT commits, ~3,338 lines, in rustty's macos/ Swift tree. Port the END
STATE: clipboard-confirmation rework (B1), untrusted OSC-8 URL policy (B2: UntrustedURL.swift +
UntrustedURLAlert.swift NEW — note the Kind::osc8 producer is a later Rust bundle; make the
Swift side compile standalone with the policy engaged for whatever URL-open path exists today,
per the assessment's atomicity note — if the Swift side CANNOT function without the C-header
kind, port the files + tests and wire the call site behind the existing path, recording the
seam in caveats), iOS/CrossKit removal (B3: DELETE iOSApp.swift, SurfaceView_UIKit.swift,
AnySortKey.swift, CrossKit.swift, HostingWindow.swift — CrossKit deletion is ATOMIC with the
OSColor->NSColor sweep), update-flow (B4), CommandLineOpenFileFilter.swift NEW + tests,
command-palette sort-key simplification (5c952ac97) + CommandPaletteTests, IME/insertText
commits as key events (ecbeea92d/ecbeb60ca — check hash in the assessment), f4309055f 0x7F fix
at SurfaceView_AppKit.swift:1468 (codepoint >= 0x20 misses 0x7F), the ~15 crash/leak/warning
fixes, 2349e974b inspector detachable-header helper removal + d19f8f7f9 hosting-window helper
removal. Carry UntrustedURLTests(+122), CommandLineOpenFileFilterTests(+85),
CommandPaletteTests end state. R2 renames throughout (Ghostty->Rustty tokens, RusttyKit).
VERIFY: xcodebuild build first; then xcodebuild test -project macos/Rustty.xcodeproj -scheme
Rustty (or the scheme that exists — list schemes with xcodebuild -list). If specific pre-
existing tests fail unrelated to your change, report them but ensure YOUR test targets pass.
This is Swift — no cargo.`,
  },
  {
    label: 'w7-font',
    prompt: `PATCHNAME w7-font. Assessment F cluster "font + pkg/macos" — 6 ABSENT, ~350 lines:
shaper grapheme-candidate rewrite (ca8868a29; G1b font_shaper_harfbuzz drop=1 — if its test
needs A's RenderState/vtStream scaffolding not yet on main, port the code now and the test in
the best form available: if truly blocked, add the test #[ignore = "P4: needs W2 RenderState
scaffolding"] with the full body so the finale un-ignores it, and say so), Apple Color Emoji
exact lookup, CoreText warmup (c454a3bf4 font-side Discover.warmup — the App-side spawn is
another wave), CT null/OOM safety (daeed25b3 coretext sibling hunks), display-name fallback,
and a177ba90a display-link error-type half in pkg/macos land: rustty's
crates/rustty-renderer/src/metal/cvdisplaylink.rs:148-156 carries a PORT NOTE deliberately
preserving the OutOfMemory mis-mapping upstream fixed — port the fix and DELETE that note
(do not amend it; the Phase-3 metal/buffer precedent). Crates: rustty-font*, and the
cvdisplaylink file in rustty-renderer (disjoint from other agents' renderer cell work — verify
no overlap by keeping to cvdisplaylink.rs only in that crate). cargo test for each touched
crate green.`,
  },
]

phase('Port')
const results = await parallel(TASKS.map(t => () =>
  agent(COMMON + '\nYOUR TASK: ' + t.prompt,
    { label: t.label, phase: 'Port', schema: SCHEMA, isolation: 'worktree' })
    .then(r => r ? { ...r, _label: t.label } : null)
))

return { tasks: results.filter(Boolean) }
