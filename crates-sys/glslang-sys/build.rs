//! Build script for glslang-sys (glslang 14.2.0 GLSL/HLSL -> SPIR-V compiler).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C++ sources for glslang 14.2.0 are vendored in-tree under
//!      `vendor/` (fetched from the exact URL in the reference's
//!      pkg/glslang/build.zig.zon — the deps.files.ghostty.org mirror of the
//!      `KhronosGroup/glslang` 14.2.0 release). This build NEVER references
//!      /Users/gael/projects/ghostty.
//!   2. They are compiled with the `cc` crate in C++17 mode, mirroring the
//!      reference's pkg/glslang/build.zig `buildGlslang` exactly: the same ~40
//!      C++ translation units under `glslang/` (GenericCodeGen,
//!      MachineIndependent + its `preprocessor/`, the C interface,
//!      ResourceLimits, and the platform `OSDependent/{Unix,Windows}/ossource`)
//!      and `SPIRV/` (incl. the SPIR-V C interface). The include path is the
//!      vendored root (`vendor/`) plus the `override/` dir, exactly like the
//!      reference (`lib.addIncludePath(upstream.path(""))` +
//!      `lib.addIncludePath(b.path("override"))`). The reference's ubsan
//!      opt-outs (`-fno-sanitize=undefined -fno-sanitize-trap=undefined`) are
//!      applied, and `-fPIC` on freebsd/musl. Vendored upstream warnings are
//!      silenced.
//!   3. `override/glslang/build_info.h` is the CMake-generated version header
//!      that upstream normally produces from `build_info.h.tmpl` via
//!      `build_info.py`. ghostty checks a fixed copy into pkg/glslang/override;
//!      it is a ghostty-authored build input and is vendored verbatim under
//!      `vendor/override/`, placed on the include path so
//!      `#include "glslang/build_info.h"` (ShaderLang.cpp / GlslangToSpv.cpp)
//!      resolves to it. The HLSL backend (`ENABLE_HLSL`) and the SPIRV-Tools
//!      optimizer (`ENABLE_OPT`) are OFF (the reference defines neither), so
//!      those headers are never pulled in and are not vendored.
//!   4. `bindgen` auto-generates the FFI from `wrapper.h` — the same two public
//!      glslang C headers the reference's `c.zig` @cImports
//!      (`glslang/Include/glslang_c_interface.h` +
//!      `glslang/Public/resource_limits_c.h`).
//!   5. `cargo:rustc-link-*` is emitted by `cc` (static archive `libglslang`);
//!      `cc` also puts the C++ standard library on the link line. Because the
//!      crate declares `links = "glslang"`, Cargo enforces a single linkage;
//!      the vendored include root is re-exported via `DEP_GLSLANG_INCLUDE`.

use std::env;
use std::path::PathBuf;

