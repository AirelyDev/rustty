//! Generates random terminal OSC requests.
//!
//! PORT: `synthetic/Osc.zig`.

use crate::bytes::{generate_alphabet, is_alphabetic, is_alphanumeric, is_print, Bytes, BEL, ESC};
use crate::enums::EnumArray;
use crate::generator::Generator;
use crate::io::{FixedWriter, WriteFailed, Writer, WriterExt};
use crate::rand::{Rng, U3};

/// Valid OSC request kinds that can be generated.
#[derive(Clone, Copy)]
pub enum ValidKind {
    ChangeWindowTitle,
    PromptStart,
    PromptEnd,
    EndOfInput,
    EndOfCommand,
    RxvtNotify,
    MouseShape,
    ClipboardOperation,
    ReportPwd,
    HyperlinkStart,
    HyperlinkEnd,
    ConemuProgress,
    Iterm2Notification,
}

impl ValidKind {
    /// PORT: the `EnumArray(ValidKind, f64).Indexer.keyForIndex(idx)` mapping
    /// (index → variant, declaration order).
    pub fn key_for_index(idx: usize) -> ValidKind {
        match idx {
            0 => ValidKind::ChangeWindowTitle,
            1 => ValidKind::PromptStart,
            2 => ValidKind::PromptEnd,
            3 => ValidKind::EndOfInput,
            4 => ValidKind::EndOfCommand,
            5 => ValidKind::RxvtNotify,
            6 => ValidKind::MouseShape,
            7 => ValidKind::ClipboardOperation,
            8 => ValidKind::ReportPwd,
            9 => ValidKind::HyperlinkStart,
            10 => ValidKind::HyperlinkEnd,
            11 => ValidKind::ConemuProgress,
            12 => ValidKind::Iterm2Notification,
            _ => unreachable!(),
        }
    }
}

/// Invalid OSC request kinds that can be generated.
#[derive(Clone, Copy)]
pub enum InvalidKind {
    /// Literally random bytes. Might even be valid, but probably not.
    Random,

    /// A good prefix, but ultimately invalid format.
    GoodPrefix,
}

impl InvalidKind {
    /// PORT: `EnumArray(InvalidKind, f64).Indexer.keyForIndex(idx)`.
    pub fn key_for_index(idx: usize) -> InvalidKind {
        match idx {
            0 => InvalidKind::Random,
            1 => InvalidKind::GoodPrefix,
            _ => unreachable!(),
        }
    }
}

/// PORT: `Osc` — the `@This()` struct.
pub struct Osc<'a> {
    /// Random number generator.
    pub rand: Rng<'a>,

    /// Probability of a valid OSC sequence being generated.
    pub p_valid: f64,

    /// Probabilities of specific valid or invalid OSC request kinds.
    /// The probabilities are weighted relative to each other, so they
    /// can sum greater than 1.0. A kind of weight 1.0 and a kind of
    /// weight 2.0 will have a 2:1 chance of the latter being selected.
    pub p_valid_kind: EnumArray<13>,
    pub p_invalid_kind: EnumArray<2>,
}

// PORT: `checkKvAlphabet` — omit 0x1B, 0x07, ';', '='.
// The alphabet for random bytes in OSC key/value pairs (omitting 0x1B,
// 0x07, ';', '=').
generate_alphabet!(pub kv_alphabet, |c| match c {
    ESC | BEL | b';' | b'=' => false,
    _ => is_print(c),
});

// PORT: `checkOscAlphabet` — omit 0x1B and 0x07.
// The alphabet for random bytes in OSCs (omitting 0x1B and 0x07).
generate_alphabet!(pub osc_alphabet, |c| !matches!(c, ESC | BEL));
generate_alphabet!(pub ascii_alphabet, |c| is_print(c));
generate_alphabet!(pub alphabetic_alphabet, |c| is_alphabetic(c));
generate_alphabet!(pub alphanumeric_alphabet, |c| is_alphanumeric(c));

/// PORT: `const osc = std.fmt.comptimePrint("{c}]", .{esc})` — bytes `1B 5D`.
const OSC: &[u8] = b"\x1b]";
/// PORT: `const st = std.fmt.comptimePrint("{c}", .{bel})` — byte `07`.
const ST: &[u8] = b"\x07";

/// PORT: `const Validity = enum { valid, invalid }`.
enum Validity {
    Valid,
    Invalid,
}

