# rustty curated C header family + ABI-fidelity gate — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give rustty a hand-curated public C API header family mirroring ghostty's `include/ghostty/vt.h`, token-renamed to `include/rustty/vt.h` + `vt/*.h`, verified against rustty's actual exported ABI, so the c-vt examples become clean 1:1 swaps and rustty's C ABI is audited against ghostty's.

**Architecture:** Token-rename ghostty's 30-header VT family → `include/rustty/vt.h` + `include/rustty/vt/*.h`. A gate script proves the curated headers (a) compile, (b) declare only real exports (cross-checked against the cbindgen-generated `rustty.h` = code-derived ground truth + `nm`), and (c) declare struct layouts matching `rustty_type_json()`. Mismatches are fixed in rustty (fidelity), except deliberate divergences which are documented + escalated.

**Tech Stack:** Rust (rustty-vt-c cdylib+staticlib), cbindgen (xtask header-gen), C compiler (`cc`/clang), `nm`, POSIX sh, ghostty oracle at `/Users/gael/projects/ghostty` (READ-ONLY).

## Global Constraints

- Product-name casing: `Ghostty`→`Rustty` (capital, product prose), `ghostty_`→`rustty_`, `GHOSTTY_`→`RUSTTY_`, `<ghostty/…>`→`<rustty/…>`; genuine oracle-path refs (`src/…`, upstream filenames) stay lowercase `ghostty` ([[rename-casing-fidelity]]).
- Scope: VT family ONLY (`ghostty/vt.h` + `ghostty/vt/*.h`, 30 headers). NOT the embedding `ghostty.h` (covered by `rustty-embed.h`). NOT the c-vt example programs (separate Phase-B plan).
- Mismatch policy: fix rustty to match ghostty by default; a genuine intentional divergence → header reflects it + PORT NOTE + flag to the human. Every rustty *code* change ships a native test.
- Canonical public header = `include/rustty/vt.h`. cbindgen `include/rustty.h` = internal verification ground-truth only.
- Native `cargo build --workspace` + native tests stay green throughout.

---

### Task 1: Foundation artifacts + ABI baseline

**Files:**
- Build outputs: `target/release/librustty_vt_c.{dylib,a}`
- Regenerated: `include/rustty.h` (cbindgen)
- Create: `.port/tools/gates/_vt_header_baseline.txt` (exported-symbol snapshot, git-ignored scratch is fine; or commit for provenance)

**Interfaces:**
- Produces: the built cdylib/staticlib, a current cbindgen `rustty.h`, and a sorted list of exported `rustty_*` symbols consumed by Task 3's check (b).

- [ ] **Step 1: Build the C-ABI artifacts**

Run: `cargo build -p rustty-vt-c --release`
Expected: `Finished`; `target/release/librustty_vt_c.dylib` and `.a` exist.

- [ ] **Step 2: Regenerate the cbindgen ground-truth header**

Run: `cargo run -p xtask -- header-gen`
Expected: writes `include/rustty.h`; `git diff --stat include/rustty.h` shows it is current (no unexpected drift).

- [ ] **Step 3: Snapshot exported symbols**

Run: `nm -gjU target/release/librustty_vt_c.dylib | sed 's/^_//' | grep '^rustty_' | sort > .port/tools/gates/_vt_header_baseline.txt`
Expected: a non-empty sorted list (~150+ `rustty_*` symbols).

- [ ] **Step 4: Commit the baseline**

```bash
git add include/rustty.h .port/tools/gates/_vt_header_baseline.txt
git commit -m "vt-headers: foundation — current cbindgen rustty.h + exported-symbol baseline"
```

---

### Task 2: Token-rename the VT header family

**Files:**
- Create: `include/rustty/vt.h` + `include/rustty/vt/*.h` (29 subheaders, incl. `vt/key/*.h`, `vt/mouse/*.h`) — from ghostty's `include/ghostty/vt.h` + `include/ghostty/vt/*.h`.

**Interfaces:**
- Consumes: nothing (mechanical translation of the oracle headers).
- Produces: the curated header family that `#include <rustty/vt.h>` pulls transitively; Task 3 verifies it.

- [ ] **Step 1: Generate the family via token-rename**

