# Build-System Gap Audit — rustty vs. ghostty

**Date:** 2026-08-25
**Oracle (READ-ONLY):** `/Users/gael/projects/ghostty` @ `683d8db64` (the pinned
`.port/ORACLE_SYNC.md` sync point)
**Scope:** everything that _builds, packages, tests-in-CI, or ships_ rustty —
not product source. Companion to
`docs/superpowers/specs/2026-07-15-top-level-project-files-port-design.md`
(Batches 1–5) and `2026-07-15-batch4-nix-flake-design.md`.

---

## Status (updated 2026-08-26) — COMPLETE

Every step of the fill order below is done, on branch `build-system-gaps`
(merged to `main`). Seven new gates run from `.port/tools/ci.sh`:

| Gate          | Covers                                                                |
| ------------- | --------------------------------------------------------------------- |
| `G-LINT`      | shellcheck, prettier, typos, swiftlint, alejandra, pinact             |
| `G-CCONSUMER` | cmake build/install, both `.pc` files, shared+static link             |
| `G-EXAMPLES`  | all 33 `example/` programs, incl. cross and SwiftPM                   |
| `G-DIST`      | `xtask dist` + `dist-lib-vt` shape, + (opt-in) `distcheck`            |
| `G-NIX`       | `nix flake check` (7 checks) + `alejandra --check`; BLOCKED w/o `nix` |
| `G-XTARGET`   | cross-target builds of `rustty-vt-c` (wasm ×2, windows, freebsd)      |

**Nix is complete and verified.** `nix build .#librustty-vt`, `nix build
.#rustty` (the GTK app), `nix flake check` (all 7), `nix develop` (exactly the
`rust-toolchain.toml` pin), and all six `nix run .#<desktop>` VM launchers
evaluate on aarch64-linux.

**The flatpak is verified**: built, installed and run — `flatpak run
io.airely.rustty +version` → `Rustty 0.1.0`. flatpak-builder needs no root; it
ships as a flatpak.

### What remains genuinely blocked

Two things, both needing privileges or hardware this environment does not have.
Neither is a code gap, and both say so in their own headers:

- **snap** — snapcraft is itself a snap and snapd is absent; installing it
  needs root. The manifest names the exact commands.
- **`nix/tests.nix`** — NixOS VM tests boot a desktop under QEMU/KVM. The
  Linux box here is itself a VM and the container the flake was verified in
  has no `/dev/kvm`. Ported, not wired into `checks`, with the three steps to
  enable it written down.

Also not done: `test/fuzz-libghostty`. rustty has G5 (`run_fuzz_smoke.sh`) at
the Rust level; a C-API-level fuzz harness is a genuine remaining gap and is
listed as such rather than stubbed.

### The defects this work found

Sixteen, all in files that looked fine and had simply never been executed. In
rough order of severity:

