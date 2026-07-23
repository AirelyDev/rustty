# WS2 — macOS Embedded-App Guards & Plumbing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the four WS2 gaps from `.port/phase-2/MACOS_APP_GAPS.md` — quicklook/Look-Up abort risk, cross-DPI contentsScale re-stamp, drainMailbox autoreleasepool, and B2–B4 present/resize verification — leaving the ledgers truthful.

**Architecture:** Research (2026-07-23, this plan's authoring pass) resolved the ledger conflict: **the quicklook exports and the entire content-scale chain are ALREADY fully implemented** — `MACOS_APP_GAPS.md` §3 is stale against the code. WS2 is therefore mostly *verification KATs pinning already-landed behavior* (extending the live `HostedSurface` harness in `crates/rustty-apprt-c/tests/macos_window_host.rs`), plus **one real production change**: the `rustty_os` autoreleasepool shim wired into the renderer thread's `drainMailbox` (closing the W3 deferral at `renderer_seam.rs:1115`). Oracle-ownership for cross-DPI is resolved: **the Swift host owns the re-stamp** (no renderer mailbox message exists in the oracle), so no Metal `set_content_scale` message is added — adding one would be an R1 deviation.

**Tech Stack:** Rust 1.93.1 (pinned), objc2 0.6 family (objc2-core-foundation / objc2-io-surface / objc2-foundation 0.3), libtest KATs against the live C-ABI (`rustty_*` exports), ghostty oracle at `../ghostty` (pinned `4749c4e`).

## Global Constraints

(Inherited verbatim from `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`.)

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e`); the oracle is reference-only — no build-time dependency on it (R3).
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty); `ghostty_*` C-ABI aliases stay.
- R4/R5: acyclic crate graph; gates (`.port/tools/gates/check_acyclic.sh`, A4 ABI gate, Miri lanes) are invariant — never weakened to pass.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI.
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Ledger discipline: strike `.port/phase-2/DEFERRALS.md` / `MACOS_APP_GAPS.md` entries in the same PR (Task 7).
- Commit style: `subsystem: summary (chunk-ref)`; branch per WS; merge to main only with both platforms green (`cargo build --workspace` on macOS AND `ssh rvm 'cd ~/projects/rustty && cargo build --workspace'`).
- All new production code in this WS is macOS-only behavior, but every change must keep non-macOS targets (Linux VM, wasm32) compiling — the shim is cross-platform pass-through by construction.

---

## Research findings this plan is built on (verified 2026-07-23)

**A) Quicklook ledger conflict — RESOLVED: implemented, NOT aborting.**

| Check | Where | Finding |
|---|---|---|
| `rustty_surface_quicklook_font` | `crates/rustty-apprt-c/src/lib.rs:2593-2710` | REAL implementation (P2.2): reads `font_grid` primary CoreText face, `attribute(kCTFontSizeAttribute)`, `copy_with_attributes(size/content_scale.y)`, returns +1 CTFontRef via `CFRetained::into_raw`. Zero `unimplemented!()`. |
| `rustty_surface_quicklook_word` | `crates/rustty-apprt-c/src/lib.rs:2732-2836` | REAL implementation (P2.1): `renderer_state.mutex` lock → `get_cursor_pos` → `posToViewport` → `pages.pin` → `select_word` → `readTextLocked`, explicit unlock at every exit. Zero `unimplemented!()`. |
| Focused unit tests | `crates/rustty-apprt-c/src/lib.rs:3054` (`quicklook_word_tests`, 2 tests), `:3149` (`quicklook_font_tests`, 1 test) | Exist and pass. |
| Ledger | `.port/phase-2/DEFERRALS.md:4567-4614` (P2 crash-guard section) | Records "quicklook_word: DONE in P2.1 … quicklook_font: DONE in P2.2". |
| Ledger | `.port/phase-2/MACOS_APP_GAPS.md` §3 HIGH rows (lines 57-61) | **STALE** — still lists both as abort risks at old line numbers 2628/2688. |

**Decision table** (Task 1 re-runs the checks; this row is the branch taken):

| State found | Action |
|---|---|
| Real impls + tests present (the verified state) | Branch **A-implemented**: Task 2 adds the missing *end-to-end* regression KAT (live surface through the actual exports); Task 7 strikes the stale §3 rows. |
| `unimplemented!()` found (regression on the executor's checkout only) | Branch **A-aborting** (contingency): `git log --oneline -- crates/rustty-apprt-c/src/lib.rs | grep -iE 'P2\.[12]|quicklook'` and restore the P2.1/P2.2 commits; if truly absent, port from the oracle `../ghostty/src/apprt/embedded.zig:2140-2250` (`ghostty_surface_quicklook_font` / `ghostty_surface_quicklook_word`) + `../ghostty/src/Surface.zig` (`selectWord`/`readTextLocked` seams) — the bodies to produce are exactly the ones quoted verbatim as `//`-comments above each export in `lib.rs:2583-2836`. |

**B) Cross-DPI contentsScale — ORACLE OWNERSHIP RESOLVED: the HOST owns the re-stamp.**
- Oracle: `../ghostty/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:836-872` — `viewDidChangeBackingProperties` sets `layer?.contentsScale = window.backingScaleFactor` (inside a `CATransaction` with actions disabled), then calls `ghostty_surface_set_content_scale(surface, xScale, yScale)` and `setSurfaceSize(backing size)`. On monitor move, `windowDidChangeScreen` re-triggers it (`SurfaceView_AppKit.swift:780-795`).
- Oracle renderer: `grep content_scale ../ghostty/src/renderer/{message.zig,Thread.zig,generic.zig}` → **no matches**. There is NO renderer mailbox content-scale message; `Metal.zig:218` just *reads* `layer.contentsScale` in `surfaceSize()`. So the "add a Metal `set_content_scale` mailbox message" idea in MACOS_APP_GAPS §3 MEDIUM is the *wrong* branch — R1 forbids inventing it.
- Rustty side is ALREADY the faithful port, end to end: Swift `macos/Sources/RusttyKit/Surface View/SurfaceView_AppKit.swift:837-871` (re-stamp + `rustty_surface_set_content_scale`), export `crates/rustty-apprt-c/src/lib.rs:1563`, `Surface::updateContentScale` `crates/rustty-apprt/src/embedded.rs:1436`, `Surface::contentScaleCallback` `crates/rustty-core/src/surface.rs:7852` (NOT :7721 — the gaps doc's line ref is stale), Metal `surface_size` reading `bounds × contentsScale` live at `crates/rustty-renderer/src/metal/metal_impl.rs:751-768`.
- Remaining work: a KAT proving the chain (Task 3) + ledger truth (Task 7). **No production code.**

**C) drainMailbox autoreleasepool — real work.** `crates/rustty-core/src/renderer_seam.rs:1104-1345` `drainMailbox` carries the W3 deferred no-op pool scope (comment at :1107-1119; DEFERRALS entry at `.port/phase-2/DEFERRALS.md:3552-3561`, which itself names the fix: "a `rustty_os` autoreleasepool shim"). `rustty-core` already deps `rustty-os` (`crates/rustty-core/Cargo.toml:31`); `rustty-os` already has a macOS-gated `objc2 = "0.6"` dep and a direct-`objc2::rc::autoreleasepool` precedent (`crates/rustty-os/src/locale.rs:117`). Oracle scope: the pool exists ONLY in `../ghostty/src/renderer/Thread.zig:338-346` (grep confirms termio/Thread.zig and cf_release_thread.zig have none) — so wire exactly one site.

