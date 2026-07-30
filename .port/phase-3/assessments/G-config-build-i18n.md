# Phase 3 assessment — Area G: config / CLI / build / pkg / i18n / packaging

Range: `4749c4e93` (2026-06-20, pin) → `6ad1fe7d8` (2026-07-29, oracle head). 365 commits.
Area footprint: `src/config` +540/−286 · `src/cli` +418/−294 · `src/build` +905/−319 ·
`pkg` +859/−399 · `po` +1454/−1062 · `src/synthetic` +174/−16 · `.github` +480/−224 ·
`nix`/`flatpak`/`build.zig.zon*` (root packaging).

**Every verdict below was re-derived from the diff and from the rustty tree. No ledger row was
trusted.** Two `MAIN_PARITY.md` rows in this area were found mis-classified (§8). Nothing was
implemented; no oracle `git checkout` was performed.

---

## 1. Summary

### 1.1 The headline

**The area's raw line counts are ~10× larger than its real porting debt.** Two upstream commits —
`e8525c0fd` (*Update to Zig 0.16.0*) and `f2a7652ab` (*mitchell's touchups*, its Zig-0.16 regression
follow-up) — account for **81 % of `pkg/` insertions and 99 % of its deletions**, and **57 %/97 % of
`src/build`**. Almost the whole `src/cli` diff and roughly half the `src/config` diff are the same
migration (`std.fs` → `std.Io`, `global_state` → `global.io()/environ()/args()`,
`ArrayListUnmanaged`→`ArrayList`, `@Type` → `@Enum`/`@Struct`, `captureStdOut()` → `captureStdOut(.{})`).
And **85 % of the `po/` diff (2142 of 2516 changed lines) is `#:` reference-comment renumbering.**

Measured, not estimated:

```
$ git show --numstat e8525c0fd -- pkg        →  694 / 394   (of 859 / 399 total)
$ git show --numstat e8525c0fd -- src/build →  519 / 309   (of 905 / 319 total)
$ git diff 4749c4e93..origin/main -- po | grep -c '^[+-]#:'  →  2142
$ git diff 4749c4e93..origin/main -- po | grep '^+msgstr' | grep -v '^+msgstr ""'
    → exactly ONE line:  +msgstr "关闭分屏"
```

### 1.2 Per-sub-area counts

Counted over **substantive** upstream changes (a Zig-language/toolchain migration hunk is not a
substantive change for a Rust port; those are folded into the N/A column).

| Sub-area | ABSENT | PARTIAL | PORTED | N/A | Notes |
|---|---|---|---|---|---|
| `src/config` (13 files) | 6 | 2 | 3 | rest (~250 lines) | Real debt: option rename + `Limit` + `-lines`, `gtk-horizontal-tab-scroll`, 2 doc rewrites, url regex fix |
| `src/cli` (24 files) | 1 | 0 | 1 | 22 files wholly | No new CLI actions. Only real item is a cosmetic error string |
| `src/build` (29 files) | 6 (5 cross-area) | 0 | 0 | 9 + Zig-0.16 bulk | Real debt is *dependency version bumps*, not build logic |
| `pkg` (46 files) | 0 | 0 | 0 | all 46 | Includes both DELETIONS — rustty already has the structural equivalent |
| `po` (32 files) | 3 msgids + 1 msgstr | 0 | 0 | 85 % of lines | Only ONE real translation upstream in the whole range |
| `src/synthetic` (6 files) | 4 | 0 | 0 | 2 | Clean, self-contained new generator |
| `.github` (13 files) | 0 | 0 | 0 | all 13 | 1 needs a caveat, not a port (§6.1) |
| `nix`/`flatpak`/`build.zig.zon*` | 0 | 0 | 0 | all | Subsystems absent from the port by documented decision |

**Total real porting debt in area G: 20 items** — 6 of them cross-area dependency bumps that other
areas own, 3 of them documentation-only, 4 of them a single self-contained new file pair.
**Everything in `pkg/`, `.github/`, `nix/`, and `flatpak/` is N/A, and `po/` is 99 % N/A.**

### 1.3 What was covered exhaustively vs sampled

**Exhaustive (every hunk of every file inspected):**
- `src/config/**` — all 13 files. `Config.zig` was read via its complete 46-hunk map with every
  substantive hunk read in full; the other 12 files were diffed and read end-to-end.
- `src/synthetic/**` — all 6 files, full diff read.
- `po/**` — exhaustive by *mechanical enumeration*: every `+msgid`/`-msgid` and every `+msgstr` line
  across all 32 files was extracted and counted; `com.mitchellh.ghostty.pot` was read in full;
  `be.po` was read as the representative for the non-reference residue and confirmed to be
  msgmerge rewrapping + untranslated additions only.
- `src/cli/**` — file-and-hunk-level exhaustive (the full 24-file hunk map was enumerated); the
  substantive hunks were read in full. The mechanical Zig-0.16 hunks were classified by
  pattern-filtered grep rather than read line-by-line.
- `.github/**`, `nix/**`, `flatpak/**`, `build.zig.zon*` — exhaustive at file level; `test.yml`'s
  full diff and `build.zig.zon`'s complete dependency table were read.

**Sampled (commit-level attribution is exhaustive; per-file diffs are not):**
- `src/build/**` — every commit touching it was enumerated and attributed. Read in full:
  `Config.zig`, `GhosttyI18n.zig`, `GhosttyResources.zig`, `combine_archives.zig`, `GitVersion.zig`,
  `mdgen/*`, `webgen/*`, `libsystem_override.sh`. **NOT read line-by-line:** `SharedDeps.zig`
  (+293), `GhosttyLibVt.zig` (+203), `uucode_config.zig` (+203, read partially), `GhosttyLib.zig`,
  `GhosttyExe.zig`, `GhosttyXcodebuild.zig`, `LibsystemOverrideStep.zig`, `XCFrameworkStep.zig`,
  `GhosttyDist/Docs/Bench/Zig/FrameData.zig`, `HelpStrings.zig`, `UnicodeTables.zig`,
  `wasm_patch_growable_table.zig`, `gtk.zig`. All of these are attributed to `e8525c0fd`/`f2a7652ab`
  /`1fe1b2d23`/`d65cb5128`/`84254a9d8` by `git log --oneline <range> -- <file>`; the *risk of this
  sampling* is that a non-Zig-0.16 hunk hides inside a Zig-0.16 commit (as `omitFramePointer` did
  inside `f2a7652ab`). Rated LOW-MEDIUM — see §9 open question OQ-3.
- `pkg/**` — commit attribution exhaustive + numstat quantified. Read: `harfbuzz/build.zig`
  (partially), the wuffs define list, `libintl`/`zlib`/`oniguruma/build.zig`,
  `apple-sdk/include/math.h`, plus all three apple-sdk commit messages. **NOT read:** `dcimgui`,
  `breakpad`, `fontconfig`, `freetype`, `glslang`, `gtk4-layer-shell`, `highway`, `libpng`,
  `libxml2`, `macos` (+111), `sentry`, `simdutf`, `spirv-cross`, `android-ndk`,
  `apple-sdk/build.zig` in full, `native_link.zig` in full. Rated LOW risk: every unread file is a
  `build.zig` whose only in-range commits are the Zig-0.16 pair, and the vendored *sources* are
  untouched.
- `.github/workflows/release-tip.yml` (+156) and `release-tag.yml` (+16) — read only their commit
  subjects (`480edb45e`, dependabot bumps). Both files are absent from rustty entirely.

---

## 2. Per-change verdict table

Legend for cost: S ≤ ½ day, M ≈ 1–2 days, L ≥ 3 days.
"grep found nothing" evidence is given as the exact query.

### 2.1 `src/config`

