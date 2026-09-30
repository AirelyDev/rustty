// P7-W3 T1 side probes (UD367, UD368): append to a SCRATCH copy of the
// oracle's `src/terminal/render.zig` at 12752b2ac and run
//   zig build test-lib-vt -Dtest-filter="P7W3 UD368 probe"
//   zig build test-lib-vt -Dtest-filter="P7W3 UD367 probe"

test "P7W3 UD368 probe" {
    const testing = std.testing;
    const alloc = testing.allocator;
    const io = testing.io;
    var t = try Terminal.init(io, alloc, .{ .cols = 10, .rows = 3 });
    defer t.deinit(alloc);
    var s = t.vtStream();
    defer s.deinit();
    s.nextSlice("plain\r\n\x1b[1mbold\x1b[0m\r\n\x1b[3mital\x1b[0m");
    var state: RenderState = .empty;
    defer state.deinit(alloc);
    state.overscan_request = .{ .above = 65535, .below = 0 };
    std.debug.print("\nPROBE UD368 before update\n", .{});
    try state.update(alloc, &t);
    const vp = state.viewportStart();
    const cells = state.row_data.items(.cells);
    std.debug.print("PROBE UD368 row1 bold={} row2 italic={}\n", .{
        cells[vp + 1].items(.style)[0].flags.bold,
        cells[vp + 2].items(.style)[0].flags.italic,
    });
}

test "P7W3 UD367 probe" {
    const testing = std.testing;
    const alloc = testing.allocator;
    const io = testing.io;
    var t = try Terminal.init(io, alloc, .{ .cols = 10, .rows = 10, .max_scrollback_bytes = 1_000_000 });
    defer t.deinit(alloc);
    {
        var s = t.vtStream();
        defer s.deinit();
        var buf: [32]u8 = undefined;
        for (0..50) |i| s.nextSlice(try std.fmt.bufPrint(&buf, "{d}\r\n", .{i}));
    }
    t.scrollViewport(.{ .delta = -5 });
    var failing = std.testing.FailingAllocator.init(alloc, .{});
    const fa = failing.allocator();
    var state: RenderState = .empty;
    defer state.deinit(fa);
    try state.update(fa, &t);
    state.overscan_request = .{ .above = 30, .below = 30 };
    failing.fail_index = failing.alloc_index;
    const r = state.update(fa, &t);
    std.debug.print("\nPROBE UD367 update result={any} len={d}\n", .{ r, state.row_data.len });
    const range = state.rowDataRange();
    std.debug.print("PROBE UD367 range={d}..{d} len={d}\n", .{ range.start, range.end, state.row_data.len });
    const raws = state.row_data.items(.raw)[range.start..range.end];
    std.debug.print("PROBE UD367 sliced {d}\n", .{raws.len});
}
