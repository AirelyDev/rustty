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
//! - `Health` — the crate-root `renderer.zig` `Health` enum (1 test): a
//!   2-variant `enum(c_int)` with zero GPU dependency, hoisted to the crate root
//!   (like the render-types A1 `size` hoist) so `rustty-apprt-api`'s
//!   `Action::renderer_health` can name it without a GPU-renderer edge.
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

/// The health status of a renderer. These must be shared across all
/// renderers even if some states aren't reachable so that our API users
/// can use the same enum for all renderers.
//
// PORT NOTE: reference `src/renderer.zig` crate-root `pub const Health =
// enum(c_int) { healthy, unhealthy }`. Hoisted into the rustty-renderer crate
// root here (mirroring the render-types A1 `size` hoist precedent) so that
// `rustty-apprt-api`'s `Action::renderer_health` variant can name it WITHOUT
// pulling in the GPU renderer. `enum(c_int)` -> `#[repr(i32)]` (C `int`) so the
// discriminants match the C ABI ordering used by the `checkGhosttyHEnum` test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Health {
    Healthy,
    Unhealthy,
}

#[cfg(test)]
mod health_tests {
    use super::Health;
    use rustty_lib::r#enum::{check_ghostty_henum, enum_type, EnumType};
    use rustty_lib::target::Target;

    /// PORT NOTE: models the reference `enum(c_int)` `Health` as an [`EnumType`]
    /// (C target) so the `checkGhosttyHEnum` test can reflect over its keys and
    /// backing int. The keys are the snake_case forms of the variants, in
    /// declaration order: healthy, unhealthy.
    fn health_enum_descriptor() -> EnumType {
        // Touch `Health` so the enum participates like the reference's
        // `refAllDeclsRecursive` reach (`Health::Healthy`/`Health::Unhealthy`).
        let _ = (Health::Healthy, Health::Unhealthy);
        enum_type(Target::C, &[Some("healthy"), Some("unhealthy")])
    }

    // test "ghostty.h Health" {
    //     try lib.checkGhosttyHEnum(Health, "GHOSTTY_RENDERER_HEALTH_");
    // }
    #[test]
    fn ghostty_h_health() {
        // PORT NOTE: `try lib.checkGhosttyHEnum(Health, "GHOSTTY_RENDERER_HEALTH_");`,
        // retargeted to `RUSTTY_RENDERER_HEALTH_` per R2. The
        // `rustty_renderer_health_e` C enum declares every `Health` key with the
        // same sequential `c_int` value as the enum field, so the lookup closure
        // returns each field's own backing int.
        let t = health_enum_descriptor();
        let fields = t.fields.clone();
        check_ghostty_henum(&t, "RUSTTY_RENDERER_HEALTH_", |name| {
            fields.iter().find_map(|(n, v)| {
                let expected = format!("RUSTTY_RENDERER_HEALTH_{}", n.to_ascii_uppercase());
                if expected == name {
                    Some(*v as i64)
                } else {
                    None
                }
            })
        })
        .unwrap();
    }
}
