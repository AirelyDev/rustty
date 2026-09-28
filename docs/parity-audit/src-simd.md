# Parity audit — src/simd

Audited all 6 oracle units in `src/simd` (base64.zig, base64_scalar.zig, codepoint_width.zig, index_of.zig, main.zig, vt.zig): 1282 lines read of 1282 lines in scope (100% — no partial units). All 6 units are **ported**, with full test parity (7 base64 tests, 20 vt tests, the codepointWidth basic test, the indexOf test, refAllDecls) plus rustty-added regressions.

3 findings were raised: **2 CONFIRMED** (1 medium, 1 low), **0 DISPUTED**, **1 REFUTED**, 0 unverified. The single most important gap is **S120-F1**: on the wasm32/scalar lane, `codepointWidth` returns `1` for every `cp <= 0xFF` because the C++ kernel's early-out was hoisted above the SIMD gate, so C0/C1 controls and TAB get width 1 instead of the oracle's uucode width 0. Native/SIMD builds are byte-identical and machine-checked.

## Coverage

| Oracle unit                                | Rustty counterpart(s)                       | Status | Note                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------ | ------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/simd/base64.zig` (full file)          | `crates/rustty-simd/src/base64.rs`          | ported | `maxLen`/`decode`/`decodeScalar`/`scalarInput`/`Padding`/`decodeStrict`/`Streaming{maxLen,feed,group,validPartialChar,finish}` and both externs (`rustty_simd_base64_max_length`/`decode`, `cbits/simd/base64.cpp`) present with identical branches; all 7 tests ported with the same assertions (base64.rs:312-538). Only deviation: `maxLenScalar`'s `log.warn` dropped (S120-F2). SIMD lane additionally skipped under `cfg(miri)`/wasm32 — the scalar lane is the oracle's `simd=false` code. |
| `src/simd/base64_scalar.zig` (full file)   | `crates/rustty-simd/src/base64_scalar.rs`   | ported | `scalar_decoder` (LazyLock instead of comptime const, same init asserts), `isAlphabetChar`, `Base64Decoder{init, calcSizeUpperBound, calcSizeForSlice, decode}` incl. the commented-out `InvalidPadding` check, Error set with `NoSpaceLeft`. `acc` u12→u16 is bit-identical for the bits observed (low 12 bits agree; output byte drawn from bits `acc_len..acc_len+7 <= 11`).                                                                                                                   |
| `src/simd/codepoint_width.zig` (full file) | `crates/rustty-simd/src/codepoint_width.rs` | ported | `codepointWidth` + extern + the "codepointWidth basic" test (9 assertions) ported; the commented-out "matches uucode" test carried as a comment. rustty adds an exhaustive kernel-agreement test. The added `cp <= 0xFF => 1` hoist is identical on the SIMD lane but diverges from the oracle's `simd=false` (wasm) uucode lane — S120-F1.                                                                                                                                                       |
| `src/simd/index_of.zig` (full file)        | `crates/rustty-simd/src/index_of.rs`        | ported | `indexOf` (null ↔ `None` on `result == len`), `indexOfScalar`, extern, and the 4-assertion "indexOf" test all ported.                                                                                                                                                                                                                                                                                                                                                                             |
| `src/simd/main.zig` (full file)            | `crates/rustty-simd/src/lib.rs`             | ported | Module re-exports (`base64`, `index_of`, `vt`, `codepointWidth`), `lanes(T)` ported as `const fn lanes(size_of::<T>())` via `native_vector_bits()` (aarch64 128 / x86 AVX2 256 / SSE2 128 / wasm simd128 128 / else `None`, ×2), `refAllDecls` ported as `ref_all_decls`. `build_options.simd` runtime toggle is Zig build plumbing; replaced by the const `SIMD=true` + cfg gates.                                                                                                               |
| `src/simd/vt.zig` (full file)              | `crates/rustty-simd/src/vt.rs`              | ported | `DecodeResult`, `utf8DecodeUntilControlSeq`, `utf8DecodeUntilControlSeqScalar` (ASCII vector fast path, lead-byte classification, Table 3-7 cb1 ranges, ESC-vs-EOF truncation split) ported line-for-line; all 20 oracle tests present with the same assertions (Tables 3-8..3-11 included). The differential fuzz test uses a different PRNG (S120-F3, refuted). rustty adds tight-buffer ASan regressions and a scalar-direct test.                                                             |

**Partial units:** none. Every oracle unit in scope was read end to end.

## Findings

### High

None.

### Medium

#### S120-F1 — Scalar (wasm32) codepointWidth returns 1 for cp <= 0xFF where oracle's simd=false lane returns uucode width (0 for C0/C1 controls)

- **Status:** ✅ CONFIRMED (both verifier lenses failed to refute; both high confidence)
- **Kind:** behavioral-difference
- **Severity:** medium (corrected severity: medium)
- **Oracle:** `src/simd/codepoint_width.zig:7-12`
- **Rustty:** `crates/rustty-simd/src/codepoint_width.rs:22-44`

**Description.** The oracle's non-SIMD path (taken on wasm32 freestanding, where `src/build/Config.zig:246` forces `simd=false`) returns `uucode.get(.width, cp)` for every `cp <= max_code_point`, which per `src/build/uucode_config.zig:99-105` is 0 for zero-in-grapheme code points such as U+0000..U+001F, TAB, U+007F..U+009F. rustty's `codepointWidth` hoists the C++ kernel's `if (input <= 0xFF) return 1;` **above** the SIMD gate, so on wasm32 (where the SIMD branch is cfg'd off) it returns 1 for those code points instead of the `rustty_unicode::props_uucode` width (which is 0 for NUL/TAB per `props_uucode.rs:127-129`). Native/SIMD builds are unaffected (the kernel already returns 1 for `<= 0xFF`, machine-checked by `codepointWidth_matches_kernel`).

**Evidence.** Oracle `codepoint_width.zig:8-11`: `if (comptime options.simd) return ghostty_simd_codepoint_width(cp); ... return uucode.get(.width, cp)`. rustty `codepoint_width.rs:29-31` `if cp <= 0xFF { return 1; }` precedes the `#[cfg(not(target_arch = "wasm32"))] if SIMD` branch at :32-34, so the wasm32 scalar lane never consults `props_uucode` for `cp <= 0xFF`. `rustty-unicode` `props_uucode.rs:127-129` documents width 0 for U+0000 and U+0009. The hoist's guard test `codepointWidth_matches_kernel` (codepoint_width.rs:97-110) is itself `#[cfg(not(target_arch = "wasm32"))]` and early-returns unless `SIMD`, so it does not cover the divergent lane; a workspace grep found no compensating wasm-only path.

