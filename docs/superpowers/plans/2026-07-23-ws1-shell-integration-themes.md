# WS1: Shell-Integration + Themes Resources Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the vendored shell-integration scripts + 534-theme corpus fully closed out: oracle-parity proven by KAT, upstream attribution vendored, the obsolete `share/{APP}/DEFERRALS.md` placeholder emission deleted, `cargo run -p xtask -- resources` verified reproducible end-to-end (clean checkout → app bundle), and the `.port/phase-2` ledgers struck truthful.

**Architecture:** The heavy lifting already landed on main (commits `bc3405f` Task 13, `2480b4f` Task 14, `9a5d616` Task 17, plus Task 18): the vendored trees live at `crates/rustty-extra/shell-integration/` (7 scripts, R2-rebranded `RUSTTY_*`/`rustty-*`) and `crates/rustty-extra/themes/` (534 theme files + `README.md`), and `xtask`'s `res_shell_integration.rs` / `res_themes.rs` emit them to `zig-out/share/{APP}/{shell-integration,themes}` (`{APP}` = `rustty` default / `ghostty` under the `identity-oracle` feature). Runtime lookups already match: `rustty-termio/src/shell_integration.rs` derives script paths from `rustty_identity::APP_NAME`/`env_name()` (`RUSTTY_` prefix), and `rustty-config/src/theme.rs` `Location::Resources` resolves `share/{APP_NAME}/themes` via `resources_dir_app()`. **This WS therefore does NOT re-vendor anything.** What remains: (1) a KAT proving the vendored shell-integration tree equals the oracle tree under the per-token rebrand mapping (with `bash-preexec.sh` compared verbatim — it is third-party and was deliberately NOT rebranded); (2) MIT attribution for the iTerm2-Color-Schemes-derived themes corpus; (3) deleting the now-false `DEFERRALS.md` placeholder emission in `res_terminfo.rs::gen_app_root` and the `mark_resolved` appendix machinery in `res_shell_integration.rs` (plus scrubbing the stale on-disk note so it never ships in the app bundle); (4) end-to-end reproducibility verification (regen, byte-compare, app rebuild, headless zsh injection smoke); (5) ledger strikes in `.port/phase-2/INTEGRATION_PARITY.md` §2.1/§2.2 + `DEFERRALS.md`.

**Tech Stack:** Rust 1.93.1 (pinned), cargo workspace `xtask` crate (plain `std::fs` emitters + `include_bytes!`), zsh (headless injection smoke), xcodebuild via `macos/build-rustty.sh`, `curl`/`tar` for the optional upstream-tarball parity check.

## Global Constraints

