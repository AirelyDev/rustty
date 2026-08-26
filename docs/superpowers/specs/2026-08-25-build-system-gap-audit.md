# Build-System Gap Audit — rustty vs. ghostty

**Date:** 2026-08-25
**Oracle (READ-ONLY):** `/Users/gael/projects/ghostty` @ `683d8db64` (the pinned
`.port/ORACLE_SYNC.md` sync point)
**Scope:** everything that _builds, packages, tests-in-CI, or ships_ rustty —
not product source. Companion to
`docs/superpowers/specs/2026-07-15-top-level-project-files-port-design.md`
(Batches 1–5) and `2026-07-15-batch4-nix-flake-design.md`.

---

> **Second pass, 2026-08-26 (oracle re-synced to `88f57ee66`):** the fill order
> below is complete and still holds. A fresh audit along axes the first pass did
> not walk — build _options_ rather than CI jobs, and per-file byte fidelity —
> found twelve more gaps. They are recorded in **§G** at the end of this
> document, not folded in above, so the record of what the first pass claimed
> stays honest.

## Status (updated 2026-08-26) — COMPLETE

Every step of the fill order below is done, on branch `build-system-gaps`
Twelve new gates run from `.port/tools/ci.sh`:

| Gate          | Covers                                                                       |
| ------------- | ---------------------------------------------------------------------------- |
| `G-LINT`      | shellcheck, prettier, typos, swiftlint, alejandra, pinact                    |
| `G-CCONSUMER` | cmake build/install, both `.pc` files, shared+static link                    |
| `G-EXAMPLES`  | all 33 `example/` programs, incl. cross and SwiftPM                          |
| `G-DIST`      | `xtask dist` + `dist-lib-vt` shape, + (opt-in) `distcheck`                   |
| `G-NIX`       | `nix flake check` (7 checks) + `alejandra --check`; BLOCKED w/o `nix`        |
| `G-XTARGET`   | cross-target builds of `rustty-vt-c` (wasm ×2, windows ×2, freebsd, android) |
| `G-I18N`      | msgcmp all 32 `po/*.po` against the `.pot`                                   |
| `G-BLUEPRINT` | all 19 `.blp` templates compile and are formatted                            |
| `G-VALGRIND`  | FFI-crate test binaries under memcheck (489 tests)                           |

**Nix is complete and verified.** `nix build .#librustty-vt`, `nix build
.#rustty` (the GTK app), `nix flake check` (all 7), `nix develop` (exactly the
`rust-toolchain.toml` pin), and all six `nix run .#<desktop>` VM launchers
evaluate on aarch64-linux.

**The flatpak is verified**: built, installed and run — `flatpak run
io.airely.rustty +version` → `Rustty 0.1.0`. flatpak-builder needs no root; it
ships as a flatpak.

### What remains

Re-derived from a fresh tree diff. Of the oracle's 52 `test.yml` jobs, **49 are
covered**. What is left:

**One thing genuinely blocked** — `snap`. snapcraft is distributed as a snap,
snapd is absent from the Linux box, and installing it needs root. It was also
removed from nixpkgs in Sep 2025 ("following removal of LXD"), and is not on
PyPI for this Python. The manifest carries an UNVERIFIED header naming the
exact commands. `snap.yml` is therefore not written either: a CI workflow for
something nobody has built once would be theatre.

**One environment limit, not a gap** — the NixOS VM tests run, but not _here_.
All four evaluate and build, and under KVM the guest kernel boots in ~3s; what
fails is systemd's backdoor.service inside a rootless container, so the test
driver never gets a shell. They are wired as their own `vmChecks` flake output
(not `checks`, which would make `nix flake check` permanently red here for an
unrelated reason) and G-NIX runs them under `RUSTTY_NIX_VM_TESTS=1`.

