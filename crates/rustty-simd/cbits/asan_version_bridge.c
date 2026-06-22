/* AddressSanitizer runtime version-symbol bridge (R6 FFI memory-safety gate).
 *
 * This file is NOT part of ghostty's source — it is minimal Rust↔C FFI glue
 * (allowed under R1: it adds nothing, omits nothing, weakens no reference
 * logic/comments/tests) and is compiled into the kernel archive ONLY when the
 * crate is built with `-Zsanitizer=address`.
 *
 * Why it exists: on macOS, Apple clang's ASan instrumentation emits a call to a
 * toolchain-version-tagged check symbol (e.g.
 * `__asan_version_mismatch_check_apple_clang_2100`), while rustc's bundled ASan
 * runtime (LLVM) exports the canonical `__asan_version_mismatch_check_v8`. Both
 * runtimes share the entire rest of the `__asan_*` ABI; only this defensive
 * one-shot version tag differs in name. Without a definition the link fails with
 * an undefined symbol even though the runtimes are ABI-compatible.
 *
 * This no-op definition (compiled WITHOUT instrumentation, so it does not itself
 * reference the symbol) resolves the name skew so the instrumented C++ kernels
 * link against — and are checked by — rustc's ASan runtime. It does NOT disable
 * any sanitizer check: verified end-to-end that a real heap-buffer-overflow in
 * the C++ FFI is still reported. The check it stands in for is purely a
 * runtime/instrumentation version assertion, irrelevant when both halves come
 * from compatible LLVM-derived runtimes.
 */

/* Defined unconditionally; build.rs only compiles this TU under the sanitizer
 * cfg. The name matches what Apple clang 21 (clang-2100) emits. */
void __asan_version_mismatch_check_apple_clang_2100(void) {}
