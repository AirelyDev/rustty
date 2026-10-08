# Ferris icon rebrand — design

**Date:** 2026-10-08
**Branch:** `icons-rebrand` (worktree `.claude/worktrees/icons-rebrand`, off local `main` @ 401b95b0e)
**Status:** implemented on branch `icons-rebrand`; the art was approved at four review gates (deviations in "As built" below)

## Goal

Every Rustty icon still shows Ghostty's ghost. Replace the ghost with Ferris the
Rustacean in every icon asset, keep each icon's existing background styling, and
make the official-icon exports reproducible from one source.

The website fork (`~/projects/ghostty-website`) is the reference for the crab
only (the stock rustacean.net happy Ferris). Its dark teal/orange site palette
and its flat terminal-window logo are **not** adopted.

## Locked decisions

| #   | Decision                                                                                                                                                                                                                                                                                             | Why                                                                                                                                                                                                                                        |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| D1  | Mascot = stock rustacean.net **happy** Ferris (`rustacean-flat-happy.svg`, CC0), stock colours body `#F74C00`, shade `#A52B00`, black eyes with white highlights. No prompt glyphs.                                                                                                                  | Matches the website's static art. Prompt-glyph eyes (`>–`, `>_`) were mocked and rejected: they collapse to a smudge at ≤ 32 px.                                                                                                           |
| D2  | **Centred and larger** placement everywhere (except Microchip, see As built) (brainstorm mockup "B"). Ferris width = **0.68 × the screen's width**, horizontally centred, vertical centre **1.2 % of screen height above** the screen centre. Microchip is the one exception (lifted; see As built). | The ghost's top-left slot makes a wide crab tiny and unreadable at small sizes.                                                                                                                                                            |
| D3  | **Backgrounds unchanged.** Official icon keeps the blue CRT screen, bezel, dot overlay, gloss. Alternates keep their plates.                                                                                                                                                                         | User direction: only the mascot changes.                                                                                                                                                                                                   |
| D4  | Alternates get a **style-matched** Ferris drawn in each icon's material (table below), not a pasted orange crab.                                                                                                                                                                                     | Mocked side by side; orange looked pasted on.                                                                                                                                                                                              |
| D5  | Custom-style mascot layer is a **neutral grey Ferris** that keeps its shading.                                                                                                                                                                                                                       | `ColorizedRusttyIcon` tints the layer with a `.color` blend from `macos-icon-ghost-color`. That only works on a near-neutral layer.                                                                                                        |
| D6  | Config key `macos-icon-ghost-color` and its doc text stay **as in Ghostty**.                                                                                                                                                                                                                         | Parity and config compatibility. Rewording would trip the config-doc parity audit (`docs/parity-audit/src-config.md`).                                                                                                                     |
| D7  | The generator is **Python 3 (Pillow + numpy) plus the `resvg` and `ictool` CLIs**, at `images/tools/build_icons.py`. Outputs are committed.                                                                                                                                                          | Fast art iteration. Python scripts already exist in-repo. Builds and CI never run it. It is macOS-only because of `ictool`. The Rust-`xtask` alternative was rejected: it would add `image`/`resvg`/`imageproc` deps for a rarely run job. |
| D8  | Ghost-free alternate **plates are produced once, reviewed, and committed.** The generator never re-runs ghost removal.                                                                                                                                                                               | Deterministic output from committed inputs. Fill quality is judged by eye once, not re-derived on every run.                                                                                                                               |

## As built (deviations approved during implementation)

- **Custom-style levels:** body 135 / shade 85 / dark 20 / white 255, chosen by the user at review gate 4 (option B). The spec's ~235/~170 gave pastel tints, because the `.color` blend keeps the backdrop's luminance and #F74C00's luminance is about 119.
- **Mirror passes:** they take an optional `dx`, because the plates' symmetry axis is about x 508, not 511.5.
- **Retro:** the pixel cell is w/30, not ≈1/22, and the pixel sprite is deliberately left-right symmetrised. Both are user-approved at gate 2.
- **Microchip:** Ferris is lifted via `alternates.PLACEMENT_OFFSET` (0, -103) so the orange trace stays visible. The user requested it at gate 3, and it is the only exception to D2's centring.
- **Glass:** a stronger frosted style (fill alpha 0.62/0.50, white rim, cool tint, soft dark edge), user-requested at gate 3.
- **Retro plate:** it shows the original gloss mirrored to the left, approved at gate 2.

