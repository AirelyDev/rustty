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
// uucode.grapheme.computeGraphemeBreakNoControl over every
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
    gb1: uucode.types.GraphemeBreakNoControl,
    gb2: uucode.types.GraphemeBreakNoControl,

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
    const info = @typeInfo(uucode.types.GraphemeBreakNoControl).@"enum";
    for (0..max_state_int + 1) |state_int| {
        for (info.fields) |field1| {
            for (info.fields) |field2| {
                var state: uucode.grapheme.BreakState = @enumFromInt(state_int);
                const key: Key = .{
                    .gb1 = @field(uucode.types.GraphemeBreakNoControl, field1.name),
                    .gb2 = @field(uucode.types.GraphemeBreakNoControl, field2.name),
                    .state = state,
                };
                const v = uucode.grapheme.computeGraphemeBreakNoControl(
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

fn writeIn(io: std.Io, dir: []const u8, name: []const u8, data: []const u8) !void {
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    const path = try std.fmt.bufPrint(&buf, "{s}/{s}", .{ dir, name });
    try std.Io.Dir.cwd().writeFile(io, .{ .sub_path = path, .data = data });
}

// zig 0.16 removed `std.process.argsAlloc` and the `std.fs.cwd()` blocking file
// API; argv now arrives through the `std.process.Init` main parameter and file
// I/O goes through `std.Io` / `std.Io.Dir` (see std/start.zig callMain).
// Bumped in lockstep with ghostty e8525c0fd "Update to Zig 0.16.0".
pub fn main(init: std.process.Init) !void {
    const alloc = init.arena.allocator();
    const io = init.io;

    const args = try init.minimal.args.toSlice(alloc);
    if (args.len < 2) {
        std.debug.print("usage: uucode-oracle <out-dir>\n", .{});
        std.process.exit(2);
    }
    const out_dir = args[1];
    std.Io.Dir.cwd().createDirPath(io, out_dir) catch {};

    const n = MAX_CP + 1;
    const width_buf = try alloc.alloc(u8, n);
    const symbol_buf = try alloc.alloc(u8, n);
    const flags_buf = try alloc.alloc(u8, n);
    const gb_buf = try alloc.alloc(u8, n);

    var cp: u32 = 0;
    while (cp <= MAX_CP) : (cp += 1) {
        const width: u8 = uucode.get(.width, @intCast(cp));
        const sym: u8 = @intFromBool(uucode.get(.is_symbol, @intCast(cp)));
        const wzg: u8 = @intFromBool(uucode.get(.wcwidth_zero_in_grapheme, @intCast(cp)));
        const evb: u8 = @intFromBool(uucode.get(.is_emoji_vs_base, @intCast(cp)));
        const gb = uucode.get(.grapheme_break_no_control, @intCast(cp));

        width_buf[cp] = width;
        symbol_buf[cp] = sym;
        flags_buf[cp] = (width & 0x3) | (wzg << 2) | (evb << 3);
        gb_buf[cp] = @intFromEnum(gb);
    }

    try writeIn(io, out_dir, "width.bin", width_buf);
    try writeIn(io, out_dir, "symbol.bin", symbol_buf);
    try writeIn(io, out_dir, "props_flags.bin", flags_buf);
    try writeIn(io, out_dir, "props_gb.bin", gb_buf);

    // Grapheme precompute (8192 entries, 1 byte each).
    var precompute_buf: [grapheme_precompute.len]u8 = undefined;
    for (grapheme_precompute, 0..) |v, i| {
        precompute_buf[i] = @as(u4, @bitCast(v));
    }
    try writeIn(io, out_dir, "grapheme.bin", &precompute_buf);
}
