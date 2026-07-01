//! rustty-renderer
//!
//! PORTS: ghostty src/renderer/ minus size.zig (renderer.zig, Metal, OpenGL, WebGL, Options,
//! Overlay, State, Thread, backend, cell, cursor, generic, ...); links
//! glslang/spirv-cross/wuffs/libpng/zlib-sys; apprt via surface trait (breaks #10, #11).
//!
//! PORT STATUS: in progress (R-a headless slice). Ported so far:
//!
//! - `cell` — renderer/cell.zig (5 tests): `Contents`, `constraintWidth`,
//!   symbol/graphics-element classification.
//! - `cursor` — renderer/cursor.zig (4 tests): the renderer `Style` + `style()`.
//! - `state` — renderer/State.zig (2 tests): the render `State`/`Preedit`.
//! - `shaders` — the CPU-side vertex-data structs (`CellText`/`CellBg`/`Uniforms`)
//!   that `cell.zig` fills, from renderer/opengl/shaders.zig.
//! - `shadertoy` — renderer/shadertoy.zig (4 tests): the custom-shader transpile
//!   path GLSL -> (glslang) -> SPIR-V -> (spirv-cross) -> MSL/GLSL, over the
//!   vendored `glslang-sys` + `spirv-cross-sys` FFI.
//! - `link` — renderer/link.zig (3 tests): the `oni.Regex`-backed renderer link
//!   `Set`/`renderCellMap` (Oniguruma deferral now RESOLVED).
//!
//! renderer/size.zig (6 tests) is already ported in the `rustty-render-types`
//! leaf (the A1 hoist) and re-exported below.
//!
//! DEFERRED:
//!
//! - `shadertoy`'s `loadFromFiles`/`loadFromFile` — they take
//!   `configpkg.RepeatablePath`, but `rustty-config` already depends on
//!   `rustty-renderer`, so a `renderer -> config` edge would cycle (G7). The
//!   verbatim bodies are preserved in `shadertoy.rs` (R5) and are NOT exercised
//!   by any test. See .port/phase-2/DEFERRALS.md.
//! - The GPU draw path (OpenGL/Metal backends, pipelines, generic renderer,
//!   images, custom shaders) — R-d/Phase 3.

pub mod cell;
pub mod cursor;
pub mod link;
pub mod shaders;
pub mod shadertoy;
pub mod state;

// renderer/size.zig lives in the rustty-render-types leaf (A1 hoist). The
// reference `renderer.zig` re-exports these size types (`GridSize`, `Size`,
// `ScreenSize`, `CellSize`, `Padding`, `Coordinate`, ...); mirror that re-export.
pub use rustty_render_types as size;
