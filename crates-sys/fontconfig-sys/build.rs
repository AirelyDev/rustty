//! Build script for fontconfig-sys (Fontconfig 2.18.3 font discovery/matching).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C sources for fontconfig 2.18.3 are vendored in-tree under
//!      `vendor/` (fetched from the exact URL in the reference's
//!      pkg/fontconfig/build.zig.zon:8 — the freedesktop generic-package
//!      `fontconfig-2.18.3.tar.xz` release). This build NEVER references
//!      /home/gael/projects/ghostty, and never links the host's system
//!      fontconfig (the parity target is the reference's pinned 2.18.3).
//!   2. They are compiled with the `cc` crate, mirroring the reference's
//!      pkg/fontconfig/build.zig `buildLib` exactly: the same 29 `src/*.c`
//!      translation units (build.zig:343-373), the same verbatim `-D`/`-W`/`-f`
//!      flag list (build.zig:148-265) plus the FreeType and libxml2 enablement
//!      flags (build.zig:267-297).
//!   3. `vendor/override/` holds the reference's twelve override headers
//!      verbatim, in the reference's directory layout
//!      (pkg/fontconfig/override/{fc-case,fc-const,fc-genericfamily,fc-lang,
//!      fontconfig,src}). The 2.18.3 tarball no longer ships the headers its
//!      autotools/meson build generates (`fc-case/fccase.h`,
//!      `fc-const/fcconst.h`, `fc-genericfamily/fcgenericfamily.h`,
//!      `fc-lang/fclang.h`, `fontconfig/fontconfig.h`, `src/fcstdint.h`, the
//!      `makealias` outputs `src/fc{,ft}alias{,tail}.h`, and the `gperf` table
//!      `src/fcobjshash.h`); upstream extracted them from an autogen run
//!      (build.zig:6-46 documents how to regenerate them). `override/src/fcint.h`
//!      is byte-identical to the tarball's `src/fcint.h`. Every
//!      `#include <config.h>` is guarded by `HAVE_CONFIG_H`, which — exactly as
//!      the reference — is never defined (all config arrives as `-D` flags
//!      below).
//!   4. FreeType (`src/fcfreetype.c`) is provided by the `freetype-sys` crate:
//!      its vendored header dir arrives via `DEP_FREETYPE_INCLUDE`, and its
//!      static `freetype` archive is linked through the normal dependency (the
//!      reference does the equivalent `lib.linkLibrary(freetype)`). libxml2
//!      (`src/fcxml.c`, under `-DENABLE_LIBXML2`) likewise comes from
//!      `libxml2-sys` via `DEP_XML2_INCLUDE` + `DEP_XML2_OVERRIDE_INCLUDE`.
//!   5. `bindgen` auto-generates the FFI from `wrapper.h` — the same public
//!      header the reference's `c.zig` translates (`fontconfig/fontconfig.h`),
//!      which in 2.18.3 exists only as `override/fontconfig/fontconfig.h` (the
//!      tarball ships `fontconfig.h.in`; the reference installs the override
//!      copy, build.zig:326-336).
//!   6. `cargo:rustc-link-*` is emitted by `cc` (static archive `libfontconfig`);
//!      the crate declares `links = "fontconfig"`, so Cargo enforces a single
//!      linkage.
//!
//! The real `fcalias.h`/`fcaliastail.h` switch on fontconfig's hidden-alias
//! scheme (`fcint.h:131-134`) on ELF targets (`__ELF__` and GCC >= 3.3): the
//! Linux and FreeBSD archives export `Fc*` as aliases of hidden `IA__Fc*`
//! symbols. Darwin and Windows are not ELF and keep the plain shape. Faithful
//! (`.port/phase-7/DECISIONS.md` B15).

use std::env;
use std::path::PathBuf;

#[path = "src/dir_defines.rs"]
mod dir_defines;

