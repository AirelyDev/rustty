# Phase 2 — Batch 4 Report: `rustty-simd` (verified parity)

**Verdict: GREEN.** All gates green (real exit codes), including AddressSanitizer
over the Rust↔C++ FFI boundary. `ci.sh` → PASS (exit 0).

- **crate:** `rustty-simd` (ports ghostty `src/simd/`)
- **tests_ported:** 23 / 23 (contract met, name-sets match)
- **g3b_missing:** 0
- **asan_clean:** true (0 ASan errors; C++ kernels instrumented)
- **branch:** `phase-1-parity-gates`

## What this batch did

1. **lib.rs PORT STATUS: done.** Flips `rustty-simd` to the claimed-done state, which
   makes every parity gate bind strictly for this crate (R5).

2. **Looped to green** (each with a real captured exit code, never `| tail`):
   - `cargo test -p rustty-simd` → 23 passed, 0 failed (exit 0).
   - `cargo test --workspace` → exit 0.
   - `check_tests_ported --crate simd --strict` → 23/23, name-sets match (exit 0).
   - `check_assertions --crate simd --strict` → rust=102 ≥ ref=98 (exit 0).
   - `check_comments_fidelity --crate simd --strict` → rust=222 ≥ ref=139 (exit 0).
   - `check_comment_coverage --crate simd` (G3b) → distinctive=79, **missing=0**, review=0.
   - `cargo clippy -p rustty-simd --all-targets -- -D warnings` → exit 0.
   - `cargo fmt --all --check` → exit 0.
   - `check_acyclic.sh` (G7) → 0 cross-crate SCCs (exit 0).
   - **ASan:** `RUSTFLAGS="-Zsanitizer=address" cargo +nightly test -p rustty-simd --target aarch64-apple-darwin`
     → 23 passed, **0 ASan errors** (exit 0). The kernel archive carries 375 asan
     symbol refs (`_asan.module_ctor`, `___asan_report_load*`) — the C++ FFI is
     genuinely instrumented, not a blind spot.

3. **Full `ci.sh` → PASS** (exit 0): fmt, clippy, build, test all green; G7/G4/G4-uni
   HARD gates green (**G4-uni: 0 diffs** over 0..=0x10FFFF + 8192 grapheme entries);
   all PROGRESS gates green.

## R3 (self-contained) verification

The four C++ kernels + two headers in `cbits/simd/` are **byte-identical** to
ghostty's `src/simd/` (verified `diff` clean for `base64.cpp`,
`codepoint_width.cpp`, `index_of.cpp`, `index_of.h`, `vt.cpp`, `vt.h`). They are
cc-built against highway-sys + simdutf-sys (same C libs, no pure-Rust substitution).
No ghostty Cargo dependency.

## R6 (FFI memory-safety gate) — the load-bearing change

Miri cannot interpret the foreign C++ SIMD kernels, so the R6 gate for this crate
is **AddressSanitizer**, with BOTH halves instrumented.

- **`build.rs` now honors the sanitizer cfg.** Added
  `rustflags_request_address_sanitizer()`, which parses `CARGO_ENCODED_RUSTFLAGS`
  (and legacy `RUSTFLAGS`) for `-Zsanitizer=address`. When present, the copied
  ghostty C++ kernels are compiled with `-fsanitize=address` (+
  `-fno-omit-frame-pointer`). `cc` does not auto-propagate this from RUSTFLAGS, so
  without this the C++ kernels would be uninstrumented blind spots across the FFI
  boundary.

- **macOS toolchain version-symbol skew, bridged (not papered over).** Under ASan
  on this box, Apple clang 21 emits a call to
  `__asan_version_mismatch_check_apple_clang_2100`, while rustc nightly's bundled
  ASan runtime exports the canonical `__asan_version_mismatch_check_v8`. The two
  runtimes share the entire rest of the `__asan_*` ABI; only this one defensive
  version-tag symbol differs in name, causing an undefined-symbol link error.
  `cbits/asan_version_bridge.c` provides a no-op definition of the Apple symbol,
  compiled **without** instrumentation as a separate TU **only** under the
  sanitizer build, so the instrumented C++ links against — and is checked by —
  rustc's ASan runtime.
  - This is minimal Rust↔C FFI glue under R1: it adds nothing, omits nothing, and
    weakens no reference logic/comment/test. It is **not** a sanitizer-disable: an
    independent end-to-end check confirmed a real heap-buffer-overflow in
    instrumented C++ is still reported (`heap-buffer-overflow ... WRITE of size 1`)
    using rustc's runtime through the bridge.

- **FFI leak detection deferred to Linux CI.** LSan/valgrind are unavailable on
  arm64 macOS, so FFI leak checks are deferred to the Linux CI box — documented,
  never skipped (DoP §G6; mirrored by `check_leaks.sh`'s FFI-crate branch).

## Test name fixes (G1 strict)

Once `PORT STATUS: done` bound G1 strictly, the test name-set had to match the
reference name-for-name (via the DoP §G1 slug convention). Renamed 7 fns to
accepted slug forms of the verbatim reference names (independently confirmed
against ghostty `src/simd/*.zig`):

| reference test name | ported fn |
| --- | --- |
| `"base64 maxLen"` | `base64_maxLen` |
| `"codepointWidth basic"` | `codepointWidth_basic` |
| `"decode incomplete UTF-8"` | `decode_incomplete_utf_8` |
| `"decode invalid UTF-8"` | `decode_invalid_utf_8` |
| `"Table 3-8: non-shortest form sequences"` | `Table_3_8_non_shortest_form_sequences` |
| `"Table 3-9: surrogate sequences"` | `Table_3_9_surrogate_sequences` |
| `"Table 3-10: other ill-formed sequences"` | `Table_3_10_other_ill_formed_sequences` |
| `"Table 3-11: truncated sequences"` | `Table_3_11_truncated_sequences` |

`#![allow(non_snake_case)]` was added to the three affected test modules with a
comment noting the slug carries the reference name verbatim. No assertions, logic,
or comments were altered.

## Gate summary

| gate | result |
| --- | --- |
| cargo fmt --check | GREEN (exit 0) |
| cargo clippy -D warnings | GREEN (exit 0) |
| cargo build --workspace | GREEN (exit 0) |
| cargo test --workspace | GREEN (exit 0) |
| G1 tests ported | GREEN (23/23, names match) |
| G2 assertions | GREEN (rust=102 ≥ ref=98) |
| G3 comments fidelity | GREEN (rust=222 ≥ ref=139) |
| G3b comment coverage | GREEN (distinctive=79, missing=0) |
| G4 differential | GREEN |
| G4-uni unicode tables | GREEN (0 diffs) |
| G7 acyclic | GREEN (0 cross-crate SCCs) |
| G6 ASan (FFI) | GREEN (0 ASan errors; C++ instrumented) |

**Blockers: none.**
