# Full macOS App Functionality Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Large parallelizable phases run as `.port/phase-2/wf-*.js` Workflow scripts per project doctrine; their task entries specify the script's contract.

**Goal:** Make the rustty macOS terminal app **fully functional, feature-for-feature with ghostty's macOS app** — every stubbed/deferred/unshipped item working like the source — carrying **rustty branding** (visible name "Rustty", rustty bundle id), while the C-ABI symbols stay `ghostty_*` (Branch A) so the reused Swift frontend links unchanged.

**Architecture:** Five completion fronts over the already-rendering app (W-ATTACH landed): (1) finish the render/present/resize path and prove it end-to-end; (2) ship all app resources (shell-integration, themes, man, locale, bat, terminfo source) as rustty data; (3) port the deferred features (quicklook, the full ImGui inspector, the Darwin exports); (4) make the shipped lib self-contained (full zlib + gettext) and enable i18n; (5) harden packaging (explicit archive set, ranlib, universal slice, code signing, notarization) and complete the identity rebrand.

**Tech Stack:** Rust workspace (rustty-{apprt-c,apprt,core,renderer,inspector,font-shaper,xev} + crates-sys/{zlib-sys,gettext-sys}), objc2/objc2-quartz-core/objc2-metal, dcimgui-sys + ImGui_ImplMetal, cbindgen, `xtask` (xcframework + resources), `xcodebuild`/`libtool`/`ranlib`/`lipo`/`codesign`/`notarytool`, the ghostty oracle at `/Users/gael/projects/ghostty` @ `4749c4e9`.

## Gap sources (read before executing)

- `.port/phase-2/MACOS_APP_GAPS.md` — critical-path audit: B1–B5 render chain, §3 HIGH/MEDIUM/LOW feature ledger.
- `.port/phase-2/INTEGRATION_PARITY.md` — parity review: §2.1–2.9 confirmed fixes (resources, self-contained lib, iOS host_view, i18n, terminfo source).
- `zig-out/share/ghostty/DEFERRALS.md` + per-resource DEFERRALS — what each resource stub is missing.
- The Verify-KAT blocker: `crates/rustty-apprt-c/tests/macos_window_host.rs::hosts_view_layer_and_presents_live_frame` fails assertion 2 ("no IOSurface presented") in the headless harness though the real app renders — the present-half discrepancy P0 must resolve.
- Oracle trees to vendor: `ghostty/src/shell-integration/{bash,zsh,fish,elvish,nushell}` (8 files + subtrees), the ghostty themes corpus, `ghostty/src/terminfo`, `ghostty/src/os/i18n` + `.mo` locale, man pages.

## Global Constraints

- **R1** fidelity: every ported body matches ghostty behavior line-for-line; vendored data (shell-integration, themes, terminfo) is byte-identical to ghostty's, carrying the `xterm-ghostty`/`ghostty` **data identity** the reused Swift app expects (the *symbols* are ghostty_* per Branch A; the *product* is rustty).
- **R2** identity: the app is **"Rustty"** everywhere user-visible (menu bar, Dock, window title, About, bundle name/id `io.airely.rustty`); the `GhosttyKit` module name and `ghostty_*` C-ABI symbols are KEPT (Branch A link contract). Resource *content* keeps ghostty's internal names where the protocol requires (`TERM=xterm-ghostty`, `share/ghostty/`, OSC identifiers) — document each such kept-name.
- **R3** self-contained: no build/runtime dependency on `/Users/gael/projects/ghostty`; vendor its data into rustty's tree. Same C libs (full zlib, full gettext), no pure-Rust substitution.
- **R5** never weaken a gate/test; a real failure (e.g. the present KAT) is fixed or precisely blocked, never hidden by re-`#[ignore]`.
- **panic=abort** shipped profile: recoverable failures return sentinels; asserts abort like Zig traps; no unwind across `extern "C"`.
- Commit after every green task; never batch phases. All new source carries ported Zig bodies as comments (R1).

## Current state (already done — do NOT redo)

- W-ATTACH render attach (`96b0116`) — the app renders a live prompt/cursor.
- P2 crash-guards on quicklook/inspector leaves → graceful sentinels (`a290323`) — INTERIM, superseded by P2 full ports below.
- xtask deployment-target pin + partial self-contained work (`252edc2` area).
- Resource generators (terminfo compiled, shell/editor/misc scaffolded — but shell-integration + themes are EMPTY stubs).
- Identity rebrand — in progress (verify + complete in P5).

