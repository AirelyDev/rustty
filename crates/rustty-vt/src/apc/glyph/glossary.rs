//! rustty-vt apc/glyph/glossary
//!
//! PORTS: ghostty src/terminal/apc/glyph/Glossary.zig
//!
//! PORT STATUS: done

//! Glossary is the per-terminal storage for Glyph Protocol
//! codepoints. We use the word Glossary to match up with the spec which
//! also uses this word.
// PORT NOTE: reference file header `const Glossary = @This();`. The file-level
// `Glossary` struct is `pub struct Glossary` here.

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const CircBuf = @import("../../../datastruct/circ_buf.zig").CircBuf;
// const FontGlyph = @import("../../../font/Glyph.zig");
// const Glyf = @import("../../../font/opentype/glyf.zig").Glyf;
// PORT NOTE: `Allocator` is not threaded through (entries own their decoded
// outlines as `Vec`s, freed on drop). `CircBuf` is imported by the reference but
// unused, so it has no port. `FontGlyph`/`Glyf` are `rustty_font` types. The
// `Glyf.Outline` (nested in Zig) is the sibling `Outline` here.
use rustty_font::glyph::{Align as ConstraintAlign, Constraint, ConstraintSize, DesignMetrics};
use rustty_font::opentype::glyf::Outline;

// const request = @import("request.zig");
// const RegisterReq = request.Request.Register;
use crate::apc::glyph::request::{self, Register as RegisterReq};

// const DesignMetrics = FontGlyph.DesignMetrics;
// const Constraint = FontGlyph.RenderOptions.Constraint;
// PORT NOTE: imported above directly under their `rustty_font::glyph` names.

// PORT NOTE: `AutoArrayHashMapUnmanaged(u21, Entry)` -> the faithful zigstd port,
// keyed on `u32` (no native `u21`).
use rustty_zigstd::array_hash_map::AutoArrayHashMapUnmanaged;

/// Maximum entries allowed in the glossary before eviction.
/// Defined by the specification.
pub const MAX_ENTRIES: usize = 1024;

/// Maximum size of any allocation made while decoding a glyf registration.
/// This is limited by the dynamic allocations not the struct size so the
/// actual size allowed is slightly larger, but worst case only by 10KB.
// pub const max_glyf_allocation = 64 * 1024; // 64 KiB
pub const MAX_GLYF_ALLOCATION: usize = 64 * 1024; // 64 KiB

/// Errors that can occur while registering a glossary entry.
// PORT NOTE: reference `RegisterError = Allocator.Error || error{OutOfNamespace}`.
// `OutOfMemory` is preserved for fidelity even though the port's map insertion
// does not surface a typed allocation failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterError {
    OutOfMemory,
    OutOfNamespace,
}

/// Errors that can occur while clearing glossary entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClearError {
    OutOfNamespace,
}

/// Glossary is the per-terminal storage for Glyph Protocol codepoints.
pub struct Glossary {
    /// The set of entries in the glossary keyed by the codepoint.
    ///
    /// The array hash map preserves insertion order and has O(N)
    /// orderedRemove, so we use it as a FIFO too for eviction when
    /// the glossary is full. Since the specification limits the protocol
    /// to 1024 maximum entries, ordered removal should never be that
    /// expensive.
    ///
    /// I'm also operating under the assumption that full glossaries
    /// for a session will be rare, so the eviction cost shouldn't
    /// happen regularly.
    //
    // PORT NOTE (parity tail S7-3): the map grows and frees through the
    // terminal's allocator handle, bound at construction (`empty_in`). The
    // reference passes that allocator per call (`self.gpa()`); the two are the
    // same allocator unless a caller swaps the screen's allocator mid-life,
    // which no API path does. The entries' decoded `Outline`s are allocated
    // through the registering allocator too (`Entry::init(alloc, ..)`, parity
    // tail S7-4).
    entries: AutoArrayHashMapUnmanaged<u32, Entry, crate::alloc::Allocator>,
}

impl Glossary {
    /// An empty glossary with no registered glyphs.
    // PORT NOTE: reference `pub const empty: Glossary = .{ .entries = .empty };`.
    // Exposed as a constructor (`AutoArrayHashMapUnmanaged::new` is not `const`).
    #[allow(clippy::should_implement_trait)]
    pub fn empty() -> Glossary {
        Glossary::empty_in(crate::alloc::Global)
    }

    /// The allocator the glossary's storage grows and frees through.
    pub fn allocator(&self) -> crate::alloc::Allocator {
        *self.entries.allocator()
    }

