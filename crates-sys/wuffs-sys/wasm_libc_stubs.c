/* PORTS: ghostty pkg/wuffs/src/main.zig `callocStub`/`freeStub` (`619555d1c`
 * build-without-libc), compiled by build.rs only for a wasm32 target without
 * libc (the reference's `if (!builtin.link_libc)`).
 *
 * Wuffs' generated `wuffs_foo__bar__alloc()` convenience functions are the
 * only code that references libc's calloc/free. We never call them
 * and linker garbage collection strips them, but that isn't guaranteed.
 * When libc isn't linked there would be nothing to provide calloc/free if
 * they survive, so export stubs to satisfy the link. Weak so that any real
 * definition wins. Hidden keeps them out of the export table where the
 * format honors it (e.g. wasm).
 */
#include <stddef.h>

__attribute__((weak, visibility("hidden"))) void *calloc(size_t count, size_t size) {
    (void)count;
    (void)size;
    return NULL;
}

__attribute__((weak, visibility("hidden"))) void free(void *ptr) {
    (void)ptr;
}
