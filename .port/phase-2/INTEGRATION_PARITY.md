# macOS-Integration Parity Report

Scope: the Swift-app integration tier — the ghostty_* C-ABI alias layer, main_c
ports, the xcframework packaging pipeline, resource generation, and the
host-view seam that crosses an apprt Surface's view into the renderer. Reference
oracle: Ghostty upstream. Rules: R1 (line/behavior fidelity), R2 (rustty
identity + ghostty_* aliases, macOS-first), R4 (acyclic crates via
renderer-defined accessor traits), Branch-A (ghostty_* alias canonical
rustty_*).

---

## 1. Verdict

The integration is **substantially faithful** to ghostty and the reused Swift
app renders a live terminal against it. Every runtime seam that matters on the
supported target — the 104 ghostty_* forwarders (zero symbol gaps either
direction, all pure pass-throughs), the Info/BuildMode/version ABI, the terminfo
DB (268/268 capabilities identical), completion/vim/sublime resources, the
content-scale plumbing, and the macOS layer-hosting attach (six operations, same
order, same values) — is behavior-identical to the reference. **No fidelity bug
crashes or corrupts the ported terminal logic.** The real divergences are all in
the *artifact-packaging* and *deferred-resource* layers, not in terminal
behavior: the shipped xcframework is not self-contained (patched at app-link
with system `-lz` / Homebrew `-lintl`). (The `share/{app}/shell-integration`
and `share/{app}/themes` trees were RESOLVED by WS1 [2026-07-23]: vendored in
`crates/rustty-extra/`, emitted by `xtask resources`, oracle-parity KAT'd —
see §2.1/§2.2.) The remaining divergence is a documented R5 deferral —
sanctioned and non-silent — but the packaging behavior it gates is genuinely
absent versus a stock ghostty build.
One true R1 dropped-mapping (host_view nulls the iOS uiview) exists but is
latent-to-unreachable on the macOS-first target. No blocker. — since fixed; see §2.6.

---

## 2. Confirmed Divergences (verified fidelity-bugs / missing-pieces)

Ranked by severity, then reachability.

### 2.1 [RESOLVED 2026-07-23, WS1] `share/{app}/shell-integration/` — now vendored + emitted (was: stubbed empty)
- **rustty**: `xtask/src/res_terminfo.rs:141-170` (`gen_ghostty_root`) writes only
  `share/ghostty/DEFERRALS.md`; no `shell-integration/` subtree emitted.
- **ghostty**: `src/build/GhosttyResources.zig:117-126` installs the full
  `src/shell-integration/` tree (bash/zsh/fish/elvish/nushell) into
  `share/ghostty/shell-integration/`, excluding `.md`.
- **Consequence**: at runtime `src/termio/shell_integration.zig:42-76` resolves
  scripts under `<resource_dir>/shell-integration/`; with the empty tree every
  lookup returns null and injection no-ops — **no OSC7/cwd reporting, prompt
  marking, cursor-shape restore, sudo wrapping, or title updates.** The single
  largest behavioral divergence in this tier. Documented R5 deferral (not
  silent), but documentation does not restore the behavior.
- **Fix sketch**: vendor ghostty's `src/shell-integration/` tree as rustty data
  (identity-oracle spelling) and copy it into `share/ghostty/shell-integration/`
  from `gen_ghostty_root`, mirroring `addInstallDirectory(exclude .md)`.