---

## Phase 0 — Finish & prove the render/present/resize path

### Task 0.1: Resolve the present-half KAT (IOSurface not delivered in the harness)

**Files:**
- Investigate: `crates/rustty-renderer/src/metal/metal_impl.rs` (`present` :786 Some-arm, `set_display_callback` :505, `draw_frame`/`present_last_target`), `crates/rustty-apprt-c/tests/macos_window_host.rs:329-372` (`hosts_view_layer_and_presents_live_frame`, `HostedSurface::pump_bounded`, `layer_has_contents`).
- Modify: whichever of the above the root cause identifies.

**Interfaces:**
- Consumes: the attached `layer: Some(IOSurfaceLayer)` from W-ATTACH.
- Produces: `layer_has_contents()` true after `pump_bounded()` — an IOSurface is set on the hosted layer.

- [ ] **Step 1: REQUIRED SUB-SKILL `superpowers:systematic-debugging`.** The real app renders but the headless KAT does not — establish WHY. Instrument: does the render thread call `present()` → `layer.set_surface(target.surface)` in the harness? Add a temporary `log::debug!` at `present` Some-arm + at the display-callback install, run `cargo test -p rustty-apprt-c --test macos_window_host hosts_view_layer_and_presents_live_frame -- --nocapture`, and confirm whether present is reached, whether `set_surface` runs (it hops to the main thread via `dispatch2::DispatchQueue::main().exec_async` — the headless harness may never drain the main queue), and whether `set_display_callback`/CVDisplayLink drives frames without a real NSWindow runloop.
- [ ] **Step 2:** Root cause (likely: `set_surface` defers to the main dispatch queue which the headless harness never pumps, OR the display callback needs the CVDisplayLink/`loop_enter` the harness stubs). Compare with ghostty's `IOSurfaceLayer.setSurface` (main-thread hop) + `Metal.zig` present. Fix at root: either make `pump_bounded` drain the main dispatch queue (harness-side, faithful — the real AppKit runloop does this), or if a real product-code ordering bug (present never reached because `loop_enter` didn't install against `Some(layer)`), fix the ordering. Do NOT weaken the assertion.
- [ ] **Step 3:** `cargo test -p rustty-apprt-c --test macos_window_host` → all non-on-device tests PASS (the NSWindow on-device test stays legitimately ignored). Commit `"apprt-c: present-half — deliver IOSurface to the hosted layer; windowed-present KAT green (P0.1)"`.

### Task 0.2: Live resize + content-scale + present-error propagation

**Files:**
- Verify/modify: `crates/rustty-renderer/src/metal/metal_impl.rs` (`surface_size` :720 Some-arm reads `layer.bounds * contentsScale`), `crates/rustty-apprt-c/src/lib.rs` (`rustty_surface_set_size`, `rustty_surface_set_content_scale`).
- Test: extend `crates/rustty-apprt-c/tests/macos_window_host.rs` with a resize KAT.

- [ ] **Step 1:** Write a KAT that attaches a host view, sets an initial size, resizes the mock host view's bounds, pumps, and asserts the presented frame tracks the new bounds (the `surface_size` Some-arm reads `layer.bounds`; AppKit manages a layer-hosting view's frame — confirm it auto-tracks, else wire the view-resize → layer-frame update per `MACOS_APP_GAPS.md` B4).
- [ ] **Step 2:** Run it — FAIL if resize doesn't track. Fix per B4 (update the hosted layer frame on `set_size`). Verify `set_content_scale` updates `contentsScale` (Retina sharpness) — compare `rustty_surface_set_content_scale` against ghostty `embedded.zig` `updateContentScale`.
- [ ] **Step 3:** Confirm `present()`'s `ObjCFailed` error propagates (no longer the `None => Ok(())` no-op). Commit `"renderer: verify live resize/content-scale tracking + present-error propagation (P0.2)"`.

---

## Phase 1 — Ship all app resources as rustty data (functional like ghostty)

### Task 1.1: Vendor + emit shell-integration (bash/zsh/fish/elvish/nushell)