## Current inventory (review findings)

| Group                    | Files                                                                                                                                                                                                                                                                                                                                 | Current state                                                                                                                                                   | Consumers                                                                                                                                                                                                                                                                                                                                               |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G1 Icon Composer source  | `images/Rustty.icon/icon.json` + `Assets/{Rustty,Screen,Screen Effects,Inner Bevel 6px,gloss}.png`                                                                                                                                                                                                                                    | Byte-identical to Ghostty. `Rustty.png` (475×541) is the ghost. It is used by layer `Rustty` and by glass layer `RusttyBlur`, filled `extended-srgb:0,0.478,1`. | Xcode/actool → macOS app icon (`ASSETCATALOG_COMPILER_APPICON_NAME = Rustty`)                                                                                                                                                                                                                                                                           |
| G2 Official flat exports | `macos/Assets.xcassets/AppIconImage.imageset/` (256/512/1024); `images/gnome/{16,32,64,128,256,512,1024,2048}.png`; `crates/rustty-apprt/assets/images/gnome/` (byte mirror); `images/icons/icon_{16,32,128,256,512,1024}{,@2x}.png` (same images renamed); `dist/windows/rustty.ico` (16/32/48/256); `dist/doxygen/favicon.png` (32) | All show the ghost. The `images/gnome` bytes differ from Ghostty's only because they are re-exports of `AppIconImage`.                                          | SettingsView, ErrorView and DockTilePlugin reset (`AppIconImage`); GTK gresource (`crates/rustty-apprt/build.rs`); Linux hicolor staging (`xtask/src/res_linux.rs`); snap (`snap/snapcraft.yaml: icon: images/icons/icon_512.png`); Windows exe resource (`dist/windows/rustty.rc`); Doxygen (`Doxyfile` `PROJECT_LOGO = images/gnome/64.png`, favicon) |
| G3 Custom-style layers   | `macos/Assets.xcassets/Custom Icon/*.imageset` (9)                                                                                                                                                                                                                                                                                    | Only `CustomIconGhost` (`ghosty.png`, top-left at bbox 232,227–475,520, `template-rendering-intent: template`) has a mascot.                                    | `ColorizedRusttyIcon.makeImage`                                                                                                                                                                                                                                                                                                                         |
| G4 Alternates            | `macos/Assets.xcassets/Alternate Icons/{Blueprint,Chalkboard,Glass,Holographic,Microchip,Paper,Retro,Xray}Image.imageset/macOS-AppIcon-1024px.png`                                                                                                                                                                                    | Byte-identical to Ghostty, flattened. No layer sources exist anywhere. Ghost is at approx. x 220–470, y 200–520.                                                | `AppIcon.swift`, `DockTilePlugin.swift`, `Rustty.ConfigTypes.swift` (`macos-icon`)                                                                                                                                                                                                                                                                      |
| — Not branding           | `ResetZoom.imageset/ResetZoom.pdf`, `crates/rustty-font/res/*`, `crates/rustty-wuffs/fixtures/*`                                                                                                                                                                                                                                      | —                                                                                                                                                               | untouched                                                                                                                                                                                                                                                                                                                                               |

There is also a factual error in the code comments today. `crates/rustty-apprt/src/gtk/gresource.rs:45-85`
and `crates/rustty-apprt/tests/gtk_icon_identity.rs` say the GTK icons are
"Rustty art, NOT Ghostty", but they show the ghost. The test only checks that the
mirror matches `images/gnome`, so it could not catch this. This rebrand makes
the claim true; the hash table and wording get refreshed.

## Source layout (new)

```
images/
  ferris/
    rustacean-flat-happy.svg   # verbatim from https://rustacean.net/assets/ (CC0)
    PROVENANCE.md              # URL, retrieval date, sha256, CC0 waiver link
  alternates/
    <name>/plate.png           # 1024², ghost removed, reviewed, committed (8 dirs)
  tools/
    build_icons.py             # the generator (D7)
    README.md                  # prerequisites + how to run + review checklist
  Rustty.icon/                 # existing; Assets/Rustty.png + icon.json regenerated/edited
```

