//! PORT NOTE: Ports ghostty `src/lib/enum.zig`.
//!
//! Zig's `Enum(target, keys)` is a *comptime type factory*: it synthesizes a
//! brand-new `enum` type whose backing integer (tag) type and field values
//! depend on the `target`. Rust cannot synthesize a nominal type at runtime,
//! so we model the SAME algorithm as a value: `EnumType` records the computed
//! backing tag type and the ordered (name, value) fields. Every observable the
//! reference tests check — `info.tag_type` and `@intFromEnum(T.x)` — maps
//! directly onto a field of this descriptor, assertion-for-assertion.

use crate::target::Target;

/// PORT NOTE: Models a Zig integer type for the synthesized enum's tag.
/// `c_int` is the C `int` ABI type; `U(bits)` is the smallest unsigned Zig
/// integer (`u<bits>`) fitting the value range. This mirrors
/// `@typeInfo(T).@"enum".tag_type` in the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagType {
    CInt,
    /// `u<bits>`, e.g. `U(2)` == Zig's `u2`.
    U(u16),
}

impl TagType {
    /// Zig's `u2`.
    pub const U2: TagType = TagType::U(2);
}

/// PORT NOTE: The runtime stand-in for the synthesized enum type. `tag_type`
/// is `@typeInfo(T).@"enum".tag_type`; `fields` are the `(name, value)` pairs
/// (`@intFromEnum(T.<name>)`).
#[derive(Debug, Clone)]
pub struct EnumType {
    pub tag_type: TagType,
    pub fields: Vec<(String, usize)>,
}

impl EnumType {
    /// `@intFromEnum(T.<name>)` for the given key.
    pub fn value(&self, name: &str) -> usize {
        for (n, v) in &self.fields {
            if n == name {
                return *v;
            }
        }
        unreachable!()
    }
}

/// PORT NOTE: Zig's `IntFittingRange(0, n)` — the smallest unsigned integer
/// type that can represent every value in `0..=n`.
fn int_fitting_range(to: usize) -> TagType {
    let mut bits: u16 = 0;
    while (1usize << bits) <= to {
        bits += 1;
    }
    TagType::U(bits)
}

/// Create an enum type with the given keys that is C ABI compatible
/// if we're targeting C, otherwise a Zig enum with smallest possible
/// backing type.
///
/// In all cases, each enum value is its index in `keys`. The order MUST NOT
/// be changed when the integer values are part of an ABI or serialized format.
/// A null key removes that field while preserving its integer hole.
///
/// C detection is up to the caller, since there are multiple ways
/// to do that. We rely on the `target` parameter to determine whether we
/// should create a C compatible enum or a Zig enum.
///
/// C enums use `c_int`. Zig enums use the smallest unsigned integer that can
/// represent every key index, including holes.
///
/// PORT NOTE (`e77c2612e`): upstream made Zig enum values STABLE — each key's
/// value is now its index in `keys` (`values_raw[to] = from`) for BOTH targets,
/// where previously the `.zig` target renumbered the retained keys densely
/// (`to`). The `.zig` backing integer now covers the full key range including
/// trailing holes (`IntFittingRange(0, keys.len - 1)`), not just the retained
/// count. Rustty builds a runtime `EnumType` descriptor rather than
/// synthesizing a type, so upstream's `@Enum`/`comptime_int`-widening half has
/// no analogue; the value/tag algorithm below follows the new upstream shape.
pub fn enum_type(target: Target, keys: &[Option<&str>]) -> EnumType {
    let mut names_actual: Vec<String> = Vec::with_capacity(keys.len());
    let mut values_actual: Vec<usize> = Vec::with_capacity(keys.len());
    // Remove null fields while preserving their positions as integer holes.
    for (from, key) in keys.iter().enumerate() {
        if let Some(key) = key {
            names_actual.push((*key).to_string());
            values_actual.push(from);
        }
    }

    let tag_int = match target {
        Target::C => TagType::CInt,
        Target::Zig => int_fitting_range(keys.len() - 1),
    };

    // PORT NOTE: upstream's `@Enum(TagInt, .exhaustive, names_actual, ...)` is
    // preceded by
    //   // We have to transform our comptime_int values into the actual int
    //   // we're creating the enum as.
    // — a `comptime_int` -> `TagInt` widening that Rust does not need, since
    // `values_actual` is already a concrete `usize` and `EnumType` stores the
    // tag width separately.
    EnumType {
        tag_type: tag_int,
        fields: names_actual.into_iter().zip(values_actual).collect(),
    }
}

