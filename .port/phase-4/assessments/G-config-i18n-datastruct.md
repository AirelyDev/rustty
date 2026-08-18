# Phase 4 assessment — Area G: config / datastruct / i18n / repo meta

Range: `6ad1fe7d8` (2026-07-29, Phase-3 sync) → `12967b68f` (2026-08-17, oracle head = new pin).
Area file-set: `src/config` · `src/datastruct` · `po` · `.github` · `typos.toml` · `PACKAGING.md`
· `src/global.zig` · `src/simd` · `src/unicode` · `src/benchmark/main.zig`.

Area footprint (measured): `src/config` +216/−56 (6 files) · `src/datastruct` +356/−498 (8 files)
· `po` **+27,267/−2,289 (35 files)** · `.github` +399/−65 (9 files) · simd/global/benchmark-main/
typos/PACKAGING +121/−58 (5 files) · `src/unicode` **zero commits in range**.

**102 non-merge commits** touch the area (`git log --oneline --no-merges 6ad1fe7d8..12967b68f -- <paths>`).
Every verdict below was re-derived from the oracle working tree (now AT the pin) and the rustty
tree. No ledger row was trusted; hash citations in comments were not accepted as evidence.
Nothing was implemented; the oracle was never written to.

---

## 1. Summary

### 1.1 The headline

**The area's porting debt is real but narrow: ~9 substantive code items, one large mechanical
i18n data wave, and three deletions blocked on other areas' rewrites.** The po/ wave dominates
the line count (+27k lines) but is translation *data*: the template grew **79 → 253 msgids**
(+179/−5) for the command-palette i18n feature, every catalog was msgmerge'd and many were
filled by translators, and a NEW Danish catalog (`da.po`) appeared. rustty's `po/` is byte-frozen
at the **exact range-start state** (79 msgids per file, matching non-empty msgstr counts) — the
entire wave is absent, and it is mechanical to port (msgid/msgstr mirror + R2 casing renames).

Two REAL bug fixes are absent and both have live rustty callers today:
- **`29b82dd80` config hex-escape byte preservation** (`\xNN` must be a raw byte, not a
  codepoint — the zig-byteslice rule made executable; rustty `string.rs` still utf8-encodes
  every escape).
- **`60b4a3585` CircBuf shrink metadata fix** (resize-to-smaller leaves `head/tail/full`
  inconsistent; rustty `circ_buf.rs` has the old buggy body; live callers: vt search
  sliding_window, apc glyph glossary, tmux viewer, inspector widgets).

Upstream **deleted three datastruct files** (`array_list_collection`, `segmented_pool`, `lru`);
rustty still carries all three as live modules. `lru` has **zero rustty callers** (verified;
grep hits in z2d/apprt-c are false positives: `FillRule`, `-lrustty…`) and can be deleted now.
The other two deletions are **blocked**: `ArrayListCollection` is still used by
`rustty-renderer/src/cell.rs` (deletion must land WITH the `cde7f9343` cell-storage rewrite)
and `SegmentedPool` is still used by `rustty-termio/src/exec.rs` (must land WITH the
`e0ef934f7` MemoryPool rewrite). Deleting either early is worse than absent.

### 1.2 Per-verdict counts (102 commits)

