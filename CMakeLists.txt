# CMake wrapper for librustty-vt
#
# This file delegates to `cargo build -p rustty-vt-c` to produce the shared and
# static libraries. It exists so that CMake-based projects can consume
# librustty-vt without interacting with the Cargo build system directly.
# Downstream users do still require `cargo` (the Rust toolchain) on the PATH.
# Please consult the Rustty docs for the required toolchain version (pinned in
# rust-toolchain.toml):
#
# https://rustty.sh/docs/install/build
#
# Building within the Rustty repo
# -------------------------------
#
#   cmake -B build
#   cmake --build build
#   cmake --install build --prefix /usr/local
#
# Pass extra flags to the Cargo build with RUSTTY_CARGO_BUILD_FLAGS:
#
#   cmake -B build -DRUSTTY_CARGO_BUILD_FLAGS="--features simd"
#
# Integrating into a downstream CMake project
# -------------------------------------------
#
# Option 1 — FetchContent (recommended, no manual install step):
#
#   include(FetchContent)
#   FetchContent_Declare(rustty
#       GIT_REPOSITORY https://github.com/AirelyDev/rustty.git
#       GIT_TAG main
#   )
#   FetchContent_MakeAvailable(rustty)
#
#   target_link_libraries(myapp PRIVATE rustty-vt)        # shared
#   target_link_libraries(myapp PRIVATE rustty-vt-static) # static
#
# To use a local checkout instead of fetching:
#
#   cmake -B build -DFETCHCONTENT_SOURCE_DIR_RUSTTY=/path/to/rustty
#
# Option 2 — find_package (after installing to a prefix):
#
#   find_package(rustty-vt REQUIRED)
#   target_link_libraries(myapp PRIVATE rustty-vt::rustty-vt)        # shared
#   target_link_libraries(myapp PRIVATE rustty-vt::rustty-vt-static) # static
#
# Cross-compilation
# -----------------
#
# For building librustty-vt for a non-native Rust target (e.g. cross-
# compiling), use the rustty_vt_add_target() function after FetchContent:
#
#   FetchContent_MakeAvailable(rustty)
#   rustty_vt_add_target(NAME linux-amd64 RUST_TARGET x86_64-unknown-linux-gnu)
#
#   target_link_libraries(myapp PRIVATE rustty-vt-static-linux-amd64) # static
#   target_link_libraries(myapp PRIVATE rustty-vt-linux-amd64)        # shared
#
# This handles the cargo invocation, build-type-to-profile mapping, and output
# path conventions internally. Extra flags can be forwarded with CARGO_FLAGS.
#
# See dist/cmake/README.md for more details, example/c-vt-cmake/ for a
# complete working example, and example/c-vt-cmake-cross/ for a cross-
# compilation example.

cmake_minimum_required(VERSION 3.19)
project(rustty-vt VERSION 0.1.0 LANGUAGES C)

# --- Options ----------------------------------------------------------------

set(RUSTTY_CARGO_BUILD_FLAGS "" CACHE STRING "Additional flags to pass to cargo build")

# Build librustty-vt with SIMD acceleration. Defaults ON to match upstream
# Ghostty's semantics (its `simd` build option defaults true for every non-WASM
# target); rustty-vt-c's underlying cargo feature is now default-ON too, so OFF
# has to actively opt out with --no-default-features. Turn OFF for a minimal
# static archive with no vendored C++ SIMD runtime (highway/simdutf) — this is
# the analog of Ghostty's `-Dsimd=false`.
option(RUSTTY_VT_SIMD "Build librustty-vt with SIMD acceleration (matches upstream default-on)" ON)

set(_RUSTTY_FEATURE_FLAGS "")
if(RUSTTY_VT_SIMD)
    list(APPEND _RUSTTY_FEATURE_FLAGS --features simd)
else()
    # `--no-default-features` alone is NO LONGER the `-Dsimd=false` analog.
    # Since the `-Dvt-features` work landed, rustty-vt-c's `default` carries
    # both `simd` AND the nine VT feature gates, so dropping defaults dropped
    # the entire C API surface: example/c-vt-cmake-static (which sets
    # RUSTTY_VT_SIMD=OFF) failed to link on _rustty_formatter_terminal_new,
    # _rustty_formatter_format_alloc and _rustty_formatter_free.
    #
    # `vt-features-default` is the aggregate of the nine, so this says exactly
    # "every VT feature, SIMD off" without restating the list here.
    list(APPEND _RUSTTY_FEATURE_FLAGS --no-default-features --features vt-features-default)
