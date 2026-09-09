# P6-W1 — vt-core (rustty-vt, rustty-lib, rustty-vt-c/lib.rs freestanding half)

Range `88f57ee66..20abdb50a`, oracle working tree AT `20abdb50a`. Branch `phase-6/w1-vt-core`.

## Coverage statement (what was read, hunk by hunk)

Every hunk of the range diff (`.port/phase-6/UPSTREAM_DIFF.patch`) for the W1 files was read
against the oracle END-STATE file region, then the rustty home was located by grep (not
assumed from the plan table) and read:

| oracle file | patch lines | end-state region read | rustty home (verified) |
|---|---|---|---|
| `src/terminal/ref_counted_set.zig` | 9436-9560 | `addWithIdContext` 290-360, test 762-820 | `crates/rustty-vt/src/ref_counted_set.rs` (`add_with_id_ptr`, `upsert`, `mod tests`) |
| `src/terminal/Screen.zig` | 8098-8114 | `splitCellBoundary` 1900-1965 | `crates/rustty-vt/src/screen.rs` `split_cell_boundary` |
| `src/terminal/Terminal.zig` | 8114-8174 | tests at 9922-9956, 13388-13400 | `crates/rustty-vt/src/terminal.rs` `mod tests` (`terminal_erase_chars_wide_char_wrap_boundary_conditions`, `terminal_delete_chars_wide_char_wrap_boundary_conditions`) |
| `src/terminal/PageList.zig` | 8035-8098 | `pageAllocator` 540-565, the three `MemoryPool.init` call sites | `crates/rustty-vt/src/pagelist/mod.rs` (`page_backing_alloc`, `MemoryPool::init`, the quoted-reference block) |
| `src/lib/allocator.zig` | 7893-7908 | `default` 20-45 | `crates/rustty-lib/src/allocator.rs` `default` |
| `include/ghostty/vt/allocator.h` | 621-670 | the three doc paragraphs | `include/rustty/vt/allocator.h` |
| `src/terminal/build_options.zig` | 8174-8202 | `Features` 60-110, `kittyGraphics`/`add` 260-330 | `crates/rustty-vt/src/build_options.rs`, `crates/rustty-vt/src/options.rs` (`KITTY_GRAPHICS`), `crates/rustty-vt/Cargo.toml` (feature comment) |
| `src/lib_vt.zig` | 7908-8035 (the `std_options`/`native_freestanding`/`debug_machinery` hunks; the `features.search` export hunk is W3's) | 434-500 | `crates/rustty-vt-c/src/lib.rs` (crate root = `lib_vt.zig` analog; `std_options.logFn` analog is `sys::log_fn`) |
| `src/terminal/snapshot/snapshot.zig` | 10952-10990 | `encode` 40-60 | `crates/rustty-vt/src/snapshot/codec.rs` `encode`, `snapshot/record.rs` `Writer`, `snapshot/io.rs` |
| `src/terminal/kitty/graphics_image.zig` | 9345-9436 | `readSharedMemory` 194-235, `validSharedMemoryName` 843-851, tests 874-920 | `crates/rustty-vt/src/kitty/graphics_image.rs` (`readSharedMemoryPosix`, `isPathInDir` neighbourhood, `mod tests`) |

Zig-only build plumbing in the same commits (`build.zig`, `src/build/*.zig`,
`.github/workflows/*`) was NOT read hunk-by-hunk beyond confirming it contains no source
semantics; verdict N-A below with the cargo/script equivalents named.

## Per-commit verdict table

Verdicts were produced BEFORE porting by reading the rustty region.

| sha | subject | verdict (pre-port) | rustty evidence (post-port) | test(s) ported | negative control | NC run? / result |
|---|---|---|---|---|---|---|
| `5f5495826` | terminal: fix living item over-count in `RefCountedSet.addWithId` | **ABSENT** — `add_with_id_ptr` still deleted the dead slot then called `upsert` and bumped `living` unconditionally (`ref_counted_set.rs:673-690` pre-port, `git show HEAD:…`) | `ref_counted_set.rs` `add_with_id_ptr`: the dead-ID branch now does `lookup_context` FIRST, ref-bumps the existing item and returns `Ok(Some(existing_id))` before `delete_item`/`insert`/`living += 1`; `upsert` deleted (PORT NOTE left in its place) | `add_with_id_dead_id_resolving_to_an_existing_value` (ref `test "addWithId dead id resolving to an existing value"`), assertion-for-assertion incl. the iterator agreement and the null-return reuse | restore the pre-fix path (delete the dead slot, lookup-or-insert, `living += 1`): `count()` reads 3 while the iterator visits 2 | **RUN** — with the mutation, `add_with_id_dead_id_resolving_to_an_existing_value` FAILED at `ref_counted_set.rs:1462` (`assertion left == right failed, left: 2, right: 3` — the `count()` after the resolving `add_with_id` read 3); restored, passes |
| `eb722cb26` | terminal: mark the previous row dirty when clearing its spacer head | **ABSENT** — `screen.rs` `split_cell_boundary` cleared the previous row's spacer head via `clear_cells` (which does not mark dirty) and returned; no `mark_dirty` on `p_row` (`screen.rs:2895-2910` pre-port) | `screen.rs` `split_cell_boundary`: `p_row.mark_dirty()` after the previous-row `clear_cells`, with the reference comment | NEW `terminal_erase_chars_clearing_wrapped_wide_char_marks_spacer_head_row_dirty` (ref `test "Terminal: eraseChars clearing wrapped wide char marks spacer head row dirty"`); `terminal_delete_chars_wide_char_wrap_boundary_conditions` gains the `clear_dirty` + three `is_dirty` assertions the reference added | drop the `p_row.mark_dirty()` line: `is_dirty(screen_pt(0, 0))` is false in both tests | **RUN** — with `p_row.mark_dirty()` removed, BOTH tests FAILED (`terminal.rs:11620` and `:12092`, `assertion failed: t.is_dirty(screen_pt(0, 0))`); restored, both pass |
| `028af92ce` | terminal: clarify page allocator fallback | **ABSENT** (comment) — the rustty quoted-reference block in `pagelist/mod.rs:990-1003` and `page_backing_alloc`'s doc still carried the OLD `pageAllocator()` doc ("This is the page allocator we'll use...") | `pagelist/mod.rs`: `page_backing_alloc` doc = the new reference doc (three sentences) + the quoted block updated to the end state | none in the reference | n/a (comment-only; G3b `vt` sentence "`alloc` is the caller-provided allocator..." was MISSING before, PRESENT after) | n/a |
| `a5423592c` | libghostty: use caller allocation on native freestanding | **ABSENT** in all three homes: `pagelist` had no native-freestanding arm; `rustty-lib` `default()` had no freestanding arm (the "Wasm" comment fell straight to `GlobalZigAllocator`); the header lacked the three "native freestanding" paragraphs | `rustty-lib/src/allocator.rs`: `default()` gains `#[cfg(all(target_os = "none", not(target_arch = "wasm32")))]` → `FailingZigAllocator` (new; `std.mem.Allocator.failing`: alloc/remap null, resize false, free no-op) with the reference comment; `include/rustty/vt/allocator.h`: the three paragraphs; `pagelist/mod.rs`: the "Native freestanding targets don't have an OS page allocator..." comment + PORT NOTE explaining why the arm is NOT compiled (see the assessment paragraph) | none in the reference | the `rustty-lib` arm is `cfg`-dead on every target we build; the only way to exercise it is a `target_os = "none"` build (not available — see below). Not runnable; recorded as such | not run (cfg-dead) |
| `3376153a4` | build: support native freestanding libghostty-vt (source halves) | **ABSENT**: `build_options.rs` inlined the OLD condition (`cpu_arch == Wasm32 && os_tag == Freestanding`) inside `add` and had NO `kittyGraphics` method; `options.rs::KITTY_GRAPHICS` ANDed `target_arch = "wasm32"`; `rustty-vt-c/src/lib.rs` had no `native_freestanding`/`debug_machinery` analog (only the `logFn` analog in `sys.rs`) | `build_options.rs`: new `Options::kitty_graphics(self, target)` with the reference doc, `if target.os_tag == OsTag::Freestanding { return false; }`; `add` calls it (with the reference's "Kitty graphics is force-disabled on some targets" comment); `CpuArch` doc says the arch is no longer inspected. `options.rs`: `KITTY_GRAPHICS = cfg!(feature) && !cfg!(target_os = "none")`. `Cargo.toml` feature comment updated. `rustty-vt-c/src/lib.rs`: `NATIVE_FREESTANDING` + `DEBUG_MACHINERY` consts with the reference docs and a per-knob note for `page_size_min/max = 16` and `allow_stack_tracing = false` | test expectations CHANGED with the reference: `kitty_graphics_enabled_on_freestanding_non_wasm32` (asserted TRUE) is now `kitty_graphics_disabled_on_freestanding_non_wasm32` (asserts FALSE) — said so in a PORT NOTE at the test; new Rust-only `kitty_graphics_method_ignores_feature_on_freestanding`; `options::tests::kitty_graphics_enabled_on_this_target` re-derived on `target_os = "none"` alone | restore the `cpu_arch == Wasm32 &&` clause in `kitty_graphics`: `kitty_graphics_disabled_on_freestanding_non_wasm32` fails | not required by the brief (not a bug-fix commit); the renamed test is the regression guard |
| `3376153a4` | (build.zig / src/build/*.zig / CI halves) | **N-A** — Zig build-graph plumbing (`-Dtarget=…-freestanding` lane, `std_options` wiring in the build, freestanding CI job). No rustty analog exists for a native freestanding build: `crates/rustty-vt/Cargo.toml` has no `no_std`/freestanding feature and `.port/tools/build-vt-wasm.sh` is the wasm32-unknown-unknown (freestanding WASM) lane only. The freestanding CI lane is W6's per the plan, with this finding | — | — | — | — |
| `ee8095d37` | terminal/snapshot: use stack fallback for record scratch | **ABSENT** — `codec.rs::encode` built `record::Writer::init(destination)` over a heap-only `AllocatingWriter` scratch | `snapshot/io.rs`: new `StackFallbackWriter<N>` (inline `[u8; N]` + spill `Vec`, the `stackFallback(N, alloc)` analog); `record.rs`: `Writer.scratch` is `StackFallbackWriter<RECORD_SCRATCH_STACK_BYTES = 512>`; `codec.rs::encode`: the reference comment ("A measured 512-byte buffer…") + PORT NOTE on where the fallback lives | none in the reference | swap `scratch` back to `AllocatingWriter`: no test fails (the reference added none; the change is an allocation-count property). Observable via `StackFallbackWriter::on_stack()` if a future test wants it | not required (not a bug-fix commit) |
| `090fca451` | terminal/kitty: validate POSIX shared memory names | **ABSENT** — `readSharedMemoryPosix` went straight from the CString build to `shm_open`; no `validSharedMemoryName` | `graphics_image.rs`: `validSharedMemoryName(name, name_max)` + `NAME_MAX = 255` (pinned: Zig `std.posix.NAME_MAX` is 255 on every POSIX target the shm path compiles on; the `libc` crate does not expose it uniformly) and the guard at the head of `readSharedMemoryPosix` with the reference comment | `shared_memory_names_follow_POSIX_rules` (8 assertions, verbatim) and `image_load_rejects_invalid_POSIX_shared_memory_names` (cfg-gated on the same targets the reference `SkipZigTest`s) | (a) drop the `name[0] != b'/'` clause: the unit test's `!valid("kitty", 8)` fails; (b) remove the guard from `readSharedMemoryPosix`: on macOS `shm_open` of a slash-less name fails anyway (→ `InvalidData`), so the load test alone does NOT discriminate there — the unit test is the discriminating control on Darwin; on Linux the load proceeds past `shm_open` | **RUN, both mutations.** (a) FAILED as expected: `shared_memory_names_follow_POSIX_rules` at `graphics_image.rs:1524` (`assertion failed: !validSharedMemoryName(b"kitty", 8)`), while the load test still passed. (b) the load test PASSED with the guard removed (macOS `shm_open("kitty-without-leading-slash")` → ENOENT → `InvalidData`, the same error the guard produces; glibc maps a slash-less name under `/dev/shm` and would ENOENT too). So the load test is NOT a discriminating control for the guard on any host where the segment does not exist — the same property the reference test has; it pins the error-path shape (`InvalidData`, not `UnsupportedMedium`/panic), and the unit test (a) is the discriminating control. Restored, both pass |

## Native-freestanding assessment (what a `target_os = "none"` rustty-vt build would still need)

The reference's native-freestanding support is three source-level knobs (`kittyGraphics` on
any freestanding OS, `std_options` page size 16 / no stack tracing / no debug machinery, and
the caller allocator threaded into `PageList.pageAllocator`) on top of Zig's `std`, which
already builds for `freestanding` with no OS. rustty-vt is NOT `no_std`, and the honest state
after this workstream is:

- **Ported at the cfg level (compiles today, semantically faithful):**
  `options::KITTY_GRAPHICS` and `build_options::Options::kitty_graphics` now key on
  `target_os = "none"` (any arch); `rustty-lib::allocator::default` returns the failing
  allocator on `target_os = "none" && !wasm32`; `rustty-vt-c::{NATIVE_FREESTANDING,
  DEBUG_MACHINERY}` are the `lib_vt.zig` predicates.
- **Not attempted (would be a no_std conversion, out of scope by the brief):**
  1. `rustty-vt` and every crate under it (`rustty-zigstd`, `rustty-datastruct`, `rustty-os`,
     `rustty-lib`, `rustty-simd`, `rustty-unicode`, …) use `std` throughout (`Vec`, `String`,
     `Box`, `std::alloc`, `std::sync`, `std::io`, `std::fs` in the kitty file paths). A
     `none` OS target has no `std` at all; the crates would need `#![no_std]` + `extern crate
     alloc` and a `#[global_allocator]` supplied by the embedder — which is exactly what the
     reference's caller-allocator threading (`a5423592c`) provides in Zig without a global.
  2. The page pool: `rustty_os::page_allocator` is `mmap`-backed (Darwin tagged / generic
     posix). The reference's freestanding arm hands the embedder's allocator to the pool with
     `page_size_min = page_size_max = 16` as the page alignment. rustty's `MemoryPool::init`
     takes no allocator (documented as a process-wide fact); threading one in is a pagelist
     API change (`init(alloc, preheat)`) that every caller (`Screen`, `Builder`, `clone`,
     the C API, the benches) would have to carry — the same shape as the reference change but
     without a `none` target to prove it on.
  3. `std_options.page_size_min/max = 16`: the port's page-size source is
     `rustty_os::page_allocator` (`sysconf`/`vm_page_size`), with a wasm constant in
     `rustty_datastruct::wasm_page_pool::PAGE_SIZE_MIN`. A freestanding build needs a third,
     compile-time source (16) wired into `DynamicMemoryPoolAligned`'s item alignment.
  4. `allow_stack_tracing = false` / `debug_machinery = false`: Rust's analog is
     `panic = "abort"` + no backtrace machinery, which is a profile/target decision (no `std`
     means no `std::backtrace`), not a source knob — nothing to port beyond the predicate.
  5. Everything that touches an OS in rustty-vt's dependency cone (kitty temp files and shm,
     `SystemTime`, `Mutex`, the oniguruma/simd C deps, the wuffs decoder) would need the same
     cfg-gating the reference does with `builtin.link_libc`/`os.tag` checks.
- **Verification impossible here:** there is no `*-none` target with a `std` in the rustc
  toolchain, so none of the new cfg arms are compiled by any lane; they are reviewed for
  shape only. A future `--target aarch64-unknown-none` lane would be the first real check and
  is where the W6 freestanding-CI analog should land (per the plan, "with W1's finding").

## G3b (comment coverage) before/after

Before (`/tmp/w1-g3b-before.txt`, arrival state): `lib` missing=2, `vt` missing=38, `vt-c` missing=4.

After (`/tmp/w1-g3b-after.txt`, this branch): `lib` missing=**0** (both freestanding-allocator
sentences landed), `vt` missing=**7** (all seven are search-subsystem sentences, W2), `vt-c`
missing=**4** (unchanged, all W3). The gate stays RED overall until W2/W3/W4 land (config 6,
core 1 are W4's).

Remaining `vt` misses NOT in W1's scope, with attribution (grep of the oracle tree):

| sentence (as the gate prints it) | oracle file | workstream |
|---|---|---|
| "(0 = most recent match). returns null if the index is out of" | `search/screen.zig` | W2 |
| "return the match at the given index in newest-to-oldest order" | `search/screen.zig` | W2 |
| "a screen switch makes all previously notified per-screen state" | `search/Thread.zig` | W2 |
| "feed the search from the terminal state. the terminal mutex must" | `search/Thread.zig` | W2 |
| "the active screen key the state below was captured against." | `search/Thread.zig` | W2 |
| "the last-notified values used by `notify` to diff search state" | `search/Thread.zig` | W2 |
| "the last-notified values used to diff search state into events." | `search/Thread.zig` | W2 |

Remaining `vt-c` misses (all W3, `c/selection.zig` / `include/ghostty/vt/selection.h`): "a caller-provided buffer of selections. this follows the same",
"conventions as rusttybuffer: ptr may be null with cap 0 to query", "the required capacity, and
len is set to the entries written on", "success or to the required capacity on
rustty_out_of_space".

The `build_options.zig` `search: bool` feature doc ("Terminal search: find matches…", four
G3b sentences) is in W1's file table, so the FIELD and its doc are carried in
`build_options.rs` (`Features.search`, `for_each_feature!`, `Default`, `NONE`, `Resolved`);
the cargo `search` feature, `options::SEARCH` and the `rustty_search_*` export gate are W3's
(PORT NOTE at the field says so).

## What was not done, and why

- No `no_std`/freestanding build of rustty-vt (out of scope by the brief; assessment above).
- The pagelist caller-allocator threading (`pageAllocator(alloc)`): comment + PORT NOTE only;
  the arm cannot be compiled or tested without a freestanding target (see item 2 above).
- `ee8095d37` has no reference test; the stack-fallback property is exposed
  (`StackFallbackWriter::on_stack`) but no Rust-only test was added for it.
- The `lib_vt.zig` `features.search` export block and `c/main.zig` search re-exports in the
  same range hunks are W3's and were left untouched.

## Verification (final)

All run on this branch after the final edit (macOS arm64, cold worktree):

```
cargo test -p rustty-vt                 test result: FAILED. 2973 passed; 1 failed; 3 ignored  (307.13s)
                                         -> search::thread::tests::search_thread: the reference's own
                                            100 ms `timedWait` (thread.rs:2378) timed out under a
                                            fully loaded machine (another session's cargo was running);
                                            re-run alone 3/3 ok, and green in the simd run below.
                                            Untouched by W1 (search/ is W2's).
cargo test -p rustty-vt --features simd  test result: ok. 2974 passed; 0 failed; 3 ignored  (299.02s)
                                         + 10 integration/doc binaries all ok
cargo test -p rustty-lib                 test result: ok. 27 passed; 0 failed
cargo test -p rustty-vt-c                test result: ok. 469 passed; 0 failed  (+ 2, 1 ignored, 1 ok)
cargo clippy -p rustty-vt -p rustty-lib -p rustty-vt-c --all-targets -- -D warnings   Finished, clean
cargo fmt -- --check                     clean
G3b  lib 2 -> 0, vt 38 -> 7 (all W2), vt-c 4 -> 4 (all W3)
```

Negative controls (script `/tmp/w1-negctl.sh`, backups restored and `cmp`-verified):
NC1 (`5f5495826`) FAILED as required; NC2 (`eb722cb26`) both tests FAILED as required;
NC6a (`090fca451` unit) FAILED as required; NC6b (guard removal) load test still passed on
Darwin — recorded in the table, the unit test is the discriminating control.
