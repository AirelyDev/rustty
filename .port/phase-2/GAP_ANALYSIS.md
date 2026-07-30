# rustty — Full Gap Analysis & Completion Roadmap

> **SUPERSEDED-BY (stamped 2026-07-10, T14):** the live planning document is
> `docs/superpowers/plans/2026-07-10-port-realignment-and-validation-hardening.md`
> (+ its "New findings (2026-07-10 execution)" addendum, e.g. Task 23 W-ATTACH),
> and the live seam ledger is `.port/phase-2/DEFERRALS.md` (reconciled to zero
> phantom open seams, 2026-07-10). This document is a POINT-IN-TIME snapshot
> (2026-07-08) that was contradicted by the tree within days on ~6 major claims
> (VALIDATION_REPORT §3b): the runtime-thread workstream was already RESOLVED at
> generation time (D6 closed 2026-07-03), macos-windowing W0–W5 landed 07-09/10,
> the tui-cli paths were wired live, synthetic/bench/extra were ported, the §6
> ledger backlog was reconciled, and the live seam count is now enumerated in
> DEFERRALS.md (69 at reconciliation, not 92). See the corrected workstream
> table at the top of §3. Do NOT drive work from this file.

_Generated 2026-07-08 from the `full-gap-analysis` workflow (14 subsystem inventories + 3 completeness critics + synthesis, 18 agents). Raw per-unit data: `gap-analysis-raw.json`._

> **Goal:** a FULL and COMPLETE port of ghostty (Zig 1.3.2-dev) to Rust, **reusing non-Zig code** (the Swift macOS/iOS app, dcimgui C++, GLSL/Metal shaders, GTK Blueprint XML, vendored C/C++ libs) rather than reimplementing it.

## 1. Executive summary

The headless port is remarkably far along: the entire terminal-VT core, font stack, config/input, os-platform, termio logic, CLI logic, the offscreen Metal renderer, and the full 71-symbol libghostty C-ABI contract are ported with near-1:1 test parity, and the xev kqueue loop already runs a live shell through Exec→pty→xev→terminal offscreen. What is NOT done is everything that turns that into pixels on a screen: the live event-loop wiring (D6), the macOS windowed-present GPU half (R-d Phase-3), and then linking the reused Swift app. Roughly 92 live unimplemented!()/todo!() seams remain, and every one maps to a tracked blocker-class — none is an untracked orphan in reachable core logic. \"Complete\" for a runnable WINDOWED macOS terminal requires exactly three sequential workstreams (runtime-thread → macos-windowing → swift-app reuse); \"complete\" for the whole product additionally requires the GTK/Linux frontend (~23k unported Zig LOC, the single largest chunk), the dcimgui inspector, the vaxis/zf TUI paths, and a decision on the unused-by-ghostty subsets (synthetic/bench/extra generators, WASM). The dominant honest gap is that runtime-live coverage of the C-ABI is ~21% until R-d+D6 land, and the ledger (DEFERRALS.md) has drifted — several real deferrals (synthetic, benchmark, src/extra) have no ledger entry and need reconciling.

**Estimated state:** ~78% done · ~39,000 Zig LOC remaining · ~35 outstanding ported tests.

Global open-seam blocker tally (units not done/reused):

| blocker-class | open units |
|---|---|
| gtk | 30 |
| gui-inspector | 14 |
| none | 11 |
| macos-windowing | 9 |
| unused-by-ghostty | 6 |
| tui-cli | 6 |
| dependency | 5 |
| runtime-thread | 4 |
| macos-swift-app | 3 |
| platform-linux | 1 |
| font-platform | 1 |

## 2. Where we are — per-subsystem status

| subsystem | status | dominant remaining blocker(s) |
|---|---|---|
| app-core | **mostly-done** | macos-windowing×2, unused-by-ghostty×2, macos-swift-app×1 |
| apprt-embedded | **mostly-done** | macos-windowing×1 |
| apprt-gtk | **not-started** | gtk×27, gui-inspector×1 |
| macos-swift-app | **partial** | macos-swift-app×2 |
| renderer | **partial** | macos-windowing×6, gtk×3, dependency×2 |
| os-platform | **complete** | dependency×1 |
| termio | **mostly-done** | runtime-thread×3, dependency×2, unused-by-ghostty×1 |
| font | **mostly-done** | unused-by-ghostty×2, platform-linux×1, font-platform×1 |
| cli | **mostly-done** | tui-cli×5 |
| inspector | **partial** | gui-inspector×13 |
| config-input | **complete** | — |
| terminal-vt | **complete** | — |
| deps-runtime | **mostly-done** | none×4 |
| misc-support | **mostly-done** | none×6, tui-cli×1 |

