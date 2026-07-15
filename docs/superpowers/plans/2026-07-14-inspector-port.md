# Ghostty Inspector Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (or a dynamic Workflow) to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Port the full ghostty terminal inspector (Dear ImGui + Metal, ~4873 LOC of Zig across `Inspector.zig` + 9 widget submodules + the `imgui_impl_metal` backend) to `rustty-inspector` + the apprt/renderer/core seams, so ⌘⌥I opens a functional inspector in Rustty.app.

**Architecture:** Three phases. (1) **Backend** — vendor + wire `imgui_impl_metal.mm/.h` into `dcimgui-sys`, implement the `rustty_inspector_metal_{init,render,shutdown}` C-ABI exports. (2) **Lifecycle** — inhabit the `Inspector` struct (per-Surface ImGui context, INV-11), wire `activate/deactivateInspector` + `initInspector` + `queueInspectorRender` (INV-12 wakeup seam) + the R2 acyclic accessor-trait for the renderer overlay edge. (3) **Widgets** — port the 9 submodules faithfully (R1), with the `@typeInfo` metadata-encoder hand-expansion, the R1 VTHandler-self-alias Miri proof, R3 pin-invalidation, and R4 reverse-iterator-mutation as gated tasks.

**Tech Stack:** Rust, `dcimgui-sys` (Dear ImGui C FFI leaf), `imgui_impl_metal.mm` (ObjC++ Metal backend), objc2/objc2-metal, Miri (Stacked/Tree Borrows), the rustty-vt parser (`Stream<Handler>`), objc2-core-foundation.

## Global Constraints

- **R1 fidelity** — port every line of each oracle `.zig` file; preserve its comments as ported comments; carry the verbatim constants from PREPORT_INSPECTOR.md §4.
- **Oracle** — `/Users/gael/projects/ghostty` @ `4749c4e9` (READ-ONLY): `src/inspector/*` + `src/apprt/embedded.zig` (inspector at 992-1250) + `src/renderer/State.zig`. **Design doc** — `.port/phase-2/PREPORT_INSPECTOR.md` (LIFETIMES rows §1, invariants §2 INV-1..13, comptime inventory §3, constants §4, hazards §5 R1..R4). Every task cites its INV/LIFETIMES/hazard rows.
- **R4 acyclicity** — `rustty-inspector` and `rustty-renderer` must NOT name apprt types; the inspector↔renderer overlay edge (INV-7) is crossed by a renderer-defined **accessor trait** impl'd for the inspector (the RendererConfig precedent), NOT a direct type edge.
- **panic=abort profile** — assert-class failures abort (Zig-trap parity); recoverable failures return a sentinel, never `catch_unwind`.
- **Main-thread-affine** — all ImGui + Metal + the ImGui context are `!Send`, main-thread only (INV-11). objc ids released per the invariant (I16-style +1/release balance).
- **INV-1 mutex scope** — every terminal deref happens while `surface.renderer_state.mutex` is held; the `*const Terminal`/`*Terminal` raw pointer must not escape the guard.
- **INV-3 single write-back** — the ONLY inspector→terminal live-state mutation is `t.scrolling_region = override` in `terminal.zig` layoutTable; every other editable widget reads into a local and discards the ImGui edit (display-only).
- **SB-class fixes PROVEN on a mirihar harness** — any raw-alias/self-alias hazard (R1 VTHandler self-alias, R3 pin) is proven on an isolated non-FFI Miri harness under `.port/phase-2/mirihar-inspector-*`, not hand-reasoned. See [[shaper-hook-sb-clean-interleave]] doctrine.
- **Git** — explicit paths; `git mv` for renames; retry on `index.lock` (sleep 3, ≤10×); never push/reset/stash. Leave the (now to-be-tracked) imgui backend files as this plan directs.
- **Build gate** — `cargo build --workspace` stays green after every task; the inspector crate + apprt-c + the touched crates build.

---

## File Structure

