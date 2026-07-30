# Phase 3 assessment — area F: UI tiers (apprt / renderer / font / inspector / macOS)

Range `4749c4e93..6ad1fe7d8` (2026-06-20 → 2026-07-29). Scope:

```
src/apprt/**     +1117 / -376  (30 files)   src/font/**      +568 / -355  (60 files)
src/renderer/**   +433 / -166  (13 files)   src/inspector/** +254 /  -71  ( 7 files)
macos/**          +356 /  -50  (11 files)
```

Rustty counterparts: `crates/rustty-apprt/`, `crates/rustty-renderer/`,
`crates/rustty-core/src/renderer_seam.rs` (the render Thread), `crates/rustty-font/`,
`crates/rustty-font-shaper/`, `crates/rustty-inspector/`, `macos/`.

## Coverage statement (read this before trusting any number)

**Exhaustive:**

- Every commit in the range touching these five paths was enumerated
  (`git log --oneline --no-merges 4749c4e93..origin/main -- src/apprt src/font
  src/renderer src/inspector macos` → 42 commits) and every one is accounted for
  in the table below.
- The complete text diff of all four `src/**` sub-trees was read line-by-line
  (`src/font` 2,605 lines excl. PNGs, `src/renderer` 1,208, `src/inspector` 619,
  `src/apprt` 2,801) plus the entire `macos/` diff (567 lines). No hunk was skipped.
- `src/font`'s 60 files were enumerated individually with per-file commit
  attribution (`git log --name-only`), so the "many small files" risk is closed:
  **54 of the 60 are the Zig 0.16 / std.Io migration or PNG regolds**; only 4
  source files carry semantics (`sprite/Face.zig`, `sprite/canvas.zig`,
  `sprite/draw/powerline.zig`, `opentype/svg.zig`).
- Every rustty verdict below carries a `file:line` + symbol, or the exact grep
  that returned nothing.

**Sampled (stated as such, not presented as complete):**

- **PNG goldens.** 24 sprite testdata PNGs + `glyf_rasterize.png` changed at byte
  level. I pixel-decoded 4 pairs (Pillow): `U+2800…-11x21+2` across the whole
  range = **pixels identical** (encoder-only churn), `glyf_rasterize.png` across
  the whole range = **pixels identical**, and `U+E000…-11x21+2` /
  `U+2500…-11x21+2` across `d0e72a3ab^..d0e72a3ab` = **76/119040 and 3/119040
  pixels differ, max delta 48 / 16**. I did not decode the other 20. The
  structural argument that closes the gap: `d0e72a3ab` is the *only* rendering
  change in `src/font` in the range, so its 6-file stat is the authoritative
  pixel-affected set; everything else is `z2d.png_exporter` output churn from the
  0.16 `std.Io` signature change.
