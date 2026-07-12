# Vendored ghostty shell-integration (rustty)

This directory is a **verbatim vendored copy** of ghostty's
`src/shell-integration/` tree (bash, zsh, fish, elvish, nushell integration
scripts). It is checked into rustty's own source tree so the build has **no
run/build dependency on the ghostty checkout** (R3 self-contained).

## Provenance

- Source: `ghostty` `src/shell-integration/` @ `4749c4e9`.
- Copied byte-for-byte (R1 fidelity). Do not hand-edit; re-vendor from the
  oracle if ghostty updates upstream.

## Kept ghostty / protocol identifiers (R2 data identity)

Although the product is **Rustty**, these scripts intentionally keep ghostty's
names and protocol/OSC identifiers **unchanged**:

- `xterm-ghostty` — the `TERM` value the reused Swift app + emitted escape
  sequences expect.
- `GHOSTTY_*` environment variables, the `ghostty` command name, `ghostty-integration`
  filenames, and the OSC / DCS sequences the scripts emit.
- Install path `share/ghostty/shell-integration/` (see below).

These are DATA/PROTOCOL identity, not product branding — the reused macOS
`Ghostty.app` and terminal escape handling key off them, so renaming would
break integration. Product branding ("Rustty") is applied elsewhere, never to
this vendored data.

## Consumption

`xtask`'s `res_shell_integration` module (`xtask/src/res_shell_integration.rs`)
embeds these files via `include_bytes!` and, during `cargo run -p xtask --
resources`, copies them into
`zig-out/share/ghostty/shell-integration/` (excluding `*.md`, matching
ghostty's `GhosttyResources.zig` `exclude_extensions = &.{".md"}`). The macOS
Xcode Copy-Resources phase then bundles that tree into the app.

`README.md` (ghostty's upstream readme) and this `VENDORING.md` are `*.md` and
are therefore **not** emitted into the app bundle — only the executable
integration scripts ship.
