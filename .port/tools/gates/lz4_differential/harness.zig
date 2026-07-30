//! G-LZ4 differential harness — ORACLE ARM.
//!
//! Compiles `lz4.zig` (extracted read-only from the ghostty oracle by
//! `check_lz4_byte_exact.sh`) standalone and walks the corpus specified by
//! `CORPUS.md`, printing one line per point. `harness_body.rs` is the
//! independent Rust transcription of the same specification.
//!
//! Nothing here may depend on anything but `std` and `lz4.zig`.

const std = @import("std");
const lz4 = @import("lz4.zig");

// ---------------------------------------------------------------------------
// PRNG + hash (CORPUS.md "PRNG", "Hash of emitted bytes")
// ---------------------------------------------------------------------------

const Rng = struct {
    state: u64,

    fn init(seed: u64) Rng {
        return .{ .state = seed };
    }

    fn next(self: *Rng) u64 {
        self.state +%= 0x9E3779B97F4A7C15;
        var z = self.state;
        z = (z ^ (z >> 30)) *% 0xBF58476D1CE4E5B9;
        z = (z ^ (z >> 27)) *% 0x94D049BB133111EB;
        return z ^ (z >> 31);
    }
};

fn fnv1a(bytes: []const u8) u64 {
    var h: u64 = 0xCBF29CE484222325;
    for (bytes) |b| {
        h ^= b;
        h *%= 0x100000001B3;
    }
    return h;
}

// ---------------------------------------------------------------------------
// Corpus
// ---------------------------------------------------------------------------

const sizes = [_]usize{
    0,     1,     2,     3,     4,     5,     6,     7,      8,      11,
    12,    13,    15,    16,    17,    31,    32,    33,     63,     64,
    65,    127,   128,   129,   255,   256,   257,   4095,   4096,   4097,
    65535, 65536, 65537, 204800, 409600,
};

const max_size: usize = 409600;