endif()

# Map CMake build types to Cargo profiles. The result is stored in
# _RUSTTY_CARGO_PROFILE (the target/ subdir) and _RUSTTY_PROFILE_FLAG (the
# cargo flag) so both the native build and rustty_vt_add_target() can reuse
# the mapping without duplicating logic. Cargo's default profile is `dev`
# (output dir target/debug); a Release-family build type maps to `--release`
# (output dir target/release).
set(_RUSTTY_CARGO_PROFILE "debug")
set(_RUSTTY_PROFILE_FLAG "")
if(CMAKE_BUILD_TYPE)
    string(TOUPPER "${CMAKE_BUILD_TYPE}" _bt)
    if(_bt STREQUAL "RELEASE" OR _bt STREQUAL "MINSIZEREL" OR _bt STREQUAL "RELWITHDEBINFO")
        set(_RUSTTY_CARGO_PROFILE "release")
        set(_RUSTTY_PROFILE_FLAG "--release")
    endif()
    unset(_bt)
endif()

# --- Find Cargo --------------------------------------------------------------

find_program(CARGO_EXECUTABLE cargo REQUIRED)
message(STATUS "Found cargo: ${CARGO_EXECUTABLE}")

# --- Build via cargo build ---------------------------------------------------

# cargo builds into target/<profile>/ relative to the workspace root. Unlike
# the Zig build, cargo emits a flat layout (no bin/ vs lib/ split) and no
# versioned soname / symlinks — just librustty_vt_c.{dylib,so,a} (the crate is
# `rustty-vt-c`, so the artifact stem is `rustty_vt_c`).
set(RUSTTY_OUT_DIR "${CMAKE_CURRENT_SOURCE_DIR}/target/${_RUSTTY_CARGO_PROFILE}")

# The public C headers are checked into the source tree (cbindgen output under
# include/rustty/), not generated into target/.
set(RUSTTY_INCLUDE_DIR "${CMAKE_CURRENT_SOURCE_DIR}/include")

# Shared + static library names (cargo produces both from crate-type
# ["cdylib","staticlib","rlib"]).
if(APPLE)
    set(RUSTTY_VT_REALNAME "${CMAKE_SHARED_LIBRARY_PREFIX}rustty_vt_c${CMAKE_SHARED_LIBRARY_SUFFIX}")
    set(RUSTTY_VT_SONAME "${RUSTTY_VT_REALNAME}")
elseif(WIN32)
    set(RUSTTY_VT_REALNAME "rustty_vt_c.dll")
    set(RUSTTY_VT_IMPLIB "rustty_vt_c.dll.lib")
else()
    set(RUSTTY_VT_REALNAME "${CMAKE_SHARED_LIBRARY_PREFIX}rustty_vt_c${CMAKE_SHARED_LIBRARY_SUFFIX}")
    # The versioned soname Zig gives libghostty-vt (S64-F5). This build asks
    # cargo for it (RUSTTY_VT_SONAME, see crates/rustty-vt-c/build.rs) and
    # links the name to the cargo artifact in the build tree below.
    set(RUSTTY_VT_SONAME "librustty-vt.so.${PROJECT_VERSION_MAJOR}")
endif()

set(RUSTTY_VT_SHARED_LIBRARY "${RUSTTY_OUT_DIR}/${RUSTTY_VT_REALNAME}")

# The INSTALLED names (S64-F5), the layout of GhosttyLibVt.zig:384-490 and
# Zig's shared-library install: a fully versioned real name, a major-version
# soname link and an unversioned link, and `librustty-vt.a` /
# `rustty-vt-static.lib` for the static archive. The build tree keeps cargo's
# `librustty_vt_c.*` names, which the examples link directly.
if(APPLE)
    set(RUSTTY_VT_INSTALL_REALNAME "librustty-vt.${PROJECT_VERSION}.dylib")
    set(RUSTTY_VT_INSTALL_SONAME "librustty-vt.${PROJECT_VERSION_MAJOR}.dylib")
    set(RUSTTY_VT_INSTALL_LINKNAME "librustty-vt.dylib")
