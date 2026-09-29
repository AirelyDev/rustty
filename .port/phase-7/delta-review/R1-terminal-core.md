# Phase 7 delta review R1: terminal core (`src/terminal/**`, `src/datastruct/**`, `src/lib/parse_int.zig`)

Range `20abdb50a..12752b2ac` (`origin/main` in `../ghostty`, read with `git show` / `git diff`; the
oracle working tree stays at the pin). Rustty at `079cc21e`. Read-only review: no Rustty code was
changed and nothing was built. Upstream line numbers are at `origin/main`; Rustty line numbers are at
`079cc21e`.

**Scope.** 37 non-merge upstream commits touch this area: 45 files, +5,129 / −779. They add 87
`test "…"` blocks and remove 2: `c/terminal.zig` renames `set write_pty callback`, and
`tmux/output.zig` deletes `parse mouse_any_flag`.

| file | +tests | file | +tests |
|---|---|---|---|
| `c/render.zig` | 10 | `kitty/graphics_image.zig` | 8 |
| `Screen.zig` | 8 | `render.zig` | 8 |
| `Terminal.zig` | 5 | `c/terminal.zig` | 5 (−1 renamed) |
| `datastruct/comptime_int_set.zig` (NEW) | 5 | `datastruct/untouched_pool.zig` (NEW) | 5 |
| `kitty/windows.zig` (NEW) | 5 | `PageList.zig` | 3 |
| `modes.zig` | 3 | `stream_terminal.zig` | 3 |
| `color.zig`, `stream.zig` | 2 each | `lib/parse_int.zig` (NEW) | 1 |
| `c/{formatter,search,selection,types}.zig`, `kitty/color.zig`, 6 × `osc/parsers/*.zig`, `search/{pagelist,screen,terminal}.zig` | 1 each | `tmux/output.zig` | −1 |

## 1. Summary table

Status is Rustty's state today. Size: S under 2 h, M under 1 day, L more than 1 day. "Obs." says whether the change is observable behaviour (Y) or internal/perf (N).

