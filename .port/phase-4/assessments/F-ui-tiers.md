# Phase 4 assessment — area F: UI tiers (macOS Swift / apprt+gtk / font / inspector / pkg / build / packaging)

Range `6ad1fe7d8..12967b68f` (2026-07-29 → 2026-08-17). Owned paths (oracle-relative):

```
macos/**                 70 files, +1952 / −1386     src/apprt/**      21 files (~3.4k diff lines)
src/font/**               6 files (~350 diff lines)  src/inspector/**   3 files (~250 diff lines)
pkg/{macos,gtk4-layer-shell,android-ndk,wuffs}       src/build/**       7 files
dist/ flatpak/ snap/     (pins + packaging)
```

**93 non-merge commits** touch these paths (`git log --oneline --no-merges 6ad1fe7d8..12967b68f -- <paths>` = 93).
Rustty counterparts: `macos/` (Swift tree, Rustty-renamed), `crates/rustty-apprt`,
`crates/rustty-apprt-api`, `crates/rustty-apprt-c`, `crates/rustty-font`,
`crates/rustty-font-shaper`, `crates/rustty-inspector`, `crates-sys/wuffs-sys`, `xtask/`, `dist/`.

## PORT STATUS of the owned crates (checked first, per instructions)

- `crates/rustty-apprt/src/lib.rs:7` — "PORT STATUS: in-progress". The header's "GTK
  (a whole unported subsystem…)" line is **stale**: `src/gtk/` is substantially inhabited
  (23 class files; `application.rs` 5,425 lines, `surface.rs` 4,898, winproto/wayland with
  `hotkeys.rs`+`vicinae.rs` — the Phase-3 F-W hotkeys series LANDED). G3b treatment aside,
  the GTK tree is real code and the range's GTK work is assessed against it below.
- `crates/rustty-font/src/lib.rs:7` — "PORT STATUS: in progress".
- `crates/rustty-inspector/src/lib.rs:6` — "PORT STATUS: complete".
- `macos/` Swift tree: last substantive commits are Phase-3 remediation
  (`35e734e5`, `d8a7aad9` = P3-W8) + `8120dd10` (build scripts only). **No Phase-4-range
  macOS work has landed** — and the tree still contains files upstream DELETED in this range
  (`AnySortKey.swift`, `CrossKit.swift`, `HostingWindow.swift`, `App/iOS/iOSApp.swift`,
  `SurfaceView_UIKit.swift`, `App/macOS/` un-flattened).

## 1. Summary

| Sub-tier | PORTED | PARTIAL | ABSENT | N-A | rows |
|---|---|---|---|---|---|
| `macos/` (Swift) | 0 | 0 | 44 | 1 | 45 |
| `src/apprt` core + gtk | 0 | 0 | 26 | 1 | 27 |
| `src/font` + `pkg/macos` | 0 | 0 | 6 | 0 | 6 |
| `src/inspector` | 0 | 0 | 2 | 0 | 2 |
| `src/build` + pkg churn + dist/flatpak/snap | 0 | 2 | 3 | 8 | 13 |
| **Total** | **0** | **2** | **82** (81 unique + spans) | **9** | **93** |

Headline reads:

- **Nothing from this range has been ported.** Phase-4 porting has not begun in area F; the
  pin was advanced first (per README), so every verdict below is measured against the target
  oracle tree. The only two non-ABSENT functional rows are *coincidental equivalence*
  (rustty's xcframework was already iOS-free; wuffs-sys already consumes the release-C
  artifact shape upstream just switched to).
- **The macOS wave is the largest block** (~45 commits, +1,952/−1,386): a clipboard-
  confirmation architecture rework, an OSC-8 untrusted-URL policy engine (+368 new lines
  + 122 test lines), the iOS/CrossKit removal wave (−~1,000 lines, `OSColor→NSColor` sweep
  across ~20 files), a command-palette scoring rework with tests, an update-flow rework,
  a CommandLineOpenFileFilter (new file + 85 test lines), IME surrogate-pair/insertText
  reworks, and ~15 small crash/leak/warning fixes.
- **Two G1b RED lanes belong to this area** (see §5): `font_shaper_harfbuzz drop=1`
  (`ca8868a29`'s new test) and `apprt_api_surface drop=4` (`53c6fdbe7`'s four new tests).
- **The GTK wave is the second block**: split drag-and-drop (new drag-handle/drop-overlay
  widgets + CSS + blueprint), `+new-tab` CLI/D-Bus IPC (C-ABI size assertion changes),
  window-title renaming, open-config-in-window, audio-bell moved surface→application,
  native GTK blur on ≥4.23.3, split-sizing flicker fix, and several teardown-correctness
  fixes to machinery rustty ported in Phase 3 (WeakRef, Hotkeys.deinit, blur regions).

## 2. Per-commit verdicts

Legend: `→` = rustty location; "grep ∅" = the stated grep over the stated tree returned
nothing. All ABSENT verdicts on the macOS tree share one piece of global evidence in
addition to per-row greps: `git -C rustty log -- macos` shows no commits after `8120dd10`
(build scripts) / `35e734e5` (P3-W8), both predating this range's absorption.

### 2a. `macos/` — Swift tree (45 rows)

End-state assessed from the full 5,469-line range diff, read hunk-by-hunk. Where several
commits rewrite the same file, the END STATE governs and every commit is listed.