elseif(WIN32)
    # A DLL cannot be renamed after linking: its import library names it. So
    # the DLL keeps cargo's name. The import library's FILE name is free (the
    # DLL name is recorded inside it), so it is installed as `rustty-vt.lib`,
    # the file `-lrustty-vt` in the .pc resolves to: Zig installs the
    # reference's as `ghostty-vt.lib`, which its `-lghostty-vt` names.
    set(RUSTTY_VT_INSTALL_REALNAME "${RUSTTY_VT_REALNAME}")
    set(RUSTTY_VT_INSTALL_SONAME "${RUSTTY_VT_REALNAME}")
    set(RUSTTY_VT_INSTALL_LINKNAME "${RUSTTY_VT_REALNAME}")
    set(RUSTTY_VT_INSTALL_IMPLIB "rustty-vt.lib")
else()
    set(RUSTTY_VT_INSTALL_REALNAME "librustty-vt.so.${PROJECT_VERSION}")
    set(RUSTTY_VT_INSTALL_SONAME "${RUSTTY_VT_SONAME}")
    set(RUSTTY_VT_INSTALL_LINKNAME "librustty-vt.so")
endif()
if(WIN32)
    set(RUSTTY_VT_STATIC_INSTALL_NAME "rustty-vt-static.lib")
else()
    set(RUSTTY_VT_STATIC_INSTALL_NAME "librustty-vt.a")
endif()

# Static library name. cargo names it librustty_vt_c.a on all Unix platforms
# and rustty_vt_c.lib on Windows (MSVC); use the platform static suffix.
if(WIN32)
    set(RUSTTY_VT_STATIC_REALNAME "rustty_vt_c.lib")
else()
    set(RUSTTY_VT_STATIC_REALNAME "librustty_vt_c.a")
endif()
set(RUSTTY_VT_STATIC_LIBRARY "${RUSTTY_OUT_DIR}/${RUSTTY_VT_STATIC_REALNAME}")

# Run cargo build -p rustty-vt-c (produces both shared + static).
set(_RUSTTY_CARGO_OUTPUTS "${RUSTTY_VT_SHARED_LIBRARY}" "${RUSTTY_VT_STATIC_LIBRARY}")
if(WIN32)
    list(APPEND _RUSTTY_CARGO_OUTPUTS "${RUSTTY_OUT_DIR}/${RUSTTY_VT_IMPLIB}")
endif()

# A custom TARGET, which always runs, not an add_custom_command(OUTPUT ...),
# which runs only when its declared outputs are missing.
#
# WHY. target/<profile>/librustty_vt_c.a is a single file that EVERY cargo
# invocation in this workspace overwrites, and its contents depend on the
# feature set of that invocation. A plain `cargo build --workspace` unifies
# `rustty-vt/oniguruma` on (another member enables it) and produces a ~115 MB
# archive carrying 23 oniguruma objects; this file's own
# `-p rustty-vt-c --features simd` produces a ~99 MB archive with none.
#
# With an OUTPUT-keyed command, `cmake --build` saw the file already present,
# skipped cargo entirely, and installed whichever archive some unrelated cargo
# run happened to leave behind — a different feature set than the one CMake was
# configured for, and one that would not be rebuilt after a source edit either.
# Confirmed by building here, then running `cargo build --workspace`, then
# `cmake --build` again: no cargo invocation, and the oniguruma objects stayed.
#
# cargo is already incremental, so an unconditional invocation costs a fraction
# of a second when nothing changed. Same reasoning as the WS19 T16 change to
# the example/*/build.sh scripts, which had the identical staleness bug.
if(APPLE)
    # The reference resolves every macOS target with a 13.0 minimum OS version
    # (Config.zig genericMacOSTarget / osVersionMin), so `zig build` stamps
    # 13.0 into the library it hands CMake. rustc's own default is older
    # (11.0 on arm64), so pass the same floor; rustc and cc-rs both read it.
    # Like the reference, which applies the default only "if we have no
    # minimum OS version", a caller's own MACOSX_DEPLOYMENT_TARGET wins.
    if(DEFINED ENV{MACOSX_DEPLOYMENT_TARGET})
        set(_rustty_macos_min "$ENV{MACOSX_DEPLOYMENT_TARGET}")
    else()
        set(_rustty_macos_min "13.0")
    endif()
    set(_RUSTTY_CARGO_ENV "${CMAKE_COMMAND}" -E env "MACOSX_DEPLOYMENT_TARGET=${_rustty_macos_min}")
    set(_RUSTTY_SONAME_LINK_CMD "")
