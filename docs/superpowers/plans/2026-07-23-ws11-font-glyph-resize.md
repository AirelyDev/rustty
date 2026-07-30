# WS11 — Font Glyph Resize (`stb_image_resize`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the FreeType bitmap-strike glyph-resize seam (`stbir_resize_uint8`)
*provably* at parity with the oracle instead of merely present. The C is already
vendored, already built with `cc`, and already wired on every non-wasm target — see
**PREMISE CORRECTION** — but **every test that reaches the resize call today resizes
at scale 1.0**, and at scale 1.0 the resampler is indistinguishable from nearest
neighbour (proved at plan-authoring time: substituting a 15-line nearest-neighbour
loop for stb leaves all 9 `face::freetype` tests green). WS11 replaces that vacuous
coverage with (a) known-answer tests on genuinely scaled bitmap strikes, grayscale
and BGRA, whose expected bytes come from an oracle-derived consensus build, (b) a
toolchain-matrix gate that compares the vendored header against **ghostty's own
build recipe at all four Zig optimize modes**, and (c) a machine-checked negative
control proving the KATs reject a wrong resampler. It then repairs the three stale
ledger rows that call this work unported, and records the one genuine, *measured*
divergence (Ghostty `ReleaseFast` differs from every other build of the same C by
±1 LSB on ≤2 % of bytes, cause identified) as a permanent divergence rather than
pretending byte-exactness with a moving target.

**Architecture:** The seam is a single call site. `crates/rustty-font-shaper/src/face/freetype.rs`
`Face::render_glyph` switches on the loaded glyph's `FT_Glyph_Format`; the
`FT_GLYPH_FORMAT_BITMAP` arm (rust `:740-845`, oracle `src/font/face/freetype.zig:575-680`)
densifies the FT bitmap to 8 bpp (`FT_Bitmap_Convert`), allocates a
`pitch = w * atlas.format().depth()` output buffer through the threaded
`allocator_api2` allocator, calls `crate::stb::stbir_resize_uint8`, wraps the result
in a synthetic `FT_Bitmap`, and `FT_Bitmap_Copy`s it over the working bitmap. Below
that, `crates/rustty-font-shaper/src/stb.rs` is the `unsafe extern "C"` declaration
(one symbol), and `crates/rustty-font-shaper/build.rs` compiles
`vendor/stb_impl.c` (`#define STB_IMAGE_RESIZE_IMPLEMENTATION` + `#include "stb_image_resize.h"`)
with `cc`, skipping `wasm32`. Nothing in this plan changes any of that code — WS11 is
verification, fixtures, a gate, and ledger truth. The target dimensions that make the
resize non-trivial come from `RenderOptions.constraint` /
`RenderOptions.constraint_width` via `rustty_font::glyph::Constraint::constrain`, so
the KATs drive the real production path with no test-only hooks.

**Tech Stack:** Rust 1.93.1 (pinned), `cc` 1.x, FreeType 2.13.2 (`crates-sys/freetype-sys`,
same version the oracle pins in `pkg/freetype/build.zig.zon`), the vendored
public-domain `stb_image_resize.h` v0.97 (byte-identical to the oracle's copy, MD5
`7381a7070120fd5a36d8fb9e3179dd7c` at both the pin and `origin/main`), Apple clang
(macOS aarch64 + x86_64 under Rosetta), Fedora 44 aarch64 VM `rvm` (gcc 16.1.1,
clang 22.1.8), `zig` 0.15.2 for the oracle-recipe comparison only (never a build
dependency of rustty), oracle git repo at `/Users/gael/projects/ghostty` pinned at
`4749c4e93`.

## Global Constraints (inherited from `2026-07-23-rustty-completion-ROADMAP.md`)

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e93`); the
  oracle is reference-only — **no build-time dependency on it** (R3). The gate this
  plan adds reads the oracle with `git -C /Users/gael/projects/ghostty show HEAD:<path>`
  into a scratch dir. **NEVER `git checkout` in the oracle repo** — four lanes depend
  on its tree staying pinned. Precedent for gates reading the oracle read-only:
  `.port/tools/gates/check_test_counts.py`, `check_rustty_h_abi.sh`.
- R2 product identity: rustty naming, per-token casing. The vendored C keeps its
  upstream `stbir_*`/`stb_image_resize.h` names verbatim (third-party public-domain
  source, not ghostty identity); the *artifact* name is already rustty-prefixed
  (`librustty_stb_image_resize.a`).
- R3 self-contained: the resampler must stay the SAME C the oracle links, compiled
  from vendored source. **A pure-Rust reimplementation is forbidden** — this plan's
  measurements show why (a wrong resampler moves 70–98 % of output bytes).
- R4/R5: acyclic graph; gates invariant, never weakened to pass. **R5 specifically:**
  this WS must not change the macOS artifact's behaviour. It adds no production code
  — only tests, fixtures, a gate, and docs — so R5 holds *by construction*, and
  Task 8 proves it by an A/B of **behaviour** plus `cargo tree -e features`.
  **Binary hashing is NOT admissible evidence**: a prior lane got three different
  hashes from rebuilds of identical source and correctly abandoned that method.
- DG5: C-ABI panic policy is ABORT. (WS11 touches no C-ABI surface. `stbir_resize_uint8`
  is a leaf C call that cannot call back into Rust, so no unwind can cross it.)
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`). LTO stays banned. **Do not
  add compiler flags to `build.rs`** — Task 3 measures why flag-pinning would be
  either a no-op or an R5 violation.
- Ledger discipline: strike/repair `.port/phase-2/DEFERRALS.md` rows in the same PR
  (Task 9). Three rows and one plan file currently assert this work is unported;
  all are wrong.
- Commit style: `subsystem: summary (WS11 T<n>)`. Branch `ws11-font-glyph-resize`;
  merge to main only with BOTH platforms green (`cargo build --workspace` on macOS
  AND on `rvm`).
- Zig byte-slices → `Vec<u8>`/`&[u8]`, never lossy `String`. Fixtures are raw
  `include_bytes!` binaries, never text-encoded pixels.
- **Do NOT run `.port/tools/gates/no_homebrew_gettext_gate.sh`** (unrelated lane).

---

## PREMISE CORRECTION (read before Task 1)

### 1. The owning ledger row is stale in the "claimed open, already shipped" direction

`.port/phase-2/DEFERRALS.md:3741` reads:

> `src/stb/stb_image_resize.h` — glyph resize (`stbir_resize_uint8`); rustty-font does NOT yet cover it
> … **Rustty counterpart:** NOT covered — `grep -rn 'stbir|stb_image_resize' crates/rustty-font/src`
> finds only `Vec::resize`/`atlas.resized` (unrelated). The glyph/atlas resize seam has no Rust analogue yet.

**All three claims are false.** Verified at plan-authoring time on `main` @ `4069662`:

| Claim in the row | Reality |
|---|---|
| "no Rust analogue yet" | `crates/rustty-font-shaper/src/stb.rs` declares `stbir_resize_uint8` (34 lines, with the oracle's `src/stb/main.zig` cited in its module doc) |
| "no `-sys`/`cc` build" | `crates/rustty-font-shaper/build.rs` compiles `vendor/stb_impl.c` with `cc::Build`, `wasm32`-skipped; `crates/rustty-font-shaper/Cargo.toml:104-108` documents it and declares `cc = "1"` under `[build-dependencies]` |
| "the C is not vendored" | `crates/rustty-font-shaper/vendor/stb_image_resize.h` is **byte-identical** to the oracle's (`git -C ../ghostty show HEAD:src/stb/stb_image_resize.h \| md5` = `7381a7070120fd5a36d8fb9e3179dd7c` = our vendored copy; also identical at `origin/main`) |
| "not wired" | the call site is `crates/rustty-font-shaper/src/face/freetype.rs:806` inside the `FT_GLYPH_FORMAT_BITMAP` arm, argument-for-argument identical to the oracle's `freetype.zig:639-652` (order, `pitch` pass-through, `atlas.format.depth()` as `num_channels`, `== 0 → error.GlyphResizeFailed`) |

**Root cause of the staleness:** the row's grep was scoped to `crates/rustty-font/src`,
but the code landed in the *sibling* crate created by the D1 three-tier split,
`crates/rustty-font-shaper` — the tier that is allowed to hold C-FFI (memory
`apc-glyf-font-layering-d1`: `rustty-font` is a PURE leaf and may never carry the FFI).
Timeline: the code landed in commit **`2e82776` "rustty: font-shaper freetype face +
9 tests" (2026-06-30)**; the ledger row was last rewritten **2026-07-09** (`734ee05`
"ledger hygiene"), i.e. **nine days after the work shipped**, and still with the
wrong grep scope.

**WS19's closeout audit repeated the same scope error.** `.port/phase-2/DEFERRALS.md:5416`
row **O17** says "`D:3741` `stb_image_resize.h` glyph resize has no rustty counterpart —
**OPEN, correctly described.** → route to WS11", and
`docs/superpowers/plans/2026-07-29-ws19-phase2-closeout.md:123,376,998` propagate it.
All four locations are wrong and Task 9 repairs them.

**Also stale, in the other direction:** the ROADMAP row
(`2026-07-23-rustty-completion-ROADMAP.md:80`) says "wired on the **non-macOS** path".
The wiring is `#[cfg(not(target_arch = "wasm32"))]`, not `#[cfg(not(target_os = "macos"))]`
— `crates/rustty-font-shaper/src/lib.rs:63-64` gates `pub mod stb;` and
`src/face/mod.rs:24-25` gates `pub mod freetype;` on wasm only. So the FreeType face
**and** the stb build are live on macOS too (`nm -g` on the macOS build artifact
`target/debug/build/rustty-font-shaper-*/out/librustty_stb_image_resize.a` shows
`T _stbir_resize_uint8`), and the same artifact exists on the VM
(`~/projects/rustty/target/debug/build/rustty-font-shaper-*/out/librustty_stb_image_resize.a`).
That is *correct* — it mirrors the oracle, whose `Backend` enum includes
`coretext_freetype` — but it means "non-macOS path" understates the blast radius and
raises R5's importance.

