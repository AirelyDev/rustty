//! PORT NOTE: `synthetic/cli/Utf8.zig`.

use crate::io::{WriteFailed, Writer};
use crate::rand::{DefaultPrng, Rng};
use crate::utf8 as synthetic_utf8;
use crate::utf8::Utf8Len;

/// PORT NOTE: the `error.InvalidValue` returned by `create` on a bad option.
#[derive(Debug)]
pub struct InvalidValue;

/// PORT NOTE: `pub const Options = struct { ... }`.
pub struct Options {
    /// Seed to use for deterministic generation. If unset, a time-based
    /// seed is used by the generic synthetic CLI.
    pub seed: Option<u64>,

    /// Relative weight for choosing 1-byte UTF-8 sequences.
    pub weight_one: f64,

    /// Relative weight for choosing 2-byte UTF-8 sequences.
    pub weight_two: f64,

    /// Relative weight for choosing 3-byte UTF-8 sequences.
    pub weight_three: f64,

    /// Relative weight for choosing 4-byte UTF-8 sequences.
    pub weight_four: f64,

    /// Restrict ASCII codepoints to printable characters.
    pub ascii_printable_only: bool,

    /// Probability that an emitted sequence is malformed UTF-8.
    pub invalid_rate: f64,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            seed: None,
            weight_one: 1.0,
            weight_two: 1.0,
            weight_three: 1.0,
            weight_four: 1.0,
            ascii_printable_only: false,
            invalid_rate: 0.0,
        }
    }
}

/// PORT NOTE: `const Utf8 = @This()`.
pub struct Utf8 {
    opts: Options,
}

impl Utf8 {
    /// Create a new terminal stream handler for the given arguments.
    pub fn create(opts: Options) -> Result<Utf8, InvalidValue> {
        if opts.invalid_rate < 0.0 || opts.invalid_rate > 1.0 {
            return Err(InvalidValue);
        }

        let weights = [
            opts.weight_one,
            opts.weight_two,
            opts.weight_three,
            opts.weight_four,
        ];
        let mut weight_sum: f64 = 0.0;
        for weight in weights {
            if weight < 0.0 {
                return Err(InvalidValue);
            }
            weight_sum += weight;
        }
        if weight_sum <= 0.0 {
            return Err(InvalidValue);
        }

        Ok(Utf8 { opts })
    }

    pub fn destroy(self) {}

    pub fn run<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        rand: Rng,
    ) -> Result<(), WriteFailed> {
        let seeded;
        let gen_rand = match self.opts.seed {
            Some(seed) => {
                seeded = DefaultPrng::init(seed);
                seeded.random()
            }
            None => rand,
        };

        let mut gen = synthetic_utf8::Utf8 {
            ascii_printable_only: self.opts.ascii_printable_only,
            invalid_rate: self.opts.invalid_rate,
            ..synthetic_utf8::Utf8::new(gen_rand)
        };
        gen.p_length.set(Utf8Len::One.index(), self.opts.weight_one);
        gen.p_length.set(Utf8Len::Two.index(), self.opts.weight_two);
        gen.p_length
            .set(Utf8Len::Three.index(), self.opts.weight_three);
        gen.p_length
            .set(Utf8Len::Four.index(), self.opts.weight_four);

        loop {
            if gen.next(writer, 1024).is_err() {
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
    fn utf8() {
        let impl_ = Utf8::create(Options {
            seed: Some(1),
            ..Options::default()
        })
        .unwrap();

        let prng = DefaultPrng::init(1);
        let rand = prng.random();

        let mut buf = [0u8; 1024];
        let mut writer = FixedWriter::fixed(&mut buf);
        impl_.run(&mut writer, rand).unwrap();
        assert_eq!(1024usize, writer.buffered().len());
        impl_.destroy();
    }
}
