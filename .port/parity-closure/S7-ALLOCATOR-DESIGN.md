# S-7 — caller allocator for libghostty-vt internals: design study (S-7a)

Status: DESIGN STUDY with a measured prototype, IMPLEMENTED by tasks S7-1..S7-9
(see §9 "Outcome"). The body below is the study as written; §9 records what shipped.
Pin: `20abdb50a`. Branch `tail-s7a` (this document); prototype branch
`tail-s7a-proto` (NOT FOR MERGE: `045dd4f0` prototype, `8f150e0d` harness).

Rows this design must close: **S174-F3** (OSC parser, narrowed), **S175-F1**
(RenderState), **S176-F1** (search), **S179-F2** (terminal internals), **S4-F2**
(include-side twin of S175-F1/S176-F1). Also in scope, because it is the same
deferral (`.port/phase-2/DEFERRALS.md` "F-008/W12-01 Tier 2"): item 3,
selection-gesture event codepoints.

## 1. What the reference does (at the pin)

### 1.1 Plumbing

- `lib.alloc.default(c_alloc_)` (`src/lib/allocator.zig:22-44`) returns
  `c_alloc.zig()` for a non-NULL allocator. `zig()` (`:78-83`) builds a
  `std.mem.Allocator{ .ptr = <the caller's GhosttyAllocator*>, .vtable =
  &zig_vtable }`. No copy of the struct is taken; every call re-reads
  `ctx`/`vtable` through the caller's pointer. So **the caller's
  `GhosttyAllocator` must outlive every object built from it**. The header
  (`include/ghostty/vt/allocator.h`) does not say this (N260).
- NULL selects `std.heap.c_allocator` when libc is linked (the port maps this to
  the Rust global heap, `GlobalZigAllocator`; already documented in
  `rustty-lib/src/allocator.rs` `default`).
- A `std.mem.Allocator` is a `{ptr, vtable}` **runtime value**, passed by value
  and stored in objects (`Screen.alloc`, `MemoryPool.alloc`,
  `RenderStateWrapper.alloc`, `SearchWrapper.alloc`, `Parser.alloc`). One
  terminal has one allocator: `Terminal.gpa()` = `screens.active.alloc`
  (`Terminal.zig:455-457`), and every screen gets the `alloc` from
  `ScreenSet.init`.
- Each C object has its **own** allocator: a render state or search created with
  allocator X over a terminal created with allocator Y allocates its own state
  through X (`c/render.zig:199-232`, `c/search.zig:263-267`).
- Failure: the header promises libghostty-vt "is resilient to allocation
  failures and will gracefully handle out-of-memory situations by returning
  error codes" (`allocator.h`). Every growth is `try`, surfacing as
  `GHOSTTY_OUT_OF_MEMORY`, or as the stream's log-and-drop, or as the OSC
  capture's fixed-buffer fallback.
- Call shape (Zig 0.16 std, read in the local 0.16.0 std): `ArrayList` growth
  tries `remap` before alloc+copy+free. `Allocator.realloc` tries `remap`
  first. `ArenaAllocator` tries the child's `resize` in place, then allocates a
  new block at 1.5x. `MultiArrayList.setCapacity` and `HashMap.grow` always do
  alloc+copy+free. `MemoryPool` is an arena plus a free list, and returns
  memory only at `deinit`/`reset`.

### 1.2 What goes to the caller allocator, and what does not

I read these files directly: `lib/allocator.zig`, `c/terminal.zig` `new_`/`wrap`,
`c/render.zig`, `c/search.zig`, `c/osc.zig`, `PageList.zig:120-200`, `:543-563`,
and `page.zig:30-80`, `:225-250`. A read-only enumeration pass over all four
families found the rest. Its citations are carried below unchanged; I did not
re-read them line by line.

**NOT caller-routed (on native targets):**
- Page memory: the standard-size `PagePool` pages and the non-standard "heap"
  pages both come from `pageAllocator(alloc)` (`PageList.zig:548-563`). That
  is `std.heap.page_allocator`, or the tagged mach allocator on macOS. It is the
  caller allocator only on native **freestanding** targets, or in tests.
- `Page.init`/`Page.clone` use raw mmap (`page.zig:35-80`, `:234-250`).
- Everything that lives inside page memory: styles, graphemes, hyperlink
  map/set, strings (their own offset allocators, `page.zig:192-215`).
- Compression scratch (a page-pool item or the page allocator).
- The debug-only `assertIntegrity` `DebugAllocator`.

