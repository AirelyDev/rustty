/* bindgen entry point for oniguruma-sys.
 *
 * This mirrors the reference's pkg/oniguruma/c.zig, which @cImports the single
 * public Oniguruma header:
 *
 *     @cInclude("oniguruma.h");
 *
 * bindgen generates the FFI (onig_new / onig_search / onig_search_with_param /
 * onig_new_match_param / OnigRegion / the ~83 ONIGERR_* codes / the encoding &
 * syntax externs) from this one header, resolved against the vendored
 * `vendor/src` include dir. */
#include <oniguruma.h>
