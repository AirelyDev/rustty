// The contents of this file is largely based on testing.zig from the Zig 0.15.1
// stdlib, distributed under the MIT license, copyright (c) Zig contributors
//
// PORT NOTE: The Zig reference is comptime-generic: `deepEqual(comptime T, old,
// new)` and `expectApproxEqual(expected, actual)` switch on `@typeInfo(T)` at
// compile time. Rust has no comptime type reflection, so the type-directed
// dispatch is expressed with traits: `DeepEqual` (one method per the reference's
// recursive contract) and `ApproxEqual`. Blanket/concrete impls reproduce each
// reference arm exactly — void, bool, int, float, enum, `[]const u8`,
// `[:0]const u8`, `[]const [:0]const u8`, optional, array, struct, union — and a
// type that declares a custom `equal`/`approx` impl overrides the field-by-field
// fallback, mirroring the reference's `@hasDecl(T, "equal")` check. The error
// semantics (`error.TestExpectedApproxEqual`) are preserved via `Result`.

/// Error returned by `expect_approx_equal` on mismatch (mirror of
/// `error.TestExpectedApproxEqual`).
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    TestExpectedApproxEqual,
}

/// A deep equality comparison function that works for most types. We
/// add types as necessary. It defers to `equal` decls on types that support
/// decls.
//
// PORT NOTE: `deep_equal` is generic over `T: DeepEqual`; each `DeepEqual` impl
// is one arm of the reference's `switch (@typeInfo(T))`. A struct/union that
// provides its own `equal` implements `DeepEqual` directly with that logic
// (mirror of `if (@hasDecl(T, "equal")) return old.equal(new)`).
pub fn deep_equal<T: DeepEqual + ?Sized>(old: &T, new: &T) -> bool {
    old.deep_equal(new)
}

/// The trait implementing the type-directed `deepEqual` dispatch.
pub trait DeepEqual {
    fn deep_equal(&self, other: &Self) -> bool;
}

// The reference `deepEqual` is two sequential switches: a first `switch (T)`
// over a handful of named types, then a `switch (@typeInfo(T))` over kinds.
// The trait-based port splits each arm into its own `DeepEqual` impl, so the
// two reference switch headers below annotate the impls reproducing each: the
// named-type impls (`[u8]`, `[&[u8]]`) and the kind impls (`()`, the
// `deep_equal_eq!` scalars, `Option`, `[T; N]`).

// Do known named types first
//
// (See the `[u8]` / `[&[u8]]` impls below: the reference's first
// `switch (T)` arms — `[]const u8`, `[:0]const u8`, `[]const [:0]const u8`.)

// Back into types of types
// .void => return true,
impl DeepEqual for () {
    fn deep_equal(&self, _other: &Self) -> bool {
        true
    }
}

// inline .bool, .int, .float, .@"enum" => return old == new,
macro_rules! deep_equal_eq {
    ($($t:ty),*) => {
        $(impl DeepEqual for $t {
            fn deep_equal(&self, other: &Self) -> bool { *self == *other }
        })*
    };
}
deep_equal_eq!(bool, i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64);

// inline []const u8, [:0]const u8, => return std.mem.eql(u8, old, new),
// (A `[]const u8` / `[:0]const u8` slice maps to `[u8]` in Rust; sentinel
// termination is not observable for equality.)
impl DeepEqual for [u8] {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}

// A `Vec<u8>` is the owned form of a `[]const u8` / `[:0]const u8` config field
// (e.g. `term`, `language`, `title`). The reference compares these via the
// `[]const u8` arm (`std.mem.eql`); here we forward an owned `Vec<u8>` to the
// `[u8]` impl so `Option<Vec<u8>>` config fields resolve through the optional
// arm exactly as the reference's `?[:0]const u8` does. (This impl lives here,
// the trait's home crate, because both `Vec<u8>` and `DeepEqual` are otherwise
// foreign to downstream crates — the orphan rule.)
impl DeepEqual for Vec<u8> {
    fn deep_equal(&self, other: &Self) -> bool {
        deep_equal::<[u8]>(self.as_slice(), other.as_slice())
    }
}

