//! rustty-vt-c kitty_graphics
//!
//! PORTS: ghostty src/terminal/c/kitty_graphics.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const testing = std.testing;
//   const build_options = @import("terminal_options");
//   const lib = @import("../lib.zig");
//   const CAllocator = lib.alloc.Allocator;
//   const kitty_storage = @import("../kitty/graphics_storage.zig");
//   const kitty_cmd = @import("../kitty/graphics_command.zig");
//   const Image = @import("../kitty/graphics_image.zig").Image;
//   const grid_ref = @import("grid_ref.zig");
//   const selection_c = @import("selection.zig");
//   const terminal_c = @import("terminal.zig");
//   const Terminal = @import("../Terminal.zig");
//   const Result = @import("result.zig").Result;
// `build_options.kitty_graphics` is `rustty_vt::options::KITTY_GRAPHICS`;
// `lib.alloc.Allocator` is `rustty_lib::allocator::Allocator`; the kitty storage
// types come from `rustty_vt::kitty`; `grid_ref`/`selection_c`/`terminal_c` are
// the sibling vt-c modules; `Terminal` is `rustty_vt::terminal::Terminal`.

use crate::cwrap::{create, destroy};
use crate::grid_ref::CGridRef;
use crate::result::Result;
use crate::selection::CSelection;
use crate::terminal::Terminal as CTerminal;
use rustty_lib::allocator::Allocator as CAllocator;
use rustty_vt::kitty::graphics_command::Transmission;
use rustty_vt::kitty::graphics_image::Image;
use rustty_vt::kitty::graphics_storage::{ImageStorage, Location, Placement, PlacementKey};
use rustty_vt::options::KITTY_GRAPHICS;
use rustty_vt::point::Tag;
use rustty_vt::terminal::Terminal as ZigTerminal;
use std::collections::hash_map;
use std::ffi::c_void;
use std::os::raw::c_int;

/// C: GhosttyKittyGraphics -> RusttyKittyGraphics (R2)
// PORT NOTE: the reference is
//   pub const KittyGraphics = if (build_options.kitty_graphics)
//       *kitty_storage.ImageStorage
//   else
//       *anyopaque;
// i.e. a non-null pointer to the screen's `ImageStorage`. The C ABI is `*mut
// c_void` either way (an opaque pointer), so the public handle stays `*mut
// c_void` (matching what `terminal.get(.kitty_graphics)` writes: `&mut
// screen.kitty_images` cast to `*mut c_void`). Internally every `get` recovers
// the concrete `*mut ImageStorage` from this opaque pointer.
pub type KittyGraphics = *mut c_void;

/// C: GhosttyKittyGraphicsImage -> RusttyKittyGraphicsImage (R2)
// PORT NOTE: the reference is
//   pub const ImageHandle = if (build_options.kitty_graphics)
//       ?*const Image
//   else
//       ?*const anyopaque;
// a nullable const pointer to an `Image` owned by the storage. Ported as a raw
// `*const Image` (null == none).
pub type ImageHandle = *const Image;

/// C: GhosttyKittyGraphicsPlacementIterator -> RusttyKittyGraphicsPlacementIterator (R2)
// PORT NOTE: the reference is
//   pub const PlacementIterator = if (build_options.kitty_graphics)
//       ?*PlacementIteratorWrapper
//   else
//       ?*anyopaque;
// a nullable pointer to the heap-allocated iterator wrapper. Ported as a raw
// `*mut PlacementIteratorWrapper` (null == none).
pub type PlacementIterator = *mut PlacementIteratorWrapper;

// const PlacementMap = if (build_options.kitty_graphics)
//     std.AutoHashMapUnmanaged(
//         kitty_storage.ImageStorage.PlacementKey,
//         kitty_storage.ImageStorage.Placement,
//     )
// else
//     void;
//
// PORT NOTE: this is the same map type as `ImageStorage.placements`
// (`HashMap<PlacementKey, Placement>`). We don't need a standalone alias; the
// iterator wrapper holds the map's `Iter` and `Entry` types directly.

// const PlacementIteratorWrapper = if (build_options.kitty_graphics)
//     struct {
//         alloc: std.mem.Allocator,
//         inner: PlacementMap.Iterator = undefined,
//         entry: ?PlacementMap.Entry = null,
//         layer_filter: PlacementLayer = .all,
//     }
// else
//     void;
//
// PORT NOTE (lifetimes): the reference `PlacementMap.Iterator` is an unmanaged
// `std.AutoHashMapUnmanaged` iterator that holds raw pointers into the map's
// metadata/entries; it has no lifetime tracked by the type system. rustty's
// `std::collections::hash_map::Iter<'a, K, V>` borrows the map, but `get`
// re-seats `inner` from `storage.placements.iter()` (the reference re-seats from
// `storage.placements.iterator()`), and the storage outlives the iterator
// wrapper (it lives in the screen). We therefore erase the borrow to `'static`
// (faithfully matching the reference's untracked unmanaged iterator) and store
// the current entry as raw `key_ptr`/`value_ptr` pointers (the reference's
// `Entry { key_ptr, value_ptr }`). The wrapper stores the original (possibly
// null) C `Allocator` from which `free` resolves the identical allocator (see
// `cwrap`), mirroring the reference's stored `alloc: std.mem.Allocator`.
pub struct PlacementIteratorWrapper {
    alloc: Option<CAllocator>,
    inner: Option<hash_map::Iter<'static, PlacementKey, Placement>>,
    entry: Option<(*const PlacementKey, *const Placement)>,
    layer_filter: PlacementLayer,
}

/// C: GhosttyKittyGraphicsData -> RusttyKittyGraphicsData (R2)
// pub const Data = enum(c_int) { invalid = 0, placement_iterator = 1, generation = 2, ... };
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Data {
    Invalid = 0,
    PlacementIterator = 1,
    Generation = 2,
}

// D11 (P3-W6 T2): proves taking this value as a `c_int` in the exported
// signature leaves the ABI unchanged.
const _: () = assert!(std::mem::size_of::<Data>() == std::mem::size_of::<std::os::raw::c_int>());

impl Data {
    /// Narrow an inbound C integer to a `Data`, mirroring
    /// `std.meta.intToEnum`; `None` outside the declared range.
    ///
    /// D11 (P3-W6 T2): `rustty_kitty_graphics_get` takes the selector as a
    /// plain `c_int` and narrows it HERE. A `#[repr(C)]` enum parameter is
    /// materialized in the callee prologue, so an out-of-range tag is UB before
    /// any check can run, and the published header declares
    /// `RUSTTY_KITTY_GRAPHICS_DATA_MAX_VALUE`. See `modes.rs:19`.
    /// `Invalid = 0` is IN range and keeps its existing `.invalid_value`
    /// disposition inside `get`; only genuinely undeclared integers are `None`.
    ///
    /// PRE-EXISTING: predates the Phase 3 resync range.
    pub fn from_c_int(v: std::os::raw::c_int) -> Option<Data> {
        match v {
            0 => Some(Data::Invalid),
            1 => Some(Data::PlacementIterator),
            2 => Some(Data::Generation),
            _ => None,
        }
    }
}

// PORT NOTE: the reference `Data.OutType` maps `.invalid => void`,
// `.placement_iterator => PlacementIterator`, `.generation => u64`. The typed
// body below dispatches on the tag and casts `out` to the matching type, so the
// comptime `OutType` function is realized inline (no separate type-level
// function).

/// C: GhosttyKittyGraphicsPlacementData -> RusttyKittyGraphicsPlacementData (R2)
// pub const PlacementData = enum(c_int) { invalid=0, image_id=1, ..., z=12 };
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlacementData {
    Invalid = 0,
    ImageId = 1,
    PlacementId = 2,
    IsVirtual = 3,
    XOffset = 4,
    YOffset = 5,
    SourceX = 6,
    SourceY = 7,
    SourceWidth = 8,
    SourceHeight = 9,
    Columns = 10,
    Rows = 11,
    Z = 12,
}

// D11 (P3-W6 T2): see `Data::from_c_int`; same rule, same header hazard
// (`RUSTTY_KITTY_GRAPHICS_PLACEMENT_DATA_MAX_VALUE`). PRE-EXISTING.
const _: () =
    assert!(std::mem::size_of::<PlacementData>() == std::mem::size_of::<std::os::raw::c_int>());

impl PlacementData {
    /// Narrow an inbound C integer to a `PlacementData`, mirroring
    /// `std.meta.intToEnum`; `None` outside the declared range.
    pub fn from_c_int(v: std::os::raw::c_int) -> Option<PlacementData> {
        match v {
            0 => Some(PlacementData::Invalid),
            1 => Some(PlacementData::ImageId),
            2 => Some(PlacementData::PlacementId),
            3 => Some(PlacementData::IsVirtual),
            4 => Some(PlacementData::XOffset),
            5 => Some(PlacementData::YOffset),
            6 => Some(PlacementData::SourceX),
            7 => Some(PlacementData::SourceY),
            8 => Some(PlacementData::SourceWidth),
            9 => Some(PlacementData::SourceHeight),
            10 => Some(PlacementData::Columns),
            11 => Some(PlacementData::Rows),
            12 => Some(PlacementData::Z),
            _ => None,
        }
    }
}

// PORT NOTE: the reference `PlacementData.OutType` maps `.image_id`/`.placement_id`
// and the geometry fields to `u32`, `.is_virtual` to `bool`, and `.z` to `i32`.
// Realized inline in `placement_get_typed`.

// pub fn get(graphics_, data, out) callconv(...) Result {
//     if (comptime !build_options.kitty_graphics) return .no_value;
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| getTyped(graphics_, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
/// # Safety
/// `graphics_` must be a valid `KittyGraphics` handle (a `*mut ImageStorage`);
/// `out` must point to writable storage of the type required by `data`.
pub unsafe fn get(graphics_: KittyGraphics, data: Data, out: *mut c_void) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    match data {
        Data::Invalid => Result::InvalidValue,
        // inline else => getTyped(graphics_, comptime_data, @ptrCast(@alignCast(out)))
        Data::PlacementIterator | Data::Generation => unsafe { get_typed(graphics_, data, out) },
    }
}

// fn getTyped(graphics_, comptime data, out: *data.OutType()) Result {
//     const storage = graphics_;
//     switch (data) {
//         .invalid => return .invalid_value,
//         .placement_iterator => {
//             const it = out.* orelse return .invalid_value;
//             it.* = .{
//                 .alloc = it.alloc,
//                 .inner = storage.placements.iterator(),
//                 .layer_filter = it.layer_filter,
//             };
//         },
//     }
//     return .success;
// }
/// # Safety
/// `graphics_` is a valid `*mut ImageStorage`; `out` points to storage of the
/// type required by `data`.
unsafe fn get_typed(graphics_: KittyGraphics, data: Data, out: *mut c_void) -> Result {
    // const storage = graphics_;
    let storage = graphics_ as *mut ImageStorage;
    match data {
        Data::Invalid => Result::InvalidValue,
        Data::PlacementIterator => {
            // const it = out.* orelse return .invalid_value;
            // SAFETY: `out` points to a `PlacementIterator` (caller contract).
            let it = unsafe { *(out as *mut PlacementIterator) };
            if it.is_null() {
                return Result::InvalidValue;
            }
            // it.* = .{ .alloc = it.alloc, .inner = storage.placements.iterator(), .layer_filter = it.layer_filter };
            // PORT NOTE: re-seats `inner` (and resets `entry` to its `null`
            // default) while preserving `alloc` and `layer_filter`. The
            // `'static` erasure matches the reference's untracked unmanaged
            // iterator (see `PlacementIteratorWrapper` PORT NOTE); the storage
            // outlives the iterator.
            // SAFETY: `storage` is the live `ImageStorage` behind the handle.
            let iter: hash_map::Iter<'_, PlacementKey, Placement> =
                unsafe { (*storage).placements.iter() };
            let iter_static: hash_map::Iter<'static, PlacementKey, Placement> =
                unsafe { core::mem::transmute(iter) };
            // SAFETY: `it` is a live wrapper (checked non-null above).
            unsafe {
                (*it).inner = Some(iter_static);
                (*it).entry = None;
            }
            Result::Success
        }
        // .generation => out.* = storage.generation,
        Data::Generation => {
            // SAFETY: `out` points to a writable `u64` (caller contract);
            // `storage` is the live `ImageStorage` behind the handle.
            unsafe { *(out as *mut u64) = (*storage).generation };
            Result::Success
        }
    }
}

/// C: GhosttyKittyPlacementLayer -> RusttyKittyPlacementLayer (R2)
// pub const PlacementLayer = enum(c_int) { all=0, below_bg=1, below_text=2, above_text=3, ... };
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlacementLayer {
    All = 0,
    BelowBg = 1,
    BelowText = 2,
    AboveText = 3,
}

