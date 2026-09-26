# c-vt-cmake-static

Demonstrates consuming librustty-vt as a **static** library from a CMake
project using `FetchContent`. Creates a terminal, writes VT sequences into
it, and formats the screen contents as plain text.

## Building

```shell-session
cd example/c-vt-cmake-static
cmake -B build
cmake --build build
./build/c_vt_cmake_static
```

To build against a local checkout instead of fetching from GitHub:

```shell-session
cmake -B build -DFETCHCONTENT_SOURCE_DIR_RUSTTY=../..
cmake --build build
```

## Platform note

No gettext package is needed: on **macOS** the static archive carries its
own copy of libintl, and on **Linux** glibc provides these symbols.

Unlike upstream Ghostty (which passes `-Dsimd=false` here), rustty-vt-c's
`simd` cargo feature is OFF by default, so no extra flag is needed for a
minimal-dependency static archive.
