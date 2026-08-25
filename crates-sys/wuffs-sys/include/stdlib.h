/* PORTS: ghostty pkg/wuffs/include/stdlib.h (`619555d1c` build-without-libc).
 * INERT in rustty today: NOT on any include path — wuffs-sys compiles the
 * vendored TU with `cc` against the real system libc (see build.rs). Vendored
 * as the R1 mirror for a future wasm32/freestanding lane. */

// Minimal stdlib.h so that wuffs can be translated and compiled without
// requiring libc headers. See string.h in this directory for details.
#ifndef RUSTTY_WUFFS_STDLIB_H
#define RUSTTY_WUFFS_STDLIB_H

#include <stddef.h>

void *malloc(size_t size);
void *calloc(size_t count, size_t size);
void *realloc(void *ptr, size_t size);
void free(void *ptr);

#endif
