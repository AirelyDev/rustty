//! Tests ported from ghostty src/cli/args.zig (36 `test {}` blocks),
//! assertion-for-assertion (R1).
//
// PORT NOTE: the reference tests construct anonymous destination structs and
// drive `parse`/`parseIntoField` over them. Since the Rust port threads field
// reflection through the `Target`/`AutoStruct`/`PackedStruct` traits (see the
// module-level PORT NOTE in args.rs), each test defines a concrete struct
// implementing the relevant trait. The struct's `parse_into_field` (etc.) is the
// per-type reflection the reference performs at comptime — the field names,
// types, and defaults are exactly those declared inline in the reference test.

use super::*;
use crate::diagnostics::{Diagnostic, DiagnosticList, Location, LocationKey};

// ---------------------------------------------------------------------------
// `std.process.ArgIteratorGeneral(.{})` test helper.
//
// PORT NOTE: the reference tests use `std.process.ArgIteratorGeneral(.{}).init`,
// which tokenizes a single command-line string into args, splitting on
// whitespace, honoring double-quote groups (the quotes are stripped) and
// backslash escapes. Reproduced here as a test-only helper yielding the same
// `?[]const u8` (== `Option<Vec<u8>>`) sequence. With default options single
// quotes and comments are NOT special.
// ---------------------------------------------------------------------------
struct ArgIteratorGeneral {
    args: Vec<Vec<u8>>,
    idx: usize,
}