| Verdict | Count | Notes |
|---|---|---|
| PORTED | **0** | Nothing from the range has landed in this area yet (expected: Phase-4 re-vet establishes debt) |
| PARTIAL | **2** | `df23bef0e`+`a00d155e9` (command-translation loop: rustty's old code equals the non-GTK arm of the end state) |
| ABSENT | **62** | 13 config, 7 datastruct, 1 global, 1 simd, 3 benchmark-main, 2 typos, 1 PACKAGING, 31 po, 3 .github |
| N-A | **38** | 28 VOUCHED.td, 7 ghostty-CI-only workflow commits, `98b828a02` (flush: rustty path unbuffered), `15c50c1db` config hunks (Zig std.Io plumbing), `e40a5475d` (governance section rustty deliberately forked) |

### 1.3 Sub-area map

| Sub-area | Commits | Verdict mix | Real debt |
|---|---|---|---|
| `src/config` | 17 | 13 ABSENT / 2 PARTIAL / 2 N-A | hex-escape fix, theme-tilde end state, `drag-handle`, `link-osc8`, edit.zig path handling, 4 doc rewrites, open_config default |
| `src/datastruct` | 7 | 7 ABSENT | CircBuf fix+3 tests, `SplitTree.locate`, lru delete NOW, 2 deletions bundled w/ other areas, 2 new files owned by area C |
| `po/` | 32 | 31 ABSENT / 1 N-A | template 79→253 msgids, all catalog fills, NEW `da.po`, README N_ note |
| `.github` | 38 | 3 ABSENT / 35 N-A | issue-template deletion mirror, issue-triage checkbox, paths-filter 4.0.2→4.0.3 |
| simd / global / bench / typos / PACKAGING | 8 | 8 ABSENT | scalar UTF-8 decoder rewrite; the rest are couplings to area C/E ports |
| `src/unicode` | 0 | — | none |

---

## 2. Per-commit verdicts

### 2.1 src/config (17)

| Commit | Verdict | Evidence |
|---|---|---|
| `29b82dd80` config: preserve bytes in hex escapes | **ABSENT** | `crates/rustty-config/src/string.rs:246-249` still routes EVERY escape through `utf8_encode(cp, …)`; no `is_byte_escape` branch; the new test `parse: hex escapes are bytes` (`"\xe6\x97\xa5…"` → `"日本語"`) is missing. **Gate blind spot**: oracle `string.zig` now has 10 `test{}`s, rustty has 10 `#[test]`s — but rustty's 10th is a Rust-only addition (`parse_escaped_surrogate_half`, string.rs:520), so `config_string 10/10 ok` in g1b is bookkeeping-green while the new upstream test is missing. Zig-byteslice rule (`Vec<u8>` fidelity) is the point of this fix. |
| `f748b17e2` expand tildes in theme path | **ABSENT** (superseded → end state) | First of a 4-commit rewrite chain; assess end state at `37174c73b`+`68a2008b3`. Intermediate `theme.zig` version must NOT be ported. |
| `c12173429` move shell expand theme.zig→config.zig | **ABSENT** (superseded → end state) | Second chain member; its `parseCLI`-time expansion was replaced by `Theme.finalize`. Carries no surviving test (its test was rewritten by `37174c73b`). |
| `37174c73b` add helper, expand light/dark too | **ABSENT** | End state: file-scope `fn expandHome(path, buf)` helper (Config.zig:4521-4543 upstream), `Theme.finalize` expanding light AND dark, `Config.finalize` calling `theme.finalize(alloc)` before `loadTheme`, `WorkingDirectory.finalize` refactored onto the helper, and the rewritten `Theme` test (`light:~/foo, dark:~/bar` + finalize). rustty: no `expand_home` helper in `config_struct.rs`; `Theme` in `value_types.rs:6729-6820` has NO `finalize`; `Config::finalize` (`config_struct.rs:7014-7042`) does not call it; the test is absent. `WorkingDirectory::finalize` (`value_types.rs:1938-1993`) keeps the OLD inline expansion — behaviorally equal but not the end-state shape. |
| `68a2008b3` fix: format | **ABSENT** (bundle member) | Whitespace-only on `Theme.finalize`; folded into the end state above. |
| `fff9f62f3` cli: always install embedded SSH terminfo | **ABSENT** (doc hunk) | Config.zig hunk is doc-only (`ssh-terminfo` wording). rustty `config_struct.rs:3865-3867` still has the OLD text ("using `infocmp` and `tic` … Requires `infocmp` to be available locally"). Behavior change lives in `src/cli/ssh*` (cli area owns). |
| `98b828a02` flush autogenerated config template | **N-A** | The Zig bug is a missing `writer.flush()` on a buffered file writer. rustty `write_config_template` (`config_struct.rs:5391-5416`) uses unbuffered `File::write_all` — there is no buffer to flush; the bug cannot exist. Suggest updating the mirrored Zig comment block to quote the new reference body when convenient (R1 comment currency), but no behavior port exists. |
| `b68eb67e9` config/edit: better handling of existing paths | **ABSENT** | Upstream `edit.zig` now returns `ConfigPathResult{name, exists}`, skips all create attempts when the file exists, and only `createDirPath`s after an `openDir(..., follow_symlinks=true)` FileNotFound probe. rustty `edit.rs:30-76` still unconditionally `create_dir_all`s + create-news; `config_path()` (`edit.rs:84`) still returns a bare path with no `exists` flag. No upstream test added (config_edit 0/0 in g1b). |
| `0a183c923` core/gtk: edit config in Ghostty window | **ABSENT** (config hunk; cross-area) | Config.zig hunk: default keybind payload `.{ .open_config = {} }` → `.{ .open_config = .default }`. rustty `rustty-input/src/binding.rs:1632,2117` still parses `open_config` as a void arm — the `OpenConfig` payload union lives in `src/input`/apprt (their areas); the config-side default-keybind change can only land with it. Its po/ hunks are counted in the po wave (§2.3). |
| `c6e7e9e4e` config: add `drag-handle` | **ABSENT** | `grep drag_handle\|DragHandle crates/` → 0 hits. New option `@"drag-handle": DragHandle = .auto` + `DragHandle` enum missing from `config_struct.rs`/`value_types.rs`. |
| `74f91d1b4` macOS: support `drag-handle` | **ABSENT** (doc end state) | Rewrites the option's doc (drops "Linux only", adds macOS semantics). Port the `74f91d1b4` doc text when `c6e7e9e4e` lands (single end-state item). macOS runtime support is area F. |
| `77537c806` macos: untrusted OSC8 handling | **ABSENT** (config hunk; cross-area) | Adds NEW option `@"link-osc8": bool = true` (not doc-only). 0 hits for `link_osc8` in all crates. The OSC8 apprt/macOS machinery is area F's; the option + C getter are portable now. |
| `a00d155e9` limit command translations to GTK | **PARTIAL** (end state w/ `df23bef0e`) | End state: `RepeatableCommand.init` loops defaults with `localized = if (gtk) cmd.translated() else cmd`, appending `localized` + `try localized.cval(alloc)`. rustty `value_types.rs:5971-5982` still appends precomputed `defaults()`/`defaults_c()` slices — byte-identical BEHAVIOR to the non-GTK arm (macOS-first build), but the loop shape, the GTK-gate comment, and the `translated()` call are unported. Coupled to `src/input/command.zig` `translated()` (input area). |
| `df23bef0e` i18n cmd-palette translation support | **PARTIAL** | Same function as above; superseded by `a00d155e9`'s gating. Its po/ additions are in the po wave. |
| `85083d23c` clarify cursor-click-to-move doc | **ABSENT** | rustty `config_struct.rs:1840` still reads "This feature requires shell integration, specifically prompt marking" (old text). |
| `a7cfa6fc2` update scrollbar doc | **ABSENT** | rustty `config_struct.rs:2376-2378` still has the OLD `system` bullet ("respect the *Scrollbar behavior* system setting…"). |
| `15c50c1db` crash: do not use global state (config hunks) | **N-A** (config hunks) | The 4 config-file hunks only thread `global.io()`/`testing.io` as a new first arg into `internal_os.{home,expandHome,xdg.config}` — Zig `std.Io` plumbing with no Rust analogue (rustty-os takes `&environ_map` and uses std::fs directly). The crash-side substance (`src/crash/*`, `src/cli/crash_report.zig`) belongs to area E/cli. |

### 2.2 src/datastruct (7) + src/global.zig (1)

| Commit | Verdict | Evidence |
|---|---|---|
| `60b4a3585` Fix CircBuf metadata after shrinking | **ABSENT** | rustty `circ_buf.rs:165-190` is the OLD body: fix-ups only in the `size > prev_cap` branch; the end-state `retained_len = min(prev_len, size)` + unconditional `head/tail/full` reset is missing, as are the 3 new tests (`resize shrink partially full`, `resize shrink to length`, `resize empty to zero`). Measured: oracle circ_buf.zig 24 `test{}`s vs rustty 21 `#[test]`s — exactly the 3. **Gate blind spot: no `circ_buf` module row exists in g1b**, so this drop is invisible to the test-count gate. Live callers: `rustty-vt/src/search/sliding_window.rs`, `rustty-vt/src/apc/glyph/glossary.rs`, `rustty-vt/src/tmux/viewer.rs`, `rustty-inspector/src/*`. Safe-Rust bug (Vec), not UB. |
| `02f34835e` remove unused LRU implementation | **ABSENT** — deletable NOW | Oracle `main.zig` no longer references lru. rustty still has `crates/rustty-datastruct/src/lru.rs` + `pub mod lru` (`lib.rs:21`). Zero real callers verified (case-insensitive grep across crates; the only hits are `FillRule`/`-lrustty_apprt_c` false positives). Delete file + export + lib.rs doc-comment mention. |
| `e0ef934f7` termio: replace SegmentedPool with std.MemoryPool | **ABSENT** — deletion BLOCKED (atomic bundle B3) | Upstream deleted `segmented_pool.zig` and rewrote `termio/Exec.zig` ThreadData onto `std.MemoryPool` (drops the ~10KB preallocation). rustty `SegmentedPool` is LIVE: `rustty-termio/src/exec.rs:61,564-565,1597,1603,1685-1686,6405-6406`. The datastruct deletion must land in the same change as the termio (area E) rewrite. **UB/SB hazard**: pooled `xev::WriteRequest` elements are intrusive-raw-ptr linked (see memory `intrusive-ptr-and-placeholder-fill`; `exec.rs:1648` comments the Default contract, `rustty-xev/src/stream.rs:133-164` documents the aliasing protocol). The MemoryPool replacement must preserve stable element addresses for requests in flight; prospective LIFETIMES.tsv row required. |
| `cde7f9343` renderer: simplify cell row storage | **ABSENT** — deletion BLOCKED (atomic bundle B2) | Upstream deleted `array_list_collection.zig` (−44) as part of a renderer/cell.zig rewrite (+154/−109). rustty keeps `array_list_collection.rs` with live caller `rustty-renderer/src/cell.rs` (+ comment refs in metal/opengl buffer.rs). The renderer area owns the cell.rs rewrite (g1b `renderer_cell drop=1` is this commit's test churn); the datastruct file deletion rides with it. |
| `590d669c4` kitty: limit png decoder allocations | **ABSENT** (area C/D owns the port) | NEW `src/datastruct/limited_allocator.zig` (+124, with its own tests) + main.zig export; caller is `terminal/kitty/graphics_image.zig` (+79, allocator-boundary + crafted-PNG regression tests — part of g1b `kitty drop=11`). No `limited_allocator`/`LimitedAllocator` anywhere in rustty. Port as one bundle with the kitty change; the datastruct file must not land caller-less. |
| `492c26067` custom memory pool for Wasm | **ABSENT** (area C owns the port) | NEW `src/datastruct/wasm_page_pool.zig` (+157) + export; caller `terminal/PageList.zig` (+132/−50, wasm-gated allocator). No `wasm_page_pool`/`WasmPagePool` in rustty. **Hazard**: PageList is a raw-pointer structure (Page/Row/Cell) — the pool port falls under R9/LIFETIMES.tsv + raw-ptr-args discipline; flag for area C. |
| `1d053bd6e` gtk: drag-to-move splits (split_tree hunk) | **ABSENT** | Adds `SplitTree.locate(view: *const View) ?Node.Handle` (pointer-equality lookup, 12 lines, no test). No `fn locate` in `rustty-datastruct/src/split_tree.rs`. Standalone-portable (pointer equality maps to `std::ptr::eq`); the GTK caller is area F's. |
| `a82637b53` crash: sentry dirs on init thread (global.zig hunk) | **ABSENT** (hunk; substance area E) | global.zig hunk changes environ-map snapshot ownership (`crash.init(alloc, environ_map)` by value, freed by the init thread). rustty-global `lib.rs:361-367` still mirrors the OLDER `crash.init(self.alloc)` comment text, and `rustty-crash/src/sentry.rs:167 init()` takes no environ map — the wiring hunk can only land with area E's sentry.zig rewrite (+91/−?, dir resolution moved onto the init thread). |

### 2.3 po/ (32 commits; +27,267/−2,289)

**Measured end state**: oracle template `com.mitchellh.ghostty.pot` went **79 → 253 msgids**
(+179 new, −5 removed; sampled additions are command-palette strings: "Change Window Title…",
"Open Configuration in New Window", "Reset Terminal", …). Every one of the 32 oracle catalogs is
at 253 msgids; rustty's 33 catalogs (incl. renamed `io.airely.rustty.pot`) are ALL at **exactly
79 msgids** with non-empty-msgstr counts matching the range-START state file-for-file (verified
per-file: e.g. `be.po` oracle-at-6ad1fe7d8 79/68 ≡ rustty 79/68; oracle-now 253/246).
`po/da.po` (Danish, NEW: 253 msgids / 230 translated) has no rustty counterpart.

| Item | Verdict | Commits |
|---|---|---|
| Template growth + all catalog updates + fills | **ABSENT** (one mechanical wave; port the END STATE of each file, R2 renames applied — 9 `Ghostty` occurrences in the oracle pot need per-token renaming, `rename-casing-fidelity`) | `f03d71d97` (template), `e0744dde6` (empty palette entries), `31f4931f8` (missing strings), `8b7c57c75`/`0a183c923` po-hunks (new msgids), `e9e7864b4` (drop fuzzy entries), plus per-locale fills: `820ed688e` `e2662d7b6` (be), `06b63c485` `7c1d0d414` `d7f782dbc` `aa25679da` (he), `1eceea92d` (ko_KR), `375ce7874` (nb), `034f5843f` `1d9fcdc15` (kk), `93e7e7e99` (zh_CN), `365e0bd00` (es_AR), `250a36fe6` (es_ES), `987ee5275` (hu), `8fd2013a0` (pl), `27d164287` (tr), `dd22396bd` (es_BO), `491806fbe` `b3514d562` `b104a3f0b` (lv) |
| NEW Danish catalog `po/da.po` | **ABSENT** | `af9ead681` (add), `bd381c4a5` `ca5b3dc77` (updates), `47136687d` (palette translations) — port the end-state file once |
| `1125fa26d` README_CONTRIBUTORS: `N_`/comptime-msgid note | **ABSENT** | rustty `po/README_CONTRIBUTORS.md` (80 lines vs oracle 96) lacks the 24-line `N_` section; no `N_` in `rustty-os/src/i18n.rs` either — the doc mirrors area E's `os/i18n.zig` `N_` port; land together (Rust-adapted example, as the file's existing style does) |
| `e40a5475d` README_TRANSLATORS teams rewrite | **N-A** | The rewrite touches ONLY the "Localization teams" section (verified via hunk map), which rustty deliberately forked into a minimal-governance pointer to upstream ("no separate vouch process… see Ghostty's upstream translators' guide") — the pointer target now contains the new text; nothing to mirror |

### 2.4 .github (38 commits) + typos.toml + PACKAGING.md + simd + benchmark

| Commit | Verdict | Evidence |
|---|---|---|
| 28× "Update VOUCHED list" (`3924a3255` `7b57c0a02` `cecf81678` `348f714ff` `e84dd3015` `f81dcadc8` `89a26a39e` `710b87239` `43fe69907` `bb019cac2` `47703753a` `d2c70a8c7` `9f9b8d1d0` `fad7f854e` `426386b85` `94d775fef` `09557e91d` `6e647a1cb` `47147324c` `7f96e0bda` `2346c4fe4` `08342c924` `bdd849fc2` `a4f9b9cea` `6837d7027` `74ad15c10` `70c498ac3` `96e39f823`) | **N-A** | rustty carries no `.github/VOUCHED.td` (verified `ls .github` + grep); the vouched-contributor roster is ghostty's AI-policy instrument, superseded by rustty's own `AI_POLICY.md` |
| `4a516fa39` remove the issue templates | **ABSENT** | Oracle `.github/ISSUE_TEMPLATE/` is GONE; rustty still carries `.github/ISSUE_TEMPLATE/{config.yml,preapproved.md}` (rebranded mirrors). Mirror the deletion (rustty README/links check: `preapproved.md` is the AI-preapproval template ghostty abandoned). |
| `04d1939d5` issue-triage: documentation-search checkbox | **ABSENT** | rustty `.github/DISCUSSION_TEMPLATE/issue-triage.yml` is a faithful rebrand but its checkbox list lacks the new required "searched the documentation" item (diff against oracle shows rustty has 1 of the oracle's 2 search checkboxes). One 2-line hunk. |
| `c285d3c24` bump dorny/paths-filter 4.0.2→4.0.3 | **ABSENT** | rustty `release-tip.yml:95` pins `dorny/paths-filter@7b450fff… # v4.0.2`; oracle moved to 4.0.3. Mechanical pin bump (pinact-managed). |
| `7a966438f` bump cachix/install-nix-action | **N-A** | No nix workflows / no `cachix` uses anywhere in rustty `.github/workflows` (grep 0). |
| `a8e9b413f` `c75559589` `29a70bc36` `b112f3954` `49fd1ae65` `66ea61dd9` (+ `1fdbb8c91`'s test.yml hunk) | **N-A** | All touch ghostty's Zig-build CI (`test.yml`/`release-tip.yml`): wasm tip artifacts, ABI-manifest schema check, kaitai verification, iOS build removal, lib-vt default deps. rustty CI is its own (`ci.yml`/`ci.sh` gate stack). CAVEAT for area C: when snapshot/kaitai and wasm artifacts land, equivalent rustty CI checks should be *considered* there (their gate design, not a mirror of these files). |
| `82df79ec8` TinyIo (typos.toml hunk) | **ABSENT** (bundle w/ area C TinyIo) | Oracle adds `tio = "tio"`, `WRONLY = "WRONLY"` extend-words. rustty typos.toml lacks both; meaningful only once area C ports `src/lib/TinyIo.zig` — land together. |
| `6b09bb3fc` ignore snapshot hex files in typos | **ABSENT** (bundle w/ area C snapshot) | Oracle excludes `src/terminal/snapshot/testdata/*.hex`; rustty needs the equivalent `crates/rustty-vt/src/…/snapshot/testdata/*.hex` exclude WHEN area C imports the fixtures — land with them (excluding a non-existent path early is harmless but pointless). |
| `87f69a12e` faster vt_write on wasm (simd/vt.zig + PACKAGING.md hunks) | **ABSENT** | rustty `rustty-simd/src/vt.rs:98,163-200` still has `Utf8SeqInfo`/`utf8_seq_info` (deleted upstream) and the old per-byte ASCII path; the end-state scalar decoder (vectorizable ASCII bulk loop + direct bit-assembly with lead-dependent first-continuation range, `cb1_lo/cb1_hi`) is unported. The existing scalar↔SIMD differential test carries the regression check once ported. PACKAGING.md "WebAssembly (libghostty-vt)" section also absent (rustty PACKAGING.md has no wasm section — 0 grep hits). Cross-area residue: stream batched-path (`src/terminal/stream.zig`, terminal area), wasm `Style.eql` PackedStyle (vt area), simd128 build default (build). The simd scalar rewrite is standalone-safe (semantics-preserving by the differential test). |
| `dc52c248e` benchmark: terminal-resize (main.zig hunk) | **ABSENT** | One re-export line; `rustty-bench/src/` has no `terminal_resize` module (new `src/benchmark/TerminalResize.zig` is the owning area's file; the export lands with it). |
| `0b5e12453` benchmark: terminal-snapshot (main.zig hunk) | **ABSENT** | Same shape; `terminal_snapshot` bench requires area C's snapshot codec first. |
| `85b1dd0dd` benchmark: formatter (main.zig hunk) | **ABSENT** | Same shape; `TerminalFormatter.zig` benchmark portable once its file (owning area) lands; rustty formatter exists today. |
| `1fdbb8c91` -Dvt-features (PACKAGING.md hunk) | **ABSENT** (bundle w/ area C `-Dvt-features`) | PACKAGING.md gains the `-Dvt-features` packaging notes; rustty's equivalent is cargo-feature flags on rustty-vt/lib — document when the feature-gating port lands (area C/build). |

---

## 3. Atomic bundles

- **B1 — theme-tilde end state**: `f748b17e2` → `c12173429` → `37174c73b` → `68a2008b3`.
  Port ONLY the final shape (Config-level `expandHome` helper + `Theme.finalize` +
  `Config.finalize` call + `WorkingDirectory.finalize` refactor + the rewritten test).
  The two intermediate placements are superseded-in-range traps.
- **B2 — `cde7f9343`**: renderer cell.rs storage rewrite (renderer area) + delete
  `array_list_collection.rs` + its `lib.rs` exports. Deleting first breaks the build;
  porting the rewrite without deleting leaves a dead module (R1 inventory drift).
- **B3 — `e0ef934f7`**: termio exec.rs MemoryPool rewrite (area E) + delete
  `segmented_pool.rs` + exports. UB hazard on pooled WriteRequest addresses (§5).
- **B4 — `590d669c4`**: `limited_allocator` + kitty `graphics_image` caller + its tests
  (part of kitty drop=11) — area C/D owns; file must not land caller-less in datastruct.
- **B5 — `492c26067`**: `wasm_page_pool` + PageList wasm allocator (area C; raw-ptr Page).
- **B6 — command-translation end state**: `df23bef0e` + `a00d155e9` (one function; port the
  gated loop once, with input-area `translated()`).
- **B7 — drag-handle doc end state**: `c6e7e9e4e` + `74f91d1b4` (option once, final doc text).
- **B8 — typos.toml rows ride their features**: `82df79ec8`-hunk with TinyIo, `6b09bb3fc`-hunk
  with snapshot testdata (both area C).

## 4. Cross-area couplings (named)

- **Area C (lib/wasm/snapshot/TinyIo)**: B4, B5, B8, `0b5e12453` snapshot bench,
  `1fdbb8c91`/`87f69a12e` PACKAGING sections, wasm CI N-A caveat.
- **Renderer area**: B2 (`cde7f9343`; g1b `renderer_cell drop=1`).
- **Area E (os/termio/crash)**: B3 (`e0ef934f7`); `15c50c1db` + `a82637b53` crash/sentry
  substance; `os/i18n.zig` `N_` (pairs with the `1125fa26d` README note).
- **Input area**: `0a183c923` OpenConfig payload union (config default-keybind hunk waits on
  it); `command.zig translated()` for B6.
- **Area F (apprt/macOS/GTK)**: `77537c806` OSC8 runtime handling (config option portable now),
  drag-handle runtimes, `1d053bd6e` GTK drag-to-move (locate() portable now).
- **cli area**: `fff9f62f3` ssh-terminfo behavior (config hunk is doc-only).

## 5. UB/SB hazards (raw-pointer discipline)

1. **B3 SegmentedPool→MemoryPool**: pooled `xev::WriteRequest`s are intrusive-raw-ptr list
   nodes whose addresses are held by the event loop while in flight
   (`rustty-xev/src/stream.rs:133-164` documents the aliasing contract; `exec.rs:1648`).
   Any replacement pool must guarantee stable addresses and never hand out `&mut` overlapping
   a queued request — port with raw-ptr args + a LIFETIMES.tsv row, Miri-harness proof per
   `shaper-hook-sb-clean-interleave` if aliasing is restructured.
2. **B5 WasmPagePool feeds PageList** (Page/Row/Cell raw-ptr structure): area C must apply the
   R9/raw-ptr-args rules; noting here because the pool file sits in this area's tree.
3. `1d053bd6e locate` uses pointer-equality over tree views — implement with `std::ptr::eq`
   on shared refs; no `&mut` aliasing introduced.
4. CircBuf fix (`60b4a3585`) is safe-Rust (Vec-backed) — correctness bug only, but note the
   vt search subsystem (a caller) already has a known SB-UB finding
   (`search-screen-sb-ub-finding`); fixing resize semantics does not interact with it.

## 6. Gate blind spots found while vetting (report, do not weaken)

- **g1b `config_string 10/10 ok` is bookkeeping-green**: the new upstream hex-escape test is
  missing; the count is padded by a legitimate Rust-only surrogate-half test. Negative control
  for the port: reverting the `is_byte_escape` branch must fail the new `#[test]`.
- **datastruct has no per-module rows in g1b** (no `circ_buf` lane), so the 3 dropped CircBuf
  tests (24 zig vs 21 rust, measured) are invisible to the test-count gate. The finale G1b run
  should either gain datastruct lanes or this file's counts serve as the record.

## 7. Dropped-test targets (w0/g1b-after-at-12967b68f.txt)

**None of the 10 RED modules belong to this area** (formatter, apc, stream, kitty, modes,
input_key_encode, cli_ssh_cache_diskcache, font_shaper_harfbuzz, renderer_cell,
apprt_api_surface). Two RED lanes are *couplings* of this area's bundles: `kitty drop=11`
includes `590d669c4`'s allocator-boundary tests (B4) and `renderer_cell drop=1` is
`cde7f9343`'s churn (B2). Conversely, this area's real test debt (config_string hex test,
3 CircBuf tests, Theme finalize test) is NOT captured by g1b — see §6.

## 8. Coverage honesty

**Exhaustive (every hunk read line-by-line):**
- All 17 `src/config` commits: full `git show` of every config hunk (Config.zig, string.zig,
  theme.zig, edit.zig, file_load.zig, path.zig), and the corresponding rustty code read at
  cited lines (string.rs parse + escape decoder in full; Theme/WorkingDirectory impls in full;
  Config::finalize head; edit.rs in full; write_config_template in full).
- All 7 datastruct commits + both deletions' rustty caller sets (grep-verified across all
  crates, false positives inspected); circ_buf.rs resize/rotate read in full; oracle
  `main.zig` end state read in full.
- `87f69a12e` simd/vt.zig diff in full + rustty vt.rs decoder region; `a82637b53` global.zig
  hunk + rustty-global/rustty-crash counterparts; all 3 benchmark main.zig hunks; both
  typos.toml hunks + full file compare oracle-vs-rustty; both PACKAGING.md hunks + rustty
  PACKAGING.md checked for the sections.
- `.github` non-VOUCHED commits: `4a516fa39` (dir-existence check both sides), `04d1939d5`
  (full file diff rustty-vs-oracle), `c285d3c24` (pin grep both sides).
- po README commits: both diffs read in full; rustty READMEs read; hunk-section attribution
  for `e40a5475d` verified against rustty's forked section.

**Sampled / mechanical (filter stated):**
- `po/*.po` catalogs were NOT read line-by-line. Filter: per-file `grep -c '^msgid '` and
  `grep -c '^msgstr "..*"'` across all 32 oracle files, all 33 rustty files, and the oracle
  at `6ad1fe7d8` (spot: be.po, pot) — rustty ≡ range-start state exactly, oracle uniformly at
  253. Template delta measured with `grep -c '^[+-]msgid'` on the range diff (+179/−5) and the
  first 20 new msgids read. `Ghostty` occurrences in the oracle pot counted (9) for the R2
  rename check. Individual translation strings were not proofread (translator content;
  correctness of foreign-language text is out of scope for the port — byte-mirroring is the
  contract).
- 28 VOUCHED commits: classified by filename only (single-file `.github/VOUCHED.td` touches,
  verified via `--name-only` listing); contents not read (rustty carries no such file).
- 6 ghostty-CI commits: full `--name-only` attribution; test.yml hunks skimmed for anything
  non-CI (none found beyond the named features), not read line-by-line.
