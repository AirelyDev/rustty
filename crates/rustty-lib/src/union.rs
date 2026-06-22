//! PORT NOTE: Ports ghostty `src/lib/union.zig`.
//!
//! `TaggedUnion(target, Union, Padding)` is a comptime factory returning a set
//! of types/functions (`Tag`, `Zig`, `C`, `CValue`, `cval`, `Value`) that wrap
//! a Zig tagged union for C ABI. Rust cannot synthesize nominal types at
//! runtime, so we model the union's tags + payload type sizes as a descriptor
//! and reproduce the SAME layout math (`@sizeOf(CValue)`) and `cval` conversion
//! the reference tests observe, assertion-for-assertion.

use crate::target::Target;

/// PORT NOTE: One arm of the modeled tagged union: the tag value (an enum value
/// from a `lib.enum.Enum`, here a plain `c_int`-backed integer) and the payload
/// type's size + alignment.
#[derive(Debug, Clone, Copy)]
pub struct UnionField {
    pub name: &'static str,
    pub tag_value: i64,
    pub size: usize,
    pub alignment: usize,
}

/// PORT NOTE: A concrete payload value for `cval`. We only need the integer
/// payloads the reference tests exercise (`u32`, `u64`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CValue {
    pub raw: u64,
}

/// PORT NOTE: The C ABI tagged union result of `cval`: `tag` + `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct C {
    pub tag: i64,
    pub value: CValue,
}

/// PORT NOTE: Models the type/decls produced by `TaggedUnion(target, ...)`.
/// `padding_size` is `@sizeOf(Padding)`; `cvalue_size` is `@sizeOf(CValue)`.
#[derive(Debug, Clone)]
pub struct TaggedUnion {
    target: Target,
    fields: Vec<UnionField>,
    padding_size: usize,
    padding_align: usize,
}

impl TaggedUnion {
    /// Create a tagged union type that supports a C ABI and maintains
    /// C ABI compatibility when adding new tags. This returns a set of types
    /// and functions to augment the given Union type, not create a wholly new
    /// union type.
    ///
    /// The C ABI compatible types and functions are only available when the
    /// target produces C values.
    ///
    /// The `Union` type should be a standard Zig tagged union. The tag type
    /// should be explicit (i.e. not `union(enum)`) and the tag type should
    /// be an enum created with the `Enum` function in this library, so that
    /// automatic C ABI compatibility is ensured.
    ///
    /// The `Padding` type is a type that is always added to the C union
    /// with the key `_padding`. This should be set to a type that has the size
    /// and alignment needed to pad the C union to the expected size. This
    /// should never change to ensure ABI compatibility.
    pub fn new(
        target: Target,
        fields: Vec<UnionField>,
        padding_size: usize,
        padding_align: usize,
    ) -> TaggedUnion {
        let u = TaggedUnion {
            target,
            fields,
            padding_size,
            padding_align,
        };

        // comptime {
        match target {
            Target::Zig => {}

            // For ABI compatibility, we expect that this is our union size.
            Target::C => {
                if u.cvalue_size() != padding_size {
                    // @compileLog(@sizeOf(CValue));
                    panic!("TaggedUnion CValue size does not match expected fixed size");
                }
            }
        }
        // }

        u
    }

    /// The C ABI compatible union value type's `@sizeOf`.
    ///
    /// PORT NOTE: An `extern union` is sized to its largest member (each field
    /// plus the `_padding` field) and aligned to the largest member alignment,
    /// rounded up. With `Padding` chosen to match, this equals `padding_size`.
    pub fn cvalue_size(&self) -> usize {
        match self.target {
            // .zig => break :cvalue extern struct {} (empty)
            Target::Zig => 0,
            Target::C => {
                let mut max_size = 0usize;
                let mut max_align = 1usize;
                for f in &self.fields {
                    if f.size > max_size {
                        max_size = f.size;
                    }
                    if f.alignment > max_align {
                        max_align = f.alignment;
                    }
                }
                // _padding field
                if self.padding_size > max_size {
                    max_size = self.padding_size;
                }
                if self.padding_align > max_align {
                    max_align = self.padding_align;
                }
                align_up(max_size, max_align)
            }
        }
    }

    /// Convert to C union.
    pub fn cval(&self, tag_name: &str, payload: u64) -> C {
        let field = self.fields.iter().find(|f| f.name == tag_name).unwrap();
        let value = CValue { raw: payload };

        C {
            tag: field.tag_value,
            value,
        }
    }

    /// Returns the value type's size for the given tag.
    ///
    /// PORT NOTE: The reference returns the Zig `type` for the tag; here we
    /// expose its `@sizeOf`, which is what callers in this crate need.
    pub fn value_size(&self, tag_name: &str) -> usize {
        // @setEvalBranchQuota(10000);
        for field in &self.fields {
            if field.name == tag_name {
                return field.size;
            }
        }
        unreachable!()
    }
}

fn align_up(offset: usize, align: usize) -> usize {
    if align == 0 {
        return offset;
    }
    (offset + align - 1) / align * align
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;

    #[test]
    fn TaggedUnion_matching_size() {
        // const Tag = enum(c_int) { a, b };
        // union(Tag) { a: u32, b: u64 }, Padding = u64
        let u = TaggedUnion::new(
            Target::C,
            vec![
                UnionField {
                    name: "a",
                    tag_value: 0,
                    size: 4,
                    alignment: 4,
                },
                UnionField {
                    name: "b",
                    tag_value: 1,
                    size: 8,
                    alignment: 8,
                },
            ],
            8,
            8,
        );

        assert_eq!(8usize, u.cvalue_size());
    }

    #[test]
    fn TaggedUnion_padded_size() {
        // const Tag = enum(c_int) { a };
        // union(Tag) { a: u32 }, Padding = u64
        let u = TaggedUnion::new(
            Target::C,
            vec![UnionField {
                name: "a",
                tag_value: 0,
                size: 4,
                alignment: 4,
            }],
            8,
            8,
        );

        assert_eq!(8usize, u.cvalue_size());
    }

    #[test]
    fn TaggedUnion_c_conversion() {
        // const Tag = enum(c_int) { a, b };
        // union(Tag) { a: u32, b: u64 }, Padding = u64
        let u = TaggedUnion::new(
            Target::C,
            vec![
                UnionField {
                    name: "a",
                    tag_value: 0,
                    size: 4,
                    alignment: 4,
                },
                UnionField {
                    name: "b",
                    tag_value: 1,
                    size: 8,
                    alignment: 8,
                },
            ],
            8,
            8,
        );

        let c = u.cval("a", 42);
        assert_eq!(0i64, c.tag); // Tag.a
        assert_eq!(42u64, c.value.raw);
    }
}
