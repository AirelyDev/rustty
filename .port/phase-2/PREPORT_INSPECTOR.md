# PREPORT_INSPECTOR.md — Phase-5 pre-port analysis, gui-inspector tier

Oracle: `/Users/gael/projects/ghostty` @ 4749c4e9 (read-only). Port crate: `crates/rustty-inspector`.
Scope: `src/inspector/` (Inspector.zig 131, main.zig 8, units.zig 3, widgets.zig 227, widgets/{key 535, page 428, pagelist 852, renderer 123, screen 400, style 125, surface 504, terminal 726, termio 811} = **4873 LOC**) + the `imgui_impl_metal` backend owner `apprt.embedded.Inspector` (embedded.zig:992-1250+). dcimgui FFI leaf is done; the ImGui-consuming bodies are all deferred.

Port status today: only `widgets/key.rs` is carved — the pure-data `Event`/`EventRing`/`label()` half is LIVE (threaded through core's key hot path); its ImGui half plus all 8 other widget submodules, `widgets.zig` helpers, `Inspector.zig`, and `units.zig` are the deferred Phase-3/5 GUI port (`lib.rs`, `widgets.rs` PORT NOTEs; `Inspector` is an uninhabited `pub enum Inspector {}` placeholder).

---

## 1. LIFETIMES.tsv rows (inspector pointer fields)

Schema (existing ledger): `file⇥struct⇥field⇥zig_type⇥class⇥rust_type⇥evidence`. Classes per the ledger's 11-class taxonomy (OWNED / SHARED / BORROW_PARAM / BORROW_FIELD / GHOSTTY_C …). No existing `inspector` rows are in GHOSTTY_LIFETIMES.tsv (grep clean) — these are all new prospective rows. Paste-ready:

```
src/inspector/widgets/key.zig	Event	binding	[]const input.Binding.Action = &.{}	OWNED	Vec<rustty_input::binding::Action> — freed in Event.deinit via alloc.free; PORTED key.rs:64	key.zig:35-39 — deinit frees self.binding, self.event.utf8, self.pty
src/inspector/widgets/key.zig	Event	pty	[]const u8 = ""	OWNED	Vec<u8> (byte-slice→Vec<u8>, never String); PORTED key.rs:69	key.zig:38 — `if (self.pty.len > 0) alloc.free(self.pty)`
src/inspector/widgets/key.zig	Event	event.utf8	[]const u8 (in input.KeyEvent)	OWNED	Vec<u8> owned by KeyEvent (input tier); Event.init dupes it (key.zig:31)	key.zig:31,37 — dupe on init, free on deinit
src/inspector/widgets/key.zig	Stream	events	EventRing (=CircBuf(Event,undefined))	OWNED	CircBuf<Event> — Stream.deinit iterates+deinits every Event then events.deinit	key.zig:271-275 — per-element deinit before ring deinit
src/inspector/widgets/termio.zig	VTHandler.State	terminal	*const terminal.Terminal	BORROW_FIELD	*const Terminal raw alias, NEVER &mut; scoped to one nextSlice call, owner is Surface	termio.zig:51-56 — state set before nextSlice, cleared by defer; termio.zig:716
src/inspector/widgets/termio.zig	VTHandler.State	events	*VTEvent.Ring	BORROW_FIELD	*mut VTEvent.Ring — SELF-ALIAS of Stream.events (sibling field of same Stream); route appends via raw-self helper, prove on Miri (see Risk R1)	termio.zig:54 `.events = &self.events`; termio.zig:784 append through the alias
src/inspector/widgets/termio.zig	VTHandler	state	?State	BORROW_PARAM	Option<State> — whole capture is call-scoped (null ⇒ drop events)	termio.zig:695, 746 `if (self.state) |*s| s else return true`
src/inspector/widgets/termio.zig	Stream	parser_stream	VTHandler.Stream (=terminal.Stream(VTHandler))	OWNED	owned parser Stream; deinit calls parser_stream.deinit	termio.zig:42, 707
src/inspector/widgets/termio.zig	VTEvent	arena_state	ArenaAllocator.State	OWNED	arena backing ALL of this event's allocations; deinit promotes+deinits	termio.zig:451,460-463 — arena.state stored, promote+deinit frees desc/metadata
src/inspector/widgets/termio.zig	VTEvent	raw_description	[:0]const u8	OWNED	CString/Vec<u8> allocated in the event's arena (not gpa)	termio.zig:438,453 — toOwnedSliceSentinel from arena
src/inspector/widgets/termio.zig	VTEvent	metadata	Metadata.Unmanaged (StringHashMap([:0]const u8))	OWNED	arena-backed map; keys+values live in arena, freed by arena.deinit	termio.zig:435,457 — md.init(arena alloc), md.unmanaged stored
src/inspector/widgets/renderer.zig	Info	features	AutoArrayHashMapUnmanaged(Tag(Overlay.Feature), Overlay.Feature)	OWNED	owned map; Info.deinit frees; renderer READS it (see INV-7 / Risk R2 accessor-trait)	renderer.zig:20-22 deinit; generic.zig:1266 renderer consumes overlayFeatures
src/inspector/widgets/screen.zig	Info.draw(data)	screen	*terminal.Screen	BORROW_PARAM	&Screen call-scoped, obtained under mutex from t.screens.get	screen.zig:29; terminal.zig:83,115 supplies it per-frame
src/inspector/widgets/screen.zig	Info.draw(data)	color_palette	*const terminal.color.DynamicPalette	BORROW_PARAM	&DynamicPalette call-scoped	screen.zig:41; terminal.zig:120 `&t.colors.palette`
src/inspector/widgets/terminal.zig	Info	screens	ScreenMap (EnumMap(ScreenSet.Key, screen.Info))	OWNED	EnumMap of owned screen.Info values (not pointers); window lifecycle re-validated each frame	terminal.zig:24,141-144; terminal.zig:83-89 removes stale entries
src/inspector/widgets/surface.zig	Inspector.draw	surface	*const Surface	BORROW_PARAM	&Surface call-scoped; all terminal reads under surface.renderer_state.mutex	surface.zig:48-60 lock/deref
src/inspector/widgets/surface.zig	Mouse	last_point	?terminal.Pin	BORROW_FIELD	Pin{node:*List.Node,…} raw-aliases pagelist memory; INVALIDATED by reflow — see Risk R3	surface.zig:203; PageList.zig:1226 `node: *List.Node`; pin.rs:533
src/inspector/Inspector.zig	Inspector	gui	widgets.surface.Inspector	—(value)	inline aggregate value (no pointer); owns key_stream/vt_stream/renderer_info	Inspector.zig:22; surface.zig:21-28
src/apprt/embedded.zig	Inspector	surface	*Surface	BORROW_FIELD	*mut Surface back-ref; owner is App's surface list; 1:1 with Surface	embedded.zig:997,1029
src/apprt/embedded.zig	Inspector	ig_ctx	*cimgui.c.ImGuiContext	GHOSTTY_C	ImGui-allocated context; OWNED via ImGui_CreateContext→ImGui_DestroyContext (dcimgui-sys raw ptr)	embedded.zig:998,1016-1017,1038
src/apprt/embedded.zig	Inspector	backend	?Backend (=enum{metal})	—(value)	tag only; imgui_impl_metal global state torn down by ImGui_ImplMetal_Shutdown	embedded.zig:999,1010,1060
src/renderer/State.zig	State	inspector	?*Inspector = null	SHARED/BORROW_FIELD	Option<*mut Inspector> aliased into renderer state; owned by core Surface; already an uninhabited placeholder in renderer	State.zig:5,21; lib.rs:31-33 documents the opaque-pointer cast
```

Non-field transient (note, no row): the ImGui **draw-data** is never stored — it is `cimgui.c.ImGui_GetDrawData()` returning a per-frame `*ImDrawData` owned by the ImGui context, consumed immediately by `ImGui_ImplMetal_RenderDrawData` (embedded.zig:1104-1108). Port as a call-local raw pointer, never a field.

---

## 2. Invariant catalog (inspector ⇄ surface/terminal state coupling)

- **INV-1 (mutex-scoped terminal read).** Every terminal deref happens while `surface.renderer_state.mutex` is held; `t = surface.renderer_state.terminal` is a raw pointer valid only inside the lock scope (surface.zig:59-61). All five sub-windows (terminal/surface/keyboard/termio/renderer) draw inside this one lock (surface.zig:57-132). Port: the `*const Terminal`/`*Terminal` must not escape the guard.
- **INV-2 (D5 dirty-tracking is READ-ONLY).** The inspector observes but never clears dirty state: `pin.isDirty()` (pagelist.zig:324), `page.isDirty()` (page.zig:79), `row.dirty` (page.zig:263). No write path touches a dirty bit. Coupling to D5 is pure observation — the port must read these accessors without mutating, and must not perturb the renderer's dirty-clear contract.
- **INV-3 (single terminal write-back seam).** The ONLY inspector→terminal mutation of live state is `t.scrolling_region = override` in layoutTable, guarded by `changed and top<bottom and left<right` (terminal.zig:407-412). Every other "editable" widget (focused checkbox, mode checkboxes, color edits, pending_wrap/protected) reads into a local `var value`/`col` and discards the ImGui edit — they are display-only. Port must preserve that only scrolling_region writes back.
- **INV-4 (mode-state reads).** `t.modes.get(Mode)` drives the modes checkboxes (terminal.zig:600, read-only); `screen.kitty_keyboard.current()` flags + `t.flags.modify_other_keys_2` (threaded as `data.modify_other_keys_2`) drive the keyboard table (screen.zig:119,232,247). All display-only.
- **INV-5 (mouse/protocol reads).** `t.flags.mouse_event/mouse_format/mouse_shift_capture`, `t.mouse_shape` (terminal.zig:436-481) display-only.
- **INV-6 (screen-window lifecycle).** A detached `Screen: …` window can outlive its screen. Each frame draw re-fetches `t.screens.get(entry.key)`; if null (screen deinitialized) the window is removed from the `screens` EnumMap (terminal.zig:83-89), and again if the user closed it (`!screen_open` → remove, terminal.zig:125). Port must re-validate every frame, never cache the `*Screen`.
- **INV-7 (inspector→renderer overlay coupling).** `renderer.Info.features` is written by the inspector's checkboxes (renderer.zig:60-92) and READ by the renderer via `state.inspector.…overlayFeatures(alloc)` (generic.zig:1266, `overlayFeatures` at renderer.zig:25-41 dupes the values into a fresh slice the renderer owns/frees). This is a genuine bidirectional edge: inspector sets, renderer consumes. See Risk R2 for the acyclic port shape.
- **INV-8 (recordPtyRead capture window).** `recordPtyRead` sets `parser_stream.handler.state = {alloc, terminal:t, events:&self.events}` immediately before `parser_stream.nextSlice(data)` and clears it via `defer …state = null` (termio.zig:51-57). The `*const Terminal` and `*VTEvent.Ring` aliases are valid ONLY for that call. Snapshot: VTEvent.init copies `t.screens.active.cursor` and `t.scrolling_region` BY VALUE at event time (termio.zig:454-455) — the rendered cursor (screen.cursorTable(&ev.cursor), termio.zig:230) is a snapshot, not a live alias.
- **INV-9 (ring eviction ordering).** Both EventRing (max 50, Inspector.zig:80-97) and VTEvent.Ring (max 100, termio.zig:783-798) grow-then-evict: on OOM at cap, deinit the oldest element (freeing its utf8/binding/pty, or its arena) BEFORE `deleteOldest(1)`. Port must run element drop before the slot is reused; the CircBuf `undefined` fill maps to the `default:T` at init (circ_buf.rs PORT NOTE, key.rs:45-48).
- **INV-10 (Event ownership).** Event.init shallow-copies the KeyEvent, empties `utf8`, then dupes only if non-empty (key.zig:29-33); the stored Event OWNS its utf8/binding/pty; deinit frees exactly those (key.zig:35-39). Already ported clone-for-dupe (key.rs:94-126).
- **INV-11 (per-Surface ImGui context; global IO).** The ImGui context is 1:1 with a Surface (embedded.zig:992-1032); `ImGui_SetCurrentContext(self.ig_ctx)` precedes every ImGui call (embedded.zig:1018,1036,1049,1113,1129,1143). `CoreInspector.setup()` (Inspector.zig:25-56) mutates the shared IO once: docking + sRGB config flags, disables ini/log files, adds the embedded regular font at `16*2`px. That IO state is process-global ImGui state, not per-inspector.
- **INV-12 (render forces terminal rebuild).** `Inspector.render` always calls `surface.renderer_thread.wakeup.notify()` after `gui.draw` because inspector edits can change terminal state (Inspector.zig:125-131). Seam into the render thread — the wakeup handle is a core/apprt object.
- **INV-13 (termio selection sync).** `selected_event_seq` (keyboard nav) and each `ev.imgui_selected` must stay coherent across frames: nav writes `selected_event_seq`, per-row draw reflects it into `imgui_selected`, and mouse clicks reconcile back (termio.zig:198-216). The reverse iterator MUTATES `ev.imgui_selected` during draw (see Risk R4).

---

## 3. Comptime inventory (what must become generics / macros / hand-expansion)

- **Heavy reflection — VTEvent metadata encoder (termio.zig:575-687).** `encodeMetadata`/`encodeMetadataSingle` switch on `@typeInfo(T)` over arbitrary OSC/parser-action payloads: `.optional`, `.bool`, `.@"enum"` (→ `@tagName`), `.@"union"` (tag_type + per-field `std.mem.eql` match + `@field`), `.@"struct"` (→ `@typeName`), `.int`; with `@compileLog`/`@compileError` on unsupported types. This is the single largest comptime obligation — Rust has no `@typeInfo`, so this must be hand-expanded per concrete `osc.Command`/`Parser.Action` variant (or a derive/visitor). DESIGN.
- **Generic parser stream — `terminal.Stream(VTHandler)`** (termio.zig:707). The VT parser is instantiated over the inspector's handler; Rust needs the handler-as-trait-impl shape the rest of rustty-vt already uses.
- **`inline for` over enum/struct fields:**
  - `@typeInfo(terminal.Parser.Action.Tag).@"enum".fields` — filter-kind checkbox table, skips apc_put/dcs_put (termio.zig:298-311).
  - `@typeInfo(terminal.Mode).@"enum".fields` with `@setEvalBranchQuota(6000)` + `@bitCast(ModeTag.Backing)` — the modes table (terminal.zig:591-616).
  - `@typeInfo(Flags).@"struct".fields` — kitty-keyboard flag rows (screen.zig:249).
  - `@typeInfo(...).fields` in encodeMetadata (above).
  - `std.meta.tags(terminal.ScreenSet.Key)` (terminal.zig:190), `std.meta.tags(terminal.point.Tag)` (pagelist.zig:225,460,471; screen already), `inline for (style_flags)` tuple (style.zig:57), the `keys` tuple in getKeyAction (termio.zig:344).
- **`comptime`-computed values:** `combo_width` comptime block scanning tag name lengths (pagelist.zig:458-464); `regionHelpText(comptime tag)` + `regionRow(pages, comptime tag)` (pagelist.zig:230,262); `std.fmt.comptimePrint` for field names (terminal.zig:613, screen.zig:256).
- **Comptime callback — `detachableHeader(label, state, ctx: anytype, comptime contentFn: fn(@TypeOf(ctx)) void)`** (widgets.zig:150-227). Generic content closure; port as `fn`/`FnOnce` generic or monomorphized per call site. DESIGN.
- **`std.meta.Tag(renderer.Overlay.Feature)`** as the AutoArrayHashMap key type (renderer.zig:12).

---

## 4. Constants list (verbatim values to carry)

- Inspector.zig: `max_capacity = 50` (key ring, :80); font `16 * 2` px, `FontDataOwnedByAtlas = false` (:42-46).
- termio.zig: `max_capacity = 100` (VT ring, :783); `current_seq` starts 1, `+%=1` wrap (:400,725,753); default `filter_exclude = initMany(&.{.print})` (:726); always-skip `.dcs_put`/`.apc_put` (:760); scratch bufs `[256]`/`[128]`/`[96]`/`[64]` (:259,405,425,446).
- key.zig: `actionColor` = press `0x1A4A6F4A`, release `0x1A6A5A5A`, repeat `0x1A4A5A6F` (:468-473); `keyColor` hue `key_int *% 47 / 256.0`, sat `0.5`, val `0.9` (:478-481); row-hover `0x1AFFFFFF` (:461); table col widths `80/160/150/80` (:307-311); action text ImVec4 green/blue/yellow (:329-331).
- surface.zig: window names `"Dear ImGui Demo"/"Keyboard"/"Terminal"/"Surface"/"Terminal IO"/"Renderer"` (:14-19); dock split `0.4` right (:70).
- screen.zig: window names `"Info"/"Cell"/"PageList"` (:12-14); pop-out size `min(400,vp)`×`min(300,vp)` (:100-104).
- widgets.zig: dock split `0.4` (:71) / `0.3` (:172); button `frame_height-4`, padding `4` (:110-111,203-204).
- terminal.zig: `@setEvalBranchQuota(6000)` (:592); palette grid `16×16` (:696-697); scroll clamps `t.rows -| 1` / `t.cols -| 1` (:367,379,391,403).
- units.zig: `toKibiBytes = bytes / 1024` (:1-3).
- Numerous ImVec4 color literals across widgets (active-green `{.4,.9,.4}`, viewport-blue `{.4,.8,1}`, dirty-red `{1,.4,.4}`, garbage-orange `{1,.5,.3}`, warning-yellow `{1,.8,0}`) — carry verbatim (R1 fidelity).

---

## 5. The 13 seams — mechanical vs. design

**Mechanical (5)** — once dcimgui-sys is wired, these are line-for-line ImGui-call + pure-read ports with no new architecture:

| # | Seam | Why mechanical |
|---|------|----------------|
| S4 | key.zig deferred ImGui section — Event.render/renderUtf8/renderPty, modsTooltip, Stream, actionColor/keyColor/hsvToRgba (key.rs:214-684 verbatim body) | pure data + ImGui calls; hsvToRgba is arithmetic; 0 tests |
| S7 | style.zig — `table`, `color` | pure reads of `terminal.Style`; palette optional |
| S13 | page.zig — inspector/treeNode/managedMemory/rowsTable/stylesList/hyperlinksList | pure reads of Page via offset-pointer accessors (`page.rows.ptr(memory)`, `styles.items.ptr`) — same discipline already used in rustty-vt Page |
| S12 | pagelist.zig — Inspector/CellChooser/CellInfo/hyperlink+grapheme tables/scrollbarWidget | mostly pure reads; `PageList.Cell` passed by value; only `pages.scroll(delta_row)` mutates (already-ported PageList method) |
| S6 | screen.zig tables — cursorTable/cursorStyle/keyboardTable/kittyGraphicsTable/internalStateTable/semanticPromptTable | pure reads of Screen/ImageStorage/SemanticPrompt; the anonymous `data` struct → a named params struct |

**Design (8)** — need a decision before/while porting:

| # | Seam | Design question |
|---|------|-----------------|
| S1 | Inspector.zig top-level `Inspector` (setup/init/deinit/rendererInfo/recordKeyEvent/**recordPtyRead**/render) | recordPtyRead sets a self-alias into `self.events` then drives `self.parser_stream` (self-borrow) → route via raw-self helper (Risk R1); render's `surface.renderer_thread.wakeup.notify()` is a core/apprt seam |
| S2 | widgets.zig — helpMarker + DetachableHeader{,State} + **detachableHeader** | the `comptime contentFn: fn(@TypeOf(ctx))void` callback → generic/closure; DockBuilder split state machine |
| S3 | surface.zig aggregate `Inspector` + Mouse + Info | owns all sub-widgets; mutex-scoped `*Terminal` deref must not escape; `Mouse.last_point: ?Pin` raw-aliases pagelist (Risk R3); `renderer.Coordinate.convert` + `pointFromPin` calls |
| S5 | renderer.zig `Info` (features map + overlayFeatures + draw) | `renderer.Overlay.Feature` coupling — must NOT name a renderer type from inspector directly (cycle). Use the accessor-trait pattern (Risk R2) |
| S8 | terminal.zig `Info` + tables | `ScreenMap = EnumMap(Key, screen.Info)` window lifecycle (INV-6); `t.scrolling_region` write-back (INV-3); comptime modes/screens inline-for; DetachableHeader children |
| S9 | termio.zig `Stream` (draw + keyboard nav) | reverse iterator MUTATES `ev.imgui_selected` (Risk R4 — CircBuf needs a mutable reverse iterator); selection/scroll state machine |
| S10 | termio.zig `VTEvent` + encodeAction/encodeOSC/**encodeMetadata**/encodeMetadataSingle | heavy comptime reflection over arbitrary union/struct/enum (§3) — hand-expand per variant; ArenaAllocator.State ownership |
| S11 | termio.zig `VTHandler` + `State` (vtRaw capture) | the `*VTEvent.Ring` self-alias + scoped `*const Terminal` (Risk R1); `terminal.Stream(VTHandler)` generic instantiation |

Adjacent (apprt tier, not in-crate but co-delivered with imgui_impl_metal): `apprt.embedded.Inspector` (embedded.zig:992-1250) — owns `ig_ctx: *ImGuiContext` (GHOSTTY_C) and the `.metal` backend; init/deinit/initMetal/renderMetal/updateContentScale/updateSize + the mouse/key/scroll callbacks. renderMetal's double-frame loop (`for(0..2)`, embedded.zig:1081) and objc release-on-defer are FFI-shaped; DESIGN (objc + Metal + dcimgui-sys three-way).

---

## 6. Risk notes

- **R1 — VTHandler.State self-alias (top SB hazard).** `recordPtyRead` writes `parser_stream.handler.state.events = &self.events` (termio.zig:54), then `parser_stream.nextSlice(data)` (termio.zig:57) takes `&mut self.parser_stream` and, inside `vtRaw`, appends through `state.events` — a raw `*mut` aliasing a *sibling field of the same `Stream`* while that Stream is `&mut`-borrowed. This is exactly the shaper-hook long-lived-raw-alias-plus-interleaved-&mut pattern (memory: shaper-hook-sb-clean-interleave / rust-mut-args-strongly-protected). Port MUST route the append via the alias's own pointer + narrow raw-self helpers (not `&mut self.events` under the outer borrow) and **PROVE it on an isolated non-FFI Miri harness** (precedent `.port/phase-2/mirihar-chunk3b`) — do not hand-reason SB. The `terminal` alias (`*const`) is lower risk (read-only, external owner) but still raw-ptr, never `&`.
- **R2 — renderer.Info.features acyclicity.** inspector→renderer is a real data edge (INV-7) but rustty-inspector must not depend on rustty-renderer's `Overlay.Feature` type by name (would risk a cycle / it is an upper tier). Apply the renderer-config precedent (memory: renderer-config-accessor-trait-acyclic): define the Feature-set surface as a trait the RENDERER implements, or keep the feature enum in a shared lower crate. Decide before S5.
- **R3 — Mouse.last_point Pin invalidation.** `Mouse.last_point: ?Pin` holds `node: *List.Node` into pagelist memory (surface.zig:203; PageList.zig:1226). A reflow/page-eviction between mouse-move and draw invalidates the node; the draw re-validates via `pointFromPin(.active, p)` returning null on failure (surface.zig:379-384, 465-471) — port must preserve the null-guarded re-resolution and treat the stored Pin as a weak/raw handle, never a live borrow.
- **R4 — mutable reverse iterator.** termio Stream draw does `var it = events.iterator(.reverse); while(it.next()) |ev| { ev.imgui_selected = … }` (termio.zig:185-216) — it mutates elements through the reverse iterator. rustty-datastruct CircBuf must expose a mutable reverse iterator (or an index-based mut path); the key.zig Stream draw uses only a forward-mut deinit loop, so this is a new requirement termio surfaces.
- **R5 — comptime reflection fidelity (encodeMetadata).** Hand-expanding §3's `@typeInfo` visitor per concrete OSC/action variant is the highest-effort correctness risk in the tier — the union/struct/enum/optional/int arms and their exact `tag=value` string formats (termio.zig:642-660) are observable in the UI and R1-covered by the 42069-comment / fidelity contract; enumerate the concrete `osc.Command` and `Parser.Action` variants against rustty-vt before writing.
- **R6 — ImGui global/current-context discipline.** Every ImGui call assumes the correct current context (INV-11); in a multi-surface build the dcimgui-sys calls are `!Send`/context-scoped. The port's ImGui-consuming bodies must set-current-context at the same points embedded.zig does, or gate on a single-inspector invariant.
- No VALIDATION_REPORT.md §5 items re-raised; the two existing MEDIUM deferrals it lists (overlayFeatures at generic.rs:4566; surface.rs:2843/2877 inspector seams) are the renderer/core *call sites* of this tier and are consistent with S5/S1 above — not re-filed here.
