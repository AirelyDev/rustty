# Phase 7 delta review — R5: records, gates and coverage (cross-cutting)

Range: pin `20abdb50a` (2026-08-31, `.port/tools/gates/ORACLE_PIN`) → `origin/main` `12752b2ac`
(2026-09-28). 333 commits, 234 non-merge, 233 files, +35,723 / −9,235. Read-only review: nothing in
`../ghostty` was checked out and no Rustty code or record was changed. Where a gate had to be
measured "after" the advance, it was run against an **export of `origin/main`** (`git archive
origin/main` into `/tmp/r5/oracle_new`, no checkout). The gate module was imported and only its
oracle-path constant was redirected; everything else ran unchanged. G-LOGFILE also needed a fake
`.git/HEAD` in that scratch export. Every "after" figure below is such a simulation unless it is
marked "by reading".

Companion files in this directory:
- `R5-commits.tsv`: all 234 non-merge commits. Columns: sha, date, subject, files, area,
  cited_by (which R1–R4 reports cite the sha), flag.
- `R5-upstream-tests.tsv`: 200 test adds, changes and removals. Columns: file, test_name, change,
  sha.

---

## 1. UD reconciliation (178 entries in `.port/parity-closure/UPSTREAM-DEFECTS.md`)

**Method.**
- 85 UDs cite a reference file that the range changes. Each was checked by six parallel readers
  (pin vs `origin/main` at the cited lines, plus the commits that touch them), and the Rustty
  PORT NOTE site was read.
- The other 93 cite only unchanged files, or name no file. For those, commit subjects, callers
  and dependency bumps were checked.
- `build.zig.zon` bumps only `uucode` and the iTerm2 themes, and moves `translate_c` in-tree.
  libxev, libvaxis, zf, highway, gobject and the Zig version (0.16.0) are unchanged. So every
  dependency UD is untouched: UD46–54, UD55, UD72, UD74, UD107, UD108, UD160, UD182, UD201,
  UD230, UD245, UD246, UD249, UD252, UD260–267, UD275, UD311, UD330, UD341, UD346, UD351 (std
  part).

### 1.1 Fixed, partly fixed, or made obsolete upstream (7 of 178)

| UD | class | upstream sha | what upstream did vs Rustty | Rustty action |
|---|---|---|---|---|
| **UD150** | FIXED-SAME | `5ca09acf6` | `GitVersion.zig:34` trims `std.ascii.whitespace` before the sanitize loop, and the branch is used untrimmed afterwards (`:87`). Rustty trims only `"\r\n "` from the end (`crates/rustty-version/src/git_version.rs:65-66`, `:98-99`). The difference can't be reached from real git output: ref names have no control characters, and `rev-parse` emits no leading whitespace. | Set status to "fixed upstream in 5ca09acf6". Re-port as a faithful copy of `5ca09acf6`, optionally with the same whitespace set. The R13 PORT NOTE at `git_version.rs:88-97` and the comment at `:145-146` become obsolete. |
| **UD200** | FIXED-DIFFERENT | `3b8141fbd` | `open.zig:93` now uses `takeDelimiterInclusive('\n')` and logs `trimEnd(line,"\n")` (`:101`). The spin and the missing reap are fixed, and newline-terminated lines (including the 255- and 256-byte boundaries) match Rustty. **The difference:** at end of stream, a last stderr line with no trailing `\n`, or a tail that exactly fills the 256-byte buffer, is not logged upstream (`EndOfStream` → `break`). Rustty logs it (`crates/rustty-os/src/open.rs:279-284`). Measured with zig 0.16.0 on upstream's loop body. | Either (a) re-align under R1: drop the pending tail at EOF, fix the test at `open.rs:330-347`, rewrite the PORT NOTE at `open.rs:254`; or (b) record upstream's EOF loss as a new UD, since it breaks the loop's evident intent, and keep Rustty's handling. **Maintainer decision** (Q1). |
| **UD141** | PARTIAL, fixed part differs | `73768913b`, `d92e3400e`, `500040d3b`, `3822aa01d` | The new `src/lib/parse_int.zig` rejects `_` everywhere, and for **unsigned** types also rejects a leading `+`/`-` (`:23-25`). It is applied at OSC 4/104 (`osc/parsers/color.zig:178,251`), OSC 66 (`kitty_text_sizing.zig:50`), OSC 133 (`semantic_prompt.zig:171`) and OSC 9 (`osc9.zig:29,197`). It is also applied at two sites UD141 never listed: RGB hex (`color.zig:720`) and the OSC 21 key (`kitty/color.zig:52`). Rustty's OSC 9 and OSC 133 handling matches. Rustty's OSC 4/104 (u9) and OSC 66 (u4) still **accept** `+10`/`-0`, which upstream now rejects. Still defective upstream: the glyph APC (`apc/glyph/request.zig:106,217,220-222,372`) and kitty graphics (`graphics_command.zig:321-322`), both still `std.fmt.parseInt`. | Re-align (R1): reject signs in `crates/rustty-vt/src/osc/parsers/color.rs:579` and `kitty_text_sizing.rs:112`, and add the `_` rejection at `color.rs:1033` and `kitty/color.rs:169`. Rewrite the `protocol_int.rs` PORT NOTE: it cites the removed `kitty_desktop_notification.zig:210-211` filter and says "signs kept". Keep the R13 fix for the glyph APC and kitty graphics. Status: "partly fixed upstream in 73768913b". |
| **UD140** | PARTIAL, fixed part same | `4b86e3594`, `42049f1c4` | Kitty zlib now goes through wuffs. `pkg/wuffs/src/zlib.zig:65` returns `Overflow` only when more output than `limit` is needed, which matches Rustty; upstream's test "accepts exact limit" is at `:91`. The file half is still defective: `graphics_image.zig:461` keeps `.limited(max_size)`. The same commit also brings unrelated behaviour to port: OOM becomes `OutOfMemory` instead of `DecompressionFailed`, trailing bytes after the stream are rejected (`zlib.zig:57`), and the log text is new. | Status: zlib half fixed in 4b86e3594. Move the `reference:` to `:461`. Delete the "differs from the reference" PORT NOTE at `crates/rustty-vt/src/kitty/graphics_image.rs:1221`. The wuffs behaviour is R1 port work. |
| **UD31** | PARTIAL, fixed part differs | `a1bf5b5e1` | `ghostty_surface_split_focus` now logs "error focusing split err={}". The same commit routes `split`, `split_focus` and `request_close` through `performBindingAction`, with a new log "error requesting close err={}". Rustty is faithful to the old copy-pasted text (`crates/rustty-apprt-c/src/lib.rs:2201-2211`). The GTK half is unchanged ("set_tab_title to app is unexpected" in `setWindowTitle`, now `application.zig:3026`). | Re-align with the R3 port of `a1bf5b5e1`: new message, new routing, new close log. Keep the GTK half faithful. Status: partly fixed. |
| **UD40** | PARTIAL | `adf161473` | `draw.pixelStore` tags corrected to `.comptime_int, .int` (`pkg/opengl/draw.zig:131-137`). `Buffer.Binding.attribute` is unchanged (`Buffer.zig:142-179`: `.Array` at `:159`, the enable is still missing at `:178`, still no caller). | Update the `reference:` to Buffer.zig only. Rewrite the PORT NOTEs at `crates/rustty-renderer/src/opengl/gl/draw.rs:276-282` (it cites `draw.zig:80-86`) and `buffer.rs:18-20`. |
| **UD32** | OBSOLETED | `2f0b65346`, `adf161473` | `OpenGL.present` was rewritten (`OpenGL.zig:298-322`). It exports a DMABUF, or falls back to a CPU copy. The default-framebuffer blit and the deferred sRGB re-enable are gone. The remaining blit is in `opengl/Target.zig:147-213` through the new `gl.blitFramebuffer` (`draw.zig:104-128`, which calls `try errors.getError()`), and a failure there logs "failed to export DMABUF, falling back to CPU presentation". | Status: obsolete upstream. The whole of Rustty's `present` (`crates/rustty-renderer/src/opengl/opengl_impl.rs:476-485, 577-588`) must be re-ported by R2 from the EGL/DMABUF design. |

