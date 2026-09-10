# Parity naming (port ↔ oracle)

The rustty port is the SUBJECT; ghostty is the ORACLE. Gap-comparison tooling
sources `.port/tools/parity_names.env` for the port↔oracle name pairs so the
rebrand never makes the port look divergent from the oracle.

## Two ways parity is proven (both name-aware)
1. **String/behaviour parity** — build rustty **with `--features identity-oracle`**,
   which flips every `rustty-identity` string constant (APP_NAME, TERM, ENV_PREFIX,
   terminfo names, config dir) to its ghostty value. Byte output then matches the
   oracle. This flips STRINGS, never filenames or symbols.
2. **Symbol parity** — `.port/audits/CABI_SYMBOLS.tsv` maps every
   `ghostty_name` (oracle header) to its live `rustty_name` export.
   `check_symbol_manifest.sh` uses this bridge; the alias forwarders are gone
   (rebrand Task 1) — the Swift app links `rustty_*` directly.

## Stays ghostty (the oracle — never renamed)
- `/Users/gael/projects/ghostty`                 — external read-only oracle tree
- `/Users/gael/projects/ghostty/include/ghostty.h` — oracle C-ABI header
- `.port/phase-0/corpora/**`, `baseline/**`, `examples-out/**` — captured oracle golden bytes
- `.port/audits/GHOSTTY_LIFETIMES.tsv`           — lifetime audit OF ghostty source
- the `ghostty_name` column of `CABI_SYMBOLS.tsv` — the oracle symbol names
- ghostty's upstream `include/ghostty.h` (via `ORACLE_HEADER_PATH`, SHA-256 pinned) — the oracle umbrella (product = include/rustty-embed.h); no longer vendored locally

