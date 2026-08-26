# Examples

Standalone projects demonstrating the Rustty library APIs.
The directories starting with `c-` use the C API, the ones starting with
`cpp-` use it from C++, and the ones starting with `wasm-` load the
`wasm32-unknown-unknown` build of the same library in a browser.

Every `c-`/`cpp-` example is built and run by its own `build.sh`, from
within the respective example directory.

## Running an Example

```shell-session
cd example/<dir>
./build.sh
```

## The `build.sh` convention

Upstream Ghostty drives its examples with `zig build` / `zig build run`,
using the Zig build system even for the C examples so they can depend
directly on the source tree. Rustty deliberately does **not** reproduce
that: it has no Zig toolchain, and a per-example Cargo manifest would
pull each example into the workspace graph, which is the opposite of
what an example is meant to prove. Instead each example ships one small
`sh` script that invokes `clang` against the _published_ artifacts —
exactly what a downstream embedder has.

`build.sh` is a common template, not a per-example program. It:

1. resolves the repo root from its own location, so it works from any
   working directory;
2. runs `cargo build -p rustty-vt-c --release --features "identity-oracle simd"`
   **unconditionally**, so the example can never link a stale or
   differently-configured `librustty_vt_c`;
3. compiles `src/*.c` (or `src/*.cpp`) with `-I include` and links
   `-lrustty_vt_c`;
4. runs the resulting binary.

The output binary name is derived from the directory, so adding an
example normally means copying a directory and editing only `src/`.
The CMake examples (`c-vt-cmake*`) are the deliberate exception: their
`build.sh` drives `cmake` instead of `clang` directly, because what they
demonstrate _is_ the CMake integration.

CI discovers examples by globbing `example/*/build.sh`, so a new example
needs no workflow edit.

See [`AGENTS.md`](AGENTS.md) for the full checklist on adding one,
including the Doxygen `@snippet` markers that let the headers in
`include/rustty/vt/` reference example code instead of duplicating it.