(Inherited verbatim from `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` — every task implicitly includes these.)

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e`); the oracle is reference-only — no build-time dependency on it (R3).
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty); `ghostty_*` C-ABI aliases stay.
- R4/R5: acyclic crate graph; gates (`check_acyclic.sh`, A4 ABI gate, Miri lanes) are invariant — never weakened to pass.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI.
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Ledger discipline: when a WS lands, strike its entries in `.port/phase-2/DEFERRALS.md` / `VERIFICATION_DEBT.md` in the same PR.
- Commit style: `subsystem: summary (chunk-ref)` convention; branch per WS; merge to main only with both platforms green (`cargo build --workspace` on macOS AND `ssh rvm 'cd ~/projects/rustty && cargo build --workspace'`).

**WS1-specific constraints (established by research — do not re-litigate):**

- The oracle KAT is a **dev-only `#[ignore]`d test** (run explicitly with `--ignored`); a default `cargo test` must never touch `../ghostty` (R3).
- The rebrand mapping is exactly three per-token replacements applied in this order: `GHOSTTY→RUSTTY`, `Ghostty→Rustty`, `ghostty→rustty` — to file **names** and file **contents**. Verified empirically: all 6 first-party scripts already match the oracle under this mapping; `bash/bash-preexec.sh` is third-party (Kitty-era `GHOSTTY:` patch annotations are upstream markers, not product identity) and matches the oracle **verbatim**.
- The themes attribution file MUST be named `LICENSE.md` so the existing `.md` exclusion (mirroring ghostty's `exclude_extensions = &.{".md"}`) keeps the emitted corpus at exactly 534 files — byte/count parity with ghostty's shipped set, which ships no license file either (attribution lives in the repo, matching ghostty's precedent where it lives in the upstream tarball's repo).
- `+list-themes` through the app binary is **blocked on WS4** (`rustty_cli_try_action`'s non-null action arm in `crates/rustty-apprt-c/src/lib.rs:259` is `unreachable!` until `GlobalState<Action>`/`detectArgs` are wired). The 534-count end-to-end check in this WS is bundle-inspection + the xtask corpus test + `cargo test -p rustty-cli list_themes`; the live-CLI check is explicitly re-queued to WS4's verification.

---

### Task 1: Branch + oracle-parity KAT for the vendored shell-integration tree

**Files:**
- Modify: `xtask/src/res_shell_integration.rs` (append to the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: the vendored tree `crates/rustty-extra/shell-integration/`, the oracle tree `/Users/gael/projects/ghostty/src/shell-integration/` (read-only, dev-machine only).
- Produces: `#[ignore]`d test `oracle_shell_integration_parity` — the standing re-vendoring gate (future re-vendors from a newer oracle must keep it green).

- [ ] **Step 1: Create the WS branch**

```bash
cd /Users/gael/projects/rustty
git checkout -b ws1-shell-integration-themes
```

- [ ] **Step 2: Write the KAT (expected to PASS — it is a characterization gate, not red/green TDD)**

Append inside `mod tests` in `/Users/gael/projects/rustty/xtask/src/res_shell_integration.rs` (after the existing `walk` helper):

```rust
    /// Per-token product rebrand (R2, rename-casing-fidelity): applied to both
    /// file names and file contents when mapping oracle -> vendored.
    fn rebrand(s: &str) -> String {
        s.replace("GHOSTTY", "RUSTTY")
            .replace("Ghostty", "Rustty")
            .replace("ghostty", "rustty")
    }

    /// Oracle-relative paths (post-rebrand spelling) whose CONTENT is compared
    /// VERBATIM against the oracle instead of through [`rebrand`]. Every entry
    /// requires a written justification:
    ///
    /// * `bash/bash-preexec.sh` — third-party vendored file (rcaloras/bash-preexec,
    ///   MIT). Task 13 deliberately did NOT rebrand it: its `# GHOSTTY:` comment
    ///   lines are ghostty-upstream PATCH ANNOTATIONS inside third-party code
    ///   (provenance markers), not product identity, and the file contains no
    ///   `GHOSTTY_*` env vars or `ghostty` command references.
    const VERBATIM_FILES: &[&str] = &["bash/bash-preexec.sh"];

    /// R1/R2 oracle KAT (dev-only): the vendored shell-integration tree must
    /// equal ghostty's `src/shell-integration/` tree mapped through the
    /// per-token rebrand (names + contents), excluding `*.md` on both sides,
    /// with [`VERBATIM_FILES`] compared byte-for-byte instead.
    ///
    /// Run explicitly (never in default `cargo test` — R3 no oracle dep):
    ///   cargo test -p xtask -- --ignored oracle_shell_integration_parity
    #[test]
    #[ignore = "requires the read-only ghostty oracle checkout at ../ghostty (R3: dev-only)"]
    fn oracle_shell_integration_parity() {
        let oracle_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../ghostty/src/shell-integration");
        let vendored_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/rustty-extra/shell-integration");
        assert!(
            oracle_root.is_dir(),
            "ghostty oracle not found at {} — this KAT only runs on the dev machine",
            oracle_root.display()
        );

        let mut failures: Vec<String> = Vec::new();
        let mut mapped_set: Vec<std::path::PathBuf> = Vec::new();

        // Forward direction: every oracle file (non-.md, incl. dotfiles like
        // zsh/.zshenv) must exist vendored under the rebranded name with the
        // expected content.
        for oracle_path in walk(&oracle_root) {
            if oracle_path.extension().and_then(|e| e.to_str()) == Some("md") {
                continue; // README.md — excluded on both sides
            }
            let rel = oracle_path
                .strip_prefix(&oracle_root)
                .unwrap()
                .to_str()
                .expect("oracle path is UTF-8");
            let mapped_rel = rebrand(rel);
            let vendored_path = vendored_root.join(&mapped_rel);
            mapped_set.push(vendored_path.clone());

            let Ok(vendored) = std::fs::read_to_string(&vendored_path) else {
                failures.push(format!("MISSING vendored file: {mapped_rel} (from oracle {rel})"));
                continue;
            };
            let oracle = std::fs::read_to_string(&oracle_path)
                .unwrap_or_else(|e| panic!("read oracle {rel}: {e}"));
            let expected = if VERBATIM_FILES.contains(&mapped_rel.as_str()) {
                oracle
            } else {
                rebrand(&oracle)
            };
            if vendored != expected {
                failures.push(format!("CONTENT DIVERGES: {mapped_rel}"));
            }
        }

        // Reverse direction: no vendored non-.md file exists that the mapped
        // oracle set does not account for (catches stray additions).
        for vendored_path in walk(&vendored_root) {
            if vendored_path.extension().and_then(|e| e.to_str()) == Some("md") {
                continue; // README.md / VENDORING.md — repo docs, never emitted
            }
            if !mapped_set.contains(&vendored_path) {
                failures.push(format!(
                    "EXTRA vendored file with no oracle counterpart: {}",
                    vendored_path.strip_prefix(&vendored_root).unwrap().display()
                ));
            }
        }

        assert!(
            failures.is_empty(),
            "vendored shell-integration tree diverged from the oracle under the \
             rebrand mapping:\n  {}",
            failures.join("\n  ")
        );
    }
```

Note: `walk` (already defined in this test module) recurses via `read_dir`, which DOES return dotfiles (`zsh/.zshenv`) — no extra handling needed.

- [ ] **Step 3: Run the KAT**

```bash
cd /Users/gael/projects/rustty
cargo test -p xtask -- --ignored oracle_shell_integration_parity
```

Expected: `test res_shell_integration::tests::oracle_shell_integration_parity ... ok` (1 passed). Research pre-verified all 7 files match under this exact mapping (6 rebranded, `bash-preexec.sh` verbatim). If it FAILS: the vendored tree has drifted — inspect each listed path against the oracle; fix the vendored file (or, only for a genuinely justified third-party case, extend `VERBATIM_FILES` with a written justification). Do NOT weaken the mapping.

- [ ] **Step 4: Run the default xtask test suite to confirm the ignored test stays out of it**

```bash
cargo test -p xtask 2>&1 | tail -5
```

Expected: all tests pass; the summary line lists `oracle_shell_integration_parity` under `ignored` (e.g. `... N passed; 0 failed; 1 ignored ...`).

- [ ] **Step 5: Commit**

```bash
git add xtask/src/res_shell_integration.rs
git commit -m "xtask: oracle-parity KAT for the vendored shell-integration tree (WS1 T1)"
```

---

### Task 2: Vendor the iTerm2-Color-Schemes MIT attribution (`LICENSE.md`)

**Files:**
- Create: `crates/rustty-extra/themes/LICENSE.md`
- Modify: `crates/rustty-extra/themes/README.md` (add a Licensing section)
- Test: `xtask/src/res_themes.rs` (append to existing `mod tests`)

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `crates/rustty-extra/themes/LICENSE.md` — repo-side attribution; excluded from emission by the existing `.md` filter in `gen_themes` (emitted count stays 534, matching ghostty's shipped set which also ships no license file).

- [ ] **Step 1: Write the failing test**

Append inside `mod tests` in `/Users/gael/projects/rustty/xtask/src/res_themes.rs`:

```rust
    /// The vendored corpus carries the upstream MIT attribution in-repo
    /// (`LICENSE.md`, iTerm2-Color-Schemes). It is `.md` so the emitter's
    /// exclusion keeps the SHIPPED set at exactly 534 files — parity with
    /// ghostty, whose bundle ships no license file either.
    #[test]
    fn vendored_corpus_carries_upstream_license() {
        let lic = Path::new(VENDORED_THEMES).join("LICENSE.md");
        let text = fs::read_to_string(&lic).expect("themes LICENSE.md present");
        assert!(text.contains("MIT License"), "must carry the upstream MIT text");
        assert!(text.contains("Mark Badolato"), "must credit the upstream author");
        assert!(
            text.contains("iTerm2-Color-Schemes"),
            "must name the upstream project"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cargo test -p xtask vendored_corpus_carries_upstream_license
```

Expected: FAIL with `themes LICENSE.md present` (NotFound).

- [ ] **Step 3: Create the attribution file**

Write `/Users/gael/projects/rustty/crates/rustty-extra/themes/LICENSE.md` with exactly this content (the MIT text below is the upstream `mbadolato/iTerm2-Color-Schemes` `LICENSE`, fetched verbatim during WS1 research; re-fetch to double-check with `curl -sL https://raw.githubusercontent.com/mbadolato/iTerm2-Color-Schemes/master/LICENSE` — if offline, use the text below as-is, it was verified 2026-07-23):

```markdown
# Themes corpus license (upstream attribution)

The theme files in this directory are derived from the
[iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes)
collection (via ghostty's pinned `iterm2_themes` release tarball
`ghostty-themes-release-20260608-160426-8c84dd1.tgz` — see `README.md` for full
provenance). The collection is MIT-licensed; the upstream `LICENSE` text is
reproduced verbatim below.

This file is repo-side attribution only: it is `.md`, so `xtask`'s theme
emitter excludes it from the app bundle (mirroring ghostty's
`exclude_extensions = &.{".md"}`), keeping the shipped corpus byte- and
count-identical (534 files) to ghostty's.

---

MIT License

Copyright (c) 2011 to Present Mark Badolato

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

This license covers the iTerm-Color-Schemes repository collection of themes.

The copyright/license for each individual theme belongs to the author of that
theme.
```

- [ ] **Step 4: Add a Licensing pointer to the themes README**

In `/Users/gael/projects/rustty/crates/rustty-extra/themes/README.md`, append at the end of the file:

```markdown

## Licensing

The corpus derives from [iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes)
(MIT); the upstream license text is vendored verbatim in `LICENSE.md` in this
directory (repo-side attribution — excluded from the app bundle by the `.md`
emit filter, keeping the shipped set exactly 534 files like ghostty's).
Per-theme copyright belongs to each theme's author (see the upstream license's
final clause).
```

- [ ] **Step 5: Run tests to verify pass (incl. the count-stays-534 invariant)**

```bash
cargo test -p xtask vendored_corpus_carries_upstream_license
cargo test -p xtask emits_full_theme_corpus
```

Expected: both PASS. `emits_full_theme_corpus` asserting 534 proves `LICENSE.md` is excluded from emission (if it shipped, the count would be 535 and the test would fail).

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-extra/themes/LICENSE.md crates/rustty-extra/themes/README.md xtask/src/res_themes.rs
git commit -m "resources: vendor iTerm2-Color-Schemes MIT attribution for the themes corpus (WS1 T2)"
```

---

### Task 3: Delete the `share/{APP}/DEFERRALS.md` placeholder emission

The shell-integration and themes subtrees ship real vendored content, so the placeholder note written by `res_terminfo.rs::gen_app_root` (and the `mark_resolved` appendix bolted onto it by `res_shell_integration.rs`) is now FALSE documentation being copied into the app bundle. Delete both; scrub any stale note left in `zig-out` by earlier builds.

**Files:**
- Modify: `xtask/src/res_terminfo.rs` (module doc lines 51-56; `gen_app_root` lines 210-254; test lines 296-298)
- Modify: `xtask/src/res_shell_integration.rs` (fn doc lines 90-93; `mark_resolved` call line 110; `RESOLVED_MARKER`/`mark_resolved` lines 114-143)
- Modify: `xtask/src/main.rs` (stale doc line ~446-448)

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `gen_app_root(share_dir) -> Result<(), String>` (same signature, new behavior: mkdir + scrub stale note, never writes one). Task 4's regeneration relies on the scrub to clean `zig-out/share/rustty/DEFERRALS.md` from the working tree.

- [ ] **Step 1: Turn the existing tests RED (assert the note is absent / scrubbed)**

In `/Users/gael/projects/rustty/xtask/src/res_terminfo.rs`, in test `gen_terminfo_produces_compiled_entry`, replace:

```rust
        // share/{APP} root + note exist.
        assert!(share.join(crate::APP).is_dir());
        assert!(share.join(crate::APP).join("DEFERRALS.md").is_file());
```

with:

```rust
        // share/{APP} root exists; the pre-WS1 DEFERRALS.md placeholder is GONE
        // (shell-integration + themes ship real vendored content now).
        assert!(share.join(crate::APP).is_dir());
        assert!(!share.join(crate::APP).join("DEFERRALS.md").exists());
```

Then append a new test to the same `mod tests`:

```rust
    /// Regenerating over a stale output dir SCRUBS the pre-WS1 placeholder note
    /// (so it can never ship in the app bundle again).
    #[test]
    fn gen_app_root_scrubs_stale_deferrals_note() {
        let base = std::env::temp_dir()
            .join(format!("rustty-xtask-approot-test-{}", std::process::id()));
        let share = base.join("share");
        let app_dir = share.join(crate::APP);
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::write(app_dir.join("DEFERRALS.md"), b"stale placeholder").unwrap();

        gen_app_root(&share).expect("gen_app_root");

        assert!(app_dir.is_dir());
        assert!(!app_dir.join("DEFERRALS.md").exists(), "stale note must be scrubbed");
        let _ = std::fs::remove_dir_all(&base);
    }
```

In `/Users/gael/projects/rustty/xtask/src/res_shell_integration.rs`, in test `emits_real_vendored_tree_excluding_md`, add directly after the `gen_shell_integration(&share).expect(...)` line:

```rust
        // WS1: this generator no longer writes/annotates any DEFERRALS.md note.
        assert!(!share.join(crate::APP).join("DEFERRALS.md").exists());
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p xtask gen_terminfo_produces_compiled_entry gen_app_root_scrubs_stale_deferrals_note
cargo test -p xtask emits_real_vendored_tree_excluding_md
```

Expected: FAIL — `gen_terminfo_produces_compiled_entry` on the `!...exists()` assertion (note still written), `emits_real_vendored_tree_excluding_md` on the new assertion (mark_resolved writes the note when absent... it writes via `read_to_string().unwrap_or_default()` + append, creating the file). `gen_app_root_scrubs_stale_deferrals_note` also FAILS (current gen_app_root overwrites, never removes).

- [ ] **Step 3: Rewrite `gen_app_root` in `res_terminfo.rs`**

Replace the whole function (current lines 210-254, doc comment included) with:

```rust
/// Create the `share/{APP}/` resource root the macOS Copy-Resources phase
/// references as `../zig-out/share/{APP}` (`{APP}` = [`crate::APP`]: `rustty`
/// by default, `ghostty` under `identity-oracle` — Task 14; this dir must
/// match the runtime lookup, which resolves the same way via
/// `rustty_os::resourcesdir`'s `app_subdir()` -> `rustty_identity::APP_NAME`).
///
/// The subtrees under it — `shell-integration/` and `themes/` — are populated
/// by the sibling generators (`res_shell_integration::gen_shell_integration`,
/// `res_themes::gen_themes`) from the REAL vendored trees in
/// `crates/rustty-extra/` (WS1); this fn only guarantees the root exists.
/// It also SCRUBS the pre-WS1 `DEFERRALS.md` placeholder note from stale
/// output dirs so it can never ship in the app bundle again.
fn gen_app_root(share_dir: &Path) -> Result<(), String> {
    let app_dir = share_dir.join(crate::APP);
    std::fs::create_dir_all(&app_dir)
        .map_err(|e| format!("create {}: {e}", app_dir.display()))?;

    let stale_note = app_dir.join("DEFERRALS.md");
    match std::fs::remove_file(&stale_note) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("remove stale {}: {e}", stale_note.display())),
    }
    Ok(())
}
```

Also replace the stale module-doc paragraph at lines 51-56:

```rust
//! The `share/{APP}/` tree (shell-integration scripts, themes) is NOT yet ported
//! as rustty data — ghostty ships those as static files under
//! `src/shell-integration/` with no generator, and rustty has not vendored them.
//! Per R5 we therefore create a minimal VALID `share/{APP}/` folder with a
//! self-documenting `DEFERRALS.md` note (never faked script content); see
//! [`gen_app_root`].
```

with:

```rust
//! The `share/{APP}/` tree's contents (shell-integration scripts, themes) are
//! emitted by the sibling generators `res_shell_integration` / `res_themes`
//! from the vendored trees under `crates/rustty-extra/` (WS1 — the pre-WS1
//! `DEFERRALS.md` placeholder note is gone); [`gen_app_root`] only guarantees
//! the root folder exists and scrubs any stale note from earlier builds.
```

- [ ] **Step 4: Delete the `mark_resolved` machinery in `res_shell_integration.rs`**

Three deletions/edits:

1. In `gen_shell_integration`, delete the line `mark_resolved(share_dir)?;` (line 110).
2. Delete the `RESOLVED_MARKER` const and the whole `mark_resolved` fn (lines 114-143).
3. In `gen_shell_integration`'s doc comment, replace:

```rust
/// `share_dir` is the `share/` directory (e.g. `zig-out/share`). Writes every
/// file in [`FILES`] under `share_dir/{APP}/shell-integration/`, creating
/// parent directories as needed, then marks the `shell-integration` entry
/// RESOLVED in the `share/{APP}/DEFERRALS.md` note (idempotent, additive —
/// it never clobbers other resolutions written to the same note).
```

with:

```rust
/// `share_dir` is the `share/` directory (e.g. `zig-out/share`). Writes every
/// file in [`FILES`] under `share_dir/{APP}/shell-integration/`, creating
/// parent directories as needed. (The pre-WS1 `DEFERRALS.md` note-annotation
/// step is gone — the emitted tree IS the real content.)
```

- [ ] **Step 5: Fix the stale doc line in `main.rs`**

In `/Users/gael/projects/rustty/xtask/src/main.rs` (fn `resources` doc, ~line 446-448), replace:

```rust
/// All ten folders end up present so the Xcode Copy-Resources phase resolves
/// every reference (R5: any not-yet-ported generator still leaves a minimal
/// VALID folder + a DEFERRALS entry, never fake content).
```

with:

```rust
/// All ten folders end up present so the Xcode Copy-Resources phase resolves
/// every reference. As of WS1 every subtree carries REAL generated/vendored
/// content — no placeholder DEFERRALS notes are emitted anywhere (R5).
```

- [ ] **Step 6: Run the full xtask test suite to verify pass**

```bash
cargo test -p xtask
```

Expected: all PASS (including the three tests from Step 1), `oracle_shell_integration_parity` ignored. If `mark_resolved`/`RESOLVED_MARKER` removal left dead-code warnings, the deletion was incomplete — remove all remnants.

- [ ] **Step 7: Commit**

```bash
git add xtask/src/res_terminfo.rs xtask/src/res_shell_integration.rs xtask/src/main.rs
git commit -m "xtask: delete the share/{APP} DEFERRALS.md placeholder emission — resources are real now (WS1 T3)"
```

---

### Task 4: Reproducibility verification — clean regeneration + byte-compare

Prove a clean checkout reproduces the exact resource tree (the roadmap's KAT gate). All commands from `/Users/gael/projects/rustty`.

**Files:**
- No source changes (verification only; `zig-out/` is untracked output).

**Interfaces:**
- Consumes: Task 3's scrub behavior; the vendored trees.
- Produces: a regenerated `zig-out/share/` that Task 5's app build consumes.

- [ ] **Step 1: Snapshot the current (pre-WS1, hand-verified-good) output for comparison**

```bash
cp -R zig-out/share /private/tmp/claude-501/-Users-gael-projects-rustty/4ae4ff76-65a5-423c-8692-da2f6a7506b2/scratchpad/share-before-ws1
```

- [ ] **Step 2: Regenerate from scratch**

```bash
rm -rf zig-out/share
cargo run -p xtask -- resources
```

Expected: exits 0.

- [ ] **Step 3: Assert the placeholder is gone and the trees are complete**

```bash
test ! -e zig-out/share/rustty/DEFERRALS.md && echo NO-PLACEHOLDER
ls zig-out/share/rustty/themes | wc -l
find zig-out/share/rustty/shell-integration -type f | sort
```

Expected: `NO-PLACEHOLDER`; theme count `534`; exactly these 7 files:
`bash/bash-preexec.sh`, `bash/rustty.bash`, `elvish/lib/rustty-integration.elv`, `fish/vendor_conf.d/rustty-shell-integration.fish`, `nushell/vendor/autoload/rustty.nu`, `zsh/.zshenv`, `zsh/rustty-integration`.

- [ ] **Step 4: Byte-compare emitted output vs the vendored sources and vs the pre-WS1 snapshot**

```bash
diff -r crates/rustty-extra/shell-integration zig-out/share/rustty/shell-integration
diff -r crates/rustty-extra/themes zig-out/share/rustty/themes
diff -r /private/tmp/claude-501/-Users-gael-projects-rustty/4ae4ff76-65a5-423c-8692-da2f6a7506b2/scratchpad/share-before-ws1/rustty zig-out/share/rustty
```

Expected: first diff prints ONLY two `Only in .../shell-integration: README.md` / `VENDORING.md` lines; second prints ONLY `Only in .../themes: README.md` and `Only in .../themes: LICENSE.md`; third prints ONLY `Only in .../share-before-ws1/rustty: DEFERRALS.md` (the scrubbed placeholder — every real file byte-identical). Any other line = regression; stop and debug before proceeding.

- [ ] **Step 5: ghostty parity profile (`identity-oracle`) still emits `share/ghostty/`**

```bash
cargo run -p xtask --features identity-oracle -- resources
ls zig-out/share/ghostty/themes | wc -l
ls zig-out/share/ghostty/shell-integration/zsh/
test ! -e zig-out/share/ghostty/DEFERRALS.md && echo NO-PLACEHOLDER-ORACLE
```

Expected: `534`; `rustty-integration` (+ hidden `.zshenv` — script CONTENT is unconditionally rustty-spelled by design, only the `share/ghostty/` ROOT flips; see `res_shell_integration.rs`'s R2 note); `NO-PLACEHOLDER-ORACLE`.

- [ ] **Step 6: Restore the clean default-profile output tree**

```bash
rm -rf zig-out/share
cargo run -p xtask -- resources
test -d zig-out/share/rustty && test ! -d zig-out/share/ghostty && echo CLEAN
```

Expected: `CLEAN`.

- [ ] **Step 7 (optional, network): themes corpus parity vs the pinned upstream tarball**

```bash
SCRATCH=/private/tmp/claude-501/-Users-gael-projects-rustty/4ae4ff76-65a5-423c-8692-da2f6a7506b2/scratchpad
curl -sL --max-time 120 -o "$SCRATCH/themes.tgz" \
  https://deps.files.ghostty.org/ghostty-themes-release-20260608-160426-8c84dd1.tgz
mkdir -p "$SCRATCH/themes-oracle" && tar -xzf "$SCRATCH/themes.tgz" -C "$SCRATCH/themes-oracle"
tar -tzf "$SCRATCH/themes.tgz" | head -3   # inspect layout: flat, or under a top-level dir
# If flat:
diff -r "$SCRATCH/themes-oracle" crates/rustty-extra/themes
# If under a single top-level dir <D>:
#   diff -r "$SCRATCH/themes-oracle/<D>" crates/rustty-extra/themes
```

Expected: only `Only in crates/rustty-extra/themes: README.md` / `LICENSE.md` lines (plus any `.md` the tarball itself carries, absent from our vendored set — acceptable, `.md` is excluded from emission on both sides). Every theme file byte-identical. If the download fails (offline), skip — the corpus is already pinned by provenance (`crates/rustty-extra/themes/README.md`) and gated by `emits_full_theme_corpus`.

- [ ] **Step 8: No commit (nothing tracked changed); record results in the task report**

---

### Task 5: App rebuild + runtime verification (headless) + both-platform gate

**Files:**
- No source changes (verification only).

**Interfaces:**
- Consumes: Task 4's regenerated `zig-out/share/`.
- Produces: the rebuilt `Rustty.app` whose bundle carries the trees; evidence for Task 6's ledger strikes.

- [ ] **Step 1: Rebuild the app (this re-runs `xtask resources` internally)**

```bash
cd /Users/gael/projects/rustty
bash macos/build-rustty.sh
```

Expected: `xcodebuild ... BUILD SUCCEEDED`.

- [ ] **Step 2: Verify the bundle carries the trees and no placeholder**

```bash
APP=$(ls -td ~/Library/Developer/Xcode/DerivedData/Rustty-*/Build/Products/Debug/Rustty.app 2>/dev/null | head -1)
echo "APP=$APP"
THEMES_DIR=$(find "$APP/Contents/Resources" -type d -name themes | head -1)
ls "$THEMES_DIR" | wc -l
find "$APP/Contents/Resources" -name 'rustty-integration' -o -name 'rustty.bash' -o -name 'rustty.nu' | sort
find "$APP" -name 'DEFERRALS.md'
```

Expected: theme count `534`; the three named scripts each found once under a `shell-integration/` dir; the `DEFERRALS.md` find prints NOTHING.

- [ ] **Step 3: Headless shell-integration injection smoke (zsh — pre-verified working during WS1 research)**

Rustty's zsh injection sets `ZDOTDIR` to the bundled `shell-integration/zsh` dir, whose `.zshenv` sources `rustty-integration`, which registers `_rustty_deferred_init` in `precmd_functions`. Emulate exactly that against the freshly emitted tree:

```bash
RUSTTY_RESOURCES_DIR=$PWD/zig-out/share/rustty RUSTTY_SHELL_FEATURES="cursor,title" \
ZDOTDIR=$PWD/zig-out/share/rustty/shell-integration/zsh \
zsh -i -c 'typeset -f _rustty_deferred_init >/dev/null && echo INJECTED; print -r -- $precmd_functions' 2>&1 | tail -2
```

Expected: line 1 `INJECTED`; line 2 contains `_rustty_deferred_init`.

- [ ] **Step 4: Termio env-injection unit lane (the code path the app actually runs)**

```bash
cargo test -p rustty-termio shell_integration
cargo test -p rustty-cli list_themes
```

Expected: all tests pass (these cover `RUSTTY_BASH_INJECT`/`XDG_DATA_DIRS`/`ZDOTDIR` env setup and theme enumeration/dedup respectively).

Note (explicit, do not silently drop): the roadmap's "`+list-themes` CLI output count = 534" cannot run headlessly yet — `rustty_cli_try_action`'s non-null arm is `unreachable!` until WS4 wires `GlobalState<Action>`/`detectArgs` (`crates/rustty-apprt-c/src/lib.rs:259`). The count is verified here via Step 2 (bundle = 534) + `emits_full_theme_corpus` + the `list_themes` unit lane; the live `Rustty.app/Contents/MacOS/Rustty +list-themes` check is RE-QUEUED into WS4's verification (add it there when authoring WS4). Live OSC7/prompt-marking inside a running Rustty.app window is a GUI observation — do it opportunistically if a window is opened (launch the app, run `cd /tmp`, confirm the titlebar/pwd updates), but the headless Steps 3-4 are the gate.

- [ ] **Step 5: Both-platform build gate (roadmap standing rule)**

```bash
cargo build --workspace
ssh rvm 'cd ~/projects/rustty && git fetch origin && git checkout ws1-shell-integration-themes && cargo build --workspace'
```

(Push the branch first if `rvm` fetches from origin: `git push -u origin ws1-shell-integration-themes`.)

Expected: both builds green.

- [ ] **Step 6: No commit (verification only); record evidence for Task 6**

---

### Task 6: Ledger strikes + merge

**Files:**
- Modify: `.port/phase-2/INTEGRATION_PARITY.md` (verdict ¶ lines 22-28; §2.1 lines 38-52; §2.2 lines 54-63; Fix List items 1-2 lines ~297-304)
- Modify: `.port/phase-2/DEFERRALS.md` (append at end, line 4803+)

**Interfaces:**
- Consumes: Tasks 1-5 evidence (KAT green, 534 bundle count, injection smoke, no placeholder).
- Produces: truthful ledgers; WS1 merged to main.

- [ ] **Step 1: Strike the verdict sentence in INTEGRATION_PARITY.md**

Replace (in the §1 Verdict paragraph):

```markdown
behavior: the shipped xcframework is not self-contained (patched at app-link
with system `-lz` / Homebrew `-lintl`), and the `share/ghostty/shell-integration`
and `share/ghostty/themes` trees are stubbed empty, shipping those features
dark. All are documented R5 deferrals — sanctioned and non-silent — but the
runtime behaviors they gate are genuinely absent versus a stock ghostty build.
```

with:

```markdown
behavior: the shipped xcframework is not self-contained (patched at app-link
with system `-lz` / Homebrew `-lintl`). (The `share/{app}/shell-integration`
and `share/{app}/themes` trees were RESOLVED by WS1 [2026-07-23]: vendored in
`crates/rustty-extra/`, emitted by `xtask resources`, oracle-parity KAT'd —
see §2.1/§2.2.) The remaining divergence is a documented R5 deferral —
sanctioned and non-silent — but the packaging behavior it gates is genuinely
absent versus a stock ghostty build.
```

- [ ] **Step 2: Strike §2.1 and §2.2**

Change the §2.1 heading to:

```markdown
### 2.1 [RESOLVED 2026-07-23, WS1] `share/{app}/shell-integration/` — now vendored + emitted (was: stubbed empty)
```

and append below the §2.1 body (keep the historical body for the record):

```markdown
- **RESOLUTION (WS1, 2026-07-23)**: vendored at `crates/rustty-extra/shell-integration/`
  (R2-rebranded: `rustty.bash`, `rustty-integration`, `RUSTTY_*` env vars —
  matching `rustty-termio`'s `env_name()`/`APP_NAME` lookups), emitted to
  `share/{APP}/shell-integration/` by `xtask/src/res_shell_integration.rs`
  excluding `.md` (Tasks 13/14); WS1 added the dev-only oracle-parity KAT
  (`oracle_shell_integration_parity`, rebrand-mapped byte-compare, `bash-preexec.sh`
  verbatim), deleted the `DEFERRALS.md` placeholder emission, and verified the
  headless zsh injection smoke + bundle contents.
```

Change the §2.2 heading to:

```markdown
### 2.2 [RESOLVED 2026-07-23, WS1] `share/{app}/themes/` — now vendored + emitted (was: not shipped)
```

and append below the §2.2 body:

```markdown
- **RESOLUTION (WS1, 2026-07-23)**: the 534-file corpus is vendored verbatim at
  `crates/rustty-extra/themes/` (provenance: ghostty's pinned `iterm2_themes`
  tarball `ghostty-themes-release-20260608-160426-8c84dd1.tgz`; MIT attribution
  vendored in `LICENSE.md`, excluded from emission), emitted to
  `share/{APP}/themes/` by `xtask/src/res_themes.rs` excluding `.md` (Task 14);
  WS1 verified count=534 in the rebuilt app bundle and the regeneration is
  byte-reproducible from a clean `zig-out`. Live `+list-themes` CLI count check
  re-queued to WS4 (blocked on `rustty_cli_try_action`).
```

- [ ] **Step 3: Strike Fix List items 1-2**

In the `## 5. Fix List` section, replace items 1 and 2:

```markdown
1. **Ship `share/ghostty/shell-integration/`** (§2.1, HIGH) — vendor ghostty's
   `src/shell-integration/` tree; copy into the resource root in
   `gen_ghostty_root`. Restores OSC7/cwd, prompt marking, cursor-shape, sudo
   wrapping, title updates.
2. **Ship `share/ghostty/themes/`** (§2.2, HIGH) — vendor the iTerm2-Color-Schemes
   corpus; emit to `share/ghostty/themes`. Makes `list_themes` + the `theme`
   setting functional.
```

with:

```markdown
1. ~~**Ship `share/ghostty/shell-integration/`** (§2.1, HIGH)~~ — ✅ DONE
   (WS1, 2026-07-23; see §2.1 RESOLUTION).
2. ~~**Ship `share/ghostty/themes/`** (§2.2, HIGH)~~ — ✅ DONE
   (WS1, 2026-07-23; see §2.2 RESOLUTION).
```

- [ ] **Step 4: Append the resolution entry to `.port/phase-2/DEFERRALS.md`**

Append at end of file:

```markdown

### `share/{APP}` resource root — shell-integration + themes SHIPPED ✅ RESOLVED [2026-07-23, WS1]
The R5 placeholder (`share/{APP}/DEFERRALS.md`, written by
`xtask/src/res_terminfo.rs::gen_app_root`) that stood in for the
shell-integration and themes subtrees is DELETED — both trees ship real
vendored content: `crates/rustty-extra/shell-integration/` (7 scripts,
R2-rebranded to `RUSTTY_*`/`rustty-*`, matching `rustty-termio`'s
`env_name()`/`APP_NAME` runtime lookups; oracle-parity KAT
`xtask res_shell_integration::tests::oracle_shell_integration_parity`,
dev-only `#[ignore]`, rebrand-mapped byte-compare with `bash-preexec.sh`
verbatim) and `crates/rustty-extra/themes/` (534 files, verbatim from ghostty's
pinned `iterm2_themes` tarball `ghostty-themes-release-20260608-160426-8c84dd1.tgz`;
MIT attribution vendored at `themes/LICENSE.md`, `.md`-excluded from emission
so the shipped count stays exactly 534). `gen_app_root` now scrubs stale
placeholder notes from old output dirs. Verified: clean-regen byte-compare,
rebuilt `Rustty.app` bundle carries 534 themes + all 7 scripts + zero
`DEFERRALS.md`; headless zsh injection smoke green
(`ZDOTDIR=<share>/shell-integration/zsh zsh -i` registers
`_rustty_deferred_init` in `precmd_functions`); `identity-oracle` profile
emits the `share/ghostty/` parity dir. Residual: live `+list-themes` CLI
count + in-window OSC7 observation re-queued to WS4 (blocked on
`rustty_cli_try_action`'s non-null arm). Ledger: INTEGRATION_PARITY.md
§2.1/§2.2 struck RESOLVED same-commit.
```

- [ ] **Step 5: Commit the ledgers**

```bash
git add .port/phase-2/INTEGRATION_PARITY.md .port/phase-2/DEFERRALS.md
git commit -m "ledger: strike shell-integration + themes resource deferrals as RESOLVED (WS1 T6)"
```

- [ ] **Step 6: Final gates + merge to main**

```bash
cargo test -p xtask
cargo test -p xtask -- --ignored oracle_shell_integration_parity
cargo build --workspace
ssh rvm 'cd ~/projects/rustty && git fetch origin && git checkout ws1-shell-integration-themes && git pull && cargo build --workspace'
git checkout main
git merge --no-ff ws1-shell-integration-themes -m "WS1: shell-integration + themes resources closed out (KAT, attribution, placeholder deletion, ledgers)"
git branch -d ws1-shell-integration-themes
git push origin main :ws1-shell-integration-themes
```

Expected: all green; merged; branch deleted. Tick the WS1 box in `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`'s plan-authoring checklist is already done by authoring; execution status is tracked by this plan's checkboxes.

---

## Self-Review Notes (authored 2026-07-23)

- **Spec deviation, deliberate:** the workstream brief assumed vendoring + emission were still to be built; research proved they landed on main (commits `bc3405f`, `2480b4f`, `9a5d616`; `git status` clean; vendored trees tracked). Re-planning that work would be a no-op; this plan covers the verified residue and makes the roadmap's actual gates (reproducibility, parity KAT, 534 count, injection, ledgers) explicit.
- **Naming verified, not assumed:** `rustty-termio/src/shell_integration.rs:466,1729` builds `shell-integration/bash/{APP_NAME}.bash`; `env_name()` (line 788) prefixes `RUSTTY` via `rustty_identity::ENV_PREFIX`; `theme.rs` `Location::Resources` uses `resources_dir_app()`. All match the vendored spellings.
- **Rebrand-diff hypothesis tested live:** all 6 first-party scripts MATCH the oracle under the 3-token mapping; `bash-preexec.sh` matches verbatim (single divergence line is the upstream `# GHOSTTY:` annotation). The zsh smoke command in Task 5 Step 3 was executed successfully during research (`INJECTED`).
- **534 vs 535:** `crates/rustty-extra/themes/` holds 535 entries (534 themes + `README.md`); `LICENSE.md` makes 536 — both `.md`, both excluded, emitted count stays 534 (gated by `emits_full_theme_corpus`).
