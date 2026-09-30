// P7-W6 T1 M6 probe (UD371): does `Surface.resizeWindow`'s (714fe9b90,
// src/Surface.zig at 12752b2ac) size arithmetic trap for a reachable input?
//
// Standalone: the expressions below are copied verbatim from the oracle
// (`resizeWindow` and `Metrics.Modifier.apply`'s `.absolute` arm, which is how
// `adjust-cell-width` reaches `self.size.cell.width`). Run each mode with
//   zig run -O Debug        p7w6t1_resize_window_overflow_probe.zig -- <case>
//   zig run -O ReleaseSafe  p7w6t1_resize_window_overflow_probe.zig -- <case>
//   zig run -O ReleaseFast  p7w6t1_resize_window_overflow_probe.zig -- <case>
// Cases: ok | mul | pad | float | min40 | padsum
const std = @import("std");

const min_resize_width_cells: u32 = 40;
const min_resize_height_cells: u32 = 10;

// stream.Action.ResizeWindow
const ResizeWindow = extern struct { rows: u16, columns: u16 };

// Metrics.Modifier.apply, `.absolute` arm, for a u32 metric.
fn applyAbsolute(v: u32, abs: i32) u32 {
    const T = u32;
    const v_i64: i64 = @intCast(v);
    const abs_i64: i64 = @intCast(abs);
    const applied_i64: i64 = v_i64 +| abs_i64;
    const clamped_i64: i64 = @max(0, applied_i64);
    const applied_T: T = std.math.cast(T, clamped_i64) orelse
        std.math.maxInt(T) * @as(T, @intCast(std.math.sign(clamped_i64)));
    return applied_T;
}

const Padding = struct { top: u32, bottom: u32, left: u32, right: u32 };

fn resizeWindow(size: ResizeWindow, cell_width: u32, cell_height: u32, padding: Padding, scale_x: f32, scale_y: f32) struct { u32, u32 } {
    // Verbatim from Surface.resizeWindow.
    const width: f32 = @floatFromInt(@max(size.columns, min_resize_width_cells) *
        cell_width + padding.left + padding.right);
    const height: f32 = @floatFromInt(@max(size.rows, min_resize_height_cells) *
        cell_height + padding.top + padding.bottom);
    return .{
        if (size.columns > 0) @intFromFloat(@ceil(width / scale_x)) else 0,
        if (size.rows > 0) @intFromFloat(@ceil(height / scale_y)) else 0,
    };
}

pub fn main(init: std.process.Init) !void {
    var it = try init.minimal.args.iterateAllocator(init.gpa);
    defer it.deinit();
    _ = it.next();
    const case = it.next() orelse "ok";

    // A 10x20 px face cell with `window-padding-x/y = 2`.
    const pad: Padding = .{ .top = 2, .bottom = 2, .left = 2, .right = 2 };
    const r: struct { u32, u32 } = if (std.mem.eql(u8, case, "ok"))
        // Positive control: CSI 8;24;80 t with no adjust.
        resizeWindow(.{ .rows = 24, .columns = 80 }, 10, 20, pad, 1, 1)
    else if (std.mem.eql(u8, case, "mul"))
        // `adjust-cell-width = 65528` (cell 65538 px), CSI 8;65535;65535 t:
        // 65535 * 65538 > maxInt(u32).
        resizeWindow(.{ .rows = 65535, .columns = 65535 }, applyAbsolute(10, 65528), 20, pad, 1, 1)
    else if (std.mem.eql(u8, case, "pad"))
        // `adjust-cell-width = 65527` (cell 65537 px): the product is exactly
        // maxInt(u32) and the padding sum overflows.
        resizeWindow(.{ .rows = 65535, .columns = 65535 }, applyAbsolute(10, 65527), 20, pad, 1, 1)
    else if (std.mem.eql(u8, case, "float"))
        // No padding, product 65535 * 65536 = 4294901760 fits u32, but a
        // content scale of 0.5 makes the point size 8589803520 > maxInt(u32).
        resizeWindow(.{ .rows = 24, .columns = 65535 }, applyAbsolute(10, 65526), 20, .{ .top = 0, .bottom = 0, .left = 0, .right = 0 }, 0.5, 1)
    else if (std.mem.eql(u8, case, "padsum"))
        // Fix round 1: `window-padding-x = 2147483648` at 72 dpi (scale 1):
        // `scaledPadding` gives 2^31 a side, which fits; the sum does not.
        // Ordinary 10x20 px cells, CSI 8;24;80 t.
        resizeWindow(.{ .rows = 24, .columns = 80 }, 10, 20, .{ .top = 0, .bottom = 0, .left = 1 << 31, .right = 1 << 31 }, 1, 1)
    else if (std.mem.eql(u8, case, "min40"))
        // `adjust-cell-width = 2147483647` (cell 2147483657 px), a bare
        // CSI 8 t (both zero): the width is discarded but still computed,
        // 40 * cell overflows.
        resizeWindow(.{ .rows = 0, .columns = 0 }, applyAbsolute(10, 2147483647), 20, pad, 1, 1)
    else
        return error.UnknownCase;
    std.debug.print("PROBE {s}: width={d} height={d}\n", .{ case, r[0], r[1] });
}