### 2. What is therefore actually open: the coverage is VACUOUS

Instrumenting the resize call (`eprintln!` of source vs target dimensions, then
reverted) shows **every** test in the workspace that reaches it does so at scale 1.0:

```
test face::freetype::tests::bitmap_glyph      ... PROBE resize: src 8x16   pitch 8   -> dst 8x16   pitch 8   depth 1
test face::freetype::tests::bitmap_glyph_bdf  ... PROBE resize: src 8x16   pitch 8   -> dst 8x16   pitch 8   depth 1
test face::freetype::tests::bitmap_glyph_otb  ... PROBE resize: src 8x16   pitch 8   -> dst 8x16   pitch 8   depth 1
test face::freetype::tests::bitmap_glyph_pcf  ... PROBE resize: src 8x16   pitch 8   -> dst 8x16   pitch 8   depth 1
test face::freetype::tests::color_emoji       ... PROBE resize: src 136x128 pitch 544 -> dst 136x128 pitch 544 depth 4
```

`render_and_resize` (the test whose *name* suggests it covers this) never enters the
bitmap arm at all — Inconsolata is an outline font, so it takes the
`FT_GLYPH_FORMAT_OUTLINE` branch; its "resize" is a `set_size` re-render, unrelated
to `stbir_resize_uint8`.

At scale 1.0 a Catmull-Rom/Mitchell kernel evaluated at integer offsets reproduces
the source sample exactly, so **any** resampler passes. Proof by negative control:
replacing the `stbir_resize_uint8` call with a nearest-neighbour loop and running the
suite gives

```
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
```

— i.e. the entire existing test suite cannot distinguish stb from nearest neighbour.
**This, not the C build, is WS11's real subject.** (The probe patch was reverted;
`git status --porcelain` is clean at plan-authoring time.)

### 3. "Bit-exactness" is not achievable as stated — and the reason is measured, not guessed

The ROADMAP row says "stb-sys cc build for bit-exactness". The vendored C *is*
byte-identical to the oracle's, but identical C does not imply identical bytes out.
Measured by compiling **ghostty's own recipe** — its `src/stb/stb.c` (which is what
`src/build/SharedDeps.zig:414-423` compiles: `addIncludePath("src/stb")` +
`addCSourceFiles(.{ .files = &.{"src/stb/stb.c"} })` with **no extra flags**) — read
out of the oracle with `git show`, built with `zig build-obj -O<mode>` at all four
optimize modes, and diffing the resize output against rustty's `cc` build on the same
input (an 8×16 gray source; the two cases WS11's KATs will use):

```
=== case 16x32 (upsample)
  ghostty-recipe Debug        vs rustty cc build: 0/512 differ, maxdelta=0
  ghostty-recipe ReleaseFast  vs rustty cc build: 9/512 differ, maxdelta=1
  ghostty-recipe ReleaseSafe  vs rustty cc build: 0/512 differ, maxdelta=0
  ghostty-recipe ReleaseSmall vs rustty cc build: 0/512 differ, maxdelta=0
=== case 8x17
  ghostty-recipe Debug/ReleaseFast/ReleaseSafe/ReleaseSmall vs rustty cc build: 0/136 differ, maxdelta=0
```

Consequences, each independently checked:

- **Ghostty is not self-consistent.** Its own `Debug` and `ReleaseFast` builds of the
  same header disagree by ±1 LSB. "Bit-exact with the oracle" therefore has no single
  referent; the honest target is "bit-exact with the oracle's non-`ReleaseFast` modes
  and every non-Zig toolchain, within ±1 LSB of `ReleaseFast`".
- **Cause identified: the loop vectorizer**, not fast-math and not FMA contraction.
  `zig cc -O2 -fno-vectorize` and `-fno-vectorize -fno-slp-vectorize` reproduce the
  consensus exactly; `-fno-slp-vectorize` alone, `-ffp-contract=off`, `-ffp-contract=on`,
  and `-mcpu=baseline` all still diverge. `zig cc -O3` output is **byte-identical** to
  the `ReleaseFast` oracle recipe (`cmp` clean), confirming the mechanism.
- **Rustty's build is on the consensus side, on every platform we ship.** Byte-identical
  output (both cases) from: Apple clang `-O0/-O1/-O2/-O3/-Os` (aarch64), Fedora 44
  gcc 16.1.1 `-O2`, Fedora 44 clang 22.1.8 `-O2`, and `zig cc -O0` — five toolchain/opt
  combinations across two operating systems, all agreeing bit-for-bit
  (MD5 `5f18d21d6d9d9c6f035bade776299e7c` for `16x32`,
  `0e2fb9f06dc7de62c1c28b5706bd918b` for `8x17`).
- **One further ±1 axis: architecture.** aarch64 vs x86_64 (same Apple clang, `-O2`)
  differ 9/512 (`maxdelta=1`) and 3/136 (`maxdelta=1`). So ±1 LSB is inherent to this
  resampler across arch, not a port defect. Fedora aarch64 gcc/clang match macOS
  aarch64 exactly, so the *platform* axis is clean and only the *arch* and
  *zig-ReleaseFast* axes carry the ±1.
- **Do not "fix" this by pinning flags.** The only flag set that would make rustty
  match `ReleaseFast` is to build the C with `zig cc -O3`, i.e. make the Zig toolchain
  a hard build dependency of the Rust port — directly against the port's premise and
  R3's spirit. Adding `-ffp-contract=off`/`-ffast-math` to `build.rs` would *create*
  divergence (both change output) and, because the seam is live on macOS, would be an
  R5 violation. **Decision (locked in this plan): change no flags.** Task 9 records
  the ReleaseFast ±1 as a permanent divergence with a four-criterion basis.

### 4. The delta budget that makes the KATs non-vacuous (folded in from `.port/phase-3/assessments/F-ui-tiers.md`)

F-ui-tiers established the standard this plan inherits: *a golden proves nothing
unless you know in advance how much it would move if the code were wrong.* It applied
that to the 24 upstream PNG regolds — pixel-decoding proved 19 were encoder-only
churn and pinned the genuinely affected set at 6 files, **~76 px / max-delta 48** —
and recommended porting `z2d`'s `Path::offset` with its own geometry tests *before*
letting goldens be the only witness.

Same method, applied here in advance:

| Comparison | bytes differing | max delta | verdict |
|---|---|---|---|
| stb vs stb, identity scale (what the 9 existing tests do) | 0 | 0 | **cannot discriminate anything** |
| stb (rustty `cc`) vs stb (Fedora gcc/clang, all opt levels, zig `-O0`) | 0 | 0 | exact — the KAT asserts equality here |
| stb aarch64 vs stb x86_64 | 9/512, 3/136 (≤2 %) | 1 | tolerated envelope |
| stb vs stb (ghostty `ReleaseFast`) | 9/512 (1.8 %) | 1 | tolerated envelope |
| **stb vs nearest neighbour** | 501/512 (98 %), 124/136 (91 %) | **89 / 192** | must be REJECTED |
| **stb vs bilinear** | 388/512 (76 %), 95/136 (70 %) | **28 / 14** | must be REJECTED |

The discrimination gap is ~14× in max delta and ~40× in affected-byte fraction, so an
"exact, with a ±1-on-≤2 % escape hatch only for the cross-toolchain gate" rule is
decisively falsifiable. Task 6 makes that rejection **machine-checked** rather than a
claim in prose.

### 5. Scope boundary (stated, not deferred)

F-ui-tiers' `d0e72a3ab` item (z2d 0.12.1 + `Path.offset` native path insetting, 6
pixel-affected sprite goldens) is **beyond the pin** (`4749c4e93..origin/main`) and is
already routed: `.port/phase-3/assessments/G-config-build-i18n.md:539` assigns it
"G identifies → F owns", i.e. the Phase-3 area-F advance, and it requires a
`crates/rustty-z2d` 0.10.0→0.12.x feature port first. Pulling it into WS11 would
break R1's pin discipline (WS11 targets the pinned oracle) and the ROADMAP's WS11
row. It is **not deferred by this plan** — it is owned elsewhere, with its measured
regold budget (6 files, ~76 px, max-delta 48) already recorded in F-ui-tiers §2b. What
WS11 *does* take from that assessment is the methodology above, applied to its own
seam.

---

## File Structure

```
crates/rustty-font-shaper/
  src/stb.rs                                    MODIFY  + #[cfg(test)] mod tests (2 tests)
  src/face/freetype.rs                          MODIFY  + 3 tests in the existing `mod tests`
  src/testdata/                                 (exists — currently only arabic.txt)
    stb_resize_terminus_i_gray_16x17.bin         NEW    fixture
    stb_resize_terminus_i_gray_8x17.bin          NEW    fixture
    stb_resize_emoji_bgra_16x15.bin              NEW    fixture
    stb_resize_emoji_bgra_8x7.bin                NEW    fixture
    README.stb-resize.md                         NEW    fixture provenance + regeneration recipe
.port/tools/
  stb-resize-oracle/
    oracle.c                                     NEW    harness: deterministic input -> raw resized bytes
    run.sh                                       NEW    toolchain matrix incl. ghostty's own recipe
    negative_control.py                          NEW    nearest/bilinear references + delta report
  gates/check_stb_resize_parity.sh               NEW    D10 gate (wraps run.sh, fail-closed)
  gates/README.md                                MODIFY register D10
  ci.sh                                          MODIFY register D10 in the gate list + header legend
.port/phase-2/DEFERRALS.md                       MODIFY strike :3741; correct O17 at :5416; add the divergence entry
docs/superpowers/plans/
  2026-07-29-ws19-phase2-closeout.md             MODIFY correct its three O17 rows
  2026-07-23-rustty-completion-ROADMAP.md        MODIFY WS11 row scope + outcome note
```