**Files:**
- Create: `crates/rustty-extra/shell-integration/` (vendored copy of `ghostty/src/shell-integration/{bash,zsh,fish,elvish,nushell}` + README, byte-identical) OR a dedicated `assets/` dir — follow how rustty already vendors C sources.
- Modify: `xtask/src/res_terminfo.rs` `gen_ghostty_root` (or a new `xtask/src/res_shell_integration.rs`) to copy the vendored tree into `zig-out/share/ghostty/shell-integration/` (excluding `.md`).
- Modify: `zig-out/share/ghostty/DEFERRALS.md` → mark RESOLVED.

**Interfaces:**
- Produces: `share/ghostty/shell-integration/{bash,zsh,fish,elvish,nushell}/…` populated — OSC7 cwd, prompt marking, cursor-shape restore, sudo wrap, title updates work (INTEGRATION_PARITY §2.1).

- [ ] **Step 1:** Vendor `ghostty/src/shell-integration/` verbatim into rustty's tree (R3 self-contained; the scripts carry ghostty's `xterm-ghostty`/OSC identifiers the terminal emits — a KEPT data identity, document in a header comment: "vendored ghostty shell-integration; protocol identifiers kept, product is rustty").
- [ ] **Step 2:** Wire the copy in the `resources` xtask; `cargo run -p xtask -- resources` then `ls -R zig-out/share/ghostty/shell-integration` shows the real scripts (non-empty).
- [ ] **Step 3:** Rebuild the app (`bash macos/build-rustty.sh`), launch, and verify shell integration works: in the rustty terminal run `cd /tmp` and confirm the title/cwd updates (OSC7), and the prompt marks appear. Screenshot. Commit `"resources: ship ghostty shell-integration (bash/zsh/fish/elvish/nushell) — R3 vendored (P1.1)"`.

### Task 1.2: Vendor + emit the themes corpus

**Files:**
- Create: `crates/rustty-extra/themes/` (vendored ghostty theme corpus — the iTerm2-Color-Schemes-derived `.conf` theme files ghostty ships).
- Modify: the `resources` xtask to copy them into `zig-out/share/ghostty/themes/`.

**Interfaces:**
- Produces: `share/ghostty/themes/*` populated — the `list_themes` fuzzy picker (already ported in rustty-cli) and the `theme` config setting resolve to the full set (INTEGRATION_PARITY §2.2).

- [ ] **Step 1:** Locate ghostty's shipped themes (its build copies the iTerm2 corpus to `share/ghostty/themes`); vendor them into rustty. Verify the count matches ghostty's.
- [ ] **Step 2:** Emit them in the xtask; `cargo run -p xtask -- resources` → `ls zig-out/share/ghostty/themes | wc -l` equals ghostty's count.
- [ ] **Step 3:** Rebuild + launch; run the rustty `list-themes` picker in-app (or set `theme = <name>` in `~/.config/rustty/config`) and confirm the theme applies (colors change). Commit `"resources: ship the ghostty themes corpus — list_themes + theme setting functional (P1.2)"`.

### Task 1.3: man pages, bat syntax, terminfo source/termcap

**Files:**
- Modify: `xtask/src/res_misc.rs` (`gen_misc`), `xtask/src/res_terminfo.rs`.

- [ ] **Step 1: man pages** — ghostty ships generated man pages (`share/man/man{1,5}/ghostty.*`). rustty-helpgen produces help; wire man-page generation (or vendor the ghostty man sources + `mandoc`) into `share/man/`. If rustty's man generation isn't ported, port it faithfully from ghostty's `build` man step. Verify `man -M zig-out/share/man ghostty` renders.
- [ ] **Step 2: bat syntax** — ghostty ships a `bat` syntax/theme under `share/bat/`. Vendor/generate it (rustty-extra sublime generator may cover the syntax). Verify non-empty.
- [ ] **Step 3: terminfo source + termcap** (INTEGRATION_PARITY §2.8) — emit `ghostty.terminfo` (source) + termcap alongside the compiled DB so Debug builds and `infocmp` match ghostty. Update `res_terminfo.rs`. Commit `"resources: man pages + bat syntax + terminfo source/termcap — resource parity (P1.3)"`.

---

## Phase 2 — Port the deferred features (make them work like ghostty)

### Task 2.1: quicklook_word (objc-free — do first)