/// fontconfig translation units compiled into the static library.
///
/// The verbatim `srcs` list from the reference's
/// pkg/fontconfig/build.zig:343-373. (`src/fcarch.c` is present in the vendored
/// tree but, exactly as in the reference, is NOT compiled — its contents are
/// pulled in via header inclusion elsewhere. `src/fcfontations.c` needs the
/// Rust `fontations` backend, which the reference does not enable.
/// `src/fcconffile.c` is omitted by the reference although upstream's meson
/// build compiles it (`src/meson.build:25`): it defines only the public
/// `FcConfigFileGenerate`/`FcConfigFileGenerateGenericAlias`, which no other TU
/// calls, so the archive leaves that API undefined. Faithful,
/// `.port/phase-7/DECISIONS.md` B14; latent note N438.)
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
    "src/fcgenericalias.c",
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
    println!("cargo:rerun-if-changed=src/dir_defines.rs");
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    let override_root = vendor.join("override");
    // The reference's override include dirs, in its order (build.zig:130-143).
    //
    // NOTE (build.zig:130-134): `override/fc-case` is present but deliberately
    // not listed. `src/fcstr.c:186` does `#include "../fc-case/fccase.h"`; the
    // file-relative lookup (`vendor/src/../fc-case/`) misses, because the
    // tarball's `fc-case/` holds only `fccase.tmpl.h` and is not vendored, so
    // the compiler walks the `-I` list and resolves the `..` against an
    // `override/<subdir>` entry: `override/fc-const/../fc-case/fccase.h`. Keep
    // at least one `override/<subdir>` on the path for that lookup to work.
    let override_dirs: Vec<PathBuf> = ["", "fc-const", "fc-genericfamily", "fc-lang", "src"]
        .iter()
        .map(|d| override_root.join(d))
        .collect();

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
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
    // Include-path order mirrors the reference (build.zig:135-143 then
    // build.zig:313): the five override dirs FIRST (so the generated headers
    // resolve, and `<fontconfig/fontconfig.h>` finds
    // `override/fontconfig/fontconfig.h`), then the upstream root (for
    // `<fontconfig/fcprivate.h>` / `<fontconfig/fcfreetype.h>`). The upstream
    // `src/` dir need not be listed: quote-includes (`"fcint.h"`) resolve
    // relative to each `.c` file's own directory, which the compiler always
    // searches first.
    for dir in &override_dirs {
        build.include(dir);
    }
    build
        .include(&vendor)
        // FreeType + libxml2 headers for fcfreetype.c / fcxml.c.
        .include(&ft_include)
        .include(&xml_include)
        .include(&xml_override_include)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w");

    // ---- FC_VERSION_* (build.zig:148-153) -----------------------------------
    // PORT NOTE: reference defect (R13), UD360. The reference derives these
    // from its `build.zig.zon` `.version`, which still reads "2.14.2" at 2.18.3
    // (stale), so it passes 2/14/2. `FcGetVersion()` does not read them (it
    // returns `FC_VERSION` from `fontconfig.h`, i.e. 21803), but the cache
    // does: `fccache.c:1340-1342` stamps every cache file it writes with them,
    // and `FcCacheIsNewVersion` (`fccache.c:793-817`) treats a cache stamped
    // with a higher version as newer and re-uses it after its directory
    // changed (`fccache.c:1015-1021`). With 2/14/2, a cache written by a real
    // 2.18.x fontconfig is re-used stale and fonts added since are not seen.
    // The port passes the vendored release's real version, 2/18/3, which is
    // what fontconfig's own autotools and meson builds pass
    // (`configure.ac:75-77`, `meson.build:45-47`). Tested by
    // `ud360_cache_stamped_by_fontconfig_2_18_3_is_rescanned_after_its_dir_changes`.
    build
        .define("FC_VERSION_MAJOR", Some("2"))
        .define("FC_VERSION_MINOR", Some("18"))
        .define("FC_VERSION_MICRO", Some("3"));

    // ---- Verbatim common flag list (build.zig:155-198) ----------------------
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
        // Selects the `FcStrDupVapFormat` arm at `fcint.h:1044` (UD358: not on
        // Windows GNU, where `HAVE__VSNPRINTF_L` below takes precedence).
        "HAVE_VSNPRINTF",
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
    // "Default errors that fontconfig can't handle" (build.zig:191-193).
    build
        .flag_if_supported("-Wno-implicit-function-declaration")
        .flag_if_supported("-Wno-int-conversion")
        // fontconfig MR#231: fontconfig's own casts trip ubsan (build.zig:195-197).
        .flag_if_supported("-fno-sanitize=undefined")
        .flag_if_supported("-fno-sanitize-trap=undefined");

    // ---- Pointer-width defines (build.zig:200-212) --------------------------
    let p = if ptr_bits == 32 { "4" } else { "8" };
    build
        .define("SIZEOF_VOID_P", Some(p))
        .define("ALIGNOF_VOID_P", Some(p));

    if windows {
        // (build.zig:213-219), verbatim. Reached by `-Dfont-backend=
        // fontconfig_freetype` on a GNU-ABI Windows target (N286).
        build
            .define("FC_CACHEDIR", Some("\"LOCAL_APPDATA_FONTCONFIG_CACHE\""))
            .define("FC_TEMPLATEDIR", Some("\"c:/share/fontconfig/conf.avail\""))
            .define("CONFIGDIR", Some("\"c:/etc/fonts/conf.d\""))
            .define(
                "FC_DEFAULT_FONTS",
                Some("\"\\t<dir>WINDOWSFONTDIR</dir>\\n\\t<dir>WINDOWSUSERFONTDIR</dir>\\n\""),
            );
        // PORT NOTE: reference defect (R13), UD358. The reference defines
        // `HAVE_VSNPRINTF` on every target (build.zig:169), which selects the
        // `FcStrDupVapFormat` arm at `fcint.h:1044-1077`. That arm calls
        // `FcLocaleSetCurrent`, which fontconfig declares and defines off
        // Windows only (`fcint.h:973-976`, `fccompat.c:374-387`), so on a
        // GNU-ABI Windows target the reference's own
        // `-Werror=implicit-function-declaration` (build.zig:292-297) fails
        // `fcstr.c` and the backend it offers there does not build. Defining
        // `HAVE__VSNPRINTF_L` selects the arm fontconfig provides for Windows
        // (`fcint.h:991-1010`: `_vsnprintf_l` with a `_create_locale` "C"
        // locale), which never calls `FcLocaleSetCurrent`. MSVC is
        // refused before this crate is built (`rustty-build-config`, N286: no
        // `unistd.h`), so only the GNU ABI reaches it.
        if target_env == "gnu" {
            build.define("HAVE__VSNPRINTF_L", None);
        }
    } else {
        // ---- Off-Windows HAVE_* + cache/font dirs (build.zig:220-243) -------
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
            // The only arm of `FcLocaleSetCurrent` (`fccompat.c:375-385`) that
            // does not `#error` off Windows.
            "HAVE_USELOCALE",
            "HAVE_SYS_MOUNT_H",
            "HAVE_SYS_STATVFS_H",
        ] {
            build.define(def, None);
        }
        // The compiled-in cache dir is ported verbatim from the reference
        // (build.zig:241). It may be unwritable in this sandbox — that is an
        // accepted out-of-scope RUNTIME concern; the KAT does not depend on a
        // writable cache or a system font scan.
        build
            .define("FC_CACHEDIR", Some("\"/var/cache/fontconfig\""))
            .define(
                "FC_DEFAULT_FONTS",
                Some("\"<dir>/usr/share/fonts</dir><dir>/usr/local/share/fonts</dir>\""),
            );

        // The FreeBSD / other-OS directory arms (build.zig:245-257, S31-F1),
        // chosen by `src/dir_defines.rs` where they are tested.
        for (name, value) in dir_defines::fc_dir_defines(&target_os) {
            build.define(name, Some(value));
        }

        // Linux-specific headers (build.zig:259-264).
        if target_os == "linux" {
            build
                .define("HAVE_SYS_STATFS_H", None)
                .define("HAVE_SYS_VFS_H", None);
        }
    }

    // ---- FreeType enablement (build.zig:267-282) ----------------------------
    // "Value-tested (#if/#elif) by upstream so it must be =1." The reference's
    // `enable-freetype` defaults to true, and this crate always links
    // `freetype-sys`. It gates the FreeType paths of `fcdir.c:35,91`,
    // `fclang.c:27,1062` and `fcpat.c:25,311,880,1102,1595`. Without it (or with
    // `=0`), `fcdir.c:89-94` selects no font-query function for
    // `FcFileScanConfig`, and the archive fails to compile at `fcdir.c:100`
    // (the negative control recorded in the P7-W10 T1 report).
    build.define("ENABLE_FREETYPE", Some("1"));

    // ---- libxml2 enablement (build.zig:284-297) -----------------------------
    // The reference always builds fontconfig's XML config parser against
    // libxml2 (`enable-libxml2` defaults to true), matching our `libxml2-sys`
    // dependency.
    build
        .define("ENABLE_LIBXML2", None)
        .define("LIBXML_STATIC", None)
        .define("LIBXML_PUSH_ENABLED", None);
    if windows {
        // "NOTE: this should be defined on all targets" — the reference adds it
        // on Windows only (build.zig:292-297), after the `-Wno-` form above, so
        // an implicit declaration is an error there. N286.
        build.flag_if_supported("-Werror=implicit-function-declaration");
    }

    // Pin the C standard to gnu17 (pre-C23). GCC 16 defaults to gnu23, where an
    // empty parameter list `()` means `(void)` rather than the historical
    // "unspecified arguments" — fontconfig 2.18.3 is older C with such
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
    // `b.addLibrary(.{ .name = "fontconfig" })` (build.zig:108-116).
    build.compile("fontconfig");

    // The reference links pthread off-Windows (build.zig:126-128). On modern
    // glibc (>= 2.34) pthread is folded into libc so this is a no-op stub, but
    // it is emitted for parity and for older/musl toolchains.
    if !windows {
        println!("cargo:rustc-link-lib=pthread");
    }

    // Auto-generate the FFI from `wrapper.h` (`<fontconfig/fontconfig.h>`)
    // against the same include path the C build uses: `<fontconfig/fontconfig.h>`
    // resolves to `override/fontconfig/fontconfig.h` (the tarball ships only
    // `fontconfig.h.in`).
    let mut bindgen_builder =
        bindgen::Builder::default().header(manifest_dir.join("wrapper.h").to_string_lossy());
    for dir in &override_dirs {
        bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", dir.display()));
    }
    let bindings = bindgen_builder
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
    // `DEP_FONTCONFIG_INCLUDE` / `DEP_FONTCONFIG_OVERRIDE_INCLUDE`; the
    // override root carries `fontconfig/fontconfig.h`, the header the reference
    // installs (build.zig:333-336).
    println!("cargo:include={}", vendor.display());
    println!("cargo:override_include={}", override_root.display());

    // The override headers are all header-only inputs of every TU.
    println!("cargo:rerun-if-changed=vendor/override");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
