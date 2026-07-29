# WS7 — Scrollback Page Compression (BL-1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the upstream ghostty BL-1 stack (~35 commits) — a standalone LZ4 block codec, a compressed `Page` representation, retained-mapping memory reclamation, PageList compression scheduling, page-generation stamps, and the search-integrity fixes built on them — into `rustty-vt` and its consumers, with no regression on the 14-benchmark perf suite.

**Architecture:** Three new leaf modules in `rustty-vt` (`compress/lz4.rs`, `compress/page.rs`, `mem.rs`) sit **below** `pagelist`. `PageList::Node` gains an `owned: Owned` tag and its `data` field becomes a two-state enum (`Resident(Page)` / `Compressed(CompressedPage)`); every existing `(*node).data` access is routed through raw-pointer `Node` accessors first, so the representation change is a one-line switch afterwards. Compression is *policy* owned by `PageList` (`compress(mode)`), scheduling is owned by the renderer thread, and the `Terminal`/lib-vt C API expose the activity token + step function to embedders. Generation stamps (`node.serial`, `page_serial_epoch`, `renew_generation`, `node_is_valid`) make cached search state safe against page replacement.

**Tech Stack:** Rust 1.93.1 (pinned), `rustty-vt` (no new crate deps), hand-rolled `extern "C"` `madvise` (matching the existing hand-rolled `mmap`/`munmap` in `page.rs`), `cargo +nightly miri` for the aliasing lanes, `.port/phase-0/baseline/run_benchmarks.py` for the perf gate.

**KEY RESEARCH FINDING — no `lz4-sys` crate is needed.** Ghostty does **not** vendor the reference C LZ4 library for this feature. `src/terminal/compress/lz4.zig` is a **891-line allocation-free pure-Zig implementation** of the raw LZ4 *block* format (not the frame format), written specifically so it keeps building for `wasm32-freestanding` with no libc and no `src/simd`/Highway dependency. It is therefore ported as **pure Rust** into `crates/rustty-vt/src/compress/lz4.rs`. **Do not create `crates-sys/lz4-sys`. Do not add an `lz4`/`lz4_flex` cargo dependency.** The only new OS interaction in this workstream is `madvise`, which follows the existing `page.rs` `extern "C"` convention (no `libc` crate).

---

## Global Constraints

Inherited verbatim from `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` ("Global constraints (inherited by every WS plan)"):

- R1 full fidelity vs the ghostty oracle at `../ghostty`; the oracle is reference-only — **no build-time dependency on it (R3)**.
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty); `ghostty_*` C-ABI aliases stay.
- R4/R5: acyclic crate graph; gates (`check_acyclic.sh`, A4 ABI gate, Miri lanes) are invariant — never weakened to pass.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI.
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned.
- Ledger discipline: when a WS lands, strike its entries in `.port/phase-2/DEFERRALS.md` / `VERIFICATION_DEBT.md` in the same PR.
- Commit style: `subsystem: summary (chunk-ref)`; branch per WS; merge to main only with both platforms green.

WS7-specific constraints, binding on every task:

- **No new third-party dependency.** The LZ4 codec is a source port. No `lz4-sys`, no `lz4_flex`, no `libc` crate.
- **No `unsafe` in `compress/lz4.rs`.** The codec is written in safe Rust (slice `try_into()` + `copy_within`). If and only if the Phase-E `+page-compression` bench shows a decode-throughput gap versus the oracle does an `unsafe` fast path get introduced — and then only with a Miri lane over it. Rationale: the oracle's own `AGENTS.md` requires "Decoders must be memory-safe for arbitrary input bytes"; safe Rust discharges that by construction.
- **Raw-pointer discipline (memory: `rust-mut-args-strongly-protected`).** Every new `Node` / `Page` / `PageList` helper that a caller may hold a live alias across takes `*mut`/`*const` arguments, never `&mut`. Compressing behind a `&mut Page` while the node's `data` union is rewritten is exactly the Stacked-Borrows hazard class this port treats as UB.
- **Oracle read protocol (R3).** The BL-1 commits are **NOT** in the pinned `/Users/gael/projects/ghostty` checkout (`4749c4e`). Run **once** at the start of the workstream:
  ```bash
  git -C /Users/gael/projects/ghostty fetch origin main
  ```
  Then read every oracle file cited in this plan as `git -C /Users/gael/projects/ghostty show origin/main:<path>` and every cited commit as `git -C /Users/gael/projects/ghostty show <hash>`. **Never check out `origin/main`** — the working checkout stays on the pin. Every file path in this plan prefixed `oracle@main:` means "read via `show origin/main:`". Every path prefixed `oracle@pin:` means the pinned tree is authoritative.
