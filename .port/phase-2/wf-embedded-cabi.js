export const meta = {
  name: 'embedded-cabi',
  description: 'Port apprt/embedded (libghostty C-ABI) per the reviewed EMBEDDED_BLUEPRINT: E0 scaffold+none/browser, E1 App backend, E2 Surface backend (pinned), E3 surface callbacks, E4 inspector placeholder, E5 CAPI structs+D2 asserts, E6-E10 the 71 exports, E11 C smoke KAT. Sequential, per-chunk commits, halt on block.',
  phases: [
    { title: 'E0' }, { title: 'E1' }, { title: 'E2' }, { title: 'E3' }, { title: 'E4' },
    { title: 'E5' }, { title: 'E6' }, { title: 'E7' }, { title: 'E8' }, { title: 'E9' },
    { title: 'E10' }, { title: 'E11' },
  ],
}

const RESULT_SCHEMA = {
  type: 'object', additionalProperties: false,
  required: ['chunk', 'ok', 'commit', 'blockers', 'notes'],
  properties: {
    chunk: { type: 'string' }, ok: { type: 'boolean' }, commit: { type: 'string' },
    blockers: { type: 'array', items: { type: 'string' } }, notes: { type: 'string' },
  },
}

const COMMON = `OBEY the rustty PARITY CONSTITUTION + /Users/gael/projects/rustty/.port/PORT_RULES.md. R1 (test COUNT is a contract — embedded.zig/none.zig/browser.zig are 0-test, so the C-ABI verification anchors + G3b comment coverage carry the gate), R3 (byte-faithful, every line + comment; byte-slices -> &[u8]/&str never lossy String), R5 (never weaken/skip). Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2).

FIRST read /Users/gael/projects/rustty/.port/phase-2/EMBEDDED_BLUEPRINT.md — the adversarially-reviewed design (ACCEPT-WITH-FIXES; honor every [ADV-FIX n] annotation) you implement one chunk of (section 7 = chunk table with embedded.zig ranges; sections 1-6 = designs). Also read the vt-c precedent (crates/rustty-vt-c: cbindgen.toml, point.rs:93/sgr.rs const-asserts, xtask/src/main.rs:63 header-gen — vt-c has NO build.rs, the gate is header self-consistency; embedded steps up with a C compile+link smoke). Landed base: rustty-core COMPLETE (generic Surface<A>/App<A>; the apprt-api App/Surface trait growth methods exist), rustty-apprt is a STUB deping core/apprt-api/vt/font/renderer/inspector/config/input/os. rustty-config already SHIPS a C-ABI (rustty_config_new/finalize/free, C3). renderer_seam Renderer/Thread + rustty-xev = Phase-3/D6 placeholder leaves (unimplemented!()).

KEY DOCTRINE: the embedded backend impls apprt-api's GENERIC traits with A=embedded::App (the FIRST concrete backend). embedded::Surface holds core_surface: Surface<App> BY VALUE and core init writes core_surface.rt_surface = &container -> [ADV-FIX 8] the Surface MUST be heap-boxed (alloc.create) + out-pointer init + NEVER moved post-init. The C-ABI wrapper crate is a SEPARATE rustty-apprt-c (cdylib/staticlib/rlib, deps rustty-apprt) [ADV-FIX 9] backend internals become pub. GPU/xev methods are documented unimplemented!() leaves (placeholder-leaf doctrine): the 71 exports 100% COMPILE+LINK+header-gen; ~14 are runtime-live, the rest panic when they reach a live surface (Phase-3 R-d + D6). Do NOT fabricate GPU/xev/objc logic.

GATES (FOREGROUND, ALL green before commit — never commit red):
- cargo build --workspace clean; cargo test on touched crates all pass; NO regression (spot-check rustty-vt 2019, rustty-core 15, rustty-termio 44, rustty-apprt-api 20, rustty-renderer 19, rustty-crash 12)
- cargo clippy --all-targets -D warnings on touched crates; cargo fmt --check
- bash .port/tools/gates/check_acyclic.sh GREEN (apprt-c must be a clean top-sink; no core<-apprt or apprt-api<-apprt edge)
- python3 .port/tools/gates/check_test_counts.py GREEN (add MAP entries per blueprint section 6.5: apprt_embedded/none/browser 0-test)
- scoped-G3b on YOUR embedded.zig (or none/browser) range: distinctive reference comments missing=0
- .port/phase-2/DEFERRALS.md updated for GPU/xev/objc leaves your chunk creates
COMMIT only your files + gate/manifest edits. Trailer:
Co-Authored-By: Claude Opus 4.8 (1M context)
If hard-blocked: commit what is cleanly green, report the blocker precisely. If a later chunk's helper is needed, add a faithful sequencing stub with the exact reference signature + #[allow(dead_code)] and say so.`

