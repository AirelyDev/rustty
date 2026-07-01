/* harfbuzz-sys bindgen surface.
 *
 * Mirrors the reference's binding imports (ghostty/pkg/harfbuzz/c.zig) exactly:
 * the same public HarfBuzz headers the reference's `c.zig` @cImports. Because
 * the vendored build enables FreeType (`build_options.freetype == true`) and
 * disables CoreText (`build_options.coretext == false`), this includes `hb.h`
 * and `hb-ft.h` but NOT `hb-coretext.h`. bindgen parses this against the
 * vendored HarfBuzz 11.0.0 `src/` tree (plus the FreeType include dir that
 * `hb-ft.h` pulls via `<ft2build.h>`).
 */
#include <hb.h>
#include <hb-ft.h>

/* On macOS the CoreText integration is enabled (`HAVE_CORETEXT`), exposing
 * `hb_coretext_font_create` used by the `coretext_harfbuzz`/`coretext_freetype`
 * backends to build an `hb_font_t` from a `CTFontRef`. Mirrors the reference's
 * `c.zig` including `hb-coretext.h` when `build_options.coretext == true`. */
#ifdef __APPLE__
#include <hb-coretext.h>
#endif
