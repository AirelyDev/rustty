//! Build script for sentry-sys (sentry-native 0.7.8 crash-reporting SDK).
//!
//! PORT STATUS: vendored (ghostty's product configuration)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream C sources for sentry-native 0.7.8 are vendored in-tree
//!      under `vendor/` (fetched from the exact URL in the reference's
//!      pkg/sentry/build.zig.zon — the deps.files.ghostty.org mirror of the
//!      `getsentry/sentry-native` 0.7.8 release). The vendor tree mirrors the
//!      upstream root layout (`include/`, `src/`, `vendor/`) so the in-tree
//!      relative includes (`"../vendor/mpack.h"`, `"transports/…"`, `<sentry.h>`)
//!      resolve unchanged. This build NEVER references /Users/gael/projects/ghostty.
//!   2. The sources are compiled with the `cc` crate, mirroring the reference's
//!      pkg/sentry/build.zig with ghostty's PRODUCT options
//!      (`backend = .breakpad`, `transport = .none` — src/build/SharedDeps.zig
//!      passes `.backend = .breakpad` whenever sentry is enabled; pkg/sentry's
//!      standalone `orelse .inproc` default is never what the product builds):
//!      the same `SRCS` translation-unit list, the same per-target
//!      path/symbolizer/unwinder/pageallocator/modulefinder file selection,
//!      the Linux-only `vendor/stb_sprintf.c`, the fixed
//!      `sentry_transport_none.c` transport and the C++
//!      `sentry_backend_breakpad.cpp` backend (compiled against the
//!      breakpad-sys headers and linked against its archive, the reference's
//!      `breakpad_dep.artifact("breakpad")`), and the same `-D` flags
//!      (`-DSENTRY_WITH_UNWINDER_LIBBACKTRACE` off-Windows /
//!      `-DSENTRY_WITH_UNWINDER_DBGHELP` on Windows; `-DSENTRY_BACKEND_BREAKPAD`).
//!      Platform detection (`SENTRY_PLATFORM_*`) is auto-derived by `sentry.h`
//!      from the compiler's predefined macros, so no extra defines are needed.
//!      Vendored upstream warnings are silenced; on MSVC the reference's ubsan
//!      opt-outs are applied; `-fPIC` on FreeBSD/musl.
//!   3. `bindgen` auto-generates the FFI from `wrapper.h` — the same public
//!      header the reference's `c.zig` @cImports (`sentry.h`).
//!   4. `cargo:rustc-link-*` is emitted by `cc` (static archive `libsentry`).
//!      The crate declares `links = "sentry"`, so Cargo enforces a single
//!      linkage; the vendored `include` dir is re-exported via
//!      `DEP_SENTRY_INCLUDE` for any downstream crate.

use std::env;
use std::path::PathBuf;

#[path = "src/modulefinder.rs"]
mod modulefinder;

/// sentry-native translation units compiled on every target.
///
/// This is the verbatim `srcs` list from the reference's pkg/sentry/build.zig
/// (`lib.addCSourceFiles`). Paths are upstream-root relative; `vendor/mpack.c`
/// is the bundled MessagePack encoder (upstream's own `vendor/` subtree).
const SRCS: &[&str] = &[
    "src/sentry_alloc.c",
    "src/sentry_backend.c",
    "src/sentry_core.c",
    "src/sentry_database.c",
    "src/sentry_envelope.c",
    "src/sentry_info.c",
    "src/sentry_json.c",
    "src/sentry_logger.c",
    "src/sentry_options.c",
    "src/sentry_os.c",
    "src/sentry_random.c",
    "src/sentry_ratelimiter.c",
    "src/sentry_scope.c",
    "src/sentry_session.c",
    "src/sentry_slice.c",
    "src/sentry_string.c",
    "src/sentry_sync.c",
    "src/sentry_transport.c",
    "src/sentry_utils.c",
    "src/sentry_uuid.c",
    "src/sentry_value.c",
    "src/sentry_tracing.c",
    "src/path/sentry_path.c",
    "src/transports/sentry_disk_transport.c",
    "src/transports/sentry_function_transport.c",
    "src/unwinder/sentry_unwinder.c",
    "vendor/mpack.c",
];

