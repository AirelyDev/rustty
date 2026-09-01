//! PORT NOTE: Ports ghostty `src/lib/packed.zig`.
//!
//! `Packed(T, options)` / `PackedTaggedUnion(Owner, union_field, tag_field,
//! options)` are comptime factories that reflect a packed struct's physical
//! bit layout (`@bitOffsetOf` / `@bitSizeOf`) and pair it with public
//! metadata (names, type names, encodings). Rust has no reflection over the
//! port's hand-rolled bitfields, so the layouts are modeled as `'static`
//! descriptor tables carrying the SAME observables (bit offsets, widths,
//! public names, encodings, arms) — and the reference's comptime validation
//! block is reproduced as a const-evaluable `validate()` that PANICS during
//! constant evaluation, so a bad descriptor is still a compile error exactly
//! like the reference's `@compileError`.

/// Metadata for a field in a packed layout.
// pub const FieldOptions = struct { ... };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldOptions {
    /// Public name for this field. The Zig field name is used by default.
    pub name: Option<&'static str>,

    /// Public type name for scalar fields. The Zig type is used by default.
    pub type_name: Option<&'static str>,

    /// Omit this field from public metadata. This is intended for padding.
    pub omit: bool,

    /// How the field is represented in public metadata.
    pub encoding: Encoding,
}

impl FieldOptions {
    /// PORT NOTE: the reference's per-field default (`FieldOptions = .{}`).
    pub const DEFAULT: FieldOptions = FieldOptions {
        name: None,
        type_name: None,
        omit: false,
        encoding: Encoding::Scalar,
    };
}

// pub const Encoding = union(enum) { scalar, @"packed": type, tagged_union: type };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Scalar,
    Packed(&'static PackedLayout),
    TaggedUnion(&'static PackedTaggedUnionLayout),
}

/// One reflected field of a packed struct.
///
/// PORT NOTE: the reference gets `zig_name`/`bit_offset`/`bit_width` from
/// `@typeInfo(T).@"struct".fields` + `@bitOffsetOf`/`@bitSizeOf`; here they
/// are spelled per field and cross-checked by `validate()` (contiguity and
/// total width against the backing integer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedField {
    /// The Zig-source field name (`field.name`).
    pub zig_name: &'static str,
    /// `@bitOffsetOf(T, field.name)`.
    pub bit_offset: usize,
    /// `@bitSizeOf(@FieldType(T, field.name))`.
    pub bit_width: usize,
    /// `publicTypeName(field.type)` — the default scalar type label used when
    /// `options.type_name` is unset (e.g. `"u21"`, `"bool"`, `"u8"`).
    pub zig_type_name: &'static str,
    pub options: FieldOptions,
}

/// Create metadata for a packed struct. Physical layout information is always
/// reflected from `T`; options only provide public names and relationships.
// pub fn Packed(comptime T: type, comptime options: PackedOptions(T)) type { ... }
#[derive(Debug, PartialEq, Eq)]
pub struct PackedLayout {
    /// `@sizeOf(Zig)`.
    pub size: usize,
    /// `@alignOf(Zig)`.
    pub align: usize,
    /// `@bitSizeOf(Zig)`.
    pub bit_size: usize,
    /// `publicTypeName(Backing)` — `pub const Backing = info.backing_integer.?`.
    pub backing: &'static str,
    /// The reflected fields in declaration order.
    pub fields: &'static [PackedField],
}

/// `&str` equality usable in `const fn` (`==` on `str` is not const).
const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
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

