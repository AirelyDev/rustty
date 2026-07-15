# completeness-wuffs — the 6 wuffs pixel-swizzle seams

Status: **PLAN ONLY** (this workstream is a spec; no code changed). Un-defers
`DEFERRALS.md` §"rustty-wuffs swizzle — 5 unreached pixel-format seams"
(lines 3741-3761), which explicitly names "a completeness pass on the wuffs
vendored subset (GAP_ANALYSIS §7 open decision 4)" as its own un-defer
trigger — this workstream *is* that trigger.

## Oracle

- `/Users/gael/projects/ghostty/pkg/wuffs/src/swizzle.zig` (121 lines, read in
  full). No `test{}` blocks in the oracle file itself — the KATs below are
  new, not ported reference assertions.
- Sole reference caller: `/Users/gael/projects/ghostty/src/renderer/image.zig`
  `Image.convert` (lines ~845-864), which dispatches `.gray/.gray_alpha/.rgb/
  .bgr/.bgra` pending-image pixel formats through the five `*ToRgba` wrappers
  before GPU upload. (Not in scope to port here — rustty's renderer already
  re-homed this as a pure-Rust loop per `RENDERER_KITTY_IMAGE_PATH_DESIGN.md`;
  this workstream only fills `rustty-wuffs::swizzle` itself.)

## Current rustty state

- `crates/rustty-wuffs/src/swizzle.rs` (121 lines) — all 5 public fns
  (`gToRgba`, `gaToRgba`, `rgbToRgba`, `bgrToRgba`, `bgraToRgba`) plus the
  private `swizzle` helper are `unimplemented!()` with the verbatim reference
  body preserved in comments. Signatures are already correct
  (`fn(alloc: A, src: &[u8]) -> Result<RawBuf<A>, Error>` for `A: Allocator +
  Copy`) — filling the seams requires **no signature change**.
- `crates-sys/wuffs-sys` already vendors wuffs 0.4.0-alpha.9 and links it; the
  bindgen allowlist (`wuffs_.*` functions/types, `WUFFS_.*` vars,
  `rustty_wuffs_.*` shim) is broad enough that **most of the swizzler surface
  is already bound** — verified below by building the crate and grepping the
  generated `OUT_DIR/bindings.rs`, not by inspection alone.

## Investigation: what's already exposed vs. what needs a new shim

Built `cargo build -p wuffs-sys` and inspected
`target/debug/build/wuffs-sys-*/out/bindings.rs` directly.

**Already bound (zero build.rs/header changes needed):**
- `wuffs_base__pixel_swizzler__prepare` and
  `wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice` — these are
  `WUFFS_BASE__MAYBE_STATIC` (expands to nothing unless
  `WUFFS_CONFIG__STATIC_FUNCTIONS` is defined, which build.rs does not
  define), i.e. **ordinary external-linkage C functions**, not
  `static inline`. Their prototypes sit inside
  `#if !defined(WUFFS_CONFIG__MODULES) || defined(WUFFS_CONFIG__MODULE__BASE)
  || defined(WUFFS_CONFIG__MODULE__BASE__PIXCONV)` (wuffs-v0.4.c:23098) —
  already satisfied by the existing `WUFFS_CONFIG__MODULE__BASE` define in
  `MODULE_DEFINES`. Confirmed present in `bindings.rs:621,631`.
- Types: `wuffs_base__pixel_format` (`{ pub repr: u32 }` — trivial,
  bindings.rs:478-481), `wuffs_base__pixel_swizzler` (opaque
  `{ private_impl: {func, transparent_black_func, u32, u32} }`,
  bindings.rs:608-619), `wuffs_base__pixel_blend` (`= u8`, bindings.rs:473),
  `wuffs_base__slice_u8` (already used by png.rs/jpeg.rs).
- Constants: `WUFFS_BASE__PIXEL_FORMAT__Y`, `__YA_PREMUL`, `__RGB`, `__BGR`,
  `__BGRA_PREMUL`, `__RGBA_PREMUL` — all plain `#define NUM` macros, all
  already emitted as `pub const … : u32` (bindings.rs:173-198, confirmed by
  grep for each name).