fn main() {
    println!("cargo:rerun-if-changed=src/modulefinder.rs");
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Crate `vendor/` root mirrors the upstream root layout, so every
    // upstream-root-relative path below joins directly.
    let vendor = manifest_dir.join("vendor");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let is_windows = target_os == "windows";

    let mut build = cc::Build::new();
    build
        // Reference include dirs: `upstream.path("include")` and
        // `upstream.path("src")`. `include` resolves `#include <sentry.h>`
        // (sentry_boot.h); `src` resolves the cross-directory quoted includes
        // (e.g. backends/…c does `#include "sentry_core.h"` and
        // `#include "transports/sentry_disk_transport.h"`).
        .include(vendor.join("include"))
        .include(vendor.join("src"))
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w");

    // Global `-D` flags, verbatim from the reference's `flags` list.
    if is_windows {
        build.define("SENTRY_WITH_UNWINDER_DBGHELP", None);
    } else {
        build.define("SENTRY_WITH_UNWINDER_LIBBACKTRACE", None);
    }
    // ghostty's product build fixes `backend = .breakpad` (SharedDeps.zig
    // passes `.backend = .breakpad` whenever `config.sentry` is enabled).
    build.define("SENTRY_BACKEND_BREAKPAD", None);

    // The reference disables ubsan on MSVC (Zig's ubsan runtime cannot be
    // bundled on Windows, leaving __ubsan_handle_* unresolved).
    if target_env == "msvc" {
        build
            .flag_if_supported("-fno-sanitize=undefined")
            .flag_if_supported("-fno-sanitize-trap=undefined");
    }
    // The reference adds -fPIC on FreeBSD and musl.
    if target_os == "freebsd" || target_env == "musl" {
        build.flag_if_supported("-fPIC");
    }

    // Collect the compiled files, mirroring the reference's per-target switches.
    let mut files: Vec<String> = SRCS.iter().map(|s| (*s).to_string()).collect();

    // Linux-only bundled printf (`stbsp_vsnprintf`, referenced by
    // sentry_utils.c only on glibc Linux).
    if target_os == "linux" {
        files.push("vendor/stb_sprintf.c".into());
    }

    // Symbolizer + Unwinder + path + pageallocator.
    if is_windows {
        files.extend(
            [
                "src/sentry_windows_dbghelp.c",
                "src/path/sentry_path_windows.c",
                "src/symbolizer/sentry_symbolizer_windows.c",
                "src/unwinder/sentry_unwinder_dbghelp.c",
            ]
            .iter()
            .map(|s| (*s).to_string()),
        );
    } else {
        files.extend(
            [
                "src/sentry_unix_pageallocator.c",
                "src/path/sentry_path_unix.c",
                "src/symbolizer/sentry_symbolizer_unix.c",
                "src/unwinder/sentry_unwinder_libbacktrace.c",
            ]
            .iter()
            .map(|s| (*s).to_string()),
        );
    }

    // Module finder (per OS), chosen by `src/modulefinder.rs` where it is
    // tested. An unsupported OS fails the build, as the reference's
    // `error.UnsupportedTarget` does (S38-F4).
    match modulefinder::modulefinder_source(&target_os) {
        Ok(Some(file)) => files.push(file.into()),
        Ok(None) => {}
        Err(warning) => {
            println!("cargo:warning={warning}");
            panic!("UnsupportedTarget: {warning}");
        }
    }

    // Transport: ghostty fixes `transport = .none`.
    files.push("src/transports/sentry_transport_none.c".into());

    // Backend: ghostty's product build fixes `backend = .breakpad`
    // (SharedDeps.zig). `sentry_backend_breakpad.cpp` is C++ where the rest of
    // sentry-native is C, so it is compiled by a C++ sub-build (the reference's
    // `zig build` dispatches per-extension within one library) and its object
    // is archived into the same `libsentry.a`. It compiles against the
    // breakpad client headers re-exported by breakpad-sys
    // (`DEP_BREAKPAD_INCLUDE` — the reference's installed pkg/breakpad header
    // dir + its lss `vendor/` include, see pkg/sentry/build.zig `.breakpad`
    // arm), and the final link resolves its `google_breakpad::*` symbols
    // against breakpad-sys's archive (`breakpad_dep.artifact("breakpad")`).
    // breakpad-sys also emits the platform C++ runtime link (the reference's
    // linkLibCpp), covering this object's libc++ references.
    let breakpad_include = env::var("DEP_BREAKPAD_INCLUDE")
        .expect("DEP_BREAKPAD_INCLUDE (exported by breakpad-sys) must be set");
    let mut backend_build = cc::Build::new();
    backend_build
        .cpp(true)
        // Match breakpad-sys: the breakpad headers this backend includes are
        // C++17 (the reference's zig-clang default is gnu++17).
        .std("gnu++17")
        .include(vendor.join("include"))
        .include(vendor.join("src"))
        .include(&breakpad_include)
        .warnings(false)
        .flag_if_supported("-w");
    // Same global `-D` flags as the C build (the reference passes one `flags`
    // list to every translation unit).
    if is_windows {
        backend_build.define("SENTRY_WITH_UNWINDER_DBGHELP", None);
    } else {
        backend_build.define("SENTRY_WITH_UNWINDER_LIBBACKTRACE", None);
    }
    backend_build.define("SENTRY_BACKEND_BREAKPAD", None);
    if target_env == "msvc" {
        backend_build
            .flag_if_supported("-fno-sanitize=undefined")
            .flag_if_supported("-fno-sanitize-trap=undefined");
    }
    if target_os == "freebsd" || target_env == "musl" {
        backend_build.flag_if_supported("-fPIC");
    }
    backend_build.file(vendor.join("src/backends/sentry_backend_breakpad.cpp"));
    println!("cargo:rerun-if-changed=vendor/src/backends/sentry_backend_breakpad.cpp");
    for obj in backend_build.compile_intermediates() {
        build.object(obj);
    }

    for src in &files {
        build.file(vendor.join(src));
        println!("cargo:rerun-if-changed=vendor/{src}");
    }

    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=sentry` + search-path directives. Because the
    // crate declares `links = "sentry"`, Cargo enforces a single linkage.
    build.compile("sentry");

    // Expose the built archive to the crate's own backend-selection KAT test
    // (which asserts the breakpad backend object is really archived and the
    // inproc backend is not).
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!(
        "cargo:rustc-env=SENTRY_SYS_ARCHIVE={}",
        out_dir.join("libsentry.a").display()
    );

    // System libraries the archive references (the equivalent of the reference's
    // `lib.linkLibC()` pulling these into the final link). On Darwin they live in
    // libSystem and are linked automatically; on other unices name them so the
    // KAT test binary links (dlopen/dladdr, pthreads, libm).
    if target_os == "linux" || target_os == "freebsd" {
        println!("cargo:rustc-link-lib=dylib=pthread");
        println!("cargo:rustc-link-lib=dylib=dl");
        println!("cargo:rustc-link-lib=dylib=m");
    }

    // Auto-generate the FFI from the same public header the reference's `c.zig`
    // @cImports (see wrapper.h: `sentry.h`), resolved against the vendored
    // include dir.
    let mut builder = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", vendor.join("include").display()));

    // On macOS, `cc` auto-supplies the SDK sysroot but libclang (bindgen) does
    // not — point it at the active SDK so the system headers sentry.h pulls
    // (stddef.h/stdint.h/stdbool.h) resolve.
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
        // Emit the sentry-native public surface (sentry_* functions/types and
        // the SENTRY_* macros/enum constants).
        .allowlist_function("sentry_.*")
        .allowlist_type("sentry_.*")
        .allowlist_var("SENTRY_.*")
        .allowlist_var("sentry_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate sentry-sys bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("unable to write sentry-sys bindings");

    // Re-export the vendored sentry include dir for any downstream crate. Cargo
    // sets `DEP_SENTRY_INCLUDE`.
    println!("cargo:include={}", vendor.join("include").display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
