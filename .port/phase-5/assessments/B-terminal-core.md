# Phase 5 — Area B re-vetting assessment: terminal core

Oracle range: `12967b68f` (Phase-4 pin, 2026-08-17) → `683d8db64` (= origin/main, 2026-08-25).
Oracle working tree is checked out AT the new pin; all "head" reads are direct file reads.
Rustty side: `main` @ `8120dd10` (Phase-4 finale `d63dd9fc` absorbed; both gates were GREEN
at the pin minutes before the advance).
Author: area-B agent. Date: 2026-08-25.

Area-B file set (task brief): `src/terminal` **excluding `src/terminal/kitty/` (area A)**,
`src/Surface.zig`, `src/simd`. This includes `src/terminal/c/`, `src/terminal/osc/`,
`src/terminal/snapshot/`, `src/terminal/compress/`, and the top-level `kitty.zig`
re-export hub. One file OUTSIDE the literal path filter is assessed here because the
brief assigns its G1b drop to this area: `src/input/paste.zig` (input_paste drop=7 —
its only range commit, `a5bb22e23`, is a member of this area's paste cluster).

Rustty targets: `crates/rustty-vt/src/`, `crates/rustty-simd/src/`,
`crates/rustty-vt-c/src/`, `crates/rustty-core/src/surface.rs`.

---

## 1. Summary

**63 distinct non-merge commits** touch this file set
(`git log --oneline --no-merges 12967b68f..683d8db64 -- src/terminal src/Surface.zig src/simd ':!src/terminal/kitty'`).
Verdicts:

| verdict | count |
|---|---:|
| **ABSENT** | **59** |
| PARTIAL | 0 |
| PORTED | 0 |
| N-A | 4 (`b56c6d88f` reverted-in-range by `b7ee3ab6b`; `b7ee3ab6b` the revert itself; `c5f4f00ed` Zig-Valgrind tooling; `322d7ae78` my-slice is Zig build-graph reshuffle) |

**Nothing from this range is ported** — the expected pre-port result, but measured, not
assumed: rustty already contains files that LOOK like range work
(`osc/parsers/kitty_clipboard_protocol.rs`, `osc/parsers/kitty_dnd_protocol.rs`,
`stream.rs` KittyClipboard mentions, `surface.rs` clipboard machinery, `vt-c/paste.rs`)
— every one of these was verified to be the **pin state** (§1.1). Changed-line mass in
my paths: **+9,405 / −1,343** (range `--stat`, kitty/ excluded); dominated by
`stream_terminal.zig` +4,277 gross, `osc/parsers/kitty_desktop_notification.zig` +1,201
(NEW), `c/terminal.zig` ~+800, `Surface.zig` ~+594, `Terminal.zig` +580, `Screen.zig` +503.

### 1.1 Coincidence probes (why 0 PORTED is a measured result)

- `stream.rs:3469–3470` lists `osc::Command::KittyClipboardProtocol(_) | KittyDndProtocol(_)`
  in the **ignored/unhandled** arm — exactly the pin's `stream.zig:2564–2565`. The head's
  `vt(.kitty_clipboard, v)` / `vt(.kitty_dnd, v)` dispatch (head `stream.zig:2642–2647`)
  does not exist in rustty.
- `osc/parsers/kitty_clipboard_protocol.rs` and `kitty_dnd_protocol.rs` are pin ports:
  dnd has the pin's 11 `#[test]`s (head shrinks the file to 5, moving the drop state
  machine into area A's `kitty/dnd*.zig`); no `kitty_metadata` import anywhere
  (`grep -rn kitty_metadata crates/` → 0).
- `modes.rs` has `BracketedPaste = 2004` but no 5522 / `kitty_paste_events`
  (`grep 5522 crates/rustty-vt/src/modes.rs` → 0).
- `stream_terminal.rs` has **zero** matches for `kitty_clipboard|KittyClipboard|paste`
  handler code (only the pin-state size-report/mode-2048 resize tests at 2841/2868).
- `vt-c/paste.rs` header: "PORTS: ghostty src/terminal/c/paste.zig", 11 `#[test]`s =
  the **pin's** c/paste.zig count (head has 15).
- `surface.rs` (14,410 lines) has the pin's clipboard config/message arms only; none of
  the head's `kittyClipboardRead/Write/Status`, `completeKittyClipboardRead`,
  `completeClipboardPasteEvent`, `denyClipboardRequest`, `CompleteClipboard` symbols.
- `rustty-simd/src/base64.rs` has exactly the pin API (`max_len`, `decode`, 2 tests);
  no `decode_strict`/`Streaming`/`feed` (grep → 0).
- `screen.rs:1508–1546` `cursor_reload` is byte-for-byte the PRE-`9313d580c` shape
  (direct pin write + trailing `manual_style_update` fallback).
- `parse_table.rs` has no 0x80–0xFF override rows for `DcsPassthrough`/`DcsIgnore`
  and only 1 `#[test]`.

### 1.2 The high-severity subset (read first)

Live bugs / live UB in rustty TODAY, all in already-ported code:

| commit | rustty site | severity |
|---|---|---|
| `33cda4dc5` reload cell pointers when print grows a page | rustty's print grapheme/wide paths hold raw `*mut Cell` (`prev.cell`) across `append_grapheme`/spacer writes that can GROW (replace) the page; head adds a node+serial identity guard because **pointer comparison is insufficient — pages are pooled and a replacement can reuse the same address**. rustty has no serial guard in print (`grep serial terminal.rs` → only activity_serial + tests) and `print_cell` still asserts on the possibly-stale `cell` | **live SB-UB / UAF-class** (same class as Phase-3 BL-3) |
| `9313d580c` stale cursor style/hyperlink after scroll clear | `screen.rs:1518–1524`: `cursor_reload`'s reset arm writes the pin directly instead of routing through `cursor_change_pin`, so cursor `style_id`/`hyperlink_id` remain IDs of the OLD page — on the new page they are dead or **alias unrelated ref-counted entries** (upstream also adds assertIntegrity refCount checks) | refcount corruption / wrong styles |
| `a8c3ab191` scalar base64 empty-input crash | `rustty-simd/src/base64.rs:85–91` `scalar_input` is the exact pre-fix loop: `input[input.len() - i - 1]` — empty (or all-`=`) input → usize underflow **panic**. Reachable on the scalar lane (Miri, wasm32, SIMD-off builds); head rewrites to `while end > 0 …` + `test "base64 empty input"` | crash (scalar lane) |
| `b31fbc846` fully reset row metadata when recycling row storage | rustty row-recycling sites (`pagelist`, `screen` scroll paths, `terminal` insert/deleteLines) still `clear_cells` only (`grep reset_row crates/rustty-vt/src` → 0): recycled rows retain `wrap`/`semantic_prompt`/other metadata → wrong selectLine joins, stale prompts on erased rows (17 upstream regression tests) | correctness (user-visible) |
| `bed20eb36` tabstop unset toggles | `tabstops.rs:195,201` still `^= MASKS[idx]` — unsetting a clear tabstop SETS it; head uses `&= ~masks[idx]` | correctness |
| `d30a9c424` cursor pin garbage flag on screen reset | rustty `Screen::reset` never clears the cursor pin's `garbage` flag after the page reset marks all tracked pins garbage — pin copies (Kitty placements) are born garbage and reaped | correctness (kitty placements) |

---

## 2. Per-commit verdicts, by cluster

Legend: cost **S** ≤ ~50 lines Rust, **M** ~50–400, **L** > 400 or cross-crate ripple.
"Tests" = new `test` blocks the commit contributes to the END STATE in my files.

### 2.1 Cluster B-1 — Kitty clipboard protocol (OSC 5522) + shared paste core (26 commits, ~5,800 lines)

The range's dominant feature. **The end state, not the commit sequence, is the port
target**: the chain rewrites its own earlier commits repeatedly (paste.zig is created by
`a5bb22e23` then rebuilt by `da27e6c90`; the mode is added disabled by `07c6fc21b`,
renamed by `128ec7cd0`, enabled per-artifact by `876032316`; `bcf44b40e`'s "unhandled"
dispatch is superseded by the full handler). Evidence for ABSENT across the whole
cluster: §1.1 probes (stream_terminal/modes/paste/c-paste/surface/clipboard all
pin-state); `grep -rln kitty_paste\|paste_events\|is_text_mime\|secure_random crates/` → 0.

| commit | what (my-paths slice) | verdict | cost |
|---|---|---|---|
| `f2d4b32be` | expose OSC 5522 parser for stream dispatch (`kitty.zig` +1, parser +6) | ABSENT | S |
| `07c6fc21b` | mode 5522 added, disabled | ABSENT — superseded; port end state of `modes.zig` only | folds |
| `bcf44b40e` | dispatch OSC 5522 as `kitty_clipboard` stream action, unhandled | ABSENT — superseded by the handler chain | folds |
| `128ec7cd0` | rename `paste_events`→`kitty_paste_events`; snapshot codec + `.ksy` + goldens follow | ABSENT — port only the final name; snapshot rider | S |
| `a5bb22e23` | NEW `paste.zig` (+185) shared paste core; `src/input/paste.zig` +166 (**7 new tests, = the whole input_paste drop**) | ABSENT | M |
| `876032316` | stream-handler paste operation (+510 stream_terminal incl. the 14 `paste:` tests); mode enabled for `.lib`/macOS via `.disabled = build_options.artifact != .lib and builtin.os.tag != .macos` (**R8: port as cfg, never fold**) | ABSENT | L |
| `60a1ae2df` | `ghostty_terminal_paste` C API (c/paste.zig +298, c/{main,result,types}) | ABSENT | M |
| `da27e6c90` | paste reads clipboard on demand, **streams to pty**: reworks paste.zig (end state 247 lines, 1 test) + c/paste.zig (end state 15 tests) + `clipboard.zig` `MimeReader` + c/io.zig +19 | ABSENT — end state over `a5bb22e23`/`60a1ae2df` | L |
| `dda8e6f31` | `sys.zig` `random_secure` override fn-ptr + `randomSecure(io,…)` (+1 test) + c/sys.zig +20 | ABSENT — rustty `sys.rs` has only the pin's image-decode fn-ptrs | S/M |
| `a5bb22e23`→`da27e6c90` | (atomicity note: see bundle B-1 below) | | |
| `e03475c0c` | `clipboard_read` effect (c/terminal.zig +252, clipboard.zig `Read`+95, OSC 52 reads); `clipboard_operation.zig` gains **terminator tracking + 1 test** ("OSC 52: get clipboard with BEL terminator" — `grep terminator crates/…/clipboard_operation.rs` → 0) | ABSENT | L |
| `6959fd46c` | Kitty clipboard write only (+662 stream_terminal, c/terminal +142) | ABSENT | L |
| `4f49dc2b8` | Kitty clipboard reads via clipboard_read effect (+460, c/terminal +75) | ABSENT | L |
| `94b6dae42` | gate mode-5522 reports on clipboard read callback (+29) | ABSENT | S |
| `7ae9b1113` | write permission prompts and grants (+277 stream_terminal, clipboard.zig `Write.reply/Status/Result` expansion, c/terminal +149) — rustty `clipboard.rs` still the thin pin `{Location, Content, Write, WriteResult}` | ABSENT | L |
| `928c7f0e7` | exempt listing reads from permission prompts (+58; Surface +7, clipboard +5) | ABSENT | S/M |
| `5350d4a5f` | configurable write limit (c/terminal +82, stream_terminal +52) | ABSENT | M |
| `70f006575` | reject oversized writes (EFBIG) | ABSENT | S |
| `e8d8945b5` | text input validation to spec (+81) | ABSENT | M |
| `bc2f7d7d2` | base64 handling to spec: **strict RFC 4648 + streaming split-chunk decode** — stream_terminal +105, Surface −39; simd side in §2.3 | ABSENT | M (+simd) |
| `1334cc213` | ENOSYS for writes to primary on macOS (+12) | ABSENT | S |
| `5984d6f73` | `clipboard.isTextMime` helper + `test isTextMime` (behavioral named test — gate-blind, §5) | ABSENT | S |
| `25c61e852` | macOS Kitty clipboard writes (Surface +140) | ABSENT | M |
| `0ce9054bf` | macOS Kitty clipboard reads (Surface +169: `kittyClipboardRead/Write/Status`, `completeKittyClipboardRead`) | ABSENT | M/L |
| `8c7a34d4c` | reads serve all clipboard content types (Surface +60) | ABSENT | M |
| `af9470b19` | reads support pw/name session grants (Surface +73) | ABSENT | M |
| `c2c0db68a` | enable mode 5522 paste events on macOS (Surface +94: `completeClipboardPasteEvent`, `denyClipboardRequest`; modes `.disabled` predicate) | ABSENT | M |

**Atomic bundle B-1:** all 26 commits above are ONE feature and must land as the
range-diff end state, carrying every test. Internal supersessions to NOT pass through:
`07c6fc21b`/`bcf44b40e` scaffolding forms, `a5bb22e23`'s original `paste.zig`,
`60a1ae2df`'s pre-`da27e6c90` c/paste shape. **Hard cross-area coupling: head
`stream_terminal.zig:18` and `paste.zig` `@import("kitty/clipboard.zig")` — the state
machines (transactions, grants, passwords) live in area A's NEW `kitty/clipboard*.zig`
×5 (the bulk of G1b kitty drop=202). B-1 cannot land before/without area A's clipboard
subsystem.** `dda8e6f31` (entropy for one-time passwords) and the §2.3 base64 work are
prerequisites inside the bundle.

Stream_terminal end-state tests carried by B-1 (of the 44 new; diffed pin vs head
`^test ` lists): `clipboard_read effect callback`; 21 `kitty clipboard …` (write
transaction round trip / write result maps / ENOSYS / EPERM / read round trip / read
result maps / caps requested types / read password grants / targets-only never consumes
/ write password grants / malformed dropped / new write replaces in-flight / invalid
write EINVAL / invalid read text does not abort write / oversized EFBIG / invalid wdata
EINVAL / wdata chunks split one base64 stream / unpadded wdata aborts at commit /
in-flight freed on deinit / allocation failure ignored / without write_pty still
commits); 14 `paste: …`; `full reset drops kitty clipboard grants`. Plus
`src/input/paste.zig`'s 7, `paste.zig`'s 1, `clipboard.zig`'s 1, `sys.zig`'s 1,
c/paste's +4, `clipboard_operation.zig`'s +1.

### 2.2 Cluster B-2 — OSC 99 Kitty desktop notifications (9 commits, ~1,350 lines)

| commit | what | verdict | evidence | cost |
|---|---|---|---|---|
| `c67a1db99` | NEW `osc/parsers/kitty_desktop_notification.zig` (+1,293 initial) + osc.zig dispatch (`.@"99"`, prefix table gains `"99;"`) | ABSENT | no `kitty_desktop_notification` anywhere in crates/ (grep → 0); `osc/parsers/` listing lacks it | **L** |
| `93bf7d810` | review: −71 | ABSENT — folds into end state | — | folds |
| `202e639d9` | comment cleanup | ABSENT — folds | — | folds |
| `301b69df4` | save OSC 99 terminator for responses (+6) | ABSENT — folds | — | folds |
| `9a8e7ae18` | osc.zig `pub const KittyDesktopNotification = parsers.…OSC` alias | ABSENT — folds | — | folds |
| `aa4ec3508` | review: −27 | ABSENT — folds | — | folds |
| `07f33ad91` | share metadata parsing: creates **transient `osc/lib.zig`** (+61), refactors kdn + kitty_clipboard_protocol onto it | ABSENT — **superseded**: `ca9e5b130` deletes `osc/lib.zig`; never create an `osc/lib.rs` | `git cat-file -e 683d8db64:src/terminal/osc/lib.zig` → gone | folds |
| `073bffcff` | drop inline switch branches (2 lines) | ABSENT — folds | — | folds |
| `ca9e5b130` | end-state shape: NEW `osc/kitty_metadata.zig` (+94, **2 tests**) replacing osc/lib.zig; kdn/clipboard-protocol consume it; c/types.zig +1 | ABSENT | `grep -rln kitty_metadata crates/` → 0 | M |

**Atomic bundle B-2:** port the head files (`kitty_desktop_notification.zig` 1,201
lines / **57 tests**; `kitty_metadata.zig` 94 / 2 tests; osc.zig dispatch+alias) as one
end state. `kitty_metadata` is shared with B-1's `kitty_clipboard_protocol.zig` — land
B-2's metadata module before or with B-1 (or as its own leaf first; it is
self-contained). This bundle is the bulk of G1b **osc drop=47** (§5).

