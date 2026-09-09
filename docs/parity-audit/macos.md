# Parity audit — macos

Twenty-two shards (9–30) audited the entire macOS app tree of the port against Ghostty pin `20abdb50a`: `macos/Sources/App`, all of `macos/Sources/Features/**`, all of `macos/Sources/Ghostty` → `macos/Sources/RusttyKit/**`, `macos/Sources/Helpers/**`, `macos/Tests/**`, plus the adjacent `pkg/{afl++,android-ndk,apple-sdk}` build units that the macOS packaging depends on. **37,392 lines read**, covering 100% of the lines in scope — every unit was read in full; there were **no partial units**.

Counterpart status: **117 ported**, 2 replaced, 2 partial, 1 missing. Findings: **6 CONFIRMED** (4 × medium, 2 × low), **2 DISPUTED** (both corrected to low), **4 REFUTED**. The single most important gap is the missing `macos/Tests/Terminal/TerminalViewContainerTests.swift` — independently confirmed by four shards (S15-F2, S16-F1, S17-F1, S30-F1); the port over-deleted it on a stale premise (upstream `fd17869d1` rewrote the file rather than deleting it), so the `glassAvailability` assertions on `glassEffectView` are the only oracle test coverage lost anywhere in the macOS tree. Everything else is byte-identical to the oracle after the mandated `Ghostty→Rustty` / `ghostty→rustty` / `GHOSTTY→RUSTTY` per-token transform plus the locked `com.mitchellh.ghostty*` → `io.airely.rustty*` product-identity rebrand.

## Coverage

