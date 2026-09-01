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

### F1 RESOLVED (branch `p3-ub-fixes`)

**Confirmed root cause — two padded types, not one.** The hypothesis above was
right in class but the specific culprits had to be located, not guessed. Method:
(a) a differential padding probe (write a value into a `0x00`-filled and a
`0xFF`-filled heap buffer of the same layout and diff the bytes) over every type
stored in page memory; (b) a byte-by-byte scan of the failing test's mapping
under Miri, which aborts on the FIRST uninit byte and so reports its exact
offset. Findings:

1. `size::OffsetSlice<T>` (`rustty-render-types/src/term_size.rs`) — `offset:
   u32` beside `len: usize` is 16 bytes with **4 tail-padding bytes**. It is
   stored in the mapping as `hyperlink::PageEntry.uri`, inside
   `PageEntryId::Explicit`, and as the grapheme map's values. This is the
   `read_u32` / `[u8; 4]` abort quoted above.
2. `hyperlink::PageEntryId` — a plain Rust `enum`, 24 bytes, tag at byte 0,
   payload at 8..24: **7 bytes of discriminant padding at 1..8** (both variants)
   plus 12 unused payload bytes at 12..24 for `Implicit`. Fixing (1) alone left
   the test failing with a *different* abort (`read_u64` / `[u8; 8]`); the byte
   scan put the first uninit byte at mapping offset **+4452**, which is
   `hyperlink_set` (starts +4392) item 1 (`items_start` 8, item stride 48) at
   intra-`PageEntryId` byte 4 — i.e. exactly the discriminant padding of the
   live hyperlink entry written by `insert_hyperlink`.

Note the dead entries of the set were already safe: `RefCountedSet::init`
zero-fills the item array and writes `meta` field-wise (WS7 Phase E). Only the
LIVE typed `PageEntry` write re-poisoned.

**Fix (WS19 T5 precedent, applied twice).** Both types are now byte-complete
`#[repr(C)]` values with named padding: `OffsetSlice { offset, _padding: u32,
len }` (16B/align 8, unchanged size) and `PageEntryId { explicit: OffsetSlice<u8>,
implicit: OffsetInt, tag: u8, _padding: [u8; 3] }` (24B/align 8, unchanged size),
with `Explicit`/`Implicit` constructors named like the former variants, a
`PageEntryIdTag` returned by `tag()`, and tag-checked `explicit()`/`implicit()`
accessors — the same shape `style::Color` already uses. `const _: () = assert!`
pins both sizes/aligns. **No page geometry changed**, so
`tests/std_size_golden.rs` is untouched and still passes.

**R1 divergence, justified and recorded:** the `PageEntryId` enum→struct reshape
is a representation change only. The oracle's `union(enum) { explicit:
Offset(u8).Slice, implicit: size.OffsetInt }` has the same 24-byte size and the
same undefined bytes; Zig permits reading undefined bytes and Rust does not, so
R1 cannot carry the enum representation over. Value semantics, size and align are
identical. Full reasoning is in the PORT NOTEs on `hyperlink::PageEntryId` and
`size::OffsetSlice`.

**Revert-proof.** Removing `OffsetSlice::_padding` alone reproduces the original
`[u8; 4]` abort; removing `PageEntryId::_padding` alone reproduces the `[u8; 8]`
abort. Both restored → test clean.

## F2. `screen::tests::screen_hyperlink_accepts_its_current_values` — RESOLVED (branch `p3-ub-fixes`)

```
error: Undefined Behavior: not granting access to tag <N> because that would remove
       [SharedReadOnly for <M>] which is strongly protected
  <M> is this argument: screen.rs  `id_: Option<&[u8]>`
  <N> created at:       hyperlink.rs:101  HyperlinkId::Explicit(v.clone())
  drop_in_place::<[u8]> <- <Vec<u8> as Drop>::drop <- Screen::end_hyperlink