| Upstream commit(s) | What changed | rustty status | Evidence | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|
| `65c48213b`, `1092204df` | `scrollback-limit` renamed to `scrollback-limit-bytes`; typed as `Limit(usize, 50_000_000)`; `compatibility` map gains `cli.compatibilityRenamed(Config,"scrollback-limit-bytes")` | **PARTIAL** — rustty has `scrollback_limit: usize` at 50 MB, i.e. the *new default* without the *new mechanism*. Matches neither the pin (10 MB) nor head. | `crates/rustty-config/src/config_struct.rs:2327` `pub scrollback_limit: usize`, `:4975` `scrollback_limit: 50_000_000`; 16 in-crate sites (`grep -c 'scrollback_limit\|scrollback-limit\|ScrollbackLimit' config_struct.rs` → 16). No `scrollback-limit-bytes` anywhere: `grep -rn 'scrollback-limit-bytes' crates/` → 0 hits. | `config_scrollback_compression_defaults_and_parses` (`config_struct.rs:10655`) asserts `scrollback_limit == 50_000_000` — pins the divergence, does not guard the rename. **Needs a compatibility-alias test.** | M | Med — see §3 |
| `65c48213b` | NEW option `scrollback-limit-lines`, default `unlimited` | **ABSENT** | `grep -rn 'scrollback.limit.lines\|scrollback_limit_lines' .` → 0 hits outside this file | none — needs one | M | Med (vacuous if landed without `f4c68d65e`) |
| `1092204df` | NEW file `src/config/limit.zig` (+115): generic `Limit(T, default)` with `parseCLI`/`formatEntry`/`optional`/`clone` and the `"unlimited"` sentinel = `maxInt(T)`; 3 unit tests | **ABSENT** | `crates/rustty-config/src/value_types.rs` has no `Limit` type; `grep -rn 'fn optional' crates/rustty-config/src/value_types.rs` → 0 hits | none — needs one (the oracle ships 3 tests; port them) | S | Low |
| `65c48213b`, `1092204df`, `0fb89f4ff` | `scrollback-limit` doc block rewritten to its final form (compression paragraphs + `-lines` cross-ref + "The default is 50 MB. Set this to `unlimited`…") | **PARTIAL, and internally contradictory** | `config_struct.rs:2300-2327` carries the `0fb89f4ff` compression paragraphs *and* still carries the pin sentence "It is not currently possible to set an unlimited scrollback buffer. This is a future planned feature." — next to a default lifted from the commit that added unlimited support. This is shipped, user-facing doc text. | G3b (see §7); no content gate | S | Low, but it is a real doc defect today |
| `0fb89f4ff` | NEW option `scrollback-compression: bool = true` + its doc block | **PORTED** | `config_struct.rs:2351` `pub scrollback_compression: bool`, `:4976` default `true`, `:491` key parse, `:7485` `c!()`, `:8027` name table, `:8537` format, `:9859` parse, `:10513` field-name list; `c_get.rs:418`; accessor at `renderer_config.rs:227` | `config_scrollback_compression_defaults_and_parses` (`config_struct.rs:10655`) — parses `false`, formats round-trip | — | — |
| `2970e9a2a` | `Color.parseCLI` now delegates to `terminal.color.RGB.parse`; `Color.fromHex` **DELETED**; `Palette.parseCLI` delegates to `terminal.color.parsePaletteEntry` | **PORTED** (both halves, with the oracle commit cited in-line) | `value_types.rs:1891-1910` (`PORT NOTE (oracle 2970e9a2a …)`, calls `rustty_vt::color::RGB::parse`); `value_types.rs:2642-2663` (calls `rustty_vt::color::parse_palette_entry`). `fromHex` is gone: `grep -n 'fn from_hex' crates/rustty-config/src/value_types.rs` → 0 hits | `rustty-vt-c/src/color.rs:483` `parse_palette_entry_valid` / `:545` `..._invalid` guard the callee. The *config-side delegation* has no dedicated test — a regression to a local hex parser would not fail anything. **Needs one.** | — | Low |
| `5ea45919d` + `b5f5ca078` | NEW option `gtk-horizontal-tab-scroll: bool = true` + its "Available since 1.4.0." line | **ABSENT — and mis-ledgered** | `grep -rn 'horizontal.tab.scroll\|horizontal_tab_scroll' . --exclude-dir=.git` → only 3 hits, all inside `.port/phase-2/MAIN_PARITY.md`. `MAIN_PARITY.md:61,197` defer it to the unported "BL-8 GTK tier". **That deferral is wrong**: rustty-config already carries the sibling GTK options — `config_struct.rs:4664` `pub gtk_wide_tabs: bool` — so the *Config field* does not depend on the GTK apprt at all. Also: `Config.zig` "Available since" count is 63 at pin, 64 at head, and rustty has 63 → the single new one is exactly this option's. | none — needs one | S | Low, but it silently breaks the 204-field contract's claim to match the oracle |
| `2da02f4d2` | `background-blur` doc block rewritten (KDE/KWin prose replaced by `ext-background-effect-v1` + X11/KWin note); net −3 lines | **ABSENT** | `config_struct.rs:1970-1978` still carries the pin text ("KDE Plasma (Wayland and X11)", "To configure KWin's global blur setting, open System Settings…"). `grep -n 'ext-background-effect-v1' crates/` → 0 hits | G3b only (§7) | S | Low |
| `d71695550` | `keybind` `global:` doc block rewritten (adds `vicinae-hotkey-v1`, restructures the XDG-portal list, adds COSMIC) | **ABSENT** | `config_struct.rs:2731,2736,2746` still carry the pin text; `grep -n 'vicinae' crates/rustty-config/` → 0 hits | G3b only (§7) | S | Low |
| `9fc2a3085` (oracle HEAD) | `src/config/url.zig`: `trailing_spaces_at_eol` regex fragment **deleted** from all 3 branches; 3 test expectations changed so trailing whitespace is no longer part of a matched path | **ABSENT — in TWO rustty sites** | (1) `crates/rustty-config/src/url.rs:65` `const TRAILING_SPACES_AT_EOL`, still spliced at `:127,:134,:155`. (2) `crates/rustty-vt/src/string_map.rs:240`, spliced at `:281,:288` — rustty duplicated the whole regex into rustty-vt because rustty-vt cannot depend on rustty-config. Upstream has it in **one** place only (`git grep -n trailing_spaces_at_eol origin/main -- src` → 0 hits; at the pin → 4 hits, all `src/config/url.zig`). | `url.rs` has the ported `test "url regex"` table; its expectations must flip in both copies. Nothing currently checks the two copies agree. **Needs a "the two regexes are byte-identical" gate.** | S | **Med** — the two copies can silently diverge; that is a fidelity hole independent of this commit |
| `e8525c0fd`, `f2a7652ab`, `e663d5405`, `a77c706a1` | `std.fs`→`std.Io`; `global_state`→`global.io()/environ()/args()/environMap()/resourcesDir()`; `ArrayListUnmanaged`→`ArrayList`; `@Type`→`@Enum`/`@Struct` in `key.zig`/`conditional.zig`; `Path.expand` restructured around `realPathFile`; `Location.dir` error set gains `Unexpected` | **N/A** — Zig-0.16 stdlib/reflection migration with no Rust analogue. `key.zig`/`conditional.zig` are comptime enum *synthesis*; rustty hand-writes `Key` as a real Rust enum with 204 explicit variants (`config_struct.rs:10832` `ALL_KEYS`). | `config_struct.rs:11060-11061` asserts `ALL_KEYS.len() == 204` and `CONFIG_FIELDS.len() == 204`; `:10570` asserts `CONFIG_FIELD_NAMES.len() == 204` | those 3 count assertions | — | — |

### 2.2 `src/cli`

| Upstream commit(s) | What changed | rustty status | Evidence | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|
| `d320cd7df` | `list-themes` preview: add `try loop.start()` / `defer loop.stop()` so the preview receives terminal input | **PORTED** (already present, independently) | `crates/rustty-cli/src/list_themes.rs:774` `loop_.start()?;` and `:826` `loop_.stop();` | `list_themes.rs:2614` — the interactive Loop path is `#[ignore]`-on-device (needs a real `/dev/tty`). So it is **exercised by no CI test**; the guard is compile-time only. | — | Low |
| `d320cd7df` | Move the `env_map` into the `Preview` struct so vaxis cannot outlive it | **N/A** | rustty's `Vaxis::init` takes no env map at all (`crates/rustty-vaxis/src/vaxis.rs:203` `pub fn init(opts: Options)`; call site `list_themes.rs:725`). The env-map parameter is itself a product of the Zig-0.16 `std.process.Environ` migration, which post-dates the pin. There is no dangling reference to fix. | — | — | — |
| `f2a7652ab` / `e8525c0fd` | `edit-config` failure message: `Error code={}` → `(E{s})` with `@tagName(errno)`; the `comptime assert(link_libc)` moved to the top of `runInner`; `execvpeZ` → raw `execve` | **ABSENT** (the message text only; the rest is Zig-0.16) | `crates/rustty-cli/src/edit_config.rs` — the ported message still mirrors the pin form | none — cosmetic | S | Low |
| `e8525c0fd`, `f2a7652ab`, `e663d5405`, `a77c706a1` | 22 of 24 files: `std.fs.File`→`std.Io.File`, `argsIterator(alloc)`→`argsIterator(alloc, global.args())`, `internal_os.getenv`→`global.environ().getAlloc`, `std.process.Child`→`std.process.spawn`, `atomicFile`→permissions-based create, `trimRight`→`trimEnd`, `ArgIteratorGeneral`→`Args.IteratorGeneral`, `@Type`→`@Struct` in `args.zig:489` | **N/A** — Zig-0.16 stdlib migration | Verified by pattern-filtered diff over all 24 files; the `ssh-cache/DiskCache.zig` (+165, the largest) residue after filtering `global.io()`/`std.Io.` is *entirely* file-API churn | — | — | — |
| — | **No new CLI action was added.** | — | `git show origin/main:src/cli/ghostty.zig` vs `4749c4e93:` — the `Action` enum member list is byte-identical; the only diff is the `stdout` writer call | `rustty-cli` action-detect tests | — | — |
| `1092204df` (consumer) | `cli.compatibilityRenamed` used for the scrollback rename | **machinery PORTED** | `crates/rustty-args/src/args.rs:739` `pub fn compatibility_renamed<T: Target>(to: &'static [u8])`; already used at `config_struct.rs:9252,9258` for `background-blur-radius` and `adw-toolbar-style` | `rustty-args/src/args/tests.rs:560` `parse_compatibility_renamed` | — | — |

### 2.3 `src/build`