**Files:**
- Modify: `crates/rustty-apprt-c/src/lib.rs:2705` (`rustty_surface_quicklook_word`, currently the P2 crash-guard sentinel).

**Interfaces:**
- Consumes: the ported `renderer_state.mutex` + `selectWord` + `readTextLocked` (all landed).
- Produces: the real quicklook word body — Swift's Look-Up word gesture (`SurfaceView_AppKit.swift:1512`) returns the word text instead of falling back.

- [ ] **Step 1:** Read ghostty `embedded.zig` `ghostty_surface_quicklook_word` (objc-free: lock renderer_state, selectWord at the cursor, readTextLocked). Port it faithfully, replacing the P2 sentinel. It returns a `Text` (ownership-transfer; host frees via `ghostty_surface_free_text`, invariant I6).
- [ ] **Step 2:** `cargo build -p rustty-apprt-c` green; a KAT drives quicklook_word over a surface with a word under the cursor and asserts the returned Text. Commit `"apprt-c: port quicklook_word (objc-free) — Look-Up word gesture functional (P2.1)"`.

### Task 2.2: quicklook_font (CoreText / objc)

**Files:**
- Modify: `crates/rustty-apprt-c/src/lib.rs:2634` (`rustty_surface_quicklook_font`).

- [ ] **Step 1:** Read ghostty `embedded.zig` `ghostty_surface_quicklook_font` (:2179 — `copyWithAttributes` returns a `+1` CTFontRef the host releases, invariant I16). Port faithfully using objc2/core-foundation `CFRetained` with matching ownership. Main-thread-affine (I5), `!Send`.
- [ ] **Step 2:** Replace the P2 sentinel; `cargo build` green; a KAT (or manual: Look-Up gesture in-app) confirms a CTFont is returned + released once. Commit `"apprt-c: port quicklook_font (CTFont +1) — accessibility/Look-Up font query functional (P2.2)"`.

### Task 2.3: The full ImGui inspector

**Files:**
- Modify: `crates/rustty-core/src/surface.rs:2857,2891` (`activateInspector`/`deactivateInspector`), `crates/rustty-apprt/src/embedded.rs:1833+` (Inspector struct + `ghostty_inspector_*`), `crates/rustty-apprt-c/src/lib.rs` (`inspector_metal_init/render/shutdown`, :2396/2399/2425), `crates/rustty-inspector/src/*` (the 13 widget seams), `crates/rustty-renderer/src/generic.rs:4600` (inspector `overlayFeatures`).

**Interfaces:**
- Consumes: dcimgui-sys (FFI leaf, done) + a ported `ImGui_ImplMetal` backend.
- Produces: cmd-opt-i opens a working terminal inspector rendered via ImGui+Metal.

- [ ] **Step 1:** This is the Phase-3 inspector tier — sizable. Port `ImGui_ImplMetal` init/render/shutdown (`inspector_metal_*` :2396/2399/2425) against dcimgui-sys, main-thread-affine (I5). Then `activateInspector`/`deactivateInspector` (surface.rs:2857/2891 — the verbatim Zig bodies are preserved as comments; construct the `inspectorpkg::Inspector`).
- [ ] **Step 2:** Port the 13 widget render seams in `rustty-inspector` (the Dear ImGui widget sections) from ghostty `src/inspector/*` — cross-reference the pre-port artifact `.port/phase-2/PREPORT_INSPECTOR.md` (22 LIFETIMES rows, 13 seams split 5 mechanical / 8 design; hazards R1 VTHandler self-alias needs isolated-Miri proof, R2 renderer acyclicity via accessor trait).
- [ ] **Step 3:** Fill `generic.rs:4600` inspector `overlayFeatures` (currently uninhabited-enum-dead; becomes live once Inspector exists). `cargo build --workspace` green. Rebuild app; cmd-opt-i opens a working inspector. Screenshot. Commit per sub-piece (metal backend / activate / each widget group) — this task is itself a mini-plan; execute via a dedicated workflow. `"inspector: full ImGui inspector — metal backend + activate + widgets (P2.3)"`.

### Task 2.4: The remaining Darwin exports + on-feature seams

**Files:**
- Modify: `crates/rustty-apprt-c/src/lib.rs` (`set_display_id` :done; audit the rest), `crates/rustty-apprt/src/embedded.rs` on-feature callback seams (the enumerated `embedded.rs:1262,1282,1659,1781,1873,1900,1944,2000,2075,2100,2120,2136,2153,2189,2213`), `crates/rustty-xev/src/thread_pool.rs:77,85,92,100` + `xev/src/lib.rs:344`.

