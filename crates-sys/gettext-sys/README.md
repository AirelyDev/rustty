# Vendored GNU gettext C sources (R3)

This directory vendors C source copied from the **same** upstream library the
reference (ghostty) links for macOS i18n: **GNU gettext 0.24**. The reference
builds libintl from gettext **only on macOS** — Linux provides libintl in libc
— see `/Users/gael/projects/ghostty/pkg/libintl/build.zig`.

Per `.port/PORT_RULES.md` R3 the port is **self-contained**: these sources are
copied in-tree and built standalone with the `cc` crate (see `../build.rs`). The
crate has **zero** build- or run-time dependency on the ghostty tree; the
reference is an external oracle only.

## Scope: the FULL macOS libintl (WS9)

As of WS9 this vendors the reference's **entire** `srcs` list — 30 translation
units — not a slice. That means both halves of libintl:

- the **message-catalog runtime**: `dgettext` / `dcgettext` / `ngettext` /
  `bindtextdomain` / `textdomain` / … (`vendor/intl/*.c`), consumed by
  `crates/rustty-os/src/i18n.rs` (the port of `os/i18n.zig`); and
- the **gnulib localename** family (`vendor/intl/gnulib-lib/*.c`), which defines
  `gl_locale_name_canonicalize`, renamed to
  `_libintl_locale_name_canonicalize` by `config.h` and consumed by
  `canonicalizeLocale`.

Because the emitted archive carries the catalog API, the macOS app no longer
links Homebrew's gettext: `-L/opt/homebrew/opt/gettext/lib -lintl` was removed
from **both** `crates/rustty-os/build.rs` and `macos/build-rustty.sh`, and
xtask folds this archive into `libRusttyKit.a`. The hermetic proof lives at
`.port/tools/gates/no_homebrew_gettext_gate.sh`.

`vendor/intl/plural.c` is the Bison-generated plural-form parser. It ships
**pre-generated** in the upstream tarball, so no bison/yacc runs at build time.

The tests in `../src/lib.rs` compile the vendored C, link it (with Apple
CoreFoundation, which the macOS canonicalizer consults via CFPreferences for
legacy locale names), and exercise both halves with known answers — proving the
end-to-end R3 toolchain (copied C -> `cc` compile -> `bindgen` FFI ->
`links = "intl"` -> Rust test linking and running the symbols).

## Provenance

- Build template — copied verbatim from `ghostty/pkg/libintl/` (the reference's
  macOS libintl build), kept at the top of `vendor/`:
  - `config.h`       — gettext's macOS `config.h` (generated on a Mac by the
                       reference; includes the reference's manual `xlocale.h`
                       addition at the end). Drives `-DHAVE_CONFIG_H`.
  - `libintl.h`      — the public libintl header.
  - `libgnuintl.h`   — `#include "libintl.h"`.
  - `wrapper.h`      — declaration-only header for `bindgen`.
- Upstream sources — copied verbatim from
  `gettext-0.24/gettext-runtime/intl/`, preserving the upstream directory shape:
  - `vendor/intl/*.c`, `vendor/intl/*.h`
  - `vendor/intl/gnulib-lib/*.c`, `*.h`, `glthread/*.h`

  Tarball: `https://deps.files.ghostty.org/gettext-0.24.tar.gz` — the exact URL
  and version `ghostty/pkg/libintl/build.zig.zon` pins. SHA-256
  `c918503d593d70daf4844d175a13d816afacb667c06fba1ec9dcd5002c1518b7`.

## Upgrading

Re-download the tarball, re-copy the files listed in `build.rs`'s `SOURCES`
plus their header closure, and re-check `config.h` against the reference's
(`ghostty/pkg/libintl/config.h`) — in particular the manual `xlocale.h` include
the reference appends at the end, which is easy to lose on a regenerate.
