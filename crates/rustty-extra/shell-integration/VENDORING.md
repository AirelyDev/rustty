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
- Backport (W11-01): the `@complete external` attributes on the Nushell
  `ssh`/`sudo` wrappers in `nushell/vendor/autoload/rustty.nu` were added
  upstream after the `4749c4e9` copy (ghostty `7fa43764`, present at the
  audit pin `6ad1fe7d`) and backported here to restore external-completer
  delegation parity.
- Re-sync (P7-W11 T1, `5d6615fc4`, `4dfa44ecd`, `ed7f046ee`, `591ccacbd`,
  `a4031c8e3`): `bash/bash-preexec.sh` and `bash/rustty.bash` were
  transplanted from `ghostty` `src/shell-integration/bash/` at the Phase 7
  pin `12752b2ac` (bash-preexec 0.6.0 -> 0.7.0; `ghostty.bash`'s
  `__ghostty_status`/`__ghostty_restore_status` -> `__rustty_status`/
  `__rustty_restore_status`), proven at 0 diff lines against the rebrand
  transform (`cargo test -p xtask -- --ignored oracle_shell_integration_parity`).
  Every other file in this tree (zsh, fish, elvish, nushell) is still at the
  `4749c4e9` pin above; re-sync those the same way when their turn comes,
  rather than treating this line as covering the whole directory.

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
