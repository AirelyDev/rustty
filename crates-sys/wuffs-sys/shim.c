/* wuffs-sys static-inline shim — external-linkage definitions.
 *
 * Compiled by build.rs as a SEPARATE C translation unit WITHOUT
 * `WUFFS_IMPLEMENTATION` (so it emits NO copy of the codec implementation — only
 * the `rustty_wuffs_*` wrappers below get external linkage). Each wrapper forwards
 * to a Wuffs base-runtime `static inline` helper, which is fully defined inline in
 * the header interface region (before the `WUFFS_IMPLEMENTATION`/`__cplusplus`
 * guards), so this TU is self-contained and does not depend on the main
 * `wuffs-v0.4.c` object.
 *
 * See shim.h for the rationale (bindgen never emits linkable symbols for a
 * `static inline`).
 */
#include "shim.h"

uint32_t rustty_wuffs_pixel_config__width(const wuffs_base__pixel_config* c) {
  return wuffs_base__pixel_config__width(c);
}

uint32_t rustty_wuffs_pixel_config__height(const wuffs_base__pixel_config* c) {
  return wuffs_base__pixel_config__height(c);
}

void rustty_wuffs_pixel_config__set(wuffs_base__pixel_config* c,
                                    uint32_t pixfmt_repr,
                                    uint32_t pixsub_repr,
                                    uint32_t width,
                                    uint32_t height) {
  wuffs_base__pixel_config__set(c, pixfmt_repr, pixsub_repr, width, height);
}

wuffs_base__status rustty_wuffs_pixel_buffer__set_from_slice(
    wuffs_base__pixel_buffer* pb,
    const wuffs_base__pixel_config* pixcfg,
    wuffs_base__slice_u8 pixbuf_memory) {
  return wuffs_base__pixel_buffer__set_from_slice(pb, pixcfg, pixbuf_memory);
}

const char* rustty_wuffs_status__message(const wuffs_base__status* z) {
  return wuffs_base__status__message(z);
}

uint32_t rustty_wuffs_pixel_format__bits_per_pixel(const wuffs_base__pixel_format* f) {
  return wuffs_base__pixel_format__bits_per_pixel(f);
}

bool rustty_wuffs_pixel_format__is_direct(const wuffs_base__pixel_format* f) {
  return wuffs_base__pixel_format__is_direct(f);
}

bool rustty_wuffs_pixel_format__is_interleaved(const wuffs_base__pixel_format* f) {
  return wuffs_base__pixel_format__is_interleaved(f);
}
