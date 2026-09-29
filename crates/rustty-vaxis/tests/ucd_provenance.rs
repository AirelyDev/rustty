//! WS12 T4 — the vendored UCD corpus (18.0.0 since P7-W10 T2; 17.0.0 since S-10b; 16.0.0 before) is TAMPER-EVIDENT.
//!
//! `build.rs` derives this crate's entire Unicode width/grapheme table from the six
//! text files under `ucd/`. A silent edit to any of them silently changes
//! `rustty-vaxis`'s observable width and grapheme-break behaviour, and nothing else in
//! the tree would notice. This test parses the digest table out of `ucd/PROVENANCE.md`
//! and asserts each recorded SHA-256 against the actual on-disk bytes.
//!
//! NON-VACUITY: the test is falsifiable two ways, and both were executed (see
//! `.port/phase-2/VAXIS_C1B_DESIGN.md`, "WS12 T4 non-vacuity proof"):
//!   - perturb one byte of a corpus file ⇒ digest mismatch ⇒ RED;
//!   - it also asserts it found EXACTLY the six expected rows, so a PROVENANCE.md whose
//!     table got emptied, renamed or reformatted fails loudly instead of vacuously
//!     verifying zero files. A digest test that silently checks nothing is the classic
//!     vacuous gate this program has already been bitten by seven times.
//!
//! PORT NOTE: SHA-256 is implemented inline (FIPS 180-4) rather than pulled in as a
//! dependency — `rustty-vaxis` takes only `libc` + `rustty-zigimg`, and a provenance
//! test must not widen the crate's dependency surface. The implementation is
//! self-checked against the two NIST one-block/multi-block vectors before use, so a
//! broken hasher cannot silently make every digest "match".
//!
//! Miri-ignored: reads files, which Miri's isolation forbids. Recorded in
//! `.port/phase-2/MIRI_EXCEPTIONS.md`.

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// SHA-256 (FIPS 180-4), std-only.
// ---------------------------------------------------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256_hex(data: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Padding: 0x80, then zeros, then the 64-bit big-endian bit length.
    let bit_len = (data.len() as u64) * 8;
    let mut padded = Vec::with_capacity(data.len() + 72);
    padded.extend_from_slice(data);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut w = [0u32; 64];
    for block in padded.chunks_exact(64) {
        for (i, word) in w.iter_mut().take(16).enumerate() {
            *word = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = String::with_capacity(64);
    for word in h {
        for byte in word.to_be_bytes() {
            out.push_str(&format!("{byte:02x}"));
        }
    }
    out
}

/// The hasher's own gate. Without this, a broken `sha256_hex` that returned a constant
/// would make the provenance test pass for a corrupted corpus (it would just disagree
/// with the recorded digests — which is RED, so actually the danger is the reverse: a
/// hasher broken in the SAME way that produced the recorded digests). Pinning against
/// the published NIST vectors removes both failure modes.
#[test]
fn sha256_matches_nist_vectors() {
    // FIPS 180-4 Appendix B.1 (one block) and B.2 (two blocks).
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
    // Empty input exercises the "padding-only block" path.
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

// ---------------------------------------------------------------------------
// The provenance gate.
// ---------------------------------------------------------------------------

fn ucd_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ucd")
}

/// The six files `build.rs` reads. Hardcoded here as well as in PROVENANCE.md so a row
/// dropped from the markdown table cannot silently shrink what gets verified.
const EXPECTED_FILES: [&str; 6] = [
    "UnicodeData.txt",
    "extracted/DerivedEastAsianWidth.txt",
    "emoji/emoji-data.txt",
    "auxiliary/GraphemeBreakProperty.txt",
    "auxiliary/GraphemeBreakTest.txt",
    "DerivedCoreProperties.txt",
];

/// Parse the `| sha256 | file |` rows out of `ucd/PROVENANCE.md`.
fn parse_provenance() -> Vec<(String, String)> {
    let md = std::fs::read_to_string(ucd_dir().join("PROVENANCE.md"))
        .expect("ucd/PROVENANCE.md must exist — it is the R3 provenance record");
    let mut rows = Vec::new();
    for line in md.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().trim_matches('`'))
            .collect();
        if cells.len() != 2 {
            continue;
        }
        let (digest, file) = (cells[0], cells[1]);
        // A digest row is exactly 64 lowercase hex chars; this skips the header row,
        // the `|---|---|` separator, and the origin table further up the file.
        if digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            rows.push((digest.to_string(), file.to_string()));
        }
    }
    rows
}

#[test]
#[cfg_attr(miri, ignore = "reads files; Miri isolation forbids file I/O")]
fn ucd_corpus_matches_recorded_digests() {
    let rows = parse_provenance();

    // Vacuity guard #1: the table must carry exactly the six rows we expect. An empty
    // or reformatted table would otherwise verify nothing and still pass.
    assert_eq!(
        rows.len(),
        EXPECTED_FILES.len(),
        "ucd/PROVENANCE.md must record exactly {} digest rows, found {}: {:?}",
        EXPECTED_FILES.len(),
        rows.len(),
        rows.iter().map(|(_, f)| f).collect::<Vec<_>>()
    );

    // Vacuity guard #2: they must be the six files build.rs actually parses, not any
    // six files.
    for expected in EXPECTED_FILES {
        assert!(
            rows.iter().any(|(_, f)| f == expected),
            "ucd/PROVENANCE.md has no digest row for `{expected}`, which build.rs reads"
        );
    }

    for (want, rel) in &rows {
        let path = ucd_dir().join(rel);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("cannot read vendored UCD input {}: {e}", path.display()));
        assert!(
            !bytes.is_empty(),
            "vendored UCD input {} is EMPTY — build.rs would derive a table of nothing",
            path.display()
        );
        let got = sha256_hex(&bytes);
        assert_eq!(
            &got,
            want,
            "vendored UCD input {} does not match its recorded SHA-256.\n  \
             recorded: {want}\n  actual:   {got}\n\
             The width/grapheme tables are derived from this file, so this is a real \
             behavioural change. If the corpus was updated deliberately, re-run the \
             `shasum -a 256` command in ucd/PROVENANCE.md and re-validate the gwidth + \
             UAX #29 suites — do NOT just paste the new digest.",
            path.display()
        );
    }
}
