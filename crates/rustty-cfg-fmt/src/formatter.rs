//! rustty-cfg-fmt::formatter
//!
//! PORTS: ghostty src/config/formatter.zig
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/config/formatter.zig` — every line, comment, and
//! `test {}` is ported here (R1). Retargeted only for product identity (R2).
//
// const formatter = @This();
//
// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const Key = @import("key.zig").Key;
//
// PORT NOTE: the reference imports `Key` from `key.zig`, but `Key` is named ONLY
// on the import line — it is DEAD code in this module (the config-value
// EntryFormatter never references it; `Key` lives with `Config` later). It is
// therefore intentionally NOT pulled in, keeping this crate a near-leaf with no
// dependency on config/Key (R3/R4).
//
// PORT NOTE: the reference is built on Zig comptime metaprogramming —
// `formatEntry(comptime T, name, value, writer)` reflects over an arbitrary value
// type `T`, dispatching on `@typeInfo(T)` at comptime. Rust has no comptime type
// reflection, so — exactly as the `rustty-args` `Target`/`tmux/output.rs`
// `FormatStruct` ports do — the faithful equivalent threads the SAME per-type
// information dynamically through a trait. Each value type implements
// `FormatEntry`, whose `format_entry` performs the per-type dispatch the
// reference does with `switch (@typeInfo(T))`, calling back to the standalone
// primitive helpers (`format_entry_bool`, `format_entry_int`, ...) so each arm is
// assertion-for-assertion identical to the reference. The "structs/unions require
// a formatEntry decl" cases are exactly types that implement `FormatEntry`
// themselves (the `@hasDecl(T, "formatEntry")` branches); the packed-struct
// special case is exposed via `format_entry_packed_struct`.

use crate::writer::Writer;

/// PORTS: the inferred error union of the reference's `formatEntry` (`!void`) —
/// the writer's error set plus `error{OutOfMemory}`.
///
/// Several value-type `formatEntry` impls format through FIXED stack buffers
/// (`Path`/`RepeatablePath` `[std.fs.max_path_bytes + 1]u8`, `ReadableIO`
/// `[4096]u8`, `RepeatableStringMap` `[256]u8`, ...) and map the buffer's
/// `error.NoSpaceLeft` to `return error.OutOfMemory`. Zig's inferred error set
/// carries that alongside the writer error; Rust spells the union as this enum.
/// The `From<E>` impl lets plain writer errors propagate with `?` unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError<E> {
    /// The underlying writer failed (`std.Io.Writer.Error`).
    Writer(E),
    /// `error.OutOfMemory` — a value-type's fixed formatting buffer overflowed
    /// (the reference's `error.NoSpaceLeft => return error.OutOfMemory` arms).
    OutOfMemory,
}

impl<E> From<E> for FormatError<E> {
    fn from(e: E) -> FormatError<E> {
        FormatError::Writer(e)
    }
}

/// Returns a single entry formatter for the given field name and writer.
// pub fn entryFormatter(
//     name: []const u8,
//     writer: *std.Io.Writer,
// ) EntryFormatter {
//     return .{ .name = name, .writer = writer };
// }
pub fn entry_formatter<'a, W: Writer>(name: &'a [u8], writer: &'a mut W) -> EntryFormatter<'a, W> {
    EntryFormatter { name, writer }
}

/// The entry formatter type for a given writer.
// pub const EntryFormatter = struct {
//     name: []const u8,
//     writer: *std.Io.Writer,
pub struct EntryFormatter<'a, W: Writer> {
    pub name: &'a [u8],
    pub writer: &'a mut W,
}

impl<W: Writer> EntryFormatter<'_, W> {
    // pub fn formatEntry(
    //     self: @This(),
    //     comptime T: type,
    //     value: T,
    // ) !void {
    //     return formatter.formatEntry(
    //         T,
    //         self.name,
    //         value,
    //         self.writer,
    //     );
    // }
    //
    // PORT NOTE: `comptime T` + `value: T` -> the generic `value: &V where V:
    // FormatEntry`. The reference dispatches to the free `formatter.formatEntry`;
    // here we call the value's own `FormatEntry::format_entry`, which performs the
    // same `@typeInfo(T)` dispatch.
    pub fn format_entry<V: FormatEntry>(&mut self, value: &V) -> Result<(), FormatError<W::Error>> {
        value.format_entry(self.name, self.writer)
    }
}

