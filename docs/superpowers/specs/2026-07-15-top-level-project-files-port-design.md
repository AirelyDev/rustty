# Top-Level Project Files Port — Design Spec

**Date:** 2026-07-15
**Status:** Approved for planning (awaiting spec review)
**Oracle:** `/Users/gael/projects/ghostty` @ 4749c4e (READ-ONLY)

## Goal

Port ghostty's top-level project files — the developer/project docs, the build
& packaging machinery, the CI/dot-dir infrastructure, and the referenced-but-
missing subsystems (i18n, source-tarball dist) — onto rustty, keeping the same
**structure** modulo Zig→Rust differences, and **filling** the infrastructure
gaps rather than pointing at ghostty's.

## Governing fidelity contract (READ FIRST)

These files are **NOT R1 byte-parity code**. Code is translated line-for-line
and gated byte-for-byte against the oracle. These are project/dev/build
artifacts, and the contract is different:

> **Structure-faithful, reality-accurate.** Keep the same file set, section
> skeletons, and headings. Rebrand per-token (oracle-arbitrated, per the
> `rename-casing-fidelity` discipline: `Ghostty`→`Rustty`, `ghostty`→`rustty`,
> paths/commands/identifiers lowercase). Translate every build command
> Zig→Cargo/xtask. Where a section describes infrastructure rustty genuinely
> has (crash reporter, C lib, examples), rebrand it accurately. Where it
> describes infrastructure rustty **lacks**, either **fill** it (this spec's
> gap-fill batches) or, for genuinely-external upstream services, document as a
> maintainer-provided placeholder — never fabricate.

Gates must not treat these files as parity artifacts. A residual-`ghostty` gate
over them follows the existing PARITY_NAMING carve-out rules (oracle
paths/provenance refs may remain; product strings must rebrand).

## Global Constraints

Copied verbatim into every batch's task requirements.

- **No commits** until the user reviews. Every batch produces reviewable
  working-tree changes; the executor stages nothing and commits nothing.
- **Rebrand map** (the product side; oracle side never changes):
  - `Ghostty`→`Rustty`, `ghostty`→`rustty`, `GHOSTTY_`→`RUSTTY_`
  - App bundle id / gettext domain: `com.mitchellh.ghostty` → `io.airely.rustty`
  - `TERM`: `xterm-ghostty` → `xterm-rustty`; env prefix `GHOSTTY_`→`RUSTTY_`
  - Config dir `~/.config/ghostty` → `~/.config/rustty`
  - C lib `libghostty`/`ghostty-vt` → `librustty`/`rustty-vt`; headers
    `include/ghostty/` → `include/rustty/`
  - Kit/xcframework `GhosttyKit` → `RusttyKit`
  - URLs `ghostty.org` → `rustty.sh`; repo refs → rustty's repo
  - Crash extension `.ghosttycrash` → `.rusttycrash`
- **identity-oracle parity:** any generated OUTPUT (terminfo, resource tree,
  domain, tarball name) that must byte-match the oracle under the
  `identity-oracle` feature keeps the existing dual-value mechanism
  (xtask `APP` const, `rustty-identity` flip). Product default = rustty.
- **Oracle-arbitrated casing:** when unsure of a token's case, diff against the
  ghostty original and preserve per-token case (memory `rename-casing-fidelity`).
- **External artifacts** (never fabricated; documented as maintainer-provided):
  minisign release keypair, Sentry crash DSN, Discord, hosted Doxygen site,
  GitHub `@rustty-org/*` teams, cachix substituter, tip-tarball secrets.
- **Build structure is already established:** Cargo workspace + `xtask/` (the
  `src/build/` analog). Do NOT reinvent it; the meta files document THIS
  structure. New machinery (Doxyfile, CMakeLists, Nix, dist) plugs into it.

## Architecture: five batches

Each batch is independently reviewable and shippable. Sequenced so earlier
batches unblock later ones (Batch 2's CMake unblocks the `c-vt-cmake` examples;
Batch 5's `xtask dist` is referenced by Batch 5's release workflows).

---

