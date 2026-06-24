const std = @import("std");
const terminal = @import("terminal");
const PageList = terminal.PageList;
const point = terminal.point;
const page = terminal.page;

fn printGeometry() !void {
    const std_cap = page.std_capacity;
    const std_layout = page.Page.layout(std_cap);
    const psm = std.heap.page_size_min;

    // item_size in a MemoryPoolAligned is the backing array size [std_size]u8
    // aligned up to the pool alignment (page_size_min). So:
    const std_size = std_layout.total_size;
    const item_size = std.mem.alignForward(usize, std_size, psm);

    const stdout = std.fs.File.stdout();
    var buf: [2048]u8 = undefined;
    const msg = try std.fmt.bufPrint(&buf,
        \\GEOMETRY (DERIVED from page.zig layout, target = native)
        \\page_size_min={d}
        \\std_capacity.cols={d} std_capacity.rows={d} std_capacity.styles={d} std_capacity.grapheme_bytes={d}
        \\std_size_total_size={d}
        \\PagePool.item_size(aligned)={d}
        \\
    , .{
        psm,
        std_cap.cols,    std_cap.rows, std_cap.styles, std_cap.grapheme_bytes,
        std_size, item_size,
    });
    try stdout.writeAll(msg);
}

// Identical workload to the rustty bench:
//   PageList.init(80,24,large_max)
//   -> grow_rows to ~2000 scrollback rows (many page allocs)
//   -> scroll across the viewport
//   -> clone the active+scrollback
//   -> resize cols (more then fewer; reflow + page churn)
//   -> reset(retain)
//   -> deinit
fn workload(alloc: std.mem.Allocator) !void {
    const large_max: usize = 1024 * 1024 * 1024; // 1 GiB max_size

    var pl = try PageList.init(alloc, 80, 24, large_max);
    defer pl.deinit();

    // grow_rows to ~2000 total rows of scrollback (start at 24 active)
    const target_rows: usize = 2000;
    while (pl.total_rows < target_rows) {
        _ = try pl.grow();
    }

    // scroll across the viewport: walk the viewport from top to active.
    {
        var r: usize = 0;
        while (r < pl.total_rows) : (r += 1) {
            pl.scroll(.{ .row = r });
        }
        pl.scroll(.{ .active = {} });
    }

    // clone the active + scrollback (full screen)
    {
        var cloned = try pl.clone(alloc, .{
            .top = .{ .screen = .{ .x = 0, .y = 0 } },
            .bot = null,
        });
        cloned.deinit();
    }

    // resize cols: more then fewer (forces reflow + page churn)
    try pl.resize(.{ .cols = 200, .reflow = true });
    try pl.resize(.{ .cols = 40, .reflow = true });
    try pl.resize(.{ .cols = 80, .reflow = true });

    // reset(retain)
    pl.reset();
}

// Counting wrapper over an underlying allocator to measure how many
// page-allocator allocations (mmaps) and bytes the arena requests.
const CountingAlloc = struct {
    child: std.mem.Allocator,
    n_alloc: usize = 0,
    n_resize_ok: usize = 0,
    n_resize_fail: usize = 0,
    bytes: usize = 0,

    fn allocator(self: *CountingAlloc) std.mem.Allocator {
        return .{ .ptr = self, .vtable = &.{
            .alloc = cAlloc,
            .resize = cResize,
            .remap = cRemap,
            .free = cFree,
        } };
    }
    fn cAlloc(ctx: *anyopaque, n: usize, a: std.mem.Alignment, ra: usize) ?[*]u8 {
        const self: *CountingAlloc = @ptrCast(@alignCast(ctx));
        const r = self.child.rawAlloc(n, a, ra);
        if (r != null) {
            self.n_alloc += 1;
            self.bytes += n;
        }
        return r;
    }
    fn cResize(ctx: *anyopaque, buf: []u8, a: std.mem.Alignment, new_len: usize, ra: usize) bool {
        const self: *CountingAlloc = @ptrCast(@alignCast(ctx));
        const ok = self.child.rawResize(buf, a, new_len, ra);
        if (ok) self.n_resize_ok += 1 else self.n_resize_fail += 1;
        return ok;
    }
    fn cRemap(ctx: *anyopaque, buf: []u8, a: std.mem.Alignment, new_len: usize, ra: usize) ?[*]u8 {
        const self: *CountingAlloc = @ptrCast(@alignCast(ctx));
        return self.child.rawRemap(buf, a, new_len, ra);
    }
    fn cFree(ctx: *anyopaque, buf: []u8, a: std.mem.Alignment, ra: usize) void {
        const self: *CountingAlloc = @ptrCast(@alignCast(ctx));
        self.child.rawFree(buf, a, ra);
    }
};

