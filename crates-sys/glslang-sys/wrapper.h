/* glslang-sys bindgen surface.
 *
 * Mirrors the reference's binding imports (ghostty/pkg/glslang/c.zig) exactly —
 * the same two public glslang C headers the reference's `c.zig` @cImports:
 *
 *     @cInclude("glslang/Include/glslang_c_interface.h");
 *     @cInclude("glslang/Public/resource_limits_c.h");
 *
 * bindgen parses these against the vendored glslang 14.2.0 tree (root include
 * dir `vendor/`, plus `vendor/override` for the CMake-generated
 * `glslang/build_info.h`). This is the full C interface used by the reference's
 * shader.zig / program.zig / init.zig wrappers: process init/finalize, shader
 * create/preprocess/parse/info-log, program link + SPIR-V generate/get, and the
 * `glslang_default_resource()` limits table. */
#include "glslang/Include/glslang_c_interface.h"
#include "glslang/Public/resource_limits_c.h"
