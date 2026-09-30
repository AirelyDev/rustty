#include "imgui.h"

// This file contains custom extensions for functionality that isn't
// properly supported by Dear Bindings yet. Namely:
// https://github.com/dearimgui/dear_bindings/issues/55

// RUSTTY-ONLY (N464): the C headers dcimgui.h includes, included here at
// global scope first, so its includes inside the namespace below are no-ops.
// Otherwise the first <stdint.h> of this TU lands inside `cimgui`, its include
// guard is then set, and a later global user finds no `::uint64_t`. That user
// is clang 23's <intrin.h>, which includes <arm_acle.h> on aarch64 (via
// <windows.h> from the OpenGL3 loader below). The reference's pinned Zig 0.16
// clang (21) has no such include; Rustty compiles with the host's clang-cl.
#include <stdint.h>
#include <stdbool.h>
#include <stdarg.h>
#include <stddef.h>
#include <assert.h>

// Wrap this in a namespace to keep it separate from the C++ API
namespace cimgui
{
#include "dcimgui.h"
}

extern "C"
{
CIMGUI_API void ImFontConfig_ImFontConfig(cimgui::ImFontConfig* self)
{
    static_assert(sizeof(cimgui::ImFontConfig) == sizeof(::ImFontConfig), "ImFontConfig size mismatch");
    static_assert(alignof(cimgui::ImFontConfig) == alignof(::ImFontConfig), "ImFontConfig alignment mismatch");
    ::ImFontConfig defaults;
    *reinterpret_cast<::ImFontConfig*>(self) = defaults;
}

CIMGUI_API void ImGuiStyle_ImGuiStyle(cimgui::ImGuiStyle* self)
{
    static_assert(sizeof(cimgui::ImGuiStyle) == sizeof(::ImGuiStyle), "ImGuiStyle size mismatch");
    static_assert(alignof(cimgui::ImGuiStyle) == alignof(::ImGuiStyle), "ImGuiStyle alignment mismatch");
    ::ImGuiStyle defaults;
    *reinterpret_cast<::ImGuiStyle*>(self) = defaults;
}

// RUSTTY-ONLY (N485): `ImGui_TextColored` with the colour passed by pointer.
// On aarch64-pc-windows-msvc every argument of a variadic function follows the
// variadic convention, so clang-cl passes `ImGui_TextColored`'s `ImVec4` in
// x0:x1 and `fmt` in x2, while rustc passes the HFA as four floats in w0..w3
// and `fmt` in x4. A pointer travels the same way under both. The body is
// Dear Bindings' `ImGui_TextColored` (dcimgui.cpp) with `*col` for `col`.
CIMGUI_API void ImGui_TextColoredPtr(const cimgui::ImVec4* col, const char* fmt, ...)
{
    va_list args;
    va_start(args, fmt);
    ::ImGui::TextColoredV(::ImVec4(col->x, col->y, col->z, col->w), fmt, args);
    va_end(args);
}

// Perform the OpenGL3 backend shutdown and then zero out the imgl3w
// function pointer table. ImGui_ImplOpenGL3_Shutdown() calls
// imgl3wShutdown() which dlcloses the GL library handles but does not
// zero out the function pointers. A subsequent ImGui_ImplOpenGL3_Init()
// sees the stale (non-null) pointers, skips loader re-initialization,
// and crashes when calling through them. Zeroing the table forces the
// next Init to reload the GL function pointers via imgl3wInit().
#ifndef IMGUI_DISABLE
#if __has_include("backends/imgui_impl_opengl3.h")
#ifdef ZIGPKG_IMGUI_ENABLE_OPENGL3
#include "backends/imgui_impl_opengl3.h"
#include "backends/imgui_impl_opengl3_loader.h"

CIMGUI_API void ImGui_ImplOpenGL3_ShutdownWithLoaderCleanup()
{
    ::ImGui_ImplOpenGL3_Shutdown();
    memset(&imgl3wProcs, 0, sizeof(imgl3wProcs));
}
#endif // ZIGPKG_IMGUI_ENABLE_OPENGL3
#endif // __has_include("backends/imgui_impl_opengl3.h")
#endif // IMGUI_DISABLE

// Metal rendering backend header (macOS). Mirrors the OpenGL3 block above:
// gated on the header being vendored (__has_include) and the backend being
// enabled (ZIGPKG_IMGUI_ENABLE_METAL, which build.rs defines on macOS,
// mirroring the reference's `backend-metal = isDarwin` in
// src/build/SharedDeps.zig). The reference compiles imgui_impl_metal.mm when
// backend-metal is set (pkg/dcimgui/build.zig:133-144). The ObjC-API
// declarations inside imgui_impl_metal.h are themselves guarded by __OBJC__, so
// in this plain-C++ TU the include resolves to nothing beyond imgui.h; the
// C-ABI shim surface the port actually calls is added as an ObjC++ TU (see
// build.rs' separate Metal cc::Build).
#ifndef IMGUI_DISABLE
#if __has_include("backends/imgui_impl_metal.h")
#ifdef ZIGPKG_IMGUI_ENABLE_METAL
#include "backends/imgui_impl_metal.h"
#endif // ZIGPKG_IMGUI_ENABLE_METAL
#endif // __has_include("backends/imgui_impl_metal.h")
#endif // IMGUI_DISABLE

}
