// P7-W1 T3: UD131 recipe re-verification at 12752b2ac (after d2ff6d77a).
// Append to a SCRATCH copy of the oracle's `src/terminal/Terminal.zig` and run
//   zig build test-lib-vt -Dtest-filter="P7W1T3 ud131"
// (Debug test build: `std_capacity.grapheme_bytes` is 512, so page 1 holds
// 593 rows at 80 columns on macOS arm64; a shipped build holds 591.)
// The 32-colour test is expected to ABORT with "increaseCapacity OutOfSpace";
// the 31-colour control must pass. Mirrors the port's
// `ud131_delete_lines_degrades_on_an_ordinary_80_column_terminal`.

fn p7w1t3Ud131Run(n_colliding: usize) !void {
    const alloc = testing.allocator;
    const colours = [_][3]u8{
        .{ 0, 3, 183 },    .{ 0, 36, 22 },    .{ 0, 212, 121 }, .{ 1, 39, 95 },    .{ 1, 89, 203 },
        .{ 1, 98, 209 },   .{ 1, 196, 183 },  .{ 3, 10, 143 },  .{ 3, 27, 63 },    .{ 3, 136, 88 },
        .{ 3, 177, 19 },   .{ 4, 4, 80 },     .{ 4, 38, 52 },   .{ 5, 170, 136 },  .{ 6, 82, 67 },
        .{ 6, 211, 183 },  .{ 6, 221, 6 },    .{ 6, 247, 172 }, .{ 7, 62, 169 },   .{ 7, 87, 247 },
        .{ 7, 177, 15 },   .{ 8, 49, 218 },   .{ 8, 179, 161 }, .{ 9, 140, 163 },  .{ 9, 205, 72 },
        .{ 10, 83, 2 },    .{ 10, 93, 46 },   .{ 10, 248, 1 },  .{ 11, 112, 126 }, .{ 11, 210, 50 },
        .{ 13, 123, 81 },  .{ 14, 56, 135 },
    };
    var t = try init(testing.io, alloc, .{ .cols = 80, .rows = 24 });
    defer t.deinit(alloc);
    var s = t.vtStream();
    defer s.deinit();
    s.nextSlice("\x1b[24;1H");
    const pages = &t.screens.active.pages;
    std.debug.print("\nPROBE ud131 page1_cap_rows={d}\n", .{pages.pages.first.?.page().capacity.rows});
    var lfs: usize = 0;
    var boundary: ?usize = null;
    while (lfs < 4000) {
        s.nextSlice("\n");
        lfs += 1;
        const n0 = pages.pin(.{ .active = .{ .x = 0, .y = 0 } }).?.node;
        var y: usize = 1;
        while (y < 24) : (y += 1) {
            if (pages.pin(.{ .active = .{ .x = 0, .y = @intCast(y) } }).?.node != n0) {
                if (y >= 4 and y <= 20) boundary = y;
                break;
            }
        }
        if (boundary != null) break;
    }
    const b = boundary.?;
    std.debug.print("PROBE ud131 n={d} lfs={d} boundary_y={d}\n", .{ n_colliding, lfs, b });
    var buf: [4096]u8 = undefined;
    var w: std.Io.Writer = .fixed(&buf);
    try w.writeAll("\x1b[1;1H");
    for (colours[0..n_colliding]) |c| try w.print("\x1b[38;2;{d};{d};{d}mX", .{ c[0], c[1], c[2] });
    try w.writeAll("\x1b[0m");
    try w.print("\x1b[{d};1H\x1b[48;2;1;2;3mY\x1b[0m", .{b + 1});
    try w.writeAll("\x1b[2;1H\x1b[M");
    const out = w.buffered();
    std.debug.print("PROBE ud131 n={d} recipe_bytes_after_lfs={d} total_bytes={d}\n", .{ n_colliding, out.len, out.len + lfs + 7 });
    s.nextSlice(out);
    std.debug.print("PROBE ud131 n={d} survived\n", .{n_colliding});
}

test "P7W1T3 ud131 thirty-one colour control" {
    try p7w1t3Ud131Run(31);
}

test "P7W1T3 ud131 thirty-two colours" {
    try p7w1t3Ud131Run(32);
}
