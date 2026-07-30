//! PORT NOTE: `synthetic/cli/Kitty.zig`.
// const Kitty = @This();

// const log = std.log.scoped(.@"kitty-gen");

use crate::io::{FixedWriter, WriteFailed, Writer};
use crate::kitty as synthetic_kitty;
use crate::rand::Rng;

/// PORT NOTE: `pub const Options = struct { ... }`.
pub struct Options {
    /// The base64 payload length of each command in bytes, rounded
    /// down to a multiple of four. Kitty clients typically chunk
    /// payloads at 4096 bytes.
    // @"data-len": usize = 4096,
    pub data_len: usize,
}

impl Default for Options {
    fn default() -> Options {
        Options { data_len: 4096 }
    }
}

impl rustty_args::args::Target for Options {
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        match key {
            // @"data-len": usize = 4096
            b"data-len" => {
                self.data_len = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}

/// PORT NOTE: `const Kitty = @This()`.
pub struct Kitty {
    // opts: Options,
    opts: Options,

    /// The buffer a single generated sequence is written into. Sized to
    /// the payload length plus room for the control data and terminator.
    // buf: []u8,
    buf: Vec<u8>,
}

impl Kitty {
    /// Create a new Kitty graphics sequence generator for the given arguments.
    // pub fn create(alloc: Allocator, opts: Options) !*Kitty
    pub fn create(opts: Options) -> Result<Kitty, WriteFailed> {
        // const buf = try alloc.alloc(u8, opts.@"data-len" + 128);
        let buf = vec![0u8; opts.data_len + 128];
        Ok(Kitty { opts, buf })
    }

    // pub fn destroy(self: *Kitty, alloc: Allocator) void
    pub fn destroy(self) {}

    // pub fn run(self: *Kitty, writer: *std.Io.Writer, rand: std.Random) !void
    pub fn run<W: Writer<Error = WriteFailed>>(
        &mut self,
        writer: &mut W,
        rand: Rng,
    ) -> Result<(), WriteFailed> {
        // var gen: synthetic.Kitty = .{ .rand = rand, .data_len = self.opts.@"data-len" };
        let mut gen = synthetic_kitty::Kitty {
            rand,
            data_len: self.opts.data_len,
        };

        // while (true) {
        loop {
            // var fixed: std.Io.Writer = .fixed(self.buf);
            let cap = self.buf.len();
            let mut fixed = FixedWriter::fixed(&mut self.buf);
            // try gen.next(&fixed, self.buf.len);
            gen.next(&mut fixed, cap)?;
            let n = fixed.buffered().len();
            // writer.writeAll(fixed.buffered()) catch |err| switch (err) {
            //     error.WriteFailed => return,
            // };
            if writer.writeAll(&self.buf[..n]).is_err() {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rand::DefaultPrng;

    // test Kitty
    #[test]
    fn kitty() {
        let mut impl_ = Kitty::create(Options::default()).unwrap();

        let prng = DefaultPrng::init(1);
        let rand = prng.random();

        let mut buf = [0u8; 8192];
        let mut writer = FixedWriter::fixed(&mut buf);
        impl_.run(&mut writer, rand).unwrap();
        impl_.destroy();
    }
}
