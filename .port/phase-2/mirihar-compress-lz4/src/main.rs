//! Isolated Miri harness for the WS7 raw LZ4 block codec (Phase A / task A6).
//!
//! Why this exists: `cargo +nightly miri test -p rustty-vt` must first build the
//! whole 2073-test `rustty-vt` test binary under Miri, which on this host takes
//! well over an hour before a single codec test runs — the codec's Miri evidence
//! is hostage to an unrelated, unbounded build. The codec is a dependency-free
//! leaf module, so the established `.port/phase-2/mirihar-*` pattern applies:
//! interpret ONLY the codec, in seconds instead of hours.
//!
//! This harness includes the REAL source file via `#[path]`. It is not a copy;
//! if `compress/lz4.rs` changes, this harness interprets the change.
//!
//! What Miri actually proves here: the codec is `unsafe`-free, so there are no
//! raw pointers whose provenance could be violated. Miri's load-bearing findings
//! for this module are therefore (a) no out-of-bounds slice access on hostile
//! input, (b) no uninitialised reads, (c) no integer UB, and (d) no leaks
//! (Miri's leak checker runs at exit). Scale is deliberately small — Miri is
//! ~100-400x native and the FULL-scale workloads are covered by the native
//! release/debug lanes.
//!
//! Run (from this directory):
//!
//! ```sh
//! cargo +nightly miri run --quiet                                   # Stacked Borrows
//! MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run --quiet     # Tree Borrows
//! ```

#[path = "../../../../crates/rustty-vt/src/compress/lz4.rs"]
mod lz4;

use lz4::{compress, compress_bound, decompress, DecompressError, HashTable};

/// xorshift64, matching the deterministic generator style used by the codec's
/// own tests so the harness exercises comparable input shapes.
struct Prng(u64);
impl Prng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn round_trip(input: &[u8], table: &mut HashTable) -> Vec<u8> {
    let mut encoded = vec![0u8; compress_bound(input.len()).unwrap()];
    let n = compress(input, &mut encoded, table).unwrap();
    encoded.truncate(n);

    let mut decoded = vec![0u8; input.len()];
    assert_eq!(decompress(&encoded, &mut decoded), Ok(input.len()));
    assert_eq!(decoded, input, "round trip must be the identity");
    encoded
}

fn main() {
    // A single reused HashTable, exactly as PageList's CompressionScratch will
    // hold it in Phase B: the table must be safe to reuse across blocks.
    let mut table: HashTable = [0; 4096];
    let mut rng = Prng(0x4C5A_3401);

    // --- 1. Structured page-shaped data (8-byte cell records, mostly zero). ---
    // This is the codec's actual production input shape.
    let page_len = 4096usize;
    let mut structured = vec![0u8; page_len];
    for i in 0..page_len / 8 {
        structured[i * 8] = (b' ' as usize + i % 95) as u8;
        structured[i * 8 + 4] = ((i / 80) % 16) as u8;
    }
    let encoded_structured = round_trip(&structured, &mut table);

    // --- 2. Maximum-ratio case: all zeros (long matches, wide copy paths). ---
    round_trip(&vec![0u8; page_len], &mut table);

    // --- 3. Text-like data (short, frequent matches). ---
    let mut text = Vec::with_capacity(page_len);
    while text.len() < page_len {
        text.extend_from_slice(b"the quick brown fox jumps over the lazy dog\n");
    }
    text.truncate(page_len);
    round_trip(&text, &mut table);

    // --- 4. Incompressible data (literal-only path, expansion near the bound). ---
    let incompressible: Vec<u8> = (0..page_len).map(|_| rng.next() as u8).collect();
    round_trip(&incompressible, &mut table);

    // --- 5. Overlapping-match path: period-1..8 runs drive copy_match's
    //        overlapping wide-copy branches, the codec's sharpest edge. ---
    for period in 1..=8usize {
        let mut periodic = vec![0u8; 512];
        for (i, b) in periodic.iter_mut().enumerate() {
            *b = (i % period) as u8;
        }
        round_trip(&periodic, &mut table);
    }

    // --- 6. Boundary sizes around every encoding transition. ---
    for len in [0usize, 1, 3, 4, 5, 11, 12, 13, 14, 15, 16, 254, 255, 256, 269, 270, 271] {
        let input: Vec<u8> = (0..len).map(|i| (i % 3) as u8).collect();
        round_trip(&input, &mut table);
    }

    // --- 7. Exact-size contract: both a too-small and a too-large output
    //        buffer must be rejected, never silently accepted. ---
    {
        let mut too_small = vec![0u8; structured.len() - 1];
        assert!(matches!(
            decompress(&encoded_structured, &mut too_small),
            Err(DecompressError::OutputTooSmall) | Err(DecompressError::OutputSizeMismatch)
        ));
        let mut too_large = vec![0u8; structured.len() + 1];
        assert_eq!(
            decompress(&encoded_structured, &mut too_large),
            Err(DecompressError::OutputSizeMismatch)
        );
    }

    // --- 8. THE property that needs an interpreter: decoding arbitrary bytes.
    //        Truncations and single-byte corruptions must terminate without any
    //        out-of-bounds access. Any Result is acceptable; UB is not. ---
    {
        let mut decoded = vec![0u8; structured.len()];

        for k in 1..16 {
            let cut = encoded_structured.len() * k / 16;
            let _ = decompress(&encoded_structured[..cut], &mut decoded);
        }

        for _ in 0..64 {
            let mut corrupted = encoded_structured.clone();
            let pos = (rng.next() as usize) % corrupted.len();
            corrupted[pos] ^= (rng.next() as u8) | 1;
            let _ = decompress(&corrupted, &mut decoded);
        }

        // Fully random "blocks": no structure at all.
        for _ in 0..64 {
            let len = 1 + (rng.next() as usize) % 96;
            let garbage: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
            let _ = decompress(&garbage, &mut decoded);
        }
    }

    println!("mirihar-compress-lz4: all codec paths executed with no UB");
}
