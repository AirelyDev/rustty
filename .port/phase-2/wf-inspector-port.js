export const meta = {
  name: 'inspector-port',
  description: 'Full port of the ghostty terminal inspector to rustty-inspector (backend + lifecycle + 9 widgets), per docs/superpowers/plans/2026-07-14-inspector-port.md — sequential implement->verify pipeline, autonomous',
  whenToUse: 'Execute the inspector port plan end-to-end via subagent orchestration',
  phases: [
    { title: 'Backend', detail: 'imgui_impl_metal vendor+wire+FFI+exports (T1-3)' },
    { title: 'Lifecycle', detail: 'Inspector struct + R2 trait + core activate/queueRender (T4-6)' },
    { title: 'Widgets', detail: '9 submodules + helpers + aggregate + shakedown (T7-18)' },
  ],
}

const OUT = {
  type: 'object', required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' },
    status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    evidence: { type: 'string' },
    commit: { type: 'string' },
    residual: { type: 'string' },
    blocker: { type: 'string' },
  },
}
const VERDICT = {
  type: 'object', required: ['faithful', 'builds', 'verdict'],
  properties: {
    faithful: { type: 'boolean' }, builds: { type: 'boolean' },
    verdict: { type: 'string', enum: ['approved', 'needs-fixes'] },
    findings: { type: 'string' },
  },
}

const COMMON = `
REPO: /Users/gael/projects/rustty (branch rebrand/rustty-full — stay on it). ORACLE (READ-ONLY): /Users/gael/projects/ghostty @ 4749c4e9. PLAN: docs/superpowers/plans/2026-07-14-inspector-port.md. DESIGN: .port/phase-2/PREPORT_INSPECTOR.md (LIFETIMES §1, invariants §2 INV-1..13, comptime inventory §3, constants §4, hazards §5 R1..R4).
RULES: R1 fidelity — port EVERY line of the named oracle .zig file, preserve its comments as ported comments, carry verbatim constants; R4 acyclicity — rustty-inspector/rustty-renderer never name apprt types (renderer-defined accessor trait for the overlay edge); panic=abort (asserts abort, recoverable->sentinel, no catch_unwind); ImGui/Metal/ig context are !Send main-thread-affine (INV-11), objc ids +1/release balanced; INV-1 terminal derefs only under surface.renderer_state.mutex, raw ptr never escapes the guard; INV-3 the ONLY inspector->terminal live mutation is t.scrolling_region override (every other editable widget reads-into-local-and-discards, display-only).
SB-class hazards PROVEN on an isolated mirihar harness (.port/phase-2/mirihar-inspector-*), NOT hand-reasoned — see the [[shaper-hook-sb-clean-interleave]] doctrine (route mutating calls via the alias's OWN ptr + raw-self helpers).
BUILD GATE: cargo build --workspace stays green after your task. GIT: explicit paths, git mv for renames, retry on index.lock (sleep 3, ≤10x), never push/reset/stash. COMMIT your files with the plan's commit message. Return via StructuredOutput. If a piece needs a large binding not present or a design decision beyond a faithful port, report partial/blocked with EXACTLY what's needed — do NOT fake it (R5).`