### 1.2 UD131 and UD351 (the private-report crashes)

- **UD131 is still live at `12752b2ac`, unfixed.**
  - Both `@panic("increaseCapacity OutOfSpace")` sites are verbatim: `Screen.zig:747` (in
    `clonePartialRowGrowCapacity` `:710`) and `PageList.zig:5133` (in `cloneRowGrowCapacity`
    `:5101-5140`, identical text to pin `:5076-5115`). `increaseCapacity` (`:4213`, OutOfSpace at
    `:4257`/`:4267`) is diff-identical.
  - The PSL arm is intact (`ref_counted_set.zig:292`, `:358`), and `style.zig` is untouched, so
    the unseeded hash recipe still works.
  - The page and PageList rewrites (`e01e75bbb`, `efa3c66ed`, `c0a4f80d8`, `6112935a2`,
    `d2ff6d77a`) touch init-from-zeroed, pooling, integrity and layout only.
  - One caveat: `d2ff6d77a` rewrote `Capacity.adjust` (cache-line row trimming, `page.zig:1970`).
    That may move the 591-LF row count in the 80×24 recipe by about one, and the wide-terminal
    threshold. Re-verify the recipe at the new pin before any advisory.
- **UD351 is still live at `12752b2ac`, unfixed.**
  - `reportPwd` is byte-identical, now at `stream_handler.zig:1463`: `host_buffer:
    [HostName.max_len]u8` at `:1509`, `uri.getHost(&host_buffer)` at `:1510`.
  - `src/os/uri.zig`, the OSC 7 parser and the `stream.zig` `report_pwd` dispatch are unchanged,
    and Zig is still 0.16.0, so std's `getHost` `unreachable` is unchanged.
- Neither is mentioned in any upstream commit. Both still need the private advisory.
  `docs/ghostty-bug-fixes.md` keeps withholding the triggers.

### 1.3 Unchanged, apart from the `reference:` line (171 entries)

All the remaining UDs are unchanged in substance. Those in touched files mostly need their pin
line numbers re-anchored at the next pin. The code moved in these:
- UD8: `tmux/viewer.zig:1166-1167`, `page.zig:280`, `PageList.zig:671`.
- UD19: `Exec.zig:1712-1727`.
- UD28: `hash_map.zig:906-917`; `keys` is now a field, via `6112935a2`.
- UD64: trap at `PageList.zig:4579`. **Re-derive the 2^32 grapheme-budget constants and the
  47,932 / 46,908 / 48,047 thresholds after `d2ff6d77a`.**
- UD65: `ref_counted_set.zig:389-403`.
- UD80: `ref_counted_set.zig:421/402`; re-measure after `d2ff6d77a`.
- UD93: `Exec.zig:1172-1211`.
- UD101: `Exec.zig:1037-1047`.
- UD250: new callers at `opengl/Target.zig:162/165/225`.
- UD251: `Pipeline.zig:72-74`. Reach **widened**: `2f0b65346` adds an unrealize/realize shader
  rebuild (`generic.zig:1210-1211`, `:964-965`) that leaks each pipeline's FBO and VAO.
- UD290: `Termio.zig:347-404`.
- UD315: `render.zig:511`. The new `overscan` field is stored mid-update (`:845`); Rustty's
  forced redraw after a failed update must also reset overscan.
- UD325/UD326: `Exec.zig:1775`, `:1784-1821`.
- UD33: `bitmap_allocator.zig:224`.

The per-UD new `origin/main` anchors are in the six reader reports, summarised in Appendix A.

UD9, UD11 and UD12 are unchanged, but `1ca2858d0` removes `mouse_any_flag` from the tmux
`list-panes` format. Rustty's fixtures `pane_state_for_a_pane_with_captures_queued_waits_for_its_own_reply`
and `pane_state_scroll_region_outside_the_pane_is_ignored` must be rebuilt for the shorter reply.

### 1.4 PORT NOTEs / R13 deviations made obsolete or needing new text

