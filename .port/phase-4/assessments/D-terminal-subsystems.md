# Phase 4 assessment — area D: terminal subsystems + input

**Range:** `6ad1fe7d8` → `12967b68f` (359 non-merge commits total; **47** touch my file set).
**Scope (oracle paths, as assigned):** `src/terminal/kitty/**`, `src/terminal/apc/**`,
`src/terminal/osc/**` (the *directory* — `osc.zig` itself is NOT in my set),
`src/terminal/formatter.zig`, `modes.zig`, `color.zig`, `device_status.zig`, `dcs.zig`,
`csi.zig`, `charsets.zig`, `hyperlink.zig`, `src/terminal/search/**`, `tmux.zig`,
`x11_color.zig`, `src/input/**`.
**No range commits touch:** `device_status.zig`, `csi.zig`, `charsets.zig`,
`x11_color.zig`, `search/**`, `tmux/**` (verified by
`git log --oneline --no-merges 6ad1fe7d8..12967b68f -- <paths>`).
**Oracle state:** working tree checked out AT the pin `12967b68f`; end-state files read
directly, per-commit hunks via `git show`.
**Rustty side:** `crates/rustty-vt/src/{kitty,apc,osc,input,formatter.rs,modes.rs,color.rs,dcs.rs,hyperlink.rs,terminal.rs}`, `crates/rustty-input/src/{binding.rs,command.rs}`,
`macos/Sources/RusttyKit` (one Swift-half note).
**No code modified.** Assessment only.

---

## 0. Coverage honesty — exhaustive vs sampled

**Exhaustive (read every hunk of the commit's diff in my file set, then located the
corresponding rustty code):**

- All 15 kitty commits (`graphics_{command,exec,image,storage,unicode}.zig`):
  `5ce1fe1ff`, `f719af00c`, `b8222f4a8`, `b5e86a428`, `8524cb593`, `4371871bc`,
  `402b9227d`, `d0c516f8f`, `590d669c4`, `f766f303a`, `ec04900ab`, `e5840bb9b`,
  `af2faa311`, `d866fa455`, `6760c6482` — every non-test hunk read in full; test hunks
  read for the security series (`af2faa311`, `ec04900ab`, `f766f303a`, `e5840bb9b`,
  `8524cb593`, `d866fa455`) and skimmed (`+test` name extraction plus first ~10 lines)
  for the pin-lifecycle/pending series.
- All 8 formatter commits: `79aa256fa`, `2ed67cadd`, `7a9c369cf`, `e523cf810`,
  `51a4311ef`, `997a2aff2`, `f024d21fc`, `75302feda` — non-test hunks read in full;
  the rustty formatter test inventory was compared **name-by-name** against upstream
  (`grep '^test '` vs the `// test "…"` markers: 102 vs 100, exactly the two G1b drops).
- apc: `433b16bcb`, `b53728241`, `946422dbc` — read in full (including the new
  `fraction.zig` for context, which is outside my set).
- modes: `cfc19e805` (modes.zig half), `c092b2bcf`, `f1ca88da3`, `7bb375800` — in full.
- color: `e3056658d`, `74a233b54` (color.zig halves), `7cd2f65f5` — in full.
- dcs `f973bd53b`, hyperlink `2fc238ed0` (hyperlink.zig half), osc parsers
  `727b8a02f` (parser hunks) and `b867a0f59` (semantic_prompt) — in full.
- input: `bd647035e`, `f4309055f` (incl. its macOS Swift half), `8b7c57c75`,
  `0a183c923`, `25b2d8a38`, `b67f8ef51`, `8cfbaf545`, `c011ad870`, `a14eba747` — in
  full. `df23bef0e` (command.zig 185/176) — **sampled**: read the header, the new
  `Command.translated` fn, and the first ~15 `i18n.N_(…)` wrap sites; verified the
  remaining hunks are the same mechanical `"…"` → `i18n.N_("…")` wrap by pattern
  (`git show df23bef0e -- src/input/command.zig | grep -c 'N_('`), and verified rustty
  `command.rs` has **zero** i18n references at all.

**Rustty evidence discipline:** every ABSENT verdict below is backed by a grep/read of
the corresponding rustty file showing the *old* (pre-range) shape at a cited line, not
by absence of a hash citation. Nothing was executed; `cargo` was not run.

**Cross-checks against gate data:** the G1b drop counts for formatter (2), modes (1),
input_key_encode (3) are each reconciled below to exact test names. apc (4) and
kitty (11) are reconciled to the full list of range-added upstream tests (6 and ~23
respectively; the gate's net counts are smaller because rustty carries rust-only extra
tests that mask part of the drop — noted in §5).

---

## 1. Summary

