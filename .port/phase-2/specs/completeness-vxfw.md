# Completeness spec — vaxis `vxfw` subset + `boo` easter-egg + `framedata` asset

Status: PLAN (durable record). Un-defers `crates/rustty-cli/src/boo.rs` (DEFERRALS
"cli C5" — the crate's last live vaxis-deferred leaf) by porting the vaxis `vxfw`
widget-framework SUBSET that `boo` consumes, plus reproducing the `framedata`
compressed frame asset.

Oracle (READ-ONLY):
- `ghostty/src/cli/boo.zig` (236 LOC) — the `+boo` action.
- vendored vaxis-0.5.1 vxfw
  (`~/.cache/zig/p/vaxis-0.5.1-BWNV_LosCQAGmCCNOLljCIw6j6-yt53tji6n6rwJ2BhS/src/vxfw/`):
  - `vxfw.zig` (578) — the core: `Widget`/`EventContext`/`DrawContext`/`Surface`/
    `SubSurface`/`Event`/`Command`/`Tick`/`Size`/`MaxSize`/`Point`/`RelativePoint`/
    `HitResult`/`CursorState`/`FlexItem`/`UserEvent` + `Text`/`Center` re-exports.
  - `App.zig` (602) — the run loop + `MouseHandler` + `FocusHandler`.
  - `Text.zig` (508) — `Text` widget + `LineIterator` + `SoftwrapIterator` (+ 10 tests).
  - `Center.zig` (113) — `Center` widget (+ 1 doctest).
- `ghostty/src/build/GhosttyFrameData.zig` (75) + `ghostty/src/build/framegen/main.c`
  (the asset generator) + `ghostty/src/build/framegen/frames/*.txt` (235 frames,
  1,390,810 bytes uncompressed).

YAGNI scope guard: the OTHER 13 vxfw widgets (Border/Button/FlexColumn/FlexRow/
ListView/Padding/RichText/ScrollBars/ScrollView/SizedBox/Spinner/SplitView/TextField)
are UNUSED by ghostty → OUT OF SCOPE. Port only the 4 files `boo` transitively needs.

---

## 1. Landing (port targets)

New module tree `crates/rustty-vaxis/src/vxfw/` (a new tier ABOVE `window`/`widgets`;
`vxfw.App` drives the already-ported `Tty`/`Vaxis`/`Loop`/`Window`):

- `crates/rustty-vaxis/src/vxfw/mod.rs`   ← PORTS `vxfw.zig` (the core value/interface
  types + `pub mod`/`pub use` for `app`/`text`/`center`; the `SubSurface: containsPoint`
  + `Surface: satisfiesConstraints` tests). ~430 LOC of live types.
- `crates/rustty-vaxis/src/vxfw/app.rs`   ← PORTS `App.zig` (`App`, `MouseHandler`,
  `FocusHandler`; the run loop). The FFI/thread path.
- `crates/rustty-vaxis/src/vxfw/text.rs`  ← PORTS `Text.zig` (`Text` + `LineIterator` +
  `SoftwrapIterator` + all 10 tests, assertion-for-assertion).
- `crates/rustty-vaxis/src/vxfw/center.rs`← PORTS `Center.zig` (`Center` + doctest).
- `crates/rustty-vaxis/src/lib.rs`        ← add `pub mod vxfw;` + module-header doc.
- `crates/rustty-cli/src/boo.rs`          ← REWRITE: un-defer; port `boo.zig` LIVE
  (Boo widget, span-parser `updateFrame`, `decompressFrames`, `run`).
- `crates/rustty-cli/build.rs`            ← EXTEND (already exists for helpgen): add the
  framedata generator (port of `GhosttyFrameData.zig` + `framegen/main.c`).
- `crates/rustty-cli/assets/framegen/frames/*.txt` ← VENDOR the 235 frame files
  (byte-faithful asset; R1). `crates/rustty-cli/assets/framegen/frames` is the input,
  `$OUT_DIR/framedata.compressed` the regenerable output (gitignore the OUT_DIR blob).
- `crates-sys/zlib-sys/src/lib.rs`        ← EXTEND: add raw-DEFLATE FFI + safe wrappers
  (see §3). `crates/rustty-cli/Cargo.toml` gains `zlib-sys` as BOTH a `[dependencies]`
  (runtime `inflate_raw`) and `[build-dependencies]` (build-time `deflate_raw`) entry.

Acyclicity (R4/G7): `rustty-cli → rustty-vaxis` already exists (WIRE1/WIRE2). vxfw is a
NEW top module of rustty-vaxis; no new crate edge. `cli → zlib-sys` is a downward edge to
a `-sys` leaf (rustty-vt already depends on zlib-sys) — acyclic.

---

## 2. Type-mapping decisions (the load-bearing seams)

### 2a. `vxfw::Event` and `LoopEvent` (`vxfw.zig:36`)
The reference `Event` union has 18 variants; `App` instantiates `vaxis.Loop(vxfw.Event)`.
Port `vxfw::Event` as a Rust enum. Precedent: `list_themes.rs` `Event` + `impl LoopEvent`.
- `KeyPress` MUST carry an OWNED `KeyOwned` (NOT borrowed `Key<'a>`) — the queued `T` is
  `Send + 'static` (loop_.rs `LoopEvent` bound). Boo's handler needs a borrowed `Key<'_>`
  for `.matches(...)` → reuse the `borrow_key` helper (list_themes precedent).
- `impl LoopEvent for Event` supplies `Some(..)` for the variants the read thread posts
  (`key_press`,`key_release`,`mouse`,`winsize`,`focus_in`,`focus_out`,`mouse_leave`,
  `paste_start`,`paste_end`,`paste`,`color_report`,`color_scheme`); the rest default
  `None` ≡ Zig `@hasField` false.
- `init`/`tick`/`mouse_enter`/`app` have NO `LoopEvent` constructor: `init`/`focus_in`
  are hand-posted via `loop.post_event(..)`; `tick` is delivered directly through
  `widget.handle_event(ctx, Event::Tick)` from the timer heap (never queued).
- `Paste(Vec<u8>)` OWNS its bytes (event.rs precedent — no "caller must free").

### 2b. `Widget` — the type-erased interface (`vxfw.zig:270`)
Zig: `userdata: *anyopaque` + 3 optional/one-required fn pointers. Port as a `Copy`
struct:
```
pub struct Widget {
    userdata: *mut c_void,
    capture_handler: Option<unsafe fn(*mut c_void, *mut EventContext, Event) -> Result<(), Error>>,
    event_handler:   Option<unsafe fn(*mut c_void, *mut EventContext, Event) -> Result<(), Error>>,
    draw_fn:         unsafe fn(*mut c_void, DrawContext) -> Result<Surface, AllocError>,
}
```
`eql` compares `userdata` + `draw_fn` addresses (`@intFromPtr`). The `anyerror!void`
handlers → `Result<(), vxfw::Error>` (a small error enum: `OutOfMemory` + the wrapped tty
errors App swallows via `std.log.err`). `Allocator.Error!Surface` → `Result<Surface, _>`;
panic=abort semantics preserved (asserts abort). Each widget's `fn widget(&self)` builds a
`Widget` with `userdata = self as *mut _ as *mut c_void`. `Center`/`Text` use
`@constCast(self)` (draw-only, no event handler); `Boo` passes `self` mutably.

### 2c. `Surface` — RAW slices, NOT `&'a` (`vxfw.zig:...`) — THE central fidelity call
`Surface{ buffer: []vaxis.Cell, children: []SubSurface }`. Zig slices here are UNPROTECTED
and the App STORES a `Surface` (`mouse_handler.last_frame = surface`) that OUTLIVES the
`arena.reset(.free_all)` at the top of the next redraw — a `&'a`-lifetime model cannot
express this soundly. DECISION: port `Surface.buffer`/`children` as RAW slices
(`RawSlice<Cell<'g>>` = `{ ptr: *mut, len }`, or `NonNull`+len) to faithfully mirror Zig's
unprotected pointers; the access DISCIPLINE (never dereference stale post-reset pointers)
is what the mirihar proves (§4). `Cell<'g>` still threads the grapheme lifetime `'g` (the
frame bytes borrow from `decompressed_data`), but Surface holds the slice as raw so the
borrow checker does not tie it to the arena. Methods `write_cell`/`read_cell`/`trim_height`/
`hit_test`/`render`/`empty`/`init`/`create_buffer`/`init_with_children`/`satisfies_constraints`
port over the raw slices with `unsafe` blocks + SAFETY notes anchored to the mirihar.

### 2d. `DrawContext.arena` — a bump arena with `reset(free_all)` (`vxfw.zig:...`)
The reference `arena: std.mem.Allocator` (an `ArenaAllocator`) allocs `[]SubSurface`/
`[]Cell` per frame and is `reset(.free_all)` between frames. Rust std has no arena, and the
existing `pretty_arena::Arena` is byte-only + never resets. NEW facility: a `vxfw` bump
arena (`vxfw/mod.rs` or a tiny `vxfw/arena.rs`) exposing `alloc_slice::<T: Copy>(&self, n)
-> *mut [T]` (raw, to match §2c) + `reset(&mut self)` (drop all chunks). It hands out RAW
slices — reset invalidates them by construction; the App guarantees no read-after-reset
(proven on the mirihar). `DrawContext` is `Copy` and carries `arena: *const VxfwArena`
(shared, `withConstraints` re-wraps the same arena ptr) + `min: Size`/`max: MaxSize`/
`cell_size: Size`. The `width_method` is a Zig FILE-LEVEL `var` (global) with `init`/
`stringWidth`/`graphemeIterator` — port as a module static (e.g. `AtomicU8`/`OnceCell`
over `gwidth::Method`) set by `DrawContext::init`, matching the reference's process-global
(App calls it once before the loop; single-threaded main-thread render).

### 2e. `Boo.buffer` (`boo.zig:...`)
`[frame_width*frame_height]vaxis.Cell = [100*41] = 4100` cells, heap-allocated in the
reference to keep binary size down. Port as `Box<[Cell<'g>]>` / `Vec<Cell<'g>>` of len
4100, `'g` = `decompressed_data` lifetime. `updateFrame` writes `self.buffer[i]` where the
`char.grapheme` slices borrow into `frames[self.frame]` (→ `decompressed_data`). The draw's
child `Surface.buffer = &self.buffer` (raw alias into Boo per §2c).

### 2f. `App` construction + run-loop wiring (`App.zig`)
Mirror the PROVEN `list_themes::run_preview` discipline (mirihar-wire2): stack-pin
`tty`/`vx`, take `&raw mut` ONCE, drive ALL access through transient `&mut *ptr` reborrows
sharing the read thread's `SharedReadWrite` tag (never a `Unique` that pops the reader's
raw tag). `App::init` heap-context is not required (Rust locals are stable); flatten the
reference's `App` struct over run-scope locals as list_themes did, OR keep an `App` struct
Boxed if stable pointers simplify `MouseHandler`/`FocusHandler` back-refs. The
`std.Thread.sleep` frame pacing → `std::thread::sleep`. `timers` heap = `Vec<Tick>` +
`sort_by`/insertion sort (Tick.lessThan is DESCENDING — pop = soonest deadline).