elseif(WIN32)
    set(_RUSTTY_CARGO_ENV "")
    set(_RUSTTY_SONAME_LINK_CMD "")
else()
    set(_RUSTTY_CARGO_ENV "${CMAKE_COMMAND}" -E env "RUSTTY_VT_SONAME=${RUSTTY_VT_SONAME}")
    # The cdylib now records RUSTTY_VT_SONAME, so a build-tree consumer needs
    # that name next to it at run time.
    set(_RUSTTY_SONAME_LINK_CMD COMMAND "${CMAKE_COMMAND}" -E create_symlink
        "${RUSTTY_VT_REALNAME}" "${RUSTTY_OUT_DIR}/${RUSTTY_VT_SONAME}")
endif()
add_custom_target(cargo_build_lib_vt ALL
    COMMAND ${_RUSTTY_CARGO_ENV} "${CARGO_EXECUTABLE}" build -p rustty-vt-c ${_RUSTTY_PROFILE_FLAG} ${_RUSTTY_FEATURE_FLAGS} ${RUSTTY_CARGO_BUILD_FLAGS}
    ${_RUSTTY_SONAME_LINK_CMD}
    BYPRODUCTS ${_RUSTTY_CARGO_OUTPUTS}
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    COMMENT "Building librustty-vt via cargo build..."
    USES_TERMINAL
)

# --- IMPORTED library targets ------------------------------------------------

# Shared
add_library(rustty-vt SHARED IMPORTED GLOBAL)
set_target_properties(rustty-vt PROPERTIES
    IMPORTED_LOCATION "${RUSTTY_VT_SHARED_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${RUSTTY_INCLUDE_DIR}"
)
if(APPLE)
    set_target_properties(rustty-vt PROPERTIES
        IMPORTED_SONAME "@rpath/${RUSTTY_VT_SONAME}"
    )
elseif(WIN32)
    set_target_properties(rustty-vt PROPERTIES
        IMPORTED_IMPLIB "${RUSTTY_OUT_DIR}/${RUSTTY_VT_IMPLIB}"
    )
else()
    set_target_properties(rustty-vt PROPERTIES
        IMPORTED_SONAME "${RUSTTY_VT_SONAME}"
    )
endif()
add_dependencies(rustty-vt cargo_build_lib_vt)

# Static
#
# The static library is a self-contained archive of the rustty-vt-c crate and
# its Rust dependencies. Consumers must link the system libraries that Rust's
# std relies on; the exact set is platform-specific and is expressed via
# INTERFACE_LINK_LIBRARIES below (populated empirically — see
# dist/cmake/rustty-vt-config.cmake.in for the installed-package equivalent).
#
# Note: with RUSTTY_VT_SIMD ON (the default), the static archive bundles the
# vendored C++ SIMD runtime (highway/simdutf), so the C++ runtime must be
# linked (covered by the lists below). Set RUSTTY_VT_SIMD=OFF for a minimal
# static archive with no C++ SIMD deps — the analog of Ghostty's `-Dsimd=false`.
add_library(rustty-vt-static STATIC IMPORTED GLOBAL)
set_target_properties(rustty-vt-static PROPERTIES
    IMPORTED_LOCATION "${RUSTTY_VT_STATIC_LIBRARY}"
    INTERFACE_INCLUDE_DIRECTORIES "${RUSTTY_INCLUDE_DIR}"
    INTERFACE_COMPILE_DEFINITIONS "RUSTTY_STATIC"
)
if(APPLE)
    # Rust std + the crate's C deps on macOS. The base list is the authoritative
    # `rustc --print native-static-libs` output for rustty-vt-c (order matters).
    #
    # No libintl (S64-F6). The gettext calls rustty-os's i18n object makes
    # (bindtextdomain/dgettext/textdomain) resolve inside the archive: the
    # vendored `gettext-sys` libintl is bundled into it (measured: `nm` shows
    # them defined in librustty_vt_c.a, and a static link of
    # example/c-vt-cmake-static with no -lintl runs). CORRECTION (2026-09-23):
    # this block used to find_library() a Homebrew libintl and link it, on the
    # premise that macOS "needs an external libintl".
    set_target_properties(rustty-vt-static PROPERTIES
        INTERFACE_LINK_LIBRARIES "c++;-framework CoreFoundation;-framework Foundation;iconv;System;objc;c;m"
    )
