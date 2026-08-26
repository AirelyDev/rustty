# `stb_resize_*.bin` — glyph-resize known-answer fixtures (WS11)

These are raw, densely-packed atlas bytes for the `stbir_resize_uint8` seam in
`src/face/freetype.rs` (`FT_GLYPH_FORMAT_BITMAP` arm, the call at the
`crate::stb::stbir_resize_uint8` site). They exist because the seam's coverage used to
be **vacuous**: every pre-WS11 test that reached the resize call did so at scale 1.0
(`8x16 -> 8x16`, `136x128 -> 136x128`), and at scale 1.0 every resampler agrees.
Proof: substituting a 15-line nearest-neighbour loop for stb left **all 9**
`face::freetype::tests::*` green. (`render_and_resize`, despite the name, never enters
the bitmap arm at all — Inconsolata is an outline font.)

Each fixture is consumed by `include_bytes!` — raw bytes, never a text encoding.

## Provenance

| fixture                                | font                                            | glyph                  | size/DPI      | atlas format | `Constraint`                                                | `constraint_width` | source strike               | target    |
| -------------------------------------- | ----------------------------------------------- | ---------------------- | ------------- | ------------ | ----------------------------------------------------------- | ------------------ | --------------------------- | --------- |
| `stb_resize_terminus_i_gray_16x17.bin` | `rustty_font::embedded::TERMINUS_TTF`           | 77 (`i`)               | 12 pt @ 96×96 | `Grayscale`  | `size: Stretch`, `align_*: Center`, else `Constraint::none` | 2                  | 8×16 (mono strike)          | **16×17** |
| `stb_resize_terminus_i_gray_8x17.bin`  | same                                            | 77 (`i`)               | 12 pt @ 96×96 | `Grayscale`  | same                                                        | 1                  | 8×16 (mono strike)          | **8×17**  |
| `stb_resize_emoji_bgra_16x15.bin`      | `rustty_font::embedded::EMOJI` (NotoColorEmoji) | `glyph_index(U+1F600)` | 12 pt @ 96×96 | `Bgra`       | `size: Cover`, `align_*: Center`, else `Constraint::none`   | 2                  | 136×128 (8-bit BGRA strike) | **16×15** |
| `stb_resize_emoji_bgra_8x7.bin`        | same                                            | same                   | 12 pt @ 96×96 | `Bgra`       | same                                                        | 1                  | 136×128 (8-bit BGRA strike) | **8×7**   |

Generated on macOS aarch64, Apple clang 21.0.0, at WS11 (branch
`agent-a6d625666fc339642`, worktree of `main` @ `297afcf`). The `Face::render_glyph`
call path is the real production path — there are no test-only hooks. Both dimension
asserts in each KAT are load-bearing: `assert_eq!` pins the target, and `assert_ne!`
against the source strike size makes a regression back to an identity resize FAIL
rather than quietly turn the KAT vacuous again.

### TRAP 1 — the grid metrics for the BGRA cases must come from the PRIMARY face

Deriving them from the emoji face itself (`Metrics::calc(emoji.get_metrics())`) makes
the constraint a **no-op**: that face's cell is as large as its strike, so all nine
`constraint_width` × `ConstraintSize` combinations return 136×128 — the identity, i.e.
vacuous. The existing `color_emoji` test does exactly that, which is why it never
witnessed the resampler. The KAT takes metrics from `embedded::INCONSOLATA`, which is
also what happens at runtime (grid metrics come from the primary text face, not the
fallback emoji face), so the faithful setup and the non-vacuous setup coincide.

### TRAP 2 — the grayscale strike is MONO, so its fixture is 255× the raw resize output

Terminus' 8×16 strike is 1-bit. `FT_Bitmap_Convert(..., 1)` densifies it to 8 bpp but
leaves `num_grays = 2`, i.e. pixel values **0 and 1**; `freetype.rs` multiplies by
`factor = 255 / (num_grays - 1)` **after** the resize. So stb both reads and writes
0/1 values here, and the stored fixture is `255 ×` the raw resampler output. The
regeneration recipe below models that with `--expect-scale 255`.

Consequence, stated plainly: the grayscale fixtures are **weak discriminators**. With a
binary input the resampler's fractional output rounds back to 0/1, so a wrong resampler
only shows up where the rounding flips. Measured against these exact fixtures:

