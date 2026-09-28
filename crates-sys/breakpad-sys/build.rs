//! Build script for breakpad-sys (google breakpad crash-dump client, pin
//! b99f444b — the reference's pkg/breakpad build.zig.zon dependency).
//!
//! PORT STATUS: vendored (mirrors pkg/breakpad/build.zig)
//!
//! R3 — self-contained, no ghostty dependency:
//!   1. The upstream breakpad sources are vendored in-tree under `vendor/`
//!      (fetched from the exact URL in the reference's pkg/breakpad
//!      build.zig.zon — the deps.files.ghostty.org mirror of google/breakpad
//!      commit b99f444b). The vendor tree mirrors the upstream root layout
//!      (`src/…`), restricted to the compiled client subtree + its transitive
//!      header closure. PORT NOTE: the reference hosts
//!      `third_party/lss/linux_syscall_support.h` in its OWN
//!      pkg/breakpad/vendor/ dir (the upstream tarball omits it; upstream DEPS
//!      checks it out at src/third_party/lss) and adds that dir as a second
//!      include path — here the same file is vendored at the upstream-canonical
//!      `vendor/src/third_party/lss/` location so the single `src` include
//!      path resolves the same `"third_party/lss/linux_syscall_support.h"`
//!      include to the same bytes.
//!   2. The sources are compiled with the `cc` crate, mirroring the
//!      reference's pkg/breakpad/build.zig exactly: the same `common` list on
//!      every target, the same per-target `common_apple`/`common_mac`/
//!      `common_linux` + `client_apple`/`client_mac`/`client_ios`/
//!      `client_linux` lists, an empty extra-flags list, C++ with libc++
//!      (`link_libcpp = true` → `cpp(true)`; `.mm` ObjC++ and `.S` asm are
//!      dispatched by clang on extension, as with `zig build`).
//!   3. No bindgen: breakpad exposes no C API and the reference consumes it
//!      link-only (pkg/sentry `backend = .breakpad` links the artifact).
//!      `cargo:include` re-exports the vendored `src` header root (the
//!      reference's `installHeadersDirectory(upstream.path("src"), "")`) as
//!      `DEP_BREAKPAD_INCLUDE` for sentry-sys.

use std::env;
use std::path::PathBuf;

/// `const common` in pkg/breakpad/build.zig — compiled on every target.
const COMMON: &[&str] = &[
    "src/common/convert_UTF.cc",
    "src/common/md5.cc",
    "src/common/string_conversion.cc",
];

/// `const common_linux` in pkg/breakpad/build.zig.
const COMMON_LINUX: &[&str] = &[
    "src/common/linux/elf_core_dump.cc",
    "src/common/linux/elfutils.cc",
    "src/common/linux/file_id.cc",
    "src/common/linux/guid_creator.cc",
    "src/common/linux/linux_libc_support.cc",
    "src/common/linux/memory_mapped_file.cc",
    "src/common/linux/safe_readlink.cc",
    "src/common/linux/scoped_pipe.cc",
    "src/common/linux/scoped_tmpfile.cc",
    "src/common/linux/breakpad_getcontext.S",
];

/// `const common_apple` in pkg/breakpad/build.zig (all Darwin targets).
const COMMON_APPLE: &[&str] = &[
    "src/common/mac/arch_utilities.cc",
    "src/common/mac/file_id.cc",
    "src/common/mac/macho_id.cc",
    "src/common/mac/macho_utilities.cc",
    "src/common/mac/macho_walker.cc",
    "src/common/mac/string_utilities.cc",
];

/// `const common_mac` in pkg/breakpad/build.zig (macOS only, not iOS).
const COMMON_MAC: &[&str] = &[
    "src/common/mac/MachIPC.mm",
    "src/common/mac/bootstrap_compat.cc",
];

/// `const client_linux` in pkg/breakpad/build.zig.
const CLIENT_LINUX: &[&str] = &[
    "src/client/minidump_file_writer.cc",
    "src/client/linux/crash_generation/crash_generation_client.cc",
    "src/client/linux/crash_generation/crash_generation_server.cc",
    "src/client/linux/dump_writer_common/thread_info.cc",
    "src/client/linux/dump_writer_common/ucontext_reader.cc",
    "src/client/linux/handler/exception_handler.cc",
    "src/client/linux/handler/minidump_descriptor.cc",
    "src/client/linux/log/log.cc",
    "src/client/linux/microdump_writer/microdump_writer.cc",
    "src/client/linux/minidump_writer/linux_core_dumper.cc",
    "src/client/linux/minidump_writer/linux_dumper.cc",
    "src/client/linux/minidump_writer/linux_ptrace_dumper.cc",
    "src/client/linux/minidump_writer/minidump_writer.cc",
    "src/client/linux/minidump_writer/pe_file.cc",
];

/// `const client_apple` in pkg/breakpad/build.zig (all Darwin targets).
const CLIENT_APPLE: &[&str] = &[
    "src/client/minidump_file_writer.cc",
    "src/client/mac/handler/breakpad_nlist_64.cc",
    "src/client/mac/handler/dynamic_images.cc",
    "src/client/mac/handler/minidump_generator.cc",
];