| Oracle unit (path:lines) | rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| macos/Sources/App/AppDelegate+Ghostty.swift:1-21 | macos/Sources/App/AppDelegate+Rustty.swift:1-21 | ported | Rename-normalized diff empty; `Rustty.Delegate` at RusttyKit/RusttyDelegate.swift. |
| macos/Sources/App/AppDelegate.swift:1-1397 | macos/Sources/App/AppDelegate.swift:1-1397 | ported | 1397 lines each, normalized diff empty; 66 IBOutlets, 53 menu-shortcut pairs, 39 SF symbols, all 12 IBActions verified. |
| macos/Sources/App/CommandLineOpenFileFilter.swift:1-59 | macos/Sources/App/CommandLineOpenFileFilter.swift + Tests/CommandLineOpenFileFilterTests.swift | ported | Identical modulo comment rename; 5 test funcs both sides. |
| macos/Sources/App/main.swift:1-33 | macos/Sources/App/main.swift; include/rustty-embed.h:1135; crates/rustty-apprt-c/src/lib.rs:241,368 | ported | `rustty_init` / `rustty_cli_try_action` declared and exported. |
| macos/Sources/Features/About/AboutController.swift | Features/About/AboutController.swift | ported | Byte-identical (48 lines); About.xib byte-identical. |
| macos/Sources/Features/About/AboutView.swift | Features/About/AboutView.swift | ported | Identical except project-identity constants (AirelyDev/rustty, rustty.org). |
| macos/Sources/Features/About/AboutViewModel.swift | Features/About/AboutViewModel.swift | ported | 9-icon order, 3s timer, indexWrapping all match. |
| macos/Sources/Features/About/CyclingIconView.swift | Features/About/CyclingIconView.swift | ported | Animation, contextMenu, accessibility identical. |
| macos/Sources/Features/App Intents/CloseTerminalIntent.swift | same path | ported | Identical after rename. |
| macos/Sources/Features/App Intents/CommandPaletteIntent.swift | same path | ported | Identical after rename. |
| macos/Sources/Features/App Intents/FocusTerminalIntent.swift | same path | ported | `focusSurface` at BaseTerminalController.swift:319. |
| macos/Sources/Features/App Intents/GetTerminalDetailsIntent.swift | same path | ported | 5 TerminalDetail cases, same raw values. |
| macos/Sources/Features/App Intents/GhosttyIntentError.swift | App Intents/RusttyIntentError.swift | ported | Renamed file; 3 cases, identical strings. |
| macos/Sources/Features/App Intents/InputIntent.swift | same path | ported | 327 lines identical after rename. |
| macos/Sources/Features/App Intents/IntentPermission.swift | same path | ported | Only UserDefaults key domain rebranded (io.airely.rustty.shortcutsPermission). |
| macos/Sources/Features/App Intents/KeybindIntent.swift | same path | ported | Identical after rename. |
| macos/Sources/Features/App Intents/NewTerminalIntent.swift | same path | ported | 178 lines identical after rename. |
| macos/Sources/Features/App Intents/QuickTerminalIntent.swift | same path | ported | Identical after rename. |
| macos/Sources/Features/App Intents/Entities/CommandEntity.swift | same path | ported | 124 lines, byte-identical after rename. |
| macos/Sources/Features/App Intents/Entities/TerminalEntity.swift | same path | ported | 196 lines, byte-identical after rename. |
| macos/Sources/Features/AppleScript/AppDelegate+AppleScript.swift | same path | ported | 351 lines, identical errAE* codes/strings. |
| macos/Sources/Features/AppleScript/Ghostty.Input.Mods+AppleScript.swift | AppleScript/Rustty.Input.Mods+AppleScript.swift | ported | 18 lines, identical. |
| macos/Sources/Features/AppleScript/ScriptInputTextCommand.swift | same path | ported | 41 lines; `@objc(RusttyScriptInputTextCommand)` matches Rustty.sdef:222. |
| macos/Sources/Features/AppleScript/ScriptKeyEventCommand.swift | same path + Tests/Features/AppleScript/ScriptKeyEventCommandTests.swift | ported | 119 lines; companion test 187 lines identical. |
| macos/Sources/Features/AppleScript/ScriptMouseButtonCommand.swift | same path | ported | 95 lines identical. |
| macos/Sources/Features/AppleScript/ScriptMousePosCommand.swift | same path | ported | 65 lines identical. |
| macos/Sources/Features/AppleScript/ScriptMouseScrollCommand.swift | same path | ported | 71 lines identical. |
| macos/Sources/Features/AppleScript/ScriptRecord.swift | same path | ported | 29 lines byte-identical. |
| macos/Sources/Features/AppleScript/ScriptSurfaceConfiguration.swift | same path | ported | 140 lines identical. |
| macos/Sources/Features/AppleScript/ScriptTab.swift | same path | ported | 186 lines identical. |
| macos/Sources/Features/AppleScript/ScriptTerminal.swift | same path | ported | Identical except one doc comment (S11-F1, refuted). |
| macos/Sources/Features/AppleScript/ScriptWindow.swift | same path | ported | 260 lines identical. |
| macos/Sources/Features/ClipboardConfirmation/ClipboardConfirmationController.swift | same path | ported | 63 lines; xib identical modulo rename. |
| macos/Sources/Features/ClipboardConfirmation/ClipboardConfirmationView.swift | same path | ported | 126 lines identical. |
| macos/Sources/Features/Command Palette/CommandPalette.swift | same path + Tests/CommandPaletteTests.swift | ported | Identical except an upstream issue URL kept verbatim (line 240). |
| macos/Sources/Features/Command Palette/TerminalCommandPalette.swift | same path | ported | 200 lines identical. |
| macos/Sources/Features/Custom App Icon/AppIcon.swift | same path | ported | 109 lines; DOCK_TILE_PLUGIN condition present in Rustty.xcodeproj. |
| macos/Sources/Features/Custom App Icon/ColorizedGhosttyIcon.swift | Custom App Icon/ColorizedRusttyIcon.swift + Tests/ColorizedRusttyIconTests.swift | ported | 115 lines; 10 tests identical. |
| macos/Sources/Features/Custom App Icon/ColorizedGhosttyIconImage.swift | ColorizedRusttyIconImage.swift | ported | 21 lines identical. |
| macos/Sources/Features/Custom App Icon/ColorizedGhosttyIconView.swift | ColorizedRusttyIconView.swift | ported | 13 lines identical. |
| macos/Sources/Features/Custom App Icon/DockTilePlugin.swift | same path | ported | Only UserDefaults suite names rebranded (io.airely.rustty[.debug]). |
| macos/Sources/Features/Custom App Icon/Extensions/Notification+AppIcon.swift | same path | ported | 8 lines; notification vendor prefix rebranded (S13-F1, refuted). |
| macos/Sources/Features/Custom App Icon/Extensions/UserDefaults+AppIcon.swift | same path | ported | 29 lines identical; keys CustomRusttyIcon/2. |
| macos/Sources/Features/Global Keybinds/GlobalEventTap.swift | same path | ported | 166 lines identical; `rustty_app_key` at rustty-embed.h:1164. |
| macos/Sources/Features/QuickTerminal/QuickTerminalController.swift | same path | ported | 804 lines; only an upstream issue URL differs (line 504). |
| macos/Sources/Features/QuickTerminal/QuickTerminalPosition.swift | same path | ported | 186 lines; `.center` initial-y quirk preserved verbatim. |
| macos/Sources/Features/QuickTerminal/QuickTerminalRestorableState.swift | same path + Tests/Terminal/TerminalRestorableTests.swift | ported | 58 lines; 215-line test identical. |
| macos/Sources/Features/QuickTerminal/QuickTerminalScreen.swift | same path | ported | 37 lines identical. |
| macos/Sources/Features/QuickTerminal/QuickTerminalScreenStateCache.swift | same path + Tests/QuickTerminalScreenStateCacheTests.swift | ported | 120 lines; TTL/prune logic identical. |
| macos/Sources/Features/QuickTerminal/QuickTerminalSize.swift | same path | ported | 84 lines; C types at rustty-embed.h:570-588. |
| macos/Sources/Features/QuickTerminal/QuickTerminalSpaceBehavior.swift | same path | ported | 36 lines identical. |
| macos/Sources/Features/QuickTerminal/QuickTerminalWindow.swift | same path | ported | 47 lines; AX identifier rebranded (S13-F1, refuted). |
| macos/Sources/Features/Secure Input/SecureInput.swift | same path | ported | 135 lines byte-identical. |
| macos/Sources/Features/Secure Input/SecureInputOverlay.swift | same path | ported | Identical except popover product name. |
| macos/Sources/Features/Services/ServiceProvider.swift | same path | ported | NSServices plist block identical. |
| macos/Sources/Features/Settings/ConfigurationErrorsController.swift | same path | ported | Byte-identical; xib differs only in customModule. |
| macos/Sources/Features/Settings/ConfigurationErrorsView.swift | same path | ported | Byte-identical. |
| macos/Sources/Features/Settings/SettingsView.swift | same path | ported | Hint string renamed; matches rustty XDG subdir `rustty`. |
| macos/Sources/Features/Splits/SplitTree.swift | same path + Tests/Splits/SplitTreeTests.swift | ported | 1413 lines byte-identical; 50 @Test companion identical. |
| macos/Sources/Features/Splits/SplitView.Divider.swift | same path | ported | Byte-identical. |
| macos/Sources/Features/Splits/SplitView.swift | same path | ported | Byte-identical. |
| macos/Sources/Features/Splits/TerminalSplitTreeView.swift | same path + Tests/Splits/TerminalSplitDropZoneTests.swift | ported | Identical after rename; 20 @Test companion identical. |
| macos/Sources/Features/Terminal/BaseTerminalController.swift:1-1741 | same path | ported | 1741 lines, no non-rename hunks except S15-F1 comment. |
| macos/Sources/Features/Terminal/ErrorView.swift:1-24 | same path | ported | Only the product name in a user-facing string. |
| macos/Sources/Features/Terminal/TerminalController.swift | same path | ported | 1766 lines; two upstream issue URLs kept verbatim. |
| macos/Sources/Features/Terminal/TerminalRestorable.swift | same path | ported | 236 lines byte-identical after transform. |
| macos/Sources/Features/Terminal/TerminalRestorableState+InteralState.swift | same path | ported | 31 lines; v5+v7 fields identical. |
| macos/Sources/Features/Terminal/TerminalTabColor.swift | same path | ported | 185 lines; 10 cases, same Codable raw values. |
| macos/Sources/Features/Terminal/TerminalView.swift | same path | ported | 180 lines identical after transform. |
| macos/Sources/Features/Terminal/TerminalViewContainer.swift:1-256 | same path | ported | 256 lines; one doc comment differs (S17-F2). |
| macos/Sources/Features/Terminal/Window Styles/HiddenTitlebarTerminalWindow.swift:1-118 | same path | ported | Identical; upstream URLs kept. |
| macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift:1-846 | same path | ported | Identical except 4 rebranded NSUserInterfaceItemIdentifiers (705-709). |
| macos/Sources/Features/Terminal/Window Styles/TitlebarTabsTahoeTerminalWindow.swift:1-329 | same path | ported | Identical after rename. |
| macos/Sources/Features/Terminal/Window Styles/TitlebarTabsVenturaTerminalWindow.swift:1-671 | same path | ported | Byte-identical; VibrantLayer ObjC bridged. |
| macos/Sources/Features/Terminal/Window Styles/TransparentTitlebarTerminalWindow.swift:1-227 | same path | ported | Byte-identical. |
| macos/Sources/Features/Update/UpdateBadge.swift | same path | ported | Byte-identical (83 lines). |
| macos/Sources/Features/Update/UpdateController.swift | same path | ported | Only doc-comment renames. |
| macos/Sources/Features/Update/UpdateDelegate.swift | same path | ported | Appcast hosts are the per-token domain transform. |
| macos/Sources/Features/Update/UpdateDriver.swift | same path | ported | Only a comment rename at line 69. |
| macos/Sources/Features/Update/UpdatePill.swift | same path | ported | Byte-identical. |
| macos/Sources/Features/Update/UpdatePopoverView.swift | same path | ported | Only the product name in one string. |
| macos/Sources/Features/Update/UpdateSimulator.swift | same path | ported | Byte-identical; all 9 cases and delays. |
| macos/Sources/Features/Update/UpdateViewModel.swift | same path + Tests/Update/* | ported | Identity substitutions only (RusttyCommit, rustty.org, AirelyDev/rustty). |
| macos/Sources/Ghostty/FullscreenMode+Extension.swift:1-23 | RusttyKit/FullscreenMode+Extension.swift | ported | Normalized diff empty; 4 fullscreen enum arms. |
| macos/Sources/Ghostty/Ghostty.Action.swift:1-176 | RusttyKit/Rustty.Action.swift | ported | 176 lines; all consumed C types diff-identical. |
| macos/Sources/Ghostty/Ghostty.App.swift:1-2453 | RusttyKit/Rustty.App.swift | ported | 2453 lines; one line differs (upstream issue URL at 818). 60 action cases verified. |
| macos/Sources/Ghostty/Ghostty.ChildExitedMessage.swift:1-41 | RusttyKit/Rustty.ChildExitedMessage.swift | ported | 41 lines; normalized diff empty. |
| macos/Sources/Ghostty/Ghostty.ClipboardConfirmationRequest.swift | RusttyKit/Rustty.ClipboardConfirmationRequest.swift | ported | 157 lines identical after transform. |
| macos/Sources/Ghostty/Ghostty.Command.swift | RusttyKit/Rustty.Command.swift | ported | 40 lines identical. |
| macos/Sources/Ghostty/Ghostty.Config.swift | RusttyKit/Rustty.Config.swift | ported | 923 lines; ~60 accessors, all enums/OptionSets identical. |
| macos/Sources/Ghostty/Ghostty.ConfigTypes.swift | RusttyKit/Rustty.ConfigTypes.swift | ported | 49 lines identical. |
| macos/Sources/Ghostty/Ghostty.Error.swift | RusttyKit/Rustty.Error.swift | ported | 12 lines identical. |
| macos/Sources/Ghostty/Ghostty.Event.swift | RusttyKit/Rustty.Event.swift | ported | 15 lines identical. |
| macos/Sources/Ghostty/Ghostty.Input.swift:1-1392 | RusttyKit/Rustty.Input.swift | ported | 1392 lines byte-identical after rename. |
| macos/Sources/Ghostty/Ghostty.Inspector.swift:1-102 | RusttyKit/Rustty.Inspector.swift | ported | Identical; all rustty_inspector_* present. |
| macos/Sources/Ghostty/Ghostty.MenuShortcutManager.swift:1-172 | RusttyKit/Rustty.MenuShortcutManager.swift | ported | Identical after rename. |
| macos/Sources/Ghostty/Ghostty.Shell.swift:1-29 | RusttyKit/Rustty.Shell.swift | ported | Identical escape/quote semantics. |
| macos/Sources/Ghostty/Ghostty.Surface.swift:1-194 | RusttyKit/Rustty.Surface.swift | ported | Identical after rename. |
| macos/Sources/Ghostty/GhosttyDelegate.swift:1-10 | RusttyKit/RusttyDelegate.swift | ported | Identical. |
| macos/Sources/Ghostty/GhosttyPackage.swift:1-408 | RusttyKit/RusttyPackage.swift | ported | Identical except 40 rebranded reverse-DNS constants. |
| macos/Sources/Ghostty/GhosttyPackageMeta.swift:1-16 | RusttyKit/RusttyPackageMeta.swift | ported | Identical after rename. |
| macos/Sources/Ghostty/NSEvent+Extension.swift:1-76 | RusttyKit/NSEvent+Extension.swift | ported | Identical after rename. |
| macos/Sources/Ghostty/Surface View/ChildExitedMessageBar.swift:1-49 | RusttyKit/Surface View/ChildExitedMessageBar.swift | ported | 49 lines; only 2 type-name tokens differ. |
| macos/Sources/Ghostty/Surface View/InspectorView.swift:1-441 | RusttyKit/Surface View/InspectorView.swift | ported | 441 lines; normalized diff empty. |
| macos/Sources/Ghostty/Surface View/OSSurfaceView.swift:1-191 | RusttyKit/Surface View/OSSurfaceView.swift + Tests/Rustty/Surface View/SurfaceView+SearchStateTests.swift | ported | 191 lines; 10 #expect companion identical. |
| macos/Sources/Ghostty/Surface View/SurfaceDragSource.swift:1-268 | RusttyKit/Surface View/SurfaceDragSource.swift | ported | 268 lines; normalized diff empty. |
| macos/Sources/Ghostty/Surface View/SurfaceGrabHandle.swift:1-98 | RusttyKit/Surface View/SurfaceGrabHandle.swift | ported | 98 lines identical. |
| macos/Sources/Ghostty/Surface View/SurfaceProgressBar.swift:1-112 | RusttyKit/Surface View/SurfaceProgressBar.swift | ported | 112 lines identical. |
| macos/Sources/Ghostty/Surface View/SurfaceScrollView.swift:1-395 | RusttyKit/Surface View/SurfaceScrollView.swift | ported | 395 lines identical; macOS 26 NSScrollPocket gating preserved. |
| macos/Sources/Ghostty/Surface View/SurfaceView+Image.swift:1-14 | RusttyKit/Surface View/SurfaceView+Image.swift | ported | 14 lines identical. |
| macos/Sources/Ghostty/Surface View/SurfaceView+Transferable.swift:1-47 | RusttyKit/Surface View/SurfaceView+Transferable.swift | ported | Only the exported UTType is rebranded; Rustty-Info.plist:113-123 agrees. |
| macos/Sources/Ghostty/Surface View/SurfaceView.swift:1-1185 | RusttyKit/Surface View/SurfaceView.swift | ported | 1185 lines; normalized diff exit 0. |
| macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift:1-2507 | RusttyKit/Surface View/SurfaceView_AppKit.swift | ported | 2507 lines; exactly one line differs (upstream issue URL at 820). |
| macos/Sources/Helpers/AppInfo.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Backport.swift | same path + Tests/Helpers/BackportPointerStyleTests.swift | ported | Byte-identical; rustty adds 4 extra tests (additive). |
| macos/Sources/Helpers/CodableBridge.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Cursor.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/ExpiringUndoManager.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Fullscreen.swift | same path | ported | Only the import rename + 1 comment word; upstream URLs kept. |
| macos/Sources/Helpers/KeyboardLayout.swift | same path + Tests/Helpers/KeyboardLayoutTests.swift | ported | Byte-identical; test identical after rename. |
| macos/Sources/Helpers/LastWindowPosition.swift | same path | ported | Identical after `UserDefaults.rustty` rename. |
| macos/Sources/Helpers/MetalView.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/NonDraggableHostingView.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/PermissionRequest.swift | same path | ported | Identical after rename. |
| macos/Sources/Helpers/TabGroupCloseCoordinator.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/TabTitleEditor.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/URLHoverBanner.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/UntrustedURL.swift | same path + Tests/Rustty/UntrustedURLTests.swift | ported | Byte-identical; test identical after rename. |
| macos/Sources/Helpers/UntrustedURLAlert.swift | same path | ported | Only the mandated product-name string. |
| macos/Sources/Helpers/Weak.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/Array+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/Double+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/Duration+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/EventModifiers+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/FileHandle+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/KeyboardShortcut+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/NSAlert+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/NSAppearance+Extension.swift | same path | ported | Only the `rusttyConfig:` label rename (4/4 use sites). |
| macos/Sources/Helpers/Extensions/NSApplication+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/NSColor+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/NSImage+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/NSMenu+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/NSMenuItem+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/NSPasteboard+Extension.swift | same path; include/rustty-embed.h:80-83 | ported | Naming only; private pasteboard name rebranded. |
| macos/Sources/Helpers/Extensions/NSScreen+Extension.swift | same path | ported | Only `UserDefaults.rustty` rename. |
| macos/Sources/Helpers/Extensions/NSView+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/NSWindow+Extension.swift | same path; Helpers/ObjCExceptionCatcher.{h,m} | ported | ObjC bridges renamed and identical. |
| macos/Sources/Helpers/Extensions/NSWorkspace+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/OSColor+Extension.swift | same path; include/rustty-embed.h:549 | ported | Only the `init(rustty:)` label rename. |
| macos/Sources/Helpers/Extensions/OSPasteboard+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/ObjectIdentifier+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/Optional+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/String+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/Transferable+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/URL+Extension.swift | same path + Tests/URLTests.swift | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/UUID+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/UndoManager+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Extensions/UserDefaults+Extension.swift | same path; RusttyUITests/RusttyCustomConfigCase.swift | ported | Renames only (`RUSTTY_USER_DEFAULTS_SUITE`). |
| macos/Sources/Helpers/Extensions/View+Extension.swift | same path | ported | Byte-identical. |
| macos/Sources/Helpers/Private/CGS.swift | same path | ported | Byte-identical (81 lines). |
| macos/Sources/Helpers/Private/Dock.swift | same path | ported | Byte-identical (38 lines). |
| macos/Tests/BenchmarkTests.swift | same path | ported | `rustty_benchmark_cli` at rustty-embed.h:1276 / rustty-bench/src/capi.rs:27. |
| macos/Tests/ColorizedGhosttyIconTests.swift | Tests/ColorizedRusttyIconTests.swift | ported | 10 @Test identical. |
| macos/Tests/CommandLineOpenFileFilterTests.swift | same path | ported | 5 tests / 13 #expect. |
| macos/Tests/CommandPaletteTests.swift | same path | ported | Identical after rename. |
| macos/Tests/NSPasteboardTests.swift | same path | ported | 1-255 identical; rustty adds an extra additive struct. |
| macos/Tests/NSScreenTests.swift | same path | ported | 4 tests / 8 #expect. |
| macos/Tests/QuickTerminalScreenStateCacheTests.swift | same path | ported | 4 tests identical. |
| macos/Tests/URLTests.swift | same path | ported | 4 tests identical. |
| macos/Tests/Features/AppleScript/ScriptKeyEventCommandTests.swift | same path | ported | 17 test funcs, parameterized rows identical. |
| macos/Tests/Ghostty/ConfigTests.swift | Tests/Rustty/ConfigTests.swift | ported | 38 tests identical. |
| macos/Tests/Ghostty/KeyEventSynthesizeTests.swift | Tests/Rustty/KeyEventSynthesizeTests.swift | ported | 6 + shortcut-translation tests identical. |
| macos/Tests/Ghostty/MenuShortcutManagerTests.swift | Tests/Rustty/MenuShortcutManagerTests.swift | ported | 3 tests / 12 assertions; `.bug()` URLs kept upstream. |
| macos/Tests/Ghostty/NormalizedMenuShortcutKeyTests.swift | Tests/Rustty/NormalizedMenuShortcutKeyTests.swift | ported | 12 tests identical. |
| macos/Tests/Ghostty/ShellTests.swift | Tests/Rustty/ShellTests.swift | ported | 22 + 9 parameterized cases identical. |
| macos/Tests/Ghostty/SurfaceViewAppKitTests.swift | Tests/Rustty/SurfaceViewAppKitTests.swift | ported | 8 parameterized + 2 tests identical. |
| macos/Tests/Ghostty/UntrustedURLTests.swift | Tests/Rustty/UntrustedURLTests.swift | ported | 13 tests identical. |
| macos/Tests/Ghostty/Surface View/SurfaceView+SearchStateTests.swift:1-110 | Tests/Rustty/Surface View/SurfaceView+SearchStateTests.swift | ported | 9 @Test identical. |
| macos/Tests/Helpers/KeyboardLayoutTests.swift:1-27 | same path | ported | Import rename only. |
| macos/Tests/Helpers/TemporaryConfig.swift:1-45 | same path | ported | Naming-only diffs. |
| macos/Tests/Helpers/TransferablePasteboardTests.swift:1-124 | same path | ported | 5 @Test identical. |
| macos/Tests/Splits/SplitTreeTests.swift:1-671 | same path | ported | 52 @Test identical. |
| macos/Tests/Splits/TerminalSplitDropZoneTests.swift:1-128 | same path | ported | 21 @Test identical. |
| macos/Tests/Terminal/TerminalRestorableTests.swift:1-215 | same path | ported | 3 tests + 4 base64 fixtures identical. |
| **macos/Tests/Terminal/TerminalViewContainerTests.swift:1-63** | *(none)* | **missing** | No counterpart anywhere in the workspace — see S15-F2 / S16-F1 / S17-F1 / S30-F1. |
| macos/Tests/Update/ReleaseNotesTests.swift:1-130 | same path | ported | 9 tests; repo-owner URLs are the fork rebrand (S30-F4, refuted). |
| macos/Tests/Update/UpdateStateTests.swift:1-125 | same path | ported | Identical after rename. |
| macos/Tests/Update/UpdateViewModelTests.swift:1-93 | same path | ported | Identical after rename. |
| pkg/afl++/build.zig:1-80 | crates/rustty-vt/fuzz/** ; .port/tools/gates/run_fuzz_smoke.sh | replaced | AFL++ instrumentation replaced by cargo-fuzz/libfuzzer with the same harnesses and seed corpora. |
| pkg/android-ndk/build.zig:1-205 | .port/tools/gates/check_cross_targets.sh:51-98 ; .github/workflows/ci.yml | partial | NDK discovery/sysroot wiring absent; Android leg silently skipped — see S30-F2. |
| pkg/apple-sdk/build.zig:1-208 | crates-sys/*/build.rs (xcrun -isysroot) | replaced | cc-rs + host Apple clang against the real SDK; Zig-libc++-only shims are N/A. |
| pkg/apple-sdk/native_link.zig:1-85 | xtask/src/main.rs:377,442-470,509-531 | partial | rustc/ld64 covers macOS; iOS + simulator slices absent (macOS-first fork) — see S30-F3. |

