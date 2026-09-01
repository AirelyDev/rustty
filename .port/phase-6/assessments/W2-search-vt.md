# Phase 6 — W2 assess+port record: search (vt side)

Oracle range: `88f57ee66` (Phase-5 pin) → `20abdb50a` (= origin/main, 2026-08-31).
Oracle working tree is checked out AT the target pin; all "end state" reads are direct
file reads; `git -C ../ghostty show 88f57ee66:src/terminal/search/Thread.zig` for the
pre-range state. Rustty side: branch `phase-6/w2-search-vt` off `phase-6-upstream-resync`
@ `62c1690d`. Worker: P6-W2. Date: 2026-09-01.

W2 file set (plan `docs/superpowers/plans/2026-09-01-phase6-upstream-resync.md` §P6-W2):
`src/terminal/search.zig` (+1), `search/Thread.zig` (−560 net), `search/screen.zig` (+23),
`search/terminal.zig` (NEW, 655). Rustty home: `crates/rustty-vt/src/search/`.

---

## 1. Coverage statement

Two non-merge range commits touch the W2 file set:

| oracle commit | subject | W2 hunks | verdict at arrival | verdict now |
|---|---|---|---|---|
| `149c9f562` | terminal: extract whole-terminal search orchestration from the search thread | `search.zig` +1, `Thread.zig`, `screen.zig`, `terminal.zig` NEW | **ABSENT** | **PRESENT** |
| `f0c918fc4` | terminal/c: allow freeing a search and its terminal in any order | `search/terminal.zig` hunk only (`c/search.zig` half = W3) | **ABSENT** | **PRESENT** (terminal.zig half); `c/search.zig` half **N-A here (W3)** |

The `c/search.zig` / `include/ghostty/vt/search.h` / `example/c-vt-search` work
(`32601cd79`, `f9202919f`, `76d9fcefe`, `674abd8a1`, and the other half of `f0c918fc4`)
is W3 and was **not** touched.

Everything in the range END STATE of the four W2 oracle files is now in rustty:
- every `pub`/private fn of `terminal.zig` (§2 map);
- every doc/code comment sentence (G3b: the 8 `search/*`-attributed `vt` sentences at
  arrival are gone; the 25 remaining `vt` sentences all attribute to W1 files — §6);
- all 6 `terminal.zig` tests, assertion-for-assertion, same names (§4);
- the 1 test that MOVED out of `Thread.zig` moved out of `thread.rs` too (no duplicate:
  `grep -c '#\[test\]' thread.rs` = 1, was 2);
- `screen.zig`'s `matchAt`/`selectedMatch` split, comments carried.

Arrival evidence (ABSENT): `git grep -n "TerminalSearch\|fn match_at\|Option<\*mut Terminal>" 62c1690d -- crates/rustty-vt/src/search` → 0 hits; `thread.rs` @ `62c1690d` carried
the pre-range private `Search` struct (`pub struct Search` at old line 995) with the
`select` body inlined in `Thread::select` (old lines 540–647) and `Search::deinit(&mut self,
t: *const Terminal)` non-optional.

## 2. Function-move map (Thread.zig pre-range → terminal.zig end state → rustty)

