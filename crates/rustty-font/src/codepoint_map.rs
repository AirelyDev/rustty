//! CodepointMap is a map of codepoints to a discovery descriptor of a font
//! to use for that codepoint. If the descriptor doesn't return any matching
//! font, the codepoint is rendered using the default font.
// const CodepointMap = @This();
//
// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const discovery = @import("discovery.zig");
//
// PORT NOTE: `assert = quirks.inlineAssert` lives in `rustty-util` (the FIRM leaf
// crate); `std.mem.Allocator` is modeled by `allocator_api2::alloc::Allocator`.
//
// PORT NOTE (discovery seam — R4): rustty-font is PURE and must not pull a
// font-discovery C-lib edge. The reference's `discovery.Descriptor` lives in
// `discovery.zig`, which imports fontconfig/macos/CoreText; the real discovery
// surface lands later in the rustty-font-shaper tier (F3). CodepointMap only
// depends on the *type* `Descriptor` for storage, `clone`, and `hash`, so here
// we SEAM a thin local `Descriptor` carrying the single field CodepointMap
// actually exercises (`family`). The full discovery.Descriptor (style/codepoint/
// size/bold/italic/monospace/variations/toFcPattern) supersedes this seam when
// discovery is ported; nothing here pulls a C/vt edge.
#![allow(non_snake_case)]

use allocator_api2::alloc::{Allocator, Global};
use allocator_api2::vec::Vec;
use core::hash::Hasher;
use rustty_util::quirks::inlineAssert as assert;

/// The allocation-failure error, mirroring Zig's `Allocator.Error` / OutOfMemory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutOfMemory;

/// Thin discovery-descriptor seam (see the discovery-seam PORT NOTE above).
/// In the reference this is `discovery.Descriptor`; here it carries only the
/// `family` field that CodepointMap stores, clones, and hashes.
// PORT NOTE: `?[:0]const u8` becomes `Option<Vec<u8>>` — the byte-slice fidelity
// rule maps Zig byte slices to `Vec<u8>`, never a lossy `String`.
#[derive(Clone, Default, Debug)]
pub struct Descriptor {
    /// Font family to search for.
    // PORT NOTE: this seam's owned byte string is `Global`-backed (`std::vec::
    // Vec<u8>`), distinct from the `allocator_api2::vec::Vec` used for `list`.
    pub family: Option<std::vec::Vec<u8>>,
}

impl Descriptor {
    /// Deep copy of the struct. The given allocator is expected to
    /// be an arena allocator of some sort since the descriptor
    /// itself doesn't support fine-grained deallocation of fields.
    // PORT NOTE: seam of `discovery.Descriptor.clone`; only the `family` byte
    // slice is duped (`alloc.dupeZ(u8, src)`). The `_alloc` parameter mirrors the
    // reference signature — the seam's `Vec<u8>` is backed by `Global`.
    pub fn clone<A: Allocator>(&self, _alloc: A) -> Result<Descriptor, OutOfMemory> {
        Ok(Descriptor {
            family: self.family.clone(),
        })
    }

    /// Hash the descriptor with the given hasher.
    // PORT NOTE: seam of `discovery.Descriptor.hash`; mirrors `autoHashStrat(.Deep)`
    // over the `family` byte slice. Only `family` is present in this seam.
    // `clippy::should_implement_trait` flags the name collision with
    // `std::hash::Hash::hash`, allowed to keep the faithful method name.
    #[allow(clippy::should_implement_trait)]
    pub fn hash<H: Hasher>(&self, hasher: &mut H) {
        match &self.family {
            Some(f) => {
                hasher.write_u8(1);
                hasher.write(f);
            }
            None => hasher.write_u8(0),
        }
    }
}

/// pub const Entry = struct { ... };
#[derive(Clone)]
pub struct Entry {
    /// Unicode codepoint range. Asserts range[0] <= range[1].
    // range: [2]u21,
    //
    // PORT NOTE: Rust has no `u21`; the smallest faithful carrier is `u32` (the
    // codepoint values all fit in 21 bits).
    pub range: [u32; 2],

    /// The discovery descriptor of the font to use for this range.
    pub descriptor: Descriptor,
}