**Partial units:** there were **none** — every unit listed in the shards' `units_fully_read` was read end-to-end, and no shard reported a `units_partial` entry.

## Findings

### High

None.

### Medium

#### S15-F2 — OUT-OF-SHARD NOTE: TerminalViewContainerTests.swift has no counterpart in rustty/macos/Tests

- **Status:** ✅ CONFIRMED (both lenses) · **Kind:** missing-test · **Severity:** medium
- **Oracle:** `macos/Tests/Terminal/TerminalViewContainerTests.swift:1-EOF`
- **rustty:** `macos/Tests/Terminal/TerminalViewContainerTests.swift` — absent

**Description.** While diffing the `macos/Tests` trees to locate tests for shard 15's units, the file-list diff shows the oracle's Swift Testing suite (`MockTerminalViewContainer` / `MockConfig` testing `TerminalViewContainer` background blur/color/opacity) has no file in `rustty/macos/Tests`, and `grep -rl TerminalViewContainer rustty/macos` finds only production sources. Reported for the aggregator to route/dedupe against the shard owning `macos/Sources/Features/Terminal/TerminalViewContainer.swift`. (rustty additionally has `macos/Tests/Helpers/BackportPointerStyleTests.swift` with no oracle counterpart — an extra test, not a gap.)

**Evidence.** `diff <(cd ghostty/macos/Tests && find . -type f | sed s/Ghostty/Rustty/g | sort) <(cd rustty/macos/Tests && find . -type f | sort)` → `23d23 < ./Terminal/TerminalViewContainerTests.swift`; oracle header: `@testable import Ghostty` / `class MockTerminalViewContainer: TerminalViewContainer`. The semantic lens notes this is in fact *in*-shard, since `TerminalViewContainer.swift` lives in `macos/Sources/Features/Terminal`.