- [ ] **Step 1:** Triage each of the ~40 remaining `unimplemented!()`/`todo!()` in the macOS-path crates against `MACOS_APP_GAPS.md`'s reachability classification: for each, determine ALWAYS/ON-FEATURE/NEVER. Produce `.port/phase-2/MACOS_SEAM_TRIAGE.md` listing every one with its verdict + the ghostty reference body.
- [ ] **Step 2:** Port every ON-FEATURE seam the Swift app can trigger (splits, tabs, clipboard confirm, notifications, bell, mouse-shape, the `embedded.rs` action callbacks) faithfully from ghostty. For NEVER-reached (correctly-dead, e.g. iOS arms, uninhabited enums) — leave with a documented DEFERRALS entry (R5). The xev `thread_pool` (:77-100) is the dead `thread_pool always None` branch (LIFETIMES-audit M1/M2) — port the ThreadPool body OR keep deferred with the un-defer gate; decide per whether any macOS path reaches it.
- [ ] **Step 3:** `cargo build --workspace` + a full macOS app exercise (open splits/tabs, copy/paste, trigger a bell/notification) with zero abort. Commit per feature group. `"apprt: port the on-feature macOS callback seams — splits/tabs/clipboard/notifications/bell functional (P2.4)"`.

---

## Phase 3 — Self-contained lib + i18n

### Task 3.1: Full deflate-capable zlib + full gettext (drop system/Homebrew links)

**Files:**
- Modify: `crates-sys/zlib-sys/build.rs` (build FULL zlib incl. `deflate.o`), `crates-sys/gettext-sys/build.rs` (build full libintl incl. `dgettext`/`bindtextdomain`/`textdomain`, not just the localename slice), `macos/build-rustty.sh` (drop `-lz`/`-lintl` once self-contained).

**Interfaces:**
- Produces: the combined `libGhosttyKit.a` defines `deflate`/`dgettext`/… → the app links WITHOUT system `-lz` or Homebrew `-lintl` (INTEGRATION_PARITY §2.3).

- [ ] **Step 1:** zlib-sys currently builds inflate-only (`nm libz.a` has no `deflate.o`). Reconfigure its `build.rs` to compile the full zlib (add the deflate sources); `nm` the rebuilt `libz.a` shows `_deflate`.
- [ ] **Step 2:** gettext-sys currently vendors only the gnulib localename slice. Extend it to provide the `dgettext`/`bindtextdomain`/`textdomain` runtime (vendor the needed gettext-runtime sources, R3 — same C lib). `nm` shows the symbols.
- [ ] **Step 3:** `cargo run -p xtask -- xcframework`; `nm` the combined lib for `_deflate` + `_dgettext` (defined, T not U). Remove `-lz -L/opt/homebrew/opt/gettext/lib -lintl` from `build-rustty.sh` (keep only `-lstdc++`). Rebuild → `** BUILD SUCCEEDED **` with no undefined symbols and no Homebrew dep. Commit `"sys: full deflate-zlib + full gettext — self-contained xcframework (drop -lz/-lintl) (P3.1)"`.

### Task 3.2: Enable i18n (dgettext + locale catalog)

**Files:**
- Modify: `crates/rustty-apprt-c` or wherever `rustty_translate` lives (the identity-return stub), the i18n binding (rustty-os/global), `xtask/src/res_misc.rs` (`share/locale` currently a `.placeholder` stub).

- [ ] **Step 1:** With full gettext linked (3.1), un-stub `rustty_translate` → actually call `dgettext(domain, msgid)` (ghostty `internal_os.i18n._`), and `bindtextdomain` to the app's `share/locale`. Port ghostty's i18n init.
- [ ] **Step 2:** Vendor/generate the `.mo` locale catalogs ghostty ships into `share/locale/<lang>/LC_MESSAGES/`. Verify a non-English `LANG` translates a known string. Commit `"i18n: enable gettext translation + ship locale catalogs (P3.2)"`.

---

## Phase 4 — Packaging robustness

### Task 4.1: Explicit archive dependency set (not glob) + ranlib normalization

**Files:**
- Modify: `xtask/src/main.rs` `xcframework()`.

