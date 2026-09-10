//! rustty-config::command
//!
//! PORTS: ghostty src/config/command.zig — the config `Command` value-type
//! (distinct from input/command). C1 leaf value-type (10 tests).
//!
//! PORT STATUS: ported (every line/comment/test{}, R1).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const formatterpkg = @import("formatter.zig");
use rustty_cfg_fmt::{FormatEntry, FormatError, Writer};

/// A command to execute (argv0 and args).
///
/// A command is specified as a simple string such as "nvim a b c".
/// By default, we expect the downstream to do some sort of shell expansion
/// on this string.
///
/// If a command is already expanded and the user does NOT want to do
/// shell expansion (because this usually requires a round trip into
/// /bin/sh or equivalent), specify a `direct:`-prefix. e.g.
/// `direct:nvim a b c`.
///
/// The whitespace before or around the prefix is ignored. For example,
/// `  direct:nvim a b c` and `direct: nvim a b c` are equivalent.
///
/// If the command is not absolute, it'll be looked up via the PATH.
/// For the shell-expansion case, we let the shell do this. For the
/// direct case, we do this directly.
// pub const Command = union(enum) {
//     const Self = @This();
//
// PORT NOTE (Rust-only glue, additive): `Clone` — Zig copies the union by
// value (shallow slice copies into config memory); the owned rustty payloads
// clone the bytes at the reference's copy sites (Surface.init copies it into
// the termio Exec config AND keeps it for the title block — Surface.zig:618/
// 649/755/769; the core app.rs `performAllChainedAction` clone precedent).
//
// PORT NOTE (Rust-only glue, additive; N11): `Debug` — `Config.zig:4746`'s
// `log.info("shell src=config value={}", .{cmd})` applies Zig's `{}` to a value
// with no `format` method, i.e. the builtin aggregate formatter
// (`config.Command{ .shell = { 47, 98, ... } }`, byte slices rendered as their
// numeric elements). Rust's derived `Debug` is the port's crate-wide stand-in
// for that formatter (`Shell([47, 98, ...])`); the spelling difference is
// tracked as N11 and swept crate-wide there, not here.
//
// PUBLIC-SURFACE JUDGMENT (task-14d-2, asked and answered rather than left
// open): this does NOT widen the surface in a way that matters.
//   * It is semver-additive. Adding a trait impl to a local type cannot break a
//     downstream build here: `Command` has no inherent `fmt`, so there is no
//     method-resolution collision, and nothing depends on `Command: !Debug`.
//   * It changes no layout, alignment or discriminant. `Command` carries no
//     `repr` attribute and crosses no C-ABI boundary; nothing in
//     `crates-sys`/`rustty-*-c` names it.
//   * The one new CAPABILITY it grants is printing a user's command line into a
//     diagnostic — which is exactly what the reference does at
//     `Config.zig:4746`, the call site this derive exists to serve. It exposes
//     nothing the oracle keeps private.
//   * It matches the crate's existing additive-glue convention: the `Clone`
//     immediately below is the same kind of Rust-only derive with the same kind
//     of PORT NOTE.
#[derive(Clone, Debug)]
pub enum Command {
    /// Execute a command directly, e.g. via `exec`. The format here
    /// is already structured to be ready to passed directly to `exec`
    /// with index zero being the command to execute.
    ///
    /// Index zero is not guaranteed to be an absolute path, and may require
    /// PATH lookup. It is up to the downstream to do this, usually via
    /// delegation to something like `execvp`.
    // direct: []const [:0]const u8,
    //
    // PORT NOTE: `[]const [:0]const u8` (slice of null-terminated byte slices) ->
    // `Vec<Vec<u8>>` (raw bytes, never lossy `String`).
    Direct(Vec<Vec<u8>>),

    /// Execute a command via shell expansion. This provides the command
    /// as a single string that is expected to be expanded in some way
    /// (up to the downstream). Usually `/bin/sh -c`.
    // shell: [:0]const u8,
    Shell(Vec<u8>),
}

/// PORTS: the error set of `parseCLI` — `error.ValueRequired || Allocator.Error`.
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    ValueRequired,
}

