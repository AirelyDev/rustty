# This is a _special_ directory.

The files in this directory are imported by `../face.rs` and provide the
`draw*` functions used to handle drawing specified codepoints. Each module's
`draw*` functions are wired into the sprite dispatch table in `../face.rs`.

## IMPORTANT

When you add a new file here, you need to declare it with a `pub mod` in
`mod.rs` and register its `draw*` functions in the dispatch table in
`../face.rs` for them to be picked up. Ghostty's Zig implementation scanned
the draw structs at comptime; the Rust port instead lists each entry
explicitly in `../face.rs`, mapping the module's concrete-error `draw*`
function to the uniform `DrawFn` via `DrawFnError::from`.

## `draw*` functions

Any function named `draw<CODEPOINT>` or `draw<MIN>_<MAX>` will be used to
draw the codepoint or range of codepoints specified in the name. These are
hex-encoded values with upper case letters.

`draw*` functions are provided with these arguments:

```rust
/// The codepoint being drawn. For single-codepoint draw functions this can
/// just be discarded, but it's needed for range draw functions to determine
/// which value in the range needs to be drawn.
cp: u32,
/// The canvas on which to draw the codepoint.
///
/// This canvas has been prepared with an extra quarter of the width/height on
/// each edge, and its transform has been set so that [0, 0] is still the upper
/// left of the cell and [width, height] is still the bottom right; in order to
/// draw above or to the left, use negative values, and to draw below or to the
/// right use values greater than the width or the height.
///
/// Because the canvas has been prepared this way, it's possible to draw glyphs
/// that exit the cell bounds by some amount- an example of when this is useful
/// is in drawing box-drawing diagonals, with enough overlap so that they can
/// seamlessly connect across corners of cells.
canvas: &mut crate::sprite::Canvas,
/// The width of the cell to draw for.
width: u32,
/// The height of the cell to draw for.
height: u32,
/// The font grid metrics.
metrics: crate::metrics::Metrics,
```

`draw*` functions may only return `Result<(), DrawFnError>` (or a module-local
concrete error that converts into `DrawFnError`, both defined in `../face.rs`).

## `special.rs`

The functions in `special.rs` are not for drawing unicode codepoints,
rather their names match the enum tag names in the `Sprite` enum from
`src/sprite/mod.rs`. They are called with the same arguments as the
other `draw*` functions.