- **RESOLUTION (WS1, 2026-07-23)**: vendored at `crates/rustty-extra/shell-integration/`
  (R2-rebranded: `rustty.bash`, `rustty-integration`, `RUSTTY_*` env vars —
  matching `rustty-termio`'s `env_name()`/`APP_NAME` lookups), emitted to
  `share/{APP}/shell-integration/` by `xtask/src/res_shell_integration.rs`
  excluding `.md` (Tasks 13/14); WS1 added the dev-only oracle-parity KAT
  (`oracle_shell_integration_parity`, rebrand-mapped byte-compare, `bash-preexec.sh`
  verbatim), deleted the `DEFERRALS.md` placeholder emission, and verified the
  headless zsh injection smoke + bundle contents.

### 2.2 [RESOLVED 2026-07-23, WS1] `share/{app}/themes/` — now vendored + emitted (was: not shipped)
- **rustty**: `xtask/src/res_terminfo.rs:141-170` emits no `themes/`.
- **ghostty**: `src/build/GhosttyResources.zig:128-139` installs the
  `iterm2_themes` corpus into `share/ghostty/themes`, gated on `emit_themes`
  which defaults **TRUE** unconditionally (`src/build/Config.zig:431-435`
  `orelse true`) — unlike the Release-gated `emit_terminfo`/`emit_termcap`.
- **Consequence**: this session's freshly-built `list_themes` interactive picker
  and the `theme` config setting enumerate/apply an **empty set** at runtime.
- **Fix sketch**: vendor the iTerm2-Color-Schemes ghostty themes as rustty data
  and emit them to `share/ghostty/themes` in `gen_ghostty_root`.
- **RESOLUTION (WS1, 2026-07-23)**: the 534-file corpus is vendored verbatim at
  `crates/rustty-extra/themes/` (provenance: ghostty's pinned `iterm2_themes`
  tarball `ghostty-themes-release-20260608-160426-8c84dd1.tgz`; MIT attribution
  vendored in `LICENSE.md`, excluded from emission), emitted to
  `share/{APP}/themes/` by `xtask/src/res_themes.rs` excluding `.md` (Task 14);
  WS1 verified count=534 in the rebuilt app bundle and the regeneration is
  byte-reproducible from a clean `zig-out`. Live `+list-themes` CLI count check
  re-queued to WS4 (blocked on `rustty_cli_try_action`).

### 2.3 [RESOLVED 2026-07-28, WS9] Combined archive IS self-contained — full zlib (P3) + full gettext (WS9); `-lz`/`-lintl` both gone
- **rustty**: `xtask/src/main.rs:242-273` folds only whatever `.a` sit under
  `target/<t>/xcframework/build/*/out` (partial `-sys` slices) +
  `macos/build-rustty.sh:19-25` `OTHER_LDFLAGS="-lstdc++ -lz
  -L/opt/homebrew/opt/gettext/lib -lintl"`.
- **ghostty**: `SharedDeps.add()` (`src/build/SharedDeps.zig:274-286`,
  `:522-531`) accumulates the FULL vendored C-dep archives (zlib WITH deflate,
  gettext WITH dgettext, harfbuzz/freetype/oniguruma/…); `GhosttyLib.zig:52-57`
  + `CombineArchivesStep.zig:13-27` fold them all into
  `libghostty-internal-fat.a` → a genuinely self-contained archive linked with
  no system/Homebrew libs.
- **Consequence**: rustty's `libz.a` is inflate-only (no `_deflate`, which sentry
  needs) and gettext-sys is the localename slice only (no `_dgettext`), so the
  packaged xcframework has undefined `_deflate`/`_dgettext` and the app is forced
  to link system zlib + Homebrew gettext. (1) shipped `GhosttyKit.xcframework` is
  not portable/self-contained; (2) hard build dependency on
  `/opt/homebrew/opt/gettext` (breaks on non-Homebrew / CI / Intel-Homebrew
  hosts); (3) a *different* zlib/gettext implementation services the calls, plus
  a possible licensing shift (ghostty deliberately vendors libintl with an LGPL
  comment). Documented in `macos/MILESTONE.md:40-43`.
- **Fix sketch**: build/vendor full deflate-capable zlib + full gettext static
  archives into the `-sys` crates so their `out/` archives carry
  `_deflate`/`_dgettext`, then drop `-lz`/`-lintl` from `build-rustty.sh`.