impl Command {
    // pub fn parseCLI(
    //     self: *Self,
    //     alloc: Allocator,
    //     input_: ?[]const u8,
    // ) !void {
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // Input is required. Whitespace on the edges isn't needed.
        // Commands must be non-empty.
        // const input = input_ orelse return error.ValueRequired;
        let input = input_.ok_or(ParseError::ValueRequired)?;
        // const trimmed = std.mem.trim(u8, input, " ");
        let trimmed = trim_spaces(input);
        // if (trimmed.len == 0) return error.ValueRequired;
        if trimmed.is_empty() {
            return Err(ParseError::ValueRequired);
        }

        // If we have a `:` then we MIGHT have a prefix to specify what
        // tag we should use.
        // const tag: std.meta.Tag(Self), const str: []const u8 = tag: {
        //     if (std.mem.indexOfScalar(u8, trimmed, ':')) |idx| {
        //         const prefix = trimmed[0..idx];
        //         if (std.mem.eql(u8, prefix, "direct")) {
        //             break :tag .{ .direct, trimmed[idx + 1 ..] };
        //         } else if (std.mem.eql(u8, prefix, "shell")) {
        //             break :tag .{ .shell, trimmed[idx + 1 ..] };
        //         }
        //     }
        //
        //     break :tag .{ .shell, trimmed };
        // };
        let (tag, str): (Tag, &[u8]) = 'tag: {
            if let Some(idx) = index_of_scalar(trimmed, b':') {
                let prefix = &trimmed[0..idx];
                if prefix == b"direct" {
                    break 'tag (Tag::Direct, &trimmed[idx + 1..]);
                } else if prefix == b"shell" {
                    break 'tag (Tag::Shell, &trimmed[idx + 1..]);
                }
            }

