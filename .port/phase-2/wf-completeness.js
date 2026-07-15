export const meta = {
  name: 'completeness-fill',
  description: 'Port z2d/wuffs seams, vxfw+boo, WASM target; reconcile ledger (WS9) — parallel design then worktree-isolated parallel ports, existing porting methodology (R1 fidelity + comments, R4 acyclic, panic=abort, Miri/ASan proofs)',
  phases: [
    { title: 'Design', detail: 'read-only oracle scouts produce a port spec per workstream' },
    { title: 'Port', detail: 'worktree-isolated fidelity port + self-verify (build/test/Miri|ASan) + commit' },
  ],
}

// ---- shared porting doctrine handed to every agent (the repo's methodology) ----
const DOCTRINE = `PORT METHODOLOGY (non-negotiable):
- R1 FIDELITY: port every reference line AND every comment. Preserve reference comments as
  Rust comments (Zig byte-slices -> Vec<u8>/&[u8], never lossy String). Provenance: a
  \`PORTS: <oracle path>\` doc-comment at the top of each ported file.
- R4 ACYCLICITY: never introduce a crate cycle. If a seam needs a type from a higher tier,
  route via an accessor trait defined in the lower crate (see prior renderer/inspector precedent).
- panic=abort semantics: asserts abort; recoverable errors return a sentinel/Result — do NOT
  fake behavior (R5). Do not leave NEW \`unimplemented!()\`/\`todo!()\` unless the reference body
  is genuinely unreachable-by-construction AND you preserve the verbatim reference body in a
  comment + a precise justification.
- HAZARDS: any long-lived raw alias into a field interleaved with &mut to that field is SB/TB UB.
  PROVE memory-safety on an isolated Miri harness under .port/phase-2/mirihar-<key>-* (NOT by
  hand-reasoning). FFI paths (Miri can't execute foreign fns) are proven under ASan instead.
- ACCEPTANCE = builds green + reference tests ported assertion-for-assertion & passing +
  Miri/ASan clean on the new hazardous paths. A count regression in ported tests is a failure.
- The oracle at /Users/gael/projects/ghostty and the vendored deps under ~/.cache/zig/p are
  READ-ONLY. Match the existing rustty crate's conventions (look at sibling files first).`

const SPEC_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['workstream', 'oracle_files', 'port_targets', 'work_items', 'hazards', 'acceptance', 'risks'],
  properties: {
    workstream: { type: 'string' },
    oracle_files: { type: 'array', items: { type: 'string' }, description: 'exact oracle paths to port from, with LOC' },
    port_targets: { type: 'array', items: { type: 'string' }, description: 'exact rustty files to create/modify' },
    work_items: { type: 'array', items: { type: 'string' }, description: 'ordered concrete port steps (seam-by-seam / file-by-file)' },
    hazards: { type: 'array', items: { type: 'string' }, description: 'SB/TB or FFI hazards + how to prove (mirihar / ASan)' },
    acceptance: { type: 'array', items: { type: 'string' }, description: 'exact verification commands + expected outcomes' },
    risks: { type: 'string', description: 'anything that could force a BLOCKED or a faithful-but-safe divergence needing a decision' },
  },
}

const PORT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['status', 'branch', 'summary', 'tests', 'miri_asan', 'files_changed', 'concerns'],
  properties: {
    status: { type: 'string', enum: ['DONE', 'DONE_WITH_CONCERNS', 'BLOCKED'] },
    branch: { type: 'string', description: 'git branch --show-current in your worktree (the branch to merge)' },
    summary: { type: 'string' },
    tests: { type: 'string', description: 'exact test command run + pass count (e.g. "cargo test -p rustty-wuffs: 12 passed"); note any ported reference test count' },
    miri_asan: { type: 'string', description: 'the Miri/ASan harness path + result, or "n/a — pure logic, no raw-alias hazard" with justification' },
    files_changed: { type: 'array', items: { type: 'string' } },
    concerns: { type: 'string', description: 'anything a reviewer must scrutinize; empty string if none' },
  },
}