| Upstream commit(s) | What changed | rustty status | Evidence | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|
| `f2a7652ab` | NEW `Config.omitFramePointer()`: never omit frame pointers on Darwin, because the Apple arm64 ABI expects x29 to be a valid frame pointer | **N/A — proven, not assumed** | `rustc +nightly -Z unstable-options --print target-spec-json --target aarch64-apple-darwin` → `"frame-pointer": "non-leaf"`. rustc keeps frame pointers on Apple ARM *unconditionally*, independent of opt-level or strip. The requirement is satisfied by the toolchain. | the target spec itself | — | — |
| `f2a7652ab` | NEW `src/build/libsystem_override.sh` (+123) + `LibsystemOverrideStep.zig` (+51): Darwin-only `nmedit`-localize of `_memcpy`/`_memmove`/`_memset`/`_strlen`/libm in the static archive, so consumers bind libSystem instead of Zig's compiler-rt | **N/A — proven, not assumed** | The premise is Zig-specific: Zig's compiler-rt exports these as *C* symbols. Rust's does not. `nm -g $(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/lib/libcompiler_builtins-*.rlib` → the mem functions exist **only** as Rust-mangled `__ZN17compiler_builtins3mem6memcpy17h…E`; there is no `T _memcpy`, no `T _memset`, and no libm symbol at all (`grep -E '^[0-9a-f]* T _(memcpy\|memmove\|memset\|memcmp\|bcmp\|strlen\|cos\|sin\|exp\|pow\|ceil)$'` → 0 hits, over 422 total `T` symbols). They cannot shadow libSystem at final link. This matters because `crates/rustty-vt-c` and `crates/rustty-apprt-c` both emit `staticlib` (`Cargo.toml:22` / `:29`) — i.e. rustty *is* in the same archive-consumer position, and is still safe. | none needed | — | — |
| `d65cb5128` | Link `libghostty-vt` on Apple hosts with the **native** linker (`pkg/apple-sdk/native_link.zig` +85, `GhosttyLibVt.zig` +131), because Zig's Mach-O linker omits `LC_ENCRYPTION_INFO_64` on physical-iOS dylibs and the framework then fails App Store validation | **N/A** | The defect is in Zig's self-hosted Mach-O linker. rustc on Apple targets already invokes `cc`→`ld64`, which emits the load command. rustty additionally does not ship an iOS dylib: `xtask xcframework` packages a **static** library (`xtask/src/main.rs:442` `-create-xcframework -library`, folding `libRusttyKit.a` at `:395`) for macOS only. | none — see §6.1 for the caveat this earns | — | Low |
| `1fe1b2d23` | Fix static `libghostty-vt` linking on Windows | **N/A-tier** | Locked fork decision: macOS-first (memory `rustty-completion-plan`). rustty's Windows path is not stood up. | — | — | — |
| `f2a7652ab` | `patch_rpath` → `patchelf { interp, rpath }`: on NixOS also `--set-interpreter` from `ld.so`, not just `--set-rpath` | **N/A-tier** | `grep -rn 'patchelf\|IN_NIX_SHELL' . --exclude-dir=.git --exclude-dir=target` → 0 hits. rustty has no Nix packaging at all; `PACKAGING.md:143` defers a reproducible Nix/crane build to release-infra Batch 4. | — | — | — |
| `f2a7652ab` | `terminalOptions(artifact, optimize)` — `slow_runtime_safety` now derives from the *artifact's* optimize mode, not the global one | **N/A** | `grep -rn 'slow_runtime_safety' crates/ xtask/ Cargo.toml` → 0 hits. rustty has no `slow_runtime_safety` build option; safety checks are Cargo profiles. No analogue. | — | — | — |
| `10e6ace6b` | `GhosttyI18n.zig`: `trimRight(locale, ".UTF-8")` → `trimEnd(…)` for the FreeBSD locale-dir name | **N/A** | Semantically identical rename in Zig 0.16. rustty's `gen_locale` derives the locale directory from the `.po` **basename** (`xtask/src/res_misc.rs:375` `po.file_stem()`), never from a `.UTF-8`-suffixed name, so the trim has nothing to do. Note this is *more* robust than the oracle's hardcoded `locales` list. | `gen_locale`'s per-catalog `msgfmt` exit-status check (`res_misc.rs:398`) | — | — |
| `4c1d69696`, `b513f1b20`, `02504bcce`, `07a56d08b`, `dc8d38503` | `iterm2_themes` dependency bumped `ghostty-themes-release-20260608-160426-8c84dd1` → `…-20260720-153658-97e244c` (5 commits) | **ABSENT — and it is squarely in this area** | rustty **vendors** the corpus rather than fetching it: `crates/rustty-extra/themes/` holds 536 entries (534 themes + `README.md` + …), and `crates/rustty-extra/themes/README.md` pins provenance explicitly to `ghostty-themes-release-20260608-160426-8c84dd1.tgz`, hash `N-V-__8AAM7eAwCK4N5RemHoJXGZr7eQl49L6r8y1WJhzZGl`. Upstream's `build.zig.zon` now says `…20260720-153658-97e244c` / `N-V-__8AAGxKBAAjYkT1jYUiOZDXQuj0u34ndIjAWxiR1KX9`. | `xtask/src/res_themes.rs` emits with the `.md` exclusion and the module header asserts "exactly 534". A count change will surface; **content drift in existing themes will not.** Needs a manifest/digest gate. | M | Med — needs a tarball fetch; delta size unknown until then |
| `d0e72a3ab` + `f2a7652ab` | `z2d` `0.10.0` → `0.12.1`, and `font/sprite` switched to z2d's native path insetting | **ABSENT — cross-area (F: font/sprite)** | rustty ports z2d as `crates/rustty-z2d`; the consumer is `rustty-font`'s sprite renderer. Not assessed here beyond the dependency fact. | — | L | Med — a vendored-dep minor-version jump plus a call-site rewrite |
| `e8525c0fd` | `vaxis` `0.5.1` → `0.6.0` | **ABSENT — cross-area, but MY consumers** | `crates/rustty-vaxis`. Consumed by `rustty-cli`'s `list-themes` (`list_themes.rs:566-571`), `list-keybinds`, `list-colors`. | — | M | Med |
| `e8525c0fd` | `zf` `0.10.3` → `0.11.0` | **ABSENT — cross-area** | `crates/rustty-zf`; consumed by the `list-themes` fuzzy picker | — | M | Low |
| `e8525c0fd`, `f2a7652ab` | `uucode` bumped; `src/build/uucode_config.zig` +203/−… (the `computeWidth` extension removed / restructured) | **ABSENT — cross-area (unicode)** | `crates/rustty-unicode` generated tables. **Ordering hazard:** rustty's CI installs **Zig 0.15.2** specifically as the uucode oracle (`.github/workflows/ci.yml:69` "Install zig 0.15.2 (pinned — G4-uni uucode oracle)"); the new uucode requires Zig 0.16, so regenerating the tables forces that pin to move. | G4-uni | M | **Med** — CI toolchain pin coupling |
| `e8525c0fd` | `libxev` bumped | **ABSENT — cross-area** | `crates/rustty-xev` (D6 Zig-port-defer, memory `integration-tier-decisions`) | — | — | Low |
| `1cc5e7bec`, `e8525c0fd`, `f2a7652ab` | `zig-gobject` 0.3.0→0.3.2, `wayland` 0.5.0-dev→0.6.0, `zig_objc`, `zig_js` bumped | **N/A-tier** | GTK apprt / wasm tiers; no Cargo analogue for the Zig bindings generators | — | — | — |
| `e31f729b3`, `82e53e3f6`, `07f6c6bb0` | `translate_c` backport dependency added/updated; deps mirrored to `deps.files.ghostty.org` | **N/A** | This is the machinery that *replaces* `@cImport` (see §5). rustty uses `bindgen`. Mirroring is upstream release infra. | — | — | — |
| `e8525c0fd`, `f2a7652ab` | Everything else in `src/build` (~519/309 lines): `linkLibC()`→`.link_libc`, `lib.addX`→`lib.root_module.addX`, `captureStdOut()`→`captureStdOut(.{})`, `pub fn main()`→`pub fn main(init: std.process.Init)`, `std.process.Child`→`std.process.spawn`, `.Ignore`→`.ignore`, `getOutput()`→`getOutputFile()`, `trimRight`→`trimEnd` | **N/A** — Zig-0.16 build-API migration | Read in full for `Config.zig`, `GhosttyResources.zig`, `combine_archives.zig`, `GitVersion.zig`, `mdgen/*`, `webgen/*`. **No install-layout change** — `GhosttyResources.zig`'s `share/` tree is byte-for-byte the same set of destinations, so `xtask resources` needs no change. `mdgen`/`webgen` *content* is untouched, so WS13's docs half is unaffected by Phase 3. | `xtask resources` | — | — |

### 2.4 `pkg`