/// Build an [`EnumType`] from an EXISTING enum's `(field name, @intFromEnum)`
/// pairs, rather than from `lib.Enum`'s key list.
///
/// PORT NOTE: `checkGhosttyHEnum(T, prefix)` reflects over whatever `T` is. Most
/// of its call sites pass a HAND-WRITTEN `enum(c_int)` (`apprt.Action.Key`,
/// `apprt.ipc.Target.Key`, `renderer.Health`, `terminal.MouseShape`, …), not a
/// type produced by `lib.Enum` — so `@typeInfo(T).@"enum".fields` yields the
/// enum's OWN declared values. [`enum_type`] models the `lib.Enum` FACTORY,
/// where each value is its index in the key list; that is the wrong model for a
/// reflected enum, because it re-derives the values instead of reading them.
///
/// This constructor is the reflected case: the caller supplies each field's
/// actual `<RustEnum>::<Variant> as i64`, so `check_ghostty_henum` compares the
/// header against the Rust enum's real discriminants. Without it the check
/// could not observe the enum at all — a transposition of two variants left
/// every `ghostty_h_*` test green (finding N29).
///
/// Panics on a negative discriminant: [`EnumType`] stores `usize` values, and
/// no reflected call site in this port has one (the reference's two
/// negative-valued enums, `GotoTab` and `ColorKind`, are exactly the two whose
/// `checkGhosttyHEnum` tests upstream has commented out).
pub fn enum_type_values(fields: &[(&str, i64)]) -> EnumType {
    let mut out: Vec<(String, usize)> = Vec::with_capacity(fields.len());
    for (name, value) in fields {
        assert!(
            *value >= 0,
            "enum_type_values: negative discriminant {} for `{}` — EnumType stores usize",
            value,
            name
        );
        out.push(((*name).to_string(), *value as usize));
    }
    EnumType {
        tag_type: TagType::CInt,
        fields: out,
    }
}

/// Verify that for every key in enum T, there is a matching declaration in
/// `rustty.h` with the correct value. This should only ever be called inside a `test`
/// because the `rustty.h` module is only available then.
///
/// PORT NOTE: The reference is a comptime function that imports `ghostty.h`
/// (the generated C header) and reflects over `T`. In rustty there is no such
/// header available to this leaf crate, so the C declarations are supplied by
/// the caller as a lookup closure `c` mapping an UPPER-CASE `<prefix><KEY>`
/// name to its declared C value (`None` == not declared, mirroring
/// `@hasDecl`). The algorithm — checking every declared key has the same
/// backing int and that no `rustty.h` value is left unmatched — is preserved
/// verbatim. Retargeted ghostty.h -> rustty.h per R2.
pub fn check_ghostty_henum<F>(t: &EnumType, prefix: &str, c: F) -> Result<(), String>
where
    F: Fn(&str) -> Option<i64>,
{
    // PORT NOTE: `info == .@"enum"` and `is_exhaustive == true` are guaranteed by
    // construction of `EnumType` (it always models an exhaustive enum). The
    // `tag_type == c_int` expect is a real check: [`henum_type!`] fills
    // `tag_type` from the enum's size ([`TagType::from_repr_size`]), so a
    // non-`c_int`-sized repr fails here. Residual: a size cannot tell `i32` from
    // `u32`, so a `#[repr(u32)]` enum still passes.
    //
    // CORRECTION (task 20): this note said all three facts, the `c_int` tag
    // included, "are guaranteed by construction of `EnumType`"; that stopped being
    // true when task 19's fix round derived the tag from the enum's size.
    if t.tag_type != TagType::CInt {
        return Err("expected c_int tag_type".to_string());
    }

    // @setEvalBranchQuota(100_000);

    // PORT NOTE: `var set: std.EnumSet(T) = .initFull();` — start with every
    // value present, removing each one we successfully match.
    let mut set: Vec<usize> = t.fields.iter().map(|(_, v)| *v).collect();

    let enum_fields = &t.fields;

    for (name, value) in enum_fields {
        let expected_name: String = {
            let mut buf = String::with_capacity(prefix.len() + name.len());
            buf.push_str(prefix);
            for ch in name.chars() {
                buf.push(ch.to_ascii_uppercase());
            }
            buf
        };

        if let Some(decl) = c(&expected_name) {
            if decl != *value as i64 {
                eprintln!(
                    "EnumType key {} does not have the same backing int as {}",
                    name, expected_name
                );
                return Err(format!(
                    "{} key {} does not have the same backing int as {}",
                    "EnumType", name, expected_name
                ));
            }

            if let Some(pos) = set.iter().position(|v| v == value) {
                set.remove(pos);
            }
        }
    }

    if !set.is_empty() {
        for v in &set {
            // Recover the key name for this leftover value.
            let upper_string: String = enum_fields
                .iter()
                .find(|(_, fv)| fv == v)
                .map(|(n, _)| n.to_ascii_uppercase())
                .unwrap_or_default();
            eprintln!("rustty.h is missing value for {}{}", prefix, upper_string);
        }
        return Err("rustty.h is missing values".to_string());
    }

    Ok(())
}