**Suggested fix.** Move the `cp <= 0xFF` early-out inside the `#[cfg(not(target_arch = "wasm32"))] if SIMD` block (it is only a hoist of the kernel's first statement), so the scalar lane keeps `if cp > MAX_CODE_POINT {1} else {props_uucode::get(cp).width}` unchanged.

### Low

#### S120-F2 — maxLenScalar drops the `log.warn("failed to calculate base64 size for payload")` on error

- **Status:** ✅ CONFIRMED (both verifier lenses failed to refute)
- **Kind:** behavioral-difference
- **Severity:** low (corrected severity: low)
- **Oracle:** `src/simd/base64.zig:18-23`
- **Rustty:** `crates/rustty-simd/src/base64.rs:38-49`

**Description.** The oracle logs a warning (scope `.simd_base64`) when `calcSizeForSlice` fails before returning 0. rustty keeps the return-0 behaviour but the log call is only a comment; no warning is emitted on the scalar lane (wasm32 / miri).

**Evidence.** Oracle `base64.zig:19-22` `catch |err| { log.warn("failed to calculate base64 size for payload: {}", .{err}); return 0; }`; rustty `base64.rs:44-47` `Err(_err) => { // log.warn(...)` → `0`, with `// const log = ...` at :7. A workspace grep for the literal "failed to calculate base64 size" hits only that comment, and `rustty-simd`'s Cargo.toml has no `log` dependency. The semantic lens notes this is a workspace-wide port convention (commented-out `std.log.scoped` in rustty-vt `dcs.rs:14`, `mem.rs:29`, `parser.rs:26`), i.e. systemic rather than a local defect; the return value is unchanged.

**Suggested fix.** Route through the workspace log facility (`log::warn!` with target `"simd_base64"`) as other ported modules do.

### Unverified (over the verification cap)

None — every finding was fully verified by both lenses.

## Performance-only differences (not findings)

- `codepoint_width.rs:29-31` hoists the C++ kernel's `input <= 0xFF -> 1` early-out into Rust (skips FFI + highway dispatch); byte-identical on the SIMD lane and guarded by the exhaustive `codepointWidth_matches_kernel` test (the wasm32 scalar-lane divergence is reported separately as S120-F1).
- `codepoint_width.rs:21` `#[inline]` on `codepointWidth` (cross-crate, no LTO) — no observable change.
- `base64_scalar.rs:14` `scalar_decoder` is a `LazyLock` singleton (init on first access) instead of a comptime const; same tables, same asserts.
- `vt.rs:87-103` the scalar ASCII vector fast path is expressed as slice `any`/loop over a const `VL` chunk instead of `@Vector` intrinsics; same consumed/decoded results.
- `base64.rs` / `vt.rs` / `index_of.rs` add `!cfg!(miri)` and `cfg(not(target_arch = "wasm32"))` gates that route to the scalar lane (the oracle's `simd=false` code) where the C++ kernel cannot run.
- `lib.rs:58-117` `lanes` takes `size_of::<T>()` and derives native width from `cfg(target_feature)` instead of `std.simd.suggestVectorLength` — same values on aarch64/x86-64/wasm+simd128, `None` elsewhere.

## Refuted candidates

- **S120-F3 — Differential fuzz test "decode simd matches scalar" uses SplitMix64 instead of `std.Random.DefaultPrng` (Xoshiro256++) seeded `0xf00dface`** (`src/simd/vt.zig:157-203` vs `crates/rustty-simd/src/vt.rs:616-686`): refuted by both lenses — the test is present with identical iteration count (10_000), buffer size (257), style mixture and byte-class weights, the same fixed seed, the same skip-on-non-SIMD gate, and exactly the same three assertions (consumed, decoded, decoded-slice equality); only the PRNG algorithm differs, changing which random bytes are drawn but neither the property asserted nor the strength of the assertions. The divergence is documented in place (vt.rs:605-615) as avoiding a layering-inverting dev-dep on rustty-synthetic.