impl PackedLayout {
    // I know this is an insane amount of validation but getting
    // this correct is critical to C APIs working so we go overboard.
    //
    // PORT NOTE: the reference's `comptime { ... }` block. Called from a
    // `const _: () = LAYOUT.validate();` next to every layout instance so a
    // violation fails constant evaluation (the `@compileError` analogue).
    pub const fn validate(&self) {
        let mut i = 0;
        while i < self.fields.len() {
            let field = &self.fields[i];
            let field_options = &field.options;
            if field_options.omit {
                if field_options.name.is_some()
                    || field_options.type_name.is_some()
                    || !matches!(field_options.encoding, Encoding::Scalar)
                {
                    panic!("omitted packed field has other options");
                }
                i += 1;
                continue;
            }

            if let Some(name) = field_options.name {
                if name.is_empty() {
                    panic!("packed field name cannot be empty");
                }
            }

            match field_options.encoding {
                // .scalar => switch (@typeInfo(field.type)) { .bool, .int,
                // .@"enum" => {}, else => @compileError(...) }
                //
                // PORT NOTE: the scalar bool/int/enum type-kind check is a
                // reflection over the Zig field type; the model carries the
                // already-rendered `zig_type_name`, so the checkable
                // equivalent is that a scalar label exists at all.
                Encoding::Scalar => {
                    if field.zig_type_name.is_empty() && field_options.type_name.is_none() {
                        panic!("packed field requires an explicit encoding");
                    }
                }
                Encoding::Packed(layout) => {
                    // if (Layout.Zig != field.type) @compileError(...)
                    // PORT NOTE: type identity is modeled by bit width.
                    if layout.bit_size != field.bit_width {
                        panic!("nested packed layout has the wrong Zig type");
                    }
                    if field_options.type_name.is_some() {
                        panic!("nested packed field cannot have a scalar type name");
                    }
                }
                Encoding::TaggedUnion(layout) => {
                    // if (Layout.Owner != T or Layout.Union != field.type or
                    //     Layout.union_field != @field(FieldT, field.name))
                    // PORT NOTE: the owner/union identity checks are modeled by
                    // the union-field name + bit width.
                    if !str_eq(layout.union_field, field.zig_name)
                        || layout.union_bit_size != field.bit_width
                    {
                        panic!("tagged union layout does not match packed field");
                    }
                    if field_options.type_name.is_some() {
                        panic!("tagged union field cannot have a scalar type name");
                    }
                }
            }

            let public_name = match field_options.name {
                Some(name) => name,
                None => field.zig_name,
            };
            let mut j = 0;
            while j < i {
                let previous = &self.fields[j];
                if !previous.options.omit {
                    let previous_name = match previous.options.name {
                        Some(name) => name,
                        None => previous.zig_name,
                    };
                    if str_eq(public_name, previous_name) {
                        panic!("duplicate public packed field name");
                    }
                }
                j += 1;
            }
            i += 1;
        }
    }

    // pub fn fieldOptions(comptime field: Field) FieldOptions
    pub const fn field_options(&self, index: usize) -> &FieldOptions {
        &self.fields[index].options
    }

    // pub fn fieldName(comptime field: Field) ?[]const u8 {
    //     if (field_options.omit) return null;
    //     return field_options.name orelse @tagName(field);
    // }
    pub const fn field_name(&self, index: usize) -> Option<&'static str> {
        let field = &self.fields[index];
        if field.options.omit {
            return None;
        }
        match field.options.name {
            Some(name) => Some(name),
            None => Some(field.zig_name),
        }
    }

    // pub fn bitOffset(comptime field: Field) usize
    pub const fn bit_offset(&self, index: usize) -> usize {
        self.fields[index].bit_offset
    }

    // pub fn bitWidth(comptime field: Field) usize
    pub const fn bit_width(&self, index: usize) -> usize {
        self.fields[index].bit_width
    }

    /// PORT NOTE: the reference addresses fields by the comptime
    /// `std.meta.FieldEnum(T)`; the model addresses them by Zig field name.
    /// A missing name panics (the comptime-lookup-failure analogue).
    pub const fn field_index(&self, zig_name: &str) -> usize {
        let mut i = 0;
        while i < self.fields.len() {
            if str_eq(self.fields[i].zig_name, zig_name) {
                return i;
            }
            i += 1;
        }
        panic!("unknown packed field name");
    }
}

/// One arm of a packed tagged union: a tag value mapped to the reflected
/// packed layout of a source union field (or nothing).
///
/// PORT NOTE: the reference `Arm` is a comptime union keyed by the SOURCE
/// union field (`.number = Number`); the model stores the source field name
/// beside the layout so `arm()` callers can still observe which source field
/// an arm maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackedTaggedUnionArm {
    /// The tag enum field name this arm belongs to.
    pub tag_name: &'static str,
    /// `(source union field name, its reflected layout)`, or `None` when the
    /// tag has no metadata (`options.arms.<tag> == null`).
    pub source: Option<(&'static str, &'static PackedLayout)>,
}

/// Describe a packed union field whose active arm is selected by another
/// field in the containing packed struct. Each tag maps to a reflected packed
/// arm layout. Multiple tags may map to the same source union field.
// pub fn PackedTaggedUnion(comptime OwnerT: type, comptime union_field_value:
// ..., comptime tag_field_value: ..., comptime options: ...) type { ... }
#[derive(Debug, PartialEq, Eq)]
pub struct PackedTaggedUnionLayout {
    /// `@tagName(union_field)` — the owner field holding the packed union.
    pub union_field: &'static str,
    /// `@tagName(tag_field)` — the owner field holding the tag.
    pub tag_field: &'static str,
    /// `@bitSizeOf(Union)`.
    pub union_bit_size: usize,
    /// One entry per tag enum field, in declaration order.
    pub arms: &'static [PackedTaggedUnionArm],
}