elseif(WIN32)
    # Rust std on Windows (MSVC) uses these system import libraries.
    set_target_properties(rustty-vt-static PROPERTIES
        INTERFACE_LINK_LIBRARIES "ntdll;kernel32;ws2_32;userenv;bcrypt;advapi32"
    )
else()
    # Rust std on Linux/BSD links libc, pthread, dl, and the unwinder.
    set_target_properties(rustty-vt-static PROPERTIES
        INTERFACE_LINK_LIBRARIES "c;m;pthread;dl;gcc_s"
    )
endif()
add_dependencies(rustty-vt-static cargo_build_lib_vt)

# --- Install -----------------------------------------------------------------

include(GNUInstallDirs)

# Install the shared library under its versioned real name, then point the
# major-version and unversioned names at it (GhosttyLibVt.zig:440-458,
# "Recreate addInstallArtifact's dylib layout"). CORRECTION (S64-F5,
# 2026-09-23): this used to install cargo's flat librustty_vt_c.* with "no
# SONAME->REALNAME symlink dance".
if(WIN32)
    install(FILES "${RUSTTY_VT_SHARED_LIBRARY}" TYPE BIN)
    install(FILES "${RUSTTY_OUT_DIR}/${RUSTTY_VT_IMPLIB}" TYPE LIB RENAME "${RUSTTY_VT_INSTALL_IMPLIB}")
else()
    install(FILES "${RUSTTY_VT_SHARED_LIBRARY}" TYPE LIB RENAME "${RUSTTY_VT_INSTALL_REALNAME}")
    # Resolved when the install script runs, so `cmake --install --prefix`
    # and DESTDIR are honoured.
    set(_rvt_libdir "\$ENV{DESTDIR}\${CMAKE_INSTALL_PREFIX}/${CMAKE_INSTALL_LIBDIR}")
    if(APPLE)
        # "Give framework consumers a stable runtime-relative identity."
        install(CODE "execute_process(COMMAND install_name_tool -id @rpath/${RUSTTY_VT_INSTALL_LINKNAME} \"${_rvt_libdir}/${RUSTTY_VT_INSTALL_REALNAME}\" COMMAND_ERROR_IS_FATAL ANY)")
    endif()
    install(CODE "file(CREATE_LINK \"${RUSTTY_VT_INSTALL_REALNAME}\" \"${_rvt_libdir}/${RUSTTY_VT_INSTALL_SONAME}\" SYMBOLIC)")
    install(CODE "file(CREATE_LINK \"${RUSTTY_VT_INSTALL_SONAME}\" \"${_rvt_libdir}/${RUSTTY_VT_INSTALL_LINKNAME}\" SYMBOLIC)")
    unset(_rvt_libdir)
endif()

# Install static library (`libghostty-vt.a` / `ghostty-vt-static.lib`).
install(FILES "${RUSTTY_VT_STATIC_LIBRARY}" TYPE LIB RENAME "${RUSTTY_VT_STATIC_INSTALL_NAME}")

# Install headers
install(DIRECTORY "${RUSTTY_INCLUDE_DIR}/rustty" DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}")

# --- CMake package config for find_package() ---------------------------------

include(CMakePackageConfigHelpers)

configure_package_config_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/dist/cmake/rustty-vt-config.cmake.in"
    "${CMAKE_CURRENT_BINARY_DIR}/rustty-vt-config.cmake"
    INSTALL_DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/rustty-vt"
)

write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/rustty-vt-config-version.cmake"
    VERSION "${PROJECT_VERSION}"
    COMPATIBILITY SameMajorVersion
)

install(
    FILES
        "${CMAKE_CURRENT_BINARY_DIR}/rustty-vt-config.cmake"
        "${CMAKE_CURRENT_BINARY_DIR}/rustty-vt-config-version.cmake"
    DESTINATION "${CMAKE_INSTALL_LIBDIR}/cmake/rustty-vt"
)

