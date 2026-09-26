# CMake Support for librustty-vt

The top-level `CMakeLists.txt` wraps the Cargo build system so that CMake
projects can consume librustty-vt without invoking `cargo build` manually.
Running `cmake --build` triggers `cargo build -p rustty-vt-c` automatically.

This means downstream projects do require a working Rust toolchain (`cargo`)
on `PATH` to build, but don't need to know any Cargo-specific details.

## Using FetchContent (recommended)

Add the following to your project's `CMakeLists.txt`:

```cmake
include(FetchContent)
FetchContent_Declare(rustty
    GIT_REPOSITORY https://github.com/AirelyDev/rustty.git
    GIT_TAG main
)
FetchContent_MakeAvailable(rustty)

add_executable(myapp main.c)
target_link_libraries(myapp PRIVATE rustty-vt)
```

This fetches the Rustty source, builds librustty-vt via Cargo during your
CMake build, and links it into your target. Headers are added to the
include path automatically.

### Using a local checkout

If you already have the Rustty source checked out, skip the download by
pointing CMake at it:

```shell-session
cmake -B build -DFETCHCONTENT_SOURCE_DIR_RUSTTY=/path/to/rustty
cmake --build build
```

## Using find_package (install-based)

Build and install librustty-vt first:

```shell-session
cd /path/to/rustty
cmake -B build
cmake --build build
cmake --install build --prefix /usr/local
```

Then in your project:

```cmake
find_package(rustty-vt REQUIRED)

add_executable(myapp main.c)
target_link_libraries(myapp PRIVATE rustty-vt::rustty-vt)
```

## Static linking

The `rustty-vt-static` target links librustty-vt as a static archive. Because
Cargo's static libraries do not bundle native platform dependencies, the
consumer must link the system libraries Rust's std relies on; these are set on
the target's `INTERFACE_LINK_LIBRARIES` automatically. No gettext package is
needed: on macOS the archive carries its own copy of libintl, and on Linux
glibc provides these symbols.

## SIMD

librustty-vt builds with SIMD acceleration **ON by default** via the
`RUSTTY_VT_SIMD` CMake option, matching upstream Ghostty's semantics (Ghostty's
`simd` build option defaults true). This applies to the shared and cross
targets and the `find_package`/FetchContent default.

Turn it off for a minimal static archive with no vendored C++ SIMD runtime
(highway/simdutf) — the analog of Ghostty's `-Dsimd=false`:

```cmake
set(RUSTTY_VT_SIMD OFF CACHE BOOL "" FORCE)   # before FetchContent_MakeAvailable
```

or `-DRUSTTY_VT_SIMD=OFF` on the command line. The `c-vt-cmake-static` example
does exactly this. SIMD does not change library output, only performance.

## Cross-compilation

For cross-compiling to a different Rust target triple, use
`rustty_vt_add_target()` after `FetchContent_MakeAvailable`:

```cmake
FetchContent_MakeAvailable(rustty)
rustty_vt_add_target(NAME linux-amd64 RUST_TARGET x86_64-unknown-linux-gnu)

add_executable(myapp main.c)
target_link_libraries(myapp PRIVATE rustty-vt-static-linux-amd64)
```

The Rust std for the target must be installed (`rustup target add
<RUST_TARGET>`).

### Using zig cc as the C/CXX compiler

When cross-compiling, the host C compiler can't link binaries for the
target platform. `RusttyCrossCompiler.cmake` provides
`rustty_cross_compiler()` to set up `zig cc` as the C/CXX compiler for
the cross target. It creates wrapper scripts (shell on Unix, `.cmd` on
Windows) and configures `CMAKE_C_COMPILER`, `CMAKE_CXX_COMPILER`, and
`CMAKE_SYSTEM_NAME`.

The module is self-contained — copy it into your project (e.g. to
`cmake/`) and include it directly. It cannot be consumed via
FetchContent because it must run before `project()`, but
`FetchContent_MakeAvailable` triggers `project()` internally:

```cmake
cmake_minimum_required(VERSION 3.19)

include(cmake/RusttyCrossCompiler.cmake)
rustty_cross_compiler(RUST_TARGET x86_64-unknown-linux-gnu)

project(myapp LANGUAGES C CXX)

FetchContent_MakeAvailable(rustty)
rustty_vt_add_target(NAME linux-amd64 RUST_TARGET x86_64-unknown-linux-gnu)

add_executable(myapp main.c)
target_link_libraries(myapp PRIVATE rustty-vt-static-linux-amd64)
```

See `example/c-vt-cmake-cross/` for a complete working example.