- **`cfg(test)` mirrors Zig `builtin.is_test`.** `mem::decommit` must simulate a successful reclaim by zeroing under `cfg(test)` (the testing allocator's ranges can share an OS mapping with unrelated allocations, so real `madvise` is unsafe there). This is the oracle's own behavior, not a test shortcut.
- **Existing vt Miri lane is RED** (memory: `search-screen-sb-ub-finding`) — 9 pre-existing SB-UB tests in the search subsystem. WS7 must not *add* to that set; Phase E's Miri gate is scoped by an explicit allow-list of the 9 known-red test names recorded in `.port/phase-2/MIRI_EXCEPTIONS.md`. Fixing them is WS8's business, not WS7's.

---

## Phase Structure & Execution Order

Five phases in a single file. **Execution order is A → B → C → D → E.**

| Phase | Scope | Tasks | Separately executable? |
|---|---|---|---|
| **A** | LZ4 block codec + differential suite (`compress/lz4.rs`) | A1–A6 | **YES** — pure leaf module, zero callers, lands and merges on its own. |
| **B** | Retained-mapping reclamation (`mem.rs`), compressed `Page` repr, `Node` ownership + storage enum, PageList compression policy | B1–B10 | **NO** — requires Phase A. Internally B1–B4 (mem + repr + accessor retarget) can land as one commit series before B5–B10 (policy). |
| **C** | Generation stamps (`renew_generation`, `page_serial_epoch`, `node_is_valid`) + search integrity | C1–C8 | **NO** — requires Phase B's `serial`/`Storage` surface. |
| **D** | Consumer surface: `Terminal` API, config option, renderer-thread scheduling + state handoff, lib-vt C API, inspector, bench actions | D1–D8 | **PARTIALLY** — requires Phase B. D4 (renderer state handoff) and D8 (bench actions) are independent of the rest of D and may be executed out of order. |
| **E** | Gates: both-platform build, Miri lanes, perf non-regression, ledgers, merge | E1–E6 | **NO** — terminal phase. |

Each phase ends with `cargo test -p rustty-vt --lib` green and a commit. Phases B–E additionally end with `cargo check --workspace --all-targets` AND `cargo build --workspace` green on **both** macOS and `rvm`. The `--all-targets` check is the load-bearing half: `cargo build --workspace` compiles neither examples nor test targets.

Branch for the whole workstream (created once, before Task A1):

```bash
cd /Users/gael/projects/rustty
git checkout -b ws7-scrollback-compression
```

---

## File Structure

**New files**

| Path | Responsibility |
|---|---|
| `crates/rustty-vt/src/compress.rs` | Namespace module. Re-exports `lz4` and `CompressedPage`. Ports `oracle@main:src/terminal/compress.zig`. |
| `crates/rustty-vt/src/compress/lz4.rs` | Allocation-free raw LZ4 **block** codec: `compress_bound`, `compress`, `decompress`, `HashTable`, `CompressError`, `DecompressError`. Ports `oracle@main:src/terminal/compress/lz4.zig`. |
| `crates/rustty-vt/src/compress/lz4_differential.rs` | `#[cfg(test)]` differential property suite: round-trip identity, independent format walker, wrong-size rejection, corruption/truncation decoding. Ports `oracle@main:src/terminal/compress/lz4_differential.zig`. |
| `crates/rustty-vt/src/compress/page.rs` | `CompressedPage` — full `Page` metadata + exact encoded block, retaining the source's virtual mapping. Ports `oracle@main:src/terminal/compress/Page.zig`. |
| `crates/rustty-vt/src/mem.rs` | `DecommitMode`, `can_reclaim`, `decommit`, `recommit` — retained-mapping physical reclamation via hand-rolled `extern "C"` `madvise`. Ports `oracle@main:src/terminal/mem.zig`. |
| `crates/rustty-vt/src/pagelist/compress.rs` | `impl PageList` compression cluster: `CompressionScratch`, `IncrementalCompressionState`, `CompressionIterator`, `compress`, `compress_page`, `memory_stats`. Ports the compression sections of `oracle@main:src/terminal/PageList.zig`. |
| `crates/rustty-bench/src/page_compression.rs` | `+page-compression` bench action (modes `compress`/`decompress`/`store`/`report`). Ports `oracle@main:src/benchmark/PageCompression.zig`. |
| `crates/rustty-bench/src/scrollback_compression.rs` | `+scrollback-compression` bench action. Ports `oracle@main:src/benchmark/ScrollbackCompression.zig`. |
| `.port/phase-2/mirihar-compress/` | Isolated non-FFI Miri harness proving the compress/restore seam is SB-clean (established pattern: `.port/phase-2/mirihar-chunk3b`). |

**Modified files**

| Path | Change |
|---|---|
| `crates/rustty-vt/src/lib.rs` | `pub mod compress; pub mod mem;` + `pub const COMPRESSION_ENABLED: bool` (ports `oracle@main:src/terminal/main.zig:80`). |
| `crates/rustty-vt/src/page_impl.rs` | Add `Page::shallow_copy()` (metadata-only clone that aliases the same mapping). |
| `crates/rustty-vt/src/pagelist/mod.rs` | `Node.owned`, `Node.data: NodeData`, `Node` raw-pointer accessors, `PreservedPage`, `page_serial_min` → `page_serial_epoch`, `page_compression` field, `renew_generation`, `node_is_valid`, `destroy_node_ext` ownership switch. |
| `crates/rustty-vt/src/pagelist/{access,erase,grow,iter,pin,resize,scroll}.rs` | Mechanical retarget of `(*node).data` → `Node::page(node)` / `Node::page_assume_resident(node)` / `Node::metadata(node)`. |
| `crates/rustty-vt/src/{screen.rs,terminal.rs,render.rs,selection.rs,formatter.rs}` | Same mechanical retarget; `Terminal::compression_activity` / `Terminal::compress`. |
| `crates/rustty-vt/src/search/{pagelist,sliding_window,viewport,screen,active,thread}.rs` | `PreservedPage`-based formatting; `AppendResult`; `Meta.rows`; generation validation. |
| `crates/rustty-vt-c/src/terminal.rs`, `include/rustty/vt/terminal.h` | `rustty_terminal_compression_activity`, `rustty_terminal_compress` + ABI enums. |
| `crates/rustty-config/src/config_struct.rs` | `scrollback_compression: bool = true`; `scrollback_limit` default 10 MB → 50 MB. |
| `crates/rustty-renderer/src/{config.rs,thread.rs,state.rs}` | `RendererConfig::scrollback_compression`; `Compression` scheduling struct; `lock_demand`/`unlock_demand`/`yield_to_demand`. |
| `crates/rustty-termio/src/exec.rs` | `yield_to_demand` call between parse batches. |
| `crates/rustty-inspector/src/widgets/pagelist.rs`, `page.rs` | Compression memory columns. |
| `crates/rustty-bench/src/{cli.rs,lib.rs,main_bench.rs}` | Register the two new actions. |
| `.port/phase-2/MAIN_PARITY.md` | Strike the BL-1 rows. |

---

# PHASE A — LZ4 block codec

**Oracle:** `oracle@main:src/terminal/compress/lz4.zig` (final form after `b9bb50c83` "optimize page compression codec" and `9a4bd2120` "optimize LZ4 decoding and add differential tests"). Port the **final** file, not the intermediate `c62c15984` version — the two optimization commits rewrote most of it and the final file is the parity target.

**Also read:** `oracle@main:src/terminal/compress/AGENTS.md` — the codec's stated priorities (ratio on page-shaped data > decompression throughput > compression throughput) and its correctness rules ("every blind or wide copy needs a stated margin argument bounding it by the output buffer"; "Do not weaken the exact-size output contract").

---

### Task A1: Codec module scaffold + `compress_bound`

**Files:**
- Create: `crates/rustty-vt/src/compress.rs`
- Create: `crates/rustty-vt/src/compress/lz4.rs`
- Modify: `crates/rustty-vt/src/lib.rs`

**Interfaces:**
- Produces: `rustty_vt::compress::lz4::{MAX_INPUT_SIZE, HashTable, CompressError, DecompressError, compress_bound}`.
  - `pub const MAX_INPUT_SIZE: usize = 0x7E00_0000;`
  - `pub type HashTable = [u32; 1 << 12];`
  - `pub enum CompressError { InputTooLarge, OutputTooSmall }`
  - `pub enum DecompressError { TruncatedInput, InvalidOffset, OutputTooSmall, OutputSizeMismatch }`
  - `pub fn compress_bound(input_len: usize) -> Result<usize, CompressError>`

- [ ] **Step 1: Write the failing test**

Append to `crates/rustty-vt/src/compress/lz4.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// PORTS: oracle@main lz4.zig `test "compressBound"`
    #[test]
    fn lz4_compress_bound() {
        assert_eq!(compress_bound(0), Ok(16));
        assert_eq!(compress_bound(255), Ok(272));
        assert_eq!(
            compress_bound(MAX_INPUT_SIZE + 1),
            Err(CompressError::InputTooLarge)
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib lz4_compress_bound`
Expected: FAIL — `error[E0433]: failed to resolve: use of undeclared crate or module compress`

- [ ] **Step 3: Write minimal implementation**

`crates/rustty-vt/src/compress.rs`:

```rust
//! Compression primitives used by the terminal.
//!
//! This namespace contains only the representation and codecs. Policy about
//! which pages to compress, when to decommit their resident memory, and when
//! to restore them belongs to `PageList`.
//!
//! PORTS: ghostty src/terminal/compress.zig (oracle origin/main)

/// The raw LZ4 block codec used for terminal page memory.
pub mod lz4;

#[cfg(test)]
mod lz4_differential;

pub mod page;

pub use page::CompressedPage;
```

Delete the `pub mod page;` / `pub use page::CompressedPage;` lines for now — they land in Task B3. For Task A1 the file is exactly:

```rust
//! Compression primitives used by the terminal.
//!
//! This namespace contains only the representation and codecs. Policy about
//! which pages to compress, when to decommit their resident memory, and when
//! to restore them belongs to `PageList`.
//!
//! PORTS: ghostty src/terminal/compress.zig (oracle origin/main)

/// The raw LZ4 block codec used for terminal page memory.
pub mod lz4;
```

`crates/rustty-vt/src/compress/lz4.rs` (header + constants + `compress_bound`; the `tests` module from Step 1 stays at the bottom):

```rust
//! An allocation-free implementation of the raw LZ4 block format.
//!
//! LZ4 has two relevant layers: the block format describes the compressed
//! bytes, while the frame format adds headers, sizes, checksums, and support
//! for a stream of blocks. Terminal pages already have their own ownership
//! and metadata, so this implements only blocks. In particular, an encoded
//! block does not contain its decompressed size. The caller must store that
//! separately and provide an exactly sized buffer when decoding.
//!
//! A block is a series of sequences. Each non-final sequence has this shape:
//!
//!     token | literal length extensions | literals | offset | match length extensions
//!
//! The token's high nibble contains the literal length and its low nibble
//! contains the match length minus four. A nibble value of 15 means that the
//! length continues in extension bytes at the corresponding point in the
//! sequence. Each extension byte adds to the length; a value of 255 means
//! another byte follows. The literal bytes are copied directly. The two-byte
//! little-endian offset then points backwards in the already decompressed
//! output to the match bytes.
//!
//! The last sequence is special: it contains literals only and ends directly
//! after them. The reference format also requires the last five input bytes to
//! be literals and the final match to begin at least twelve bytes before the
//! end. The compressor observes these restrictions so its output can be
//! consumed by optimized LZ4 decoders which copy in larger units.
//!
//! Format reference:
//! https://github.com/lz4/lz4/blob/dev/doc/lz4_Block_format.md
//!
//! PORTS: ghostty src/terminal/compress/lz4.zig (oracle origin/main, final
//! form after b9bb50c83 + 9a4bd2120).
//!
//! PORT NOTE (WS7): this module contains NO `unsafe`. The oracle's blind wide
//! copies are expressed as safe slice reads/writes whose margin conditions are
//! restated in comments exactly as the oracle requires them. See the WS7 plan's
//! Global Constraints for the (measured, not assumed) escalation rule.

/// Maximum input accepted by the reference LZ4 block API. Keeping the same
/// limit means `compress_bound` fits in the integer sizes used by LZ4 callers
/// and gives us the same compatibility boundary as other implementations.
pub const MAX_INPUT_SIZE: usize = 0x7E00_0000;

/// Every LZ4 match represents at least four bytes. The token stores the number
/// of bytes beyond this minimum rather than the full match length.
const MIN_MATCH: usize = 4;

/// Number of bytes at the end of a conforming block which must remain literals.
const LAST_LITERALS: usize = 5;

/// A match may not begin in the final 12 bytes. This leaves enough room for the
/// minimum match and the required five trailing literals.
const MATCH_FIND_LIMIT: usize = 12;

/// We retain one input position for each 12-bit hash. LZ4 refers to this as
/// memory usage 14 because the 4096 entries are four bytes each (16 KiB).
const HASH_LOG: usize = 12;

/// Multiplicative hash used by the reference LZ4 fast compressor. The high
/// `HASH_LOG` bits provide the table index.
const HASH_MULTIPLIER: u32 = 2_654_435_761;

/// Scratch memory used while compressing one block. Each entry packs the low
/// 16 bits of the two most recent input positions for its hash. All-ones marks
/// an empty half; LZ4's 16-bit offset is enough to reconstruct the only useful
/// preceding address from the current position.
/// The table is reset by every call to `compress` and can be reused afterwards.
pub type HashTable = [u32; 1 << HASH_LOG];

/// Errors which can occur while encoding a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressError {
    /// The input exceeds the maximum size supported by the block compressor.
    InputTooLarge,

    /// The provided output buffer cannot hold the encoded block.
    OutputTooSmall,
}

/// Errors which can occur while decoding a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecompressError {
    /// The encoded block ended in the middle of a sequence.
    TruncatedInput,

    /// A match offset was zero or pointed before the produced output.
    InvalidOffset,

    /// A sequence would write beyond the provided output buffer.
    OutputTooSmall,

    /// The block ended before filling the exact-size output buffer.
    OutputSizeMismatch,
}

/// Return the maximum number of bytes needed to encode `input_len` bytes.
///
/// Incompressible input is represented as one literal run. Every 255 literal
/// bytes can require one extension byte. The additional 16-byte margin covers
/// the token and the format's fixed overhead. Callers can allocate this amount
/// once and reuse it for any block no larger than `input_len`.
pub fn compress_bound(input_len: usize) -> Result<usize, CompressError> {
    if input_len > MAX_INPUT_SIZE {
        return Err(CompressError::InputTooLarge);
    }
    Ok(input_len + input_len / 255 + 16)
}
```

In `crates/rustty-vt/src/lib.rs`, add next to the other `pub mod` declarations (alphabetical position, after `color`):

```rust
pub mod compress;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib lz4_compress_bound`
Expected: PASS — `test compress::lz4::tests::lz4_compress_bound ... ok`

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/compress.rs crates/rustty-vt/src/compress/lz4.rs crates/rustty-vt/src/lib.rs
git commit -m "vt/compress: LZ4 block codec scaffold + compressBound (WS7 A1)"
```

---

### Task A2: LZ4 decoder

**Files:**
- Modify: `crates/rustty-vt/src/compress/lz4.rs`

**Interfaces:**
- Consumes: A1's constants and error enums.
- Produces: `pub fn decompress(input: &[u8], output: &mut [u8]) -> Result<usize, DecompressError>`; private helpers `read_u16/read_u32/read_u64`, `copy_u16_within`, `copy_u64_within`, `copy_u128_within`, `decode_length`, `copy_match`.

- [ ] **Step 1: Write the failing test**

Add inside the existing `mod tests`:

```rust
    /// PORTS: oracle@main lz4.zig `test "literal-only compatibility vectors"`
    #[test]
    fn lz4_literal_only_compatibility_vectors() {
        let mut empty: [u8; 0] = [];
        assert_eq!(decompress(&[0], &mut empty), Ok(0));

        let mut hello = [0u8; 5];
        assert_eq!(
            decompress(&[0x50, b'h', b'e', b'l', b'l', b'o'], &mut hello),
            Ok(5)
        );
        assert_eq!(&hello, b"hello");

        let mut fifteen = [0u8; 15];
        let mut encoded = [b'x'; 17];
        encoded[0] = 0xF0;
        encoded[1] = 0;
        decompress(&encoded, &mut fifteen).unwrap();
        assert!(fifteen.iter().all(|&b| b == b'x'));
    }

    /// PORTS: oracle@main lz4.zig `test "overlapping match compatibility vector"`
    #[test]
    fn lz4_overlapping_match_compatibility_vector() {
        let mut output = [0u8; 5];
        assert_eq!(decompress(&[0x10, b'a', 0x01, 0x00], &mut output), Ok(5));
        assert_eq!(&output, b"aaaaa");
    }

    /// PORTS: oracle@main lz4.zig `test "extended overlapping match compatibility vector"`
    #[test]
    fn lz4_extended_overlapping_match_compatibility_vector() {
        let mut output = [0u8; 275];
        assert_eq!(
            decompress(&[0x1F, b'a', 0x01, 0x00, 0xFF, 0x00], &mut output),
            Ok(275)
        );
        assert!(output.iter().all(|&b| b == b'a'));
    }

    /// PORTS: oracle@main lz4.zig `test "short offset compatibility vectors"`
    #[test]
    fn lz4_short_offset_compatibility_vectors() {
        let mut offset_two = [0u8; 6];
        decompress(&[0x20, b'a', b'b', 0x02, 0x00], &mut offset_two).unwrap();
        assert_eq!(&offset_two, b"ababab");

        let mut offset_three = [0u8; 9];
        decompress(&[0x32, b'a', b'b', b'c', 0x03, 0x00], &mut offset_three).unwrap();
        assert_eq!(&offset_three, b"abcabcabc");

        let mut offset_four = [0u8; 8];
        decompress(&[0x40, b'a', b'b', b'c', b'd', 0x04, 0x00], &mut offset_four).unwrap();
        assert_eq!(&offset_four, b"abcdabcd");
    }

    /// PORTS: oracle@main lz4.zig
    /// `test "bounded wild copies are overwritten by final literals"`
    #[test]
    fn lz4_bounded_wild_copies_are_overwritten_by_final_literals() {
        let mut repeated_byte = [0u8; 15];
        decompress(
            &[
                0x15, b'a', 0x01, 0x00, 0x50, b'1', b'2', b'3', b'4', b'5',
            ],
            &mut repeated_byte,
        )
        .unwrap();
        assert_eq!(&repeated_byte, b"aaaaaaaaaa12345");

        let mut word_offset = [0u8; 22];
        decompress(
            &[
                0x85, b'a', b'b', b'c', b'd', b'e', b'f', b'g', b'h', 0x08, 0x00, 0x50,
                b'1', b'2', b'3', b'4', b'5',
            ],
            &mut word_offset,
        )
        .unwrap();
        assert_eq!(&word_offset, b"abcdefghabcdefgha12345");
    }

    /// PORTS: oracle@main lz4.zig `test "decompress rejects malformed blocks"`
    #[test]
    fn lz4_decompress_rejects_malformed_blocks() {
        let mut output = [0u8; 32];
        assert_eq!(
            decompress(&[0xF0], &mut output),
            Err(DecompressError::TruncatedInput)
        );
        assert_eq!(
            decompress(&[0x10, b'a', 1], &mut output[0..5]),
            Err(DecompressError::TruncatedInput)
        );
        assert_eq!(
            decompress(&[0x10, b'a', 0, 0], &mut output[0..5]),
            Err(DecompressError::InvalidOffset)
        );
        assert_eq!(
            decompress(&[0x10, b'a', 2, 0], &mut output[0..5]),
            Err(DecompressError::InvalidOffset)
        );
        assert_eq!(
            decompress(&[0x10, b'a', 1, 0], &mut output[0..4]),
            Err(DecompressError::OutputTooSmall)
        );
        assert_eq!(
            decompress(&[0], &mut output[0..1]),
            Err(DecompressError::OutputSizeMismatch)
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib lz4_`
Expected: FAIL — `cannot find function decompress in this scope`

- [ ] **Step 3: Write minimal implementation**

Insert above the `mod tests` block in `crates/rustty-vt/src/compress/lz4.rs`:

```rust
/// Decompress a raw LZ4 block into an exact-size output buffer.
///
/// Returns `output.len()` on success. Both consuming all input and filling all
/// output are required. Raw LZ4 blocks do not carry their decoded size, so this
/// exact-size contract validates the size metadata maintained by the caller.
/// The input and output buffers must not overlap (guaranteed by `&[u8]` +
/// `&mut [u8]` in Rust).
pub fn decompress(input: &[u8], output: &mut [u8]) -> Result<usize, DecompressError> {
    // `ip` and `op` always identify the next unread input byte and the next
    // unwritten output byte respectively.
    //
    // The decoder is written around one observation: almost every sequence
    // in real blocks has a short literal run and a short match. Both fast
    // paths below copy a fixed number of bytes blindly and let the length
    // arithmetic sort out how many of them were meaningful. Writing past a
    // run's logical end is safe within the output buffer because decoding
    // is strictly in order: every byte past `op` is either rewritten by a
    // later copy before anything can read it, or lies beyond the block's
    // final length and is never part of the result. The margin conditions
    // on the fast paths also subsume the exact bounds checks they replace,
    // which keeps decoding of malformed blocks memory-safe.
    let mut ip: usize = 0;
    let mut op: usize = 0;

    loop {
        // A normal block ends after the literal bytes of its final sequence.
        // This also accepts the empty block produced by our compressor, which
        // consists of a zero token and no literals.
        if ip == input.len() {
            if op != output.len() {
                return Err(DecompressError::OutputSizeMismatch);
            }
            return Ok(op);
        }

        let token = input[ip];
        ip += 1;

        // The high nibble and any extension bytes describe the literal run.
        // The literals are copied as a side effect of computing the length.
        let literal_len: usize = {
            let nibble = (token >> 4) as usize;
            if nibble != 15 && core::cmp::min(input.len() - ip, output.len() - op) >= 16 {
                // A run below the extension threshold is at most 14 bytes,
                // so with a 16-byte margin on both buffers one wide copy
                // covers it.
                output[op..op + 16].copy_from_slice(&input[ip..ip + 16]);
                nibble
            } else {
                // Extended or margin-poor runs take the checked path. Bounds are
                // verified before copying so malformed blocks never cause a
                // partial read or write.
                let len = decode_length(input, &mut ip, nibble)?;
                if len > input.len() - ip {
                    return Err(DecompressError::TruncatedInput);
                }
                if len > output.len() - op {
                    return Err(DecompressError::OutputTooSmall);
                }
                output[op..op + len].copy_from_slice(&input[ip..ip + len]);
                len
            }
        };
        ip += literal_len;
        op += literal_len;

        // Ending immediately after the literals marks the final sequence. Any
        // non-final sequence must continue with an offset and match length.
        if ip == input.len() {
            if op != output.len() {
                return Err(DecompressError::OutputSizeMismatch);
            }
            return Ok(op);
        }

        if input.len() - ip < 2 {
            return Err(DecompressError::TruncatedInput);
        }
        let offset = read_u16(input, ip) as usize;
        ip += 2;
        if offset == 0 || offset > op {
            return Err(DecompressError::InvalidOffset);
        }

        // The token stores the match length minus the four-byte minimum. As
        // with literals, a low nibble of 15 is extended by following bytes.
        let match_nibble = (token & 0x0F) as usize;

        // A match whose length fits its nibble spans at most 18 bytes, so
        // three blind copies always cover it. They are overlap-safe when the
        // offset is at least a word: each load lies a full word behind the
        // store which could observe it, so repeating patterns propagate
        // correctly.
        if match_nibble != 15 && offset >= 8 && output.len() - op >= 18 {
            let m = op - offset;
            copy_u64_within(output, op, m);
            copy_u64_within(output, op + 8, m + 8);
            copy_u16_within(output, op + 16, m + 16);
            op += match_nibble + MIN_MATCH;
            continue;
        }

        let encoded_match_len = decode_length(input, &mut ip, match_nibble)?;
        let match_len = encoded_match_len
            .checked_add(MIN_MATCH)
            .ok_or(DecompressError::OutputTooSmall)?;
        if match_len > output.len() - op {
            return Err(DecompressError::OutputTooSmall);
        }

        // Match copies may overlap, so this cannot always be one copy.
        // `copy_match` expands the common small repeating periods into wide
        // stores and uses word copies for larger offsets.
        copy_match(output, op, offset, match_len);
        op += match_len;
    }
}

/// Decode a length from its token nibble and any following extension bytes.
/// `ip` is advanced past every consumed extension byte.
fn decode_length(
    input: &[u8],
    ip: &mut usize,
    nibble: usize,
) -> Result<usize, DecompressError> {
    let mut length: usize = nibble;
    if nibble != 15 {
        return Ok(length);
    }

    loop {
        if *ip >= input.len() {
            return Err(DecompressError::TruncatedInput);
        }
        let value = input[*ip];
        *ip += 1;
        length = length
            .checked_add(value as usize)
            .ok_or(DecompressError::TruncatedInput)?;
        if value != 255 {
            return Ok(length);
        }
    }
}

/// Copy one decoded match from `offset` bytes behind `op`.
///
/// The caller has validated that the match fits: `op + match_len` never
/// exceeds `output.len()`. Wide copies may write a few scratch bytes past the
/// match's logical end; as described in `decompress`, that is safe anywhere
/// the write stays inside the output buffer. Every path therefore bounds its
/// wide stores by both the match end and the buffer end, and the bytewise
/// loop at the bottom finishes whatever remains.
fn copy_match(output: &mut [u8], op_: usize, offset: usize, match_len: usize) {
    let mut op = op_;
    let end = op_ + match_len;

    // A source which ends behind the copy can never overlap it. Long
    // distant matches are common in structured pages (repeated rows and
    // whole blank regions), and one exact copy moves them in cache-line
    // units. Shorter matches are not worth the call overhead.
    if offset >= match_len && match_len >= 64 {
        output.copy_within(op - offset..op - offset + match_len, op);
        return;
    }

    match offset {
        // A period which divides the word size expands into one repeated
        // pattern word. Long runs (blank lines, repeated cells) then become
        // independent stores, with no load waiting on a preceding store.
        // Stores advance by whole words from `op`, which preserves the
        // pattern's phase.
        1 | 2 | 4 | 8 => {
            let pattern: u64 = match offset {
                1 => (output[op - 1] as u64) * 0x0101_0101_0101_0101,
                2 => (read_u16(output, op - 2) as u64) * 0x0001_0001_0001_0001,
                4 => (read_u32(output, op - 4) as u64) * 0x0000_0001_0000_0001,
                8 => read_u64(output, op - 8),
                _ => unreachable!(),
            };

            let limit = core::cmp::min(end, output.len().saturating_sub(7));
            while op < limit {
                output[op..op + 8].copy_from_slice(&pattern.to_le_bytes());
                op += 8;
            }
        }

        // Wide copies are overlap-safe for the remaining offsets of at
        // least a copy unit: each load lies a full unit behind the store
        // which could observe it. Offsets 3, 5, 6, and 7 fall through to
        // the bytewise loop; they are rare in real data and word tricks
        // for them cost more in complexity than they return.
        _ => {
            if offset >= 16 {
                let limit = core::cmp::min(end, output.len().saturating_sub(15));
                while op < limit {
                    copy_u128_within(output, op, op - offset);
                    op += 16;
                }
            } else if offset >= 8 {
                let limit = core::cmp::min(end, output.len().saturating_sub(7));
                while op < limit {
                    copy_u64_within(output, op, op - offset);
                    op += 8;
                }
            }
        }
    }

    while op < end {
        output[op] = output[op - offset];
        op += 1;
    }
}

// -- Unaligned little-endian reads / intra-buffer wide copies --------------
//
// PORT NOTE: the oracle uses `std.mem.readInt(..., .little)` on a fixed-size
// slice plus `copyIntAt` to move one integer between byte ranges. The Rust
// equivalents below are pure safe slice operations. `try_into().unwrap()` on a
// fixed-length subslice is a compile-time-sized conversion that LLVM lowers to
// an unaligned load; the `unwrap` is provably infallible.

#[inline(always)]
fn read_u16(buf: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes(buf[pos..pos + 2].try_into().unwrap())
}

#[inline(always)]
fn read_u32(buf: &[u8], pos: usize) -> u32 {
    u32::from_le_bytes(buf[pos..pos + 4].try_into().unwrap())
}

#[inline(always)]
fn read_u64(buf: &[u8], pos: usize) -> u64 {
    u64::from_le_bytes(buf[pos..pos + 8].try_into().unwrap())
}

#[inline(always)]
fn copy_u16_within(buf: &mut [u8], dst: usize, src: usize) {
    let v = read_u16(buf, src);
    buf[dst..dst + 2].copy_from_slice(&v.to_le_bytes());
}

#[inline(always)]
fn copy_u64_within(buf: &mut [u8], dst: usize, src: usize) {
    let v = read_u64(buf, src);
    buf[dst..dst + 8].copy_from_slice(&v.to_le_bytes());
}

#[inline(always)]
fn copy_u128_within(buf: &mut [u8], dst: usize, src: usize) {
    let v = u128::from_le_bytes(buf[src..src + 16].try_into().unwrap());
    buf[dst..dst + 16].copy_from_slice(&v.to_le_bytes());
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib lz4_`
Expected: PASS — 7 tests pass (`lz4_compress_bound` + the 6 decoder tests).

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/compress/lz4.rs
git commit -m "vt/compress: LZ4 block decoder with bounded wide copies (WS7 A2)"
```

---

### Task A3: LZ4 sequence emitters

**Files:**
- Modify: `crates/rustty-vt/src/compress/lz4.rs`

**Interfaces:**
- Consumes: A1 constants, A2 `read_u16`.
- Produces: private `emit_sequence`, `emit_last_literals`, `encoded_length_bytes`, `write_length`, `write_u16_at`.

- [ ] **Step 1: Write the failing test**

Add inside `mod tests`:

```rust
    /// PORTS: oracle@main lz4.zig `test "maximum match offset compatibility vector"`
    ///
    /// Builds a maximum-offset block using the encoder's own `write_length`,
    /// then decodes it. This is the encoder/decoder handshake at the format's
    /// 16-bit offset boundary.
    #[test]
    fn lz4_maximum_match_offset_compatibility_vector() {
        let literal_len: usize = u16::MAX as usize;
        let extension_len = (literal_len - 15) / 255 + 1;
        let mut encoded = vec![0u8; 1 + extension_len + literal_len + 2];
        let mut output = vec![0u8; literal_len + MIN_MATCH];

        let mut op: usize = 0;
        encoded[op] = 0xF0;
        op += 1;
        write_length(&mut encoded, &mut op, literal_len - 15);
        for (i, byte) in encoded[op..op + literal_len].iter_mut().enumerate() {
            *byte = i as u8;
        }
        op += literal_len;
        write_u16_at(&mut encoded, op, u16::MAX);
        op += 2;

        assert_eq!(encoded.len(), op);
        assert_eq!(decompress(&encoded, &mut output), Ok(output.len()));
        assert_eq!(
            &encoded[1 + extension_len..1 + extension_len + 4],
            &output[literal_len..]
        );
    }

    /// Direct unit coverage for the extension-byte length encoding rule: an
    /// exact multiple of 255 needs a trailing zero byte.
    #[test]
    fn lz4_encoded_length_bytes_boundaries() {
        assert_eq!(encoded_length_bytes(0), 0);
        assert_eq!(encoded_length_bytes(14), 0);
        assert_eq!(encoded_length_bytes(15), 1);
        assert_eq!(encoded_length_bytes(15 + 254), 1);
        assert_eq!(encoded_length_bytes(15 + 255), 2);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib lz4_maximum_match_offset lz4_encoded_length_bytes`
Expected: FAIL — `cannot find function write_length in this scope`

- [ ] **Step 3: Write minimal implementation**

Insert above `mod tests`:

```rust
/// Emit one non-final sequence.
///
/// A sequence starts with a token, followed by optional literal length bytes,
/// the literals themselves, the two-byte offset, and optional match length
/// bytes. This function computes the complete size first so `OutputTooSmall`
/// is reported without partially writing a sequence.
fn emit_sequence(
    output: &mut [u8],
    op: &mut usize,
    literals: &[u8],
    offset: u16,
    match_len: usize,
) -> Result<(), CompressError> {
    debug_assert!(match_len >= MIN_MATCH);
    debug_assert!(offset > 0);

    let encoded_match_len = match_len - MIN_MATCH;

    // One byte is always needed for the token and two for the offset. Each
    // length may additionally need extension bytes after its token nibble.
    let required = 1
        + encoded_length_bytes(literals.len())
        + literals.len()
        + 2
        + encoded_length_bytes(encoded_match_len);
    if required > output.len() - *op {
        return Err(CompressError::OutputTooSmall);
    }

    let token_pos = *op;
    *op += 1;

    // Lengths below 15 fit directly in their nibble. Larger values put 15 in
    // the nibble and encode the remainder immediately after the token.
    output[token_pos] = ((core::cmp::min(literals.len(), 15) as u8) << 4)
        | (core::cmp::min(encoded_match_len, 15) as u8);

    // Literal length extensions precede the literals they describe.
    if literals.len() >= 15 {
        write_length(output, op, literals.len() - 15);
    }
    output[*op..*op + literals.len()].copy_from_slice(literals);
    *op += literals.len();

    // Match length extensions follow the offset because this is where the
    // decoder expects them in an LZ4 sequence.
    write_u16_at(output, *op, offset);
    *op += 2;
    if encoded_match_len >= 15 {
        write_length(output, op, encoded_match_len - 15);
    }

    Ok(())
}

/// Emit the literal-only sequence which terminates every block.
///
/// There is no offset or match length after these bytes. As with
/// `emit_sequence`, capacity is checked before modifying the output.
fn emit_last_literals(
    output: &mut [u8],
    op: &mut usize,
    literals: &[u8],
) -> Result<(), CompressError> {
    let required = 1 + encoded_length_bytes(literals.len()) + literals.len();
    if required > output.len() - *op {
        return Err(CompressError::OutputTooSmall);
    }

    output[*op] = (core::cmp::min(literals.len(), 15) as u8) << 4;
    *op += 1;
    if literals.len() >= 15 {
        write_length(output, op, literals.len() - 15);
    }
    output[*op..*op + literals.len()].copy_from_slice(literals);
    *op += literals.len();

    Ok(())
}

/// Return the number of extension bytes needed when a length is represented by
/// a token nibble plus zero or more bytes. An extended length always ends with
/// a byte below 255, so an exact multiple of 255 requires a final zero byte.
fn encoded_length_bytes(encoded_len: usize) -> usize {
    if encoded_len < 15 {
        return 0;
    }
    (encoded_len - 15) / 255 + 1
}

/// Write the portion of a length which did not fit in the token nibble.
///
/// Each 255 byte means "add 255 and continue". The final byte is always less
/// than 255 and may be zero.
fn write_length(output: &mut [u8], op: &mut usize, length_: usize) {
    let mut length = length_;
    while length >= 255 {
        output[*op] = 255;
        *op += 1;
        length -= 255;
    }
    output[*op] = length as u8;
    *op += 1;
}

/// Write an unaligned little-endian `u16` at `position`.
#[inline(always)]
fn write_u16_at(output: &mut [u8], position: usize, value: u16) {
    output[position..position + 2].copy_from_slice(&value.to_le_bytes());
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib lz4_`
Expected: PASS — 9 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/compress/lz4.rs
git commit -m "vt/compress: LZ4 sequence emitters and length encoding (WS7 A3)"
```

---

### Task A4: LZ4 compressor

**Files:**
- Modify: `crates/rustty-vt/src/compress/lz4.rs`

**Interfaces:**
- Consumes: A1–A3.
- Produces: `pub fn compress(input: &[u8], output: &mut [u8], table: &mut HashTable) -> Result<usize, CompressError>`; private `hash_sequence`, `remember_position`, `candidate_position`, `advance_search`, `match_begin`, `match_end_position`.

- [ ] **Step 1: Write the failing test**

Add inside `mod tests`:

```rust
    /// Shared round-trip assertion used by the corpus-style tests below.
    /// PORTS: oracle@main lz4.zig `fn expectRoundTrip`
    fn expect_round_trip(input: &[u8]) {
        let bound = compress_bound(input.len()).unwrap();
        let mut encoded = vec![0u8; bound];
        let mut decoded = vec![0u8; input.len()];

        let mut table: HashTable = [0; 1 << HASH_LOG];
        let encoded_len = compress(input, &mut encoded, &mut table).unwrap();
        assert_eq!(
            decompress(&encoded[..encoded_len], &mut decoded),
            Ok(input.len())
        );
        assert_eq!(&decoded[..], input);
    }

    /// PORTS: oracle@main lz4.zig `test "round trips boundary-sized inputs"`
    #[test]
    fn lz4_round_trips_boundary_sized_inputs() {
        const LENGTHS: [usize; 22] = [
            0, 1, 3, 4, 5, 12, 15, 16, 19, 20, 254, 255, 256, 269, 270, 271, 510, 511,
            512, 65_535, 65_536, 65_537,
        ];
        for len in LENGTHS {
            let buf: Vec<u8> = (0..len).map(|i| (i.wrapping_mul(31)) as u8).collect();
            expect_round_trip(&buf);
        }
    }

    /// PORTS: oracle@main lz4.zig `test "round trips compressible page-sized inputs"`
    #[test]
    fn lz4_round_trips_compressible_page_sized_inputs() {
        const PAGE_LEN: usize = 400 * 1024;

        let zeros = vec![0u8; PAGE_LEN];
        expect_round_trip(&zeros);

        let mut structured = vec![0u8; PAGE_LEN];
        for i in 0..PAGE_LEN / 8 {
            structured[i * 8] = (b' ' as usize + i % 95) as u8;
            structured[i * 8 + 4] = ((i / 80) % 16) as u8;
        }
        expect_round_trip(&structured);
    }

    /// PORTS: oracle@main lz4.zig `test "round trips deterministic random inputs"`
    ///
    /// PORT NOTE: the oracle uses `std.Random.DefaultPrng` (xoshiro256++). The
    /// exact stream does not matter — the property is round-trip identity over
    /// incompressible data — so this uses a small deterministic xorshift64*.
    #[test]
    fn lz4_round_trips_deterministic_random_inputs() {
        let mut state: u64 = 0x4C5A_3401;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..256 {
            let len = (next() % (32 * 1024)) as usize;
            let input: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            expect_round_trip(&input);
        }
    }

    /// PORTS: oracle@main lz4.zig `test "compress reports short output"`
    #[test]
    fn lz4_compress_reports_short_output() {
        let input = b"a terminal page needs enough output space";
        let mut table: HashTable = [0; 1 << HASH_LOG];
        let mut output = [0u8; 4];
        assert_eq!(
            compress(input, &mut output, &mut table),
            Err(CompressError::OutputTooSmall)
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib lz4_round_trips`
Expected: FAIL — `cannot find function compress in this scope`

- [ ] **Step 3: Write minimal implementation**

Insert above `mod tests`:

```rust
/// Compress `input` into a raw LZ4 block in `output`.
///
/// Returns the initialized length of `output`. The input and output buffers
/// must not overlap. `table` is scratch space and does not need to be
/// initialized by the caller; it is reset before use.
pub fn compress(
    input: &[u8],
    output: &mut [u8],
    table: &mut HashTable,
) -> Result<usize, CompressError> {
    if input.len() > MAX_INPUT_SIZE {
        return Err(CompressError::InputTooLarge);
    }

    // All-ones in either packed half means "no previous position".
    table.fill(u32::MAX);

    // `ip` is the current input position, `anchor` is the first literal not yet
    // emitted, and `op` is the next output position. A successful match emits
    // input[anchor..ip] as literals followed by the match, then moves both input
    // positions to the end of that match.
    let mut op: usize = 0;
    let mut anchor: usize = 0;

    // LZ4's format leaves the final five input bytes as literals and starts
    // the final match at least twelve bytes before the end. This is not
    // required by our safe decoder, but makes blocks compatible with fast
    // decoders that rely on the standard format restrictions. Inputs too
    // short for any match are emitted below as one literal-only sequence.
    if input.len() >= MATCH_FIND_LIMIT {
        let search_end = input.len() - MATCH_FIND_LIMIT;
        let match_end_limit = input.len() - LAST_LITERALS;
        let mut ip: usize = 0;
        let mut search_attempts: usize = 0;

        while ip <= search_end {
            // Hash the next four bytes and replace the table entry
            // immediately. Hash collisions are expected, so equality is
            // checked below before accepting a saved position as a match.
            let sequence = read_u32(input, ip);
            let hash = hash_sequence(sequence);
            let candidates = table[hash];
            remember_position(table, hash, ip);

            let mut match_pos = match candidate_position(ip, candidates as u16)
                .or_else(|| candidate_position(ip, (candidates >> 16) as u16))
            {
                Some(p) => p,
                None => {
                    advance_search(&mut ip, anchor, &mut search_attempts);
                    continue;
                }
            };

            if read_u32(input, match_pos) != sequence {
                // The nearest candidate collided. Fall back to the older
                // one, which must additionally match one byte beyond the
                // minimum so collision-prone minimum-length matches from
                // the stale half are not emitted.
                match candidate_position(ip, (candidates >> 16) as u16).filter(|&older| {
                    read_u32(input, older) == sequence
                        && input[older + MIN_MATCH] == input[ip + MIN_MATCH]
                }) {
                    Some(older) => match_pos = older,
                    None => {
                        advance_search(&mut ip, anchor, &mut search_attempts);
                        continue;
                    }
                }
            }

            // Pull the match backwards into the current literal run. This is
            // particularly helpful around aligned cell records. As with
            // forward extension, compare words before locating the first
            // differing byte.
            let (begin_position, begin_candidate) = match_begin(input, ip, match_pos, anchor);
            ip = begin_position;
            match_pos = begin_candidate;

            // We already compared the first four bytes. Continue up to the
            // point where the required last five literals begin.
            let match_end = match_end_position(
                input,
                ip + MIN_MATCH,
                match_pos + MIN_MATCH,
                match_end_limit,
            );

            emit_sequence(
                output,
                &mut op,
                &input[anchor..ip],
                (ip - match_pos) as u16,
                match_end - ip,
            )?;

            // The main loop jumps over the matched bytes rather than hashing
            // every position within them. Seed one position near the end so
            // an adjacent repeated record can still refer back into this
            // match. The next loop iteration will then seed `match_end`
            // normally.
            if match_end >= 2 && match_end - 2 + MIN_MATCH <= input.len() {
                let seed = match_end - 2;
                remember_position(table, hash_sequence(read_u32(input, seed)), seed);
            }

            ip = match_end;
            anchor = ip;
            search_attempts = 0;
        }
    }

    // Whatever remains after the last match is the terminal literal-only
    // sequence. For short inputs this is also the only sequence in the block.
    emit_last_literals(output, &mut op, &input[anchor..])?;
    Ok(op)
}

/// Map a four-byte input sequence to its scratch-table slot.
#[inline(always)]
fn hash_sequence(sequence: u32) -> usize {
    (sequence.wrapping_mul(HASH_MULTIPLIER) >> (32 - HASH_LOG as u32)) as usize
}

/// Add a position to one hash slot and shift the previous newest position into
/// the fallback half. A position whose low bits are 0xFFFF conflicts with the
/// sentinel and is simply not stored.
#[inline(always)]
fn remember_position(table: &mut HashTable, hash: usize, position: usize) {
    let low = position as u16;
    if low == u16::MAX {
        return;
    }
    table[hash] = (table[hash] << 16) | (low as u32);
}

/// Recover the nearest preceding position from a stored low half. Modular
/// subtraction directly produces the LZ4 offset within the current window.
#[inline(always)]
fn candidate_position(ip: usize, stored: u16) -> Option<usize> {
    if stored == u16::MAX {
        return None;
    }
    let distance = (ip as u16).wrapping_sub(stored);
    if distance == 0 {
        return None;
    }
    Some(ip - distance as usize)
}

/// Advance through a literal run. The first KiB inspects every byte without
/// maintaining a counter, which keeps ordinary terminal-page searches cheap.
/// Longer runs enable gradually increasing steps for incompressible data.
#[inline(always)]
fn advance_search(ip: &mut usize, anchor: usize, attempts: &mut usize) {
    if *attempts == 0 {
        *ip += 1;
        if *ip - anchor == 1024 {
            *attempts = 1;
        }
        return;
    }

    *attempts += 1;
    *ip += 1 + *attempts / 64;
}

/// Extend a match backwards without crossing the current literal anchor or the
/// beginning of the candidate. The returned positions preserve their offset.
fn match_begin(
    input: &[u8],
    position_: usize,
    candidate_: usize,
    anchor: usize,
) -> (usize, usize) {
    let mut position = position_;
    let mut candidate = candidate_;

    while core::cmp::min(position - anchor, candidate) >= 8 {
        let position_word = position - 8;
        let candidate_word = candidate - 8;
        let difference = read_u64(input, position_word) ^ read_u64(input, candidate_word);
        if difference != 0 {
            // Little-endian reads make the MOST-significant differing bit map
            // to the LAST byte in memory, so leading zeros count equal trailing
            // bytes of the two words.
            let equal_bytes = (difference.leading_zeros() / 8) as usize;
            return (position - equal_bytes, candidate - equal_bytes);
        }

        position = position_word;
        candidate = candidate_word;
    }

    while position > anchor && candidate > 0 && input[position - 1] == input[candidate - 1] {
        position -= 1;
        candidate -= 1;
    }
    (position, candidate)
}

/// Return the first input position where two matching runs differ, or `limit`
/// when they remain equal. Both positions are known to have matched through
/// `MIN_MATCH` before this is called.
///
/// PORT NOTE: named `match_end_position` because `match_end` shadows a local in
/// `compress`; the oracle's function is `matchEnd`.
fn match_end_position(
    input: &[u8],
    position_: usize,
    candidate_: usize,
    limit: usize,
) -> usize {
    let mut position = position_;
    let mut candidate = candidate_;

    // Reading as little endian makes the least-significant differing bit map
    // to the earliest byte in memory on every target.
    while limit.saturating_sub(position) >= 8 {
        let difference = read_u64(input, position) ^ read_u64(input, candidate);
        if difference != 0 {
            return position + (difference.trailing_zeros() / 8) as usize;
        }

        position += 8;
        candidate += 8;
    }

    while position < limit && input[position] == input[candidate] {
        position += 1;
        candidate += 1;
    }
    position
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib lz4_ --release`
Expected: PASS — 14 tests. (`--release` because the 400 KiB and 65 KiB round-trips are slow in a debug build; also run the debug variant once: `cargo test -p rustty-vt --lib lz4_`.)

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/compress/lz4.rs
git commit -m "vt/compress: LZ4 fast compressor with two-slot hash table (WS7 A4)"
```

---

### Task A5: LZ4 differential property suite

**Files:**
- Create: `crates/rustty-vt/src/compress/lz4_differential.rs`
- Modify: `crates/rustty-vt/src/compress.rs`

**Oracle:** `oracle@main:src/terminal/compress/lz4_differential.zig` (493 lines, added by `9a4bd2120`). Read it in full before writing this task: it defines (1) an **independent format walker** that re-parses an encoded block without using the decoder, (2) the standard-format restriction checks (final five bytes literal; final match begins ≥12 bytes before the end), (3) wrong-size output rejection, and (4) corruption/truncation decoding.

**Interfaces:**
- Consumes: `super::lz4::{compress, decompress, compress_bound, HashTable, CompressError, DecompressError}`.
- Produces: nothing public — `#[cfg(test)]` only.
- The exhaustive variant is gated on an environment variable, mirroring the oracle's `GHOSTTY_LZ4_SLOW`: **`RUSTTY_LZ4_SLOW=1`** (R2 casing rule: `GHOSTTY_` → `RUSTTY_`).

- [ ] **Step 1: Write the failing test**

Create `crates/rustty-vt/src/compress/lz4_differential.rs` with the walker and the four property tests. The walker (the part that must be independent of the decoder):

```rust
//! Differential property suite for the raw LZ4 block codec.
//!
//! PORTS: ghostty src/terminal/compress/lz4_differential.zig (oracle
//! origin/main @ 9a4bd2120).
//!
//! Every codec needs: round-trip identity, an independent format walker,
//! wrong-size output rejection, and corruption/truncation decoding. The light
//! version runs in the normal test suite; the exhaustive version is gated
//! behind `RUSTTY_LZ4_SLOW=1` so the default suite stays fast.

use super::lz4::{compress, compress_bound, decompress, CompressError, DecompressError, HashTable};

/// What an independent walk of an encoded block observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Walk {
    /// Total decoded length implied by the sequences.
    decoded_len: usize,
    /// Number of sequences in the block, including the final literal-only one.
    sequences: usize,
    /// Byte offset of the first literal of the final sequence.
    final_literals_start: usize,
    /// Number of literals in the final sequence.
    final_literals: usize,
    /// Decoded position at which the last match ended, if any match existed.
    last_match_end: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalkError {
    Truncated,
    InvalidOffset,
}

/// Re-parse an encoded block WITHOUT using `decompress`. This is deliberately
/// the slow, obvious implementation: it exists to disagree with the optimized
/// decoder when the decoder is wrong.
fn walk(input: &[u8]) -> Result<Walk, WalkError> {
    let mut ip: usize = 0;
    let mut produced: usize = 0;
    let mut sequences: usize = 0;
    let mut final_literals_start: usize = 0;
    let mut final_literals: usize = 0;
    let mut last_match_end: Option<usize> = None;

    loop {
        if ip == input.len() {
            return Ok(Walk {
                decoded_len: produced,
                sequences,
                final_literals_start,
                final_literals,
                last_match_end,
            });
        }

        let token = input[ip];
        ip += 1;
        sequences += 1;

        let literal_len = walk_length(input, &mut ip, (token >> 4) as usize)?;
        if literal_len > input.len() - ip {
            return Err(WalkError::Truncated);
        }
        final_literals_start = ip;
        final_literals = literal_len;
        ip += literal_len;
        produced += literal_len;

        if ip == input.len() {
            return Ok(Walk {
                decoded_len: produced,
                sequences,
                final_literals_start,
                final_literals,
                last_match_end,
            });
        }

        if input.len() - ip < 2 {
            return Err(WalkError::Truncated);
        }
        let offset = u16::from_le_bytes([input[ip], input[ip + 1]]) as usize;
        ip += 2;
        if offset == 0 || offset > produced {
            return Err(WalkError::InvalidOffset);
        }

        let match_len = walk_length(input, &mut ip, (token & 0x0F) as usize)? + 4;
        produced += match_len;
        last_match_end = Some(produced);
    }
}

fn walk_length(input: &[u8], ip: &mut usize, nibble: usize) -> Result<usize, WalkError> {
    let mut length = nibble;
    if nibble != 15 {
        return Ok(length);
    }
    loop {
        if *ip >= input.len() {
            return Err(WalkError::Truncated);
        }
        let value = input[*ip];
        *ip += 1;
        length += value as usize;
        if value != 255 {
            return Ok(length);
        }
    }
}

fn encode(input: &[u8]) -> Vec<u8> {
    let mut encoded = vec![0u8; compress_bound(input.len()).unwrap()];
    let mut table: HashTable = [0; 4096];
    let len = compress(input, &mut encoded, &mut table).unwrap();
    encoded.truncate(len);
    encoded
}

/// The corpora used by both the light and exhaustive runs.
fn corpora(exhaustive: bool) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = Vec::new();

    // Structured page-shaped data: 8-byte cell records, mostly zero.
    let page_len = if exhaustive { 400 * 1024 } else { 32 * 1024 };
    let mut structured = vec![0u8; page_len];
    for i in 0..page_len / 8 {
        structured[i * 8] = (b' ' as usize + i % 95) as u8;
        structured[i * 8 + 4] = ((i / 80) % 16) as u8;
    }
    out.push(structured);

    // All zeros: the maximum-ratio case.
    out.push(vec![0u8; page_len]);

    // Text-like data.
    let line = b"the quick brown fox jumps over the lazy dog\n";
    let mut text = Vec::with_capacity(page_len);
    while text.len() < page_len {
        text.extend_from_slice(line);
    }
    text.truncate(page_len);
    out.push(text);

    // Incompressible data plus a set of short boundary lengths.
    let mut state: u64 = 0x4C5A_3402;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    out.push((0..page_len).map(|_| next() as u8).collect());

    let count = if exhaustive { 4096 } else { 256 };
    for _ in 0..count {
        let len = (next() % 8192) as usize;
        out.push((0..len).map(|_| (next() % 4) as u8).collect());
    }

    out
}

fn exhaustive_enabled() -> bool {
    std::env::var("RUSTTY_LZ4_SLOW").as_deref() == Ok("1")
}

/// PORTS: oracle@main lz4_differential.zig — round-trip identity.
#[test]
fn lz4_differential_round_trip_identity() {
    for input in corpora(exhaustive_enabled()) {
        let encoded = encode(&input);
        let mut decoded = vec![0u8; input.len()];
        assert_eq!(decompress(&encoded, &mut decoded), Ok(input.len()));
        assert_eq!(decoded, input);
    }
}

/// PORTS: oracle@main lz4_differential.zig — independent walker agreement +
/// standard-format restrictions (final five bytes literal; the final match
/// begins at least twelve bytes before the end).
#[test]
fn lz4_differential_walker_agrees_and_format_is_standard() {
    for input in corpora(exhaustive_enabled()) {
        let encoded = encode(&input);
        let observed = walk(&encoded).expect("compressor emits a walkable block");
        assert_eq!(observed.decoded_len, input.len());

        // Every block ends with a literal-only sequence.
        assert!(observed.sequences >= 1);
        assert_eq!(
            observed.final_literals_start + observed.final_literals,
            encoded.len()
        );

        if let Some(last_match_end) = observed.last_match_end {
            // The final five input bytes must be literals.
            assert!(
                input.len() - last_match_end >= 5,
                "final match ends {} bytes before the end of a {}-byte input",
                input.len() - last_match_end,
                input.len()
            );
            // A match may not BEGIN in the final twelve bytes; the shortest
            // match is four bytes, so its end is at least twelve minus four
            // from the end when the restriction holds.
            assert!(input.len() >= 12);
        }
    }
}

/// PORTS: oracle@main lz4_differential.zig — wrong-size output rejection.
#[test]
fn lz4_differential_rejects_wrong_size_output() {
    for input in corpora(false) {
        if input.is_empty() {
            continue;
        }
        let encoded = encode(&input);

        let mut too_small = vec![0u8; input.len() - 1];
        assert!(matches!(
            decompress(&encoded, &mut too_small),
            Err(DecompressError::OutputTooSmall) | Err(DecompressError::OutputSizeMismatch)
        ));

        let mut too_large = vec![0u8; input.len() + 1];
        assert_eq!(
            decompress(&encoded, &mut too_large),
            Err(DecompressError::OutputSizeMismatch)
        );
    }
}

/// PORTS: oracle@main lz4_differential.zig — corruption and truncation must
/// never panic, read out of bounds, or write out of bounds. Any Result is
/// acceptable; the property is that decoding TERMINATES safely.
#[test]
fn lz4_differential_corruption_and_truncation_are_memory_safe() {
    let mut state: u64 = 0x4C5A_3403;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    for input in corpora(false) {
        if input.len() < 32 {
            continue;
        }
        let encoded = encode(&input);
        let mut decoded = vec![0u8; input.len()];

        // Truncation at every 1/16th boundary.
        for k in 1..16 {
            let cut = encoded.len() * k / 16;
            let _ = decompress(&encoded[..cut], &mut decoded);
        }

        // Single-byte corruption at pseudo-random positions.
        for _ in 0..64 {
            let mut corrupted = encoded.clone();
            let pos = (next() as usize) % corrupted.len();
            corrupted[pos] ^= (next() as u8) | 1;
            let _ = decompress(&corrupted, &mut decoded);
        }
    }
}

/// The exhaustive suite is a no-op unless `RUSTTY_LZ4_SLOW=1`, so the default
/// test run stays fast. This test documents the switch and fails loudly if the
/// gate itself breaks.
#[test]
fn lz4_differential_exhaustive_gate() {
    if !exhaustive_enabled() {
        return;
    }
    for input in corpora(true) {
        let encoded = encode(&input);
        let observed = walk(&encoded).unwrap();
        assert_eq!(observed.decoded_len, input.len());
        let mut decoded = vec![0u8; input.len()];
        assert_eq!(decompress(&encoded, &mut decoded), Ok(input.len()));
        assert_eq!(decoded, input);
    }
}
```

Add to `crates/rustty-vt/src/compress.rs`:

```rust
#[cfg(test)]
mod lz4_differential;
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib lz4_differential`
Expected: FAIL — `file not found for module lz4_differential` before the file is created; after creating it, any assertion that the codec violates is a real parity bug and must be fixed in `lz4.rs`, not in the test.

- [ ] **Step 3: Write minimal implementation**

No production code changes are expected. If `lz4_differential_walker_agrees_and_format_is_standard` fails, the compressor is emitting non-standard blocks — re-read `oracle@main:src/terminal/compress/lz4.zig` `compress` and fix `match_end_limit` / `search_end` in `compress`.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rustty-vt --lib lz4_ --release
RUSTTY_LZ4_SLOW=1 cargo test -p rustty-vt --lib lz4_differential_exhaustive_gate --release
```
Expected: PASS for both; the exhaustive run takes minutes, not hours.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/compress/lz4_differential.rs crates/rustty-vt/src/compress.rs
git commit -m "vt/compress: LZ4 differential property suite (RUSTTY_LZ4_SLOW) (WS7 A5)"
```

---

### Task A6: Phase A gate — wasm build + Miri + commit

**Files:**
- Test only. No production changes expected.

**Interfaces:**
- Consumes: everything from A1–A5.
- Produces: the recorded evidence that the codec is target-portable and SB-clean, which Phase B depends on.

**Why:** the oracle's `AGENTS.md` requires the codec keep building for `wasm32-freestanding` (it ships in libghostty-vt). rustty's equivalent is the `wasm32-unknown-unknown` vt build driven by `.port/tools/build-vt-wasm.sh`.

- [ ] **Step 1: Run the wasm target build**

```bash
cd /Users/gael/projects/rustty
cargo build -p rustty-vt --target wasm32-unknown-unknown
```
Expected: `Finished` with no errors. The codec must not reference `std::env` outside `#[cfg(test)]`, must not use `std::time`, and must not use any `mmap`-adjacent code (it does not).

- [ ] **Step 2: Run the codec Miri lane**

```bash
bash .port/tools/miri-run.sh rustty-vt lz4_ 4 20 10
```
Expected: every `compress::lz4::tests::lz4_*` and `compress::lz4_differential::lz4_differential_*` test reports `ok`. **Zero `UB`, zero `TIMEOUT`.** If `lz4_round_trips_compressible_page_sized_inputs` TIMEOUTs (400 KiB under Miri is slow), reduce nothing — instead record it in `.port/phase-2/MIRI_EXCEPTIONS.md` with the reason "byte-loop over 400 KiB; covered natively" and re-run the remaining filter. Any actual `UB` line is a hard stop.

- [ ] **Step 3: Run the both-platform build gate**

```bash
cargo build --workspace
ssh rvm 'cd ~/projects/rustty && cargo build --workspace'
```
Expected: both `Finished`.

- [ ] **Step 4: Record the phase result**

Append to `.port/phase-2/MIRI_EXCEPTIONS.md` only if Step 2 produced a documented TIMEOUT. Otherwise no file change.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "vt/compress: phase A gate — wasm32 build + Miri lane green (WS7 A6)"
```

**PHASE A COMPLETE.** `crates/rustty-vt/src/compress/lz4.rs` is a standalone, dependency-free, `unsafe`-free LZ4 block codec with the oracle's full test set plus a differential suite. It has no callers yet and can be merged to `main` on its own.

---

# PHASE B — Reclamation, compressed page representation, PageList integration

**Requires Phase A.**

**Oracle commits, in upstream order:**

| # | hash | subject | rustty task |
|---|---|---|---|
| 1 | `16e4b5e98` | terminal: track page ownership explicitly on pagelist nodes | B4 |
| 2 | `f6fd4cb08` | terminal: generalize page memory reclamation | B1 |
| 3 | `ebc3ffd22` | terminal: add compressed page representation | B2, B3 |
| 4 | `421fe8dab` | terminal: integrate compressed pages into PageList | B5, B6 |
| 5 | `9156ada16` | terminal: add cold-history compression pass | B7 |
| 6 | `70e788f06` | terminal: add incremental scrollback compression | B8, B9 |
| 7 | `e7969ed43` | terminal: make PageList compression self-contained | B3 (`clone_buf`), B8 |
| 8 | `f8c217e55` | terminal: track pending scrollback compression | B8 |
| 9 | `181254d36` | terminal: debounce compression by page activity | B8 |
| 10 | `95685afd2` | terminal: compress offscreen scrollback history | B8 |

Read each with `git -C /Users/gael/projects/ghostty show <hash>`. The **final** state of `oracle@main:src/terminal/PageList.zig` is the parity target for B5–B9 — port the end state, using the individual commits only to understand *why* a piece exists.

**Boundary with WS8/BL-2:** upstream `896aca499` ("return free-listed page memory to the OS") is BL-2, owned by WS8. It uses the same `mem.decommit(.zero, ...)` primitive this phase introduces. WS7 lands `mem.rs` and calls `decommit(.zero, ...)` from `destroy_node_ext` **only where `oracle@main:PageList.zig` `destroyNodeExt` does** (which already includes the BL-2 behavior in the final file). Do not port BL-2's benchmark/tests here; note in the commit message that BL-2's production hunk is subsumed and tell WS8 in `MAIN_PARITY.md` (Task E4).

---

### Task B1: Retained-mapping reclamation (`mem.rs`)

**Files:**
- Create: `crates/rustty-vt/src/mem.rs`
- Modify: `crates/rustty-vt/src/lib.rs`

**Oracle:** `oracle@main:src/terminal/mem.zig` (222 lines, from `f6fd4cb08`).

**Interfaces:**
- Produces:
  - `pub enum DecommitMode { Zero, Strict }`
  - `pub fn can_reclaim(mode: DecommitMode) -> bool`
  - `pub unsafe fn decommit(mode: DecommitMode, ptr: *mut u8, len: usize, dirty_len: usize) -> bool`
  - `pub unsafe fn recommit(ptr: *mut u8, len: usize)`
  - `pub const COMPRESSION_ENABLED: bool` re-exported from `lib.rs` as `rustty_vt::COMPRESSION_ENABLED`.
- PORT NOTE for the implementer: the oracle takes `memory: []align(page_size_min) u8`. rustty's `Page` stores `memory_ptr`/`memory_len` (no slice), and passing a `&mut [u8]` here would create exactly the `&mut`-across-raw-alias hazard the port treats as UB. So these take raw pointer + length and are `unsafe fn`.

- [ ] **Step 1: Write the failing test**

Append to `crates/rustty-vt/src/mem.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn aligned_alloc(len: usize) -> (*mut u8, std::alloc::Layout) {
        let layout = std::alloc::Layout::from_size_align(len, crate::page::PAGE_SIZE_MIN).unwrap();
        // SAFETY: len > 0 and the layout is valid.
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!ptr.is_null());
        (ptr, layout)
    }

    /// PORTS: oracle@main mem.zig
    /// `test "decommit with zero fallback clears the dirty prefix"`
    #[test]
    fn mem_decommit_with_zero_fallback_clears_the_dirty_prefix() {
        let memory_len = 2 * crate::page::PAGE_SIZE_MIN;
        let (ptr, layout) = aligned_alloc(memory_len);
        unsafe {
            core::ptr::write_bytes(ptr, 0xAA, memory_len);
            let _ = decommit(DecommitMode::Zero, ptr, memory_len, memory_len);
            assert!(core::slice::from_raw_parts(ptr, memory_len).iter().all(|&b| b == 0));

            // The tail is already zero by contract, so a partially dirty mapping
            // only needs its dirty prefix cleared.
            core::ptr::write_bytes(ptr, 0xAA, 1024);
            let _ = decommit(DecommitMode::Zero, ptr, memory_len, 1024);
            assert!(core::slice::from_raw_parts(ptr, memory_len).iter().all(|&b| b == 0));

            std::alloc::dealloc(ptr, layout);
        }
    }

    /// PORTS: oracle@main mem.zig
    /// `test "strict decommit retains the mapping for recommit"`
    #[test]
    fn mem_strict_decommit_retains_the_mapping_for_recommit() {
        let memory_len = 2 * crate::page::PAGE_SIZE_MIN;
        let (ptr, layout) = aligned_alloc(memory_len);
        unsafe {
            core::ptr::write_bytes(ptr, 0xAA, memory_len);
            assert!(decommit(DecommitMode::Strict, ptr, memory_len, memory_len));
            assert!(core::slice::from_raw_parts(ptr, memory_len).iter().all(|&b| b == 0));

            recommit(ptr, memory_len);
            core::ptr::write_bytes(ptr, 0xBB, memory_len);
            assert!(core::slice::from_raw_parts(ptr, memory_len).iter().all(|&b| b == 0xBB));

            std::alloc::dealloc(ptr, layout);
        }
    }

    /// PORTS: oracle@main mem.zig `test "test builds can reclaim retained mappings"`
    #[test]
    fn mem_test_builds_can_reclaim_retained_mappings() {
        assert!(can_reclaim(DecommitMode::Zero));
        assert!(can_reclaim(DecommitMode::Strict));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib mem_`
Expected: FAIL — `failed to resolve: use of undeclared crate or module mem`

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/rustty-vt/src/mem.rs`:

```rust
//! Virtual-memory operations shared by terminal page owners.
//!
//! Terminal pages use page-aligned, page-multiple mappings. This module can
//! discard the physical pages behind one of those mappings without releasing
//! its virtual address range, then prepare the same range for reuse. It does
//! not allocate memory or decide which terminal pages should be discarded.
//!
//! PORTS: ghostty src/terminal/mem.zig (oracle origin/main @ f6fd4cb08).
//!
//! PORT NOTE (WS7): the oracle's `[]align(page_size_min) u8` argument becomes a
//! raw pointer + length here. A `&mut [u8]` over page memory would be a live
//! `&mut` alias of the same bytes a `*mut Page` also names, which this port
//! treats as Stacked-Borrows UB (see the WS7 plan's raw-pointer discipline).
//!
//! PORT NOTE (WS7): the oracle reaches `madvise` through `std.posix`. rustty
//! deliberately does NOT depend on the `libc` crate, so this declares the one
//! syscall it needs with `extern "C"` exactly like the hand-rolled
//! `mmap`/`munmap` in `page.rs::page_alloc`.

use crate::page::PAGE_SIZE_MIN;

// const log = std.log.scoped(.terminal_mem);

/// What guarantee decommit must provide when the OS cannot discard a mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecommitMode {
    /// The dirty prefix must read as zero after this call, even when physical
    /// reclamation is unavailable. Bytes after the prefix must already be zero.
    Zero,

    /// Physical-memory reclamation is required. Do not touch the mapping when
    /// reclamation is unsupported or fails; report the failure to the caller.
    Strict,
}

// -- madvise bindings ------------------------------------------------------
//
// PORT NOTE: values verified against <sys/mman.h> on both targets.
//   Darwin: MADV_DONTNEED 4, MADV_FREE 5, MADV_FREE_REUSABLE 7, MADV_FREE_REUSE 8
//   Linux:  MADV_DONTNEED 4
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
const MADV_DONTNEED: i32 = 4;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const MADV_FREE: i32 = 5;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const MADV_FREE_REUSABLE: i32 = 7;
#[cfg(any(target_os = "macos", target_os = "ios"))]
const MADV_FREE_REUSE: i32 = 8;

#[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
extern "C" {
    #[link_name = "madvise"]
    fn libc_madvise(addr: *mut core::ffi::c_void, len: usize, advice: i32) -> i32;
}

/// Issue one `madvise` call. Returns true when the kernel accepted it.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "linux"))]
#[inline]
unsafe fn madvise(ptr: *mut u8, len: usize, advice: i32) -> bool {
    libc_madvise(ptr as *mut core::ffi::c_void, len, advice) == 0
}

#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
#[inline]
unsafe fn madvise(_ptr: *mut u8, _len: usize, _advice: i32) -> bool {
    false
}

/// Return whether this target can reclaim physical memory for `mode` while
/// retaining the mapping's virtual address range.
///
/// Test builds support both modes because `decommit` simulates reclamation by
/// clearing the supplied range. Runtime reclamation is intentionally limited
/// to 64-bit Linux and Darwin. Other targets must leave strict callers' memory
/// resident; zero mode still provides its documented memset fallback through
/// `decommit` even when this function returns false.
#[inline]
pub fn can_reclaim(mode: DecommitMode) -> bool {
    // Keeping the match exhaustive makes additions to DecommitMode choose
    // target support explicitly rather than inheriting it accidentally.
    match mode {
        DecommitMode::Zero | DecommitMode::Strict => {
            // Tests never call into the OS because their allocator ranges can
            // share mappings with unrelated allocations. `decommit` simulates
            // successful reclamation by zeroing the requested range instead,
            // so both modes are always available to tests on every target.
            if cfg!(test) {
                return true;
            }

            // Compression currently retains complete page mappings for its
            // lifetime. Limit the initial runtime support to 64-bit address
            // spaces where that virtual-memory cost is negligible and where
            // the retained-mapping behavior has been validated.
            if cfg!(not(target_pointer_width = "64")) {
                return false;
            }

            // Linux provides MADV_DONTNEED, which immediately discards pages
            // from a private anonymous mapping and faults them back as zeroes.
            // Darwin provides the paired MADV_FREE_REUSABLE/FREE_REUSE
            // operations used below.
            cfg!(any(target_os = "linux", target_os = "macos", target_os = "ios"))
        }
    }
}

/// Discard physical pages while retaining a mapping's virtual address range.
///
/// The complete mapping must be page-aligned and a multiple of the minimum
/// system page size. `dirty_len` identifies the prefix whose contents may be
/// nonzero. Strict mode requires the complete mapping to be dirty because a
/// successful discard invalidates all of its contents.
///
/// The return value reports whether the OS accepted the reclamation request.
/// Test builds return true after simulating reclamation by zeroing dirty bytes.
/// In zero mode, the requested bytes are guaranteed to be zero regardless of
/// the return value.
///
/// # Safety
/// `ptr` must be a live, page-aligned mapping of at least `len` bytes with no
/// live Rust reference aliasing it, and `dirty_len <= len`.
pub unsafe fn decommit(
    mode: DecommitMode,
    ptr: *mut u8,
    len: usize,
    dirty_len: usize,
) -> bool {
    debug_assert!(len > 0);
    debug_assert!(ptr as usize % PAGE_SIZE_MIN == 0);
    debug_assert!(len % PAGE_SIZE_MIN == 0);
    debug_assert!(dirty_len <= len);
    if matches!(mode, DecommitMode::Strict) {
        debug_assert!(dirty_len == len);
    }

    // Testing allocator ranges may share an OS mapping with unrelated memory,
    // so madvise is not safe. Zeroing models the only content guarantee callers
    // have after a successful discard.
    if cfg!(test) {
        core::ptr::write_bytes(ptr, 0, dirty_len);
        return true;
    }

    // DONTNEED immediately reclaims private anonymous pages on Linux and
    // faults them back as zero-filled pages. We deliberately avoid MADV_FREE:
    // it does not reduce RSS until memory pressure and does not guarantee that
    // the next read is zero.
    #[cfg(target_os = "linux")]
    {
        if madvise(ptr, len, MADV_DONTNEED) {
            return true;
        }
        // log.warn("madvise(DONTNEED) failed");
        if matches!(mode, DecommitMode::Strict) {
            return false;
        }
        // Zero mode falls through to the memset below.
    }

    // FREE_REUSABLE removes the range from the Darwin process footprint while
    // retaining its mapping. Zero mode clears its dirty prefix first because
    // the kernel may preserve the contents. Strict mode avoids that write
    // because its caller will replace the entire mapping after recommit.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        if matches!(mode, DecommitMode::Zero) {
            core::ptr::write_bytes(ptr, 0, dirty_len);
        }

        if madvise(ptr, len, MADV_FREE_REUSABLE) {
            return true;
        }

        match mode {
            DecommitMode::Strict => {
                // log.warn("madvise(FREE_REUSABLE) failed");
                return false;
            }
            DecommitMode::Zero => {
                // Plain FREE can still reclaim the already-zero mapping
                // under pressure and does not require a reuse pairing.
                let _ = madvise(ptr, len, MADV_FREE);
                return false;
            }
        }
    }

    #[allow(unreachable_code)]
    {
        if matches!(mode, DecommitMode::Zero) {
            core::ptr::write_bytes(ptr, 0, dirty_len);
        }
        false
    }
}

/// Prepare a mapping previously passed to `decommit` for reuse.
///
/// Linux and test builds need no explicit operation. Darwin pairs
/// FREE_REUSABLE with FREE_REUSE so pages touched by the caller are accounted
/// to the process again. Failure does not invalidate the retained mapping, so
/// reuse can continue after logging the accounting failure.
///
/// # Safety
/// Same contract as [`decommit`].
pub unsafe fn recommit(ptr: *mut u8, len: usize) {
    debug_assert!(len > 0);
    debug_assert!(ptr as usize % PAGE_SIZE_MIN == 0);
    debug_assert!(len % PAGE_SIZE_MIN == 0);

    if cfg!(test) {
        return;
    }
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        if !madvise(ptr, len, MADV_FREE_REUSE) {
            // log.warn("madvise(FREE_REUSE) failed");
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        let _ = (ptr, len);
    }
}
```

In `crates/rustty-vt/src/lib.rs` add, next to the other `pub mod` lines:

```rust
pub mod mem;

/// Whether this target supports terminal page compression.
///
/// PORTS: oracle@main src/terminal/main.zig:80
/// `pub const compression_enabled = @import("mem.zig").canReclaim(.strict);`
///
/// PORT NOTE: `can_reclaim` is a `const fn`-shaped `cfg!` chain, but Rust cannot
/// evaluate it in a `const` initializer while it reads `cfg!(test)`, so this is
/// a function. Call sites use `if rustty_vt::compression_enabled()` exactly
/// where the oracle writes `if (comptime terminalpkg.compression_enabled)`.
#[inline]
pub fn compression_enabled() -> bool {
    mem::can_reclaim(mem::DecommitMode::Strict)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib mem_`
Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/mem.rs crates/rustty-vt/src/lib.rs
git commit -m "vt: generalize page memory reclamation via retained-mapping madvise (WS7 B1)"
```

---

### Task B2: `Page::shallow_copy`

**Files:**
- Modify: `crates/rustty-vt/src/page_impl.rs` (next to `clone_buf`, ~line 804)

**Interfaces:**
- Produces: `pub fn shallow_copy(&self) -> Page` — a metadata-only copy that **aliases the same `memory_ptr`/`memory_len`** and is never `owned`.
- Consumed by: B3 (`CompressedPage::init`, `restore`), B6 (`Node::restore`).

**Why:** the oracle writes `.page = source.*` and `var result = self.page;` — a plain Zig struct copy. rustty's `Page` is not `Copy` (it holds `StyleSet`/`hyperlink::Set` which derive `Clone`), so the copy needs an explicit helper. It must NOT set `owned = true`, or two `Page` values would both try to `munmap` the mapping.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `crates/rustty-vt/src/page_tests.rs`:

```rust
/// A shallow copy aliases the SAME backing mapping and never owns it.
#[test]
fn page_shallow_copy_aliases_the_same_mapping_and_is_unowned() {
    let mut page = Page::init(Capacity {
        cols: 10,
        rows: 8,
        ..std_capacity()
    })
    .unwrap();
    page.size = PageSize { cols: 9, rows: 7 };
    page.dirty = true;

    let copy = page.shallow_copy();
    assert_eq!(copy.memory_base(), page.memory_base());
    assert_eq!(copy.memory_len(), page.memory_len());
    assert_eq!(copy.size, page.size);
    assert_eq!(copy.capacity, page.capacity);
    assert!(copy.dirty);
    // Dropping `copy` must not free the mapping; `page.deinit()` below would
    // then double-free and the test allocator/leak gate would catch it.
    drop(copy);
    page.deinit();
}
```

(If `memory_base()`/`memory_len()` accessors do not exist under those names, use the existing accessor names found in `page_impl.rs` — grep `fn memory_base` / `fn memory_len` before writing the test.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib page_shallow_copy_aliases`
Expected: FAIL — `no method named shallow_copy found for struct Page`

- [ ] **Step 3: Write minimal implementation**

Add to `impl Page` in `crates/rustty-vt/src/page_impl.rs`, immediately before `clone_buf`:

```rust
    /// Copy every field of this page WITHOUT copying or reallocating its
    /// backing memory. The result aliases the same mapping.
    ///
    /// PORT NOTE (WS7): the oracle expresses this as a plain Zig struct copy
    /// (`.page = source.*` in compress/Page.zig, `var result = self.page` in
    /// `restore`). rustty's `Page` is not `Copy`, so the copy is explicit. The
    /// result is ALWAYS `owned = false`: the original owner keeps the mapping,
    /// and a shallow copy must never `munmap` it.
    pub fn shallow_copy(&self) -> Page {
        Page {
            memory_ptr: self.memory_ptr,
            memory_len: self.memory_len,
            owned: false,
            rows: self.rows,
            cells: self.cells,
            dirty: self.dirty,
            string_alloc: self.string_alloc,
            grapheme_alloc: self.grapheme_alloc,
            grapheme_map: self.grapheme_map,
            styles: self.styles.clone(),
            hyperlink_map: self.hyperlink_map,
            hyperlink_set: self.hyperlink_set.clone(),
            size: self.size,
            capacity: self.capacity,
            pause_integrity_checks: self.pause_integrity_checks,
        }
    }
```

(Copy the exact field list from the `Page` struct definition at `crates/rustty-vt/src/page_impl.rs:115`; if a field has been added since this plan was written, include it — `cargo build` will name any omission.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib page_shallow_copy_aliases`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/page_impl.rs crates/rustty-vt/src/page_tests.rs
git commit -m "vt/page: shallow_copy for alias-preserving page metadata copies (WS7 B2)"
```

---

### Task B3: Compressed page representation

**Files:**
- Create: `crates/rustty-vt/src/compress/page.rs`
- Modify: `crates/rustty-vt/src/compress.rs`

**Oracle:** `oracle@main:src/terminal/compress/Page.zig` (398 lines: `ebc3ffd22` + `421fe8dab` + `e7969ed43`). Port the **final** file including `cloneBuf`.

**Interfaces:**
- Produces:
  - `pub struct CompressedPage { pub page: Page, pub encoded: Vec<u8> }`
  - `pub fn required_scratch(raw_len: usize) -> Result<usize, lz4::CompressError>`
  - `pub unsafe fn init(source: *const Page, scratch: &mut [u8], table: &mut lz4::HashTable) -> Result<Option<CompressedPage>, lz4::CompressError>`
  - `pub unsafe fn restore(&self) -> Result<Page, lz4::DecompressError>`
  - `pub unsafe fn clone_buf(&self, memory: *mut u8, memory_len: usize) -> Result<Page, lz4::DecompressError>`
- PORT NOTE: the oracle carries an `alloc: Allocator` field so a node can free its encoding without a back-reference to the PageList. rustty's `Vec<u8>` carries `Global`, so the field is unnecessary and `deinit` is `Drop`. Record this in the module doc.
- PORT NOTE: `init` takes `*const Page` (not `&Page`) so `PageList::compress_page` never forms a shared reference to a page it is about to replace in the node union.

- [ ] **Step 1: Write the failing test**

Create `crates/rustty-vt/src/compress/page.rs` with a `#[cfg(test)] mod tests` porting the oracle's four tests. Names:

| oracle test | rustty test fn |
|---|---|
| `"compressed Page retained mapping round trip"` | `compressed_page_retained_mapping_round_trip` |
| `"compressed Page requires the maximum useful scratch"` | `compressed_page_requires_the_maximum_useful_scratch` |
| `"compressed Page rejects a representation without savings"` | `compressed_page_rejects_a_representation_without_savings` |
| `"compressed Page can retry after malformed encoded data"` | `compressed_page_can_retry_after_malformed_encoded_data` |

The round-trip test is the load-bearing one; write it exactly:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::{Capacity, Page};
    use crate::style::Style;

    /// PORTS: oracle@main compress/Page.zig
    /// `test "compressed Page retained mapping round trip"`
    #[test]
    fn compressed_page_retained_mapping_round_trip() {
        let mut resident = Page::init(Capacity {
            cols: 12,
            rows: 9,
            styles: 8,
            grapheme_bytes: 128,
            string_bytes: 128,
            hyperlink_bytes: crate::page::std_capacity().hyperlink_bytes,
        })
        .unwrap();
        resident.size = crate::page::PageSize { cols: 10, rows: 7 };
        resident.dirty = true;

        // Put state in both the backing memory and the Page value. In
        // particular, the style and hyperlink sets retain live counters
        // outside Page.memory.
        let rac = resident.get_row_and_cell(2, 3);
        unsafe {
            *rac.cell = crate::page::Cell::init('A');
            resident.append_grapheme(rac.row, rac.cell, 0x0301).unwrap();

            let style_id = resident
                .styles
                .add(resident.memory_base(), Style { flags: crate::style::Flags { bold: true, ..Default::default() }, ..Default::default() })
                .unwrap();
            (*rac.cell).style_id = style_id;
            (*rac.row).styled = true;

            let hyperlink_id = resident
                .insert_hyperlink(
                    crate::hyperlink::Id::Explicit(b"compressed-page".to_vec()),
                    b"https://rustty.org/docs",
                )
                .unwrap();
            resident.set_hyperlink(rac.row, rac.cell, hyperlink_id).unwrap();
        }
        resident.verify_integrity();

        let expected: Vec<u8> =
            unsafe { core::slice::from_raw_parts(resident.memory_base(), resident.memory_len()) }
                .to_vec();
        let memory_ptr = resident.memory_base();
        let memory_len = resident.memory_len();

        let mut scratch = vec![0u8; required_scratch(memory_len).unwrap()];
        let mut table: lz4::HashTable = [0; 4096];
        let compressed = unsafe {
            CompressedPage::init(&resident as *const Page, &mut scratch, &mut table)
        }
        .unwrap()
        .unwrap();

        assert_eq!(compressed.page.memory_base(), memory_ptr);
        assert_eq!(compressed.page.memory_len(), memory_len);
        assert!(compressed.encoded.len() < memory_len);
        let expected_encoded = compressed.encoded.clone();

        // Virtual-memory operations belong to PageList, so clearing the
        // contents models a successful decommit: none of the resident bytes
        // remain.
        unsafe { core::ptr::write_bytes(memory_ptr, 0, memory_len) };

        // A clone decodes into independent storage for read-only consumers
        // which must not change the compressed representation. In particular,
        // it does not recommit or overwrite the retained source mapping.
        let layout =
            std::alloc::Layout::from_size_align(memory_len, crate::page::PAGE_SIZE_MIN).unwrap();
        let clone_memory = unsafe { std::alloc::alloc_zeroed(layout) };
        let cloned = unsafe { compressed.clone_buf(clone_memory, memory_len) }.unwrap();
        assert_ne!(cloned.memory_base(), memory_ptr);
        assert!(unsafe { core::slice::from_raw_parts(memory_ptr, memory_len) }
            .iter()
            .all(|&b| b == 0));
        assert_eq!(compressed.encoded, expected_encoded);
        assert_eq!(
            unsafe { core::slice::from_raw_parts(cloned.memory_base(), memory_len) },
            &expected[..]
        );
        assert_eq!(cloned.size, resident.size);
        assert!(cloned.dirty);
        cloned.verify_integrity();
        drop(cloned);
        unsafe { std::alloc::dealloc(clone_memory, layout) };

        let restored = unsafe { compressed.restore() }.unwrap();
        assert_eq!(restored.memory_base(), memory_ptr);
        assert_eq!(restored.memory_len(), memory_len);
        assert_eq!(
            unsafe { core::slice::from_raw_parts(restored.memory_base(), memory_len) },
            &expected[..]
        );
        assert_eq!(restored.size, resident.size);
        assert!(restored.dirty);
        restored.verify_integrity();

        let restored_rac = restored.get_row_and_cell(2, 3);
        assert_eq!(
            unsafe { restored.lookup_grapheme(restored_rac.cell) }.unwrap(),
            &[0x0301u32][..]
        );
        assert!(unsafe {
            restored
                .styles
                .get(restored.memory_base(), (*restored_rac.cell).style_id)
        }
        .flags
        .bold);

        drop(restored);
        drop(compressed);
        resident.deinit();
    }
}
```

**Implementer note:** the exact accessor names (`get_row_and_cell`, `append_grapheme`, `insert_hyperlink`, `set_hyperlink`, `lookup_grapheme`, `verify_integrity`, `memory_base`, `memory_len`, `Cell::init`) must be confirmed against `crates/rustty-vt/src/page_impl.rs` and adjusted; they exist but may differ in arity. Do **not** simplify the test to avoid them — exercising styles/graphemes/hyperlinks is the whole point (it proves that metadata living outside `Page.memory` survives).

Port the other three oracle tests verbatim in the same style; the FailingAllocator in `"rejects a representation without savings"` becomes "assert `init` returns `Ok(None)` before any allocation happens" (rustty's allocator is infallible `Global`), and the assertion `!failing.has_induced_failure` becomes an assertion that `resident`'s bytes are unchanged.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib compressed_page_`
Expected: FAIL — `cannot find type CompressedPage in this scope`

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/rustty-vt/src/compress/page.rs`:

```rust
//! A compressed terminal page which retains its resident virtual mapping.
//!
//! Terminal pages have two kinds of state: the large `Page.memory` allocation
//! and a comparatively small `Page` value containing offsets, dimensions,
//! dirty state, and allocator metadata. Not all of the latter state lives in
//! the backing memory, so preserving only the memory bytes is insufficient.
//! We preserve the complete `Page` value instead. This follows the same model
//! as `Page::clone_buf`: page internals use offsets, so a shallow page copy
//! remains valid when its memory contents are restored at the same address.
//!
//! The resident memory is deliberately not freed by this type. `PageList`
//! keeps the virtual range allocated while asking the operating system to
//! discard its physical pages. Keeping the range has two useful properties:
//! the embedded page never contains a dangling pointer, and restoring the page
//! does not require a fallible allocation.
//!
//! The intended state transition is:
//!
//! 1. Create this value while the source page is resident.
//! 2. Ask the OS to decommit the source page's memory.
//! 3. Replace the PageList node's resident state with this value.
//! 4. To restore, recommit the retained range and call `restore`.
//! 5. After committing the resident state, drop this value to free the encoded
//!    bytes.
//!
//! If decommit is unavailable or fails, the caller should drop the compressed
//! candidate and leave the source page resident. This type does not perform any
//! virtual-memory operations itself.
//!
//! PORTS: ghostty src/terminal/compress/Page.zig (oracle origin/main).
//!
//! PORT NOTE (WS7): the oracle stores an `alloc: Allocator` so a node can free
//! its encoding without a back-reference to the PageList. rustty's `Vec<u8>`
//! carries its `Global` allocator, so that field is unnecessary and `deinit` is
//! just `Drop`.
//!
//! PORT NOTE (WS7): `init`/`restore`/`clone_buf` take RAW pointers, never
//! `&Page`/`&mut Page`. `PageList::compress_page` holds a `*mut Node` whose
//! `data` union it is about to overwrite; forming a reference to the embedded
//! page across that write is Stacked-Borrows UB in this port.

use super::lz4;
use crate::page::Page;

/// Complete page metadata together with the retained resident mapping.
pub struct CompressedPage {
    /// The bytes in `page`'s mapping may have been discarded by the OS while
    /// this value is in compressed state. They must not be read until `restore`
    /// has successfully decoded the page.
    pub page: Page,

    /// Exact raw LZ4 block for the page's mapping.
    pub encoded: Vec<u8>,
}

/// Return the largest scratch buffer that can produce a useful compressed
/// representation for `raw_len` bytes.
///
/// This is deliberately smaller than the general LZ4 compression bound. The
/// compressed representation includes an additional allocation compared to a
/// resident terminal page, so an encoded block which fills more than this
/// buffer cannot reduce resident memory. Limiting the output here lets a
/// PageList borrow a standard page-pool item as scratch instead of retaining a
/// larger, compression-bound allocation.
///
/// Callers are expected to reuse the scratch memory when practical. The
/// scratch buffer and hash table are never retained by this type.
pub fn required_scratch(raw_len: usize) -> Result<usize, lz4::CompressError> {
    // Validate the codec's input limit before doing arithmetic based on the
    // raw size. The bound itself is intentionally not returned; see above.
    let _ = lz4::compress_bound(raw_len)?;

    let representation_overhead =
        core::mem::size_of::<CompressedPage>() - core::mem::size_of::<Page>();
    if raw_len <= representation_overhead {
        return Ok(0);
    }

    // The savings comparison is strict, so reserve one fewer byte than the
    // break-even encoded size.
    Ok(raw_len - representation_overhead - 1)
}

impl CompressedPage {
    /// Create a compressed representation of `source`.
    ///
    /// `source` is not modified and continues to own its backing allocation.
    /// The scratch buffer must be at least `required_scratch(source.memory_len())`
    /// bytes. It must not overlap the source memory. On success, the returned
    /// value aliases the source's resident mapping and owns only its
    /// exact-sized `encoded` allocation.
    ///
    /// Returns `Ok(None)` if retaining the compressed representation would not
    /// use less memory than the resident representation. An `OutputTooSmall`
    /// result from the codec also means the encoding crossed that break-even
    /// point and is therefore returned as `Ok(None)`.
    ///
    /// # Safety
    /// `source` must point at a live `Page` whose mapping is resident and does
    /// not overlap `scratch`.
    pub unsafe fn init(
        source: *const Page,
        scratch: &mut [u8],
        table: &mut lz4::HashTable,
    ) -> Result<Option<CompressedPage>, lz4::CompressError> {
        let raw_ptr = (*source).memory_base();
        let raw_len = (*source).memory_len();

        let required = required_scratch(raw_len)?;
        if scratch.len() < required {
            return Err(lz4::CompressError::OutputTooSmall);
        }
        if required == 0 {
            return Ok(None);
        }

        let input = core::slice::from_raw_parts(raw_ptr, raw_len);
        let encoded_len = match lz4::compress(input, &mut scratch[..required], table) {
            Ok(n) => n,
            // The scratch limit is the largest representation worth retaining.
            // Running out of room therefore means compression cannot save
            // memory, rather than that the caller failed to provide the
            // documented size.
            Err(lz4::CompressError::OutputTooSmall) => return Ok(None),
            Err(e @ lz4::CompressError::InputTooLarge) => return Err(e),
        };

        // required_scratch already accounts for the representation structs. The
        // retained virtual range contributes no resident bytes after PageList
        // decommits it.
        debug_assert!(
            core::mem::size_of::<CompressedPage>() + encoded_len
                < core::mem::size_of::<Page>() + raw_len
        );

        Ok(Some(CompressedPage {
            page: (*source).shallow_copy(),
            encoded: scratch[..encoded_len].to_vec(),
        }))
    }

    /// Restore the embedded terminal page into its retained resident mapping.
    ///
    /// The caller must recommit the mapping before calling this on platforms
    /// which require an explicit recommit operation. The compressed value
    /// remains valid whether decoding succeeds or fails, allowing the caller to
    /// retry or discard it. On success the returned page aliases the same
    /// resident mapping; it does not own a new allocation.
    ///
    /// # Safety
    /// The retained mapping must be recommitted and unaliased.
    pub unsafe fn restore(&self) -> Result<Page, lz4::DecompressError> {
        let result = self.page.shallow_copy();
        let out = core::slice::from_raw_parts_mut(result.memory_base(), result.memory_len());
        lz4::decompress(&self.encoded, out)?;
        Ok(result)
    }

    /// Clone this page into caller-owned memory without restoring its mapping.
    ///
    /// `memory` must be at least as large as the retained page mapping.
    /// Decoding writes only to that buffer, so both the discarded contents of
    /// the retained mapping and this value's encoded representation remain
    /// unchanged. The returned Page borrows `memory`; the caller must keep it
    /// alive for the Page's lifetime and release it directly rather than
    /// calling `Page::deinit`.
    ///
    /// # Safety
    /// `memory` must be a live, page-aligned, unaliased allocation of at least
    /// `memory_len >= self.page.memory_len()` bytes.
    pub unsafe fn clone_buf(
        &self,
        memory: *mut u8,
        memory_len: usize,
    ) -> Result<Page, lz4::DecompressError> {
        debug_assert!(memory_len >= self.page.memory_len());

        // Page internals are offsets into the backing buffer, so all metadata
        // can be copied verbatim when paired with an equally laid-out mapping.
        let mut result = self.page.shallow_copy();
        result.set_memory(memory, self.page.memory_len());
        let out = core::slice::from_raw_parts_mut(memory, self.page.memory_len());
        lz4::decompress(&self.encoded, out)?;
        Ok(result)
    }
}
```

`set_memory` is a new tiny helper on `Page` (add it beside `shallow_copy` in `page_impl.rs`):

```rust
    /// Repoint this page's backing mapping. Only valid on an UNOWNED page whose
    /// contents are about to be rewritten (see `CompressedPage::clone_buf`).
    ///
    /// PORT NOTE (WS7): the oracle writes `result.memory = memory[0..len]`.
    pub(crate) fn set_memory(&mut self, ptr: *mut u8, len: usize) {
        debug_assert!(!self.owned);
        self.memory_ptr = ptr;
        self.memory_len = len;
    }
```

Add to `crates/rustty-vt/src/compress.rs`:

```rust
pub mod page;
pub use page::{required_scratch, CompressedPage};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib compressed_page_`
Expected: PASS — 4 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/compress/page.rs crates/rustty-vt/src/compress.rs crates/rustty-vt/src/page_impl.rs
git commit -m "vt/compress: compressed page representation retaining its mapping (WS7 B3)"
```

---

### Task B4: Explicit page ownership on pagelist nodes

**Files:**
- Modify: `crates/rustty-vt/src/pagelist/mod.rs` (`Node`, `create_page`/`init_pages` sites, `destroy_node_ext:1484-1512`)
- Modify: `crates/rustty-vt/src/pagelist/grow.rs`, `resize.rs`, `access.rs` (every node-construction site)

**Oracle:** `16e4b5e98` "terminal: track page ownership explicitly on pagelist nodes" (209 insertions / 72 deletions, `PageList.zig` only).

**Interfaces:**
- Produces: `pub enum Owned { Pool, Heap }` and `Node.owned: Owned` (no `Default` — every construction site must decide explicitly, exactly as the oracle requires).
- Consumed by: B6 (`destroy_node_ext` restore/free), B9 (`memory_stats`).

**Why this must land before the representation change:** today `destroy_node_ext` infers pool-vs-heap from `page_memory_len(node) <= std_size()`. That inference is **wrong** for compacted heap pages (a heap-owned page can be smaller than the standard size) and returning one to the pool corrupts it. The oracle's commit message says exactly this.

- [ ] **Step 1: Write the failing test**

Add to `crates/rustty-vt/src/pagelist/tests.rs`:

```rust
/// PORTS: oracle@main PageList.zig — every node records how its backing memory
/// was allocated. A heap-owned page smaller than the standard pool item must
/// still be freed to the heap, never returned to the pool.
#[test]
fn pagelist_node_ownership_is_explicit() {
    let mut s = PageList::init(80, 24, 1000).unwrap();
    // Every page allocated for a standard-capacity list is pool owned.
    let mut it = s.pages.first;
    while let Some(node) = ptr_opt(it) {
        assert_eq!(unsafe { (*node).owned }, Owned::Pool);
        it = unsafe { (*node).next_opt() };
    }
    s.deinit();
}

/// A non-standard (oversized) page is heap owned.
#[test]
fn pagelist_oversized_page_is_heap_owned() {
    // 4000 columns forces a layout larger than std_size().
    let mut s = PageList::init(4000, 24, 0).unwrap();
    let node = s.pages.first.unwrap();
    assert_eq!(unsafe { (*node).owned }, Owned::Heap);
    s.deinit();
}
```

(Use the existing helpers in `pagelist/tests.rs` for list iteration; `ptr_opt`/`next_opt` above are placeholders for whatever the file already uses — grep the top of `tests.rs` for the established pattern and reuse it verbatim.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib pagelist_node_ownership pagelist_oversized_page_is_heap_owned`
Expected: FAIL — `no field owned on type Node`

- [ ] **Step 3: Write minimal implementation**

In `crates/rustty-vt/src/pagelist/mod.rs`, extend `Node`:

```rust
/// How the backing memory of the embedded Page was allocated. Pool-owned
/// memory is always a full standard-size item from the memory pool,
/// regardless of the page layout size. Heap-owned memory is allocated
/// directly with the page allocator and is exactly `Page.memory_len()` bytes.
///
/// This must never be inferred from the memory length: heap-owned pages can be
/// smaller than the standard size (e.g. compacted pages), and returning one to
/// the pool would corrupt it.
///
/// This has no `Default` on purpose so that every construction site is forced
/// to make an explicit decision.
///
/// PORTS: oracle@main PageList.zig `Node.Owned` (16e4b5e98).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owned {
    Pool,
    Heap,
}

pub struct Node {
    pub(crate) prev: *mut Node,
    pub(crate) next: *mut Node,
    pub data: Page,
    pub serial: u64,
    pub owned: Owned,
}
```

Then:
1. `grep -rn 'Node {' crates/rustty-vt/src/pagelist/` and add `owned: Owned::Pool` or `owned: Owned::Heap` at every construction. The rule is: the site that called `pool.pages.create()` sets `Pool`; the site that called the direct page-allocator path (`alloc_non_standard_page` / `free_non_standard_page`) sets `Heap`.
2. Rewrite `destroy_node_ext` (`pagelist/mod.rs:1484`) to switch on `owned` instead of on the length:

```rust
    fn destroy_node_ext(pool: &mut MemoryPool, node: *mut Node, total_size: Option<&mut usize>) {
        let page: *mut Page = unsafe { core::ptr::addr_of_mut!((*node).data) };
        let owned = unsafe { (*node).owned };
        let (ptr, len) = Self::page_memory(page);

        // Update our accounting for page size. This must mirror what was added
        // at creation time: a pool-owned page always accounts for a full pool
        // item even if its layout is smaller, while a heap-owned page accounts
        // for its exact memory length.
        if let Some(v) = total_size {
            *v -= match owned {
                Owned::Pool => std_size(),
                Owned::Heap => len,
            };
        }

        match owned {
            Owned::Pool => {
                debug_assert!(len <= std_size());
                // Reset the memory to zero (and decommit it, where supported)
                // so it can be reused.
                //
                // SAFETY: `ptr` is the pool item base; nothing aliases it here.
                unsafe {
                    let _ = crate::mem::decommit(
                        crate::mem::DecommitMode::Zero,
                        ptr,
                        std_size(),
                        len,
                    );
                    pool.pages.destroy(NonNull::new(ptr).unwrap());
                }
            }
            Owned::Heap => Self::free_non_standard_page(node),
        }

        unsafe { pool.nodes.destroy(NonNull::new(node).unwrap()) };
    }
```

3. Audit every `page_size` accounting site (`self.page_size += ...`) so a pool page always adds `std_size()`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustty-vt --lib pagelist`
Expected: PASS — the 2 new tests plus the full existing pagelist suite unchanged. Any pre-existing test that regressed here is an accounting mismatch introduced by step 3; fix the accounting, not the test.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/pagelist/
git commit -m "vt/pagelist: track page ownership explicitly on nodes (WS7 B4, oracle 16e4b5e98)"
```

---

### Task B5: Node access indirection (mechanical retarget)

**Files:**
- Modify: `crates/rustty-vt/src/pagelist/mod.rs` (add accessors)
- Modify: every file with a `(*node).data` / `node.data` access — 612 non-test sites across
  `crates/rustty-vt/src/pagelist/{access,erase,grow,iter,mod,pin,resize,scroll}.rs`,
  `crates/rustty-vt/src/{screen.rs,terminal.rs,render.rs,selection.rs,formatter.rs}`,
  `crates/rustty-vt/src/search/*.rs`,
  plus `crates/rustty-vt-c/src/{grid_ref.rs,grid_ref_tracked.rs}` and `crates/rustty-inspector/src/widgets/{page,pagelist}.rs`.

**Interfaces:**
- Produces (all on `Node`, all raw-self per the port's aliasing discipline):

```rust
impl Node {
    /// Return the terminal page stored in this node.
    ///
    /// WARNING: this will DECOMPRESS compressed pages! Only use it if you need
    /// access to the underlying memory. If you only need metadata (row/col
    /// counts etc.) use `metadata`/`rows`/`cols`/`capacity`.
    pub unsafe fn page(node: *mut Node) -> *mut Page;

    /// Return the terminal page only when its raw memory is resident, else null.
    pub unsafe fn page_if_resident(node: *mut Node) -> *mut Page;

    /// Return a page which the caller KNOWS is already resident. This avoids
    /// the representation check in hot paths which already hold live pointers
    /// into the page mapping.
    pub unsafe fn page_assume_resident(node: *mut Node) -> *mut Page;

    /// Return the embedded Page metadata without restoring its memory.
    pub unsafe fn metadata(node: *const Node) -> *const Page;

    pub unsafe fn rows(node: *const Node) -> size::CellCountInt;
    pub unsafe fn cols(node: *const Node) -> size::CellCountInt;
    pub unsafe fn capacity(node: *const Node) -> Capacity;
}
```

- Consumed by: B6, which changes only the **bodies** of these seven functions.

**Why a separate task:** B6 flips `Node.data` from `Page` to an enum. If the 612 call sites and the representation change land together, a reviewer cannot separate a mechanical rename from a semantic change, and a single missed `page_assume_resident`-vs-`page` decision becomes a silent decompression storm. This task is pure refactor: **no behavior change, all existing tests must pass unchanged.**

- [ ] **Step 1: Write the failing test**

No new test. The gate for this task is that the **existing** suite passes with zero diff in results:

```bash
cargo test -p rustty-vt --lib 2>&1 | tail -3 > /tmp/ws7-b5-before.txt
```
Record the baseline (`N passed; 0 failed`) before touching anything.

- [ ] **Step 2: Add the accessors (still over a plain `Page` field)**

In `crates/rustty-vt/src/pagelist/mod.rs`, in an `impl Node` block:

```rust
impl Node {
    /// PORTS: oracle@main PageList.zig `Node.page`.
    ///
    /// PORT NOTE (WS7 B5): this returns the resident page directly while
    /// `data` is still a plain `Page`. WS7 B6 changes ONLY this body.
    #[inline]
    pub unsafe fn page(node: *mut Node) -> *mut Page {
        core::ptr::addr_of_mut!((*node).data)
    }

    /// PORTS: oracle@main PageList.zig `Node.pageIfResident`.
    #[inline]
    pub unsafe fn page_if_resident(node: *mut Node) -> *mut Page {
        core::ptr::addr_of_mut!((*node).data)
    }

    /// PORTS: oracle@main PageList.zig `Node.pageAssumeResident`.
    #[inline]
    pub unsafe fn page_assume_resident(node: *mut Node) -> *mut Page {
        core::ptr::addr_of_mut!((*node).data)
    }

    /// PORTS: oracle@main PageList.zig `Node.metadata`.
    #[inline]
    pub unsafe fn metadata(node: *const Node) -> *const Page {
        core::ptr::addr_of!((*node).data)
    }

    /// PORTS: oracle@main PageList.zig `Node.rows`.
    #[inline]
    pub unsafe fn rows(node: *const Node) -> size::CellCountInt {
        (*Node::metadata(node)).size.rows
    }

    /// PORTS: oracle@main PageList.zig `Node.cols`.
    #[inline]
    pub unsafe fn cols(node: *const Node) -> size::CellCountInt {
        (*Node::metadata(node)).size.cols
    }

    /// PORTS: oracle@main PageList.zig `Node.capacity`.
    #[inline]
    pub unsafe fn capacity(node: *const Node) -> Capacity {
        (*Node::metadata(node)).capacity
    }
}
```

- [ ] **Step 3: Retarget every call site**

Make `data` private to the module so the compiler enumerates the work:

```rust
pub struct Node {
    pub(crate) prev: *mut Node,
    pub(crate) next: *mut Node,
    pub(crate) data: Page,   // was `pub data`
    pub serial: u64,
    pub owned: Owned,
}
```

Then `cargo check --workspace --all-targets` and fix every error, choosing per site using **exactly** the oracle's choice at the corresponding line in `oracle@main:src/terminal/PageList.zig` / `Screen.zig` / `Terminal.zig`:

| Call-site shape | Replacement |
|---|---|
| Reads/writes page **contents** (cells, rows, styles, graphemes, memory) | `Node::page(node)` |
| Already holds a live pointer INTO the page mapping (hot loops in `iter.rs`, `render.rs`, `screen.rs` row walks) | `Node::page_assume_resident(node)` |
| Only reads `size.rows` / `size.cols` / `capacity` | `Node::rows(node)` / `Node::cols(node)` / `Node::capacity(node)` |
| Diagnostics that may skip a non-resident page | `Node::page_if_resident(node)` |

**Decision rule when the oracle is ambiguous:** default to `Node::page`. `page_assume_resident` is an optimization that is only correct where the caller has independently established residency; using it wrongly is UB after B6. The oracle uses it in exactly the places `421fe8dab` introduced it — diff that commit for the list.

- [ ] **Step 4: Run tests to verify no behavior changed**

```bash
cargo test -p rustty-vt --lib 2>&1 | tail -3
cargo test -p rustty-vt-c --lib 2>&1 | tail -3
cargo check --workspace --all-targets
cargo build --workspace
```
Expected: the `rustty-vt` count is **identical** to `/tmp/ws7-b5-before.txt`. Not "similar" — identical.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "vt/pagelist: route page access through Node accessors (WS7 B5, no behavior change)"
```

---

### Task B6: Two-state node storage + restore + `PreservedPage`

**Files:**
- Modify: `crates/rustty-vt/src/pagelist/mod.rs`

**Oracle:** `421fe8dab` "terminal: integrate compressed pages into PageList" — specifically the `Node` block in `oracle@main:src/terminal/PageList.zig:46-292`.

**Interfaces:**
- Produces:
  - `pub enum NodeData { Resident(Page), Compressed(CompressedPage) }` (the new type of `Node.data`)
  - `pub enum Storage { Resident, Compressed }`
  - `pub unsafe fn Node::storage(node: *const Node) -> Storage`
  - `pub unsafe fn Node::is_compressed(node: *const Node) -> bool`
  - `pub enum PreservedPage { Borrowed(*const Page), Owned { page: Page, ptr: *mut u8, len: usize } }` with `pub fn page(&self) -> *const Page` and `pub fn deinit(&mut self)`
  - `pub unsafe fn Node::page_preserving_state(node: *const Node) -> PreservedPage`
  - private `enum RestoreMode { Preserve, Discard }` and `unsafe fn Node::restore(node: *mut Node, mode: RestoreMode) -> *mut Page`
- Consumed by: B7–B9, C, D.

- [ ] **Step 1: Write the failing test**

Add to `crates/rustty-vt/src/pagelist/tests.rs`:

```rust
/// A node built from a compressed representation restores to the SAME mapping
/// and yields identical bytes; `page()` transparently restores.
#[test]
fn pagelist_node_restore_preserve_recovers_the_same_mapping() {
    let mut s = PageList::init(20, 5, 1000).unwrap();
    let node = s.pages.first.unwrap();
    unsafe {
        let page = Node::page(node);
        let ptr = (*page).memory_base();
        let len = (*page).memory_len();
        let expected: Vec<u8> = core::slice::from_raw_parts(ptr, len).to_vec();

        let mut scratch = vec![0u8; crate::compress::required_scratch(len).unwrap()];
        let mut table: crate::compress::lz4::HashTable = [0; 4096];
        let compressed =
            crate::compress::CompressedPage::init(page as *const Page, &mut scratch, &mut table)
                .unwrap()
                .unwrap();

        // Model a successful decommit, then publish the compressed state.
        core::ptr::write_bytes(ptr, 0, len);
        (*node).data = NodeData::Compressed(compressed);
        assert_eq!(Node::storage(node), Storage::Compressed);
        assert!(Node::page_if_resident(node).is_null());

        // `page()` restores transparently.
        let restored = Node::page(node);
        assert_eq!(Node::storage(node), Storage::Resident);
        assert_eq!((*restored).memory_base(), ptr);
        assert_eq!(core::slice::from_raw_parts(ptr, len), &expected[..]);
    }
    s.deinit();
}

/// `page_preserving_state` reads a compressed node WITHOUT restoring it.
#[test]
fn pagelist_page_preserving_state_leaves_a_compressed_node_compressed() {
    let mut s = PageList::init(20, 5, 1000).unwrap();
    let node = s.pages.first.unwrap();
    unsafe {
        let page = Node::page(node);
        let ptr = (*page).memory_base();
        let len = (*page).memory_len();
        let expected: Vec<u8> = core::slice::from_raw_parts(ptr, len).to_vec();

        let mut scratch = vec![0u8; crate::compress::required_scratch(len).unwrap()];
        let mut table: crate::compress::lz4::HashTable = [0; 4096];
        let compressed =
            crate::compress::CompressedPage::init(page as *const Page, &mut scratch, &mut table)
                .unwrap()
                .unwrap();
        core::ptr::write_bytes(ptr, 0, len);
        (*node).data = NodeData::Compressed(compressed);

        let mut preserved = Node::page_preserving_state(node);
        let p = preserved.page();
        assert_ne!((*p).memory_base(), ptr);
        assert_eq!(
            core::slice::from_raw_parts((*p).memory_base(), len),
            &expected[..]
        );
        preserved.deinit();

        // Still compressed, and the discarded mapping is untouched.
        assert_eq!(Node::storage(node), Storage::Compressed);
        assert!(core::slice::from_raw_parts(ptr, len).iter().all(|&b| b == 0));

        // Restore so deinit's accounting sees a resident page.
        let _ = Node::page(node);
    }
    s.deinit();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib pagelist_node_restore_preserve pagelist_page_preserving_state`
Expected: FAIL — `cannot find type NodeData in this scope`

- [ ] **Step 3: Write minimal implementation**

Replace `Node.data`'s type and add the new bodies in `crates/rustty-vt/src/pagelist/mod.rs`:

```rust
/// The physical representation of the page contents.
///
/// Both states retain the same page mapping and the same `Page` metadata. A
/// resident page can access the mapping directly since it is, well, resident.
///
/// A compressed page owns an encoded copy while the physical memory behind the
/// raw mapping is discarded. HOWEVER, we retain the virtual allocation so that
/// restoration is infallible.
///
/// PORTS: oracle@main PageList.zig `Node.Data`.
pub enum NodeData {
    Resident(Page),
    Compressed(crate::compress::CompressedPage),
}

/// The backing-memory representation currently stored by a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Storage {
    Resident,
    Compressed,
}

/// Read-only page with full memory access that doesn't change a node's storage
/// state: if a node is compressed it remains compressed.
///
/// Resident pages are borrowed directly, so they're basically free. Compressed
/// pages are decoded into an independently owned Page so callers can inspect
/// their contents. Because resident pages are still borrowed, you have to
/// continue to have exclusive access to the PageList during this operation.
///
/// PORTS: oracle@main PageList.zig `Node.PreservedPage`.
pub enum PreservedPage {
    Borrowed(*const Page),
    Owned {
        page: Page,
        ptr: *mut u8,
        len: usize,
    },
}

impl PreservedPage {
    /// Return the read-only page represented by this value.
    #[inline]
    pub fn page(&self) -> *const Page {
        match self {
            PreservedPage::Borrowed(p) => *p,
            PreservedPage::Owned { page, .. } => page as *const Page,
        }
    }

    /// Release storage owned by this preserved page.
    pub fn deinit(&mut self) {
        if let PreservedPage::Owned { ptr, len, .. } = *self {
            // The clone buffer came from the global allocator rather than the
            // page allocator, so it must not use Page::deinit.
            let layout = Layout::from_size_align(len, PAGE_SIZE_MIN).unwrap();
            unsafe { std::alloc::dealloc(ptr, layout) };
        }
        *self = PreservedPage::Borrowed(core::ptr::null());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestoreMode {
    /// Decode the compressed representation back into the raw mapping.
    Preserve,
    /// Discard the compressed representation without decoding it. The caller
    /// must not read the page contents before overwriting them.
    Discard,
}

impl Node {
    #[inline]
    pub unsafe fn page(node: *mut Node) -> *mut Page {
        match &mut *core::ptr::addr_of_mut!((*node).data) {
            NodeData::Resident(p) => p as *mut Page,
            NodeData::Compressed(_) => Node::restore(node, RestoreMode::Preserve),
        }
    }

    #[inline]
    pub unsafe fn page_if_resident(node: *mut Node) -> *mut Page {
        match &mut *core::ptr::addr_of_mut!((*node).data) {
            NodeData::Resident(p) => p as *mut Page,
            NodeData::Compressed(_) => core::ptr::null_mut(),
        }
    }

    /// Return a page which the caller knows is already resident.
    ///
    /// This avoids the representation check in hot paths which already hold
    /// live pointers into the page mapping. Such pointers are only valid while
    /// the page is resident. Prefer `page` unless the caller can establish that
    /// invariant independently.
    #[inline]
    pub unsafe fn page_assume_resident(node: *mut Node) -> *mut Page {
        match &mut *core::ptr::addr_of_mut!((*node).data) {
            NodeData::Resident(p) => p as *mut Page,
            NodeData::Compressed(_) => {
                debug_assert!(false, "page_assume_resident on a compressed node");
                core::hint::unreachable_unchecked()
            }
        }
    }

    #[inline]
    pub unsafe fn metadata(node: *const Node) -> *const Page {
        match &*core::ptr::addr_of!((*node).data) {
            NodeData::Resident(p) => p as *const Page,
            NodeData::Compressed(c) => &c.page as *const Page,
        }
    }

    #[inline]
    pub unsafe fn storage(node: *const Node) -> Storage {
        match &*core::ptr::addr_of!((*node).data) {
            NodeData::Resident(_) => Storage::Resident,
            NodeData::Compressed(_) => Storage::Compressed,
        }
    }

    #[inline]
    pub unsafe fn is_compressed(node: *const Node) -> bool {
        matches!(Node::storage(node), Storage::Compressed)
    }

    /// Return read-only page contents without changing this node's storage.
    ///
    /// A compressed node decodes into an exact-sized, page-aligned buffer owned
    /// by the returned value. The caller must call `PreservedPage::deinit`.
    pub unsafe fn page_preserving_state(node: *const Node) -> PreservedPage {
        match &*core::ptr::addr_of!((*node).data) {
            NodeData::Resident(p) => PreservedPage::Borrowed(p as *const Page),
            NodeData::Compressed(compressed) => {
                let len = compressed.page.memory_len();
                let layout = Layout::from_size_align(len, PAGE_SIZE_MIN).unwrap();
                let ptr = std::alloc::alloc_zeroed(layout);
                assert!(!ptr.is_null(), "OOM allocating a preserved page buffer");

                match compressed.clone_buf(ptr, len) {
                    Ok(page) => PreservedPage::Owned { page, ptr, len },
                    Err(_err) => {
                        // The encoded data was produced by our codec and remains
                        // immutable while compressed. Failure is internal
                        // corruption, matching the normal restoration boundary.
                        std::alloc::dealloc(ptr, layout);
                        // log.err("failed to clone compressed page err={}", err);
                        panic!("failed to clone compressed terminal page");
                    }
                }
            }
        }
    }

    /// Restore this node to the resident representation.
    ///
    /// Preserve mode reconstructs the page contents and is used by `page`.
    /// Discard mode skips decoding for callers which will overwrite or destroy
    /// the page. Both modes recommit the retained mapping, free the encoded
    /// allocation, and leave the node in a valid resident state.
    ///
    /// PORTS: oracle@main PageList.zig `Node.restore` (`noinline`).
    #[inline(never)]
    unsafe fn restore(node: *mut Node, mode: RestoreMode) -> *mut Page {
        let data = core::ptr::addr_of_mut!((*node).data);
        let compressed: &crate::compress::CompressedPage = match &*data {
            NodeData::Resident(_) => return Node::page_assume_resident(node),
            NodeData::Compressed(c) => c,
        };

        // Decommit only discarded the physical pages. Recommit prepares the
        // still-reserved mapping for decoding or reuse by the caller.
        crate::mem::recommit(compressed.page.memory_base(), compressed.page.memory_len());

        let restored: Page = match mode {
            RestoreMode::Preserve => match compressed.restore() {
                Ok(p) => p,
                Err(_err) => {
                    // Compressed nodes retain the immutable encoding and exact
                    // output mapping used to create them. Any decode error is an
                    // internal codec bug or memory corruption.
                    // log.err("failed to restore compressed page err={}", err);
                    panic!("failed to restore compressed terminal page");
                }
            },
            RestoreMode::Discard => compressed.page.shallow_copy(),
        };

        // Replacing `data` drops the CompressedPage, freeing its encoding.
        core::ptr::write(data, NodeData::Resident(restored));
        Node::page_assume_resident(node)
    }
}
```

Also update `destroy_node_ext` from B4 to begin with `let page = Node::restore(node, RestoreMode::Discard);` (the oracle's `const page = node.restore(.discard);`) and take `ptr`/`len` from that, and update every `Node { .. }` construction to `data: NodeData::Resident(page)`.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rustty-vt --lib
cargo build --workspace
```
Expected: PASS, count = B5 baseline + 2.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/pagelist/
git commit -m "vt/pagelist: two-state node storage with retained-mapping restore (WS7 B6, oracle 421fe8dab)"
```

---

### Task B7: `compress_page` primitive + full pass

**Files:**
- Create: `crates/rustty-vt/src/pagelist/compress.rs`
- Modify: `crates/rustty-vt/src/pagelist/mod.rs` (`pub mod compress;`)

**Oracle:** `9156ada16` "terminal: add cold-history compression pass" plus the final `CompressionScratch`, `CompressionIterator`, `compress_page`, `compress_full` sections of `oracle@main:src/terminal/PageList.zig:3940-4300`.

**Interfaces:**
- Produces:
  - `enum CompressionScratch { Pooled(*mut u8), Allocated { ptr: *mut u8, len: usize } }` with `init(pool, required, raw_len)`, `bytes(&mut self) -> &mut [u8]`, `deinit(self, pool)`
  - `struct CompressionIterator { current, active, viewport_first, viewport_last }` with `init(&PageList)`, `next() -> Option<*mut Node>`, `done() -> bool`
  - `impl PageList { fn compress_page(&mut self, node: *mut Node) -> bool; fn compress_full(&mut self); }`
- Consumed by: B8.

- [ ] **Step 1: Write the failing test**

Add to `crates/rustty-vt/src/pagelist/tests.rs`:

```rust
/// A cold history page compresses, its mapping is discarded, and reading it
/// back restores identical bytes.
#[test]
fn pagelist_compress_page_round_trips_a_cold_history_page() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    // Fill enough compressible history to create at least three pages.
    fill_compressible_history(&mut s, 3);

    let node = s.pages.first.unwrap();
    let (ptr, len, expected) = unsafe {
        let p = Node::page(node);
        let ptr = (*p).memory_base();
        let len = (*p).memory_len();
        (ptr, len, core::slice::from_raw_parts(ptr, len).to_vec())
    };

    assert!(s.compress_page(node));
    assert_eq!(unsafe { Node::storage(node) }, Storage::Compressed);
    // The test build of `mem::decommit` zeroes instead of calling madvise.
    assert!(unsafe { core::slice::from_raw_parts(ptr, len) }.iter().all(|&b| b == 0));

    let restored = unsafe { Node::page(node) };
    assert_eq!(unsafe { (*restored).memory_base() }, ptr);
    assert_eq!(unsafe { core::slice::from_raw_parts(ptr, len) }, &expected[..]);
    s.deinit();
}

/// Compressing twice is a no-op: `compress_page` only accepts resident nodes.
#[test]
fn pagelist_compress_page_rejects_an_already_compressed_node() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    fill_compressible_history(&mut s, 3);
    let node = s.pages.first.unwrap();
    assert!(s.compress_page(node));
    assert!(!s.compress_page(node));
    let _ = unsafe { Node::page(node) };
    s.deinit();
}

/// A full pass compresses every complete offscreen history page and leaves the
/// viewport and active pages resident.
#[test]
fn pagelist_compress_full_leaves_viewport_and_active_resident() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    fill_compressible_history(&mut s, 4);

    s.compress_full();

    let active = s.get_top_left(Point::Active).node;
    let viewport_first = s.get_top_left(Point::Viewport).node;
    let viewport_last = s.get_bottom_right(Point::Viewport).unwrap().node;
    assert_eq!(unsafe { Node::storage(active) }, Storage::Resident);
    assert_eq!(unsafe { Node::storage(viewport_first) }, Storage::Resident);
    assert_eq!(unsafe { Node::storage(viewport_last) }, Storage::Resident);
    assert_eq!(unsafe { Node::storage(s.pages.first.unwrap()) }, Storage::Compressed);
    s.deinit();
}
```

Add the shared helper next to the other test helpers in `tests.rs`:

```rust
/// Write highly compressible rows until the list holds at least `pages` pages.
fn fill_compressible_history(s: &mut PageList, pages: usize) {
    let mut row = 0usize;
    while s.total_pages() < pages {
        let p = s.grow().unwrap_or(core::ptr::null_mut());
        if p.is_null() {
            // grow() returns null when it reused an existing page; keep going.
        }
        row += 1;
        assert!(row < 100_000, "history did not grow to {pages} pages");
    }
}
```

(If `grow()` has a different signature in `pagelist/grow.rs`, adapt — the requirement is only "produce ≥ `pages` pages of compressible content".)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib pagelist_compress_page pagelist_compress_full`
Expected: FAIL — `no method named compress_page found for struct PageList`

- [ ] **Step 3: Write minimal implementation**

Create `crates/rustty-vt/src/pagelist/compress.rs` porting, in order:
1. `CompressionScratch` — `oracle@main:PageList.zig:3940-3990`. Pooled scratch borrows a page-pool item (`pool.pages.create()` + `mem::recommit`); oversized scratch allocates directly from the page-backing allocator. `deinit` decommits (`DecommitMode::Zero`) the pooled item before returning it.
2. `CompressionIterator` — `oracle@main:PageList.zig:4066-4110`. Iterates complete historical pages that do not intersect the viewport, never restoring anything (it uses only `Node::metadata`-level information and the list links).
3. `compress_page` — `oracle@main:PageList.zig:4290-4370`. The exact order is load-bearing and must be reproduced:
   - reject already-compressed nodes;
   - `let required = compress::required_scratch(len)`; `required == 0` ⇒ ineligible;
   - build the candidate inside a scope so the scratch is released **before** the decommit attempt;
   - `CompressedPage::init(...)` returning `Ok(None)` ⇒ break-even, node untouched;
   - **strict decommit is the final fallible step**; on failure drop the candidate and leave the node exactly as it was;
   - publish `node.data = NodeData::Compressed(compressed)` only after both succeeded.
4. `compress_full` — `oracle@main:PageList.zig:4275-4288`.

Signature skeleton (fill the bodies from the oracle):

```rust
//! PageList page-compression policy.
//!
//! PORTS: the compression sections of ghostty src/terminal/PageList.zig
//! (oracle origin/main; commits 9156ada16, 70e788f06, e7969ed43, f8c217e55,
//! 181254d36, 95685afd2).

use super::*;
use crate::compress;
use crate::mem;

/// Temporary output memory used while creating a compressed page.
///
/// Standard-sized output borrows a page-pool item so repeated compression can
/// reuse the same virtual mapping. Oversized pages use a temporary allocation
/// from the page allocator and release it immediately after compression.
pub(crate) enum CompressionScratch {
    Pooled(*mut u8),
    Allocated { ptr: *mut u8, len: usize },
}

impl CompressionScratch {
    pub(crate) fn init(
        pool: &mut MemoryPool,
        required: usize,
        raw_len: usize,
    ) -> Result<CompressionScratch, OutOfMemory> { /* oracle PageList.zig:3949-3968 */ }

    pub(crate) fn bytes(&mut self) -> &mut [u8] { /* oracle :3970-3975 */ }

    pub(crate) fn deinit(self, pool: &mut MemoryPool) { /* oracle :3977-3988 */ }
}

/// Iterate complete historical pages which do not intersect the viewport.
///
/// All boundaries come from PageList pins and Page metadata. Advancing this
/// iterator never restores a compressed page or reads its backing memory.
pub(crate) struct CompressionIterator {
    current: *mut Node,
    active: *mut Node,
    viewport_first: *mut Node,
    viewport_last: *mut Node,
}

impl CompressionIterator {
    pub(crate) fn init(list: &PageList) -> CompressionIterator { /* oracle :4076-4086 */ }
    pub(crate) fn next(&mut self) -> Option<*mut Node> { /* oracle :4088-4110 */ }
    pub(crate) fn done(&self) -> bool { self.current == self.active }
}

impl PageList {
    /// Attempt to compress one resident page while retaining its raw mapping.
    ///
    /// Compression is opportunistic: every failure leaves the page resident and
    /// usable. Candidate selection and retry policy belong to `compress`; this
    /// primitive only performs one state transition.
    pub(crate) fn compress_page(&mut self, node: *mut Node) -> bool { /* oracle :4290-4370 */ }

    /// Compress every fully historical resident page which is currently cold.
    pub(crate) fn compress_full(&mut self) { /* oracle :4275-4288 */ }
}
```

**Aliasing requirement (binding):** inside `compress_page`, obtain the page as `let page: *mut Page = Node::page(node);` and pass `page as *const Page` to `CompressedPage::init`. Do **not** bind `let page = &*Node::page(node);` — that shared reference would still be live when `node.data` is overwritten at the end of the function, which Miri's Stacked Borrows rejects. This exact shape is what the Phase-E Miri harness proves.

Add `pub mod compress;` to the cluster list in `crates/rustty-vt/src/pagelist/mod.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustty-vt --lib pagelist_compress`
Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/pagelist/compress.rs crates/rustty-vt/src/pagelist/mod.rs crates/rustty-vt/src/pagelist/tests.rs
git commit -m "vt/pagelist: cold-history compression pass and compressPage primitive (WS7 B7)"
```

---

### Task B8: Incremental compression scheduling

**Files:**
- Modify: `crates/rustty-vt/src/pagelist/compress.rs`
- Modify: `crates/rustty-vt/src/pagelist/mod.rs` (new `page_compression` field; `mark_activity` calls)
- Modify: `crates/rustty-vt/src/pagelist/{grow,resize,scroll,erase}.rs` (mutation sites that call `mark_activity` / `reset`)

**Oracle:** the final `IncrementalCompressionState`, `IncrementalCompressionResult`, `compress`, `compress_incremental` in `oracle@main:src/terminal/PageList.zig:3991-4275`, plus the `page_compression.markActivity()` / `page_compression.reset()` call sites at `:903-907, :1228-1232, :2833, :3236, :3312, :3533, :3840`.

**Interfaces:**
- Produces:
  - `pub struct IncrementalCompressionState { flags: { did_compress, verifying }, activity_serial: u64 /* 48 bits used */, last_serial: Option<u64>, next_serial: u64 }` with `mark_activity()` and `reset()`
  - `pub enum CompressionMode { Incremental, Drain, Full }`
  - `pub enum IncrementalCompressionResult { Unsupported, Pending, Complete }`
  - `impl PageList { pub fn compress(&mut self, mode: CompressionMode) -> IncrementalCompressionResult }`
  - `const INCREMENTAL_COMPRESSION_MAX_INSPECTED: usize = 8;`
  - `PageList.page_compression: IncrementalCompressionState` (public field, read by `Terminal::compression_activity`)
- PORT NOTE: the oracle's `activity_serial: u48` reserves 16 bits for `Terminal` to add state. Rust has no `u48`; use `u64` and mask to 48 bits in `mark_activity` (`self.activity_serial = (self.activity_serial + 1) & 0xFFFF_FFFF_FFFF;`). `Terminal::compression_activity` composes its extra 16 bits in the high bits exactly as `oracle@main:Terminal.zig:2555-2560` does. Document the mask.

- [ ] **Step 1: Write the failing test**

Add to `crates/rustty-vt/src/pagelist/tests.rs`:

```rust
/// Incremental compression converges: repeated steps eventually report
/// `Complete`, having compressed the cold history.
#[test]
fn pagelist_compress_incremental_converges_to_complete() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    fill_compressible_history(&mut s, 6);

    let mut steps = 0usize;
    loop {
        match s.compress(CompressionMode::Incremental) {
            IncrementalCompressionResult::Pending => {
                steps += 1;
                assert!(steps < 10_000, "incremental compression did not converge");
            }
            IncrementalCompressionResult::Complete => break,
            IncrementalCompressionResult::Unsupported => {
                panic!("test builds always support strict reclamation")
            }
        }
    }
    assert_eq!(unsafe { Node::storage(s.pages.first.unwrap()) }, Storage::Compressed);
    s.deinit();
}

/// `Drain` performs incremental steps until the pass and its verification pass
/// finish, in one call.
#[test]
fn pagelist_compress_drain_finishes_in_one_call() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    fill_compressible_history(&mut s, 6);
    assert_eq!(
        s.compress(CompressionMode::Drain),
        IncrementalCompressionResult::Complete
    );
    assert_eq!(unsafe { Node::storage(s.pages.first.unwrap()) }, Storage::Compressed);
    s.deinit();
}

/// PageList mutations bump the activity token so schedulers know to recompress.
#[test]
fn pagelist_mutations_mark_compression_activity() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    let before = s.page_compression.activity_serial;
    fill_compressible_history(&mut s, 2);
    assert_ne!(s.page_compression.activity_serial, before);

    // Compression itself changes representation, not the activity token.
    let after_growth = s.page_compression.activity_serial;
    let _ = s.compress(CompressionMode::Drain);
    assert_eq!(s.page_compression.activity_serial, after_growth);
    s.deinit();
}

/// A node destroyed between steps must not strand the traversal cursor: the
/// next step restarts safely rather than skipping newly allocated pages.
#[test]
fn pagelist_compress_incremental_restarts_after_list_mutation() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    fill_compressible_history(&mut s, 6);
    assert_eq!(
        s.compress(CompressionMode::Incremental),
        IncrementalCompressionResult::Pending
    );

    // Erase the history the cursor is standing on, then keep stepping.
    s.erase_history();
    fill_compressible_history(&mut s, 4);

    let mut steps = 0usize;
    while s.compress(CompressionMode::Incremental) == IncrementalCompressionResult::Pending {
        steps += 1;
        assert!(steps < 10_000);
    }
    s.deinit();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib pagelist_compress_incremental pagelist_compress_drain pagelist_mutations_mark`
Expected: FAIL — `cannot find type CompressionMode in this scope`

- [ ] **Step 3: Write minimal implementation**

Port, verbatim in structure, from `oracle@main:src/terminal/PageList.zig`:
- `IncrementalCompressionState` (`:3991-4051`) — including the doc comments explaining why the cursor is a **serial**, not a node pointer, and why `next_serial` exists.
- `IncrementalCompressionResult` (`:4053-4065`).
- `INCREMENTAL_COMPRESSION_MAX_INSPECTED = 8` (`:4177`).
- `PageList::compress(mode)` (`:4200-4240`) — `Incremental` / `Drain` / `Full` arms.
- `PageList::compress_incremental()` (`:4242-4273`) — continuation lookup, cap on inspected pages, verification pass, `reset` on completion.

Add the field to `PageList` in `mod.rs`:

```rust
    /// Continuation state for incremental page compression. This allows
    /// `compress(.incremental)` to work. More details on all that there and
    /// on `IncrementalCompressionState`.
    pub page_compression: IncrementalCompressionState,
```
initialized `IncrementalCompressionState::default()` in `init` and `clone`.

Then add the `mark_activity` / `reset` calls at every mutation site the oracle marks. Find them with:
```bash
git -C /Users/gael/projects/ghostty show origin/main:src/terminal/PageList.zig \
  | grep -n 'page_compression\.'
```
and map each Zig line to the corresponding rustty function by name (`reset` → `PageList::reset`, `resize` → `pagelist/resize.rs`, `eraseHistory` → `pagelist/erase.rs`, `grow`/`adjustCapacity`/`compact` → `pagelist/grow.rs`, `scrollClear` → `pagelist/scroll.rs`). **Every one of the 8 sites must be ported** — a missing `mark_activity` means the renderer stops scheduling compression after that operation, which no test in this suite would catch.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rustty-vt --lib pagelist
```
Expected: PASS — 4 new tests plus the full existing suite.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/pagelist/
git commit -m "vt/pagelist: incremental scrollback compression with activity debounce (WS7 B8)"
```

---

### Task B9: `memory_stats`

**Files:**
- Modify: `crates/rustty-vt/src/pagelist/compress.rs`

**Oracle:** `oracle@main:src/terminal/PageList.zig:6111-6180` (`MemoryStats`, `memoryStats`).

**Interfaces:**
- Produces:
  - `#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)] pub struct MemoryStats { pub resident_pages, pub compressed_pages, pub raw_bytes, pub resident_raw_bytes, pub decommitted_raw_bytes, pub resident_backing_bytes, pub encoded_bytes: usize }`
  - `impl MemoryStats { pub fn estimated_resident_bytes(&self) -> usize; pub fn estimated_savings(&self) -> usize }`
  - `impl PageList { pub fn memory_stats(&self) -> MemoryStats }`
- Consumed by: D6 (inspector), and the `0d015b2fc` search regression test in Phase C.

- [ ] **Step 1: Write the failing test**

```rust
/// PORTS: the memoryStats assertions in oracle 0d015b2fc / 6d5dda40d.
#[test]
fn pagelist_memory_stats_accounts_compressed_and_resident_bytes() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    fill_compressible_history(&mut s, 4);

    let before = s.memory_stats();
    assert_eq!(before.compressed_pages, 0);
    assert_eq!(before.encoded_bytes, 0);
    assert_eq!(before.decommitted_raw_bytes, 0);
    assert_eq!(before.resident_raw_bytes, before.raw_bytes);

    let _ = s.compress(CompressionMode::Drain);
    let after = s.memory_stats();
    assert!(after.compressed_pages >= 1);
    assert_eq!(after.resident_pages + after.compressed_pages, s.total_pages());
    assert_eq!(after.raw_bytes, before.raw_bytes);
    assert_eq!(
        after.resident_raw_bytes + after.decommitted_raw_bytes,
        after.raw_bytes
    );
    assert!(after.encoded_bytes > 0);
    // Compression only pays if the encoding is smaller than what it replaced.
    assert!(after.encoded_bytes < after.decommitted_raw_bytes);
    assert!(after.estimated_savings() > 0);
    assert!(after.estimated_resident_bytes() < before.estimated_resident_bytes());
    s.deinit();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib pagelist_memory_stats`
Expected: FAIL — `no method named memory_stats found`

- [ ] **Step 3: Write minimal implementation**

Port `MemoryStats` + `memory_stats` from the oracle lines above. Key detail to reproduce exactly (the oracle comments it): `resident_backing_bytes` uses `std_size()` for `Owned::Pool` and `raw_len` for `Owned::Heap`, and **can exceed** `resident_raw_bytes` because a pool-owned page uses only part of a standard item — compressing such a page decommits its initialized range but the unused tail of the item stays resident.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib pagelist_memory_stats`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/pagelist/compress.rs crates/rustty-vt/src/pagelist/tests.rs
git commit -m "vt/pagelist: metadata-only memory stats for compressed pages (WS7 B9)"
```

---

### Task B10: Phase B gate

**Files:** test only.

- [ ] **Step 1: Full vt suite**

```bash
cargo test -p rustty-vt --lib
cargo test -p rustty-vt --lib --release
```
Expected: PASS both. The release run matters: `std_size()` differs between the `cfg(test)` debug geometry and release, so the compression break-even math is exercised at both page sizes.

- [ ] **Step 2: Both-platform build**

```bash
cargo check --workspace --all-targets
cargo build --workspace
ssh rvm 'cd ~/projects/rustty && cargo check --workspace --all-targets && cargo build --workspace'
```
Expected: all `Finished`. The Linux leg is the one that exercises the `MADV_DONTNEED` arm of `mem::decommit`.

`--all-targets` is NOT optional and is the reason this step is spelled with
`cargo check` first: `cargo build --workspace` compiles neither examples nor
test targets, so a `Node.data` retarget (B5) verified only with `cargo build`
leaves `examples/` and `#[cfg(test)]` call sites uncompiled. That is exactly
how three residual sites (`rustty-vt/examples/rss_probe.rs`, and BOTH
`rustty-font-shaper` shaper test modules) shipped through the original B10.

- [ ] **Step 3: Linux runtime check of the reclamation arm**

```bash
ssh rvm 'cd ~/projects/rustty && cargo test -p rustty-vt --lib pagelist_compress --release'
```
Expected: PASS. (Still the `cfg(test)` simulated path — the real `madvise` arms are exercised by the D8 bench in Phase E.)

- [ ] **Step 4: Acyclic gate**

```bash
bash .port/tools/gates/check_acyclic.sh
```
Expected: PASS — `rustty-vt` gained no new crate edges (`compress` and `mem` are internal modules).

- [ ] **Step 5: Commit**

```bash
git commit --allow-empty -m "vt: phase B gate — compression integrated, both platforms green (WS7 B10)"
```

**PHASE B COMPLETE.** `PageList` can compress cold history, incrementally or in full, and restore it transparently.

---

# PHASE C — Page generation stamps + search integrity

**Requires Phase B** (`Node::storage`, `Node::page_preserving_state`, `page_compression`).

This is upstream PR **#13282** ("update page serial to be treated as a generation marker (correctness)", merge `53bd14fec`) plus the four search fixes from #13275 that depend on it. The problem it solves: a cached `(node pointer, serial)` pair could survive a page *replacement* — the pointer is reused from the pool and the serial is stale-but-plausible — so search results, selections, and flattened highlights could point at rows that no longer mean what they meant.

rustty already has `Node.serial` and `PageList.page_serial` / `page_serial_min` (ported at the pin). This phase (a) renames the floor to *epoch* with the oracle's exact semantics, (b) adds `renew_generation` at every layout-changing site, (c) adds `node_is_valid`, and (d) ports the seven search/render consumers.

**Oracle commits, in upstream order:**

| # | hash | subject | task |
|---|---|---|---|
| 1 | `d239f9805` | terminal/search: drop pruned selections | C6 |
| 2 | `5d8eb78b7` | terminal/search: reset pins while feeding | C6 |
| 3 | `86e1f7b8b` | terminal/search: reject replaced result pages | C6 |
| 4 | `73ac36fa5` | terminal/search: discard destroyed screen state | C6 |
| 5 | `f1a5fab45` | terminal: keep page serial floor conservative | C3 |
| 6 | `a804e3ac7` | terminal: invalidate partially erased page refs | C3 |
| 7 | `91a63e528` | terminal: invalidate split source page refs | C3 |
| 8 | `83aada205` | terminal/search: preserve serial order in reverse matches | C5 |
| 9 | `c243d89bd` | terminal/search: include generations in viewport fingerprints | C7 |
| 10 | `a89c6133c` | terminal/search: validate history before reload | C7 |
| 11 | `d0a26d146` | terminal: reschedule compression after page replacement | C4 |
| 12 | `dd956f37c` | terminal: renew generations for shifted rows | C2 |
| 13 | `f6d9a582e` | terminal: rename page serial floor as epoch | C1 |
| 14 | `0bad91adb` | terminal: renew generations for direct row shifts | C2 |
| 15 | `89eca063e` | terminal/search: avoid pruning history on reload | C7 |
| 16 | `c9ad708ef` | terminal: validate flattened highlight generations | C7 |
| 17 | `0d015b2fc` | terminal: preserve compressed pages during search | C5 |

**Binding rule for this whole phase:** port each commit's **test hunks** along with its production hunk (MAIN_PARITY doctrine, verbatim: "ALSO port the test hunks from the same commit"). The oracle test names are listed per task; each becomes a rustty test named `<subject>_<zig name in snake_case>`.

---

### Task C1: Rename the serial floor to an epoch

**Files:**
- Modify: `crates/rustty-vt/src/pagelist/mod.rs` (`page_serial_min` at `:552`, `:932`, `:1138`, `:1257`)
- Modify: `crates/rustty-vt/src/search/screen.rs` (`:565-566`)

**Oracle:** `f6d9a582e` (137-line `PageList.zig` diff + 16 lines in `search/screen.zig`). The final field doc is `oracle@main:src/terminal/PageList.zig:382-392`.

**Interfaces:**
- Produces: `PageList.page_serial_epoch: u64` (replacing `page_serial_min`), with the oracle's semantics:
  - Only `reset` advances it.
  - A generation **below** the epoch is definitely invalid → O(1) rejection.
  - A generation **at or above** the epoch is only *potentially* valid and must still be verified against the live list.
  - Generations are **not monotonic in list order**: replacement and split operations can put a fresh generation before older live pages. (This is the whole reason the field is an epoch and not a minimum.)
- Consumed by: C2's `node_is_valid`, C3, C7.

- [ ] **Step 1: Write the failing test**

Add to `crates/rustty-vt/src/pagelist/tests.rs`:

```rust
/// PORTS: oracle f6d9a582e — only `reset` begins a new whole-list validity
/// epoch. Growth, replacement and splitting must NOT advance it, because
/// generations are not monotonic in list order.
#[test]
fn pagelist_only_reset_advances_the_serial_epoch() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    let epoch0 = s.page_serial_epoch;
    fill_compressible_history(&mut s, 4);
    assert_eq!(s.page_serial_epoch, epoch0, "growth must not advance the epoch");

    s.reset();
    assert!(s.page_serial_epoch > epoch0, "reset begins a new epoch");
    // Every live node belongs to the current epoch.
    let mut it = s.pages.first;
    while let Some(node) = it {
        assert!(unsafe { (*node).serial } >= s.page_serial_epoch);
        it = unsafe { core::ptr::NonNull::new((*node).next).map(|p| p.as_ptr()) };
    }
    s.deinit();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib pagelist_only_reset_advances`
Expected: FAIL — `no field page_serial_epoch on type PageList`

- [ ] **Step 3: Write minimal implementation**

Rename the field and port the oracle's doc comment verbatim:

```rust
    /// The next globally unique reference generation for this PageList. A
    /// generation is assigned whenever a page is allocated, reused as new, or
    /// has its coordinate layout changed by a live-page operation.
    ///
    /// The serial number can be used to detect whether the page is identical
    /// across operations. Because we use both a pool and heap memory, pointer
    /// stability is not guaranteed, but the serial will always be different for
    /// different page generations.
    pub page_serial: u64,

    /// The first serial in the current whole-list validity epoch. Only `reset`
    /// advances this; it is NOT the generation of the first page or the exact
    /// minimum live generation. Page generations are not monotonic in list
    /// order: replacement and split operations can put a fresh generation
    /// before older live pages.
    ///
    /// A generation below this epoch is definitely invalid, allowing O(1)
    /// rejection. A generation at or above it is only POTENTIALLY valid and
    /// must still be verified against the live list.
    ///
    /// PORTS: oracle@main PageList.zig `page_serial_epoch` (f6d9a582e).
    pub page_serial_epoch: u64,
```

Update the four rustty sites (`:932` init, `:1138` integrity assertion, `:1257` reset) and `search/screen.rs:565-566`. The integrity assertion message becomes the oracle's:
`"PageList integrity violation: page serial predates epoch serial={} epoch={}"`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib pagelist`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/pagelist/mod.rs crates/rustty-vt/src/search/screen.rs crates/rustty-vt/src/pagelist/tests.rs
git commit -m "vt/pagelist: rename page serial floor as epoch (WS7 C1, oracle f6d9a582e)"
```

---

### Task C2: `renew_generation` + `node_is_valid`

**Files:**
- Modify: `crates/rustty-vt/src/pagelist/mod.rs` (new methods; `:3122-3165` and `:5023-5041` in the oracle)
- Modify: `crates/rustty-vt/src/pagelist/erase.rs` (`erase_row`, `erase_row_bounded`, trailing-row truncation)
- Modify: `crates/rustty-vt/src/screen.rs`, `crates/rustty-vt/src/terminal.rs` (direct row-shift paths)

**Oracle:** `dd956f37c` (62 lines, PageList only) and `0bad91adb` (PageList 4, Screen 51, Terminal 45). Final form: `oracle@main:src/terminal/PageList.zig:3122-3165` (`renewGeneration`) and `:5023-5041` (`nodeIsValid`).

**Interfaces:**
- Produces:
```rust
impl PageList {
    /// Give a live node a new generation before changing its coordinate layout.
    ///
    /// Without a new generation, a cached pointer, serial, and coordinate could
    /// still pass validation while naming a different row.
    ///
    /// The ONLY state changed here is `node.serial` and the next-generation
    /// counter `page_serial`. Consequently every previously captured
    /// pointer-plus-serial reference to this node is rejected by
    /// `node_is_valid` after this call. This does NOT advance
    /// `page_serial_epoch`: only `reset` starts a new whole-list validity epoch.
    ///
    /// This does NOT reset compression continuation state; the surrounding
    /// operation remains responsible for that.
    ///
    /// PORTS: oracle@main PageList.zig `renewGeneration`.
    pub fn renew_generation(&mut self, node: *mut Node);

    /// Returns whether a node pointer and serial still identify the same page
    /// in this list. The node pointer is only COMPARED and is safe even if the
    /// node has been destroyed or reused since the serial was captured.
    ///
    /// PORTS: oracle@main PageList.zig `nodeIsValid`.
    pub fn node_is_valid(&self, target: *const Node, serial: u64) -> bool;
}
```
- Consumed by: C3, C4, C5, C6, C7.

**Where `renew_generation` must be called** (from `dd956f37c` + `0bad91adb`): every place that shifts rows within a live page or truncates trailing rows without replacing the page — `PageList::erase_row`, `PageList::erase_row_bounded`, the trailing-row truncation in `erase_rows`, `Screen::cursor_scroll_region_up` and its siblings, and the `Terminal` scroll-region paths the oracle touches. Diff both commits and port site-for-site.

**DIVERGENCE #11 (recorded, not silent — see `.port/phase-2/DEFERRALS.md` "Deferred to WS8 upstream sweep — WS7 Phase C divergence #11"):** `Screen::cursor_scroll_region_up` does not exist in rustty — it is defined by unported upstream commit `77190bd02` (`MAIN_PARITY.md` BL-4), so `0bad91adb`'s 4th `invalidateNodeLayout` site (inside that function) and the test below have nothing to port onto. The other 3 Screen sites (`cursor_scroll_above`'s fast path + `cursor_scroll_above_rotate`'s two sites) and their 3 tests ARE ported verbatim. This omission belongs to WS8, not WS7.

- [ ] **Step 1: Write the failing test**

Port all three oracle tests:

```rust
/// PORTS: oracle dd956f37c `test "PageList row erasure renews affected page generations"`
#[test]
fn pagelist_row_erasure_renews_affected_page_generations() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    fill_compressible_history(&mut s, 3);
    let node = s.pages.first.unwrap();
    let before = unsafe { (*node).serial };
    assert!(s.node_is_valid(node, before));

    s.erase_row(point::Point::Screen(point::Coordinate { x: 0, y: 0 })).unwrap();

    let after = unsafe { (*node).serial };
    assert_ne!(after, before, "erasing a row must renew the page generation");
    assert!(!s.node_is_valid(node, before), "the stale generation is rejected");
    assert!(s.node_is_valid(node, after));
    s.deinit();
}

/// PORTS: oracle dd956f37c `test "PageList trailing row truncation renews page generation"`
#[test]
fn pagelist_trailing_row_truncation_renews_page_generation() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    fill_compressible_history(&mut s, 3);
    let last = s.pages.last.unwrap();
    let before = unsafe { (*last).serial };
    s.erase_rows(
        point::Point::Active(point::Coordinate { x: 0, y: 0 }),
        Some(point::Point::Active(point::Coordinate { x: 0, y: 0 })),
    );
    assert_ne!(unsafe { (*last).serial }, before);
    s.deinit();
}

/// PORTS: oracle 0bad91adb `test "Screen: cursorScrollRegionUp renews page generation"`
/// Lives in `crates/rustty-vt/src/screen.rs` tests.
#[test]
fn screen_cursor_scroll_region_up_renews_page_generation() {
    let mut s = Screen::init(80, 24, 10 * 1024 * 1024).unwrap();
    let node = s.pages.pages.last.unwrap();
    let before = unsafe { (*node).serial };
    s.cursor_scroll_region_up();
    assert_ne!(unsafe { (*node).serial }, before);
    assert!(!s.pages.node_is_valid(node, before));
    s.deinit();
}
```

(Adapt the exact `erase_row`/`erase_rows`/`cursor_scroll_region_up` call shapes to the rustty signatures; the assertion structure is the contract.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib renews_page_generation renews_affected_page`
Expected: FAIL — `no method named node_is_valid found for struct PageList`

- [ ] **Step 3: Write minimal implementation**

```rust
    pub fn renew_generation(&mut self, node: *mut Node) {
        unsafe { (*node).serial = self.page_serial };
        self.page_serial += 1;
    }

    pub fn node_is_valid(&self, target: *const Node, serial: u64) -> bool {
        // Reset invalidates the whole prior epoch, so reject those generations
        // without scanning the live list.
        if serial < self.page_serial_epoch {
            return false;
        }

        let mut it = self.pages.first;
        while let Some(node) = it {
            if node as *const Node == target {
                return unsafe { (*node).serial } == serial;
            }
            it = core::ptr::NonNull::new(unsafe { (*node).next }).map(|p| p.as_ptr());
        }

        false
    }
```

Then add the `renew_generation` calls at the sites from `dd956f37c` and `0bad91adb`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustty-vt --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/pagelist/ crates/rustty-vt/src/screen.rs crates/rustty-vt/src/terminal.rs
git commit -m "vt/pagelist: renew page generations for shifted rows + nodeIsValid (WS7 C2)"
```

---

### Task C3: Invalidate split / partially-erased page refs; keep the floor conservative

**Files:**
- Modify: `crates/rustty-vt/src/pagelist/{mod,erase,grow,resize}.rs`
- Modify: `crates/rustty-vt/src/search/screen.rs` (the two `select after ...` tests)

**Oracle:** `91a63e528` (35 lines), `a804e3ac7` (78 lines PageList + 33 search/screen), `f1a5fab45` (183 lines PageList — the biggest of the three).

**What each does:**
- `91a63e528`: splitting a page leaves the *source* node holding rows that moved. Call `renew_generation` on the source so any cached reference to it is rejected.
- `a804e3ac7`: a partial erase shifts rows within a page without replacing it. Same treatment, plus it must **restart the compression continuation** (the cursor's serial no longer exists).
- `f1a5fab45`: bounded pruning previously advanced the serial floor to the first live node's serial. That is wrong once generations are non-monotonic — a fresh generation can sit *before* older live pages, so advancing the floor would invalidate live references. Keep the floor conservative: only `reset` moves it.

**Interfaces:**
- Consumes: `renew_generation`, `node_is_valid`, `page_compression.reset()`.
- Produces: no new public API — this is behavior on existing methods.

- [ ] **Step 1: Write the failing test**

Port all six oracle tests. In `pagelist/tests.rs`:

- `pagelist_bounded_pruning_after_split_invalidation_preserves_live_serials` (from `91a63e528`)
- `pagelist_bounded_pruning_after_partial_erase_preserves_live_serials` (from `a804e3ac7`)
- `pagelist_partial_erase_restarts_compression_before_continuation` (from `a804e3ac7`)
- `pagelist_repeated_bounded_pruning_after_split_preserves_live_serials` (from `f1a5fab45`)
- `pagelist_bounded_pruning_after_front_replacement_preserves_live_serials` (from `f1a5fab45`)
- `pagelist_bounded_pruning_after_middle_replacement_preserves_live_serials` (from `f1a5fab45`)

The common shape of all six (read the oracle bodies for the exact setup):

```rust
#[test]
fn pagelist_bounded_pruning_after_split_invalidation_preserves_live_serials() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    fill_compressible_history(&mut s, 4);

    // Capture (node, serial) for every live page BEFORE the operation.
    let live: Vec<(*mut Node, u64)> = {
        let mut v = Vec::new();
        let mut it = s.pages.first;
        while let Some(n) = it {
            v.push((n, unsafe { (*n).serial }));
            it = core::ptr::NonNull::new(unsafe { (*n).next }).map(|p| p.as_ptr());
        }
        v
    };

    // <the operation this test names: split / partial erase / bounded prune>

    // Every reference that names a still-live, still-unshifted page must remain
    // valid; every reference to a shifted or replaced page must be rejected.
    for (node, serial) in live {
        let still_live = { /* walk s.pages and compare pointers */ true };
        if still_live {
            assert_eq!(s.node_is_valid(node, serial), unsafe { (*node).serial } == serial);
        } else {
            assert!(!s.node_is_valid(node, serial));
        }
    }
    s.deinit();
}
```

In `search/screen.rs` tests:
- `search_screen_select_after_history_page_split_ignores_moved_results` (from `91a63e528`)
- `search_screen_select_after_partial_history_page_erase_ignores_shifted_results` (from `a804e3ac7`)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib preserves_live_serials ignores_moved_results ignores_shifted_results`
Expected: FAIL — the pruning currently advances the floor and/or does not renew the source generation, so at least one assertion trips.

- [ ] **Step 3: Write minimal implementation**

Port the three commits' production hunks. The `f1a5fab45` hunk is the one to read most carefully: it removes the floor-advancing logic from bounded pruning and replaces it with a comment explaining that generations are not monotonic. Do **not** re-derive it — diff it.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustty-vt --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/pagelist/ crates/rustty-vt/src/search/screen.rs
git commit -m "vt/pagelist: invalidate split/partially-erased page refs; conservative serial floor (WS7 C3)"
```

---

### Task C4: Reschedule compression after page replacement

**Files:**
- Modify: `crates/rustty-vt/src/pagelist/{grow,resize}.rs` (every page-replacement site)

**Oracle:** `d0a26d146` (30 lines, PageList only).

**Interfaces:**
- Consumes: `page_compression.reset()`, `page_compression.mark_activity()`.
- Produces: no new API.

**Why:** replacing a node (e.g. `increase_capacity`) destroys the node the incremental cursor's `last_serial` names. Without a `reset()` + `mark_activity()`, the incremental pass silently restarts from the beginning *without* the scheduler knowing there is work — so compression stalls until the next unrelated mutation.

- [ ] **Step 1: Write the failing test**

```rust
/// PORTS: oracle d0a26d146
/// `test "PageList replacements preserve compression continuation and mark activity"`
#[test]
fn pagelist_replacements_preserve_compression_continuation_and_mark_activity() {
    let mut s = PageList::init(80, 24, 10 * 1024 * 1024).unwrap();
    fill_compressible_history(&mut s, 6);

    // Advance the incremental cursor part-way.
    assert_eq!(
        s.compress(CompressionMode::Incremental),
        IncrementalCompressionResult::Pending
    );
    assert!(s.page_compression.last_serial.is_some());
    let activity = s.page_compression.activity_serial;

    // Replace a page (capacity increase replaces the node).
    let node = s.pages.first.unwrap();
    let _ = s.increase_capacity(node, IncreaseCapacity::Styles).unwrap();

    // The stale continuation is dropped and the scheduler is told to run again.
    assert!(s.page_compression.last_serial.is_none());
    assert_ne!(s.page_compression.activity_serial, activity);
    s.deinit();
}
```

(Match `increase_capacity`'s real signature from `pagelist/grow.rs`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib pagelist_replacements_preserve_compression`
Expected: FAIL — `last_serial` is still `Some` after the replacement.

- [ ] **Step 3: Write minimal implementation**

Port `d0a26d146`'s hunk: add `self.page_compression.reset(); self.page_compression.mark_activity();` at each replacement site the oracle marks.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib pagelist`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/pagelist/
git commit -m "vt/pagelist: reschedule compression after page replacement (WS7 C4, oracle d0a26d146)"
```

---

### Task C5: Search reads compressed history without decompressing it

**Files:**
- Modify: `crates/rustty-vt/src/search/sliding_window.rs` (`Meta`, `append`, `flatten`)
- Modify: `crates/rustty-vt/src/search/pagelist.rs` (`PageListSearch::feed`)
- Modify: `crates/rustty-vt/src/search/viewport.rs`, `active.rs` (call-site updates)

**Oracle:** `0d015b2fc` "terminal: preserve compressed pages during search" (153 insertions) and `83aada205` "terminal/search: preserve serial order in reverse matches" (11 lines, `sliding_window.zig`).

**Interfaces:**
- Produces:
```rust
/// Information copied from a page while appending it to the window.
///
/// Both values remain safe after the node's preserved page is released. In
/// particular, callers use `last_row_wrapped` to decide whether an adjacent
/// page can contribute to a cross-page match without reading the node again.
///
/// PORTS: oracle@main search/sliding_window.zig `SlidingWindow.AppendResult`.
pub struct AppendResult {
    pub content_len: usize,
    pub last_row_wrapped: bool,
}

impl SlidingWindow {
    pub fn append(&mut self, node: *mut Node) -> Result<AppendResult, OutOfMemory>;
}
```
- `SlidingWindow::Meta` gains `pub rows: size::CellCountInt` — a **copy** of the page's row count taken while the preserved page is alive, so `flatten` never dereferences a node after the terminal lock is released.
- `PageListSearch::feed` becomes `rem -= self.window.append(node)?.content_len;`.
- `83aada205`: in the reverse-match path, `flatten` must use a snapshotted `cross_page_end_rows` (captured while walking `Meta`) instead of `nodes[0].rows()`.

**Why it matters:** without this, searching decompresses every history page it inspects, silently undoing the entire feature.

- [ ] **Step 1: Write the failing test**

Port `0d015b2fc`'s regression test into `crates/rustty-vt/src/search/pagelist.rs` tests:

```rust
/// PORTS: oracle 0d015b2fc
/// `test "compressed history match spanning page boundary remains compressed"`
#[test]
fn search_pagelist_compressed_history_match_spanning_page_boundary_remains_compressed() {
    let mut t = Terminal::init(TerminalOptions {
        cols: 80,
        rows: 24,
        max_scrollback: 10 * 1024 * 1024,
        ..Default::default()
    })
    .unwrap();
    let mut stream = t.vt_stream();

    let pages: *mut PageList = &mut t.screens.active_mut().pages;
    let first = unsafe { (*pages).pages.first.unwrap() };
    let first_page_rows = unsafe { Node::capacity(first) }.rows;

    // Put half the needle on either side of a soft-wrapped page boundary.
    for _ in 0..first_page_rows - 1 {
        stream.next_slice(b"\r\n");
    }
    for _ in 0..unsafe { (*pages).cols } - 2 {
        stream.next_slice(b"x");
    }
    stream.next_slice(b"Test");
    let second = unsafe { (*first).next };
    assert_eq!(second, unsafe { (*pages).pages.last.unwrap() });

    // Move both matching pages completely into history.
    while unsafe { (*pages).pages.last.unwrap() } == second {
        stream.next_slice(b"\r\n");
    }
    for _ in 0..unsafe { (*pages).rows } {
        stream.next_slice(b"\r\n");
    }
    assert_ne!(unsafe { (*pages).get_top_left(Point::Active).node }, second);

    let _ = unsafe { (*pages).compress(CompressionMode::Full) };
    assert_eq!(unsafe { Node::storage(first) }, Storage::Compressed);
    assert_eq!(unsafe { Node::storage(second) }, Storage::Compressed);
    let compressed = unsafe { (*pages).memory_stats() };
    assert!(compressed.compressed_pages >= 2);

    let mut search = PageListSearch::init(b"Test", pages, unsafe {
        (*pages).pages.last.unwrap()
    })
    .unwrap();

    // Drain and feed incrementally until the boundary match is available. The
    // exact number of feeds depends on how much blank-page text the formatter
    // trims, so bound progress by the number of pages.
    let mut found = None;
    for _ in 0..unsafe { (*pages).total_pages() } + 1 {
        if let Some(m) = search.next() {
            found = Some(m);
            break;
        }
        if !search.feed().unwrap() {
            break;
        }
    }
    let found = found.expect("cross-page match in compressed history");

    let m = found.untracked();
    assert_eq!(m.start.node, first);
    assert_eq!(m.end.node, second);
    assert_eq!(m.start.x, unsafe { (*pages).cols } - 2);
    assert_eq!(m.end.x, 1);

    // Search owns formatted text and coordinate maps, not page snapshots. The
    // original raw mappings therefore remain discarded after matching.
    assert_eq!(unsafe { (*pages).memory_stats() }, compressed);
    assert_eq!(unsafe { Node::storage(first) }, Storage::Compressed);
    assert_eq!(unsafe { Node::storage(second) }, Storage::Compressed);

    search.deinit();
    t.deinit();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib search_pagelist_compressed_history_match`
Expected: FAIL — the final `Storage::Compressed` assertions fail, because `append` currently calls `Node::page(node)` and restores both pages.

- [ ] **Step 3: Write minimal implementation**

Port `0d015b2fc` + `83aada205`:
1. `SlidingWindow::append` obtains the page with `Node::page_preserving_state(node)`, formats through it, copies `rows` and `last_row_wrapped` into the returned `AppendResult` and the stored `Meta`, then `deinit()`s the preserved page.
2. `Meta` gains `rows`; the two `meta.node.rows()` sites in `flatten` become `meta.rows`.
3. Snapshot `cross_page_end_rows` while traversing `Meta` and use it in the reverse cross-page fixup instead of `nodes[0].rows()`.
4. `PageListSearch::feed` unwraps `.content_len`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustty-vt --lib search`
Expected: PASS — the new test plus the entire existing search suite.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/search/
git commit -m "vt/search: read compressed history through preserved page snapshots (WS7 C5)"
```

---

### Task C6: Search rejects destroyed / replaced / pruned state

**Files:**
- Modify: `crates/rustty-vt/src/search/{thread,screen,pagelist}.rs`
- Modify: `crates/rustty-vt/src/pagelist/mod.rs` (`86e1f7b8b` adds 18 lines to `PageList.zig`)

**Oracle:** `d239f9805` (39), `5d8eb78b7` (31), `86e1f7b8b` (18 PageList + 69 search/screen), `73ac36fa5` (100 search/Thread + 7 search/pagelist + 23 search/screen).

**Interfaces:**
- Consumes: `PageList::node_is_valid`.
- Produces: no new public API in `rustty-vt`'s exported surface; each fix is a guard on an existing search path.

**Oracle tests to port (names → rustty test fns in `search/screen.rs` / `search/pagelist.rs`):**

| oracle test | rustty test fn |
|---|---|
| `"select after partial history erase drops a pruned selection"` (`d239f9805`) | `search_screen_select_after_partial_history_erase_drops_a_pruned_selection` |
| `"feed keeps its tracked pin within a shorter page"` (`5d8eb78b7`) | `search_pagelist_feed_keeps_its_tracked_pin_within_a_shorter_page` |
| `"select after history compaction ignores replaced results"` (`86e1f7b8b`) | `search_screen_select_after_history_compaction_ignores_replaced_results` |
| `"select after active screen removal"` (`73ac36fa5`) | `search_screen_select_after_active_screen_removal` |

- [ ] **Step 1: Write the failing tests**

Port all four verbatim from the oracle commits (`git -C /Users/gael/projects/ghostty show <hash>` and read the `+test` blocks). Each follows the same shape: build a terminal, search, mutate the page list in the way named by the test, then assert `select`/`feed` returns nothing (or a clamped pin) instead of a stale coordinate.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rustty-vt --lib search_screen_select_after search_pagelist_feed_keeps`
Expected: FAIL — 4 tests, each with a stale/out-of-range coordinate assertion.

- [ ] **Step 3: Write minimal implementation**

Port the four production hunks:
- `d239f9805`: drop a selection whose result pages were pruned.
- `5d8eb78b7`: reset the feed's tracked pin when the page it lands on shrank.
- `86e1f7b8b`: reject a result whose page was replaced — this is where `PageList::node_is_valid` gets its first real caller; the 18-line `PageList.zig` hunk is a helper this needs.
- `73ac36fa5`: discard `search::Thread` state whose screen was destroyed.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustty-vt --lib search`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/search/ crates/rustty-vt/src/pagelist/mod.rs
git commit -m "vt/search: reject destroyed, replaced and pruned result state (WS7 C6)"
```

---

### Task C7: Generation-aware fingerprints, reload, and highlight flattening

**Files:**
- Modify: `crates/rustty-vt/src/search/viewport.rs` (`c243d89bd`)
- Modify: `crates/rustty-vt/src/search/screen.rs` (`a89c6133c`, `89eca063e`)
- Modify: `crates/rustty-vt/src/render.rs` (`c9ad708ef` — 125-line `render.zig` diff)

**Oracle tests to port:**

| oracle test | rustty test fn |
|---|---|
| `"fingerprint detects node generation changes"` (`c243d89bd`) | `search_viewport_fingerprint_detects_node_generation_changes` |
| `"reload after partial history page erase drops shifted selection first"` (`a89c6133c`) | `search_screen_reload_after_partial_history_page_erase_drops_shifted_selection_first` |
| `"reload defers pruning unselected history results"` (`89eca063e`) | `search_screen_reload_defers_pruning_unselected_history_results` |
| `"flattened highlights require matching page serial"` (`c9ad708ef`) | `render_flattened_highlights_require_matching_page_serial` |

**Interfaces:**
- `c243d89bd`: the viewport fingerprint must include each node's **generation**, not just its pointer and row count. Add the serial to the fingerprint tuple; `viewport.rs` already stores per-node fingerprint entries.
- `a89c6133c` / `89eca063e`: reload must validate the history *before* pruning, and must **defer** pruning results that are merely unselected (pruning them on reload loses results a user could still reach).
- `c9ad708ef`: `render::Flattened` gains a per-chunk serial check — a flattened highlight whose chunk serial no longer matches its node is dropped rather than drawn at a stale coordinate.

- [ ] **Step 1: Write the failing tests**

Port all four verbatim from the oracle commits.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rustty-vt --lib search_viewport_fingerprint search_screen_reload render_flattened_highlights`
Expected: FAIL — 4 tests.

- [ ] **Step 3: Write minimal implementation**

Port the four production hunks. `c9ad708ef`'s `render.zig` diff is the largest (108 insertions); read it in full — it restructures the flattening loop, it is not a one-line guard.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustty-vt --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/search/ crates/rustty-vt/src/render.rs
git commit -m "vt: generation-aware search fingerprints, reload and highlight flattening (WS7 C7)"
```

---

### Task C8: Phase C gate

- [ ] **Step 1: Full suite, debug and release**

```bash
cargo test -p rustty-vt --lib
cargo test -p rustty-vt --lib --release
```
Expected: PASS both.

- [ ] **Step 2: Both-platform build**

```bash
cargo build --workspace
ssh rvm 'cd ~/projects/rustty && cargo build --workspace'
```

- [ ] **Step 3: Confirm the known-red Miri set did not grow**

```bash
bash .port/tools/miri-run.sh rustty-vt search 6 45 15
```
Expected: the UB set is **exactly** the 9 pre-existing `search/screen` tests recorded in `.port/phase-2/MIRI_EXCEPTIONS.md` (memory: `search-screen-sb-ub-finding`). **Any new UB name is a WS7 regression and a hard stop** — fix it here, do not add it to the exceptions file.

- [ ] **Step 4: Commit**

```bash
git commit --allow-empty -m "vt: phase C gate — generation stamps + search integrity green (WS7 C8)"
```

**PHASE C COMPLETE.**

---

# PHASE D — Consumer surface

**Requires Phase B.** (Phase C is not a hard prerequisite for D, but the merge order is A→B→C→D→E.)

**D4 and D8 are independently executable** once Phase B is in: D4 (renderer state mutex handoff) touches only `rustty-renderer`/`rustty-termio` and D8 (bench actions) touches only `rustty-bench`. (D5, the lib-vt C API, is NOT independent: it wraps D1's `Terminal` API.)

---

### Task D1: `Terminal` compression API

**Files:**
- Modify: `crates/rustty-vt/src/terminal.rs`

**Oracle:** `oracle@main:src/terminal/Terminal.zig:2541-2605` (final form of `f8c217e55` + `181254d36` + `172f15da3`).

**Interfaces:**
- Produces:
```rust
/// The amount of compression work performed by `compress` before returning.
///
/// PORTS: oracle@main Terminal.zig `CompressionMode` (ABI enum, `lib.Enum`).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalCompressionMode {
    Incremental = 0,
    Full = 1,
}

/// The scheduling result of a `compress` call.
///
/// PORTS: oracle@main Terminal.zig `CompressionResult`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalCompressionResult {
    Unsupported = 0,
    Pending = 1,
    Complete = 2,
}

impl Terminal {
    /// Return the current compression activity value.
    ///
    /// Callers should schedule a `compress` call whenever this value changes.
    ///
    /// Note that compression requires specific system features, namely the
    /// ability to decommit physical memory backings. Callers must still use
    /// `compress` to determine whether compression is supported on the current
    /// target.
    pub fn compression_activity(&self) -> u64;

    /// Compress cold memory to save resident memory space.
    ///
    /// Full compression does a full pass compressing everything it can before
    /// returning; compression is relatively slow and with large scrollbacks
    /// this can cause a noticeable pause. Incremental compression bounds itself
    /// on how much data it can look at and how much work it does before
    /// returning.
    ///
    /// Callers should schedule compression when it doesn't impact the user.
    pub fn compress(&mut self, mode: TerminalCompressionMode) -> TerminalCompressionResult;
}
```
- `compression_activity` reads `self.screens.get(Primary).pages.page_compression.activity_serial` and composes `Terminal`'s reserved high 16 bits exactly as `oracle@main:Terminal.zig:2555-2560` does (read those six lines — the composition is not guessable).
- Consumed by: D3 (renderer scheduling), D5 (C API).

- [ ] **Step 1: Write the failing test**

Add to the `terminal.rs` test module:

```rust
/// The activity token changes when the page list mutates, and does NOT change
/// merely because compression ran.
#[test]
fn terminal_compression_activity_tracks_pagelist_mutation_not_compression() {
    let mut t = Terminal::init(TerminalOptions {
        cols: 80,
        rows: 24,
        max_scrollback: 10 * 1024 * 1024,
        ..Default::default()
    })
    .unwrap();
    let initial = t.compression_activity();

    let mut stream = t.vt_stream();
    let line = b"repeated and compressible terminal history\r\n";
    for _ in 0..4_000 {
        stream.next_slice(line);
    }
    drop(stream);

    let activity = t.compression_activity();
    assert_ne!(activity, initial);

    let mut result = TerminalCompressionResult::Pending;
    for _ in 0..1_000 {
        result = t.compress(TerminalCompressionMode::Incremental);
        match result {
            TerminalCompressionResult::Pending => continue,
            TerminalCompressionResult::Complete => break,
            TerminalCompressionResult::Unsupported => {
                panic!("test builds always support strict reclamation")
            }
        }
    }
    assert_eq!(result, TerminalCompressionResult::Complete);

    // Compression changes storage representation, not the activity token.
    assert_eq!(t.compression_activity(), activity);
    t.deinit();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt --lib terminal_compression_activity`
Expected: FAIL — `no method named compression_activity found for struct Terminal`

- [ ] **Step 3: Write minimal implementation**

Port `oracle@main:Terminal.zig:2541-2605` verbatim, including all doc comments.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustty-vt --lib terminal_compression`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt/src/terminal.rs
git commit -m "vt/terminal: compression activity token and compress scheduling API (WS7 D1)"
```

---

### Task D2: `scrollback-compression` config option

**Files:**
- Modify: `crates/rustty-config/src/config_struct.rs` (field at `:2317`, default at `:4941`, `Key` at `:7449`, `cv!` at `:7736`, formatter at `:8497`)
- Modify: `crates/rustty-config/src/config-template/` (if the template enumerates options — grep for `scrollback-limit` there)

**Oracle:** `0fb89f4ff` "terminal: configure scrollback compression" — `Config.zig` hunk only (the `renderer/Thread.zig` hunk is D3).

**Interfaces:**
- Produces: `Config.scrollback_compression: bool` (default `true`), `Key::ScrollbackCompression`, and the `scrollback-limit` default raised from `10_000_000` to `50_000_000`.
- Doc comment: port the oracle's verbatim (with R2 renaming `Ghostty`→`Rustty`), including the "70% to 90% reduction", "virtual memory does not decrease", and "already-compressed pages remain compressed" paragraphs, and the amended `scrollback-limit` doc.

- [ ] **Step 1: Write the failing test**

Add to the `rustty-config` test module (next to the other option-parsing tests):

```rust
/// PORTS: oracle 0fb89f4ff — a default-on scrollback compression option, and
/// a 50 MB default logical scrollback limit.
#[test]
fn config_scrollback_compression_defaults_and_parses() {
    let c = Config::default();
    assert!(c.scrollback_compression);
    assert_eq!(c.scrollback_limit, 50_000_000);

    let mut c = Config::default();
    c.parse_line("scrollback-compression = false").unwrap();
    assert!(!c.scrollback_compression);

    // Round-trips through the formatter.
    let mut out = String::new();
    c.format_entry("scrollback-compression", &mut out).unwrap();
    assert!(out.contains("scrollback-compression = false"));
}
```

(Match the existing config-test helper names in `crates/rustty-config/src/` — grep for an existing `fn config_` test that parses a bool option and copy its shape.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-config --lib config_scrollback_compression`
Expected: FAIL — `no field scrollback_compression on type Config`

- [ ] **Step 3: Write minimal implementation**

Add the field, default, `Key` variant, comparison macro entry and formatter entry at the five sites listed above, with the oracle's doc comment.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rustty-config --lib
```
Expected: PASS. **Note:** raising the `scrollback-limit` default will break any test that asserts `10_000_000`; those assertions must be updated to `50_000_000` (the oracle did the same), not the default reverted.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-config/
git commit -m "config: scrollback-compression option; raise scrollback-limit default to 50MB (WS7 D2)"
```

---

### Task D3: Renderer-thread compression scheduling

**Files:**
- Modify: `crates/rustty-renderer/src/config.rs` (`RendererConfig` trait + `DerivedConfig`)
- Modify: `crates/rustty-config/src/config_struct.rs` (the `impl RendererConfig for Config` block — memory: `renderer-config-accessor-trait-acyclic`)
- Modify: `crates/rustty-renderer/src/thread.rs`

**Oracle:** `461562ca4` (renderer/Thread.zig +157), `0fb89f4ff` (Thread.zig +20), `181254d36` (Thread.zig 253-line rewrite — this is the final shape), `25e624569` (Thread.zig +8), `95685afd2` (Thread.zig -1). Read the **final** `oracle@main:src/renderer/Thread.zig` `Compression` struct at `:758-870` plus its call sites at `:77-78, :119-124, :189-192, :207-208, :513-518, :567-569`.

**Interfaces:**
- `RendererConfig` gains `fn scrollback_compression(&self) -> bool;` (the accessor-trait pattern — the renderer must never name a config type).
- `DerivedConfig` gains `pub scrollback_compression: bool`.
- New private struct in `thread.rs`:

```rust
/// Schedules incremental terminal compression after renderer activity stops.
///
/// This owns all renderer-specific compression state. The terminal decides
/// when compression-relevant activity changes and performs the actual work;
/// this only decides WHEN to ask.
///
/// PORTS: oracle@main renderer/Thread.zig `Compression` (:758-870).
struct Compression {
    /// The last activity token we acted on. `None` forces the next wake to
    /// schedule a pass.
    activity: Option<u64>,
    timer: xev::Timer,
    timer_c: xev::Completion,
}

impl Compression {
    fn init() -> Result<Compression, xev::Error>;
    fn deinit(&mut self);
    /// Start or postpone compression after a renderer wake.
    fn wake(&mut self, thread: *mut Thread);
    fn schedule(&mut self, thread: *mut Thread, delay_ms: u64);
    /// One incremental step. Returns the delay (ms) until the next step, or
    /// `None` when the pass is complete/unsupported/disabled.
    fn step(&mut self, thread: *mut Thread) -> Option<u64>;
}
```
- `Thread` gains `compression: Compression`, gated on `rustty_vt::compression_enabled()` at `init`/`deinit`/`wake` exactly where the oracle writes `if (comptime terminalpkg.compression_enabled)`.
- `change_config` must clear `self.compression.activity = None` when the option transitions **disabled → enabled**, so a fresh idle pass starts (oracle `:513-518`).
- `25e624569`: the render wake path must not starve compression — port the 8-line guard verbatim.

- [ ] **Step 1: Write the failing test**

Add to `crates/rustty-renderer/src/thread.rs` tests:

```rust
/// PORTS: oracle 0fb89f4ff/461562ca4 — the renderer honors the config option
/// across reloads, and re-enabling starts a fresh idle pass.
#[test]
fn renderer_thread_compression_honors_config_reload() {
    let mut c = TestRendererConfig::default();
    c.scrollback_compression = true;
    let mut derived = DerivedConfig::init(&c);
    assert!(derived.scrollback_compression);

    c.scrollback_compression = false;
    derived = DerivedConfig::init(&c);
    assert!(!derived.scrollback_compression);
}

/// A disabled option means `step` never asks the terminal to compress.
#[test]
fn renderer_thread_compression_step_is_inert_when_disabled() {
    // Uses the offscreen thread harness already present in this module; assert
    // that `Compression::step` returns None when
    // `thread.config.scrollback_compression` is false, without touching the
    // terminal.
    // (See `crates/rustty-renderer/src/thread.rs` existing test scaffolding.)
}
```

Fill the second test's body against the module's existing offscreen harness (grep `mod tests` in `thread.rs` for the established construction). If no harness exists, the deliverable test for this task is the first one plus the D-phase build gate; state that in the commit message rather than inventing a fake harness.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-renderer --lib renderer_thread_compression`
Expected: FAIL — `no field scrollback_compression on type DerivedConfig`

- [ ] **Step 3: Write minimal implementation**

Port the `Compression` struct and every call site from the final `oracle@main:src/renderer/Thread.zig`. The `xev::Timer` usage mirrors the other timers already in `crates/rustty-renderer/src/thread.rs` (`draw_now`, `cursor` timers) — copy that idiom rather than inventing a new one.

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rustty-renderer --lib
cargo build --workspace
```
Expected: PASS / `Finished`.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-renderer/ crates/rustty-config/src/config_struct.rs
git commit -m "renderer: schedule idle scrollback compression, debounced by activity (WS7 D3)"
```

---

### Task D4: Renderer state mutex handoff

**Files:**
- Modify: `crates/rustty-renderer/src/state.rs`
- Modify: `crates/rustty-renderer/src/generic.rs`
- Modify: `crates/rustty-termio/src/exec.rs`

**Oracle:** `d34b54e9b` "renderer: hand off state mutex to avoid starving frames" (State.zig +73, generic.zig 6, termio/Exec.zig +9).

**Independently executable** — this fix is about mutex fairness under heavy pty output; it is bundled into BL-1 because compression made the starvation visible, but it has no compression dependency.

**Interfaces:**
- Produces on `renderer::State`:
```rust
    /// The number of threads currently waiting to acquire `mutex` via
    /// `lock_demand`. Not protected by the mutex; read by hot lock/unlock loops
    /// (the IO parse thread) in `yield_to_demand`.
    demand: AtomicU32,

    /// Handoff generation counter. Incremented (with a futex wake) by
    /// `unlock_demand` after a demanding waiter releases the mutex.
    handoff_gen: AtomicU32,
```
and:
```rust
    /// Acquire `mutex` while signaling demand for it. Must be released with
    /// `unlock_demand`.
    pub fn lock_demand(&self) -> StateGuard<'_>;

    /// Release and notify hot loops parked in `yield_to_demand`.
    pub fn unlock_demand(&self, guard: StateGuard<'_>);

    /// Called by hot lock/unlock loops between critical sections with `mutex`
    /// NOT held: if a `lock_demand` waiter exists, sleep until it has acquired
    /// and released the mutex (or `HANDOFF_TIMEOUT` passes).
    pub fn yield_to_demand(&self);
```
- `const HANDOFF_TIMEOUT: Duration = Duration::from_millis(1);`
- All atomics use `Ordering::Relaxed` **on purpose**: they are a scheduling hint, not a synchronization boundary; the mutex still protects the terminal state. Port that comment verbatim.
- PORT NOTE: Zig uses `std.Thread.Futex.wake/timedWait`. rustty has no futex wrapper in `rustty-os`; use `std::sync::{Condvar, Mutex}` **or** `rustty_zigstd`'s existing futex shim if one exists (grep `Futex` in `crates/rustty-zigstd/src/`). Whichever is chosen, the observable contract is the one documented above; record the choice in a PORT NOTE.

- [ ] **Step 1: Write the failing test**

```rust
/// PORTS: oracle d34b54e9b — a hot lock/unlock loop must hand the mutex to a
/// demanding waiter instead of starving it.
#[test]
fn renderer_state_yield_to_demand_hands_off_to_a_waiter() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let state = Arc::new(State::new_for_test());
    let waiter_ran = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));

    // Renderer-like thread: takes the lock with demand exactly once.
    let s = state.clone();
    let w = waiter_ran.clone();
    let renderer = std::thread::spawn(move || {
        let g = s.lock_demand();
        w.store(true, Ordering::Relaxed);
        s.unlock_demand(g);
    });

    // Parse-loop-like thread: relocks in a tight loop with a yield between.
    let s = state.clone();
    let st = stop.clone();
    let parser = std::thread::spawn(move || {
        while !st.load(Ordering::Relaxed) {
            {
                let _g = s.mutex.lock().unwrap();
            }
            s.yield_to_demand();
        }
    });

    renderer.join().unwrap();
    assert!(waiter_ran.load(Ordering::Relaxed));
    stop.store(true, Ordering::Relaxed);
    parser.join().unwrap();
}
```

This test would hang (and be killed by the harness) without the handoff on a loaded machine; with the handoff it completes immediately. Give it an explicit bound so a failure is a failure rather than a hang: wrap the `renderer.join()` in a 5-second watchdog using a `std::sync::mpsc::channel` + `recv_timeout`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-renderer --lib renderer_state_yield_to_demand`
Expected: FAIL — `no method named lock_demand found for struct State`

- [ ] **Step 3: Write minimal implementation**

Port `d34b54e9b`'s three hunks: `State` (the two atomics + three methods + all doc comments), `generic.rs` (the renderer's frame snapshot switches from `mutex.lock()` to `lock_demand`/`unlock_demand`), and `termio/exec.rs` (`yield_to_demand()` between parse batches).

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test -p rustty-renderer --lib
cargo test -p rustty-termio --lib
cargo build --workspace
ssh rvm 'cd ~/projects/rustty && cargo build --workspace'
```

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-renderer/src/{state.rs,generic.rs} crates/rustty-termio/src/exec.rs
git commit -m "renderer: hand off state mutex to avoid starving frames (WS7 D4, oracle d34b54e9b)"
```

**PROSPECTIVE WARNING (WS7 Phase C review, M5) — not a current bug.** A hot
`lock_demand`/`yield_to_demand` handoff loop that keeps `mutex` demanded could
in principle interact badly with `Surface::deinit`'s renderer-thread stop/join
if that join ever came to be attempted while `opts.mutex` were held. This is
**NOT live today**: `crates/rustty-core/src/surface.rs:2608`'s `Surface::deinit`
takes and deinits `search` FIRST, before touching `renderer_thread`/joining it,
so no code path holds `opts.mutex` across the join. If Task D4 or a later
change reorders teardown (e.g. moves renderer-thread stop/join earlier, or adds
a new consumer that locks `mutex` around a join), re-check this ordering
invariant before shipping.

---

### Task D5: lib-vt C API

**Files:**
- Modify: `crates/rustty-vt-c/src/terminal.rs`
- Modify: `include/rustty/vt/terminal.h`, `include/rustty/vt.h`
- Modify: `.port/tools/gates/` A4 ABI gate fixture (whatever file records the frozen export list — grep `rustty_terminal_` there)

**Oracle:** `172f15da3` "terminal: expose compression through libghostty-vt" — `src/terminal/c/terminal.zig` (+136), `include/ghostty/vt/terminal.h` (+96), `include/ghostty/vt.h` (+6), `src/lib_vt.zig` (+2), `src/terminal/c/main.zig` (+2).

**Interfaces (R2: new exports are `rustty_*` only — the `ghostty_*` alias layer was deleted upstream at `e4cc852`; see the WS5 research note in the ROADMAP):**

```rust
/// C: RusttyTerminalCompressionMode
pub type CompressionMode = rustty_vt::TerminalCompressionMode;

/// C: RusttyTerminalCompressionResult
pub type CompressionResult = rustty_vt::TerminalCompressionResult;

#[no_mangle]
pub extern "C" fn rustty_terminal_compression_activity(
    terminal: Terminal,
    out_activity: *mut u64,
) -> Result;

#[no_mangle]
pub extern "C" fn rustty_terminal_compress(
    terminal: Terminal,
    mode: c_int,
    out_result: *mut CompressionResult,
) -> Result;
```
Both return `Result::InvalidValue` for a null terminal, a null out-pointer, or a `mode` outside the enum (the oracle uses `std.meta.intToEnum`; rustty uses an explicit `match`). DG5 applies: no panic may cross the ABI — wrap the body in the crate's existing abort-on-unwind helper (grep `cwrap` in `crates/rustty-vt-c/src/cwrap.rs`).

- [ ] **Step 1: Write the failing test**

Port both oracle tests into `crates/rustty-vt-c/src/terminal.rs`:

```rust
/// PORTS: oracle 172f15da3 `test "compression invalid arguments"`
#[test]
fn vtc_compression_invalid_arguments() {
    let mut activity: u64 = 0;
    let mut result = CompressionResult::Unsupported;

    assert_eq!(
        rustty_terminal_compression_activity(core::ptr::null_mut(), &mut activity),
        Result::InvalidValue
    );
    assert_eq!(
        rustty_terminal_compress(
            core::ptr::null_mut(),
            CompressionMode::Incremental as c_int,
            &mut result
        ),
        Result::InvalidValue
    );

    let mut t: Terminal = core::ptr::null_mut();
    assert_eq!(
        rustty_terminal_new(
            &TEST_ALLOCATOR,
            &mut t,
            &TerminalOptions { cols: 80, rows: 24, max_scrollback: 10_000_000 }
        ),
        Result::Success
    );

    assert_eq!(
        rustty_terminal_compression_activity(t, core::ptr::null_mut()),
        Result::InvalidValue
    );
    assert_eq!(
        rustty_terminal_compress(t, CompressionMode::Incremental as c_int, core::ptr::null_mut()),
        Result::InvalidValue
    );
    assert_eq!(
        rustty_terminal_compress(t, -1, &mut result),
        Result::InvalidValue
    );

    rustty_terminal_free(t);
}

/// PORTS: oracle 172f15da3 `test "compression activity and incremental scheduling"`
#[test]
fn vtc_compression_activity_and_incremental_scheduling() {
    let mut t: Terminal = core::ptr::null_mut();
    assert_eq!(
        rustty_terminal_new(
            &TEST_ALLOCATOR,
            &mut t,
            &TerminalOptions { cols: 80, rows: 24, max_scrollback: 10_000_000 }
        ),
        Result::Success
    );

    let mut initial_activity: u64 = 0;
    assert_eq!(
        rustty_terminal_compression_activity(t, &mut initial_activity),
        Result::Success
    );

    let line = b"repeated and compressible terminal history\r\n";
    let mut input = Vec::with_capacity(line.len() * 4_000);
    for _ in 0..4_000 {
        input.extend_from_slice(line);
    }
    rustty_terminal_vt_write(t, input.as_ptr(), input.len());

    let mut activity: u64 = 0;
    assert_eq!(rustty_terminal_compression_activity(t, &mut activity), Result::Success);
    assert_ne!(activity, initial_activity);

    let mut result = CompressionResult::Unsupported;
    let mut completed = false;
    for _ in 0..1_000 {
        assert_eq!(
            rustty_terminal_compress(t, CompressionMode::Incremental as c_int, &mut result),
            Result::Success
        );
        match result {
            CompressionResult::Pending => continue,
            CompressionResult::Complete => {
                completed = true;
                break;
            }
            CompressionResult::Unsupported => unreachable!(),
        }
    }
    assert!(completed);

    // Compression changes storage representation, not the activity token.
    let mut final_activity: u64 = 0;
    assert_eq!(rustty_terminal_compression_activity(t, &mut final_activity), Result::Success);
    assert_eq!(final_activity, activity);

    rustty_terminal_free(t);
}
```

(Adapt `TEST_ALLOCATOR`, `rustty_terminal_new`, `rustty_terminal_vt_write`, `rustty_terminal_free` to the crate's actual export names — grep `crates/rustty-vt-c/src/terminal.rs`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-vt-c --lib vtc_compression`
Expected: FAIL — `cannot find function rustty_terminal_compress in this scope`

- [ ] **Step 3: Write minimal implementation**

Port the C-API functions plus the header declarations. Header text: port `oracle@main:include/ghostty/vt/terminal.h`'s 96-line block with R2 renaming (`Ghostty`→`Rustty`, `ghostty_`→`rustty_`, `GHOSTTY_`→`RUSTTY_`) — including the incremental contract documentation, which the oracle's commit message explicitly calls out.

- [ ] **Step 4: Run tests + the ABI gate**

```bash
cargo test -p rustty-vt-c --lib
bash .port/tools/gates/a4-abi-gate.sh   # exact name: grep for the A4 gate script under .port/tools/gates/
```
Expected: PASS. The A4 gate will flag the two new exports as unfrozen; add them to the gate's frozen list in the same commit (that is the gate's designed workflow — it is not a weakening).

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-vt-c/ include/rustty/ .port/tools/gates/
git commit -m "vt-c: expose compression activity and scheduling through lib-vt (WS7 D5, oracle 172f15da3)"
```

---

### Task D6: Inspector compression memory view

**Files:**
- Modify: `crates/rustty-inspector/src/widgets/pagelist.rs`
- Modify: `crates/rustty-inspector/src/widgets/page.rs`

**Oracle:** `70e788f06`'s inspector hunks (`widgets/page.zig` +19, `widgets/pagelist.zig` +129) and `6d5dda40d` "inspector: clarify page compression memory" (`widgets/pagelist.zig`, 136 insertions / 85 deletions — a rewrite of the memory section; port the **final** file).

**Interfaces:**
- Consumes: `PageList::memory_stats()`, `MemoryStats::{estimated_resident_bytes, estimated_savings}`, `Node::storage`, `Node::page_preserving_state`.
- Produces: no new API. The pagelist widget gains resident/compressed page counts, raw vs. resident vs. encoded bytes, and the estimated-savings line; the page widget must render a compressed page **without restoring it** (use `page_preserving_state`).

- [ ] **Step 1: Write the failing test**

The inspector widgets are ImGui draw code with no unit-test harness. The deliverable check is a compile + a live run, so this task's "test" is:

```rust
/// A compile-time guard that the inspector reads compressed pages through the
/// non-restoring accessor. If someone changes this call to `Node::page`, the
/// inspector silently decompresses everything it displays.
#[test]
fn inspector_pagelist_widget_uses_preserving_accessor() {
    let src = include_str!("pagelist.rs");
    assert!(
        src.contains("page_preserving_state"),
        "the pagelist widget must read compressed pages without restoring them"
    );
    assert!(
        !src.contains("Node::page("),
        "the pagelist widget must not restore compressed pages"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-inspector --lib inspector_pagelist_widget_uses_preserving`
Expected: FAIL — the string is absent.

- [ ] **Step 3: Write minimal implementation**

Port the final `oracle@main:src/inspector/widgets/pagelist.zig` memory section and `page.zig` hunk.

- [ ] **Step 4: Run test + live check**

```bash
cargo test -p rustty-inspector --lib
cargo build --workspace
```
Then, on macOS, launch `Rustty.app`, open the inspector (cmd-opt-i), generate scrollback, idle for a few seconds, and confirm the PageList panel shows a non-zero compressed page count and a positive estimated saving. Record the observation in the commit message.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-inspector/
git commit -m "inspector: clarify page compression memory in the pagelist widget (WS7 D6)"
```

---

### Task D7: `c-vt-compression` example

**Files:**
- Create: `examples/c-vt-compression/README.md`, `examples/c-vt-compression/main.c`, and whatever build glue the sibling C examples use (mirror `examples/` — grep for the existing `c-vt-*` example layout; `.port/phase-2/EXAMPLES_PORT.md` documents the convention).

**Oracle:** `172f15da3`'s `example/c-vt-compression/` (README 17, build.zig 32, build.zig.zon 14, `src/main.c` 72).

**Interfaces:**
- Consumes: `rustty_terminal_compression_activity`, `rustty_terminal_compress` from D5.
- Produces: a runnable C program demonstrating the incremental contract (poll activity, step until `Complete`).

- [ ] **Step 1: Write the failing test**

The example's test is that it builds and runs:

```bash
# expected to fail before the example exists
ls examples/c-vt-compression/main.c
```

- [ ] **Step 2: Verify it fails**

Expected: `No such file or directory`.

- [ ] **Step 3: Write the example**

Port `oracle@main:example/c-vt-compression/src/main.c` and `README.md` with R2 renaming, and add the build glue in the style of the existing C examples (the oracle's `build.zig`/`build.zig.zon` become whatever the rustty examples use — do **not** add Zig build files).

- [ ] **Step 4: Build and run it**

```bash
# using the same command the sibling c-vt examples document in
# .port/phase-2/EXAMPLES_PORT.md
```
Expected: the program prints a decreasing resident-memory / increasing compressed-page count and exits 0.

- [ ] **Step 5: Commit**

```bash
git add examples/c-vt-compression/ .port/phase-2/EXAMPLES_PORT.md
git commit -m "example: c-vt-compression demonstrating the incremental contract (WS7 D7)"
```

---

### Task D8: Compression benchmarks

**Files:**
- Create: `crates/rustty-bench/src/page_compression.rs`
- Create: `crates/rustty-bench/src/scrollback_compression.rs`
- Modify: `crates/rustty-bench/src/{lib.rs,cli.rs,main_bench.rs}`

**Oracle:** `oracle@main:src/benchmark/PageCompression.zig` (401+125+3 lines across `c62c15984`, `ebc3ffd22`, `461562ca4`) and `oracle@main:src/benchmark/ScrollbackCompression.zig` (299+116+32+33+7 lines across `9156ada16`, `70e788f06`, `e7969ed43`, `95685afd2`, `461562ca4`). Also read `oracle@main:src/benchmark/AGENTS.md` for the general workflow.

**Independently executable** once Phase B is in.

**Interfaces:**
- `+page-compression` with `--mode=compress|decompress|store|report` and `--data=<corpus>`; `report` prints the ratio.
- `+scrollback-compression` measures the PageList transitions around the codec rather than the codec itself.
- Both register through `Action::from_name` in `crates/rustty-bench/src/cli.rs` exactly like the existing eight actions (see `.port/phase-2/PERF_COMPARISON_PLAN.md` §1 for the CLI contract these must honor: `+`-prefixed subcommand, `--data=`, `--mode=`, print nothing on success).

- [ ] **Step 1: Write the failing test**

```rust
/// The two new actions are reachable by name, matching the oracle's CLI.
#[test]
fn bench_registers_compression_actions() {
    assert!(Action::from_name("page-compression").is_some());
    assert!(Action::from_name("scrollback-compression").is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustty-bench --lib bench_registers_compression_actions`
Expected: FAIL — `from_name` returns `None`.

- [ ] **Step 3: Write minimal implementation**

Port both benchmark modules and register the actions.

- [ ] **Step 4: Run them**

```bash
cargo build -p rustty-bench --release
./target/release/rustty-bench +page-compression --mode=report \
  --data=.port/phase-0/baseline/corpora/ascii.bin
./target/release/rustty-bench +scrollback-compression \
  --data=.port/phase-0/baseline/corpora/ascii.bin
```
Expected: `report` prints a ratio; the others exit 0 silently. **Record the ratio and the compress/decompress throughputs in `.port/phase-2/PERF_LOG.md`** — Phase E compares against them and the oracle's own numbers.

Note: this is the first execution path that exercises the **real** `madvise` arms of `mem::decommit` (release build ⇒ `cfg!(test)` is false). Run it on **both** platforms:
```bash
ssh rvm 'cd ~/projects/rustty && cargo build -p rustty-bench --release && ./target/release/rustty-bench +scrollback-compression --data=.port/phase-0/baseline/corpora/ascii.bin'
```

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-bench/ .port/phase-2/PERF_LOG.md
git commit -m "bench: page-compression and scrollback-compression actions (WS7 D8)"
```

**PHASE D COMPLETE.**

---

# PHASE E — Gates, ledgers, merge

### Task E1: Both-platform build gate

- [ ] **Step 1: macOS**

```bash
cd /Users/gael/projects/rustty
cargo build --workspace
```
Expected: `Finished`.

- [ ] **Step 2: Linux VM**

```bash
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout ws7-scrollback-compression && git pull && cargo build --workspace'
```
Expected: `Finished`. This leg is the one that compiles the `MADV_DONTNEED` arm and the non-Darwin `recommit` no-op.

- [ ] **Step 3: wasm target**

```bash
cargo build -p rustty-vt --target wasm32-unknown-unknown
```
Expected: `Finished` — the codec must stay portable (oracle `AGENTS.md` requirement). `mem::can_reclaim` returns `false` on wasm, so compression is simply disabled there.

- [ ] **Step 4: Acyclic + ABI gates**

```bash
bash .port/tools/gates/check_acyclic.sh
bash .port/tools/ci.sh
```
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit --allow-empty -m "vt: WS7 both-platform + wasm build gate green (WS7 E1)"
```

---

### Task E2: Miri lane on the (de)compression seams

**Files:**
- Create: `.port/phase-2/mirihar-compress/` (isolated non-FFI Miri harness, following `.port/phase-2/mirihar-chunk3b`)

**Why an isolated harness and not only the in-tree lane** (memory: `shaper-hook-sb-clean-interleave`): the compress/restore seam holds a raw `*mut Page` derived from `Node.data` while it *writes* `Node.data`. That is exactly the long-lived-raw-alias-plus-interleaved-`&mut` shape that must be **proven** on a minimal harness rather than hand-reasoned, because a full `PageList` test drags in enough machinery that a Miri failure is hard to localize.

- [ ] **Step 1: Build the isolated harness**

Create `.port/phase-2/mirihar-compress/` as a standalone cargo project (copy the `Cargo.toml` shape from `.port/phase-2/mirihar-chunk3b`) that reproduces, with a stub `Page`-shaped struct and a stub two-state node union, exactly these four interleavings:

1. `let p = Node::page(node); /* read through p */ node.data = Compressed(..); /* p is dead */`
2. `Node::page_preserving_state(node)` returning `Borrowed` while the node is later mutated.
3. `Node::restore(node, Preserve)` writing `node.data` while `compressed` was borrowed from it.
4. `compress_page`'s full sequence: `Node::page` → `CompressedPage::init(page as *const Page, ..)` → `mem::decommit` → `node.data = ...`.

Interleaving **3** is the sharpest: the current B6 body takes `let compressed: &CompressedPage = match &*data {...}` and then does `core::ptr::write(data, ...)` while that shared reference is conceptually live. If Miri flags it, the fix is to *copy out* the two things needed (`memory_base`/`memory_len`, and the decoded `Page`) and drop the borrow **before** the write — e.g. by reading through `core::ptr::addr_of!` and `core::ptr::read`/`write` rather than through a `&`.

- [ ] **Step 2: Run the harness**

```bash
cd .port/phase-2/mirihar-compress && cargo +nightly miri test
```
Expected: PASS with zero UB. If interleaving 3 reports UB, fix `Node::restore` in `crates/rustty-vt/src/pagelist/mod.rs` per the note above, re-run the harness, then re-run the vt lane.

- [ ] **Step 3: Run the in-tree compression lane**

```bash
bash .port/tools/miri-run.sh rustty-vt compress 6 45 15
bash .port/tools/miri-run.sh rustty-vt pagelist_compress 6 45 15
bash .port/tools/miri-run.sh rustty-vt lz4_ 4 20 10
```
Expected: zero `UB`. TIMEOUTs on the 400 KiB corpora are acceptable and go in `MIRI_EXCEPTIONS.md` with a reason; UB is not.

- [ ] **Step 4: Confirm the search lane did not regress**

```bash
bash .port/tools/miri-run.sh rustty-vt search 6 45 15
```
Expected: the UB set is **exactly** the 9 pre-existing `search/screen` names. Any new name is a WS7 regression.

- [ ] **Step 5: Commit**

```bash
git add .port/phase-2/mirihar-compress/ .port/phase-2/MIRI_EXCEPTIONS.md crates/rustty-vt/src/pagelist/mod.rs
git commit -m "vt: Miri harness + lanes over the compress/restore aliasing seams (WS7 E2)"
```

---

### Task E3: Perf non-regression gate

**Files:**
- Modify: `.port/phase-2/PERF_LOG.md`

**Baseline:** the 14 documented wins in `.port/phase-2/PERF_COMPARISON.md` (Apple M4 Pro, `Mac16,7`). The harness is bench-generic and drives the binary from `sys.argv[1]` (see `.port/phase-2/PERF_COMPARISON_PLAN.md` §0–§1).

**Why this can regress:** WS7 adds a branch to `Node::page` on **every** page access — the single hottest indirection in the terminal. If `Node::page` is not inlined to a predictable-branch load, `+terminal-parser` and `+screen-clone` will show it.

- [ ] **Step 1: Build the release bench binary**

```bash
cd /Users/gael/projects/rustty
cargo clean -p rustty-bench
cargo build -p rustty-bench --release
```

- [ ] **Step 2: Run the 14-config suite**

```bash
python3 .port/phase-0/baseline/run_benchmarks.py \
  /Users/gael/projects/rustty/target/release/rustty-bench \
  > /tmp/ws7-perf-after.json
```
Do **not** regenerate the corpora — reuse the exact bytes in `.port/phase-0/baseline/corpora/`.

- [ ] **Step 3: Compare against the recorded medians**

Compare each of the 14 configs' median against the Rust medians recorded in `.port/phase-2/PERF_COMPARISON.md`.

**Gate:** no config may regress by more than **3%** (the documented run-to-run noise band), and the suite total must not regress at all. If any config regresses beyond that band, re-run to rule out noise; if it holds:
- For `+terminal-parser` / `+screen-clone`: the cause is almost certainly `Node::page`'s new match. Confirm by checking whether the hot call site should be `Node::page_assume_resident` (B5's decision table) — an active-area write path always holds a resident page.
- Do **not** weaken the gate. Do **not** disable compression to pass it.

- [ ] **Step 4: Record the result**

Append a WS7 section to `.port/phase-2/PERF_LOG.md` with: the 14 medians before/after, the delta column, the machine, the date, and the D8 compression bench numbers (ratio + compress/decompress throughput) from Phase D.

- [ ] **Step 5: Commit**

```bash
git add .port/phase-2/PERF_LOG.md
git commit -m "perf: WS7 compression non-regression run — 14/14 within noise (WS7 E3)"
```

---

### Task E4: Strike the BL-1 ledger rows

**Files:**
- Modify: `.port/phase-2/MAIN_PARITY.md`
- Modify: `.port/phase-2/DEFERRALS.md`, `.port/phase-2/VERIFICATION_DEBT.md` (only if they carry BL-1 entries — grep first)
- Modify: `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` (tick WS7 in the plan-authoring checklist)

**What to change in `MAIN_PARITY.md`:**

1. **§2 backlog entry BL-1** — replace the entry body with a `CLOSED by WS7` note in the same style as the existing BL-5 entry: list what landed (LZ4 block codec, compressed page representation, retained-mapping reclamation, PageList compression policy, generation stamps + `node_is_valid`, the seven search/render consumers, the `Terminal`/lib-vt/config/renderer/inspector/bench surface) and name the new modules (`crates/rustty-vt/src/compress/`, `crates/rustty-vt/src/mem.rs`, `crates/rustty-vt/src/pagelist/compress.rs`).
2. **§3 per-commit table** — change the verdict cell for each of these **33** rows from `missing BL-1` to `**PORTED**` with a rustty-file evidence pointer and the ported test names:

   `16e4b5e98`, `c62c15984`, `ebc3ffd22`, `f6fd4cb08`, `421fe8dab`, `9156ada16`, `70e788f06`, `e7969ed43`, `0d015b2fc`, `f8c217e55`, `461562ca4`, `b9bb50c83`, `181254d36`, `95685afd2`, `0fb89f4ff`, `6d5dda40d`, `172f15da3`, `25e624569`, `d34b54e9b`, `9a4bd2120`, `d239f9805`, `5d8eb78b7`, `86e1f7b8b`, `73ac36fa5`, `f1a5fab45`, `a804e3ac7`, `91a63e528`, `83aada205`, `c243d89bd`, `a89c6133c`, `d0a26d146`, `dd956f37c`, `f6d9a582e`, `0bad91adb`, `89eca063e`, `c9ad708ef`

   (that list is 36 entries — the merge rows `7e02af879` and `53bd14fec` stay `n/a-merge`, and `4f53b846b` stays `n/a`).
3. **Note the BL-2 overlap for WS8:** add a line to the `896aca499` row saying its production behavior (pool-item `decommit(.zero, ...)` on free) landed with WS7 B4/B6 via `mem.rs`, and that WS8 owns only its benchmark/test hunks.

- [ ] **Step 1: Grep for every BL-1 mention**

```bash
grep -rn 'BL-1' .port/phase-2/ docs/superpowers/plans/
```

- [ ] **Step 2: Edit `MAIN_PARITY.md`** per the three items above.

- [ ] **Step 3: Edit the roadmap checklist**

In `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`, add `- [x] WS7 scrollback compression (research note: BL-1's LZ4 is a pure-source port — ghostty vendors NO C lz4; no lz4-sys crate was created)` to the plan-authoring checklist.

- [ ] **Step 4: Verify no stale claim remains**

```bash
grep -rn 'missing BL-1' .port/phase-2/MAIN_PARITY.md
```
Expected: **no output**.

- [ ] **Step 5: Commit**

```bash
git add .port/phase-2/ docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md
git commit -m "parity: mark BL-1 scrollback-compression series ported (WS7 E4)"
```

---

### Task E5: Live verification in Rustty.app

- [ ] **Step 1: Build and launch**

```bash
cd /Users/gael/projects/rustty
./build-rustty.sh
open ./Rustty.app   # or the path build-rustty.sh reports
```

- [ ] **Step 2: Generate scrollback and idle**

In the terminal, run something that produces tens of thousands of lines (e.g. `yes "compressible terminal history" | head -200000`), then leave the window idle for ~10 seconds.

- [ ] **Step 3: Confirm compression happened**

Open the inspector (cmd-opt-i) → PageList panel. Expected: a non-zero **compressed pages** count and a positive **estimated savings**. Cross-check with `footprint` or Activity Monitor: the process's physical footprint should be materially lower than the raw page bytes the panel reports.

- [ ] **Step 4: Confirm reading it back is transparent**

Scroll to the top of the scrollback and search (the search UI) for a string only present in early history. Expected: the match is found, the viewport renders correctly, and the inspector still shows compressed pages afterwards (search must not have decompressed everything — that is C5's contract, verified live).

- [ ] **Step 5: Repeat on Linux**

```bash
ssh rvm 'cd ~/projects/rustty && cargo build --release'
```
and run the GTK app on the VM's session; confirm the same via the GTK inspector.

Record both observations in the commit message.

```bash
git commit --allow-empty -m "vt: WS7 live verification — compression active in Rustty.app (macOS + GTK) (WS7 E5)"
```

---

### Task E6: Merge

**Per the roadmap's sequencing rule 2:** branch → task loop → both-platform build gate → ledger update → merge to main → delete branch.

- [ ] **Step 1: Final full test sweep**

```bash
cargo test --workspace 2>&1 | tail -30
ssh rvm 'cd ~/projects/rustty && cargo test --workspace 2>&1 | tail -30'
```
Expected: zero failures on both.

- [ ] **Step 2: Request review**

Use `superpowers:requesting-code-review` over the full branch diff. Focus the reviewer on: (a) the `Node::page` vs `page_assume_resident` decisions from B5, (b) the `compress_page` failure-ordering in B7, (c) `mem::decommit`'s `cfg(test)` simulation, (d) the E3 perf numbers.

- [ ] **Step 3: Merge**

```bash
git checkout main
git merge --no-ff ws7-scrollback-compression -m "merge: WS7 scrollback page compression (BL-1)"
```

- [ ] **Step 4: Post-merge gate**

```bash
cargo build --workspace && cargo test --workspace 2>&1 | tail -5
ssh rvm 'cd ~/projects/rustty && git pull && cargo build --workspace'
```

- [ ] **Step 5: Delete the branch**

```bash
git branch -d ws7-scrollback-compression
```

**WS7 COMPLETE.**

---

## Self-Review

Run after the plan is written; findings below are already fixed inline.

**1. Spec coverage.** Every element of the WS7 brief maps to a task:

| Brief requirement | Task |
|---|---|
| LZ4 codec | A1–A5 |
| Compressed `Page` representation | B3 |
| Generation stamps | C1, C2 |
| Search-integrity integration | C5, C6, C7 |
| MAIN_PARITY BL-1 commit hashes extracted | Phase B / Phase C commit tables (36 hashes) |
| Oracle `origin/main` read protocol stated | Global Constraints ("Oracle read protocol"), every `oracle@main:` path |
| `Backing::alloc/free` convention followed for new OS interaction | B1 (`extern "C" madvise`, no `libc` crate) |
| lz4-sys question answered | Headline finding + Global Constraints: **not needed** |
| Miri harnesses planned | E2 (`.port/phase-2/mirihar-compress`) + lanes in A6, B10, C8, E2 |
| Perf non-regression check | E3 (`run_benchmarks.py`, 14 configs, 3% band) + D8 (compression benches) |
| Phases with explicit boundaries + order | "Phase Structure & Execution Order" table |
| Which phases are separately executable | Same table: A yes; D4/D8 yes; B/C/E no |
| Both-platform build gate | A6 S3, B10 S2, C8 S2, D4 S4, E1 |
| Branch/merge flow per roadmap | Branch created before A1; E6 |

**2. Placeholder scan.** Three residual soft spots, each with a stated resolution rather than a TODO:
- D3's second test and D6's live check depend on harnesses whose exact shape must be read from the existing files. Each names the file to grep and states what to do if the harness is absent (fall back to the named alternative gate) — not "write an appropriate test".
- C3, C6, C7 say "port the commit's test hunks verbatim" **and** give the exact oracle test names and the target rustty test-fn names, so the implementer has an unambiguous, enumerable checklist rather than prose.
- B7/B8 give signature skeletons with per-function oracle **line ranges** rather than inlined 300-line bodies. This is deliberate: reproducing the oracle's `compress_page` failure ordering by transcription is more reliable than by paraphrase, and the ordering is spelled out step-by-step in B7 Step 3.

**3. Type consistency** (checked across phases):
- `CompressedPage` — introduced B3, used B6/B7, test-constructed in B6's tests. Consistent.
- `required_scratch` — B3 free function in `compress::page`, re-exported from `compress`; called as `crate::compress::required_scratch` in B6/B7. Consistent.
- `Node::page` / `page_if_resident` / `page_assume_resident` / `metadata` / `rows` / `cols` / `capacity` — declared B5, bodies replaced B6, used B7/B9/C/D. Same names throughout.
- `Storage::{Resident, Compressed}` — B6, asserted in B6/B7/B8/C5 tests. Consistent.
- `CompressionMode::{Incremental, Drain, Full}` (PageList) vs `TerminalCompressionMode::{Incremental, Full}` (Terminal, `#[repr(C)]`) — **deliberately different types**: the oracle likewise has an internal three-mode `compress` and a two-mode ABI enum. Called out here so a reader does not "fix" the mismatch.
- `IncrementalCompressionResult::{Unsupported, Pending, Complete}` (PageList) vs `TerminalCompressionResult` (`#[repr(C)]`, same three variants) — same deliberate split.
- `page_serial_epoch` — renamed in C1; every later reference (C2 `node_is_valid`, C3) uses the new name; no `page_serial_min` survives after C1.
- `mem::{DecommitMode, can_reclaim, decommit, recommit}` — B1, used B4/B6/B7 and by `compression_enabled()`.
- `MemoryStats` field names are identical between B9's definition and C5's `memory_stats()` equality assertion.
- `AppendResult { content_len, last_row_wrapped }` — C5 only. Consistent.

**Gap found and fixed during review:** B4's `destroy_node_ext` originally kept the length-based pool/heap inference in its `total_size` accounting; corrected to switch on `Owned` for both the free path *and* the byte accounting, with an explicit Step 3 instruction to audit every `page_size +=` site. Without that, `memory_stats` (B9) and `PageList::page_size` would disagree for compacted heap pages.
