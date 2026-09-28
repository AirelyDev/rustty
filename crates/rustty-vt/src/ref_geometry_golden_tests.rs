//! Rustty-only (task R13-L2, NEW-FINDINGS N88 / N91): the port's reference
//! item geometry against the golden the reference is measured into.
//!
//! `.port/tools/gates/fixtures/ref_item_geometry_golden.txt` holds the
//! reference's `@sizeOf` / `@alignOf` of the set items the page layout is tied
//! to, and of the declarations they are built from, at the pin, for
//! aarch64-macos, aarch64-linux and wasm32-wasi. The gate
//! `check_ref_item_geometry.py` re-measures the reference and compares it with
//! that file; this module compares the PORT with the same file, so both sides
//! are pinned to one set of numbers:
//!
//! - the `#[repr(C)]` shadows the port derives the reference geometry from
//!   (`style.rs`, `hyperlink.rs`) and the set strides built on them
//!   (`ItemStride`);
//! - the port's own types where they must equal the reference's
//!   (`OffsetSlice`, `PageEntryId`, `PageEntry`, the hyperlink item, the item
//!   metadata), and the style item where it must fit the reference's slot.
//!
//! The 64-bit lines are checked against the host's real `size_of` /
//! `align_of`. The wasm32 lines cannot be: wasm32 tests do not run here, and a
//! host build lays types out for the host. They are checked against a MODEL of
//! the same types on a 4-byte `usize`, computed from constants: each type's
//! fields as `(size, align)` pairs, combined by the `repr(C)` rule (in order,
//! each field aligned, the total rounded to the largest alignment) or, for the
//! port's default-repr types, by the same rule over the fields sorted by
//! descending alignment. The model is itself checked against the host at
//! `usize` = 8, field list by field list, so a field it misses shows up as a
//! host mismatch. The wasm32 build's own `const` asserts (`ItemStride`, the
//! ones beside both sets and beside `PageEntryId`) are the direct check; G-XTARGET
//! (`check_cross_targets.sh`) builds that target.

use crate::hyperlink::{self, PageEntry, PageEntryId};
use crate::ref_counted_set::{ItemStride, Metadata, RefItemGeometry};
use crate::style::{self, Style};
use rustty_render_types::term_size as size;
use rustty_render_types::term_size::OffsetSlice;

use core::mem::{align_of, size_of};

const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../.port/tools/gates/fixtures/ref_item_geometry_golden.txt"
);

/// `(target, name) -> (size, align)`, in file order.
fn golden() -> Vec<(String, String, usize, usize)> {
    let text = std::fs::read_to_string(GOLDEN_PATH)
        .unwrap_or_else(|e| panic!("reading the golden {GOLDEN_PATH}: {e}"));
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(4, f.len(), "malformed golden line {line:?}");
        out.push((
            f[0].to_string(),
            f[1].to_string(),
            f[2].parse().unwrap(),
            f[3].parse().unwrap(),
        ));
    }
    out
}

/// A `(size, align)` pair.
type L = (usize, usize);

fn align_up(n: usize, a: usize) -> usize {
    n.div_ceil(a) * a
}

/// `#[repr(C)]`: the fields in order.
fn repr_c(fields: &[L]) -> L {
    let (mut off, mut al) = (0, 1);
    for &(s, a) in fields {
        off = align_up(off, a) + s;
        al = al.max(a);
    }
    (align_up(off, al), al)
}

/// Default repr, modelled as `repr(C)` over the fields by descending
/// alignment (checked against the host at `usize` = 8).
fn repr_rust(fields: &[L]) -> L {
    let mut f = fields.to_vec();
    f.sort_by(|a, b| b.1.cmp(&a.1));
    repr_c(&f)
}

/// The model of every type the golden names, on a `ptr`-byte `usize`: the
/// reference's (through the port's shadows' field lists) and the port's own.
struct Model {
    ref_slice: L,
    ref_page_entry_id: L,
    ref_page_entry: L,
    ref_hl_item: L,
    ref_style_item: L,
    ref_style: L,
    ref_color: L,
    port_offset_slice: L,
    port_page_entry_id: L,
    port_page_entry: L,
    port_metadata: L,
    port_hl_item: L,
    port_style_item: L,
}