impl PlacementLayer {
    // fn matches(self: PlacementLayer, z: i32) bool {
    //     return switch (self) {
    //         .all => true,
    //         .below_bg => z < std.math.minInt(i32) / 2,
    //         .below_text => z >= std.math.minInt(i32) / 2 and z < 0,
    //         .above_text => z >= 0,
    //     };
    // }
    // PORT NOTE: clippy `manual_range_contains` would rewrite the `below_text`
    // arm as `(i32::MIN / 2..0).contains(&z)`, deviating from the reference's
    // explicit `z >= std.math.minInt(i32) / 2 and z < 0`. R1 keeps the literal
    // comparison form, so the lint is allowed here.
    #[allow(clippy::manual_range_contains)]
    fn matches(self, z: i32) -> bool {
        match self {
            PlacementLayer::All => true,
            PlacementLayer::BelowBg => z < i32::MIN / 2,
            PlacementLayer::BelowText => z >= i32::MIN / 2 && z < 0,
            PlacementLayer::AboveText => z >= 0,
        }
    }
}

/// C: GhosttyKittyGraphicsPlacementIteratorOption -> RusttyKittyGraphicsPlacementIteratorOption (R2)
// pub const PlacementIteratorOption = enum(c_int) { layer = 0, ... };
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlacementIteratorOption {
    Layer = 0,
}

// PORT NOTE: the reference `PlacementIteratorOption.InType` maps `.layer =>
// PlacementLayer`. Realized inline in `placement_iterator_set_typed`.

/// C: GhosttyKittyImageFormat -> RusttyKittyImageFormat (R2)
// pub const ImageFormat = kitty_cmd.Transmission.Format;
pub type ImageFormat = rustty_vt::kitty::graphics_command::Format;

/// C: GhosttyKittyImageCompression -> RusttyKittyImageCompression (R2)
// pub const ImageCompression = kitty_cmd.Transmission.Compression;
pub type ImageCompression = rustty_vt::kitty::graphics_command::Compression;

// PORT NOTE: silence unused-import warnings for `Transmission` — the reference
// names `kitty_cmd.Transmission.Format`/`.Compression`; in rustty those nested
// types are re-exported directly as `Format`/`Compression`, so the `Transmission`
// path is referenced only to mirror the reference import.
const _: fn() = || {
    let _ = core::mem::size_of::<Transmission>;
};

/// C: GhosttyKittyGraphicsImageData -> RusttyKittyGraphicsImageData (R2)
// pub const ImageData = enum(c_int) { invalid=0, id=1, ..., data_len=8, generation=9 };
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ImageData {
    Invalid = 0,
    Id = 1,
    Number = 2,
    Width = 3,
    Height = 4,
    Format = 5,
    Compression = 6,
    DataPtr = 7,
    DataLen = 8,
    Generation = 9,
}

// D11 (P3-W6 T2): see `Data::from_c_int`; same rule, same header hazard
// (`RUSTTY_KITTY_GRAPHICS_IMAGE_DATA_MAX_VALUE`). PRE-EXISTING.
const _: () =
    assert!(std::mem::size_of::<ImageData>() == std::mem::size_of::<std::os::raw::c_int>());

impl ImageData {
    /// Narrow an inbound C integer to an `ImageData`, mirroring
    /// `std.meta.intToEnum`; `None` outside the declared range.
    pub fn from_c_int(v: std::os::raw::c_int) -> Option<ImageData> {
        match v {
            0 => Some(ImageData::Invalid),
            1 => Some(ImageData::Id),
            2 => Some(ImageData::Number),
            3 => Some(ImageData::Width),
            4 => Some(ImageData::Height),
            5 => Some(ImageData::Format),
            6 => Some(ImageData::Compression),
            7 => Some(ImageData::DataPtr),
            8 => Some(ImageData::DataLen),
            9 => Some(ImageData::Generation),
            _ => None,
        }
    }
}

// PORT NOTE: the reference `ImageData.OutType` maps `.id`/`.number`/`.width`/
// `.height` to `u32`, `.format` to `ImageFormat`, `.compression` to
// `ImageCompression`, `.data_ptr` to `[*]const u8`, `.data_len` to `usize`,
// `.generation` to `u64`.
// Realized inline in `image_get_typed`.

// pub fn image_get_handle(graphics_, image_id) callconv(...) ImageHandle {
//     if (comptime !build_options.kitty_graphics) return null;
//     const storage = graphics_;
//     return storage.images.getPtr(image_id);
// }
/// # Safety
/// `graphics_` must be a valid `KittyGraphics` handle (a `*mut ImageStorage`).
pub unsafe fn image_get_handle(graphics_: KittyGraphics, image_id: u32) -> ImageHandle {
    // if (comptime !build_options.kitty_graphics) return null;
    if !KITTY_GRAPHICS {
        return core::ptr::null();
    }
    // const storage = graphics_;
    let storage = graphics_ as *mut ImageStorage;
    // return storage.images.getPtr(image_id);
    // SAFETY: `storage` is the live `ImageStorage` behind the handle.
    match unsafe { (*storage).images.get(&image_id) } {
        Some(image) => image as *const Image,
        None => core::ptr::null(),
    }
}

// pub fn image_get(image_, data, out) callconv(...) Result {
//     if (comptime !build_options.kitty_graphics) return .no_value;
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| imageGetTyped(image_, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
/// # Safety
/// `image_`, if non-null, must point to a valid `Image`; `out` must point to
/// writable storage of the type required by `data`.
pub unsafe fn image_get(image_: ImageHandle, data: ImageData, out: *mut c_void) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    match data {
        ImageData::Invalid => Result::InvalidValue,
        // inline else => imageGetTyped(image_, comptime_data, @ptrCast(@alignCast(out)))
        _ => unsafe { image_get_typed(image_, data, out) },
    }
}

// pub fn image_get_multi(image_, count, keys, values, out_written) callconv(...) Result {
//     if (comptime !build_options.kitty_graphics) return .no_value;
//     const k = keys orelse return .invalid_value;
//     const v = values orelse return .invalid_value;
//     for (0..count) |i| {
//         const result = image_get(image_, k[i], v[i]);
//         if (result != .success) { if (out_written) |w| w.* = i; return result; }
//     }
//     if (out_written) |w| w.* = count;
//     return .success;
// }
/// # Safety
/// `image_`, if non-null, points to a valid `Image`. `keys`/`values`, if
/// non-null, point to `count` readable entries each; `values[i]` must point to
/// writable storage of the type required by `keys[i]`. `out_written`, if
/// non-null, must be writable.
pub unsafe fn image_get_multi(
    image_: ImageHandle,
    count: usize,
    keys: *const ImageData,
    values: *const *mut c_void,
    out_written: *mut usize,
) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    // const k = keys orelse return .invalid_value;
    if keys.is_null() {
        return Result::InvalidValue;
    }
    // const v = values orelse return .invalid_value;
    if values.is_null() {
        return Result::InvalidValue;
    }

    // for (0..count) |i| { ... }
    for i in 0..count {
        // const result = image_get(image_, k[i], v[i]);
        // SAFETY: `keys`/`values` point to `count` readable entries (caller).
        let key = unsafe { *keys.add(i) };
        let val = unsafe { *values.add(i) };
        let result = unsafe { image_get(image_, key, val) };
        // if (result != .success) { if (out_written) |w| w.* = i; return result; }
        if result != Result::Success {
            if !out_written.is_null() {
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    // if (out_written) |w| w.* = count;
    if !out_written.is_null() {
        unsafe { *out_written = count };
    }
    // return .success;
    Result::Success
}

// fn imageGetTyped(image_, comptime data, out: *data.OutType()) Result {
//     const image = image_ orelse return .invalid_value;
//     switch (data) {
//         .invalid => return .invalid_value,
//         .id => out.* = image.id, ...
//         .data_ptr => out.* = (image.data.bytes() orelse return .no_value).ptr,
//         .data_len => out.* = image.data.len(),
//     }
//     return .success;
// }
/// # Safety
/// `image_`, if non-null, points to a valid `Image`; `out` points to storage of
/// the type required by `data`.
unsafe fn image_get_typed(image_: ImageHandle, data: ImageData, out: *mut c_void) -> Result {
    // const image = image_ orelse return .invalid_value;
    if image_.is_null() {
        return Result::InvalidValue;
    }
    // SAFETY: `image_` is non-null and points to a live `Image` (caller).
    let image = unsafe { &*image_ };

    match data {
        // .invalid => return .invalid_value,
        ImageData::Invalid => return Result::InvalidValue,
        // .id => out.* = image.id,
        ImageData::Id => unsafe { *(out as *mut u32) = image.id },
        // .number => out.* = image.number,
        ImageData::Number => unsafe { *(out as *mut u32) = image.number },
        // .width => out.* = image.width,
        ImageData::Width => unsafe { *(out as *mut u32) = image.width },
        // .height => out.* = image.height,
        ImageData::Height => unsafe { *(out as *mut u32) = image.height },
        // .format => out.* = image.format,
        ImageData::Format => unsafe { *(out as *mut ImageFormat) = image.format },
        // .compression => out.* = image.compression,
        ImageData::Compression => unsafe { *(out as *mut ImageCompression) = image.compression },
        // .data_ptr => out.* = (image.data.bytes() orelse return .no_value).ptr,
        ImageData::DataPtr => match image.data.bytes() {
            Some(bytes) => unsafe { *(out as *mut *const u8) = bytes.as_ptr() },
            None => return Result::NoValue,
        },
        // .data_len => out.* = image.data.len(),
        ImageData::DataLen => unsafe { *(out as *mut usize) = image.data.len() },
        // .generation => out.* = image.generation,
        ImageData::Generation => unsafe { *(out as *mut u64) = image.generation },
    }

    // return .success;
    Result::Success
}

// pub fn placement_iterator_new(alloc_, out: *PlacementIterator) callconv(...) Result {
//     if (comptime !build_options.kitty_graphics) { out.* = null; return .no_value; }
//     const alloc = lib.alloc.default(alloc_);
//     const ptr = alloc.create(PlacementIteratorWrapper) catch { out.* = null; return .out_of_memory; };
//     ptr.* = .{ .alloc = alloc };
//     out.* = ptr;
//     return .success;
// }
/// # Safety
/// `alloc_`, if non-null, must point to a valid `CAllocator`; `out` must be a
/// valid writable pointer to a `PlacementIterator`.
pub unsafe fn placement_iterator_new(
    alloc_: Option<&CAllocator>,
    out: *mut PlacementIterator,
) -> Result {
    // if (comptime !build_options.kitty_graphics) { out.* = null; return .no_value; }
    if !KITTY_GRAPHICS {
        unsafe { *out = core::ptr::null_mut() };
        return Result::NoValue;
    }
    // const alloc = lib.alloc.default(alloc_);
    // const ptr = alloc.create(PlacementIteratorWrapper) catch { ... };
    // ptr.* = .{ .alloc = alloc };
    // PORT NOTE: the reference stores the resolved `std.mem.Allocator`; rustty
    // stores the original (possibly null) C `Allocator` from which `free`
    // resolves the identical allocator (see `cwrap`). The `inner`/`entry`/
    // `layer_filter` fields ride their reference defaults (`undefined`/`null`/
    // `.all`).
    let ptr = unsafe {
        create(
            alloc_,
            PlacementIteratorWrapper {
                alloc: alloc_.copied(),
                inner: None,
                entry: None,
                layer_filter: PlacementLayer::All,
            },
        )
    };
    if ptr.is_null() {
        // catch { out.* = null; return .out_of_memory; }
        unsafe { *out = core::ptr::null_mut() };
        return Result::OutOfMemory;
    }
    // out.* = ptr;
    unsafe { *out = ptr };
    // return .success;
    Result::Success
}

// pub fn placement_iterator_free(iter_: PlacementIterator) callconv(...) void {
//     if (comptime !build_options.kitty_graphics) return;
//     const iter = iter_ orelse return;
//     iter.alloc.destroy(iter);
// }
/// # Safety
/// `iter_`, if non-null, must be a pointer returned by `placement_iterator_new`,
/// not yet freed.
pub unsafe fn placement_iterator_free(iter_: PlacementIterator) {
    // if (comptime !build_options.kitty_graphics) return;
    if !KITTY_GRAPHICS {
        return;
    }
    // const iter = iter_ orelse return;
    if iter_.is_null() {
        return;
    }
    // iter.alloc.destroy(iter);
    // SAFETY: `iter_` is a live wrapper from `placement_iterator_new`; read its
    // stored allocator (a `Copy` C struct) then free via the matching allocator.
    let alloc = unsafe { (*iter_).alloc };
    unsafe { destroy(alloc.as_ref(), iter_) };
}

// pub fn placement_iterator_set(iter_, option, value) callconv(...) Result {
//     if (comptime !build_options.kitty_graphics) return .no_value;
//     if (comptime std.debug.runtime_safety) {
//         _ = std.meta.intToEnum(PlacementIteratorOption, @intFromEnum(option)) catch { return .invalid_value; };
//     }
//     return switch (option) {
//         inline else => |comptime_option| placementIteratorSetTyped(iter_, comptime_option, @ptrCast(@alignCast(value orelse return .invalid_value))),
//     };
// }
/// # Safety
/// `iter_`, if non-null, must be a live iterator wrapper; `value`, if non-null,
/// must point to a valid value of the type required by `option`.
pub unsafe fn placement_iterator_set(
    iter_: PlacementIterator,
    option: PlacementIteratorOption,
    value: *const c_void,
) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    // PORT NOTE: the `intToEnum` runtime-safety guard lives on the raw `c_int`
    // crossing the C boundary (see the `placement_iterator_set` extern wrapper in
    // `lib.rs`); the typed `option: PlacementIteratorOption` here is always in
    // range, so the guard is a no-op.

    // return switch (option) {
    //     inline else => placementIteratorSetTyped(iter_, comptime_option, @ptrCast(@alignCast(value orelse return .invalid_value))),
    // };
    if value.is_null() {
        return Result::InvalidValue;
    }
    match option {
        PlacementIteratorOption::Layer => unsafe {
            placement_iterator_set_typed(iter_, option, value)
        },
    }
}

// fn placementIteratorSetTyped(iter_, comptime option, value: *const option.InType()) Result {
//     const iter = iter_ orelse return .invalid_value;
//     switch (option) {
//         .layer => iter.layer_filter = value.*,
//     }
//     return .success;
// }
/// # Safety
/// `iter_`, if non-null, is a live wrapper; `value` points to a value of the
/// type required by `option`.
unsafe fn placement_iterator_set_typed(
    iter_: PlacementIterator,
    option: PlacementIteratorOption,
    value: *const c_void,
) -> Result {
    // const iter = iter_ orelse return .invalid_value;
    if iter_.is_null() {
        return Result::InvalidValue;
    }
    match option {
        // .layer => iter.layer_filter = value.*,
        PlacementIteratorOption::Layer => unsafe {
            (*iter_).layer_filter = *(value as *const PlacementLayer);
        },
    }
    // return .success;
    Result::Success
}

// pub fn placement_iterator_next(iter_: PlacementIterator) callconv(...) bool {
//     if (comptime !build_options.kitty_graphics) return false;
//     const iter = iter_ orelse return false;
//     while (iter.inner.next()) |entry| {
//         if (iter.layer_filter.matches(entry.value_ptr.z)) {
//             iter.entry = entry;
//             return true;
//         }
//     }
//     return false;
// }
/// # Safety
/// `iter_`, if non-null, must be a live iterator wrapper whose `inner` was
/// seeded by `get(.., .placement_iterator, ..)`.
pub unsafe fn placement_iterator_next(iter_: PlacementIterator) -> bool {
    // if (comptime !build_options.kitty_graphics) return false;
    if !KITTY_GRAPHICS {
        return false;
    }
    // const iter = iter_ orelse return false;
    if iter_.is_null() {
        return false;
    }
    // SAFETY: `iter_` is a live wrapper (caller contract).
    let iter = unsafe { &mut *iter_ };
    let Some(inner) = iter.inner.as_mut() else {
        return false;
    };
    // while (iter.inner.next()) |entry| { ... }
    for (key, val) in inner {
        // if (iter.layer_filter.matches(entry.value_ptr.z)) {
        if iter.layer_filter.matches(val.z) {
            // iter.entry = entry;
            iter.entry = Some((key as *const PlacementKey, val as *const Placement));
            // return true;
            return true;
        }
    }
    // return false;
    false
}

// pub fn placement_get(iter_, data, out) callconv(...) Result {
//     if (comptime !build_options.kitty_graphics) return .no_value;
//     return switch (data) {
//         .invalid => .invalid_value,
//         inline else => |comptime_data| placementGetTyped(iter_, comptime_data, @ptrCast(@alignCast(out))),
//     };
// }
/// # Safety
/// `iter_`, if non-null, is a live wrapper positioned on an entry; `out` must
/// point to writable storage of the type required by `data`.
pub unsafe fn placement_get(
    iter_: PlacementIterator,
    data: PlacementData,
    out: *mut c_void,
) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    match data {
        PlacementData::Invalid => Result::InvalidValue,
        // inline else => placementGetTyped(iter_, comptime_data, @ptrCast(@alignCast(out)))
        _ => unsafe { placement_get_typed(iter_, data, out) },
    }
}