**Suggested fix.** Port the file to `rustty/macos/Tests/Terminal/` with the same assertions (rename-only), and add it to `Rustty.xctestplan` if tests are enumerated there.

#### S16-F1 — TerminalViewContainerTests.swift (glassAvailability) has no rustty counterpart

- **Status:** ✅ CONFIRMED (both lenses) · **Kind:** missing-test · **Severity:** medium
- **Oracle:** `macos/Tests/Terminal/TerminalViewContainerTests.swift:1-63`
- **rustty:** `macos/Tests/Terminal/TerminalViewContainerTests.swift` — absent

**Description.** The oracle file defines `MockTerminalViewContainer`, `MockConfig`, and `@Test func glassAvailability()` which calls `ghosttyConfigDidChange(config, preferredBackgroundColor: nil)` with `backgroundBlur` `.macosGlassRegular` and asserts `view.glassEffectView != nil` on macOS 26+ and `== nil` otherwise. `rustty/macos/Tests/Terminal` contains only `TerminalRestorableTests.swift`; nothing under `rustty/macos` references `TerminalViewContainerTests` or exercises `glassEffectView` from a test. The production file `rustty/macos/Sources/Features/Terminal/TerminalViewContainer.swift` exposes `glassEffectView` / `windowThemeFrameView` / `windowCornerRadius` (lines 10-17), so the test is directly portable.

