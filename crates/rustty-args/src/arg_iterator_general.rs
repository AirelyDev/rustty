//! PORTS: Zig 0.16 `std.process.Args.IteratorGeneral(.{})` — the "general"
//! iterator that splits one command-line string into arguments, honouring
//! double quotes and backslash escapes.
//!
//! The reference's benchmark and synthetic CLIs both parse their `Args.string`
//! arm with it (`benchmark/cli.zig:99-106`, `synthetic/cli.zig:80-87`), so it
//! lives here, below both `rustty-bench` and `rustty-synthetic` (parity S62-F1,
//! S121-F2).
//
// PORT NOTE: only the `options = .{}` instantiation is ported (`comments =
// false`, `single_quotes = false`), the one both callers use. The two comptime
// branches that `.{}` never takes are kept as comments at their sites. The
// reference allocates a decode `buffer` of `cmd_line.len + 1` bytes and `next`
// returns a `[:0]const u8` slice of it; here `buffer` is an owned `Vec<u8>` of
// the same size and `next` returns a borrow into it (the sentinel byte is still
// written). `std.process.Args.IteratorGeneral`'s `skip` and `initTakeOwnership`
// have no caller in the port. rustty-config's `command.rs` carries its own
// private copy of this state machine for `Command.shell`; it is left in place.

use crate::args::ArgIterator;
use crate::diagnostics::LocationProvider;

/// A general iterator to parse a string into a set of arguments.
// pub fn IteratorGeneral(comptime options: IteratorGeneralOptions) type {
//     return struct {
pub struct ArgIteratorGeneral<'a> {
    // index: usize = 0,
    index: usize,
    // cmd_line: []const u8,
    cmd_line: &'a [u8],
    /// buffer MUST be long enough to hold the cmd_line plus a null terminator.
    // buffer: []u8,
    buffer: Vec<u8>,
    // start: usize = 0,
    start: usize,
    // end: usize = 0,
    end: usize,
}

impl<'a> ArgIteratorGeneral<'a> {
    /// cmd_line_utf8 MUST remain valid and constant while using this instance
    // pub fn init(allocator: Allocator, cmd_line_utf8: []const u8) InitError!Self {
    //     const buffer = try allocator.alloc(u8, cmd_line_utf8.len + 1);
    pub fn init(cmd_line_utf8: &'a [u8]) -> ArgIteratorGeneral<'a> {
        ArgIteratorGeneral {
            index: 0,
            cmd_line: cmd_line_utf8,
            buffer: vec![0u8; cmd_line_utf8.len() + 1],
            start: 0,
            end: 0,
        }
    }

    // Skips over whitespace in the cmd_line.
    // Returns false if the terminating sentinel is reached, true otherwise.
    // Also skips over comments (if supported).
    // fn skipWhitespace(self: *Self) bool {
    fn skip_whitespace(&mut self) -> bool {
        // while (true) : (self.index += 1) {
        loop {
            // const character = if (self.index != self.cmd_line.len) self.cmd_line[self.index] else 0;
            let character = if self.index != self.cmd_line.len() {
                self.cmd_line[self.index]
            } else {
                0
            };
            match character {
                // 0 => return false,
                0 => return false,
                // ' ', '\t', '\r', '\n' => continue,
                b' ' | b'\t' | b'\r' | b'\n' => {}
                // '#' => { if (options.comments) { ... } else { break; } },
                // (`options.comments` is false for `.{}`: the break arm.)
                // else => break,
                _ => break,
            }
            self.index += 1;
        }
        // return true;
        true
    }

    /// Returns a slice of the internal buffer that contains the next argument.
    /// Returns null when it reaches the end.
    // pub fn next(self: *Self) ?[:0]const u8 {
    pub fn next(&mut self) -> Option<&[u8]> {
        // if (!self.skipWhitespace()) {
        //     return null;
        // }
        if !self.skip_whitespace() {
            return None;
        }

        // var backslash_count: usize = 0;
        let mut backslash_count: usize = 0;
        // var in_quote = false;
        let mut in_quote = false;
        // while (true) : (self.index += 1) {
        loop {
            // const character = if (self.index != self.cmd_line.len) self.cmd_line[self.index] else 0;
            let character = if self.index != self.cmd_line.len() {
                self.cmd_line[self.index]
            } else {
                0
            };
            match character {
                // 0 => {
                0 => {
                    // self.emitBackslashes(backslash_count);
                    self.emit_backslashes(backslash_count);
                    // self.buffer[self.end] = 0;
                    self.buffer[self.end] = 0;
                    // const token = self.buffer[self.start..self.end :0];
                    let (start, end) = (self.start, self.end);
                    // self.end += 1;
                    self.end += 1;
                    // self.start = self.end;
                    self.start = self.end;
                    // return token;
                    return Some(&self.buffer[start..end]);
                }
                // '"', '\'' => {
                b'"' | b'\'' => {
                    // if (!options.single_quotes and character == '\'') {
                    //     self.emitBackslashes(backslash_count);
                    //     backslash_count = 0;
                    //     self.emitCharacter(character);
                    //     continue;
                    // }
                    if character == b'\'' {
                        self.emit_backslashes(backslash_count);
                        backslash_count = 0;
                        self.emit_character(character);
                    } else {
                        // const quote_is_real = backslash_count % 2 == 0;
                        let quote_is_real = backslash_count.is_multiple_of(2);
                        // self.emitBackslashes(backslash_count / 2);
                        self.emit_backslashes(backslash_count / 2);
                        // backslash_count = 0;
                        backslash_count = 0;

                        // if (quote_is_real) {
                        //     in_quote = !in_quote;
                        // } else {
                        //     self.emitCharacter('"');
                        // }
                        if quote_is_real {
                            in_quote = !in_quote;
                        } else {
                            self.emit_character(b'"');
                        }
                    }
                }
                // '\\' => {
                //     backslash_count += 1;
                // },
                b'\\' => backslash_count += 1,
                // ' ', '\t', '\r', '\n' => {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    // self.emitBackslashes(backslash_count);
                    self.emit_backslashes(backslash_count);
                    // backslash_count = 0;
                    backslash_count = 0;
                    // if (in_quote) {
                    //     self.emitCharacter(character);
                    // } else {
                    if in_quote {
                        self.emit_character(character);
                    } else {
                        // self.buffer[self.end] = 0;
                        self.buffer[self.end] = 0;
                        // const token = self.buffer[self.start..self.end :0];
                        let (start, end) = (self.start, self.end);
                        // self.end += 1;
                        self.end += 1;
                        // self.start = self.end;
                        self.start = self.end;
                        // return token;
                        //
                        // (the `: (self.index += 1)` continue expression does not
                        // run on `return`; the next call's skipWhitespace steps
                        // over this whitespace byte.)
                        return Some(&self.buffer[start..end]);
                    }
                }
                // else => {
                _ => {
                    // self.emitBackslashes(backslash_count);
                    self.emit_backslashes(backslash_count);
                    // backslash_count = 0;
                    backslash_count = 0;
                    // self.emitCharacter(character);
                    self.emit_character(character);
                }
            }
            // : (self.index += 1)
            self.index += 1;
        }
    }