| Commit(s) | What changed (end state) | Verdict | Evidence / notes |
|---|---|---|---|
| `d695ffff3` + `57c1baf43` + parts of `97ae25749` | **Clipboard-confirmation rework** (atomic bundle B1): NEW `Ghostty.ClipboardConfirmationRequest.swift` (+114) — a one-shot request object owning callback state, deinit-safe cancellation; `SurfaceView.pendingClipboardConfirmation` @Published + didSet cancel; `BaseTerminalController` drops the NotificationCenter `confirmClipboard` path for a Combine `MergeMany`/`switchToLatest` publisher over the split tree, defers presentation until the surface is focused (rings bell as attention signal), replaces the visible sheet in place on overlapping requests; `ClipboardConfirmationController` takes the request object; `Ghostty.App.confirm_read_clipboard`/`write_clipboard` build requests instead of posting notifications; `Notification.confirmClipboard`+3 keys DELETED; `ClipboardRequest` moved out of `GhosttyPackage.swift` and its `osc_52_write(OSPasteboard?)` payload dropped | **ABSENT** | grep ∅ `ClipboardConfirmationRequest\|pendingClipboardConfirmation` over `macos/`; `macos/Sources/RusttyKit/RusttyPackage.swift` still defines `confirmClipboard` notification keys; `Features/ClipboardConfirmation/ClipboardConfirmationController.swift` still has `surface/contents/request/state` fields |
| `7a047553c` (+`e83cf0b06` interplay) | **IOSurface leak on automated surface creation**: `BaseTerminalController.controller(owning:)` — static weak-to-weak `NSMapTable<SurfaceView, BaseTerminalController>` maintained from `surfaceTree.didSet`; App Intents (`CloseTerminalIntent`, `FocusTerminalIntent`) + AppleScript `ScriptTerminal` route through it instead of `surfaceView.window?.windowController` | **ABSENT** | grep ∅ `surfaceControllers\|controller(owning` over `macos/` |
| `e83cf0b06` | Quit-alert-missing-when-hidden: `QuickTerminalController.confirmCloseAsync` override animates in before super; AppDelegate drops its all-windows-invisible early-terminate check | **ABSENT** | `macos/Sources/Features/QuickTerminal/QuickTerminalController.swift` has no `confirmCloseAsync` override (grep ∅) |
| `77537c806` | **Untrusted OSC-8 hyperlinks** (bundle B2, cross-area): NEW `UntrustedURL.swift` (+262: allow/confirm/deny policy, unsafe-scalar rejection, file classification via UTType + executable bit, `displayString` sanitizer), NEW `UntrustedURLAlert.swift` (+104: deferred modal because the core action callback holds the renderer mutex), NEW `UntrustedURLTests.swift` (+122); `Ghostty.App.openURL` routes `kind == .osc8` to `openUntrustedURL`; `URLHoverBanner` displays sanitized string; `Ghostty.Action.OpenURL.Kind` gains `.osc8`. Zig half: `src/apprt/action.zig` `OpenUrl.Kind.osc8` (+ C enum `GHOSTTY_ACTION_OPEN_URL_KIND_OSC8`, area C header) | **ABSENT** | rustty `macos/Sources/Helpers/` has `URLHoverBanner.swift` but no `UntrustedURL*.swift` (find ∅); `crates/rustty-apprt-api/src/action.rs` grep ∅ `osc8/Osc8`. Coupling: **C** (ghostty.h enum), **A/B** (core sets `.osc8` on OSC-8 link opens) |
| `90154e289` + `8c9fd7aa7` | **CommandLineOpenFileFilter** (NEW `App/CommandLineOpenFileFilter.swift`, end state 59 lines): consumes AppKit open-file events for existing paths after `-e`, once each, normalized as file URLs relative to CWD (tilde-expanded, standardized); lazily built in AppDelegate; `application(_:openFile:)` early-consumes. Tests: NEW `CommandLineOpenFileFilterTests.swift` (85 lines end state, 6 @Test) | **ABSENT** | find ∅ `CommandLineOpenFileFilter*` under rustty `macos/` |
| `daab08ec0` + `3901168b1` + `400be4cc1` + `d19f8f7f9` (bundle B3, iOS/CrossKit removal) | DELETE `CrossKit.swift` (OSView/OSColor/OSSize/OSPasteboard/OSApplication aliases), `SurfaceView_UIKit.swift`, `App/iOS/iOSApp.swift`, `HostingWindow.swift`; flatten `App/macOS/*` → `App/*`; `OSColor→NSColor`/`OSView→NSView`/`OSViewRepresentable→NSViewRepresentable` sweep across ~20 files; strip `#if os(macOS)`/`canImport(AppKit)` guards; `Ghostty.App` drops the iOS callback stubs; `Ghostty.Config` drops iOS arms; pbxproj drops the iOS target (−329) | **ABSENT** | rustty still HAS all four deleted files (`ls macos/Sources/Helpers/` shows `AnySortKey.swift CrossKit.swift HostingWindow.swift`; `macos/Sources/App/{iOS,macOS}` both exist; `find` hits `SurfaceView_UIKit.swift`, `iOSApp.swift`). README Phase-4 explicitly lists these as must-delete. R2 note: rustty's aliases live under `RusttyKit/` |
| `5c952ac97` | Command-palette sort keys: `AnySortKey.swift` DELETED; `CommandOption.sortKey: ObjectIdentifier?`; `sortedTerminalPaletteOptions` free function (`:`→`\t` normalize, case-insensitive compare, sortKey tiebreak); +26 test lines | **ABSENT** | rustty still has `macos/Sources/Helpers/AnySortKey.swift`; no `CommandPaletteTests.swift` in `macos/Tests/` (ls) |
| `d02ad967b` + `066a0b7c4` | Palette match order rework: `CommandOptionMatch` scored type (colorScore, textScore title=3 > subtitle=2 > description=1), `filteredAndSorted(query:)` collection extension; subtitle falls back to description in `CommandRow`; +79 test lines (`CommandPaletteTests.swift`) | **ABSENT** | grep ∅ `CommandOptionMatch\|filteredAndSorted` over `macos/` |
| `76907d8de` | Removes 30 lines of flaky color-match tests from `CommandPaletteTests.swift` | **N-A** | rustty never had `CommandPaletteTests.swift`; subsumed by porting the END-STATE test file from `d02ad967b`+`5c952ac97` |
| `ecbeb60ca` + `f4309055f` | **insertText/key-event rework**: ALL committed text (IME, dictation) sent as key events via renamed `committedTextAction` (was `committedPreeditTextAction`), never `surfaceModel.sendText`; encode-text guard becomes `!text.startsWithASCIIControlCharacter` (C0 or **DEL 0x7F**) — new `String.startsWithASCIIControlCharacter` in `String+Extension.swift` | **ABSENT** | rustty `SurfaceView_AppKit.swift` (P3-W8 state) still has `committedPreeditTextAction` + `hadMarkedText` branch + `sendText` fallthrough; grep ∅ `startsWithASCIIControlCharacter` |
| `f0e3be3ee` | UnicodeHexInput surrogate pairs: `LeadSurrogate`/`TrailSurrogate` structs; `insertText` pairs a stored lead with an arriving trail (`String(decoding:[lead,trail],as:UTF16.self)`), Terminal.app-style ignore of orphan trails | **ABSENT** | grep ∅ `LeadSurrogate` over `macos/` |
| `f2022fe88` + `97ae25749` | `showUserNotification` → async/await `UNUserNotificationCenter.add` with `[weak self]`, removes delivered notification if surface deallocated, warning fixes | **ABSENT** | rustty still has closure-callback `add(request) { @MainActor error in … }` shape (P3 state), no `Task { @MainActor [weak self]` around it |
| `44a05a88a` | Accessibility selection notification: `.map { _ in }` before `.debounce` so the debouncer does not retain the Notification (retain cycle) | **ABSENT** | rustty has the P3 `.filter{}.debounce.sink { _ in }` chain without the discarding `.map` |
| `d28bc121a` | `CachedValue<T>` gets an `NSLock` around get/expire/deinit | **ABSENT** | grep `lock` in rustty `SurfaceView_AppKit.swift` `CachedValue` ∅ |
| `74f01cf5d` + `49e4df783` | Search-state rework: `SearchState.Needle {text, selection}` struct replacing parallel `needle`/`needleSelection`, `setNeedle(_:selectAll:)`, `Ghostty.App.startSearch` uses `setNeedle`; stale-selection crash fix; `OSSurfaceView: NSView` direct; searchNeedle publisher `.map(\.text)`; ± test updates in `SurfaceView+SearchStateTests.swift` | **ABSENT** | grep ∅ `setNeedle\|struct Needle` over `macos/` |
| `74f91d1b4` | `drag-handle` config: `Ghostty.Config.dragHandle` (`DragHandle: always/auto/never`), `SurfaceGrabHandle` takes it and short-circuits visibility | **ABSENT** | grep ∅ `dragHandle\|DragHandle` over `macos/Sources` |
| `8696bef64` + `96826853b` | ObjC exception guard #2: `GhosttyShowWindowSafely` (ObjCExceptionCatcher.h/.m, shared `GhosttyErrorFromException`), `NSWindowController.showWindowSafely`, used by `TerminalController` tab presentation; split-drag window sizing computes `treeSize` BEFORE `TerminalController.init` | **ABSENT** | rustty `macos/Sources/Helpers/ObjCExceptionCatcher.m` has only `AddTabbedWindowSafely` (grep ∅ `ShowWindowSafely`); R2: export as `RusttyShowWindowSafely` |
| `c93752a00` + `a86c49d7a` + `63d08c034` + `3a606c6c4` + `cc1d26210` + `cb7eaa059` + `0c8ec225b` (update-flow bundle B4) | Update rework end state: `UpdateState.Installing{appcastItem: SUAppcastItem?, retryTerminatingApplication}` (drops `isAutoUpdate`/`dismiss`); `shouldTerminateWithoutWarning`; `isIdle`→`isHidden` (pill hidden during silent auto-update install); `isCancellable`/`cancelAction`; mutating `confirm()`; `UpdateController.checkForUpdates` shows a "Restart Required" NSAlert w/ `InstallingAccessoryView` when already installed; `installUpdate`/`validateMenuItem` DELETED; palette shows Cancel only when cancellable; UpdatePill weak fixes; error-pill dismiss acknowledges; ± tests (`UpdateStateTests.swift` ~35, `UpdateViewModelTests.swift` 4) | **ABSENT** | rustty has `macos/Tests/Update/` from P3 but grep ∅ `shouldTerminateWithoutWarning\|isHidden\|appcastItem` over `macos/` |
| `2b6a1e41f` | `GlobalEventTap.enable` prompts once via `AXIsProcessTrustedWithOptions` and polls `AXIsProcessTrusted` instead of retrying tap creation (each failed creation leaks a Mach port) | **ABSENT** | grep ∅ `AXIsProcessTrusted` in rustty `GlobalEventTap.swift` |
| `da8b17126` | `nonisolated(unsafe)` on `Ghostty.Surface.surface` + `Ghostty.Inspector.inspector`; deletes the `@retroactive Sendable` extensions on `ghostty_command_s`/`ghostty_surface_t` | **ABSENT** | grep ∅ `nonisolated(unsafe)` in `Rustty.Surface.swift` |
| `c78226bfa` | `SurfaceView.find(uuid:)` main-actor warning fix in OSSurfaceView | **ABSENT** | mechanical; tree untouched |
| `fd9837021` | swiftlint fix in SurfaceView_AppKit | **ABSENT** | mechanical; tree untouched |
| `e11bfb513` | `TerminalController.showWindow` calls `syncAppearance()` after super | **ABSENT** | grep: rustty `showWindow` has no trailing `syncAppearance()` |
| `18f06ef03` | Unsupported-action fallthrough fix: `TOGGLE_TAB_OVERVIEW`/`TOGGLE_WINDOW_DECORATIONS`/`SIZE_LIMIT`/`QUIT_TIMER` no longer fall through into `showChildExited` | **ABSENT** | real correctness bug reproduced in rustty's copy of the switch |
| `301bd6f8b` | `NSMenuItem` `preferredImageVisibility = .automatic` on macOS 27 (`#if compiler(>=6.4)`) | **ABSENT** | grep ∅ `preferredImageVisibility` |
| `4b9d589bc` | `BackportSelectionTextField` availability forced macOS 15→26 (SwiftUI selection crash on 15) | **ABSENT** | rustty Backport still `#available(iOS 18.0, macOS 15, *)` |
| `da745630b` | Auto-tab check only when `NSWindow.userTabbingPreference == .always` (avoids ~15-20 ms tabGroup materialization) | **ABSENT** | grep ∅ `userTabbingPreference` |
| `4b1e02c7c` | ConfigurationErrorsController: `performClose` only `if isWindowLoaded` (avoids nib load on every launch) | **ABSENT** | grep: rustty file lacks `isWindowLoaded` guard |
| `cfa0ca710` | TransparentTitlebar KVO rebinding deferred to next main-queue turn + `observedTabGroup` identity check | **ABSENT** | grep ∅ `observedTabGroup` |
| `8b7c57c75` (Swift sliver) | `Ghostty.Command.unsupportedActionKeys += "prompt_window_title"` | **ABSENT** | with the GTK row below; grep ∅ `prompt_window_title` in `Rustty.Command.swift` |
| `ad96613a8` (Swift sliver) | `GHOSTTY_ACTION_EXPORT_TERMINAL_IO` handler: copies borrowed bytes, NSSavePanel sheet, atomic write | **ABSENT** | with §2d row; grep ∅ `exportTerminalIO` over `macos/` |

