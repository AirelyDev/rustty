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
  (Ghostty→Rustty, ghostty→rustty); `ghostty_*` C-ABI aliases stay.
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
- Shell-integration/themes: real content exists in `zig-out/share/rustty/` and
  the current app bundle, but is NOT reproducible — xtask still emits the
  deferral placeholder (`xtask/src/res_terminfo.rs:216-245`). WS1 makes it
  vendored + generated.

---

## Wave 0 — quick wins (each ≤ a day)

| WS | Plan file | Scope |
|----|-----------|-------|
| **WS1 resources** | `2026-07-23-ws1-shell-integration-themes.md` | Vendor ghostty `src/shell-integration/` (bash/zsh/fish/elvish/nushell) + the iTerm2 themes corpus (oracle tarball `ghostty-themes-release-20260608`) as rustty data; make `xtask resources` emit `share/rustty/{shell-integration,themes}` (+ ghostty parity profile dir) with R2 renaming; delete the placeholder DEFERRALS.md emission; KAT: byte-compare against oracle output; verify OSC7/prompt-marking live in Rustty.app and `list_themes` shows 534 themes. |
| **WS2 macOS guards & plumbing** | `2026-07-23-ws2-macos-guards-plumbing.md` | Verify current state of `quicklook_word`/`quicklook_font` (ledger conflict: E10 says filled, MACOS_APP_GAPS says open) and close whatever remains with real impls or Swift-tolerated null returns (no abort paths reachable from Look-Up/VoiceOver); cross-DPI `contentsScale` re-stamp on monitor move (Metal `set_content_scale` mailbox); `drainMailbox` autoreleasepool shim; run the ignored `macos_window_host.rs` present/resize KATs (B2–B4) and un-ignore what passes. |
| **WS3 small parity fills** | `2026-07-23-ws3-small-parity-fills.md` | `sys.rs decode_png` default-hook wiring to `rustty-wuffs::png::decode` (kitty-graphics PNG parity); `+version` STUB lines (RENDERER/LIBXEV/TOOLCHAIN) → real values; `host_view()` iOS mapping + non-darwin compile check (INTEGRATION_PARITY §2.6/§2.9); deployment-target mismatch warning fix. |

## Wave 1 — bootstrap & ABI surface