/// PORTS: the comptime value type `T` of `formatEntry`.
///
/// The reference reflects over `T` at comptime with `switch (@typeInfo(T))`;
/// here each value type implements this trait to thread the same dispatch
/// dynamically. `format_entry` performs, for the concrete type, exactly the arm
/// the reference's `switch` would select, using the standalone `format_entry_*`
/// helpers below for the primitive cases.
///
/// Structs/unions in the reference require a `formatEntry` decl (the
/// `@hasDecl(T, "formatEntry")` branches) which is given the `EntryFormatter` so
/// they can call BACK to write each primitive value — that is precisely a type
/// implementing this trait whose `format_entry` calls
/// `EntryFormatter::format_entry` on its own fields.
pub trait FormatEntry {
    /// Format a single value with the given `name`, writing to `writer`. This is
    /// the per-type body of the reference's
    /// `formatEntry(comptime T, name, value, writer)`.
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>>;
}

// ---------------------------------------------------------------------------
// Standalone "magic by type" helpers reproducing the `switch (@typeInfo(T))`
// arms of the reference's `formatEntry`. Each `FormatEntry` implementor calls
// the helper matching its `@typeInfo` arm so every arm is assertion-for-
// assertion identical to the reference.
//
// Format a single type with the given name and value.
// pub fn formatEntry(
//     comptime T: type,
//     name: []const u8,
//     value: T,
//     writer: *std.Io.Writer,
// ) !void {
//     switch (@typeInfo(T)) {
// ---------------------------------------------------------------------------

/// `.bool, .int => try writer.print("{s} = {}\n", .{ name, value });`
//
// PORT NOTE: the reference groups `.bool` and `.int` into one arm that prints
// the value with the default `{}` formatter. Booleans print as `true`/`false`
// and integers as their decimal form; `Display` reproduces both, so this helper
// serves both the `.bool` and `.int` arms (callers use it for either).
pub fn format_entry_display<W: Writer, V: core::fmt::Display>(
    name: &[u8],
    value: &V,
    writer: &mut W,
) -> Result<(), FormatError<W::Error>> {
    // try writer.print("{s} = {}\n", .{ name, value });
    // return;
    writer.write_all(name)?;
    writer.write_all(b" = ")?;
    writer.write_all(format!("{value}").as_bytes())?;
    Ok(writer.write_all(b"\n")?)
}

/// The float primitives the `.float` arm accepts (`f32`/`f64` config fields).
/// `Display` supplies the shortest round-trip decimal; the two query methods
/// let `format_entry_float` reproduce Zig `{d}`'s special-case spellings
/// (see the RUST CONTRACT note there).
pub trait FloatEntry: core::fmt::Display + Copy {
    fn is_nan(self) -> bool;
    fn is_sign_negative(self) -> bool;
}
impl FloatEntry for f32 {
    fn is_nan(self) -> bool {
        f32::is_nan(self)
    }
    fn is_sign_negative(self) -> bool {
        f32::is_sign_negative(self)
    }
}
impl FloatEntry for f64 {
    fn is_nan(self) -> bool {
        f64::is_nan(self)
    }
    fn is_sign_negative(self) -> bool {
        f64::is_sign_negative(self)
    }
}

/// `.float => try writer.print("{s} = {d}\n", .{ name, value });`
//
// PORT NOTE: the reference uses the `{d}` formatter for floats, which prints the
// shortest round-trippable decimal (e.g. `0.7`). Rust's default float `Display`
// produces the same shortest decimal, so `{value}` reproduces `{d}` — EXCEPT for
// NaN (RUST CONTRACT, VALIDATION_REPORT §3d): Zig `{d}` spells it `nan` with the
// sign bit preserved (`-nan`), while Rust `Display` prints `NaN` and always drops
// the sign. The NaN branch below restores the Zig spelling byte-for-byte
// (verified against zig 0.15.2: `nan`/`-nan`/`inf`/`-inf`; `inf` spellings
// already agree).
pub fn format_entry_float<W: Writer, V: FloatEntry>(
    name: &[u8],
    value: &V,
    writer: &mut W,
) -> Result<(), FormatError<W::Error>> {
    // try writer.print("{s} = {d}\n", .{ name, value });
    // return;
    writer.write_all(name)?;
    writer.write_all(b" = ")?;
    if value.is_nan() {
        writer.write_all(if value.is_sign_negative() {
            b"-nan"
        } else {
            b"nan"
        })?;
    } else {
        writer.write_all(format!("{value}").as_bytes())?;
    }
    Ok(writer.write_all(b"\n")?)
}