- [ ] **Step 1:** Replace the filesystem-glob archive discovery with an explicit set derived from `cargo build -p rustty-apprt-c --message-format=json` (parse the `compiler-artifact`/`build-script-executed` records for the `.a` link deps) — INTEGRATION_PARITY §2.4. Avoids missed-nested / stale-duplicate archives.
- [ ] **Step 2:** Before `libtool`, copy each archive to a temp path and run `/usr/bin/ranlib` on it (§2.5) so newer-Xcode libtool doesn't silently drop 64-bit members. `cargo run -p xtask -- xcframework` → combined lib intact (`nm` symbol counts stable). Commit `"xtask: explicit archive set (cargo json) + ranlib normalization (P4.1)"`.

### Task 4.2: Universal binary (add x86_64 slice)

**Files:**
- Modify: `xtask/src/main.rs` `xcframework()`, `macos/build-rustty.sh`.

- [ ] **Step 1:** Add the `x86_64-apple-darwin` target build + `lipo -create` the per-arch archives into a universal one, and stage the slice as `macos-arm64_x86_64` (matching ghostty's `GhosttyKit.xcframework/macos-arm64_x86_64`). Requires `rustup target add x86_64-apple-darwin`.
- [ ] **Step 2:** `plutil -p dist/GhosttyKit.xcframework/Info.plist` shows both architectures; `lipo -info` the lib shows arm64 + x86_64. Commit `"xtask: universal (arm64+x86_64) xcframework slice — parity with ghostty (P4.2)"`.

### Task 4.3: Code signing + entitlements + notarization path

**Files:**
- Modify: `macos/build-rustty.sh` (a signed-build variant), document in `macos/MILESTONE.md`.

- [ ] **Step 1:** The current build disables signing (`CODE_SIGNING_ALLOWED=NO`). Add a signed variant: with a Developer ID (or ad-hoc `-` for local), sign the app + framework, using the rustty entitlements (P5 rebrand supplies the rustty app-group ids). Verify `codesign --verify --deep --strict` passes.
- [ ] **Step 2:** Document the notarization path (`notarytool submit` + `stapler`) for distribution (do NOT run it without credentials — document the exact commands + the required Developer ID). Commit `"macos: signed build variant + documented notarization path (P4.3)"`.

---

## Phase 5 — Complete the identity rebrand ("Rustty")

### Task 5.1: Verify + complete the visible rename

**Files:**
- Verify/modify: `macos/Ghostty-Info.plist`, `macos/*.entitlements`, `macos/Ghostty.xcodeproj/project.pbxproj` (PRODUCT_NAME), `macos/Sources/**` (user-facing display strings only), `macos/build-rustty.sh`, `macos/REBRAND.md`.

- [ ] **Step 1:** The p2p3 rebrand agent's work landed — VERIFY it's complete: build the app and confirm the **menu bar, Dock, window title, and About panel all read "Rustty"** (screenshot). Fill any gap the agent missed (grep `macos/Sources` for user-facing `"Ghostty"` display strings — About window, app-name constants, notification titles — and rename the DISPLAY ones; leave `GhosttyKit`, `ghostty_*`, URL schemes, and bundle-path references alone).
- [ ] **Step 2:** Confirm `CFBundleIdentifier = io.airely.rustty` matches the config dir already in use (`~/Library/Application Support/io.airely.rustty`) and the rustty-identity crate. `plutil -lint` passes. Rebuild → the app is "Rustty" everywhere visible while still linking (GhosttyKit/ghostty_* kept). Commit `"macos: complete R2 rebrand — 'Rustty' everywhere user-visible, verified by screenshot (P5.1)"`.

### Task 5.2: Sparkle auto-updater config

**Files:**
- Modify: `macos/Ghostty-Info.plist` (SUFeedURL / SUEnableAutomaticChecks).

- [ ] **Step 1:** The app bundles Sparkle. Either point `SUFeedURL` at a rustty appcast (if distribution is planned) or disable auto-checks (`SUEnableAutomaticChecks = false`) for now so it doesn't phone ghostty's feed. Document the choice. Commit `"macos: Sparkle updater — rustty appcast or disabled (no ghostty feed) (P5.2)"`.

---

## Phase 6 — Polish & correctness (small gaps, none omitted)

### Task 6.1: iOS host_view arm + non-darwin cfg (INTEGRATION_PARITY §2.6, §2.9)

**Files:** Modify: `crates/rustty-apprt/src/embedded.rs` (`host_view` impl).

- [ ] **Step 1:** `host_view()` currently returns null for the iOS arm (dropped R1 code path vs `Metal.init`'s uiview handling). Add `Platform::IOS(i) => i.uiview` (mirroring ghostty). If the `Platform::MacOS`/`IOS` field access doesn't compile off-darwin (§2.9), `#[cfg(target_os=…)]`-gate the arms.
- [ ] **Step 2:** `cargo build -p rustty-apprt` + a cross-check `cargo check --target x86_64-apple-darwin -p rustty-apprt` (or the linux target) green. Commit `"apprt: host_view iOS uiview arm + non-darwin cfg (R1 parity) (P6.1)"`.

### Task 6.2: Update the stale renderer doc block (MACOS_APP_GAPS §3 LOW)

**Files:** Modify: `crates/rustty-renderer/src/generic.rs:11-38`.

- [ ] **Step 1:** The doc block claims `DerivedConfig::init`/`setScreenSize`/`changeConfig`/`setFontGrid` are unimplemented seams — all four are landed & real. Narrow the DEFERRED note to reflect reality (W-ATTACH was the last gap and is now fixed). Commit `"renderer: correct stale doc block — the four listed seams are landed (P6.2)"`.

### Task 6.3: Final full-app functional shakedown

- [ ] **Step 1:** Rebuild the fully-featured app (`bash macos/build-rustty.sh`), launch, and exercise EVERY feature end-to-end with screenshots: render + type + scroll + select + copy/paste; resize (tracks); a theme applied; shell-integration cwd/title; a split + a tab; Look-Up word + font; open the inspector (cmd-opt-i); confirm "Rustty" branding throughout. Zero aborts.
- [ ] **Step 2:** Update `macos/MILESTONE.md` to "fully functional" with the screenshot evidence, and reconcile every DEFERRALS entry this plan resolved. Commit `"macos: fully-functional rustty terminal — feature parity with ghostty, rustty branding (P6.3)"`.

---

## Sequencing & parallelism

```
P0 (render/present) → gates everything visual; do FIRST (0.1 blocks the KAT, 0.2 resize).
P1 (resources)   ∥  P4 (packaging)  ∥  P5 (rebrand)  — independent, parallel workflows.
P3.1 (self-contained lib) before P3.2 (i18n needs full gettext).
P2 (features): 2.1/2.2 (quicklook) parallel + small; 2.3 (inspector) is its own sub-workflow (largest); 2.4 (seam triage+ports) parallel.
P6 (polish) any time; 6.3 (shakedown) LAST, after all else.
```
Parallelizable as workflows: P1 resources (4 agents), P2.4 seam ports (per-feature), P4 packaging (3 agents), P5 rebrand. P0 and P2.3 (inspector) are focused/serial. P3.1 is delicate -sys work (verify `nm` at each step).

## Self-review notes
- Every gap source item mapped: MACOS_APP_GAPS §3 HIGH quicklook→P2.1/2.2, inspector→P2.3, on-feature seams→P2.4; MEDIUM/LOW render→P0, stale doc→P6.2, off-platform→P2.4 triage. INTEGRATION_PARITY §2.1 shell-integration→P1.1, §2.2 themes→P1.2, §2.3 self-contained→P3.1, §2.4 archive-glob→P4.1, §2.5 ranlib→P4.1, §2.6 iOS host_view→P6.1, §2.7 i18n→P3.2, §2.8 terminfo source→P1.3, §2.9 non-darwin cfg→P6.1. Verify-KAT blocker→P0.1. Resources man/bat→P1.3. Universal/signing/notarize→P4.2/4.3. Sparkle→P5.2. Rebrand→P5.1. Every one of the 50 macOS-path `unimplemented!()` is triaged in P2.4 Step 1.
- No placeholders: each task names exact files:line, the ghostty reference to port from, and a concrete verification (nm/cargo test/screenshot/plutil).
- "Functional like the source, rustty branding": R1/R3 keep behavior + data identity faithful (protocol names like `xterm-ghostty`/`share/ghostty`/OSC kept and documented); R2 makes only the user-visible product "Rustty".