| Upstream commit(s) | What changed | rustty status | Evidence | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|
| `e8525c0fd` | `pkg/harfbuzz/c.zig` **DELETED** (−8); replaced by a `translate_c`-backport `Translator` producing an `hb_c` module imported at `pkg/harfbuzz/main.zig:11` `pub const c = @import("hb_c")` | **N/A — nothing to follow** | Zig 0.16 removed `@cImport`. rustty never had a `c.zig`: `crates-sys/harfbuzz-sys/build.rs:116` `bindgen::Builder::default()` over `crates-sys/harfbuzz-sys/wrapper.h`. The `wrapper.h` **is** the structural equivalent of the new `hb_c` module — rustty arrived at upstream's destination before upstream did. | `harfbuzz-sys` build + its harvested doctests (`Cargo.toml:31`) | — | — |
| `e8525c0fd` | `pkg/wuffs/src/c.zig` **DELETED** (−18); its `defines` array moved into `pkg/wuffs/build.zig` | **N/A — nothing to follow.** One stale reference to clean. | The define list is **byte-identical** in both trees. Upstream `origin/main:pkg/wuffs/build.zig:5-14` and `crates-sys/wuffs-sys/build.rs:41-50` list the same 10 `WUFFS_CONFIG__MODULE*` gates in the same order. **But** `crates-sys/wuffs-sys/build.rs:14` and `:35` both cite `pkg/wuffs/src/c.zig` as the provenance of that list — a file that no longer exists upstream. Cosmetic, but it is exactly the kind of oracle-path rot that makes fidelity auditing unreliable. | none — the list is not diffed against the oracle | S (comment fix) | Low |
| `1c861e3c4` | `pkg/apple-sdk/include/math.h` (+21) + build wiring: `#include_next <math.h>` shim defining `INFINITY`/`NAN`, because Xcode 27's `math.h` uses the `__need_infinity_nan` protocol that **Zig 0.16's bundled `float.h`** predates | **N/A** | The premise is Zig's *bundled* Clang resource headers. rustty's C/C++ compiles go through the `cc` crate → Apple `clang` with its own matching resource headers; `bindgen`/libclang likewise. No bundled `float.h` exists in the rustty toolchain. | the `-sys` crates' own builds | — | — |
| `dac134d25` | `pkg/apple-sdk/build.zig` (+24): enable the Apple vendor availability table for libc++, because Zig's bundled upstream libc++ headers assume every LLVM-21 ABI symbol is present in the system dylib (crash on macOS without `std::__hash_memory`) | **N/A** | Same premise: Zig *bundles* libc++ headers. rustty's C++ deps compile against the Apple SDK's own libc++ with correct availability annotations by construction. | — | — | Low |
| `84254a9d8` | Avoid the MSVC C++ runtime in no-libcxx builds (`pkg/highway`, `pkg/simdutf`, `GhosttyLibVt`, `SharedDeps`, `src/lib_vt.zig`) | **N/A — and the equivalent already exists** | rustty already handles the "don't drag in a C++ runtime" case explicitly: `crates-sys/simdutf-sys/build.rs:50` `build.cpp_link_stdlib(None)` and `crates-sys/highway-sys/build.rs:68` the same, with the reasoning spelled out at `highway-sys/build.rs:39,64`. The Windows-specific MSVC half is macOS-first N/A. | those crates' builds | — | — |
| `d65cb5128` | `pkg/apple-sdk/native_link.zig` (+85) | **N/A** | See `d65cb5128` in §2.3 | — | — | — |
| `e8525c0fd`, `f2a7652ab` | The other 40 `pkg/*/build.zig` files: `lib.linkLibC()`→`.link_libc = true` in the module options; `lib.addIncludePath/addCSourceFiles/addConfigHeader`→`lib.root_module.*`; `linkSystemLibrary2`→`root_module.linkSystemLibrary`; `build.zig.zon` hash bumps | **N/A** — Zig-0.16 build-API migration, 694/394 of the 859/399 total | Read `pkg/libintl/build.zig`, `pkg/zlib/build.zig`, `pkg/oniguruma/build.zig` in full and confirmed the pattern; commit attribution exhaustive for the remaining 37 | — | — | — |
| — | **WS9's gettext posture is unaffected.** | `pkg/libintl/build.zig` changed by only 10 lines, all `.link_libc`/`root_module` churn. Nothing upstream re-litigated the vendored-`intl` decision, and nothing added a new host-tool requirement. WS9's `crates-sys/gettext-sys` → `librustty_intl.a` and the Homebrew-`msgfmt`-as-build-tool sanction stand. | — | — | — |

### 2.5 `po` (i18n)

WS13's findings are taken as binding input and not re-derived. What follows is *only* what the
upstream `po/` diff adds on top.

