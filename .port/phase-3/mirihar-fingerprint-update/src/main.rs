//! `P3-W11 T3` (`P3-W6 T6` Step 2) — the aliasing story for
//! `search::viewport::Fingerprint::update`, proved BEFORE the fast path was
//! written.
//!
//! ORACLE: `9659167ec` rewrote `Fingerprint.init` + `eql` into a single-pass
//! `update` that reuses the retained buffer. `bc8bb6c0f` then reverted part of
//! it with the commit message:
//!
//!   > Using `clearRetainingCapacity` as it was being used in
//!   > Fingerprint.update produces undefined behavior; both the old "copy" and
//!   > current version of the fingerprint would still be using the same
//!   > storage.
//!
//! The question this harness answers is narrow and load-bearing: **does that
//! UB claim survive translation into Rust, and if so which of the two shapes
//! is safe to ship?** The port has to choose one; guessing is not acceptable
//! for a shape upstream itself labelled UB.
//!
//! Isolated, non-FFI reproduction of the exact borrow structure: a
//! `Vec<Entry>` whose capacity is reserved UP FRONT (so no reallocation can
//! occur inside the loop — that ordering is `9659167ec`'s allocation-failure
//! contract and is preserved in both candidates), then rebuilt in place while
//! the previous contents are consulted.
//!
//! Candidates, selected by **argv** — never by an environment variable, since
//! Miri does not forward host env, which is what made an earlier env-switched
//! control vacuous (`../mirihar-hashmap-shift/README.md`,
//! `../../phase-2/mirihar-chunk3b/README.md`):
//!
//!   * `snapshot_raw`  — `9659167ec` translated LITERALLY: snapshot the old
//!                       items as a raw base pointer, `clear()`, then
//!                       interleave raw reads of `old[i]` with `push`es that
//!                       write through `&mut Vec` into the same storage.
//!   * `snapshot_slice`— the same, but the snapshot is a real `&[Entry]`
//!                       (`from_raw_parts`), i.e. a tagged shared reference
//!                       held across the mutation. This is the closest Rust
//!                       analogue of a Zig slice, and it is the candidate that
//!                       can actually pop a tag.
//!   * `reindex`       — `bc8bb6c0f`'s shape: no snapshot at all; re-read
//!                       `self.entries[i]` fresh each iteration, assign in
//!                       place when `i < len`, `push` otherwise, and
//!                       `truncate(i)` at the end. This is what the port ships.
//!
//! VACUITY GUARD: every candidate is made to actually execute the interleaved
//! read/write at the SAME index — `assert`ing the observed values — because a
//! raw read plus a raw write to a *different* element does not pop anything
//! (SB stacks are per-location) and would make the contrast meaningless. Each
//! candidate prints the `changed` flag and the entry count it computed, and
//! all three must agree; a candidate that silently skipped the loop would show
//! a different count.
//!
//! Run from this directory:
//!
//! ```sh
//! for c in snapshot_raw snapshot_slice reindex; do
//!   echo "== $c =="
//!   cargo +nightly miri run --quiet -- "$c"
//!   MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- "$c"
//! done
//! ```

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Entry {
    node: usize,
    serial: u64,
}

struct Fingerprint {
    entries: Vec<Entry>,
}

/// The "viewport" the update walks. Stands in for the `PageIterator` chunks;
/// the iterator itself is irrelevant to the aliasing question.
fn viewport(n: usize, serial_base: u64) -> Vec<Entry> {
    (0..n)
        .map(|i| Entry {
            node: 0x1000 + i,
            serial: serial_base + i as u64,
        })
        .collect()
}

impl Fingerprint {
    fn new() -> Fingerprint {
        Fingerprint {
            entries: Vec::new(),
        }
    }

    /// `9659167ec` translated literally, with the snapshot as a RAW base
    /// pointer. `clearRetainingCapacity` -> `Vec::clear`.
    fn update_snapshot_raw(&mut self, chunks: &[Entry]) -> bool {
        self.entries.reserve(chunks.len().saturating_sub(self.entries.len()));

        // const old = self.entries.items;
        let old_ptr: *const Entry = self.entries.as_ptr();
        let old_len = self.entries.len();
        let mut changed = false;
        // self.entries.clearRetainingCapacity();
        self.entries.clear();

        let mut i = 0usize;
        for entry in chunks.iter().copied() {
            if !changed {
                // old[i].node != entry.node or old[i].serial != entry.serial
                changed = i >= old_len || unsafe {
                    let o = *old_ptr.add(i);
                    o.node != entry.node || o.serial != entry.serial
                };
            }
            // self.entries.appendAssumeCapacity(entry);
            self.entries.push(entry);
            i += 1;
        }

        changed || i != old_len
    }

