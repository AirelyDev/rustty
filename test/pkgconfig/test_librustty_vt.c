/* Minimal librustty-vt consumer, used to prove the installed pkg-config
 * metadata is actually linkable.
 *
 * PORTS: ghostty nix/test-src/test_libghostty_vt.c — the same "include the
 * public header, call one function, print the answer" smoke test its
 * `test-lib-vt-pkgconfig` CI job compiles twice (shared and static) against
 * whatever `pkg-config --cflags --libs` returns.
 *
 * Deliberately trivial. The point is not to exercise the VT engine — the
 * workspace test suite does that — but to prove that a plain C program with
 * no knowledge of cargo, of the crate name, or of the vendored C++ deps can
 * find the header, resolve every symbol, and run.
 */
#include <rustty/vt.h>
#include <stdio.h>

int main(void) {
  bool simd = false;
  RusttyResult r = rustty_build_info(RUSTTY_BUILD_INFO_SIMD, &simd);
  if (r != RUSTTY_SUCCESS) return 1;
  printf("SIMD: %s\n", simd ? "yes" : "no");
  return 0;
}
