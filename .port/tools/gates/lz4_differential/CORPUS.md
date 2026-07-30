# The G-LZ4 differential corpus — normative specification

`harness.zig` and `harness_body.rs` are two independent transcriptions of the
generator described here. **Neither is the specification; this file is.** If the
two harnesses ever disagree on a point that is *not* a codec difference, the bug
is in whichever one departed from this document.

The generator is fully deterministic: no clock, no address, no allocator
ordering, no platform-dependent integer width (every intermediate is `u64` with
explicit wrapping arithmetic, every index is `usize` but never exceeds 2^32).

## PRNG

SplitMix64, seeded per corpus point:

```
state +%= 0x9E3779B97F4A7C15
z = state
z = (z ^ (z >> 30)) *% 0xBF58476D1CE4E5B9
z = (z ^ (z >> 27)) *% 0x94D049BB133111EB
next = z ^ (z >> 31)
```

## Hash of emitted bytes

FNV-1a 64: `h = 0xCBF29CE484222325`; per byte `h ^= b; h *%= 0x100000001B3`.
Printed lowercase hex, no padding.

## Boundary sizes (35)

```
0 1 2 3 4 5 6 7 8 11 12 13 15 16 17 31 32 33 63 64 65 127 128 129
255 256 257 4095 4096 4097 65535 65536 65537 204800 409600
```

These straddle every structural constant of the codec: `MIN_MATCH` 4,
`LAST_LITERALS` 5, `MATCH_FIND_LIMIT` 12, the 15-value token nibbles, the
255-byte length-extension unit, the 64-byte `copy_match` memcpy threshold, the
4096-entry hash table, the 65,535 maximum match offset, and two page-multiple
sizes above the 1024-byte `advance_search` threshold.

## Families (8)

Seeded `SplitMix64(seed ^ 0x9E3779B97F4A7C15 ^ (family *% 0x100000001B3))`.

Where a family consumes bytes from `next()` "eight at a time", the rule is:
refill `w = next()` when `i % 8 == 0`, then `buf[i] = (w >> ((i % 8) * 8)) as u8`.
This keeps the PRNG call count independent of size-vs-word alignment.

| # | name | rule |
|---|---|---|
| 0 | zeros | `buf[i] = 0` |
| 1 | cell-structured | 8-byte records; `rec = i/8`, `off = i%8`; `off` 0 -> `(rec % 95) + 32`, 3 -> `(rec / 95) % 7`, 4 and 5 -> `0xFF`, otherwise 0 |
| 2 | random | eight-at-a-time PRNG bytes |
| 3 | runs | repeat: `v = next()`, run length `1 + v % 64`, byte `(v >> 32) & 0xFF` |
| 4 | periodic | `period = 1 + next() % 96`; pattern of `period` eight-at-a-time PRNG bytes; `buf[i] = pattern[i % period]` |
| 5 | sparse | all zero, then `size / 64` pokes: `v = next()`, `buf[v % size] = (v >> 32) & 0xFF` |
| 6 | text | repeat: `v = next()`, word length `2 + v % 9`, letters `b'a' + ((v >> (8 + (k % 6) * 4)) % 26)`, then one separator (`\n` if `(v >> 60) % 8 == 0` else space) |
| 7 | ramp | `buf[i] = (i % 251) as u8` |

## Phases (7,976 points)

| phase | points | construction | key |
|---|---|---|---|
| p1 | 280 | family 0..7 x the 35 boundary sizes, `seed = family * 1000 + size_index` | `encoded_len,fnv1a(encoded)` |
| p2 | 4,000 | `r = SplitMix64(0x5EED0002)`; per point `family = next()%8`, `size = next()%409601`, `seed = next()` | same |
| p3 | 96 | `period` 1..=96 over 204,800 bytes; pattern from `SplitMix64(0xBEEF0000 + period)` | same |
| p4 | 3,600 | 60 valid blocks x 60 corruptions (below) | error variant name, or `decoded_len,fnv1a(whole output buffer)` |

