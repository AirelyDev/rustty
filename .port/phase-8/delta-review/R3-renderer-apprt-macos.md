# Phase 8 delta review R3: renderer, GTK, macOS Swift, core, CLI, config, termio

Range `12752b2ac..a60e9e2a5` (`origin/main` in `../ghostty`, read with `git show` / `git diff`; the
oracle working tree stayed at the pin). Rustty at `401b95b0e` (branch `phase-8-upstream-resync`).
Read-only review: no Rustty code changed and nothing was built. Upstream line numbers are at
`a60e9e2a5`; Rustty line numbers are at `401b95b0e`.

**Scope.** `src/renderer.zig`, `src/renderer/{Metal,OpenGL,Options,WebGL,backend,generic,shadertoy}.zig`,
new `src/renderer/{metal,opengl}/Device.zig`, `pkg/opengl/egl.zig`, `src/App.zig`, `src/Surface.zig`,
`src/apprt/gtk/class/surface.zig`, `macos/Sources/**`, `src/cli/{ssh,ssh_cache}.zig`,
`src/cli/ssh-cache/DiskCache.zig`, `src/config/Config.zig`, `src/termio/{Termio,stream_handler}.zig`.
28 files, +845 / −259. 20 non-merge commits touch them (`git log --no-merges 12752b2ac..a60e9e2a5
-- <paths>`); the six vt-side commits among them (`9272a2f7c`, `00a19db51`, `01b9f4958`, `b1bfd1d9c`,
`7b11f3dca`, `bae2c3cdb`) are reviewed here only for their config/termio halves; R1/R2 own the
terminal side. `src/apprt/embedded.zig`, `include/ghostty.h` and the rest of `src/apprt/**` are
unchanged in the range.

**Method.** Full diff of every in-scope file; the two new `Device.zig` files read whole; each change
mapped to Rustty by grep across the whole workspace (`crates/`, `crates-sys/`, `macos/`, `.port/`)
and read at the site. Records checked: `UPSTREAM-DEFECTS.md` (UD20, UD247, UD356 overlap),
`NEW-FINDINGS.md`, `VERIFICATION_DEBT.md` (WS14 WebGL note).

**Tests in scope** (added / changed / removed `test "…"` blocks):

| file | test | sha | kind |
|---|---|---|---|
| `src/renderer/shadertoy.zig:357` | `custom shader selection uniform layout matches compiled GLSL` | `80456f057` | added |
| `src/Surface.zig:6652` | `promptClickRelativeRow` | `5170b06f6` | added |
| `src/cli/ssh-cache/DiskCache.zig:838` | `test splitHostPort` (decl test) | `6fbb560cd` | added |
| `src/cli/ssh-cache/DiskCache.zig:858` | `test isValidCacheKey` (decl test) | `6fbb560cd`, `dfd7bafcd` | changed (+15 asserts) |
| `src/cli/ssh.zig:681` | `parseDestination: non-default port` | `6fbb560cd` | added |
| `src/cli/ssh.zig:695` | `parseDestination: port before hostname` | `6fbb560cd` | added |
| `src/cli/ssh.zig:704` | `parseDestination: IPv6 hostname with non-default port` | `6fbb560cd` | added |
| `src/cli/ssh.zig:713` | `parseDestination: result is a valid cache key` | `6fbb560cd` | added |
| `src/cli/ssh_cache.zig:375` | `test matchesQuery` (decl test) | `6fbb560cd`, `dfd7bafcd` | changed (+18 asserts) |
| `macos/Tests/.../TerminalControllerRestorationTests.swift` | whole file | `934ef8140` added, `0e75d015f` removed | net none |

No test is removed in the end state. `Config.zig`, `Termio.zig`, `stream_handler.zig`, `egl.zig`,
`App.zig`, `generic.zig`, `Metal.zig`, `OpenGL.zig`, both `Device.zig` and GTK `surface.zig` add none.

## 1. Summary table

Size: S under 2 h, M under 1 day, L more than 1 day. "Obs." = observable behaviour (Y) or internal (N).
"Lane" = where it can be compiled and verified (mac = this Mac, rvm = Linux VM, rwin = Windows VM).