---

## 3. `framedata` asset plan (compression format + reproduce)

Format (from `framegen/main.c`): read `frames/*.txt`, keep only `*.txt`, SORT by name
(`strcmp`), JOIN with a single `\x01` (SEPARATOR) between frames (234 separators for 235
frames), then **raw DEFLATE** via zlib `deflateInit2(Z_DEFAULT_COMPRESSION=-1, Z_DEFLATED=8,
windowBits=-MAX_WBITS=-15, memLevel=8, Z_DEFAULT_STRATEGY=0)` — the negative windowBits =
NO zlib/gzip wrapper (bare DEFLATE). `GhosttyFrameData.zig` runs that C exe at build time
and `@embedFile`s the output as `framedata.compressed`. Runtime `boo.decompressFrames` uses
`std.compress.flate.Decompress` in `.raw` mode, then `splitScalar('\x01')` into 235 frames.

Fidelity note: only the DECOMPRESSED frames are fidelity-load-bearing (DEFLATE is lossless;
any valid raw-deflate of the joined blob inflates back to identical bytes). The COMPRESSED
blob need not be byte-identical to ghostty's — but reproducing the generator from the
vendored frames IS the faithful port of `GhosttyFrameData.zig` + `main.c`.

DECISION (recommended — Option A "reproduce the generator"):
1. Vendor the 235 `frames/*.txt` under `crates/rustty-cli/assets/framegen/frames/`
   (byte-faithful source asset; `cargo:rerun-if-changed` on the dir).