For p4 the output buffer is zeroed before every decode and the hash covers the
**entire** buffer, not `decoded[0..decoded_len]`. `9a4bd2120`'s decoder does
blind wide copies that deliberately write scratch bytes past a run's logical
end, so "which scratch bytes survive" is observable behaviour that two
byte-faithful ports can drift apart on. This is free sensitivity, so it is on —
but see "Known residual blind spot" below: on this corpus it does not, by
itself, reach the `copy_match` memcpy-threshold knob.

### p4 blocks and corruptions

Block `b` in 0..60: `family = b % 8`, `size = 64 + (b *% 137) % 8192`,
`seed = 0xB10C +% b`. Encode it once; call the result `enc[0..L]`.

Corruption `c` in 0..60, `r = SplitMix64(b *% 1000 +% c)`:

| `c % 4` | corruption |
|---|---|
| 0 | bit flip: `pos = next() % L`, `bit = next() % 8`, `work[pos] ^= 1 << bit` |
| 1 | byte overwrite: `pos = next() % L`, `work[pos] = next() & 0xFF` |
| 2 | 16-byte splice: `pos = next() % L`, then for `k` in 0..16 always draw `v = next()` and apply `work[pos+k] = v & 0xFF` only when `pos+k < L` (the draw is unconditional so the stream stays aligned regardless of `pos`) |
| 3 | random-prefix truncation: `len = next() % L` |

Output buffer length rotates with `c % 3`: exact `size`, short `size / 2`,
long `size * 2 + 16`.

Every corruption kind still *consumes* its PRNG draws in the fixed order above,
so a harness that skips a kind desynchronizes visibly rather than silently.

## Output contract

One line per point, in phase order, `\n`-terminated:

```
p1,<family>,<size>,<seed>,<encoded_len>,<fnv1a hex>
p2,<family>,<size>,<seed>,<encoded_len>,<fnv1a hex>
p3,<period>,204800,<seed>,<encoded_len>,<fnv1a hex>
p4,<block>,<corruption>,<outbuf_len>,ERR:<VariantName>
p4,<block>,<corruption>,<outbuf_len>,<decoded_len>,<fnv1a hex>
```

The two arms must be byte-identical. `check_lz4_byte_exact.sh` diffs them.

## Measured sensitivity (this corpus, this machine)

Injecting each of the three faults `lz4_differential_light` is *known to miss*
(assessment B §2.3) into the Rust arm only, and diffing against the unmodified
oracle arm:

| injected fault | diverging rows / 7,976 | caught by G-LZ4? |
|---|---|---|
| `HASH_LOG` 12 -> 11 | **1,631** | yes — this is the `--self-test` arm |
| `advance_search` 1024 -> 512 | **1** | yes |
| `copy_match` memcpy threshold 64 -> 8 | **0** | **no** |

Unmodified rustty vs the oracle: **0 / 7,976**.

### Known residual blind spot

`copy_match`'s `match_len >= 64` memcpy threshold is **not** covered by either
guard. Changing it only moves a match between the exact-copy path and the wide
overshooting path; the overshoot bytes are subsequently overwritten by the next
sequence in every block this corpus produces, so the observable output is
identical. Reaching it needs a block whose *final* sequence is a match with
`8 <= match_len < 64` and `offset >= match_len`, decoded into an output buffer
strictly longer than the decoded length. No point in this corpus has that shape.
This is a real, named gap, recorded here rather than papered over; closing it
would be a targeted fifth phase, and would change the point total and therefore
the pinned `--self-test` count.

## Deviation from the plan's expected self-test count

`docs/superpowers/plans/2026-07-29-phase3-upstream-resync.md` (P3-W0 T1 step 3)
requires `--self-test` to report **exactly 1,929** diverging rows, on the grounds
that a different number means this generator diverged from assessment B's.
That check cannot be honoured: assessment B §2.1 records its own harnesses as
`/tmp/b-comp/{z,rs}` and explicitly **transient, not committed** — its §5.4 asks
for exactly the permanent gate being built here. There is no artifact to
converge on, only the prose description in §2.1, which this corpus implements.

The number is therefore pinned to what *this* corpus measures — **1,631** — in
`check_lz4_byte_exact.sh`. Pinning it (rather than asserting "> 0") is what
keeps the gate's sensitivity from silently eroding: a corpus edit that halves
the reach fails the self-test instead of passing it.