/// `.@"enum" => try writer.print("{s} = {t}\n", .{ name, value });`
//
// PORT NOTE: the `{t}` formatter prints an enum's active tag name. Rust has no
// comptime tag-name reflection, so the enum value provides its tag name as bytes
// (`tag_name`), exactly the string `{t}` would emit.
pub fn format_entry_enum<W: Writer>(
    name: &[u8],
    tag_name: &[u8],
    writer: &mut W,
) -> Result<(), FormatError<W::Error>> {
    // try writer.print("{s} = {t}\n", .{ name, value });
    // return;
    writer.write_all(name)?;
    writer.write_all(b" = ")?;
    writer.write_all(tag_name)?;
    Ok(writer.write_all(b"\n")?)
}

/// `.void => try writer.print("{s} = \n", .{name});`
pub fn format_entry_void<W: Writer>(
    name: &[u8],
    writer: &mut W,
) -> Result<(), FormatError<W::Error>> {
    // try writer.print("{s} = \n", .{name});
    // return;
    writer.write_all(name)?;
    Ok(writer.write_all(b" = \n")?)
}

/// `.optional => |info| { if (value) |inner| formatEntry(info.child, ...) else
/// writer.print("{s} = \n", .{name}); }`
//
// PORT NOTE: the optional arm recurses into the child type for `Some`, and
// prints the bare `name = \n` line for `None`. The generic `inner: &V where V:
// FormatEntry` reproduces `formatEntry(info.child, name, inner, writer)`.
pub fn format_entry_optional<W: Writer, V: FormatEntry>(
    name: &[u8],
    value: Option<&V>,
    writer: &mut W,
) -> Result<(), FormatError<W::Error>> {
    // if (value) |inner| {
    if let Some(inner) = value {
        // try formatEntry(
        //     info.child,
        //     name,
        //     inner,
        //     writer,
        // );
        inner.format_entry(name, writer)?;
    } else {
        // try writer.print("{s} = \n", .{name});
        writer.write_all(name)?;
        writer.write_all(b" = \n")?;
    }

    // return;
    Ok(())
}

/// `.pointer => switch (T) { []const u8, [:0]const u8 => try writer.print("{s} =
/// {s}\n", .{ name, value }); else => {} }`
//
// PORT NOTE: the reference's pointer arm only handles the two string slice types
// (`[]const u8`, `[:0]const u8`), printing the bytes verbatim; both map to a
// byte slice here (the sentinel of `[:0]` is not part of the observable bytes).
pub fn format_entry_string<W: Writer>(
    name: &[u8],
    value: &[u8],
    writer: &mut W,
) -> Result<(), FormatError<W::Error>> {
    // try writer.print("{s} = {s}\n", .{ name, value });
    // return;
    writer.write_all(name)?;
    writer.write_all(b" = ")?;
    writer.write_all(value)?;
    Ok(writer.write_all(b"\n")?)
}

// Structs of all types require a "formatEntry" function
// to be defined which will be called to format the value.
// This is given the formatter in use so that they can
// call BACK to our formatEntry to write each primitive
// value.
//
// .@"struct" => |info| if (@hasDecl(T, "formatEntry")) {
//     try value.formatEntry(entryFormatter(name, writer));
//     return;
// } else switch (info.layout) {
//
// PORT NOTE: the `@hasDecl(T, "formatEntry")` branch (for both structs and
// unions) is exactly a type implementing `FormatEntry` and delegating to its
// own fields via `entry_formatter`. There is no standalone helper for it: such a
// type's `FormatEntry::format_entry` body calls `entry_formatter(name, writer)`
// then `format_entry` on each field, reproducing `value.formatEntry(entryFormatter(...))`.

// Packed structs we special case.
//
// .@"packed" => {
//     try writer.print("{s} = ", .{name});
//     inline for (info.fields, 0..) |field, i| {
//         if (i > 0) try writer.print(",", .{});
//         try writer.print("{s}{s}", .{
//             if (!@field(value, field.name)) "no-" else "",
//             field.name,
//         });
//     }
//     try writer.print("\n", .{});
//     return;
// },
//
// PORT NOTE: the comptime `inline for (info.fields)` over a packed (all-`bool`)
// struct becomes a runtime loop over the type's `(field_name, value)` pairs,
// which the implementor supplies in declaration order. Each field prints
// `no-`-prefixed when false, comma-separated, exactly as the reference.
pub fn format_entry_packed_struct<W: Writer>(
    name: &[u8],
    fields: &[(&[u8], bool)],
    writer: &mut W,
) -> Result<(), FormatError<W::Error>> {
    // try writer.print("{s} = ", .{name});
    writer.write_all(name)?;
    writer.write_all(b" = ")?;
    // inline for (info.fields, 0..) |field, i| {
    for (i, (field_name, field_value)) in fields.iter().enumerate() {
        // if (i > 0) try writer.print(",", .{});
        if i > 0 {
            writer.write_all(b",")?;
        }
        // try writer.print("{s}{s}", .{
        //     if (!@field(value, field.name)) "no-" else "",
        //     field.name,
        // });
        if !*field_value {
            writer.write_all(b"no-")?;
        }
        writer.write_all(field_name)?;
    }
    // try writer.print("\n", .{});
    Ok(writer.write_all(b"\n")?)
    // return;
}

