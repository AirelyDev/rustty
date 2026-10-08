# Phase 8 delta review — R4: records, gates, coverage, build and CI (cross-cutting)

Range: pin `12752b2ac` (2026-09-28, `.port/tools/gates/ORACLE_PIN`) → `origin/main` `a60e9e2a5`
(2026-10-07). 107 commits, 69 non-merge, 85 files, +7,466 / −497. Rustty at `401b95b0e` (branch
`phase-8-upstream-resync`).

**Scope.** Everything cross-cutting: the commit inventory, the upstream test list, the UD / N / V /
LEDGER records the range touches, `build.zig.zon` and its renderings, `.github/`, `test/esctest/`,
the iTerm2 themes, and the effect of moving the pin on every registered gate. Per-commit port work
in `src/` belongs to R1 (terminal engine), R2 (OSC + C API) and R3 (renderer, apprt, macOS, CLI,
config, termio); this report only cites it where a record or a gate depends on it.

**Method.**
- Read-only. The oracle stayed detached at `12752b2ac` (checked before and after). The new side was
  read with `git show a60e9e2a5:<path>` and `git diff 12752b2ac a60e9e2a5`.
- Gates "after the advance" were **simulated**: `git archive a60e9e2a5` into
  `/private/tmp/claude-501/p8-r4/oracle_new`, made a throwaway git repository there (one commit, so
  the audits' `git ls-files src` works), with `zig-pkg/` symlinked to the oracle's (the deps are
  unchanged except the themes). A 15-line wrapper imports each gate with only
  `_gatelib.oracle_root` and `_gatelib.oracle_pin` redirected to that export, then runs the gate
  unchanged. Each gate was also run the same way **at the pin** as the "before" control: every one
  was GREEN there. Figures marked "by reading" were not run.
- Test blocks were extracted from every changed `.zig` file at both revisions, and per commit, with
  a brace matcher that skips strings, multi-line strings and comments; bodies were compared byte for
  byte.
- UD reference lines were mapped through the range's `-U0` hunks (a line inside a hunk is TOUCHED,
  otherwise it is shifted by the hunks above it).
- The new themes tarball was downloaded into its own directory, its sha256 checked against
  `flatpak/zig-packages.json` (`072d88d0…`), and compared file by file with
  `crates/rustty-extra/themes/`.

Companion files:
- `R4-commits.tsv`: all 69 non-merge commits. Columns: sha, date, subject, files, area (primary
  first), cited_by (which of R1–R4 cite the sha, from a grep of the four reports), flag.
- `R4-upstream-tests.tsv`: 120 rows, 90 added and 30 changed, 0 removed. Columns: file, test_name,
  change, sha (comma-separated when several commits shaped the end state).

---

## 1. Summary

| # | commit(s) | theme | observable? | Rustty status | size | top risk |
|---|---|---|---|---|---|---|
| 1 | all | pin advance `12752b2ac` → `a60e9e2a5`, gate arrival state | gates | — | M | G9's parity-surface generator silently drops CSI arms if regenerated as is (§5.3) |
| 2 | `80456f057`, `51d3ca168` (+`02c51decf`) | UD20 and UD312 fixed upstream, the same way as Rustty | record | ALREADY-HAS | S | the docs rows must go in the same commit as the status lines (G-UDDOCS) |
| 3 | `e8b858cc6`, `157c44f64`, `6fbb560cd`/`dfd7bafcd` | UD6, UD60, UD160: code moved, defects still live | record | re-anchor | S | none: Rustty's fixes stay |
| 4 | 47 more UDs | reference lines shifted | record | re-anchor | S | none |
| 5 | `a573781c6` | LEDGER S133-F1 (CLOSED) ported the behaviour upstream now calls "backwards" | Y (perf only) | ABSENT (R1) | S | the CLOSED row's text goes stale; no G-PARITY effect |
| 6 | `406f2e544` | iTerm2 themes `20260921-150923-0b55a9e` → `20260928-151043-99d9701` | Y | ABSENT | S | 617 is hard-coded in 4 files; G-THEMES is a Rustty-internal manifest and cannot see it |
| 7 | `438d584f0`, `edcaf6670` | `test/esctest` harness + report-only `test-esctest` CI job | CI | ABSENT | M | depends on R1's DECRQCRA/XTCHECKSUM and R2's exports; esctest2 is fetched Python |
| 8 | `a7cf951eb` | Windows: run `c-vt-effects` against the DLL | CI | ABSENT | S | none |
| 9 | `a141f9bdb` (`src/lib/windows_dll.zig`) | Zig DLL global-constructor workaround | Windows | N/A candidate (NON-PORT) | S | G-FILES/G-SYMS stay RED until the NON_PORTS row names the file |
| 10 | 15 × `Update VOUCHED list` | governance | no | N/A (NON_PORTS phase-3 G13 §6) | — | — |
| 11 | `build.zig.zon.{json,nix,txt}`, `flatpak/zig-packages.json` | renderings of the themes URL | no | N/A (phase-3 NON_PORTS "Zig-package and Nix renderings") | — | — |
| 12 | `05392dcc9` | WebGL stub renderer deleted | — | Rustty still has `webgl.rs` (R3) | S | `check_leaks.sh:1016` lists `rustty-renderer:webgl`; drop with the file |
| 13 | `9272a2f7c`, `00a19db51`, `bae2c3cdb`, `7b11f3dca` | new config keys, new OSC kinds | G9 | ABSENT (R1/R2/R3) | — | G9 RED after regeneration until ported |
| 14 | all | G-TESTS name matching masks 17 of the 90 added tests | gate | — | S | a checklist is the only meter, as in Phase 7 |

No `build.zig.zon` dependency other than the themes moved: libxev, libvaxis, zf, highway,
fontconfig, uucode, wuffs and Zig (0.16.0) are unchanged. So every dependency UD is untouched
(UD46–55, UD72, UD74, UD107, UD108, UD182, UD201, UD230's std part, UD245, UD246, UD249, UD252,
UD260–267, UD275, UD311, UD330, UD341, UD346, UD358–360, UD376, UD377). `include/ghostty.h` is
unchanged (A3 needs no sha bump), and no `po/` file changed (G-I18N untouched).

---

## 2. Records: UD, N, V and LEDGER reconciliation

### 2.1 UDs fixed upstream (2 of 205)

