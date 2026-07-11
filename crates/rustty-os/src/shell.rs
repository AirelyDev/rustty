//! rustty-os::shell
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/shell.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the Zig reference imports `std`, `testing`, `Allocator`, and
// `Writer = std.Io.Writer`. The writer interface is the crate-local `Writer`
// trait (defined in `string_encoding`, mirroring the `std.Io.Writer`
// `writeByte`/`writeAll` surface used here). The `Allocating` writer is modeled
// by `ShellCommandBuilder`'s owned buffer; `std.Io.Writer.fixed`/`buffered` are
// `string_encoding::FixedWriter`.
#![allow(non_snake_case)]

use crate::string_encoding::Writer;

/// Builder for constructing space-separated shell command strings.
/// Uses a caller-provided allocator (typically with stackFallback).
//
// PORT NOTE: `buffer: std.Io.Writer.Allocating` is modeled by an owned `Vec<u8>`.
// The reference's `init(allocator)` takes the allocator; in Rust allocation is
// implicit, so `init()` takes no allocator argument.
pub struct ShellCommandBuilder {
    buffer: Vec<u8>,
}

impl ShellCommandBuilder {
    pub fn init() -> ShellCommandBuilder {
        ShellCommandBuilder { buffer: Vec::new() }
    }

    pub fn deinit(self) {
        // self.buffer.deinit();
    }

    /// PORT NOTE: `self.buffer.written()` — the bytes accumulated so far.
    pub fn written(&self) -> &[u8] {
        &self.buffer
    }

    /// Append an argument to the command with automatic space separation.
    pub fn appendArg(&mut self, arg: &[u8]) {
        // PORT NOTE: the reference returns `(Allocator.Error || Writer.Error)!void`;
        // the `Vec`/allocating writer here cannot fail in this environment.
        if arg.is_empty() {
            return;
        }
        if !self.buffer.is_empty() {
            self.buffer.push(b' ');
        }
        self.buffer.extend_from_slice(arg);
    }

    /// Get the final null-terminated command string, transferring ownership to caller.
    /// Calling deinit() after this is safe but unnecessary.
    //
    // PORT NOTE: `toOwnedSliceSentinel(0)` returns a `[:0]const u8` (NUL-terminated,
    // length excludes the NUL). Modeled as an owned `Vec<u8>` whose final byte is
    // the NUL terminator; the logical command length is `len() - 1`.
    pub fn toOwnedSlice(mut self) -> Vec<u8> {
        self.buffer.push(0);
        self.buffer
    }
}

/// Writer that escapes characters that shells treat specially to reduce the
/// risk of injection attacks or other such weirdness. Specifically excludes
/// linefeeds so that they can be used to delineate lists of file paths.
//
// PORT NOTE: the reference embeds a `std.Io.Writer` (`writer`) whose vtable `drain`
// dispatches to `writeEscaped`, and holds a `child: *Writer`. We model
// `ShellEscapeWriter` as a `Writer` whose `writeAll`/`writeByte` perform the
// escaping (the `drain`/`writeEscaped` logic) and forward to `child`. The
// reference's `.writer` field is reachable via the `Writer` trait impl below
// (i.e. `shell.writer.writeAll(..)` becomes `shell.writeAll(..)`).
pub struct ShellEscapeWriter<'c, C: Writer> {
    child: &'c mut C,
    // PORT NOTE: `count: usize` tracks the number of source bytes consumed (the
    // reference's `drain` return value). It is maintained for fidelity.
    count: usize,
}