/// The compile-time checks shared by [`henum_type!`] and [`enum_name_table!`].
///
/// PORT NOTE: no reference counterpart (finding N36). Every argument is a
/// parallel slice, one entry per list row: the hand-written key, the Rust
/// variant's identifier (`stringify!`), and the variant's discriminant. Called
/// from a `const` item, so each `panic!` here is a COMPILE error at the macro's
/// call site — in whichever crate expands it. That matters: the
/// unreachable-pattern lint a duplicated `match` arm would raise is suppressed
/// for macros expanded from another crate, so a lint cannot be the defence.
///
/// 1. **The key names its variant.** The reference's key IS `@tagName`, so it
///    cannot disagree with the variant. Here the key is typed by hand, and the
///    two-edit mutation "transpose two variants, then swap the list's keys to
///    make the red test green" would otherwise pass while `Health::Healthy` is
///    1 in Rust and `HEALTHY` is 0 in C. Compared ignoring `_` and ASCII case,
///    because the Zig snake_case spellings are not a mechanical function of the
///    Rust idents (`digit_0` / `Digit0` but `f1` / `F1`, `osc8` / `Osc8`,
///    `export_terminal_io` / `ExportTerminalIO`).
/// 2. **No variant is listed twice** (equal discriminants — Rust forbids two
///    variants sharing one).
/// 3. **No key is listed twice.**
#[doc(hidden)]
pub const fn __henum_const_check(keys: &[&str], idents: &[&str], discs: &[i64]) {
    assert!(keys.len() == idents.len() && keys.len() == discs.len());
    let mut i = 0;
    while i < keys.len() {
        if !eq_ignoring_underscores_and_ascii_case(keys[i].as_bytes(), idents[i].as_bytes()) {
            panic!("henum/enum_name_table: a key does not name its variant (compared ignoring `_` and case) — the list pairs a key with the wrong variant");
        }
        let mut j = i + 1;
        while j < keys.len() {
            if discs[i] == discs[j] {
                panic!("henum/enum_name_table: a variant is listed twice");
            }
            if eq_bytes(keys[i].as_bytes(), keys[j].as_bytes()) {
                panic!("henum/enum_name_table: a key is listed twice");
            }
            j += 1;
        }
        i += 1;
    }
}