            (Tag::Shell, trimmed)
        };

        // switch (tag) {
        match tag {
            // .shell => {
            Tag::Shell => {
                // We have a shell command, so we can just dupe it.
                // const copy = try alloc.dupeZ(u8, std.mem.trim(u8, str, " "));
                let copy = trim_spaces(str).to_vec();
                // self.* = .{ .shell = copy };
                *self = Command::Shell(copy);
            }

            // .direct => {
            Tag::Direct => {
                // We're not shell expanding, so the arguments are naively
                // split on spaces.
                // var builder: std.ArrayListUnmanaged([:0]const u8) = .empty;
                let mut builder: Vec<Vec<u8>> = Vec::new();
                // var args = std.mem.splitScalar(u8, std.mem.trim(u8, str, " "), ' ');
                // while (args.next()) |arg| {
                //     const copy = try alloc.dupeZ(u8, arg);
                //     try builder.append(alloc, copy);
                // }
                for arg in trim_spaces(str).split(|&b| b == b' ') {
                    let copy = arg.to_vec();
                    builder.push(copy);
                }

                // self.* = .{ .direct = try builder.toOwnedSlice(alloc) };
                *self = Command::Direct(builder);
            }
        }

        Ok(())
    }

    /// Creates a command as a single string, joining arguments as
    /// necessary with spaces. Its not guaranteed that this is a valid
    /// command; it is only meant to be human readable.
    // pub fn string(
    //     self: *const Self,
    //     alloc: Allocator,
    // ) Allocator.Error![:0]const u8 {
    //     return switch (self.*) {
    //         .shell => |v| try alloc.dupeZ(u8, v),
    //         .direct => |v| try std.mem.joinZ(alloc, " ", v),
    //     };
    // }
    pub fn string(&self) -> Vec<u8> {
        match self {
            // .shell => |v| try alloc.dupeZ(u8, v),
            Command::Shell(v) => v.clone(),
            // .direct => |v| try std.mem.joinZ(alloc, " ", v),
            Command::Direct(v) => v.join(&b' '),
        }
    }

    /// Get an iterator over the arguments array. This may allocate
    /// depending on the active tag of the command.
    ///
    /// For direct commands, this is very cheap and just iterates over
    /// the array. There is no allocation.
    ///
    /// For shell commands, this will use Zig's ArgIteratorGeneral as
    /// a best effort shell string parser. This is not guaranteed to be
    /// 100% accurate, but it works for common cases. This requires allocation.
    // pub fn argIterator(
    //     self: *const Self,
    //     alloc: Allocator,
    // ) Allocator.Error!ArgIterator {
    //     return switch (self.*) {
    //         .direct => |v| .{ .direct = .{ .args = v } },
    //         .shell => |v| .{ .shell = try .init(alloc, v) },
    //     };
    // }
    pub fn arg_iterator(&self) -> ArgIterator<'_> {
        match self {
            // .direct => |v| .{ .direct = .{ .args = v } },
            Command::Direct(v) => ArgIterator::Direct(DirectIterator { i: 0, args: v }),
            // .shell => |v| .{ .shell = try .init(alloc, v) },
            Command::Shell(v) => ArgIterator::Shell(ArgIteratorGeneral::init(v)),
        }
    }

    // pub fn clone(
    //     self: *const Self,
    //     alloc: Allocator,
    // ) Allocator.Error!Self {
    //     return switch (self.*) {
    //         .shell => |v| .{ .shell = try alloc.dupeZ(u8, v) },
    //         .direct => |v| direct: {
    //             const copy = try alloc.alloc([:0]const u8, v.len);
    //             for (v, 0..) |arg, i| copy[i] = try alloc.dupeZ(u8, arg);
    //             break :direct .{ .direct = copy };
    //         },
    //     };
    // }
    //
    // PARITY: named `clone` to match the reference method (Config relies on it).
    #[allow(clippy::should_implement_trait)]
    pub fn clone(&self) -> Command {
        match self {
            // .shell => |v| .{ .shell = try alloc.dupeZ(u8, v) },
            Command::Shell(v) => Command::Shell(v.clone()),
            // .direct => |v| direct: { ... },
            Command::Direct(v) => {
                let copy = v.to_vec();
                Command::Direct(copy)
            }
        }
    }

    // pub fn deinit(self: *const Self, alloc: Allocator) void {
    //     switch (self.*) {
    //         .shell => |v| alloc.free(v),
    //         .direct => |l| {
    //             for (l) |v| alloc.free(v);
    //             alloc.free(l);
    //         },
    //     }
    // }
    //
    // PORT NOTE: explicit `deinit` is `Drop` in Rust (the `Vec`s free themselves).
    // Preserved as a clear for call-site parity; dropping the value suffices.
    pub fn deinit(&mut self) {
        match self {
            Command::Shell(v) => v.clear(),
            Command::Direct(l) => l.clear(),
        }
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void {
    //
    // PORT NOTE: implemented via the `FormatEntry` trait (the reference's
    // `@hasDecl(T, "formatEntry")` union branch). See the trait impl below.
}

/// PORTS: `std.meta.Tag(Command)` — the tag of the `Command` union, used by
/// `parseCLI` to decide how to construct the value.
enum Tag {
    Direct,
    Shell,
}

impl FormatEntry for Command {
    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // switch (self) {
        match self {
            // .shell => |v| try formatter.formatEntry([]const u8, v),
            Command::Shell(v) => StringField(v).format_entry(name, writer),

            // .direct => |v| {
            Command::Direct(v) => {
                // var buf: [4096]u8 = undefined;
                // var writer: std.Io.Writer = .fixed(&buf);
                // writer.writeAll("direct:") catch return error.OutOfMemory;
                // for (v) |arg| {
                //     writer.writeAll(arg) catch return error.OutOfMemory;
                //     writer.writeByte(' ') catch return error.OutOfMemory;
                // }
                //
                // const written = writer.buffered();
                // try formatter.formatEntry(
                //     []const u8,
                //     written[0..@intCast(written.len - 1)],
                // );
                //
                // PORT NOTE: the reference writes into a 4096-byte fixed buffer
                // then trims the trailing space (`written.len - 1`). We build the
                // joined bytes the same way (each arg + a space), then drop the
                // final space, and pass the result through the string-slice arm.
                let mut buf: Vec<u8> = Vec::new();
                buf.extend_from_slice(b"direct:");
                for arg in v {
                    buf.extend_from_slice(arg);
                    buf.push(b' ');
                }

                let written = &buf[0..buf.len() - 1];
                StringField(written).format_entry(name, writer)
            }
        }
    }
}