impl<'c, C: Writer> ShellEscapeWriter<'c, C> {
    pub fn init(child: &'c mut C) -> ShellEscapeWriter<'c, C> {
        ShellEscapeWriter {
            // TODO: Actually use a buffer here
            child,
            count: 0,
        }
    }

    // PORT NOTE: the reference's `drain(w, data, splat)` iterates the data chunks
    // (last one repeated `splat` times) and escapes each. Our `Writer::writeAll`
    // receives a single contiguous chunk, so it forwards directly to
    // `writeEscaped`. The fidelity-preserving body and comments are kept:
    //
    // TODO: This is a very naive implementation and does not really make
    // full use of the post-Writergate API. However, since we know that
    // this is going into an Allocating writer anyways, we can be a bit
    // less strict here.

    fn writeEscaped(&mut self, s: &[u8]) -> Result<(), C::Error> {
        for &byte in s {
            let buf: &[u8] = match byte {
                b'\\' | b'"' | b'\'' | b'$' | b'`' | b'*' | b'?' | b' ' | b'|' | b'(' | b')' => {
                    &[b'\\', byte]
                }
                _ => std::slice::from_ref(&byte),
            };
            self.child.writeAll(buf)?;
            self.count += 1;
        }
        Ok(())
    }
}

impl<C: Writer> Writer for ShellEscapeWriter<'_, C> {
    type Error = C::Error;
    fn writeByte(&mut self, byte: u8) -> Result<(), C::Error> {
        self.writeEscaped(std::slice::from_ref(&byte))
    }
    fn writeAll(&mut self, bytes: &[u8]) -> Result<(), C::Error> {
        self.writeEscaped(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::string_encoding::FixedWriter;

    #[test]
    #[allow(clippy::octal_escapes)]
    fn shell_command_builder() {
        // Empty command
        {
            let cmd = ShellCommandBuilder::init();
            assert_eq!(b"".as_slice(), cmd.written());
            cmd.deinit();
        }

        // Single arg
        {
            let mut cmd = ShellCommandBuilder::init();
            cmd.appendArg(b"bash");
            assert_eq!(b"bash".as_slice(), cmd.written());
            cmd.deinit();
        }

        // Multiple args
        {
            let mut cmd = ShellCommandBuilder::init();
            cmd.appendArg(b"bash");
            cmd.appendArg(b"--posix");
            cmd.appendArg(b"-l");
            assert_eq!(b"bash --posix -l".as_slice(), cmd.written());
            cmd.deinit();
        }

        // Empty arg
        {
            let mut cmd = ShellCommandBuilder::init();
            cmd.appendArg(b"bash");
            cmd.appendArg(b"");
            assert_eq!(b"bash".as_slice(), cmd.written());
            cmd.deinit();
        }

        // toOwnedSlice
        {
            let mut cmd = ShellCommandBuilder::init();
            cmd.appendArg(b"bash");
            cmd.appendArg(b"--posix");
            let result = cmd.toOwnedSlice();
            // result is NUL-terminated; the logical string excludes the NUL.
            assert_eq!(b"bash --posix".as_slice(), &result[0..result.len() - 1]);
            assert_eq!(0u8, result[result.len() - 1]);
        }
    }

    #[test]
    fn shell_escape_1() {
        let mut buf = [0u8; 128];
        let mut writer = FixedWriter::fixed(&mut buf);
        {
            let mut shell = ShellEscapeWriter::init(&mut writer);
            shell.writeAll(b"abc").unwrap();
        }
        assert_eq!(b"abc".as_slice(), writer.buffered());
    }

    #[test]
    fn shell_escape_2() {
        let mut buf = [0u8; 128];
        let mut writer = FixedWriter::fixed(&mut buf);
        {
            let mut shell = ShellEscapeWriter::init(&mut writer);
            shell.writeAll(b"a c").unwrap();
        }
        assert_eq!(b"a\\ c".as_slice(), writer.buffered());
    }

    #[test]
    fn shell_escape_3() {
        let mut buf = [0u8; 128];
        let mut writer = FixedWriter::fixed(&mut buf);
        {
            let mut shell = ShellEscapeWriter::init(&mut writer);
            shell.writeAll(b"a?c").unwrap();
        }
        assert_eq!(b"a\\?c".as_slice(), writer.buffered());
    }

    #[test]
    fn shell_escape_4() {
        let mut buf = [0u8; 128];
        let mut writer = FixedWriter::fixed(&mut buf);
        {
            let mut shell = ShellEscapeWriter::init(&mut writer);
            shell.writeAll(b"a\\c").unwrap();
        }
        assert_eq!(b"a\\\\c".as_slice(), writer.buffered());
    }

    #[test]
    fn shell_escape_5() {
        let mut buf = [0u8; 128];
        let mut writer = FixedWriter::fixed(&mut buf);
        {
            let mut shell = ShellEscapeWriter::init(&mut writer);
            shell.writeAll(b"a|c").unwrap();
        }
        assert_eq!(b"a\\|c".as_slice(), writer.buffered());
    }

    #[test]
    fn shell_escape_6() {
        let mut buf = [0u8; 128];
        let mut writer = FixedWriter::fixed(&mut buf);
        {
            let mut shell = ShellEscapeWriter::init(&mut writer);
            shell.writeAll(b"a\"c").unwrap();
        }
        assert_eq!(b"a\\\"c".as_slice(), writer.buffered());
    }

    #[test]
    fn shell_escape_7() {
        let mut buf = [0u8; 128];
        let mut writer = FixedWriter::fixed(&mut buf);
        {
            let mut shell = ShellEscapeWriter::init(&mut writer);
            shell.writeAll(b"a(1)").unwrap();
        }
        assert_eq!(b"a\\(1\\)".as_slice(), writer.buffered());
    }
}
