/* bindgen wrapper for gettext-sys (R3: self-contained, no ghostty dep).
 *
 * Declares the single symbol this crate's FFI exposes:
 * `_libintl_locale_name_canonicalize` — the macOS gnulib
 * `gl_locale_name_canonicalize`, renamed by config.h's
 *   #define gl_locale_name_canonicalize _libintl_locale_name_canonicalize
 * (see vendor/config.h). The function canonicalizes a locale name in place
 * (input/output buffer `name`); consumed by os/i18n.zig `canonicalizeLocale`.
 *
 * This is a thin declaration-only header for bindgen; the definition is
 * compiled from vendor/localename-unsafe.c by build.rs. */

void _libintl_locale_name_canonicalize(char *name);