### 2b. `src/apprt` — core seam files (`action.zig`, `ipc.zig`, `surface.zig`, `embedded.zig`)

| Commit(s) | What changed | Verdict | Evidence | Guarding test |
|---|---|---|---|---|
| `53c6fdbe7` | **apprt owns desktop-notification truncation**: `Message.DesktopNotification` struct with `init(title, body)` + `copyUtf8Z` (backs up over UTF-8 continuation bytes so truncation never splits a codepoint) + `isUtf8ContinuationByte`; 4 new tests (init, per-sequence-length boundary, truncation boundary, fits) | **ABSENT** | `crates/rustty-apprt-api/src/surface.rs:83` still bare `DesktopNotification { title: [u8; 64], body: [u8; 256] }`; grep ∅ `copy_utf8` workspace-wide. The old truncation still lives at the termio call site (`crates/rustty-termio/src/stream_handler.rs:2194-2208` quotes the superseded Zig) | **This is G1b `apprt_api_surface drop=4`** — the four tests land here. Coupling **E** (termio stream handler must switch to `Message::DesktopNotification::init`) |
| `77537c806` (Zig sliver) | `OpenUrl.Kind.osc8` + doc | **ABSENT** | grep ∅ `osc8` in `rustty-apprt-api/src/action.rs` | `ghostty.h` enum KAT (coupling **C**) |
| `8b7c57c75` (action sliver) | `set_window_title: SetTitle` action + `PromptTitle.window` | **ABSENT** | `action.rs:952-955` `PromptTitle { Surface, Tab }` — no `Window`; grep ∅ `set_window_title` | `checkGhosttyHEnum` KATs (coupling **C**) |
| `0a183c923` (action sliver) | `open_config` becomes payload-carrying `open_config: OpenConfig` (`os_open`/`new_window`); `embedded.zig` `ghostty_app_open_config` passes `.new_window`; CValue size assert 4-byte arm 16→24 | **ABSENT** | `action.rs:743` `Action::OpenConfig => ActionCValue { open_config: () }` — still void; `crates/rustty-apprt-c/src/lib.rs:692-698` still quotes `.open_config, {}` | `ghostty.h` `GHOSTTY_ACTION_OPEN_CONFIG_*` KAT (coupling **C**); CoreApp `Message.OpenConfig` (coupling **A**/core), `internal_os.getConfigEditCommand` = `os/edit.zig` (coupling **E**) |
| `25b2d8a38` | `move_tab_to_new_window` action key (+ gtk unimplemented arm at intro) | **ABSENT** | grep ∅ `move_tab_to_new_window` in `rustty-apprt-api` | header KAT (coupling **C**) |
| `ad96613a8` (action sliver) | `export_terminal_io: ExportTerminalIO {contents: []const u8}` + C struct + format | **ABSENT** | grep ∅ `export_terminal_io\|ExportTerminalIO` over `crates/` | header KAT (coupling **C**) |
| `034506f14` (ipc sliver) | `apprt.ipc.Action.NewTab {surface_id: u64, arguments}` + C struct; NewWindow doc rewrite; **CValue size assert 4→12 / 8→16**; embedded `performIpc` gains `.new_tab => return false` | **ABSENT** | grep ∅ `new_tab\|NewTab\|surface_id` in `crates/rustty-apprt-api/src/ipc.rs`; `embedded.rs` lacks the arm | size assert is its own KAT; coupling **E** (cli `+new-tab`), **C** (`ghostty_ipc_action_tag_e`) |
| `db6d20dce` | embedded TIS keymap lazy: `keymap: ?input.Keymap` init'd on first `keyboardLayout()` call (multi-ms first TIS call), `reloadKeymap` no-ops when uninitialized, `keyboardLayout` takes `*App` | **ABSENT** | `crates/rustty-apprt/src/embedded.rs:158` `pub keymap: Keymap` — eager, non-optional | none upstream — needs one (a lazy-init KAT is cheap: assert `keymap.is_none()` after `App::init`) |

