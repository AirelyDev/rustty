# Full Rustty Rebrand Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert every product-facing "Ghostty" name in the port — the C-ABI link seam, the macOS Swift app, the runtime identity, and the source-fidelity trace names — to "Rustty", while keeping the differential-parity machinery working by making it explicitly name-aware (port = `rustty` ↔ oracle = `ghostty`).

**Architecture:** Three seams change together. (1) **C-ABI seam** — drop the `ghostty_*` alias layer entirely; the Swift app calls the already-canonical `rustty_*` symbols directly through a `RusttyKit` module. (2) **Swift app** — mechanical symbol/import repoint, then file/dir/Xcode-target renames. (3) **Runtime identity** — route the handful of hardcoded `xterm-ghostty` / `GHOSTTY_*` sites through the `rustty-identity` consts that already default to rustty. The `identity-ghostty` cargo feature is **retained** but **decoupled from the alias**: it now only flips identity *strings* so the byte-differential harness can still reproduce the ghostty oracle exactly.

**Tech Stack:** Rust (cargo workspace + `xtask`), Swift/SwiftUI + Xcode (`xcodebuild`), cbindgen, macOS Metal, shell-integration scripts (bash/zsh/fish/elvish/nushell), terminfo.

## Global Constraints

- **Canonical C symbols are `rustty_*`** — they already exist in `crates/rustty-apprt-c/src/lib.rs` (e.g. `rustty_app_new`, `rustty_surface_new`). The `ghostty_*` names are alias forwarders and are **deleted** by this plan.
- **Shipped Swift module = `RusttyKit`**; xcframework = `RusttyKit.xcframework`; combined static lib = `libRusttyKit.a`; umbrella header staged as `rustty.h` (from `include/rustty-embed.h` — the validated frozen `include/ghostty.h` umbrella token-renamed, NOT the cbindgen headers, which use a bare-name convention and miss config/benchmark). The frozen `include/ghostty.h` is KEPT as a local oracle carve-out (`ORACLE_UMBRELLA`) for name-mapped gap comparison.
- **Swift namespace enum `Ghostty` → `Rustty`** (the `enum Ghostty { … }` / `extension Ghostty` namespace used throughout `macos/Sources`).
- **TERM = `xterm-rustty`**, terminfo entry `rustty|xterm-rustty|Rustty`, env-var prefix `RUSTTY_`, config dir `rustty` — all sourced from `rustty-identity` consts (never hardcoded).
- **Preserve ghostty's TERM-downgrade ssh fallback** (converts TERM → `xterm-256color` on remote hosts lacking the entry). It is not identity-specific; do not remove it.
- **`identity-ghostty` feature is retained** as the parity/oracle profile: under it, every identity string flips back to the ghostty value so the differential harness reproduces the oracle byte-for-byte. It must **no longer** compile any `ghostty_*` C alias (the alias is gone).
- **Oracle carve-out — these KEEP ghostty names** because they *are* the external ghostty reference, not the rustty product: the read-only oracle tree `/Users/gael/projects/ghostty`; the captured baseline corpora under `.port/phase-0/**` (literal ghostty output bytes used for byte-differential comparison); `.port/audits/GHOSTTY_LIFETIMES.tsv` (a field-lifetime audit *of the ghostty source*). Any script or doc that references these must be made **name-aware**: it maps the rustty port artifact to its ghostty oracle counterpart explicitly, rather than assuming one name.
- **R1 fidelity / R2 identity / R3 self-contained / R4 acyclicity** rules from the port continue to apply. Renaming a fidelity-trace *code* file (e.g. `main_ghostty.rs`) keeps its header comment's `PORTS: ghostty src/…` provenance line intact — the trace lives in the comment, not the filename.
- **Every rename uses `git mv`** (never `rm`+`add`) so history is preserved. **Never `git push`, `git reset`, or `git stash`.** Retry on `index.lock` (sleep 3, ≤10×).
- **No parallel implementer subagents** — several tasks touch `crates/rustty-apprt-c` and `macos/Ghostty.xcodeproj/project.pbxproj`; they must run sequentially.

---

## File Structure

This plan renames rather than restructures. The units of change:

- **C-ABI seam** (`crates/rustty-apprt-c/src/ghostty_alias.rs`, `crates/rustty-config/src/ghostty_alias.rs`, `crates/rustty-bench/src/ghostty_alias.rs`, their `Cargo.toml` feature blocks, `xtask/src/main.rs::xcframework`, `include/`) — deletes the alias, renames the packaging output.
- **Swift app** (`macos/**`, `images/Ghostty.icon`) — symbol repoint, then filesystem + Xcode renames.
- **Runtime identity** (`crates/rustty-identity`, `crates/rustty-config/src/config_struct.rs`, `crates/rustty-terminfo`, `crates/rustty-termio/src/shell_integration.rs` + the env-write sites, `crates/rustty-extra/{shell-integration,themes}`, `xtask/src/main.rs::resources`) — route through identity consts + rename resources.
- **Trace names** (`crates/rustty/src/main_ghostty.rs`, `crates/rustty-cli/src/ghostty.rs`, `.port/tools/gates/check_ghostty_h_abi.sh`) — rename the port's own files.
- **Parity name-awareness** (the differential harness / gate scripts under `.port/tools`, `crates/rustty-terminfo` parity path) — make the port↔oracle mapping explicit.

Phases run top-to-bottom; within the C-ABI and Swift phases the order is load-bearing (drop alias → repoint symbols → rename files → repackage).

---

## Phase 0 — Parity name-awareness (the safety net, do FIRST)

### Task 0: Make the parity/differential machinery name-aware before any rename

Context: the differential/gap infrastructure already treats **the external `/Users/gael/projects/ghostty` checkout + the frozen `.port/phase-0/**` corpora** as the read-only ORACLE (these stay ghostty), the **rustty port** as the SUBJECT, and `.port/audits/CABI_SYMBOLS.tsv` (columns `ghostty_name<TAB>rustty_name<TAB>signature<TAB>source_file<TAB>darwin_only`) as the canonical symbol bridge that `check_symbol_manifest.sh` reads. The rebrand must **preserve** that port↔oracle mapping so a future "compare gaps against ghostty" run (symbols, config constants, terminfo caps, differential VT) does not see the two as artificially divergent just because the port renamed itself. This task makes the name correspondence an explicit, sourced map — not an assumption baked into each script.

**Files:**
- Read: `.port/tools/gates/check_symbol_manifest.sh`, `.port/tools/gates/run_differential.sh`, `.port/tools/gates/check_parity_surface.py`, `.port/tools/gates/check_constants_parity.py`, `.port/tools/ci.sh`
- Read: `.port/audits/CABI_SYMBOLS.tsv` (the ghostty↔rustty symbol map — the model for name-awareness)
- Read: `crates/rustty-identity/src/lib.rs:72-127` (the `#[cfg(feature = "identity-ghostty")] mod values` block)
- Create: `.port/tools/parity_names.env` (the single sourced port↔oracle name map)
- Create: `.port/tools/PARITY_NAMING.md` (the invariant + carve-out + how the map is used)

**Interfaces:**
- Produces: `.port/tools/parity_names.env` defining `PORT_NAME=rustty` / `ORACLE_NAME=ghostty`, `PORT_TERM=xterm-rustty` / `ORACLE_TERM=xterm-ghostty`, `PORT_ENV_PREFIX=RUSTTY` / `ORACLE_ENV_PREFIX=GHOSTTY`, `PORT_KIT=RusttyKit` / `ORACLE_KIT=GhosttyKit`, `PORT_HEADER=rustty.h` / `ORACLE_HEADER=ghostty.h`, `ORACLE_TREE=/Users/gael/projects/ghostty`, `CABI_MAP=.port/audits/CABI_SYMBOLS.tsv`. Later gap-comparison tooling sources this instead of hardcoding either name.

- [ ] **Step 1: Inventory every parity/gap script that keys on a rustty-or-ghostty name.**
```bash
cd /Users/gael/projects/rustty
grep -rn 'ghostty\|rustty\|identity-ghostty\|xterm-\|GHOSTTY_\|/projects/ghostty\|CABI_SYMBOLS' \
  .port/tools/gates/*.sh .port/tools/gates/*.py .port/tools/ci.sh 2>/dev/null \
  | grep -viE '^\s*#' | head -60
```
Expected: a finite list — `check_symbol_manifest.sh` (reads CABI_SYMBOLS.tsv + oracle header), `run_differential.sh` (`RUSTTY_VT_DRIVER` + phase-0 corpora), `check_parity_surface.py` (`infocmp rustty`), `check_constants_parity.py` (`ORACLE=/Users/gael/projects/ghostty`), `ci.sh` (oracle path). Note which side each keys on.

- [ ] **Step 2: Create `.port/tools/parity_names.env`** — the single source of truth:
```bash
cat > .port/tools/parity_names.env <<'EOF'
# Port <-> oracle name map. SOURCE this from any parity/gap-comparison script
# so the ghostty->rustty rebrand never makes the port look divergent from the
# ghostty oracle. The oracle side stays "ghostty" forever (it names the
# external reference); the port side is "rustty".
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
# External read-only oracle checkout (R3) + captured golden corpora — stay ghostty.
ORACLE_TREE=/Users/gael/projects/ghostty
ORACLE_HEADER_PATH=/Users/gael/projects/ghostty/include/ghostty.h
CABI_MAP=.port/audits/CABI_SYMBOLS.tsv
EOF
```