2. `build.rs` (port of `GhosttyFrameData.zig` + `framegen/main.c`): read+sort+join(`\x01`)
   → `deflate_raw` (zlib-sys, §below) → write `$OUT_DIR/framedata.compressed`.
3. `boo.rs`: `const FRAMEDATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"),
   "/framedata.compressed"));` (faithful image of `@embedFile("framedata.compressed")`).
4. `boo.decompressFrames` → `zlib_sys::inflate_raw(FRAMEDATA, MAX)` then split on `\x01`.
This ports BOTH the build-side (GhosttyFrameData/main.c) AND runtime-side (decompressFrames)
faithfully, keeps the binary small (compressed, per the reference's stated intent), and the
compressed blob is regenerable (gitignore `$OUT_DIR` — memory `gitignore regenerable
corpora`). Fallback Option B (vendor a pre-generated `framedata.compressed`, need only
`inflate_raw`) is NOT chosen: it ships an opaque blob and does not port the generator.

zlib-sys EXTENSION (`crates-sys/zlib-sys/src/lib.rs`) — R3-clean (same vendored zlib 1.3.1
the reference links; rustty-vt already consumes it):
- Add `inflate_raw(data, max_size)` = existing `inflate_zlib` but `window_bits = -MAX_WBITS`
  (or parameterize the private helper by window_bits and keep `inflate_zlib` wrapping it).
- Add DEFLATE FFI: `deflateInit2_`, `deflate`, `deflateEnd`, `compressBound` (+ consts
  `Z_FINISH=4`, `Z_DEFLATED=8`, `Z_DEFAULT_COMPRESSION=-1`, `Z_DEFAULT_STRATEGY=0`,
  `Z_STREAM_END=1`) and a safe `deflate_raw(data) -> Vec<u8>` wrapper mirroring `main.c`
  (windowBits=-15, memLevel=8). Verify the vendored zlib is built with deflate symbols
  (it is — full zlib; `build.rs` compiles `deflate.c`/`trees.c`). Add a known-answer test:
  `inflate_raw(deflate_raw(x)) == x`.

---

## 4. Hazards + Miri/ASan plan

Two DISJOINT proof surfaces:

### 4a. FFI/thread path (App.run) — ASan + the existing mirihar-wire2 precedent
`App::run` drives real `Tty` termios/ioctl FFI + a read thread reading the tty — Miri CANNOT
execute foreign fns/threads over real fds. The raw-ptr `tty`/`vx`/`loop` aliasing discipline
is ALREADY PROVEN by `.port/phase-2/mirihar-wire2` (SB+TB). NEW FFI in this workstream is
only the zlib `deflate`/`inflate` calls — prove under ASan (a small harness compressing then
decompressing the joined frames, `-Zsanitizer=address`), same class as existing zlib-sys use.

### 4b. NEW pure-memory hazards — the `mirihar-vxfw` harness (Miri PRIMARY)
Isolate the widget/surface/arena machinery WITHOUT the tty/thread and drive the EXACT
`App.run` per-frame sequence under BOTH Stacked AND Tree Borrows
(`.port/phase-2/mirihar-vxfw/`, mirror the wire2 standalone-crate layout):

- **H1 — type-erased `Widget` re-entry.** `*mut Boo` stored in `Widget.userdata:*mut c_void`,
  cast back to `&mut Boo` via `unsafe fn(*mut c_void,...)` on each tick/draw, INTERLEAVED with
  a `Surface` holding a raw alias into `Boo.buffer`. Prove: routing `updateFrame` through the
  userdata's OWN pointer with narrow field access (raw-self helpers, memory
  `shaper-hook-sb-clean-interleave` + `rust-mut-args-strongly-protected`) — never a live
  `&mut Boo` spanning a read of the buffer alias — is SB/TB clean. A `&mut Boo` reborrow that
  covers the whole struct while a raw alias into `.buffer` is live = the UB shape to REJECT.