// .optional => |info| { ... deepEqual(info.child, old.?, new.?) }
impl<T: DeepEqual> DeepEqual for Option<T> {
    fn deep_equal(&self, other: &Self) -> bool {
        if self.is_none() && other.is_none() {
            return true;
        }
        if self.is_none() || other.is_none() {
            return false;
        }
        deep_equal(self.as_ref().unwrap(), other.as_ref().unwrap())
    }
}

// .array => |info| for (old, new) |old_elem, new_elem| { ... } else return true,
impl<T: DeepEqual, const N: usize> DeepEqual for [T; N] {
    fn deep_equal(&self, other: &Self) -> bool {
        for (old_elem, new_elem) in self.iter().zip(other.iter()) {
            if !deep_equal(old_elem, new_elem) {
                return false;
            }
        }
        true
    }
}

/// A `[]const [:0]const u8` (slice of NUL-terminated strings). The reference
/// special-cases this: compares lengths then element-by-element with
/// `std.mem.eql`. Modeled as a slice of `&[u8]`.
//
// []const [:0]const u8 => { if (old.len != new.len) return false; ... }
impl DeepEqual for [&[u8]] {
    fn deep_equal(&self, other: &Self) -> bool {
        if self.len() != other.len() {
            return false;
        }
        for (a, b) in self.iter().zip(other.iter()) {
            if a != b {
                return false;
            }
        }

        true
    }
}

/// Generic, recursive equality testing utility using approximate comparison for
/// floats and equality for everything else
///
/// Based on `testing.expectEqual` and `testing.expectEqualSlices`.
///
/// The relative tolerance is currently hardcoded to `sqrt(eps(float_type))`.
pub fn expect_approx_equal<T: ApproxEqual + ?Sized>(expected: &T, actual: &T) -> Result<(), Error> {
    expect_approx_equal_inner(expected, actual)
}

fn expect_approx_equal_inner<T: ApproxEqual + ?Sized>(
    expected: &T,
    actual: &T,
) -> Result<(), Error> {
    expected.approx_equal(actual)
}

/// The trait implementing the type-directed `expectApproxEqual` dispatch.
pub trait ApproxEqual {
    fn approx_equal(&self, actual: &Self) -> Result<(), Error>;
}

// check approximate equality for floats
//   .float => { const sqrt_eps = sqrt(floatEps(T));
//               if (!approxEqRel(T, expected, actual, sqrt_eps)) { ...; return error; } }
impl ApproxEqual for f32 {
    fn approx_equal(&self, actual: &Self) -> Result<(), Error> {
        let sqrt_eps = f32::EPSILON.sqrt();
        if !approx_eq_rel_f32(*self, *actual, sqrt_eps) {
            // print("expected approximately {any}, found {any}\n", ...)
            return Err(Error::TestExpectedApproxEqual);
        }
        Ok(())
    }
}

impl ApproxEqual for f64 {
    fn approx_equal(&self, actual: &Self) -> Result<(), Error> {
        let sqrt_eps = f64::EPSILON.sqrt();
        if !approx_eq_rel_f64(*self, *actual, sqrt_eps) {
            return Err(Error::TestExpectedApproxEqual);
        }
        Ok(())
    }
}

// recurse into containers (arrays / vectors)
//   .array => { ... expectApproxEqual(actual[i], expected[i]) ... }
//   .vector => { ... }
// In Rust both fixed arrays `[T; N]` and "vectors" (`@Vector(N, f32)`) map to
// `[T; N]`, so a single impl covers both reference arms. Note: the reference's
// array arm computes a `diff_index` and prints it on mismatch; the resulting
// error is the same `error.TestExpectedApproxEqual` in every path.
impl<T: ApproxEqual, const N: usize> ApproxEqual for [T; N] {
    fn approx_equal(&self, actual: &Self) -> Result<(), Error> {
        let expected = self;
        // const diff_index: usize = diff_index: { ... };
        let mut diff: Option<usize> = None;
        {
            let shortest = core::cmp::min(expected.len(), actual.len());
            let mut index: usize = 0;
            while index < shortest {
                // expectApproxEqual(actual[index], expected[index]) catch break :diff_index index;
                if expect_approx_equal(&actual[index], &expected[index]).is_err() {
                    diff = Some(index);
                    break;
                }
                index += 1;
            }
            // break :diff_index if (expected.len == actual.len) return else shortest;
            if diff.is_none() {
                if expected.len() == actual.len() {
                    return Ok(());
                } else {
                    diff = Some(shortest);
                }
            }
        }
        let _diff_index = diff.unwrap();
        // print("slices not approximately equal. first significant difference ...");
        Err(Error::TestExpectedApproxEqual)
    }
}

