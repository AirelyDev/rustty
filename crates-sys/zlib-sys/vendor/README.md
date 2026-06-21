# Vendored zlib C sources (R3)

This directory vendors C source copied from the **same** upstream library the
reference (ghostty) links: **zlib 1.3.1** (`ZLIB_VERSION "1.3.1"`, confirmed
against the read-only oracle's resolved `zlib.h` in
`/Users/gael/projects/ghostty/.zig-cache/.../zlib.h`).

Per `.port/PORT_RULES.md` R3 the port is **self-contained**: these sources are
copied in-tree and built standalone with the `cc` crate (see `../build.rs`). The
crate has **zero** build- or run-time dependency on the ghostty tree; the
reference is an external oracle only.

## Scope of this vendor slice

This is the **vendor-pattern proof slice** for Phase-1: it vendors zlib's
checksum unit (`adler32`) — a small, fully self-contained translation unit that
depends only on `zlib.h`/`zconf.h` and produces bit-exact, known-answer output.
It proves the end-to-end R3 toolchain (copied C -> `cc` compile -> `links = "z"`
-> Rust FFI -> `cargo test` linking and exercising the symbol). The remaining
zlib translation units (`deflate.c`, `inflate.c`, `crc32.c`, `zutil.c`, ...) are
vendored into this same directory and added to the `cc` build as the crates that
need them (`libpng-sys`, image decode) are carved, with no change to the pattern.

Files:
- `adler32.c`  — upstream zlib 1.3.1 Adler-32 checksum (verbatim algorithm).
- `zlib.h`     — minimal public surface needed by `adler32.c` and the FFI
                 (the subset the proof slice consumes: `z_*` typedefs +
                 `adler32`/`adler32_z`/`adler32_combine` prototypes + version).
- `zconf.h`    — upstream configuration header subset (`z_size_t`, `OF`,
                 `ZEXTERN`/`ZEXPORT` macros, `Z_U4`).