| Rustty site | why |
|---|---|
| `crates/rustty-version/src/git_version.rs:88-97, :145-146` | UD150 fixed upstream (`5ca09acf6`) |
| `crates/rustty-os/src/open.rs:254` (+ test `:330-347`) | UD200 fixed upstream, differently (`3b8141fbd`) |
| `crates/rustty-vt/src/.../protocol_int.rs` PORT NOTE | UD141: cites the removed `kitty_desktop_notification.zig:210-211` filter |
| `crates/rustty-vt/src/kitty/graphics_image.rs:1221` | UD140 zlib half fixed (`4b86e3594`) |
| `crates/rustty-renderer/src/opengl/gl/draw.rs:276-282`, `buffer.rs:18-20` | UD40 `pixelStore` fixed (`adf161473`) |
| `crates/rustty-renderer/src/opengl/opengl_impl.rs:476-485, 577-588` | UD32 code deleted upstream (`2f0b65346`) |
| `crates/rustty-core/src/surface.rs:3082-3085` (and `threadEnter` in `deinit` `:3235-3237`) | UD104: `2f0b65346` removed `renderer.threadEnter` from `deinit` and removed `finalizeSurfaceInit` |
| `crates/rustty-apprt-c/src/lib.rs:2201-2211` | UD31 embedded half fixed (`a1bf5b5e1`) |
| `crates/rustty-vt/src/tmux/viewer.rs:1356-1360` | `a41248013`: the errdefer now captures `actions_len` (`viewer.zig:860-861`), so it is no longer a no-op |

`a41248013` is not a UD. It fixes a use-after-free (the `.windows` action held a slice of a
temporary list). **Rustty does not have it**: the action owns a clone at `viewer.rs:1458`. Rustty
must still re-align three things:
- the new `Action.format` text `[N windows]` (`viewer.zig:232-234` vs `viewer.rs:346-360`);
- appending the action after `syncLayouts`;
- the stale PORT NOTE above.

### 1.5 New reference-defect candidates introduced by the range

Flagged by the readers, not hunted. Each needs an R13 decision when its commit is ported; the
next free id is UD352.

1. **`714fe9b90` `Surface.resizeWindow`** (`Surface.zig:1923-1954`). It is reachable from program
   output (`CSI 8;rows;cols t`) when `vt-window-resize-allowed = true`.
   - It computes `@max(cols,40) * cell.width + padding.left + padding.right` in unchecked `u32`
     (`:1940-1943`), then `@intFromFloat(@ceil(width/scale.x))` (`:1950-1951`).
   - A 65535-column request overflows once a cell is wider than 65,537 px, which
     `adjust-cell-width` can produce.
   - It also calls `scaledPadding`'s trapping cast (UD71).
   - Same class as UD59/UD71: extend them or open UD352.
2. **`219cca932` GTK `resizeDelta`** (`gtk/class/window.zig:996-1009`): an unchecked `c_int`
   conversion (`:1008`) and sum (`:996-997`). Same class as UD96.
3. **`3b8141fbd`**: the unterminated last stderr line is lost at EOF (see UD200, Q1).

---

## 2. Open items and waived ledger rows affected by the delta

### 2.1 Still-open items (closure README §"Still open")

| item | affected? | evidence |
|---|---|---|
| **N11** (renderer `error.<Name>` log args) | **yes, re-scope** | `2f0b65346` removes the `OpenGL.zig:242` site (displayRealized), moves `:308` → `opengl/Target.zig:159`, and turns `Frame.zig:65` `log.err` into `Frame.zig:70` `log.warn`. New `err={}` sites: `OpenGL.zig:77,87,93,255`, `Target.zig:87,125,192`, `pkg/opengl/egl.zig:90`. `Thread.zig:481` → `:471`. N11's two named tests lose their basis: `display_realized_warns_and_continues_when_context_prep_fails` (`opengl_impl.rs:986`, reference deleted) and `present_logs_when_the_srgb_re_enable_fails` (`:1070`, site now in the DMABUF export). |
| N130 (GTK title bytes) | no | Only `9fc8d9ebd` touches titles: `--title=` parsing moved to `class/Overrides.zig:101-106`, still raw `[:0]const u8`. |
| N35 (search_thread flake) | no | `search/Thread.zig` is unchanged. `06178eeaa` changes `ScreenSearch.feed` (`search/screen.zig:354`, `.complete` → `.history` instead of `unreachable`); that is new port work. |
| N354 (Windows flake) | code no | `Exec.zig` changes only the cImport line. `380778e3c` deletes upstream's `test-windows` job (see N376). |
| **N376** (CI gaps) | 5 gaps unchanged, **1 new** | The five gaps are unchanged, only moved: wasm ABI manifest `test.yml:742`, apple `:882`, nix `build-example-c-vt-build-info` `:1074-1092`, 5 MB check `:1183`, translations `:1983`. **New:** `380778e3c` deleted `test-windows` and `build-libghostty-windows-gnu`. Rustty's `ci.yml:519-527, 607, 674-686` now have no reference job (over-coverage; Q9). |
| N432 (async.zig:568) | no | The libxev pin is unchanged, and every `Async.wait` is still registered once at thread start. |
| N286 (fontconfig built without a fontconfig backend) | residue no; **new work** | The backend gate is unchanged (`SharedDeps.zig:283`), and the MSVC refusal basis still holds (`pkg/fontconfig/build.zig:160`, `override/src/fcint.h:45`). But `501e7b5c1` bumps fontconfig 2.14.2 → **2.18.3** with new override headers, so Rustty's `crates-sys/fontconfig-sys` (pinned at 2.14.2, `Cargo.toml:4,14`) needs a vendored resync (R4). |
| N311 | no | Already CLOSED in `NEW-FINDINGS.md`, but README §"Still open" (`:89-90`) still lists it (a stale doc line). |

### 2.2 WAIVED ledger rows (94)

