//! Build script for glslang-sys (glslang GLSL->SPIR-V compiler).
//!
//! PORT STATUS: stub
//!
//! TODO(R3 — self-contained, no ghostty dependency):
//!   1. Vendor the upstream C/C++ sources for `glslang` into this crate
//!      (copied in-tree; do NOT reference /Users/gael/projects/ghostty).
//!   2. Compile them with the `cc` crate (matching the reference's build flags).
//!   3. Generate Rust bindings with `bindgen` (or hand-write a thin FFI surface).
//!   4. Emit the appropriate `cargo:rustc-link-*` directives.
//!
//! No C is compiled yet — this is a scaffold so `cargo build --workspace` is
//! green with zero native toolchain requirements.

fn main() {
    // Intentionally empty for the scaffold. Re-run only if this script changes.
    println!("cargo:rerun-if-changed=build.rs");
}