Fully complete: **terminal-vt**, **config-input**, **os-platform**. Mostly done (headless logic ported, seams gated on windowing/threads): app-core, apprt-embedded, termio, font, cli, deps-runtime, misc-support. Not started: **apprt-gtk** (zero counterpart). Partial: renderer (offscreen done, present pending), inspector (FFI done, widgets pending), macos-swift-app (reuse pending C-ABI glue).

## 3. The completion plan — workstreams (recommended order)

> **CORRECTED WORKSTREAM TABLE (2026-07-10, T14 — supersedes the per-workstream
> prose below; evidence = git log + DEFERRALS.md reconciliation):**
>
> | # | workstream | status as of 2026-07-10 | residual |
> |---|---|---|---|
> | 1 | runtime-thread (D6) | **DONE — was already resolved when this doc was generated** (xev Milestone B, 2026-07-03; live-shell KAT) | none (windows/flatpak arms are workstream 6) |
> | 2 | macos-windowing | **DONE through W5** (W0 540c924, W1 74c5720, W2 accbd4a, W3 8e604eb, W4 73acefa, W5 9f16b9d) | **W-ATTACH** only: core `renderer_seam::Renderer::init` host-attach (plan Task 23) |
> | 3 | macos-swift-app | open (unchanged) | alias/xcframework decisions (§7.1) |
> | 4 | gui-inspector | open (unchanged) | 13 apprt leaves + core activate/deactivate + widgets |
> | 5 | tui-cli | **DONE except `boo`** (vaxis chunks 1a–4b + zf ported; list_colors/list_keybinds prettyPrint e55e39d; list_themes picker 1f88eec) | `boo.rs:22` (needs unported vxfw tier) |
> | 6 | platform-linux-windows | open (unchanged) | fontconfig arms, windows spawn/tty, flatpak |
> | 7 | gtk | open, not started (unchanged) | whole subsystem |
> | 8 | unused-subset | **substantially DONE**: synthetic PORTED (fb12303, 4 binary-entry seams), benchmark PORTED (rustty-bench done, 8/8), src/extra PORTED (rustty-extra done, byte-exact goldens) | wasm target + mdgen/webgen + stb_image_resize decisions |
> | 9 | misc-headless + ledger hygiene | **DONE** (DEFERRALS.md reconciled 2026-07-10 to zero phantom open seams; the §6 backlog below is closed) | — |

### 1. runtime-thread (D6 live xev event loop)  ·  effort **L** · risk **high**

- **Blocker-class:** runtime-thread
- **Goal:** Spin the real io + render event loops so a live Surface exists at all. The xev kqueue backend (rustty-xev Milestone A/B) already works; this workstream fills the verbatim-preserved live bodies that drive it: termio threadEnter/threadMain/processOutput/queueWrite, the renderer Thread event loop (GPU+xev wakeup/draw timers), os/cf_release_thread xev_seam (8 markers), and resolves the Subprocess.start process-storage model. This is the foundational unblock — without it every ghostty_surface_* panics.
- **Depends on:** nothing (can start now)
- **Reuse strategy:** Pure Zig-port; no non-Zig reuse. rustty-xev (kqueue loop, async/timer/process/stream) is already the working substrate — reuse it as-is. The live_shell.rs integration harness already drives the real loop end-to-end and is the regression backstop.
- **First milestone:** termio threadEnter/threadMain + processOutput live over the existing rustty-xev loop, verified by the macOS live_shell.rs integration test spawning /bin/sh and round-tripping output offscreen (no GPU).
- **Key units:**
  - `crates/rustty-termio/src/thread.rs (threadMain event loop, 3 markers)`
  - `crates/rustty-termio/src/exec.rs (threadEnter/threadExit/processExit/queueWrite/ttyWrite xev callbacks + ReadThread.threadMainPosix + Subprocess.start)`
  - `crates/rustty-termio/src/termio_impl.rs (processOutput/processOutputLocked/queueWrite, 14 markers)`
  - `crates/rustty-renderer/src/thread.rs (Phase-3 render event loop)`
  - `crates/rustty-os/src/cf_release_thread.rs (xev_seam, 8 markers)`