| fixture    | vs nearest neighbour          | vs bilinear                   |
| ---------- | ----------------------------- | ----------------------------- |
| gray 16×17 | 11/272 (4.0 %), max delta 255 | 2/272 (0.7 %), max delta 255  |
| gray 8×17  | 5/136 (3.7 %), max delta 255  | **0/136 — INDISTINGUISHABLE** |

The BGRA fixtures are the strong discriminators (a genuine 8-bit source, and a large
downsample through `num_channels = 4`). Measured against those exact fixtures:

| fixture    | vs nearest neighbour            | vs bilinear                     |
| ---------- | ------------------------------- | ------------------------------- |
| BGRA 16×15 | 596/960 (62.1 %), max delta 213 | 577/960 (60.1 %), max delta 114 |
| BGRA 8×7   | 185/224 (82.6 %), max delta 217 | 187/224 (83.5 %), max delta 121 |

Do not "simplify" the KAT set down to the grayscale pair — that would throw away all
of the bilinear discrimination.

## Delta budget (the F-ui-tiers rule: a golden proves nothing unless you know how far it would move if the code were wrong)

- **0 bytes** vs the D10 consensus build (host `cc` at `-O0/-O1/-O2/-O3/-Os`, all five
  byte-identical) — the KATs assert exact equality, and `run.sh --expect` re-verified
  each fixture against that consensus at generation time.
- **0 bytes** vs ghostty's own build recipe at `Debug`, `ReleaseSafe`, `ReleaseSmall`
  (`zig build-obj`, oracle @ `4749c4e93`, zig 0.16.0).
- **≤ 1 LSB on ≤ 2 % of bytes** vs ghostty `ReleaseFast` — a measured property of its
  loop vectorizer, recorded as a permanent divergence in
  `.port/phase-2/DEFERRALS.md`, continuously guarded by gate D10.
- **A wrong resampler moves far more than that**, and never by ≤1: see the two tables
  above (grayscale: max delta 255; BGRA: see D10's output). The whole point is that the
  tolerated envelope (max delta 1) and a wrong resampler (max delta ≥ 10, usually far
  more) are disjoint — machine-checked by
  `face::freetype::tests::stb_resize_kat_rejects_nearest_and_bilinear` and by
  `.port/tools/stb-resize-oracle/negative_control.py`.

**If a future change makes these fixtures need a regold, that is a signal about the
code, not the fixture.** A regold whose delta exceeds the budget above means the
resampler, its build flags, or the constraint math changed — investigate before
regolding.

## Regeneration

Fixtures are regenerated from the pipeline and then **validated against the oracle-derived
consensus** — never trusted just because our build printed them.

1. Apply the two temporary env-gated dumps (they are not committed): one at the resize
   call site writing the pre-resize densified FT bitmap as
   `pre_<sw>x<sh>_to_<w>x<h>_d<depth>.bin`, one in each KAT writing the extracted
   atlas bytes to the fixture name. Both keyed on `RUSTTY_WS11_DUMP=<dir>`.
2. Run the KAT with `RUSTTY_WS11_DUMP` set (with correctly-sized all-zero placeholder
   fixtures in place so `include_bytes!` compiles and the length assert passes).
3. **Validate each fixture** — this is the step that makes it oracle-derived:

```bash
# grayscale (mono source: model the post-resize num_grays normalisation)
bash .port/tools/stb-resize-oracle/run.sh \
  --case <dump>/pre_8x16_to_16x17_d1.bin 8 16 1 16 17 \
  --expect crates/rustty-font-shaper/src/testdata/stb_resize_terminus_i_gray_16x17.bin \
  --expect-scale 255

# BGRA (already 8-bit; no normalisation)
bash .port/tools/stb-resize-oracle/run.sh \
  --case <dump>/pre_136x128_to_16x15_d4.bin 136 128 4 16 15 \
  --expect crates/rustty-font-shaper/src/testdata/stb_resize_emoji_bgra_16x15.bin
```

Each must print `fixture ... vs consensus: 0/<n> differ, maxdelta=0  [EXACT]` and
`0/<n> differ` for ghostty `Debug`/`ReleaseSafe`/`ReleaseSmall`. If the exact
comparison fails, the fixture is wrong (or the pipeline feeds stb different
arguments than the harness) — **do not relax the comparison**, find the difference. 4. Remove the temporary dumps and confirm the KATs are green. 5. Re-run the falsification: apply a nearest-neighbour substitution at the resize call
site; the `stb_resize_kat_*` tests must FAIL while the four `bitmap_glyph*` tests
still pass.
