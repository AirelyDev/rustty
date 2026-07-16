# Batch 1 — Prose + Agent Docs + Dotfiles + Governance — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port ghostty's top-level prose/dev docs, applicable dotfiles, nested AGENTS.md files, and mapped nested READMEs onto rustty — structure-faithful, reality-accurate, rebranded, with build commands translated Zig→Cargo/xtask.

**Architecture:** Pure documentation + config. Each file is derived from its ghostty oracle counterpart by (a) per-token rebrand, (b) Zig→Cargo command translation, (c) enumerated section-level adaptations for infrastructure that differs. No source code changes. No external dependencies.

**Tech Stack:** Markdown, INI/YAML/TOML config, plain text.

## Global Constraints

- **Oracle (READ-ONLY):** `/Users/gael/projects/ghostty` @ 4749c4e. Read the counterpart file there; never modify the oracle.
- **NO commits.** Leave all changes in the working tree for user review. Do not `git add`/`git commit`.
- **Fidelity contract:** structure-faithful, reality-accurate (NOT R1 byte-parity). Keep the same file set, section skeletons, and headings. Rebrand per-token, oracle-arbitrated (preserve per-token case by diffing against the original).
- **Rebrand map** (product side only; oracle-provenance refs stay `ghostty`):
  - `Ghostty`→`Rustty`, `ghostty`→`rustty`, `GHOSTTY_`→`RUSTTY_`
  - bundle id / gettext domain `com.mitchellh.ghostty` → `io.airely.rustty`
  - `TERM` `xterm-ghostty`→`xterm-rustty`; env prefix `GHOSTTY_`→`RUSTTY_`
  - config dir `~/.config/ghostty`→`~/.config/rustty`; state dir `~/.local/state/ghostty`→`~/.local/state/rustty`
  - C lib `libghostty`/`ghostty-vt`→`librustty`/`rustty-vt`; headers `include/ghostty/`→`include/rustty/`
  - `GhosttyKit`→`RusttyKit`; xcframework `macos/GhosttyKit.xcframework`→`dist/RusttyKit.xcframework`
  - URLs `ghostty.org`→`rustty.sh`; upstream repo citations may remain `github.com/ghostty-org/ghostty` (provenance)
  - crash ext `.ghosttycrash`→`.rusttycrash`
- **Command translation** (Zig→rustty):
  - `zig build`→`cargo build`; `zig build run`→`cargo run -p rustty`; `zig build test`→`cargo test`; `-Dtest-filter=X`→`cargo test <name>` / `-p <crate>`
  - `zig fmt .`→`cargo fmt`; add `cargo clippy`
  - libghostty-vt build → `cargo build -p rustty-vt-c --release [--features simd]`; wasm → `cargo build -p rustty-vt-c --release --target wasm32-unknown-unknown`
  - `zig build dist`→`cargo run -p xtask -- dist`; `update-translations`→`cargo run -p xtask -- update-translations`
  - macOS lib: `zig build -Demit-macos-app=false`→`cargo run -p xtask -- xcframework`
- **Allowed residual `ghostty`:** legitimate upstream/provenance references only (e.g. "full-fidelity port of Ghostty", "upstream ghostty", `github.com/ghostty-org`, oracle paths). Everything product-facing must rebrand.
- **REBRAND AUDIT (run after writing each file):**
  `grep -niE 'ghostty|com\.mitchellh|ghostty\.org|zig build|GhosttyKit' <file>` and confirm every remaining hit is a legitimate upstream/provenance reference. A stray product ref or stale `zig build` command is a failure.
- **Casing arbiter:** when unsure of a token's case, `diff` the section against the ghostty original and preserve per-token case (memory `rename-casing-fidelity`).

---

### Task 1: Top-level `AGENTS.md`

**Files:**
- Create: `AGENTS.md`
- Reference: `/Users/gael/projects/ghostty/AGENTS.md`

**Interfaces:**
- Produces: the canonical agent-guidance file other tools read. Later tasks' nested AGENTS.md defer to its command vocabulary.

- [ ] **Step 1: Read the oracle** `/Users/gael/projects/ghostty/AGENTS.md` (39 lines).