// pub fn placement_get_multi(iter_, count, keys, values, out_written) callconv(...) Result {
//     if (comptime !build_options.kitty_graphics) return .no_value;
//     const k = keys orelse return .invalid_value;
//     const v = values orelse return .invalid_value;
//     for (0..count) |i| {
//         const result = placement_get(iter_, k[i], v[i]);
//         if (result != .success) { if (out_written) |w| w.* = i; return result; }
//     }
//     if (out_written) |w| w.* = count;
//     return .success;
// }
/// # Safety
/// `iter_`, if non-null, is a live wrapper positioned on an entry. `keys`/
/// `values`, if non-null, point to `count` readable entries each; `values[i]`
/// must point to writable storage of the type required by `keys[i]`.
/// `out_written`, if non-null, must be writable.
pub unsafe fn placement_get_multi(
    iter_: PlacementIterator,
    count: usize,
    keys: *const PlacementData,
    values: *const *mut c_void,
    out_written: *mut usize,
) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    // const k = keys orelse return .invalid_value;
    if keys.is_null() {
        return Result::InvalidValue;
    }
    // const v = values orelse return .invalid_value;
    if values.is_null() {
        return Result::InvalidValue;
    }

    // for (0..count) |i| { ... }
    for i in 0..count {
        // const result = placement_get(iter_, k[i], v[i]);
        // SAFETY: `keys`/`values` point to `count` readable entries (caller).
        let key = unsafe { *keys.add(i) };
        let val = unsafe { *values.add(i) };
        let result = unsafe { placement_get(iter_, key, val) };
        // if (result != .success) { if (out_written) |w| w.* = i; return result; }
        if result != Result::Success {
            if !out_written.is_null() {
                unsafe { *out_written = i };
            }
            return result;
        }
    }
    // if (out_written) |w| w.* = count;
    if !out_written.is_null() {
        unsafe { *out_written = count };
    }
    // return .success;
    Result::Success
}

// fn placementGetTyped(iter_, comptime data, out: *data.OutType()) Result {
//     const iter = iter_ orelse return .invalid_value;
//     const entry = iter.entry orelse return .invalid_value;
//     const key = entry.key_ptr;
//     const val = entry.value_ptr;
//     switch (data) { ... }
//     return .success;
// }
/// # Safety
/// `iter_`, if non-null, is a live wrapper; `out` points to storage of the type
/// required by `data`.
unsafe fn placement_get_typed(
    iter_: PlacementIterator,
    data: PlacementData,
    out: *mut c_void,
) -> Result {
    // const iter = iter_ orelse return .invalid_value;
    if iter_.is_null() {
        return Result::InvalidValue;
    }
    // const entry = iter.entry orelse return .invalid_value;
    // SAFETY: `iter_` is a live wrapper (caller contract).
    let entry = match unsafe { (*iter_).entry } {
        Some(e) => e,
        None => return Result::InvalidValue,
    };

    // const key = entry.key_ptr;
    // const val = entry.value_ptr;
    // SAFETY: the entry pointers are borrowed from the storage `placements` map,
    // which outlives the iterator (see `PlacementIteratorWrapper` PORT NOTE).
    let key = unsafe { &*entry.0 };
    let val = unsafe { &*entry.1 };

    match data {
        // .invalid => return .invalid_value,
        PlacementData::Invalid => return Result::InvalidValue,
        // .image_id => out.* = key.image_id,
        PlacementData::ImageId => unsafe { *(out as *mut u32) = key.image_id },
        // .placement_id => out.* = key.placement_id.id,
        PlacementData::PlacementId => unsafe { *(out as *mut u32) = key.placement_id.id },
        // .is_virtual => out.* = val.location == .virtual,
        PlacementData::IsVirtual => unsafe {
            *(out as *mut bool) = matches!(val.location, Location::Virtual)
        },
        // .x_offset => out.* = val.x_offset,
        PlacementData::XOffset => unsafe { *(out as *mut u32) = val.x_offset },
        // .y_offset => out.* = val.y_offset,
        PlacementData::YOffset => unsafe { *(out as *mut u32) = val.y_offset },
        // .source_x => out.* = val.source_x,
        PlacementData::SourceX => unsafe { *(out as *mut u32) = val.source_x },
        // .source_y => out.* = val.source_y,
        PlacementData::SourceY => unsafe { *(out as *mut u32) = val.source_y },
        // .source_width => out.* = val.source_width,
        PlacementData::SourceWidth => unsafe { *(out as *mut u32) = val.source_width },
        // .source_height => out.* = val.source_height,
        PlacementData::SourceHeight => unsafe { *(out as *mut u32) = val.source_height },
        // .columns => out.* = val.columns,
        PlacementData::Columns => unsafe { *(out as *mut u32) = val.columns },
        // .rows => out.* = val.rows,
        PlacementData::Rows => unsafe { *(out as *mut u32) = val.rows },
        // .z => out.* = val.z,
        PlacementData::Z => unsafe { *(out as *mut i32) = val.z },
    }

    // return .success;
    Result::Success
}

// pub fn placement_rect(iter_, image_, terminal_, out: *selection_c.CSelection) callconv(...) Result {
//     if (comptime !build_options.kitty_graphics) return .no_value;
//     const wrapper = terminal_ orelse return .invalid_value;
//     const image = image_ orelse return .invalid_value;
//     const iter = iter_ orelse return .invalid_value;
//     const entry = iter.entry orelse return .invalid_value;
//     const r = entry.value_ptr.rect(image.*, wrapper.terminal) orelse return .no_value;
//     out.* = .{ .start = grid_ref.CGridRef.fromPin(r.top_left), .end = grid_ref.CGridRef.fromPin(r.bottom_right), .rectangle = true };
//     return .success;
// }
/// # Safety
/// `iter_`/`image_`/`terminal_`, if non-null, must be live; `out` must be a
/// valid, writable pointer to a `CSelection`.
pub unsafe fn placement_rect(
    iter_: PlacementIterator,
    image_: ImageHandle,
    terminal_: CTerminal,
    out: *mut CSelection,
) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // const image = image_ orelse return .invalid_value;
    if image_.is_null() {
        return Result::InvalidValue;
    }
    // const iter = iter_ orelse return .invalid_value;
    if iter_.is_null() {
        return Result::InvalidValue;
    }
    // const entry = iter.entry orelse return .invalid_value;
    // SAFETY: `iter_` is a live wrapper (caller contract).
    let entry = match unsafe { (*iter_).entry } {
        Some(e) => e,
        None => return Result::InvalidValue,
    };

    // const r = entry.value_ptr.rect(image.*, wrapper.terminal) orelse return .no_value;
    // SAFETY: `image_` points to a live `Image`; `wrapper.terminal` is the live
    // owning terminal read through the raw `*mut TerminalWrapper`.
    let image = unsafe { &*image_ };
    let t: &ZigTerminal = unsafe { &*(*terminal_).terminal };
    let val = unsafe { &*entry.1 };
    let r = match val.rect(image, t) {
        Some(r) => r,
        None => return Result::NoValue,
    };

    // out.* = .{ .start = ..fromPin(r.top_left), .end = ..fromPin(r.bottom_right), .rectangle = true };
    unsafe {
        *out = CSelection {
            start: CGridRef::from_pin(r.top_left),
            end: CGridRef::from_pin(r.bottom_right),
            rectangle: true,
            ..CSelection::default()
        };
    }

    // return .success;
    Result::Success
}

// pub fn placement_pixel_size(iter_, image_, terminal_, out_width: *u32, out_height: *u32) callconv(...) Result {
//     ...
//     const s = entry.value_ptr.pixelSize(image.*, wrapper.terminal);
//     out_width.* = s.width; out_height.* = s.height;
//     return .success;
// }
/// # Safety
/// `iter_`/`image_`/`terminal_`, if non-null, must be live; `out_width`/
/// `out_height` must be valid, writable pointers to `u32`.
pub unsafe fn placement_pixel_size(
    iter_: PlacementIterator,
    image_: ImageHandle,
    terminal_: CTerminal,
    out_width: *mut u32,
    out_height: *mut u32,
) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // const image = image_ orelse return .invalid_value;
    if image_.is_null() {
        return Result::InvalidValue;
    }
    // const iter = iter_ orelse return .invalid_value;
    if iter_.is_null() {
        return Result::InvalidValue;
    }
    // const entry = iter.entry orelse return .invalid_value;
    let entry = match unsafe { (*iter_).entry } {
        Some(e) => e,
        None => return Result::InvalidValue,
    };
    // const s = entry.value_ptr.pixelSize(image.*, wrapper.terminal);
    let image = unsafe { &*image_ };
    let t: &ZigTerminal = unsafe { &*(*terminal_).terminal };
    let val = unsafe { &*entry.1 };
    let s = val.pixel_size(image, t);

    // out_width.* = s.width; out_height.* = s.height;
    unsafe {
        *out_width = s.width;
        *out_height = s.height;
    }

    // return .success;
    Result::Success
}

