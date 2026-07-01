/* rustty-font-shaper: instantiate the vendored stb_image_resize single-header
 * implementation. R3: the SAME C library the reference links
 * (`src/stb/stb_image_resize.h`), copied into our own tree and built with `cc`.
 * The freetype face uses `stbir_resize_uint8` to scale bitmap-strike glyphs. */
#define STB_IMAGE_RESIZE_IMPLEMENTATION
#include "stb_image_resize.h"
