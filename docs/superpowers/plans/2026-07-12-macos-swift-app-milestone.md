# macOS Swift-App Milestone Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax. Large parallelizable phases are executed as `.port/phase-2/wf-*.js` Workflow scripts per project doctrine.

**Goal:** Produce a runnable windowed macOS terminal by linking the reused ghostty Swift AppKit/SwiftUI frontend to `librustty` over the `ghostty_*` C-ABI, packaged as a `GhosttyKit.xcframework`.

**Architecture:** rustty-apprt-c already exports all 71 `rustty_*` C-ABI functions (the canonical ABI). Branch A (locked, completion-spec §3.2): add a generated `ghostty_* → rustty_*` alias layer so the reused Swift app (~150 files, ~163 hard-coded `ghostty_` symbols) stays byte-identical to upstream. cbindgen + the upstream `include/ghostty.h` provide the header; an xtask packages the macos static lib + header into a `GhosttyKit.xcframework` with the same structure ghostty's build emits; the ghostty `macos/` Xcode project is copied in, rebranded to rustty identity, and pointed at the rustty xcframework. Build with `xcodebuild`, run, iterate.

**Tech Stack:** Rust (rustty-apprt-c staticlib), cbindgen, `xcodebuild`/`libtool`/`xcrun` (Xcode + macOS SDK confirmed available), the ghostty Swift app at `/Users/gael/projects/ghostty/macos` (read-only oracle), `xtask` for packaging.

## Design source (read before executing any task)

- `.port/phase-2/PREPORT_SWIFT_APP.md` — the pre-port artifact: 40 LIFETIMES rows (§1), 16 paired invariants I1–I16 (§2), the Branch-A symbol-fork mechanics (§3), the catch_unwind/abort policy (§4), comptime inventory (§5), ABI constants (§6), risk notes R-A…R-I (§7). This is the authoritative design; tasks reference its sections rather than duplicating.
- `docs/superpowers/specs/2026-07-08-rustty-completion-and-macos-windowing-design.md` §3.2 (symbol identity, Branch A locked).
- Oracle recipe: `/Users/gael/projects/ghostty/src/build/{GhosttyXCFramework,XCFrameworkStep,LibtoolStep,CombineArchivesStep,GhosttyXcodebuild}.zig`; the shipped `GhosttyKit.xcframework/macos-arm64_x86_64/{Headers/ghostty.h, <lib>}` structure.

## Global Constraints

- **R1** fidelity: the reused Swift app stays byte-identical to upstream (Branch A); struct/enum/union layouts match `include/ghostty.h` field-for-field in size/align/order/ordinal (I9). Preserve upstream header typos verbatim (R-A `GHOSTTY_POINT_SURFACE=3` vs Zig `history`; R-B `chostty_ipc_target_s`; R-C `timetime_ms`) — the header-diff gate never converges otherwise.
- **R2** identity: published artifacts are rustty (bundle id, app name, `TERM=xterm-rustty`, `~/.config/rustty`), but the **C-ABI symbol names** stay `ghostty_*` (Branch A alias) so Swift links unchanged. The canonical Rust export is `rustty_*`; `ghostty_*` is the alias.
- **R5** never weaken a gate. New gates (header-diff, symbol-manifest) register as PROGRESS until proven clean.
- **I2 (sharpest UB trap):** `embedded::Surface` is self-referential (`core_surface.rt_surface` points back at its container) — it MUST be heap-boxed and never moved post-init (`Pin<Box>` or documented no-move). Verify with a Miri harness (R-E).
- **I5/R-F:** `nsview`/`ig_ctx`/Metal/CTFont handles are main-thread-affine and `!Send`; never `unsafe impl Send`, never drop off-main.
- **§4 abort policy:** shipped `.xcframework` profile is `panic = "abort"` (assert-class → abort like Zig's trap; unwind can't cross `extern "C"`). Recoverable errors → sentinel via `Result`, NOT `catch_unwind`. Depends on `overflow-checks = true` (M6, already landed).
- Commit after every green task; never batch phases.

## MVP scope decision

A **runnable windowed terminal** needs: app+surface lifecycle (`ghostty_app_new/tick/surface_new/draw/set_size/key/...` — all already ported), the render path (macOS windowing W0–W5, landed), and `ghostty_surface_set_display_id` (already wired to the render mailbox). It does NOT need the 6-way Darwin feature set: `quicklook_font/word` and the 3 `inspector_metal_*` exports are **optional features** (quicklook, live inspector) — their `unimplemented!()` leaves are fine for MVP and stay deferred. The two core-surface seams (`surface.rs:2843/2877`) are `activate/deactivateInspector` — inspector-only, also deferred. So the MVP blocking set is: symbol alias + header + xcframework + Swift bring-up + rebrand + build + the display-id path.