The port already maps `pageAllocator` faithfully
(`rustty_os::page_allocator`, `pagelist/mod.rs` `MemoryPool::init`), so **this
whole class is out of scope and already correct**. It is also where almost all
the bytes are.

**Caller-routed:**

| family | containers (approx.) | hot? |
|---|---|---|
| Terminal (`t.init(alloc)`) | about 25 containers. PageList `NodePool` + `PinPool` (arenas over `gen_alloc`) and the `tracked_pins` map. Compressed page bytes (`compress/Page.zig:138`), and `pagePreservingState(alloc)` decode buffers (whichever allocator the caller passes). `Tabstops.dynamic_stops` (>512 cols). `pwd`, `title`, `glyph_glossary`, `kitty_dnd`. `cursor.hyperlink`. Kitty graphics storage (images/placements maps, loading, image data, animations, restores; PNG decode through a `LimitedAllocator` over `gpa`). APC/DCS builders. The stream's OSC parser. Clipboard grants and write arena. The OSC 52 content array. The wrapper's `tracked_grid_refs` and `searches` maps. stackFallback overflow (falls back to `gpa()`) | Printing allocates nothing while the page has room. Scrolling allocates about one node per page from the caller arena (the page comes from the page allocator). Pins are allocated per selection, grid ref or search. The rest is per command |
| RenderState (`state.alloc`) | about 6: `row_data` MultiArrayList, per-row `cells` MultiArrayList, per-row arena (grapheme dupes), `pending_styles`, per-row `applied_styles`, plus the iterator wrappers | Growth only on a dimension change or when capacity is exceeded; capacity is retained |
| Search (`wrapper.alloc`) | about 12 own: TerminalSearch, viewport matches, fingerprint entries, per-screen results, the SlidingWindow `data`/`meta` CircBufs, needle, overlap buffer, chunk buffer, `cell_map`s, a full-page decode buffer for compressed nodes. **4 charged to the TERMINAL's allocator**: the `searches` map entry and three `trackPin`s (PageListSearch start, history `start_pin`, selected-match highlight) (N262) | `feed`/`viewportMatches` |
| OSC parser (`parser.alloc`) | 4: the parser struct, the allocating capture (done, S174-F3 part 1), `color_operation.requests` (`SegmentedList(Request, 2)`: allocates only above 2 requests), `kitty_color_protocol.list` (ArrayList). Text payloads are **borrowed** slices into the parser's buffer or capture, with no allocation | per OSC |

## 2. What the port does today

- rustty-vt spells `std.mem.Allocator` as `use allocator_api2::alloc::Global as
  Allocator;` in 28 files. That is a zero-sized handle that is threaded
  through the same signatures as the reference and ignored almost everywhere
  (`let _ = alloc`). Real storage is std `Vec`/`HashMap`/`Box`. Only a few
  types are already allocator-generic (`Tabstops<A>`, `dcs::Handler<A>`, the
  zigstd `MemoryPool<T, A>`), and even those are instantiated at `Global`.
- `PageList::init(opts)` lost the reference's `alloc` parameter, which has 336
  call sites, almost all tests (N264).
- vt-c already routes the **wrapper storage** (Tier 1), the stream's OSC
  capture (`osc::CaptureAllocator`), and `TrackedGridRef` through the caller
  allocator. Everything in §1.2's table (the internals) is on the Rust heap.
- OOM: std containers abort (DG5 abort policy). The port adapts 26
  reference `FailingAllocator` test sites into about 66 port-side notes and
  seams across 20 files, because there is no failable allocator to pass.
- OSC `Command` text payloads are owned `Vec<u8>` copies (about 15 variants,
  `osc/mod.rs:52-211`). These are allocations the reference never makes (N263).

## 3. Fidelity tiers (what "closed" means)

- **T1 routing.** Every allocation the reference sends to the caller allocator
  goes to it; nothing else does, and page memory stays on the page allocator.
  Frees go to the same allocator with the same length and alignment.
- **T2 failure semantics.** A NULL from the caller allocator takes the
  reference's error path (`OUT_OF_MEMORY`, log-and-drop, fixed fallback). It
  never aborts. T2 is **mandatory with T1**: routing without it turns every
  "limited arena" embedder into a process abort (a new, reachable crash that
  the reference does not have). The 26 reference `FailingAllocator` tests then
  port faithfully instead of as adaptations.
- **T3 call shape.** The same sequence of alloc/resize/remap/free calls, with
  the same sizes (growth factors, arena 1.5x, SegmentedList prealloc 2,
  MemoryPool arena chunks). Faithful where a zigstd equivalent exists (the
  pools and arenas already are). For `Vec`-shaped containers it is a
  documented residual: Rust `RawVec` growth is not Zig's `growCapacity`. The
  observable difference is call counts and sizes, not routing. Recorded, not
  silently accepted (see §7 task S7-9).

