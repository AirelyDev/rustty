# Phase 6 — W3 assess+port record: search C API

Oracle range: `88f57ee66` (Phase-5 pin) → `20abdb50a` (= origin/main, 2026-08-31).
Oracle working tree is checked out AT the target pin; all "end state" reads are direct
file reads (`git -C ../ghostty show` only for history). Rustty side: branch
`phase-6/w3-search-capi` off `phase-6-upstream-resync` @ `ca2eb64d` (after the W1 and W2
merges). Worker: P6-W3. Date: 2026-09-01.

W3 file set (plan `docs/superpowers/plans/2026-09-01-phase6-upstream-resync.md` §P6-W3):
`src/terminal/c/search.zig` (NEW, 1,009), `c/selection.zig` (+12), `c/types.zig` (+7),
`c/terminal.zig` (+4), `c/main.zig` (+10), `src/lib_vt.zig` (the `features.search` export
hunk; W1 did the `std_options` half), `src/terminal/build_options.zig` (the cargo feature /
`options::SEARCH` half; W1 did the `Features.search` model field),
`include/ghostty/vt/search.h` (NEW, 501), `vt/selection.h` (+23), `vt/types.h` (+13),
`vt.h` (+9), `example/c-vt-search/*` (NEW). Rustty home: `crates/rustty-vt-c`,
`crates/rustty-vt/{Cargo.toml,src/options.rs}`, `include/rustty/vt`, `example/c-vt-search`.

---

## 1. Coverage statement

Five non-merge range commits touch the W3 file set. All were re-vetted against the
arrival tree (`git grep` on `ca2eb64d`) before porting; the END STATE of each file was
ported (the plan's range-diff trap: `f9202919f`'s `GhosttySearchOptions` was DROPPED by
`76d9fcefe` and is not in the port — there is no options struct, the needle is set via
`ghostty_search_set`/`rustty_search_set`).

| oracle commit | subject | W3 hunks | verdict at arrival | verdict now |
|---|---|---|---|---|
| `32601cd79` | terminal/c: add search wrapper implementing the whole-terminal search API | `c/search.zig` NEW, `c/selection.zig` (`CSelectionBuffer`), `c/main.zig`, `c/types.zig`, `build_options.zig` `Features.search` | **ABSENT** (`git grep -n "SearchWrapper\|CSelectionBuffer\|rustty_search_" ca2eb64d -- crates include example` → 0 hits; `build_options.rs` had the model field only, W1) | **PRESENT** — `crates/rustty-vt-c/src/search.rs`, `selection.rs:276` (`CSelectionBuffer`), `lib.rs` (`pub mod search`), `types.rs` (6 registry rows), `rustty-vt/Cargo.toml` + `rustty-vt-c/Cargo.toml` `search` feature, `options.rs:139` `SEARCH` |
| `f9202919f` | libghostty: add ghostty_search_* terminal search C API | `lib_vt.zig` `if (features.search)` block (8 exports), `include/ghostty/vt/search.h`, `vt/types.h` (`GhosttySearch`), `vt/selection.h` (`GhosttySelectionBuffer`), `vt.h` | **ABSENT** (`ls include/rustty/vt/search.h` → none; `nm target/release/librustty_vt_c.dylib \| grep search` → 0; A5 arm (d) RED on the 8 names, §6) | **PRESENT** — `lib.rs` `rustty_search_{new,free,tick,feed,run,set,get,get_multi}` under `#[cfg(feature = "search")]`; `include/rustty/vt/search.h` (rename transform of the oracle, byte-checked §5); `types.h`, `selection.h`, `vt.h` hunks |
| `f0c918fc4` | terminal/c: allow freeing a search and its terminal in any order | `c/search.zig` (`terminal: terminal_c.Terminal` nullable, `free` two arms, `zigTerminal`), `c/terminal.zig` (`searches` registry + detach in `free`) | **ABSENT** (`git grep -n "searches" ca2eb64d -- crates/rustty-vt-c/src/terminal.rs` → 0; the W2 half `TerminalSearch::deinit(Option<*mut Terminal>)` was PRESENT) | **PRESENT** — `terminal.rs` `TerminalWrapper.searches: HashSet<*mut SearchWrapper>` + detach loop in `free`; `search.rs` `new` registers, `free` unregisters/detaches; tests "search free after terminal free" + "search freed before terminal detaches from the registry" (§3, §4 NC1/NC3) |
| `76d9fcefe` | libghostty: set the search needle via ghostty_search_set, drop GhosttySearchOptions | `c/search.zig` (`new` takes no options; `.needle` option arm incl. same-needle keep + NULL/empty clear), `search.h` (`GHOSTTY_SEARCH_OPT_NEEDLE` doc), `example` | **ABSENT** | **PRESENT** — `search::new(alloc, out, terminal)` (3 params, no options struct anywhere: `git grep -n "SearchOptions" -- crates include example` → 0); `set_typed(.needle)` |
| `674abd8a1` | example: add c-vt-search demonstrating the terminal search C API | `example/c-vt-search/{README.md,build.zig,build.zig.zon,src/main.c}`, `vt.h` `@example`/`@ref` lines | **ABSENT** (`ls example/c-vt-search` → none) | **PRESENT** — `example/c-vt-search/{README.md,build.sh,src/main.c}` (rustty layout: `build.sh` byte-identical to the sibling `c-vt-selection/build.sh`; no `build.zig*` per `example/README.md`); built AND run, output §7 |