- **H2 — arena `Surface` stored across `reset(free_all)`.** `mouse_handler.last_frame =
  surface` retains a `Surface` whose `children`/(child)`buffer` point into arena memory;
  the next redraw does `arena.reset(.free_all)` BEFORE re-`doLayout`. Prove: `Surface` slices
  are RAW (§2c) so no live reference outlives the reset, AND `last_frame`'s stale pointers are
  never dereferenced after reset — they are OVERWRITTEN each redraw before any `hitTest` read.
  Model: draw→store last_frame→reset arena→re-alloc→(mouse) hitTest, asserting the read path
  only ever touches the CURRENT frame's live allocation. Reproduce + REJECT the buggy shape
  (hitTest against a pre-reset `last_frame`).

- **H3 — `Surface.buffer = &self.buffer` alias vs `updateFrame`'s `&mut self.buffer[i]`.**
  Boo's child surface aliases `Boo.buffer` (raw) while the next tick's `updateFrame` writes it
  through the struct. Prove temporal non-overlap via the raw-self helper (write through the
  buffer's own raw ptr with a narrow `&mut [Cell]` slice access, never `&mut *self` spanning
  the alias). Same class as chunk3b/wire2.

Harness shape: a fake `VxfwArena` (real bump + reset), a fake in-memory "loop" feeding
`Event::{Init,Tick,KeyPress}`, a `Boo` over a small stub frame set (no zlib, no tty), calling
`draw`/`updateFrame`/`hit_test`/`render`-into-a-plain-`Screen` — exercising H1/H2/H3. Also
fold `Center::draw`/`Text::draw` (arena alloc of children/buffer + the child buffer alias)
into the harness. Run: `cargo +nightly miri run` with `MIRIFLAGS="-Zmiri-tree-borrows"` AND
default Stacked Borrows; BOTH must be clean.