**D) B2–B4 KATs — reshaped by reality.** `crates/rustty-apprt-c/tests/macos_window_host.rs` (NOT rustty-renderer/tests): `hosts_view_layer_and_presents_live_frame` was ALREADY un-ignored (P0.1, 2026-07-12) and `set_display_id_pushes_to_render_thread_panic_free` un-ignored (M4, 2026-07-10). The only remaining `#[ignore]` is `shows_visible_window_on_device` (:496-562, real NSWindow — needs a WindowServer + the process main thread). B2 (present-error propagation) is code-verified at `metal_impl.rs:815-829` (`Some(layer)` arm maps `iosurface_layer::Error::ObjCFailed` → `Error::ObjCFailed` and `?`-propagates) with the async-completion catch+log at `crates/rustty-renderer/src/generic.rs:5188-5199` and the sync `present_last_target()?` propagation at `generic.rs:5525` — matching the oracle `Metal.zig:253-259` `try` + `generic.zig` catch/log split. B4 (resize tracking) and the harness run get new KATs (Tasks 4, 5).

**Harness facts** (for Tasks 2-4): `HostedSurface::spawn()` (`macos_window_host.rs:228-259`) stands up a REAL app + surface + render thread over a `MockHostView` at `scale_factor = 2.0`, layer sized to 128×64 pt by `size_host_layer()` ⇒ first presented IOSurface is **256×128 px**. `pump_bounded()` re-sets bounds to 128×64 each call — resize/DPI tests must use their own pump loop. Tests may skip (`return`) when Metal is unavailable — keep that guard in every new test. `Text` is `rustty_apprt_c::capi::Text` (`capi.rs:177-190`, all-pub fields, size 40/align 8), freed via `rustty_surface_free_text` (`lib.rs:1390`).

## File structure (locked)

- `crates/rustty-apprt-c/tests/macos_window_host.rs` — all new KATs + 4 small helpers on `HostedSurface` (Tasks 2-4). One file: it owns the only live-surface C-ABI harness.
- `crates/rustty-apprt-c/Cargo.toml` — one macOS dev-dep line (`objc2-core-foundation`) for Task 2.
- `crates/rustty-os/src/autorelease.rs` (new) + `crates/rustty-os/src/lib.rs` (one `pub mod` line) — Task 6 shim.
- `crates/rustty-core/src/renderer_seam.rs:1104-1121` — Task 6 wiring (wrapper + body hoist; the 220-line drain body itself is untouched).
- `.port/phase-2/MACOS_APP_GAPS.md`, `.port/phase-2/DEFERRALS.md` — Task 7 ledger truth.

---

### Task 1: Branch + WS2-A verification (decision table gate)

**Files:**
- No file changes. Read-only verification + branch creation.

**Interfaces:**
- Consumes: nothing.
- Produces: the branch `ws2-macos-guards-plumbing`; a recorded decision (**A-implemented** vs **A-aborting**) that Task 2 consumes.

- [ ] **Step 1: Create the WS branch**

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull
git checkout -b ws2-macos-guards-plumbing
```

- [ ] **Step 2: Verify the quicklook exports are real (no abort paths)**

Run:
```bash
awk 'NR>=2593 && NR<=2836' crates/rustty-apprt-c/src/lib.rs | grep -c "unimplemented!\|todo!"
grep -n "pub unsafe extern \"C\" fn rustty_surface_quicklook" crates/rustty-apprt-c/src/lib.rs
grep -rn "quicklook" crates/rustty-apprt/src/embedded.rs
```
Expected: `0` from the first command (no panic macros in either export body); the second shows both exports at ~:2595 and ~:2734; the third shows only the `:696` doc-comment (the embedded backend itself has no quicklook leaf — the exports live wholly in apprt-c, faithful to the oracle where both are `Darwin` C-API fns in `embedded.zig`, not backend methods).

- [ ] **Step 3: Run the existing focused quicklook unit tests**

Run: `cargo test -p rustty-apprt-c --lib quicklook`
Expected: PASS — 3 tests (`quicklook_word_tests::quicklook_word_selects_word_under_cursor`, `quicklook_word_tests::quicklook_word_empty_cell_yields_no_word`, `quicklook_font_tests::quicklook_font_handoff_is_plus_one_the_host_releases`).

- [ ] **Step 4: Take the decision-table branch**

If Steps 2-3 match Expected (the state verified at plan-authoring time): record **A-implemented** — proceed to Task 2 as written; the MACOS_APP_GAPS §3 HIGH quicklook rows are confirmed stale (struck in Task 7).

Contingency (**A-aborting** — only if the checkout differs from the verified state): find and restore the landing commits with `git log --oneline -- crates/rustty-apprt-c/src/lib.rs | grep -iE 'P2\.[12]|quicklook'`; if genuinely absent, implement both exports from the verbatim reference bodies already present as `//`-comments interleaved at `lib.rs:2583-2836` (oracle: `../ghostty/src/apprt/embedded.zig:2140-2250`), then re-run Step 3 until green. Do not proceed to Task 2 until Step 2 reports `0`.