/// PORTS: the `[]const u8`/`[:0]const u8` value passed to
/// `formatter.formatEntry` — routes through the string-slice arm of the
/// reference formatter.
struct StringField<'a>(&'a [u8]);

impl FormatEntry for StringField<'_> {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        rustty_cfg_fmt::format_entry_string(name, self.0, writer)
    }
}

/// Iterates over each argument in the command.
// pub const ArgIterator = union(enum) {
pub enum ArgIterator<'a> {
    // shell: std.process.ArgIteratorGeneral(.{}),
    Shell(ArgIteratorGeneral),
    // direct: struct {
    //     i: usize = 0,
    //     args: []const [:0]const u8,
    // },
    Direct(DirectIterator<'a>),
}

pub struct DirectIterator<'a> {
    // i: usize = 0,
    i: usize,
    // args: []const [:0]const u8,
    args: &'a [Vec<u8>],
}

impl ArgIterator<'_> {
    /// Return the next argument. This may or may not be a copy
    /// depending on the active tag. If you want to ensure that every
    /// argument is a copy, use the `clone` method first.
    // pub fn next(self: *ArgIterator) ?[:0]const u8 {
    //     return switch (self.*) {
    //         .shell => |*v| v.next(),
    //         .direct => |*v| {
    //             if (v.i >= v.args.len) return null;
    //             defer v.i += 1;
    //             return v.args[v.i];
    //         },
    //     };
    // }
    //
    // PORT NOTE: the reference returns a `[:0]const u8` that, for the shell arm,
    // borrows the iterator's internal decode buffer; for the direct arm, borrows
    // the source args. Both are byte slices; here `next` returns `Option<&[u8]>`.
    // The shell-arm token borrows `self` (the ArgIteratorGeneral's buffer).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&[u8]> {
        match self {
            // .shell => |*v| v.next(),
            ArgIterator::Shell(v) => v.next(),
            // .direct => |*v| { ... }
            ArgIterator::Direct(v) => {
                // if (v.i >= v.args.len) return null;
                if v.i >= v.args.len() {
                    return None;
                }
                // defer v.i += 1;
                let idx = v.i;
                v.i += 1;
                // return v.args[v.i];
                Some(&v.args[idx])
            }
        }
    }

    // pub fn deinit(self: *ArgIterator) void {
    //     switch (self.*) {
    //         .shell => |*v| v.deinit(),
    //         .direct => {},
    //     }
    // }
    //
    // PORT NOTE: `deinit` frees the shell iterator's internal buffer; in Rust that
    // is the `ArgIteratorGeneral`'s owned `Vec` freed on drop. Preserved as a
    // no-op-shaped method for call-site parity.
    pub fn deinit(&mut self) {
        match self {
            ArgIterator::Shell(_v) => {}
            ArgIterator::Direct(_) => {}
        }
    }
}

/// PORTS: `std.process.ArgIteratorGeneral(.{})` — a best-effort shell-string
/// argument tokenizer.
//
// PORT NOTE: faithful port of Zig 0.15's `std.process.ArgIteratorGeneral` with
// `options = .{}` (comments:false, single_quotes:false), the exact options the
// reference `Command.shell` uses. The original allocates a decode `buffer` of
// `cmd_line.len + 1`; here `buffer` is an owned `Vec<u8>` of that size and `next`
// returns a borrow into it. `cmd_line` is owned so the iterator is self-contained
// (the reference borrows it for the iterator's lifetime; owning it avoids a
// lifetime entanglement with the `Command`).
pub struct ArgIteratorGeneral {
    // index: usize = 0,
    index: usize,
    // cmd_line: []const u8,
    cmd_line: Vec<u8>,
    // buffer: []u8,
    buffer: Vec<u8>,
    // start: usize = 0,
    start: usize,
    // end: usize = 0,
    end: usize,
}

