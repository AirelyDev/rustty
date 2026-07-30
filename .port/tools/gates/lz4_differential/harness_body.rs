// G-LZ4 differential harness — RUSTTY ARM (body).
//
// `check_lz4_byte_exact.sh` generates a two-line `main.rs` that
// `#[path]`-includes `crates/rustty-vt/src/compress/lz4.rs` verbatim as `mod
// lz4` and then `include!`s this file. Nothing here may depend on anything but
// `core`/`std` and that module.
//
// This is an independent transcription of `CORPUS.md`, which is the normative
// specification. `harness.zig` is the other one.

// ---------------------------------------------------------------------------
// PRNG + hash (CORPUS.md "PRNG", "Hash of emitted bytes")
// ---------------------------------------------------------------------------

struct Rng {
    state: u64,
}

impl Rng {
    fn init(seed: u64) -> Rng {
        Rng { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xCBF2_9CE4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

const SIZES: [usize; 35] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 11, 12, 13, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255,
    256, 257, 4095, 4096, 4097, 65535, 65536, 65537, 204800, 409600,
];

const MAX_SIZE: usize = 409600;

fn fill(family: u64, size: usize, seed: u64, buf: &mut [u8]) {
    let mut rng = Rng::init(seed ^ 0x9E37_79B9_7F4A_7C15 ^ family.wrapping_mul(0x0000_0100_0000_01B3));
    let dst = &mut buf[..size];
    match family {
        0 => dst.fill(0),
        1 => {
            for (i, p) in dst.iter_mut().enumerate() {
                let rec = i / 8;
                *p = match i % 8 {
                    0 => ((rec % 95) + 32) as u8,
                    3 => ((rec / 95) % 7) as u8,
                    4 | 5 => 0xFF,
                    _ => 0,
                };
            }
        }
        2 => {
            let mut w: u64 = 0;
            for (i, p) in dst.iter_mut().enumerate() {
                if i % 8 == 0 {
                    w = rng.next();
                }
                *p = (w >> ((i % 8) * 8)) as u8;
            }
        }
        3 => {
            let mut i = 0usize;
            while i < size {
                let v = rng.next();
                let run = (1 + v % 64) as usize;
                let b = ((v >> 32) & 0xFF) as u8;
                let mut k = 0usize;
                while k < run && i < size {
                    dst[i] = b;
                    i += 1;
                    k += 1;
                }
            }
        }
        4 => {
            let period = (1 + rng.next() % 96) as usize;
            let mut pattern = [0u8; 96];
            let mut w: u64 = 0;
            for j in 0..period {
                if j % 8 == 0 {
                    w = rng.next();
                }
                pattern[j] = (w >> ((j % 8) * 8)) as u8;
            }
            for (i, p) in dst.iter_mut().enumerate() {
                *p = pattern[i % period];
            }
        }
        5 => {
            dst.fill(0);
            let pokes = size / 64;
            for _ in 0..pokes {
                let v = rng.next();
                dst[(v % size as u64) as usize] = ((v >> 32) & 0xFF) as u8;
            }
        }
        6 => {
            let mut i = 0usize;
            while i < size {
                let v = rng.next();
                let wlen = (2 + v % 9) as usize;
                let mut k = 0usize;
                while k < wlen && i < size {
                    let shift = 8 + (k % 6) * 4;
                    dst[i] = (b'a' as u64 + (v >> shift) % 26) as u8;
                    i += 1;
                    k += 1;
                }
                if i < size {
                    dst[i] = if (v >> 60) % 8 == 0 { b'\n' } else { b' ' };
                    i += 1;
                }
            }
        }
        7 => {
            for (i, p) in dst.iter_mut().enumerate() {
                *p = (i % 251) as u8;
            }
        }
        _ => unreachable!(),
    }
}

fn err_name(e: lz4::DecompressError) -> &'static str {
    match e {
        lz4::DecompressError::TruncatedInput => "TruncatedInput",
        lz4::DecompressError::InvalidOffset => "InvalidOffset",
        lz4::DecompressError::OutputTooSmall => "OutputTooSmall",
        lz4::DecompressError::OutputSizeMismatch => "OutputSizeMismatch",
    }
}

fn main() {
    use std::fmt::Write as _;
    use std::io::Write as _;

    let mut out = String::with_capacity(16 << 20);
    let mut input = vec![0u8; MAX_SIZE];
    let bound = lz4::compress_bound(MAX_SIZE).unwrap();
    let mut encoded = vec![0u8; bound];
    let mut decoded = vec![0u8; 2 * MAX_SIZE + 64];
    let mut table: lz4::HashTable = [0u32; lz4::HASH_TABLE_LEN];

    // --- phase 1: families x boundary sizes -------------------------------
    for family in 0u64..8 {
        for (si, &size) in SIZES.iter().enumerate() {
            let seed = family * 1000 + si as u64;
            fill(family, size, seed, &mut input);
            let n = lz4::compress(&input[..size], &mut encoded, &mut table).unwrap();
            let _ = writeln!(
                out,
                "p1,{},{},{},{},{:x}",
                family,
                size,
                seed,
                n,
                fnv1a(&encoded[..n])
            );
        }
    }

    // --- phase 2: randomized triples --------------------------------------
    let mut r2 = Rng::init(0x5EED_0002);
    for _ in 0..4000 {
        let fam = r2.next() % 8;
        let size = (r2.next() % 409601) as usize;
        let seed = r2.next();
        fill(fam, size, seed, &mut input);
        let n = lz4::compress(&input[..size], &mut encoded, &mut table).unwrap();
        let _ = writeln!(
            out,
            "p2,{},{},{},{},{:x}",
            fam,
            size,
            seed,
            n,
            fnv1a(&encoded[..n])
        );
    }

    // --- phase 3: every match period 1..96 over 200 KiB --------------------
    for period in 1usize..=96 {
        let seed = 0xBEEF_0000u64 + period as u64;
        let mut rng = Rng::init(seed);
        let mut pattern = [0u8; 96];
        let mut w: u64 = 0;
        for j in 0..period {
            if j % 8 == 0 {
                w = rng.next();
            }
            pattern[j] = (w >> ((j % 8) * 8)) as u8;
        }
        let size = 204800usize;
        for i in 0..size {
            input[i] = pattern[i % period];
        }
        let n = lz4::compress(&input[..size], &mut encoded, &mut table).unwrap();
        let _ = writeln!(
            out,
            "p3,{},{},{},{},{:x}",
            period,
            size,
            seed,
            n,
            fnv1a(&encoded[..n])
        );
    }

    // --- phase 4: decoder behaviour on corrupted blocks --------------------
    let mut work = vec![0u8; bound];
    for b in 0u64..60 {
        let fam = b % 8;
        let size = (64 + b.wrapping_mul(137) % 8192) as usize;
        let seed = 0xB10Cu64.wrapping_add(b);
        fill(fam, size, seed, &mut input);
        let enc_len = lz4::compress(&input[..size], &mut encoded, &mut table).unwrap();

        for c in 0u64..60 {
            let mut rng = Rng::init(b.wrapping_mul(1000).wrapping_add(c));
            work[..enc_len].copy_from_slice(&encoded[..enc_len]);
            let mut len = enc_len;
            match c % 4 {
                0 => {
                    let pos = (rng.next() % enc_len as u64) as usize;
                    let bit = (rng.next() % 8) as u32;
                    work[pos] ^= 1u8 << bit;
                }
                1 => {
                    let pos = (rng.next() % enc_len as u64) as usize;
                    work[pos] = (rng.next() & 0xFF) as u8;
                }
                2 => {
                    let pos = (rng.next() % enc_len as u64) as usize;
                    for k in 0..16usize {
                        let v = rng.next();
                        if pos + k < enc_len {
                            work[pos + k] = (v & 0xFF) as u8;
                        }
                    }
                }
                3 => len = (rng.next() % enc_len as u64) as usize,
                _ => unreachable!(),
            }
            let obuf = match c % 3 {
                0 => size,
                1 => size / 2,
                _ => size * 2 + 16,
            };
            // Zero the whole output buffer first and hash ALL of it, not just
            // `decoded[..dn]`. `9a4bd2120`'s decoder does blind wide copies
            // that deliberately write scratch bytes past a run's logical end;
            // hashing only the logical output would leave "which scratch bytes
            // survive" uncompared, and that is exactly where two byte-faithful
            // ports can silently drift apart.
            decoded[..obuf].fill(0);
            match lz4::decompress(&work[..len], &mut decoded[..obuf]) {
                Ok(dn) => {
                    let _ = writeln!(
                        out,
                        "p4,{},{},{},{},{:x}",
                        b,
                        c,
                        obuf,
                        dn,
                        fnv1a(&decoded[..obuf])
                    );
                }
                Err(e) => {
                    let _ = writeln!(out, "p4,{},{},{},ERR:{}", b, c, obuf, err_name(e));
                }
            }
        }
    }

    std::io::stdout().write_all(out.as_bytes()).unwrap();
}
