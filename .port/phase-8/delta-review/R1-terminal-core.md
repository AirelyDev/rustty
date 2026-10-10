# Phase 8 delta review R1: terminal core (`src/terminal/**` except OSC parsing and the C API, `src/datastruct/**`, `src/input/{key_encode,mouse_encode,function_keys}.zig`)

Range `12752b2ac..a60e9e2a5` (read with `git -C ../ghostty show` / `git diff`; the oracle working
tree stays detached at the pin `12752b2ac`, verified). Rustty at `401b95b0e`. Read-only review: no
Rustty code or record was changed and nothing was built. Upstream line numbers are at `a60e9e2a5`;
Rustty line numbers are at `401b95b0e`.

**Scope.** 30 non-merge commits touch this area, in whole or in part (listed in §1; `5170b06f6` only
touches `src/Surface.zig`, `9a4787d90` only `include/ghostty/vt/terminal.h`, both assigned here). The
OSC parser (`osc.zig`, `osc/parsers/*`), `c/*.zig` and `include/ghostty/vt/*.h` halves belong to R2;
the `config/` and `termio/` halves of the DECRQCRA set belong to whoever owns those tiers (§4 Q4).

**Method.** For each commit: read the upstream diff and the end-state file; locate the Rustty
counterpart (whole-workspace grep before calling anything absent); check UD/N/V records; enumerate
`test "…"` blocks with `git diff 12752b2ac a60e9e2a5 -- <file> | grep '^[+-]test "'` and attribute
them per commit with `git show <sha> -- <file>`.

**Tests in scope: 43 added, 25 changed, 0 removed** (the two `xt_checksum: rect …` tests added by
`9272a2f7c` were renamed to `selection …` by `5f0d4febc` inside the range; port the end-state names).

| file | added | changed |
|---|---|---|
| `Terminal.zig` | 8 | 2 (`print charset outside of ASCII` expectation; a comment above `print wide char at right edge with hyperlink`) |
| `xt_checksum.zig` (NEW) | 12 | — |
| `stream_terminal.zig` | 8 | 1 (`unknown APC effect callback`) |
| `PageList.zig` | 3 | 3 (`finalize` signature) |
| `tmux/control.zig` | 3 | — |
| `stream.zig` | 2 | — |
| `Screen.zig` | 1 | 12 (pending-wrap resize tests) |
| `page.zig`, `Parser.zig`, `snapshot/history.zig`, `input/key_encode.zig`, `input/mouse_encode.zig`, `Surface.zig` | 1 each | `snapshot/history.zig` 3 |
| `search/{pagelist,screen}.zig` | — | 1 each (`finalize` signature) |
| `datastruct/untouched_pool.zig` | — | 1 (`UntouchedPool: create, destroy, reuse`) |

## 1. Summary table

Status is Rustty's state today. Size: S under 2 h, M under 1 day, L more than 1 day. "Obs." says
whether the change is observable behaviour (Y) or internal (N).

| # | commit(s) | theme | obs.? | Rustty status | size | top risk |
|---|---|---|---|---|---|---|
| 1 | `e6db5b633` | narrowing without reflow clears a wide head whose tail was cut | **Y (crash fix)** | ABSENT; Rustty **carries the defect**, no UD/N record | S | debug-build panic today; port before any resize-heavy test lane |
| 2 | `f9ab34f10` | live cursor pending wrap repaired after resize | Y | ABSENT | S–M | 12 Screen test expectations change with it |
| 3 | `afded91df` | saved cursor stays put across repeated widening (reflow pin clamp uses the destination column) | Y | ABSENT | S | none |
| 4 | `a573781c6` | full-row clear always drops the kitty placeholder row flag | Y (placement scan) | ABSENT | S | two sites (Screen + Page) must change together |
| 5 | `bb20f8e45`, `2b1edded2` | RIS resets the palette; `fullReset` reorder | Y | ABSENT (palette); reorder is cosmetic | S | G3 comment drift only |
| 6 | `6220a3617` | codepoints > 0xFF print unmapped under a non-UTF-8 charset | Y | ABSENT (Rustty prints a space) | S | none |
| 7 | `c706451fe` | single shift consumed once per printed codepoint; new `writeCell` | Y | ABSENT | S–M | hot print path (`print_cell` split); perf fence |
| 8 | `13b5ab204` | `selectLine` whitespace trim bounded by the end column | Y | ABSENT | S | none |
| 9 | `9272a2f7c`, `5f0d4febc`, `b2f557c6c`, `cc0276778`, `01b9f4958`, `00a19db51` (terminal half) | DECRQCRA / XTCHECKSUM; new `xt_checksum.zig` | **Y** (new replies) | ABSENT (no `xt_checksum` anywhere in the workspace) | M–L | security default (off); cross-tier with config, termio, C option; new file not in the G3b `crate_map` |
| 10 | `e8b858cc6` | tmux control parser: unstored bytes don't count against `max_bytes` | Y | ABSENT | S | UD6 citations go stale; UD6 itself still open upstream |
| 11 | `e55c687a4` | legacy ctrl+alt+shift+backspace → `ESC BS` | Y | ABSENT | S | none |
| 12 | `157c44f64` | DECSET 1005: the button is UTF-8 encoded | Y | ABSENT | S | keep UD60's "validate before writing" order |
| 13 | `8834e161c` (PageList/Screen/pool halves) | `memoryUsage` on PageList and Screen; pool `freeCount` | N (new API) | ABSENT (`memory_stats` exists) | S–M | wasm pool arm returns 0 |
| 14 | `88e66cbc6` (PageList/history/snapshot halves) | compress each prepended history page during snapshot decode | N (RSS; opt-in) | ABSENT | M | `finalize` signature churn across 6 call sites |
| 15 | `7bb45ba34` (stream_terminal half) | `semantic_prompt` and `reset` effects; `SemanticPrompt` event type | Y (embedder API) | ABSENT | M | C enum order (R2); `SemanticPrompt` name collides with `stream::SemanticPrompt` |
| 16 | `7b11f3dca` (stream + stream_terminal halves) | `osc_unknown` action, `osc_unknown_max_bytes`, `UnknownSequence.osc` | Y (embedder API) | ABSENT | S | depends on R2's `osc.Command.Unknown` |
| 17 | `bae2c3cdb` (stream + stream_terminal halves) | `program_status` action + effect; RIS clears records | Y (embedder API) | ABSENT | S–M | depends on R2's OSC 7501 parser |
| 18 | `b1bfd1d9c` (stream + stream_terminal halves) | empty OSC 22 → `mouse_shape_reset` | Y | ABSENT | S | termio half resolves differently (by design) |
| 19 | `520d8f55a` (stream_terminal test) | CAN/SUB cancel an OSC (behaviour lives in `osc.zig`, R2) | Y | test ABSENT | S | none in R1 |
| 20 | `36953bca8` (Parser test) | `osc: 105 empty` (behaviour in `osc.zig`, R2) | Y | test ABSENT | S | none in R1 |
| 21 | `5170b06f6` | relative prompt click counts screen rows between pins | Y | ABSENT (`saturating_sub` of a page-local y) | S | none |
| 22 | `9a4787d90` | header doc: same-size resize is not a no-op | N (doc) | ABSENT (`include/rustty/vt/terminal.h:2177`) | S | doc only; behaviour already matches |