### 2.3 Cluster B-3 — SIMD: OSC bulk reading + strict/streaming base64 (3 commits, ~750 lines)

| commit | what | verdict | evidence | tests | cost |
|---|---|---|---|---|---|
| `8c5bc3d29` | OSC bulk path: `stream.zig` `consumeOscString` (vectorized `< 0x20` scan + bulk store, mirrors the APC path; +263) and `osc.zig` `Parser.nextSlice`/`Capture.writeSlice` with hard cap + truncate-at-limit (+124) | ABSENT | `grep consume_osc_string\|next_slice crates/rustty-vt/src/stream.rs osc/mod.rs` → 0 (stream.rs has only `consume_apc_string:1328`) | 2 stream (`osc 52 large payload in chunks`, `osc bulk path matches per-byte path`) + 3 osc (`nextSlice allocating captures have a hard limit`, `overflowing slice is truncated at the limit`, `nextSlice matches per-byte parsing`) | **M/L** |
| `bc2f7d7d2` (simd side) | `base64.zig` `decodeStrict` + `Streaming{maxLen,feed,finish}` (strict RFC 4648 via simdutf strict mode; arbitrary chunk-split wdata), `base64_scalar.zig` +6 | ABSENT | base64.rs API surface = pin (`max_len`,`decode` only) | 5 (`strict decode valid/invalid`, `streaming decode chunk boundaries`, `streaming decode invalid`, + spec-tightened `decode`) | **M** (FFI: simdutf strict entry points via base64.cpp shim → rustty-simd C++ side + ASan lane per R6) |
| `a8c3ab191` | scalar empty-input crash fix + `test "base64 empty input"` | **ABSENT — live crash, §1.2** | `base64.rs:85–91` pre-fix loop | 1 | **S — do first, independent** |