// pub fn placement_grid_size(iter_, image_, terminal_, out_cols: *u32, out_rows: *u32) callconv(...) Result {
//     ...
//     const s = entry.value_ptr.gridSize(image.*, wrapper.terminal);
//     out_cols.* = s.cols; out_rows.* = s.rows;
//     return .success;
// }
/// # Safety
/// `iter_`/`image_`/`terminal_`, if non-null, must be live; `out_cols`/
/// `out_rows` must be valid, writable pointers to `u32`.
pub unsafe fn placement_grid_size(
    iter_: PlacementIterator,
    image_: ImageHandle,
    terminal_: CTerminal,
    out_cols: *mut u32,
    out_rows: *mut u32,
) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // const image = image_ orelse return .invalid_value;
    if image_.is_null() {
        return Result::InvalidValue;
    }
    // const iter = iter_ orelse return .invalid_value;
    if iter_.is_null() {
        return Result::InvalidValue;
    }
    // const entry = iter.entry orelse return .invalid_value;
    let entry = match unsafe { (*iter_).entry } {
        Some(e) => e,
        None => return Result::InvalidValue,
    };
    // const s = entry.value_ptr.gridSize(image.*, wrapper.terminal);
    let image = unsafe { &*image_ };
    let t: &ZigTerminal = unsafe { &*(*terminal_).terminal };
    let val = unsafe { &*entry.1 };
    let s = val.grid_size(image, t);

    // out_cols.* = s.cols; out_rows.* = s.rows;
    unsafe {
        *out_cols = s.cols;
        *out_rows = s.rows;
    }

    // return .success;
    Result::Success
}

// pub fn placement_viewport_pos(iter_, image_, terminal_, out_col: *i32, out_row: *i32) callconv(...) Result {
//     ...
//     const vp = computeViewportPos(entry.value_ptr, image, wrapper.terminal);
//     if (!vp.visible) return .no_value;
//     out_col.* = vp.col; out_row.* = vp.row;
//     return .success;
// }
/// # Safety
/// `iter_`/`image_`/`terminal_`, if non-null, must be live; `out_col`/`out_row`
/// must be valid, writable pointers to `i32`.
pub unsafe fn placement_viewport_pos(
    iter_: PlacementIterator,
    image_: ImageHandle,
    terminal_: CTerminal,
    out_col: *mut i32,
    out_row: *mut i32,
) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // const image = image_ orelse return .invalid_value;
    if image_.is_null() {
        return Result::InvalidValue;
    }
    // const iter = iter_ orelse return .invalid_value;
    if iter_.is_null() {
        return Result::InvalidValue;
    }
    // const entry = iter.entry orelse return .invalid_value;
    let entry = match unsafe { (*iter_).entry } {
        Some(e) => e,
        None => return Result::InvalidValue,
    };

    // const vp = computeViewportPos(entry.value_ptr, image, wrapper.terminal);
    let image = unsafe { &*image_ };
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let vp = unsafe { compute_viewport_pos(entry.1, image, t) };
    // if (!vp.visible) return .no_value;
    if !vp.visible {
        return Result::NoValue;
    }

    // out_col.* = vp.col; out_row.* = vp.row;
    unsafe {
        *out_col = vp.col;
        *out_row = vp.row;
    }

    // return .success;
    Result::Success
}

// pub fn placement_source_rect(iter_, image_, out_x, out_y, out_width, out_height) callconv(...) Result {
//     if (comptime !build_options.kitty_graphics) return .no_value;
//     const image = image_ orelse return .invalid_value;
//     const iter = iter_ orelse return .invalid_value;
//     const entry = iter.entry orelse return .invalid_value;
//     const p = entry.value_ptr;
//     const x = @min(p.source_x, image.width);
//     const y = @min(p.source_y, image.height);
//     const w = @min(if (p.source_width > 0) p.source_width else image.width, image.width - x);
//     const h = @min(if (p.source_height > 0) p.source_height else image.height, image.height - y);
//     out_x.* = x; out_y.* = y; out_width.* = w; out_height.* = h;
//     return .success;
// }
/// # Safety
/// `iter_`/`image_`, if non-null, must be live; the four `out_*` pointers must be
/// valid, writable pointers to `u32`.
pub unsafe fn placement_source_rect(
    iter_: PlacementIterator,
    image_: ImageHandle,
    out_x: *mut u32,
    out_y: *mut u32,
    out_width: *mut u32,
    out_height: *mut u32,
) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    // const image = image_ orelse return .invalid_value;
    if image_.is_null() {
        return Result::InvalidValue;
    }
    // const iter = iter_ orelse return .invalid_value;
    if iter_.is_null() {
        return Result::InvalidValue;
    }
    // const entry = iter.entry orelse return .invalid_value;
    let entry = match unsafe { (*iter_).entry } {
        Some(e) => e,
        None => return Result::InvalidValue,
    };
    // const p = entry.value_ptr;
    let image = unsafe { &*image_ };
    let p = unsafe { &*entry.1 };

    // Apply "0 = full image dimension" convention, then clamp to image bounds.
    // const x = @min(p.source_x, image.width);
    let x = p.source_x.min(image.width);
    // const y = @min(p.source_y, image.height);
    let y = p.source_y.min(image.height);
    // const w = @min(if (p.source_width > 0) p.source_width else image.width, image.width - x);
    let w = (if p.source_width > 0 {
        p.source_width
    } else {
        image.width
    })
    .min(image.width - x);
    // const h = @min(if (p.source_height > 0) p.source_height else image.height, image.height - y);
    let h = (if p.source_height > 0 {
        p.source_height
    } else {
        image.height
    })
    .min(image.height - y);

    // out_x.* = x; out_y.* = y; out_width.* = w; out_height.* = h;
    unsafe {
        *out_x = x;
        *out_y = y;
        *out_width = w;
        *out_height = h;
    }

    // return .success;
    Result::Success
}

/// C: GhosttyKittyGraphicsPlacementRenderInfo -> RusttyKittyGraphicsPlacementRenderInfo (R2)
// pub const PlacementRenderInfo = extern struct {
//     size: usize = @sizeOf(PlacementRenderInfo),
//     pixel_width: u32 = 0, ... source_height: u32 = 0,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PlacementRenderInfo {
    pub size: usize,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub grid_cols: u32,
    pub grid_rows: u32,
    pub viewport_col: i32,
    pub viewport_row: i32,
    pub viewport_visible: bool,
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
}

impl Default for PlacementRenderInfo {
    // PORT NOTE: the reference declares per-field defaults (`size =
    // @sizeOf(PlacementRenderInfo)`, the rest `0`/`false`); `Default`
    // materializes them verbatim so `PlacementRenderInfo = .{}` (the test's
    // `var ri: PlacementRenderInfo = .{}`) round-trips.
    fn default() -> Self {
        PlacementRenderInfo {
            size: core::mem::size_of::<PlacementRenderInfo>(),
            pixel_width: 0,
            pixel_height: 0,
            grid_cols: 0,
            grid_rows: 0,
            viewport_col: 0,
            viewport_row: 0,
            viewport_visible: false,
            source_x: 0,
            source_y: 0,
            source_width: 0,
            source_height: 0,
        }
    }
}

// pub fn placement_render_info(iter_, image_, terminal_, out_: ?*PlacementRenderInfo) callconv(...) Result {
//     ...
// }
/// # Safety
/// `iter_`/`image_`/`terminal_`, if non-null, must be live; `out_`, if non-null,
/// must be a valid, writable pointer to a `PlacementRenderInfo`.
pub unsafe fn placement_render_info(
    iter_: PlacementIterator,
    image_: ImageHandle,
    terminal_: CTerminal,
    out_: *mut PlacementRenderInfo,
) -> Result {
    // if (comptime !build_options.kitty_graphics) return .no_value;
    if !KITTY_GRAPHICS {
        return Result::NoValue;
    }

    // const wrapper = terminal_ orelse return .invalid_value;
    if terminal_.is_null() {
        return Result::InvalidValue;
    }
    // const image = image_ orelse return .invalid_value;
    if image_.is_null() {
        return Result::InvalidValue;
    }
    // const iter = iter_ orelse return .invalid_value;
    if iter_.is_null() {
        return Result::InvalidValue;
    }
    // const entry = iter.entry orelse return .invalid_value;
    let entry = match unsafe { (*iter_).entry } {
        Some(e) => e,
        None => return Result::InvalidValue,
    };
    // const out = out_ orelse return .invalid_value;
    if out_.is_null() {
        return Result::InvalidValue;
    }
    // if (out.size < @sizeOf(PlacementRenderInfo)) return .invalid_value;
    if unsafe { (*out_).size } < core::mem::size_of::<PlacementRenderInfo>() {
        return Result::InvalidValue;
    }

    // const p = entry.value_ptr;
    let image = unsafe { &*image_ };
    let t: *mut ZigTerminal = unsafe { (*terminal_).terminal };
    let t_ref: &ZigTerminal = unsafe { &*t };
    let p = unsafe { &*entry.1 };

    // const ps = p.pixelSize(image.*, wrapper.terminal);
    let ps = p.pixel_size(image, t_ref);
    // out.pixel_width = ps.width; out.pixel_height = ps.height;
    unsafe {
        (*out_).pixel_width = ps.width;
        (*out_).pixel_height = ps.height;
    }

    // const gs = p.gridSize(image.*, wrapper.terminal);
    let gs = p.grid_size(image, t_ref);
    // out.grid_cols = gs.cols; out.grid_rows = gs.rows;
    unsafe {
        (*out_).grid_cols = gs.cols;
        (*out_).grid_rows = gs.rows;
    }

    // const vp = computeViewportPos(p, image, wrapper.terminal);
    let vp = unsafe { compute_viewport_pos(p, image, t) };
    // out.viewport_col = vp.col; out.viewport_row = vp.row; out.viewport_visible = vp.visible;
    unsafe {
        (*out_).viewport_col = vp.col;
        (*out_).viewport_row = vp.row;
        (*out_).viewport_visible = vp.visible;
    }

    // const x = @min(p.source_x, image.width);
    let x = p.source_x.min(image.width);
    // const y = @min(p.source_y, image.height);
    let y = p.source_y.min(image.height);
    // out.source_x = x; out.source_y = y;
    // out.source_width = @min(if (p.source_width > 0) p.source_width else image.width, image.width - x);
    // out.source_height = @min(if (p.source_height > 0) p.source_height else image.height, image.height - y);
    unsafe {
        (*out_).source_x = x;
        (*out_).source_y = y;
        (*out_).source_width = (if p.source_width > 0 {
            p.source_width
        } else {
            image.width
        })
        .min(image.width - x);
        (*out_).source_height = (if p.source_height > 0 {
            p.source_height
        } else {
            image.height
        })
        .min(image.height - y);
    }

    // return .success;
    Result::Success
}