fn model(ptr: usize) -> Model {
    let u8_: L = (1, 1);
    let u16_: L = (2, 2);
    let u32_: L = (4, 4);
    let usize_: L = (ptr, ptr);

    // Reference, via the shadows' field lists (`hyperlink.rs`, `style.rs`).
    let ref_slice = repr_c(&[u32_, usize_]);
    let ref_page_entry_id = repr_c(&[ref_slice, u8_]);
    let ref_page_entry = repr_c(&[ref_page_entry_id, ref_slice]);
    let ref_hl_item = repr_c(&[ref_page_entry_id, ref_slice, u16_, u16_, u16_]);
    let ref_color = repr_c(&[u32_, u8_]);
    let ref_style = repr_c(&[ref_color, ref_color, ref_color, u16_]);
    let ref_style_item = repr_c(&[ref_style, u16_, u16_, u16_]);

    // The port's own types.
    // `OffsetSlice`: `offset: u32`, a 64-bit-only `_padding: u32`, `len`.
    let port_offset_slice = if ptr == 8 {
        repr_c(&[u32_, u32_, usize_])
    } else {
        repr_c(&[u32_, usize_])
    };
    // `PageEntryId`: `payload: OffsetSlice`, `tag: u8`,
    // `_padding: [u8; size_of::<usize>() - 1]`.
    let port_page_entry_id = repr_c(&[port_offset_slice, u8_, (ptr - 1, 1)]);
    let port_page_entry = repr_rust(&[port_page_entry_id, port_offset_slice]);
    let port_metadata = repr_rust(&[u16_, u16_, u16_]);
    let port_hl_item = repr_rust(&[port_page_entry, port_metadata]);
    // The port's `Style` has no pointer-sized field: its layout is the
    // host's on every target.
    let port_style: L = (size_of::<Style>(), align_of::<Style>());
    let port_style_item = repr_rust(&[port_style, port_metadata]);

    Model {
        ref_slice,
        ref_page_entry_id,
        ref_page_entry,
        ref_hl_item,
        ref_style_item,
        ref_style,
        ref_color,
        port_offset_slice,
        port_page_entry_id,
        port_page_entry,
        port_metadata,
        port_hl_item,
        port_style_item,
    }
}

fn lay<T>() -> L {
    (size_of::<T>(), align_of::<T>())
}

type StyleItem = crate::ref_counted_set::Item<Style, style::Id, size::CellCountInt>;
type HlItem = crate::ref_counted_set::Item<PageEntry, hyperlink::Id, size::CellCountInt>;
type StyleStride = ItemStride<Style, style::Id, size::CellCountInt>;
type HlStride = ItemStride<PageEntry, hyperlink::Id, size::CellCountInt>;

/// The model at `usize` = 8 is the host's real layout, type by type. This is
/// what makes the wasm32 model trustworthy: the same field lists, one
/// pointer width down.
#[test]
#[cfg(target_pointer_width = "64")]
fn the_layout_model_reproduces_the_host() {
    let m = model(8);
    assert_eq!(
        lay::<hyperlink::RefShadowSlice>(),
        m.ref_slice,
        "RefShadowSlice"
    );
    assert_eq!(
        lay::<hyperlink::RefShadowPageEntryId>(),
        m.ref_page_entry_id,
        "RefShadowPageEntryId"
    );
    assert_eq!(
        lay::<hyperlink::RefShadowItem>(),
        m.ref_hl_item,
        "hyperlink RefShadowItem"
    );
    assert_eq!(
        lay::<style::RefShadowColor>(),
        m.ref_color,
        "RefShadowColor"
    );
    assert_eq!(
        lay::<style::RefShadowStyle>(),
        m.ref_style,
        "RefShadowStyle"
    );
    assert_eq!(
        lay::<style::RefShadowItem>(),
        m.ref_style_item,
        "style RefShadowItem"
    );
    assert_eq!(lay::<OffsetSlice<u8>>(), m.port_offset_slice, "OffsetSlice");
    assert_eq!(lay::<PageEntryId>(), m.port_page_entry_id, "PageEntryId");
    assert_eq!(lay::<PageEntry>(), m.port_page_entry, "PageEntry");
    assert_eq!(
        lay::<Metadata<style::Id, size::CellCountInt>>(),
        m.port_metadata,
        "Metadata"
    );
    assert_eq!(lay::<HlItem>(), m.port_hl_item, "hyperlink Item");
    assert_eq!(lay::<StyleItem>(), m.port_style_item, "style Item");
    assert_eq!(m.ref_page_entry, m.port_page_entry, "the model's PageEntry");
}

