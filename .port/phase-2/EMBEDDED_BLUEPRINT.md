# EMBEDDED_BLUEPRINT — the libghostty C-ABI (apprt/embedded)

Architect blueprint for the `apprt/embedded` milestone: the libghostty C-ABI, the
FIRST concrete apprt backend that proves the apprt-api trait seam. Grounded in
`src/apprt/embedded.zig` (2250 loc, 0 tests), `none.zig` (19), `browser.zig` (4),
the apprt-api seam (`seam.rs`/`surface.rs`/`action.rs`), rustty-core
(`surface.rs`/`lib.rs`), and the vt-c C-ABI precedent (`cbindgen.toml`,
`point.rs`/`sgr.rs` D2 padding, `xtask header-gen`).

**Adversarially reviewed: ACCEPT-WITH-FIXES (2026-07-02) — the 9 fixes below are folded in**
(marked `[ADV-FIX n]` at their sites). The architecture (separate apprt-c top-sink, backend
in rustty-apprt, cbindgen+D2 const-asserts, acyclic graph, complete trait coverage, honest
GPU/xev split) survived; fixes are all local:
1. Export tally sums to **71** (Surface render/window is **8** — `_tty_name` belongs there; the
   runtime-safe list is **14** fns, corrected in §1).
2. **Smoke KAT rewrite (§6.3):** embedded's 71 are NOT self-sufficient — a C host builds the
   config via the ALREADY-PORTED rustty-config C-ABI (`rustty_config_new`/`_finalize`/`_free`,
   config C3) and bootstraps via `rustty_init`. The KAT links BOTH apprt-c and the config
   C-ABI; it does not fabricate a `&cfg`.
3. **`ghostty_set_window_background_blur` is NOT pure** (§1/§3/§8.1): its Darwin body does objc
   `msgSend` + private CoreGraphics `CGSSetWindowBackgroundBlurRadius`. Reclassify as a macOS
   objc/CGS leaf (`#[cfg(target_os="macos")]` objc glue or `unimplemented!()`); DROP from the
   runtime-safe list; smoke KAT must not call it.
4. `CoreSurfaceHandle::rt_surface_ptr` is **rustty-core's** (already landed, surface.rs:12781,
   returns `self.rt_surface.cast()` — NOT `self`), not embedded-backend work. §2 corrected.
5. Renderer runtime leaf is `Renderer::surfaceInit` (surface.rs:**1620**), not the field decls
   :324/333 (those are struct-field declarations). §1/§8.3 corrected.
6. **CLI-precede caveat:** the `cli/action/{list_fonts,show_config,list_keybinds}.zig` impls are
   a SEPARATE unported subsystem — "PRECEDE" adds unscoped work; flagged as such in §1, not "free".
7. **fn-ptr-ABI backstop overstated (§3/§6.1/§8.2):** `size_of::<Options>()` is INVARIANT to
   callback-signature drift (all fn-ptrs are `usize`-sized; `Option<extern"C"fn>` uses the null
   niche → identical size). The REAL backstop is the cbindgen-emitted typedef + the C smoke test
   passing REAL typed stub callbacks (compile-time type-check against the generated typedef).
8. **Self-referential pinning invariant (§2):** `embedded::Surface` holds `core_surface:
   Surface<App>` by value and core `Surface::init` writes `core_surface.rt_surface =
   &embedded_surface` (the container) — so the embedded Surface MUST be heap-boxed and never
   moved post-init or `rt_surface` dangles. Made explicit.
9. **pub-widening:** the backend/wrapper split means apprt-c's `ghostty_*` wrappers touch only
   `pub` items — internals the reference accesses file-privately (`core_surface`, `app`,
   `inspector`, `title`, `opts`, `Point.pin`, `Selection.core`) become `pub` or gain accessors
   (vt-c shows this is workable).

---

## 1. Framing decision: what lands NOW vs Phase-3 — and the honest fraction

**Doctrine (CORE_BLUEPRINT C0):** big GUI files port LIVE via inhabited placeholder
structs whose GPU/xev methods are `unimplemented!()` leaves. Applied here that means:

- **ALL 71 `ghostty_*` exports compile, link, and header-generate NOW.** The C-ABI
  contract — every symbol, every extern struct, every callback typedef, every
  const-size-assert, the generated header, the C compile+link — is 100% achievable
  now. That is the deliverable.