// Replays the page-pool arena allocation sequence (item_size = std page)
// through a REAL std ArenaAllocator + std page_allocator wrapped in a counter.
// MEASURED at the arena layer (isolated from PageList, identical mechanics):
// preheat(4) then N std-page item allocations.
fn arenaSim(n_items_total: usize) !void {
    const item_size = item_size_const;
    const align_v: std.mem.Alignment = .fromByteUnits(std.heap.page_size_min);

    var counter: CountingAlloc = .{ .child = std.heap.page_allocator };
    var arena = std.heap.ArenaAllocator.init(counter.allocator());
    defer arena.deinit();
    const a = arena.allocator();

    // preheat(4): MemoryPool.initPreheated allocates 4 items up front.
    var i: usize = 0;
    while (i < n_items_total) : (i += 1) {
        const p = a.rawAlloc(item_size, align_v, @returnAddress()) orelse return error.OutOfMemory;
        _ = p;
    }

    const stdout = std.fs.File.stdout();
    var buf: [1024]u8 = undefined;
    const msg = try std.fmt.bufPrint(&buf,
        \\ARENA-SIM (MEASURED, real std ArenaAllocator over std.heap.page_allocator)
        \\items_allocated={d} item_size={d}
        \\underlying_page_allocs(mmap-ish)={d}
        \\underlying_bytes={d}
        \\arena_resize_ok={d} arena_resize_fail={d}
        \\
    , .{ n_items_total, item_size, counter.n_alloc, counter.bytes, counter.n_resize_ok, counter.n_resize_fail });
    try stdout.writeAll(msg);
}

const item_size_const: usize = blk: {
    const std_layout = page.Page.layout(page.std_capacity);
    break :blk std.mem.alignForward(usize, std_layout.total_size, std.heap.page_size_min);
};