| WS | Plan file | Scope |
|----|-----------|-------|
| **WS4 app bootstrap (BREAK #1)** | `2026-07-23-ws4-app-bootstrap.md` | Port the full `global.zig` `state.init()`: allocator selection, `detectArgs` CLI-action detection, `GHOSTTY_LOG` logging config, `crash.init`, `ensureLocale`/`i18n.init`, SIGPIPE setup; then un-stub `rustty_cli_try_action`'s non-null path (run action + exit semantics through the C ABI). Verified via embedded harness + Rustty.app relaunch. |
| **WS5 lib-vt C-API (BL-5)** | `2026-07-23-ws5-libvt-capi.md` | Upstream BL-5 series: OSC-52 `clipboard_set`, color utility APIs, color-scheme report, OSC/Kitty color queries, unicode-width APIs, scroll-to-row, RGB-by-pointer ABI fix. Extends `rustty-vt-c` + headers + A4 gate freeze. |

## Wave 2 — features & robustness

| WS | Plan file | Scope |
|----|-----------|-------|
| **WS6 macOS inspector** | `2026-07-23-ws6-macos-inspector.md` | dcimgui `ImGui_ImplMetal` FFI (mirror the GTK OpenGL3 precedent in `dcimgui-sys`); port the ~4.4k-LOC inspector widget render sections; fill `Surface.activate/deactivateInspector` + `embedded::Inspector::{init,initMetal,renderMetal}` + `rustty_inspector_metal_*` exports; remove the no-op crash sentinel; cmd-opt-i opens a live inspector in Rustty.app. |
| **WS7 scrollback compression (BL-1)** | `2026-07-23-ws7-scrollback-compression.md` | The ~35-commit upstream stack: LZ4 codec crate-sys, compressed `Page` representation, generation stamps, search-integrity integration. Largest single WS; Miri + parity KATs against oracle transcripts; perf gate: no regression on the 14-benchmark suite. |
| **WS8 upstream small-fix sweep** | `2026-07-23-ws8-upstream-small-sweep.md` | BL-3 pending-small queue (~30 runtime-safety fixes), BL-2 free-list page return-to-OS, BL-6 kitty/gfx generation stamps, BL-7 OSC 72 drag-and-drop parser, tabstops-on-resize PARTIAL completion. Sequenced per MAIN_PARITY triage order. |
| **WS9 packaging** | `2026-07-23-ws9-packaging.md` | Self-contained xcframework: vendor full-zlib deflate already done — extend gettext-sys to full libintl (~23 TUs incl. Bison `plural.c`) killing the Homebrew `-lintl` link; archive set from `cargo build --message-format=json` instead of the glob; ranlib normalization before libtool; `MACOSX_DEPLOYMENT_TARGET` alignment. Gate: `build-rustty.sh` succeeds on a PATH without `/opt/homebrew`. |

## Wave 3 — perf, niche & wasm

| WS | Plan file | Scope |
|----|-----------|-------|
| **WS10 perf series (BL-4)** | `2026-07-23-ws10-perf-series.md` | SIMD scans, bulk CSI/print, pty pipelining. Guarded by the benchmark suite (goal: hold or extend the 14/14 wins). |
| **WS11 font glyph resize** | `2026-07-23-ws11-font-glyph-resize.md` | `stb_image_resize` parity for the FreeType glyph-resize path (stb-sys cc build for bit-exactness), wired on the non-macOS path, KAT vs oracle rasterization. |
| **WS12 vaxis completion** | `2026-07-23-ws12-vaxis-completion.md` | Resolve the uucode width-table fork (presents Option A extend rustty-unicode vs Option B vendor uucode-0.1.0 tables to the human — decision REQUIRED in-plan, not deferred); zigimg image tier (`transmit_image`/`load_image`/`Image::draw`/`cell_size`); verify `boo` end-to-end. |
| **WS13 i18n & docs** | `2026-07-23-ws13-i18n-docs.md` | Vendor the `.po` corpus + msgfmt step (real `share/locale`); mdgen man-page pipeline (markdown→roff); `rustty_translate` live. |
| **WS14 wasm/WebGL renderer** | `2026-07-23-ws14-wasm-webgl.md` | Port the WebGL backend + wasm entry (`main_wasm`, `Atlas.Wasm` C-ABI/JS surface); build `wasm32` target green; **validate live in Chrome via the browser automation tools** (load harness page, screenshot the rendered canvas, assert glyph output). |

## Housekeeping / project-health backlog (found during the program, not part of the original scope)

| WS | Plan file | Scope |
|----|-----------|-------|
| **WS18 release-lane repair** | `TBD — author at start` | `cargo test --release` has **12 failing tests on main** (independently corroborated during the WS7 Phase B review, and present verbatim at `a21f965` — NOT caused by Phase B). Root cause: `rustty_util::tripwire::enabled()` is deliberately `cfg!(debug_assertions)` (the faithful equivalent of Zig's `builtin.is_test`, per its PORT NOTE), so in `--release` the fail-points never trip and every test that expects an injected failure fails. The design is correct; the *tests* are simply not gated to match. Fix: mark the affected tests `#[cfg_attr(not(debug_assertions), ignore)]` — the precedent WS7 Phase B already established for its own new tripwire test. Then make `cargo test --release` a real, green gate so this cannot regress. Scope check: confirm all 12 are genuinely tripwire/allocation-failure tests before blanket-ignoring anything — do NOT silence a real release-only bug. **DONE** — see the outcome note below. |

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

| WS | Plan file | Scope |
|----|-----------|-------|
| **WS15 GTK GlobalShortcuts** | `2026-07-23-ws15-gtk-globalshortcuts.md` | XDG-portal DBus round-trip (`refresh`/`close`/`request`) on the VM's GNOME session; `global:` keybinds functional. |
| **WS16 X11 winproto** | `2026-07-23-ws16-x11-winproto.md` | Port `gtk/winproto/x11.rs` (gdk4-x11), runtime-verified under XWayland on the VM. |
| **WS17 verification debt** | `2026-07-23-ws17-verification-debt.md` | VD:V2 real-GPU rendering sign-off under virgl (VM display set to GPU-accelerated); VD:V4 mutter `set_urgent` KAT on the VM's real GNOME session; VD:V3 KDE/KWin quick-terminal (install `plasma-workspace` in the VM, run the slide/blur checks). |

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
- [ ] WS10–WS17: authored at wave start

## Wave status
- **Wave 0 COMPLETE** (WS1/WS2/WS3) — both platforms green.
- **Wave 1 COMPLETE** (WS4/WS5) — both platforms green.
- **Wave 2 COMPLETE** (WS6/WS7-A/WS8/WS9) — main 53f1387; macOS + Linux both `cargo build --workspace`
  exit 0 and rustty-vt 2111 passed/0 failed; BL-2 rss_probe PASS (9.0 MiB returned) on Linux.
- **Round 2 next**: WS7 Phases B-E (unblocked — BL-2 has landed).