// .@"union" => if (@hasDecl(T, "formatEntry")) {
//     try value.formatEntry(entryFormatter(name, writer));
//     return;
// },
//
// PORT NOTE: identical to the struct `@hasDecl` branch — a union type with a
// `formatEntry` decl is a type implementing `FormatEntry` that delegates to its
// active variant via `entry_formatter`.

// else => {},
//
// // Compile error so that we can catch missing cases.
// @compileLog(T);
// @compileError("missing case for type");
//
// PORT NOTE: the trailing `@compileLog`/`@compileError` is the reference's
// comptime guard that every supported type matches a `switch` arm. In the Rust
// port this guard is the type system itself: a type can only be formatted if it
// implements `FormatEntry`, and its `format_entry` must call one of the helpers
// above (or delegate via `entry_formatter`). A type with no valid case simply
// cannot implement the trait meaningfully, which is the compile-time error.

#[cfg(test)]
mod tests {
    // PORT NOTE: the standalone `format_entry_*` helpers are referenced through
    // `super::` (rather than glob-imported) so a test fn may carry the canonical
    // snake_case slug of its reference test name (e.g. `format_entry_float`)
    // without shadowing the like-named helper it calls.
    use super::{FormatEntry, FormatError, Writer};
    use crate::writer::AllocatingWriter;

    // PORT NOTE: the optional test formats `?bool`; `bool` implements
    // `FormatEntry` via the `.bool` arm (`format_entry_display`), so the optional
    // helper recurses into it for `Some` and prints the bare line for `None`.
    // This impl is hoisted to the test-module level (rather than nested in the
    // test fn) to keep it a local trait impl.
    impl FormatEntry for bool {
        fn format_entry<W: Writer>(
            &self,
            name: &[u8],
            writer: &mut W,
        ) -> Result<(), FormatError<W::Error>> {
            super::format_entry_display(name, self, writer)
        }
    }

    // test "formatEntry bool"
    #[test]
    fn format_entry_bool() {
        {
            // var buf: std.Io.Writer.Allocating = .init(testing.allocator);
            // defer buf.deinit();
            let mut buf = AllocatingWriter::init();
            // try formatEntry(bool, "a", true, &buf.writer);
            super::format_entry_display(b"a", &true, &mut buf).unwrap();
            // try testing.expectEqualStrings("a = true\n", buf.written());
            assert_eq!(buf.written(), b"a = true\n");
        }

        {
            let mut buf = AllocatingWriter::init();
            // try formatEntry(bool, "a", false, &buf.writer);
            super::format_entry_display(b"a", &false, &mut buf).unwrap();
            // try testing.expectEqualStrings("a = false\n", buf.written());
            assert_eq!(buf.written(), b"a = false\n");
        }
    }

    // test "formatEntry int"
    #[test]
    fn format_entry_int() {
        {
            let mut buf = AllocatingWriter::init();
            // try formatEntry(u8, "a", 123, &buf.writer);
            super::format_entry_display(b"a", &123u8, &mut buf).unwrap();
            // try testing.expectEqualStrings("a = 123\n", buf.written());
            assert_eq!(buf.written(), b"a = 123\n");
        }
    }

    // test "formatEntry float"
    #[test]
    fn format_entry_float() {
        {
            let mut buf = AllocatingWriter::init();
            // try formatEntry(f64, "a", 0.7, &buf.writer);
            super::format_entry_float(b"a", &0.7f64, &mut buf).unwrap();
            // try testing.expectEqualStrings("a = 0.7\n", buf.written());
            assert_eq!(buf.written(), b"a = 0.7\n");
        }
    }

