# c-vt-cmake-cross

Demonstrates using `rustty_vt_add_target()` to cross-compile
librustty-vt with static linking. The target OS is chosen automatically:

| Host    | Target          |
| ------- | --------------- |
| Linux   | Windows (MinGW) |
| Windows | Linux (glibc)   |
| macOS   | Linux (glibc)   |

Override with `-DRUST_TARGET=...` if needed.

## Building

```shell-session
cd example/c-vt-cmake-cross
cmake -B build -DFETCHCONTENT_SOURCE_DIR_RUSTTY=../..
cmake --build build
file build/c_vt_cmake_cross
```

## Requirements

Cross-compiling with Cargo needs more setup than the Zig-based upstream, which
cross-compiles C dependencies transparently:

- The target's Rust standard library must be installed:
  `rustup target add <RUST_TARGET>` (e.g. `x86_64-unknown-linux-gnu`).
- Linking the final C executable for the foreign target uses `zig cc` (set up
  by `RusttyCrossCompiler.cmake`), so `zig` must be on `PATH`.
- Cargo must be able to build librustty-vt's `-sys` C dependencies for the
  target. Point Cargo's target C toolchain at `zig cc` via environment (e.g.
  `CC_<target>=zig cc -target <zig-triple>` and the matching `CARGO_TARGET_
<TARGET>_LINKER`), or use [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild).

The CMake configuration (target derivation, cross compiler, imported targets)
is complete; the toolchain setup above is environment-specific.