- **A subset PANICS at runtime** because it transitively reaches the GPU renderer
  (`renderer_seam::Renderer`/`Thread`, Phase-3 R-d) or the xev loop (D6).

**The runtime boundary is `ghostty_surface_new`.** Core `Surface::init`
(surface.rs) constructs `renderer: crate::renderer_seam::Renderer` +
`renderer_thread: renderer_seam::Thread<A>` (surface.rs:324/333) — both Phase-3
placeholder leaves. So **any live surface hits the leaf**. The clean split:

**Runtime-exercisable NOW (no GPU/xev)** — **14** of 71, all app/config/pure-marshal:
`ghostty_app_new`/`_free`/`_userdata`/`_tick`†/`_set_focus`/`_key`/
`_keyboard_changed`/`_needs_confirm_quit`/`_has_global_keybinds`/
`_set_color_scheme`/`_open_config`, `ghostty_config_key_is_binding`,
`ghostty_surface_config_new` (returns a zeroed `Options`, pure).
(†`_tick` is safe only while no surface exists — it drains the app mailbox.)
`app_new` builds `CoreApp` + keymap + config clone; no renderer, no xev.
[ADV-FIX 3] `ghostty_set_window_background_blur` is NOT in this list — its Darwin
body does objc `msgSend` + private CGS calls (embedded.zig:2114-2127); it is a
macOS objc/CGS leaf (E10), and the smoke KAT must not call it.

**Compiles+links NOW, panics when called (Phase-3 leaf)** — the other ~56:
everything `ghostty_surface_*` that needs a live surface (new/draw/refresh/size/
key/mouse/text/selection/split/…), all `ghostty_inspector_*` (dcimgui+Metal), and
the Darwin `quicklook`/`metal`/`display_id` block.

**Honest fraction:** **C-ABI surface = 100% now; runtime-live coverage ≈ 21%
(15/71) until R-d+D6.** The rendering/event-loop half is genuinely deferred — do
not pretend otherwise.

### RECOMMENDATION (decisive)