### Batch 1 — Prose + agent docs + dotfiles + governance

Pure documentation and config; no external deps; no code.

**Top-level prose docs** (create at repo root):

| Source            | Dest              | Approach                                                                                                                                                                                                                                                                                                               |
| ----------------- | ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `AGENTS.md`       | `AGENTS.md`       | Rewrite command table to Cargo/xtask; rustty dir+crate structure; drop the "sad dumb AI" issue/PR gag or keep rebranded (minimal).                                                                                                                                                                                     |
| `README.md`       | `README.md`       | Rustty-as-full-fidelity-port framing; keep the roadmap-table structure but reflect port status; `libghostty`→`librustty`; links → rustty.sh / rustty repo; keep About/Download/Docs/Crash-Reports sections rebranded.                                                                                                  |
| `HACKING.md`      | `HACKING.md`      | Build-steps table → `cargo build` / `cargo test` / `cargo run -p rustty` / xtask; logging (`RUSTTY_LOG`); linting (rustfmt/clippy + prettier/shellcheck/swiftlint/alejandra via the Nix devShell from Batch 4); Miri/ASan note; translations pointer → Batch 5; drop the zig-cache-hash section (crane eliminates it). |
| `CONTRIBUTING.md` | `CONTRIBUTING.md` | Keep "understand your code" + AI-policy pointers; **minimal governance** (trim vouch/denounce/Discord to a short pointer at upstream ghostty governance, since rustty is a solo port); keep issue/PR-are-actionable structure rebranded.                                                                               |
| `AI_POLICY.md`    | `AI_POLICY.md`    | Rebrand; near-verbatim (policy is portable).                                                                                                                                                                                                                                                                           |
| `PACKAGING.md`    | `PACKAGING.md`    | Zig-version section → Rust toolchain (`rust-toolchain.toml`); build-for-packages recipe → `cargo build --release` + xtask install; tarball URLs → rustty release infra (Batch 5) with minisign-key placeholder.                                                                                                        |
| `CODEOWNERS`      | `CODEOWNERS`      | **Minimal rewrite**: a crate-subsystem map (font/renderer/terminal/shell/macos → crate paths) owned by the single maintainer; trim the BDFL/maintainer-model preamble to reflect a solo port.                                                                                                                          |

**Dotfiles** (copy + light rebrand):
`.clang-format`, `.editorconfig`, `.shellcheckrc`, `.prettierignore`
(→ rustty image paths), `.swiftlint.yml` (points at `macos/`), `typos.toml`
(adjust excludes for Rust/`crates-sys/*/vendor`), `.mailmap` (optional),
`valgrind.supp` (copy; Rust suppressions additive). `.gitignore`/
`.gitattributes` already exist — leave.

**Nested AGENTS.md:**

| Source                             | Dest                                                                                                                                                                                                                                                                              |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `example/AGENTS.md`                | `example/AGENTS.md` — adapt: new-example flow uses `build.sh`+cargo (per the established `c-vt-*` pattern), not `build.zig.zon`; Doxygen `@snippet` conventions kept.                                                                                                             |
| `src/benchmark/AGENTS.md`          | `crates/rustty-bench/AGENTS.md` — methodology verbatim; commands → `cargo run -p rustty-bench` / `rustty-synthetic`; `hyperfine`, fixed seeds, no-parallel.                                                                                                                       |
| `src/inspector/AGENTS.md`          | `crates/rustty-inspector/AGENTS.md` — dcimgui refs → the `dcimgui-sys` crate; "no unit tests" framing kept.                                                                                                                                                                       |
| `src/terminal/apc/glyph/AGENTS.md` | `crates/rustty-vt/src/apc/glyph/AGENTS.md` — near-verbatim (Rio spec pointer + local-summary rule).                                                                                                                                                                               |
| `src/terminal/c/AGENTS.md`         | `crates/rustty-vt-c/AGENTS.md` — rewrite the 4-step `@export` chain to the cbindgen reality (`rustty-vt-c` module → cbindgen → `include/rustty/vt/`); ABI principles kept.                                                                                                        |
| —                                  | Fix `macos/AGENTS.md`: the stale `zig build -Demit-macos-app=false` line → `cargo run -p xtask -- xcframework` (rebuild the lib artifact), then `macos/build.nu`/xcodebuild builds the app; note `dist/RusttyKit.xcframework` path (ghostty used `macos/GhosttyKit.xcframework`). |
| `test/fuzz-libghostty/AGENTS.md`   | **No target** (no rustty fuzz harness). Note as future; not ported.                                                                                                                                                                                                               |