impl<'a> Osc<'a> {
    /// PORT: the struct-literal defaults (`p_valid = 1.0`,
    /// `p_valid_kind = .initFill(1.0)`, `p_invalid_kind = .initFill(1.0)`).
    pub fn new(rand: Rng<'a>) -> Osc<'a> {
        Osc {
            rand,
            p_valid: 1.0,
            p_valid_kind: EnumArray::init_fill(1.0),
            p_invalid_kind: EnumArray::init_fill(1.0),
        }
    }

    pub fn generator<'g, W: Writer<Error = WriteFailed>>(&'g mut self) -> Generator<'g, W> {
        fn next_trampoline<W: Writer<Error = WriteFailed>>(
            ptr: *mut (),
            writer: &mut W,
            max_len: usize,
        ) -> Result<(), WriteFailed> {
            let self_: &mut Osc = unsafe { &mut *(ptr as *mut Osc) };
            self_.next(writer, max_len)
        }
        Generator::init(self as *mut Osc as *mut (), next_trampoline::<W>)
    }

    /// Get the next OSC request in bytes. The generated OSC request will
    /// have the prefix `ESC ]` and the terminator `BEL` (0x07).
    ///
    /// This will generate both valid and invalid OSC requests (based on
    /// the `p_valid` probability value). Invalid requests still have the
    /// prefix and terminator, but the content in between is not a valid
    /// OSC request.
    ///
    /// The buffer must be at least 3 bytes long to accommodate the
    /// prefix and terminator.
    pub fn next<W: Writer<Error = WriteFailed>>(
        &mut self,
        writer: &mut W,
        max_len: usize,
    ) -> Result<(), WriteFailed> {
        assert!(max_len >= 3);
        writer.writeAll(OSC)?;
        self.next_unwrapped(writer, max_len - (OSC.len() + ST.len()))?;
        writer.writeAll(ST)?;
        Ok(())
    }

    fn next_unwrapped<W: Writer<Error = WriteFailed>>(
        &mut self,
        writer: &mut W,
        max_len: usize,
    ) -> Result<(), WriteFailed> {
        match self.choose_validity() {
            Validity::Valid => {
                let idx = self.rand.weighted_index_f64(&self.p_valid_kind.values);
                self.next_unwrapped_valid_exact(writer, ValidKind::key_for_index(idx), max_len)
            }

            Validity::Invalid => {
                let idx = self.rand.weighted_index_f64(&self.p_invalid_kind.values);
                self.next_unwrapped_invalid_exact(writer, InvalidKind::key_for_index(idx), max_len)
            }
        }
    }

    // PORT: several arms keep a running `remaining` whose final decrement is
    // unread (the reference does the same); the assignments are faithful.
    #[allow(unused_assignments)]
    fn next_unwrapped_valid_exact<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        k: ValidKind,
        max_len: usize,
    ) -> Result<(), WriteFailed> {
        match k {
            ValidKind::ChangeWindowTitle => 'change_window_title: {
                if max_len < 3 {
                    break 'change_window_title;
                }
                // Set window title
                writer.writeAll(b"0;")?;
                self.bytes().at_most(max_len - 3).format(writer)?;
            }

            ValidKind::PromptStart => 'prompt_start: {
                if max_len < 4 {
                    break 'prompt_start;
                }
                let mut remaining = max_len;

                writer.writeAll(b"133;A")?; // Start prompt
                remaining -= 4;

                // aid
                if self.rand.boolean() {
                    'aid: {
                        if remaining < 6 {
                            break 'aid;
                        }
                        writer.writeAll(b";aid=")?;
                        remaining -= 5;
                        remaining -= self
                            .bytes()
                            .new_alphabet(Some(kv_alphabet()))
                            .at_most(16.min(remaining))
                            .write(writer)?;
                    }
                }

                // redraw
                if self.rand.boolean() {
                    'redraw: {
                        if remaining < 9 {
                            break 'redraw;
                        }
                        writer.writeAll(b";redraw=")?;
                        if self.rand.boolean() {
                            writer.writeAll(b"1")?;
                        } else {
                            writer.writeAll(b"0")?;
                        }
                        remaining -= 9;
                    }
                }
            }

            ValidKind::PromptEnd => 'prompt_end: {
                if max_len < 4 {
                    break 'prompt_end;
                }
                writer.writeAll(b"133;B")?; // End prompt
            }

            ValidKind::EndOfInput => 'end_of_input: {
                if max_len < 5 {
                    break 'end_of_input;
                }
                let mut remaining = max_len;
                writer.writeAll(b"133;C")?; // End prompt
                remaining -= 5;
                if self.rand.boolean() {
                    'cmdline: {
                        let prefix: &[u8] = b";cmdline_url=";
                        if remaining < prefix.len() + 1 {
                            break 'cmdline;
                        }
                        writer.writeAll(prefix)?;
                        remaining -= prefix.len();
                        let mut buf = [0u8; 128];
                        let mut w = FixedWriter::fixed(&mut buf);
                        self.bytes()
                            .new_alphabet(Some(ascii_alphabet()))
                            .at_most(remaining.min(128))
                            .format(&mut w)?;
                        rustty_os::string_encoding::urlPercentEncode(writer, w.buffered())?;
                        remaining -= w.buffered().len();
                        let _ = remaining;
                    }
                }
            }

            ValidKind::EndOfCommand => 'end_of_command: {
                if max_len < 4 {
                    break 'end_of_command;
                }
                writer.writeAll(b"133;D")?; // End prompt
                if self.rand.boolean() {
                    'exit_code: {
                        if max_len < 7 {
                            break 'exit_code;
                        }
                        let n = self.rand.int::<u8>();
                        writer.writeAll(b";")?;
                        writer.writeUint(n as u64)?;
                    }
                }
            }

            ValidKind::MouseShape => 'mouse_shape: {
                if max_len < 4 {
                    break 'mouse_shape;
                }
                // Start prompt
                writer.writeAll(b"22;")?;
                self.bytes()
                    .new_alphabet(Some(alphabetic_alphabet()))
                    .at_most(32.min(max_len - 3))
                    .format(writer)?;
            }

            ValidKind::RxvtNotify => 'rxvt_notify: {
                let prefix: &[u8] = b"777;notify;";
                if max_len < prefix.len() {
                    break 'rxvt_notify;
                }
                let mut remaining = max_len;
                writer.writeAll(prefix)?;
                remaining -= prefix.len();
                remaining -= self
                    .bytes()
                    .new_alphabet(Some(kv_alphabet()))
                    .at_most((remaining - 2).min(32))
                    .write(writer)?;
                writer.writeByte(b';')?;
                remaining -= 1;
                remaining -= self
                    .bytes()
                    .new_alphabet(Some(osc_alphabet()))
                    .at_most(remaining)
                    .write(writer)?;
            }

            ValidKind::ClipboardOperation => {
                writer.writeAll(b"52;")?;
                let mut remaining = max_len - 3;
                if self.rand.boolean() {
                    remaining -= self
                        .bytes()
                        .new_alphabet(Some(alphabetic_alphabet()))
                        .at_most(1)
                        .write(writer)?;
                }
                writer.writeByte(b';')?;
                remaining -= 1;
                if self.rand.boolean() {
                    remaining -= self
                        .bytes()
                        .new_alphabet(Some(osc_alphabet()))
                        .at_most(remaining)
                        .write(writer)?;
                }
            }

            ValidKind::ReportPwd => 'report_pwd: {
                let prefix: &[u8] = b"7;file://localhost";
                if max_len < prefix.len() {
                    break 'report_pwd;
                }
                let mut remaining = max_len;
                writer.writeAll(prefix)?;
                remaining -= prefix.len();
                for _ in 0..self.rand.int_range_at_most::<usize>(2, 5) {
                    writer.writeByte(b'/')?;
                    remaining -= 1;
                    remaining -= self
                        .bytes()
                        .new_alphabet(Some(alphanumeric_alphabet()))
                        .at_most(16.min(remaining))
                        .write(writer)?;
                }
            }

            ValidKind::HyperlinkStart => {
                writer.writeAll(b"8;")?;
                if self.rand.boolean() {
                    writer.writeAll(b"id=")?;
                    self.bytes()
                        .new_alphabet(Some(alphanumeric_alphabet()))
                        .at_most(16)
                        .format(writer)?;
                }
                writer.writeAll(b";https://localhost")?;
                for _ in 0..self.rand.int_range_at_most::<usize>(2, 5) {
                    writer.writeAll(b"/")?;
                    self.bytes()
                        .new_alphabet(Some(alphanumeric_alphabet()))
                        .at_most(16)
                        .format(writer)?;
                }
            }

            ValidKind::HyperlinkEnd => 'hyperlink_end: {
                if max_len < 3 {
                    break 'hyperlink_end;
                }
                writer.writeAll(b"8;;")?;
            }

            ValidKind::ConemuProgress => {
                writer.writeAll(b"9;")?;
                match self.rand.int_range_at_most::<U3>(U3(0), U3(4)).0 {
                    c @ (0 | 3) => {
                        writer.writeAll(b";")?;
                        writer.writeUint(c)?;
                    }
                    c @ (1 | 2 | 4) => {
                        if self.rand.boolean() {
                            writer.writeAll(b";")?;
                            writer.writeUint(c)?;
                        } else {
                            let d = self.rand.int_range_at_most::<u8>(0, 100);
                            writer.writeAll(b";")?;
                            writer.writeUint(c)?;
                            writer.writeAll(b";")?;
                            writer.writeUint(d as u64)?;
                        }
                    }
                    _ => unreachable!(),
                }
            }

            ValidKind::Iterm2Notification => 'iterm2_notification: {
                if max_len < 3 {
                    break 'iterm2_notification;
                }
                // add a prefix to ensure that this is not interpreted as a ConEmu OSC
                writer.writeAll(b"9;_")?;
                self.bytes()
                    .new_alphabet(Some(ascii_alphabet()))
                    .at_most(max_len - 3)
                    .format(writer)?;
            }
        }
        Ok(())
    }

    fn next_unwrapped_invalid_exact<W: Writer<Error = WriteFailed>>(
        &self,
        writer: &mut W,
        k: InvalidKind,
        max_len: usize,
    ) -> Result<(), WriteFailed> {
        match k {
            InvalidKind::Random => {
                self.bytes().at_most(max_len).format(writer)?;
            }

            InvalidKind::GoodPrefix => {
                writer.writeAll(b"133;")?;
                self.bytes().at_most(max_len - 4).format(writer)?;
            }
        }
        Ok(())
    }

    fn bytes(&self) -> Bytes<'a> {
        Bytes {
            rand: self.rand,
            alphabet: Some(osc_alphabet()),
            min_len: 1,
            max_len: usize::MAX,
        }
    }

    /// Choose whether to generate a valid or invalid OSC request based
    /// on the validity probability.
    fn choose_validity(&self) -> Validity {
        if self.rand.float_f64() > self.p_valid {
            Validity::Invalid
        } else {
            Validity::Valid
        }
    }
}