- [ ] **Step 3: Make the parity scripts source the map instead of hardcoding a single name.** For each script from Step 1, add near the top (bash) / import (python) a load of `parity_names.env` and replace the inline literal with the variable, WITHOUT changing behaviour:
  - `check_symbol_manifest.sh`: source the env; keep `GHOSTTY_H="${GHOSTTY_H:-$ORACLE_HEADER_PATH}"` (already the external oracle) and `CABI_SYMBOLS="${CABI_MAP}"`. The gate keeps reading the `ghostty_name→rustty_name` TSV as the bridge. (Its A2 "alias forwarder" section is removed by Task 1, not here — leave a `# NOTE: A2 alias section removed by rebrand Task 1` marker where that block is so Task 1's implementer finds it.)
  - `run_differential.sh`: source the env; the port driver var stays `RUSTTY_VT_DRIVER` (already rustty). Add a comment that the corpora under `phase-0` are the ORACLE golden (carve-out, stays ghostty).
  - `check_parity_surface.py` / `check_constants_parity.py`: read the two env values they need (the terminfo entry name → `PORT_NAME`, the oracle path → `ORACLE_TREE`) rather than the string literals `"rustty"` / `"/Users/gael/projects/ghostty"`. A tiny loader is fine:
    ```python
    def _parity_names(root):
        d = {}
        with open(os.path.join(root, ".port/tools/parity_names.env")) as f:
            for line in f:
                line = line.strip()
                if line and not line.startswith("#") and "=" in line:
                    k, v = line.split("=", 1); d[k] = v
        return d
    ```
    Then `infocmp` uses `names["PORT_NAME"]`, oracle path uses `names["ORACLE_TREE"]`.

- [ ] **Step 4: Write `.port/tools/PARITY_NAMING.md`** capturing the invariant, the carve-out, and how the map is used:
```markdown
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
```

- [ ] **Step 5: Verify the map is 1:1 and every parity script still runs (progress-mode is fine).**
```bash
cd /Users/gael/projects/rustty
# map sanity: every PORT_ has an ORACLE_ pair and they differ
grep -E '^(PORT|ORACLE)_' .port/tools/parity_names.env
# the touched scripts still parse/execute (progress or green, not crash)
bash -n .port/tools/gates/check_symbol_manifest.sh && echo "symbol_manifest: parses"
bash -n .port/tools/gates/run_differential.sh && echo "run_differential: parses"
python3 -c "import ast; ast.parse(open('.port/tools/gates/check_parity_surface.py').read()); print('parity_surface: parses')"
python3 -c "import ast; ast.parse(open('.port/tools/gates/check_constants_parity.py').read()); print('constants_parity: parses')"
```
Expected: pairs present and differing; all four scripts parse.

- [ ] **Step 6: Commit.**
```bash
git add .port/tools/parity_names.env .port/tools/PARITY_NAMING.md .port/tools/gates .port/tools/ci.sh
git commit -m "parity: sourced port<->oracle name map + name-aware gap scripts + invariant doc (Task 0)"
```

---

## Phase 1 — C-ABI seam: drop the alias, repackage as RusttyKit

### Task 1: Delete the `ghostty_*` C-ABI alias layer; decouple `identity-ghostty` from it

**Files:**
- Delete: `crates/rustty-apprt-c/src/ghostty_alias.rs`
- Delete: `crates/rustty-config/src/ghostty_alias.rs`
- Delete: `crates/rustty-bench/src/ghostty_alias.rs`
- Modify: `crates/rustty-apprt-c/src/lib.rs` (remove the `mod ghostty_alias;` + any force-link ref)
- Modify: `crates/rustty-config/src/lib.rs`, `crates/rustty-bench/src/lib.rs` (same)
- Modify: `crates/rustty-apprt-c/Cargo.toml`, `crates/rustty-config/Cargo.toml`, `crates/rustty-bench/Cargo.toml` (the `identity-ghostty` feature keeps flipping `rustty-identity` strings but no longer references the alias module)
- Modify: `.port/tools/gates/check_symbol_manifest.sh` (remove the A2 "alias forwarder" verification section — the forwarders are gone; keep the A1 `CABI_SYMBOLS.tsv` bridge + live-`rustty_*`-export check; find the `# NOTE: A2 alias section removed by rebrand Task 1` marker left by Task 0)

**Interfaces:**
- Consumes: the canonical `rustty_*` exports (unchanged).
- Produces: a workspace where `nm` shows **no** `_ghostty_*` symbols under the default profile.

- [ ] **Step 1: Find every reference to the alias modules.**
```bash
cd /Users/gael/projects/rustty
grep -rn 'ghostty_alias\|mod ghostty_alias' crates/*/src/lib.rs crates/*/Cargo.toml
```
Expected: `mod ghostty_alias;` (often `#[cfg(feature = "identity-ghostty")]`-gated) in the three crates + feature wiring in the Cargo.tomls.

- [ ] **Step 2: Delete the three alias source files.**
```bash
git rm crates/rustty-apprt-c/src/ghostty_alias.rs \
       crates/rustty-config/src/ghostty_alias.rs \
       crates/rustty-bench/src/ghostty_alias.rs
```

- [ ] **Step 3: Remove the `mod ghostty_alias;` declarations** from `crates/rustty-apprt-c/src/lib.rs`, `crates/rustty-config/src/lib.rs`, `crates/rustty-bench/src/lib.rs` (and any `#[used]`/force-link line that named an alias symbol). Leave the rest of each `lib.rs` untouched.

- [ ] **Step 4: Update the three `Cargo.toml` `identity-ghostty` feature entries** so the feature still enables `rustty-config/identity-ghostty` + `rustty-bench/identity-ghostty` + `rustty-identity/identity-ghostty` (the string flip) but its doc comment no longer claims to compile an alias. In `crates/rustty-apprt-c/Cargo.toml` change the feature line to also propagate the identity crate:
```toml
identity-ghostty = ["rustty-config/identity-ghostty", "rustty-bench/identity-ghostty", "rustty-identity/identity-ghostty"]
```
(Confirm `rustty-identity` is a dependency of apprt-c; if not, add it under `[dependencies]` — it is a tiny leaf.)

- [ ] **Step 5: Build both profiles and assert the alias is gone.**
```bash
cargo build -p rustty-apprt-c
cargo build -p rustty-apprt-c --features identity-ghostty
```
Expected: both succeed.

- [ ] **Step 6: Verify no `ghostty_` symbol is exported** by the default build:
```bash
cargo build -p rustty-apprt-c --release 2>/dev/null
nm -gU target/release/librustty_apprt_c.a 2>/dev/null | grep -c ' _ghostty_' || echo 0
```
Expected: `0`.

- [ ] **Step 7: Remove the A2 alias-forwarder section from the symbol-manifest gate.** In `.port/tools/gates/check_symbol_manifest.sh`, find the `# NOTE: A2 alias section removed by rebrand Task 1` marker (left by Task 0) and delete the alias-forwarder verification block it marks — the `ghostty_*` forwarders no longer exist. Keep the A1 checks: (a) every `CABI_SYMBOLS.tsv` `rustty_name` has a live `extern "C" fn` export, and (the header-count check against the external oracle header). Run it:
```bash
bash .port/tools/gates/check_symbol_manifest.sh 2>&1 | tail -8
```
Expected: passes (or progress-mode if the oracle header is absent) — NOT red on missing `ghostty_*` aliases.

- [ ] **Step 8: Commit.**
```bash
git add -A crates/rustty-apprt-c crates/rustty-config crates/rustty-bench .port/tools/gates/check_symbol_manifest.sh
git commit -m "abi: delete ghostty_* alias layer; identity-ghostty flips strings only; drop A2 gate section (Task 1)"
```

### Task 2: Produce the canonical `include/rustty-embed.h` umbrella by renaming the validated frozen `ghostty.h`

Context (why this shape): the frozen `include/ghostty.h` is NOT a cbindgen output — it is the hand-vendored, pinned-upstream **Ghostty embedding-API umbrella** (1209 lines: all 90 functions, 97 `_t/_s/_e` types, 28 config + 1 benchmark symbol), and `check_symbol_manifest.sh` already validates it 90/90 against the live `rustty_*` exports through `CABI_SYMBOLS.tsv` (columns `ghostty_name → rustty_name`). The cbindgen headers (`include/rustty.h` = vt-c; `include/rustty-apprt.h` = apprt-c) use a *bare-name* convention (`App`, not `ghostty_app_t`) and do NOT cover config/benchmark — they are the wrong artifact for the Swift app. Because `CABI_SYMBOLS.tsv` proves `ghostty_X ↔ rustty_X` is a 1:1 correspondence, a **token-rename** of the frozen umbrella (`ghostty_`→`rustty_`, `GHOSTTY_`→`RUSTTY_`) yields an ABI-faithful `rustty_*`-prefixed umbrella with the identical `_t/_s/_e` convention, covering every symbol the Swift app calls. This also makes the Swift repoint (Task 5) a genuine lockstep token-swap. The original frozen `include/ghostty.h` is **kept** as a local frozen ORACLE header (a carve-out, exactly like the phase-0 corpora) so future gap-comparison can diff the product umbrella against the oracle by the `CABI_SYMBOLS.tsv` name-map — serving the "compare gaps later" goal.

**Files:**
- Read: `include/ghostty.h` (the frozen oracle umbrella — the source of the rename)
- Read: `.port/audits/CABI_SYMBOLS.tsv` (the `ghostty_name → rustty_name` map — the correctness oracle)
- Create: `include/rustty-embed.h` (the product umbrella — `ghostty.h` token-renamed)
- Modify: `.port/tools/parity_names.env` (add `PORT_UMBRELLA=include/rustty-embed.h`, `ORACLE_UMBRELLA=include/ghostty.h`)
- Modify: `.port/tools/PARITY_NAMING.md` (add `include/ghostty.h` to the "stays ghostty" carve-out list as the local frozen oracle header)

**Interfaces:**
- Produces: `include/rustty-embed.h` — the `rustty_*`-prefixed embedding-API umbrella (staged as `rustty.h` in the xcframework by Task 3). Keeps `include/ghostty.h` as the frozen oracle.

- [ ] **Step 1: Create the product umbrella by token-renaming the frozen umbrella.** The rename is purely lexical — symbols, the include guard, and the module comment:
```bash
cd /Users/gael/projects/rustty
sed -e 's/ghostty_/rustty_/g' -e 's/GHOSTTY_/RUSTTY_/g' -e 's/Ghostty/Rustty/g' \
    include/ghostty.h > include/rustty-embed.h
```
Then eyeball the head of `include/rustty-embed.h`: the guard must read `#ifndef RUSTTY_H` / `#define RUSTTY_H`, and the leading comment should describe the Rustty embedding API. (Keep a one-line provenance note, e.g. `// Rustty embedding API — token-renamed from the Ghostty embedding-API umbrella (oracle: include/ghostty.h).`)

- [ ] **Step 2: Prove the umbrella is ABI-faithful — every `rustty_*` function in it maps 1:1 to the frozen oracle via `CABI_SYMBOLS.tsv` AND has a live export.** Three checks:
```bash
cd /Users/gael/projects/rustty
# (a) function count preserved vs oracle (both should be 90)
echo "oracle fns: $(grep -cE 'ghostty_[a-z_]+\(' include/ghostty.h)  product fns: $(grep -cE 'rustty_[a-z_]+\(' include/rustty-embed.h)"
# (b) every rustty_* function name in the product umbrella appears as a rustty_name in CABI_SYMBOLS.tsv
comm -23 \
  <(grep -oE 'rustty_[a-z_]+\(' include/rustty-embed.h | sed 's/($//;s/(//' | sort -u) \
  <(cut -f2 .port/audits/CABI_SYMBOLS.tsv | sort -u) > /tmp/unmapped.txt
echo "unmapped product fns (want empty):"; cat /tmp/unmapped.txt
# (c) every mapped rustty_* has a live extern "C" export somewhere under crates/
bash .port/tools/gates/check_symbol_manifest.sh 2>&1 | tail -6
```
Expected: (a) both 90; (b) `/tmp/unmapped.txt` empty; (c) gate GREEN (90/90). If (b) is non-empty, a symbol in the umbrella has no TSV mapping — investigate (do NOT hand-edit the umbrella to hide it).

- [ ] **Step 3: Prove the umbrella covers every symbol the Swift app calls** (the Task-5 prerequisite). The Swift sources still say `ghostty_*`; prefix-swap and confirm each is declared in the product umbrella (functions) or defined as a typedef (types):
```bash
cd /Users/gael/projects/rustty
cd /Users/gael/projects/rustty
# lowercase functions + _t/_s/_e types
grep -rhoE 'ghostty_[a-z_]+' macos/Sources macos/GhosttyUITests macos/Tests \
  | sort -u | sed 's/^ghostty_/rustty_/' > /tmp/needed_syms.txt
grep -oE 'rustty_[a-z_]+' include/rustty-embed.h | sort -u > /tmp/have_syms.txt
echo "missing lowercase syms (want empty):"; comm -23 /tmp/needed_syms.txt /tmp/have_syms.txt
# UPPERCASE enum/macro constants
grep -rhoE 'GHOSTTY_[A-Z0-9_]+' macos/Sources macos/GhosttyUITests macos/Tests \
  | sort -u | sed 's/^GHOSTTY_/RUSTTY_/' > /tmp/needed_consts.txt
grep -oE 'RUSTTY_[A-Z0-9_]+' include/rustty-embed.h | sort -u > /tmp/have_consts.txt
echo "missing enum consts (want empty):"; comm -23 /tmp/needed_consts.txt /tmp/have_consts.txt
```
Expected: **both empty** — every symbol the Swift app references (lowercase function/`_t`/`_s`/`_e` type AND UPPERCASE enum/macro constant) is present in `include/rustty-embed.h`. (The lowercase check was RED against `rustty-apprt.h`; both must be GREEN against `rustty-embed.h`. Coverage is guaranteed by construction since the umbrella is a verbatim rename of the oracle — but assert it, don't assume. If anything prints, report it precisely, do not fabricate.)

- [ ] **Step 4: Record the umbrella in the parity map + carve-out.** Add to `.port/tools/parity_names.env`:
```
PORT_UMBRELLA=include/rustty-embed.h
ORACLE_UMBRELLA=include/ghostty.h
```
and add a line to the "Stays ghostty (the oracle — never renamed)" list in `.port/tools/PARITY_NAMING.md`:
```
- `include/ghostty.h`                            — LOCAL frozen oracle umbrella (product = include/rustty-embed.h)
```

- [ ] **Step 5: Commit.**
```bash
git add include/rustty-embed.h .port/tools/parity_names.env .port/tools/PARITY_NAMING.md
git commit -m "abi: canonical rustty embedding umbrella include/rustty-embed.h (renamed from validated oracle ghostty.h); keep ghostty.h as frozen oracle (Task 2)"
```

### Task 3: Repackage the xcframework as `RusttyKit` (`xtask::xcframework`)

**Files:**
- Modify: `xtask/src/main.rs` — the `xcframework()` fn and its doc comments (lines ~30, ~193-321)

**Interfaces:**
- Consumes: `include/rustty-embed.h` (Task 2), the `rustty_*` staticlib.
- Produces: `dist/RusttyKit.xcframework` containing `libRusttyKit.a` + `Headers/rustty.h` + a `module RusttyKit` modulemap.

- [ ] **Step 1: Rewrite the staging block.** In `xcframework()` change the combined-lib path, the header staging, the modulemap, and the `-create-xcframework` output:
```rust
// combined self-contained lib
let lib = root.join(format!("target/{target}/xcframework/libRusttyKit.a"));
// ...
// 2. Stage the headers dir: the rustty embedding umbrella staged as rustty.h + the RusttyKit module map.
let src_h = root.join("include/rustty-embed.h");
std::fs::copy(&src_h, headers.join("rustty.h"))
    .map_err(|e| format!("copy rustty.h: {e}"))?;
std::fs::write(
    headers.join("module.modulemap"),
    "// RusttyKit module for the rustty macOS Swift app (canonical rustty_* ABI).\n\
     module RusttyKit {\n    umbrella header \"rustty.h\"\n    export *\n}\n",
)
.map_err(|e| format!("write modulemap: {e}"))?;
// 3. xcodebuild -create-xcframework
let out = dist.join("RusttyKit.xcframework");
```
Update the `libtool -o` / `CombineArchivesStep` target name to `libRusttyKit.a`, and update the doc comments that say `dist/GhosttyKit.xcframework` / "frozen `include/ghostty.h` + a `GhosttyKit` module map" / the stale `identity-ghostty`-emits-aliases note (xtask ~line 201, flagged in Task 1) to the Rustty equivalents. The abort-profile staticlib is still built with `--features identity-ghostty`? **No** — for the shipped product it must be the **default** profile now (the alias is gone; the Swift app links `rustty_*`). Change the staticlib build args to drop `--features identity-ghostty` (leave the `[profile.xcframework]` panic=abort profile).

- [ ] **Step 2: Update the usage string** at line ~30: `Package rustty-apprt-c into dist/RusttyKit.xcframework (macOS Swift-app tier)`.

- [ ] **Step 3: Build the xcframework.**
```bash
cargo run -p xtask -- xcframework
```
Expected: `dist/RusttyKit.xcframework/` created with `macos-arm64/libRusttyKit.a` + `Headers/rustty.h` + `Headers/module.modulemap`.

- [ ] **Step 4: Verify the lib exports `rustty_app_new` and NOT `ghostty_app_new`, and is still self-contained (zlib `deflate` present).**
```bash
nm dist/RusttyKit.xcframework/macos-arm64/libRusttyKit.a 2>/dev/null | grep -E ' T _rustty_app_new| T _deflate' | sort -u
nm dist/RusttyKit.xcframework/macos-arm64/libRusttyKit.a 2>/dev/null | grep -c ' _ghostty_app_new' || echo 0
```
Expected: both `_rustty_app_new` and `_deflate` present (T); `ghostty_app_new` count `0`.

- [ ] **Step 5: Commit.**
```bash
git add xtask/src/main.rs
git commit -m "xtask: package RusttyKit.xcframework (rustty.h + module RusttyKit, default profile) (Task 3)"
```

### Task 4: Repoint the ABI gate to the product umbrella (oracle `ghostty.h` stays as carve-out)

Revised from the original "delete ghostty.h": Task 2 established that `include/ghostty.h` is the validated frozen ORACLE umbrella and `CABI_SYMBOLS.tsv` is the name-map bridging it to the product `include/rustty-embed.h`. Keeping the oracle header locally is what lets a future gap-comparison diff product↔oracle by name WITHOUT the external checkout — the user's explicit "compare gaps later" goal. So this task does NOT delete `ghostty.h`; it repoints the ABI gate to validate the PRODUCT umbrella (`rustty-embed.h`) against the oracle (`ghostty.h`) through the name-map, and renames the gate to reflect that it now guards the rustty product.

**Files:**
- Rename: `.port/tools/gates/check_ghostty_h_abi.sh` → `.port/tools/gates/check_rustty_h_abi.sh`
- Modify: the renamed gate script (validate `include/rustty-embed.h` ↔ `include/ghostty.h` via `CABI_SYMBOLS.tsv`, sourced from `parity_names.env`)
- Modify: any CI workflow (`.github/workflows/*.yml`) or `.port/tools/ci.sh` that invoked the old gate name

**Interfaces:**
- Produces: a name-aware ABI gate proving the product umbrella stays ABI-equivalent to the frozen oracle by the token-map.

- [ ] **Step 1: See what the old gate did + who calls it.**
```bash
cd /Users/gael/projects/rustty
sed -n '1,60p' .port/tools/gates/check_ghostty_h_abi.sh
grep -rn 'check_ghostty_h_abi' .github .port/tools/ci.sh 2>/dev/null
```

- [ ] **Step 2: Rename the gate.**
```bash
git mv .port/tools/gates/check_ghostty_h_abi.sh .port/tools/gates/check_rustty_h_abi.sh
```

- [ ] **Step 3: Rewrite the gate** to: source `.port/tools/parity_names.env`; take the product umbrella `$PORT_UMBRELLA` (`include/rustty-embed.h`) and the oracle `$ORACLE_UMBRELLA` (`include/ghostty.h`); assert that applying the `CABI_SYMBOLS.tsv` `ghostty_name→rustty_name` map to the oracle's function set yields exactly the product's function set (i.e. the product umbrella is the oracle token-renamed, nothing added or dropped). It must also assert the product umbrella contains **zero** `ghostty_` function tokens. Keep it BLOCKED-tolerant if `CABI_SYMBOLS.tsv` is somehow absent (mirror the existing gates' oracle-absent handling). Update the caller name in any workflow / `ci.sh`.

- [ ] **Step 4: Run the gate.**
```bash
bash .port/tools/gates/check_rustty_h_abi.sh 2>&1 | tail -8
```
Expected: passes (exit 0) — product umbrella == oracle-renamed, zero `ghostty_` functions in the product.

- [ ] **Step 5: Commit.**
```bash
git add -A .port/tools/gates .port/tools/ci.sh .github
git commit -m "abi: repoint ABI gate to product umbrella rustty-embed.h vs oracle ghostty.h via name-map; rename -> check_rustty_h_abi.sh (Task 4)"
```

---

## Phase 2 — Swift app: symbol/import repoint, then filesystem + Xcode renames

### Task 5: Repoint every Swift `ghostty_*` symbol and `import GhosttyKit`

**Files:**
- Modify: all `.swift` under `macos/Sources`, `macos/GhosttyUITests`, `macos/Tests`
- Modify: `macos/Sources/App/macOS/ghostty-bridging-header.h`

**Interfaces:**
- Consumes: `RusttyKit` module (Task 3), `rustty_*` symbols.
- Produces: a Swift tree that references only `rustty_*` and `import RusttyKit`.

- [ ] **Step 1: Confirm the mapping is a clean 1:1 prefix swap** (no `ghostty_` symbol whose `rustty_` twin is missing — proven in Task 2 Step 3 against `include/rustty-embed.h`). The Swift app references two lexical families the renamed umbrella covers: lowercase `ghostty_*` functions/types (→ `rustty_*`) and UPPERCASE `GHOSTTY_*` enum/macro constants (→ `RUSTTY_*`, ~377 in use). Snapshot the before-count:
```bash
grep -rhcE 'ghostty_|GHOSTTY_|GhosttyKit' macos/Sources macos/GhosttyUITests macos/Tests | awk -F: '{s+=$1} END {print "refs:",s}'
```

- [ ] **Step 2: Repoint C-symbol references, enum constants, and the module import.** Apply across all Swift + the bridging header:
```bash
cd /Users/gael/projects/rustty
grep -rlZ --include='*.swift' --include='*.h' -e 'ghostty_' -e 'GHOSTTY_' -e 'import GhosttyKit' -e '#import "ghostty' macos/Sources macos/GhosttyUITests macos/Tests \
  | xargs -0 sed -i '' \
      -e 's/\bghostty_/rustty_/g' \
      -e 's/\bGHOSTTY_/RUSTTY_/g' \
      -e 's/import GhosttyKit/import RusttyKit/g' \
      -e 's/GhosttyKit\./RusttyKit./g'
```
(The `\b` keeps the C-symbol and enum-constant families targeted; the Swift `Ghostty` *namespace enum* — CamelCase whole-word `Ghostty` — is deliberately NOT touched here, it is Task 6.)

- [ ] **Step 3: Fix the bridging header include** if it pulls the umbrella by path: in `macos/Sources/App/macOS/ghostty-bridging-header.h` change any `#import "ghostty.h"` / `<GhosttyKit/ghostty.h>` to the `RusttyKit`/`rustty.h` form. (The file itself is renamed to `rustty-bridging-header.h` in Task 7.)

- [ ] **Step 4: Verify no `ghostty_`/`GHOSTTY_` C reference or `GhosttyKit` import remains** (the CamelCase `Ghostty` *namespace* enum still exists — expected until Task 6):
```bash
grep -rnE 'ghostty_|GHOSTTY_|GhosttyKit' macos/Sources macos/GhosttyUITests macos/Tests
```
Expected: empty.

- [ ] **Step 5: Commit.**
```bash
git add -A macos
git commit -m "swift: repoint ghostty_* -> rustty_*, import RusttyKit (Task 5)"
```

### Task 6: Rename the Swift `Ghostty` namespace enum → `Rustty` (symbol-level, not files yet)

**Files:**
- Modify: all `.swift` under `macos/Sources`, `macos/GhosttyUITests`, `macos/Tests`

**Interfaces:**
- Produces: Swift source where the namespace type is `Rustty` (e.g. `Rustty.App`, `extension Rustty`, `enum Rustty`).

- [ ] **Step 1: Snapshot the identifiers that will change** so the rename is auditable:
```bash
grep -rhoE '\bGhostty\b(\.[A-Za-z]+)?' macos/Sources | sort | uniq -c | sort -rn | head -30
```
Expected: `enum Ghostty`, `extension Ghostty`, `Ghostty.App`, `Ghostty.Config`, `Ghostty.Surface`, `GhosttyApp`, etc. Note whole-word `Ghostty` and the CamelCase compounds (`GhosttyApp`, `GhosttyDelegate`).

- [ ] **Step 2: Rename the namespace and its CamelCase compounds.** Do the compound forms first (so they don't get partially rewritten), then whole-word `Ghostty`:
```bash
cd /Users/gael/projects/rustty
grep -rlZ --include='*.swift' 'Ghostty' macos/Sources macos/GhosttyUITests macos/Tests \
  | xargs -0 sed -i '' \
      -e 's/GhosttyApp/RusttyApp/g' \
      -e 's/GhosttyDelegate/RusttyDelegate/g' \
      -e 's/GhosttyPackage/RusttyPackage/g' \
      -e 's/GhosttyIntentError/RusttyIntentError/g' \
      -e 's/ColorizedGhosttyIcon/ColorizedRusttyIcon/g' \
      -e 's/\bGhostty\b/Rustty/g'
```

- [ ] **Step 3: Build the Swift package to prove symbol consistency.** (Requires the Task 3 xcframework staged — do that first if not already:)
```bash
rm -rf macos/RusttyKit.xcframework && cp -R dist/RusttyKit.xcframework macos/RusttyKit.xcframework
```
Then attempt a compile of the module sources only (full app build happens in Task 9). Expected: no "cannot find 'Ghostty' in scope" errors; residual check:
```bash
grep -rnw 'Ghostty' macos/Sources macos/GhosttyUITests macos/Tests
```
Expected: empty (all whole-word `Ghostty` gone; filenames still say Ghostty — Task 7/8).

- [ ] **Step 4: Commit.**
```bash
git add -A macos
git commit -m "swift: rename Ghostty namespace enum + compounds -> Rustty (Task 6)"
```

### Task 6b: Rename the lowercase Swift `ghostty` identifier + config-path family → `rustty` (keep upstream URLs)

Context: discovered during Task 6 review — beyond the `ghostty_*` C symbols (Task 5) and the CamelCase `Ghostty` namespace (Task 6), the Swift app has a THIRD family: ~460 lowercase `ghostty` Swift identifiers, config paths, and user-facing strings (`ghostty` var, `ghosttyApplication()`, `ghosttySuite`, `ghosttyConfigDidChange`, `UserDefaults.ghostty`, `~/.config/ghostty/`, `config.ghostty`, `.appendingPathExtension("ghostty")`, `"Initialize ghostty backend failed"`, persisted keys `CustomGhosttyIcon`/`GhosttyCommit`). `rustty-identity` already commits (default profile) to `CONFIG_DIR="rustty"` / `CONFIG_FILE="config.rustty"`, so the Swift hardcodes are drift to be aligned, not a new decision. EXCEPTION — 25 upstream `github.com/ghostty-org/ghostty` URLs in comments are external references and MUST STAY.

**Files:**
- Modify: all `.swift` under `macos/Sources`, `macos/GhosttyUITests`, `macos/Tests`

**Interfaces:**
- Produces: Swift source where the lowercase identifier/config-path family reads `rustty` (config dir `~/.config/rustty/`, file `config.rustty`, ext `rustty`), upstream URLs preserved.

- [ ] **Step 1: Snapshot the families + isolate what must stay.**
```bash
cd /Users/gael/projects/rustty
echo "total lowercase ghostty refs: $(git grep -c 'ghostty' -- 'macos/**/*.swift' | awk -F: '{s+=$2} END{print s}')"
echo "upstream URL lines (MUST STAY): $(git grep -c 'github.com/ghostty' -- 'macos/**/*.swift' | awk -F: '{s+=$2} END{print s}')"
echo "bundle-id-like refs (flag for review, do NOT blind-rename): "; git grep -in 'com\.mitchellh\.ghostty\|mitchellh' -- 'macos/**/*.swift'
```

- [ ] **Step 2: Rename lowercase `ghostty`→`rustty` in `.swift`, protecting upstream-URL lines.** Use perl (BSD sed lacks lookarounds; the harness grep drops `-Z` — collect files via `git grep -l`). Skip any line containing `github.com/ghostty` (upstream URL) and any `com.mitchellh.ghostty` bundle-id token:
```bash
cd /Users/gael/projects/rustty
for f in $(git grep -l 'ghostty' -- 'macos/**/*.swift'); do
  perl -i -pe 's/ghostty/rustty/g unless /github\.com\/ghostty/ || /com\.mitchellh\.ghostty/' "$f"
done
```
Then handle the config paths/extension explicitly (they are lowercase `ghostty` so already covered, but VERIFY): `~/.config/ghostty/`→`~/.config/rustty/`, `config.ghostty`→`config.rustty`, `.appendingPathExtension("ghostty")`→`("rustty")`.

- [ ] **Step 3: Verify — lowercase `ghostty` remaining are ONLY upstream URLs (and any flagged bundle-id).**
```bash
cd /Users/gael/projects/rustty
git grep -in 'ghostty' -- 'macos/**/*.swift' | grep -viE 'github.com/ghostty|com\.mitchellh\.ghostty'
```
Expected: **empty** (every remaining lowercase `ghostty` is an upstream URL or a deliberately-flagged bundle id). If a bundle-id token `com.mitchellh.ghostty` remains, report it in the task report for the human to decide (rebrand to `io.airely.rustty` vs keep) — do NOT silently rename a bundle identifier.

- [ ] **Step 4: Confirm the config paths are now rustty-consistent.**
```bash
grep -rn 'config/rustty\|config.rustty\|"rustty"' macos/Sources macos/Tests macos/GhosttyUITests | grep -iE 'config|extension|icns' | head
grep -rn 'config/ghostty\|config.ghostty' macos/Sources macos/Tests macos/GhosttyUITests
```
Expected: first shows the rustty config paths; second is empty.

- [ ] **Step 5: Commit.**
```bash
git add -A macos
git commit -m "swift: rename lowercase ghostty identifiers + config paths -> rustty (config.rustty, ~/.config/rustty); keep upstream URLs (Task 6b)"
```

### Task 6c: Rename the app's reverse-DNS namespace `com.mitchellh.ghostty` → `io.airely.rustty`

Context: discovered in Task 6b — the app uses ~48 `com.mitchellh.ghostty.*` reverse-DNS strings as internal identifiers: mostly `Notification.Name` (the in-process event bus: `.configDidChange`, `.newTab`, `.quickTerminal`, …), one drag/drop UTI (`com.mitchellh.ghosttySurfaceId`, declared in both Swift and Info.plist), and Pasteboard types. Task 6b deliberately preserved them (its brief protected the bundle-id namespace). The product bundle id is already `io.airely.rustty`, and verification confirmed these strings have **no** entitlement / app-group / keychain coupling (only one Info.plist UTI). They are purely-internal app namespaces — some already half-branded (`com.mitchellh.ghostty.rusttySelectionDidChange`). For the full rebrand they become `io.airely.rustty.*`. This is safe (fresh product, no external interop that should masquerade as ghostty).

**Files:**
- Modify: all `.swift` under `macos/Sources`, `macos/Tests`, `macos/GhosttyUITests` (the `com.mitchellh.ghostty*` Notification.Name / UTI / Pasteboard strings)
- Modify: `macos/Ghostty-Info.plist` (the `com.mitchellh.ghosttySurfaceId` UTI value at line ~117 — renamed WITH its Swift counterpart so the exported UTI stays matched; the file itself is renamed to Rustty-Info.plist in Task 8, this only touches the UTI string)

**Interfaces:**
- Produces: the app's reverse-DNS namespace is `io.airely.rustty.*`, matching the bundle id, consistent across Swift + Info.plist.

- [ ] **Step 1: Confirm the scope + no entitlement coupling** (guard against renaming a provisioning-coupled identifier):
```bash
cd /Users/gael/projects/rustty
echo "reverse-DNS refs in swift: $(git grep -c 'com.mitchellh.ghostty' -- 'macos/**/*.swift' | awk -F: '{s+=$2} END{print s}')"
echo "entitlement/app-group/keychain coupling (want empty):"; git grep -n 'com.mitchellh.ghostty' -- 'macos/*.entitlements'
echo "Info.plist UTI:"; git grep -n 'com.mitchellh.ghostty' -- 'macos/Ghostty-Info.plist'
```
Expected: entitlements empty; the Info.plist has the one `ghosttySurfaceId` UTI.

- [ ] **Step 2: Rename the reverse-DNS namespace across Swift + Info.plist.**
```bash
cd /Users/gael/projects/rustty
for f in $(git grep -l 'com.mitchellh.ghostty' -- 'macos/**/*.swift' 'macos/Ghostty-Info.plist'); do
  sed -i '' 's/com\.mitchellh\.ghostty/io.airely.rustty/g' "$f"
done
```
(This turns `com.mitchellh.ghostty.newTab`→`io.airely.rustty.newTab` and `com.mitchellh.ghosttySurfaceId`→`io.airely.rusttySurfaceId`, matching the bundle id `io.airely.rustty`.)

- [ ] **Step 3: Verify the Swift UTI and the Info.plist UTI still MATCH** (a drag/drop UTI must be byte-identical on both sides) and nothing `com.mitchellh.ghostty` remains:
```bash
cd /Users/gael/projects/rustty
echo "swift UTI:"; git grep -hoE 'io\.airely\.rustty[A-Za-z]*SurfaceId' -- 'macos/**/*.swift' | sort -u
echo "plist UTI:"; git grep -hoE 'io\.airely\.rustty[A-Za-z]*SurfaceId' -- 'macos/Ghostty-Info.plist' | sort -u
git grep -n 'com.mitchellh.ghostty' -- macos
```
Expected: the two UTI values are identical; final grep empty. (Genuine `Mitchell Hashimoto`/`NOTE(mitchellh)` author credits in comments are NOT reverse-DNS strings and stay.)

- [ ] **Step 4: Commit.**
```bash
git add -A macos
git commit -m "swift: rebrand app reverse-DNS namespace com.mitchellh.ghostty -> io.airely.rustty (Notification.Name/UTI/Pasteboard) (Task 6c)"
```

### Task 7: Rename the Swift source files and directories

**Files (git mv — exact inventory):**
- `macos/Sources/Ghostty/` → `macos/Sources/RusttyKit/`
- `macos/Sources/Ghostty/Ghostty.*.swift` → `macos/Sources/RusttyKit/Rustty.*.swift` (Action, App, ChildExitedMessage, Command, Config, ConfigTypes, Error, Event, Input, Inspector, MenuShortcutManager, Shell, Surface)
- `macos/Sources/Ghostty/GhosttyDelegate.swift` → `.../RusttyKit/RusttyDelegate.swift`; `GhosttyPackage.swift` → `RusttyPackage.swift`; `GhosttyPackageMeta.swift` → `RusttyPackageMeta.swift`
- `macos/Tests/Ghostty/` → `macos/Tests/Rustty/`
- `macos/GhosttyUITests/` → `macos/RusttyUITests/` and each `Ghostty*Tests.swift` → `Rustty*Tests.swift`
- `macos/Sources/App/macOS/AppDelegate+Ghostty.swift` → `AppDelegate+Rustty.swift`
- `macos/Sources/App/macOS/ghostty-bridging-header.h` → `rustty-bridging-header.h`
- `macos/Sources/Features/App Intents/GhosttyIntentError.swift` → `RusttyIntentError.swift`
- `macos/Sources/Features/AppleScript/Ghostty.Input.Mods+AppleScript.swift` → `Rustty.Input.Mods+AppleScript.swift`
- `macos/Sources/Features/Custom App Icon/ColorizedGhosttyIcon.swift` (+`Image`,`View`) → `ColorizedRusttyIcon*.swift`
- `macos/Tests/ColorizedGhosttyIconTests.swift` → `ColorizedRusttyIconTests.swift`

Also fixes an ObjC coupling introduced by Task 6: the `.m`/`.h` ObjC helper is NOT `.swift`, so Task 6 renamed the Swift CALLER `RusttyAddTabbedWindowSafely` (NSWindow+Extension.swift) but left the ObjC definition `GhosttyAddTabbedWindowSafely` — a dangling symbol until fixed here.

**Additional files (ObjC content — no file rename needed, names are Ghostty-neutral):**
- Modify: `macos/Sources/Helpers/ObjCExceptionCatcher.h` + `.m` (`GhosttyAddTabbedWindowSafely` → `RusttyAddTabbedWindowSafely`; error domain `@"Ghostty.ObjCException"` → `@"Rustty.ObjCException"`)

**Interfaces:**
- Produces: filesystem names matching the Task 6 symbol names + the ObjC symbol matching its now-renamed Swift caller. (The `.pbxproj` file references are fixed in Task 8.)

- [ ] **Step 0: Fix the ObjC `.m`/`.h` symbol coupling** (Task 6 renamed the Swift caller; align the definition):
```bash
cd /Users/gael/projects/rustty
sed -i '' -e 's/GhosttyAddTabbedWindowSafely/RusttyAddTabbedWindowSafely/g' -e 's/@"Ghostty\.ObjCException"/@"Rustty.ObjCException"/g' \
  macos/Sources/Helpers/ObjCExceptionCatcher.h macos/Sources/Helpers/ObjCExceptionCatcher.m
grep -rn 'Ghostty' macos/Sources/Helpers/ObjCExceptionCatcher.h macos/Sources/Helpers/ObjCExceptionCatcher.m
```
Expected: the grep is empty (both the function symbol and error domain now Rustty, matching the Swift caller `RusttyAddTabbedWindowSafely`).

- [ ] **Step 1: Rename directories.**
```bash
cd /Users/gael/projects/rustty
git mv "macos/Sources/Ghostty" "macos/Sources/RusttyKit"
git mv "macos/Tests/Ghostty" "macos/Tests/Rustty"
git mv "macos/GhosttyUITests" "macos/RusttyUITests"
```

- [ ] **Step 2: Rename the files.** Rename every remaining tracked file whose basename contains `Ghostty`/`ghostty`, `Ghostty`→`Rustty` and `ghostty`→`rustty`:
```bash
for f in $(git ls-files macos | grep -iE '/[^/]*ghostty[^/]*$'); do
  d=$(dirname "$f"); b=$(basename "$f")
  nb=$(echo "$b" | sed -e 's/Ghostty/Rustty/g' -e 's/ghostty/rustty/g')
  git mv "$f" "$d/$nb"
done
```

- [ ] **Step 3: Verify no Ghostty-named file remains under `macos/Sources` / `Tests` / `RusttyUITests`.**
```bash
git ls-files macos/Sources macos/Tests macos/RusttyUITests | grep -i ghostty
```
Expected: empty.

- [ ] **Step 4: Commit.**
```bash
git add -A macos
git commit -m "swift: rename source files/dirs Ghostty -> Rustty (Task 7)"
```

### Task 8: Rename the Xcode project, targets, scheme, test plan, Info.plist, entitlements, sdef

**Files (git mv + content edits):**
- `macos/Ghostty.xcodeproj` → `macos/Rustty.xcodeproj` (dir), incl. `xcshareddata/xcschemes/Ghostty.xcscheme` → `Rustty.xcscheme`
- `macos/Ghostty-Info.plist` → `macos/Rustty-Info.plist`
- `macos/Ghostty.entitlements` → `macos/Rustty.entitlements`; `GhosttyDebug.entitlements` → `RusttyDebug.entitlements`; `GhosttyReleaseLocal.entitlements` → `RusttyReleaseLocal.entitlements`
- `macos/Ghostty.sdef` → `macos/Rustty.sdef`
- `macos/Ghostty.xctestplan` → `macos/Rustty.xctestplan`
- Modify: `macos/Rustty.xcodeproj/project.pbxproj` (all path/target/product/module references)
- Modify (content, no file rename): the 10 `.xib` files under `macos/Sources` (`customModule="Ghostty"` → `"Rustty"`, and the `GhosttyFullScreen`/`Ghostty.*` custom-class refs → `Rustty*`)
- Modify: `macos/Ghostty.sdef` → `macos/Rustty.sdef` CONTENT (the `cocoa class="GhosttyScript*"` names → `RusttyScript*`) AND the coupled Swift `@objc(GhosttyScript*)` overrides in `macos/Sources/Features/AppleScript/*.swift` (rename together — these were deliberately left by Task 6 to keep Swift↔sdef matching)
- Modify: `macos/Rustty-Info.plist` keys `GhosttyBuild`/`GhosttyCommit` → `RusttyBuild`/`RusttyCommit`, the `Ghostty.sdef` value → `Rustty.sdef`, the `Ghostty Surface Identifier` string → `Rustty Surface Identifier` (+ the Swift readers of `GhosttyBuild`/`GhosttyCommit`, if any survived Task 6b)
- Modify: `macos/build.nu`, `macos/AGENTS.md` (Ghostty → Rustty product/build references)

**Interfaces:**
- Produces: an Xcode project that builds a `Rustty.app` product with module name `RusttyKit`, referencing the renamed files, with `.xib` customModule + AppleScript cocoa classes + Info.plist keys all consistent.

- [ ] **Step 1: Rename the `.xcodeproj` directory.** NOTE: Task 7's file-rename loop already renamed the top-level config files (`Rustty-Info.plist`, `Rustty.entitlements`, `RusttyDebug.entitlements`, `RusttyReleaseLocal.entitlements`, `Rustty.sdef`, `Rustty.xctestplan`) and the scheme (`Rustty.xcscheme`, still inside the `Ghostty.xcodeproj` dir). Only the `.xcodeproj` directory itself remains:
```bash
cd /Users/gael/projects/rustty/macos
git mv Ghostty.xcodeproj Rustty.xcodeproj
ls -d Rustty.xcodeproj && ls Rustty.xcodeproj/xcshareddata/xcschemes/  # should show Rustty.xcscheme
```
(The pbxproj CONTENT still references the old paths/names — that is fixed in Step 2.)

- [ ] **Step 2: Rewrite `project.pbxproj` references.** The pbxproj references old paths (`Sources/Ghostty`, `GhosttyUITests`, `Ghostty-Info.plist`, `Ghostty.entitlements`, the `Ghostty` target/product/scheme, the `GhosttyKit` framework, `PRODUCT_MODULE_NAME`, `SWIFT_OBJC_BRIDGING_HEADER`, `INFOPLIST_FILE`, `CODE_SIGN_ENTITLEMENTS`). Apply the path + name rewrites:
```bash
cd /Users/gael/projects/rustty/macos
sed -i '' \
  -e 's#Sources/Ghostty#Sources/RusttyKit#g' \
  -e 's/GhosttyUITests/RusttyUITests/g' \
  -e 's/Tests\/Ghostty/Tests\/Rustty/g' \
  -e 's/Ghostty-Info\.plist/Rustty-Info.plist/g' \
  -e 's/GhosttyDebug\.entitlements/RusttyDebug.entitlements/g' \
  -e 's/GhosttyReleaseLocal\.entitlements/RusttyReleaseLocal.entitlements/g' \
  -e 's/Ghostty\.entitlements/Rustty.entitlements/g' \
  -e 's/Ghostty\.sdef/Rustty.sdef/g' \
  -e 's/Ghostty\.xctestplan/Rustty.xctestplan/g' \
  -e 's/ghostty-bridging-header\.h/rustty-bridging-header.h/g' \
  -e 's/GhosttyKit/RusttyKit/g' \
  -e 's/ColorizedGhosttyIcon/ColorizedRusttyIcon/g' \
  -e 's/AppDelegate+Ghostty/AppDelegate+Rustty/g' \
  -e 's/GhosttyIntentError/RusttyIntentError/g' \
  -e 's/Ghostty\.Input\.Mods/Rustty.Input.Mods/g' \
  -e 's/\bGhostty\.app\b/Rustty.app/g' \
  -e 's/"Ghostty"/"Rustty"/g' \
  -e 's/= Ghostty;/= Rustty;/g' \
  -e 's/Ghostty\.xcscheme/Rustty.xcscheme/g' \
  -e 's/EXECUTABLE_NAME = ghostty;/EXECUTABLE_NAME = rustty;/g' \
  -e 's/ASSETCATALOG_COMPILER_APPICON_NAME = Ghostty;/ASSETCATALOG_COMPILER_APPICON_NAME = Rustty;/g' \
  -e 's/com\.mitchellh\.ghostty-dock-tile/io.airely.rustty-dock-tile/g' \
  -e 's/com\.mitchellh\.ghostty-ios/io.airely.rustty-ios/g' \
  -e 's/com\.mitchellh\.GhosttyTests/io.airely.RusttyTests/g' \
  -e 's/com\.mitchellh\.GhosttyUITests/io.airely.RusttyUITests/g' \
  Rustty.xcodeproj/project.pbxproj
```
Notes: `PRODUCT_NAME = "$(TARGET_NAME)"` (unchanged) means renaming the `Ghostty` target → `Rustty` (via `= Ghostty;`→`= Rustty;`) makes the built product **`Rustty.app`** automatically; `EXECUTABLE_NAME = rustty` makes the executable `rustty` (the rationale for keeping it `ghostty` — the Branch-A `ghostty_*` link — is gone, we're on `rustty_*`/RusttyKit). `ASSETCATALOG_COMPILER_APPICON_NAME = Rustty` couples to the icon asset rename in Task 9. The main app bundle id is already `io.airely.rustty[.debug]` (prior R2); the secondary component bundle ids are now rebranded too.

Then hand-audit remaining `Ghostty` tokens in the pbxproj (individual `Ghostty*.swift` file entries the loop in Task 7 renamed on disk):
```bash
grep -oE '[A-Za-z.+ -]*Ghostty[A-Za-z.+ -]*' Rustty.xcodeproj/project.pbxproj | sort -u
```
For each remaining Swift file entry, replace `Ghostty`→`Rustty` in its `path`/`name`/`fileRef` comment to match the on-disk name from Task 7.

- [ ] **Step 3: Update the scheme + test plan contents** (`Rustty.xcscheme`, `Rustty.xctestplan`) — replace `Ghostty` blueprint/target names and the `*.app` product name with `Rustty`:
```bash
sed -i '' -e 's/GhosttyUITests/RusttyUITests/g' -e 's/\bGhostty\b/Rustty/g' \
  Rustty.xcodeproj/xcshareddata/xcschemes/Rustty.xcscheme Rustty.xctestplan
```

- [ ] **Step 4: Rename the `.xib` module + custom-class references** (customModule couples to the now-`Rustty` module/target):
```bash
cd /Users/gael/projects/rustty
for f in $(git grep -l 'Ghostty' -- 'macos/**/*.xib'); do
  sed -i '' -e 's/customModule="Ghostty"/customModule="Rustty"/g' -e 's/customClass="Ghostty/customClass="Rustty/g' -e 's/\bGhostty/Rustty/g' "$f"
done
git grep -n 'Ghostty' -- 'macos/**/*.xib'
```
Expected: empty. (Every `.xib` customModule/customClass now Rustty.)

- [ ] **Step 5: Rename the AppleScript cocoa classes in the `.sdef` + the coupled Swift `@objc` overrides together.** These were left by Task 6 precisely because the `@objc(GhosttyScript…)` runtime name must byte-match the `.sdef` `cocoa class`:
```bash
cd /Users/gael/projects/rustty
sed -i '' 's/GhosttyScript/RusttyScript/g' macos/Rustty.sdef
for f in $(git grep -l 'GhosttyScript' -- 'macos/Sources/Features/AppleScript'); do
  sed -i '' 's/GhosttyScript/RusttyScript/g' "$f"
done
echo "sdef cocoa classes vs swift @objc — must be identical sets:"
grep -oE 'RusttyScript[A-Za-z]+' macos/Rustty.sdef | sort -u > /tmp/sdef.txt
git grep -hoE '@objc\(RusttyScript[A-Za-z]+\)' -- 'macos/Sources/Features/AppleScript' | sed -E 's/@objc\((.*)\)/\1/' | sort -u > /tmp/objc.txt
diff /tmp/sdef.txt /tmp/objc.txt && echo "MATCH"
git grep -n 'GhosttyScript' -- macos
```
Expected: `MATCH` (the `.sdef` classes and Swift `@objc` names are the identical renamed set); final grep empty.

- [ ] **Step 6: Rename the Info.plist keys + the sdef reference + surface-identifier string.**
```bash
cd /Users/gael/projects/rustty
sed -i '' -e 's/GhosttyBuild/RusttyBuild/g' -e 's/GhosttyCommit/RusttyCommit/g' -e 's/Ghostty\.sdef/Rustty.sdef/g' -e 's/Ghostty Surface Identifier/Rustty Surface Identifier/g' macos/Rustty-Info.plist
# any Swift reader of the renamed plist keys (should be caught by 6b, but confirm the pair matches)
git grep -n 'GhosttyBuild\|GhosttyCommit' -- macos
```
Expected: empty (plist keys renamed; Swift readers already `RusttyBuild`/`RusttyCommit` from Task 6b — if a Swift reader still says `GhosttyCommit`, rename it to match).

- [ ] **Step 7: Set the product/display identity in `Rustty-Info.plist`.** Confirm `CFBundleName`=`Rustty` (already set), and set `CFBundleExecutable`/`CFBundleIdentifier` build-var-driven values to Rustty where literal. Verify:
```bash
plutil -p macos/Rustty-Info.plist | grep -iE 'bundlename|executable|identifier'
```
Expected: no literal `Ghostty`.

- [ ] **Step 8: Update `build.nu` + `AGENTS.md`, and replace the now-superseded `REBRAND.md`.**
```bash
cd /Users/gael/projects/rustty
sed -i '' -e 's/GhosttyKit/RusttyKit/g' -e 's/Ghostty\.xcodeproj/Rustty.xcodeproj/g' -e 's/Ghostty\.app/Rustty.app/g' -e 's/\bGhostty\b/Rustty/g' macos/build.nu macos/AGENTS.md
git grep -n 'Ghostty' -- macos/build.nu macos/AGENTS.md
```
Expected: empty (or only genuine upstream references — review any hit). Then **rewrite `macos/REBRAND.md`**: it documented the PRIOR *partial* R2 rebrand (which kept `Ghostty.app`/`ghostty` exe, the `GhosttyKit` link, secondary bundle ids, appicon name — all justified solely by preserving the `ghostty_*` link that no longer exists). Replace its body with a short note that the full rebrand (this plan) supersedes it: the app is now `Rustty.app` / `rustty` executable, module `RusttyKit`, symbols `rustty_*`, all bundle ids `io.airely.rustty*`, appicon `Rustty` — i.e. there is no longer a "deliberately left as Ghostty" list on the macOS side (the oracle carve-out lives in `.port/tools/PARITY_NAMING.md`). Keep it factual and short; do not delete the file (it is a referenced landmark).

- [ ] **Step 9: Resolve + list the scheme to prove the project parses AND the product is `Rustty.app`.**
```bash
cd /Users/gael/projects/rustty/macos
xcodebuild -project Rustty.xcodeproj -list
xcodebuild -project Rustty.xcodeproj -target Rustty -showBuildSettings 2>/dev/null | grep -E 'FULL_PRODUCT_NAME|EXECUTABLE_NAME|PRODUCT_BUNDLE_IDENTIFIER|ASSETCATALOG_COMPILER_APPICON_NAME' | head
```
Expected: lists targets `Rustty`, `RusttyUITests`, `RusttyTests` and scheme `Rustty` with no parse error; build settings show `FULL_PRODUCT_NAME = Rustty.app`, `EXECUTABLE_NAME = rustty`, `PRODUCT_BUNDLE_IDENTIFIER = io.airely.rustty*`, `ASSETCATALOG_COMPILER_APPICON_NAME = Rustty`.

- [ ] **Step 10: Commit.**
```bash
git add -A macos
git commit -m "xcode: full product rebrand — Rustty.app/rustty exe, RusttyKit, all bundle ids io.airely.rustty*, appicon Rustty, xib/sdef/plist/scheme; supersede REBRAND.md (Task 8)"
```

### Task 9: Rename the app icon asset and wire the reproducible build to RusttyKit; full app build

**Files:**
- `images/Ghostty.icon` → `images/Rustty.icon` (dir), incl. `Assets/Ghostty.png` → `Assets/Rustty.png`, `icon.json`
- Modify: `macos/build-rustty.sh` (RusttyKit.xcframework, project name)
- Modify: `macos/MILESTONE.md`, `macos/REBRAND.md` (open-command paths, product name)
- Modify: any `icon.json`/asset-catalog reference to the icon image name

**Interfaces:**
- Produces: a building, launchable `Rustty.app`.

- [ ] **Step 1: Rename the icon asset + its pbxproj reference.** The app icon is the Icon Composer `images/Ghostty.icon` bundle, referenced in the pbxproj as `Ghostty.icon` (`path = ../images/Ghostty.icon`), resolved by `ASSETCATALOG_COMPILER_APPICON_NAME = Rustty` (set in Task 8). Rename the dir, the inner asset, and the pbxproj reference together so the icon resolves:
```bash
cd /Users/gael/projects/rustty
git mv images/Ghostty.icon images/Rustty.icon
git mv images/Rustty.icon/Assets/Ghostty.png images/Rustty.icon/Assets/Rustty.png
sed -i '' 's/Ghostty/Rustty/g' images/Rustty.icon/icon.json
grep -rln 'Ghostty' macos/Assets.xcassets 2>/dev/null | xargs -r sed -i '' 's/Ghostty/Rustty/g'
# pbxproj icon reference (project is now Rustty.xcodeproj after Task 8):
sed -i '' -e 's/Ghostty\.icon/Rustty.icon/g' -e 's#\.\./images/Ghostty\.icon#../images/Rustty.icon#g' macos/Rustty.xcodeproj/project.pbxproj
grep -n 'Ghostty.icon\|images/Ghostty' macos/Rustty.xcodeproj/project.pbxproj
```
Expected: the final grep is empty (all icon references now `Rustty.icon` / `../images/Rustty.icon`, matching `ASSETCATALOG_COMPILER_APPICON_NAME = Rustty`).

- [ ] **Step 2: Rewrite `macos/build-rustty.sh`** — `GhosttyKit.xcframework`→`RusttyKit.xcframework`, `libGhosttyKit.a`→`libRusttyKit.a`, `-project Ghostty.xcodeproj`→`-project Rustty.xcodeproj`, `-scheme Ghostty`→`-scheme Rustty`, and the `cp -R dist/RusttyKit.xcframework macos/RusttyKit.xcframework` staging line. Update the header comments that name `Ghostty.app`.

- [ ] **Step 3: Update the two docs** (`MILESTONE.md`, `REBRAND.md`) — the `open …/Ghostty.app` command becomes `…/Rustty.app`, product name Rustty.

- [ ] **Step 4: Full reproducible build.**
```bash
cd /Users/gael/projects/rustty
bash macos/build-rustty.sh
```
Expected: xcframework packaged, `xcodebuild` succeeds, a `Rustty.app` is produced under DerivedData.

- [ ] **Step 5: Launch + screenshot to confirm it renders with the shell prompt** (per the app-functionality milestone — do NOT claim success from the process tree; screenshot it):
```bash
open ~/Library/Developer/Xcode/DerivedData/Rustty-*/Build/Products/Debug/Rustty.app
```
Then capture the window (computer-use screenshot). Expected: a titled "Rustty" window with a live shell prompt + cursor.

- [ ] **Step 6: Commit.**
```bash
git add -A images macos
git commit -m "app: rename icon asset + wire build-rustty.sh to RusttyKit; Rustty.app builds & renders (Task 9)"
```

---

## Phase 3 — Runtime identity (Class D): route through `rustty-identity`

### Task 10: Add the env-var-prefix identity const

**Files:**
- Modify: `crates/rustty-identity/src/lib.rs` (both `mod values` blocks + the public re-export section)

**Interfaces:**
- Produces: `rustty_identity::ENV_PREFIX` — `"RUSTTY"` by default, `"GHOSTTY"` under `identity-ghostty`.

- [ ] **Step 1: Add the const to the default (`#[cfg(not(feature = "identity-ghostty"))]`) `mod values`:**
```rust
/// Uppercase prefix for product environment variables (`{ENV_PREFIX}_RESOURCES_DIR`, …).
pub const ENV_PREFIX: &str = "RUSTTY";
```
and to the `#[cfg(feature = "identity-ghostty")]` block:
```rust
pub const ENV_PREFIX: &str = "GHOSTTY";
```

- [ ] **Step 2: Add the public re-export** in the stable-API section:
```rust
/// Uppercase prefix for product environment variables.
pub const ENV_PREFIX: &str = values::ENV_PREFIX;
```

- [ ] **Step 3: Extend the identity tests** (`crates/rustty-identity/src/lib.rs` test mod) to assert `ENV_PREFIX == "RUSTTY"` (default) and, under the ghostty profile test, `"GHOSTTY"`. Run:
```bash
cargo test -p rustty-identity
cargo test -p rustty-identity --features identity-ghostty
```
Expected: both green.

- [ ] **Step 4: Commit.**
```bash
git add crates/rustty-identity/src/lib.rs
git commit -m "identity: add ENV_PREFIX const (RUSTTY / GHOSTTY under harness) (Task 10)"
```

### Task 11: Route the default `TERM` through `rustty-identity`

**Files:**
- Modify: `crates/rustty-config/src/config_struct.rs:5077` and `:6977` (the `term: b"xterm-ghostty".to_vec()` sites)

**Interfaces:**
- Consumes: `rustty_identity::TERM` (`"xterm-rustty"` default).
- Produces: config default TERM = `xterm-rustty` (default) / `xterm-ghostty` (harness).

- [ ] **Step 1: Add the dependency** if absent — confirm `rustty-config/Cargo.toml` has `rustty-identity` under `[dependencies]`; add it if not.

- [ ] **Step 2: Replace both hardcoded defaults.** At line ~5077:
```rust
term: rustty_identity::TERM.as_bytes().to_vec(),
```
and at line ~6977 (the reset/normalize site):
```rust
self.term = rustty_identity::TERM.as_bytes().to_vec();
```
Keep the verbatim Zig comment lines above them intact.

- [ ] **Step 3: Confirm the ssh TERM-downgrade fallback is untouched.**
```bash
grep -n 'xterm-256color\|xterm-ghostty\|term' crates/rustty-cli/src/ssh.rs | head
```
Expected: the ssh fallback logic remains (it converts whatever TERM to `xterm-256color` on hosts lacking the entry); it must not hardcode `xterm-ghostty` as a magic gate — if it does, route that comparison through `rustty_identity::TERM` too.

- [ ] **Step 4: Build + a focused test.**
```bash
cargo build -p rustty-config
cargo test -p rustty-config -- term 2>/dev/null | tail -5
```
Expected: builds; any TERM-default test now expects `xterm-rustty`. Update the expected value in that test if present.

- [ ] **Step 5: Commit.**
```bash
git add crates/rustty-config
git commit -m "config: default TERM via rustty_identity::TERM (xterm-rustty) (Task 11)"
```

### Task 12: Route terminfo entry names through `rustty-identity`; keep parity emit under the harness

**Files:**
- Modify: `crates/rustty-terminfo/src/source.rs` (the real generation-time `names` construction — NOT the two `#[cfg(test)]` fixtures at 226/286, which are unit-test data and stay ghostty-literal or get their own identity-aware assertion), `crates/rustty-terminfo/src/lib.rs` (the note at line 9)

**Interfaces:**
- Produces: generated terminfo header line `rustty|xterm-rustty|Rustty` (default) / `ghostty|xterm-ghostty|Ghostty` (harness).

- [ ] **Step 1: Locate the non-test names construction** — the site that builds the `Source.names` for real generation (search outside `#[cfg(test)]`):
```bash
grep -n 'names' crates/rustty-terminfo/src/source.rs crates/rustty-terminfo/src/lib.rs | grep -v test
```

- [ ] **Step 2: Build the names from identity consts** at that site:
```rust
names: vec![
    rustty_identity::TERMINFO_PREFERRED_NAME.as_bytes().to_vec(), // "rustty"
    rustty_identity::TERM.as_bytes().to_vec(),                    // "xterm-rustty"
    rustty_identity::APP_NAME_TITLECASE.as_bytes().to_vec(),      // "Rustty" (see note)
],
```
Note: if `rustty-identity` has no title-case const, add `APP_NAME_TITLECASE` = `"Rustty"`/`"Ghostty"` in Task 10's pattern, or derive it inline. Keep it identity-sourced, not literal.

- [ ] **Step 3: Update the doc note** at `crates/rustty-terminfo/src/lib.rs:9` to: "Default profile emits rustty/xterm-rustty; feature identity-ghostty emits ghostty/xterm-ghostty for the parity harness."

- [ ] **Step 4: Update/guard the `#[cfg(test)]` `encode` fixture** at source.rs:315 — its `expected` literal `ghostty|xterm-ghostty|Ghostty` is a hand-built `Source` with literal names, so it still passes (it doesn't go through identity). Leave the literal test as-is (it tests the *encoder*, not the product name). Add one new test that builds the real generator names and asserts they equal `b"rustty|xterm-rustty|Rustty,\n…"` under the default profile.

- [ ] **Step 5: Test both profiles.**
```bash
cargo test -p rustty-terminfo
cargo test -p rustty-terminfo --features identity-ghostty 2>/dev/null | tail -5
```
Expected: default emits rustty names; harness emits ghostty names.

- [ ] **Step 6: Commit.**
```bash
git add crates/rustty-terminfo
git commit -m "terminfo: entry names via rustty-identity (rustty/xterm-rustty), parity keeps ghostty under harness (Task 12)"
```

### Task 13: Rename shell-integration scripts + route their `GHOSTTY_*` env vars to `RUSTTY_*`

**Files (git mv + content):**
- `crates/rustty-extra/shell-integration/bash/ghostty.bash` → `rustty.bash`
- `crates/rustty-extra/shell-integration/elvish/lib/ghostty-integration.elv` → `rustty-integration.elv`
- `crates/rustty-extra/shell-integration/fish/vendor_conf.d/ghostty-shell-integration.fish` → `rustty-shell-integration.fish`
- `crates/rustty-extra/shell-integration/nushell/vendor/autoload/ghostty.nu` → `rustty.nu`
- `crates/rustty-extra/shell-integration/zsh/ghostty-integration` → `rustty-integration`
- Modify: script contents (all `GHOSTTY_*` → `RUSTTY_*`)
- Modify: `crates/rustty-termio/src/shell_integration.rs` (29 `GHOSTTY_*` refs) + the env-write sites `crates/rustty-core/src/surface.rs`, `crates/rustty-termio/src/exec.rs`, `crates/rustty-os/src/resourcesdir.rs`, `crates/rustty-global/src/lib.rs`, `crates/rustty-renderer/src/lib.rs`, `crates/rustty-cli/src/pager.rs`, `crates/rustty-apprt-api/src/{action,ipc}.rs`, `crates/rustty-apprt/src/embedded.rs`, `crates/rustty-config/src/config_struct.rs`

**Interfaces:**
- Produces: the running terminal sets `RUSTTY_RESOURCES_DIR`, `RUSTTY_SHELL_*`, etc., and injects the `rustty.*` integration scripts. Under `identity-ghostty` the prefix reverts to `GHOSTTY_` (via `rustty_identity::ENV_PREFIX`) so parity holds.

- [ ] **Step 1: Rename the five scripts.**
```bash
cd /Users/gael/projects/rustty/crates/rustty-extra/shell-integration
git mv bash/ghostty.bash bash/rustty.bash
git mv elvish/lib/ghostty-integration.elv elvish/lib/rustty-integration.elv
git mv fish/vendor_conf.d/ghostty-shell-integration.fish fish/vendor_conf.d/rustty-shell-integration.fish
git mv nushell/vendor/autoload/ghostty.nu nushell/vendor/autoload/rustty.nu
git mv zsh/ghostty-integration zsh/rustty-integration
```

- [ ] **Step 2: Rewrite env vars + self-references inside the scripts.**
```bash
cd /Users/gael/projects/rustty/crates/rustty-extra/shell-integration
grep -rlZ 'GHOSTTY_\|ghostty' . | xargs -0 sed -i '' -e 's/GHOSTTY_/RUSTTY_/g' -e 's/ghostty/rustty/g'
```
Then eyeball each for a name that should stay (there should be none in the shipped scripts — they are the port's).

- [ ] **Step 3: Route the emulator's env writes/reads through `rustty_identity::ENV_PREFIX`.** In `crates/rustty-termio/src/shell_integration.rs`, replace the literal `"GHOSTTY_…"` strings with formatted values, e.g.:
```rust
// was: env.put("GHOSTTY_RESOURCES_DIR", …)
env.put(&format!("{}_RESOURCES_DIR", rustty_identity::ENV_PREFIX), …);
```
Apply the same pattern at each env-write/read site listed above. Where a var name is matched (reads), compare against the `ENV_PREFIX`-built name. Keep the ported Zig comments.

- [ ] **Step 4: Confirm no literal `GHOSTTY_` remains in non-test emulator code** (test files may keep literals if they assert the harness profile):
```bash
grep -rn 'GHOSTTY_' crates/rustty-*/src | grep -v '#\[cfg(' | grep -viE 'test|identity-ghostty'
```
Expected: empty (every remaining `GHOSTTY_` is inside an `identity-ghostty`/test context or built from `ENV_PREFIX`).

- [ ] **Step 5: Build + shell-integration tests.**
```bash
cargo build -p rustty-termio -p rustty-core -p rustty-os
cargo test -p rustty-termio 2>/dev/null | tail -8
```
Expected: builds; shell-integration tests updated to `RUSTTY_` pass (update expected env names in those tests).

- [ ] **Step 6: Commit.**
```bash
git add -A crates
git commit -m "shell-integration: rename scripts + route env vars via rustty_identity::ENV_PREFIX (RUSTTY_) (Task 13)"
```

### Task 14: Rename the theme, and route resource generation (`xtask::resources`) to rustty names

**Files:**
- `crates/rustty-extra/themes/Ghostty Default Style Dark` → `Rustty Default Style Dark`
- Modify: `xtask/src/main.rs::resources()` + its `res_terminfo`/`res_shell`/`res_shell_integration`/`res_misc` modules (output filenames: `ghostty.1`/`ghostty.5` man pages, `_ghostty`/`ghostty.fish`/`ghostty.bash` completions, `share/ghostty/` tree, `ghostty.termcap`/`ghostty.terminfo`, the vim/nvim `ghostty.vim`)
- Modify: any tracked default-config or docs that name the theme
- Modify: `macos/Rustty.xcodeproj/project.pbxproj` — the bundled-resources fileRef `name = ghostty; path = "../zig-out/share/ghostty"` → `name = rustty; path = "../zig-out/share/rustty"` (the macOS app bundles the generated resources dir; when Task 14 renames the output `share/ghostty`→`share/rustty`, this pbxproj reference must follow or the app build can't find the resources). Left as `ghostty` by Task 8 precisely because the resource dir isn't renamed until here.

**Interfaces:**
- Produces: `zig-out/share/` regenerates under rustty names (`share/rustty/…`, `rustty.1`, `_rustty`, `rustty.terminfo`, `Rustty Default Style Dark`); the macOS app's bundled-resources reference points at `share/rustty`.

- [ ] **Step 1: Rename the theme file + fix references.**
```bash
cd /Users/gael/projects/rustty
git mv "crates/rustty-extra/themes/Ghostty Default Style Dark" "crates/rustty-extra/themes/Rustty Default Style Dark"
grep -rln 'Ghostty Default Style Dark' crates docs | xargs -r sed -i '' 's/Ghostty Default Style Dark/Rustty Default Style Dark/g'
```

- [ ] **Step 2: Route `resources()` output names through identity.** In `xtask/src/main.rs`, replace literal `"ghostty"` in the output tree/filename construction with `rustty_identity::APP_NAME` (or the `rustty` literal where identity isn't linkable from xtask — prefer a single `const APP: &str = "rustty";` at the top of the module so man/completions/terminfo/share-dir all derive from it): `share/{APP}/…`, `{APP}.1`, `{APP}.5`, `_{APP}`, `{APP}.fish`, `{APP}.bash`, `{APP}.termcap`, `{APP}.terminfo`, `{APP}.vim`.

- [ ] **Step 3: Regenerate resources.**
```bash
cargo run -p xtask -- resources
```
Expected: `zig-out/share/` now contains `share/rustty/`, `rustty.1`, `_rustty`, `rustty.terminfo`, `rustty.vim`, etc. (zig-out is gitignored — this verifies the generator, not tracked output).

- [ ] **Step 4: Verify no `ghostty`-named output remains.**
```bash
find zig-out/share -iname '*ghostty*' | head
```
Expected: empty.

- [ ] **Step 5: Commit.**
```bash
git add -A crates/rustty-extra xtask docs
git commit -m "resources: rename theme + route xtask resource names to rustty (Task 14)"
```

---

## Phase 4 — Class A trace names + final sweep

### Task 15: Rename the port's own fidelity-trace code files

**Files (git mv + content):**
- `crates/rustty/src/main_ghostty.rs` → `crates/rustty/src/main_rustty.rs`
- `crates/rustty-cli/src/ghostty.rs` → `crates/rustty-cli/src/rustty.rs`
- Modify: the `mod`/`path` declarations that reference them (`crates/rustty/src/*.rs`, `crates/rustty-cli/src/lib.rs`), and `use rustty_cli::ghostty::…` call sites

**Interfaces:**
- Produces: trace-file names that read `rustty`. The header-comment provenance line (`PORTS: ghostty src/main_ghostty.zig`) stays — the R1 trace lives there.

- [ ] **Step 1: Find the module wiring.**
```bash
cd /Users/gael/projects/rustty
grep -rn 'main_ghostty\|cli::ghostty\|mod ghostty\|rustty_cli::ghostty' crates/rustty/src crates/rustty-cli/src
```

- [ ] **Step 2: Rename the files.**
```bash
git mv crates/rustty/src/main_ghostty.rs crates/rustty/src/main_rustty.rs
git mv crates/rustty-cli/src/ghostty.rs crates/rustty-cli/src/rustty.rs
```

- [ ] **Step 3: Update the module declarations + import paths.** Replace `mod ghostty;`→`mod rustty;` (or `#[path=…]`), `pub mod ghostty`→`pub mod rustty`, `rustty_cli::ghostty::`→`rustty_cli::rustty::`, `cli::ghostty::Action`→`cli::rustty::Action`, and the `main_ghostty` reference in the binary target. Keep the `PORTS: ghostty src/…` doc lines verbatim.

- [ ] **Step 4: Build the binary crates.**
```bash
cargo build -p rustty -p rustty-cli
```
Expected: builds.

- [ ] **Step 5: Commit.**
```bash
git add -A crates/rustty crates/rustty-cli
git commit -m "trace: rename main_ghostty.rs->main_rustty.rs, cli/ghostty.rs->cli/rustty.rs (provenance kept in comments) (Task 15)"
```

### Task 15b: Rename the vendored SIMD/highway `ghostty_*` C symbols → `rustty_*`

Context: discovered during Task 1 — the vendored C/C++ in `rustty-simd` and `highway-sys` defines internal FFI symbols with a hardcoded `ghostty_` prefix (`ghostty_simd_*`, `ghostty_hwy_detect_targets`). These are **internal** helpers (NOT the Swift-app C-ABI), each defined in a `.cpp`/`.c` file and declared in a Rust `extern "C"` block + called within the same crate — so each is a self-contained rename across both sides. This closes the "no ghostty name anywhere in the port" gap.

**Files (both the C definition and the Rust `extern` + call sites):**
- `crates/rustty-simd/cbits/simd/base64.cpp` (`ghostty_simd_base64_max_length`, `ghostty_simd_base64_decode`)
- `crates/rustty-simd/cbits/simd/codepoint_width.cpp` (`ghostty_simd_codepoint_width`)
- `crates/rustty-simd/cbits/simd/index_of.cpp` (`ghostty_simd_index_of`)
- `crates/rustty-simd/cbits/simd/vt.cpp` (`ghostty_simd_decode_utf8_until_control_seq`)
- `crates/rustty-simd/src/*.rs` (the `extern "C"` decls + callers: `base64.rs`, `codepoint_width.rs`, `index_of.rs`, `vt.rs` — grep to confirm the set)
- `crates-sys/highway-sys/cpp/detect.c`, `crates-sys/highway-sys/cpp/targets.cpp` (`ghostty_hwy_detect_targets`)
- `crates-sys/highway-sys/src/lib.rs` (the `extern "C"` decl + link-test caller)

**Interfaces:**
- Produces: internal SIMD/highway symbols named `rustty_simd_*` / `rustty_hwy_detect_targets`; no behaviour change.

- [ ] **Step 1: Enumerate every occurrence (definition, declaration, caller, comment).**
```bash
cd /Users/gael/projects/rustty
grep -rn 'ghostty_simd_\|ghostty_hwy_' crates/rustty-simd crates-sys/highway-sys
```
Expected: the sites above (C defs, Rust `extern` decls, callers, and doc comments referencing the reference's Zig symbol).

- [ ] **Step 2: Rename `ghostty_simd_`→`rustty_simd_` and `ghostty_hwy_`→`rustty_hwy_` across both crates.** Keep any comment that describes the *reference's* Zig symbol as provenance (e.g. "C port of the reference's Zig `ghostty_hwy_detect_targets`") — rewrite the *live* symbol but you may keep a `// (ref: ghostty …)` provenance note:
```bash
cd /Users/gael/projects/rustty
grep -rlZ 'ghostty_simd_\|ghostty_hwy_' crates/rustty-simd crates-sys/highway-sys \
  | xargs -0 sed -i '' -e 's/ghostty_simd_/rustty_simd_/g' -e 's/ghostty_hwy_/rustty_hwy_/g'
```

- [ ] **Step 3: Build both crates (forces the C recompile + the FFI link).**
```bash
cargo build -p rustty-simd -p highway-sys
cargo test -p rustty-simd -p highway-sys 2>/dev/null | tail -8
```
Expected: builds + the link-test/SIMD tests pass (the rename is symmetric on both FFI sides, so any missed site is a link error).

- [ ] **Step 4: Confirm zero `ghostty_simd_`/`ghostty_hwy_` live symbols remain** (provenance comments referencing the reference Zig symbol are allowed):
```bash
grep -rn 'ghostty_simd_\|ghostty_hwy_' crates/rustty-simd crates-sys/highway-sys | grep -viE 'ref:|reference|//.*ghostty'
```
Expected: empty.

- [ ] **Step 5: Commit.**
```bash
git add -A crates/rustty-simd crates-sys/highway-sys
git commit -m "simd: rename internal vendored ghostty_simd_*/ghostty_hwy_* FFI symbols -> rustty_* (Task 15b)"
```

### Task 16: Final residual sweep + workspace gates + parity harness

**Files:**
- Read-only across the tree; may touch stragglers surfaced by the sweep.

**Interfaces:**
- Produces: a tree whose only `ghostty` names are the documented oracle carve-out + `identity-ghostty` string blocks.

- [ ] **Step 1: Residual grep — everything tracked, minus the carve-out.**
```bash
cd /Users/gael/projects/rustty
git ls-files | grep -i ghostty \
  | grep -vE '^\.port/phase-0/|^\.port/audits/GHOSTTY_LIFETIMES\.tsv|^\.port/tools/PARITY_NAMING\.md'
```
Expected: **empty**. Any survivor is either a straggler to rename or must be added to the carve-out doc with a one-line justification.

- [ ] **Step 2: Residual grep — content (case-insensitive `ghostty`) outside carve-out + identity/parity contexts.**
```bash
git grep -in 'ghostty' -- crates macos xtask include \
  | grep -viE 'identity-ghostty|ENV_PREFIX|PORTS: ghostty|oracle|parity|xterm-ghostty".to_vec\(\).*harness' \
  | grep -viE 'cfg\(feature = "identity-ghostty"\)' | head -40
```
Review each hit: it must be (a) a `PORTS:`/oracle/provenance comment, (b) inside an `identity-ghostty` cfg block, or (c) a genuine straggler → fix it.

- [ ] **Step 3: Workspace build + test.**
```bash
cargo build --workspace
cargo test --workspace 2>/dev/null | tail -20
```
Expected: green (or the pre-existing known-red lanes only — no NEW failures from the rebrand).

- [ ] **Step 4: Prove parity still works** — build the harness profile and confirm identity strings flip back to ghostty:
```bash
cargo test -p rustty-identity --features identity-ghostty
cargo test -p rustty-terminfo --features identity-ghostty 2>/dev/null | tail -5
```
Expected: under the harness, `APP_NAME=="ghostty"`, `TERM=="xterm-ghostty"`, `ENV_PREFIX=="GHOSTTY"`, terminfo emits ghostty names.

- [ ] **Step 5: Rebuild + screenshot the app end-to-end** (final shakedown): resources regenerate under rustty names, `bash macos/build-rustty.sh`, launch `Rustty.app`, screenshot showing a "Rustty" window with a live prompt; open the inspector (⌘⌥I) if P2.3 is landed; confirm `echo $TERM` inside the terminal prints `xterm-rustty` and `env | grep RUSTTY_` shows the integration vars.

- [ ] **Step 6: Commit any stragglers + a completion note.**
```bash
git add -A
git commit -m "rebrand: final residual sweep — tree is rustty-branded, oracle carve-out documented, parity green (Task 16)"
```

---

## Self-Review

**1. Spec coverage** (against the audit's 4 classes + the two locked decisions + the mid-turn "rename A–D, keep parity name-aware"):
- Class A (trace names) → Tasks 4, 15 (+ carve-out for the genuine oracle artifacts, Task 0). ✅
- Class B (C-ABI seam) → Tasks 1–4 (alias deleted, RusttyKit packaged, header regenerated, frozen header dropped). ✅
- Class C (Swift app files/dirs/targets) → Tasks 5–9. ✅
- Class D (runtime identity: TERM, terminfo, env vars, theme, man/completions) → Tasks 10–14. ✅
- Decision 1 "Full RusttyKit rename, drop alias" → Tasks 1, 3, 5, 8. ✅
- Decision 2 "xterm-rustty honest + keep ssh fallback" → Tasks 11 (fallback preserved, Step 3), 12. ✅
- "Parity scripts name-aware" → Task 0 (doc + invariant) + parity re-verified in Tasks 12/16. ✅

**2. Placeholder scan:** No "TBD"/"handle edge cases"/"similar to Task N". The one soft spot — Task 12 Step 2 references `APP_NAME_TITLECASE` which may not yet exist; the step says to add it in Task 10's pattern or derive inline, with an exact value. Task 13's env-write list is enumerated from the actual grep. Acceptable — flagged inline, not hidden.

**3. Type/name consistency:** `RusttyKit` (module), `libRusttyKit.a`, `rustty.h`, `Rustty.xcodeproj`, `Sources/RusttyKit`, `RusttyUITests`, `Rustty` (Swift namespace), `rustty_*` (C symbols), `rustty_identity::{TERM, ENV_PREFIX, TERMINFO_PREFERRED_NAME}`, `xterm-rustty` — used consistently across all tasks. The `identity-ghostty` feature name is deliberately unchanged (it names the *oracle* it reproduces — a name-aware exception, documented in Task 0).

**Known coupling to respect during execution:** Tasks 1→2→3→4 are strictly ordered (alias must die before header regen before repackage before frozen-header delete). Tasks 5→6→7→8→9 are strictly ordered (symbols before namespace before files before pbxproj before build). Phase 3 tasks are independent of Phase 2 and may be done by a separate implementer, but Task 13 and Task 11/12 all touch `config_struct.rs` — run them sequentially, not in parallel.