const fn eq_bytes(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

const fn eq_ignoring_underscores_and_ascii_case(a: &[u8], b: &[u8]) -> bool {
    let (mut i, mut j) = (0, 0);
    loop {
        while i < a.len() && a[i] == b'_' {
            i += 1;
        }
        while j < b.len() && b[j] == b'_' {
            j += 1;
        }
        if i == a.len() || j == b.len() {
            return i == a.len() && j == b.len();
        }
        if !a[i].eq_ignore_ascii_case(&b[j]) {
            return false;
        }
        i += 1;
        j += 1;
    }
}

impl TagType {
    /// The tag type an enum's REPRESENTATION implies: `c_int` when it is
    /// `c_int`-sized, otherwise `u<bits>` of its size.
    ///
    /// PORT NOTE (finding N36, task 19 fix round 1): the reference's
    /// `checkGhosttyHEnum` opens with `expect(info.tag_type == c_int)` against
    /// the enum's REAL tag type. The port's [`enum_type_values`] hardcodes
    /// [`TagType::CInt`], so that check could not fail — `#[repr(u8)]` on a
    /// header-synced enum stayed green. [`henum_type!`] now derives the tag
    /// from `size_of` of the enum (or of its C-value mapper's return type).
    ///
    /// Residual, by construction: a size cannot tell `i32` from `u32`, so
    /// `#[repr(u32)]` still reads as `c_int`. Rust exposes no way to observe
    /// an enum repr's signedness without a negative discriminant.
    pub fn from_repr_size(bytes: usize) -> TagType {
        if bytes == core::mem::size_of::<core::ffi::c_int>() {
            TagType::CInt
        } else {
            TagType::U((bytes * 8) as u16)
        }
    }
}

/// [`enum_type_values`] with the tag type observed from the enum rather than
/// assumed. Used by [`henum_type!`]; not for hand-built fixtures.
#[doc(hidden)]
pub fn __henum_type(tag_bytes: usize, fields: &[(&str, i64)]) -> EnumType {
    let mut t = enum_type_values(fields);
    t.tag_type = TagType::from_repr_size(tag_bytes);
    t
}

/// `size_of` the return type of a C-value mapper, and the mapper's values.
#[doc(hidden)]
pub fn __henum_mapped<E: Copy, R: Into<i64>>(mapper: fn(E) -> R, v: E) -> (usize, i64) {
    (core::mem::size_of::<R>(), mapper(v).into())
}

/// Build the [`EnumType`] a `checkGhosttyHEnum` port compares against the
/// header, from a hand-written `(key, Variant)` list that the compiler binds
/// to the enum.
///
/// PORT NOTE: no reference counterpart (finding N36). `lib.checkGhosttyHEnum`
/// reflects with `inline for (@typeInfo(T).@"enum".fields)` — the keys ARE
/// `@tagName`, the values ARE `@intFromEnum`, the tag type IS the enum's. Rust
/// has no such reflection, so every `check_ghostty_henum` caller writes the
/// list by hand, and each of those three facts became a thing the list could
/// get wrong silently. This macro makes each one observed:
///
/// - **roster** — an exhaustive `match` over `$E`, so a variant the list does
///   not name is a COMPILE error (an appended variant was invisible before);
/// - **values** — read out of the enum (`$E::$V as i64`, or a mapper), never
///   re-derived from list order (finding N29);
/// - **tag type** — `size_of` the enum (or the mapper's return type), so a
///   non-`c_int`-SIZED repr fails the check's opening `tag_type == c_int` test.
///   Residual: a size cannot tell `i32` from `u32`, so `#[repr(u32)]` passes
///   ([`TagType::from_repr_size`]; added to this bullet by task 20, where a reader
///   of the macro looks);
/// - **key ↔ variant, duplicates** — [`__henum_const_check`], at compile time.
///
/// Two forms: the default reads the discriminant; the second takes a
/// `fn($E) -> R` for enums whose C value comes from a method
/// (e.g. `ProgressReportState::int_from_enum`), and takes the tag type from `R`.
///
/// ```ignore
/// let t = rustty_lib::henum_type!(TargetKey, [("class", Class), ("detect", Detect)]);
/// ```
#[macro_export]
macro_rules! henum_type {
    ($E:ident, [ $( ($key:literal, $V:ident) ),* $(,)? ]) => {{
        #[allow(dead_code)]
        fn __henum_exhaustive(v: $E) -> &'static str {
            match v { $( $E::$V => $key, )* }
        }
        const _: () = $crate::r#enum::__henum_const_check(
            &[ $( $key ),* ],
            &[ $( stringify!($V) ),* ],
            &[ $( $E::$V as i64 ),* ],
        );
        $crate::r#enum::__henum_type(
            ::core::mem::size_of::<$E>(),
            &[ $( ($key, $E::$V as i64) ),* ],
        )
    }};
    ($E:ident, $value:expr, [ $( ($key:literal, $V:ident) ),* $(,)? ]) => {{
        #[allow(dead_code)]
        fn __henum_exhaustive(v: $E) -> &'static str {
            match v { $( $E::$V => $key, )* }
        }
        const _: () = $crate::r#enum::__henum_const_check(
            &[ $( $key ),* ],
            &[ $( stringify!($V) ),* ],
            &[ $( $E::$V as i64 ),* ],
        );
        let __henum_rows = [ $( ($key, $crate::r#enum::__henum_mapped($value, $E::$V)) ),* ];
        let __henum_bytes = ::core::mem::size_of::<$E>();
        let __henum_bytes = __henum_rows.first().map_or(__henum_bytes, |(_, (b, _))| *b);
        let __henum_fields: ::std::vec::Vec<(&str, i64)> =
            __henum_rows.iter().map(|(k, (_, v))| (*k, *v)).collect();
        $crate::r#enum::__henum_type(__henum_bytes, &__henum_fields)
    }};
}