| # | commit(s) | theme | obs.? | Rustty status | size | lane | top risk |
|---|---|---|---|---|---|---|---|
| 1 | `05392dcc9` | remove the stub WebGL renderer and `Backend.webgl` | N (build tag) | ABSENT: Rustty carries the stub in 10 places | S | mac + rvm (wasm32 build check) | an exhaustive `match` missed in a cfg'd crate |
| 2 | `40d5b860d` | app-scoped render `Device` (Metal + OpenGL); `App.device`; `Options.device`; warmup moves to `Device`; `CreateError.DeviceFailed` | Y (GTK without EGL now fails at App create, not per surface) | ABSENT | L | mac + rvm + rwin (build) | SB: render threads read `App.device` while main thread retags `&mut App`; core App tests (Miri, GPU-less) now need a device |
| 3 | `2e36658c6` | EGL: say why the surfaceless display can't be created (`queryClientExtensions`, `hasExtension`, `logUnsupportedPlatform`) | Y (log text) | PARTIAL, different shape: Rustty pre-checks the extension and fails before `eglGetPlatformDisplay` | S | rvm | three distinct messages need three triggers |
| 4 | `140feb86a` | `threadExit` marks the display unrealized so shaders are always released and no later draw runs | Y (macOS: no draw after thread exit) | PARTIAL: Metal shaders are freed by `Drop` (no leak) but `display_realized` stays true | S | mac + rvm | both twins must change together |
| 5 | `80456f057` | custom-shader selection uniforms swapped (std140 layout) | Y | ALREADY-HAS: UD20 fixed the same way; **FIXED-SAME** | S | mac | record update only + upstream-named test |
| 6 | `0d82239b5` | GTK warns once about missing/invalid DPI | Y (log volume) | ABSENT | S | rvm | per-site flag must be atomic in Rust |
| 7 | `5170b06f6` | relative prompt click row computed in screen space (across pages / history) | Y (bug fix) | ABSENT | S | mac | `point_from_pin` unwraps |
| 8 | `6fbb560cd` + `dfd7bafcd` | ssh-terminfo cache key carries the port; `splitHostPort`; query semantics | Y | ABSENT | M | mac | IPv6 bracket rules; 4+1 tests |
| 9 | `00a19db51` + `9272a2f7c` + `01b9f4958` (config/termio halves) | `vt-xt-checksum-report`, `vt-xt-checksum-extension`, XTCHECKSUM/DECRQCRA plumbing | Y (opt-in) | ABSENT (needs R1/R2's `xt_checksum` vt module) | M | mac | config key order; packed-struct `no-` parsing |
| 10 | `b1bfd1d9c` (termio half) | empty OSC 22 resets the mouse shape | Y | ABSENT (needs vt `mouse_shape_reset` action) | S | mac | none |
| 11 | `7b11f3dca`, `bae2c3cdb` (termio half) | `osc_unknown`, `program_status` actions ignored by termio | N | ABSENT (needs the vt actions) | S | mac | none |
| 12 | `136029788` + `934ef8140` + `0e75d015f` | macOS: `window-save-state=never` disables restoration; re-synced on config reload | Y | ABSENT | S | mac | none |
| 13 | `e2721d093` | macOS: never show the Kitty clipboard program name in the prompt | Y (security) | ABSENT (Rustty shows it) | S | mac | none |
| 14 | `be595d8f0` | macOS: drop `#if compiler(>=6.4)` around the macOS 27 code | N (build) | ABSENT | S | mac | must still build on Xcode 26.6 / Swift 6.3.3 (this Mac) |

## 2. Per-theme sections

### 2.1 Remove the WebGL stub (`05392dcc9`) — #1

**Upstream.** `src/renderer/WebGL.zig` deleted; `renderer.zig:37-45` is now
`pub const Renderer = GenericRenderer(GraphicsAPI)` with `GraphicsAPI = switch { .metal => Metal,
.opengl => OpenGL }`; `backend.zig` drops `webgl` and its wasm arm (`_ = wasm_target;` then
Darwin → metal, else → opengl). Nothing else in `src/` named it at either end (`git grep -i webgl
a60e9e2a5 -- src` is empty).

**Rustty (all must go).**
- `crates/rustty-renderer/src/webgl.rs` (whole file, incl. test `webgl_arm_exists_and_is_a_unit_placeholder`).
- `crates/rustty-renderer/src/lib.rs:165-169` (`pub mod webgl; pub use webgl::WebGL;`), `:259-264`
  (`#[cfg(target_arch = "wasm32")] pub use webgl::WebGL as Renderer;`), docs at `:3`, `:62-66`, `:229`.
- `crates/rustty-build-config/src/renderer.rs:15` (`Webgl`), `:20-24` (wasm arm). Port the end state
  literally: keep the `wasm_target` parameter, discard it (`let _ = wasm_target;`).
- Exhaustive matches: `crates/rustty-apprt/src/lib.rs:241` and the test list at `:728`,
  `crates/rustty-cli/src/version.rs:59-66`, `crates/rustty-crash/src/sentry.rs:795`.
- Comments: `crates/rustty-renderer/src/metal.rs:8-9,33-34`, `generic.rs:9-11`, `image.rs:24`,
  `crates/rustty-core/src/app.rs:252`.
- Records: `VERIFICATION_DEBT.md:267-277` (WS14 owns "the `renderer/WebGL.zig` placeholder") needs a
  note that the placeholder is gone upstream. The WS14 browser gate (`ws14_webgl_browser_gate.*`,
  `crates/rustty-wasm/tests/artifact_shape.rs`) tests the wasm module and its WebGL consumer, not
  the renderer arm, so it is unaffected; only its prose mentions the placeholder.

`@typeName(renderer.Renderer)` is unchanged on metal/opengl (`renderer.generic.Renderer(renderer.Metal)`
still; `GraphicsAPI` is an alias of the same struct), so the two remaining string arms keep their
text. `rustty-wasm` does not depend on `rustty-renderer` (checked `crates/rustty-wasm/Cargo.toml`), so
no wasm32 build pulls the renderer after the arm is gone; confirm with a
`cargo build -p rustty-wasm --target wasm32-unknown-unknown` (mac).

### 2.2 App-scoped render device (`40d5b860d`) — #2

**Upstream end state.**
- `src/renderer/metal/Device.zig` (149 lines): fields `device: objc.Object` (MTLDevice),
  `default_storage_mode`, `max_texture_size`; `init(self, alloc)` (chooseDevice + metadata + the
  `log.debug("device properties …")`), `deinit` (release), `warmup()`, `chooseDevice`,
  `queryMaxTextureSize`, all moved verbatim out of `Metal.zig`.
- `src/renderer/opengl/Device.zig` (96 lines): fields `display: *egl.Display`, `config: *egl.Config`;
  `init` does `egl.load()`, `initPlatform(SURFACELESS_MESA)`, the two `log.info` lines and
  `Config.choose` (moved from `OpenGL.init`); `deinit` does not terminate the display.
- `Metal.zig:44-45`: `device: *const Device`; `init(alloc, device, opts)` only creates the
  MTLCommandQueue; every `self.device` / `self.default_storage_mode` / `self.max_texture_size` read
  goes through `self.device.*`; `deinit` no longer releases the MTLDevice.
- `OpenGL.zig`: `device: *const Device`, `egl_context` stays per renderer ("Contexts are
  surface-scoped because a context can only be current on one thread at a time"); `bindApi` then
  `Context.create(display, device.config, …)` in `init`; `deinit`/`threadEnter`/`threadExit`/
  `present` use `self.device.display`.
- `Options.zig:7-9`: new first field `device: *renderer.Device`. `generic.zig:700-705` passes
  `options.device` to `GraphicsAPI.init`. `renderer.zig:45`: `pub const Device = GraphicsAPI.Device`.
- `App.zig:57-58` `device: renderer.Device`; `:76-78` `CreateError` gains `error{DeviceFailed}`;
  `:102-105` warmup probes `renderer.Device` (was `renderer.Renderer.API`); `:133-142` init sets
  `.device = undefined` then `device.init`, logging `failed to initialize render device err={}` and
  returning `error.DeviceFailed`; `:157-159` `deinit` calls `device.deinit()` last ("must happen after
  all surfaces are gone since their renderers borrow it").
- `Surface.zig:564`: `.device = &app.device`.

**Rustty today.**
- Metal: `crates/rustty-renderer/src/metal/metal_impl.rs:196-216` holds `device`,
  `default_storage_mode`, `max_texture_size` per renderer; `choose_device` `:258`,
  `query_max_texture_size` `:296`, `init_device` `:318-355`, `init_offscreen` `:362`, windowed
  `init` `:481`, `warmup` `:966`, reads at `:688-856`, `:896-902`.
- OpenGL: `crates/rustty-renderer/src/opengl/opengl_impl.rs:186-201` holds one
  `OffscreenContext` per renderer; `crates/rustty-renderer/src/opengl/gl/sys.rs:156-330`
  `OffscreenContext` owns the dlopen'd `libEGL.so.1` (`egl::DynamicInstance`), display, context and the
  two `EGL_MESA_image_dma_buf_export` entry points, all created per renderer.
- Options: `crates/rustty-renderer/src/generic.rs:1025-1050` (no `device`); core wrapper
  `crates/rustty-core/src/renderer_seam.rs:376-430`; call site
  `crates/rustty-core/src/surface/lifecycle.rs:275-288`.
- App: `crates/rustty-core/src/app.rs:150-200` (struct), `:200-262` (create + warmup calling
  `rustty_renderer::metal::Metal::warmup` at `:256`), `:277-298` (init, infallible after the grid
  set), `:302-327` (deinit); `CreateError = shared_grid_set::InitError` at `:182`.
- C API: `crates/rustty-apprt-c/src/lib.rs:563` maps `CoreApp::create` errors to
  `AppNewError::Create`.

**What to port.**
1. `crates/rustty-renderer/src/metal/device.rs` and `opengl/device.rs` (file-for-file with the new
   Zig files), `pub use … Device` per target from `lib.rs` (macOS Metal; linux/freebsd/windows OpenGL),
   `warmup` only on the Metal one (`@hasDecl` probe → only macOS spawns it).
2. `Metal`: replace the three fields with `device: *const Device`; `init_device` shrinks to the
   queue. `init_offscreen` and the test helpers take a `&Device` (tests build one first).
3. `OpenGL`: split `OffscreenContext` into the app-scoped half (`DynamicInstance`, display, config,
   the dmabuf entry points — `eglGetProcAddress` results are display-independent) and the per-renderer
   context. `bind_api` stays in `OpenGL::init` (it is per-thread EGL state and must run on the thread
   that creates the context, as upstream).
4. `generic::Options` gains `device: *const Device` first; core `renderer_seam::Options` forwards it;
   `lifecycle.rs:277` passes the app's device.
5. `App`: `CreateError` becomes an enum `{ GridSet(InitError), DeviceFailed }`; `App::init` logs the
   reference line and returns `DeviceFailed`; `deinit` tears the device down after the surface loop and
   the grid-set assert. Update `rustty-apprt-c` `AppNewError` and the GTK/`main_rustty.rs` error paths.

**Hazards.**
- **Stacked Borrows / data race (main).** Upstream embeds `device` by value in `App` and hands
  `&app.device` to every renderer, which reads it on its render thread. In Rust any later main-thread
  `&mut App` (e.g. `addSurface`, `tick`, `deinit` paths) retags every byte of `App`, popping the render
  threads' shared tags for the device bytes; Miri reports this as UB/data race. This is the N441/N444
  pattern (renderer and thread blocks moved out of line). Recommendation: keep the device in its own
  allocation (`DeviceBox(*mut Device)` from `Box::into_raw`, freed in `App::deinit` at the reference's
  point), and pass `*const Device` in `Options`. See Q1.
- **Thread safety.** A shared `*const Device` is read by N render threads concurrently. Write the
  soundness argument at the type: MTLDevice is documented thread-safe; EGL 1.5 requires display-level
  calls to be thread-safe; the fields are written once before any surface exists. No `Sync` impl is
  needed while only raw pointers cross threads, but say so.
- **Lifetime.** `App::deinit` deinits every surface before the device (`app.rs:304-315`), so the
  reference ordering holds by construction; on GTK verify on rvm (G-LSAN-GTK) that no core surface
  outlives `App::deinit`.
- **Core App tests.** 17 Rustty-only tests call `App::<TestApp>::init` (`app.rs:1607-2120`) and run
  under Miri (`app.rs:1577-1589` documents this). After the port `init` calls `MTLCopyAllDevices` /
  dlopens libEGL, which Miri cannot run and a GPU-less Linux runner cannot satisfy. See Q2.
- **Behaviour change (observable).** With no EGL driver, upstream GTK now fails `App.create` in
  `main` (process error), where at the pin it opened a window and failed per surface. Rustty's UD356
  measured trigger (`__EGL_VENDOR_LIBRARY_FILENAMES=/nonexistent.json`) now fails at App create, so
  `gtk_render_surface.rs::surface_init_failure_shows_the_error_page` needs a per-surface trigger
  (context creation, e.g. a GL-version override). UD356 itself stays valid: `setError` still has no
  `true` caller at `a60e9e2a5` (`gtk/class/surface.zig:2305`, `:3431`).
- **Windows (rwin).** The OpenGL `Device` now initialises in `App::init`; Rustty's Windows build uses
  the OpenGL renderer (N342) and dlopens `libEGL.so.1`, so any Windows test or embedder that creates a
  core App will now fail there. Upstream has the same shape; check rwin for affected tests.
- **Comments.** The moved doc comments and `Device.zig` headers move with the code (G3b); token
  rename `Ghostty` → `Rustty` in `opengl/Device.zig:69`.

UD candidates: none. (`App.zig:142`'s `errdefer self.device.deinit()` is after the last fallible
statement and therefore dead, which is harmless.)

### 2.3 EGL surfaceless diagnostics (`2e36658c6`) — #3

**Upstream.** `pkg/opengl/egl.zig:74-91` adds `queryClientExtensions()` (`eglQueryString(null,
EGL_EXTENSIONS)`, null when unsupported) and `hasExtension(exts, name)` (space tokenizer).
`opengl/Device.zig:33-40,65-89`: if `initPlatform` fails with `error.BadParameter`, log one of three
`log.err` messages (no client extensions → "no EGL driver could be loaded; …"; extension missing →
"the EGL driver does not support EGL_MESA_platform_surfaceless client extensions={s}"; otherwise "the
EGL driver rejected the surfaceless platform; client extensions={s}") and return the error.

**Rustty.** `crates/rustty-renderer/src/opengl/gl/sys.rs:188-200` queries the client extensions
first and returns `Error::MissingEglExtension("EGL_MESA_platform_surfaceless")` without calling
`get_platform_display`; `has_extension` exists at `:1033` with a test at `:1265-1270`;
`Error::BadParameter` at `:524,547`. So Rustty fails earlier with no log line, and cannot distinguish
"no driver" from "driver lacks the extension". Port: drop the pre-check, call
`get_platform_display`, and on `BadParameter` run `log_unsupported_platform` with the three verbatim
messages (scope `.opengl`). Keep `has_extension` as the `egl.zig` port. Tests: none upstream; add
Rustty-only log-capture tests on rvm for the three arms (no vendor JSON; a vendor without the
extension is hard to fake, so the middle arm may be unit-testable only through a split
`fn classify(exts: Option<&str>)`). See Q3.

### 2.4 Release shaders on render-thread exit (`140feb86a`) — #4

**Upstream.** `generic.zig:899-905`: inside `threadExit`'s locked block, `self.display_realized =
false;` before `releaseGpuResources()`, so `releaseGpuResources` frees the shaders (it only does
while unrealized) and `drawFrame` returns early afterwards (`:1736-1738` comment updated: "only the
case while unrealized (GTK) or after the render thread exits").

**Rustty.** ABSENT in both twins: `crates/rustty-renderer/src/generic/macos_impl/lifecycle.rs:547-566`
and `generic/linux_impl/lifecycle.rs:292-313` (`thread_exit_locked`), `release_gpu_resources` at
`macos_impl/lifecycle.rs:872-887`. Does Rustty leak? On macOS no: the shader pipelines are `Retained`
and are released when `RendererBox::dealloc_after_deinit` drops the renderer
(`renderer_seam.rs:349-357`); upstream leaked them. On Linux GTK unrealizes before destroy, and an
unreleased GL program dies with its context anyway. The part Rustty lacks is the second effect: a
`drawFrame` after thread exit (the main-thread sync draw path) still sees `display_realized == true`
and can rebuild a swap chain with no render thread. Port the one line in both
`thread_exit_locked`s, plus the comment edits at the draw early-return (`linux_impl/draw.rs:538` and
the macOS twin). Test (Rustty-only): after `thread_exit`, `display_realized` is false and the shaders
are defunct (Linux `opengl::shaders` has a `defunct` flag; Metal needs an equivalent probe).

### 2.5 Custom-shader selection uniform layout (`80456f057`) — #5

**Upstream.** `shadertoy.zig:40-41` swaps the two fields into GLSL order; new test at `:357` compiles a
shader with spirv-cross reflection and checks the offsets of the two selection members.

**Rustty.** Already fixed as **UD20** (`UPSTREAM-DEFECTS.md:382-392`): `crates/rustty-renderer/src/shadertoy.rs:158-171`
declares the fields in the same order, with const asserts at `:200-202` and the broader test
`globals_block_members_receive_the_uniform_fields_they_are_named_for` at `:1151` (checks every member).
Overlap verdict: **FIXED-SAME**. To do: mark UD20 upstream status fixed by `80456f057`; turn the
"reference defect (R13)" PORT NOTE at `:158-169` and `:200` into a plain port comment; update
`docs/ghostty-bug-fixes.md`; add the upstream-named test `custom shader selection uniform layout matches
compiled GLSL` (the existing `spirv_block_member_offsets` helper is enough; keep the Rustty test too).

### 2.6 GTK DPI warnings once (`0d82239b5`) — #6

**Upstream.** `apprt/gtk/class/surface.zig:1546,1558,1570` call `logWarnOnce`; helper at `:3948-3963`
keeps a per-(format, arg-type) `var flag` in a comptime-instantiated container, non-atomic by design
("A few duplicates aren't critical").

**Rustty.** `crates/rustty-apprt/src/gtk/class/surface/core_callbacks.rs:475-476,492,503` warn every
time. Port as a `log_warn_once!` macro with a per-call-site `static ONCE: AtomicBool` and `Relaxed`
loads/stores (a non-atomic static would be UB in Rust; the three sites have distinct formats, so
per-site and per-(format, type) keying coincide). Keep the helper's doc comments. Lane rvm.

### 2.7 Relative prompt click row (`5170b06f6`) — #7

**Upstream.** `Surface.zig:4336-4339` and new `promptClickRelativeRow` at `:4385-4393`: the row is
`pointFromPin(.screen, click).y - pointFromPin(.screen, prompt).y + 1`, replacing `pos_vp.y -|
prompt_pin.y +| 1`, which mixed a viewport row with a page-node row and broke across pages and when
the prompt scrolled above the viewport. Test `promptClickRelativeRow` (`:6652-6691`, four layouts).

**Rustty.** ABSENT: `crates/rustty-core/src/surface/mouse_input.rs:1892-1903` still computes
`pos_vp.y.saturating_sub(u32::from(prompt_pin.y)).saturating_add(1)`. Port the helper over
`PageList::point_from_pin` (`crates/rustty-vt/src/pagelist/pin.rs:134`) with `.expect` for the `.?`
unwraps; the subtraction cannot underflow because `click_pin.before(prompt_pin)` returned early
(`:1872`). Port the test under its upstream name (it needs `PageList` only, no surface). Lane mac;
Miri-clean.

### 2.8 ssh-terminfo cache key includes the port (`6fbb560cd`, `dfd7bafcd`) — #8

**Upstream.**
- `ssh.zig:441-482` `parseDestination` also reads `port`; emits `user@host` for port 22 or none,
  `user@[host]:port` for IPv6, `user@host:port` otherwise. Four new tests (`:681-727`).
- `DiskCache.zig:413-475`: `isValidCacheKey` validates `isValidHostPort`; new `pub HostPort`,
  `pub splitHostPort` (brackets only for IPv6 with a port; bare multi-colon = IPv6 without port),
  `isValidHostPort` (bracketed iff IPv6, when a port is present), `isValidPort` (digits only, no
  leading zero, fits `u16`). New `test splitHostPort`; `test isValidCacheKey` gains 15 asserts.
- `ssh_cache.zig`: doc comment and usage line (`:37-60`), the three error strings now say
  `(expected [user@]hostname[:port])` (`:212,232,287`), `matchesQuery` (`:358-373`) compares user,
  host and port with a missing port meaning 22, and a bare host matching every port; `test
  matchesQuery` gains 18 asserts.

**Rustty.** ABSENT: `crates/rustty-cli/src/ssh.rs:788-` `parse_destination` (tests `:1308-1350`);
`crates/rustty-cli/src/ssh_cache/disk_cache.rs:531-` `is_valid_cache_key`, `:672` `is_valid_host`,
test `:1781`; `crates/rustty-cli/src/ssh_cache.rs:363,393,490` error strings, `:593` `matches_query`,
test `:799`. Byte-slice APIs throughout (keep `&[u8]`). Port tests: `split_host_port` (new),
extend `is_valid_cache_key_test` and `matches_query_test`, add the four `parse_destination_*` tests
following the existing name mapping. Check the CLI help text (`+ssh-cache --help`) carries the new
doc comment and usage line.

Not a defect: `host` and `host:22` are distinct keys for `--add`/`--remove`/`contains` while queries
treat them as equal. Upstream's own tests accept `[2001:db8::1]:22` as a key and assert
`matchesQuery("user@[::1]:22", "user@::1")`, so this is deliberate; port it as is (Q8).

### 2.9 XTCHECKSUM / DECRQCRA config and termio halves (`9272a2f7c`, `00a19db51`, `01b9f4958`) — #9

**Upstream.**
- `Config.zig:3073-3124`: `vt-xt-checksum-report: bool = false` and `vt-xt-checksum-extension:
  XtChecksumExtension = .{}` placed right after `vt-window-resize-allowed`; `:8901-8908`
  `XtChecksumExtension = packed struct { negate = true, attributes = true, trim = true, undrawn =
  false, full = false }`. Doc comments link xterm sources and say "Ghostty follows the
  implementation".
- `Termio.zig:170-180` `xtChecksumFlags` (inverts `negate`/`attributes`/`trim` into
  `positive`/`no_attributes`/`no_trim`); `DerivedConfig` fields `xt_checksum_report`,
  `xt_checksum_extension` (`:196-197`, set `:235-236`); terminal option `.default_xt_checksum`
  (`:283`); handler field (`:327`).
- `stream_handler.zig`: field `xt_checksum_report` (`:60`), `changeConfig` sets it and calls
  `terminal.setDefaultXtChecksum` (`:127,130`); actions `.xt_checksum_extension` (only when
  reporting is enabled, `:312-314`) and `.request_xt_checksum` → `reportXtChecksum` (`:605-621`,
  returns early when disabled, encodes into a `write_small`).

**Rustty.** ABSENT. Sites: config field list `crates/rustty-config/src/config_struct.rs:3148-3164`
(insert after `vt_window_resize_allowed`), key table `:4325` and `config_struct/key.rs:438`,
`c_get.rs:448` (bool getter; packed getter pattern at `:435-438`), `config_struct/defaults.rs`,
`finalize_clone.rs`, `format.rs`, packed type beside `value_types/packed.rs:45-80` with its
`PackedStruct` parse (the `no-` prefix) and format impls. Termio: `crates/rustty-termio/src/termio_impl.rs:300-380`
(DerivedConfig), `:474` (handler init), `stream_handler.rs:150-160,265-280` (field + changeConfig),
`stream_handler/handler_impl.rs` (actions), `stream_handler/osc_handlers.rs` (`reportXtChecksum`
beside `sendSizeReport` `:891`). Depends on R1/R2's `xt_checksum` module, `Terminal.flags.xt_checksum`,
`setDefaultXtChecksum`, `rectXtChecksum` and the two stream actions; land after them.

Note: upstream inserted `xtChecksumFlags` between `DerivedConfig`'s doc comment and the struct
(`Termio.zig:165-180`), so that doc comment now documents the function. Cosmetic, not a UD; Q9.

### 2.10 Termio no-op / reset arms (`b1bfd1d9c`, `7b11f3dca`, `bae2c3cdb`) — #10, #11

`stream_handler.zig:355-357`: `.mouse_shape_reset` → `setMouseShape(if mouse_event != .none
.default else .text)`. `:390-391`: `.osc_unknown` and `.program_status` join the ignored list.
Rustty: `handler_impl.rs:598-600` (`mouse_shape`), `:704-711` (ignored list), `osc_handlers.rs:86-100`
(`setMouseShape`). ABSENT; trivial once the vt actions exist (R1/R2).

### 2.11 macOS Swift (#12–#14)

- **Window restoration** (`136029788`, `934ef8140`, `0e75d015f`, end state only): new private
  `syncRestoration(_ config:)` in `TerminalController.swift` sets `window.isRestorable = restorable &&
  config.windowSaveState != "never"` and always sets `restorationClass` and `identifier`; called from
  `windowDidLoad` (replacing the inline block) and from the app-level branch of the config-change
  handler. Rustty: `macos/Sources/Features/Terminal/TerminalController.swift:1093-1099` (inline block),
  `:563-578` (handler); `Rustty.Config.windowSaveState` exists (`RusttyKit/Rustty.Config.swift:214`).
  The Swift test file was added then deleted in range: port nothing for it.
- **Clipboard program name** (`e2721d093`): remove `programName` from
  `ClipboardConfirmationController.swift:57`, `ClipboardConfirmationView.swift:38,66`,
  `RusttyKit/Rustty.App.swift:408`, `RusttyKit/Rustty.ClipboardConfirmationRequest.swift:85,104,112`;
  `text()` loses its parameter and always says "An application", with the new doc comment. The C
  struct's `name` field stays (no ABI change). GTK already says "An application"
  (`crates/rustty-apprt/src/gtk/class/clipboard_confirmation_dialog.rs:289-294`). Not previously
  recorded in Rustty.
- **Compiler checks** (`be595d8f0`): drop the `#if compiler(>=6.4)`/`#endif` pairs at
  `App/AppDelegate.swift:447,470`, `TitlebarTabsTahoeTerminalWindow.swift:207,215`,
  `TransparentTitlebarTerminalWindow.swift:107,116,134,139` (with upstream's blank-line edits). This Mac
  has Xcode 26.6 / Swift 6.3.3 / SDK 26; the newly exposed code uses `#available(macOS 27.0, *)` and a
  local `expectsActivation` extension (`Helpers/Extensions/NSAppleEventDescriptor+Extension.swift:32`),
  no SDK-27 symbol, so it should compile; verify with the macOS app build and
  `swiftlint lint --strict`.

## 3. Proposed workstreams

| WS | content | order | effort | lane |
|---|---|---|---|---|
| R3-A | #1 WebGL removal | first, independent | S | mac (+ wasm32 build) |
| R3-B | #2 Device (Metal + OpenGL, App, Options, core, apprt-c errors, test seam) + #3 EGL diagnostics + UD356 test trigger | after R3-A (same `lib.rs` arms) | L (1.5–2 days) | mac then rvm; rwin build/test check; Miri on core App tests |
| R3-C | #4 thread-exit unrealize + #5 UD20 record/test | after R3-B (same lifecycle files) | S | mac + rvm |
| R3-D | #7 prompt click row | any time | S | mac (Miri) |
| R3-E | #8 ssh cache port | any time | M (4–5 h) | mac |
| R3-F | #9–#11 config + termio halves | after R1/R2 vt work for xt_checksum, `mouse_shape_reset`, `osc_unknown`, `program_status` | M | mac |
| R3-G | #12–#14 Swift | any time | S | mac (Xcode build + swiftlint) |
| R3-H | #6 GTK warn-once | any time | S | rvm |

R3-B and R3-C touch `generic/{macos,linux}_impl` and `opengl_impl.rs`; run them in one worktree or
strictly in sequence. R3-B's Linux half cannot be compiled on the Mac: schedule its rvm lane early.

## 4. Open questions (with recommended answers)

**Q1. Device by value in `App` (literal) or out of line?** Out of line. Keep `App.device` as a
`DeviceBox` owning a `Box::into_raw` allocation, create it in `App::init` at the reference's point and
free it in `App::deinit` after the surfaces and the grid-set assert; `Options.device` is `*const
Device`. Reason: by value, every main-thread `&mut App` retag races with render-thread reads (SB/TB),
the same defect class N441/N444 fixed for the renderer and thread blocks. `Arc<Device>` would change
the lifetime contract (surfaces could keep it alive past `App::deinit`); not needed. Record a PORT-ONLY
N for it.

**Q2. How do the 17 core `App::init` tests survive a GPU-requiring `init` (Miri, GPU-less CI)?** Add a
Rustty-only `#[cfg(test)]` constructor that builds the App without initialising a device (a null
`DeviceBox` that `deinit` skips), used by tests that create no surface; keep `App::init` faithful. Do
not `#[cfg_attr(miri, ignore)]` them: that would drop the Miri coverage the comment at
`app.rs:1577-1589` relies on. Add one non-Miri test that `App::init` creates and destroys a real
device (mac; rvm with llvmpipe).

**Q3. Keep Rustty's EGL extension pre-check?** No. Port upstream's order (try the platform display,
diagnose on `BadParameter`) so the log lines and the error match. The pre-check was a Rustty-only
convenience from when `OffscreenContext` was test-only.

**Q4. `OffscreenContext` after the split?** Rename/move its app-scoped half into
`opengl::Device` (libEGL instance, display, config, dmabuf entry points) and keep a small per-renderer
context type. The test helper `test_support::offscreen_gl` builds a `Device` first. No `eglTerminate`
anywhere (DECISIONS B10 unchanged).

**Q5. UD20 after `80456f057`?** FIXED-SAME: mark it fixed upstream, remove the "reference defect"
PORT NOTE, update `docs/ghostty-bug-fixes.md`, add the upstream-named test and keep Rustty's broader
one.

**Q6. UD356 after `40d5b860d`?** Still valid (no `setError(true)` caller at `a60e9e2a5`); keep the fix.
Its measured trigger now fails at App creation, so change the test to a per-surface context failure
and re-measure on rvm. Note in the UD that the no-driver case is now an App-creation failure upstream.

**Q7. Keep any WebGL name for the wasm module?** No. Upstream removed the renderer arm and enum tag;
the wasm module and its WebGL consumer gate are unrelated and stay. Remove every `Backend::Webgl` arm.

**Q8. Treat `host` vs `host:22` cache keys as a defect?** No. Upstream's tests accept `:22` keys and
equate them only in queries; port faithfully.

**Q9. `Termio.zig`'s displaced `DerivedConfig` doc comment?** Carry the comments in upstream order
(G3b). Attach the two lines that now precede `xtChecksumFlags` to that function, as Zig does. Not a
UD: it has no behavioural effect.

**Q10. `logWarnOnce` atomics?** Use a per-site `AtomicBool` with `Relaxed`. Same behaviour as upstream
(maybe a few duplicates), and no data race in Rust.
