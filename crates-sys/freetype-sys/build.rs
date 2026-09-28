//! Build script for freetype-sys (FreeType 2.13.2 font rasterizer).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C sources for FreeType 2.13.2 are vendored in-tree under
//!      `vendor/` (fetched from the exact URL in the reference's
//!      pkg/freetype/build.zig.zon — the deps.files.ghostty.org mirror of the
//!      `freetype/freetype` `VER-2-13-2` tree). This build NEVER references
//!      /Users/gael/projects/ghostty.
//!   2. They are compiled with the `cc` crate, mirroring the reference's
//!      pkg/freetype/build.zig `buildLib` exactly: the same `SOURCES` module
//!      list, the same platform-specific `ftsystem.c`/`ftdebug.c` selection,
//!      and the same flags (`-DFT2_BUILD_LIBRARY`,
//!      `-DFT_CONFIG_OPTION_SYSTEM_ZLIB=1`, `-fno-sanitize=undefined`; plus
//!      `-DHAVE_UNISTD_H -DHAVE_FCNTL_H` off-Windows and `-fPIC` on
//!      FreeBSD/musl). Vendored upstream warnings are silenced.
//!   3. System zlib (`FT_CONFIG_OPTION_SYSTEM_ZLIB=1`) is provided by the
//!      `zlib-sys` crate: its `zlib.h` dir arrives via `DEP_Z_INCLUDE` and its
//!      static `z` archive is linked through the normal dependency (the
//!      reference does the equivalent `lib.linkLibrary(zlib_dep)`).
//!   4. `bindgen` auto-generates the FFI from `wrapper.h` — the same public
//!      FreeType headers the reference's `c.zig` @cImports.
//!   5. `cargo:rustc-link-*` is emitted by `cc` (static archive `libfreetype`).
//!      The vendored `include/` dir is re-exported via `DEP_FREETYPE_INCLUDE`
//!      for downstream crates (harfbuzz-sys, rustty-font).

use std::env;
use std::path::PathBuf;

/// FreeType translation units compiled into the static library.
///
/// This is the verbatim `srcs` list from the reference's
/// pkg/freetype/build.zig — the amalgamated per-module compilation units. Each
/// module `.c` (`autofit.c`, `cff.c`, `sfnt.c`, …) `#include`s its sibling
/// sources, so the whole `vendor/src/` tree must be present even though only
/// these entry points are handed to the compiler.
const SOURCES: &[&str] = &[
    "src/autofit/autofit.c",
    "src/base/ftbase.c",
    "src/base/ftbbox.c",
    "src/base/ftbdf.c",
    "src/base/ftbitmap.c",
    "src/base/ftcid.c",
    "src/base/ftfstype.c",
    "src/base/ftgasp.c",
    "src/base/ftglyph.c",
    "src/base/ftgxval.c",
    "src/base/ftinit.c",
    "src/base/ftmm.c",
    "src/base/ftotval.c",
    "src/base/ftpatent.c",
    "src/base/ftpfr.c",
    "src/base/ftstroke.c",
    "src/base/ftsynth.c",
    "src/base/fttype1.c",
    "src/base/ftwinfnt.c",
    "src/bdf/bdf.c",
    "src/bzip2/ftbzip2.c",
    "src/cache/ftcache.c",
    "src/cff/cff.c",
    "src/cid/type1cid.c",
    "src/gzip/ftgzip.c",
    "src/lzw/ftlzw.c",
    "src/pcf/pcf.c",
    "src/pfr/pfr.c",
    "src/psaux/psaux.c",
    "src/pshinter/pshinter.c",
    "src/psnames/psnames.c",
    "src/raster/raster.c",
    "src/sdf/sdf.c",
    "src/sfnt/sfnt.c",
    "src/smooth/smooth.c",
    "src/svg/svg.c",
    "src/truetype/truetype.c",
    "src/type1/type1.c",
    "src/type42/type42.c",
    "src/winfonts/winfnt.c",
];