    /// The same, but holding a genuine `&[Entry]` across the mutation — the
    /// closest Rust analogue of the Zig slice `bc8bb6c0f` complains about.
    fn update_snapshot_slice(&mut self, chunks: &[Entry]) -> bool {
        self.entries.reserve(chunks.len().saturating_sub(self.entries.len()));

        let old_ptr: *const Entry = self.entries.as_ptr();
        let old_len = self.entries.len();
        // SAFETY (for the harness only): this is the construct under test. It
        // is a shared slice over storage that the loop below mutates.
        let old: &[Entry] = unsafe { core::slice::from_raw_parts(old_ptr, old_len) };
        let mut changed = false;
        self.entries.clear();

        let mut i = 0usize;
        for entry in chunks.iter().copied() {
            if !changed {
                changed = i >= old.len() || old[i].node != entry.node || old[i].serial != entry.serial;
            }
            self.entries.push(entry);
            i += 1;
        }

        changed || i != old_len
    }

    /// `bc8bb6c0f`'s shape — no snapshot; the previous value is re-read from
    /// `self.entries` each iteration, immediately before being overwritten.
    fn update_reindex(&mut self, chunks: &[Entry]) -> bool {
        self.entries.reserve(chunks.len().saturating_sub(self.entries.len()));

        let old_len = self.entries.len();
        let mut changed = false;

        let mut i = 0usize;
        for entry in chunks.iter().copied() {
            if !changed {
                changed = i >= old_len
                    || self.entries[i].node != entry.node
                    || self.entries[i].serial != entry.serial;
            }
            if i < self.entries.len() {
                self.entries[i] = entry;
            } else {
                self.entries.push(entry);
            }
            i += 1;
        }

        self.entries.truncate(i);
        changed || i != old_len
    }
}

fn run(name: &str, f: fn(&mut Fingerprint, &[Entry]) -> bool) {
    let mut fp = Fingerprint::new();

    // 1. First update over an empty fingerprint: everything is new.
    let v0 = viewport(4, 100);
    let c0 = f(&mut fp, &v0);
    assert!(c0, "{name}: first update must report changed");
    assert_eq!(4, fp.entries.len());

    // 2. Identical viewport: the interleaved read/write at the SAME index
    //    happens 4 times here, which is the whole point of the harness.
    let c1 = f(&mut fp, &v0);
    assert!(!c1, "{name}: identical viewport must report unchanged");
    assert_eq!(v0, fp.entries, "{name}: entries must be rebuilt exactly");

    // 3. One serial bumped in the middle: read-then-overwrite at index 2 must
    //    observe the OLD serial, or `changed` would be missed.
    let mut v1 = v0.clone();
    v1[2].serial += 1;
    let c2 = f(&mut fp, &v1);
    assert!(c2, "{name}: a changed serial must be detected");
    assert_eq!(v1, fp.entries);

    // 4. Shorter viewport: exercises the shrink path.
    let v2 = viewport(2, 100);
    let c3 = f(&mut fp, &v2);
    assert!(c3, "{name}: a shorter viewport must report changed");
    assert_eq!(v2, fp.entries);

    // 5. Same short viewport again: unchanged, and no growth.
    let c4 = f(&mut fp, &v2);
    assert!(!c4, "{name}: repeated short viewport must report unchanged");

    println!("{name}: len={} ok", fp.entries.len());
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    match which.as_str() {
        "snapshot_raw" => run("snapshot_raw", Fingerprint::update_snapshot_raw),
        "snapshot_slice" => run("snapshot_slice", Fingerprint::update_snapshot_slice),
        "reindex" => run("reindex", Fingerprint::update_reindex),
        other => {
            eprintln!("unknown candidate {other:?}; expected one of: snapshot_raw snapshot_slice reindex");
            std::process::exit(2);
        }
    }
}