- 39 rows cite a changed file; 55 cite unchanged files.
- Of the 39, **38 stay valid**; they only need their `oracle` column re-anchored.
- **1 needs re-adjudication:**
  - **S119-F3** (`renderer/opengl/Frame.zig:62-70`). The waiver argued that Rustty's
    `owns_context` path has no oracle counterpart, and that the windowed GDK path matches.
  - `2f0b65346`, `a925a97e3` and `47693cc4b` remove the GdkGLContext path. Upstream now always
    owns a surfaceless EGL context, and `present` returns `?ExportedFrame`. The log is now
    `log.warn` (`Frame.zig:70`), followed by `gl.finish()`, `pushFrame` and a `.redraw`.
  - Rustty's gate at `generic/linux_impl.rs:862` must be re-adjudicated with the GtkGLArea
    resync. Reopening the row turns the HARD `G-PARITY` gate red until it is closed or
    re-waived (Q6).
- Two **CLOSED** rows lose their reference to the same rewrite:
  - S113-F1: `displayRealized` was deleted, and `opengl_impl.rs:310` and its test `:986` are
    now orphaned.
  - S113-F4: the sRGB `log.err` moved to `Target.zig:159`.
- No waiver became obsolete.
- The 38 re-anchor rows are: S4-F1, S4-F4, S31-F2..F5, S33-F2, S37-F1, S38-F6, S41-F1, S45-F3,
  S46-F5, S55-F3, S55-F6, S56-F2, S57-F3, S58-F2, S58-F5, S65-F2, S65-F9, S76-F5, S94-F4,
  S100-F2, S101-F1, S112-F10, S115-F4, S116-F2, S145-F1, S146-F1, S148-F2, S149-F3, S155-F3,
  S163-F1, S163-F3, S165-F1, S180-F2, S207-F1, S207-F2. S37-F1, S46-F5, S94-F4 and S207-F2
  keep their line numbers.

### 2.3 NON_PORTS / ADJUDICATION entries whose basis moved

- **`.port/phase-4/NON_PORTS.md` TinyIo row.**
  - The verdict still holds: the reopen trigger, a new consumer that needs pluggable blocking IO,
    is not met. The Windows consumer is still `terminal_c.Io` (`c/terminal.zig:54-60`).
  - The row names only `src/lib/TinyIo.zig`. `f6cb8312b` split it into
    `src/lib/tinyio/{posix,windows,test}.zig`.
  - `audit_test_coverage.py` builds its non-port set from those backticked paths, so the 17
    moved tests and 26 `pub fn`s show as unexplained (§3) until the row names the new files.
- **phase-3 G13 §6** "`CLAUDE.md` (symlink to AGENTS.md) PORTED" is obsolete: `9d2d9acac`
  dropped CLAUDE.md upstream. Rustty's CLAUDE.md is the project's own agent instructions (Q8).
- **phase-3 G11**: the verdict holds, but the citations are stale. The translate-c backport
  dependency is gone (`fe9cf6a26`), and the wuffs `defines` moved to `pkg/wuffs/build.zig:5-16`,
  while `crates-sys/wuffs-sys/build.rs:14,46,52` cite `:4-15`.
- **phase-3 E25** (`os/windows.zig` "destination shape"): the new `ntdll.Nt*` and `FILE_*`
  declarations (`windows.zig:125-132`, `292-358`) now have callers in `tinyio/windows.zig`. They
  stay unused in Rustty while TinyIo is a non-port.
- **phase-3 G9** (`lib_vt` `_fltused`): the citations are one line off (`:167` → `:168`).

---

## 3. Gate impact of the pin advance

"Arrival" means right after `ORACLE_PIN` and the oracle checkout move to `12752b2ac`.