const WS = [
  {
    key: 'wuffs',
    model: 'claude-sonnet-5',
    title: 'wuffs pixel-swizzle seams (6)',
    design: `Scope the port of ghostty's wuffs pixel-swizzlers to fill the 6 \`unimplemented!()\` seams in
\`crates/rustty-wuffs/src/swizzle.rs\` (gToRgba, gaToRgba, rgbToRgba, bgrToRgba, bgraToRgba + the internal \`swizzle\` helper).
Oracle: /Users/gael/projects/ghostty/pkg/wuffs/src/swizzle.zig (read it fully) — it calls the wuffs C
\`wuffs_base__pixel_swizzler\` API. Inspect crates/rustty-wuffs (it already vendors/links wuffs C for decode:
find the existing C bindgen/FFI surface in crates-sys or the crate's build.rs and c.rs) to determine exactly how
to reach the swizzler symbols + pixel-format enum constants. Determine whether the swizzler API is already exposed
by the current wuffs C build or needs an added header/symbol. Produce the seam-by-seam port plan + the byte-exact
unit tests (each format conversion on a known input) + the ASan proof plan (FFI -> Miri can't).`,
    port: `Fill the 6 wuffs swizzle seams in crates/rustty-wuffs/src/swizzle.rs by porting
/Users/gael/projects/ghostty/pkg/wuffs/src/swizzle.zig VERBATIM (every line + comment), binding the wuffs C
\`wuffs_base__pixel_swizzler\` API through the crate's existing wuffs C FFI. Replace each \`unimplemented!()\` with
the real ported body (keep the reference body that is currently in the comment as the guide). Add byte-exact unit
tests for gray/gray_alpha/rgb/bgr/bgra -> rgba on known inputs. Verify: \`cargo build -p rustty-wuffs\` +
\`cargo test -p rustty-wuffs\`. Because the swizzle crosses C FFI, prove memory-safety under ASan
(RUSTFLAGS=-Zsanitizer=address on nightly, or the repo's existing ASan harness pattern) — NOT Miri. If the wuffs C
build does not currently export the swizzler symbols, add the minimal binding to the -sys crate to expose them.`,
  },
  {
    key: 'z2d',
    title: 'z2d rasterizer seams (9): AA supersample fill + dashed tessellation + out-of-subset formats',
    design: `Scope filling the 9 z2d \`unimplemented!()\` seams (crates/rustty-z2d/src/: painter.rs:128 anti-aliased
supersample fill, painter.rs:322, surface.rs:355 & :392 out-of-subset surface formats, internal/tess/polyline_plotter.rs:47
dashed polyline tessellation, internal/tess/stroke_plotter.rs:46 dashed stroke tessellation, and the related dasher.rs /
internal/raster/hairline.rs subset-guards). Oracle: the vendored z2d-0.10.0 at
~/.cache/zig/p/z2d-0.10.0-j5P_Hu-6FgBsZNgwphIqh17jDnj8_yPtD8yzjO6PpHRQ/src/ — read internal/raster/{supersample,multisample}.zig,
internal/tess/{dashed_plotter,Dasher,stroke_plotter,polyline_plotter}.zig, painter.zig, surface.zig, options.zig. Map each
rustty seam to its oracle body. Determine which z2d reference TESTS exercise these paths (supersample/dashed/format) and
must come along. Note SB hazards in the tessellation buffer handling. Produce the seam-by-seam plan + test port list + Miri plan.`,
    port: `Fill the 9 z2d rasterizer seams in crates/rustty-z2d/src/ by porting the corresponding bodies VERBATIM (every
line + comment) from the vendored z2d-0.10.0 oracle
(~/.cache/zig/p/z2d-0.10.0-j5P_Hu-6FgBsZNgwphIqh17jDnj8_yPtD8yzjO6PpHRQ/src/): anti-aliased supersample fill
(internal/raster/{supersample,multisample}.zig + painter.zig), dashed polyline & stroke tessellation
(internal/tess/{dashed_plotter,Dasher,stroke_plotter,polyline_plotter}.zig), and the out-of-subset surface formats
(surface.zig). Bring along the z2d reference tests that exercise these paths, assertion-for-assertion. Verify:
\`cargo build -p rustty-z2d\` + \`cargo test -p rustty-z2d\` (no regression in existing count). Prove the new pure-compute
raster/tess paths Miri-clean on an isolated harness under .port/phase-2/mirihar-z2d-* (buffer aliasing in the plotters is
the risk). If a seam is genuinely unreachable by the sole consumer (font sprite) even after this fill, keep it a seam but
document why with the verbatim body preserved.`,
  },
  {
    key: 'vxfw',
    title: 'vaxis vxfw subset + boo easter-egg + framedata asset',
    design: `Scope porting the vaxis \`vxfw\` widget-framework SUBSET that ghostty's \`boo\` easter-egg needs, then un-deferring
boo. boo.rs (crates/rustty-cli/src/boo.rs) currently preserves the full reference verbatim as a commented \`unimplemented!()\`
leaf (DEFERRALS.md "cli C5"). Oracle: ghostty src/cli/boo.zig(236) + the vendored vaxis-0.5.1 vxfw at
~/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS/src/vxfw/ — boo uses ONLY: vxfw.zig(578, the core:
Widget/EventContext/DrawContext/Surface/SubSurface/Event/Text/Center re-exports), App.zig(602 run loop), Text.zig(508),
Center.zig(113). The other vxfw widgets are UNUSED by ghostty -> OUT OF SCOPE (YAGNI — port only what the consumer uses).
Also scope the \`framedata\` compressed asset: ghostty generates it via src/build/GhosttyFrameData.zig(75) over
src/build/framegen/frames/ (235 frame files) — determine the compression format and how to reproduce it as a rustty build.rs
or vendored asset (byte-faithful). Landing: new module crates/rustty-vaxis/src/vxfw/ (rustty-vaxis already has Tty/Vaxis/Loop
which vxfw.App drives). Identify the SB/TB hazards in vxfw's type-erased Widget pointers + arena DrawContext (a mirihar proof
will be required, like prior vaxis chunks). Produce the file-by-file port plan + framedata plan + Miri harness plan.`,
    port: `Port the vaxis \`vxfw\` SUBSET that boo needs, then un-defer boo. Steps: (1) Create crates/rustty-vaxis/src/vxfw/
porting VERBATIM (every line + comment) ONLY the files boo uses from
~/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS/src/vxfw/: vxfw.zig (core Widget/EventContext/
DrawContext/Surface/SubSurface/Event + Text/Center re-exports), App.zig (the run loop — drive it off rustty-vaxis's existing
Tty/Vaxis/Loop), Text.zig, Center.zig. Do NOT port unused widgets. (2) Reproduce the \`framedata\` compressed asset byte-faithfully
(port src/build/GhosttyFrameData.zig's compression over src/build/framegen/frames/ as a build.rs or vendor the exact compressed
bytes). (3) Un-defer crates/rustty-cli/src/boo.rs: replace the \`unimplemented!()\` leaf with the real port of ghostty
src/cli/boo.zig (the commented reference body is your guide — port it live, decompress framedata, run the vxfw.App loop).
Wire boo back into the CLI action dispatch if it was stubbed there. Verify: \`cargo build -p rustty-vaxis -p rustty-cli\` +
\`cargo test -p rustty-vaxis -p rustty-cli\` + port any vxfw reference tests. The animation is interactive (not visually
verifiable headlessly — like the other TUI paths): verification is build + unit tests + a Miri-clean proof of the vxfw
type-erased-Widget / arena-DrawContext aliasing on an isolated harness under .port/phase-2/mirihar-vxfw-*.`,
  },
  {
    key: 'wasm',
    title: 'WASM/web target (headless VT+font+config C-ABI lib)',
    design: `Scope completing ghostty's WASM/web target in rustty and proving it builds for wasm32. Ghostty's wasm module
(src/main_wasm.zig) is a HEADLESS C-ABI library (NOT the GUI app): it \`comptime\`-imports os/wasm.zig, font/main.zig,
terminal/main.zig, config.zig.Wasm, App.zig.Wasm. Oracle files to read fully: src/main_wasm.zig, src/os/wasm.zig(25) +
os/wasm/{log,target}.zig, src/config/Wasm.zig(54), src/App.zig (the \`Wasm\` section ~line 578-end), src/font/shaper/web_canvas.zig(305,
the browser-Canvas font shaper backend using extern JS fns), src/build/wasm_patch_growable_table.zig(the post-build wasm binary
patcher, 7 tests). ALREADY PORTED in rustty (audit their completeness): crates/rustty-os/src/wasm.rs + wasm/{log,target}.rs,
crates/rustty-config/src/wasm.rs. GAPS to determine: the \`web_canvas\` font-shaper backend (crates/rustty-font-shaper — how is the
backend selected? add a wasm/web_canvas backend behind cfg(target_arch="wasm32")), the App.Wasm C-ABI surface, and the wasm
MODULE ENTRY (Rust equivalent of main_wasm.zig = a cdylib crate-type or a wasm lib target that re-exports the C-ABI). Decide the
crate structure: a new thin \`rustty-wasm\` cdylib crate that pulls the pieces together, or a wasm feature/target on an existing
crate. The browser Canvas FFI = \`extern "C"\` imports (JS-provided) — declare them, they need not run here. Acceptance is
\`cargo build --target wasm32-unknown-unknown\` of the wasm surface + unit tests for the pure logic (config load/finalize,
allocator). Produce the crate-structure decision + file plan + the exact wasm32 build command. Flag clearly if the font-shaper
backend wiring is too invasive for a clean isolated port (then the port stage will report BLOCKED with a precise follow-up).`,
    port: `Complete the WASM/web target and prove it builds for wasm32-unknown-unknown. Following the design spec: (1) audit and
finish the already-started ports (crates/rustty-os/src/wasm.rs, crates/rustty-config/src/wasm.rs) against their oracles
(src/os/wasm.zig, src/config/Wasm.zig) — fill any gaps VERBATIM. (2) Port src/font/shaper/web_canvas.zig(305) as a wasm
font-shaper backend in crates/rustty-font-shaper behind cfg(target_arch="wasm32"), declaring the browser-Canvas extern imports as
\`extern "C"\`. (3) Port the App.Wasm C-ABI section from src/App.zig and the wasm module entry (Rust equivalent of src/main_wasm.zig)
as the crate structure the design chose (prefer a thin \`rustty-wasm\` cdylib crate if a new crate is needed — add it to the
workspace root Cargo.toml). (4) Port src/build/wasm_patch_growable_table.zig with its 7 tests. Preserve every line + comment (R1).
Verify: \`cargo build --target wasm32-unknown-unknown\` succeeds for the wasm surface, AND \`cargo test\` (host) passes for the
pure-logic units (config, allocator, wasm_patch). Do NOT attempt browser execution. If the font-shaper backend wiring proves too
invasive to complete cleanly in isolation, commit what builds and report status BLOCKED with the exact remaining wiring + why.`,
  },
  {
    key: 'ws9',
    model: 'claude-sonnet-5',
    title: 'WS9 ledger hygiene — reconcile DEFERRALS.md drift + strip stale status comments',
    design: `Scope the WS9 ledger-hygiene reconciliation. Read .port/phase-2/DEFERRALS.md and .port/phase-2/GAP_ANALYSIS.md
section 6 ("Untracked gaps & ledger drift"). Enumerate: (a) real deferrals with NO ledger entry that GAP_ANALYSIS names —
src/synthetic, benchmark, src/extra, mdgen/webgen, stb_image_resize, the ghostty_*->rustty_* alias layer, the .xcframework
repackaging; (b) app-core over-claims to correct (main_gen/main_wasm/main_build_data marked done but ledger-deferred); (c) stale
port-status header comments in crates/rustty-vt/src/terminal.rs & screen.rs (and anywhere else) that FALSELY assert skeletons/
todo!() that no longer exist. Verify each claim against current code before listing it (grep the actual files — do not trust the
5-day-old gap doc blindly). Produce the exact edit list: which DEFERRALS.md entries to add/mark-resolved, which comments to strip,
with file:line anchors.`,
    port: `Execute the WS9 ledger hygiene from the design spec. Edit .port/phase-2/DEFERRALS.md to add entries for the untracked
deferrals (synthetic, benchmark, src/extra, mdgen/webgen, stb_image_resize, ghostty_*->rustty_* alias layer, .xcframework
repackaging) and correct the app-core over-claims — each entry must be accurate against current code (verify by grep before
writing). Strip the stale port-status header comments in crates/rustty-vt/src/terminal.rs, screen.rs, and any other file that
falsely asserts a skeleton/todo!() that no longer exists (verify the todo!() is actually gone first). These are COMMENT/DOC edits
only — do not change any logic. Verify: \`cargo build --workspace\` stays green (comment-only), and a final grep shows no remaining
false "skeleton"/"placeholder"/"todo!()" status claims in the files you touched. NOTE: four other deferrals (cli C5 boo, wuffs
swizzle, z2d subset seams, wasm target) are being resolved in PARALLEL in other worktrees right now — do NOT mark those resolved
(the controller reconciles them after merge); focus only on the PRE-EXISTING drift above.`,
  },
]

