// P7-W1 T3 side probe: can `Capacity.adjust`'s `@intCast(new_rows)` overflow
// `CellCountInt` from a reachable caller? Append to a SCRATCH copy of the
// oracle's `src/terminal/PageList.zig` and run
//   zig build test-lib-vt -Dtest-filter="P7W1T3 adjust intCast probe"

test "P7W1T3 adjust intCast probe" {
    const testing = std.testing;
    const alloc = testing.allocator;
    var s = try init(alloc, .{ .cols = 50000, .rows = 24, .max_size = 0 });
    defer s.deinit();
    std.debug.print("\nPROBE cap cols={d} rows={d}\n", .{ s.pages.first.?.page().capacity.cols, s.pages.first.?.page().capacity.rows });
    try s.resize(.{ .cols = 80, .reflow = true });
    std.debug.print("PROBE resized cols={d} first_cap_rows={d}\n", .{ s.cols, s.pages.first.?.page().capacity.rows });
}