- `crates-sys/dcimgui-sys/` — vendor `vendor/imgui/backends/imgui_impl_metal.{mm,h}` (present, untracked), compile them in `build.rs`, expose `ImGui_ImplMetal_{Init,NewFrame,RenderDrawData,Shutdown,CreateDeviceObjects,DestroyDeviceObjects}` FFI in `ext.cpp` + `src/lib.rs`.
- `crates/rustty-apprt-c/src/lib.rs` — the `rustty_inspector_metal_{init,render,shutdown}` + `rustty_inspector_{set_size,set_content_scale}` + `rustty_surface_inspector` (:2192) + `rustty_inspector_free` (:2217) exports.
- `crates/rustty-apprt/src/embedded.rs` — the `Inspector` struct (ports embedded.zig:992-1250), `redrawInspector` (:514), its ImGui-context lifecycle + Metal frame loop.
- `crates/rustty-core/src/surface.rs` — `activateInspector`/`deactivateInspector` (the verbatim-comment seam), `initInspector`, `queueInspectorRender` (INV-12), the keyCallback inspector defer (:1524), the `inspector: Option<*mut inspectorpkg::Inspector>` field (:395).
- `crates/rustty-renderer/src/generic.rs:4600` — the `overlayFeatures` consumer (INV-7); `crates/rustty-render-types` / `State.inspector` — the R2 accessor-trait definition.
- `crates/rustty-inspector/src/` — `lib.rs` (the top-level `Inspector` shell, currently `pub enum Inspector {}`), `widgets.rs`, `units.rs`, `widgets/{key,style,renderer,screen,page,pagelist,surface,terminal,termio}.rs`, `Inspector.rs` (the aggregate).
- `.port/phase-2/mirihar-inspector-*` — isolated Miri harnesses for R1 (self-alias) + R3 (pin).

---

## Phase 1 — Backend (ImGui_ImplMetal)

### Task 1: Vendor + compile `imgui_impl_metal` into dcimgui-sys

**Files:**
- Track (git add): `crates-sys/dcimgui-sys/vendor/imgui/backends/imgui_impl_metal.{mm,h}` (present, untracked)
- Modify: `crates-sys/dcimgui-sys/build.rs` (compile the `.mm` under an `objc` + Metal cfg), `crates-sys/dcimgui-sys/Cargo.toml` (a `metal-backend` feature or macos-cfg), `crates-sys/dcimgui-sys/ext.cpp` (include `backends/imgui_impl_metal.h` under `__has_include`, mirroring the existing opengl3 block at ext.cpp:39-50)

**Interfaces:**
- Produces: a dcimgui-sys that compiles `imgui_impl_metal.mm` (ObjC++, `-x objective-c++`, linking Metal + QuartzCore) on macOS.

- [ ] **Step 1: Confirm the reference's build wiring.** Read how ghostty compiles the Metal backend (`/Users/gael/projects/ghostty/src/build/` cimgui/imgui steps) — it compiles `imgui_impl_metal.mm` with ARC-off ObjC++ against the imgui headers. Match that: `cc::Build` with `.file("vendor/imgui/backends/imgui_impl_metal.mm").flag("-x").flag("objective-c++").flag("-fno-objc-arc")`, `.include("vendor/imgui")`, macOS-only (`#[cfg(target_os=…)]` in build.rs), link `framework=Metal`, `framework=QuartzCore`, `framework=Foundation`.
- [ ] **Step 2: Add the `__has_include` block** for `imgui_impl_metal.h` in `ext.cpp` (mirror the opengl3 block at :39-50) so the C wrapper surface can reach it.
- [ ] **Step 3: Build.** `cargo build -p dcimgui-sys` on macOS → compiles the `.mm` clean. Expected: green; `nm` of the built lib shows `_ImGui_ImplMetal_Init` etc.
- [ ] **Step 4: Commit** (`inspector: vendor + compile imgui_impl_metal into dcimgui-sys (Task 1)`).

### Task 2: Expose the ImGui_ImplMetal FFI in dcimgui-sys