// ---------- run: parallel design (read-only) -> worktree-isolated parallel port ----------
phase('Design')
log(`Designing ${WS.length} workstreams in parallel (read-only oracle scouts)`)

const results = await pipeline(
  WS,
  // Stage 1: design scout (read-only, main tree) — also writes a durable spec file.
  (ws) => agent(
    `${DOCTRINE}\n\n## Workstream: ${ws.title}\n\n${ws.design}\n\nAlso WRITE your full spec to ` +
    `\`.port/phase-2/specs/completeness-${ws.key}.md\` (durable record). Return the structured spec.`,
    { label: `design:${ws.key}`, phase: 'Design', schema: SPEC_SCHEMA, model: ws.model, effort: 'high' },
  ),
  // Stage 2: port (worktree-isolated, own target dir -> no cargo race, clean merge) + self-verify + commit.
  (spec, ws) => agent(
    `${DOCTRINE}\n\n## Workstream: ${ws.title}\n\nYou are in an ISOLATED git worktree branched from the ` +
    `\`completeness/fill\` integration branch. Do the port here and COMMIT it (so the branch can be merged back).\n\n` +
    `### Design spec (from the scout):\n${JSON.stringify(spec, null, 2)}\n\n### Your task:\n${ws.port}\n\n` +
    `When done: run the acceptance commands, then \`git add -A && git commit\` with a clear message ` +
    `(subject: "rustty: <what> (completeness-fill: ${ws.key})"). Report your branch via \`git branch --show-current\`. ` +
    `If genuinely blocked, commit whatever builds and report BLOCKED with the precise remaining work.`,
    { label: `port:${ws.key}`, phase: 'Port', schema: PORT_SCHEMA, model: ws.model, effort: 'high', isolation: 'worktree' },
  ).then((r) => ({ ...r, key: ws.key })),
)

return results.filter(Boolean)