---

## 5. Test port (assertion-for-assertion — count is a gate)

Port EVERY reference test (R1; a count regression is a failure):
- `Text.zig`: `SoftwrapIterator: LF breaks` / `soft breaks that fit` / `... longer than
  width` / `... with leading spaces`; `LineIterator: LF/CR/CRLF/CRLF-empty-line`; `Text`
  doctest (softwrap size + non-softwrap ellipsis) → 10 tests in `text.rs`.
- `Center.zig`: `Center` doctest (two constraint cases: 10×10 and 5×3 centering) → 1 test.
- `vxfw.zig`: `SubSurface: containsPoint`, `Surface: satisfiesConstraints` (+ the
  `refAllDecls`/doctest-existence meta-tests are Zig-AST-specific → NOT ported; note in a
  comment) → 2 tests in `mod.rs`.
- `App.zig`: no reference unit tests (excluded from the meta-test) — behavior covered by the
  mirihar + the manual run smoke.
- `boo.zig`: 0 reference tests (matches DEFERRALS "cli_boo 0/0"). Add a rustty-only
  `decompressFrames` round-trip test (235 frames, each 41×100 after span-strip; assert
  `updateFrame` reaches `cell_idx == 4100`) guarded so it runs headless (no tty).

`Text.zig` tests depend on `gwidth`/grapheme iteration already ported (`DrawContext::init`
+ `stringWidth`/`graphemeIterator` delegate to `gwidth`/`unicode`). Verify `stringWidth`/
grapheme paths match the reference widths the tests assert (e.g. `…` width 1, `\t`→8).

---

## 6. Acceptance

- `cargo build -p rustty-vaxis -p rustty-cli --workspace` GREEN.
- `cargo test -p rustty-vaxis vxfw::` — all ported vxfw tests pass; ported COUNT ≥ reference
  (13 vxfw + 1 boo round-trip), no regression.
- zlib-sys: `cargo test -p zlib-sys` incl. the new `deflate_raw`/`inflate_raw` round-trip KAT.
- `mirihar-vxfw`: `cargo +nightly miri run` clean under BOTH Stacked and Tree Borrows
  (H1/H2/H3, incl. the reject-the-buggy-shape assertions).
- ASan: the zlib compress+decompress harness clean under `-Zsanitizer=address`.
- Manual smoke: `rustty +boo` renders the animation (≥100w×41h) / shows the size warning
  below that / quits on `Ctrl-C`/`Esc` — run under the project `/run` skill (TUI).
- Comment-fidelity (scoped-G3b) GREEN over the new files; acyclic (G7); clippy -D; fmt.
- Update `.port/phase-2/DEFERRALS.md`: retire "cli C5 boo" (crate's last live seam) and the
  rustty-vaxis "vxfw-deferred" note; record the framedata generator + the mirihar-vxfw proof.

---

## 7. Risks / decisions needing sign-off

- **Surface = raw slices vs `&'a`** (§2c): the App-stores-across-reset invariant forces raw
  slices for faithfulness. If a reviewer prefers a lifetime-bound model, `last_frame` storage
  must be redesigned (diverges from the reference control flow) — flag before coding.
- **New arena facility** (§2d): no std arena; a `vxfw`-local bump arena with `reset` is new
  code (not a direct port of a reference file — Zig's `ArenaAllocator` is std). Keep it
  minimal + Miri-proven; justify as the faithful image of `std.heap.ArenaAllocator`.
- **zlib deflate FFI** (§3): confirm the vendored zlib build exports deflate symbols (it
  compiles full zlib); if trimmed, either re-enable deflate.c/trees.c or fall back to Option B
  (vendored `framedata.compressed`, inflate-only). Decision point before build.rs work.
- **`DrawContext.width_method` global**: a process-global `var` in the reference; modeled as
  a module static. Sound because render is single-threaded main-thread and set once pre-loop;
  note the assumption.
- **App.run is not Miri-executable** (FFI+thread): relies on the wire2 precedent + ASan for
  the new zlib FFI. If the run loop introduces a NEW aliasing shape beyond wire2 (e.g. the
  `MouseHandler`/`FocusHandler` back-references into `App`), extend the mirihar to model it.