```bash
cd /Users/gael/projects/rustty
G=/Users/gael/projects/ghostty/include/ghostty
mkdir -p include/rustty/vt/key include/rustty/vt/mouse
# umbrella + every subheader, preserving subdirs
( cd "$G" && find vt.h vt -name '*.h' ) | while read rel; do
  mkdir -p "include/rustty/$(dirname "$rel")"
  sed -e 's/GHOSTTY_API/RUSTTY_API/g' \
      -e 's/GHOSTTY_/RUSTTY_/g' \
      -e 's/ghostty_/rustty_/g' \
      -e 's/<ghostty\//<rustty\//g' \
      -e 's/Ghostty/Rustty/g' \
      "$G/$rel" > "include/rustty/$rel"
done
```
Expected: `include/rustty/vt.h` + 29 subheaders created.

- [ ] **Step 2: Audit residual `ghostty` (only genuine oracle-path/prose refs may remain)**

Run: `grep -rniE 'ghostty' include/rustty/vt.h include/rustty/vt/ | grep -viE 'src/|oracle|upstream|reference|libghostty'`
Expected: empty, OR only lines that are genuine `src/…` oracle-path references (leave those). Fix any product-name prose that slipped (e.g. a lowercase `ghostty` product mention → `rustty`). Manually retarget `libghostty-vt` → `librustty-vt` if present.

- [ ] **Step 2b: Product-URL branding sweep — `ghostty.org` → `rustty.sh` (repo-wide)**

The rustty product website is **`rustty.sh`**. Replace every product-site
`ghostty.org` URL (and `Ghostty's website` prose) with `rustty.sh`, repo-wide —
NOT only in the new headers. CRITICAL exclusion: **`deps.files.ghostty.org`** URLs
are real hosted vendored-dependency tarballs (provenance for the `-sys` crates) —
leave them untouched.

```bash
# preview product-site refs (must exclude the deps host):
grep -rn 'ghostty\.org' . 2>/dev/null | grep -vE '/target/|/\.git/|/\.claude/|files\.ghostty\.org'
# apply only to product-site URLs:
sed -i '' 's|https://ghostty\.org|https://rustty.sh|g' include/rustty/vt.h
sed -i '' "s|on Ghostty's website, at https://ghostty\.org/docs/config|on Rustty's website, at https://rustty.sh/docs/config|" crates/rustty-config/src/config-template
```
Verify: `grep -rn 'ghostty\.org' . | grep -vE '/target/|/\.git/|/\.claude/|files\.ghostty\.org'` → empty; `deps.files.ghostty.org` count unchanged.

- [ ] **Step 3: Verify the family compiles (gate check a, pre-gate smoke)**

**Compiler note:** in this environment `cc` is a shell alias to `claude` (from the
user profile) — NEVER invoke bare `cc`. Resolve the real compiler once and reuse:
`CC="$(xcrun --find clang 2>/dev/null || echo /usr/bin/clang)"`.

```bash
CC="$(xcrun --find clang 2>/dev/null || echo /usr/bin/clang)"
printf '#include <rustty/vt.h>\nint main(void){return 0;}\n' > "${TMPDIR:-/tmp}/vt_probe.c"
"$CC" -fsyntax-only -Iinclude "${TMPDIR:-/tmp}/vt_probe.c" && echo "SYNTAX OK"
```
Expected: `SYNTAX OK`, no warnings/errors. If a subheader references a macro/type not yet renamed, fix the sed output and re-run.

- [ ] **Step 4: Commit the header family**

```bash
git add include/rustty/
git commit -m "vt-headers: token-rename ghostty/vt.h family -> include/rustty/vt.h + vt/*.h"
```

---

### Task 3: The ABI-fidelity gate

**Files:**
- Create: `.port/tools/gates/check_rustty_vt_headers.sh`

**Interfaces:**
- Consumes: `include/rustty/vt*.h` (Task 2), `target/release/librustty_vt_c.dylib` + `include/rustty.h` (Task 1), `rustty_type_json()`.
- Produces: a pass/fail gate + a printed mismatch report (declared-but-not-exported symbols; struct size/offset diffs), consumed by Task 4.

- [ ] **Step 1: Write the gate script**