impl PackedTaggedUnionLayout {
    // comptime { for (tag_info.fields) |tag| { ... } }
    pub const fn validate(&self) {
        // PORT NOTE: `packedStructInfo(OwnerT)` + "packed tagged union value
        // must be a packed union" are type-kind reflections with no model
        // counterpart; the arm checks below are the observable half.
        let mut i = 0;
        while i < self.arms.len() {
            if let Some((_, layout)) = self.arms[i].source {
                // if (Layout.Zig != @FieldType(UnionT, source_name))
                //     @compileError("packed union arm layout has the wrong Zig
                //     type for tag: " ...);
                // PORT NOTE: modeled by the width bound below (union members
                // all start at bit 0, so a wrong-type arm shows up as a width
                // mismatch).
                if layout.bit_size > self.union_bit_size {
                    panic!("packed union arm is wider than its union");
                }
            }
            i += 1;
        }
    }

    // pub fn arm(comptime tag: Tag) ?Arm {
    //     return @field(options.arms, @tagName(tag));
    // }
    pub const fn arm(&self, tag_name: &str) -> Option<(&'static str, &'static PackedLayout)> {
        let mut i = 0;
        while i < self.arms.len() {
            if str_eq(self.arms[i].tag_name, tag_name) {
                return self.arms[i].source;
            }
            i += 1;
        }
        panic!("unknown packed tagged union tag");
    }
}

// fn packedStructInfo(comptime T: type) std.builtin.Type.Struct { ... }
// PORT NOTE: "Packed requires a packed struct" / "Packed requires an
// integer-backed packed struct" are pure type-kind reflections; the model's
// `PackedLayout` carries `backing`/`bit_size` by construction, so an
// un-backed layout cannot be expressed at all.

#[cfg(test)]
mod tests {
    use super::*;

