//! PORT NOTE: Ports ghostty `src/lib/union.zig`.
//!
//! `TaggedUnion(target, Union, options)` is a comptime factory returning a set
//! of types/functions (`Tag`, `Zig`, `C`, `CValue`, `cval`, `Value`) that wrap
//! a Zig tagged union for C ABI. Rust cannot synthesize nominal types at
//! runtime, so we model the union's tags + payload type sizes as a descriptor
//! and reproduce the SAME layout math (`@sizeOf(CValue)`), `cval` conversion
//! and `cFieldRename` metadata the reference tests observe,
//! assertion-for-assertion.

use crate::target::Target;

/// PORT NOTE: One arm of the modeled tagged union: the tag value (an enum value
/// from a `lib.enum.Enum`, here a plain `c_int`-backed integer) and the payload
/// type's size + alignment. `rename` models the corresponding
/// `options.field_renames` entry (default null).
#[derive(Debug, Clone, Copy)]
pub struct UnionField {
    pub name: &'static str,
    pub tag_value: i64,
    pub size: usize,
    pub alignment: usize,
    pub rename: Option<&'static str>,
}

/// The C ABI compatible union value type.
///
/// PORT NOTE: A concrete payload value for `cval`. We only need the integer
/// payloads the reference tests exercise (`u32`, `u64`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CValue {
    pub raw: u64,
}

/// The C ABI compatible tagged union type.
///
/// PORT NOTE: The result of `cval`: `tag` + `value`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct C {
    pub tag: i64,
    pub value: CValue,
}

/// Options for generating the C representation of a tagged union.
///
/// PORT NOTE: `TaggedUnionOptions(Union)` synthesizes a per-tag
/// `field_renames` option struct at comptime; the model carries each tag's
/// rename on its `UnionField` (`rename`, default `None`) and the padding
/// type's size/alignment here (`@sizeOf`/`@alignOf(options.padding)`).
#[derive(Debug, Clone, Copy)]
pub struct TaggedUnionOptions {
    pub padding_size: usize,
    pub padding_align: usize,
}

