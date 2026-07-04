export const meta = {
  name: 'core-wave2',
  description: 'Core port wave 2: Surface.zig fill per CORE_BLUEPRINT section-6 rows C5-C15 (sequential, same file), then C-T termio un-defers and C-F finale sweep. Per-chunk commits + gates, halt on block.',
  phases: [
    { title: 'C5' }, { title: 'C6' }, { title: 'C7' }, { title: 'C8' }, { title: 'C9' },
    { title: 'C10' }, { title: 'C11' }, { title: 'C12' }, { title: 'C13' },
    { title: 'C14a' }, { title: 'C14b' }, { title: 'C15' }, { title: 'C-T' }, { title: 'C-F' },
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

const COMMON = `OBEY the rustty PARITY CONSTITUTION + /Users/gael/projects/rustty/.port/PORT_RULES.md. R1 (test COUNT is a contract — Surface.zig has 0 tests, so COMMENT COVERAGE and faithful line-by-line porting carry the gate), R3 (byte-faithful, every line + comment preserved/interleaved; byte-slices -> &[u8]/&str never lossy String), R5 (never weaken/skip). Reference READ-ONLY at /Users/gael/projects/ghostty (Zig 0.15.2).

FIRST read /Users/gael/projects/rustty/.port/phase-2/CORE_BLUEPRINT.md — the adversarially-reviewed design you implement one chunk of (section 6 = the chunk table with Surface.zig line ranges; sections 1-5 = the designs; "## C0 DECISION" appendix = ADOPTED: type Core option 1, self-referential bound). Waves already landed: C0-C4 (xev crate, zigstd guard-less Mutex, apprt-api growth methods incl. clipboard_request->bool, renderer_seam.rs placeholders, app.rs = full App.zig with TestRuntime seam tests, surface_mouse.rs, inspector Event). crates/rustty-core/src/surface.rs holds the C3-declared skeleton your chunks fill IN PLACE.

CORE DOCTRINE (blueprint headline): port Surface.zig functions LIVE against the landed placeholder leaves — GPU Renderer / xev handles / uninhabited Inspector are the ONLY unimplemented!() leaves; do NOT defer portable logic as comments. Function-level GPU/xev map is blueprint section 5. Manual Clone/Copy (no A: Copy bound) and documented Send-wrapper newtypes per [ADV-FIX 6]. Raw-pointer discipline per the rust-mut-args memory: raw-ptr args across integrity-asserting calls, no long-lived &mut.

GATES (FOREGROUND, ALL green before commit — never commit red):
- cargo build --workspace; cargo test -p rustty-core (all pass; core currently 15) + NO regression spot-checks (rustty-vt default 2019, rustty-termio 44, rustty-apprt-api 20, rustty-renderer 19, rustty-crash 12)
- cargo clippy --all-targets -D warnings on touched crates; cargo fmt --check
- bash .port/tools/gates/check_acyclic.sh GREEN; python3 .port/tools/gates/check_test_counts.py GREEN
- scoped-G3b on YOUR Surface.zig line range: distinctive reference comments missing=0
- .port/phase-2/DEFERRALS.md updated for any deferral created/resolved
COMMIT only your files + gate edits. Trailer:
Co-Authored-By: Claude Opus 4.8 (1M context)
If hard-blocked: commit what is cleanly green, report the blocker precisely. If a PRIOR chunk left something your range needs (a helper above your range), port that helper too and say so.`

const CHUNKS = [
  { phase: 'C5', prompt: `CHUNK C5 (Surface.zig lines 1-456) — the Surface skeleton fill: full struct fields per blueprint section-1 signatures + InputEffect/Search/Mouse/Keyboard types + DerivedConfig (+scaledPadding) + module wiring in lib.rs. Then run the FIELD-PARITY AUDIT: enumerate every Surface.zig struct field and every DerivedConfig field vs your Rust structs — counts must match exactly (the config-cluster audit caught 7 dropped fields; blueprint risk 6 lists the field-type availability checks: RepeatableClipboardCodepointMap, MouseScrollMultiplier, KeyRemapSet, SharedGridSet::DerivedConfig, oni.Regex links). Record the audit numbers in your commit message body. Commit: "rustty: core C5 — Surface skeleton + DerivedConfig (field-parity N==N)"` },
  { phase: 'C6', prompt: `CHUNK C6 (Surface.zig 457-957): init/deinit/close/surfaceMailbox/queueIo/draw + activateInspector/deactivateInspector (the ONLY verbatim-comment deferrals in core, ~45 loc, per blueprint section 4) + needsConfirmQuit. init spawns pty/renderer/io threads: use the documented Send-wrapper newtypes [ADV-FIX 6b]; GPU/xev calls hit the placeholder leaves (surfaceInit/init(alloc,opts)/finalizeSurfaceInit; queueRender's Async::notify). Commit: "rustty: core C6 — Surface init/deinit/close/draw (inspector activate deferred verbatim)"` },
  { phase: 'C7', prompt: `CHUNK C7 (Surface.zig 958-1523): handleMessage + selectionScrollTick + childExited(+Abnormally) + passwordInput + searchCallback(_). The vt search thread is fully real (blueprint section 5 "Fully REAL now"); searchCallback's 3x wakeup.notify are xev-leaf calls. handleMessage dispatches the apprt-api surface Message enum — check arms against the reference exhaustively. Commit: "rustty: core C7 — handleMessage/childExited/search callbacks"` },
  { phase: 'C8', prompt: `CHUNK C8 (Surface.zig 1524-1866 + 2396-2604): modsChanged/mouseRefreshLinks/updateRendererHealth/updateScrollbar/notifyConfigConditionalState/updateConfig/recomputeInitialSize + setCellSize through preeditCallback. updateConfig uses renderer Message::initChangeConfig placeholder + io changeConfig (now unblocked by the C0 mutex — check whether C-T has run; if not, the io.changeConfig call sites compile against termio's current surface, defer-marking only if genuinely still blocked). mouseRefreshLinks "must be called with mutex held" — the guard-less zigstd Mutex from C0. perform_action dispatch arms per blueprint section-2 [ADV-FIX 4] regenerated list (renderer_health :1681, scrollbar :1692 are in YOUR range). Commit: "rustty: core C8 — config/mods/scrollbar/cell-size tier"` },
  { phase: 'C9', prompt: `CHUNK C9 (Surface.zig 1867-2395): Text/dumpText(Locked)/hasSelection/selectionString/pwd/resolvePathForOpening/imePoint/clipboardWrite/copySelectionToClipboards/setSelection(AndCopy). imePoint locks/unlocks WITHOUT defer — the guard-less Mutex pattern, pair every lock/unlock verbatim with the reference lines. selection_changed (:2351) perform_action arm is in your range. Commit: "rustty: core C9 — text/selection/clipboard-write tier"` },
  { phase: 'C10', prompt: `CHUNK C10 (Surface.zig 2605-3281 + 6014-end): the key tier — keyEventIsBinding/keyCallback/maybeHandleBinding/deactivateAllKeyTables/catchAllIsIgnore/endKeySequence/encodeKey(Opts) + crashThreadState + impl rustty_crash::sentry::ThreadStateSurface for Surface<A> (blueprint section-1 impl, resolving that crash deferral — update DEFERRALS.md). [ADV 6.1] the reference does 13x direct "crash.sentry.thread_state = self.crashThreadState(); defer ... = null" (2469..4708) — map Zig-defer onto explicit clear or a small guard at each site IN YOUR RANGE (other chunks handle their own sites; port the helper + pattern here). The inspector keyCallback defer-block: None-arm live (ev.deinit), Some-arm unreachable!() + verbatim recordKeyEvent comment (blueprint section 4). getProcessInfo comptime-return split per the rustty-os pty.rs precedent. Audit rustty-input for the Binding API surface (Set.getEvent, GenericLeaf, leaf.generic(), actionsSlice, action.scoped(.app)) — any gaps are additive input ports (blueprint risk 7). Commit: "rustty: core C10 — key tier + crash ThreadStateSurface un-defer"` },
  { phase: 'C11', prompt: `CHUNK C11 (Surface.zig 3282-3639): textCallback/occlusionCallback/focusCallback/refreshCallback/scrollCallback/contentScaleCallback. keyboardLayout (:3271-ish boundary — if it fell to C10's range it's done; verify) via the apprt-api App::keyboard_layout growth method. Commit: "rustty: core C11 — focus/scroll/content-scale callbacks"` },
  { phase: 'C12', prompt: `CHUNK C12 (Surface.zig 3640-4278): the mouse tier — isMouseReporting/mouseReport/mouseShiftCapture/mouseCaptured/mouseButtonCallback/maybePromptClick. Uses surface_mouse.rs (landed, SurfaceMouse.isRectangleSelectState call shape preserved). Commit: "rustty: core C12 — mouse report/button tier"` },
  { phase: 'C13', prompt: `CHUNK C13 (Surface.zig 4279-4774): linkAtPos/linkAtPin/mouseModsWithCapture/processLinks/openUrl/osc8URI/mousePressureCallback/cursorPosCallback/colorSchemeCallback/posToViewport/scrollToBottom/hideMouse/showMouse. Links use DerivedConfig.Link oni.Regex (rustty-oniguruma, landed C5 fields). Commit: "rustty: core C13 — link/cursor-pos/color-scheme tier"` },
  { phase: 'C14a', prompt: `CHUNK C14a (Surface.zig 4775-~5200): performBindingAction FIRST HALF — the highest-review-load switch. Check every dispatch arm against the blueprint section-2 [ADV-FIX 4] regenerated Action checklist; copy_title_to_clipboard (:5065) is in your range. Port arms whose helpers live in later ranges by calling the (not-yet-ported) method and adding a minimal faithful signature stub ONLY if unavoidable — prefer arm-complete porting with helpers pulled in. Commit: "rustty: core C14a — performBindingAction (first half)"` },
  { phase: 'C14b', prompt: `CHUNK C14b (Surface.zig ~5200-5650): performBindingAction SECOND HALF + closingAction. Same arm-by-arm checklist discipline as C14a. After this chunk performBindingAction must be arm-complete vs the reference switch — do a mechanical arm-count diff and record it in the commit body. Commit: "rustty: core C14b — performBindingAction (second half) + closingAction (arms N==N)"` },
  { phase: 'C15', prompt: `CHUNK C15 (Surface.zig 5651-6036): writeScreenFile/completeClipboardRequest/startClipboardRequest/completeClipboardPaste/completeClipboardReadOSC52/showDesktopNotification/presentSurface/getProcessInfo (if not C10). startClipboardRequest returns the load-bearing bool through the [ADV-FIX 1] clipboard_request -> Result<bool,_> seam. Commit: "rustty: core C15 — clipboard completions/screen-file/notifications"` },
  { phase: 'C-T', prompt: `CHUNK C-T — the termio un-defer family (blueprint section 3 verdicts), now unblocked by C0's zigstd Mutex + concrete Timeout: mailbox::send caller-held-mutex pattern, Termio::queueMessage locked path, stream_handler surfaceMessageWriter + rendererMessageWriter, and the mutex-only Termio method family (changeConfig, resize, sizeReport*, focusGained, resetSynchronizedOutput, clearScreen, scrollViewport, jumpToPrompt, colorSchemeReport*). ALSO realize apprt-api surface.rs's newConfig verbatim block (needs App::focused_surface + Surface::pwd — landed by C9; put the realized fn where the blueprint's apprt-api deferral note says it lands — likely core app glue; update the apprt-api deferral note + DEFERRALS.md). xev-blocked Termio paths (init/threadEnter/processOutput*) STAY deferred (D6) — do not touch. TIME-BOX per blueprint risk 10: anything beyond this list stays deferred. Verify termio still >= 44 tests + workspace green. Commit: "rustty: core C-T — termio mutex-family un-defers + newConfig realized"` },
  { phase: 'C-F', prompt: `CHUNK C-F — the finale sweep: (1) DEFERRALS.md consolidation for core (activate/deactivateInspector, renderer_seam placeholders, D6 xev list, anything chunks left); (2) PORT_PLAN.md crate-24 corrections final (core tests = actual count); (3) check_test_counts.py MAP final (core_surface mapped to surface.rs, counts honest); (4) FULL-FILE scoped-G3b sweep over Surface.zig + App.zig + surface_mouse.zig — every distinctive reference comment present (missing=0); fix any gaps found; (5) whole-workspace gates: build, ALL crate tests (report total), clippy -D warnings workspace, fmt, acyclic, count-gate; (6) grep surface.rs/app.rs for unimplemented!() and verify EVERY one is a documented placeholder leaf (GPU/xev/Inspector) or DEFERRALS-tracked — list any stragglers and fix; (7) update .port/phase-2/NEXT_PHASE_PLAN.md: CORE DONE checkpoint section (mirror the earlier checkpoint style). Commit: "rustty: core C-F — finale sweep (G3b full-file, gates, plan checkpoint)"` },
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
  log(`${c.phase}: ${r.ok ? 'OK ' + r.commit : 'BLOCKED — halting wave'}`)
  if (!r.ok) break
}
return { results }
