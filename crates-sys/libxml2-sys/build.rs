//! Build script for libxml2-sys (libxml2 2.11.5 XML parser).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C sources for libxml2 2.11.5 are vendored in-tree under
//!      `vendor/` (fetched from the exact URL in the reference's
//!      pkg/libxml2/build.zig.zon:8 — the deps.files.ghostty.org mirror of the
//!      `GNOME/libxml2` `v2.11.5` tree). This build NEVER references
//!      /home/gael/projects/ghostty, and never links the host's system libxml2
//!      (2.12 on this box — the parity target is the reference's pinned 2.11.5).
//!   2. They are compiled with the `cc` crate, mirroring the reference's
//!      pkg/libxml2/build.zig `build` exactly: the same `srcs` list
//!      (build.zig:182-227, 44 translation units), the same include-path order
//!      (build.zig:19-26), the same hardcoded version/`HAVE_*` flags
//!      (build.zig:30-65), and the same default `Options` set expanded to
//!      `-DLIBXML_{FIELD}_ENABLED=1` plus its companion defines
//!      (build.zig:68-98).
//!   3. `vendor/override/` holds the reference's two pre-generated config
//!      headers verbatim (pkg/libxml2/override/{config/posix/config.h,
//!      config/win32/config.h, include/libxml/xmlversion.h}) — what upstream's
//!      `./configure` would otherwise template. Upstream ships only
//!      `include/libxml/xmlversion.h.in`, so the override supplies the real
//!      header; the rest of its content arrives as `-D` flags below.
//!   4. `bindgen` auto-generates the FFI from `wrapper.h` over the same include
//!      path the C build uses.
//!   5. `cargo:rustc-link-*` is emitted by `cc` (static archive `libxml2`).
//!      The vendored `include/` dir is re-exported via `DEP_XML2_INCLUDE` for
//!      downstream `fontconfig-sys`, whose `fcxml.c` needs the libxml2 headers.

use std::env;
use std::path::PathBuf;

/// libxml2 translation units compiled into the static library.
///
/// The verbatim `srcs` list from the reference's pkg/libxml2/build.zig:182-227.
/// Several of these are inert under the default `Options` (e.g. `nanoftp.c` is
/// wholly guarded by `LIBXML_FTP_ENABLED`, `xzlib.c` by `LIBXML_LZMA_ENABLED`),
/// but the reference hands the compiler the same list, so we do too.
const SOURCES: &[&str] = &[
    "buf.c",
    "c14n.c",
    "catalog.c",
    "chvalid.c",
    "debugXML.c",
    "dict.c",
    "encoding.c",
    "entities.c",
    "error.c",
    "globals.c",
    "hash.c",
    "HTMLparser.c",
    "HTMLtree.c",
    "legacy.c",
    "list.c",
    "nanoftp.c",
    "nanohttp.c",
    "parser.c",
    "parserInternals.c",
    "pattern.c",
    "relaxng.c",
    "SAX.c",
    "SAX2.c",
    "schematron.c",
    "threads.c",
    "tree.c",
    "uri.c",
    "valid.c",
    "xinclude.c",
    "xlink.c",
    "xmlIO.c",
    "xmlmemory.c",
    "xmlmodule.c",
    "xmlreader.c",
    "xmlregexp.c",
    "xmlsave.c",
    "xmlschemas.c",
    "xmlschemastypes.c",
    "xmlstring.c",
    "xmlunicode.c",
    "xmlwriter.c",
    "xpath.c",
    "xpointer.c",
    "xzlib.c",
];

/// The reference's `Options` struct (build.zig:144-180) — libxml2's `./configure`
/// `--with-X` switches — reduced to the fields whose DEFAULT is `true`. Each
/// becomes `-DLIBXML_{NAME}_ENABLED=1` (build.zig:70-77), name uppercased.
///
/// The default-`false` fields (`ftp`, `icu`, `legacy`, `mem_debug`, `run_debug`,
/// `thread_alloc`, `lzma`, `zlib`, `xptr_locs`) are deliberately absent: the
/// reference exposes them as `b.option` overrides but ghostty never sets them,
/// so the archive it links is built with exactly this set. Listed in the
/// reference's declaration order.
const ENABLED_OPTIONS: &[&str] = &[
    "C14N",
    "CATALOG",
    "DEBUG",
    "HISTORY",
    "HTML",
    "ICONV",
    "ISO8859X",
    "MINIMUM",
    "OUTPUT",
    "PATTERN",
    "PUSH",
    "READER",
    "REGEXP",
    "SAX1",
    "SCHEMAS",
    "SCHEMATRON",
    "THREAD",
    "TREE",
    "VALID",
    "WRITER",
    "XINCLUDE",
    "XPATH",
    "XPTR",
    "MODULES",
];