| Upstream commit(s) | What changed | rustty status | Evidence | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|
| `9c6f287aa` | **`#:` reference-comment renumbering + `POT-Creation-Date` bumps + msgmerge line-rewrapping** across all 32 files | **N/A — and must NOT be copied** | 2142 of the 2516 changed lines are `#:` comments (`grep -c '^[+-]#:'`). They cite `src/apprt/gtk/**/*.{zig,blp}` at *upstream's* line numbers. rustty's `po/io.airely.rustty.pot` carries those same oracle paths verbatim (`po/io.airely.rustty.pot:20+`, e.g. `#: src/apprt/gtk/class/clipboard_confirmation_dialog.zig:197`) — paths that exist in **no rustty file**. Copying upstream's renumbering would make stale metadata *differently* stale. WS13 T4 regenerates these against rustty paths via `xtask update-translations`, which supersedes the whole hunk class. gettext ignores `#:` at lookup time, so there is zero functional impact either way. | — (WS13 T4 owns it) | — | — |
| `9c6f287aa` | **3 NEW msgids**, added to the `.pot` and all 31 catalogs: `"The keybind was revoked by the system."`, `"The keybind was denied by the system."`, `"Global keybind unavailable"` | **ABSENT — but zero user-visible value today** | `grep -c "Global keybind unavailable" po/io.airely.rustty.pot po/de.po` → `0`, `0`. Their `msgstr` is **empty in every one of the 32 upstream files** (`git show origin/main:po/de.po \| grep -A2 'Global keybind unavailable'` → `msgstr ""`). Their source is `src/apprt/gtk/class/application.zig:1708,1710,1721` — the `vicinae-hotkey-v1` work (`88bd4fdce`), area F. | none | S | Low. **Do not land these before their call sites exist** — a msgid with no call site and no translation is a vacuous catalog entry, and WS13 T1's `code_msgids_all_wrapped_and_in_pot` gate is designed to *fail* on exactly that shape. |
| `9c6f287aa` | `"Close Split"` msgid **added as an untranslated entry** to the catalogs that lacked it | **ABSENT** | `grep -n 'Close Split' po/be.po` → only `:293 msgid "Close Split?"` (the confirmation-dialog string); the bare `"Close Split"` entry is missing, matching the pin. `po/kk.po:178` has it (it was the pin commit's subject). | none | S | Low |
| `ae76cfbfc` | `po/zh_CN.po`: `"Close Split"` → `"关闭分屏"` — **the only real translation content in the entire +1454/−1062 diff** | **ABSENT** | `grep -A1 'msgid "Close Split"' po/zh_CN.po` → `msgstr ""`. Repo-wide proof of uniqueness: `git diff 4749c4e93..origin/main -- po \| grep '^+msgstr' \| grep -v '^+msgstr ""'` → exactly one line, `+msgstr "关闭分屏"`. | `msgfmt --statistics` per catalog would show the count move; nothing asserts it | S | Low |
| — | **R2 interaction with the new msgids: none.** | None of the 3 new msgids and neither `"Close Split"` variant contains "Ghostty" or any identity token. The rename map does not touch them, and WS13's resolution (the 31 committed catalogs are authoritative for msgids) is unaffected by anything in this range. The R2 problem stays exactly the 3 `.blp` msgids WS13 already identified. | — | — | — |
| — | **The locale set did not change.** | `git diff 4749c4e93..origin/main -- src/os/i18n_locales.zig` → empty. rustty's 31-entry list (`crates/rustty-os/src/i18n_locales.rs`) still matches. No catalog was added or dropped. | — | — | — |

### 2.6 `src/synthetic`

| Upstream commit(s) | What changed | rustty status | Evidence | Guarding test/gate | Cost | Risk |
|---|---|---|---|---|---|---|
| `f6f79acce` (APC bulk dispatch series) | NEW `src/synthetic/Kitty.zig` (+88): random Kitty-graphics APC generator — `\x1b_G` + `a=t,f=100,i=<rand>;` + a length-rounded-to-4 random base64 payload + `\x1b\\`; plus `base64_alphabet` via `Bytes.generateAlphabet`; 1 test | **ABSENT** | `grep -rn 'kitty\|Kitty' crates/rustty-synthetic/src/` → 0 hits. `crates/rustty-synthetic/src/` contains `bytes, cli, cli.rs, enums, generator, io, lib, main_gen, osc, rand, utf8` — no kitty module. | none — needs one (port the oracle's `test "kitty"`) | S | Low |
| same | NEW `src/synthetic/cli/Kitty.zig` (+70): the `+kitty` CLI action with `--data-len` (default 4096) and a `create`/`destroy`/`run` triple; 1 test | **ABSENT** | same grep | none — needs one | S | Low |
| same | `src/synthetic/cli.zig`: `Action` gains `.kitty`; `Action.Struct` gains its arm | **ABSENT** | `crates/rustty-synthetic/src/cli.rs` — no `Kitty` variant | rustty-synthetic action-detect tests | S | Low |
| same | `src/synthetic/main.zig`: `pub const Kitty = @import("Kitty.zig")` | **ABSENT** | `crates/rustty-synthetic/src/lib.rs` — no `pub mod kitty` | — | S | Low |
| `e8525c0fd`, `f2a7652ab` | `cli/Ascii.zig` and `cli/Utf8.zig`: `else => return err` arm removed (the error set became exhaustive); `cli.zig`: `main(init: std.process.Init)`, `Args.cli` carries `std.process.Args`, `nanoTimestamp()` → `std.Io.Timestamp.now(io,.real)`, `u128`→`u96` bitcast | **N/A** — Zig-0.16. `f2a7652ab`'s note "synthetic: fix UB by accessing global.io before it was initialized" is a Zig global-init-order bug with no Rust analogue. | Read in full | — | — | — |

### 2.7 `.github`, `nix`, `flatpak`, root packaging

All N/A. Itemised with per-item justification in §6.

---

## 3. Verdict on the runtime-limits refactor vs rustty's just-landed `scrollback-limit`

### 3.1 What upstream actually did (three commits, one design)

1. **`65c48213b`** — added `scrollback-limit-lines`, renamed `scrollback-limit` →
   `scrollback-limit-bytes`, registered a `compatibilityRenamed` alias, and threaded both through
   `Termio.init` as `max_scrollback_bytes` / `max_scrollback_lines`.
2. **`1092204df`** — introduced `src/config/limit.zig`'s `Limit(T, default)` so both options accept
   the literal `unlimited` (stored as `maxInt(T)`, unwrapped by `.optional() → ?usize`).
3. **`f4c68d65e`** — *"terminal: support runtime scrollback limits"*: moved byte **and** line state
   into a `PageList.Limits` struct (`origin/main:src/terminal/PageList.zig:405` `limits: Limits`,
   `:6267` `pub fn max(self, key)`), with `set` / `resize` / `exceeded` / `enforce` and runtime
   setters on `PageList` and `Terminal`. 670 changed lines in `PageList.zig`, 153 in `Terminal.zig`.
   Lowering a limit prunes immediately; zero bytes degrades the primary screen to no-scrollback;
   alternate screens are untouched.

`pages.maxSize()` becomes `pages.limits.max(.bytes)`; the byte limit stops being a fixed field and
becomes `@max(explicit, min)` recomputed on every resize.

### 3.2 Where rustty actually stands

- Config: `scrollback_limit: usize` at `config_struct.rs:2327`, default `50_000_000` at `:4975`.
- PageList: `explicit_max_size: usize` (`crates/rustty-vt/src/pagelist/mod.rs:1068`) and
  `min_max_size: usize` (`:1073`) as **plain fields**, fixed at init. No `Limits` struct, no line
  limit, no runtime setter. `total_rows` exists (`:1077`) so a line limit is *computable*.
- A concurrent lane owns `crates/rustty-vt/src/{compress,pagelist}/` right now.

**Three concrete problems:**

1. **rustty's default matches neither end of the range.** The pin is `10_000_000`; upstream head is
   `Limit(usize, 50_000_000)` named `scrollback-limit-bytes`. rustty is `50_000_000` named
   `scrollback-limit`. WS7 D2 lifted the *value* from the rename commit ("the oracle did the same",
   `docs/superpowers/plans/2026-07-23-ws7-scrollback-compression.md:4520`) without the mechanism.
   The `MAIN_PARITY.md:128` row calling `0fb89f4ff` PORTED is *correct for `0fb89f4ff`* — the 50 MB
   default simply is not from `0fb89f4ff`, and `65c48213b`/`1092204df` post-date that ledger's
   triage head, so no row covers them.

2. **The shipped documentation is now self-contradictory.** `config_struct.rs:2322-2323` still says
   *"It is not currently possible to set an unlimited scrollback buffer. This is a future planned
   feature."* — sitting directly above a default that only exists because upstream added exactly
   that feature. This text reaches users through `+show-config --docs`, the generated `.5` man page,
   and the webdata.

3. **rustty already carries a written, KNOWN gap that `f4c68d65e` closes.**
   `crates/rustty-vt/src/pagelist/compress.rs:165-173` documents it verbatim:

   > *"the oracle has ELEVEN `markActivity` call sites; rustty has ten. The missing one is
   > `oracle@main:PageList.zig:6312`, inside `Limits.enforce` — and rustty has no `PageList::Limits`
   > struct at all (the byte limit is a plain field), so there is no counterpart function to port it
   > onto. If a `Limits` struct is ever introduced, `enforce` must `mark_activity` on entry."*

   That is a correctness gap (trimming scrollback mutates the node set under an in-flight
   incremental compression traversal, which is precisely what the activity token detects), currently
   parked in `.port/phase-2/DEFERRALS.md`.

### 3.3 Verdict

**Yes — `f4c68d65e` supersedes rustty's `scrollback-limit`, and the three commits must be ported as
one unit. Do not port the config half alone.**

Reasoning:

- Landing `scrollback-limit-lines` without `f4c68d65e` produces a **vacuous option**: a documented,
  completion-visible, C-ABI-exposed config field that nothing enforces. That is strictly worse than
  ABSENT — it is a shipped lie, and it is the exact failure shape this program has been burned by
  (the `OUT_DIR/locale` emit, the guard test that passes on unmodified code).
- Landing `f4c68d65e` without the config half is fine but pointless: `Limits` with no user-facing
  knob and no runtime setter caller.
- The `Limit` type (`1092204df`) is a prerequisite of the config half and is 115 lines with 3 tests
  — cheap and self-contained.
- `f4c68d65e` also closes the documented `markActivity` deferral for free, converting a red
  ledger row into a green one.

**Recommended sequencing:**

| Step | Content | Cost | Blocked on |
|---|---|---|---|
| G-L1 | Port `Limit(T, default)` into `rustty-config` `value_types.rs` (+ its 3 oracle tests) | S | — |
| G-L2 | Port `f4c68d65e`: `PageList::Limits { bytes, lines }` with `set`/`resize`/`exceeded`/`enforce`/`max`, runtime setters on `PageList` and `Terminal`, and `mark_activity()` at the top of `enforce` (closing the `compress.rs:165` deferral) | **L** | **WS7 BL-1 must land first** — the concurrent lane owns `pagelist/` + `compress/`, and `f4c68d65e` rewrites `erasePage`/`fixupViewport`/`markActivity`, the same paths |
| G-L3 | Config half: rename `scrollback_limit` → `scrollback_limit_bytes` (16 sites in `config_struct.rs` + `c_get.rs` + `renderer_config.rs`), add `scrollback_limit_lines`, register `compatibility_renamed(b"scrollback-limit-bytes")`, replace the contradictory doc block with upstream's final text, bump the three `204` assertions to `205`, regenerate the 5 `rustty-extra/testdata/` goldens | **M** | G-L1, G-L2 |
| G-L4 | Thread both limits through the `Termio` construction site as `?usize` via `.optional()` | S | G-L3; cross-area E (`src/termio`) |
| G-L5 | Tests: port the oracle's `test "scrollback limits"` and `test "compatibility: scrollback-limit renamed to bytes"` (`Config.zig:10865-10930`) verbatim, plus a `Limits.enforce` prune test | S | G-L3 |

**Cost of the config half, measured (not estimated).** WS7 D2's "one new bool = 13 in-crate sites +
`c_get.rs` + 6 field-count contracts + 5 goldens" tax applies, and the rename is worse because it
touches an existing field:

- `grep -c 'scrollback_limit\|scrollback-limit\|ScrollbackLimit' crates/rustty-config/src/config_struct.rs` → **16**
- adding `-lines` adds ~16 more, by symmetry with `scrollback_compression` (**21** sites)
- `crates/rustty-config/src/c_get.rs` (1 site) + `renderer_config.rs`
- **three** `204` assertions: `config_struct.rs:10570` (`CONFIG_FIELD_NAMES`), `:11060` (`ALL_KEYS`),
  `:11061` (`CONFIG_FIELDS`)
- **five** byte-exact goldens, all confirmed to contain the literal `scrollback-limit`:
  `crates/rustty-extra/testdata/{_rustty,rustty.bash,rustty.fish,vim-syntax.vim,rustty.sublime-syntax}`
- **Run the golden suite with `cargo test --workspace --no-fail-fast`.** Plain `cargo test
  --workspace` fails fast on the pre-existing `rustty-apprt-c` cold-target dylib ordering issue,
  which masks golden failures in later crates. (Not re-verified here — build cost; taken as given.)

**Rejected alternative:** keep `scrollback-limit` and record a permanent divergence. Rejected
because (a) the shipped doc is factually wrong *today*, (b) `compatibility_renamed` machinery already
exists (`crates/rustty-args/src/args.rs:739`, already used twice), so the rename costs users nothing,
and (c) it would leave the `markActivity` deferral permanently open.

---

## 4. Partials, divergences, and vacuous artifacts

### 4.1 Partials with exact missing hunks

**P-1 — `scrollback-limit` (see §3).** Missing hunks: `Config.zig:1366-1428` (the whole rewritten
doc block + rename + `Limit(usize, 50_000_000)` + `scrollback-limit-lines` +
`Limit(usize, maxInt(usize))`); `Config.zig:96-99` (the `compatibility` map entry); all of
`src/config/limit.zig`; `Config.zig:10865-10930` (both tests); `Termio.zig:245-247`.

**P-2 — `scrollback-limit` doc block.** rustty holds a three-way blend: pin text + `0fb89f4ff`
compression paragraphs + a post-`1092204df` default. Exact missing/stale lines:
`config_struct.rs:2322-2323` must be **deleted** (`"It is not currently possible to set an unlimited
scrollback buffer. / This is a future planned feature."`) and replaced by upstream
`Config.zig:1401-1405` (`"A separate maximum can be set with \`scrollback-limit-lines\`; …"` +
`"The default is 50 MB. Set this to \`unlimited\` to remove the byte limit."`).

**P-3 — `Color.parseCLI` delegation is ported but unguarded.** `value_types.rs:1894` correctly calls
`rustty_vt::color::RGB::parse`, and `fromHex` is gone — but no rustty test asserts the *delegation*.
A regression to a local hex parser (which is what the pin had) would pass everything. The oracle
renamed its own test `"fromHex"` → `"parseCLI hex"` for exactly this reason; port that rename.

**P-4 — `url` regex duplicated across two crates.** `crates/rustty-config/src/url.rs` and
`crates/rustty-vt/src/string_map.rs:225-300` each hold a full independent transcription of the same
regex. Upstream has one copy. Fixing `9fc2a3085` requires editing both; nothing detects if only one
is edited. This is a standing fidelity hole, not created by Phase 3 but exposed by it.

### 4.2 Vacuous artifacts

**V-1 — `crates/rustty-apprt/build.rs`'s `msgfmt` → `OUT_DIR/locale` (re-confirmed, still live).**
`build.rs:459-506` compiles every `po/*.po` into `OUT_DIR/locale/<locale>/LC_MESSAGES/<domain>.mo`
(`:492-504`). `grep -rn OUT_DIR crates/rustty-apprt/src crates/rustty-apprt/tests` → **0 hits**.
Nothing reads it. This is the *second* msgfmt leg; the real one is `xtask/src/res_misc.rs:353-405`
`gen_locale` → `zig-out/share/locale/…`. WS13 T9 Step 4 converts it into a real `msgfmt --check`
validity gate; until then it is dead output that burns build time on every `rustty-apprt` rebuild.

**V-2 — `po/io.airely.rustty.pot`'s `#:` references cite files that do not exist in rustty.**
Every reference block names `src/apprt/gtk/**/*.zig` / `*.blp` at *oracle* line numbers
(`po/io.airely.rustty.pot:20+`). rustty has no `src/apprt/` directory. The `.pot` is therefore not a
regenerable artifact of rustty's own tree: no gate can regenerate-and-diff it, and its
`POT-Creation-Date` is still frozen at the pin's `2026-03-25 16:24-0700`. Consequence for Phase 3:
**do not port upstream's reference renumbering** — it would substitute one flavour of stale for
another. WS13 T4 (adding `--keyword=gettext_` and pointing extraction at `crates/rustty-apprt/`)
is the real fix.

**V-3 — `crates-sys/wuffs-sys/build.rs` cites a deleted oracle file.** `:14` and `:35` both name
`pkg/wuffs/src/c.zig` as the provenance of `WUFFS_DEFINES`. Upstream deleted that file in
`e8525c0fd`; the list now lives in `pkg/wuffs/build.zig:5-14`. The list itself is still correct and
byte-identical — only the citation rots. Same class of defect as V-2: an oracle reference that no
longer resolves.

**V-4 — the 3 new vicinae msgids would be vacuous if landed early.** All three have empty `msgstr`
in all 32 upstream files and their only call sites are in the unported GTK
`application.zig:1708,1710,1721`. Landing them into rustty's catalogs before the call sites exist
creates entries that translate nothing and are reachable from nothing — and WS13 T1's
`code_msgids_all_wrapped_and_in_pot` gate is written to *fail* on precisely that (a `.pot` msgid
with a code reference and no matching wrapped call site). Sequence them **with** the vicinae port,
not before.

### 4.3 Other divergences worth recording

**D-1 — rustty's `gen_locale` is deliberately better than the oracle's.** The oracle uses a
hardcoded `locales` list from `src/os/i18n_locales.zig` plus a FreeBSD `.UTF-8` trim
(`GhosttyI18n.zig:26-31`). rustty discovers catalogs from `po/*.po` basenames
(`res_misc.rs:375`). This is a fidelity divergence in the *safe* direction (it cannot drift out of
sync with `po/`) and it makes `10e6ace6b` N/A. Recording it so a future reviewer does not "fix" it
back.

**D-2 — `gtk-*` options in `rustty-config` vs the "BL-8 tier" deferral.** rustty-config carries GTK
options (`gtk_wide_tabs`, and by inspection many more) while the GTK apprt is unported. That is the
right call — `Config` is a data structure, not a runtime — but it makes `MAIN_PARITY.md:61,197`'s
deferral of `gtk-horizontal-tab-scroll` to the GTK tier internally inconsistent, and it is the
reason the 204-field contract currently does not match the oracle's field set.

---

## 5. Deletions section — what upstream removed, and what rustty must follow

| Upstream deletion | Why | rustty must… | Verdict |
|---|---|---|---|
| `pkg/harfbuzz/c.zig` (−8) | Zig 0.16 removed `@cImport`. Replaced by a lazy `translate_c` backport dependency whose `Translator` emits an `hb_c` module (`origin/main:pkg/harfbuzz/build.zig:200-330`), imported at `pkg/harfbuzz/main.zig:11` as `pub const c = @import("hb_c")`. | **do nothing.** rustty has no `c.zig` analogue and never did: `crates-sys/harfbuzz-sys/build.rs:116` runs `bindgen` over `crates-sys/harfbuzz-sys/wrapper.h`. `wrapper.h` *is* the `hb_c` module's structural equivalent — upstream converged on rustty's design. | **N/A, nothing to follow** |
| `pkg/wuffs/src/c.zig` (−18) | Same `@cImport` removal; the 10-entry `WUFFS_CONFIG__MODULE*` `defines` array moved verbatim into `pkg/wuffs/build.zig:5-14`. | **fix one comment.** The define list is already correct and byte-identical at `crates-sys/wuffs-sys/build.rs:41-50`. Only the provenance citation at `:14` and `:35` (`pkg/wuffs/src/c.zig`) must be retargeted to `pkg/wuffs/build.zig`. | **N/A + S comment fix** |
| `src/config/Config.zig`: `Color.fromHex` (−32 incl. its test) | Folded into `terminal.color.RGB.parse` (`2970e9a2a`) | **already followed.** `grep -n 'fn from_hex' crates/rustty-config/src/value_types.rs` → 0 hits. This deletion is a *cause* of the current G3b config `missing=9` (§7). | **already done** |
| `src/config/Config.zig`: `Palette.parseCLI`'s local `=`-split + `parseInt` (−13) | Folded into `terminal.color.parsePaletteEntry` | **already followed.** `value_types.rs:2645-2663` delegates. Also a G3b `missing` contributor. | **already done** |
| `src/config/Config.zig`: `const x11_color = @import("../terminal/x11_color.zig")` (−1) | No longer needed after the `RGB.parse` delegation | **already followed.** `grep -n x11_color crates/rustty-config/src/value_types.rs` → 0 hits | **already done** |
| `src/config/url.zig`: `trailing_spaces_at_eol` (−4 fragment + 3 splice sites) | Trailing whitespace is no longer part of a detected file path (`9fc2a3085`) | **follow, in TWO places** — `crates/rustty-config/src/url.rs:65,127,134,155` **and** `crates/rustty-vt/src/string_map.rs:240,281,288` — plus flip 3 test expectations in each. | **ABSENT, S, must not miss the second copy** |
| `src/config/Config.zig`: the `background-blur` KDE/KWin prose (−15) | Rewritten for `ext-background-effect-v1` (`2da02f4d2`) | **follow** (delete `config_struct.rs:1970-1982`, insert upstream's replacement). Will *raise* G3b's config `missing` count if the oracle checkout is not advanced first (§7). | **ABSENT, S** |
| `src/config/Config.zig`: the `keybind` `global:` Linux prose (−12) | Rewritten for `vicinae-hotkey-v1` (`d71695550`) | **follow** (`config_struct.rs:2731-2748`). Same G3b caveat. | **ABSENT, S** |
| `src/synthetic/cli/{Ascii,Utf8}.zig`: `else => return err` (−1 each) | The error set became exhaustive in Zig 0.16 | **do nothing.** Rust `match` on a closed error enum has no catch-all to remove. | **N/A** |
| `nix/devShell.nix` (−1 line) | dep churn | **do nothing** — rustty has no `nix/`. | **N/A** |
| *(named in the Phase-3 README but not in area G)* `src/os/args.zig`, `src/os/env.zig` | Subsumed by `std.process.Args`/`Environ` in Zig 0.16 | area E (`src/os`) | out of scope |

**No upstream deletion in area G requires a rustty file to be deleted.** Every one is either already
followed, structurally moot (rustty's `-sys` crates never had a `c.zig`), or a doc/regex hunk.

---

## 6. Explicit N/A justification list (auditable)

### 6.1 `.github` — 13 files, +480/−224

| Item | One-line reason |
|---|---|
| `VOUCHED.td` (+26) and the 6 `vouch-*.yml` workflows (`a34bf0dce`, `232d40c06`, `7bea975bd`, `95befb337`, `28f02ac3c`, `2de5e7d38`, `4c725242b`, `30e1f3bb8`, `74d0c72fd`, `c9c017e8e`, `f21d37688`, `f8041e849`, `a3ac713b7`, `78a9731c9`, `d8464a5f5`, `243f7df7c`, `131ca6d9e`, `61ce641fc`, `3b63c6dc6`, `9314b2c55`, `df5cee238`, `28f9367be`, `f9194f93d`, `5e872a6a6`, `4789bbdb9`, `45db2c255`, `0ac423d57`, `97f70cccf`, `3f6b9a16b`, `8093d2d43`, `76222008e`, `e1e2e5f34`) | `mitchellh/vouch` contributor-vouching governance for the upstream GitHub org. Encodes no build requirement. rustty has no vouch workflow (`ls .github/workflows` → `ci.yml`, `clean-artifacts.yml`, `milestone.yml`). |
| Dependabot action-SHA bumps (`f5477459e`, `0f0ede81d`, `68376f5ac`, `59c3a0836`, `035ae8ddb`, `78733419a`, `5a699032f`, `c6b0c0dcb`, `5d476024b`, `1fcc622d6`, `b3cc5000e`) — `actions/checkout` 6.0.3→7.0.1, `dorny/paths-filter` 4.0.1→4.0.2, `nscloud-cache-action` 1.4.3→1.6.1, `install-nix-action` 31.10.6→31.11.0, `softprops/action-gh-release`, `docker/build-push-action` | Pinned-SHA hygiene for workflows rustty does not have. rustty runs its own `dependabot.yml`. |
| `release-tip.yml` (+156, incl. `480edb45e` "skip tip release when only non-artifact files change"), `release-tag.yml` (+16), `snap.yml`, `nix.yml` | These workflows do not exist in rustty; release infrastructure is Batch 4 (`PACKAGING.md:143`, and `PACKAGING.md:37-40` notes the release-artifact host is not yet stood up). |
| `test.yml`: `xcode-select -s /Applications/Xcode_26.3.app` → `26.6`; new `build-macos-xcode-27` job (`d97a57424`) | rustty CI pins the Rust toolchain (`rust-toolchain.toml`), not an Xcode version; there is no Xcode-matrix job to update. |
| `update-colorschemes.yml` (+6) | Action-SHA bumps only. The *payload* of this workflow (the theme corpus) **is** a real item and is tracked as ABSENT in §2.3, not dismissed. |
| `.github/scripts/check-apple-libghostty-vt.nu` (+131) + the `aarch64-ios-simulator` matrix row with `platform`/`arch`/`expect_encryption` | **N/A with a recorded caveat, not a dismissal.** The script asserts, on a *dylib*: exactly one architecture (`lipo -archs`), install name `@rpath/libghostty-vt.dylib` (`otool -D`), exactly one `LC_BUILD_VERSION` with the expected numeric platform, and `LC_ENCRYPTION_INFO_64` present for physical iOS. rustty ships **no iOS dylib**: `xtask xcframework` builds a *static* library and calls `xcodebuild -create-xcframework -library` on `libRusttyKit.a` (`xtask/src/main.rs:395,442`), macOS-only. None of the four assertions has a subject. **Caveat to carry forward:** if rustty ever ships an iOS framework containing `rustty-vt-c`'s `cdylib`, this validation becomes mandatory — and it will pass, because rustc links Apple targets through `ld64`, which emits the load command Zig's Mach-O linker omitted. Recommend recording it as a precondition on the (unscheduled) iOS tier rather than porting a nushell script now. |

### 6.2 `nix/` (4 files) and `flatpak/` (1 file)

| Item | One-line reason |
|---|---|
| `nix/package.nix`, `nix/libghostty-vt.nix`, `nix/devShell.nix`, `nix/build-support/{check-zig-cache.sh,update-mirror.nu}` | rustty has **no `nix/` directory**; a reproducible Nix/crane build is an explicit, documented deferral to release-infra Batch 4 (`PACKAGING.md:141-147`). Every changed line here is a Zig-dependency hash or a Zig-cache helper with no Cargo analogue. |
| `flatpak/zig-packages.json` (+102/−…) | rustty has **no `flatpak/` directory**; the file is a machine-generated manifest of Zig dependency tarballs (the flatpak offline-build vendor set). Cargo's equivalent is `Cargo.lock`, which is already committed. |
| `src/build/Config.zig` `patch_rpath` → `patchelf` (interp + rpath) | The consumer is the NixOS build, which does not exist in the port (see above). Recorded, not dismissed: if a Nix packaging lane is stood up in Batch 4, the `--set-interpreter` half is a real requirement that a naive `--set-rpath`-only port would miss. |

### 6.3 Root packaging files

| Item | One-line reason |
|---|---|
| `build.zig.zon` (+55) | Zig's dependency manifest. Its *content* was mined for real items (§2.3: iterm2_themes, z2d, vaxis, zf, uucode, libxev bumps are all tracked as ABSENT); the file format itself has no Cargo counterpart beyond `Cargo.lock`. |
| `build.zig.zon.json` (+96), `build.zig.zon.nix` (+120), `build.zig.zon.txt` (+30) | Generated lock/mirror representations of the above, consumed only by Nix and the upstream dependency mirror. No analogue. |
| `build.zig` (+50) | The Zig build entry point. rustty's equivalents are `Cargo.toml` + `xtask`; the changed lines are the Zig-0.16 `b.graph.io` / step-API migration. |
| `.gitignore` (−1/+1, in `f2a7652ab`) | Zig cache path. |

### 6.4 `pkg/` and `src/build` toolchain-workaround items

Already itemised with proof in §2.3 / §2.4. Summarised for the auditor:

| Item | One-line reason |
|---|---|
| `libsystem_override.sh` + `LibsystemOverrideStep.zig` | Zig's compiler-rt exports libc/libm as *C* symbols; Rust's `compiler_builtins` exports them only Rust-mangled (`nm` proof in §2.3), so they cannot shadow libSystem. |
| `Config.omitFramePointer()` | rustc's `aarch64-apple-darwin` target spec is `"frame-pointer": "non-leaf"` unconditionally. |
| `pkg/apple-sdk/include/math.h`, apple-sdk libc++ availability | Both work around Zig's **bundled** Clang resource headers / bundled libc++; rustty compiles C/C++ with Apple `clang` against the real SDK. |
| `pkg/apple-sdk/native_link.zig` + `d65cb5128` | Works around Zig's self-hosted Mach-O linker; rustc uses `ld64`. |
| `84254a9d8` (no-libcxx MSVC runtime) | Windows half is macOS-first N/A; the "don't drag in a C++ runtime" half already exists (`cpp_link_stdlib(None)` in both simdutf-sys and highway-sys). |
| `1fe1b2d23` (static libghostty-vt on Windows) | macOS-first locked fork. |
| `10e6ace6b` (FreeBSD `trimEnd`) | Semantically identical Zig rename; rustty's locale-dir derivation makes the trim unnecessary (D-1). |
| `terminalOptions(.., optimize)` / `slow_runtime_safety` | No such build option in rustty; safety is a Cargo profile. |
| ~40 `pkg/*/build.zig` files, `src/build/*` bulk | Zig-0.16 build-API migration; quantified at 694/394 of `pkg` and 519/309 of `src/build`. |

---

## 7. G3b comment-coverage: where Phase 3 trips it, and what actually fixes it

**Measured now (this worktree, `python3 .port/tools/gates/check_comment_coverage.py`):**

```
G3b [config]:  distinctive ref comments=2777  missing=9   review=10
G3b [cli]:     distinctive ref comments=591   missing=2   review=2
G3b [args]:    185 / 0 / 1      G3b [build-config]: 27 / 0 / 0     G3b [extra]: 20 / 0 / 2
G3b [synthetic]: 95 / 0 / 0
```

**All 9 config "missing" entries are pin artifacts of beyond-pin ports — not real drops.** Verbatim:

```
allocated lazily up to this limit, so if you set this to a very large   ┐
larger this value, the larger potential memory usage. scrollback is    │ scrollback-limit doc
scrollback currently exists completely in memory. this means that the   │ rewrite (65c48213b)
value, it will not immediately consume a lot of memory.                 ┘
expand short hex values to full hex values                             ┐
fromhex parses a color from a hex value such as #rrggbb. the "#"        │ Color.fromHex +
parse the color part (color.parsecli will handle whitespace)            │ Palette.parseCLI
parse the key part (trim whitespace)                                    │ deletions (2970e9a2a)
trim any whitespace before processing                                   ┘
```

That is a **100 % false-positive rate** for this crate's row: every one is a sentence the oracle
*itself deleted* beyond the pin, which rustty correctly does not have. This confirms and extends the
WS7 D2 observation (config 5→9). The `cli` row's 2 (`"decompress the frames into a slice of
individual frames"`, `"heap allocated list of frames…"`) are unrelated to this range.

**Phase 3 will make this worse before it makes it better.** Porting area G's doc deletions raises
the count roughly:

| Item | Added false "missing" |
|---|---|
| `background-blur` doc rewrite (`2da02f4d2`) | ~15 |
| `keybind` `global:` doc rewrite (`d71695550`) | ~12 |
| `scrollback-limit` doc block final form (P-2) | ~2 (the "not currently possible … future planned feature" pair) |
| `url.zig` `trailing_spaces_at_eol` removal (`9fc2a3085`) | ~1 |
| **Projected config row after area G lands** | **~9 → ~39** |

**Advancing `.port/ORACLE_SYNC.md` alone does NOT fix this.** The gate does not read the pin SHA —
it reads the oracle **working tree**:

```
.port/tools/gates/check_comment_coverage.py:34
GHOSTTY_SRC = os.environ.get("GHOSTTY_SRC", "/Users/gael/projects/ghostty/src")
```

and that tree is pinned at `4749c4e93`. So the Phase-3 finale needs **two** actions, not one:

1. update `.port/ORACLE_SYNC.md`'s pin to `6ad1fe7d8` (a doc change), **and**
2. make `GHOSTTY_SRC` resolve to a `6ad1fe7d8` tree.

**Recommendation for (2): do not `git checkout` in `../ghostty`.** The Phase-3 README forbids it and
six lanes depend on the tree staying put. Instead create a second checkout —
`git -C ../ghostty worktree add ../ghostty-head origin/main` — and point `GHOSTTY_SRC` at
`../ghostty-head/src`, either by moving the default in `check_comment_coverage.py` (and
`check_comments_fidelity.py`, which has the same `ghostty->rustty` map assumption) or by exporting it
in `.port/tools/ci.sh`. That is a one-line default change plus a worktree, it is reversible, and it
lets the *whole* Phase-3 wave's G3b artifacts resolve in a single step rather than per-area. It also
removes the current perverse incentive where porting an upstream deletion turns a gate redder.

**Cross-check to run at the finale:** with `GHOSTTY_SRC` advanced and area G's doc work landed, the
config row should return to `missing=0`. If it does not, the residue is a *real* drop and must be
triaged individually — that is the whole value of doing (2) as a distinct, verifiable step.

---

## 8. Ledger corrections (`MAIN_PARITY.md`)

Only **2 of the ~20 commits touching area G** appear in `.port/phase-2/MAIN_PARITY.md` at all
(`grep -n` for all 20 SHAs → hits only for `b5f5ca078` and `5ea45919d`). Both rows are wrong:

| Row | Claim | Correction |
|---|---|---|
| `MAIN_PARITY.md:197` — `5ea45919d` | *"n/a-tier BL-8 · gtk tier unported; Config option lands with that tier"* | **Wrong.** The `Config` field has no GTK-apprt dependency. rustty-config already carries sibling GTK options (`config_struct.rs:4664` `gtk_wide_tabs`). Reclassify as **ABSENT, S**, and land it with area G's config work so the 204-field contract stops silently diverging from the oracle's field set. |
| `MAIN_PARITY.md:196` — `b5f5ca078` | *"n/a-tier BL-8 · Config docs for gtk option below"* | **Wrong for the same reason.** It is the one-line `"Available since 1.4.0."` on that option. Corroborating count: `Config.zig` has 63 `"Available since"` at the pin, 64 at head; rustty has 63. |
| `MAIN_PARITY.md:128` — `0fb89f4ff` | *"PORTED (WS7 D2 + D3) … + the 50 MB `scrollback-limit` docs"* | **Accurate for `0fb89f4ff` itself**, but the 50 MB default is **not from `0fb89f4ff`** — it is from `65c48213b`/`1092204df`, which post-date this ledger's `53bd14fec` triage head and have no row. The parenthetical is misleading; see §3. |

**All 16 other area-G commits are unledgered.** Consistent with the Phase-3 README's warning that
`MAIN_PARITY.md` cannot speak for `pkg/`, `po/`, `.github/`, `nix/`, or `flatpak/` — which is most
of this area.

---

## 9. Cross-area dependencies, ordering, and open questions

### 9.1 Cross-area dependencies (recorded, not owned here)

| Dep | Direction | Detail |
|---|---|---|
| **`f4c68d65e` `PageList::Limits`** | G → A–D (`src/terminal`) | The runtime-limits refactor is 670 lines of `PageList.zig` + 153 of `Terminal.zig`. Area G owns the config half; A–D owns the PageList half. **They cannot land separately** (§3.3). Also closes the deferral at `crates/rustty-vt/src/pagelist/compress.rs:165-173`. |
| **WS7 BL-1 lane** | blocks G-L2 | A concurrent lane owns `crates/rustty-vt/src/{compress,pagelist}/`; `f4c68d65e` rewrites the same `erasePage`/`fixupViewport`/`markActivity` paths. Sequence after. |
| **`Termio` limits wiring** | G → E (`src/termio`) | `Termio.init` gains `max_scrollback_bytes`/`max_scrollback_lines` from `.optional()`. |
| **3 new `po` msgids** | G ← F (`src/apprt/gtk`) | Sources are `application.zig:1708,1710,1721` (`88bd4fdce`, vicinae-hotkey-v1). Land the catalog entries **with** that port (V-4). |
| **`keybind` `global:` + `background-blur` doc rewrites** | G ← F | Both describe unported subsystems (`vicinae-hotkey-v1`, `ext-background-effect-v1`). The *doc text* is still portable now and should be, so the generated man page / webdata track the oracle. |
| **`gtk-horizontal-tab-scroll`** | G, not F | The Config field is area G's (§8). Only its *behaviour* belongs to the GTK tier. |
| **`z2d` 0.10.0 → 0.12.1 + native path insetting** | G identifies → F owns | `crates/rustty-z2d` + `rustty-font`'s sprite renderer (`d0e72a3ab`). Cost L. |
| **`vaxis` 0.5.1 → 0.6.0** | G identifies → shared | `crates/rustty-vaxis`; area G's `rustty-cli` TUI actions are the consumers. |
| **`zf` 0.10.3 → 0.11.0** | G identifies → shared | `crates/rustty-zf`; `list-themes` picker. |
| **`uucode` bump + `uucode_config.zig`** | G identifies → unicode | **Also a CI-toolchain dependency:** `.github/workflows/ci.yml:69` pins Zig 0.15.2 as the uucode oracle; the new uucode needs Zig 0.16. |
| **`libxev` bump** | G identifies → D6 | `crates/rustty-xev`. |
| **`iterm2_themes` 20260608 → 20260720** | G owns | `crates/rustty-extra/themes/` re-vendor + `README.md` provenance update. Requires fetching `ghostty-themes-release-20260720-153658-97e244c.tgz`. |
| **G3b `GHOSTTY_SRC`** | G identifies → Phase-3 finale | Whole-wave concern; see §7. Affects `check_comment_coverage.py` and `check_comments_fidelity.py`. |
| **WS13** | G ← WS13 | WS13 T4 owns `.pot` regeneration and therefore owns the `#:` reference class (V-2). WS13's docs half (mdgen/webgen) is **unaffected** by Phase 3 — those files changed by Zig-0.16 boilerplate only. |

### 9.2 Recommended intra-area ordering

1. **Independent, land any time (all S):** `src/synthetic` Kitty pair (4 items) · `url.zig`
   trailing-space fix in **both** copies · `background-blur` doc · `keybind global:` doc ·
   `gtk-horizontal-tab-scroll` · `edit-config` error string · V-3 comment fix.
2. **Then, one batch (M):** `iterm2_themes` re-vendor + a digest/manifest gate so future bumps are
   detectable.
3. **Then, gated on WS7 BL-1 (L):** the runtime-limits unit G-L1…G-L5 (§3.3).
4. **With the vicinae/GTK port (S):** the 3 new `po` msgids + the `Close Split` msgid additions +
   the zh_CN `关闭分屏` translation.
5. **At the Phase-3 finale:** advance the pin **and** `GHOSTTY_SRC` together (§7), then re-run G3b
   and triage any residue as real.

Fold the `gtk-horizontal-tab-scroll` addition into the same commit as G-L3's rename so the 204→206
count assertions and the 5 goldens are regenerated once, not three times.

### 9.3 Open questions, each with a recommendation

**OQ-1 — Does area G port the 3 new vicinae msgids now, or with the GTK port?**
*Recommendation: with the GTK port.* Landing them now creates 96 catalog lines that translate
nothing and are reachable from nothing, and WS13 T1's coverage gate is designed to fail on that
shape. There is no user-visible cost to waiting: all 32 upstream files have an empty `msgstr`.

**OQ-2 — `iterm2_themes`: re-vendor now, or defer to release infra?**
*Recommendation: re-vendor now, and add a digest gate.* Themes are user-visible data that
`xtask resources` ships on every build, and the current provenance `README.md` pins a tarball
upstream has moved past. Cost is dominated by the fetch. The gate matters more than the bump: today
nothing would detect content drift in an existing theme file, only a count change.

**OQ-3 — How much residual risk is in the `src/build` / `pkg` sampling?**
*Recommendation: one targeted follow-up sweep, ~1 hour.* `f2a7652ab` proved the hazard is real — it
smuggled `omitFramePointer` (a genuine cross-platform requirement) into a commit whose subject reads
as pure Zig-0.16 cleanup. I read `f2a7652ab`'s bullet list in full and caught it, plus its libSystem
sibling. But `SharedDeps.zig` (+293) and `GhosttyLibVt.zig` (+203) were **not** read line-by-line.
Recommend `git show f2a7652ab -- src/build/SharedDeps.zig src/build/GhosttyLibVt.zig` and
`git show e8525c0fd -- src/build/SharedDeps.zig` before area G is considered closed. I do not
believe anything is hiding there — every other commit touching those files is unambiguously
toolchain work — but "I do not believe" is not the evidence standard this program uses.

**OQ-4 — Should `xtask` gain a manifest gate for the two duplicated `url` regexes (P-4)?**
*Recommendation: yes, and it is trivially cheap.* A single test asserting that
`rustty_config::url::regex()` and `rustty_vt::string_map`'s test-local regex produce byte-identical
strings would have made `9fc2a3085` a one-line-per-site fix with a guarantee, instead of a
two-site edit with a silent-divergence hazard. This is a fidelity hole that outlives Phase 3.

**OQ-5 — Does the pot's non-regenerability (V-2) block a `po/` gate?**
*Recommendation: accept it for Phase 3; WS13 T4 resolves it.* Until extraction runs against
`crates/rustty-apprt/`, `po/io.airely.rustty.pot` is hand-maintained data and no regenerate-and-diff
gate is possible. WS13 T1's msgid-coverage harness is the right interim guard because it checks the
*sets* agree without needing the `.pot` to be reproducible.

**OQ-6 — Is the 204-field contract's fidelity claim currently true?**
*Recommendation: no, and say so in the contract.* rustty has 204 config fields; upstream head has
204 + `scrollback-limit-lines` + `gtk-horizontal-tab-scroll` − 0 = 206 (with `scrollback-limit`
renamed). The three `204` assertions guard rustty against *itself*, which is useful, but nothing
asserts the set matches the oracle's. Recommend the count-assertion comment record the oracle SHA
the number was last reconciled against, so the next drift is visible rather than inferred.

---

## 10. Bottom line

Area G's 507-file, +4,830/−2,680 footprint contains **20 real items**, of which **6 are cross-area
dependency bumps**, **3 are documentation-only**, **4 are one self-contained new file pair**, and
**1 is a design decision** (§3) that is genuinely consequential and genuinely blocked on another
lane. `pkg/`, `.github/`, `nix/`, and `flatpak/` are **wholly N/A** — including both upstream
deletions, for which rustty already holds the structural equivalent. `po/` is **99 % N/A**: one
translated string.

The two findings I would not want a reviewer to miss:

1. **rustty's `scrollback-limit` matches neither the pin nor upstream head**, and ships a doc block
   that denies a feature whose default value it borrowed. The fix is a three-commit unit, not a
   config tweak, and it retires an already-documented correctness deferral.
2. **G3b's config row is currently 100 % false positives**, and area G's own doc work will push it
   from 9 to ~39 before the finale. Advancing the pin document does not fix it — the gate reads the
   oracle *working tree*. Point `GHOSTTY_SRC` at a `6ad1fe7d8` worktree; do not check out in
   `../ghostty`.
