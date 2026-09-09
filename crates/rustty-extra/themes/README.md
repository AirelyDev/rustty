# Vendored ghostty themes corpus

This directory holds the **iTerm2-Color-Schemes-derived color-theme `.conf` files**
that ghostty ships under `share/ghostty/themes/`, vendored **verbatim** into
rustty's tree so the build has **no dependency on the ghostty checkout** (R3
self-contained) and no network fetch of the upstream tarball.

## Provenance (R1 fidelity)

Vendored byte-for-byte from ghostty's `iterm2_themes` build dependency:

- Upstream tarball: `ghostty-themes-release-20260824-153547-75c93ee.tgz`
  (`https://deps.files.ghostty.org/`), pinned in ghostty's `build.zig.zon`
  under the `iterm2_themes` dependency
  (zig hash `N-V-__8AAGZkBACH0haGC9R-hfNPYWxu16hr-ydEsolad9GV`; the tarball
  file's own SHA-256 is
  `b1fd1f31057f97c1274db03c6ce44960837a1a41f52275cbc64aee2e6e34eb9e`,
  which is exactly the `sha256` ghostty records for it in
  `flatpak/zig-packages.json` — verified on the downloaded file with
  `shasum -a 256`; `zig fetch` was not run, so the zig hash is cited from the
  oracle, not reproduced).
  Re-vendored by phase-6 `w6-platform-tail` (oracle `094d175ef`, range
  `88f57ee66..20abdb50a`) from `…20260810-152212-0173c3c`: **+4 themes**
  (`Grok Day`, `Grok Night`, `Warm Burnout Dark`, `Warm Burnout Light`),
  **0 content updates**, 0 removals (602 -> 606; the G-THEMES gate reported
  exactly those four `ADDED` rows against the old manifest before `--write`).
  Previous hop, phase-4 `w8-themes` (reconciliation orphan O7, oracle range
  `6ad1fe7d8..12967b68f`) from `…20260720-153658-97e244c`: **+10 themes**
  (`CRT Amber`, `Cassette Tape`, `Midnight Arcade`, `Outrun Electric`,
  `Purple Portal`, `Terminal Green 1999`, `Vaporwave Sunset`, `Y2K Chrome`,
  `datum (dark)`, `datum (light)`), **1 content update**
  (`JetBrains Islands Dark`), 0 removals.
  Previous hop, `P3-W9 T5` (oracle `4c1d69696` `b513f1b20` `02504bcce`
  `07a56d08b` `dc8d38503`) from `…20260608-160426-8c84dd1`: +58 themes,
  4 content updates (the four `Catppuccin *` files), 0 removals.
- Ghostty installs it via `src/build/GhosttyResources.zig` (the `emit_themes`
  block) into `share/ghostty/themes`, `exclude_extensions = &.{".md"}`.
- **606** theme files, byte-identical to the upstream set (`README.md` and
  `LICENSE.md` are the only non-theme files here and are **excluded on emit** —
  mirroring ghostty's `.md` exclusion, so the shipped count stays exactly 606).

## The digest gate (`G-THEMES`) — read this before re-vendoring

`xtask/src/res_themes.rs` asserts the emitted corpus has exactly 606 files.
That catches an added or deleted theme and **nothing else** — content drift
inside an existing theme file is invisible to a count, and only ONE theme body
is ever read by a test (`0x96f`, spot-checked for `palette = 0=`). Content
drift is not hypothetical: the `20260720` hop changed the four `Catppuccin *`
bodies, and the `20260810` hop changed `JetBrains Islands Dark`.

`crates/rustty-extra/themes.manifest` therefore carries a SHA-256 per file plus
a total digest over the rows, checked by
`.port/tools/gates/check_theme_corpus.py`. After a deliberate re-vendor, run
`python3 .port/tools/gates/check_theme_corpus.py --write` and commit the
manifest with the corpus.

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
re-vendor verbatim from the pinned `iterm2_themes` tarball
(`.port/phase-3/w9-revendor-themes.py <extracted-dir>`, which preserves the
single R2 filename rename below), then regenerate the digest manifest.

**R2 filename rename (exactly one).** Upstream ships
`Ghostty Default Style Dark`; rustty vendors it as
`Rustty Default Style Dark`, contents byte-identical. It is the only identity
token anywhere in the corpus — no theme BODY contains one (proven by the
byte-compare in `.port/phase-3/w9-themes-diff.py` on the `20260720` corpus,
and re-proven on the `20260810` and `20260824` corpora by a case-insensitive
`grep -rIl 'ghostty|rustty'` over the extracted tarball: filenames matched
only `Ghostty Default Style Dark`, bodies matched nothing).

## Licensing

The corpus derives from [iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes)
(MIT); the upstream license text is vendored verbatim in `LICENSE.md` in this
directory (repo-side attribution — excluded from the app bundle by the `.md`
emit filter, keeping the shipped set exactly 606 files like ghostty's).
Per-theme copyright belongs to each theme's author (see the upstream license's
final clause).
