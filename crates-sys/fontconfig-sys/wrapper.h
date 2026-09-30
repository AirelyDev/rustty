/* fontconfig-sys bindgen surface.
 *
 * Mirrors the reference's binding imports (ghostty/pkg/fontconfig/build.zig:89-97,
 * the translate-c `fontconfig_c` module) exactly: the ENTIRE fontconfig binding
 * surface is the single public umbrella header `<fontconfig/fontconfig.h>`. It transitively
 * pulls `<fontconfig/fcprivate.h>`, giving bindgen the whole `Fc*` API surface
 * (functions, `Fc*` types, the `FC_*` property-name macros, the
 * `FC_WEIGHT_*`/`FC_SLANT_*`/`FC_PROPORTIONAL`/`FC_MONO`/... constants).
 *
 * bindgen parses this against the vendored fontconfig 2.18.3 include path; in
 * 2.18.3 `<fontconfig/fontconfig.h>` resolves to the reference's generated
 * `override/fontconfig/fontconfig.h` (the tarball ships only `fontconfig.h.in`).
 * The public header needs no FreeType/libxml2
 * headers, but their include dirs are passed too (harmless) so the FFI stays in
 * lock-step with the C build's include path.
 */
#include <fontconfig/fontconfig.h>
