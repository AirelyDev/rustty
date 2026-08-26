//! PORT NOTE: `synthetic/cli/Ascii.zig`.

use crate::bytes::{generate_alphabet, is_print, Bytes};
use crate::io::{WriteFailed, Writer};
use crate::rand::Rng;

// PORT NOTE: `const log = std.log.scoped(.@"terminal-stream-bench")` — logging seam,
// not exercised by the generator path.

/// PORT NOTE: `pub const Options = struct { ... }` (7031c892b). The zero field
/// defaults are the derived `Default`.
#[derive(Default)]
pub struct Options {
    /// When nonzero, emit lines whose printable length is uniformly
    /// distributed in `[line-min, line-max]`, each terminated by CR LF.
    /// When zero (the default), emit an unbroken stream of printable
    /// bytes that relies on terminal wrapping.
    // @"line-min": usize = 0,
    // @"line-max": usize = 0,
    pub line_min: usize,
    pub line_max: usize,
}

impl rustty_args::args::Target for Options {
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        match key {
            // @"line-min": usize = 0
            b"line-min" => {
                self.line_min = rustty_args::args::parse_field_int::<usize>(value)?;
                Ok(())
            }
            // @"line-max": usize = 0
            b"line-max" => {
                self.line_max = rustty_args::args::parse_field_int::<usize>(value)?;
                Ok(())
            }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}

// PORT NOTE: `checkAsciiAlphabet` — printable ASCII minus the space.
generate_alphabet!(pub ascii, |c| match c {
    b' ' => false,
    _ => is_print(c),
});

/// PORT NOTE: `const Ascii = @This()`.
pub struct Ascii {
    opts: Options,
}

impl Ascii {
    /// Create a new terminal stream handler for the given arguments.
    pub fn create(opts: Options) -> Result<Ascii, WriteFailed> {
        Ok(Ascii { opts })
    }

    /// PORT NOTE: `destroy(self, alloc)` — frees the heap instance; a no-op here (the
    /// struct is owned by the caller).
    pub fn destroy(self) {}

    pub fn run<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        rand: Rng,
    ) -> Result<(), WriteFailed> {
        // const line_max = @max(self.opts.@"line-min", self.opts.@"line-max");
        let line_max = self.opts.line_min.max(self.opts.line_max);
        // const lines = line_max > 0;
        let lines = line_max > 0;
        let gen = Bytes {
            rand,
            alphabet: Some(ascii()),
            // .min_len = if (lines) @max(self.opts.@"line-min", 1) else 1024,
            min_len: if lines {
                self.opts.line_min.max(1)
            } else {
                1024
            },
            // .max_len = if (lines) line_max else 1024,
            max_len: if lines { line_max } else { 1024 },
        };

        loop {
            if write_chunk(&gen, writer, lines).is_err() {
                // error.BrokenPipe => return, // stdout closed
                // error.WriteFailed => return, // fixed buffer full
                return Ok(());
            }
        }
    }
}

// fn writeChunk(gen: *const Bytes, writer: *std.Io.Writer, lines: bool) std.Io.Writer.Error!void {
fn write_chunk<W: Writer<Error = WriteFailed>>(
    gen: &Bytes,
    writer: &mut W,
    lines: bool,
) -> Result<(), WriteFailed> {
    // _ = try gen.write(writer);
    let _ = gen.write(writer)?;
    // if (lines) try writer.writeAll("\r\n");
    if lines {
        writer.writeAll(b"\r\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::FixedWriter;
    use crate::rand::DefaultPrng;

    #[test]
    fn ascii() {
        let impl_ = Ascii::create(Options::default()).unwrap();

        let prng = DefaultPrng::init(1);
        let rand = prng.random();

        let mut buf = [0u8; 1024];
        let mut writer = FixedWriter::fixed(&mut buf);
        impl_.run(&mut writer, rand).unwrap();
        impl_.destroy();
    }

    // test "Ascii lines" {
    #[test]
    fn ascii_lines() {
        let impl_ = Ascii::create(Options {
            line_min: 5,
            line_max: 20,
        })
        .unwrap();

        let prng = DefaultPrng::init(1);
        let rand = prng.random();

        let mut buf = [0u8; 1024];
        let mut writer = FixedWriter::fixed(&mut buf);
        impl_.run(&mut writer, rand).unwrap();

        // Every emitted line respects the configured bounds.
        // PORT NOTE: `std.mem.splitSequence(u8, out, "\r\n")` with the
        // `if (it.rest().len == 0) break` guard — the fixed buffer may end
        // mid-line, so the final segment (rest empty) is never checked.
        // (The final delimiter-less segment — `it.next()` with `it.rest()`
        // empty — falls out of the `while let` when no "\r\n" remains; the
        // reference breaks without checking it.)
        let mut rest: &[u8] = writer.buffered();
        while let Some(pos) = rest.windows(2).position(|window| window == b"\r\n") {
            let line = &rest[..pos];
            rest = &rest[pos + 2..];
            // The fixed buffer may end mid-line.
            if rest.is_empty() {
                break;
            }
            assert!(line.len() >= 5);
            assert!(line.len() <= 20);
        }
        impl_.destroy();
    }
}
