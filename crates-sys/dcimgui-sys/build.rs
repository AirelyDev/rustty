//! Build script for dcimgui-sys (Dear ImGui 1.92.5 docking + Dear Bindings 0.17
//! C API — the reference's inspector UI toolkit).
//!
//! PORT STATUS: vendored (Dear ImGui core + FreeType backend + Dear Bindings C
//! API; platform rendering backends deferred — see Cargo.toml).
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream sources are vendored in-tree under `vendor/`:
//!      * `vendor/imgui/` — Dear ImGui 1.92.5 (docking) core, fetched from the
//!        exact release the reference's pkg/dcimgui/build.zig.zon pins
//!        (`ocornut/imgui` `v1.92.5-docking`).
//!      * `vendor/dcimgui/` — the Dear Bindings 0.17 generated C API, fetched
//!        from the exact tarball the reference pins
//!        (`https://deps.files.ghostty.org/DearBindings_v0.17_ImGui_v1.92.5-docking.tar.gz`).
//!      * `ext.cpp` — ghostty's own extension shim (pkg/dcimgui/ext.cpp), copied
//!        in so the reference's `ImFontConfig_ImFontConfig` /
//!        `ImGuiStyle_ImGuiStyle` C entry points exist. This build NEVER
//!        references /Users/gael/projects/ghostty.
//!   2. They are compiled with the `cc` crate in C++17 mode, mirroring the
//!      reference's pkg/dcimgui/build.zig `buildLib` exactly: the five Dear
//!      ImGui core `.cpp` TUs, the FreeType font backend
//!      (`misc/freetype/imgui_freetype.cpp`, compiled because SharedDeps.zig
//!      sets `.freetype = true`), the two Dear Bindings TUs (`dcimgui.cpp`,
//!      `dcimgui_internal.cpp`), and `ext.cpp` — with the include paths
//!      `vendor/imgui` + `vendor/dcimgui` and the reference's `-D` flags
//!      (`-DIMGUI_HAS_DOCK=1 -DIMGUI_USE_WCHAR32=1
//!      -DIMGUI_DISABLE_OBSOLETE_FUNCTIONS=1 -DIMGUI_ENABLE_FREETYPE=1`, plus
//!      the non-Windows `IMGUI_IMPL_API=extern "C"`). Vendored upstream warnings
//!      are silenced.
//!   3. FreeType (`-DIMGUI_ENABLE_FREETYPE=1`) is provided by the `freetype-sys`
//!      crate: its vendored header dir arrives via `DEP_FREETYPE_INCLUDE` (so
//!      `imgui_freetype.cpp` resolves `<ft2build.h>` + `FT_FREETYPE_H`), and its
//!      static `freetype` archive (which also carries the bundled libpng) is
//!      linked through the normal dependency — the reference does the equivalent
//!      `lib.linkLibrary(freetype_dep.artifact("freetype"))`.
//!   4. `bindgen` auto-generates the FFI from `wrapper.h` — the same public C
//!      header the reference's `main.zig` @cIncludes (`dcimgui.h`), with the two
//!      layout-affecting macros it @cDefines (`IMGUI_USE_WCHAR32`,
//!      `IMGUI_HAS_DOCK`) plus `IMGUI_DISABLE_OBSOLETE_FUNCTIONS` so the
//!      generated symbol set matches the compiled archive.
//!   5. `cargo:rustc-link-*` is emitted by `cc` (static archive `libdcimgui`).

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    let imgui = vendor.join("imgui");
    let dcimgui = vendor.join("dcimgui");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    // FreeType header dir, exported by freetype-sys via `cargo:include`
    // (`DEP_FREETYPE_INCLUDE`). Required because `-DIMGUI_ENABLE_FREETYPE=1`
    // makes `misc/freetype/imgui_freetype.cpp` `#include <ft2build.h>` /
    // `FT_FREETYPE_H` and call the FreeType API.
    let ft_include =
        env::var("DEP_FREETYPE_INCLUDE").expect("DEP_FREETYPE_INCLUDE not set by freetype-sys");

    let mut build = cc::Build::new();
    build
        // Dear ImGui + Dear Bindings are C++; the reference links libc++
        // (lib.linkLibCpp()). cc auto-links the C++ runtime for `cpp(true)`.
        .cpp(true)
        .std("c++17")
        // Include paths (reference: lib.addIncludePath(imgui) +
        // lib.addIncludePath(bindings)).
        .include(&imgui)
        .include(&dcimgui)
        // FreeType headers for imgui_freetype.cpp (reference wires this via the
        // freetype dependency's include path).
        .include(&ft_include)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w")
        // Reference flags common to all TUs (pkg/dcimgui/build.zig `buildLib`).
        .define("IMGUI_HAS_DOCK", "1")
        .define("IMGUI_USE_WCHAR32", "1")
        .define("IMGUI_DISABLE_OBSOLETE_FUNCTIONS", "1")
        // freetype == true (SharedDeps.zig sets `.freetype = true`).
        .define("IMGUI_ENABLE_FREETYPE", "1");

    // The reference defines IMGUI_IMPL_API for the backend impl headers. On
    // non-Windows it is `extern "C"` (Windows adds `__declspec(dllexport)`).
    // None of the TUs compiled here reference it (the platform backend impl
    // files are deferred), but it is part of the reference's "flags common to
    // all", so mirror it on non-Windows.
    if target_os != "windows" {
        build.define("IMGUI_IMPL_API", "extern \"C\"");
    }

    // The reference disables ubsan on MSVC (Zig's ubsan runtime cannot be
    // bundled on Windows, leaving __ubsan_handle_* unresolved).
    if target_env == "msvc" {
        build
            .flag_if_supported("-fno-sanitize=undefined")
            .flag_if_supported("-fno-sanitize-trap=undefined");
    }

    // The reference adds -fPIC on FreeBSD / musl.
    if target_os == "freebsd" || target_env == "musl" {
        build.flag_if_supported("-fPIC");
    }

    // On macOS the Metal rendering backend is compiled (as a separate ObjC++
    // TU below). Activate ext.cpp's `imgui_impl_metal.h` __has_include block by
    // defining ZIGPKG_IMGUI_ENABLE_METAL for the TU that compiles ext.cpp,
    // mirroring how the reference sets ZIGPKG_IMGUI_ENABLE_OPENGL3 when its
    // `backend-opengl3` option is on (pkg/dcimgui/build.zig:72-74). The
    // reference's own selector is `backend-metal = isDarwin`
    // (src/build/SharedDeps.zig:539).
    if target_os == "macos" {
        build.define("ZIGPKG_IMGUI_ENABLE_METAL", "1");
    }

    // Dear ImGui core translation units (reference addCSourceFiles).
    for src in [
        "imgui/imgui.cpp",
        "imgui/imgui_draw.cpp",
        "imgui/imgui_tables.cpp",
        "imgui/imgui_widgets.cpp",
        "imgui/imgui_demo.cpp",
        // FreeType font backend (reference: freetype == true).
        "imgui/misc/freetype/imgui_freetype.cpp",
        // Dear Bindings C API TUs (reference: bindings dependency).
        "dcimgui/dcimgui.cpp",
        "dcimgui/dcimgui_internal.cpp",
    ] {
        build.file(vendor.join(src));
        println!("cargo:rerun-if-changed=vendor/{src}");
    }
    // ghostty's extension shim (reference: lib.addCSourceFiles ext.cpp from the
    // package root). It #includes "imgui.h" and (namespaced) "dcimgui.h".
    build.file(manifest_dir.join("ext.cpp"));
    println!("cargo:rerun-if-changed=ext.cpp");

    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=dcimgui` + search-path directives. Because
    // the crate declares `links = "dcimgui"`, Cargo enforces a single linkage.
    build.compile("dcimgui");

    // Metal rendering backend (macOS only). The reference compiles
    // `backends/imgui_impl_metal.mm` when its `backend-metal` option is set,
    // which SharedDeps.zig turns on for all Darwin targets
    // (src/build/SharedDeps.zig:539) — pkg/dcimgui/build.zig:133-144 adds the
    // `.mm` with the same "flags common to all" TUs. It is Objective-C++, so it
    // cannot share the C++ `cc::Build` above (which would force `-x
    // objective-c++` onto every pure-C++ TU); compile it as its own archive.
    // Zig compiles `.mm` files ARC-off, so pass `-fno-objc-arc` to match. The
    // `.mm` `#include`s "imgui.h" (vendor/imgui) and "imgui_impl_metal.h"
    // (vendor/imgui/backends) and `#import`s <Metal/Metal.h> +
    // <QuartzCore/CAMetalLayer.h>.
    if target_os == "macos" {
        let backends = imgui.join("backends");
        let mut metal = cc::Build::new();
        metal
            .cpp(true)
            .std("c++17")
            // Compile as Objective-C++ with ARC disabled (the reference relies
            // on Zig's default ARC-off `.mm` compilation).
            .flag("-x")
            .flag("objective-c++")
            .flag("-fno-objc-arc")
            // Include paths: vendor/imgui (for "imgui.h") and vendor/imgui/
            // backends (for the sibling "imgui_impl_metal.h").
            .include(&imgui)
            .include(&backends)
            // Vendored upstream code: do not let its warnings fail the build.
            .warnings(false)
            .flag_if_supported("-w")
            // Same "flags common to all" TUs the reference applies (build.zig
            // `flags`): the layout macros, obsolete-function stripping, the
            // freetype flag (part of the common set), and the non-Windows
            // IMGUI_IMPL_API = extern "C" that names the backend's exported
            // symbols.
            .define("IMGUI_HAS_DOCK", "1")
            .define("IMGUI_USE_WCHAR32", "1")
            .define("IMGUI_DISABLE_OBSOLETE_FUNCTIONS", "1")
            .define("IMGUI_ENABLE_FREETYPE", "1")
            .define("IMGUI_IMPL_API", "extern \"C\"")
            .file(backends.join("imgui_impl_metal.mm"));
        metal.compile("dcimgui_metal");
        println!("cargo:rerun-if-changed=vendor/imgui/backends/imgui_impl_metal.mm");
        println!("cargo:rerun-if-changed=vendor/imgui/backends/imgui_impl_metal.h");

        // The Metal backend `#import`s <Metal/Metal.h> +
        // <QuartzCore/CAMetalLayer.h> and uses Foundation types; link the
        // frameworks so downstream binaries resolve those symbols.
        println!("cargo:rustc-link-lib=framework=Metal");
        println!("cargo:rustc-link-lib=framework=QuartzCore");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }

    // Auto-generate the FFI from the same public header the reference's
    // `main.zig` @cIncludes (see wrapper.h: `dcimgui.h`). The include path needs
    // both the Dear Bindings dir (for `dcimgui.h`) and the Dear ImGui dir (for
    // `imconfig.h`, which `dcimgui.h` includes).
    let mut builder = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", dcimgui.display()))
        .clang_arg(format!("-I{}", imgui.display()))
        // Match the reference's `main.zig` @cDefines (layout-affecting: widens
        // ImWchar to 32 bits + selects the docking API) plus the compiled
        // archive's obsolete-function stripping, so the generated bindings match
        // the actual symbols in `libdcimgui.a`.
        .clang_arg("-DIMGUI_USE_WCHAR32=1")
        .clang_arg("-DIMGUI_HAS_DOCK=1")
        .clang_arg("-DIMGUI_DISABLE_OBSOLETE_FUNCTIONS=1");

    // On macOS, libclang (bindgen) does not auto-supply the SDK sysroot the way
    // `cc` does — point it at the active SDK so `<stdint.h>` etc. resolve.
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
        // Emit the Dear ImGui C surface (the Dear Bindings `ImGui_*` functions,
        // the `ImGui*` enums/flags, and the `Im*` structs/typedefs).
        .allowlist_function("ImGui_.*")
        .allowlist_function("ImGui.*")
        .allowlist_function("Im.*")
        .allowlist_type("ImGui.*")
        .allowlist_type("Im.*")
        .allowlist_var("ImGui.*")
        .allowlist_var("Im.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate dcimgui-sys bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("unable to write dcimgui-sys bindings");

    // Re-export the vendored Dear Bindings header dir (the public C API,
    // `dcimgui.h`) for downstream crates. Cargo sets `DEP_DCIMGUI_INCLUDE`. The
    // sibling `imgui/` dir (for `imconfig.h`/`imgui.h`) lives next to it under
    // the same `vendor/` root.
    println!("cargo:include={}", dcimgui.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