/// Every golden line, for every target: the reference's value (the port's
/// shadow, on the host for the 64-bit targets and through the model for
/// wasm32) must equal it, and so must the port's own type where the layout
/// needs equality; the style item must fit the reference's slot, and each
/// set's stride is the reference item.
#[test]
#[cfg_attr(
    miri,
    ignore = "MIRI EXCEPTION (rustty-vt, parity tail C-1): `open` is forbidden under Miri isolation (reads the checked-in geometry golden); runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
)]
fn the_port_matches_the_reference_geometry_golden() {
    let golden = golden();
    let targets: Vec<&str> = {
        let mut t: Vec<&str> = golden.iter().map(|(t, ..)| t.as_str()).collect();
        t.dedup();
        t
    };
    assert_eq!(
        vec!["aarch64-macos", "aarch64-linux", "wasm32-wasi"],
        targets,
        "the golden's targets"
    );

    let mut checked = 0;
    for (target, name, gsize, galign) in &golden {
        let g: L = (*gsize, *galign);
        let ptr = if target.starts_with("wasm32") { 4 } else { 8 };
        let host = ptr == size_of::<usize>();
        let m = model(ptr);
        let at = format!("{target} {name}");

        // (the reference, as the port derives it; the port's own type, when it
        // must be equal)
        let (reference, port): (L, Option<L>) = match name.as_str() {
            "usize" => ((ptr, ptr), None),
            // The shadow's `RGB` payload is a `u32`.
            "color.RGB" => (lay::<u32>(), None),
            "style.Style.Color" => (m.ref_color, None),
            // The shadow's `flags` is a `u16`.
            "style.Style.Flags" => (lay::<u16>(), None),
            "style.Style" => (m.ref_style, None),
            "style.Set.Item.Metadata" | "hyperlink.Set.Item.Metadata" => {
                (repr_c(&[(2, 2), (2, 2), (2, 2)]), Some(m.port_metadata))
            }
            "style.Set.Item" => (m.ref_style_item, None),
            "size.Offset(u8).Slice" => (m.ref_slice, Some(m.port_offset_slice)),
            "hyperlink.PageEntry.Id" => (m.ref_page_entry_id, Some(m.port_page_entry_id)),
            "hyperlink.PageEntry" => (m.ref_page_entry, Some(m.port_page_entry)),
            "hyperlink.Set.Item" => (m.ref_hl_item, Some(m.port_hl_item)),
            other => panic!("{target}: the golden names {other}, which this test does not check"),
        };
        assert_eq!(g, reference, "{at}: the port's reference shadow");
        if let Some(port) = port {
            assert_eq!(g, port, "{at}: the port's own type");
        }

        match name.as_str() {
            "style.Set.Item" => {
                // The port's item fits the reference's slot and stays aligned.
                assert!(m.port_style_item.0 <= g.0, "{at}: the port's item fits");
                assert_eq!(
                    0,
                    g.0 % m.port_style_item.1,
                    "{at}: slot starts stay aligned"
                );
                assert!(m.port_style_item.1 <= g.1, "{at}: the array alignment");
                if host {
                    assert_eq!(g, (StyleStride::SIZE, StyleStride::ALIGN), "{at}: stride");
                    assert_eq!(
                        g,
                        (
                            <Style as RefItemGeometry>::REF_ITEM_SIZE,
                            <Style as RefItemGeometry>::REF_ITEM_ALIGN
                        ),
                        "{at}: RefItemGeometry"
                    );
                    assert_eq!(g, lay::<style::RefShadowItem>(), "{at}: shadow on the host");
                }
            }
            "hyperlink.Set.Item" => {
                if host {
                    assert_eq!(g, (HlStride::SIZE, HlStride::ALIGN), "{at}: stride");
                    assert_eq!(
                        g,
                        (
                            <PageEntry as RefItemGeometry>::REF_ITEM_SIZE,
                            <PageEntry as RefItemGeometry>::REF_ITEM_ALIGN
                        ),
                        "{at}: RefItemGeometry"
                    );
                    assert_eq!(
                        g,
                        lay::<hyperlink::RefShadowItem>(),
                        "{at}: shadow on the host"
                    );
                    assert_eq!(g, lay::<HlItem>(), "{at}: the port's item on the host");
                    assert_eq!(g.0, crate::page::hyperlink_set_item_size(), "{at}");
                }
            }
            "style.Style.Color" if host => {
                assert_eq!(
                    g,
                    lay::<style::RefShadowColor>(),
                    "{at}: shadow on the host"
                )
            }
            "style.Style" if host => {
                assert_eq!(
                    g,
                    lay::<style::RefShadowStyle>(),
                    "{at}: shadow on the host"
                )
            }
            "size.Offset(u8).Slice" if host => {
                assert_eq!(
                    g,
                    lay::<hyperlink::RefShadowSlice>(),
                    "{at}: shadow on the host"
                );
                assert_eq!(
                    g,
                    lay::<OffsetSlice<u8>>(),
                    "{at}: the port's type on the host"
                );
            }
            "hyperlink.PageEntry.Id" if host => {
                assert_eq!(g, lay::<hyperlink::RefShadowPageEntryId>(), "{at}: shadow");
                assert_eq!(g, lay::<PageEntryId>(), "{at}: the port's type on the host");
            }
            "hyperlink.PageEntry" if host => {
                assert_eq!(g, lay::<PageEntry>(), "{at}: the port's type on the host")
            }
            "usize" if host => assert_eq!(g, lay::<usize>(), "{at}"),
            _ => {}
        }
        checked += 1;
    }
    assert_eq!(36, checked, "every golden line was checked");
}

/// The wasm32 geometry, spelled out: the reference's hyperlink item is 28/4
/// there (measured by the probe), and so is the port's since `PageEntryId`
/// shares its payload bytes as the union does (task R13-L2; the side-by-side
/// payloads made it 32, which does not fit the 28-byte slot).
#[test]
fn the_wasm32_hyperlink_item_is_the_references() {
    let m = model(4);
    assert_eq!((12, 4), m.port_page_entry_id);
    assert_eq!((20, 4), m.port_page_entry);
    assert_eq!((28, 4), m.port_hl_item);
    assert_eq!((28, 4), m.ref_hl_item);
    assert_eq!((36, 4), m.ref_style_item);
    assert_eq!((28, 2), m.port_style_item);
}
