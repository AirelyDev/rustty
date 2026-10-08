# Icon generator

Rebuilds every derived Rustty icon from committed sources. Spec:
`docs/superpowers/specs/2026-10-08-ferris-icon-rebrand-design.md`.

## Prerequisites (macOS only)

- Xcode 26+ (Icon Composer's `ictool`; override the path with `ICTOOL=`)
- `brew install resvg icoutils`
- Python 3 with Pillow and numpy

## Commands (run from anywhere)

```
python3 -I images/tools/build_icons.py all        # rebuild everything
python3 -I images/tools/build_icons.py verify     # no-ghost check + determinism (runs `all` twice)
python3 -I images/tools/build_icons.py mascot | official | flats | alternates [Name ...]
python3 -I images/tools/build_icons.py plate <Name>   # one-off: redraft a ghost-free plate
python3 -I images/tools/build_icons.py review official|custom|alternates [Name ...]
```

Tests: `python3 -I -m unittest discover -s images/tools/tests -t images/tools -v`

## Sources → outputs

| Source                                                     | Outputs                                                                                                                                                              |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `images/ferris/rustacean-flat-happy.svg` (CC0)             | `Rustty.icon/Assets/Rustty.png`, `Custom Icon/CustomIconGhost.imageset/ghosty.png`                                                                                   |
| `images/Rustty.icon` (via `ictool`)                        | `AppIconImage.imageset/*`, `images/gnome/*` (+ `crates/rustty-apprt/assets/images/gnome/*`), `images/icons/*`, `dist/windows/rustty.ico`, `dist/doxygen/favicon.png` |
| `images/alternates/<name>/plate.png` + `iconlib/styles.py` | `Alternate Icons/<Name>Image.imageset/macOS-AppIcon-1024px.png`                                                                                                      |

After regenerating the GTK icons, refresh the sha256 table in
`crates/rustty-apprt/src/gtk/gresource.rs` and run
`cargo test -p rustty-apprt --test gtk_icon_identity`.

## Notes

- Alternate plates (`images/alternates/<name>/plate.png`) are the original
  icon backgrounds with the ghost removed. They were drafted once with
  `plate <Name>` (from the pre-rebrand revision `401b95b0e`), then retouched
  by hand and reviewed. `all` never re-runs ghost removal; it only composes
  the committed plates with a style-matched Ferris.
- Ferris is placed at 0.68 of the screen width, centred horizontally and 1.2%
  of the screen height above the screen centre. `PLACEMENT_OFFSET` in
  `iconlib/alternates.py` holds explicit per-icon nudges applied after that
  placement (Microchip is lifted so Ferris does not hide the orange trace).
- Randomness uses fixed seeds and PNGs carry no text, time or ICC chunks, so
  two runs are byte-identical. `verify` checks this, and also that every
  derived asset differs from the pre-rebrand revision.