/// PORT NOTE: Models the type/decls produced by `TaggedUnion(target, ...)`.
/// `padding_size` is `@sizeOf(options.padding)`; `cvalue_size` is
/// `@sizeOf(CValue)`.
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
    /// `options.padding` is a type that is always added to the C union with the
    /// key `_padding`. It should have the size and alignment needed to pad the
    /// C union to the expected size and should never change to ensure ABI
    /// compatibility.
    ///
    /// Each tag has an optional field in `options.field_renames` that may
    /// rename its C value field. Multiple tags may map to the same field when
    /// their C value types are identical. Tags with no payload are omitted.
    /// This is used for metadata and should match the C headers directly.
    pub fn new(
        target: Target,
        fields: Vec<UnionField>,
        options: TaggedUnionOptions,
    ) -> TaggedUnion {
        let u = TaggedUnion {
            target,
            fields,
            padding_size: options.padding_size,
            padding_align: options.padding_align,
        };

        // comptime {
        match target {
            Target::Zig => {}

            // For ABI compatibility, we expect that this is our union size.
            Target::C => {
                if u.cvalue_size() != options.padding_size {
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
                    // Types can provide custom types for their CValue.
                    //
                    // PORT NOTE: in the reference, for each tag field whose
                    // payload type is an enum/struct/union that declares a `C`
                    // decl, the `CValue` union member uses `Type.C` instead of
                    // `Type`. Our descriptor stores each field's already-resolved
                    // (`Type.C` when present) size/alignment in `UnionField`.
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

    /// Returns the public C value-union field name for `tag`, or
    /// null when the tag has no value field. This is metadata only;
    /// it does not rename fields in `CValue`.
    // pub fn cFieldRename(comptime tag: Tag) ?[]const u8 {
    //     const tag_name = @tagName(tag);
    //     const value = @field(@unionInit(Union, tag_name, undefined), tag_name);
    //     if (@field(options.field_renames, tag_name)) |name| return name;
    //     if (@sizeOf(@TypeOf(value)) == 0) return null;
    //     return tag_name;
    // }
    pub fn c_field_rename(&self, tag_name: &str) -> Option<&'static str> {
        let field = self.fields.iter().find(|f| f.name == tag_name).unwrap();
        if let Some(name) = field.rename {
            return Some(name);
        }
        if field.size == 0 {
            return None;
        }
        Some(field.name)
    }

    /// PORT NOTE: `@hasField(U.CValue, name)` over the synthesized C value
    /// union: its members are the TAG names plus `_padding` — the renames are
    /// metadata only and never rename `CValue` fields.
    pub fn has_cvalue_field(&self, name: &str) -> bool {
        name == "_padding" || self.fields.iter().any(|f| f.name == name)
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

    /// PORT NOTE: shorthand for a `UnionField` with no rename (the synthesized
    /// `field_renames` default).
    fn field(name: &'static str, tag_value: i64, size: usize, alignment: usize) -> UnionField {
        UnionField {
            name,
            tag_value,
            size,
            alignment,
            rename: None,
        }
    }

    #[test]
    fn TaggedUnion_matching_size() {
        // const Tag = enum(c_int) { a, b };
        // union(Tag) { a: u32, b: u64 }, .{ .padding = u64 }
        let u = TaggedUnion::new(
            Target::C,
            vec![field("a", 0, 4, 4), field("b", 1, 8, 8)],
            TaggedUnionOptions {
                padding_size: 8,
                padding_align: 8,
            },
        );

        assert_eq!(8usize, u.cvalue_size());
    }

    #[test]
    fn TaggedUnion_padded_size() {
        // const Tag = enum(c_int) { a };
        // union(Tag) { a: u32 }, .{ .padding = u64 }
        let u = TaggedUnion::new(
            Target::C,
            vec![field("a", 0, 4, 4)],
            TaggedUnionOptions {
                padding_size: 8,
                padding_align: 8,
            },
        );

        assert_eq!(8usize, u.cvalue_size());
    }

    #[test]
    fn TaggedUnion_c_conversion() {
        // const Tag = enum(c_int) { a, b };
        // union(Tag) { a: u32, b: u64 }, .{ .padding = u64 }
        let u = TaggedUnion::new(
            Target::C,
            vec![field("a", 0, 4, 4), field("b", 1, 8, 8)],
            TaggedUnionOptions {
                padding_size: 8,
                padding_align: 8,
            },
        );

        let c = u.cval("a", 42);
        assert_eq!(0i64, c.tag); // Tag.a
        assert_eq!(42u64, c.value.raw);
    }

    // test "TaggedUnion: custom C value fields" {
    #[test]
    fn TaggedUnion_custom_C_value_fields() {
        // const Tag = enum(c_int) { a, b, none };
        // const Union = union(Tag) { a: u32, b: u32, none };
        // const U = TaggedUnion(.c, Union, .{
        //     .padding = u64,
        //     .field_renames = .{ .a = "value", .b = "value" },
        // });
        let u = TaggedUnion::new(
            Target::C,
            vec![
                UnionField {
                    name: "a",
                    tag_value: 0,
                    size: 4,
                    alignment: 4,
                    rename: Some("value"),
                },
                UnionField {
                    name: "b",
                    tag_value: 1,
                    size: 4,
                    alignment: 4,
                    rename: Some("value"),
                },
                field("none", 2, 0, 1),
            ],
            TaggedUnionOptions {
                padding_size: 8,
                padding_align: 8,
            },
        );

        // try testing.expect(!@hasField(U.CValue, "value"));
        assert!(!u.has_cvalue_field("value"));
        // try testing.expect(@hasField(U.CValue, "a"));
        assert!(u.has_cvalue_field("a"));
        // try testing.expect(@hasField(U.CValue, "b"));
        assert!(u.has_cvalue_field("b"));
        // try testing.expect(@hasField(U.CValue, "none"));
        assert!(u.has_cvalue_field("none"));
        // try testing.expectEqualStrings("value", U.C.cFieldRename(.a).?);
        assert_eq!("value", u.c_field_rename("a").unwrap());
        // try testing.expect(U.C.cFieldRename(.none) == null);
        assert!(u.c_field_rename("none").is_none());

        // const a = U.cval(.{ .a = 42 });
        let a = u.cval("a", 42);
        // try testing.expectEqual(Tag.a, a.tag);
        assert_eq!(0i64, a.tag);
        // try testing.expectEqual(42, a.value.a);
        assert_eq!(42u64, a.value.raw);

        // const none = U.cval(.none);
        let none = u.cval("none", 0);
        // try testing.expectEqual(Tag.none, none.tag);
        assert_eq!(2i64, none.tag);
    }
}