### 2. macos-windowing (GPU windowed-present + NSView/Metal host)  ·  effort **XL** · risk **high**

- **Blocker-class:** macos-windowing
- **Goal:** Turn the working offscreen Metal renderer into a live windowed present. Fill the ~20 embedded.rs macOS leaves + 8 apprt-c #[cfg(macos)] leaves + the renderer R5 seams: Metal::{init,loop_enter} binding a real device+CAMetalLayer, Frame::complete async addCompletedHandler present, IOSurfaceLayer windowed present, generic.rs windowed draw path + updateSize, Renderer::{init,deinit,surfaceInit}, Kitty image GPU texture upload, Surface draw/performBindingAction, window blur (objc/CGS), display_id, quicklook.
- **Depends on:** runtime-thread (D6 live xev event loop)
- **Reuse strategy:** REUSE shaders/shaders.metal verbatim (already vendored byte-identical, compiled to .metallib by build.rs). New objc2/coretext/CoreGraphics-private (CGS) bindings must be written in Rust — this is the one place raw objc msgSend + private CGSSetWindowBackgroundBlurRadius is unavoidable. The offscreen Metal stack (device/buffer/texture/pipeline/render-pass) is already ported and golden-pixel-tested — only the live-layer present half is new.
- **First milestone:** Metal::init binds a CAMetalLayer to a test NSView and generic.rs presents one real frame of a terminal state to an on-screen drawable (single-window, no Swift app yet) — the first actual pixels.
- **Key units:**
  - `crates/rustty-renderer/src/generic.rs (6 R5 windowed-present seams)`
  - `crates/rustty-renderer/src/metal/metal_impl.rs (init/loop_enter)`
  - `crates/rustty-renderer/src/metal/frame.rs (async present, 2 seams)`
  - `src/renderer/metal/IOSurfaceLayer.zig (no rustty file yet)`
  - `crates/rustty-renderer/src/image.rs (kitty GPU upload)`
  - `crates/rustty-apprt/src/embedded.rs + crates/rustty-apprt-c/src/lib.rs (macOS objc/CGS/Metal leaves)`
  - `crates/rustty-core/src/surface.rs (draw->renderer_seam, performBindingAction)`

### 3. macos-swift-app (Swift frontend reuse + symbol/identity + xcframework)  ·  effort **L** · risk **med**

