# Backward-shift deletion pointer-invalidation Miri harness

`P3-W2` **Task 1 Step 3 negative control**, and the acceptance test referenced by
`P3-W2` Task 3 Step 4. It exists to answer one question: is
`.port/phase-3/HASHMAP_CALLER_AUDIT.md` **necessary** or merely precautionary?

Isolated, **non-FFI** reproduction of the borrow structure of rustty's page hyperlink map
(`crates/rustty-vt/src/hash_map.rs` + `crates/rustty-vt/src/page_impl.rs`): one backing
allocation holding `[Header][metadata; cap][keys; cap][values; cap]`, with `metadata`,
`keys()` and `values()` all materialized as RAW pointers off the same base — the shape area
D §4.5 (`.port/phase-3/assessments/D-terminal-subsystems.md:421`) requires be preserved.

Each candidate is one executed path (Miri only checks executed code), selected by **argv**.
Not by an environment variable: Miri does not forward host env, which is what made an earlier
env-switched control vacuous (`../mirihar-chunk3b/README.md`).

Run (from this dir):

```sh
for c in tombstone_hold backshift_hold backshift_nohold; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                            # Stacked Borrows (default)
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c"   # Tree Borrows
done
```

## Results (the load-bearing evidence), 2026-07-30

| candidate | models | SB | TB |
|---|---|---|---|
| `tombstone_hold` | value pointer held across a removal, **today's** tombstone removal (single-slot clear) | **CLEAN** (`tombstone_hold: 301`) | **CLEAN** |
| `backshift_hold` | the same caller against `fedd42e8d`'s backward-shift removal | **UB** | **UB** |
| `backshift_nohold` | backward-shift, value copied out before the removal and the pointer re-fetched after — the idiom `page_impl.rs` ships (audit rows 4/6/11/12) | **CLEAN** (`backshift_nohold: 301`) | **CLEAN** |

The contrast between rows 1 and 2 is the proof that the audit was necessary rather than
precautionary: the *same caller source* is sound under the old removal and UB under the new one.
Row 3 is the acceptance test for the shipped idiom.

### `backshift_hold` — Stacked Borrows

```
error: Undefined Behavior: attempting a read access using <1943> at alloc877[0x5c], but that tag does not exist in the borrow stack for this location
   --> src/main.rs:215:5
    |
215 |     *held += 1;
    |     ^^^^^^^^^^ this error occurs as part of an access at alloc877[0x5c..0x60]
help: <1943> was created by a Unique retag at offsets [0x5c..0x60]
   --> src/main.rs:208:35
    |
208 |     let held: &mut u32 = unsafe { &mut *map.get_ptr(30).unwrap() };
help: <1943> was later invalidated at offsets [0x5c..0x60] by a write access
   --> src/main.rs:180:13
    |
180 |             *values_ptr.add(hole) = 0;
```

### `backshift_hold` — Tree Borrows

```
error: Undefined Behavior: read access through <1890> at alloc877[0x5c] is forbidden
   --> src/main.rs:215:5
    |
215 |     *held += 1;
    = help: the accessed tag <1890> has state Disabled which forbids this child read access
help: the accessed tag <1890> was created here, in the initial state Reserved
   --> src/main.rs:208:35
    |
208 |     let held: &mut u32 = unsafe { &mut *map.get_ptr(30).unwrap() };
```

## Why the UB is real and not an artifact

Stacked Borrows is **per-location**. Tombstone removal writes only `metadata[idx]` and the
header — never the value slot the caller is holding — so the caller's `Unique` tag survives.
Backward-shift removal compacts the probe cluster and ends by clearing the *final* hole, which
in this arrangement is precisely the slot the caller holds; that write goes through a
raw pointer derived from the parent allocation and pops the `Unique` tag.

Note the corollary, which is why the harness reborrows `&mut` rather than only reading through a
raw `*mut`: a bare raw-pointer read after the shift would produce the **wrong value** but would
not be flagged by Miri, because raw siblings off one base share a tag. The UB appears exactly
where the port takes a `&mut` reborrow of a map value pointer — which it does at
`page_impl.rs:2341` (audit row 9, the one long-lived pointer). That row is safe today only
because nothing mutates the map in its window.

## Cluster arrangement

`home(key) == 0` for every key, so inserts 10/20/30 occupy slots 0/1/2. Removing key 10 leaves
`hole = 0`; slot 1 (key 20) is cyclically within `[home, j)` so it moves to 0 and the hole
becomes 1; slot 2 (key 30) likewise moves to 1 and the hole becomes 2; slot 3 is free so the
scan stops and slot 2 — the held location — is cleared.