- `WUFFS_BASE__PIXEL_BLEND__SRC` is **not** bound — it's a function-style cast
  macro (`#define WUFFS_BASE__PIXEL_BLEND__SRC ((wuffs_base__pixel_blend)0)`,
  wuffs-v0.4.c:4387) that bindgen cannot parse as a value. This is the *exact*
  situation png.rs already documents and works around: pass the literal `0u8`
  (`wuffs_base__pixel_blend` is a `u8` typedef) — same PORT NOTE, reused
  verbatim.

**NOT bound — genuinely `static inline`, need a new shim (like the 5 helpers
`shim.h`/`shim.c` already wrap for png.rs/jpeg.rs):**
- `wuffs_base__pixel_format__bits_per_pixel` — **load-bearing**: its result
  sizes the destination allocation (`dst_size = bits_per_pixel/8`). Its body
  indexes the **private** extern table
  `wuffs_private_impl__pixel_format__bits_per_channel[16]`
  (wuffs-v0.4.c:4509), which is not itself exposed — reimplementing the bit
  math in Rust would require duplicating that private table (real risk of
  silent drift from a future wuffs bump). **Must shim**, not hand-replicate.
- `wuffs_base__pixel_format__is_direct` / `..__is_interleaved` — used only in
  `assert()` sanity checks (not load-bearing for the returned bytes), each a
  1-line bitwise read of `repr`. These *could* be hand-replicated in Rust
  (mirroring the existing KAT precedent of reading `status.repr` directly
  instead of calling `wuffs_base__status__is_ok`, and hand-constructing
  `wuffs_base__slice_u8{ptr,len}` instead of calling `make_slice_u8`) — but
  recommend shimming these too, for three reasons: (1) uniformity with the
  `bits_per_pixel` shim that's mandatory anyway, (2) zero risk of the repr
  bit-layout comment going stale, (3) the marginal cost is two more 1-line
  wrapper functions using the SAME established shim pattern.
  **Decision (flag if reviewer disagrees): shim all three.**
- `wuffs_base__make_pixel_format` / `make_slice_u8` / `empty_slice_u8` — all
  are one-field-copy struct constructors. Per the established KAT/png.rs
  precedent, construct these by hand in `swizzle.rs` (`wuffs_base__pixel_format
  { repr: FMT }`, `wuffs_base__slice_u8 { ptr, len }`,
  `wuffs_base__slice_u8 { ptr: null_mut(), len: 0 }` for the empty-palette
  slices) — **no shim needed**, matches how png.rs already avoids
  `make_slice_u8`.
- `wuffs_base__status__is_ok` — already solved crate-wide by
  `crate::error::check(&status)` (reads `status.repr.is_null()` directly).
  **Reuse it**, see fidelity note below.

Net new FFI surface: **3 new shim functions**, all in the same
`rustty_wuffs_*` family, all already covered by the existing
`allowlist_function("rustty_wuffs_.*")` — **build.rs needs zero edits**.

## Work items

1. **`crates-sys/wuffs-sys/shim.h`** — add 3 prototypes (append after the
   existing 4):
   ```c
   /* wuffs_base__pixel_format__bits_per_pixel — bits/pixel (0 if planar). */
   uint32_t rustty_wuffs_pixel_format__bits_per_pixel(const wuffs_base__pixel_format* f);
   /* wuffs_base__pixel_format__is_direct — true iff not an indexed/palette format. */
   bool rustty_wuffs_pixel_format__is_direct(const wuffs_base__pixel_format* f);
   /* wuffs_base__pixel_format__is_interleaved — true iff single-plane. */
   bool rustty_wuffs_pixel_format__is_interleaved(const wuffs_base__pixel_format* f);
   ```
2. **`crates-sys/wuffs-sys/shim.c`** — add the 3 matching one-line forwarding
   definitions (exact mirror of the existing `rustty_wuffs_pixel_config__width`
   pattern).
3. **`crates-sys/wuffs-sys/build.rs`** — **no change** (allowlist already
   matches `rustty_wuffs_.*`; confirm by rebuilding and re-grepping
   `bindings.rs` for the 3 new symbols after the shim edit).
