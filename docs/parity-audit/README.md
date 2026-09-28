# rustty ↔ ghostty functional-parity audit

> **Closed 2026-09-28.** All 497 findings are closed or waived, and `G-PARITY` is HARD in CI. What was closed, what was waived and why, and what is still open or unverified: [`.port/parity-closure/README.md`](../../.port/parity-closure/README.md).

## Method

Scope was the **entire** ghostty oracle tree at pin `20abdb50a` — 983 files, 403,246 lines, split into 215 shards — with no sampling: every shard's finder read its assigned line ranges **in full**, including tests, comments, build files and generated data, and every line in scope is accounted for in an area file's coverage table.

A **finding** is any functional divergence between the oracle and rustty: a missing file/function/branch/error path/test, a different constant, a stub, or a behavioural difference. **Performance-only differences are excluded by design** — rustty is free to be faster or slower, allocate differently, or restructure hot loops, as long as the observable behaviour matches. Deliberate, documented R4/R5 port divergences are recorded but only count as findings where they change behaviour.

Every candidate finding was then judged by **two independent adversarial lenses**, each trying to refute it: a _presence/relocation_ lens (is the logic actually absent, or just moved to another crate/file?) and a _semantic-equivalence_ lens (is the difference observable at all?). **CONFIRMED** = neither lens could refute it; **DISPUTED** = exactly one lens refuted it; **REFUTED** = both did. Refuted findings are retained in the area files with the refutation, but do not count toward the surviving totals. Severity (high/medium/low) is the _corrected_ severity after verification — a lens may raise or lower it.

Each shard had a per-shard verification cap on how many findings could be double-verified. Anything over that cap would be labelled **UNVERIFIED** in its area file. **The cap was never hit: 0 UNVERIFIED findings across all 215 shards**, and no shard failed to return a result.

## Totals

**215 shards · 983 oracle files · 403,246 lines in scope, 403,246 read (100%) · 0 partial units · 0 shards with no result.**
**600 candidate findings: 404 CONFIRMED, 93 DISPUTED, 103 REFUTED, 0 UNVERIFIED.**
**497 surviving findings (confirmed + disputed): 12 high, 140 medium, 345 low.**

## Areas

| Area                                |  Shards | Lines read / in scope | Partial |   High |  Medium |     Low |    CONF |   DISP |     REF |
| ----------------------------------- | ------: | --------------------: | ------: | -----: | ------: | ------: | ------: | -----: | ------: |
| [include](include.md)               |       8 |       14,640 / 14,640 |       0 |      2 |       4 |       6 |       9 |      3 |       7 |
| [macos](macos.md)                   |      22 |       37,392 / 37,392 |       0 |      0 |       4 |       4 |       6 |      2 |       4 |
| [pkg](pkg.md)                       |       9 |       14,431 / 14,431 |       0 |      4 |       7 |      22 |      21 |     12 |      13 |
| [src](src.md)                       |       5 |       11,039 / 11,039 |       0 |      1 |       7 |      11 |      17 |      2 |       0 |
| [src/apprt](src-apprt.md)           |      17 |       30,175 / 30,175 |       0 |      0 |      16 |      66 |      63 |     19 |      16 |
| [src/benchmark](src-benchmark.md)   |       1 |         1,971 / 1,971 |       0 |      0 |       5 |       2 |       7 |      0 |       0 |
| [src/build](src-build.md)           |       4 |         7,122 / 7,122 |       0 |      0 |      16 |      14 |      26 |      4 |       4 |
| [src/cli](src-cli.md)               |       5 |         9,162 / 9,162 |       0 |      0 |      13 |      21 |      30 |      4 |       5 |
| [src/config](src-config.md)         |       8 |       15,822 / 15,822 |       0 |      0 |       7 |      26 |      31 |      2 |       0 |
| [src/datastruct](src-datastruct.md) |       4 |         7,421 / 7,421 |       0 |      0 |       0 |       5 |       3 |      2 |       6 |
| [src/font](src-font.md)             |      17 |       31,229 / 31,229 |       0 |      0 |       8 |      38 |      32 |     14 |       5 |
| [src/input](src-input.md)           |       6 |       11,431 / 11,431 |       0 |      1 |       2 |       6 |       7 |      2 |       6 |
| [src/inspector](src-inspector.md)   |       3 |         6,623 / 6,623 |       0 |      0 |       1 |       3 |       2 |      2 |       0 |
| [src/lib](src-lib.md)               |       2 |         3,584 / 3,584 |       0 |      0 |       3 |      12 |      13 |      2 |       3 |
| [src/os](src-os.md)                 |       2 |         4,210 / 4,210 |       0 |      0 |       2 |      15 |      15 |      2 |       0 |
| [src/renderer](src-renderer.md)     |       6 |       11,567 / 11,567 |       0 |      2 |       3 |       9 |      13 |      1 |       6 |
| [src/simd](src-simd.md)             |       1 |         1,282 / 1,282 |       0 |      0 |       1 |       1 |       2 |      0 |       1 |
| [src/stb](src-stb.md)               |       1 |         1,785 / 1,785 |       0 |      0 |       1 |       1 |       2 |      0 |       0 |
| [src/terminal](src-terminal.md)     |      88 |     173,694 / 173,694 |       0 |      2 |      36 |      72 |      90 |     20 |      24 |
| [src/terminfo](src-terminfo.md)     |       1 |             673 / 673 |       0 |      0 |       0 |       0 |       0 |      0 |       0 |
| [src/termio](src-termio.md)         |       4 |         7,124 / 7,124 |       0 |      0 |       4 |      10 |      14 |      0 |       2 |
| [src/unicode](src-unicode.md)       |       1 |             869 / 869 |       0 |      0 |       0 |       1 |       1 |      0 |       1 |
| **Total**                           | **215** | **403,246 / 403,246** |   **0** | **12** | **140** | **345** | **404** | **93** | **103** |