    /// [`Glossary::empty`] whose storage grows through `alloc`.
    pub fn empty_in(alloc: crate::alloc::Allocator) -> Glossary {
        Glossary {
            entries: AutoArrayHashMapUnmanaged::new_in(alloc),
        }
    }

    /// Release all glyph entries and hash map storage owned by the glossary.
    // PORT NOTE: reference `deinit` deinits every entry then the map and sets
    // `self.* = undefined`. Here the map and its `Entry` values (each owning a
    // decoded `Outline`) are freed on drop, so this is a no-op retained for
    // call-site fidelity.
    pub fn deinit(&mut self) {
        // for (self.entries.values()) |*entry| entry.deinit(alloc);
        // self.entries.deinit(alloc);
        // self.* = undefined;
    }

    /// Register the given glyph entry.
    ///
    /// This will act according to the glyph specification
    pub fn register(&mut self, cp: u32, entry: Entry) -> Result<(), RegisterError> {
        // Validate codepoint according to spec.
        // if (!isPrivateUse(cp)) return error.OutOfNamespace;
        if !is_private_use(cp) {
            return Err(RegisterError::OutOfNamespace);
        }

        // const gop = try self.entries.getOrPut(alloc, cp);
        // if (gop.found_existing) {
        if self.entries.contains(&cp) {
            // Found an existing entry, we need to shift the FIFO so
            // that this is now the most recent (at the end). This is
            // O(N) but N is usually small and max N is bounded by the spec.
            // gop.value_ptr.*.deinit(alloc);
            // assert(self.entries.orderedRemove(cp));
            // PORT NOTE: removing the existing entry drops its old `Entry`
            // (freeing the prior `Outline`), reproducing the reference's
            // explicit `deinit`. `orderedRemove` returning `true` is the
            // reference's `assert`; the key is known present here.
            assert!(self.entries.ordered_remove(&cp));

            // We already had enough capacity for this key before removing it, so
            // reinserting the replacement cannot require another allocation.
            // self.entries.putAssumeCapacity(cp, entry);
            // PORT NOTE: `put` re-appends at the end since `cp` is now absent,
            // matching `putAssumeCapacity` after the `orderedRemove` (newest
            // position); the removal left the capacity, so it cannot allocate.
            self.entries.put(cp, entry);
            // return;
            return Ok(());
        }

        // Array hash maps preserve insertion order so always immediately insert.
        // gop.value_ptr.* = entry;
        //
        // PORT NOTE: the reference's `try self.entries.getOrPut(alloc, cp)` is
        // the fallible step; `try_put` reserves first, so a failure leaves the
        // map unchanged and drops `entry`.
        self.entries
            .try_put(cp, entry)
            .map_err(|_| RegisterError::OutOfMemory)?;

        // Fast, typical path: we fit within the glossary, just return.
        // if (self.entries.count() <= max_entries) return;
        if self.entries.count() <= MAX_ENTRIES {
            return Ok(());
        }

        // Slow path: we need to evict.
        // self.entries.values()[0].deinit(alloc);
        // self.entries.orderedRemoveAt(0);
        // PORT NOTE: `ordered_remove_index(0)` removes the oldest entry and drops
        // its `Entry` (freeing the `Outline`), reproducing the reference's
        // `values()[0].deinit` + `orderedRemoveAt(0)`.
        self.entries.ordered_remove_index(0);
        Ok(())
    }

    /// Delete a single entry from the glossary. If the entry doesn't exist,
    /// then this does nothing and is safe.
    pub fn delete(&mut self, cp: u32) -> Result<(), ClearError> {
        // if (!isPrivateUse(cp)) return error.OutOfNamespace;
        if !is_private_use(cp) {
            return Err(ClearError::OutOfNamespace);
        }
        // const kv = self.entries.fetchOrderedRemove(cp) orelse return;
        // var entry = kv.value;
        // entry.deinit(alloc);
        // PORT NOTE: `fetch_ordered_remove` returns the removed `KV`; dropping it
        // frees the `Entry`'s `Outline`, reproducing the reference `entry.deinit`.
        // Absent key -> `None` -> early return.
        let _ = self.entries.fetch_ordered_remove(&cp);
        Ok(())
    }