impl ArgIteratorGeneral {
    /// cmd_line_utf8 MUST remain valid and constant while using this instance
    // pub fn init(allocator: Allocator, cmd_line_utf8: []const u8) InitError!Self {
    //     const buffer = try allocator.alloc(u8, cmd_line_utf8.len + 1);
    //     errdefer allocator.free(buffer);
    //
    //     return Self{
    //         .allocator = allocator,
    //         .cmd_line = cmd_line_utf8,
    //         .free_cmd_line_on_deinit = false,
    //         .buffer = buffer,
    //     };
    // }
    fn init(cmd_line_utf8: &[u8]) -> ArgIteratorGeneral {
        // const buffer = try allocator.alloc(u8, cmd_line_utf8.len + 1);
        let buffer = vec![0u8; cmd_line_utf8.len() + 1];
        ArgIteratorGeneral {
            index: 0,
            cmd_line: cmd_line_utf8.to_vec(),
            buffer,
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
            // switch (character) {
            match character {
                // 0 => return false,
                0 => return false,
                // ' ', '\t', '\r', '\n' => continue,
                b' ' | b'\t' | b'\r' | b'\n' => {
                    self.index += 1;
                    continue;
                }
                // '#' => {
                //     if (options.comments) { ... } else { break; }
                // },
                //
                // PORT NOTE: `options.comments` is `false` for `.{}`, so the
                // comptime branch always takes the `else => break` arm. The
                // comments-enabled body is dead for this instantiation and is
                // preserved as a comment for fidelity:
                //     while (true) : (self.index += 1) {
                //         switch (self.cmd_line[self.index]) {
                //             '\n' => break,
                //             0 => return false,
                //             else => continue,
                //         }
                //     }
                //     continue;
                b'#' => break,
                // else => break,
                _ => break,
            }
        }
        // return true;
        true
    }

