export const meta = {
  name: 'p4-par4-snapshot2-capi',
  description: 'Phase-4 parallel tier 4: snapshot grid/screen/terminal + codec, C-API bundles T then R/M/K, embed-header bundle E',
  phases: [{ title: 'Port', detail: 'wsnap-2 ∥ (w5-T → w5-RMK pipeline) ∥ w5-E' }],
}

const REPO = '/Users/gael/projects/rustty'
const REF = '/Users/gael/projects/ghostty'
const PATCHDIR = '/Users/gael/.claude/jobs/b740b914/tmp'

const COMMON = `
You are a Phase-4 porter for the rustty port (full-fidelity Zig->Rust port of ghostty).
You run in an ISOLATED git worktree of ${REPO}; FIRST ACTION: git log --oneline -1 — if not
bd157dee or newer, run: git reset --hard main. Own build dir; edit freely.
Oracle (read-only, AT target pin 12967b68f): ${REF}; working-tree files ARE the end state.
Range: 6ad1fe7d8..12967b68f. Intermediates: git -C ${REF} show <hash>:<path>.

READ FIRST: your assessment in ${REPO}/.port/phase-4/assessments/ (B-snapshot-io.md or
C-capi-lib-wasm.md per task); the plan docs/superpowers/plans/2026-08-17-phase4-upstream-resync.md;
.port/PORT_RULES.md. ALREADY LANDED on main (build on it, never revert): W0/W1/par-1/par-2/
par-3 + W2 (see git log --oneline -12) — notably snapshot scaffold+records live at
crates/rustty-vt/src/snapshot/ (io/envelope/record/checkpoint/continuation/page/style/
hyperlink; codec.rs documents the deferred drivers), PageList Builder/allocate_page,
stream_continuation, crc32c, kitty end state, render-state core side, build_options Features.

BINDING: R1 (every line/comment/test assertion-for-assertion; END STATE only); R2 renames;
R7 arithmetic; R9 SAFETY tags; DG5 extern "C" abort; closed-enum rule (inbound C integers
narrow via match; check_closed_enum_abi{,_runtime}.sh stay green); SB/UB raw-ptr discipline —
decoder/iterator objects holding Terminal/Screen aliases across C calls are the named UB
shape; prove restructured aliasing under Miri. Match surrounding idiom.

VERIFY: cargo build + cargo test --lib green per touched crate; the vt lane ~2min. For C-API
work also run the header gates you touch: bash .port/tools/gates/check_rustty_vt_headers.sh
(expect arm (d) missing-count to DROP with your exports; do not weaken any arm) and
.port/tools/check-vt-struct-layouts.mjs via its usual runner if layout rows change.

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
  // Lane 1: snapshot back half (heavy, standalone)
  () => agent(COMMON + `
YOUR TASK (PATCHNAME wsnap-2): assessment B clusters C3+C4 (+C8 benches if reachable) — the
snapshot GRID codec + SCREEN/TERMINAL/HISTORY records + the CODEC DRIVERS, completing
crates/rustty-vt/src/snapshot/. Port the END STATE of ${REF}/src/terminal/snapshot/
{grid,screen,terminal,history,fixture}.zig and the codec drivers in snapshot.zig
(EncodeError/DecodeError/DecodeExactError, encode, decode, decodeExact, Decoded, Decoder=
init/ready/next — codec.rs on main documents the deferred surface; make it live), plus
snapshot/main re-export lines going live, ALL remaining testdata hex goldens bit-for-bit,
and the .ksy spec if not landed. Grid codec: the FINAL format only (8-byte cells, blank
elision, variable width, vectorized enc/dec 2aaad3ca9/973f619a2, grapheme suffix batching
593762cfa, wide pairs per-row 7c1014ef6, skip-remap-tables 47a518262, single-pass style
1359973ae). Screen/terminal: b867a0f59's snapshot-screen half; kitty 1-line hunks; history
records; 9be6c2ea2 retain-continuations decode option. decode-into-Page/Screen raw-ptr
discipline (the scaffold's transactional release-on-error pattern is the precedent); run
Miri on the snapshot test filter at the end (cargo +nightly miri test -p rustty-vt --lib
snapshot -- --test-threads=1; if some tests are Miri-intractable, follow
.port/phase-2/MIRI_EXCEPTIONS.md routing and say so). Then C8 if time/context allows:
src/benchmark/{TerminalSnapshot,TerminalFormatter,TerminalResize}.zig -> rustty-bench with
action registrations (G-BENCH parity gate: bash .port/tools/gates/check_bench_tests.sh must
stay green — it is oracle-driven and will REQUIRE the new actions; if you skip C8, run the
gate and report its state honestly). Every upstream test assertion-for-assertion. Full vt
lane green.`,
    { label: 'wsnap-2', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),

  // Lane 2: C-API pipeline — T then R/M/K (same files, sequential)
  () => agent(COMMON + `
YOUR TASK (PATCHNAME w5-T): assessment C **Bundle T — ABI type metadata** (7 ABSENT, ~2,630
lines): types.zig registry 45 -> 135 entries w/ enums/unions/opaques/aliases (309440e07 all
public structs, e4ec4f0f9 enum underlying-type detection, 9673a22b0 expanded metadata) in
crates/rustty-vt-c/src/types.rs; c75559589 ABI manifest: port src/terminal/c/types.schema.json
+ types-schema-verify.py into the rustty tree (crates/rustty-vt-c/ next to types.rs; R2-rename
keys per parity_names.env conventions) and register the verify step as a PROGRESS gate row in
.port/tools/ci.sh (additive, with its FAIL arm provable — a deliberately broken local schema
copy must fail; record the negative control in the gate comment); 0e8b7bea6 packed.zig
GhosttyPackedCell -> the C-visible packed-cell layout (core half landed on main — wire the C
surface + include/rustty/vt header additions); d930c74c4 sized-init valid C++ + 51992ab01
self-contained device/point headers + types.h underlying-type fixes in include/rustty/vt/;
RECONCILIATION O3: sgr.zig Attribute.C = lib.CUnion options w/ 7 field_renames (fg_256 etc.)
— the rustty-lib CUnion machinery may need the options-struct extension (rustty-lib enum
stable values landed in W1); O12 Doxyfile HTML_EXTRA_FILES rider. e77c2612e is DONE (W1).
Every touched header must keep A5's existing arms green (run the gate; the layout arm may
gain rows — update type_json accordingly, never shrink). Return your patch; the R/M/K agent
runs AFTER you and builds on your worktree state — in your final message include the path of
your WORKTREE so the orchestrator can chain (also harvest the patch normally).`,
    { label: 'w5-T', phase: 'Port', schema: SCHEMA, isolation: 'worktree' })
    .then(async t => {
      if (!t) return [null, null]
      const rmk = await agent(COMMON + `