    /// Clear all entries from the glossary and free up any underlying
    /// storage.
    pub fn clear_and_free(&mut self) {
        // for (self.entries.values()) |*entry| entry.deinit(alloc);
        // self.entries.deinit(alloc);
        // self.entries = .empty;
        // PORT NOTE: `clear_and_free` drops every `Entry` (freeing each
        // `Outline`) and frees the map's storage through its allocator,
        // reproducing the per-entry `deinit` + map `deinit` + reset.
        self.entries.clear_and_free();
    }

    /// Contains returns true if the codepoint is covered by the glossary.
    pub fn contains(&self, cp: u32) -> bool {
        // return self.entries.contains(cp);
        self.entries.contains(&cp)
    }
}

/// A single glyph registration entry.
pub struct Entry {
    /// The glyph itself. The tagged union only has glyf right now but
    /// will eventually expand to support COLR and maybe other formats.
    /// These are stored as raw outlines; rasterization is delayed to
    /// renderers. The outlines have been validated.
    pub glyph: Glyph,

    /// Authored metrics for the glyph's design coordinate space.
    pub design: DesignMetrics,

    /// Unicode cell width requested by the registration.
    pub width: request::Width,

    /// Normalized scale, alignment, and padding behavior for rasterization.
    pub constraint: Constraint,
}

/// Stored glyph payload variants.
// PORT NOTE: reference nested `Entry.Glyph = union(enum) { glyf: Glyf.Outline }`.
// Hoisted to a sibling enum; the single `glyf` variant carries an owned
// `Outline` whose vectors were allocated through the registering allocator
// (parity tail S7-4).
pub enum Glyph {
    Glyf(Outline<crate::alloc::Allocator>),
}

/// Errors that can occur while constructing a glossary entry from a
/// register request.
// PORT NOTE: reference `InitError = RegisterReq.DecodeError || error{
// InvalidOptions, UnsupportedFormat }`. The `DecodeError` members are folded in
// via `From<request::DecodeError>` below so a decode error flows transparently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryInitError {
    OutOfMemory,
    PayloadTooLarge,
    MalformedPayload,
    CompositeUnsupported,
    HintingUnsupported,

    /// The register request is missing a required option or has an invalid
    /// explicitly-provided option value.
    InvalidOptions,

    /// The requested payload format is not supported by this glossary.
    UnsupportedFormat,
}

// PORT NOTE: reproduces the `||` error-set union: a `request::DecodeError` flows
// transparently into `EntryInitError` (the reference `try req.decodeGlyfPayload`).
impl From<request::DecodeError> for EntryInitError {
    fn from(err: request::DecodeError) -> EntryInitError {
        match err {
            request::DecodeError::OutOfMemory => EntryInitError::OutOfMemory,
            request::DecodeError::PayloadTooLarge => EntryInitError::PayloadTooLarge,
            request::DecodeError::MalformedPayload => EntryInitError::MalformedPayload,
            request::DecodeError::CompositeUnsupported => EntryInitError::CompositeUnsupported,
            request::DecodeError::HintingUnsupported => EntryInitError::HintingUnsupported,
        }
    }
}

impl Entry {
    /// Initialize a glossary entry from a register request.
    ///
    /// This validates the request fields needed to construct the entry,
    /// decodes the base64 glyph payload, and stores the decoded outline. The
    /// returned entry owns decoded glyph memory and must be released with
    /// `deinit`.
    pub fn init(
        alloc: crate::alloc::Allocator,
        req: &RegisterReq,
    ) -> Result<Entry, EntryInitError> {
        // Validate format
        // const fmt = req.get(.fmt) orelse return error.InvalidOptions;
        let fmt = req.get_fmt().ok_or(EntryInitError::InvalidOptions)?;
        // const design: DesignMetrics = .{ ... };
        let design = DesignMetrics {
            // .units_per_em = req.get(.upm) orelse return error.InvalidOptions,
            units_per_em: req.get_upm().ok_or(EntryInitError::InvalidOptions)?,
            // .advance_width = req.get(.aw) orelse return error.InvalidOptions,
            advance_width: req.get_aw().ok_or(EntryInitError::InvalidOptions)?,
            // .line_height = req.get(.lh) orelse return error.InvalidOptions,
            line_height: req.get_lh().ok_or(EntryInitError::InvalidOptions)?,
        };
        // if (design.units_per_em == 0 or design.advance_width == 0 or
        //     design.line_height == 0) return error.InvalidOptions;
        if design.units_per_em == 0 || design.advance_width == 0 || design.line_height == 0 {
            return Err(EntryInitError::InvalidOptions);
        }
        // const width = req.get(.width) orelse return error.InvalidOptions;
        let width = req.get_width().ok_or(EntryInitError::InvalidOptions)?;

        // Get our constraints
        // const constraint = try constraintFromRegister(req);
        let constraint = constraint_from_register(req)?;

        // Decode the payload into some usable glyph format for
        // future rasterization.
        // const glyph: Glyph = switch (fmt) {
        //     .glyf => .{ .glyf = try req.decodeGlyfPayload(alloc, max_glyf_allocation) },
        //     .colrv0, .colrv1 => return error.UnsupportedFormat,
        // };
        let glyph: Glyph = match fmt {
            request::Format::Glyf => {
                Glyph::Glyf(req.decode_glyf_payload(alloc, MAX_GLYF_ALLOCATION)?)
            }
            request::Format::Colrv0 | request::Format::Colrv1 => {
                return Err(EntryInitError::UnsupportedFormat);
            }
        };

        // No more errors, since we never do glyph cleanup above.
        // errdefer comptime unreachable;

        Ok(Entry {
            glyph,
            design,
            width,
            constraint,
        })
    }