```

**Confirmed root cause.** `Screen::start_hyperlink` took `uri: &[u8]` /
`id_: Option<&[u8]>`. A reference-typed argument is STRONGLY PROTECTED for the
whole call. The call reaches `start_hyperlink_once` → `end_hyperlink`, which
drops the `Box<Hyperlink>` holding the cursor's current URI/ID — and the oracle's
`startHyperlinkOnce` explicitly documents that "the source slices are allowed to
reference our current hyperlink", with a dedicated test for it. Freeing a
strongly-protected allocation is UB regardless of whether the reference is ever
read again, so the contract is simply not expressible with `&[u8]` parameters.
Copying the arguments earlier does not help: the protector covers the whole
frame, not the last use.

**Fix.** The implementation moved to
`pub unsafe fn Screen::start_hyperlink_raw(&mut self, uri: *const [u8], id_:
Option<*const [u8]>)`. Raw pointers carry no protector — exactly Zig's slice
parameter semantics — so the aliasing contract ports faithfully. The same class
as the `&mut Page` / `&mut [Cell]` → raw-pointer argument conversions already in
this crate. `start_hyperlink(&[u8], Option<&[u8]>)` is kept as the safe wrapper
for the ~50 non-aliasing callers; it is sound because `cursor.hyperlink` is an
`Option<*mut Hyperlink>`, so no safe code can obtain a slice into the allocation
the call releases. The ported test keeps the aliasing and goes through the raw
entry point.

**Revert-proof.** Restoring the test to `&[u8]` + `start_hyperlink` reproduces
the identical strongly-protected abort; restored → clean.


## G6 / G6c — the 2026-07-31 CI triage (first full `ci.sh` on merged `main`)

**G6c: re-measured GREEN on current `main`, no change made.**
`check_leaks.sh --debug-assertions-off` → `test result: ok. 82 passed; 0 failed;
0 ignored; 2239 filtered out; finished in 2690.31s`, i.e. the vacuity floor (82,
derived from the `cfg_attr(all(miri, debug_assertions))` annotations) is met
exactly, so `-C debug-assertions=off` really reached rustc. The ci.sh RED predates
the adversarial MIRI-EXCEPTION re-vet and the xev Stacked-Borrows fix and does not
reproduce.

**Measurement hazard worth recording.** The first attempt at this lane was heading
for the gate's 5400s hard bound — 90 minutes in, 11 of 82 tests done — purely
because three other Miri lanes were running on the same box. Given the machine to
itself it finished in 45 minutes. Two consequences: (1) G6c needs an
otherwise-idle host or it flaps on the bound and reports "the lane did not
finish"; (2) killed Miri runs ORPHAN their `miri` child, which reparents to PID 1
and keeps burning a core — three such orphans were found and killed during this
triage. Always `killtree`, then re-check `ps`, before trusting a timing.

**G6: the lane could not previously reach most crates.** Miri aborts the WHOLE
test binary at the first foreign call, so a single test took each crate down with
it: `dladdr` (rustty-bench), `os_log_create` (rustty-core), `inflateInit2_`
(rustty-cli), `objc_autoreleasePoolPush` (rustty-os), plus rustty-extra never
terminating on an un-annotated doc-generation test. Fixed per
`.port/phase-2/MIRI_EXCEPTIONS.md` (`cfg(miri)` no-ops for the two instrumentation
SINKS; measured per-test exceptions for the 6 tests that genuinely cross a
boundary). Verified after the fix: rustty-bench 16 passed/3 ignored,
rustty-os 42/23, rustty-cli 54/16.

**Still open at hand-off:** `rustty-core`'s
`renderer_seam::compression_tests::change_config_restarts_the_pass_only_on_disabled_to_enabled`
ran for 35+ minutes under Miri without completing (twice) and was killed, so
rustty-core's lane has no verdict yet — it is a candidate for the
allocation-volume exception class, but that must be MEASURED with a bound
(`.port/tools/miri-run.sh`) before anything is annotated. Crates after
`rustty-extra` in the lane order were never reached by any run in this triage and
remain unverified.

---

# G6 lane completion (2026-08-29) — two REAL findings

The G6 section above ended with most crates unreached. Working that frontier to a
verdict produced two findings that are code defects, not Miri artifacts. Both are
fixed; the Miri-cannot-model cases found alongside them are argued in
`.port/phase-2/MIRI_EXCEPTIONS.md` ("G6 lane completion").

## F5. `rustty-args` diagnostics — REAL Stacked-Borrows UB, and it was in PRODUCTION code

```
error: Undefined Behavior: attempting a read access using <1748069> at
       alloc519900[0x1e], but that tag does not exist in the borrow stack
  core::ffi::c_str::strlen              (CStr::from_ptr)
  diagnostics::tests::precomputed_message_is_nul_terminated_but_excludes_the_sentinel
