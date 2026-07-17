# Linux / GTK Port Plan — rustty

> Branch: `linux-port/gtk-apprt` · worktree `/home/gael/projects/rustty-gtk`
> Oracle: `/home/gael/projects/ghostty` @ `4749c4e9` (read-only).
> Governs the port of everything needed for a **runnable Linux (GTK4/libadwaita) terminal**,
> and every tier currently blocked on Linux. Grounded in `.port/phase-2/PREPORT_GTK.md`
> (the ratified R11 GTK pre-port gate), `NEXT_PHASE_PLAN.md`, `GAP_ANALYSIS.md`,
> `XEV_BLUEPRINT.md`, and a 4-agent code + reference audit (2026-07-16).

## 0. TL;DR — "Linux" is a stack, not just the GTK apprt

The headless `rustty` binary already **runs and spawns a live shell — but only on macOS**,
because three foundational tiers below the GUI are macOS-only today, plus the GUI tier itself
is unported. rustty-os is the one big surprise: **already Linux-complete**. The work sequences
into 7 tiers (L0 tiny → L5 huge):

| Tier | Name | State | Reference | Effort |
|---|---|---|---|---|
| **L0** | Compile-gating | 2-edit manifest fix | — | XS |
| **L1** | `rustty-xev` epoll loop | not started | libxev `backend/epoll.zig` (2061 loc/12t) | L |
| **L2** | rustty-os Linux | **✅ already done** | — | — |
| **L3** | Linux font discovery | skip-stubbed | fontconfig discoverer + `fontconfig-sys`/`libxml2-sys` | M |
| **L4** | renderer OpenGL backend | not started | `OpenGL.zig` 461 + `opengl/*` 1090 loc + 10 GLSL | L |
| **L5** | GTK apprt (native backend) | not started | `src/apprt/gtk/**` 22k loc + 21 `.blp` + 4 `.css` | XL |
| **L6** | winproto / inspector / layer-shell | not started | `winproto/{wayland,x11}`, imgui GL, gtk4-layer-shell | L |