| UD | class | upstream | what upstream did vs Rustty | Rustty action |
|---|---|---|---|---|
| **UD20** (swapped selection uniforms) | FIXED-SAME | `80456f057` | `shadertoy.zig:40-41` now declares `selection_foreground_color` before `selection_background_color`, the GLSL order, exactly the field order Rustty's fix uses (`crates/rustty-renderer/src/shadertoy.rs`, R3 cites `:158-171`). Upstream adds the test "custom shader selection uniform layout matches compiled GLSL" (SPIRV-Cross member offsets vs `@offsetOf`). | Status line `fixed upstream in 80456f057, the same way`; remove the docs row (`docs/ghostty-bug-fixes.md:51`); rewrite the "reference defect (R13)" PORT NOTEs at the two fields as plain ports; port upstream's test under its own name (Rustty's `globals_block_members_receive_the_uniform_fields_they_are_named_for` checks all 27 members and can stay beside it). LEDGER S117-F1 (CLOSED, `shadertoy.zig:14-42`) is TOUCHED by the same hunk; re-anchor only. |
| **UD312** (`ghostty_osc_command_data(NULL)`) | FIXED-SAME | `51d3ca168`, test `02c51decf` | `commandDataTyped` now does `command_ orelse return false` (`c/osc.zig:133`), which is Rustty's fix (`crates/rustty-vt-c/src/osc.rs`, PORT NOTE UD312). | Status `fixed upstream in 51d3ca168, the same way`; remove the docs row (`:123`); add upstream's test "command data null" (R2 §2). |

The precedent is Phase 7's UD150: its docs row was removed when its status became "fixed upstream
in". `check_ud_docs.py` exempts a `fixed upstream in` entry from needing a docs row, so the two
edits go in one commit.

### 2.2 UDs whose cited code the range changes, still live

- **UD6** (oversized tmux block leaves the viewer alive). `e8b858cc6` moves the `max_bytes` check
  below the state switch (`control.zig:137-142`) so a line that exactly fills the buffer can be
  terminated. A block that really exceeds the limit still sets `broken` and returns
  `error.OutOfMemory`, and `dcs.zig` is unchanged, so the defect is live. Re-anchor:
  `control.zig:71-74` → `:137-142`, `stream_handler.zig:411` → `:426`. Rustty keeps `dcs.rs:204`.
  R1 agrees.
- **UD60** (UTF-8 mouse surrogate coordinate). `157c44f64` now encodes the **button** with
  `printUnicodeCodepoint(32 + button)` (always below `0x120`, never a surrogate). The coordinate
  `catch unreachable` arms are unchanged. Re-anchor `mouse_encode.zig:134-149` → `:134-147`
  (`utf8Encode` at `:142`, `:145`). R1 notes the port must keep validate-before-write.
- **UD160** (ssh-cache scoped IPv6 longer than 255 bytes). `6fbb560cd`/`dfd7bafcd` add
  `splitHostPort` and port handling, but `isValidHost` still calls `IpAddress.parseIp6(host, 0)`
  (`DiskCache.zig:488`). A bare scoped address (more than one colon, no brackets) has no port and
  reaches it unchanged; the new `[addr%scope]:port` form reaches it too. Re-anchor `:428-443` →
  `:478-493`. Rustty's `ip6_parse` keeps a `usize` index.
- **UD113** (enum range checks only in safe builds): R2 finds one more site, the new
  `ghostty_osc_set`'s `c_int` option narrowing. Add it to the entry's reference list.
- **UD32** (already obsolete upstream since Phase 7): `40d5b860d` rewrites `OpenGL.zig` again.
  Nothing to do beyond moving the history anchors.
- **UD131 and UD351** (the private-report crashes): both still live. Their cited lines shift but are
  not touched (UD131: `Screen.zig:747` → `:771`, `PageList.zig:5133` → `:5176`; UD351:
  `stream_handler.zig:1463-1515` → `:1496-1548`). `ref_counted_set.zig`, `style.zig` and
  `src/os/uri.zig` are unchanged, and no commit mentions either. The advisory decision is unchanged.
  `8834e161c` and `88e66cbc6` touch `PageList.zig` (memory usage, compressed prepends) but not
  `increaseCapacity` or the clone retry loops.

### 2.3 Re-anchors only (47 entries)

All other UDs whose reference file changed are unchanged in substance. Their cited lines moved (the
full map, old → new at `a60e9e2a5`, is Appendix A): UD7, UD11, UD15, UD16, UD17, UD18, UD26, UD34,
UD57, UD58, UD59, UD62, UD63, UD64, UD68, UD70, UD71, UD75, UD76, UD80, UD96, UD98, UD104, UD113,
UD115, UD131, UD141, UD172, UD180, UD181, UD205, UD230, UD250, UD251, UD270, UD272, UD273, UD290,
UD310, UD343 (`test.yml` lines only), UD351, UD354, UD355, UD361, UD364. Lines that did not move
(no edit needed): UD8, UD12, UD90, UD95, UD295, UD305, UD321, UD358. UD305 cites
`include/ghostty/vt/allocator.h`, whose only change is the doc of the alignment field
(`82ae67b9f`: "a bit count").

Entries cite two pins today (`at 12752b2ac`, or `at 20abdb50a` for code that did not move in Phase
7). Phase 7's rule (DECISIONS D4) re-anchors only entries whose cited code moved; the same rule here
re-stamps the 47 above at `a60e9e2a5` and leaves the rest as they are.

Matches on a basename that is not the cited file were discarded: UD46/48/252/260/261/275 cite the
libxev or libvaxis `build.zig.zon`, UD246 cites libvaxis's `App.zig`, UD343's `build.zig` is the
root one, not `test/esctest/build.zig`.

UDs that cite functions without line numbers and live in files the range rewrites were read at the
function: UD70, UD352 and UD363 (Metal display link and draw mutex; `40d5b860d` moves device state
out of `Metal.zig`, but the display-link callback and `drawFrameLocked` keep their locking),
UD362 (`140feb86a` adds `display_realized = false` under the draw mutex in `threadExit`; the read
in `drawFrameLocked` is also under it), UD104 and UD115 (`Surface.init` gains `.device =
&app.device`; the error paths are unchanged), UD356 (R3: still valid). None changes class.

### 2.4 LEDGER and other records

- **LEDGER.tsv** (497 rows: 404 CLOSED, 93 WAIVED in the `state` column): 125 rows cite a changed
  file.
  - 16 WAIVED rows: 13 shifted, 3 same, **0 touched**, so no waiver needs re-adjudication. Re-anchor
    the 13 shifted ones: S41-F1, S56-F2, S71-F5, S76-F5, S115-F4, S116-F2, S145-F1, S146-F1,
    S148-F2, S163-F1, S163-F3, S165-F1, S180-F2 (S55-F3, S55-F6, S106-F1 unchanged).
  - 3 CLOSED rows touched: **S117-F1** (shadertoy `Uniforms`, the UD20 hunk), **S113-F2**
    (`Metal.zig:85-88` log sites, moved into `metal/Device.zig` by `40d5b860d`; R3 ports them),
    and **S133-F1**.
  - **S133-F1** ("`Screen.clearCells` does not clear the row's `kitty_virtual_placeholder` flag",
    CLOSED) closed a port gap by porting the pin's behaviour: keep the flag if any erased cell was a
    placeholder. `a573781c6` calls that "backwards" (the cells are blank afterwards) and clears the
    flag on every full-row clear (`Screen.zig:1843`, `page.zig:1284`). The consequence was only
    extra placement scans of rows with no placeholder (no wrong output), so it is not a missed UD.
    R1 ports the new behaviour; the row stays CLOSED with a note, and G-PARITY is unaffected.
- **NEW-FINDINGS open items the delta touches.**
  - **N11** (renderer `error.<Name>` log args): re-scope. `40d5b860d` moves the EGL init sites from
    `OpenGL.zig` to `opengl/Device.zig` (6 `log.` sites, including `err={}`), and `2e36658c6` adds
    `egl.queryClientExtensions` (`pkg/opengl/egl.zig`) and an explanation of why the surfaceless
    display could not be created (`opengl/Device.zig`). Metal device sites move to
    `metal/Device.zig` (2 sites).
  - **N376** (CI gaps): two new reference CI items, `test-esctest` (§4.2) and the Windows
    `c-vt-effects` DLL steps (§4.3). The five open gaps are unchanged, only moved (`test.yml`
    +86 lines).
  - **N438**, **N489**, **N490**: not touched. **N130**, **N35**, **N354**, **N432**, **N286**: not
    touched (no GTK title, search thread, Windows exec, libxev or fontconfig change).
- **VERIFICATION_DEBT.** V25 (CI jobs never run on GitHub) gains the new jobs if they are ported.
  V24 (Nix) and V30/V31/V33 are untouched. Nothing in the range discharges a V.
- **NON_PORTS.**
  - `.port/phase-4/NON_PORTS.md:10` already records `src/main_c.zig`'s `DllMain` as a NON-PORT
    (Zig's `_DllMainCRTStartup` skips the CRT; a rustc `cdylib` links the MSVC CRT's own entry).
    `src/lib/windows_dll.zig` is the same workaround for libghostty-vt's DLL, and its own header says
    "Remove it when Zig runs global constructors for Windows dynamic libraries itself". Extend that
    row, or add one beside it (Q2).
  - Phase-3 `NON_PORTS.md:1079` (Zig-package and Nix renderings) and `:1082` (VOUCHED) cover this
    range's 5 rendering files and 15 VOUCHED commits; add the shas to the rows.
- **COMMENT-NOT-CARRIED.tsv**: rows 40–45 (`lib_vt.zig:506/509/510/511/512/514`, the
  `std.Options.debug_io` block) go stale again. `a141f9bdb` inserts the `DllMain` block above it.
  Re-anchor by +9 lines.

### 2.5 Reference-defect candidates in R4's scope

None. Two **gate** defects found while measuring (§5.3, §5.4) are N candidates (next free N517).

---

## 3. Upstream tests (`R4-upstream-tests.tsv`)

**90 added, 30 changed, 0 removed.**
- The raw `^+\s*test` count of the diff is 91: the YAML key `test-esctest:` plus 90 blocks, one of
  them the identifier form `test splitHostPort {`.
- `test/esctest/src/main.zig` has no `test` block.
- Two tests were added and then renamed inside the range ("xt_checksum: rect origin mode" and "rect
  defaults and clamping" became "selection …" in `5f0d4febc`). Only the end-state names are listed.

**Added, by file:**

| file | tests |
|---|---|
| `osc/parsers/program_status.zig` | 15 |
| `osc/parsers/unknown.zig` | 13 |
| `xt_checksum.zig` | 12 |
| `Terminal.zig` | 8 |
| `stream_terminal.zig` | 8 |
| `c/terminal.zig` | 7 |
| `cli/ssh.zig` | 4 |
| `tmux/control.zig`, `PageList.zig` | 3 each |
| `stream.zig`, `c/osc.zig` | 2 each |
| `osc.zig`, `Parser.zig`, `page.zig`, `Screen.zig`, `snapshot/history.zig`, `c/paste.zig`, `c/snapshot.zig`, `c/sys.zig`, `cli/ssh-cache/DiskCache.zig`, `input/key_encode.zig`, `input/mouse_encode.zig`, `renderer/shadertoy.zig`, `Surface.zig` | 1 each |

**Changed bodies (30):**
- `Screen.zig` 12, all resize tests from `f9ab34f10` (pending-wrap repair adds assertions);
- `snapshot/history.zig` 3 and `PageList.zig` 3 (`88e66cbc6`, the `PageAllocation` API);
- `Terminal.zig` 2 (`6220a3617`, `c706451fe`);
- `c/terminal.zig` 2 ("get mouse_shape", "set unknown_sequence callback");
- 1 each: `stream_terminal.zig`, `search/pagelist.zig`, `search/screen.zig`, `c/types.zig`
  ("manifest uses public enum names": SEMANTIC_PROMPT = 42, RESET = 43), `c/snapshot.zig`,
  `untouched_pool.zig` (`freeCount`), `DiskCache.zig` (`isValidCacheKey`), `ssh_cache.zig`
  (`matchesQuery`).

No gate sees a changed body (G1b counts, G-TESTS matches names), so the TSV is the checklist.

**Name-masked adds.** G-TESTS reports only 73 of the 90 adds (§5.1). The other 17 are not in
Rustty: a grep for each snake-case name finds nothing except `next_slice_matches_per_byte`, which is
an unrelated older test in `osc/mod.rs`. G-TESTS passes them because the first four tokens of the
name co-occur in some existing Rust test name. They are:
- `splitHostPort`, "parseDestination: port before hostname";
- "legacy: ctrl+alt+shift+backspace";
- "PageList PageAllocation compresses prepended pages unless visible", "PageList resize reflow pin
  in blank cells after line break";
- "Screen: selectLine semantic boundary around unwritten cells";
- "Terminal: eraseLine complete clears kitty placeholder flag", "…resize less cols without reflow
  cutting wide char tail", "…resize pending wrap live and saved cursors";
- c/osc "command data null", "unknown";
- c/terminal "set semantic_prompt callback", "set reset callback", "checksum report requires
  explicit opt in";
- "OSC unknown: nextSlice matches per-byte parsing";
- "stream: DECRQCRA dispatch", "stream: XTCHECKSUM dispatch";
- "unknown OSC effect callback", "xt_checksum: encode".

At the finale, tick every row of `R4-upstream-tests.tsv` by exact name, as Phase 7's
`w0/tests-checklist.txt` did.

---

## 4. Build, dependencies and CI

### 4.1 `build.zig.zon` and renderings (`406f2e544`)

Only `iterm2_themes` moved: `ghostty-themes-release-20260921-150923-0b55a9e.tgz` →
`…-20260928-151043-99d9701.tgz` (hash `N-V-__8AAAfDBACe1jGqjr9jIG3UAK8KbzJKQ7xrzYoau-_a`, sha256
`072d88d0f6a3561c01ee7fa11b0eec018c24f26f2fa461a67660277fea4e32c9`). `build.zig.zon.json`, `.nix`,
`.txt` and `flatpak/zig-packages.json` render the same URL: N/A (phase-3 NON_PORTS `:1079`).

**Measured** (tarball fetched and compared with `crates/rustty-extra/themes/`):
- **617 → 657 files: 40 added, 1 changed, 0 removed.** The rename-only file "Ghostty Default Style
  Dark" ↔ "Rustty Default Style Dark" is byte-identical.
- Added: After Hours, Arizona Monsoon, Blueprint, Charcoal, CRT Afterglow, Dark 2026, Deep Current,
  Ember, Emberglass, EVH FrankenTone, Golden Retriever Cream/Honey/Red, High Desert Snow, Hyper,
  Light 2026, Madman's Diary, Nightfall, Ozz's Blizzard, Prince of Darkness, RadicalReborn, Rhoads
  Concord/Insipired/Legacy/LP/RR Black, Sonoran Dusk, Syncopation, and X, X Berlin, X Bogota,
  X Helsinki, X Lahabana, X London, X Madrid, X Miami, X Oslo, X Paris, X Praha, X Tokio.
- Changed: "Dark Modern" (palette 0–9 and on).
- No new file contains "ghostty" in any casing, so no R2 rename is needed.

**Rustty work (size S, about 0.25 d):**
- re-vendor via `xtask/src/res_themes.rs`;
- `python3 .port/tools/gates/check_theme_corpus.py --write` (`themes.manifest`);
- update the hard-coded 617:
  - `xtask/src/res_themes.rs:30, 59, 109-115` (history comment) and `:140` (the assert);
  - `.port/tools/ci.sh:155, 1163`;
  - `.port/tools/gates/check_theme_corpus.py` docstring (`:9`);
  - `crates/rustty-extra/themes/README.md:12, 27, 46, 48, 52, 97`.

G-THEMES compares the corpus with Rustty's own manifest. It stays GREEN after the advance and cannot
see the upstream bump, so this item rests on the record (as in Phase 7).

### 4.2 `test/esctest` and the `test-esctest` job (`438d584f0`, `edcaf6670`)

What upstream added:
- A separate Zig package `test/esctest` (`build.zig`, `build.zig.zon`, `src/main.zig` 262 lines,
  `README.md`). Its dependencies are `ghostty` by path and `esctest2` at commit `2798f121…`
  (ThomasDickey's Python suite, lazy).
- `main.zig` forks a pty, runs esctest with `--expected-terminal=xterm --xterm-checksum=411
  --xterm-reverse-wrap=411`, and feeds every byte through a libghostty-vt `Terminal`. That terminal
  is 80x25, has DECRQCRA enabled with xterm's `checksumExtension` 23, and VT520 device attributes.
  The terminal's replies are written back to the pty.
- A report-only job, `test-esctest` (`test.yml:1536-1595`):
  - `continue-on-error: true`, not in the required checks;
  - `nix develop -c zig build run > esctest.log || true`;
  - a step-summary tally and an uploaded log.
- `edcaf6670` enables DECRQCRA in the runner once `9272a2f7c` landed.

Rustty counterpart: none. The harness needs R1's DECRQCRA/XTCHECKSUM (`9272a2f7c`, `5f0d4febc`,
`00a19db51`) and the `xt_checksum` / `device_attributes` re-exports from R2 (`25b04b410`).

Proposal (Q1):
- Port it as a separate Cargo package at `test/esctest/`, outside the workspace (`exclude`), the
  way upstream keeps it a separate Zig package. It takes a path dependency on `rustty-vt`, and
  `forkpty`/`execvp` through `libc`.
- Fetch esctest2 at the same commit, by sha256, into `target/`.
- Add `ci.yml` job `test-esctest`, report-only (`continue-on-error: true`, not in `required.needs`),
  with the same summary and artifact.
- `test/` is outside G-FILES/G-TESTS (they read `git ls-files src`), so this is tracked by hand.
  The job is new V25-type debt until it runs on GitHub.

### 4.3 Windows DLL (`a141f9bdb`, `a7cf951eb`)

- `a7cf951eb` adds two steps to `build-libghostty-vt-windows` (`test.yml:957-980`): build
  `example/c-vt-effects` against the DLL, copy `ghostty-vt.dll` beside it, and run it under `pwsh`
  so a crash's full NTSTATUS is printed.
- The input `c-vt-effects` writes has no multi-byte UTF-8. By reading, it reaches simdutf through
  the OSC 52 base64 decode; not verified.
- The Rustty counterpart is the `windows` job in `.github/workflows/ci.yml:638-770`:
  - it already tests `rustty-vt-c` (`:723-727`) and builds it release;
  - it runs the apprt-c `test_dll_init.c` smoke (`:745-768`);
  - it never runs a vt example against `rustty_vt.dll`.
  Port the two steps: build `example/c-vt-effects` with the DLL, copy `rustty_vt.dll` beside the
  exe, run it under `pwsh`, and print the exit code as `0x%08X`.
- `a141f9bdb` (`src/lib/windows_dll.zig`, 91 lines; `lib_vt.zig:510-517` `pub const DllMain`): a
  rustc `cdylib` on `*-windows-msvc` and `*-windows-gnu` enters through the CRT's own
  `_DllMainCRTStartup` (`dllcrt2.o` for GNU), which runs C++ initializers. So simdutf-sys's static
  kernels are constructed without help, and the file is a NON-PORT candidate (Q2).
- The ported CI step is the proof. Run it once on rwin (x64 MSVC) before the row is written, with
  an OSC 52 write or a multi-byte `vt_write` added to a local copy of the example if base64 turns
  out not to reach simdutf.

### 4.4 Rustty-only CI that the range affects

- `05392dcc9` deletes `src/renderer/WebGL.zig`. Rustty's `crates/rustty-renderer/src/webgl.rs` (the
  two-line placeholder port) goes with it (R3). `check_leaks.sh:1016` names
  `rustty-renderer:webgl` in a Miri module list; remove the entry in the same commit or G6 reports a
  stale module.
- The opt-in `ci.sh --webgl` lane (`ws14_webgl_browser_gate.sh`) tests `example/wasm-webgl`. That
  is an upstream example, unchanged by the range, so the lane stays.
- `N376`'s five open gaps are unchanged.

---

## 5. Gate impact of the pin advance

"Arrival" means right after `ORACLE_PIN` moves to `a60e9e2a5`, before any port work. All
simulated rows ran GREEN at the pin first.

| gate | at pin (run) | arrival at `a60e9e2a5` (simulated unless noted) | what makes it green |
|---|---|---|---|
| **G-TESTS** (HARD) | GREEN, 4,188 blocks, 0 unexplained | **RED: 4,277 blocks, 73 unexplained** (program_status 15, unknown 12, xt_checksum 11, stream_terminal 7, Terminal 5, c/terminal 4, tmux/control 3, cli/ssh 3, stream 2, 1 each in 11 files) | port the 90 adds; tick the 17 masked ones by hand (§3) |
| **G-FILES** (HARD) | GREEN, 557 files | **RED: 562 files, 6 MISSING**: `lib/windows_dll.zig`, `renderer/metal/Device.zig`, `renderer/opengl/Device.zig`, `terminal/osc/parsers/program_status.zig`, `…/unknown.zig`, `terminal/xt_checksum.zig` | port 5; NON_PORTS row for `windows_dll.zig`. `WebGL.zig`'s deletion is invisible to it (it walks the oracle) |
| **G-SYMS** (HARD) | GREEN, 2,890 | **RED: 2,911, 8 unexplained**: `DiskCache.splitHostPort`, `UntouchedPool.freeCount`, `WasmPagePool.freeCount`, `windows_dll.DllMain`, `PageList.memoryUsage`, `Screen.memoryUsage`, `Terminal.setDefaultXtChecksum`, `Terminal.rectXtChecksum` | 29 `pub fn` added and 4 removed in the diff; the rest are name-matched or in the 6 missing files |
| **G-CENSUS** (HARD) | GREEN (4,397 / 17,717 / 357) | **RED by design**: `total_test_blocks` 4,487 (+90), `total_assertions` 18,258 (+541), `files_with_tests` 360 (+3: `xt_checksum`, `program_status`, `unknown`) | record the three figures plus a history row in `.port/phase-3/CENSUS_DELTA.md` in the advance commit |
| **G1b** (PROGRESS) | GREEN | **RED, 2 rows**: `input_mouse_encode` zig 24 / rust 23 (drop 1), `cli_ssh` 13 / 12 (drop 1). Slack lost elsewhere: osc 316→345 of 355, terminal 428→436, stream_terminal 134→142 of 149, vt_c 451→463, tmux 113→116, pagelist 291→294 | add a MAP row for `terminal/xt_checksum.zig` (12 tests, today in no row: the honest meter, rust 0); the `osc` row globs `osc/parsers/*` and already counts the 28 new parser tests, green by slack only |
| **G3b** (PROGRESS, default scope) | GREEN | **RED: 411 missing + 6 NOT-CARRIED-INVALID**: vt 277, vt-c 62, config 32, cli 21, bench 8, datastruct 4, termio 4, core 3; rows 40–45 of `COMMENT-NOT-CARRIED.tsv` (`lib_vt.zig`, +9 lines) | carry the comments with each port; re-anchor rows 40–45. The 6 new files are not in the phase-0 `crate_map`, so their comments are not counted until the map is extended (Phase 7 T4 precedent): G3b will rise when they are added |
| G3b `--strict` (apprt, bin, renderer) | GREEN at the Phase 7 finale | **RED: 433 missing** (411 + renderer 11, bin 7, apprt 4) + the same 6 NOT-CARRIED-INVALID | as G3b; Appendix B |
| **G-LOGFILE** (HARD) | GREEN (1,392 sites) | **RED, 5 arms** (176 files, 1,406 live sites): (1) 3 undeclared files, 10 sites: `opengl/Device.zig` 6, `metal/Device.zig` 2, `osc/parsers/program_status.zig` 2; (2) 1 dead row off-line, `Termio.zig:717`; (3) 19 `LOG-ABSENT-SITES.tsv` rows off-line, all `generic.zig` (`:1051`, `:1237-1258`, `:1626`, `:3000`, …); (4) 9 files short: `generic.zig` −8, `Config.zig` −3, `stream.zig` −4, `Surface.zig` −2, `stream_handler.zig` −2, and −1 each in `App.zig`, `c/osc.zig`, `stream_terminal.zig`, `Termio.zig`; (5) 1 N20 comment, `crates/rustty-apprt/src/gtk/class/surface/core_callbacks.rs:475`. Stale preserved comments 19, divergences 13, 28 of the cap of 28 | declare the 3 files in `LOG-PORTMAP.tsv`; re-anchor the 20 rows; port the new sites (`App.zig` "failed to initialize render device", `Config.zig` checksum config, the stream/termio checksum and program-status sites) |
| **G-REFGEOM** (HARD) | GREEN | **expected GREEN (by reading)**: `style.zig`, `hyperlink.zig`, `size.zig`, `ref_counted_set.zig` unchanged; `page.zig`'s only change is `clearCells`' placeholder flag (`:1284`) and a test. The simulation itself was BLOCKED by a scratch-copy build artifact (`uucode_generate` FileNotFound in the export), not by the gate | run it after the real advance |
| **G4** (HARD) | GREEN (12 fixtures, 23 c-vt goldens) | **BLOCKED until rebuilt, then RED on leg B** (by reading): the oracle `zig-out/include` and `vt_driver_ref` must be rebuilt at the new pin. `example/c-vt-effects/src/main.c` gains the unknown-OSC case and its output line, so `corpora/cvt/c-vt-effects.golden` must be re-captured (`.port/phase-0/run-cvt-corpora.sh`) and Rustty's example updated. `c-vt-snapshot` gains a silent compressed decode (same output); `c-vt-compression` has no golden. Leg A: no fixture changes size, so the resize fixes do not reach it; `6220a3617` (code points above 0xFF unmapped in a charset) and `c706451fe` (single shift) can diff a fixture that prints non-ASCII under a designated charset | rebuild both, re-capture, port the examples (R2) |
| A3 | GREEN | **GREEN**: `include/ghostty.h` unchanged; its sha256 is still `78232aec…`, so `oracle-ghostty-h.sha256` needs no bump | — |
| A4 | GREEN | GREEN (no embed ABI change) | — |
| **A5 / VT headers** | GREEN at the Phase 7 finale | **already RED today, by reading** (it reads `origin/main`, which is `a60e9e2a5` since the fetch, not the pin; DECISIONS D1, N486). Export arm: new `ghostty_osc_set`. Type-registry arm: 10 new `c/types.zig` entries (`TerminalMemoryUsage`, `TerminalProgramStatus`, `TerminalSemanticPrompt`, `TerminalUnknownOscSequence`, `OscOption`, `OscTerminator`, `ProgramStatusKind`, `ProgramStatusState`, `SemanticPromptKind`, `SemanticPromptPromptKind`). Not run: the gate builds | R2's C-API port and regenerated headers; every new C enum needs the `_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` sentinel. rvm's oracle must also be fetched (N486) |
| **G9** (parity surface) | GREEN 693/693 (frozen surface at `12752b2ac`) | GREEN against the frozen surface; **after regenerating** (DECISIONS R10, simulated in scratch): config 210 → 212 (`vt-xt-checksum-report`, `vt-xt-checksum-extension`), OSC kinds 27 → 29 (`program_status`, `unknown`), OSC numbers +2 and +7501. **And the CSI surface shrinks 24 → 22** (§5.3) | fix the generator first, then regenerate; the new ids close with R1/R2/R3's ports |
| G-EOL (HARD) | GREEN | expected GREEN (by reading): no fuzz corpus, `dist/windows` or harness fixture changed. The themes are G-THEMES's | — |
| G-THEMES (HARD) | GREEN (617) | GREEN (Rustty-internal manifest; blind to upstream) | §4.1 |
| G-I18N (HARD) | GREEN | GREEN: no `po/` change | — |
| G-UDDOCS (HARD) | GREEN | GREEN until UD20/UD312's statuses change; then remove their two docs rows in the same commit | §2.1 |
| G-PARITY (HARD) | GREEN | GREEN: no WAIVED row touched; S133-F1 stays CLOSED with a note | §2.4 |
| G7 | GREEN | GREEN (no new crate edge; a `test/esctest` package outside the workspace adds none) | — |
| G-BENCH, G-LZ4 | GREEN | GREEN (by reading): `benchmark/cli.zig` (G-BENCH's action list) and `terminal/compress/*` unchanged. `OscParser.zig`/`TerminalStream.zig` gain `--unknown-max-bytes` / `--osc-unknown-max-bytes` options (R2 ports them; G-BENCH does not check options) | — |
| G1, G2, D3, G-VTFEAT | GREEN | unaffected (frozen contracts; `terminal/build_options.zig` unchanged) | — |

### 5.1 G-TESTS blind spot (N candidate)

`audit_test_coverage.py` treats a test as present when the first four normalized tokens of its name
co-occur in any Rust test name (`analyze`, `need = parts[:4]`). Seventeen of the 90 adds (19%) pass
that way with no counterpart (§3). Phase 7 measured the same thing and used a checklist. Recommend
recording it as an N, with the checklist as the remedy. A stricter matcher would need its own
self-test arm and would risk the false gaps the file's comments describe.

### 5.2 G3b default scope does not see new files

The phase-0 `crate_map` lists files. The six new files have no row, so their comments (e.g.
`xt_checksum.zig` 422 lines, `program_status.zig` 628, `unknown.zig` 297) are invisible to G3b
until a row is added. Phase 7 T4 added rows with `test_blocks` 0 so that G1's frozen 3,234 does
not move; do the same at phase start.

### 5.3 The parity-surface generator reads stale line windows (N candidate)

- `.port/phase-0/inventory/parse_csi.py:15-16` hard-codes `START = 807`, `END = 1943`, and
  `parse_esc.py:13-14` hard-codes `2105`–`2376`. Those are phase-0 line numbers of `stream.zig`.
- At the **current** pin, `csiDispatch` runs from `:1410` to the `else` at `:2586`. So the frozen
  surface already **misses 13 CSI finals**: `e g h l m n p q r s t u @`.
- The "ESC handlers" it records at `:2125`, `:2180`, `:2220`, `:2288`, `:2330` are CSI arms `n`,
  `p`, `q`, `r`, `s`. `escDispatch` is at `:2762`.
- Regenerated at `a60e9e2a5` (simulated in scratch), the window loses `c` (DA1, now `:1945`) and
  `d` (VPA, `:1969`). The new `y` arm (DECRQCRA/XTCHECKSUM, `:2597`) is outside it too.
- So G9 has under-covered CSI and mis-covered ESC since some earlier advance, and a plain
  regeneration narrows it further without any signal.
- Fix: find the switch by its function header (`fn csiDispatch`, `fn escDispatch`) and match
  braces, with a self-test that fails on a known final (e.g. `'m'`) missing. Then regenerate. Expect
  G9 to go RED on any of the 13 + 1 finals Rustty's G9 mapping lacks. Next free id N517.

### 5.4 A5 tracks `origin/main`

A5 reads `origin/main` rather than the pin, so it went RED with the fetch, before any advance
(Phase 7 Q10 / DECISIONS D1 recorded the same design). Nothing new; record its RED as pre-existing
in the "before" set, as Phase 7 did.

### 5.5 G3b `--strict`

The simulated `--strict` run (apprt, bin, renderer included) at `a60e9e2a5` adds 22 to the
default scope's 411: renderer 11 (from `40d5b860d` and `140feb86a`), bin 7 and apprt 4. See
Appendix B.

---

## 6. Proposed workstreams (R4 scope)

| WS | content | depends on | size |
|---|---|---|---|
| **R4-A advance** | W0 T3 equivalent (§7 steps 1–7): pin, CENSUS_DELTA 4,487 / 18,258 / 360, oracle `zig-out/include` + `vt_driver_ref` rebuild, c-vt-effects golden re-capture, generator fix then surface regen, before/after evidence in `.port/phase-8/w0/` | none; first | S–M (0.5 d) |
| **R4-B records at phase start** | W0 T4 equivalent: UD20/UD312 statuses + docs rows; 47 + UD6/UD60/UD160 re-anchors; UD113 site; LEDGER 13 WAIVED re-anchors and S133-F1/S117-F1/S113-F2 notes; COMMENT-NOT-CARRIED rows 40–45; NON_PORTS (windows_dll, renderings, VOUCHED shas); G1b MAP row `xt_checksum`; G3b `crate_map` rows for the 6 new files; `LOG-PORTMAP.tsv` rows for the 3 new log files; N for §5.1 and §5.3 | R4-A | S (0.5 d) |
| **R4-C themes** | re-vendor 657, manifest, the four hard-coded 617s | none | S (0.25 d) |
| **R4-D Windows DLL CI** | `ci.yml` `windows` job: c-vt-effects against `rustty_vt.dll`; rwin proof; NON_PORTS row | R2's c-vt-effects example update | S (0.25 d + rwin) |
| **R4-E esctest** | `test/esctest` Rust package + report-only `test-esctest` job; first local run recorded (pass tally) | R1-D (DECRQCRA/XTCHECKSUM, `00a19db51` config), R2 exports | M (1–1.5 d) |
| **R4-F gate repair** | parity-surface generator (function-scoped windows + self-test); `check_leaks.sh:1016` webgl entry with R3's deletion | R4-A for the regen | S (0.25 d) |
| **finale** | §7 step 8 and the validation lanes | everything | see §7 |

---

## 7. Advance procedure and finale/validation checklist

Derived from Phase 7's plan (`docs/superpowers/plans/2026-09-28-phase7-upstream-resync.md`, W0 T3
`:151-202`, W0 T4 `:204-231`, W12 T1/T1b/T2 `:843-885`) and `.port/ORACLE_SYNC.md` §"Advancing the
pin" (`:134-174`), plus the lanes added since Phase 7's finale.

**Advance (R4-A):**
1. **Pre-flight.**
   - `git worktree list`: only the main checkout and the phase worktree.
   - `git -C ../ghostty status --porcelain` empty; HEAD `12752b2ac1bb05ce53402ed8c853ed1f96eef0b1`.
   - `zig version` 0.16.0.
   - `df -h /Users/gael`: 58 GiB free at this review; stop below 12 GiB.
2. **Before**, at `12752b2ac`, into `.port/phase-8/w0/*-before-at-12752b2ac.txt`:
   - G1b, G3b and `--strict`, G-TESTS / G-FILES / G-SYMS (`--limit 500`), G-CENSUS, G-LOGFILE,
     G-REFGEOM, A3;
   - A5 (record its RED as pre-existing, §5.4);
   - G9 against the frozen surface.
3. Range artifacts: already in `.port/phase-8/` (`UPSTREAM_*`), plus `R4-*.tsv` as the assignment
   checklist.
4. **Advance.**
   - `git -C ../ghostty checkout --detach a60e9e2a57f73e1eef2bd1cf2995a467f69e7fb0`.
   - `zig build -Demit-lib-vt=true` (rebuild `zig-out/include`).
   - `.port/phase-0/harness/build_ref_driver.sh` (per checkout; Phase 7 F6).
   - Re-capture the c-vt goldens (`.port/phase-0/run-cvt-corpora.sh`).
   - `bash .port/tools/gates/_oracle_root.sh --self-test`.
   - On rvm, fetch and detach its oracle too (N486).
5. **One commit** naming the absorbed range (107 commits, 69 non-merge):
   - `ORACLE_PIN`;
   - `.port/ORACLE_SYNC.md` (pinned block, history row, §"Upstream beyond this pin");
   - `.port/PORT_RULES.md` R12;
   - `.port/cycle-analysis.md`;
   - `.port/phase-3/CENSUS_DELTA.md` to 4,487 / 18,258 / 360 with a history row;
   - **not** `oracle-ghostty-h.sha256` (unchanged).
   - `parity-surface.json` is regenerated **after** R4-F's generator fix, not with the stale
     windows (§5.3).
6. **After**: the same set, saved as `*-after-at-a60e9e2a5.txt`. Expected arrival (§5):
   - G-TESTS 73;
   - G-FILES 6;
   - G-SYMS 8;
   - G1b 2 rows;
   - G3b 411 (`--strict` 433) + 6 NOT-CARRIED-INVALID;
   - G-LOGFILE 5 arms;
   - G-CENSUS GREEN with the new delta;
   - A3 GREEN;
   - G-REFGEOM GREEN.
   Any other RED is a finding.
7. **Phase-start records (R4-B).**

**Finale (W12 T1 equivalent), at `a60e9e2a5`:**
8. Merge every workstream; remove each worktree, its `worktree-agent` branch and its merged branch
   (memory `cleanup-as-you-go`).
9. Tick every row of `R4-upstream-tests.tsv` by exact name (90 present, 30 changed bodies
   re-aligned); save as `.port/phase-8/w0/tests-checklist.txt`.
10. Gates GREEN:
    - G-TESTS (0 unexplained), G-FILES, G-SYMS;
    - G1b (extended map), G3b and `--strict`, G-LOGFILE (and their self-tests);
    - G-CENSUS, A3, A4, A5 (and the masking self-test);
    - G4 (drivers rebuilt, goldens re-captured);
    - G-REFGEOM, G-EOL, G-I18N, G-THEMES (657), G-UDDOCS, G-PARITY, G7;
    - G9 (regenerated surface), G-P2, G-ORACLE-SH-SELF, G-SKIP;
    - `postmerge_gates.sh`.
    Phase 7's finale ran 35 gate invocations; reuse its summary script
    (`w0/gates-finale-summary-at-12752b2ac.txt`'s format).
11. Targeted cross-host checks (Phase 7's finale table): rvm GTK-feature clippy and `cargo test -p`
    for every touched crate plus the live GTK KATs; rwin as in §4.3; macOS `xcodebuild` RusttyTests
    and the menu-shortcut parity harness (Swift changed in 5 commits).

**Cross-platform build check (W12 T1b):**
12. From fresh clones:
    - rvm: `cargo build --workspace --all-targets --features rustty/gtk,rustty/x11,rustty/wayland`,
      clippy, a release `rustty`;
    - rwin: MSVC workspace build (excluding libxml2-sys, fontconfig-sys, sentry-sys), clippy,
      `x86_64-pc-windows-gnu` `rustty-font-shaper`, `rustty-vt-c --release`;
    - Mac: workspace build and `xcodebuild`.
    Delete the VM clones by literal path afterwards (memory `vm-disk-images-eat-mac-disk`: fstrim
    after each heavy VM lane; one VM lane at a time).

**Validation lanes (W12 T2 equivalent, plus everything added since):**

| lane | command | host | approx. runtime (last recorded) |
|---|---|---|---|
| full `ci.sh` | `bash .port/tools/ci.sh` | Mac, then rvm | hours; never recorded end to end ("Not verified here"); dominated by G6/G6c, G4, G5 and G8 (`ci.sh:321-322`) |
| G6 Miri SB (nextest) | `check_leaks.sh --all-done` | Mac | 13,354 s (~3.7 h; `rustty-vt` 9,525 s) |
| G6c (debug assertions off) | `check_leaks.sh --debug-assertions-off` | Mac | 1,110 s |
| G6t Tree Borrows | `check_leaks.sh --tree-borrows` | Mac | about 1–2 h (scope `G6T_SCOPE`; P7-W15 recorded completion, not wall time) |
| G-ASAN | `sanitizers.sh --gate asan` | Mac (and rvm) | ~15 min (Mac) |
| G-LSAN | `sanitizers.sh --gate lsan` | Mac and rvm | 33 min (Mac), 20 min (rvm) |
| G-MSAN | `sanitizers.sh --gate msan` | rvm | 50 min |
| G-HWASAN | `sanitizers.sh --gate hwasan` | rvm | 56–77 min |
| G-TSAN | `sanitizers.sh --gate tsan` | Mac and rvm | 38 min (Mac), 21 min (rvm) |
| G-LSAN-GTK | `sanitizers.sh --gate lsan-gtk` (headless sway) | rvm | minutes (one live surface; not recorded) |
| G-SAN-SELF | `sanitizers.sh --self-test` | Mac and rvm | minutes |
| G-VALGRIND | `check_valgrind.sh` (4 shards, `--jobs 4`) | rvm | 13,451 s (3 h 44 min) |
| perf fence G8b | `.port/tools/perf/ws10_suite.py` against a **zigbench rebuilt at `a60e9e2a5` with `-Doptimize=ReleaseFast`** | Mac, idle (VM stopped) | ~1–2 h |
| G8 | `check_baseline_regression.py --strict` | Mac | minutes |
| BASELINE.json re-base | re-measure the oracle at the new pin (V17 recipe; the size axis per V16/P7-W22) | Mac, idle | with G8b |
| Linux full lane | `cargo test --workspace`, G-GTKFEAT, G-REL, G-EXAMPLES | rvm | ~1–2 h |
| Windows | V26 command; plus §4.3's DLL step | rwin | ~1 h |
| esctest (new) | `test/esctest` run, report-only | Mac or rvm | ~1 min (upstream's README) |
| re-audit | Task 41 step 3 over the most-changed areas (`src/terminal`, `src/terminal/c`, `src/renderer`) | Mac | — |

Perf note: `osc-parser` and `terminal-stream` are in the 14-config suite (`ws10_suite.py:103-133`).
The range changes both hot paths: the OSC parser gains the unknown tier and CAN/SUB cancel, and the
stream gains the DECRQCRA/XTCHECKSUM dispatch, program status and unknown-OSC effects. Hold the
14/14 wins against a zigbench at the new pin (memory `comptime-flattening-perf-hazard`: `const`
tables, never `OnceLock`/`HashMap`; loop-sweep ns/op).

Records at the finale:
- `.port/phase-8/README.md` finale table;
- `ORACLE_SYNC.md` finale note;
- `.port/parity-closure/README.md` id register;
- UD/N/V entries opened or closed, starting from UD378, N517, V34.

---

## 8. Open questions (with recommended answers under the rule)

1. **esctest.** Port `test/esctest` and the report-only job? **Yes.** It is reference CI and a
   reference test program, and porting it is the parity end state. Use a separate Cargo package
   outside the workspace, like upstream's separate Zig package. It stays report-only, and its pass
   tally is recorded, not gated. Size M, after R1-D.
2. **`windows_dll.zig`.** Port, or NON-PORT? **NON-PORT**, extending the phase-4 `DllMain` row. It
   is a Zig toolchain workaround (its own header says to remove it once Zig runs DLL constructors),
   and the rustc CRT entry already runs C++ initializers. Port the CI step as proof, and run it on
   rwin before writing the row.
3. **UD20/UD312 entries.** Keep both entries with `fixed upstream in <sha>` and drop their docs rows
   (UD150 precedent)? **Yes.**
4. **UD re-anchoring.** Re-stamp only the 47 moved entries at `a60e9e2a5` (D4 rule), leaving the
   rest at their current pins? **Yes.** A blanket re-stamp would hide which entries were re-read.
5. **S133-F1.** Reopen the LEDGER row? **No.** Port `a573781c6` under R1 and add a note to the CLOSED
   row. The old behaviour was not a defect that broke output, so it is not a retro-UD.
6. **Parity-surface generator.** Fix before regenerating `parity-surface.json` at the advance?
   **Yes, fix first.** A regeneration with the stale windows silently drops DA1/VPA from G9. The 13
   already-missing CSI finals then become G9 work, which may turn it RED for mappings Rustty never
   had to declare.
7. **G-TESTS masking.** Change the matcher, or keep the checklist? **Keep the checklist** (Phase 7
   practice) and record the 19% masking rate as an N. A stricter matcher needs a self-test arm per
   false-gap class the file already documents.
8. **New CI debt.** Add `test-esctest` and the Windows DLL steps to V25, or open a new V? **Extend
   V25** (same cause: never run on GitHub until a push).

---

## Appendix A — UD reference lines at `a60e9e2a5` (old → new; "TOUCHED" = inside a changed hunk)

- UD6 (at 12752b2ac): stream_handler.zig:411→426, control.zig:71-74 TOUCHED (now :137-142)
- UD7 (at 20abdb50a): control.zig:238→240, :269→271, :309→311, :347→349, :377→379, :408→410, :413→415, :470→472
- UD11: Screen.zig:828→852, :1265→1289
- UD15: PageList.zig:18987-19033→19250-19296, :19024-19026→19287-19289, :19029→19292, :5726-5741→5769-5784, :5733→5776, :5737→5780
- UD16: renderer.zig:52→55, osc.zig:246→297
- UD17 (at 20abdb50a): snapshot.zig:1637-1697→1657-1717, :1676→1696
- UD18: Config.zig:5135-5142→5188-5195, :5148-5186→5201-5239, :5158-5165→5211-5218, :10641→10703, :10658→10720, :10677→10739, :10751→10813, :10795→10857
- UD20: shadertoy.zig:40-41 TOUCHED (fixed), generic.zig:2363-2381→2371-2389, :1975→1983, Config.zig:3178-3180→3231-3233
- UD26: Surface.zig:673-674→674-675
- UD32: OpenGL.zig:298-331 TOUCHED, :307-309→286-288, :316-327→295-306, :330→309
- UD34: PageList.zig:6642-6649→6685-6692
- UD57: generic.zig:2354-2381→2362-2389 (:2356-2358→2364-2366, :2366-2368→2374-2376, :2376-2378→2384-2386), Config.zig:5718-5721→5771-5774, :4996→5049, :5023→5076
- UD58: Surface.zig:5407-5421→5422-5436, :5418→5433
- UD59: Surface.zig:1882-1912→1883-1913, :709-710→710-711, Config.zig:4869-4870→4922-4923
- UD60 (at 20abdb50a): mouse_encode.zig:134-149 TOUCHED (now :134-147), :144→142, :147→145
- UD62: Surface.zig:5356-5358→5371-5373, :3610→3611, :3619→3620, :3633→3634
- UD63: c/terminal.zig:1341-1349→1504-1512
- UD64: PageList.zig:7566-7578→7654-7666, :4383-4396→4389-4402, :4579→4622, snapshot/history.zig:215-219→228-232
- UD68: Ghostty.App.swift:943→942, :979→978, :1007→1006
- UD70: generic.zig:918-922→926-930, Metal.zig:164-170→148-154, :146→129, :172-176→156-160
- UD71: Surface.zig:2617→2618, :2473→2474, :2581→2582, :3752→3753
- UD75: Surface.zig:804-808→805-809
- UD76: Surface.zig:2195→2196, :2208→2209, :3752→3753, generic.zig:2439→2447, :2444→2452, :2395→2403
- UD80: stream_terminal.zig:393→597, Terminal.zig:650→685, :1128→1163, :1741→1793, :3776→3828, PageList.zig:2064→2067, :2083→2086, :2455→2458
- UD96: Config.zig:9502-9510→9564-9572, :9512-9535→9574-9597
- UD98: Config.zig:6456-6460→6509-6513
- UD104: Surface.zig:586→587, :590→591, :725-730→726-731, :733-737→734-738, :748-795→749-796, :754→755
- UD113: c/terminal.zig:1485-1496→1657-1668 (+ the new `osc_set` site, R2)
- UD115: Surface.zig:837→838, App.zig:132-143→145-160, :141→154
- UD131: Screen.zig:747→771, :1201→1225, :1142→1166, :958→982, :1380→1404, :2433→2458; PageList.zig:5133→5176, :5219→5262, :5385→5428; Terminal.zig:3235→3287, :3067→3119, :2748→2800
- UD141: osc/parsers/color.zig:177→178, :178→179
- UD160 (at 20abdb50a): DiskCache.zig:428-443→478-493 (`parseIp6` at :488)
- UD172: gtk/class/surface.zig:3982-4069→3999-4086, :4169-4208→4186-4225, :4210-4246→4227-4263
- UD180: Config.zig:7564-7595→7617-7648
- UD181: Config.zig:8167-8176→8220-8229, :8214-8225→8267-8278
- UD205: stream_handler.zig:1557→1590, :1558-1778→1591-1811, :1569-1574→1602-1607, :1772-1777→1805-1810
- UD230: stream_handler.zig:1494-1538→1527-1571
- UD250: OpenGL.zig:312→291
- UD251: generic.zig:1777-1781→1785-1789, :2191-2197→2199-2205, :2203-2204→2211-2212
- UD270: tmux/control.zig:213-243→215-245, :522→524
- UD272: Terminal.zig:1855-1857→1907-1909
- UD273: Screen.zig:1995-1998→2013-2016
- UD290: Termio.zig:347-404→367-424 (:386→406, :395→415, :401→421)
- UD310: osc/parsers/color.zig:283-291→284-292
- UD343: test.yml:959-970→984-995, :1711-1724→1797-1810
- UD351: stream_handler.zig:1463-1515→1496-1548, :338→348, stream.zig:2647-2649→2729-2731
- UD354: OpenGL.zig:286-292→265-271, :214-215→193-194
- UD355: pkg/opengl/egl.zig:218-229→237-248, :267-271→286-290
- UD361: page.zig:1970-1996→1964-1990, :1989→1983
- UD364: untouched_pool.zig:119→125

Unstamped entries default to `at 12752b2ac`. The map is mechanical (hunk arithmetic). Confirm each
new line by content when editing; a cited line that only lies near a hunk can still have changed in
meaning.

## Appendix B — G3b `--strict` at `a60e9e2a5` (simulated)

RED, **433 missing + 6 NOT-CARRIED-INVALID** (734 s). That is the default scope's 411 plus the
three crates the default skips:
- **renderer 11**, all from `40d5b860d` and `140feb86a`. Examples: "the app-scoped render device
  from which surface renderers are created", "create a MTLCommandQueue for our app-scoped
  MTLDevice", "bind to OpenGL API explicitly before we create the context", "destroying a surface
  (macOS never does), so mark it unrealized";
- **bin 7**;
- **apprt 4**.
The comments of the two new `Device.zig` files are counted only where they moved out of
`Metal.zig`/`OpenGL.zig`. The rest are unmapped until the `crate_map` row exists (§5.2).
