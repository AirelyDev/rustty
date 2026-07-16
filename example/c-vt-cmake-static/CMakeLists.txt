cmake_minimum_required(VERSION 3.19)
project(c-vt-cmake-static LANGUAGES C)

include(FetchContent)
FetchContent_Declare(rustty
    GIT_REPOSITORY https://github.com/AirelyDev/rustty.git
    GIT_TAG main
)
# Build without SIMD so the static archive has no vendored C++ SIMD runtime
# (highway/simdutf) to link — the direct analog of upstream Ghostty's
# `-Dsimd=false` for its static example. (librustty-vt otherwise defaults
# SIMD ON via RUSTTY_VT_SIMD.)
set(RUSTTY_VT_SIMD OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(rustty)

add_executable(c_vt_cmake_static src/main.c)
target_link_libraries(c_vt_cmake_static PRIVATE rustty-vt-static)
