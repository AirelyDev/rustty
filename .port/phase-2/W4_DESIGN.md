# W4 DESIGN — apprt-c macOS host handshake: blur + display_id + the culminating NSWindow KAT

Chunk **W4** of `docs/superpowers/plans/2026-07-08-macos-windowing-sub1.md` (tasks
W4-1..W4-4). Fills the **two remaining** macOS `#[cfg(target_os="macos")]` C-ABI
leaves in `crates/rustty-apprt-c/src/lib.rs` (window blur, display_id), promotes
the config blur-radius accessor to `pub`, and lands the culminating integration
harness that hands a macOS layer-host view across the published C-ABI
(`platform.macos.nsview`) and drives one live on-screen frame — **no Swift app**.
The headless proof uses a main-thread-agnostic `MockHostView` layer-host double
(renderer precedent); a real `NSWindow`/`NSView` is exercised in the `#[ignore]`
on-device visible-window test.

**Disposition: `ready`.** No genuine blocker. W0–W3 are DONE, so every seam this
chunk consumes is live: the renderer attaches an `IOSurfaceLayer` on-screen
(`Metal::init(view)`), the `renderer_seam::Thread` event loop runs over rustty-xev
and drains its mailbox (including `Message::MacosDisplayId` →
`Renderer::set_macos_display_id`), and `Surface::draw` is live. Both leaves are the
canonical R5 "deferred = `unimplemented!()` + verbatim body" promotion now that
their consumed seams exist. `quicklook`/`secure-input`/`inspector` leaves STAY
deferred (sub-project #6) — do NOT fill.

> **FOLD (2026-07-09) — two adversary blockers resolved in place:**
> 1. **CGS symbols must actually link into the cdylib.** The realized private-CGS
>    `extern "C"` block now carries an explicit
>    `#[link(name = "CoreGraphics", kind = "framework")]` (§2.2) — a self-contained,
>    dep-independent link directive, NOT a transitive dep on the dev-only
>    `objc2-core-graphics`. `cargo build -p rustty-apprt-c` (the cdylib gate) resolves
>    `_CGSSetWindowBackgroundBlurRadius`/`_CGSDefaultConnectionForThread` from
>    `CoreGraphics.framework`. `objc2-core-graphics` stays a **dev-dep only** (harness
>    `CGMainDisplayID`), never the production link path (§1.2, §5).
> 2. **The headless KAT must actually run** (libtest runs `#[test]` bodies on worker
>    threads, so `MainThreadMarker::new()` is `None` there and a real-`NSWindow` test
>    would skip unconditionally). §4 is rebuilt on **Option A**: the three headless
>    tests hand a plain-`NSObject` **`MockHostView` layer-host double** across the
>    C-ABI (renderer `golden_presents_frame_on_screen` precedent) — no
>    `MainThreadMarker`, no AppKit, runs green under default parallel `cargo test`.
>    The real `NSWindow` lives ONLY in the `#[ignore]` on-device visible test. The
>    false "cargo runs the first test on the main thread" claim is removed.

> **AUTHORITY.** The plan's W4 section (lines 1527–1865) + its Canonical Interfaces
> are prescriptive and this design conforms to them. This doc's job is to resolve
> the remaining method-level signatures against the actual tree and to flag the
> handful of plan→tree drifts found during verification (§6). Reference (read in
> full): `ghostty/src/apprt/embedded.zig:2101–2141` (blur + the two private CGS
> externs + the Darwin `set_display_id`), `:242–405` (newSurface / Platform /
> Platform.init) and `:460–479` (Surface.init content_scale); `src/renderer/
> Metal.zig:95–150` (the NSView→layer host handshake); `src/renderer/Thread.zig:
> 485–487` (drainMailbox `.macos_display_id`).

---

## 0. Files & current state (verified line numbers, 2026-07-09)

- `crates/rustty-apprt-c/src/lib.rs`
  - `rustty_set_window_background_blur` — export `:2164`; the non-macOS no-op arm
    `:2167–2171`; the **macOS `unimplemented!()` block to replace** `:2172–2191`
    (the `#[cfg(target_os="macos")] { … }` block, verbatim Zig body in comments
    `:2174–2184`); the **preserved CGS extern-decl comment to realize**
    `:2194–2204`.
  - `rustty_surface_set_display_id` — `#[cfg(target_os="macos")]` `:2234`, export
    `:2236`; the **`let _ = (ptr, display_id); + unimplemented!()` body to replace**
    `:2237–2247` (verbatim Zig body in comments `:2237–2242`).
  - Live exports the harness drives (all already present): `rustty_app_new` `:254`,
    `rustty_app_tick` `:323`, `rustty_app_update_config` `:502`, `rustty_app_free`
    `:353`(area), `rustty_surface_config_new` `:735`, `rustty_surface_new` `:784`,
    `rustty_surface_draw` `:1151`, `rustty_surface_set_content_scale` `:1291`,
    `rustty_surface_free` `:827`. Type aliases: `App`/`Surface`/`SurfaceOptions`/
    `PlatformC`/`PlatformCMacOS` imported from `rustty_apprt::embedded` `:67–73`;
    `Config` from `rustty_config` `:32`.
- `crates/rustty-apprt/src/embedded.rs`
  - `App.config: UnsafeCell<Config>` `:147` (the blur leaf reads through
    `.get()`); `Options` callback table `:186–217` (fields `userdata`/
    `supports_selection_clipboard`/`wakeup`/`action`/`read_clipboard`/
    `confirm_read_clipboard`/`write_clipboard`/`close_surface`); `Platform::init`
    `:781` (nsview unwrap, LIVE from E2); `Surface.updateContentScale` `:1401`
    (LIVE from E3); `Surface.core_surface` `:889`. **No embedded.rs leaf is filled
    in W4** — the NSView/content-scale host handshake is already live and only
    *exercised* here (§6 drift note).
- `crates/rustty-config/src/value_types.rs` — `pub(crate) fn background_blur_cval(
  v: BackgroundBlur) -> i16` **`:2326`** (the visibility line to bump); the
  reference-provenance comments `:2325`,`:2327+` stay config-local. `BackgroundBlur`
  is `pub use rustty_render_types::BackgroundBlur;` `:2225`.
- `crates/rustty-config/src/config_struct.rs` — `pub background_opacity: f64` `:1743`,
  `pub background_blur: BackgroundBlur` `:1801` (both already `pub` — no change).
- `crates/rustty-core/src/renderer_seam.rs` — `Thread<A>` with `pub wakeup:
  xev::Async` `:357`(area), `pub mailbox: *mut thread::Mailbox` `:410`(area);
  `Thread::init` **live** `:484`; `drainMailbox` handles `Message::MacosDisplayId(v)
  => set_macos_display_id(v)?` **live** `:1161`.
- `crates/rustty-renderer/src/generic.rs` — `set_macos_display_id(&self,id) -> Result`
  `:2281` (live); `golden_renderer` gate `:6259`; `golden_presents_frame_on_screen`
  (MockHostView present-path precedent) `:6580`; `read_bgra` readback primitive on
  `metal::target::Target` (`target.rs:~206`), `px_at` `:6355`.
- `crates/rustty-apprt-c/tests/` — `smoke.rs` (C-ABI compile+link+run KAT, the
  integration-test precedent). **New file this chunk: `macos_window_host.rs`.**

---

## W4-1 — deps + config blur-radius accessor promotion

### 1.1 `crates/rustty-config/src/value_types.rs:2326` — one-line visibility bump

```diff
- pub(crate) fn background_blur_cval(v: BackgroundBlur) -> i16 {
+ pub fn background_blur_cval(v: BackgroundBlur) -> i16 {
```

Faithful to the reference `pub fn cval(self: BackgroundBlur) i16` (it is a *public*
method there). Body + the distinctive provenance comments stay unchanged and
config-local — exactly the Chunk-3a hoist split (the enum lives in
`rustty-render-types`; the value methods stay in `rustty-config`). The existing
`c_get.rs::c_get_background_blur` golden (`:570`) already exercises the mapping and
must stay green (pure visibility change, no behavior change). No `#[allow(dead_code)]`
to remove here — that attribute is on `background_blur_enabled`, not `_cval`.

### 1.2 `crates/rustty-apprt-c/Cargo.toml` — new edges

Insert into `[dependencies]` (after the `rustty-config` entry, ~L48):

```toml
# W4-2/W4-3: the blur leaf's objc msgSend(windowNumber) needs the objc2 runtime;
# the display_id leaf pushes rustty_renderer::Message::MacosDisplayId over the
# render-thread rustty_datastruct BlockingQueue with a Timeout. Forward edges to
# crates far below apprt-c (the top sink) — check_acyclic.sh stays GREEN.
objc2 = { version = "0.6" }
rustty-renderer = { workspace = true }
rustty-datastruct = { workspace = true }
```

**Production CoreGraphics linkage (blocker fix).** The realized private-CGS
`extern "C"` block in §2.2 carries an explicit
`#[link(name = "CoreGraphics", kind = "framework")]`, so the cdylib link of
`rustty-apprt-c` emits `-framework CoreGraphics` and resolves the two CGS SPI
symbols directly — **no** `objc2-core-graphics` in `[dependencies]`. This is the
self-contained choice the adversary flagged as "more robust": it does not rely on
referencing an item from a crate to emit a `#[link]`, and it keeps the cdylib from
gaining a normal `objc2-core-graphics` edge it would otherwise only use for the two
private symbols. (Task W4-1's "objc2-core-graphics for CGS" intent is satisfied by
the framework link; the crate itself is still pulled in — dev-only — for the
harness's `CGMainDisplayID`.)

Add a new `[dev-dependencies]` section (harness-only — these do NOT compile into
the published cdylib, so non-macOS + release builds are unaffected):

```toml
[dev-dependencies]
# W4-4 macos_window_host.rs. The HEADLESS proof hands a plain-NSObject MockHostView
# layer-host double across the C-ABI (renderer golden_presents_frame_on_screen
# precedent) => it needs only objc2 (already a normal dep) + quartz-core + io-surface,
# NO app-kit and NO main-thread marker. objc2-app-kit is dev-only and consumed ONLY
# by the #[ignore] on-device visible-window test (real NSWindow). objc2-core-graphics
# supplies CGMainDisplayID. All 0.3 crates are the SAME family already resolved in-lock
# by rustty-renderer against objc2 0.6.
objc2-quartz-core = { version = "0.3", default-features = false, features = ["CALayer", "CAMetalLayer", "objc2-core-foundation"] }
objc2-foundation = { version = "0.3", features = ["NSGeometry"] }
objc2-core-graphics = { version = "0.3", default-features = false, features = ["CGDirectDisplay"] }
objc2-io-surface = { version = "0.3", features = ["IOSurfaceRef", "IOSurfaceTypes", "objc2", "objc2-core-foundation"] }
objc2-app-kit    = { version = "0.3", default-features = false, features = ["NSWindow", "NSView", "NSResponder", "NSGraphics"] }
rustty-xev = { workspace = true }
```

**Notes.** (a) `objc2-app-kit` is a **dev-dependency** consumed ONLY by the
`#[ignore]` on-device test; the production blur leaf needs only `objc2::msg_send!`
on a raw `AnyObject*` (no AppKit type), so the cdylib never links AppKit, and the
three headless tests use the `MockHostView` double instead of a real view. Because
no headless-gated test depends on `objc2-app-kit`, a per-type feature-name skew in
app-kit 0.3 cannot break the headless gate. (b) `objc2-io-surface` is dev-only for
`read_presented_bgra` (lock + `base_address`, mirroring `Target::read_bgra`).
(c) Feature names confirmed against `Cargo.lock` (§6.4): the co-released 0.3.2
family (quartz-core/foundation/core-graphics/io-surface/core-foundation) is already
in-lock via rustty-renderer; `CGMainDisplayID` lives under
`objc2-core-graphics`'s `CGDirectDisplay` feature (verified in the 0.3.2 source).
Only `objc2-app-kit` is new to the lock — and only the `#[ignore]` test needs it.

**Gates:** `cargo build -p rustty-apprt-c`; `cargo test -p rustty-config background_blur`;
`cargo clippy -p rustty-config -p rustty-apprt-c --all-targets -- -D warnings`;
`bash .port/tools/gates/check_acyclic.sh`.

---

## W4-2 — fill `rustty_set_window_background_blur`

### 2.1 Replace the `#[cfg(target_os="macos")]` block `lib.rs:2172–2191`

The verbatim Zig comments (`:2174–2184`) are RETAINED above the realized body:

```rust
    #[cfg(target_os = "macos")]
    {
        // const config = &app.config;
        //
        // // Do nothing if we don't have background transparency enabled
        // if (config.@"background-opacity" >= 1.0) return;
        //
        // const nswindow = objc.Object.fromId(window);
        // _ = CGSSetWindowBackgroundBlurRadius(
        //     CGSDefaultConnectionForThread(),
        //     nswindow.msgSend(usize, objc.sel("windowNumber"), .{}),
        //     @intCast(config.@"background-blur".cval()),
        // );
        //
        // SAFETY: `app` is a live *mut App (the C host is single-threaded); `window`
        // is a live NSWindow id, only handed to msg_send!/CGS (never deref'd as a
        // Rust reference — no aliasing obligation).
        let config = unsafe { &*(*app).config.get() };

        // Do nothing if we don't have background transparency enabled.
        if config.background_opacity >= 1.0 {
            return;
        }

        let nswindow: *mut objc2::runtime::AnyObject = window.cast();
        // PARITY: `[nswindow windowNumber]` returns `NSInteger`. The reference reads
        // it straight as `usize` (`nswindow.msgSend(usize, sel("windowNumber"), .{})`)
        // and hands it to the CGS `window` arg. We read the ObjC value with its true
        // signed type (`isize`/`NSInteger`) and then `as usize` — a same-width
        // REINTERPRET of the identical bit pattern the reference passes (window
        // numbers are always small positive ints, so the value is unchanged either
        // way; the `as usize` documents that we are matching the reference's
        // `msgSend(usize, …)` byte-for-byte, not performing a conversion).
        let window_number: isize = unsafe { objc2::msg_send![nswindow, windowNumber] };
        let _ = unsafe {
            CGSSetWindowBackgroundBlurRadius(
                CGSDefaultConnectionForThread(),
                window_number as usize,
                // @intCast(config.@"background-blur".cval())
                rustty_config::value_types::background_blur_cval(config.background_blur)
                    as core::ffi::c_int,
            )
        };
    }
```

Remove the `let _ = (app, window);` + `unimplemented!(…)` (`:2185–2190`). The
non-macOS arm `:2167–2171` is UNCHANGED (the reference's comptime early-return).

### 2.2 Realize the private CGS extern block `lib.rs:2194–2204`

Turn the preserved comment into a live `extern "C"` block placed exactly where the
reference declares it (right after the fn). Keep the reference lines as comments:

```rust
// /// See ghostty_set_window_background_blur
// extern "c" fn CGSSetWindowBackgroundBlurRadius(*anyopaque, usize, c_int) i32;
// extern "c" fn CGSDefaultConnectionForThread() *anyopaque;
//
// PORT NOTE: the two private CoreGraphics symbols (embedded.zig:2128-2130) — an
// undocumented, non-public SkyLight/CoreGraphics SPI (the same one Terminal.app
// uses). They are now REALIZED (W4-2) as a live `extern "C"` block. The explicit
// `#[link(name = "CoreGraphics", kind = "framework")]` on the block is what makes
// the cdylib link resolve them: it emits `-framework CoreGraphics` into
// `librustty.dylib`, from which both private symbols export. This is a
// self-contained link directive — it does NOT depend on any `objc2-core-graphics`
// dependency (that crate is dev-only, for the harness's `CGMainDisplayID`), and it
// does NOT rely on QuartzCore/Metal transitively pulling CoreGraphics (QuartzCore's
// `LC_LOAD_DYLIB` on CoreGraphics is not a reexport, and rustc's cdylib link does
// not pass `-undefined dynamic_lookup`, so the symbols would otherwise be
// unresolved). No crate exports these two symbols, so the manual extern is faithful
// to the reference decls.
#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGSSetWindowBackgroundBlurRadius(
        connection: *mut core::ffi::c_void,
        window: usize,
        radius: core::ffi::c_int,
    ) -> i32;
    fn CGSDefaultConnectionForThread() -> *mut core::ffi::c_void;
}
```

**Link check (blocker fix).** After realizing this block, run
`cargo build -p rustty-apprt-c` (which builds the `cdylib`/`staticlib`/`rlib`
targets) and confirm the link succeeds with no
`Undefined symbols: _CGSSetWindowBackgroundBlurRadius,
_CGSDefaultConnectionForThread`. The `#[link]` attribute is the mechanism that
guarantees this — CoreGraphics is not otherwise on `rustty-apprt-c`'s link line
(the renderer links CoreVideo/QuartzCore/Metal but explicitly NOT CoreGraphics).

**Faithfulness map.** `*anyopaque → *mut c_void`; `usize → usize`; `c_int →
c_int`; return `i32 → i32`. The `@intCast(cval())` widening is
`background_blur_cval(...) as c_int` (`i16 → c_int`). `config.@"background-opacity"
>= 1.0` and the `objc.Object.fromId(window)` + `msgSend(usize, sel("windowNumber"))`
map 1:1. The `@export` is UNCONDITIONAL in the reference (only the body is
comptime-gated) — rustty already mirrors this (the fn exists on all targets;
`#[cfg(not(macos))]` = no-op, `#[cfg(macos)]` = the real body).

**SB/TB / Miri:** `window` is a foreign NSWindow pointer never turned into a Rust
`&`/`&mut`; only `msg_send!`/CGS consume it → no aliasing obligation. Foreign
objc/CGS calls ⇒ Miri-excluded (record in `MIRI_EXCEPTIONS.md`, "rustty-apprt-c
W4"); ASan-covered on the FFI gate.

**Focused KAT** (`set_window_background_blur_is_panic_free_on_transparent_config`,
lives in `macos_window_host.rs`): build a transparent-config App (opacity 0.5 ⇒ the
early-return is NOT taken ⇒ `windowNumber` + the private CGS call run for real),
`make_nswindow(mtm)`, call the export, assert it returns cleanly; then flip to an
opaque config (opacity 1.0) **via `rustty_app_update_config`** (drift fix — see §6)
and assert the early-return path is also panic-free.

---

## W4-3 — fill `rustty_surface_set_display_id`

### 3.1 Replace the body `lib.rs:2237–2247`

Retain the verbatim Zig comments (`:2237–2242`) above the realized body:

```rust
#[cfg(target_os = "macos")]
#[no_mangle]
pub unsafe extern "C" fn rustty_surface_set_display_id(ptr: *mut Surface, display_id: u32) {
    // const surface = &ptr.core_surface;
    // _ = surface.renderer_thread.mailbox.push(
    //     .{ .macos_display_id = display_id },
    //     .{ .forever = {} },
    // );
    // surface.renderer_thread.wakeup.notify() catch {};
    //
    // SAFETY: `ptr` validity is the caller's contract. `renderer_thread.mailbox` is
    // a *mut Mailbox (a Box pointee OUTSIDE the Surface struct's Stacked-Borrows
    // coverage — the render-thread's raw-alias discipline); pushing through
    // `&*mailbox` while the render thread holds its own raw alias is the sanctioned
    // construct-then-spawn pattern (no `&mut *self` whole-struct reborrow).
    let surface = unsafe { &(*ptr).core_surface };
    let _ = unsafe { &*surface.renderer_thread.mailbox }.push(
        rustty_renderer::Message::MacosDisplayId(display_id),
        rustty_datastruct::blocking_queue::Timeout::Forever,
    );
    // surface.renderer_thread.wakeup.notify() catch {};
    let _ = surface.renderer_thread.wakeup.notify();
}
```

Also update the **preceding PORT NOTE** (`lib.rs:2229–2233`) to record the leaf is
now realized (it was deferred "until a live surface + render thread exist" — W2/W3
provide exactly that).

**Faithfulness map.** `_ = …push(…)` → `let _ = …push(…)` (discard the `Size`);
`.{ .macos_display_id = display_id }` → `rustty_renderer::Message::MacosDisplayId(
display_id)` (message.rs:118); `.{ .forever = {} }` →
`rustty_datastruct::blocking_queue::Timeout::Forever`; `wakeup.notify() catch {}` →
`let _ = surface.renderer_thread.wakeup.notify()` (swallow the `NotifyError`,
faithful to `catch {}`). This is precisely the established core precedent
(`surface.rs:3884–3891`, `:7197`). The consumer side is already live
(`renderer_seam.rs:1161` `MacosDisplayId(v) => set_macos_display_id(v)?`).

**Focused KAT** (`set_display_id_pushes_to_render_thread_panic_free`): reuse
`HostedSurface::spawn(mtm)` (a LIVE surface with a real render thread + mailbox),
push `CGMainDisplayID()`, `pump_bounded()` so the drain runs, and assert survival.

**Miri:** crosses into the live xev render loop / foreign wakeup ⇒ Miri-excluded;
the raw-pointer aliasing is proven SB/TB-clean on the mirihar harness
(`renderthread_shipped=CLEAN`) before shipping.

---

## W4-4 — culminating harness `crates/rustty-apprt-c/tests/macos_window_host.rs`

A **new integration-test binary**. The whole file is `#![cfg(target_os = "macos")]`.
It is NOT a byte-fidelity port of any one reference fn — it is the harness the plan
mandates, modeled structurally on `live_shell.rs` (bounded `RunMode::NoWait`-style
pump: tick cap + wall-clock deadline + explicit teardown; NEVER a blocking
run-until-done) and reusing the offscreen golden gate's STRUCTURAL readback asserts.

**Main-thread strategy (blocker fix — Option A).** libtest runs `#[test]` bodies on
spawned worker threads, NOT the process main thread, so `MainThreadMarker::new()` is
`None` inside every test and any test that constructs a real `NSWindow`/`NSView`
would either skip unconditionally or throw an uncatchable ObjC main-thread
exception. The three HEADLESS tests therefore hand a plain-`NSObject`
**`MockHostView` layer-host double** across the C-ABI (the exact precedent from the
renderer's `golden_presents_frame_on_screen`, `generic.rs:6531`): it responds to the
four layer-host selectors `Metal::init` sends (`setLayer:`/`layer`/`setWantsLayer:`/
`setClipsToBounds:`) plus `windowNumber` (for the blur leaf), needs NO
`MainThreadMarker` and NO AppKit, and is robust under default parallel `cargo test`.
The real `NSWindow` is used ONLY in the `#[ignore]` on-device visible-window test.
There is no claim anywhere that "cargo runs the first test on the main thread" —
that is false, which is exactly why the double is used.

### 4.1 Skip discipline (headless-green)

The three headless tests have ONE skip guard, returning cleanly (matching the
Metal-skip precedent so headless CI stays green while the on-screen proof runs on
the Apple-Silicon golden runner):

- `rustty_surface_new` returns null (no Metal device on a headless host) ⇒
  `HostedSurface::spawn` returns `None` ⇒ skip.

They do NOT gate on `MainThreadMarker` (the double is main-thread-agnostic), so on
any macOS host WITH a Metal device the host-handshake + present + blur + display_id
paths ACTUALLY run — the culmination is not vacuous. Only the `#[ignore]` on-device
test additionally requires the real AppKit main thread (see §4.3).

### 4.2 The host handshake path (what is proven)

`rustty_app_new(opts, config)` → `rustty_surface_new(app, &surface_opts)` with
`surface_opts.platform_tag = 1` (`PlatformTag::MacOS`) and
`surface_opts.platform.macos.nsview =` **the `MockHostView` double** (a `*mut
c_void` the C-ABI treats identically to a real `NSView*` — `Platform::init` only
stores/unwraps the pointer and `Metal::init` only sends it the four layer-host
selectors the double implements) → core `Surface::init` runs the LIVE
`Platform::init` (unwraps the nsview) + seeds `content_scale` from `scale_factor`
(E2/E3, from `SurfaceOptions.scale_factor` — NOT read off the view, so the double
needs no `backingScaleFactor`) + spawns the render + io threads → `Renderer::init`
runs the W1 `Metal::init(nsview)` which makes the host view layer-hosting
(`setLayer:` + `setWantsLayer:`) with an `IOSurfaceLayer` → the render thread
presents an `IOSurface` as the layer's `contents`. The harness reads the double's
`[view layer]` back and asserts (1) the layer exists (handshake ran), (2)
`contents != nil` after a bounded pump (FIRST ON-SCREEN PIXELS via the C-ABI),
(3) the read-back BGRA is structurally non-vacuous. This is the mandated
"host-handshake + present path proven headless via an off-screen layer + golden
readback" — delivered, and it runs.

### 4.3 The runnable KAT (signatures resolved; helper bodies fixed in SHAPE)

```rust
#![cfg(target_os = "macos")]
#![allow(non_snake_case)] // PARITY: reference C-ABI identifiers verbatim.
//! W4 culminating KAT — FIRST TIME the port hosts a macOS layer-host view and
//! presents an on-screen terminal frame through the published C-ABI with NO Swift
//! app. A Rust-owned layer-host view (a plain-NSObject `MockHostView` double, the
//! renderer `golden_presents_frame_on_screen` precedent — main-thread-agnostic, no
//! AppKit) is handed across the C-ABI (platform.macos.nsview); the embedded
//! Surface's LIVE Platform/content-scale handshake (E2/E3) drives the W1-W3
//! renderer to attach an IOSurfaceLayer to that view and present an IOSurface; a
//! bounded live-loop pump (live_shell.rs discipline) ticks until FIRST PIXELS, then
//! the presented IOSurface is read back and structurally compared to the committed
//! offscreen golden gate. display_id + blur are exercised end-to-end. The real
//! NSWindow lives ONLY in the #[ignore] on-device test.
//!
//! Real objc/Metal/CGS FFI + a live xev render loop => Miri-excluded
//! (.port/phase-2/MIRI_EXCEPTIONS.md, "rustty-apprt-c W4"). The `MockHostView`
//! double has no AppKit main-thread affinity, so the headless tests run correctly
//! on libtest's parallel worker threads (unlike a real NSWindow, which would need
//! the process main thread libtest does not provide).
use std::ffi::{c_char, c_int, c_void};
use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass};

use rustty_apprt::embedded::{
    App, ClipboardContent, ClipboardRequest, ClipboardRequestType, Options,
    PlatformC, PlatformCMacOS, Surface, SurfaceOptions, TargetC, ActionC,
};
use rustty_apprt_c::{
    rustty_app_free, rustty_app_new, rustty_app_tick, rustty_app_update_config,
    rustty_set_window_background_blur, rustty_surface_config_new, rustty_surface_free,
    rustty_surface_new, rustty_surface_set_display_id,
};
use rustty_config::value_types::BackgroundBlur;

// --- MockHostView: the layer-host double handed across the C-ABI as `nsview` ---
// A plain `NSObject` subclass (the renderer `generic.rs:6531` precedent, verbatim
// selectors) so it has NO AppKit main-thread constraint and needs NO
// `MainThreadMarker`. It responds to exactly the selectors the port sends it:
//   - `setLayer:`/`layer`/`setWantsLayer:`/`setClipsToBounds:` — `Metal::init`'s
//     layer-host handshake (stores the CALayer id, returns it from `layer`);
//   - `windowNumber` — the W4-2 blur leaf's `msg_send![window, windowNumber]`
//     (the headless blur test passes THIS double as the `window` arg; a fixed
//     small positive id keeps the private-CGS call well-formed).
struct MockHostViewIvars {
    layer: core::cell::Cell<*mut AnyObject>,
}
define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSObject))]
    #[name = "RusttyApprtCMockHostView"]
    #[ivars = MockHostViewIvars]
    struct MockHostView;

    impl MockHostView {
        #[unsafe(method(setLayer:))]
        fn set_layer(&self, layer: *mut AnyObject) { self.ivars().layer.set(layer); }
        #[unsafe(method(layer))]
        fn layer(&self) -> *mut AnyObject { self.ivars().layer.get() }
        #[unsafe(method(setWantsLayer:))]
        fn set_wants_layer(&self, _wants: bool) {}
        #[unsafe(method(setClipsToBounds:))]
        fn set_clips_to_bounds(&self, _clips: bool) {}
        // `[window windowNumber]` — NSInteger; a stable small positive id.
        #[unsafe(method(windowNumber))]
        fn window_number(&self) -> isize { 1 }
    }
);
fn make_host_view() -> Retained<MockHostView> {
    let this = MockHostView::alloc().set_ivars(MockHostViewIvars {
        layer: core::cell::Cell::new(core::ptr::null_mut()),
    });
    unsafe { msg_send![super(this), init] }
}

// --- minimal C-ABI runtime callback table (embedded::Options) ---
extern "C" fn cb_wakeup(_ud: *mut c_void) {}
extern "C" fn cb_action(_app: *mut App, _t: TargetC, _a: ActionC) -> bool { true }
extern "C" fn cb_read_clipboard(_ud: *mut c_void, _t: c_int, _r: *mut ClipboardRequest) -> bool { false }
extern "C" fn cb_confirm_read_clipboard(_ud: *mut c_void, _s: *const c_char, _r: *mut ClipboardRequest, _t: ClipboardRequestType) {}
extern "C" fn cb_write_clipboard(_ud: *mut c_void, _t: c_int, _c: *const ClipboardContent, _n: usize, _cf: bool) {}
extern "C" fn cb_close_surface(_ud: *mut c_void, _p: bool) {}

fn test_options() -> Options {
    Options {
        userdata: core::ptr::null_mut(),
        supports_selection_clipboard: false,
        wakeup: cb_wakeup,
        action: cb_action,
        read_clipboard: cb_read_clipboard,
        confirm_read_clipboard: cb_confirm_read_clipboard,
        write_clipboard: cb_write_clipboard,
        close_surface: Some(cb_close_surface),
    }
}

// opacity 0.5 (< 1.0) so the blur early-return is NOT taken; blur=.true (=> radius 20).
fn make_config_transparent_blur() -> *mut rustty_config::Config {
    let cfg = rustty_config_new_and_finalize();
    unsafe {
        (*cfg).background_opacity = 0.5;
        (*cfg).background_blur = BackgroundBlur::True;
    }
    cfg
}
fn make_config_opaque() -> *mut rustty_config::Config {
    let cfg = rustty_config_new_and_finalize();
    unsafe { (*cfg).background_opacity = 1.0 };
    cfg
}
// rustty_config_new() (config c_api.rs:29) + finalize; fields are pub.
fn rustty_config_new_and_finalize() -> *mut rustty_config::Config { /* rustty_config::c_api::rustty_config_new() then _finalize */ unimplemented!("build+finalize a default Config") }

// The host `view` is a `MockHostView` double (a `Retained<MockHostView>`); its
// pointer crosses the C-ABI as `platform.macos.nsview`, and it doubles as the
// `window` arg for the blur leaf (it answers `windowNumber`).
struct HostedSurface { app: *mut App, surface: *mut Surface, view: Retained<MockHostView> }
impl HostedSurface {
    fn spawn() -> Option<Self> {
        let view = make_host_view();
        let nsview_ptr: *mut c_void = Retained::as_ptr(&view) as *mut c_void;
        let app = unsafe { rustty_app_new(&test_options(), make_config_transparent_blur()) };
        if app.is_null() { return None; }
        let mut opts: SurfaceOptions = unsafe { rustty_surface_config_new() };
        opts.platform_tag = 1;                                   // PlatformTag::MacOS
        opts.platform = PlatformC { macos: PlatformCMacOS { nsview: nsview_ptr } };
        opts.scale_factor = 2.0;                                 // Retina content-scale handshake
        let surface = unsafe { rustty_surface_new(app, &opts) };
        if surface.is_null() { unsafe { rustty_app_free(app) }; return None; } // no Metal device => skip
        Some(HostedSurface { app, surface, view })
    }
    fn view_ptr(&self) -> *mut c_void { Retained::as_ptr(&self.view) as *mut c_void }
    fn pump_bounded(&self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        for _ in 0..2000 {
            unsafe { rustty_app_tick(self.app) };
            if self.layer_has_contents() { break; }
            if Instant::now() > deadline { break; }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    // `[view layer]` -> the IOSurfaceLayer (a CALayer subclass) Metal::init attached.
    fn layer(&self) -> Option<Retained<AnyObject>> { unimplemented!("[view layer] as AnyObject") }
    // CALayer.contents() != nil (an IOSurface was presented).
    fn layer_has_contents(&self) -> bool { unimplemented!("layer().contents().is_some()") }
    // Lock the layer's IOSurface contents, read w*h*4 BGRA — mirrors Target::read_bgra
    // (IOSurfaceLockOptions::ReadOnly, base_address, bytes_per_row honored).
    fn read_presented_bgra(&self) -> Option<Vec<u8>> { unimplemented!("lock+read layer IOSurface contents") }
}
impl Drop for HostedSurface {
    fn drop(&mut self) { unsafe { rustty_surface_free(self.surface); rustty_app_free(self.app); } }
}

#[test]
fn hosts_view_layer_and_presents_live_frame() {
    // NO MainThreadMarker guard — MockHostView is main-thread-agnostic, so this
    // ACTUALLY runs on libtest worker threads. Only skip guard is the Metal probe.
    let host = match HostedSurface::spawn() {
        Some(h) => h,
        None => { eprintln!("no Metal device / app_new failed; skipping"); return; }
    };
    // (1) HOST HANDSHAKE: the renderer attached an IOSurfaceLayer to our host view.
    assert!(host.layer().is_some(), "host view is not layer-hosting");
    // (2) FIRST ON-SCREEN PIXELS: pump until the render thread presents an IOSurface.
    host.pump_bounded();
    assert!(host.layer_has_contents(), "no IOSurface presented (no on-screen frame)");
    // (3) GOLDEN GATE REUSE (STRUCTURAL, not exact-PNG): non-vacuous, view-sized.
    if let Some(px) = host.read_presented_bgra() {
        assert!(px.iter().any(|&b| b != 0), "presented frame is all-zero");
    }
    // (4) display_id: CGMainDisplayID pushed to the live render-thread mailbox.
    let display_id: u32 = unsafe { objc2_core_graphics::CGMainDisplayID() };
    unsafe { rustty_surface_set_display_id(host.surface, display_id) };
    host.pump_bounded();
    // (5) blur: transparent config => the real objc windowNumber + private CGS run.
    //     The MockHostView double answers `windowNumber`, so the CGS SPI is invoked
    //     for real (proving the #[link] CoreGraphics resolution end-to-end).
    unsafe { rustty_set_window_background_blur(host.app, host.view_ptr()) };
    // Surviving all five phases IS the culmination.
}

// --- W4-2 focused sub-KAT ---
#[test]
fn set_window_background_blur_is_panic_free_on_transparent_config() {
    let app = unsafe { rustty_app_new(&test_options(), make_config_transparent_blur()) };
    assert!(!app.is_null(), "app_new");
    let view = make_host_view();
    let wptr: *mut c_void = Retained::as_ptr(&view) as *mut c_void;
    unsafe { rustty_set_window_background_blur(app, wptr) };     // opacity 0.5 => runs for real
    unsafe { rustty_app_update_config(app, make_config_opaque()) }; // DRIFT FIX (§6): not rustty_config_replace
    unsafe { rustty_set_window_background_blur(app, wptr) };     // opacity 1.0 => early-return path
    unsafe { rustty_app_free(app) };
    let _keep = view; // the host view outlives the calls
}

// --- W4-3 focused sub-KAT ---
#[test]
fn set_display_id_pushes_to_render_thread_panic_free() {
    let Some(host) = HostedSurface::spawn() else { return; }; // no Metal => skip
    let display_id: u32 = unsafe { objc2_core_graphics::CGMainDisplayID() };
    unsafe { rustty_surface_set_display_id(host.surface, display_id) };
    host.pump_bounded();                                          // drain runs
    drop(host);
}

// --- on-device visible variant (#[ignore]): a REAL NSWindow, run ON THE MAIN THREAD ---
// This is the ONLY test that uses AppKit (`objc2-app-kit`, dev-only). It is #[ignore]
// and NOT part of the headless gate: it needs a live WindowServer/GUI session AND the
// process main thread. Under the default libtest harness it runs on a worker thread,
// where `MainThreadMarker::new()` is `None` and it early-returns with a note — to
// actually show the window, invoke it from a main-thread runner (e.g. a
// `harness = false` shim, or `cargo test -- --ignored --test-threads=1` from a GUI
// login session where the first frame lands on the main thread). It is a manual
// on-device confirmation, never a CI gate.
#[test]
#[ignore = "on-device: real NSWindow; needs a WindowServer + the process main thread"]
fn shows_visible_window_on_device() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("shows_visible_window_on_device: not on the AppKit main thread; \
                   run from a main-thread runner to see a real window. Skipping.");
        return;
    };
    let window: Retained<NSWindow> = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(200.0, 120.0)),
            NSWindowStyleMask::Titled,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    let view = window.contentView().expect("contentView");
    let nsview_ptr: *mut c_void = Retained::as_ptr(&view) as *mut c_void;
    let app = unsafe { rustty_app_new(&test_options(), make_config_transparent_blur()) };
    assert!(!app.is_null(), "app_new");
    let mut opts: SurfaceOptions = unsafe { rustty_surface_config_new() };
    opts.platform_tag = 1;
    opts.platform = PlatformC { macos: PlatformCMacOS { nsview: nsview_ptr } };
    opts.scale_factor = 2.0;
    let surface = unsafe { rustty_surface_new(app, &opts) };
    if surface.is_null() { unsafe { rustty_app_free(app) }; return; } // no Metal => skip
    unsafe { window.makeKeyAndOrderFront(None) };                // VISIBLE (needs WindowServer)
    // bounded pump against the real view's layer contents.
    let deadline = Instant::now() + Duration::from_secs(5);
    for _ in 0..2000 {
        unsafe { rustty_app_tick(app) };
        if Instant::now() > deadline { break; }
        std::thread::sleep(Duration::from_millis(2));
    }
    // real windowNumber + private CGS blur on the live NSWindow.
    let wptr: *mut c_void = Retained::as_ptr(&window) as *mut c_void;
    unsafe { rustty_set_window_background_blur(app, wptr) };
    unsafe { rustty_surface_free(surface); rustty_app_free(app) };
    let _keep = (window, view);
}
```

### 4.4 The three `unimplemented!()` helper shapes (fixed here, spelled out at impl)

- `rustty_config_new_and_finalize` — `rustty_config::c_api::rustty_config_new()`
  (returns `*mut Config`) then `rustty_config_finalize`. Fields are `pub`, so the
  opacity/blur overrides are direct writes.
- `layer()` / `layer_has_contents()` — `[view layer]` on the `MockHostView` double
  (`let l: *mut AnyObject = msg_send![&*self.view, layer]`), the id `Metal::init`'s
  `setLayer:` stored — no objc2-app-kit `NSView::layer` needed (that stays in the
  `#[ignore]` on-device test). Cast the returned id through
  `objc2_quartz_core::CALayer` and read `contents()` (an `unsafe fn` ⇒
  `Option<Retained<AnyObject>>`, the precedent is `generic.rs:6659`).
- `read_presented_bgra()` — cast the `contents()` id to `*const IOSurfaceRef`, lock
  with `IOSurfaceLockOptions::ReadOnly`, read `base_address` for
  `bytes_per_row * height`, unlock — byte-for-byte the `Target::read_bgra`
  (`target.rs:~206`) recipe, applied to the layer's IOSurface instead of the
  renderer's own Target. On-screen compare stays STRUCTURAL (non-vacuous), never
  exact-PNG (P3/premultiply/BGRA rounding on the on-screen path is not bit-stable).

### 4.5 Gates

`cargo build -p rustty-apprt-c`; `cargo test -p rustty-apprt-c --test
macos_window_host`; `cargo clippy -p rustty-apprt-c --all-targets -- -D warnings`;
`cargo fmt --check`; `bash .port/tools/gates/check_acyclic.sh`;
`python3 .port/tools/gates/check_test_counts.py` (macOS-gated file ⇒ 0 tests on
non-macOS ⇒ no count regression; annotate as rustty-only margin like `smoke.rs`/
`live_shell.rs`); `python3 .port/tools/gates/check_comment_coverage.py`.

---

## 5. R-rule compliance summary

- **R1** — both leaf bodies retain the verbatim Zig above the realized code; the
  `_ =`/`let _ =` discards mirror `_ =`/`catch {}`; `@intCast` → `as c_int`. The
  KAT is a rustty-only integration harness (no reference `test{}`), authored 1:1.
- **R4/acyclic** — apprt-c is the top sink; `objc2`/`rustty-renderer`/
  `rustty-datastruct` are forward edges to crates far below it; AppKit stays
  dev-only (consumed only by the `#[ignore]` on-device test). `check_acyclic.sh`
  GREEN. `rustty-renderer` keeps NO app-kit edge (the real NSWindow lives in
  apprt-c's dev-deps, not the renderer).
- **CoreGraphics linkage (production cdylib)** — the private CGS symbols link via
  an explicit `#[link(name = "CoreGraphics", kind = "framework")]` on the realized
  `extern "C"` block (§2.2), NOT via any crate dependency. `objc2-core-graphics` is
  a **dev-dependency only** (harness `CGMainDisplayID`) and is NOT on the cdylib's
  link line. This replaces the earlier (incorrect) claim that CoreGraphics was
  "transitively linked via objc2-core-graphics" — that crate is dev-only, so it
  could never have linked the cdylib; the framework `#[link]` is the real mechanism.
  Gate: `cargo build -p rustty-apprt-c` resolves the two CGS symbols.
- **R5** — the two leaves are the "deferred = `unimplemented!()` + verbatim body"
  promotion, now that W0–W3 supply the consumed seams.
- **R6** — the FFI harness is ASan-eligible, Miri-excluded (record both leaves +
  the harness in `MIRI_EXCEPTIONS.md`, "rustty-apprt-c W4").

## 6. Plan → tree drifts found during verification (corrections applied above)

1. **`rustty_config_replace` does not exist.** The plan's W4-2 KAT (line 1626)
   calls `rustty_config_replace(app, opaque)`. The real export is
   **`rustty_app_update_config(v: *mut App, config: *const Config)`** (`lib.rs:502`).
   Corrected in §4.3.
2. **No new `embedded.rs` macOS leaf is filled.** The task brief's `~1813-2176`
   range is the Inspector block (deferred to sub-project #6). The NSView/
   content-scale host handshake (`Platform::init`, `Surface::init` content_scale,
   `updateContentScale`) is ALREADY LIVE from E2/E3 and is only *exercised* by the
   harness. W4 touches only `lib.rs` (2 leaves), `Cargo.toml`, `value_types.rs`
   (1 line), and the new test file. (Matches the plan's own Open-risks note.)
3. **`background_blur_cval` is a free fn, not a method.** The reference
   `BackgroundBlur.cval()` is a method, but rustty already ported it as
   `pub(crate) fn background_blur_cval` (value_types.rs:2326) with the enum hoisted
   to `rustty-render-types`. The faithful minimal move is the visibility bump (not
   re-homing it as an inherent method) — consistent with the Chunk-3a hoist note.
4. **objc2-* feature gate names — confirmed against `Cargo.lock` (2026-07-09).**
   `objc2 = 0.6.4`; `objc2-quartz-core`/`objc2-foundation`/`objc2-core-graphics`/
   `objc2-io-surface`/`objc2-core-foundation` are all `0.3.2` and ALREADY in-lock
   via rustty-renderer. `CGMainDisplayID` is verified to live under
   `objc2-core-graphics 0.3.2`'s `CGDirectDisplay` feature (source-checked;
   `CGMainDisplayID() -> CGDirectDisplayID = u32`, matching the harness's `u32`).
   `objc2-app-kit` is the ONLY crate new to the lock, and it is consumed EXCLUSIVELY
   by the `#[ignore]` on-device test (`NSWindow`/`NSWindowStyleMask`/
   `NSBackingStoreType` under the requested `NSWindow`/`NSView`/`NSResponder`/
   `NSGraphics` features) — so if an app-kit 0.3 per-type feature name needs a
   one-line adjustment, it can only affect that one `#[ignore]` test, NEVER the
   headless gate (which uses the `MockHostView` double + already-locked crates).
   Confirm at build with `cargo build -p rustty-apprt-c --tests`.

## 7. Risks (carried forward)

- **HIGH:** W4-4 transitively needs ALL of W0–W3 live; land it LAST, after W0–W3
  gate-green. Mitigated by the single Metal-skip guard (headless CI stays green;
  the on-screen proof runs on any Apple-Silicon runner WITH a Metal device — the
  `MockHostView` double removes the main-thread dependency, so the three headless
  tests actually execute rather than skip). Only the `#[ignore]` on-device test
  needs the AppKit main thread.
- **MED:** reading `layer.contents` off the render thread races the presenter —
  read only after the bounded pump observes `contents != nil`; keep the compare
  STRUCTURAL (non-vacuous), never exact-PNG.
- **MED:** the private CGS symbols are undocumented — the cdylib link depends on
  CoreGraphics.framework exporting them (it does; the reference links them
  identically). Record the OS-stability assumption in MIRI_EXCEPTIONS/ASan notes.
- **LOW:** objc2-* 0.3 version skew vs objc2 0.6 — pin the in-lock versions.
