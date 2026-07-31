# vt Miri lane findings (P3, run 2026-07-30 20:30, tree f35ac06 = W0+W1+W2+W7)

## F1. `compress::page::tests::compressed_page_retained_mapping_round_trip` — REAL UB, PRE-EXISTING, MISSED BY THE BASELINE

```
error: Undefined Behavior: constructing invalid value of type [u8; 4]: at [3],
       encountered uninitialized memory, but expected an integer
  compress::lz4::read_u32           crates/rustty-vt/src/compress/lz4.rs:710
  compress::lz4::compress           crates/rustty-vt/src/compress/lz4.rs:170
  compress::page::CompressedPage::init  crates/rustty-vt/src/compress/page.rs:137
```

**Not a W2 regression — verified.** Reproduced on `f312826` (the pre-W2 merge base)
in a throwaway worktree: identical error. W2 changed page layout, but this predates it.

**It is a BASELINE GAP, and that is the more important half of this finding.**
`.port/phase-2/VT_MIRI_SWEEP.md` records a 9-test known-red set and contains ZERO
`compress::` rows (`grep -c compressed_page_retained_mapping_round_trip` = 0). So
the "known-red set" was never the truth about this crate — it was the truth about
the subset previously swept. Any future run comparing against it would have called
this a NEW regression. The baseline needs to be re-derived from a completed lane,
not extended by hand.

**Root cause (the established class in this repo).** `page_alloc::alloc` uses
`std::alloc::alloc_zeroed` (`page.rs:342`), so the backing starts fully
initialized. But a TYPED write of a struct that has PADDING re-poisons the padding
bytes under Miri, and `CompressedPage::init` then builds a `&[u8]` over the whole
mapping (`page.rs:137`) and hands it to the LZ4 compressor, which reads 4 bytes at
a time. Reading a padding byte as `u8` is UB in Rust; the same read is legal in
Zig, so R1 fidelity does not carry over here.

**This is the SAME class WS19 T5 already fixed once** — "make `style::Color`
padding-free so page mappings are fully initialized" (`7bda552`). The fix pattern
is therefore established: identify the struct(s) whose padding lands inside the
compressed range and make them padding-free, rather than weakening the check.

**NOT sound as a fix, for the record:** `read_volatile` per byte (still yields an
uninit `u8`), copying to a zeroed buffer first (memcpy preserves uninit-ness), or
`-Zmiri-disable-uninit-checks` (hides it rather than fixing it).

**Owner:** dedicated lane, see the P3 tracker.
