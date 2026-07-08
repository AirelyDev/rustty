/* wuffs-sys static-inline shim — external-linkage declarations.
 *
 * Wuffs' base runtime exposes a handful of helpers as `static inline` (e.g.
 * `wuffs_base__pixel_config__set`, `..__width`, `..__pixel_buffer__set_from_slice`,
 * `..__status__message`). bindgen NEVER emits a linkable FFI symbol for a
 * `static inline` function, so the decode path (rustty-wuffs) cannot call them
 * directly. This shim wraps each needed helper with an ordinary external-linkage
 * function (`rustty_wuffs_*`) whose definition lives in `shim.c`; bindgen parses
 * these prototypes (via `wrapper.h` -> `#include "shim.h"`) and emits real FFI
 * bindings, and the linker resolves them against the compiled `shim.o`.
 *
 * This is the SAME hand-around-a-`static inline` technique the wuffs-sys KAT
 * already uses (constructing `wuffs_base__slice_u8` by hand == `make_slice_u8`,
 * reading `status.repr` directly == `status__is_ok`); here we expose the ~5
 * helpers the PNG/JPEG decode path needs. Additive to the `-sys` leaf; no
 * reference-fidelity surface.
 */
#ifndef RUSTTY_WUFFS_SHIM_H
#define RUSTTY_WUFFS_SHIM_H

/* Pulls in the Wuffs base types the prototypes below name (guard-protected, so a
 * second include from wrapper.h / shim.c is a no-op). WITHOUT
 * `WUFFS_IMPLEMENTATION`: declarations + the `static inline` helpers only. */
#include "wuffs-v0.4.c"

/* wuffs_base__pixel_config__width — the decoded image width. */
uint32_t rustty_wuffs_pixel_config__width(const wuffs_base__pixel_config* c);

/* wuffs_base__pixel_config__height — the decoded image height. */
uint32_t rustty_wuffs_pixel_config__height(const wuffs_base__pixel_config* c);

/* wuffs_base__pixel_config__set — set the pixel format / subsampling / dims. */
void rustty_wuffs_pixel_config__set(wuffs_base__pixel_config* c,
                                    uint32_t pixfmt_repr,
                                    uint32_t pixsub_repr,
                                    uint32_t width,
                                    uint32_t height);

/* wuffs_base__pixel_buffer__set_from_slice — bind a pixel buffer to memory. */
wuffs_base__status rustty_wuffs_pixel_buffer__set_from_slice(
    wuffs_base__pixel_buffer* pb,
    const wuffs_base__pixel_config* pixcfg,
    wuffs_base__slice_u8 pixbuf_memory);

/* wuffs_base__status__message — the human-readable status/error string. */
const char* rustty_wuffs_status__message(const wuffs_base__status* z);

#endif /* RUSTTY_WUFFS_SHIM_H */