/// glslang translation units compiled into the static library.
///
/// This is the verbatim `files` list from the reference's
/// pkg/glslang/build.zig `buildGlslang` (`lib.addCSourceFiles`), minus the
/// platform `OSDependent` TU which is selected per-OS below.
const SOURCES: &[&str] = &[
    // GenericCodeGen
    "glslang/GenericCodeGen/CodeGen.cpp",
    "glslang/GenericCodeGen/Link.cpp",
    // MachineIndependent
    "glslang/MachineIndependent/glslang_tab.cpp",
    "glslang/MachineIndependent/attribute.cpp",
    "glslang/MachineIndependent/Constant.cpp",
    "glslang/MachineIndependent/iomapper.cpp",
    "glslang/MachineIndependent/InfoSink.cpp",
    "glslang/MachineIndependent/Initialize.cpp",
    "glslang/MachineIndependent/IntermTraverse.cpp",
    "glslang/MachineIndependent/Intermediate.cpp",
    "glslang/MachineIndependent/ParseContextBase.cpp",
    "glslang/MachineIndependent/ParseHelper.cpp",
    "glslang/MachineIndependent/PoolAlloc.cpp",
    "glslang/MachineIndependent/RemoveTree.cpp",
    "glslang/MachineIndependent/Scan.cpp",
    "glslang/MachineIndependent/ShaderLang.cpp",
    "glslang/MachineIndependent/SpirvIntrinsics.cpp",
    "glslang/MachineIndependent/SymbolTable.cpp",
    "glslang/MachineIndependent/Versions.cpp",
    "glslang/MachineIndependent/intermOut.cpp",
    "glslang/MachineIndependent/limits.cpp",
    "glslang/MachineIndependent/linkValidate.cpp",
    "glslang/MachineIndependent/parseConst.cpp",
    "glslang/MachineIndependent/reflection.cpp",
    "glslang/MachineIndependent/preprocessor/Pp.cpp",
    "glslang/MachineIndependent/preprocessor/PpAtom.cpp",
    "glslang/MachineIndependent/preprocessor/PpContext.cpp",
    "glslang/MachineIndependent/preprocessor/PpScanner.cpp",
    "glslang/MachineIndependent/preprocessor/PpTokens.cpp",
    "glslang/MachineIndependent/propagateNoContraction.cpp",
    // C Interface
    "glslang/CInterface/glslang_c_interface.cpp",
    // ResourceLimits
    "glslang/ResourceLimits/ResourceLimits.cpp",
    "glslang/ResourceLimits/resource_limits_c.cpp",
    // SPIRV
    "SPIRV/GlslangToSpv.cpp",
    "SPIRV/InReadableOrder.cpp",
    "SPIRV/Logger.cpp",
    "SPIRV/SpvBuilder.cpp",
    "SPIRV/SpvPostProcess.cpp",
    "SPIRV/doc.cpp",
    "SPIRV/disassemble.cpp",
    "SPIRV/CInterface/spirv_c_interface.cpp",
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    let override_dir = vendor.join("override");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    let mut build = cc::Build::new();
    build
        // Windows (MSVC ABI), task F-1 (N353): the reference compiles this with
        // Zig's clang even when `abi == .msvc`, so on an MSVC target cc drives
        // clang-cl (the MSVC-ABI clang driver; a CC/CXX override still wins),
        // not cl.exe. A no-op on every other target.
        .prefer_clang_cl_over_msvc(true)
        // glslang is C++; the reference links libc++ (lib.linkLibCpp()).
        .cpp(true)
        .std("c++17")
        // Reference include path: the upstream root (root-relative includes like
        // `glslang/Include/...`, `SPIRV/...`, `StandAlone/...`) plus the
        // `override/` dir (supplies the generated `glslang/build_info.h`).
        .include(&vendor)
        .include(&override_dir)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w")
        // Reference flags (pkg/glslang/build.zig `buildGlslang`): disable ubsan
        // so release builds don't need the ubsan runtime symbols.
        .flag_if_supported("-fno-sanitize=undefined")
        .flag_if_supported("-fno-sanitize-trap=undefined");

    // The reference adds `-fPIC` on freebsd and musl targets.
    if target_os == "freebsd" || target_env == "musl" {
        build.flag_if_supported("-fPIC");
    }

    // GCC 16 / libstdc++ no longer transitively pulls <cstdint> in through the
    // STL headers glslang relies on (<stack>, <unordered_map>, ...), so several
    // SPIRV/ headers (e.g. SpvBuilder.h) reference uint32_t before it is
    // declared. The reference builds with Zig's bundled clang/libc++ (which
    // still leaks <cstdint>), so it never hit this. Force the include to restore
    // the same visibility without touching the pristine vendored sources — this
    // is semantically inert: <cstdint> only supplies the fixed-width integer
    // typedefs the code already assumes are in scope.
    if target_env == "msvc" {
        build.flag_if_supported("/FIcstdint");
    } else {
        build.flag("-include").flag("cstdint");
    }

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

    for src in SOURCES {
        build.file(vendor.join(src));
        println!("cargo:rerun-if-changed=vendor/{src}");
    }

    // Platform OSDependent TU (reference selects Unix vs. Windows).
    let ossource = if target_os == "windows" {
        "glslang/OSDependent/Windows/ossource.cpp"
    } else {
        "glslang/OSDependent/Unix/ossource.cpp"
    };
    build.file(vendor.join(ossource));
    println!("cargo:rerun-if-changed=vendor/{ossource}");

    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=glslang` + search-path directives (and the
    // C++ stdlib link directive). Because the crate declares `links =
    // "glslang"`, Cargo enforces a single linkage.
    build.compile("glslang");

    // Auto-generate the FFI from the same public headers the reference's
    // `c.zig` @cImports (see wrapper.h), resolved against the vendored root and
    // the override dir (for `glslang/build_info.h`, transitively pulled).
    let mut builder = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", vendor.display()))
        .clang_arg(format!("-I{}", override_dir.display()));

    // On macOS, point libclang at the active SDK so `<stdbool.h>`/`<stdlib.h>`
    // (pulled by glslang_c_interface.h) resolve. `cc` auto-supplies the SDK
    // sysroot for compilation, but libclang (bindgen) does not.
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
        // Emit the glslang C interface surface (glslang_* functions/types +
        // GLSLANG_* enums) plus the resource-limits accessor.
        .allowlist_function("glslang_.*")
        .allowlist_type("glslang_.*")
        .allowlist_var("GLSLANG_.*")
        // The glslang enum variants are unique C global-namespace constants
        // (`GLSLANG_STAGE_VERTEX`, `GLSLANG_SOURCE_GLSL`, ...). Drop bindgen's
        // enum-name prefix so they surface unadorned, exactly as the reference's
        // Zig `@cImport` exposes them (`c.GLSLANG_STAGE_VERTEX`).
        .prepend_enum_name(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate glslang-sys bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("unable to write glslang-sys bindings");

    // Re-export the vendored glslang include root for downstream crates that
    // build against the glslang headers (rustty-renderer's shader compilation).
    // Cargo sets `DEP_GLSLANG_INCLUDE`.
    println!("cargo:include={}", vendor.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