// .float fallback for everything else is handled by concrete impls; the
// reference's `.@"struct"`, `.@"union"`, `.optional`, `.error_union` arms are
// reproduced by concrete/blanket impls below + per-type impls in tests.
impl ApproxEqual for i32 {
    // fall back to expectEqual for everything else
    fn approx_equal(&self, actual: &Self) -> Result<(), Error> {
        if *self == *actual {
            Ok(())
        } else {
            Err(Error::TestExpectedApproxEqual)
        }
    }
}

// A `u32` struct field (e.g. `Metrics.cell_width`) hits the reference's
// "fall back to expectEqual" arm just like `i32`; exact-equality compare.
impl ApproxEqual for u32 {
    fn approx_equal(&self, actual: &Self) -> Result<(), Error> {
        if *self == *actual {
            Ok(())
        } else {
            Err(Error::TestExpectedApproxEqual)
        }
    }
}

// unwrap unions, optionals, and error unions
//
// (The reference groups the `.@"union"`, `.optional`, and `.error_union` arms
// here. In the trait-based port the `.@"union"` arm is reproduced per-type —
// see the `TUnion` impl in tests — the `.optional` arm is the `Option<T>` impl
// below, and `.error_union` maps to `Result` with the same `expectEqual`
// fallthrough.)

// .optional => { if (expected) |e| if (actual) |a| return expectApproxEqual(e, a);
//                testing.expectEqual(expected, actual) catch return error; }
impl<T: ApproxEqual> ApproxEqual for Option<T> {
    fn approx_equal(&self, actual: &Self) -> Result<(), Error> {
        if let Some(expected_payload) = self {
            if let Some(actual_payload) = actual {
                return expect_approx_equal(expected_payload, actual_payload);
            }
        }
        // we only reach this point if there's at least one null or error,
        // in which case expectEqual is all we need
        match (self, actual) {
            (None, None) => Ok(()),
            _ => Err(Error::TestExpectedApproxEqual),
        }
    }
}

/// `approxEqRel` for f32: |a-b| <= max(|a|,|b|) * tolerance (with exact-equal
/// fast path), matching `std.math.approxEqRel`.
fn approx_eq_rel_f32(x: f32, y: f32, tolerance: f32) -> bool {
    assert_ne_debug(tolerance);
    // Fast path for equal values (and signed zeros and infinites).
    if x == y {
        return true;
    }
    if x.is_nan() || y.is_nan() {
        return false;
    }
    (x - y).abs() <= x.abs().max(y.abs()) * tolerance
}

fn approx_eq_rel_f64(x: f64, y: f64, tolerance: f64) -> bool {
    assert_ne_debug64(tolerance);
    if x == y {
        return true;
    }
    if x.is_nan() || y.is_nan() {
        return false;
    }
    (x - y).abs() <= x.abs().max(y.abs()) * tolerance
}

#[inline]
fn assert_ne_debug(_tolerance: f32) {
    // std.math.approxEqRel asserts tolerance >= 0; mirrored as a debug check.
    debug_assert!(_tolerance >= 0.0);
}

#[inline]
fn assert_ne_debug64(_tolerance: f64) {
    debug_assert!(_tolerance >= 0.0);
}