No file under `crates/` outside `rustty-font-shaper` is touched. No production
(non-`cfg(test)`) line of Rust changes anywhere.

---

### Task 1: Branch, baseline, and reproduce the premise-correction evidence

**Files:** none modified (a temporary probe patch is applied and reverted).

**Interfaces:** none.

- [ ] **Step 1: Branch**

```bash
cd /Users/gael/projects/rustty && git checkout main && git pull && git checkout -b ws11-font-glyph-resize
```

- [ ] **Step 2: Baseline the suite that owns the seam**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-font-shaper --lib face::freetype:: 2>&1 | tail -14
```

Expected, verbatim (this is the plan-authoring baseline — if it differs, stop and
re-derive; the 9 names below are the real ones and the KAT gate in Task 9 keys on
them):

```
running 9 tests
test face::freetype::tests::svg_font_table ... ok
test face::freetype::tests::face_name ... ok
test face::freetype::tests::bitmap_glyph ... ok
test face::freetype::tests::bitmap_glyph_pcf ... ok
test face::freetype::tests::bitmap_glyph_otb ... ok
test face::freetype::tests::bitmap_glyph_bdf ... ok
test face::freetype::tests::mono_to_bgra ... ok
test face::freetype::tests::color_emoji ... ok
test face::freetype::tests::render_and_resize ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 98 filtered out
```

- [ ] **Step 3: Confirm the C actually builds and exports the symbol on this host**

```bash
nm -g /Users/gael/projects/rustty/target/debug/build/rustty-font-shaper-*/out/librustty_stb_image_resize.a 2>/dev/null | grep stbir_resize_uint8
```

Expected: at least `T _stbir_resize_uint8` (plus the `_srgb`/`_generic` siblings the
single header also defines). **Negative control:** if the `out/` dir does not exist,
the build never ran — re-run Step 2 first, do not proceed on a stale artifact.

- [ ] **Step 4: Reproduce the vacuity finding (the whole justification for this WS)**

Temporarily replace the `stbir_resize_uint8` call in
`crates/rustty-font-shaper/src/face/freetype.rs` (the `if unsafe { crate::stb::stbir_resize_uint8(…) } == 0 {`
block at `:805-819`) with a nearest-neighbour loop:

```rust
            // TEMPORARY PROBE — revert before Step 5.
            let probe_ok = {
                let d = atlas.format().depth() as usize;
                let sw = glyph_bitmap.width as usize;
                let sh = glyph_bitmap.rows as usize;
                let sp = glyph_bitmap.pitch as usize;
                for oy in 0..(h as usize) {
                    let sy = oy * sh / (h as usize);
                    for ox in 0..(w as usize) {
                        let sx = ox * sw / (w as usize);
                        for ch in 0..d {
                            buf[oy * (pitch as usize) + ox * d + ch] =
                                unsafe { *glyph_bitmap.buffer.add(sy * sp + sx * d + ch) };
                        }
                    }
                }
                1
            };
            if probe_ok == 0 {
```

then:

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-font-shaper --lib face::freetype:: 2>&1 | tail -4
```

Expected (the finding): `test result: ok. 9 passed; 0 failed`. **This is the RED
baseline WS11 must flip.** If instead a test fails, the premise correction is wrong
for this tree — stop and re-audit before writing any fixture.

- [ ] **Step 5: Revert the probe and confirm a clean tree**

```bash
cd /Users/gael/projects/rustty && git checkout crates/rustty-font-shaper/src/face/freetype.rs && git status --porcelain
```

Expected: no output.

- [ ] **Step 6: Commit the branch point (empty marker commit, so the RED baseline is dated)**

```bash
cd /Users/gael/projects/rustty
git commit --allow-empty -m "font-shaper: WS11 branch point — stb resize coverage proven vacuous at scale 1.0 (WS11 T1)"
```

---

### Task 2: Direct-FFI unit tests for `crate::stb` (the smallest falsifiable step)

**Files:**
- Modify: `crates/rustty-font-shaper/src/stb.rs` (append a `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `crate::stb::stbir_resize_uint8` (unchanged).
- Produces: tests `stbir_resize_uint8_upsamples_a_known_ramp`,
  `stbir_resize_uint8_rejects_zero_target_size`.

Rationale: `stb.rs` has **zero** tests today. These two are independent of FreeType,
so they isolate "is the C linked, and does the declaration's argument order/ABI
match?" from "does the pipeline drive it correctly?" — the second is Tasks 4–5.

- [ ] **Step 1: Write the failing tests**

Append to `crates/rustty-font-shaper/src/stb.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::stbir_resize_uint8;

    /// The deterministic ramp used by `.port/tools/stb-resize-oracle/oracle.c`, so a
    /// value produced here can be cross-checked against every toolchain in the D10
    /// matrix without re-deriving the input.
    fn ramp_8x16() -> Vec<u8> {
        (0..16)
            .flat_map(|y| (0..8).map(move |x| ((x * 31 + y * 17) & 0xff) as u8))
            .collect()
    }

    /// Not a golden — a STRUCTURAL assertion that the C ran and interpolated:
    /// a 2x horizontal upsample of a strictly increasing row must be monotonic
    /// non-decreasing across that row, and must contain values that appear in NO
    /// source row (proof of interpolation rather than sample copying). Nearest
    /// neighbour would produce only source values and fails the second half.
    #[test]
    fn stbir_resize_uint8_upsamples_a_known_ramp() {
        let src = ramp_8x16();
        let mut dst = vec![0u8; 16 * 32];
        let rc = unsafe {
            stbir_resize_uint8(src.as_ptr(), 8, 16, 8, dst.as_mut_ptr(), 16, 32, 16, 1)
        };
        assert_eq!(1, rc, "stbir_resize_uint8 must succeed on a valid in-memory resize");

        let row: Vec<u8> = dst[0..16].to_vec();
        assert!(
            row.windows(2).all(|w| w[0] <= w[1]),
            "a 2x upsample of a monotonic row must stay monotonic, got {row:?}"
        );
        let source_values: std::collections::HashSet<u8> = src.iter().copied().collect();
        assert!(
            dst.iter().any(|b| !source_values.contains(b)),
            "output contains only source sample values — the resampler did not interpolate"
        );
    }

    /// The `== 0` failure arm the freetype face maps to `Error::GlyphResizeFailed`
    /// (oracle `freetype.zig:649-652`). stb rejects a non-positive target extent, so
    /// this pins that the arm is reachable at the C boundary even if the pipeline's
    /// own `rect.width < 0.25` early-return makes it unreachable from `render_glyph`.
    #[test]
    fn stbir_resize_uint8_rejects_zero_target_size() {
        let src = ramp_8x16();
        let mut dst = [0u8; 16];
        let rc = unsafe {
            stbir_resize_uint8(src.as_ptr(), 8, 16, 8, dst.as_mut_ptr(), 0, 4, 0, 1)
        };
        assert_eq!(0, rc, "a zero-width target must be rejected");
    }
}
```

- [ ] **Step 2: Run — expect the compile to succeed and both tests to pass immediately**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-font-shaper --lib stb:: 2>&1 | tail -8
```

Expected: `test result: ok. 2 passed`. These are *characterization* tests over already
correct code, so green-on-first-run is correct; the TDD obligation is discharged by
Step 3's falsification, not by an initially-red run.

- [ ] **Step 3: Falsify them (mandatory — do not skip)**

Temporarily swap the `input_w`/`input_h` arguments at the call inside
`stbir_resize_uint8_upsamples_a_known_ramp` (pass `16, 8` instead of `8, 16`) and
re-run. Expected: the test fails (the monotonicity assertion or a differing output).
Then temporarily change `assert_eq!(0, rc, …)` in the second test to expect `1`;
expected: that test fails. Revert both.

If either mutation leaves the test green, the assertion is vacuous — strengthen it
before continuing.

- [ ] **Step 4: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-font-shaper/src/stb.rs
git commit -m "font-shaper/stb: direct-FFI tests for stbir_resize_uint8 (interpolation + zero-target rejection) (WS11 T2)"
```

---

### Task 3: The oracle harness + the D10 toolchain-matrix gate

**Files:**
- Create: `.port/tools/stb-resize-oracle/oracle.c`
- Create: `.port/tools/stb-resize-oracle/run.sh`
- Create: `.port/tools/stb-resize-oracle/negative_control.py`
- Create: `.port/tools/gates/check_stb_resize_parity.sh`
- Modify: `.port/tools/gates/README.md`, `.port/tools/ci.sh`

**Interfaces:**
- Consumes: `crates/rustty-font-shaper/vendor/stb_image_resize.h` (ours) and, read-only
  via `git -C /Users/gael/projects/ghostty show HEAD:src/stb/{stb.c,stb_image.h,stb_image_resize.h}`,
  the oracle's own copies + build recipe.
- Produces: `exit 0` GREEN / non-zero RED / `exit 3` BLOCKED (missing `zig`, missing
  oracle repo) — never a silent green.

This is the piece that makes the fixtures in Tasks 4–5 *oracle-derived* rather than
self-referential.

- [ ] **Step 1: Write `oracle.c`**

```c
/* WS11: emit raw resized bytes for a deterministic input, so any toolchain's build of
 * the vendored stb_image_resize can be diffed byte-for-byte. Deliberately links no
 * rustty code: the only variable under test is the C build. */
#include <stdio.h>
#include <string.h>
extern int stbir_resize_uint8(const unsigned char *, int, int, int,
                              unsigned char *, int, int, int, int);
int main(int argc, char **argv) {
    unsigned char src[8 * 16];
    for (int y = 0; y < 16; y++)
        for (int x = 0; x < 8; x++)
            src[y * 8 + x] = (unsigned char)((x * 31 + y * 17) & 0xff);
    int w = 16, h = 32;
    if (argc > 2) { sscanf(argv[1], "%d", &w); sscanf(argv[2], "%d", &h); }
    static unsigned char dst[256 * 256];
    memset(dst, 0, sizeof dst);
    stbir_resize_uint8(src, 8, 16, 8, dst, w, h, w, 1);
    fwrite(dst, 1, (size_t)(w * h), stdout);
    return 0;
}
```

Note: the ramp is byte-identical to `stb.rs`'s `ramp_8x16()` from Task 2 — keep them
in sync, and say so in a comment in both.

- [ ] **Step 2: Write `run.sh` (the matrix)**

It must, for the two cases `16 32` and `8 17`:

1. build our vendored header standalone with the host `cc` at `-O0 -O1 -O2 -O3 -Os`
   and require all five outputs identical (call this the **consensus**);
2. build the *oracle's* recipe — `git show HEAD:src/stb/{stb.c,stb_image.h,stb_image_resize.h}`
   into a scratch dir, then `zig build-obj -O$MODE -lc -I. -fno-sanitize-c stb.c`
   for `MODE` in `Debug ReleaseFast ReleaseSafe ReleaseSmall`, linked against
   `oracle.c` — and report each mode's delta vs the consensus;
3. assert: `Debug`, `ReleaseSafe`, `ReleaseSmall` are **exactly** equal to the
   consensus; `ReleaseFast` is within **max delta 1 on ≤ 2 % of bytes**;
4. print a one-line summary per comparison in the form
   `<label>: <n>/<total> differ, maxdelta=<d>`.

Notes learned the hard way at plan-authoring time (bake these in, they are not
optional):

- `zig build-obj -ODebug` needs **`-fno-sanitize-c`** or the link fails with
  `undefined symbol: ___ubsan_handle_type_mismatch_v1` (etc.). `-fno-sanitize=undefined`
  is **not** a valid `zig build-obj` flag. This flag affects integer/pointer UB traps
  only, never FP codegen, so it does not perturb the measurement.
- On Linux, linking `stb_image_resize` needs **`-lm`** (`pow` from the sRGB paths);
  on macOS it does not. Add `-lm` unconditionally on non-Darwin.
- Do **not** `git checkout` in the oracle; `git show` into `$(mktemp -d)` only, and
  `trap`-clean it.

- [ ] **Step 3: Write `negative_control.py`**

Reads the consensus output for both cases and compares against reference
nearest-neighbour and bilinear resamplings of the same ramp, printing
`differ`/`maxdelta`, and **exiting non-zero if either would pass the ±1-on-≤2 %
envelope** — i.e. it proves the envelope discriminates. The expected output, measured
at plan-authoring time (treat any large deviation from these as a signal the harness
is wrong, not that the numbers moved):

```
16x32 stb-vs-nearest:  501/512 differ, maxdelta=89,  mean=13.0
16x32 stb-vs-bilinear: 388/512 differ, maxdelta=28,  mean=7.0
8x17  stb-vs-nearest:  124/136 differ, maxdelta=192, mean=15.0
8x17  stb-vs-bilinear:  95/136 differ, maxdelta=14,  mean=2.8
```

- [ ] **Step 4: Write the gate wrapper**

`.port/tools/gates/check_stb_resize_parity.sh` runs `run.sh` then
`negative_control.py`, maps their results to the gate contract
(`0` GREEN / non-zero RED / `3` BLOCKED when `zig` or the oracle repo is absent — per
`.port/tools/gates/README.md`'s "never a silent green" rule), and supports
`--self-test`, which rebuilds our header with `-ffast-math` and requires the gate to
go **RED** on it (the D-series convention: every detector ships a proof of its own RED
flip; `check_sanitizer_canary.sh` is the model).

- [ ] **Step 5: Run the gate; record its output verbatim in the commit message**

```bash
cd /Users/gael/projects/rustty && bash .port/tools/gates/check_stb_resize_parity.sh; echo "exit=$?"
bash .port/tools/gates/check_stb_resize_parity.sh --self-test; echo "self-test exit=$?"
```

Expected (matching the plan-authoring measurement):

```
=== case 16x32
  ghostty-recipe Debug        vs consensus: 0/512 differ, maxdelta=0
  ghostty-recipe ReleaseFast  vs consensus: 9/512 differ, maxdelta=1     [within envelope]
  ghostty-recipe ReleaseSafe  vs consensus: 0/512 differ, maxdelta=0
  ghostty-recipe ReleaseSmall vs consensus: 0/512 differ, maxdelta=0
=== case 8x17
  ghostty-recipe <all four modes> vs consensus: 0/136 differ, maxdelta=0
exit=0
self-test exit=<non-zero>
```

- [ ] **Step 6: Register D10 in `ci.sh` and the gates README**

Add to the `ci.sh` gate array, alongside the other detectors (`ci.sh:197-204`):

```
  "D10|PROGRESS|bash $GATES/check_stb_resize_parity.sh"
```

and a matching legend line in the header comment block (`ci.sh:60-72` style):

```
#   D10 check_stb_resize_parity.sh    vendored stb_image_resize output vs ghostty's
#                                     own build recipe (4 zig optimize modes)  -> PROGRESS
```

PROGRESS, not HARD: it is `exit 3` BLOCKED on any box without `zig` or the oracle
checkout, and `run_gate`'s "PROGRESS went RED" branch still escalates a genuine
mismatch to a hard failure — so R5 is not weakened.

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/tools/stb-resize-oracle .port/tools/gates/check_stb_resize_parity.sh \
        .port/tools/gates/README.md .port/tools/ci.sh
git commit -m "gates: D10 — vendored stb_image_resize vs ghostty's own build recipe across 4 zig optimize modes (WS11 T3)"
```

---

### Task 4: Grayscale bitmap-strike resize KAT

**Files:**
- Create: `crates/rustty-font-shaper/src/testdata/stb_resize_terminus_i_gray_16x17.bin`
- Create: `crates/rustty-font-shaper/src/testdata/stb_resize_terminus_i_gray_8x17.bin`
- Create: `crates/rustty-font-shaper/src/testdata/README.stb-resize.md`
- Modify: `crates/rustty-font-shaper/src/face/freetype.rs` (`mod tests`)

**Interfaces:**
- Consumes: `Face::render_glyph`, `rustty_font::glyph::{Align, Constraint, ConstraintSize, RenderOptions}`,
  `rustty_font::embedded::TERMINUS_TTF`, `Atlas<Global>` with `Format::Grayscale`.
- Produces: test `stb_resize_kat_bitmap_strike_grayscale`.

**Why these dimensions:** the source strike is 8×16 (pinned independently by the
existing `bitmap_glyph` test, which asserts `glyph.width == 8 && glyph.height == 16`
against the `TERMINUS_I` ASCII-art constant at `freetype.rs:1660-1698`). Driving
`Constraint { size: ConstraintSize::Stretch, … }` with `constraint_width: 2` yields
**16×17** — a 2.0× horizontal and 1.0625× vertical resample — and with
`constraint_width: 1` yields **8×17**. Both measured at plan-authoring time:

```
PROBE cw=1 size=Cover   -> 8x16    (identity — NOT usable as a KAT)
PROBE cw=1 size=Stretch -> 8x17
PROBE cw=1 size=Fit     -> 8x16    (identity)
PROBE cw=2 size=Cover   -> 8x17
PROBE cw=2 size=Stretch -> 16x17
PROBE cw=2 size=Fit     -> 8x16    (identity)
```

The KAT must assert the dimensions too — that is what keeps it from silently
regressing back into an identity resize if FreeType or the constraint math changes.

- [ ] **Step 1: Write the failing test**

Append inside the existing `mod tests` in `crates/rustty-font-shaper/src/face/freetype.rs`:

```rust
    /// KAT: the `stbir_resize_uint8` seam on a NON-UNIT bitmap-strike resize.
    ///
    /// The four `bitmap_glyph*` tests above all resize at scale 1.0, where every
    /// resampler agrees (proved: substituting nearest neighbour leaves them green),
    /// so they cannot witness resampler parity. This one does: Terminus' 8x16 strike
    /// (pinned by `bitmap_glyph`) is stretched to 16x17 and 8x17, and the resulting
    /// atlas bytes are compared against fixtures generated from the vendored C by the
    /// D10 consensus build (`.port/tools/stb-resize-oracle/`, provenance in
    /// `src/testdata/README.stb-resize.md`). Both dimension asserts are load-bearing:
    /// if the constraint math ever collapses these back to 8x16 the test must fail
    /// loudly rather than quietly become vacuous again.
    #[test]
    fn stb_resize_kat_bitmap_strike_grayscale() -> Result<(), Error> {
        use rustty_font::glyph::{Align, Constraint};

        const KAT_16X17: &[u8] =
            include_bytes!("../testdata/stb_resize_terminus_i_gray_16x17.bin");
        const KAT_8X17: &[u8] =
            include_bytes!("../testdata/stb_resize_terminus_i_gray_8x17.bin");

        let alloc = Global;
        let mut lib = Library::init()?;
        let mut atlas = Atlas::init(alloc, 512, Format::Grayscale)?;
        let mut ft_font = Face::init(lib, embedded::TERMINUS_TTF, opts(size(12.0, 96, 96)))?;
        let metrics = Metrics::calc(ft_font.get_metrics());

        // (constraint_width, expected w, expected h, fixture)
        let cases: [(u8, u32, u32, &[u8]); 2] = [(2, 16, 17, KAT_16X17), (1, 8, 17, KAT_8X17)];

        for (cw, ew, eh, expected) in cases {
            // glyph 77 = 'i' (same glyph the `bitmap_glyph` test pins at 8x16).
            let g = ft_font.render_glyph(
                alloc,
                &mut atlas,
                77,
                RenderOptions {
                    grid_metrics: metrics,
                    constraint_width: cw,
                    constraint: Constraint {
                        size: ConstraintSize::Stretch,
                        align_horizontal: Align::Center,
                        align_vertical: Align::Center,
                        ..Constraint::none
                    },
                    ..Default::default()
                },
            )?;

            assert_eq!((ew, eh), (g.width, g.height), "constraint_width={cw}: target size");
            assert_ne!(
                (8u32, 16u32),
                (g.width, g.height),
                "constraint_width={cw}: this must be a NON-identity resize or the KAT is vacuous"
            );
            assert_eq!(expected.len(), (g.width * g.height) as usize, "fixture length");

            let mut actual = Vec::with_capacity(expected.len());
            for y in 0..g.height as usize {
                for x in 0..g.width as usize {
                    actual.push(
                        atlas.data()[(g.atlas_y as usize + y) * atlas.size() as usize
                            + (g.atlas_x as usize + x)],
                    );
                }
            }
            assert_eq!(
                expected, &actual[..],
                "constraint_width={cw}: resized bytes differ from the stb consensus fixture"
            );
        }

        ft_font.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }
```

- [ ] **Step 2: Run to verify it fails**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-font-shaper --lib stb_resize_kat_bitmap_strike_grayscale 2>&1 | tail -12
```

Expected: a **compile** failure — ``couldn't read `.../testdata/stb_resize_terminus_i_gray_16x17.bin`: No such file or directory``.
This is the correct first failure: the fixture is the thing under construction.

- [ ] **Step 3: Generate the fixtures — from the pipeline, then VALIDATE them against the D10 consensus**

The bytes must not be "whatever our build printed". Procedure:

1. Add a temporary `eprintln!` dump (hex) of `actual` to the test, run it, and write
   the two byte streams to the fixture paths.
2. **Validate, don't trust:** run `.port/tools/stb-resize-oracle/run.sh` restricted to
   the *same* transform the pipeline performed. The pipeline's input is the densified
   FT bitmap, not the harness ramp, so add a mode to `run.sh` that accepts an input
   `.bin` + source dims on the command line, and feed it the pre-resize FT bitmap
   (dump it with a second temporary `eprintln!` next to the resize call). Require:
   the harness's output under **every** toolchain in the matrix, and under ghostty's
   `Debug`/`ReleaseSafe`/`ReleaseSmall` recipes, is **byte-identical to the fixture**;
   and ghostty `ReleaseFast` is within max delta 1 on ≤2 % of bytes.
3. Remove both temporary `eprintln!`s.

If step 2's exact comparison fails, the fixture is wrong (or the pipeline feeds stb
different arguments than the harness) — **do not** relax the comparison; find the
difference.

- [ ] **Step 4: Write the provenance file**

`crates/rustty-font-shaper/src/testdata/README.stb-resize.md` must record, for each
fixture: the font constant, glyph index, point size/DPI, the `Constraint` used, the
source strike dims, the target dims, the generating commit, the exact regeneration
command, and the **delta budget** (`0` vs the consensus and vs ghostty
Debug/ReleaseSafe/ReleaseSmall; `≤1 LSB on ≤2 %` vs ghostty `ReleaseFast`; a wrong
resampler moves 70–98 % of bytes at max delta 14–192). A future regold that exceeds
the budget means the *code* changed, not the fixture — that is the F-ui-tiers property
this preserves.

- [ ] **Step 5: Run — expect green**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-font-shaper --lib stb_resize_kat 2>&1 | tail -8
```

Expected: `test result: ok. 1 passed`.

- [ ] **Step 6: Falsify it — the negative control this task exists for**

Re-apply Task 1 Step 4's nearest-neighbour substitution and re-run:

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-font-shaper --lib face::freetype:: 2>&1 | tail -14
```

Expected: `stb_resize_kat_bitmap_strike_grayscale` **FAILS** while the four
`bitmap_glyph*` tests still pass — which is simultaneously the proof that the new KAT
discriminates and that the old ones never did. Record the failure output in the commit
message. Then `git checkout crates/rustty-font-shaper/src/face/freetype.rs`-style
revert of the probe only (careful: the KAT lives in the same file — revert the probe
hunk by hand, then re-run Step 5 to confirm green again).

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-font-shaper/src/face/freetype.rs crates/rustty-font-shaper/src/testdata
git commit -m "font-shaper/freetype: grayscale bitmap-strike resize KAT (16x17 + 8x17, consensus-validated fixtures) (WS11 T4)"
```

---

### Task 5: BGRA (color-emoji) bitmap-strike resize KAT

**Files:**
- Create: `crates/rustty-font-shaper/src/testdata/stb_resize_emoji_bgra_16x15.bin`
- Create: `crates/rustty-font-shaper/src/testdata/stb_resize_emoji_bgra_8x7.bin`
- Modify: `crates/rustty-font-shaper/src/face/freetype.rs` (`mod tests`),
  `crates/rustty-font-shaper/src/testdata/README.stb-resize.md`

**Interfaces:**
- Consumes: `embedded::EMOJI` (NotoColorEmoji, a 136×128 BGRA strike), `Format::Bgra`,
  and — critically — grid metrics from a **different, outline** face.
- Produces: test `stb_resize_kat_bitmap_strike_bgra`.

**Why this case matters and how to make it non-trivial:** `num_channels = 4` exercises
a completely different stb code path (interleaved channels, `pitch = w * 4`), and the
transform is a large *downsample* (136×128 → 16×15), which selects the Mitchell
downsampling kernel rather than the upsampling one. Measured trap: passing
`Metrics::calc(emoji_face.get_metrics())` makes the constraint a **no-op** — the emoji
face's own cell is as large as its strike, so every combination returns 136×128
(identity, i.e. vacuous):

```
PROBE-EMOJI (metrics from the EMOJI face)        cw=1..3 x {Cover,Stretch,Fit} -> 136x128 in all 9 cases
PROBE-EMOJI (metrics from INCONSOLATA @ 12pt/96) cw=1 Cover -> 8x7    cw=1 Stretch -> 8x17
                                                 cw=2 Cover -> 16x15  cw=2 Stretch -> 16x17
                                                 cw=3 Cover -> 16x15  cw=3 Stretch -> 16x17
```

Taking metrics from the primary text face is also what really happens at runtime
(grid metrics come from the primary face, not the fallback emoji face), so the
faithful setup and the non-vacuous setup coincide.

- [ ] **Step 1: Write the failing test**

Append inside the same `mod tests`:

```rust
    /// KAT: the 4-channel (BGRA) side of the resize seam, on a large DOWNSAMPLE.
    ///
    /// `color_emoji` above renders the same face but at scale 1.0 (136x128 -> 136x128)
    /// because it derives grid metrics from the emoji face itself, whose cell is as
    /// large as its strike. Real rendering takes grid metrics from the PRIMARY text
    /// face, which is what makes the constraint bite; that is reproduced here, giving
    /// a 136x128 -> 16x15 / 8x7 downsample through `num_channels = 4`.
    #[test]
    fn stb_resize_kat_bitmap_strike_bgra() -> Result<(), Error> {
        use rustty_font::glyph::{Align, Constraint};

        const KAT_16X15: &[u8] = include_bytes!("../testdata/stb_resize_emoji_bgra_16x15.bin");
        const KAT_8X7: &[u8] = include_bytes!("../testdata/stb_resize_emoji_bgra_8x7.bin");

        let alloc = Global;
        let mut lib = Library::init()?;
        let mut atlas = Atlas::init(alloc, 1024, Format::Bgra)?;

        // Grid metrics come from the PRIMARY (outline) face, as at runtime.
        let mut primary = Face::init(lib, embedded::INCONSOLATA, opts(size(12.0, 96, 96)))?;
        let metrics = Metrics::calc(primary.get_metrics());
        primary.deinit();

        let mut emoji = Face::init(lib, embedded::EMOJI, opts(size(12.0, 96, 96)))?;
        let idx = emoji.glyph_index(0x1F600).expect("U+1F600 must be in NotoColorEmoji");

        let cases: [(u8, u32, u32, &[u8]); 2] = [(2, 16, 15, KAT_16X15), (1, 8, 7, KAT_8X7)];

        for (cw, ew, eh, expected) in cases {
            let g = emoji.render_glyph(
                alloc,
                &mut atlas,
                idx,
                RenderOptions {
                    grid_metrics: metrics,
                    constraint_width: cw,
                    constraint: Constraint {
                        size: ConstraintSize::Cover,
                        align_horizontal: Align::Center,
                        align_vertical: Align::Center,
                        ..Constraint::none
                    },
                    ..Default::default()
                },
            )?;

            assert_eq!((ew, eh), (g.width, g.height), "constraint_width={cw}: target size");
            assert_ne!(
                (136u32, 128u32),
                (g.width, g.height),
                "constraint_width={cw}: must be a real downsample, not the identity path"
            );
            assert_eq!(expected.len(), (g.width * g.height * 4) as usize, "fixture length (BGRA)");

            let mut actual = Vec::with_capacity(expected.len());
            for y in 0..g.height as usize {
                for x in 0..g.width as usize {
                    let i = ((g.atlas_y as usize + y) * atlas.size() as usize
                        + (g.atlas_x as usize + x))
                        * 4;
                    actual.extend_from_slice(&atlas.data()[i..i + 4]);
                }
            }
            assert_eq!(
                expected, &actual[..],
                "constraint_width={cw}: resized BGRA bytes differ from the stb consensus fixture"
            );
        }

        emoji.deinit();
        atlas.deinit(alloc);
        lib.deinit();
        Ok(())
    }
```

- [ ] **Step 2: Run to verify it fails** — same expected failure mode as Task 4 Step 2
  (missing fixture files).

- [ ] **Step 3: Generate + validate the fixtures** by the identical two-stage procedure
  as Task 4 Step 3 (dump, then require the D10 harness to reproduce the bytes exactly
  under every consensus toolchain from the same pre-resize FT bitmap, with
  `num_channels = 4`).

- [ ] **Step 4: Extend `README.stb-resize.md`** with the two BGRA rows (including the
  "metrics must come from the primary face" trap, so a regold cannot silently reintroduce
  the identity case).

- [ ] **Step 5: Run — expect green**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-font-shaper --lib stb_resize_kat 2>&1 | tail -8
```

Expected: `test result: ok. 2 passed`.

- [ ] **Step 6: Falsify** — re-apply the nearest-neighbour probe; the BGRA KAT must
  FAIL. Measured at plan-authoring time (FNV-1a over the extracted bytes, stb vs
  nearest neighbour) — every scaled case moves, every identity case does not:

```
                          stb                NN
cw=1 Cover   8x7  n=224   27e13941ea9c7317   1f09afb5539f7b4e   -> differs
cw=1 Stretch 8x17 n=544   9793842968f4dc98   f2e107e19c84c5aa   -> differs
cw=2 Cover  16x15 n=960   1a94418d940482df   676eda64207ebd2f   -> differs
cw=2 Stretch 16x17 n=1088 ce8dc76eb6bf4da4   c3a5874748337831   -> differs
```

  Revert the probe and re-confirm green.

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-font-shaper/src/face/freetype.rs crates/rustty-font-shaper/src/testdata
git commit -m "font-shaper/freetype: BGRA bitmap-strike resize KAT (136x128 -> 16x15 / 8x7 downsample) (WS11 T5)"
```

---

### Task 6: Make the negative control machine-checked, not procedural

**Files:**
- Modify: `crates/rustty-font-shaper/src/face/freetype.rs` (`mod tests`)

**Interfaces:**
- Consumes: the two grayscale fixtures from Task 4.
- Produces: test `stb_resize_kat_rejects_nearest_and_bilinear`.

Tasks 4–6 rely on a human re-applying the NN probe to know the KATs discriminate. That
evidence evaporates after the commit. This test bakes it in: it computes nearest-neighbour
and bilinear resamplings *of the same source strike* in the test itself and asserts they
are **rejected** by the fixture, with the measured delta magnitudes as the assertion —
so if someone later loosens the KAT to a tolerance, this test fails.

- [ ] **Step 1: Write the test**

Requirements (name and shape fixed; the implementer writes the arithmetic):

- Render the unconstrained glyph 77 from `embedded::TERMINUS_TTF` at 12pt/96 to obtain
  the 8×16 source strike bytes from the atlas (the existing `bitmap_glyph` path).
- Compute `nearest(16,17)` and `bilinear(16,17)` from it.
- Assert against `stb_resize_terminus_i_gray_16x17.bin`:
  - `nearest != fixture` and `bilinear != fixture` (exact inequality), AND
  - the fraction of differing bytes is `> 0.5` for both, AND
  - `max |delta| >= 10` for both.
- Assert the tolerated-noise envelope is *disjoint* from those: a byte-vector equal to
  the fixture with `+1` applied to 2 % of its bytes must satisfy
  `max delta <= 1 && differing fraction <= 0.02` — i.e. the envelope used by the D10
  gate cannot be satisfied by a wrong resampler.

Measured targets for the assertion thresholds (Terminus source, harness ramp numbers
are the same order): nearest ≈ 91–98 % of bytes at max delta 89–192; bilinear
≈ 70–76 % at max delta 14–28. Choose `> 0.5` / `>= 10` so the test is robust to small
fixture changes but still fails for anything resembling a correct-but-different filter.

- [ ] **Step 2: Run**

```bash
cd /Users/gael/projects/rustty && cargo test -p rustty-font-shaper --lib stb_resize_kat 2>&1 | tail -8
```

Expected: `test result: ok. 3 passed`.

- [ ] **Step 3: Falsify the falsifier**

Temporarily weaken the assertion to `assert!(nearest_diff_fraction > 0.999)` and
confirm it fails (measured 0.91 for the 8×17 case, so a >0.999 threshold must be
unmet); revert. This proves the thresholds are inside the measured band rather than
trivially true.

- [ ] **Step 4: Commit**

```bash
cd /Users/gael/projects/rustty
git add crates/rustty-font-shaper/src/face/freetype.rs
git commit -m "font-shaper/freetype: machine-checked negative control — the resize KATs reject nearest + bilinear (WS11 T6)"
```

---

### Task 7: Non-macOS (FreeType/Linux) verification on `rvm`

**Files:** none modified.

**Platform:** Linux aarch64 (Fedora 44). This is the platform the ROADMAP row names,
and the only one where the FreeType face is the *default* backend
(`face/mod.rs:41-44`: macOS default `coretext`, Linux default `fontconfig_freetype`).

**VM SAFETY:** never run a heavy build on `rvm` concurrently with headed compositor
work in another lane. Check first: `ssh rvm 'pgrep -a -f "rustty|gtk4|Xwayland" | head'`
— if a headed session is live, wait.

- [ ] **Step 1: Sync and build**

```bash
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout ws11-font-glyph-resize && git pull && cargo build -p rustty-font-shaper 2>&1 | tail -3'
```

Expected: `Finished`. (The vendored `cc` build is already known to work there — the
artifact `~/projects/rustty/target/debug/build/rustty-font-shaper-*/out/librustty_stb_image_resize.a`
exists on the VM today.)

- [ ] **Step 2: Run the whole face suite, including the new KATs**

```bash
ssh rvm 'cd ~/projects/rustty && cargo test -p rustty-font-shaper --lib face::freetype:: 2>&1 | tail -18'
ssh rvm 'cd ~/projects/rustty && cargo test -p rustty-font-shaper --lib stb:: 2>&1 | tail -8'
```

Expected: 12 passed in the first (the original 9 + the 3 new KATs), 2 passed in the
second, 0 failed in both. **This is the substantive claim of the task**: the fixtures
generated on macOS/Apple-clang are reproduced byte-for-byte by Fedora gcc/clang. If any
KAT fails here with `maxdelta <= 1`, do **not** add a tolerance — stop and report; it
would mean the consensus is narrower than measured (plan-authoring measurement: Fedora
gcc 16.1.1 `-O2` and clang 22.1.8 `-O2` both produce output byte-identical to macOS
Apple-clang `-O2`, MD5 `5f18d21d6d9d9c6f035bade776299e7c` / `0e2fb9f06dc7de62c1c28b5706bd918b`
for the 16×32 / 8×17 harness cases).

- [ ] **Step 3: Run the D10 gate there too (second, independent toolchain family)**

```bash
ssh rvm 'cd ~/projects/rustty && bash .port/tools/gates/check_stb_resize_parity.sh; echo exit=$?'
```

Expected: `exit=0`, or `exit=3` (BLOCKED) if `zig` is not installed on the VM — in
which case record that in the commit message and note that the ghostty-recipe arm is
macOS-only, while the gcc/clang consensus arm still ran.

- [ ] **Step 4: Full-workspace build on the VM (the standing cross-platform rule)**

```bash
ssh rvm 'cd ~/projects/rustty && cargo build --workspace 2>&1 | tail -3'
```

Expected: `Finished`.

- [ ] **Step 5: Commit the recorded evidence**

Append a short "Task 7 RECORDED EVIDENCE" block to this plan file with the verbatim
test-result lines and the VM's compiler versions, then:

```bash
cd /Users/gael/projects/rustty
git add docs/superpowers/plans/2026-07-23-ws11-font-glyph-resize.md
git commit -m "docs: WS11 T7 recorded evidence — resize KATs reproduce byte-exactly under Fedora gcc 16 + clang 22 (WS11 T7)"
```

---

### Task 8: R5 — prove the macOS artifact is behaviour-identical

**Files:** none modified.

**Platform:** macOS aarch64. The seam is live on macOS too (see PREMISE CORRECTION §1),
so R5 is a real obligation, not a formality.

**Method note (do not deviate):** **do not hash binaries.** A prior lane obtained three
different hashes from rebuilds of byte-identical source and correctly switched to an
A/B of *behaviour* plus a feature-graph diff. Follow that.

- [ ] **Step 1: Prove no production code changed**

```bash
cd /Users/gael/projects/rustty
git diff main...ws11-font-glyph-resize --stat
git diff main...ws11-font-glyph-resize -- crates/ | grep -E '^\+' | grep -v '^+++' | grep -vc 'cfg(test)\|^\+\s*//\|testdata'
```

Expected: the `--stat` touches only `crates/rustty-font-shaper/src/{stb.rs,face/freetype.rs}`,
`crates/rustty-font-shaper/src/testdata/*`, `.port/**`, and `docs/**`; and every added
line under `crates/` is inside a `#[cfg(test)] mod tests`, a comment, or a testdata
file. If a single production line changed, this task's premise is void — re-scope.

- [ ] **Step 2: Feature-graph A/B**

```bash
cd /Users/gael/projects/rustty
git checkout main  && cargo tree -e features -p rustty-font-shaper > /tmp/ws11_tree_main.txt
git checkout ws11-font-glyph-resize && cargo tree -e features -p rustty-font-shaper > /tmp/ws11_tree_ws11.txt
diff /tmp/ws11_tree_main.txt /tmp/ws11_tree_ws11.txt && echo "FEATURE GRAPH IDENTICAL"
```

Expected: `FEATURE GRAPH IDENTICAL`. **Negative control:** confirm the command can
detect a change at all — temporarily add `features = ["parallel"]` to any dependency
in `crates/rustty-font-shaper/Cargo.toml`, re-run the diff, see output, revert.

- [ ] **Step 3: Behaviour A/B on the macOS-relevant surface**

The CoreText face is macOS's default backend, and the shaper's macOS-facing suites are
`shaper::coretext_tests` and `face::coretext`. Run the whole crate on both branches and
diff the *result lines*:

```bash
cd /Users/gael/projects/rustty
git checkout main && cargo test -p rustty-font-shaper 2>&1 | grep -E '^test .* \.\.\. ' | sort > /tmp/ws11_tests_main.txt
git checkout ws11-font-glyph-resize && cargo test -p rustty-font-shaper 2>&1 | grep -E '^test .* \.\.\. ' | sort > /tmp/ws11_tests_ws11.txt
diff /tmp/ws11_tests_main.txt /tmp/ws11_tests_ws11.txt
```

Expected: the only differences are **additions** — the 5 new test lines
(`stb::tests::stbir_resize_uint8_upsamples_a_known_ramp`,
`stb::tests::stbir_resize_uint8_rejects_zero_target_size`,
`face::freetype::tests::stb_resize_kat_bitmap_strike_grayscale`,
`face::freetype::tests::stb_resize_kat_bitmap_strike_bgra`,
`face::freetype::tests::stb_resize_kat_rejects_nearest_and_bilinear`). No pre-existing
line may change from `ok` to anything else, and none may disappear.

- [ ] **Step 4: Whole-workspace macOS build + the app-facing suites**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -3 && cargo test -p rustty-font-shaper -p rustty-font 2>&1 | tail -6
```

Expected: `Finished`, and `test result: ok` for both crates.

- [ ] **Step 5: Record and commit**

Append a "Task 8 RECORDED EVIDENCE" block (the three diffs' verbatim outcomes, plus an
explicit line stating that binary hashing was deliberately NOT used and why), then commit.

---

### Task 9: Ledger truth pass

**Files:**
- Modify: `.port/phase-2/DEFERRALS.md` (the `:3741` entry; the O17 row at `:5416`; a new
  permanent-divergence entry)
- Modify: `docs/superpowers/plans/2026-07-29-ws19-phase2-closeout.md` (`:123`, `:376`, `:998`)
- Modify: `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md` (`:80` + the
  "Ledger corrections already established" list at `:38-48`)

- [ ] **Step 1: Rewrite `DEFERRALS.md:3741` as RESOLVED-WITH-CORRECTION**

Do not delete it — the file's convention is to keep the audit trail (see the
`## … — **DISCHARGED [date]; row was STALE**` heading style used in
`VERIFICATION_DEBT.md`). Replace the body with: the four false claims and their
refutations from PREMISE CORRECTION §1; the landing commit `2e82776` (2026-06-30) vs
the row's last rewrite `734ee05` (2026-07-09); the root cause (grep scoped to
`crates/rustty-font/src`, code lives in `crates/rustty-font-shaper` per the D1 split);
and the WS11 outcome (5 new tests + the D10 gate + the ReleaseFast divergence entry).
State explicitly that the ROADMAP's "non-macOS path" phrasing was also wrong — the
seam is `cfg(not(target_arch = "wasm32"))`, live on macOS too.