fn fill(family: u64, size: usize, seed: u64, buf: []u8) void {
    var rng = Rng.init(seed ^ 0x9E3779B97F4A7C15 ^ (family *% 0x100000001B3));
    const dst = buf[0..size];
    switch (family) {
        0 => @memset(dst, 0),
        1 => {
            for (dst, 0..) |*p, i| {
                const rec = i / 8;
                p.* = switch (i % 8) {
                    0 => @intCast((rec % 95) + 32),
                    3 => @intCast((rec / 95) % 7),
                    4, 5 => 0xFF,
                    else => 0,
                };
            }
        },
        2 => {
            var w: u64 = 0;
            for (dst, 0..) |*p, i| {
                if (i % 8 == 0) w = rng.next();
                p.* = @truncate(w >> @intCast((i % 8) * 8));
            }
        },
        3 => {
            var i: usize = 0;
            while (i < size) {
                const v = rng.next();
                const run: usize = @intCast(1 + v % 64);
                const b: u8 = @truncate((v >> 32) & 0xFF);
                var k: usize = 0;
                while (k < run and i < size) : (k += 1) {
                    dst[i] = b;
                    i += 1;
                }
            }
        },
        4 => {
            const period: usize = @intCast(1 + rng.next() % 96);
            var pattern: [96]u8 = undefined;
            var w: u64 = 0;
            var j: usize = 0;
            while (j < period) : (j += 1) {
                if (j % 8 == 0) w = rng.next();
                pattern[j] = @truncate(w >> @intCast((j % 8) * 8));
            }
            for (dst, 0..) |*p, i| p.* = pattern[i % period];
        },
        5 => {
            @memset(dst, 0);
            const pokes = size / 64;
            var k: usize = 0;
            while (k < pokes) : (k += 1) {
                const v = rng.next();
                dst[@intCast(v % size)] = @truncate((v >> 32) & 0xFF);
            }
        },
        6 => {
            var i: usize = 0;
            while (i < size) {
                const v = rng.next();
                const wlen: usize = @intCast(2 + v % 9);
                var k: usize = 0;
                while (k < wlen and i < size) : (k += 1) {
                    const shift: u6 = @intCast(8 + (k % 6) * 4);
                    dst[i] = @intCast('a' + (v >> shift) % 26);
                    i += 1;
                }
                if (i < size) {
                    dst[i] = if ((v >> 60) % 8 == 0) '\n' else ' ';
                    i += 1;
                }
            }
        },
        7 => {
            for (dst, 0..) |*p, i| p.* = @intCast(i % 251);
        },
        else => unreachable,
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

var out_buf: []u8 = undefined;
var out_len: usize = 0;

fn emit(comptime fmt: []const u8, args: anytype) void {
    const s = std.fmt.bufPrint(out_buf[out_len..], fmt, args) catch @panic("output buffer overflow");
    out_len += s.len;
}

var table: lz4.HashTable = undefined;

extern "c" fn write(fd: c_int, buf: [*]const u8, n: usize) isize;
const c_write = write;

pub fn main() !void {
    const alloc = std.heap.page_allocator;
    out_buf = try alloc.alloc(u8, 16 << 20);
    const input = try alloc.alloc(u8, max_size);
    const encoded = try alloc.alloc(u8, try lz4.compressBound(max_size));
    const decoded = try alloc.alloc(u8, 2 * max_size + 64);

    // --- phase 1: families x boundary sizes -------------------------------
    var family: u64 = 0;
    while (family < 8) : (family += 1) {
        for (sizes, 0..) |size, si| {
            const seed: u64 = family * 1000 + si;
            fill(family, size, seed, input);
            const n = try lz4.compress(input[0..size], encoded, &table);
            emit("p1,{d},{d},{d},{d},{x}\n", .{ family, size, seed, n, fnv1a(encoded[0..n]) });
        }
    }

    // --- phase 2: randomized triples --------------------------------------
    var r2 = Rng.init(0x5EED0002);
    var p: usize = 0;
    while (p < 4000) : (p += 1) {
        const fam = r2.next() % 8;
        const size: usize = @intCast(r2.next() % 409601);
        const seed = r2.next();
        fill(fam, size, seed, input);
        const n = try lz4.compress(input[0..size], encoded, &table);
        emit("p2,{d},{d},{d},{d},{x}\n", .{ fam, size, seed, n, fnv1a(encoded[0..n]) });
    }

    // --- phase 3: every match period 1..96 over 200 KiB --------------------
    var period: usize = 1;
    while (period <= 96) : (period += 1) {
        const seed: u64 = 0xBEEF0000 + period;
        var rng = Rng.init(seed);
        var pattern: [96]u8 = undefined;
        var w: u64 = 0;
        var j: usize = 0;
        while (j < period) : (j += 1) {
            if (j % 8 == 0) w = rng.next();
            pattern[j] = @truncate(w >> @intCast((j % 8) * 8));
        }
        const size: usize = 204800;
        var i: usize = 0;
        while (i < size) : (i += 1) input[i] = pattern[i % period];
        const n = try lz4.compress(input[0..size], encoded, &table);
        emit("p3,{d},{d},{d},{d},{x}\n", .{ period, size, seed, n, fnv1a(encoded[0..n]) });
    }

    // --- phase 4: decoder behaviour on corrupted blocks --------------------
    const work = try alloc.alloc(u8, try lz4.compressBound(max_size));
    var b: u64 = 0;
    while (b < 60) : (b += 1) {
        const fam = b % 8;
        const size: usize = @intCast(64 + (b *% 137) % 8192);
        const seed: u64 = 0xB10C +% b;
        fill(fam, size, seed, input);
        const enc_len = try lz4.compress(input[0..size], encoded, &table);

        var c: u64 = 0;
        while (c < 60) : (c += 1) {
            var rng = Rng.init(b *% 1000 +% c);
            @memcpy(work[0..enc_len], encoded[0..enc_len]);
            var len = enc_len;
            switch (c % 4) {
                0 => {
                    const pos: usize = @intCast(rng.next() % enc_len);
                    const bit: u3 = @intCast(rng.next() % 8);
                    work[pos] ^= @as(u8, 1) << bit;
                },
                1 => {
                    const pos: usize = @intCast(rng.next() % enc_len);
                    work[pos] = @truncate(rng.next() & 0xFF);
                },
                2 => {
                    const pos: usize = @intCast(rng.next() % enc_len);
                    var k: usize = 0;
                    while (k < 16) : (k += 1) {
                        const v = rng.next();
                        if (pos + k < enc_len) work[pos + k] = @truncate(v & 0xFF);
                    }
                },
                3 => len = @intCast(rng.next() % enc_len),
                else => unreachable,
            }
            const obuf: usize = switch (c % 3) {
                0 => size,
                1 => size / 2,
                else => size * 2 + 16,
            };
            // Zero the whole output buffer first and hash ALL of it, not just
            // `decoded[0..dn]`. `9a4bd2120`'s decoder does blind wide copies
            // that deliberately write scratch bytes past a run's logical end;
            // hashing only the logical output would leave "which scratch bytes
            // survive" uncompared, and that is exactly where two byte-faithful
            // ports can silently drift apart.
            @memset(decoded[0..obuf], 0);
            if (lz4.decompress(work[0..len], decoded[0..obuf])) |dn| {
                emit("p4,{d},{d},{d},{d},{x}\n", .{ b, c, obuf, dn, fnv1a(decoded[0..obuf]) });
            } else |err| {
                emit("p4,{d},{d},{d},ERR:{s}\n", .{ b, c, obuf, @errorName(err) });
            }
        }
    }

    // Write straight to fd 1 through libc. `std.io`/`std.Io`'s writer surface
    // has been rewritten twice between 0.14 and 0.16 (0.16 removed
    // `std.posix.write` outright and routes `File.writer` through an `Io`
    // instance threaded from `std.process.Init`), so a locally declared
    // `extern "c" write` is the one spelling that survives a toolchain bump.
    // The gate builds this arm with `-lc`.
    var written: usize = 0;
    while (written < out_len) {
        const n = c_write(1, out_buf[written..].ptr, out_len - written);
        if (n <= 0) return error.WriteFailed;
        written += @intCast(n);
    }
}