**Deliberately not ported** — release-tip's `build-macos-debug-{slow,fast}` and
`sentry-dsym-debug-{slow,fast}`. These publish _debug_ nightly builds for bug
reports; release-**tag** has the `sentry-dsym` job (secret-gated) and
release-tip has the three lib-vt artifacts. Adding debug tip builds is a
release-policy decision, not a porting gap.

**Correctly absent** — no analog: the zig-cache machinery (`zigCacheHash.nix`,
`check-zig-cache.sh`, `fetch-zig-cache.sh`, `update-mirror.nu`,
`flatpak/zig-packages.json`), the three `example/zig-*` programs, the five
`vouch-*.yml` governance workflows, and `test.yml` / `nix.yml` as separate
files — rustty's equivalent is `ci.sh` plus the gate suite, by design.

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

---

## G. Second-pass findings (2026-08-26, oracle `88f57ee66`)

The first pass walked the oracle's **CI job list** and closed it to 49/52. This
pass walked two axes it never touched:

- **Build _options_** — `src/build/Config.zig`'s 19 semantic `-D` flags, versus
  rustty's cargo-feature / target-derived seams. A ported *option* that no
  longer flips anything is invisible to a job-list diff.
- **Per-file byte fidelity** — `git ls-files --eol` across the whole tree,
  versus the oracle's per-directory `.gitattributes` guards.

Both produced findings, and the second produced a live data-corruption bug.

### G1 — `-Dflatpak=true` has no analog: the Flatpak ships with Flatpak support OFF

`crates/rustty-build-config/src/build.rs:113` hardcodes `flatpak: false`, and
`rustty-build-config` declares no feature that overrides it (its only feature is
`artifact-lib`). `flatpak/io.airely.rustty.yml:92` builds
`-p rustty --no-default-features --features "gtk wayland x11"` — it never
enables `rustty-termio/flatpak`, and nothing could make
`rustty_build_config::flatpak()` return true anyway.

Four call sites are therefore dead **inside the Flatpak**:

| Site                                    | What is lost                                       |
| --------------------------------------- | -------------------------------------------------- |
| `crates/rustty-apprt/src/gtk/flatpak.rs:24,81` | portal / DBus host integration                |
| `crates/rustty-termio/src/exec.rs:1991`  | `flatpak-spawn --host` — commands run **sandboxed** |
| `crates/rustty-os/src/passwd.rs:86`      | host passwd lookup                                  |

The oracle passes `-Dflatpak=true` in its manifest precisely to turn these on.

This is the "verified but vacuous" class the first pass warned about: the
Flatpak was genuinely built, installed and run, but the smoke test is
`+version`, which touches none of the above. **The Flatpak is real; its Flatpak
integration is not.**

**Fix:** add a `flatpak` feature to `rustty-build-config` that flips
`CONFIG.flatpak` and chains `rustty-termio/flatpak`; pass it from the manifest;
extend `flatpak.yml`'s smoke step past `+version` to something that exercises
the spawn path.

### G2 — `-Dsnap` likewise

`build.rs:115` hardcodes `snap: false`. `crates/rustty-os/src/open.rs:138`
gates snap-confined `xdg-open` handling on it. `snap/snapcraft.yaml:91` builds
the same feature set as the Flatpak. Same shape, same fix; lower priority only
because the Snap is `UNVERIFIED` anyway.

### G3 — A fuzz seed is corrupted in the committed tree (`.gitattributes`)

`.port/phase-0/corpora/cvt/parser-initial/02-crlf` exists to push CRLF bytes
through the parser. Proven, not inferred:

```
oracle blob : 6c69 6e65 310d 0a6c 696e 6532 0d0a ...   line1\r\nline2\r\n...
rustty blob : 6c69 6e65 310a 6c69 6e65 320a ...        line1\nline2\n...
```

rustty's `.gitattributes` opens with `* text=auto` and protects fixtures by
**extension** (`*.vt`, `*.golden`, `*.bin`). The corpus seeds have no extension,
so `text=auto` detected `02-crlf` as text and normalized the CRLF away at
`git add`. The oracle guards exactly this with a per-directory rule in
`test/fuzz-libghostty/.gitattributes`: `corpus/parser-initial/** binary`.

