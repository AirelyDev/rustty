# Phase 5 assessment — area D: UI tiers (macOS Swift / apprt / renderer / font / input / pkg / dist)

Range `12967b68f..683d8db64` (2026-08-17 → 2026-08-25). Owned paths (oracle-relative):

```
macos/**            30 files, +1279 / −411      src/apprt/** + src/apprt.zig   8 files (~650 diff lines)
src/renderer/**      4 files (+1010 / −213)     src/font/glyf_rasterize.zig    1 file (12 lines)
src/input/paste.zig  1 file (+164 / −4)         pkg/{wuffs,afl++}              7 files (~270 lines)
dist/                0 files in range
```

**38 non-merge commits** touch these paths (`git log --oneline --no-merges 12967b68f..683d8db64 -- macos src/apprt src/apprt.zig src/renderer src/font src/input pkg dist` = 38; every one is verdicted below exactly once, cross-area spans noted).
Rustty counterparts: `macos/` (Swift tree, Rustty-renamed), `crates/rustty-apprt`,
`crates/rustty-apprt-api`, `crates/rustty-apprt-c`, `crates/rustty-renderer`,
`crates/rustty-font`, `crates/rustty-vt/src/input/`, `crates/rustty-wuffs`,
`crates-sys/wuffs-sys`.

## Global evidence (applies to every ABSENT verdict)

Rustty HEAD is `d63dd9fc` — the Phase-4 finale itself. `git log d63dd9fc..HEAD` is empty:
**no rustty commit postdates the Phase-4 absorption of `12967b68f`**, so nothing from this
range can have been ported yet. Every ABSENT below additionally carries a per-cluster
symbol grep. The Phase-4 macOS wave DID land (rustty has `UntrustedURL.swift`,
`CommandLineOpenFileFilter.swift`, `Rustty.MenuShortcutManager.swift`,
`preferredImageVisibility` — all Phase-4 rows), so the tree measures at exactly the OLD
pin, which is what makes the two in-range reverts/deletions below real work items.

## 1. Summary

| Cluster | PORTED | PARTIAL | ABSENT | N-A | rows |
|---|---|---|---|---|---|
| Kitty clipboard protocol OSC 5522 (macOS+apprt) | 0 | 0 | 11 | 0 | 11 |
| Renderer GPU-release + kitty animation | 0 | 0 | 4 | 0 | 4 |
| Kitty placement geometry (renderer half) | 0 | 0 | 4 | 0 | 4 |
| macOS menu-shortcut / KeyboardLayout wave | 0 | 0 | 7 | 0 | 7 |
| AppleScript ScriptKeyEventCommand tests | 0 | 0 | 1 | 0 | 1 |
| macOS glass rework + misc | 0 | 0 | 4 | 0 | 4 |
| pkg/wuffs + pkg/afl++ | 0 | 0 | 2 | 3 | 5 |
| gtk XKB modifier remap | 0 | 0 | 1 | 0 | 1 |
| font glyf direct rasterize | 0 | 0 | 1 | 0 | 1 |
| **Total** | **0** | **0** | **35** | **3** | **38** |

Headline reads:

