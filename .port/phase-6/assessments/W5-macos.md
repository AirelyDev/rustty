# P6-W5 — macOS app (`macos/`) — assess + port record

Range `88f57ee66..20abdb50a`, oracle pin `20abdb50a` (read-only tree at `/Users/gael/projects/ghostty`).
Branch `phase-6/w5-macos`, worktree `/Users/gael/projects/rustty-wt/w5-macos`.

## Coverage statement

Every `macos/**` hunk of `.port/phase-6/UPSTREAM_DIFF.patch` (patch lines 1230–2015: 17 source
files + `Ghostty.sdef` + pbxproj + `Package.resolved` + 3 test files) and the two
`.github/workflows/release-*.yml` `SPARKLE_VERSION` hunks are ported at their END STATE. Method:
the range slice was re-pathed (`macos/Ghostty.sdef → macos/Rustty.sdef`, `macos/Sources/Ghostty/ →
macos/Sources/RusttyKit/`, `macos/Tests/Ghostty/ → macos/Tests/Rustty/`, `Ghostty.xcodeproj →
Rustty.xcodeproj`) and renamed per-token (`Ghostty→Rustty`, `ghostty→rustty`, `GHOSTTY→RUSTTY`),
then `git apply`ed cleanly (every hunk matched — the pre-range Swift tree was already line-identical
to the oracle modulo the rename). Post-port proof: for all 18 touched/added Swift/sdef files, the
renamed oracle file diffs against the rustty file with **zero** lines except the pre-existing,
deliberate rustty divergences that are outside every hunk (bundle-id identifiers
`io.airely.rustty.*` in `TerminalWindow.swift:705-709`; oracle GitHub issue URLs kept as
`ghostty-org`/`mitchellh/ghostty` in `TerminalController.swift:421,1227`,
`SurfaceView_AppKit.swift:820`). Every comment in the hunks is carried (renamed where it names the
product or a renamed Swift symbol: `NSEvent.rusttyCharacters` / `NSEvent.rusttyKeyEvent`, both of
which exist at `macos/Sources/RusttyKit/NSEvent+Extension.swift:54,12`).

C-ABI: the only C entrypoint the new Swift needs is `ghostty_surface_key_translation_mods` → rustty
`rustty_surface_key_translation_mods` (`include/rustty-embed.h:1194`,
`crates/rustty-apprt-c/src/lib.rs:1749`), already present and already used by
`SurfaceView_AppKit.swift:1112`. `include/ghostty.h` is not in the range; no new C surface. The
embed ABI is untouched (parity harness below proves it).

Xcode project: `Sources/` and `Tests/` are `PBXFileSystemSynchronizedRootGroup`s
(`project.pbxproj:122-123`), so the four NEW files need no pbxproj edit; `Rustty.xctestplan`
enumerates targets, not files. swiftlint (`macos/AGENTS.md`) is clean on every touched file.

## Per-commit verdicts