// Per-task specs (oracle file + LOC + the INV/LIFETIMES/hazard rows + acceptance). Deps enforced by sequential execution.
const TASKS = [
  { key: 'T1', phase: 'Backend', title: 'vendor+compile imgui_impl_metal into dcimgui-sys',
    spec: `Plan Task 1. Git-track the present-but-untracked crates-sys/dcimgui-sys/vendor/imgui/backends/imgui_impl_metal.{mm,h}. Compile the .mm in dcimgui-sys/build.rs (cc::Build, -x objective-c++, -fno-objc-arc, include vendor/imgui, macOS-cfg, link framework Metal+QuartzCore+Foundation) mirroring how ghostty's src/build compiles it; add the __has_include block for imgui_impl_metal.h in ext.cpp (mirror the opengl3 block ext.cpp:39-50). Verify cargo build -p dcimgui-sys green + nm shows _ImGui_ImplMetal_Init. Commit "inspector: vendor + compile imgui_impl_metal into dcimgui-sys (Task 1)".` },
  { key: 'T2', phase: 'Backend', title: 'dcimgui-sys ImGui_ImplMetal FFI surface',
    spec: `Plan Task 2. Expose the ImGui_ImplMetal calls the reference inspector makes (read ghostty src/apprt/embedded.zig inspector 992-1250 + ~2225-2250): Init(device), NewFrame(descriptor), RenderDrawData(draw_data, command_buffer, command_encoder), Shutdown, {Create,Destroy}DeviceObjects. objc ids cross as *mut c_void. If bindgen can't emit ObjC++ sigs, add extern "C" C-ABI shims in ext.cpp (dcimgui_ImplMetal_Init(void* device) -> ImGui_ImplMetal_Init((__bridge id<MTLDevice>)device)) + declare in src/lib.rs (stable names — apprt-c calls them). A #[test] referencing each symbol (link-error if missing). cargo build -p dcimgui-sys. Commit "inspector: dcimgui-sys ImGui_ImplMetal FFI surface (Task 2)".` },
  { key: 'T4', phase: 'Lifecycle', title: 'inhabit Inspector struct + per-Surface ImGui context',
    spec: `Plan Task 4 (do BEFORE the metal exports T3 — they need the struct fields). Port apprt.embedded.Inspector (embedded.zig:992-1250): struct fields per LIFETIMES §1 (surface:*mut Surface BORROW_FIELD, ig_ctx:*mut ImGuiContext GHOSTTY_C owned Create->Destroy, backend:Option<Backend> enum{metal}); init (ImGui_CreateContext + CoreInspector.setup() the shared IO ONCE: docking+sRGB+no-ini/log+embedded font @16*2px, INV-11); deinit (ImplMetal_Shutdown if backend, DestroyContext). In crates/rustty-inspector/src/lib.rs replace pub enum Inspector {} with the inhabited aggregate (Inspector.zig:22 gui owns key_stream/vt_stream/renderer_info) — keep R4-clean. Files: crates/rustty-apprt/src/embedded.rs (Inspector struct + redrawInspector:514->queueInspectorRender), crates/rustty-inspector/src/lib.rs. cargo build -p rustty-apprt -p rustty-inspector. Commit "inspector: inhabit Inspector struct + per-Surface ImGui context (Task 4)".` },
  { key: 'T3', phase: 'Backend', title: 'port metal init/render/shutdown exports (apprt-c)',
    spec: `Plan Task 3 (after T2+T4). Replace the unimplemented!() in crates/rustty-apprt-c/src/lib.rs rustty_inspector_metal_{init,render,shutdown} with faithful ports of ghostty_inspector_metal_init/render/shutdown (embedded.zig ~2225-2250): init -> SetCurrentContext(ig_ctx)+ImplMetal_Init(device); render -> ImplMetal_NewFrame(descriptor)+Inspector.render draw+ImplMetal_RenderDrawData(GetDrawData(),command_buffer,command_encoder); shutdown -> ImplMetal_Shutdown. INV-11 SetCurrentContext before every ImGui call; do NOT over-release the host-owned device/command_buffer/descriptor ids. cargo build -p rustty-apprt-c. Commit "inspector: port metal init/render/shutdown exports (Task 3)".` },
  { key: 'T5', phase: 'Lifecycle', title: 'R2 acyclic accessor-trait for renderer overlay edge',
    spec: `Plan Task 5. Define a RENDERER-owned accessor trait (e.g. InspectorOverlay { fn overlay_features(&self)->Vec<crate::overlay::Feature> }) — mirror the RendererConfig precedent ([[renderer-config-accessor-trait-acyclic]]); impl it in crates/rustty-inspector/src/widgets/renderer.rs (overlayFeatures dupes Info.features into a fresh owned Vec, renderer.zig:25-41). Replace the stub at crates/rustty-renderer/src/generic.rs:4600 with live match state.inspector {Some(i)=>i.overlay_features(), None=>vec![]} via the trait (verbatim body in the ported comment 4602-4610). INV-7 bidirectional edge. Verify NO rustty-renderer->rustty-inspector dep edge added (cargo tree / bash .port/tools/gates/check_acyclic.sh). cargo build --workspace. Commit "inspector: R2 acyclic accessor-trait for renderer overlay edge (Task 5)".` },
  { key: 'T6', phase: 'Lifecycle', title: 'core activate/deactivate/init + queueInspectorRender',
    spec: `Plan Task 6. In crates/rustty-core/src/surface.rs port activateInspector/deactivateInspector (from the preserved verbatim Zig comments — construct/destroy inspectorpkg::Inspector, wire onto renderer_state.inspector + push to render-thread mailbox per the comment), initInspector, queueInspectorRender (INV-12: after gui.draw ALWAYS renderer_thread.wakeup.notify() — Inspector.zig:125-131), the keyCallback inspector defer (:1524, record key events into the inspector key stream when an inspector exists). Field inspector:Option<*mut inspectorpkg::Inspector> is already at :395. cargo build --workspace; cargo test -p rustty-core (a lifecycle KAT: activate->field set->deactivate->field null if feasible). Commit "inspector: core activate/deactivate/init + queueInspectorRender wakeup seam (Task 6)".` },
  { key: 'T7', phase: 'Widgets', title: 'widgets.rs + units.rs shared helpers',
    spec: `Plan Task 7. Port widgets.zig (227) -> crates/rustty-inspector/src/widgets.rs: detachableHeader(label,state,ctx,contentFn) (widgets.zig:150-227 comptime callback -> Rust generic fn/FnMut monomorphized per call site, §3), shared table/window helpers. Port units.zig (3) -> units.rs. cargo build -p rustty-inspector. Commit "inspector: widgets.rs + units.rs shared helpers (Task 7)".` },
  { key: 'T8', phase: 'Widgets', title: 'key.rs ImGui draw half',
    spec: `Plan Task 8. crates/rustty-inspector/src/widgets/key.rs — the pure-data Event/EventRing/label() half is LIVE; ADD the ImGui draw half of key.zig (535, the keyboard-event table) against dcimgui-sys, preserving the live data half. §3 inline for over the getKeyAction key tuple. LIFETIMES: Event.binding/pty/utf8 OWNED (INV-10 done), Stream.events CircBuf (INV-9 eviction). cargo build -p rustty-inspector. Commit "inspector: key.rs ImGui draw half (Task 8)".` },
  { key: 'T9', phase: 'Widgets', title: 'style.rs widget',
    spec: `Plan Task 9. Create crates/rustty-inspector/src/widgets/style.rs from style.zig (125) faithfully (R1). §3 inline for(style_flags) tuple. Register the mod. cargo build -p rustty-inspector. Commit "inspector: style.rs widget (Task 9)".` },
  { key: 'T10', phase: 'Widgets', title: 'renderer.rs widget draw half',
    spec: `Plan Task 10. crates/rustty-inspector/src/widgets/renderer.rs (created in T5 for the trait impl) — port the full renderer-info widget from renderer.zig (123): checkboxes writing Info.features (INV-7), §3 std.meta.Tag(Overlay.Feature) map key. The overlay_features dupe was T5; add the draw half. cargo build -p rustty-inspector. Commit "inspector: renderer.rs widget (Task 10)".` },
  { key: 'T11', phase: 'Widgets', title: 'screen.rs widget',
    spec: `Plan Task 11. Create crates/rustty-inspector/src/widgets/screen.rs from screen.zig (400). LIFETIMES: Info.draw(data).screen/color_palette BORROW_PARAM call-scoped under mutex (INV-1) — never cache the *Screen/*DynamicPalette. INV-4 kitty_keyboard flag reads, INV-6 screen-window lifecycle re-fetch each frame. §3 @typeInfo(Flags).@"struct".fields kitty-keyboard rows + comptimePrint field names (hand-expand). cargo build -p rustty-inspector. Commit "inspector: screen.rs widget (Task 11)".` },
  { key: 'T12', phase: 'Widgets', title: 'page.rs widget',
    spec: `Plan Task 12. Create crates/rustty-inspector/src/widgets/page.rs from page.zig (428). INV-2 dirty READ-ONLY (page.isDirty()/row.dirty — observe never clear). cargo build -p rustty-inspector. Commit "inspector: page.rs widget (Task 12)".` },
  { key: 'T13', phase: 'Widgets', title: 'pagelist.rs widget + comptime hand-expansion',
    spec: `Plan Task 13. Create crates/rustty-inspector/src/widgets/pagelist.rs from pagelist.zig (852, largest). INV-2 dirty read (pin.isDirty()). §3 comptime hand-expansion: std.meta.tags(point.Tag) (verbatim set), combo_width comptime block (scan tag-name lengths -> a const), regionHelpText(comptime tag)+regionRow(pages,comptime tag) monomorphized per tag. cargo build -p rustty-inspector. Commit "inspector: pagelist.rs widget + comptime hand-expansion (Task 13)".` },
  { key: 'T14', phase: 'Widgets', title: 'terminal.rs widget + modes table + INV-3 write-back',
    spec: `Plan Task 14. Create crates/rustty-inspector/src/widgets/terminal.rs from terminal.zig (726). LIFETIMES Info.screens EnumMap of owned screen.Info (INV-6 re-validate each frame, remove stale/closed). §3 comptime: hand-expand @typeInfo(Mode).@"enum".fields (modes table, INV-4 t.modes.get display-only), std.meta.tags(ScreenSet.Key), comptimePrint. INV-3 CRITICAL: layoutTable's t.scrolling_region=override (guarded changed && top<bottom && left<right) is the ONLY live-state write-back; EVERY other editable widget reads-into-local-and-discards (display-only). cargo build -p rustty-inspector. Commit "inspector: terminal.rs widget + modes table + INV-3 scrolling_region write-back (Task 14)".` },
  { key: 'T15', phase: 'Widgets', title: 'surface.rs widget + R3 pin Miri proof',
    spec: `Plan Task 15. R3 HAZARD FIRST: Mouse.last_point:?terminal.Pin (BORROW_FIELD) raw-aliases PageList memory INVALIDATED by reflow. Create .port/phase-2/mirihar-inspector-pin/ isolated harness proving the port's Pin handling is SB-clean (Pin held across a reflow-like &mut PageList must re-validate/drop, not deref stale); cargo +nightly miri test it GREEN before shipping. Then create crates/rustty-inspector/src/widgets/surface.rs from surface.zig (504) — the top-level surface inspector window aggregating terminal/keyboard/termio/renderer sub-windows under ONE mutex lock (INV-1). cargo build -p rustty-inspector. Commit "inspector: surface.rs widget + R3 pin Miri proof (Task 15)".` },
  { key: 'T16', phase: 'Widgets', title: 'termio.rs + metadata-encoder + R1 self-alias Miri',
    spec: `Plan Task 16 (the design-heaviest). STEP 1 metadata-encoder hand-expansion (§3, the single largest comptime obligation): termio.zig encodeMetadata/encodeMetadataSingle switch on @typeInfo(T) over OSC/parser-action payloads (.optional/.bool/.enum->tagName/.union/.struct/.int). Rust has no @typeInfo — hand-expand a visitor/match over EVERY concrete rustty-vt osc.Command/Parser.Action variant, faithful to the reference per-type output. STEP 2 R1 self-alias Miri: VTHandler.State.events=&self.events is a SELF-ALIAS of Stream.events (sibling field) appended-through during nextSlice (INV-8 capture window) — route the append via the alias's OWN ptr + raw-self helpers ([[shaper-hook-sb-clean-interleave]]); PROVE on .port/phase-2/mirihar-inspector-vthandler/ (long-lived raw alias + interleaved &mut to that field = SB/TB UB) GREEN before shipping. STEP 3 port termio.zig (811): Stream<VTHandler> parser (§3 handler-as-trait per rustty-vt), VTEvent ring (INV-9 element-drop-before-slot-reuse), arena-backed VTEvent (raw_description/metadata in the event arena, INV-8 snapshot cursor/scrolling_region BY VALUE at event time), R4 selection sync (INV-13 reverse iterator MUTATES ev.imgui_selected during draw — port faithfully). cargo +nightly miri test the vthandler harness GREEN; cargo build -p rustty-inspector. Commit "inspector: termio.rs + metadata-encoder hand-expansion + R1 self-alias Miri proof (Task 16)".` },
  { key: 'T17', phase: 'Widgets', title: 'Inspector.rs aggregate + top-level render',
    spec: `Plan Task 17. Port Inspector.zig (131) + main.zig (8) -> crates/rustty-inspector/src/Inspector.rs (or lib.rs): the top-level render (INV-11 SetCurrentContext, then draw gui = the surface.Inspector aggregate of all sub-windows, then INV-12 renderer_thread.wakeup.notify()), the aggregate owning key_stream/vt_stream/renderer_info + the surface.Inspector gui. Wire all widget mods. cargo build --workspace. Commit "inspector: Inspector.rs aggregate + top-level render (Task 17)".` },
]

