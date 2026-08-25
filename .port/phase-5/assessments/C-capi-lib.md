# Phase 5 assessment — area C: the C API / headers / lib tier (`include/**`, `src/lib/**`, `src/lib_vt.zig`, `src/terminal/c/**`, `example/**`, `src/build/**`, `src/global.zig`)

Range: `12967b68f` (Phase-4 pin, 2026-08-17) → `683d8db64` (`origin/main` = the NEW pin, 2026-08-25).
Oracle working tree is checked out AT the new pin, so file reads are target-state reads.
Rustty side: `crates/rustty-vt-c/`, `crates/rustty-lib/`, `include/rustty*`,
`example/`, the A5/A3 gates (`.port/tools/gates/check_rustty_vt_headers.sh`,
`check_rustty_h_abi.sh`).
Assessment only — **nothing was implemented**. Written per the Phase-4 binding rules
(port from the RANGE DIFF / END STATE, never commit-by-commit; "already ported" needs
symbol + `file:line` + a gate that fails on regression).

**31 non-merge upstream commits** touch this area's paths (the task brief's 22-commit
log missed `include/**` — 9 additional commits land only header hunks here; all 31 are
individually verdicted below). **Headline: 1 PORTED (pre-existing equivalence),
0 PARTIAL, 29 ABSENT, 1 N-A.** Phase 4's finale (`d63dd9fc`) left this area FULLY
caught up at the old pin — rustty `Option_` ends at `TerminfoName = 37`
(`crates/rustty-vt-c/src/terminal.rs:2160`), `TerminalData` at `CursorAtPrompt = 39`
(`terminal.rs:3286`, `TERMINAL_DATA_MAX = 39` at `:3359`), and export parity was exact.
Everything this range added on top is missing. The range is dominated by ONE feature
family: the **Kitty clipboard protocol (OSC 5522)** and the **paste C API** built on it.

---

## 0. Coverage statement — exhaustive vs sampled

| Work | Coverage |
|---|---|
| Upstream commit enumeration for the area paths | **EXHAUSTIVE** (`git log --no-merges 12967b68f..683d8db64 -- include src/lib src/lib_vt.zig src/terminal/c example src/build src/global.zig` → 31 commits; per-commit `--stat` captured for the include/macos and build commits) |
| Exported C symbol set, oracle new pin vs rustty, **both directions** | **EXHAUSTIVE** (mechanical: oracle `@export(..., .{ .name = "ghostty_..." })` in `src/lib_vt.zig` at the new pin = **195** names, vs `grep -rhoE 'extern "C" fn (rustty\|ghostty)_...' crates/rustty-vt-c/src` = **194** unique names, all `rustty_*`, zero `ghostty_` aliases; `comm` both ways — §3) |
| Range-diff END STATE of `src/terminal/c/{terminal,kitty_graphics,io,sys,types,result,main,paste}.zig`, `src/lib_vt.zig`, `src/lib/{types,union}.zig`, `src/build/GhosttyZig.zig` | **EXHAUSTIVE** — the full range diff of each file read line-by-line (terminal.zig 1,032 diff lines; paste.zig read as the whole 522-line target file) |
| Header hunks `ghostty.h`, `vt.h`, `sys.h`, `modes.h`, `osc.h`, `kitty_graphics.h` | **EXHAUSTIVE** — full range diffs read line-by-line |
| Header hunks `paste.h` (182), `terminal.h` (419), `io.h` (44), `types.h` (6) | **SAMPLED** — attributed to commits via per-commit stats and the already-read Zig source they document; NOT re-read line-by-line. Safe for the verdicts: absence in rustty is established by the mechanical rename-diff (§4) plus exhaustive symbol greps that returned zero hits |
| Token-renamed oracle-vs-rustty header diff, per affected file | **EXHAUSTIVE at file granularity** (changed-line counts in §4, mechanical `sed`-rename + `diff`) |
| Rustty-side absence greps | **EXHAUSTIVE per feature**: `can_remember`, `ClipboardRead`, `clipboard_read`, `MimeReader`, `random_secure`/`RandomSecure`, `terminal_paste`, `PasteSource` — **all 0 hits** over `crates/rustty-vt-c/src`, `crates/rustty-vt/src`, `include/rustty*`; plus targeted reads: `result.rs` (no `Rejected = -7`), `terminal.rs:359-371` (old 4-field `ClipboardWrite`, `ClipboardWriteFn` still returns `clipboard::WriteResult`), `kitty_graphics.rs` (no `renderData`/`resolveChain`/`relative` arm), `include/rustty-embed.h:84-92` (old `clipboard_content_s`, no `len`, no `PRIMARY`) |
| The 6 macos/apprt clipboard commits (`0ce9054bf`, `8c7a34d4c`, `af9470b19`, `c2c0db68a`, `25c61e852`, `1334cc213`) | Only their `include/ghostty.h` hunks are this area's; the ghostty.h **range diff read in full** (the END STATE). Their `Surface.zig`/`apprt`/`macos`/`termio` bodies were NOT read (areas E/F) |
| `d8b920e50`/`322d7ae78` non-build hunks (kitty animation/pixel), `dda8e6f31` TinyIo hunk | Build/sys slices read in full; the TinyIo `randomSecure` hunk read (coupling note §5); kitty subsystem bodies are **area D's** and not body-assessed here |
| `src/global.zig` | **ZERO commits in range** (mechanical: absent from the 31-commit log) — nothing to assess |
| Gate source reading (A5 arm (d) oracle-read + allowlist, A3 sha-pin) + the sha-pin mismatch measurement | **EXHAUSTIVE by source reading**; **no gate was executed as a whole** (no build run for this assessment); the arm-(d) comm and the A3 sha comparison were reproduced mechanically |
| `example/**` | Range diff shows exactly 2 examples changed (`c-vt-effects/src/main.c`, `c-vt-paste/{README.md,src/main.c}`); both oracle diffs read in full; rustty counterparts grepped (`c-vt-paste/src/main.c` = 56 lines, old safety demo; `c-vt-effects` still registers the OLD sync `on_clipboard_write` returning `RusttyClipboardWriteResult`) |

