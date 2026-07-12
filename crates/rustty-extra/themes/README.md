# Vendored ghostty themes corpus (data identity: `ghostty`)

This directory holds the **iTerm2-Color-Schemes-derived color-theme `.conf` files**
that ghostty ships under `share/ghostty/themes/`, vendored **verbatim** into
rustty's tree so the build has **no dependency on the ghostty checkout** (R3
self-contained) and no network fetch of the upstream tarball.

## Provenance (R1 fidelity)

Vendored byte-for-byte from ghostty's `iterm2_themes` build dependency:

- Upstream tarball: `ghostty-themes-release-20260608-160426-8c84dd1.tgz`
  (`https://deps.files.ghostty.org/`), pinned in ghostty's `build.zig.zon`
  under the `iterm2_themes` dependency
  (hash `N-V-__8AAM7eAwCK4N5RemHoJXGZr7eQl49L6r8y1WJhzZGl`).
- Ghostty installs it via `src/build/GhosttyResources.zig` (the `emit_themes`
  block) into `share/ghostty/themes`, `exclude_extensions = &.{".md"}`.
- **534** theme files, byte-identical to the upstream set (this `README.md` is
  the only non-theme file here and is **excluded on emit** — mirroring ghostty's
  `.md` exclusion, so the shipped count stays exactly 534).

## R2 data-identity note — why `ghostty`, not `rustty`

The product is **Rustty**, but this corpus keeps ghostty's **data identity**:

- It installs to `share/ghostty/themes/` (not `share/rustty/…`).
- The reused macOS Swift app and the ported `list_themes` fuzzy-picker / `theme`
  config setting resolve theme names against the `share/ghostty/themes` path the
  app hard-codes.

Renaming the directory or the corpus would break the picker and the `theme =`
setting lookup, so the ghostty name is retained deliberately (R2).

Do not hand-edit these files — they are upstream data. To refresh, re-vendor
verbatim from the pinned `iterm2_themes` tarball.