- [ ] **Step 2: Write `AGENTS.md`** applying:
  - `## Commands`: `zig build`→`cargo build`; drop the `-Demit-macos-app=false` speed tip (replace with: to skip the macOS app, just build the workspace/crate you need — `cargo build -p <crate>`). Test → `cargo test` (note `-p <crate>` and `cargo test <name>` filters; keep the "full suite is slow, prefer targeted" guidance). Formatting: `cargo fmt` + `cargo clippy`; keep `swiftlint lint --strict --fix` (Swift) and `prettier -w .` (other).
  - `## libghostty-vt` → `## librustty-vt`: build `cargo build -p rustty-vt-c --release`; wasm `--target wasm32-unknown-unknown --release`; test `cargo test -p rustty-vt-c` (+ Miri note). Keep the C-enum `_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` sentinel rule (rebrand the sentinel name), path `include/rustty/vt/`.
  - `## Directory Structure`: Shared core → the Cargo workspace `crates/` (name the crown-jewel crates: `rustty-vt`, `rustty-core`, `rustty-renderer`, …); macOS app `macos/`; GTK app `crates/rustty-apprt/…` (note: not yet live).
  - `## Issue and PR Guidelines`: keep "Never create an issue/PR" rebranded; keep or drop the gag line (keep it, rebranded, for fidelity).

- [ ] **Step 3: Rebrand audit** — run the Global-Constraints audit command on `AGENTS.md`; confirm no stray product refs, no `zig build`.

- [ ] **Step 4: Structure check** — the section headings match the oracle's set (Commands, librustty-vt, Directory Structure, Issue and PR Guidelines), reordered/renamed only as above.

---

### Task 2: `README.md`

**Files:**
- Create: `README.md`
- Reference: `/Users/gael/projects/ghostty/README.md` (226 lines)

- [ ] **Step 1: Read the oracle** README.

- [ ] **Step 2: Write `README.md`** applying:
  - Logo/title block → `Rustty` (drop the ghostty logo asset URL or replace with a rustty placeholder / text title; do NOT hotlink ghostty's asset). Tagline: reframe as "a full-fidelity Rust port of Ghostty — fast, native, feature-rich terminal emulator" with a one-line provenance credit to upstream Ghostty.
  - Nav links → `#about`, rustty.sh/download + docs (placeholder if not live), `CONTRIBUTING.md`, `HACKING.md`.
  - `## About`: rebrand; `libghostty`→`librustty`; keep the embeddable-library framing; link the `example/` dir; credit upstream Ghostty (`github.com/ghostty-org/ghostty`) as the source of the port.
  - `## Download` / `## Documentation`: point at rustty.sh (note "coming soon" if not live) — do not invent live URLs.
  - `## Roadmap and Status`: keep the 6-step table structure; reframe status to reflect the **port's** state (standards-compliant VT ✅ via the ported engine, etc.) — accurate, not copied verbatim.
  - Sub-sections (Standards-Compliant, Performance, Windowing, Native, librustty, Rustty-only sequences): rebrand; keep architecture prose (multi-threaded, SIMD parser, Metal/OpenGL) since it's ported faithfully; `libghostty-vt`→`librustty-vt`; drop the mitchellh.com blog link or keep as upstream provenance.
  - `## Crash Reports`: rebrand — `.rusttycrash`, `$XDG_STATE_HOME/rustty/crash` (default `~/.local/state`), `rustty +crash-report`. **The Sentry DSN is ghostty's** — replace with a maintainer-provided-DSN placeholder note (do NOT reuse ghostty's DSN).

- [ ] **Step 3: Rebrand audit** on `README.md`; confirm the Sentry DSN is a placeholder, not ghostty's value, and no hotlinked ghostty assets.

- [ ] **Step 4: Structure check** — section set matches oracle (About/Download/Documentation/Contributing/Roadmap/Crash Reports).

---

### Task 3: `HACKING.md`

**Files:**
- Create: `HACKING.md`
- Reference: `/Users/gael/projects/ghostty/HACKING.md` (495 lines)

- [ ] **Step 1: Read the oracle** HACKING.md.

