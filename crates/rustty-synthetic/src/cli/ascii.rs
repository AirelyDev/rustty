//! PORT NOTE: `synthetic/cli/Ascii.zig`.

use crate::bytes::{generate_alphabet, is_print, Bytes};
use crate::io::{WriteFailed, Writer};
use crate::rand::Rng;

// PORT NOTE: `const log = std.log.scoped(.@"terminal-stream-bench")` — logging seam,
// not exercised by the generator path.

/// PORT NOTE: `pub const Options = struct {}`.
#[derive(Default)]
pub struct Options {}

impl rustty_args::args::Target for Options {
    fn parse_into_field(
        &mut self,
        _key: &[u8],
        _value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        // Options is empty — no field matches; `parse` maps this to "unknown field".
        Err(rustty_args::args::Error::InvalidField)
    }
}

// PORT NOTE: `checkAsciiAlphabet` — printable ASCII minus the space.
generate_alphabet!(pub ascii, |c| match c {
    b' ' => false,
    _ => is_print(c),
});

/// PORT NOTE: `const Ascii = @This()`.
pub struct Ascii {}

impl Ascii {
    /// Create a new terminal stream handler for the given arguments.
    pub fn create(_: Options) -> Result<Ascii, WriteFailed> {
        Ok(Ascii {})
    }

    /// PORT NOTE: `destroy(self, alloc)` — frees the heap instance; a no-op here (the
    /// struct is owned by the caller).
    pub fn destroy(self) {}

    pub fn run<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        rand: Rng,
    ) -> Result<(), WriteFailed> {
        let gen = Bytes {
            rand,
            alphabet: Some(ascii()),
            min_len: 1024,
            max_len: 1024,
        };

        loop {
            if gen.write(writer).is_err() {
                // error.BrokenPipe => return, // stdout closed
                // error.WriteFailed => return, // fixed buffer full
                return Ok(());
            }
        }
    }
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
}