- **Nothing from this range has been ported** (expected — pin advanced first, per README).
- **The Kitty clipboard protocol (OSC 5522) is the range's dominant feature** and is a
  cross-cutting atomic bundle: apprt structs/surface/embedded + gtk + 9 macOS commits +
  core Surface.zig + termio + the NEW terminal `kitty/clipboard*.zig` subsystem (area
  A/kitty's G1b `kitty drop=202`) + `include/ghostty.h`. Its `read_clipboard` callback
  signature change (bool → `ClipboardReadResult` c_int) is a **C-ABI break** on the
  embedded callback table — half-porting it miscalls the Swift host.
- **The renderer wave rewrites the frame/animation clock**: the draw timer is deleted and
  replaced by a renderer-computed `animationWake()`; the swap chain becomes nullable and
  is released while hidden (head commit `c4e16970a`) and rebuilt in `drawFrame`.
- **One already-shipping rustty divergence gets FIXED upstream**: `86db30785` repairs
  `gaToRgba` (gray+alpha swizzle failed for ALL inputs). Rustty documented exactly that
  upstream bug as dead code and pinned it with a test asserting the FAILURE
  (`crates/rustty-wuffs/src/swizzle.rs:340 ga_to_rgba_always_unsupported`) — that test
  must now be replaced by the oracle's new byte-expectation test.
- **Two in-range deletions of Phase-4-ported code**: `f6990690f` reverts the
  `preferredImageVisibility` extension (rustty still ships it), and `fd17869d1` deletes
  `TerminalViewContainerTests.swift` (rustty still has it). Commit-by-commit porting
  would miss both; the range diff handles them (Binding rules).

## 2. Per-commit verdicts (range-diff end state)

Legend: `→` = rustty location; "grep ∅" = stated grep over the stated tree returned nothing.

### 2a. Kitty clipboard protocol, OSC 5522 (bundle D-B1, 11 commits)

End state across my paths: `src/apprt/structs.zig` gains `ClipboardReadResult
{started, unavailable, unsupported}` (enum(c_int), "must also update ghostty.h"),
`ClipboardRequestType` grows `kitty_read/kitty_write/list`, `ClipboardRequest.paste`
becomes payload-carrying (`paste: Clipboard`), and NEW arena-owning `KittyRead`/
`KittyWrite` request structs (created on IO thread, completed on app thread, `destroy`
moves the arena out of a struct allocated IN the arena). `src/apprt/surface.zig`
`Message` gains `kitty_clipboard_read/write: *KittyRead/*KittyWrite` (ownership-passing).
`src/apprt.zig` re-exports `ClipboardReadResult`. `src/apprt/embedded.zig`: callback
`read_clipboard` returns `ClipboardReadResult` (was bool), NEW extern structs
`ClipboardComplete`/`ClipboardConfirm` (with `contents: ?[*]const ClipboardContent`),
`completeKittyClipboardWrite`, `denyClipboardRequest`, and export
`ghostty_surface_deny_clipboard_request`. gtk: `clipboard.zig` request paths return
`ClipboardReadResult`, `.kitty_read/.kitty_write/.list => .unsupported` (GTK opts out),
paste completion becomes `.{ .paste = .standard }` + `ClipboardContent` slices. macOS:
`Ghostty.App.swift` read/write confirmation flow rebuilt around
`Ghostty.ClipboardConfirmationRequest` carrying mimes/pw/name/granted,
`NSPasteboard+Extension` serves ALL content types + `text/uri-list` for file copies +
ENOSYS for primary writes, mode-5522 paste events, image previews in the confirmation
dialog.

| Commit(s) | What changed (my-path sliver) | Verdict | Evidence / notes |
|---|---|---|---|
| `0ce9054bf` | Protocol READS: structs `kitty_read`+`ClipboardReadResult`, surface Message, embedded plumbing, gtk `.unsupported` arms, macOS App/ConfirmationRequest | **ABSENT** | `crates/rustty-apprt-api/src/structs.rs:88` `ClipboardRequestType` still `{paste, osc_52_read, osc_52_write}` only; grep ∅ `ClipboardReadResult\|KittyRead\|kitty_read` over `crates/rustty-apprt*`; grep ∅ `kittyRead\|KittyClipboard\|5522` over `macos/Sources` |
| `8c7a34d4c` | Reads serve all clipboard content types (`ClipboardContent` arrays through CAPI + Swift) | **ABSENT** | `crates/rustty-apprt/src/embedded.rs:222` doc still states the OLD bool contract ("was started and complete_clipboard_request may be called") |
| `af9470b19` | pw/name session grants (KittyRead/Write `pw`,`name`,`granted` fields; Swift grant store) | **ABSENT** | same greps ∅ |
| `c1f0ef73a` | Copied files served as `text/uri-list` (NSPasteboard+Extension only) | **ABSENT** | grep ∅ `uriList\|text/uri-list` in `macos/Sources/Helpers/Extensions/NSPasteboard+Extension.swift` |
| `df14efaf3` | Image previews in the read-confirmation dialog | **ABSENT** | macOS tree at old pin (global evidence) |
| `c2c0db68a` | Mode 5522 paste events (`paste: Clipboard` payload; NSPasteboard; modes.zig sliver is area A's) | **ABSENT** | `structs.rs:101` `ClipboardRequest` paste is still void-like |
| `25c61e852` | Protocol WRITES (`KittyWrite`, `ClipboardConfirm`/`ClipboardComplete` CAPI, termio slivers are E's) | **ABSENT** | grep ∅ `KittyWrite\|ClipboardConfirm` over `crates/` |
| `1334cc213` | ENOSYS answer for Kitty writes to primary (NSPasteboard + embedded sliver; stream_terminal sliver is A's) | **ABSENT** | grep ∅ `ENOSYS` under `macos/` |
| `550151882` | Dropped-text paste semantics restored (Ghostty.Surface + SurfaceView route drops through the paste encoder) | **ABSENT** | macOS tree at old pin |
| `a2212a5b1` | `aspectRatio` → `scaledToFit` in clipboard preview | **ABSENT** | mechanical; tree at old pin |
| `a5bb22e23` | **Shared paste core**: `src/input/paste.zig` gains `bracketed_prefix/suffix`, `max_frame_size`, `encodeWriter`, `isSafeWith` + **7 new tests**; NEW `src/terminal/paste.zig` + kitty clipboard_response (area A's rows) | **ABSENT** | `crates/rustty-vt/src/input/paste.rs` grep ∅ `bracketed_prefix\|encode_writer\|is_safe_with\|max_frame_size`; 10 `#[test]`s (oracle now 17). **This is G1b `input_paste drop=7`** (§5) |

### 2b. Renderer — GPU resource release + kitty animation clock (bundle D-B2)

| Commit(s) | What changed (end state) | Verdict | Evidence / notes |
|---|---|---|---|
| `c4e16970a` (HEAD) | **Release GPU resources for hidden surfaces**: `swap_chain` becomes `?SwapChain` (defunct flag DELETED; `nextFrame` infallible), `display_realized: bool`, `releaseGpuResources()` under draw_mutex (deinit waits in-flight frames, then `api.gpuResourcesReleased()` hook — `OpenGL.gpuResourcesReleased` drops `last_target`), `setVisible` releases when `!visible` gated on NEW `apprt.must_draw_from_app_thread` (decl HOISTED from renderer/Thread.zig into `src/apprt.zig`), `drawFrame` rebuilds the swap chain (`swap_chain_rebuilt` forces redraw), completion asserts `self.swap_chain.?` | **ABSENT** | `crates/rustty-renderer/src/generic.rs:1590-1596` still has `defunct: bool` + doc; grep ∅ `release_gpu_resources\|gpu_resources_released\|display_realized` over `crates/rustty-renderer`; `must_draw_from_app_thread` exists only as the GTK-side const (`crates/rustty-apprt/src/gtk/app.rs:319`), no apprt-level hoist |
| `aee7bf347` | **Kitty graphics animation drive**: Thread's `DRAW_INTERVAL`/`draw_h`/`draw_active`/`syncDrawTimer` DELETED; `render_h` becomes the animation timer (`reset` + `render_c_cancel`, `error.Canceled => .disarm`); renderer computes `animationWake() ?{delay_ms, kind: draw/update}` from `custom_shader_animation` (config field MOVES Thread.DerivedConfig → renderer Options) + `kitty_animation_next_ms` (absolute deadline on `kitty_animation_clock`, floored at `draw_interval_ms=8`); `updateFrame` calls `storage.animationTick(io, now_ms)` before the dirty check; visibility regain routes through `renderCallback`; `needs_redraw` uses `animationWake() != null` (was `hasAnimations()`); +1 image.zig test ("uploads the current animation frame") | **ABSENT** | `crates/rustty-renderer/src/thread.rs:50-90` still ports `DerivedConfig { custom_shader_animation }`; `generic.rs:5470` still `has_animations()`; grep ∅ `animation_wake\|kitty_animation` over the crate. Coupling: `storage.animationTick` is area A/kitty (graphics_animation, part of G1b `kitty drop=202`); config accessor move rides rustty's `RendererConfig` trait (memory `renderer-config-accessor-trait-acyclic`) |
| `169213cd2` | `prepImage` fuses copy-to-owned + pixel-format conversion (single pass, no intermediate) | **ABSENT** | `crates/rustty-renderer/src/image/{macos,linux}_impl.rs` last touched `bd157dee` (Phase-4); old two-step shape quoted at `macos_impl.rs:1530-1551` |
| `b17abd96d` | `prepForUpload` restricted to `Image.Pending` (takes Pending, not the union; −16 lines) | **ABSENT** | `macos_impl.rs:1530` `prep_for_upload` still on the image union |

### 2c. Renderer — kitty placement geometry (bundle D-B3; storage halves are area A's)

| Commit(s) | What changed (my-path sliver, end state) | Verdict | Evidence / notes |
|---|---|---|---|
| `cd5f9eef0` | image.zig sliver (+4) for relative placement storage intro (bulk = `graphics_storage.zig` +529, area A) | **ABSENT** | with `9490f7134` below |
| `9490f7134` | **Relative placements** (+386): `Origin {pin: *const Pin, h/v offsets}`; `prepKittyPlacement` rewritten `p.rect()` → `p.gridSize()` with signed/widened i64 edge math + `origin.pin.garbage` check + `std.math.cast(i32)` culling; `pending_relative` list for chains rooted at VIRTUAL placements, resolved after the placeholder scan via `virtual_origins` min-x/min-y map + `storage.placeholderTarget`; `resolveChain` orphans skipped; **+3 tests** | **ABSENT** | grep ∅ `resolve_chain\|pending_relative\|placeholder_target` over `crates/rustty-renderer`; `image/macos_impl.rs:658` still the OLD "viewport-relative position" shape. Couplings: `storage.resolveChain/placeholderTarget/gridSize` (A), `c/kitty_graphics.zig` (C) |
| `52190a5d8` | Source rectangles intersected before sizing (`p.rect` → intersected source; c/kitty_graphics sliver is C's); **+1 test** ("intersected source rectangle") | **ABSENT** | same greps ∅ |
| `c5a3c7e2e` | Placement offsets constrained to cell bounds (image.zig sliver ±5; exec/storage = A) | **ABSENT** | same |

### 2d. macOS menu-shortcut / KeyboardLayout wave (bundle D-B5, 7 commits)

End state: `KeyboardLayout.character(for:modifiers:)` (43-line file; AppKit
`NSEvent.keyEvent`+`characters(byApplyingModifiers:)`, @MainActor); `Ghostty.Input.
keyboardShortcut(for:)` translates printable PHYSICAL triggers through the current layout
(`writingSystemKeyRange` = BACKQUOTE...SLASH, command-only modifier intersection,
`Key.init(cKey:)`); `Config.keybindTrigger(for:)` split out; `MenuShortcutManager`:
`MenuShortcutKey.Identity {keyEquivalent(String) | physicalKeyCode(UInt16)}`, physical
lookup takes precedence in `performGhosttyBindingMenuKeyEquivalent`,
`allowsAutomaticKeyEquivalentLocalization/Mirroring = !isPhysical`; AppDelegate observes
`NSTextInputContext.keyboardSelectionDidChangeNotification` → re-sync + `relabelTabs()`.
Tests: `KeyboardLayoutTests.swift` NEW (3 @Test, not in 12967b68f), MenuShortcutManagerTests
+15, NormalizedMenuShortcutKeyTests ±16.

| Commit(s) | What changed | Verdict | Evidence |
|---|---|---|---|
| `569ff3307` | Physical menu-shortcut translation (initial Carbon impl + tests) | **ABSENT** | rustty `macos/Sources/Helpers/KeyboardLayout.swift` = **14 lines = byte-shape of the 12967b68f state** (oracle end state 43); grep ∅ `keybindTrigger\|physicalKeyCode\|writingSystemKeyRange\|keyboardSelectionDidChange` over `macos/Sources`; no `macos/Tests/Helpers/KeyboardLayoutTests.swift` (find ∅) |
| `761696c34` | Menu shortcut identity simplified (`Identity` enum; NSMenuItem-derived init deleted) | **ABSENT** | grep ∅ `Identity` in `Rustty.MenuShortcutManager.swift` |
| `9886f4817` | Keyboard-layout actor isolation (@MainActor sweep App/Config/Input/KeyboardLayout + test adjustments) | **ABSENT** | with the wave; end-state file greps above |
| `f5ad3a0a4` | Carbon `UCKeyTranslate` → AppKit `characters(byApplyingModifiers:)` | **ABSENT** | superseded-into-end-state; port END STATE only |
| `f1b9efed8` | `character(for:modifiers:)` default-behavior fix + Ghostty.Input tweak (−15 test lines) | **ABSENT** | end state carries it |
| `043abc7b6` | MainMenu.xib: settings menu in a separate group | **ABSENT** | rustty has `macos/Sources/App/MainMenu.xib` at old pin; xib text sync |
| `f6990690f` | **Revert** of Phase-4 `301bd6f8b`/`99c483f47`: DELETE the `preferredImageVisibility` NSMenuItem extension | **ABSENT** (revert not applied) | rustty `macos/Sources/Helpers/Extensions/NSMenuItem+Extension.swift:12` still sets `preferredImageVisibility = .automatic` — Phase 4 ported it; this range deletes it. NOT N-A: original is outside this range |

### 2e. AppleScript ScriptKeyEventCommand tests

| Commit | What changed | Verdict | Evidence |
|---|---|---|---|
| `d7f5ba3b4` | `ScriptKeyEventCommand` refactored for testability: static `parse(directParameter:evaluatedArguments:translationMods:)` throwing `ArgumentError {missingKey, unknownKey, unknownModifiers}`; NEW `macos/Tests/Features/AppleScript/ScriptKeyEventCommandTests.swift` (230 lines, **16 @Test**) | **ABSENT** | rustty `macos/Sources/Features/AppleScript/ScriptKeyEventCommand.swift` grep ∅ `static func parse\|ArgumentError`; find ∅ `ScriptKeyEventCommandTests*` under `macos/Tests` |

### 2f. macOS glass rework + misc

| Commit(s) | What changed | Verdict | Evidence |
|---|---|---|---|
| `fd17869d1` | Rework for #10943: `TerminalGlassView` rebuilt as SwiftUI `GlassBackground` over an ObservableObject `GlassViewModel` (replaces NSGlassEffectView + tint-overlay + `updateKeyStatus`); NSColor tint helpers deleted; Backport tweak; **DELETES `TerminalViewContainerTests.swift`** (−40) | **ABSENT** (deletion outstanding) | grep ∅ `GlassViewModel` over `macos/Sources`; rustty still HAS `macos/Tests/Terminal/TerminalViewContainerTests.swift` — must be deleted (its subject class shape is rewritten) |
| `10d732688` | GlassViewModel cleanup (1 stray line from `fd17869d1`) | **ABSENT** | subsumed by porting the end state of `fd17869d1` |
| `1d24eecb2` | Repeated new-tab action responsiveness (TerminalController +10) | **ABSENT** | tree at old pin (global evidence) |
| `6cf7e0cc5` | swiftlint fixes (2 one-liners) | **ABSENT** | mechanical; tree at old pin |

### 2g. pkg/wuffs + pkg/afl++ (bundle D-B4 with area A's graphics_pixel)

| Commit(s) | What changed (my-path sliver) | Verdict | Evidence / notes |
|---|---|---|---|
| `322d7ae78` | pkg/wuffs `swizzle.zig` gains `rgbaSrcOver` (in-place SRC_OVER composite via `wuffs_base__pixel_swizzler`, nonpremul RGBA, asserts len%4) + `rgbaSrcOver` test; consumed by `kitty/graphics_pixel.zig` (area A) | **ABSENT** | grep ∅ `rgba_src_over\|SRC_OVER` over `crates/rustty-wuffs/src`. Bundle: kitty animation compositing (A) strands without it |
| `86db30785` | **gaToRgba FIXED**: `YA_PREMUL→RGBA_PREMUL` (unsupported pair, failed for ALL inputs) becomes `YA_NONPREMUL→RGBA_NONPREMUL` + comment; NEW `gaToRgba` byte test; `rgbaSrcOver` test expanded | **ABSENT** — and a live divergence | `crates/rustty-wuffs/src/swizzle.rs:54` still swizzles FROM `YA_PREMUL`; `swizzle.rs:330-340` documents the upstream bug and test `ga_to_rgba_always_unsupported` ASSERTS the failure. Porting must REPLACE that test with the oracle's expectation (that is fixing, not weakening — R5). Verify vendored alpha.9 supports the NONPREMUL pair; if not, the alpha.10 bump (Phase-4 debt `7c4c7adad`, still unpaid: `crates-sys/wuffs-sys/vendor/release/c/wuffs-v0.4.c:89` = `alpha.9`) becomes REQUIRED here |
| `619555d1c` | Build wuffs without libc: NEW shim headers `pkg/wuffs/include/{stdlib.h,string.h}` (minimal decls so translate-c needs no libc/Apple SDK), weak hidden `calloc`/`free` stubs in main.zig, `link_libc` only on Windows | **N-A** | Zig translate-c/no-libc mechanics. rustty's `wuffs-sys` compiles the vendored release-C artifact with `cc` against the REAL system libc (`crates-sys/wuffs-sys/build.rs`) — the shim headers have no seam to couple into. Checked per instructions: no coupling. (Flag: if `rustty-wasm` ever links wuffs for wasm32, revisit) |
| `dff13b41c` | CI build fixes: afl++ archive+ubsan-rt+PIC bundling; wuffs `-fno-sanitize=undefined` on ALL Windows ABIs (was msvc-only) | **N-A** | rustty has no afl++ fuzz infra (find ∅ `*afl*` outside .git); `cc` never enables Zig's ubsan, so the Windows flag has no analogue |
| `ffad4c6ec` | wuffs dep URL: git+https mirror → `deps.files.ghostty.org` tarball, SAME rev `7411f488` (v0.4.0-alpha.10); rest of commit (build.zig.zon/nix/flatpak) out of area | **N-A** | rustty vendors the artifact in-tree (R3), no URL to update. Note: the pinned rev remains alpha.10 while rustty vendors alpha.9 — pre-existing Phase-4 debt (`7c4c7adad` PARTIAL), not created by this commit |

### 2h. Singletons

| Commit | What changed | Verdict | Evidence |
|---|---|---|---|
| `ef01d5fda` | gtk XKB remap: `key.zig` gains `remapKey(physical, keyval, is_modifier)` + `isModifierKeyval` (26 keyvals incl. `Mode_switch`, ISO level/group latches) — modifier keyvals ghostty can't represent yield `.unidentified`, never the physical key; `class/surface.zig` inline remap block replaced by the call; **+1 test "remap key"** (4 asserts) | **ABSENT** | `crates/rustty-apprt/src/gtk/key.rs:540` still the OLD inline `should_be_remappable` fallthrough (exact superseded shape); grep ∅ `remap_key\|is_modifier_keyval` |
| `6f02d9aad` | font: glyf renders directly into the output bitmap — `z2d.Surface.initBuffer` over a pre-allocated `alloc`'d buffer (errdefer free), no `dupe` copy | **ABSENT** | `crates/rustty-font/src/glyf_rasterize.rs:149` still `Surface::init(SurfaceType::ImageSurfaceAlpha8, …)` + copy-out (old shape) |

## 3. Atomic bundles (land together or be worse than absent)

- **D-B1 Kitty clipboard (11 commits + cross-area)**: the `read_clipboard` bool→
  `ClipboardReadResult` callback signature change breaks the embedded C ABI the moment
  either side moves alone; `ClipboardRequest.paste` gaining a payload touches every
  requester; `KittyRead/KittyWrite` + `ghostty.h` structs/enums + Surface.zig
  orchestration + termio messages + terminal `kitty/clipboard*.zig` are one motion. Port
  as the range-diff end state; the intermediate per-commit CAPI shapes (`0ce9054bf`'s
  15-line ghostty.h vs `af9470b19`'s 36) never need to exist.
- **D-B2 animation clock (`aee7bf347`+`c4e16970a`)**: `custom_shader_animation` LEAVES
  Thread.DerivedConfig in the same motion the renderer Options gains it;
  `syncDrawTimer`→`armAnimationTimer` and nullable-swap-chain touch the same
  `renderCallback` — port the merged end state (Thread.zig is 199 changed lines total).
- **D-B3 placement geometry (`cd5f9eef0`+`9490f7134`+`52190a5d8`+`c5a3c7e2e`)**: the 5
  new image.zig tests assert the END-state math (gridSize + intersect + clamp together).
- **D-B4 wuffs pixel (`322d7ae78`+`86db30785` + A's graphics_pixel/animation)**:
  animation frame compositing calls `rgbaSrcOver`; gray+alpha frames call the fixed
  `gaToRgba`.
- **D-B5 menu shortcuts (7 commits)**: tests assert the end shape (Identity enum +
  AppKit translation + isolation); `f6990690f`'s deletion rides along.

## 4. Cross-area couplings (blocking or ordering)

| This area's change | Needs | Owner |
|---|---|---|
| D-B1 structs/embedded CAPI (`ClipboardReadResult`, `ClipboardComplete/Confirm`, deny export, kitty request types) | `ghostty.h` enums/structs + `checkGhosttyHEnum`/size KATs + embed-ABI byte gate (memory `embed-abi-byvalue-sret-gate`) | **C-capi** |
| D-B1 orchestration | `src/Surface.zig` kitty clipboard flow (~620 lines across 5 commits) | **core** |
| D-B1 IO-thread half | termio `message.zig`/`stream_handler.zig` kitty_clipboard messages | **E/termio** |
| D-B1 protocol core | NEW `terminal/kitty/clipboard*.zig` ×5 + osc 5522 parser churn + `terminal/paste.zig` + stream_terminal ENOSYS + modes 5522 (G1b kitty 202 / osc 47 / stream_terminal 38) | **A/terminal** |
| `aee7bf347` animationTick | `graphics_storage`/`graphics_animation` tick API | **A/terminal** |
| D-B3 relative placements | `storage.resolveChain/placeholderTarget/PlacementKey/gridSize`; `c/kitty_graphics.zig` | **A** + **C** |
| `aee7bf347` config move | rustty-side: `RendererConfig` accessor trait row moves (renderer never names config types — memory `renderer-config-accessor-trait-acyclic`) | **D (self)** + config |
| `c4e16970a` apprt hoist | `apprt.must_draw_from_app_thread` const at the rustty apprt-api/apprt seam (today only the GTK-local const exists, `gtk/app.rs:319`) | **D (self)** |
| `86db30785` gaToRgba fix | possibly wuffs alpha.10 vendor bump (verify alpha.9 first) | **D (self)**, debt from Phase-4 `7c4c7adad` |

**SB/UB hazards (name them before porting):**

1. **`Origin.pin: *const Pin` + `pending_relative`** (D-B3): raw pin pointers obtained
   from `storage.placements` are held across the placeholder scan and re-derefed later
   (`pointFromPin`), while `virtual_origins`/`placeholderTarget` walk the same storage.
   In Rust this is exactly the raw-`Screen`/`Pin` aliasing class of the search-subsystem
   UB (memory `search-screen-sb-ub-finding`): no `&mut` to storage/screen may interleave
   with the held pins — use raw-ptr args per memory `rust-mut-args-strongly-protected`,
   and Miri-prove the loop on an isolated harness (mirihar doctrine).
2. **`KittyRead/KittyWrite.destroy`** frees an arena that owns the struct itself
   (self-referential free: move the arena out by value first, exactly as the Zig does) —
   created on the IO thread, destroyed on the app thread ⇒ Send + single-owner contract;
   needs LIFETIMES.tsv rows (R11 prospective prong) before the D-B1 port.
3. **Nullable swap chain vs in-flight completion callbacks** (`c4e16970a`): the
   completion handler derefs `self.swap_chain.?` AFTER `releaseGpuResources` could have
   run; the oracle's safety argument is "teardown waits on frame_sema which this callback
   posts". Rustty's generic.rs already passes raw self-pointers into callbacks
   (`generic.rs:5765`) — carry the invariant as a SAFETY comment + keep the
   draw_mutex/frame_sema ordering byte-faithful; ASan lane (FFI, Miri can't run Metal).
4. **Embedded callback-table ABI** (D-B1): bool→c_int return on `read_clipboard` and the
   two new by-value/pointer structs must byte-match the regenerated header (A4 gate +
   menu-shortcut-parity harness precedent).

## 5. Dropped-test mapping for this area (`w0/g1b-after-at-683d8db64.txt`)

| G1b row | drop | Source commit | The tests |
|---|---|---|---|
| `input_paste` | 7 | `a5bb22e23` | `isSafeWith`, `"encodeWriter bracketed"`, `"encodeWriter unbracketed"`, `"encodeWriter empty"`, `"encodeWriter chunks through a small writer buffer"`, `"encodeWriter too small"`, `max_frame_size` (`src/input/paste.zig`, oracle 17 vs rustty 10) |

That is this area's ONLY red G1b row. **Gate-invisible dropped tests this area must
carry anyway** (per memory `derived-gate-green-can-be-bookkeeping` — say so in the tasks'
negative controls):

- `src/renderer/image.zig` **+5 tests** (`aee7bf347` ×1, `9490f7134` ×3, `52190a5d8` ×1)
  — G1b has NO `renderer_image` module row at all.
- `src/apprt/gtk/key.zig` **+1 test** (`"remap key"`, `ef01d5fda`) — gtk modules have no
  G1b rows.
- `pkg/wuffs/src/swizzle.zig` **+2 tests** (`gaToRgba`, `rgbaSrcOver`) — the `wuffs` row
  reads ok (zig 8 ≤ rust 14) only because rustty carries extra KATs; the two NEW
  oracle tests are absent, and one existing rustty test
  (`ga_to_rgba_always_unsupported`) asserts the now-fixed BUG and must be replaced.
- Swift tests are entirely outside G1b (xcodebuild lane): `ScriptKeyEventCommandTests`
  NEW **16 @Test**, `KeyboardLayoutTests` NEW **3 @Test**, MenuShortcutManagerTests +15
  lines, NormalizedMenuShortcutKeyTests ±16 — plus one DELETION
  (`TerminalViewContainerTests.swift`) that a test-count-only view would misread as a
  regression when it is upstream-mandated.

## 6. Coverage honesty

**Exhaustive (read line-by-line):**
- The commit enumeration: 38/38 commits verdicted, spans noted; per-commit `--stat` read
  for every one.
- Full end-state range diffs for: `src/apprt.zig`, `src/apprt/structs.zig`,
  `src/apprt/surface.zig`, `src/apprt/gtk/key.zig`, `src/renderer/Thread.zig`,
  `src/renderer/generic.zig`, `src/renderer/OpenGL.zig`, `src/font/glyf_rasterize.zig`,
  `pkg/wuffs/*` (build.zig, zon, both shim headers, main.zig, swizzle.zig),
  `pkg/afl++/build.zig`, `macos/Sources/Helpers/KeyboardLayout.swift`,
  `Ghostty.MenuShortcutManager.swift`, `Ghostty.Input.swift`, `Ghostty.Config.swift`,
  `AppDelegate.swift`, `ScriptKeyEventCommand.swift`, and `src/input/paste.zig` (all
  hunks; test bodies past the first skimmed by name + assertion count).
- `src/renderer/image.zig`: the relative-placement half (+260 lines) read hunk-by-hunk;
  hunk map enumerated for the whole file.

**Sampled / inferred (stated as such):**
- The macOS Kitty-clipboard Swift diffs (`Ghostty.App.swift` 235 lines,
  `NSPasteboard+Extension` 85, `ClipboardConfirmationRequest` 77, GhosttyPackage 15) were
  characterized from per-commit stats + the apprt-side contract they consume, not read
  hunk-by-hunk; their ABSENT verdicts rest on the global no-commits-since-finale evidence
  plus cluster greps (`kittyRead|KittyClipboard|5522|uriList|ENOSYS` ∅ over
  `macos/Sources`).
- `src/apprt/embedded.zig` (332 diff lines): read via API-surface extraction (every
  added/removed `fn`/`export`/`const` line), not full-hunk.
- `fd17869d1` read via a filtered symbol diff (class/func/var lines), not full-hunk.
- Small macOS rows (`1d24eecb2`, `6cf7e0cc5`, `10d732688`, `a2212a5b1`, `550151882`,
  `df14efaf3`): verdicts rest on tree-untouched evidence, no per-row symbol grep.
- The 5 new image.zig test BODIES and the `animationTick` terminal-side contract were not
  read; assertion-for-assertion porting must read them at port time (R1).
- Nothing was built or run (assessment only); "old shape" claims are quotes of rustty
  source at the cited lines, not test executions.

## 7. Suggested ordering within D (non-binding)

1. **`86db30785` + `322d7ae78` wuffs slivers first** — smallest, fixes a live divergence
   rustty ships today, unblocks A's graphics_pixel/animation work; decide the alpha.10
   vendor question here.
2. **D-B1 Kitty clipboard as one C-synced batch** (with C's header rows, core's
   Surface.zig, E's termio, after/with A's `kitty/clipboard*` core) — clears G1b
   `input_paste` (a5bb22e23 rides here) and most of A's kitty/osc reds.
3. **D-B2 + D-B3 renderer end state** (after A's storage/animation APIs exist) — carry
   the 5 gate-invisible image tests + name the SB hazards in each task.
4. **D-B5 menu shortcuts + `f6990690f` revert + 2e AppleScript tests** (xcodebuild lane
   carries 19 new @Tests), then 2f glass rework (delete TerminalViewContainerTests).
5. `ef01d5fda` gtk remap and `6f02d9aad` glyf — independent, anytime.