4. **`crates-sys/wuffs-sys/src/lib.rs`** `kat` module — add 1 new test
   proving the 3 new shims link + return correct values, mirroring the
   existing `adler32_abc_known_answer`/`crc32_abc_known_answer` KATs:
   ```rust
   #[test]
   fn pixel_format_rgba_premul_shims() {
       let fmt = wuffs_base__pixel_format { repr: WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL };
       unsafe {
           assert!(rustty_wuffs_pixel_format__is_direct(&fmt));
           assert!(rustty_wuffs_pixel_format__is_interleaved(&fmt));
           assert_eq!(rustty_wuffs_pixel_format__bits_per_pixel(&fmt), 32);
       }
   }
   ```
5. **`crates/rustty-wuffs/src/swizzle.rs`** — fill all 6 seams. Add the
   `use wuffs_sys::{ wuffs_base__pixel_format, wuffs_base__pixel_swizzler,
   wuffs_base__pixel_swizzler__prepare,
   wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice,
   wuffs_base__slice_u8, rustty_wuffs_pixel_format__bits_per_pixel,
   rustty_wuffs_pixel_format__is_direct,
   rustty_wuffs_pixel_format__is_interleaved, WUFFS_BASE__PIXEL_FORMAT__Y,
   WUFFS_BASE__PIXEL_FORMAT__YA_PREMUL, WUFFS_BASE__PIXEL_FORMAT__RGB,
   WUFFS_BASE__PIXEL_FORMAT__BGR, WUFFS_BASE__PIXEL_FORMAT__BGRA_PREMUL,
   WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL };` import block; `use
   crate::error::check;` (currently only `crate::{Error, RawBuf}` is
   imported).

   Private `swizzle` helper body (replaces the `unimplemented!()` +
   preserves every reference comment line-for-line above the code, same
   convention as png.rs/jpeg.rs):
   ```rust
   fn swizzle<A: Allocator + Copy>(
       alloc: A,
       src: &[u8],
       src_pixel_format: u32,
       dst_pixel_format: u32,
   ) -> Result<RawBuf<A>, Error> {
       // const src_slice = c.wuffs_base__make_slice_u8(@constCast(src.ptr), src.len);
       let src_slice = wuffs_base__slice_u8 { ptr: src.as_ptr() as *mut u8, len: src.len() };

       // const dst_fmt = c.wuffs_base__make_pixel_format(dst_pixel_format);
       let dst_fmt = wuffs_base__pixel_format { repr: dst_pixel_format };
       // assert(c.wuffs_base__pixel_format__is_direct(&dst_fmt));
       // assert(c.wuffs_base__pixel_format__is_interleaved(&dst_fmt));
       // assert(c.wuffs_base__pixel_format__bits_per_pixel(&dst_fmt) % 8 == 0);
       // SAFETY: `dst_fmt` is a valid, fully-initialized 1-field POD; the shims
       // are pure accessors (no aliasing, no retained pointer).
       unsafe {
           assert!(rustty_wuffs_pixel_format__is_direct(&dst_fmt));
           assert!(rustty_wuffs_pixel_format__is_interleaved(&dst_fmt));
       }
       let dst_bpp = unsafe { rustty_wuffs_pixel_format__bits_per_pixel(&dst_fmt) };
       assert!(dst_bpp % 8 == 0);
       // const dst_size = c.wuffs_base__pixel_format__bits_per_pixel(&dst_fmt) / 8;
       let dst_size = (dst_bpp / 8) as usize;

       // const src_fmt = c.wuffs_base__make_pixel_format(src_pixel_format);
       let src_fmt = wuffs_base__pixel_format { repr: src_pixel_format };
       // assert(is_direct) / assert(is_interleaved) / assert(bpp % 8 == 0) — same shape.
       unsafe {
           assert!(rustty_wuffs_pixel_format__is_direct(&src_fmt));
           assert!(rustty_wuffs_pixel_format__is_interleaved(&src_fmt));
       }
       let src_bpp = unsafe { rustty_wuffs_pixel_format__bits_per_pixel(&src_fmt) };
       assert!(src_bpp % 8 == 0);
       // const src_size = c.wuffs_base__pixel_format__bits_per_pixel(&src_fmt) / 8;
       let src_size = (src_bpp / 8) as usize;

       // assert(src.len % src_size == 0);
       assert!(src.len() % src_size == 0);

       // const dst = try alloc.alloc(u8, src.len * dst_size / src_size);
       // errdefer alloc.free(dst);
       //
       // PORT NOTE: `errdefer` is subsumed by `RawBuf`'s `Drop` (png.rs precedent).
       let dst = RawBuf::alloc(alloc, src.len() * dst_size / src_size)?;

       // const dst_slice = c.wuffs_base__make_slice_u8(dst.ptr, dst.len);
       let dst_slice = wuffs_base__slice_u8 { ptr: dst.ptr(), len: dst.len() };

       // var swizzler: c.wuffs_base__pixel_swizzler = undefined;
       // SAFETY: `undefined` -> zeroed POD; `prepare` fully initializes `private_impl`
       // (its `func`/`transparent_black_func` are `Option<extern "C" fn>`, whose
       // Rust-guaranteed null-pointer-optimized `None` bit-pattern is all-zero).
       let mut swizzler: wuffs_base__pixel_swizzler = unsafe { core::mem::zeroed() };
       {
           // const status = c.wuffs_base__pixel_swizzler__prepare(&swizzler, dst_fmt,
           //     c.wuffs_base__empty_slice_u8(), src_fmt, c.wuffs_base__empty_slice_u8(),
           //     c.WUFFS_BASE__PIXEL_BLEND__SRC);
           let empty = wuffs_base__slice_u8 { ptr: core::ptr::null_mut(), len: 0 };
           // SAFETY: `swizzler` is a valid, owned, exclusively-borrowed local;
           // `dst_fmt`/`src_fmt` are plain PODs; `empty` has a null ptr + 0 len (the
           // C side never dereferences ptr when len == 0).
           let status = unsafe {
               wuffs_base__pixel_swizzler__prepare(
                   &mut swizzler, dst_fmt, empty, src_fmt, empty, 0, // WUFFS_BASE__PIXEL_BLEND__SRC
               )
           };
           // if (!is_ok(status)) { log.warn(...); return error.WuffsError; }
           //
           // PORT NOTE: the reference re-derives this exact check inline instead of
           // calling error.zig's `check` (unlike png.zig/jpeg.zig, which do call it);
           // reusing `crate::error::check` here is the DRY, already-established
           // choice (same fold-the-log-scope-into-the-message precedent as
           // png.rs/jpeg.rs) rather than re-duplicating the 4-line block.
           check(&status)?;
       }
       {
           // _ = c.wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice(
           //     &swizzler, dst_slice, c.wuffs_base__empty_slice_u8(), src_slice);
           let empty = wuffs_base__slice_u8 { ptr: core::ptr::null_mut(), len: 0 };
           // SAFETY: `swizzler` is now shared (prepare's &mut borrow already ended);
           // `dst_slice` covers exactly `dst.len()` owned bytes; `src_slice` covers
           // `src.len()` caller-owned bytes for the duration of this call only.
           let _n = unsafe {
               wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice(
                   &swizzler, dst_slice, empty, src_slice,
               )
           };
       }
       // return dst;
       Ok(dst)
   }
   ```
   Each of the 5 public wrappers becomes a 1-line body (signature unchanged):
   `swizzle(alloc, src, WUFFS_BASE__PIXEL_FORMAT__<SRC>, WUFFS_BASE__PIXEL_FORMAT__RGBA_PREMUL)`.
