//! Build script for fontconfig-sys (Fontconfig 2.14.2 font discovery/matching).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C sources for fontconfig 2.14.2 are vendored in-tree under
//!      `vendor/` (fetched from the exact URL in the reference's
//!      pkg/fontconfig/build.zig.zon:8 — the
//!      `https://deps.files.ghostty.org/fontconfig-2.14.2.tar.gz` mirror of the
//!      freedesktop `fontconfig` `2.14.2` release). This build NEVER references
//!      /home/gael/projects/ghostty, and never links the host's system
//!      fontconfig (2.17 on this box — the parity target is the reference's
//!      pinned 2.14.2).
//!   2. They are compiled with the `cc` crate, mirroring the reference's
//!      pkg/fontconfig/build.zig `buildLib` exactly: the same 28 `src/*.c`
//!      translation units (build.zig:269-298), the same verbatim `-D`/`-W`/`-f`
//!      flag list (build.zig:87-194) plus the libxml2 enablement flags
//!      (build.zig:216-221).
//!   3. `vendor/override/include/` holds the reference's five override headers
//!      verbatim (pkg/fontconfig/override/include): the four EMPTY alias stubs
//!      `fcalias.h`/`fcaliastail.h`/`fcftalias.h`/`fcftaliastail.h` (so the
//!      `#include "fcalias.h"` machinery resolves for a static, non-exported
//!      build where upstream's `makealias` is not run) + the 347-line
//!      pre-generated `fcobjshash.h` (the `gperf` object-name hash table,
//!      avoiding a build-time `gperf` dependency). Placed FIRST on the include
//!      path. The rest of what `./configure` would template is unneeded: the
//!      distribution tarball already ships `src/fcstdint.h`,
//!      `fc-lang/fclang.h`, `fc-case/fccase.h`; and every `#include <config.h>`
//!      is guarded by `HAVE_CONFIG_H`, which — exactly as the reference — is
//!      never defined (all config arrives as `-D` flags below).
//!   4. FreeType (`src/fcfreetype.c`) is provided by the `freetype-sys` crate:
//!      its vendored header dir arrives via `DEP_FREETYPE_INCLUDE`, and its
//!      static `freetype` archive is linked through the normal dependency (the
//!      reference does the equivalent `lib.linkLibrary(freetype)`). libxml2
//!      (`src/fcxml.c`, under `-DENABLE_LIBXML2`) likewise comes from
//!      `libxml2-sys` via `DEP_XML2_INCLUDE` + `DEP_XML2_OVERRIDE_INCLUDE`.
//!   5. `bindgen` auto-generates the FFI from `wrapper.h` — the same public
//!      header the reference's `c.zig` @cImports (`fontconfig/fontconfig.h`).
//!   6. `cargo:rustc-link-*` is emitted by `cc` (static archive `libfontconfig`);
//!      the crate declares `links = "fontconfig"`, so Cargo enforces a single
//!      linkage.

use std::env;
use std::path::PathBuf;

