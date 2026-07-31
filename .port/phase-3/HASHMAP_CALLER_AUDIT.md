# Hash-map key/value pointer lifetime audit (`P3-W2` Task 1)

**Prerequisite gate for `P3-W2` Task 3** (`fedd42e8d`, backward-shift deletion). This is not
documentation: Task 3 may not start until every row below is either *"no pointer survives a
removal"* or carries a rewrite plan.

## Why

`fedd42e8d` replaces tombstones with backward-shift deletion (Knuth vol. 3, §6.4, algorithm R).
Upstream states the new contract in the file header
(`git -C ../ghostty show fedd42e8d:src/terminal/hash_map.zig`, lines 36-40):

> Pointer stability: insertion never moves existing entries, but removal may move *other* entries
> within a probe cluster. Any key or value pointers previously returned by the map must be
> considered invalidated by any removal.

Before `fedd42e8d`, removal wrote a tombstone byte into one slot and touched nothing else, so a
`getPtr`/`getEntry` result stayed valid across an unrelated removal. After it, removal walks the
probe cluster and *moves* entries into the hole. In Zig, using a stale entry pointer afterwards is
a logic bug that reads the wrong entry. **In rustty these pointers are raw `*mut K` / `*mut V` into
the `Page`'s single backing allocation, so the same use is undefined behaviour** — the pointee is
overwritten by a value with a different provenance history, and any `&mut` reborrow taken from it
(`append_grapheme_inner`'s `slice_ref`) has its Stacked-Borrows tag invalidated by the writes
`remove_by_index` performs through `values()`.

Area D §4.5 (`.port/phase-3/assessments/D-terminal-subsystems.md:421`) calls this out as the
*contract* hazard, distinct from the intra-`remove_by_index` aliasing hazard (three overlapping raw
views of one allocation), which Task 3 Step 1 handles by keeping `metadata`/`keys()`/`values()` as
raw pointers.

## Scope

Every site in `crates/rustty-vt` that obtains a key pointer, value pointer or `Entry` from a page
map (`Page::hyperlink_map` : `hyperlink::Map`, `Page::grapheme_map` : `GraphemeMap`). Both maps are
`AutoOffsetHashMap` instantiations, so the audit covers the whole blast radius of the contract
change. Enumerated with:

```sh
grep -n 'get_ptr\|get_entry\|fetch_remove\|remove_context\|remove_by_ptr\|get_key_ptr\|get_or_put' \
  crates/rustty-vt/src/page.rs crates/rustty-vt/src/page_impl.rs
grep -rn --include='*.rs' 'hyperlink_map\|grapheme_map' crates/
grep -rn --include='*.rs' '\.iterator()' crates/rustty-vt/src crates/rustty-bench/src crates/rustty-inspector/src
```

Findings that bound the scope:

* **No production code iterates either page map.** The only `.iterator()` calls on
  `HashMapUnmanaged` in the whole workspace are inside `hash_map.rs`'s own tests
  (`hash_map.rs:1320`, `:1452`, `:2144`). Upstream's new iterator doc comment ("Any mutation of the
  map invalidates the iterator: removal may move entries across the iteration cursor") therefore
  has no live caller to break.
* **`rustty-inspector/src/widgets/page.rs:548`** takes a map handle but only calls `count()` and
  `capacity()` — no entry pointers, no removal.
* **`rustty-bench/src/hyperlink_map.rs`** drives churn through `Page::set_hyperlink` /
  `Page::clear_hyperlink`, never through the map API directly.
* `page.rs` itself contains no map call sites; all of them are in `page_impl.rs`.

## Per-call-site rows

`lifetime` = the lifetime of every pointer this site is handed, measured against the next
`remove_by_ptr` / `remove_context` / `fetch_remove_context` on **the same map**.

| # | site (`crates/rustty-vt/src/page_impl.rs`) | fn | pointer(s) obtained | removal on same map? | lifetime vs. that removal | verdict |
|---|---|---|---|---|---|---|
| 1 | `:1822`, `:1823` | `swap_cells_inner` (grapheme arm) | 2 × `Entry{key_ptr, value_ptr}` | none | n/a — pure value swap through both `value_ptr`s, map shape untouched | **no pointer survives a removal** |
| 2 | `:1845`, `:1846` | `swap_cells_inner` (hyperlink arm) | 2 × `Entry{key_ptr, value_ptr}` | none | n/a — same shape as row 1 | **no pointer survives a removal** |
| 3 | `:1973` | `lookup_hyperlink` | none (`get_context` returns `V` **by value**) | none | n/a | **no pointer at all** |
| 4 | `:1993` → `:2003` | `clear_hyperlink_inner` | `Entry{key_ptr, value_ptr}` | yes, `remove_by_ptr(entry.key_ptr)` at `:2003` | `value_ptr` is dereferenced once at `:1999` (`let value = *entry.value_ptr`) **before** the removal; `key_ptr` is *consumed by* the removal (converted to an index on entry) and never read afterwards. Nothing is used after `:2003`. | **no pointer survives a removal** |
| 5 | `:2153` | `set_hyperlink_inner` | `GetOrPutResult{key_ptr, value_ptr}` | none — `get_or_put_context` only inserts | `value_ptr` written at `:2187`. Insertion never moves existing entries under either the old or the new contract, and the returned slot is the one just filled. | **no pointer survives a removal** |
| 6 | `:2205` → `:2207` → `:2208` | `move_hyperlink` | `Entry{key_ptr, value_ptr}` | yes, `remove_by_ptr(entry.key_ptr)` at `:2207` | `value_ptr` read into a local `value` at `:2206`, **before** the removal; `key_ptr` consumed by the removal. The following `put_assume_capacity_*` at `:2208` uses only the local copy. | **no pointer survives a removal** |
| 7 | `:2270` | `set_graphemes_inner` | none (`put_no_clobber_context` returns `Result<(), _>`) | none | n/a | **no pointer at all** |
| 8 | `:2325` | `append_grapheme_inner` (fresh-slice arm) | none (`put_no_clobber_context`) | none | n/a | **no pointer at all** |
| 9 | `:2341` → `:2374` | `append_grapheme_inner` (append arm) | `*mut OffsetSlice<u32>` from `get_ptr_context`, immediately reborrowed as `&mut OffsetSlice<u32>` (`slice_ref`, `:2341`) | none | `slice_ref` is live for the rest of the function (across `grapheme_alloc.alloc` at `:2357` and `grapheme_alloc.free` at `:2374`), but **no map mutation of any kind** occurs in that window — the allocator it calls into owns the disjoint `grapheme_alloc` region of the page, not the map's key/value arrays. | **no pointer survives a removal** — but see *Longest-lived pointer* below |
| 10 | `:2386` | `lookup_grapheme` | none from the map (`get_context` returns `OffsetSlice<u32>` **by value**); the returned `&[u32]` points into `grapheme_alloc`, not into map storage | none | n/a — invalidated by `clear_grapheme`/`append_grapheme` freeing the chunk, which is a pre-existing contract unaffected by `fedd42e8d` | **no pointer at all (into the map)** |
| 11 | `:2407` → `:2409` → `:2410` | `move_grapheme` | `Entry{key_ptr, value_ptr}` | yes, `remove_by_ptr(entry.key_ptr)` at `:2409` | identical shape to row 6: value copied out at `:2408` first, `key_ptr` consumed by the removal. | **no pointer survives a removal** |
| 12 | `:2434` → `:2441` | `clear_grapheme_inner` | `Entry{key_ptr, value_ptr}` | yes, `remove_by_ptr(entry.key_ptr)` at `:2441` | `value_ptr` dereferenced at `:2437` to obtain the codepoint slice, which is freed at `:2438` — both **before** the removal. `key_ptr` consumed by the removal. Nothing read after `:2441`. | **no pointer survives a removal** |

**Result: 12 / 12 rows resolved as "no pointer survives a removal". Zero rows need a rewrite plan;
zero rows remain unresolved. The Task 3 gate is OPEN.**

### Longest-lived pointer (the one to watch)

Row 9 is the only site where a map value pointer stays live across other work. It is safe today
*only* because nothing in that window mutates the map. Any future edit that inserts a map
`remove`/`fetch_remove` between `page_impl.rs:2341` and `:2374` becomes UB under the new contract,
silently. The `debug_assert!`-style canonical-placement invariant that Task 3 Step 3 adds to the
type is the standing detector for this class; row 9 is the reason it is carried in the type rather
than only in tests.

### Structural invariant that makes rows 4/6/11/12 safe

All four removal sites follow the same three-statement idiom, which the port should keep verbatim:

1. copy the value out of `entry.value_ptr` into a **local by value**;
2. hand `entry.key_ptr` **into** the removal call (it is converted to a slot index on entry to
   `remove_by_ptr`, before any shifting happens);
3. never touch `entry` again.

Step 2 is worth stating precisely because it looks like a violation and is not: `remove_by_ptr`
computes `idx = (key_ptr - keys()) / size_of::<K>()` as its *first* action, so the pointer is dead
before `remove_by_index` performs any write. It does mean `remove_by_ptr` must keep computing the
index before the shift loop; the shift loop must never be hoisted above it.

## Field ledger (`LIFETIMES.tsv` schema, memory `adopt-lifetimes-tsv`)

Same 7 columns as `.port/audits/GHOSTTY_LIFETIMES.tsv`
(`file / struct / field / zig_type / class / rust_type / evidence`), for the raw-pointer fields
this change touches. These are prospective rows for the `fedd42e8d` port, recorded here rather
than appended to the retroactive 2026-07-10 ledger (which is pinned at `4749c4e93`).

```tsv
src/terminal/hash_map.zig	HashMapUnmanaged	metadata	?[*]Metadata = null	BORROW_FIELD	Option<*mut Metadata> — raw ptr, NEVER &mut [Metadata]; aliases the Page allocation which also backs keys()/values()	crates/rustty-vt/src/hash_map.rs:408 — already a raw ptr; area D §4.5 requires it stay one because removeByIndexContext writes metadata[hole] while keys/values views of the same base are live
src/terminal/hash_map.zig	Header	keys	Offset(K)	BORROW_FIELD	Offset<K>; materialized on demand by keys() -> *mut K, never &mut [K]	crates/rustty-vt/src/hash_map.rs:536 — keys() returns *mut K derived from the metadata base
src/terminal/hash_map.zig	Header	values	Offset(V)	BORROW_FIELD	Offset<V>; materialized on demand by values() -> *mut V, never &mut [V]	crates/rustty-vt/src/hash_map.rs:541 — values() returns *mut V derived from the same metadata base as keys(); forming &mut [K] and &mut [Metadata] simultaneously from that base is the §4.5 hazard
src/terminal/hash_map.zig	Entry	key_ptr	*K	BORROW_PARAM	*mut K — call-scoped; INVALIDATED BY ANY REMOVAL on the same map (fedd42e8d contract). Consume it into remove_by_ptr, never read after.	crates/rustty-vt/src/page_impl.rs:2003,2207,2409,2441 — all four removal sites pass it in and never read it again (rows 4/6/11/12 above)
src/terminal/hash_map.zig	Entry	value_ptr	*V	BORROW_PARAM	*mut V — call-scoped; INVALIDATED BY ANY REMOVAL on the same map. Copy the pointee to a local before any removal.	crates/rustty-vt/src/page_impl.rs:2206,2408 — value copied by value before remove_by_ptr; page_impl.rs:2341 is the one long-lived reborrow (row 9) and has no removal in scope
src/terminal/hash_map.zig	GetOrPutResult	key_ptr/value_ptr	*K / *V	BORROW_PARAM	*mut K / *mut V — call-scoped; stable across INSERTIONS (insertion never moves existing entries), invalidated by removals	crates/rustty-vt/src/page_impl.rs:2153 — set_hyperlink_inner writes through value_ptr with no intervening removal
```

## Negative control (Task 1 Step 3)

The audit above is only worth its ink if the hazard it describes is real. `.port/phase-3/mirihar-hashmap-shift/`
is a standalone Miri harness that holds a value pointer across a removal on a page-map-shaped
offset hash map and runs it under two map implementations selected by **argv** (not an env var —
Miri does not forward host env, which is what made an earlier env-switched control vacuous; see
`.port/phase-2/mirihar-chunk3b/README.md`).

| candidate | map implementation | expected |
|---|---|---|
| `tombstone_hold` | today's tombstone removal (single-slot clear) | CLEAN |
| `backshift_hold` | prototype backward-shift removal | **UB** |
| `backshift_nohold` | prototype backward-shift, pointer re-fetched after the removal (the shipped idiom) | CLEAN |

Observed results are recorded in that harness's `README.md`.