/// The list of entries. We use a multiarraylist because Descriptors are
/// quite large and we will very rarely match, so we'd rather pack our
/// ranges together to make everything more cache friendly for lookups.
///
/// Note: we just do a linear search because we expect to always have very
/// few entries, so the overhead of a binary search is not worth it. This is
/// possible to defeat with some pathological inputs, but there is no realistic
/// scenario where this will be a problem except people trying to fuck around.
// list: std.MultiArrayList(Entry) = .{},
//
// PORT NOTE: `std.MultiArrayList(Entry)` is a struct-of-arrays optimization for
// cache-friendly range scans; here it is modeled as a plain `Vec<Entry, A>`
// (array-of-structs). The linear-search *behavior* is identical — only the
// (unobservable) memory layout differs. The allocator is carried as a generic
// `A: Allocator` (defaulting to `Global`) so `add`/`clone`/`deinit` thread it.
pub struct CodepointMap<A: Allocator = Global> {
    list: Vec<Entry, A>,
}

impl Default for CodepointMap<Global> {
    // CodepointMap = .{}
    fn default() -> Self {
        CodepointMap {
            list: Vec::new_in(Global),
        }
    }
}

impl<A: Allocator> CodepointMap<A> {
    // pub fn deinit(self: *CodepointMap, alloc: Allocator) void {
    //     self.list.deinit(alloc);
    // }
    //
    // PORT NOTE: the allocator owned by `list` frees its storage on drop; we take
    // `alloc` to mirror the reference signature.
    pub fn deinit(self, _alloc: A) {
        let _ = self;
    }

    /// Deep copy of the struct. The given allocator is expected to
    /// be an arena allocator of some sort since the struct itself
    /// doesn't support fine-grained deallocation of fields.
    // pub fn clone(self: *const CodepointMap, alloc: Allocator) !CodepointMap {
    pub fn clone(&self, alloc: A) -> Result<CodepointMap<A>, OutOfMemory>
    where
        A: Copy,
    {
        // var list = try self.list.clone(alloc);
        let mut list: Vec<Entry, A> = Vec::new_in(alloc);
        list.try_reserve_exact(self.list.len())
            .map_err(|_| OutOfMemory)?;
        for e in self.list.iter() {
            list.push(e.clone());
        }
        // for (list.items(.descriptor)) |*d| {
        //     d.* = try d.clone(alloc);
        // }
        for d in list.iter_mut() {
            d.descriptor = d.descriptor.clone(alloc)?;
        }

        // return .{ .list = list };
        Ok(CodepointMap { list })
    }

    /// Add an entry to the map.
    ///
    /// For conflicting codepoints, entries added later take priority over
    /// entries added earlier.
    // pub fn add(self: *CodepointMap, alloc: Allocator, entry: Entry) !void {
    pub fn add(&mut self, _alloc: A, entry: Entry) -> Result<(), OutOfMemory> {
        // assert(entry.range[0] <= entry.range[1]);
        assert(entry.range[0] <= entry.range[1]);
        // try self.list.append(alloc, entry);
        self.list.try_reserve(1).map_err(|_| OutOfMemory)?;
        self.list.push(entry);
        Ok(())
    }

    /// Get a descriptor for a codepoint.
    // pub fn get(self: *const CodepointMap, cp: u21) ?discovery.Descriptor {
    //
    // PORT NOTE: the reference returns a (shallow) struct copy of the stored
    // descriptor; this seam returns an owned `Descriptor` clone, which is
    // behavior-identical for callers that only read it.
    pub fn get(&self, cp: u32) -> Option<Descriptor> {
        // const items = self.list.items(.range);
        // for (0..items.len) |forward_i| {
        let len = self.list.len();
        for forward_i in 0..len {
            // const i = items.len - forward_i - 1;
            let i = len - forward_i - 1;
            // const range = items[i];
            let range = self.list[i].range;
            // if (range[0] <= cp and cp <= range[1]) {
            if range[0] <= cp && cp <= range[1] {
                // const descs = self.list.items(.descriptor);
                // return descs[i];
                //
                // PORT NOTE: the inherent `Descriptor::clone(&self, alloc)` (the
                // discovery-descriptor deep clone) shadows the derived `Clone`, so
                // we disambiguate to the trait for this shallow value copy.
                return Some(Clone::clone(&self.list[i].descriptor));
            }
        }

        // return null;
        None
    }