/// `const client_mac` in pkg/breakpad/build.zig (macOS only).
const CLIENT_MAC: &[&str] = &[
    "src/client/mac/handler/exception_handler.cc",
    "src/client/mac/crash_generation/crash_generation_client.cc",
];

/// `const client_ios` in pkg/breakpad/build.zig (iOS only).
const CLIENT_IOS: &[&str] = &[
    "src/client/ios/exception_handler_no_mach.cc",
    "src/client/ios/handler/ios_exception_minidump_generator.mm",
    "src/client/mac/crash_generation/ConfigFile.mm",
    "src/client/mac/handler/protected_memory_allocator.cc",
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let vendor = manifest_dir.join("vendor");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let is_darwin = target_os == "macos" || target_os == "ios";

    let mut build = cc::Build::new();
    build
        // Windows (MSVC ABI), task F-1 (N353): the reference compiles this with
        // Zig's clang even when `abi == .msvc`, so on an MSVC target cc drives
        // clang-cl (the MSVC-ABI clang driver; a CC/CXX override still wins),
        // not cl.exe. A no-op on every other target.
        .prefer_clang_cl_over_msvc(true)
        // Reference: `link_libcpp = true` — the whole library is C++ (clang
        // dispatches the .mm ObjC++ and .S asm members by extension). cc emits
        // the platform C++ runtime link (c++ on Darwin, stdc++ elsewhere), the
        // equivalent of the reference's linkLibCpp.
        .cpp(true)
        // Reference: `zig build` drives clang with its default C++ standard
        // (gnu++17); Apple's `c++` driver may default lower, so pin it — the
        // sources use std::optional (C++17).
        //
        // On MSVC targets cc spells `.std()` as `-std:gnu++17`, which cl.exe
        // (D9002) and clang-cl ("argument unused during compilation", the
        // default C++14 then applies) both ignore, MEASURED on the Windows
        // lane (task F-1, N353). clang-cl takes the clang spelling through
        // `/clang:`.
        .flag(if target_env == "msvc" {
            "/clang:-std=gnu++17"
        } else {
            "-std=gnu++17"
        })
        // Reference: `lib.root_module.addIncludePath(upstream.path("src"))`,
        // plus `b.path("vendor")` for the lss header (see the PORT NOTE in the
        // module docs — both resolve against `vendor/src` here).
        .include(vendor.join("src"))
        // Vendored upstream code: do not let its warnings fail the host build.
        .warnings(false)
        .flag_if_supported("-w");

    // The reference's `flags` list is EMPTY for breakpad (no -D defines).

    // The sibling -sys crates add -fPIC on FreeBSD/musl (the reference's PIC
    // default under zig build).
    if target_os == "freebsd" || target_env == "musl" {
        build.flag_if_supported("-fPIC");
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

    // Per-target source lists, verbatim from pkg/breakpad/build.zig.
    let mut files: Vec<&str> = COMMON.to_vec();
    if is_darwin {
        files.extend_from_slice(COMMON_APPLE);
        files.extend_from_slice(CLIENT_APPLE);
        match target_os.as_str() {
            "macos" => {
                files.extend_from_slice(COMMON_MAC);
                files.extend_from_slice(CLIENT_MAC);
            }
            "ios" => files.extend_from_slice(CLIENT_IOS),
            _ => unreachable!(),
        }
    }
    if target_os == "linux" {
        files.extend_from_slice(COMMON_LINUX);
        files.extend_from_slice(CLIENT_LINUX);
    }

    for src in &files {
        build.file(vendor.join(src));
        println!("cargo:rerun-if-changed=vendor/{src}");
    }

    // Compiles the vendored sources into a static archive and emits the
    // `cargo:rustc-link-lib=static=breakpad` + search-path directives. Because
    // the crate declares `links = "breakpad"`, Cargo enforces a single linkage.
    build.compile("breakpad");

    // System frameworks the archive references on Darwin (CFString/CFData/… in
    // MinidumpGenerator::GatherSystemInformation and MacStringUtils). The
    // reference gets these from the app link's apple-sdk framework set; here
    // the crate names its own requirement so any consumer binary (e.g. the
    // sentry-sys KAT harness) links.
    if is_darwin {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    }

    // Expose the built archive to the crate's own KAT test (which asserts the
    // reference's per-target client objects are really in the archive).
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!(
        "cargo:rustc-env=BREAKPAD_SYS_ARCHIVE={}",
        out_dir.join("libbreakpad.a").display()
    );

    // Re-export the vendored header root for downstream crates (sentry-sys).
    // The reference installs `upstream.path("src")` as the public header dir
    // and additionally exposes its lss `vendor/` dir to pkg/sentry; both are
    // served by `vendor/src` here. Cargo sets `DEP_BREAKPAD_INCLUDE`.
    println!("cargo:include={}", vendor.join("src").display());

    println!("cargo:rerun-if-changed=build.rs");
}