Create `.port/tools/gates/check_rustty_vt_headers.sh`:
```sh
#!/bin/sh
# ABI-fidelity gate for the curated rustty/vt.h family (spec:
# docs/superpowers/specs/2026-07-15-rustty-c-header-family-design.md).
set -e
root=$(cd "$(dirname "$0")/../../.." && pwd); cd "$root"
lib=target/release/librustty_vt_c.dylib
[ -f "$lib" ] || cargo build -p rustty-vt-c --release >/dev/null
# `cc` is aliased to `claude` in this env — resolve the real compiler.
CC="$(xcrun --find clang 2>/dev/null || echo /usr/bin/clang)"

echo "== (a) headers compile =="
printf '#include <rustty/vt.h>\nint main(void){return 0;}\n' > "${TMPDIR:-/tmp}/vt_probe.c"
"$CC" -fsyntax-only -Wall -Iinclude "${TMPDIR:-/tmp}/vt_probe.c"
echo "  OK"

echo "== (b) every declared rustty_* prototype is a real export =="
# prototypes declared in the curated headers:
grep -rhoE 'rustty_[a-z0-9_]+' include/rustty/vt.h include/rustty/vt | sort -u > "${TMPDIR:-/tmp}/vt_decl.txt"
nm -gjU "$lib" | sed 's/^_//' | grep '^rustty_' | sort -u > "${TMPDIR:-/tmp}/vt_exp.txt"
# a declared *function* (matches an export prefix) missing from exports is a mismatch;
# filter to function-like tokens by intersecting with the cbindgen ground-truth symbol set.
grep -oE 'rustty_[a-z0-9_]+' include/rustty.h | sort -u > "${TMPDIR:-/tmp}/vt_cbindgen.txt"
missing=$(comm -23 <(comm -12 "${TMPDIR:-/tmp}/vt_decl.txt" "${TMPDIR:-/tmp}/vt_cbindgen.txt") "${TMPDIR:-/tmp}/vt_exp.txt")
if [ -n "$missing" ]; then echo "  MISMATCH — declared+cbindgen but NOT exported:"; echo "$missing" | sed 's/^/    /'; exit 1; fi
echo "  OK"

echo "== (c) struct layouts match rustty_type_json() =="
node .port/tools/check-vt-struct-layouts.mjs
echo "  OK"
echo "VT-HEADERS: GREEN"
```
Make executable: `chmod +x .port/tools/gates/check_rustty_vt_headers.sh`.

- [ ] **Step 2: Write the struct-layout cross-checker (check c)**

Create `.port/tools/check-vt-struct-layouts.mjs`: instantiate the freestanding `rustty_vt_c.wasm` (or a tiny host runner), read `rustty_type_json()`, and for every struct the curated headers declare, compile a `static_assert(sizeof(T)==size && offsetof(T,f)==off)` probe TU (`#include <rustty/vt.h>`) using the `type_json` numbers. A compile failure = a header/ABI layout mismatch. (Reuse the layout-reading logic from `.port/tools/wasm-parity.mjs`.) Emit each mismatch as `STRUCT T.field: header vs type_json`.

```
node .port/tools/check-vt-struct-layouts.mjs   # exits non-zero + prints diffs on mismatch
```

- [ ] **Step 3: Run the gate; capture the mismatch report**

Run: `.port/tools/gates/check_rustty_vt_headers.sh 2>&1 | tee .port/tools/gates/_vt_mismatch_report.txt`
Expected: either `VT-HEADERS: GREEN`, or a printed list of (b) missing-export and (c) struct-layout mismatches. This report drives Task 4.

- [ ] **Step 4: Commit the gate (red is acceptable at this checkpoint)**

```bash
git add .port/tools/gates/check_rustty_vt_headers.sh .port/tools/check-vt-struct-layouts.mjs
git commit -m "vt-headers: ABI-fidelity gate (compile / symbols vs cbindgen / struct layouts vs type_json)"
```

---

### Task 4: Triage + fix every mismatch (policy A) — iterate to green

**Files:**
- Modify (per mismatch): the relevant `crates/rustty-vt-c/src/*.rs` (or the upstream crate defining the type/enum) AND/OR `include/rustty/vt/*.h` (only for a deliberate divergence).
- Test (per code fix): the crate's own `#[cfg(test)]` module or `tests/`.

**Interfaces:**
- Consumes: `.port/tools/gates/_vt_mismatch_report.txt` (Task 3).
- Produces: a green gate. Each mismatch → one focused commit.

Method — for EACH mismatch the gate reports, in order:

- [ ] **Step 1: Classify the mismatch**