**Mapped nested READMEs** (target must exist; skip unclear):
`src/cli/README.md`→`crates/rustty-cli/README.md`;
`src/font/res/README.md` & `src/font/sprite/draw/README.md`→`crates/rustty-font/res|src/...`;
`pkg/README.md`→`crates-sys/README.md`;
`test/README.md`→(map if a rustty test-harness dir exists, else skip);
`dist/cmake/README.md`→Batch 2; `po/README_*`→Batch 5.
Already present (leave): `crates/rustty-extra/shell-integration/README.md`,
`crates/rustty-extra/themes/README.md`.

---

### Batch 2 — C-API build tooling

Unblocks C/CMake consumers of the VT lib and the deferred `c-vt-cmake*` examples.

| Source                                  | Dest                                   | Approach                                                                                                                                                                                                                                                                                                                                                                                                           |
| --------------------------------------- | -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `Doxyfile`                              | `Doxyfile`                             | `PROJECT_NAME="librustty"`; `INPUT=include/rustty`; output path rebranded; CSS from `dist/doxygen/`.                                                                                                                                                                                                                                                                                                               |
| `DoxygenLayout.xml`                     | `DoxygenLayout.xml`                    | Copy (nav layout).                                                                                                                                                                                                                                                                                                                                                                                                 |
| `dist/doxygen/*`                        | `dist/doxygen/*`                       | Copy + rebrand (favicon/header/footer/css/js).                                                                                                                                                                                                                                                                                                                                                                     |
| `CMakeLists.txt`                        | `CMakeLists.txt`                       | Rewrite the Zig-delegation as **cargo-delegation**: `project(rustty-vt …)`, shell out to `cargo build -p rustty-vt-c --release [--features simd]`, expose `rustty-vt`/`rustty-vt-static` IMPORTED targets over `target/release/librustty_vt_c.{dylib,a}` + cbindgen headers `include/rustty/`, install rules, FetchContent + find_package, and a `rustty_vt_add_target()` cross-compile helper (cargo `--target`). |
| `dist/cmake/GhosttyZigCompiler.cmake`   | `dist/cmake/RusttyCargoCompiler.cmake` | The cargo-invocation helper (analog of the zig-compiler cmake).                                                                                                                                                                                                                                                                                                                                                    |
| `dist/cmake/ghostty-vt-config.cmake.in` | `dist/cmake/rustty-vt-config.cmake.in` | find_package config template.                                                                                                                                                                                                                                                                                                                                                                                      |
| `dist/cmake/README.md`                  | `dist/cmake/README.md`                 | Rebrand + cargo delegation.                                                                                                                                                                                                                                                                                                                                                                                        |
| `Makefile`                              | `Makefile`                             | Trivial: `clean`→`cargo clean` + rm `dist/RusttyKit.xcframework`; keep an `init` hint. Drop the `glad` target (OpenGL loader) unless/until the GL renderer needs it.                                                                                                                                                                                                                                               |
| `example/c-vt-cmake*` (3 dirs)          | `example/c-vt-cmake{,-static,-cross}/` | Port the 3 CMake examples (symbol-swap + rustty CMake wiring), enabled by the CMakeLists above.                                                                                                                                                                                                                                                                                                                    |

---

### Batch 3 — dist/ packaging assets (Linux + Windows + macOS)

Language-neutral packaging assets; copy + rebrand (`com.mitchellh.ghostty`→
`io.airely.rustty`).

- `dist/linux/*`: `app.desktop.in`, `*.metainfo.xml.in`, dbus service `.in`
  files, systemd service, Nautilus/Dolphin file-manager integration
  (`ghostty_nautilus.py`→rustty), icons.