/// Version pieces, hardcoded exactly as the reference's `Version` struct
/// (build.zig:123-139).
const VERSION_MAJOR: &str = "2";
const VERSION_MINOR: &str = "11";
const VERSION_MICRO: &str = "5";

/// The reference's `Version.number()` (build.zig:128-130):
/// `major ++ "0" ++ minor ++ "0" ++ micro`.
///
/// NOTE (fidelity, deliberate): for 2.11.5 this yields **`"201105"`**, not the
/// `21105` upstream's `./configure` computes (`major*10000 + minor*100 + micro`).
/// The reference's concat only coincides with upstream when `minor` and `micro`
/// are single digits (e.g. 2.9.3 → `"20903"` ✓); with a two-digit minor it does
/// not. That is a latent bug in the reference, but R3 pins us to the archive
/// ghostty actually builds, and the value is **provably inert for our link
/// graph**: the sole consumer of `LIBXML_VERSION` in libxml2 is `xmlCheckVersion`
/// (parserInternals.c:64), which only runs if a client invokes the
/// `LIBXML_TEST_VERSION` macro — and fontconfig 2.14.2 (our only consumer)
/// references neither macro anywhere. So this reproduces the reference exactly
/// with no reachable behavior difference. `LIBXML_DOTTED_VERSION` — the string
/// anything human-facing reads — is correct either way (`"2.11.5"`).
/// Do not "fix" this to 21105 without re-deciding the parity question.
fn version_number() -> String {
    format!("{VERSION_MAJOR}0{VERSION_MINOR}0{VERSION_MICRO}")
}

