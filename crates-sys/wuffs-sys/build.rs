//! Build script for wuffs-sys (Wuffs 0.4.0-alpha.9 image codecs).
//!
//! PORT STATUS: vendored (full single-file library, ghostty's module subset)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream Wuffs single-file C library is vendored in-tree at
//!      `vendor/release/c/wuffs-v0.4.c` (fetched from the exact URL in the
//!      reference's pkg/wuffs/build.zig.zon — the deps.files.ghostty.org mirror
//!      of the `google/wuffs` v0.4.0-alpha.9 release). This build NEVER
//!      references /Users/gael/projects/ghostty.
//!   2. It is compiled with the `cc` crate as ONE C translation unit, mirroring
//!      the reference's pkg/wuffs/build.zig exactly: `-DWUFFS_IMPLEMENTATION`
//!      (turns the stb-style header into its implementation) plus the precise
//!      `WUFFS_CONFIG__MODULE__*` define list from pkg/wuffs/build.zig:4-15 —
//!      only
//!      the modules ghostty actually uses (MODULES gate + AUX__BASE/AUX__IMAGE +
//!      BASE + the ADLER32/CRC32/DEFLATE/JPEG/PNG/ZLIB codecs). The include dir
//!      is `release/c` (reference: `module.addIncludePath(release/c)`). The AUX
//!      modules are C++-only (`#if defined(__cplusplus)`-guarded); like the
//!      reference's `addCSourceFile` we compile as C, so those two defines are
//!      inert but kept for a faithful 1:1 mirror. Vendored upstream warnings are
//!      silenced.
//!   3. `bindgen` auto-generates the FFI from `wrapper.h`, which `#include`s the
//!      same `wuffs-v0.4.c` the reference's generated `wuffs_c_source` module
//!      includes (`pkg/wuffs/build.zig:19`) — WITHOUT
//!      `WUFFS_IMPLEMENTATION` (declarations only, matching the `@cImport`), but
//!      WITH the identical module defines so the declared surface matches the
//!      compiled object.
//!   4. `cargo:rustc-link-*` is emitted by `cc` (static archive `libwuffs`).
//!      The crate declares `links = "wuffs"`, so Cargo enforces a single
//!      linkage; the vendored `release/c` include dir is re-exported via
//!      `DEP_WUFFS_INCLUDE` for downstream crates.
//!
//! Reference `pkg/wuffs/include/{stdlib.h,string.h}` (`619555d1c`
//! build-without-libc) are mirrored at `include/`. They exist so the reference
//! can translate and compile wuffs without libc headers (wasm32-freestanding /
//! no Apple SDK). Since P7-W5 T2 (DECISIONS B8: kitty zlib decodes through
//! wuffs on every target the reference builds, wasm32 included) they are LIVE
//! on wasm32: `include/` goes first on the include path of the codec TU, the
//! shim TU and bindgen there, as the reference puts it first on every
//! non-Windows target. Native targets keep compiling against the real system
//! libc headers, as before (the headers only declare what libc declares).
//! Where no libc is linked (wasm32 other than WASI) `wasm_libc_stubs.c`
//! supplies the reference `main.zig`'s weak, hidden `calloc`/`free` stubs.
//!
//! wasm32 needs a C compiler that targets it (`CC_wasm32_unknown_unknown`, or
//! a `clang` with the WebAssembly backend), as the reference's Zig toolchain
//! does; Apple's clang has none.

use std::env;
use std::path::PathBuf;

