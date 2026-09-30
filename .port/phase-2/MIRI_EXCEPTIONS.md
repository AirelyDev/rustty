# Miri Gate — Documented Exceptions (PageList / vt-3c)

Miri is the primary memory-safety gate (R6): it proves Stacked-Borrows/provenance
soundness + leak-freedom on pure-Rust code. But Miri interprets every memory op at
~100–400× native, and a handful of PageList tests do so much page allocation +
cell rewriting (large/non-standard/many-page workloads) that they are **CPU-bound
for hours each** under Miri — empirically intractable (a full serial run never
finishes; even 8-way parallel, individual tests pegged 90+ min and were killed at
a 20-min budget). This file enumerates those tests and justifies, per strict
criteria, why excepting them from the Miri gate leaves **zero unsafe code path
unverified**.

This is NOT a weakening of the gate (R5). Each excepted test is verified by the
*right tool* (R6): native pass + AddressSanitizer (memory corruption) + the page
tracking allocator (leaks) + Miri-on-the-same-methods-via-lighter-tests (aliasing).

## Strict criteria (all four must hold)

1. **Empirically intractable** — exceeds a 20-min hard Miri budget (measured by
   `.port/tools/miri-run.sh`, not assumed). The observed gap is unambiguous:
   tractable tests ≤ ~80 s, these ≥ 90 min.
2. **Passes natively** — `cargo test -p rustty-vt pagelist` green (the exception is
   about Miri *runtime*, never a hidden failure).
3. **No uncovered unsafe path** *(the teeth)* — every PageList method the test
   exercises is also exercised by ≥ 1 test that PASSES under Miri (lighter input),
   so the method's Stacked-Borrows aliasing IS Miri-verified; the heavy test only
   does *more* of the same. PLUS the heavy test itself is run under **ASan** (no
   UAF/OOB/double-free) and the **page tracking allocator** (no leak).
4. **Intractability is allocation-volume, not logic** — large/non-standard/many-page
   allocation (Miri's per-byte overhead), evidenced by the test's setup; not a hang.

## The exception list — 23 tests

Verified GREEN by the complement: **ASan ran all 210 PageList tests in ~18 s, 0
findings** (incl. these 23) and the **page tracking allocator** asserts all pages
returned across grow/scroll/clone/resize-reflow churn (tracking allocator landed
in commit 1a2dbd9; the ASan-210 + tracking-allocator gate run is recorded in the
vt-3c integration commit 243d4af — re-pinned 2026-07-10, the original worktree
hash `58a5faa` did not survive the squash into 243d4af). The
"covering Miri-passing test(s)" column is the criterion-3 evidence: the method's
aliasing is Miri-verified by a lighter test that DOES complete.

| # | Excepted test (TIMEOUT) | PageList method(s) | Miri-verified by (lighter, passing) |
|---|---|---|---|
| 1 | `compact_oversized_page` | `compact` | `compact_std_size_page_returns_null`, `compact_insufficient_savings_returns_null` |
| 2–7 | `eraseRow_invalidates_viewport_offset_cache`, `eraseRows_invalidates_…`, `eraseRowBounded_invalidates_…`, `…_multi_page_…`, `…_full_page_shift_…`, `…_exhausts_pages_…` | `erase_row`, `erase_rows`, `erase_row_bounded`, `fixup_viewport` | the ported point-range erase tests + `erase` family that pass under Miri |
| 8–13 | `grow_allocate`, `grow_reuses_non_standard_page_without_leak`, `grow_non_standard_page_prune_protection`, `grow_prune_required_with_a_single_page`, `grow_prune_scrollback`, `grow_prune_scrollback_with_viewport_pin_not_in_pruned_page` | `grow`, `increase_capacity`, `create_page`/non-standard alloc | `active_after_grow`, `grow_fit_in_capacity`, `grow_allows_exceeding_max_size`, **`pagelist_grow_prune_miri_tractable`** (rustty-only, added 2026-07-10 — surgically-filled constant-`grow()` variant driving the PRUNE branch: `pop_first` → pin remap → buffer reset → `init_buf` reuse → `insert_after` → serial rebump; Miri-clean in ~31 s. Before it, this row self-cited its own excepted tests as coverage — VALIDATION_REPORT §3c.6) |
| 14 | `increaseCapacity_multi_page` | `increase_capacity` | the 8 passing `increaseCapacity_*` single-page tests |
| 15–19 | `resize_more_rows_and_cols_doesnt_fit_in_single_std_page`, `resize_no_reflow_less_rows_trims_blank_lines_erases_pages`, `resize_no_reflow_more_cols_forces_less_rows_per_page`, `…_forces_smaller_cap`, `…_remaps_pins_in_backfill_path` | `resize`, `resize_without_reflow`, `resize_without_reflow_grow_cols` | the many passing `resize_no_reflow_*` tests (incl. the 4 that just had their UB fixed + Miri-verified — commit 846c32c, "fix Stacked-Borrows UB in resize_without_reflow_grow_cols"; re-pinned 2026-07-10, the original hash `c46a6a2` did not survive the vt-3c squash) |
| 20–23 | `resize_reflow_exceeds_grapheme_memory_forcing_capacity_increase`, `…_exceeds_hyperlink_memory_…`, `…_exceeds_style_memory_…`, `resize_reflow_grapheme_map_capacity_exceeded` | `resize_cols`, `ReflowCursor` (reflow) | the passing `resize_reflow_*` tests (less_cols/more_cols/wrapped) |

Every method appearing above is exercised by at least one Miri-PASSING test, so no
unsafe path is left un-Miri-checked; the heavy tests add ASan + leak coverage on
top. Criterion 4 holds: each is heavy due to allocation volume (oversized/
non-standard pages, multi-page reflow, capacity-exceeding grapheme/style/hyperlink
churn), not a logic anomaly.

## WS7 Phase C additions — 8 more tests (31 total)

Phase C (generation-stamp renewal + bounded pruning of stale serials, oracles
`dd956f37c`/`91a63e528`/`a804e3ac7`/`f1a5fab45`/`f6d9a582e`) added 10 new PageList
tests. Per the review finding (I1), each of the 10 was individually measured with
`.port/tools/miri-run.sh rustty-vt <exact-test-name> 1 3 4` (a bounded 4-minute
hard timeout, single worker, warm Miri build) rather than assumed from the class.
**8 are intractable (TIMEOUT at the 4-min bound, killed cleanly by the harness);
2 are tractable and were deliberately left un-annotated** — this is the
criterion-4 discipline (allocation-volume, not logic; measure, don't
blanket-annotate):

| Test | Result | Measured |
|---|---|---|
| `pagelist_only_reset_advances_the_serial_epoch` | TIMEOUT | 240s @ 4min bound (unbounded run by the reviewer ran >9min) |
| `pagelist_row_erasure_renews_affected_page_generations` | TIMEOUT | 240s @ 4min bound (unbounded run by the reviewer ran >9min) |
| `pagelist_trailing_row_truncation_renews_page_generation` | **tractable, left unignored** | 9s |
| `pagelist_bounded_pruning_after_split_invalidation_preserves_live_serials` | TIMEOUT | 240s @ 4min bound |
| `pagelist_bounded_pruning_after_partial_erase_preserves_live_serials` | TIMEOUT | 240s @ 4min bound |
| `pagelist_partial_erase_restarts_compression_before_continuation` | TIMEOUT | 240s @ 4min bound |
| `pagelist_repeated_bounded_pruning_after_split_preserves_live_serials` | TIMEOUT | 240s @ 4min bound |
| `pagelist_bounded_pruning_after_front_replacement_preserves_live_serials` | TIMEOUT | 240s @ 4min bound |
| `pagelist_bounded_pruning_after_middle_replacement_preserves_live_serials` | TIMEOUT | 240s @ 4min bound |
| `pagelist_replacements_preserve_compression_continuation_and_mark_activity` | **tractable, left unignored** | 37s |

All 8 TIMEOUTs ran 0 UB before being killed (the harness's `killtree` fires only
after the full 240s, well past when Stacked-Borrows/leak errors would already have
printed). The 8 are now annotated in `crates/rustty-vt/src/pagelist/tests.rs` with
the same `#[cfg_attr(miri, ignore = "MIRI EXCEPTION …")]` form and reason wording
as the 23 above (they are the same class: `while s.total_pages() < N`-style
multi-page construction, `fill_compressible_history`, or `fill_last_page_for_test`
driving a real prune). Per-test criterion-3 coverage (the method's Stacked-Borrows
aliasing is Miri-verified by a lighter, passing test):

| Excepted test | PageList method(s) | Miri-verified by (lighter, passing) |
|---|---|---|
| `pagelist_only_reset_advances_the_serial_epoch` | `reset`, `grow` (via `fill_compressible_history`) | `pagelist_reset_invalidates_stale_untracked_refs_even_if_node_memory_is_reused` (single-page reset), the existing `grow`-family Miri-passing tests |
| `pagelist_row_erasure_renews_affected_page_generations` | `erase_row`, `erase_row_bounded`, `node_is_valid` | the ported point-range erase tests + `erase` family that pass under Miri (same coverage row as #2–7 above) |
| `pagelist_bounded_pruning_after_split_invalidation_preserves_live_serials` | `split`, `grow`, `node_is_valid` prune check | `pagelist_split_at_middle_row` and the single-page `pagelist_split_*` family (aliasing), `active_after_grow`/`grow_fit_in_capacity` (grow) |
| `pagelist_bounded_pruning_after_partial_erase_preserves_live_serials` | `erase_history`, `node_is_valid`, `grow` | the passing `erase_history`/point-range erase tests + `grow`-family |
| `pagelist_partial_erase_restarts_compression_before_continuation` | `compress(Full)`, `erase_history`, incremental-compression state restart | native + ASan (all PageList tests). **CORRECTION:** this row originally cited `pagelist_compress_incremental_restarts_after_list_mutation`/`pagelist_compress_full_mode_leaves_no_continuation` as "lighter, passing" Miri coverage — checked and found FALSE: both are themselves Miri-intractable (TIMEOUT at a 2–3 min bound, confirmed in isolation, and `…_leaves_no_continuation` reproduces the same on pre-Phase-C commit `098fc63`, so it is a pre-existing WS7-Phase-B gap, not introduced here). See the DEFERRALS.md WS7 Phase E item 3 addendum; those two need their own exception review — not claimed as this row's Miri-verified coverage anymore |
| `pagelist_repeated_bounded_pruning_after_split_preserves_live_serials` | `split`, `grow`, `node_is_valid` (repeated) | same as the split-invalidation row above; repetition is volume, not new logic |
| `pagelist_bounded_pruning_after_front_replacement_preserves_live_serials` | `increase_capacity`, `node_is_valid`, `grow` | the 8 passing single-page `pagelist_increaseCapacity_*` tests (same coverage row as #14 above) |
| `pagelist_bounded_pruning_after_middle_replacement_preserves_live_serials` | `increase_capacity`, `node_is_valid`, `grow` (repeated prune) | same as the front-replacement row above |

Criterion 4 holds for all 8: each builds a real multi-page list (`while
s.total_pages() < 2/3`) or a long compressible-history run
(`fill_compressible_history`) specifically to exercise `node_is_valid`/prune over
≥ 2 live pages — allocation volume, not a logic difference from the tractable
single-page siblings of the same methods.

The 2 left un-annotated
(`pagelist_trailing_row_truncation_renews_page_generation`,
`pagelist_replacements_preserve_compression_continuation_and_mark_activity`)
build only a single page's worth of state before asserting, so they stay on the
Miri-gated surface — this is the proof that the annotation pass was per-test, not
blanket-applied to "the other 8 also build multi-page lists" as the review
described the pre-fix state.

## Category 2 — raw syscall memory management (outside Miri's model)

Miri models Rust's abstract machine; it does not model `mmap`/`munmap`/`madvise`
on OS-backed pages. Code that manages its own mappings therefore cannot be run
under Miri at all — this is a *model* limitation, not a soundness gap, and it is
NOT a regression to chase.

| Site | Syscall | Miri coverage | Complement |
|---|---|---|---|
| `crates/rustty-vt/src/page.rs` `Backing::alloc`/`Backing::free` (~250-313) | `mmap`/`munmap` | none (pre-existing; no `.port/phase-2/mirihar-*` touches `page.rs` or `pagelist`) | native + ASan over all 210 PageList tests; page tracking allocator asserts no leaks |
| `crates/rustty-vt/src/pagelist/mod.rs` `decommit_pool_item`/`recommit_pool_item` (WS8 BL-2, oracle `896aca499`) | `madvise` (`MADV_DONTNEED` on Linux; `MADV_FREE_REUSABLE`/`MADV_FREE_REUSE`/`MADV_FREE` on Darwin) | none — the `madvise` arms are `#[cfg(not(test))]`, so Miri and `cargo test -p rustty-vt` both execute the plain-memset `#[cfg(test)]` arm instead, which IS Miri-provable and IS what upholds the pool zero-invariant. The `#[cfg(test)]` gate is crate-local to `rustty-vt`: when `rustty-vt` is compiled as an ordinary (non-test) dependency of another crate's test build — e.g. `cargo test -p rustty-vt-c` — `not(test)` is true for it and the real `madvise` arms run, so those downstream test runs are NOT covered by this Miri gap; they run outside Miri, same as any other native execution | native + ASan; the zero-invariant unit tests `pagelist_decommit_pool_item_zeroes_the_dirty_region` and `pagelist_destroyed_pool_page_reuse_is_zeroed`; the empirical footprint probe `crates/rustty-vt/examples/rss_probe.rs` |

**CORRECTION (WS7 Phase C review, I1 proof pass, 2026-07-29):** the claim above
that `pagelist_decommit_pool_item_zeroes_the_dirty_region` "IS Miri-provable"
was never actually verified under Miri and is **false as measured**. Running
the pagelist module's bounded Miri lane (as part of proving I1) surfaced a
real, reproducible Stacked-Borrows violation in that test, confirmed
deterministic in isolation
(`cargo +nightly miri test -p rustty-vt -- --exact
pagelist::tests::pagelist_decommit_pool_item_zeroes_the_dirty_region
--test-threads=1`):

```
error: Undefined Behavior: trying to retag from <NNN> for SharedReadOnly
permission at allocNNN[0x0], but that tag does not exist in the borrow stack
  --> crates/rustty-vt/src/pagelist/tests.rs:2735:13  (assert!(buf.iter()...))
help: <NNN> was created by a Unique retag at ...tests.rs:2730:24
  (let buf = unsafe { core::slice::from_raw_parts_mut(ptr, len) };)
help: <NNN> was later invalidated ... by a write access
  --> crates/rustty-vt/src/mem.rs:175:18 (core::ptr::write_bytes(ptr, 0, ...))
```

The test takes a long-lived `&mut [u8]` (`buf`) over the allocation, then calls
`decommit_pool_item` (which reaches `mem::decommit`'s `write_bytes` through a
*separate* raw pointer) while `buf`'s tag is still logically live, then reads
through `buf` again — the same long-lived-alias-plus-interleaved-write shape
this codebase's own discipline warns against elsewhere (see
`rust-mut-args-strongly-protected`/`shaper-hook-sb-clean-interleave`). **This is
a TEST-CODE bug, not new, not caused by WS7 Phase C** — `mem.rs`/this test were
last touched at commit `0413d2b` (WS7 Phase B review response) and no Phase C
commit (`6af540d`/`f94716d`/`0349439`/`e5a9217`) touches either file. It
predates WS7 entirely (WS7 B1, commit `39cd6b8`). The "native + ASan"
complement in the table above still holds (native `cargo test` passes; ASan
does not flag it, consistent with a Stacked-Borrows-only violation that
leaves no actual memory corruption).

**FIXED in WS7 Phase E (2026-07-29).** The test no longer holds any slice across
`decommit_pool_item`: each write and each read derives its own slice fresh from
`ptr`, so no reference is live while `mem::decommit` writes through its own raw
pointer. What the test asserts is unchanged — both the fully-dirty and the
partially-dirty item are still required to read back all-zero over the whole
`2 * PAGE_SIZE_MIN` buffer. Measured, same command as the reproduction above:

```
before: error: Undefined Behavior: trying to retag from <698648> for SharedReadOnly
        permission at alloc219109[0x0], but that tag does not exist in the borrow stack
          --> crates/rustty-vt/src/pagelist/tests.rs:2735:13
        help: <698648> was created by a Unique retag at offsets [0x0..0x8000]
          --> crates/rustty-vt/src/pagelist/tests.rs:2730:24
        help: <698648> was later invalidated at offsets [0x0..0x8000] by a write access
          --> crates/rustty-vt/src/mem.rs:175:18

after:  test pagelist::tests::pagelist_decommit_pool_item_zeroes_the_dirty_region ... ok
        test result: ok. 1 passed; 0 failed; ... finished in 13.28s
```

So the Category 2 table's "IS Miri-provable" claim for this test is now TRUE as
measured (13.28s under Miri), not merely asserted.

Criterion note: the WS8 entry is *stronger* than the `page.rs` precedent, because
the exact code path Miri would need to verify (the zero-invariant) is still run
under Miri via the `#[cfg(test)]` arm. Only the OS-hint calls — which have no
observable effect on program semantics, only on RSS/footprint — are unverifiable,
and those are covered by the probe below.

### The empirical footprint probe (WS8 T13)

**Why it is an EXAMPLE, not an `#[ignore]`d test.** The WS8 plan specified an
`#[ignore]`d test run via `cargo test --release -- --ignored`. That does not work
and was *measured* not to work: a libtest build always has `cfg(test)` true, so
`decommit_pool_item` compiles its memset arm, and the memset **commits** the
pages — the probe recorded RSS going *up* (grow 8.8 MiB → clear 17.4 MiB) rather
than down. A standalone example is the only build in which the library's
production `cfg` arms and production page geometry are compiled; this is the same
reason `examples/alloc_bench.rs` exists (see `crates/rustty-vt/Cargo.toml`, the
`alloc-bench` feature comment). The probe therefore lives at
`crates/rustty-vt/examples/rss_probe.rs`.

**macOS metric.** `MADV_FREE_REUSABLE` removes pages from the process *physical
footprint* immediately; it does not necessarily move `ps -o rss`. The probe reads
`ri_phys_footprint` via `proc_pid_rusage(RUSAGE_INFO_V0)` (hand-rolled
`extern "C"`, matching the crate's no-`libc` convention). Linux reads
`/proc/self/statm` resident pages, which `MADV_DONTNEED` does move.

Run:

```bash
cargo run -p rustty-vt --example rss_probe --release
```

MEASURED, macOS 15 / aarch64, 2026-07-28:

| Build | before | after_grow | after_clear | returned |
|---|---|---|---|---|
| with decommit (landed) | 1.5 MiB | 11.4 MiB | **2.4 MiB** | **9.0 MiB** |
| negative control (`madvise` call disabled, memset only) | 1.5 MiB | 11.4 MiB | 11.4 MiB | 0.0 MiB |

The control was produced by disabling only the `MADV_FREE_REUSABLE` call and
rebuilding, then reverting — it isolates the return-to-OS to the `madvise` hint.

**Linux (`MADV_DONTNEED`) arm: NOT YET RUN.** The WS8 executor was scoped to
macOS only and explicitly barred from the `rvm` Linux gate. Outstanding action:
run the same command on `rvm` and append the row.

## Revisiting

These are documented **debt, not permanent**: a faster Miri, a Linux box with more
RAM, or smaller Miri-only input variants could fold any of them back into the Miri
gate. The set is re-derived (not hand-maintained) by `.port/tools/miri-run.sh`
with a hard timeout; if a test stops timing out, it leaves the list automatically.

## Standing gate (R6 layered net)

- **Every porting iteration:** native `cargo test` (assertion parity, ~11 s).
- **Per-crate completion / CI:** Miri (182 tractable PageList tests, 0 UB/leak) +
  these 23 via ASan + tracking allocator; `.port/tools/gates/sanitizers.sh`
  (ASan + TSan; LSan/MSan/HWASan = Linux-CI lanes).
- Result: **0 PageList unsafe path is unverified.**

---

## Crate-scale Miri compile wall (rustty-vt, from vt-11a onward)

By vt-11a (PageList + osc + kitty + stream + Screen-SCC), `cargo +nightly miri test
-p rustty-vt` takes **66+ min just to compile the crate to MIR** before any test
runs — and it pulls C deps Miri cannot execute (zlib-sys, simd base64 FFI). Per-
sub-batch full Miri is therefore **infeasible** (66 min × every sub-batch).

**Go-forward memory-safety gate for rustty-vt (R6 "right tool"):**
- **Per sub-batch (fast, every iteration):** ASan via `sanitizers.sh` (catches
  UAF/OOB/double-free — the real risk for the `*mut Pin`/`*mut Row`/`*mut Cell`
  raw pointers) + the **`addr_of_mut!` raw-pointer discipline** (structurally
  avoids the Stacked-Borrows aliasing class Miri catches — used everywhere a
  `*mut Page`/cell is derived) + adversarial faithfulness review.
- **Periodic deep-check (full Miri):** one long/overnight `cargo miri test -p
  rustty-vt` run at sub-batch FINALES (e.g. the vt-11 "complete Screen" gate) and
  on smaller still-tractable crates (rustty-zigstd/os Miri stay green every run).
  The Screen-foundation tests are light (small screens) so they DO interpret once
  compiled — the wall is the compile, not the interpretation.
- NOT splitting rustty-vt into sub-crates (would speed Miri but diverge from the
  faithful "one terminal/ module = one crate" design — R2/R3).

This extends, not weakens, the gate: ASan + discipline cover per-sub-batch; full
Miri still runs, at crate-completion granularity instead of per-sub-batch.

---

## UPDATE (post Screen-SCC): the compile wall was an anomaly — per-sub-batch Miri RESTORED

The vt-11a "66+ min Miri compile" was NOT reproducible. An isolated full-crate Miri
run on the Screen-SCC clone tests (the highest Stacked-Borrows risk — the pin-remap
into cloned pages) **compiled AND ran 15 clone tests in 44 s total, 0 UB / 0 leak**.
The 66-min figure was a cold-cache + concurrent-workflow-contention artifact in one
verify agent, not a fundamental wall.

**Revised go-forward (restores the user's "keep full Miri" preference):**
- **Per sub-batch:** run Miri on a SMALL subset of the just-filled tests that touch
  raw pointers (cursor/pin/clone/page-ptr paths) — feasible (~tens of seconds with a
  warm Miri target cache) — alongside ASan + the `addr_of_mut!` discipline. Run Miri
  in ISOLATION (not concurrent with other heavy workflows) to keep it fast.
- **At finales:** a broader Miri pass on the module's tractable tests.
- Heavy/many-page tests still time out under Miri (the original PageList exception
  list stands) → ASan covers those.

Net: Miri is back as a per-sub-batch gate for the raw-pointer-risky tests on vt-12+,
not just a periodic check.

---

## rustty-xev C3 (kqueue loop core) — MIRI exception

The kqueue backend makes real syscalls (`kqueue`/`kevent64`/`clock_gettime`/
`socket`/`bind`/`connect`/`accept`/`send`/`recv`/`close`/`shutdown`/`read`/`write`
+ mach `mach_port_allocate`/`mach_msg`/…). Miri cannot call these foreign
functions on macOS ("unsupported operation: can't call foreign function `kqueue`"),
so the **kqueue/mach tests (loop time/stop/timer×family/cancel×2/socket×2/mach
port + KAT 2) are a documented MIRI EXCEPTION** — they run natively (the live KATs
are the memory-safety proof for the raw-pointer intrusive-completion paths there;
the raw-pointer discipline is the mitigation, per `rust-mut-args-strongly-protected`).

The **pure C1 intrusive structures remain Miri-gated** and stay green after C3:
`cargo +nightly miri test -p rustty-xev` on `heap::tests::*` (excl. the slow
million-values), `queue::tests::intrusive`, `queue_mpsc::tests::intrusive` → all
pass (the crate Miri-compiles clean; the FFI-bearing loop functions are compiled
but not interpreted when those tests run). This is the same split as the FFI crates
(Miri for pure code, native/ASan for the C-boundary).

**N431 (2026-09-28).** `kqueue_callback_reenters_through_stored_loop_pointer`
(PORT-ADDED: a callback re-enters the loop through a stored pointer while
`Loop::run_raw` runs) joins this exception for the same reason. Its borrow shape is
proven twice without the exception: on the isolated harness
`.port/phase-2/mirihar-n431` (shipped shape UB, raw-self shape CLEAN, SB and TB),
and on the REAL epoll backend, whose twin
`epoll::tests::epoll_callback_reenters_through_stored_loop_pointer` runs under Miri
from the Mac via the Linux target (`cargo +nightly miri test -p rustty-xev --target
aarch64-unknown-linux-gnu --lib <name>`; Miri shims epoll/eventfd): CLEAN under SB and
TB with the fix, UB under both when entered through `(*root).run(..)` or on the
pre-N431 code.

## rustty-xev C4 (Async mach-port wakeup) — MIRI exception

C4's `async.rs` `wait`/`drain` + all 4 tests (`async`, `async: notify first`,
`async batches multiple notifications`, KAT 1) issue live mach syscalls
(`mach_port_allocate`/`mach_port_insert_right`/`mach_port_set_attributes`/
`mach_msg`/`mach_port_destroy`) via the kqueue loop — the same foreign-function
class as the C3 exception above, so they **run natively**, not under Miri. KAT 1
(cross-thread `notify` → `run(.once)` wakes exactly once) is the memory-safety +
coalescing-correctness proof for the raw-ptr `machport` trampoline. The pure C1
structures stay the Miri-gated surface.

## rustty-xev C5 (Process EVFILT_PROC wait) — MIRI exception

C5's `process.rs` `wait` + trampoline + all 4 tests (`process wait`, `process wait
with non-zero exit code`, `process wait on a process that already exited`, KAT 3)
spawn a real child (`std::process::Command` / `posix_spawn`) and drive an
`EVFILT_PROC` (`NOTE_EXIT|NOTE_EXITSTATUS`) completion whose trampoline reaps with
`waitpid` — all live syscalls via the kqueue loop, the same foreign-function class
as the C3/C4 exceptions above, so they **run natively**, not under Miri. KAT 3
(`posix_spawn /usr/bin/true` → `Process.wait` → `run(.until_done)` asserts exit
status 0) is the memory-safety + exit-decode-correctness proof for the raw-ptr
`proc` trampoline. The pure C1 structures stay the Miri-gated surface.

## rustty-xev C6 (Stream/WriteQueue ordered-write engine) — MIRI exception

C6's `stream.rs` `GenericStream` (`read`/`write`/`close`/`poll`/`queueWrite`) + the
4 reference tests (`Stream decls`, `pty: child to parent`, `pty: parent to child`,
`pty: queued writes`) + rustty KAT 4 open a real pty (`openpty`) and drive
`EVFILT_READ`/`EVFILT_WRITE` completions whose trampolines call live
`read`/`write`/`close` syscalls via the kqueue loop — the same foreign-function
class as the C3/C4/C5 exceptions above, so they **run natively**, not under Miri
(`Stream decls` is a pure compile-time reflection test and would run under Miri, but
the pty tests would hit "can't call foreign function `openpty`"). KAT 4 (`openpty`
→ `queueWrite` a payload → read it back on the child end → assert bytes + the
`WriteQueue` drains) is the memory-safety + ordered-write/partial-write-correctness
proof for the raw-ptr `queueWrite` trampoline (the `WriteRequest::from` container_of
recovery + the re-`writeInit` remainder path). The pure C1 structures stay the
Miri-gated surface.

## rustty-xev C7 (package surface + Timer reset/cancel finale) — MIRI exception + Miri-clean margins

C7's un-deferred `Timer::reset` (delegates to `loop.timer_reset`, reusing the `run`
trampoline) + `Timer::cancel` (a `.cancel`-op `Completion::new_op` + `loop.add` +
`cancel_trampoline`) and their 5 reference tests (`timer`, `timer reset`, `timer reset
before tick`, `timer reset after trigger`, `timer cancel`) drive the real kqueue loop
(`kqueue`/`kevent64`/`clock_gettime` via `run(.until_done)`) — the same
foreign-function class as the C3–C6 exceptions above, so they **run natively**, not
under Miri. The loop-level `timer`/`timer_reset`/`.cancel`-op machinery they ride was
already exercised by the C3 kqueue timer×family tests; KAT 2 (Timer fires) is the
live backstop.

C7's NEW pure code IS Miri-clean and stays on the Miri-gated surface: `backend.rs`
(the `Backend` enum + `default()`/`candidates()`, no `unsafe`, no raw ptrs) and
`lib.rs`'s `api_tests` (`ref_all_decls` = `size_of` reflection; `completion_is_zero_able`
= `Completion::default()`, which only writes null ptrs / the noop callback, never
derefs). The pure C1 intrusive structures (`heap`/`queue`/`queue_mpsc`) remain the
Miri-gated core and stay green after C7 (surface-only chunk — the C1 files are
untouched).

## rustty-termio B1 (Subprocess.start fork/exec spawn) — MIRI exception

B1's un-deferred `Subprocess::start` (`exec.rs`) + its rustty-only KAT
`subprocess_start_spawns_and_reaps` open a real pty (`openpty`), `fork()` +
`execvpe` a real child, and `stop()` it (`killpg(SIGHUP)` + `waitpid`) — all live
syscalls of the same foreign-function class as the rustty-xev C5 (Process wait)
exception above (`fork`/`execve`/`openpty`/`waitpid` are "can't call foreign
function" under Miri on macOS), so the KAT **runs natively**, not under Miri.
`fork` in particular Miri cannot model at all (it never sees the child's copied
address space).

The teeth (why no unsafe path is left un-verified): the KAT is the memory-safety +
spawn-correctness proof for the scoped-borrow spawn (blueprint §2). The
Stacked-Borrows surface it exercises is the PARENT side — the transient
argv/env/cwd borrows scoped to `start()`'s frame, the raw `*mut Subprocess`
self-pointer (`data`) derived via `addr_of_mut!` before any `&self.*` borrow and
NEVER dereferenced in the parent, and the stored `Process::ForkExec(Command)`
whose reference fields are TRUE `'static` empties (`c""`/`&[]`/`None`) so no
self-referential borrow exists at rest. That parent-side borrow scoping is what
Miri *would* check; it is designed SB-clean (no `&'static`-to-non-static, no
write through the raw self-pointer while `&self.env` is live — [ADV-FIX F4]) and
is small/side-effect-free enough to Miri-check in isolation with the fork/exec
stubbed if a future harness wants to. The `data` pointer's only deref is in the
forked child (`subprocess_pre_exec` → `childPreExec`), which Miri never observes
(the child is a separate process). [ADV-FIX F3] the pty.slave `File` is
`ManuallyDrop` (no double-close) and the single verbatim `posix.close(pty.slave)`
does the one close — a fd-lifetime property, native/ASan territory, not Miri.

## rustty-termio B2 (Exec threadEnter/queueWrite/ttyWrite xev wiring) — MIRI exception

B2's un-deferred `Exec::threadEnter` / `queueWrite` / `ttyWrite` /
`processExit` / `termiosTimer` / `threadExit` (`exec.rs`) + its rustty-only KAT
`threadenter_queuewrite_writes_to_pty` build on B1's spawn and additionally: create
a real **kqueue** `Loop`, register a **`EVFILT_PROC`** process watcher + a
**`Stream::queue_write`** ordered write on the pty master + a **`Timer`** (termios
poll), spawn a real **read thread**, and drive `loop.run(NoWait)` over live
`kevent64`/`read`/`write`/`fcntl` syscalls — the same "can't call foreign function"
class as the rustty-xev C3–C6 exceptions (and B1's fork/exec). So the KAT **runs
natively**, not under Miri.

The teeth (why no unsafe path is left un-verified): the KAT is the memory-safety +
correctness proof for the xev write path and the frame-pinned `ThreadData` model
(blueprint §3). It drives the actual round-trip (queueWrite → `Stream::queue_write`
→ pty master → canonical-echo/cat passthrough → parent read), so the pool-buffer
lifetimes (`write_buf_pool`/`write_req_pool` slices stable across the async write,
recycled by `ttyWrite`), the raw completion pointers the loop re-enters across ticks
(`process_wait_c`/`termios_timer_c`/`write_queue`, held raw — never a long-lived
`&mut` per `rust-mut-args-strongly-protected`), and the `io: *mut Termio<M>` handed
raw to the read thread ([ADV-FIX F2]) are all exercised natively. Under strict
Stacked Borrows the KAT is NOT clean (the frame-pinned `td` is passed to the Exec
methods as `&mut` and a raw pointer is derived from it and stored into the loop; the
full frame-local raw threading of `td` — matching the reference `td: *ThreadData` —
lands with `Thread::threadMain` in B4). That is precisely why B2's loop-driving KAT
is Miri-excluded rather than Miri-checked; on real hardware the `td`/loop memory is
frame-pinned and never moves, so the raw pointers stay valid for the bounded pump.
Since B3, the read-thread `io` pointer IS dereferenced (the real `processOutput`
read loop landed): B2's KAT now hands the read thread a real, heap-pinned
`Termio<TestMailbox>` (the `TestTermio` harness) and asserts on the terminal
`Screen` the read thread commits cat's echo into. The read thread — the master's
sole reader — is joined in `threadExit` before the frame (and the harness Termio)
are torn down.

## rustty-termio B3 (ReadThread read loop + processOutput raw-ptr [F2]) — MIRI exception

B3's un-deferred `ReadThread::threadMainPosix` (the raw `poll(2)`/`read`/
`processOutput` loop over `[pty_fd, quit_fd]`, `O_NONBLOCK`) + the [ADV-FIX F2]
raw-`*mut Termio<M>` retyping of `Termio::processOutput`/`processOutputLocked`
(`exec.rs`/`termio_impl.rs`) + its rustty-only KAT `readthread_pty_bytes_land_in_screen`
open a real pty (`openpty`), spawn a real **read thread**, and drive live
`fcntl`/`poll`/`read`/`write`/`close` syscalls plus a mach-port `Async::notify`
(`queueRender`) — the same "can't call foreign function" class as the rustty-xev
C3–C6 exceptions and B1/B2. So the KAT **runs natively**, not under Miri.

The teeth (why no unsafe path is left un-verified): the KAT is the memory-safety +
correctness proof for the read path. It builds a real, heap-pinned `Termio<TestMailbox>`
whose `StreamHandler` self-pointers (`terminal`/`size`/`termio_mailbox`) are re-seated
at the FINAL address (the [ADV-FIX F1] discipline, exercised here ahead of B4's
`Termio::init`), feeds a byte string to the pty slave, and asserts it round-trips the
real read thread (`threadMainPosix` → `Termio::processOutput` → renderer-mutex lock →
`terminal_stream.next_slice` → the terminal `Screen`) under the shared guard-less
`zigstd::thread::Mutex`. The raw `io: *mut Termio<M>` is threaded on both concurrent
paths (the read thread's `processOutput` and, in the shared harness, the parent's
Screen reads) per [ADV-FIX F2]: only transient, non-overlapping borrows are formed
under the lock (never a long-lived `&mut` per `rust-mut-args-strongly-protected`), so
no protected-`Unique` aliasing exists. The read thread is joined (quit-pipe teardown)
before the heap-pinned Termio is deinit'd — moving it out of the box (which invalidates
the self-pointers) is sound only post-join. The fork/`openpty`/`poll` syscalls are why
it is Miri-excluded rather than Miri-checked; the borrow scoping itself is
Stacked-Borrows-clean (transient borrows under the mutex, no `&mut` retag spanning the
loop).

### WS10 T20 amendment — the two-stage pipeline (Category 2)

`2f0e6659d` + `bb0ac4c72` replaced the single read loop with a **gather/parse
pipeline**: an `io-gather` thread drains the pty into a 4 x 64 KiB ring while the
`io-reader` thread parses the previous batch. The real-`poll`/`read` arms stay in
this exception for the same reason as above (Category 2: live syscalls Miri
cannot execute), joined by the three new KATs
`readthread_pipeline_{sustains_saturated_producer,keeps_interactive_latency,shuts_down_cleanly_from_both_stages}`.

**But the pipeline's core claim is NOT a syscall claim and is NOT exempted.** The
upstream comment "a buffer is owned by exactly one stage at a time, so buffer
contents need no locking" is, in Rust, an aliasing claim about ONE allocation
with two live threads inside it. That claim is proven under Miri, on an isolated
non-FFI model of the ring in **`.port/phase-2/mirihar-ws10-pipeline`** (two real
threads, the same mutex + `batch_ready`/`slot_free` condvars + `head`/`tail`/
`count`/`lens`/`done` protocol, a fake producer/consumer, zero fds):

| candidate | models | SB | TB |
|---|---|---|---|
| `shipped` | the shipped raw-pointer slot handoff (`Pipeline::slot_ptr`) | **CLEAN** | **CLEAN** |
| `naive_mutslice` | per-slot `&mut (*bufs)[i]` reborrow | clean | clean |
| `naive_copyfield` | "hold no reference, just copy the bytes out" | **UB** (data race) | **UB** |
| `naive_wholeref` | the literal transliteration: `&mut *bufs` then index | **UB** (tag not in borrow stack) | **UB** (write forbidden) |

Candidate selection is by **argv**, not an env var: Miri does not forward the
host environment, so an env switch would collapse every candidate onto the
default and make the negative controls vacuous.

Two results worth stating plainly. First, `naive_wholeref` — which is what a
careful reader of the Zig naturally writes — is UB under both models, so the raw
`slot_ptr` discipline is load-bearing, not defensive styling. Second,
`naive_mutslice` is CLEAN: a per-element `&mut` reborrow through the raw pointer
is narrow enough. The hazard is specifically the WHOLE-OBJECT reference or copy,
which matches the standing `rust-mut-args-strongly-protected` finding that a
by-value read of a `Copy` field is still a foreign access on the owning
allocation. The shipped code uses `slot_ptr` anyway, because the narrower shape
costs nothing and does not depend on that distinction holding in a future
aliasing model.

## rustty-termio B5 (live-shell KAT: real /bin/sh round-trip) — MIRI exception

B5's live-shell KAT (`crates/rustty-termio/tests/live_shell.rs`,
`live_shell_echo_round_trips_into_the_screen`, its own integration-test process,
`#![cfg(target_os = "macos")]`) is the milestone culmination: it `fork`/`exec`s a real
`/bin/sh -c 'echo RUSTTY_OK'` on a real pty and drives the WHOLE live stack —
`Termio::init` ([ADV-FIX F1] in-place out-param), `Termio::threadEnter` → `Exec::threadEnter`
(fork/exec + xev `Process`/`Stream`/`Timer` wiring + a real read-thread spawn), a bounded
`Loop::run(NoWait)` pump, and the read thread's raw-`*mut` `Termio::processOutput` — so the
shell's output round-trips into the terminal `Screen`. Real `fork`/`execve`/`openpty`/
`kevent`/mach-port syscalls place it in the same "can't call foreign function" class as the
rustty-xev C3–C6 exceptions and B1–B3: it **runs natively**, not under Miri.

The teeth (why no unsafe path is left un-verified): the KAT is the end-to-end memory-safety +
correctness proof that the port runs a real terminal. It builds a heap-pinned
`Termio<TestMailbox>` via the public `Termio::init` (self-pointers re-seated at the FINAL
address, [ADV-FIX F1]); the concurrent read thread is the sole `*mut Termio` toucher, threading
the raw pointer per [ADV-FIX F2] (only transient, non-overlapping borrows under the guard-less
`zigstd::thread::Mutex`, never a long-lived `&mut` spanning a loop tick); teardown reaps the
shell + joins the read thread (`Backend::threadExit`, field-scoped to avoid the whole-`&mut io`
retag) BEFORE the heap-pinned Termio is deinit'd. Bounded (tick cap + a 3 s wall-clock deadline
+ explicit teardown), so it never `run(.until_done)`-blocks on the child's life.

