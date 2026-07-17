/* fontconfig-sys bindgen surface.
 *
 * Mirrors the reference's binding imports (ghostty/pkg/fontconfig/c.zig) exactly:
 * the ENTIRE fontconfig binding surface is the single public umbrella header
 * `<fontconfig/fontconfig.h>` (c.zig:1-3 @cIncludes only that). It transitively
 * pulls `<fontconfig/fcprivate.h>`, giving bindgen the whole `Fc*` API surface
 * (functions, `Fc*` types, the `FC_*` property-name macros, the
 * `FC_WEIGHT_*`/`FC_SLANT_*`/`FC_PROPORTIONAL`/`FC_MONO`/... constants).
 *
 * bindgen parses this against the vendored fontconfig 2.14.2 root (for
 * `<fontconfig/fontconfig.h>`); the public header needs no FreeType/libxml2
 * headers, but their include dirs are passed too (harmless) so the FFI stays in
 * lock-step with the C build's include path.
 */
#include <fontconfig/fontconfig.h>
