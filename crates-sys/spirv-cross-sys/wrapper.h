/* spirv-cross-sys bindgen surface.
 *
 * Mirrors the reference's binding import (ghostty/pkg/spirv-cross/c.zig)
 * exactly — the single public SPIRV-Cross C header the reference's `c.zig`
 * @cImports:
 *
 *     @cInclude("spirv_cross_c.h");
 *
 * bindgen parses this against the vendored SPIRV-Cross tree (include dir
 * `vendor/`). This is the full C interface used by the renderer's Metal-shader
 * cross-compilation path: context create/destroy, SPIR-V parse, compiler
 * create + options + compile, and the reflection/error surfaces. The GLSL and
 * MSL backends are enabled (`SPIRV_CROSS_C_API_GLSL`/`_MSL`); HLSL/CPP/REFLECT
 * are OFF (matching the reference), but their public struct/enum declarations
 * still live in this header and are harmlessly emitted by bindgen. */
#include "spirv_cross_c.h"