---

## 1. Summary with numbers

**Commit verdicts.** 31 commits: **1 PORTED, 0 PARTIAL, 29 ABSENT, 1 N-A.**
No commit in the range was reverted in-range; every ABSENT survives to the end state.

**Exports (C symbols).** Oracle at the new pin exports **195** (`src/lib_vt.zig`
`@export` list). rustty-vt-c exports **194** unique `extern "C" fn rustty_*`.

| | count |
|---|---|
| **oracle exports rustty is MISSING** (added in this range) | **1** — `terminal_paste` (`60a1ae2df`) |
| **rustty exports the oracle removed in-range** (stale) | **0** ✅ (`comm -13` empty) |
| `ghostty_*` aliases in rustty-vt-c (R2) | **0** ✅ |

**Consequence for gates:** A5 arm (d) (`check_rustty_vt_headers.sh:299` reads
`origin/main:src/lib_vt.zig`; allowlist EMPTY per `:333`) is **RED with exactly
`rustty_terminal_paste` today**. That is the intended tracker — do NOT allowlist it
(R5); port it. **A3 (`check_rustty_h_abi.sh`) is BLOCKED/RED today**: the sha-pin
`.port/tools/oracle-ghostty-h.sha256` = `aa01353d…` but the oracle `ghostty.h` at the
new pin hashes `de852e83…` (6 in-range macos clipboard commits changed it, +59 lines
net). Re-pin only WITH the embed-header port (Bundle E2), never before — the red is
telling the truth.

**Headers.** Token-renamed oracle-vs-rustty diff, changed lines per affected file:
`terminal.h` **431**, `paste.h` **174**, `io.h` **44**, `sys.h` **33**, `vt.h` 19,
`kitty_graphics.h` 6, `types.h` 6, `modes.h` 1, `osc.h` 1. (terminal.h/vt.h counts
slightly exceed the range stat because they include small pre-existing recorded
divergences.) All other vt headers unchanged in-range. `include/rustty-embed.h`: the
full 59-line ghostty.h clipboard-ABI rework is missing (§2 #21–27).

**Terminal option/data tags.** rustty `Option_` ends at **37** (`terminal.rs:2160`),
oracle at **39** (missing: `clipboard_read = 38`, `clipboard_write_max_bytes = 39`).
rustty `TerminalData` ends at **39** (`terminal.rs:3286`), oracle at **40** (missing:
`clipboard_write_max_bytes = 40`). rustty `Result` ends at `LimitExceeded = -6`
(`result.rs:22`), oracle adds `rejected = -7`. rustty sys `Option` ends at `log = 2`,
oracle adds `random_secure = 3`.

**ABI BREAKS this range makes to EXISTING surface** (not just additions):
1. `ClipboardWriteFn` return type: `clipboard.WriteResult` → `void`, and
   `ClipboardWrite` grows `name`/`granted`/`can_remember`/`ctx`/`reply` (reply-based
   protocol). rustty `terminal.rs:370` still has the old sync signature.
2. `writePtyTrampoline` payload: `[:0]const u8` → `[]const u8` (no NUL guarantee).
3. embed `ghostty_clipboard_content_s` grows `size_t len` (LAYOUT change);
   `ghostty_runtime_read_clipboard_cb` return `bool` → `ghostty_clipboard_read_result_e`
   with 3 new params; `ghostty_surface_complete_clipboard_request` re-signatured; NEW
   `ghostty_surface_deny_clipboard_request`. All the sret/layout crash class (memory
   `embed-abi-byvalue-sret-gate`).

**G1b honesty note (measured).** In `w0/g1b-after-at-683d8db64.txt` the area's rustty
modules all read ok — `vt_c` **zig 416 / rust 461 ok**, `lib_vt` ok, `main_c` ok — but
the zig side of `vt_c` is `glob(src/terminal/c/*.zig)` and **already includes the
range's new tests**: terminal.zig **+5** (`kitty clipboard write via C effects`,
`set clipboard write max bytes`, `set clipboard_read callback`, `mode 2048 enable and
disable use C callbacks`, `mode 2048 enable tolerates missing C callbacks`) and
paste.zig **+4** (`terminal_paste null handling` / `without write_pty is invalid` /
`text and unsafe` / `event`), plus reworked assertions inside 5 existing tests
(terminal.zig ×2, kitty_graphics.zig ×2, io.zig ×1, types.zig ×1). The green is
**bookkeeping**: rustty's +45 surplus masks the 9 unported zig tests — exactly the
memory-`derived-gate-green-can-be-bookkeeping` failure shape. **None of the 5 RED G1b
modules is this area's** (§7).