**Bundle note:** `8c5bc3d29` exists BECAUSE of B-1 (megabyte OSC 5522 payloads) but is
independently portable and testable; its two stream tests + three osc tests are what
G1b will demand. The base64 `Streaming` decoder is a B-1 prerequisite (wdata chunks).

### 2.4 Cluster B-4 — terminal-core correctness fixes (10 commits, ~1,400 lines)

| commit | what | verdict | evidence | tests | cost |
|---|---|---|---|---|---|
| `b31fbc846` | `page.zig` `Page.resetRow` + `Row.reset` (single 8-byte store; asserts `!managedMemory()`); all recycle sites in PageList (eraseRow/eraseRowBounded/eraseActive/split/resize-trim) + Screen scroll paths + Terminal insert/deleteLines switch from `clearCells` to `resetRow`; **snapshot golden `complete-v1.hex` changes** | **ABSENT — §1.2** | `grep reset_row crates/rustty-vt/src` → 0 | **17**: PageList 5 (`eraseRow/eraseRowBounded recycled row has default metadata`, `eraseActive regrown`, `split retired`, `resize trimmed`), Screen 6 (`cursorScrollRegionUp` ×2, `cursorScrollAbove`, `cursorDownScroll` ×2, `selectLine does not join lines across a recycled row`), Terminal 6 (`scroll region linefeed`, `alt screen scroll up`, `insertLines`, `deleteLines count`, `deleteLines blank … semantic prompt`, `eraseDisplay complete ignores stale prompt`) | **M/L** |
| `9313d580c` | `cursorReload` reset arm routes through `cursorChangePin` (style/hyperlink migration; re-reads pin after capacity adjustment; refreshes cached row/cell FIRST because cursorChangePin dirties the old row); `cursorChangePin` zeroes `hyperlink_id` before style ops; assertIntegrity gains style/hyperlink refCount asserts | **ABSENT — §1.2** | `screen.rs:1508–1546` pre-fix; no refCount asserts in rustty assert_integrity cursor block | 2 (`Screen scrollClear across pages migrates cursor style and hyperlink`, `Terminal: scrollClear across pages keeps cursor hyperlink refs page-local`) | **M** |
| `33cda4dc5` | print grapheme `.wide` path: re-read dest cell from cursor after `appendGrapheme` (can grow page); spacer write guarded by **node+serial identity** (pool reuse defeats pointer compare); `printCell` asserts through cursor not the stale `cell` | **ABSENT — §1.2** | no serial guard in `terminal.rs` print | 2 (`VS16 widening when the spacer tail grows the page`, `grapheme transfer when widening wraps to the next line`) | **M** |
| `d30a9c424` | `Screen.reset` clears `cursor_pin.garbage` | **ABSENT — §1.2** | `grep garbage screen.rs` → constructor sites only | 1 (`Screen reset cursor pin is not garbage`) | **S** |
| `bed20eb36` | `Tabstops.unset` `^=` → `&= ~mask` | **ABSENT — §1.2** | `tabstops.rs:195,201` | 0 new blocks (extends `basic` + `dynamic allocations` — port the added assertions; count-invisible to G1b) | **S** |
| `eeab85b96` | full reset clears progress bar (`.full_reset` arm calls `progressReport(.{.state=.remove})`) | ABSENT | `stream_terminal.rs` full_reset arm bare; progress_report exists at 2511 | 0 new blocks (extends `progress_report effect callback` — port the RIS assertions) | **S** |
| `b56c6d88f` | only clear progress bar if there is one | **N-A** — reverted in-range by `b7ee3ab6b`; end state = `eeab85b96` alone | revert hash `b7ee3ab6b` | — | — |
| `b7ee3ab6b` | Revert of `b56c6d88f` | **N-A** (the revert) | — | — | — |
| `07af4612b` | `reportMode2048` on mode-2048 ENABLE (late-enabling clients get geometry immediately); size-effect doc updated; c/terminal C API test side | ABSENT | `stream_terminal.rs:905` `.in_band_size_reports` arm commented/no-op; no `report_mode_2048` | 2 (`mode 2048 enable reports current geometry and disable is silent`, `… tolerates missing effects`) | **S/M** |
| `4e817e79a` | parse_table: 0x80–0xFF are payload in `dcs_passthrough` (`.put`) and `dcs_ignore` (`.ignore`), overriding "anywhere" C1 transitions (8-bit ST 0x9C deliberately included — UTF-8 "Ü" is 0xC3 0x9C; rationale comments must ride, R1) | ABSENT | `parse_table.rs` no high-byte rows; 1 `#[test]` vs head 4 | 3 (`dcs_passthrough: high bytes are payload data`, `dcs_ignore: …`, + one more table test) — **this is G1b parser drop=1** (2 masked by rustty's +2 surplus) | **S** (comptime table — R8: regenerate, verify bit-for-bit) |
| `f1948d505` | `selectionString(opts.map)` → new `selectionStringMap` API returning owned StringMap; deletes the `selectionString map allocation failure cleanup` test + its tripwire; StringMap.zig −11; Surface `linkAtPin` switches | ABSENT | `screen.rs:468` still `pub map: Option<&'a mut StringMap>`; rustty still HAS the deleted test (`screen.rs:12628`) — **must be removed with the port** (R1: the oracle deleted it) | net −1 test | **S/M** |

Also in this cluster's files but N-A:

| commit | reason |
|---|---|
| `c5f4f00ed` fix up recent Valgrind errors | **N-A.** All three my-path hunks are Zig-tooling-specific: Screen/compress hunks are `std.valgrind.memcheck.makeMemDefined` padding annotations (rustty's memory gate is Miri/ASan per R6 — no Valgrind lane, no `std.valgrind` analogue); the stream_terminal hunk fixes 4 tests retaining the callee's pty slice past the call — rustty's capture helper already copies (`stream_terminal.rs:4592` `cap_write_pty` does `data.to_vec()`), so the dangling-slice hazard does not exist by construction. |

### 2.5 Cluster B-5 — Surface perf idioms (2 commits)

| commit | what | verdict | evidence | cost |
|---|---|---|---|---|
| `a9f7f6d21` | `parseTextBinding` with `std.heap.stackFallback` | ABSENT | rustty surface.rs treats stackFallback as documented plain-alloc + PORT NOTE (precedent at `surface.rs:3339–3347`); port the end state the same way with `PERF(port): stackFallback` per R10 | S |
| `5a8921ecb` | clipboard content list kept on stack | ABSENT | same treatment | S |

### 2.6 Cluster B-6 — kitty-subsystem riders in my paths (bodies owned by area A; 12 commits)

These commits' behavioral centers are in `src/terminal/kitty/` (area A); what falls in
MY files is the wiring/tests below. Verdicts are for my slice only; each is a
**cross-area atomic member** of the corresponding area-A bundle.

| commit | my-paths slice | verdict | tests (mine) | cost |
|---|---|---|---|---|
| `7c845e8af` dnd command decoding | `kitty.zig` +1 re-export | ABSENT | 0 | S |
| `50f69b883` dnd drop state machine | `osc/parsers/kitty_dnd_protocol.zig` **shrinks** 363→~91 lines, 11→5 tests (state machine + 6 tests MOVE to area A's `kitty/dnd*.zig`) — rustty file still the fat pin shape (11 `#[test]`s) | ABSENT (end-state rewrite) | −6 here, +N in area A — **coordinate so the moved tests land in A, not vanish** | M |
| `af8d28a94` dnd stream handler | stream_terminal +253 (handler + effect), stream.zig `kitty_dnd` vt action, Terminal +10, c/terminal +1 | ABSENT | 5 (`kitty dnd: query response / register, drop, and serve data / state updates work without write_pty / registration survives terminal reset / effect reports …`) | M — needs area A `kitty/dnd.zig` |
| `90bce0d2d` scroll/clip placements within margins | Terminal.zig +134 (`indexScrollWithImages`, `kittyScrollMarginsBegin` glue + 3 tests) | ABSENT | 3 (`storage: scroll margins placement …` ×3) | M — needs area A storage API |
| `bcbc93a6b` preserve image limits across full reset | Terminal fullReset preserves `image_limits`/`total_limit`; Screen +8 | ABSENT | 1 (`Terminal: fullReset preserves kitty graphics limits`) | S |
| `f8b40a023` graphics deletion mismatches | Terminal.zig ±12 | ABSENT | 0 (mine) | S |
| `4be9d782e` avoid Kitty image reset test if no images | Terminal.zig +2 guard | ABSENT | 0 | S |
| `73903f76a` c: image data returns current animation frame | c/kitty_graphics.zig +5 | ABSENT | 0 | S — needs area A animation storage |
| `f5b3efe45` c: resolve relative placement viewport positions | c/kitty_graphics.zig +44 | ABSENT | 0 | M — needs area A relative placements |
| `cd5f9eef0` relative placement storage | c/kitty_graphics.zig 2-line rider | ABSENT | 0 | S |
| `52190a5d8` intersect source rectangles before sizing | c/kitty_graphics.zig ±19 | ABSENT | 0 | S |
| `322d7ae78` switch to wuffs for pixel work | my slice = `build_options.zig` extracts `pub fn kittyGraphics(Options, target)` (wasm-freestanding force-disable moved into a helper for build.zig/wuffs wiring) | **N-A** (my slice) — Zig build-graph reshuffle with no behavioral delta; rustty `build_options.rs` already encodes the same wasm force-disable as its documented WASM DIVERGENCE + cargo-feature wiring. **Re-check when area A ports the wuffs pixel switch** (pkg/wuffs shim headers are another area) | 0 | — |

---

## 3. Atomic bundles (summary)

| bundle | members | why atomic |
|---|---|---|
| B-1 Kitty clipboard 5522 + paste | the 26 commits of §2.1 (+ `a8c3ab191`/`bc2f7d7d2` simd prerequisites) | one feature built by successive rewrites; **requires area A `kitty/clipboard*.zig`**; C API + Surface + snapshot-mode riders must land with the core or the mode/effects dangle |
| B-2 OSC 99 | §2.2's 9 commits | end-state files only; transient `osc/lib.zig` must never exist; `kitty_metadata` shared with B-1's parser |
| B-3 SIMD OSC/base64 | `8c5bc3d29` + `bc2f7d7d2`(simd) + `a8c3ab191` | `Streaming` decoder is B-1's wdata engine; `a8c3ab191` independent-first |
| B-4 row-metadata recycle | `b31fbc846` alone (17 tests + golden) | one semantic (`resetRow`) applied across 3 files + golden regen |
| B-5 dnd wiring | `7c845e8af`+`50f69b883`+`af8d28a94` **with area A's `kitty/dnd*.zig`** | protocol file shrinks as the state machine moves to A; 6 tests migrate across the area boundary |
| B-6 cursor-safety pair | `9313d580c`+`33cda4dc5` (+`d30a9c424`) | same cursor/page-pointer invariants; each S/M and independently landable but share test infrastructure |

## 4. UB/SB hazards (PORT_RULES seams)

1. **`33cda4dc5` — the commit IS an SB lesson.** Upstream documents that a page-pool
   replacement can reuse the same address, so identity = (node ptr, serial). rustty's
   print already holds raw `*mut Cell` for `prev.cell`; the port must (a) keep all
   post-grow reads through `cursor.page_cell` (re-derived), (b) implement the serial
   guard on copied scalars, never a held `&Page`. Until ported, rustty's print has a
   genuine stale-raw-pointer deref under page growth — a Miri target: extend the vt Miri
   sweep with the two new tests (per `search-screen-sb-ub-finding` precedent).
2. **`9313d580c`** — `cursor_change_pin` releases the OLD page's hyperlink ref then
   zeroes `hyperlink_id` BEFORE style operations that may run integrity checks; the
   reset arm must refresh `page_row`/`page_cell` from the pin FIRST (the old cached
   pointers may already be dangling — that is often why cursor_reload was called).
   rustty's `cursor.page_pin` is already a raw ptr (`screen.rs:1515` unsafe deref);
   keep the whole migration on raw-ptr discipline, re-deriving after
   `cursor_change_pin`'s possible capacity adjustment.
3. **`b31fbc846` `Row.reset`** asserts `!managedMemory()` — the cell-clearing side must
   release grapheme/style/hyperlink memory while the flags are still accurate; order is
   part of the contract (reset does not touch cell memory). Rustty rows are reached via
   raw `*mut Row` in the recycle paths — no `&mut Row` may live across the paired
   `clear_cells` call into the same page.
4. **B-1 callback plumbing** — `clipboard.MimeReader.read_fn`/`Write.reply_fn`/
   `Read.reply_fn` are `*anyopaque`-context C fn pointers (R9 origin `callback-ctx`);
   `paste.Request.contents` is documented "borrowed only during the paste call" — no
   Rust lifetime may outlive the call; the on-demand pty streaming (`da27e6c90`,
   c/io.zig) hands chunks to the writer while the reader owns the source. DG5: every
   new exported fn (`ghostty_terminal_paste`, effects, sys random override) is plain
   `extern "C"`; by-value C structs (paste event payloads in c/types.zig) must
   byte-match the header per the embed-abi-byvalue-sret gate.
5. **Base64 `Streaming`** — `feed` decodes into caller output across chunk boundaries
   holding carry state; the scalar-lane fix (`a8c3ab191`) replaces an underflowing
   reverse scan — R7: the Rust port must use the `while end > 0` form, not a
   `len - i - 1` index.
6. **`8c5bc3d29` `consumeOscString`** — mirrors `consume_apc_string`'s existing
   vector-scan + bulk-store aliasing discipline (stream.rs precedent); the osc
   `Capture.writeSlice` grows a bounded buffer — truncate-at-limit is release-checked
   validation (R7: never a debug_assert).
7. **`d30a9c424`** — tracked-pin `garbage` flag semantics: the cursor pin is exempted
   from the reset's garbage-marking; pin copies (kitty placements) inherit the flag —
   double-check rustty's tracked-pin reap loop when porting.
8. **`f1948d505`** — PinMap ownership moves into the returned `StringMap` (Phase-4 A-4
   landed the PinMap); the refactor must keep exactly one untrack per tracked pin on
   both the success and error paths (`errdefer pins.deinit`).
9. **R8 switches:** `876032316`/`c2c0db68a`'s `.disabled = build_options.artifact !=
   .lib and builtin.os.tag != .macos` on `kitty_paste_events` is a build/target switch —
   port as `cfg`/runtime build-options field, never folded to one profile;
   `4e817e79a`'s table rows are comptime-generated — regenerate `parse_table.rs`
   bit-for-bit (R8 derived constants).

## 5. Dropped-test targets (G1b `w0/g1b-after-at-683d8db64.txt`) belonging to area B

- **`osc` RED drop=47.** Real missing: **61** = kdn 57 + osc.zig 3 (`Parser nextSlice
  …` ×3, from `8c5bc3d29`) + clipboard_operation 1 (`OSC 52: get clipboard with BEL
  terminator`, from `e03475c0c`); 6 dnd tests LEAVE this module (move to area A's
  kitty/dnd files with `50f69b883`); rustty's 8-test surplus masks the rest → 47.
- **`stream_terminal` RED drop=38.** Real missing: **44** (full list in §2.1 + §2.4/§2.6:
  22 kitty-clipboard-side incl. `clipboard_read effect callback`, 14 `paste:`, 2
  `mode 2048 …`, 5 `kitty dnd:`, 1 `full reset drops kitty clipboard grants`);
  6-test surplus masks to 38.
- **`input_paste` RED drop=7.** Exactly `a5bb22e23`'s 7 new tests in
  `src/input/paste.zig` (10→17); rustty `input/paste.rs` = 10.
- **`parser` RED drop=1.** `4e817e79a`'s 3 new parse_table tests; +2 surplus → 1.
- **GATE BLIND SPOTS (the finale G1b CANNOT certify these — extend the gate or verify
  by checklist):** `check_test_counts.py` maps **neither** `src/terminal/paste.zig`
  (1 test) **nor** `src/terminal/clipboard.zig` (1: `test isTextMime`) **nor**
  `src/terminal/sys.zig` (1: `randomSecure: override is preferred over the Io`) **nor**
  `osc/kitty_metadata.zig` (2 — the osc zig-side glob is `osc/parsers/*.zig` + 3 named
  files; kitty_metadata sits in `osc/` unmatched) **nor** any `src/simd/*.zig`
  (base64 2→7: 5 new tests incl. the `a8c3ab191` crash regression) **nor**
  `src/terminal/snapshot/*`. Recommend: add mappings in the same change that ports
  each file (per the "derived-gate green can be bookkeeping" doctrine).
- **Masked-green warnings (surplus hides real drops — use §2's per-commit lists as the
  checklist):** `terminal` 422/436 ok but **10 new upstream tests all missing** (14
  surplus fully masks); `screen` 216/218 ok but 8 new missing (+1 obsolete test rustty
  must DELETE with `f1948d505`); `pagelist` 288/316 ok but 5 missing; `stream` 61/67 ok
  but 2 missing (`osc 52 large payload in chunks`, `osc bulk path matches per-byte
  path`); `vt_c` 416/461 ok but c/paste +4, c/terminal +N, c/sys/c/io additions all
  missing under a 45-test surplus; `tabstops`/`modes` count-unchanged but
  assertion-level changes (`bed20eb36`, mode entry) are count-invisible.

## 6. Coverage honesty

**Exhaustive enumeration:** all 63 non-merge commits from the stated git-log filter are
verdicted above (26 + 9 + 3 + 12 + 2 + 12 across clusters, minus shared members;
every hash appears exactly once as a table row or N-A row). The `src/input/paste.zig`
add-on was enumerated by its own log (1 commit, `a5bb22e23`).

**Read line-by-line (complete relevant hunks, oracle + corresponding rustty code):**
`4e817e79a`, `a8c3ab191` (+ rustty `base64.rs` in full), `bed20eb36`, `d30a9c424`,
`eeab85b96`, `b56c6d88f`/`b7ee3ab6b` (revert pair verified by subject + stat),
`c5f4f00ed` (all three my-path hunks + rustty `cap_write_pty`), `f1948d505`,
`9313d580c` (Screen hunk in full + rustty `cursor_reload` in full), `33cda4dc5`
(Terminal hunk + test preamble), `b31fbc846` (page.zig hunk in full; PageList
call-site conversions; test-name inventory), `bcbc93a6b`, `07af4612b`
(stream_terminal hunk), `322d7ae78` (build_options hunk in full), `dda8e6f31`
(sys.zig symbol surface), `5984d6f73`/`e03475c0c` (clipboard/clipboard_operation
deltas), `128ec7cd0` (stat-level + modes end state read), modes.zig end state,
head `paste.zig` (first 60 lines + full symbol/test inventory).

**Sampled / symbol-level only (stated so it is not mistaken for line coverage):**

- **B-1's stream_terminal/c-terminal/Surface bodies** (~4,000 lines): verified ABSENT
  by symbol probes (`kitty_clipboard`, `paste`, 5522, `kittyClipboardRead`,
  `write_limit`, `MimeReader`, `random_secure` — all 0 in rustty), by the head import
  list (`stream_terminal.zig:18–21`), by full new-test-name inventories (pin-vs-head
  `^test ` diffs), and by per-commit `--stat`. Interior logic NOT read line-by-line —
  porting must go from the head files regardless.
- **B-2's kdn parser body** (1,201 lines / 57 tests): commit-message chain + refactor
  stat trail + end-state existence checks; interiors unread.
- **`8c5bc3d29`/`bc2f7d7d2`** read at added-symbol + test-name level plus benchmark
  tables in the messages; vector-loop interiors unread.
- **`90bce0d2d`/`f5b3efe45`/`52190a5d8`/`f8b40a023`** (kitty riders): stat + added-fn
  inventory only; their area-A cores are out of scope here.
- Filter: every "rustty is pin-state" claim cites a concrete rustty file:line read
  (§1.1, §2), except pure new-symbol absences (kitty_metadata,
  kitty_desktop_notification, reset_row, consume_osc_string, next_slice,
  decode_strict/Streaming, random_secure, is_text_mime, report_mode_2048, serial guard,
  paste handler) where `grep -rn` over `crates/` is the evidence.
- I did NOT vet the non-my-path halves of shared PRs (kitty/clipboard*/dnd* bodies =
  area A; include/ headers, macos Swift, example/, lib/ = other areas) beyond `--stat`
  ownership confirmation.

## 7. Cross-area couplings (name the owner)

| change | also lands in | owner |
|---|---|---|
| B-1 core (`stream_terminal` 5522 handler, `paste.zig`) | `kitty/clipboard*.zig` ×5 state machines (G1b kitty drop=202 bulk) | **area A (blocking)** |
| B-5 dnd handler + protocol shrink | `kitty/dnd*.zig` ×5 (+ 6 tests migrating out of my `kitty_dnd_protocol.zig`) | **area A (blocking)** |
| B-1 C API (`c/paste.zig`, `c/terminal.zig`, `c/io.zig`, `c/sys.zig`, `c/types.zig`, `c/result.zig`) | `include/ghostty/vt/*.h`, `example/` paste-events, `src/lib/` | area C-analog (headers/examples); the `c/` zig files themselves are MINE (rustty-vt-c) |
| B-1 Surface arms (`kittyClipboardRead/Write/Status`, paste events, grants) | apprt clipboard content types, macos Swift app | apprt/macos area; `surface.rs` arms are MINE (rustty-core) |
| `dda8e6f31` entropy override | consumed by area A's `kitty/clipboard.zig` password generation | mine (sys) / A (consumer) |
| `128ec7cd0` + `876032316` mode in snapshots | `snapshot/terminal.zig`, `snapshot.ksy`, testdata | mine (snapshot is in my file set); .ksy spec doc rider |
| `b31fbc846` golden change (`complete-v1.hex`) | G6c differential/golden harness — **regenerate goldens with the port; `cargo clean` first** (memory: rename-casing-fidelity) | mine |
| `322d7ae78` wuffs switch | `pkg/wuffs` shim headers, kitty pixel work | area A / pkg area |
| kitty graphics riders (`90bce0d2d`, `bcbc93a6b`, `f8b40a023`, `73903f76a`, `f5b3efe45`, `cd5f9eef0`, `52190a5d8`, `4be9d782e`) | `kitty/graphics_*.zig` bodies | **area A (blocking)** |
| `c2c0db68a`/`876032316` mode `.disabled` predicate | build_options artifact/target plumbing | mine, R8 cfg |

## 8. Recommended landing order (area B)

1. **Wave 0 — live-bug hotfixes, no cross-area block:** `a8c3ab191`, `bed20eb36`,
   `d30a9c424`, `9313d580c`, `33cda4dc5` (with Miri proof on the print-growth path),
   `eeab85b96`, `4e817e79a`, `07af4612b`, `f1948d505` (+ delete the obsolete rustty
   test), `b31fbc846` (+ golden regen).
2. **Wave 1 — B-3** (`8c5bc3d29`, `bc2f7d7d2` simd side): unblocks B-1's payload
   handling; independently green-able.
3. **Wave 2 — B-2 OSC 99** (kdn + kitty_metadata): self-contained; lands the
   metadata module B-1's parser refactor needs.
4. **Wave 3 — B-1** jointly with **area A's kitty/clipboard subsystem**; includes
   `dda8e6f31`, the c/ API files, `modes`+snapshot riders, Surface arms,
   `a9f7f6d21`/`5a8921ecb`.
5. **Wave 4 — B-5 dnd** jointly with area A's kitty/dnd; then the kitty graphics
   riders as area A lands each subsystem (animation, relative placements, margins).
6. **Gate work (any wave, before finale):** extend `check_test_counts.py` for the six
   blind spots in §5.