- [ ] **Step 2: Write `HACKING.md`** applying:
  - Clone/build intro → rustty repo; debug build is `cargo build` (default debug).
  - Build-steps table → cargo/xtask: `cargo run -p rustty` (run), Miri/ASan via `.port/tools` gates (memory-leak analog — reference `check_leaks.sh`), `cargo test [-p <crate>]`, `cargo run -p xtask -- update-translations`, `cargo run -p xtask -- dist` / `distcheck`.
  - Extra Dependencies: Linux GTK deps (`blueprint-compiler`) — note GTK app not yet live; macOS none extra.
  - Xcode/SDK section: keep (macOS app reuses Xcode); drop the Zig-0.15/Xcode-26.4 linking-issue note (Zig-specific) OR reframe as an upstream note. Keep "Xcode 26 + macOS 26 SDK" if still accurate for the reused app.
  - AI and Agents: point at rustty `AGENTS.md`; `.agents/commands` (Batch 5).
  - Logging: `RUSTTY_LOG` env, `stderr`/`macos` destinations, subsystem `io.airely.rustty`.
  - Linting: Prettier / SwiftLint / ShellCheck / Alejandra — via the Nix dev shell (**Batch 4**; note "once the Nix flake lands" as a forward reference) plus non-Nix install instructions; add `cargo fmt`/`cargo clippy`.
  - **DROP** "Updating the Zig Cache Fixed-Output Derivation Hash" (crane eliminates it — see the spec's Batch-4 note).
  - Translations pointer → `po/README_CONTRIBUTORS.md` (Batch 5).
  - Memory-leak checking: reframe from Valgrind/`zig build run-valgrind` to rustty's Miri/ASan gates (`.port/tools/gates/check_leaks.sh`) + a Valgrind note for the FFI/C-dep paths.
  - Input-stack / IME testing: keep (portable manual test matrix), rebrand app name.
  - Nix VMs / VM integration tests: keep the structure, point at the **Batch 4** flake (`nix run .#<vmtype>`, `nix flake check`); rebrand.

- [ ] **Step 3: Rebrand audit**; confirm no `zig build`, no zig-cache-hash section.

- [ ] **Step 4: Structure check** vs oracle section set (minus the dropped zig-cache section).

---

### Task 4: `CONTRIBUTING.md` + `AI_POLICY.md`

**Files:**
- Create: `CONTRIBUTING.md`, `AI_POLICY.md`
- Reference: `/Users/gael/projects/ghostty/CONTRIBUTING.md` (195), `AI_POLICY.md` (65)

- [ ] **Step 1: Read both oracles.**

- [ ] **Step 2: Write `AI_POLICY.md`** — near-verbatim rebrand (policy is portable): `Ghostty`→`Rustty`; keep the disclosure/understand-your-code/no-AI-media/denounce rules and the "There are Humans / AI is Welcome" sections. Keep `m@mitchellh.com`? No — that's upstream contact; replace with the rustty maintainer contact placeholder or drop the address.

- [ ] **Step 3: Write `CONTRIBUTING.md`** — **minimal governance** per spec:
  - Keep: the "Critical Rule — understand your code", the AI-usage pointer to `AI_POLICY.md`, "Issues are Actionable", "Pull Requests Implement an Issue" (rebranded).
  - Trim to a short upstream pointer (rustty is a solo full-fidelity port; community governance lives upstream at ghostty): the vouch system, denouncement system, Discord server, GitHub discussion-category links, the "wall of text" personal note. Replace with 2–3 sentences: rustty is a port; for the terminal's design discussions see upstream Ghostty; open rustty issues/PRs that you fully understand.
  - Do NOT invent a rustty Discord / vouch list / discussion categories.

- [ ] **Step 4: Rebrand audit** on both files; confirm no live ghostty Discord/vouch/discussion URLs presented as rustty's.

---

### Task 5: `PACKAGING.md`

**Files:**
- Create: `PACKAGING.md`
- Reference: `/Users/gael/projects/ghostty/PACKAGING.md` (124)

- [ ] **Step 1: Read the oracle.**

- [ ] **Step 2: Write `PACKAGING.md`** applying:
  - Keep the "accurate only for this version" warning + source-tarball framing.
  - Source-tarball URLs → rustty release infra placeholder (do NOT reuse `release.files.ghostty.org` or ghostty's minisign public key — mark the minisign key as maintainer-provided).
  - Tip tarballs → rustty GitHub releases (placeholder if not set up).
  - "Zig Version" → "Rust Toolchain": the required toolchain lives in `rust-toolchain.toml` (channel pinned); no Zig.
  - Building for system packages: replace the `fetch-zig-cache.sh` + `zig build --prefix --system` recipe with the cargo equivalent — `cargo build -p rustty --release` (+ `--offline` with a vendored `cargo vendor` dir for the no-network step), then install via `cargo run -p xtask -- ...`/`DESTDIR`. Reference the Nix/crane path (Batch 4) as the reproducible option.
  - Build Options: map `--prefix`/`DESTDIR`/`-Doptimize=ReleaseFast`/`-Dcpu=baseline`/`-Dtarget` to cargo profiles + `--target` triples + `RUSTFLAGS` for baseline CPU.

- [ ] **Step 3: Rebrand audit**; confirm no ghostty minisign key / release URLs presented as rustty's.

---

### Task 6: `CODEOWNERS` (minimal rewrite)

**Files:**
- Create: `CODEOWNERS`
- Reference: `/Users/gael/projects/ghostty/CODEOWNERS` (201)

- [ ] **Step 1: Read the oracle** to see the subsystem structure.

- [ ] **Step 2: Write `CODEOWNERS`** as a minimal solo-port version:
  - Trim the long BDFL/maintainer-model preamble to a few lines noting rustty is a solo full-fidelity port; the subsystem map documents crate ownership.
  - Map the ghostty `src/` subsystem paths to rustty **crate paths** with the single maintainer as owner (use the repo's actual owner handle; if unknown, use a `# TODO: owner handle` placeholder comment rather than inventing a team):
    - Font → `/crates/rustty-font/ /crates/rustty-font-shaper/`
    - Renderer → `/crates/rustty-renderer/ /crates/rustty-render-types/`
    - Terminal → `/crates/rustty-vt/ /crates/rustty-vt-c/ /crates/rustty-input/ /crates/rustty-terminfo/ /crates/rustty-termio/ /crates/rustty-unicode/ /crates/rustty-simd/`
    - Shell → `/crates/rustty-extra/shell-integration/`
    - macOS → `/crates/rustty-apprt-c/ /crates/rustty-os/ /macos/ /include/rustty/`
  - **Drop** all `@ghostty-org/*` team handles and the per-locale `/po/*.po` team rows (localization ownership → a single note; the `.po` files land in Batch 5).

- [ ] **Step 3: Audit** — confirm no `@ghostty-org/*` handles remain and paths point at real rustty crate dirs (`ls` each path prefix).

---

### Task 7: Dotfiles

**Files:**
- Create: `.clang-format`, `.editorconfig`, `.shellcheckrc`, `.prettierignore`, `.swiftlint.yml`, `typos.toml`, `.mailmap`, `valgrind.supp`
- Reference: the same-named files under `/Users/gael/projects/ghostty/`
- Leave untouched: `.gitignore`, `.gitattributes` (already rustty-correct)

- [ ] **Step 1: Copy the straightforward ones verbatim:** `.clang-format`, `.editorconfig`, `.shellcheckrc`, `valgrind.supp` (Rust suppressions are additive — copy as-is).

- [ ] **Step 2: Copy + rebrand `.prettierignore`** — swap any `images/Ghostty.*` / ghostty paths for rustty equivalents; keep generic ignores (vendored, build outputs → `target/`).

- [ ] **Step 3: Copy + adjust `.swiftlint.yml`** — ensure it points at `macos/` (rustty's Swift app dir); rebrand any Ghostty references.

- [ ] **Step 4: Copy + adjust `typos.toml`** — change excludes from `src/`/`vendor/`/`*.zon` to rustty's tree: `crates-sys/*/vendor/`, `target/`, `.port/phase-0/**` goldens, fonts/images. Keep the tool's option structure.

- [ ] **Step 5: `.mailmap`** — either copy ghostty's (historical upstream authors, harmless) or start a minimal rustty one. Recommend: minimal rustty `.mailmap` (the port's own authorship); note upstream provenance is in `NOTICE`.

- [ ] **Step 6: Audit** each file with the rebrand audit; confirm `.swiftlint.yml` targets `macos/` and `typos.toml` excludes resolve to real rustty paths.

---

### Task 8: Nested `AGENTS.md` (6 files) + `macos/AGENTS.md` fix

**Files:**
- Create: `example/AGENTS.md`, `crates/rustty-bench/AGENTS.md`, `crates/rustty-inspector/AGENTS.md`, `crates/rustty-vt/src/apc/glyph/AGENTS.md`, `crates/rustty-vt-c/AGENTS.md`
- Modify: `macos/AGENTS.md`
- Reference: the counterpart ghostty `AGENTS.md` files (see spec Batch-1 table for the source→dest map)

- [ ] **Step 1: `example/AGENTS.md`** — read `/Users/gael/projects/ghostty/example/AGENTS.md`. Adapt the "add a new example" flow to rustty's established `c-vt-*` pattern: create `example/<name>/{src,build.sh,README.md}`, symbol-swap against `include/rustty/vt.h`, build via `build.sh` (clang + `librustty_vt_c`) — NOT `build.zig.zon`. Keep the Doxygen `@snippet` and naming conventions; `ghostty-vt`→`rustty-vt`.

- [ ] **Step 2: `crates/rustty-bench/AGENTS.md`** — read `/Users/gael/projects/ghostty/src/benchmark/AGENTS.md`. Keep the methodology verbatim (two roles gen/bench, don't pipe, reuse files, fixed seeds, `hyperfine`, medians, no parallel, branch-comparison). Commands → `cargo run -p rustty-bench` / `cargo run -p rustty-synthetic`; build `cargo build -p rustty-bench --release`.

- [ ] **Step 3: `crates/rustty-inspector/AGENTS.md`** — read `/Users/gael/projects/ghostty/src/inspector/AGENTS.md`. dcimgui header ref → the `dcimgui-sys` crate's vendored header; imgui demo pointer kept; "no unit tests in this package" framing kept; API-verify note → `cargo build -p rustty-inspector`.

- [ ] **Step 4: `crates/rustty-vt/src/apc/glyph/AGENTS.md`** — read `/Users/gael/projects/ghostty/src/terminal/apc/glyph/AGENTS.md`. Near-verbatim: Rio glyph-protocol spec pointer, the "summary at top of `glyph.rs` (was `glyph.zig`), prefer local spec over fetching" rule (adjust the file path to the rust module).

- [ ] **Step 5: `crates/rustty-vt-c/AGENTS.md`** — read `/Users/gael/projects/ghostty/src/terminal/c/AGENTS.md`. Rewrite the 4-step `@export` propagation chain to the **cbindgen** reality: add function in the `rustty-vt-c` module → re-export → cbindgen emits into `include/rustty/vt/` (via `cargo run -p xtask -- header-gen`) → keep header sorting + ABI patterns (opaque ptrs, padding, sized-struct `RUSTTY_INIT_SIZED`, `lib.TaggedUnion` → the rust tagged-union C projection). Keep ABI-compatibility principles.

- [ ] **Step 6: Fix `macos/AGENTS.md`** — read the existing rustty file; replace the stale `zig build -Demit-macos-app=false` line with: rebuild the underlying library artifact via `cargo run -p xtask -- xcframework` (produces `dist/RusttyKit.xcframework`), then build the app via `macos/build.nu`/xcodebuild. Verify no other `zig`/`ghostty` residue remains.

- [ ] **Step 7: Audit** all 6 files with the rebrand audit; confirm no `zig build`, `@export`, or `.zig-cache` references survive except as clearly-labeled provenance.

---

### Task 9: Mapped nested READMEs

**Files:**
- Create: `crates/rustty-cli/README.md`, `crates-sys/README.md`, plus font READMEs where a target exists (`crates/rustty-font/res/README.md` and/or the sprite-draw analog)
- Reference: `src/cli/README.md`, `pkg/README.md`, `src/font/res/README.md`, `src/font/sprite/draw/README.md` under the oracle
- Already present (leave): `crates/rustty-extra/shell-integration/README.md`, `crates/rustty-extra/themes/README.md`

- [ ] **Step 1: `crates/rustty-cli/README.md`** — read `/Users/gael/projects/ghostty/src/cli/README.md`; rebrand; adjust any build/run commands to cargo.

- [ ] **Step 2: `crates-sys/README.md`** — read `/Users/gael/projects/ghostty/pkg/README.md`; adapt: it documented the vendored C `pkg/` packages; rustty's analog is the `crates-sys/*` FFI crates (list them, note each vendors its C source under `vendor/`). Rebrand.

- [ ] **Step 3: Font READMEs** — read `/Users/gael/projects/ghostty/src/font/res/README.md` and `src/font/sprite/draw/README.md`; map to `crates/rustty-font/res/README.md` and the sprite-draw location if it exists under `crates/rustty-font/src/` (verify with `ls` first; skip a file whose target dir doesn't exist and note it).

- [ ] **Step 4: Audit** each with the rebrand audit; verify targets exist before writing.

---

## Self-Review (completed by author)

- **Spec coverage:** All Batch-1 items in the design spec map to a task (prose docs → T1–T5, CODEOWNERS → T6, dotfiles → T7, nested AGENTS + macos fix → T8, mapped READMEs → T9). The `test/fuzz-libghostty/AGENTS.md` is intentionally out of scope (no rustty fuzz harness) per the spec.
- **Placeholder scan:** External values (Sentry DSN, minisign key, release URLs, owner handle) are explicitly maintainer-provided placeholders, not invented — this is the spec's external-artifact policy, not a plan gap.
- **Consistency:** the rebrand map + audit command are defined once in Global Constraints and referenced by every task (DRY). Command translations are centralized there too.
- **Forward references:** HACKING/PACKAGING reference the Nix devShell (Batch 4) and `xtask dist` (Batch 5) — labeled as forward references, not blockers (the docs describe the target state).