---

## 2. Per-commit verdict table

Ordered oldest→newest. "Evidence" cites the rustty location proving the verdict; greps
listed with **0 hits** were run over `crates/rustty-vt-c/src`, `crates/rustty-vt/src`,
`include/rustty*` unless narrowed. Gate: A5(d) = export-parity arm; A3 = embed-header
sha-pin/rename gate; "none" = no current gate can fail on it.

| # | upstream | what changed (in-area) | verdict | evidence | gate | cost |
|---|---|---|---|---|---|---|
| 1 | `f4f9991d2` | lib_vt.zig +1: `pub const snapshot = terminal.snapshot` (module-surface export) | **PORTED** (pre-existing equivalence) | `crates/rustty-vt/src/lib.rs:125` `pub mod snapshot` is already public; regression guard: `crates/rustty-vt-c/src/snapshot.rs` consumes it, so privatizing it breaks `cargo build --workspace` | cargo build | – |
| 2 | `07af4612b` | mode 2048 in-band size reports: terminal.zig +107 — `SizeFn` doc now covers mode-2048 enable reports + 2 NEW tests (`mode 2048 enable and disable use C callbacks`, `…tolerates missing C callbacks`) | **ABSENT** | `grep -n "2048" crates/rustty-vt-c/src/terminal.rs` → no mode-2048 tests; SizeFn doc still XTWINOPS-only | G1b masked (vt_c aggregate) | S — report logic is **area D** `stream_terminal.zig` (its drop=38 includes the handler tests); the C tests + doc are this area's |
| 3 | `5b9a77f20` | terminal.h ±25: SizeFn/OPT_SIZE/WRITE_PTY doc rewritten for mode 2048 | **ABSENT** | part of the 431-line terminal.h rename-diff | none | XS (R1 comment fidelity, rides #2) |
| 4 | `52190a5d8` | kitty C: `placement_source_rect` + `placement_render_info` bodies replaced by `p.sourceRect(image.*)` (intersect-before-size helper); test `placement_source_rect clamps to image bounds` extended with `placement_pixel_size` assertions | **ABSENT** | `kitty_graphics.rs:1150+` still transcribes the OLD inline `@min` clamp logic; no `source_rect`-helper call (grep `sourceRect\|source_rect(` on non-test code → old bodies) | G1b masked | S — `sourceRect` helper lives in **area D** `graphics_storage`; port C slice WITH it |
| 5 | `cd5f9eef0` | kitty C ±2: `.virtual, .relative => unreachable` arms for the new location tag | **ABSENT** | no `relative` variant anywhere in `kitty_graphics.rs` (grep 0) | none | XS — rides area D relative-placement storage |
| 6 | `d8b920e50` | build slice +9: wuffs module imported into vt (animation frame composition consumer) | **ABSENT** (build slice) | rustty's `wuffs` feature is decode-only (`crates/rustty-vt/Cargo.toml:55-65`, `rustty-wuffs` wired for `sys` PNG decode, not pixel ops) | none | S — END STATE is #9's conditional wiring; kitty animation itself is **area D** (kitty drop=202) |
| 7 | `f5b3efe45` | kitty C +44: `computeViewportPos` resolves `.relative` chains via `storage.resolveChain(rel)` (root pin + accumulated `+\|` offsets), horizontal visibility clipping | **ABSENT** | `kitty_graphics.rs:1359-1438` computeViewportPos has no chain resolution, no `right_col` clip (grep `resolve_chain` 0) | G1b masked | M — blocked on area D `resolveChain`; R7: `+\|` → `saturating_add` |
| 8 | `73903f76a` | kitty C +5: `imageGetTyped` `.data_ptr`/`.data_len` via `image.renderData()` (current animation frame) + generation-coherence comment | **ABSENT** | `kitty_graphics.rs` imageGetTyped still reads `image.data` (grep `renderData\|render_data` in src → 0 non-comment) | G1b masked | S — rides area D `renderData` |
| 9 | `322d7ae78` | build slice ±19: wuffs wired only when `vt_options.kittyGraphics(target)` (lazyDependency) — supersedes #6's unconditional import; **port the conditional END STATE** | **ABSENT** | as #6; no kitty-conditional wuffs edge in any Cargo.toml (grep `kitty` in rustty-vt/-vt-c Cargo.toml → comments only) | none | S — Rust shape: `rustty-wuffs` behind the kitty-graphics feature axis |
| 10 | `07c6fc21b` | modes.h +1: `GHOSTTY_MODE_PASTE_EVENTS` (5522, disabled) | **ABSENT** | modes.h rename-diff = 1 line | none | XS — mode registration is area D `modes.zig`; header rides Bundle P |
| 11 | `ca9e5b130` | types.zig test +1: `KITTY_DESKTOP_NOTIFICATION = 26` manifest assertion; osc.h +1 enum row | **ABSENT** | rustty osc.h rename-diff = 1 line (no `= 26` row); no manifest assertion | G1b masked | XS — rides area D `osc/parsers/kitty_desktop_notification.zig` (osc drop=47) |
| 12 | `e03475c0c` | **clipboard_read effect**: terminal.zig +252 (`ClipboardRead`/`ClipboardReadReply`/`ClipboardReadReplyFn` sized structs, `Effects.clipboard_read`, read trampoline + reply trampoline with stack-fallback MIME conversion, dynamic install in `setTyped`, `Option.clipboard_read = 38`, test `set clipboard_read callback`), lib/types.zig +1 (`else => @compileError` in `String.init`), lib_vt +1 (`pub const clipboard`), types.zig +3 registry rows, c-vt-effects +52 (`on_clipboard_read`) | **ABSENT** | grep `ClipboardRead\|clipboard_read` → **0** in all rustty; `Option_` ends 37; example still lacks read callback. lib/types.zig arm: rustty `String::init(&[u8])` (`crates/rustty-lib/src/types.rs:15`) is statically typed — the compile-error arm is satisfied by the type system (note, no code needed) | none today; check_closed_enum_abi once typedef'd | **L** — Bundle C; blocked on area D `clipboard.zig` `Read`/reply rework |
| 13 | `6959fd46c` | **Kitty clipboard write only**: terminal.zig ±142 — `ClipboardWrite` reply-based REWORK (`ClipboardWriteReply`/`ClipboardWriteReplyFn`, `ClipboardWriteFn` now returns `void`, stack `ClipboardWriteCtx` + reply trampoline, dynamic trampoline install with the null-fast-path doc), lib_vt ±1 | **ABSENT** | `terminal.rs:359-371`: old 4-field struct; `ClipboardWriteFn` still `-> clipboard::WriteResult`; `wrap()` still installs the write trampoline statically | none (the stale sync ABI compiles fine) | **L** — Bundle C; **ABI BREAK** of an existing typedef |
| 14 | `4f49dc2b8` | Kitty clipboard protocol reads served through the clipboard_read effect: terminal.zig +70 (test `kitty clipboard write via C effects` read arms: EPERM w/o callback, OK/DATA/DONE with) | **ABSENT** | as #12/#13 (end-state test read in full) | G1b masked | – (folded into Bundle C) |
| 15 | `af8d28a94` | terminal.zig +1: `.drag_and_drop = null` handler-effect slot | **ABSENT** | no `drag_and_drop` in `terminal.rs` (grep 0) | none | XS — rides area D `kitty/dnd*.zig` (kitty drop=202) |
| 16 | `a5bb22e23` | lib_vt +3: `Paste`/`PasteSource`/`PasteError` module surface (shared paste core) | **ABSENT** | grep `PasteSource` → 0 | none | XS — rides area A/D `src/terminal/paste.zig` (NEW, input_paste drop=7) |
| 17 | `876032316` | lib_vt +4: `max_paste_frame_size`, `isSafePasteWith`, `encodePasteWriter` (input paste additions) + mode 5522 enabled in libghostty | **ABSENT** | grep `is_safe_with\|encode.*writer\|max_paste_frame` in rustty-vt input paste → 0 | none | XS — rides area A/D `src/input/paste.zig` |
| 18 | `dda8e6f31` | **sys secure random override**: sys.zig +20 (`RandomSecureFn`, `Option.random_secure = 3`, global slot + `randomSecureWrapper` → `terminal_sys.random_secure`), sys.h +33; TinyIo +53 (platform default: arc4random_buf/getrandom, test `randomSecure fills with fresh entropy`) is **area B's** | **ABSENT** | `grep random_secure crates/rustty-vt-c/src/sys.rs` → 0; sys `Option` ends at `log = 2` | none | M — this area: sys.rs + sys.h; consumer `terminal/sys.zig` +48 & `kitty/clipboard_grants.zig` are area D; TinyIo default area B. Coordinate the three |
| 19 | `60a1ae2df` | **THE terminal_paste C API**: paste.zig +298 (`Source` enum, sized `Request` (`C: GhosttyPaste`), `terminal_paste` with size-versioning check + `Sink`/`readTrampoline`, 4 tests + `TerminalPasteCapture` harness), NEW export `ghostty_terminal_paste`, main.zig +1, **result.zig +1 (`rejected = -7`)**, types.zig +4 rows (`GhosttyPaste`, `GhosttyPasteSource`, `GhosttyMimeReader`…), paste.h +152, terminal.h +5, types.h +6, vt.h ±10, c-vt-paste example +205 | **ABSENT** | §3 comm: `terminal_paste` is THE missing export; `result.rs` ends at `LimitExceeded = -6`; `example/c-vt-paste/src/main.c` = 56 lines (oracle 269), only `rustty_paste_is_safe` calls | **A5(d) RED** (the 1 name) | **XL** — Bundle P; blocked on areas A/D paste core + kitty clipboard paste events (mode 5522) |
| 20 | `da27e6c90` | paste reads clipboard on demand: paste.zig ±236 (Request drops inline data for `mimes` + `io.MimeReader`; streaming `Sink` writer; `@constCast(req)` ctx), **io.zig +19 (`MimeReaderFn`/`MimeReader` + layout-test arms)**, terminal.zig ±2, types.zig ±4, example ±58 — supersedes parts of #19; **port the streaming END STATE** | **ABSENT** | `grep MimeReader crates/rustty-vt-c/src/io.rs` → 0; io.rs layout test lacks the MimeReader offset asserts | A5(d) rides #19 | – (folded into Bundle P) |
| 21 | `0ce9054bf` | ghostty.h +15: `GHOSTTY_CLIPBOARD_REQUEST_KITTY_READ`, confirm-cb signature | **ABSENT** | `grep KITTY_READ include/rustty-embed.h` → 0 | **A3** (pin mismatch live) | S (header) — impl areas E/F |
| 22 | `8c7a34d4c` | ghostty.h ±27: **`ghostty_clipboard_content_s` grows `size_t len`** (binary-safe), `ghostty_clipboard_complete_s`, complete/deny request re-signature | **ABSENT** | `include/rustty-embed.h:84-87` struct still `{mime, data}` only; grep `clipboard_complete_s` → 0 | A3 | S (header) — **LAYOUT break**, embed callers atomic (E/F) |
| 23 | `af9470b19` | ghostty.h ±36: `ghostty_clipboard_confirm_s`, `ghostty_clipboard_read_result_e`, `read_clipboard_cb` returns enum + mime/list params | **ABSENT** | grep `clipboard_confirm_s\|read_result_e` → 0 | A3 | S (header) — cb-signature break |
| 24 | `7ae9b1113` | Kitty write permission prompts/grants: terminal.zig ±149 (`ClipboardWrite` gains `name`/`granted`/`can_remember`; grant round-trip test arms), types.zig ±3 (`Write.Status`/`Read.Status` enum rows), c-vt-effects +13 (reply doc) | **ABSENT** | grep `can_remember` → **0** anywhere in rustty | none | – (folded into Bundle C END STATE) |
| 25 | `c2c0db68a` | ghostty.h +1 (mode-5522 macOS enable slice) | **ABSENT** | rides #21–23 delta | A3 | XS |
| 26 | `25c61e852` | ghostty.h +1: `GHOSTTY_CLIPBOARD_REQUEST_KITTY_WRITE` | **ABSENT** | grep 0 | A3 | XS — impl areas E/F |
| 27 | `1334cc213` | ghostty.h +1: `GHOSTTY_CLIPBOARD_PRIMARY` | **ABSENT** | `include/rustty-embed.h:80-82` enum lacks PRIMARY | A3 | XS |
| 28 | `928c7f0e7` | terminal.h +9: listing reads (`type=list`) exempt from permission prompts (doc contract on ClipboardRead.list) | **ABSENT** (header slice) | part of terminal.h rename-diff; handler logic is area D `stream_terminal`/`kitty/clipboard_command` | none | XS — rides Bundle C |
| 29 | `5350d4a5f` | write limit configurable: terminal.zig +82 — `Option.clipboard_write_max_bytes = 39` (NULL restores `kitty_clipboard.max_write_size`), `TerminalData.clipboard_write_max_bytes = 40`, test `set clipboard write max bytes` (EFBIG path) | **ABSENT** | `Option_` ends 37 (`terminal.rs:2160`); `TERMINAL_DATA_MAX = 39` (`terminal.rs:3359`) | none | S — handler field `kitty_clipboard_write_max_bytes` is area D |
| 30 | `70f006575` | reject oversized writes: terminal.zig ±7 (setTyped wiring vs the handler cap) — END-STATE rider of #29 | **ABSENT** | as #29 | none | – |
| 31 | `8c5bc3d29` | lib/union.zig ±8: comptime `Value()` body → `@FieldType` (drops `@setEvalBranchQuota` + inline-for) | **N-A** — comptime-internal refactor, zero observable ABI/behavior delta; rustty's `union.rs` descriptor model resolves field types at construction (`crates/rustty-lib/src/union.rs:132-138` PORT NOTE) and transcribes no inline-for body to update | verified: no transcription of the old `Value` body exists in `union.rs` | – | – |

---

## 3. Symbol-level delta (mechanical, both directions)

Method identical to Phase 4 §3: oracle `@export` lines in `src/lib_vt.zig` at the new
pin (**195** names), token-renamed, `comm`'d against the rustty-vt-c grep (**194**
unique names, all `rustty_*`).