/// The reference's `Version.dottedString()` (build.zig:136-138), quoted for `-D`.
fn version_dotted_quoted() -> String {
    format!("\"{VERSION_MAJOR}.{VERSION_MINOR}.{VERSION_MICRO}\"")
}

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    let xml_include = vendor.join("include");
    let override_include = vendor.join("override/include");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let windows = target_os == "windows";

    let mut build = cc::Build::new();

    // Include-path order mirrors the reference (build.zig:19-26): upstream
    // `include/` first, then `override/include` (which supplies the generated
    // `libxml/xmlversion.h` upstream ships only as a `.in` template), then the
    // platform config dir holding `config.h` (`libxml.h:29` includes it
    // unconditionally).
    build
        .include(&xml_include)
        .include(&override_include)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w");

    if windows {
        build.include(vendor.join("override/config/win32"));
        // The reference's `lib.linkSystemLibrary("ws2_32")` (build.zig:23) —
        // nanohttp/nanoftp's socket layer.
        println!("cargo:rustc-link-lib=ws2_32");
    } else {
        build.include(vendor.join("override/config/posix"));
    }

    // ---- Verbatim flag list (build.zig:30-47) --------------------------------
    build
        // Version info, hardcoded.
        .define("LIBXML_VERSION", Some(version_number().as_str()))
        .define(
            "LIBXML_VERSION_STRING",
            Some(format!("\"{}\"", version_number()).as_str()),
        )
        .define("LIBXML_VERSION_EXTRA", Some("\"\""))
        .define(
            "LIBXML_DOTTED_VERSION",
            Some(version_dotted_quoted().as_str()),
        )
        // "These might now always be true (particularly Windows) but for now we
        // just set them all." — the reference's comment (build.zig:37-38).
        .define("SEND_ARG2_CAST", Some(""))
        .define("GETHOSTBYNAME_ARG_CAST", Some(""))
        .define("GETHOSTBYNAME_ARG_CAST_CONST", Some(""))
        // Always on.
        .define("LIBXML_STATIC", Some("1"))
        .define("LIBXML_AUTOMATA_ENABLED", Some("1"))
        .define("WITHOUT_TRIO", Some("1"));

    // ---- Off-Windows HAVE_* probes (build.zig:48-66) -------------------------
    // The reference hardcodes these rather than probing; we mirror it exactly.
    if !windows {
        for have in [
            "HAVE_ARPA_INET_H",
            "HAVE_ARPA_NAMESER_H",
            "HAVE_DL_H",
            "HAVE_NETDB_H",
            "HAVE_NETINET_IN_H",
            "HAVE_PTHREAD_H",
            "HAVE_SHLLOAD",
            "HAVE_SYS_DIR_H",
            "HAVE_SYS_MMAN_H",
            "HAVE_SYS_NDIR_H",
            "HAVE_SYS_SELECT_H",
            "HAVE_SYS_SOCKET_H",
            "HAVE_SYS_TIMEB_H",
            "HAVE_SYS_TIME_H",
            "HAVE_SYS_TYPES_H",
        ] {
            build.define(have, Some("1"));
        }
    }

    // ---- `Options` → `-DLIBXML_{FIELD}_ENABLED=1` (build.zig:68-98) ----------
    for opt in ENABLED_OPTIONS {
        // `iconv` is the one option the consumer overrides per target: the
        // reference's only libxml2 consumer, pkg/fontconfig/build.zig:9-13,
        // passes `.iconv = libxml2_iconv_enabled`, whose default is
        // `target.result.os.tag != .windows` (Windows has no `iconv.h`), to
        // `b.lazyDependency("libxml2", ...)` (build.zig:226-230). N286.
        if *opt == "ICONV" && windows {
            continue;
        }
        build.define(&format!("LIBXML_{opt}_ENABLED"), Some("1"));
    }
    // The reference's per-field companion defines (build.zig:79-97). Only the
    // three whose field defaults to `true` are reachable: `history`, `regexp`,
    // `thread`. (`mem_debug` → `DEBUG_MEMORY_LOCATION=1` and `run_debug` →
    // `LIBXML_DEBUG_RUNTIME=1` default off, so they are not emitted.)
    build
        // history (build.zig:79-84)
        .define("HAVE_LIBHISTORY", Some("1"))
        .define("HAVE_LIBREADLINE", Some("1"))
        // regexp (build.zig:88-90)
        .define("LIBXML_UNICODE_ENABLED", Some("1"))
        // thread (build.zig:94-96)
        .define("HAVE_LIBPTHREAD", Some("1"));

    // Pin the C standard to gnu17 (pre-C23). GCC 16 defaults to gnu23, where an
    // empty parameter list `()` means `(void)` rather than the historical
    // "unspecified arguments" — libxml2 2.11.5 is C89-era source with several
    // such prototypes. The reference's clang toolchain (Zig 0.15.2) defaults to
    // gnu17, so this reproduces exactly how the vendored source was compiled
    // upstream; it is a front-end language-mode choice only, no codegen
    // difference. Same precedent as crates-sys/oniguruma-sys/build.rs:187.
    build.flag_if_supported("-std=gnu17");

    for src in SOURCES {
        build.file(vendor.join(src));
        println!("cargo:rerun-if-changed=vendor/{src}");
    }

    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=xml2` + search-path directives. Because the
    // crate declares `links = "xml2"`, Cargo enforces a single linkage. The
    // archive name matches the reference's `b.addLibrary(.{ .name = "xml2" })`
    // (build.zig:10).
    build.compile("xml2");

    // `LIBXML_ICONV_ENABLED` (see `ENABLED_OPTIONS`, matching the reference's
    // `Options.iconv = true` default at build.zig:153) makes `encoding.c`
    // reference `iconv`/`iconv_open`/`iconv_close`. On Apple platforms those
    // live in a standalone `libiconv`, NOT in `libSystem`, so a link that only
    // gets `-lSystem` (as rustc's `-nodefaultlibs` link line does) fails with
    // undefined `_iconv*` symbols the moment anything actually pulls in
    // `encoding.o` — e.g. this crate's own KAT test binary. The Zig reference
    // never hits this because its libc linkage supplies libiconv. glibc and
    // musl fold iconv into libc, so no directive is needed there.
    if target_os == "macos" || target_os == "ios" {
        println!("cargo:rustc-link-lib=iconv");
    }

    // Auto-generate the FFI from `wrapper.h` against the same include path the C
    // build uses.
    let bindings = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", xml_include.display()))
        .clang_arg(format!("-I{}", override_include.display()))
        .clang_arg(format!(
            "-I{}",
            vendor
                .join(if windows {
                    "override/config/win32"
                } else {
                    "override/config/posix"
                })
                .display()
        ))
        // The public tree/parser surface. Kept minimal per the plan: this is
        // what `fontconfig-sys`'s `fcxml.c` consumer and the KAT need. Widen
        // only if a downstream build actually requires more.
        .allowlist_function("xml.*")
        .allowlist_type("xml.*")
        .allowlist_type("_xml.*")
        .allowlist_var("xml.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate libxml2-sys bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("unable to write libxml2-sys bindings");

    // Re-export the vendored libxml2 include dir for downstream crates that
    // build against the libxml2 headers (fontconfig-sys, whose `fcxml.c`
    // `#include`s <libxml/parser.h> under `ENABLE_LIBXML2`). Cargo sets
    // `DEP_XML2_INCLUDE`. NOTE: consumers also need `override/include` on their
    // include path for `libxml/xmlversion.h`; it is exported separately below.
    println!("cargo:include={}", xml_include.display());
    println!("cargo:override_include={}", override_include.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