1. **`cargo clippy -D warnings` (ci.sh check #2, HARD-FAIL) was RED on `main`**
   with 66 warnings. Now 0 — style lints that a faithful Zig port
   systematically trips are allowed workspace-wide with the reason recorded at
   the table; genuinely incidental ones were fixed at the site.
2. **`cargo fmt --check` (check #1, HARD-FAIL) was RED on `main`** — 243 hunks
   across 73 files.
3. **The Linux build was broken** — `set_macos_display_id` gained a `draw_now`
   argument on the macOS arm only, and the shared mailbox drain calls it
   unconditionally.
4. `crates/rustty-os/src/i18n.rs` declared `_libintl_locale_name_canonicalize`
   `extern` on every non-macOS target, breaking every non-macOS **static**
   link. Found only in a hermetic sandbox — the macOS host had a Homebrew
   gettext lying around to satisfy it.
5. `librustty_vt_c.a` carried an `_OnigEncodingKOI8` nothing defined.
6. rustty emitted **no pkg-config files at all**; the oracle emits four.
7. CMake's `add_custom_command(OUTPUT …)` meant `cmake --build` **never
   re-invoked cargo**.
   8–11. Four separate defects made `example/c-vt-cmake-cross` unbuildable: no
   cross compiler exported to cargo, cc-rs's `--target=` flag zig rejects,
   host `ar`/`ranlib` on cross objects (silently dropping archive members),
   and `INTERFACE_INCLUDE_DIRECTORIES` resolving to the empty string for every
   downstream consumer.
8. `--no-default-features` stopped meaning "SIMD off" when `-Dvt-features`
   landed, so CMake's SIMD-off path dropped the whole C API surface.
9. `+terminal-snapshot` sat behind a feature nothing enabled, hiding six
   compile errors' worth of drift from the codec that had already landed.
10. Three `# shellcheck disable=… — <reason>` directives were silently inert.
11. `overlays` read `final.stdenv` — a cycle that made every Nix VM app fail
    with "infinite recursion encountered".
12. `G-LINT` was scanning `.claude/worktrees/`.

### A note on where defects hide

Fourteen of the sixteen were found by _running_ something that had never been
run. Two needed a **hermetic** environment specifically. That is the argument
for keeping `G-NIX` even though it overlaps `G-CCONSUMER`, and for `G-XTARGET`
even though `G-WINCHK` already exists: same code, checked without the host's
ambient conveniences.

---

## Verdict

The **artifact-producing** half of the build system is in good shape. What is
missing is almost entirely the **distribution and verification** half:

| Layer                                                                                   | State                                                                                                                        |
| --------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| Cargo workspace + `crates-sys` vendoring                                                | **done** — replaces `build.zig` + `pkg/` + `vendor/` + `build.zig.zon*`; all 16 `-sys` crates vendor their C sources in-tree |
| `xtask` (dist, resources, headers, xcframework, translations, release-info, wasm patch) | **done** — replaces `src/build/`'s 34 Zig steps                                                                              |
| CMake consumer path + Doxygen + Makefile + `dist/{cmake,doxygen,linux,windows,macos}`   | **done** — 1:1 with the oracle                                                                                               |
| macOS app + Sparkle appcast + release/publish workflows                                 | **mostly done** — see §D for the holes                                                                                       |
| **Nix** (flake, devShell, packages, 16 VMs, NixOS tests)                                | **Phase A done** — flake + devShell + librustty-vt, verified; VMs and `tests.nix` still absent (Phase B)                     |
| **Flatpak / Snap manifests**                                                            | **absent** — 0 of 7 files (the `dist/linux` inputs they consume _are_ ported)                                                |
| **CI breadth**                                                                          | **1 job** (2-OS matrix) + 5 new gates, vs. the oracle's **~52** jobs                                                         |
| **Lint enforcement**                                                                    | **done** — all six run as `G-LINT`                                                                                           |

> The two rows above marked done were **absent** when this audit was written;
> the rest of the document describes the state at that time. See the Status
> section for what has since changed.

Put bluntly: rustty can _build_ everything ghostty builds, but much of what is
outside `cargo` + the parity gates is still not _verified_ on every push, and
rustty cannot yet be _installed_ by a Flatpak or Snap user (a Nix user can now
get the C library and the dev shell, but not the app).

---

## A. Nix — the largest single gap

**Missing (everything):** `flake.nix`, `flake.lock`, `default.nix`, `shell.nix`,
`.envrc`, and ~29 of the oracle's 33 `nix/` files (4 are zig-cache machinery
with no analog and are explicitly dropped by the Batch-4 spec).

```
nix/devShell.nix  nix/package.nix  nix/libghostty-vt.nix  nix/tests.nix
nix/build-support/{build-inputs,gi-typelib-path,ld-library-path}.nix
nix/build-support/check-blueprints.sh          ← rustty ships 19 .blp files, so this applies
nix/pkgs/{blessed,ucs-detect,wcwidth,wraptest}.nix
nix/test-src/test_libghostty_vt.c
nix/vm/*  (16 files)
```

**Blast radius beyond packaging.** The flake is a load-bearing dependency of
things that look unrelated:

- **HACKING.md has ~15 forward references to it** ("once the Nix flake lands,
  Batch 4"). Every lint section tells contributors to run `nix develop -c
prettier/alejandra/shellcheck/swiftlint` — commands that do not exist.
- The devShell is where ghostty **pins lint-tool versions**. Without it,
  "matches the version in `nix/devShell.nix`" (HACKING.md:301) is unresolvable
  and lint results are machine-dependent.
- `nix.yml` CI, the `alejandra` CI job, and the 16 desktop-environment VMs used
  for GTK manual testing all hang off it.

**Good news, and it is substantial:** the Rust port makes this _easier_ than the
oracle's version, not harder. `crates-sys` vendors all C sources in-tree and
`Cargo.lock` is a first-class Nix input, so the entire `zon2nix` /
`build.zig.zon.nix` / `zigCacheHash.nix` / `check-zig-cache.sh` /
`fetch-zig-cache.sh` / `update-mirror.nu` complex — the drift-prone part of
ghostty's flake — **collapses to crane's `cargoVendorDir`.** No hash file to
keep in sync. The Batch-4 spec already locks this decision (crane +
`oxalica/rust-overlay` honoring `rust-toolchain.toml`).

**Two blockers, both soft:**

1. `nix` is **not installed on this machine** (`which nix` → not found).
2. The Batch-4 spec deferred execution to Linux because Phase B (`package.nix`,
   VMs, `tests.nix`) needs a building GTK app. **That premise has since
   weakened:** `crates/rustty-apprt/src/gtk/` is now broadly live (only 12
   `unimplemented!()` left in the whole crate, 19 `.blp` files, and ci.sh runs
   GTK-specific gates G-GTKINIT / G-GTKFEAT / the ws15/ws17/ws19 portal gates).
   Phase B is likely unblocked; it still wants a Linux box to _verify_.

Phase A (`flake.nix` skeleton, `devShell`, `librustty-vt`, the flake-compat
shims, `nix/pkgs/*`) is verifiable on **aarch64-darwin** once Nix is installed —
it does not need Linux.

---

## B. Flatpak / Snap — manifests only

Missing: `flatpak/{com.mitchellh.ghostty.yml, com.mitchellh.ghostty-debug.yml,
dependencies.yml, exceptions.json, zig-packages.json}` and `snap/{snapcraft.yaml,
local/}` — 7 files.

Everything they _consume_ is already ported: `dist/linux/app.desktop.in`,
`io.airely.rustty.metainfo.xml.in`, `dbus.service{,.flatpak}.in`,
`systemd.service.in`, the Nautilus/Dolphin integrations, and `xtask resources
--target-os linux --flatpak`. So this is manifest authoring, not asset work.

`flatpak/zig-packages.json` is the Zig-dependency manifest — **no analog**; the
Flatpak Rust story is `flatpak-cargo-generator.py` producing a
`cargo-sources.json`, or (simpler, and worth considering) vendoring via
`cargo vendor` into the source tarball `xtask dist` already produces.

Both are gated on the GTK Linux app actually building on Linux, same as Nix
Phase B.

---

## C. CI breadth — the highest-risk gap

`.github/workflows/ci.yml` is **one job** on `{ubuntu, macos}-latest` that
invokes `.port/tools/ci.sh`. That script is excellent _at what it covers_ —
fmt, clippy, workspace build/test, and ~60 parity gates. But it covers **only
the Cargo world**. The oracle's `test.yml` has ~52 jobs. Classes with **zero**
rustty coverage:

| Class                   | Oracle jobs                                                                                                 | Why it matters for rustty specifically                                                                                                                         |
| ----------------------- | ----------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **C/CMake consumer**    | `build-cmake`, `test-lib-vt-pkgconfig`, `build-examples-cmake`, `build-examples-cmake-windows`              | `CMakeLists.txt` (400 lines) + `dist/cmake/rustty-vt-config.cmake.in` + the pkg-config install are **ported but never exercised**. Silent rot is near-certain. |
| **Example builds**      | `list-examples`, `build-examples-{cmake,swift}`                                                             | 33 examples under `example/` are checked in; nothing compiles them.                                                                                            |
| **Cross-target builds** | `build-libghostty-vt-{windows,android,macos,wasm,features}`, `build-ghostty-windows-gnu`                    | rustty has a `G-WINCHK` gate (`cargo check`-level) and an opt-in `--webgl` lane, but no real Windows/Android/wasm **build** legs.                              |
| **Dist validation**     | `build-dist`, `build-dist-lib-vt`                                                                           | `xtask dist`/`distcheck` exist and are used by the release workflows — but are first exercised _at release time_, which is the worst time to find out.         |
| **Lint / format**       | `prettier`, `shellcheck`, `swiftlint`, `typos`, `alejandra`, `pinact`, `blueprint-compiler`, `translations` | **All configs are checked in** (`typos.toml`, `.prettierignore`, `.shellcheckrc`, `.swiftlint.yml`, `.github/pinact.yml`) and **none are enforced.**           |
| **Packaging smoke**     | `build-flatpak`, `build-snap`, `build-nix`, `test-pkg-linux`, `test-debian-13`, `valgrind`                  | Blocked on §A/§B — except `valgrind`, which only needs the checked-in `valgrind.supp`.                                                                         |
| **Meta**                | `skip`, `required`                                                                                          | The `required`-aggregator pattern is what makes branch protection sane once the matrix grows. Worth adopting _before_ the matrix grows.                        |

Note the oracle's `test-gtk`, `test-simd`, `test-i18n`, `test-kaitai`,
`test-sentry-linux` **are** substantially covered — by ci.sh gates
(G-GTKFEAT, G-KAITAI, the `i18n_msgid_coverage` test, the SIMD feature lanes)
rather than by separate jobs. That is a legitimate structural difference, not a
gap.

---

## D. Release pipeline holes

`release-tag.yml` / `release-tip.yml` / `publish-tag.yml` are real ports (493 /
465 / 100 lines, inert-by-default behind `RUSTTY_RELEASE_ENABLED`). Missing jobs
vs. the oracle:

| Missing job                                                        | Notes                                                                                                                                                                                                                         |
| ------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sentry-dsym` (tag), `sentry-dsym{,-debug-slow,-debug-fast}` (tip) | Needs a maintainer-provided Sentry DSN/auth token — the spec's external-artifact policy says placeholder, never fabricate. `crates-sys/sentry-sys` + `rustty-crash` are ported, so the upload step is the only missing piece. |
| `source-tarball-lib-vt` (tip)                                      | A VT-library-only source tarball, separate from the app tarball.                                                                                                                                                              |
| `build-lib-vt-xcframework` (tip)                                   | `xtask xcframework` exists; the _release upload_ leg does not.                                                                                                                                                                |
| `build-lib-vt-wasm` (tip)                                          | `xtask wasm-patch-growable-table` + the wasm examples exist; no release artifact.                                                                                                                                             |
| `build-macos-debug-{slow,fast}` (tip)                              | Debug tip builds for bug reports.                                                                                                                                                                                             |

Also missing: `.github/workflows/update-colorschemes.yml` (rustty ships
`check_theme_corpus.py` as a gate, but nothing _refreshes_ the corpus), and
`.github/scripts/check-apple-libghostty-vt.nu` (verifies the xcframework's Apple
platform slices).

---

## E. Smaller consumer/test assets

| Missing                                | Oracle path                                             | Comment                                                                                                   |
| -------------------------------------- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Swift package example                  | `example/swift-vt-xcframework/`                         | rustty builds `RusttyKit.xcframework` but ships no example consuming it — and no CI job proving it links. |
| C fuzz harness                         | `test/fuzz-libghostty/` (+ corpus, `replay-crashes.nu`) | rustty has `run_fuzz_smoke.sh` (G5) but not the **C-API-level** fuzzer against the public header.         |
| Windows DLL init test                  | `test/windows/test_dll_init.c`                          | Pairs with the missing Windows build legs.                                                                |
| `test/ucs-detect.sh`, `test/README.md` | —                                                       | `ucs-detect` is also one of the `nix/pkgs/*` derivations.                                                 |

---

## F. `.gitattributes` — a small real bug

rustty's `.gitattributes` opens with `* text=auto` and never exempts Windows
resource files. The oracle deliberately marks them `-text`:

```
*.rc -text
*.manifest -text
```

rustty ships `dist/windows/rustty.rc` and `rustty.manifest`; under `text=auto`
git may normalize their line endings, which native Windows resource tooling is
picky about. Also absent: `eol=lf` for `*.nix`, `*.yml`, `*.yaml`, `*.blp`,
`*.swift`, `*.glsl`, `CMakeLists.txt`, `*.cmake`, `Makefile`, and any
`linguist-vendored` markers for `crates-sys/*/vendor/**` (≈54 MB of vendored C
currently counted as rustty's own code, and diffed in full on review).

---

## Recommended fill order

Ordered by _value ÷ cost_, and by what the current machine (macOS, no Nix) can
actually verify.

**1. Lint enforcement + `.gitattributes` (hours, verifiable here).**
Six configs are checked in and unenforced; wiring `typos`, `prettier`,
`shellcheck`, `swiftlint`, and `pinact` jobs is near-zero-risk and immediately
stops drift. Add the `.rc`/`.manifest` `-text` fix and the `linguist-vendored`
markers in the same change. Decide up front whether these become ci.sh gates
(consistent with "ci.sh is the single source of truth", per ci.yml's own header)
or separate workflow jobs (matches the oracle) — **recommend ci.sh gates**, so
the local and CI stories stay identical.

**2. CMake/pkg-config + examples CI (days, verifiable here).**
This is the highest-risk gap, because a 400-line `CMakeLists.txt`, a
`*-config.cmake.in`, a pkg-config install, and 33 examples are all _ported and
untested_. `build-cmake` + `test-lib-vt-pkgconfig` + a `list-examples` →
`build-examples-cmake` matrix + a `build-dist`/`distcheck` job. Fold in the
missing `example/swift-vt-xcframework` while you are there — it and
`build-examples-swift` are the only proof the xcframework is consumable.

**3. Nix Batch 4 Phase A (days–week; needs `nix` installed, macOS is fine).**
`flake.nix` + `devShell` + `librustty-vt` + `default.nix`/`shell.nix`/`.envrc` +
`nix/pkgs/*`. Verifiable on `aarch64-darwin` with `nix flake check`, `nix build
.#librustty-vt`, `nix develop`. This unblocks the `alejandra` job, `nix.yml`,
and — importantly — **retires ~15 stale forward references in HACKING.md**.
Once the devShell exists, re-point step 1's tool versions at it.

**4. Release-pipeline holes (days).** `build-lib-vt-{xcframework,wasm}` and
`source-tarball-lib-vt` need no secrets and can land now. The `sentry-dsym` jobs
land as placeholder-gated steps per the external-artifact policy.

**5. Nix Phase B + Flatpak + Snap (Linux, week+).** `package.nix`, the 16 VMs,
`tests.nix`, the Flatpak/Snap manifests, and the `build-{nix,flatpak,snap}` +
`trigger-{snap,flatpak}` CI jobs. Re-validate the Batch-4 spec's "deferred until
GTK is live" premise first — the GTK apprt has advanced a long way since
2026-07-15, and this may now be straightforwardly executable on a Linux box.

**6. Cross-target build legs + fuzz/Windows test assets (as capacity allows).**
Windows/Android/wasm build legs, `test/windows/test_dll_init.c`,
`test/fuzz-libghostty`, `update-colorschemes.yml`.

### Cross-cutting notes

- Steps 1–4 are all doable on this macOS host; only step 5 genuinely needs
  Linux. Step 3 needs a Nix install (Determinate or nix-darwin).
- Adopt the oracle's `skip` / `required` aggregator-job pattern **when the
  matrix first grows** (step 2), not after — retrofitting branch protection
  across 20 job names is the annoying version.
- Keep the ci.yml ↔ ci.sh lockstep invariant that ci.yml's header declares. Any
  new check should have a local invocation, or the invariant quietly dies.