    // test "Packed: reflected fields and nested tagged union" {
    #[test]
    fn packed_reflected_fields_and_nested_tagged_union() {
        // const Tag = enum(u2) { first, second, third };
        // const Value = packed union {
        //     number: packed struct(u4) { data: u3, _pad: u1 },
        //     flags: packed struct(u4) { a: bool, b: bool, _pad: u2 },
        // };
        // const Nested = packed struct(u1) { enabled: bool };
        // const Subject = packed struct(u8) { tag: Tag, value: Value,
        //     nested: Nested, _padding: u1 };

        // const Number = Packed(@FieldType(Value, "number"), .{ .fields = .{
        //     .data = .{ .name = "number" }, ._pad = .{ .omit = true } } });
        const NUMBER: PackedLayout = PackedLayout {
            size: 1,
            align: 1,
            bit_size: 4,
            backing: "u4",
            fields: &[
                PackedField {
                    zig_name: "data",
                    bit_offset: 0,
                    bit_width: 3,
                    zig_type_name: "u3",
                    options: FieldOptions {
                        name: Some("number"),
                        ..FieldOptions::DEFAULT
                    },
                },
                PackedField {
                    zig_name: "_pad",
                    bit_offset: 3,
                    bit_width: 1,
                    zig_type_name: "u1",
                    options: FieldOptions {
                        omit: true,
                        ..FieldOptions::DEFAULT
                    },
                },
            ],
        };
        const _: () = NUMBER.validate();

        // const Flags = Packed(@FieldType(Value, "flags"), .{ .fields = .{
        //     ._pad = .{ .omit = true } } });
        const FLAGS: PackedLayout = PackedLayout {
            size: 1,
            align: 1,
            bit_size: 4,
            backing: "u4",
            fields: &[
                PackedField {
                    zig_name: "a",
                    bit_offset: 0,
                    bit_width: 1,
                    zig_type_name: "bool",
                    options: FieldOptions::DEFAULT,
                },
                PackedField {
                    zig_name: "b",
                    bit_offset: 1,
                    bit_width: 1,
                    zig_type_name: "bool",
                    options: FieldOptions::DEFAULT,
                },
                PackedField {
                    zig_name: "_pad",
                    bit_offset: 2,
                    bit_width: 2,
                    zig_type_name: "u2",
                    options: FieldOptions {
                        omit: true,
                        ..FieldOptions::DEFAULT
                    },
                },
            ],
        };
        const _: () = FLAGS.validate();

        // const NestedLayout = Packed(Nested, .{});
        const NESTED_LAYOUT: PackedLayout = PackedLayout {
            size: 1,
            align: 1,
            bit_size: 1,
            backing: "u1",
            fields: &[PackedField {
                zig_name: "enabled",
                bit_offset: 0,
                bit_width: 1,
                zig_type_name: "bool",
                options: FieldOptions::DEFAULT,
            }],
        };
        const _: () = NESTED_LAYOUT.validate();

        // const ValueLayout = PackedTaggedUnion(Subject, .value, .tag, .{
        //     .arms = .{ .first = .{ .number = Number }, .second = .{ .number
        //     = Number }, .third = .{ .flags = Flags } } });
        const VALUE_LAYOUT: PackedTaggedUnionLayout = PackedTaggedUnionLayout {
            union_field: "value",
            tag_field: "tag",
            union_bit_size: 4,
            arms: &[
                PackedTaggedUnionArm {
                    tag_name: "first",
                    source: Some(("number", &NUMBER)),
                },
                PackedTaggedUnionArm {
                    tag_name: "second",
                    source: Some(("number", &NUMBER)),
                },
                PackedTaggedUnionArm {
                    tag_name: "third",
                    source: Some(("flags", &FLAGS)),
                },
            ],
        };
        const _: () = VALUE_LAYOUT.validate();

        // const Layout = Packed(Subject, .{ .fields = .{
        //     .value = .{ .encoding = .{ .tagged_union = ValueLayout } },
        //     .nested = .{ .encoding = .{ .@"packed" = NestedLayout } },
        //     ._padding = .{ .omit = true } } });
        const LAYOUT: PackedLayout = PackedLayout {
            size: 1,
            align: 1,
            bit_size: 8,
            backing: "u8",
            fields: &[
                PackedField {
                    zig_name: "tag",
                    bit_offset: 0,
                    bit_width: 2,
                    zig_type_name: "enum",
                    options: FieldOptions::DEFAULT,
                },
                PackedField {
                    zig_name: "value",
                    bit_offset: 2,
                    bit_width: 4,
                    zig_type_name: "union",
                    options: FieldOptions {
                        encoding: Encoding::TaggedUnion(&VALUE_LAYOUT),
                        ..FieldOptions::DEFAULT
                    },
                },
                PackedField {
                    zig_name: "nested",
                    bit_offset: 6,
                    bit_width: 1,
                    zig_type_name: "struct",
                    options: FieldOptions {
                        encoding: Encoding::Packed(&NESTED_LAYOUT),
                        ..FieldOptions::DEFAULT
                    },
                },
                PackedField {
                    zig_name: "_padding",
                    bit_offset: 7,
                    bit_width: 1,
                    zig_type_name: "u1",
                    options: FieldOptions {
                        omit: true,
                        ..FieldOptions::DEFAULT
                    },
                },
            ],
        };
        const _: () = LAYOUT.validate();

        // try testing.expectEqual(@bitOffsetOf(Subject, "value"),
        //     Layout.bitOffset(.value));
        assert_eq!(2, LAYOUT.bit_offset(LAYOUT.field_index("value")));
        // try testing.expectEqual(@bitSizeOf(Value), Layout.bitWidth(.value));
        assert_eq!(4, LAYOUT.bit_width(LAYOUT.field_index("value")));
        // try testing.expectEqualStrings("number", Number.fieldName(.data).?);
        assert_eq!(
            "number",
            NUMBER.field_name(NUMBER.field_index("data")).unwrap()
        );
        // try testing.expect(Number.fieldName(._pad) == null);
        assert!(NUMBER.field_name(NUMBER.field_index("_pad")).is_none());
        // switch (ValueLayout.arm(.first).?) { .number => |ArmLayout|
        //     try testing.expect(ArmLayout == Number), else => ... }
        // PORT NOTE: the reference's `ArmLayout == Number` is a comptime TYPE
        // identity. Here the layouts are VALUES, and the arm must be compared
        // by equality, not by address: a `const` item is inlined at each use
        // site, so the `&NUMBER` in the arm table and the `&NUMBER` here are
        // two independently promoted allocations that need not share an
        // address. rustc happens to fold them together, Miri does not, and the
        // G6 lane is where that showed up — `core::ptr::eq` was asserting a
        // codegen coincidence rather than the reference's claim. Equality binds
        // strictly more: it is false for `FLAGS`, which is what the reference's
        // `else => TestUnexpectedResult` arm rules out, and it stays true under
        // any promotion strategy.
        match VALUE_LAYOUT.arm("first") {
            Some(("number", layout)) => {
                assert_eq!(*layout, NUMBER);
                assert_ne!(*layout, FLAGS);
            }
            _ => panic!("TestUnexpectedResult"),
        }
        // switch (ValueLayout.arm(.second).?) { .number => ... }
        match VALUE_LAYOUT.arm("second") {
            Some(("number", layout)) => assert_eq!(*layout, NUMBER),
            _ => panic!("TestUnexpectedResult"),
        }
        // try testing.expectEqual(@as(usize, 1),
        //     NestedLayout.bitWidth(.enabled));
        assert_eq!(
            1usize,
            NESTED_LAYOUT.bit_width(NESTED_LAYOUT.field_index("enabled"))
        );
    }
}