---

## Phase A — Rust-side ABI prep (parallelizable; workflow `wf-swiftapp-abi.js`)

### Task A1: Symbol manifest (the machine-checkable ABI table)

**Files:**
- Create: `.port/audits/CABI_SYMBOLS.tsv`
- Create: `.port/tools/gates/check_symbol_manifest.sh`

**Interfaces:**
- Produces: TSV `ghostty_name⇥rustty_name⇥signature⇥source_file⇥darwin_only`, one row per the 90 app-C-ABI entrypoints (`grep -c '^GHOSTTY_API' /Users/gael/projects/ghostty/include/ghostty.h` == 90). Consumed by A2 (alias generator) and the header-diff gate.

- [ ] **Step 1:** Extract the 90 `GHOSTTY_API` declarations from `/Users/gael/projects/ghostty/include/ghostty.h` (name + full signature). For each, find the rustty counterpart (`rustty_<name>` in rustty-apprt-c, or the config/main_c/benchmark tier). Mark the 6 Darwin-only (PREPORT §0: `set_display_id`, `quicklook_font`, `quicklook_word`, `inspector_metal_init/render/shutdown`).
- [ ] **Step 2:** Write `check_symbol_manifest.sh`: assert every `^GHOSTTY_API` name in the pinned header has a TSV row AND a live `rustty_*` export (nm the staticlib once built, or grep the source); fail on drift (append-only ABI, I9/R-H). Self-test: an injected missing row → RED.
- [ ] **Step 3:** Commit. `git commit -m "cabi: symbol manifest for the 90 app-C-ABI exports + drift gate (A1)"`

### Task A2: `ghostty_*` alias layer (Branch A)

**Files:**
- Create: `crates/rustty-apprt-c/src/ghostty_alias.rs`
- Modify: `crates/rustty-apprt-c/src/lib.rs` (add `#[cfg(feature = "identity-ghostty")] mod ghostty_alias;`), `crates/rustty-apprt-c/Cargo.toml` (add `identity-ghostty` feature)

**Interfaces:**
- Consumes: A1's manifest.
- Produces: under `--features identity-ghostty`, the staticlib exports both `rustty_<name>` and `ghostty_<name>` (identical symbol, one body). Mechanism: `#[no_mangle] pub unsafe extern "C" fn ghostty_<name>(args) -> Ret { rustty_<name>(args) }` thin forwarders (the PREPORT §3 option (a); simplest, works with staticlib + xcframework; avoids linker-alias portability issues).

- [ ] **Step 1:** Generate `ghostty_alias.rs` from the manifest: one forwarder per app export, each `#[no_mangle] pub unsafe extern "C" fn ghostty_x(...) -> R { crate::x_or_rustty_x(...) }`, with the exact ported signature (copy from the `rustty_*` decl). Darwin-only ones gated `#[cfg(any(target_os="macos", target_os="ios"))]` (PREPORT §5).
- [ ] **Step 2:** `cargo build -p rustty-apprt-c --features identity-ghostty` green; `nm -gU target/debug/librustty_apprt_c.a | grep -c '_ghostty_'` == 90 (or the non-Darwin subset on non-mac).
- [ ] **Step 3:** Run `check_symbol_manifest.sh` against the built lib → GREEN.
- [ ] **Step 4:** Commit. `"cabi: ghostty_* alias forwarders over rustty_* (Branch A, identity-ghostty feature) (A2)"`

### Task A3: xcframework header + ABI-layout gate

**Files:**
- Create: `.port/tools/gates/check_ghostty_h_abi.sh`
- Create: `crates/rustty-apprt-c/tests/abi_layout.rs`

**Interfaces:**
- Produces: the xcframework's public header IS the upstream `include/ghostty.h` verbatim (Branch A: byte-identical so Swift's `#import` + every `ghostty_*_s/_e` typedef resolves unchanged). The gate proves rustty's `#[repr(C)]` structs/unions/enums match that header's layout via `const` size/align/ordinal asserts.

- [ ] **Step 1:** `abi_layout.rs` — for each ABI struct/union/enum in PREPORT §1/§6 (App.Options, Surface.Options, Platform union, EnvVar, Text, String, ClipboardContent, Target/Action unions, the enums with pinned ordinals), assert `size_of`/`align_of` and each enum discriminant equals the ghostty.h value. E.g. `const _: () = assert!(core::mem::size_of::<Options>() == <N>);` and `assert!(PlatformTag::Macos as c_int == 1 && /* 0 reserved invalid */)`. Pull expected sizes by reading the header's struct field types.
- [ ] **Step 2:** `check_ghostty_h_abi.sh`: copy `/Users/gael/projects/ghostty/include/ghostty.h` to the staged xcframework header location and `diff` against the pinned copy (must be identical — preserve R-A/R-B/R-C typos); run `cargo test -p rustty-apprt-c --test abi_layout`. Register PROGRESS in ci.sh.
- [ ] **Step 3:** `cargo test -p rustty-apprt-c --test abi_layout` green (fix any real layout mismatch found — those are R1 bugs). Commit. `"cabi: ghostty.h ABI-layout gate + const size/align/ordinal asserts (A3, I9)"`