    /// Release memory owned by this entry.
    // PORT NOTE: reference `deinit` deinits the outline and sets `self.* =
    // undefined`. The `Outline`'s `Vec`s are freed on drop, so this is a no-op
    // retained for call-site fidelity.
    pub fn deinit(&mut self) {
        // switch (self.glyph) { .glyf => |*outline| outline.deinit(alloc) }
        // self.* = undefined;
    }
}

/// Return the renderer constraint for a register request.
///
/// Glyph Protocol §8.5 defines sizing, alignment, and padding in terms of
/// the authored extent and render span. This function is the single
/// normalization point for how protocol sizing choices map to the
/// renderer-neutral constraint stored here.
// PORT NOTE: reference nested `Entry.constraintFromRegister`; hoisted to a module
// fn. Returns `error{InvalidOptions}!Constraint`.
fn constraint_from_register(req: &RegisterReq) -> Result<Constraint, EntryInitError> {
    // Register.get applies the Glyph Protocol §6.1 defaults when options
    // are omitted: size=height, align=center,center, and pad=0,0,0,0.
    // const size = req.get(.size) orelse return error.InvalidOptions;
    let size = req.get_size().ok_or(EntryInitError::InvalidOptions)?;
    // const alignment = req.get(.@"align") orelse return error.InvalidOptions;
    let alignment = req.get_align().ok_or(EntryInitError::InvalidOptions)?;
    // const pad = req.get(.pad) orelse return error.InvalidOptions;
    let pad = req.get_pad().ok_or(EntryInitError::InvalidOptions)?;

    Ok(Constraint {
        // .size = switch (size) { ... },
        size: match size {
            // The rasterizer's base transform already maps the design em
            // to the cell height. That is the closest existing behavior to
            // the protocol's default height-driven mode.
            // .height => .none,
            request::Size::Height => ConstraintSize::None,
            // There is no width-driven, aspect-preserving constraint mode
            // today. Leave the base transform intact rather than forcing a
            // fit/contain policy that would unexpectedly prevent overflow.
            // .advance => .none,
            request::Size::Advance => ConstraintSize::None,
            // Constraint.cover currently scales preserving aspect ratio to
            // the available bounds, which is the best existing match for
            // the protocol's contain mode.
            // .contain => .cover,
            request::Size::Contain => ConstraintSize::Cover,
            // There is no true protocol-cover equivalent that chooses the
            // larger axis scale, so use the nearest named renderer policy.
            // .cover => .cover,
            request::Size::Cover => ConstraintSize::Cover,
            // .stretch => .stretch,
            request::Size::Stretch => ConstraintSize::Stretch,
        },
        // .align_horizontal = switch (alignment.horizontal) { ... },
        align_horizontal: match alignment.horizontal {
            // .start => .start,
            request::AlignHorizontal::Start => ConstraintAlign::Start,
            // .center => .center,
            request::AlignHorizontal::Center => ConstraintAlign::Center,
            // .end => .end,
            request::AlignHorizontal::End => ConstraintAlign::End,
        },
        // .align_vertical = switch (alignment.vertical) { ... },
        align_vertical: match alignment.vertical {
            // .start => .start,
            request::AlignVertical::Start => ConstraintAlign::Start,
            // .center => .center,
            request::AlignVertical::Center => ConstraintAlign::Center,
            // .end => .end,
            request::AlignVertical::End => ConstraintAlign::End,
            // The current constraint API has no baseline alignment mode.
            // Start is the closest stable default because the glyf
            // rasterizer's coordinate model already treats y=0 as the
            // baseline/bottom before constraints are applied.
            // .baseline => .start,
            request::AlignVertical::Baseline => ConstraintAlign::Start,
        },
        // .pad_top = pad.top,
        pad_top: pad.top,
        // .pad_right = pad.right,
        pad_right: pad.right,
        // .pad_bottom = pad.bottom,
        pad_bottom: pad.bottom,
        // .pad_left = pad.left,
        pad_left: pad.left,
        // PORT NOTE: the reference relies on `Constraint`'s remaining field
        // defaults (relative_*, max_xy_ratio, max_constraint_width, height) by
        // not setting them in the struct literal; `Constraint::none` carries the
        // same defaults, so they are spread in here.
        ..Constraint::none
    })
}