# --- pkg-config --------------------------------------------------------------
#
# PORTS: ghostty src/build/GhosttyLibVt.zig `pkgConfigFiles()` — the shared +
# static `.pc` pair, installed to share/pkgconfig/ (its `libghostty-vt.pc` and
# `libghostty-vt-static.pc`). rustty shipped NEITHER until now, so the whole
# `pkg-config`-consumer story the reference tests in CI had no analog here.
# The reference emits them from build.zig; rustty has no build.zig, and CMake
# is already the thing that knows the install prefix and the per-platform link
# interface, so the pair is generated here.
#
# Consumers link the CARGO artifact stem, not the package name: the crate is
# `rustty-vt-c`, so the file is librustty_vt_c.{a,dylib,so} and the flag is
# `-lrustty_vt_c`. The .pc NAME stays `librustty-vt` to match the reference's
# spelling and the CMake package name.
#
# Libs.private mirrors the INTERFACE_LINK_LIBRARIES set on rustty-vt-static
# above — keep the two in sync when either changes.
#
# DIVERGENCE FROM THE REFERENCE (deliberate, not an oversight): ghostty builds
# its vendored C++ deps in no-libcxx mode and its CI asserts that
# `Libs.private` contains no `-lc++`. rustty's vendored C++ (simdutf, highway)
# is built by the `cc` crate against the platform's default C++ runtime, so
# `-lc++` IS required. Asserting the reference's "no libc++" property would be
# asserting something untrue of this build.
if(APPLE)
    # No libintl, and so no machine-specific absolute path in an installed file
    # (S64-F6): the archive carries its own vendored libintl (see
    # rustty-vt-static above).
    set(RUSTTY_VT_PC_LIBS_PRIVATE "-lc++ -framework CoreFoundation -framework Foundation -liconv -lobjc -lc -lm")
elseif(WIN32)
    set(RUSTTY_VT_PC_LIBS_PRIVATE "-lntdll -lkernel32 -lws2_32 -luserenv -lbcrypt -ladvapi32")
else()
    set(RUSTTY_VT_PC_LIBS_PRIVATE "-lstdc++ -lc -lm -lpthread -ldl -lgcc_s")
endif()

# Every C/C++ dependency is vendored under crates-sys/*/vendor and linked into
# the archive, so nothing is required from the system's pkg-config graph. The
# reference populates this when built with `-fsys=simdutf` / `-fsys=highway`;
# rustty has no system-integration switch yet, so it is empty by construction
# rather than by omission.
set(RUSTTY_VT_PC_REQUIRES_PRIVATE "")

# `-lrustty-vt` resolves to the unversioned install link (S64-F5).
set(RUSTTY_VT_PC_LINK_NAME "rustty-vt")

configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/dist/cmake/librustty-vt.pc.in"
    "${CMAKE_CURRENT_BINARY_DIR}/librustty-vt.pc"
    @ONLY
)
configure_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/dist/cmake/librustty-vt-static.pc.in"
    "${CMAKE_CURRENT_BINARY_DIR}/librustty-vt-static.pc"
    @ONLY
)

install(
    FILES
        "${CMAKE_CURRENT_BINARY_DIR}/librustty-vt.pc"
        "${CMAKE_CURRENT_BINARY_DIR}/librustty-vt-static.pc"
    DESTINATION "${CMAKE_INSTALL_DATAROOTDIR}/pkgconfig"
)

# --- Cross-compilation helper ------------------------------------------------
#
# For downstream projects that need to build librustty-vt for a specific Rust
# target triple. For native builds, use the IMPORTED targets above
# (rustty-vt, rustty-vt-static) directly.
#
# Usage (in a downstream CMakeLists.txt after FetchContent_MakeAvailable):
#
#   rustty_vt_add_target(NAME linux-amd64 RUST_TARGET x86_64-unknown-linux-gnu)
#
# Creates:
#   rustty-vt-static-linux-amd64   (IMPORTED STATIC library)
#   rustty-vt-linux-amd64          (IMPORTED SHARED library)
#
# Optional CARGO_FLAGS to pass additional flags to cargo build:
#
#   rustty_vt_add_target(NAME linux-amd64 RUST_TARGET x86_64-unknown-linux-gnu
#                        CARGO_FLAGS --features simd)
#
# Note: the target's Rust std must be installed (`rustup target add
# <RUST_TARGET>`), and linking the final C executable for a foreign target
# needs a cross C linker — see dist/cmake/RusttyCrossCompiler.cmake.

