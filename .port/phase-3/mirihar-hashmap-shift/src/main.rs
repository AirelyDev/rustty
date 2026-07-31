//! `P3-W2` Task 1 Step 3 — NEGATIVE CONTROL for the backward-shift deletion
//! pointer-invalidation contract (`fedd42e8d`).
//!
//! Isolated, non-FFI reproduction of the borrow structure of rustty's page
//! hyperlink map (`crates/rustty-vt/src/hash_map.rs` +
//! `crates/rustty-vt/src/page_impl.rs`): one backing allocation holding
//! `[Header][metadata; cap][keys; cap][values; cap]`, with `metadata`, `keys()`
//! and `values()` all materialized as RAW pointers off the same base — exactly
//! the shape area D §4.5 requires be preserved.
//!
//! The question this harness answers: is the audit
//! (`.port/phase-3/HASHMAP_CALLER_AUDIT.md`) necessary or merely precautionary?
//! It holds a value pointer across a removal and runs the same caller against
//! two removal implementations:
//!
//!   * `tombstone_hold`   — today's tombstone removal (single-slot clear)
//!   * `backshift_hold`   — prototype backward-shift removal (Knuth 6.4 alg. R)
//!   * `backshift_nohold` — backward-shift, pointer re-fetched after the removal
//!                          (the idiom `page_impl.rs` actually ships)
//!
//! Candidates are selected by **argv**, never by an environment variable: Miri
//! does not forward host env, which is what made an earlier env-switched control
//! vacuous (see `../../phase-2/mirihar-chunk3b/README.md`).
//!
//! Run from this directory:
//!
//! ```sh
//! for c in tombstone_hold backshift_hold backshift_nohold; do
//!   echo "== $c =="
//!   cargo +nightly miri run --quiet -- "$c"
//! done
//! ```

use std::alloc::{alloc_zeroed, dealloc, Layout};

// ---------------------------------------------------------------------------
// The map. Mirrors `HashMapUnmanaged<u32, u32, Ctx>`'s storage discipline.
// ---------------------------------------------------------------------------

const CAP: usize = 16;
const MASK: usize = CAP - 1;

/// Every key homes to slot 0 so that inserts form one probe cluster and
/// backward-shift deletion has to move entries. This is the collision shape
/// `hyperlink::Map` sees when many cells on one row share a hyperlink.
fn home(_key: u32) -> usize {
    0
}

#[repr(C)]
struct Header {
    size: u32,
}

/// Metadata byte: 0 == free, bit 7 == used, bits 0..6 == fingerprint.
/// Tombstone (the pre-`fedd42e8d` state) is the byte 1.
const META_FREE: u8 = 0;
const META_TOMBSTONE: u8 = 1;
const META_USED: u8 = 1 << 7;

struct Map {
    base: *mut u8,
    layout: Layout,
    /// Raw, as in the port: `metadata: Option<*mut Metadata>`.
    metadata: *mut u8,
}

impl Map {
    fn new() -> Map {
        let meta_start = std::mem::size_of::<Header>();
        let keys_start = meta_start + CAP;
        let vals_start = keys_start + CAP * 4;
        let total = vals_start + CAP * 4;
        let layout = Layout::from_size_align(total, 4).unwrap();
        let base = unsafe { alloc_zeroed(layout) };
        assert!(!base.is_null());
        let metadata = unsafe { base.add(meta_start) };
        Map {
            base,
            layout,
            metadata,
        }
    }

    /// `unsafe fn header(&self) -> *mut Header` — derived from the same base.
    fn header(&self) -> *mut Header {
        self.base as *mut Header
    }

    /// `unsafe fn keys(&self) -> *mut K` — derived from the same base.
    fn keys(&self) -> *mut u32 {
        unsafe { self.metadata.add(CAP) as *mut u32 }
    }

    /// `unsafe fn values(&self) -> *mut V` — derived from the same base.
    fn values(&self) -> *mut u32 {
        unsafe { self.metadata.add(CAP + CAP * 4) as *mut u32 }
    }

    fn put_no_clobber(&mut self, key: u32, value: u32) {
        let mut idx = home(key);
        while unsafe { *self.metadata.add(idx) } & META_USED != 0 {
            idx = (idx + 1) & MASK;
        }
        unsafe {
            *self.metadata.add(idx) = META_USED | 1;
            *self.keys().add(idx) = key;
            *self.values().add(idx) = value;
            (*self.header()).size += 1;
        }
    }

    fn get_index(&self, key: u32) -> Option<usize> {
        let mut idx = home(key);
        let mut limit = CAP;
        while unsafe { *self.metadata.add(idx) } != META_FREE && limit != 0 {
            let m = unsafe { *self.metadata.add(idx) };
            if m & META_USED != 0 && unsafe { *self.keys().add(idx) } == key {
                return Some(idx);
            }
            limit -= 1;
            idx = (idx + 1) & MASK;
        }
        None
    }