Totals: 22 themes over 30 commits. **Status:** 22 absent (two are test-only, one doc-only); 0
partial; 0 N/A. **Size:** 1 M–L, 6 M or S–M, the rest S. **UD candidates:** none strong (see §2.10).
**Carried defect:** #1 (no record).

## 2. Per-theme sections

### 2.1 Wide character cut by a non-reflow narrow (#1, `e6db5b633`)

- **Upstream.** `PageList.zig:2823-2829` (`resizeWithoutReflow`, `.lt` arm): before
  `clearCells(row, start, self.cols)`, `start = cols - 1` when `cells[cols - 1].wide == .wide`, else
  `cols`. Commit message: a head left in the new last column makes `insertBlanks` and
  `eraseLine(.left)` panic in `Screen.clearCells`, and `rowWillBeShifted` writes `cells[cols]`.
- **Rustty.** `crates/rustty-vt/src/pagelist/resize.rs:708-712` clears from `cols` only — the
  pre-fix code. Consequences at `401b95b0e`:
  - debug builds: the range `debug_assert!` in `Screen::clear_cells`
    (`crates/rustty-vt/src/screen/clear.rs:196-199`) panics on ICH / EL 1 against the orphaned head
    (`terminal/edit.rs:650` `insert_blanks` → `clear_cells(…, end_multi[0..2])`);
  - release builds: no memory unsafety (a row keeps `capacity.cols` cells, so `cells[cols]` stays
    inside the row's stride), but `row_will_be_shifted` (`terminal/edit.rs:19-75`) and the clears
    write a cell past `size.cols`, and the half wide character renders in the last column.
  - Reachable from any program: resize narrower while the alternate screen (never reflows) or
    `wraparound` off holds a wide char straddling the new edge, then `CSI @` or `CSI 1 K`.
- **Records.** No UD/N/V entry covers it (grepped `UPSTREAM-DEFECTS.md`, `NEW-FINDINGS.md`,
  `VERIFICATION_DEBT.md`, `docs/ghostty-bug-fixes.md` for the function names and wording).
- **Port.** 3-line change at `resize.rs:711` reading `cells.add(cols - 1)` through the raw row
  pointer (same raw discipline as the surrounding block).
- **Test (1):** `Terminal: resize less cols without reflow cutting wide char tail`
  (`Terminal.zig:15939`), next to `terminal/tests/resize.rs:429`.
- **Fix completeness.** Checked: a `spacer_head` cannot sit at `cols - 1` after the cut (spacer heads
  only live in the old last column, which is cleared); grapheme data on the head is released by
  `clearCells`. No residual defect.

### 2.2 Resize, wrap and saved cursor (#2, #3)

- **`f9ab34f10` upstream.** `Screen.zig:2151-2160`: after `cursorReload()`, if
  `cursor.pending_wrap and cursor.x != opts.cols - 1` then clear `pending_wrap` and `cursorRight(1)`.
  `cursorRight` is safe because `cursorReload` clamps `x <= cols - 1` and the guard excludes the edge.
- **Rustty.** `crates/rustty-vt/src/screen/clear.rs:723` (`cursor_reload`) — insert the block
  between it and `clear_prompt_for_redraw` at `:728`, mirroring the saved-cursor block at `:749-756`.
- **Tests.** Added: `Terminal: resize pending wrap live and saved cursors` (7 cases × restore on/off)
  after `terminal/tests/resize.rs:1307`. Changed (12, `screen/tests/resize.rs`):
  `Screen: resize more rows with populated scrollback`, `… more cols no reflow` (`:574`, now
  `x + 1` and `!pending_wrap`), `… more cols with reflow that fits full width`, `… that forces more
  wrapping`, `… that unwraps multiple times`, `… more cols with populated scrollback`, `… more cols
  with reflow`, `… more rows and cols with wrapping` (`:1461`, x 3 → 4), `… less rows no scrollback`,
  `… less rows moving cursor`, `… less cols with reflow but row space`, `… less cols with reflow
  previously wrapped and scrollback`. Ten of them only add `s.cursor.pending_wrap = false;` after
  `cursorAbsolute` (a state `Terminal` never produces).
- **`afded91df` upstream.** `PageList.zig:1688-1706` (`ReflowCursor`, tracked-pin loop):
  `dst_x = if (self.new_rows > 0 or self.pending_wrap) 0 else self.x`, and the blank-pin clamp uses
  `size.cols - 1 - dst_x`.
- **Rustty.** `crates/rustty-vt/src/pagelist/resize.rs:1511-1540` uses `self.x` at `:1533`; the
  `ReflowCursor` already has `new_rows` and `pending_wrap` (`:1330`, `:1335`). Two-line change.
- **Tests (2):** `PageList resize reflow pin in blank cells after line break` (3 cases; next to
  `pagelist/tests/resize_noreflow_b.rs:1869`), `Terminal: saved cursor survives repeated widening`.
- **Risks.** Both touch tracked-pin / cursor-pin raw paths already under the vt Miri lanes; no new
  aliasing. Same-size resizes still return early (`terminal/resize.rs:131`), so the repair never runs
  on them — same as upstream.

### 2.3 Print path: charsets and single shift (#6, #7)

- **Upstream end state.**
  - `print` (`Terminal.zig:1267-1276`) resolves the charset once, after the `status_display` check:
    `key = single_shift orelse gl; single_shift = null; charset = charsets.get(key)`. So a combining
    mark or VS16 also uses up a single shift.
  - `printCell(unmapped_c, wide, set)` (`:1653-1678`) maps through `set`; above 0xFF it now returns
    the codepoint as is (`:1670`, was `' '`), then `@call(.always_inline, writeCell, …)`.
  - `writeCell(c, wide)` (`:1680`) is the old `printCell` body without mapping. Every spacer, blank
    and the grapheme path's re-placement of an already-mapped `prev_cp` (`:1387`, `:1424-1429`,
    `:1453`, `:1618-1633`) use `writeCell`; only `:1592` (narrow) and `:1626` (wide) map.
  - This also fixes a latent bug: the old wide path's `printCell(0, .spacer_head)` consumed the
    single shift before the character it belonged to.