- [ ] **Step 2: Correct the WS19 audit row O17 at `DEFERRALS.md:5416`**

It currently reads "**OPEN, correctly described.** | **route to WS11** | **M**".
Rewrite the verdict cell to record that the audit re-ran the ledger's own grep with the
ledger's own (wrong) scope and so inherited the error, with the one-line refutation
(`crates/rustty-font-shaper/src/stb.rs` + `build.rs` + byte-identical `vendor/`
header + call site at `face/freetype.rs:806`), and the real residual (vacuous
identity-scale coverage) that WS11 closed. This is a **methodology** correction worth
recording: a status audit that re-runs a stale row's own grep verbatim cannot detect a
row whose grep scope is the bug.

- [ ] **Step 3: Add the permanent-divergence entry**

New `DEFERRALS.md` entry: **`stbir_resize_uint8` output differs from a `ReleaseFast`
ghostty build by ≤1 LSB on ≤2 % of bytes.** Include the measured table from PREMISE
CORRECTION §3, the identified cause (`ReleaseFast`'s loop vectorizer; `zig cc -O2
-fno-vectorize` restores the consensus; `zig cc -O3` is byte-identical to the
`ReleaseFast` recipe), and the four-criterion permanent-divergence basis:

1. **Not closable within the port's constraints** — the only construction that matches
   `ReleaseFast` is compiling the C with `zig cc -O3`, making the Zig toolchain a hard
   build dependency of the Rust port.
2. **The oracle is not self-consistent** — ghostty's own `Debug` and `ReleaseFast`
   builds of the identical header disagree by the same ±1, so "bit-exact with the
   oracle" has no unique referent.
3. **Not port-specific** — the same ±1 appears between aarch64 and x86_64 builds of the
   same compiler at the same optimization level; it is a property of the resampler's
   float summation order.
4. **Below the observable threshold, by upstream's own standard** — upstream accepted
   sprite-golden regolds at max-delta 48 (`d0e72a3ab`, per
   `.port/phase-3/assessments/F-ui-tiers.md`); ±1 LSB in an anti-aliased glyph bitmap
   is 48× smaller and imperceptible.

Also state what is *guarded*: D10 continuously asserts the envelope, so a future drift
beyond ±1/2 % is RED, and the in-crate KATs assert exact equality on the shipping
toolchains.

- [ ] **Step 4: Correct the three WS19 plan rows**

`docs/superpowers/plans/2026-07-29-ws19-phase2-closeout.md`:
- `:123` (`| O17 stb_image_resize glyph resize | **→ WS11** |`) — annotate that the
  routing was right but the status was wrong (already shipped 2026-06-30; WS11 closed
  the coverage gap instead).
- `:376` (`| WS11 font glyph resize | **NOT authored** | no | open; inherits O17 |`) —
  update to authored/landed with this plan's filename.
- `:998` (the routing table row) — same annotation.

- [ ] **Step 5: Update the ROADMAP**

`docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md:80` — replace the WS11
scope cell so it describes what WS11 actually is (the C build, the vendoring, and the
wiring already shipped in `2e82776`; WS11 added scaled-bitmap KATs on both channel
depths, the D10 toolchain-matrix gate vs ghostty's own recipe, and the ±1 `ReleaseFast`
divergence basis), fix "non-macOS path" → "every non-wasm target (macOS included)", and
append a `**DONE**` outcome note in the same style as the WS18 row. Add a bullet to the
"Ledger corrections already established (do not re-litigate)" list at `:38-48`:
`stb_image_resize glyph resize (D:3741) — SHIPPED 2026-06-30 in rustty-font-shaper; the row's grep was scoped to the wrong crate. WS11 closed the coverage gap.`

- [ ] **Step 6: Verify no other stale references remain**

```bash
cd /Users/gael/projects/rustty
grep -rn "stb_image_resize\|stbir" --include='*.md' .port/ docs/ | grep -iv "resolved\|shipped\|corrected\|divergence\|WS11" | head
```

Expected: no output (or only historical/immutable records such as
`.port/phase-3/UPSTREAM_*` and this plan's own PREMISE CORRECTION).

- [ ] **Step 7: Commit**

```bash
cd /Users/gael/projects/rustty
git add .port/phase-2/DEFERRALS.md docs/superpowers/plans/2026-07-29-ws19-phase2-closeout.md \
        docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md
git commit -m "parity: strike the stale stb_image_resize deferral (shipped 2026-06-30, wrong grep scope); ledger the ReleaseFast +/-1 divergence (WS11 T9)"
```

---

### Task 10: Merge to main and clean up

**Files:** none (git flow).

- [ ] **Step 1: Final green sweep on both platforms**

```bash
cd /Users/gael/projects/rustty
cargo build --workspace 2>&1 | tail -3
cargo test -p rustty-font-shaper 2>&1 | tail -6
bash .port/tools/gates/check_stb_resize_parity.sh; echo "D10 exit=$?"
python3 .port/tools/gates/check_test_counts.py font_shaper_face_freetype 2>&1 | tail -5
ssh rvm 'cd ~/projects/rustty && cargo build --workspace 2>&1 | tail -3 && cargo test -p rustty-font-shaper --lib 2>&1 | tail -5'
```

Expected: `Finished` on both; `test result: ok` with the new tests present; `D10 exit=0`
(or `3` on the VM without zig). `check_test_counts.py` must stay GREEN — its
`font_shaper_face_freetype` mapping (`check_test_counts.py:533`) requires
`rust >= zig` (currently 9 ≥ 8), and WS11 only adds Rust tests, so the margin grows
from +1 to +4. **Negative control for that claim:** the gate's contract line says
`rust < zig is RED`; run it and read the printed counts rather than assuming.

- [ ] **Step 2: Merge**

```bash
cd /Users/gael/projects/rustty
git checkout main && git pull
git merge --no-ff ws11-font-glyph-resize -m "merge: WS11 font glyph resize — scaled bitmap-strike resize KATs (gray + BGRA), D10 stb toolchain-matrix gate, stale-deferral repair"
git push
```

- [ ] **Step 3: Verify the merge result on both platforms**

```bash
cd /Users/gael/projects/rustty && cargo build --workspace 2>&1 | tail -3 && cargo test -p rustty-font-shaper 2>&1 | tail -4
ssh rvm 'cd ~/projects/rustty && git checkout main && git pull && cargo build --workspace 2>&1 | tail -3'
```

- [ ] **Step 4: Delete the branch**

```bash
cd /Users/gael/projects/rustty && git branch -d ws11-font-glyph-resize && git push origin --delete ws11-font-glyph-resize
ssh rvm 'cd ~/projects/rustty && git branch -D ws11-font-glyph-resize 2>/dev/null; true'
```

- [ ] **Step 5: Confirm a clean tree**

```bash
cd /Users/gael/projects/rustty && git status --short
```

Expected: no output.

---

## Verification debt

**None.** Every check in this plan was demonstrated runnable on the available hardware
during plan authoring:

- the vendored `cc` build: proven working on macOS (`nm` on the built archive) **and**
  on the Fedora VM (the same artifact exists there);
- the ghostty-recipe comparison at all four Zig optimize modes: run locally with
  `zig build-obj` on oracle sources read via `git show` (no checkout);
- the cross-architecture axis (`x86_64`): discharged locally under Rosetta
  (`cc -arch x86_64`), so no "needs an Intel box" debt;
- the cross-toolchain axis (gcc/clang): discharged on `rvm`;
- the non-macOS runtime path: `rvm` runs the FreeType face as its default backend.

The only environment-shaped caveat is that the `zig`-based arm of the D10 gate reports
`exit 3` BLOCKED on a box without `zig` installed — that is the gate contract working
as designed (never a silent green), not debt.

---

## Task 7 RECORDED EVIDENCE — Linux (Fedora 44 aarch64) verification

Host: `gael@192.168.64.4` (`~/projects/rustty`), branch `ws11-verify` =
`origin/ws11-font-glyph-resize`. Compilers: **gcc (GCC) 16.1.1 20260515 (Red Hat
16.1.1-2)**, **clang version 22.1.8 (Fedora 22.1.8-1.fc44)**. `zig`: **ABSENT** — so
the ghostty-recipe arm reports BLOCKED there, exactly as the plan's Step 3 anticipated.
VM safety checked first: `uptime` load 0.45–0.56, no `cargo`/`rustty` process running.

### The substantive claim — fixtures generated on macOS/Apple clang reproduce byte-for-byte on Fedora

All four KAT fixtures, replayed through the D10 harness from the dumped pre-resize FT
bitmaps, under **both** compiler families at **all five** optimization levels:

```
CC=gcc / CC=clang, -O0 -O1 -O2 -O3 -Os, every combination:
  fixture stb_resize_terminus_i_gray_16x17.bin : 0/272 differ, maxdelta=0 [EXACT]
  fixture stb_resize_terminus_i_gray_8x17.bin  : 0/136 differ, maxdelta=0 [EXACT]
  fixture stb_resize_emoji_bgra_16x15.bin      : 0/960 differ, maxdelta=0 [EXACT]
  fixture stb_resize_emoji_bgra_8x7.bin        : 0/224 differ, maxdelta=0 [EXACT]
```

### The runtime path (FreeType is Linux's default backend)

```
running 12 tests
test face::freetype::tests::bitmap_glyph_pcf ... ok
test face::freetype::tests::bitmap_glyph ... ok
test face::freetype::tests::bitmap_glyph_bdf ... ok
test face::freetype::tests::face_name ... ok
test face::freetype::tests::bitmap_glyph_otb ... ok
test face::freetype::tests::svg_font_table ... ok
test face::freetype::tests::stb_resize_kat_bitmap_strike_grayscale ... ok
test face::freetype::tests::mono_to_bgra ... ok
test face::freetype::tests::color_emoji ... ok
test face::freetype::tests::stb_resize_kat_rejects_nearest_and_bilinear ... ok
test face::freetype::tests::render_and_resize ... ok
test face::freetype::tests::stb_resize_kat_bitmap_strike_bgra ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 73 filtered out
```

```
running 2 tests
test stb::tests::stbir_resize_uint8_upsamples_a_known_ramp ... ok
test stb::tests::stbir_resize_uint8_treats_channels_independently ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 83 filtered out
```

12 and 2 — the plan's predicted counts exactly (9 original + 3 new; 2 new).

### What Linux verification FOUND — a genuine mis-specification in D10 arm 1

The plan (and the gate as first written) required the host `cc`'s five optimization
levels to be **byte-identical**, calling that value "the consensus". That is an Apple
clang property, not a property of the C. Measured on Fedora, on the RAMP cases:

```
gcc   -O0  16x32=770a168dbeb3a54a75eb663cf017ed6e  8x17=4dc5c919ff484e653f5427a2503ed290
gcc   -O1  16x32=770a168dbeb3a54a75eb663cf017ed6e  8x17=4dc5c919ff484e653f5427a2503ed290
gcc   -O2  16x32=5f18d21d6d9d9c6f035bade776299e7c  8x17=0e2fb9f06dc7de62c1c28b5706bd918b
gcc   -O3  16x32=5f18d21d6d9d9c6f035bade776299e7c  8x17=0e2fb9f06dc7de62c1c28b5706bd918b
gcc   -Os  16x32=5f18d21d6d9d9c6f035bade776299e7c  8x17=0e2fb9f06dc7de62c1c28b5706bd918b
clang -O0..-Os (all five) = 5f18d21d... / 0e2fb9f0...  (the consensus)
```

So **Fedora gcc -O0/-O1 form a third variant**, 9/512 and 3/136 bytes off the
consensus at max delta 1 — the same ±1 codegen axis, third instance (after ghostty
`ReleaseFast` and aarch64-vs-x86_64). Everything else — Apple clang at all five levels,
Fedora clang at all five levels, Fedora gcc at -O2/-O3/-Os — produces the consensus
MD5s the plan recorded (`5f18d21d…` / `0e2fb9f0…`).

Consequences, applied in the T7 commit:

1. **Arm 1 now holds the non-reference opt levels to the same ±1 envelope** as every
   other codegen axis, instead of demanding byte-identity. Left as-authored, D10 would
   be permanently and **falsely** RED on any Linux box whose `cc` is gcc — i.e. CI.
2. **The fixture (`--expect`) comparison is now required EXACT at every opt level**,
   not just at -O2. That is strictly stronger than the plan specified, and it holds on
   both platforms and both compiler families (see the table above).
3. **The fraction bound moved 2 % → 3 %.** 3/136 = 2.21 % exceeds a hard 2.00 % purely
   because the buffer is small; 1.76 % and 2.21 % are the largest rates observed
   anywhere, so 3 % is the smallest round bound above measurement. `negative_control.py`
   and the in-crate KAT were updated to validate the SAME bound the gate enforces (so
   the check can never validate a stricter envelope than the one in force), and the
   discrimination margin is still ~18–20× (a wrong resampler moves 55–98 % of bytes).

Post-fix, on Linux: `VM RESULT: fail=0`, every `run.sh` invocation
`GREEN on the consensus arm; BLOCKED on the ghostty-recipe arm` (no zig), the two
gcc-only deltas now reported as `[within the +/-1-on-3% codegen envelope]`.

### Not run on the VM

`cargo build --workspace` (plan Step 4) was **deliberately not run there**: the standing
VM rule forbids a heavy build concurrently with a headed compositor session, and
Xwayland was live. The crate-scoped build/test above covers the seam, and the
whole-workspace build is verified on macOS (Task 8). This is a scope note, not a
deferral of any WS11 check.

---

## Task 8 RECORDED EVIDENCE — R5, the macOS artifact is behaviour-identical

**Binary hashing was deliberately NOT used.** A prior lane obtained three different
hashes from rebuilds of byte-identical source; hashes measure build nondeterminism, not
behaviour. R5 is discharged here by (a) proving no production line changed, (b) a
feature-graph A/B with its own negative control, and (c) a behaviour A/B of every test
outcome in the crate.

### Step 1 — no production code changed

`git diff 297afcf..HEAD --stat`: **1518 insertions, 0 deletions**, touching only
`.port/tools/**`, `docs/superpowers/plans/2026-07-23-ws11-font-glyph-resize.md`,
`crates/rustty-font-shaper/src/{stb.rs,face/freetype.rs}`, and
`crates/rustty-font-shaper/src/testdata/**`.

Machine-checked that every added `crates/` line is test-only:

```
crates/rustty-font-shaper/src/face/freetype.rs: mod tests at line 1455;
    1 added hunk; hunks at/above `mod tests`: []; removed lines: 0
crates/rustty-font-shaper/src/stb.rs:           mod tests at line 32;
    1 added hunk (starts at the `#[cfg(test)]` attribute itself); removed lines: 0
```

Zero removed lines in either file, and no hunk touches anything above the
`#[cfg(test)] mod tests` boundary. No `build.rs` flag changed (the locked Task-3
decision), no `Cargo.toml` changed, no vendored C changed.

### Step 2 — feature-graph A/B

```
cargo tree -e features -p rustty-font-shaper   (1193 lines)
diff <297afcf> <HEAD>  ->  FEATURE GRAPH IDENTICAL
```

**Negative control for that claim** (proving the comparison can detect anything at
all): temporarily adding `features = ["nightly"]` to `allocator-api2` in
`crates/rustty-font-shaper/Cargo.toml` makes the same diff report

```
7a8,9
> ├── allocator-api2 feature "nightly"
> │   └── allocator-api2 v0.2.21
```

Reverted; `git status --porcelain` clean.

### Step 3 — behaviour A/B on the whole crate

Every `^test … \.\.\. ` result line, sorted, on both revisions:

```
$ diff tests_main.txt tests_ws11.txt
39a40,42
> test face::freetype::tests::stb_resize_kat_bitmap_strike_bgra ... ok
> test face::freetype::tests::stb_resize_kat_bitmap_strike_grayscale ... ok
> test face::freetype::tests::stb_resize_kat_rejects_nearest_and_bilinear ... ok
106a110,111
> test stb::tests::stbir_resize_uint8_treats_channels_independently ... ok
> test stb::tests::stbir_resize_uint8_upsamples_a_known_ramp ... ok
```

**Additions only** — exactly the 5 new tests. No pre-existing line changed from `ok`,
and none disappeared. 107 -> 112 tests, `test result: ok. 112 passed; 0 failed`.
The macOS-facing suites (`face::coretext`, `shaper::coretext_tests`) are byte-identical
in that listing, which is the R5 claim: the CoreText path — macOS's default backend —
is untouched, and the FreeType/stb path that IS live on macOS gains only tests.
