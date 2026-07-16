# RusttyCrossCompiler.cmake — set up a cross C/C++ compiler for a Rust target
#
# Provides rustty_cross_compiler() which configures a cross C/CXX compiler for
# a given Rust target triple, so the final C example executable can be linked
# for a foreign target. cargo builds librustty-vt itself (see
# rustty_vt_add_target), but linking a C program that consumes it needs a C
# toolchain that targets the same platform.
#
# rustty ships no compiler of its own, so this helper uses `zig cc` as a
# target-agnostic cross compiler/linker (a common approach in the Rust
# ecosystem; `cargo-zigbuild` uses the same trick). It requires `zig` on the
# PATH. If you prefer a platform cross-toolchain, set CMAKE_C_COMPILER
# yourself before project() instead of calling this function.
#
# It creates small wrapper scripts (shell on Unix, .cmd on Windows) and sets
# the following CMake variables in the caller's scope:
#
#   CMAKE_C_COMPILER, CMAKE_CXX_COMPILER,
#   CMAKE_C_COMPILER_FORCED, CMAKE_CXX_COMPILER_FORCED,
#   CMAKE_SYSTEM_NAME, CMAKE_EXECUTABLE_SUFFIX (Windows only)
#
# This file is self-contained with no dependencies on the rustty source tree.
# Copy it into your project and include it directly. It cannot be consumed via
# FetchContent because it must run before project(), but
# FetchContent_MakeAvailable triggers project() internally.
#
# Must be called BEFORE project() — CMake reads the compiler variables at
# project() time and won't re-detect after that.
#
# Usage:
#
#   cmake_minimum_required(VERSION 3.19)
#
#   include(cmake/RusttyCrossCompiler.cmake)
#   rustty_cross_compiler(RUST_TARGET x86_64-unknown-linux-gnu)
#
#   project(myapp LANGUAGES C CXX)
#
#   FetchContent_MakeAvailable(rustty)
#   rustty_vt_add_target(NAME linux-amd64 RUST_TARGET x86_64-unknown-linux-gnu)
#   target_link_libraries(myapp PRIVATE rustty-vt-static-linux-amd64)
#
# See example/c-vt-cmake-cross/ for a complete working example.

include_guard(GLOBAL)

# Map a Rust target triple (e.g. x86_64-unknown-linux-gnu) to the zig target
# triple `zig cc -target` expects (e.g. x86_64-linux-gnu). Sets the result in
# the variable named by out_var in the caller's scope.
function(_rustty_zig_triple rust_target out_var)
    if(rust_target MATCHES "^([a-z0-9_]+)-unknown-linux-(gnu[a-z0-9]*|musl)$")
        set(${out_var} "${CMAKE_MATCH_1}-linux-${CMAKE_MATCH_2}" PARENT_SCOPE)
    elseif(rust_target MATCHES "^([a-z0-9_]+)-pc-windows-(gnu|msvc)$")
        # zig uses the gnu ABI for windows cross builds
        set(${out_var} "${CMAKE_MATCH_1}-windows-gnu" PARENT_SCOPE)
    elseif(rust_target MATCHES "^([a-z0-9_]+)-apple-darwin$")
        set(${out_var} "${CMAKE_MATCH_1}-macos" PARENT_SCOPE)
    else()
        # Fall back to passing the triple through unchanged; zig may accept it.
        set(${out_var} "${rust_target}" PARENT_SCOPE)
    endif()
endfunction()

function(rustty_cross_compiler)
    cmake_parse_arguments(PARSE_ARGV 0 _RCC "" "RUST_TARGET" "")

    if(NOT _RCC_RUST_TARGET)
        message(FATAL_ERROR "rustty_cross_compiler: RUST_TARGET is required")
    endif()

    find_program(_RCC_ZIG zig)
    if(NOT _RCC_ZIG)
        message(FATAL_ERROR
            "rustty_cross_compiler: `zig` not found on PATH. It is used as the "
            "cross C compiler for linking the C example. Install zig, or set "
            "CMAKE_C_COMPILER to a cross toolchain for ${_RCC_RUST_TARGET} "
            "before project().")
    endif()

    _rustty_zig_triple("${_RCC_RUST_TARGET}" _zig_target)

    if(CMAKE_HOST_SYSTEM_NAME STREQUAL "Windows")
        set(_cc "${CMAKE_CURRENT_BINARY_DIR}/zig-cc.cmd")
        set(_cxx "${CMAKE_CURRENT_BINARY_DIR}/zig-cxx.cmd")
        file(WRITE "${_cc}" "@\"${_RCC_ZIG}\" cc -target ${_zig_target} %*\n")
        file(WRITE "${_cxx}" "@\"${_RCC_ZIG}\" c++ -target ${_zig_target} %*\n")
    else()
        set(_cc "${CMAKE_CURRENT_BINARY_DIR}/zig-cc")
        set(_cxx "${CMAKE_CURRENT_BINARY_DIR}/zig-c++")
        file(WRITE "${_cc}" "#!/bin/sh\nexec \"${_RCC_ZIG}\" cc -target ${_zig_target} \"$@\"\n")
        file(WRITE "${_cxx}" "#!/bin/sh\nexec \"${_RCC_ZIG}\" c++ -target ${_zig_target} \"$@\"\n")
        file(CHMOD "${_cc}" "${_cxx}"
            PERMISSIONS OWNER_READ OWNER_WRITE OWNER_EXECUTE)
    endif()

    set(CMAKE_C_COMPILER "${_cc}" PARENT_SCOPE)
    set(CMAKE_CXX_COMPILER "${_cxx}" PARENT_SCOPE)
    set(CMAKE_C_COMPILER_FORCED TRUE PARENT_SCOPE)
    set(CMAKE_CXX_COMPILER_FORCED TRUE PARENT_SCOPE)

    if(_RCC_RUST_TARGET MATCHES "windows")
        set(CMAKE_SYSTEM_NAME Windows PARENT_SCOPE)
        set(CMAKE_EXECUTABLE_SUFFIX ".exe" PARENT_SCOPE)
    elseif(_RCC_RUST_TARGET MATCHES "linux")
        set(CMAKE_SYSTEM_NAME Linux PARENT_SCOPE)
    elseif(_RCC_RUST_TARGET MATCHES "darwin|apple")
        set(CMAKE_SYSTEM_NAME Darwin PARENT_SCOPE)
    endif()
endfunction()
