# Vendored GNU gettext C sources (R3)

This directory vendors C source copied from the **same** upstream library the
reference (ghostty) links for macOS i18n: **GNU gettext** (its gnulib
`localename` family). The reference builds libintl from gettext **only on
macOS** — Linux provides libintl in libc — see
`/Users/gael/projects/ghostty/pkg/libintl/build.zig`.

Per `.port/PORT_RULES.md` R3 the port is **self-contained**: these sources are
copied in-tree and built standalone with the `cc` crate (see `../build.rs`). The
crate has **zero** build- or run-time dependency on the ghostty tree; the
reference is an external oracle only.

## Scope of this vendor slice

This vendors the single symbol `os/i18n.zig` consumes via
`canonicalizeLocale`: the macOS `gl_locale_name_canonicalize`, renamed to
`_libintl_locale_name_canonicalize` by `config.h`'s
`#define gl_locale_name_canonicalize _libintl_locale_name_canonicalize`. It
canonicalizes a macOS / BCP-47 locale name into a Unix (XPG) locale name in
place.

The link test (`cargo test -p gettext-sys`, macOS) compiles the vendored C,
links it (with Apple CoreFoundation, which the macOS canonicalizer consults via
CFPreferences for legacy locale names), and exercises the symbol with known
answers, proving the end-to-end R3 toolchain (copied C -> `cc` compile ->
`bindgen` FFI -> `links = "intl"` -> Rust test linking and running the symbol).

## Provenance

- Build template — copied verbatim from `ghostty/pkg/libintl/` (the reference's
  macOS libintl build):
  - `config.h`       — gettext's macOS `config.h` (generated on a Mac by the
                       reference; includes the reference's manual `xlocale.h`
                       addition at the end). Drives `-DHAVE_CONFIG_H`.
  - `libintl.h`      — the public libintl header.
  - `libgnuintl.h`   — `#include "libintl.h"`.
- gnulib `localename` sources — copied from the gettext upstream
  (`gettext-runtime/intl/gnulib-lib/`); this is the **macOS subset** of
  `build.zig`'s `srcs` needed to define and link the canonicalize symbol:
  - `localename-unsafe.c`        — defines `gl_locale_name_canonicalize` (and
                                   the `*_unsafe` locale-name helpers).
  - `getlocalename_l-unsafe.c`   — `getlocalename_l` helper.
  - `localename-environ.c`       — environment-based locale-name lookup.
  - `setlocale_null.c`,
    `setlocale_null-unlocked.c`,
    `setlocale-lock.c`           — `setlocale (..., NULL)` query helpers.
  - headers: `localename.h`, `getlocalename_l-unsafe.h`, `setlocale_null.h`,
    `arg-nonnull.h`, `localename-table.h`, `flexmember.h`, `verify.h`.
- `wrapper.h` — declaration-only header for `bindgen` (declares
  `_libintl_locale_name_canonicalize`).

The threaded `localename.c` is intentionally **not** vendored: it pulls in
gnulib's `glthread/` locking layer and is not required for the canonicalize
symbol (which lives in the `-unsafe` unit). If a future need requires it, vendor
it and its `glthread/` deps with no change to the `cc`/`bindgen` pattern.