ENVIRONMENT NOTE: because `Subprocess::start` on macOS routes the command through
`/usr/bin/login -flp <user> …` (`execCommand`, matching the reference), the KAT requires a
non-sandboxed, login-capable session (real `fork`/`execve` + a reapable `login` child). B5's
self-terminating command with a real read thread DRAINING the pty reaps cleanly here; note the
pre-existing B1 unit KAT `subprocess_start_spawns_and_reaps` (which spawns a login-wrapped shell
but has NO read thread to drain the pty) can wedge `killPid`'s `killpg`/`waitpid(WNOHANG)` reap
loop in a headless/undrained context — a property of that no-drain KAT, unrelated to B5 (which
drains).

## GPU R-d Chunk 3b (CPU cell-fill raw-ptr sites) — MIRI exception, SB/TB-clean by construction

Chunk 3b's cell-fill (`crates/rustty-renderer/src/generic.rs`, `mod macos_impl`:
`Renderer::{rebuild_cells, rebuild_row, add_cursor, add_glyph, add_underline, add_overline,
add_strikethrough, add_preedit_cell}`) is written around raw pointers into
`self.terminal_state` and the shaper, faithfully mirroring the reference
(`src/renderer/generic.zig`, whose methods take `self: *Self` — a raw pointer — and bind
`const state = &self.terminal_state`). **Unlike a pure FFI exception, the borrow structure
here is Stacked-Borrows AND Tree-Borrows clean by construction — PROVEN on an isolated,
non-FFI Miri harness** (`.claude/jobs/.../mirihar`, retained under the design doc) that
reproduces the exact borrow shape of the run loop and the cursor alias; both models flag
the naive `&mut self` structure as UB and pass the shipped structure. The raw-ptr sites,
per `GPU_RD_CHUNK3B_DESIGN.md` §C.3:

- **`rebuild_cells` `state: *const RenderState` (§C.3(1), remedy 1).** All inline
  `terminal_state` scalars the post-row-loop cursor/preedit blocks read
  (`colors.{foreground,background,cursor,palette}`, `cursor.{viewport,cell}`,
  `rows/cols/dirty`, the SGR-at-cursor `Style`) are **Copy-hoisted to locals BEFORE the
  row loop**, so no inline-self alias read through `state` is live-crossed by a sub-call.
  The per-row `dirty=false` write-back is a fresh `&mut self.terminal_state` BEFORE
  `rebuild_row`.
- **`add_cursor(self_raw: *mut Renderer, cursor_state: *const render::Cursor, ...)`
  (§C.3/§D.6).** BOTH `self` and `cursor_state` are RAW, exactly as the reference
  `addCursor(self: *Self, cursor_state: *const RenderState.Cursor, ...)`. The caller's
  `cursor_ptr` aliases inline `self.terminal_state.cursor`; a `&mut self` RECEIVER would
  retag the whole `Renderer` and pop that alias BEFORE the body derefs it (SB UB — the
  harness `cursor_current` candidate reproduces this). Taken raw, receiver and
  `cursor_state` coexist as two raw pointers; the body reads `self` only via NARROW
  `(*self_raw).field`.
- **`rebuild_row` `cells`/`highlights: *const [_]` args + the shaper hook (§C.3(2,3)).**
  `cells`/`highlights` are HEAP-backed (the `Vec` backing buffers of `row_data[y]`, a
  separate allocation from the `Renderer`), so a sub-call's retag never touches them. The
  `RunIteratorHook { shaper: *mut Shaper }` aliases inline `self.font_shaper` and stays
  live across the whole run loop. A `self.font_shaper.shape(...)` SIBLING reborrow, or a
  `self.add_glyph(...)` WHOLE-`Renderer` receiver reborrow, POPS the hook tag — permanent
  UB under both SB and TB (harness `row_current`/`row_shaper_idiom`/`row_rawsibling`
  candidates all reproduce it). The shipped loop is clean because: (a) `run_iterator`
  returns an iterator holding only raw ptrs (no `self` borrow); (b) `shape` is invoked
  THROUGH the hook's own raw ptr `run_iter.hooks.shaper` — a CHILD reborrow that does not
  pop the hook — never `self.font_shaper.shape`; (c) the `add_*` helpers are RAW-self
  associated fns called via a `self_raw: *mut Renderer`, using only NARROW field access
  (never a whole `&mut *self_raw`); (d) direct `self.field` access in the loop
  (`self.cells.bg_cell`, `self.uniforms`, `self.font_shaper_cache`) is disjoint from
  `font_shaper`, retagging a different location. The `shaper_cells: Option<*const
  [shape::Cell]>` is a heap slice into the shaper `cell_buf` / cache Box: captured right
  after `shape`/`get`, consumed within one run's glyph loop, `put` COPIES (does not free)
  it, and `run_iter.next` nulls it first.

**Why Miri can't run the actual test:** the ONLY test that exercises this code
(`generic::macos_impl::tests::rebuild_cells_fills_bg_fg_cursor`) is macOS/Metal-gated —
it builds an offscreen `Metal` (`MTLCreateSystemDefaultDevice` + `newCommandQueue`),
`init_shaders`, a `SwapChain`, and a CoreText `Shaper`/`SharedGrid`. Miri cannot execute
the Metal/CoreText foreign functions (same "can't call foreign function" class as the
rustty-xev C3–C6 and rustty-termio B1–B5 exceptions). So it **runs natively**, not under
Miri. The teeth (why no unsafe path is un-verified): the borrow structure is not merely
"adversary-reviewed" — it is Miri-proven SB/TB-clean on the isolated harness that mirrors
it exactly, so the aliasing IS Miri-checked, just on the non-FFI reproduction rather than
the FFI-bearing test. Any future edit that re-reads `state` across a `&mut *self` sub-call,
routes `shape` through `self.font_shaper` instead of the hook, or reverts an `add_*` helper
to a `&mut self` receiver re-opens the hazard — re-run the harness. The native run passes
(assertions on `bg_cells` + a foreground glyph + a red block cursor at `[2,0]`), and every
crossed value (`page::Cell`/`Style`/`RGB`/`shape::Cell`/`Colors` scalars) is `Copy` POD.

> NOTE (latent, tracked): the CoreText/harfbuzz shaper crate's OWN run-loop idiom
> (`let sp: *mut Shaper = &mut ...; (*sp).run_iterator(); ... (*sp).shape(run)` — e.g.
> `harfbuzz_tests.rs`) has the SAME sibling-reborrow shape and is likewise UB under SB/TB
> (harness `row_shaper_idiom`), but is never Miri-checked (harfbuzz/CoreText FFI). It works
> in practice (the Shaper never moves). Not fixed here (out of Chunk-3b scope); flagged so a
> future shaper Miri pass routes shape through `run_iter.hooks.shaper` the same way.

## GPU R-d Chunk 4 (draw_frame + completion-trampoline raw-ptr sites) — MIRI exception, SB/TB-clean by construction

Chunk 4's offscreen draw path (`crates/rustty-renderer/src/generic.rs`, `mod macos_impl`:
`Renderer::{draw_frame, frame_completed, sync_atlas_texture}` + the free `metal_buffer_completed`
trampoline) is written around raw pointers into `self` and into `self.swap_chain.frames[i]`,
faithfully mirroring the reference (`src/renderer/generic.zig` `drawFrame`/`frameCompleted`/
`syncAtlasTexture`, whose methods take `self: *Self` and pass `*Texture` raw pointers). **Like
the Chunk-3b entry, the borrow structure here is Stacked-Borrows AND Tree-Borrows clean by
construction — PROVEN on the same isolated, non-FFI Miri harness** (`.port/phase-2/mirihar-chunk3b`,
argv candidates `drawframe_shipped` / `drawframe_naive` / `atlas_self_isolated`), which reproduces
the exact borrow shape and flags the naive structures as UB while passing the shipped one:

| candidate | models | SB | TB |
|---|---|---|---|
| `drawframe_naive` | hold `frame` (a `&mut self.swap_chain[i]`-derived raw) across a whole-`&mut self` call, then re-deref (H1) | **UB** | clean |
| `drawframe_shipped` | **shipped** structure (below) | **CLEAN** | **CLEAN** |
| `atlas_self_isolated` | the design §6.3 LITERAL `fn sync_atlas_texture(&self, …, texture: &mut Texture)` — `&self` receiver + `&mut`-into-`self.swap_chain` texture | **UB** | **UB** |

The raw-ptr sites, per GPU_RD_CHUNK4_DESIGN §5.2 (H1/H2) + the mirihar evidence:

- **`draw_frame` single raw root `self_ptr: *mut Renderer` + re-derived `frame` (H1).** All of
  `draw_frame`'s `self`-access goes through `self_ptr` with NARROW field reborrows
  (`(*self_ptr).field`). The frame raw root `frame: *mut FrameState` is re-derived FRESH via
  `addr_of_mut!((*self_ptr).swap_chain.frames[idx])` AFTER every whole-`&mut self` call
  (`upload_background_image`, `update_custom_shader_uniforms_for_frame`,
  `update_screen_size_uniforms`, `init_shaders`) — because a whole `&mut *self_ptr` reborrow
  RETAGS the entire `Renderer` allocation and POPS a held `frame` tag (the deref then UB; the
  harness `drawframe_naive` reproduces this exactly under SB). `swap_chain.frames` is a fixed
  ARRAY, so `[idx]` is a place projection (no `Index` autoref that would re-introduce a whole
  reborrow). Narrow field reborrows retag only the field's byte range, so a held `frame`
  (pointing into `swap_chain`, a disjoint range from `images`/`api`/`shaders`) survives them.