**Evidence.** `ls ghostty/macos/Tests/Terminal` → `TerminalRestorableTests.swift`, `TerminalViewContainerTests.swift`; `ls rustty/macos/Tests/Terminal` → `TerminalRestorableTests.swift` only. Oracle assertions at lines 49-62. The presence lens identified the root cause: rustty commit `4e9d5e10` ("TerminalViewContainerTests deleted (upstream deletion)") removed all 103 lines, but the upstream change it tracked (`.port/phase-5/UPSTREAM_DIFF.patch:3744`, `@@ -60,44 +60,4 @@`) deleted only the 40-line `configChangeUpdatesGlass` test and kept `glassAvailability` — so the port over-deleted; this is a genuine missing test, not a legitimate upstream removal.

**Suggested fix.** Add `macos/Tests/Terminal/TerminalViewContainerTests.swift` as a token-transformed copy of the oracle file (`Ghostty`→`Rustty`, `ghosttyConfigDidChange`→`rusttyConfigDidChange`), keeping the single `@Test glassAvailability` with both `#expect` arms.

#### S17-F1 — TerminalViewContainerTests.swift (glassAvailability Swift Testing case) has no rustty counterpart

- **Status:** ✅ CONFIRMED (both lenses) · **Kind:** missing-test · **Severity:** medium
- **Oracle:** `macos/Tests/Terminal/TerminalViewContainerTests.swift:1-63`
- **rustty:** `macos/Tests/Terminal/` — no such file