    /// Hash with the given hasher.
    // pub fn hash(self: *const CodepointMap, hasher: anytype) void {
    // `clippy::should_implement_trait` flags the `hash` name collision with
    // `std::hash::Hash::hash`, allowed to keep the faithful method name.
    #[allow(clippy::should_implement_trait)]
    pub fn hash<H: Hasher>(&self, hasher: &mut H) {
        // const autoHash = std.hash.autoHash;
        // autoHash(hasher, self.list.len);
        hasher.write_usize(self.list.len());
        // const slice = self.list.slice();
        // for (0..slice.len) |i| {
        for i in 0..self.list.len() {
            // const entry = slice.get(i);
            let entry = &self.list[i];
            // autoHash(hasher, entry.range);
            hasher.write_u32(entry.range[0]);
            hasher.write_u32(entry.range[1]);
            // entry.descriptor.hash(hasher);
            entry.descriptor.hash(hasher);
        }
    }

    /// Returns a hash code that can be used to uniquely identify this
    /// action.
    // pub fn hashcode(self: *const CodepointMap) u64 {
    //
    // PORT NOTE: the reference uses `std.hash.Wyhash`; this hash is never
    // oracle-compared or serialized, so any self-consistent hasher is parity-
    // preserving. We use the std `DefaultHasher`, matching the `hash`/`hashcode`
    // seam used elsewhere in the port.
    pub fn hashcode(&self) -> u64 {
        // var hasher = std.hash.Wyhash.init(0);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        // self.hash(&hasher);
        self.hash(&mut hasher);
        // return hasher.final();
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "codepointmap"
    #[test]
    fn codepointmap() {
        // const testing = std.testing;
        // const alloc = testing.allocator;
        let alloc = Global;

        // var m: CodepointMap = .{};
        // defer m.deinit(alloc);
        let mut m: CodepointMap = CodepointMap::default();

        // Exact range
        // try testing.expect(m.get(1) == null);
        assert!(m.get(1).is_none());
        // try m.add(alloc, .{ .range = .{ 1, 1 }, .descriptor = .{ .family = "A" } });
        m.add(
            alloc,
            Entry {
                range: [1, 1],
                descriptor: Descriptor {
                    family: Some(b"A".to_vec()),
                },
            },
        )
        .unwrap();
        {
            // const d = m.get(1).?;
            let d = m.get(1).unwrap();
            // try testing.expectEqualStrings("A", d.family.?);
            assert_eq!(b"A".as_slice(), d.family.as_deref().unwrap());
        }

        // Later entry takes priority
        // try m.add(alloc, .{ .range = .{ 1, 2 }, .descriptor = .{ .family = "B" } });
        m.add(
            alloc,
            Entry {
                range: [1, 2],
                descriptor: Descriptor {
                    family: Some(b"B".to_vec()),
                },
            },
        )
        .unwrap();
        {
            // const d = m.get(1).?;
            let d = m.get(1).unwrap();
            // try testing.expectEqualStrings("B", d.family.?);
            assert_eq!(b"B".as_slice(), d.family.as_deref().unwrap());
        }

        // Non-matching
        // try testing.expect(m.get(0) == null);
        assert!(m.get(0).is_none());
        // try testing.expect(m.get(3) == null);
        assert!(m.get(3).is_none());

        // try m.add(alloc, .{ .range = .{ 3, 4 }, .descriptor = .{ .family = "C" } });
        m.add(
            alloc,
            Entry {
                range: [3, 4],
                descriptor: Descriptor {
                    family: Some(b"C".to_vec()),
                },
            },
        )
        .unwrap();
        // try m.add(alloc, .{ .range = .{ 5, 6 }, .descriptor = .{ .family = "D" } });
        m.add(
            alloc,
            Entry {
                range: [5, 6],
                descriptor: Descriptor {
                    family: Some(b"D".to_vec()),
                },
            },
        )
        .unwrap();
        {
            // const d = m.get(3).?;
            let d = m.get(3).unwrap();
            // try testing.expectEqualStrings("C", d.family.?);
            assert_eq!(b"C".as_slice(), d.family.as_deref().unwrap());
        }
        {
            // const d = m.get(1).?;
            let d = m.get(1).unwrap();
            // try testing.expectEqualStrings("B", d.family.?);
            assert_eq!(b"B".as_slice(), d.family.as_deref().unwrap());
        }

        m.deinit(alloc);
    }
}
