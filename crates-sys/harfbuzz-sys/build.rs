//! Build script for harfbuzz-sys (HarfBuzz 11.0.0 text shaping engine).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C++ sources for HarfBuzz 11.0.0 are vendored in-tree under
//!      `vendor/src/` (fetched from the exact URL in the reference's
//!      pkg/harfbuzz/build.zig.zon — the
//!      `https://deps.files.ghostty.org/harfbuzz-11.0.0.tar.xz` mirror of the
//!      `harfbuzz/harfbuzz` v11.0.0 release). This build NEVER references
//!      /Users/gael/projects/ghostty.
//!   2. They are compiled with the `cc` crate in C++17 mode, mirroring the
//!      reference's pkg/harfbuzz/build.zig `buildLib` exactly: HarfBuzz builds
//!      as ONE amalgamation translation unit (`src/harfbuzz.cc`, which
//!      `#include`s the entire `src/` tree), with the include path `src` and
//!      the reference's flags (`-DHAVE_STDBOOL_H`; off-Windows
//!      `-DHAVE_UNISTD_H -DHAVE_SYS_MMAN_H -DHAVE_PTHREAD=1`; FreeType
//!      `-DHAVE_FREETYPE=1` plus the four `HAVE_FT_*` capability flags the
//!      reference hard-enables). CoreText is OFF for the vendored build (the
//!      reference defaults `enable-coretext` to false). Vendored upstream
//!      warnings are silenced.
//!   3. FreeType (`-DHAVE_FREETYPE=1`) is provided by the `freetype-sys` crate:
//!      its vendored header dir arrives via `DEP_FREETYPE_INCLUDE` (so
//!      `hb-ft.cc`/`hb-ft.h` resolve `<ft2build.h>` + `FT_FREETYPE_H`), and its
//!      static `freetype` archive is linked through the normal dependency (the
//!      reference does the equivalent `lib.linkLibrary(freetype)`).
//!   4. `bindgen` auto-generates the FFI from `wrapper.h` — the same public
//!      HarfBuzz headers the reference's `c.zig` @cImports (`hb.h` +, since
//!      FreeType is enabled, `hb-ft.h`).
//!   5. `cargo:rustc-link-*` is emitted by `cc` (static archive `libharfbuzz`).

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    let hb_src = vendor.join("src");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    // FreeType header dir, exported by freetype-sys via `cargo:include`
    // (`DEP_FREETYPE_INCLUDE`). Required because `-DHAVE_FREETYPE=1` makes
    // `src/hb-ft.cc` `#include <ft2build.h>` / `FT_FREETYPE_H`.
    let ft_include =
        env::var("DEP_FREETYPE_INCLUDE").expect("DEP_FREETYPE_INCLUDE not set by freetype-sys");

    let mut build = cc::Build::new();
    build
        // Windows (MSVC ABI), task F-1 (N353): the reference compiles this with
        // Zig's clang even when `abi == .msvc`, so on an MSVC target cc drives
        // clang-cl (the MSVC-ABI clang driver; a CC/CXX override still wins),
        // not cl.exe. A no-op on every other target.
        .prefer_clang_cl_over_msvc(true)
        // HarfBuzz is C++; the reference links libc++ (lib.linkLibCpp()).
        .cpp(true)
        .std("c++17")
        // The amalgamation `#include`s siblings by relative path, so `src` must
        // be on the include path (reference: lib.addIncludePath(upstream/src)).
        .include(&hb_src)
        // FreeType headers for hb-ft.cc (reference wires this via the freetype
        // dependency's include path).
        .include(&ft_include)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w")
        // Reference flags (pkg/harfbuzz/build.zig `buildLib`).
        .define("HAVE_STDBOOL_H", None)
        // FreeType is enabled (reference default `enable-freetype = true`), so
        // define HAVE_FREETYPE plus the four capability flags the reference
        // hard-enables ("Let's just assume a new freetype").
        .define("HAVE_FREETYPE", Some("1"))
        .define("HAVE_FT_GET_VAR_BLEND_COORDINATES", Some("1"))
        .define("HAVE_FT_SET_VAR_BLEND_COORDINATES", Some("1"))
        .define("HAVE_FT_DONE_MM_VAR", Some("1"))
        .define("HAVE_FT_GET_TRANSFORM", Some("1"));

    // Off-Windows the reference defines these (POSIX headers / pthread).
    if target_os != "windows" {
        build
            .define("HAVE_UNISTD_H", None)
            .define("HAVE_SYS_MMAN_H", None)
            .define("HAVE_PTHREAD", Some("1"));
    }

    // CoreText backend (macOS): the `coretext_harfbuzz` / `coretext_freetype`
    // backends shape with HarfBuzz over a CoreText-provided `hb_font_t` created
    // via `hb_coretext_font_create` (`src/hb-coretext-font.cc`). Enable the
    // capability and link the system frameworks HarfBuzz's CoreText integration
    // uses (CoreText/CoreGraphics/CoreFoundation, umbrella'd by
    // ApplicationServices). Additive: the FreeType `hb-ft` path is unchanged.
    if target_os == "macos" {
        build.define("HAVE_CORETEXT", Some("1"));
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
    }

    // The reference disables ubsan on MSVC (Zig's ubsan runtime cannot be
    // bundled on Windows, leaving __ubsan_handle_* unresolved).
    if target_env == "msvc" {
        build
            .flag_if_supported("-fno-sanitize=undefined")
            .flag_if_supported("-fno-sanitize-trap=undefined");
    }

    // HarfBuzz builds as ONE amalgamation TU (reference addCSourceFile:
    // upstream/src/harfbuzz.cc). It `#include`s the whole vendored `src/` tree.
    let amalgamation = "src/harfbuzz.cc";
    build.file(vendor.join(amalgamation));
    println!("cargo:rerun-if-changed=vendor/{amalgamation}");

    // Compiles the vendored amalgamation into a static archive and emits the
    // `cargo:rustc-link-lib=static=harfbuzz` + search-path directives. Because
    // the crate declares `links = "harfbuzz"`, Cargo enforces a single linkage.
    // C++ exceptions on MSVC targets (task F-1, N353). The reference passes no
    // exception flag, so Zig's clang compiles with its default, which on an MSVC
    // target is `-fcxx-exceptions -fexceptions` (MEASURED: `clang
    // --target=x86_64-pc-windows-msvc -### -x c++`). clang-cl enables C++
    // exceptions only through `/EH`, and cc passes none, so without this the
    // sources' `throw` is rejected ("cannot use 'throw' with exceptions
    // disabled", MEASURED on spirv-cross). `/EHs` gives exactly that cc1 pair
    // (`/EHsc` would add `-fexternc-nounwind`, which the reference does not).
    if target_env == "msvc" {
        build.flag("/EHs");
    }

    build.compile("harfbuzz");

    // Auto-generate the FFI from the same public headers the reference's
    // `c.zig` @cImports (see wrapper.h: `hb.h` + `hb-ft.h`). Uses the vendored
    // HarfBuzz src include dir plus the FreeType include dir (hb-ft.h pulls
    // <ft2build.h>).
    let mut builder = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", hb_src.display()))
        .clang_arg(format!("-I{ft_include}"));

    // On macOS, `wrapper.h` pulls `hb-coretext.h`, which includes the
    // `ApplicationServices` framework umbrella header. `cc` auto-supplies the
    // SDK sysroot, but libclang (bindgen) does not — point it at the active SDK
    // so the framework headers resolve.
    if target_os == "macos" {
        if let Ok(output) = std::process::Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
        {
            if output.status.success() {
                let sdk = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !sdk.is_empty() {
                    builder = builder.clang_arg(format!("-isysroot{sdk}"));
                }
            }
        }
    }

    let bindings = builder
        // Emit the HarfBuzz public surface (hb_* functions/types + HB_* macros).
        .allowlist_function("hb_.*")
        .allowlist_type("hb_.*")
        .allowlist_var("HB_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate harfbuzz-sys bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("unable to write harfbuzz-sys bindings");

    // Re-export the vendored HarfBuzz src (header) dir for downstream crates
    // that build against the HarfBuzz headers (rustty-font). Cargo sets
    // `DEP_HARFBUZZ_INCLUDE`.
    println!("cargo:include={}", hb_src.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
