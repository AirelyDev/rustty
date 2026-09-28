# Parity audit — src/build

40 oracle units across `src/build` (including `src/build/mdgen`, `src/build/webgen` and the three
`src/cli` units bundled with the mdgen shard) were fully read: 7,122 lines read out of 7,122 lines in
scope — every unit was read end to end, none partial.
34 candidate findings were filed; after two independent verification lenses: **26 CONFIRMED, 4 DISPUTED,
4 REFUTED**, 0 unverified. By corrected severity (confirmed + disputed): **0 high, 16 medium, 14 low**.
Most of the area is legitimately _replaced_ (Zig build graph → cargo features, per-crate `build.rs`,
`xtask`, CMake), and the surviving gaps cluster in three places: Apple multi-arch packaging, git-derived
versioning, and Linux/Nix/Windows install plumbing.

The single most important gap is the **missing universal/multi-slice Apple build path** (S63-F3, S64-F3,
S64-F4, S65-F3): `xtask` hardcodes `aarch64-apple-darwin`, there is no `lipo` step anywhere, and neither
the iOS nor iOS-simulator lib-vt slices are produced — so the shipped `RusttyKit.xcframework`,
`rustty-vt.xcframework` and `Rustty.app` cannot run on Intel Macs or be embedded on iOS, where the oracle
defaults to `.universal`. Close behind it is S63-F2 / S64-F1: `GitVersion.detect` is not ported, so every
untagged build reports version `0.1.0` on the `stable` release channel where the oracle reports
`X.Y.Z-<branch>+<hash>` on `tip`.

## Coverage