- **RESOLUTION**: the zlib half was already closed by the P3 pass (`crates-sys/
  zlib-sys` vendors `deflate.c`/`trees.c`/`compress.c`; `-lz` dropped) — that
  part of the text above was stale when written. WS9 closed the gettext half:
  `crates-sys/gettext-sys` now vendors the reference's FULL `pkg/libintl/
  build.zig` `srcs` set (30 TUs from gettext 0.24, incl. the pre-generated Bison
  `plural.c`) and emits `librustty_intl.a` defining `_dgettext`/
  `_bindtextdomain`/`_textdomain`. `-L/opt/homebrew/opt/gettext/lib -lintl` was
  removed from BOTH link sites — `macos/build-rustty.sh` `OTHER_LDFLAGS` (now
  just `-lstdc++`) and `crates/rustty-os/build.rs` (the WS4-added block; review
  finding **WS4-M1**, which also hardcoded the ARM-Homebrew prefix and missed
  Intel Homebrew — RESOLVED here). Proof:
  `.port/tools/gates/no_homebrew_gettext_gate.sh` renames Homebrew's
  linker-facing `libintl.{dylib,a}`, verifies `-lintl` no longer links, runs a
  clean `macos/build-rustty.sh`, and asserts `nm`/`otool` symbol conditions on
  both `libRusttyKit.a` and the app binary. Homebrew `msgfmt` remains a
  build-TIME tool dependency of `xtask resources` — the oracle needs it too
  (`GhosttyI18n.zig`) — and is out of scope here.

### 2.4 [RESOLVED 2026-07-28, WS9] Archive set is now an explicit cargo-derived dependency list, not a filesystem glob
- **rustty**: `xtask/src/main.rs:243-256` — `read_dir(build_dir)` then one-level
  `<crate>/out/*.a` per crate, pushing every `.a` found.
- **ghostty**: `GhosttyLib.zig:52-53` — `deps.add(lib)` + `getEmittedBin()`, an
  exact enumerated list.
- **Consequence**: (1) a `-sys` crate emitting its archive in a nested OUT_DIR
  subdir is silently missed (non-recursive glob) → undefined symbols; (2)
  multiple hash-suffixed build dirs leave >1 matching `.a` → libtool folds
  stale/duplicate objects → duplicate-symbol or wrong-version. The combine is a
  function of leftover filesystem state, not a deterministic dep list.
- **Fix sketch**: derive the set via `cargo build --message-format=json`
  (staticlib + build-script archives it reports); de-dup per crate.
- **RESOLUTION**: `xtask/src/archives.rs` (`parse_artifacts` +
  `NativeArchiveRequest::resolve`) derives the set from `cargo build
  --message-format=json` — the staticlib target's own `filenames`, plus every
  `cargo:rustc-link-lib=static=<name>` a build script declared, resolved against
  that same script's `cargo:rustc-link-search` paths and de-duplicated per
  (package, lib). A nested OUT_DIR now resolves by construction (cargo reports
  the nested dir), stale hash-suffixed build dirs are never consulted, and an
  archive that cannot be located is a HARD ERROR instead of a silent omission.
  Unit-tested in `xtask/src/archives.rs`.

### 2.5 [RESOLVED 2026-07-28, WS9] ranlib per-source normalization runs before libtool
- **rustty**: `xtask/src/main.rs:259-263` feeds each source `.a` straight to
  `libtool -static -o` with no pre-pass.
- **ghostty**: `LibtoolStep.zig:36-78` `normalizeArchive()` runs `cp src dst &&
  ranlib dst` on EVERY source archive first, with a comment that newer Xcode
  libtool can drop 64-bit members when the input layout is unexpected.
- **Consequence**: rustty combines many cc/ar-produced `-sys` archives — exactly
  the layout the comment warns about — with no normalization. On an affected
  Xcode, libtool drops members → undefined symbols at app-link or runtime,
  nondeterministically. rustty is *more* exposed than the oracle.
- **Fix sketch**: copy each archive to a temp path, run `/usr/bin/ranlib`, then
  pass the ranlib'd copies to libtool (mirror `normalizeArchive`).
- **RESOLUTION**: `xtask::archives::normalize_archive` ports
  `LibtoolStep.zig`'s `normalizeArchive` — every source archive is copied to
  `target/<t>/xcframework/normalized/<i>-libRusttyKit.a` and `/usr/bin/ranlib`'d
  before `libtool -static` sees it. Unit-tested (`ar -t` shows `__.SYMDEF`
  appearing on the normalized copy and the source left unmodified) and asserted
  end-to-end by the acceptance gate.

### 2.6 [LOW] `host_view()` drops the iOS→uiview mapping that Metal.init performs (R1 dropped code path) — ✅ FIXED (uiview arm restored pre-WS3; WS3 adds the platform_host_view helper + ios_platform_maps_uiview regression KAT)
- **rustty**: `crates/rustty-apprt/src/embedded.rs:2228-2234` — `host_view()`
  matches only `Platform::MacOS(m) => m.nsview` and returns `null_mut()` for
  `Platform::IOS` via the `_ =>` catch-all.
- **ghostty**: `src/renderer/Metal.zig:99-102` — `switch { .macos => v.nsview,
  .ios => v.uiview }`; both darwin arms yield a real view.
- **Consequence**: `Platform::IOS{uiview}` is reachable at runtime
  (`embedded.rs:838-851`, platform_tag==2, non-null uiview enforced) even on a
  macOS build; `surface.rs:2299-2301` feeds the null to `attach_host_view`,
  `generic.rs:2216-2218` early-returns on null → blank surface, whereas ghostty
  attaches. A silent behavior substitution in the exact seam that claims to
  mirror Metal.init.
- **Reachability (why LOW)**: the real macOS Swift app always creates `.macos`
  surfaces (tag=1/nsview), so the null branch is never taken; the only case the
  IOS value matters is the degenerate macOS-build + iOS-tagged-surface cross
  config; on a genuine iOS build `attach_host_view` is a whole-function no-op
  (`#[cfg(target_os="macos")]`), so ghostty's iOS addSublayer host-attach is
  unported anyway.
- **Fix sketch**: `match &self.platform { Platform::MacOS(m) => m.nsview,
  Platform::IOS(i) => i.uiview }` (drop the catch-all). The renderer's iOS
  addSublayer belongs in `attach_view`'s comment-only `.ios` arm
  (`metal_impl.rs:349-353`) when that tier lands.

### 2.7 [LOW] i18n disabled — `rustty_translate` drops dgettext; `share/locale` is a `.placeholder` stub
- **rustty**: `crates/rustty-apprt-c/src/lib.rs:947-952` returns msgid
  unchanged; `xtask/src/res_misc.rs:89-96` writes `share/locale/.placeholder`
  only.
- **ghostty**: `src/main_c.zig:150-152` → `src/os/i18n.zig:64-67` calls
  `dgettext`; `build_config.i18n` defaults TRUE; `GhosttyI18n.zig` compiles `.po`
  → `.mo`.
- **Consequence**: in a localized locale ghostty returns a translated string
  while rustty always returns the English msgid — a real observable loss for
  non-English users. Faithful only while no catalog is bound (the deferred
  state). Documented deferral; not runtime-critical for a terminal window.


> **OWNER [WS19 T19, 2026-07-29]: WS13 (i18n & docs).** Flagged by gate G-P2 as a row with
> no disposition and no owner. WS13's Goal already names outcome (2) as "`rustty_translate`
> … stops returning the identity stub and forwards to real `dgettext`, which WS9's vendored
> libintl now makes linkable" — so the work is owned and planned; this row simply never
> said so, which is how a planned item still reads as an orphan to anyone sweeping the
> ledgers. Fix-list item 7 below carries the same owner.

### 2.8 terminfo SOURCE (`ghostty.terminfo`) + termcap never emitted — **RECLASSIFIED as a permanent Justified R1 Divergence [WS19 T14, 2026-07-29 — audit row O20]**
- **MOVED.** The authoritative entry, with its 4-criterion basis, is now
  "terminfo SOURCE + termcap files never emitted (Debug builds only)" in
  `.port/phase-2/DEFERRALS.md`'s `# Justified R1 Divergences` section. Fix-list item 8
  below is DELETED: it was marked "(Optional)", and "optional" is not a disposition — a
  row nobody owns and nobody will schedule reads as open work to any closure sweep.
- The facts are unchanged and are restated at the divergence entry: rustty compiles the
  DB via `tic` (`xtask/src/res_terminfo.rs:50-90`) but installs neither the `.terminfo`
  source nor a `.termcap`; ghostty installs both when `emit_terminfo`/`emit_termcap`,
  which **default FALSE for Release, TRUE for Debug** (`Config.zig:410-429`). **The
  shipping app is Release, so rustty matches the oracle exactly in the shipped
  configuration**; the divergence is confined to a Debug build and to informational
  files, while the COMPILED DB — what TERM resolution actually reads — IS produced.

### 2.9 [LOW] `host_view` field access on `Platform::MacOS` may not compile on non-darwin — ✅ RESOLVED (WS3: `platform_host_view` arms are `#[cfg]`-gated per target with a `non_darwin_returns_null` regression KAT; Linux both-platform gate GREEN on main 57a63d0 — `cargo build --workspace` exit 0 on aarch64 Fedora, 2026-07-23)
- **rustty**: `embedded.rs:2230` `Platform::MacOS(m) => m.nsview` in an
  un-cfg-gated impl; on non-darwin `MacOS` is a fieldless unit struct
  (`embedded.rs:711-712`).
- **Consequence**: if rustty-apprt's embedded module is compiled on Linux,
  `m.nsview` on a fieldless struct fails to compile. NOT confirmed (review is
  read-only; no build run). Worth a `cargo build` on a non-darwin target.
- **Fix sketch**: if it fails, cfg-gate the MacOS arm body per `target_os`,
  returning null on non-darwin.

---

- **WS13 follow-through (2026-07-30).** WS9 made `dgettext` LINKABLE; WS13 made
  it REACHABLE. `rustty_translate` (`crates/rustty-apprt-c/src/lib.rs`) no
  longer returns the identity stub — it forwards to
  `rustty_os::i18n::gettext_`, proven by
  `crates/rustty-apprt-c/tests/i18n_translate.rs`, which compiles `po/de.po` and
  `po/ja.po` with `msgfmt`, binds the domain, and asserts a German hit, a
  Japanese hit that differs from it, a `LANG=C` passthrough, and a
  stale-`Ghostty`-msgid passthrough. FAIL arm demonstrated: restoring the
  identity stub makes it fail with `left: "About Rustty" right: "Über Rustty"`.
  Also fixed in the same pass: three `.blp` msgids still spelled "Ghostty" and
  therefore matched NO catalog, and all 25 code-side strings the reference wraps
  in `i18n._()` were unwrapped literals — measured, `xgettext` extracted **0**
  msgids from any `.rs` file before WS13 and **25** after.
  Homebrew `msgfmt` remains the build-TIME tool this section already sanctions;
  **no link-line change was made**, so `.port/tools/gates/no_homebrew_gettext_gate.sh`
  was deliberately NOT re-run — it renames Homebrew's `libintl` host-wide, has
  already broken two concurrent lanes mid-run, and two lanes were live. The
  property was confirmed statically instead: `grep -rn homebrew
  crates/rustty-os/build.rs macos/build-rustty.sh` → no match (no link search),
  and the only `opt/homebrew` reference in `xtask/src/res_misc.rs` is
  `msgfmt_path()`'s PATH prepend.

## 3. Sanctioned Divergences (deliberate R2 / R4 / Branch-A restructures)

Each attested behavior-preserving unless flagged.

1. **Branch-A alias layer** (`ghostty_alias.rs` × both crates) — 104 forwarders,
   each body exactly `unsafe { crate::rustty_<name>(args…) }`, byte-identical arg
   order, zero symbol gaps (full comm-diff vs `include/ghostty.h:1064-1200`).
   **Behavior-preserving.**

2. **R4 offscreen→attach reorder** (`core/src/surface.rs:2290-2302` vs
   `src/Surface.zig:551-558`) — renderer built offscreen then handed the view at
   `finalizeSurfaceInit` via the `host_view()`/`host_content_scale()` accessor
   trait (renderer never names an apprt type). Verified the same nsview pointer +
   content_scale.x reach the same layer-hosting messages, attach completes on the
   main thread before the render thread spawns, no frame presented before then.
   **Behavior-preserving** (only *when* the layer attaches shifts, unobservably).

3. **`rustty_info` collapses Zig's 4 build modes to Debug xor ReleaseFast**
   (`lib.rs:895-910`) — Rust has no 4-mode reflection; an optimized build always
   reports ReleaseFast. ghostty's default release IS ReleaseFast, so it matches
   in practice; the app uses `build_mode` only for display/logging. Cosmetic;
   version/version_len faithful. **Behavior-preserving for the two modes rustty
   can be built in.**

4. **`rustty_translate` identity return** (§2.7) — **FLAGGED**: behavior-
   preserving *only* while no message catalog is bound. In a localized locale it
   loses translations. Sanctioned deferral, scope = "no translations applied".

5. **`rustty_cli_try_action` — both null AND non-null paths now faithful**
   (`lib.rs`, WS4 2026-07-23). `rustty_init` runs `detectArgs` over the stored
   argv (`detect_from_argv`) and stashes the result (presence in
   `rustty_core::STATE.action: Option<()>`, the concrete value in the
   crate-local `CLI_ACTION` stash); `rustty_cli_try_action`'s `Some` arm now
   runs the action and exits with its code, matching ghostty's
   `ghostty_cli_try_action`. The null-action GUI path (`None => return`) is
   unchanged and still byte-faithful. **Behavior-preserving; no residual gap.**

6. **`inspector_metal_shutdown` returns void** (`ghostty_alias.rs:502-505`) — the
   upstream header (`ghostty.h:1195`) declares `bool` but the exported Zig symbol
   (`embedded.zig:2243`) returns `void`; rustty follows the *source* (true ABI).
   **Fidelity-correct.**

7. **Single macos-arm64 slice; arch hardcoded aarch64** (`main.rs:209`) — matches
   ghostty's `.native` Target shape; omits the `.universal` x86_64/iOS/iOS-sim
   lipo slices. Documented scope reduction. **Behavior-preserving on the arm64
   macOS host**; nuance — ghostty's native uses HOST arch, rustty hardcodes
   aarch64 (divergent only on an x86_64 host).

8. **xcframework staged dist/→macos/** (`main.rs:298-299` + `build-rustty.sh`) vs
   ghostty's direct `macos/` out_path — same delete-before-write discipline, Xcode
   project resolves to `macos/GhosttyKit.xcframework`. Extra copy hop only.
   **Behavior-preserving.**

9. **Combined-archive basename `libGhosttyKit.a`** vs `libghostty-internal-fat.a`
   — cosmetic; Swift import resolves via module NAME (GhosttyKit), not the
   archive filename. **Behavior-preserving.**

10. **`tic` invoked directly into `share/terminfo`** (`res_terminfo.rs:96-112`) vs
    ghostty's tic-to-temp + `cp -R` (a workaround for Zig InstallDir mangling
    symlinks). rustty has no such intermediary; `tic` writes alias entries
    natively. Output tree equivalent (gated by `compiled_entry_exists`).
    **Behavior-preserving.**

11. **bat sublime-syntax from embedded golden copy** (`res_misc.rs:57-58
    include_bytes!`) vs ghostty's live `+sublime` generator — bytes pinned
    identical by `sublime::syntax`'s own golden test. **Behavior-preserving**;
    latent drift risk only if that golden test were deleted.

---

## 4. Attestations (confirmed faithful)

- **Alias coverage complete + pure pass-through** — 89 apprt-c + 15 config + 1
  string_free, zero gaps either direction; sampled signatures byte-match
  `ghostty.h` (arg count/type/return); ABI lowerings (opaque `*mut`,
  `enum(c_int)`, `const char*`, by-value structs) all identical.
- **Info struct layout + BuildMode ordinals + version source** — `{c_int, *const
  u8, usize}` bit-matches `ghostty_info_s`; BuildMode 0/1/2/3 order preserved;
  version from `&'static str` (VERSION = "1.3.2-dev" under identity-oracle),
  read as non-NUL bytes for exactly version_len.
- **Darwin gating** — 6 Darwin-only forwarders gated `macos|ios`;
  `set_window_background_blur` left ungated exactly as `ghostty.h`.
- **Metal attach (device/queue/storage_mode/max_texture_size)** — the offscreen
  half (`init_device`, `metal_impl.rs:231-253`) reproduces ghostty's
  view-independent init half verbatim; nothing view-dependent missed.
- **attach_view layer-hosting** — six operations, same sequence (setLayer BEFORE
  setWantsLayer, load-bearing), same values as `Metal.zig:111-149`; byte-for-byte
  the same as rustty's own faithful `init`.
- **Deferred attach is behavior-preserving** — nothing between init and
  thread-spawn reads the layer as Some (resize mails IO only; Metal has no
  finalizeSurfaceInit; surfaceSize/loopEnter run on the render thread after
  attach).
- **content_scale source/units/cast** — same field (`content_scale.x`, not y),
  same f32→f64 widening (`.into()` vs `@floatCast`), set once at construction,
  provably identical value.
- **No first surfaceSize/resize in Metal.init** — init sizes from `options.size`;
  the offscreen split defers nothing on the size axis.
- **Single-layer allocation, no leak** — exactly one IOSurfaceLayer for the
  windowed path, as in ghostty.
- **Null-view rejection** — `NSViewMustBeSet`/`UIViewMustBeSet`/`InvalidEnumTag`/
  `UnsupportedPlatform` all lowered faithfully (objc null-check → raw-ptr
  `is_null()`), tag mapping macos=1/ios=2 line-for-line.
- **Terminfo capability table** — 268/268 names identical (empty set-diff both
  directions); acsc/Smulx/Setulc/sgr byte-for-byte; identity-oracle names
  `xterm-ghostty|ghostty|Ghostty`.
- **Completion/vim/sublime install paths + content** — every `install_subdir`
  string matches character-for-character; single generated tree reused for
  vim+nvim as in the reference.
- **`share/man` placeholder** — byte-exact to ghostty's `installDummy` ("emit-docs
  not true so no man pages"); Release app takes the same branch.
- **module.modulemap + headers staging** — module name GhosttyKit, umbrella
  `ghostty.h`, `export *` all match; stages only `ghostty.h` (avoiding the same
  umbrella-warning trap ghostty documents).
- **panic=abort profile** — preserves ghostty's no-unwind-across-C-ABI contract;
  overflow-checks=true matches ReleaseSafe trap semantics.
- **xcodebuild -create-xcframework** — flag-for-flag match (`-library`,
  `-headers`, `-output`, preceding recursive delete); static-lib dSYM correctly
  omitted (`-debug-symbols` passed by neither).

---

## 5. Fix List (ordered real fixes)

Terminal runtime logic needs no fix. The following restore missing/absent
behavior or harden the artifact, ordered by user-visible impact then robustness:

1. ~~**Ship `share/ghostty/shell-integration/`** (§2.1, HIGH)~~ — ✅ DONE
   (WS1, 2026-07-23; see §2.1 RESOLUTION).
2. ~~**Ship `share/ghostty/themes/`** (§2.2, HIGH)~~ — ✅ DONE
   (WS1, 2026-07-23; see §2.2 RESOLUTION).
3. ✅ DONE (WS9, 2026-07-28) — **Make the xcframework self-contained** (§2.3, MEDIUM) — vendor full
   deflate-zlib + full gettext into the `-sys` crates; drop `-lz`/`-lintl` from
   `build-rustty.sh`. Removes the hard Homebrew/host dependency.
4. ✅ DONE (WS9, 2026-07-28) — **Deterministic archive set** (§2.4, MEDIUM) — derive from `cargo build
   --message-format=json`, de-dup per crate; replaces the fragile glob.
5. ✅ DONE (WS9, 2026-07-28) — **ranlib-normalize before libtool** (§2.5, MEDIUM) — mirror
   `normalizeArchive` to defend against newer-Xcode member dropping.
6. ✅ DONE (WS3) — **Map `Platform::IOS(i) => i.uiview` in `host_view`** (§2.6, LOW) — trivial R1
   fix; drop the null catch-all. (Full iOS attach path is a separate deferred
   tier.)
7. **Wire i18n runtime seam** (§2.7, LOW) — real `dgettext` link + `.po`/`msgfmt`
   pipeline when i18n is un-BROKEN. **OWNER [WS19 T19, 2026-07-29]: WS13 (i18n & docs)**,
   which owns exactly this as outcome (2) of its Goal; the `.po`/`msgfmt` half is already
   done (`xtask/src/res_misc.rs` `gen_locale`) and WS9's vendored libintl made the
   `dgettext` link possible, so what remains is the call-site wrapping and dropping the
   identity stub. The trigger has already fired — this is scheduled work, not a deferral.
8. ~~**(Optional) Debug-gate terminfo source + termcap** (§2.8, LOW) — exact-mirror
   the reference's Debug build.~~ **DELETED [WS19 T14, 2026-07-29 — audit row O20].**
   "(Optional)" is not a disposition: the row had no owner, no trigger and no schedule,
   yet read as open work. §2.8 is now a permanent **Justified R1 Divergence** in
   `DEFERRALS.md` with a 4-criterion basis — rustty matches the oracle exactly in the
   SHIPPED (Release) configuration, and the trigger, if a Debug-parity need ever
   appears, is named there along with the two `GhosttyResources.zig` hunks to mirror.
9. ✅ DONE (WS3) — **(Verify) non-darwin compile of `host_view`** (§2.9, LOW) — run `cargo build`
   on a non-darwin target; cfg-gate the MacOS arm if it fails.