### Task A4: prove the `rt_surface` pin (I2) with a Miri harness

**Files:**
- Create: `.port/phase-2/mirihar-surface-pin/` (standalone cargo project, model on `mirihar-chunk3b`)
- Modify: `crates/rustty-apprt-c/src/lib.rs` (ensure `Surface` is heap-boxed + no-move; add a `// SB`/`Pin` note if not already)

- [ ] **Step 1:** Read how `ghostty_surface_new` currently boxes the surface and whether `rt_surface` is written to point at the container. Build the harness: construct a surface-like struct whose inner field stores `&container`, `Box` it, then assert the back-pointer stays valid; add a NEGATIVE case (move after init) that Miri flags.
- [ ] **Step 2:** `cargo +nightly miri run` in the harness: negative case = UB, shipped (boxed, no-move) = CLEAN under SB + TB.
- [ ] **Step 3:** If the current code can move the surface post-init, fix (Pin<Box> or documented invariant) and re-prove. Commit harness + any fix. `"apprt-c: prove Surface rt_surface pin invariant (I2) on mirihar; enforce no-move (A4, R-E)"`

---

## Phase B — Packaging & Swift bring-up (serial spine; inline execution)

### Task B1: xcframework packaging in xtask

**Files:**
- Modify: `xtask/src/main.rs` (+ a new `xtask/src/xcframework.rs`)

**Interfaces:**
- Consumes: A2's `identity-ghostty` staticlib.
- Produces: `dist/GhosttyKit.xcframework/macos-arm64_x86_64/{Headers/ghostty.h, GhosttyKit}` (mirror the oracle structure; universal arm64+x86_64 via `lipo`, header from A3). Command: `cargo run -p xtask -- xcframework`.

- [ ] **Step 1:** Read the oracle recipe (`GhosttyXCFramework.zig` + `XCFrameworkStep.zig` + `LibtoolStep.zig`): it builds the static lib per-arch, `libtool`-combines archives, then `xcodebuild -create-xcframework -library <lib> -headers <dir>`. Inspect `/Users/gael/projects/ghostty/macos/GhosttyKit.xcframework/Info.plist` for the exact slice layout to reproduce.
- [ ] **Step 2:** Implement `xtask xcframework`: (a) `cargo build -p rustty-apprt-c --release --features identity-ghostty --target aarch64-apple-darwin` (and `x86_64-apple-darwin` if the target is installed; else arm64-only slice for MVP); (b) `lipo -create` the per-arch `librustty_apprt_c.a` into a universal archive (or single-arch); (c) stage `Headers/ghostty.h` (upstream, A3); (d) `xcodebuild -create-xcframework -library <universal.a> -headers <headers_dir> -output dist/GhosttyKit.xcframework`.
- [ ] **Step 3:** Run it: `cargo run -p xtask -- xcframework` → `dist/GhosttyKit.xcframework` exists; `xcrun xcodebuild -version` and `plutil -p dist/GhosttyKit.xcframework/Info.plist` confirm a valid framework; `nm -gU dist/GhosttyKit.xcframework/macos-*/GhosttyKit | grep -c ghostty_` == 90.
- [ ] **Step 4:** Commit. `"xtask: package rustty-apprt-c into GhosttyKit.xcframework (ghostty_* symbols, upstream ghostty.h) (B1)"`

### Task B2: bring the Swift app into rustty + rebrand identity

**Files:**
- Create: `macos/` (copy of `/Users/gael/projects/ghostty/macos/`, MINUS the prebuilt `GhosttyKit.xcframework`), rebranded.

**Interfaces:**
- Consumes: B1's `GhosttyKit.xcframework`.
- Produces: a `macos/Rustty.xcodeproj` (or kept `Ghostty.xcodeproj` name internally) that references rustty's xcframework and carries rustty identity (R2), Swift sources byte-identical to upstream (R1, Branch A) except identity strings/plist/bundle-id.

