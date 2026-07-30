//! Renderer implementation for WebGL in the browser.
//!
//! PORTS: ghostty `src/renderer/WebGL.zig` (2 lines, 0 tests).
//!
//! PORT STATUS: ported — COMPLETE. The reference file is a two-line
//! PLACEHOLDER: a doc comment and `pub const WebGL = @This();` (the
//! file-as-struct idiom, i.e. an empty struct type). It has no fields, no
//! methods, no shaders, no draw path, and no `test {}`. It is named in exactly
//! two places, both in `src/renderer.zig` (`:20` the re-export, `:41` the
//! `.webgl` arm of the comptime `Renderer` switch). Verified identical on the
//! oracle's `origin/main`, so this is not an artifact of the pin.
//!
//! Consequently there is NOTHING deeper to port here, and inventing a WebGL
//! draw path would be an R1 violation (no reference to be faithful to). The
//! browser-side drawing that rustty DOES ship lives where the reference's own
//! wasm model puts it — in the consumer: `example/wasm-webgl/` renders the
//! `Atlas.Wasm` texture through WebGL2 against the wasm module's C-ABI. See
//! `docs/superpowers/plans/2026-07-23-ws14-wasm-webgl.md`.

// PORT: `pub const WebGL = @This();` — the file's own type. An empty Zig struct
// maps to a Rust unit struct (both zero-sized).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WebGL;

#[cfg(test)]
mod tests {
    use super::WebGL;

    /// O16: the reference `renderer.zig` switch has THREE arms; the port must name a
    /// concrete type for `.webgl` too. Oracle: `src/renderer/WebGL.zig` (2 lines) —
    /// a placeholder unit type with no methods, identical on the oracle's origin/main.
    #[test]
    fn webgl_arm_exists_and_is_a_unit_placeholder() {
        let _: WebGL = WebGL;
        assert_eq!(std::mem::size_of::<WebGL>(), 0);
    }
}
