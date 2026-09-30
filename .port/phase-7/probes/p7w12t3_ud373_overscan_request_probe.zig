// P7-W12 T3 probe (UD373 candidate): an overscan REQUEST change that keeps
// the `row_data` length and the captured overscan the same does not force a
// redraw. Append to a SCRATCH copy of the oracle's `src/terminal/render.zig`
// at 12752b2ac (`git -C ghostty archive 12752b2ac | tar -x -C <scratch>`) and
// run
//   zig build test-lib-vt -Dtest-filter="P7W12T3 UD373 probe"
//
// 10x3 terminal, no scrollback: request {1,0} captures 0 rows above (none
// exist) and puts the viewport at row_data[1..4]; request {0,1} captures 0
// rows below (the viewport follows the active area) and puts the viewport at
// row_data[0..3]. Both give row_data.len == 4 and overscan == {0,0}.

test "P7W12T3 UD373 probe" {
    const testing = std.testing;
    const alloc = testing.allocator;
    const io = testing.io;
    var t = try Terminal.init(io, alloc, .{ .cols = 10, .rows = 3 });
    defer t.deinit(alloc);
    var s = t.vtStream();
    defer s.deinit();
    s.nextSlice("AAA\r\nBBB\r\nCCC");

    var state: RenderState = .empty;
    defer state.deinit(alloc);
    state.overscan_request = .{ .above = 1, .below = 0 };
    try state.update(alloc, &t);
    state.clean();

    state.overscan_request = .{ .above = 0, .below = 1 };
    try state.update(alloc, &t);

    const vp = state.viewportStart();
    const cells = state.row_data.items(.cells);
    // A never-built entry has zero cells; print it as '-'.
    var line: [3][10]u8 = @splat(@splat('-'));
    var ncells: [3]usize = undefined;
    for (0..3) |y| {
        const cps = cells[vp + y].items(.raw);
        ncells[y] = cps.len;
        for (0..@min(10, cps.len)) |x| {
            const cp = cps[x].codepoint();
            line[y][x] = if (cp == 0) '.' else @intCast(cp);
        }
    }
    std.debug.print(
        "\nPROBE UD373 dirty={t} len={d} overscan=({d},{d}) vp={d}\n" ++
            "PROBE UD373 row0='{s}'({d} cells) row1='{s}'({d}) row2='{s}'({d})\n",
        .{
            state.dirty,
            state.row_data.len,
            state.overscan.above,
            state.overscan.below,
            vp,
            &line[0],
            ncells[0],
            &line[1],
            ncells[1],
            &line[2],
            ncells[2],
        },
    );
}
