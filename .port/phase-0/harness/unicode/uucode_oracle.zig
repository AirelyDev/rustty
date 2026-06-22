const std = @import("std");
const uucode = @import("uucode");

// =============================================================================
// rustty unicode differential oracle (ORACLE #2) — harness-only tooling.
//
// This is the INDEPENDENT external oracle for the rustty-unicode tables. It is
// built against uucode 0.2.0 (the exact dependency ghostty uses) with ghostty's
// own src/build/uucode_config.zig, and dumps — for EVERY code point in
// 0..=0x10FFFF — the same fields ghostty's reference tests read out of uucode:
//
//   * width                     (uucode.get(.width, cp))
//   * is_symbol                 (uucode.get(.is_symbol, cp))
//   * wcwidth_zero_in_grapheme  (uucode.get(.wcwidth_zero_in_grapheme, cp))
//   * is_emoji_vs_base          (uucode.get(.is_emoji_vs_base, cp))
//   * grapheme_break_no_control (uucode.get(.grapheme_break_no_control, cp))
//
// plus the 8192-entry grapheme-break precompute, computed bit-for-bit the same
// way ghostty's src/unicode/grapheme.zig embeds it at comptime — via
// uucode.x.grapheme.computeGraphemeBreakNoControl over every
// (state, gb1, gb2) key.
//
// The reference's `props_uucode.zig` / `symbols_uucode.zig` `test "... tables
// match uucode"` blocks compare ghostty's GENERATED tables against this LIVE
// uucode source. rustty cannot depend on uucode in-crate (R3), so that
// full-range differential is reproduced HERE as a permanent harness gate
// (oracle #2). The rust side (dump_rust_tables) dumps rustty's embedded tables
// over the identical range; diff_unicode.py asserts 0 diffs.
//
// R3: this Zig generator + its uucode dependency are harness-only. No rustty
// Cargo crate depends on uucode or ghostty.
//
// Output layout (all under the directory given as argv[1]):
//   width.bin        1 byte/cp,  0..=0x10FFFF   (uucode width, u8)
//   symbol.bin       1 byte/cp,  0..=0x10FFFF   (is_symbol, 0/1)
//   props_flags.bin  1 byte/cp,  0..=0x10FFFF   (bit0..1 width(u2),
//                                                 bit2 wcwidth_zero_in_grapheme,
//                                                 bit3 is_emoji_vs_base)
//   props_gb.bin     1 byte/cp,  0..=0x10FFFF   (grapheme_break_no_control int)
//   grapheme.bin     8192 bytes  (result | state<<1), the precompute table
// =============================================================================

const MAX_CP: u32 = 0x10FFFF; // uucode.config.max_code_point

// Mirror of ghostty src/unicode/grapheme.zig Precompute (comptime), recomputed
// here directly from uucode's own break logic so the embedded rustty table can
// be diffed against an INDEPENDENT regeneration.
const Key = packed struct(u13) {
    state: uucode.grapheme.BreakState,
    gb1: uucode.x.types.GraphemeBreakNoControl,
    gb2: uucode.x.types.GraphemeBreakNoControl,

    fn index(self: Key) usize {
        return @intCast(@as(u13, @bitCast(self)));
    }
};

const Value = packed struct(u4) {
    result: bool,
    state: uucode.grapheme.BreakState,
};

const grapheme_precompute = precompute: {
    var result: [std.math.maxInt(u13) + 1]Value = undefined;

    const max_state_int = blk: {
        var max: usize = 0;
        for (@typeInfo(uucode.grapheme.BreakState).@"enum".fields) |field| {
            if (field.value > max) max = field.value;
        }
        break :blk max;
    };

    @setEvalBranchQuota(1_000_000);
    const info = @typeInfo(uucode.x.types.GraphemeBreakNoControl).@"enum";
    for (0..max_state_int + 1) |state_int| {
        for (info.fields) |field1| {
            for (info.fields) |field2| {
                var state: uucode.grapheme.BreakState = @enumFromInt(state_int);
                const key: Key = .{
                    .gb1 = @field(uucode.x.types.GraphemeBreakNoControl, field1.name),
                    .gb2 = @field(uucode.x.types.GraphemeBreakNoControl, field2.name),
                    .state = state,
                };
                const v = uucode.x.grapheme.computeGraphemeBreakNoControl(
                    key.gb1,
                    key.gb2,
                    &state,
                );
                result[key.index()] = .{ .result = v, .state = state };
            }
        }
    }

    break :precompute result;
};

fn createIn(dir: []const u8, name: []const u8) !std.fs.File {
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    const path = try std.fmt.bufPrint(&buf, "{s}/{s}", .{ dir, name });
    return std.fs.cwd().createFile(path, .{});
}

pub fn main() !void {
    var arena = std.heap.ArenaAllocator.init(std.heap.page_allocator);
    defer arena.deinit();
    const alloc = arena.allocator();

    const args = try std.process.argsAlloc(alloc);
    if (args.len < 2) {
        std.debug.print("usage: uucode-oracle <out-dir>\n", .{});
        std.process.exit(2);
    }
    const out_dir = args[1];
    std.fs.cwd().makePath(out_dir) catch {};

    var bw: [1 << 16]u8 = undefined;
    var bs: [1 << 16]u8 = undefined;
    var bf: [1 << 16]u8 = undefined;
    var bg: [1 << 16]u8 = undefined;

    var fw = try createIn(out_dir, "width.bin");
    defer fw.close();
    var ww = fw.writer(&bw);

    var fs = try createIn(out_dir, "symbol.bin");
    defer fs.close();
    var sw = fs.writer(&bs);

    var ff = try createIn(out_dir, "props_flags.bin");
    defer ff.close();
    var flw = ff.writer(&bf);

    var fg = try createIn(out_dir, "props_gb.bin");
    defer fg.close();
    var gbw = fg.writer(&bg);

    var cp: u32 = 0;
    while (cp <= MAX_CP) : (cp += 1) {
        const width: u8 = uucode.get(.width, @intCast(cp));
        const sym: u8 = @intFromBool(uucode.get(.is_symbol, @intCast(cp)));
        const wzg: u8 = @intFromBool(uucode.get(.wcwidth_zero_in_grapheme, @intCast(cp)));
        const evb: u8 = @intFromBool(uucode.get(.is_emoji_vs_base, @intCast(cp)));
        const gb = uucode.get(.grapheme_break_no_control, @intCast(cp));

        const flags: u8 = (width & 0x3) | (wzg << 2) | (evb << 3);

        try ww.interface.writeByte(width);
        try sw.interface.writeByte(sym);
        try flw.interface.writeByte(flags);
        try gbw.interface.writeByte(@intFromEnum(gb));
    }
    try ww.interface.flush();
    try sw.interface.flush();
    try flw.interface.flush();
    try gbw.interface.flush();

    // Grapheme precompute (8192 entries, 1 byte each).
    var bp: [1 << 16]u8 = undefined;
    var fp = try createIn(out_dir, "grapheme.bin");
    defer fp.close();
    var pw = fp.writer(&bp);
    for (grapheme_precompute) |v| {
        const byte: u8 = @as(u4, @bitCast(v));
        try pw.interface.writeByte(byte);
    }
    try pw.interface.flush();
}