fn accounting(alloc: std.mem.Allocator) !void {
    const stdout = std.fs.File.stdout();
    var buf: [4096]u8 = undefined;
    var off: usize = 0;
    const W = struct {
        fn line(b: []u8, o: *usize, comptime fmt: []const u8, args: anytype) !void {
            const s = try std.fmt.bufPrint(b[o.*..], fmt, args);
            o.* += s.len;
        }
    };

    const large_max: usize = 1024 * 1024 * 1024;
    var pl = try PageList.init(alloc, 80, 24, large_max);
    defer pl.deinit();

    try W.line(&buf, &off, "STAGE init: page_size={d} total_rows={d} approx_pages={d}\n", .{ pl.page_size, pl.total_rows, pl.page_size / item_size_const });

    // grow_rows to ~2000: count grow() calls that allocated a NEW node.
    var grow_calls: usize = 0;
    var new_node_allocs: usize = 0;
    const target_rows: usize = 2000;
    while (pl.total_rows < target_rows) {
        const r = try pl.grow();
        grow_calls += 1;
        if (r != null) new_node_allocs += 1;
    }
    try W.line(&buf, &off, "STAGE grow: grow_calls={d} grow_returned_node(prune/reuse)={d} page_size={d} total_rows={d} approx_std_pages={d}\n", .{ grow_calls, new_node_allocs, pl.page_size, pl.total_rows, pl.page_size / item_size_const });

    // scroll
    {
        var r: usize = 0;
        while (r < pl.total_rows) : (r += 1) pl.scroll(.{ .row = r });
        pl.scroll(.{ .active = {} });
    }
    try W.line(&buf, &off, "STAGE scroll: page_size={d} total_rows={d}\n", .{ pl.page_size, pl.total_rows });

    // clone
    {
        var cloned = try pl.clone(alloc, .{ .top = .{ .screen = .{ .x = 0, .y = 0 } }, .bot = null });
        try W.line(&buf, &off, "STAGE clone: clone_page_size={d} clone_total_rows={d} clone_approx_pages={d}\n", .{ cloned.page_size, cloned.total_rows, cloned.page_size / item_size_const });
        cloned.deinit();
    }

    // resize cols
    try pl.resize(.{ .cols = 200, .reflow = true });
    try W.line(&buf, &off, "STAGE resize->200: page_size={d} total_rows={d} approx_pages={d}\n", .{ pl.page_size, pl.total_rows, pl.page_size / item_size_const });
    try pl.resize(.{ .cols = 40, .reflow = true });
    try W.line(&buf, &off, "STAGE resize->40: page_size={d} total_rows={d} approx_pages={d}\n", .{ pl.page_size, pl.total_rows, pl.page_size / item_size_const });
    try pl.resize(.{ .cols = 80, .reflow = true });
    try W.line(&buf, &off, "STAGE resize->80: page_size={d} total_rows={d} approx_pages={d}\n", .{ pl.page_size, pl.total_rows, pl.page_size / item_size_const });

    pl.reset();
    try W.line(&buf, &off, "STAGE reset: page_size={d} total_rows={d} approx_pages={d}\n", .{ pl.page_size, pl.total_rows, pl.page_size / item_size_const });

    try stdout.writeAll(buf[0..off]);
}

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    const args = try std.process.argsAlloc(alloc);
    defer std.process.argsFree(alloc, args);

    var iters: usize = 200;
    var warmup: usize = 20;
    for (args[1..]) |a| {
        if (std.mem.eql(u8, a, "--info")) {
            try printGeometry();
            return;
        }
        if (std.mem.eql(u8, a, "--accounting")) {
            try accounting(alloc);
            return;
        }
        if (std.mem.startsWith(u8, a, "--arena-sim=")) {
            const n = try std.fmt.parseInt(usize, a["--arena-sim=".len..], 10);
            try arenaSim(n);
            return;
        }
    }
    for (args[1..]) |a| {
        if (std.mem.startsWith(u8, a, "--iters=")) {
            iters = try std.fmt.parseInt(usize, a["--iters=".len..], 10);
        } else if (std.mem.startsWith(u8, a, "--warmup=")) {
            warmup = try std.fmt.parseInt(usize, a["--warmup=".len..], 10);
        }
    }

    // Warmup
    for (0..warmup) |_| try workload(alloc);

    // Timed
    var samples = try alloc.alloc(u64, iters);
    defer alloc.free(samples);

    var timer = try std.time.Timer.start();
    for (0..iters) |i| {
        timer.reset();
        try workload(alloc);
        samples[i] = timer.read();
    }

    std.mem.sort(u64, samples, {}, std.sort.asc(u64));
    const min = samples[0];
    const max = samples[iters - 1];
    const median = samples[iters / 2];
    const p99 = samples[(iters * 99) / 100];
    var sum: u128 = 0;
    for (samples) |s| sum += s;
    const mean: u64 = @intCast(sum / iters);

    const stdout = std.fs.File.stdout();
    var buf: [1024]u8 = undefined;
    const msg = try std.fmt.bufPrint(&buf,
        \\iters={d} warmup={d}
        \\min_ns={d}
        \\median_ns={d}
        \\mean_ns={d}
        \\p99_ns={d}
        \\max_ns={d}
        \\
    , .{ iters, warmup, min, median, mean, p99, max });
    try stdout.writeAll(msg);
}
