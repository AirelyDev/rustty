/* freetype-sys bindgen surface.
 *
 * Mirrors the reference's binding header (ghostty/pkg/freetype/freetype-zig.h)
 * exactly: the same public FreeType headers the reference's `c.zig` @cImports.
 * bindgen parses this against the vendored FreeType 2.13.2 `include/` tree.
 */
#include <ft2build.h>
#include FT_FREETYPE_H
#include FT_TRUETYPE_TABLES_H
#include <freetype/ftmm.h>
#include <freetype/ftoutln.h>
#include <freetype/ftsnames.h>
#include <freetype/ttnameid.h>
#include <freetype/ftbitmap.h>
#include <freetype/ftbbox.h>