- **The completion trampoline `metal_buffer_completed` (H2).** `Frame::complete(sync=true)` (the
  golden path) commits → `waitUntilCompleted` → invokes the trampoline DIRECTLY, INSIDE
  `draw_frame`, while `draw_frame`'s `&mut self` (the raw root `self_ptr`) is still live on the
  stack. The trampoline recovers the renderer as a RAW `*mut Renderer` (`ctx.renderer as *mut
  Renderer`, the container_of / `@fieldParentPtr` re-entry) — NEVER a `&mut` held across the
  callback (`rust-mut-args-strongly-protected` / [FIX-2]) — and reborrows `&mut (*renderer).api`
  (present, narrow) + `&(*renderer)` (`frame_completed`, whole-shared). The whole-shared
  `frame_completed` reborrow is clean because `frame` is NEVER used after `complete()` (the
  reference's last frame use is `frame_ctx.complete`), so popping the frame tag is harmless.
- **`sync_atlas_texture(self_raw: *const Renderer, atlas: &FontAtlas, texture: *mut Texture)` —
  raw self + raw texture (NOT the design §6.3 `&self`+`&mut Texture`).** The `texture` is
  `&mut frame.grayscale`, which aliases INTO `self.swap_chain`; a `&self` RECEIVER freezes the
  texture bytes (SharedReadOnly) and the subsequent `*texture` write pops that frozen tag — UB
  under BOTH SB and TB (harness `atlas_self_isolated` reproduces it under both). Ported raw
  (matching the Zig `self: *const Self, texture: *Texture`), it reborrows only `self.api`
  (narrow, disjoint from `swap_chain`) and writes `*texture`. `atlas` stays a real `&FontAtlas`
  — it lives in the `*mut SharedGrid`, a SEPARATE allocation (no aliasing with `self`).

**Why Miri can't run the actual test:** the ONLY test that exercises this code
(`generic::macos_impl::tests::golden_renders_cursor_fg_bg`) is macOS/Metal-gated — it builds an
offscreen `Metal` (`MTLCreateSystemDefaultDevice` + `newCommandQueue`), `init_shaders` (metallib
load), a `SwapChain`, a CoreText `Shaper`/`SharedGrid`, and then ENCODES + SUBMITS a real GPU
frame (`commit`/`waitUntilCompleted`) and reads the IOSurface back. Miri cannot execute the
Metal/CoreText/IOSurface foreign functions (same "can't call foreign function" class as the
rustty-xev C3–C6 / rustty-termio B1–B5 / Chunk-3b exceptions). So it **runs natively**, not under
Miri. The teeth: the borrow structure is Miri-proven SB/TB-clean on the isolated harness that
mirrors it exactly (the aliasing IS Miri-checked, on the non-FFI reproduction), and the native
golden run ASSERTS real GPU output (cursor red at grid (2,0); opaque dark bg; a fg glyph in cell
(0,0)) — a wrong `is_cursor_glyph` bit or a popped `frame`/`self` tag would surface as a visible
pixel/behavior failure. Any future edit that holds a `frame` raw root across a whole-`&mut self`
call, reverts `sync_atlas_texture` to a `&self`+`&mut Texture` signature, or holds a `&mut
Renderer` across the completion callback re-opens the hazard — re-run the harness.

## macOS windowing W0 (cf_release live loop + IOSurfaceLayer) — MIRI exceptions

- **`crates/rustty-os/tests/cf_release_live.rs`** (`cf_release_thread_releases_on_real_xev_loop`).
  Drives the real `rustty-xev` kqueue `Loop`/`Async` (mach-port alloc/`kevent64`/`mach_msg`
  syscalls) plus a `std::thread` notifier and CoreFoundation `CFString`/`CFGetRetainCount`/
  `CFRelease`. Miri cannot execute the mach/kqueue/CF foreign functions (same class as the
  rustty-xev C3–C6 exceptions). Runs natively; ASan-eligible (R6). The teeth: the retain-count
  asserts the object went 2→1, proving `drainMailbox`→`CFRelease` fired on the live loop (the old
  `unimplemented!()` placeholder would have panicked).
- **`crates/rustty-renderer/src/metal/iosurface_layer.rs` tests** (`iosurface_layer_sets_surface_as_contents`,
  `iosurface_layer_display_callback_fires`). Register a real ObjC `CALayer` subclass (objc2
  `define_class!`), mint an IOSurface-backed offscreen `Metal` `Target` (`MTLCreateSystemDefaultDevice`
  + IOSurface create), and send `-display`/`setContents:` through the ObjC runtime. Miri cannot
  execute the objc runtime + Metal/IOSurface foreign functions. Runs natively; ASan-eligible (R6).

## macOS windowing W1-7 (Renderer windowed lifecycle: init/deinit) — MIRI exceptions

- **`crates/rustty-renderer/src/generic.rs`**
  (`generic::macos_impl::tests::windowed_lifecycle::windowed_renderer_init_then_deinit_round_trips`).
  Builds an offscreen `Metal` (`MTLCreateSystemDefaultDevice` + `newCommandQueue`), `init_shaders`
  (metallib load), a `SwapChain`, a CoreText `Shaper`/`SharedGrid`, mints a real `NSObject` through
  the ObjC runtime (the `*mut S` for the generic surface hooks), runs `Renderer::surface_init` /
  `finalize_surface_init` (faithful no-ops for Metal), `Renderer::init`, then `Renderer::deinit`
  (the full verbatim teardown). Miri cannot execute the objc runtime + Metal/CoreText/IOSurface
  foreign functions (same class as the golden-render and iosurface_layer exceptions). Runs
  natively; ASan-eligible (R6) — ASan covers any double-free/leak across the objc/Metal FFI in the
  init→deinit round-trip.

## macOS windowing W1-6 (FIRST PIXELS: on-screen present of a golden frame) — MIRI exceptions

- **`crates/rustty-renderer/src/generic.rs`**
  (`generic::macos_impl::tests::golden_presents_frame_on_screen`). The culminating W1 KAT. Renders
  one real terminal frame OFFSCREEN via the golden path (`MTLCreateSystemDefaultDevice` +
  `newCommandQueue` + `init_shaders` metallib load + CoreText `Shaper`/`SharedGrid` + a real GPU
  `commit`/`waitUntilCompleted` + IOSurface readback), then constructs a plain-`NSObject` layer-host
  double (`RusttyMockHostView`, objc2 `define_class!` — NO AppKit, so no main-thread constraint and
  no `class!(NSView)` panic under the default parallel `cargo test`), runs the on-screen
  `Metal::init(view)` (creates an `IOSurfaceLayer`, sends the `setLayer:`/`setWantsLayer:`/
  `setClipsToBounds:` layer-host selectors to the double), `present`s the golden Target
  (`set_surface_sync` → `CALayer.setContents(surface)`), and asserts the layer's `contents` IS the
  exact IOSurface (FIRST PIXELS) + the view hosts our layer. Miri cannot execute the objc runtime +
  Metal / CoreText / IOSurface foreign functions (same class as the golden-render, iosurface_layer,
  and windowed_lifecycle exceptions). Runs natively; ASan-eligible (R6). Skips gracefully on a
  headless host (no Metal device).

## macOS windowing W2 (CVDisplayLink → xev::Async → loop vsync round-trip) — MIRI exception

- **`crates/rustty-renderer/src/metal/cvdisplaylink.rs`**
  (`metal::cvdisplaylink::tests::kat_vsync_displaylink_notifies_async`). The W2-T3 vsync KAT. Binds
  a real CoreVideo `CVDisplayLink` to the active display(s) (`CVDisplayLinkCreateWithActiveCGDisplays`
  + `SetOutputCallback` + `Start`), whose `output_callback` bumps a `#[cfg(test)] TICKS` counter then
  `notify()`s a real `xev::Async` (mach port), and drains one `xev::Loop` wakeup — asserting `TICKS >= 1`
  (a genuine display-link tick, distinct from the 2 s guard thread's fallback `notify()` which bypasses
  the callback). Miri cannot execute the CoreVideo foreign functions or the mach-port syscalls (same
  class as the golden-render / iosurface_layer / windowed_lifecycle exceptions). Runs natively (verified
  on-device, TICKS>=1 in 0.14 s); ASan-eligible (R6) — ASan covers the callback→`*const Async`
  aliasing shape (separate `Box` allocation, `Copy`-by-value read) across the CoreVideo FFI. `#[ignore]`
  by default (on-device: requires a live WindowServer display) AND internally 2 s-bounded so a manual
  un-ignore on a headless host FAILS the assert rather than hanging.

## macOS windowing W3 (renderer-Thread construct-then-spawn event loop) — MIRI exception, SB/TB-clean by construction

W3 (E2a/E2b) fills the core-resident `crate::renderer_seam::Thread<A>` event loop
(`crates/rustty-core/src/renderer_seam.rs`: `threadMain`/`threadMain_` + `wakeupCallback`/
`drawNowCallback`/`drawCallback`/`renderCallback`/`cursorTimerCallback`/`stopCallback`/
`drainMailbox`/`syncDrawTimer` + the `drawFrame` helper), faithfully mirroring the reference
(`src/renderer/Thread.zig`, whose `threadMain(self: *Thread)` + callbacks take a raw `self_: ?*Thread`
handed back by the xev loop as `userdata`). After the construct-then-spawn (`surface.rs`
`RendererThreadArg(*mut Thread<A>)`), the child thread holds `*mut Thread<A>` for its whole life and
the xev loop re-hands that SAME raw pointer into every callback; the main thread, post-spawn, touches
ONLY the Copy `wakeup`/`draw_now` `Async` handles + the `*const` mailbox + a narrow `&renderer_thread.stop`
(§2.2 rule 4, `rust-mut-args-strongly-protected`). **Like the Chunk-3b/Chunk-4 entries, the borrow
structure here is Stacked-Borrows AND Tree-Borrows clean by construction — PROVEN on the same isolated,
non-FFI Miri harness** (`.port/phase-2/mirihar-chunk3b`, argv candidates `renderthread_shipped` /
`renderthread_naive`), which reproduces the construct-then-spawn + xev-loop callback-reentry shape
(xev stubbed as a channel-driven tick sequence firing the stored callback with the raw userdata) and
flags the naive structure as UB while passing the shipped one:

| candidate | models | SB | TB |
|---|---|---|---|
| `renderthread_naive` | child `thread_main` binds a whole `&mut *tp` held across the loop; a callback reborrows `&mut *tp` (sibling Unique retag) → pops `t`; the next `t.field` use is use-after-pop (main producer also reborrows `&mut *tp` whole) | **UB** | **UB** |
| `renderthread_shipped` | **shipped**: `thread_main` + every callback use raw `tp` with NARROW per-field access only (`(*self_).renderer` draw_frame, `(*self_).mailbox` drain, `(*self_).field`/`.cursor_blink_visible` writes, `(*self_).wakeup.notify()`); main uses a Copy `(*tp).wakeup` read + `*const (*tp).mailbox` push + a narrow `&(*tp).stop` notify | **CLEAN** | **CLEAN** |

The raw-ptr sites, per W3_DESIGN §2.2 (the aliasing discipline) + the mirihar evidence:

- **`threadMain(self_: *mut Thread<A>)` — raw self, NO whole reborrow (§2.2 rule 1).** `threadMain_`
  and every callback take raw `self_` and project only NARROW per-field accesses `(*self_).<field>` /
  `&(*self_).<field>` / `&mut (*self_).<field>` — never a whole `&mut *self_`/`&*self_`. A whole
  `&mut *self_` reborrow (the naive shape) retags the entire `Thread<A>` allocation as a fresh Unique;
  any subsequent callback that re-derefs `self_` (or reborrows `&mut *self_` again) pops the earlier
  tag, and the next field use is use-after-pop (the harness `renderthread_naive` reproduces this under
  BOTH SB and TB). Narrow field reborrows retag only the field's byte range, so disjoint fields
  (`renderer`/`mailbox`/`field`/`wakeup`) survive each other's use across the loop.
- **xev registration args stay raw (§2.2 rule 3).** `wakeup.wait(&raw mut (*self_).loop_, &raw mut
  (*self_).wakeup_c, self_, wakeupCallback)` — `loop_`/`c` are raw (the loop re-enters `c` across ticks;
  a `&mut` re-read would be popped), `userdata = self_` (the same raw pointer re-handed to the callback).
- **Main thread post-spawn: Copy handles + narrow shared only (§2.2 rule 4).** After spawn the main
  thread NEVER takes `&mut renderer_thread`. It reads the Copy `wakeup`/`draw_now` `Async` handles
  captured pre-spawn (`Async::notify(&self)` reads only the mach-port handle), pushes to the `*const`
  mailbox, and at deinit takes a narrow `&renderer_thread.stop` for `stop.notify()` — all compatible
  with the child's raw-self narrow use (the `callback_shipped` wakeup-notify shape, independently proven).

**Why Miri can't run the actual code path:** the live `Surface::draw → threadMain` loop needs a
live-Metal offscreen renderer + an apprt windowing host (the windowing-integration milestone), and the
xev loop itself makes real `kqueue`/`kevent64`/mach-port syscalls — the same "can't call foreign
function" class as the rustty-xev C3–C6 / rustty-termio B1–B5 / Chunk-3b/4 exceptions. So there is no
unit test that drives the live loop under Miri. The teeth (why no unsafe path is un-verified): the
aliasing structure is Miri-proven SB/TB-clean on the isolated harness that mirrors the construct-then-
spawn + callback-reentry shape exactly (the aliasing IS Miri-checked, on the non-FFI reproduction), and
`cargo build --workspace` type-checks the real `threadMain` + every callback against the xev/renderer
signatures. Any future edit that binds a whole `&mut *self_` in `threadMain`/a callback, or takes a
whole `&mut renderer_thread` on the main thread post-spawn, re-opens the hazard — re-run the harness
(`renderthread_shipped` must stay green under both SB and TB, `renderthread_naive` UB, BEFORE the core
fill ships).

## macOS windowing W4 (apprt-c host handshake: blur + display_id + NSWindow KAT) — MIRI exception

W4 realizes the two remaining macOS `#[cfg(target_os="macos")]` C-ABI leaves in
`crates/rustty-apprt-c/src/lib.rs` and lands the culminating integration harness
`crates/rustty-apprt-c/tests/macos_window_host.rs`. All of it is foreign-call /
live-render-loop territory — Miri cannot run it (the "can't call foreign function"
class shared with the rustty-xev C3–C6, rustty-termio B1–B5, GPU Chunk-3b/4, and W0–W3
entries). ASan-eligible on the FFI gate.

- **`rustty_set_window_background_blur` (W4-2).** `window` is a foreign NSWindow id
  (or, in the headless KAT, a plain-`NSObject` `MockHostView` double answering
  `windowNumber`) — it is NEVER turned into a Rust `&`/`&mut`; only `objc2::msg_send!`
  (`windowNumber`) and the private CGS SPI (`CGSSetWindowBackgroundBlurRadius`
  /`CGSDefaultConnectionForThread`, resolved via `#[link(name="CoreGraphics",
  kind="framework")]`) consume it ⇒ no aliasing obligation. `app` is read through
  `&*(*app).config.get()` (an `UnsafeCell<Config>` shared read; the C host is
  single-threaded). Foreign objc/CGS calls ⇒ Miri-excluded.
- **`rustty_surface_set_display_id` (W4-3).** Pushes `rustty_renderer::Message::
  MacosDisplayId` through `&*surface.renderer_thread.mailbox` (a `*mut Mailbox` Box
  pointee OUTSIDE the Surface struct's Stacked-Borrows coverage) and wakes the loop
  via the Copy `wakeup` `Async` handle — the SAME construct-then-spawn raw-alias
  discipline PROVEN clean for the core render `Thread<A>` (see the W3 entry:
  `.port/phase-2/mirihar-chunk3b`, `renderthread_shipped`=CLEAN under SB+TB). No new
  aliasing shape is introduced (`&*mailbox` shared push + `&wakeup` narrow notify),
  so W3's mirihar evidence covers it; the leaf itself crosses into the live xev
  render loop + foreign wakeup ⇒ Miri-excluded.
- **The `macos_window_host.rs` harness.** Real objc2 (`define_class!` MockHostView,
  `msg_send!`), Metal, the private CGS SPI, `objc2_core_graphics::CGMainDisplayID`,
  `objc2_io_surface` IOSurface lock/read, and a live surface render thread — all
  foreign ⇒ Miri-excluded, ASan-eligible.

## macOS windowing W5 (Metal::loop_enter display-callback trampoline) — MIRI exception, SB/TB-clean by construction

W5 fills the LAST live-windowing seam: `renderer::metal::metal_impl::Metal::loop_enter`
(the `unimplemented!("windowing-host")` that aborted any live surface's render thread),
installing the `IOSurfaceLayer` display callback so Core Animation can drive a frame,
mirroring `ghostty/src/renderer/Metal.zig:164-176` (`loopEnter`/`displayCallback`). The
CA `display` method (main thread, `needsDisplayOnBoundsChange` commit) recovers `*mut
Renderer` via container_of (`@fieldParentPtr` → `offset_of!` `byte_sub`) and calls the
RAW-self draw path (`draw_frame_raw`), CONCURRENTLY with the render thread's own
`drawFrame` — both serialised by `draw_mutex`. **Like the Chunk-3b/4 and W3 entries, the
borrow structure is Stacked-Borrows AND Tree-Borrows clean by construction — PROVEN on
the same isolated, non-FFI Miri harness** (`.port/phase-2/mirihar-chunk3b`, argv
candidates `displaycb_shipped` / `displaycb_naive`), which reproduces the two-thread
concurrent-draw + container_of shape and flags the pre-fix structure as UB while passing
the shipped one:

| candidate | models | SB | TB |
|---|---|---|---|
| `displaycb_naive` | PRE-FIX: the render thread draws through the `&mut self` `draw_frame` WRAPPER (a protected whole-`Renderer` `&mut` fn arg, `renderer_seam.rs:1208` pre-fix) CONCURRENTLY with the raw display callback's `draw_frame_raw` (mutex lock + plain-field write) | **UB** | **UB** |
| `displaycb_shipped` | **shipped**: BOTH concurrent callers raw-self — render thread via `&raw mut (*(*owner).renderer).0` (raw place projection, no protector; `renderer_seam.rs:1208` post-fix), display callback via the container_of round-trip (`&raw mut (*rp).api` → `byte_sub(offset_of!)` → `*mut c_void` in a `Cell` → back → deref ALL of Renderer, §2.4); `draw_frame_raw` locks `draw_mutex`, narrow field write + nested completion re-entry, unlocks | **CLEAN** | **CLEAN** |

The raw-ptr sites, per W5_DESIGN §2 + the mirihar evidence:

- **`draw_frame_raw(self_ptr: *mut Renderer)` — raw-self, faithful to Zig `drawFrame(self:
  *Renderer)` (§2.3).** The `&mut self` `draw_frame` wrapper (a protected whole-`Renderer`
  `&mut` function argument) is restricted to the single-threaded offscreen unit tests; it
  is unreachable from any concurrent path. BOTH concurrent production draw callers reach
  `draw_frame_raw` through a raw `*mut Renderer` — the render thread's `&raw mut
  (*(*self_).renderer).0` (a raw place projection off the already-raw `renderer` handle,
  no `&mut` retag) and the main-thread display callback's recovered ctx. No whole-
  `Renderer` `&mut` protector ever spans either call, so the `draw_mutex` (locked inside
  the body) legally serialises the field accesses. The naive shape's UB fires at the
  display callback's very first foreign touch (the `draw_mutex.lock()` — a foreign access
  that would remove/Disable the render thread's strongly-protected `Unique`), proving the
  mutex **cannot** rescue a protector established before the lock (`displaycb_naive` UB
  under BOTH SB and TB) — the raw-self split of both callers is load-bearing, not cosmetic.
- **The container_of recovery `Metal::loop_enter` (§2.4).** `@fieldParentPtr("api", self)`
  → `(api_ptr as *mut u8).byte_sub(offset_of!(Renderer, api)).cast::<Renderer>()`. The leaf
  is handed a raw `*mut Metal` derived from the WHOLE-`Renderer` `self_ptr` (not a narrowing
  `&mut self.api` reborrow), so the `+offset`/`−offset` round-trip keeps one unbroken
  allocation provenance and the recovered `*mut Renderer` legally derefs ALL of Renderer.
  The ctx is stored/loaded as a POINTER in `Cell<*mut c_void>` (no int round-trip →
  provenance preserved). `displaycb_shipped` exercises exactly this round-trip and is CLEAN.

**Why Miri can't run the actual code path:** the live `display → display_callback →
draw_frame_raw` path (and the concurrent render thread) needs a live-Metal offscreen
renderer + the ObjC `IOSurfaceLayer` `display` method + a real render thread — the same
"can't call foreign function" class as the W0–W4 / Chunk-3b/4 exceptions. The new W5
headless KAT (`loop_enter_display_callback_draws_a_frame`) drives the trampoline's
*functional* leg natively (container_of → `set_display_callback` → `display` →
`display_callback` → `draw_frame_raw` → a full frame) but is single-threaded and
observes ZERO cross-thread aliasing. The concurrency soundness — the whole point of the
raw-self re-route of both draw callers — is covered SOLELY by the `displaycb_shipped`
mirihar candidate (with `displaycb_naive` as the UB control). Any future edit that
routes a concurrent draw caller back through the `&mut self` `draw_frame` wrapper, or
narrows the container_of source to `&mut self.api`, re-opens the hazard — re-run the
harness (`displaycb_shipped` must stay green under both SB and TB, `displaycb_naive` UB,
BEFORE the seam ships).

**Deviation recorded (KAT runtime blocker) — RESOLVED-then-REVISED 2026-07-10
(VALIDATION_REPORT M4).** The original blocker (`Metal::loop_enter` still
`unimplemented!()`) is GONE — the leaf landed in W5. The instructed un-ignore was
executed and the two live-render-thread KATs were RUN on a Metal host (this
machine, 2026-07-10):
- `set_display_id_pushes_to_render_thread_panic_free` PASSES — **un-ignored**
  (live).
- `hosts_view_layer_and_presents_live_frame` FAILS at its first assertion,
  `host view is not layer-hosting` (macos_window_host.rs:329): the on-screen
  host-attach never happens because `generic::Renderer::init` constructs Metal
  via `Metal::init_offscreen` (the W1-7 apprt-free divergence) and the
  renderer_seam wiring that was to supply `Metal::init(view)` drops
  `rt_surface` — the still-open "live-windowing `Renderer::init`" seam. It
  REMAINS `#[ignore]` with that honest reason (a deterministic failure, not a
  Miri/FFI constraint) and is a triaged finding: the windowing-host wiring
  chunk must wire the attach and un-ignore it.
- ALSO FOUND + HANDLED (lldb-diagnosed): two tests calling `rustty_app_new` on
  parallel libtest workers SIGABRT with zero diagnostics inside HIToolbox —
  `App::init` -> `Keymap::init` -> `TISCopyCurrentKeyboardLayoutInputSource` ->
  `islGetInputSourceListWithAdditions` -> `abort()` (TIS is not thread-safe;
  the mirrored C host calls `ghostty_app_new` from the main thread only). App
  creation in the test binary is serialized via a test-local
  `app_new_serialized` mutex (the `parallel-cargo-test-exposes-c-init-races`
  precedent; test-harness glue, not a port change). NOTE for the "TIS ...
  thread-affinity verified" attestation (VALIDATION_REPORT §3c): that covered
  single-App hosts; CONCURRENT `TISCopyCurrentKeyboardLayoutInputSource` calls
  abort.
The W4-2 blur sub-KAT does NOT spawn a surface, so it runs GREEN headless and proves
the realized blur leaf (real `windowNumber` msg_send + the private CGS SPI, incl. the
`#[link] CoreGraphics` symbol resolution) end-to-end.

## rustty-vaxis / rustty-zigimg — WS12 image tier + UCD provenance

Three narrow additions from WS12, all of the already-established "Miri cannot do this
particular thing" class. **Everything else added by WS12 stays Miri-primary**, including
every `Image::draw` / `Image::cell_size` KAT (pure `Window` geometry, zero FFI, zero
file I/O) and the whole pre-existing 97-test `rustty-vaxis` lib suite.

- **`crates/rustty-vaxis/tests/ucd_provenance.rs::ucd_corpus_matches_recorded_digests`
  — Miri EXCEPTED (file I/O).** It `std::fs::read`s the six vendored UCD 16.0.0 inputs to
  check them against the SHA-256 digests in `ucd/PROVENANCE.md`; Miri's isolation forbids
  file I/O. `#[cfg_attr(miri, ignore)]`. Note the *deliberate contrast* with the UAX #29
  conformance corpus in `src/uucode/grapheme.rs`, which is `include_str!`-embedded at
  compile time precisely so it DOES run under Miri isolation — the provenance test is the
  one thing that inherently needs the bytes on disk. The sibling
  `sha256_matches_nist_vectors` test is pure computation and stays Miri-live.

- **`crates/rustty-zigimg/tests/codec.rs` — Miri EXCEPTED (wuffs + zlib FFI).**
  `rustty-zigimg` re-homes the six-operation zigimg surface onto ghostty's own vendored
  codecs (WS12 Decision 2, Option Z-C): decode calls into `rustty-wuffs`
  (`wuffs_png__decoder` / `wuffs_jpeg__decoder`) and PNG encode calls into `zlib-sys`
  (`deflateInit2_`/`deflate`/`deflateEnd`). Miri fails only when a foreign function is
  actually *called*, so each decode/encode test carries `#[cfg_attr(miri, ignore)]`.
  **Precedent:** `rustty-vt` links `zlib-sys` and still runs a Miri lane
  (`.port/phase-2/mirihar-pagelist-compress`). The crate's non-FFI logic — the magic-byte
  format sniff, the rgba32↔rgb24 converter, the PNG chunk framing and the CRC-32 table —
  is covered by tests that stay Miri-live, so the excepted set is exactly "the tests that
  cross into C", not "the tests that cover the crate".

- **`crates/rustty-vaxis/tests/image_tier.rs` PNG arms — Miri EXCEPTED (transitive FFI).**
  The `transmit_image(..., Png)` and `load_image` KATs reach the encoder/decoder above.
  The RGB/RGBA transmit arms and the `Image::draw`/`cell_size` geometry KATs do not touch
  C and stay Miri-live, as does the `NoGraphicsCapability` negative control.

## rustty-vaxis Chunk 3b — the `tty.rs` termios/ioctl/sigaction FFI

Chunk 3b (`crates/rustty-vaxis/src/{queue,tty,loop_}.rs`) adds the IMPURE half of the
vaxis backend. Its memory-safety coverage splits cleanly by module, and only the tty
FFI is excepted from Miri:

- **`queue.rs` — Miri PRIMARY (no exception).** The thread-safe fixed-size blocking
  two-mirror ring is pure `std::sync::{Mutex,Condvar}` with a `[Option<T>; N]` buf
  (zero `unsafe`, §2.2). All **7 ported tests** (incl. the two spurious-wake `while`-loop
  pins and the multi-reader/writer `thread::scope` cases) run GREEN under
  `cargo +nightly miri test -p rustty-vaxis` (default Stacked Borrows) — the ring
  wraparound + the Condvar blocking handoff are the fidelity contract and are Miri-clean.

- **`tty.rs` — Miri EXCEPTED (ASan/native).** `PosixTty`/`TestTty` are the crate's ONLY
  FFI surface: `libc::{open,read,write,tcgetattr,tcsetattr,ioctl(TIOCGWINSZ),sigaction,
  sigemptyset,close,pipe}`. Miri cannot execute these foreign functions (the same
  "can't call foreign function" class as the PageList/GPU exceptions above), and
  `/dev/tty` is absent in the sandbox. There are **0 Zig `test{}`** in `tty.zig` to port;
  any device smoke test (`make_raw` round-trip, `get_winsize`) needs a real tty and is
  therefore `#[ignore]`-on-device. The R6 gate here is ASan/native. `TestTty` (pipe-
  backed via `libc::pipe`, which the sandbox *does* allow) covers the device-free path.
  PORT-NOTE: the SIGWINCH handler locks a `Mutex` + would `ioctl`/push a queue — NOT
  async-signal-safe, but Zig's exact design; faithful port keeps it (ASan/native, never
  Miri).

- **`loop_.rs` — the read-thread ALIASING → mirihar (`loopthread_*` + `stopread_*`).**
  The construct-then-spawn read thread (raw-self `*mut Loop<T>`, `should_quit:
  AtomicBool`, `thread: Cell`, the type-erased SIGWINCH winsize callback, the OWNED
  queued grapheme text via `Key::into_owned`) cannot be Miri-checked directly (it needs
  a live tty + the SIGWINCH handler). Its soundness is proven by TWO mirihar-chunk3b
  candidate sets (see that crate's README): the `loopthread_naive`(UB)/
  `loopthread_shipped`(CLEAN, SB **and** TB) pair — the shipped narrow-projection split
  is CLEAN under both borrow models; the naive whole-`&mut *self_ptr` is UB under both
  (its whole-struct retag races main's legitimate narrow `thread.set`) — and the
  `stopread_*` quartet (VALIDATION_REPORT H1 / plan T11), which pins the overlap
  loopthread does NOT model: `stop()`'s DSR wake executing while the reader is BLOCKED
  inside `Tty::read`. `stopread_naive` (the pre-fix `(*tty).writer()` whole-`&mut Tty`)
  and `stopread_projonly` (writer projected but `read(&self)` kept) are UB under SB+TB;
  `stopread_dsrref` (protected `&Vaxis` DSR vs raw caps write) is UB under SB+TB;
  `stopread_shipped` (raw-receiver `Tty::read(this: *const Self)` + `Tty::writer_raw`
  narrow projection + self-less `Vaxis::device_status_report`, the shape shipped in
  tty.rs/loop_.rs/vaxis.rs) is CLEAN under SB **and** TB. The one ported `test Loop`
  runs un-`#[ignore]`d (`loop_lifecycle`, TestTty pipe-backed) including under
  `cargo +nightly miri test -p rustty-vaxis`.

**Why the split leaves zero unsafe path unverified:** queue's ring/Condvar (the only
non-FFI concurrency) is Miri-primary; the loop's raw-self aliasing — including the
`stop()`-vs-blocked-`read` overlap on the interactive exit path — is mirihar-proven
under SB+TB (`loopthread_*` + `stopread_*`); the tty's `unsafe` is `libc` FFI calls
(ASan/native) plus the raw-receiver `read`/`writer_raw` narrow projections whose
aliasing side is the `stopread_shipped` model. Any future edit that routes a concurrent
`Loop`/`Tty`/`Vaxis` access through a whole `&mut Loop`, a whole `&mut Tty` while the
reader is live, or a `&Vaxis` held across the DSR wake re-opens the hazard — re-run
`mirihar-chunk3b` (`loopthread_shipped` + `stopread_shipped` must stay green under both
SB and TB, their naive counterparts UB, BEFORE the change ships).

---

## Live-syscall test binaries (G6 lane routing — Miri → ASan)

`check_leaks.sh` (G6) runs claimed-done pure-Rust crates under Miri. A crate may
carry an integration-test binary whose entire PURPOSE is to drive a **real**
kernel object (kqueue, mach ports) — Miri does not merely time out on such a
test, it **cannot execute it at all** (hard error: "unsupported operation:
can't call foreign function `kqueue` on OS `macos`"). For these binaries the
G6 Miri lane runs the crate's unit surface (`cargo +nightly miri test -p <crate>
--lib`) and the listed binary is ROUTED to the ASan lane (`-Zsanitizer=address`
+ native host target). Nothing is skipped (R5): the binary still runs under
plain `cargo test` AND under ASan on every G6 run — only the tool changes,
per R6 (right tool per job). The routing table is `live_ffi_tests()` in
`.port/tools/gates/check_leaks.sh`; every row there requires a 4-criterion
entry here.

### `rustty-os` — `--test cf_release_live` (`cf_release_thread_releases_on_real_xev_loop`)

The W0 live-loop KAT: the CFReleaseThread runs its REAL rustty-xev kqueue loop;
a `Release` message drains and CFReleases a CFString across a genuine
mach-port wakeup (`crates/rustty-os/tests/cf_release_live.rs`).

1. **Empirically intractable — stronger: structurally impossible.** Miri
   hard-errors, it does not run slow: "error: unsupported operation: can't
   call foreign function `kqueue` on OS `macos`" at
   `crates/rustty-xev/src/kqueue.rs:325` via `rustty_os::cf_release_thread::
   Thread::init` (live G6 RED evidence: ci-run-2026-07-10.log:2160). No Miri
   budget can ever admit this test.
2. **Passes natively** — `cargo test -p rustty-os --test cf_release_live`:
   `test result: ok. 1 passed; 0 failed` (2026-07-10, this change).
3. **No uncovered unsafe path.** The test's unsafe surface is the FFI boundary
   itself — kqueue/mach-port syscalls (rustty-xev) and `CFRelease`
   (CoreFoundation) — for which ASan IS the designated R6 tool (Miri cannot
   execute foreign code, so ASan coverage here is not a downgrade but the only
   sound verifier). The pure-Rust interior the test exercises is Miri-verified
   elsewhere: the mailbox is `rustty_datastruct::blocking_queue::BlockingQueue`
   (Miri lane of the done crate `rustty-datastruct`), and the rest of
   `rustty-os`'s unit surface stays in the Miri lane via `--lib` (40 passed,
   0 failed; the 18 ignores are the crate's pre-existing
   `#[cfg_attr(miri, ignore)]` real-OS-state unit tests, each individually
   annotated at source). The routed binary itself is ASan-clean:
   `test result: ok. 1 passed; 0 failed` under `-Zsanitizer=address`
   (2026-07-10).
4. **The blocker is the foreign syscall, not logic.** The test exists precisely
   to prove the REAL kqueue/mach-port loop drains the mailbox (its header says
   so: "Real mach-port/kqueue syscalls -> Miri-excluded"). A Miri-compatible
   variant would have to fake the loop and would therefore verify nothing this
   test was written to verify — the logic-level equivalents already live in the
   Miri-covered unit surface.

**Standing rule:** adding a binary to `live_ffi_tests()` without a matching
section here is a gate violation — `check_leaks.sh`'s comment points at this
file, and review must reject unmatched additions.

## rustty-core `renderer_config_tests` (T1/T2 real-Config seam tests) — in-lib `cfg_attr(miri, ignore)`

`surface.rs` `renderer_config_tests::{renderer_derived_config_from_real_config,
init_change_config_from_real_config}` — the renderer-config accessor-trait seam
tests that deliberately build a REAL `Config` (they even carry the
`oni::testing::ensure_init()` OnceLock C-init guard).

1. **Structurally impossible under Miri.** Building the default `Config`
   compiles the default link regex through the vendored oniguruma C FFI; Miri
   hard-errors: "unsupported operation: extern static `OnigEncodingUTF8` is not
   supported by Miri" (phase-1/2 close run, 2026-07-10; surfaced the moment the
   earlier `app.rs` seam-test SB fix let the rustty-core Miri lane run past
   `add_and_delete_surface_drive_the_quit_timer`). No Miri budget admits an
   extern static of a non-Rust library.
2. **Passes natively** — both tests run green in `cargo test --workspace`
   (CI HARD step 4) on every run.
3. **No uncovered unsafe path.** The C surface they reach is oniguruma, whose
   designated R6 verifier is the ASan lane (`rustty-oniguruma` and
   `rustty-config` are both `FFI_CRATES` rows in `check_leaks.sh`). The
   pure-Rust logic they exercise (accessor trait plumbing, `DerivedConfig`
   clamps/casts, `Message::ChangeConfig` box building) has no unsafe of its
   own; rustty-core's remaining unit surface (20 tests incl. every app-seam
   raw-pointer test) stays in the Miri lane.
