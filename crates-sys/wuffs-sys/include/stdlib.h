/* PORTS: ghostty pkg/wuffs/include/stdlib.h (`619555d1c` build-without-libc).
 * LIVE on wasm32 since P7-W5 T2 (DECISIONS B8): wuffs-sys's build.rs puts
 * this directory first on the include path there (the reference does so on
 * every non-Windows target); native targets compile against the real system
 * libc headers. */

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