help: <1748069> was created by a SharedReadOnly retag at offsets [0x0..0x1e]
  crates/rustty-args/src/diagnostics.rs:410
```

**Not a test-only defect.** `DiagnosticList::precompute_message` returns the
reference's `[:0]const u8` VIEW — the formatted bytes with the
`toOwnedSliceSentinel(0)` NUL *excluded* from the length. `c_api.rs:247`
(`rustty_config_get_diagnostic`) then handed `message.as_ptr()` to C. A pointer
derived from that view carries provenance for exactly `len` bytes, and every C
reader walks `len + 1` to find the NUL. The bytes were right; the provenance was
not — so no byte-comparison test could see it, and only the test that performed
the same read in Rust (`CStr::from_ptr`) exposed it.

This is the shape the D-detectors exist for: a divergence with no compile error,
no failing assertion, and no wrong output. The reference gets it right for free —
Zig's `[:0]const u8` `.ptr` is derived from the sentinel-carrying allocation, not
from a length-truncated reborrow of it.

**Fix.** A second accessor, `precompute_message_c`, returning the WHOLE stored
buffer (sentinel included); `c_api.rs` uses it. `precompute_message` keeps its
reference-faithful view for Rust callers and now documents why it must not be the
C source. The test asserts through the C accessor, so it pins the real contract
instead of reproducing the bug. Nothing about the bytes C sees changes.

**Verified:** `cargo +nightly miri test -p rustty-args --lib` → 69 passed, 0
failed (was: aborted at the first error).

## F6. `rustty-lib` packed — an assertion on `const` ADDRESS identity

`packed::tests::packed_reflected_fields_and_nested_tagged_union` asserted
`core::ptr::eq(layout, &NUMBER)`. The reference's claim is
`ArmLayout == Number`, a comptime TYPE identity; over rustty's data model the
layouts are values, and a `const` item is inlined at each use site — the `&NUMBER`
in the arm table and the `&NUMBER` in the assertion are two independently
promoted allocations that need not share an address. rustc folds them together,
Miri does not, so the assertion was pinning a codegen coincidence.

**Fix.** `PartialEq`/`Eq` on the layout descriptors, and the assertion compares by
VALUE (plus an explicit `assert_ne!` against `FLAGS`, which is what the
reference's `else => TestUnexpectedResult` arm rules out). Equality binds strictly
more than address identity here and holds under any promotion strategy.

**Verified:** `cargo +nightly miri test -p rustty-lib --lib` → 27 passed, 0 failed.

## F7. `rustty-z2d` `Face::intersect` — a latent divide-by-zero, RECORDED not fixed

Surfaced by (not caused by) Miri's libm: see `MIRI_EXCEPTIONS.md` for the ULP
evidence. `Face::intersect` (`tess/face.rs:138`) computes the miter join as

```rust
… / (in_slope.dx * out_slope.dy - out_slope.dx * in_slope.dy)
```

guarded only by `debug_assert!(in_.dev_slope.compare(out.dev_slope) != 0)`. Two
slopes can compare as DIFFERENT while their normalized cross product rounds to
exactly zero, and the division is then 0/0 = NaN. Miri's `hypot` produces exactly
such a pair (2 ULP apart in each component) for the input
`slope_difference_below_epsilon_does_not_produce_nan` already pins.

**Why it is not fixed here.** The miter math is z2d 0.12.1's, itself derived from
Cairo's `cairo-path-stroke-polygon.c`; changing its numerics would be a deliberate
divergence from the vendored reference (R1), and the safe predicate ("the
determinant is non-zero") belongs upstream rather than as a local patch. It is
recorded so that the next platform whose libm rounds differently finds this entry
instead of rediscovering a NaN.

**Reopen trigger:** the assertion fires natively on any host, or z2d upstream
changes the guard.