1. **Land this milestone as "the C-ABI contract + non-GPU backend", rendering
   deferred.** Verified by: Rust const-size-asserts (D2) + cbindgen header +
   **C compile-AND-LINK smoke KAT** that calls the ~15 non-surface fns (a step up
   from vt-c's header-only gate). This is a real, gate-green deliverable: the
   published `librustty.{dylib,a}` + header, callable by a host, with the live
   surface path a documented Phase-3 seam.
2. **Interleave a minimal headless `rustty` CLI bin over the `none` backend
   (§4) — recommend it PRECEDE the embedded C-ABI.** `none` has zero rendering;
   a CLI (`ghostty +list-fonts`/`+show-config`/`+list-keybinds`-style, over the
   already-ported rustty-args/terminfo/config/vt/font) is a **tangible headless
   deliverable that exercises real ported code end-to-end with no GPU/xev leaf at
   all** — a stronger "it runs" signal than a C smoke that can't open a surface.
   The embedded C-ABI proves the *trait seam*; the CLI proves the *stack runs*.
   [ADV-FIX 6] CAVEAT: the `cli/action/{list_fonts,show_config,list_keybinds}.zig` action
   impls are a SEPARATE, not-yet-ported subsystem — "PRECEDE" is NOT free; scope the CLI as
   its own milestone (own design pass), not a freebie bundled into embedded.

---

## 2. The concrete backend: embedded App/Surface implementing apprt-api traits

This is the FIRST concrete `A` — it proves the self-referential `type Core` bound
compiles against a REAL backend (core's C0 gate used a dummy `A`).

**Binding:** `A = embedded::App`. Then:
- `impl apprt_api::App for embedded::App { type Surface = embedded::Surface; … }`
- `impl apprt_api::Surface for embedded::Surface { type App = embedded::App;
  type Core = rustty_core::Surface<embedded::App>; … }`
- `embedded::Surface` holds `core_surface: rustty_core::Surface<embedded::App>`
  (reference `core_surface: CoreSurface`, embedded.zig:414). Core is instantiated
  as `Surface<embedded::App>` — satisfying core's `A::Surface: Surface<App = A,
  Core = Surface<A>>` bound (core lib.rs C0 gate) with a live backend.

### `embedded::App` (embedded.zig:29–342)
```
pub struct App {
    core_app: *mut rustty_core::App<App>,   // reference core_app: *CoreApp
    opts: Options,                          // the host C callbacks (below)
    keymap: rustty_input::Keymap,
    config: rustty_config::Config,          // owned clone
}
```
`Options` = `ghostty_runtime_config_s`, an **`#[repr(C)]`** struct of host callbacks
(embedded.zig:35–81). Faithful field-for-field, `callconv(.c)` → `extern "C" fn`:
```
#[repr(C)] pub struct Options {
    userdata: *mut c_void,
    supports_selection_clipboard: bool,
    wakeup:  extern "C" fn(*mut c_void),
    action:  extern "C" fn(*mut App, TargetC, ActionC) -> bool,
    read_clipboard: extern "C" fn(*mut c_void, c_int, *mut ClipboardRequest) -> bool,
    confirm_read_clipboard: extern "C" fn(*mut c_void, *const c_char, *mut ClipboardRequest, ClipboardRequestType),
    write_clipboard: extern "C" fn(*mut c_void, c_int, *const ClipboardContent, usize, bool),
    close_surface: Option<extern "C" fn(*mut c_void, bool)>,  // reference `?*const fn` → nullable
}
```
`TargetC`/`ActionC` already exist in apprt-api (`action.rs`, D2-asserted). **App
trait methods** map to real bodies (not callbacks): `perform_action` →
`performPreAction` + `self.opts.action(self, target.cval(), action.cval())`
(embedded.zig:267); `perform_ipc` → the static `performIpc` (returns false for
both keys); `wakeup` → `self.opts.wakeup(self.opts.userdata)`; `config` → `&self.config`;
`keyboard_layout` → macOS `keymap.sourceId` detection else `Unknown`.

### `embedded::Surface` (embedded.zig:410–990)
```
pub struct Surface {
    app: *mut App,
    platform: Platform,               // holds raw nsview/uiview *mut c_void (no objc dep — §8)
    userdata: *mut c_void,
    core_surface: rustty_core::Surface<App>,
    content_scale: ContentScale,
    size: SurfaceSize,
    cursor_pos: CursorPos,
    inspector: Option<*mut Inspector>,
    title: Option<CString>,           // backend stores title so getTitle works
}
```
**Every apprt-api growth-point method → stored field or host callback** (all bodies
verified in embedded.zig; the seam listed them grep-complete in surface.rs:113):

| apprt-api method | embedded body | kind |
|---|---|---|
| `core()` | `&mut self.core_surface` (:633) | real |
| `rt_app()` | `self.app` (:637) | real |
| `close(alive)` | `self.app.opts.close_surface?(userdata, alive)` (:641) | **callback** (nullable) |
| `get_content_scale()` | `self.content_scale` (:650) | stored field |
| `get_size()` | `self.size` (:654) | stored field |
| `get_title()` | `self.title` (:658) | stored field |
| `supports_clipboard(t)` | standard→true; sel/primary→`opts.supports_selection_clipboard` (:662) | field |
| `clipboard_request(t,st)` | heap `state_ptr`, `opts.read_clipboard(userdata,t,ptr)` (:672) | **callback** |
| `set_clipboard(t,c,cfm)` | marshal `&[ClipboardContent]`, `opts.write_clipboard(...)` (:735) | **callback** |
| `get_cursor_pos()` | `self.cursor_pos` (:760) | stored field |
| `default_termio_env()` | `os::getEnvMap` + Darwin XCode-var scrub (:953) | real |
| `deinit()` | free title, `core_app.deleteSurface`, `core_surface.deinit` (:597) | real |

The **callbacks** (close/read/write clipboard) are the essence of "embedded": the
backend owns no windowing — it forwards to host C function pointers.
[ADV-FIX 4] `CoreSurfaceHandle::rt_surface_ptr` is NOT embedded-backend work — it is
implemented by **rustty-core's** `Surface<A>` (already landed, surface.rs:12781,
`self.rt_surface.cast()`, returning the pointer *to* this embedded backend). Do not
re-implement it here.

[ADV-FIX 8] **PINNING INVARIANT (correctness-critical):** `embedded::Surface` holds
`core_surface: Surface<App>` BY VALUE, and core `Surface::init` writes
`core_surface.rt_surface = &embedded_surface` (a pointer back to the container). The
embedded `Surface` MUST therefore be **heap-allocated (Box) and never moved after
init** — `ghostty_surface_new` returns a `*mut Surface` to a boxed, pinned backend;
any move dangles `rt_surface`. Mirror the reference's `alloc.create(Surface)` +
out-pointer `init(surface, ...)` exactly (same self-referential discipline core's own
`init` uses). [ADV-FIX 9] backend internals the apprt-c wrappers must reach
(`core_surface`, `app`, `inspector`, `title`, `opts`) become `pub`.

**Non-trait callback methods** (the surface event sinks the CAPI drives:
`refresh`/`draw`/`updateSize`/`mouseButtonCallback`/`cursorPosCallback`/… :764–920)
delegate straight to `core_surface.*Callback`. `draw`/`refresh` reach the GPU leaf
at runtime; the rest are real core logic.

---

## 3. The C-ABI deliverable (71 exports)

### Crate placement — RECOMMEND a separate `rustty-apprt-c` (vt-c precedent)

Mirror vt-c exactly: **pure-Rust backends live in `rustty-apprt`** (modules
`embedded`/`none`/`browser`, impl the apprt-api traits, rlib, unit-testable); **the
C artifact is a new `rustty-apprt-c`** — `crate-type = ["cdylib","staticlib","rlib"]`,
deps `rustty-apprt`, contains ONLY the 71 `#[no_mangle] pub unsafe extern "C" fn
ghostty_*` wrappers + the CAPI-only extern structs + `cbindgen.toml` + const-asserts.
Rationale: identical to `rustty-vt` (logic rlib) / `rustty-vt-c` (C wrapper cdylib);
keeps the backend testable without the C toolchain, isolates the `librustty.{dylib,a}`
artifact, and keeps cbindgen scoped to one thin crate. (`App.Options` is C-ABI but is
a *backend field*, so it lives in `rustty-apprt::embedded`; apprt-c hand-pins it in
cbindgen `after_includes`, per vt-c's cross-crate-type pinning discipline.)

### cbindgen strategy (copy vt-c's `cbindgen.toml`)
- `language=C`, `cpp_compat=true` (compiles as C AND C++), `style=type`,
  `documentation_style=doxy`, `parse_deps=false`.
- Emit the `#[no_mangle]` surface from apprt-c; **hand-pin cross-crate C types** in
  `after_includes` (vt-c does this for speed+determinism): `App.Options`
  (`ghostty_runtime_config_s`) with its 7 callback typedefs, `Platform.C` union,
  `EnvVar`, `TargetC`/`ActionC` (already repr(C) in apprt-api), `ClipboardRequest`/
  `ClipboardRequestType`, `Clipboard`, `NewSurfaceContext`.
- `xtask header-gen` currently emits `include/rustty.h` from vt-c. **Extend it**: add
  a `header-gen-apprt` (or merge) emitting `include/rustty-apprt.h` from apprt-c.

### PINNED `[u64;N]` padding + const-asserts (D2) — which structs
Extern structs crossing the ABI, each getting `const _: () = assert!(size_of::<T>()
== …); assert!(align_of::<T>() == …)` in apprt-c (vt-c point.rs:93 / sgr.rs:178):

- **Tagged unions → PIN `_padding: [u64;N]`** (vt-c precedent): `Platform.C`
  (`extern union { macos{nsview}, ios{uiview} }`, embedded.zig:363) — pointer-sized
  members, pad to the reference union size. `TargetCValue`/`ActionCValue` already
  pinned+asserted in apprt-api (`ActionCValue==24`); apprt-c re-asserts nothing there.
- **Plain extern structs → size/align assert only** (no padding needed):
  `Options` (assert `size_of` == reference `ghostty_runtime_config_s`),
  `KeyEvent` (:1248), `SurfaceSize` (:1277, `{u16,u16,u32,u32,u32,u32}`),
  `ClipboardContent` (:1287), `Text` (:1293), `Point`/`PointC` (:1309, tag+`{u32,u32}`),
  `Selection` (:1371, `{Point,Point,bool}`), `EnvVar` (:402).
- **Callback fn-ptr ABI [ADV-FIX 7]:** `size_of::<Options>()` does NOT catch callback
  SIGNATURE drift (all fn-ptrs are `usize`-sized; `Option<extern"C"fn>` uses the null
  niche → identical size). The real backstop is the **cbindgen-emitted callback typedef
  + the C smoke KAT passing REAL typed stub callbacks** (the C compiler type-checks the
  stubs against the generated typedefs at compile time). `close_surface` MUST be
  `Option<extern "C" fn>` (reference `?*const fn`), the other six non-nullable
  `extern "C" fn`. Keep the `size_of::<Options>()` assert too (catches field add/remove),
  but it is not the signature guard.

### The 71 exports grouped by subsystem
- **App lifecycle (11):** `ghostty_app_new`, `_tick`, `_userdata`, `_free`,
  `_set_focus`, `_key`, `_keyboard_changed`, `_open_config`, `_needs_confirm_quit`,
  `_has_global_keybinds`, `_set_color_scheme`.
- **App/config (2):** `ghostty_app_update_config`, `ghostty_config_key_is_binding`.
- **Surface lifecycle (7):** `ghostty_surface_config_new`, `_new`, `_free`,
  `_userdata`, `_app`, `_inherited_config`, `_update_config`.
- **Surface state/query (8):** `_needs_confirm_quit`, `_process_exited`,
  `_has_selection`, `_read_selection`, `_read_text`, `_free_text`, `_size`,
  `_foreground_pid` (+`_tty_name`).
- **Surface render/window (7):** `_refresh`, `_draw`, `_set_size`, `_tty_name`,
  `_set_color_scheme`, `_set_content_scale`, `_set_focus`, `_set_occlusion`. *(GPU leaf)*
- **Input (11):** `_key_translation_mods`, `_key`, `_key_is_binding`, `_text`,
  `_preedit`, `_mouse_captured`, `_mouse_button`, `_mouse_pos`, `_mouse_scroll`,
  `_mouse_pressure`, `_ime_point`.
- **Surface actions (6):** `_request_close`, `_split`, `_split_focus`,
  `_split_resize`, `_split_equalize`, `_binding_action`.
- **Clipboard (1):** `ghostty_surface_complete_clipboard_request`.
- **Inspector (10):** `ghostty_surface_inspector`, `ghostty_inspector_free`,
  `_set_size`, `_set_content_scale`, `_mouse_button`, `_mouse_pos`, `_mouse_scroll`,
  `_key`, `_text`, `_set_focus`. *(dcimgui/Metal Phase-3 leaf)*
- **Misc (1):** `ghostty_set_window_background_blur`. *([ADV-FIX 3] macOS objc/CGS leaf — E10, `#[cfg(target_os="macos")]`; not runtime-safe.)*
- **Darwin-gated (6):** `ghostty_surface_set_display_id`, `_quicklook_font`,
  `_quicklook_word`, `ghostty_inspector_metal_init`, `_metal_render`, `_metal_shutdown`.
  *(objc/Metal/coretext Phase-3 leaf)*

(11+2+7+8+7+11+6+1+10+1+6 = 71 ✓)

**Global-state precursor (RISK, flag):** the CAPI reads `global.alloc`
(`ghostty_surface_free_text`, `_tty_name`, `Text.deinit`). In rustty-core this is the
`STATE: OnceLock<GlobalState>` (lib.rs). The reference sets `global.state` in
`ghostty_init` (`main_c.zig`, NOT embedded.zig). **Port a minimal `rustty_init`
export (or lazily init STATE in `ghostty_app_new`) as an apprt-c precursor** — the
smoke KAT must call it before `ghostty_app_new`.

---

## 4. none.zig + browser.zig

- **`none` (19 loc) = the headless/CLI IPC backend.** Its `App` has ONLY the static
  `performIpc` (returns false); `Surface = struct {}` empty. It does **NOT** implement
  the full `apprt_api::App`/`Surface` traits (faithful: reference `none.App` has no
  `performAction`/`wakeup`). Port as `rustty-apprt::none` — a plain module with a
  free/assoc `perform_ipc` fn, no trait impls. This is the backend the headless
  `rustty` CLI bin sits on (§1 rec 2). Honest and minimal.
- **`browser` (4 loc) = trivial wasm stub.** `App = struct {}`, `Window = struct {}`.
  Port as `rustty-apprt::browser` empty structs, no traits. Defer everything else to
  the (unscheduled) wasm milestone.

---

## 5. Acyclicity + Cargo deps

- **`rustty-apprt-c` is a NEW TOP SINK.** Deps: `rustty-apprt` (+ `libc`/`core::ffi`).
  Nothing deps apprt-c. Edge chain: `apprt-c → apprt → {core, apprt-api, vt, font,
  renderer, inspector, config, input, os}`; `core → apprt-api` (never apprt);
  `apprt-api → {config, input, vt, datastruct, renderer, render-types}` (never
  core/apprt). No back-edge → **Tarjan `check_acyclic.sh` stays green.** Add
  `crates/rustty-apprt-c` to workspace `members` + the internal path-dep list.
- **New deps:** **NONE required for the non-GPU milestone.** `Platform` holds raw
  `*mut c_void` for nsview/uiview (embedded.zig:353 wraps them via `objc.Object.fromId`,
  but nothing is *messaged* until Phase-3 rendering) → **no `objc2`/`coretext` now.**
  The Darwin `metal`/`quicklook` block (which WOULD need objc2/coretext) is Phase-3
  `unimplemented!()` leaves, `#[cfg(target_os)]`-gated, deferred. `dcimgui` (Inspector)
  is likewise Phase-3.

---

## 6. Test / verification accounting (embedded/none/browser = 0 unit tests)

No `#[test]` counterparts exist to port (all three files are 0-test). The gate is
the vt-c C-ABI discipline, made stricter:

1. **Rust const-size/align-asserts (D2)** on every ABI struct in §3 — build-failure
   gate if layout drifts (vt-c point.rs:93 pattern). `assert!(size_of::<Options>()==N)`
   is the fn-ptr-ABI backstop.
2. **cbindgen header generation** via extended `xtask header-gen` — must succeed and
   the header must compile standalone as **C and C++** (`cpp_compat=true`), vt-c gate.
3. **C compile-AND-LINK smoke KAT (new, the deliverable's proof) — [ADV-FIX 2]:** a
   `tests/smoke.c` that `#include`s BOTH the apprt-c header and the already-generated
   rustty-config header, links the `staticlib`(s), and builds the config the way a real
   host must (embedded's 71 do NOT include config construction): `rustty_init()` →
   `cfg = rustty_config_new()` → `rustty_config_finalize(cfg)` (config C-ABI, C3, already
   shipped) → `rt_cfg = ghostty_surface_config_new()` → `app = ghostty_app_new(&rt_cfg,
   cfg)` with REAL typed `extern "C"` no-op callback stubs (so the compiler type-checks
   them against the cbindgen-emitted callback typedefs — the actual fn-ptr-ABI backstop,
   NOT `size_of` per [ADV-FIX 7]) → `ghostty_config_key_is_binding(...)` →
   `ghostty_app_needs_confirm_quit(app,...)` → `ghostty_app_free(app)` →
   `rustty_config_free(cfg)`. Driven by a Rust integration test that shells `cc`/`clang`
   (or a `cc`-crate build-dep), asserting compile+link+run exit 0. **It MUST NOT call
   `ghostty_surface_new`** (GPU leaf) **or `ghostty_set_window_background_blur`** (objc leaf,
   [ADV-FIX 3]). Step up from vt-c (header self-consistency only).
4. **G3b scoped comment coverage** per chunk (§7).
5. **`check_test_counts` MAP rows** for the three 0-test files
   (`GHAPPRT=/ghostty/src/apprt`, `RSAPPRT=/rustty/crates/rustty-apprt/src`):
   `embedded`→zig 0 / rust ≥0, `none`→0, `browser`→0 (documented 0-test modules,
   same treatment as other 0-test files). apprt-c's `#[no_mangle]` wrappers add
   rustty-only helper `#[test]`s freely (rust ≥ zig always holds).

---

## 7. Chunked execution plan (skeleton → fill; 2250 loc)

Order E0→E1→E2→E3→E5→E6…E10→E11; **E4 (Inspector) deferrable/parallel** (pure
Phase-3 leaf). Each chunk gates on scoped-G3b + acyclic + build (const-asserts).

| Chunk | embedded.zig range | Content |
|---|---|---|
| **E0** | scaffold | `rustty-apprt-c` crate (Cargo cdylib/staticlib/rlib + cbindgen.toml, vt-c copy); `rustty-apprt::{embedded,none,browser}` module shells; wire `STATE`/`rustty_init` precursor; `none`+`browser` stubs DONE here (§4) |
| **E1** | 29–342 | `embedded::App` + `Options` (callbacks) + `KeyEvent` + init/terminate/focusEvent/keyEvent/reloadKeymap/keyboardLayout/wakeup/wait/newSurface/closeSurface/performAction/performPreAction/performIpc; **`impl apprt_api::App`** |
| **E2** | 343–609 | `Platform`/`PlatformTag`/`EnvVar` (+union pad-assert) + `Surface` struct + `Options` + `init`/`deinit`; **`impl apprt_api::Surface`** (the §2 growth-point table); `CoreSurfaceHandle` |
| **E3** | 610–990 | Surface callbacks: refresh/draw/updateContentScale/updateSize/colorScheme/mouse*/scroll/cursorPos/preedit/text/focus/occlusion/completeClipboardRequest/newSurfaceOptions/defaultTermioEnv/cursorPosToPixels (draw/refresh = runtime GPU leaf) |
| **E4** | 992–1243 | `Inspector` (dcimgui+Metal) — **Phase-3 inhabited placeholder, `unimplemented!()` leaves**; non-GPU set_size/set_content_scale real. *Deferrable.* |
| **E5** | 1244–1397 | CAPI extern structs → apprt-c: KeyEvent/SurfaceSize/ClipboardContent/Text/Point(+PointC)/Selection + **all D2 const-asserts** |
| **E6** | 1398–1537 | CAPI **app-lifecycle** exports (11) — the runtime-exercisable core |
| **E7** | 1538–1760 | CAPI **surface lifecycle + state/query** exports |
| **E8** | 1763–1922 | CAPI **input** exports (key/text/preedit/mouse) |
| **E9** | 1924–2106 | CAPI **surface-actions + clipboard + inspector** exports |
| **E10** | 2108–2250 | **misc + Darwin-gated** exports (blur, display_id, quicklook, metal_*) — Phase-3 `#[cfg]` leaves |
| **E11** | — | **C-ABI smoke KAT** (its own chunk): extend `xtask header-gen`, add `tests/smoke.c` + the cc-driving Rust integration test; full const-assert audit; `check_test_counts` MAP rows |

---

## 8. Risks / ambiguities + resolutions

1. **macOS objc/window glue fraction.** Only `Platform` (nsview/uiview),
   `Inspector` Metal, and `quicklook_font/word` touch objc/coretext.
   *Resolution:* `Platform` holds raw `*mut c_void` (never messaged pre-render) → no
   objc2 dep now; Metal/quicklook/inspector are `#[cfg(target_os)]` Phase-3
   `unimplemented!()` leaves. Non-Darwin glue fraction of the milestone ≈ 0.
2. **Callback-pointer ABI exactness.** 7 fn-ptr fields in `Options`; drift in
   signature or nullability is silent UB. *Resolution:* `close_surface` =
   `Option<extern "C" fn>` (reference `?*const fn`), others non-null `extern "C" fn`;
   `assert!(size_of::<Options>()==reference)` + the smoke KAT passing REAL stub
   callbacks catches drift at build+link time.
3. **GPU/xev boundary honesty.** `ghostty_surface_new → core Surface::init →
   renderer_seam::{Renderer,Thread}` (surface.rs:324/333) = Phase-3 `unimplemented!()`.
   *Resolution:* symbol links, calling panics; smoke KAT stays app-level; document
   "live surface = Phase-3 (R-d + D6)". This is the milestone's honest line.
4. **`global.state` precursor.** CAPI free/tty_name/Text.deinit use `global.alloc`
   (set by `ghostty_init` in `main_c.zig`, not embedded.zig). *Resolution:* port a
   minimal `rustty_init` export (or lazy-init `STATE` in `ghostty_app_new`) in apprt-c;
   smoke calls it first.
5. **`String`/`Text` ownership across ABI.** `tty_name`/`read_text` return
   heap C-strings freed by `ghostty_surface_free_text`/dedicated frees.
   *Resolution:* faithful `global.alloc` alloc/free pair; assert `Text` layout;
   most of these are surface-bound (Phase-3 runtime) so not smoke-exercised.
6. **`app_new_` / `CoreApp.create` spawns nothing GPU/xev** (verified
   embedded.zig:1413 — allocates app+keymap+config only). *Resolution:* app-level
   smoke is genuinely leaf-free — confidence the 15-fn runtime subset is real.