Two things make this nasty:

- **`git status` is clean and the working tree still has CRLF** — the
  smudge/clean filter round-trips it. The corruption is invisible locally and
  only exists for anyone who clones.
- `crates/rustty-vt/tests/fuzz_corpus_replay.rs` replays this seed from the
  working tree. It tests CRLF on this machine and LF on CI. The test is green
  in both cases.

A whole-tree sweep (`git ls-files --eol | grep '^i/lf.*w/crlf'`) found **16**
such files: this seed plus 15 vendored FreeType MSVC project files under
`crates-sys/freetype-sys/vendor/builds/`, which upstream ships CRLF. Nothing
builds those, so they are byte-drift rather than breakage — but
`crates-sys/*/vendor/**` is marked `linguist-vendored` and not `-text`, so
vendored third-party trees are being line-ending-rewritten as a class.

The mechanism demonstrably works where it was applied: the five
`.port/phase-0/harness/fixtures/*.vt` files report `i/crlf w/crlf attr/-text`.

**Fix:** `.port/phase-0/corpora/** binary` and `crates-sys/*/vendor/** -text`,
then re-add `02-crlf` with its true bytes.

### G4 — The deep fuzz corpora exist only in the oracle

`crates/rustty-vt/fuzz/corpus/` is **gitignored**: 11,730 files locally, zero
tracked. The oracle *commits* all 4,001 of its seeds, splitting them into
hand-written `*-initial` and afl-cmin-deduped `*-cmin` tiers with a
`.gitattributes` marking the latter `linguist-generated`.

rustty tracks the 94 `*-initial` seeds (copied to `.port/phase-0/corpora/cvt/`)
and reaches the 3,907 `*-cmin` cases only when `GHOSTTY_ORACLE` is exported —
so on CI, and for anyone else who clones, the edge-deduped corpus never
replays. Defensible as an R3 call; worth making deliberately rather than by
`.gitignore` default.

### G5 — No Windows CI leg at all

The oracle runs three Windows jobs (`test-windows`,
`build-libghostty-vt-windows`, `build-ghostty-windows-gnu`). rustty runs none,
and `windows-latest` is free on GitHub — so unlike `snap`, nothing blocks this.

`G-WINCHK` is `cargo check` from a non-Windows host. `G-XTARGET`'s
`x86_64-pc-windows-msvc` arm skips off-Windows (no MSVC). **No Windows object
code is ever produced, linked, or run.** `test/windows/test_dll_init.c` and its
README are checked in, and the README's own Status section says "Not run by any
gate."

### G6 — The macOS app is only ever built at release time

`xcodebuild` appears in `release-tip.yml` and `release-tag.yml` and **nowhere
else** — no gate, no `ci.yml` step. `macos/Rustty.xctestplan` and
`macos/RusttyUITests/` are never executed by anything. The oracle has four CI
jobs driving `xcodebuild` (`build-macos`, `build-macos-xcode-27`,
`build-macos-freetype`, `test-macos`).

This is precisely the failure mode `G-DIST` was created to close — "first
exercised at release time, which is the worst time to find out" — still open
for the app bundle, on a host that can run it.

### G7 — `xtask resources` is run by no gate and no workflow

Grepping every `xtask -- <cmd>` invocation across `.port/tools/` and
`.github/workflows/` yields `dist`, `dist-lib-vt`, `distcheck`, `header-gen`,
`header-gen-apprt`, `release-info`, `update-translations`, `xcframework-vt`.
Missing: **`resources`**, `gen`, `xcframework` (the app one — only the `-vt`
variant is gated), `wasm-patch-growable-table`.

`resources` generates **1,215 files**: man pages, the compiled terminfo
database, 32 `.mo` catalogues, themes, shell integration, and the Linux
desktop/dbus/systemd units. Its only callers are the Flatpak and Snap
manifests. I ran it during this audit — exit 0 on macOS, output correct — so
this is uncovered rather than broken, but it is 1,215 files of release-critical
output behind zero gates.

