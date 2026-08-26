# Rustty Completion Program — Roadmap (no-deferrals pass)

> **For agentic workers:** This is the PROGRAM roadmap. Each workstream (WS) has —
> or will have — its own detailed implementation plan in `docs/superpowers/plans/`
> (linked below as they are authored). Execute workstreams with
> superpowers:subagent-driven-development or superpowers:executing-plans, one WS
> plan at a time, in wave order. Do NOT execute from this file alone.

**Goal:** Close every remaining gap in the rustty port that is addressable from
this environment (macOS M4 Pro + the Fedora aarch64 VM `rvm` over SSH + Chrome
browser automation) — no new deferrals.

**Environment:** macOS builds/runs the Metal app (Xcode 26.5); the VM builds/runs
the GTK app (Wayland, XWayland available); Chrome + automation tools validate the
wasm/WebGL target. Every workstream ends with BOTH platform builds green
(`cargo build --workspace` on macOS AND `ssh rvm 'cd ~/projects/rustty && cargo
build --workspace'`) — this is the standing lesson from the two cross-platform
breakages (xev `Loop::init`, `COMPLETION_SIZE`).

## Global constraints (inherited by every WS plan)

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e`); the
  oracle is reference-only — no build-time dependency on it (R3).
- R2 product identity: rustty naming with per-token casing preservation
  (Ghostty→Rustty, ghostty→rustty); ~~`ghostty_*` C-ABI aliases stay~~ —
  **CORRECTED [WS19 T2, 2026-07-29, audit rows W7 / §E.3]: `ghostty_*` C-ABI aliases
  were DELETED UPSTREAM at `e4cc852`; rustty exports `rustty_*` only, in every
  profile** (`crates/rustty-apprt-c/Cargo.toml:31-33`; a repo-wide search for
  `export_name` under `crates/rustty-apprt-c/src/` returns 0 hits). `identity-oracle`
  flips only the `rustty-identity` STRING identity, for the differential harness.
  **Do not re-add aliases.**
- R4/R5: acyclic crate graph; gates (`check_acyclic.sh`, A4 ABI gate, Miri lanes)
  are invariant — never weakened to pass.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI.
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Ledger discipline: when a WS lands, strike its entries in
  `.port/phase-2/DEFERRALS.md` / `VERIFICATION_DEBT.md` in the same PR — the
  ledgers are stale in places (see "Ledger corrections" below) and this pass
  must leave them truthful.
- Commit style: follow existing `subsystem: summary (chunk-ref)` convention;
  branch per WS; merge to main only with both platforms green.

## Ledger corrections already established (do not re-litigate)

- Linux `Timer::cancel` "core won't build" (D:3366) — RESOLVED by the xev batch;
  Linux builds green.
- `ghostty_*` alias layer + signed xcframework xtask — DONE (app links & runs).
- GTK inspector — DONE (2026-07-19 OpenGL3 inspector).
- VD:V1 (rustty-xev never compiled on macOS) — DISCHARGED 2026-07-23.
- **The WS12 row was STALE in two of its three clauses** (recorded 2026-07-29 so the next
  planner does not re-derive it — the same pattern already found in WS16's and WS9's rows).
  Its "uucode fork — decision REQUIRED" clause described a decision that had been made AND
  SHIPPED 19 days earlier at `e0abb03`; its "verify `boo` end-to-end" clause implied `boo`
  might be unported when it had been live since `e7e5a8c`. Only the zigimg-image-tier clause
  was accurate. **Lesson: a ROADMAP scope cell is a snapshot, and a shipped-but-unratified
  decision reliably rots into a contradictory ledger row** — DEFERRALS.md was simultaneously
  presenting the uucode fork as an open human decision AND (240 lines apart) calling `boo`
  both "STILL DEFERRED" and "✅ RESOLVED". Both are now banner-resolved in place with the
  original analysis retained as marked history.
- `stb_image_resize` glyph resize (D:3741 / audit row O17) — **SHIPPED 2026-06-30**
  at `2e82776` in `crates/rustty-font-shaper`; the row's grep was scoped to the wrong
  crate (`crates/rustty-font/src`, pre-D1-split). The seam is `cfg(not(target_arch =
"wasm32"))`, i.e. LIVE ON macOS, not “non-macOS”. WS11 closed the real residual
  (vacuous scale-1.0 coverage) on 2026-07-29 and ledgered the `ReleaseFast` ±1 LSB as
  a permanent divergence. Do NOT change the stb `build.rs` flags.
- Shell-integration/themes: real content exists in `zig-out/share/rustty/` and
  the current app bundle, but is NOT reproducible — xtask still emits the
  deferral placeholder (`xtask/src/res_terminfo.rs:216-245`). WS1 makes it
  vendored + generated.

---

## Wave 0 — quick wins (each ≤ a day)

| WS                              | Plan file                                    | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ------------------------------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **WS1 resources**               | `2026-07-23-ws1-shell-integration-themes.md` | Vendor ghostty `src/shell-integration/` (bash/zsh/fish/elvish/nushell) + the iTerm2 themes corpus (oracle tarball `ghostty-themes-release-20260608`) as rustty data; make `xtask resources` emit `share/rustty/{shell-integration,themes}` (+ ghostty parity profile dir) with R2 renaming; delete the placeholder DEFERRALS.md emission; KAT: byte-compare against oracle output; verify OSC7/prompt-marking live in Rustty.app and `list_themes` shows 534 themes.            |
| **WS2 macOS guards & plumbing** | `2026-07-23-ws2-macos-guards-plumbing.md`    | Verify current state of `quicklook_word`/`quicklook_font` (ledger conflict: E10 says filled, MACOS_APP_GAPS says open) and close whatever remains with real impls or Swift-tolerated null returns (no abort paths reachable from Look-Up/VoiceOver); cross-DPI `contentsScale` re-stamp on monitor move (Metal `set_content_scale` mailbox); `drainMailbox` autoreleasepool shim; run the ignored `macos_window_host.rs` present/resize KATs (B2–B4) and un-ignore what passes. |
| **WS3 small parity fills**      | `2026-07-23-ws3-small-parity-fills.md`       | `sys.rs decode_png` default-hook wiring to `rustty-wuffs::png::decode` (kitty-graphics PNG parity); `+version` STUB lines (RENDERER/LIBXEV/TOOLCHAIN) → real values; `host_view()` iOS mapping + non-darwin compile check (INTEGRATION_PARITY §2.6/§2.9); deployment-target mismatch warning fix.                                                                                                                                                                               |

## Wave 1 — bootstrap & ABI surface

| WS                               | Plan file                         | Scope                                                                                                                                                                                                                                                                                                                                                |
| -------------------------------- | --------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **WS4 app bootstrap (BREAK #1)** | `2026-07-23-ws4-app-bootstrap.md` | Port the full `global.zig` `state.init()`: allocator selection, `detectArgs` CLI-action detection, `GHOSTTY_LOG` logging config, `crash.init`, `ensureLocale`/`i18n.init`, SIGPIPE setup; then un-stub `rustty_cli_try_action`'s non-null path (run action + exit semantics through the C ABI). Verified via embedded harness + Rustty.app relaunch. |
| **WS5 lib-vt C-API (BL-5)**      | `2026-07-23-ws5-libvt-capi.md`    | Upstream BL-5 series: OSC-52 `clipboard_set`, color utility APIs, color-scheme report, OSC/Kitty color queries, unicode-width APIs, scroll-to-row, RGB-by-pointer ABI fix. Extends `rustty-vt-c` + headers + A4 gate freeze.                                                                                                                         |

## Wave 2 — features & robustness

| WS                                    | Plan file                                  | Scope                                                                                                                                                                                                                                                                                                                                                                                                |
| ------------------------------------- | ------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **WS6 macOS inspector**               | `2026-07-23-ws6-macos-inspector.md`        | dcimgui `ImGui_ImplMetal` FFI (mirror the GTK OpenGL3 precedent in `dcimgui-sys`); port the ~4.4k-LOC inspector widget render sections; fill `Surface.activate/deactivateInspector` + `embedded::Inspector::{init,initMetal,renderMetal}` + `rustty_inspector_metal_*` exports; remove the no-op crash sentinel; cmd-opt-i opens a live inspector in Rustty.app.                                     |
| **WS7 scrollback compression (BL-1)** | `2026-07-23-ws7-scrollback-compression.md` | The ~35-commit upstream stack: LZ4 codec crate-sys, compressed `Page` representation, generation stamps, search-integrity integration. Largest single WS; Miri + parity KATs against oracle transcripts; perf gate: no regression on the 14-benchmark suite.                                                                                                                                         |
| **WS8 upstream small-fix sweep**      | `2026-07-23-ws8-upstream-small-sweep.md`   | BL-3 pending-small queue (~30 runtime-safety fixes), BL-2 free-list page return-to-OS, BL-6 kitty/gfx generation stamps, BL-7 OSC 72 drag-and-drop parser, tabstops-on-resize PARTIAL completion. Sequenced per MAIN_PARITY triage order.                                                                                                                                                            |
| **WS9 packaging**                     | `2026-07-23-ws9-packaging.md`              | Self-contained xcframework: vendor full-zlib deflate already done — extend gettext-sys to full libintl (~23 TUs incl. Bison `plural.c`) killing the Homebrew `-lintl` link; archive set from `cargo build --message-format=json` instead of the glob; ranlib normalization before libtool; `MACOSX_DEPLOYMENT_TARGET` alignment. Gate: `build-rustty.sh` succeeds on a PATH without `/opt/homebrew`. |

## Wave 3 — perf, niche & wasm

| WS                           | Plan file                              | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ---------------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **WS10 perf series (BL-4)**  | `2026-07-23-ws10-perf-series.md`       | SIMD scans, bulk CSI/print, pty pipelining. Guarded by the benchmark suite (goal: hold or extend the 14/14 wins).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| **WS11 font glyph resize**   | `2026-07-23-ws11-font-glyph-resize.md` | ~~`stb_image_resize` parity for the FreeType glyph-resize path (stb-sys cc build for bit-exactness), wired on the non-macOS path, KAT vs oracle rasterization.~~ **CORRECTED [WS19 T14, 2026-07-29 — audit row O17]: the PORT ALREADY SHIPPED at `2e82776` (2026-06-30)** — `crates/rustty-font-shaper/src/stb.rs:18`, `build.rs` `cc::Build` over `vendor/stb_impl.c`, `vendor/stb_image_resize.h` byte-identical to the oracle (md5 `7381a7070120fd5a36d8fb9e3179dd7c`), call site `face/freetype.rs:806`. Two further corrections: (1) the gating is `cfg(not(target_arch = "wasm32"))`, **not** `not(target_os = "macos")` — the seam is LIVE ON macOS (`nm -g` shows `T _stbir_resize_uint8` in the macOS archive), so "wired on the non-macOS path" is wrong; (2) **do NOT chase bit-exactness by changing `build.rs` flags** — ghostty's own Debug and ReleaseFast builds of the identical C disagree (9/512 bytes, maxdelta 1, ReleaseFast's loop vectorizer), rustty sits on the consensus side across 5 toolchain/optimization combos on 2 OSes, and since the seam is live on macOS a flag change would _create_ divergence and violate R5. **WS11's real scope is that the coverage was VACUOUS**: every test reaching the resize ran at scale 1.0 (`8x16→8x16`, `136x128→136x128`) and `render_and_resize` never enters the bitmap arm (Inconsolata is an outline font) — swapping stb for nearest-neighbour left all 9 tests green. **DONE 2026-07-29.** Closed with 5 new tests — 2 direct-FFI (`src/stb.rs`: interpolation + `num_channels=4` channel independence) and 3 KATs (`face/freetype.rs`: Terminus 8×16→16×17/8×17 grayscale, NotoColorEmoji 136×128→16×15/8×7 BGRA downsample, plus a permanent machine-checked negative control) — and new gate **D10** (`check_stb_resize_parity.sh`) diffing our vendored C against ghostty's OWN build recipe at all four zig optimize modes, with `negative_control.py` proving the ±1 envelope still rejects nearest/bilinear. Nearest-neighbour substitution now FAILS both KATs while all 9 pre-existing tests still pass. Fixtures reproduce byte-for-byte under Fedora gcc 16.1.1 + clang 22.1.8 at all five opt levels; Linux runtime path 12/12 + 2/2 green. The `ReleaseFast` ±1 LSB is ledgered as a permanent divergence with a four-criterion basis. Two things WS11 found that this row did not: the grayscale strike is MONO (`num_grays=2`), so its fixtures are weak discriminators and bilinear is inseparable at 8×17 — the BGRA KAT is therefore not optional; and Fedora gcc `-O0/-O1` form a THIRD ±1 variant vs its own `-O2/-O3/-Os`, a fourth instance of the same codegen axis. **No deferral taken.** |
| **WS12 vaxis completion**    | `2026-07-23-ws12-vaxis-completion.md`  | **DELIVERED 2026-07-29.** `rustty-vaxis` now has **zero non-`#[cfg(windows)]` `unimplemented!()`**. (1) uucode width-table fork **RATIFIED as Option B, as shipped** (`e0abb03`) — recorded in `VAXIS_C1B_DESIGN.md`, and its gate PROVEN non-vacuous (perturb one generated table entry ⇒ `spot_check` + `gwidth_missing_wide_anchor` RED), plus UCD SHA-256 provenance pins and a ≥1000-case floor on the UAX #29 corpus. (2) Image tier LIVE (`transmit_image`/`load_image`/`Image::draw`/`cell_size`) via the new **`crates/rustty-zigimg`** — Option Z-C, re-homing the six zigimg operations libvaxis instantiates onto ghostty's own vendored codecs (~840 src LOC, vs 18,316 for a faithful zigimg port). One bounded PORT-NOTE divergence: `load_image` takes PNG+JPEG, not 15 formats. (3) `boo` verified end-to-end by a committed pty KAT with a DSR responder and a no-DSR negative control.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| **WS13 i18n & docs**         | `2026-07-23-ws13-i18n-docs.md`         | **DONE (2026-07-30), no deferrals.** The original row's premise was two-thirds stale: the `.po` corpus (31 catalogs, `e020db2`) and the `msgfmt` → `share/locale` step (`res_misc::gen_locale`, the port of `GhosttyI18n.zig:33-43`) were already shipping, and the REAL i18n defect went unmentioned — 3 `.blp` msgids still said "Ghostty" (matching no catalog) and all 25 code-side strings the reference wraps in `i18n._()` were unwrapped literals, so `xgettext` extracted **0** msgids from any `.rs` file and none of them could ever translate. WS13 fixed both (**0 → 25** extracted msgids), made `rustty_translate` forward to real `dgettext` (unblocked by WS9's vendored libintl), taught `xtask update-translations` to extract `gettext_`/`tr` msgids and regenerated the `.pot`/catalog refs under a msgid-set-invariance guard, turned `rustty-apprt/build.rs`'s dead `OUT_DIR/locale` emit into a real `msgfmt --check` gate, and ported the whole `src/build/mdgen` + `src/build/webgen` pipeline — real generated man pages (204 config options / 18 CLI actions / 85 keybind actions), `share/rustty/doc/*.{md,html}`, `share/rustty/webdata/*.mdx` — with a Rust markdown→roff/html renderer in place of `pandoc` (absent on both build hosts). Also closed the scope-addition rows: `src/main_build_data.zig`'s `+gen-*` CLI (`xtask gen +<action>`, O13a/O13b). Verified: `mandoc -Tlint` rc=0 with zero diagnostics (macOS) and `groff -man -ww` rc=0 (VM) — the cross-check found two real defects mandoc had accepted. Residual verification scope: `VERIFICATION_DEBT.md` **V7** (RTL Hebrew layout).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| **WS14 wasm/WebGL renderer** | `2026-07-23-ws14-wasm-webgl.md`        | DONE, no deferral. Premise corrected: upstream `renderer/WebGL.zig` is a **2-line placeholder** (a doc comment + `pub const WebGL = @This();`, identical on the oracle's `origin/main`) — there is no backend to port, and writing one would be an R1 violation — and `main_wasm.zig` was already ported as an rlib, so the real residual was the ARTIFACT, the retention set, and verification. Shipped: the WebGL placeholder port + the third `Renderer` selection arm; the resurrected `os/wasm.zig` host-ownership tracker (deleted upstream by `9dc2e5978`, which is why `Atlas.Wasm` could not compile as written) unblocking `Atlas.Wasm` with **all 4 reference asserts live**; `App.Wasm` completed (empty by construction — its exports are commented out upstream); a real freestanding `rustty_wasm.wasm` cdylib (`config_*` + `atlas_*` + malloc/free, 4 imports all `env`, zero wasi) enabled by an xev freestanding-wasm arm and default-on `renderer`/`oniguruma` features that drop 5 C `-sys` deps from the wasm link; and `example/wasm-webgl/` — a WebGL2 consumer gated by `.port/tools/gates/ws14_webgl_browser_gate.sh` (opt-in lane `ci.sh --webgl`), which asserts per-cell ink counts against a CPU expectation computed from the module's own atlas bytes and runs, on **every** invocation, two negative controls (wrong atlas offset; black shader) that must FAIL, plus 5 product-sensitivity arms proven RED against three real mutations of `Atlas::set` / `atlas_debug_canvas_`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |

## Housekeeping / project-health backlog (found during the program, not part of the original scope)

| WS                           | Plan file               | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ---------------------------- | ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **WS18 release-lane repair** | `TBD — author at start` | `cargo test --release` has **12 failing tests on main** (independently corroborated during the WS7 Phase B review, and present verbatim at `a21f965` — NOT caused by Phase B). Root cause: `rustty_util::tripwire::enabled()` is deliberately `cfg!(debug_assertions)` (the faithful equivalent of Zig's `builtin.is_test`, per its PORT NOTE), so in `--release` the fail-points never trip and every test that expects an injected failure fails. The design is correct; the _tests_ are simply not gated to match. Fix: mark the affected tests `#[cfg_attr(not(debug_assertions), ignore)]` — the precedent WS7 Phase B already established for its own new tripwire test. Then make `cargo test --release` a real, green gate so this cannot regress. Scope check: confirm all 12 are genuinely tripwire/allocation-failure tests before blanket-ignoring anything — do NOT silence a real release-only bug. **DONE** — see the outcome note below. |

### WS18 outcome (release lane now green + gated)

The "12" was the `-p rustty-vt --lib` slice only. A full `cargo test --release
--workspace` on a clean tree surfaced **22 failing tests across 7 targets**, and
the lane did not even reach the tests until a link break was fixed. Per-failure
triage:

- **1 build break (both profiles):** `libxml2-sys`'s KAT test binary failed to
  link — the vendored libxml2 is built `LIBXML_ICONV_ENABLED`, and on Apple
  targets `iconv` lives outside `libSystem`. Fixed by emitting
  `cargo:rustc-link-lib=iconv`.
- **19 debug-only-mechanism tests:** 17 tripwire fail-point tests plus
  `pagelist_erase_row_error_path_runs_integrity_gate` and
  `terminal_full_reset_tracked_pins`, which drive the slow-runtime-safety
  integrity gate / `pin_is_valid` (the reference compiles those under
  `build_options.slow_runtime_safety`). All now
  `#[cfg_attr(not(debug_assertions), ignore)]`; all still run and pass in debug.
- **1 flaky test, BOTH profiles (not release-only):** `rustty-bench`'s
  `Benchmark` test asserts `result.duration > 0` on two back-to-back monotonic
  reads; Apple Silicon's clock advances in ~41.67ns steps. Measured unmodified:
  27/200 failures in release, 19/200 in debug. De-flaked by spinning until the
  clock advances (300/300 green in both profiles afterwards).
- **1 ordering artifact (both profiles):** `rustty-apprt-c`'s C-ABI smoke needs
  the cdylib/staticlib uplifted into `target/<profile>/`, which only
  `cargo build` does. The gate runs the build first.
- **3 tests in one flaky HANG (both profiles, unresolved — see below).**

**Zero genuine release-ONLY product bugs were found.**

Enforcement: `.port/tools/gates/check_release_lane.sh` (**G-REL**), registered
HARD in `ci.sh`.

**Open finding — `rustty-termio` `subprocess_drop_repro` intermittent hang.**
Under full-workspace parallel load its three tests wedged indefinitely. The
forked children are macOS `login(1)` processes stuck in kernel "exiting" state
(`ps` STAT `?Es`), so they never reap; `killPid`'s faithful SIGHUP + `waitpid`
loop then spins forever. Not release-specific (two orphaned debug-build copies
from another worktree had been stuck for 5 days), and not reproducible when the
binary is run standalone in either profile. Not fixed here — the product loop is
faithful to the reference. G-REL wraps the run in a watchdog so a recurrence is a
loud RED rather than an infinite CI wait.

## VM track (Linux — addressable NOW via `ssh rvm`, not deferred)

| WS                           | Plan file                                | Scope                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------------------------- | ---------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **WS15 GTK GlobalShortcuts** | `2026-07-23-ws15-gtk-globalshortcuts.md` | XDG-portal DBus round-trip (`refresh`/`close`/`request`) on the VM's GNOME session; `global:` keybinds functional.                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| **WS16 X11 winproto**        | `2026-07-29-ws16-x11-winproto.md`        | DONE [2026-07-29]. The arm itself had already landed at `aae9536` (2026-07-19) — the "unported" premise in this row and in `DEFERRALS.md` was STALE. WS16 closed its three residual sub-deferrals (XKB init, `eventMods` XKB peek, `addSubprocessEnv`/`WINDOWID` + the missing dispatch seam), added the `rustty` binary's `wayland`/`x11` pass-through features, and verified live under `GDK_BACKEND=x11` on the VM (4 KATs + `xprop` on `WM_CLASS`/`_MOTIF_WM_HINTS`/blur/`WINDOWID`/urgency, each with a negative control). Residual verification scope in `VERIFICATION_DEBT.md` V5. |
| **WS17 verification debt**   | `2026-07-23-ws17-verification-debt.md`   | VD:V2 real-GPU rendering sign-off under virgl (VM display set to GPU-accelerated); VD:V4 mutter `set_urgent` KAT on the VM's real GNOME session; VD:V3 KDE/KWin quick-terminal (install `plasma-workspace` in the VM, run the slide/blur checks).                                                                                                                                                                                                                                                                                                                                         |

## Out of scope (platform-impossible, NOT deferrals of choice)

- Windows (ConPTY, iocp), wasi_poll backend, iOS device runtime: no target
  hardware/OS in this environment. These remain the only ledgered exclusions.

## Sequencing rules

1. Waves execute in order; within a wave, workstreams are independent and may
   run in parallel (macOS WSs and VM-track WSs can also overlap freely).
2. Each WS: branch → detailed plan's task loop (TDD, frequent commits) → both-
   platform build gate → ledger update → merge to main → delete branch.
3. WS plans are authored just-in-time at wave start by the plan-authoring
   protocol used for Wave 0/1 (research agent per WS, writing-plans format);
   authoring status tracked in the checklist below.

## Plan-authoring checklist

- [x] ROADMAP (this file)
- [x] WS1 shell-integration + themes (research note: vendoring + xtask emission already landed on main; plan covers verified residue)
- [x] WS2 macOS guards & plumbing (research note: quicklook real, contentsScale already faithful; one production change — autoreleasepool shim)
- [x] WS3 small parity fills (research note: host_view iOS arm + deployment target already implemented; PNG hook + version stubs are the real work)
- [x] WS4 app bootstrap (research note: detectArgs/Action::run already ported; GlobalState<Action> binds in rustty-apprt-c to avoid a cli→core cycle)
- [x] WS5 lib-vt C-API (research note: BL-5 commits live on oracle origin/main, not the 4749c4e pin; ghostty_* alias layer deleted upstream at e4cc852 — new exports rustty_* only)
- [x] WS6 macOS inspector — AUTHORED + EXECUTED + MERGED (both platforms green)
- [x] WS7 scrollback compression — AUTHORED; **Phase A EXECUTED + MERGED** (LZ4 codec); Phases B-E = Round 2
- [x] WS8 upstream small-sweep — AUTHORED + EXECUTED + MERGED (both platforms green)
- [x] WS9 packaging — AUTHORED + EXECUTED + MERGED (self-contained xcframework proven)
- ~~[ ] WS10–WS17: authored at wave start~~ **STALE [WS19 T2, 2026-07-29 — audit
  §E.4]; per-WS rows:**
- [x] WS10 perf (BL-4) — **EXECUTED 2026-07-30, T1–T22, merged** (`ea80044`, `d7c3afa`,
      `1073c9d`, `18fc255`; re-pin `badd4e8`). **DONE.** The row above said "NOT authored, not
      executed" until 2026-07-30 — it was STALE by 22 tasks, the same falsely-open drift the
      WS19 audit found in the ledgers, and it was keeping G-P2 green for the wrong reason.
      Corrected against git, not against memory. The measurement residue is ledgered where it
      belongs and cannot be run yet: **V17** (oracle-anchored campaign) and **V18** (`f2a7652ab`'s
      two BL-4 configs), both `Phase 3 — BEYOND-PIN`, because per **V15** the pinned oracle no
      longer builds under Zig 0.16.0. Audit row **O3** (`cursor_scroll_region_up`, `77190bd02`)
      is genuinely beyond-pin and belongs to Phase 3.
- [x] WS11 font glyph resize — **authored 2026-07-29, EXECUTED 2026-07-29 (10 tasks,
      no deferral)**. **CLOSED.** Audit row **O17** as reclassified: the `stb_image_resize`
      port itself SHIPPED at `2e82776`; WS11 owned its **vacuous** coverage (all reaching
      tests at scale 1.0; the bitmap arm never entered) and closed it with 5 new tests +
      gate **D10** vs ghostty's own build recipe at 4 zig optimize modes, each with a
      demonstrated RED arm. The stb `build.rs` flags were NOT changed — and D10's
      `--self-test` now proves why (`-ffast-math` makes the gate RED against ghostty's own
      Debug/ReleaseSafe/ReleaseSmall builds, i.e. a flag change would CREATE divergence).
- [x] WS12 vaxis completion — authored (`c7c93f7`); **EXECUTED 2026-07-29** (all 13
      tasks, no deferrals taken). Discharges the DEFERRALS.md "rustty-vaxis Chunk 1b"
      uucode row and the whole "Chunks 4a/4b" image-tier row (both now carry ✅ RESOLVED
      banners), and deletes the `boo` double-entry. Did NOT touch the `#[cfg(windows)]`
      family (Chunk 3b items 1/3) — **that was left open for WS20, which ported it in full
      at `4070c39`/`fe53d38`** (`WindowsTty` + the windows `Loop` arms). WS12 is therefore
      **merged and DONE**; the only remainder is running that code, which needs a Windows
      host and is filed as `ENVIRONMENT-BLOCKED` **V10**. Also HARDENED WS19 T1's
      `.port/tools/restore_zig_deps.sh`, whose only restore path was dead under Zig 0.16.0.
- [x] WS13 i18n & docs — authored (`39355ff`); **EXECUTED 2026-07-30 (10/10 tasks)**,
      branch `ws13-i18n-docs-final`, not yet merged. DONE. Audit rows **N8** (3 `.blp`
      msgids + the unwrapped code-side call sites) and **O13a/O13b**
      (`main_build_data` + the `+gen-*` CLI/file-emit wiring) are both CLOSED. The
      `src/build/mdgen/**` + `src/build/webgen/**` deferral row is struck. One
      residual, ledgered as verification scope not a deferral:
      `VERIFICATION_DEBT.md` **V7** (RTL Hebrew UI layout — human visual sign-off).
- [x] WS14 wasm/WebGL — authored (`2026-07-23-ws14-wasm-webgl.md`, 12 tasks);
      **EXECUTED 2026-07-30, all 12 tasks, no deferral taken**; branch
      `ws14-wasm-webgl`. **DONE.** Discharges all four inherited audit rows —
      **O13a** (`main_wasm.zig`: ported already; the residual artifact now ships as the
      freestanding `rustty_wasm.wasm` cdylib), **O14** (`Atlas.Wasm` test: all 4 asserts
      live, after resurrecting the tracker `9dc2e5978` deleted upstream), **O15**
      (`App.zig` `Wasm` block: complete, and empty by construction — which is why
      `rustty-wasm` needs no `rustty-core` edge), **O16** (WebGL backend: the premise was
      wrong — upstream's `renderer/WebGL.zig` is a 2-line placeholder, ported verbatim).
      Browser rendering is gated, not asserted by hand:
      `.port/tools/gates/ws14_webgl_browser_gate.sh` / `ci.sh --webgl`.
- [x] WS15 GTK GlobalShortcuts — authored + EXECUTED + **MERGED `e8a9f59`**. DONE.
- [x] WS16 X11 winproto — authored + EXECUTED + **MERGED `07b64db`**. DONE.
- [x] WS17 verification debt — authored (`ef7621a`); **EXECUTED 2026-07-29/30, merged**
      (`a099760`). **DONE.** Scope was V2/V3/V4/V1b + V5. Outcome: **V1b** signed off [WS6],
      **V3** decoration globals discharged under a live KWin, **V4** DISCHARGED against real
      mutter without going near the primary session (a second `--headless` mutter on its own
      socket — `ws17_set_urgent_mutter_gate.sh`, GREEN, VM uptime unchanged so the crash
      correlation was avoided rather than risked), **V5** discharged [WS16]. **V2** is the
      only remainder and is now `ENVIRONMENT-BLOCKED`: correctness on the GL renderer IS
      validated here (llvmpipe is a legitimate correctness environment and the glyph output
      was checked against upstream Ghostty 1.3.1); only the perf/behaviour-under-real-GL axis
      needs a box with an actual GPU, and `hw_gl_probe.sh` makes that a one-command answer.
- [x] WS18 release lane — authored in-ROADMAP + EXECUTED + **MERGED `eeec83f`**. DONE.
- [x] WS20 Phase 2 finish — the residue the WS19 audit surfaced. **EXECUTED 2026-07-30,
      merged in three waves** (`fe53d38` vaxis `WindowsTty` + Windows font backend;
      `ef519ca` flatpak spawn tier + iocp/wasi_poll watcher layer + ThreadPool futex;
      `12c951c` the 9 zero-counterpart GTK files — 5 real, 4 false positives). **DONE.**
      The row below is the ORIGINAL scope statement, kept verbatim as the record of what was
      owed; every item in it is now ported. What remains is verification that no machine here
      can perform, re-filed as `ENVIRONMENT-BLOCKED` rows **V8–V12** (Windows, WASI, flatpak
      sandbox) and **V7** (RTL + human visual judgment) — each naming its ported artifacts,
      which gate G-P2 resolves against disk so the marker cannot be asserted for unported
      work. **V13** (wiring `Command` to the ported `pre_exec`/`post_fork`) and **V14**
      (`gtk-v4_16` handler-level proof) were real gaps and are being closed separately.
      ORIGINAL SCOPE, for the record:
      Owns everything the item-level audit (`.port/phase-3/PHASE2_FINAL_AUDIT.md`) found
      still unported, none of which may be excused as scope: **vaxis `WindowsTty`** (7 stubs
      vs `vaxis/src/tty.zig:189-691`, ~503 reference lines, + the `Loop.zig` arms); the
      **Windows font backend** (`DeferredFace.Windows` payload + the whole `Windows`
      discoverer, commented out at `discovery.rs:1911-2073`, while `Backend::FreetypeWindows`
      already exists); the **flatpak spawn tier** (`exec.rs:109` holds an EMPTY local
      placeholder while `rustty_os::FlatpakHostCommand` is fully ported at
      `flatpak.rs:202-700`); the **9 GTK files with zero counterpart** (~950 LOC:
      `ipc/{DBus,new_window,toggle_quick_terminal}`, `pre_exec`, `post_fork`, `cgroup`,
      `flatpak`, `class/config_errors_dialog`, `build/blueprint`); the **libxev watcher layer
      instantiated against `iocp`/`wasi_poll`** (8 `watcher_seam` sites) and **`ThreadPool`'s
      Windows futex arm**; the 2 GTK `v4_14`-ceiling forced-legacy arms; and the Linux
      `GL-ATTACH` no-op (confirm its premise first). Also owns **V8/V9** (the new backends
      are cross-compile-verified but never executed) and **V7** (RTL layout), all three of
      which were filed against CLOSED workstreams or against `Phase 3` without being
      beyond-pin. NOT in scope: BL-5's `render_state` split and `cursor_scroll_region_up`,
      which are genuinely BEYOND-PIN and belong to Phase 3.
- [ ] WS19 Phase 2 closeout — authored
      (`docs/superpowers/plans/2026-07-29-ws19-phase2-closeout.md`); **IN FLIGHT, not
      executed to completion — open.** Lands the standing closure gate **G-P2** and closes
      Phase 2. **Executed: T1 T2 T3 T4 T7 T11 T13 T14 T15 T16 T18 T19** (+ the new audit row
      N11, and the O17 correction from the WS11 lane). **REMAINING: T5** (`style::Color`
      padding-free), **T6** (the `-C debug-assertions=off` vt Miri lane; must follow T5),
      **T8** (GTK portal `OpenURI`, VM), **T9** (wayland decoration-mode re-read, VM),
      **T10** (kqueue `thread_perform` `&mut Loop`), **T12** (`+version` OSC-8 hyperlink),
      **T17** (termio `threadEnter` drain ordering + `posix_waitpid` `ECHILD`), and **T20**
      (the both-platform finale). Gate **G-P2** enumerates exactly these as RED and will not
      go green until they land — see its output, which is the authoritative worklist.

## Wave status

- **Wave 0 COMPLETE** (WS1/WS2/WS3) — both platforms green.
- **Wave 1 COMPLETE** (WS4/WS5) — both platforms green.
- **Wave 2 COMPLETE** (WS6/WS7-A/WS8/WS9) — main 53f1387; macOS + Linux both `cargo build --workspace`
  exit 0 and rustty-vt 2111 passed/0 failed; BL-2 rss_probe PASS (9.0 MiB returned) on Linux.
- ~~**Round 2 next**: WS7 Phases B-E (unblocked — BL-2 has landed).~~
  **STALE [WS19 T2, 2026-07-29 — audit §E.4]: WS7 Phases B (`098fc63`), C (`ab2c08c`),
  D1–D8 and E (`bbecaf7`) are ALL MERGED.**

### True remaining scope [WS19 T2, 2026-07-29]

| WS                        | Plan                                      | Executed                                                                                            | True state                                                                                                                                                                                                                   |
| ------------------------- | ----------------------------------------- | --------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| WS1–WS9                   | authored                                  | **merged**                                                                                          | DONE                                                                                                                                                                                                                         |
| WS10 perf (BL-4)          | **NOT authored**                          | no                                                                                                  | open; inherits O3 (pin-sequenced)                                                                                                                                                                                            |
| WS11 font glyph resize    | authored 2026-07-29 (WS11 lane, 10 tasks) | **yes — 2026-07-29**                                                                                | **CLOSED, no deferral.** O17 as reclassified: the port shipped at `2e82776`; the VACUOUS COVERAGE was the work, closed by 5 tests + gate D10 (both platforms verified; Linux D10 = BLOCKED/3 per contract, no zig on the VM) |
| WS12 vaxis completion     | authored (`c7c93f7`)                      | **executed 2026-07-29 (13/13 tasks); branch `ws12-vaxis-completion`, not yet merged**               | image tier LIVE via new `rustty-zigimg`; uucode Option B ratified + gate proven non-vacuous; `boo` pty KAT committed                                                                                                         |
| WS13 i18n & docs          | authored (`39355ff`)                      | **executed 2026-07-30 (10/10 tasks); branch `ws13-i18n-docs-final`, not yet merged**                | DONE, no deferrals; N8 + O13a/O13b closed; mdgen/webgen deferral struck; residual = VERIFICATION_DEBT V7 (RTL layout)                                                                                                        |
| WS14 wasm/WebGL           | authored (12 tasks)                       | **executed 2026-07-30 (12/12 tasks, no deferrals); branch `ws14-wasm-webgl`**                       | **CLOSED.** O13a/O14/O15/O16 all discharged; O16's premise corrected (no WebGL backend exists upstream); browser output pixel-gated with 2 always-run negative controls + 5 product arms                                     |
| WS15 GTK GlobalShortcuts  | authored                                  | **merged `e8a9f59`**                                                                                | DONE                                                                                                                                                                                                                         |
| WS16 X11 winproto         | authored                                  | **merged `07b64db`**                                                                                | DONE                                                                                                                                                                                                                         |
| WS17 verification debt    | authored (`ef7621a`)                      | **no**                                                                                              | open; scope V2/V3/V4/V1b **+ V5**                                                                                                                                                                                            |
| WS18 release lane         | authored in-ROADMAP                       | **merged `eeec83f`**                                                                                | DONE                                                                                                                                                                                                                         |
| **WS19 Phase 2 closeout** | this wave                                 | **partially — T1–T4, T7, T11, T13–T16, T18, T19 landed; T5, T6, T8, T9, T10, T12, T17, T20 REMAIN** | open; lands **G-P2**, which is RED and names the remainder                                                                                                                                                                   |

**The open set is exactly WS10, WS11, WS12, WS13, WS17 — plus Phase 3
(`.port/phase-3/`, upstream resync).** Gate **G-P2**
(`.port/tools/gates/check_phase2_closure.py`) derives that set FROM THIS CHECKLIST and
FAILS if any Phase-2 ledger row is open without an owner in it, so this table and the
per-WS checklist above are load-bearing: marking a WS `merged`/`DONE` immediately makes
filing new work into it a gate failure.
