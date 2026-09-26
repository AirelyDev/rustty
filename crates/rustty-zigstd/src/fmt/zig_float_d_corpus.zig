//! Generator of `zig_float_d_corpus.rs`: Zig 0.16.0's `{d}` text for a corpus
//! of `f32` and `f64` values (given by bit pattern), which the port's
//! `format_float_d` tests compare against. Not compiled by cargo.
//!
//! Regenerate: `zig run crates/rustty-zigstd/src/fmt/zig_float_d_corpus.zig 2> out.txt`
//! with Zig 0.16.0; each line is `f32|f64 <hex bits> <text>`.

const std = @import("std");

const f32_values = [_]f32{
    0,        -0.0,      1,          -1,        12,        13.5,       0.5,       0.1,
    0.2,      0.3,       1.0e-7,     1.0e-10,   1.0e20,    1.0e38,     3.4028235e38, 1.1754944e-38,
    1.0e-45,  7.25,      10.5,       100.25,    0.001,     123456.79,  16777216,  16777218,
    2.5e-5,   9.999999,  72,         96,        144.5,     11.25,      13.333333, 1.0e7,
    1.0e8,    1.23e15,   -13.5,      std.math.inf(f32), -std.math.inf(f32),
};

const f64_values = [_]f64{
    0,       -0.0,  1,      12,        13.5,     0.1,      0.3,     1.0e-7,
    1.0e100, 5e-324, 1.7976931348623157e308, 123456789.123, 0.30000000000000004, 1.0e21, 1.0e22, -2.5,
    std.math.inf(f64),
};

pub fn main() void {
    for (f32_values) |v| {
        std.debug.print("f32 {x:0>8} {d}\n", .{ @as(u32, @bitCast(v)), v });
    }
    std.debug.print("f32 {x:0>8} {d}\n", .{ @as(u32, @bitCast(std.math.nan(f32))), std.math.nan(f32) });
    std.debug.print("f32 {x:0>8} {d}\n", .{ @as(u32, @bitCast(-std.math.nan(f32))), -std.math.nan(f32) });
    for (f64_values) |v| {
        std.debug.print("f64 {x:0>16} {d}\n", .{ @as(u64, @bitCast(v)), v });
    }
    std.debug.print("f64 {x:0>16} {d}\n", .{ @as(u64, @bitCast(std.math.nan(f64))), std.math.nan(f64) });
}