4. **The blocker is the foreign static, not logic.** A Miri-compatible variant
   would need a fake `Config` without compiled links and would no longer be
   the "real `Config` end-to-end" contract these tests exist to lock.

Both tests are individually annotated at source with this reason
(`crates/rustty-core/src/surface.rs`, `#[cfg_attr(miri, ignore = ...)]`), the
same convention as the 58 pre-existing real-OS-state in-lib ignores.

## rustty-core `clipboard_codepoint_map_e2e` (S151-F1 production-path test) — in-lib `cfg_attr(miri, ignore)`

`surface.rs` `clipboard_codepoint_map_e2e::clipboard_codepoint_map_reaches_
copy_selection_to_clipboards` — the S151-F1 closure's production-path proof
that `Surface::copySelectionToClipboards` actually applies the
`clipboard-codepoint-map` config option end to end. It builds a real
`Config` via `Config::default_config()` (which always compiles a default URL
link), then a real `DerivedConfig::init(...)` off it — same shape as
`renderer_config_tests` above.

1. **Structurally impossible under Miri, same root cause as
   `renderer_config_tests`.** `DerivedConfig::init` -> `Link::oni_regex` ->
   `Encoding::utf8` reaches the same non-thread-safe oniguruma `extern static
   OnigEncodingUTF8` Miri cannot model (2026-09-08 close run).
2. **Passes natively** — `cargo test -p rustty-core clipboard_codepoint_map`:
   `test result: ok. 2 passed; 0 failed` (this change).
3. **No uncovered unsafe path — proved, not asserted.** Unlike
   `renderer_config_tests`, this test's very purpose is a hand-fabricated
   `MaybeUninit<Surface<TestApp>>` with field-projected raw-pointer writes
   (`io.terminal`/`config`/`rt_surface`) plus a manual teardown ordering —
   exactly the kind of `unsafe` this project's Miri lane exists to catch, and
   which an earlier draft of this test in fact got wrong twice (a
   `MutexGuard`-outlives-the-freed-`Box` use-after-free found by SIGSEGV
   bisection, and a Stacked-Borrows read of `surface.rt_surface` after a
   write through the parent pointer `sp` invalidated its tag — both fixed,
   see the S151-F1 review). So this exception carries a SECOND, Miri-clean
   twin test, `clipboard_codepoint_map_fixture_is_miri_clean`, which runs the
   IDENTICAL fixture and assertions on a `Config` with `link.links` cleared
   (the only field `DerivedConfig::init` needs oniguruma for) — proving the
   fixture's raw-pointer/Stacked-Borrows/leak soundness under Miri without
   touching the oniguruma FFI at all:
   `.port/tools/miri-run.sh rustty-core clipboard_codepoint_map_fixture_is_miri_clean 1 15 10`
   -> `VERDICT: ALL-1-MIRI-CLEAN` (0 UB, 0 leaks; 2026-09-08). The two tests
   share a `fn run(config)` body so they cannot drift apart.
4. **The blocker is the foreign static, not logic.** A Miri-compatible
   variant of the *ignored* test specifically would need a `Config` without
   compiled links and would no longer prove the real
   `Config::default_config()` path `Surface::init` actually uses — which is
   exactly why the Miri-clean twin exists as a SEPARATE test rather than a
   replacement.

## rustty-vt — full-sweep Miri exceptions (2026-07-11)

The first exhaustive `rustty-vt` Miri sweep (all 2037 lib tests; see
`.port/phase-2/VT_MIRI_SWEEP.md`) routed the following via `#[cfg_attr(miri, ignore)]`.
Each was measured (parallel discovery + isolation re-verify) and carries compensating
coverage. R5/R6: routed, never silently skipped — all run natively and under ASan.

**4-criterion basis (all entries):** (1) intractable/unrunnable under Miri as measured;
(2) compensating coverage exists — native `cargo test -p rustty-vt` (2035 pass) + the
ASan lane (`sanitizers.sh`, all PageList tests ~18s); (3) the excluded behavior is not a
Miri-detectable soundness property (timeouts/FFI/ICE, not UB — the sweep's 9 real UB
findings were FIXED, commit 9e3fe1c, not excepted); (4) documented here with cause.

- **Multi-page-PageList intractable (70 tests):** build/reflow multi-page PageLists that
  exceed Miri's interpreter budget (>20min isolated). In `pagelist/tests.rs`,
  `terminal.rs`, `screen.rs`, `formatter.rs`, `search/{viewport,sliding_window}.rs`.
  This is the same class as the pre-existing 23 PageList exceptions (now enforced in
  source). Isolation-confirmed for the 6 sub-timeout candidates; the 64 hit the full
  1200s discovery budget. ASan runs all PageList tests in ~18s.
- **zlib C FFI (3 tests, now 4):** `kitty::graphics_image::image_load_rgb_zlib_compressed_*` and (P7-W5 T2) `image_load_png_zlib_compressed_direct` —
  Miri cannot call the vendored zlib decoder (`inflateInit2_`; since P7-W5 T2, DECISIONS B8, the Wuffs `wuffs_zlib__decoder__transform_io`), a foreign function. ASan covers the
  C boundary.
- **rustc/Miri ICE (1 test):** `search::thread::search_thread` — Miri interpreter ICE
  (`rustc_const_eval/.../operand.rs:160`, an upstream Miri bug, not a soundness signal).

Full per-test list + measured durations: `/Users/gael/.claude/jobs/22556231/tmp/miri_master.txt`
(regenerable) and the `#[cfg_attr(miri, ignore = "MIRI EXCEPTION ...")]` reason strings in source.

## WS7 Phase A — LZ4 block codec: routed to an isolated harness (2026-07-28)

The plan's task A6 calls for `bash .port/tools/miri-run.sh rustty-vt lz4_ 4 20 10`. That
harness (and any `cargo +nightly miri test -p rustty-vt`) must first build the whole
2073-test `rustty-vt` test binary under Miri. On this host that build alone ran **70+
minutes without completing** and was killed before a single codec test executed. This is
the same intractability class as the `rustty-vt` full-sweep entry above, except the cost is
in the *build*, not the tests — so no per-test budget can recover it.

**Routed, not skipped.** `crates/rustty-vt/src/compress/lz4.rs` is a dependency-free leaf
module, so the established `.port/phase-2/mirihar-*` pattern applies. The harness at
`.port/phase-2/mirihar-compress-lz4/` includes the **real source file** via
`#[path = "../../../../crates/rustty-vt/src/compress/lz4.rs"]` — it is not a copy, and it
interprets any later change to the codec.

| model | command | result | time |
|---|---|---|---|
| Stacked Borrows | `cargo +nightly miri run --quiet` | **CLEAN** — no UB, no leak | 41 s |
| Tree Borrows | `MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q` | **CLEAN** — no UB, no leak | 180 s |

Paths executed under both models: structured page-shaped input, all-zeros (long/wide
copies), text, incompressible (literal-only), period-1..8 overlapping matches (the
`copy_match` overlapping wide-copy branches), 17 boundary sizes, the exact-size decode
contract in both directions, and — the property that actually needs an interpreter —
decoding of 15 truncations, 64 single-byte corruptions and 64 fully random blocks.

**4-criterion basis:** (1) intractable as measured (70+ min build, killed, no test run);
(2) compensating coverage — all 15 codec tests pass natively in BOTH release and debug
(debug = overflow checks on), plus `RUSTTY_LZ4_SLOW=1` exhaustive in both profiles;
(3) no uncovered unsafe path — **the module contains zero `unsafe`**, so there is no
raw-pointer provenance for Miri to check; the properties that remain (OOB slice access,
uninit reads, integer UB, leaks) are exactly what the harness executes; (4) documented
here with cause, and the harness is committed so it is re-runnable in seconds.

Phase B's `.port/phase-2/mirihar-compress/` (the compress/restore *seam*, which has real
raw-pointer aliasing) is separate and still owed.

## WS7 Phase B — compressed scrollback pages: isolated harness + one OPEN finding (2026-07-29)

Harness: `.port/phase-2/mirihar-pagelist-compress/` — a binary crate that depends on the
REAL `rustty-vt` by path (only the library is built under Miri, not the 2136-test test
binary, which has never completed on this host — see the Phase A entry above). Scenarios
are selectable: `split`, `siblings`, `roundtrip`, `leak`, `leakcheck`, and (added in WS7
Phase E) `mapinit`, `mapinit_styled`, `leakcheck_negative`.

Accommodations (all `cfg(miri)`, none affecting production behaviour):
`rustty-os`'s page allocator already served page memory from the std heap under
`cfg(miri)`; `crates/rustty-vt/src/mem.rs`'s `can_reclaim`/`decommit`/`recommit` now take
their simulated arm under `cfg!(miri)` as well as `cfg!(test)` (Miri has no FFI, so
`madvise` is uncallable, and the std heap is not an OS mapping); the harness sets
`[profile.dev] debug-assertions = false` because `Page::assert_integrity` is O(page), runs
on nearly every PageList mutation, and adds no SB/TB or leak coverage.

| scenario | model | result | time |
|---|---|---|---|
| `split` + `siblings` (the I2 aliasing proof) | Stacked Borrows | **CLEAN** — no UB, no leak | ~4 min |
| `split` with the PRE-FIX reference-derived `Node::page` | Stacked Borrows | **UB reproduced as predicted** (see below) | ~3 min |
| `roundtrip` + `leak` | Stacked Borrows | **OPEN FINDING** — see below | ~45 min |