All options below are judged at T1+T2. T3 is identical across them.

## 4. Options

### (A) Generic `A: Allocator + Copy = Global` threading (the T-A scope)

Every struct that owns a caller-routed container gets a type parameter:
Terminal, ScreenSet, Screen, PageList, MemoryPool, Tabstops, ImageStorage and
its image types, the stream handler, osc::Parser/Command, the dcs/apc handlers,
Glossary, RenderState/Row/CellList, TerminalSearch/ScreenSearch/SlidingWindow/
ViewportSearch/ActiveSearch/PageListSearch. That is 27 top-level structs.

- **Fidelity.** Reaches T1+T2, but **vt-c still needs a runtime handle**. NULL
  vs non-NULL is a runtime choice per `ghostty_*_new` call, so vt-c instantiates
  `Terminal<RuntimeHandle>` where `RuntimeHandle` is exactly option B's type. A
  is therefore B plus a type parameter. It also turns something that is
  runtime in Zig into a compile-time choice, the reverse of the port's usual
  comptime-flattening hazard. The measured cost of the runtime handle in (B) is
  within noise (§5), so A buys nothing measurable.
- **Edits (measured by grep on `d33aef69`).** All of B's container work, plus
  the generic plumbing:
  - 27 struct definitions.
  - Up to 355 `impl` blocks in the family files (terminal 277, render 5,
    search 35, osc 38; a subset needs `<A>`).
  - Type-mention sites in rustty-vt: Terminal 136, Screen 175, PageList 57.
  - 109 mentions in vt-c.
  - Downstream, 29/19/12/7 files name Terminal/Screen/PageList/RenderState. The
    default parameter keeps them compiling but pins them to `Global`.
  - Consistent with T-A's 700–900 edits across 45–60 files.
- **Perf.** App instantiation: identical to (B)'s Global arm, plus zero bytes
  per handle. The prototype bounds that difference at ≤ noise (§5). The
  vt-c instantiation is the same as (B). **Code size**: any binary that
  instantiates both, such as vt-c's own tests over Global and C handles,
  duplicates the whole terminal core. The rustty-vt suite exercises only
  `Global`, so the C instantiation is tested only through vt-c.
- **Miri/SB.** No aliasing change. To cover both instantiations, Miri must
  run the rustty-vt suite twice.
- **C-ABI.** None. Headers, exports and the ABI gates are untouched.
- **Risk.** High. The largest diff, cross-crate generic churn in raw-pointer-heavy
  code (search holds `*mut Screen`, pins hold `*mut Node`), and the longest
  review. Its one benefit, a zero-sized app handle, is unmeasurable.

### (B) Runtime handle stored in each object: the faithful map of `std.mem.Allocator` (RECOMMENDED)

`VtAllocator`: `Copy`, 8 bytes, `Option<NonNull<RusttyAllocator>>`.
- `None` means the Rust global heap: the fast path, no vtable. This is what NULL
  and the app use.
- `Some` is the **caller's own C struct pointer**. That is exactly Zig's
  `std.mem.Allocator.ptr` for a C allocator. Zig's `.vtable` word is the
  constant `&zig_vtable`, so it is elided. This keeps the reference's
  lifetime contract with no holder box.
- It implements allocator_api2's `Allocator`: `grow`/`shrink` are `remap`
  then alloc+copy+free, as Zig's `realloc` is. It implements zigstd's
  `ResizeInPlace` via the vtable's `resize`, as Zig's arena asks its child.
- The inlined `allocate`/`deallocate` are a null test plus the `Global` call,
  and the C arms are out of line (the `CaptureAllocator` shape, T-A3 lesson).
- It is threaded as a concrete type. The port already passes an `Allocator`
  value through every reference signature, so the threading is an **import
  swap**. The module exports `Allocator` (and `Global`) in both the type and
  the value namespace, like a unit struct, so `Allocator` used as a value keeps
  compiling. `osc::CaptureAllocator` folds into it.

- **Fidelity.** T1+T2 by construction. It is also the closest structural
  mirror: a runtime `{ptr}` value stored where Zig stores `alloc` (`Screen.alloc`,
  `pool.alloc`, wrapper `.alloc`), and passed where Zig passes it.
