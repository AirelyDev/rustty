# Viewport-fingerprint `update` aliasing Miri harness

`P3-W11 T3` (= `P3-W6 T6` Step 2). It exists to answer one question before the
fast path was written: **does `bc8bb6c0f`'s UB claim survive translation into
Rust, and which of the two shapes is safe to ship?**

`9659167ec` rewrote `Fingerprint.init` + `eql` into a single-pass `update` that
reuses the retained buffer. `bc8bb6c0f` then partially reverted it:

> Using `clearRetainingCapacity` as it was being used in Fingerprint.update
> produces undefined behavior; both the old "copy" and current version of the
> fingerprint would still be using the same storage.

Isolated, non-FFI reproduction of the borrow structure: a `Vec<Entry>` whose
capacity is reserved UP FRONT (so no reallocation can occur inside the loop —
that ordering *is* `9659167ec`'s allocation-failure contract and is preserved in
every candidate), then rebuilt in place while the previous contents are
consulted.

Candidates are selected by **argv**, never by an environment variable: Miri does
not forward host env, which is what made an earlier env-switched control vacuous
(`../mirihar-hashmap-shift/README.md`).

Run (from this dir):

```sh
for c in snapshot_raw snapshot_slice reindex; do
  echo "== $c =="
  cargo +nightly miri run --quiet -- "$c"                            # Stacked Borrows
  MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c"   # Tree Borrows
done
```

## Results (the load-bearing evidence), 2026-07-31

| candidate | models | SB | TB |
|---|---|---|---|
| `snapshot_raw` | `9659167ec` literally, snapshot as a raw `*const Entry` base pointer | **CLEAN** (`snapshot_raw: len=2 ok`) | **CLEAN** |
| `snapshot_slice` | the same, snapshot as a real `&[Entry]` — the true Rust analogue of a Zig slice | **UB** | **UB** |
| `reindex` | `bc8bb6c0f`'s shape: no snapshot, re-read `self.entries[i]` each iteration, assign in place, `truncate(i)` | **CLEAN** (`reindex: len=2 ok`) | **CLEAN** |

**The first row is the most important one, and it is a near-miss.** A lane that
translated `9659167ec` the obvious way — snapshot the old items as a raw pointer
— would have got a clean Miri run and concluded that `bc8bb6c0f`'s UB claim does
not translate to Rust. That conclusion would have been an artifact of the
*translation choice*, not a property of the code: raw siblings derived off one
base share a tag, so the interleaved read never pops anything. The UB appears
only once the snapshot is a genuine reference, which is exactly what a Zig slice
is. `bc8bb6c0f` is therefore a real finding in Rust too, and the port ships
`reindex`.

### `snapshot_slice` — Stacked Borrows

```
error: Undefined Behavior: trying to retag from <2887> for SharedReadOnly permission at alloc1132[0x0], but that tag does not exist in the borrow stack for this location
   --> src/main.rs:135:32
    |
135 |                 changed = i >= old.len() || old[i].node != entry.node || old[i].serial != entry.serial;
help: <2887> was created by a SharedReadOnly retag at offsets [0x0..0x40]
   --> src/main.rs:128:38
    |
128 |         let old: &[Entry] = unsafe { core::slice::from_raw_parts(old_ptr, old_len) };
help: <2887> was later invalidated at offsets [0x0..0x40] by a Unique retag
   --> src/main.rs:130:9
    |
130 |         self.entries.clear();
```

### `snapshot_slice` — Tree Borrows

```
help: the accessed tag <2830> later transitioned to Disabled due to a foreign write access at offsets [0x0..0x10]
   --> src/main.rs:137:13
    |
137 |             self.entries.push(entry);
    = help: this transition corresponds to a loss of read permissions
```

Note that SB and TB blame *different* operations — SB blames the `clear()`
(`&mut Vec` retag), TB blames the first `push` (foreign write). Either way the
snapshot is dead before the first read.

## Vacuity guard

Every candidate executes the interleaved read/write **at the same index**, four
times, on a viewport that is unchanged — because a raw read plus a write to a
*different* element does not pop anything (SB stacks are per-location) and the
contrast would be meaningless. Each candidate additionally asserts:

* the rebuilt `entries` equals the input viewport exactly (step 2, 3, 4),
* `changed` is **false** for an identical viewport,
* `changed` is **true** when one serial in the middle is bumped — i.e. the
  read of `old[i]` really did observe the pre-overwrite value,
* `changed` is **true** when the viewport shrinks, and the buffer is truncated.

A candidate that silently skipped the loop, or whose read observed the
just-written value, fails those assertions rather than passing quietly. All
three candidates print the same final length (`len=2`).

## What the port ships

`crates/rustty-vt/src/search/viewport.rs`'s `Fingerprint::update` is `reindex`:
no snapshot of any kind, `try_reserve` first (allocation failure preserves the
old fingerprint), read-then-overwrite per index, `truncate(i)` at the end.
