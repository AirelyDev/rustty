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
//!
//! renderer/size.zig (6 tests) is already ported in the `rustty-render-types`
//! leaf (the A1 hoist) and re-exported below.
//!
//! DEFERRED:
//!
//! - renderer/link.zig (3 tests) — blocked on the Oniguruma binding
//!   (`oni.Regex`), which is not yet ported anywhere in the tree (see
//!   rustty-input `link.rs`'s deferred `oni_regex` seam). `Set.fromConfig`
//!   compiles a regex and `renderCellMap` runs `regex.search`, so its code and
//!   all 3 tests require real regex matching that cannot be honestly satisfied
//!   without the binding. Ported when Oniguruma lands (R5: not weakened/stubbed).
//! - The GPU draw path (OpenGL/Metal backends, pipelines, generic renderer,
//!   images, custom shaders) — R-d/Phase 3.

pub mod cell;
pub mod cursor;
pub mod shaders;
pub mod state;

// renderer/size.zig lives in the rustty-render-types leaf (A1 hoist). The
// reference `renderer.zig` re-exports these size types (`GridSize`, `Size`,
// `ScreenSize`, `CellSize`, `Padding`, `Coordinate`, ...); mirror that re-export.
pub use rustty_render_types as size;
