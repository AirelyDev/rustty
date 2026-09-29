# P7-W1 T2: the hash map's cached key/value pointers

This is an isolated, non-FFI Miri harness for upstream `6112935a2` ("hash map keeps its capacity
and entry pointers in the struct"). Before that commit, a `HashMapUnmanaged` value cached only
`metadata`, and it re-derived `keys`/`values` from offsets stored in the in-buffer header on every
access. Since then the value caches all three pointers, derived once by `OffsetHashMap::map(base)`.
Every page method keeps that map value alive across other accesses to the page memory: the
hyperlink set release, the grapheme alloc, and `getOffset`. So the derivation of `base` is
load-bearing. This harness proves the rule behind the `.port/audits/GHOSTTY_LIFETIMES.tsv` rows
(`HashMapUnmanaged` `metadata`/`keys`/`values`, P7-W1 T2): the pointers must derive from the page's
raw base (`Offset::ptr`), never from a `&mut` projection. See memories
`rust-mut-args-strongly-protected` and `shaper-hook-sb-clean-interleave`.

## The model

- **Hash map.** It is the REAL `crates/rustty-vt/src/hash_map.rs`, included by `#[path]` at its
  post-`6112935a2` end state. It is not a copy.
- **Page functions.** They mirror `crates/rustty-vt/src/page_impl.rs` statement for statement:
  - `set_hyperlink_inner`
  - `clear_hyperlink_inner`
  - `move_hyperlink`
  - `set_graphemes_inner`
  - `append_grapheme_inner` (the new-slice arm and the spare-chunk-space arm)
- **Where the page functions read the base.** Each place where the reference reads `self.memory`
  is written `mem!(self)`. The Rust port hoists this into a single `let base = self.memory_ptr();`.
  In `raw` mode the hoist produces the same pointer with the same tag, so it changes nothing.
- **Grapheme alloc and hyperlink set.** They keep the real access shape: a bitmap word plus a chunk
  array, and a ref-count array, all inside the page memory and reached through `base`.
- **Page memory.** It is zero-filled (`alloc_zeroed`), and the maps are set up with
  `OffsetHashMap::init_assume_zeroed`, the way `d2ff6d77a`'s page init does it.
- **Seeding.** Every scenario is seeded with the `raw` derivation, so the operation under test is
  the only thing that differs between the two modes.

The candidates are named `<mode>_<scenario>`.

- **mode `raw`:** `base` is the page's raw `memory_ptr` (the shipped port).
- **mode `mutref`:** `base` is `self.memory_mut().as_mut_ptr()`, a `&mut Page` → `&mut [u8]`
  projection that is re-evaluated at each reference `self.memory`.
- **scenarios:**
  - `set_hyperlink`: found_existing → release → write through `gop.value_ptr`.
  - `clear_hyperlink`: entry → release → `removeByPtr`.
  - `set_graphemes`: map → alloc → `getOffset` → `putNoClobber`.
  - `append_grapheme`: both arms.
  - `move_hyperlink`: a single map value with no other page-memory access in between (control).
  - `held_across`: a map value handed around by value and held across a page mutation of the same
    map. This is the pattern of the terminal test "VS16 widening when the spacer tail grows the
    page" and of the inspector page widget, and it is why `Header` keeps the size counter in the
    buffer.

## Result

```
candidate                  SB     TB
mutref_set_hyperlink       UB     CLEAN
mutref_clear_hyperlink     UB     CLEAN
mutref_set_graphemes       UB     CLEAN
mutref_append_grapheme     UB     CLEAN
mutref_move_hyperlink      CLEAN  CLEAN
mutref_held_across         UB     UB
raw_set_hyperlink          CLEAN  CLEAN
raw_clear_hyperlink        CLEAN  CLEAN
raw_set_graphemes          CLEAN  CLEAN
raw_append_grapheme        CLEAN  CLEAN
raw_move_hyperlink         CLEAN  CLEAN
raw_held_across            CLEAN  CLEAN
```

Every `mutref` UB fires on a cached map pointer, in `hash_map.rs` or on `gop.value_ptr`. None of
them fires on the harness's own slices:

| Scenario | Where the UB fires |
| --- | --- |
| `set_hyperlink` | the write through `gop.value_ptr`, which is `values.add(idx)` |
| `clear_hyperlink` | `removeByIndex`'s read of `keys[j]` |
| `set_graphemes` / `append_grapheme` | the `putNoClobber` probe of `metadata` |
| `held_across` | `count()`'s read of the header |

**Why TB accepts four `mutref` rows that SB rejects.** Under Stacked Borrows, each fresh `&mut
[u8]` retag of the whole page memory pops every tag derived from the previous one, at every
location. Tree Borrows is per-location and lazy: an unprotected reborrow performs no access. The
release, alloc and `getOffset` interleavings touch the set items and grapheme chunks, which are
disjoint from the map's header and arrays. `held_across` writes the same map, the header and the
slot arrays, through the second derivation. Tree Borrows then disables the held value's tag at
those locations, and the next read is UB under both models. The `raw` derivation is clean under
both models in every scenario, because all the pointers are SharedRW siblings from the one raw
base.

## Run

```sh
bash run-all.sh      # 12 candidates, SB + TB (seconds)
cargo +nightly miri run -q -- raw_held_across
```
