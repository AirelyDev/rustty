# Full Rustty Rebrand Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert every product-facing "Ghostty" name in the port — the C-ABI link seam, the macOS Swift app, the runtime identity, and the source-fidelity trace names — to "Rustty", while keeping the differential-parity machinery working by making it explicitly name-aware (port = `rustty` ↔ oracle = `ghostty`).

**Architecture:** Three seams change together. (1) **C-ABI seam** — drop the `ghostty_*` alias layer entirely; the Swift app calls the already-canonical `rustty_*` symbols directly through a `RusttyKit` module. (2) **Swift app** — mechanical symbol/import repoint, then file/dir/Xcode-target renames. (3) **Runtime identity** — route the handful of hardcoded `xterm-ghostty` / `GHOSTTY_*` sites through the `rustty-identity` consts that already default to rustty. The `identity-ghostty` cargo feature is **retained** but **decoupled from the alias**: it now only flips identity *strings* so the byte-differential harness can still reproduce the ghostty oracle exactly.

**Tech Stack:** Rust (cargo workspace + `xtask`), Swift/SwiftUI + Xcode (`xcodebuild`), cbindgen, macOS Metal, shell-integration scripts (bash/zsh/fish/elvish/nushell), terminfo.

## Global Constraints

- **Canonical C symbols are `rustty_*`** — they already exist in `crates/rustty-apprt-c/src/lib.rs` (e.g. `rustty_app_new`, `rustty_surface_new`). The `ghostty_*` names are alias forwarders and are **deleted** by this plan.
- **Shipped Swift module = `RusttyKit`**; xcframework = `RusttyKit.xcframework`; combined static lib = `libRusttyKit.a`; umbrella header staged as `rustty.h` (from cbindgen `include/rustty-apprt.h`).
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

**Files:**
- Read: `.port/tools/gates/check_ghostty_h_abi.sh`
- Read: `crates/rustty-terminfo/src/lib.rs` (the `identity-ghostty emits ghostty/xterm-ghostty` note at line 9)
- Read: `crates/rustty-identity/src/lib.rs:72-127` (the `#[cfg(feature = "identity-ghostty")] mod values` block)
- Create: `.port/tools/PARITY_NAMING.md`

**Interfaces:**
- Produces: the documented invariant that later tasks rely on — "port artifacts are `rustty`-named; the differential harness reproduces the ghostty oracle **only** under `--features identity-ghostty`, which flips identity strings, not filenames."

- [ ] **Step 1: Inventory every place the differential harness compares a rustty artifact to a ghostty-named oracle.**

Run:
```bash
cd /Users/gael/projects/rustty
grep -rn 'identity-ghostty\|xterm-ghostty\|phase-0/corpora\|GHOSTTY_LIFETIMES' .port/tools crates/rustty-terminfo/src 2>/dev/null
```
Expected: a finite list — the `identity-ghostty` feature gate, the terminfo parity emit path, and the phase-0 corpora references. Record each in the doc below.

- [ ] **Step 2: Write `.port/tools/PARITY_NAMING.md`** capturing the mapping and the carve-out:

```markdown
# Parity naming (port ↔ oracle)

The rustty port is the SUBJECT; ghostty is the ORACLE. The differential harness
proves byte-equivalence by building rustty **with `--features identity-ghostty`**,
which flips every `rustty-identity` string constant to its ghostty value. It does
NOT rename any file or symbol.

## Stays ghostty (the oracle — do not rename)
- `/Users/gael/projects/ghostty`               — external read-only oracle tree
- `.port/phase-0/corpora/**`                   — captured ghostty output bytes
- `.port/phase-0/baseline/**`, `examples-out/**` — ghostty's own build outputs
- `.port/audits/GHOSTTY_LIFETIMES.tsv`         — lifetime audit OF ghostty source

## Becomes rustty (the port — renamed by this plan)
- all C symbols: ghostty_* -> rustty_* (alias deleted)
- macos/ Swift app files, dirs, targets, module
- runtime identity strings (routed through rustty-identity)
- shell-integration scripts + GHOSTTY_* env vars -> RUSTTY_*
- trace files: main_ghostty.rs -> main_rustty.rs, cli/ghostty.rs -> cli/rustty.rs

## The invariant
Under the DEFAULT profile everything reports `rustty`. Under `identity-ghostty`
the identity STRINGS report `ghostty` for the oracle diff. A residual-grep gate
that fails on stray `ghostty` MUST exclude the oracle carve-out paths above and
MUST allow the `#[cfg(feature = "identity-ghostty")]` string blocks.
```

- [ ] **Step 3: Commit.**
```bash
git add .port/tools/PARITY_NAMING.md
git commit -m "parity: document port<->oracle naming invariant before rebrand (Task 0)"
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

- [ ] **Step 7: Commit.**
```bash
git add -A crates/rustty-apprt-c crates/rustty-config crates/rustty-bench
git commit -m "abi: delete ghostty_* alias layer; identity-ghostty now flips strings only (Task 1)"
```

### Task 2: Regenerate the canonical apprt header and confirm it covers every symbol the Swift app needs

**Files:**
- Modify (regenerate): `include/rustty-apprt.h` via `cargo run -p xtask -- header-gen-apprt`
- Read: `include/ghostty.h` (the frozen alias header — the symbol set the Swift app currently consumes)

**Interfaces:**
- Produces: `include/rustty-apprt.h` declaring all `rustty_*` entry points the Swift app calls (staged as `rustty.h` in Task 3).

- [ ] **Step 1: Regenerate the header.**
```bash
cd /Users/gael/projects/rustty
cargo run -p xtask -- header-gen-apprt
```
Expected: `include/rustty-apprt.h` rewritten, exit 0.

- [ ] **Step 2: Confirm every symbol the Swift app uses has a `rustty_*` declaration.** Build the exact set the Swift sources reference and diff against the header:
```bash
grep -rhoE 'ghostty_[a-z_]+' macos/Sources macos/GhosttyUITests macos/Tests \
  | sort -u | sed 's/^ghostty_/rustty_/' > /tmp/needed_syms.txt
grep -oE 'rustty_[a-z_]+' include/rustty-apprt.h | sort -u > /tmp/have_syms.txt
comm -23 /tmp/needed_syms.txt /tmp/have_syms.txt
```
Expected: **empty** (every needed `rustty_*` symbol is declared). If any print, they are `_t`/`_s`/`_e` **type** names, not functions — verify each is a typedef in the header (`grep '<name>' include/rustty-apprt.h`); a genuinely missing *function* is a blocker → report it, do not fabricate.

- [ ] **Step 3: Commit.**
```bash
git add include/rustty-apprt.h
git commit -m "abi: regenerate include/rustty-apprt.h (canonical rustty_* surface) (Task 2)"
```

### Task 3: Repackage the xcframework as `RusttyKit` (`xtask::xcframework`)

**Files:**
- Modify: `xtask/src/main.rs` — the `xcframework()` fn and its doc comments (lines ~30, ~193-321)

**Interfaces:**
- Consumes: `include/rustty-apprt.h` (Task 2), the `rustty_*` staticlib.
- Produces: `dist/RusttyKit.xcframework` containing `libRusttyKit.a` + `Headers/rustty.h` + a `module RusttyKit` modulemap.

- [ ] **Step 1: Rewrite the staging block.** In `xcframework()` change the combined-lib path, the header staging, the modulemap, and the `-create-xcframework` output:
```rust
// combined self-contained lib
let lib = root.join(format!("target/{target}/xcframework/libRusttyKit.a"));
// ...
// 2. Stage the headers dir: canonical rustty-apprt.h staged as rustty.h + the RusttyKit module map.
let src_h = root.join("include/rustty-apprt.h");
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
Update the `libtool -o` / `CombineArchivesStep` target name to `libRusttyKit.a`, and update the two doc comments that say `dist/GhosttyKit.xcframework` / "frozen `include/ghostty.h` + a `GhosttyKit` module map" to the Rustty equivalents. The abort-profile staticlib is still built with `--features identity-ghostty`? **No** — for the shipped product it must be the **default** profile now (the alias is gone; the Swift app links `rustty_*`). Change the staticlib build args to drop `--features identity-ghostty` (leave the `[profile.xcframework]` panic=abort profile).

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

### Task 4: Delete the frozen `include/ghostty.h` and retire its ABI gate

**Files:**
- Delete: `include/ghostty.h`
- Rename: `.port/tools/gates/check_ghostty_h_abi.sh` → `.port/tools/gates/check_rustty_h_abi.sh`
- Modify: the renamed gate script + any CI workflow that calls it

**Interfaces:**
- Produces: a tree with a single canonical apprt header (`include/rustty-apprt.h`).

- [ ] **Step 1: Confirm nothing but the (now-updated) xtask referenced the frozen header.**
```bash
grep -rn 'include/ghostty.h\|ghostty\.h' xtask .github .port crates --include='*.rs' --include='*.sh' --include='*.yml' | grep -v rustty-apprt.h
```
Expected: only the gate script (handled next) and possibly comments. If a build path still copies `include/ghostty.h`, fix it to `include/rustty-apprt.h` first.

- [ ] **Step 2: Delete the frozen header + rename the gate.**
```bash
git rm include/ghostty.h
git mv .port/tools/gates/check_ghostty_h_abi.sh .port/tools/gates/check_rustty_h_abi.sh
```

- [ ] **Step 3: Rewrite the gate** to assert the canonical header exposes `rustty_*` (and zero `ghostty_*`) instead of ABI-equivalence to a frozen ghostty.h. Point it at `include/rustty-apprt.h`. Update any `.github/workflows/*.yml` that invoked the old path.

- [ ] **Step 4: Run the gate.**
```bash
bash .port/tools/gates/check_rustty_h_abi.sh
```
Expected: passes (exit 0).

- [ ] **Step 5: Commit.**
```bash
git add -A include .port/tools/gates .github
git commit -m "abi: drop frozen include/ghostty.h; rename gate -> check_rustty_h_abi.sh (Task 4)"
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

- [ ] **Step 1: Confirm the mapping is a clean 1:1 prefix swap** (no `ghostty_` symbol whose `rustty_` twin is missing — already proven in Task 2 Step 2). Snapshot the before-count:
```bash
grep -rc 'ghostty_\|GhosttyKit' macos/Sources macos/GhosttyUITests macos/Tests | awk -F: '{s+=$2} END {print "refs:",s}'
```

- [ ] **Step 2: Repoint C-symbol references and the module import.** Apply across all Swift + the bridging header:
```bash
cd /Users/gael/projects/rustty
grep -rlZ --include='*.swift' --include='*.h' 'ghostty_\|import GhosttyKit\|#import "ghostty' macos/Sources macos/GhosttyUITests macos/Tests \
  | xargs -0 sed -i '' \
      -e 's/\bghostty_/rustty_/g' \
      -e 's/import GhosttyKit/import RusttyKit/g' \
      -e 's/GhosttyKit\./RusttyKit./g'
```
(The `\b` keeps `ghostty_` C symbols targeted; the Swift `Ghostty` namespace enum is handled in Task 6, not here.)

- [ ] **Step 3: Fix the bridging header include** if it pulls the umbrella by path: in `macos/Sources/App/macOS/ghostty-bridging-header.h` change any `#import "ghostty.h"` / `<GhosttyKit/ghostty.h>` to the `RusttyKit`/`rustty.h` form.

- [ ] **Step 4: Verify no `ghostty_` C reference or `GhosttyKit` import remains** (the `Ghostty` *namespace* enum still exists — that is expected until Task 6):
```bash
grep -rn 'ghostty_\|GhosttyKit' macos/Sources macos/GhosttyUITests macos/Tests
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

**Interfaces:**
- Produces: filesystem names matching the Task 6 symbol names. (The `.pbxproj` file references are fixed in Task 8.)

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

**Interfaces:**
- Produces: an Xcode project that builds a `Rustty.app` product with module name `RusttyKit`, referencing the renamed files.

- [ ] **Step 1: Rename the container files/dirs.**
```bash
cd /Users/gael/projects/rustty/macos
git mv Ghostty.xcodeproj Rustty.xcodeproj
git mv "Rustty.xcodeproj/xcshareddata/xcschemes/Ghostty.xcscheme" "Rustty.xcodeproj/xcshareddata/xcschemes/Rustty.xcscheme"
git mv Ghostty-Info.plist Rustty-Info.plist
git mv Ghostty.entitlements Rustty.entitlements
git mv GhosttyDebug.entitlements RusttyDebug.entitlements
git mv GhosttyReleaseLocal.entitlements RusttyReleaseLocal.entitlements
git mv Ghostty.sdef Rustty.sdef
git mv Ghostty.xctestplan Rustty.xctestplan
```

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
  Rustty.xcodeproj/project.pbxproj
```
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

- [ ] **Step 4: Set the product/display identity in `Rustty-Info.plist`.** Confirm `CFBundleName`=`Rustty` (already set), and set `CFBundleExecutable`/`CFBundleIdentifier` build-var-driven values to Rustty where literal. Verify:
```bash
plutil -p macos/Rustty-Info.plist | grep -iE 'bundlename|executable|identifier'
```
Expected: no literal `Ghostty`.

- [ ] **Step 5: Resolve + list the scheme to prove the project parses.**
```bash
cd /Users/gael/projects/rustty/macos
xcodebuild -project Rustty.xcodeproj -list
```
Expected: lists targets `Rustty`, `RusttyUITests`, (Tests) and scheme `Rustty` with no parse error.

- [ ] **Step 6: Commit.**
```bash
git add -A macos
git commit -m "xcode: rename project/targets/scheme/plist/entitlements/sdef Ghostty -> Rustty (Task 8)"
```

### Task 9: Rename the app icon asset and wire the reproducible build to RusttyKit; full app build

**Files:**
- `images/Ghostty.icon` → `images/Rustty.icon` (dir), incl. `Assets/Ghostty.png` → `Assets/Rustty.png`, `icon.json`
- Modify: `macos/build-rustty.sh` (RusttyKit.xcframework, project name)
- Modify: `macos/MILESTONE.md`, `macos/REBRAND.md` (open-command paths, product name)
- Modify: any `icon.json`/asset-catalog reference to the icon image name

**Interfaces:**
- Produces: a building, launchable `Rustty.app`.

- [ ] **Step 1: Rename the icon asset.**
```bash
cd /Users/gael/projects/rustty
git mv images/Ghostty.icon images/Rustty.icon
git mv images/Rustty.icon/Assets/Ghostty.png images/Rustty.icon/Assets/Rustty.png
sed -i '' 's/Ghostty/Rustty/g' images/Rustty.icon/icon.json
grep -rln 'Ghostty' macos/Assets.xcassets 2>/dev/null | xargs -r sed -i '' 's/Ghostty/Rustty/g'
```

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

**Interfaces:**
- Produces: `zig-out/share/` regenerates under rustty names (`share/rustty/…`, `rustty.1`, `_rustty`, `rustty.terminfo`, `Rustty Default Style Dark`).

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