YOUR TASK (PATCHNAME w5-RMK): assessment C bundles **R + M + K** on TOP of bundle T's work.
FIRST: apply the T patch into your fresh worktree: git apply --3way ${t.patch_path} (resolve
trivially if needed; it was built on the same base). Then port:
Bundle R — render fast-path C end state: chunked updates, dirty-row iteration
(rustty_render_state_clean/_next_dirty — raw-ptr iterator vs clean() mutation seam, SAFETY
tags), structured cursor read (b4079f00c), REMOVE rustty_render_state_colors_get (update
include/, types registry, and any example/callers; e3056658d paletteCvalSlice).
Bundle M — mode defaults C API (cfc19e805 vt-side landed: GHOSTTY_TERMINAL_OPT_MODE_DEFAULT
analog + REMOVE rustty_terminal_mode_get/mode_set exports + headers), title_report C option
(W0 landed the vt flag default-off — expose RUSTTY_TERMINAL_OPT_TITLE_REPORT + tests),
XTGETTCAP terminfo_name opt (bounds-checked memcpy, 960c2cca5 class; vt side landed),
unknown-sequence C API (UnknownSequence landed in vt), cursor_at_prompt (vt cursorIsAtPrompt
— check it landed with par-3 w6-tail/or port the tiny vt getter if absent), -Dvt-features C
guards over the new surfaces (build_options landed).
Bundle K — kitty C surface (6 ABSENT, ~285 lines): pending payloads no_value contract
(image_get data_ptr NO_VALUE for pending landed in par-2 — complete the rest), hardened
geometry tests, prune reclaim, pin release, infallible set_limit C arm (vt side landed).
All new/removed exports flow through include/rustty/vt/* and the types registry consistently.
The A5 export arm (d) counts must MOVE toward parity — run
bash .port/tools/gates/check_rustty_vt_headers.sh before and after; report both. Closed-enum
gates stay green (c_int narrowing for every new inbound enum). vt-c full lane + vt lane green.`,
        { label: 'w5-RMK', phase: 'Port', schema: SCHEMA, isolation: 'worktree' })
      return [t, rmk]
    }),

  // Lane 3: embed header bundle E (apprt side)
  () => agent(COMMON + `
YOUR TASK (PATCHNAME w5-E): assessment C **Bundle E — embed header (rustty-embed.h)** + the
seams par-2/par-3 left with PORT NOTEs. The apprt-api Action/ActionKey enums are POSITIONAL
C-ABI enumerators pinned by rustty-apprt-c's action_key_positional_ordinals against the
pinned include/rustty-embed.h (cardinality 66; embed-abi-byvalue-sret-gate memory). Land as
ONE motion: (1) the header advance — append/insert the range's new enumerators in ORACLE
ORDER (export_terminal_io, set_window_title mid-enum insert, move_tab_to_new_window, open_url
OSC8 kind, IPC new_tab already half-landed via CValue, open_config payload, window-title
pair, internal-API doc rewrite) mirroring ${REF}/include/ghostty.h at the pin; (2) the
apprt-api enum arms (replacing the PORT NOTE stubs from par-2/par-3: Action::ExportTerminalIO,
SetWindowTitle, MoveTabToNewWindow, OpenUrl kind osc8) and the core Surface/App arms that
par-3 left as Ok(false) seams (w6-tail caveat: SetWindowTitle + MoveTabToNewWindow); (3) the
positional-ordinals gate update (66 -> the new cardinality, tighten with the oracle count);
(4) the macOS PORT-SEAM unlock: macos/Sources/RusttyKit/Rustty.Action.swift has the osc8
mapping case commented with a PORT-SEAM marker — uncomment/wire it now that the header kind
exists, and xcodebuild build (full test run optional but preferred); (5) A3 embed-header gate
(check_embed_h_abi.sh / check_rustty_h_abi.sh — find which pins the oracle ghostty.h sha) —
re-pin to the new oracle sha IN THE SAME PATCH with the delta documented; (6) Cluster D
deletion: rustty-lib/src/string.rs (cut_prefix) — its last caller migrated in par-3 (verify
with grep; if a caller remains, migrate it to strip_prefix here) — delete the file + pub uses.
Every enum add carries its checkGhosttyHEnum-analog KAT per the F assessment. apprt-api/
apprt-c/core/cli lanes green; gates green (tightened, never weakened).`,
    { label: 'w5-E', phase: 'Port', schema: SCHEMA, isolation: 'worktree' }),
])

const flat = []
for (const r of results) {
  if (Array.isArray(r)) flat.push(...r.filter(Boolean))
  else if (r) flat.push(r)
}
return { tasks: flat }