Compare the curated header's declaration against ghostty's original header AND rustty's actual ABI:
- **rustty bug** (rustty's export/layout/enum differs from ghostty's, no deliberate reason) → Step 2.
- **intentional divergence** (a documented faithful-but-safe choice, or a genuinely-absent ghostty feature) → Step 4.

- [ ] **Step 2 (rustty bug): fix the Rust so the ABI matches ghostty**

Edit the defining Rust type/fn/enum in `crates/rustty-vt-c/src/` (or the crate it re-exports) so the export name / `#[repr(C)]` layout / enum discriminant matches ghostty's header. **Worked precedent:** the `type_json` `usize`→`u32` fix (`crates/rustty-vt-c/src/types.rs::resolve_ty`) — a real layout-metadata mismatch, fixed centrally, native output unchanged.

- [ ] **Step 3 (rustty bug): add a native test + re-run the gate**

Add a `#[test]` asserting the corrected ABI (e.g. the struct's `size_of`/`offset_of`, or that the symbol/enum value equals ghostty's). Run:
`cargo test -p rustty-vt-c <test> && cargo build -p rustty-vt-c --release && .port/tools/gates/check_rustty_vt_headers.sh`
Expected: test passes; that mismatch clears. Commit: `git commit -am "vt-headers: fix <thing> ABI to match ghostty (<check>)"`. Return to Step 1 for the next mismatch.

- [ ] **Step 4 (intentional divergence): reflect + flag**

Edit the curated header to match rustty's actual ABI, add a `// PORT NOTE:` explaining the deliberate divergence (cite the doc/precedent), and record it in the plan's status ledger below. Re-run the gate; commit `git commit -am "vt-headers: document intentional ABI divergence for <thing>"`. **Then surface it to the human** before continuing (policy A escalation).

- [ ] **Step 5: Loop until green**

Repeat Steps 1–4 until `.port/tools/gates/check_rustty_vt_headers.sh` prints `VT-HEADERS: GREEN`.

---

### Task 5: Wire the gate + finalize

**Files:**
- Modify: `.port/tools/gates/README.md` (register the new gate)
- Modify: `.port/phase-2/EXAMPLES_PORT.md` (note the header foundation is done; Phase B unblocked)

- [ ] **Step 1: Register the gate**

Add a line to `.port/tools/gates/README.md` describing `check_rustty_vt_headers.sh` (what it checks, when to run). Confirm it is listed alongside the other `check_*` gates.

- [ ] **Step 2: Full green sweep**

Run: `cargo build --workspace && .port/tools/gates/check_rustty_vt_headers.sh`
Expected: workspace builds; `VT-HEADERS: GREEN`.

- [ ] **Step 3: Update the examples ledger**

In `.port/phase-2/EXAMPLES_PORT.md`, under Phase B, note: "Foundation DONE — curated `include/rustty/vt.h` family + `check_rustty_vt_headers.sh` gate; c-vt example ports now unblocked (each `.c` symbol-swaps onto `<rustty/vt.h>`)."

- [ ] **Step 4: Commit**

```bash
git add .port/tools/gates/README.md .port/phase-2/EXAMPLES_PORT.md
git commit -m "vt-headers: register gate + mark Phase-B foundation complete"
```

## Status ledger (intentional divergences)
_(append here as Task 4 Step 4 finds them; empty = full fidelity)_

---

## Self-Review

**Spec coverage:** Component 1 (header family) → Task 2. Component 2 (gate, checks a/b/c) → Task 3 + the layout checker. Mismatch policy A → Task 4. cbindgen-as-ground-truth → Task 3 check (b). Enum-value verification → folded into Task 4 (discriminant mismatches surface via check c's static_assert on enum-typed fields + Phase-B parity). Testing/gate registration → Task 5. Foundation build → Task 1. All spec sections mapped.

**Placeholder scan:** Task 4 is intentionally iterative (the mismatch set is discovered by running the gate), but each *step* is concrete (classify → fix Rust → native test → commit) with a worked precedent (the `resolve_ty` fix). No "TBD"/"handle edge cases"/vague steps.

**Type consistency:** the gate script names (`check_rustty_vt_headers.sh`, `check-vt-struct-layouts.mjs`, `_vt_mismatch_report.txt`) are used consistently across Tasks 3–5; header paths (`include/rustty/vt.h`) consistent Tasks 2–5.