### 2c. `src/apprt/gtk` (+ `pkg/gtk4-layer-shell`)

| Commit(s) | What changed | Verdict | Evidence | Guarding test |
|---|---|---|---|---|
| `034506f14` + `3650bef4e` (gtk half, bundle B5) | `app.new-tab` GAction (`(tas)` variant: surface-id + args), `parseOverrides` factored from `actionNewWindow` (adds `--shell-integration=` via `cli.args.parseIntoField`), fallback to new window when no surface; NEW `gtk/ipc/new_tab.zig` (71 lines, D-Bus `org.gtk.Actions.Activate new-tab`); `Surface.applyCommandOverrides` — explicit `-e` command forces `shell-integration = .detect` unless configured `.none` (**fixes forced-integration inheritance**) + **test `"command and shell integration overrides"`** (7 assertions); overrides plumbed through `SplitTree.newSplit`/`Tab.new`/`Window.newTab{,ForWindow,Page}` | **ABSENT** | no `crates/rustty-apprt/src/gtk/ipc*` (ls fails); grep ∅ `parse_overrides\|apply_command_overrides\|shell_integration` in `gtk/class/application.rs`/`surface.rs` | upstream's `applyCommandOverrides` test ports verbatim (pure over Config); D-Bus path is live-VM only. Coupling **E** (cli `+new-tab` action + `App.zig` `performIpc` dispatch), **A** (`core_app.findSurfaceByID`/`focusedSurface`) |
| `8b7c57c75` + parts of `0a183c923` (gtk) | Window-title renaming: `Window.title-override` property + `setTitleOverride` + `promptWindowTitle` + `closureTitle`; `TitleDialog.Target.window`; `win.prompt-window-title` action; blueprint menu rows; `Action.setWindowTitle`/`prompt_title .window` arms | **ABSENT** | grep ∅ `prompt_window_title\|title_override` in `gtk/class/window.rs` | none upstream; a `set_title_override` property KAT is cheap |
| `0a183c923` (gtk half) | `app.open-config` action takes a string param (`os-open`/`new-window`); accelerators split `app.open-config::os-open` / `::new-window`; blueprint Config submenu; `Action.openConfig(self, value)` `.new_window` arm spawns `/bin/sh -c $(getConfigEditCommand)` window titled "Editing configuration file …" | **ABSENT** | grep ∅ `os-open\|os_open` in `gtk/class/application.rs` | coupling **E** (`os/edit.zig` `getConfigEditCommand`), **G** (i18n msgids) |
| `1d053bd6e` + `98fae16a0` + `850ca8c7b` + `65a3e666e` + `90da3aba5` (bundle B6, split drag-and-drop + sizing) | End state: surface.blp gains `drag_handle` CenterBox (DragSource, grab cursor), `drop_overlay` DrawingArea, `surface_drop_target` (uint64 surface-id payload, preload, self-drop reject via `notify::value`); `surfaceDragPrepare/Begin` (0.2× WidgetPaintable preview), `surfaceDrop/Leave/Motion`, `calcDropDirection`, `setDropOverlayDirection`; `closureShouldDragHandleBeShown` (config `drag-handle`); `SplitTree.moveSplit` (same-tree and **cross-tree** move; re-binds `is-split` via new `Surface.bindIsSplit` which unbinds the stale `is_split_binding`); css/style.css + application CSS drop-gradient classes; **split flicker fix**: `SplitTreeSplit` caches `ratio`, `syncSplitRatio(.tree_to_widget)` called synchronously from `propMaxPosition` during size-allocate, `setSizeRequest(1,1)` on surface children, idle handler reduced to `.widget_to_tree` | **ABSENT** | grep ∅ `bind_is_split\|move_split\|sync_split_ratio\|drag_handle\|drop_overlay\|surface_drop` over `crates/rustty-apprt/src/gtk`; rustty vendors `.blp`/`.css` under `crates/rustty-apprt/assets/{ui,css}` so the template/CSS halves land there | none upstream (widget-tree); `calcDropDirection` is pure and unit-testable; `moveSplit` tree-surgery testable headlessly against `Surface.Tree` |
| `f6fca9aab` | `Window.moveTabToNewWindow` (transferPage to fresh window; skip if single tab) + application dispatch arm | **ABSENT** | grep ∅ `move_tab_to_new_window` in `gtk/class/window.rs` | pairs with `25b2d8a38` (action key) |
| `f0f3f4d8d` + `f5419b9b1` (bell bundle B7) | Audio bell moved surface→application: `Application.Private.bell_media` + `app.ring-bell` action (`actionRingBell`); `Surface.ringBell` now only emits + activates `tab.`/`win.`/`app.ring-bell`; surface's `bell_media` field + audio block DELETED; separately: don't enter `bell_ringing` state when already focused (`if (self.getFocused()) return` after ring) | **ABSENT** | `crates/rustty-apprt/src/gtk/class/surface.rs:394` still owns `bell_media: RefCell<Option<gtk4::MediaFile>>`; `application.rs` grep ∅ `bell_media`; `ring_bell` in application.rs is the ACTION dispatch, not the app-owned audio | none upstream; rustty can KAT the focused-no-ring predicate |
| `3263fc6c4` + `04f1bc096` (blur bundle B8) | Native GTK blur ≥4.23.3: `Surface.snapshot` virtual override (`pushCopy`/`pushBlur(radius)`/`appendPaste`, compile-gated ≥4.22); winproto/wayland `Window.init` skips custom `ext_background_effect` when `runtimeAtLeast(4,23,3)` | **ABSENT** | grep ∅ `push_blur\|append_paste` in `gtk/class/surface.rs`; grep ∅ `runtime_at_least(4, 23` under `gtk/` | **atomic pair** — porting only `3263fc6c4` double-blurs on new GTK; only `04f1bc096` removes blur entirely there |
| `a6fc3e0e2` | Blur-region cleanup: `errdefer`→`defer wl_region.destroy()` (wl_region leaked on success path) + `self.blur_region.deinit(alloc)` before overwriting | **ABSENT** | rustty `gtk/winproto/wayland.rs:549-659` has `blur_region` + `clear()` in deinit but the set-path leak fix text is absent (no `defer`-equivalent destroy after `set_blur_region`); verify at port time against `set_blur_region`'s actual ownership | leak gate (valgrind lane) if any |
| `1dbc8ca30` | `WeakRef.deinit` (named clear-before-free; big safety doc) used at 4 teardown sites (application config_errors_dialog, command_palette jump surface, split_tree last_focused, window command_palette) | **ABSENT** | grep `fn deinit` in `crates/rustty-apprt/src/gtk/weak_ref.rs` ∅ | none upstream; rustty's WeakRef wrapper should get the same named method + doc (R1) |
| `fd47b15cd` | `Hotkeys.deinit` frees entries directly (`entry.deinit()` + `clearAndFree`) instead of `self.clear()` — clear() re-registers empty state and the old path leaked the entries' memory on app teardown | **ABSENT** | `crates/rustty-apprt/src/gtk/winproto/wayland/hotkeys.rs:265-267` — `deinit` still `self.clear()` (exact superseded shape) | Phase-3 F-W4 tests cover behavior; teardown leak needs the GTK leak lane |
| `f17b425aa` | Command-palette jump `sort_key` = `surface.core().?.id` (stable u64) instead of `@intFromPtr(surface)` | **ABSENT** | `crates/rustty-apprt/src/gtk/class/command_palette.rs:677-678` — still `surface.as_ptr() as usize` with the OLD oracle line quoted | coupling **A** (needs `CoreSurface.id` — added by the range's core `surface: use id` work) |
| `488b7109b` | Middle-click forwarded to TUIs when `core_surface.mouseReportingActive()` (both press and release paths) | **ABSENT** | grep ∅ `mouse_reporting_active` over `crates/` | coupling **A** (`Surface.mouseReportingActive` core helper) |
| `2c0d2588a` + `ad96613a8` (gtk half) | Export-terminal-IO: `Action.exportTerminalIO` — dup contents, `gtk.FileChooserNative` save dialog, `ExportTerminalIORequest` heap ctx w/ destroyData, `file.replaceContents`; type-safe FileChooser cast folded in | **ABSENT** | grep ∅ `export_terminal_io` under `gtk/` | bundle with §2d + action row |
| `7a512c312` | Banner titles "Command Succeeded"/"Command Failed" (capitalization) | **ABSENT** | `gtk/class/surface_child_exited.rs:154` still `c"Command succeeded"` | i18n msgid change — coupling **G** (po/) |
| `261848c2f` | Blueprint reformat (5 .blp: multi-line closure binds) | **ABSENT** (mechanical) | rustty vendors the .blp files under `crates/rustty-apprt/assets/ui/` — sync text verbatim | gresource build is the gate |
| `1d6bc6829` | `lib.cutPrefix` → `std.mem.cutPrefix` in application.zig | **N-A** | Zig-std migration; rustty uses `str::strip_prefix`. (The `src/lib/string.zig` deletion itself is area C's row) | — |
| `b9113d2e7` (gtk4-layer-shell half) | `pkg/gtk4-layer-shell/build.zig` rework: `LocalDeps` cached struct so the translate-c module ALSO gets local include paths when not using system integration (fixes local build) | **N-A** | rustty links the SYSTEM gtk4-layer-shell via the `gtk4-layer-shell` crate (`crates/rustty-apprt/Cargo.toml:78,348-349` — pkg-config `gtk4-layer-shell-0`); no vendored-source build path exists | — |

### 2d. `src/inspector`

| Commit(s) | What changed | Verdict | Evidence | Guarding test |
|---|---|---|---|---|
| `2349e974b` | DELETE `DetachableHeaderState` + `detachableHeader` (89 lines, obsolete since the widgets moved to per-window) | **ABSENT** (deletion not performed) | `crates/rustty-inspector/src/widgets.rs:340` still defines `DetachableHeaderState` with a "LIVE" header note at `:6` — that note is now wrong against the new pin; delete or justify per README | `cargo build -p rustty-inspector` catches dangling users after deletion |
| `ad96613a8` (inspector half) | termio Stream: `copyEvents` (via `rt_surface.setClipboard`), `exportEvents` (via `rt_app.performAction(.export_terminal_io)`), `formatEvents` (TSV, oldest-first), Copy/"Export to file" buttons; `Stream.draw` takes `*Surface` (was `alloc`); `widgets/surface.zig` `draw` takes `*Surface` (was `*const`) | **ABSENT** | grep ∅ `format_events\|copy_events\|export_events` in `crates/rustty-inspector/src/widgets/termio.rs` | atomic with the action + gtk + macOS arms (B9): inspector emits, apprts consume |
| `68beeeb3f` (inspector sliver) | `Stream.init`: `.initAlloc(alloc, handler)` → `.init(.{ .allocator, .handler })` (stream-continuation API change) | **ABSENT** | `crates/rustty-inspector/src/widgets/termio.rs:121-124` still `VtStream::init_alloc` quoting the superseded Zig | coupling **A** (`src/terminal/stream_continuation.zig` + Stream init options rework is area A's row; this call site must move with it — build breaks if desynced) |

### 2e. `src/font` + `pkg/macos`

| Commit(s) | What changed | Verdict | Evidence | Guarding test |
|---|---|---|---|---|
| `ca8868a29` | **Shaper grapheme-candidate allocations eliminated**: `run.zig` `fontIndexForGrapheme` rewritten from build-candidates-Vec + second pass into a single labeled `candidate:` loop (try primary, then each additional codepoint's font, first that covers the whole grapheme wins — same selection semantics, zero allocation); NEW harfbuzz test `"shaper selects font for entire grapheme"` (`#` + U+20E3 keycap; asserts run.font_index == the additional codepoint's font) | **ABSENT** | `crates/rustty-font-shaper/src/shaper/run.rs:520-524` still builds `candidates: Vec` quoting the superseded Zig; grep ∅ "selects font for entire grapheme" | **This is G1b `font_shaper_harfbuzz drop=1`.** The test needs `terminal.RenderState` + `t.vtStream()` scaffolding — coupling **A** |
| `afc79b8cc` | Apple-Color-Emoji exact-name lookup: `pkg/macos` `Font.createWithName`; `discovery.CoreText.discoverExactFamily` (CTFontCreateWithName + verify family name — CT substitutes rather than returning null); `SharedGridSet.collection` fast path `@hasDecl(Discover, "discoverExactFamily")` | **ABSENT** | grep ∅ `discover_exact_family\|warmup` over `crates/rustty-font{,-shaper}/src` | none upstream; the family-verify logic is KAT-able on macOS |
| `c454a3bf4` | `discovery.CoreText.warmup()` — one throwaway `CTFontCreateWithName` to init the system font registry (first query costs ms) | **ABSENT** | same grep ∅ | coupling: the warmup **thread** spawn is in core app/Surface (other area); this half is the pure fn |
| `daeed25b3` | CoreText creation OOM-safety: `String.createWithCharactersNoCopy` and `Typesetter.createLine` return `Allocator.Error!*T` (`orelse return error.OutOfMemory`); shaper coretext adds two `try`s | **ABSENT** | `crates/rustty-font-shaper/src/shaper/coretext.rs:622` quotes the OLD non-try signature; rustty's core-text binding calls must null-check | R7/R9: null-deref → panic path; unit-testable only by fault injection — document if not gateable |
| `d166c05ed` | `Font.copyDisplayName` → `?*String`; `DeferredFace.name` falls back to `familyName(buf)` when null | **ABSENT** | `crates/rustty-font-shaper/src/deferred_face.rs:278-284` PORT-NOTE quotes the old non-optional `copyDisplayName` contract | none upstream |
| `a177ba90a` | `pkg/macos/video/display_link.zig`: `Error.CreationFailed` replaces the `OutOfMemory` mis-mapping (renderer call site tolerating failure is `src/renderer`, not owned here) | **ABSENT** | `crates/rustty-renderer/src/metal/cvdisplaylink.rs:148-156` PORT NOTE explicitly preserves the `InvalidOperation -> OutOfMemory` mapping "keeping `try … catch error.OutOfMemory` faithful" — that note is now invalidated by upstream; delete it with the fix (same trap as Phase-3's metal/buffer quirk note) | coupling: the tolerate-at-call-site half lives in `src/renderer` (whichever area owns renderer this phase) |

### 2f. `src/build` + pkg churn + dist/flatpak/snap

| Commit(s) | What changed | Verdict | Evidence | Notes |
|---|---|---|---|---|
| `1fdbb8c91` + `87f69a12e` + `49fd1ae65` (Config.zig, lib-vt bundle B10) | `-Dvt-features` (`TerminalBuildOptions.Features.parse`, `-Dcpu`-style `+feat/-feat/all`; app always all-on, lib artifacts trimmable); wasm targets default-add `simd128` unless CPU explicitly set (50%+ vt_write win); `emit-lib-vt` defaults to **true when built as a dependency** | `1fdbb8c91`/`87f69a12e` **ABSENT**, `49fd1ae65` **N-A** | grep ∅ `vt_features\|vt-features` over `crates/rustty-build-config`, `xtask`; grep ∅ `simd128` over `xtask`, `crates/rustty-wasm` | vt-features = cargo feature-gates on `rustty-vt`/`rustty-lib` — **coupling C** (the Features struct + gated modules are C/A's terminal-side rows; this is the build plumbing). simd128 = `-Ctarget-feature=+simd128` in the wasm build profile. Dep-default N-A: cargo consumers pick crates directly |
| `5364e5158` | Wasm stack reservation 1 MB → 128 KB (`exe.stack_size`, measured 17 KB peak) | **ABSENT** | grep ∅ `stack_size` in `crates/rustty-wasm`, `xtask` | rustty analogue: `-Clink-arg=-zstack-size=131072` for the wasm artifact (`rustty-wasm` is "COMPLETE (WS14)" so this is a real regression against target) |
| `7a171895d` + `3901168b1` (build half) + `aa7497128` | Full-app iOS support removed: xcframework macOS-only, `MetallibStep` iOS arms deleted, `build.nu` change, hard error for iOS non-lib-vt builds; lib-vt iOS deployment target 13.0 via `osVersionMinLibVt` | **PARTIAL** (equivalent-by-construction where it counts) | rustty's `dist/RusttyKit.xcframework` contains only `macos-arm64` (ls) — never built iOS slices, so the END STATE (no iOS in xcframework) already holds; rustty ships **no iOS lib-vt artifact at all**, so `osVersionMinLibVt` has no analogue (`aa7497128` = N-A). Residual: the SWIFT iOS files still present (§2a bundle B3) are the real gap | `aa7497128` counted N-A |
| `7c4c7adad` | wuffs source moved to `google/wuffs-mirror-release-c` @ `v0.4.0-alpha.10` (7411f488) | **PARTIAL** | `crates-sys/wuffs-sys/build.rs:1-12` — already vendors the **release-C artifact** (the exact thing the mirror repo publishes), i.e. the mechanism matches; but the vendored copy is **v0.4.0-alpha.9**, one release behind the new pin | bump the vendored `wuffs-v0.4.c` to alpha.10; gate = `rustty-wuffs`/zigimg decode KATs + G1b `wuffs` row |
| `c75559589` | lib-c-docs Dockerfile copies `src/terminal/c/types.schema.json` (ABI manifest) | **N-A** | docs-docker infra; rustty has no lib-c-docs docker. The schema itself is area **C**'s row (`c_abi.zig`/types.schema.json) | flag to C: if rustty adopts the manifest, doc pipeline may want it too |
| `16e13a59a` | android-ndk build: Linux SDK fallback dir `Android/sdk`→`Android/Sdk` (merged with windows arm) | **N-A** | rustty has no Android NDK build infra (grep `android` hits only config conditionals + kitty img comment) | — |
| `b9113d2e7` (non-layer-shell half) | flatpak/snap Zig 0.16 pins; snap resolv.conf workaround; Config.zig patchelf split into `-Dpatch-interp`/`-Dpatch-rpath` (restores rpath opt) | **N-A** | rustty has no `flatpak/`/`snap/` dirs (ls) and no patchelf step (cargo/xtask builds) | if Linux packaging is ever added, revisit |
| `f2897f31d`, `7e8b4f360` | iTerm2 color-scheme dep bumps (flatpak zig-packages.json mirror of build.zig.zon) | **N-A** here | the authoritative themes pin is `build.zig.zon` (area **G**); `xtask/src/res_themes.rs` grep ∅ for the release URL — G should verify rustty's themes snapshot matches the new `20260810-152212-0173c3c` release | cross-ref to **G** |
| `df23bef0e` (build half) | `GhosttyI18n`: xgettext `--keyword=N_` + scan `src/input/command.zig` (command-palette i18n) | **ABSENT** | `xtask/src/translations.rs:201-209` has `--keyword=_`, `--keyword=C_:1c,2`, `--keyword=gettext_` — no `N_`, no command-source input | coupling **G** (po/ wave + Latvian) and **E/input** (`command.zig` gaining `N_()` markers is input's row) |

## 3. Atomic bundles (land together or be worse than absent)

- **B1 clipboard-confirmation rework** (`d695ffff3`+`57c1baf43`): the notification path is
  DELETED in the same change that adds the publisher path — a partial port strands OSC-52
  reads with no UI. Port as one unit incl. `Ghostty.ClipboardConfirmationRequest.swift`.
- **B2 untrusted OSC-8** (`77537c806` Swift+Zig+header): the Zig `.osc8` kind, the C enum
  (area C), the core producer (area A/B), and the Swift consumer must move together; a kind
  the switch doesn't know falls to `.unknown` → the *unrestricted* opener, i.e. shipping the
  enum without the handler silently drops the security policy. GTK: the value is additive
  (GTK has no osc8 arm upstream — verify before inventing one).
- **B3 iOS/CrossKit removal** (`3901168b1`+`daab08ec0`+`400be4cc1`+`d19f8f7f9`+`5c952ac97`'s
  AnySortKey): deleting `CrossKit.swift` breaks every `OSColor` reference unless the
  `→NSColor` sweep lands simultaneously; pbxproj must drop the files in the same commit or
  the Xcode build fails.
- **B4 update-flow rework** (7 commits, §2a): `Installing` loses `dismiss` and gains
  `appcastItem` in one motion; the tests (`UpdateStateTests`) assert the END shape.
- **B5 gtk new-tab** (`034506f14`+`3650bef4e` + E's cli action): ipc.zig CValue size assert
  (8→16) fails the moment `NewTab` is added without the C header struct (area C) and vice
  versa; `applyCommandOverrides` + its test ride along.
- **B6 split DnD + sizing** (`1d053bd6e`+`98fae16a0`+`850ca8c7b`+`65a3e666e`+`90da3aba5`):
  cross-tree drop calls `moveSplit` which calls `bindIsSplit`; blueprint child bindings
  (`drag_handle`, `drop_overlay`, `surface_drop_target`) panic template init if the .blp and
  the class disagree.
- **B7 bell relocation** (`f0f3f4d8d`+`f5419b9b1`): surface loses the `bell_media` field in
  the same change the application gains it; porting only one side leaks or silences audio.
- **B8 blur pair** (`3263fc6c4`+`04f1bc096`): see row — double-blur vs no-blur hazards.
- **B9 export-terminal-io** (`ad96613a8`+`2c0d2588a` across inspector/action/gtk/macos/C):
  inspector emits `performAction(.export_terminal_io)`; without a consumer it logs
  "runtime does not support" (safe), but the action enum + header KAT must move as one.
- **B10 lib-vt build flags** (`1fdbb8c91`+`87f69a12e`+`49fd1ae65`): `-Dvt-features` parse
  errors reference the Features struct (area C/A); port after the terminal-side features
  exist.

## 4. Cross-area couplings (blocking or ordering)

| This area's change | Needs | Owner |
|---|---|---|
| `53c6fdbe7` DesktopNotification truncation (+4 tests, G1b) | termio stream-handler call site switches to `Message::DesktopNotification::init` (`rustty-termio/src/stream_handler.rs:2194`) | **E** |
| `034506f14` gtk `+new-tab` | cli `+new-tab` action (`src/cli/new_tab.zig`, NEW) + `App.performIpc` dispatch | **E** |
| `0a183c923` open-config-in-window | `CoreApp.Message.OpenConfig`, `os/edit.zig getConfigEditCommand` | **A** (core) / **E** (os) |
| `ca8868a29` harfbuzz test (G1b) | `terminal.RenderState` + `Terminal.vtStream()` test scaffolding | **A** |
| `68beeeb3f` inspector Stream.init | `stream_continuation.zig` Stream options rework | **A** |
| `f17b425aa` palette sort key | `CoreSurface.id: u64` | **A** |
| `488b7109b` middle-click | `Surface.mouseReportingActive()` core helper | **A** |
| `77537c806` osc8 | `GHOSTTY_ACTION_OPEN_URL_KIND_OSC8` header + core producer | **C** + **A/B** |
| action/ipc enum adds (`export_terminal_io`, `set_window_title`, `move_tab_to_new_window`, `OpenConfig`, `NewTab`) | ghostty.h enums/structs + `checkGhosttyHEnum` KATs; ipc CValue size 8→16 | **C** |
| `1fdbb8c91` -Dvt-features | `TerminalBuildOptions.Features` + feature-gated vt modules | **C**/**A** |
| `c454a3bf4` warmup | warmup-thread spawn in core App | **A** (core) |
| `a177ba90a` display link | renderer call-site tolerance (`src/renderer`) | renderer owner |
| i18n (`df23bef0e`, `7a512c312`, new gtk msgids) | po/ catalogs + xtask translations | **G** |
| themes pins (`f2897f31d`,`7e8b4f360`) | build.zig.zon themes release verification | **G** |

**UB/SB hazards:** none of this area's changes touch Page/Row/Cell/Screen raw-pointer
seams. Two adjacent hazards worth naming: (1) the `ca8868a29` test drives
`RenderState.update(&t)` then reads `state.row_data` — when porting, keep the established
raw-ptr discipline of the A-side RenderState tests rather than holding `&mut Terminal`
across the read; (2) GTK teardown fixes (`1dbc8ca30`, `fd47b15cd`) are exactly the class of
freed-GWeakRef/lock-bit UB the upstream doc comment describes — rustty's gtk4-rs bindings
may make `1dbc8ca30` partially moot (glib::WeakRef manages registration), so verify against
rustty's own `weak_ref.rs` wrapper semantics, don't blind-copy.

## 5. Dropped-test targets from `w0/g1b-after-at-12967b68f.txt` owned by F

| G1b row | drop | Source commit | The tests |
|---|---|---|---|
| `font_shaper_harfbuzz` | 1 | `ca8868a29` | `"shaper selects font for entire grapheme"` (`src/font/shaper/harfbuzz.zig:865`) |
| `apprt_api_surface` | 4 | `53c6fdbe7` | `"DesktopNotification init"`, `"copyUtf8Z handles len at the final byte of every UTF-8 sequence length"`, `"copyUtf8Z keeps a complete codepoint at the truncation boundary"`, `"copyUtf8Z preserves UTF-8 that fits"` (`src/apprt/surface.zig:233-306`) |

Both rows go green only with their functional rows above. Not in G1b but must be carried:
the gtk `"command and shell integration overrides"` test (`surface.zig:4481`, 7 assertions)
— gtk modules have no G1b row, so this one is invisible to the gate; port it with B5 and
say so in the task's negative control.

## 6. Coverage honesty

**Exhaustive (read line-by-line):**
- The commit enumeration: `git log --oneline --no-merges 6ad1fe7d8..12967b68f -- <all 12 owned paths>` = 93 commits; every one appears in §2 exactly once (spans noted).
- The complete `macos/` range diff (5,469 lines) — every hunk read, including the full
  new-file bodies of `UntrustedURL.swift`, `UntrustedURLAlert.swift`,
  `CommandLineOpenFileFilter.swift`, `Ghostty.ClipboardConfirmationRequest.swift`, and the
  deleted-file bodies (CrossKit, UIKit view, iOSApp).
- The complete non-macos range diff for owned paths (4,470 lines: src/apprt, src/font,
  src/inspector, src/build, pkg/*, dist/flatpak/snap) — every hunk read.
- Rustty-side evidence: every ABSENT/PARTIAL verdict above carries a specific file:line or
  a named grep that returned ∅ over the named tree. The macOS-tree rows additionally rest
  on the git-log fact that no commit after the Phase-3 W8 wave touched `macos/Sources`.

**Sampled / inferred (stated as such):**
- For ~12 of the 44 macOS ABSENT rows marked "mechanical; tree untouched" (warning fixes,
  swiftlint, one-line guards: `c78226bfa`, `fd9837021`, `cb7eaa059`, and the like) I did
  not run a per-row symbol grep; the verdict rests on the tree-untouched-since-P3 evidence
  plus the read of the upstream hunk. Risk: a P3-era port coincidentally matching a P4
  hunk — possible in principle only for `97ae25749`/`f2022fe88` (P3-W8 touched
  showUserNotification's file), and I did grep those two (`Task { @MainActor [weak self]`
  ∅, `.map { _ in }` before debounce ∅).
- `a6fc3e0e2` (blur-region leak): I verified rustty has the pre-fix structure but did NOT
  trace rustty's `set_blur_region` ownership end-to-end; the row says so.
- I did not attempt to build/run anything (assessment only, per instructions), so
  "equivalent-by-construction" claims (`7a171895d` xcframework, wuffs-sys mechanism) rest
  on directory listings and build.rs prose, not on produced artifacts.
- pbxproj hunks (461 lines of project-file churn) were skimmed for file membership only,
  not read token-by-token; the file-tree facts they encode were verified against the
  rename/delete diff lines instead.

## 7. Suggested ordering within F (non-binding)

1. **G1b-red rows first**: `53c6fdbe7` (with E's call-site swap) and `ca8868a29` (after A's
   RenderState scaffolding exists) — they block the finale gate.
2. **C-coupled enum/struct adds in one C-synced batch** (action keys, ipc NewTab, osc8,
   OpenConfig) — every one carries a `checkGhosttyHEnum`/size-assert KAT that fails
   half-landed.
3. **macOS B3 (iOS/CrossKit removal) early** — it is pure deletion + rename, unblocks
   every later macOS diff from applying cleanly, and the README mandates the deletions.
4. macOS B1/B2/B4 + filter/tests (all have upstream tests to carry; `xcodebuild test` is
   the gate).
5. GTK B5–B8 + teardown fixes (`fd47b15cd` first — one-line, fixes a leak in code rustty
   already ships).
6. font rows (macOS-gated, KAT-able), inspector B9, build B10 + wasm flags, wuffs bump.