/// libpng 1.6.43 compilation units — the verbatim `srcs` list from the
/// reference's pkg/libpng/build.zig, vendored under `vendor/libpng/` so FreeType
/// can be built with `FT_CONFIG_OPTION_USE_PNG` (color-bitmap glyph decoding).
const LIBPNG_SOURCES: &[&str] = &[
    "png.c",
    "pngerror.c",
    "pngget.c",
    "pngmem.c",
    "pngpread.c",
    "pngread.c",
    "pngrio.c",
    "pngrtran.c",
    "pngrutil.c",
    "pngset.c",
    "pngtrans.c",
    "pngwio.c",
    "pngwrite.c",
    "pngwtran.c",
    "pngwutil.c",
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    let ft_include = vendor.join("include");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    let mut build = cc::Build::new();
    build
        // Windows (MSVC ABI), task F-1 (N353): the reference compiles this with
        // Zig's clang even when `abi == .msvc`, so on an MSVC target cc drives
        // clang-cl (the MSVC-ABI clang driver; a CC/CXX override still wins),
        // not cl.exe. A no-op on every other target.
        .prefer_clang_cl_over_msvc(true)
        .include(&ft_include)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w")
        // Reference flags (pkg/freetype/build.zig `buildLib`).
        .define("FT2_BUILD_LIBRARY", None)
        .define("FT_CONFIG_OPTION_SYSTEM_ZLIB", Some("1"))
        // The reference builds FreeType with libpng enabled
        // (`SharedDeps.zig`: `.@"enable-libpng" = true`) so color-bitmap fonts
        // that store glyphs as PNG (e.g. NotoColorEmoji's CBDT strikes) can be
        // decoded. Without it `FT_Load_Glyph` returns `Unimplemented_Feature` on
        // such glyphs. Enabled here and satisfied by the vendored libpng below.
        .define("FT_CONFIG_OPTION_USE_PNG", Some("1"))
        .flag_if_supported("-fno-sanitize=undefined");

    // Off-Windows the reference defines HAVE_UNISTD_H / HAVE_FCNTL_H (used by
    // the base `ftsystem.c` memory-mapping implementation).
    if target_os != "windows" {
        build
            .define("HAVE_UNISTD_H", None)
            .define("HAVE_FCNTL_H", None);
    }

    // The reference adds -fPIC on FreeBSD and musl.
    if target_os == "freebsd" || target_env == "musl" {
        build.flag_if_supported("-fPIC");
    }

    // System zlib header dir, exported by zlib-sys via `cargo:include`
    // (`DEP_Z_INCLUDE`). Required because `FT_CONFIG_OPTION_SYSTEM_ZLIB=1` makes
    // `src/gzip/ftgzip.c` `#include <zlib.h>`.
    let zlib_include = env::var("DEP_Z_INCLUDE").expect("DEP_Z_INCLUDE not set by zlib-sys");
    build.include(&zlib_include);

    // Vendored libpng 1.6.43 (the reference's `libpng` dependency), compiled into
    // the same archive so FreeType's `FT_CONFIG_OPTION_USE_PNG` sfnt png-shim can
    // decode PNG color-bitmap glyphs. `FreeType`'s `pngshim.c` `#include <png.h>`,
    // so the libpng dir goes on the include path; `png.c` et al. use the vendored
    // `pnglibconf.h` (the reference's own build config) and the system `zlib.h`
    // already added above. SIMD paths are disabled to keep the build portable
    // (matches the reference's `-DPNG_*_OPT=0` flags).
    let libpng = vendor.join("libpng");
    build.include(&libpng);
    build
        .define("PNG_ARM_NEON_OPT", Some("0"))
        .define("PNG_POWERPC_VSX_OPT", Some("0"))
        .define("PNG_INTEL_SSE_OPT", Some("0"))
        .define("PNG_MIPS_MSA_OPT", Some("0"));
    for src in LIBPNG_SOURCES {
        build.file(libpng.join(src));
        println!("cargo:rerun-if-changed=vendor/libpng/{src}");
    }

    // The module entry points.
    for src in SOURCES {
        build.file(vendor.join(src));
        println!("cargo:rerun-if-changed=vendor/{src}");
    }

    // Platform system layer: the reference selects the OS-specific `ftsystem.c`
    // (memory mapping) and `ftdebug.c`.
    let ftsystem = match target_os.as_str() {
        "linux" => "builds/unix/ftsystem.c",
        "windows" => "builds/windows/ftsystem.c",
        _ => "src/base/ftsystem.c",
    };
    build.file(vendor.join(ftsystem));
    println!("cargo:rerun-if-changed=vendor/{ftsystem}");

    // Windows uses `builds/windows/ftdebug.c` (+ a resource file the reference
    // adds, irrelevant to a static archive); every other target uses the base
    // `ftdebug.c`.
    let ftdebug = if target_os == "windows" {
        "builds/windows/ftdebug.c"
    } else {
        "src/base/ftdebug.c"
    };
    build.file(vendor.join(ftdebug));
    println!("cargo:rerun-if-changed=vendor/{ftdebug}");

    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=freetype` + search-path directives. Because
    // the crate declares `links = "freetype"`, Cargo enforces a single linkage.
    build.compile("freetype");

    // Auto-generate the FFI from the same public headers the reference's
    // `c.zig` @cImports (see wrapper.h). Uses the vendored FreeType include dir
    // plus the system-zlib include dir (ftgzip pulls <zlib.h>).
    let bindings = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", ft_include.display()))
        .clang_arg(format!("-I{zlib_include}"))
        // Emit the FreeType public surface (FT_*) plus the TrueType tag enums.
        .allowlist_function("FT_.*")
        .allowlist_type("FT_.*")
        .allowlist_var("FT_.*")
        .allowlist_type("TT_.*")
        .allowlist_var("TT_.*")
        .allowlist_var("FREETYPE_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate freetype-sys bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("unable to write freetype-sys bindings");

    // Re-export the vendored FreeType include dir for downstream crates that
    // build against the FreeType headers (harfbuzz-sys, rustty-font). Cargo
    // sets `DEP_FREETYPE_INCLUDE`.
    println!("cargo:include={}", ft_include.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