### 3a. Oracle → rustty missing: **1**

```
terminal_paste        60a1ae2df
```

**A5 arm (d) is RED with exactly this name today** (allowlist empty; oracle read at
`origin/main` = the new pin). Do not allowlist (R5); port it via Bundle P.

### 3b. rustty → oracle stale: **0** ✅

`comm -13` is empty — the Phase-4 stale-export debt (15 names) was fully remediated;
no in-range removal creates new staleness. The one-directionality gap flagged in
Phase 4 §6 #1 is MOOT for this range but remains structurally open.

---

## 4. Headers — per-file changed-line counts (token-renamed oracle @ new pin vs rustty)

| header | Δ lines | owner commits | note |
|---|---|---|---|
| `terminal.h` | 431 | e03475c0c, 6959fd46c, 7ae9b1113, 5350d4a5f, 928c7f0e7, 5b9a77f20, 60a1ae2df | ClipboardRead/Write reply ABI, options 38–39, data 40, mode-2048 docs |
| `paste.h` | 174 | 60a1ae2df, da27e6c90 | `GhosttyPaste`/`GhosttyPasteSource`/`terminal_paste`, streaming reader docs |
| `io.h` | 44 | da27e6c90 | `GhosttyMimeReader`/`GhosttyMimeReaderFn` |
| `sys.h` | 33 | dda8e6f31 | `GhosttySysRandomSecureFn`, `GHOSTTY_SYS_OPT_RANDOM_SECURE = 3` |
| `vt.h` | 19 | 60a1ae2df (+ pre-existing recorded divergence) | paste module/example doc rewrite |
| `kitty_graphics.h` | 6 | 73903f76a | DATA_PTR animation-frame doc contract |
| `types.h` | 6 | 60a1ae2df | paste typedefs |
| `modes.h` | 1 | 07c6fc21b | `GHOSTTY_MODE_PASTE_EVENTS` (5522) |
| `osc.h` | 1 | ca9e5b130 | `KITTY_DESKTOP_NOTIFICATION = 26` |
| all other vt headers | 0 | — | byte-identical ✅ |