### G8 — `src/build/docker/` is not ported (2 files), and Doxygen never runs

- `src/build/docker/lib-c-docs/{Dockerfile,entrypoint.sh}` — the container that
  builds and serves the Doxygen C-API docs. rustty has the `Doxyfile` and all
  of `dist/doxygen/`, and **nothing anywhere runs `doxygen`**.
- `src/build/docker/debian/Dockerfile` — the pinned Debian dependency set
  (`blueprint-compiler`, `libonig-dev`, `libbz2-dev`, …). `check_dist.sh`'s
  debian arm substitutes a prebuilt `rust:trixie` image, so it builds only the
  lib-vt slice and rustty has no canonical "what a Debian build needs" artifact.

### G9 — Two `Doxyfile` deltas

Diffed modulo naming, everything else is path relocation:

- `PROJECT_LOGO` is **empty**; the oracle sets `images/gnome/64.png`, and
  rustty ships that exact file.
- `HTML_TIMESTAMP = NO` was **dropped**, so generated docs embed a build
  timestamp and stop being reproducible.

### G10 — `Makefile clean` misses `zig-out/`

The oracle's clean removes `zig-out .zig-cache`. rustty's removes
`dist/RusttyKit.xcframework`, `build-cmake`, `doc` — but not `zig-out/`, which
is where `xtask resources` writes. The live consequence in this working tree: a
`zig-out/share/ghostty/` directory and `ghostty.terminfo` / `ghostty.termcap`
dated 2026-07-14, from before the rename, sitting alongside the current
`rustty.*` output. Gitignored, so not an R2 violation — but it is exactly the
staleness a clean target prevents.

### G11 — `.gitignore` / `.gitattributes` drift

- `.gitignore` misses `/result*` and `/.nixos-test-history` (Nix build
  symlinks) and `vgcore.*` (Valgrind core dumps). All three became relevant
  when rustty gained the flake and `G-VALGRIND`; the oracle has all three.
- `macos/GhosttyKit.xcframework/` is a stale **R2-named** ignore entry; the
  artifact is `RusttyKit`.
- `.gitattributes` lacks the oracle's `*.txt text eol=lf`.

### G12 — Still no `required` / `skip` aggregator

Carried forward from §C. The first pass recommended adopting it *before* the
matrix grows. G5 and G6 grow the matrix; this is the moment.

### Re-classified as correctly absent

- **`-Dfont-backend`** — the oracle's `build-macos-freetype` job keeps
  `coretext_freetype` compiling. rustty's `rustty-font` is a **pure-Rust** port
  with no FreeType, fontconfig or CoreText dependency at all, so the `Backend`
  enum carries no build-graph meaning and there is nothing to matrix.
- **`-Drenderer`** — `metal` is `cfg(target_os = "macos")` and `opengl` is
  `cfg(any(linux, freebsd))`; the selection is target-derived with no override
  in the oracle's CI either.
- **`Makefile`'s `glad` target** — rustty's GL bindings are generated Rust
  loaded via `gl::load_with`, not a vendored `glad.zip`.
- **`nix/{zigCacheHash.nix,check-zig-cache.sh,fetch-zig-cache.sh,update-mirror.nu}`**,
  `flatpak/zig-packages.json`, `example/zig-*`, the five `vouch-*.yml`
  workflows and their `.github/{VOUCHED.td,issue-unvouched-message}` assets —
  as recorded in the first pass.

### Suggested order

1. **G3** — a proven corrupted blob; a `.gitattributes` rule plus one re-add.
2. **G1/G2** — the packages are functionally wrong today.
3. **G10/G11/G9** — one-liners, minutes each.
4. **G7** — wire `xtask resources` into a gate; the machinery already works.
5. **G5/G6** — the two real CI legs. Adopt **G12** in the same change.
6. **G4/G8** — deliberate calls, not defects.