    /// Returns the raw value pointer, as `get_ptr_context` does.
    fn get_ptr(&self, key: u32) -> Option<*mut u32> {
        self.get_index(key)
            .map(|idx| unsafe { self.values().add(idx) })
    }

    // -- removal, the two implementations under test ------------------------

    /// PRE-`fedd42e8d`: single-slot clear. Writes `metadata[idx]` and the
    /// header only; no other slot's key or value location is touched.
    fn remove_tombstone(&mut self, key: u32) -> bool {
        let Some(idx) = self.get_index(key) else {
            return false;
        };
        unsafe {
            *self.metadata.add(idx) = META_TOMBSTONE;
            (*self.header()).size -= 1;
        }
        true
    }

    /// `fedd42e8d`: backward-shift deletion (Knuth vol. 3, §6.4, algorithm R).
    /// Writes `metadata[hole]`, `keys[hole]`, `values[hole]` for a moving hole.
    fn remove_backshift(&mut self, key: u32) -> bool {
        let Some(idx) = self.get_index(key) else {
            return false;
        };
        let metadata = self.metadata;
        let keys_ptr = self.keys();
        let values_ptr = self.values();

        let mut hole = idx;
        let mut j = idx;
        let mut limit = CAP - 1;
        while limit != 0 {
            limit -= 1;
            j = (j + 1) & MASK;
            if unsafe { *metadata.add(j) } == META_FREE {
                break;
            }
            let h = home(unsafe { *keys_ptr.add(j) }) & MASK;
            if ((hole.wrapping_sub(h)) & MASK) < ((j.wrapping_sub(h)) & MASK) {
                unsafe {
                    *metadata.add(hole) = *metadata.add(j);
                    *keys_ptr.add(hole) = *keys_ptr.add(j);
                    *values_ptr.add(hole) = *values_ptr.add(j);
                }
                hole = j;
            }
        }
        unsafe {
            *metadata.add(hole) = META_FREE;
            *keys_ptr.add(hole) = 0;
            *values_ptr.add(hole) = 0;
            (*self.header()).size -= 1;
        }
        true
    }
}

impl Drop for Map {
    fn drop(&mut self) {
        unsafe { dealloc(self.base, self.layout) };
    }
}

// ---------------------------------------------------------------------------
// Candidates
// ---------------------------------------------------------------------------

/// The caller shape under audit: obtain a value pointer, reborrow it `&mut`
/// (exactly `page_impl.rs:2341`'s `let slice_ref = unsafe { &mut *slice };`),
/// then perform a removal on the SAME map, then keep using the reborrow.
fn hold_across_removal(remove: fn(&mut Map, u32) -> bool) -> u32 {
    let mut map = Map::new();
    // One probe cluster at slots 0,1,2.
    map.put_no_clobber(10, 100);
    map.put_no_clobber(20, 200);
    map.put_no_clobber(30, 300);

    // Key 30 lands at slot 2. Hold a `&mut` into its value.
    let held: &mut u32 = unsafe { &mut *map.get_ptr(30).unwrap() };

    // Remove the head of the cluster. Backward-shift compacts 20 -> slot 0 and
    // 30 -> slot 1, then clears slot 2 — the location `held` points at.
    assert!(remove(&mut map, 10));

    // Use the held reborrow AFTER the removal.
    *held += 1;
    *held
}

/// The shipped idiom: never keep a map pointer across a removal — re-fetch.
fn nohold_across_removal(remove: fn(&mut Map, u32) -> bool) -> u32 {
    let mut map = Map::new();
    map.put_no_clobber(10, 100);
    map.put_no_clobber(20, 200);
    map.put_no_clobber(30, 300);

    // Copy the value out BEFORE the removal (rows 4/6/11/12 of the audit).
    let value = unsafe { *map.get_ptr(30).unwrap() };
    assert!(remove(&mut map, 10));

    // Re-fetch after the removal, then use.
    let refetched: &mut u32 = unsafe { &mut *map.get_ptr(30).unwrap() };
    assert_eq!(*refetched, value);
    *refetched += 1;
    *refetched
}

fn main() {
    let candidate = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: mirihar <tombstone_hold|backshift_hold|backshift_nohold>");
        std::process::exit(2);
    });

    let out = match candidate.as_str() {
        "tombstone_hold" => hold_across_removal(Map::remove_tombstone),
        "backshift_hold" => hold_across_removal(Map::remove_backshift),
        "backshift_nohold" => nohold_across_removal(Map::remove_backshift),
        other => {
            eprintln!("unknown candidate: {other}");
            std::process::exit(2);
        }
    };
    println!("{candidate}: {out}");
}