## Becomes rustty (the port — renamed by this plan)
- all C exports: ghostty_* -> rustty_* (alias deleted, Task 1)
- macos/ Swift app files/dirs/targets/module (GhosttyKit -> RusttyKit)
- runtime identity strings (routed through rustty-identity)
- shell-integration scripts + GHOSTTY_* env vars -> RUSTTY_*
- trace files: main_ghostty.rs -> main_rustty.rs, cli/ghostty.rs -> cli/rustty.rs
- **log SCOPE TAGS**: `std.log.scoped(.gtk_ghostty_*)` -> `LOG_SCOPE =
  "gtk_rustty_*"`. Ruled by task-14d-2 (which introduced `gtk_rustty_config`)
  and applied to the other 7 by task-14d-3 (`gtk_rustty_application`,
  `_surface`, `_window`, `_split_tree`, `_command_palette`, `_resize_overlay`,
  `_global_shortcuts`). Three reasons, recorded here rather than in one file's
  code comment:
    1. a scope tag IS a runtime identity string — it is emitted into
       user-visible diagnostic text (`warning(gtk_rustty_surface): ...`), which
       is exactly what the line above this one covers, and a hardcoded
       `ghostty` literal in new code is what the residual-grep rule targets;
    2. no oracle byte-parity is lost. Nothing in the workspace compares scope
       tags against the oracle, and there is no `identity-oracle` hook for
       them, so nothing that flips strings for oracle comparison is bypassed;
    3. it does NOT conflict with the GType registered `NAME = "GhosttyConfig"`
       / `"GhosttySurface"` / ... kept verbatim a few lines away in the same
       files. A GType name is a REGISTERED TYPE IDENTITY the reference's `.name`
       fixes and the type system keys off; a log scope is a label with no
       consumer.
  **Two things in this family are NOT rebranded**, and both have live examples:
  a URL into ghostty's own documentation inside a diagnostic message
  (`gtk/class/surface.rs`'s `https://ghostty.org/docs/help/gtk-opengl-context`)
  is upstream's page and still has to resolve; and a scope with no brand token
  (`gtk`, `openuri`, `gtk_x11`, `winproto_wayland`, `embedded_window`, ...) is
  carried verbatim, there being nothing to rename.
  If a later stage wants both spellings, route scope tags through
  `rustty-identity` so `--features identity-oracle` can flip them; each
  `LOG_SCOPE` constant is a one-line change.

## Residual-grep rule
A gate that fails on a stray `ghostty` MUST exclude the carve-out paths above
and MUST allow the `ghostty_name` TSV column. The `identity-oracle` feature
name is no longer a ghostty-branding exception — it carries no literal
`ghostty` substring, so it needs no carve-out from this rule (only the
ghostty-valued STRING CONSTANTS it flips to, e.g. `APP_NAME == "ghostty"`,
`TERM == "xterm-ghostty"`, are the oracle values and remain covered by the
"Stays ghostty" list above).

## How the map is sourced

`.port/tools/parity_names.env` is the single sourced port↔oracle name map:

```
PORT_NAME=rustty
ORACLE_NAME=ghostty
PORT_TERM=xterm-rustty
ORACLE_TERM=xterm-ghostty
PORT_ENV_PREFIX=RUSTTY
ORACLE_ENV_PREFIX=GHOSTTY
PORT_KIT=RusttyKit
ORACLE_KIT=GhosttyKit
PORT_HEADER=rustty.h
ORACLE_HEADER=ghostty.h
ORACLE_TREE=/Users/gael/projects/ghostty
ORACLE_HEADER_PATH=/Users/gael/projects/ghostty/include/ghostty.h
CABI_MAP=.port/audits/CABI_SYMBOLS.tsv
```

- **Bash gates** (`check_symbol_manifest.sh`, `run_differential.sh`) `source`
  it directly near the top and use the `$PORT_*` / `$ORACLE_*` variables
  instead of an inline literal.
- **Python gates** (`check_parity_surface.py`, `check_constants_parity.py`)
  load it with a tiny same-shaped `_parity_names(root)` loader (parses
  `KEY=VALUE` lines, skips blanks/comments) into a `PARITY_NAMES` dict, and
  read `PARITY_NAMES["PORT_NAME"]` / `PARITY_NAMES["ORACLE_TREE"]` instead of
  a hardcoded string literal.
- This is a **behaviour-preserving** change: every value in the map is
  identical to what was previously hardcoded, so sourcing it changes no gate's
  pass/fail outcome — it only removes the duplicated literal so a future
  rename has exactly one place to update the PORT_* side (the ORACLE_* side
  never changes).
- `check_constants_parity.py`'s oracle path is still NOT an environment-variable
  override (that would reopen the vacuous-green spoof hole its header warns
  about) — it is read from this checked-in, repo-committed file, which is the
  same trust boundary as the script's own source.
- `ci.sh`'s `run_test_counts()` intentionally keeps its own literal
  `/Users/gael/projects/ghostty` (mirroring `check_test_counts.py`'s hardcode,
  per that function's own "must stay identical" comment) — it is out of scope
  for this task and is left untouched.
- `check_ghostty_h_abi.sh` (the old external-oracle byte-freeze gate) was
  renamed `check_rustty_h_abi.sh`. It `source`s this file for `$PORT_UMBRELLA`
  / `$ORACLE_HEADER_PATH` / `$CABI_MAP` and validates the PRODUCT umbrella
  (`include/rustty-embed.h`) against ghostty's upstream `include/ghostty.h` via
  `CABI_SYMBOLS.tsv`'s name-map. The oracle is read from `$ORACLE_HEADER_PATH`
  (the pinned `/Users/gael/projects/ghostty` checkout) and pinned to an exact
  version by a SHA-256 (`.port/tools/oracle-ghostty-h.sha256`). **Update
  (2026-07-15):** the previously-vendored local carve-out `include/ghostty.h`
  was removed — the gate references the pinned oracle checkout directly rather
  than a byte-copy, and BLOCKs gracefully if that checkout is absent.