Everything in the range END STATE of the W3 oracle files is now in rustty:
- every `pub`/private fn of `c/search.zig` (§2 map); every doc/code comment sentence (G3b
  vt-c 4 → 0 crate-wide, and the scoped `--ref-files terminal/c/search.zig` run reads
  68 distinctive sentences, missing=0 — §6);
- all 17 `c/search.zig` tests, assertion-for-assertion, same names (§3);
  `c/selection.zig`/`c/terminal.zig`/`c/types.zig`/`c/main.zig` gained NO tests in the
  range (checked: `git -C ../ghostty diff 88f57ee66..20abdb50a -- src/terminal/c/selection.zig src/terminal/c/terminal.zig src/terminal/c/types.zig src/terminal/c/main.zig | grep -c '^+test'` → 0);
- the `c/main.zig` `pub const search = @import(...)` + 8 `search_* = search.*` aliases +
  `test { _ = search; }` → `lib.rs` `pub mod search` + the 8 `extern "C"` wrappers (the
  module's own `#[cfg(test)] mod tests` is the test reference);
- the `c/types.zig` 6 registry rows (`GhosttySelectionBuffer` struct, `GhosttySearchData`
  / `GhosttySearchOption` / `GhosttySearchScroll` / `GhosttySearchStatus` enums,
  `GhosttySearch` opaque) → `types.rs` (`check-vt-struct-layouts.mjs` registry parity with
  the oracle: 153 → 159 types, 0 residuals);
- the four curated headers under the rename transform (§5), and `include/rustty.h`
  regenerated (`cargo run -p xtask -- header-gen`, +105 lines, arm (a3) byte-identical).

## 2. Function map (`c/search.zig` → `search.rs`)

| `c/search.zig` | `crates/rustty-vt-c/src/search.rs` | notes |
|---|---|---|
| `pub const Search = ?*SearchWrapper` | `pub type Search = *mut SearchWrapper` | null = Zig `null` |
| `SearchWrapper{alloc, terminal, search, select_scroll}` | `pub struct SearchWrapper` (same 4 fields) | `alloc: Box<dyn ZigAllocator>` (C-resolved, owns the wrapper storage — F-008 pattern of `selection_gesture.rs`); `TerminalSearch::init` takes `rustty_vt`'s `allocator_api2::Global` (its own signature) — same seam as render/gesture |
| `SearchWrapper.zigTerminal` | `SearchWrapper::zig_terminal(*const Self) -> *mut ZigTerminal` | raw-self helper, no `&SearchWrapper` formed |
| `SearchWrapper.status` | `SearchWrapper::status(*mut Self) -> SearchStatus` | " |
| `Status` / `Scroll` / `Data` / `Option` (`enum(c_int)`) | `SearchStatus` / `SearchScroll` / `SearchData` / `SearchOption` (`#[repr(C)]`, int-size asserted, `C: Ghostty… -> Rustty…` doc lines) | `Option` renamed (would shadow `core::Option`); `Search*` prefix avoids the crate's pre-existing cbindgen name collisions (three `Data` enums already fold to one `Data` in `include/rustty.h`) — the generated header now carries `SearchStatus`/`SearchData`/`Search` unambiguously |
| `Data.OutType` / `Option.Type` (comptime type tables) | the per-arm casts in `get_typed` / `set_typed` | documented at each enum |
| `Status.fromZig`, `Scroll.toZig/fromZig` | `SearchStatus::from_zig`, `SearchScroll::to_zig/from_zig` | + `from_c_int` narrowing (`std.enums.fromInt`) on `SearchScroll`/`SearchData`/`SearchOption` |
| `new` | `new(Option<&CAllocator>, *mut Search, Terminal)` | registers in `(*terminal).searches` |
| `free` | `free(Search)` | two arms (§4 lifetime table) |
| `tick`, `feed`, `run` | same names | `run` loop verbatim |
| `set` (+ runtime-safety `fromInt` guard) | `set(Search, SearchOption, *const c_void)` + `set_checked(Search, c_int, ..)` | the guard + `log.warn("search_set invalid option value=…")` on the raw `c_int` (`sys::log_fn`, scope `search_c`); `rustty_search_set` calls `set_checked` |
| `setTyped` | `set_typed` | `.needle`: NULL/empty → `clear_needle`; null `ptr` → invalid; same-needle `eqlIgnoreCase` → `eq_ignore_ascii_case`; replacement built BEFORE `deinit` |
| `clearNeedle` | `clear_needle` | |
| `get` (+ guard) | `get(Search, SearchData, *mut c_void)` + `get_checked(Search, c_int, ..)` | W66-GETMULTI-RAW-01 shape: `get_multi` reaches the guard per element |
| `get_multi` | `get_multi(Search, usize, *const c_int, *mut *mut c_void, *mut usize)` | keys read as raw `c_int` |
| `getTyped` | `get_typed` | all 8 arms; buffer arms fill `CSelectionBuffer` (`cap < n` → `len = n`, `OutOfSpace`; null `ptr` with `n > 0` → `InvalidValue`) |
| `selectionFromHighlight` | `selection_from_highlight(&Flattened) -> CSelection` | `&` because rustty's `Flattened` owns its chunk `Vec` (not `Copy`); `.size` via `CSelection::default()` |
| `testString`, `testNewSearch` | `tests::test_string`, `tests::test_new_search` | + `test_terminal`/`vt_write`/`needle_bytes` glue |

## 3. Test name map (17/17)

| `c/search.zig` `test "…"` | `search::tests::…` | Miri |
|---|---|---|
| search lifecycle and run to complete | `search_lifecycle_and_run_to_complete` | ok |
| search tick reports feed required before first feed | `search_tick_reports_feed_required_before_first_feed` | ok |
| search feed after write updates results | `search_feed_after_write_updates_results` | ok |
| search alt screen flip and return retains results | `search_alt_screen_flip_and_return_retains_results` | ok |
| search select wraps in both directions | `search_select_wraps_in_both_directions` | ok |
| search select with no matches returns no value | `search_select_with_no_matches_returns_no_value` | ok |
| search select rejects a non-null reserved value | `search_select_rejects_a_non_null_reserved_value` | ok |
| search select scroll option | `search_select_scroll_option` | ok |
| search selection dropped when reset prunes results | `search_selection_dropped_when_reset_prunes_results` | ok |
| search matches buffer semantics | `search_matches_buffer_semantics` | ok |
| search get_multi returns first failing index | `search_get_multi_returns_first_failing_index` | ok |
| search new validates options | `search_new_validates_options` | ok |
| search without a needle is idle | `search_without_a_needle_is_idle` | ok |
| search needle change and clear | `search_needle_change_and_clear` | ok |
| search free after terminal free | `search_free_after_terminal_free` | ok |
| search freed before terminal detaches from the registry | `search_freed_before_terminal_detaches_from_the_registry` | ok |
| search free null | `search_free_null` | ok |

`cargo test -p rustty-vt-c search::` → `17 passed; 0 failed; … 469 filtered out`; the full
crate → `486 passed` (469 + 17). Miri (`cargo +nightly miri test -p rustty-vt-c
--no-default-features --features vt-features-default search::`) → `17 passed`, UB=0. One
Miri finding during the work was TEST-side (a hoisted `&mut buf as *mut` raw tag popped by
a later by-value `buf = …` reassignment); fixed by re-taking the pointer per call, which
is what the reference's per-call `&buf` does. The `integer-to-pointer cast` warnings are
the pre-existing `expose_provenance` in `terminal.rs`.

## 4. Pointer-lifetime table (`f0c918fc4` contract) and negative controls

| pointer field | aliases | who invalidates it | what keeps it valid |
|---|---|---|---|
| `SearchWrapper.terminal: *mut TerminalWrapper` | the C terminal handle the search was created with (borrowed, never freed by the search) | `terminal_c::free` — writes `null` through the registry's copy of the search pointer BEFORE the terminal storage is returned (the `f0c918fc4` detach loop) | the terminal's `searches` registry: every live search is in it, so `terminal_c::free` reaches every borrower |
| `TerminalWrapper.searches: HashSet<*mut SearchWrapper>` | every live search bound to this terminal | `search::free` — `remove(&search_)` while the terminal is alive; when the terminal dies first the set is `clear`ed with the wrapper | `search::new` inserts; a freed search unregisters itself first, so the set never holds a dangling search (NC3) |
| `SearchWrapper.search: Option<TerminalSearch>` (its `*mut Screen`/`*mut Node` raw aliases + tracked pins into the terminal's page storage) | the Zig terminal's screens/pagelists (W2's raw-pointer orchestration type) | any terminal mutation (validity-checked by serial/generation inside `TerminalSearch`); terminal death | `deinit(Some(t))` while the terminal lives releases tracked pins THROUGH `t`; `deinit(None)` after the terminal died releases only search-owned memory (the pins died with the pool) |
| `rustty_lib::String` returned by `get(.needle)` | the search-owned needle bytes | `set(.needle)` with a different needle, `clear_needle`, `free` | documented per the header ("remain valid until the needle is changed or the search is freed") |
| `CSelection` written by `get(.selected_match/.matches/.viewport_matches)` | untracked `CGridRef` snapshots (page `*mut Node` + x/y) | the next terminal-modifying operation | standard `GhosttySelection`/`RusttySelection` snapshot rules (header) |

No `&mut SearchWrapper` is ever formed: the wrapper is reached only through the raw
`Search` pointer (field-projection `&mut (*search_).search` reborrows, scoped to one
call), so the registry's write of `terminal = null` from `terminal_c::free` never races a
live reference. The Zig terminal is handed to `TerminalSearch` as the raw `*mut
ZigTerminal` it expects.

Negative controls (each actually executed, then the source restored and re-verified):

| # | mutation | test | outcome |
|---|---|---|---|
| NC1 | remove the `f0c918fc4` detach loop in `terminal_c::free` (`(*search).terminal = null` never runs) | `search_free_after_terminal_free` | **FAILS**: `SIGABRT` — `feed(search)` walks the freed terminal instead of returning `InvalidValue` |
| NC2 | drop the reserved-value `if !value.is_null() { return InvalidValue }` in `set(.select_next/.select_prev)` | `search_select_rejects_a_non_null_reserved_value` | **FAILS**: `left: InvalidValue, right: NoValue` |
| NC3 | drop `searches.remove(&search_)` in `search::free` (freed search stays registered) | `search_freed_before_terminal_detaches_from_the_registry` under Miri | **UB**: `in-bounds pointer arithmetic failed: alloc… has been freed, so this pointer is dangling` (terminal free detaching the freed search `a`) |

## 5. Headers

Rename transform re-derived by diffing an existing pair
(`include/ghostty/vt/selection.h` vs `include/rustty/vt/selection.h`): a pure per-token
rename `Ghostty→Rustty`, `ghostty→rustty`, `GHOSTTY→RUSTTY` (no other edits). Applied
mechanically:

- `include/rustty/vt/search.h` NEW = `sed` of the oracle: `diff` against the transformed
  oracle → empty.
- `vt/selection.h` (`RusttySelectionBuffer`), `vt/types.h` (`RusttySearch`) → `diff`
  against the transformed oracle → empty.
- `vt.h` (the `@ref search`, `@ref c-vt-search` example, `@example c-vt-search` block,
  `#include <rustty/vt/search.h>`) → `diff` against the transformed oracle shows ONLY the
  pre-existing rustty-only lines (the `rustty.sh` URL and the `color_scheme` entries that
  predate this range).
- `.port/tools/oracle-ghostty-h.sha256` pins `include/ghostty.h` (the embed umbrella,
  `check_rustty_h_abi.sh`); the range does not touch that file
  (`shasum -a 256 ../ghostty/include/ghostty.h` = `de852e83…72b29` = the pin) — unchanged,
  and `.port/audits/CABI_SYMBOLS.tsv` (the embed-ABI manifest) is therefore also unchanged
  (`check_symbol_manifest.sh` GREEN 91/91 before and after).

## 6. Gate before/after

| gate | before (at `ca2eb64d`) | after |
|---|---|---|
| A5 `check_rustty_vt_headers.sh` | arm (b) `195 symbols cross-checked, floor 195`; arm (d) **MISMATCH** — 8 oracle exports with no counterpart (`rustty_search_{feed,free,get,get_multi,new,run,set,tick}`); (c)/(e) not reached | **GREEN**: (b) `203 symbols cross-checked, floor 203` (floor re-pinned upward 195→203, monotone); (d) `OK (203 oracle exports checked; 0 documented residual(s))`; (c) `61 sizeof + 249 offsetof asserts across 59 header aggregates OK [type_json registry vs oracle: 159 types, 0 residuals]`; (e) `630 enumerator discriminants across 68 enums OK` |
| `check-vt-struct-layouts.mjs` (run standalone) | **RED**: `type_json REGISTRY DIVERGES FROM THE ORACLE` — `GhosttySearch`, `GhosttySearchData`, `GhosttySearchOption`, `GhosttySearchScroll`, `GhosttySearchStatus`, `GhosttySelectionBuffer` absent | GREEN (159 types, 0 residuals; `RusttySelectionBuffer` sizeof/offsetof asserted) |
| `check-vt-enum-discriminants.mjs` | `613 enumerator discriminants across 64 enums OK` | `630 … across 68 enums OK` (+17 enumerators / +4 enums) |
| G-VTFEAT `check_vt_features.py` | **RED**: `default build exports 190, oracle all-enabled expects 198; missing=[rustty_search_*]`; 9-row matrix | **GREEN**; the feature list is now READ FROM THE ORACLE (`build_options.zig` `Features` fields, floor 9) → `10 (…, search, …)`; new row `--features search  clean build, 52 exports — exact`; `default … 198 exports — exact` |
| all-but-search build (`--no-default-features --features simd,<nine>`) | n/a | `rustty_search_*` exports: **0**; total `rustty_*`: 190 |
| G3b `check_comment_coverage.py` `[vt-c]` | `distinctive ref comments=671 missing=4` (the `CSelectionBuffer` doc) | `missing=0 review=1` (the REVIEW row is the pre-existing formatter sentence); scoped `--ref-files terminal/c/search.zig --port-glob crates/rustty-vt-c/src/search.rs` → `68 missing=0`; scoped `c/selection.zig,c/terminal.zig,c/types.zig` → `331 missing=0` |
| `check_symbol_manifest.sh` | `GREEN: 91 header exports == 91 TSV rows` | same (embed ABI untouched) |
| `cargo test -p rustty-vt-c` | 469 | **486** (`486 passed; 0 failed`) |
| `cargo build --workspace` | — | clean |
| `cargo clippy -p rustty-vt-c -p rustty-vt --all-targets -- -D warnings` | — | clean |
| `rustfmt --check` on touched files | — | clean |
| G1b `check_test_counts.py` `vt_c` row | globs `c/*.zig` (already saw `c/search.zig`, green by slack) | 17 named tests now exist (§3); per-test G1 resolver forms `test "a b c"` → `fn a_b_c` |

Gate edits (both tighten, neither weakens): `check_vt_features.py` derives the token
list from the oracle instead of a hand list (a new upstream feature is now a new matrix
row + a RED until declared); `check_rustty_vt_headers.sh` `VT_BOTH_FLOOR` 195 → 203 with
its P6-W3 note.

## 7. Example build/run

`sh example/c-vt-search/build.sh` (release, `identity-oracle simd`) — built and RAN:

```
3 matches for "error"
selected 1 of 3
selected 2 of 3
selected 3 of 3
highlight rows 2-2, cols 22-26
highlight rows 3-3, cols 11-15
highlight rows 4-4, cols 10-14
```

Hand-checked against the program's input: `error` occurs at row 2 col 22 ("compiling
module B... error: …"), row 3 col 11 ("linking... error: …") and, case-insensitively, row 4
col 10 ("$ grep -n ERROR …"); newest-first selection wraps after 3; all three share the
viewport page. `.port/tools/cvt-parity.sh c-vt-search` is **BLOCKED oracle-side**: the
oracle example (`zig build run`) panics `unable to find artifact 'ghostty-vt'` (only
`ghostty-vt-static` is exposed to dependents at this pin) with zig 0.16.0_1, and the pin
refuses zig 0.15.2 (`src/build/zig.zig: Your Zig version v0.15.2 does not meet the
required build version of v0.16.0`). The sibling `example/c-vt-selection` fails
identically in the oracle tree, so this is the harness's environment at the new pin, not
a W3 defect. The oracle tree was not modified (`git -C ../ghostty status --short` → empty
before and after; zig writes only its ignored caches).

## 8. Open items

- `cvt-parity.sh` for every c-vt example is blocked by the oracle-side artifact name at
  this pin (§7) — coordinator/W7 item (either the harness learns `ghostty-vt-static` /
  a `-D` flag, or it stays a documented BLOCKED, never a green).
- The pre-existing cbindgen `Data` fold in `include/rustty.h` (three Rust `Data` enums
  → one C `Data`; `const Data *keys` on the render/gesture `get_multi` declarations refers
  to the kitty one) is untouched and unrelated to W3; W3 avoided adding to it by naming
  its enums `Search*`.
- G1b file-list extension for `c/search.zig` — coordinator (the row already globs it).