| gate | now (at pin) | arrival (simulated) | what makes it green |
|---|---|---|---|
| **G-CENSUS** (HARD) | GREEN (4288 / 17127 / 348) | **RED by design** | Record in `.port/phase-3/CENSUS_DELTA.md`: `total_test_blocks` 3234 -> **4397**, `total_assertions` 11975 -> **17717**, `files_with_tests` 279 -> **357** (+109 / +590 / +9 over the pin), plus a history-table row. Measured with the phase-0 `extract_tests.py` on the exported tree. |
| **G1b** (`check_test_counts.py`, PROGRESS) | GREEN | **RED, 1 row**: `input_key_encode` zig 104 / rust 96, drop=8 | Port the 9 new `key_encode.zig` tests. Other rows lose slack: pagelist 288→291, screen 216→224, render 20→28, terminal 423→428, stream_terminal 131→134, osc 310→316, stream 63→65, kitty 336→345, color 25→27, modes 13→16, search 72→75, tmux 114→113, vt_c 433→451, config_struct 116→117. **Green by slack, not coverage**, as in phase 6. The MAP does not cover the new or changed test files `kitty/windows.zig`, `datastruct/{comptime_int_set,untouched_pool,wasm_page_pool}.zig`, `lib/parse_int.zig`, `lib/types.zig`, `apprt/gtk/scale.zig`, `pkg/wuffs/src/zlib.zig` (the wuffs row lists only png/jpeg/main/error/swizzle), `font/nerd_font_attributes.zig`, `snapshot/terminal.zig`. Extend the MAP at the start of the phase. |
| **G-TESTS** (`audit_test_coverage.py`, HARD) — the honest meter | GREEN (4085 blocks, 0 unexplained) | **RED: 4211 blocks, 94 unexplained**: tinyio/test.zig 17 + tinyio/windows.zig 1 (the TinyIo non-port row), c/render 10, gtk/scale 7, kitty/graphics_image 7, render 7, kitty/windows 5, key_encode 4, datastruct/untouched_pool 4, Terminal 4, and 28 more across 21 files | Port the tests and extend the TinyIo NON_PORTS row. **Blind spot:** it matches by name only, so none of the **40 changed test bodies** (§4) can turn it red. |
| G1 (`check_tests_ported.py`) | GREEN 5898/3227 | unaffected | Its contract is frozen at `4749c4e93`. The two tests deleted in the range (`c/terminal.zig` "set write_pty callback", `tmux/output.zig` "parse mouse_any_flag") are not in the contract. |
| **G-FILES** (HARD) | GREEN (545 files) | **RED: 557, 8 MISSING**: `gtk/class/Overrides.zig`, `gtk/class/render_surface.zig`, `gtk/scale.zig`, `datastruct/comptime_int_set.zig`, `datastruct/untouched_pool.zig`, `font/nerd_font_tables.zig`, `lib/parse_int.zig`, `renderer/Dmabuf.zig` | Port them, or write NON_PORTS rows. |
| **G-SYMS** (HARD) | GREEN (2845 `pub fn`) | **RED: 2922, 57 unexplained** (tinyio/posix 13 + tinyio/windows 13, gtk/scale 8, generic.zig 4, kitty/windows 3, …) | Port them; extend the TinyIo row. |
| **G3b** (PROGRESS) | GREEN, 0 missing | **RED, 610 missing**: vt 437, vt-c 60, core 29, bench 27, os 17, config 13, font 12, apprt-api 5, lib 5, termio 5. apprt, bin and renderer stay SKIPPED (PORT STATUS not done), so the GTK and OpenGL rewrite text is not measured at all. | Carry the new reference comments with each port. `--not-carried-check` stays GREEN (37 rows). |
| G3 (`check_comments_fidelity.py`) | — | not run | Reads the same directory as G3b; expect the same drift. |
| **G9** (`check_parity_surface.py`) | GREEN | **unaffected, but blind**: it reads the frozen `.port/phase-0/inventory/parity-surface.json` | New surface it cannot see: config `vt-window-resize-allowed` (`714fe9b90`/`219cca932`/`2a987d59d`) and apprt action `resize_window` (`action.zig`). Audit them by hand. |
| **G-LOGFILE** (HARD) | GREEN (1392 sites) | **RED, 6 arms** (below) | Re-anchor the LOG-*.tsv rows, declare new files, port the new sites. |
| G-REFGEOM (HARD) | GREEN | **expected GREEN (by reading, not run)** | `style.zig`, `hyperlink.zig` and `size.zig` are unchanged, and `RefCountedSet.Item` is unchanged (`c0a4f80d8` only adds `initAssumeZeroed`). It still has to run at the new pin: it builds a scratch copy with zig 0.16.0 (available locally). **Not covered by it:** `d2ff6d77a` changes `Page.layout` (cache-line aligned cells, one metadata block) and `Capacity.adjust`, so the port's page-capacity parity (N88) and the UD64/UD80/UD131 constants must be re-derived. |
| **G4** (differential) | armed | **BLOCKED** until HEAD == pin, then needs the oracle `zig-out/include` rebuilt and `.port/phase-0/harness/build_ref_driver.sh` re-run | Behaviour changes that can diff leg A: reverse-wrap cursor (`d4f45bee3`), word selection (`c4f15c884`, `a3e80a685`), and **possibly every multi-page fixture if the canonical PageList serialization reflects page capacity (`d2ff6d77a`)**; check the comparator's format. Leg B (c-vt goldens): no `example/` file changed, but `dd2edd760` (safe non-NULL pointers for empty output) and DECRQM ANSI replies (`9dc0d974e`) are C-API-visible. `cvt-parity.sh` was already BLOCKED oracle-side at the pin (phase-6 open item). |
| G-EOL (HARD) | GREEN | expected GREEN after the pin moves (arm 3 is BLOCKED until then) | `test/fuzz-libghostty/corpus` is unchanged in the range, as are `dist/windows` and the harness fixtures. The `crates-sys/*/vendor` families change only when R4 vendors fontconfig 2.18.3; add the new override headers byte-exact. |
| A3 (`check_rustty_h_abi.sh`) | GREEN | **BLOCKED**: the `oracle-ghostty-h.sha256` freeze-pin mismatches (`de852e83…` → `78232aec…`) | Bump the sha in the advance commit. The function-set compare stays green: no new `GHOSTTY_API` function (A1's 90 are unchanged). `include/rustty-embed.h` must gain `rustty_action_resize_window_s`, `RUSTTY_ACTION_RESIZE_WINDOW` and the union member (the exact rename of the new `ghostty.h` hunk). |
| A4 (`check_embed_h_abi.sh`) and `crates/rustty-apprt-c/tests/abi_layout.rs` | GREEN | green until the umbrella is regenerated, then needs a `#[repr(C)]` 8-byte `ResizeWindow` plus the new ordinal in the `ActionKey` pins | Part of R2/R3's `714fe9b90` port. |
| **A5 / VT headers** (`check_rustty_vt_headers.sh`) | **already affected now** | same | Its export arm and `check-vt-struct-layouts.mjs` read **`origin/main`, not the pin**. `lib_vt.zig`'s export list is unchanged (only a doc comment), so the export arm is fine. But the type-registry arm reads `origin/main:src/terminal/c/types.zig`, which now has `GhosttyRenderStateOverscan`, `GhosttyRenderStateRowId` and `GhosttyMouseShape`, so it is **expected RED already**, since the fetch, whether or not the pin moves (by reading; not run, because the gate builds). Green needs the three `rustty_type_json` types plus the regenerated `include/rustty/vt/*.h`. |
| VT header content (cbindgen) | — | — | Upstream header delta: `render.h` +228 (OVERSCAN data/option, ROW_DATA_VIEWPORT_Y/ID, two structs), `terminal.h` +176 (OPT_RESIZE_PULL_SCROLLBACK=40, OPT_RENDER_HOLD=41, DATA_MOUSE_SHAPE=41, `GhosttyTerminalRenderHoldFn`), `mouse.h` +51 (`GhosttyMouseShape`, 34 values plus `_MAX_VALUE` sentinel), docs in allocator/formatter/search/selection/types. `mouse/shape.h` is added by `78494bad8` and folded into `mouse.h` by `67fb0c940` (net end state: no new file). All new C enums carry the `_MAX_VALUE` sentinel, which Rustty's CLAUDE.md rule also requires. |
| G-ABISCHEMA | GREEN | unaffected until the port | Internal (Rust manifest vs the ported schema); upstream's schema is unchanged. The 3 new manifest types must validate. |
| G-VTFEAT | GREEN | unaffected | `terminal/build_options.zig` and the lib_vt export blocks are unchanged. |
| G-I18N | GREEN | unaffected until the catalogs are copied | Internal msgcmp against Rustty's own .pot; upstream's .pot is unchanged. 10 catalogs change upstream (de, eu, hr, it, lt, mk, nb, ru, vi, zh_CN); porting them must keep it green. |
| D3 (`check_constants_parity.py`) | GREEN | GREEN (simulated) | Uses the frozen crate_map, so it is blind to new files. |
| D10 (stb) | GREEN | unaffected | Only `src/stb/main.zig`'s `@cImport` → `@import("stb_c")` changes; the C sources are unchanged. |
| G-BENCH / G-LZ4 | GREEN | unaffected | Both read `origin/main`, but `benchmark/cli.zig` and `terminal/compress/*` are unchanged. |
| G-PARITY (HARD) | GREEN | GREEN unless S119-F3 is reopened | — |
| G2 (assertions) | GREEN | unaffected | Frozen floor. |

**G-LOGFILE arrival detail** (simulated; 172 files, 1392 → 1412 live sites):
1. **4 files with live `log.` sites and no declaration** (16 sites): `gtk/class/Overrides.zig` 7,
   `gtk/class/render_surface.zig` 4, `renderer/opengl/Target.zig` 4, `pkg/opengl/egl.zig` 1.
2. **4 `LOG-DEAD-SITES.tsv` rows off-line**: `Exec.zig:1078`, `gtk/class/surface.zig:3618`,
   `bitmap_allocator.zig:215`, `Termio.zig:705`.
3. **36 `LOG-ABSENT-SITES.tsv` rows off-line**. Examples: `gtk/class/surface.zig:2735/2762`, and
   `generic.zig:991/1169/1177/1183/1188/1190`.
4. **1 absent row now excuses a live emission**: `:116`, `application.zig:1804` →
   `application.rs:2422`.
5. **14 files short of their allowance**. Most of the shortfall is line drift in the absent rows
   plus new upstream sites. Surface.zig is 7 short (+5 new sites), embedded.zig 1,
   gtk/application.zig 7 (7 moved to Overrides.zig), imgui_widget 1, ScreenClone 4 (+4 new),
   Config.zig 3, OpenGL.zig 5 (+7/−2), generic.zig 9, bitmap_allocator 1, mem.zig 1 (+1),
   stream.zig 1 (+1), Exec.zig 3, Termio.zig 2 (+1), stream_handler 1.
6. **7 preserved comments that quote a call not at the pin (N20)**: `surface.rs:3520/4412/4524`,
   `linux_impl.rs:1280`, `opengl_impl.rs:281/319`, `graphics_image.rs:1238`.

Also: stale preserved comments rise 18 → 25, against a divergence cap of 28.

**New or removed `log.` sites in the range**, from the per-file `+`/`−` count of the range diff
(moves count on both sides):
- `pkg/opengl/egl.zig` +1
- `Surface.zig` +5
- `apprt/embedded.zig` +2/−1
- `gtk/class/Overrides.zig` +7
- `gtk/class/application.zig` +2/−7
- `gtk/class/imgui_widget.zig` +1
- `gtk/class/render_surface.zig` +4
- `gtk/class/surface.zig` +6/−14
- `benchmark/ScreenClone.zig` +4
- `os/open.zig` +1/−1
- `renderer/OpenGL.zig` +7/−2
- `renderer/Thread.zig` +1/−1
- `renderer/opengl/Frame.zig` +1/−1
- `renderer/opengl/Target.zig` +4
- `kitty/graphics_image.zig` +1/−1
- `terminal/mem.zig` +1
- `terminal/stream.zig` +1
- `termio/Termio.zig` +1

Total: +50 / −28.

**LOG-*.tsv, COMMENT-NOT-CARRIED.tsv, LEDGER `oracle` column.**
- All are line-anchored at the pin.
- `LOG-ABSENT-SITES.tsv` (127 lines) and `LOG-DEAD-SITES.tsv` (46 lines) are the ones G-LOGFILE
  proves stale (36 + 4 rows).
- `LOG-PORTMAP.tsv` needs rows for the four new files.
- `LOG-PORT-ONLY` / `LOG-ATTRIBUTION` / `LOG-TEXT-DIVERGENCES` did not trip.
- COMMENT-NOT-CARRIED is GREEN simulated.
- LEDGER: 38 waived rows need re-anchoring (§2.2), plus every CLOSED row citing a changed file
  (not enumerated here).

---

## 4. Upstream tests to carry (`R5-upstream-tests.tsv`)

Every `test` block in every `.zig` file the range touches was extracted at both revisions, and
the body text compared. That gives **137 added, 40 changed, 23 removed** (200 rows). The raw
`^[+-]\s*test` counts of the range diff confirm the add and remove numbers (137 / 23).

- **Moves are not new tests.** 21 of the removals and 21 of the additions are `TinyIo.zig` →
  `lib/tinyio/test.zig` (one of them renamed, "pipes: streaming works, positional…" → "…;
  positional…"). `tinyio/windows.zig` adds 2. All 24 are in the **TinyIo non-port** (§2.3). The
  2 other adds are anonymous `test {}` import blocks (`TinyIo.zig`, `graphics_image.zig`).
- **Tests the port must carry by name: 112.**
  - 107 in `src/`: terminal 77 (of which c/* 19), datastruct 10, input 9, apprt 7, lib 2,
    config 1, font 1.
  - 5 in `pkg/wuffs/src/zlib.zig`, outside G-CENSUS and G-TESTS scope; they belong to
    `rustty-wuffs`.
  - Per file: key_encode 9, c/render 10, render 8, Screen 8, kitty/graphics_image 8 (named),
    gtk/scale 7, kitty/windows 6, Terminal 5, c/terminal 5, comptime_int_set 5, untouched_pool 5,
    wuffs zlib 5, PageList 3, modes 3, stream_terminal 3, color 2, stream 2, 1 each in 17 other
    files.
- **Changed tests the port must re-align: 40.**
  - bitmap_allocator 15 (`ffe015ee5`)
  - color `DynamicPalette` 7 (`d1cd56a56`)
  - snapshot/terminal 3 (`d1cd56a56`)
  - c/terminal 2
  - tmux/viewer 2 (`9a4ba7d54`, `1ca2858d0`)
  - 1 each in SharedGrid, key_encode, surface_mouse, wasm_page_pool, modes, render,
    stream_terminal, c/allocator, c/search, c/types, gtk.zig
  - **No gate sees a changed body** (G1b and G-TESTS count or match names), so the TSV is the
    checklist.
- **Removed, other than moves: 2.**
  - `c/terminal.zig` "set write_pty callback", replaced by the DECRQM variants in `9dc0d974e`.
  - `tmux/output.zig` "parse mouse_any_flag" (`1ca2858d0`).
  - Rustty should drop or retarget its counterparts.

---

## 5. Commit inventory (`R5-commits.tsv`)

The `area` column follows each reviewer's declared scope:
- R1: `src/terminal/**` except `c/`, `src/datastruct/**`, `src/lib/parse_int.zig`.
- R2: apprt (except embedded and gtk/build), renderer, font, `pkg/opengl`, `vendor/glad`,
  `macos/`, `Surface.zig`, `surface_mouse.zig`, and pkg harfbuzz, freetype, spirv-cross,
  glslang, dcimgui, gtk4-layer-shell and macos.
- R3: `src/lib/**`, `include/**`, input, config, termio, os, shell-integration, benchmark,
  `embedded.zig`, `example/`, `src/terminal/c/**`, `lib_vt.zig`.
- R4: build, `src/build/**`, `gtk/build`, pkg fontconfig, translate-c, wuffs, oniguruma and
  sentry, `.github`, `po`, `vendor` (except glad), `CLAUDE.md`.
- non-port: `VOUCHED.td`.

**Counts:** R4 101, R2 53, R3 22, R1 21, non-port 21, R1+R3 8, R2+R4 3, R2+R3 2,
R2+R3+R4 1, R1+R2+R3 1, R1+R4 1.

**Files outside every declared scope**, all in commits another reviewer does cite:
- nix, flake and flatpak, cited by R4 (10 commits);
- `src/unicode/grapheme.zig` (`12542b392`), cited by R4 as workstream "B. uucode → Unicode 18";
- `src/stb/main.zig` (`fe9cf6a26`), cited by R2, R3 and R4;
- `src/App.zig` and `src/apprt.zig` (`2f0b65346`), cited by R2 (`R2-…md:206,346`).

These rows carry `outside-declared-scopes` in the `flag` column; the TSV assigns them to R4 or R2.

**Commits no reviewer cites (4):** `b32b2f87a`, `9c4a5a899`, `3879bff33`, `1aaca23e3`, all
`po/de.po` only. They are duplicate-history copies of the cited `ba8690c42` / `8b5678bd1` /
`fee8cb523` / `4cc8fa335` (same subjects, merged twice). R4's end-state `po/de.po` port covers
them. Every other non-VOUCHED commit is cited by at least one R1–R4 report.

---

## 6. Proposed advance procedure (from phase 6 and `ORACLE_SYNC.md` §"Advancing the pin")

1. **Pre-flight.**
   - `git worktree list` shows only main `079cc21e`, so no other lane is active.
   - `git -C ../ghostty status --porcelain` is empty.
   - The oracle HEAD is `20abdb50a` (verified 2026-09-28).
   - `zig version` is 0.16.0, matching upstream's `minimum_zig_version`.
   - Plan the Linux (`rvm`) lane early: the GtkGLArea removal and EGL/DMABUF code are Linux-only,
     and so are 30 N11 sites.
2. **Record "before" at `20abdb50a`** into `.port/phase-7/w0/*-before-at-20abdb50a.txt`:
   - G1b, G3b (default, plus `--strict` for apprt/renderer/bin);
   - G-TESTS, G-FILES, G-SYMS (all with `--limit 500`);
   - G-CENSUS, G-LOGFILE, G-REFGEOM, A3, A5.
   - A5's struct-registry arm already reads `origin/main`; record its RED as pre-existing.
3. **Range artifacts** (as phase 6): `UPSTREAM_DIFF.patch`, `UPSTREAM_DIFF_STAT.txt`,
   `UPSTREAM_COMMITS.txt` (234), `UPSTREAM_COMMITS_WITH_MERGES.txt` (333), and `R5-*.tsv` as the
   assignment checklist.
4. **Advance.**
   - `git -C ../ghostty checkout --detach 12752b2ac1bb05ce53402ed8c853ed1f96eef0b1`.
   - Rebuild the oracle `zig-out/include`, then re-run
     `.port/phase-0/harness/build_ref_driver.sh`. G4's guard cannot see a stale driver; at the
     last closure G4 had passed against stale drivers.
   - `bash .port/tools/gates/_oracle_root.sh --self-test`.
5. **One commit, stating the absorbed range:**
   - `.port/tools/gates/ORACLE_PIN`;
   - `.port/ORACLE_SYNC.md` (the pinned block, a history row, §"Upstream beyond this pin");
   - `.port/PORT_RULES.md` R12;
   - `.port/cycle-analysis.md`;
   - `.port/tools/oracle-ghostty-h.sha256` → `78232aec570e266b1ccacaa69b0536c9b7757f4cf747ad26b9d7fb2d378848dd`
     (A3 BLOCKED otherwise);
   - `.port/phase-3/CENSUS_DELTA.md` to 4397 / 17717 / 357 (G-CENSUS is HARD; recording the
     delta is legitimate once the pin moves).
6. **Record "after"** (same gate set) plus the per-crate G3b delta in both directions. The
   expected arrival state is §3: G1b RED (1 row), G-TESTS 94, G-FILES 8, G-SYMS 57, G3b 610,
   G-LOGFILE RED (6 arms). Extend the G1b MAP rows for the new test files **at phase start** (as
   phase 6 did for `search/terminal.zig`), so the per-module floor measures them.
7. **Records at phase start:**
   - Extend the TinyIo NON_PORTS row to `src/lib/tinyio/{posix,windows,test}.zig`; this is
     documentation, not porting, and removes 18 test gaps and 26 symbol gaps that are not real.
   - Set the UD status lines for UD150, UD140, UD141, UD31, UD40 and UD32 (§1.1).
   - Reopen S119-F3 if the maintainer agrees (Q6).
8. **Workstreams and finale** as phase 6: one hand-made worktree per workstream, at most three
   building at once, Linux lane early. At the finale:
   - G-TESTS 0 unexplained, with all 112 tests present by name;
   - every changed test in `R5-upstream-tests.tsv` ticked;
   - G1b, G3b, G-FILES, G-SYMS, G-LOGFILE, A3, A4, A5 and G4 green, G-REFGEOM re-run;
   - xcodebuild RusttyTests and the menu-shortcut parity harness (macOS Swift changed in 9
     commits);
   - the Linux full lane, plus Windows for tinyio and kitty/windows;
   - re-derive the UD64/UD80/UD131 constants and re-verify the UD131 recipe.

---

## 7. Open questions for the maintainer

1. **UD200**: re-align to upstream `3b8141fbd`, which loses an unterminated last stderr line at
   EOF, or keep Rustty's behaviour and record the EOF loss as a new UD (R13 evident intent)?
2. **Upstream-fixed UDs.** Once Rustty re-aligns and there is no divergence left (UD150, UD140's
   zlib half), should the entry stay with `upstream status: fixed upstream in <sha>`, and its
   `docs/ghostty-bug-fixes.md` row be removed? The row is currently required by the UD-docs
   check, `.superpowers/…/task-S-6-check_ud_docs.py`, which lives in a gitignored directory and
   is not a `ci.sh` gate.
3. **UD131 / UD351** are still unfixed at `12752b2ac`. Does the private-advisory decision stand as
   is? The UD131 recipe (LF count) should be re-verified at the new pin first.
4. **New defect sites from `714fe9b90` / `219cca932`**: extend UD59, UD71 and UD96, or open
   UD352 and later (§1.5)?
5. **UD reference lines.** All 178 entries are "at 20abdb50a". Re-anchor every entry at the
   advance, or only those whose code changed, leaving the rest pin-stamped at `20abdb50a`?
6. **S119-F3**: reopen now, which turns HARD G-PARITY red during the resync, or re-adjudicate in
   place when the GtkGLArea removal lands?
7. **TinyIo**: extend the NON_PORT row (verdict unchanged), or treat `f6cb8312b`'s Windows
   TinyIo in the C API as meeting the reopen trigger?
8. **CLAUDE.md**: upstream dropped it (`9d2d9acac`). Keep Rustty's as a documented divergence?
9. **N376**: upstream deleted `test-windows` and `build-libghostty-windows-gnu` (`380778e3c`).
   Keep Rustty's jobs (over-coverage; the `scope-parity-only` rule suggests dropping them), or
   keep them as Rustty's own Windows validation?
10. **Gate design.**
    - A5's export and struct-registry arms read `origin/main` rather than the pin, so a plain
      `git fetch` changes their verdict (expected RED now).
    - G9, G1 and D3 read frozen inventories, and no gate sees changed test bodies.
    - Should any of these become pin-relative or gain a body-diff arm?
11. **G4 serialization**: does the comparator's canonical PageList serialization include page
    boundaries or capacity? If so, `d2ff6d77a` can diff every multi-page fixture until the
    layout change is ported.

---

## Appendix A — new `origin/main` anchors for unchanged UDs (from the reader reports)

- UD3 `x11.zig:342-427` (callers `:214-231, :239, :285`)
- UD4 `application.zig:2385-2428`
- UD6 `stream_handler.zig:411`
- UD9 `viewer.zig:1076-1083`
- UD11 `viewer.zig:1019-1030`
- UD12 `viewer.zig:1323, :1337`
- UD13 `application.zig:2076, :1581-1584`
- UD15 `PageList.zig:5726-5741`, test `:18987-19033`
- UD16 `action.zig:441…`
- UD18 `Config.zig:5135-5186`, tests `:10641…`
- UD20 `generic.zig:2363-2381`
- UD26 `Surface.zig:673-674`
- UD34 `PageList.zig:6642-6649`
- UD57 `generic.zig:2356-2378`
- UD58 `Surface.zig:5418`
- UD59 `Surface.zig:1882-1912`
- UD61 `graphics_image.zig:210-276`
- UD62 `Surface.zig:5358`
- UD63 `c/terminal.zig:1343`
- UD66 `snapshot/terminal.zig:889-897`
- UD67 `Exec.zig:302`
- UD68 `Ghostty.App.swift:943/979/1007`
- UD70 `Thread.zig:221-222`, `generic.zig:918`
- UD71 `Surface.zig:445-463`
- UD73 `Thread.zig:548…`
- UD75 `Thread.zig:722-731`
- UD76 `Surface.zig:2195, 2208`
- UD90 `surface.zig:661-662, 3222-3277`
- UD95 `Surface.zig:402`
- UD96 `Config.zig:9502-9535`
- UD98 `Config.zig:6460`
- UD102 `Exec.zig:104`
- UD103 `window.zig:1437-1446`
- UD104 `Surface.zig:586,590,696`
- UD112 `application.zig:365-376`
- UD113 `c/terminal.zig:1485-1496`
- UD115 `Surface.zig:528-531`
- UD172 `surface.zig:3982-4246`
- UD173 `application.zig:1132-1208`
- UD174 `application.zig:1764-1795`
- UD180 `Config.zig:7564`
- UD181 `Config.zig:8167-8176`
- UD205 `stream_handler.zig:1558-1778`
- UD206 `Surface.zig:1344`
- UD230 `stream_handler.zig:1463-1556`
- UD247 `Program.zig:28-44` (same lines)
- UD270 `viewer.zig:1111-1126`
- UD272 `viewer.zig:1067, 1108`
- UD273 `viewer.zig:1046-1109`
- UD295 `surface.zig:3697-3724`, `surface.blp:266-274`
- UD300 `application.zig:2371, 2512, 3185`. The entry's `moveTabToNewWindow` remark is wrong:
  its text is "ignoring new_tab_to_new_window".
- UD305 `c/search.zig:182-211`
- UD310 `color.zig:283-291`
- UD313 `color.zig:88`
- UD320 unchanged lines
- UD321 `surface.zig:2679-2753`
- UD350 `SharedGrid.zig:83-117`, test `:647`

Two entries already had wrong pin anchors: UD205 (actually `:1557`, buffer `:1569`) and UD230
(actually `:1462-1555`).
