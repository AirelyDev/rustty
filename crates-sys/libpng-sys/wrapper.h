/* libpng-sys bindgen surface.
 *
 * The reference (pkg/libpng) exposes libpng's single public header, `png.h`
 * (which pulls in `pngconf.h` + the vendored `pnglibconf.h`). bindgen parses
 * this against the vendored libpng 1.6.43 source tree with the ghostty-provided
 * `pnglibconf.h` on the include path.
 */
#include <png.h>
