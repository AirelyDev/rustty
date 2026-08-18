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
    // PORT NOTE: `info == .@"enum"` and `tag_type == c_int` and
    // `is_exhaustive == true` are guaranteed by construction of `EnumType`
    // (it always models an exhaustive enum); we assert the C-int tag here to
    // mirror the reference's three opening `expect`s.
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

#[cfg(test)]
mod tests {
    use super::*;

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
