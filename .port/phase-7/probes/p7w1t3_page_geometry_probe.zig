// P7-W1 T3 (d2ff6d77a) page-geometry probe. Append to a SCRATCH copy of the
// oracle's `src/terminal/page.zig` (never the oracle itself) and run
//   zig build test-lib-vt -Dtest-filter="P7W1T3 geometry probe"
// natively on macOS arm64, and with -Dtarget=aarch64-linux-gnu (run the
// built test binary on Linux aarch64). Output lines start with PROBE.
// Feeds: page_tests.rs `page_geometry_matches_the_reference`,
// tests/std_size_golden.rs, and the UD64 / UD80 / UD131 thresholds.

test "P7W1T3 geometry probe" {
    const widths = [_]u16{ 1, 40, 80, 100, 119, 120, 132, 160, 200, 215, 256, 1000, 10000 };
    const styles_list = [_]u16{ 256, 2048, 8192, 65535 };
    std.debug.print("\nPROBE page_size_min={d} cache_line={d} cells_align={d} meta_align={d} sizeof_row={d} sizeof_cell={d}\n", .{
        std.heap.page_size_min, std.atomic.cache_line, cells_align, Page.MetaLayout.alignment, @sizeOf(Row), @sizeOf(Cell),
    });
    for ([_]u32{ 512, 8192 }) |gb| {
        var cap = std_capacity;
        cap.grapheme_bytes = gb;
        const l = Page.layout(cap);
        const m = Page.MetaLayout.init(cap);
        std.debug.print("PROBE gb={d} total_size={d} rows_size={d} cells_start={d} cells_size={d} styles_start={d} styles={d} galloc={d} gmap={d} string={d} hset={d} hmap={d} meta_total={d} available_bytes={d} max_cols={?d}\n", .{
            gb,                                l.total_size,                        l.rows_size,                       l.cells_start,
            l.cells_size,                      l.styles_start,                      l.styles_layout.total_size,        l.grapheme_alloc_layout.total_size,
            l.grapheme_map_layout.total_size,  l.string_alloc_layout.total_size,    l.hyperlink_set_layout.total_size, l.hyperlink_map_layout.total_size,
            m.total_size,                      cap.availableBytesForGrid(),         cap.maxCols(),
        });
        for (widths) |c| {
            const a = try cap.adjust(.{ .cols = c });
            std.debug.print("PROBE gb={d} adjust cols={d} rows={d}\n", .{ gb, c, a.rows });
        }
        for (styles_list) |s| {
            var c2: Capacity = .{ .cols = 80, .rows = 591 };
            c2.styles = s;
            c2.grapheme_bytes = gb;
            std.debug.print("PROBE gb={d} non_std styles={d} total={d}\n", .{ gb, s, Page.layout(c2).total_size });
        }
        // UD80/UD64 thresholds: the smallest width at which adjust fails.
        const mc = cap.maxCols().?;
        const fails = if (cap.adjust(.{ .cols = mc + 1 })) |_| false else |_| true;
        const at_max = try cap.adjust(.{ .cols = mc });
        std.debug.print("PROBE gb={d} threshold_first_fail={d} fails={} rows_at_max={d}\n", .{ gb, mc + 1, fails, at_max.rows });
        // UD131: the most cells a standard page holds over every width up to maxCols.
        var best: usize = 0;
        var best_c: u32 = 0;
        var c: u32 = 1;
        while (c <= mc) : (c += 1) {
            const a = try cap.adjust(.{ .cols = @intCast(c) });
            const cells = @as(usize, a.rows) * @as(usize, a.cols);
            if (cells > best) {
                best = cells;
                best_c = c;
            }
        }
        const a215 = try cap.adjust(.{ .cols = 215 });
        std.debug.print("PROBE gb={d} max_cells={d} at_cols={d} rows_at_215={d} cells_at_215={d}\n", .{ gb, best, best_c, a215.rows, @as(usize, a215.rows) * 215 });
    }
    // UD64 boundary: 48,151 x std rows, the test-build budget as the start.
    {
        var native = std_capacity;
        native.cols = 48151;
        native.grapheme_bytes = 512;
        const limit: u64 = size.max_page_size;
        const lay = struct {
            fn f(n: Capacity, gb: u32) u64 {
                var x = n;
                x.grapheme_bytes = gb;
                return Page.layout(x).total_size;
            }
        }.f;
        var lo: u32 = 512;
        var hi: u32 = std.math.maxInt(u32);
        while (hi - lo > 1) {
            const mid = lo + (hi - lo) / 2;
            if (lay(native, mid) > limit) hi = mid else lo = mid;
        }
        std.debug.print("PROBE ud64 at_limit={d} layout={d} fits={d} layout_fits={d}\n", .{ hi, lay(native, hi), lo, lay(native, lo) });
        for ([_]u32{ 1 << 29, 1 << 30, 1 << 31 }) |d| {
            std.debug.print("PROBE ud64 doubling {d} layout={d} ok={}\n", .{ d, lay(native, d), lay(native, d) <= limit });
        }
    }
}