function(rustty_vt_add_target)
    cmake_parse_arguments(PARSE_ARGV 0 _RVT "" "NAME;RUST_TARGET" "CARGO_FLAGS")

    if(NOT _RVT_NAME)
        message(FATAL_ERROR "rustty_vt_add_target: NAME is required")
    endif()
    if(NOT _RVT_RUST_TARGET)
        message(FATAL_ERROR "rustty_vt_add_target: RUST_TARGET is required")
    endif()

    # CMAKE_CURRENT_FUNCTION_LIST_DIR, i.e. the directory of THIS file, and the
    # only reliable way to reach rustty's own tree from here. A CMake function
    # body executes in the CALLER's variable scope, so directory-scope
    # variables set at the top of this file (RUSTTY_INCLUDE_DIR,
    # RUSTTY_OUT_DIR, ...) are NOT visible when a downstream project calls this
    # function after FetchContent_MakeAvailable: subdirectory variables do not
    # propagate upward. INTERFACE_INCLUDE_DIRECTORIES was being set from
    # ${RUSTTY_INCLUDE_DIR} and therefore resolved to the EMPTY STRING for
    # every downstream caller, so example/c-vt-cmake-cross failed with
    # "fatal error: 'rustty/vt.h' file not found". Derive it from _src_dir.
    set(_src_dir "${CMAKE_CURRENT_FUNCTION_LIST_DIR}")
    set(_prefix "${CMAKE_CURRENT_BINARY_DIR}/rustty-${_RVT_NAME}")

    # Build flags. Cross builds always use --release (a debug cross build can
    # pull in sanitizer/unwind runtimes that are not available for every
    # target). SIMD follows the RUSTTY_VT_SIMD option (default ON, matching
    # upstream); pass extra flags via CARGO_FLAGS.
    set(_flags
        build -p rustty-vt-c --release
        --target ${_RVT_RUST_TARGET}
        --target-dir "${_prefix}"
    )
    if(RUSTTY_VT_SIMD)
        list(APPEND _flags --features simd)
    else()
        # rustty-vt-c's `simd` feature is default-ON (ghostty's non-WASM
        # `-Dsimd` default), so OFF must actively opt out.
        list(APPEND _flags --no-default-features)
    endif()
    if(_RVT_CARGO_FLAGS)
        list(APPEND _flags ${_RVT_CARGO_FLAGS})
    endif()

    # Output paths: cargo nests cross output under
    # <target-dir>/<triple>/release/ (flat).
    set(_out "${_prefix}/${_RVT_RUST_TARGET}/release")
    if(_RVT_RUST_TARGET MATCHES "windows")
        set(_static_lib "${_out}/rustty_vt_c.lib")
        set(_shared_lib "${_out}/rustty_vt_c.dll")
        set(_implib "${_out}/rustty_vt_c.dll.lib")
    elseif(_RVT_RUST_TARGET MATCHES "darwin|apple")
        set(_static_lib "${_out}/librustty_vt_c.a")
        set(_shared_lib "${_out}/librustty_vt_c.dylib")
    else()
        set(_static_lib "${_out}/librustty_vt_c.a")
        set(_shared_lib "${_out}/librustty_vt_c.so")
    endif()

    # Hand cargo the SAME cross C/C++ compiler CMake itself is configured with.
    #
    # Without this the cross build is broken on any host that lacks a GNU cross
    # toolchain, which is the whole situation RusttyCrossCompiler.cmake exists
    # to avoid: it points CMAKE_C_COMPILER at a `zig cc -target <triple>`
    # wrapper, but that only covered the DOWNSTREAM project's own C sources.
    # The cargo invocation below builds ~16 crates-sys/* packages through
    # cc-rs, which knows nothing about CMake's compiler choice and falls back
    # to the conventional `<arch>-linux-gnu-gcc`. On macOS with zig installed
    # and no GNU cross toolchain that is:
    #
    #   error occurred in cc-rs: failed to find tool "aarch64-linux-gnu-gcc"
    #
    # for rustty-os, zlib-sys, and every other -sys crate — i.e.
    # example/c-vt-cmake-cross could not build at all.
    #
    # cc-rs reads CC_<triple with - replaced by _>; cargo reads
    # CARGO_TARGET_<TRIPLE UPPERCASED AND UNDERSCORED>_LINKER.
    set(_cargo_cmd "${CARGO_EXECUTABLE}" ${_flags})
    if(CMAKE_CROSSCOMPILING AND CMAKE_C_COMPILER)
        string(REPLACE "-" "_" _t_us "${_RVT_RUST_TARGET}")
        string(TOUPPER "${_t_us}" _t_up)
        set(_env
            "CC_${_t_us}=${CMAKE_C_COMPILER}"
            "CXX_${_t_us}=${CMAKE_CXX_COMPILER}"
            "CARGO_TARGET_${_t_up}_LINKER=${CMAKE_C_COMPILER}")
        # AR/RANLIB too, when rustty_cross_compiler() supplied them: cc-rs
        # otherwise archives cross objects with the HOST ar/ranlib, which
        # produces an archive whose members are silently dropped from the final
        # staticlib. See RusttyCrossCompiler.cmake for the full symptom.
        if(RUSTTY_CROSS_AR)
            list(APPEND _env "AR_${_t_us}=${RUSTTY_CROSS_AR}")
        endif()
        if(RUSTTY_CROSS_RANLIB)
            list(APPEND _env "RANLIB_${_t_us}=${RUSTTY_CROSS_RANLIB}")
        endif()
        set(_cargo_cmd "${CMAKE_COMMAND}" -E env ${_env} ${_cargo_cmd})
        unset(_env)
        unset(_t_us)
        unset(_t_up)
    endif()

    # An always-running target, for the same reason as cargo_build_lib_vt
    # above: an OUTPUT-keyed command would skip cargo whenever the artifact
    # already existed, however it was produced.
    set(_build_target "cargo_build_lib_vt_${_RVT_NAME}")
    add_custom_target(${_build_target} ALL
        COMMAND ${_cargo_cmd}
        BYPRODUCTS "${_static_lib}" "${_shared_lib}"
        WORKING_DIRECTORY "${_src_dir}"
        COMMENT "Building librustty-vt for ${_RVT_RUST_TARGET}..."
        USES_TERMINAL
    )

    # Static target
    set(_static_target "rustty-vt-static-${_RVT_NAME}")
    add_library(${_static_target} STATIC IMPORTED GLOBAL)
    set_target_properties(${_static_target} PROPERTIES
        IMPORTED_LOCATION "${_static_lib}"
        INTERFACE_INCLUDE_DIRECTORIES "${_src_dir}/include"
        INTERFACE_COMPILE_DEFINITIONS "RUSTTY_STATIC"
    )
    # The Rust-std / vendored-C link interface, per target family. Only the
    # `windows` arm existed; a Linux or Darwin cross target got an EMPTY
    # interface, so the final link failed on the whole _Unwind_* family
    # (library/std/src/sys/personality/gcc.rs) among others. These mirror the
    # native branches near the top of this file.
    if(_RVT_RUST_TARGET MATCHES "windows")
        set_target_properties(${_static_target} PROPERTIES
            INTERFACE_LINK_LIBRARIES "ntdll;kernel32;ws2_32;userenv;bcrypt;advapi32"
        )
    elseif(_RVT_RUST_TARGET MATCHES "darwin|apple")
        # No `intl`: the archive bundles its own vendored libintl (S64-F6).
        set_target_properties(${_static_target} PROPERTIES
            INTERFACE_LINK_LIBRARIES "c++;-framework CoreFoundation;-framework Foundation;iconv;objc;c;m"
        )
    else()
        set_target_properties(${_static_target} PROPERTIES
            INTERFACE_LINK_LIBRARIES "c;m;pthread;dl;gcc_s"
        )
    endif()
    add_dependencies(${_static_target} ${_build_target})

    # Shared target
    set(_shared_target "rustty-vt-${_RVT_NAME}")
    add_library(${_shared_target} SHARED IMPORTED GLOBAL)
    set_target_properties(${_shared_target} PROPERTIES
        IMPORTED_LOCATION "${_shared_lib}"
        INTERFACE_INCLUDE_DIRECTORIES "${_src_dir}/include"
    )
    if(_RVT_RUST_TARGET MATCHES "windows")
        set_target_properties(${_shared_target} PROPERTIES
            IMPORTED_IMPLIB "${_implib}"
        )
    elseif(_RVT_RUST_TARGET MATCHES "darwin|apple")
        set_target_properties(${_shared_target} PROPERTIES
            IMPORTED_SONAME "@rpath/librustty_vt_c.dylib"
        )
    else()
        set_target_properties(${_shared_target} PROPERTIES
            IMPORTED_SONAME "librustty_vt_c.so"
        )
    endif()
    add_dependencies(${_shared_target} ${_build_target})
endfunction()
