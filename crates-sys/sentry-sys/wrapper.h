/* bindgen entry point for sentry-sys.
 *
 * Mirrors the reference's pkg/sentry/c.zig @cImport, which does exactly:
 *
 *     @cInclude("sentry.h");
 *
 * `bindgen` (see build.rs) parses this against the vendored sentry-native
 * public header (vendor/include/sentry.h) to emit the `sentry_*` / `SENTRY_*`
 * FFI surface.
 */
#include <sentry.h>