    // rustty-only test (VALIDATION_REPORT §3d): Zig `{d}` spells NaN `nan`
    // (sign-preserving: `-nan`); Rust `Display` prints `NaN` and drops the
    // sign. Lock the restored Zig spellings, for f64 and f32.
    #[test]
    fn format_entry_float_nan_matches_zig_spelling() {
        {
            let mut buf = AllocatingWriter::init();
            super::format_entry_float(b"a", &f64::NAN, &mut buf).unwrap();
            assert_eq!(buf.written(), b"a = nan\n");
        }
        {
            let mut buf = AllocatingWriter::init();
            super::format_entry_float(b"a", &(-f64::NAN), &mut buf).unwrap();
            assert_eq!(buf.written(), b"a = -nan\n");
        }
        {
            let mut buf = AllocatingWriter::init();
            super::format_entry_float(b"a", &f32::NAN, &mut buf).unwrap();
            assert_eq!(buf.written(), b"a = nan\n");
        }
        // inf spellings already agree between Zig `{d}` and Rust `Display`.
        {
            let mut buf = AllocatingWriter::init();
            super::format_entry_float(b"a", &f64::NEG_INFINITY, &mut buf).unwrap();
            assert_eq!(buf.written(), b"a = -inf\n");
        }
    }

    // test "formatEntry enum"
    #[test]
    fn format_entry_enum() {
        // const Enum = enum { one, two, three };
        #[allow(dead_code)]
        enum Enum {
            One,
            Two,
            Three,
        }
        impl Enum {
            // `{t}` prints the active tag name.
            fn tag_name(&self) -> &'static [u8] {
                match self {
                    Enum::One => b"one",
                    Enum::Two => b"two",
                    Enum::Three => b"three",
                }
            }
        }

        {
            let mut buf = AllocatingWriter::init();
            // try formatEntry(Enum, "a", .two, &buf.writer);
            super::format_entry_enum(b"a", Enum::Two.tag_name(), &mut buf).unwrap();
            // try testing.expectEqualStrings("a = two\n", buf.written());
            assert_eq!(buf.written(), b"a = two\n");
        }
    }

    // test "formatEntry void"
    #[test]
    fn format_entry_void() {
        {
            let mut buf = AllocatingWriter::init();
            // try formatEntry(void, "a", {}, &buf.writer);
            super::format_entry_void(b"a", &mut buf).unwrap();
            // try testing.expectEqualStrings("a = \n", buf.written());
            assert_eq!(buf.written(), b"a = \n");
        }
    }

    // test "formatEntry optional"
    #[test]
    fn format_entry_optional() {
        {
            let mut buf = AllocatingWriter::init();
            // try formatEntry(?bool, "a", null, &buf.writer);
            super::format_entry_optional::<_, bool>(b"a", None, &mut buf).unwrap();
            // try testing.expectEqualStrings("a = \n", buf.written());
            assert_eq!(buf.written(), b"a = \n");
        }

        {
            let mut buf = AllocatingWriter::init();
            // try formatEntry(?bool, "a", false, &buf.writer);
            super::format_entry_optional(b"a", Some(&false), &mut buf).unwrap();
            // try testing.expectEqualStrings("a = false\n", buf.written());
            assert_eq!(buf.written(), b"a = false\n");
        }
    }

    // test "formatEntry string"
    #[test]
    fn format_entry_string() {
        {
            let mut buf = AllocatingWriter::init();
            // try formatEntry([]const u8, "a", "hello", &buf.writer);
            super::format_entry_string(b"a", b"hello", &mut buf).unwrap();
            // try testing.expectEqualStrings("a = hello\n", buf.written());
            assert_eq!(buf.written(), b"a = hello\n");
        }
    }

    // test "formatEntry packed struct"
    #[test]
    fn format_entry_packed_struct() {
        // const Value = packed struct {
        //     one: bool = true,
        //     two: bool = false,
        // };
        struct Value {
            one: bool,
            two: bool,
        }
        impl Default for Value {
            fn default() -> Value {
                Value {
                    one: true,
                    two: false,
                }
            }
        }

        {
            let mut buf = AllocatingWriter::init();
            // try formatEntry(Value, "a", .{}, &buf.writer);
            let value = Value::default();
            super::format_entry_packed_struct(
                b"a",
                &[(b"one", value.one), (b"two", value.two)],
                &mut buf,
            )
            .unwrap();
            // try testing.expectEqualStrings("a = one,no-two\n", buf.written());
            assert_eq!(buf.written(), b"a = one,no-two\n");
        }
    }
}