| Verdict | Commits | Notes |
|---|---|---|
| **PORTED** | **0** | nothing from this range has landed in my area |
| **PARTIAL** | **0** | rustty is uniformly at the pre-range (Phase-3 finale) shape |
| **ABSENT** | **44** | includes 2 live *bug* fixes (`d866fa455`, `7cd2f65f5`) and a 6-commit security-hardening series |
| **N-A** | **3** | `74a233b54` (Zig-0.16 LLVM-vectorizer workaround; end state assessed under `e3056658d`), `727b8a02f` (my slice is 2 mechanical follow-on lines; the substantive 8 MiB cap lives in `osc.zig`, outside my set), `b867a0f59` (Zig `lib.Enum` reflection; rustty plain enum is behavior-identical; C-ABI half is area C) |

**Highest-severity findings:**

1. **`d866fa455` is a live, exploitable-behavior bug in rustty today.**
   `graphics_storage.rs:673` still joins the range-delete bounds with `||`
   (`key.image_id >= first || key.image_id <= last`), which matches **every**
   placement for any valid range. Upstream fixed it to `&&`. A targeted kitty
   `d=r`/`d=R` delete wipes all graphics placements. One-character fix + the two
   amended range tests.
2. **`7cd2f65f5` is a live behavior bug**: `color.rs:687` `reset()` still does
   `self.override_ = self.default;` instead of `= None`, so OSC 110/111/112 pin the
   reset color to the *current* default instead of deferring to future default changes.
3. **The kitty security series (`af2faa311`, `ec04900ab`, `f766f303a`, `590d669c4`,
   `e5840bb9b`, + `d866fa455`) is wholly absent.** rustty still: validates image paths
   *before* open (TOCTOU, `graphics_image.rs:472-523`), accepts `/tmpX` via byte-prefix
   checks (`:585-605`), multiplies untrusted u32 dimensions unchecked
   (`pixel_size`/`grid_size`/`rect`), has no shared-memory range validation, and no
   png-decoder allocation cap.
4. **The kitty placement-pin lifecycle series (`6760c6482`, `4371871bc`, `d0c516f8f`,
   `402b9227d`, `b5e86a428`, `b8222f4a8`, `8524cb593`) is wholly absent** — pin leaks
   on placement replacement/eviction/retransmit, stale garbage-pin placements, wrong
   d=p/d=c interior-row deletion, no pending-payload union (which the snapshot API,
   area C, depends on).
5. **The formatter is at the Phase-3 shape end-to-end** — all 8 commits absent,
   including the `997a2aff2` pending-wrap preservation and the `7a9c369cf`/`e523cf810`
   tabstop-ordering fixes (both correctness, not perf: replayed snapshots resume input
   at the wrong position without them).

---

## 2. Per-commit verdicts

### 2.1 Kitty graphics — 15 commits, all ABSENT

Where several commits rewrite the same function, the row states what the END STATE
requires; port from the range diff of the four files, not commit-by-commit.