**Description.** The oracle ships a Swift Testing suite for `TerminalViewContainer`: `MockTerminalViewContainer` (overrides `windowThemeFrameView` → `NSView()`, `windowCornerRadius`), `MockConfig` (overrides `backgroundBlur`/`backgroundColor`/`backgroundOpacity`, `super.init(config: nil)`), and `@Test glassAvailability` which builds the container with `EmptyView`, calls `ghosttyConfigDidChange(MockConfig(.macosGlassRegular, .clear, 1), preferredBackgroundColor: nil)`, sleeps 100 ms, and asserts `glassEffectView != nil` on macOS 26+ / `== nil` otherwise. Greps for `glassAvailability` / `MockTerminalViewContainer` / `TerminalViewContainer` across `rustty/macos/Tests`, `rustty/macos/RusttyUITests` and `Rustty.xcodeproj/project.pbxproj` return nothing. The production file is ported 1:1, so the test would port mechanically.

**Evidence.** Oracle lines 12-21 (`MockTerminalViewContainer`), 23-46 (`MockConfig`), 48-62 (`@Test glassAvailability`). `git ls-tree 20abdb50a macos/Tests/Terminal/` lists the blob at the pin, so the file is live upstream. In rustty the only hits are `.port/phase-5/UPSTREAM_DIFF.patch` and `.port/phase-5/assessments/D-ui-tiers.md:157`, where the assessment asserts `fd17869d1` "DELETES TerminalViewContainerTests.swift" — a factually wrong premise against the pin.

**Suggested fix.** Add `rustty/macos/Tests/Terminal/TerminalViewContainerTests.swift` as a token-renamed copy (`@testable import Rustty`, `rusttyConfigDidChange`) and ensure it is included in the Rustty test target / `Rustty.xctestplan`.

#### S30-F1 — TerminalViewContainerTests.swift (glassAvailability) has no counterpart

- **Status:** ✅ CONFIRMED (both lenses) · **Kind:** missing-test · **Severity:** medium
- **Oracle:** `macos/Tests/Terminal/TerminalViewContainerTests.swift:1-63`
- **rustty:** `macos/Tests/Terminal/` — n/a

**Description.** Same gap seen from the tests-tree shard. rustty ports the production class (`macos/Sources/Features/Terminal/TerminalViewContainer.swift`: `glassEffectView:10`, `windowThemeFrameView:13`, `windowCornerRadius:17`, `rusttyConfigDidChange:77`) but has no test file; `macos/Tests/Terminal` contains only `TerminalRestorableTests.swift` and nothing in the workspace references `glassAvailability`, `MockTerminalViewContainer` or `MockConfig`.

**Evidence.** Oracle `macos/Tests/Terminal/TerminalViewContainerTests.swift:12-62` vs `ls /Users/gael/projects/rustty/macos/Tests/Terminal` → `TerminalRestorableTests.swift` only; `grep -rn 'glassAvailability|MockTerminalViewContainer|MockConfig' /Users/gael/projects/rustty/macos` → no hits.

**Suggested fix.** Add the token-renamed oracle file (`Rustty.Config`, `rusttyConfigDidChange`); the Tests group is a `PBXFileSystemSynchronizedRootGroup` (`project.pbxproj:123`) so no pbxproj edit is needed.

### Low

#### S15-F1 — Comment cites upstream GitHub issue as "Rustty #560" instead of "Ghostty #560"

- **Status:** ✅ CONFIRMED (both lenses) · **Kind:** other · **Severity:** low
- **Oracle:** `macos/Sources/Features/Terminal/BaseTerminalController.swift:483`
- **rustty:** `macos/Sources/Features/Terminal/BaseTerminalController.swift:483`

**Description.** The bulk rename rewrote a reference to an upstream `ghostty-org/ghostty` issue number inside a comment. Issue #560 belongs to the Ghostty tracker; there is no Rustty #560, so the pointer is now dangling. Doc-only, no runtime effect.

**Evidence.** Oracle line 483: `// due to SwiftUI bugs (see Ghostty #560). To repeat from #560, the bug is that`. Rustty line 483: `// due to SwiftUI bugs (see Rustty #560). To repeat from #560, the bug is that`. The surrounding `confirmClose` block is byte-identical.

**Suggested fix.** Restore "Ghostty #560" (or `ghostty-org/ghostty#560`); upstream issue references are proper nouns and should be exempt from the rename.

#### S17-F2 — Doc comment on glassEffectView differs from oracle (doc-only)

- **Status:** ✅ CONFIRMED (both lenses) · **Kind:** other · **Severity:** low
- **Oracle:** `macos/Sources/Features/Terminal/TerminalViewContainer.swift:9`
- **rustty:** `macos/Sources/Features/Terminal/TerminalViewContainer.swift:9`

**Description.** Oracle line 9 reads `/// Background color applied with glass effect`; rustty line 9 reads `/// Combined glass effect and inactive tint overlay view`. The declaration (`private(set) var glassEffectView: NSView?`) and every other line of the file are identical after the token rename, so this is cosmetic only.

