export const meta = {
  name: 'p23-inspector',
  description: 'Plan P2.3: the full ImGui terminal inspector — ImGui_ImplMetal backend + activate/deactivate + queueInspectorRender + the metal exports + the widget render seams — functional like ghostty (cmd-opt-i)',
  whenToUse: 'macOS full-functionality plan P2.3 (the largest single item)',
  phases: [
    { title: 'Backend', detail: 'ImGui_ImplMetal init/render/shutdown (inspector_metal_*) against dcimgui-sys' },
    { title: 'Lifecycle', detail: 'activate/deactivateInspector + queueInspectorRender + the embedded/apprt-c inspector wiring' },
    { title: 'Widgets', detail: 'the inspector widget render seams (rustty-inspector) from ghostty src/inspector/*' },
  ],
}
const OUT = {
  type: 'object', required: ['task', 'status', 'evidence'],
  properties: {
    task: { type: 'string' }, status: { type: 'string', enum: ['done', 'partial', 'blocked'] },
    evidence: { type: 'string' }, committed: { type: 'boolean' }, commit_refs: { type: 'array', items: { type: 'string' } }, residual: { type: 'string' }, blocker: { type: 'string' },
  },
}
const COMMON = `
REPO: /Users/gael/projects/rustty. Oracle (READ-ONLY): /Users/gael/projects/ghostty @ 4749c4e9 — src/inspector/* + the embedded.zig inspector exports + renderer/Metal.zig imgui backend are your reference; port faithfully (R1: every line, comments preserved). DESIGN: .port/phase-2/PREPORT_INSPECTOR.md (22 LIFETIMES rows, 13 seams split 5 mechanical / 8 design; HAZARDS: R1 VTHandler self-alias needs isolated-Miri proof, R2 renderer acyclicity via accessor trait). dcimgui-sys (crates-sys/dcimgui-sys) is the ImGui FFI leaf (present). The inspector seams: crates/rustty-apprt/src/embedded.rs (queueInspectorRender :1659, Inspector struct, ghostty_inspector_*), crates/rustty-apprt-c/src/lib.rs (inspector_metal_init/render/shutdown ~:2396-2425, ghostty_surface_inspector), crates/rustty-core/src/surface.rs (activate/deactivateInspector :2857/2891), crates/rustty-inspector/src/* (widget render), crates/rustty-renderer/src/generic.rs:4600 (overlayFeatures).
RULES: R1 fidelity; R4 acyclicity (inspector/renderer must not name apprt types — use accessor traits per the renderer-config precedent); R5; panic=abort (recoverable→sentinel, asserts abort); main-thread-affine ImGui/Metal (I5, !Send). Any SB-class fix (VTHandler self-alias) PROVEN on a mirihar harness. Git: explicit paths, index.lock retry (sleep 3, ≤10x), no push/reset/stash. Return via StructuredOutput; COMMIT own files. If a piece needs a large new binding not present, report partial with exactly what's needed (do NOT fake it, R5).`

phase('Backend')
const backend = await agent(`${COMMON}
TASK P2.3-Backend: port ImGui_ImplMetal init/render/shutdown — the metal backend for the inspector. In crates/rustty-apprt-c/src/lib.rs the exports rustty_inspector_metal_init / _render / _shutdown (~:2396/2399/2425) are unimplemented!() leaves. Reference: ghostty embedded.zig ghostty_inspector_metal_init/render/shutdown (:2225/2229/2243) which call ImGui_ImplMetal_Init/NewFrame+RenderDrawData/Shutdown. Wire them against dcimgui-sys's ImGui_ImplMetal bindings (check crates-sys/dcimgui-sys for the ImplMetal symbols — ext.cpp may need the impl_metal wrapper added; if the ImplMetal backend isn't compiled into dcimgui-sys, add it to its build.rs/ext.cpp vendoring the imgui_impl_metal.mm, R3). Main-thread-affine (I5), objc ids released per I16 (the exports release the passed device/descriptor/command-buffer). Replace the unimplemented!() with the faithful bodies + ported comments. Verify: cargo build -p rustty-apprt-c green (+ dcimgui-sys if changed). Commit ("inspector: ImGui_ImplMetal backend — metal init/render/shutdown (P2.3-backend)"). If ImplMetal needs vendoring into dcimgui-sys and that's large, do it (R3) or report exactly what .mm/symbols are needed.`,
  { label: 'imgui-metal-backend', phase: 'Backend', schema: OUT })
log(`Backend: ${backend ? backend.status : 'failed'}`)

phase('Lifecycle')
const lifecycle = await agent(`${COMMON}
TASK P2.3-Lifecycle: port the inspector lifecycle. (1) crates/rustty-core/src/surface.rs activateInspector (:2857) + deactivateInspector (:2891) — the verbatim Zig bodies are preserved as comments; construct/destroy the inspectorpkg::Inspector, wire it onto renderer_state.inspector + the render-thread mailbox (the comments show the exact push). (2) crates/rustty-apprt/src/embedded.rs queueInspectorRender (:1659) + the Inspector struct fields (:1833+) + ghostty_inspector_* exports — port faithfully. (3) crates/rustty-apprt-c/src/lib.rs the ghostty_surface_inspector / inspector_free / inspector_set_* exports if stubbed. Respect R4 (renderer/inspector accessor traits, no apprt type names) and the I2/VTHandler self-alias hazard (PREPORT_INSPECTOR R1 — if the inspector holds a self-alias into the surface/vt, prove the aliasing on a mirihar harness before shipping, per doctrine). Build on P2.3-Backend (the metal backend is now wired). Verify: cargo build --workspace green; cargo test where a lifecycle KAT is feasible. Commit ("inspector: activate/deactivate + queueInspectorRender + embedded/apprt-c wiring (P2.3-lifecycle)"). Report any hazard proof (mirihar) or partial.`,
  { label: 'inspector-lifecycle', phase: 'Lifecycle', schema: OUT })
log(`Lifecycle: ${lifecycle ? lifecycle.status : 'failed'}`)

phase('Widgets')
const widgets = await agent(`${COMMON}
TASK P2.3-Widgets: port the inspector widget render seams. crates/rustty-inspector/src/* has the Dear ImGui widget render sections (the terminal inspector panels: modes, screen, size, cursor, keyboard, termio, cell inspector, etc.) as seams; crates/rustty-renderer/src/generic.rs:4600 overlayFeatures is the render hook. Reference: ghostty src/inspector/* (Inspector.zig + the per-panel render fns) — port each panel's ImGui widget calls faithfully (R1) against dcimgui-sys. Use PREPORT_INSPECTOR.md's 13-seam split (5 mechanical / 8 design). Build on the backend + lifecycle. Fill generic.rs:4600 overlayFeatures (now live once Inspector exists). Verify: cargo build --workspace green. This is large — commit per panel-group; if some panels need design work beyond a faithful port (the 8 "design" seams), port the mechanical ones + report the design ones as partial with what's needed. Commit ("inspector: widget render panels (P2.3-widgets)"). Return status done/partial with exactly which panels landed vs remain.`,
  { label: 'inspector-widgets', phase: 'Widgets', schema: OUT })
log(`Widgets: ${widgets ? widgets.status : 'failed'}`)

return {
  backend: backend ? backend.status : 'fail', lifecycle: lifecycle ? lifecycle.status : 'fail', widgets: widgets ? widgets.status : 'fail',
  summary: `backend ${backend ? backend.status : 'fail'}; lifecycle ${lifecycle ? lifecycle.status : 'fail'}; widgets ${widgets ? widgets.status : 'fail'}`,
}