- [ ] **Step 5: Commit (branch marker only — no changes expected on the verified path)**

No commit if nothing changed. If the contingency path modified code:
```bash
git add crates/rustty-apprt-c/src/lib.rs
git commit -m "rustty-apprt-c: restore quicklook_word/font real impls (WS2-A)"
```

### Task 2: WS2-A end-to-end quicklook regression KAT (live surface, real exports)

The P2.1/P2.2 unit tests exercise the *bodies'* logic against a bare vt `Screen` / standalone CTFont. What no test pins yet — and what MACOS_APP_GAPS §3 feared — is the **actual exports on a live surface under panic=abort-style scrutiny** (Look-Up + VoiceOver fire them without user intent). Add that KAT.

**Files:**
- Modify: `crates/rustty-apprt-c/Cargo.toml:148` (add one dev-dep line after `objc2-io-surface`)
- Modify: `crates/rustty-apprt-c/tests/macos_window_host.rs:65-69` (imports) and append one test after `set_display_id_pushes_to_render_thread_panic_free` (:485)

**Interfaces:**
- Consumes: `HostedSurface::spawn() -> Option<HostedSurface>`, `pump_bounded(&self)` (existing harness); `rustty_surface_quicklook_font(*mut Surface) -> *mut c_void`, `rustty_surface_quicklook_word(*mut Surface, *mut Text) -> bool`, `rustty_surface_free_text(*mut Surface, *mut Text)` (existing exports); `rustty_apprt_c::capi::Text` (pub fields).
- Produces: test `quicklook_exports_are_abort_free_on_live_surface`.

- [ ] **Step 1: Add the objc2-core-foundation dev-dep**

In `crates/rustty-apprt-c/Cargo.toml`, under `[target.'cfg(target_os = "macos")'.dev-dependencies]`, after the `objc2-io-surface` line add:

```toml
# WS2-A quicklook regression KAT: CFRetained::from_raw to balance the +1 CTFontRef
# the quicklook_font export hands the host (I16). Same 0.3 family as the rest.
objc2-core-foundation = { version = "0.3", default-features = false, features = ["CFBase"] }
```

- [ ] **Step 2: Extend the test-file import block**

Replace `macos_window_host.rs:65-69`:

```rust
use rustty_apprt_c::{
    rustty_app_free, rustty_app_new, rustty_app_tick, rustty_app_update_config, rustty_init,
    rustty_set_window_background_blur, rustty_surface_config_new, rustty_surface_free,
    rustty_surface_new, rustty_surface_set_display_id,
};
```

with:

```rust
use rustty_apprt_c::capi::Text;
use rustty_apprt_c::{
    rustty_app_free, rustty_app_new, rustty_app_tick, rustty_app_update_config, rustty_init,
    rustty_set_window_background_blur, rustty_surface_config_new, rustty_surface_free,
    rustty_surface_free_text, rustty_surface_new, rustty_surface_quicklook_font,
    rustty_surface_quicklook_word, rustty_surface_set_content_scale,
    rustty_surface_set_display_id, rustty_surface_set_size,
};
```

(`set_content_scale`/`set_size` are for Tasks 3-4; importing once avoids churn.)

- [ ] **Step 3: Write the failing-or-passing regression KAT**

Append to `macos_window_host.rs` (after `set_display_id_pushes_to_render_thread_panic_free`, before the on-device `#[ignore]` block):

```rust
// --- WS2-A regression KAT ---
// The two Look-Up/accessibility exports must be ABORT-FREE on a LIVE surface:
// `quicklook_font` fires from force-click Look-Up AND every VoiceOver/accessibility
// query (SurfaceView_AppKit.swift:1521,1949,2361 in the oracle app), `quicklook_word`
// from the Look-Up gesture (:1512) — no explicit user intent required, so under the
// DG5 panic=abort policy any leftover `unimplemented!()` here is app-fatal.
// MACOS_APP_GAPS.md §3 listed both as open abort risks; the exports were in fact
// fully ported in P2.1/P2.2 (DEFERRALS.md P2 section). This KAT pins that they STAY
// real: it drives the actual C-ABI exports against a live surface (running render
// thread + populated CoreText font grid).
#[test]
fn quicklook_exports_are_abort_free_on_live_surface() {
    use objc2_core_foundation::{CFRetained, CFType};

    let Some(host) = HostedSurface::spawn() else {
        eprintln!("no Metal device / app_new failed; skipping");
        return;
    };
    host.pump_bounded(); // live render thread; the shared font grid is populated

    // quicklook_font: the live grid's primary CoreText face resolves ⇒ a +1
    // CTFontRef crosses the ABI (I16); the host balances it with exactly ONE
    // CFRelease — modeled as CFRetained::from_raw + drop (the P2.2 KAT's inverse).
    // SAFETY: `host.surface` is the live surface from `rustty_surface_new`.
    let font = unsafe { rustty_surface_quicklook_font(host.surface) };
    assert!(
        !font.is_null(),
        "primary CoreText face must resolve on a live surface"
    );
    // SAFETY: `font` is the +1 CTFontRef the export handed over; reclaiming it
    // consumes exactly that one reference (the host's CFRelease).
    drop(unsafe {
        CFRetained::<CFType>::from_raw(std::ptr::NonNull::new(font.cast::<CFType>()).unwrap())
    });

    // quicklook_word: MUST return without abort. `true` ⇒ `text` owns a heap
    // C-string freed via rustty_surface_free_text; `false` (no word under the
    // mouse-cursor cell — the harness never moved the mouse, so (0,0) is
    // typically empty) is a VALID no-word degrade. The assertion IS abort-freedom.
    let mut text = Text {
        tl_px_x: 0.0,
        tl_px_y: 0.0,
        offset_start: 0,
        offset_len: 0,
        text: core::ptr::null(),
        text_len: 0,
    };
    // SAFETY: `host.surface` is live; `text` is a writable Text for the call.
    let has_word = unsafe { rustty_surface_quicklook_word(host.surface, &mut text) };
    if has_word {
        assert!(!text.text.is_null(), "true return must populate the text");
        // SAFETY: `text` was populated by the export just above; freed exactly once.
        unsafe { rustty_surface_free_text(host.surface, &mut text) };
    }
}
```