- **Edits** (the prototype diff is measured; the rest is a grep census of
  non-test code on `d33aef69`, a lower bound because the regex misses
  multi-line types):
  - The handle plus import swap across 28 rustty-vt files. In the prototype
    this compiled the whole lib after 1-line swaps plus `Tabstops`/`dcs`
    re-pointing.
  - Containers: terminal family 57 fields / 239 growth-or-clone sites (44
    files); render ≈ 8 fields / 12 sites; search 11 / 59 (9 files); osc 17 / 48
    (21 files). About 90 fields and ≤ 360 growth sites. The T2 fallible
    conversion touches only the caller-routed subset.
  - vt-c: `new_`, `from_decoded`, render `update`/`begin_update`/`free`, search
    `init`, `rustty_osc_new`, selection-gesture. About 20 sites. The
    `alloc_holder` / `ZigAllocatorRef` aliasing for internals retires.
  - Downstream (termio, core, renderer, bench, inspector): about 10 import swaps
    to rustty-vt's alias, or 52 `From` conversions (the prototype measured 52
    across 10 files).
  - Mechanical: 157 fully qualified `allocator_api2::alloc::Global` in rustty-vt
    tests plus 11 in vt-c. Restoring `PageList::init(alloc, ...)` touches 336
    call sites (N264).
  - Total: roughly 350–500 semantic edits plus about 500 mechanical ones.
    That is about half of (A).
- **Perf.** Measured in §5. The Global arm matches the baseline on all hot paths,
  within the noise of a loaded machine. The one real hazard is **container
  choice**, not the handle: allocator_api2's stable `Vec` has no
  `extend_from_slice`→memcpy or `resize`→fill specialization. Swapping
  RenderState's `Vec`s naively cost **+81 %** on `screen-clone.render`. A 30-line
  `CopyVecExt` (`extend_copy`/`resize_copy`) removed all of it (N261). (A) has
  the same hazard, because it uses the same containers.
- **Miri/SB.** The handle is a raw `NonNull` to the caller's struct, copied by
  value. It is dereferenced only transiently (`as_ref()` inside one allocator
  call) and never through `&mut`, so it cannot conflict with the `&mut
  Terminal`/raw-alias patterns (memory: `rust-mut-args-strongly-protected`,
  `shaper-hook-sb-clean-interleave`). `unsafe impl Send + Sync` keeps
  `Terminal` `Send` for the app's thread hand-offs. The contract, that a C
  allocator must be callable from whatever thread drives the object, is the
  reference's implicit one. It is documented, not checked. `Default` =
  Global keeps the trait surface (memory: `intrusive-ptr-and-placeholder-fill`).
- **C-ABI.** None. No header, export, struct size or ABI gate changes; the
  headers already take `const RusttyAllocator*`. The behaviour change is the
  intended one: the caller's vtable is now called for internals.
- **Risk.** Moderate, and each part is gated:
  1. The container-specialization hazard: `CopyVecExt` plus a clippy
     `disallowed-methods` ban on allocator_api2 `Vec::{extend_from_slice,
     resize}` for the vt crates.
  2. The breadth of T2: an allocation-failure sweep gate.
  3. Managed versus unmanaged containers: Zig's `.empty` containers take the
     allocator per call, while allocator_api2 containers bind it at
     construction. RenderState binds it on first growth, as the prototype does,
     with a `debug_assert` that later calls pass the same handle. vt-c always
     passes `state.alloc`, so no API path can pass a different one.
  4. +8 bytes per container, which was not visible in any measurement.

### (C) Narrower options

- **C1: vt-c-only scoped `#[global_allocator]`.** vt-c installs a global
  allocator that dispatches to a thread-local "current C allocator" set on
  entry to each `rustty_*` call. REJECTED:
  - It needs a per-block header to route frees, so the caller's allocator sees
    lengths and alignments the reference never passes (T1 broken).
  - It over-routes: every Rust-internal allocation, such as log formatting and
    OSC payload copies, lands on the caller.
  - It frees across objects: a render update frees terminal memory through the
    state's allocator.
  - It conflicts with any Rust host that links the staticlib, and gives no T2.
- **C2: wrapper storage only.** This is already done (Tier 1) and does not close
  any of the five rows. Their content is the internals. REJECTED as incomplete.
- **C3: (B) restricted to the reference's routing set.** Not a separate
  mechanism; it is the scope (B) must have, and the two facts that make (B)
  tractable:
  1. **Page memory is out of scope and already faithful.** It is the bulk of
     the bytes and the hottest allocator.
  2. **The per-byte print path makes no caller allocation in the reference.**
     The prototype confirms this: the counting C allocator saw 6 allocations for
     the whole 64 MiB `terminal-stream` run, at the bench's 10 000-byte
     scrollback, where pages and nodes are recycled. A larger scrollback adds
     about one node allocation per page, not per byte.

  Within (B), the implementation should route exactly the §1.2 table: nothing
  more (no port-only copies on the caller, see N263) and nothing less.