    // fn emitBackslashes(self: *Self, emit_count: usize) void {
    fn emit_backslashes(&mut self, emit_count: usize) {
        // var i: usize = 0;
        // while (i < emit_count) : (i += 1) {
        //     self.emitCharacter('\\');
        // }
        for _ in 0..emit_count {
            self.emit_character(b'\\');
        }
    }

    // fn emitCharacter(self: *Self, char: u8) void {
    fn emit_character(&mut self, char: u8) {
        // self.buffer[self.end] = char;
        self.buffer[self.end] = char;
        // self.end += 1;
        self.end += 1;
    }

    /// Call to free the internal buffer of the iterator.
    // pub fn deinit(self: *Self) void {
    //     self.allocator.free(self.buffer);
    //     if (self.free_cmd_line_on_deinit) { ... }
    // }
    //
    // PORT NOTE: the buffer is an owned `Vec` freed on drop; kept so callers can
    // mirror the reference's `defer iter.deinit()`.
    pub fn deinit(&mut self) {}
}

// `cli.args.parse(Options, alloc, &opts, &iter)` duck-types `iter.next()`. The
// trait's `Item` cannot borrow the iterator, so each argument is copied out.
impl ArgIterator for ArgIteratorGeneral<'_> {
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Vec<u8>> {
        ArgIteratorGeneral::next(self).map(<[u8]>::to_vec)
    }
}

// IteratorGeneral has no `location` decl => `Location.fromIter` yields `.none`.
impl LocationProvider for ArgIteratorGeneral<'_> {}

#[cfg(test)]
mod tests {
    use super::ArgIteratorGeneral;

    // fn testGeneralCmdLine(input_cmd_line: []const u8, expected_args: []const []const u8) !void {
    //     var it = try IteratorGeneral(.{}).init(std.testing.allocator, input_cmd_line);
    //     defer it.deinit();
    //     for (expected_args) |expected_arg| {
    //         const arg = it.next().?;
    //         try testing.expectEqualStrings(expected_arg, arg);
    //     }
    //     try testing.expect(it.next() == null);
    // }
    fn test_general_cmd_line(input_cmd_line: &[u8], expected_args: &[&[u8]]) {
        let mut it = ArgIteratorGeneral::init(input_cmd_line);
        for expected_arg in expected_args {
            let arg = it.next().unwrap();
            assert_eq!(*expected_arg, arg);
        }
        assert!(it.next().is_none());
        it.deinit();
    }

    // Zig 0.16 `lib/std/process/Args.zig`: test "general parsing", verbatim.
    #[test]
    fn general_parsing() {
        test_general_cmd_line(b"a   b\tc d", &[b"a", b"b", b"c", b"d"]);
        test_general_cmd_line(b"\"abc\" d e", &[b"abc", b"d", b"e"]);
        test_general_cmd_line(b"a\\\\\\b d\"e f\"g h", &[b"a\\\\\\b", b"de fg", b"h"]);
        test_general_cmd_line(b"a\\\\\\\"b c d", &[b"a\\\"b", b"c", b"d"]);
        test_general_cmd_line(b"a\\\\\\\\\"b c\" d e", &[b"a\\\\b c", b"d", b"e"]);
        test_general_cmd_line(b"a   b\tc \"d f", &[b"a", b"b", b"c", b"d f"]);
        test_general_cmd_line(b"j k l\\", &[b"j", b"k", b"l\\"]);
        test_general_cmd_line(b"\"\" x y z\\\\", &[b"", b"x", b"y", b"z\\\\"]);

        test_general_cmd_line(
            b"\".\\..\\zig-cache\\build\" \"bin\\zig.exe\" \".\\..\" \".\\..\\zig-cache\" \"--help\"",
            &[
                b".\\..\\zig-cache\\build",
                b"bin\\zig.exe",
                b".\\..",
                b".\\..\\zig-cache",
                b"--help",
            ],
        );

        // \\ 'foo' "bar"
        test_general_cmd_line(b" 'foo' \"bar\"", &[b"'foo'", b"bar"]);
    }
}
