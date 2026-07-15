# Vendored ghostty shell-integration (rustty)

This directory started as a **verbatim vendored copy** of ghostty's
`src/shell-integration/` tree (bash, zsh, fish, elvish, nushell integration
scripts). It is checked into rustty's own source tree so the build has **no
run/build dependency on the ghostty checkout** (R3 self-contained).

## Provenance

- Source: `ghostty` `src/shell-integration/` @ `4749c4e9`.
- Initially copied byte-for-byte (R1 fidelity), then rebranded in full (see
  below). If re-vendoring from a newer ghostty checkout, re-apply the same
  rename pass rather than reverting to the raw upstream bytes.

## Rebranded identifiers (Task 13 — R2 product identity)

Unlike the man-page prose / bat-syntax "documentation data-identity"
carve-outs elsewhere in this port, these scripts are **live, executed shell
code that runs inside every rustty session**, so Task 13 renamed every
`GHOSTTY_*`/`ghostty` identifier inside them to `RUSTTY_*`/`rustty`
unconditionally (there is no `identity-oracle` dual-profile for this
content — a leftover `ghostty` reference here would be a real user-facing
bug, not a harmless data label):

- `RUSTTY_*` environment variables (e.g. `RUSTTY_RESOURCES_DIR`,
  `RUSTTY_SHELL_FEATURES`, `RUSTTY_BASH_INJECT`), the `rustty` command name,
  and `rustty-integration`/`rustty.nu`/`rustty.bash` filenames.
- Install path `share/{APP}/shell-integration/` — `share/rustty/…` by
  default, `share/ghostty/…` only under the `identity-oracle` parity harness
  (see `xtask/src/res_shell_integration.rs`).

The OSC / DCS terminal-protocol sequences the scripts emit are unchanged
protocol bytes (those are VT wire format, not a "ghostty" identifier).

## Consumption

`xtask`'s `res_shell_integration` module (`xtask/src/res_shell_integration.rs`)
embeds these files via `include_bytes!` and, during `cargo run -p xtask --
resources`, copies them into
`zig-out/share/{APP}/shell-integration/` (excluding `*.md`, matching
ghostty's `GhosttyResources.zig` `exclude_extensions = &.{".md"}`). The macOS
Xcode Copy-Resources phase then bundles that tree into the app.

`README.md` (developer documentation for this rebranded copy) and this
`VENDORING.md` are `*.md` and are therefore **not** emitted into the app
bundle — only the executable integration scripts ship.