6. **Ledger**: close out `DEFERRALS.md:3741-3761` ("rustty-wuffs swizzle — 5
   unreached pixel-format seams") once this lands, and update the two
   `PORT STATUS` comments that currently say "R5 seam (deferred)" —
   `crates/rustty-wuffs/src/swizzle.rs:3-8` and `crates/rustty-wuffs/src/
   lib.rs:6-7` ("swizzle is an R5 seam — not on the bg-image decode path").

## Key finding: `gaToRgba` is unconditionally `Err(Error::WuffsError)`

Traced `wuffs_base__pixel_swizzler__prepare`'s `switch (src_pixfmt.repr)`
(wuffs-v0.4.c:29893-29963) against the src format each wrapper passes:

| wrapper | src format | repr | `prepare()` case? |
|---|---|---|---|
| `gToRgba` | `Y` | `0x20000008` | yes → `prepare__y` |
| `gaToRgba` | `YA_PREMUL` | `0x22000088` | **no case** (only `YA_NONPREMUL` = `0x21000088` is handled) |
| `rgbToRgba` | `RGB` | `0xA0000888` | yes → `prepare__rgb` |
| `bgrToRgba` | `BGR` | `0x80000888` | yes → `prepare__bgr` |
| `bgraToRgba` | `BGRA_PREMUL` | `0x82008888` | yes → `prepare__bgra_premul` |

`YA_PREMUL` and `YA_NONPREMUL` are genuinely different `repr` values (confirmed
by their `#define`s, wuffs-v0.4.c:4472-4473) — this is not a typo in my
reading, it's the oracle itself calling `swizzle` with the PREMUL variant
while wuffs' swizzler only implements the NONPREMUL source case. When no
`switch` arm matches, `func` stays `NULL`, and `prepare()`'s final line
(`func ? NULL : wuffs_base__error__unsupported_pixel_swizzler_option`,
wuffs-v0.4.c:29976-29977) returns a **non-OK status** — which the (inlined,
in the oracle) `is_ok` check turns into `return error.WuffsError`. So, as
literally written, `pkg/wuffs/swizzle.zig`'s `gaToRgba` **always** fails,
for every input, before touching a single byte. (The allocation itself still
succeeds first — `YA_PREMUL` bpp = 16 bits = 2 bytes/pixel, a normal,
supported *destination-sizing* computation — so this is `errdefer`-freed
cleanly, not a crash.)

This is a genuine oracle quirk, not a rustty defect to paper over. Per R1
fidelity, port it AS-IS: `gaToRgba` faithfully becomes a function that always
returns `Err(Error::WuffsError)` for any well-formed (even-length) input, and
`assert!`-panics (not a recoverable error) for a malformed odd-length input
(same as the other 4). The KAT for `gaToRgba` asserts exactly this — it is
the single most important test in this seam, since it's the one place a
"clean up the oracle" instinct could silently diverge from ghostty.

## Byte-exact KATs (new — the oracle has none to port)

Derived directly from the vendored swizzle-function bodies (not guessed):
`swizzle_xxxx__y` (wuffs-v0.4.c:27629), `swizzle_bgrw__bgr` (26629, selected
for `RGB→{RGBA,BGRA,...}` dst by `prepare__rgb`'s switch, wuffs-v0.4.c:29556-
29566), `swizzle_bgrw__rgb` (26826, selected for `BGR→RGBA` by
`prepare__bgr`, wuffs-v0.4.c:29068-29076), `swizzle_swap_rgbx_bgrx`
(24053, selected for `BGRA_PREMUL→RGBA_PREMUL` blend=SRC by
`prepare__bgra_premul`, wuffs-v0.4.c:29343-29349). (On an x86_64-v2 host with
runtime SSE4.2, `prepare()` picks the `__x86_sse42` sibling of some of these
instead — functionally identical, vectorized; the byte results below hold
either way, that equivalence is wuffs' own contract, out of rustty's port
scope to re-verify.)

```rust
#[test]
fn g_to_rgba_two_pixels() {
    // Y bytes are copied into R=G=B, alpha forced to 0xFF
    // (wuffs_private_impl__swizzle_xxxx__y: poke_u32le(0xFF000000 | 0x010101*s0)).
    let out = gToRgba(Global, &[0x00, 0xFF]).unwrap();
    assert_eq!(out.as_slice(), &[0,0,0,255, 255,255,255,255]);
}

#[test]
fn ga_to_rgba_always_unsupported() {
    // YA_PREMUL has no case in wuffs_base__pixel_swizzler__prepare's src switch
    // (only YA_NONPREMUL is handled) => prepare() ALWAYS returns a non-OK status,
    // for every input, before any byte is touched. Any even-length input (2 bytes
    // = 1 gray+alpha pixel) demonstrates this; must not panic, must not succeed.
    let err = gaToRgba(Global, &[0x80, 0xFF]).unwrap_err();
    assert_eq!(err, Error::WuffsError);
}

#[test]
fn rgb_to_rgba_is_identity_plus_alpha() {
    // RGB->RGBA_PREMUL dispatches to swizzle_bgrw__bgr, which peek_u24le's the 3
    // src bytes as a raw little-endian value and poke_u32le's it back with the
    // MSB forced to 0xFF — net effect: byte-for-byte copy + trailing 0xFF (no
    // channel reorder, since RGB's memory order already matches RGBA_PREMUL's).
    let out = rgbToRgba(Global, &[10, 20, 30, 40, 50, 60]).unwrap();
    assert_eq!(out.as_slice(), &[10,20,30,255, 40,50,60,255]);
}

#[test]
fn bgr_to_rgba_swaps_r_and_b() {
    // BGR->RGBA_PREMUL dispatches to swizzle_bgrw__rgb: d0=s2,d1=s1,d2=s0,d3=0xFF.
    // src (B,G,R) = (10,20,30) => dst (R,G,B,A) = (30,20,10,255).
    let out = bgrToRgba(Global, &[10, 20, 30]).unwrap();
    assert_eq!(out.as_slice(), &[30, 20, 10, 255]);
}

#[test]
fn bgra_premul_to_rgba_premul_swaps_r_and_b_keeps_alpha() {
    // BGRA_PREMUL->RGBA_PREMUL (blend=SRC) dispatches to swizzle_swap_rgbx_bgrx:
    // d0=s2,d1=s1,d2=s0,d3=s3. src (B,G,R,A) = (10,20,30,200) => dst (30,20,10,200).
    let out = bgraToRgba(Global, &[10, 20, 30, 200]).unwrap();
    assert_eq!(out.as_slice(), &[30, 20, 10, 200]);
}

#[test]
#[should_panic]
fn swizzle_len_not_multiple_of_pixel_size_panics() {
    // assert(src.len % src_size == 0) — an odd-length RGB (3 bytes/pixel) buffer
    // is a caller contract violation -> panic (Zig's `assert`), NOT Err.
    let _ = rgbToRgba(Global, &[1, 2, 3, 4]);
}
```

`gaToRgba`'s KAT is the load-bearing one for R1 fidelity: it is the proof
that the port does NOT "fix" the oracle by routing `gaToRgba` through
`YA_NONPREMUL` instead (which a reasonable-looking but UNFAITHFUL fix would
do, since it would make the function actually work).

## Hazards (FFI — Miri can't execute the vendored C, prove under ASan)

- **Destination-buffer sizing**: `dst`'s byte length is computed as
  `src.len() * dst_size / src_size` in Rust, then handed to
  `wuffs_base__pixel_swizzler__swizzle_interleaved_from_slice` as a raw
  `(ptr, len)` pair the C function writes through. If `bits_per_pixel`
  (via the new shim) or the format constant were wrong, the C function would
  write past the Rust-allocated `RawBuf` — this is a genuine
  heap-buffer-overflow class hazard, not a data race.
- **Why ASan and not Miri**: Miri cannot execute the vendored
  `wuffs_base__pixel_swizzler__{prepare,swizzle_interleaved_from_slice}`
  calls (foreign, no MIR) — this whole seam is FFI end-to-end, per HAZARDS
  policy it is proven with ASan instead.
- **Why ASan actually catches it without instrumenting the C TU**: `dst`'s
  allocation flows through `allocator_api2::Global` → the process global
  allocator, which ASan's malloc/free interceptor wraps regardless of
  whether the code calling into it (the uninstrumented vendored `wuffs-v0.4.c`
  object) was itself compiled with `-fsanitize=address`. ASan places a
  poisoned redzone around every intercepted allocation and validates it at
  free time; a C-side out-of-bounds write into that redzone is reported as
  `heap-buffer-overflow` (or corrupted-redzone-at-free) even though the write
  instruction itself carries no inline shadow check. This is the same
  technique implicitly relied on by the *existing* (unexercised — see below)
  png.rs/jpeg.rs `RawBuf`/`Scratch` FFI writes; this workstream is the first
  to actually spell out and run it for `rustty-wuffs`.
- **Proof plan** (uses the repo's existing turnkey gate, no new tooling):
  ```sh
  .port/tools/gates/sanitizers.sh rustty-wuffs swizzle:: address
  ```
  Expect `SANITIZERS: GREEN (rustty-wuffs address)`. Because the hazard is a
  heap-overflow class (not a race), `address` alone suffices; the gate's
  default matrix also runs `thread` on macOS — harmless to include (no
  shared mutable state crosses a thread boundary in this seam) but not
  load-bearing here.
  Also run once with `ASAN_OPTIONS=detect_leaks=0:strict_string_checks=1` (the
  gate already sets `detect_leaks=0` on macOS) plus one deliberately-broken
  run (temporarily widen `dst_size` by +1 byte in a scratch build) to confirm
  the harness actually trips `heap-buffer-overflow` — a proof that doesn't
  fire on an injected bug is not a proof.
- No SB/TB (Stacked/Tree Borrows) concern: `&mut swizzler` (for `prepare`)
  and `&swizzler` (for `swizzle_interleaved_from_slice`) are used
  **sequentially**, never overlapping — the mutable borrow's lifetime ends
  before the shared one begins (this is plain sequential local-variable use,
  not the "long-lived raw alias interleaved with `&mut`" shape the
  `rust-mut-args-strongly-protected` / `shaper-hook-sb-clean-interleave`
  memories warn about) — no Miri harness needed for this half even if the
  calls were somehow made through a Miri-visible shim, because there is no
  interleaving to prove safe.

## Acceptance

- `cargo build --workspace` green (no signature changes to any of the 6
  functions; only their bodies change from `unimplemented!()` to real code,
  plus the 2 new shim files + wuffs-sys `kat` addition).
- `cargo test -p wuffs-sys` — existing `adler32_abc_known_answer` /
  `crc32_abc_known_answer` still pass; new `pixel_format_rgba_premul_shims`
  passes.
- `cargo test -p rustty-wuffs` — existing PNG/JPEG KATs unaffected
  (regression check: count of passing tests must not drop); the 6 new
  swizzle tests above all pass, including the `#[should_panic]` one.
- `.port/tools/gates/sanitizers.sh rustty-wuffs swizzle:: address` reports
  `SANITIZERS: GREEN`.
- `crates/rustty-wuffs/src/swizzle.rs` and `lib.rs`'s `PORT STATUS` comments
  updated to drop "R5 seam (deferred)"/"is deferred"; `DEFERRALS.md:3741-3761`
  closed out (struck through with a resolution note, matching the file's own
  convention elsewhere in the ledger for resolved entries).

## Risks / open decisions for the implementer

1. **Shim-vs-hand-replicate for `is_direct`/`is_interleaved`**: I recommend
   shimming (see rationale above); a reviewer preferring the leaner "hand
   -replicate the 1-line bit test, cite the `status.repr` precedent" path
   is a legitimate, smaller-surface-area alternative — either is R1-faithful,
   this is a style call, not a correctness one. **Not blocking.**
2. **`gaToRgba` behavior**: confirmed by direct C-source inspection
   (not by running the port, since it doesn't exist yet) that it always
   errors. If the port's own test run disagrees (i.e. it unexpectedly
   *succeeds* on some wuffs build configuration — e.g. if a future wuffs bump
   adds a `YA_PREMUL` case, or if `WUFFS_CONFIG__DST_PIXEL_FORMAT__ENABLE_ALLOWLIST`
   were ever defined and excluded `RGBA_PREMUL`, changing the error path) —
   that is a signal to re-read `prepare()`'s switch against the *actually
   vendored* `wuffs-v0.4.c`, not to "fix" the test to match a suspicious pass.
3. **SIMD variance**: on a non-x86_64-v2 build (or with
   `WUFFS_PRIVATE_IMPL__CPU_ARCH__X86_64_V2` undefined, e.g. aarch64), the
   `prepare__*` dispatch picks the plain scalar functions instead of the
   `__x86_sse42` siblings analyzed above. I read the scalar bodies (the
   `#if defined(WUFFS_PRIVATE_IMPL__CPU_ARCH__X86_64_V2)` guarded SSE4.2
   variants sit immediately above each scalar fn and are functionally
   identical per wuffs' own design contract) — the KATs are portable across
   both dev boxes (aarch64-apple-darwin per this repo's environment) and any
   x86_64 CI lane.
4. **No build.rs edit needed** is a load-bearing claim of this spec (verified
   empirically by building and grepping `bindings.rs`, not asserted from
   reading the header alone) — if the implementer's wuffs-sys checkout ever
   changes `MODULE_DEFINES` (e.g. someone later restricts to
   `WUFFS_CONFIG__MODULE__BASE__CORE` without `__PIXCONV`), the swizzler
   functions would vanish from the compiled object and this whole plan's
   "already bound" claims for `prepare`/`swizzle_interleaved_from_slice`
   would need re-verification.