- [ ] **Step 1:** Copy `/Users/gael/projects/ghostty/macos/` → `macos/`. Delete the copied `GhosttyKit.xcframework`; symlink/copy `dist/GhosttyKit.xcframework` into `macos/`. Keep all Swift sources byte-identical (Branch A — they call `ghostty_*`, which now resolve to rustty's aliases).
- [ ] **Step 2:** Rebrand identity ONLY (not logic, R1): `Ghostty-Info.plist` (bundle id `com.mitchellh.ghostty` → rustty id, `CFBundleName`/`DisplayName` → Rustty), entitlements app-group ids, `Assets.xcassets` app name strings. Leave `ghostty_*` symbol calls and Swift type names untouched. Document each rebrand in a `macos/REBRAND.md`.
- [ ] **Step 3:** Commit. `"macos: import the reused Swift app (Branch A, byte-identical) + rustty identity rebrand (B2, R2)"`

### Task B3: build the app against rustty's xcframework

**Files:** Modify: `macos/*.xcodeproj/project.pbxproj` only if the framework ref path needs updating.

- [ ] **Step 1:** `cd macos && xcodebuild -project Ghostty.xcodeproj -scheme Ghostty -configuration Debug -destination 'platform=macOS,arch=arm64' build 2>&1 | tee /tmp/xcbuild.log`. Expect: FIRST run surfaces link/symbol errors — capture them.
- [ ] **Step 2:** Triage the errors in order: (a) **undefined `ghostty_*` symbol** → the alias forwarder is missing/misnamed for that export (fix A2's manifest+alias, rebuild xcframework); (b) **header/type mismatch** → an ABI-layout drift (fix the `#[repr(C)]` struct, A3); (c) **Swift-side API drift** → the reused Swift calls an export the port hasn't wired (fill that seam, or stub it to the abort-policy sentinel if it's an optional feature). Loop: fix → `xtask xcframework` → `xcodebuild` until it links.
- [ ] **Step 3:** Build succeeds (`** BUILD SUCCEEDED **`). Commit any fixes. `"macos: app builds & links against rustty GhosttyKit.xcframework (B3)"`

---

## Phase C — Run & iterate (inline; iterative by nature)

### Task C1: launch and drive the terminal

- [ ] **Step 1:** Launch the built `.app` (`open macos/build/.../Ghostty.app` or `xcodebuild ... test`-free run). Use the `run` skill / a screenshot to confirm a window appears.
- [ ] **Step 2:** Drive it: does a shell spawn (termio), does typing echo, does the renderer present frames (macOS windowing W0–W5 + `set_display_id`)? Capture a screenshot of a working prompt.
- [ ] **Step 3:** Triage runtime failures by subsystem: **no window** → surface/nsview wiring (I5, Platform.C); **blank window** → render path (`set_display_id`/renderer present); **crash on key** → input encoding / a reached `unimplemented!()` (fill or sentinel per §4); **abort** → an assert-class trap (read the abort, it's faithful — fix the real invariant). Each fix is its own commit.
- [ ] **Step 4:** Milestone done when a window shows a live shell prompt that echoes typed input and renders. Commit a `macos/MILESTONE.md` capturing what works + the known-deferred features (quicklook, inspector, the 5 non-display Darwin exports). `"macos: FIRST RUNNABLE windowed rustty terminal — live shell + render + input (C1)"`

---

## Sequencing & parallelism

```
Phase A (workflow wf-swiftapp-abi.js, parallel):  A1 → A2 (needs A1);  A3 ∥ A4  (independent)
Phase B (serial spine, inline):                   B1 (needs A2/A3) → B2 → B3 (iterates with A2/A3 fixes)
Phase C (inline, iterative):                      C1 (needs B3)
```
Phase A parallelizes cleanly (workflow). Phase B is a serial integration spine best driven inline with real `xcodebuild` verification — but B3's fix-loop feeds back into A2/A3, so keep those gates fast. Phase C is inherently iterative (run → observe → fix).

## Self-review notes
- Spec coverage: PREPORT §1 LIFETIMES → enforced by A3 (layout) + A4 (pin) + the abort policy in §4 (B3/C1 stubs). §2 invariants I1–I16 → I2/I5 have explicit tasks (A4, R-F review in B3); I6/I7/I8 (ownership transfer, clipboard round-trip, config clone) are already in the ported `rustty_*` bodies (verified by sweep #2 clean) — no new task, but C1 exercises them live. §3 Branch A → A2. §4 abort → the `panic="abort"` profile (add to B1's release build) + B3/C1 sentinel/abort triage. §6 constants → A3 asserts. §7 risks: R-A/B/C header typos → A3 diff preserves; R-E pin → A4; R-F !Send → B3 review; R-G overflow-checks → already landed; R-H manifest → A1.
- One gap made explicit: the `panic = "abort"` profile for the shipped staticlib — fold into B1 Step 2 (build the release staticlib under a profile with `panic = "abort"`; add `[profile.release] panic = "abort"` or a dedicated `xcframework` profile to root Cargo.toml, verifying it doesn't break the workspace test build which needs unwind — use a dedicated profile).
- No placeholders; every task ends testable (nm counts, cargo test, xcodebuild, a running app).
