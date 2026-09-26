//! Generator of `zig_corpus.rs`: Zig 0.16.0's own `std.fmt.parseInt` and
//! `std.fmt.parseFloat` results over a corpus of inputs, which the port's
//! `fmt` tests compare against. Not compiled by cargo.
//!
//! Regenerate: `zig run crates/rustty-zigstd/src/fmt/zig_corpus.zig 2> out.txt`
//! with Zig 0.16.0, then convert `out.txt` (one tab-separated line per input,
//! the input in `std.zig.fmtString` escapes) into the two Rust tables.

const std = @import("std");

const int_inputs = [_][]const u8{
    "0",        "10",       "-10",       "+10",      "-0",       "+0",      "1_0",       "1__0",
    "_10",      "10_",      "-_1",       "+_1",      "1_000",    "65535",   "65536",     "32767",
    "32768",    "-32768",   "-32769",    "255",      "256",      "",        "+",         "-",
    " 1",       "1 ",       "0x10",      "1e3",      "1.0",      "--1",     "+-1",       "9223372036854775807",
    "9223372036854775808", "-9223372036854775808", "18446744073709551615", "18446744073709551616",
    "4294967295", "4294967296", "-2147483648", "2147483648", "0010", "٣",
    "99999999999999999999999x", "256x", "1_2_3", "12_",
};

const float_inputs = [_][]const u8{
    "0",           "-0",          "+0",          "1",          "1.",         ".5",         "0.",         ".0",
    "1e3",         "1E3",         "1e+3",        "1e-3",       "1e",         "1e+",        "e3",         ".",
    ".e1",         "0.e",         "1_0",         "1__0",       "_1",         "1_",         "1_.5",       "1._5",
    "1.5_5",       "1e1_0",       "1e_10",       "0x1p1",      "0x1.8p3",    "0x1.8",      "0X1P-2",     "0x",
    "0x.8",        "0x1_0p0",     "0x1_p0",      "0x1p",       "0xffffffffffffffff.0p0", "inf", "INF", "-inf",
    "infinity",    "Infinity",    "infin",       "nan",        "NaN",        "-nan",       "nanx",       "1abc",
    "   1",        "1 ",          "+",           "-",          "",           "3.141",      "13.5",       "0.1",
    "1e-5000",     "1e+5000",     "3.4028234664E38", "3.4028235677973366e38", "1.7976931348623157E308",
    "1e39",        "1e-46",       "1e-45",       "2.71828182845904523536", "123456789012345678901234567890",
    "0.000000000000000000000000000000000000000000001401298464324817", "0x1p-149", "0x1p-150", "0x1.fffffep127",
    "0x1p128",     "0x0.1234570p-125", "2_0",   "-2_0.5",      "12",         "1.2e1",      "0x1.8p3",   "0_1_2_3_4_5_6.7_8_9_0_0_0e0_0_1_0",
    "0.4e0066999999999999999999999999999999999999999999999999999", "9007199254740993.0", "1.00000017881393432617187499",
    "0x1.000001p0", "0x1.0000018p0", "0x.0000000000000000001p0", "08", "1e0_1", "1.e5",
};

fn printInt(comptime T: type, s: []const u8) void {
    if (std.fmt.parseInt(T, s, 10)) |v| {
        std.debug.print("{d}", .{v});
    } else |e| {
        std.debug.print("{s}", .{@errorName(e)});
    }
}

pub fn main() void {
    std.debug.print("INTS\n", .{});
    for (int_inputs) |s| {
        std.debug.print("{f}\t", .{std.zig.fmtString(s)});
        inline for (.{ i16, u16, i32, u32, isize, usize, u8, i64 }) |T| {
            printInt(T, s);
            std.debug.print("\t", .{});
        }
        std.debug.print("\n", .{});
    }
    std.debug.print("FLOATS\n", .{});
    for (float_inputs) |s| {
        std.debug.print("{f}\t", .{std.zig.fmtString(s)});
        if (std.fmt.parseFloat(f32, s)) |v| {
            std.debug.print("0x{x:0>8}", .{@as(u32, @bitCast(v))});
        } else |e| std.debug.print("{s}", .{@errorName(e)});
        std.debug.print("\t", .{});
        if (std.fmt.parseFloat(f64, s)) |v| {
            std.debug.print("0x{x:0>16}", .{@as(u64, @bitCast(v))});
        } else |e| std.debug.print("{s}", .{@errorName(e)});
        std.debug.print("\n", .{});
    }
}
