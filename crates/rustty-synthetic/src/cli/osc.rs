//! PORT: `synthetic/cli/Osc.zig`.

use crate::io::{FixedWriter, WriteFailed, Writer, WriterExt};
use crate::osc as synthetic_osc;
use crate::rand::Rng;

// PORT: `const log = std.log.scoped(.@"terminal-stream-bench")` — logging seam.

/// PORT: the `style` option's inline enum.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Style {
    /// Write all OSC data, including ESC ] and ST for end-to-end tests
    #[default]
    Streaming,
    /// Only write data, prefixed with a length, used for testing just the
    /// OSC parser.
    Parser,
}

/// PORT: `pub const Options = struct { ... }`.
pub struct Options {
    /// Probability of generating a valid value.
    pub p_valid: f64,

    pub style: Style,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            p_valid: 0.5,
            style: Style::Streaming,
        }
    }
}

/// PORT: `const Osc = @This()`.
pub struct Osc {
    opts: Options,
}

impl Osc {
    /// Create a new terminal stream handler for the given arguments.
    pub fn create(opts: Options) -> Result<Osc, WriteFailed> {
        Ok(Osc { opts })
    }

    pub fn destroy(self) {}

    pub fn run<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        rand: Rng,
    ) -> Result<(), WriteFailed> {
        let mut gen = synthetic_osc::Osc {
            rand,
            p_valid: self.opts.p_valid,
            ..synthetic_osc::Osc::new(rand)
        };

        let mut buf = [0u8; 1024];
        loop {
            let mut fixed = FixedWriter::fixed(&mut buf);
            gen.next(&mut fixed, 1024)?;
            let data = fixed.buffered();
            match self.opts.style {
                Style::Streaming => {
                    if writer.writeAll(data).is_err() {
                        // error.WriteFailed => return
                        return Ok(());
                    }
                }
                Style::Parser => {
                    if writer.writeIntUsizeLittle(data.len() - 3).is_err() {
                        // error.WriteFailed => return
                        return Ok(());
                    }
                    if writer.writeAll(&data[2..data.len() - 1]).is_err() {
                        // error.WriteFailed => return
                        return Ok(());
                    }
                }
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
    fn osc() {
        let impl_ = Osc::create(Options::default()).unwrap();

        let prng = DefaultPrng::init(1);
        let rand = prng.random();

        let mut buf = [0u8; 1024];
        let mut writer = FixedWriter::fixed(&mut buf);
        impl_.run(&mut writer, rand).unwrap();
        impl_.destroy();
    }
}