- [ ] **Step 4: Run the KAT**

Run: `cargo test -p rustty-apprt-c --test macos_window_host quicklook_exports_are_abort_free_on_live_surface`
Expected: PASS (this is a regression KAT over already-landed code). If it aborts or panics: that IS the §3 gap being real on this checkout — apply Task 1 Step 4's contingency (restore/port per the oracle), using superpowers:systematic-debugging; do not null-guard around a genuine failure without understanding it.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-apprt-c/Cargo.toml crates/rustty-apprt-c/tests/macos_window_host.rs
git commit -m "rustty-apprt-c: live-surface quicklook abort-free regression KAT (WS2-A)"
```

### Task 3: WS2-B cross-DPI contentsScale KAT (+ shared harness helpers)

No production change (see Research findings B: host-owned per oracle; rustty's Swift host + full chain already faithful). This task adds the KAT that drives the exact host sequence and 4 small helpers Task 4 reuses.

**Files:**
- Modify: `crates/rustty-apprt-c/tests/macos_window_host.rs` — add 4 methods inside `impl HostedSurface` (insert after `read_presented_bgra`, i.e. just before the closing `}` of the impl at :384) + 1 test.

**Interfaces:**
- Consumes: existing `HostedSurface` fields/methods (`view: Retained<MockHostView>`, `app`, `surface`, `drive_display`, `pump_bounded`); `rustty_surface_set_content_scale`, `rustty_surface_set_size` (imported in Task 2).
- Produces (Task 4 consumes these exact signatures):
  - `fn presented_size(&self) -> Option<(usize, usize)>`
  - `fn set_host_layer_bounds(&self, w_pt: f64, h_pt: f64)`
  - `fn set_host_layer_contents_scale(&self, scale: f64)`
  - `fn pump_until_presented_size(&self, expected: (usize, usize)) -> Option<(usize, usize)>`

- [ ] **Step 1: Add the harness helpers**

Insert into `impl HostedSurface` (after `read_presented_bgra`):

```rust
    // The presented IOSurface's pixel dimensions (None until a frame presented).
    fn presented_size(&self) -> Option<(usize, usize)> {
        use objc2_io_surface::IOSurfaceRef;
        // SAFETY: `self.view` is a live MockHostView answering `layer`.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        if l.is_null() {
            return None;
        }
        // SAFETY: `l` is the live layer id; `contents` returns nil or the IOSurface id.
        let contents: *mut AnyObject = unsafe { msg_send![l, contents] };
        if contents.is_null() {
            return None;
        }
        // SAFETY: `contents` IS the presented IOSurfaceRef (see read_presented_bgra).
        let surface: &IOSurfaceRef = unsafe { &*(contents as *const IOSurfaceRef) };
        Some((surface.width(), surface.height()))
    }

    // AppKit-layout stand-in for a RESIZE: re-lay the hosted layer to the new view
    // bounds (points). A real layer-hosting NSView does this on live resize.
    fn set_host_layer_bounds(&self, w_pt: f64, h_pt: f64) {
        use objc2_foundation::{NSPoint, NSRect, NSSize};
        // SAFETY: `self.view` is a live MockHostView answering `layer`.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        if l.is_null() {
            return;
        }
        let bounds = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w_pt, h_pt));
        // SAFETY: `l` is the live CALayer; `setBounds:` takes a CGRect (== NSRect).
        unsafe {
            let _: () = msg_send![l, setBounds: bounds];
        }
    }

    // Swift-host stand-in for a CROSS-DPI MONITOR MOVE: re-stamp the hosted layer's
    // contentsScale exactly as `viewDidChangeBackingProperties` does
    // (`layer?.contentsScale = window.backingScaleFactor`,
    // macos/Sources/RusttyKit/Surface View/SurfaceView_AppKit.swift:857).
    fn set_host_layer_contents_scale(&self, scale: f64) {
        // SAFETY: `self.view` is a live MockHostView answering `layer`.
        let l: *mut AnyObject = unsafe { msg_send![&*self.view, layer] };
        if l.is_null() {
            return;
        }
        // SAFETY: `l` is the live CALayer; `setContentsScale:` takes a CGFloat.
        unsafe {
            let _: () = msg_send![l, setContentsScale: scale];
        }
    }

    // Bounded pump until the presented IOSurface reaches `expected` px dimensions.
    // Unlike `pump_bounded`, does NOT touch the layer geometry (the caller owns it —
    // `pump_bounded` would reset bounds to 128×64 and defeat resize/DPI tests).
    fn pump_until_presented_size(&self, expected: (usize, usize)) -> Option<(usize, usize)> {
        let deadline = Instant::now() + Duration::from_secs(5);
        for _ in 0..2000 {
            // SAFETY: `self.app` is a live App pointer for the surface's lifetime.
            unsafe { rustty_app_tick(self.app) };
            self.drive_display();
            let sz = self.presented_size();
            if sz == Some(expected) {
                return sz;
            }
            if Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        self.presented_size()
    }
```

- [ ] **Step 2: Write the cross-DPI KAT**

Append after `quicklook_exports_are_abort_free_on_live_surface`:

```rust
// --- WS2-B cross-DPI KAT ---
// ORACLE OWNERSHIP (resolved, ghostty@4749c4e): the HOST owns the contentsScale
// re-stamp on a cross-DPI monitor move — `viewDidChangeBackingProperties` sets
// `layer.contentsScale = backingScaleFactor`, then calls
// `ghostty_surface_set_content_scale` + sends the new backing size
// (macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:836-872; re-driven
// on `windowDidChangeScreen`, :780-795). NO renderer mailbox content-scale message
// exists in the oracle (src/renderer/message.zig has none) — the renderer just
// READS `layer.contentsScale` in `surfaceSize()` (Metal.zig:217-218 ==
// metal_impl.rs `surface_size`). Rustty's Swift host is the faithful port
// (SurfaceView_AppKit.swift:837-871), so this KAT drives that exact host sequence
// through the C-ABI and asserts the presented frame follows: Retina (2.0) →
// non-Retina (1.0) at constant 128×64 pt must halve the pixel dimensions.
#[test]
fn content_scale_restamp_tracks_cross_dpi_move() {
    let Some(host) = HostedSurface::spawn() else {
        eprintln!("no Metal device / app_new failed; skipping");
        return;
    };
    host.pump_bounded();
    assert_eq!(
        host.presented_size(),
        Some((256, 128)),
        "first frame must be 128×64 pt @2x"
    );

    // The monitor move to a 1x display, exactly as the Swift host performs it:
    // (1) re-stamp the layer scale, (2) update the core content scale (font DPI),
    // (3) send the new backing pixel size.
    host.set_host_layer_contents_scale(1.0);
    // SAFETY: `host.surface` is live for both calls.
    unsafe { rustty_surface_set_content_scale(host.surface, 1.0, 1.0) };
    unsafe { rustty_surface_set_size(host.surface, 128, 64) };
    let sz = host.pump_until_presented_size((128, 64));
    assert_eq!(
        sz,
        Some((128, 64)),
        "presented IOSurface must track the re-stamped contentsScale (no blurry \
         stale-scale frame)"
    );
}
```

- [ ] **Step 3: Run the KAT**

Run: `cargo test -p rustty-apprt-c --test macos_window_host content_scale_restamp_tracks_cross_dpi_move`
Expected: PASS. If the size never converges to 128×64: the chain broke somewhere between `rustty_surface_set_content_scale` (`lib.rs:1563`) → `updateContentScale` (`embedded.rs:1436`) → `contentScaleCallback` (`surface.rs:7852`, DPI/font-size recompute + `resize`) → Metal `surface_size` (`metal_impl.rs:751`) — debug with superpowers:systematic-debugging along those four stations; the fix must stay host-owned (no renderer mailbox message — R1).

- [ ] **Step 4: Commit**

```bash
git add crates/rustty-apprt-c/tests/macos_window_host.rs
git commit -m "rustty-apprt-c: cross-DPI contentsScale re-stamp KAT + size helpers (WS2-B)"
```

### Task 4: WS2-D B4 resize-tracking KAT + B2 present-error verification

**Files:**
- Modify: `crates/rustty-apprt-c/tests/macos_window_host.rs` — 1 new test.
- Read-only verify: `crates/rustty-renderer/src/metal/metal_impl.rs:799-830`, `crates/rustty-renderer/src/generic.rs:5188-5199,5520-5530`.

**Interfaces:**
- Consumes: Task 3 helpers (`presented_size`, `set_host_layer_bounds`, `pump_until_presented_size`); `rustty_surface_set_size` (imported in Task 2).
- Produces: test `resize_tracks_host_layer_bounds`; a B2 verification record for the Task 7 ledger note.

- [ ] **Step 1: B2 — verify present-error propagation (read, no code)**

Run and read:
```bash
sed -n '799,835p' crates/rustty-renderer/src/metal/metal_impl.rs
sed -n '5185,5200p' crates/rustty-renderer/src/generic.rs
sed -n '5518,5530p' crates/rustty-renderer/src/generic.rs
```
Confirm (all three were verified at plan time — this re-run is the executor's evidence):
1. `Metal::present` `Some(layer)` async arm: `layer.set_surface(...).map_err(|iosurface_layer::Error::ObjCFailed| Error::ObjCFailed)?` — the error PROPAGATES (no `Ok(())` swallow on the live arm; the `None => Ok(())` no-op is offscreen-only).
2. `generic.rs:5195` (async buffer-completed path): present error is caught + logged — the oracle's `catch |err| log` parity.
3. `generic.rs:5525`: sync `present_last_target()?` propagates via `?`.
Record the three citations for Task 7's B2 strike note. If any arm differs, STOP and fix to match `../ghostty/src/renderer/Metal.zig:253-259` + the generic.zig caller before proceeding.

- [ ] **Step 2: B4 — write the resize-tracking KAT**

Append after `content_scale_restamp_tracks_cross_dpi_move`:

```rust
// --- WS2-D B4 KAT ---
// MACOS_APP_GAPS §2 B4: post-attach, `surfaceSize()` = layer.bounds ×
// contentsScale must FOLLOW a live resize. AppKit re-lays a layer-hosting view's
// layer on resize; the harness stands in (set_host_layer_bounds), then performs
// the Swift host's resize handshake (`setSurfaceSize` → rustty_surface_set_size
// with the BACKING pixel size, SurfaceView_AppKit.swift:869-871). The
// IOSurfaceLayer discards wrong-sized surfaces (IOSurfaceLayer.zig:104-116 ==
// iosurface_layer.rs), so convergence to the new size proves the whole reflow:
// core resize → renderer rebuild → present at bounds × scale.
#[test]
fn resize_tracks_host_layer_bounds() {
    let Some(host) = HostedSurface::spawn() else {
        eprintln!("no Metal device / app_new failed; skipping");
        return;
    };
    host.pump_bounded();
    assert!(host.layer_has_contents(), "no first frame presented");
    assert_eq!(
        host.presented_size(),
        Some((256, 128)),
        "first frame must be 128×64 pt @2x"
    );

    // Live resize to 200×100 pt (@2x ⇒ 400×200 px backing size).
    host.set_host_layer_bounds(200.0, 100.0);
    // SAFETY: `host.surface` is live.
    unsafe { rustty_surface_set_size(host.surface, 400, 200) };
    let sz = host.pump_until_presented_size((400, 200));
    assert_eq!(
        sz,
        Some((400, 200)),
        "presented IOSurface must track the layer resize (B4 reflow)"
    );
}
```

- [ ] **Step 3: Run the KAT**

Run: `cargo test -p rustty-apprt-c --test macos_window_host resize_tracks_host_layer_bounds`
Expected: PASS. If the presented size stays 256×128: the reflow chain (`rustty_surface_set_size` → `updateSize` `embedded.rs:1464` → core `resize` → renderer `Resize` message → `surface_size` re-read) broke — systematic-debugging along those stations.

- [ ] **Step 4: Commit**

```bash
git add crates/rustty-apprt-c/tests/macos_window_host.rs
git commit -m "rustty-apprt-c: B4 resize-tracking KAT; B2 present-propagation verified (WS2-D)"
```

### Task 5: WS2-D full harness run on this Mac + the on-device `#[ignore]` window test

**Files:**
- Possibly modify: `crates/rustty-apprt-c/tests/macos_window_host.rs:496-497` (the `#[ignore]` attribute — decision-gated below).

**Interfaces:**
- Consumes: all tests from Tasks 2-4 + the 3 pre-existing tests.
- Produces: a green full-suite run; an un-ignore decision for `shows_visible_window_on_device`; run records for Task 7.

- [ ] **Step 1: Run the full headless suite**

Run: `cargo test -p rustty-apprt-c --test macos_window_host`
Expected: `6 passed; 0 failed; 1 ignored` (hosts_view_layer_and_presents_live_frame, set_window_background_blur…, set_display_id…, quicklook_exports…, content_scale_restamp…, resize_tracks…; `shows_visible_window_on_device` ignored). Any failure: systematic-debugging; the suite must be fully green before Step 2.

- [ ] **Step 2: Run the on-device visible-window test (live display available on this Mac)**

Run: `cargo test -p rustty-apprt-c --test macos_window_host -- --ignored --test-threads=1`
Two possible outcomes (both are data, not failures):
- **Main thread acquired** (single-threaded libtest runs on the process main thread in a GUI login session): a real 200×120 NSWindow appears, the pump runs ~5s, blur is applied via the real CGS SPI, test PASSES.
- **`MainThreadMarker::new()` is `None`** (libtest still used a worker thread): the test prints its skip note and passes vacuously. In that case retry once with the env harness: `RUST_TEST_THREADS=1 cargo test -p rustty-apprt-c --test macos_window_host -- --ignored`. If still off-main-thread, the vacuous pass stands.

- [ ] **Step 3: Un-ignore decision**

Rule (the roadmap's "un-ignore what passes", applied honestly):
- If Step 2 achieved the REAL window path and passed: remove the `#[ignore = …]` attribute (`macos_window_host.rs:497`) — the test is safe un-ignored because its off-main-thread arm self-skips with a note (`:503-509`), so default parallel runs and the Linux VM (file is `#![cfg(target_os = "macos")]`) are unaffected. Update the test's doc comment (`:487-495`) to say it runs un-ignored, self-skipping off the main thread, and was live-verified 2026-07-23 on this Mac.
- If only the vacuous skip was reachable: KEEP `#[ignore]` (an always-vacuous "passing" test is worse than an honest ignore) and record in Task 7 that the on-device path remains manual-only, with the Step 2 command as the runbook.

- [ ] **Step 4: Re-run the suite + commit (only if Step 3 changed the file)**

Run: `cargo test -p rustty-apprt-c --test macos_window_host`
Expected: all pass (`7 passed` if un-ignored, else `6 passed; 1 ignored`).

```bash
git add crates/rustty-apprt-c/tests/macos_window_host.rs
git commit -m "rustty-apprt-c: un-ignore on-device window KAT after live run (WS2-D)"
```

### Task 6: WS2-C `rustty_os` autoreleasepool shim + `drainMailbox` wiring (TDD)

The one production change. Oracle scope: `../ghostty/src/renderer/Thread.zig:338-346` — the ONLY AutoreleasePool site (verified: termio/Thread.zig and os/cf_release_thread.zig have none), so exactly one call site is wired.

**Files:**
- Create: `crates/rustty-os/src/autorelease.rs`
- Modify: `crates/rustty-os/src/lib.rs` (one `pub mod` line, after `pub mod args;` at :38)
- Modify: `crates/rustty-core/src/renderer_seam.rs:1104-1121` (wrapper + body hoist)
- Modify: `.port/phase-2/DEFERRALS.md:3552-3561` (RESOLVED note — same-commit ledger discipline)

**Interfaces:**
- Consumes: `objc2::rc::autoreleasepool` (rustty-os's existing macOS-gated `objc2 = "0.6"` dep, `Cargo.toml:63`; direct-use precedent `locale.rs:117`). `rustty-core` already deps `rustty-os` unconditionally (`crates/rustty-core/Cargo.toml:31`) — NO Cargo.toml change anywhere.
- Produces: `pub fn rustty_os::autorelease::autoreleasepool<T, F: FnOnce() -> T>(f: F) -> T` (macOS: real ObjC pool; every other target incl. wasm32: identity — so this compiles everywhere, which is what the rvm gate confirms).

- [ ] **Step 1: Write the failing tests (new module file, tests first)**

Create `crates/rustty-os/src/autorelease.rs`:

````rust
//! Autorelease-pool shim.
//!
//! PORTS: the reference's vendored `objc` package `AutoreleasePool` as consumed by
//! `renderer/Thread.zig:338-346` (`drainMailbox`):
//!
//! ```zig
//! const pool = if (builtin.os.tag.isDarwin())
//!     @import("objc").AutoreleasePool.init()
//! else
//!     void;
//! defer if (builtin.os.tag.isDarwin()) pool.deinit();
//! ```
//!
//! On macOS this is a thin wrapper over `objc2::rc::autoreleasepool` — the SAME
//! Objective-C runtime pool (R3: a binding, not a reimplementation; the
//! `os/locale.rs:117` precedent). On every other target it is the identity, the
//! runtime analogue of the reference's comptime `void` arm — so the module is
//! unconditionally compiled (incl. wasm32) and callers need no cfg.

/// Run `f` inside an Objective-C autorelease pool (macOS); plain call elsewhere.
/// The closure-exit IS the reference `defer pool.deinit()`.
#[cfg(target_os = "macos")]
#[inline]
pub fn autoreleasepool<T, F: FnOnce() -> T>(f: F) -> T {
    objc2::rc::autoreleasepool(|_pool| f())
}

/// Non-Darwin arm: the reference comptime-selects `void` — identity.
#[cfg(not(target_os = "macos"))]
#[inline]
pub fn autoreleasepool<T, F: FnOnce() -> T>(f: F) -> T {
    f()
}

#[cfg(test)]
mod tests {
    // Cross-platform: the shim is value-transparent.
    #[test]
    fn returns_closure_value() {
        assert_eq!(super::autoreleasepool(|| 7), 7);
    }

    // macOS: the pool actually DRAINS — an object autoreleased inside the scope
    // is released when the closure exits (the memory-hygiene property the W3
    // deferral punted; a no-op shim would leave the +1 alive and fail this).
    #[cfg(target_os = "macos")]
    #[test]
    fn pool_drains_autoreleased_references() {
        use objc2::msg_send;
        use objc2::rc::Retained;
        use objc2::runtime::NSObject;

        fn retain_count(obj: &NSObject) -> usize {
            // SAFETY: `retainCount` is a plain NSUInteger-returning selector.
            unsafe { msg_send![obj, retainCount] }
        }

        let obj = NSObject::new();
        let baseline = retain_count(&obj);
        super::autoreleasepool(|| {
            // Clone (+1) and hand that reference to the ambient pool.
            let extra: Retained<NSObject> = obj.clone();
            let raw = Retained::into_raw(extra);
            // SAFETY: `raw` is a live +1 NSObject; `autorelease` transfers that
            // +1 to the innermost (our) pool and returns the object id.
            let _: *mut NSObject = unsafe { msg_send![raw, autorelease] };
            assert_eq!(retain_count(&obj), baseline + 1, "autoreleased +1 pending");
        });
        // Pool drained on scope exit: the pending +1 was released.
        assert_eq!(retain_count(&obj), baseline, "pool must drain on closure exit");
    }
}
````

Register it in `crates/rustty-os/src/lib.rs` — after the line `pub mod args;` (:38) add:

```rust
pub mod autorelease;
```

- [ ] **Step 2: Run the shim tests**

Run: `cargo test -p rustty-os autorelease`
Expected: PASS — 2 tests (`returns_closure_value`, `pool_drains_autoreleased_references`). (The "failing first" phase here is the pre-file state: without Step 1 the module doesn't exist; both tests and impl land in one file by construction — the drain test is the meaningful gate, and it would fail against a no-op macOS arm.)

- [ ] **Step 3: Wire `drainMailbox` (wrapper + body hoist)**

In `crates/rustty-core/src/renderer_seam.rs`, replace lines 1102-1121 exactly:

```rust
    // /// Drain the mailbox.
    // fn drainMailbox(self: *Thread) !void {
    fn drainMailbox(self_: *mut Thread<A>) -> Result<(), RendererError> {
        // There's probably a more elegant way to do this...
        //
        // This is effectively an @autoreleasepool{} block, which we need in
        // order to ensure that autoreleased objects are properly released.
        // const pool = if (builtin.os.tag.isDarwin())
        //     @import("objc").AutoreleasePool.init()
        // else
        //     void;
        // defer if (builtin.os.tag.isDarwin()) pool.deinit();
        //
        // DEFERRED (W3 renderer-Thread leaf; .port/phase-2/DEFERRALS.md): rustty-core
        // does NOT dep objc2 (only rustty-renderer/os do), so this Darwin
        // memory-hygiene autoreleasepool (not semantics) is ported as a no-op
        // scope carrying the verbatim reference block above. Re-arm when core
        // gains an objc2 edge (or via a `rustty_os` autoreleasepool shim).

        // while (self.mailbox.pop()) |message| {
```

with:

```rust
    // /// Drain the mailbox.
    // fn drainMailbox(self: *Thread) !void {
    fn drainMailbox(self_: *mut Thread<A>) -> Result<(), RendererError> {
        // There's probably a more elegant way to do this...
        //
        // This is effectively an @autoreleasepool{} block, which we need in
        // order to ensure that autoreleased objects are properly released.
        // const pool = if (builtin.os.tag.isDarwin())
        //     @import("objc").AutoreleasePool.init()
        // else
        //     void;
        // defer if (builtin.os.tag.isDarwin()) pool.deinit();
        //
        // REALIZED (WS2-C, closes the W3 deferral): the pool is live via the
        // `rustty_os::autorelease::autoreleasepool` shim (objc2-backed on macOS,
        // identity elsewhere — the exact re-arm path the deferral named; core
        // still has NO objc2 edge). The drain body is hoisted to
        // `drainMailboxBody` so the pool scopes the whole drain; the closure
        // exit IS the reference `defer pool.deinit()`.
        rustty_os::autorelease::autoreleasepool(|| Self::drainMailboxBody(self_))
    }

    /// The reference `drainMailbox` body (see the pool wrapper above).
    fn drainMailboxBody(self_: *mut Thread<A>) -> Result<(), RendererError> {
        // while (self.mailbox.pop()) |message| {
```

Nothing else changes: the 220-line message-match body (`:1122-1345`, ending `Ok(())` + `}`) now closes `drainMailboxBody`; the wrapper keeps the `drainMailbox` name so the caller (`renderCallback`/wakeup path) is untouched.

- [ ] **Step 4: Build + run the touched-crate tests**

Run:
```bash
cargo build -p rustty-core
cargo test -p rustty-core
cargo test -p rustty-apprt-c --test macos_window_host
```
Expected: build green; core tests pass unchanged; the window-host suite (whose every `pump_bounded` tick exercises the wrapped drain on a live render thread) stays green — proving the pooled drain is behavior-neutral.

- [ ] **Step 5: Strike the DEFERRALS entry (same-commit ledger discipline)**

In `.port/phase-2/DEFERRALS.md`, at the end of the W3 autoreleasepool paragraph (after `…the same class as the io-thread's deferred autoreleasepool.`, line 3561), append:

```markdown
**RESOLVED (WS2-C, 2026-07-23):** realized exactly via the named re-arm path — the
`rustty_os::autorelease::autoreleasepool` shim (objc2-backed on macOS, identity
elsewhere; core still has no objc2 edge). `drainMailbox` is now the pool wrapper
around the hoisted `drainMailboxBody`; drain semantics unchanged (rustty-core +
macos_window_host suites green). Pool-drain property pinned by
`rustty-os/src/autorelease.rs` `pool_drains_autoreleased_references`.
```

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-os/src/autorelease.rs crates/rustty-os/src/lib.rs \
        crates/rustty-core/src/renderer_seam.rs .port/phase-2/DEFERRALS.md
git commit -m "rustty-os/core: autoreleasepool shim wired into renderer drainMailbox (WS2-C)"
```

### Task 7: Ledger truth pass — MACOS_APP_GAPS §2/§3

**Files:**
- Modify: `.port/phase-2/MACOS_APP_GAPS.md`

**Interfaces:**
- Consumes: Task 4 Step 1's B2 citations; Task 5's run records; Tasks 2/3 test names.
- Produces: a truthful ledger (roadmap "Ledger corrections" mandate).

- [ ] **Step 1: Strike/annotate the stale and now-verified entries**

Apply these edits to `.port/phase-2/MACOS_APP_GAPS.md`:

1. **§2 B2** (present no-op arm, line ~32-35): append to the entry:
   `**VERIFIED (WS2-D, 2026-07-23):** live-arm propagation confirmed — metal_impl.rs:815-829 maps setSurface's ObjCFailed into Error::ObjCFailed and ?-propagates; generic.rs:5195 catch+logs on the async completion (oracle parity), generic.rs:5525 ?-propagates on the sync path. Presents exercised live by the macos_window_host suite.`
2. **§2 B3** (display callback): append:
   `**VERIFIED (WS2-D, 2026-07-23):** installed at loop_enter with layer=Some (attach precedes thread spawn); the -display-driven present path is exercised every pump tick of the (un-ignored) hosts_view_layer_and_presents_live_frame KAT.`
3. **§2 B4** (surface_size frozen): append:
   `**VERIFIED (WS2-D, 2026-07-23):** resize_tracks_host_layer_bounds KAT — presented IOSurface follows layer.bounds × contentsScale through a live 128×64→200×100 pt resize.`
4. **§3 HIGH quicklook_font / quicklook_word rows** (table lines ~59-60): replace both rows' `Note` cells with:
   `RESOLVED-STALE (WS2-A, 2026-07-23): fully ported in P2.1/P2.2 before this audit's line refs went stale (now lib.rs:2595/2734, zero unimplemented!()); pinned by quicklook_exports_are_abort_free_on_live_surface + the P2.1/P2.2 unit KATs.`
   (Leave the **Inspector subsystem** row untouched — that is WS6.)
5. **§3 MEDIUM contentsScale row** (line ~66): replace the note with:
   `RESOLVED (WS2-B, 2026-07-23): oracle-ownership verified — the HOST owns the re-stamp (SurfaceView_AppKit.swift viewDidChangeBackingProperties: layer.contentsScale + ghostty_surface_set_content_scale + backing-size resend; NO renderer mailbox message exists in the oracle). Rustty's Swift host + chain (lib.rs:1563 → embedded.rs:1436 → surface.rs:7852 (was :7721, ref stale) → metal_impl.rs:751) were already faithful; pinned by content_scale_restamp_tracks_cross_dpi_move. No Metal mailbox message added (would deviate from the oracle).`
6. **§3 LOW drainMailbox bullet** (line ~69): append:
   `— RESOLVED (WS2-C, 2026-07-23) via rustty_os::autorelease::autoreleasepool wired at renderer_seam.rs drainMailbox.`
7. If Task 5 kept `shows_visible_window_on_device` ignored, add one line at the end of §2:
   `On-device visible-window KAT: remains #[ignore] (needs the process main thread); manual runbook: cargo test -p rustty-apprt-c --test macos_window_host -- --ignored --test-threads=1 (run green/skipped 2026-07-23).` — otherwise note it was un-ignored.

- [ ] **Step 2: Commit**

```bash
git add .port/phase-2/MACOS_APP_GAPS.md
git commit -m "port-ledgers: strike WS2 rows in MACOS_APP_GAPS (A/B/C/D resolved+verified)"
```

### Task 8: Both-platform gate, full verification, merge

**Files:**
- No new changes; gates + merge flow only.

**Interfaces:**
- Consumes: everything above.
- Produces: WS2 merged to main, branch deleted, ROADMAP checklist updated.

- [ ] **Step 1: Local full gates (macOS)**

```bash
cd /Users/gael/projects/rustty
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings
.port/tools/gates/check_acyclic.sh
cargo build --workspace
cargo test -p rustty-os && cargo test -p rustty-core && cargo test -p rustty-apprt-c
```
Expected: all green (acyclic gate unchanged — no new crate edges were added anywhere in this WS). Fix anything red before proceeding; gates are invariant (R4/R5) — never weaken them.

- [ ] **Step 2: Remote build gate (Linux VM)**

```bash
git push origin ws2-macos-guards-plumbing
ssh rvm 'cd ~/projects/rustty && git fetch origin && git checkout ws2-macos-guards-plumbing && git pull --ff-only && cargo build --workspace'
```
Expected: green. The only cross-platform-visible change is the `renderer_seam.rs` hoist + the unconditional `autorelease` module (identity arm on Linux) — this gate is precisely the standing lesson (xev `Loop::init` / `COMPLETION_SIZE`) applied.

- [ ] **Step 3: Verification-before-completion + merge**

Use superpowers:verification-before-completion (re-run Step 1's commands and confirm outputs — evidence before claims), then superpowers:finishing-a-development-branch:

```bash
git checkout main
git merge --no-ff ws2-macos-guards-plumbing -m "merge ws2-macos-guards-plumbing: macOS guards & plumbing (WS2)"
git push origin main
git branch -d ws2-macos-guards-plumbing
git push origin --delete ws2-macos-guards-plumbing
```

- [ ] **Step 4: Tick the ROADMAP checklist**

In `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`, change `- [ ] WS2 macOS guards & plumbing` to `- [x] WS2 macOS guards & plumbing`; commit on main:

```bash
git add docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md
git commit -m "docs: tick WS2 in the completion roadmap"
git push origin main
```

---

## Self-review notes (plan-author pass)

- **Spec coverage:** A → Tasks 1-2 (+7); B → Task 3 (+7, oracle-ownership answered: host-owned, no mailbox message); C → Task 6; D → Tasks 4-5 (+7, B2 verified by citation + live suite, B3 by ordering + live suite, B4 by new KAT; the "ignored KAT" from the WS brief was found already un-ignored — the remaining `#[ignore]` on-device test is handled by Task 5's decision rule). Both-platform gate + ledgers + merge → Tasks 7-8.
- **Line-number caveat for executors:** all `file.rs:NNN` refs are against the checkout at plan-authoring (main @ 8d38349). If edits have shifted lines, anchor on the quoted code, not the numbers.
- **Type consistency:** helper signatures in Task 3's Produces block match their uses in Tasks 3-4; the Task 2 import block includes everything Tasks 3-4 call; `Text` field names match `capi.rs:177-190`.
