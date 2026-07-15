# macOS identity rebrand — superseded by the full product rebrand

This file previously documented the **R2 partial identity rebrand**: under Branch A the Swift app
was kept byte-identical to upstream ghostty at the ABI/product level, and only the
*user-visible* name/bundle-identity was rebranded to "Rustty" / `io.airely.rustty`, while the
`.app`/executable/`GhosttyKit` module/secondary bundle ids were deliberately left as
`Ghostty`/`ghostty` to avoid breaking the `ghostty_*` C-ABI link.

**That constraint no longer exists.** The project is now fully on the `rustty_*` C-ABI /
`RusttyKit` module (no `ghostty_*` symbols remain), so the full product rebrand has landed:

- App bundle: **`Rustty.app`** (target `Rustty`, `PRODUCT_NAME = "$(TARGET_NAME)"`).
- Executable: **`rustty`** (`EXECUTABLE_NAME = rustty`).
- Framework/module: **`RusttyKit`** (`RusttyKit.xcframework`, `Sources/RusttyKit`).
- Bundle ids: all **`io.airely.rustty*`** — main app `io.airely.rustty[.debug]`, dock-tile plugin
  `io.airely.rustty-dock-tile`, iOS target `io.airely.rustty-ios`, unit tests
  `io.airely.RusttyTests`, UI tests `io.airely.RusttyUITests`.
- App icon: `ASSETCATALOG_COMPILER_APPICON_NAME = Rustty`.
- Xcode project: `Rustty.xcodeproj`, scheme `Rustty`, targets `Rustty` / `Rustty-iOS` /
  `RusttyTests` / `RusttyUITests` / `DockTilePlugin`.
- `.xib` files: `customModule="Rustty"`, custom classes `Rustty*`/`*Rustty*`.
- AppleScript: `Rustty.sdef` `cocoa class="RusttyScript*"` matching Swift `@objc(RusttyScript*)`.
- `Rustty-Info.plist`: `RusttyBuild`/`RusttyCommit` keys, `OSAScriptingDefinition = Rustty.sdef`,
  `Rustty Surface Identifier` UTI description.

There is no longer a "deliberately left as Ghostty" list on the macOS side — every product/bundle/
module identifier here is `Rustty`/`rustty`/`io.airely.rustty*`. The one remaining oracle-only
carve-out (parity-testing tooling that must literally match upstream Ghostty for comparison
purposes) is documented separately in `.port/tools/PARITY_NAMING.md`, not here.