| pre-range `Thread.zig` | end-state `terminal.zig` (`TerminalSearch`) | rustty (`crates/rustty-vt/src/search/…`) |
|---|---|---|
| `const Search = struct` (private) | `pub const TerminalSearch = struct` | `terminal.rs:68 pub struct TerminalSearch` (was `thread.rs pub struct Search`) |
| `Search.viewport` | `.viewport` | `terminal.rs` field `viewport: ViewportSearch` |
| `Search.screens: EnumMap(Key, ScreenSearch)` | `.screens` | field `screens: ScreenSearchMap` (`terminal.rs:165`, moved from thread.rs) |
| `Search.screen_generations` | `.screen_generations` | field `[Option<usize>; Key::COUNT]` |
| `Search.last_screen.key` | `.active_key` | field `active_key: ScreenSetKey` |
| `Search.last_screen.{total,selected}` + `Search.last_complete` | **stay in the thread** as `Thread.NotifyState{key,total,selected,complete}` | `thread.rs:1130 pub struct NotifyState`, `:1164 NotifyStateSelected`, field `Thread.notify_state` (`thread.rs:172`) |
| `Search.stale_viewport_matches` | `.stale_viewport_matches` | field |
| — (arena-collected in `notify`) | `.viewport_matches: ArrayList(Flattened)` + `.alloc` | fields `viewport_matches: Vec<FlattenedHighlight>`, `alloc` |
| `Search.init(alloc, needle)` | `init(alloc, needle_unowned)` | `TerminalSearch::init` `terminal.rs:219` |
| `Search.deinit(self, t: *Terminal)` | `deinit(self, t_: ?*Terminal)` (`f0c918fc4`) | `deinit(&mut self, Option<*mut Terminal>)` `terminal.rs:261` |
| `Search.screenIsValid` | `screenIsValid` | `screen_is_valid` `terminal.rs:322` |
| — | `needle()` NEW | `needle` `terminal.rs:348` |
| — | `activeScreenSearch()` NEW | `active_screen_search` `terminal.rs:357` |
| `Search.isComplete` | `isComplete` | `is_complete` `terminal.rs:364` |
| — | `Status` + `status()` NEW | `terminal::Status` `:117`, `status` `:388` |
| `Search.Tick` / `Search.tick` | `Tick` / `tick` | `terminal::Tick` `:150` (was `thread::Search_Tick`), `tick` `:427` |
| `Search.feed(self, alloc, t)` incl. the `t.flags.search_viewport_dirty` read+clear | `feed(self, t, active_dirty)` — flag handling moved OUT to `Thread.feedLocked` | `feed(&mut self, *mut Terminal, bool)` `terminal.rs:513`; `Thread::feed_locked(&Options, &mut TerminalSearch)` `thread.rs:520` |
| `Search.notify` viewport-collection block (arena + `viewport.next()` loop) | `viewportMatches()` (cached) + `clearViewportMatches()` NEW | `viewport_matches` `terminal.rs:715`, `clear_viewport_matches` `:768` |
| `Search.notify(self, alloc, cb, ud)` (rest) | **stays in the thread** as `Thread.notify(self, s, cb, ud)` | `Thread::notify(&mut NotifyState, &mut TerminalSearch, cb, ud)` `thread.rs:689` |
| `Thread.select(sel)` body: feed → getPtr → select → selectedMatch → viewport-overlap scan → `screen.scroll` | `select(self, t, to, scroll: SelectScroll)` NEW; `SelectScroll{if_needed,none}` | `TerminalSearch::select` `terminal.rs:800`, `terminal::SelectScroll` `:137`; `Thread::select` `thread.rs:567` keeps the mutex + `notify_state.selected = null` |
| `ScreenSearch.selectedMatch` (screen.zig) | split: `matchAt(idx)` + `selectedMatch` | `screen.rs:1393 match_at`, `:1420 selected_match` |
| `test "select after active screen removal"` (Thread.zig) | same name, `terminal.zig` | `terminal.rs:1038 select_after_active_screen_removal` (deleted from `thread.rs`) |

`Thread.zig` end-state deltas mirrored in `thread.rs`: `ArenaAllocator`/`Selection`/
`ViewportSearch` imports dropped, `TerminalSearch` import added; `search: ?TerminalSearch`;
`notify_state` field (reset in `changeNeedle` at both the stop and the new-search points);
`changeNeedle` compares `s.needle()`; all four feed sites go through `feedLocked`.

## 3. Raw-pointer / lifetime notes (every pointer-carrying field of `TerminalSearch`)