const CHUNKS = [
  { phase: 'E0', prompt: `CHUNK E0 (scaffold) per blueprint section 3 + 7 row E0: (1) create crates/rustty-apprt-c (crate-type=["cdylib","staticlib","rlib"], deps rustty-apprt + libc; copy vt-c's cbindgen.toml adjusted; register workspace member). (2) rustty-apprt module shells: src/{embedded.rs (or embedded/mod.rs), none.rs, browser.rs} + lib.rs wiring. (3) none.zig (19 loc) + browser.zig (4 loc) ported FULLY here (blueprint section 4: none = performIpc-only + empty Surface, NO full trait impls; browser = empty structs). (4) the global-state precursor: a minimal rustty_init export (blueprint section 3 "Global-state precursor" + risk 4 — sets rustty-core STATE, mirroring main_c.zig ghostty_init; lazy-init in ghostty_app_new is the alt). Verify apprt-c builds as a cdylib/staticlib, check_acyclic GREEN (apprt-c top-sink). MAP: apprt_none/apprt_browser (0). Commit: "rustty: embedded E0 — apprt-c crate scaffold + none/browser backends + rustty_init"` },
  { phase: 'E1', prompt: `CHUNK E1 (embedded.zig 29-342): embedded::App backend + Options (the #[repr(C)] host-callback struct, blueprint section 2 — 7 callbacks, close_surface = Option<extern"C"fn>, others non-null extern"C"fn) + KeyEvent + the App methods (init/terminate/focusEvent/keyEvent/reloadKeymap/keyboardLayout/wakeup/wait/newSurface/closeSurface/performAction/performPreAction/performIpc) + impl rustty_apprt_api::App for embedded::App (type Surface = embedded::Surface; perform_action -> performPreAction + opts.action callback; wakeup -> opts.wakeup; config -> &self.config; keyboard_layout). [ADV-FIX 9] pub internals. Commit: "rustty: embedded E1 — App backend + Options callbacks + impl apprt_api::App"` },
  { phase: 'E2', prompt: `CHUNK E2 (embedded.zig 343-609): Platform/PlatformTag/EnvVar (raw *mut c_void nsview/uiview, NO objc dep — blueprint section 5/8.1; Platform.C union pad-assert) + embedded::Surface struct (blueprint section 2 shape) + Options + init (out-pointer, heap-boxed per [ADV-FIX 8]: alloc.create(Surface) + init(surface,...) self-referential, never-move invariant — mirror the reference + core's own init discipline) + deinit + impl rustty_apprt_api::Surface for embedded::Surface (type App=embedded::App; type Core=rustty_core::Surface<embedded::App>; the full section-2 growth-point method table: core/rt_app/close(callback)/get_content_scale/get_size/get_title/supports_clipboard/clipboard_request(callback,->Result<bool,_>)/set_clipboard(callback)/get_cursor_pos/default_termio_env/deinit). [ADV-FIX 4] do NOT re-impl CoreSurfaceHandle (it's core's). This proves the self-referential type Core bound against a REAL backend. Commit: "rustty: embedded E2 — Surface backend (heap-pinned) + impl apprt_api::Surface"` },
  { phase: 'E3', prompt: `CHUNK E3 (embedded.zig 610-990): the surface event-sink callback methods (refresh/draw/updateContentScale/updateSize/colorScheme/mouseButton/cursorPos/mouseScroll/mousePressure/scrollCallback/preedit/text/focus/occlusion/completeClipboardRequest/newSurfaceOptions/defaultTermioEnv/cursorPosToPixels) delegating to core_surface.*Callback. draw/refresh reach the GPU leaf at RUNTIME (they call renderer_seam methods via core) — port the delegation LIVE; the panic is in the leaf, not here. The rest are real core logic. Commit: "rustty: embedded E3 — Surface event-sink callbacks"` },
  { phase: 'E4', prompt: `CHUNK E4 (embedded.zig 992-1243): the Inspector (dcimgui + Metal) — Phase-3 INHABITED placeholder per placeholder-leaf doctrine + blueprint row E4: struct + set_size/set_content_scale REAL (non-GPU), all the imgui/Metal render + mouse/key/text methods = documented unimplemented!() leaves. Do NOT wire dcimgui-sys logic. Update DEFERRALS.md (inspector Phase-3). Commit: "rustty: embedded E4 — Inspector Phase-3 placeholder"` },
  { phase: 'E5', prompt: `CHUNK E5 (embedded.zig 1244-1397): the CAPI extern structs into rustty-apprt-c (or apprt where they're backend fields, hand-pinned in apprt-c cbindgen after_includes): KeyEvent/SurfaceSize/ClipboardContent/Text/Point(+PointC)/Selection + EVERY D2 const-size/align-assert (blueprint section 3, vt-c point.rs:93 pattern). [ADV-FIX 7] size_of asserts catch field add/remove, NOT callback signatures. Commit: "rustty: embedded E5 — CAPI extern structs + D2 const-asserts"` },
  { phase: 'E6', prompt: `CHUNK E6 (embedded.zig 1398-1537): the CAPI App-lifecycle exports (11: ghostty_app_new/_tick/_userdata/_free/_set_focus/_key/_keyboard_changed/_open_config/_needs_confirm_quit/_has_global_keybinds/_set_color_scheme) + app/config (ghostty_app_update_config, ghostty_config_key_is_binding) as #[no_mangle] pub unsafe extern "C" fn in apprt-c. These are the runtime-exercisable core (blueprint section 1). Commit: "rustty: embedded E6 — CAPI app-lifecycle exports"` },
  { phase: 'E7', prompt: `CHUNK E7 (embedded.zig 1538-1760): CAPI surface lifecycle (ghostty_surface_config_new/_new/_free/_userdata/_app/_inherited_config/_update_config) + surface state/query (_needs_confirm_quit/_process_exited/_has_selection/_read_selection/_read_text/_free_text/_size/_foreground_pid/_tty_name) + render/window (_refresh/_draw/_set_size/_set_color_scheme/_set_content_scale/_set_focus/_set_occlusion). ghostty_surface_new heap-boxes the embedded Surface [ADV-FIX 8]. The render/window ones reach the GPU leaf at runtime (compile+link fine). Commit: "rustty: embedded E7 — CAPI surface lifecycle + state + render exports"` },
  { phase: 'E8', prompt: `CHUNK E8 (embedded.zig 1763-1922): CAPI input exports (11: _key_translation_mods/_key/_key_is_binding/_text/_preedit/_mouse_captured/_mouse_button/_mouse_pos/_mouse_scroll/_mouse_pressure/_ime_point). Delegate to embedded Surface callback methods (E3). Commit: "rustty: embedded E8 — CAPI input exports"` },
  { phase: 'E9', prompt: `CHUNK E9 (embedded.zig 1924-2106): CAPI surface-actions (6: _request_close/_split/_split_focus/_split_resize/_split_equalize/_binding_action) + clipboard (ghostty_surface_complete_clipboard_request) + inspector exports (10: ghostty_surface_inspector/ghostty_inspector_free/_set_size/_set_content_scale/_mouse_button/_mouse_pos/_mouse_scroll/_key/_text/_set_focus — reach the E4 placeholder at runtime). Commit: "rustty: embedded E9 — CAPI actions/clipboard/inspector exports"` },
  { phase: 'E10', prompt: `CHUNK E10 (embedded.zig 2108-2250): misc + Darwin-gated exports — ghostty_set_window_background_blur ([ADV-FIX 3] macOS objc msgSend + private CGS: #[cfg(target_os="macos")] objc/CGS leaf, non-Darwin no-op return; do NOT put it in any runtime-safe path) + Darwin block (ghostty_surface_set_display_id/_quicklook_font/_quicklook_word/ghostty_inspector_metal_init/_metal_render/_metal_shutdown = #[cfg(target_os="macos")] Phase-3 objc/Metal unimplemented!() leaves). Update DEFERRALS.md. After this chunk ALL 71 exports exist — do a mechanical export-count diff vs the reference's 71 export fn and record it in the commit body. Commit: "rustty: embedded E10 — CAPI misc + Darwin-gated exports (71==71)"` },
  { phase: 'E11', prompt: `CHUNK E11 (the C-ABI verification KAT + finale) per blueprint section 6 [ADV-FIX 2]: (1) extend xtask header-gen (or add header-gen-apprt) to emit include/rustty-apprt.h from apprt-c via cbindgen; header must compile standalone as C AND C++ (cpp_compat). (2) tests/smoke.c: #include the apprt-c header + the shipped rustty-config header; rustty_init() -> cfg=rustty_config_new() -> rustty_config_finalize(cfg) -> rt_cfg=ghostty_surface_config_new() -> app=ghostty_app_new(&rt_cfg,cfg) with REAL typed extern"C" no-op callback stubs (compiler type-checks them vs the cbindgen callback typedefs — the actual fn-ptr backstop) -> ghostty_config_key_is_binding(...) -> ghostty_app_needs_confirm_quit(app,...) -> ghostty_app_free(app) -> rustty_config_free(cfg). MUST NOT call ghostty_surface_new or ghostty_set_window_background_blur. (3) a Rust integration test that shells cc/clang to compile+link+run smoke.c against the staticlib, asserting exit 0. (4) full const-assert audit; check_test_counts MAP rows for embedded/none/browser; full-file scoped-G3b sweep over embedded.zig+none.zig+browser.zig (missing=0). (5) grep apprt/apprt-c for unimplemented!() and verify each is a documented GPU/xev/objc/inspector leaf (blueprint section 1); list stragglers. (6) update .port/phase-2/NEXT_PHASE_PLAN.md with the EMBEDDED C-ABI DONE checkpoint (C-ABI 71/71 compiles+links+header+smoke; runtime-live subset; GPU/xev/objc deferred to Phase-3). Commit: "rustty: embedded E11 — C-ABI smoke KAT (cc compile+link) + finale sweep"` },
]

const results = []
for (const c of CHUNKS) {
  phase(c.phase)
  let r
  try {
    r = await agent(`${COMMON}\n\n${c.prompt}\n\nReturn the structured result (chunk, ok, commit, blockers, notes).`,
      { label: c.phase, phase: c.phase, schema: RESULT_SCHEMA })
  } catch (e) {
    r = { chunk: c.phase, ok: false, commit: '', blockers: ['threw: ' + String(e)], notes: '' }
  }
  if (!r) r = { chunk: c.phase, ok: false, commit: '', blockers: ['agent returned null'], notes: '' }
  results.push(r)
  log(`${c.phase}: ${r.ok ? 'OK ' + r.commit : 'BLOCKED — halting'}`)
  if (!r.ok) break
}
return { results }