| # | commit | theme | obs.? | Rustty status | size | top risk |
|---|---|---|---|---|---|---|
| 1 | `06178eeaa` | search resumes a complete search when history is prepended | Y (crash fix) | ABSENT; Rustty carries a reachable `unreachable!()` with no record | S | search Miri lane; do after the node-pool change (#7) |
| 2 | `e01e75bbb` | `UntouchedPool`: the page pool free list no longer touches items; `releasePages` | N (RSS) | ABSENT | L | pool rewrite; alloc and bench baselines; G6/G6b |
| 3 | `ffe015ee5` | bitmap allocator: 0 bits mark free chunks; `initAssumeZeroed` | N | ABSENT | S | 15 inverted test literals |
| 4 | `c0a4f80d8` | hash map and `RefCountedSet` initialise from zeroed memory | N | ABSENT | S | the lz4 uninit-read PORT NOTE must stay valid |
| 5 | `6112935a2` | hash map keeps capacity and key/value pointers in the struct | N (page layout shrinks) | ABSENT | M | SB: three cached raw pointers; page geometry |
| 6 | `d2ff6d77a` | pages initialise from zeroed memory; cells cache-line aligned; `MetaLayout` | **Y** (rows per page, `maxCols`, page sizes) | ABSENT | M | N88 geometry golden RED; needs a re-probe |
| 7 | `efa3c66ed` | PageList nodes pooled one by one from the gpa (`UntouchedPool`), not an arena | N (RSS) | ABSENT | M | every teardown must return its nodes; SB; `sc.clone` perf |
| 8 | `35a6fb747` | PageList preheats 1 node and 2 pins; `initTrackedPins` capacity 2 | N | ABSENT | S | S7 counting tests re-derive |
| 9 | `105d0a545` | C wrapper allocates the kitty temp-dir path only when it is set | N | PARTIAL (per-screen global-heap `Vec`) | S | S7 T1 routing decision |
| 10 | `d1cd56a56` | dynamic palette shares the built-in default | N (+ a C OOM path) | ABSENT | M | `DynamicPalette` loses `Copy`; termio half |
| 11 | `dd2edd760` | libvt returns NULL or real storage for empty output, never a sentinel | **Y** (ABI contract) | ABSENT; contrary by design (N331/N361) | S–M | 4 headers; tests flip; vt-c Miri |
| 12 | `0b5446364` | kitty rejects unsafe Windows paths; new `kitty/windows.zig` | Y (Windows) | ABSENT | M | rwin lane mandatory; R14 cfg forms |
| 13 | `60b43068c` | Windows `DiscardVirtualMemory` reclaim; compression scratch clears only its dirty prefix | Y (Windows RSS) | ABSENT | S | rwin lane; G6b |
| 14 | `f6cb8312b` | TinyIo on Windows; the C `Io` becomes stateless | N | N/A (NON-PORT); quoted comments stale | S | G3 comment text only |
| 15 | `9dc0d974e` | answer ANSI DECRQM queries | Y | ABSENT | S | 1 test renamed in vt-c |
| 16 | `3beb6d717` | DECRQM keeps 16-bit mode numbers | Y | ABSENT (`& 0x7FFF`) | S | termio call site too |
| 17 | `a41248013` | tmux list-windows use-after-free | Y (format, errdefer) | PARTIAL (the UAF cannot happen; order and format differ) | S | retires LEDGER S209-F3 |
| 18 | `c55f213aa` | option to disable scrollback pull on resize (C option 40) | **Y** | ABSENT | M | C enum; tracked-pin SB in `resize_cols`; 8 tests |
| 19 | `079502e23` | modes looked up by a comptime sorted set; new `ComptimeIntSet` | N (perf) + a >u15 guard | ABSENT | S–M | must be a `const` table; the runtime `disabled_by_artifact` arm has to survive |
| 20 | `e50779498` | render-hold effect for mode 2026 (Zig + C option 41) | **Y** | ABSENT | S–M | callback re-enters the terminal (SB); option id needs #18 first |
| 21 | `9a4ba7d54` | tmux list-windows lifetime test | test | ABSENT | (with #17) | the ptr-identity assert has no equivalent for an owned `Vec` |
| 22 | `c4f15c884` | word selection stops at hard line breaks | Y | ABSENT | S | none |
| 23 | `a4f0d9f4a` | batched special-graphics character writes | N (perf) | ABSENT | S–M | hot print path; perf fence |
| 24 | `78494bad8` | C API exposes the application-requested mouse shape | Y (new C API) | ABSENT (the core field exists) | S | new header and enum |
| 25 | `714fe9b90` | CSI 8 t window resize (terminal + termio + core + apprt + macOS) | Y (opt-in) | ABSENT (all tiers) | M | embed ABI; config; cross-tier |
| 26 | `d4f45bee3` | reverse wrap cursor jump fix | Y | ABSENT | S | none |
| 27 | `a3e80a685` | word selection across wide characters | Y | ABSENT | S | none |
| 28 | `ca4f719f7` | RenderState overscan; row ids; 6 ScreenClone bench modes | Y (Zig API) | ABSENT | M | hot `begin_update`; N315 counts; G-BENCH |
| 29 | `2ea1eeae2` | RenderState overscan in the C API | Y (C ABI) | ABSENT | M | iterator sub-slice provenance (SB); manifests |
| 30 | `73768913b` | OSC integers reject digit separators | Y | PARTIAL: UD141 fixed first, different shape | M (#30–34 together) | sign semantics diverge; config RGB reach |
| 31 | `4b86e3594` | kitty zlib via Wuffs | Y (OOM error kind) | ABSENT; closes the zlib half of UD140 | M | build graph: wuffs becomes mandatory for vt |
| 32 | `1ca2858d0` | tmux maps pane mouse flags to the selectors tmux reports | Y (modes + wire format) | ABSENT | S | hand-indexed `v[N]` shift; −1 test |
| 33 | `d92e3400e` | OSC integers parsed in a single pass | N | ABSENT | (#30) | accumulator width semantics |
| 34 | `500040d3b` | `parseInt` moves to `src/lib` | N | ABSENT | (#30) | new file invisible to G3b |
| 35 | `31bbb1273` | doc: why `parseInt` avoids `std.fmt.parseInt` | N | ABSENT | (#30) | G3b |
| 36 | `3822aa01d` | test: OSC 3008 numeric fields reject non-digits | test | behaviour present; test absent | S | none |
| 37 | `b1c264163` | vt rejects search ticks after the terminal is freed | Y (UAF fix) | ABSENT; Rustty carries the UAF, unrecorded | S | only Miri or ASan tell pass from fail |

Totals: 37 commits.
- **Status:** 30 absent; 4 partial or re-alignment (#9, #17, #30, #11 as a design reversal); 1 N/A (#14); the remainder are tests or docs folded into a parent.
- **Size:** 1 L, 12 M or S–M, the rest S.

## 2. Per-theme sections

### 2.1 Page memory model (#2–#8, #13): the largest and riskiest block

**Upstream end state.**
- **Page pool.** `PagePool = UntouchedPool([std_size]u8, page_size_min)` (`PageList.zig:338`). The free list lives in a side `ArrayList(ItemPtr)` allocated from `gen_alloc`, and every item is its own `alignedAlloc`.
  - `create()` reserves the free-list slot first, so `destroy()` cannot fail. `release()` frees an item immediately.
  - `reset(limit)` frees items above the limit. `deinit` and `reset` assert `live == free.len` (`untouched_pool.zig`).
- **Zeroing contract.** Every path that returns a page to the pool must zero it (`PageList.zig:338` comment).
  - `releasePoolPage` (`:948`) calls `release` natively and decommit plus `destroy` on wasm. `releasePages` (`:929`) replaces three teardown loops.
  - The arena scrub in `reset` is gone (`:1019`).
  - `clone` now appends the node before `cloneFrom` (`:1139`), which closes a latent page leak on `cloneFrom` failure.
- **Node and pin pools.**
  - `NodePool = UntouchedPool(List.Node, .of(List.Node))` (`:317`, with `node_preheat = 1` at `:44`).
  - `PinPool` stays an arena `MemoryPool`, now preheated with `pin_preheat = 2` (`:49`).
  - `initTrackedPins` (`:700-707`) sizes the pin set to exactly 2.
- **Zeroed-memory initialisation.**
  - The bitmap allocator flips to "0 = free", so zeroed memory is already valid (`bitmap_allocator.zig:71` `initAssumeZeroed`; `findFreeChunks` works on `~bitmap`, `:353`).
  - `RefCountedSet.initAssumeZeroed` (`ref_counted_set.zig:224`) writes nothing, and `assertIntegrity` skips item 0 and bucket id 0 (`:755`, `:769`).
  - The hash map moves `keys`, `values` and `capacity` into the struct (`hash_map.zig:146-152`); the in-buffer `Header` shrinks to `{size: u32}` (`:261`).
- **Page layout.**
  - `Page.initBuf` requires zeroed memory (`page.zig:263`).
  - Cells align to `max(@alignOf(Cell), min(cache_line, page_size_min))` (`:133`).
  - The meta block is a separate `MetaLayout` (`:1802`); `maxCols` and `adjust` are rewritten in bytes (`:1949`, `:1970`, `availableBytesForGrid` at `:2003`).
- **Windows.** `mem.zig:67-72` reclaims with `DiscardVirtualMemory` (`:160-182`). `CompressionScratch.deinit(pool, dirty_len)` clears only the dirty prefix on every OS (`PageList.zig:4635-4680`, `:4987-5013`).

**Rustty today.**
- **Pools.**
  - `PagePool = DynamicMemoryPoolAligned<PageBackingAlloc>` (`crates/rustty-vt/src/pagelist/mod.rs:848`). This is the arena plus intrusive-free-list design; `destroy` writes a `Node` into the page at `crates/rustty-zigstd/src/memory_pool.rs:367`, which is exactly what upstream removed.
  - `NodePool = ZigMemoryPool<Node, Allocator>` over `gen_alloc` (`mod.rs:722-725`, `:1085`). `PinPool` is preheated with 8 (`mod.rs:1101`).
- **Teardown.** `deinit` does not destroy nodes because "they all reside in the pool" (`mod.rs:2122-2160`). `zero_retained_page_arena` (`mod.rs:1164`, `:2262`) reproduces the removed arena scrub.
- **Clone ordering.** `clone` appends after `clone_from` (`pagelist/access.rs:888-941`), the pre-fix order.
- **Zeroed-memory init and layout.**
  - Bitmap: `init` fills `u64::MAX` (`bitmap_allocator.rs:112-123`).
  - Hash map: header `{values, keys, capacity}` (`hash_map.rs:454-458`); `metadata: Option<*mut Metadata>` (`:483-499`).
  - Page: `init_buf` calls `::init`, not `init_assume_zeroed` (`page_impl.rs:480-560`); `available_bits_for_grid` is at `page.rs:1377-1415`.
- **Windows and compression.** `can_reclaim` has no Windows arm (`mem.rs:206-246`). The compress scratch clears `std_size()` (`pagelist/compress.rs:76-99`, call site `:399-414`).
- No UD or N id matches any of these commits. N89 (node 480 B vs 472 B, `NEW-FINDINGS.md:2885-2897`) and N88 (page geometry golden, `NEW-FINDINGS.md:2839`) are affected.

**How to port faithfully, and where it conflicts with Rustty's design.**
1. **`UntouchedPool` goes into `rustty-datastruct`**, the upstream home. It needs two shapes:
   - `UntouchedPool<T, A>` with a const item size, for nodes.
   - `DynamicUntouchedPool<A>` with a runtime item size. `std_size()` is runtime in Rustty, a deviation `DynamicMemoryPoolAligned` already records.

   Both allocators are S7 handles. The gpa is `VtAllocator` (T1/T2 failure semantics: free-list growth is a new `OUT_OF_MEMORY` route into `createPageExt`). The item allocator is `PageBackingAlloc` for pages and `gen_alloc` for nodes. Grow the free list with the `zig_list.rs` `ArrayList` port so the call shape (T3) matches `ensureTotalCapacity`.

   S7 does not conflict in principle: `MemoryPool.alloc` already carries `gen_alloc` (S7-3). The conflict is with the pool *implementation*: `DynamicMemoryPoolAligned`/`ZigMemoryPool` for pages and nodes are retired, while `PinPool` keeps `ZigMemoryPool`.
2. **The `live == free.len` assert is an R7 invariant**, so it ports as `assert!`. Rustty's `PageList` has no `Drop`, so every teardown path must now return its nodes:
   - `deinit`
   - `release_error_pages` (`mod.rs:2638`)
   - `builder.rs:100`, `:254`
   - the clone error paths (`access.rs:824+`)
   - `mod.rs:2563`

   Expect port-only tests that leaked quietly to fail loudly. That is intended.
3. **Stacked Borrows.**
   - The free list holds `NonNull<Node>`; never form `&mut Node`.
   - Read `(*node).next` before `destroy` (upstream reordered this; the Rust loop at `mod.rs:2139-2152` reads `next` last and must change).
   - Hash-map pointers (#5) must all derive from the same raw `base` (`Offset::ptr`), never from a `&mut` projection, and no `&mut Page` may be held across them (memory `rust-mut-args-strongly-protected`).
   - Recycled node addresses now come from the global allocator, which makes the `page_serial_epoch` guard (`mod.rs:2182`) and fresh serial assignment in `createPage` load-bearing. Row ids (#28) depend on serials never being recycled.
4. **Zeroed-memory contract.** `init_assume_zeroed` relies on mmap or `allocate_zeroed` bytes (`rustty-os/src/page_allocator.rs:182-193` under Miri). Keep the `ref_counted_set.rs` byte-zero PORT NOTE (the lz4 uninit read, `MIRI_EXCEPTIONS.md`) on `init` only. Verify that every Rustty caller of `init_buf` hands it zeroed memory: `init_pages`, `create_page_ext`, the grow-prune reuse, and `Page::reinit` (`page_impl.rs:579-590`). This must hold under the new "release without zeroing" rule, because `release` frees rather than recycles.
5. **Test injection.**
   - Upstream's "memory pool never touches idle page memory" (`PageList.zig:20472`) and "fast path does not allocate" (`:20510`) inject a `FixedBufferAllocator`/`FailingAllocator` as the page allocator. `MemoryPool::init`'s page allocator is the concrete `PageBackingAlloc`, so it must become a generic parameter defaulting to it, or a test-only enum (Q8).
   - The existing S7 count assertions (`pagelist/tests.rs:19121+`, "4 allocator calls for node + pin pool arenas") are re-derived. So are the vt-c allocation-failure sweeps.
6. **Page geometry (#6).**
   - `page_tests.rs:609` `page_geometry_matches_the_reference` hardcodes Zig-measured `total_size`, `available_bits`, `maxCols`, 13 `adjust` row counts and 4 non-standard sizes at `20abdb50a`, for macOS and Linux aarch64. It goes RED and must be re-probed with the Zig probe at `12752b2ac` on both targets, at 512 and 8,192 grapheme bytes. Rename the field to `available_bytes`.
   - **G-REFGEOM will stay GREEN and cannot catch this.** It measures only set-item geometry.
   - `cells_align` needs a per-target cache-line constant matching Zig's `std.atomic.cache_line` (128 on aarch64/x86_64), not a crate's guess, or the wasm and Windows geometry drifts.
   - `MetaLayout` must stay `#[inline(always)]`-foldable like `Page::layout` (memory `comptime-flattening-perf-hazard`).

**Tests owed:** 5 in `untouched_pool.zig`, 2 in `PageList.zig` (#2), 15 bitmap literals inverted (#3). #4, #5, #7, #8 and #13 add none.

**Verification:**
- G6 and G6c vt lanes filtered to `hash_map|ref_counted_set|bitmap_allocator|page|pagelist|snapshot`.
- G6b (`check_pagelist_compress_leaks.sh` + `.port/phase-2/mirihar-pagelist-compress`, whose counting global allocator sees the new per-node allocations).
- The S7 controller Miri lane groups `vt-pagelist` and `vtc` (`.superpowers/sdd/2026-09-08-parity-defect-closure/miri/run.sh`).
- Miri on the new rustty-datastruct module.
- `sanitizers.sh` and `check_leaks.sh`.
- Re-baseline the alloc bench and `sc.clone` on both sides (`.port/phase-1/bench-zig-pagelist` changes too; build zigbench at `origin/main` with `-Doptimize=ReleaseFast`).
- Windows `DiscardVirtualMemory`: rwin only. `can_reclaim` is short-circuited under `cfg(test)`/Miri, so it needs a release smoke run.

### 2.2 Resize and palette behaviour (#18, #10)

**#18 `c55f213aa` — option to disable scrollback pull on resize.**
- **Upstream.**
  - PageList, Screen and Terminal gain `pull_scrollback`/`resize_pull_scrollback`: `PageList.zig:1247`, `Screen.zig:2020`/`:2117`, `Terminal.zig:135`/`:4111`/`:4125`. The flag survives `fullReset` (`:4944-4953`).
  - `resizeCols` pins the top of the active area with a temporary tracked pin and pads blank rows after reflow.
  - `resizeWithoutReflow` uses a `pull` predicate (`:2897`).
  - C API: `GHOSTTY_TERMINAL_OPT_RESIZE_PULL_SCROLLBACK = 40` (`terminal.h:1693`; `c/terminal.zig:1189`, `:1219`, `:1442`).
  - No config, termio or apprt consumer at `origin/main`; it is libvt-only.
- **Rustty homes.** `pagelist/resize.rs:82-100` (`Resize`), `:265`/`:362`/`:596`/`:736-760`; `screen.rs:541-557`; `terminal.rs:330-395`, `:6199`, `:6907`; `rustty-vt-c/src/terminal.rs:~2671`, `:3408-3420`; `include/rustty/vt/terminal.h:1547`.
- **Risk.** The temporary tracked pin inside `resize_cols` is the same raw-pin class as search and selection. The `defer untrackPin` must run on every error path of `resize_cols_inner`. Re-run G6 on `pagelist::resize*|screen::resize*|terminal::resize*`.
- **Tests (8):** 1 PageList (`:18151`), 5 Screen (`:7187-7297`), 1 Terminal (`:15942`), 1 vt-c (`c/terminal.zig:3397`).

**#10 `d1cd56a56` — dynamic palette shares the built-in default.**
- **Upstream.** `DynamicPalette.original` becomes `*const Palette`. `init`/`changeDefault` can fail (`color.zig:323+`, `:385`, `:402`, `:427`). `Terminal.deinit` frees the palette (`Terminal.zig:366`). The C `color_palette` set gains `OUT_OF_MEMORY`. Snapshot decode owns and frees it (`snapshot/terminal.zig:775`, `:879`, `:999`). Termio applies the palette after init (`Termio.zig:283`, `:473-478`, outside this area).
- **Rustty.** `color.rs:419-470` is `#[derive(Clone, Copy, …)]` with `original: Palette`.
- **Port.** An owner enum, `Builtin(&'static Palette)` or an owned copy through the terminal's `VtAllocator` (T1/T2). This loses `Copy`, and `Colors: Clone` (`terminal.rs:409`) needs care. `PartialEq` compares values. The termio half (`rustty-termio/src/termio_impl.rs:607`, `:1157`; `stream_handler.rs:3956`) must land in the same change.
- **Test:** 1 new, "DynamicPalette: changeDefault shares the built-in default" (`color.zig:1125`), which uses a `FailingAllocator`. The S7 failing `VtAllocator` makes it portable as written.

### 2.3 C API (#9, #11, #14, #24, #37; the C halves of #18, #20, #29)

- **#11 `dd2edd760`: a reversal of a recorded Rustty design choice.**
  - Upstream now returns NULL from `ghostty_alloc(0)` (`c/allocator.zig:16`) and from the empty `format_alloc`/`selection format_alloc`/`continuation_alloc` (`c/formatter.zig:239`, `c/selection.zig:250`, `c/terminal.zig:1076`). `lib.String.init` maps `len == 0` to the static `""` (`src/lib/types.zig:11`).
  - The docs change in `allocator.h`, `formatter.h`, `selection.h`, `terminal.h` and `types.h`.
  - The reason is Go's stack scanner aborting on sentinel addresses.
  - Rustty deliberately returns sentinels: `rustty-vt-c/src/allocator.rs:21` (`EMPTY_BYTES = usize::MAX`), `rustty-lib/src/types.rs:18-23` (the dangling pointer of an empty `Vec`), and the kitty temp-dir getter (`terminal.rs:4262-4276`). Recorded as N331 (`NEW-FINDINGS.md:5989-6013`) and N361 (`:6804-6820`).
  - Re-align both and flip `formatter.rs:985` and `allocator.rs:135`.
  - **Tests (6):** `String.init empty output`, `alloc zero length` (+1 assertion), `format_alloc empty output`, `selection_format_alloc empty output`, the continuation exact-suffix test, and `kitty_image_medium_temp_file empty output` (the last is R14 cfg-gated on `KITTY_GRAPHICS`).
  - Re-run Miri on the vt-c `formatter::|selection::|terminal::continuation|allocator::` tests. N361's provenance work sits on the same sentinel.
- **#9 `105d0a545`.**
  - Upstream: `tmp_dir_path: ?[]u8` is duped through `terminal.gpa()`, and every screen's `temporary_file.directory` borrows the one copy (`c/terminal.zig:81`, `:1370-1383`).
  - Rustty clones a global-heap `Vec<u8>` per screen (`rustty-vt/src/kitty/graphics_image.rs:168-174`; `rustty-vt-c/src/terminal.rs:91-100`, `:215-225`, `:3137-3190`).
  - The quoted reference text must move to the end state either way. The routing is an S7 T1 decision (Q3).
- **#14 `f6cb8312b`.** The `Io` owner is a NON-PORT (`rustty-vt/src/lib.rs:58-71`, `.port/phase-4/NON_PORTS.md`). Only quoted comments change: `rustty-vt-c/src/terminal.rs:207-215`, `:1694`, `:1940-2015`; `snapshot.rs:842-855`; `rustty-vt/src/lib.rs:66-71`. The 836-line `tinyio/test.zig` belongs to the `src/lib` reviewer. Confirm the live per-test audit carries the NON-PORT exclusion for it.
- **#24 `78494bad8`.**
  - New `include/ghostty/vt/mouse/shape.h` (`GhosttyMouseShape` 0..33 plus the `_MAX_VALUE` sentinel) and `GHOSTTY_TERMINAL_DATA_MOUSE_SHAPE = 41` (`c/terminal.zig:1700`).
  - The Rustty core already has `Terminal.mouse_shape` (`rustty-vt/src/terminal.rs:817`) and `mouse::Shape` (`mouse.rs:67-70`). The C layer is absent: `TerminalData` ends at 40 (`rustty-vt-c/src/terminal.rs:3830`, `include/rustty/vt/terminal.h:1953`).
  - Gates: `check_closed_enum_abi{,_runtime}.sh`, `check_abi_schema.sh`, `check_rustty_vt_headers.sh`, `check_symbol_manifest.sh`.
  - **Tests:** `get mouse_shape` (`c/terminal.zig:6350`), plus manifest assertions in `c/types.zig`.
  - Adjacent: `67fb0c940` (in range, `include/`-only) later folds `mouse/shape.h` into `mouse.h`. Whoever owns `include/` must port the end state, not the intermediate header.
- **#37 `b1c264163`.**
  - `ghostty_search_tick` returns `INVALID_VALUE` once the terminal is freed (`c/search.zig:176`). Before, a history search's tick read `history.start_pin` in the freed PageList.
  - Rustty has the same use-after-free (`rustty-vt-c/src/search.rs:501-525`), and no UD or N records it.
  - Port the guard through the raw-self `zig_terminal` helper (`:110-120`, already SB-safe). Extend `search_free_after_terminal_free` (`search.rs:2173`) and add `search tick after terminal free with history` (`c/search.zig:988`).
  - **The negative control only discriminates under Miri or ASan** (`check_sanitizer_canary.sh`). Run `cargo +nightly miri test -p rustty-vt-c search::`.
- **Option and data enum numbering.** In Rustty the last option is 39 (`terminal.h:1547`) and the last data value is 40 (`:1953`).
  - Upstream end state: `OPT_RESIZE_PULL_SCROLLBACK = 40` (#18), `OPT_RENDER_HOLD = 41` (#20), `DATA_MOUSE_SHAPE = 41` (#24).
  - Port these in upstream order so discriminants match; A5 and the enum-discriminant gate compare them. All `c/terminal.zig` Effects, Option and Data table edits should be serialised in one worker.

### 2.4 Modes, stream and selection (#15, #16, #19, #22, #23, #25, #26, #27)

No commit here matches a UD or N id. Every fix is a straight upstream fix that Rustty had ported faithfully in its buggy form.

- **#15 `9dc0d974e`.**
  - The CSI `p` arm `2 =>` becomes `1, 2 =>` (`stream.zig:2181`). Rustty still has `2 => 'decrqm:` at `stream.rs:3402`, so its inner ANSI arm (`:3409-3413`) is dead.
  - **Tests:** `stream: DECRQM dispatch` (`stream.zig:3425`), `request mode DECRQM ANSI responses` (`stream_terminal.zig:4784`), 2 extended tests, and 1 vt-c test rename (`set write_pty callback DECRQM`). The rename must be matched by name for `audit_test_coverage.py`.
- **#16 `3beb6d717`.**
  - New `Report.Tag {value: u16, ansi}` (`modes.zig:237`); `getReport` takes it (`:80`); `max_size` comes from `maxInt(u16)` (`:251`).
  - Rustty masks with `& 0x7FFF` at `stream_terminal.rs:2532-2537` and in `rustty-termio/src/stream_handler.rs:898-901`. `modes.rs:150` and `:407-419` hold the old types, `rustty-vt-c/src/modes.rs:63-69` builds the report from the packed backing, and a test caller sits at `rustty-vt-c/src/terminal.rs:11138`.
  - The C ABI is unchanged. The termio half lands in the same change.
  - **Tests (2):** `getReport large unknown modes`, `Report.encode maximum size`.
- **#19 `079502e23`.**
  - New `src/datastruct/comptime_int_set.zig` (`ComptimeIntSet` `:49`, `indexOf` `:95`). `modeFromInt` rejects `v > maxInt(u15)` and uses a binary search (`modes.zig:186`, `:205`). Upstream measured 10.5 ns → 3.2 ns on the mode-2026 render check.
  - Rustty uses a linear scan (`modes.rs:375`).
  - Port it as a `const fn`-built sorted `[(u16, u16); N]` table with a const duplicate check and fixed-step search, `#[inline]`. It must **not** be a runtime `HashMap` or `OnceLock` (comptime-flattening hazard).
  - Keep Rustty's runtime `disabled_by_artifact` arm (S6-F2, `modes.rs:493-499`, `:642-656`) after the index lookup. The upstream `entries_any_disabled` fold covers only the comptime half.
  - Fence it with a paired bench.
  - **Tests (6 new, 1 extended):** 5 `ComptimeIntSet:`, plus `modeFromInt matches entries exhaustively`, which runs 2 × 65,536 iterations. Under Miri that needs an R14-declared cap (Q7).
- **#23 `a4f0d9f4a`.**
  - `printSlice` keeps charset-mapped GL on the fast path. `charset_table: ?[]const u16` is computed in `printSliceFill` (`Terminal.zig:849`) and applied in `printSliceStoreRun` (`:770`) as a scalar loop that returns before the vector loop.
  - Rustty homes: `terminal.rs:180`, `:1355-1366`, `:1388`, `:1541`, `:1647`; `charsets::table` already exists (`charsets.rs:53`).
  - Port `assert(charset_table == null or !allow_unicode)` as a checked invariant (R7).
  - Needs a perf fence on the print benches. Bench after #6 lands, because cache-line-aligned cells move the same numbers.
  - **Tests (3 new):** `Terminal.zig:14347/14385/14421`. The differential test randomises over every `Charset`.
- **#26 `d4f45bee3`.** `if (count == 0) return;` after pending wrap (`Terminal.zig:1942`) goes at `terminal.rs:3064-3067`. **Test:** 1 (`:11022`).
- **#22 `c4f15c884` → #27 `a3e80a685`.**
  - `selectWord`: the hard-break check moves before the boundary test and reads `prev` (`Screen.zig:3297`). New `selectWordCodepoint` resolves the spacer tail and head (`:3353`).
  - Rustty: `screen.rs:5008-5080`.
  - **Tests (3):** `Screen.zig:10214/10254/10286`.
  - Check vt-c `selection_gesture` and core double-click tests for pinned old end points.
- **#25 `714fe9b90`: cross-tier.**
  - vt: `Action.ResizeWindow` (`stream.zig:358`), the CSI t arm `8` (`:2395`), and a no-op in `stream_terminal.zig:565`.
  - termio forwards it (`stream_handler.zig:307`).
  - Outside this area:
    - `Surface.resizeWindow`, the minimum resize cells, and `DerivedConfig`;
    - `apprt/action.zig`: `resize_window {u32, u32}`;
    - the GTK unimplemented list;
    - `config`: `vt-window-resize-allowed = false`;
    - `include/ghostty.h`: `GHOSTTY_ACTION_RESIZE_WINDOW` plus a union member (8 B, so the union size is unchanged);
    - two macOS Swift files.
  - No Rustty trace exists anywhere.
  - Gates: A4 (embed by-value ABI), A5, the embed header-enum check, and config G3b/helpgen.
  - **Test:** 1 in vt (`stream.zig:4236`). The Swift side is untested upstream.

### 2.5 RenderState (#20, #28, #29)

- **#28 `ca4f719f7`.**
  - `overscan_request`/`overscan` (`render.zig:173`, `:185`). `row_data` is `above + rows + below` long, with `viewportStart`/`viewportY`/`rowDataRange` (`:1024-1050`) and `Row.Id {serial, y}` (`:371-389`).
  - `beginUpdate` (`:519-590`) walks `upOverflow`/`downOverflow`, and two new triggers force a full redraw.
  - The cursor is reported only inside the viewport (`:748`). `overscan` is written after the loop (`:845`), which keeps the UD315 "failed update forces rebuild" property.
  - `string`/`linkCells` slice to the viewport (`:1201-1290`).
  - The bench gains 6 `ScreenClone` modes (`src/benchmark/ScreenClone.zig`), absent from `crates/rustty-bench/src/screen_clone.rs`. G-BENCH reads `origin/main` and may already flag them.
  - Rustty `render.rs:134-180`, `:992-1053`, `:1106-1149`, `:1178-1355`, `:1491`, `:1584`, `:1641`. All the pin primitives exist (`pagelist/pin.rs:537`, `:556`, `:606`).
  - Overscan multiplies the per-row allocations that N315 (per-column cell storage, user-accepted) already triples. Restate N315's worked number.
  - `resize_rows_capacity` (`render.rs:2082`) must take `row_data_len`.
  - **Tests (8):** `render.zig`, plus the `testCompareStates` assertions.
- **#29 `2ea1eeae2`.**
  - The wrapper copies `overscan_request` into the state on update (`c/render.zig:32-36`).
  - The iterator is cut to `rowDataRange()`; `viewport_y_base` is at `:71`.
  - New by-pointer structs `Overscan` (4 B) and `RowId {u64[2]}` (16 B, `{serial, y+1}`) at `:122`, `:141`.
  - New enum values: `DATA_OVERSCAN = 20`, `DATA_OVERSCAN_REQUEST = 21`, `OPTION_OVERSCAN = 1`, `ROW_DATA_VIEWPORT_Y = 6`, `ROW_DATA_ID = 7`.
  - A large amount of `render.h` prose (a G3b mass).
  - **SB hazard.** Rustty's `RowIteratorWrapper.rows: *mut [RenderRow]` (`rustty-vt-c/src/render.rs:118-145`, PORT NOTE `:128-140`) must stay buffer-root. Build the sub-slice with `ptr::slice_from_raw_parts_mut(row_data.as_mut_ptr().add(start), len)`, never `&mut row_data[range]`: `rustty_render_state_clean` interleaves, which Miri caught before in `render_row_get_set_dirty`.
  - The structs are by pointer, so the sret hazard does not apply, but the types manifest and closed-enum gates do.
  - **Tests (11):** 10 in `c/render.zig`, 1 in `c/types.zig`.
- **#20 `e50779498`.**
  - `Effects.render_hold` (`stream_terminal.zig:204-245`, `:1607-1611`). `setMode(.synchronized_output)` reports only real transitions, and its later `unreachable` is an R7 invariant, not R13.
  - `restore_mode`, `full_reset` and `resize` release the hold. In C: `RenderHoldFn`, a trampoline (`c/terminal.zig:598`), `OPT_RENDER_HOLD = 41`, and C `reset()` fires `false`.
  - Rustty treats 2026 as a pure bit (`stream_terminal.rs:1662-1666`).
  - **SB hazard: the callback is documented to call `RenderState.update(terminal)` re-entrantly.**
    - In the Zig-API path, no `term()` borrow may be live across `renderHold`.
    - In C, `vt_write` holds `&mut (*w).stream` while the callback forms `&mut *(*w).terminal`. The fields are disjoint, but nothing proves it.
    - Add a vt-c test that calls `rustty_render_state_update` inside the callback and run it under Miri, as a new target in the `vtc` group.
    - In C `reset`, reach the handler via `addr_of_mut!`.
  - **Tests (3):** 2 in `stream_terminal.zig`, 1 in `c/terminal.zig`.
- **No consumer in the GUI tier.** A `git grep overscan|render_hold` over `src/renderer`, `termio`, `apprt` and `Surface.zig` at `origin/main` is empty.

### 2.6 OSC integer parsing (#30, #33–#36)

**Upstream.**
- New `src/lib/parse_int.zig:16-53`, exported via `src/lib/main.zig` and `src/terminal/lib.zig:20`. It refuses `_`, refuses signs on unsigned types, parses in one pass with an accumulator of `@max(8, bits)`, and carries the rationale doc at `:11-15`.
- Applied at `osc/parsers/color.zig:178,251`, `osc9.zig:29,197`, `kitty_text_sizing.zig:50`, `semantic_prompt.zig:171`, `kitty_desktop_notification.zig:211`, `context_signal.zig:166`, **`kitty/color.zig:52`** and **`color.zig:720` `RGB.fromHex`**. The last also reaches config and themes: `rgb:f_f/0/0` is now `InvalidFormat`.
- Still on `std.fmt.parseInt` at `origin/main`: the glyph APC (`apc/glyph/request.zig:106,217-222,372`), the kitty graphics command (`kitty/graphics_command.zig:321-322`), `color.zig:67`, `x11_color.zig:45-47`, and tmux.

**Rustty.** **UD141** (`UPSTREAM-DEFECTS.md:1737`; `docs/ghostty-bug-fixes.md:25`) already refuses `_` through `rustty-vt/src/protocol_int.rs:31`, at the OSC sites plus the glyph APC and kitty graphics. It differs from upstream's shape in three ways:
1. It deliberately **keeps accepting signs** on unsigned fields (`+11` and `-0` are pinned accepted at `osc/parsers/color.rs:805ff`).
2. It never covered OSC 21 (`kitty/color.rs:169`, test `:236-237` pins `"1_2" == 12`) or `RGB::from_hex` (`color.rs:1033`, test `:1857-1859` pins r=15).
3. Upstream has not fixed the glyph APC or kitty graphics.

**Port.**
- `crates/rustty-lib/src/parse_int.rs` (the `src/lib` home), with the width-faithful accumulator: i3 `"-4"` succeeds and `"4"` overflows, so no `i128` shortcut.
- Call it at the 9 upstream sites. Keep `protocol_int` only for glyph and kitty graphics.
- Flip the pinned tests. Narrow UD141 to "partially fixed upstream in `73768913b`; remaining: glyph APC, kitty graphics command" and update the bug-fixes row.
- Run the rustty-config tests (RGB).

**Tests (9):** `protocol integer parsing`, `RGB.parse rejects digit separators`, OSC 21/4/99/66/9/133 separator tests, and `OSC 3008: numeric fields reject non-digits` (#36).

### 2.7 Kitty graphics (#12, #31)

- **#12 `0b5446364`.**
  - New `kitty/windows.zig`: `checkPath` `:42` (UNC, device namespaces, reserved DOS names), `checkCanonicalPath` `:67`, `isDriveAbsolute` `:75`.
  - `readFile` checks before opening (`graphics_image.zig:337`, `:354-359`). The canonical buffer is now allocated through `alloc` (about 96 KiB on Windows).
  - `validatedFilePath` (`:474`) and `isPathInTempDir(alloc)` (`:497`, now `Allocator.Error!bool`).
  - Rustty homes: `graphics_image.rs:734-800`, `:939-958`, `:964-990`, and the Windows shims at `:1840`/`~:1887`.
  - Route the buffer through `VtAllocator` and record the T3 shape in N360.
  - **Tests (13):** 6 pure tests in `windows.zig` run everywhere. 7 `image load: windows …` tests are `#[cfg(windows)]` (R14); one of them ("UNC share") needs the R14 *runtime* skip form.
  - Validation needs the rwin lane.
- **#31 `4b86e3594`.**
  - New `pkg/wuffs/src/zlib.zig` `decode(alloc, data, limit, size_hint)`: doubling growth capped at `limit`, final shrink.
  - `decompressZlib` (`graphics_image.zig:635`) passes a size hint and **propagates `OutOfMemory`**; `WuffsError`/`Overflow` become `DecompressionFailed`. New testdata file (95 B).
  - Rustty uses `zlib_sys::inflate_zlib_into` (`graphics_image.rs:1209-1254`) and maps OOM to `DecompressionFailed`.
  - `rustty-wuffs` is **optional** (`rustty-vt/Cargo.toml:146-156`, `:257`, off for `-p rustty-vt-c`). `crates-sys/wuffs-sys/build.rs:61-71` already compiles the zlib module.
  - The port makes Wuffs mandatory whenever kitty graphics is on and the target is not wasm32, **drops `zlib-sys` from rustty-vt**, and splits "wuffs linked" from the `wuffs` PNG-decoder feature (Q5).
  - **Closes the zlib half of UD140** (`UPSTREAM-DEFECTS.md:1722`). The `readFile` half is still `.limited(max_size)` upstream (`:461`). Retire the zlib PORT NOTE at `graphics_image.rs:1221-1226`.
  - Wuffs is C and cannot run under Miri: `cfg_attr(miri, ignore = "… (R14)")`.
  - **Tests (4):** 3 in `zlib.zig` (belongs to the `pkg/` reviewer, lands in `rustty-wuffs`) and 1 `image load: png, zlib compressed, direct`.
  - Re-run G-VTFEAT, the symbol manifest, G-EXAMPLES, and the Linux and Windows lanes.

### 2.8 tmux (#17, #21, #32)

- **#17 + #21 (`a41248013`, `9a4ba7d54`).**
  - `receivedListWindows` (`viewer.zig:853`) now runs `syncLayouts` first and then appends `.windows = self.windows.items`. The errdefer truncates to the captured `actions_len`. `Action.format` prints `[{d} windows]` (`:234`).
  - Rustty clones a `Vec` into `Action::Windows` **before** `sync_layouts` (`tmux/viewer.rs:1354-1462`, `:1456-1461`). The UAF is impossible, but the order and format differ.
  - Re-align the order and errdefer and change `Display` (`:311-360`). This makes output byte-identical and **retires LEDGER S209-F3** (`LEDGER.tsv:487`).
  - The test's pointer-identity assert has no owned-`Vec` equivalent (Q4).
- **#32 `1ca2858d0`.**
  - New mapping: `mouse_all_flag`→1003, `mouse_button_flag`→1002, `mouse_standard_flag`→1000, and nothing→X10.
  - `mouse_any_flag` is removed from `output.zig` `Variable` and from the `list-panes` format, which is a wire change.
  - Rustty: `output.rs:196, 258, 337`, test `:664-668`; `viewer.rs:1655-1663`, `:2301`, `:2537-2580`.
  - **Hazard:** hand-indexed `v[16]…` fields shift, and a miss misparses silently. Delete `parse_mouse_any_flag` (upstream −1).

### 2.9 Search (#1)

- `search/screen.zig:354`: `.complete => unreachable` becomes `.complete => self.state = .history`, with doc updates at `:124-129` and `:307-314`.
- Rustty `crates/rustty-vt/src/search/screen.rs:618` has `State::Complete => unreachable!()`, **reachable**: `snapshot/history.rs:467` calls `finalize(Location::Prepend)` on a list that a completed search then feeds, via the search thread or `rustty_search_*`.
- No UD or N id exists. File an N entry: a reachable pre-existing panic the parity closure missed, fixed upstream. A faithful port removes it, so no R13 divergence remains.
- **Tests (3):** in `search/{pagelist,screen,terminal}.zig`.
- Re-run the vt search Miri lane plus `pagelist::builder`. Order after #7, because prepend must preserve the tracked pin's node identity under per-node pooling.

## 3. Proposed workstreams, ordering and effort

At most 3 building worktrees at once (memory `parallel-worktree-disk-budget`).

| WS | commits, in order | depends on | effort |
|---|---|---|---|
| **R1-A memory model** (one worker, sequential) | `ffe015ee5` → `c0a4f80d8` → `6112935a2` → `d2ff6d77a` (+ N88 re-probe) → `e01e75bbb` (`UntouchedPool` in rustty-datastruct) → `efa3c66ed` → `35a6fb747` → `60b43068c` (rwin smoke) | none; critical path | 5–6 d |
| **R1-B C-API tables** (one worker; owns every `c/terminal.zig` Effects/Option/Data edit) | `b1c264163` → `dd2edd760` → `105d0a545` → `f6cb8312b` (comments) → `78494bad8` → `c55f213aa` (core + C opt 40) → `e50779498` (C opt 41) → `d1cd56a56` (+ termio half) | the `c55f213aa` PageList part rebases onto R1-A's `resize.rs`; land after R1-A or accept churn | 4–5 d |
| **R1-C render** | `ca4f719f7` → `2ea1eeae2` (header after `e50779498`) + 6 bench modes | `e50779498` (header order); after `efa3c66ed` for the serial/row-id check | 2–2.5 d |
| **R1-D modes/stream/selection** | `9dc0d974e` → `3beb6d717` (+ termio) → `079502e23` (`ComptimeIntSet`, perf fence); `d4f45bee3`; `c4f15c884` → `a3e80a685`; `a4f0d9f4a` (bench after `d2ff6d77a`); `714fe9b90` (cross-tier; coordinate with the core/apprt/macOS reviewers) | independent, apart from the bench ordering | 3–3.5 d |
| **R1-E OSC/kitty/tmux/search** | parse_int set (`73768913b`, `d92e3400e`, `500040d3b`, `31bbb1273`, `3822aa01d`) as one end-state task; `0b5446364` → `4b86e3594` (build-graph change); `a41248013` + `9a4ba7d54` → `1ca2858d0`; `06178eeaa` last | `06178eeaa` after `efa3c66ed` | 3.5–4 d |
| **R1-F verification** | G6/G6c/G6b Miri (filtered lanes above), the S7 Miri groups `vt-pagelist`/`vt-render`/`vtc`, `sanitizers.sh`/`check_leaks.sh`, alloc/`sc.clone`/print/render bench re-baselines against a zigbench at `origin/main`, the rwin lane (#12, #13, #31), the Linux lane early (geometry table 2, #31 build graph), A5/closed-enum/ABI-schema/symbol-manifest, G3b, and the live per-test audit | all | 2–3 d |

**Total: about 20–24 dev-days.** Porting is about 18–21 days, plus verification. On the critical path with R1-A in parallel with R1-D and R1-E, then R1-B and R1-C: about 10–12 working days wall-clock.

**Gate bookkeeping to do at the advance (a blind spot, like Phase 6's `search/terminal.zig`).**
- The phase-0 `crate_map` (`.port/phase-0/inventory/tests-summary.json`) that G3b reads has no entry for the four NEW files: `datastruct/untouched_pool.zig`, `datastruct/comptime_int_set.zig`, `lib/parse_int.zig` and `terminal/kitty/windows.zig`. It also lacks the pre-existing `datastruct/wasm_page_pool.zig`. Their comments are invisible to G3b until the map is extended.
- G1b (`check_test_counts.py`) has no `datastruct` or `lib` row. `kitty/windows.zig` is picked up live by the `kitty` glob.
- `audit_test_coverage.py` reads `git ls-files src` live and is the honest meter for the 87 new tests, the rename, and the 2 removals.

**Records to update:**
- UD141: narrowed; partially fixed upstream in `73768913b`.
- UD140: zlib half fixed upstream in `4b86e3594`.
- LEDGER S209-F3: retire.
- N331 and N361: reversed by `dd2edd760`.
- N88: re-probe.
- N89 and N360: new T3 shapes (per-node allocations, the Windows path buffer, the wuffs realloc).
- N315: overscan counts.
- New N entries for the two defects the port carried without a record: the search `unreachable!()` (#1) and the search tick UAF (#37).
- `docs/ghostty-bug-fixes.md`: the UD140 and UD141 rows.

## 4. Open questions for the maintainer

1. **Doctrine.** Is Phase 7 advance-first like Phases 4–6? Several oracle-reading gates already read `origin/main` (A5, G-BENCH, G-LZ4). The others go RED on the advance, by design.
2. **`dd2edd760` reverses N331/N361.** Confirm that the vt-c empty-output sentinels become NULL (for allocated outputs) and a static `""` (for borrowed strings) exactly as upstream does. `rustty-lib::types::String::init` is shared, so check the rustty-apprt-c users.
3. **`105d0a545` routing.** Restructure the kitty temp dir to one wrapper-owned copy from the caller's allocator, borrowed by every screen (faithful T1, adds a raw-borrow Miri surface)? Or keep per-screen `Vec`s and record a T1/T3 residual?
4. **tmux `Action::Windows`.** The upstream test asserts pointer identity with `v.windows.items`. Should the action borrow or index `self.windows`, or should an owned `Vec` with a content-equality assert plus a PORT NOTE be accepted?
5. **`4b86e3594` build graph.** Should `rustty-wuffs` become a mandatory dependency of `rustty-vt`/`rustty-vt-c` whenever kitty graphics is on and not wasm32 (as upstream links wuffs into libghostty-vt), dropping `zlib-sys`? This changes the standalone vt-c artifact's link surface.
6. **`714fe9b90` ownership.** Its terminal half is small, but the change spans termio, core, apprt-api, config, `rustty-embed.h` and Swift. Port it in R1-D, or hand the whole commit to the core/macOS reviewer?
7. **`modeFromInt matches entries exhaustively`** runs 131,072 iterations. Under Miri, is an R14-declared `cfg_attr(miri, …)` iteration cap acceptable, or should the test run in full on the Miri lane?
8. **`UntouchedPool` test injection.** Should `MemoryPool`'s page allocator be made a generic parameter (default `PageBackingAlloc`), so the two new PageList tests port assertion for assertion with a fixed-buffer or failing allocator? Or should a test-only enum be used?
9. **N88 geometry re-probe.** It needs the Zig page probe built at `12752b2ac` on macOS and Linux aarch64. The oracle tree must not move before the advance, so should the probe run from a separate `git worktree` of `../ghostty` at `origin/main`, or wait for the advance?
10. **Palette `Copy` loss (`d1cd56a56`).** `DynamicPalette` and `Colors` stop being `Copy`. Is an owner enum (static reference or `VtAllocator`-owned copy) the accepted shape, given that the termio half must move in the same change?
11. **Windows-only commits (#12, #13).** The rwin lane is the only validation. Is an rwin run a hard requirement for R1-A and R1-E, or can those workstreams merge on G-WINCHK cross-checks with rwin at the finale?