    /// Returns a slice of the internal buffer that contains the next argument.
    /// Returns null when it reaches the end.
    // pub fn next(self: *Self) ?[:0]const u8 {
    fn next(&mut self) -> Option<&[u8]> {
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
            // switch (character) {
            match character {
                // 0 => {
                0 => {
                    // self.emitBackslashes(backslash_count);
                    self.emit_backslashes(backslash_count);
                    // self.buffer[self.end] = 0;
                    self.buffer[self.end] = 0;
                    // const token = self.buffer[self.start..self.end :0];
                    let start = self.start;
                    let end = self.end;
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
                    //
                    // PORT NOTE: `options.single_quotes` is `false`, so a `'`
                    // is emitted literally (treated as a normal character).
                    if character == b'\'' {
                        self.emit_backslashes(backslash_count);
                        backslash_count = 0;
                        self.emit_character(character);
                        self.index += 1;
                        continue;
                    }
                    // const quote_is_real = backslash_count % 2 == 0;
                    let quote_is_real = backslash_count.is_multiple_of(2);
                    // self.emitBackslashes(backslash_count / 2);
                    self.emit_backslashes(backslash_count / 2);
                    // backslash_count = 0;
                    backslash_count = 0;

                    // if (quote_is_real) {
                    if quote_is_real {
                        // in_quote = !in_quote;
                        in_quote = !in_quote;
                    } else {
                        // self.emitCharacter('"');
                        self.emit_character(b'"');
                    }
                }
                // '\\' => {
                //     backslash_count += 1;
                // },
                b'\\' => {
                    backslash_count += 1;
                }
                // ' ', '\t', '\r', '\n' => {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    // self.emitBackslashes(backslash_count);
                    self.emit_backslashes(backslash_count);
                    // backslash_count = 0;
                    backslash_count = 0;
                    // if (in_quote) {
                    if in_quote {
                        // self.emitCharacter(character);
                        self.emit_character(character);
                    } else {
                        // self.buffer[self.end] = 0;
                        self.buffer[self.end] = 0;
                        // const token = self.buffer[self.start..self.end :0];
                        let start = self.start;
                        let end = self.end;
                        // self.end += 1;
                        self.end += 1;
                        // self.start = self.end;
                        self.start = self.end;
                        // return token;
                        self.index += 1;
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
    //     var i: usize = 0;
    //     while (i < emit_count) : (i += 1) {
    //         self.emitCharacter('\\');
    //     }
    // }
    fn emit_backslashes(&mut self, emit_count: usize) {
        let mut i: usize = 0;
        while i < emit_count {
            self.emit_character(b'\\');
            i += 1;
        }
    }

    // fn emitCharacter(self: *Self, char: u8) void {
    //     self.buffer[self.end] = char;
    //     self.end += 1;
    // }
    fn emit_character(&mut self, char: u8) {
        self.buffer[self.end] = char;
        self.end += 1;
    }
}

/// PORTS: `std.mem.trim(u8, s, " ")` — trims only ASCII space (0x20) from edges.
fn trim_spaces(s: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = s.len();
    while start < end && s[start] == b' ' {
        start += 1;
    }
    while end > start && s[end - 1] == b' ' {
        end -= 1;
    }
    &s[start..end]
}

/// PORTS: `std.mem.indexOfScalar(u8, slice, value)`.
fn index_of_scalar(slice: &[u8], value: u8) -> Option<usize> {
    slice.iter().position(|&b| b == value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_cfg_fmt::{entry_formatter, AllocatingWriter};

    // test "Command: parseCLI errors" {
    #[test]
    fn command_parse_cli_errors() {
        // var v: Self = undefined;
        let mut v = Command::Shell(Vec::new());
        // try testing.expectError(error.ValueRequired, v.parseCLI(alloc, null));
        assert_eq!(Err(ParseError::ValueRequired), v.parse_cli(None));
        // try testing.expectError(error.ValueRequired, v.parseCLI(alloc, ""));
        assert_eq!(Err(ParseError::ValueRequired), v.parse_cli(Some(b"")));
        // try testing.expectError(error.ValueRequired, v.parseCLI(alloc, " "));
        assert_eq!(Err(ParseError::ValueRequired), v.parse_cli(Some(b" ")));
    }

    // test "Command: parseCLI shell expanded" {
    #[test]
    fn command_parse_cli_shell_expanded() {
        // var v: Self = undefined;
        let mut v = Command::Shell(Vec::new());
        // try v.parseCLI(alloc, "echo hello");
        v.parse_cli(Some(b"echo hello")).unwrap();
        // try testing.expect(v == .shell);
        // try testing.expectEqualStrings(v.shell, "echo hello");
        match &v {
            Command::Shell(s) => assert_eq!(s.as_slice(), b"echo hello"),
            _ => panic!("expected shell"),
        }

        // Spaces are stripped
        // try v.parseCLI(alloc, " echo hello ");
        v.parse_cli(Some(b" echo hello ")).unwrap();
        // try testing.expect(v == .shell);
        // try testing.expectEqualStrings(v.shell, "echo hello");
        match &v {
            Command::Shell(s) => assert_eq!(s.as_slice(), b"echo hello"),
            _ => panic!("expected shell"),
        }
    }

    // test "Command: parseCLI direct" {
    #[test]
    fn command_parse_cli_direct() {
        // var v: Self = undefined;
        let mut v = Command::Shell(Vec::new());
        // try v.parseCLI(alloc, "direct:echo hello");
        v.parse_cli(Some(b"direct:echo hello")).unwrap();
        // try testing.expect(v == .direct);
        // try testing.expectEqual(v.direct.len, 2);
        // try testing.expectEqualStrings(v.direct[0], "echo");
        // try testing.expectEqualStrings(v.direct[1], "hello");
        match &v {
            Command::Direct(d) => {
                assert_eq!(d.len(), 2);
                assert_eq!(d[0].as_slice(), b"echo");
                assert_eq!(d[1].as_slice(), b"hello");
            }
            _ => panic!("expected direct"),
        }

        // Spaces around the prefix
        // try v.parseCLI(alloc, " direct:  echo hello");
        v.parse_cli(Some(b" direct:  echo hello")).unwrap();
        // try testing.expect(v == .direct);
        // try testing.expectEqual(v.direct.len, 2);
        // try testing.expectEqualStrings(v.direct[0], "echo");
        // try testing.expectEqualStrings(v.direct[1], "hello");
        match &v {
            Command::Direct(d) => {
                assert_eq!(d.len(), 2);
                assert_eq!(d[0].as_slice(), b"echo");
                assert_eq!(d[1].as_slice(), b"hello");
            }
            _ => panic!("expected direct"),
        }
    }

    // test "Command: argIterator shell" {
    #[test]
    fn command_arg_iterator_shell() {
        // var v: Self = .{ .shell = "echo hello world" };
        let v = Command::Shell(b"echo hello world".to_vec());
        // var it = try v.argIterator(alloc);
        let mut it = v.arg_iterator();
        // defer it.deinit();

        // try testing.expectEqualStrings(it.next().?, "echo");
        assert_eq!(it.next().unwrap(), b"echo");
        // try testing.expectEqualStrings(it.next().?, "hello");
        assert_eq!(it.next().unwrap(), b"hello");
        // try testing.expectEqualStrings(it.next().?, "world");
        assert_eq!(it.next().unwrap(), b"world");
        // try testing.expect(it.next() == null);
        assert!(it.next().is_none());
        it.deinit();
    }

    // test "Command: argIterator direct" {
    #[test]
    fn command_arg_iterator_direct() {
        // var v: Self = .{ .direct = &.{ "echo", "hello world" } };
        let v = Command::Direct(vec![b"echo".to_vec(), b"hello world".to_vec()]);
        // var it = try v.argIterator(alloc);
        let mut it = v.arg_iterator();
        // defer it.deinit();

        // try testing.expectEqualStrings(it.next().?, "echo");
        assert_eq!(it.next().unwrap(), b"echo");
        // try testing.expectEqualStrings(it.next().?, "hello world");
        assert_eq!(it.next().unwrap(), b"hello world");
        // try testing.expect(it.next() == null);
        assert!(it.next().is_none());
        it.deinit();
    }

    // test "Command: string shell" {
    #[test]
    fn command_string_shell() {
        // var v: Self = .{ .shell = "echo hello world" };
        let v = Command::Shell(b"echo hello world".to_vec());
        // const str = try v.string(alloc);
        let str = v.string();
        // defer alloc.free(str);
        // try testing.expectEqualStrings(str, "echo hello world");
        assert_eq!(str.as_slice(), b"echo hello world");
    }

    // test "Command: string direct" {
    #[test]
    fn command_string_direct() {
        // var v: Self = .{ .direct = &.{ "echo", "hello world" } };
        let v = Command::Direct(vec![b"echo".to_vec(), b"hello world".to_vec()]);
        // const str = try v.string(alloc);
        let str = v.string();
        // defer alloc.free(str);
        // try testing.expectEqualStrings(str, "echo hello world");
        assert_eq!(str.as_slice(), b"echo hello world");
    }

    // test "Command: formatConfig shell" {
    #[test]
    fn command_format_config_shell() {
        // var buf: std.Io.Writer.Allocating = .init(alloc);
        let mut buf = AllocatingWriter::init();

        // var v: Self = undefined;
        let mut v = Command::Shell(Vec::new());
        // try v.parseCLI(alloc, "echo hello");
        v.parse_cli(Some(b"echo hello")).unwrap();
        // try v.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&v).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = echo hello\n", buf.written());
        assert_eq!(b"a = echo hello\n".as_slice(), buf.written());
    }

    // test "Command: formatConfig direct" {
    #[test]
    fn command_format_config_direct() {
        // var buf: std.Io.Writer.Allocating = .init(alloc);
        let mut buf = AllocatingWriter::init();

        // var v: Self = undefined;
        let mut v = Command::Shell(Vec::new());
        // try v.parseCLI(alloc, "direct: echo hello");
        v.parse_cli(Some(b"direct: echo hello")).unwrap();
        // try v.formatEntry(formatterpkg.entryFormatter("a", &buf.writer));
        let mut f = entry_formatter(b"a", &mut buf);
        f.format_entry(&v).unwrap();
        // try std.testing.expectEqualSlices(u8, "a = direct:echo hello\n", buf.written());
        assert_eq!(b"a = direct:echo hello\n".as_slice(), buf.written());
    }

    // test {
    //     _ = Command;
    // }
    //
    // PORT NOTE: the reference's bare `test { _ = Command; }` forces analysis of
    // all of `Command`'s decls (its own tests). In Rust the `#[test]` functions
    // above are compiled and run directly, so this reference-analysis test is a
    // no-op here (kept for the test-count contract).
    #[test]
    fn ref_all_decls() {
        let _ = Command::Shell(Vec::new());
    }
}
