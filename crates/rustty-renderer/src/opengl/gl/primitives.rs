//! PORTS: ghostty `pkg/opengl/primitives.zig` (18 loc), in full.

/// PORTS: `primitives.zig:3-18`.
///
/// ```zig
/// pub const Primitive = enum(c_int) {
///     point = c.GL_POINTS,
///     line = c.GL_LINES,
///     line_strip = c.GL_LINE_STRIP,
///     triangle = c.GL_TRIANGLES,
///     triangle_strip = c.GL_TRIANGLE_STRIP,
///
///     // Commented out primitive types are excluded for parity with Metal.
///     //
///     // line_loop = c.GL_LINE_LOOP,
///     // line_adjacency = c.GL_LINES_ADJACENCY,
///     // line_strip_adjacency = c.GL_LINE_STRIP_ADJACENCY,
///     // triangle_fan = c.GL_TRIANGLE_FAN,
///     // triangle_adjacency = c.GL_TRIANGLES_ADJACENCY,
///     // triangle_strip_adjacency = c.GL_TRIANGLE_STRIP_ADJACENCY,
/// };
/// ```
///
/// The commented-out members above are carried over as comments, exactly as the
/// reference carries them: they record that the omission is a deliberate Metal-
/// parity decision (`MTLPrimitiveType` has no fan/adjacency counterparts), not an
/// oversight to be "completed" by a later port.
///
/// PORT NOTE (`enum(c_int)` -> `#[repr(i32)]`): `c_int` is `i32` on every target
/// this crate builds for. Unlike `Buffer`'s `Target`/`Usage`, this enum has NO
/// trailing `_`, i.e. it is CLOSED — so a real Rust enum (not the open-enum
/// newtype those two use) is the exact model.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(i32)]
pub enum Primitive {
    Point = gl::POINTS as i32,
    Line = gl::LINES as i32,
    LineStrip = gl::LINE_STRIP as i32,
    Triangle = gl::TRIANGLES as i32,
    TriangleStrip = gl::TRIANGLE_STRIP as i32,
    // line_loop = c.GL_LINE_LOOP,
    // line_adjacency = c.GL_LINES_ADJACENCY,
    // line_strip_adjacency = c.GL_LINE_STRIP_ADJACENCY,
    // triangle_fan = c.GL_TRIANGLE_FAN,
    // triangle_adjacency = c.GL_TRIANGLES_ADJACENCY,
    // triangle_strip_adjacency = c.GL_TRIANGLE_STRIP_ADJACENCY,
}

#[cfg(test)]
mod tests {
    use super::Primitive;

    /// RUSTTY-ONLY. The reference gets `enum(c_int) { point = c.GL_POINTS, … }`
    /// checked by the Zig compiler: the tag values ARE the `c.GL_*` constants, by
    /// construction. The Rust port re-states them through an `as i32` cast, which
    /// is a place a typo could hide, so the identity is asserted instead of
    /// assumed. No GL context needed — these are compile-time constants.
    #[test]
    fn primitive_discriminants_are_the_gl_constants() {
        assert_eq!(Primitive::Point as u32, gl::POINTS);
        assert_eq!(Primitive::Line as u32, gl::LINES);
        assert_eq!(Primitive::LineStrip as u32, gl::LINE_STRIP);
        assert_eq!(Primitive::Triangle as u32, gl::TRIANGLES);
        assert_eq!(Primitive::TriangleStrip as u32, gl::TRIANGLE_STRIP);
    }
}