**Evidence.** Diff of the renamed oracle vs rustty yields exactly one hunk at line 9; both files are 256 lines and the remaining 255 match byte-for-byte.

**Suggested fix.** Restore the oracle wording for R1 comment fidelity (or leave as-is; no behavioural effect).

#### S30-F2 — Android NDK auto-discovery and sysroot wiring not reproduced; Android lib-vt build never runs

- **Status:** ⚠️ DISPUTED (one lens refuted) · **Kind:** missing-function · **Severity:** medium as filed, **corrected to low** by both lenses
- **Oracle:** `pkg/android-ndk/build.zig:18-205`
- **rustty:** `.port/tools/gates/check_cross_targets.sh:51-98`

**Description.** The oracle locates the NDK (`ANDROID_NDK_HOME` with existence check; else `ANDROID_HOME`/`ANDROID_SDK_ROOT` + latest-semver `ndk/<ver>` dir; else `$HOME/Library/Android/Sdk` on macOS or `$HOME`/`%LOCALAPPDATA%`/`Android/Sdk` elsewhere), maps host → prebuilt dir and arch → NDK triple, then writes a `libc.txt` with `include_dir`/`sys_include_dir`/`crt_dir=<sysroot>/usr/lib/<triple>/<api-level>` and adds the `c++/v1` system include and lib path, so `zig build -Dtarget=aarch64-linux-android` for lib-vt works with only an installed NDK. Its use site (`GhosttyLibVt.zig:273-278`, highway/simdutf `build.zig`) is exercised by the oracle CI job `build-libghostty-vt-android` (`test.yml:891-936`, setup-ndk r29). rustty has no NDK discovery at all: G-XTARGET's `aarch64-linux-android` leg relies on `${CXX:-c++} --target=aarch64-linux-android` succeeding and otherwise reports `skipped`; `ci.yml:151-155` does not even `rustup target add aarch64-linux-android`, so the Android build of `rustty-vt-c` is never compiled anywhere.

**Evidence.** Oracle `pkg/android-ndk/build.zig:119-205` (`findNDKPath`/`findLatestNDK`/`hostTag`/`ndkTriple`) and `:53-116` (sysroot/libc.txt/cpp include/lib wiring); rustty `.port/tools/gates/check_cross_targets.sh:94-98`, `.github/workflows/ci.yml:151-155`; `grep -rn ANDROID_NDK_HOME` → no source hits.

**Refuting lens — semantic (confidence: medium), verbatim:** "pkg/android-ndk/build.zig is Zig build plumbing (NDK path discovery + libc.txt/sysroot wiring for `zig build`), explicitly in the 'replaced' class: cargo/cc-rs consume the standard CC_/AR_/CARGO_TARGET_*_LINKER env instead of an in-build NDK finder, so there is no ported code path whose behaviour differs. The use site is still represented: .port/tools/gates/check_cross_targets.sh:94-98 lists aarch64-linux-android as a real leg and reports 'skipped' when `${CXX} --target=aarch64-linux-android` cannot compile (check_cross_targets.sh:51-76). What remains is a CI-coverage gap (no android job in ci.yml:151-155), not an observable functional difference in the port; per the ground rules that is not a finding."

