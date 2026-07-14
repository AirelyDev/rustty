# Parity naming (port ↔ oracle)

The rustty port is the SUBJECT; ghostty is the ORACLE. Gap-comparison tooling
sources `.port/tools/parity_names.env` for the port↔oracle name pairs so the
rebrand never makes the port look divergent from the oracle.

## Two ways parity is proven (both name-aware)
1. **String/behaviour parity** — build rustty **with `--features identity-ghostty`**,
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
- every `#[cfg(feature = "identity-ghostty")]` string block — the oracle-string profile
- `include/ghostty.h`                            — LOCAL frozen oracle umbrella (product = include/rustty-embed.h)

## Becomes rustty (the port — renamed by this plan)
- all C exports: ghostty_* -> rustty_* (alias deleted, Task 1)
- macos/ Swift app files/dirs/targets/module (GhosttyKit -> RusttyKit)
- runtime identity strings (routed through rustty-identity)
- shell-integration scripts + GHOSTTY_* env vars -> RUSTTY_*
- trace files: main_ghostty.rs -> main_rustty.rs, cli/ghostty.rs -> cli/rustty.rs

## Residual-grep rule
A gate that fails on a stray `ghostty` MUST exclude the carve-out paths above
and MUST allow `#[cfg(feature = "identity-ghostty")]` blocks and the `ghostty_name`
TSV column.

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
- `ci.sh`'s `run_test_counts()` and `check_ghostty_h_abi.sh` intentionally keep
  their own literal `/Users/gael/projects/ghostty` (mirroring
  `check_test_counts.py`'s hardcode, per that function's own "must stay
  identical" comment) — they are out of scope for this task and are left
  untouched.