**Files:**
- Modify: `crates-sys/dcimgui-sys/ext.cpp` (a C wrapper `extern "C"` surface for the ImplMetal calls the port needs, if the raw ObjC++ signatures aren't directly bindgen-able), `crates-sys/dcimgui-sys/src/lib.rs` (the `extern "C"` decls / bindgen include)

**Interfaces:**
- Produces: `dcimgui_sys::ImGui_ImplMetal_Init(device: *mut c_void) -> bool`, `_NewFrame(render_pass_descriptor: *mut c_void)`, `_RenderDrawData(draw_data: *mut ImDrawData, command_buffer: *mut c_void, command_encoder: *mut c_void)`, `_Shutdown()`, `_CreateDeviceObjects(device)`, `_DestroyDeviceObjects()` — the exact set embedded.zig's inspector calls (ghostty embedded.zig:1090-1145). `id<MTLDevice>` etc. cross as `*mut c_void` (objc id).

- [ ] **Step 1: Enumerate the exact ImplMetal calls** the reference inspector makes: read `/Users/gael/projects/ghostty/src/apprt/embedded.zig` inspector (992-1250) — the `ImGui_ImplMetal_Init(device)`, `_NewFrame(descriptor)`, `_RenderDrawData(draw_data, command_buffer, command_encoder)`, `_Shutdown()`. List the exact C signatures (objc ids as opaque pointers).
- [ ] **Step 2: Wire the FFI.** If bindgen can't emit the ObjC++ signatures, add thin `extern "C"` C-ABI shims in `ext.cpp` (`dcimgui_ImplMetal_Init(void* device)` → `ImGui_ImplMetal_Init((__bridge id<MTLDevice>)device)`) and declare them in `src/lib.rs`. Keep the shim names stable — the apprt-c metal exports (Task 3) call these.
- [ ] **Step 3: Build + link-test.** `cargo build -p dcimgui-sys`; a `#[test]` that references each FFI symbol so a missing symbol is a link error (no runtime Metal needed).
- [ ] **Step 4: Commit** (`inspector: dcimgui-sys ImGui_ImplMetal FFI surface (Task 2)`).

### Task 3: Implement `rustty_inspector_metal_{init,render,shutdown}` in apprt-c

**Files:**
- Modify: `crates/rustty-apprt-c/src/lib.rs` — the `rustty_inspector_metal_init` / `_metal_render` / `_metal_shutdown` exports (currently `unimplemented!()` E4 sinks; header docs at :36)

**Interfaces:**
- Consumes: the dcimgui-sys ImplMetal FFI (Task 2), the `Inspector` (Task 4 — but the metal exports operate on the ImGui context + Metal handles, so they can land before the widget bodies; they need the `Inspector`'s `ig_ctx` + `backend` fields, which Task 4 provides — sequence Task 4's struct fields before this, or land the struct-fields subset first).
- Produces: `rustty_inspector_metal_init(ptr: *mut Inspector, device: *mut c_void) -> bool`, `rustty_inspector_metal_render(ptr, command_buffer, descriptor) -> bool`, `rustty_inspector_metal_shutdown(ptr)` — faithful ports of ghostty embedded.zig `ghostty_inspector_metal_init/render/shutdown` (:2225/2229/2243).

- [ ] **Step 1: Read the reference exports** `ghostty_inspector_metal_init/render/shutdown` (embedded.zig ~2225-2250): init calls `ImGui_ImplMetal_Init` after `ImGui_SetCurrentContext(self.ig_ctx)`; render does `ImGui_ImplMetal_NewFrame(descriptor)` + `Inspector.render` (the draw) + `ImGui_ImplMetal_RenderDrawData(GetDrawData(), command_buffer, command_encoder)`; shutdown does `ImGui_ImplMetal_Shutdown`. Objc ids released per INV/I16.
- [ ] **Step 2: Port the three bodies faithfully** (R1, ported comments), replacing the `unimplemented!()`. Main-thread-affine; `ImGui_SetCurrentContext(self.ig_ctx)` precedes every ImGui call (INV-11). The passed `device`/`command_buffer`/`descriptor` are objc ids the host owns — do NOT over-release.
- [ ] **Step 3: Verify.** `cargo build -p rustty-apprt-c` green. A KAT is impractical without a live Metal device — verify via the app shakedown (Task 16); at minimum a compile + the FFI link.
- [ ] **Step 4: Commit** (`inspector: port metal init/render/shutdown exports (Task 3)`).

---

## Phase 2 — Lifecycle

### Task 4: Inhabit the `Inspector` struct + per-Surface ImGui context (embedded.rs)

**Files:**
- Modify: `crates/rustty-apprt/src/embedded.rs` — the `Inspector` struct (ports embedded.zig:992-1250), its `init`/`deinit`, `redrawInspector` (:514→:517)
- Modify: `crates/rustty-inspector/src/lib.rs` — replace `pub enum Inspector {}` with the inhabited top-level `Inspector` shell (LIFETIMES §1 rows: `gui`, `surface` back-ref, `ig_ctx` GHOSTTY_C, `backend`)

**Interfaces:**
- Consumes: `dcimgui_sys` context calls (`ImGui_CreateContext`/`DestroyContext`, `ImGui_SetCurrentContext`, IO setup).
- Produces: `apprt::embedded::Inspector` with `surface: *mut Surface` (BORROW_FIELD, LIFETIMES row), `ig_ctx: *mut ImGuiContext` (GHOSTTY_C, owned via Create→Destroy), `backend: Option<Backend>` (enum{metal} tag). `rustty_inspector::Inspector` becomes the inhabited aggregate (owns key_stream/vt_stream/renderer_info per Inspector.zig:22).

- [ ] **Step 1: Port `apprt.embedded.Inspector`** (embedded.zig:992-1250) — the struct fields (LIFETIMES §1: surface/ig_ctx/backend), `init` (creates ImGui context, `CoreInspector.setup()` the shared IO once — docking + sRGB + no ini/log + embedded font @ 16*2px, INV-11), `deinit` (`ImGui_ImplMetal_Shutdown` if backend, `ImGui_DestroyContext`). Ported comments.
- [ ] **Step 2: Inhabit `rustty_inspector::Inspector`** (Inspector.zig — the `gui: widgets.surface.Inspector` inline aggregate owning key_stream/vt_stream/renderer_info). Keep the crate R4-clean (no apprt type names).
- [ ] **Step 3: Wire `redrawInspector`** (embedded.rs:514) to the real `queueInspectorRender` (Task 6).
- [ ] **Step 4: Verify.** `cargo build -p rustty-apprt -p rustty-inspector -p rustty-apprt-c` green (the metal exports from Task 3 now compile against the real fields).
- [ ] **Step 5: Commit** (`inspector: inhabit Inspector struct + per-Surface ImGui context (Task 4)`).

### Task 5: R2 accessor-trait for the inspector↔renderer overlay edge

**Files:**
- Modify: `crates/rustty-renderer/src/generic.rs:4600` (the `overlayFeatures` consumer) + a renderer-defined accessor trait (mirror the `RendererConfig` precedent — see [[renderer-config-accessor-trait-acyclic]])
- Modify: `crates/rustty-inspector/src/widgets/renderer.rs` (impl the trait; `overlayFeatures` dupes the values into a fresh slice the renderer owns — renderer.zig:25-41)

**Interfaces:**
- Consumes: `State.inspector: Option<*mut Inspector>` (State.zig, renderer already has the opaque-pointer placeholder at generic.rs:6514/6616).
- Produces: a renderer-defined trait (e.g. `InspectorOverlay { fn overlay_features(&self) -> Vec<crate::overlay::Feature>; }`) impl'd by the inspector's `renderer::Info` — the renderer calls it through the trait object, never naming a `rustty-inspector` type. INV-7 bidirectional edge (inspector checkboxes write `Info.features`, renderer reads via the trait).

- [ ] **Step 1: Define the trait in the renderer** (acyclic — the renderer owns the trait; the inspector impls it). The `overlayFeatures(alloc)` semantics (renderer.zig:25-41): dupe `Info.features` values into a fresh owned `Vec<Feature>`.
- [ ] **Step 2: Replace generic.rs:4600's `unimplemented!()`/stub** with the live `match state.inspector { Some(insp) => insp.overlay_features(), None => vec![] }` via the trait (the verbatim body is in the ported comment at 4602-4610).
- [ ] **Step 3: Verify acyclicity** — `cargo build --workspace`; confirm no `rustty-renderer → rustty-inspector` or `rustty-inspector → rustty-apprt` dep edge was added (`cargo tree` / the R4 acyclic gate `.port/tools/gates/check_acyclic.sh`).
- [ ] **Step 4: Commit** (`inspector: R2 acyclic accessor-trait for renderer overlay edge (Task 5)`).

### Task 6: Core lifecycle — activate/deactivate/initInspector + queueInspectorRender (INV-12)

**Files:**
- Modify: `crates/rustty-core/src/surface.rs` — `activateInspector`/`deactivateInspector` (the verbatim-comment seam at :25), `initInspector`, `queueInspectorRender`, the `inspector` field (:395 already `Option<*mut inspectorpkg::Inspector>`), the keyCallback inspector defer (:1524)

**Interfaces:**
- Consumes: `rustty_inspector::Inspector` (Task 4), `redrawInspector` (apprt).
- Produces: `Surface::activateInspector` (constructs the Inspector, wires it onto `renderer_state.inspector` + the render-thread mailbox — the verbatim comment shows the exact push), `deactivateInspector` (tears down), `queueInspectorRender` (INV-12: after `gui.draw`, always `renderer_thread.wakeup.notify()` because inspector edits can change terminal state).

- [ ] **Step 1: Port `activateInspector`/`deactivateInspector`** from the preserved verbatim Zig comments — construct/destroy the `inspectorpkg::Inspector`, wire onto `renderer_state.inspector` + push to the render-thread mailbox (the comment shows the exact push).
- [ ] **Step 2: Port `initInspector` + `queueInspectorRender`** — INV-12 wakeup seam (Inspector.zig:125-131: after draw, notify the render-thread wakeup handle). The keyCallback inspector defer (:1524) records key events into the inspector's key stream when an inspector exists.
- [ ] **Step 3: Verify.** `cargo build --workspace`; `cargo test -p rustty-core` where a lifecycle KAT is feasible (activate→field-set→deactivate→field-null).
- [ ] **Step 4: Commit** (`inspector: core activate/deactivate/init + queueInspectorRender wakeup seam (Task 6)`).

---

## Phase 3 — Widgets (one task per submodule + the gated design/hazard tasks)

Port order = dependency order: shared helpers first, then leaf widgets, then aggregators, with `termio` (the metadata-encoder design + R1 Miri) last.

### Task 7: Shared helpers — `widgets.rs` + `units.rs`

**Files:** Create/fill `crates/rustty-inspector/src/widgets.rs` (currently 24-line stub) + `crates/rustty-inspector/src/units.rs`.
**Interfaces:** Produces `detachableHeader(label, state, ctx, content_fn)` (widgets.zig:150-227 — the comptime callback, §3: port as a generic `FnOnce`/monomorphized closure), the units helpers (units.zig:3).
- [ ] **Step 1:** Port `widgets.zig` (227 LOC) — `detachableHeader` (generic content closure → Rust generic `fn`/`FnMut`), the shared table/window helpers. Port `units.zig`.
- [ ] **Step 2:** Verify `cargo build -p rustty-inspector`. Commit (`inspector: widgets.rs + units.rs shared helpers (Task 7)`).

### Task 8: `widgets/key.rs` ImGui half

**Files:** Modify `crates/rustty-inspector/src/widgets/key.rs` (the pure-data `Event`/`EventRing`/`label()` half is LIVE; add the ImGui draw half). Oracle: `key.zig` (535). LIFETIMES rows: Event.binding/pty/event.utf8 (OWNED), Stream.events (CircBuf, INV-9 eviction). INV-10 Event ownership already ported.
- [ ] **Step 1:** Port the ImGui draw half of `key.zig` (the keyboard-event table) against dcimgui-sys, preserving the live data half. §3 `inline for` over the key tuple (getKeyAction). Commit (`inspector: key.rs ImGui draw half (Task 8)`).

### Task 9: `widgets/style.rs`

**Files:** Create `crates/rustty-inspector/src/widgets/style.rs`. Oracle: `style.zig` (125). §3 `inline for (style_flags)` tuple.
- [ ] Port faithfully (R1). Build. Commit (`inspector: style.rs widget (Task 9)`).

### Task 10: `widgets/renderer.rs`

**Files:** Modify `crates/rustty-inspector/src/widgets/renderer.rs` (created in Task 5 for the trait impl — now port the full widget). Oracle: `renderer.zig` (123). LIFETIMES: Info.features (OWNED map, INV-7). §3 `std.meta.Tag(Overlay.Feature)` map key.
- [ ] Port the renderer-info widget (checkboxes writing `Info.features`, INV-7); the `overlayFeatures` dupe was done in Task 5 — now the draw half. Build. Commit (`inspector: renderer.rs widget (Task 10)`).

### Task 11: `widgets/screen.rs`

**Files:** Create `crates/rustty-inspector/src/widgets/screen.rs`. Oracle: `screen.zig` (400). LIFETIMES: Info.draw(data).screen / color_palette (BORROW_PARAM, call-scoped under mutex, INV-1). INV-4 mode reads (kitty_keyboard flags), INV-6 screen-window lifecycle re-fetch. §3 `@typeInfo(Flags).@"struct".fields` kitty-keyboard rows, `comptimePrint` field names.
- [ ] Port faithfully. The `*Screen`/`*DynamicPalette` are per-frame call-scoped (never cached, INV-1/INV-6). Build. Commit (`inspector: screen.rs widget (Task 11)`).

### Task 12: `widgets/page.rs`

**Files:** Create `crates/rustty-inspector/src/widgets/page.rs`. Oracle: `page.zig` (428). INV-2 dirty READ-ONLY (`page.isDirty()`, `row.dirty` — observe never clear).
- [ ] Port faithfully; dirty reads are pure observation (INV-2). Build. Commit (`inspector: page.rs widget (Task 12)`).

### Task 13: `widgets/pagelist.rs` (comptime-heavy)

**Files:** Create `crates/rustty-inspector/src/widgets/pagelist.rs`. Oracle: `pagelist.zig` (852 — largest widget). INV-2 dirty read (`pin.isDirty()`). §3 comptime: `std.meta.tags(point.Tag)`, `combo_width` comptime block (scan tag-name lengths → const), `regionHelpText(comptime tag)` + `regionRow(pages, comptime tag)` (monomorphize per tag).
- [ ] **Step 1:** Port the comptime obligations by hand-expansion/monomorphization per the §3 inventory (the `comptime`-computed `combo_width`, the per-tag `regionRow`/`regionHelpText`). Preserve the verbatim `point.Tag` set.
- [ ] **Step 2:** Build. Commit (`inspector: pagelist.rs widget + comptime hand-expansion (Task 13)`).

### Task 14: `widgets/terminal.rs` (comptime modes table + INV-3 write-back)

**Files:** Create `crates/rustty-inspector/src/widgets/terminal.rs`. Oracle: `terminal.zig` (726). LIFETIMES: Info.screens (EnumMap of owned screen.Info, INV-6 re-validate each frame). **INV-3: the ONLY write-back — `t.scrolling_region = override` in layoutTable, guarded `changed and top<bottom and left<right`.** §3 comptime: `@typeInfo(Mode).@"enum".fields` with `@setEvalBranchQuota(6000)` + `@bitCast(ModeTag.Backing)` (modes table), `std.meta.tags(ScreenSet.Key)`, `comptimePrint`.
- [ ] **Step 1:** Port the modes table by hand-expanding the `inline for` over `Mode` enum fields (INV-4 `t.modes.get` display-only). Port the ScreenSet.Key EnumMap with the INV-6 per-frame re-fetch/removal.
- [ ] **Step 2:** Port layoutTable with the INV-3 single write-back (`scrolling_region` override, guarded) — EVERY other editable widget reads-into-local-and-discards (display-only). This is the one place the inspector mutates live terminal state.
- [ ] **Step 3:** Build. Commit (`inspector: terminal.rs widget + modes table + INV-3 scrolling_region write-back (Task 14)`).

### Task 15: `widgets/surface.rs` + R3 pin-invalidation hazard

**Files:** Create `crates/rustty-inspector/src/widgets/surface.rs`. Create `.port/phase-2/mirihar-inspector-pin/`. Oracle: `surface.zig` (504). LIFETIMES: Inspector.draw.surface (BORROW_PARAM, all reads under `renderer_state.mutex`, INV-1), **Mouse.last_point (BORROW_FIELD `?terminal.Pin` — R3: raw-aliases pagelist memory, INVALIDATED by reflow)**.
- [ ] **Step 1: R3 proof.** The `Mouse.last_point: Option<Pin>` raw-aliases PageList memory that reflow invalidates. Prove the port's pin-handling is SB-clean on an isolated `mirihar-inspector-pin` harness (Pin held across a reflow-like `&mut PageList` → must re-validate or drop, not deref stale). Only then port.
- [ ] **Step 2:** Port `surface.zig` — the top-level surface inspector window (aggregates terminal/keyboard/termio/renderer sub-windows, all under one mutex lock, INV-1). Build.
- [ ] **Step 3:** `cargo +nightly miri test` the pin harness green. Commit (`inspector: surface.rs widget + R3 pin Miri proof (Task 15)`).

### Task 16: `widgets/termio.rs` — metadata-encoder design + R1 self-alias Miri + R4

**Files:** Create `crates/rustty-inspector/src/widgets/termio.rs`. Create `.port/phase-2/mirihar-inspector-vthandler/`. Oracle: `termio.zig` (811 — the design-heaviest widget). LIFETIMES: **VTHandler.State.events (BORROW_FIELD `*mut VTEvent.Ring` SELF-ALIAS of Stream.events — R1), State.terminal (BORROW_FIELD call-scoped, INV-8), VTEvent.{arena_state,raw_description,metadata} (OWNED, arena-backed)**. INV-8 recordPtyRead capture window, INV-9 ring eviction, **INV-13 termio selection sync (R4: reverse iterator MUTATES `ev.imgui_selected` during draw)**.
- [ ] **Step 1: The metadata-encoder hand-expansion (the single largest design task, §3).** `encodeMetadata`/`encodeMetadataSingle` switch on `@typeInfo(T)` over arbitrary OSC/parser-action payloads (`.optional`/`.bool`/`.enum`→tagName/`.union`/`.struct`/`.int`). Rust has no `@typeInfo` — hand-expand per concrete `osc.Command`/`Parser.Action` variant (a visitor/`match` over the real rustty-vt variants). DESIGN: enumerate every concrete variant the parser produces and its metadata encoding, faithful to the reference's per-type output.
- [ ] **Step 2: R1 self-alias Miri proof.** `VTHandler.State.events = &self.events` is a self-alias of a sibling field of the same Stream, appended-through during `nextSlice` (INV-8 capture window). Route the append via the alias's OWN pointer + raw-self helpers (the [[shaper-hook-sb-clean-interleave]] doctrine); PROVE on `mirihar-inspector-vthandler` (long-lived raw alias into a field + interleaved `&mut` to that field = SB/TB UB) before shipping.
- [ ] **Step 3:** Port `termio.zig` — the `Stream<VTHandler>` parser (§3 generic-parser-as-trait, the handler impls the rustty-vt handler trait), the VTEvent ring (INV-9 eviction: element drop before slot reuse), the arena-backed VTEvent (raw_description/metadata in the event's arena, INV-8 snapshot cursor/scrolling_region BY VALUE at event time), the R4 selection sync (reverse iterator mutates `imgui_selected` — port the mutation-during-draw faithfully).
- [ ] **Step 4:** `cargo +nightly miri test` the vthandler harness green; `cargo build -p rustty-inspector`. Commit (`inspector: termio.rs + metadata-encoder hand-expansion + R1 self-alias Miri proof (Task 16)`).

### Task 17: `Inspector.rs` aggregate + wire the top-level draw

**Files:** Create `crates/rustty-inspector/src/Inspector.rs` (or fold into lib.rs). Oracle: `Inspector.zig` (131), `main.zig` (8).
**Interfaces:** Produces `Inspector::render` — sets current ImGui context (INV-11), draws `gui` (the surface.Inspector aggregate = all sub-windows), then `renderer_thread.wakeup.notify()` (INV-12). Ties key_stream/vt_stream/renderer_info together.
- [ ] Port `Inspector.zig` — the top-level `render` (INV-11 SetCurrentContext, INV-12 wakeup), the aggregate that owns the streams + info + the surface.Inspector gui. Build `cargo build --workspace`. Commit (`inspector: Inspector.rs aggregate + top-level render (Task 17)`).

### Task 18: End-to-end shakedown — ⌘⌥I opens the inspector

**Files:** none (verification); may fix wiring surfaced by the live build.
- [ ] **Step 1:** `cargo build --workspace` green; `bash macos/build-rustty.sh` → Rustty.app.
- [ ] **Step 2:** Launch Rustty.app, press ⌘⌥I. **Screenshot-verify** (do NOT claim from process tree) the inspector opens with live panels (terminal/screen/cursor/keyboard/termio/modes/cell). Type input → the keyboard-events panel populates; run output → the termio VT-events panel populates.
- [ ] **Step 3:** Confirm INV-3 (a scrolling-region edit in the terminal panel takes effect) and INV-12 (edits trigger a re-render). Commit any wiring fixes (`inspector: end-to-end shakedown — ⌘⌥I inspector functional (Task 18)`).

---

## Self-Review

**1. Spec coverage** (against PREPORT_INSPECTOR.md):
- Backend (imgui_impl_metal vendor+wire+FFI+exports) → Tasks 1-3. ✅
- Lifecycle (Inspector struct/context INV-11, R2 accessor-trait, activate/deactivate/init/queueInspectorRender INV-12) → Tasks 4-6. ✅
- 9 widget submodules → Tasks 8-16 (+ shared helpers Task 7, aggregate Task 17). ✅
- The §3 comptime obligations: metadata encoder → Task 16; modes table → Task 14; pagelist comptime → Task 13; detachableHeader → Task 7; inline-fors → their owning widget tasks. ✅
- Hazards: R1 self-alias Miri → Task 16; R2 acyclic → Task 5; R3 pin Miri → Task 15; R4 reverse-iter → Task 16. ✅
- Invariants INV-1..13 → cited in their owning tasks (INV-1 mutex Task 11/15, INV-2 dirty Task 12/13, INV-3 write-back Task 14, INV-7 overlay Task 5/10, INV-8/9/13 Task 16, INV-11 Task 4/17, INV-12 Task 6/17, INV-6 Task 11/14). ✅

**2. Placeholder scan:** No "TBD"/"handle edge cases". Port tasks reference the exact oracle file + LOC + the §-rows; the "code" is the faithful port (this is a port, not greenfield — literal Rust would duplicate the oracle). The design tasks (metadata encoder Task 16 §Step-1, comptime Task 13/14) name the exact reflection sites to hand-expand. Acceptable for a port plan.

**3. Type/name consistency:** `Inspector` (apprt struct + rustty-inspector aggregate — distinct, named per crate), `ig_ctx`/`backend`/`surface` fields (LIFETIMES §1), `overlay_features()` trait method (Task 5→10), `queueInspectorRender`/`activateInspector`/`deactivateInspector` (Task 6→17), the dcimgui-sys ImplMetal FFI names (Task 2→3). Consistent.

**Sequencing for the Workflow:** Phase 1 (T1→T2→T3) strictly sequential (FFI chain). Phase 2: T4 before T5/T6 (needs the struct); T5 and T6 can parallelize (different crates). Phase 3: T7 first (shared helpers), then T8-T16 mostly parallel BUT termio (T16) + surface (T15) are the hazard-gated heavies (Miri) and terminal (T14) has the write-back — run those with care; T17 aggregates (needs all widgets); T18 last (shakedown, needs everything). The widget tasks touch DISTINCT files (one submodule each) so they parallelize cleanly; only the aggregate (T17) + lib.rs mod-wiring serialize.
