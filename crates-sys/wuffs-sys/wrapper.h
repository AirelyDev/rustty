/* wuffs-sys bindgen surface.
 *
 * Mirrors the reference's binding import (ghostty/pkg/wuffs/src/c.zig) exactly:
 * the same single-file header the reference's `c.zig` @cIncludes. Wuffs ships as
 * a stb-style "single file C library"; WITHOUT `WUFFS_IMPLEMENTATION` it acts as
 * a header (declarations + `static inline` helpers only), which is precisely
 * what bindgen needs — the `@cImport` in c.zig likewise omits
 * `WUFFS_IMPLEMENTATION` and only @cDefine's the module gate list.
 *
 * The `WUFFS_CONFIG__MODULE__*` defines are supplied on the bindgen command line
 * (build.rs `MODULE_DEFINES`), matching c.zig's `for (defines) |d| @cDefine(d,
 * "1");`, so the declared surface here matches the compiled object's modules.
 */
#include "wuffs-v0.4.c"
