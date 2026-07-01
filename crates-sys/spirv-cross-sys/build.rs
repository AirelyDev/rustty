//! Build script for spirv-cross-sys (KhronosGroup SPIRV-Cross shader
//! cross-compiler; C API version 0.59).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C++ sources for SPIRV-Cross (upstream commit
//!      476f384eb7d9e48613c45179e502a15ab95b6b49) are vendored in-tree under
//!      `vendor/` (fetched from the exact URL in the reference's
//!      pkg/spirv-cross/build.zig.zon — the deps.files.ghostty.org mirror of the
//!      `KhronosGroup/SPIRV-Cross` release). This build NEVER references
//!      /Users/gael/projects/ghostty.
//!   2. They are compiled with the `cc` crate in C++17 mode, mirroring the
//!      reference's pkg/spirv-cross/build.zig `buildSpirvCross` exactly: the
//!      same seven C++ translation units — the Core (`spirv_cross.cpp`,
//!      `spirv_parser.cpp`, `spirv_cross_parsed_ir.cpp`, `spirv_cfg.cpp`), the
//!      C API (`spirv_cross_c.cpp`), and the GLSL + MSL backends
//!      (`spirv_glsl.cpp`, `spirv_msl.cpp`). The include path is the vendored
//!      upstream root (`vendor/`), exactly like the reference
//!      (`lib.addIncludePath(upstream.path(""))`). The reference's flags are
//!      applied verbatim: `-DSPIRV_CROSS_C_API_GLSL=1
//!      -DSPIRV_CROSS_C_API_MSL=1` (the HLSL/CPP/REFLECT C-API backends are
//!      OFF, so `spirv_hlsl`/`spirv_cpp`/`spirv_reflect` headers+TUs are never
//!      pulled in and are not vendored) plus the ubsan opt-outs
//!      (`-fno-sanitize=undefined -fno-sanitize-trap=undefined`), and `-fPIC`
//!      on freebsd/musl. Vendored upstream warnings are silenced.
//!   3. `bindgen` auto-generates the FFI from `wrapper.h` — the same public
//!      SPIRV-Cross C header the reference's `c.zig` @cImports
//!      (`spirv_cross_c.h`).
//!   4. `cargo:rustc-link-*` is emitted by `cc` (static archive
//!      `libspirv_cross`); `cc` also puts the C++ standard library on the link
//!      line (the reference's `lib.linkLibCpp()`). Because the crate declares
//!      `links = "spirv_cross"`, Cargo enforces a single linkage; the vendored
//!      include root is re-exported via `DEP_SPIRV_CROSS_INCLUDE`.

use std::env;
use std::path::PathBuf;

/// SPIRV-Cross translation units compiled into the static library.
///
/// This is the verbatim `files` list from the reference's
/// pkg/spirv-cross/build.zig `buildSpirvCross` (`lib.addCSourceFiles`).
const SOURCES: &[&str] = &[
    // Core
    "spirv_cross.cpp",
    "spirv_parser.cpp",
    "spirv_cross_parsed_ir.cpp",
    "spirv_cfg.cpp",
    // C
    "spirv_cross_c.cpp",
    // GLSL
    "spirv_glsl.cpp",
    // MSL
    "spirv_msl.cpp",
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    let mut build = cc::Build::new();
    build
        // SPIRV-Cross is C++; the reference links libc++ (lib.linkLibCpp()).
        .cpp(true)
        .std("c++17")
        // Reference include path: the vendored upstream root, so root-relative
        // includes (`spirv_cross.hpp`, `spirv_glsl.hpp`, ...) resolve.
        .include(&vendor)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w")
        // Reference flags (pkg/spirv-cross/build.zig `buildSpirvCross`): enable
        // the GLSL and MSL C-API backends (HLSL/CPP/REFLECT stay OFF).
        .define("SPIRV_CROSS_C_API_GLSL", Some("1"))
        .define("SPIRV_CROSS_C_API_MSL", Some("1"))
        // Reference flags: disable ubsan so release builds don't need the ubsan
        // runtime symbols.
        .flag_if_supported("-fno-sanitize=undefined")
        .flag_if_supported("-fno-sanitize-trap=undefined");

    // The reference adds `-fPIC` on freebsd and musl targets.
    if target_os == "freebsd" || target_env == "musl" {
        build.flag_if_supported("-fPIC");
    }

    for src in SOURCES {
        build.file(vendor.join(src));
        println!("cargo:rerun-if-changed=vendor/{src}");
    }

    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=spirv_cross` + search-path directives (and
    // the C++ stdlib link directive). Because the crate declares
    // `links = "spirv_cross"`, Cargo enforces a single linkage.
    build.compile("spirv_cross");

    // Auto-generate the FFI from the same public header the reference's `c.zig`
    // @cImports (see wrapper.h), resolved against the vendored root.
    let mut builder = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", vendor.display()));

    // On macOS, point libclang at the active SDK so `<stddef.h>`/`<stdbool.h>`
    // (pulled by spirv_cross_c.h) resolve. `cc` auto-supplies the SDK sysroot
    // for compilation, but libclang (bindgen) does not.
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
        // Emit the SPIRV-Cross C interface surface (spvc_* functions/types +
        // SPVC_* macros/enum variants) plus the `SpvId` word type the parse API
        // takes.
        .allowlist_function("spvc_.*")
        .allowlist_type("spvc_.*")
        .allowlist_type("SpvId")
        .allowlist_var("SPVC_.*")
        // The spvc enum variants are unique C global-namespace constants
        // (`SPVC_BACKEND_GLSL`, `SPVC_RESULT_SUCCESS`,
        // `SPVC_CAPTURE_MODE_TAKE_OWNERSHIP`, ...). Drop bindgen's enum-name
        // prefix so they surface unadorned, exactly as the reference's Zig
        // `@cImport` exposes them (`c.SPVC_BACKEND_GLSL`).
        .prepend_enum_name(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate spirv-cross-sys bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("unable to write spirv-cross-sys bindings");

    // Re-export the vendored SPIRV-Cross include root for downstream crates that
    // build against the SPIRV-Cross headers (the renderer's Metal-shader
    // cross-compilation). Cargo sets `DEP_SPIRV_CROSS_INCLUDE`.
    println!("cargo:include={}", vendor.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