/// Declare a `&[(Enum, &str)]` lookup table that the compiler keeps exhaustive.
///
/// PORT NOTE: no reference counterpart. The oracle reaches every variant of an
/// enum with `@tagName` / `inline for (@typeInfo(T).@"enum".fields)`; a Rust
/// port that replaces that reflection with a hand-written parallel table gains
/// a table that can fall behind the enum, and the failure shows up as a
/// runtime `unreachable!()` rather than as a build error. Wrapping the table in
/// this macro emits an exhaustive `match` beside it, so a new variant is a
/// COMPILE ERROR until the table names it, and runs [`__henum_const_check`]
/// over it, so a duplicated row or a name paired with the wrong variant is one
/// too. See finding N36.
///
/// The table body is written exactly as it would be without the macro:
///
/// ```ignore
/// rustty_lib::enum_name_table! {
///     /// docs
///     pub const TAG_NAMES: &[(Key, &str)] = &[
///         (Key::Unidentified, "unidentified"),
///     ];
/// }
/// ```
#[macro_export]
macro_rules! enum_name_table {
    (
        $(#[$meta:meta])*
        $vis:vis const $NAME:ident : &[($E:ident, &str)] = &[
            $( ($_e:ident :: $V:ident, $s:literal) ),* $(,)?
        ];
    ) => {
        $(#[$meta])*
        $vis const $NAME: &[($E, &str)] = &[ $( ($E::$V, $s) ),* ];

        const _: () = {
            #[allow(dead_code)]
            fn exhaustive(v: $E) -> &'static str {
                match v { $( $E::$V => $s, )* }
            }
            $crate::r#enum::__henum_const_check(
                &[ $( $s ),* ],
                &[ $( stringify!($V) ),* ],
                &[ $( $E::$V as i64 ),* ],
            );
        };
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `henum_type!` values are read OUT OF THE ENUM, not re-derived from
    /// the list's order, and the tag type is observed from the representation.
    ///
    /// PORT NOTE: no reference analogue. The roster, key-binding and duplicate
    /// halves cannot be negative-controlled from a test, because their failure
    /// mode is a COMPILE error; they were verified by real mutations at real
    /// call sites in other crates (finding N36).
    #[test]
    fn henum_type_reads_values_and_tag_from_the_enum() {
        #[derive(Clone, Copy)]
        #[repr(i32)]
        enum Probe {
            A = 3,
            B = 9,
        }
        let t = crate::henum_type!(Probe, [("a", A), ("b", B)]);
        assert_eq!(
            vec![("a".to_string(), 3usize), ("b".to_string(), 9usize)],
            t.fields
        );
        assert_eq!(TagType::CInt, t.tag_type);

        // The mapper form, for enums whose C value comes from a method; the tag
        // type is the mapper's return type.
        impl Probe {
            fn doubled(self) -> i32 {
                (self as i32) * 2
            }
            fn doubled_u8(self) -> u8 {
                (self as u8) * 2
            }
        }
        let mapped = crate::henum_type!(Probe, Probe::doubled, [("a", A), ("b", B)]);
        assert_eq!(
            vec![("a".to_string(), 6usize), ("b".to_string(), 18usize)],
            mapped.fields
        );
        assert_eq!(TagType::CInt, mapped.tag_type);
        let narrow = crate::henum_type!(Probe, Probe::doubled_u8, [("a", A), ("b", B)]);
        assert_eq!(TagType::U(8), narrow.tag_type);

        #[derive(Clone, Copy)]
        #[repr(u8)]
        enum Narrow {
            X,
        }
        let t = crate::henum_type!(Narrow, [("x", X)]);
        assert_eq!(TagType::U(8), t.tag_type);
        assert!(
            check_ghostty_henum(&t, "P_", |_| Some(0)).is_err(),
            "a u8 repr must fail the c_int check"
        );
    }

    #[test]
    fn henum_key_matching_ignores_underscores_and_case() {
        assert!(eq_ignoring_underscores_and_ascii_case(
            b"export_terminal_io",
            b"ExportTerminalIO"
        ));
        assert!(eq_ignoring_underscores_and_ascii_case(
            b"digit_0", b"Digit0"
        ));
        assert!(eq_ignoring_underscores_and_ascii_case(b"f1", b"F1"));
        assert!(!eq_ignoring_underscores_and_ascii_case(
            b"healthy",
            b"Unhealthy"
        ));
        assert!(!eq_ignoring_underscores_and_ascii_case(b"f1", b"F10"));
        assert!(!eq_ignoring_underscores_and_ascii_case(b"f10", b"F1"));
    }

    #[test]
    fn enum_name_table_declares_the_table() {
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        enum Probe2 {
            One,
            Two,
        }
        crate::enum_name_table! {
            /// doc comments survive the macro.
            const PROBE_NAMES: &[(Probe2, &str)] = &[
                (Probe2::One, "one"),
                (Probe2::Two, "two"),
            ];
        }
        assert_eq!(2, PROBE_NAMES.len());
        assert_eq!((Probe2::Two, "two"), PROBE_NAMES[1]);
    }

    #[test]
    fn zig() {
        let t = enum_type(Target::Zig, &[Some("a"), Some("b"), Some("c"), Some("d")]);
        let info = &t;
        assert_eq!(TagType::U(2), info.tag_type);
    }

    #[test]
    fn c() {
        let t = enum_type(Target::C, &[Some("a"), Some("b"), Some("c"), Some("d")]);
        let info = &t;
        assert_eq!(TagType::CInt, info.tag_type);
    }

    #[test]
    fn stable_values_when_removing_a_key() {
        // C
        {
            let t = enum_type(Target::C, &[Some("a"), Some("b"), None, Some("d")]);
            let info = &t;
            assert_eq!(TagType::CInt, info.tag_type);
            assert_eq!(3, info.value("d"));
        }

        // Zig
        {
            let t = enum_type(Target::Zig, &[Some("a"), Some("b"), None, Some("d")]);
            let info = &t;
            assert_eq!(TagType::U(2), info.tag_type);
            assert_eq!(3, info.value("d"));
        }
    }

    #[test]
    fn zig_backing_integer_includes_trailing_holes() {
        let t = enum_type(Target::Zig, &[Some("a"), None, None, None, None]);
        let info = &t;
        assert_eq!(TagType::U(3), info.tag_type);
        assert_eq!(0, info.value("a"));
    }

    #[test]
    fn zig_values_remain_stable_across_multiple_holes() {
        let t = enum_type(
            Target::Zig,
            &[None, Some("b"), None, Some("d"), None, Some("f")],
        );
        let info = &t;
        assert_eq!(TagType::U(3), info.tag_type);
        assert_eq!(1, info.value("b"));
        assert_eq!(3, info.value("d"));
        assert_eq!(5, info.value("f"));
    }
}