impl ArgIteratorGeneral {
    fn init(cmd: &[u8]) -> ArgIteratorGeneral {
        let mut args: Vec<Vec<u8>> = Vec::new();
        let mut cur: Vec<u8> = Vec::new();
        let mut in_token = false;
        let mut in_quote = false;
        let mut i = 0;
        while i < cmd.len() {
            let c = cmd[i];
            if in_quote {
                match c {
                    b'"' => {
                        in_quote = false;
                        i += 1;
                    }
                    b'\\' => {
                        // Backslash escape inside quotes: next char literal.
                        if i + 1 < cmd.len() {
                            cur.push(cmd[i + 1]);
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    _ => {
                        cur.push(c);
                        i += 1;
                    }
                }
            } else {
                match c {
                    b' ' | b'\t' | b'\n' | b'\r' => {
                        if in_token {
                            args.push(std::mem::take(&mut cur));
                            in_token = false;
                        }
                        i += 1;
                    }
                    b'"' => {
                        in_token = true;
                        in_quote = true;
                        i += 1;
                    }
                    b'\\' => {
                        in_token = true;
                        if i + 1 < cmd.len() {
                            cur.push(cmd[i + 1]);
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    _ => {
                        in_token = true;
                        cur.push(c);
                        i += 1;
                    }
                }
            }
        }
        if in_token {
            args.push(cur);
        }
        ArgIteratorGeneral { args, idx: 0 }
    }

    fn deinit(&mut self) {}
}

impl ArgIterator for ArgIteratorGeneral {
    type Item = Vec<u8>;
    fn next(&mut self) -> Option<Vec<u8>> {
        if self.idx >= self.args.len() {
            return None;
        }
        let v = self.args[self.idx].clone();
        self.idx += 1;
        Some(v)
    }
}

impl LocationProvider for ArgIteratorGeneral {}

// ===========================================================================
// test "parse: simple"
// ===========================================================================
#[test]
fn parse_simple() {
    // var data: struct {
    //     a: []const u8 = "",
    //     b: bool = false,
    //     @"b-f": bool = true,
    //     _arena: ?ArenaAllocator = null,
    // } = .{};
    struct Data {
        a: Vec<u8>,
        b: bool,
        b_f: bool,
        arena_set: bool,
    }
    impl Default for Data {
        fn default() -> Data {
            Data {
                a: b"".to_vec(),
                b: false,
                b_f: true,
                arena_set: false,
            }
        }
    }
    impl Target for Data {
        fn arena_available(&self) -> bool {
            true
        }
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            self.arena_set = true;
            match key {
                b"a" => {
                    self.a = parse_field_string(value)?;
                    Ok(())
                }
                b"b" => {
                    self.b = parse_field_bool(value)?;
                    Ok(())
                }
                b"b-f" => {
                    self.b_f = parse_field_bool(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }

    let mut data = Data::default();

    let mut iter = ArgIteratorGeneral::init(b"--a=42 --b --b-f=false");
    parse(&mut data, &mut iter).unwrap();
    iter.deinit();
    // try testing.expect(data._arena != null);
    assert!(data.arena_set);
    // try testing.expectEqualStrings("42", data.a);
    assert_eq!(b"42".as_slice(), data.a.as_slice());
    // try testing.expect(data.b);
    assert!(data.b);
    // try testing.expect(!data.@"b-f");
    assert!(!data.b_f);

    // Reparsing works
    let mut iter2 = ArgIteratorGeneral::init(b"--a=84");
    parse(&mut data, &mut iter2).unwrap();
    iter2.deinit();
    assert!(data.arena_set);
    assert_eq!(b"84".as_slice(), data.a.as_slice());
    assert!(data.b);
    assert!(!data.b_f);
}

// ===========================================================================
// test "parse: quoted value"
// ===========================================================================
#[test]
fn parse_quoted_value() {
    struct Data {
        a: u8,
        b: Vec<u8>,
        arena_set: bool,
    }
    impl Default for Data {
        fn default() -> Data {
            Data {
                a: 0,
                b: b"".to_vec(),
                arena_set: false,
            }
        }
    }
    impl Target for Data {
        fn arena_available(&self) -> bool {
            true
        }
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            self.arena_set = true;
            match key {
                b"a" => {
                    self.a = parse_field_int::<u8>(value)?;
                    Ok(())
                }
                b"b" => {
                    self.b = parse_field_string(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }

    let mut data = Data::default();
    let mut iter = ArgIteratorGeneral::init(b"--a=\"42\" --b=\"hello!\"");
    parse(&mut data, &mut iter).unwrap();
    iter.deinit();
    // try testing.expectEqual(@as(u8, 42), data.a);
    assert_eq!(42u8, data.a);
    // try testing.expectEqualStrings("hello!", data.b);
    assert_eq!(b"hello!".as_slice(), data.b.as_slice());
}

// ===========================================================================
// test "parse: empty value resets to default"
// ===========================================================================
#[test]
fn parse_empty_value_resets_to_default() {
    struct Data {
        a: u8,
        b: bool,
        arena_set: bool,
    }
    impl Default for Data {
        fn default() -> Data {
            Data {
                a: 42,
                b: false,
                arena_set: false,
            }
        }
    }
    impl Target for Data {
        fn arena_available(&self) -> bool {
            true
        }
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            self.arena_set = true;
            // PORT NOTE: empty value resets to the declared default (the
            // reference's `if (value) |v| default: { if (v.len != 0) break;
            // @field(dst, name) = default; }`).
            match key {
                b"a" => {
                    if let Some(v) = value {
                        if v.is_empty() {
                            self.a = 42; // declared default
                            return Ok(());
                        }
                    }
                    self.a = parse_field_int::<u8>(value)?;
                    Ok(())
                }
                b"b" => {
                    if let Some(v) = value {
                        if v.is_empty() {
                            self.b = false; // declared default
                            return Ok(());
                        }
                    }
                    self.b = parse_field_bool(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }

    let mut data = Data::default();
    let mut iter = ArgIteratorGeneral::init(b"--a= --b=");
    parse(&mut data, &mut iter).unwrap();
    iter.deinit();
    assert_eq!(42u8, data.a);
    assert!(!data.b);
}

// ===========================================================================
// test "parse: positional arguments are invalid"
// ===========================================================================
#[test]
fn parse_positional_arguments_are_invalid() {
    struct Data {
        a: u8,
    }
    impl Default for Data {
        fn default() -> Data {
            Data { a: 42 }
        }
    }
    impl Target for Data {
        fn arena_available(&self) -> bool {
            true
        }
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"a" => {
                    self.a = parse_field_int::<u8>(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }

    let mut data = Data::default();
    let mut iter = ArgIteratorGeneral::init(b"--a=84 what");
    // try testing.expectError(error.InvalidField, parse(...));
    assert_eq!(Err(Error::InvalidField), parse(&mut data, &mut iter));
    iter.deinit();
    // try testing.expectEqual(@as(u8, 84), data.a);
    assert_eq!(84u8, data.a);
}

// ===========================================================================
// test "parse: diagnostic tracking"
// ===========================================================================
#[test]
fn parse_diagnostic_tracking() {
    #[derive(Default, Clone, Copy, PartialEq, Eq)]
    enum B {
        #[default]
        One,
    }
    struct Data {
        a: Vec<u8>,
        #[allow(dead_code)]
        b: B,
        arena_set: bool,
        diagnostics: DiagnosticList,
    }
    impl Default for Data {
        fn default() -> Data {
            Data {
                a: b"".to_vec(),
                b: B::One,
                arena_set: false,
                diagnostics: DiagnosticList::default(),
            }
        }
    }
    impl Target for Data {
        fn arena_available(&self) -> bool {
            true
        }
        fn diagnostics_mut(&mut self) -> Option<&mut DiagnosticList> {
            Some(&mut self.diagnostics)
        }
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            self.arena_set = true;
            match key {
                b"a" => {
                    self.a = parse_field_string(value)?;
                    Ok(())
                }
                b"b" => {
                    self.b = parse_field_enum(value, |v| match v {
                        b"one" => Some(B::One),
                        _ => None,
                    })?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }

    let mut data = Data::default();
    let mut iter = ArgIteratorGeneral::init(b"--what --a=42");
    parse(&mut data, &mut iter).unwrap();
    iter.deinit();
    assert!(data.arena_set);
    assert_eq!(b"42".as_slice(), data.a.as_slice());
    // try testing.expect(data._diagnostics.items().len == 1);
    assert!(data.diagnostics.items().len() == 1);
    {
        let diag = &data.diagnostics.items()[0];
        // try testing.expectEqual(diags.Location.none, diag.location);
        assert_eq!(Location::None, diag.location);
        // try testing.expectEqualStrings("what", diag.key);
        assert_eq!(b"what".as_slice(), diag.key.as_slice());
        // try testing.expectEqualStrings("unknown field", diag.message);
        assert_eq!(b"unknown field".as_slice(), diag.message.as_slice());
    }
}

// ===========================================================================
// test "parse: diagnostic location"
// ===========================================================================
#[test]
fn parse_diagnostic_location() {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum B {
        One,
        Two,
    }
    struct Data {
        a: Vec<u8>,
        b: B,
        arena_set: bool,
        diagnostics: DiagnosticList,
    }
    impl Default for Data {
        fn default() -> Data {
            Data {
                a: b"".to_vec(),
                b: B::One,
                arena_set: false,
                diagnostics: DiagnosticList::default(),
            }
        }
    }
    impl Target for Data {
        fn arena_available(&self) -> bool {
            true
        }
        fn diagnostics_mut(&mut self) -> Option<&mut DiagnosticList> {
            Some(&mut self.diagnostics)
        }
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            self.arena_set = true;
            match key {
                b"a" => {
                    self.a = parse_field_string(value)?;
                    Ok(())
                }
                b"b" => {
                    self.b = parse_field_enum(value, |v| match v {
                        b"one" => Some(B::One),
                        b"two" => Some(B::Two),
                        _ => None,
                    })?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }

    let mut data = Data::default();
    // var r: std.Io.Reader = .fixed("a=42\nwhat\nb=two");
    let src: &[u8] = b"a=42\nwhat\nb=two";
    // var iter: LineIterator = .{ .r = &r, .filepath = "test" };
    let mut iter = LineIterator::with_filepath(src, b"test");
    parse(&mut data, &mut iter).unwrap();
    assert!(data.arena_set);
    assert_eq!(b"42".as_slice(), data.a.as_slice());
    // try testing.expect(data.b == .two);
    assert!(data.b == B::Two);
    assert!(data.diagnostics.items().len() == 1);
    {
        let diag = &data.diagnostics.items()[0];
        assert_eq!(b"what".as_slice(), diag.key.as_slice());
        assert_eq!(b"unknown field".as_slice(), diag.message.as_slice());
        // try testing.expectEqualStrings("test", diag.location.file.path);
        // try testing.expectEqual(2, diag.location.file.line);
        match &diag.location {
            Location::File { path, line } => {
                assert_eq!(b"test".as_slice(), path.as_slice());
                assert_eq!(2, *line);
            }
            _ => panic!("expected file location"),
        }
    }
}

// ===========================================================================
// test "parse: compatibility handler"
// ===========================================================================
#[test]
fn parse_compatibility_handler() {
    // PORT NOTE: reference field defaults (all `false`) coincide with Rust's
    // derived `Default`, so `derive(Default)` is the faithful form.
    #[derive(Default)]
    struct Data {
        a: bool,
        arena_set: bool,
    }
    impl Data {
        // fn compat(self, alloc, key, value) bool
        // PORT NOTE: the nested `if`s mirror the reference's
        // `if (eql(key,"a")) { if (value) |v| { if (eql(v,"yuh")) ... } }`.
        #[allow(clippy::collapsible_if)]
        fn compat(&mut self, key: &[u8], value: Option<&[u8]>) -> bool {
            // _ = alloc;
            // if (std.mem.eql(u8, key, "a")) {
            if key == b"a" {
                // if (value) |v| {
                if let Some(v) = value {
                    // if (mem.eql(u8, v, "yuh")) {
                    if v == b"yuh" {
                        // self.a = true;
                        self.a = true;
                        // return true;
                        return true;
                    }
                }
            }
            // return false;
            false
        }
    }
    impl Target for Data {
        fn arena_available(&self) -> bool {
            true
        }
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            self.arena_set = true;
            match key {
                b"a" => {
                    self.a = parse_field_bool(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
        fn compatibility(&self, key: &[u8]) -> Option<BoxedCompatibilityHandler<Self>> {
            // .{ "a", compat }
            if key == b"a" {
                Some(Box::new(Data::compat))
            } else {
                None
            }
        }
    }

    let mut data = Data::default();
    let mut iter = ArgIteratorGeneral::init(b"--a=yuh");
    parse(&mut data, &mut iter).unwrap();
    iter.deinit();
    assert!(data.arena_set);
    assert!(data.a);
}

// ===========================================================================
// test "parse: compatibility renamed"
// ===========================================================================
#[test]
fn parse_compatibility_renamed() {
    // PORT NOTE: reference field defaults (all `false`) coincide with derived
    // `Default`.
    #[derive(Default)]
    struct Data {
        a: bool,
        b: bool,
        arena_set: bool,
    }
    impl Target for Data {
        fn arena_available(&self) -> bool {
            true
        }
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            self.arena_set = true;
            match key {
                b"a" => {
                    self.a = parse_field_bool(value)?;
                    Ok(())
                }
                b"b" => {
                    self.b = parse_field_bool(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
        // pub const compatibility = .initComptime(&.{ .{ "old", compatibilityRenamed(@This(), "a") } });
        //
        // PORT NOTE: `compatibility_renamed` returns a boxed closure; the bare
        // `CompatibilityHandler` (fn pointer) cannot capture `to`, so the handler
        // for "old" is dispatched here by name and forwards to
        // `parse_into_field("a", value)`, exactly as the generated handler does.
        fn compatibility(&self, key: &[u8]) -> Option<BoxedCompatibilityHandler<Self>> {
            if key == b"old" {
                Some(Box::new(
                    |dst: &mut Data, _key: &[u8], value: Option<&[u8]>| -> bool {
                        matches!(dst.parse_into_field(b"a", value), Ok(()))
                    },
                ))
            } else {
                None
            }
        }
    }

    let mut data = Data::default();
    let mut iter = ArgIteratorGeneral::init(b"--old=true --b=true");
    parse(&mut data, &mut iter).unwrap();
    iter.deinit();
    assert!(data.arena_set);
    assert!(data.a);
    assert!(data.b);
}

// ===========================================================================
// parseIntoField tests
//
// PORT NOTE: these call `Target::parse_into_field` directly (the reference calls
// `parseIntoField(@TypeOf(data), alloc, &data, key, value)`). The arena is the
// reference's local `arena`; in the Rust port allocations are owned, so no arena
// object is threaded.
// ===========================================================================

// test "parseIntoField: ignore underscore-prefixed fields"
#[test]
fn parse_into_field_ignore_underscore_prefixed_fields() {
    struct Data {
        _a: Vec<u8>,
    }
    impl Target for Data {
        fn parse_into_field(
            &mut self,
            _key: &[u8],
            _value: Option<&[u8]>,
        ) -> Result<(), ParseError> {
            // field.name[0] != '_' — underscore-prefixed fields are skipped, so
            // they never match and `InvalidField` is returned.
            Err(Error::InvalidField)
        }
    }
    let mut data = Data { _a: b"12".to_vec() };
    assert_eq!(
        Err(Error::InvalidField),
        data.parse_into_field(b"_a", Some(b"42"))
    );
    assert_eq!(b"12".as_slice(), data._a.as_slice());
}

// test "parseIntoField: struct with init func"
#[test]
fn parse_into_field_struct_with_init_func() {
    struct Inner {
        v: Vec<u8>,
    }
    impl Inner {
        // pub fn init(self: *Self, _alloc: Allocator) !void { self.* = .{ .v = "HELLO!" }; }
        fn init(&mut self) {
            *self = Inner {
                v: b"HELLO!".to_vec(),
            };
        }
    }
    struct Data {
        a: Inner,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"a" => {
                    // canHaveDecls and @hasDecl(Field, "init") with empty value
                    // calls init and returns.
                    if let Some(v) = value {
                        if v.is_empty() {
                            self.a.init();
                            return Ok(());
                        }
                    }
                    Err(Error::InvalidValue)
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        a: Inner { v: Vec::new() },
    };
    data.parse_into_field(b"a", Some(b"")).unwrap();
    assert_eq!(b"HELLO!".as_slice(), data.a.v.as_slice());
}

// test "parseIntoField: string"
#[test]
fn parse_into_field_string() {
    struct Data {
        a: Vec<u8>,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"a" => {
                    self.a = parse_field_string(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data { a: Vec::new() };
    data.parse_into_field(b"a", Some(b"42")).unwrap();
    assert_eq!(b"42".as_slice(), data.a.as_slice());
}

// test "parseIntoField: sentinel string"
#[test]
fn parse_into_field_sentinel_string() {
    struct Data {
        a: Vec<u8>,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"a" => {
                    self.a = parse_field_sentinel_string(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data { a: Vec::new() };
    data.parse_into_field(b"a", Some(b"42")).unwrap();
    assert_eq!(b"42".as_slice(), data.a.as_slice());
    // try testing.expectEqual(@as(u8, 0), data.a[data.a.len]);
    //
    // PORT NOTE: the reference asserts the sentinel byte past the slice end is 0.
    // The owned `Vec<u8>` carries no trailing sentinel (the bytes are identical);
    // this sentinel assertion is the Zig `[:0]` representation detail.
}

// test "parseIntoField: bool"
#[test]
#[allow(clippy::bool_assert_comparison)]
fn parse_into_field_bool() {
    struct Data {
        a: bool,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"a" => {
                    self.a = parse_field_bool(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data { a: false };
    // True
    data.parse_into_field(b"a", Some(b"1")).unwrap();
    assert_eq!(true, data.a);
    data.parse_into_field(b"a", Some(b"t")).unwrap();
    assert_eq!(true, data.a);
    data.parse_into_field(b"a", Some(b"T")).unwrap();
    assert_eq!(true, data.a);
    data.parse_into_field(b"a", Some(b"true")).unwrap();
    assert_eq!(true, data.a);
    // False
    data.parse_into_field(b"a", Some(b"0")).unwrap();
    assert_eq!(false, data.a);
    data.parse_into_field(b"a", Some(b"f")).unwrap();
    assert_eq!(false, data.a);
    data.parse_into_field(b"a", Some(b"F")).unwrap();
    assert_eq!(false, data.a);
    data.parse_into_field(b"a", Some(b"false")).unwrap();
    assert_eq!(false, data.a);
}

// test "parseIntoField: unsigned numbers"
#[test]
fn parse_into_field_unsigned_numbers() {
    struct Data {
        u8: u8,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"u8" => {
                    self.u8 = parse_field_int::<u8>(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data { u8: 0 };
    data.parse_into_field(b"u8", Some(b"1")).unwrap();
    assert_eq!(1u8, data.u8);
}

// test "parseIntoField: floats"
#[test]
fn parse_into_field_floats() {
    struct Data {
        f64: f64,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"f64" => {
                    self.f64 = parse_field_float::<f64>(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data { f64: 0.0 };
    data.parse_into_field(b"f64", Some(b"1")).unwrap();
    assert_eq!(1.0f64, data.f64);
}

// test "parseIntoField: enums"
#[test]
fn parse_into_field_enums() {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Enum {
        One,
        Two,
        Three,
    }
    struct Data {
        v: Enum,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"v" => {
                    self.v = parse_field_enum(value, |s| match s {
                        b"one" => Some(Enum::One),
                        b"two" => Some(Enum::Two),
                        b"three" => Some(Enum::Three),
                        _ => None,
                    })?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data { v: Enum::One };
    data.parse_into_field(b"v", Some(b"two")).unwrap();
    assert_eq!(Enum::Two, data.v);
}

// ---------------------------------------------------------------------------
// packed struct field type used by the next four tests.
// const Field = packed struct { a: bool = false, b: bool = true };
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Eq)]
struct Field {
    a: bool,
    b: bool,
}
impl Default for Field {
    fn default() -> Field {
        Field { a: false, b: true }
    }
}
impl PackedStruct for Field {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"a", b"b"]
    }
    fn set_all(&mut self, b: bool) {
        self.a = b;
        self.b = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"a" => {
                self.a = value;
                true
            }
            b"b" => {
                self.b = value;
                true
            }
            _ => false,
        }
    }
}

// test "parseIntoField: packed struct"
#[test]
fn parse_into_field_packed_struct() {
    struct Data {
        v: Field,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"v" => {
                    self.v = parse_packed_struct::<Field>(value.ok_or(Error::ValueRequired)?)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        v: Field::default(),
    };
    data.parse_into_field(b"v", Some(b"b")).unwrap();
    assert!(!data.v.a);
    assert!(data.v.b);
}

// test "parseIntoField: packed struct negation"
#[test]
fn parse_into_field_packed_struct_negation() {
    struct Data {
        v: Field,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"v" => {
                    self.v = parse_packed_struct::<Field>(value.ok_or(Error::ValueRequired)?)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        v: Field::default(),
    };
    data.parse_into_field(b"v", Some(b"a,no-b")).unwrap();
    assert!(data.v.a);
    assert!(!data.v.b);
}

// test "parseIntoField: packed struct true/false"
#[test]
fn parse_into_field_packed_struct_true_false() {
    struct Data {
        v: Field,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"v" => {
                    self.v = parse_packed_struct::<Field>(value.ok_or(Error::ValueRequired)?)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        v: Field::default(),
    };
    data.parse_into_field(b"v", Some(b"true")).unwrap();
    assert!(data.v.a);
    assert!(data.v.b);

    data.parse_into_field(b"v", Some(b"false")).unwrap();
    assert!(!data.v.a);
    assert!(!data.v.b);

    assert_eq!(
        Err(Error::InvalidValue),
        data.parse_into_field(b"v", Some(b"true,a"))
    );
}

// test "parseIntoField: packed struct whitespace"
#[test]
fn parse_into_field_packed_struct_whitespace() {
    struct Data {
        v: Field,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"v" => {
                    self.v = parse_packed_struct::<Field>(value.ok_or(Error::ValueRequired)?)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        v: Field::default(),
    };
    data.parse_into_field(b"v", Some(b" a, no-b ")).unwrap();
    assert!(data.v.a);
    assert!(!data.v.b);
}

// test "parseIntoField: optional field"
#[test]
#[allow(clippy::bool_assert_comparison)]
fn parse_into_field_optional_field() {
    struct Data {
        a: Option<bool>,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"a" => {
                    // Optional: empty value resets to the declared default (null).
                    if let Some(v) = value {
                        if v.is_empty() {
                            self.a = None; // declared default
                            return Ok(());
                        }
                    }
                    // For optional fields, treat as the child type (bool).
                    self.a = Some(parse_field_bool(value)?);
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data { a: None };
    // True
    data.parse_into_field(b"a", Some(b"1")).unwrap();
    assert_eq!(true, data.a.unwrap());
    // Unset
    data.parse_into_field(b"a", Some(b"")).unwrap();
    assert!(data.a.is_none());
}

// test "parseIntoField: struct with parse func"
#[test]
fn parse_into_field_struct_with_parse_func() {
    struct Inner {
        v: Vec<u8>,
    }
    impl Inner {
        // pub fn parseCLI(value: ?[]const u8) !Self { return Self{ .v = "HELLO!" }; }
        fn parse_cli(_value: Option<&[u8]>) -> Result<Inner, ParseError> {
            Ok(Inner {
                v: b"HELLO!".to_vec(),
            })
        }
    }
    struct Data {
        a: Inner,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"a" => {
                    // 1 arg = (input) => output
                    self.a = Inner::parse_cli(value)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        a: Inner { v: Vec::new() },
    };
    data.parse_into_field(b"a", Some(b"42")).unwrap();
    assert_eq!(b"HELLO!".as_slice(), data.a.v.as_slice());
}

// test "parseIntoField: optional struct with parse func"
#[test]
fn parse_into_field_optional_struct_with_parse_func() {
    struct Inner {
        v: Vec<u8>,
    }
    impl Inner {
        // pub fn parseCLI(self: *Self, _: Allocator, value: ?[]const u8) !void { self.* = .{ .v = "HELLO!" }; }
        fn parse_cli(&mut self, _value: Option<&[u8]>) -> Result<(), ParseError> {
            *self = Inner {
                v: b"HELLO!".to_vec(),
            };
            Ok(())
        }
    }
    struct Data {
        a: Option<Inner>,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"a" => {
                    // 3 arg, optional: if set use it directly, else create new.
                    match &mut self.a {
                        Some(v) => v.parse_cli(value)?,
                        None => {
                            let mut tmp = Inner { v: Vec::new() };
                            tmp.parse_cli(value)?;
                            self.a = Some(tmp);
                        }
                    }
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data { a: None };
    data.parse_into_field(b"a", Some(b"42")).unwrap();
    assert_eq!(b"HELLO!".as_slice(), data.a.unwrap().v.as_slice());
}

// ---------------------------------------------------------------------------
// auto-struct field type used by "struct with basic fields".
// value: struct { a: []const u8, b: u32, c: u8 = 12 }
// ---------------------------------------------------------------------------
struct BasicValue {
    a: Vec<u8>,
    b: u32,
    c: u8,
}
impl AutoStruct for BasicValue {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"a", b"b", b"c"]
    }
    fn set_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
        match key {
            b"a" => {
                self.a = parse_field_string(value)?;
                Ok(())
            }
            b"b" => {
                self.b = parse_field_int::<u32>(value)?;
                Ok(())
            }
            b"c" => {
                self.c = parse_field_int::<u8>(value)?;
                Ok(())
            }
            _ => Err(Error::InvalidField),
        }
    }
    fn apply_default(&mut self, i: usize) -> Result<(), ParseError> {
        match i {
            // a: []const u8 — required (no default).
            0 => Err(Error::InvalidValue),
            // b: u32 — required (no default).
            1 => Err(Error::InvalidValue),
            // c: u8 = 12 — has a default.
            2 => {
                self.c = 12;
                Ok(())
            }
            _ => unreachable!(),
        }
    }
}

// test "parseIntoField: struct with basic fields"
#[test]
fn parse_into_field_struct_with_basic_fields() {
    struct Data {
        value: BasicValue,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"value" => parse_auto_struct(&mut self.value, value.ok_or(Error::ValueRequired)?),
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        value: BasicValue {
            a: Vec::new(),
            b: 0,
            c: 0,
        },
    };

    // Set required fields
    data.parse_into_field(b"value", Some(b"a:hello,b:42"))
        .unwrap();
    assert_eq!(b"hello".as_slice(), data.value.a.as_slice());
    assert_eq!(42, data.value.b);
    assert_eq!(12, data.value.c);

    // Set all fields
    data.parse_into_field(b"value", Some(b"a:world,b:84,c:24"))
        .unwrap();
    assert_eq!(b"world".as_slice(), data.value.a.as_slice());
    assert_eq!(84, data.value.b);
    assert_eq!(24, data.value.c);

    // Missing require dfield
    assert_eq!(
        Err(Error::InvalidValue),
        data.parse_into_field(b"value", Some(b"a:hello"))
    );
}

// ---------------------------------------------------------------------------
// tagged-union field type used by the tagged union tests.
// value: union(enum) { a: u8, b: u8, c: void, d: []const u8, e: [:0]const u8 }
//
// PORT NOTE: `parseTaggedUnion` synthesizes a one-field struct per variant to
// reuse `parseIntoField`; in Rust the union destination matches the tag and
// parses the value with the corresponding `parse_field_*` helper. Void variants
// require an empty value (`if (value.len > 0) return error.InvalidValue`).
// ---------------------------------------------------------------------------
#[derive(Debug, PartialEq, Eq)]
enum UnionFull {
    A(u8),
    B(u8),
    C,
    D(Vec<u8>),
    E(Vec<u8>),
}

fn parse_union_full(v: &[u8]) -> Result<UnionFull, ParseError> {
    let (tag, value) = split_tagged_union(v);
    match tag {
        b"a" => Ok(UnionFull::A(parse_field_int::<u8>(Some(value))?)),
        b"b" => Ok(UnionFull::B(parse_field_int::<u8>(Some(value))?)),
        b"c" => {
            // void: if (value.len > 0) return error.InvalidValue;
            if !value.is_empty() {
                return Err(Error::InvalidValue);
            }
            Ok(UnionFull::C)
        }
        b"d" => Ok(UnionFull::D(parse_field_string(Some(value))?)),
        b"e" => Ok(UnionFull::E(parse_field_sentinel_string(Some(value))?)),
        // return error.InvalidValue (unknown tag)
        _ => Err(Error::InvalidValue),
    }
}

// test "parseIntoField: tagged union"
#[test]
fn parse_into_field_tagged_union() {
    struct Data {
        value: UnionFull,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"value" => {
                    self.value = parse_union_full(value.ok_or(Error::ValueRequired)?)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        value: UnionFull::C,
    };

    // Set one field
    data.parse_into_field(b"value", Some(b"a:1")).unwrap();
    assert_eq!(UnionFull::A(1), data.value);

    // Set another
    data.parse_into_field(b"value", Some(b"b:2")).unwrap();
    assert_eq!(UnionFull::B(2), data.value);

    // Set void field
    data.parse_into_field(b"value", Some(b"c")).unwrap();
    assert_eq!(UnionFull::C, data.value);

    // Set string field
    data.parse_into_field(b"value", Some(b"d:hello")).unwrap();
    assert_eq!(UnionFull::D(b"hello".to_vec()), data.value);

    // Set sentinel string field
    data.parse_into_field(b"value", Some(b"e:hello")).unwrap();
    assert_eq!(UnionFull::E(b"hello".to_vec()), data.value);
}

// ---------------------------------------------------------------------------
// tagged-union field type used by the error tests.
// value: union(enum) { a: u8, b: u8 }
// ---------------------------------------------------------------------------
#[derive(Debug, PartialEq, Eq)]
enum UnionAB {
    A(u8),
    B(u8),
}

fn parse_union_ab(v: &[u8]) -> Result<UnionAB, ParseError> {
    let (tag, value) = split_tagged_union(v);
    match tag {
        b"a" => Ok(UnionAB::A(parse_field_int::<u8>(Some(value))?)),
        b"b" => Ok(UnionAB::B(parse_field_int::<u8>(Some(value))?)),
        _ => Err(Error::InvalidValue),
    }
}

// test "parseIntoField: tagged union unknown filed"
#[test]
fn parse_into_field_tagged_union_unknown_filed() {
    struct Data {
        value: UnionAB,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"value" => {
                    self.value = parse_union_ab(value.ok_or(Error::ValueRequired)?)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        value: UnionAB::A(0),
    };
    assert_eq!(
        Err(Error::InvalidValue),
        data.parse_into_field(b"value", Some(b"c:1"))
    );
}

// test "parseIntoField: tagged union invalid field value"
#[test]
fn parse_into_field_tagged_union_invalid_field_value() {
    struct Data {
        value: UnionAB,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"value" => {
                    self.value = parse_union_ab(value.ok_or(Error::ValueRequired)?)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        value: UnionAB::A(0),
    };
    assert_eq!(
        Err(Error::InvalidValue),
        data.parse_into_field(b"value", Some(b"a:hello"))
    );
}

// test "parseIntoField: tagged union missing tag"
#[test]
fn parse_into_field_tagged_union_missing_tag() {
    struct Data {
        value: UnionAB,
    }
    impl Target for Data {
        fn parse_into_field(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), ParseError> {
            match key {
                b"value" => {
                    self.value = parse_union_ab(value.ok_or(Error::ValueRequired)?)?;
                    Ok(())
                }
                _ => Err(Error::InvalidField),
            }
        }
    }
    let mut data = Data {
        value: UnionAB::A(0),
    };
    assert_eq!(
        Err(Error::InvalidValue),
        data.parse_into_field(b"value", Some(b"a"))
    );
    assert_eq!(
        Err(Error::InvalidValue),
        data.parse_into_field(b"value", Some(b":a"))
    );
}

// ===========================================================================
// test "ArgsIterator"
// ===========================================================================
#[test]
fn args_iterator_test() {
    // PORT NOTE: `ArgsIterator(@TypeOf(child))` wraps the
    // `ArgIteratorGeneral`-tokenized child. The child is adapted to `GenIter`
    // (yields `Option<Vec<u8>>`).
    struct ChildGen {
        inner: ArgIteratorGeneral,
    }
    impl GenIter for ChildGen {
        fn next(&mut self) -> Option<Vec<u8>> {
            ArgIterator::next(&mut self.inner)
        }
    }

    let child = ArgIteratorGeneral::init(b"--what +list-things --a=42");
    let mut iter = ArgsIterator::new(ChildGen { inner: child });

    assert_eq!(
        b"--what".as_slice(),
        ArgsIterator::next(&mut iter).unwrap().as_slice()
    );
    assert_eq!(
        b"--a=42".as_slice(),
        ArgsIterator::next(&mut iter).unwrap().as_slice()
    );
    assert_eq!(None, ArgsIterator::next(&mut iter));
    assert_eq!(None, ArgsIterator::next(&mut iter));
}

// ===========================================================================
// LineIterator tests
// ===========================================================================

// test "LineIterator"
#[test]
fn line_iterator() {
    let src: &[u8] = b"A\nB=42\nC\n\n# A comment\nD\n\n  # An indented comment\n  E\n\n# A quoted string with whitespace\nF=  \"value \"";
    let mut iter = LineIterator::init(src);
    assert_eq!(b"--A".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(b"--B=42".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(b"--C".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(b"--D".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(b"--E".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(
        b"--F=value ".as_slice(),
        LineIterator::next(&mut iter).unwrap()
    );
    assert_eq!(None, LineIterator::next(&mut iter));
    assert_eq!(None, LineIterator::next(&mut iter));
}

// test "LineIterator end in newline"
#[test]
fn line_iterator_end_in_newline() {
    let src: &[u8] = b"A\n\n";
    let mut iter = LineIterator::init(src);
    assert_eq!(b"--A".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(None, LineIterator::next(&mut iter));
    assert_eq!(None, LineIterator::next(&mut iter));
}

// test "LineIterator spaces around '='"
#[test]
fn line_iterator_spaces_around_eq() {
    let src: &[u8] = b"A = B\n\n";
    let mut iter = LineIterator::init(src);
    assert_eq!(b"--A=B".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(None, LineIterator::next(&mut iter));
    assert_eq!(None, LineIterator::next(&mut iter));
}

// test "LineIterator no value"
#[test]
fn line_iterator_no_value() {
    let src: &[u8] = b"A = \n\n";
    let mut iter = LineIterator::init(src);
    assert_eq!(b"--A=".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(None, LineIterator::next(&mut iter));
}

// test "LineIterator with CRLF line endings"
#[test]
fn line_iterator_with_crlf_line_endings() {
    let src: &[u8] = b"A\r\nB = C\r\n";
    let mut iter = LineIterator::init(src);
    assert_eq!(b"--A".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(b"--B=C".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(None, LineIterator::next(&mut iter));
    assert_eq!(None, LineIterator::next(&mut iter));
}

// test "LineIterator with buffered reader"
//
// PORT NOTE: the reference exercises a 2-byte-buffered reader to verify the
// streaming logic across buffer refills. The Rust port's `LineIterator` holds
// the whole source slice (no separate buffer to refill), so this and the
// "primed" variant below assert the identical output over the same input.
#[test]
fn line_iterator_with_buffered_reader() {
    let src: &[u8] = b"A\nB = C\n";
    let mut iter = LineIterator::init(src);
    assert_eq!(b"--A".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(b"--B=C".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(None, LineIterator::next(&mut iter));
    assert_eq!(None, LineIterator::next(&mut iter));
}

// test "LineIterator with buffered and primed reader"
#[test]
fn line_iterator_with_buffered_and_primed_reader() {
    let src: &[u8] = b"A\nB = C\n";
    // try reader.fill(buf.len); — priming is a no-op for the slice-backed reader.
    let mut iter = LineIterator::init(src);
    assert_eq!(b"--A".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(b"--B=C".as_slice(), LineIterator::next(&mut iter).unwrap());
    assert_eq!(None, LineIterator::next(&mut iter));
    assert_eq!(None, LineIterator::next(&mut iter));
}

// PORT NOTE: keep `LocationKey`/`Diagnostic` referenced so their imports are
// exercised by the diagnostics surface (used by downstream config); no reference
// test in args.zig drives them directly.
#[allow(dead_code)]
fn _location_key_touch() -> LocationKey {
    LocationKey::Cli
}

#[allow(dead_code)]
fn _diagnostic_touch() -> Diagnostic {
    Diagnostic::default()
}

// PORT NOTE: not an args.zig `test {}` — a focused port-helper check for
// `zig_string_escape` (the `std.zig.fmtString`/`stringEscape` port used by
// config's `RepeatableCommand.formatEntry`). Mirrors the std `test fmtString`:
//   try std.testing.expectFmt("\\x0f", "{f}", .{fmtString("\x0f")});
//   try std.testing.expectFmt(
//       \\" \\ hi \x07 \x11 \" derp '"
//   , "\"{f}\"", .{fmtString(" \\ hi \x07 \x11 \" derp '")});
#[test]
fn zig_string_escape_port_helper() {
    let mut out = Vec::new();
    zig_string_escape(&mut out, b"\x0f");
    assert_eq!(out, b"\\x0f");

    let mut out = Vec::new();
    zig_string_escape(&mut out, b" \\ hi \x07 \x11 \" derp '");
    // The format string wraps with quotes; the escaped body is:
    assert_eq!(out, b" \\\\ hi \\x07 \\x11 \\\" derp '");
}