- **Blocker-class:** macos-swift-app
- **Goal:** Link the entire reused macOS/iOS Swift AppKit/SwiftUI frontend (~32k LOC, ~150 files) against librustty and ship a running app. No Swift porting — the deliverable is the C-ABI adaptation: a ghostty_*→rustty_* symbol bridge, regenerated umbrella header/modulemap, config C-ABI cbindgen header, an xtask that repackages librustty_apprt_c into a signed .xcframework consumable by Ghostty.xcodeproj, and identity rebranding (bundle id, app-group, keychain, appcast).
- **Depends on:** macos-windowing (GPU windowed-present + NSView/Metal host)
- **Reuse strategy:** MAXIMAL non-Zig reuse: all Ghostty/*.swift binding layer, Surface View, Terminal/Splits/QuickTerminal/Settings/AppleScript/AppIntents/CommandPalette features, Helpers+Extensions, the 11 .xib, and the 2 ObjC pairs (ObjCExceptionCatcher/VibrantLayer) reuse verbatim once symbols resolve. Sparkle stays an SPM dep. Only glue = the C-ABI header/alias/packaging layer. The Swift-facing ghostty_info/ghostty_translate/benchmark_cli stems need rustty exports or aliases.
- **First milestone:** Decide the symbol-identity fork, then get GhosttyKit to import and the Ghostty target to link+launch to a first live terminal window driving rustty_surface_draw — the whole product milestone.
- **Key units:**
  - `ghostty_*→rustty_* alias/bridging shim (~163 hard-coded Swift symbols)`
  - `regenerated GhosttyKit module.modulemap + umbrella header covering rustty_apprt.h + rustty_config_*.h`
  - `xtask: librustty_apprt_c → signed .xcframework (arm64)`
  - `macos/Ghostty.xcodeproj framework-ref swap + Sparkle SPM + scheme/signing rename`
  - `identity rebrand: Info.plist/entitlements (io.airely.rustty), About/AppIcon assets`

### 4. gui-inspector (dcimgui developer overlay)  ·  effort **L** · risk **med**

- **Blocker-class:** gui-inspector
- **Goal:** Port the Dear ImGui developer-tools overlay: the ~4.4k LOC of widget render sections (pagelist/termio/terminal/surface/page/screen/style/renderer widgets, all preserved as verbatim comments), Inspector.zig setup/gui tree, widgets.zig ImGui helpers, units.zig, and the key.rs render section — plus the Metal/OpenGL imgui_impl backends.
- **Depends on:** macos-windowing (GPU windowed-present + NSView/Metal host)
- **Reuse strategy:** REUSE dcimgui: crates-sys/dcimgui-sys already vendors Dear ImGui 1.92.5-docking + Dear Bindings 0.17 C++ and builds it (cc + bindgen, freetype-linked). The FFI leaf is DONE — every widget render section calls into it. Only the platform render backend (imgui_impl_metal) and the Rust widget logic on top are new Zig-port work.
- **First milestone:** activateInspector builds a live dcimgui context and renders the key-events widget table into the Metal layer alongside the terminal.
- **Key units:**
  - `crates/rustty-inspector/src/lib.rs (uninhabited Inspector placeholder → live)`
  - `crates/rustty-inspector/src/widgets.rs + widgets/key.rs render sections`
  - `src/inspector/widgets/{pagelist,termio,terminal,surface,page,screen,style,renderer}.zig (all missing)`
  - `src/inspector/units.zig`
  - `imgui_impl_metal render backend glue`

### 5. tui-cli (vaxis/zf interactive CLI paths)  ·  effort **M** · risk **low**

- **Blocker-class:** tui-cli
- **Goal:** Fill the four interactive terminal-UI entry points whose reference logic is already ported verbatim but whose render path is unimplemented!(): the boo lorem-ipsum animation, list_colors/list_keybinds prettyPrint, and the list_themes interactive preview picker. The plain/non-tty/streaming paths of all four are already live.
- **Depends on:** nothing (can start now)
- **Reuse strategy:** No macOS/GPU dependency — this is a self-contained terminal-UI stack. Requires a Rust equivalent of Zig's vaxis (TUI lib) + zf (fuzzy matcher); either adopt existing Rust crates (ratatui/nucleo) as the render substrate or port vaxis/zf. Embed lorem_ipsum.txt as-is. Independent of the windowing critical path — can run in parallel.
- **First milestone:** list_keybinds --plain path stays live and prettyPrint renders the keybind table interactively via the chosen TUI substrate on a real tty.
- **Key units:**
  - `crates/rustty-cli/src/boo.rs (vaxis animation, + lorem_ipsum.txt asset)`
  - `crates/rustty-cli/src/list_colors.rs (prettyPrint)`
  - `crates/rustty-cli/src/list_keybinds.rs (prettyPrint)`
  - `crates/rustty-cli/src/list_themes.rs (vaxis+zf preview picker)`

### 6. platform-linux-windows (fontconfig / ConPTY / flatpak)  ·  effort **M** · risk **med**

- **Blocker-class:** platform-linux
- **Goal:** Close the non-macOS platform seams so Linux and Windows targets build and run: the Linux Fontconfig discovery backend (discovery toFcPattern + Fontconfig struct + DeferredFace fontconfig arms, 3 markers), the Windows spawn path (Command/pty ConPTY, cfg-gated + unverified), and the flatpak exec arms (currently const-false compiled-out).
- **Depends on:** nothing (can start now)
- **Reuse strategy:** REUSE fontconfig-sys (already vendored) for the toFcPattern path. Windows path uses win32 CreateNamedPipeW/ConPTY via libc/windows crates already shimmed in rustty-os/windows.rs. Mostly Zig-port; no large non-Zig reuse. Partly a prerequisite for the GTK workstream (fontconfig font discovery on Linux).
- **First milestone:** Fontconfig discoverer resolves a face on Linux (unblocks the 3 DeferredFace arms), verified by the existing discovery tests running under a Linux target.
- **Key units:**
  - `crates/rustty-font-shaper/src/discovery.rs (Fontconfig discoverer + toFcPattern)`
  - `crates/rustty-font-shaper/src/deferred_face.rs (3 fontconfig arms)`
  - `crates/rustty-termio/src/command.rs + crates/rustty-os/src/pty.rs (Windows/ConPTY)`
  - `crates/rustty-os/src/flatpak.rs (exec arms behind const-false)`

### 7. gtk (GTK4/libadwaita frontend + OpenGL backend — Linux)  ·  effort **XL** · risk **high**

- **Blocker-class:** gtk
- **Goal:** Port the entire Linux frontend: ~55 .zig files / ~23k LOC under src/apprt/gtk/** (application/surface/window/split_tree widgets, dialogs, overlays, winproto wayland/x11, portals, GDBus IPC, cgroup/systemd/flatpak glue) PLUS the OpenGL renderer backend (opengl/*.zig, ~1.2k LOC) that instantiates the same generic.Renderer with the GL API. This is the single largest unported chunk and the whole subsystem currently has ZERO rustty counterpart.
- **Depends on:** runtime-thread (D6 live xev event loop), platform-linux-windows (fontconfig / ConPTY / flatpak)
- **Reuse strategy:** REUSE the 21 .blp Blueprint XML files + 4 .css themes VERBATIM via blueprint-compiler + GResource + gtk::Builder::from_resource; REUSE the 10 glsl/*.glsl shaders as-is for the GL backend. Requires a gtk-rs/gtk4-rs + libadwaita GObject binding stack that does NOT yet exist in the workspace (no gtk/glib/gio/gdk/adwaita -sys crate) — establishing that stack is itself a large prerequisite. macOS-first strategy places this LAST.
- **First milestone:** A gtk4-rs stack is wired into the workspace and a single AdwApplicationWindow hosts a GLArea rendering one terminal frame via the ported OpenGL backend + a reused .blp — proving the binding stack before the bulk widget port.
- **Key units:**
  - `src/apprt/gtk/class/{application,surface,window,split_tree}.zig (the 4k+ LOC control-plane cores)`
  - `src/apprt/gtk/winproto/{wayland,x11}.zig + portal/OpenURI.zig + ipc/DBus.zig`
  - `src/renderer/OpenGL.zig + src/renderer/opengl/*.zig (GL backend)`
  - `src/renderer/shaders/glsl/*.glsl (10 GL shaders)`
  - `src/apprt/gtk/build/{gresource,blueprint}.zig (asset build pipeline)`

### 8. unused-subset (synthetic / bench / extra generators / subset seams)  ·  effort **M** · risk **low**

- **Blocker-class:** unused-by-ghostty
- **Goal:** Resolve the feasible-headless-but-open and unused-by-ghostty units, most currently UNTRACKED in DEFERRALS.md: the synthetic differential-fuzzing generators (1157 LOC scaffold), the benchmark harness (1441 LOC scaffold), the src/extra shell-completion + editor-syntax generators (941 LOC, no counterpart), ~~the vt-c get/set FFI accessors (todo! stubs)~~ [CORRECTION 2026-07-10, VALIDATION_REPORT M3: STALE — all 34 vt-c modules landed, 308 tests, zero live todo!()], and a decision on the unreachable z2d (AA-fill/dashed-stroke) + wuffs (swizzle) subset seams and stb_image_resize [both now ledgered in DEFERRALS.md, 2026-07-10].
- **Depends on:** nothing (can start now)
- **Reuse strategy:** Mostly Zig-port; headless-feasible with no external blocker. src/extra generators depend on the CLI Action/Config reflection surface (already ported). The z2d/wuffs subset seams are input-variant guards the sole consumer never constructs — candidate to leave as documented unreachable unimplemented!() unless the GTK-OpenGL path later needs them. Benchmark harness feeds phase-0 BASELINE.json regression.
- **First milestone:** Port the synthetic generators (Bytes/Utf8/Osc/Ascii + cli) to feed the differential fuzz harness and add its DEFERRALS entry — clearing the largest untracked gap.
- **Key units:**
  - `crates/rustty-synthetic (9-file generators, 9 test blocks)`
  - `crates/rustty-bench (12-file harness, benchmark_cli C-ABI)`
  - `src/extra/{bash,fish,zsh,vim,sublime}.zig (+gen-* CLI actions)`
  - ~~`crates/rustty-vt-c/src/terminal.rs (todo! get/set accessors)`~~ [CORRECTION 2026-07-10: STALE — landed; zero live todo!() in rustty-vt-c (VALIDATION_REPORT M3)]
  - `z2d painter/stroke subset + wuffs swizzle.rs (unreached seams) + ~~stb_image_resize verification~~` [CORRECTION 2026-07-29: `stb_image_resize verification` is DONE — WS11 landed 5 discriminating tests (2 direct-FFI + 3 KATs on genuinely scaled bitmap strikes, grayscale and BGRA) and gate **D10** (`check_stb_resize_parity.sh`) diffing our vendored C against ghostty's own build recipe at all four zig optimize modes. The port itself had shipped 2026-06-30 at `2e82776`.]

### 9. misc-feasible-headless + ledger hygiene  ·  effort **S** · risk **low**

- **Blocker-class:** none
- **Goal:** Reconcile documentation/ledger drift and finish small headless tails: add DEFERRALS.md entries for every real deferral the inventory found untracked (synthetic, benchmark, src/extra, mdgen/webgen, stb_image_resize, the ghostty_* alias layer, the .xcframework repackaging), correct the app-core over-claims (main_gen/main_wasm/main_build_data marked done but ledger-deferred), and strip stale port-status header comments in rustty-vt terminal.rs/screen.rs that falsely assert skeletons.
- **Depends on:** nothing (can start now)
- **Reuse strategy:** No code reuse; documentation + comment hygiene. Ensures the ledger is authoritative before the heavy workstreams proceed, so completeness audits don't miss units. Pure bookkeeping — can run immediately and in parallel with everything.
- **First milestone:** DEFERRALS.md carries an entry for each of the ~7 inventory-flagged untracked units, and the stale terminal.rs/screen.rs comments are corrected.
- **Key units:**
  - `DEFERRALS.md (add ~7 missing deferral entries; single-source-of-truth reconciliation)`
  - `crates/rustty-vt/src/terminal.rs + screen.rs (stale skeleton comments)`
  - `app-core main_gen/main_wasm/main_build_data status correction`
  - `mdgen/webgen man-page + website doc generators (untracked)`

## 4. Critical path to a runnable windowed macOS terminal

1. runtime-thread (D6 live xev loop)
2. macos-windowing (GPU present + NSView/Metal host)
3. macos-swift-app (Swift reuse + symbol/identity + xcframework)

Everything else (GTK, inspector, TUI-CLI, platform Linux/Windows, unused-subset, ledger hygiene) is **off** the macOS critical path and can proceed in parallel or after.

## 5. Non-Zig reuse opportunities (keep, don't reimplement)

- macOS/iOS Swift AppKit/SwiftUI frontend (~32k LOC, ~150 .swift + 11 .xib + 2 ObjC pairs): links against librustty and reuses VERBATIM once the ghostty_*→rustty_* symbol identity is resolved — zero Swift porting, only a C-ABI header/alias/xcframework glue layer.
- dcimgui C++ (Dear ImGui 1.92.5-docking + Dear Bindings 0.17): already vendored and building in crates-sys/dcimgui-sys (cc C++17 + bindgen, freetype-linked). The FFI leaf is DONE; every deferred inspector widget render section calls straight into it — only imgui_impl_metal/opengl backends remain.
- Metal shader source (shaders.metal, 25KB): copied byte-identical into rustty-renderer and compiled to a .metallib by build.rs — reused as-is, no glue beyond the shader-load path.
- GLSL shaders for the OpenGL backend (10 glsl/*.glsl) + shadertoy prefix/fixtures: language-independent, reused verbatim when the GTK/Linux GL backend lands.
- GTK Blueprint UI (21 .blp across ui/1.0–1.5) + 4 CSS themes: declarative markup reused verbatim via blueprint-compiler + GResource + gtk::Builder::from_resource; glue is the build.rs pipeline + template-callback wiring.
- ObjC helpers (ObjCExceptionCatcher.{h,m}, VibrantLayer.{h,m}) exposed via the Swift bridging header — pure AppKit/ObjC, reused verbatim.
- Vendored C/C++ libraries already wrapped by -sys crates: oniguruma, wuffs, highway, simdutf, freetype, harfbuzz, fontconfig, libpng, zlib, glslang, spirv-cross, gettext, sentry, dcimgui — compiled as-is via build.rs, only bindgen/cc glue written.
- shell-integration scripts (bash/zsh/fish/elvish/nushell) reused verbatim as runtime data assets loaded by path — never re-embedded.
- Sparkle auto-updater stays a Swift SPM dependency (macOS Update feature reuses it directly); only a rustty-branded appcast URL/EdDSA key is needed.
- ghostty's own Highway/simdutf C++ SIMD kernels (base64/codepoint_width/index_of/vt .cpp) copied byte-identical into rustty-simd/cbits and cc-built — reused rather than reimplemented.

## 6. Untracked gaps & ledger drift (hygiene backlog)

The completeness critics found the code has **no untracked orphan seams in reachable core logic** — every live `unimplemented!()` maps to a tracked blocker-class. But `DEFERRALS.md` has drifted: several real deferrals have **no ledger entry**, and a few units are over-claimed as done. Reconcile before heavy work so audits don't go blind:

- **[notable]** `src/helpgen.zig` — 190-LOC standalone +helpgen generator emitting config/CLI/keybind help strings, in NO file list and no missed note; only conceptually tracked via the rustty-helpgen crate.
- **[notable]** `src/terminal/apc/glyph.zig` — 164-LOC Glyph APC protocol module root; inventory lists apc.zig and apc/glyph/{Glossary,execute,request,response}.zig but omits this file.
- **[notable]** `crates/rustty-wuffs/src/swizzle.rs:26,39,51,63,75 (gToRgba/gaToRgba/rgbToRgba/bgrToRgba/bgraToRgba)` — Five pixel-format swizzlers are pure-compute (feasible-headless, blocker_class effectively none) yet left as unimplemented!(). Matches the feasible-but-open criterion, but no rustty caller reaches them: the bg-image decode path converts internally and the Kitty path uses image::convert, so they are fidelity-preserving dead code within the deps-runtime (wuffs) vendored-subset unit, not reachable open work. Flagged because the deferral rationale is scope-trim, not a real external blocker.
- **[notable]** `crates/rustty-z2d subset guards: painter.rs:128 (supersample_4x fill), painter.rs:322 (supersample_4x stroke), internal/tess/stroke_plotter.rs:46 + polyline_plotter.rs:47 (dashed stroking), surface.rs:355,392 (sub-8-bit packed-alpha surfaces)` — z2d 2D-raster subset seams are pure-compute (feasible-headless, blocker_class none) but left unimplemented!(). Each is guarded by an AntiAliasMode/Pixel/dash-array variant the sole consumer (font sprite/glyf canvas) never constructs — it uses Default/Multisample4x AA, Alpha8/Rgba surfaces, no dashes. Covered by the deps-runtime (z2d) vendored-subset unit; unreached, so latent rather than parity-blocking.
- **[notable]** `src/synthetic/** (rustty-synthetic) — inventory units deps-runtime AND misc-support` — Ledger is completely silent: grep -i synthetic over DEFERRALS.md returns nothing. The inventory carries src/synthetic (1157 LOC, 9 files) as an explicit gap in TWO subsystems ('only a 9-line scaffold', 'untracked gap'). A real, inventory-recognized deferral has no ledger entry, so the ledger is not current for it.
- **[notable]** `app-core entry points main_gen.zig / main_wasm.zig / main_build_data.zig` — Direct disagreement. DEFERRALS C6 (line 2864) explicitly DEFERS all three (0t each, no MAP entry, target = future gen/wasm tooling milestone), but the inventory app-core unit lists them under files with missed:[] (asserts fully ported). Inventory says done, ledger says open.
- **[notable]** `Benchmark harness — src/benchmark/**, main_bench.zig, ghostty_benchmark_cli` — Ledger has NO benchmark deferral at all (grep bench / main_bench return nothing). Inventory is split: misc-support and app-core list benchmark files with missed:[] while macos-swift-app flags benchmark_cli/benchmark harness unported. The ledger tracks neither the port nor the deferral of this unit.
- **[notable]** `apprt-gtk subsystem (~55 .zig, ~23k LOC, .blp/.css)` — No dedicated apprt-gtk deferral entry exists. GTK appears only tangentially (Command pre_exec/post_fork growth points line 757, the main_ghostty GUI-launch leaf line 2839, comptime app_runtime==.gtk guards). Inventory treats apprt-gtk as a full first-class subsystem with zero rustty counterpart; the ledger never enumerates it as a unit, so the bulk is effectively untracked.
- **[notable]** `macos Swift seams — ghostty_* alias shim + signed .xcframework repackaging` — Ledger is silent on both: no entry for a ghostty_*->rustty_* alias/bridging layer (Swift app hard-codes ~163 ghostty_ symbols), and no xtask/CI step repackaging librustty_apprt_c into a signed .xcframework for Ghostty.xcodeproj (E11 line 2521 only links the cdylib in a smoke test). Inventory flags both untracked. Note: two adjacent inventory claims are false alarms in the ledger's favor — ghostty_info/ghostty_translate ARE referenced (line 2860) and the config C-ABI header IS tracked (E11 item 3, line 2533).

## 7. Open decisions — need the user (genuine forks)

1. SYMBOL IDENTITY (the biggest fork, gates Swift reuse): export ghostty_* aliases from rustty-apprt-c so the ~163 hard-coded Swift symbols link with ZERO Swift edits, vs keep the rustty_* ABI and ship a #define/bridging-header + regenerated umbrella-header shim. Also decides whether ghostty_info/ghostty_translate/benchmark_cli get rustty_* stems or ghostty_* aliases. This choice ripples through the modulemap, xcframework, and every macos-swift-app unit.
2. macOS-first vs GTK-first: the established strategy (EMBEDDED_BLUEPRINT) is macOS-first with GTK last, but GTK is the single largest unported chunk (~23k LOC + a from-scratch gtk4-rs binding stack). Confirm GTK stays deferred to after a shipping macOS app, and whether Linux gets a windowed terminal in this port at all or is explicitly out of near-term scope.
3. UNUSED-BY-GHOSTTY subsets — port or skip: (a) synthetic differential-fuzzing generators (1157 LOC, untracked), (b) benchmark harness (1441 LOC) + benchmark_cli export, (c) src/extra shell-completion + editor-syntax generators (941 LOC, untracked), (d) WASM target entirely (main_wasm, web_canvas face+shaper, WebGL, os/wasm) — currently out-of-scope; confirm it stays out, (e) stb_image_resize (glyph/atlas resize) — verify whether rustty-font already covers it or a stb-sys cc build is needed.
4. Unreachable subset seams: the z2d (anti-aliased/supersample fill, dashed-stroke tessellation) and wuffs (pixel swizzle) unimplemented!() seams are pure-compute but guarded by input variants the sole consumer (font sprite/glyf) never constructs. Leave as documented-unreachable, or fill them for completeness and to de-risk the future GTK-OpenGL renderer that may exercise them?
5. iOS target: rustty-apprt-c currently gates the 6 Darwin exports on target_os=macos, not iOS. Widen to isDarwin (macOS OR iOS) to enable the reused iOSApp.swift, or keep macOS-only and treat iOS as out of scope?
6. Ledger authority / process: adopt DEFERRALS.md as the single source of truth by adding the ~7 missing entries (synthetic, benchmark, src/extra, mdgen/webgen, stb_image_resize, ghostty_* alias layer, .xcframework repackaging) and correcting the app-core over-claims — or accept the inventory as the authoritative gap list going forward? The two currently disagree.
7. Renderer/io thread ownership model (decided WITH D6): Subprocess.start in the reference stores dangling raw-arena pointers (forbidden strongly-protected UB in Rust). Confirm the pid-only/Box-owned process-storage substitution as the faithful-but-safe divergence before the runtime-thread workstream commits to it.