Critical path to *pixels on screen*: **L0 → L1 → L4 → L5** (L3 parallel; L6 after L5's noop core).

## 1. Current state (what's already true on this machine)

- **System toolchain: all present.** gtk4 4.22.4, libadwaita 1.9.2, gtk4-layer-shell 1.3.0,
  blueprint-compiler 0.20.4, glib-compile-resources, glib-compile-schemas, msgfmt, pkg-config,
  fontconfig 2.17, libxml2 2.12.10, wayland-client 1.25, x11 1.8.13, GCC 16.1.1.
- **-sys layer: all 14 crates build on Linux** (this branch's parent commit `85d1902` fixed
  highway/oniguruma/glslang; `clang-devel` installed for bindgen).
- **rustty-os: Linux-complete** (audit): pty, passwd/shell, xdg, env, cgroup, systemd, dbus,
  desktop, open, resourcesdir, locale/i18n all have real gated Linux paths. No porting needed.
- **apprt seam exists**: `rustty-apprt-api` 3-trait seam (`App`/`Surface`/`CoreSurfaceHandle`),
  generic (not `dyn`). `embedded` is the only concrete backend. `Runtime` enum already has
  `None`/`Gtk`; `Runtime::default_for(Linux) == Gtk`. GUI launch is one `unimplemented!()` at
  `crates/rustty/src/main_rustty.rs:292`.

## 2. Architecture decisions (recommended, baked into the plan)

- **AD-1 — GTK is a NATIVE Rust apprt backend, not an app over the C-ABI.** ghostty's macOS is a
  reused Swift app over `libghostty`; its GTK is native Zig. rustty mirrors exactly: a new
  `gtk` module in `crates/rustty-apprt` (sibling of `embedded`/`none`) that `impl`s
  `apprt_api::App`/`Surface` against gtk4-rs GObject widgets. It does **not** touch
  `rustty-apprt-c`. `type Core = rustty_core::Surface<gtk::App>`.
- **AD-2 — Binding stack: gtk4-rs from crates.io** (`glib`, `gtk4`, `libadwaita`, `gdk4`,
  `gio`, `gdk4-wayland`/`gdk4-x11` later), built against system pkg-config gtk4. No new `-sys`
  crate needed for GTK itself. `gtk4-layer-shell` via the `gtk4-layer-shell-sys`/`gtk-layer-shell`
  crate (or a small `-sys`) for the quick-terminal (L6).
- **AD-3 — Pin ONE modern baseline: gtk ≥ 4.14 / adw ≥ 1.5** (system has 4.22 / 1.9). This lets
  us DELETE every version-conditional comptime branch in the reference (`adw_version.supportsX()`,
  comptime `Parent` type selection, comptime-conditional field types like
  `remember_choice: if(can_remember) …`). Record the baseline in the crate. (PREPORT_GTK §3.)
- **AD-4 — `.blp` + `.css` reuse VERBATIM** via a `build.rs` pipeline (blueprint-compiler →
  glib-compile-resources → gresource, `gio::Resource::register`). No UI re-authoring. (PREPORT_GTK §5.3.)
- **AD-5 — Event loop: epoll + io_uring + live `Dynamic` selector** (DECIDED D-A, 2026-07-16).
  Port BOTH libxev Linux backends (`backend/epoll.zig` 2061 loc + `backend/io_uring.zig` 1787 loc)
  AND the real `dynamic.zig` (525 loc) runtime selector — ghostty's actual Linux default. This is
  a genuine architectural step beyond macOS: on macOS `dynamic.zig` was comptime-elided (single
  kqueue candidate); on Linux there are TWO candidates so `Dynamic`/`DynamicCompletion` become
  LIVE, and downstream (`termio`/`core`) must consume the loop through the dynamic dispatch layer
  rather than a flat concrete type. Selector: io_uring preferred, epoll fallback (probe at init).
- **AD-6 — winproto: Wayland inside L5, X11 in L6** (DECIDED D-B, 2026-07-16). L5 ports the
  `wayland` winproto backend (wl_display/Globals, KDE blur/slide, xdg-activation, CSD, gtk4-layer-
  shell quick-terminal) alongside the GObject widgets. `x11` + `noop` remain cargo-feature arms;
  X11 lands in L6. The union's ordered-init fallback still yields `.none` when a protocol isn't
  compiled/active.
- **AD-7 — Hand-rolled main loop, NOT `ApplicationExt::run`.** Reproduce ghostty's
  `acquire → register → activate → while(running){ MainContext::iteration(blocking); core_app.tick(); check-quit }`
  with the `glib::MainContext::wakeup(null)` + inline-tick wakeup bridge. (agent-D §1.)

## 3. Milestone roadmap

### L0 — Compile the Linux workspace  ·  effort XS · risk none · deps: none
- **Goal:** `cargo build --workspace` (and `cargo test`) green on Linux.
- **Work:** `crates/rustty-apprt-c/Cargo.toml` — move `objc2` (`:101`) into
  `[target.'cfg(target_os = "macos")'.dependencies]` (a gated objc2 block already exists at
  `:130`); move the 5 dev-deps (`:142–146`, objc2-quartz-core/foundation/core-graphics/io-surface/
  app-kit) into `[target.'cfg(target_os = "macos")'.dev-dependencies]`.
- **Verify:** `cargo build --workspace` + `cargo test --workspace --no-run` compile on aarch64
  Linux; macOS build unaffected (deps still present under the cfg).
- **Gate:** workspace compiles; no regression to the -sys or headless crates.

### L1 — `rustty-xev` Linux event loop: epoll + io_uring + `Dynamic`  ·  effort L/XL · risk high · deps: L0
- **Goal:** termio can spawn and pump a live shell on Linux (the same `live_shell.rs` KAT that
  passes on macOS/kqueue, now on Linux). This is the foundational unblock — without it every
  `Surface::init` (pty + io thread + renderer thread) is inert on Linux.
- **Scope (DECIDED D-A):** port BOTH Linux backends + the runtime selector:
  `backend/epoll.zig` (2061 loc, 12t), `backend/io_uring.zig` (1787 loc, 13t), `dynamic.zig`
  (525 loc, 5t) + `linux/timerfd.zig` and any `linux/*` helpers the two backends force. Reference =
  pinned `libxev-34fa50878aec6e5fa8f532867001ab3c36fae23e`
  (`deps.files.ghostty.org/libxev-34fa5087….tar.gz`).
- **The architectural step vs macOS:** on macOS `dynamic.zig` was comptime-elided (single kqueue
  candidate, flat `Api()`); on Linux `candidates() = [io_uring, epoll]`, so `Dynamic`/
  `DynamicCompletion`/`DynamicResult` become LIVE. termio/core `use rustty_xev::*` must resolve to
  the dynamic dispatch layer on Linux while staying the flat kqueue type on macOS — the crate's
  `lib.rs` re-export surface picks per-target. The shared `Loop`/`Completion`/`Operation`/`Result`/
  `Async`/`Timer`/`Stream`/`Process` abstractions from the kqueue lane are reused; epoll/io_uring
  are sibling backends behind them.
- **Backend shapes:** epoll = `epoll_create1` + `eventfd` (Async) + `timerfd` (Timer) +
  `pidfd_open`/`waitpid`-on-SIGCHLD (Process); io_uring = `io_uring_setup` + SQE/CQE rings
  (submission-based; Async via `IORING_OP_*`/eventfd, Timer via `IORING_OP_TIMEOUT`). Gate modules
  `#[cfg(target_os = "linux")]`, mirroring the kqueue `#[cfg(target_os="macos")]` gating at
  `crates/rustty-xev/src/lib.rs:60–94`. `Backend::default()` on Linux probes io_uring→epoll.
- **First milestone:** async + timer + process + stream round-trip KATs on epoll, then the same on
  io_uring, then `Dynamic` selecting between them (mirrors the 4 kqueue live KATs per backend).
- **Verify:** `cargo test -p rustty-xev` on Linux (12 epoll + 13 io_uring + 5 dynamic `#[test]` +
  live KATs per backend); `rustty-termio` `live_shell.rs` (widen from macОS-only to `cfg(unix)`)
  spawns `/bin/sh` and round-trips output into the `Screen` under the Dynamic loop.
- **Risk:** raw-ptr/Stacked-Borrows discipline (kqueue-lane precedent); epoll edge-vs-level /
  EPOLLONESHOT; io_uring SQ/CQ ownership + kernel-shared-memory lifetime + feature probing on older
  kernels; process-exit reaping race; the dynamic-dispatch retype rippling into termio/core `use`
  sites. Lean on live KATs (syscalls Miri-excluded); io_uring ring memory is an isolated-Miri/ASan
  candidate.
- **Gate:** xev tests (both backends + dynamic) + termio live-shell green on Linux; macOS kqueue
  path byte-unaffected (per-target re-export); acyclic/counts green.

### L2 — rustty-os Linux  ·  ✅ ALREADY DONE
- No work. Two documented non-blocking notes: `i18n::canonicalizeLocale` references a libintl
  symbol only reachable on the macOS path (flag if any Linux caller appears); flatpak host-command
  is behind the off-by-default `flatpak` feature (intentional parity).

### L3 — Linux font discovery (fontconfig)  ·  effort M · risk med · deps: L0 (parallel with L1)
- **Goal:** a font resolves on Linux via Fontconfig, replacing the `unimplemented!()` DeferredFace
  leaves (`crates/rustty-font-shaper/src/deferred_face.rs:479,482,489`) and the discovery skip-stubs.
- **Reference:** ghostty `src/font/discovery.zig` (Fontconfig `toFcPattern` + `Fontconfig` struct)
  + the `DeferredFace` fontconfig arms. Backend already cfg-selects
  `Linux default = FontconfigFreetype` (`face/mod.rs`, `discovery.rs`).
- **Work:** vendor **`fontconfig-sys`** + **`libxml2-sys`** (the deferred new-dep pair — follow the
  validated -sys template; fontconfig links freetype-sys + libxml2-sys; system libs present),
  then implement the Fontconfig discoverer + `toFcPattern` + the 3 DeferredFace fontconfig arms
  over freetype-sys faces (already built).
- **Verify:** the existing `rustty-font-shaper` discovery tests run under Linux
  (`BACKEND == FontconfigFreetype`), resolving a real system face.
- **Gate:** font-shaper tests green on Linux; the 3 leaves gone; acyclic (fontconfig-sys →
  freetype-sys + libxml2-sys).

### L4 — renderer OpenGL backend  ·  effort L · risk high · deps: L0 (needed by L5)
- **Goal:** the monomorphic `Renderer` over a new concrete `OpenGL` backend renders a terminal
  state through a GL context (the GLArea path L5 will host).
- **Reference:** `src/renderer/OpenGL.zig` (461 loc) + `src/renderer/opengl/*.zig` (1090 loc:
  buffer/texture/program/pipeline/framebuffer/…) + the **10 GLSL shaders**
  (`shaders/glsl/*.glsl`) reused VERBATIM. The GPU-agnostic `generic.rs` cell-fill / uniforms /
  draw-frame machinery is already ported (Metal lane); OpenGL is a second backend behind the
  same generic seam (mirrors how Metal plugs in at `renderer/src/lib.rs:91`).
- **Binding:** an OpenGL loader — `glow` or `gl` crate (GLArea provides the context; we load via
  `epoxy`/`gl` against the current context). Decide in the L4 detailed plan.
- **First milestone:** offscreen — `OpenGL::init` + `generic.rs` GL draw path renders one frame of
  a terminal `Screen` to an FBO, pixel-compared against the existing golden (mirrors the Metal
  offscreen KAT). Windowed present comes with L5's GLArea.
- **Verify:** an offscreen GL render KAT (headless GL context, e.g. EGL surfaceless) draws a known
  terminal state.
- **Risk:** GL context/threading (`must_draw_from_app_thread`), GLSL uniform/attrib layout parity
  with the shared vertex structs, offscreen-GL availability in CI.
- **Gate:** OpenGL offscreen KAT green; Metal lane unaffected; `renderer` compiles cfg-clean on Linux.

### L5 — GTK apprt (the native Rust backend)  ·  effort XL · risk high · deps: L1, L4, (L3 for fonts)
The main body. Governed by `PREPORT_GTK.md` (LIFETIMES rows, 12 paired invariants, comptime
inventory, constants). Sub-milestones:

- **L5.0 — Crate/build scaffold.** New `gtk` module in `rustty-apprt` (or a `rustty-apprt-gtk`
  crate — decide in L5 plan) + gtk4-rs deps (AD-2) + the **build.rs asset pipeline** (AD-4):
  per-`.blp` `blueprint-compiler compile` (adw ≥ blp `major.minor`, blueprint ≥ 0.16); generate
  the gresource XML (4 css + icons {16..1024}@1x/2x + the 21 ui, prefix `/com/mitchellh/ghostty`
  → rebrand to rustty id); `glib-compile-resources --c-name … --generate-source/--generate-header`
  with cwd = crate asset root; compile+link the generated `.c`; `gio::Resource::register` once at
  `Application` class init; `msgfmt` `po/*.po` → `.mo` + `bindtextdomain`. Copy the 21 `.blp` +
  4 `.css` + icons verbatim. (agent-D §4.)
- **L5.1 — App lifecycle + hand-rolled loop (AD-7).** `Application` (adw.Application subclass via
  gtk4-rs `glib::object_subclass`): startup/activate/dispose/finalize vfuncs, single-instance from
  config, `Application::new(rt_app, core_app)`, the manual `run()` loop with the
  `MainContext::wakeup` + inline `core_app.tick(rt_app)` bridge, quit timer, SIGUSR2 source,
  action map, style manager. Wire the GUI launch leaf at `crates/rustty/src/main_rustty.rs:292`
  for `Runtime::Gtk` → `App::create` + `gtk::App::init` + `run()`.
- **L5.2 — Surface widget (the heart).** `Surface` (adw.Bin + gtk.Scrollable subclass), GLArea
  template child, **lazy `core_surface` on first resize** (not realize), the render contract
  (renderer-thread → `.redraw_surface` mailbox → `performAction(.render)` → `gl_area.queue_render()`
  → `gl_render` on main thread → `renderer.draw_frame(true)`), and the input pipeline (key →
  `im_context.filter_keypress` first → `keyCallback`; IME preedit/commit; mouse gestures; focus on
  idle; resize → content-scale/size callbacks). Honor the load-bearing invariants: self-referential
  Surface pin (heap-box, never move), floating-ref sink, WeakRef finalize races, template
  double-free `set_child(null)` before dispose, glib source-id RAII, `deleteSurface` before deinit.
  (PREPORT_GTK §2, agent-D §2.)
- **L5.3 — `performAction` switch + the GObject class family.** The ~60 handled `Action.Key` arms
  (`application.zig:668-799`) and the remaining GObject subclasses via gtk4-rs
  (`#[glib::object_subclass]` + `CompositeTemplate` + `glib::Properties`): Window, Tab, SplitTree,
  CommandPalette, dialogs/overlays (~22 classes, PREPORT_GTK §1). Plus the `getGObjectType`
  boxed-type registrations deferred out of apprt-api (`Clipboard`/`ClipboardRequest`/`ChildExited`/
  `InitialSize`).
- **First milestone (proves the stack):** a single `AdwApplicationWindow` hosts a GLArea that
  renders **one live terminal frame** via L4's OpenGL backend from a reused `.blp`, over
  `winproto::noop` — end-to-end L1+L4+L5 on Linux. Then fill widgets to functional parity.
- **Verify:** the app launches, shows a window, runs a shell, renders, takes input; `/run` skill
  drives it. Weak unit-test gate (per ghostty) → PREPORT_GTK LIFETIMES + G3b comment-coverage +
  per-chunk review are the gate, plus a launch/type/resize/close smoke.
- **Risk (highest in the plan):** gtk4-rs subclassing-idiom volume (22 classes), GObject lifetime
  hazards (floating refs, WeakRef races, source-id leaks — PREPORT_GTK §5.2), the render-thread
  contract.

### L6 — winproto + inspector + layer-shell (polish to full parity)  ·  effort L · risk med · deps: L5
- **winproto Wayland/X11** (feature-gated): `wayland` (wl_display/Globals, KDE blur/slide,
  xdg-activation, CSD) + `x11` (XKB, `_MOTIF_WM_HINTS`, `_KDE_NET_WM_BLUR_BEHIND_REGION`) via
  `wayland-client`/`wayland-protocols` + `x11rb`/`gdk4-x11`. Replaces the noop union arms.
- **gtk4-layer-shell quick-terminal** (Wayland) via the layer-shell crate.
- **Inspector**: dcimgui developer overlay on the GL backend (`imgui_widget.zig` → `ImGui_ImplOpenGL3`
  against the widget's own GLArea; dcimgui-sys already built). This is the GAP_ANALYSIS
  gui-inspector workstream, GL variant.
- **Portals / IPC**: `portal/OpenURI` (xdg-desktop-portal), GDBus IPC (`ipc/`), cgroup/systemd
  single-instance glue (rustty-os pieces already exist).

## 4. Decision points (need confirmation; recommendations given)

- **D-A · Event-loop backend — DECIDED (2026-07-16): epoll + io_uring + live `Dynamic`.** See AD-5.
- **D-B · winproto scope — DECIDED (2026-07-16): Wayland inside L5; X11 in L6.** See AD-6.
- **D-C · Crate placement.** *Recommend:* GTK backend as a `gtk` **module inside `rustty-apprt`**
  (sibling of `embedded`, matches ghostty's single-apprt-crate shape and Agent-C's acyclic
  analysis). *Alt:* a dedicated `rustty-apprt-gtk` crate (cleaner feature isolation; more wiring).
- **D-D · GL loader for L4.** `glow` vs `gl` vs raw `epoxy`-via-`-sys`. Decide in the L4 detail plan.
- **D-E · Identity/branding.** The gresource prefix, application-id, `.mo` domain are
  `com.mitchellh.ghostty` in the reference. Rebrand to the rustty identity (R2) — confirm the exact
  ids to embed (mirrors the config/apprt-c identity choices already made).

## 5. Critical path & suggested execution order

1. **L0** (unblocks compiling) — immediate, safe, tiny.
2. **L1 epoll** (unblocks live shell) + **L3 fontconfig** in parallel (independent).
3. **L4 OpenGL** (offscreen) — parallel with L1/L3 once L0 lands.
4. **L5** GTK backend — needs L1 (loop) + L4 (render) + L3 (fonts) for a *fully* live window;
   L5.0/L5.1 scaffolding can begin against noop before L4 finishes.
5. **L6** — after L5's noop core is functional.

Each tier gets its own detailed TDD implementation plan (writing-plans) before its first line of
code — L0 is small enough to execute directly; L1/L4/L5 each warrant a dedicated plan doc.

## 6. Risks (largest first)
1. **L5 GTK volume + GObject lifetimes** — 22k loc, 22 subclasses, floating-ref/WeakRef/source-id
   hazards; weak test gate (mitigated by PREPORT_GTK LIFETIMES rows + G3b + review).
2. **L4 OpenGL parity + offscreen-GL in CI** — shader/uniform layout fidelity; headless GL context.
3. **L1 epoll UB discipline** — raw-ptr/SB safety, process-exit reaping (mitigated by live KATs).
4. **L3 new -sys pair** (fontconfig + libxml2) — build/link wiring (validated template exists).
5. **Identity/ABI drift** on the rebrand (mitigated by the existing R2 machinery).
