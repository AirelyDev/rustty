# Terminal Compression

Guidance for the codecs and the compressed page representation
(`page.rs`) in this directory. These compress terminal page backing
memory (`rustty_vt::Page`).

## Priorities

When making tradeoffs, in order:

1. **Compression ratio on page-shaped data.** Encoded bytes are retained
   scrollback memory, and raw `Page` backing memory is the only thing we
   actually compress. Ratio on text files or synthetic data is a
   secondary signal.
2. **Decompression throughput.** Pages are compressed once when they go
   cold but restored on demand (scrollback access, search, inspection), so
   restore latency is felt directly.
3. **Compression throughput.** Runs on idle pages in the background; being
   fast is nice, being slow is tolerable.

## Testing

- Targeted tests: `cargo test -p rustty-vt --lib compress::<codec>`
- Prefer running them under the lib-vt surface as well when practical;
  this code ships in librustty-vt.
- Codecs must keep building for `wasm32-unknown-unknown` (librustty-vt):
  no libc, no SIMD-dispatch dependencies. Verify with
  `cargo check -p rustty-vt --target wasm32-unknown-unknown`.
- Every codec needs a differential property suite: round-trip identity,
  an independent format walker, wrong-size output rejection, and
  corruption/truncation decoding. Keep a light version in normal unit
  tests and gate the exhaustive version behind an environment variable so
  the default test suite stays fast.

## Verifying Correctness

- Decoders must be memory-safe for arbitrary input bytes. Every blind or
  wide copy needs a stated margin argument bounding it by the output
  buffer; keep those arguments in comments next to the code.
- Writing scratch bytes past a copy's logical end is safe only inside the
  output buffer, because in-order decoding rewrites them before any match
  can read them back. Do not weaken the exact-size output contract.
- When a change should not alter compressor output, prove it: compare
  encoded sizes (or a sequence-count fingerprint) on the same corpus
  before and after. Ratio drift is a functional change, not noise.

## Benchmarking

- Use `rustty-bench +page-compression` (see `crates/rustty-bench`'s
  guidance for the general workflow). Modes: `compress`, `decompress`,
  `store`, and `report` for ratio.
- Build: `cargo build -p rustty-bench --release`
- The most representative corpus is a raw dump of real page backing
  memory, chunked at the page size (400 KiB on release targets).
  Supplement with a text corpus and random bytes for worst cases, but
  weigh page corpora highest per the priorities above. Keep corpora
  outside the repository and reuse identical files across comparisons.
- `rustty-bench +scrollback-compression` measures the PageList
  transitions around the codec rather than the codec itself.
- For fast iteration, keep codecs dependent only on `core`/`std` so a
  standalone harness can build them directly with `rustc -O` and time the
  codec in-process (report min-of-N, verify round-trips).
- Measure one change at a time and re-measure the final state; run-to-run
  noise is a few percent, so re-run before believing small deltas.

## Performance Notes

- Real page data decodes as millions of tiny operations (in LZ4: mostly
  zero literals plus a 4-18 byte match). Per-item overhead dominates, so
  branch-light fast paths with blind fixed-size copies win.
- Wide copies are the only SIMD that pays here. Vectorized compares and
  other wide-stride tricks measured as net losses because matches are
  short; prefer the simple word loop unless a measurement on page corpora
  says otherwise.
- `copy_from_slice`/`copy_within` beat stride loops only for long copies
  (roughly 64 bytes and up); call overhead loses below that.

## LZ4 Specific

- The codec is `lz4.rs`, an allocation-free raw block (not frame)
  implementation. Blocks do not carry their decoded size; callers supply
  an exact-size output buffer.
- Tests: `cargo test -p rustty-vt --lib compress::lz4`. The differential
  suite is `lz4_differential.rs`; run the exhaustive version for any
  codec change:
  `RUSTTY_LZ4_SLOW=1 cargo test -p rustty-vt --lib --release --ignored lz4_differential_exhaustive`
- The compressor must keep the standard format restrictions (final five
  bytes literal, matches start at least twelve bytes before the end) so
  blocks stay consumable by optimized external decoders. The differential
  walker checks this.

## Rustty-specific: how the ratio-drift rule is enforced

The "prove it" rule above is not self-enforcing, and the differential
suite does not cover it. Assessment B injected seven faults into `lz4.rs`
and `lz4_differential_light` caught four; the three it missed
(`HASH_LOG` 12 -> 11, `advance_search` 1024 -> 512, `copy_match`'s
memcpy threshold 64 -> 8) are exactly the ratio/output-shape class this
section names, because they preserve both round-trip identity and format
validity.

`.port/tools/gates/check_lz4_byte_exact.sh` (gate **G-LZ4**, registered
in `.port/tools/ci.sh`) is the realization of that rule. It compiles
`origin/main:src/terminal/compress/lz4.zig` and this crate's `lz4.rs`
standalone, walks an identical 7,976-point deterministic corpus through
both, and requires the encoded byte streams and the decoder's behaviour
on corrupted input to be identical. Its `--self-test` arm patches
`HASH_LOG` 12 -> 11 in a temp copy and requires exactly 1,631 diverging
rows, so the gate's own sensitivity cannot erode unnoticed. The corpus is
specified normatively in
`.port/tools/gates/lz4_differential/CORPUS.md`, which also records the
one fault the gate still does not reach.

Run it before and after any change to this directory that is not intended
to alter compressor output.