/// Return true if `cp` is in one of the Unicode Private Use Areas.
fn is_private_use(cp: u32) -> bool {
    // return (cp >= 0xE000 and cp <= 0xF8FF) or
    //     (cp >= 0xF0000 and cp <= 0xFFFFD) or
    //     (cp >= 0x100000 and cp <= 0x10FFFD);
    (0xE000..=0xF8FF).contains(&cp)
        || (0xF0000..=0xFFFFD).contains(&cp)
        || (0x100000..=0x10FFFD).contains(&cp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apc::glyph::request::Request;

    // PORT NOTE: `testParseRegister(alloc, data)` dupes `data`, parses it, and
    // returns the `.register` variant (unreachable otherwise). The `alloc` dup is
    // an owned `Vec<u8>` here; parsing consumes it.
    fn test_parse_register(data: &[u8]) -> RegisterReq {
        // const raw = try alloc.dupe(u8, data);
        let raw = crate::alloc::heap_bytes(data);
        // const req = try request.Request.parse(alloc, raw);
        let req = Request::parse(raw).unwrap();
        // switch (req) { .register => |reg| return reg, else => unreachable }
        match req {
            Request::Register(reg) => reg,
            _ => unreachable!(),
        }
    }

    // Base64-encoded glyf payload from the "glyf: decode triangle" test in
    // font/opentype/glyf.zig. This is a real simple-glyph record with one contour
    // and three on-curve points.
    const TEST_TRIANGLE_GLYF_PAYLOAD: &str = "AAEAZABkA4QDhAACAAABAQEB9P5wAyADhPzgAAA=";

    fn test_register_req(cp: u32) -> RegisterReq {
        // const data = try std.fmt.allocPrint(alloc,
        //   "r;cp={x};upm=2048;aw=1024;lh=1536;width=2;size=stretch;align=end,start;pad=0.1,0.2,0.3,0.4;{s}",
        //   .{ cp, test_triangle_glyf_payload });
        let data = format!(
            "r;cp={:x};upm=2048;aw=1024;lh=1536;width=2;size=stretch;align=end,start;pad=0.1,0.2,0.3,0.4;{}",
            cp, TEST_TRIANGLE_GLYF_PAYLOAD
        )
        .into_bytes();

        // const req = try request.Request.parse(alloc, data);
        let req = Request::parse(crate::alloc::heap_bytes(&data)).unwrap();
        match req {
            Request::Register(reg) => reg,
            _ => unreachable!(),
        }
    }

    fn test_register_entry(cp: u32) -> Entry {
        // const req = try testRegisterReq(alloc, cp);
        let req = test_register_req(cp);
        // defer alloc.free(req.raw);
        // return try Entry.init(alloc, req);
        Entry::init(crate::alloc::Global, &req).unwrap()
    }

    #[test]
    fn entry_init_decodes_glyf_payload_and_applies_register_fields() {
        // const req = try testRegisterReq(alloc, 0xE000);
        let req = test_register_req(0xE000);

        // var entry = try Entry.init(alloc, req);
        let entry = Entry::init(crate::alloc::Global, &req).unwrap();

        // try testing.expectEqual(@as(u32, 2048), entry.design.units_per_em);
        assert_eq!(2048u32, entry.design.units_per_em);
        // try testing.expectEqual(@as(u32, 1024), entry.design.advance_width);
        assert_eq!(1024u32, entry.design.advance_width);
        // try testing.expectEqual(@as(u32, 1536), entry.design.line_height);
        assert_eq!(1536u32, entry.design.line_height);
        // try testing.expectEqual(request.Width.wide, entry.width);
        assert_eq!(request::Width::Wide, entry.width);
        // try testing.expectEqual(Constraint.Size.stretch, entry.constraint.size);
        assert_eq!(ConstraintSize::Stretch, entry.constraint.size);
        // try testing.expectEqual(Constraint.Align.end, entry.constraint.align_horizontal);
        assert_eq!(ConstraintAlign::End, entry.constraint.align_horizontal);
        // try testing.expectEqual(Constraint.Align.start, entry.constraint.align_vertical);
        assert_eq!(ConstraintAlign::Start, entry.constraint.align_vertical);
        // try testing.expectEqual(@as(f64, 0.1), entry.constraint.pad_top);
        assert_eq!(0.1f64, entry.constraint.pad_top);
        // try testing.expectEqual(@as(f64, 0.2), entry.constraint.pad_right);
        assert_eq!(0.2f64, entry.constraint.pad_right);
        // try testing.expectEqual(@as(f64, 0.3), entry.constraint.pad_bottom);
        assert_eq!(0.3f64, entry.constraint.pad_bottom);
        // try testing.expectEqual(@as(f64, 0.4), entry.constraint.pad_left);
        assert_eq!(0.4f64, entry.constraint.pad_left);

        // try testing.expectEqual(@as(usize, 3), entry.glyph.glyf.points.len);
        // try testing.expectEqual(@as(usize, 1), entry.glyph.glyf.contours.len);
        let Glyph::Glyf(outline) = &entry.glyph;
        assert_eq!(3usize, outline.points.len());
        assert_eq!(1usize, outline.contours.len());
    }

    #[test]
    fn entry_init_rejects_invalid_register_payload() {
        // const req = try testParseRegister(alloc, "r;cp=e000;%%%not-base64%%%");
        let req = test_parse_register(b"r;cp=e000;%%%not-base64%%%");

        // try testing.expectError(error.MalformedPayload, Entry.init(alloc, req));
        // PORT NOTE: `Entry` (the Ok payload) has no `PartialEq`/`Debug` (it owns
        // an `Outline`), so the result is mapped to `()` before comparison.
        assert_eq!(
            Err(EntryInitError::MalformedPayload),
            Entry::init(crate::alloc::Global, &req).map(|_| ())
        );
    }

    // fn testRepeatedPointRegisterReq(
    //     alloc: Allocator,
    //     end_points: []const u16,
    // ) !RegisterReq {
    fn test_repeated_point_register_req(end_points: &[u16]) -> RegisterReq {
        // assert(end_points.len > 0);
        assert!(!end_points.is_empty());

        // var decoded: std.ArrayList(u8) = .empty;
        let mut decoded: Vec<u8> = Vec::new();

        // Simple glyph header with zero bounds.
        // const contour_count: u16 = @intCast(end_points.len);
        let contour_count: u16 = u16::try_from(end_points.len()).unwrap();
        // try decoded.appendSlice(alloc, &.{ @truncate(contour_count >> 8), @truncate(contour_count) });
        decoded.extend_from_slice(&[(contour_count >> 8) as u8, contour_count as u8]);
        // try decoded.appendNTimes(alloc, 0, 8);
        decoded.extend(core::iter::repeat_n(0u8, 8));
        // for (end_points) |end_point| try decoded.appendSlice(
        //     alloc,
        //     &.{ @truncate(end_point >> 8), @truncate(end_point) },
        // );
        for &end_point in end_points {
            decoded.extend_from_slice(&[(end_point >> 8) as u8, end_point as u8]);
        }
        // try decoded.appendSlice(alloc, &.{ 0x00, 0x00 }); // No instructions.
        decoded.extend_from_slice(&[0x00, 0x00]); // No instructions.

        // Maximum-length runs describe on-curve, zero-delta points compactly.
        // var remaining: usize = @as(usize, end_points[end_points.len - 1]) + 1;
        let mut remaining: usize = end_points[end_points.len() - 1] as usize + 1;
        // while (remaining > 0) { ... }
        while remaining > 0 {
            let run = core::cmp::min(remaining, 256);
            if run == 1 {
                decoded.push(0x31);
            } else {
                decoded.extend_from_slice(&[0x39, u8::try_from(run - 1).unwrap()]);
            }
            remaining -= run;
        }

        // const Encoder = std.base64.standard.Encoder;
        // const payload = try alloc.alloc(u8, Encoder.calcSize(decoded.items.len));
        // const encoded = Encoder.encode(payload, decoded.items);
        let encoded = test_base64_standard_encode(&decoded);

        // const command = try std.fmt.allocPrint(alloc, "r;cp=e000;{s}", .{encoded});
        let command = format!("r;cp=e000;{encoded}").into_bytes();
        // return try testParseRegister(alloc, command);
        test_parse_register(&command)
    }

    /// `std.base64.standard.Encoder` (alphabet + `'='` pad) for the test
    /// helper above; the crate only ports the decoder.
    fn test_base64_standard_encode(data: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(ALPHABET[(n >> 18) as usize & 0x3F] as char);
            out.push(ALPHABET[(n >> 12) as usize & 0x3F] as char);
            out.push(if chunk.len() > 1 {
                ALPHABET[(n >> 6) as usize & 0x3F] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHABET[n as usize & 0x3F] as char
            } else {
                '='
            });
        }
        out
    }

    // test "Entry init rejects glyf allocation over limit"
    #[test]
    fn entry_init_rejects_glyf_allocation_over_limit() {
        // 5,462 points require a 65,544-byte points allocation.
        // const req = try testRepeatedPointRegisterReq(alloc, &.{5461});
        let req = test_repeated_point_register_req(&[5461]);
        // defer alloc.free(req.raw);

        // try testing.expectError(error.PayloadTooLarge, Entry.init(alloc, req));
        assert_eq!(
            Err(EntryInitError::PayloadTooLarge),
            Entry::init(crate::alloc::Global, &req).map(|_| ())
        );
    }

    // test "Entry init accepts largest glyf points allocation under limit"
    #[test]
    fn entry_init_accepts_largest_glyf_points_allocation_under_limit() {
        // 5,461 points require a 65,532-byte points allocation.
        // const req = try testRepeatedPointRegisterReq(alloc, &.{ 0, 5460 });
        let req = test_repeated_point_register_req(&[0, 5460]);
        // defer alloc.free(req.raw);

        // var entry = try Entry.init(alloc, req);
        // defer entry.deinit(alloc);
        let mut entry = Entry::init(crate::alloc::Global, &req).unwrap();
        // try testing.expectEqual(@as(usize, 5461), entry.glyph.glyf.points.len);
        // try testing.expectEqual(@as(usize, 2), entry.glyph.glyf.contours.len);
        let Glyph::Glyf(outline) = &entry.glyph;
        assert_eq!(5461usize, outline.points.len());
        assert_eq!(2usize, outline.contours.len());
        entry.deinit();
    }

    #[test]
    fn glossary_register_overwrites_and_moves_entry_to_newest_position() {
        // var glossary: Glossary = .empty;
        let mut glossary = Glossary::empty();

        glossary
            .register(0xE000, test_register_entry(0xE000))
            .unwrap();
        glossary
            .register(0xE001, test_register_entry(0xE001))
            .unwrap();
        glossary
            .register(0xE000, test_register_entry(0xE000))
            .unwrap();

        // try testing.expectEqual(@as(usize, 2), glossary.entries.count());
        assert_eq!(2usize, glossary.entries.count());
        // try testing.expectEqual(@as(u21, 0xE001), glossary.entries.keys()[0]);
        let keys: Vec<u32> = glossary.entries.keys().copied().collect();
        assert_eq!(0xE001u32, keys[0]);
        // try testing.expectEqual(@as(u21, 0xE000), glossary.entries.keys()[1]);
        assert_eq!(0xE000u32, keys[1]);
    }

    #[test]
    fn glossary_register_evicts_oldest_entry() {
        let mut glossary = Glossary::empty();

        // for (0..max_entries + 1) |i| {
        for i in 0..(MAX_ENTRIES + 1) {
            // const cp: u21 = @intCast(0xE000 + i);
            let cp: u32 = (0xE000 + i) as u32;
            glossary.register(cp, test_register_entry(cp)).unwrap();
        }

        // try testing.expectEqual(@as(usize, max_entries), glossary.entries.count());
        assert_eq!(MAX_ENTRIES, glossary.entries.count());
        // try testing.expect(!glossary.entries.contains(0xE000));
        assert!(!glossary.entries.contains(&0xE000));
        // try testing.expect(glossary.entries.contains(0xE001));
        assert!(glossary.entries.contains(&0xE001));
        // try testing.expect(glossary.entries.contains(0xE000 + max_entries));
        assert!(glossary.entries.contains(&(0xE000 + MAX_ENTRIES as u32)));
    }

    #[test]
    fn glossary_register_rejects_non_pua_codepoint() {
        let mut glossary = Glossary::empty();

        // var entry = try testRegisterEntry(alloc, 0xE000);
        let entry = test_register_entry(0xE000);
        // try testing.expectError(error.OutOfNamespace, glossary.register(alloc, 'A', entry));
        assert_eq!(
            Err(RegisterError::OutOfNamespace),
            glossary.register('A' as u32, entry)
        );
    }

    #[test]
    fn glossary_delete_removes_one_pua_slot_and_ignores_empty_pua_slot() {
        let mut glossary = Glossary::empty();

        glossary
            .register(0xE000, test_register_entry(0xE000))
            .unwrap();
        glossary
            .register(0xE001, test_register_entry(0xE001))
            .unwrap();

        // try glossary.delete(alloc, 0xE000);
        glossary.delete(0xE000).unwrap();
        // try testing.expectEqual(@as(usize, 1), glossary.entries.count());
        assert_eq!(1usize, glossary.entries.count());
        // try testing.expect(!glossary.contains(0xE000));
        assert!(!glossary.contains(0xE000));
        // try testing.expect(glossary.contains(0xE001));
        assert!(glossary.contains(0xE001));

        // try glossary.delete(alloc, 0xE000);
        glossary.delete(0xE000).unwrap();
        assert_eq!(1usize, glossary.entries.count());
        assert!(glossary.contains(0xE001));
    }

    #[test]
    fn glossary_delete_rejects_non_pua_codepoint() {
        let mut glossary = Glossary::empty();

        glossary
            .register(0xE000, test_register_entry(0xE000))
            .unwrap();
        // try testing.expectError(error.OutOfNamespace, glossary.delete(alloc, 'A'));
        assert_eq!(Err(ClearError::OutOfNamespace), glossary.delete('A' as u32));
        // try testing.expectEqual(@as(usize, 1), glossary.entries.count());
        assert_eq!(1usize, glossary.entries.count());
        // try testing.expect(glossary.contains(0xE000));
        assert!(glossary.contains(0xE000));
    }

    #[test]
    fn glossary_clear_and_free_removes_all_slots_and_remains_reusable() {
        let mut glossary = Glossary::empty();

        glossary
            .register(0xE000, test_register_entry(0xE000))
            .unwrap();
        glossary
            .register(0xE001, test_register_entry(0xE001))
            .unwrap();

        // glossary.clearAndFree(alloc);
        glossary.clear_and_free();
        // try testing.expectEqual(@as(usize, 0), glossary.entries.count());
        assert_eq!(0usize, glossary.entries.count());
        assert!(!glossary.contains(0xE000));
        assert!(!glossary.contains(0xE001));

        glossary
            .register(0xE002, test_register_entry(0xE002))
            .unwrap();
        assert_eq!(1usize, glossary.entries.count());
        assert!(glossary.contains(0xE002));
    }

    #[test]
    fn glossary_contains_reports_registered_slots() {
        let mut glossary = Glossary::empty();

        // try testing.expect(!glossary.contains(0xE000));
        assert!(!glossary.contains(0xE000));

        glossary
            .register(0xE000, test_register_entry(0xE000))
            .unwrap();
        assert!(glossary.contains(0xE000));
        assert!(!glossary.contains(0xE001));
    }

    // Parity tail S7-3 routing evidence (Rust-only): the glossary's map grows
    // and frees through the terminal's allocator (`Glossary::empty_in`), and a
    // refused growth is `error.OutOfMemory` with the glossary unchanged.
    #[test]
    fn glossary_map_routes_through_the_allocator_handle() {
        use crate::alloc::testing::{with_handle, Config, CountingAllocator, FailingAllocator};

        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut glossary = Glossary::empty_in(alloc);
            assert_eq!(0, counting.alloc_calls.get(), ".empty allocates nothing");
            glossary
                .register(0xE000, test_register_entry(0xE000))
                .unwrap();
            // The entries array and its index.
            assert_eq!(
                2,
                counting.alloc_calls.get(),
                "entries + index come from alloc"
            );
            glossary.clear_and_free();
            assert_eq!(
                0,
                counting.live_bytes.get(),
                "clearAndFree returns the storage"
            );
        });

        let failing = FailingAllocator::over_heap(Config {
            fail_index: 0,
            ..Config::default()
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut glossary = Glossary::empty_in(alloc);
            assert_eq!(
                Err(RegisterError::OutOfMemory),
                glossary.register(0xE000, test_register_entry(0xE000))
            );
            assert!(failing.has_induced_failure.get());
            assert!(!glossary.contains(0xE000));
        });
    }
}