## Highest-priority gaps

All 12 surviving HIGH findings, by area. Each is CONFIRMED (neither lens could refute it).

**[include](include.md)**

- **S1-F1** — `rustty_command_s` elements handed to C are not the header's 4× `const char*` layout.
- **S3-F1** — `IMAGE_DATA_FORMAT`/`COMPRESSION` write a 1-byte Rust enum into the caller's 4-byte C enum.

**[pkg](pkg.md)**

- **S34-F1** — x86 AVX2 gate drops the XCR0 (`xgetbv`) OS-support check and the LZCNT/BMI/BMI2 requirements.
- **S34-F4** — PowerPC target detection (ppc8/ppc9/ppc10) has no counterpart; powerpc falls to `return 0`.
- **S34-F5** — s390x target detection (z14/z15) has no counterpart; s390x falls to `return 0`.
- **S34-F6** — RISC-V target detection (rvv) has no counterpart; riscv falls to `return 0`.

**[src](src.md)**

- **S43-F1** — `global.init` does not re-sync the captured environment after `ensureLocale()`.

**[src/input](src-input.md)**

- **S103-F1** — `Command.C` is not a C-ABI struct but is handed to C as `rustty_command_s` (command-palette-entry ABI mismatch).

**[src/renderer](src-renderer.md)**

- **S117-F1** — `shadertoy::Uniforms` drops the per-field `align(16)`; the GPU byte layout differs from the std140 block the custom shaders read.
- **S118-F1** — IOSurface Display P3 colorspace never set on the render target.

**[src/terminal](src-terminal.md)**

- **S151-F1** — `codepoint_map` is discarded when Terminal/Screen/PageList formatters delegate downward (`clipboard-codepoint-map` has no effect).
- **S198-F1** — search refresh timer is one-shot in rustty: the callback's re-arm is immediately cancelled by `apply_action(Disarm)`.

## How to act on this

Each area file lists its findings under High/Medium/Low headings, and every finding carries **file:line on both sides** (oracle path:lines and rustty crate path:lines), the evidence that established it, and a suggested fix — enough to open a work item without re-deriving the diff.

**CONFIRMED** findings are actionable as written. **DISPUTED** findings (93) survived only one lens and need a **human tie-break** before any code changes: read the finding and the recorded refutation together and decide which reading is right. **REFUTED** findings are kept for the audit trail only — do not act on them.

There are **0 UNVERIFIED** findings, so no verification pass is outstanding; if a future re-run of a shard exceeds its verification cap, its UNVERIFIED items are labelled as such in the area file and must be double-verified before they are trusted.
