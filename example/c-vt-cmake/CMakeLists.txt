cmake_minimum_required(VERSION 3.19)
project(c-vt-cmake LANGUAGES C)

include(FetchContent)
FetchContent_Declare(rustty
    GIT_REPOSITORY https://github.com/AirelyDev/rustty.git
    GIT_TAG main
)
# librustty-vt builds with SIMD ON by default (RUSTTY_VT_SIMD), matching how
# upstream Ghostty builds the shared library for this example — no flag needed.
FetchContent_MakeAvailable(rustty)

add_executable(c_vt_cmake src/main.c)
target_link_libraries(c_vt_cmake PRIVATE rustty-vt)