| upstream | what changed (my files) | rustty evidence (pre-range shape) | new upstream tests to carry |
|---|---|---|---|
| `6760c6482` support pending image payloads (storage +298/−28, image +45/−10) | `Image.data` → complete/pending tagged union carrying expected byte length; `addPendingImage(io, alloc, s, …)`; pending images reserve `total_bytes`; completion preserves generation/age | `graphics_image.rs:820` `pub data: Vec<u8>` (plain); no `add_pending`/`Pending` anywhere in `kitty/` (grep = 0) | `storage: pending image completes once and preserves age`, `storage: pending images share exact eviction ordering`, `storage: stale pending completion loses to delete replacement and eviction`, `storage: replacement reuses pending reservation and preserves placements` (later renamed by `b8222f4a8`) |
| `4371871bc` evict without scratch allocation | `Image.metadata.placement_count` maintained by add/remove; `evictImage` selects best candidate directly (unused → transient → generation → id), no allocation, **infallible** (`!void` → `void`); `deinit` clears placements before images | `graphics_storage.rs:812-857` still builds `Vec<Candidate>` + O(images×placements) `used` scan; no `placement_count`/`metadata` on Image (grep = 0); `evict_image` returns `Result<bool, OutOfMemory>` | `storage: placement count limit permits replacement`, `storage: evicts images in priority order` |
| `d0c516f8f` release replaced placement pins | `addPlacement(io, alloc, **s**, …)` deinits the found-existing placement before overwrite | `graphics_storage.rs:322-369` `add_placement` has **no screen param** and `self.placements.insert(key, p)` drops the old placement without `deinit(s)` → tracked-pin leak | `storage: replacing placement releases tracked pin` |
| `402b9227d` reclaim pruned placements | garbage-pin sweep before growing the placement map (`removePlacementByPtr`, preserves virtual placements); garbage pins yield no rect/render-pos | no sweep in `add_placement`; no `remove_placement_by_ptr` (grep = 0). C-API visible half (`computeViewportPos` garbage check) is area C | `storage: adding placement reclaims garbage placements` |
| `b5e86a428` release evicted placement pins | `addImage`/`evictImage*` take `s: *Screen`; eviction deinits each placement before removal | `add_image(&mut self, alloc, img)` at `graphics_storage.rs:263` — no screen param; eviction removes placements without deinit | `storage: eviction releases placement pins` |
| `b8222f4a8` clear placements on image retransmit (#13719) | `addImage` on found-existing calls `removePlacementsByImageId(s, id)`; `placement_count` reset to 0; spec-mandated | depends on `4371871bc`+`d0c516f8f` structures; none present | `kittygfx retransmit same id removes existing placements`, rename of the pending-reservation test to `…and removes placements` |
| `8524cb593` fix d=p/d=c point deletion | `Rect.contains(pin)` (column checked independently; page order only for the row span) replaces `pin.isBetween(tl, br)` | `graphics_storage.rs:618,789` still `target_pin.is_between(rect.top_left, rect.bottom_right)`; no `Rect::contains` | `storage: delete intersecting cursor checks interior row column`, `storage: delete intersecting cell checks interior row column` |
| `d866fa455` **fix graphics range deletion** | range-delete bounds joined with `and` | **`graphics_storage.rs:673` still `\|\|` — live bug** (see §1.1) | amended `storage: delete images by range 3` / `range 4` (3 placements, keep-below-and-above assertions, `tracked + 2` pin counts) |
| `af2faa311` restrict temporary image file paths | `isPathInDir(dir, path)` component-boundary helper replaces 4 byte-prefix `startsWith` checks in `isPathInTempDir` | `graphics_image.rs:584-609` still `starts_with(path, b"/tmp")` etc. — accepts `/tmpX`, `/dev/shm-evil` | `temporary file path must be inside directory` (7 assertions), `image load: temporary file outside directory prefix is rejected` (incl. "rejection precedes cleanup arming" access assert) |
| `ec04900ab` validate opened image file paths | open **first**, then `validatedFilePath(io, file, buf)` derives the canonical path from the **handle** (blocklist `/proc/`,`/sys/`,`/dev/` sans `/dev/shm/`); temp-file policy + delete keyed to handle path (`delete_path`), null-byte check message change | `graphics_image.rs:458-523` validates `path` (the submitted string) *before* `open_file(path)`; unlink guard armed pre-open — the exact TOCTOU upstream closes | `image load: blocklist applies to opened file after symlink swap` |
| `f766f303a` validate shared memory ranges | `sharedMemoryRange(t, stat_size)` — dimension check *before* multiply (`max_dimension`), offset/size cast+bounds vs stat, `data_size > max_size` reject; S interpreted as byte count from O; `stat.size` checked cast | `graphics_image.rs` `readSharedMemory*` still computes `expected_size` by unchecked multiply and slices `map[start..end]` with `t.offset as usize` (`:442-447`); no `shared_memory_range` (grep = 0) | `shared memory range with offset and size`, `… rejects out of bounds offset`, `… validates dimensions before multiplication` |
| `590d669c4` limit png decoder allocations | wrap `decode_png` in `LimitedAllocator(max_size)`; `limit_exceeded` → `InvalidData` (not OOM) | rustty png decode path has no allocator cap (no `LimitedAllocator`/`limit` in `graphics_image.rs` decode path) | `image load: png rejects oversized decoder allocation`, `image load: png rejects oversized Wuffs image before allocation` |
| `e5840bb9b` harden placement geometry | `saturatingMul`, `scaleDimension` (u64 intermediate, round-half-up, u32-saturate) replace f64 casts in `pixelSize`; `gridSize` numerators can't wrap; `rect` returns `null` on empty grid and saturates `br.x`; exec `display` bounds `.after` cursor rows to `terminal.rows` and saturates the CUP column | no `saturating_mul`/`scale_dimension` in `graphics_storage.rs` (grep = 0); rect still does `grid_size.rows - 1` unguarded; exec `display` unbounded | `storage: placement geometry handles untrusted dimensions` (6-part), `kittygfx placement bounds cursor movement for untrusted dimensions` |
| `f719af00c` fixed table for control keys | `KV` = `{ values: [52]u32, present: u64 }` dense table replaces `AutoHashMapUnmanaged(u8,u32)`; arena removed from `Parser` (plain `alloc`); unknown non-letter keys ignored | `graphics_command.rs:38` `type Kv = HashMap<u8, u32>`; parser holds arena-era PORT NOTEs | `ignore unknown keys (non-letter)` |
| `5ce1fe1ff` document the experiment variants | 14-line doc comment on `KV` (why dense table beats `[52]?u32`, `EnumMap`, `StaticBitSet`) | comment absent (attaches to the `f719af00c` structure) | — (R1 comment fidelity; strip nothing — it is technical, not gag prose) |

### 2.2 Formatter — 8 commits, all ABSENT (one end-state bundle)

rustty `formatter.rs` (5,965 lines) is verbatim at the Phase-3 finale shape: per-byte
`m.map.push(pin)` pin-map, tabstops emitted *after* screen content, CUP-only cursor,
`write!` formatting throughout, no `write_cell_run`.

| upstream | what changed | rustty evidence | tests |
|---|---|---|---|
| `79aa256fa` speed up formatter (+475/−88) | `fastprint` decimal emit for u8/u21 replaces `std.fmt`; `writeCellRun(emit, comptime track_points, …)` specialization; pin-map `ensureUnusedCapacity` + `appendAssumeCapacity`; `PointMap` named type; benchmark `roundtrip` mode (bench file = other area) | no `write_cell_run`/`fastprint` analogue; `write!` at `formatter.rs:412-552` etc. | (perf; roundtrip bench is the oracle's own differential check — see §6.3) |
| `2ed67cadd` redesign pin map (+267/−170) | `PinMap.Map` becomes a struct (`nodes` + `points` arrays) with `append(alloc, pin, len)` run-length API; all call sites emit (pin, byte-count) pairs instead of one pin per byte | `formatter.rs:434-437,483-490,575-583` all do `for _ in 0..discarding.count { m.map.push(pin) }`; `PinMap` at `:266` is the old `Vec<Pin>` shape | pin-map assertions across existing tests change shape |
| `7a9c369cf` preserve cursor when formatting tabstops (#13269) | tabstop emission moved **before** screen formatting (cursor restored afterwards); pin-map maps tabstop bytes to top-left | rustty emits tabstops *after* the screen (`formatter.rs:544-556`) | extended `Terminal vt with tabstops` (cursor x/y equality + map length) |
| `e523cf810` cursor home after tabstops | `\x1b[H` after tabstop loop (screen contents are top-left-relative) | absent (no `writeAll("\x1b[H")` in the tabstop path) | extended `Terminal vt with tabstops` ("hello" lands at row 0 cols 0..5) |
| `51a4311ef` clean up overzealous inlining (formatter half) | de-specializes the `point_map == null` fork of `writeCellRun` (the `Terminal.zig` `printRepeat` half is area A) | structure doesn't exist yet — folds into the `79aa256fa` end state | — |
| `997a2aff2` **preserve pending wrap** | `extra.cursor` handled *first* in ScreenFormatter extras: pending-wrap at right edge → CUP to `start_x` (spacer-tail aware) + reformat the final cell via a 1-cell `PageFormatter` instead of plain CUP; SGR/hyperlink/protection/charset restored after; TerminalFormatter re-runs `screen_formatter` with `content = .none, extra = self.extra.screen` **last** (scrolling regions move the cursor); `extra.screen` removed from the first pass | `formatter.rs:895-899` unconditional CUP in the *old* position; TerminalFormatter `format` (`:396-615`) sets `screen_formatter.extra = self.extra.screen` on the first pass and has no trailing pass | `Terminal vt cursor preserves pending wrap` (**G1b formatter drop 1 of 2**) |
| `f024d21fc` fix superfluous html newline | when the closing `</div>` is emitted, `if (blank_rows >= 1) blank_rows -= 1` in the trailing-state return | rustty `formatter.rs` trailing-state returns (`:1206-1702`) have no decrement (grep `blank_rows -= 1` = 0) | — |
| `75302feda` test for the above | — | test absent | `Page html trailing blank lines` (**G1b formatter drop 2 of 2**) |

**Bundle order:** `79aa256fa` → `2ed67cadd` → `7a9c369cf` → `e523cf810` → `51a4311ef`
→ `997a2aff2`; `f024d21fc`+`75302feda` are independent and can land first.
Note `997a2aff2` also **deletes** a line (`screen_formatter.extra = self.extra.screen`)
from the first pass — porting only the additive hunks would emit extras twice.

### 2.3 APC / fraction — 3 commits

| upstream | verdict | what changed | rustty evidence | tests |
|---|---|---|---|---|
| `946422dbc` custom fraction parsing | **ABSENT** (my slice) | `request.zig` `parseFraction` → `fraction.parse` ; `color.zig` `fromIntensity` → `fraction.parse` (rejects exponent/hex-float syntax that std parsers accept — a real behavior change, e.g. `"1e-1"` now invalid); `fraction.zig` itself is a NEW file outside my set | `apc/glyph/request.rs:993` `parse_float(value)` + range check (old shape); `color.rs:1140` `value.parse::<f64>()` — accepts exponents | fraction.zig ships its own tests (owner: terminal-core area) |
| `433b16bcb` limit glyf decode allocations | **ABSENT** | `Glossary.max_glyf_allocation = 64 KiB`; `decodeGlyfPayload(alloc, max_allocation_bytes)` wraps the decoder in `LimitedAllocator`; `limit_exceeded` → `PayloadTooLarge`; glyph.zig doc updated (FIFO-evict prose removed) | `glossary.rs:285` `req.decode_glyf_payload()` — **no limit param**; `request.rs:442` signature has no max; no `LimitedAllocator` in rustty | `Entry init rejects glyf allocation over limit`, `Entry init accepts largest glyf points allocation under limit` (+ the `testRepeatedPointRegisterReq` helper) |
| `b53728241` report unknown APC sequences | **ABSENT** (apc half) | `Handler.unknown_max_bytes` (0 = drop); new `.unknown` state capturing up to that many bytes; disabled-but-known protocols are *not* reported unknown; `glyph.zig` +3 doc lines. Stream/`lib_vt` halves (`UnknownSequence` action) are cross-area | `apc/mod.rs`: no `unknown_max_bytes`/`Unknown` state (grep = 0) | `capture unknown APC command`, `capture short unknown APC command`, `disabled known APC protocol is not unknown`, `unknown APC effect callback` (**the G1b apc drop=4**; with `433b16bcb`'s 2 Glossary tests the range adds 6 — rustty's 2 rust-only extra tests mask the difference in the gate's net arithmetic) |

### 2.4 Modes — 4 commits, all ABSENT

| upstream | what changed | rustty evidence | tests |
|---|---|---|---|
| `c092b2bcf` report DECECM as permanently reset | `getReport`: DEC private mode 117 → `.permanently_reset` before the known-mode lookup | `modes.rs:147-160` `get_report` goes straight to `mode_from_int` → 117 reports `NotRecognized` | `getReport DECECM permanently reset` (**the G1b modes drop=1**) |
| `f1ca88da3` clarify DECECM handling | inlines the `117` literal + short comment | — (comment end state superseded by `7bb375800`) | — |
| `7bb375800` document DECECM report behavior | final 9-line comment (VT520 reference, why permanently-reset beats not-recognized) | absent | — |
| `cfc19e805` configurable mode defaults (modes.zig half; C API half is area C) | `setPacked`/`getPacked` extracted; `ModeState.setDefault(mode, value)` (sets current + default); `ModeEntry.default_configurable: bool` flags (~17 modes marked false); `pub fn defaultConfigurable(mode)` | `modes.rs`: no `set_default`/`default_configurable` (grep = 0) | upstream adds C-API-side tests (area C); modes.zig itself gains the field table — port the flags exactly |

DECECM bundle: assess as one row — port `c092b2bcf`'s code+test with `7bb375800`'s
final comment text.

### 2.5 Color — 3 commits

| upstream | verdict | what changed | rustty evidence | tests |
|---|---|---|---|---|
| `7cd2f65f5` **color reset → null override** (#12755) | **ABSENT** | `DynamicRGB.reset`: `self.override = null` (was `= self.default`); regression test lives in `stream_terminal.zig` (cross-area file, same bundle) | `color.rs:686-688` `self.override_ = self.default;` — **live bug** | the OSC 11/111 + changed-default regression in `stream_terminal.zig` (coupling: stream owner) |
| `e3056658d` faster render state / C API reads (color.zig half) | **ABSENT** | new `pub fn paletteCvalSlice(src: []const RGB, dst: []RGB.C)` — 4→3-byte palette conversion, vectorized on LE, scalar tail; `paletteCval` rewritten over it. Consumed by `c/render.zig` `colors_get` (area C) | no `palette_cval_slice` in `color.rs` (grep = 0) | end-state parity: convert + LE/BE equivalence (upstream tests live with the C API half) |
| `74a233b54` wasm-target render-state fixes (color.zig half) | **N-A** | rewrites `e3056658d`'s vector loop through `@Vector`-typed pointers because **Zig 0.16 disables the LLVM auto-vectorizer** and array-typed loads scalarize | Rust keeps the LLVM auto-vectorizer: a plain indexed loop in `palette_cval_slice` will vectorize; the pointer-cast gymnastics are Zig-specific. End state assessed under `e3056658d`; if a bench shows the Rust loop scalarized, THEN revisit | — |

### 2.6 DCS / hyperlink / OSC parsers

| upstream | verdict | what changed | rustty evidence | tests |
|---|---|---|---|---|
| `f973bd53b` DECRQSS reports overline (#11638) | **ABSENT** | `Terminal.printAttributes`: `attrs` becomes `[9]u8` of *numeric* values (not ASCII digits), SGR `53` inserted between underline and blink, `writer.print("{d}")`; `dcs.zig` largest-response test grows `;53` and one `setAttribute(.overline)`; `stream_terminal.zig` +6 (cross-area) | `terminal.rs:5375-5460` `print_attributes` still `[u8; 8]` of ASCII digit bytes, no overline arm; `dcs.rs:960` expected string still `0;1;2;3;4:5;5;7;8;9;…` | the *modified* `DECRQSS largest response fits fixed buffer` — **exists in rustty at the old assertion; test-count gates will NOT catch this one** (measured: dcs 10/10 "ok") |
| `2fc238ed0` snapshot decode into pages (hyperlink.zig half) | **ABSENT** | `PageEntry.deinit`: guard both frees with `len > 0` (snapshot decode can materialize zero-length slices) | `hyperlink.rs:417-431` `PageEntry::free` frees unconditionally | — (driver + tests are `snapshot/` — area C) |
| `727b8a02f` bound OSC and grapheme allocations (my slice: 2 parser lines) | **N-A** (my slice) | `cap.writer.writeByte(0)` → `cap.writeByte(0)` in `clipboard_operation.zig`, `kitty_text_sizing.zig` — mechanical follow-on of the `osc.zig` Capture redesign (8 MiB `max_allocating_bytes`), which is **outside my file set** | rustty parsers already call an adapted `cap.write_byte(&mut parser.buffer, 0)` (`parsers/clipboard_operation.rs:17`, `parsers/kitty_text_sizing.rs:166`); the **bound itself is absent** in `osc/mod.rs` `Capture` (`:584-660`, no `max_allocating`) — that gap belongs to the `osc.zig` owner. When it ports, re-touch these two call sites if the capture API shape changes | — here; the capture-cap tests ride with `osc.zig` |
| `b867a0f59` snapshot lib.Enum enums (semantic_prompt half) | **N-A** | `ClickEvents`/`Click` become `lib.Enum(lib.target, …)` (C-ABI-stable enums for snapshot); `Click.init` → free fn `parseClick`; comments folded; **behavior identical** | rustty `parsers/semantic_prompt.rs` plain enums are the Rust equivalent; the C-ABI enum surface is area C (`lib/c_abi`, types.schema.json). No action in my area | — |

### 2.7 Input — 10 commits

| upstream | verdict | what changed | rustty evidence | tests |
|---|---|---|---|---|
| `bd647035e` no fallback text on key release | **ABSENT** | `kitty()` no-entry fallback: `if (event.action == .release) return;` before the utf8 write (dead-key/level-5 keys duplicated text in e.g. Neovim with event reporting on) | `input/key_encode.rs:281-289` — no release guard in the no-entry arm (the `:250` release check is the *plain-text* path, which upstream kept as-is) | `kitty: text fallback on release` (2-mode loop), `kitty: text fallback on repeat` (**2 of the G1b input_key_encode drop=3**) |
| `f4309055f` macOS: don't put 0x7F as text | **ABSENT** | key_encode half: new test only. Swift half: `SurfaceView_AppKit` uses `startsWithASCIIControlCharacter` (C0 **or DEL**) instead of `codepoint >= 0x20` | rustty Swift `macos/Sources/RusttyKit/Surface View/SurfaceView_AppKit.swift:1468` still `codepoint >= 0x20` (misses 0x7F); test absent | `kitty: alt+backspace emits CSI u` (consumed-alt, empty utf8 → `\x1b[127;3u`) (**3 of 3**) |
| `8b7c57c75` window title renaming (input half; GTK half area F) | **ABSENT** | `Action.prompt_window_title`, `Action.set_window_title: []const u8` (+ `.surface`-scope arms); `command.zig` palette entry (title deliberately NOT i18n-wrapped upstream) | no `PromptWindowTitle`/`SetWindowTitle` in `rustty-input/src/binding.rs` (grep = 0) | `Binding.zig` parse/format tests ride along |
| `0a183c923` edit config in a Ghostty window (input half) | **ABSENT** | `open_config` becomes `open_config: OpenConfig` (`os_open`\|`new_window`, default `os_open`); two palette commands | `binding.rs:1056` `OpenConfig` is still a void variant; `command.rs:723` single entry | parse tests for enum-arg actions |
| `a14eba747` + `c011ad870` toggle_maximize doc | **ABSENT** (fold to end state) | doc: "no effect on macOS" → "On macOS, this zooms the window…" (final text = `c011ad870`'s) | `binding.rs:1107` still "maximized windows." (old text) — R1 comment fidelity | — |
| `25b2d8a38` move_tab_to_new_window action | **ABSENT** | `Action.move_tab_to_new_window` (+ `.surface` scope arm, palette entry; note commit title says *new_tab*, action is *move_tab*) | grep `move_tab_to_new_window` in rustty-input = 0 | — |
| `8cfbaf545` formatted action parses back | **ABSENT** | `action: format` test becomes a parse-round-trip (`a.equal(Action.parse(formatted))`) with `"👻Ghostty'\""` | `binding.rs:7727` `action_format` still asserts the escaped literal `text:\xf0\x9f\x91\xbb` | the rewritten `action: format` |
| `b67f8ef51` don't escape Binding.Action.String | **ABSENT** | `formatValue` string arm: `std.zig.stringEscape` → raw `{s}` print | `binding.rs:2690-2712` `format_value_string` is a faithful port of `stringEscape` — now to be **deleted** | — (covered by the round-trip test) |
| `df23bef0e` i18n for command palette (command.zig half) | **ABSENT** | every `Command.title`/`description` wrapped in `i18n.N_`; new `Command.translated()`; Latvian po (area G) | `rustty-input/src/command.rs`: zero i18n references (grep = 0) | — |

**Bundle:** `8cfbaf545` + `b67f8ef51` must land together (the round-trip test is what
makes the un-escaping safe; on rename note **R2**: the test string `"👻Ghostty'\""`
becomes `"👻Rustty'\""` per-token). `8b7c57c75`'s palette title is intentionally not
`N_`-wrapped upstream (landed after `df23bef0e`) — preserve that exact inconsistency.

---

## 3. Atomic bundles

1. **Kitty pin-lifecycle + pending payloads** (`6760c6482` → `4371871bc` →
   `d0c516f8f` → `402b9227d` → `b5e86a428` → `8524cb593` → `b8222f4a8`): the
   `placement_count` metadata, screen-parameter threading, and pending-data union
   rewrite the same `addImage`/`addPlacement`/`evictImage` repeatedly. Port the END
   STATE of `graphics_{storage,exec,image}.zig` as one unit and carry every commit's
   tests. Landing `b8222f4a8` without `4371871bc` is impossible (it edits
   `placement_count` bookkeeping); landing `d0c516f8f` without `b5e86a428` leaves
   eviction still leaking pins (worse than a clean half: the new test suite would
   assert mixed semantics).
2. **Kitty security series** (`af2faa311`, `ec04900ab`, `f766f303a`, `590d669c4`,
   `e5840bb9b`, `d866fa455`): mostly independent of each other, but `ec04900ab`
   *reorders* the temp-file logic that `af2faa311` hardens — port `af2faa311`'s
   `isPathInDir` first, then `ec04900ab`'s open-first restructure on top (that is the
   upstream order). `d866fa455` is independent and can land immediately.
3. **Formatter** (§2.2 order). `997a2aff2` structurally depends on `7a9c369cf`'s
   extras re-ordering; `e523cf810` amends `7a9c369cf`'s hunk in place.
4. **Binding format/parse pair** `8cfbaf545` + `b67f8ef51`.
5. **DECECM trio** `c092b2bcf` (+`f1ca88da3`/`7bb375800` comment end state).
6. **`946422dbc` before `433b16bcb`** (both edit `request.zig`'s decode path; the
   limit param lands on the fraction-using shape).
7. **`f719af00c` + `5ce1fe1ff`** (doc attaches to the new KV struct).

---

## 4. Aliasing / Stacked-Borrows hazards (PORT_RULES raw-pointer discipline)

1. **The kitty screen-parameter threading is the big one.** Upstream adds
   `s: *terminal.Screen` to `addImage`/`addPlacement`/`addPendingImage`/`evictImage*`
   /`deinit`-time sweeps, and callers pass `terminal.screens.active` — but rustty's
   `ImageStorage` lives INSIDE `Screen` (`screen.kitty_images`). A safe-reference port
   (`fn add_placement(&mut self, …, s: &mut Screen)`) is instant UB: `self` is
   reachable through `s`. rustty's existing `delete()` already shows the sanctioned
   shape — `t: &mut Terminal` + `unsafe { &mut *t.screens.active }` deref at the use
   site (`graphics_storage.rs:678`), i.e. the `rust-mut-args-strongly-protected`
   raw-pointer-argument pattern. The new `s` parameters must be `*mut Screen`
   (or the terminal), with narrow-scope derefs around each `placement.deinit(s)` /
   `pages.untrackPin` call and **no** `&mut Screen` live across re-entry into
   `self.placements`/`self.images`. Prove the sweep paths on the isolated Miri
   harness (`.port/phase-2/mirihar-chunk3b` precedent) before wiring to FFI.
2. **`402b9227d`'s garbage sweep + `b8222f4a8`'s `removePlacementsByImageId`** iterate
   `self.placements` while removing entries *and* mutating `self.images[…]
   .metadata.placement_count` through `removePlacementByPtr`. Zig's `removeByPtr`
   during iteration is legal for its hash map; Rust's `HashMap` iteration is not.
   rustty's existing delete arms already collect keys into a `Vec<PlacementKey>` first
   (`graphics_storage.rs:671`) — keep that shape; `placements` and `images` are
   disjoint fields, so the metadata decrement is a plain second field borrow.
3. **`997a2aff2` pending-wrap reformat**: ScreenFormatter (holding `screen: *const
   Screen` raw) constructs a 1-cell `PageFormatter` from
   `cursor.page_pin.node.page()` — a second alias into page memory while the screen
   alias is live. Both are read-only; keep the PageFormatter's page access via the pin's
   own node pointer (as the existing rustty formatter does for content pages), never
   via a fresh `&mut`.
4. **`2ed67cadd` pin-map struct**: the new `PinMap.Map` stores `node` pointers with
   run lengths. Same invalidation contract as the old per-byte vec (nodes die on
   reflow); no new hazard, but the `append(alloc, pin, len)` API must not hold a
   `&Pin` into the map across a growth of either internal array.
5. **`ec04900ab`**: deriving the canonical path *from the open handle* has no direct
   Rust std equivalent (`File::realPath`): macOS `fcntl(F_GETPATH)`,
   Linux `/proc/self/fd/N` readlink. Platform-gate exactly like the existing
   `readSharedMemoryPosix` cfg split; do not fall back to re-resolving the submitted
   path (that reintroduces the TOCTOU the commit exists to close).
6. **R7 discipline** is the substance of `e5840bb9b`/`f766f303a`: `saturating_mul`,
   `u64`/`i64` widening before arithmetic, `try_into`-style checked casts — map
   `std.math.mul/cast` 1:1; no `as` narrowing in the new geometry code.

---

## 5. Dropped-test reconciliation (G1b `w0/g1b-after-at-12967b68f.txt`)

| module | drop | exact targets (this area) |
|---|---|---|
| formatter | 2 | `Terminal vt cursor preserves pending wrap` (`997a2aff2`), `Page html trailing blank lines` (`75302feda`) — confirmed by name-by-name inventory diff (102 vs 100, no other gaps) |
| apc | 4 | `capture unknown APC command`, `capture short unknown APC command`, `disabled known APC protocol is not unknown`, `unknown APC effect callback` (`b53728241`) **plus** `Entry init rejects glyf allocation over limit`, `Entry init accepts largest glyf points allocation under limit` (`433b16bcb`) — 6 missing; net 4 because rustty carries rust-only extras. All 6 must land |
| kitty | 11 | the ~23 range-added tests enumerated in §2.1 (net 11 for the same reason). Representative: the 4 pending-image tests, `evicts images in priority order`, `placement count limit permits replacement`, the 3 shm-range tests, both png-limit tests, both interior-row-column deletion tests, both temp-dir-boundary tests, blocklist-after-symlink-swap, both untrusted-geometry tests, `ignore unknown keys (non-letter)`, retransmit-removes-placements, the pin-release trio |
| modes | 1 | `getReport DECECM permanently reset` (`c092b2bcf`) |
| input_key_encode | 3 | `kitty: text fallback on release`, `kitty: text fallback on repeat` (`bd647035e`), `kitty: alt+backspace emits CSI u` (`f4309055f`) |

**Gate blind spot worth recording:** `f973bd53b` and `d866fa455`/`7cd2f65f5` change
*assertions inside existing tests* (dcs 10/10 "ok", kitty range tests exist at old
assertions). G1b cannot see these; only porting the amended test bodies does.

---

## 6. Cross-area couplings and ordering

**I depend on (cannot land my rows first):**

| my item | blocking area/owner | needed |
|---|---|---|
| `433b16bcb`, `590d669c4` | datastruct owner (wasm-allocator-infra area) | `LimitedAllocator` (`src/datastruct/limited_allocator.zig`, new in range) with `limit_exceeded` observable |
| `946422dbc` | area A (terminal core) | `src/terminal/fraction.zig` port (+ its tests) |
| `6760c6482` C-API semantics | area C | header doc + `image_get` NO_VALUE-for-pending behavior must match the union I port |
| `7cd2f65f5` regression test | stream/`stream_terminal.zig` owner | the OSC 11/111 test lives there; land as one bundle |
| `b53728241` stream half | stream owner + area C | `Stream.UnknownSequence` action, `lib_vt` export, effect callback; my apc half alone is inert (nothing sets `unknown_max_bytes`) |

**Others depend on me / expect my surfaces:**

- **Area C:** `cfc19e805` (`setDefault`/`defaultConfigurable` feed
  `GHOSTTY_TERMINAL_OPT_MODE_DEFAULT`); `e3056658d` (`paletteCvalSlice` feeds
  `colors_get`); `402b9227d`/`e5840bb9b`/`b5e86a428`/`6760c6482` c/kitty_graphics
  halves; `2fc238ed0` (snapshot decode relies on the zero-len free guards).
- **Area F (GTK/macOS):** `8b7c57c75`/`0a183c923` apprt+GTK halves;
  `f4309055f` Swift half (`SurfaceView_AppKit.swift:1468` still `>= 0x20` — fix +
  `startsWithASCIIControlCharacter` extension in RusttyKit).
- **Area G (i18n/po):** `df23bef0e`'s `.po` wave and the `i18n.N_`/`_` runtime my
  `command.rs` wrap needs.
- **osc.zig owner:** the 8 MiB capture cap (`727b8a02f`); my two parser call sites
  follow whatever capture API shape lands.

**Recommended order within area D:**

1. Immediately, standalone, highest severity-per-line: `d866fa455` (one-char + tests),
   `7cd2f65f5` (one-line + cross-file test), `f973bd53b`, `2fc238ed0` guard,
   `f024d21fc`+`75302feda`, `bd647035e`, DECECM trio, `a14eba747`/`c011ad870` docs.
2. Kitty security series (bundle 2), then kitty pin-lifecycle bundle (bundle 1) —
   both gated on the raw-pointer screen-param decision (§4.1) and a Miri pass.
3. Formatter bundle (§2.2), then `f719af00c`+`5ce1fe1ff`.
4. Input actions (`25b2d8a38`, `8b7c57c75`, `0a183c923`) alongside their F halves;
   `8cfbaf545`+`b67f8ef51`; `df23bef0e` after area G's i18n runtime.
5. `946422dbc` → `433b16bcb` once `fraction.zig` and `LimitedAllocator` exist;
   `b53728241` with the stream owner; `cfc19e805`/`e3056658d` with area C.