// ---- sequential implement->verify pipeline (deps + shared-file safety) ----
const results = []
for (const t of TASKS) {
  phase(t.phase)
  const impl = await agent(`${COMMON}\n\nTASK ${t.key} (${t.title}):\n${t.spec}`,
    { label: `impl:${t.key}`, phase: t.phase, schema: OUT })
  log(`${t.key} impl: ${impl ? impl.status : 'FAILED'}${impl && impl.commit ? ' @'+impl.commit : ''}`)
  let verdict = null
  if (impl && impl.status === 'done') {
    verdict = await agent(`${COMMON}\n\nREVIEW ${t.key} (${t.title}). The implementer reports: ${impl.evidence} (commit ${impl.commit||'?'}). Verify against the oracle + PREPORT_INSPECTOR.md: (a) FAITHFUL — every line of the named oracle .zig ported, comments preserved, invariants/hazards honored (esp. INV-1 mutex scope, INV-3 single write-back, R1/R2/R3/R4 where in scope), NOT stubbed/faked; (b) BUILDS — cargo build for the touched crate is green (run it read-only); (c) any Miri harness for this task is actually GREEN. Return faithful/builds/verdict + findings. Do NOT mutate the tree.`,
      { label: `verify:${t.key}`, phase: t.phase, schema: VERDICT })
    log(`${t.key} verify: ${verdict ? verdict.verdict : 'FAILED'}`)
  }
  results.push({ task: t.key, title: t.title, status: impl ? impl.status : 'failed', commit: impl && impl.commit, residual: impl && (impl.residual || impl.blocker), verify: verdict && verdict.verdict, findings: verdict && verdict.findings })
  // A blocked/partial task does not abort the run — later widgets touch distinct files; log + continue.
  if (impl && impl.status !== 'done') log(`⚠ ${t.key} ${impl.status}: ${impl.blocker || impl.residual || ''} — continuing`)
}

const done = results.filter(r => r.status === 'done').length
const approved = results.filter(r => r.verify === 'approved').length
log(`inspector port: ${done}/${TASKS.length} implemented, ${approved} approved`)
return {
  summary: `${done}/${TASKS.length} done, ${approved} verify-approved`,
  results,
  needs_attention: results.filter(r => r.status !== 'done' || r.verify === 'needs-fixes'),
}