- **rustty-side "N/A (mechanical)" rows.** For the ~50 files whose only upstream
  change is `std.Thread.Mutex`→`std.Io.Mutex`, `ArrayListUnmanaged`→`ArrayList`,
  `@Type`→`@Struct`/`@Union`/`@Fn`/`@Int`/`@Enum`, `std.fs`→`std.Io.Dir`,
  `fixedBufferStream`→`std.Io.Reader/Writer`, `std.meta.intToEnum`→
  `std.enums.fromInt`, `@cImport`→translate-c module, I classified from the diff
  text and did **not** re-verify each corresponding rustty file. Two of these
  were spot-checked and found already-equivalent (`Inspector.rs:260-265` had
  already elided the dead `else => return err` arm; `svg.rs` already had the
  bounds check). Residual risk: a semantic change hiding inside a mechanical
  hunk. The one such case I did find by reading (`gtk/class/surface.zig`
  `defaultTermioEnv`'s inlined flatpak check) proved behaviour-preserving —
  `src/os/env.zig:10-15` at the pin already did `if (isFlatpak()) EnvMap.init`.

---

## 1. Summary

29 distinct **functional** upstream changes (excluding pure language/std migration).

| Sub-tier | ABSENT | PARTIAL | PORTED | N/A (mechanical) |
|---|---|---|---|---|
| `src/apprt` | 4 | 1 | 0 | ~20 files (0.16 std.Io + translate-c + build tooling) |
| `src/font` | 1 | 1 | 1 | 54 of 60 files (0.16 sweep + 19 encoder-only PNG regolds) |
| `src/renderer` | 3 | 0 | 3 | 7 files (0.16 + comment deletions) |
| `src/inspector` | 0 | 2 | 2 | 3 files (0.16 writer/ptrCast) |
| `macos` | 11 | 0 | 0 | 0 |
| **Total** | **19** | **4** | **6** | — |

Headline reads:

- **`macos/` is 0% ported for this range — 11/11 changes absent.** Every one is
  Swift-side only; none needs Rust work. This is the single largest block of
  cheap, directly-portable, unit-testable work in the area (2 of the 11 arrive
  with 285 lines of upstream Swift tests).
- **The renderer's three heavyweight series are genuinely ported with real
  gates** (mutex handoff, compression scheduling, kitty generation stamps) — this
  is where the pre-Phase-3 opportunistic porting actually held up. But three
  smaller renderer changes were dropped, including a **real GPU
  over-allocation bug that rustty deliberately reproduced as a "faithfulness
  quirk"** and upstream has now fixed.
- **`src/font`'s 60-file scare is real but small**: 1 ported, 1 partial, 1 absent
  — the absent one (`z2d` native path insetting) is the area's second-biggest
  cost because it needs a **z2d 0.10.0 → 0.12.x feature port in
  `crates/rustty-z2d`** before the font change can even compile.
- **The Wayland hotkey path is wholly absent** and is additive-with-preference
  over rustty's shipped portal implementation — see §3.
- **One latent defect found in already-merged rustty test code** (§5, confirmed
  empirically), unrelated to the upstream range.

---

## 2. Per-change verdict table

Legend for evidence: `→` = rustty location. "grep ∅" = the named grep over
`crates/` (and `macos/` for Swift) returned no hit.

### 2a. `src/apprt`

| Upstream commit(s) | What changed | Status | Evidence | Platform | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|---|
| `88bd4fdce` `c3b5cab94` `7ee3ac9ec` `0075c75b6` `3024c5d19` | **vicinae-hotkey-v1 global hotkeys.** NEW `winproto/wayland/Hotkeys.zig` (164) + `protocols/vicinae-hotkey-v1.xml` (305); `winproto.zig:82-100` `App.bindGlobalShortcuts`/`clearGlobalShortcuts`; `noop.zig:40-47`+`x11.zig:162-171` false arms; `wayland.zig:31,54,68-79` hotkeys field + arms; `Globals.zig:36,46` `vicinae_hotkey_manager` tag; `global_shortcuts.zig:86-101` `BindFailed` boxed type, `:117-126` `bind-failed` signal, `:190-197` winproto-first in `startup`, `:1235-1253` public `emitTrigger`/`emitBindFailed`; `application.zig:1475-1481,1691-1725` connect + desktop-notification handler; `key.zig:60-67` `keysymFromTrigger` + test `:340-360` | **ABSENT** (whole path) | grep ∅ for `vicinae`, `HotkeyManager`, `hotkey` over `crates/` (only hit: an unrelated prose line, `global_shortcuts.rs:9`); grep ∅ for `bind_global_shortcuts`, `clear_global_shortcuts`, `bind_failed`, `bind-failed`; grep ∅ for `keysym_from_trigger`. Insertion points exist: `crates/rustty-apprt/src/gtk/winproto.rs:95` `pub enum App` + per-arm dispatch, `winproto/wayland/globals.rs:60` `enum Tag`, `class/application.rs:2454` `winproto()` / `:2462` `winproto_mut()`, `:2995` `desktop_notification` | Linux/Wayland | **none — needs one** (see §3 + §5) | **L** | M — new wire protocol, no crates.io binding, no mainstream compositor implements it yet |
| `0a117e079` | gtk: missing dbus connection caused a crash. `close()` restructured so subscription-unsubscribe + `Session.Close` sit inside `if (priv.dbus_connection) \|dbus\|`; `close()` now also calls `winproto().clearGlobalShortcuts()` unconditionally; `refresh()` gains `if (priv.dbus_connection == null) return` after the winproto attempt | **PARTIAL** — the crash-safety is already present and independently derived; the `clearGlobalShortcuts()` call and the post-winproto early-return are absent (they are part of the Hotkeys series) | `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs:254-300` `close()` — subscription drops hoisted *above* the connection check with a PORT NOTE stating exactly why; `:349-360` `request()` returns instead of `.?`-unwrapping a null connection. Divergence: rustty's `refresh` (`:227-249`) builds the map even with no bus ("headless refresh"), upstream now returns before building | Linux/GTK | `global_shortcuts_refresh_builds_map_without_dbus` (`:974`), `global_shortcuts_refresh_clears_prior_map` (`:990`) | S | Low |
| `2f3814ca5` | gtk: honor suspended window state. `class/surface.zig:3344-3358` `updateOcclusion(self)` — signature loses `visible`, computes `mapped and !windowSuspended()`, made `pub`; new `windowSuspended()` via `gtk.Window.isSuspended`; `class/window.zig:1370-1379,1400-1408` connect `notify::suspended` → `propSuspended` → every displayed surface's `updateOcclusion()` | **ABSENT** | `crates/rustty-apprt/src/gtk/class/surface.rs:2741` `fn update_occlusion(&self, visible: bool)` — still the old two-argument form, still private; callers `:3574` `update_occlusion(true)` / `:3581` `update_occlusion(false)`. grep ∅ for `is_suspended`, `window_suspended`, `"suspended"` in `crates/rustty-apprt/src/` | Linux/GTK (Wayland xdg_toplevel suspended) | **none — needs one** | S | Low — but the missing behaviour is "keep rendering an occluded window", a real power regression |
| `004c88e41` | fix: set max window clamp to current monitor size. `class/window.zig:1159-1180` new `toplevelComputeSize` (skip when maximized/fullscreen/quick-terminal; skip on 0-size or 0-bounds; re-assert size only when it *exceeds* compositor bounds) + `:1365-1374` connect `gdk.Toplevel::compute-size` with `.after = true` | **ABSENT** | grep ∅ for `compute_size`, `compute-size`, `toplevel_compute`, `get_bounds` in `crates/rustty-apprt/src/`. `class/window.rs` has the realize path where the connect belongs | Linux/GTK | **none — needs one**; see §5 for why a single-monitor VM cannot fully prove it | M | M — `gdk4-rs` `Toplevel::connect_compute_size` + `ToplevelSize` binding availability must be confirmed first |
| `5ea45919d` | config: `gtk-horizontal-tab-scroll`. `class/surface.zig:3034-3090` `ecMouseScrollHorizontal` rewritten: the tab-switch accumulator now runs only when the option is on **and** the unit is `.surface`; otherwise horizontal scroll is **forwarded to the terminal** via `scrollCallback(scaled.x * -1, 0, .{})` (neovim etc.) | **ABSENT** | `crates/rustty-apprt/src/gtk/class/surface.rs:3299-3355` `scroll_horizontal` — still `match ec.unit() { Surface => {}, _ => return false }` then unconditional accumulator; no config read, no terminal forwarding. grep ∅ for `gtk_horizontal_tab_scroll` in `crates/rustty-config/src/` | Linux/GTK | **none — needs one** | S (+S in config) | Low — **cross-area G**: the config option does not exist yet |
| `e8525c0fd` `b988efcfe` `1cc5e7bec` | Zig 0.16 + zig-gobject 0.3.2 sweep across `action.zig`, `embedded.zig`, `ipc.zig`, `class.zig`, `command_palette.zig`, `imgui_widget.zig`, `ext/actions.zig`, `media.zig`, `portal.zig`, `portal/OpenURI.zig`, `post_fork.zig`, `pre_exec.zig`, `ipc/DBus.zig`, `gtk_version.zig`, `adw_version.zig`, `Surface.zig`, plus `@cImport`→`adw_c`/`gtk_c`/`ghostty_gtk_resources` modules, `setenv`+`global.syncEnviron()`, `std.Random.IoSource` token RNG, `std.process.EnvMap`→`std.process.Environ.Map` | **N/A** | Language/std/bindings migration with no observable behaviour delta. `EnvMap`→`Environ.Map` is a std rename (`remove`→`orderedRemove`); the inlined flatpak check in `gtk/class/surface.zig:1590-1600` reproduces `src/os/env.zig:10-15` at the pin exactly | both | n/a | — | Low. **Cross-area E**: `src/os/env.zig` + `src/os/args.zig` are DELETED upstream; `internal_os.open()` loses its allocator arg (`application.zig:1953-1959`) |
| `e8525c0fd` | `build/blueprint.zig` +94/−? and `build/gresource.zig` +33 — `pub fn main(init: std.process.Init)`, `std.process.run`, `std.Io.Dir.access`, arena from `init.arena` | **N/A** | Zig build-tooling only; rustty generates gresource/blueprint from `build.rs` | Linux | n/a | — | Low. **Cross-area G** |

### 2b. `src/font`

| Upstream commit(s) | What changed | Status | Evidence | Platform | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|---|
| `dac341cad` + `e8f3f6c43` | `sprite/Face.zig:208-217` full-height cursor sprites (`cursor_rect`, `cursor_hollow_rect`, `cursor_bar`) use `metrics.cursor_height` instead of `cell_height`; `:231-246` `offset_y` re-centres the glyph in the cell by `(cell_height - height)/2`; + 89-line regression test | **PORTED** | `crates/rustty-font/src/sprite/face.rs:695-712` (the switch, with the reference text quoted at `:698-701`) | both | `crates/rustty-font/src/sprite/face.rs:1161` `full_height_cursor_sprites_respect_cursor_height_metric` — asserts height+offset_y for all 3 sprites at cursor_height 12/16/20, i.e. all three of upstream's `<`, `==`, `>` cases | — | — |
| `d0e72a3ab` | **font/sprite: z2d 0.12.1 + native path insetting.** `sprite/canvas.zig:375-390` `innerStrokePath` collapses from the 60-line two-surface fill×stroke mask workaround to `path.offset(alloc, -line_width/2)` + `strokePath`; error set becomes `z2d.Path.OffsetError \|\| StrokeError`; `sprite/draw/powerline.zig:292-320` `drawE0B5` grows `staticPath(4)`→`(6)` with a leading `lineTo(1,0)` and trailing `lineTo(0,float_height)` to force butt caps perpendicular; 6 goldens regolded | **ABSENT** | `crates/rustty-font/src/sprite/canvas.rs:718-...` `innerStrokePath` is still the two-surface mask (fill_sfc + stroke_sfc + manual alpha multiply + composite), with a long PORT NOTE about the growable-clone workaround; `crates/rustty-font/src/sprite/draw/powerline.rs:513` `canvas.staticPath::<4>()`, no `line_to(1.0, 0.0)`. **Blocker:** `crates/rustty-z2d/src/path.rs` has no `offset` (`grep -n 'pub fn ' path.rs` lists `empty…is_closed`, no `offset`), and `crates/rustty-z2d/Cargo.toml:1` states the port targets **z2d 0.10.0** | both | Existing sprite golden tests are the gate once regolded (`crates/rustty-font/res/*.png`, 37 files) — 6 of them change | **L** | M — needs a z2d 0.10→0.12.x feature port (`Path.offset`); pixel delta is small and localized (76 px max-delta 48 in U+E0xx; 3 px in U+25xx), so a regression here is easy to miss without the goldens |
| `b988efcfe` | `opentype/svg.zig:39-40` bounds-check the document-list offset (`if (offset > data.len) return error.EndOfStream`) before seeking; + test `"SVG document list offset past end"` | **PARTIAL** — behaviour present and *stricter* than upstream; the guarding test is missing | `crates/rustty-font/src/opentype/svg.rs:62-64` has the exact check; `:75-79` adds a further `len > (data.len - pos)/12` rejection that upstream still lacks (documented as an R6 UB-avoidance). Tests: `svg.rs:169-178` has only `fn svg()` — no past-end case | both | **none — needs one** (port upstream's test verbatim; add a second for the record-count guard) | S | Low |
| `e8525c0fd` (+`421fe8dab` test scaffolding) | 0.16 sweep over `Atlas.zig`, `Collection.zig` (incl. `std.SegmentedList`→local `datastruct/segmented_list.zig`), `Metrics.zig` (`@Type`→`@Enum`), `SharedGrid.zig` (`std.Thread.RwLock`→`std.Io.RwLock`, +120-line manual `RenderGlyphError` set), `SharedGridSet.zig`, `discovery.zig`, `face/coretext.zig` (+`error.ReadFailed` arm), `face/freetype.zig`, `library.zig`, `glyf_rasterize_png_test.zig`, `opentype/{glyf,head,hhea,os2,post,sfnt,svg}.zig` (`fixedBufferStream`→`std.Io.Reader` + `lib/compat/reader.zig`), `shaper/{coretext,harfbuzz,feature}.zig`, and ~200 test-only `Terminal.init(io, …)` / `node.page()` / `.codepoint = .{ .data = … }` edits | **N/A** | Language/std migration; rustty uses Rust containers, `std::sync` locks, and `Result` error enums throughout. `std.SegmentedList`'s relocation is invisible to rustty (`crates/rustty-datastruct/src/segmented_pool.rs:11-42` documents the Rust equivalent) | both | n/a | — | Low. **Cross-area (datastruct / A–D)**: `node.page()` and `codepoint.data` track terminal-side API changes |
| `e8525c0fd` `d0e72a3ab` | 24 `sprite/testdata/*.png` + `testdata/glyf_rasterize.png` regolded | **N/A for 19, tied to `d0e72a3ab` for 6** | Pixel-decoded: `U+2800…-11x21+2` and `glyf_rasterize.png` are **pixel-identical** across the whole range → PNG-encoder churn from `writeToPNGFile(io, …)`. The 6 pixel-affected files are exactly `d0e72a3ab`'s stat: `U+2500…{11x21+2,9x17+1}`, `U+E000…{11x21+2,12x24+3,18x36+4,9x17+1}` | both | see the `d0e72a3ab` row | — | Low |

### 2c. `src/renderer`

| Upstream commit(s) | What changed | Status | Evidence | Platform | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|---|
| `d34b54e9b` (+`4f53b846b` cosmetic) | `State.zig:33-127` mutex hand-off to avoid starving frames: `demand` + `handoff_gen` atomics, `lockDemand`/`unlockDemand`/`yieldToDemand` with a 1 ms futex timeout; `generic.zig:1170` uses `state.lockDemand`/`unlockDemand` for the frame snapshot | **PORTED** | `crates/rustty-renderer/src/state.rs:153` `handoff_gen`, `:231` `lock_demand`, `:250` `unlock_demand`, `:283` `yield_to_demand`; call site `crates/rustty-renderer/src/generic.rs:4502,4521` | both | 4 tests: `state.rs:503` `renderer_state_yield_to_demand_hands_off_to_a_waiter`, `:552` `…_is_free_without_demand`, `:585` `…_parks_while_demand_exists`, `:617` `renderer_state_unlock_demand_wakes_a_parked_yield` | — | — |
| `461562ca4` `25e624569` `0fb89f4ff` | `Thread.zig:74-77,169-195,204-205,509-520,564-568,754-866` idle scrollback-compression scheduling: `Compression` struct (250 ms idle / 1 ms step), `DerivedConfig.scrollback_compression`, `changeConfig` resets `activity` on disabled→enabled, `wakeupCallback` calls `compression.wake`, and the `completion.state() == .active` contention arm that prevents inspector frames starving the timer | **PORTED** | `crates/rustty-core/src/renderer_seam.rs:577` field, `:1781` `pub(crate) struct Compression`, `:1796/1798` `IDLE_INTERVAL`/`STEP_INTERVAL`, `:1820-1877` `wake` (incl. the `Completion::state_of(...) == Active` arm at `:1863-1871`), `:1881` `schedule`, `:1949-1996` `step`; `crates/rustty-renderer/src/thread.rs:84,104` DerivedConfig | both | 3 tests: `renderer_seam.rs:2254` `compression_idle_pass_compresses_scrollback`, `:2302` `compression_is_inert_when_the_option_is_disabled`, `:2335` `change_config_restarts_the_pass_only_on_disabled_to_enabled` | — | — |
| `bdc0b6c19` | `image.zig` kitty/gfx generation stamps replace `transmit_time: std.time.Instant` — `prepImage(… generation: u64 …)`, `ImageMap` value carries `generation`, overlay takes a fresh `nextGeneration()` | **PORTED** | `crates/rustty-renderer/src/image.rs:288` `next_generation()`, `:926` `generation: u64` param, `:938` staleness compare, `:980` `generation: 0` sentinel, `:1170` map field | both | Covered indirectly by the kitty-image renderer tests; **no dedicated staleness test** — a same-id/different-generation replace assertion would be cheap and is worth adding | — | Low |
| `9c9cf3e82` | `link.zig:24-36` new `Link.active(mouse_viewport, mouse_mods)` helper; `:83-90` `renderCellMap` early-returns **before** allocating the render-state string + byte→cell map when no link can highlight; `:278-338` new test using a `FailingAllocator{fail_index=0}` | **ABSENT** | `crates/rustty-renderer/src/link.rs:162-215` — still allocates `builder`/`map` and calls `render_state.string(...)` first, then does the per-link `match link.highlight` inline; grep ∅ for `fn active` in `link.rs` | both | **none — needs one** (a failing/counting allocator test mirroring upstream's) | S | Low — pure per-frame allocation win |
| `4a22eed6d` | `metal/buffer.zig:81-93,138-150` **fix 2× `sizeof` over-allocation in `Buffer.sync`/`syncFromArrayLists`**: the old code computed `size = req_bytes * 2` then passed `size * @sizeOf(T)`, i.e. `data.len * sizeof(T)² * 2` (64× for the 32-byte `CellText` buffers, 8× for `CellBg`, per swap-chain frame — ~9.4 MB instead of ~300 KB for a 120×40 screen). Fix tracks the new length in units of `T` (`self.len = data.len * 2`) — which also repairs a second latent bug: `self.len` was never updated on realloc | **ABSENT** | `crates/rustty-renderer/src/metal/buffer.rs:150-161` reproduces the bug **on purpose**: `let size = req_bytes * 2;` then `size * std::mem::size_of::<T>()` under a comment reading *"FAITHFULNESS QUIRK (R1): … over-allocating by a `size_of::<T>()` factor (a latent reference quirk; harmless…). Reproduced byte-for-byte … NOT corrected."* Same at `:219-226` in `sync_from_array_lists`. `len` field exists (`:79`) and is likewise left stale on realloc | **macOS only** | **none — needs one, and one is genuinely possible** (see §5: headless `MTLCreateSystemDefaultDevice` unit test) | S | Low to fix, but the PORT NOTE must be **deleted**, not edited — leaving "quirk preserved deliberately" text next to the fix would mislead the next reader |
| `446f80f4e` | `generic.zig:1157-1290` render-state update split: `terminal_state.beginUpdate(alloc, terminal)` inside the state mutex, `terminal_state.endUpdate()` outside it (style denormalization moves out of the critical section — upstream measures 2.7×–11× less lock hold) | **ABSENT**, blocked | `crates/rustty-renderer/src/generic.rs:4564` still `self.terminal_state.update(self.alloc, …)?`; grep ∅ for `begin_update`/`end_update` in `crates/rustty-renderer/src/` **and** in `crates/rustty-vt/src/` | both | **none — needs one** (a lock-hold-duration assertion is flaky; prefer an ordering test: `endUpdate` must run before any `rebuildCells` read) | M | M — **cross-area A–D**: requires `terminal.RenderState.beginUpdate/endUpdate` in `crates/rustty-vt/src/render.rs` first (`src/terminal/render.zig` is +913/−168 in this range and belongs to areas A–D) |
| `e8525c0fd` | 0.16 sweep: `Overlay.zig` (`z2d.Context.init(io, …)`), `generic.zig` locks/`std.Io.Timestamp`/`std.Io.Semaphore`/`compat_file`, `cell.zig`+`cursor.zig` test-only `Terminal.init(io,…)`, `metal/api.zig` (`@Type`→`@Int`), `metal/shaders.zig`+`opengl/shaders.zig` (`@Type`→`@Struct`), `shadertoy.zig` (`std.Io.Dir`), plus deletion of ~40 lines of commented-out timing scaffolding | **N/A** | Language/std migration | both | n/a | — | Low |

### 2d. `src/inspector`

| Upstream commit(s) | What changed | Status | Evidence | Platform | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|---|
| `6d5dda40d` + `421fe8dab` (compression-aware widgets) | `widgets/page.zig:27-45,66-84` `treeNode` takes `cols`/`rows`/`compressed`/`dirty: ?bool` instead of `page: *const Page`, renders a "compressed" badge; `widgets/pagelist.zig:36-42,76-104` per-node traversal is metadata-only (`node.rows()`, `pageIfResident()`, `storage() == .compressed`) and opens contents through `pagePreservingState(page_allocator)`; `:265-402` new "Page Compression" table (platform support / page states / uncompressed / compressed storage / estimated resident / estimated savings) fed by `pages.memoryStats()`; `:141-172` "Page Bytes"/"Max Size"/"Tracked Pins" rows replaced by "Scrollback Limit"; `:471-487` tracked-pins dirty via `pageIfResident` with a "compressed" fallback | **PORTED** | `crates/rustty-inspector/src/widgets/pagelist.rs:21-26` invariant header, `:166` `compressionTable(pages)`, `:248` `Node::page_if_resident`, `:319` `Node::page_preserving_state`, `:478-480` `compressionTable` + `pages.memory_stats()`, `:413-435` the "Page Bytes"→"Scrollback Limit" swap with a PORT NOTE citing oracle `6d5dda40d`; `widgets/page.rs:156-168` `cols`/`rows`/`compressed`/`dirty: Option<bool>` | both | `crates/rustty-apprt/tests/gtk_inspector_dump.rs` (live GTK dump harness) + the vt-side compression tests. **No headless assertion that the traversal is metadata-only** — worth a `#[test]` that walks a PageList with a compressed node and asserts `compressed_pages()` is unchanged after a draw | — | Low |
| `6d5dda40d` + `421fe8dab` (CellChooser) | `widgets/pagelist.zig:697-735,754,808,855-862,923,1021` `CellChooser.draw` stops using `pages.getCell(pt)` (which resolves through the PageList node and would restore a compressed page) and instead pins, copies via `pin.node.pagePreservingState(...)`, and passes a new `InspectedCell { row, cell, row_idx, col_idx }` + an explicit `page: *const Page` down to `CellInfo.draw`, `hyperlinkTable`, `graphemeTable` — deliberately making it *impossible* for the widget to touch the original node | **PARTIAL** | `crates/rustty-inspector/src/widgets/pagelist.rs:1678-1691` still `pages.get_cell(pt)` then `self.cell_info.draw(&cell, pt)`; `:1697+` reads `cell.node` / page memory through the PageList node. grep ∅ for `InspectedCell` in `crates/rustty-inspector/`. The invariant is *documented* at `:21-26` but only the tree-node and tracked-pins paths honour it | both | **none — needs one**: a test that opens the cell chooser on a compressed node and asserts the node is still `.compressed` afterwards | M | **M — this is a live correctness gap, not cosmetics**: inspecting a cell in a compressed region silently un-compresses it, defeating the compression the renderer just did |
| `f4c68d65e` (via `6d5dda40d`) | `widgets/screen.zig:353-362` and `widgets/pagelist.zig:154-168` read the limit as `pages.limits.max(.bytes)` instead of `pages.maxSize()` | **PARTIAL**, blocked | `crates/rustty-inspector/src/widgets/screen.rs:838-843` still `pages.max_size()` (twice); `widgets/pagelist.rs:441` `formatBytes(&mut limit_buf, pages.max_size())`. `crates/rustty-vt/src/pagelist/mod.rs:2175` has `max_size()`; grep ∅ for a `limits` field/`Limits` type | both | n/a until the vt side lands | S | Low. **Cross-area A–D**: needs `f4c68d65e` "terminal: support runtime scrollback limits" (`src/config/limit.zig` is NEW upstream — also **cross-area G**) |
| `e8525c0fd` | `Inspector.zig:92-95` and `widgets/termio.zig:793-796` drop the now-unreachable `else => return err` arm; `widgets/key.zig` `fixedBufferStream`→`std.Io.Writer.fixed`; `widgets/surface.zig` mutex + `@ptrCast` on the switch result | **PORTED-EQUIVALENT / N/A** | `crates/rustty-inspector/src/Inspector.rs:260-265` already matches only `Err(CircBuf::Error::OutOfMemory)` with a PORT NOTE stating the `else` arm is unreachable | both | existing inspector tests | — | — |

### 2e. `macos/` — **all Swift-side; none requires Rust work**

| Upstream commit | What changed | Status | Evidence (rustty) | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|
| `0274e7ad8` | `QuickTerminalScreenStateCache.swift:69-80,106-114` a cached frame is valid only on **exact** geometry (`scale == backingScaleFactor && screenSize == frame.size`); stop growing `entry.screenSize` on a larger screen. Fixes a stale partial-size restore after display reconnect | **ABSENT** | `macos/Sources/Features/QuickTerminal/QuickTerminalScreenStateCache.swift:112-113` still `screenSize.width <= … && screenSize.height <= …`; `:77` still `entry.screenSize = screen.frame.size` | `macos/Tests/QuickTerminalScreenStateCacheTests.swift` (+61, NEW) — **directly portable**; `macos/Tests/NSScreenTests.swift:84` already has a `MockNSScreen` precedent | S | Low |
| `49806fc4c` | `NSPasteboard+Extension.swift:38-52` `getOpinionatedStringContents` rewritten: per-`pasteboardItem`, in order — file URL → escaped path, else the item's plain string; join with a space; nil when nothing usable. Fixes remote file promises (Transmit/Nova) and ordering | **ABSENT** | `macos/Sources/Helpers/Extensions/NSPasteboard+Extension.swift:40-49` still `readObjects(forClasses:[NSURL.self])`-first with an `absoluteString` fallback | `macos/Tests/NSPasteboardTests.swift` +224 (17 `@Test` cases) — **directly portable**; rustty's file is 33 lines | M | Low |
| `acd09c0a6` | the 224 lines of tests above | **ABSENT** | `wc -l macos/Tests/NSPasteboardTests.swift` = 33 | — | (same row) | — |
| `6e267d336` | `SurfaceView_AppKit.swift:2214-2217,2248-2255` drop `.URL` from `dropTypes` and replace 18 lines of bespoke drop handling with `pb.getOpinionatedStringContents()` | **ABSENT** | `macos/Sources/RusttyKit/Surface View/SurfaceView_AppKit.swift:2214-2217` still lists `.URL`; `:2237-2254` still the bespoke ladder | none upstream; the `NSPasteboardTests` suite covers the shared helper | S | Low — depends on `49806fc4c` |
| `10565995b` | `ServiceProvider.swift:36` `readObjects(forClasses:[NSURL.self], options:[.urlReadingFileURLsOnly: true])` — new-terminal services read file URLs only | **ABSENT** | `macos/Sources/Features/Services/ServiceProvider.swift:36` has no `options:` | **none — needs one** | S | Low |
| `f52f8aab9` | `SurfaceView_AppKit.swift:292-300` avoid a notification-publisher retain cycle: `publisher(for:)` without `object:` + a `.filter { [weak self] … }` identity check | **ABSENT** | `macos/Sources/RusttyKit/Surface View/SurfaceView_AppKit.swift:295` still `.publisher(for: .rusttySelectionDidChange, object: self)` | **none — needs one** (leak test / Instruments) | S | Low |
| `751a60df6` | `SurfaceView_AppKit.swift:2042,2055-2062` route preedit commits through key events: capture `hasMarkedText()` before `unmarkText()`, and when a marked-text commit produces chars, send `committedPreeditTextAction(GHOSTTY_ACTION_PRESS, text:)` instead of `sendText` | **ABSENT** | `macos/Sources/RusttyKit/Surface View/SurfaceView_AppKit.swift:2039-2050` `insertText` has no `hadMarkedText`; it ends in `surfaceModel.sendText(chars)`. The helper already exists at `:1486` `committedPreeditTextAction` (used from `:1186`) — so this is a 6-line change | **none — needs one** (IME keybind-interpretation test) | S | Low |
| `a6edca2d7` | `Ghostty.Surface.swift:26-33` free the surface **synchronously** when `deinit` already runs on the main thread (the detached-Task path leaves unretained userdata registered) | **ABSENT** | `macos/Sources/RusttyKit/Rustty.Surface.swift:26-36` unconditional `Task.detached { @MainActor in rustty_surface_free(surface) }` | **none — needs one** | S | M — teardown-ordering / use-after-free class of bug |
| `35790a7e5` | Revert of "macOS: fix undo new tab will cause a crash": `TerminalController.swift:503-512` drop the `DispatchQueue.main.async` wrapper around the undo close | **ABSENT** | `macos/Sources/Features/Terminal/TerminalController.swift:507-513` still has the wrapper **and the now-obsolete comment citing ghostty PR #9512** | none upstream | S | Low — a revert; do not port it as "new work" without checking the follow-up fix landed (`a6edca2d7` is that fix) |
| `2104e0749` | `HiddenTitlebarTerminalWindow.swift:73-84` hide `NSScrollPocket` on macOS 27 (AppKit moves it into the title bar, covering terminal content) | **ABSENT** | grep for `NSScrollPocket` hits only `macos/Sources/RusttyKit/Surface View/SurfaceScrollView.swift:124-340` (a different, frame-change workaround); nothing in `HiddenTitlebarTerminalWindow.swift` | **none — needs one**; only observable on macOS 27 | S | Low |
| `632aa6756` | `TitlebarTabsTahoeTerminalWindow.swift:166-170` accept `NSTitlebarAccessoryContainerView` as well as `…ClipView` (macOS 27 beta 2 renamed it) | **ABSENT** | `macos/Sources/Features/Terminal/Window Styles/TitlebarTabsTahoeTerminalWindow.swift:167` only `NSTitlebarAccessoryClipView` | **none — needs one**; only observable on macOS 27 | S | Low |
| `fa3802a70` | `Backport.swift:110-115` split-drag pointer style → HIG: `frameResize(...)` becomes `columnResize(directions:)` / `rowResize(directions:)` | **ABSENT** | `macos/Sources/Helpers/Backport.swift:110-115` still `frameResize(position:directions:)` | **none — needs one** (visual) | S | Low |

---

## 3. Verdict: the Wayland `Hotkeys` question

**It is ADDITIVE with PREFERENCE, not a replacement.** Both mechanisms ship
upstream simultaneously and the portal path is untouched and still required.

The evidence, from `global_shortcuts.zig:187-224` (post-change):

```zig
// Prefer a windowing protocol native mechanism over the
// XDG desktop portal when available.
if (Application.default().winproto().bindGlobalShortcuts(self, config)) {
    log.debug("global shortcuts bound via winproto", .{});
    return;
}
// The portal fallback requires a dbus connection.
if (priv.dbus_connection == null) return;
// … unchanged CreateSession/BindShortcuts portal flow …
```

and from `wayland.zig:68-74`:

```zig
const manager = self.globals.get(.vicinae_hotkey_manager) orelse return false;
self.hotkeys.bind(manager, shortcuts, config);
return true;
```

So the decision is made **per-session at runtime**, keyed on whether the
compositor advertises `vicinae_hotkey_manager_v1`. `noop` and `x11` both return
`false` unconditionally, so X11 and non-GTK builds keep the portal verbatim.
Nothing in rustty's shipped WS15 portal implementation becomes dead code.

Three further facts that shape the recommendation:

1. **The two mechanisms are not behaviourally equivalent, and upstream now
   depends on the difference.** The portal is a session-scoped, user-mediated,
   persisted grant; vicinae-hotkey is ephemeral, per-object, client-chosen, and
   the compositor answers each request individually with `bound` / `denied(reason,
   message)` / `revoked(reason, message)`. That per-binding failure channel is
   what motivated the *other half* of the series: the new `BindFailed` boxed
   type, the `bind-failed` GObject signal, and
   `application.zig:1691-1725 globalShortcutBindFailed`, which raises a **desktop
   notification** (not a toast — it can fire before any window exists) naming the
   failed trigger. The portal path never emits `bind-failed`. Porting the winproto
   arm without the failure channel would produce silent no-op keybinds.
2. **`vicinae_hotkey_v1` is an unratified, testing-phase protocol** — the XML's own
   description says *"currently in the testing phase. Backward incompatible
   changes may be added."* It is authored by the vicinae launcher project, is not
   in `wayland-protocols`, and **no mainstream compositor (mutter, KWin, wlroots)
   implements it today**. On the project's own GNOME VM, `globals.get(...)`
   returns null and every run takes the portal path.
3. **rustty has no binding for it and cannot get one from crates.io.** rustty's
   Wayland arm consumes typed bindings from `wayland-protocols` /
   `wayland-protocols-plasma` (`crates/rustty-apprt/Cargo.toml:283-285`); there
   is no crate for this protocol. The mechanism is `wayland-scanner`
   (`generate_client_code!`) over a vendored copy of the XML in
   `crates/rustty-apprt/build.rs` — the same thing `wayland-protocols` does
   internally, so it is well-trodden, but it is new machinery for this crate.

### Recommendation

**Port it, in full, but sequence it so the untestable part is last and is not on
the critical path.** Four steps:

- **F-W1 (S, testable today).** The parts with no protocol dependency:
  `key::keysym_from_trigger` + its upstream test verbatim; `winproto::App::{bind_global_shortcuts,
  clear_global_shortcuts}` with `noop`/`x11` returning `false`; `GlobalShortcuts::BindFailed`
  + the `bind-failed` signal + public `emit_trigger`/`emit_bind_failed`;
  `Application::global_shortcut_bind_failed` → `desktop_notification`
  (`class/application.rs:2995`). Gate: headless unit tests for the keysym mapping
  and the notification body formatting; a signal-emission KAT in the style of
  `global_shortcuts_activation_emits_trigger` (`global_shortcuts.rs:1009`).
  **Implementation hazard:** `Application::winproto()` returns `Ref<'_, winproto::App>`
  (`class/application.rs:2454`) while `clear_global_shortcuts` needs `&mut` —
  route through `winproto_mut()` (`:2462`) and make sure `close()` does not hold a
  `Ref` across the call, or you convert an upstream crash-fix into a `RefCell`
  double-borrow panic.
- **F-W2 (S).** Wire `refresh()` to try winproto first and then
  `if dbus_connection.is_none() { return; }` — and decide explicitly whether to
  keep rustty's "headless refresh still builds the map" divergence
  (`global_shortcuts.rs:227-249`), which two existing tests assert. My
  recommendation: keep rustty's behaviour, and record the divergence in the port
  ledger, because two KATs depend on it and upstream's early return is only an
  optimisation.
- **F-W3 (M).** Vendor `vicinae-hotkey-v1.xml` verbatim (keep the copyright
  header — MIT, Aurelien Brabant) under
  `crates/rustty-apprt/protocols/`, generate client bindings with
  `wayland-scanner` in `build.rs`, add the `Tag::VicinaeHotkeyManager` arm to
  `winproto/wayland/globals.rs:60-70` + the registry bind at `:246-266`.
  Gate: a headless test asserting the generated interface name/version, plus
  `globals` graceful-absence (already the established pattern — the KDE globals
  are absent on GNOME and the code copes).
- **F-W4 (M).** `Hotkeys` itself. The entry-selection logic (`leaf.flags.global`,
  skip `catch_all`, skip multi-action) is pure and unit-testable against a
  `Config` **without any compositor** — that is where most of the porting risk
  lives, so test it there. The wire half (`manager.bind`, the listener
  state machine, `Entry::fail` → `emit_bind_failed`) is honest verification debt;
  see §5 for the recipe.

**Do not** treat the winproto arm as a replacement and do not weaken the portal
path. And do not gate F-W1/F-W2 on F-W3/F-W4: the failure-notification channel is
useful on its own and F-W2 is the tail of an upstream crash fix.

---

## 4. Partials and divergences — exact missing hunks

1. **`opentype/svg.zig` (`b988efcfe`)** — behaviour ported, **test missing**.
   Missing hunk: the whole `test "SVG document list offset past end"` block
   (`src/font/opentype/svg.zig:112-121` post-change). Note rustty's check is
   strictly stronger (`svg.rs:75-79` also rejects a record count that overruns the
   table), so a faithful port of the upstream test passes as-is; add a second case
   for the record-count guard so the stronger behaviour is also pinned.
2. **`global_shortcuts.zig` (`0a117e079`)** — crash-safety ported independently;
   missing hunks are (a) `close()`'s `Application.default().winproto().clearGlobalShortcuts();`
   (`:143-146` post-change) and (b) `refresh()`'s `if (priv.dbus_connection == null) return;`
   (`:198-199`). Both are entangled with the Hotkeys series; land them in F-W1/F-W2.
   Divergence to record either way: upstream's `refresh` returns before building
   the map when there is no bus; rustty builds it (two KATs depend on this).
3. **`widgets/pagelist.zig` `CellChooser` (`421fe8dab`/`6d5dda40d`)** — missing hunks:
   the `InspectedCell` struct (`:8-19`), `CellChooser.draw`'s
   `pages.pin(pt)` + `pin.node.pagePreservingState(page_allocator)` + `page.getRowAndCell(pin.x, pin.y)`
   replacement of `pages.getCell(pt)` (`:697-735`), and the added
   `page: *const terminal.Page` parameter threaded through `CellInfo.draw`
   (`:855-862`), `hyperlinkTable` (`:754`), `graphemeTable` (`:808`) with
   `cell.node.data` → `page` at `:923` and `:1021`.
4. **Inspector limit rows (`f4c68d65e`)** — missing hunks: `widgets/screen.zig:353-362`
   and `widgets/pagelist.zig:154-168` reading `pages.limits.max(.bytes)`. Blocked
   on the vt side; leave `max_size()` until then rather than inventing a shim.
5. **`metal/buffer.rs`** — not a partial but a **deliberate divergence that upstream
   has now retired**. The PORT NOTE at `crates/rustty-renderer/src/metal/buffer.rs:155-160`
   asserts the over-allocation is a harmless reference quirk preserved on purpose.
   That text must be **removed**, not amended, when the fix lands; and the fix has
   two halves (`self.len = data.len * 2` / `= total_len * 2`, plus passing
   `self.len * size_of::<T>()` exactly once).

---

## 5. Verification plan per sub-tier — and the honest debt

### Pre-work: fix the confirmed test-binary tripwire (blocks any new Wayland KAT)

`crates/rustty-apprt/tests/gtk_wayland_winproto.rs` has **two** `#[test]`s that
each call `gtk4::init()` — `:25 wayland_app_init_selects_wayland_arm` (`:26`) and
`:80 wayland_set_urgent_dispatches_activation_token` (`:81`) — in one binary.
`gtk4::init()` pins GTK to the calling thread; the second call from a different
thread panics *"Attempted to initialize GTK from two different threads."*

**Confirmed empirically**, not from memory. A two-test throwaway binary compiled
with `rustc --test` and run with `--test-threads=1 --nocapture` printed:

```
test a ... A tid=ThreadId(2)
test b ... B tid=ThreadId(3)
test main_tid ... name=Some("main_tid")
```

libtest spawns a fresh named thread per `#[test]` **even at `--test-threads=1`**,
so serialising the run does not save it. Today the trap is masked because both
tests are `#[ignore]`d and the doc-comment recipe runs them one at a time with
`--exact`; it fires the moment anyone runs `-- --ignored` for the file, or adds a
third test. **Fix: one `gtk4::init()` test per binary** — split into
`gtk_wayland_winproto.rs` and `gtk_wayland_urgent.rs`. Do this before adding any
Wayland hotkey KAT, or the new test inherits the trap. The same audit should be run
across all 21 `crates/rustty-apprt/tests/gtk_*.rs` files.

### `src/apprt` (GTK — no unit-gate by program decision, but plenty is provable)

Provable headlessly, today:
- `key::keysym_from_trigger` — pure function, upstream's test ports verbatim.
- `Hotkeys` entry selection — pure over a `Config`; assert global-only,
  `catch_all`-skipped, multi-action-skipped, order preserved.
- `BindFailed` → notification body formatting — pure string assembly.
- `gtk-horizontal-tab-scroll` decision logic — extract the predicate
  (`tab_scroll_enabled && unit == Surface`) so it is testable without a widget.

Provable live on the VM (`ssh gael@192.168.64.4`, repo `~/projects/rustty`):
- **Hotkeys fallback** — extend `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh`
  with two new assertions: the log must **not** contain `global shortcuts bound via
  winproto`, and the existing 6-step portal chain must still go green. On mutter
  this is the *real-world* path, so it is a genuine regression gate for F-W1..F-W4.
- **suspended/occlusion** — run headed, `gnome-shell`-minimise the window (or move
  another window fully over it on a compositor that reports `suspended`), and
  assert the occlusion callback fires and frame production stops; the observable
  is the existing occlusion log line plus a frame counter.
- **NEVER** run a heavy `cargo build` concurrently with headed compositor work on
  that VM — that combination preceded three crashes. Build first, to completion,
  then run the headed step.

Honest verification debt, with recipes rather than a shrug:
- **vicinae-hotkey positive path (bound/pressed/denied/revoked).** No mainstream
  compositor implements the protocol, so it cannot be exercised on the VM.
  Recipe: a minimal fake compositor in a `#[test]`-local `wayland-server`
  (`wayland-server` 0.31, sibling of the `wayland-client` already vendored) that
  advertises `vicinae_hotkey_manager_v1` on its own socket, then point the client
  at it with `WAYLAND_DISPLAY=<fake>`. This is a genuinely runnable test, just a
  day of scaffolding — schedule it as its own task rather than pretending F-W4 is
  verified without it. Interim marker: `#[test] #[ignore = "needs fake
  vicinae compositor"]` so the debt is visible in the test list, not only in a doc.
- **`compute-size` monitor clamp.** Reproducing it needs the window to move to a
  *larger* monitor, i.e. two heads. The VM has one. Recipe: GNOME's
  `--virtual-monitor` / a second `wayland-headless` output if the session supports
  it; otherwise the honest position is "logic reviewed + guard conditions unit
  tested (maximized/fullscreen/quick-terminal/zero-size/zero-bounds early
  returns), geometry effect unverified" — and say exactly that, do not mark it green.

### `src/renderer` (GPU — no unit-gate, but two of the three gaps *are* gateable)

- **`link.rs` early-out** — fully headless. Rust equivalent of upstream's
  `FailingAllocator{fail_index=0}`: a counting allocator wrapper (or, since
  `render_cell_map` takes an explicit `alloc`, an allocator that panics on first
  use). Assert `result.count() == 0` and zero allocations for the three
  non-matching highlight modes.
- **`metal/buffer.rs`** — this one is genuinely unit-testable on the macOS dev
  machine and should not be written off as "GPU, unverifiable":
  `MTLCreateSystemDefaultDevice()` works headless in a test process. Test: build a
  `Buffer<CellText>` at len 1, `sync` a 100-element slice, assert
  `buffer.length() == 200 * size_of::<CellText>()` (not `200 * size_of²`) and
  `self.len == 200`; then `sync` a 150-element slice and assert **no** realloc
  happened (`length()` unchanged) — which is precisely what the stale-`len` half
  of the bug broke. `#[cfg(target_os = "macos")]`.
- **`begin_update`/`end_update`** — blocked on area A–D. When it lands, prefer an
  ordering/observability test (a flag set by `end_update` that `rebuild_cells`
  asserts) over a lock-hold-duration measurement, which is flaky. The renderer is
  render-complete offscreen, so an offscreen `update_frame` → `draw_frame` KAT can
  cover the sequencing without a window.

### `src/font`

- The 37 golden PNGs in `crates/rustty-font/res/` are a real pixel gate. Sequence
  for `d0e72a3ab`: (1) port `Path::offset` into `crates/rustty-z2d/src/path.rs`
  with its **own** unit tests (offset of a closed rect, of an open path, negative
  inset, degenerate/zero-length segments) — do not let the sprite goldens be the
  only witness for a new geometry algorithm; (2) rewrite `innerStrokePath`;
  (3) apply the `drawE0B5` 6-node path; (4) regold **only** the 6 affected files.
  Per the project's own hard-won note: `cargo clean` before regenerating goldens.
  Expected magnitude, measured from the oracle: ~76 differing pixels (max delta 48)
  in U+E0xx, ~3 (max delta 16) in U+25xx — if a regold moves far more than that,
  the `offset` port is wrong, not the goldens.
- `svg.rs` test: plain `cargo test -p rustty-font`.
- Cursor-height row needs no new work; `sprite/face.rs:1161` is the guard.

### `src/inspector`

- `gtk_inspector_dump.rs` is the live harness. Add a headless test for the
  CellChooser fix: build a `PageList`, compress a node, run the chooser's draw
  path against a coordinate inside it, assert `memory_stats().compressed_pages`
  is unchanged. That assertion is exactly the invariant already written down at
  `widgets/pagelist.rs:21-26` but currently unenforced for this path.

### `macos/` (Swift)

Real, cheap gate — and the best value in this whole area:
`xcodebuild test -project macos/Rustty.xcodeproj -scheme Rustty`. Two upstream
test files port essentially verbatim (`QuickTerminalScreenStateCacheTests.swift`
+61 with a `MockSizedScreen` that mirrors the existing
`macos/Tests/NSScreenTests.swift:84 MockNSScreen`; `NSPasteboardTests.swift` +224,
17 cases, all using private named pasteboards so they neither touch the user's
clipboard nor serialise). Those two cover 3 of the 11 macOS rows outright.
Honest debt: the macOS-27-only AppKit view-hierarchy workarounds (`2104e0749`,
`632aa6756`) and the HIG pointer style (`fa3802a70`) are visual/OS-version-bound
— recipe is a screenshot check on a macOS 27 beta VM/host; until one exists, mark
them "ported, unverified on 27" rather than green. `a6edca2d7` (synchronous free
on main thread) should get an assertion that the surface pointer is freed before
`deinit` returns, e.g. a test double counting `rustty_surface_free` calls.

### R5 (platform-gating) discipline

Every item here is platform-gated: GTK/Wayland/X11 under
`#[cfg(target_os = "linux")]` + `feature = "gtk"/"wayland"`, `metal/buffer.rs`
under macOS, `macos/**` not compiled on Linux at all. **Do not attempt to prove
"the other platform's artifact is unchanged" by hashing binaries** — a prior lane
got three different hashes from rebuilds of identical source and correctly moved
to an A/B behaviour comparison. Use that: (a) `cargo tree -e features` before/after
must be byte-identical for the untouched platform's feature set (this is
reproducible, unlike a binary); (b) the untouched platform's test suite output
(names + pass/fail) must be identical; (c) for `metal/buffer.rs`, confirm the file
is inside a `#[cfg(target_os = "macos")]` module tree so the Linux build never
parses it.

---

## 6. Cross-area dependencies, ordering, open questions

### Cross-area dependencies (blocking)

| This area's change | Needs | Owner area |
|---|---|---|
| `renderer/generic.rs` `begin_update`/`end_update` | `terminal.RenderState.beginUpdate/endUpdate` (`src/terminal/render.zig`, +913/−168 in range, `446f80f4e`) | **A–D** |
| Inspector "Scrollback Limit" / "Memory Limit" value source | `PageList.limits` + `Limits.max(.bytes)` (`f4c68d65e`), and NEW `src/config/limit.zig` (115) | **A–D** + **G** |
| Inspector CellChooser fix | `Node::page_preserving_state` / `pin()` — **already present** in `crates/rustty-vt` (used at `widgets/pagelist.rs:319`), so this one is *not* blocked | — |
| `gtk-horizontal-tab-scroll` | the config option itself (`5ea45919d`, `src/config/Config.zig`) | **G** |
| `bind-failed` notification strings | 3 new `i18n._()` msgids → `po/` | **G** |
| `vicinae-hotkey-v1.xml` vendoring + `wayland-scanner` in `build.rs` | build-system review (new codegen step, new dev-dependency) | **G** |
| apprt `internal_os.open()` losing its allocator arg; `src/os/env.zig` + `src/os/args.zig` DELETED upstream | `src/os` migration | **E** |
| `font/Collection` `SegmentedList` relocation | `src/datastruct/segmented_list.zig` (NEW, 533) | (datastruct — unassigned in the area table; flag to the program lead) |
| `font/shaper` test scaffolding (`node.page()`, `codepoint = .{ .data = … }`) | terminal page/cell API | **A–D** |

### Suggested ordering within area F

1. **Fix the test-binary tripwire** (§5 pre-work) — it gates every new GTK KAT.
2. **`macos/` block** (11 rows, mostly S, 2 arrive with tests, real `xcodebuild`
   gate, zero cross-area deps). Highest verified-value-per-hour in the area.
   Within it: `49806fc4c`+`acd09c0a6` first (they unblock `6e267d336`), then
   `0274e7ad8`+its test, then `a6edca2d7` before `35790a7e5` (the revert is only
   safe once the real fix is in).
3. **`renderer` small gaps**: `metal/buffer.rs` (S, real bug, real gate) then
   `link.rs` (S, real gate).
4. **`font`**: `svg.rs` test (S) now; `d0e72a3ab` (L) as its own task because of
   the z2d dependency.
5. **`apprt` non-hotkey**: `2f3814ca5` suspended (S), `5ea45919d` tab-scroll
   (after the config row lands), `004c88e41` clamp (M, partial verification).
6. **Hotkeys series** F-W1 → F-W2 → F-W3 → F-W4 (§3).
7. **Blocked-on-A–D**: `begin_update`/`end_update`, inspector limits — schedule
   after the terminal areas land, not before.
8. **Inspector CellChooser** (M) — not blocked; slot it wherever, but do not ship
   the compression story as "done" while this path can silently decompress.

### Open questions, each with a recommendation

1. **Should rustty carry an unratified, single-vendor Wayland protocol?**
   *Recommendation: yes — port it.* The port's contract is full fidelity to the
   oracle, and the arm is inert (returns `false`) on every compositor that does not
   advertise the global, so the blast radius is zero. But vendor the XML with its
   upstream copyright header intact and record in the port ledger that the protocol
   is testing-phase, so a future incompatible bump is expected rather than
   surprising.
2. **Keep rustty's "headless refresh builds the map" divergence from upstream's new
   early return?** *Recommendation: keep it, and document it.* Two existing KATs
   (`global_shortcuts.rs:974,990`) depend on it and it is strictly more testable;
   upstream's return is an optimisation with no observable difference once a bus
   exists.
3. **The `metal/buffer.rs` PORT NOTE says the over-allocation was preserved
   deliberately as a reference quirk. Are there other such notes now invalidated by
   upstream fixes?** *Recommendation: run a targeted sweep for
   `FAITHFULNESS QUIRK` / `latent reference quirk` / `NOT corrected` across
   `crates/` and cross-check each against `4749c4e93..origin/main`.* This one was
   found by reading; a grep-driven sweep is cheap and this failure mode (rustty
   faithfully carrying a bug the oracle has since fixed) is invisible to every
   existing gate. Suggest raising it to the program lead as an all-areas check, not
   just F.
4. **Is `gdk4-rs` sufficient for `004c88e41`?** `Toplevel::connect_compute_size` +
   `ToplevelSize::bounds`/`set_size` must exist in the pinned `gdk4` 0.11.
   *Recommendation: confirm before estimating; if the binding is missing the cost
   moves from M to L (manual `gdk_toplevel_size_*` FFI).*
5. **How much of the Zig-0.16 "N/A" bucket deserves a second pass?** I classified
   ~50 files from the diff text without re-reading their rustty counterparts.
   *Recommendation: a single cheap sweep of the 0.16 hunks looking only for
   changed constants, changed control flow, and changed error-handling arms
   (rather than type/API renames)* — that is where a semantic change can hide. The
   two I checked were clean, which is weak evidence, not proof.