| Oracle unit                               | rustty counterpart(s)                                                                                                                                                                                                                                                                        | Status         | Note                                                                                                                                                                                                                                                                                                                                        |
| ----------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/build/CombineArchivesStep.zig`       | `xtask/src/main.rs:571-599`, `xtask/src/archives.rs:276-300`                                                                                                                                                                                                                                 | partial        | Darwin `libtool -static` path ported over ranlib-normalized archives; the non-Darwin MRI `zig ar -M` path (`lib<name>-fat.a`) has no counterpart.                                                                                                                                                                                           |
| `src/build/Config.zig`                    | `crates/rustty-build-config/src/{build.rs,lib.rs,apprt.rs,renderer.rs,font.rs}`, `crates/rustty-crash/src/build_options.rs`, `crates/rustty-vt/src/options.rs:72-103`, `crates/rustty-vt-c/Cargo.toml`, `crates/rustty-identity/src/lib.rs`, `xtask/src/main.rs:370`, `CMakeLists.txt:74-95` | replaced       | Build options → cargo features/profiles. `fromOptions` view, field defaults, apprt/renderer/font selectors, i18n/flatpak/snap/sentry/simd defaults, ten `vt_features`, `ExeEntrypoint`, `ReleaseChannel`, macOS 13.0 floor all present. Zig-toolchain arms (baselineTarget, genericMacOSTarget, MSVC, iOS-error, `-fsys`) are NON_PORTS G5. |
| `src/build/GhosttyBench.zig`              | `crates/rustty-bench/Cargo.toml:21-23`, `crates/rustty-synthetic/Cargo.toml:31-34`                                                                                                                                                                                                           | replaced       | `ghostty-bench` → `rustty-bench`; `ghostty-gen` ships as `rustty-synthetic` (S63-F12). ReleaseFast forcing → cargo `release-fast` profile.                                                                                                                                                                                                  |
| `src/build/GhosttyDist.zig`               | `xtask/src/dist.rs`, `.port/tools/gates/check_dist.sh`                                                                                                                                                                                                                                       | partial        | git-archive + VERSION injection + prefix + distcheck ported; `dist-lib-vt` exists. Missing: `lib_vt_excludes` pathspecs, lib-vt distcheck, and distcheck builds instead of testing. Prebuilt-resource fallback replaced by build.rs generation.                                                                                             |
| `src/build/GhosttyDocs.zig`               | `xtask/src/res_misc.rs:135-194`, `crates/rustty-extra/src/mdgen.rs`, `mdgen/render.rs`                                                                                                                                                                                                       | partial        | man1/man5 + `share/<app>/doc/*.{md,html}` emitted from ported mdgen; pandoc replaced by an in-tree renderer; `installDummy` intentionally not ported.                                                                                                                                                                                       |
| `src/build/GhosttyExe.zig`                | `crates/rustty/Cargo.toml`, `crates/rustty/src/main.rs`                                                                                                                                                                                                                                      | partial        | The exe is the `rustty` bin crate. Missing: Windows subsystem + `.rc` embedding, `checkNixShell`, `addPatchElf`. PIE/strip/unwind are cargo profile concerns.                                                                                                                                                                               |
| `src/build/GhosttyFrameData.zig`          | `crates/rustty-cli/build.rs:141-184`                                                                                                                                                                                                                                                         | ported         | `framedata.compressed` generated from `assets/framegen/frames` (sorted, `\x01`-joined, raw deflate via vendored zlib-sys) and `include_bytes!`d.                                                                                                                                                                                            |
| `src/build/GhosttyI18n.zig`               | `xtask/src/res_misc.rs:216-296`, `xtask/src/translations.rs`                                                                                                                                                                                                                                 | partial        | msgfmt install loop ported; update step ported with rustty identity but drops the command-palette source and `N_` keyword (S63-F1).                                                                                                                                                                                                         |
| `src/build/GhosttyLib.zig`                | `xtask/src/main.rs:496-599`, `xtask/src/archives.rs`                                                                                                                                                                                                                                         | partial        | `initStatic` + combine ported for the macOS xcframework. Missing `initMacOSUniversal`, `initShared`, pkg-config files, header install, dsymutil.                                                                                                                                                                                            |
| `src/build/GhosttyLibVt.zig`              | `CMakeLists.txt`, `dist/cmake/librustty-vt*.pc.in`, `crates/rustty-vt-c/Cargo.toml`, `xtask/src/main.rs`, `.port/tools/build-vt-wasm.sh`                                                                                                                                                     | partial        | cargo `[cdylib,staticlib]` + CMake wrapper + `xcframework-vt` + wasm build script. Gaps: Apple universal/iOS slices, versioned dylib naming/symlinks, `Libs.private` content, Android 16 KiB arm, artifact basenames.                                                                                                                       |
| `src/build/GhosttyResources.zig`          | `xtask/src/main.rs`, `res_terminfo.rs`, `res_shell.rs`, `res_editor.rs`, `res_misc.rs`, `res_themes.rs`, `res_shell_integration.rs`, `res_linux.rs`, `main_build_data.rs`                                                                                                                    | replaced       | `xtask resources` reproduces every install (terminfo/tic/infotocap, shell integration, themes, completions, vim/nvim, bat syntax, Linux app resources incl. flatpak/system-package forks and icons). Only `emit_terminfo`/`emit_termcap` gates are unconditional (S64-F8).                                                                  |
| `src/build/GhosttyWebdata.zig`            | `xtask/src/res_misc.rs`, `crates/rustty-extra/src/webgen.rs`                                                                                                                                                                                                                                 | replaced       | Three webgen executables become library fns; same three install paths.                                                                                                                                                                                                                                                                      |
| `src/build/GhosttyXCFramework.zig`        | `xtask/src/main.rs`, `macos/build-rustty.sh`                                                                                                                                                                                                                                                 | partial        | XCF_KIT spec, umbrella header + module map, `-create-xcframework` ported; `.universal` target missing (S64-F3).                                                                                                                                                                                                                             |
| `src/build/GhosttyXcodebuild.zig`         | `macos/build-rustty.sh`, `macos/build.nu`, `.github/workflows/ci.yml`                                                                                                                                                                                                                        | partial        | build + xctest covered. `open` (PlistBuddy + launch env) and `copy` steps have no counterpart; `build-rustty.sh` does not scrub env (S64-F7).                                                                                                                                                                                               |
| `src/build/GhosttyZig.zig`                | `crates/rustty-vt-c/Cargo.toml`, `crates/rustty-vt/Cargo.toml`, `rustty-vt/src/sys.rs`, `rustty-vt/src/kitty/graphics_pixel.rs`, `rustty-vt-c/src/build_info.rs`                                                                                                                             | replaced       | Module wiring → cargo features (simd, oniguruma isolation, ten vt features, `VT_LIB_VERSION`); wuffs optional with byte-exact scalar mirrors.                                                                                                                                                                                               |
| `src/build/GitVersion.zig`                | `crates/rustty-cli/build.rs`, `xtask/src/release.rs`                                                                                                                                                                                                                                         | partial        | Only the short hash is captured. Branch/tag/dirty detection and the `GitNotFound`/`GitNotRepository` split are absent (S64-F1).                                                                                                                                                                                                             |
| `src/build/HelpStrings.zig`               | `crates/rustty-helpgen/src/lib.rs`, `rustty-config/build.rs`, `rustty-cli/build.rs`, `rustty-input/src/help_strings.rs`                                                                                                                                                                      | replaced       | helpgen exe + generated module become three `build.rs` codegen steps writing OUT_DIR tables. `install()` has no counterpart (refuted as non-finding).                                                                                                                                                                                       |
| `src/build/LibsystemOverrideStep.zig`     | `docs/superpowers/plans/2026-07-29-phase3-upstream-resync.md:2991-2996`                                                                                                                                                                                                                      | not-applicable | Rust's `compiler_builtins` does not export C `memcpy`/`memset`/libm on darwin std targets, so libSystem is already used; proven N/A.                                                                                                                                                                                                        |
| `src/build/LibtoolStep.zig`               | `xtask/src/archives.rs`, `xtask/src/main.rs`                                                                                                                                                                                                                                                 | ported         | Per-index `<i>-<stem>.a` normalization + `libtool -static -o` with identical command shape.                                                                                                                                                                                                                                                 |
| `src/build/LipoStep.zig`                  | —                                                                                                                                                                                                                                                                                            | missing        | No `lipo -create -output` anywhere in xtask; all Apple builds are aarch64-only.                                                                                                                                                                                                                                                             |
| `src/build/MetallibStep.zig`              | `crates/rustty-renderer/build.rs`                                                                                                                                                                                                                                                            | partial        | `xcrun metal` + `metallib` and the macOS gate ported; `-mmacos-version-min` missing (S64-F2); toolchain-absent case degrades to runtime shader compile.                                                                                                                                                                                     |
| `src/build/SharedDeps.zig`                | `Cargo.toml`, `rustty-simd/build.rs`, `rustty-os/build.rs`, `rustty-apprt/build.rs`, `rustty-renderer/build.rs`, `crates-sys/{sentry,dcimgui,simdutf,freetype}-sys/build.rs`, `rustty-font/src/embedded.rs`, `rustty-font-shaper/src/stb.rs`                                                 | replaced       | Build-graph wiring → cargo deps + per-crate build scripts; SIMD flags, sentry/breakpad, dcimgui, freetype, pty.c, gresource/blueprint, wayland protocols, embedded fonts all verified. Gaps: MSVC-only SIMD flags, dcimgui `backend-osx`, bold/italic embedded fonts.                                                                       |
| `src/build/UnicodeTables.zig`             | `crates/rustty-unicode/src/props_uucode.rs`, `symbols_uucode.rs`, `generated/props_data.rs`, `generated/symbols_data.rs`                                                                                                                                                                     | replaced       | Build-time unigen executables → pre-generated embedded tables + in-crate regenerators; full-range differential is the G4-uni gate.                                                                                                                                                                                                          |
| `src/build/XCFrameworkStep.zig`           | `xtask/src/main.rs`                                                                                                                                                                                                                                                                          | partial        | `rm -rf` + `xcodebuild -create-xcframework -library -headers -output` with exit check; `-debug-symbols` unused (refuted — dead in the oracle too); single slice only.                                                                                                                                                                       |
| `src/build/combine_archives.zig`          | `xtask/src/main.rs`, `xtask/src/archives.rs`                                                                                                                                                                                                                                                 | partial        | macOS folds archives via `libtool -static` after ranlib normalization; the cross-platform MRI `ar -M` combiner has no counterpart (S65-F5, disputed).                                                                                                                                                                                       |
| `src/build/gtk.zig`                       | `crates/rustty/Cargo.toml`, `crates/rustty-apprt/Cargo.toml`                                                                                                                                                                                                                                 | replaced       | `pkg-config --variable=targets gtk4` autodetection → default-on `wayland`/`x11` cargo features (S65-F6).                                                                                                                                                                                                                                    |
| `src/build/main.zig`                      | `xtask/src/main.rs`                                                                                                                                                                                                                                                                          | replaced       | Pure re-export index; xtask is the orchestration entry point.                                                                                                                                                                                                                                                                               |
| `src/build/uucode_config.zig`             | `.port/phase-0/harness/unicode/uucode_config.zig`, `rustty-unicode/src/generated/*.rs`, `crates/rustty-vaxis/build.rs`                                                                                                                                                                       | replaced       | Byte-identical harness copy drives table generation/verification; libvaxis-only fields generated by `rustty-vaxis/build.rs`. `case_folding_full` has no Rust table (S65-F1).                                                                                                                                                                |
| `src/build/wasm_patch_growable_table.zig` | `xtask/src/wasm_patch_growable_table.rs`, `xtask/src/main.rs:766-792`                                                                                                                                                                                                                        | ported         | `patch_table_growable`, `read_leb128`, `uleb128_size` ported verbatim (u5 shift wrap reproduced); all 7 tests present.                                                                                                                                                                                                                      |
| `src/build/xcframework.zig`               | `xtask/src/main.rs`                                                                                                                                                                                                                                                                          | partial        | `Target = { native, universal }`: only `native` exists; `universal` explicitly deferred at `main.rs:363-364`.                                                                                                                                                                                                                               |
| `src/build/zig.zig`                       | `rust-toolchain.toml`, `Cargo.toml`                                                                                                                                                                                                                                                          | replaced       | `requireZig` → `channel = "1.93.1"` + `rust-version = "1.93"` (equivalent toolchain pin).                                                                                                                                                                                                                                                   |
| `src/build/mdgen/main_ghostty_1.zig`      | `crates/rustty-extra/src/mdgen.rs:175-183`, `mdgen/rustty_1_{header,footer}.md`                                                                                                                                                                                                              | ported         | Same call order: substitute(header), genActions, genConfig(cli=true), substitute(footer).                                                                                                                                                                                                                                                   |
| `src/build/mdgen/main_ghostty_5.zig`      | `crates/rustty-extra/src/mdgen.rs:185-193`, `mdgen/rustty_5_{header,footer}.md`                                                                                                                                                                                                              | ported         | Same order: substitute(header), genConfig(cli=false), genKeybindActions, substitute(footer).                                                                                                                                                                                                                                                |
| `src/build/mdgen/mdgen.zig`               | `crates/rustty-extra/src/mdgen.rs:60-173`                                                                                                                                                                                                                                                    | ported         | `substitute`, `gen_config`, `gen_actions`, `gen_keybind_actions` with the same `_`-skip, `--` prefix, flag spellings and definition-list emitter; one extra newline per section header (S66-F1).                                                                                                                                            |
| `src/build/webgen/main_actions.zig`       | `crates/rustty-extra/src/webgen.rs:163-175`                                                                                                                                                                                                                                                  | ported         | Delegates to `helpgen_actions::generate(Markdown, true, RealHelpStrings)`.                                                                                                                                                                                                                                                                  |
| `src/build/webgen/main_commands.zig`      | `crates/rustty-extra/src/webgen.rs:46-47,177-217`                                                                                                                                                                                                                                            | ported         | Header, `## name`, doc lines and fenced invocation all match; `editOnGithubLink` retargeted (R2 identity).                                                                                                                                                                                                                                  |
| `src/build/webgen/main_config.zig`        | `crates/rustty-extra/src/webgen.rs:38-44,53-78,80-161`                                                                                                                                                                                                                                       | ported         | Field walk, undocumented-field grouping and the code/Note/Warning block state machine are faithful; header prose deviates (S66-F2, disputed).                                                                                                                                                                                               |
| `src/cli/CommaSplitter.zig`               | `crates/rustty-args/src/comma_splitter.rs`                                                                                                                                                                                                                                                   | ported         | Error set, `init/next/rest`, `ESCAPE_OUTSIDE_QUOTES`, every state transition incl. the accumulator quirk; all 28 tests.                                                                                                                                                                                                                     |
| `src/cli/Pager.zig`                       | `crates/rustty-cli/src/pager.rs`                                                                                                                                                                                                                                                             | ported         | TTY check, `$RUSTTY_PAGER > $PAGER > less`, pipe/inherit wiring, spawn-failure fallback, close-then-wait; both tests.                                                                                                                                                                                                                       |
| `src/cli/action.zig`                      | `crates/rustty-cli/src/action.rs`                                                                                                                                                                                                                                                            | ported         | `DetectError`, `SpecialCase`, `detect_iter`, `detect_args` with the same return ordering; all 7 tests.                                                                                                                                                                                                                                      |

**Partial units:** none — no shard reported any `units_partial`; every unit listed above was read in full.
(The `partial` values in the Status column describe counterpart _coverage_, not reading coverage.)

## Findings

### High

None.

### Medium

#### S63-F1 — update-translations drops the command-palette source and the `N_` keyword from xgettext

**Status: CONFIRMED** · kind: missing-branch · severity: medium
Oracle: `src/build/GhosttyI18n.zig:64-71, 152-155` · rustty: `xtask/src/translations.rs:79-92, 196-219`

The oracle's update step passes `--keyword=_ --keyword=N_ --keyword=C_:1c,2` and explicitly adds
`src/input/command.zig` as an xgettext input. rustty's `collect_gtk_sources` walks only
`crates/rustty-apprt/{src/gtk,assets/ui}` and passes `_`, `gettext_`, `tr`, `C_:1c,2` but no `N_`.
`crates/rustty-input/src/command.rs` marks its 180 palette titles/descriptions with `i18n::N_(...)`, so a
regeneration would neither scan that file nor recognise the marker. The committed
`po/io.airely.rustty.pot` carries those references only because it was hand-mirrored
(`i18n_msgid_coverage.rs:302-323` pins the block at 180 and says extraction is not wired); running
`xtask update-translations` would orphan those 180 msgids in every catalog via msgmerge.

Evidence: `GhosttyI18n.zig:69 "--keyword=N_"`, `:153-155 const command_palette_path = "src/input/command.zig"` vs
`translations.rs:88-93` (apprt-only walk) and `:196-211` keyword list. `grep -c 'N_(' crates/rustty-input/src/command.rs` = 180.

Fix: add `crates/rustty-input/src/command.rs` and `--keyword=N_` to the GTK xgettext leg in `translations.rs`.

#### S63-F2 — Version pre-release/build are never derived from git, so dev builds report the `stable` channel

**Status: CONFIRMED** · kind: behavioral-difference · severity: medium
Oracle: `src/build/Config.zig:276-339, 693-701` · rustty: `crates/rustty-build-config/src/lib.rs:93-103, 131-141, 159-181`

The oracle computes `config.version` from git: an untagged checkout gets `pre = branch`,
`build = short_hash`; no git yields `pre = "dev", build = "0000000"`; only a `vX.Y.Z` tag (validated
against build.zig, else `@panic`) or an explicit `-Dversion-string` yields a bare stable version.
`addOptions` derives `release_channel = tip` whenever `pre` is non-empty. rustty's `VERSION` is the
constant `rustty_identity::VERSION` ("0.1.0") and `version_pre()` scans that constant, so every build —
including dev/tip builds — resolves `release_channel() == Stable`. This feeds `auto-update-channel`'s
default (`config_struct.rs:7533`) and the `+version` string. Only the `+build` hash half is recovered
separately (`rustty-cli` build.rs `RUSTTY_COMMIT_HASH`).

Evidence: `Config.zig:332-338`, `:299-307`, `:693-701`; `lib.rs:136 pub const VERSION: &str = rustty_identity::VERSION;`
(identity `lib.rs:78 "0.1.0"`), `lib.rs:101-103`.

Fix: detect git branch/short hash in a build.rs, expose `VERSION_STRING` as `X.Y.Z-<branch>+<hash>` (or
`-dev+0000000`), derive `ReleaseChannel` from it, keeping bare `X.Y.Z` for `vX.Y.Z` tags / explicit override.

#### S63-F3 — No universal (aarch64 + x86_64) macOS library; `xcframework_target` default `.universal` not honoured

**Status: CONFIRMED** · kind: missing-branch · severity: medium
Oracle: `src/build/GhosttyLib.zig:181-211` · rustty: `xtask/src/main.rs:363-364, 496-524`

`initMacOSUniversal` builds the static lib twice (aarch64 and x86_64) and lipo-combines them;
`Config.zig:22` defaults `xcframework_target = .universal`. rustty's `build_xcframework` hardcodes
`target = "aarch64-apple-darwin"` and its doc says the universal slice is deferred, so the shipped
RusttyKit/rustty-vt xcframeworks and the macOS app run only on Apple Silicon.

Evidence: `GhosttyLib.zig:185-199`, `Config.zig:22`; `xtask main.rs:500`, `:363-364`; `release-tag.yml:37-38`.

Fix: build both darwin targets, `lipo -create` the combined archives, and expose an
`--xcframework-target universal|native` switch.

#### S63-F4 — patchelf interp/rpath step for NixOS native builds is absent

**Status: CONFIRMED** · kind: missing-function · severity: medium
Oracle: `src/build/Config.zig:371-406, 642-656` · rustty: `nix/devShell.nix:206-208`

On native Linux inside a Nix shell the oracle patches the built binary with
`patchelf --set-interpreter $(realpath ld.so) --set-rpath $LD_LIBRARY_PATH` (with `-Dpatch-interp` /
`-Dpatch-rpath` overrides), making a `zig build` binary runnable outside the shell. rustty has no analog:
the devShell only exports `LD_LIBRARY_PATH` with a comment that it "should be set onto the rpath", and
NON_PORTS G10 records the interp half as a real, not-dismissed requirement. A `cargo build` in the rustty
devShell yields a binary that does not launch outside it.

Evidence: `Config.zig:377, 382-386, 401-404, 642-656`; `grep -rn 'patchelf|IN_NIX_SHELL'` over crates/xtask/nix → 0 hits
(only `devShell.nix:206`); `.port/phase-3/NON_PORTS.md` G10.

Fix: add a post-link step (xtask, devShell `shellHook`, or cargo runner) that runs patchelf with
`--set-interpreter`/`--set-rpath` when `IN_NIX_SHELL` is set on native Linux, with explicit overrides.

#### S63-F6 — `checkNixShell` NixOS-outside-nix-shell build error is not ported

**Status: CONFIRMED** · kind: missing-error-path · severity: medium
Oracle: `src/build/GhosttyExe.zig:36-37, 66-106` · rustty: `crates/rustty/Cargo.toml:1-40`

When building natively on Linux with `/etc/NIXOS` present and `IN_NIX_SHELL` unset, the oracle fails the
exe step with a multi-line message explaining that the binary will fail to dlopen X11/Wayland libs and
telling the user to run `nix develop -c zig build`. rustty builds silently in that situation, producing a
silently-broken binary.

Evidence: `GhosttyExe.zig:72-84` guards, `:86-105 exe.step.addError(...)`; no build.rs in `crates/rustty`;
`grep -rn 'IN_NIX_SHELL' crates xtask` → 0 hits.

Fix: add the same check to a build.rs of the `rustty` bin crate, emitting `nix develop -c cargo build`
guidance as a hard error.

#### S63-F7 — No distcheck for the lib-vt tarball (test-lib-vt run + CMake configure/build inside the extracted tree)

**Status: CONFIRMED** · kind: missing-test · severity: medium
Oracle: `src/build/GhosttyDist.zig:126-147, 156-171` · rustty: `xtask/src/dist.rs:223-283`, `.port/tools/gates/check_dist.sh:98-160`

For `emit_lib_vt`, the oracle's check step extracts the lib-vt tarball, runs
`zig build test-lib-vt -Demit-lib-vt=true` in it, asserts each dist resource path, then runs
`cmake -B cmake-build` + `cmake --build` from the extracted tree. rustty's `distcheck()` always calls
`dist()` (Workspace scope) and never exercises `dist_lib_vt()`; check_dist.sh's Debian arm only runs
`cargo build --offline --release -p rustty-vt-c --features simd` — no tests and no CMake configure/build
of the shipped `CMakeLists.txt` from the tarball.

Evidence: `GhosttyDist.zig:128-129`, `:159-170` with `expectExitCode(0)`; `dist.rs:228 let tarball = dist()?;`;
`check_dist.sh:156`; `grep -n cmake .port/tools/gates/check_dist.sh` → 0 hits.

Fix: add `distcheck-lib-vt` to xtask: extract `dist/librustty-vt-<v>.tar.gz`, run
`cargo test -p rustty-vt -p rustty-vt-c --offline --locked` and `cmake -B cmake-build && cmake --build cmake-build` in it.

#### S63-F8 — Workspace distcheck builds the extracted tarball instead of running the test suite

**Status: CONFIRMED** · kind: weakened-assertion · severity: medium
Oracle: `src/build/GhosttyDist.zig:128-131` · rustty: `xtask/src/dist.rs:266-279`

The oracle's distcheck runs `zig build test` inside the extracted source tree (expecting exit 0). rustty
runs `cargo build --workspace --offline --locked` — a compile-only check — so a tarball whose tests fail
(e.g. missing testdata dropped by `export-ignore`) passes. rustty is stronger on offline/vendor
assertions but strictly weaker on the assertion the oracle makes.

Evidence: `GhosttyDist.zig:130-131` + `:138 step.expectExitCode(0)`; `dist.rs:274 .args(["build","--workspace","--offline","--locked"])`.

Fix: run `cargo test --workspace --offline --locked` (at least `--no-run` plus the fast lanes) in the extracted tree.

#### S64-F1 — `GitVersion.detect` not ported: branch/tag/dirty detection and the derived pre-release version are absent

**Status: CONFIRMED** · kind: missing-function · severity: medium
Oracle: `src/build/GitVersion.zig:19-86` · rustty: `crates/rustty-cli/build.rs:49-94`

The oracle detects branch (`git rev-parse --abbrev-ref HEAD`, non-`[0-9A-Za-z-]` rewritten to `-`), short
hash, exact tag (`git describe --exact-match --tags`) and dirty state (`git diff --quiet --exit-code`),
distinguishing `GitNotFound` from `GitNotRepository`. `Config.zig:297-338` turns this into
`X.Y.Z-<branch>+<hash>`, `X.Y.Z-dev+0000000` without git, and panics if a tag is not `vX.Y.Z` matching the
manifest. rustty only captures `git rev-parse --short HEAD` into `RUSTTY_COMMIT_HASH`;
`rustty_identity::VERSION` is the fixed literal "0.1.0", so the version string never carries the
branch/hash pre-release, tagged builds are never validated, and `release_channel()` is always `Stable`
where the oracle reports `Tip`.

Evidence: `GitVersion.zig:22-41, 56-64, 66-78`; `rustty-cli/build.rs:61-94`; `rustty-identity/src/lib.rs:78`;
`rustty-build-config/src/build.rs:249-253`.

Fix: add a build-time git probe reproducing `detect()` and the `Config.zig` composition rules, exposing
`VERSION`/`VERSION_STRING` with the pre-release so `release_channel()` becomes `Tip` on untagged builds.

#### S64-F2 — metal compile omits `-mmacos-version-min=<os_version_min>`

**Status: CONFIRMED** · kind: missing-branch · severity: medium
Oracle: `src/build/MetallibStep.zig:36-56` · rustty: `crates/rustty-renderer/build.rs:70-79`

The oracle appends `-mmacos-version-min=<min_version>` to the `xcrun -sdk macosx metal` invocation, where
min_version is the target's `os_version_min` (13.0.0, default "10.14" if unset). rustty's build.rs runs
`xcrun -sdk macosx metal -o <ir> -c shaders.metal` with no version-min flag, so the embedded metallib is
stamped with the host toolchain's default deployment target rather than the app's 13.0 floor. Partial
mitigation: the xcframework path exports `MACOSX_DEPLOYMENT_TARGET`, which clang-based `xcrun metal`
honours; a plain `cargo build` of `rustty-renderer` has no such env.

Evidence: `MetallibStep.zig:36-41, 51-56`; `renderer/build.rs:70-75`.

Fix: add `-mmacos-version-min=13.0` (reading `MACOSX_DEPLOYMENT_TARGET` when set) to the metal compile.

#### S64-F3 — XCFramework `.universal` target (arm64 + x86_64 via lipo) not built; app/xcframework are aarch64-only

**Status: CONFIRMED** · kind: missing-branch · severity: medium
Oracle: `src/build/GhosttyXCFramework.zig:18-57` · rustty: `xtask/src/main.rs:363-364, 500-522`

The oracle's default xcframework target is universal: `GhosttyLib.initMacOSUniversal` builds aarch64 and
x86_64 static libs and lipo-combines them before `-create-xcframework`; `.native` is the opt-in
alternative. rustty's `build_xcframework` hardcodes `aarch64-apple-darwin` and never invokes lipo, so
`RusttyKit.xcframework` (and the shipped Rustty.app, per `release-tag.yml:37-38`) cannot run on Intel
Macs. `LipoStep.zig` has no counterpart at all.

Evidence: `GhosttyXCFramework.zig:19, 42-49`; `LipoStep.zig:30-34`; `xtask/src/main.rs:501`, `:363-364`.

Fix: build both darwin staticlibs (each normalised + libtool-combined), `lipo -create -output` them, and
hand the universal archive to `-create-xcframework`; keep a `--native` opt-out.

#### S64-F4 — lib-vt xcframework lacks the macOS universal slice and the iOS device/simulator slices

**Status: CONFIRMED** · kind: missing-branch · severity: medium
Oracle: `src/build/GhosttyLibVt.zig:146-215, 547-586, 631-659` · rustty: `xtask/src/main.rs:455-471, 500-522`

`initStaticAppleUniversal` always produces a macOS universal `libghostty-vt.a` via LipoStep and, when the
iOS SDK is detected (`detectAppleSDK` via `LibCInstallation.findNative`), additionally builds `ios`
(aarch64, os_version_min 13.0) and `ios_simulator` (aarch64, abi `.simulator`, apple_a17) static libs;
`xcframework()` packs every present slice with the shared headers dir + `module GhosttyVt` map. rustty's
`xtask xcframework-vt` packs a single `aarch64-apple-darwin` slice; no x86_64, no iOS, no simulator, no
SDK detection.

Evidence: `GhosttyLibVt.zig:158-185, 188-213, 631-644`; `xtask/src/main.rs:501`; no `lipo`/`ios`/`simulator` tokens in xtask.

Fix: extend `build_xcframework` with a slice list — macOS universal, plus `aarch64-apple-ios` and
`aarch64-apple-ios-sim` when `xcrun --sdk iphoneos --show-sdk-path` succeeds.

#### S64-F5 — Shared/static library install layout differs: no versioned real name, soname symlinks, or `compatibility_version`

**Status: CONFIRMED** · kind: behavioral-difference · severity: medium
Oracle: `src/build/GhosttyLibVt.zig:384-490` · rustty: `CMakeLists.txt:122-145, 267-274`

The oracle installs the shared lib as `libghostty-vt.<maj>.<min>.<patch>.dylib` with
`libghostty-vt.<maj>.dylib` and `libghostty-vt.dylib` symlinks (on Apple with
`-install_name @rpath/libghostty-vt.dylib -current_version X.Y.Z -compatibility_version 1.0.0`; elsewhere
Zig's versioned `.so.X.Y.Z`/`.so.X` layout), the static lib as `libghostty-vt.a`
(`ghostty-vt-static.lib` on Windows), and the `.pc` `Libs: -lghostty-vt`. rustty installs cargo's flat
`librustty_vt_c.{dylib,so,a}` with no version in the name, no soname symlinks and no
`compatibility_version` (CMakeLists notes this explicitly), and the `.pc` links `-lrustty_vt_c` while its
Name is `librustty-vt`. The missing ABI versioning is observable to every downstream linker/packager;
the basename/link-name half is internally consistent and therefore cosmetic.

Evidence: `GhosttyLibVt.zig:395-402, 424-438, 445-467, 528, 541`; `CMakeLists.txt:134-154, 267-268, 348`.

Fix: rename/copy the cargo artifacts to `librustty-vt.<X.Y.Z>.{dylib,so}` + `.<X>` + unversioned symlinks
(`install_name_tool -id @rpath/librustty-vt.dylib` or link args), install the static archive as
`librustty-vt.a` / `rustty-vt-static.lib`, and point the `.pc` `Libs` at `-lrustty-vt`.

#### S64-F6 — pkg-config `Libs.private` differs: static archive requires libc++ and an external libintl, with a hardcoded Homebrew path

**Status: CONFIRMED** · kind: behavioral-difference · severity: medium
Oracle: `src/build/GhosttyLibVt.zig:493-499, 509-556, 565-576` · rustty: `CMakeLists.txt:231-249, 329-346`

The oracle's static archive is self-contained: `libsPrivate` is empty unless system simdutf is used, and
libghostty-vt never depends on gettext. rustty's `.pc` emits
`-lc++ -framework CoreFoundation -framework Foundation -liconv <absolute Homebrew libintl path> -lobjc -lc -lm`
on macOS, and the static archive pulls rustty-os's i18n object referencing `bindtextdomain`/`dgettext`
(with a WARNING fallback to a bare `intl`). A consumer following the oracle's documented static-link
recipe fails to link, and the emitted `.pc` bakes a machine-specific `/opt/homebrew/...` path. (The
verification pass established that the `-lc++`/`-lstdc++` list is a toolchain-inherent, accurate
description and that `-DRUSTTY_STATIC` in the static `.pc` Cflags mirrors the oracle's header visibility
contract; the surviving defect is the gettext dependency and the absolute path.)

Evidence: `GhosttyLibVt.zig:493-499, 565-576, 511-556`; `CMakeLists.txt:239-246, 250-253, 334`.

Fix: drop the i18n object from the `rustty-vt-c` static archive (feature-gate rustty-os's gettext
bindings), and never bake absolute Homebrew paths into an installed `.pc`.

#### S65-F1 — `case_folding_full` runtime table not ported; non-ASCII key case folding falls back to identity

**Status: CONFIRMED** · kind: missing-function · severity: medium
Oracle: `src/build/uucode_config.zig:31-38` · rustty: `crates/rustty-input/src/binding.rs:3390-3410`

The `runtime` uucode table declares `case_folding_full` for use by `input/Binding.zig`'s
`foldedCodepoint`. rustty has no case-folding table in rustty-unicode; `folded_codepoint` documents that
the Unicode slow path is NOT ported and returns `[cp, 0, 0]` for every non-ASCII codepoint, whereas the
oracle folds e.g. U+00C9 → U+00E9. Two triggers differing only in the case of a non-ASCII letter
hash/compare unequal in rustty and equal in the oracle.

Evidence: `uucode_config.zig:31-38`; `Binding.zig:1997-2012`; `binding.rs:3392-3399` PORT NOTE, `:3409` returns `[cp,0,0]`.

Fix: generate a `case_folding_full` table into rustty-unicode via the same harness pipeline and use it in
`folded_codepoint` for single-codepoint folds.

#### S65-F2 — addSimd MSVC-only C++ flags not applied (`-D_Avx2WmemEnabledWeakValue=_Avx2WmemEnabled`, `-fno-autolink`)

**Status: DISPUTED** · kind: missing-branch · severity: medium
Oracle: `src/build/SharedDeps.zig:995-999` · rustty: `crates/rustty-simd/build.rs:120-127`

The oracle appends two extra flags for the msvc ABI when compiling `src/simd/*.cpp`. rustty-simd's
build.rs only mirrors the windows ubsan-off pair and never adds the MSVC define / `-fno-autolink`, so an
`x86_64-pc-windows-msvc` build of the SIMD kernels differs (UCRT `/alternatename` fallback and autolink
pragmas remain active).

Evidence: `SharedDeps.zig:995-999`; `rustty-simd/build.rs:120-127`; grep for `_Avx2WmemEnabledWeakValue`/`fno-autolink` in
`crates/rustty-simd` finds nothing.

Fix: `if target_env == "msvc" { build.define("_Avx2WmemEnabledWeakValue", Some("_Avx2WmemEnabled")); build.flag_if_supported("-fno-autolink"); }`.

**Refuted by the semantic lens** — verbatim: "SharedDeps.zig:995-999 adds `-D_Avx2WmemEnabledWeakValue=_Avx2WmemEnabled`
and `-fno-autolink` only for `abi == .msvc`, i.e. when Zig's bundled clang cross-compiles for the MSVC ABI;
both are workarounds for that toolchain (clang's autolink pragmas and Zig's lack of UCRT's /alternatename
fallback). rustty-simd/build.rs:160 compiles the same four .cpp files through cc-rs, which on an
x86_64-pc-windows-msvc host target invokes MSVC cl.exe — no clang autolink pragmas are emitted and the UCRT
alternatename symbol is provided by the real UCRT, so neither flag applies (and `-fno-autolink` is not even a
cl.exe flag). No difference in the observable behaviour of the compiled kernels; this is a toolchain-artifact
difference in replaced build plumbing." (The presence lens confirmed the textual absence at high confidence.)

#### S65-F3 — xcframework `universal` target (x86_64 slice + lipo) not implemented; only native aarch64 built

**Status: CONFIRMED** · kind: missing-branch · severity: medium
Oracle: `src/build/xcframework.zig:1-3` · rustty: `xtask/src/main.rs:363-364, 498-616`

The oracle's `Target = enum { native, universal }` drives GhosttyXCFramework to build both aarch64 and
x86_64 slices and lipo them for release. rustty's `xtask xcframework` hardcodes `aarch64-apple-darwin` and
states the universal slice is deferred, so Intel-Mac app builds are not producible from the port. (This
is the `xcframework.zig` enum-level view of the same defect as S64-F3 / S63-F3.)

Evidence: `xcframework.zig:3`; `xtask/src/main.rs:363-364, 500`.

Fix: add a `--universal` mode building both targets and lipo-ing the archives before `-create-xcframework`.

### Low

#### S63-F5 — Windows build lacks the GUI subsystem and the `.rc` resource (icon/manifest) embedding

**Status: CONFIRMED** · kind: missing-branch · severity: low (filed medium; semantic lens regraded to low)
Oracle: `src/build/GhosttyExe.zig:43-52` · rustty: `crates/rustty/Cargo.toml:1-40`

For a Windows target the oracle sets `exe.subsystem = .Windows` and compiles `dist/windows/ghostty.rc`
into the executable. rustty ships `dist/windows/rustty.rc` + `rustty.manifest` + `rustty.ico` but nothing
compiles or links them: the `rustty` bin crate has no build.rs, no `embed-resource`/`winres` dependency,
and no `#![windows_subsystem = "windows"]`. A Windows rustty.exe would open with a console and without
icon/manifest. Regraded low because the macOS-first fork means there is no Windows build path today.

Evidence: `GhosttyExe.zig:44-49`; `ls crates/rustty` (no build.rs); `grep -rn 'rustty.rc|embed_resource|winres|windows_subsystem'` → 0 hits.

Fix: add `#![cfg_attr(windows, windows_subsystem = "windows")]` and a build.rs compiling
`dist/windows/rustty.rc` (e.g. via `embed-resource`) on `target_os = "windows"`.

#### S63-F9 — Non-Darwin `ghostty-internal` shared/static library, header install and pkg-config files are not produced

**Status: CONFIRMED** · kind: missing-function · severity: low
Oracle: `src/build/GhosttyLib.zig:83-179, 213-300` · rustty: absent (nearest: `CMakeLists.txt:307-365`, which is the GhosttyLibVt port)

For non-lib-vt builds on Linux/Windows the oracle installs `ghostty-internal.so`/`.a` (or
`.dll`/`-static.lib`), `include/ghostty.h`, and generates `ghostty-internal.pc` +
`ghostty-internal-static.pc`. rustty's `rustty-apprt-c` yields cdylib/staticlib artifacts under `target/`,
but no install step, no combined fat archive off macOS, no header install and no `.pc` pair exist
(`grep -rn 'rustty-internal'` → doc-comment mentions only). The oracle itself marks the library "not for
external use", hence low.

Evidence: `GhosttyLib.zig:216-232, 257-284, 288-300`; xtask has no `install` subcommand and no pkgconfig writer.

Fix: add an xtask `install-lib` step for Linux/Windows installing the cdylib/staticlib,
`include/rustty-embed.h`, and emitting `rustty-internal.pc` / `rustty-internal-static.pc` with the same fields.

#### S63-F10 — lib-vt tarball does not apply the `lib_vt_excludes` pathspecs

**Status: CONFIRMED** · kind: missing-branch · severity: low
Oracle: `src/build/GhosttyDist.zig:95-102, 184-209` · rustty: `xtask/src/dist.rs:139-165, 409-425`

The oracle's lib-vt archive passes `:(exclude)<path>` for images, macos, `dist/{doxygen,linux,macos,windows}`,
flatpak, snap, po, example, test, `src/font/res`, `src/crash/testdata`, large pkg binaries and vendor.
rustty runs a plain `git archive HEAD` with no pathspecs, and `prune_to_lib_vt` only rewrites workspace
members and deletes out-of-closure dirs under `crates/` and `crates-sys/`, so `macos/`, `images/`, `po/`,
`example/`, `test/`, `flatpak/`, `snap/` and `dist/{linux,macos,windows,doxygen}` all ship in
`librustty-vt-<v>.tar.gz`. `check_dist.sh` only asserts it is smaller than the full tarball.

Evidence: `GhosttyDist.zig:98-102, 184-209`; `dist.rs:409-425`; `grep -n 'images|macos|"po"|example|exclude' xtask/src/dist.rs` → 0 hits.

Fix: remove the equivalent directories from the staged tree in `prune_to_lib_vt`, mirroring `lib_vt_excludes`.

#### S63-F12 — The `ghostty-gen` data generator binary is named `rustty-synthetic`, not `rustty-gen`

**Status: CONFIRMED** · kind: different-constant · severity: low
Oracle: `src/build/GhosttyBench.zig:16-31` · rustty: `crates/rustty-synthetic/Cargo.toml:31-34`

The naming transform gives `ghostty-gen` → `rustty-gen`, and rustty-bench's own docs tell users to run
`rustty-gen ascii | …`, but the only bin built from `main_gen.rs` is `rustty-synthetic`, so the documented
command does not exist. `crates/rustty-global/src/lib.rs:429` itself states the intended mapping.

Evidence: `GhosttyBench.zig:19 .name = "ghostty-gen"`; `rustty-synthetic/Cargo.toml:32-34`;
`rustty-bench/src/terminal_snapshot.rs:40`.

Fix: rename the bin to `rustty-gen` (or add a second `[[bin]]` alias).

#### S64-F7 — `zig build run` open step and `copy app bundle` step have no counterpart

**Status: CONFIRMED** · kind: missing-branch · severity: low
Oracle: `src/build/GhosttyXcodebuild.zig:127-178` · rustty: `macos/build-rustty.sh:49-54`, `xtask/src/main.rs:60-110`

The oracle's `open` step disables state restoration
(`PlistBuddy -c 'Add :NSQuitAlwaysKeepsWindows bool false'`), then launches `<app>/Contents/MacOS/ghostty`
with `GHOSTTY_LOG=stderr,macos` and `GHOSTTY_MAC_LAUNCH_SOURCE=zig_run`, forwarding `b.args`; the `copy`
step `cp -R`s the built bundle into the install prefix. rustty has no run/open command (so the Swift
side's `zig_run` launch-source branch is unreachable) and no bundle-copy step; `build-rustty.sh` also does
not scrub the environment to PATH-only as the oracle's steps do (`macos/build.nu` does).

Evidence: `GhosttyXcodebuild.zig:129-163, 171-178, 58-60`; `macos/build-rustty.sh:49-54`; grep for
PlistBuddy/NSQuitAlwaysKeepsWindows finds only UI tests, AppDelegate and the release workflows' version stamping.

Fix: add an `xtask run-macos` (or `macos/run-rustty.sh`) applying the PlistBuddy tweak, setting
`RUSTTY_LOG=stderr,macos` and `RUSTTY_MAC_LAUNCH_SOURCE=zig_run`, and exec'ing the app binary with
forwarded args; scrub env in `build-rustty.sh` like `build.nu`.

#### S64-F8 — `emit_terminfo` / `emit_termcap` build gates are unconditional in xtask

**Status: CONFIRMED** · kind: behavioral-difference · severity: low
Oracle: `src/build/GhosttyResources.zig:43-77` + `Config.zig:504-522` · rustty: `xtask/src/res_terminfo.rs:132-141, 168-170`

The oracle installs `share/terminfo/ghostty.terminfo` only when `cfg.emit_terminfo` and
`ghostty.termcap` only when `cfg.emit_termcap` (both default false for Release, true for Debug). rustty
always emits the terminfo source and the termcap, so a release resource tree contains extra files the
oracle's release install does not, and `infotocap` becomes a hard build dependency for every profile.
The port documents the divergence at `res_terminfo.rs:45-49`.

Note: the finding as filed also claimed `emit_themes`; verification established that `Config.zig:525-529`
defaults `emit_themes` to true unconditionally, so rustty's unconditional themes install matches the
oracle default (only the opt-out flag is absent). The surviving defect is terminfo/termcap.

Fix: add `--emit-terminfo/--emit-termcap` flags to `xtask resources` defaulting to the oracle's
profile-dependent values.

#### S64-F9 — Android lib-vt arm (16 KiB max page size, NDK paths) not represented

**Status: CONFIRMED** · kind: missing-branch · severity: low
Oracle: `src/build/GhosttyLibVt.zig:261-278` · rustty: `CMakeLists.txt:391-440`

For Android ABIs the oracle sets `link_z_max_page_size = 16384` (required for Android 15+) and adds NDK
include/lib paths; `test.yml:890-936` actually builds lib-ghostty-vt for three Android targets. rustty's
CMake cross-target helper and cargo configuration have no Android arm and pass no
`-z max-page-size=16384`. Latent: rustty ships no Android target today.

Evidence: `GhosttyLibVt.zig:273-278`; grep for `max-page-size` across crates/xtask/CMakeLists finds only unrelated
terminal page-size constants.

Fix: add an Android arm to `rustty_vt_add_target` passing
`-C link-arg=-Wl,-z,max-page-size=16384` via `CARGO_TARGET_<T>_RUSTFLAGS`.

#### S65-F5 — Cross-platform MRI `ar -M` archive combiner not ported

**Status: DISPUTED** · kind: missing-file · severity: low (presence lens regraded from medium)
Oracle: `src/build/combine_archives.zig:14-55` · rustty: `xtask/src/main.rs:598-616`

`combine_archives` builds a CREATE/ADDLIB/SAVE/END MRI script and pipes it to `zig ar -M` to fold
dependency archives into one fat static lib on every platform (used by GhosttyLib and GhosttyLibVt).
rustty only folds archives on macOS via `libtool -static` inside `xtask xcframework`; there is no
Linux/Windows combine step.

Evidence: `combine_archives.zig:28-44`; grep for `ADDLIB` / `ar -M` across rustty returns nothing;
`main.rs:598-616` uses `libtool -static`.

Fix: port `combine_archives` as an xtask subcommand driving `ar -M` (llvm-ar / zig ar) and use it for the
CMake static rustty-vt-c and Linux libs.

**Refuted by the semantic lens** — verbatim: "combine_archives.zig exists because Zig's static libs do not
bundle dependency archives; every rustty consumer path is covered otherwise. On macOS rustty ports the step
(xtask/src/main.rs:592-615 + archives.rs:1-24, explicitly citing GhosttyLib.zig:52-57 and LibtoolStep
normalizeArchive). Off macOS, crates/rustty-simd/build.rs:160 uses `cc::Build::compile`, which emits
`cargo:rustc-link-lib=static=rustty_simd_kernels` with the default `+bundle`, so the objects are already inside
the rlib and hence inside rustc's `librustty_vt_c.a` — no separate MRI/ar step is needed for self-containment.
The finding's cited evidence (CMakeLists.txt:231-247 libintl/gettext) is about an external SYSTEM library, which
the oracle's combine step would not fold either (and rustty's archives.rs:63-78 deliberately refuses system
archives). No substantiated observable gap."

#### S65-F6 — GTK x11/wayland target defaults are static (both on) instead of probed from `pkg-config --variable=targets gtk4`

**Status: CONFIRMED** · kind: behavioral-difference · severity: low
Oracle: `src/build/gtk.zig:9-27` · rustty: `crates/rustty/Cargo.toml:38-59`

The oracle derives the default x11/wayland build arms by grepping pkg-config's `targets` output (missing
pkg-config ⇒ both false). rustty defaults both features on unconditionally; on a Wayland-only or X11-only
distro GTK the default build compiles (and can enter) the absent backend's winproto arm unless the
packager opts out with `--no-default-features`. Runtime winproto selection is by actual GdkDisplay type in
both trees, so the difference is a build-time default.

Evidence: `gtk.zig:14-26`; `crates/rustty/Cargo.toml:38` and the comment at 41-56 acknowledging the deviation.

Fix: probe `pkg-config --variable=targets gtk4` in `rustty-apprt/build.rs` and emit cfgs
(`rustty_gtk_x11`/`rustty_gtk_wayland`) gating the winproto arms when the features are default-selected.

#### S65-F7 — wasm patcher accepts extra trailing arguments instead of failing the `args.len != 3` usage check

**Status: CONFIRMED** · kind: missing-error-path · severity: low
Oracle: `src/build/wasm_patch_growable_table.zig:21-26` · rustty: `xtask/src/main.rs:771-792` (dispatch at `:222-231`)

The oracle exits(1) with the usage error when argc != 3 (too few OR too many). The rustty subcommand only
errors when fewer than two args are present; surplus args are silently ignored, so
`xtask wasm-patch-growable-table a.wasm b.wasm junk` succeeds where the oracle exits 1.

Evidence: `wasm_patch_growable_table.zig:22-26`; `main.rs:775-781` two `.next().ok_or(usage)?` calls with no
exhaustion check.

Fix: after taking input/output, `if args.next().is_some() { return Err(usage) }`.

#### S65-F8 — `jetbrains_mono_bold` / `_italic` / `_bold_italic` anonymous imports left as a DEFERRED comment

**Status: CONFIRMED** · kind: stub-or-unimplemented · severity: low
Oracle: `src/build/SharedDeps.zig:623-635` · rustty: `crates/rustty-font/src/embedded.rs:62-65`

SharedDeps wires six JetBrains Mono anonymous imports; `src/font/embedded.zig:15-19` re-exports `bold`,
`italic` and `bold_italic`. rustty's `embedded.rs` ships regular/variable/variable_italic but leaves the
other three as a commented `PORT NOTE (DEFERRED …)` block, and the vendored `res/` has no
`JetBrainsMono-Bold*.ttf`. No oracle use-site of `embedded.bold`/`.italic`/`.bold_italic` exists (grep of
`ghostty/src` and `ghostty/macos` is empty), so this is a missing public constant and an explicit
deferral in production code rather than a runtime behaviour change.

Evidence: `SharedDeps.zig:624-635`; `embedded.rs:62-65`.

Fix: vendor `JetBrainsMono-{Bold,Italic,BoldItalic}.ttf` into `crates/rustty-font/res` and add the three
`include_bytes!` consts.

#### S65-F9 — dcimgui `backend-osx` (`imgui_impl_osx.mm`) not compiled on macOS

**Status: DISPUTED** · kind: missing-branch · severity: low
Oracle: `src/build/SharedDeps.zig:597-614` · rustty: `crates-sys/dcimgui-sys/build.rs:150-200`

The oracle passes `.@"backend-osx" = (os == .macos)` so `pkg/dcimgui` compiles `imgui_impl_osx.mm` and
exports the `ImGui_ImplOSX_*` symbols into libghostty. dcimgui-sys compiles the core TUs, the OpenGL3
backend (non-macOS) and a separate ObjC++ archive for `imgui_impl_metal.mm`, but no osx backend TU. No
use-site of `ImGui_ImplOSX_*` was found in `ghostty/src` or `ghostty/macos`.

Evidence: `SharedDeps.zig:602`; `pkg/dcimgui/build.zig:146`; `grep -rni osx crates-sys/dcimgui-sys/` → nothing.

Fix: add a second ObjC++ `cc::Build` for `backends/imgui_impl_osx.mm` on macOS if symbol-set parity of the
shipped library is required.

**Refuted by the semantic lens** — verbatim: "Checked SharedDeps.zig:597-614 (`.@\"backend-osx\" = os.tag == .macos`)
and pkg/dcimgui/build.zig:143-154 (compiles imgui_impl_osx.mm, installs imgui_impl_osx.h). Then grepped
ghostty/src, ghostty/macos and ghostty/include for `ImGui_ImplOSX`/`imgui_impl_osx`/`ImplOSX`: zero hits — no
oracle use-site exists, and the header is not re-exported to any Ghostty consumer. This is Zig build plumbing in
a legitimately 'replaced' area (crates-sys/dcimgui-sys/build.rs, which does compile the Metal backend at lines
169-216 matching the only backend actually used). The delta is confined to unreferenced symbols in a static
archive with no observable behaviour, so it is not a functional-parity finding."

#### S66-F1 — mdgen section headers emit one extra trailing newline

**Status: CONFIRMED** · kind: behavioral-difference · severity: low
Oracle: `src/build/mdgen/mdgen.zig:22-27, 52-57, 87-92` · rustty: `crates/rustty-extra/src/mdgen.rs:103, 129, 155`

The reference multiline literals emit `"\n# CONFIGURATION OPTIONS\n\n"` (Zig adds no trailing newline
after the last line); rustty writes `b"\n# CONFIGURATION OPTIONS\n\n\n"` for all three headers
(CONFIGURATION OPTIONS, COMMAND LINE ACTIONS, KEYBIND ACTIONS). The generated markdown bytes differ by one
blank line per section; rendered roff/HTML is unaffected because consecutive blank lines collapse. The
port's own `COMMANDS_HEADER` in `webgen.rs:47` applies the no-trailing-newline rule correctly, so the
mdgen headers are inconsistent with the port's own reading.

Fix: change the three header literals to end with `"\n\n"` instead of `"\n\n\n"`.

#### S66-F2 — config.mdx preamble drops a paragraph and adds an extra blank line

**Status: DISPUTED** · kind: behavioral-difference · severity: low
Oracle: `src/build/webgen/main_config.zig:14-31` · rustty: `crates/rustty-extra/src/webgen.rs:38-44`

The reference preamble ends `"for.\n\nIn the future, we'll have a more user-friendly way to view and\norganize these options.\n\n"`
followed directly by the first `## ` heading. rustty's `CONFIG_HEADER` deliberately omits the "In the
future…" paragraph (documented under strip-ghostty-gag-prose) and ends with `"for.\n\n\n"` — with the
paragraph removed, the finder argues the faithful tail would be `"for.\n\n"`. The `editOnGithubLink`
retarget is a documented R2 identity deviation and is not the reported defect.

**Refuted by the semantic lens** — verbatim: "Read main_config.zig:14-31 and webgen.rs:44. The reference
literal's line list after \"for.\" is: \"\", \"In the future, we'll have a more user-friendly way to view and\",
\"organize these options.\", \"\", \"\". The deletion of the two prose lines (a documented, policy-sanctioned
deviation per strip-ghostty-gag-prose, which the finder itself accepts and does not report as the defect)
leaves the line list \"...for.\", \"\", \"\", \"\" — which joins to exactly \"for.\\n\\n\\n\", i.e. rustty's
CONFIG_HEADER tail. So the 'extra blank line' is not an independent error but an artifact of which blank
separator you consider deleted alongside the paragraph; the finder's claim that the faithful tail 'would be
\\n\\n' is one of two equally defensible readings, i.e. inconclusive. The editOnGithubLink retarget is an
explicitly sanctioned R2 identity deviation, not a finding. No functional difference in the generated mdx (a
blank line in markdown before a '## ' heading is inert)."

Fix (if adopted): end `CONFIG_HEADER` with `"for.\n\n"`.

## Performance-only differences (not findings)

- `Config.zig:113-130` defaults wasm builds to the `simd128` CPU feature; rustty has no `.cargo/config.toml`
  enabling `-Ctarget-feature=+simd128` for wasm32. Identical results, slower wasm hot paths.
- `Config.zig:414-422` strips ReleaseFast/ReleaseSmall binaries and drops unwind tables when stripped;
  rustty's `[profile.release]` sets no `strip`, so release binaries retain symbol tables — size/backtrace detail only.
- `GhosttyBench.zig` forces ReleaseFast for gen/bench regardless of `-Doptimize`; rustty relies on the
  caller choosing `--profile release-fast`.
- CombineArchivesStep/LibtoolStep: rustty additionally ranlib-normalizes every archive and derives the
  archive set from cargo JSON rather than a glob — stricter, same output.
- GhosttyZig wuffs wiring: `rustty-vt-c` does not enable the `wuffs` feature; kitty pixel ops use
  byte-exact scalar mirrors of the wuffs swizzles (verified by `wuffs_and_scalar_src_over_agree`).
- GhosttyResources terminfo: the oracle runs `tic -x -o <hashed-dir>/terminfo` then `cp -R` into `share/`;
  rustty runs `tic -x` directly into `share/` — same tree, one fewer copy.
- GhosttyResources templates: the oracle renders cmake templates via ConfigHeader and strips the generated
  first line with `tail -n +2`; rustty does plain `@TOKEN@` substitution — same output bytes.
- GhosttyWebdata/HelpStrings/main_build_data: the oracle spawns host executables and captures stdout;
  rustty calls the ported generator functions in-process — same bytes at the same install paths.
- CombineArchives + Libtool for the CMake path: cargo's staticlib already bundles the vendored SIMD
  archives, so no separate combine step is needed there.
- MetallibStep toolchain-absent case: rustty skips the metallib and falls back to runtime
  `newLibraryWithSource` over the same shader source (documented pixel-identical) where the oracle fails the build.
- SharedDeps system-integration toggles (`-fsys=…`) have no Cargo analogue; rustty always builds the
  vendored `-sys` crates — identical to the oracle's default configuration.
- Linux `/usr/lib/<triple>` library-path injection is replaced by pkg-config-driven link search in the
  gtk4-rs / `-sys` build scripts.
- `src/stb/stb.c` (stb_image_resize) is ported to Rust in `rustty-font-shaper/src/stb.rs`;
  `vendor/glad/src/gl.c` is replaced by the Rust GL loader in `rustty-renderer/src/opengl/gl.rs`.
- `wasm_patch_growable_table`: Zig `std.Io.Writer.Allocating` + arena leak replaced by `Vec<u8>`/`Cow`;
  pointer-identity return preserved via `Cow::Borrowed`.
- `requireZig`'s exact-minor pin becomes `rust-toolchain.toml channel = "1.93.1"` plus an MSRV `rust-version`.
- `mdgen.rs substitute` does a single-pass scan/copy instead of the reference's
  replacementSize+alloc+replace two-pass; output identical.
- mdgen/webgen build the whole document in a `Vec<u8>` instead of streaming through a buffered stdout
  writer; byte output is the same.
- `Pager`: a `BufWriter` owned by the returned writer (flushed on drop) instead of a caller-supplied buffer
  flushed in `deinit`; same bytes reach the pager/stdout.
- `action.rs detect_args` reads argv via `std::env::args_os` (lossless `as_encoded_bytes`) instead of an
  allocator-backed iterator; no fallible init to propagate.

## Refuted candidates

- **S63-F11 — pandoc man/html conversion replaced by an in-tree renderer; installDummy placeholder never emitted.**
  Both lenses: the ported `mdgen::render` writes the same artifacts at the same install paths
  (`res_misc.rs:145-196`), pandoc is a third-party host tool whose absence is documented, and always emitting
  real pages is a superset of the `installDummy` placeholder layout; no concrete byte diff was shown.
- **S64-F10 — No installable `helpgen` executable (`HelpStrings.install` / `-Demit-helpgen`).**
  Both lenses: the exe exists only to emit a Zig source module; rustty generates the same three help tables
  at build time into OUT_DIR (`rustty-config/build.rs`, `rustty-input/build.rs`, `rustty-helpgen`), and
  `emit_helpgen` defaults false. No user-visible behaviour attached.
- **S65-F4 — `-debug-symbols <dSYM>` never passed to `xcodebuild -create-xcframework`.**
  Both lenses: every oracle call site passes `dsym = null` (static libraries cannot have dSYMs —
  `GhosttyLib.zig:75-77, 205-207`; `GhosttyLibVt.zig:604-608`), so the branch is dead upstream and rustty's
  argument list matches the oracle's effective invocation.
- **S66-F3 — Embedded man-page fixture prose deviates from the reference fixtures.**
  Both lenses: all six deltas are real but each is an explicitly sanctioned deviation enumerated at
  `mdgen.rs:24-41` under the strip-ghostty-gag-prose and R2 identity policies; all functional content
  (options, flags, config paths) is intact and the finder itself filed it "for completeness, no fix required".