/// A fixed seed we can use for our tests to avoid flakes.
#[cfg(test)]
const TEST_SEED: u64 = 0xC0FFEEEEEEEEEEEE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::FixedWriter;
    use crate::rand::DefaultPrng;

    #[test]
    fn osc_generator() {
        let prng = DefaultPrng::init(TEST_SEED);
        let mut buf = [0u8; 256];
        {
            let mut v = Osc::new(prng.random());
            for _ in 0..50 {
                // NOTE: the `Generator<W>` binds the writer type (and its
                // lifetime) at construction, so it is rebuilt per iteration to
                // pair with the fresh fixed writer; it re-wraps the same
                // prng-backed `Osc`, so the byte stream is identical.
                let gen = v.generator();
                let mut writer = FixedWriter::fixed(&mut buf);
                gen.next(&mut writer, 256).unwrap();
                let result = writer.buffered();
                assert!(!result.is_empty());
            }
        }
    }

    #[test]
    fn osc_generator_valid() {
        let prng = DefaultPrng::init(TEST_SEED);
        let mut buf = [0u8; 256];
        let mut gen = Osc {
            rand: prng.random(),
            p_valid: 1.0,
            p_valid_kind: EnumArray::init_fill(1.0),
            p_invalid_kind: EnumArray::init_fill(1.0),
        };
        for _ in 0..50 {
            let mut writer = FixedWriter::fixed(&mut buf);
            gen.next(&mut writer, 256).unwrap();
            let seq = writer.buffered();
            let mut parser = rustty_vt::osc::Parser::init(None);
            for &c in &seq[2..seq.len() - 1] {
                parser.next(c);
            }
            assert!(parser.end(None).is_some());
        }
    }

    #[test]
    fn osc_generator_invalid() {
        let prng = DefaultPrng::init(TEST_SEED);
        let mut buf = [0u8; 256];
        let mut gen = Osc {
            rand: prng.random(),
            p_valid: 0.0,
            p_valid_kind: EnumArray::init_fill(1.0),
            p_invalid_kind: EnumArray::init_fill(1.0),
        };
        for _ in 0..50 {
            let mut writer = FixedWriter::fixed(&mut buf);
            gen.next(&mut writer, 256).unwrap();
            let seq = writer.buffered();
            let mut parser = rustty_vt::osc::Parser::init(None);
            for &c in &seq[2..seq.len() - 1] {
                parser.next(c);
            }
            assert!(parser.end(None).is_none());
        }
    }
}