## 5. Prototype and measurements

Prototype `tail-s7a-proto` @ `045dd4f0` (NOT FOR MERGE). It contains:
- `VtAllocator` (`crates/rustty-vt/src/vt_alloc.rs`).
- The import swap across rustty-vt.
- The PageList node/pin pools carrying the handle (`MemoryPool.alloc`, the
  reference's `pool.alloc`), plus `PageList::init_in`/`MemoryPool::init_in`.
- RenderState's `row_data`, `pending_styles`, `applied_styles` and `CellList`
  ×4 as `allocator_api2::vec::Vec<T, VtAllocator>`, bound to the `update`
  allocator.
- `CopyVecExt`.
- 52 downstream conversions.
- Bench seams: `RUSTTY_S7A_CALLOC` builds the terminal over a C-ABI allocator
  (`rustty_lib::allocator::c_allocator()`, whose vtable reaches the Rust heap
  through two indirect calls, as a C caller's would). `RUSTTY_S7A_COUNT` counts
  the vtable calls.
- The rustty-vt lib tests do not build on that branch: 4 kitty test files
  spell `allocator_api2::alloc::Global` in full.

**Routing check (counting C allocator):**

| config | caller-allocator calls |
|---|---|
| `screen-clone.clone` | 3006 allocs (3 per clone: node-pool and pin-pool arena chunks plus one), 1 resize probe (refused, as the global child does), 0 remaps |
| `screen-clone.render` | 328 allocs / 328 frees (live 0 at exit), 10 remaps |
| `terminal-stream/ascii` (64 MiB, 10 000-byte scrollback) | 6 allocs / 6 frees |

**Method.** `rustty-bench` built with the `release-fast` profile. Binaries:
- `base` = `d33aef69`.
- `B1` = naive container swap.
- `B` = with `CopyVecExt` (`B3` in the harness).
- `B+C` = B with the terminal on the C vtable arm.

Runs are paired, interleaved and order-rotated per round
(`.port/parity-closure/s7a-prototype/paired.py` on the proto branch). The
metric is **child CPU time** (user+sys via `getrusage`), because the host was
oversubscribed: **load average 25–55 on 14 cores** throughout, with other
agents building. The table reports min over N and the median of per-round
paired ratios. The render loop is also reported as a loops-sweep slope,
`(L5 − L1)/4/50 000` ns per update.

| config (N) | base min ms | B (Global arm) | B+C (C vtable arm) | B1 naive swap |
|---|---|---|---|---|
| terminal-stream/ascii (31) | 49.17 | 49.19, +0.0 % (paired −0.1 %) | 49.17, +0.0 % (paired +0.2 %) | — |
| terminal-parser/utf8 (15) | 168.61 | 166.33, −1.4 % (paired −0.3 %) | 171.11, +1.5 % (paired +1.6 %) | — |
| screen-clone.clone/small (15) | 21.30 | 21.95, +3.1 % (paired 0.0 %) | 19.33, −9.2 % (paired −0.6 %) | — |
| screen-clone.clone/small, wall (41) | 25.32 | +3.0 % (paired −0.9 %) | +2.1 % (paired −0.6 %) | — |
| screen-clone.render L1 (21) | 137.92 | +8.0 % (paired +1.8 %) | +2.4 % (paired −4.6 %) | +80.5 % (N=9) |
| screen-clone.render L5 (21) | 692.37 | +1.3 % (paired +2.4 %) | −0.4 % (paired +0.7 %) | +81.0 % (N=9) |
| render slope, ns/update (N=9 run) | 2592 | 2602 (+0.4 %) | — | 4695 (+81 %) |
| render slope, ns/update (N=15 run) | 2766 | 2822 (+2.0 %) | 2879 (+4.1 %) | — |

**Reading.**
- The handle and the Global arm show no signal on terminal-stream,
  terminal-parser or screen-clone. On render the spread across runs is −2 % to
  +2.4 % paired, and the min-statistic noise band at this load is ±3–8 %.
- The C vtable arm, which only vt-c users pay and which the reference pays
  too (its allocator is always vtable-dispatched), is also within noise.
- The naive allocator_api2 `Vec` swap is the only real regression (+81 %), and
  `CopyVecExt` removes it.
- **UNVERIFIED on an idle machine.** The G8b fence (`check_perf_fence.sh`,
  both fences) must be rerun at load ≤ its idle threshold before the S-7
  implementation merges.

## 6. Options table

| | fidelity (T1+T2) | edits | measured perf | Miri/SB | C-ABI | risk |
|---|---|---|---|---|---|---|
| (A) generic `A` | yes, but needs (B)'s handle inside vt-c anyway | ≈ 700–900 semantic + mechanical; 27 structs, ≤ 355 impls, ~480 type mentions | app = B's Global arm minus 8 B/handle, unmeasurably different; same container hazard | unchanged aliasing; suite ×2 to cover both instantiations | none | high |
| **(B) runtime handle** | **yes, the structural mirror of `std.mem.Allocator`** | **≈ 350–500 semantic + ≈ 500 mechanical** | **Global arm and C arm within noise on all hot paths (load 25–55); naive container swap +81 %, fixed by `CopyVecExt`** | **raw `NonNull` by value, no `&mut`; Send/Sync contract documented** | **none** | **moderate, gated** |
| (C1) scoped global allocator | no (headers change sizes, over-routes, cross-object frees, no T2) | small | not measured (rejected on fidelity) | TLS plus global allocator; unsound cross-object frees | none, but conflicts with Rust hosts | high |
| (C2) wrapper only | no (that is today) | 0 | — | — | none | — |
| (C3) scope facts | = (B)'s scope | — | the per-byte path makes no caller allocation (6 calls per 64 MiB) | — | — | — |

## 7. Recommendation

**(B)**, scoped by (C3).

- It is the faithful map: Zig's allocator is a runtime value, and (B) keeps it
  one. The reference's lifetime and NULL semantics carry over unchanged, and the
  handle is stored and passed exactly where Zig stores and passes `alloc`.
- It is about half of (A)'s edits, because the port already threads an
  `Allocator` value through every reference signature. The prototype showed that
  a one-line import swap per file compiles the whole library.
- (A)'s only advantage, a zero-sized handle for the app, has no measurable
  value: the Global arm is within noise on every hot path. (A) cannot avoid the
  runtime handle anyway, because vt-c needs it.
- The real perf hazard, container specialization, is independent of A versus
  B. It is solved and gated.
- T2 comes with it. The 26 reference `FailingAllocator` tests become
  portable as written, which retires about 66 adaptation notes.

### Implementation plan (S-7b onward): closes all 5 rows plus deferral item 3

Order matters. Each task is one commit group with its own gates, and each
leaves `main` green.

1. **S7-1 foundation**
   - Add `rustty-vt/src/alloc.rs`: `VtAllocator` (the prototype's, plus
     `debug_assert` equality helpers), the type+value `Allocator`/`Global`
     exports, `CopyVecExt`, and fallible helpers (`try_push`, `try_extend_copy`,
     `try_resize_copy` returning `OutOfMemory`).
   - Add `impl ResizeInPlace`.
   - Add test allocators: counting, failing-after-n and rejecting, wrapping
     `RusttyAllocator`.
   - Fold `osc::CaptureAllocator` into it.
   - Gates: unit tests of every arm (zero-size, grow via remap hit/miss,
     shrink, resize-in-place, free length/alignment) with negative controls;
     clippy `disallowed-methods` for allocator_api2 `Vec::{extend_from_slice,
     resize}` in rustty-vt, with a self-test that proves the ban fires.
   - Miri: the `alloc.rs` tests.
2. **S7-2 import swap (mechanical)**
   - Rewrite the 28 files' imports and the 157 + 11 fully qualified test
     spellings.
   - Point downstream crates at the rustty-vt alias.
   - Restore `PageList::init(alloc, opts)` and `MemoryPool.init(gen_alloc, ...)`
     (336 sites; N264).
   - No behaviour change: everything is still `Global`.
   - Gates: build, `cargo test -p rustty-vt/-vt-c` ×3, **perf fence G8b plus
     the paired loops-sweep on terminal-stream / terminal-parser /
     screen-clone.{clone,render}** against the pre-S7 binary. This isolates
     the handle's cost.
3. **S7-3 PageList + Screen + Terminal (S179-F2 part 1)**
   - Node/pin pools and `tracked_pins` (`AutoArrayHashMapUnmanaged<K, V, A>`)
     over the handle.
   - Compressed-page bytes and `pagePreservingState(alloc)` buffers.
   - `Tabstops<VtAllocator>`, pwd/title, `cursor.hyperlink`, the
     glossary, `kitty_dnd`.
   - T2 at each reference `try` site.
   - Miri: pagelist pool, compress/restore and `pagePreservingState` tests,
     `track_pin`/untrack. Pagelist/page changes need the controller's Miri
     lane.
4. **S7-4 kitty graphics, APC/DCS, stream, clipboard, wrapper maps
   (S179-F2 part 2)**
   - `hashbrown` with `allocator-api2` for the image and placement maps.
   - The PNG `LimitedAllocator` over the handle.
   - stackFallback overflow onto `gpa()`.
   - vt-c `new_`/`from_decoded` pass `VtAllocator::from_c(alloc_)` into
     `Terminal::init`; the `searches`/`tracked_grid_refs` maps use it too.
   - Close **S179-F2**.
5. **S7-5 RenderState (S175-F1, S4-F2 render half)**
   - Containers as in the prototype, with `CopyVecExt` on the memcpy/fill
     paths, and a per-row arena for grapheme dupes (zigstd `ArenaAllocator`
     over the handle).
   - vt-c `update`/`begin_update`/`free` pass `state.alloc`.
   - Close **S175-F1**.
6. **S7-6 search (S176-F1, S4-F2 search half)**
   - `TerminalSearch::init(alloc, needle)` and its screen, viewport and
     sliding-window containers (CircBuf, chunk buffer, `cell_map`s, decode
     buffer) over the search's handle.
   - The 4 terminal-charged allocations stay on the terminal (N262): the
     `searches` entry and the `trackPin`s.
   - Close **S176-F1** and **S4-F2**.
7. **S7-7 OSC commands (S174-F3 remainder)**
   - `color_operation.requests` as a zigstd `SegmentedList<Request, 2, A>`
     (inline prealloc of 2, so a C allocator sees no call for ≤ 2 requests, as
     in the reference). `kitty_color_protocol.list` as a vector on the parser's
     handle.
   - Text payloads: borrow from the parser buffer or capture, as the reference
     does (N263): `Command<'p>`, consumed before the next `next`/`end`. The
     inspector keeps an owned copy at its own boundary (it is outside the
     C API).
   - Close **S174-F3**.
8. **S7-8 selection-gesture codepoints (deferral item 3)**: `event.alloc`.
   Update `DEFERRALS.md` F-008 Tier 2 to CLOSED.
9. **S7-9 records**
   - T3 residual (N261's sibling): Vec growth factors versus Zig
     `growCapacity`, as a documented deviation with a counting-allocator test
     that pins what the caller sees.
   - N260 (header silent on allocator lifetime; port matches the reference).
   - Ledger rows closed with test and negative-control evidence.

**Gates for every task:**
- `cargo build --workspace`; `cargo test` ×3 for touched crates; clippy
  `-D warnings`; `cargo fmt --check`.
- G-PARITY count drop equals the rows closed.
- ABI gates and `check_rustty_vt_headers.sh`, run although no header changes, to
  prove it.

**Row-closing evidence:**
- **Routing:** per family, a counting C allocator test showing internals hit
  the vtable and live bytes are 0 after free, with a negative control that
  reverts the handle to Global.
- **T2 (vt-c): allocation-failure sweep.** For n in 0..N, a failing-after-n
  C allocator drives new, write (a corpus with OSC/APC/kitty graphics/
  resize/scroll), render update, search and free. The process never aborts;
  every result is `SUCCESS` or `OUT_OF_MEMORY`, or the stream's log-and-drop;
  and live bytes are 0 after free. This is the Zig
  `checkAllAllocationFailures` idea, which the reference lacks for the C API.
- The ported reference `FailingAllocator` tests, assertion for assertion.

**Perf fence:**
- G8b (both fences) at idle.
- Plus the paired interleaved loops-sweep (`paired.py`, CPU time) on
  terminal-stream, terminal-parser, screen-clone.{noop,clone,render L1/L5},
  osc-parser and terminal-snapshot/formatter (the Vec-heavy paths).
- Band: +2 % median paired on each config, with G8b's +5 % as the hard stop.
- Any new allocator_api2 container on a hot path must be checked with `nm` for
  an out-of-line `extend`/`grow` (memory: comptime-flattening hazard). No LTO.

**Miri targets (controller lane):**
- `alloc.rs` arms.
- Pagelist pool, compression and pin tests.
- `render.rs` update tests over a counting C allocator.
- `search/*` feed and select tests.
- The OSC `Command<'p>` borrow tests, which are the SB-sensitive part: a
  command that borrows the parser buffer while `next` takes `&mut` is exactly
  the aliasing pattern to prove.
- vt-c lifecycle tests (terminal, render, search, osc) with the counting
  allocator.
- A small-corpus instance of the failure sweep.

**Estimated size:** 8 implementation tasks. S7-2 and S7-3 are serial
(foundation, then terminal). S7-5, S7-6 and S7-7 can run in parallel after
S7-4, within the ≤ 3 building worktrees disk budget.

## 8. Findings recorded by this study (ids N260–N264; to be filed in NEW-FINDINGS.md by S7-9)

- **N260.** The caller's `GhosttyAllocator` must outlive every object built
  from it (`lib/allocator.zig:78-83` stores the caller's pointer). The header
  does not say so. The port matches (`CAllocatorAsZig` / the prototype
  handle). Informational: no port change, and header text stays verbatim (R1).
- **N261.** allocator_api2 0.2.21's stable `Vec::extend_from_slice` is
  `extend(iter.cloned())` and `resize` is a per-element `ptr::write` loop.
  There is no memcpy or fill specialization. Swapping RenderState's std `Vec`s
  for it cost +81 % on screen-clone.render; `CopyVecExt` restores parity. This
  is the same class as T-A2's OSC capture finding, and is gated by
  clippy `disallowed-methods`.
- **N262.** A search's allocations are split: its own state goes on the search
  allocator, but the `searches` map entry and three `trackPin`s go on the
  **terminal's** allocator (`c/search.zig:143-146`, `search/pagelist.zig:58`,
  `search/screen.zig:557`, `highlight.zig:71-73`). S7-6 must reproduce this
  split.
- **N263.** OSC `Command` text payloads are owned `Vec<u8>` copies in the port
  (about 15 variants) but borrowed slices in the reference. Routing the copies
  to the caller allocator would over-report calls the reference never makes,
  and keeping them on the Rust heap leaves memory unaccounted. The faithful fix
  is to borrow (S7-7).
- **N264.** `PageList::init` lost the reference's `alloc` parameter (336 call
  sites). S7-2 restores it together with `MemoryPool.init(gen_alloc, page_alloc,
  preheat)`.

## 9. Outcome (task S7-9, 2026-09-27)

Option (B) shipped as recommended, in the planned order (S7-1 foundation, S7-2
import swap, S7-3 PageList/Screen/Terminal, S7-4 kitty/APC/stream/clipboard/vt-c
wrapper, S7-5 RenderState, S7-6 search, S7-7 OSC commands, S7-8 selection-gesture
codepoints and the C API's allocating writers, S7-9 records). The runtime handle is
`rustty_vt::alloc::VtAllocator`.

- **Rows**: S174-F3, S175-F1, S176-F1, S179-F2 and S4-F2 are CLOSED in `LEDGER.tsv`,
  each with routing tests over counting allocators and negative controls (N272,
  N274, N315, N320, N325). `DEFERRALS.md` F-008/W12-01: Tier 1 FIXED, Tier 2 CLOSED
  (N334), Tier 3 RECORDED (N360).
- **T1/T2**: every allocation the reference sends to the caller's allocator reaches
  it, page memory stays on the page allocator, and every reference `try` is
  reachable as `OUT_OF_MEMORY`, log-and-drop or the fixed fallback. The C-API
  failure sweeps (terminal, render, search, OSC, selection gesture, allocating
  writers, snapshot decode) never abort and return every byte.
- **T3**: the call-shape differences that remain are consolidated in N360, with
  file:line and reason for each. One of them was taken for performance and accepted
  by the user (2026-09-26): the render state's per-column cell storage (N315). The
  allocator_api2 growth residual is pinned by
  `zig_list::tests::rawvec_growth_is_the_recorded_t3_residual`.
- **Findings of this study**: N260-N264 are filed in `NEW-FINDINGS.md` (all CLOSED or
  informational). The series found and fixed the reference defects UD305, UD310-UD315
  (R13), and the port soundness defects N270 and N273.
- **Where the implementation departed from §7**: the per-row arena became retained
  lists, and the cells are per-column allocations (N315). The tracker, pin map and
  writers use a port of Zig's `ArrayList` growth (`zig_list.rs`), not allocator_api2
  growth. `Stream::init_in` was retired again by S7-7 (`Options.allocator`).
- **Miri**: the controller's lane runs the S7 targets
  (`.superpowers/.../miri/run.sh`). S7-9 made them tractable and removed the
  integer-to-pointer casts on the page-offset path (N361, N362). The results are the
  lane's to record.
- **Perf**: the tasks' reports carry their paired-fence numbers (not re-measured
  here). The idle-machine G8b rerun that §5 asks for is not part of
  this record (UNVERIFIED here).