/// Copy of testing.print (not public)
//
// PORT NOTE: `print` is a debug helper in the reference that writes a mismatch
// diagnostic to stderr (or `@compileError` at comptime). It is purely
// diagnostic and never affects the boolean/Result outcome; the reference's
// failure paths all return `error.TestExpectedApproxEqual` regardless. We keep
// it as a no-op shim to preserve the source's structure without spamming test
// output.
#[allow(dead_code)]
fn print(_fmt: &str) {}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;

    // ---- Test-local types mirroring the Zig anonymous types ----

    // Tests based on the `expectEqual` tests in the Zig stdlib

    // union(enum) { a: i32, b: f32 } with the reference's approx-union arm:
    //   testing.expectEqual(tags); switch (expected) { inline else => |val, tag|
    //       try expectApproxEqual(val, @field(actual, @tagName(tag))) }
    #[derive(PartialEq)]
    #[allow(dead_code)]
    enum TUnion {
        A(i32),
        B(f32),
    }

    impl ApproxEqual for TUnion {
        fn approx_equal(&self, actual: &Self) -> Result<(), Error> {
            // The reference `.@"union"` arm first handles the untagged case:
            //   untagged unions can only be compared bitwise,
            //   so expectEqual is all we need
            // Rust enums are always tagged, so only the tagged path applies
            // here; we compare the active tag then recurse into the payload.
            // expectEqual(expectedTag, actualTag)

            // we only reach this switch if the tags are equal
            match (self, actual) {
                (TUnion::A(e), TUnion::A(a)) => expect_approx_equal(e, a),
                (TUnion::B(e), TUnion::B(a)) => expect_approx_equal(e, a),
                _ => Err(Error::TestExpectedApproxEqual),
            }
        }
    }

    #[test]
    fn expectApproxEqual_union_enum() -> Result<(), Error> {
        let b10 = TUnion::B(10.0);
        let b10plus = TUnion::B(10.000001);

        expect_approx_equal(&b10, &b10plus)?;
        Ok(())
    }

    #[test]
    fn expectApproxEqual_nested_array() -> Result<(), Error> {
        let a: [[f32; 2]; 2] = [[1.0, 0.0], [0.0, 1.0]];

        let b: [[f32; 2]; 2] = [[1.000001, 0.0], [0.0, 0.999999]];

        expect_approx_equal(&a, &b)?;
        Ok(())
    }

    #[test]
    fn expectApproxEqual_vector() -> Result<(), Error> {
        let a: [f32; 4] = [4.0; 4];
        let b: [f32; 4] = [4.000001; 4];

        expect_approx_equal(&a, &b)?;
        Ok(())
    }

    // const a = .{ 1, @as(f32, 1.0) }; — an anonymous (tuple) struct with an
    // i32 and an f32 field. Modeled as a 2-field struct implementing the
    // reference's struct arm (field-by-field expectApproxEqual).
    struct TStruct(i32, f32);

    impl ApproxEqual for TStruct {
        fn approx_equal(&self, actual: &Self) -> Result<(), Error> {
            // inline for (structType.fields) |field| try expectApproxEqual(...)
            expect_approx_equal(&self.0, &actual.0)?;
            expect_approx_equal(&self.1, &actual.1)?;
            Ok(())
        }
    }

    #[test]
    fn expectApproxEqual_struct() -> Result<(), Error> {
        let a = TStruct(1, 1.0);
        let b = TStruct(1, 0.999999);

        expect_approx_equal(&a, &b)?;
        Ok(())
    }

    #[test]
    fn deepEqual_void() {
        assert!(deep_equal(&(), &()));
    }

    #[test]
    fn deepEqual_bool() {
        assert!(deep_equal(&true, &true));
        assert!(deep_equal(&false, &false));
        assert!(!deep_equal(&true, &false));
        assert!(!deep_equal(&false, &true));
    }

    #[test]
    fn deepEqual_int() {
        assert!(deep_equal(&42i32, &42i32));
        assert!(deep_equal(&-100i32, &-100i32));
        assert!(!deep_equal(&42i32, &43i32));
        assert!(deep_equal(&0u64, &0u64));
        assert!(!deep_equal(&0u64, &1u64));
    }

    #[test]
    // PARITY: the reference `comparison.zig:262-263` uses the literals 3.14159 /
    // 3.14158 to exercise `deepEqual` on f64; substituting `std::f64::consts::PI`
    // would change the values under test and destroy the assertion, so we keep
    // the literals and silence `approx_constant`.
    #[allow(clippy::approx_constant)]
    fn deepEqual_float() {
        assert!(deep_equal(&1.0f32, &1.0f32));
        assert!(!deep_equal(&1.0f32, &1.1f32));
        assert!(deep_equal(&3.14159f64, &3.14159f64));
        assert!(!deep_equal(&3.14159f64, &3.14158f64));
    }

    #[derive(PartialEq)]
    enum Color {
        Red,
        Green,
        Blue,
    }
    impl DeepEqual for Color {
        fn deep_equal(&self, other: &Self) -> bool {
            self == other
        }
    }

    #[test]
    fn deepEqual_enum() {
        assert!(deep_equal(&Color::Red, &Color::Red));
        assert!(deep_equal(&Color::Blue, &Color::Blue));
        assert!(!deep_equal(&Color::Red, &Color::Green));
        assert!(!deep_equal(&Color::Green, &Color::Blue));
    }

    #[test]
    fn deepEqual_const_u8() {
        assert!(deep_equal::<[u8]>(b"hello", b"hello"));
        assert!(deep_equal::<[u8]>(b"", b""));
        assert!(!deep_equal::<[u8]>(b"hello", b"world"));
        assert!(!deep_equal::<[u8]>(b"hello", b"hell"));
        assert!(!deep_equal::<[u8]>(b"hello", b"hello!"));
    }

    #[test]
    fn deepEqual_0_const_u8() {
        assert!(deep_equal::<[u8]>(b"foo", b"foo"));
        assert!(!deep_equal::<[u8]>(b"foo", b"bar"));
        assert!(!deep_equal::<[u8]>(b"foo", b"fo"));
    }

    #[test]
    fn deepEqual_const_0_const_u8() {
        let a: &[&[u8]] = &[b"one", b"two", b"three"];
        let b: &[&[u8]] = &[b"one", b"two", b"three"];
        let c: &[&[u8]] = &[b"one", b"two"];
        let d: &[&[u8]] = &[b"one", b"two", b"four"];
        let e: &[&[u8]] = &[];

        assert!(deep_equal::<[&[u8]]>(a, b));
        assert!(!deep_equal::<[&[u8]]>(a, c));
        assert!(!deep_equal::<[&[u8]]>(a, d));
        assert!(deep_equal::<[&[u8]]>(e, e));
        assert!(!deep_equal::<[&[u8]]>(a, e));
    }

    #[test]
    fn deepEqual_optional() {
        assert!(deep_equal::<Option<i32>>(&None, &None));
        assert!(deep_equal::<Option<i32>>(&Some(42), &Some(42)));
        assert!(!deep_equal::<Option<i32>>(&None, &Some(42)));
        assert!(!deep_equal::<Option<i32>>(&Some(42), &None));
        assert!(!deep_equal::<Option<i32>>(&Some(42), &Some(43)));
    }

    #[derive(PartialEq)]
    struct Nested {
        x: i32,
        y: i32,
    }
    impl DeepEqual for Nested {
        fn deep_equal(&self, other: &Self) -> bool {
            // If a struct doesn't declare an "equal" function, we fall back
            // to a recursive field-by-field compare.
            deep_equal(&self.x, &other.x) && deep_equal(&self.y, &other.y)
        }
    }

    #[test]
    fn deepEqual_optional_nested() {
        assert!(deep_equal::<Option<Nested>>(&None, &None));
        assert!(deep_equal::<Option<Nested>>(
            &Some(Nested { x: 1, y: 2 }),
            &Some(Nested { x: 1, y: 2 })
        ));
        assert!(!deep_equal::<Option<Nested>>(
            &Some(Nested { x: 1, y: 2 }),
            &Some(Nested { x: 1, y: 3 })
        ));
        assert!(!deep_equal::<Option<Nested>>(
            &Some(Nested { x: 1, y: 2 }),
            &None
        ));
    }

    #[test]
    fn deepEqual_array() {
        assert!(deep_equal::<[i32; 3]>(&[1, 2, 3], &[1, 2, 3]));
        assert!(!deep_equal::<[i32; 3]>(&[1, 2, 3], &[1, 2, 4]));
        assert!(!deep_equal::<[i32; 3]>(&[1, 2, 3], &[0, 2, 3]));
        assert!(deep_equal::<[i32; 0]>(&[], &[]));
    }

    #[test]
    fn deepEqual_nested_array() {
        let a: [[i32; 2]; 2] = [[1, 2], [3, 4]];
        let b: [[i32; 2]; 2] = [[1, 2], [3, 4]];
        let c: [[i32; 2]; 2] = [[1, 2], [3, 5]];

        assert!(deep_equal::<[[i32; 2]; 2]>(&a, &b));
        assert!(!deep_equal::<[[i32; 2]; 2]>(&a, &c));
    }

    struct Point {
        x: i32,
        y: i32,
    }
    impl DeepEqual for Point {
        fn deep_equal(&self, other: &Self) -> bool {
            deep_equal(&self.x, &other.x) && deep_equal(&self.y, &other.y)
        }
    }

    #[test]
    fn deepEqual_struct() {
        assert!(deep_equal(&Point { x: 10, y: 20 }, &Point { x: 10, y: 20 }));
        assert!(!deep_equal(
            &Point { x: 10, y: 20 },
            &Point { x: 10, y: 21 }
        ));
        assert!(!deep_equal(
            &Point { x: 10, y: 20 },
            &Point { x: 11, y: 20 }
        ));
    }

    struct Inner {
        value: i32,
    }
    impl DeepEqual for Inner {
        fn deep_equal(&self, other: &Self) -> bool {
            deep_equal(&self.value, &other.value)
        }
    }
    struct Outer {
        a: Inner,
        b: Inner,
    }
    impl DeepEqual for Outer {
        fn deep_equal(&self, other: &Self) -> bool {
            deep_equal(&self.a, &other.a) && deep_equal(&self.b, &other.b)
        }
    }

    #[test]
    fn deepEqual_struct_nested() {
        let x = Outer {
            a: Inner { value: 1 },
            b: Inner { value: 2 },
        };
        let y = Outer {
            a: Inner { value: 1 },
            b: Inner { value: 2 },
        };
        let z = Outer {
            a: Inner { value: 1 },
            b: Inner { value: 3 },
        };

        assert!(deep_equal(&x, &y));
        assert!(!deep_equal(&x, &z));
    }

    struct Custom {
        value: i32,
    }
    impl DeepEqual for Custom {
        // pub fn equal(self, other) bool { return @mod(self.value,10) == @mod(other.value,10); }
        fn deep_equal(&self, other: &Self) -> bool {
            self.value.rem_euclid(10) == other.value.rem_euclid(10)
        }
    }

    #[test]
    fn deepEqual_struct_with_equal_decl() {
        assert!(deep_equal(&Custom { value: 5 }, &Custom { value: 15 }));
        assert!(deep_equal(&Custom { value: 100 }, &Custom { value: 0 }));
        assert!(!deep_equal(&Custom { value: 5 }, &Custom { value: 6 }));
    }

    // union(enum) { int: i32, float: f32, none } — no `equal` decl: compare
    // active tag then payload.
    #[derive(PartialEq)]
    enum Value {
        Int(i32),
        Float(f32),
        None,
    }
    impl DeepEqual for Value {
        fn deep_equal(&self, other: &Self) -> bool {
            // const old_tag = activeTag(old); ... if (old_tag != new_tag) return false;
            // then deepEqual of the active field.
            match (self, other) {
                (Value::Int(a), Value::Int(b)) => deep_equal(a, b),
                (Value::Float(a), Value::Float(b)) => deep_equal(a, b),
                (Value::None, Value::None) => true,
                _ => false,
            }
        }
    }

    #[test]
    fn deepEqual_union() {
        assert!(deep_equal(&Value::Int(42), &Value::Int(42)));
        assert!(!deep_equal(&Value::Int(42), &Value::Int(43)));
        assert!(!deep_equal(&Value::Int(42), &Value::Float(42.0)));
        assert!(deep_equal(&Value::None, &Value::None));
        assert!(!deep_equal(&Value::None, &Value::Int(0)));
    }

    // union(enum) { num: i32, str: []const u8 } with a custom `equal` decl.
    enum Value2 {
        Num(i32),
        Str(Vec<u8>),
    }
    impl DeepEqual for Value2 {
        // pub fn equal(self, other) bool { switch (self) { .num => ... @mod==@mod, .str => s.len==t.len } }
        fn deep_equal(&self, other: &Self) -> bool {
            match self {
                Value2::Num(n) => match other {
                    Value2::Num(m) => n.rem_euclid(10) == m.rem_euclid(10),
                    _ => false,
                },
                Value2::Str(s) => match other {
                    Value2::Str(t) => s.len() == t.len(),
                    _ => false,
                },
            }
        }
    }

    #[test]
    fn deepEqual_union_with_equal_decl() {
        assert!(deep_equal(&Value2::Num(5), &Value2::Num(25)));
        assert!(!deep_equal(&Value2::Num(5), &Value2::Num(6)));
        assert!(deep_equal(
            &Value2::Str(b"abc".to_vec()),
            &Value2::Str(b"xyz".to_vec())
        ));
        assert!(!deep_equal(
            &Value2::Str(b"abc".to_vec()),
            &Value2::Str(b"ab".to_vec())
        ));
    }

    // const Item = struct { id: i32, name: []const u8 };
    struct Item {
        id: i32,
        name: Vec<u8>,
    }
    impl DeepEqual for Item {
        fn deep_equal(&self, other: &Self) -> bool {
            deep_equal(&self.id, &other.id) && deep_equal::<[u8]>(&self.name, &other.name)
        }
    }

    #[test]
    fn deepEqual_array_of_structs() {
        let a: [Item; 2] = [
            Item {
                id: 1,
                name: b"one".to_vec(),
            },
            Item {
                id: 2,
                name: b"two".to_vec(),
            },
        ];
        let b: [Item; 2] = [
            Item {
                id: 1,
                name: b"one".to_vec(),
            },
            Item {
                id: 2,
                name: b"two".to_vec(),
            },
        ];
        let c: [Item; 2] = [
            Item {
                id: 1,
                name: b"one".to_vec(),
            },
            Item {
                id: 2,
                name: b"TWO".to_vec(),
            },
        ];

        assert!(deep_equal::<[Item; 2]>(&a, &b));
        assert!(!deep_equal::<[Item; 2]>(&a, &c));
    }

    // const Config = struct { name: []const u8, port: ?u16 };
    struct Config {
        name: Vec<u8>,
        port: Option<u16>,
    }
    impl DeepEqual for Config {
        fn deep_equal(&self, other: &Self) -> bool {
            deep_equal::<[u8]>(&self.name, &other.name) && deep_equal(&self.port, &other.port)
        }
    }

    #[test]
    fn deepEqual_struct_with_optional_field() {
        assert!(deep_equal(
            &Config {
                name: b"app".to_vec(),
                port: Some(8080)
            },
            &Config {
                name: b"app".to_vec(),
                port: Some(8080)
            }
        ));
        assert!(deep_equal(
            &Config {
                name: b"app".to_vec(),
                port: None
            },
            &Config {
                name: b"app".to_vec(),
                port: None
            }
        ));
        assert!(!deep_equal(
            &Config {
                name: b"app".to_vec(),
                port: Some(8080)
            },
            &Config {
                name: b"app".to_vec(),
                port: None
            }
        ));
        assert!(!deep_equal(
            &Config {
                name: b"app".to_vec(),
                port: Some(8080)
            },
            &Config {
                name: b"app".to_vec(),
                port: Some(8081)
            }
        ));
    }

    // const Data = struct { values: [3]i32 };
    struct Data {
        values: [i32; 3],
    }
    impl DeepEqual for Data {
        fn deep_equal(&self, other: &Self) -> bool {
            deep_equal::<[i32; 3]>(&self.values, &other.values)
        }
    }

    #[test]
    fn deepEqual_struct_with_array_field() {
        assert!(deep_equal(
            &Data { values: [1, 2, 3] },
            &Data { values: [1, 2, 3] }
        ));
        assert!(!deep_equal(
            &Data { values: [1, 2, 3] },
            &Data { values: [1, 2, 4] }
        ));
    }
}
