cmake_minimum_required(VERSION 3.19)

# --- Determine cross-compilation target before project() --------------------
#
# We need to know the target before project() so we can set up a cross C/C++
# compiler for it.

# Pick a cross-compilation target: build for a different OS than the host.
# Can be overridden with -DRUST_TARGET=... on the command line.
if(NOT RUST_TARGET)
    # CMAKE_HOST_SYSTEM_PROCESSOR may not be set before project(), so
    # fall back to `uname -m`.
    if(CMAKE_HOST_SYSTEM_PROCESSOR)
        set(_arch "${CMAKE_HOST_SYSTEM_PROCESSOR}")
    else()
        execute_process(COMMAND uname -m OUTPUT_VARIABLE _arch OUTPUT_STRIP_TRAILING_WHITESPACE)
    endif()
    if(_arch MATCHES "^(x86_64|AMD64)$")
        set(_arch "x86_64")
    elseif(_arch MATCHES "^(aarch64|arm64|ARM64)$")
        set(_arch "aarch64")
    endif()

    if(CMAKE_HOST_SYSTEM_NAME STREQUAL "Linux")
        set(RUST_TARGET "${_arch}-pc-windows-gnu")
    elseif(CMAKE_HOST_SYSTEM_NAME STREQUAL "Windows")
        set(RUST_TARGET "${_arch}-unknown-linux-gnu")
    elseif(CMAKE_HOST_SYSTEM_NAME STREQUAL "Darwin")
        set(RUST_TARGET "${_arch}-unknown-linux-gnu")
    else()
        message(FATAL_ERROR
            "Cannot derive RUST_TARGET for ${CMAKE_HOST_SYSTEM_NAME}. "
            "Pass -DRUST_TARGET=... manually.")
    endif()

    message(STATUS "Cross-compiling for RUST_TARGET: ${RUST_TARGET}")
endif()

# --- Set up a cross C/C++ compiler ------------------------------------------

# RusttyCrossCompiler.cmake must be included before project().
# Downstream projects would copy this file into their tree; here we
# include it directly from the repo.
include(../../dist/cmake/RusttyCrossCompiler.cmake)
rustty_cross_compiler(RUST_TARGET "${RUST_TARGET}")

project(c-vt-cmake-cross LANGUAGES C CXX)

include(FetchContent)
FetchContent_Declare(rustty
    GIT_REPOSITORY https://github.com/AirelyDev/rustty.git
    GIT_TAG main
)
FetchContent_MakeAvailable(rustty)

# SIMD is ON by default (RUSTTY_VT_SIMD), mirroring upstream Ghostty's cross
# build. The C++ SIMD runtime is linked via the zig cc/c++ cross compiler
# configured above (hence project(... C CXX)).
rustty_vt_add_target(NAME cross RUST_TARGET "${RUST_TARGET}")

add_executable(c_vt_cmake_cross src/main.c)
target_link_libraries(c_vt_cmake_cross PRIVATE rustty-vt-static-cross)