## Generator (`images/tools/build_icons.py`)

Run it from the repo root on macOS with Xcode 26+. Prerequisites are `python3`
with Pillow and numpy, `resvg` (Homebrew), `icotool` (icoutils), and Icon
Composer's `ictool` at
`/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool`
(override with `ICTOOL=`). It fails fast and names any tool that is missing.

The subcommands can be run one at a time, and `all` runs them in order:

1. **`mascot`**: rasterise the Ferris SVG with `resvg` at a high resolution (≥ 2400 px wide). Derive:
   - `images/Rustty.icon/Assets/Rustty.png`: orange Ferris with a soft orange (`#FF5A00`) outer glow and transparent padding. Sized so that, with the `icon.json` translation below, D2 holds against `Screen.png` (825 pt screen → ≈ 560 pt Ferris).
   - `macos/Assets.xcassets/Custom Icon/CustomIconGhost.imageset/ghosty.png`: 1024² canvas, grey Ferris placed per D2 against the screen-mask bbox (174–848). Luminance mapping: body → 135, shade → 85, dark 20, highlights white 255 (as built; the originally planned ~235/~170 gave pastel tints, see As built), plus a soft light glow like today's `ghosty.png`. The file name, asset name and `Contents.json` (incl. `template-rendering-intent`) are unchanged.
2. **`official`**: edit `images/Rustty.icon/icon.json`:
   - both `Rustty.png` layers get `translation-in-points` `[0, -10]` and scale 1;
   - the `RusttyBlur` glass fill changes from blue to `extended-srgb:0.96863,0.29804,0.00000,1.00000` (`#F74C00`).

   Nothing else in `icon.json` changes.