/// Compute viewport-relative position of a placement.
///
/// Converts the placement's internal pin to viewport-relative column
/// and row coordinates by getting screen-absolute coordinates for
/// both the pin and the viewport origin, then subtracting to get
/// viewport-relative coordinates. The row value can be negative when
/// the placement's origin has scrolled above the top of the viewport.
///
/// A placement is considered not visible if it is a virtual (unicode
/// placeholder) placement, or if it is fully off-screen (its bottom
/// edge is above the viewport or its top edge is at or below the
/// viewport's last row).
// fn computeViewportPos(p: *const ...Placement, image: *const Image, t: *Terminal) struct { col: i32, row: i32, visible: bool } { ... }
//
/// # Safety
/// `p`/`image` must point to live values; `t` must be the live owning terminal.
unsafe fn compute_viewport_pos(
    p: *const Placement,
    image: &Image,
    t: *mut ZigTerminal,
) -> ViewportPos {
    // SAFETY: `p` is a live placement borrowed from the storage map (caller).
    let p = unsafe { &*p };

    // Virtual placements use unicode placeholders and don't have a
    // screen position — they are rendered inline by the text layout.
    // const pin = switch (p.location) { .pin => |pin| pin, .virtual => return ... };
    let pin = match p.location {
        Location::Pin(pin) => pin,
        Location::Virtual => {
            return ViewportPos {
                col: 0,
                row: 0,
                visible: false,
            }
        }
    };

    // Convert both the placement's pin and the viewport's top-left
    // corner to screen-absolute coordinates so we can subtract them
    // to get viewport-relative coordinates.
    // const pages = &t.screens.active.pages;
    // SAFETY: `t` is the live terminal; `screens.active` is its live active screen.
    let pages = unsafe { &(*(*t).screens.active).pages };
    // const pin_screen = pages.pointFromPin(.screen, pin.*) orelse return ...;
    let pin_screen = match pages.point_from_pin(Tag::Screen, unsafe { *pin }) {
        Some(p) => p,
        None => {
            return ViewportPos {
                col: 0,
                row: 0,
                visible: false,
            }
        }
    };
    // const vp_tl = pages.getTopLeft(.viewport);
    let vp_tl = pages.get_top_left(Tag::Viewport);
    // const vp_screen = pages.pointFromPin(.screen, vp_tl) orelse return ...;
    let vp_screen = match pages.point_from_pin(Tag::Screen, vp_tl) {
        Some(p) => p,
        None => {
            return ViewportPos {
                col: 0,
                row: 0,
                visible: false,
            }
        }
    };

    // Subtracting viewport origin from the pin gives us viewport-
    // relative coordinates. The row can be negative when the
    // placement has partially scrolled above the viewport.
    // const vp_row: i32 = @as(i32, @intCast(pin_screen.screen.y)) - @as(i32, @intCast(vp_screen.screen.y));
    let vp_row: i32 = pin_screen.coord().y as i32 - vp_screen.coord().y as i32;
    // const vp_col: i32 = @intCast(pin_screen.screen.x);
    let vp_col: i32 = pin_screen.coord().x as i32;

    // A placement is invisible if its bottom edge (row + height)
    // is above the viewport, or its top edge is at or below the
    // viewport's last row.
    // const grid_size = p.gridSize(image.*, t);
    let grid_size = p.grid_size(image, unsafe { &*t });
    // const rows_i32: i32 = @intCast(grid_size.rows);
    let rows_i32: i32 = grid_size.rows as i32;
    // const term_rows: i32 = @intCast(t.rows);
    let term_rows: i32 = unsafe { (*t).rows } as i32;
    // const visible = vp_row + rows_i32 > 0 and vp_row < term_rows;
    let visible = vp_row + rows_i32 > 0 && vp_row < term_rows;

    // return .{ .col = vp_col, .row = vp_row, .visible = visible };
    ViewportPos {
        col: vp_col,
        row: vp_row,
        visible,
    }
}

// PORT NOTE: the reference `computeViewportPos` returns an anonymous struct
// `struct { col: i32, row: i32, visible: bool }`. Rust requires a named type;
// `ViewportPos` is that named analog (module-private, never crosses the C ABI).
struct ViewportPos {
    col: i32,
    row: i32,
    visible: bool,
}

// PORT NOTE: `c_int` is referenced by the `#[repr(C)]` enum discriminants above
// only implicitly; keep the import used to mirror the reference's `enum(c_int)`
// intent without a dead-code warning.
const _: c_int = 0;

#[cfg(test)]
// PORT NOTE: the `placement_iterator_set layer filter` test asserts `z >=
// std.math.minInt(i32) / 2 and z < 0` verbatim; clippy `manual_range_contains`
// would rewrite it to `(..).contains(&z)`, deviating from the reference. R1
// keeps the literal form, so the lint is allowed on the test module.
#[allow(clippy::manual_range_contains)]
mod tests {
    use super::*;
    use crate::terminal as terminal_c;
    use crate::test_util::with_test_allocator;

    // PORT NOTE: the reference passes `&lib.alloc.test_allocator` (Zig's
    // leak-detecting testing allocator). rustty has no such global; the parity
    // analog is `crate::test_util::with_test_allocator`, a C `Allocator` backed
    // by the process global allocator (leaks are caught by Miri). The shared
    // helper builds that C allocator and runs `f` with it.

    // PORT NOTE: `terminal_c.vt_write(t, cmd.ptr, cmd.len)` — a thin helper
    // mirroring the reference call so the tests read 1:1.
    fn vt_write(t: terminal_c::Terminal, cmd: &[u8]) {
        unsafe { terminal_c::vt_write(t, cmd.as_ptr(), cmd.len()) };
    }

    // test "placement_iterator new/free"
    #[test]
    fn placement_iterator_new_free() {
        with_test_allocator(|ta| {
            // var iter: PlacementIterator = null;
            let mut iter: PlacementIterator = core::ptr::null_mut();
            // try testing.expectEqual(Result.success, placement_iterator_new(&lib.alloc.test_allocator, &iter));
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            // try testing.expect(iter != null);
            assert!(!iter.is_null());
            // placement_iterator_free(iter);
            unsafe { placement_iterator_free(iter) };
        });
    }

    // test "placement_iterator free null"
    #[test]
    fn placement_iterator_free_null() {
        // placement_iterator_free(null);
        unsafe { placement_iterator_free(core::ptr::null_mut()) };
    }

    // PORT NOTE: every test below short-circuits in the reference with
    // `if (comptime !build_options.kitty_graphics) return error.SkipZigTest;`.
    // The rustty analog is a runtime guard on `KITTY_GRAPHICS`; on the supported
    // targets it is always `true`, so the body runs.

