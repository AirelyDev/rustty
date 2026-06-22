// simdutf-sys link-test shim (rustty).
//
// simdutf's public API lives in the C++ `simdutf` namespace, so its symbols are
// name-mangled and not directly callable from Rust FFI. This translation unit
// exposes a single `extern "C"` wrapper so the crate's link test can exercise
// the compiled-and-linked simdutf archive end-to-end (R3 vendor-toolchain
// proof). It is NOT part of the reference; it only proves the vendored library
// linked. The real FFI surface rustty-simd needs is its own copied
// src/simd/*.cpp (e.g. base64.cpp), which call simdutf directly in C++.

#include <simdutf.h>
#include <stddef.h>

extern "C" {

// Thin wrapper over simdutf::validate_utf8 (returns 1 if the input is valid
// UTF-8, else 0). Used only by the simdutf-sys link test.
int simdutf_sys_validate_utf8(const char* buf, size_t len) {
  return simdutf::validate_utf8(buf, len) ? 1 : 0;
}

}  // extern "C"
