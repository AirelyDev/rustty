# macOS R2 identity rebrand — the app is "Rustty" everywhere the user sees it

This is the sanctioned **R2 identity exception**: under Branch A the Swift app is otherwise kept
byte-identical to upstream ghostty, but the *user-visible product name* and the *app bundle
identity* are rebranded to **Rustty** / `io.airely.rustty`. The C-ABI (`ghostty_*` symbols, the
`GhosttyKit` module, `#import`s, the on-disk product/executable name) is deliberately left as
`ghostty`/`Ghostty` because renaming it breaks the link.

The canonical rustty identity comes from `crates/rustty-identity` (the R2 source of truth):
`APP_NAME = "rustty"`, `BUNDLE_ID = "io.airely.rustty"`.

## Strategy (why the `.app`/executable stay named `Ghostty`)

The macOS app menu ("bold" application menu), Dock label, and Finder name are driven by
**`CFBundleName` / `CFBundleDisplayName`**, *not* by the product/executable filename. So we
rebrand those keys (+ hardcoded Swift display strings) and leave `PRODUCT_NAME` /
`EXECUTABLE_NAME` untouched. Benefits:

- The build product stays `Ghostty.app` and the executable stays `ghostty` — so the Branch-A
  `ghostty_*` link, the `ghostty +help` CLI invocation, `build-rustty.sh`, `build.nu`, and every
  agent that launches `macos/build/Debug/Ghostty.app` keep working with **zero path churn**.
- The user still sees "Rustty" everywhere: Finder/Dock render `CFBundleDisplayName`/`CFBundleName`
  (they hide the `.app` filename), the app menu renders `CFBundleName`, and the window title +
  About panel are hardcoded Swift strings we changed.

The only place "Ghostty" still surfaces is developer-facing (Activity Monitor process name =
executable `ghostty`, and the literal `Ghostty.app` filename on disk) — not a normal user surface.

## Changes made

### 1. `Ghostty-Info.plist`
- **Added** `CFBundleName` = `Rustty`. This is what the macOS **application menu** (and Dock)
  reads. It was previously absent, so Xcode was injecting `CFBundleName = $(PRODUCT_NAME)` =
  `Ghostty`; the explicit key now wins (Xcode only auto-injects `CFBundleName` when the key is
  missing).
- The NSServices menu items (`New $(INFOPLIST_KEY_CFBundleDisplayName) Tab/Window Here`) reference
  the display-name build setting, so they became **"New Rustty Tab/Window Here"** automatically
  once the build setting below changed — no literal edit needed.
- `plutil -lint Ghostty-Info.plist` → **OK**.