`TerminalSearch` holds **no pointer to the `Terminal`** — the reference passes `t: *Terminal`
per call, and so does the port (`*mut Terminal`; `Option<*mut Terminal>` for `?*Terminal`).
Inside a call, the terminal is read only through that raw argument: `&t.screens` is
`core::ptr::addr_of!((*t).screens)` (a raw `*const ScreenSet`, no reference is formed
into the `Terminal`), `t.screens.active.pages` is a narrow `*const PageList` for the
single `viewport.update` call. This is the same discipline the prior SB fix restored
(memory `search-screen-sb-ub-finding`; `9e3fe1c`), now applied unchanged to the moved code.

| field | aliases | who invalidates it | what keeps it alive / how staleness is detected |
|---|---|---|---|
| `screens[k].screen: *mut Screen` (inside each `ScreenSearch`) | the ScreenSet's heap `Screen` for key `k` (`t.screens.all[k]`) | `ScreenSet.remove`/replacement (`switchScreen` creating a new alt screen), `Terminal.deinit` | `screen_generations[k]` captured at `ScreenSearch.init`; `screen_is_valid` compares generation AND pointer before any deref (`feed` reconciliation, `deinit`). A dead screen's searcher is released with `deinit_screen_invalid` (never untracks pins) |
| `ScreenSearch` tracked pins (`selected.highlight`, `history.start_pin`) and `PageListSearch.list` | that Screen's `PageList` tracked-pin pool / node list | the same events as above; also `Terminal.deinit` (page storage freed) | same validity check; `deinit(None)` (`f0c918fc4`) takes the invalid path for every entry because the terminal is gone |
| `viewport: ViewportSearch` (fingerprint of the active PageList's viewport pages) | page nodes of the ACTIVE screen's PageList | page pruning/reflow/resize | fingerprint compares node pointer + serial on `update`; never dereferences a stale node |
| `viewport_matches: Vec<Flattened>` (chunks carry `*mut Node` + `serial`) | page nodes of the active PageList | pruning/reflow between feeds | cache is owned by the search; validity contract per the reference doc: valid until the next `viewportMatches`/`feed`/`deinit`; consumers (`Thread::notify`) use it only inside the callback issued right after collection. `Flattened::start_pin()` is only used in `select` immediately after `feed` under the caller's exclusive terminal access |
| `alloc: Allocator` | ZST global allocator | — | — |

Thread-side: `Thread.opts.terminal: *mut Terminal` (unchanged) is dereferenced only under
`opts.mutex` (`feed_locked`, `select`, `deinit`, `change_needle`), matching the reference's
`lockUncancelable` sites. The hand-off from the thread to `TerminalSearch` never creates a
`&mut Terminal`.

`Thread::feed_locked` / `Thread::notify` are associated fns taking `&Options` /
`&mut NotifyState` plus `&mut TerminalSearch` instead of `&mut self`: the reference's `s`
always points INTO `self.search`, so the port borrows the two fields disjointly rather
than forming an aliasing `*mut Thread` (PORT NOTEs at `thread.rs:520`, `:673`).

Test-side SB discipline: every `terminal.rs` test reaches the terminal only through
`t_ptr = addr_of_mut!(t)` once a search exists (including `vt_stream()`, `switch_screen`,
`screens.remove`, `deinit`); a `&mut t` reborrow of the local would be a Unique retag
popping the raw tag (memory `shaper-hook-sb-clean-interleave`). The reference's single
long-lived `stream` is kept (the `Stream` binds a raw `*mut Terminal` derived from `t_ptr`,
which survives the search's reads through the parent pointer) — proven under Miri, §5.

## 4. Tests (assertion-for-assertion)

| oracle `test "…"` (terminal.zig) | rustty `#[test]` (`terminal.rs`) | asserts |
|---|---|---|
| starts feed required and runs to complete | `starts_feed_required_and_runs_to_complete` | `isComplete` true on fresh; `status == feed_required`; pump to complete; `matchesLen == 2` |
| viewport matches are cached until the next feed | `viewport_matches_are_cached_until_the_next_feed` | 2, 2 (cached), then 3 after a feed that observed a change |
| select after active screen removal (MOVED from Thread.zig) | `select_after_active_screen_removal` | `active_key == alternate` + contains; after switch+remove and `select`: `active_key == primary`, `!contains(alternate)` |
| select scrolls the viewport only when needed | `select_scrolls_the_viewport_only_when_needed` | first select: point `{active 0,1}`, visible; second: visible (scrolled); third with `.none`: selected but NOT visible |
| deinit after the terminal is gone (`f0c918fc4`) | `deinit_after_the_terminal_is_gone` | run to complete, `select(.next,.none)` true; `stream.deinit(); t.deinit(); search.deinit(null)` |
| no matches selects nothing | `no_matches_selects_nothing` | `select(.next)` false, `select(.prev)` false |

`Visible.check` helper ported as a nested `struct Visible { fn check(*mut Terminal, &mut
TerminalSearch) }` with the reference's `pointFromPin(.viewport)` + `y < pages.rows` check
(and its comment on why the row bound is needed).

Counts: `grep -c '#\[test\]' crates/rustty-vt/src/search/terminal.rs` = **6**;
`thread.rs` = **1** (was 2: the moved test only). Net search tests +5, as the plan predicts.
G1b: the `search` row of `check_test_counts.py` must gain `search/terminal.zig` — coordinator
(not edited here, per the brief).

## 5. Verification

All runs in this worktree on the committed tree (`170c232a`).

| check | result (verbatim summary line) |
|---|---|
| `cargo build -p rustty-vt` | `Finished` — 0 warnings |
| `cargo build --workspace` (cold) | `Finished \`dev\` profile … in 57.10s`, EXIT=0, 0 warnings — consumers (`rustty-core` surface.rs uses only `Thread`/`Options`/`Message`/`WriteReq`/`Event`/`xev`, all unchanged) needed **no** rename |
| `cargo test -p rustty-vt --lib search` | `test result: ok. 79 passed; 0 failed; 0 ignored; 0 measured; 2898 filtered out` (3/3 re-runs; the first run tripped the pre-existing `search_thread` load flake — §8) |
| `cargo test -p rustty-vt` (full) | `test result: ok. 2974 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 307.48s` + every integration target ok, EXIT=0 |
| `cargo clippy -p rustty-vt --all-targets -- -D warnings` | `Finished` — clean |
| `rustfmt --check --edition 2021` on terminal.rs / thread.rs / screen.rs / mod.rs | exit 0 |
| `grep -c '#\[test\]'` | terminal.rs **6**, thread.rs **1** (was 2) |

### 5.1 Miri search lane — `bash .port/tools/miri-run.sh rustty-vt search 6 45 15`

Harness line: `=== FINAL: completed=79/79  ok=41  UB=4  FAIL=34  TIMEOUT=0  elapsed=0h10m43s` /
`VERDICT: PROBLEM` — which, per the brief, must be read per test (the harness's `UB`
pattern also matches `error: aborting`, and its `FAIL` is "not `1 passed`"). Per-test
reclassification of `/tmp/mirirun.rustty-vt/out/*` (script kept at
`/tmp/w2-miri-reclass.sh`; counts `Undefined Behavior` / `memory leaked` separately):

| class | n | what |
|---|---:|---|
| **real UB (`Undefined Behavior`)** | **0** | — |
| **memory leaked** | **0** | — |
| PASS | 43 | incl. **all 6 `search::terminal::tests::*`** (`deinit_after_the_terminal_is_gone`, `no_matches_selects_nothing`, `select_after_active_screen_removal`, `select_scrolls_the_viewport_only_when_needed`, `starts_feed_required_and_runs_to_complete`, `viewport_matches_are_cached_until_the_next_feed`) |
| IGNORED (documented `cfg_attr(miri, ignore)` / G6-lane-only, `.port/phase-2/MIRI_EXCEPTIONS.md`) | 35 | pagelist/screen/sliding_window multi-page tests, `search_thread` (Miri ICE exception), `viewport::history_search_no_active_area` — the harness's 34 `FAIL` + 1 of its `UB` |
| harness `UB` false positives | 4 | 3 × the integration target `tests/viewport_fingerprint_alloc.rs` failing to **build** under a parallel-worker rlib race (`error: extern location for rustty_vt does not exist`) AFTER the lib test in that invocation had already printed `1 passed`/`1 ignored`; 1 × `sliding_window_init_allocations_are_fallible` = Miri "resource exhaustion" on a deliberately impossible allocation (documented, MIRI_EXCEPTIONS.md:2440; rustty-only test, not a W2 file) |

**Criterion UB=0: met.** The 6 new tests exercise every raw-pointer path of the new type
under Stacked Borrows: `feed`/`select`/`deinit(Some)` through `t_ptr` interleaved with a
live `vt_stream()` on the same terminal, `switch_screen` + `screens.remove` under the
searcher, and `deinit(None)` after `Terminal::deinit`.

### 5.2 Negative controls (mutation harness `/tmp/w2-mutations.py`, source restored via cp backups; tree re-verified green afterwards)

| # | mutation | must-fail test | result |
|---|---|---|---|
| M1 | `status()`: drop the `!saw_any → FeedRequired` guard | `starts_feed_required_and_runs_to_complete` | **FAIL** (as required) |
| M2 | `viewport_matches()`: never serve the cache | `viewport_matches_are_cached_until_the_next_feed` | **FAIL** |
| M3 | `select()`: `SelectScroll::None` still scrolls | `select_scrolls_the_viewport_only_when_needed` | **FAIL** |
| M4 | `select()`: skip the reconciling `feed(t,false)` | `select_after_active_screen_removal` | **FAIL** |
| M5 | `match_at()`: `active_len - idx` off-by-one | `screen::tests::select_next` | **FAIL** |
| M6 | `deinit(None)` treating the gone terminal as valid | `deinit_after_the_terminal_is_gone` | not run natively (a UAF may be silent); it is Miri-detectable and the shipped code passes Miri |

Harness caveat worth recording: restoring a file with `shutil.move` keeps the OLD mtime,
so cargo does not rebuild — the first post-restore "baseline" run executed the stale M5
binary. `touch` + rebuild → `79 passed`. Every M1–M5 mutation was freshly written (new
mtime), so those verdicts are genuine.

Items without a reference test (and therefore without a unit negative control) — carried
by fidelity + the `search_thread` integration test only: `Thread::feed_locked` clearing
`search_viewport_dirty`, `NotifyState` reset in `change_needle`, the `state.key` reset in
`Thread::notify`. The reference has no tests for these either.

## 6. G3b attribution (remaining `vt` sentences)

`python3 .port/tools/gates/check_comment_coverage.py | grep -E "^G3b \[vt\]"`:
arrival **missing=38** → now **missing=30** (`review` 75 → 73). The 8 sentences that
attributed to W2 files at arrival are gone:

| arrival sentence (normalized) | oracle file | now |
|---|---|---|
| "return the match at the given index in newest-to-oldest order" / "(0 = most recent match). returns null if the index is out of" | `search/screen.zig` `matchAt` | carried at `screen.rs:1380` |
| "feed the search from the terminal state. the terminal mutex must" | `search/Thread.zig` `feedLocked` | `thread.rs:509` |
| "a screen switch makes all previously notified per-screen state" | `search/Thread.zig` `notify` | `thread.rs:697` |
| (remaining `terminal.zig` doc sentences below the gate's dedup threshold) | `search/terminal.zig` | all carried verbatim |

Every sentence still listed attributes to a **W1** oracle file (grep -F against the oracle):

| oracle file | sentences |
|---|---|
| `terminal/ref_counted_set.zig` (`5f5495826`) | "a different id: we must resolve to the existing item and", "another id then ref count that and increase the id", "be reaped by the trim loop in `add`, then release it", "create a dead item between two live ones", "in the set: the requested id must be used", "mirrors a page style set after a styled run is erased", "request the dead id for a value that is already live", "requested id is dead, but if the value exists not under", "set. this allows callers to clean up any", "take a reference, without changing the living count" |
| `terminal/PageList.zig` (`028af92ce`, `a5423592c`) | "`alloc` is the caller-provided allocator…", "native freestanding targets don't have an os page allocator", "targets, where no os page allocator is available", "the allocator provided by the embedder" |
| `terminal/build_options.zig` (`3376153a4`) | "terminal search: find matches for a string in the active", "area and scrollback (ascii case-insensitive), with results", "scrollback pruning, plus match selection with wrap-around", "and viewport scrolling. used to implement find bars." |
| `terminal/Terminal.zig` + `Screen.zig` (`eb722cb26`) | "deleting/erasing the wide char also clears the spacer head on the previous", "row, so that row must be dirty too", "`clearcells` does not mark rows dirty, and our" |
| `terminal/kitty/graphics_image.zig` (`090fca451`) | "posix shared memory names must begin with a slash…", "least one character after it, contain no other slashes", "returns whether a name follows the posix shared memory name format" |

`vt-c` missing=4 (all `c/selection.zig`/`c/search.zig` buffer conventions) → W3.

## 7. Public API of `rustty_vt::search::terminal` (for W3)

```rust
pub struct TerminalSearch {
    pub alloc: Allocator, pub viewport: ViewportSearch, pub screens: ScreenSearchMap,
    pub screen_generations: [Option<usize>; Key::COUNT], pub active_key: screen_set::Key,
    pub stale_viewport_matches: bool, pub viewport_matches: Vec<Flattened>,
}
pub enum Status { Running, FeedRequired, Complete }
pub enum SelectScroll { IfNeeded, None }
pub enum Tick { Complete, Progress, Blocked }
pub struct ScreenSearchMap { pub entries: [Option<ScreenSearch>; Key::COUNT] }
  // init / get_ptr / get / contains / put / remove / key_from_index
impl TerminalSearch {
    pub fn init(alloc: Allocator, needle_unowned: &[u8]) -> Result<TerminalSearch, OutOfMemory>;
    pub fn deinit(&mut self, t_: Option<*mut Terminal>);          // ?*Terminal
    pub fn needle(&self) -> &[u8];
    pub fn active_screen_search(&mut self) -> Option<&mut ScreenSearch>;
    pub fn is_complete(&self) -> bool;
    pub fn status(&self) -> Status;
    pub fn tick(&mut self) -> Tick;
    pub fn feed(&mut self, t: *mut Terminal, active_dirty: bool);
    pub fn viewport_matches(&mut self) -> Result<&[Flattened], OutOfMemory>;
    pub fn select(&mut self, t: *mut Terminal, to: search::screen::Select, scroll: SelectScroll)
        -> Result<bool, OutOfMemory>;
}
// screen.rs: pub fn ScreenSearch::match_at(&self, idx: usize) -> Option<Flattened>
// thread.rs: pub struct NotifyState { key, total, selected: Option<NotifyStateSelected>, complete } (Default)
```

## 8. Open items

- `search::thread::tests::search_thread` is a **pre-existing** parallel-load flake (its
  100 ms `timedWait` budget; documented at `.port/phase-2/PERF_LOG.md:285` and
  `.port/codex-vet/STATE.json`): it failed once in the first `cargo test -p rustty-vt search`
  run of this work (timeout → poisoned test mutex → callback panic), then passed 3/3
  re-runs and in isolation. Not introduced by W2 (the thread's control flow is
  unchanged; only the search state moved). Left as-is (a budget change would be a
  fidelity deviation); worth a coordinator note if it keeps firing on the merged branch.
- G1b file-list extension (`search/terminal.zig` in the `search` row) — coordinator.
- W3 consumes `TerminalSearch` from §7; the `c/search.zig` half of `f0c918fc4` (search
  handle outliving the terminal handle) maps onto `deinit(None)`.
