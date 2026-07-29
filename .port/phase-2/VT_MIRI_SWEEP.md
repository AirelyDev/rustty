# rustty-vt — First Full Miri Sweep (2026-07-11)

The first exhaustive Miri run over **all 2037 `rustty-vt` lib tests** (the crate was
`stub`-headered until the Phase-2 realignment, so its Miri lane had never run). Driven
from the main loop with a portable per-test process-group timeout; every candidate
non-clean result **re-verified in isolation** before classification (parallel Miri
over-reports FAIL under memory pressure — see below).

## Inventory (2037 tests)

| Result | Count | Disposition |
|---|---|---|
| Miri-clean | 1948 | ✓ |
| **Real SB UB** | **9** | 2 traces, one class, in `search/screen.rs` — **fix pending** |
| Intractable timeouts | ~70 | multi-page-PageList / large-construction class → documented `#[cfg_attr(miri, ignore)]` + 4-criterion MIRI_EXCEPTIONS entries (ASan runs all PageList tests ~18s + native cover them) |
| zlib FFI-limit | 3 | `image_load_rgb_zlib_compressed_*` — Miri can't call `inflateInit2_` → ASan route |
| Miri ICE | 1 | `search::thread::search_thread` (rustc `operand.rs:160` interpreter bug) → exception |
| parallelism false-FAIL | 8 | pass clean in isolation → **no exception** (final lane runs serial/low-parallel) |
| Raw syscall memory mgmt | 2 sites | `page.rs` mmap/munmap + `pagelist/mod.rs` madvise decommit (WS8 BL-2) → MIRI_EXCEPTIONS.md "Category 2"; the `#[cfg(test)]` arms ARE Miri-run, only the OS hints are excepted (complement: `examples/rss_probe.rs`, 9.0 MiB returned on macOS) |

**Method note (R5-critical):** running 6–8 concurrent memory-hungry Miri processes on
multi-page tests causes OOM/ICE-cascade false failures. UB is deterministic (unaffected);
timeouts and panics are not. Every exception candidate MUST be re-verified in isolation
before being excepted — never except a test that actually passes.

## The UB (real bug — the sweep's payoff)

9 tests: `search::screen::{simple_search, select_next, select_prev, select_prev_then_next,
active_change_contents, select_after_all_matches_disappear_drops_the_selection,
select_in_active_changes_contents_completely}`, `search::viewport::clear_screen_and_search_dirty_tracking`,
`render::link_cells`. Confirmed UB in isolation.

**Root cause:** the search objects hold raw aliases derived from `self.screen: *mut Screen`
— `PageListSearch.list: *mut PageList` points *into* the `Screen` allocation at the `.pages`
offset; tracked highlights hold `*mut Pin`. The port translated the reference's `*Screen`
(raw, freely-aliasing in Zig) method params to `screen: &mut Screen`
(`highlight.rs:47/76/104`, `search/screen.rs:138/158`), and ~14 sites reborrow
`unsafe { &mut *self.screen }`. Each whole-`Screen` `&mut` (Unique) retag pops the stored
`*mut PageList`/pin aliases off the Stacked-Borrows stack; the later `untrack_pin` through
the dead alias (`search/pagelist.rs:141`, `highlight.rs:105`) is UB.
- Trace A: `list` created `screen.rs:716`, invalidated `1496` (`&mut *active_ptr`), used `pagelist.rs:141`.
- Trace B: created `1741`, invalidated `1324` (`track(&mut *self.screen)`), used `1760` via `term_size.rs` OffsetBuf deref.

Same class as the already-fixed vt-c Handler / kitty `display()` / `Loop::stop` bugs
(memory `rust-mut-args-strongly-protected`, `shaper-hook-sb-clean-interleave`).

**Fix (pending — the hard-aliasing class, needs iterative Miri proof):** restore raw-pointer
semantics across `highlight.rs` + `search/screen.rs` + `search/pagelist.rs` —
`deinit`/`track`/`init` take `*mut Screen` (not `&mut`); `screen.rs:716` uses `addr_of_mut!`;
all `.pages` access via narrow scoped `(*screen).pages.method()` reborrows off the raw;
`PageListSearch` must never let a `&mut Screen` live while its `list` alias is held. The 9 UB
tests (8–23s each under Miri) ARE the proof harness. The vt Miri G6 lane stays correctly
**RED** until this lands (R5 — a real bug keeps the gate red).

## Closing sequence (pending)

1. Fix the `search/screen.rs` UB (above), verifying each of the 9 tests goes Miri-clean +
   still passes natively (no regression).
2. Route the ~70 intractables + 1 ICE as `#[cfg_attr(miri, ignore)]` with 4-criterion
   MIRI_EXCEPTIONS entries (measured durations recorded); the 3 zlib tests to the ASan lane.
3. Run the final vt Miri lane **serially / low-parallel** (avoids memory-pressure false-fails)
   → expect clean.
4. Closing `ci.sh` → first fully-green binding-mode run.

Full per-test verdicts: `/Users/gael/.claude/jobs/22556231/tmp/miri_master.txt` (regenerable).