/// The `WUFFS_CONFIG__MODULE__*` gate list — the verbatim `defines` array from
/// the reference's `pkg/wuffs/build.zig:4-15`.
///
/// CITATION CORRECTED by `P3-W9 T7` step 1: both this comment and the module
/// header above cited `pkg/wuffs/src/c.zig`, a file upstream **DELETED** in
/// `e8525c0fd` (Zig 0.16 removed `@cImport`); the array moved verbatim into
/// `build.zig`. The list itself was and is correct — re-verified byte-for-byte
/// against `origin/main:pkg/wuffs/build.zig:4-15`, same ten entries in the same
/// order — so only the provenance rotted. An oracle path that no longer
/// resolves makes fidelity auditing unreliable, which is why a comment-only
/// change is worth a commit (same defect class as the `.pot` `#:` references).
///
/// Enabling `WUFFS_CONFIG__MODULES` turns
/// on selective compilation; each `MODULE__*` then opts a single module in.
/// AUX__BASE/AUX__IMAGE are the C++ auxiliary helpers (inert under a C compile);
/// BASE is the shared runtime; the rest are the exact codecs ghostty decodes.
const MODULE_DEFINES: &[&str] = &[
    "WUFFS_CONFIG__MODULES",
    "WUFFS_CONFIG__MODULE__AUX__BASE",
    "WUFFS_CONFIG__MODULE__AUX__IMAGE",
    "WUFFS_CONFIG__MODULE__BASE",
    "WUFFS_CONFIG__MODULE__ADLER32",
    "WUFFS_CONFIG__MODULE__CRC32",
    "WUFFS_CONFIG__MODULE__DEFLATE",
    "WUFFS_CONFIG__MODULE__JPEG",
    "WUFFS_CONFIG__MODULE__PNG",
    "WUFFS_CONFIG__MODULE__ZLIB",
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");
    // Reference layout preserved: the source and its include dir are the
    // vendored `release/c` subtree.
    let c_dir = vendor.join("release").join("c");
    let source = c_dir.join("wuffs-v0.4.c");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();

    // The reference's `pkg/wuffs/build.zig` (`c0c5473da` end state):
    //   // Windows always has a libc available.
    //   const windows = target.result.os.tag == .windows;
    //   ...
    //   // Wuffs only needs stdlib.h and string.h from libc, and only for
    //   // a handful of declarations. We provide minimal versions of these
    //   // headers so that wuffs can be translated and compiled without
    //   // libc, notably for freestanding targets (wasm32) but this also
    //   // avoids requiring an Apple SDK for translate-c on macOS.
    //   false => &.{ b.path("include"), wuffs_dep.path("release/c") },
    // PORT NOTE (DECISIONS B8): applied on wasm32, the target that has no libc
    // headers here; native targets have their SDK's (see the module header).
    let wasm = target_arch == "wasm32";
    let libc_include = manifest_dir.join("include");
    // `builtin.link_libc` is false: nothing else provides `calloc`/`free`.
    // WASI links wasi-libc through Rust's std, so only the other wasm32 targets.
    let no_libc = wasm && target_os != "wasi";

    let mut build = cc::Build::new();
    if wasm {
        build.include(&libc_include);
    }
    build
        // Windows (MSVC ABI), task F-1 (N353): the reference compiles this with
        // Zig's clang even when `abi == .msvc`, so on an MSVC target cc drives
        // clang-cl (the MSVC-ABI clang driver; a CC/CXX override still wins),
        // not cl.exe. A no-op on every other target.
        .prefer_clang_cl_over_msvc(true)
        .include(&c_dir)
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w")
        // Reference flag: `-DWUFFS_IMPLEMENTATION` (stb-style: emit the impl).
        .define("WUFFS_IMPLEMENTATION", None);

    // The exact `WUFFS_CONFIG__MODULE__*` gate list (reference: c.zig `defines`,
    // applied by build.zig as `-D<key>`). Bare `-D` (value None) matches the
    // reference `flags.append("-D" ++ key)`.
    for def in MODULE_DEFINES {
        build.define(def, None);
    }

    // The reference (build.zig, `dff13b41c` end state) disables ubsan on ALL
    // Windows ABIs — was msvc-only:
    //   // Disable ubsan on Windows to avoid undefined __ubsan_handle_*
    //   // references: Zig's ubsan runtime can't be bundled on Windows
    //   // (its /exclude-symbols directives break the MSVC linker), so
    //   // these handlers would go unresolved. This affects both the
    //   // MSVC and GNU ABIs.
    //   if (windows) { -fno-sanitize=undefined; -fno-sanitize-trap=undefined; }
    // (`cc` never enables Zig's ubsan, so this mirrors the condition, not a
    // live hazard.)
    if target_os == "windows" {
        build
            .flag_if_supported("-fno-sanitize=undefined")
            .flag_if_supported("-fno-sanitize-trap=undefined");
    }

    // The wasm `simd128` default (N422, `src/build/Config.zig:113-130`) is a
    // property of the resolved target, so the reference compiles this C with
    // it too. cc-rs does not forward Rust's target features to the C compiler;
    // pass it when the Rust side has it (`.cargo/config.toml`).
    let simd128 = env::var("CARGO_CFG_TARGET_FEATURE")
        .unwrap_or_default()
        .split(',')
        .any(|f| f == "simd128");
    if wasm && simd128 {
        build.flag("-msimd128");
    }

    build.file(&source);
    println!("cargo:rerun-if-changed=vendor/release/c/wuffs-v0.4.c");

    // Compiles the single vendored TU into a static archive and emits the
    // `cargo:rustc-link-lib=static=wuffs` + search-path directives. Because the
    // crate declares `links = "wuffs"`, Cargo enforces a single linkage.
    build.compile("wuffs");

    // The static-inline shim (shim.c): a SECOND C translation unit compiled
    // WITHOUT `WUFFS_IMPLEMENTATION` (so it emits NO duplicate copy of the codec
    // implementation — only its `rustty_wuffs_*` external-linkage wrappers). Each
    // wrapper forwards to a Wuffs base-runtime `static inline` helper (fully
    // defined inline in the header, independent of the impl), so the TU is
    // self-contained. This exposes the ~5 `static inline` helpers the PNG/JPEG
    // decode path needs but bindgen cannot bind (it never emits a linkable symbol
    // for a `static inline`). Compiled into its OWN static archive; the `links =
    // "wuffs"` uniqueness key is unaffected (it gates the -sys crate, not the
    // count of `rustc-link-lib` directives).
    let mut shim = cc::Build::new();
    if wasm {
        shim.include(&libc_include);
    }
    // Same compiler as the codec TU above (task F-1, N353).
    shim.prefer_clang_cl_over_msvc(true)
        .include(&c_dir)
        .warnings(false)
        .flag_if_supported("-w");
    // The SAME module gate list, WITHOUT `WUFFS_IMPLEMENTATION` (declarations +
    // static inlines only — mirrors wrapper.h / the `@cImport`).
    for def in MODULE_DEFINES {
        shim.define(def, None);
    }
    if target_os == "windows" {
        shim.flag_if_supported("-fno-sanitize=undefined")
            .flag_if_supported("-fno-sanitize-trap=undefined");
    }
    if wasm && simd128 {
        shim.flag("-msimd128");
    }
    shim.file(manifest_dir.join("shim.c"));
    println!("cargo:rerun-if-changed=shim.c");
    println!("cargo:rerun-if-changed=shim.h");
    shim.compile("wuffs_shim");

    // The reference `main.zig`'s no-libc stubs:
    //   // Wuffs' generated `wuffs_foo__bar__alloc()` convenience functions are the
    //   // only code that references libc's calloc/free. [...] When libc isn't
    //   // linked there would be nothing to provide calloc/free if they survive,
    //   // so export stubs to satisfy the link. Weak so that any real definition
    //   // wins. Hidden keeps them out of the export table where the format
    //   // honors it (e.g. wasm).
    //   comptime { if (!builtin.link_libc) { @export(&callocStub, ...); ... } }
    // PORT NOTE: Rust has no stable weak linkage, so the two stubs are C, in
    // their own archive, compiled only where the reference exports them.
    if no_libc {
        let mut stubs = cc::Build::new();
        stubs
            .include(&libc_include)
            .warnings(false)
            .flag_if_supported("-w")
            .file(manifest_dir.join("wasm_libc_stubs.c"));
        stubs.compile("wuffs_libc_stubs");
    }
    println!("cargo:rerun-if-changed=wasm_libc_stubs.c");
    println!("cargo:rerun-if-changed=include/stdlib.h");
    println!("cargo:rerun-if-changed=include/string.h");

    // Auto-generate the FFI from `wrapper.h` (which `#include`s the same
    // `wuffs-v0.4.c` the reference's `c.zig` @cIncludes). The module defines are
    // passed here too — WITHOUT `WUFFS_IMPLEMENTATION`, mirroring the `@cImport`
    // (declarations only) — so the generated surface matches the compiled
    // object.
    let mut builder =
        bindgen::Builder::default().header(manifest_dir.join("wrapper.h").to_string_lossy());
    if wasm {
        builder = builder.clang_arg(format!("-I{}", libc_include.display()));
        // clang gives wasm targets hidden default visibility, and bindgen
        // emits no binding for a hidden function (it could not be linked from
        // another object): parse with default visibility so the declared
        // surface is the same as on every other target.
        builder = builder.clang_arg("-fvisibility=default");
    }
    builder = builder.clang_arg(format!("-I{}", c_dir.display()));
    for def in MODULE_DEFINES {
        builder = builder.clang_arg(format!("-D{def}=1"));
    }

    let bindings = builder
        // Emit the Wuffs public surface: the `wuffs_*` functions/types, the
        // `sizeof__wuffs_*` struct-size helpers, and the `WUFFS_*` constants.
        .allowlist_function("wuffs_.*")
        .allowlist_function("sizeof__wuffs_.*")
        // The static-inline shim wrappers (shim.h prototypes, shim.c definitions)
        // that expose the base-runtime `static inline` helpers the decode path
        // drives. Effective ONLY because wrapper.h `#include`s shim.h (bindgen must
        // parse a prototype to emit its binding).
        .allowlist_function("rustty_wuffs_.*")
        .allowlist_type("wuffs_.*")
        .allowlist_var("WUFFS_.*")
        // The suspension status strings (`extern const char
        // wuffs_base__suspension__short_write[]` etc.). The reference's
        // `pkg/wuffs/src/zlib.zig` compares a status `repr` against
        // `c.wuffs_base__suspension__short_write` by address (`4b86e3594`).
        .allowlist_var("wuffs_base__suspension__.*")
        // Wuffs' generated header is enormous; block the layout-tests bindgen
        // would otherwise emit for every internal struct (they slow the build
        // and add nothing to the FFI surface).
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("unable to generate wuffs-sys bindings");
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("unable to write wuffs-sys bindings");

    // Re-export the vendored `release/c` include dir for downstream crates that
    // build against the Wuffs header (Cargo sets `DEP_WUFFS_INCLUDE`).
    println!("cargo:include={}", c_dir.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=wrapper.h");
}