`include/rustty-embed.h`: 6 in-range `ghostty.h` commits, net one coherent clipboard-ABI
rework (§2 #21–27) — **must port as ONE unit** with the E/F embed/apprt work.
`include/rustty.h` (cbindgen) regenerates from the crate; arm (a3) enforces freshness
once the crate changes land.

---

## 5. Atomic bundles, cross-area couplings, UB/SB hazards

### Bundles (must land together or be worse than absent)

- **Bundle P (terminal_paste C API)** — `60a1ae2df` + `da27e6c90` (+ lib_vt surface
  riders `a5bb22e23`, `876032316`; modes.h rider `07c6fc21b`). Port the streaming END
  STATE (MimeReader, not #19's intermediate inline-data Request). One commit-set:
  paste.zig port + `rustty_terminal_paste` export + `result.rs Rejected = -7` +
  io.rs `MimeReader` + layout-test arms + `paste.h`/`terminal.h`/`io.h`/`types.h`/
  `vt.h` hunks + types.rs registry rows + the `c-vt-paste` example rewrite (both
  README and main.c; the example exercises paste events end-to-end). **Blocked on**:
  area A/D `src/terminal/paste.zig` (shared paste core, input_paste drop=7),
  stream-handler paste op + mode 5522 (stream_terminal drop=38), kitty clipboard paste
  events (`kitty/clipboard*.zig`, kitty drop=202). A header/impl skew here is the sret
  crash class (memory `embed-abi-byvalue-sret-gate`).
- **Bundle C (Kitty clipboard C effects)** — `e03475c0c` + `6959fd46c` + `4f49dc2b8` +
  `7ae9b1113` + `5350d4a5f` + `70f006575` + `928c7f0e7`(header slice). Same structs
  rewritten repeatedly; port the END STATE: reply-based `ClipboardWrite` (**ABI BREAK**:
  `ClipboardWriteFn` → `void` return + 5 new fields), `ClipboardRead`+replies, dynamic
  trampoline install (null fast paths, with the oracle's rationale comment), Options
  38/39, TerminalData 40, `Write.Status`/`Read.Status` registry rows, terminal.h, and
  the `c-vt-effects` example rework. **Blocked on**: area D `src/terminal/clipboard.zig`
  (Write/Read reworked to Status + reply fn), `kitty/clipboard*.zig` ×5 (the 202-test
  kitty debt), stream_terminal. Bundle C MUST precede or accompany Bundle P (paste
  events reuse ClipboardRead/ClipboardContent — `paste.zig` imports them from
  `terminal.zig`).
- **Bundle E2 (embed clipboard ABI)** — `0ce9054bf` + `8c7a34d4c` + `af9470b19` +
  `c2c0db68a` + `25c61e852` + `1334cc213`: the rustty-embed.h rework (content_s `len`,
  PRIMARY, KITTY_READ/WRITE/LIST request kinds, confirm/complete structs,
  read-result enum, deny export) + re-pin `.port/tools/oracle-ghostty-h.sha256` +
  A4 byte-match re-verify. Rides **areas E/F** (Surface.zig, apprt/embedded, termio
  messages, macos). **A3 is RED today**; re-pin only with this bundle.
- **Bundle KG (kitty graphics C reads)** — `52190a5d8` + `cd5f9eef0` + `f5b3efe45` +
  `73903f76a` (+ build riders `d8b920e50`/`322d7ae78`, END STATE = kitty-conditional
  wuffs wiring). The C hunks reference `sourceRect`/`resolveChain`/`renderData` that
  don't exist in rustty-vt yet — port WITH area D's animation/relative-placement/pixel
  landing, not before.
- **Bundle SR (sys secure random)** — `dda8e6f31` splits three ways: sys.rs + sys.h
  (this area), `terminal/sys.zig` + `kitty/clipboard_grants.zig` consumer (area D),
  TinyIo platform default (area B). The C option is meaningless until the consumer
  exists; land after/with area D's clipboard_grants (grants need entropy for the pw
  scheme — Bundle C dependency).
- **Bundle M2048 (mode 2048)** — `07af4612b` + `5b9a77f20`: C tests + header doc ride
  area D's stream_terminal report logic.
- **Singles**: `ca9e5b130`/`af8d28a94` are 1-line riders of area D subsystems (osc
  kitty-notification, dnd) — port with those, keyed so they are not dropped.

### Cross-area couplings (named areas)

| item | blocked on / shared with | area |
|---|---|---|
| shared paste core (`src/terminal/paste.zig`), input paste additions (`isSafeWith`, `encodeWriter`, `max_frame_size`) | input_paste drop=7 | A/D |
| `clipboard.zig` Write/Read Status+reply rework; kitty clipboard protocol (`kitty/clipboard*.zig` ×5) | kitty drop=202 | D |
| kitty dnd handler slot | `kitty/dnd*.zig` ×5, kitty drop=202 | D |
| stream_terminal: mode 2048 report, paste op, listing-read exemption, write-limit cap field | stream_terminal drop=38 | D |
| osc `KITTY_DESKTOP_NOTIFICATION = 26` | osc drop=47 (`osc/parsers/kitty_desktop_notification.zig`) | D |
| kitty animation/pixel/relative storage (`renderData`, `resolveChain`, `sourceRect`, wuffs swizzle) | `graphics_{animation,pixel,storage,image}.zig`, `pkg/wuffs` | D (pkg/wuffs: its own lane) |
| TinyIo `randomSecure` platform default + test | `src/lib/TinyIo.zig` | **B** (explicit fan-out split) |
| `terminal/sys.zig` `random_secure` slot + `clipboard_grants` entropy consumer | area D sys | D |
| embed clipboard ABI impl (Surface.zig, apprt/embedded, termio message plumbing, macos Swift) | Bundle E2 | E/F |
| `example/c-vt-effects`, `example/c-vt-paste` | ride Bundles C/P (build.sh convention per EXAMPLES_PORT) | this area |

### UB/SB hazards for the coming ports (PORT_RULES raw-pointer discipline)

1. **Clipboard reply contexts are stack-lifetime raw pointers**: `ClipboardWrite.ctx`
   / `ClipboardRead.ctx` point at a STACK `ClipboardWriteCtx`/`ClipboardReadCtx` inside
   the trampoline frame; the C callback MUST call `reply` before returning (documented
   contract). The Rust port must model `ctx` as an opaque `*const` valid only for the
   callback duration — never retain, never `&mut` from it. `ClipboardReadCtx` also
   carries `wrapper: *TerminalWrapper` and the reply trampoline allocates via
   `ctx.wrapper.terminal.gpa()` **while the stream-handler call chain is live on the
   same wrapper** — the exact interleaved-alias shape of memories
   `shaper-hook-sb-clean-interleave` / `search-screen-sb-ub-finding`. Route through the
   wrapper's OWN raw pointer (the terminfo_name_buf precedent at `terminal.rs:2441`),
   add LIFETIMES.tsv rows for both ctx structs BEFORE porting (R11 prospective prong),
   and prove on an isolated Miri harness.
2. **Re-entrancy**: `write.reply(...)`/`read.reply(...)` re-enter the Zig handler
   (spooled Kitty transaction commit → pty writes) from INSIDE the C callback, which
   is itself inside a handler frame. In Rust this is a call back into handler state
   through a raw alias while a `&mut`-derived frame is active — raw-ptr args only, no
   `&mut Handler` re-borrows across the callback.
3. **`@constCast(req)` in `terminal_paste`** (`paste.zig:76`): the `*const Request` is
   const-cast for the reader ctx. In Rust NEVER mint a `*mut` intending writes from a
   `&`-derived pointer — keep it `*const` and cast only at the opaque-ctx boundary
   (the callback never writes through it).
4. **Size-versioned sized structs**: `req.size < @sizeOf(Request)` (and reply `.size`)
   is the ABI-versioning guard — carry it verbatim (R1) and never read fields beyond
   the validated prefix (R7). New sized structs (`GhosttyPaste`, `ClipboardRead`,
   `ClipboardWriteReply`, `ClipboardReadReply`) need types.rs layout rows so arm (c)
   asserts them.
5. **ABI-break atomicity**: `ClipboardWriteFn`'s return type change and
   `ghostty_clipboard_content_s`'s new `len` field are silent-UB-if-skewed changes
   (wrong sret/register use, OOB reads of the shorter struct). Header + typedef +
   trampoline + example must land in ONE commit-set per bundle; A4 +
   `menu-shortcut-parity`-class harness re-run for the embed side (memory
   `embed-abi-byvalue-sret-gate`).
6. **`readTrampoline` streaming sink**: the C callback writes through a
   `GhosttyWriter` into a Zig `*std.Io.Writer` owned by the paste encoder while the
   handler frame is live — the Rust `Sink` must hold only its own `*mut` state, with
   `write_failed` telling sink-failure from reader-failure (port the distinction
   exactly; it decides `WriteFailed` vs `ReadFailed` → `io_error` mapping).
7. **kitty relative placements**: `computeViewportPos` now calls
   `storage.resolveChain(rel)` while the C placement ITERATOR holds `entry.value_ptr`
   into the same storage map — iterator + storage aliasing; raw-ptr discipline, and the
   `.relative => unreachable` in the root arm ports as `unreachable!()` (checked, R7)
   with the oracle's "resolveChain roots are never relative" comment.
8. **R7 arithmetic**: `+|` saturating adds in viewport offset math →
   `saturating_add`; the `i32`/`i64` widening for `bottom_row`/`right_col` visibility
   must keep the exact widths (no silent `as` narrowing).
9. **sys global `random_secure` slot**: same global-mut pattern as `decode_png`/`log`
   — non-thread-safe global init is a measured hazard (memory
   `parallel-cargo-test-exposes-c-init-races`); use the existing sys.rs
   synchronization shape, never a bare `static mut`.

---

## 6. Gate coverage deltas this range creates

| # | hole | proposal |
|---|---|---|
| 1 | **A3 sha-pin stale**: pinned `aa01353d…` vs oracle-at-pin `de852e83…` ⇒ A3 BLOCKED/RED right now | re-pin as part of Bundle E2, never before (R5: the red is currently telling the truth) |
| 2 | **A5(d) RED** with `rustty_terminal_paste` | the intended tracker; clears with Bundle P |
| 3 | **Header-text drift beyond exports has no gate** (Phase-3/4 arm-(c) registry-blindness, still open): the 431-line terminal.h drift, reply-fn typedef changes, and modes.h/osc.h rows are invisible until typedef'd into types.rs | unchanged recommendation: header-driven enumeration; Bundles P/C add ~10 registry rows — write them WITH the port |
| 4 | **G1b vt_c aggregate masking** (§1): 9 new zig tests invisible under the +45 rust surplus | when Bundles P/C/M2048 land, verify per-file: terminal.zig (oracle 121 tests), paste.zig (15), kitty_graphics.zig, io.zig — not via the module total |
| 5 | **Closed-enum runtime coverage**: new inbound `c_int` surfaces (Option 38/39, TerminalData 40, sys Option 3, Result −7, `Write.Status`/`Read.Status`, `PasteSource`) | extend `check_closed_enum_abi{,_runtime}.sh` coverage in the same commits |
| 6 | **Embed ABI byte-match**: content_s layout change + cb signature changes | A4 + the byvalue-sret harness re-run in Bundle E2 (memory `embed-abi-byvalue-sret-gate`) |

## 7. Dropped-test targets (g1b-after file) belonging to this area

**None of the 5 RED rows (kitty 202, osc 47, stream_terminal 38, input_paste 7,
parser 1) are this area's rustty modules** — they are areas A/D subsystems that this
area's bundles DEPEND on (§5 couplings). The area's REAL test debt is masked under the
green `vt_c` row (§1): **9 new zig tests** (terminal.zig 5, paste.zig 4) plus reworked
assertions in 5 existing tests (terminal.zig `set clipboard_write callback` /
`clipboard_write without callback…`, kitty_graphics.zig `placement_source_rect clamps…`
/ `placement_render_info returns all fields`, io.zig `C reader and writer layouts…`,
types.zig `manifest uses public enum names`) — all owed by the bundles above,
enumerable per commit in §2. `lib_vt` (zig 0) and `main_c` rows carry no zig-side
signal for this area by construction.