    // test "placement_iterator next on empty storage"
    #[test]
    fn placement_iterator_next_on_empty_storage() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            // var t: terminal_c.Terminal = null;
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            // try testing.expectEqual(Result.success, terminal_c.new(&lib.alloc.test_allocator, &t, 80, 24));
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });
            // defer terminal_c.free(t);

            // var graphics: KittyGraphics = undefined;
            let mut graphics: KittyGraphics = core::ptr::null_mut();
            // try testing.expectEqual(Result.success, terminal_c.get(t, .kitty_graphics, @ptrCast(&graphics)));
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            // var iter: PlacementIterator = null;
            let mut iter: PlacementIterator = core::ptr::null_mut();
            // try testing.expectEqual(Result.success, placement_iterator_new(&lib.alloc.test_allocator, &iter));
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            // defer placement_iterator_free(iter);

            // try testing.expectEqual(Result.success, get(graphics, .placement_iterator, @ptrCast(&iter)));
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            // try testing.expect(!placement_iterator_next(iter));
            assert!(!unsafe { placement_iterator_next(iter) });

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_iterator get before next returns invalid"
    #[test]
    fn placement_iterator_get_before_next_returns_invalid() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });

            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });

            // var image_id: u32 = undefined;
            let mut image_id: u32 = 0;
            // try testing.expectEqual(Result.invalid_value, placement_get(iter, .image_id, @ptrCast(&image_id)));
            assert_eq!(Result::InvalidValue, unsafe {
                placement_get(
                    iter,
                    PlacementData::ImageId,
                    &mut image_id as *mut u32 as *mut c_void,
                )
            });

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_iterator with transmit and display"
    #[test]
    fn placement_iterator_with_transmit_and_display() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            // Transmit and display a 1x2 RGB image (image_id=1, placement_id=1).
            // a=T (transmit+display), t=d (direct), f=24 (RGB), i=1, p=1
            // s=1,v=2 (1x2 pixels), c=10,r=1 (10 cols, 1 row)
            // //////// = 8 base64 chars = 6 bytes = 1*2*3 RGB bytes
            let cmd = b"\x1b_Ga=T,t=d,f=24,i=1,p=1,s=1,v=2,c=10,r=1;////////\x1b\\";
            vt_write(t, cmd);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });

            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });

            // Should have exactly one placement.
            assert!(unsafe { placement_iterator_next(iter) });

            // var image_id: u32 = undefined;
            let mut image_id: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_get(
                    iter,
                    PlacementData::ImageId,
                    &mut image_id as *mut u32 as *mut c_void,
                )
            });
            assert_eq!(1, image_id);

            // var placement_id: u32 = undefined;
            let mut placement_id: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_get(
                    iter,
                    PlacementData::PlacementId,
                    &mut placement_id as *mut u32 as *mut c_void,
                )
            });
            assert_eq!(1, placement_id);

            // var is_virtual: bool = undefined;
            let mut is_virtual: bool = false;
            assert_eq!(Result::Success, unsafe {
                placement_get(
                    iter,
                    PlacementData::IsVirtual,
                    &mut is_virtual as *mut bool as *mut c_void,
                )
            });
            assert!(!is_virtual);

            // No more placements.
            assert!(!unsafe { placement_iterator_next(iter) });

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_iterator with multiple placements"
    #[test]
    fn placement_iterator_with_multiple_placements() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            // Transmit image 1 then display it twice with different placement IDs.
            let transmit = b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\";
            let display1 = b"\x1b_Ga=p,i=1,p=1,c=10,r=1;\x1b\\";
            let display2 = b"\x1b_Ga=p,i=1,p=2,c=5,r=1;\x1b\\";
            vt_write(t, transmit);
            vt_write(t, display1);
            vt_write(t, display2);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });

            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });

            // Count placements and collect image IDs.
            // var count: usize = 0;
            let mut count: usize = 0;
            // var seen_p1 = false;
            let mut seen_p1 = false;
            // var seen_p2 = false;
            let mut seen_p2 = false;
            // while (placement_iterator_next(iter)) { ... }
            while unsafe { placement_iterator_next(iter) } {
                count += 1;

                let mut image_id: u32 = 0;
                assert_eq!(Result::Success, unsafe {
                    placement_get(
                        iter,
                        PlacementData::ImageId,
                        &mut image_id as *mut u32 as *mut c_void,
                    )
                });
                assert_eq!(1, image_id);

                let mut placement_id: u32 = 0;
                assert_eq!(Result::Success, unsafe {
                    placement_get(
                        iter,
                        PlacementData::PlacementId,
                        &mut placement_id as *mut u32 as *mut c_void,
                    )
                });
                // if (placement_id == 1) seen_p1 = true;
                if placement_id == 1 {
                    seen_p1 = true;
                }
                // if (placement_id == 2) seen_p2 = true;
                if placement_id == 2 {
                    seen_p2 = true;
                }
            }

            assert_eq!(2, count);
            assert!(seen_p1);
            assert!(seen_p2);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_iterator_set layer filter"
    #[test]
    fn placement_iterator_set_layer_filter() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            // Transmit image 1.
            let transmit = b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\";
            vt_write(t, transmit);

            // Display with z=5 (above text), z=-1 (below text), z=-1073741825 (below bg).
            // INT32_MIN/2 = -1073741824, so -1073741825 < INT32_MIN/2.
            let d1 = b"\x1b_Ga=p,i=1,p=1,z=5;\x1b\\";
            let d2 = b"\x1b_Ga=p,i=1,p=2,z=-1;\x1b\\";
            let d3 = b"\x1b_Ga=p,i=1,p=3,z=-1073741825;\x1b\\";
            vt_write(t, d1);
            vt_write(t, d2);
            vt_write(t, d3);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });

            // Filter: above_text (z >= 0) — should yield only p=1.
            // var layer = PlacementLayer.above_text;
            let mut layer = PlacementLayer::AboveText;
            assert_eq!(Result::Success, unsafe {
                placement_iterator_set(
                    iter,
                    PlacementIteratorOption::Layer,
                    &layer as *const PlacementLayer as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });

            // var count: u32 = 0;
            let mut count: u32 = 0;
            while unsafe { placement_iterator_next(iter) } {
                let mut z: i32 = 0;
                assert_eq!(Result::Success, unsafe {
                    placement_get(iter, PlacementData::Z, &mut z as *mut i32 as *mut c_void)
                });
                assert!(z >= 0);
                count += 1;
            }
            assert_eq!(1, count);

            // Filter: below_text (INT32_MIN/2 <= z < 0) — should yield only p=2.
            layer = PlacementLayer::BelowText;
            assert_eq!(Result::Success, unsafe {
                placement_iterator_set(
                    iter,
                    PlacementIteratorOption::Layer,
                    &layer as *const PlacementLayer as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });

            count = 0;
            while unsafe { placement_iterator_next(iter) } {
                let mut z: i32 = 0;
                assert_eq!(Result::Success, unsafe {
                    placement_get(iter, PlacementData::Z, &mut z as *mut i32 as *mut c_void)
                });
                assert!(z >= i32::MIN / 2 && z < 0);
                count += 1;
            }
            assert_eq!(1, count);

            // Filter: below_bg (z < INT32_MIN/2) — should yield only p=3.
            layer = PlacementLayer::BelowBg;
            assert_eq!(Result::Success, unsafe {
                placement_iterator_set(
                    iter,
                    PlacementIteratorOption::Layer,
                    &layer as *const PlacementLayer as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });

            count = 0;
            while unsafe { placement_iterator_next(iter) } {
                let mut z: i32 = 0;
                assert_eq!(Result::Success, unsafe {
                    placement_get(iter, PlacementData::Z, &mut z as *mut i32 as *mut c_void)
                });
                assert!(z < i32::MIN / 2);
                count += 1;
            }
            assert_eq!(1, count);

            // Filter: all — should yield all 3.
            layer = PlacementLayer::All;
            assert_eq!(Result::Success, unsafe {
                placement_iterator_set(
                    iter,
                    PlacementIteratorOption::Layer,
                    &layer as *const PlacementLayer as *const c_void,
                )
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });

            count = 0;
            while unsafe { placement_iterator_next(iter) } {
                count += 1;
            }
            assert_eq!(3, count);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "image_get_handle returns null for missing id"
    #[test]
    fn image_get_handle_returns_null_for_missing_id() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            // try testing.expectEqual(@as(ImageHandle, null), image_get_handle(graphics, 999));
            assert!(unsafe { image_get_handle(graphics, 999) }.is_null());

            unsafe { terminal_c::free(t) };
        });
    }

    // test "image_get_handle and image_get with transmitted image"
    #[test]
    fn image_get_handle_and_image_get_with_transmitted_image() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            // Transmit a 1x2 RGB image with image_id=1.
            let cmd = b"\x1b_Ga=T,t=d,f=24,i=1,p=1,s=1,v=2;////////\x1b\\";
            vt_write(t, cmd);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            // const img = image_get_handle(graphics, 1);
            let img = unsafe { image_get_handle(graphics, 1) };
            // try testing.expect(img != null);
            assert!(!img.is_null());

            // var id: u32 = undefined;
            let mut id: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                image_get(img, ImageData::Id, &mut id as *mut u32 as *mut c_void)
            });
            assert_eq!(1, id);

            let mut w: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                image_get(img, ImageData::Width, &mut w as *mut u32 as *mut c_void)
            });
            assert_eq!(1, w);

            let mut h: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                image_get(img, ImageData::Height, &mut h as *mut u32 as *mut c_void)
            });
            assert_eq!(2, h);

            // var fmt: ImageFormat = undefined;
            let mut fmt: ImageFormat = ImageFormat::Rgb;
            assert_eq!(Result::Success, unsafe {
                image_get(
                    img,
                    ImageData::Format,
                    &mut fmt as *mut ImageFormat as *mut c_void,
                )
            });
            // try testing.expectEqual(.rgb, fmt);
            assert_eq!(ImageFormat::Rgb, fmt);

            // var comp: ImageCompression = undefined;
            let mut comp: ImageCompression = ImageCompression::None;
            assert_eq!(Result::Success, unsafe {
                image_get(
                    img,
                    ImageData::Compression,
                    &mut comp as *mut ImageCompression as *mut c_void,
                )
            });
            // try testing.expectEqual(.none, comp);
            assert_eq!(ImageCompression::None, comp);

            // var data_len: usize = undefined;
            let mut data_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                image_get(
                    img,
                    ImageData::DataLen,
                    &mut data_len as *mut usize as *mut c_void,
                )
            });
            // try testing.expect(data_len > 0);
            assert!(data_len > 0);

            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_rect with transmit and display"
    #[test]
    fn placement_rect_with_transmit_and_display() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            // Set cell size so grid calculations are deterministic.
            // 80 cols * 10px = 800px, 24 rows * 20px = 480px.
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 24, 10, 20)
            });

            // Transmit and display a 1x2 RGB image at cursor (0,0).
            // c=10,r=1 => 10 columns, 1 row.
            let cmd = b"\x1b_Ga=T,t=d,f=24,i=1,p=1,s=1,v=2,c=10,r=1;////////\x1b\\";
            vt_write(t, cmd);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });

            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            // var sel: selection_c.CSelection = undefined;
            let mut sel = CSelection::default();
            assert_eq!(Result::Success, unsafe {
                placement_rect(iter, img, t, &mut sel)
            });

            // Placement starts at cursor origin (0,0).
            assert_eq!(0, sel.start.x);
            assert_eq!(0, sel.start.y);

            // 10 columns wide, 1 row tall => bottom-right is (9, 0).
            assert_eq!(9, sel.end.x);
            assert_eq!(0, sel.end.y);

            assert!(sel.rectangle);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_rect null args return invalid_value"
    #[test]
    fn placement_rect_null_args_return_invalid_value() {
        if !KITTY_GRAPHICS {
            return;
        }
        // var sel: selection_c.CSelection = undefined;
        let mut sel = CSelection::default();
        // try testing.expectEqual(Result.invalid_value, placement_rect(null, null, null, &sel));
        assert_eq!(Result::InvalidValue, unsafe {
            placement_rect(
                core::ptr::null_mut(),
                core::ptr::null(),
                core::ptr::null_mut(),
                &mut sel,
            )
        });
    }

    // test "placement_pixel_size with transmit and display"
    #[test]
    fn placement_pixel_size_with_transmit_and_display() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            // 80 cols * 10px = 800px, 24 rows * 20px = 480px.
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 24, 10, 20)
            });

            // Transmit and display a 1x2 RGB image with c=10,r=1.
            // 10 cols * 10px = 100px width, 1 row * 20px = 20px height.
            let cmd = b"\x1b_Ga=T,t=d,f=24,i=1,p=1,s=1,v=2,c=10,r=1;////////\x1b\\";
            vt_write(t, cmd);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });

            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut w: u32 = 0;
            let mut h: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_pixel_size(iter, img, t, &mut w, &mut h)
            });

            assert_eq!(100, w);
            assert_eq!(20, h);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_pixel_size null args return invalid_value"
    #[test]
    fn placement_pixel_size_null_args_return_invalid_value() {
        if !KITTY_GRAPHICS {
            return;
        }
        let mut w: u32 = 0;
        let mut h: u32 = 0;
        assert_eq!(Result::InvalidValue, unsafe {
            placement_pixel_size(
                core::ptr::null_mut(),
                core::ptr::null(),
                core::ptr::null_mut(),
                &mut w,
                &mut h,
            )
        });
    }

    // test "placement_grid_size with transmit and display"
    #[test]
    fn placement_grid_size_with_transmit_and_display() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            // 80 cols * 10px = 800px, 24 rows * 20px = 480px.
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 24, 10, 20)
            });

            // Transmit and display a 1x2 RGB image with c=10,r=1.
            let cmd = b"\x1b_Ga=T,t=d,f=24,i=1,p=1,s=1,v=2,c=10,r=1;////////\x1b\\";
            vt_write(t, cmd);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });

            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut cols: u32 = 0;
            let mut rows: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_grid_size(iter, img, t, &mut cols, &mut rows)
            });

            assert_eq!(10, cols);
            assert_eq!(1, rows);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_grid_size null args return invalid_value"
    #[test]
    fn placement_grid_size_null_args_return_invalid_value() {
        if !KITTY_GRAPHICS {
            return;
        }
        let mut cols: u32 = 0;
        let mut rows: u32 = 0;
        assert_eq!(Result::InvalidValue, unsafe {
            placement_grid_size(
                core::ptr::null_mut(),
                core::ptr::null(),
                core::ptr::null_mut(),
                &mut cols,
                &mut rows,
            )
        });
    }

    // test "placement_viewport_pos with transmit and display"
    #[test]
    fn placement_viewport_pos_with_transmit_and_display() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 24, 10, 20)
            });

            // Transmit and display at cursor (0,0).
            let cmd = b"\x1b_Ga=T,t=d,f=24,i=1,p=1,s=1,v=2,c=10,r=1;////////\x1b\\";
            vt_write(t, cmd);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });

            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut col: i32 = 0;
            let mut row: i32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_viewport_pos(iter, img, t, &mut col, &mut row)
            });

            assert_eq!(0, col);
            assert_eq!(0, row);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_viewport_pos fully off-screen above"
    #[test]
    fn placement_viewport_pos_fully_off_screen_above() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 5)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 5, 10, 20)
            });

            // Transmit image, then display at cursor (0,0) spanning 1 row.
            let transmit = b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\";
            let display = b"\x1b_Ga=p,i=1,p=1,c=1,r=1;\x1b\\";
            vt_write(t, transmit);
            vt_write(t, display);

            // Scroll the image completely off: 10 newlines in a 5-row terminal
            // scrolls by 5+ rows, so a 1-row image at row 0 is fully gone.
            let scroll = b"\n\n\n\n\n\n\n\n\n\n";
            vt_write(t, scroll);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut col: i32 = 0;
            let mut row: i32 = 0;
            assert_eq!(Result::NoValue, unsafe {
                placement_viewport_pos(iter, img, t, &mut col, &mut row)
            });

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_viewport_pos top off-screen"
    #[test]
    fn placement_viewport_pos_top_off_screen() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 5)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 5, 10, 20)
            });

            // Transmit image, display at cursor (0,0) spanning 4 rows.
            // C=1 prevents cursor movement after display.
            let transmit = b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\";
            let display = b"\x1b_Ga=p,i=1,p=1,c=1,r=4,C=1;\x1b\\";
            vt_write(t, transmit);
            vt_write(t, display);

            // Scroll by 2: cursor starts at row 0, 4 newlines to reach bottom,
            // then 2 more to scroll by 2. Image top-left moves to vp_row=-2,
            // but bottom rows -2+4=2 > 0 so it's still partially visible.
            let scroll = b"\n\n\n\n\n\n";
            vt_write(t, scroll);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut col: i32 = 0;
            let mut row: i32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_viewport_pos(iter, img, t, &mut col, &mut row)
            });
            assert_eq!(0, col);
            assert_eq!(-2, row);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_viewport_pos bottom off-screen"
    #[test]
    fn placement_viewport_pos_bottom_off_screen() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 5)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 5, 10, 20)
            });

            // Transmit image, move cursor to row 3 (1-based: row 4), display spanning 4 rows.
            // C=1 prevents cursor movement after display.
            // Image occupies rows 3-6 but viewport only has rows 0-4, so bottom is clipped.
            let transmit = b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\";
            let cursor = b"\x1b[4;1H";
            let display = b"\x1b_Ga=p,i=1,p=1,c=1,r=4,C=1;\x1b\\";
            vt_write(t, transmit);
            vt_write(t, cursor);
            vt_write(t, display);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut col: i32 = 0;
            let mut row: i32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_viewport_pos(iter, img, t, &mut col, &mut row)
            });
            assert_eq!(0, col);
            assert_eq!(3, row);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_viewport_pos top and bottom off-screen"
    #[test]
    fn placement_viewport_pos_top_and_bottom_off_screen() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 5)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 5, 10, 20)
            });

            // Transmit image, display at cursor (0,0) spanning 10 rows.
            // C=1 prevents cursor movement after display.
            // After scrolling by 3, image occupies vp rows -3..6, viewport is 0..4,
            // so both top and bottom are clipped but center is visible.
            let transmit = b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\";
            let display = b"\x1b_Ga=p,i=1,p=1,c=1,r=10,C=1;\x1b\\";
            vt_write(t, transmit);
            vt_write(t, display);

            // Scroll by 3: 4 newlines to reach bottom + 3 more to scroll.
            let scroll = b"\n\n\n\n\n\n\n";
            vt_write(t, scroll);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut col: i32 = 0;
            let mut row: i32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_viewport_pos(iter, img, t, &mut col, &mut row)
            });
            assert_eq!(0, col);
            assert_eq!(-3, row);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_viewport_pos null args return invalid_value"
    #[test]
    fn placement_viewport_pos_null_args_return_invalid_value() {
        if !KITTY_GRAPHICS {
            return;
        }
        let mut col: i32 = 0;
        let mut row: i32 = 0;
        assert_eq!(Result::InvalidValue, unsafe {
            placement_viewport_pos(
                core::ptr::null_mut(),
                core::ptr::null(),
                core::ptr::null_mut(),
                &mut col,
                &mut row,
            )
        });
    }

    // test "placement_source_rect defaults to full image"
    #[test]
    fn placement_source_rect_defaults_to_full_image() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 24, 10, 20)
            });

            // Transmit and display a 1x2 RGB image with no source rect specified.
            // source_width=0 and source_height=0 should resolve to full image (1x2).
            let cmd = b"\x1b_Ga=T,t=d,f=24,i=1,p=1,s=1,v=2;////////\x1b\\";
            vt_write(t, cmd);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut x: u32 = 0;
            let mut y: u32 = 0;
            let mut w: u32 = 0;
            let mut h: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_source_rect(iter, img, &mut x, &mut y, &mut w, &mut h)
            });
            assert_eq!(0, x);
            assert_eq!(0, y);
            assert_eq!(1, w);
            assert_eq!(2, h);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_source_rect with explicit source rect"
    #[test]
    fn placement_source_rect_with_explicit_source_rect() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 24, 10, 20)
            });

            // Transmit a 4x4 RGBA image (64 bytes = 4*4*4).
            // Base64 of 64 zero bytes: 88 chars (21 full groups + AA== padding).
            let transmit = concat!(
                "\x1b_Ga=t,t=d,f=32,i=1,s=4,v=4;",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
                "\x1b\\"
            )
            .as_bytes();
            // Display with explicit source rect: x=1, y=1, w=2, h=2.
            let display = b"\x1b_Ga=p,i=1,p=1,x=1,y=1,w=2,h=2;\x1b\\";
            vt_write(t, transmit);
            vt_write(t, display);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut x: u32 = 0;
            let mut y: u32 = 0;
            let mut w: u32 = 0;
            let mut h: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_source_rect(iter, img, &mut x, &mut y, &mut w, &mut h)
            });
            assert_eq!(1, x);
            assert_eq!(1, y);
            assert_eq!(2, w);
            assert_eq!(2, h);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_source_rect clamps to image bounds"
    #[test]
    fn placement_source_rect_clamps_to_image_bounds() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 24, 10, 20)
            });

            // Transmit a 4x4 RGBA image (64 bytes = 4*4*4).
            let transmit = concat!(
                "\x1b_Ga=t,t=d,f=32,i=1,s=4,v=4;",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==",
                "\x1b\\"
            )
            .as_bytes();
            // Display with source rect that exceeds image bounds: x=3, y=3, w=10, h=10.
            // Should clamp to x=3, y=3, w=1, h=1.
            let display = b"\x1b_Ga=p,i=1,p=1,x=3,y=3,w=10,h=10;\x1b\\";
            vt_write(t, transmit);
            vt_write(t, display);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut x: u32 = 0;
            let mut y: u32 = 0;
            let mut w: u32 = 0;
            let mut h: u32 = 0;
            assert_eq!(Result::Success, unsafe {
                placement_source_rect(iter, img, &mut x, &mut y, &mut w, &mut h)
            });
            assert_eq!(3, x);
            assert_eq!(3, y);
            assert_eq!(1, w);
            assert_eq!(1, h);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_source_rect null args return invalid_value"
    #[test]
    fn placement_source_rect_null_args_return_invalid_value() {
        if !KITTY_GRAPHICS {
            return;
        }
        let mut x: u32 = 0;
        let mut y: u32 = 0;
        let mut w: u32 = 0;
        let mut h: u32 = 0;
        assert_eq!(Result::InvalidValue, unsafe {
            placement_source_rect(
                core::ptr::null_mut(),
                core::ptr::null(),
                &mut x,
                &mut y,
                &mut w,
                &mut h,
            )
        });
    }

    // test "image_get on null returns invalid_value"
    #[test]
    fn image_get_on_null_returns_invalid_value() {
        if !KITTY_GRAPHICS {
            return;
        }
        let mut id: u32 = 0;
        assert_eq!(Result::InvalidValue, unsafe {
            image_get(
                core::ptr::null(),
                ImageData::Id,
                &mut id as *mut u32 as *mut c_void,
            )
        });
    }

    // test "placement_render_info returns all fields"
    #[test]
    fn placement_render_info_returns_all_fields() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 24, 10, 20)
            });

            let cmd = b"\x1b_Ga=T,t=d,f=24,i=1,p=1,s=1,v=2,c=10,r=1;////////\x1b\\";
            vt_write(t, cmd);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            // var ri: PlacementRenderInfo = .{};
            let mut ri = PlacementRenderInfo::default();
            assert_eq!(Result::Success, unsafe {
                placement_render_info(iter, img, t, &mut ri)
            });
            assert!(ri.viewport_visible);
            assert_eq!(0, ri.viewport_col);
            assert_eq!(0, ri.viewport_row);
            assert_eq!(10, ri.grid_cols);
            assert_eq!(1, ri.grid_rows);
            assert_eq!(0, ri.source_x);
            assert_eq!(0, ri.source_y);
            assert_eq!(1, ri.source_width);
            assert_eq!(2, ri.source_height);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_render_info off-screen sets viewport_visible false"
    #[test]
    fn placement_render_info_off_screen_sets_viewport_visible_false() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 5)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 5, 10, 20)
            });

            let transmit = b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\";
            let display = b"\x1b_Ga=p,i=1,p=1,c=1,r=1;\x1b\\";
            vt_write(t, transmit);
            vt_write(t, display);

            // Scroll the image completely off-screen.
            let scroll = b"\n\n\n\n\n\n\n\n\n\n";
            vt_write(t, scroll);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            let mut ri = PlacementRenderInfo::default();
            assert_eq!(Result::Success, unsafe {
                placement_render_info(iter, img, t, &mut ri)
            });
            assert!(!ri.viewport_visible);
            // Other fields should still be populated.
            assert_eq!(1, ri.grid_cols);
            assert_eq!(1, ri.grid_rows);
            assert_eq!(1, ri.source_width);
            assert_eq!(2, ri.source_height);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_render_info null returns invalid_value"
    #[test]
    fn placement_render_info_null_returns_invalid_value() {
        if !KITTY_GRAPHICS {
            return;
        }
        let mut ri = PlacementRenderInfo::default();
        assert_eq!(Result::InvalidValue, unsafe {
            placement_render_info(
                core::ptr::null_mut(),
                core::ptr::null(),
                core::ptr::null_mut(),
                &mut ri,
            )
        });
    }

    // test "image_get_multi success"
    #[test]
    fn image_get_multi_success() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 24, 10, 20)
            });

            let cmd = b"\x1b_Ga=T,t=d,f=24,i=1,p=1,s=1,v=2;////////\x1b\\";
            vt_write(t, cmd);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            // var id: u32 = 0; var width: u32 = 0; var height: u32 = 0; var written: usize = 0;
            let mut id: u32 = 0;
            let mut width: u32 = 0;
            let mut height: u32 = 0;
            let mut written: usize = 0;

            // const keys = [_]ImageData{ .id, .width, .height };
            let keys = [ImageData::Id, ImageData::Width, ImageData::Height];
            // var values = [_]?*anyopaque{ @ptrCast(&id), @ptrCast(&width), @ptrCast(&height) };
            let values: [*mut c_void; 3] = [
                &mut id as *mut u32 as *mut c_void,
                &mut width as *mut u32 as *mut c_void,
                &mut height as *mut u32 as *mut c_void,
            ];
            // try testing.expectEqual(Result.success, image_get_multi(img, keys.len, &keys, &values, &written));
            assert_eq!(Result::Success, unsafe {
                image_get_multi(
                    img,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_ptr(),
                    &mut written,
                )
            });
            assert_eq!(keys.len(), written);
            assert_eq!(1, id);
            assert_eq!(1, width);
            assert_eq!(2, height);

            unsafe { terminal_c::free(t) };
        });
    }

    // test "image_get_multi error sets out_written"
    #[test]
    fn image_get_multi_error_sets_out_written() {
        if !KITTY_GRAPHICS {
            return;
        }
        // var id: u32 = 0; var written: usize = 99;
        let mut id: u32 = 0;
        let mut written: usize = 99;

        // const keys = [_]ImageData{ .id, .invalid };
        let keys = [ImageData::Id, ImageData::Invalid];
        // var values = [_]?*anyopaque{ @ptrCast(&id), @ptrCast(&id) };
        let values: [*mut c_void; 2] = [
            &mut id as *mut u32 as *mut c_void,
            &mut id as *mut u32 as *mut c_void,
        ];
        // try testing.expectEqual(Result.invalid_value, image_get_multi(null, keys.len, &keys, &values, &written));
        assert_eq!(Result::InvalidValue, unsafe {
            image_get_multi(
                core::ptr::null(),
                keys.len(),
                keys.as_ptr(),
                values.as_ptr(),
                &mut written,
            )
        });
        // try testing.expectEqual(0, written);
        assert_eq!(0, written);
    }

    // test "image_get_multi null keys returns invalid_value"
    #[test]
    fn image_get_multi_null_keys_returns_invalid_value() {
        if !KITTY_GRAPHICS {
            return;
        }
        let mut id: u32 = 0;
        let values: [*mut c_void; 1] = [&mut id as *mut u32 as *mut c_void];
        // try testing.expectEqual(Result.invalid_value, image_get_multi(null, 1, null, &values, null));
        assert_eq!(Result::InvalidValue, unsafe {
            image_get_multi(
                core::ptr::null(),
                1,
                core::ptr::null(),
                values.as_ptr(),
                core::ptr::null_mut(),
            )
        });
    }

    // test "placement_get_multi success"
    #[test]
    fn placement_get_multi_success() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });
            assert_eq!(Result::Success, unsafe {
                terminal_c::resize(t, 80, 24, 10, 20)
            });

            let cmd = b"\x1b_Ga=T,t=d,f=24,i=1,p=1,s=1,v=2,c=10,r=1;////////\x1b\\";
            vt_write(t, cmd);

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            let mut iter: PlacementIterator = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                placement_iterator_new(Some(ta), &mut iter)
            });
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::PlacementIterator,
                    &mut iter as *mut PlacementIterator as *mut c_void,
                )
            });
            assert!(unsafe { placement_iterator_next(iter) });

            // var image_id: u32 = 0; var columns: u32 = 0; var z: i32 = 99; var written: usize = 0;
            let mut image_id: u32 = 0;
            let mut columns: u32 = 0;
            let mut z: i32 = 99;
            let mut written: usize = 0;

            // const keys = [_]PlacementData{ .image_id, .columns, .z };
            let keys = [
                PlacementData::ImageId,
                PlacementData::Columns,
                PlacementData::Z,
            ];
            // var values = [_]?*anyopaque{ @ptrCast(&image_id), @ptrCast(&columns), @ptrCast(&z) };
            let values: [*mut c_void; 3] = [
                &mut image_id as *mut u32 as *mut c_void,
                &mut columns as *mut u32 as *mut c_void,
                &mut z as *mut i32 as *mut c_void,
            ];
            // try testing.expectEqual(Result.success, placement_get_multi(iter, keys.len, &keys, &values, &written));
            assert_eq!(Result::Success, unsafe {
                placement_get_multi(
                    iter,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_ptr(),
                    &mut written,
                )
            });
            assert_eq!(keys.len(), written);
            assert_eq!(1, image_id);
            assert_eq!(10, columns);
            assert_eq!(0, z);

            unsafe { placement_iterator_free(iter) };
            unsafe { terminal_c::free(t) };
        });
    }

    // test "placement_get_multi error sets out_written"
    #[test]
    fn placement_get_multi_error_sets_out_written() {
        if !KITTY_GRAPHICS {
            return;
        }
        let mut id: u32 = 0;
        let mut written: usize = 99;

        // const keys = [_]PlacementData{ .image_id, .invalid };
        let keys = [PlacementData::ImageId, PlacementData::Invalid];
        let values: [*mut c_void; 2] = [
            &mut id as *mut u32 as *mut c_void,
            &mut id as *mut u32 as *mut c_void,
        ];
        // try testing.expectEqual(Result.invalid_value, placement_get_multi(null, keys.len, &keys, &values, &written));
        assert_eq!(Result::InvalidValue, unsafe {
            placement_get_multi(
                core::ptr::null_mut(),
                keys.len(),
                keys.as_ptr(),
                values.as_ptr(),
                &mut written,
            )
        });
        assert_eq!(0, written);
    }

    // test "placement_get_multi null keys returns invalid_value"
    #[test]
    fn placement_get_multi_null_keys_returns_invalid_value() {
        if !KITTY_GRAPHICS {
            return;
        }
        let mut id: u32 = 0;
        let values: [*mut c_void; 1] = [&mut id as *mut u32 as *mut c_void];
        // try testing.expectEqual(Result.invalid_value, placement_get_multi(null, 1, null, &values, null));
        assert_eq!(Result::InvalidValue, unsafe {
            placement_get_multi(
                core::ptr::null_mut(),
                1,
                core::ptr::null(),
                values.as_ptr(),
                core::ptr::null_mut(),
            )
        });
    }

    // test "storage generation via get"
    #[test]
    fn storage_generation_via_get() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            let refresh = |t: terminal_c::Terminal, graphics: &mut KittyGraphics| {
                assert_eq!(Result::Success, unsafe {
                    terminal_c::get(
                        t,
                        terminal_c::TerminalData::KittyGraphics,
                        graphics as *mut KittyGraphics as *mut c_void,
                    )
                });
            };
            refresh(t, &mut graphics);

            // Fresh storage: generation zero.
            let mut gen0: u64 = 99;
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::Generation,
                    &mut gen0 as *mut u64 as *mut c_void,
                )
            });
            assert_eq!(0u64, gen0);

            // Transmit bumps the generation.
            vt_write(t, b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\");
            refresh(t, &mut graphics);
            let mut gen1: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::Generation,
                    &mut gen1 as *mut u64 as *mut c_void,
                )
            });
            assert!(gen1 > gen0);

            // Unrelated terminal writes (plain text) do not bump it.
            vt_write(t, b"hello world");
            refresh(t, &mut graphics);
            let mut gen2: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::Generation,
                    &mut gen2 as *mut u64 as *mut c_void,
                )
            });
            assert_eq!(gen1, gen2);

            // Placement bumps it.
            vt_write(t, b"\x1b_Ga=p,i=1,p=1,c=1,r=1;\x1b\\");
            refresh(t, &mut graphics);
            let mut gen3: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::Generation,
                    &mut gen3 as *mut u64 as *mut c_void,
                )
            });
            assert!(gen3 > gen2);

            // Delete bumps it.
            vt_write(t, b"\x1b_Ga=d,d=A\x1b\\");
            refresh(t, &mut graphics);
            let mut gen4: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::Generation,
                    &mut gen4 as *mut u64 as *mut c_void,
                )
            });
            assert!(gen4 > gen3);

            unsafe { terminal_c::free(t) };
        });
    }

    // test "image generation detects same-sized retransmission"
    #[test]
    fn image_generation_detects_same_sized_retransmission() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            // Transmit a 1x2 RGB image with id=1.
            vt_write(t, b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\");

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            let mut gen1: u64 = 0;
            let mut w1: u32 = 0;
            let mut h1: u32 = 0;
            let mut len1: usize = 0;
            {
                let img = unsafe { image_get_handle(graphics, 1) };
                assert!(!img.is_null());
                assert_eq!(Result::Success, unsafe {
                    image_get(
                        img,
                        ImageData::Generation,
                        &mut gen1 as *mut u64 as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    image_get(img, ImageData::Width, &mut w1 as *mut u32 as *mut c_void)
                });
                assert_eq!(Result::Success, unsafe {
                    image_get(img, ImageData::Height, &mut h1 as *mut u32 as *mut c_void)
                });
                assert_eq!(Result::Success, unsafe {
                    image_get(
                        img,
                        ImageData::DataLen,
                        &mut len1 as *mut usize as *mut c_void,
                    )
                });
                assert!(gen1 > 0);
            }

            // Retransmit the same ID with identical dimensions but different
            // pixel bytes. All size heuristics match; only the generation
            // reveals the change.
            vt_write(t, b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;AAAAAAAA\x1b\\");
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });

            {
                let img = unsafe { image_get_handle(graphics, 1) };
                assert!(!img.is_null());
                let mut gen2: u64 = 0;
                let mut w2: u32 = 0;
                let mut h2: u32 = 0;
                let mut len2: usize = 0;
                assert_eq!(Result::Success, unsafe {
                    image_get(
                        img,
                        ImageData::Generation,
                        &mut gen2 as *mut u64 as *mut c_void,
                    )
                });
                assert_eq!(Result::Success, unsafe {
                    image_get(img, ImageData::Width, &mut w2 as *mut u32 as *mut c_void)
                });
                assert_eq!(Result::Success, unsafe {
                    image_get(img, ImageData::Height, &mut h2 as *mut u32 as *mut c_void)
                });
                assert_eq!(Result::Success, unsafe {
                    image_get(
                        img,
                        ImageData::DataLen,
                        &mut len2 as *mut usize as *mut c_void,
                    )
                });

                // Size heuristics are identical...
                assert_eq!(w1, w2);
                assert_eq!(h1, h2);
                assert_eq!(len1, len2);

                // ...but the generation changed.
                assert!(gen2 > gen1);
            }

            unsafe { terminal_c::free(t) };
        });
    }

    // test "image generation via image_get_multi"
    #[test]
    fn image_generation_via_image_get_multi() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            vt_write(t, b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\");

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            let mut id: u32 = 0;
            let mut generation: u64 = 0;
            let mut written: usize = 0;
            let keys = [ImageData::Id, ImageData::Generation];
            let values: [*mut c_void; 2] = [
                &mut id as *mut u32 as *mut c_void,
                &mut generation as *mut u64 as *mut c_void,
            ];
            assert_eq!(Result::Success, unsafe {
                image_get_multi(
                    img,
                    keys.len(),
                    keys.as_ptr(),
                    values.as_ptr(),
                    &mut written,
                )
            });
            assert_eq!(keys.len(), written);
            assert_eq!(1, id);
            assert!(generation > 0);

            // The image stamp came from the same sequence as (and here, the
            // same event as) the storage stamp.
            let mut storage_gen: u64 = 0;
            assert_eq!(Result::Success, unsafe {
                get(
                    graphics,
                    Data::Generation,
                    &mut storage_gen as *mut u64 as *mut c_void,
                )
            });
            assert_eq!(storage_gen, generation);

            unsafe { terminal_c::free(t) };
        });
    }

    // test "image compression and format always report decoded data"
    #[test]
    fn image_compression_and_format_always_report_decoded_data() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            // Transmit a zlib-compressed 1x1 RGB image (o=z). The payload is
            // base64(zlib([0xFF, 0x00, 0x00])).
            //
            // zlib stream for the 3 bytes FF 00 00 (stored block).
            // 78 01: zlib header; 01: final stored block; 03 00 len;
            // FC FF nlen; FF 00 00 data; adler32 = 0x03000100 (big endian).
            //   raw = [0x78,0x01,0x01,0x03,0x00,0xFC,0xFF,0xFF,0x00,0x00,0x03,0x00,0x01,0x00]
            //
            // PORT NOTE: the reference computes the base64 at `comptime` with
            // `std.base64.standard.Encoder`. Rust has no comptime base64 in
            // std, so the encoding is inlined as the literal below (verified:
            // `base64(raw) == "eAEBAwD8//8AAAMAAQA="` and
            // `zlib_inflate(raw) == FF 00 00`).
            vt_write(
                t,
                b"\x1b_Ga=t,t=d,f=24,o=z,i=1,s=1,v=1;eAEBAwD8//8AAAMAAQA=\x1b\\",
            );

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::get(
                    t,
                    terminal_c::TerminalData::KittyGraphics,
                    &mut graphics as *mut KittyGraphics as *mut c_void,
                )
            });
            let img = unsafe { image_get_handle(graphics, 1) };
            assert!(!img.is_null());

            // Compression must report NONE: the data was inflated at
            // transmission time.
            let mut comp: ImageCompression = ImageCompression::ZlibDeflate;
            assert_eq!(Result::Success, unsafe {
                image_get(
                    img,
                    ImageData::Compression,
                    &mut comp as *mut ImageCompression as *mut c_void,
                )
            });
            assert_eq!(ImageCompression::None, comp);

            // Data is the decoded pixels: width * height * bpp.
            let mut data_len: usize = 0;
            assert_eq!(Result::Success, unsafe {
                image_get(
                    img,
                    ImageData::DataLen,
                    &mut data_len as *mut usize as *mut c_void,
                )
            });
            assert_eq!(3usize, data_len);

            let mut data_ptr: *const u8 = core::ptr::null();
            assert_eq!(Result::Success, unsafe {
                image_get(
                    img,
                    ImageData::DataPtr,
                    &mut data_ptr as *mut *const u8 as *mut c_void,
                )
            });
            assert_eq!(&[0xFFu8, 0x00, 0x00][..], unsafe {
                core::slice::from_raw_parts(data_ptr, data_len)
            });

            unsafe { terminal_c::free(t) };
        });
    }

    // test "generation never recurs across resets and screen switches"
    #[test]
    fn generation_never_recurs_across_resets_and_screen_switches() {
        if !KITTY_GRAPHICS {
            return;
        }
        with_test_allocator(|ta| {
            let mut t: terminal_c::Terminal = core::ptr::null_mut();
            assert_eq!(Result::Success, unsafe {
                terminal_c::new(ta as *const CAllocator, &mut t, 80, 24)
            });

            let mut graphics: KittyGraphics = core::ptr::null_mut();
            let mut gen: u64 = 0;

            macro_rules! read_gen {
                () => {{
                    assert_eq!(Result::Success, unsafe {
                        terminal_c::get(
                            t,
                            terminal_c::TerminalData::KittyGraphics,
                            &mut graphics as *mut KittyGraphics as *mut c_void,
                        )
                    });
                    assert_eq!(Result::Success, unsafe {
                        get(
                            graphics,
                            Data::Generation,
                            &mut gen as *mut u64 as *mut c_void,
                        )
                    });
                }};
            }

            let transmit = b"\x1b_Ga=t,t=d,f=24,i=1,s=1,v=2;////////\x1b\\";

            // Transmit on the main screen.
            vt_write(t, transmit);
            read_gen!();
            let gen_main = gen;
            assert!(gen_main > 0);

            // Switch to the alternate screen: its storage is untouched, so its
            // generation is zero (which always means "empty").
            vt_write(t, b"\x1b[?1049h");
            read_gen!();
            assert_eq!(0u64, gen);

            // A transmit on the alt screen draws from the same global sequence,
            // so its stamp is strictly greater than anything seen on the main
            // screen: an embedder keying a cache on the generation value alone
            // can never confuse the two storages.
            vt_write(t, transmit);
            read_gen!();
            let gen_alt = gen;
            assert!(gen_alt > gen_main);

            // Back to the main screen: its generation is unchanged.
            vt_write(t, b"\x1b[?1049l");
            read_gen!();
            assert_eq!(gen_main, gen);

            // Reset zeroes the storage (empty), and the next mutation continues
            // from the global sequence: past values are never reused for
            // different content.
            unsafe { terminal_c::reset(t) };
            read_gen!();
            assert_eq!(0u64, gen);

            vt_write(t, transmit);
            read_gen!();
            assert!(gen > gen_alt);

            unsafe { terminal_c::free(t) };
        });
    }

    // ==================================================================
    // W33-B02-SELECTOR-01: out-of-range raw selectors return
    // RUSTTY_INVALID_VALUE.
    //
    // DELIBERATE DIVERGENCE (D11, `.port/phase-3/README.md`): the reference's
    // `get`/`image_get`/`placement_get` take the selector as an `enum(c_int)`
    // parameter and `switch` on it, so an undeclared integer is a corrupt enum
    // tag — a safety-checked Zig build panics ("switch on corrupt value") and a
    // ReleaseFast build is UB, i.e. there is no defined oracle behavior to
    // match. Rust cannot receive an out-of-range `#[repr(C)]` enum by value at
    // all (it is materialized in the callee prologue, before any check could
    // run), so these exports take the selector as a plain `c_int` and narrow it
    // via `from_c_int`, returning `.invalid_value` — the same disposition the
    // reference gives `placement_iterator_set`, where it DOES carry an explicit
    // `std.enums.fromInt` guard. These tests pin that contract so it cannot
    // regress silently.

    // The narrowing itself: undeclared integers are rejected, declared ones
    // (including `invalid = 0`) are not.
    #[test]
    fn selector_from_c_int_rejects_out_of_range() {
        // (the selector enums derive no `Debug`, so these are `is_none`/
        // `matches!` rather than `assert_eq!`)
        assert!(Data::from_c_int(99).is_none());
        assert!(ImageData::from_c_int(99).is_none());
        assert!(PlacementData::from_c_int(99).is_none());

        // -1 (a sign-extended C `int`) is out of range too.
        assert!(Data::from_c_int(-1).is_none());
        assert!(ImageData::from_c_int(-1).is_none());
        assert!(PlacementData::from_c_int(-1).is_none());

        // The declared `invalid = 0` tag IS in range; it keeps its
        // `.invalid_value` disposition inside `get`, not here.
        assert!(matches!(Data::from_c_int(0), Some(Data::Invalid)));
        assert!(matches!(ImageData::from_c_int(0), Some(ImageData::Invalid)));
        assert!(matches!(
            PlacementData::from_c_int(0),
            Some(PlacementData::Invalid)
        ));
    }

    // The exported C entry points: an undeclared selector returns
    // RUSTTY_INVALID_VALUE (-2) WITHOUT dereferencing the handle, and the
    // declared `invalid = 0` selector is the matched in-range control.
    #[test]
    fn exported_getters_reject_out_of_range_selector() {
        const INVALID_VALUE: std::os::raw::c_int = -2;
        assert_eq!(INVALID_VALUE, Result::InvalidValue.to_c_int());

        // rustty_kitty_graphics_get
        assert_eq!(INVALID_VALUE, unsafe {
            crate::rustty_kitty_graphics_get(core::ptr::null_mut(), 99, core::ptr::null_mut())
        });
        assert_eq!(INVALID_VALUE, unsafe {
            crate::rustty_kitty_graphics_get(core::ptr::null_mut(), 0, core::ptr::null_mut())
        });

        // rustty_kitty_graphics_image_get
        assert_eq!(INVALID_VALUE, unsafe {
            crate::rustty_kitty_graphics_image_get(core::ptr::null(), 99, core::ptr::null_mut())
        });
        assert_eq!(INVALID_VALUE, unsafe {
            crate::rustty_kitty_graphics_image_get(core::ptr::null(), 0, core::ptr::null_mut())
        });

        // rustty_kitty_graphics_placement_get
        assert_eq!(INVALID_VALUE, unsafe {
            crate::rustty_kitty_graphics_placement_get(
                core::ptr::null_mut(),
                99,
                core::ptr::null_mut(),
            )
        });
        assert_eq!(INVALID_VALUE, unsafe {
            crate::rustty_kitty_graphics_placement_get(
                core::ptr::null_mut(),
                0,
                core::ptr::null_mut(),
            )
        });

        // The matched control the reference DOES implement with an explicit
        // guard: `placement_iterator_set` rejects an undeclared option the same
        // way in both trees.
        assert_eq!(INVALID_VALUE, unsafe {
            crate::rustty_kitty_graphics_placement_iterator_set(
                core::ptr::null_mut(),
                99,
                core::ptr::null(),
            )
        });
    }
}
