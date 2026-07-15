# Vendored ghostty themes corpus

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

## R2 identity note — install path IS rebranded (Task 14 correction)

The theme file **contents** (each `.conf`'s color values) are vendored data,
untouched either way. The **install path**, however, is fully identity-routed:
`xtask/src/res_themes.rs` installs this corpus to `share/{APP}/themes/` —
`share/rustty/themes/` by default, `share/ghostty/themes/` only under the
`identity-oracle` parity harness — and `rustty-config`'s theme resolution
(used by both the ported `list_themes` fuzzy-picker and the `theme =` config
setting) looks up `share/{APP_NAME}/themes` dynamically via
`rustty_identity::APP_NAME`. There is no hard-coded `ghostty` path on either
side (an earlier version of this note claimed otherwise; that claim was
verified false and corrected — see Task 14's report).

Do not hand-edit these theme files — they are upstream data. To refresh,
re-vendor verbatim from the pinned `iterm2_themes` tarball.
