//! PORTS: ghostty src/Surface.zig — the window-size arithmetic of `recomputeInitialSize`
//! (`Surface.zig:1866-1886`), the `size_limit` action in `init`, and the CSI 8 t
//! `resizeWindow` path: `initial_size_px`, `size_limit_min`, `resize_window_pts` (the R13
//! checked helpers, UPSTREAM-DEFECTS.md UD59).

use super::*;

/// `recomputeInitialSize`'s pixel computation (`Surface.zig:1866-1886`), checked.
///
/// PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD59). The reference
/// multiplies `@max(window_height, 4) * cell.height` (and the width) in `u32`,
/// converts `@ceil(px / scale)` with `@intFromFloat`, and adds the padding, all
/// unchecked. `window-height = 1000000000` overflows the first product: ReleaseSafe
/// panics, ReleaseFast sends a wrapped size. Returns `None` when any step
/// overflows or the scaled size is not a `u32`; the caller then takes its
/// existing skip path.
pub(crate) fn initial_size_px(
    window_width: u32,
    window_height: u32,
    size: rendererpkg::size::Size,
    scale: apprt::ContentScale,
) -> Option<(u32, u32)> {
    let height = window_height
        .max(MIN_WINDOW_HEIGHT_CELLS)
        .checked_mul(size.cell.height)?;
    let width = window_width
        .max(MIN_WINDOW_WIDTH_CELLS)
        .checked_mul(size.cell.width)?;
    let width_f32: f32 = width as f32;
    let height_f32: f32 = height as f32;
    let final_width = u32_from_ceil(width_f32 / scale.x)?
        .checked_add(size.padding.left)?
        .checked_add(size.padding.right)?;
    let final_height = u32_from_ceil(height_f32 / scale.y)?
        .checked_add(size.padding.top)?
        .checked_add(size.padding.bottom)?;
    Some((final_width, final_height))
}

/// `@as(u32, @intFromFloat(@ceil(v)))`, which traps on an infinite value or
/// one outside `u32` in safety-checked builds; `None` there instead (R13,
/// UD59).
///
/// PORT NOTE (R13, UD59; decided by task R13-C): a NaN does NOT trap there.
/// With a NaN content scale the reference's `recomputeInitialSize` computes a
/// size of 0 and sends 0 plus the padding (measured, Zig 0.16.0: `8` for an
/// 800 px width and 4 px padding a side, Debug, ReleaseSafe and ReleaseFast,
/// arm64 and x86_64 under Rosetta 2). The port returns `None` and so takes the
/// function's own skip path. Why not match the 0: the language makes a
/// conversion with no `u32` result illegal behaviour (`@intFromFloat` docs),
/// the safety check merely lets NaN through, and the 0 is what LLVM's
/// `fptoui` of a NaN (a poison value) happens to lower to on these two
/// targets; it is not a size anything asked for. A content scale that cannot
/// size the window is the same case as a size that does not fit, and UD59's
/// handling for that is the skip. Reaching it takes a NaN content scale from
/// the embedder (`rustty_surface_set_content_scale`, or `scale_factor` in the
/// surface config).
pub(super) fn u32_from_ceil(v: f32) -> Option<u32> {
    let c = v.ceil();
    // NaN is outside every range, so `contains` rejects it too.
    if !(0.0..4_294_967_296.0).contains(&c) {
        return None;
    }
    Some(c as u32)
}

/// The `size_limit` minimums (`Surface.zig:705-706`), checked; `None` when a
/// product overflows `u32` (R13, UPSTREAM-DEFECTS.md UD59).
pub(crate) fn size_limit_min(cell_width: u32, cell_height: u32) -> Option<(u32, u32)> {
    Some((
        cell_width.checked_mul(MIN_WINDOW_WIDTH_CELLS)?,
        cell_height.checked_mul(MIN_WINDOW_HEIGHT_CELLS)?,
    ))
}

/// `resizeWindow`'s point-size computation (714fe9b90), checked: the
/// `(width, height)` in points to send with the `resize_window` action, or
/// `None` when a requested dimension does not fit a `u32`.
///
/// PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD371). The reference
/// computes `@max(size.columns, 40) * cell.width + padding.left +
/// padding.right` (and the height) in unchecked `u32`, then converts
/// `@ceil(px / scale)` with `@intFromFloat`. A cell wider than 65537 px
/// (`adjust-cell-width = 65528` on a 10 px cell) overflows the product for
/// `CSI 8;65535;65535 t`, and one wider than 107374182 px overflows it for any
/// CSI 8 t: ReleaseSafe panics "integer overflow" (or "integer part of floating
/// point value out of bounds" for the conversion), ReleaseFast sends a wrapped
/// size. The port checks each step; `None` makes `resizeWindow` skip the
/// action, as UD59's `initial_size_px` does. A zero (omitted) dimension is not
/// computed at all: the reference computes and then discards it, so a
/// dimension the program did not ask for can no longer fail the request.
pub(crate) fn resize_window_pts(
    size: terminal::stream::ResizeWindow,
    cell: rendererpkg::size::CellSize,
    padding: rendererpkg::size::Padding,
    scale: apprt::ContentScale,
) -> Option<(u32, u32)> {
    // const width: f32 = @floatFromInt(@max(size.columns, min_resize_width_cells) *
    //     self.size.cell.width + padding.left + padding.right);
    // const height: f32 = @floatFromInt(@max(size.rows, min_resize_height_cells) *
    //     self.size.cell.height + padding.top + padding.bottom);
    //
    // A zero dimension is passed through so the apprt keeps it as is.
    // .width = if (size.columns > 0) @intFromFloat(@ceil(width / scale.x)) else 0,
    // .height = if (size.rows > 0) @intFromFloat(@ceil(height / scale.y)) else 0,
    let width = if size.columns > 0 {
        let px = u32::from(size.columns)
            .max(MIN_RESIZE_WIDTH_CELLS)
            .checked_mul(cell.width)?
            .checked_add(padding.left)?
            .checked_add(padding.right)?;
        u32_from_ceil(px as f32 / scale.x)?
    } else {
        0
    };
    let height = if size.rows > 0 {
        let px = u32::from(size.rows)
            .max(MIN_RESIZE_HEIGHT_CELLS)
            .checked_mul(cell.height)?
            .checked_add(padding.top)?
            .checked_add(padding.bottom)?;
        u32_from_ceil(px as f32 / scale.y)?
    } else {
        0
    };
    Some((width, height))
}