> **OWNER [WS19 T19, 2026-07-29]: WS19 T5** (audit row O1). This OPEN FINDING is the
> `style::Color` uninitialized-payload defect: `crates/rustty-vt/src/style.rs:143` is a
> plain Rust `enum Color { None, Palette(u8), Rgb(RGB) }` with no `#[repr]`, so a typed
> write of a `Style` leaves the inactive payload uninit and `CompressedPage::init`
> compresses those bytes. It was filed to **WS8**, which has MERGED — the row was
> ORPHANED. **NOT yet executed:** the fix is a byte-complete `#[repr(C)]` value with
> accessors (closer to the reference's `union(Tag)` than a Rust enum is), across ~160
> call sites, with the `mapinit_styled` harness scenario as the RED and the production
> Miri test `pagelist_compress_page_round_trips_a_cold_history_page` as the acceptance
> evidence. Gate **G-P2** reports it RED until then.

### The I2 negative control (this is the proof the fix is load-bearing)

Restoring B6's `match &mut *addr_of_mut!((*node).data) { Resident(p) => p as *mut Page }`
body and running `split` gives, at `pagelist/grow.rs:128` (inside `split`'s clear-rows
block, after the tracked-pin loop):

```
error: Undefined Behavior: trying to retag from <364963> for SharedReadOnly permission
       at alloc1161[0x1e8], but that tag does not exist in the borrow stack for this location
help: <364963> was created by a SharedReadWrite retag at offsets [0x1e8..0x378]
help: <364963> was later invalidated at offsets [0x1e0..0x390] by a Unique retag
```

Exactly the predicted failure: the `&mut NodeData` in the second accessor call is the
`Unique` retag that invalidates the first call's result. With the `#[repr(C, u8)]` +
`offset_of!` accessors it is clean.

### FINDING (RESOLVED in WS7 Phase E, see the Phase E section below) — uninit bytes in the page mapping fed to LZ4

`roundtrip`/`leak` abort in `compress_page`:

```
error: Undefined Behavior: constructing invalid value of type [u8; 8]: at [2],
       encountered uninitialized memory, but expected an integer
  3: rustty_vt::compress::lz4::read_u64            compress/lz4.rs:720
  4: rustty_vt::compress::lz4::match_end_position  compress/lz4.rs:355
  5: rustty_vt::compress::lz4::compress            compress/lz4.rs:214
  6: rustty_vt::compress::CompressedPage::init     compress/page.rs:137
  7: PageList::compress_page                       pagelist/compress.rs:406
```

This is **not pre-existing** — it is **introduced by B7; the Phase A codec is not
implicated and `main` is unaffected**. `rustty-os/src/page_allocator.rs`'s `cfg(miri)` arm
(`miri_alloc::allocate`) serves page memory via `Global.allocate_zeroed`, so under Miri the
mapping IS fully initialized on handout — identical to the mmap contract. The uninit bytes
Miri reports are therefore real de-initialization: typed writes of padded Rust structs into
the mapping (`Cell`/`Row` are `repr(transparent) u64` with no padding, so the candidates are
the style/hyperlink/string/grapheme table entries and the alignment gaps between
sub-tables), not a Miri-vs-FFI artifact. The defect is entirely at the B7 call site —
`CompressedPage::init`'s `slice::from_raw_parts(raw_ptr, raw_len)` reads the WHOLE mapping
including those never-written gaps. Phase A's LZ4 codec faithfully reads whatever slice
it is given (every Phase A test feeds it a fully-initialized `Vec`), and nothing merged to
`main` today feeds a page mapping to LZ4 — merging Phase B is what introduces the exposure.

The correct impact justification is blast radius, not FFI: the poisoned bytes are padding /
inter-table gaps only, never live cell data. Under the Rust abstract machine, LLVM would
have to materialize genuinely inconsistent values across two reads of the *same* padding
byte for this to do anything at all; the worst realistic outcome is a spurious B7 verify
failure (the page simply stays resident — a SAFE failure) or differing padding bytes after
a decompress/restore round-trip — never cell corruption. A future, more aggressive compiler
could in principle exploit the formal UB, but not into anything worse than that.

**Status: RESOLVED in WS7 Phase E** (fix + measured before/after in the
"WS7 Phase E" section below; two claims in the paragraphs above were WRONG and are
corrected there — the "alignment gaps between sub-tables" hypothesis, and the framing of
the defect as being "entirely at the B7 call site"). It does NOT affect the I2 or I3
evidence: `split`/`siblings` are SB-clean with the negative control reproducing the exact
predicted UB, and the I3 leak check is proven failing-then-passing natively through the
harness's counting `#[global_allocator]` (now also a gate — G6b).

## WS7 Phase E — the uninit-in-page-mapping finding: root-caused and FIXED (2026-07-29)

### What it actually was (two corrections to the Phase B write-up)

The Phase B entry listed "the alignment gaps between sub-tables" as a candidate source and
placed the defect "entirely at the B7 call site". **Both are wrong.**

* The inter-table alignment gaps are NOT a source. Nothing writes them, and the mapping
  arrives fully zeroed (mmap/`VirtualAlloc` by contract; `Global.allocate_zeroed` under
  `cfg(miri)`). They stay initialized.
* The defect is not in `CompressedPage::init` at all. Compressing the mapping as one
  `&[u8]` is exactly what the oracle does (`compress/Page.zig`'s
  `lz4.compress(source.memory, ...)`), so changing that read would have been the fidelity
  break, not the fix. The bug was a WRITE that de-initialized part of the mapping.

The write was `RefCountedSet::init`'s
`slice::from_raw_parts_mut(items.ptr(base), l.cap).fill(Item::default())` — the port of the
reference's `@memset(items.ptr(base)[0..l.cap], .{})`. Every terminal page runs it twice
(the style set and the hyperlink set), over the item array *inside the page mapping*.
`Item<Style, Id, RefCount>` contains three `style::Color`s, and `style::Color` is a Rust
enum whose `None` and `Palette(u8)` variants do not cover all four bytes of their slot. A
typed Rust write initializes the discriminant plus the ACTIVE variant's fields and leaves
every other byte of the destination **uninitialized** — Miri actively de-initializes them,
even when the destination was zeroed a moment earlier. This was verified in isolation
before touching anything:

```
// probe: write Color::None over a zeroed heap buffer, read the 4 bytes back
error: Undefined Behavior: reading memory at alloc2085[0x1..0x2], but memory is
       uninitialized at [0x1..0x2], and this operation requires initialized memory
```

### The fix

`crates/rustty-vt/src/ref_counted_set.rs`'s `init` now zeroes the item array's bytes and
writes `meta` FIELD BY FIELD (`bucket`, `psl`, `ref_count` are integers; a field-wise write
touches only that field's bytes and cannot re-poison the surrounding ones). See the PORT
NOTE at the site.

**Why this is the fidelity-correct choice among the four candidates the Phase B write-up
listed.** The reference's `Item.value` field has NO default — the reference's own comment is
that it is `undefined` — so the reference's `.{}` leaves `value` unspecified and reads of a
dead item's value are already forbidden. Writing zeroes there is *strictly more defined*
than the reference, and the all-zero item-value state is already reachable today (the page
mapping arrives zeroed, and `Page::reinit` re-zeroes it before `init_buf` runs). By
contrast: "zero the padding on write" cannot be expressed generically in Rust (there is no
way to enumerate which bytes a typed write left uninitialized); "read through a
freeze/volatile path" does not work at all (a volatile or byte-wise read of an
uninitialized byte is the very UB being reported, and a byte-wise copy propagates
uninit-ness rather than defining it); and a per-path exception would have left real,
fixable UB in production code.

### Measured before/after

The Phase B evidence run took ~45 min because `roundtrip`/`leak` build several
std-capacity (215x215) pages and LZ4 every byte of each mapping. The defect needs neither,
so Phase E added a **seconds-long** probe to the harness, `mapinit`: allocate one small
`Page` and read every byte of its mapping exactly as `CompressedPage::init` does. Under
Miri that aborts at the FIRST uninitialized byte and names its offset.

| probe | before the fix | after the fix |
|---|---|---|
| `mapinit` (one page, `styles: 128`) | `error: Undefined Behavior: reading memory at alloc1356[0x160..0x161] ... memory is uninitialized` (offset 0x160 = the first `Item` slot in the style set's item array) | `mapping of 16384 bytes fully initialized (checksum 57974)` — CLEAN |
| `mapinit` (one page, `styles: 1`) | CLEAN even before the fix — `items_cap = 0.8 * table_cap = 0`, so the fill never runs. Recorded because it is why a too-small probe capacity gives a false green | CLEAN |
| `cargo test -p rustty-vt --lib` | 2157 passed / 0 failed / 2 ignored | 2157 passed / 0 failed / 2 ignored (unchanged) |

**The decisive before/after is on a REAL production test, not the harness.** The harness's
`roundtrip`/`leak` scenarios could not be used as the acceptance run: once the fix lets
them past the abort they interpret several std-capacity pages to completion, and two
attempts were killed by the host at ~45 min and ~65 min without finishing. The equivalent
coverage — and better, because it is the shipped test rather than a harness scenario — comes
from running `pagelist::tests::pagelist_compress_page_round_trips_a_cold_history_page`
itself under Miri (`-C debug-assertions=off --include-ignored`, so the interpretation fits
in minutes; it is `cfg_attr(miri, ignore)` per the Phase E section below). It drives
`PageList::compress_page` -> `CompressedPage::init` -> `decommit` -> `restore` and compares
the restored bytes:

```
BEFORE (ref_counted_set.rs reverted to 76bd5a9^, everything else identical):
  error: Undefined Behavior: constructing invalid value of type [u8; 8]: at [2],
         encountered uninitialized memory, but expected an integer
  (2m28s to the abort)

AFTER:
  test result: ok. 1 passed; 0 failed; ... finished in 194.01s
```

`pagelist_compress_drain_finishes_in_one_call` — which compresses SIX std-capacity page
mappings — also runs to completion post-fix (`ok, 1631.36s`, zero UB, zero leaks). Both
would abort in `lz4::compress` within seconds before the fix.

### ~~KNOWN RESIDUAL~~ — RESOLVED in WS19 T5 (2026-07-29)

> **RESOLVED — WS19 T5.** The residual described below is FIXED, and its 4-criterion
> acceptance basis is therefore retired (kept verbatim for the record, struck through at
> the end of this subsection). `style::Color` is now a byte-complete
> `#[repr(C)] { tag: u8, data: [u8; 3] }` — a `u8` tag plus the reference's own 24-bit
> `PackedStyle.Data` payload — with `Color::None` / `Color::Palette(idx)` /
> `Color::Rgb(rgb)` preserved as an associated const and two associated constructors, so
> the ~160 construction sites did not move; only the ~22 pattern-match sites changed, to
> `tag()` + tag-checked `palette()` / `rgb()` accessors. `const _` asserts pin
> `size_of == 4`, `align_of == 1`. This is a fidelity *improvement*, not a weakening: the
> four bytes are exactly what the reference specifies per tag (`none => 0`,
> `palette => idx` with `_padding: u16 = 0`, `rgb => r|g<<8|b<<16`), so `data_from_color`
> is now a little-endian widening of the type's own bytes.
>
> Fixing `Color` exposed a **second, previously unknown instance of the same class**:
> `RefCountedSet::insert` wrote whole `Item`s (`*item = Item::default()` on reaping a dead
> item; `*items.add(chosen_id) = new_item` on placement). `Item<Style, u16, u16>` is 28
> bytes (`meta` at 0..6, `value` at 6..27) and so carries **1 trailing padding byte**,
> which a typed whole-struct write de-initializes even over the zeroes `init` wrote. Both
> writes are now FIELD-WISE, the same accommodation `init` already documents.
>
> **Measured, on the committed `mapinit_styled` probe (seconds, primary signal):**
>
> | state | `mapinit_styled` |
> |---|---|
> | before WS19 T5 | `error: Undefined Behavior: reading memory at alloc1412[0x17c..0x17d], but memory is uninitialized` |
> | after the `Color` reshape only | `error: … at alloc1412[0x187..0x188] …` — i.e. the colour slots were clean and the *`Item` padding byte* was the next offender (0x187 = item base 0x16c + 27) |
> | after the field-wise `Item` writes | `styled mapping of 16384 bytes fully initialized (checksum 57545)` — CLEAN |
> | `mapinit` (regression check) | `mapping of 16384 bytes fully initialized (checksum 57974)` — CLEAN |
> | `cargo test -p rustty-vt --lib` | 2159 passed / 0 failed / 2 ignored — unchanged |
>
> **The whole uninit-in-page-mapping class is now closed**, so no exception remains for it.
> `roundtrip`/`leak` were NOT used as acceptance (they were host-killed at ~45 and ~65
> minutes and never finished); the production-test acceptance run is recorded in the
> WS19 T6 section below.

<details><summary>Retired 4-criterion basis (historical record)</summary>

`RefCountedSet::add` must place the caller's `T` into the item slot with a typed write, so
a page that has stored at least one non-default `style::Style` (or hyperlink `PageEntry`)
still contains uninitialized bytes in the unused payload of each `style::Color`. New
harness scenario `mapinit_styled` reproduces it deterministically in seconds, so this is
MEASURED, not assumed:

```
error: Undefined Behavior: reading memory at alloc1407[0x17c..0x17d], but memory is
       uninitialized at [0x17c..0x17d], and this operation requires initialized memory
```

**Why it is not fixed here, concretely.** There is no way to place a padded/niche-free-less
Rust value into memory without leaving those bytes uninitialized; the only fix is to make
the stored type padding-free, i.e. turn `style::Color` from a Rust enum into a
byte-complete `#[repr(C)]` value with accessors. That is **163 construction/match sites
across 17 files** (`crates/rustty-vt/src/terminal.rs`, `screen.rs`, `render.rs`,
`formatter.rs`, `kitty/graphics_unicode.rs`, `rustty-vt-c`, `rustty-inspector`,
`rustty-vaxis`, `rustty-font-shaper`, `rustty-cli`), several of which were owned by other
in-flight WS7 lanes at the time. It is a cross-crate type change, not a compression fix.

**4-criterion basis for accepting the residual for now:** (1) *measured*, not assumed —
`mapinit_styled` is a committed, seconds-long reproduction with the exact offset;
(2) *compensating coverage* — the whole-mapping initialization property IS gated by
`mapinit` (empty pages, which is what `PageList` compresses in the common cold-history
case), the native suite is green, and ASan sees no corruption; (3) *no uncovered unsafe
path* — the aliasing and lifetime properties of the compress/restore seam are separately
Miri-proven by `split`/`siblings`/`roundtrip`/`leak`; what remains is a byte-validity
property of one leaf type, whose blast radius is the *unused payload bytes of a colour
slot* only — never cell data, never an offset, never a length (worst realistic outcome
remains a spurious verify failure, which leaves the page resident: a SAFE failure);
(4) *documented here with the exact required change* and carried as a WS8 work item, not
left silently red.

</details>

## WS7 Phase E — the page-compression test family, measured per test (2026-07-29)

DEFERRALS.md's WS7-Phase-E item 4 recorded 4 compression tests as Miri-intractable and
undocumented, and asked Phase E to measure the REST of the family the same way. Done. The
sweep also found **2 more intractable tests the earlier pass had not named**
(`pagelist_mutations_mark_compression_activity`,
`pagelist_memory_stats_accounts_compressed_and_resident_bytes`) and **1 that is TRACTABLE
and is deliberately left un-annotated**.

### Method

`bash .port/tools/miri-run.sh rustty-vt pagelist_compress 4 3 6` for discovery, then
**every candidate re-measured in single-worker isolation** (`... <exact-name> 1 5 6`)
before being excepted — parallel Miri over-reports under memory pressure, so a parallel
TIMEOUT is a candidate, never a verdict. Native debug times were taken alongside because
they are the cheapest predictor of Miri cost. macOS has no coreutils `timeout`, so the
bound always comes from `miri-run.sh`'s own `killtree`.

### Results (default lane: debug assertions ON, 6-minute hard bound)

| Test | parallel (4 workers) | ISOLATED (1 worker) | native debug | disposition |
|---|---|---|---|---|
| `pagelist_compress_drain_finishes_in_one_call` | TIMEOUT 360s | **TIMEOUT 360s** | 2.49s | annotated |
| `pagelist_compress_full_leaves_viewport_and_active_resident` | TIMEOUT 360s | **TIMEOUT 360s** | 1.51s | annotated |
| `pagelist_compress_full_mode_leaves_no_continuation` | TIMEOUT 360s | **TIMEOUT 361s** | 1.49s | annotated |
| `pagelist_compress_incremental_converges_to_complete` | TIMEOUT 360s | **TIMEOUT 360s** | 2.51s | annotated |
| `pagelist_compress_incremental_restarts_after_list_mutation` | TIMEOUT 360s | **TIMEOUT 360s** | 4.48s | annotated |
| `pagelist_compress_page_decommit_failure_leaves_the_page_resident` | TIMEOUT 360s | **TIMEOUT 360s** | 1.00s | annotated |
| `pagelist_compress_page_rejects_an_already_compressed_node` | TIMEOUT 360s | **TIMEOUT 360s** | 1.00s | annotated |
| `pagelist_compress_page_round_trips_a_cold_history_page` | TIMEOUT 360s | **TIMEOUT 360s** | 1.00s | annotated |
| `pagelist_mutations_mark_compression_activity` (NEW find) | — | **TIMEOUT 360s** | 0.50s | annotated |
| `pagelist_memory_stats_accounts_compressed_and_resident_bytes` (NEW find) | — | **TIMEOUT 360s** | 1.50s | annotated |
| `pagelist_page_preserving_state_leaves_a_compressed_node_compressed` | — | **ok, 77s** | 0.00s | **TRACTABLE — left un-annotated** |
| `search::pagelist::…::compressed_history_match_spanning_page_boundary_remains_compressed` | — | already `cfg_attr(miri, ignore)` | — | unchanged |

The last row is also a worked example of the harness caveat: `miri-run.sh` classified it
`res=FAIL` and printed `VERDICT: PROBLEM`, but the per-test output file reads
`ignored, MIRI EXCEPTION …` — the harness scores an ignored test as FAIL. That banner is
never by itself evidence of a problem; the per-test output is.

So of 12 measured, **10 are excepted, 1 is tractable and stays on the Miri-gated surface,
1 was already excepted.** This is the same criterion-4 discipline as the Phase C pass, and
it paid again: `pagelist_page_preserving_state_leaves_a_compressed_node_compressed` passes
in 77s, and a blanket "the compression tests are intractable" annotation would have hidden
a working check.

### Root-cause investigation: WHY compression tests are this expensive

They are not hangs and they are not a logic difference from their tractable siblings. Two
independent measurements establish that:

1. **Native debug runtime already separates them.** A typical PageList test finishes in
   0.00s; every excepted test here is 0.50–4.48s, i.e. 100–1000x more work *before* Miri's
   interpreter multiplier. The work is `fill_compressible_history`, which calls
   `PageList::grow()` once per row until N std-capacity (215x215) pages exist — ~215 * N
   grows — followed by `compress()`, which LZ4-compresses AND verify-decompresses every
   byte of each ~16 KB-multiple page mapping. Millions of interpreted byte operations.
2. **They terminate cleanly under Miri when given the time.** Two of them were run to
   COMPLETION under Miri (with `-C debug-assertions=off`, see below), both `1 passed`,
   **zero UB, zero leaks**:

   | Test | Miri, `-C debug-assertions=off` |
   |---|---|
   | `pagelist_compress_drain_finishes_in_one_call` | **ok, 1631.36s** (27m11s wall) |
   | `pagelist_compress_page_round_trips_a_cold_history_page` | **ok, 194.01s** |

   That is direct positive evidence, stronger than the usual criterion-3 argument from a
   lighter sibling: these tests are Miri-CLEAN, just slow.

   A wider `-C debug-assertions=off` sweep (3 workers, 20-min bound) confirms the split:
   `pagelist_compress_full_mode_leaves_no_continuation`,
   `pagelist_compress_drain_finishes_in_one_call` and
   `pagelist_compress_full_leaves_viewport_and_active_resident` all still TIMEOUT at 1200s
   even with integrity checks disabled (consistent with drain's 1631s solo run), i.e. the
   multi-page tests are LZ4-volume-bound and no lane flag rescues them. The other five in
   that sweep reported `res=FAIL` in 8s purely because the annotations in this commit had
   already landed and the harness scores an ignored test as FAIL — again, the per-test
   output files, not the banner.

**The one lever that helps, and why it is not applied here.** `Page::assert_integrity` is
O(page) and runs on nearly every PageList mutation under `debug_assertions`.
`.port/phase-2/mirihar-pagelist-compress` already exploits this (`[profile.dev]
debug-assertions = false`, on the documented grounds that integrity checking adds no
Stacked-Borrows/Tree-Borrows or leak coverage). Applying the same lever to the crate lane
moves `pagelist_compress_page_round_trips_a_cold_history_page` from TIMEOUT(360s) to
**ok in 194s** — i.e. the page-level tests are dominated by `assert_integrity`, and would
be TRACTABLE in a `RUSTFLAGS=-Cdebug-assertions=off` Miri lane — while the multi-page ones
are dominated by LZ4 volume and stay far out of budget (drain: 1631s). So:

* The annotations above are correct for the lane as it actually runs today
  (`check_leaks.sh` runs `cargo +nightly miri test -p rustty-vt --lib`, debug assertions
  ON). Without them the rustty-vt G6 lane cannot complete at all.
* **Recommended follow-up (WS8, not done here):** add a `-C debug-assertions=off` variant
  of the vt Miri lane and re-measure the whole `cfg_attr(miri, ignore)` inventory against
  it. Changing G6's lane flags is a cross-crate change to a shared gate and would
  re-classify far more than the compression family, so it does not belong in this lane.

> **SUPERSEDED IN PART — WS19 T6 (2026-07-30).** This basis was written against a
> one-lane world. 2 of these 10 are now RUN by the G6c `-C debug-assertions=off`
> lane, and the remaining 8 were re-measured against it. See
> "WS19 T6 — the SECOND vt Miri lane" at the end of this file for the current,
> per-test, two-lane basis.

### 4-criterion basis for the 10 annotations (as of WS7 Phase E; see the WS19 T6 note above)

1. **Empirically intractable** — TIMEOUT at a 6-minute hard bound, measured per test in
   single-worker isolation (table above), not assumed from the class. The two that were
   run without a bound needed 194s and 1631s *with* the integrity checks disabled.
2. **Passes natively** — all 10 pass in `cargo test -p rustty-vt --lib` (2157 passed / 0
   failed / 2 ignored, unchanged by this work) and in the release lane.
3. **No uncovered unsafe path** — two of the ten have now been run to COMPLETION under
   Miri with zero UB and zero leaks (see above), which covers `compress(Drain)`,
   `compress(Incremental)` (Drain drives incremental steps plus the verification pass),
   `compress_page`, `CompressedPage::init`/`restore`, `decommit_pool_item` and the
   node storage transition. `pagelist_page_preserving_state_leaves_a_compressed_node_compressed`
   stays on the Miri surface (77s) and covers `Node::page_preserving_state` over a
   compressed node. The compress/restore SEAM's aliasing is separately Miri-proven by
   `.port/phase-2/mirihar-pagelist-compress`'s `split`/`siblings`/`roundtrip`/`leak`, and
   leak-freedom of the encodings is now continuously gated natively by G6b.

   > **`Pending` does NOT mean "unfinished work remains" [WS19 T18, audit row N10].**
   > A fresh `compress(Incremental)` issued right after a pass that returned `Complete`
   > returns **`Pending`** again — the incremental traversal RESTARTS rather than
   > remembering that it converged (observed, and pinned by
   > `terminal_compress_incremental_restarts_after_complete` in
   > `crates/rustty-vt/src/terminal.rs`). This matters for the coverage argument above:
   > "drive until `Complete`" is the correct way to exercise a full pass, but
   > `result == Complete` is **not** a durable "compression is done" probe, so a test
   > that calls `compress` once and asserts `Complete` is asserting something else than
   > it appears to. The `Terminal::compress` doc comment carries the same note.
4. **Intractability is allocation volume, not logic** — established by both measurements
   in the root-cause section: the same methods on a single page are tractable
   (77s / 194s), and the excepted tests differ only in how many std-capacity page
   mappings they build and compress.


## WS19 T6 — the SECOND vt Miri lane (G6c, `-C debug-assertions=off`), and the whole `cfg_attr(miri, ignore)` inventory re-measured against it (2026-07-30)

**What this closes.** `DEFERRALS.md`'s "NEW WS8 item" — closure-audit row **O2**: *add a
`-C debug-assertions=off` variant of the vt Miri lane and re-measure the whole
`cfg_attr(miri, ignore)` inventory against it*. WS8 closed without taking it. WS19 T6 owns
and closes it.

**The lane.** `bash .port/tools/gates/check_leaks.sh --debug-assertions-off` — **G6c**,
registered in `.port/tools/ci.sh` as PROGRESS. It is **ADDITIVE, never a replacement**
(R5): G6 still runs `cargo +nightly miri test -p rustty-vt --lib` with debug assertions ON,
unchanged, because that is what upholds the `Page::assert_integrity` coverage and because
without the annotations the vt G6 lane cannot complete at all. G6c disables
`assert_integrity` — which is O(page), runs on nearly every PageList mutation, and adds no
Stacked-Borrows/Tree-Borrows and no leak coverage — and thereby *runs* the tests G6 can only
time out on. The partition is expressed in the annotation itself:
`#[cfg_attr(all(miri, debug_assertions), ignore = …)]` = ignored in G6, RUN in G6c;
`#[cfg_attr(miri, ignore = …)]` = out of both. G6c derives its test set by grepping those
annotations out of `crates/rustty-vt/src`, so **the annotation is the registration** and the
two lanes cannot drift apart.

### Headline: the inventory was 97 tests, and 82 of them were never intractable

The `cfg_attr(miri, ignore)` inventory for `rustty-vt` is **109** annotations: 97 in the two
"intractable" classes this pass re-measured (87 `multi-page-PageList intractable` + 10
`page-compression intractable`), plus 11 kitty-graphics ones (real-OS temp files / zlib FFI —
a different exception class, untouched here) and 1 rustc/Miri interpreter ICE
(`search::thread::search_thread` — untouched).

Of the 97:

| | |
|---|---|
| **RESCUED** — now RUN, UB-clean and leak-clean, in G6c | **82** |
| Still excepted, re-measured and justified per test | **15** |
| Real Stacked-Borrows violations the blanket annotation was hiding | **10** (all fixed here) |

The `multi-page-PageList intractable` reason text asserted these tests "exceed Miri's
interpreter budget (>20min)". Measured per test, the great majority finish in **6–30
seconds**. That number was a class-level assumption, never a per-test measurement, and it is
the exact failure mode this file's own criterion 1 exists to prevent ("*measured per test in
single-worker isolation, not assumed from the class*"). It is corrected below and in every
annotation.

### Method

* Build once: `RUSTFLAGS="-C debug-assertions=off" cargo +nightly miri test -p rustty-vt --lib --no-run` (**9.3s** — worth recording, because this file has twice carried a "70-minute Miri build" figure that was never real).
* The 10-test page-compression family: **each test measured in single-worker isolation**, `--exact`, `MIRIFLAGS=-Zmiri-disable-isolation`, hard 6-minute bound (killtree; macOS has no coreutils `timeout`).
* The 87-test multi-page class: 4-worker **triage** first, then — per this file's own rule that a parallel non-`ok` is a *candidate, never a verdict* — every non-`ok` re-measured in isolation. That rule paid immediately: `select_after_clearing_scrollback` was `TIMEOUT` at 240s in the parallel run and is **ok in 260s** in isolation, so it is rescued rather than excepted.
* Finally the whole rescued set is re-run **serially in one process** by the G6c lane itself (`--test-threads=1`), which is the isolated confirmation of every row in the first table below. That run is GREEN: `test result: ok. 82 passed; 0 failed; 0 ignored; 0 measured; 2078 filtered out; finished in 3092.38s` — **52 minutes of Miri interpretation that the vt crate previously had no way to run at all.**

### The 82 rescued tests

Each is now `#[cfg_attr(all(miri, debug_assertions), ignore = …)]` and runs in G6c.

| Test (82) | file | G6c (`-C debug-assertions=off`) |
|---|---|---|
| `feed_iteratively_through_multiple_matches` | `search/pagelist.rs` | **ok, 15s** (4-worker triage) |
| `feed_multiple_pages_no_matches` | `search/pagelist.rs` | **ok, 23s** (isolated) |
| `feed_multiple_pages_with_matches` | `search/pagelist.rs` | **ok, 27s** (4-worker triage) |
| `feed_with_match_spanning_page_boundary` | `search/pagelist.rs` | **ok, 13s** (4-worker triage) |
| `feed_with_match_spanning_page_boundary_with_newline` | `search/pagelist.rs` | **ok, 15s** (4-worker triage) |
| `feed_with_pruned_page` | `search/pagelist.rs` | **ok, 21s** (4-worker triage) |
| `history_search_no_active_area` | `search/viewport.rs` | **ok, 20s** (4-worker triage) |
| `link_cells_with_scrollback_spanning_pages` | `render.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_bounded_pruning_after_front_replacement_preserves_live_serials` | `pagelist/tests.rs` | **ok, 8s** (4-worker triage) |
| `pagelist_bounded_pruning_after_middle_replacement_preserves_live_serials` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_bounded_pruning_after_partial_erase_preserves_live_serials` | `pagelist/tests.rs` | **ok, 8s** (4-worker triage) |
| `pagelist_bounded_pruning_after_split_invalidation_preserves_live_serials` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_compact_oversized_page` | `pagelist/tests.rs` | **ok, 49s** (4-worker triage) |
| `pagelist_compress_page_round_trips_a_cold_history_page` | `pagelist/tests.rs` | **ok, 195s** (isolated) |
| `pagelist_destroyed_pool_page_reuse_is_zeroed` | `pagelist/tests.rs` | **ok, 167s** (4-worker triage) |
| `pagelist_eraseRowBounded_exhausts_pages_invalidates_viewport_offset_cache` | `pagelist/tests.rs` | **ok, 6s** (4-worker triage) |
| `pagelist_eraseRowBounded_full_page_shift_invalidates_viewport_offset_cache` | `pagelist/tests.rs` | **ok, 6s** (4-worker triage) |
| `pagelist_eraseRowBounded_invalidates_viewport_offset_cache` | `pagelist/tests.rs` | **ok, 8s** (4-worker triage) |
| `pagelist_eraseRowBounded_multi_page_invalidates_viewport_offset_cache` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_eraseRow_invalidates_viewport_offset_cache` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_eraseRows_invalidates_viewport_offset_cache` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_grow_allocate` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_grow_non_standard_page_prune_protection` | `pagelist/tests.rs` | **ok, 8s** (4-worker triage) |
| `pagelist_grow_prune_required_with_a_single_page` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_grow_prune_scrollback` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_grow_prune_scrollback_with_viewport_pin_not_in_pruned_page` | `pagelist/tests.rs` | **ok, 8s** (4-worker triage) |
| `pagelist_grow_reuses_non_standard_page_without_leak` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_increaseCapacity_multi_page` | `pagelist/tests.rs` | **ok, 8s** (4-worker triage) |
| `pagelist_mutations_mark_compression_activity` | `pagelist/tests.rs` | **ok, 5s** (isolated) |
| `pagelist_only_reset_advances_the_serial_epoch` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_pagepool_no_leak_clone_many_pages` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_pagepool_no_leak_grow_and_scroll` | `pagelist/tests.rs` | **ok, 6s** (4-worker triage) |
| `pagelist_pagepool_no_leak_reset_then_deinit` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_pagepool_no_leak_resize_reflow` | `pagelist/tests.rs` | **ok, 16s** (4-worker triage) |
| `pagelist_plain_spanning_two_pages` | `formatter.rs` | **ok, 12s** (isolated) |
| `pagelist_plain_with_x_offset_spanning_two_pages` | `formatter.rs` | **ok, 6s** (isolated) |
| `pagelist_repeated_bounded_pruning_after_split_preserves_live_serials` | `pagelist/tests.rs` | **ok, 8s** (4-worker triage) |
| `pagelist_resize_more_rows_and_cols_doesnt_fit_in_single_std_page` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_resize_no_reflow_less_rows_trims_blank_lines_erases_pages` | `pagelist/tests.rs` | **ok, 27s** (4-worker triage) |
| `pagelist_resize_no_reflow_more_cols_forces_less_rows_per_page` | `pagelist/tests.rs` | **ok, 10s** (4-worker triage) |
| `pagelist_resize_no_reflow_more_cols_forces_smaller_cap` | `pagelist/tests.rs` | **ok, 14s** (4-worker triage) |
| `pagelist_resize_no_reflow_more_cols_remaps_pins_in_backfill_path` | `pagelist/tests.rs` | **ok, 58s** (4-worker triage) |
| `pagelist_resize_reflow_exceeds_grapheme_memory_forcing_capacity_increase` | `pagelist/tests.rs` | **ok, 127s** (4-worker triage) |
| `pagelist_resize_reflow_exceeds_hyperlink_memory_forcing_capacity_increase` | `pagelist/tests.rs` | **ok, 165s** (4-worker triage) |
| `pagelist_resize_reflow_exceeds_style_memory_forcing_capacity_increase` | `pagelist/tests.rs` | **ok, 13s** (4-worker triage) |
| `pagelist_resize_reflow_grapheme_map_capacity_exceeded` | `pagelist/tests.rs` | **ok, 129s** (4-worker triage) |
| `pagelist_resize_reflow_more_cols_creates_multiple_pages` | `pagelist/tests.rs` | **ok, 38s** (4-worker triage) |
| `pagelist_row_erasure_renews_affected_page_generations` | `pagelist/tests.rs` | **ok, 7s** (4-worker triage) |
| `pagelist_soft_wrapped_line_spanning_two_pages_with_unwrap` | `formatter.rs` | **ok, 26s** (4-worker triage) |
| `pagelist_soft_wrapped_line_spanning_two_pages_without_unwrap` | `formatter.rs` | **ok, 26s** (4-worker triage) |
| `pagelist_vt_spanning_two_pages` | `formatter.rs` | **ok, 12s** (isolated) |
| `reload_active_with_history_change` | `search/screen.rs` | **ok, 28s** (4-worker triage) |
| `reload_after_partial_history_page_erase_drops_shifted_selection_first` | `search/screen.rs` | **ok, 64s** (isolated) |
| `reload_defers_pruning_unselected_history_results` | `search/screen.rs` | **ok, 12s** (isolated) |
| `screen_cursor_absolute_to_page_with_insufficient_capacity` | `screen.rs` | **ok, 15s** (4-worker triage) |
| `screen_cursor_set_hyperlink_oom_uri_too_large_for_string_alloc` | `screen.rs` | **ok, 16s** (4-worker triage) |
| `screen_scrolling_when_viewport_is_pruned` | `screen.rs` | **ok, 13s** (4-worker triage) |
| `select_after_clearing_scrollback` | `search/screen.rs` | **ok, 260s** (isolated) |
| `select_after_history_compaction_ignores_replaced_results` | `search/screen.rs` | **ok, 62s** (isolated) |
| `select_after_history_page_split_ignores_moved_results` | `search/screen.rs` | **ok, 15s** (isolated) |
| `select_after_partial_history_erase_drops_a_pruned_selection` | `search/screen.rs` | **ok, 119s** (isolated) |
| `select_after_partial_history_page_erase_ignores_shifted_results` | `search/screen.rs` | **ok, 12s** (isolated) |
| `select_into_history` | `search/screen.rs` | **ok, 52s** (4-worker triage) |
| `select_prev_with_history` | `search/screen.rs` | **ok, 53s** (4-worker triage) |
| `select_prev_wraps_when_all_matches_are_in_history` | `search/screen.rs` | **ok, 45s** (4-worker triage) |
| `simple_search_with_history` | `search/screen.rs` | **ok, 49s** (4-worker triage) |
| `sliding_window_two_pages` | `search/sliding_window.rs` | **ok, 11s** (isolated) |
| `sliding_window_two_pages_match_across_boundary` | `search/sliding_window.rs` | **ok, 12s** (4-worker triage) |
| `sliding_window_two_pages_match_across_boundary_reversed` | `search/sliding_window.rs` | **ok, 12s** (4-worker triage) |
| `sliding_window_two_pages_no_match_across_boundary_with_newline` | `search/sliding_window.rs` | **ok, 11s** (4-worker triage) |
| `sliding_window_two_pages_no_match_across_boundary_with_newline_reverse` | `search/sliding_window.rs` | **ok, 14s** (4-worker triage) |
| `sliding_window_two_pages_no_match_keeps_both_pages` | `search/sliding_window.rs` | **ok, 15s** (4-worker triage) |
| `sliding_window_two_pages_no_match_keeps_both_pages_reversed` | `search/sliding_window.rs` | **ok, 20s** (4-worker triage) |
| `sliding_window_two_pages_no_match_prunes_first_page` | `search/sliding_window.rs` | **ok, 16s** (4-worker triage) |
| `sliding_window_two_pages_no_match_prunes_first_page_reversed` | `search/sliding_window.rs` | **ok, 16s** (4-worker triage) |
| `sliding_window_two_pages_reversed` | `search/sliding_window.rs` | **ok, 11s** (4-worker triage) |
| `sliding_window_two_pages_single_char` | `search/sliding_window.rs` | **ok, 16s** (4-worker triage) |
| `terminal_delete_lines_across_page_boundary_marks_all_shifted_rows_dirty` | `terminal.rs` | **ok, 13s** (4-worker triage) |
| `terminal_input_glitch_text` | `terminal.rs` | **ok, 8s** (4-worker triage) |
| `terminal_insert_lines_across_page_boundary_marks_all_shifted_rows_dirty` | `terminal.rs` | **ok, 13s** (4-worker triage) |
| `terminal_pagepool_no_leak_scroll_insert_multi_page` | `terminal.rs` | **ok, 11s** (4-worker triage) |
| `terminal_resize_with_high_unique_style_per_cell_with_wrapping` | `terminal.rs` | **ok, 17s** (4-worker triage) |

### The 15 that stay excepted, and why

| Test (15) | file | G6c verdict | why it is NOT rescuable |
|---|---|---|---|
| `compressed_history_match_spanning_page_boundary_remains_compressed` | `search/pagelist.rs` | TIMEOUT, 361s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |
| `feed_keeps_its_tracked_pin_within_a_shorter_page` | `search/pagelist.rs` | FAIL, 50s (4-worker triage) | needs `debug_assertions` (already `cfg_attr(not(debug_assertions), ignore)`); G6c makes it inert, not fast |
| `pagelist_compress_drain_finishes_in_one_call` | `pagelist/tests.rs` | TIMEOUT, 360s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |
| `pagelist_compress_full_leaves_viewport_and_active_resident` | `pagelist/tests.rs` | TIMEOUT, 360s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |
| `pagelist_compress_full_mode_leaves_no_continuation` | `pagelist/tests.rs` | TIMEOUT, 360s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |
| `pagelist_compress_incremental_converges_to_complete` | `pagelist/tests.rs` | TIMEOUT, 360s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |
| `pagelist_compress_incremental_restarts_after_list_mutation` | `pagelist/tests.rs` | TIMEOUT, 361s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |
| `pagelist_compress_page_decommit_failure_leaves_the_page_resident` | `pagelist/tests.rs` | FAIL, 150s (isolated) | needs `debug_assertions` (already `cfg_attr(not(debug_assertions), ignore)`); G6c makes it inert, not fast |
| `pagelist_compress_page_rejects_an_already_compressed_node` | `pagelist/tests.rs` | TIMEOUT, 360s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |
| `pagelist_memory_stats_accounts_compressed_and_resident_bytes` | `pagelist/tests.rs` | TIMEOUT, 360s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |
| `pagelist_partial_erase_restarts_compression_before_continuation` | `pagelist/tests.rs` | TIMEOUT, 360s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |
| `reload_active_partial_history_cleanup_on_append_slice_error` | `search/screen.rs` | FAIL, 106s (4-worker triage) | needs `debug_assertions` (already `cfg_attr(not(debug_assertions), ignore)`); G6c makes it inert, not fast |
| `reload_active_partial_history_cleanup_on_loop_append_error` | `search/screen.rs` | FAIL, 129s (4-worker triage) | needs `debug_assertions` (already `cfg_attr(not(debug_assertions), ignore)`); G6c makes it inert, not fast |
| `screen_set_attribute_splits_page_on_out_of_space_at_max_styles` | `screen.rs` | TIMEOUT, 360s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |
| `terminal_pagepool_no_leak_resize_grow` | `terminal.rs` | TIMEOUT, 360s (isolated) | TIMEOUT in BOTH lanes — page-mapping/LZ4 volume, not assertion overhead |

Two distinct reasons, both measured, neither a class assumption:

1. **TIMEOUT in BOTH lanes (11).** Turning the integrity assertions off does not rescue
   them: they are bound by how many std-capacity (215x215) page mappings they build,
   LZ4-compress and verify-decompress, not by assertion overhead. `pagelist_compress_drain_finishes_in_one_call`
   remains the calibration point — WS7 Phase E ran it to completion, unbounded, in **1631s
   with assertions already off**, zero UB and zero leaks.
2. **Needs `debug_assertions` (4).** These drive a `debug_assertions`-only mechanism
   (tripwire failure injection, or `PageList::pin_is_valid`, which the port compiles out
   with slow-runtime-safety off, faithful to the reference). They already carry
   `#[cfg_attr(not(debug_assertions), ignore)]`. G6c would make them **inert, not fast**, so
   G6c cannot rescue them and their `cfg_attr(miri, ignore)` is correct. *Do not read the
   `FAIL` in the table as a soundness signal*: it is what a forced `--include-ignored`
   measurement run produces when a test asserts on an injection that cannot fire — e.g.
   `assertion failed: !s.compress_page(node)`, and `assertion failed: cfg!(debug_assertions)`
   from `pagelist/mod.rs:2210` itself. The G6c lane does **not** pass `--include-ignored`, so
   they are correctly ignored there.

### What the lane found: 10 real Stacked-Borrows violations, hidden by the blanket annotation

Making the tests runnable made them *fail*. Ten of the 97 reported genuine Miri
Stacked-Borrows UB, deterministically, in seconds. All ten are one pattern, in **test code**:

```rust
let active_ptr: *mut Screen = t.screens.active;
let list: *mut PageList = unsafe { &mut (*active_ptr).pages };   // cached ...
let mut stream = t.vt_stream();
stream.next_slice(b"error\r\n");                                  // ... invalidated here
let first = unsafe { (*list).pages.first };                       // <- UB: stale tag
```

`Terminal::active_mut()` (`unsafe { &mut *self.screens.active }`, `terminal.rs:5263`) is a
Unique retag over the whole `Screen` allocation, so it pops every raw pointer derived from
that allocation earlier. Reading through the cached one afterwards is UB (memory:
`rust-mut-args-strongly-protected`). Production code never caches this; the fix is
test-local — re-derive per use via a `|| -> *mut PageList { unsafe { &raw mut (*active_ptr).pages } }`
closure — and every assertion is unchanged.

Fixed and re-verified under G6c (all `ok`, zero UB, zero leaks):

| Test | file | after the fix |
|---|---|---|
| `pagelist_plain_spanning_two_pages` | `formatter.rs` | ok, 12s |
| `pagelist_vt_spanning_two_pages` | `formatter.rs` | ok, 12s |
| `pagelist_plain_with_x_offset_spanning_two_pages` | `formatter.rs` | ok, 6s |
| `select_after_partial_history_erase_drops_a_pruned_selection` | `search/screen.rs` | ok, 119s |
| `select_after_history_compaction_ignores_replaced_results` | `search/screen.rs` | ok, 62s |
| `reload_defers_pruning_unselected_history_results` | `search/screen.rs` | ok, 12s |
| `reload_after_partial_history_page_erase_drops_shifted_selection_first` | `search/screen.rs` | ok, 64s |
| `select_after_partial_history_page_erase_ignores_shifted_results` | `search/screen.rs` | ok, 12s |
| `select_after_history_page_split_ignores_moved_results` | `search/screen.rs` | ok, 15s |
| `compressed_history_match_spanning_page_boundary_remains_compressed` | `search/pagelist.rs` | **fixed, but now TIMEOUT** — see below |

The last row is the sharpest lesson in this file. Before the fix it "finished" in **8s** —
because it aborted on UB. Fixed, it runs to a 6-minute TIMEOUT. **A short runtime that ends
in UB is not a measurement of that test's cost**, and any exception table that had recorded
"8s" would have been recording an abort. (This file has made that exact mistake before: a
"~45 min" figure that was really a run aborting early on UB.) It is therefore filed under
"TIMEOUT in BOTH lanes" above, on the post-fix measurement.

### Corrections to earlier write-ups in this file

1. **"WS7 Phase E — the page-compression test family" predicted too much.** It wrote that
   "the page-level tests are dominated by `assert_integrity` and would be TRACTABLE in a
   `RUSTFLAGS=-Cdebug-assertions=off` Miri lane". Measured: **2 of the 10 are**
   (`pagelist_compress_page_round_trips_a_cold_history_page` **ok 195s**, reproducing its
   194.01s figure after WS19 T5 landed; `pagelist_mutations_mark_compression_activity` **ok
   5s**). A third page-level test, `pagelist_compress_page_rejects_an_already_compressed_node`,
   is *still* TIMEOUT(360s) with assertions off, and `pagelist_compress_page_decommit_failure_leaves_the_page_resident`
   is not rescuable at all. Right in direction, wrong in extent.
2. **The `multi-page-PageList intractable` ">20min" claim was never measured.** 82 of 87 run
   in seconds. Every one of those annotations has been rewritten with what was actually
   measured.
3. **`search::pagelist::…::compressed_history_match_spanning_page_boundary_remains_compressed`
   was carried as a plain "already excepted" row.** It was excepted *and* UB. Now fixed, and
   excepted on a real post-fix timing.

### 4-criterion basis for the 15 remaining annotations

1. **Empirically intractable / inapplicable — measured per test, in BOTH lanes.** 11 TIMEOUT
   at a 6-minute hard bound in single-worker isolation with `-C debug-assertions=off`, i.e.
   the one lever that exists has already been pulled and did not help; 4 are
   `debug_assertions`-only by construction. Never assumed from the class — that assumption is
   exactly what this pass overturned for the other 82.
2. **Passes natively** — all 15 pass in `cargo test -p rustty-vt --lib` (2159 passed / 0
   failed / 2 ignored) and in the release lane.
3. **No uncovered unsafe path.** The compress/restore seam's aliasing and lifetime properties
   are Miri-proven by `.port/phase-2/mirihar-pagelist-compress` (`split`/`siblings`/
   `roundtrip`/`leak`); `compress(Drain)`/`compress(Incremental)`/`compress_page`/
   `CompressedPage::init`/`restore`/`decommit_pool_item` were run to completion under Miri by
   WS7 Phase E (194s and 1631s, zero UB, zero leaks); encoding leak-freedom is gated natively
   and continuously by G6b; and the multi-page PageList surface these 15 shared with the other
   82 is now genuinely Miri-covered by G6c rather than merely asserted to be covered "natively
   + by ASan".
4. **Documented here with the exact per-test evidence**, and the annotations name which lane
   they apply to, so widening one back to a plain `cfg_attr(miri, ignore)` is a visible edit
   rather than a silent loss of coverage.

### Honest limits of this pass

* **The ON-lane cost of the 82 rescued tests was not re-measured.** They are annotated
  `all(miri, debug_assertions)` on the strength of the original class annotation. Some of them
  might well fit G6's budget too, which would let them run in *both* lanes. That is a pure
  optimisation, **not** a coverage gap — every one of the 82 is now Miri-covered by G6c — and
  it was not attempted because 87 tests x a 6-minute ON-lane bound is ~9 hours of interpreter
  time on a shared host.
* **The 11 kitty-graphics annotations and the `search_thread` interpreter ICE were not
  re-measured.** Neither class is assertion-bound (real-OS temp files, zlib FFI, an upstream
  rustc bug), so the G6c lever cannot apply to them; they keep their existing entries.

## WS10 T16 — the two `print_slice` STREAM tests (foreign SIMD decoder), 2026-07-30

`47e26df60`'s printable-run batching is emitted from the ground-state loop in
`Stream::next_slice_capped`, which calls
`rustty_simd::vt::utf8DecodeUntilControlSeq`. That wrapper's `SIMD` const is
`true` unconditionally in `rustty-simd` (it is not the `rustty-vt` `simd`
feature), so it always invokes the cc-built C++/highway kernel
`rustty_simd_decode_utf8_until_control_seq`. Miri reports:

```
error: unsupported operation: can't call foreign function
  `rustty_simd_decode_utf8_until_control_seq` on OS `macos`
 --> crates/rustty-simd/src/vt.rs:38:12
```

Same class as the existing kqueue / openpty / Metal / zlib entries above:
structural interpreter limitation, not a soundness finding.

| test | file | annotation |
|---|---|---|
| `stream_print_slice_matches_per_byte` | `stream.rs` | `cfg_attr(miri, ignore)` |
| `stream_print_slice_batches_printable_runs` | `stream.rs` | `cfg_attr(miri, ignore)` |

### Criterion 3 (no uncovered unsafe path) — the teeth

Every line of `unsafe` T16 adds is in `Terminal::print_slice_fill_inner`,
`print_slice_store_run` and `print_slice_store_pairs` (raw-pointer cell writes
through `cursor.page_cell`). Those are reached with NO SIMD decode at all by
`Terminal::print_slice`, and the four ported oracle tests
(`terminal_print_slice_simple_ascii`, `terminal_print_slice_wraps_and_scrolls`,
`terminal_print_slice_pending_wrap_state`, and the
`terminal_print_slice_differential_fuzz_vs_print` fuzz) drive exactly that entry
point and DO run under Miri. The two excepted tests add no unsafe of their own —
they assert the stream's RUN STRUCTURE (that batching happens at all, and where
the run boundaries fall), which is a logic property fully covered natively in
both the `simd` and non-`simd` configurations.

Owned by WS10 (OPEN).

### The differential fuzz test — a THIRD exception, different reason

`terminal_print_slice_differential_fuzz_vs_print` (`terminal.rs`) is the port of
`47e26df60`'s own `test "Terminal: printSlice differential fuzz vs print"`. It is
annotated `#[cfg_attr(miri, ignore)]` for the ordinary intractability reason, not
the FFI one:

| test | file | measured | annotation |
|---|---|---|---|
| `terminal_print_slice_differential_fuzz_vs_print` | `terminal.rs` | **>40 min, killed** (6-min bound) | `cfg_attr(miri, ignore)` |

* **Criterion 1 (intractable) — MEASURED, not assumed.** 1 700 randomized ops
  across four terminal geometries, each followed by two full-screen
  `dump_string_alloc` walks and a cursor comparison, plus two `verify_integrity`
  page walks. Native: 42s. Under Miri: still running at >40 min of interpreter
  time when killed. The three sibling tests finish in ~10s each under Miri.
* **Criterion 2 (passes natively).** Yes — `cargo test -p rustty-vt --lib
  terminal_print_slice` -> 4 passed, in both the default and `--features simd`
  lanes.
* **Criterion 3 (no uncovered unsafe path) — the teeth.** The fuzz exercises no
  unsafe code the other three do not. `terminal_print_slice_simple_ascii`,
  `terminal_print_slice_wraps_and_scrolls` and
  `terminal_print_slice_pending_wrap_state` all enter `print_slice` ->
  `print_slice_fast` -> `print_slice_fill` -> `print_slice_fill_inner` and reach
  `print_slice_store_run` (the raw-pointer cell-run store), the simple-cell scan,
  the row-advance and the pending-wrap path. Miri: `cargo +nightly miri test -p
  rustty-vt --lib terminal_print_slice` -> **3 passed, 0 failed, 1 ignored, clean,
  15.64s.** The fuzz's extra value is BEHAVIOURAL breadth (does batched output
  equal per-codepoint output across margins/modes/charsets/hyperlinks), which is
  fully delivered natively.
* **Criterion 4 (volume, not logic).** Op count x screen-dump cost, evidenced by
  the linear relationship between the `ops` argument and native runtime.

Owned by WS10 (OPEN).

## The 2026-07-31 re-audit — every exception family re-checked against its own stated control

An independent pass over the whole `MIRI EXCEPTION` inventory, run on branch
`miri-reaudit`. The question was not "is the ignore convenient" but **"does the
justification still hold, and does the compensating control it names actually
run?"** Four families were in scope. Two survive exactly as written; one was
re-measured and confirmed; one's compensating control turned out **not to exist
as a control at all**, and has been made real here.

All bounded runs used `perl -e 'alarm N; exec @ARGV'` — macOS has no coreutils
`timeout`, the trap this file has been burned by before (a `timeout`-based "Miri
measurement" that returned RC=0 after running nothing).

### Family 1 — `rustty-xev`, 30 live-syscall annotations (`kqueue`/`timer`/`stream`/`process`/`async`) — **KEEP, but the control was fiction**

**The exception is legitimate, re-verified in 2 seconds.** `cargo +nightly miri
test -p rustty-xev --lib -- --include-ignored kqueue_loop` →
``error: unsupported operation: can't call foreign function `kqueue` on OS `macos` ``.
Miri does not run these slowly; it cannot run them at all.

**The stated compensating control was not running.** All 30 reason strings said
*"runs natively + ASan (sanitizers.sh)"*.

* The **native** half is real and gate-enforced: `ci.sh` HARD step 4,
  `cargo test --workspace --no-fail-fast`.
* The **ASan** half was not. `.port/tools/gates/sanitizers.sh` is a manual
  single-crate harness. `grep -rn 'sanitizers.sh'` over the repo finds it in
  documentation and in these very reason strings — and in **no gate and no CI
  workflow**. `ci.sh` registers `check_sanitizer_canary.sh` (D6), which proves
  ASan can *see* a canary crate's heap; it never runs ASan over `rustty-xev`.
  `check_leaks.sh --list` routed `rustty-xev` to `Miri` only. So ~30 exceptions
  rested for their entire life on a control nobody had executed. (VALIDATION_REPORT
  M8 predicted exactly this shape — "miri-run.sh and sanitizers.sh are single-crate
  manual harnesses" — and it was still open.)
* **And when finally invoked, it was RED.** `sanitizers.sh rustty-xev "" address`
  crashes: `AddressSanitizer: BUS … ___chkstk_darwin` inside `pthread_create`
  from `ThreadPool::notify_slow` (`thread_pool.rs:297`). Cause: the thread-pool
  tests build a config with `stack_size: 0` (faithful to the reference; std
  clamps it to `PTHREAD_STACK_MIN`), and a 16 KB stack does not fit ASan's
  instrumented frames. An ASan/tiny-stack artifact in a test Miri already covers
  — *not* one of the excepted tests — but nobody could have known that without
  running the control.

**Fixed here, not merely reported:**

1. `check_leaks.sh` gained a **G6 ASan companion arm** (`live_ffi_unit_filters`)
   that runs the excepted families under `-Zsanitizer=address` on every G6 run,
   with a **floor derived from the source annotations** (`live_ffi_unit_floor` =
   the count of `MIRI EXCEPTION (rustty-xev …)` strings = **30**), so an
   exception added without ASan reach is a RED rather than a smaller green.
   Measured: `31 test(s) ran — test result: ok. 31 passed; 0 failed`, 0 findings.

   **A 31st annotation was added later** (2026-07-31, the xev Stacked-Borrows
   fix): `crates/rustty-xev/tests/surface_parity.rs:200`,
   `public_loop_surface_is_live` — live `kqueue(2)`, the same structural class as
   the other 30 and pre-existing red before it was annotated. `live_ffi_unit_floor`
   greps **`crates/<crate>/src` only**, so the floor stays 30 and remains
   correct; do NOT "update" it to 31, which would make the gate demand one more
   *unit* test than the crate has and turn it permanently red. The integration
   test is covered by the same ASan companion invocation, just not counted by the
   unit floor. Recorded here rather than left as a silent discrepancy between the
   annotation count (32 strings, 31 attributes + 1 comment header) and the
   documented 30.
2. `sanitizers.sh`'s green line was **vacuity-blind**: it echoed
   `grep 'test result' | tail -1`, i.e. the *Doc-tests* summary, always
   `0 passed; 0 filtered out`. A run of 31 real tests and a filter matching
   nothing printed the *same* "evidence". It now sums the `running N tests`
   headers, prints every summary, and goes RED on a zero-test run. Both arms
   demonstrated: real filter → `✅ address: 31 test(s) ran`; bogus filter →
   `❌ address: VACUOUS — 0 tests ran … a green over an empty set is not a pass`.
3. All 30 reason strings now name the gate that actually runs, not a script.

**Verdict: KEEP the 30 exceptions — Miri structurally cannot execute them — but
the control they cited is new as of this pass. Before 2026-07-31 the ASan half
of "runs natively + ASan" had never run.**

### Family 1 side-finding (OPEN, RED today): the `rustty-xev` Miri lane is failing

Running the gate — not the exceptions — surfaced this. `cargo +nightly miri test
-p rustty-xev` **fails**, reproducible in isolation:

```
thread_pool::tests::multi_worker_quiesce_then_join_50x
error: Undefined Behavior: attempting a read access using <wildcard> at alloc49568[0x18],
       but no exposed tags have suitable permission in the borrow stack for this location
   --> crates/rustty-xev/src/thread_pool.rs:1643:28   (let ctx = unsafe { &*(*job).ctx };)
```

Same shape as the 10 test-code Stacked-Borrows findings WS19 T6 unearthed: a raw
`*const LifeCtx` recovered by `offset_of!` container_of in test code, read through
a tag that is not exposed. It is **not** covered by any exception — `G6
[rustty-xev]` is simply RED, and since `ci.sh` treats a PROGRESS gate's RED as a
HARD_FAILURE, CI should be red on it. Not fixed here (outside this pass's
mandate); filed as OPEN. Unrelated to the 30 annotations above.

### Family 2 — the `G6 LANE ONLY` annotations in `render.rs` / `screen.rs` / `terminal.rs` — **KEEP, claim verified end-to-end**

The claim under audit: these are `#[cfg_attr(all(miri, debug_assertions), ignore)]`
(ignored in G6, **RUN in G6c**), and the warning "do NOT widen this to a plain
`miri` ignore" is load-bearing. All parts checked:

1. **The attribute really is lane-narrow.** All 9 sites in these files carry
   `all(miri, debug_assertions)`; none is a plain `miri`.
2. **G6c really picks them up.** `check_leaks.sh`'s `g6c_names` derivation was
   re-run: it emits **82 names from 82 annotation sites** — no drift — and all 9
   are in the set. (The derivation matches a line whose `strip()` is exactly
   `all(miri, debug_assertions),`, so a *one-line* `#[cfg_attr(...)]` spelling
   would silently escape both lanes; there is currently no such site in
   `rustty-vt/src`.)
3. **They actually run and pass**, re-measured per test under G6c's exact flags
   (`RUSTFLAGS=-C debug-assertions=off`, `MIRIFLAGS=-Zmiri-disable-isolation`,
   `--test-threads=1`), on a host visibly slower than the M4 Pro of the WS19 table:

| Test | file | this pass | WS19 T6 table |
|---|---|---|---|
| `link_cells_with_scrollback_spanning_pages` | `render.rs` | ok, **19.82s** | ok, 7s |
| `screen_cursor_absolute_to_page_with_insufficient_capacity` | `screen.rs` | ok, **53.17s** | ok, 15s |
| `screen_scrolling_when_viewport_is_pruned` | `screen.rs` | ok, **25.74s** | ok, 13s |
| `screen_cursor_set_hyperlink_oom_uri_too_large_for_string_alloc` | `screen.rs` | ok, **24.74s** | ok, 16s |
| `terminal_insert_lines_across_page_boundary_marks_all_shifted_rows_dirty` | `terminal.rs` | ok, **13.88s** | ok, 13s |
| `terminal_delete_lines_across_page_boundary_marks_all_shifted_rows_dirty` | `terminal.rs` | ok, **13.88s** | ok, 13s |
| `terminal_pagepool_no_leak_scroll_insert_multi_page` | `terminal.rs` | ok, **18.81s** | ok, 11s |
| `terminal_resize_with_high_unique_style_per_cell_with_wrapping` | `terminal.rs` | ok, **19.15s** | ok, 17s |
| `terminal_input_glitch_text` | `terminal.rs` | ok, **8.49s** | ok, 8s |

9/9 pass, 0 UB, 0 leaks. The 2–3.5x spread against the WS19 numbers is host load,
not a difference in kind.

4. **The negative control — the arm that would catch a silently-plain ignore.**
   The same test re-run with `-C debug-assertions=on` (i.e. the G6 lane):
   `test result: ok. 0 passed; 0 failed; 1 ignored`. The partition is therefore
   real in both directions: ignored with assertions ON, executed with them OFF.
   A plain `cfg_attr(miri, ignore)` would have produced `1 ignored` in *both*
   runs — exactly the loss the comment warns about, and it is not happening.

**Verdict: KEEP, and the "it RUNS in G6c" claim is true — measured, not asserted.**

### Family 3 — the two hard TIMEOUTs (`screen.rs:14857`, `terminal.rs:15740`) — **KEEP, re-measured**

Re-measured exactly as the annotations claim: single-worker isolation,
`-C debug-assertions=off`, `--include-ignored` (without it a `cfg_attr(miri,
ignore)` test reports `1 ignored` in ~0s, and a careless harness records that as
a pass), 6-minute hard bound via `perl alarm`:

| Test | file | result |
|---|---|---|
| `screen_set_attribute_splits_page_on_out_of_space_at_max_styles` | `screen.rs` | **TIMEOUT at 361s** (rc=142/SIGALRM), no libtest summary |
| `terminal_pagepool_no_leak_resize_grow` | `terminal.rs` | **TIMEOUT at 360s** (rc=142/SIGALRM), no libtest summary |

Both logs show the tests genuinely entered and still interpreting when killed
(`Screen::init` → `PageList::init_pages` frames in Miri's provenance warnings) —
not an early abort, and not a "0-second success". The annotations' wording ("the
one lever that exists has already been pulled and did not help") is accurate.

**Verdict: KEEP both. The 360s figures in the WS19 T6 table reproduce.**

### Family 4 — `rustty-extra` `webgen`/`mdgen` — **KEEP, but they were absent from this file**

`crates/rustty-extra/src/webgen.rs` (1) and `mdgen.rs` (2) carry bare
`#[cfg_attr(miri, ignore)]` with the justification only in a doc comment. This
file is the exception registry and had **no entry for any of them** — an
inventory gap, not a bad exception. Registered now:

| Test | file | native | Miri |
|---|---|---|---|
| `config_mdx_frontmatter_and_grouping` | `webgen.rs` | **0.03s** (re-measured 2026-07-31) | **no completion at a 600s bound** (re-measured; the log ends mid-`running 1 test`. Previously recorded as no completion in 1500s) |
| `gen_config_covers_every_field_and_honors_the_cli_flag` | `mdgen.rs` | ms | no completion in 71 min (Linux) / 53 min (macOS) |
| `gen_actions_covers_every_action_with_the_right_sigil` | `mdgen.rs` | ms | as above |

4-criterion basis: (1) intractable, measured in both directions; (2) passes
natively in `cargo test --workspace`; (3) **no `unsafe` anywhere in the module** —
these are `String`/`Vec` formatting over `rustty-config` metadata, so Miri has no
UB surface here the native run does not already cover; (4) the cost is
structural — `s.matches(&format!("\n## `{name}`\n")).count()` per public config
field (206 of them) over the whole rendered document, i.e. O(fields x len)
substring scanning: trivial natively, brutal byte-by-byte under an interpreter.
`rustty-extra` is `PORT STATUS: done`, so these ignores are load-bearing for its
G6 lane and belong here.

**Verdict: KEEP all three; documentation gap closed.**

**Family 4 addendum, 2026-07-31 (CI triage).** A FOURTH test in this family was
un-annotated and is why `rustty-extra`'s G6 Miri lane never terminated in the
first full `ci.sh` run on merged `main`:
`crates/rustty-extra/tests/w9_doc_rewrites_reach_generated_artifacts.rs::background_blur_and_keybind_doc_rewrites_reach_man5_and_webdata`
(P3-W9 T2). It calls `mdgen::man_5_markdown()` **and** `webgen::config_mdx()` —
both generators in the table above — then substring-scans both documents for 13
sentences. Measured: milliseconds natively (it runs in ci.sh's
`cargo test --workspace`), **no completion after 30+ minutes** under Miri in the
ci.sh lane, sitting in `running 1 test` exactly like `config_mdx_frontmatter_and_grouping`.
Same 4-criterion basis as the three above (no `unsafe` in the module; the cost is
the same O(fields x len) scanning). Now annotated with the same
`#[cfg_attr(miri, ignore = "MIRI EXCEPTION …")]` form.

### Two smaller things this pass checked

* **Env-var test selection — checked, and clean.** Miri does not forward host env
  vars, so any env-var-driven test selection is silently vacuous under it.
  `miri-run.sh` selects with `--exact <name>` on argv; `check_leaks.sh`'s G6c
  passes its derived names as libtest positional filters. Neither depends on an
  env var reaching the test binary (`G6C_FILTERS`/`G6C_MIN_PASSED` are *shell*
  overrides consumed by the gate before launch, not by the interpreted test).
  `miri-run.sh` also does not pass `--include-ignored`, so an already-annotated
  test measured through it lands in `FAIL` (no `1 passed`) rather than being
  recorded as a fast pass — no vacuity there either.
* **Redundant attributes.** The four "needs `debug_assertions`" tests carry both
  `cfg_attr(miri, ignore = …)` and `cfg_attr(not(debug_assertions), ignore)`;
  rustc emits `warning: unused attribute` for the second when both apply
  (`search/pagelist.rs:870`, `search/screen.rs:2721`, `search/screen.rs:2793`, …).
  Harmless to coverage, noisy in every Miri log. Left alone deliberately —
  dropping either changes which lane the test is excluded from.

---

## The 2026-07-31 ADVERSARIAL re-audit — the four families re-attacked

A second, deliberately hostile pass over the four families the re-audit above
declared "all KEEP", run on branch `miri-adversarial`. The brief was to try to
BREAK each verdict, treating a justification written once and never re-tested as
a hypothesis rather than a fact. Two families held, two were partially wrong.

**Method note that invalidates part of the earlier pass.** Every bounded run in
this file (and the pass above) used `perl -e 'alarm N; exec @ARGV' -- cargo
+nightly miri test …`. `alarm` kills **cargo**, not the `miri` child it spawned.
The child is reparented to PID 1 and **keeps running at 100% CPU**, and it still
writes its `test result:` summary into the same log when it eventually finishes.
Two consequences, both observed here:

1. A run recorded as "TIMEOUT, no libtest summary" can acquire a summary
   minutes-to-hours later. `terminal_input_glitch_text`, recorded by this pass as
   `rc=142 TIMEOUT at 900s`, has `test result: ok … finished in 3928.86s` in its
   own log.
2. Orphans from earlier bounded runs silently steal CPU from later ones. Two
   99%-CPU orphans were found alive here and had been corrupting the timings of
   every subsequent measurement.

`.port/tools/miri-run.sh` has a `killtree` for exactly this reason; ad-hoc `perl
alarm` measurements do not. **Any future bounded Miri measurement must kill the
process tree, and must re-read the log after the bound before recording
"TIMEOUT".**

### Family 1 — `rustty-xev`, 30 live-syscall annotations — **PARTIALLY WRONG (29 KEEP, 1 REMOVED)**

The structural claim was tested per test rather than per class: all 30 were run
individually with `cargo +nightly miri test -p rustty-xev --lib -- --include-ignored
--exact <name> --test-threads=1`, and the log grepped for `can't call foreign function`.

* **29 of 30 hold**, and hold instantly: each aborts in ≤1s with
  `error: unsupported operation: can't call foreign function {kqueue,mach_port_allocate,
  posix_spawn,openpty,…}`. Miri does not run these slowly; it cannot start them.
  The 31st annotation (`tests/surface_parity.rs`, `public_loop_surface_is_live`)
  was checked separately and also aborts on `kqueue`.
* **1 is INVALID and has been REMOVED**: `kqueue::tests::kqueue_file_io_on_thread_pool`
  (`crates/rustty-xev/src/kqueue.rs`). Its reason string claimed "live
  kqueue/kevent64/mach/socket syscalls -- Miri cannot execute foreign functions".
  **The test makes no syscall at all.** While `MACOS_FILE_WATCHER_PORTED == false`
  its entire body is an `include_str!` of `DEFERRALS.md` plus a `contains()`
  assertion and an early `return` — a deferral-ledger skip-stub. It runs **clean
  under Miri in 23.87s**. The annotation had been applied by file/module, not by
  body — exactly the sweep-in failure mode the class argument invites.
  Verified after removal, without `--include-ignored`:
  `test kqueue::tests::kqueue_file_io_on_thread_pool ... ok` /
  `1 passed; 0 failed; 0 ignored`, with the negative control on a still-annotated
  sibling (`kqueue_mach_port`) printing `0 passed; 1 ignored` from the same build.

Gate impact: `live_ffi_unit_floor` is source-derived and drops 30 → 29 on its
own; the ASan companion still runs 31 module-prefix-matched tests, so the arm
stays green. No hand-edit of the floor is needed (and must not be made).

### Family 2 — the `G6 LANE ONLY` partition — **ATTACKED AND HELD**

The verdict was not re-litigated (the earlier pass proved the tests run in G6c).
What was attacked is the *causal* story: that `assert_integrity` is the dominant
cost, rather than that being a narrative fitted to the outcome. Tested directly
by stubbing `Page::assert_integrity`, `PageList::assert_integrity` and
`Screen::assert_integrity` behind a temporary const while leaving
`debug_assertions` **ON** (so every other `debug_assert!` still ran), in a fresh
`CARGO_TARGET_DIR` so no stale artifact could be reused:

| test | assertions ON, unmodified | assertions ON, integrity stubbed | assertions OFF (G6c) |
|---|---|---|---|
| `terminal_input_glitch_text` | **3928.86s** | **3.87s** | 8.49s (prior pass) |
| `screen_cursor_absolute_to_page_with_insufficient_capacity` | >900s (orphan killed) | **24.04s** | 53.17s (prior pass) |

Stubbing the integrity checks **alone** rescues the tests — a **1015x** reduction
on the first. The stated cause is correct, not fitted. The `all(miri,
debug_assertions)` partition is the right lever and the "do NOT widen this to a
plain `miri` ignore" warning is load-bearing.

*Honest limit: 2 of the 9 sites were measured this way, not all 9.*

### Family 3 — `screen.rs:14857` — **PARTIALLY WRONG (KEEP stands; "TIMEOUT" and the stated cause do not)**

The question posed was whether "361s at a 6-minute bound" meant *genuinely slow*
or *hung/livelocked*. Answered definitively: **genuinely slow, and it completes
clean.**

Run with no practical bound, `-C debug-assertions=off`, single worker,
`--include-ignored`, essentially alone on the host:

```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2320 filtered out; finished in 1656.89s
```

**27.6 minutes, 0 UB, 0 leaks.** An independent earlier run of the same test
(under 3-way CPU contention) also completed, at 1977.51s. There is no hang: every
loop in the test is counter-bounded and monotone, the test is single-threaded,
and an instrumented run shows uninterrupted forward progress the whole way.

**The stated cause in the annotation was wrong.** WS19 T6 wrote "the cost is
page-mapping/LZ4 volume". This test never invokes LZ4. Instrumented breakdown of
the 1656.89s:

| phase | time | share |
|---|---|---|
| `increase_capacity` doubling 128 → 65535 styles (10 iterations, the "page-mapping" phase) | 13.0s | 0.8% |
| filling the `RefCountedSet` style map with 53248 entries | 1650.7s | 99.6% |

The per-iteration curve is smooth and finite (0.77s, 0.78s, 0.80s, 0.85s, 0.94s,
1.11s, 1.48s, 2.23s, 3.89s for the doublings; the fill loop degrades gently from
~3s to ~20s per 200 inserts as the set's load factor rises).

**Verdict: KEEP** — 27.6 min is far past the file's own 20-min tractability
criterion and past any lane budget — **but the annotation must say "too slow,
completes in ~28 min" and must not say "LZ4"**. Corrected in place at
`crates/rustty-vt/src/screen.rs`.

*Honest limit: only `screen.rs:14857` was run to completion. `terminal.rs:15740`
(`terminal_pagepool_no_leak_resize_grow`) was NOT re-run here and its
"slow vs hung" question remains formally open, though the same reasoning applies.*

### Family 4 — the self-declared-unmeasured compression exceptions — **OVERTURNED (2 of 4 REMOVED)**

First, a factual correction to the premise this family is usually stated with:
there are **not ~20** annotated-but-unmeasured compression tests. The
`pagelist_incremental_compression_*` family (10 tests) carries **no Miri ignore at
all** and is already fully Miri-gated. Exactly **4** annotations in the tree carry
the self-incriminating string *"NOT individually re-measured by this lane"*. All
four were measured — not a sample — with debug assertions **ON** (the strict G6
lane), 600s bound:

| test | measured | verdict |
|---|---|---|
| `pagelist_memory_stats_include_unused_pool_backing` | **2.05s** (re-verified 1.69s) | **REMOVED** |
| `pagelist_does_not_compress_the_mixed_history_and_active_page` | **1.57s** (re-verified 1.26s) | **REMOVED** |
| `pagelist_memory_stats_do_not_restore_compressed_pages` | TIMEOUT at 600s | KEEP |
| `pagelist_full_and_incremental_compression_skip_a_spanning_viewport` | TIMEOUT at 600s | KEEP |

The two removals are **three orders of magnitude** inside budget. Their reason
string blamed "LZ4 page-compression volume"; one compresses a single 1x1 pool
page, and the other exists precisely to assert that a mixed history/active page
is **not** compressed — i.e. it was excepted for the one code path it never
takes. Both now run in the ordinary G6 lane: verified without
`--include-ignored`, `1 passed; 0 failed; 0 ignored`.

The two that did reproduce as TIMEOUTs have had their reason strings rewritten to
record the measurement and to warn against re-applying the string by class
inference.

**This is the same defect as Family 1, in a different crate: an exception
inherited from a class rather than established from a body.** Two of four
annotations written on assumption were wrong, which is the base rate this file
should assume for any future "same class as" reason string.

### What this pass did NOT test

* `terminal.rs:15740` slow-vs-hung (see Family 3).
* 7 of the 9 Family-2 sites (only 2 had the stub experiment run against them).
* Whether the ASan companion arm added by the previous pass is itself
  sound — it was taken at face value here.
* The `rustty-extra` `webgen`/`mdgen` family (Family 4 of the previous pass) was
  out of scope for this one.
* The OPEN `rustty-xev` `thread_pool.rs` Stacked-Borrows failure is being handled
  by a concurrent lane and was deliberately not touched.

---

## rustty-cli `boo` framedata (vendored zlib FFI) — added 2026-07-31 (CI triage)

`crates/rustty-cli/src/boo.rs::tests::decompress_frames_round_trip` —
**Miri EXCEPTED (vendored zlib FFI).** Same class as
`crates/rustty-zigimg/tests/codec.rs` above.

Found by the first full `ci.sh` run on merged `main`: the G6 Miri lane for
`rustty-cli` died on `error: unsupported operation: can't call foreign function
`inflateInit2_` on OS `macos`` (`zlib-sys/src/lib.rs:291`, via
`boo::decompress_frames` at `boo.rs:505`). Miri aborts the whole test binary at
the first foreign call, so this one test was taking the crate's entire lib
surface down with it.

The four criteria, in the form this file uses for the FFI class:

1. **Miri cannot run it at all** — not slow, *impossible*: `inflate_raw` enters
   the vendored zlib 1.3.1 C. Re-verified 2026-07-31 from the ci.sh G6 log.
2. **Passes natively** — `cargo test -p rustty-cli boo::` is green in ci.sh's
   HARD `cargo test --workspace` step, which runs this test for real.
3. **No uncovered unsafe path** — the excepted test contains no `unsafe`; the
   `unsafe` it reaches lives in `zlib-sys::inflate_impl`, whose FFI-boundary
   memory safety is an ASan question, not a Miri one (`zlib-sys` is a
   `crates-sys` crate and is routed to the ASan lane by `check_leaks.sh`'s
   `is_ffi()` `*-sys` rule). Everything `boo.rs` owns — the frame splitter,
   `update_frame`'s span parser, the cell buffer — is exercised by the other
   `boo` tests, which stay Miri-live.
4. **A stand-in would be vacuous, not equivalent** — the deliberate contrast
   with the `cfg(miri)` accommodations in `rustty-os/src/macos.rs` (os_log /
   signpost), landed in the same triage: those stub a *sink* nothing asserts on,
   so the ported Rust keeps running under Miri. Here the C IS the assertion
   (`decompress_frames` must reproduce the asset byte-faithfully), so stubbing
   it would leave a test that proves nothing. Excepting it is honest; stubbing
   it would not be.

## rustty-bench — the 2026-07-31 CI triage (3 tests)

The first full `ci.sh` run on merged `main` had `rustty-bench`'s G6 Miri lane
dying on `dladdr` before a single test finished. Two distinct causes, fixed two
different ways — the split is the point:

**Not excepted: the `os_signpost` instrumentation.** `Benchmark::run` opens an
Instruments signpost interval on Darwin (faithful to `Benchmark.zig:44-66`), so
`macos::signpost::init` -> `dladdr` ran in EVERY bench test. Nothing asserts on
signposts; they are a sink. That is now a `cfg(miri)` no-op arm in
`crates/rustty-os/src/macos.rs` (alongside the same treatment for the `os_log`
sink), so all 19 bench tests stay in the Miri lane instead of 19 exceptions
being written. `cfg(miri)`-only, `not(miri)` keeps the real libSystem calls, and
the precedent is `rustty-xev`'s `cfg(miri)` futex stand-ins and
`rustty-os`'s `cfg(miri)` page allocator.

**Excepted: the 3 tests that genuinely cross a boundary Miri cannot cross.**
Measured per test, each in its own Miri process (`.port/phase-3/citriage-bench-miri-probe.sh`
— Miri aborts the whole binary at the first offence, so a lane run only ever
names one). 16 of 19 are Miri-GREEN and stay that way; these 3 are not:

| Test | class | measured |
|---|---|---|
| `scrollback_compression::scrollback_compression_restores_cold_terminal_pages` | SIMD FFI | `can't call foreign function `rustty_simd_decode_utf8_until_control_seq`` (highway C++, via `Stream::next_slice`) |
| `scrollback_compression::scrollback_compression_drains_incremental_compression_steps` | SIMD FFI | as above |
| `terminal_stream::terminal_stream_uses_the_full_terminal_handler` | file I/O | `` `open` not available when isolation is enabled `` (`File::create` of its corpus) |

4-criterion basis. (1) **Impossible, not slow** — a foreign call and a forbidden
syscall; no budget makes either work. (2) **Pass natively** — all three run in
ci.sh's HARD `cargo test --workspace`. (3) **No uncovered unsafe path** — the
`unsafe` these reach is `rustty-simd`'s FFI wrapper (a `crates-sys` consumer,
routed to the ASan lane by `check_leaks.sh`'s `is_ffi()`) and `rustty-vt`'s
page/cell writes, which are Miri-covered by the vt lane's own tests; the exact
same pairing already justifies `rustty-vt::stream_print_slice_matches_per_byte`.
(4) **The remaining 16 bench tests are Miri-live**, including every parser
benchmark — the excepted set is "the tests that cross into C or the filesystem",
not "the tests that cover the crate".

## rustty-cli / rustty-os — the 2026-07-31 CI triage (3 more tests)

Both crates' G6 Miri lanes were dying on their first foreign call, taking their
whole lib binary with them. Each crate was probed test-by-test in its own Miri
process (`.port/phase-3/citriage-bench-miri-probe.sh`) so the excepted set is the
measured minimum, not a crate-wide retreat: **69 of 70 rustty-cli tests and 63 of
65 rustty-os tests are Miri-GREEN and stay in the lane.**

| Test | class | measured |
|---|---|---|
| `rustty-cli` `boo::tests::decompress_frames_round_trip` | vendored zlib FFI | `can't call foreign function `inflateInit2_`` (see the dedicated section above) |
| `rustty-cli` `version::tests::commit_hash_is_captured_in_a_git_checkout` | process/file I/O | `` `open` not available when isolation is enabled `` — it runs `git rev-parse` to decide whether it is in a checkout at all. **2026-09-23 (review T-B):** replaced by `commit_hash_is_the_version_build_metadata`, which spawns nothing and is back in the Miri lane; the git probe moved to `rustty-version` `tests::version_is_derived_from_git`, which carries this exception now (same class, same `cfg_attr(miri, ignore)`, same 4-criterion basis) |
| `rustty-os` `autorelease::tests::{returns_closure_value, pool_drains_autoreleased_references}` | Objective-C runtime | `can't call foreign function `objc_autoreleasePoolPush`` / `objc_getClass` |

4-criterion basis. (1) **Impossible, not slow.** (2) **Pass natively** in ci.sh's
HARD `cargo test --workspace`. (3) **No uncovered unsafe path** — the `version`
test contains no `unsafe` at all (it shells out and compares a string); the
`autorelease` pair's `unsafe` is `msg_send!` into the ObjC runtime, which is an
ASan/native question by construction, and the Rust side of the shim (`f()`
pass-through on non-Darwin) has no memory surface. (4) **The alternative is
vacuity**: dropping `version`'s `git rev-parse` probe re-opens the exact hole WS19
T12 closed (a `commit_hash()` that returns `None` unconditionally would pass), and
an autorelease pool that is stubbed out cannot demonstrate that it drains.
---

## G6 lane completion (2026-08-29) — the crates the earlier triages never reached

`.port/phase-3/miri/FINDINGS.md` closed its G6 section with "crates after
`rustty-extra` in the lane order were never reached by any run in this triage and
remain unverified", and `rustty-core` with "no verdict yet". This is that
frontier, worked to a verdict. Miri aborts a whole test binary at the FIRST
error, so each crate had to be re-run after every annotation to surface what was
hiding behind it — the counts below are from the final green run of each lane.

Two findings were REAL and are fixed in code, not excepted (written up in
`.port/phase-3/miri/FINDINGS.md`): a Stacked-Borrows violation in
`rustty-args`/`rustty-config`'s diagnostic C pointer, and a `const`-address
assertion in `rustty-lib`. Everything below is a genuine
Miri-cannot-model case.

| crate | test(s) | class | final lane |
|---|---|---|---|
| `rustty-cli` | `ssh_cache::…run_inner_fails_cleanly_when_the_cache_is_unusable` | `mkdir` under isolation | 55 passed, 17 ignored |
| `rustty-core` | `app::test_runtime::app_create_and_destroy_empty` | CoreText warmup thread → `CFStringCreateWithBytes` | see below |
| `rustty-global` | `environ::…global_environ_map_reflects_the_process`, `rlimit_tests::restore_round_trips_nofile` | empty environ under isolation; `getrlimit` | 8 passed, 2 ignored |
| `rustty-vaxis` | `recover_tests::{recover_writes_exact_reset_bundle_to_stored_fd, recover_restores_saved_termios_on_a_pty, install_panic_recovery_runs_recover_then_chains_previous_hook}` | real pty + `tcsetattr` | 98 passed, 3 ignored |
| `rustty-vt` | `hyperlink::…try_dupe_bytes_reports_allocation_failure` | Miri hard-errors instead of failing an allocation | — |
| `rustty-vt-c` | `kitty_graphics::…image_compression_and_format_always_report_decoded_data` | vendored zlib decoder (Wuffs since P7-W5 T2; was zlib `inflateInit2_`) | — |
| `rustty-z2d` | `path_fx::offset::tests::offset_e2e_{inset,outset}`, `tess::stroke_plotter::…slope_difference_below_epsilon_does_not_produce_nan` | **Miri's libm is not bit-faithful** | 81 passed, 13 ignored |

Each is annotated at its source with the reason, the convention this file already
uses. The three interesting ones:

### `rustty-global` — real process state

Miri's isolation gives the interpreted process an **empty environment**, so
`std::env::var_os("PATH")` is `None` and the test's premise cannot hold. Note the
failure mode if this were "fixed" by softening the assertion to "compare whatever
is set": both sides would be empty and the test would pass while asserting
nothing — a vacuous green, which is worse than an honest exception. `getrlimit`
is the ordinary foreign-function case; the value under test IS the live process
rlimit, so there is nothing for Miri to model.

### `rustty-z2d` — Miri's libm differs from the host's, and it propagates

Not a tolerance question and not randomness. Measured with a standalone probe
(`cargo miri run` vs `cargo run`, same source):

| call | native (host libm) | Miri |
|---|---|---|
| `hypot(3, 4)` | `0x4014000000000000` (exactly 5.0) | `0x4014000000000001` |
| `hypot(1, 2)` | `0x4001e3779b97f4a8` | `0x4001e3779b97f4a4` |
| `hypot(0.1, 0.7)` | `0x3fe6a09e667f3bcc` | `0x3fe6a09e667f3bc8` |
| `sqrt` of the same sums | identical | identical |

`sqrt` agrees bit-for-bit because IEEE-754 requires it to be correctly rounded
and Miri implements it exactly; `hypot` carries no such requirement, and Miri's
differs by 1-4 ULP. It is stable, not random — identical across repeated runs and
across `-Zmiri-seed` values, so it is a different implementation, not injected
noise.

`Slope::normalize` (`slope.rs:243`) uses `hypot`, so the error reaches every
derived coordinate. Two consequences:

1. the `offset_e2e_*` goldens are z2d 0.12.1's own recorded node sets and are
   compared to the last bit, so they mismatch;
2. `slope_difference_below_epsilon_does_not_produce_nan` goes further and
   PANICS. Probed at `Face::intersect`, under Miri the two normalized slopes
   arrive 2 ULP apart — `in=(bfeda7d91c1e082a, 3fd80b8d70be677c)`,
   `out=(bfeda7d91c1e082c, 3fd80b8d70be677e)` — and the miter determinant
   `in.dx*out.dy - out.dx*in.dy` rounds to **exactly 0**, so the division is
   0/0 = NaN. The same probe never fires natively.

Loosening the goldens to a tolerance would give up the last-bit oracle agreement
they exist to pin, which is the wrong trade. None of the three tests touches any
`unsafe` — `grep -c unsafe` over all five `path_fx/*.rs` is 0 — so removing them
from the Miri lane forfeits no UB coverage at all.

The latent fragility (2) exposes is recorded in `.port/phase-3/miri/FINDINGS.md`:
`Face::intersect` guards on `Slope::compare(..) != 0`, which is not the same
predicate as "the determinant is non-zero". It is NOT fixed here because the
miter math is z2d 0.12.1's, itself Cairo's `cairo-path-stroke-polygon.c`, and
changing its numerics would be a deliberate divergence from the vendored
reference (R1).

### `rustty-vt` — Miri cannot fail an allocation

`try_dupe_bytes_reports_allocation_failure` exists to prove a real allocator
failure REPORTS rather than aborts. Miri does not model a failing allocation: an
impossible request is an interpreter-level hard error ("resource exhaustion"),
which kills the binary before the `is_err()` is evaluated. There is no
Miri-compatible variant — any request small enough for Miri to serve succeeds,
and the assertion becomes vacuous.

### `rustty-vt` — `pagelist_integrity_detects_max_lines_exceeded`: no G6c escape valve exists

Task 14b's review flagged this test (pre-existing; 14b only added a
`log_capture` assertion to its already-passing body, no change to its
fixture) as timing out under Miri (measured: still running past 180s single-
worker isolation, `ub=0` the whole time — no UB signal, purely slow). Unlike
the `WS19 T6` G6c-eligible family documented above (the page-compression
tests), this one has **no G6c escape valve**: it is already
`#[cfg_attr(not(debug_assertions), ignore)]`, because `verify_integrity` is a
no-op without `debug_assertions` and the test's own assertions
(`Err(IntegrityError::MaxLinesExceeded)`) would be vacuous under
`-C debug-assertions=off` — confirmed by directly running it that way
(`RUSTFLAGS="-C debug-assertions=off" cargo +nightly miri test ...`), which
fails with `left: Err(MaxLinesExceeded) right: Ok(())`. So the `all(miri,
debug_assertions)` / G6c pairing the WS19 T6 annotation text promises
("It RUNS there") would be **false** for this test; that exact wording must
not be copied onto it.

**Root cause, isolated separately** (confirmed on the sibling row
`pagelist_integrity_detects_viewport_pin_not_in_list`, added in the same
14b change and originally authored with the identical shape — `grow_rows`
called in a loop up to hundreds of rows before the assertion under test):
`grow_rows` runs a page-level `assert_integrity` **per row** under
`debug_assertions`, each call O(current page size), making an n-row single-
page buildup O(n²) under Miri's per-instruction interpretation — independent
of the page's byte CAPACITY (a wide-`cols` page with a tiny row-capacity
floor was tried and made no difference, because a wide `cols` inflates
per-row cell-scan cost by the same factor it shrinks row count). The fix
applied to the sibling test was to stop building the second page via a
`grow_rows` loop and instead splice in an already-existing crate pattern —
one `create_page` call plus direct `pages.append`/`total_rows` bookkeeping
under `pause_integrity_checks` (the same technique `max_lines_exceeded`
itself already uses for its OWN second page) — which cut it from a
still-running-past-4-minutes background run to a clean, single-worker
**30.54s pass, 0 UB, 0 leaks**.

`max_lines_exceeded`'s SPECIFIC 594-row buildup (`s.grow_rows(page_rows)`,
`page_rows = PageList::initial_capacity(cols).rows`) exists to fill the
oldest page to EXACTLY standard capacity before the second (already
`create_page`-spliced) page is appended — unlike the sibling test, changing
its fixture to avoid the large `grow_rows` call would require either
faking a "this page is full" state without actually filling it (risking a
false pass that no longer represents "one complete historical page", the
scenario `IntegrityError::MaxLinesExceeded` specifically requires) or
restructuring what the test proves. That fixture change was judged out of
this stage's scope (14b's task is restoring LOG-class diagnostics, not
rewriting a pre-existing PageList test's construction strategy) and is left
for a future stage.

**Disposition:** `#[cfg_attr(all(miri, debug_assertions), ignore = "MIRI
EXCEPTION — NO G6c ESCAPE (this file, 'no G6c escape valve exists' above):
unlike the WS19 T6 compression family, this test is ALSO
`#[cfg_attr(not(debug_assertions), ignore)]` — it needs `debug_assertions`
ON to assert anything non-vacuous, so it cannot run in the
`-C debug-assertions=off` G6c lane either. Confirmed slow-not-UB: multiple
single-worker runs sustained `ub=0` past 180-240s before being stopped, no
Miri-compatible fixture found within the 14b time budget (see this file's
longer note). This is a genuine gap in Miri coverage for this one test, not
a silent skip — the same fixture change that fixed the sibling
`viewport_pin_not_in_list` test is the documented path to closing it."]`
added to `pagelist_integrity_detects_max_lines_exceeded`
(`crates/rustty-vt/src/pagelist/tests.rs`). It still runs, and is required
to pass, natively (`cargo test -p rustty-vt --lib`) and in the release lane.

**RESOLVED (parity tail C-1, 2026-09-27): the exception is removed; the test runs in G6.**
The fixture now pauses the first page's own integrity checks while `grow_rows`
fills it (`Page::pause_integrity_checks`, the technique the reference's grow-loop
tests use, e.g. `pagelist_pointFromPin_active_from_prior_page`), then un-pauses and
asserts `Page::verify_integrity` and `PageList::verify_integrity` on the complete
page before the forbidden state is built. The page is still filled row by row by
`grow_rows`, so "one complete historical page" is the real state, not a fake; only
the O(n^2) per-row re-check during setup is skipped. Measured under the G6
conditions (isolation on, debug assertions on, single test): **pass, 49 s (Miri
clock), 0 UB, 0 leaks**. With the `all(miri, debug_assertions)` annotation gone it
also leaves G6c's derived set, which it had been counted in since `bbc76bc9`
although `cfg_attr(not(debug_assertions), ignore)` kept it from ever running there:
that is the `83 test(s) RAN, expected at least 84` G6c red. The set is now 83
tests, all of which run.

## Parity tail C-1 — the G6 triage of 2026-09-27 (tests added since the 2026-08-29 lane)

The full `ci.sh` run at `9fd17d93` had G6 red in nine crates. Miri aborts a whole test binary at the
first offence and cargo stops at the first failing binary, so the lane only ever named one test per
crate. Task C-1 therefore ran EVERY test of the nine crates in its own Miri process under the G6
conditions (isolation on, no `MIRIFLAGS`, debug assertions on, 20-minute bound per test), then
re-ran each crate's lane exactly as `check_leaks.sh` does. Nearly all were added by the parity-closure
waves, whose briefs ran no Miri.

Real defects found this way are FIXED in code, not excepted: N391 (`rustty-bench` terminal-stream
Stacked Borrows), and the others listed under their crates below. Everything in the tables is a
genuine Miri-cannot-model case, annotated at its source `#[cfg_attr(miri, ignore = "MIRI EXCEPTION
(<crate>, parity tail C-1): ...")]`.

4-criterion basis, for every row: (1) **impossible, not slow** — a syscall Miri's isolation forbids or
a foreign function Miri cannot execute; each was measured, one test per Miri process, and failed at
that call; (2) **passes natively** in ci.sh's HARD `cargo test --workspace` (and the release lane),
which is the companion lane; (3) **no uncovered unsafe path** — the calls are the OS boundary itself
(process spawning, files, directories, clocks, `fcntl`, `select`, kqueue, Objective-C/CoreFoundation),
and the crate's own Rust code on either side of them is exercised by its Miri-passing tests; (4) the
rest of each crate stays in the lane (counts below, from the lane re-run).

### `rustty-bench` (lane: 37 passed, 12 ignored), `rustty-cli` (88 passed, 35 ignored), `rustty-global` (13 passed, 7 ignored), `rustty-synthetic` (26 passed, 1 ignored)

| crate | test | what Miri cannot do |
|---|---|---|
| `rustty-bench` | `cli::tests::string_args_honour_quotes_like_arg_iterator_general` | `mkdir` is forbidden under Miri isolation |
| `rustty-bench` | `global_init_runs_before_the_benchmark` | `open` is forbidden under Miri isolation |
| `rustty-bench` | `page_compression_report_goes_to_stderr` | `open` is forbidden under Miri isolation |
| `rustty-bench` | `page_compression_report_prints_to_stderr_not_stdout` | `clock_gettime(REALTIME)` is forbidden under Miri isolation |
| `rustty-bench` | `scrollback_compression_report_goes_to_stderr` | `open` is forbidden under Miri isolation |
| `rustty-bench` | `terminal_formatter_report_goes_to_stderr` | `open` is forbidden under Miri isolation |
| `rustty-bench` | `terminal_formatter_roundtrip_goes_to_stderr` | `open` is forbidden under Miri isolation |
| `rustty-bench` | `terminal_snapshot_report_goes_to_stderr` | `open` is forbidden under Miri isolation |
| `rustty-cli` | `crash_report::tests::a_report_too_old_for_a_duration_saturates_instead_of_trapping` | `clock_gettime(REALTIME)` is forbidden under Miri isolation |
| `rustty-cli` | `edit_config::exec_tests::exec_failure_report_prints_the_raw_command_bytes` | calls the foreign function `execve`, which Miri cannot execute |
| `rustty-cli` | `edit_config::exec_tests::the_editor_inherits_the_ignored_sigpipe` | `mkdir` is forbidden under Miri isolation |
| `rustty-cli` | `io_file::tests::a_write_to_a_stdout_closed_at_exec_fails_with_ebadf` | `current_exe` (`_NSGetExecutablePath`) is forbidden under Miri isolation |
| `rustty-cli` | `io_file::tests::a_write_to_an_unwritable_stdout_fails_where_std_reports_success` | `open` is forbidden under Miri isolation |
| `rustty-cli` | `io_file::tests::an_appended_stdout_keeps_the_existing_content` | `mkdir` is forbidden under Miri isolation |
| `rustty-cli` | `list_colors::r13_flush_tests::plain_listing_reaches_stdout_in_full` | `current_exe` (`_NSGetExecutablePath`) is forbidden under Miri isolation |
| `rustty-cli` | `list_themes::picker_tests::keys_on_a_filter_that_matches_nothing_do_not_index_it` | calls the foreign function `openpty`, which Miri cannot execute |
| `rustty-cli` | `list_themes::picker_tests::the_preview_loop_delivers_a_resize` | calls the foreign function `openpty`, which Miri cannot execute |
| `rustty-cli` | `list_themes::r13_flush_tests::a_home_too_long_for_the_xdg_lookup_is_out_of_memory` | `current_exe` (`_NSGetExecutablePath`) is forbidden under Miri isolation |
| `rustty-cli` | `list_themes::r13_flush_tests::no_themes_messages_reach_stderr` | `mkdir` is forbidden under Miri isolation |
| `rustty-cli` | `ssh::tests::install_failures_are_install_failed` | `arc4random_buf` is not implemented by Miri on macOS |
| `rustty-cli` | `ssh_cache::disk_cache::tests::add_does_not_chmod_an_existing_cache_file` | `mkdir` is forbidden under Miri isolation |
| `rustty-cli` | `zig_error::tests::realpath_error_names_follow_the_reference_mapping` | `realpath` is forbidden under Miri isolation |
| `rustty-global` | `environ::tests::environ_from_process_matches_vars_os` | real process environment: Miri isolation gives an empty environ (the test would be vacuous there) |
| `rustty-global` | `get_env_map_survives_non_utf8` | `current_exe` (`_NSGetExecutablePath`) is forbidden under Miri isolation |
| `rustty-global` | `posix_duplicate_env_parent_spawns_the_child` | `current_exe` (`_NSGetExecutablePath`) is forbidden under Miri isolation |
| `rustty-synthetic` | `the_generator_binary_is_named_rustty_gen` | `open` is forbidden under Miri isolation |
| `rustty-cli` | `new_window::tests::error_parsing_args_prints_the_zig_error_name (unix)` | `realpath` is forbidden under Miri isolation |
| `rustty-cli` | `new_tab::tests::error_parsing_args_prints_the_zig_error_name` | `realpath` is forbidden under Miri isolation |

`rustty-bench`'s `log_site_census::every_reference_diagnostic_is_live_at_its_level_and_scope` is NOT
excepted: it re-collapsed a whole source file per expected entry and ran past the bound; it now
collapses each file once and passes under Miri in 232 s.

### `rustty-os` (lane `--lib`: 77 passed, 45 ignored), `rustty-core` (58 passed, 14 ignored)

| crate | test | what Miri cannot do |
|---|---|---|
| `rustty-core` | `renderer_seam::compression_tests::change_config_restarts_the_pass_only_on_disabled_to_enabled` | the harness's `Thread::init` opens a real xev event loop, whose `kqueue` Miri cannot call, and the pass is driven by that loop |
| `rustty-core` | `renderer_seam::compression_tests::compression_idle_pass_compresses_scrollback` | the harness's `Thread::init` opens a real xev event loop, whose `kqueue` Miri cannot call, and the pass is driven by that loop |
| `rustty-core` | `renderer_seam::compression_tests::compression_is_inert_when_the_option_is_disabled` | the harness's `Thread::init` opens a real xev event loop, whose `kqueue` Miri cannot call, and the pass is driven by that loop |
| `rustty-core` | `surface::r13_window_size_overflow_tests::a_click_repeat_interval_above_u32_nanoseconds_is_widened_not_trapped` | `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute |
| `rustty-core` | `surface::r13_window_size_overflow_tests::an_oversized_window_padding_saturates_and_leaves_a_one_cell_grid` | `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute |
| `rustty-core` | `surface::r13_window_size_overflow_tests::initial_size_px_overflow_takes_the_skip_path` | `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute |
| `rustty-core` | `surface::r13_window_size_overflow_tests::initial_size_px_with_a_nan_content_scale_takes_the_skip_path` | `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute |
| `rustty-core` | `surface::r13_window_size_overflow_tests::size_limit_min_overflow_skips_the_action` | `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute |
| `rustty-core` | `surface_occlusion_callback_dedups_and_reports` | calls the foreign function `CFStringCreateWithBytes`, which Miri cannot execute |
| `rustty-os` | `cf_release_thread::tests::a_clean_run_emits_the_three_debug_lines_in_the_reference_order` | calls the foreign function `kqueue`, which Miri cannot execute |
| `rustty-os` | `file::tests::alloc_tmp_dir_keeps_the_root_directory` | `stat` is forbidden under Miri isolation |
| `rustty-os` | `locale::tests::preferred_languages_are_bounded_by_the_fixed_buffer_and_nul` | calls the foreign function `_libintl_locale_name_canonicalize`, which Miri cannot execute |
| `rustty-os` | `locale::tests::the_nslocale_class_lookup_resolves` | calls the foreign function `objc_getClass`, which Miri cannot execute |
| `rustty-os` | `open::open_thread_tests::open_thread_reaps_an_opener_that_writes_stderr` | `open` is forbidden under Miri isolation |
| `rustty-os` | `open::open_thread_tests::open_thread_spawn_failure_is_an_error` | calls the foreign function `posix_spawnattr_init`, which Miri cannot execute |
| `rustty-os` | `pipe::fallback_tests::failing_setfd_closes_both_descriptors` | `fcntl(F_SETFD)` is not implemented by Miri |
| `rustty-os` | `pipe::fallback_tests::fallback_sets_cloexec_on_both_and_closes_nothing` | `fcntl(F_SETFD)` is not implemented by Miri |
| `rustty-os` | `resourcesdir::tests::candidate_longer_than_max_path_is_no_space_left` | `stat` is forbidden under Miri isolation |
| `rustty-os` | `resourcesdir::tests::non_utf8_resources_dir_variable_is_used_raw` | `stat` is forbidden under Miri isolation |
| `rustty-os` | `resourcesdir::tests::walk_finds_the_share_tree_above_the_executable` | `lstat` is forbidden under Miri isolation |
| `rustty-os` | `systemd::tests::comm_check_reads_16_bytes_and_has_two_failure_arms` | `lstat` is forbidden under Miri isolation |

The three `rustty-core` `renderer_seam::compression_tests` were the tests the ci run killed after an
hour. The hour is their setup (2,000 lines streamed through a real terminal under debug
assertions), but shrinking it would not make them Miri-tractable: the harness then calls
`Thread::init`, which opens a real xev loop (`xev::Loop::init` -> `kqueue`, `renderer_seam.rs`
`Thread::init`), and the pass under test is driven by that loop's timer. So they are excepted as
impossible, not as slow. The scheduler's own logic has no `unsafe` beyond the loop and the terminal;
the terminal's compression paths are covered by `rustty-vt`'s Miri-passing compression tests.

### `rustty-vaxis` (lane: 157 passed, 12 ignored)

| crate | test | what Miri cannot do |
|---|---|---|
| `rustty-vaxis` | `loop_::stop_wake_tests::a_terminal_that_answers_ends_the_read_without_the_wake` | calls the foreign function `select`, which Miri cannot execute |
| `rustty-vaxis` | `loop_::stop_wake_tests::a_terminal_that_never_answers_is_woken_after_the_grace` | calls the foreign function `select`, which Miri cannot execute |
| `rustty-vaxis` | `loop_::tests::loop_lifecycle` | `fcntl(F_SETFD)` is not implemented by Miri |
| `rustty-vaxis` | `tty::tests::a_sigwinch_while_the_handlers_are_locked_still_reaches_the_callback` | `fcntl(F_SETFD)` is not implemented by Miri |
| `rustty-vaxis` | `tty::tests::the_sigwinch_handler_neither_blocks_on_a_full_pipe_nor_clobbers_errno` | `fcntl(F_SETFD)` is not implemented by Miri |

Not excepted, FIXED (NEW-FINDINGS N392): the `tests/image_tier.rs` binary did not COMPILE under Miri
(`png_fixture` was `#[cfg(not(miri))]` while its two callers are only `cfg_attr(miri, ignore)`), so
the lane stopped there and the binary's other 19 tests never ran under Miri. Once it compiled,
`img_draw_expect` took `&mut Screen` alongside a `Window` holding a raw pointer to the same screen,
a Stacked Borrows violation in the test helper; it now reads back through the window's pointer.

### `rustty-vt-c` (lane: 540 passed, 17 ignored)

Isolation / foreign-call exceptions (same 4-criterion basis as above):

| test | what Miri cannot do |
|---|---|
| `tests/header_scrollback_limit_docs.rs` `scrollback_max_bytes_doc_matches_oracle`, `scrollback_max_lines_doc_matches_oracle` | `open` is forbidden under isolation (they read the shipped header) |
| `panic::tests::tiny_panic_prints_only_the_message_and_aborts` | `current_exe` (`_NSGetExecutablePath`) is forbidden under isolation (re-runs the test binary as a child) |
| `sys::tests::log_stderr_writes_non_utf8_scope_and_message_bytes_verbatim` | as above |
| `types::manifest_header_sync_tests::manifest_enums_match_the_shipped_headers` | `opendir` is forbidden under isolation (reads the shipped headers) |

**Slow exceptions (criterion 1 is the budget, not impossibility).** Seven S7/S7-5/S7-6/reference
tests. In the per-test enumeration each ran past its bound under the G6 conditions (8 workers on a
14-core host with other agents' builds, so the bound was contended); the sweep corpus was then
profiled alone with `Instant` checkpoints under Miri: one pass is about 1,770 s of Miri time before
its final resize, of which the `resize(600, 30)` alone is 1,632 s (natively 70 ms of a 185 ms pass),
and the terminal sweep makes one pass per allocation the scenario performs. The causes of the other
five were not profiled (UNVERIFIED); each writes one to three full pages of history through the
terminal.

| test | what makes it heavy |
|---|---|
| `terminal::tests::allocation_failure_sweep_through_the_c_api` | one sweep-corpus pass (above) per allocation, then per remap |
| `terminal::tests::sweep_corpus_reaches_the_s7_4_families` | one sweep-corpus pass (above) |
| `terminal::tests::compression_activity_and_incremental_scheduling` (reference test, oracle 172f15da3) | 4,000 lines of history, then an incremental compression pass |
| `snapshot::tests::snapshot_incremental_decoder_exposes_ready_and_page_progress` | three pages of history at 215 columns, then an encode and a decode |
| `render::tests::render_state_allocation_failure_sweep_through_the_c_api` | one render-state update per allocation of the sweep |
| `search::tests::search_feed_and_run_report_out_of_memory_instead_of_spinning` | two pages of scrollback before the search |
| `search::tests::search_internals_route_through_the_callers_allocator` | two pages of scrollback before the search |

Criterion 2: all seven pass natively in `cargo test --workspace`, and under AddressSanitizer
(`sanitizers.sh rustty-vt-c <the seven names> address`, run once by task C-1 on 2026-09-27: 9 tests
ran — the filters also match two sibling sweeps — all passed, no finding). The ASan run is not
repeated by a gate (the same standing as the PageList exceptions at the top of this file). Leaks:
the four sweeps assert, natively, that the `FailingAllocator` got every byte back
(`allocated_bytes == freed_bytes`, `allocations == deallocations`) for every n.

Criterion 3: the same code paths are Miri-verified by lighter `rustty-vt-c` tests that pass in the
lane: `osc::tests::osc_allocation_failure_sweep_through_the_c_api`,
`terminal::tests::allocation_failure_sweep_through_the_allocating_writers`,
`search::tests::search_allocation_failure_sweep_through_the_c_api`,
`search::tests::search_shared_allocator_failure_sweep_through_the_c_api`,
`snapshot::tests::allocation_failure_sweep_through_snapshot_decode`, the `*_route_through_*`
allocator-routing tests of `terminal`, `render`, `search` and `snapshot`, and
`render::tests::render_state_internals_route_through_the_callers_allocator`; the page, reflow and
compression code underneath is `rustty-vt`'s, covered by its own G6/G6c lanes.

Not excepted, FIXED: `style::tests::convert_style_with_colors` (UB, NEW-FINDINGS N393);
`render::tests::render_row_get_set_dirty` and `render::tests::render_update` (UB in `rustty-vt`'s
`RenderState::update`, N394); `osc::tests::osc_allocation_failure_sweep_through_the_c_api` (its
`cfg(miri)` corpus had dropped the one capture that grows, so the remap sweep asserted nothing to
refuse; the Miri corpus keeps a `MAX_BUF + 64` capture and the test passes in 51 s).

### `rustty-vt` (G6 lane: lib 3015 passed, 207 ignored, every integration binary and the doctests green; G6c: 119 passed in 6272 s)

Found through this triage and FIXED in code, not excepted: N394 (`RenderState::update` held a shared
row slice across `set_dirty` writes, Stacked Borrows); `page_tests.rs`'s two unaligned-buffer tests
leaked their buffer on purpose (a drop guard now frees it on unwind); the snapshot golden tests
(`snapshot::fixture::expect_equal`) now compare bytes only under `cfg(miri)`, where the candidate file
they also write (a debugging aid for a mismatch) is forbidden by isolation, so the golden
encode/decode paths stay in the lane; `pagelist_integrity_detects_max_lines_exceeded` fills its page
with that page's integrity checks paused (see its entry above), so it runs in G6.

Isolation / foreign-call exceptions:

| test | what Miri cannot do |
|---|---|
| `alloc::tests::failing_allocator_captures_the_stack_at_the_first_induced_failure` | formatting the captured `Backtrace` resolves symbols, which calls `getcwd` |
| `kitty::graphics_image::tests::zlib_inflate_accepts_exactly_the_limit` | vendored Wuffs zlib decoder (`wuffs_zlib__decoder__initialize`; was zlib `inflateInit2_` before P7-W5 T2) |
| `ref_geometry_golden_tests::the_port_matches_the_reference_geometry_golden` | `open` (reads the checked-in golden) |
| `stream_terminal::tests::kitty_graphics_shared_memory_copies_the_requested_range` | `shm_open` / `shm_unlink` |
| `tests/fuzz_corpus_replay.rs`: `replay_fixed_fuzz_findings_osc`, `replay_port_full_corpus`, `replay_port_initial_seeds_{cross,osc,parser,stream}` | `stat` / `opendir` (walk the mirrored corpus) |
| `tests/kitty_temp_dir_security.rs` `kitty_temp_file_medium_ignores_the_process_tmpdir` | real temporary-directory file system calls |

Budget exceptions that are not about page integrity:

| test | why |
|---|---|
| `compress::lz4_differential::lz4_differential_light` | a 24 s native property suite even with debug assertions off, over `lz4.rs`, which has no `unsafe`; the codec is Miri-covered by the 14 `compress::lz4::tests` (all pass in G6) and `.port/phase-2/mirihar-compress-lz4` |
| `tests/viewport_fingerprint_alloc.rs` `fingerprint_only_allocates_on_changes` | past a 15-minute bound; the contract is Miri-verified in G6 by the port of the reference test, `search::viewport::tests::fingerprint_only_allocates_on_changes` and `fingerprint_allocation_failure_preserves_the_old_fingerprint` |
| `tests/print_grapheme_transfer_alloc.rs` `cross_page_wide_grapheme_transfer_does_not_allocate` | fills a whole page line by line (stopped after 9 minutes); the cross-page transfer path is Miri-verified in G6c by `terminal::tests::terminal_print_wide_grapheme_transfers_across_a_page_seam` |

**Moved from G6 to G6c: 36 tests** (`cfg_attr(all(miri, debug_assertions), ignore)`, the WS19 T6
partition). Each ran past a 15-minute bound in G6 (one test per process, 8 workers; 20 minutes for
some in the first pass), and each passed in G6c conditions (`-C debug-assertions=off`,
`-Zmiri-disable-isolation`) within an 8-minute bound, 7 workers on a host other agents were also
loading. Natively, with debug assertions, they take the times below; with them off every one of them
takes at most 0.3 s, cargo overhead included: the integrity walks are the cost.

| test | native, debug assertions on | G6c, measured |
|---|---|---|
| `terminal::tests::terminal_set_scrollback_max_lines` | 3.86 s | 4 s (found in a later pass) |
| `terminal::tests::ud80_wide_terminal_capacity_clone_past_the_refcount_ceiling_keeps_every_cell_styled` | 0.24 s | 171 s (found in a later pass) |
| `pagelist::tests::pagelist_enforce_limit_marks_activity_and_preserves_continuation` | 2.28 s | 2 s |
| `pagelist::tests::pagelist_eraseRowBounded_hyperlink_dense_row_crosses_page_boundary` | 2.35 s | 2 s |
| `pagelist::tests::pagelist_eraseRow_hyperlink_dense_row_crosses_page_boundary` | 2.31 s | 2 s |
| `pagelist::tests::pagelist_increaseCapacity_returns_OutOfSpace_at_max_capacity` | 0.10 s | 77 s |
| `pagelist::tests::pagelist_incremental_compression_advances_after_allocation_failure` | 1.18 s | 44 s |
| `pagelist::tests::pagelist_incremental_compression_advances_after_decommit_failure` | 1.17 s | 45 s |
| `pagelist::tests::pagelist_incremental_compression_advances_after_failure` | 1.18 s | 106 s |
| `pagelist::tests::pagelist_incremental_compression_bounds_inspected_pages` | 5.01 s | 304 s |
| `pagelist::tests::pagelist_incremental_compression_keeps_progress_after_tail_growth` | 5.47 s | 305 s |
| `pagelist::tests::pagelist_incremental_compression_restarts_after_prune_reuse` | 1.18 s | 50 s |
| `pagelist::tests::pagelist_incremental_compression_restarts_after_reset` | 1.18 s | 48 s |
| `pagelist::tests::pagelist_incremental_compression_skips_visible_history` | 1.73 s | 113 s |
| `pagelist::tests::pagelist_lazily_restores_compressed_history_made_active_by_resize` | 0.63 s | 31 s |
| `pagelist::tests::pagelist_max_lines_and_max_size_enforce_the_smaller_limit` | 3.33 s | 3 s |
| `pagelist::tests::pagelist_max_lines_applies_to_resize_and_clone` | 2.24 s | 23 s |
| `pagelist::tests::pagelist_max_lines_does_not_round_larger_limits` | 0.77 s | 0 s |
| `pagelist::tests::pagelist_max_lines_uses_one_page_minimum` | 0.63 s | 0 s |
| `pagelist::tests::pagelist_prune_reuses_malformed_compressed_page_memory` | 1.17 s | 22 s |
| `pagelist::tests::pagelist_set_max_bytes_prunes_immediately_and_can_be_raised` | 6.05 s | 4 s |
| `pagelist::tests::pagelist_set_max_limits_remain_independent` | 3.89 s | 2 s |
| `pagelist::tests::pagelist_set_max_lines_prunes_immediately_and_can_be_raised` | 4.96 s | 3 s |
| `render::tests::incremental_updates_match_full_rebuild` | 0.30 s | 135 s |
| `screen::tests::screen_scroll_above_hyperlink_dense_row_to_existing_page` | 16.40 s | 28 s |
| `screen::tests::screen_scroll_above_hyperlink_dense_row_to_fresh_page` | 16.47 s | 26 s |
| `screen::tests::screen_scrollback_bytes_zero_none_and_finite_are_distinct` | 32.70 s | 20 s |
| `snapshot::screen::tests::screen_encodes_the_minimal_complete_page_active_suffix` | 0.63 s | 18 s |
| `terminal::tests::terminal_deleteLines_hyperlink_dense_row_crosses_page_boundary` | 20.33 s | 23 s |
| `terminal::tests::terminal_insertLines_hyperlink_dense_row_crosses_page_boundary` | 20.49 s | 19 s |
| `terminal::tests::terminal_print_wide_grapheme_transfers_across_a_page_seam` | 0.59 s | 1 s |
| `terminal::tests::terminal_scroll_clear_across_pages_keeps_cursor_hyperlink_refs_page_local` | 0.31 s | 17 s |
| `terminal::tests::terminal_set_scrollback_max_bytes` | 3.85 s | 8 s |
| `terminal::tests::ud131_delete_lines_degrades_on_an_ordinary_80_column_terminal` | 10.62 s | 58 s |
| `terminal::tests::ud131_thirty_one_colours_is_the_80_column_control` | 0.67 s | 12 s |
| `terminal::tests::ud80_wide_terminal_decaln_past_the_refcount_ceiling_keeps_every_cell_styled` | 0.19 s | 271 s |

**Both lanes: 21 tests** (plain `cfg_attr(miri, ignore)`, "BOTH LANES" in the reason): past 15
minutes in G6 AND past 8 minutes in G6c, one test per process. Criterion 2: all pass natively,
and all 21 ran under AddressSanitizer (20 in one run, the late one alone) (`sanitizers.sh rustty-vt <the 20 names> address`, task C-1,
2026-09-28: 20 + 1 passed, no finding; not repeated by a gate). Criterion 3: their families stay
Miri-covered by lighter tests that pass in G6 or G6c: the incremental-compression family by the
`pagelist_incremental_compression_*` tests moved to G6c above and the WS7 compression tests already
there; the UD80/UD131 style-ceiling family by the G6c-moved `ud80_*`/`ud131_*` siblings and the
`style`/`page` unit tests; snapshot history/page decode by the snapshot tests that pass in G6.
The cost of the ud80/ud131 and compression members was not profiled further (UNVERIFIED).

| test | native, debug assertions on |
|---|---|
| `terminal::tests::ud80_wide_terminal_live_styles_at_the_capacity_ceiling_still_split` | 1.45 s (found in a later pass; ASan: 1 passed) |
| `pagelist::tests::pagelist_compress_page_rejects_an_already_compressed_node` | 1.19 s |
| `pagelist::tests::pagelist_compression_restores_through_page_access` | 0.10 s |
| `pagelist::tests::pagelist_incremental_compression_restarts_after_active_boundary_resize` | 2.26 s |
| `pagelist::tests::pagelist_incremental_compression_restarts_after_earlier_replacement` | 2.79 s |
| `pagelist::tests::pagelist_incremental_compression_restarts_after_replacement` | 1.73 s |
| `pagelist::tests::ud131_erase_row_loop_degrades_only_new_styles_and_grows_the_other_dimensions` | 4.02 s |
| `pagelist::tests::ud131_erase_row_loop_full_live_set_degrades_new_style_to_default` | 1.45 s |
| `pagelist::tests::ud80_erase_row_into_a_page_full_of_orphaned_pinned_styles_reclaims_them` | 0.41 s |
| `render::tests::render_state_allocation_failure_sweep` | 0.39 s |
| `screen::tests::ud131_clone_partial_row_loop_degrades_only_new_styles_and_grows_the_other_dimensions` | 4.01 s |
| `screen::tests::ud131_clone_partial_row_loop_full_live_set_degrades_new_style_to_default` | 1.46 s |
| `snapshot::history::tests::history_encodes_newest_first_and_restores_complete_history` | 1.20 s |
| `snapshot::page::tests::decode_sparse_page_rejects_every_truncation` | 0.15 s |
| `terminal::tests::terminal_compress_incremental_restarts_after_complete` | 3.66 s |
| `terminal::tests::terminal_compression_activity_tracks_pagelist_mutation_not_compression` | 3.66 s |
| `terminal::tests::terminal_restore_cursor_uses_default_style_on_out_of_space` | 0.12 s |
| `terminal::tests::ud80_delete_lines_into_a_page_full_of_orphaned_pinned_styles_reclaims_them` | 0.41 s |
| `terminal::tests::ud80_wide_terminal_one_style_past_the_refcount_ceiling_keeps_every_cell_styled` | 0.13 s |
| `terminal::tests::ud80_wide_terminal_orphaned_pinned_styles_at_the_capacity_ceiling_are_reclaimed_not_split` | 1.81 s |
| `tmux::layout::tests::dropping_a_very_deep_layout_does_not_recurse` | 0.11 s |

