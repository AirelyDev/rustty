// P7-W6 T2 M6 probe (UD372): does the GTK half of CSI 8 t (219cca932,
// src/apprt/gtk/class/window.zig at 12752b2ac) trap for a size the core can
// now send?
//
// Standalone: `coreResizeWindow` is `Surface.resizeWindow`'s two expressions
// (714fe9b90) and `resizeDelta` plus the `setDefaultSize` sums are
// `Window.resizeSurface`/`resizeDelta` (219cca932), all copied verbatim from
// the oracle. Only the GTK getters are replaced by parameters. Run each mode:
//   zig run -O Debug        p7w6t2_gtk_resize_delta_probe.zig -- <case>
//   zig run -O ReleaseSafe  p7w6t2_gtk_resize_delta_probe.zig -- <case>
//   zig run -O ReleaseFast  p7w6t2_gtk_resize_delta_probe.zig -- <case>
// Cases: ok | hidpi | cvt | add
const std = @import("std");

const min_resize_width_cells: u32 = 40;
const min_resize_height_cells: u32 = 10;

// stream.Action.ResizeWindow
const ResizeWindow = extern struct { rows: u16, columns: u16 };
// apprt.action.ResizeWindow
const ApprtResizeWindow = extern struct { width: u32, height: u32 };
const Padding = struct { top: u32, bottom: u32, left: u32, right: u32 };

// Verbatim from Surface.resizeWindow (714fe9b90). The u32 arithmetic is
// UD371's; the inputs below keep it in range, so the value reaching the
// apprt is a valid u32 (the port's checked `resize_window_pts` sends the same).
fn coreResizeWindow(size: ResizeWindow, cell_width: u32, cell_height: u32, padding: Padding, scale_x: f32, scale_y: f32) ApprtResizeWindow {
    const width: f32 = @floatFromInt(@max(size.columns, min_resize_width_cells) *
        cell_width + padding.left + padding.right);
    const height: f32 = @floatFromInt(@max(size.rows, min_resize_height_cells) *
        cell_height + padding.top + padding.bottom);
    return .{
        .width = if (size.columns > 0) @intFromFloat(@ceil(width / scale_x)) else 0,
        .height = if (size.rows > 0) @intFromFloat(@ceil(height / scale_y)) else 0,
    };
}

/// Returns the change in logical pixels from the current device pixel
/// size to the requested size, or zero if the requested size is zero.
fn resizeDelta(requested: u32, scale: f32, current: u32, factor: f32) c_int {
    if (requested == 0) return 0;
    const px = @as(f32, @floatFromInt(requested)) * scale;
    return @intFromFloat(@round((px - @as(f32, @floatFromInt(current))) / factor));
}

const Gtk = struct {
    // surface.getContentScale()
    scale_x: f32,
    scale_y: f32,
    // surface.getSize() (device pixels)
    cur_w: u32,
    cur_h: u32,
    // widget.getScaleFactor()
    scale_factor: c_int,
    // widget.getWidth()/getHeight() (logical pixels, incl. headerbar)
    widget_w: c_int,
    widget_h: c_int,
};

// The arithmetic of Window.resizeSurface (219cca932) after the refusals,
// returning the two `setDefaultSize` arguments.
fn resizeSurface(g: Gtk, size: ApprtResizeWindow) struct { c_int, c_int } {
    const factor: f32 = @floatFromInt(g.scale_factor);
    const dw = resizeDelta(size.width, g.scale_x, g.cur_w, factor);
    const dh = resizeDelta(size.height, g.scale_y, g.cur_h, factor);
    return .{
        g.widget_w + dw,
        g.widget_h + dh,
    };
}

pub fn main(init: std.process.Init) !void {
    var it = try init.minimal.args.iterateAllocator(init.gpa);
    defer it.deinit();
    _ = it.next();
    const case = it.next() orelse "ok";

    const nopad: Padding = .{ .top = 0, .bottom = 0, .left = 0, .right = 0 };
    // A 1x window: an 800x480 device-px surface under a 47 px headerbar.
    const g1: Gtk = .{ .scale_x = 1, .scale_y = 1, .cur_w = 800, .cur_h = 480, .scale_factor = 1, .widget_w = 800, .widget_h = 527 };

    const r: struct { c_int, c_int } = if (std.mem.eql(u8, case, "ok"))
        // Positive control: CSI 8;24;80 t, 10x20 px cells, 2 px padding on a
        // 1x display. Core sends 804x484; the window grows by 4x4.
        resizeSurface(g1, coreResizeWindow(.{ .rows = 24, .columns = 80 }, 10, 20, .{ .top = 2, .bottom = 2, .left = 2, .right = 2 }, 1, 1))
    else if (std.mem.eql(u8, case, "hidpi"))
        // Positive control on a 2x display: 20x40 device-px cells, core sends
        // points (content scale 2), GTK converts back to device px, then to
        // logical px with the integer factor 2.
        resizeSurface(
            .{ .scale_x = 2, .scale_y = 2, .cur_w = 1600, .cur_h = 960, .scale_factor = 2, .widget_w = 800, .widget_h = 527 },
            coreResizeWindow(.{ .rows = 24, .columns = 80 }, 20, 40, nopad, 2, 2),
        )
    else if (std.mem.eql(u8, case, "cvt"))
        // `adjust-cell-width = 39990` on a 10 px cell (cell 40000 px),
        // CSI 8;24;65535 t on a 1x display. Core: 65535 * 40000 = 2621400000,
        // which fits u32 (no UD371 overflow), so the action is sent. The
        // delta 2621399200 exceeds maxInt(c_int).
        resizeSurface(g1, coreResizeWindow(.{ .rows = 24, .columns = 65535 }, 40000, 20, nopad, 1, 1))
    else if (std.mem.eql(u8, case, "add"))
        // `adjust-cell-height` giving a 32768 px cell and `window-padding-y =
        // 16610` (scale 1), CSI 8;65535;0 t on a 1x display with a 600 px
        // surface under a 50 px headerbar. Core: 65535 * 32768 + 2 * 16610 =
        // 2147484100, which fits u32. As f32 it is 2147484160; minus 600 it
        // rounds to 2147483520, which fits c_int, so the conversion passes,
        // but the `widget.getHeight() + dh` sum exceeds maxInt(c_int).
        blk: {
            const size = coreResizeWindow(.{ .rows = 65535, .columns = 0 }, 10, 32768, .{ .top = 16610, .bottom = 16610, .left = 0, .right = 0 }, 1, 1);
            const g: Gtk = .{ .scale_x = 1, .scale_y = 1, .cur_w = 800, .cur_h = 600, .scale_factor = 1, .widget_w = 800, .widget_h = 650 };
            std.debug.print("PROBE add: core height={d} dh={d}\n", .{ size.height, resizeDelta(size.height, g.scale_y, g.cur_h, 1) });
            break :blk resizeSurface(g, size);
        }
    else
        return error.UnknownCase;
    std.debug.print("PROBE {s}: setDefaultSize({d}, {d})\n", .{ case, r[0], r[1] });
}
