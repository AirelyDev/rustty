# Per-item caller-reachability triage

`P3-W0 T6`. The assessments assign severity without uniformly proving
reachability. This file replaces inherited severity with per-item caller
evidence, and it — not the assessments — sets `P3-W1`'s task order.

**Verdicts.**

* **LIVE** — reachable from a public API with an argument domain that hits the
  defect. A LIVE row requires a `#[test]` reproducer that fails on today's
  `main`. No reproducer, no LIVE verdict.
* **LATENT** — the defect is real and directly reachable by calling the function
  with an out-of-domain argument, but no production caller supplies one. A
  LATENT row requires a direct-call `#[test]` that *does* trip it, so the
  verdict is falsifiable rather than convenient.
* **UNREACHABLE** — no caller constructs the state at all.
* **UNTRIAGED** — reachability not established here. Recorded as such rather
  than guessed. `P3-W1` must close these before scheduling them.

The direct-call controls for the LATENT rows live in
`crates/rustty-vt/src/pagelist/pin.rs`, module
`p3_reachability_negative_controls`.

---

## The table

| # | rustty site | claimed trigger | ALL callers (file:line) | argument domain at each caller | reachable from a public API? | verdict | evidence |
|---|---|---|---|---|---|---|---|
| 1 | `pagelist/pin.rs:494` `right_wrap` | `extra % cols - 1` underflows on an exact row multiple | `selection_gesture.rs:910,915,925,930` (all four) | `n = 1`, literal, at every one | **no** at cols >= 2 | **LATENT** | `n = 1` gives `extra_after_remaining = 1`, and `1 % cols == 1` for every `cols >= 2`, so the `- 1` never sees 0. Direct-call control `p3_right_wrap_exact_row_multiple_underflows` (n = 159 on an 80-col list) panics "attempt to subtract with overflow" on today's tree. **Refinement the plan's analysis missed: at `cols == 1`, `1 % 1 == 0` and `n = 1` DOES reach it.** Whether a 1-column terminal is constructible is not established here — see row 1b. |
| 1b | as above, at `cols == 1` | same | same four | same | **unknown** | **UNTRIAGED** | The `n = 1` argument that makes row 1 LATENT stops protecting at `cols == 1`. Nothing in this triage establishes whether a 1-column `PageList`/selection gesture is constructible. `P3-W1` must resolve this **before** downgrading row 1; if a 1-column terminal is reachable, row 1 is LIVE and schedules first. |
| 2 | `pagelist/pin.rs:468` `left_wrap` | symmetric | same four | `n = 1` | **no** at cols >= 2 | **LATENT**, and *not* a panic | `x = cols - extra % cols` yields `x == cols` on an exact row multiple — **one past the last valid column**, a silently invalid pin rather than an overflow. Control `p3_left_wrap_exact_row_multiple_yields_out_of_range_x` asserts `got.x == 80` on an 80-column list today. Same `cols == 1` caveat as row 1b. |
| 3 | `pagelist/iter.rs:489` reverse `Limit::Count` | `row.y == 0` -> `row.y - 1` | **none** | — | **no** | **UNREACHABLE** | `grep -rn 'Limit::Count(' crates/ --include='*.rs'` finds **only the two `match` arms** (`iter.rs:382`, `iter.rs:476`). The only `Limit` values ever *constructed* in the tree are `Limit::None` (`access.rs:681,1036`) and `Limit::Row`. Nothing builds a `Limit::Count`, so the whole reverse-count branch is dead code today. Still worth fixing for fidelity; **schedules last**, and any fix must add a constructor path or it cannot be tested. |
| 4 | `pagelist/scroll.rs:253,346` | `isize::MIN` delta: `(-n) as usize` / `-delta` | production: `rustty-inspector/src/widgets/pagelist.rs:837` `pages.scroll(Scroll::DeltaRow(delta_row))`. All other call sites are `pagelist/tests.rs` | inspector UI delta; test literals in -100..=5 | **only via the inspector** | **UNTRIAGED** | The single production caller is the inspector's pagelist widget. `isize::MIN` reachability depends on the provenance of that widget's `delta_row`, which this triage did not follow into the inspector's scroll accounting. Recorded, not guessed. `P3-W1` must close it; the fix (`unsigned_abs()`, per `REVIEW_CLASSES.md` §1) is cheap enough that it may be worth doing regardless of the verdict. |
| 5 | `terminal.rs:1764` `debug_assert!(!brk)` | mode 2027 off -> base + U+200B -> 2027 on -> U+0301 | reached from `print_slice` / `print` | arbitrary VT input | **yes, if the sequence drives it** | **UNTRIAGED** | The site is at `terminal.rs:1764`, **not `:970`** as the plan states (`:970` is `print_slice`'s doc block). The claimed trigger is a four-step mode-toggle sequence and the plan is explicit that it must be driven through `Stream` end to end, not by calling `print` directly. That end-to-end reproducer was **not written** in this lane. Because a `debug_assert` is compiled out in release, the release-mode consequence also needs stating before severity can be assigned. |
| 6 | `pagelist/grow.rs:495` `adjust_field!` | `increase_capacity` from a zero capacity dimension | production callers: `terminal.rs:4042,4337,9764`; `screen.rs:1121,1127,2887,3053,3146,3278,3291` (10 sites) | the four `IncreaseCapacity` tags; the *capacity* comes from the page, not the caller | **yes** — the precondition is a page state, not an argument | **UNTRIAGED** | Unlike rows 1-4 this is not an argument-domain question: reaching it needs a page whose capacity in the adjusted dimension is already **0**, which `compact` (`grow.rs:610`) / `exact_row_capacity` (`page_impl.rs:898`) can in principle produce. Establishing that requires reasoning about those two functions, which this triage did not do. **The consequence is NOT a panic** — `0 * 2 == 0` so the dimension never grows, and the unbounded-retry callers (`start_hyperlink`, `cursor_set_hyperlink`, the reflow probes, `insert_lines`/`delete_lines`) spin allocating a page per iteration, while the single-retry style/grapheme paths lose data silently. A control for that needs a bounded iteration counter, not `#[should_panic]`. |
| 7 | `pagelist/access.rs:1095` `screen_point` | scrollback > 65,535 physical rows overflows `CellCountInt` | **tests only** — every call site is `pagelist/tests.rs` (30+ sites); no production caller | test-constructed lists | **no** | **LATENT (test-only)** | The function's own doc says "This is REALLY expensive/slow so it isn't pub … built for debugging and tests", and the caller census agrees: `grep -rn 'screen_point(' crates/ --include='*.rs'` returns only `pagelist/tests.rs`. A 65k-row scrollback would also have to be built first. Lowest priority of the nine. |
| 8 | `search/sliding_window.rs:1165` empty needle | `ascii_index_of_ignore_case` returns `Some(0)` where Zig returns `null` — "claimed strictly worse than the pin" | internal to the sliding window | the window "never passes an empty needle" per the PORT NOTE | — | **NOT A DEFECT — the claim is wrong; the COMMENT is the bug** | Zig's `std.ascii.indexOfIgnoreCase` delegates to `indexOfIgnoreCasePos(haystack, 0, needle)`, whose empty-needle result is `start_index`, i.e. **`0`, not `null`** (the `mem.indexOfPos` contract it mirrors). rustty's `Some(0)` therefore **matches** the oracle. What is wrong is rustty's own PORT NOTE two lines above, which asserts "Returns null for an empty needle per the std contract used by the reference" and directly contradicts the code beneath it. Action: **fix the comment, not the code** — a one-line change, owner `P3-W1`. Do not "fix" this to return `None`; that would be the actual divergence. |
| 9 | `search/screen.rs:822,1409` cached-highlight walk | resize between feeds frees nodes, leaving dangling cached nodes | `reload_active` and the cached-match walk | — | — | **UNTRIAGED — but see the standing finding** | Area D calls this its highest-severity gap. It is **not independent** of the `search-screen-sb-ub-finding` already on record: the first full vt Miri sweep found REAL Stacked-Borrows UB in 9 tests in this subsystem, caused by search objects holding raw `Screen` aliases invalidated by `&mut Screen`. The fix approach is documented there and **not yet applied**, and the G6 vt Miri lane stays RED until it is. This row should be scheduled as part of that fix, not as a separate item, and its reproducer already exists in the form of those 9 Miri failures. |

---

## What this changes about `P3-W1`'s order

Area A §4's "Wave A1" recommendation leads with `pin.rs:494` as "the
highest-severity single finding in area A". The evidence does not support
scheduling it first:

| rank | item | why here |
|---|---|---|
| 1 | row 9 — the search-screen SB UB | the only row with a **reproducer that already exists and already fails** (9 Miri tests), and the only one holding a gate RED (`G6` vt lane). Everything else is documentation until this is fixed. |
| 2 | row 8 — the `sliding_window.rs` comment | one line, no code change, removes a false claim from the source. Cheapest possible correction of the record. |
| 3 | row 1b — establish whether `cols == 1` is constructible | this is the gate on rows 1 and 2. If it is constructible, rows 1/2 become LIVE and jump to the top; if not, they stay LATENT and drop to rank 6. Cheap to answer, high information. |
| 4 | row 6 — `grow.rs:495` | the only row whose consequence is an unbounded allocation loop with 10 production call sites. Even UNTRIAGED, its blast radius dominates the LATENT rows. |
| 5 | row 4 — `scroll.rs` `isize::MIN` | the fix (`unsigned_abs()` + a `debug_assert`, per `REVIEW_CLASSES.md` §1) is smaller than the triage that would justify deferring it. |
| 6 | rows 1, 2 — `pin.rs` wrap underflow / out-of-range x | LATENT with proven direct-call controls. Real, but not "a live crash hotfix", and shipping it as one would misstate the evidence. |
| 7 | row 5 — `terminal.rs:1764` | needs an end-to-end `Stream` reproducer before it can be ranked at all; a `debug_assert` also has no release-mode effect. |
| 8 | row 7 — `screen_point` | test-only caller census; needs a 65k-row scrollback to reach. |
| 9 | row 3 — `iter.rs:489` | dead code. Fix for fidelity, last, and note that it cannot be tested without also adding a `Limit::Count` constructor. |

## Incomplete

Rows **1b, 4, 5, 6, 9** are marked UNTRIAGED and are **not** closed by this
lane. That is a deliberate refusal to convert "I did not check" into a verdict:
four of the nine seed items needed evidence this triage did not gather (inspector
scroll provenance, `compact`/`exact_row_capacity` capacity reasoning, an
end-to-end `Stream` drive, and the search-subsystem Miri fix). `P3-W1` owns
closing them, and the ordering above is explicitly conditional on rows 1b and 4.