*(The presence lens did not refute: it confirmed the absence of any NDK wiring in the workspace — hits only in plan/audit docs — but corrected severity to low for the same build-plumbing reason, noting rustty's own build-system gap audit already records "no real Windows/Android/wasm build legs".)*

**Suggested fix.** Add NDK discovery mirroring `findNDKPath`/`findLatestNDK` in the cross gate or a small xtask helper that exports `CC_aarch64_linux_android`, `AR`, `CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER` from `<ndk>/toolchains/llvm/prebuilt/<host>/bin`, and add an Android leg to `ci.yml` (setup-ndk r29 + `rustup target add aarch64-linux-android`) so the build is actually exercised.

#### S30-F3 — iOS / iOS-simulator SDK and clang-target mappings have no counterpart; VT xcframework is macOS-only

- **Status:** ⚠️ DISPUTED (one lens refuted) · **Kind:** missing-branch · **Severity:** medium as filed, **corrected to low** by both lenses
- **Oracle:** `pkg/apple-sdk/native_link.zig:13-85`
- **rustty:** `xtask/src/main.rs:377-470`

**Description.** `native_link.available()` accepts `.macos` and `.ios` targets, `sdkName()` maps `ios(.simulator)` → `iphonesimulator` / `ios` → `iphoneos`, and `clangTarget()` emits `<arch>-apple-ios<min>[-simulator]`, so the oracle links lib-vt for iOS device and simulator slices (`GhosttyLibVt.zig:117,404-485`, platform pairs at `:128-138,207-210`). rustty replaces the native-link step with rustc's ld64 link and pins only `MACOSX_DEPLOYMENT_TARGET=13.0` (`xtask/src/main.rs:377,509-531`); `xtask xcframework-vt` packages a single macOS static slice (`main.rs:442-470`), so the shipped `rustty-vt.xcframework` lacks the `ios-arm64` and `ios-simulator` slices. Recorded as an unscheduled iOS tier at `docs/superpowers/plans/2026-07-29-phase3-upstream-resync.md:2997-3002` (macOS-first fork), so a known deferral rather than an accidental omission, but still an observable artifact difference.

**Evidence.** Oracle `native_link.zig:16-19` (.macos,.ios), `:45-54` (macosx/iphoneos/iphonesimulator), `:68-84` (`-apple-macosx` / `-apple-ios` / `-apple-ios-simulator`); rustty `xtask/src/main.rs:377`, `:395`, `:442-470`; `grep -rn 'iphonesimulator|apple-ios'` over crates/xtask/crates-sys → no hits.

**Refuting lens — semantic (confidence: high), verbatim:** "The finder's own load-bearing claim ('no DEFERRALS.md row mentions apple-sdk/native_link/iOS') is false: /Users/gael/projects/rustty/.port/phase-3/NON_PORTS.md:515-536 is a dedicated row 'G7 — d65cb5128 native-linker link for lib-vt on Apple + pkg/apple-sdk/native_link.zig — N/A, WITH A PRECONDITION', citing xtask/src/main.rs:395,442 (static, macOS-only) and recording the iOS tier as an unscheduled precondition. Additionally the oracle's own iOS slices are conditional (GhosttyLibVt.zig:186-210 gates each on detectAppleSDK), and native_link.zig is Zig build plumbing replaced by rustc's ld64 path. Documented scope of the locked macOS-first fork, not an accidental omission of ported behaviour."

*(The presence lens did not refute: it substantiated the artifact difference — `xtask` pins `aarch64-apple-darwin` and packages one slice, no iOS build path anywhere — while also noting `.port/phase-2/DEFERRALS.md:2907-2946,4697` already carries iOS ledger material, and corrected severity to low given the locked macOS-first fork.)*

**Suggested fix.** When the iOS tier is scheduled: build `rustty-vt-c` for `aarch64-apple-ios` and `aarch64-apple-ios-sim` with `IPHONEOS_DEPLOYMENT_TARGET`, and fold those slices into `xcodebuild -create-xcframework` in `xtask xcframework-vt`.

### Unverified (over the verification cap)

None — every finding raised in this area was verified by both lenses.

## Performance-only differences (not findings)

- **macos/Sources/Features/Custom App Icon/Extensions, QuickTerminal (shard 13):** no performance-only differences — all 11 Swift units (plus `QuickTerminal.xib`, `QuickTerminalScreenStateCacheTests.swift`, `TerminalRestorableTests.swift`) are line-for-line identical after the rename, apart from the vendor-prefix rebrand and two upstream `ghostty-org` URLs left verbatim in comments (`QuickTerminalController.swift:504`, `QuickTerminalWindow.swift:34,44`).
- **Env var (shard 13):** `GHOSTTY_QUICK_TERMINAL` → `RUSTTY_QUICK_TERMINAL` is the sanctioned per-token transform, consistent with the GTK apprt (`crates/rustty-apprt/src/gtk/class/surface.rs:2574` via `rustty_identity::ENV_PREFIX = "RUSTTY"`).
- **Terminal shard 16:** all five units byte-identical after the token transform; the only residual textual differences are two upstream GitHub issue URLs in comments (`TerminalController.swift:421`, `:1227`) that correctly retain the ghostty hostnames. Adjacent: `TerminalViewContainer.swift` differs only in one doc-comment line.
- **Window styles (shard 17):** no performance-only divergences; the four tab-context-menu `NSUserInterfaceItemIdentifier` strings use the rebranded `io.airely.rustty.*` prefix and are only compared against themselves via `removeItems(withIdentifiers:)`. `rustty_set_window_background_blur` is realized with a real CGS body at `crates/rustty-apprt-c/src/lib.rs:2647-2695` (a stale PORT NOTE above it still says `unimplemented!()`).
- **RusttyKit (shard 22):** all nine files identical under a normalized diff; only `RusttyPackage.swift` shows the `io.airely.rustty` reverse-DNS prefix lines. Cross-shard note: the oracle's `src/build/GhosttyXcodebuild.zig:159` sets `GHOSTTY_MAC_LAUNCH_SOURCE=zig_run`; rustty's xtask has no equivalent setter, but both use sites treat `.zig_run` and `.cli` identically and an unset var already yields `.cli`, so behaviour is unchanged.
- **Helpers (shard 26) / Extensions (shard 27) / Private+Tests (shard 28):** byte-identical after the token transform across all 17 + 29 + 18 files; the sole non-mechanical rename is the private selection pasteboard name (`com.mitchellh.ghostty.selection` → `io.airely.rustty.selection`), used only by rustty's own code. rustty's `NSPasteboardTests.swift` carries one extra additive struct and `Tests/Helpers` an extra `BackportPointerStyleTests.swift`; neither weakens an oracle assertion.
- **Tests shard 29:** every unit is a pure naming-transform copy (`Ghostty`→`Rustty`, `GhosttyKit`→`RusttyKit`, `ghostty_config_get`→`rustty_config_get`, `.ghostty` temp extension → `.rustty`).
- **pkg/afl++ (shard 30):** AFL++ compile-time instrumentation (`afl-cc -O3` over LLVM bitcode, lld on Darwin) is replaced by libFuzzer (cargo-fuzz, libfuzzer-sys 0.4) with `[profile.release] debug = 1` — a different fuzzing engine with the same harness bodies and seed corpora; no harness-semantics difference.
- **pkg/apple-sdk (shard 30):** the oracle caches the generated `libc.txt` per (arch,os,abi) because `xcrun` is expensive; rustty's `build.rs` files each spawn `xcrun --show-sdk-path` once per crate build (dcimgui/glslang/harfbuzz/spirv-cross/libpng/sentry-sys) — build cost only, same resolved SDK.

## Refuted candidates

- **S9-F1 — Help menu URL became `https://rustty.org/docs` via the mechanical rename.** Both lenses: this is exactly the mandated per-token `ghostty→rustty` transform (which the ground rules exempt); `showHelp(_:)` is otherwise byte-identical and domain hosting is project infrastructure, not a parity defect.
- **S11-F1 — Doc comment differs: rustty corrects the oracle's stale `@objc` class name.** Both lenses: the oracle's line-10 comment (`GhosttyAppleScriptTerminal`) is stale relative to its own attribute at line 20 (`GhosttyScriptTerminal`); rustty's comment matches its real attribute and `Rustty.sdef:87`. Comment-only, Cocoa scripting resolves via the attribute.
- **S13-F1 — Quick terminal accessibility identifier / icon notification use the `io.airely` vendor prefix.** Both lenses: locked, documented product rebrand (`macos/REBRAND.md:10-27`, `PRODUCT_BUNDLE_IDENTIFIER = io.airely.rustty`); the notification's poster and observer are both rebranded so the icon-change path is unchanged, and the AX identifier is self-assigned. The finding itself concedes "no rustty-internal behaviour differs".
- **S30-F4 — GitHub compare/commit URL host path is `AirelyDev/rustty` rather than `rustty-org/rustty`.** Both lenses: test (`macos/Tests/Update/ReleaseNotesTests.swift:31,47,63,78,125`) and production (`UpdateViewModel.swift:347,349`) agree, assertion count and strictness identical — a deliberate repository-owner rebrand, not a weakened test.