### 2. `Ghostty.xcodeproj/project.pbxproj` (main **Ghostty** macOS app target only)
- `INFOPLIST_KEY_CFBundleDisplayName` (→ `CFBundleDisplayName`, drives Dock/Finder + NSServices):
  - Debug: `"Ghostty[DEBUG]"` → `"Rustty[DEBUG]"` (kept the `[DEBUG]` marker — faithful to
    upstream's debug/release split; this is the config `build-rustty.sh` builds).
  - Release / ReleaseLocal: `Ghostty` → `Rustty`.
  - The 3 **iOS** target configs' `CFBundleDisplayName` were also swept `Ghostty` → `Rustty` for
    identity consistency (free, not built here, zero risk).
- `PRODUCT_BUNDLE_IDENTIFIER` (→ `CFBundleIdentifier`; authoritative — build setting overrides any
  plist value):
  - Debug: `com.mitchellh.ghostty.debug` → `io.airely.rustty.debug`.
  - Release / ReleaseLocal: `com.mitchellh.ghostty` → `io.airely.rustty`.
  - (The `.debug` suffix split is preserved, matching upstream's pattern.)
- All 43 user-visible permission-prompt strings `"A program running within Ghostty would like
  to …"` (camera/mic/contacts/location/… `INFOPLIST_KEY_NS*UsageDescription`) → `"… within
  Rustty …"`. These are shown by macOS when the app requests a permission.
- `plutil -lint …/project.pbxproj` → **OK**.

> **Why `CFBundleDisplayName` lives in the build setting, not literally in the plist:** the
> project sets it per-configuration via `INFOPLIST_KEY_CFBundleDisplayName` (so Debug can carry
> the `[DEBUG]` marker) and the plist's NSServices interpolate `$(INFOPLIST_KEY_CFBundleDisplayName)`.
> A literal `CFBundleDisplayName` in the plist would be *overridden* by the build setting, so it
> would be dead/misleading. The effective built value is `Rustty` (`Rustty[DEBUG]` for debug).

### 3. Swift user-visible display strings (`macos/Sources/…`)
App-name text the user reads, changed `Ghostty` → `Rustty` (string literals only — **no logic
change**):

| File | What |
| --- | --- |
| `Features/About/AboutView.swift` | About panel title `Text("Ghostty")` |
| `Features/About/CyclingIconView.swift` | accessibility label "Ghostty Application Icon" |
| `Features/Terminal/Window Styles/TitlebarTabsTahoeTerminalWindow.swift` | default window title `👻 Ghostty` |
| `Features/Terminal/Window Styles/TitlebarTabsVenturaTerminalWindow.swift` | default window title `👻 Ghostty` |
| `App/macOS/AppDelegate.swift` | alert "Allow Ghostty to execute …", "Quit Ghostty?" (×2) |
| `App/macOS/main.swift` | CLI init-failure banner "Ghostty failed to initialize! … executing Ghostty …" (the lowercase `ghostty +help` command token was left — it's the executable name) |
| `Features/Command Palette/TerminalCommandPalette.swift` | "Update Ghostty and Restart" |
| `Features/Terminal/TerminalView.swift` | debug-build banner + its accessibility value |
| `Features/Terminal/ErrorView.swift` | "…restart Ghostty." |
| `Features/Update/UpdatePopoverView.swift` | "Ghostty can automatically check for updates…" |
| `Features/App Intents/GhosttyIntentError.swift` | 2 Shortcuts error strings |
| `Features/App Intents/IntentPermission.swift` | "Allow Shortcuts to interact with Ghostty?" |
| `Features/AppleScript/AppDelegate+AppleScript.swift` | scriptErrorString "Ghostty app delegate is unavailable." (×2) |
| `Features/Settings/SettingsView.swift` | "…and restart Ghostty." (the config **path** in the same string was left — see leaves) |
| `App/iOS/iOSApp.swift` | splash `Text("Ghostty")` (iOS, for identity consistency) |

### 4. App-group / bundle-identity coupling
- `Features/Custom App Icon/DockTilePlugin.swift`: the dock-tile plugin opens the **main app's**
  `UserDefaults` by `suiteName` = the app's bundle id (to share the custom-app-icon preference
  cross-process). Updated `com.mitchellh.ghostty[.debug]` → `io.airely.rustty[.debug]` so it
  still matches the app's new bundle id. This is a **bundle-identity** string (explicitly in
  scope), not a logic change — the behavior (custom icon sharing) is preserved, not altered.
- **Entitlements** (`Ghostty.entitlements`, `GhosttyDebug.entitlements`,
  `GhosttyReleaseLocal.entitlements`): checked — they contain **no** `application-groups` /
  `keychain-access-groups` keys, so nothing embedding `com.mitchellh.ghostty` to change here.
  All three still `plutil -lint` OK.

### 5. `build-rustty.sh` / `MILESTONE.md`
- `build-rustty.sh`: **no change needed** — `PRODUCT_NAME` was left as `$(TARGET_NAME)`, so the
  built artifact is still `Ghostty.app` and the script has no launch/find path to update.
- `MILESTONE.md`: updated the two "identity rebrand … deferred / still Ghostty" follow-ups to
  reflect this landing, and clarified that the on-disk `Ghostty.app` now presents as "Rustty".

## Deliberately left as `ghostty` / `Ghostty` (NOT changed)

These are the Branch-A ABI, internal identifiers, or resource/path references — renaming them
would break the link or behavior, and they are not a user-visible *app name*:

- **`GhosttyKit` xcframework/module + all `ghostty_*` C symbols + `#import`/bridging header** —
  the Branch-A ABI. Renaming breaks the link. (Task hard constraint.)
- **`PRODUCT_NAME` / `EXECUTABLE_NAME = ghostty`** — the on-disk `Ghostty.app` bundle and the
  `ghostty` executable. Kept so build/launch paths and the `ghostty +help` CLI keep working.
- **`Ghostty.xcodeproj` project file + `Ghostty` scheme** names — renaming risks the build.
- **Secondary component bundle ids** (not user-visible, not app-groups):
  `com.mitchellh.ghostty-dock-tile` (dock-tile plugin), `com.mitchellh.GhosttyTests`,
  `com.mitchellh.GhosttyUITests`, `com.mitchellh.ghostty-ios`. Left to minimize build risk;
  a follow-up may rebrand them.
- **Internal opaque constants** (values only need to be self-consistent, not match the bundle id):
  `Notification.Name("com.mitchellh.ghostty.iconDidChange")`,
  `…terminalWindowBellDidChange`, the `NSUserInterfaceItemIdentifier` menu-item ids in
  `TerminalWindow.swift`, and the `"com.mitchellh.ghostty.shortcutsPermission"` UserDefaults key
  (changing it would reset users' saved permission choice).
- **`UTExportedTypeDeclarations`** `UTTypeIdentifier = com.mitchellh.ghosttySurfaceId` (a
  functional pasteboard/UTI identifier) + its description — internal type id, not an app name.
- **Info.plist internal keys**: `GhosttyBuild`, `GhosttyCommit` (read by Swift via
  `infoDictionary[...]`), `OSAScriptingDefinition = Ghostty.sdef` (file path),
  `NSDockTilePlugIn = DockTilePlugin.plugin` (path), `GHOSTTY_MAC_LAUNCH_SOURCE` env var.
- **`ASSETCATALOG_COMPILER_APPICON_NAME = Ghostty`** and the app icon asset set — the icon is
  upstream's; the asset is *named* `Ghostty`. Renaming without renaming the asset breaks the icon.
- **Swift symbol/type/namespace references**: the `Ghostty.*` namespace (e.g. `Ghostty.logger`,
  `Ghostty.Shell`), type names like `GhosttyIntentError` / `ColorizedGhosttyIcon`, AppleScript
  `class="GhosttyScript…"` sdef class names, and UserDefaults storage keys
  `CustomGhosttyIcon` / `CustomGhosttyIcon2` (persisted keys — changing them drops user data).
- **`$HOME/.config/ghostty/config.ghostty`** path fragment in `SettingsView.swift` — a config
  **path** reference (left per "leave path references"); only the app-name word in that sentence
  was rebranded. A config-path pass can revisit it.

## Verification
- `plutil -lint` passes on: `Ghostty-Info.plist`, `Ghostty.xcodeproj/project.pbxproj`, and all
  three `*.entitlements`.
- No `xcodebuild` was run here — the build/link verify is owned by a separate agent (this change
  set touches only name strings + bundle-identity settings; no Swift logic).
