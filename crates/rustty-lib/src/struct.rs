//! PORT NOTE: Ports ghostty `src/lib/struct.zig`.
//!
//! `Struct(target, Zig)` is a comptime function that, for `.c`, rebuilds a Zig
//! struct as an `extern` (C-layout) struct, resolving any zero alignments to
//! the field type's natural alignment. Rust cannot synthesize a nominal type at
//! runtime, so we model a struct as an ordered list of `FieldInfo` and
//! reproduce the SAME conversion algorithm + layout math, so the reference
//! tests (`info.layout`, `@sizeOf(@FieldType(...))`, `@sizeOf(C)`) map onto
//! descriptor fields assertion-for-assertion.

use crate::target::Target;

/// PORT NOTE: A single field of the modeled struct, mirroring
/// `std.builtin.Type.StructField`. `size`/`natural_align` describe the field's
/// type; `alignment` is the field's declared alignment (0 == "use natural",
/// as Zig packed-struct fields report).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldInfo {
    pub name: &'static str,
    pub size: usize,
    pub natural_align: usize,
    pub alignment: usize,
}

/// PORT NOTE: Models a struct type: its memory `layout` and ordered `fields`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    Auto,
    Extern,
    Packed,
}

#[derive(Debug, Clone)]
pub struct StructType {
    pub layout: Layout,
    pub fields: Vec<FieldInfo>,
    pub is_tuple: bool,
}

impl StructType {
    /// `@FieldType(T, name)`'s `@sizeOf`.
    pub fn field_size(&self, name: &str) -> usize {
        self.fields.iter().find(|f| f.name == name).unwrap().size
    }

    /// `@offsetOf(T, name)` computed for the (extern) layout.
    pub fn offset_of(&self, name: &str) -> usize {
        let mut offset = 0usize;
        for f in &self.fields {
            let align = if f.alignment > 0 {
                f.alignment
            } else {
                f.natural_align
            };
            offset = align_up(offset, align);
            if f.name == name {
                return offset;
            }
            offset += f.size;
        }
        unreachable!()
    }

    /// `@sizeOf(T)` computed for the (extern) layout, padded to the struct's
    /// own alignment (the max field alignment).
    pub fn size_of(&self) -> usize {
        let mut offset = 0usize;
        let mut max_align = 1usize;
        for f in &self.fields {
            let align = if f.alignment > 0 {
                f.alignment
            } else {
                f.natural_align
            };
            if align > max_align {
                max_align = align;
            }
            offset = align_up(offset, align);
            offset += f.size;
        }
        align_up(offset, max_align)
    }
}

fn align_up(offset: usize, align: usize) -> usize {
    if align == 0 {
        return offset;
    }
    (offset + align - 1) / align * align
}

/// Create a struct type that is C ABI compatible from a Zig struct type.
///
/// When the target is `.zig`, the original struct type is returned as-is.
/// When the target is `.c`, the struct is recreated with an `extern` layout,
/// ensuring a stable, C-compatible memory layout.
///
/// This handles packed structs by resolving zero alignments to the natural
/// alignment of each field's type, since extern structs require explicit
/// alignment. This means packed struct fields like `bool` will take up
/// their full size (1 byte) rather than being bit-packed.
pub fn struct_type(target: Target, zig: &StructType) -> StructType {
    match target {
        Target::Zig => zig.clone(),
        Target::C => {
            let info = zig;
            let mut fields: Vec<FieldInfo> = Vec::with_capacity(info.fields.len());
            for field in &info.fields {
                fields.push(FieldInfo {
                    name: field.name,
                    size: field.size,
                    natural_align: field.natural_align,
                    alignment: if field.alignment > 0 {
                        field.alignment
                    } else {
                        field.natural_align
                    },
                });
            }

            StructType {
                layout: Layout::Extern,
                fields,
                is_tuple: info.is_tuple,
            }
        }
    }
}

/// Returns true if a struct of type `T` with size `size` can set
/// field `field` (if it fits within the size). This is used for ABI
/// compatibility for structs that have an explicit size field.
pub fn sized_field_fits(t: &StructType, size: usize, field: &str) -> bool {
    let offset = t.offset_of(field);
    let field_size = t.field_size(field);
    size >= offset + field_size
}

#[cfg(test)]
mod tests {
    #![allow(non_snake_case)]
    use super::*;

    /// PORT NOTE: The reference uses real `extern struct { size: usize, a: u8,
    /// b: u32 }` and reflects with `@offsetOf`/`@sizeOf`. We build the same
    /// descriptor: usize=8/align8, u8=1/align1, u32=4/align4, giving offsets
    /// size@0, a@8, b@12 and size 16 — identical to the reference's C layout.
    fn sized() -> StructType {
        StructType {
            layout: Layout::Extern,
            fields: vec![
                FieldInfo {
                    name: "size",
                    size: 8,
                    natural_align: 8,
                    alignment: 8,
                },
                FieldInfo {
                    name: "a",
                    size: 1,
                    natural_align: 1,
                    alignment: 1,
                },
                FieldInfo {
                    name: "b",
                    size: 4,
                    natural_align: 4,
                    alignment: 4,
                },
            ],
            is_tuple: false,
        }
    }

    #[test]
    fn sizedFieldFits_boundary_checks() {
        let sized = sized();

        let size_required = sized.offset_of("size") + sized.field_size("size");
        let a_required = sized.offset_of("a") + sized.field_size("a");
        let b_required = sized.offset_of("b") + sized.field_size("b");

        assert!(sized_field_fits(&sized, size_required, "size"));
        assert!(!sized_field_fits(&sized, size_required - 1, "size"));

        assert!(sized_field_fits(&sized, a_required, "a"));
        assert!(!sized_field_fits(&sized, a_required - 1, "a"));

        assert!(sized_field_fits(&sized, b_required, "b"));
        assert!(!sized_field_fits(&sized, b_required - 1, "b"));
    }

    #[test]
    fn sizedFieldFits_respects_alignment_padding() {
        let sized = sized();

        let up_to_padding = sized.offset_of("b");
        assert!(sized_field_fits(&sized, up_to_padding, "a"));
        assert!(!sized_field_fits(&sized, up_to_padding, "b"));
    }

    #[test]
    fn packed_struct_converts_to_extern_with_full_size_bools() {
        // PORT NOTE: The reference packs `flag1: bool, flag2: bool, value: u8`.
        // A Zig packed bool reports `@sizeOf == 1` and natural alignment 1, but
        // in a packed struct the field alignment is 0 (resolved to natural by
        // `Struct`). We model that exactly: each field size 1, natural_align 1,
        // declared alignment 0.
        let packed = StructType {
            layout: Layout::Packed,
            fields: vec![
                FieldInfo {
                    name: "flag1",
                    size: 1,
                    natural_align: 1,
                    alignment: 0,
                },
                FieldInfo {
                    name: "flag2",
                    size: 1,
                    natural_align: 1,
                    alignment: 0,
                },
                FieldInfo {
                    name: "value",
                    size: 1,
                    natural_align: 1,
                    alignment: 0,
                },
            ],
            is_tuple: false,
        };

        let c = struct_type(Target::C, &packed);
        let info = &c;

        assert_eq!(Layout::Extern, info.layout);
        assert_eq!(1usize, c.field_size("flag1"));
        assert_eq!(1usize, c.field_size("flag2"));
        assert_eq!(1usize, c.field_size("value"));
        assert_eq!(3usize, c.size_of());
    }
}
