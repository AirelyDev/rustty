# Example librustty Projects

Each example is a standalone project with its own `build.sh`, `README.md`,
and `src/main.c` (or `.cpp` for the C++ examples). Examples build the ported C directly against
`librustty_vt_c` + `include/rustty/vt.h` via `clang` — there is no
`build.zig.zon` and no per-example Cargo manifest. New examples are picked up
automatically by CI, which discovers `example/*/build.sh`, so no workflow file
edits are needed when adding a new example.

## Adding a New Example

1. Copy an existing example directory (e.g., `c-vt-encode-focus/`) as a
   starting point.
2. Update `build.sh`: it is a common `sh` template — keep it consistent with
   the others. It compiles `"$here"/src/*.c` against
   `target/release/librustty_vt_c.dylib` under the oracle-parity profile
   (`--features "identity-oracle simd"`) and runs the result. The only
   per-example variance is the source under `src/`; the output binary name is
   derived from the directory, so nothing in `build.sh` usually needs editing.
3. Swap the API symbols against `include/rustty/vt.h` for the surface your
   example exercises (`rustty_*` functions and `Rustty*` types).
4. Write a `README.md` following the existing format.

## Doxygen Snippet Tags

Example source files use Doxygen `@snippet` tags so the corresponding
header in `include/rustty/vt/` can reference them. Wrap the relevant
code with `//! [snippet-name]` markers:

```c
//! [my-snippet]
int main() { ... }
//! [my-snippet]
```

The header then uses `@snippet <dir>/src/main.c my-snippet` instead of
inline `@code` blocks. Never duplicate example code inline in the
headers — always use `@snippet`. When modifying example code, keep the
snippet markers in sync with the headers in `include/rustty/vt/`.

## Conventions

- Directory names use hyphens: `c-vt-encode-focus`. The `build.sh` derives
  the output binary name from the directory basename.
- All C examples link `librustty_vt_c` (via `-lrustty_vt_c -L target/release`)
  and include `-I include`. Rustty emits a standard C library
  (`librustty_vt_c` + `include/rustty/vt.h`) usable with any C tooling.
- `build.sh` files follow a common template — keep them consistent.