- `dist/windows/*`: `ghostty.ico`→`rustty.ico`, `.manifest`, `.rc`.
- `dist/macos/*`: Sparkle appcast updaters (`update_appcast_tag.py`,
  `update_appcast_tip.py`) — rebrand; appcast URLs → rustty release infra
  (placeholder host).

Note: rustty is macOS-first; the Linux desktop/dbus/systemd assets are inert
until the GTK app is live but are copy-and-rebrand and belong with the port.

---

### Batch 4 — Nix (crane-based flake, full, incl. 16 VMs)

**Builder: crane.** Cargo.lock-native FOD vendoring; the real `cargo build`
runs in-sandbox so every `build.rs` / `-sys` C-compile / cbindgen step works.
No drift-prone hash file → **`build.zig.zon.{json,nix,txt}` and
`nix/zigCacheHash.nix` + `check-zig-cache.sh` have NO analog and are dropped.**

Files (from-scratch, NOT a line-port):

- `flake.nix` — inputs: `nixpkgs`, `crane`, `fenix`/`rust-overlay` (pin
  `rust-toolchain.toml`'s channel + rustfmt/clippy/miri), `flake-utils`.
  Outputs mirror ghostty:
  - `packages.rustty-{debug,releasesafe,releasefast}` (cargo profile map)
  - `packages.librustty-vt-{native,no-simd,…}` (toggle the `simd` cargo feature)
  - `devShells.default` — toolchain + `cbindgen cmake pkg-config prettier
alejandra shellcheck swiftlint gettext doxygen`
  - `overlays.default` (consumed by the VMs); `checks` (fmt/clippy/test)
- `default.nix` / `shell.nix` — flake-compat shims.
- `.envrc` — `use flake`.
- `nix/`:
  - `devShell.nix`, `package.nix`, `librustty-vt.nix` (crane derivations)
  - `tests.nix` (NixOS VM integration tests)
  - `build-support/` — mirror-update / path helpers as needed (the
    zig-cache scripts are NOT ported)
  - `pkgs/` — the test-only deps ghostty vendors (blessed, ucs-detect,
    wcwidth, wraptest) if the VM tests reference them
  - `vm/` — **all 16 NixOS VM configs** (wayland/x11 × gnome/plasma/cinnamon/
    xfce, etc.), rebranded, consuming `overlays.default`. **In-file note:**
    inert until the GTK Linux app is live.

**Boundary:** the macOS `.app` stays OUT of Nix (xcodebuild isn't hermetic) —
same as ghostty. Nix covers CLI + C lib + future GTK app + dev shell.

---

### Batch 5 — `.github/` + dot-dirs + gap-fill engineering

**`.github/` — port + rebrand (to cargo/xtask/nix), keep, or drop:**

- KEEP: existing `ci.yml` (the real gate mirroring `.port/tools/ci.sh`).
- PORT+REBRAND: `release-tag.yml`, `release-tip.yml`, `publish-tag.yml`
  (→ `xtask dist` + minisign-placeholder), `nix.yml` (→ crane flake checks),
  `flatpak.yml`, `snap.yml` (→ Batch 3 assets), `update-colorschemes.yml`,
  `milestone.yml`, `clean-artifacts.yml`; `dependabot.yml` (→ cargo ecosystem);
  `pinact.yml`; `test.yml` example-build jobs (zig→cargo, cmake, swift) folded
  into a rustty examples workflow; `scripts/check-translations.sh` (→ xtask),
  `scripts/ghostty-tip`→`rustty-tip`.
- DROP (minimal-governance): `vouch-*.yml` (5 workflows), `VOUCHED.td`,
  `issue-unvouched-message`, `DISCUSSION_TEMPLATE/vouch-request.yml`.
- MINIMAL REWRITE: `ISSUE_TEMPLATE/*`, `DISCUSSION_TEMPLATE/issue-triage.yml`,
  PR template.

**Other dot-dirs:**

- `.agents/` — `commands/review-branch`, `skills/writing-commit-messages/`:
  portable; adapt (cargo/rustty references).
- `.claude/` — reconcile with rustty's existing `.claude/`; adapt ghostty
  commands/settings that are useful, don't clobber rustty's.

**Gap-fill engineering (real code):**

1. **`xtask dist` + `distcheck`** — `git archive --format=tgz` honoring
   `.gitignore` + `export-ignore` attrs, `--add-file` for build-generated files
   a checkout regenerates, `VERSION` file + `rustty-<version>/` prefix.
   `distcheck` extracts + runs `cargo test` (+ the CMake path for the vt lib).
   Add `export-ignore` `.gitattributes` entries for dev-only paths (`.port/`,
   `docs/superpowers/`, etc.).

2. **Full i18n** (infra + all 33 catalogs):
   - **`_()` / `C_()`** domain-qualified gettext wrapper (over `gettextrs`),
     domain = `io.airely.rustty`, domain-scoped (`dgettext`, NOT global
     `textdomain`) so the lib stays embeddable. Ordered locale list (port
     `i18n_locales`). macOS `libintl` + zh Hans/Hant canonicalization + FreeBSD
     locale-dir quirks.
   - **`xtask update-translations`** — `xgettext` (keywords `_`, `C_:1c,2`)
     over rustty sources (+ any blueprint/py inputs) → `po/io.airely.rustty.pot`,
     then `msgmerge --no-fuzzy-matching` per locale.
   - **`.po` catalogs** — port all 33 from `po/*.po` (rebrand `.pot` header +
     msgids/locations; translations preserved). `po/README_TRANSLATORS.md` +
     `po/README_CONTRIBUTORS.md` rebranded.
   - **Install/runtime** — `xtask` compiles each `.po` via `msgfmt` to
     `share/locale/<loc>/LC_MESSAGES/io.airely.rustty.mo`; startup
     `bindtextdomain(domain, "<share>/locale")`.

3. **Crash + signing docs/stubs** — README/PACKAGING crash section rebranded
   (`.rusttycrash`, `~/.local/state/rustty/crash`, `rustty +crash-report`);
   Sentry DSN + minisign keypair = maintainer-provided placeholders wired into
   the release workflow but not fabricated.

## External-artifact policy (applies to all batches)

These are referenced but **cannot be fabricated**; each becomes a clearly
labeled maintainer-provided placeholder (env/secret) with a doc note, never an
invented value:
minisign release keypair, Sentry crash DSN, Discord invite, hosted Doxygen
site URL, GitHub `@rustty-org/*` teams, cachix substituter, tip-tarball CI
secrets.

## Testing / verification per batch

- **B1** docs: `prettier --check` (once Nix devShell lands, else skip);
  residual-`ghostty` audit over the new files (PARITY_NAMING carve-outs);
  human review of prose.
- **B2**: `doxygen Doxyfile` builds clean; `cmake -B build && cmake --build`
  builds the vt lib; the 3 `c-vt-cmake*` examples build+run at byte-parity
  (`.port/tools/cvt-parity.sh`).
- **B3**: desktop-file/metainfo validate (`desktop-file-validate`,
  `appstreamcli validate`) where tooling available; icons render.
- **B4**: `nix flake check`; `nix build .#librustty-vt-*`; `nix develop`
  enters the shell; VM configs evaluate (`nix eval`), even if inert.
- **B5**: `cargo run -p xtask -- dist` produces a valid tarball; `distcheck`
  passes; `xtask update-translations` regenerates the `.pot` deterministically;
  a smoke locale renders a translated string; workflows lint (`actionlint`).

## Execution model

Sequential batches; each gets its own bite-sized implementation plan
(writing-plans) and executes via subagents/workflow. **No commits** — the user
reviews each batch's working-tree diff before anything is staged.

## Out of scope

- The `test/fuzz-libghostty` AGENTS.md (no rustty fuzz harness yet).
- Actually generating external secrets/keys/accounts.
- Making the GTK Linux app live (the VM configs and Linux dist assets are
  ported but inert until then).