| oracle sha | subject | verdict (pre-port) | rustty end state (file:line) | tests | negative control |
|---|---|---|---|---|---|
| `6229d4eb6` | macOS: fix AppleScript send key for non-control keys | **ABSENT** (no `synthesizing:` init, no `keyTranslationMods`, no `keyEventText`; `ScriptKeyEventCommandTests` still had the 6 `Issue.record(..., severity: .warning)` stubs) | `macos/Sources/RusttyKit/Rustty.Input.swift:243-300` (`KeyEvent.init(synthesizing:action:mods:translationMods:)`); `Rustty.Surface.swift:61-70` (`keyTranslationMods`); `Features/AppleScript/ScriptKeyEventCommand.swift:34-38,112-117`; `Surface View/SurfaceView_AppKit.swift:1487`; `Helpers/Extensions/String+Extension.swift:8-17` (`keyEventText`); `macos/Rustty.sdef:229` | `Tests/Features/AppleScript/ScriptKeyEventCommandTests.swift` (6 stubs → real `#expect`s); NEW `Tests/Rustty/KeyEventSynthesizeTests.swift` (5 tests + 37-arg parameterized + `KeyboardShortcutTranslationTests`) — 42+1 runs | **NC-A** run: `if action == .release` → `.press` in the synthesizing init ⇒ 11 tests FAIL (`KeyEventSynthesizeTests` ×5, `ScriptKeyEventCommandTests` ×6), `** TEST FAILED **`; restored ⇒ 159/0 |
| `777929a8f` | macOS: review windows when closing multiple tabs | **ABSENT** (`NSAlert+Extension.swift` did not exist; `closeWindow` took `.first(where:)`) | NEW `macos/Sources/Helpers/Extensions/NSAlert+Extension.swift:1-19`; `App/AppDelegate.swift:1349-1351`; `Features/Terminal/TerminalController.swift:1389-1442` (`confirmControllers`, `reviewWindows(_:window:)`) | none upstream (modal UI) | compile-level only: the `TerminalController` hunk is exercised by the app build (`** BUILD SUCCEEDED **`); no unit test upstream, none invented |
| `2de159611` + `1c3a4a831` + `07abbd1e7` | macos: normalize action working directory paths (+2 follow-up edits) | **ABSENT** (`URL+Extension.swift` did not exist; callers used `path(percentEncoded: false)`) | NEW `macos/Sources/Helpers/Extensions/URL+Extension.swift:1-9` (END STATE: `FilePath(path(percentEncoded: false)).string`); `Features/App Intents/NewTerminalIntent.swift:77`; `Features/Services/ServiceProvider.swift:62` | NEW `Tests/URLTests.swift` (4 tests) | **NC-B** run: `pathWithoutTrailingSlash` → bare `path(percentEncoded: false)` ⇒ 3 tests FAIL (`pathWithoutTrailingSlash`, `pathWithoutMultipleTrailingSlashes`, `pathWithoutTrailingSlashDecodesPath`; `PreservesRoot` passes by construction), `** TEST FAILED **`; restored ⇒ 159/0 |
| `572fd5837` | macOS: use the same default BellFeatures as config | **ABSENT** (`.init()` = empty set on both fallbacks) | `macos/Sources/RusttyKit/Rustty.Config.swift:131,134,814` (`BellFeatures.defaultValue = [.attention, .title]`) — equals the Rust default `crates/rustty-config/src/value_types.rs:1351-1361` (`attention: true, title: true`, rest false) | none upstream | none upstream; cross-checked value against the Rust `Default` impl (the parity contract is the C-ABI packed-flags default, unchanged) |
| `7a15898bc` | macOS: use same ResetZoomAccessoryView | **N-A** — REVERTED by `b6ac6e1d4` inside the range | nothing ported (`TerminalWindow.swift` keeps its own `resetZoomTabButton`/`ResetZoomAccessoryView`, identical to the pin) | — | — |
| `b6ac6e1d4` | Revert "macOS: use same ResetZoomAccessoryView" | **N-A** (net zero with `7a15898bc`) | — | — | — |
| `0a9f47cae` | macOS: update note about tab accessory view | **ABSENT** | `macos/Sources/Features/Terminal/Window Styles/TerminalWindow.swift:160` (comment: "... not clickable on macOS 15.") | — | comment-only |
| `851751a11` | macOS: clean up deprecated toolbar button | **ABSENT** (`resetZoomToolbarButton`, `.resetZoom` identifier, `selectTabAndZoom` all still present) | `Window Styles/TitlebarTabsVenturaTerminalWindow.swift` (−49 net; `toolbarAllowedItemIdentifiers` = `[.titleText, .flexibleSpace]`, `NSToolbarItem.Identifier.resetZoom` gone) | none upstream | deletion-with-consumer check: `grep -rn "resetZoomToolbarButton\|\.resetZoom\b\|selectTabAndZoom" macos/` → 0 hits after the port; app build green |
| `166d2fe34` | build: update Sparkle to 2.9.4 | **N-A** — superseded by `c18198325` (intermediate not ported) | — | — | — |
| `c18198325` | build: update Sparkle to 2.9.6 and pin SPM | **ABSENT** (pbxproj `upToNextMajorVersion 2.5.1`; `Package.resolved` 2.9.0 @ `21d8df80`; both release workflows `SPARKLE_VERSION: 2.9.0`) | `macos/Rustty.xcodeproj/project.pbxproj:1102-1103` (`exactVersion` / `2.9.6`); `project.xcworkspace/xcshareddata/swiftpm/Package.resolved` (2.9.6 @ `ac2def288cbff5cfc7df3ffef6abdf45b72bcb0a`, byte-identical to the oracle's); `.github/workflows/release-tag.yml:376`, `release-tip.yml:425` (rustty's restructured workflows have one Setup-Sparkle step each vs the oracle's 2 + 3 — all occurrences in both trees are 2.9.6) | — | xcodebuild resolution: `Resolved source packages: Sparkle: https://github.com/sparkle-project/Sparkle @ 2.9.6`; `git status` after build+test shows `Package.resolved` unchanged from the committed (oracle-identical) content — no drift from resolution |

## Revert / end-state handling

- `7a15898bc` → `b6ac6e1d4` cancel out; `0a9f47cae` is the only surviving `TerminalWindow.swift`
  change (one comment line). Verified: renamed-oracle `TerminalWindow.swift` vs rustty differs only
  in the four `io.airely.rustty` identifier lines (pre-existing rebrand). Nothing from
  `7a15898bc`'s intermediate state exists in rustty.
- `166d2fe34` (2.9.4) is skipped; only the `c18198325` 2.9.6 + `exactVersion` end state is ported.

## Build + test evidence (the `rustty-macos-app` CI job, run verbatim from the worktree)

1. `cargo run --release -p xtask -- xcframework` → `wrote .../macos/RusttyKit.xcframework`
2. `cargo run --release -p xtask -- resources` → `wrote .../zig-out/share`
3. `xcodebuild -project Rustty.xcodeproj -scheme Rustty -configuration Debug -derivedDataPath build CODE_SIGNING_ALLOWED=NO`
   → `** BUILD SUCCEEDED **` (`Resolved source packages: Sparkle: ... @ 2.9.6`)
4. `xcodebuild test -project Rustty.xcodeproj -scheme Rustty -configuration Debug -derivedDataPath build -destination 'platform=macOS' -only-testing:RusttyTests CODE_SIGNING_ALLOWED=NO`
   → `** TEST SUCCEEDED **`

   xcodebuild under Xcode 26 prints no "Executed N tests" line for Swift Testing; the counts from the
   run's xcresult (`xcrun xcresulttool get test-results summary`):

   ```
   "passedTests" : 403,   "failedTests" : 0,   "skippedTests" : 1,   "expectedFailures" : 0,
   "totalTestCount" : 278   ("23 tests ran with dynamic parameters" / "149 test runs")
   "result" : "Passed"
   ```

   Log-line cross-check: `Test case '…' passed` ×403 (one line is interleaved in the log), `failed` ×0,
   `skipped` ×1. Previous finale reported 356/0; this run is **403/0 = 356 + 47** new runs
   (`KeyEventSynthesizeTests` 42 = 5 + 37 parameterized, `KeyboardShortcutTranslationTests` 1,
   `URLTests` 4).

## Negative controls (run; both restored via `git checkout --`, tree clean)

| id | mutation | subset run (`-only-testing` KeyEventSynthesizeTests + ScriptKeyEventCommandTests + URLTests) |
|---|---|---|
| NC-A | `Rustty.Input.swift` synthesizing init: `if action == .release {` → `if action == .press {` (press/release text mapping flipped) | `** TEST FAILED **`, exit 65; failed: `KeyEventSynthesizeTests/{controlIsNeverConsumedAndDoesNotAffectText,optionFollowsTranslationMods,pressHasLayoutText,releaseHasNoText,shiftAppliesToTextAndIsConsumed}`, `ScriptKeyEventCommandTests/{controlKeepsBaseTextAndIsNotConsumed,optionExcludedFromTranslationIsNotConsumed,optionIncludedInTranslationIsConsumed,pressCarriesLayoutText,releaseCarriesNoText,shiftShiftsTextAndIsConsumed}` (the parameterized no-text test passes by construction under this flip) |
| NC-B | `URL+Extension.swift`: `FilePath(path(percentEncoded: false)).string` → `path(percentEncoded: false)` | `** TEST FAILED **`, exit 65; failed: `URLTests/{pathWithoutTrailingSlash,pathWithoutMultipleTrailingSlashes,pathWithoutTrailingSlashDecodesPath}` |
| restored | — | `** TEST SUCCEEDED **`, 159 passed / 0 failed |

## Embed-ABI guard

`GHOSTTY_ROOT=/Users/gael/projects/ghostty .port/tools/macos-parity/menu-shortcut-parity.sh`
(the worktree is not a sibling of `ghostty/`, so the default `../ghostty` needed overriding; the
harness only reads the oracle's prebuilt `ghostty-internal.a`, outputs go to a mktemp dir; oracle
`git status` clean afterwards). Since R13-G part F2 the override no longer exists and the plain
`.port/tools/macos-parity/menu-shortcut-parity.sh` finds the oracle from any worktree (see Open
items):

```
menu-shortcut parity OK — 51 actions resolve byte-identically (rustty == ghostty)
```

## Commits (branch `phase-6/w5-macos`)

| sha | subject |
|---|---|
| `5e2b6c01` | macos: fix AppleScript send key for non-control keys (oracle 6229d4eb6) |
| `4d1e8e4e` | macos: review windows when closing multiple tabs (oracle 777929a8f) |
| `65ee6cf3` | macos: normalize action working directory paths (oracle 2de159611 + 1c3a4a831 + 07abbd1e7) |
| `1758144e` | macos: use the same default BellFeatures as config (oracle 572fd5837) |
| `a257aa7a` | macos: TerminalWindow end state + drop the deprecated Ventura reset-zoom toolbar button (oracle 851751a11, 0a9f47cae) |
| `6dc9c677` | build: update Sparkle to 2.9.6 and pin SPM (oracle c18198325; 166d2fe34 2.9.4 superseded) |

## Open items

None blocking for W5. Observations outside the range (not touched, for the finale's awareness):

- `macos/build-rustty.sh:2-3` header still says "Branch A: ghostty_* ABI aliases" — stale wording
  since the `rustty_*` rebrand (`macos/REBRAND.md`); doc-only, not in this range.
- `.port/tools/macos-parity/menu-shortcut-parity.sh:26` derived `GHOSTTY_ROOT` from
  `$RUSTTY_ROOT/../ghostty`, which is wrong from any worktree under `rustty-wt/`. At the time the
  env override worked around it, so no change was made here. **Superseded (R13-G part F2):** the
  `GHOSTTY_ROOT` override is gone (R5: nothing caller-settable may point a parity check at another
  oracle). The harness now resolves the oracle with the shared `_oracle_root.sh`, which finds
  `ghostty` beside the MAIN checkout from any worktree, so no override is needed. An exported
  `GHOSTTY_ROOT` is silently ignored.
- `Package.resolved` `originHash` is unchanged by the `exactVersion` requirement in BOTH trees
  (oracle kept `e721da7f…`); xcodebuild accepted it without rewriting the file.