- **Rustty.** `crates/rustty-vt/src/terminal/print.rs`: `print` `:984`, `print_inner` `:1003`;
  `print_cell` `:1589` resolves the single shift (`:1600-1606`) and maps `> u8::MAX` to `' '`
  (`:1612-1615`). Call sites: `:1070` (narrow), grapheme helper `:1277`, `:1360`, `:1369`, `:1394`,
  wide helper `:1568`, `:1570`, `:1576`, `:1578`, `:1583`. The batched fast path already refuses a
  pending single shift (`:318`), so it is unaffected.
- **Port.** Compute the charset at the top of `print_inner` (Rustty's split into
  `print_inner_grapheme` / `print_inner_wide` means passing it into the two narrow/wide mapping
  calls only); add `write_cell`; switch the 9 non-mapping call sites. Keep `#[inline(always)]` on
  both (`@call(.always_inline)`); the charset load stays one `EnumArray` index per call.
- **Tests (3 added, 1 changed):** `Terminal: print wide codepoint in a charset prints it unmapped`,
  `Terminal: print single shift is used up by a combining character`, `Terminal: VS16 moving a
  character to the next line keeps it unmapped`; `Terminal: print charset outside of ASCII`
  (`terminal/tests/print.rs:2183`: `"◆ "` → `"◆😀"`). The comment above `print wide char at right
  edge with hyperlink` (`print.rs:2801`) renames `printCell` → `writeCell` (G3 text).
- **Risks.** Hot path: run the print / `t-parser` benches (memory `comptime-flattening-perf-hazard`:
  use loops-sweep ns/op). Raw `charset` access stays a short-lived place expression through
  `self.screens.active` — no `&mut Screen` held across `print_wrap`.

### 2.4 Clears, selection, reset, input encoders, tmux (#4, #5, #8, #10, #11, #12)