3. **`flats`**: render `Rustty.icon` with `ictool --export-image --platform macOS --rendition Default` at 1024 and 2048. `ictool` renders edge to edge, so the generator then fits the result to the current export geometry: opaque bbox **96,100–919,923** in a 1024 canvas, scaled proportionally at the other sizes, with the existing drop shadow. Downscaling uses Lanczos from the 2048 master.
   - Writes `AppIconImage` (256/512/1024, existing file names), `images/gnome/*` (8 sizes), the byte mirror `crates/rustty-apprt/assets/images/gnome/*`, `images/icons/*` (12 names, keeping today's name → size mapping), `dist/windows/rustty.ico` (16/32/48/256, 32-bit, via `icotool -c`), and `dist/doxygen/favicon.png` (32).
   - **Framing check:** the new 1024 export's opaque bbox must be within ±2 px of 96,100–919,923. Its alpha mask must also have **IoU ≥ 0.98** against the pre-rebrand `images/gnome/1024.png`, read via `git show 401b95b0e:images/gnome/1024.png`. The script prints both results and exits non-zero on failure.
4. **`alternates`**: for each of the 8, composite `images/alternates/<name>/plate.png` with that icon's styled Ferris, placed per D2 against that plate's screen bbox (measured once and recorded in the script), and write the imageset PNG.

The generator is deterministic. Two consecutive `all` runs on the same machine
must produce byte-identical outputs (any random texture uses a fixed seed). PNGs
are written without timestamps or text chunks.

### Style-matched Ferris per alternate (D4)

Each style mirrors how that icon draws its ghost today:

| Alternate   | Ferris treatment                                                                                                                                                             |
| ----------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Chalkboard  | Bright chalk white with grain, slightly broken edges, shade a little dimmer, eyes/mouth as holes showing the board                                                           |
| Paper       | Saturated fuzzy blue ink (fibrous edge), eyes/mouth as paper-coloured holes                                                                                                  |
| Blueprint   | White fuzzy fill matching the blueprint ghost, with the eyes/mouth showing blueprint blue                                                                                    |
| Retro       | Green phosphor pixel art (cell w/30 as built, sprite left-right symmetrised; see As built), darker green shade, eyes as dim pixels, glow, plate scanlines visible through it |
| Holographic | Brushed chrome (concentric/conic sheen), iridescent pink rim halo, eyes with the rainbow sheen the ghost's face has                                                          |
| Microchip   | Brushed chrome like the ghost, with the plate's orange trace left untouched                                                                                                  |
| Xray        | Brushed chrome like the ghost over the board                                                                                                                                 |
| Glass       | Frosted translucent glass: low-alpha light fill, bright rim, dark eyes, like the glass ghost (built stronger; see As built)                                                  |

The brainstorm prototypes (chalk, ink, pixel, chrome) are a starting point. The
final art is iterated with the user in the review batches.

### Plate production (D8)

This is a one-off step that is not part of `build_icons.py all`. A helper,
`build_icons.py plate <name>`, writes a draft plate by texture-aware cloning: a
feathered patch from a clean region of the same plate, with shifts that are
multiples of any periodic texture (Retro scanlines ≈ 59.6 px; Paper rule lines;
Blueprint grid). The ghost's glow halo is covered too, not just its body. Seams
are then retouched until review passes, and the reviewed plate is committed.
Ferris covers part of the ghost's old region, but the area above Ferris must be
clean on its own.

## Repo changes outside the generator

- `crates/rustty-apprt/src/gtk/gresource.rs`: refresh the `images/gnome/*` sha256 table (lines 78–85). Its W14-01 wording stays true and gains "(Ferris art)".
- `crates/rustty-apprt/tests/gtk_icon_identity.rs`: unchanged logic, doc comment stays accurate.
- `NOTICE`: add a Ferris credit ("Ferris the Rustacean artwork from rustacean.net, CC0 1.0").
- `README.md`: add a logo `<img>` (`images/gnome/256.png`, width 128) in the `<!-- LOGO -->` header, matching Ghostty's README structure.
- No changes to Swift sources, the Xcode project, `xtask`, the snap/flatpak manifests, the `Doxyfile`, or config.

## Review checkpoints (with the user, via the brainstorm browser companion)

1. **Official icon:** 1024 plus 256/64/32/16 strips, light and dark backgrounds.
2. **Alternates batch 1:** Chalkboard, Paper, Blueprint, Retro (plates and final).
3. **Alternates batch 2:** Holographic, Microchip, Xray, Glass.
4. **Custom style:** 4 frames × a few tints (orange, white, green, purple) rendered through the same blend modes as `ColorizedRusttyIcon`.

## Verification

- **Generator:** a determinism re-run (byte-identical), framing IoU ≥ 0.98, and a size/format check of every output (dimensions, RGBA, ICO entries 16/32/48/256).
- **No ghost left:** the generator lists every PNG/ICO under G1–G4 and asserts that each file's sha256 differs from its pre-rebrand value, except the layers that are explicitly unchanged.
- **Tests:**
  - `cargo test -p rustty-apprt --test gtk_icon_identity` (any host);
  - `cargo test -p rustty-apprt --test gtk_assets` and `xtask resources --target-os linux` staging on the Linux VM (rvm);
  - `cargo test -p rustty --test windows_resources` on the Windows VM (rwin).
  - Run the VM lanes one at a time and clean up after each.
- **macOS:** build the app (`xcodebuild`), launch it, and check visually:
  - dock and Finder icon;
  - Settings and the error view (`AppIconImage`);
  - every `macos-icon` value;
  - `custom-style` with each frame;
  - the DockTilePlugin reset path.

## Out of scope

- The website and its palette or logo.
- Wording changes to config docs (D6).
- New icon variants or settings beyond Ghostty's set.

## Risks

- **`ictool` framing vs Xcode compile.** The flat exports are generated from `ictool` and the dock icon from actool, and they may differ slightly in lighting. This is acceptable as long as the shape matches; checkpoint 1 compares both.
- **Plate fills.** Microchip and Xray have non-periodic circuitry under the ghost, so they need the most retouching. Budget extra iteration time for them.
- **Small sizes.** Ferris at 16 px is a small orange blob on blue. That is accepted (it was mocked), but it is checked again at checkpoint 1.
- **Tool availability.** `ictool` needs Xcode 26+. The generator documents this, and no build depends on it.