/// fontconfig translation units compiled into the static library.
///
/// The verbatim `srcs` list from the reference's
/// pkg/fontconfig/build.zig:269-298. (`src/fcarch.c` is present in the vendored
/// tree but, exactly as in the reference, is NOT compiled — its contents are
/// pulled in via header inclusion elsewhere.)
const SOURCES: &[&str] = &[
    "src/fcatomic.c",
    "src/fccache.c",
    "src/fccfg.c",
    "src/fccharset.c",
    "src/fccompat.c",
    "src/fcdbg.c",
    "src/fcdefault.c",
    "src/fcdir.c",
    "src/fcformat.c",
    "src/fcfreetype.c",
    "src/fcfs.c",
    "src/fcptrlist.c",
    "src/fchash.c",
    "src/fcinit.c",
    "src/fclang.c",
    "src/fclist.c",
    "src/fcmatch.c",
    "src/fcmatrix.c",
    "src/fcname.c",
    "src/fcobjs.c",
    "src/fcpat.c",
    "src/fcrange.c",
    "src/fcserialize.c",
    "src/fcstat.c",
    "src/fcstr.c",
    "src/fcweight.c",
    "src/fcxml.c",
    "src/ftglue.c",
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    let override_include = vendor.join("override/include");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let windows = target_os == "windows";
    let ptr_bits: u32 = env::var("CARGO_CFG_TARGET_POINTER_WIDTH")
        .ok()
        .and_then(|w| w.parse().ok())
        .unwrap_or(64);

    // FreeType header dir, exported by freetype-sys via `cargo:include`
    // (`DEP_FREETYPE_INCLUDE`). Required because `src/fcfreetype.c` does
    // `#include <ft2build.h>` / `FT_FREETYPE_H`.
    let ft_include =
        env::var("DEP_FREETYPE_INCLUDE").expect("DEP_FREETYPE_INCLUDE not set by freetype-sys");
    // libxml2 header dirs, exported by libxml2-sys (`DEP_XML2_INCLUDE` for the
    // upstream `include/`, `DEP_XML2_OVERRIDE_INCLUDE` for the generated
    // `libxml/xmlversion.h`). Required because `src/fcxml.c` (under
    // `-DENABLE_LIBXML2`) does `#include <libxml/parser.h>`.
    let xml_include =
        env::var("DEP_XML2_INCLUDE").expect("DEP_XML2_INCLUDE not set by libxml2-sys");
    let xml_override_include = env::var("DEP_XML2_OVERRIDE_INCLUDE")
        .expect("DEP_XML2_OVERRIDE_INCLUDE not set by libxml2-sys");

    let mut build = cc::Build::new();
    build
        // Include-path order mirrors the reference (build.zig:82 then
        // build.zig:243): `override/include` FIRST (so `fcobjshash.h` and the
        // empty alias stubs resolve), then the upstream root (for
        // `<fontconfig/fontconfig.h>` / `<fontconfig/fcprivate.h>`). The `src/`
        // dir need not be listed: quote-includes (`"fcint.h"`,
        // `"../fc-lang/fclang.h"`) resolve relative to each `.c` file's own
        // directory, which the compiler always searches first.
        .include(&override_include)
        .include(&vendor)
        // FreeType + libxml2 headers for fcfreetype.c / fcxml.c.
        .include(&ft_include)
        .include(&xml_include)
        .include(&xml_override_include)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w");

    // ---- Verbatim common flag list (build.zig:87-129) -----------------------
    for def in [
        "HAVE_DIRENT_H",
        "HAVE_FCNTL_H",
        "HAVE_STDLIB_H",
        "HAVE_STRING_H",
        "HAVE_UNISTD_H",
        "HAVE_SYS_PARAM_H",
        "HAVE_MKSTEMP",
        "HAVE_RAND",
        "HAVE_VPRINTF",
        "HAVE_FT_GET_BDF_PROPERTY",
        "HAVE_FT_GET_PS_FONT_INFO",
        "HAVE_FT_HAS_PS_GLYPH_NAMES",
        "HAVE_FT_GET_X11_FONT_FORMAT",
        "HAVE_FT_DONE_MM_VAR",
        "HAVE_POSIX_FADVISE",
        "FLEXIBLE_ARRAY_MEMBER",
        "HAVE_STDATOMIC_PRIMITIVES",
    ] {
        build.define(def, None);
    }
    build.define("FC_GPERF_SIZE_T", Some("size_t"));
    // "Default errors that fontconfig can't handle" (build.zig:122-124).
    build
        .flag_if_supported("-Wno-implicit-function-declaration")
        .flag_if_supported("-Wno-int-conversion")
        // fontconfig MR#231: fontconfig's own casts trip ubsan (build.zig:126-128).
        .flag_if_supported("-fno-sanitize=undefined")
        .flag_if_supported("-fno-sanitize-trap=undefined");

    // ---- Pointer-width defines (build.zig:131-143) --------------------------
    let p = if ptr_bits == 32 { "4" } else { "8" };
    build
        .define("SIZEOF_VOID_P", Some(p))
        .define("ALIGNOF_VOID_P", Some(p));

    if windows {
        // (build.zig:144-150) — kept for completeness; this crate's parity
        // target is Linux, but the Windows arm mirrors the reference verbatim.
        build
            .define("FC_CACHEDIR", Some("\"LOCAL_APPDATA_FONTCONFIG_CACHE\""))
            .define("FC_TEMPLATEDIR", Some("\"c:/share/fontconfig/conf.avail\""))
            .define("CONFIGDIR", Some("\"c:/etc/fonts/conf.d\""))
            .define(
                "FC_DEFAULT_FONTS",
                Some("\"\\t<dir>WINDOWSFONTDIR</dir>\\n\\t<dir>WINDOWSUSERFONTDIR</dir>\\n\""),
            );
    } else {
        // ---- Off-Windows HAVE_* + cache/font dirs (build.zig:151-173) -------
        for def in [
            "HAVE_FSTATFS",
            "HAVE_FSTATVFS",
            "HAVE_GETOPT",
            "HAVE_GETOPT_LONG",
            "HAVE_LINK",
            "HAVE_LRAND48",
            "HAVE_LSTAT",
            "HAVE_MKDTEMP",
            "HAVE_MKOSTEMP",
            "HAVE__MKTEMP_S",
            "HAVE_MMAP",
            "HAVE_PTHREAD",
            "HAVE_RANDOM",
            "HAVE_RAND_R",
            "HAVE_READLINK",
            "HAVE_SYS_MOUNT_H",
            "HAVE_SYS_STATVFS_H",
        ] {
            build.define(def, None);
        }
        // The compiled-in cache dir is ported verbatim from the reference
        // (build.zig:171). It may be unwritable in this sandbox — that is an
        // accepted out-of-scope RUNTIME concern; the KAT does not depend on a
        // writable cache or a system font scan.
        build
            .define("FC_CACHEDIR", Some("\"/var/cache/fontconfig\""))
            .define(
                "FC_DEFAULT_FONTS",
                Some("\"<dir>/usr/share/fonts</dir><dir>/usr/local/share/fonts</dir>\""),
            );

        // Non-FreeBSD (Linux) arm (build.zig:181-186).
        build
            .define(
                "FC_TEMPLATEDIR",
                Some("\"/usr/share/fontconfig/conf.avail\""),
            )
            .define("FONTCONFIG_PATH", Some("\"/etc/fonts\""))
            .define("CONFIGDIR", Some("\"/usr/local/fontconfig/conf.d\""));

        // Linux-specific headers (build.zig:189-193).
        if target_os == "linux" {
            build
                .define("HAVE_SYS_STATFS_H", None)
                .define("HAVE_SYS_VFS_H", None);
        }
    }

    // ---- libxml2 enablement (build.zig:216-221) -----------------------------
    // The reference always builds fontconfig's XML config parser against
    // libxml2 (`enable-libxml2` defaults to true), matching our `libxml2-sys`
    // dependency.
    build
        .define("ENABLE_LIBXML2", None)
        .define("LIBXML_STATIC", None)
        .define("LIBXML_PUSH_ENABLED", None);

    // Pin the C standard to gnu17 (pre-C23). GCC 16 defaults to gnu23, where an
    // empty parameter list `()` means `(void)` rather than the historical
    // "unspecified arguments" — fontconfig 2.14.2 is older C with such
    // prototypes. The reference's clang toolchain (Zig 0.15.2) defaults to
    // gnu17, so this reproduces exactly how the vendored source was compiled
    // upstream; it is a front-end language-mode choice only, no codegen
    // difference. Same precedent as crates-sys/{oniguruma,libxml2}-sys/build.rs.
    build.flag_if_supported("-std=gnu17");

    for src in SOURCES {
        build.file(vendor.join(src));
        println!("cargo:rerun-if-changed=vendor/{src}");
    }

    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=fontconfig` + search-path directives. Because
    // the crate declares `links = "fontconfig"`, Cargo enforces a single
    // linkage. The archive name matches the reference's
    // `b.addLibrary(.{ .name = "fontconfig" })` (build.zig:70).
    build.compile("fontconfig");

    // The reference links pthread off-Windows (build.zig:78-80). On modern
    // glibc (>= 2.34) pthread is folded into libc so this is a no-op stub, but
    // it is emitted for parity and for older/musl toolchains.
    if !windows {
        println!("cargo:rustc-link-lib=pthread");
    }

    // Auto-generate the FFI from `wrapper.h` (`<fontconfig/fontconfig.h>`)
    // against the same include path the C build uses.
    let bindings = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", override_include.display()))
        .clang_arg(format!("-I{}", vendor.display()))
        .clang_arg(format!("-I{ft_include}"))
        .clang_arg(format!("-I{xml_include}"))
        .clang_arg(format!("-I{xml_override_include}"))
        // Emit the fontconfig public surface (Fc* functions/types + FC_* macros).
        .allowlist_function("Fc.*")
        .allowlist_type("Fc.*")
        .allowlist_type("_Fc.*")
        .allowlist_var("FC_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate fontconfig-sys bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("unable to write fontconfig-sys bindings");

    // Re-export the vendored fontconfig include roots for downstream crates that
    // build against the fontconfig headers (Task 4's `fc.rs` links the crate
    // directly rather than re-including headers, but this keeps the -sys
    // template's `cargo:include` contract complete). Cargo sets
    // `DEP_FONTCONFIG_INCLUDE` / `DEP_FONTCONFIG_OVERRIDE_INCLUDE`.
    println!("cargo:include={}", vendor.display());
    println!("cargo:override_include={}", override_include.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