- **`a573781c6` kitty placeholder flag.** Upstream `Screen.zig:1843-1845` and `page.zig:1284-1286`:
  on a full-row clear set `row.kitty_virtual_placeholder = false` unconditionally (the old scan kept
  the flag when the cleared cells held placeholders — backwards). Rustty still scans:
  `screen/clear.rs:286-319` and `page_impl.rs:2453-2470`. Port both (removes a loop; perf-neutral or
  better). Tests: `Terminal: eraseLine complete clears kitty placeholder flag` (after
  `terminal/tests/attrs.rs:511`), `Page clearCells full row clears kitty placeholder flag`
  (`page_tests.rs`). Both skip without kitty graphics.
- **`13b5ab204` selectLine.** Upstream `Screen.zig:3162-3163`: inside the forward whitespace trim,
  `if (end_pin.before(p)) return null;` because `CellIterator` bounds only the row. Rustty
  `screen/select.rs:334-337`. The backward trim cannot overrun once a start was found (it meets
  `start` first), so the one-sided fix is complete. Test: `Screen: selectLine semantic boundary
  around unwritten cells` (after `screen/tests/select.rs:283`).
- **`bb20f8e45` + `2b1edded2` RIS.** End state `Terminal.zig:4983-5026`: `colors.palette.resetAll()`
  after `setCursorStyle(.default)`; flags re-initialised in place, `modes.reset()` moved after it,
  and `.xt_checksum = self.default_xt_checksum` (from #9). Rustty `terminal/state.rs:502-593` lacks
  the palette reset (`DynamicPalette::reset_all` exists at `color.rs:545`). The redraw is covered by
  `dirty.clear` (render state copies the palette on any terminal-level dirty flag,
  `render.zig:619-624`). The reorder is behaviour-neutral; port it for G3 comment text ("Reset our
  basic state", "Remove alternate screen", "Reset primary screen"). No test upstream for the palette.
- **`e8b858cc6` tmux `max_bytes`.** Upstream `tmux/control.zig:137-142`: the limit check moves after
  the state switch, so the terminating `\n` and the next `%` are never rejected. Rustty
  `tmux/control.rs:176-179` checks first. Tests (3): `tmux notification exactly max_bytes`,
  `tmux block terminator exactly max_bytes`, `tmux exceeding max_bytes` (after `control.rs:1487`).
  **UD6** (`UPSTREAM-DEFECTS.md:144`, oversized block leaves the viewer alive) is **not fixed** by
  this commit and `dcs.zig` is unchanged in the range: refresh its reference citations to
  `control.zig:137-142` and `stream_handler.zig:426`. Rustty's UD6 handling (`dcs.rs:204`) is
  unaffected.
- **`e55c687a4` backspace.** Upstream `function_keys.zig:157` adds `{alt, shift, ctrl}` →
  `"\x1b\x08"` (modify_other_keys `.set`). Rustty `crates/rustty-vt/src/input/function_keys.rs`:
  insert `m(true, true, true, false)` after the alt+ctrl entry at `:433-438`, keeping table order.
  Test: `legacy: ctrl+alt+shift+backspace` (after `input/key_encode.rs:3389`). Note (pre-existing,
  out of scope): this table is a `LazyLock` of `String`s (`function_keys.rs:179`), not a `const`.
- **`157c44f64` 1005 mouse.** Upstream `mouse_encode.zig:136`: `printUnicodeCodepoint(32 + button)`.
  Rustty `input/mouse_encode.rs:262-263` writes a raw byte. Port as `char::from_u32(32 + code)`
  (always a scalar value, < 0x100) encoded after the UD60 coordinate validation at `:245-260`, so
  the "no partial `ESC [ M`" property of UD60 holds. Test: `utf8 encodes extended mouse buttons`
  (after `mouse_encode.rs:892`).

### 2.5 DECRQCRA and XTCHECKSUM (#9)

**Upstream end state.**
- `xt_checksum.zig` (NEW, 479 lines): `Flags = packed struct(u5) {positive, no_attributes, no_trim,
  undrawn, full}` (`:38-57`); `Request = extern struct {id, top, left, bottom, right: u16}` (`:62`);
  `Request.selection(pages, origin)` (`:73-98`) clamps 1-based coordinates (0 = default) plus the
  DECOM margin into the screen and returns a **rectangle** `Selection` of active pins, or null;
  `compute(screen, sel, flags)` (`:102-154`) walks `tl.rowIterator(.right_down, br)`, slices
  `row.cells(.all)[min(tl.x,len)..min(br.x+1,len)]`, and sums with wrapping `u16` arithmetic;
  `value` (`:167-189`), `attributes` (`:192-201`), `extended` (`:205-210`);
  `max_encode_size`, `encode` → `DCS Pi ! ~ XXXX ST` (`:213-222`).
- `Terminal.zig`: `default_xt_checksum` field (`:89-91`) and `Options.default_xt_checksum`
  (`:304-305`), `flags.xt_checksum` (`:128-129`, set from the option in `init`), `setDefaultXtChecksum`
  (`:416-419`, also sets the current value), `rectXtChecksum` (`:423-433`, origin from DECOM), and RIS
  restores the default (`:4996`).
- `stream.zig`: actions `request_xt_checksum: xt_checksum.Request`, `xt_checksum_extension:
  XtChecksumExtension{flags}` with `cval` → `u8` (`:135-138`, `:374-382`); CSI `y` dispatch
  (`:2597-2642`): `*` → DECRQCRA (> 6 params rejected; Pg ignored), `#` → XTCHECKSUM (> 1 param
  rejected; `@truncate` to u5).
- `stream_terminal.zig`: `Handler.xt_checksum_report: bool = false` (`:71-76`, a program can read the
  screen back one cell at a time, so it is opt-in); XTCHECKSUM only applies when enabled (`:683-685`);
  `reportXtChecksum` (`:1750-1762`) writes the reply NUL-terminated via `writePty`.
- Cross-tier (not R1): config `vt-xt-checksum-report` (bool, default false) and
  `vt-xt-checksum-extension` (`XtChecksumExtension` packed struct named for what it turns ON; termio's
  `xtChecksumFlags` inverts to the XTCHECKSUM bits); termio `StreamHandler.reportXtChecksum` via
  `write_small`; C option `GHOSTTY_TERMINAL_OPT_XT_CHECKSUM_EXTENSION` (`7e2d1793d`, R2);
  `lib_vt.zig` exports (`25b04b410`, R2); esctest enablement (`edcaf6670`, test infra).

**Rustty today.** Nothing: no `xt_checksum`, `DECRQCRA` or `XTCHECKSUM` in `crates/`, `include/`,
`macos/` or `.port/` records. Insertion points: new `crates/rustty-vt/src/xt_checksum.rs` + `pub mod`
in `lib.rs`; `Flags` `terminal.rs:143`; `Options` `terminal.rs:297`; `init` `terminal.rs:778`;
`set_default_xt_checksum` next to `set_default_cursor_style` (`terminal.rs:956`); `full_reset`
`terminal/state.rs:541-551`; CSI `y` before DECSASD at `stream/csi_dispatch.rs:1842`; two new
`Handler` trait methods in `stream.rs` (no-op defaults, like `insert_blanks` at `:589`);
`Handler.xt_checksum_report` beside `title_report` (`stream_terminal.rs:131`); the reply beside
`report_xtversion` (`stream_terminal/reports.rs:225`). The needed pin APIs exist:
`Pin::row_iterator` (`pagelist/iter.rs:606`), `Pin::cells` (`pagelist/pin.rs:255`), `Pin::style` /
`Pin::grapheme` (`pin.rs:657-664`), `Selection::top_left` / `bottom_right` (`selection.rs:183-204`).

**Port notes.**
- `Flags` must round-trip through `u5`/`u8` (`cval`, the C option, `@bitCast` from params). Use a
  `#[repr(transparent)] u8` newtype or a bool struct with explicit `from_bits(u5)`/`bits()`;
  field order is the bit order above.
- `Request` is `extern struct` (C-visible through the action's C value): `#[repr(C)]`, field order as
  upstream, `Default`.
- `compute` takes `&Screen`; the rectangle is always in the active area, whose pages are never
  compressed, so `Node::page()` never restores. Still read cells through the raw `*mut [Cell]` from
  `Pin::cells` (do not form `&[Cell]` across `Pin::style`), matching the page-access discipline.
- `encode` into a stack buffer of `max_encode_size + 1`; upstream's `catch unreachable` is
  unreachable by construction (max `"\x1bP65535!~FFFF\x1b\\"` = 15 bytes); in Rust use a fixed
  writer and `expect` with a PORT NOTE, or `debug_assert`.
- Checked for defects: `limit`'s `n +| margin` cannot overflow, `cells[left..right]` is always a
  valid slice, origin-mode clamping to the screen (not the margins) matches xterm's
  `extractRectangle`. No UD candidate.

**Tests (17):** `xt_checksum.zig` 12 (`encode`, `selection defaults and clamping`, `selection origin
mode`, `DEC trims blanks and skips undrawn cells`, `DEC counts a leading space`, `empty rectangle`,
`rectangle bounds`, `attributes`, `extended attributes keep a space`, `DEC 8-bit values`, `wide
characters`, `combining marks`); `stream.zig` 2 (`stream: DECRQCRA dispatch`, `stream: XTCHECKSUM
dispatch`; next to `stream/tests/csi_dispatch.rs:93`); `stream_terminal.zig` 3 (`DECRQCRA disabled`,
`DECRQCRA enabled`, `XTCHECKSUM ignored without xt checksum report`; after
`stream_terminal/tests/effects.rs:885`); `Terminal.zig` 1 (`Terminal: default xt checksum survives
resets`, after `terminal/tests/resize.rs:334`).

**Risks.** (1) Security default: `xt_checksum_report` must default to false in the vt handler, the
config and the C surface. (2) Gate bookkeeping: `terminal/xt_checksum.zig` is NEW and absent from the
phase-0 `crate_map` (`.port/phase-0/inventory/tests-summary.json`, checked), so G3b cannot see its
comments until the map is extended. (3) Snapshot does not encode `flags.xt_checksum` (upstream
`snapshot/terminal.zig:392-402` lists the encoded flags; unchanged in range): see §4 Q2.

### 2.6 Memory usage and compressed history restore (#13, #14)

- **`8834e161c` upstream (core halves).** `UntouchedPool.freeCount` (`untouched_pool.zig:111-115`);
  `WasmPagePool.freeCount` always 0 (`wasm_page_pool.zig:116-122`); `PageList.MemoryUsage`
  {pages, virtual_bytes, resident_bytes, compressed_pages, compressed_bytes} and `memoryUsage`
  (`PageList.zig:6888-6931`): `virtual_bytes = page_size + pages.freeCount() * PagePool.item_size`,
  `resident_bytes = memoryStats().estimatedResidentBytes()`; `Screen.MemoryUsage` + `memoryUsage`
  (`Screen.zig:394-416`) adding `kitty_images.total_bytes` (0 without kitty graphics).
- **Rustty.** `memory_stats` / `estimated_resident_bytes` exist (`pagelist/compress.rs:651`, `:361`,
  raw-derived node walk); the pool is the ported `UntouchedPool`
  (`rustty-datastruct/src/untouched_pool.rs`, accessor `free()` at `:253`, no `free_count`);
  `DynamicWasmPagePool` (`wasm_page_pool.rs:290`) has no `free_count`; `PagePool` alias
  `pagelist/mod.rs:876-879`; `PAGE_PREHEAT = 4` (`mod.rs:127`, matches upstream `page_preheat`).
- **SB.** `memory_usage(&self)` only adds a `free.len()` read to an existing read-only walk that
  never restores pages. No new aliasing hazard; it must not call `Node::page()`.
- **Tests:** `PageList memory usage` (next to `pagelist/tests/compress.rs:2668`; it calls
  `page()` on a compressed node and `erase_rows`, so it belongs to the vt-pagelist Miri group);
  `UntouchedPool: create, destroy, reuse` gains three `freeCount` asserts (`untouched_pool.rs:525`).
- **`88e66cbc6` upstream (core halves).** `PageAllocation.FinalizeOptions{compress}` and
  `finalize(location, options)` (`PageList.zig:4430-4466`); `prepend` compresses the new node after
  publishing it when `compress and canReclaim(.strict) and getTopLeft(.viewport).node != node`
  (`:4522-4532`, failure is not an error). `history.DecodeOptions{compress}` threaded through
  `decodePage` / `Decoder.decode` / `decode` (`snapshot/history.zig:196-325`);
  `snapshot.DecodeOptions.compress_history` (`snapshot.zig:106-120`) stored in the decoder's
  per-screen state as `page_options` (`:262-264`, `:403`, `:583`); doc block in `snapshot/main.zig`.
- **Rustty.** `pagelist/builder.rs:119` `finalize(location)`, `:125` `prepend`; callers
  `snapshot/history.rs:467`, `history.rs:551`/`:579`, `snapshot/codec.rs:1192`, plus tests
  `pagelist/tests/builder.rs:221/365/448`, `search/pagelist.rs:964`, `search/screen.rs:3423`;
  `codec::DecodeOptions` (`codec.rs:392`, only `max_continuation_bytes`); decoder state `codec.rs:565`.
  Helpers exist: `compress_page` (`compress.rs:383`), `mem::can_reclaim` (`mem.rs:214`),
  `get_top_left` (`mod.rs:2945`).
- **SB.** `prepend` already works through the raw `destination: *mut PageList`; call
  `(*destination).compress_page(node)` after `assert_integrity`, with no `&mut` alive across it. The
  new node carries no tracked pins. Search objects that later feed a compressed prepended page
  restore it through the existing path (the search-screen SB fix area: run the vt search Miri lane).
- **Tests:** `PageList PageAllocation compresses prepended pages unless visible` (3 cases);
  `HISTORY decode compresses history pages when requested`; signature-only changes in 3 PageList, 2
  search and 3 HISTORY tests (`HISTORY encodes and restores an empty sequence` also asserts zero
  compressed pages with `compress = true`).
- **Platform.** `can_reclaim(.strict)` is false where compression is unsupported, so the test's
  `compressed` expectations hold only on macOS/Linux; check the wasm and Windows (rwin) arms.

### 2.7 Stream / stream_terminal plumbing for new effects (#15–#20)

Rustty's `Stream` dispatches to `Handler` trait methods (`stream.rs:589`ff, no-op defaults) rather
than upstream's `Action` union, so each new action is a new trait method plus a `stream_terminal.rs`
implementation. The OSC parser outputs these depend on (`osc.Command.Unknown`, `ProgramStatus`, the
CAN/SUB cancel in `osc.Parser.end`, OSC 105 routing) are R2's; land R2's parser work first.

- **`7bb45ba34` semantic prompt + reset effects.** Upstream `stream_terminal.zig:226-250` (effects
  `program_status`, `semantic_prompt`, `reset`), `Handler.SemanticPrompt` (`:363-466`; `Kind` =
  `invalid, prompt_start, input_start, output_start, command_end`; `PromptKind` = `primary, right,
  continuation, secondary` — C ABI order, R2 owns the header sentinels), `semanticPrompt` wrapper
  (`:885-937`: applies the command first, reports nothing on error; `output_start` decodes the
  command line into a buffer of `options_unvalidated.len`, empty on failure), `full_reset` arm
  (`:707-714`: progress remove, then program-status `clear`, then `reset`), and
  `main.zig:64 SemanticPrompt` export. Rustty: `Effects` `stream_terminal.rs:252`ff,
  `full_reset` `:993-1016`, `semantic_prompt` `:1447-1448` (`self.vt(…, |t| t.semantic_prompt(v))`
  — the effect needs `v` after the call: borrow it or require `Copy`), `write_command_line` exists
  (`osc/parsers/semantic_prompt.rs:70`). **Name hazard:** `rustty_vt::stream::SemanticPrompt`
  (`stream.rs:397`) is the OSC command alias; the new crate-root `SemanticPrompt` must be the
  stream_terminal event (`lib.rs:307` neighbourhood), so do not glob-reexport. Tests:
  `semantic_prompt effect callback`, `reset effect callback` (after
  `stream_terminal/tests/effects.rs:225`).
- **`7b11f3dca` unknown OSC.** `stream.zig:548-558` `Options.osc_unknown_max_bytes = 0` → parser
  `unknown_max_bytes` (`:574`); action `osc_unknown` (`:2799-2802`, `@branchHint(.unlikely)`);
  `UnknownSequence.osc: osc.Command.Unknown` (`stream_terminal.zig:350-361`) dispatched at `:740-742`.
  Rustty: `stream.rs:906-918` (Options), `UnknownSequence` `stream_terminal.rs:205-207` has only
  `Apc` — adding `Osc` breaks exhaustive `match`es in rustty-vt-c and tests (grep before landing; the
  upstream doc now asks callers to keep an `else` arm, so consider `#[non_exhaustive]` only if the
  C-API reviewer agrees). Tests: `unknown OSC effect callback` (added), `unknown APC effect callback`
  (changed: `osc_count == 0`; `stream_terminal/tests/basics.rs:213`).
- **`bae2c3cdb` program status.** `stream.zig:2794-2796` action; `stream_terminal.zig:944-966`
  (`programStatus`: report → effect; query `OSC 7501;?` replied with fixed bytes only when the effect
  is set). Test: `program_status effect callback`.
- **`b1bfd1d9c` empty OSC 22.** `stream.zig:2735-2738` dispatches `mouse_shape_reset` for an empty
  value; stream_terminal sets `.text` (`:720`); termio resolves `.default` while mouse tracking
  else `.text` (documented in the commit, not a defect). Rustty `stream/dispatch.rs:186-203` (logs
  "unknown cursor shape" today), `stream_terminal.rs:1392`; termio half in
  `crates/rustty-termio/src/stream_handler/osc_handlers.rs`. Test: in `c/terminal.zig` (R2).
- **`520d8f55a` / `36953bca8` tests.** `window_title not changed by cancelled OSC` (stream_terminal;
  exercises Rustty's slice fast path and byte path — the parser already passes the cancelling byte as
  the terminator, `parser.rs:747-756`) and `osc: 105 empty` (`parser.rs`, after `:1571`). Both go
  green only with R2's `osc.rs` changes.

### 2.8 Prompt click relative row (#21, `5170b06f6`)

- Upstream `Surface.zig:4322-4325`, helper `promptClickRelativeRow` (`:4371-4379`):
  `pointFromPin(.screen, click).y - pointFromPin(.screen, prompt).y + 1`. The subtraction cannot
  underflow: `maybePromptClick` returns early when `click_pin.before(prompt_pin)` (`:4320`).
- Rustty `crates/rustty-core/src/surface/mouse_input.rs:1892-1904` still does
  `pos_vp.y.saturating_sub(prompt_pin.y).saturating_add(1)` (viewport row minus page-local row).
- Test: `promptClickRelativeRow` (4 layouts, uses `PageList` directly; rustty-core surface tests).

### 2.9 Header doc (#22, `9a4787d90`)

`include/rustty/vt/terminal.h:2177` still says "If the dimensions are unchanged, this is a no-op."
Upstream now says the grid is left as is but pixel geometry, synchronized-output reset and size
reports still apply. Behaviour already matches (`terminal/resize.rs:115-133`). Text-only; coordinate
with R2 since it is a public header.

### 2.10 UD candidates and records

- **No strong UD candidate in R1's scope.** Checked specifically: the DECRQCRA arithmetic and
  encoding (§2.5), `promptClickRelativeRow` underflow (§2.8), the tmux limit move (§2.4), the
  `semanticPrompt` command-line buffer (decoding never lengthens; failure degrades to ""), and the
  pending-wrap `cursorRight(1)` (guarded).
- **`2b1edded2` Zig result-location aliasing** (`self.flags = .{ .visible = self.flags.visible, … }`):
  each field reads only itself, and upstream's own `Terminal: resize without scrollback pull`
  (`Terminal.zig:16106`, `resize_pull_scrollback = false` survives `fullReset`) would fail if default
  fields were stored first. Not a defect; Rust has no such hazard.
- **Weak candidate (intent gap, not a crash):** a snapshot round trip drops a program-selected
  XTCHECKSUM variant (§4 Q2).
- **Rustty-carried, unrecorded:** #1 (fixed upstream in range; record as an N entry, not a UD).
- **Records to update:** UD6 citations (§2.4); new N for #1; `.port/phase-0` `crate_map` for
  `xt_checksum.zig`.

## 3. Proposed workstreams, ordering and effort

At most 3 building worktrees at once (memory `parallel-worktree-disk-budget`).

| WS | commits, in order | depends on | effort |
|---|---|---|---|
| **R1-A resize** | `e6db5b633` → `afded91df` → `f9ab34f10` (12 Screen test edits) | none; do first (removes a carried debug panic) | 1–1.5 d |
| **R1-B print** | `6220a3617` → `c706451fe` (+ print / `t-parser` bench vs zigbench at `a60e9e2a5`) | none | 0.5–1 d |
| **R1-C small fixes** | `a573781c6`, `13b5ab204`, `bb20f8e45` + `2b1edded2`, `e8b858cc6` (+ UD6 citation refresh), `e55c687a4`, `157c44f64`, `5170b06f6`, `9a4787d90` | `2b1edded2`'s `.xt_checksum` line waits for R1-D (or add it there) | 1–1.5 d |
| **R1-D DECRQCRA** | `9272a2f7c` → `5f0d4febc` → `b2f557c6c` → `cc0276778` → `01b9f4958` as one end-state task; `00a19db51` terminal half; then config + termio halves and the C option `7e2d1793d` (other owners) | R1-C's `fullReset` reorder; C option after R2's header work | 2–3 d (R1 part 1.5–2 d) |
| **R1-E memory** | `8834e161c` core → `88e66cbc6` core (+ snapshot `DecodeOptions`) | none; C halves (R2) after | 1–1.5 d |
| **R1-F stream effects** | `7b11f3dca` → `b1bfd1d9c` → `7bb45ba34` → `bae2c3cdb` stream/stream_terminal halves; tests of `520d8f55a`, `36953bca8` | R2's OSC parser (`Unknown`, `ProgramStatus`, CAN/SUB, OSC 105) | 1.5–2 d |
| **R1-G verification** | vt Miri groups (`vt-pagelist`, `vt-screen`, search lane for #14), G3b with the extended `crate_map`, G1b counts, `audit_test_coverage.py`, print bench fence, Linux (rvm) lane for #14's `can_reclaim` arms, rwin smoke for #14 | all | 1–1.5 d |

**Total: about 9–12 dev-days** (R1's own share about 8–10). R1-A, R1-B, R1-E can run in parallel;
R1-C then R1-D; R1-F after R2's OSC work. Wall-clock about 5–6 working days.

## 4. Open questions, with recommended answers

1. **`e6db5b633` record.** Rustty carries a reachable debug-build panic (release: silent stale write
   inside the row stride) that the parity closure did not record. *Recommendation:* port the fix in
   R1-A and file an N entry (as Phase 7 did for the carried search `unreachable!()` and tick UAF); no
   UD, since upstream fixed it in range and a faithful port removes it.
2. **XTCHECKSUM lost across a snapshot.** `flags.xt_checksum` is program state but the TERMINAL
   record does not carry it, so a restored terminal answers DECRQCRA with the default variant.
   *Recommendation:* keep the wire format byte-identical with Ghostty (the snapshot is a documented
   binary format and Rustty's goldens are cross-checked against it) and file an N entry, not a UD:
   it is opt-in (`xt_checksum_report` off by default), not a crash, and fixing it means a format
   change upstream should make. Revisit if upstream extends the TERMINAL record.
3. **New `Handler` trait methods vs exhaustive `UnknownSequence`.** *Recommendation:* add
   `request_xt_checksum`, `xt_checksum_extension`, `osc_unknown`, `mouse_shape_reset`,
   `program_status` as trait methods with no-op defaults (the existing pattern), and add the `Osc`
   variant to `UnknownSequence` without `#[non_exhaustive]` (matches upstream's closed union; fix
   the internal matches in the same change).
4. **Ownership of the DECRQCRA cross-tier halves** (`Config.zig` keys and `XtChecksumExtension`,
   `Termio.zig` `xtChecksumFlags`, `stream_handler.zig` `reportXtChecksum`, the C option).
   *Recommendation:* R1 lands the vt core and `stream_terminal` first; the config/termio owner and R2
   (C option, header, `lib_vt` exports) follow in one workstream so `default_xt_checksum` /
   `set_default_xt_checksum` have exactly one caller shape. All three defaults stay off.
5. **`Flags` representation.** *Recommendation:* a `#[repr(transparent)] u8` with const accessors
   (or a 5-bool struct with `from_bits`/`bits`) so `@bitCast(@as(u5, @truncate(bits)))`, `cval` and
   the C option round-trip exactly; avoid a `HashMap`/`OnceLock` anywhere on the CSI path.
6. **`SemanticPrompt` naming.** The crate already exports `stream::SemanticPrompt` (the OSC 133
   command). *Recommendation:* keep the module paths as upstream (`stream_terminal::SemanticPrompt`
   for the event) and make the crate-root `SemanticPrompt` the event, as `main.zig:64` does; leave
   the `stream::SemanticPrompt` alias where it is.
