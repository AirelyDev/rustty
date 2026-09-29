const std = @import("std");
const uucode = @import("uucode");

// P7-W10 T2: the rustty-vaxis uucode differential probe; see run.sh.
//
// Dumps uucode 9d555245's four vaxis fields for 0..=0x10FFFF (1 byte each:
// eaw, gc, gb, emoji_pres) and the 5x21x21 computeGraphemeBreak table
// (result | tableIndex<<1), key = state*441 + gb1*21 + gb2.
pub fn main(init: std.process.Init) !void {
    const alloc = init.arena.allocator();
    const io = init.io;
    const n = 0x110000;
    const buf = try alloc.alloc(u8, n * 4);
    var cp: u32 = 0;
    while (cp < n) : (cp += 1) {
        const c: u21 = @intCast(cp);
        buf[cp * 4 + 0] = @intFromEnum(uucode.get(.east_asian_width, c));
        buf[cp * 4 + 1] = @intFromEnum(uucode.get(.general_category, c));
        buf[cp * 4 + 2] = @intFromEnum(uucode.get(.grapheme_break, c));
        buf[cp * 4 + 3] = @intFromBool(uucode.get(.is_emoji_presentation, c));
    }
    try std.Io.Dir.cwd().writeFile(io, .{ .sub_path = "zig_fields.bin", .data = buf });

    const GB = uucode.types.GraphemeBreak;
    const fields = @typeInfo(GB).@"enum".fields;
    const ngb = fields.len;
    var tbl: [5 * ngb * ngb]u8 = undefined;
    for (0..uucode.grapheme.BreakState.table_len) |si| {
        for (0..ngb) |g1| {
            for (0..ngb) |g2| {
                var st = uucode.grapheme.BreakState.fromTableIndex(si);
                const r = uucode.grapheme.computeGraphemeBreak(@enumFromInt(g1), @enumFromInt(g2), &st);
                tbl[si * ngb * ngb + g1 * ngb + g2] = @as(u8, @intFromBool(r)) | (@as(u8, @intCast(st.tableIndex())) << 1);
            }
        }
    }
    try std.Io.Dir.cwd().writeFile(io, .{ .sub_path = "zig_gb_table.bin", .data = &tbl });
}
