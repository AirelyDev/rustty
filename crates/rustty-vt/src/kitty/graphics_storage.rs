//! rustty-vt kitty/graphics_storage
//!
//! PORTS: ghostty src/terminal/kitty/graphics_storage.zig
//!
//! PORT STATUS: ported

// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
//
// const terminal = @import("../main.zig");
// const point = @import("../point.zig");
// const size = @import("../size.zig");
// const command = @import("graphics_command.zig");
// const PageList = @import("../PageList.zig");
// const Screen = @import("../Screen.zig");
// const LoadingImage = @import("graphics_image.zig").LoadingImage;
// const Image = @import("graphics_image.zig").Image;
// const Rect = @import("graphics_image.zig").Rect;
// const Command = command.Command;
//
// PORT NOTE: the reference imports above map to the following Rustty paths.
// `terminal.Screen`/`terminal.Terminal` are `crate::screen::Screen` /
// `crate::terminal::Terminal`; `PageList.Pin` is `crate::pagelist::Pin`; the
// `command`/`Image`/`LoadingImage`/`Rect` come from the sibling kitty modules.
use crate::kitty::graphics_command as command;
use crate::kitty::graphics_image::{Image, Limits, LoadingImage, Rect};
use crate::pagelist as PageList;
use crate::point;
use crate::screen::Screen;
use crate::size;
use crate::terminal::Terminal;
use allocator_api2::alloc::Global as Allocator;
use rustty_util::quirks::inlineAssert as assert;
use std::collections::HashMap;

// const log = std.log.scoped(.kitty_gfx);

/// Process-global counter backing all generation stamps (see
/// [`ImageStorage::generation`] and [`Image::generation`]). This is global
/// rather than per-storage so that stamps are unique across every storage in
/// the process: two mutation events never produce the same value, even across
/// separate screens (main vs. alt), storage resets, or separate terminals. This
/// lets consumers use a generation value alone as a cache key without any
/// ambiguity.
///
/// Atomic because separate terminals may mutate their storages from different
/// threads.
//
// PORT NOTE (8fe5913f7): upstream's comptime-selected mutex fallback for
// targets without 64-bit atomics is not needed — every target in rustty's
// matrix (`aarch64-apple-darwin`, `x86_64-apple-darwin`,
// `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
// `wasm32-unknown-unknown`) reports `target_has_atomic = "64"`, verified WS8
// T14 with `rustc --target <t> --print cfg | grep target_has_atomic=\"64\"`.
// `8fe5913f7` is therefore ported as this note (N/A), not as code. Rust would
// not even compile a fallback-free `AtomicU64` on such a target, so a
// regression here is a build error, not a silent miscompile.
static GENERATION_COUNTER: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Returns the next generation stamp. Stamps are unique and strictly
/// monotonically increasing process-wide, starting at 1 (0 is reserved to mean
/// "never stamped").
pub fn next_generation() -> u64 {
    GENERATION_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed) + 1
}

/// An image storage is associated with a terminal screen (i.e. main
/// screen, alt screen) and contains all the transmitted images and
/// placements.
//
// PORT NOTE: the reference uses `std.AutoHashMapUnmanaged(u32, Image)` for
// `images` and `std.AutoHashMapUnmanaged(PlacementKey, Placement)` for
// `placements`. The Image value owns a `Vec<u8>` (non-`Copy`), so the
// allocator-backed pointer map (`crate::hash_map::HashMapUnmanaged`, which is
// `K: Copy, V: Copy`) cannot store it. As elsewhere in the vt port for
// heap-side maps (see `mouse`/`x11_color`/`page_impl`), we use Rust's
// `std::collections::HashMap`. Both Zig and Rust hash maps are unordered; the
// reference's iterate-and-`removeByPtr` pattern is reproduced by collecting the
// matching keys first and then removing them, which is observably identical (no
// behavior depends on iteration order — the tests assert only counts/membership).
pub struct ImageStorage {
    /// Dirty is set to true if placements or images change. This is
    /// purely informational for the renderer and doesn't affect the
    /// correctness of the program. The renderer must set this to false
    /// if it cares about this value.
    ///
    /// Note that dirty is also set by scrolling and resizing (outside of this
    /// struct) because those move placement pins, even though the set of
    /// images/placements itself is unchanged. See [`Self::generation`] for a
    /// signal that only tracks content mutations.
    ///
    /// Invariant: dirty is always set when generation changes (`mark_mutated`
    /// sets both); dirty set without a generation change means a geometry-only
    /// event.
    pub dirty: bool,

    /// Generation stamp of the last content mutation to this storage: any image
    /// transmit/replace, placement add, or delete of either. Zero means the
    /// storage has never been mutated (and is therefore empty).
    ///
    /// Unlike `dirty`, this is NOT updated by scrolling/resizing, so an
    /// unchanged generation means the placement set and all image data are
    /// identical; only placement geometry (pins) may have moved. Values come
    /// from a process-global monotonic counter, so a value observed from any
    /// storage never recurs for different content, even across screen switches
    /// or storage resets.
    ///
    /// This field must only be written via `mark_mutated`.
    pub generation: u64,

    /// This is the next automatically assigned image ID. We start mid-way
    /// through the u32 range to avoid collisions with buggy programs.
    /// TODO: This isn't good enough, it's perfectly legal for programs
    ///       to use IDs in the latter half of the range and collisions
    ///       are not gracefully handled.
    pub next_image_id: u32,

    /// This is the next automatically assigned placement ID. This is never
    /// user-facing so we can start at 0. This is 32-bits because we use
    /// the same space for external placement IDs. We can start at zero
    /// because any number is valid.
    pub next_internal_placement_id: u32,

    /// The set of images that are currently known.
    pub images: ImageMap,

    /// The set of placements for loaded images.
    pub placements: PlacementMap,

    /// Non-null if there is an in-progress loading image.
    pub loading: Option<Box<LoadingImage>>,

    /// The limits of what medium types are allowed for image loading.
    pub image_limits: Limits,

    /// The total bytes of image data that have been loaded and the limit.
    /// If the limit is reached, the oldest images will be evicted to make
    /// space. Unused images take priority.
    pub total_bytes: usize,
    pub total_limit: usize,
}

// const ImageMap = std.AutoHashMapUnmanaged(u32, Image);
type ImageMap = HashMap<u32, Image>;
// const PlacementMap = std.AutoHashMapUnmanaged(PlacementKey, Placement);
type PlacementMap = HashMap<PlacementKey, Placement>;

impl Default for ImageStorage {
    // PORT NOTE: the reference declares all field defaults inline on the struct
    // (`dirty: bool = false`, ..., `total_limit: usize = 320 * 1000 * 1000`).
    // Rust has no per-field defaults, so `Default` materializes them verbatim,
    // mirroring the `.{}` struct literal used throughout the reference (and the
    // tests, e.g. `var s: ImageStorage = .{};`).
    fn default() -> Self {
        ImageStorage {
            dirty: false,
            generation: 0,
            next_image_id: 2147483647,
            next_internal_placement_id: 0,
            images: ImageMap::new(),
            placements: PlacementMap::new(),
            loading: None,
            image_limits: Limits::DIRECT, // .direct
            total_bytes: 0,
            total_limit: 320 * 1000 * 1000, // 320MB
        }
    }
}

impl ImageStorage {
    pub fn deinit(&mut self, alloc: Allocator, s: &mut Screen) {
        // if (self.loading) |loading| loading.destroy(alloc);
        // PORT NOTE: `LoadingImage.destroy` frees its data + the heap allocation;
        // dropping the owning `Box<LoadingImage>` does both.
        let _ = &alloc;
        self.loading = None;

        // var it = self.images.iterator();
        // while (it.next()) |kv| kv.value_ptr.deinit(alloc);
        // self.images.deinit(alloc);
        // PORT NOTE: `Image.deinit` frees its `data`; clearing the owning map
        // drops every `Image` (and its `Vec<u8>`).
        self.images.clear();

        self.clear_placements(s);
        // self.placements.deinit(alloc);
        self.placements = PlacementMap::new();
    }

    /// Kitty image protocol is enabled if we have a non-zero limit.
    pub fn enabled(&self) -> bool {
        self.total_limit != 0
    }

    /// Record a content mutation: marks the storage dirty and assigns a fresh
    /// generation stamp. Must be called by anything that changes the set of
    /// images or placements (or image contents).
    ///
    /// Do NOT call this for geometry-only events (scrolling, resizing, screen
    /// switches); those must set only the dirty flag directly. Bumping the
    /// generation for geometry changes would break the contract that an
    /// unchanged generation means unchanged contents.
    fn mark_mutated(&mut self) {
        self.dirty = true;
        self.generation = next_generation();
    }

    /// Sets the limit in bytes for the total amount of image data that
    /// can be loaded. If this limit is lower, this will do an eviction
    /// if necessary. If the value is zero, then Kitty image protocol will
    /// be disabled.
    pub fn set_limit(
        &mut self,
        alloc: Allocator,
        s: &mut Screen,
        limit: usize,
    ) -> Result<(), OutOfMemory> {
        // Special case disabling by quickly deleting all
        if limit == 0 {
            // PORT NOTE: `Limits` is no longer `Copy` (it owns the temp-file
            // directory bytes); the save/restore around `deinit` moves instead.
            let image_limits = core::mem::replace(&mut self.image_limits, Limits::DIRECT);
            self.deinit(alloc, s);
            *self = ImageStorage {
                image_limits,
                ..ImageStorage::default()
            };
            self.mark_mutated();
        }

        // If we re lowering our limit, check if we need to evict.
        if limit < self.total_bytes {
            let req_bytes = self.total_bytes - limit;
            // log.info("evicting images to lower limit, evicting={}", .{req_bytes});
            if !self.evict_image(alloc, req_bytes)? {
                // log.warn("failed to evict enough images for required bytes", .{});
            }
        }

        self.total_limit = limit;
        Ok(())
    }

    /// Add an already-loaded image to the storage. This will automatically
    /// free any existing image with the same ID.
    // PORT NOTE: `Allocator.Error!void` → `Result<(), OutOfMemory>`.
    pub fn add_image(&mut self, alloc: Allocator, img: Image) -> Result<(), OutOfMemory> {
        // If the image itself is over the limit, then error immediately
        if img.data.len() > self.total_limit {
            return Err(OutOfMemory);
        }

        // If this would put us over the limit, then evict.
        let total_bytes = self.total_bytes + img.data.len();
        if total_bytes > self.total_limit {
            let req_bytes = total_bytes - self.total_limit;
            // log.info("evicting images to make space for {} bytes", .{req_bytes});
            if !self.evict_image(alloc, req_bytes)? {
                // log.warn("failed to evict enough images for required bytes", .{});
                return Err(OutOfMemory);
            }
        }

        // Do the gop op first so if it fails we don't get a partial state
        // PORT NOTE: `getOrPut` on a Rust `HashMap` is infallible (it owns its
        // backing storage and grows via the global allocator); the reference's
        // `try` cannot fire for OOM here. We mirror `found_existing` accounting
        // via `HashMap::get`/`insert`.

        // log.debug("addImage image={}", .{img: {
        //     var copy = img;
        //     copy.data = "";
        //     break :img copy;
        // }});

        // Write our new image
        // if (gop.found_existing) {
        //     self.total_bytes -= gop.value_ptr.data.len;
        //     gop.value_ptr.deinit(alloc);
        // }
        let id = img.id;
        let data_len = img.data.len();
        if let Some(existing) = self.images.get(&id) {
            self.total_bytes -= existing.data.len();
            // existing `Image` (and its `Vec`) is dropped by the `insert` below.
        }
        let _ = &alloc;

        // gop.value_ptr.* = img;
        self.images.insert(id, img);
        // self.total_bytes += img.data.len;
        self.total_bytes += data_len;

        // Stamp the stored image with a fresh generation. This gives every
        // add/replace a unique stamp even when the same image ID is
        // retransmitted with identical dimensions, so consumers (e.g. renderer
        // texture caches) can detect content changes.
        self.mark_mutated();
        let generation = self.generation;
        self.images.get_mut(&id).unwrap().generation = generation;
        Ok(())
    }

    /// Add a placement for a given image. The caller must verify in advance
    /// the image exists to prevent memory corruption.
    pub fn add_placement(
        &mut self,
        alloc: Allocator,
        image_id: u32,
        placement_id: u32,
        p: Placement,
    ) -> Result<(), OutOfMemory> {
        let _ = &alloc;
        // assert(self.images.get(image_id) != null);
        assert(self.images.contains_key(&image_id));
        // log.debug("placement image_id={} placement_id={} placement={}\n", .{
        //     image_id,
        //     placement_id,
        //     p,
        // });

        // The important piece here is that the placement ID needs to
        // be marked internal if it is zero. This allows multiple placements
        // to be added for the same image. If it is non-zero, then it is
        // an external placement ID and we can only have one placement
        // per (image id, placement id) pair.
        let key: PlacementKey = PlacementKey {
            image_id,
            placement_id: if placement_id == 0 {
                PlacementId {
                    tag: PlacementIdTag::Internal,
                    id: {
                        // defer self.next_internal_placement_id +%= 1;
                        let id = self.next_internal_placement_id;
                        self.next_internal_placement_id =
                            self.next_internal_placement_id.wrapping_add(1);
                        id
                    },
                }
            } else {
                PlacementId {
                    tag: PlacementIdTag::External,
                    id: placement_id,
                }
            },
        };

        // const gop = try self.placements.getOrPut(alloc, key);
        // gop.value_ptr.* = p;
        self.placements.insert(key, p);

        self.mark_mutated();
        Ok(())
    }

    fn clear_placements(&mut self, s: &mut Screen) {
        // var it = self.placements.iterator();
        // while (it.next()) |entry| entry.value_ptr.deinit(s);
        for entry in self.placements.values() {
            entry.deinit(s);
        }
        self.placements.clear(); // clearRetainingCapacity
    }

    /// Get an image by its ID. If the image doesn't exist, null is returned.
    pub fn image_by_id(&self, image_id: u32) -> Option<Image> {
        self.images.get(&image_id).cloned()
    }

    /// Get an image by its number. If the image doesn't exist, return null.
    pub fn image_by_number(&self, image_number: u32) -> Option<Image> {
        let mut newest: Option<Image> = None;

        // var it = self.images.iterator();
        // while (it.next()) |kv| { ... }
        // PORT NOTE: the reference nests `if (number == ...) { if (newest == null
        // or ...) {` — we preserve that nested form per R1 (clippy's
        // `collapsible_if` would merge them).
        #[allow(clippy::collapsible_if)]
        for kv in self.images.values() {
            if kv.number == image_number {
                if newest.is_none() || kv.generation > newest.as_ref().unwrap().generation {
                    newest = Some(kv.clone());
                }
            }
        }

        newest
    }

    /// Delete placements, images.
    pub fn delete(&mut self, alloc: Allocator, t: &mut Terminal, cmd: command::Delete) {
        // Deletes only ever remove placements/images, so comparing counts
        // before and after tells us whether anything actually changed. Only
        // then do we mark a mutation. This matters because a delete-all runs on
        // every screen clear (e.g. `ESC [ 2 J`), and we don't want empty clears
        // to dirty the image state or bump the generation.
        //
        // PORT NOTE: Zig's `defer` runs on every return path; Rust has no
        // `defer`. To keep the trailing generation check unconditionally
        // correct — including if a future arm needs to bail out after
        // already removing a placement/image — the `match` is wrapped in a
        // labeled block and every early exit is a `break 'delete` rather
        // than a bare `return`, so control always reaches the count
        // comparison below.
        let placements_before = self.placements.len();
        let images_before = self.images.len();

        'delete: {
            match cmd {
                command::Delete::All(delete_images) => {
                    // var it = self.placements.iterator();
                    // while (it.next()) |entry| { ... }
                    let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
                    for key in keys {
                        // Skip virtual placements
                        match self.placements.get(&key).unwrap().location {
                            Location::Pin(_) => {}
                            Location::Virtual => continue,
                        }

                        // Deinit the placement and remove it
                        let image_id = key.image_id;
                        let entry = self.placements.get(&key).unwrap();
                        entry.deinit(unsafe { &mut *t.screens.active });
                        self.placements.remove(&key);
                        if delete_images {
                            self.delete_if_unused(alloc, image_id);
                        }
                    }

                    if delete_images {
                        // var image_it = self.images.iterator();
                        // while (image_it.next()) |kv| self.deleteIfUnused(alloc, kv.key_ptr.*);
                        let image_keys: Vec<u32> = self.images.keys().copied().collect();
                        for k in image_keys {
                            self.delete_if_unused(alloc, k);
                        }
                    }
                }

                command::Delete::Id {
                    delete,
                    image_id,
                    placement_id,
                } => self.delete_by_id(
                    alloc,
                    unsafe { &mut *t.screens.active },
                    image_id,
                    placement_id,
                    delete,
                ),

                command::Delete::Newest {
                    delete,
                    image_number,
                    placement_id,
                } => {
                    // const img = self.imageByNumber(v.image_number) orelse break :newest;
                    let img = match self.image_by_number(image_number) {
                        Some(img) => img,
                        None => break 'delete,
                    };
                    self.delete_by_id(
                        alloc,
                        unsafe { &mut *t.screens.active },
                        img.id,
                        placement_id,
                        delete,
                    );
                }

                command::Delete::IntersectCursor(delete_images) => {
                    let cursor = unsafe { &(*t.screens.active).cursor };
                    let p = point::Point::Active(point::Coordinate {
                        x: cursor.x,
                        y: cursor.y as u32,
                    });
                    self.delete_intersecting(alloc, t, p, delete_images, |_p| true);
                }

                // PORT NOTE: the reference guards (`v.x <= 0 or v.y <= 0`, `v.x <= 0`,
                // `v.first <= 0 or v.last <= 0`) compare an unsigned `u32` against
                // zero literally; we preserve that exact `<= 0` form (semantically
                // `== 0`) per R1, which trips clippy's `absurd_extreme_comparisons`
                // on the redundant `< 0` half — allowed here for fidelity.
                #[allow(clippy::absurd_extreme_comparisons)]
                command::Delete::IntersectCell { delete, x, y } => {
                    if x <= 0 || y <= 0 {
                        // log.warn("delete intersect cell coords must be at least 1", .{});
                        break 'delete;
                    }

                    // .x = std.math.cast(size.CellCountInt, v.x - 1) orelse break :intersect_cell,
                    let cx = match size::CellCountInt::try_from(x - 1) {
                        Ok(v) => v,
                        Err(_) => break 'delete,
                    };
                    // .y = std.math.cast(size.CellCountInt, v.y - 1) orelse break :intersect_cell,
                    // PORT NOTE: the reference casts `v.y - 1` to `size.CellCountInt`
                    // (returning on overflow); `point.Coordinate.y` is `u32` in this
                    // port, so we range-check against `CellCountInt` first, then widen.
                    let cy = match size::CellCountInt::try_from(y - 1) {
                        Ok(v) => v as u32,
                        Err(_) => break 'delete,
                    };
                    let p = point::Point::Active(point::Coordinate { x: cx, y: cy });
                    self.delete_intersecting(alloc, t, p, delete, |_p| true);
                }

                #[allow(clippy::absurd_extreme_comparisons)]
                command::Delete::IntersectCellZ { delete, x, y, z } => {
                    if x <= 0 || y <= 0 {
                        // log.warn("delete intersect cell coords must be at least 1", .{});
                        break 'delete;
                    }

                    // .x = std.math.cast(size.CellCountInt, v.x - 1) orelse break :intersect_cell_z,
                    let cx = match size::CellCountInt::try_from(x - 1) {
                        Ok(v) => v,
                        Err(_) => break 'delete,
                    };
                    // .y = std.math.cast(size.CellCountInt, v.y - 1) orelse break :intersect_cell_z,
                    let cy = match size::CellCountInt::try_from(y - 1) {
                        Ok(v) => v as u32,
                        Err(_) => break 'delete,
                    };
                    let p = point::Point::Active(point::Coordinate { x: cx, y: cy });
                    // struct { fn filter(ctx: i32, p: Placement) bool { return p.z == ctx; } }.filter
                    self.delete_intersecting(alloc, t, p, delete, move |placement| {
                        placement.z == z
                    });
                }

                #[allow(clippy::absurd_extreme_comparisons)]
                command::Delete::Column { delete, x } => {
                    if x <= 0 {
                        // log.warn("delete column must be greater than zero", .{});
                        break 'delete;
                    }

                    let x = x - 1;
                    let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
                    for key in keys {
                        let img = match self.image_by_id(key.image_id) {
                            Some(img) => img,
                            None => continue,
                        };
                        let rect = match self.placements.get(&key).unwrap().rect(&img, t) {
                            Some(r) => r,
                            None => continue,
                        };
                        if rect.top_left.x as u32 <= x && rect.bottom_right.x as u32 >= x {
                            self.placements
                                .get(&key)
                                .unwrap()
                                .deinit(unsafe { &mut *t.screens.active });
                            self.placements.remove(&key);
                            if delete {
                                self.delete_if_unused(alloc, img.id);
                            }
                        }
                    }
                }

                #[allow(clippy::absurd_extreme_comparisons)]
                command::Delete::Row { delete, y } => {
                    if y <= 0 {
                        // log.warn("delete row must be greater than zero", .{});
                        break 'delete;
                    }

                    // v.y is in active coords so we want to convert it to a pin
                    // so we can compare by page offsets.
                    let ty = match size::CellCountInt::try_from(y - 1) {
                        Ok(v) => v,
                        Err(_) => break 'delete,
                    };
                    let target_pin = match unsafe { &(*t.screens.active).pages }.pin(
                        point::Point::Active(point::Coordinate { x: 0, y: ty as u32 }),
                    ) {
                        Some(p) => p,
                        None => break 'delete,
                    };

                    let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
                    for key in keys {
                        let img = match self.image_by_id(key.image_id) {
                            Some(img) => img,
                            None => continue,
                        };
                        let rect = match self.placements.get(&key).unwrap().rect(&img, t) {
                            Some(r) => r,
                            None => continue,
                        };

                        // We need to copy our pin to ensure we are at least at
                        // the top-left x.
                        let mut target_pin_copy = target_pin;
                        target_pin_copy.x = rect.top_left.x;
                        if target_pin_copy.is_between(rect.top_left, rect.bottom_right) {
                            self.placements
                                .get(&key)
                                .unwrap()
                                .deinit(unsafe { &mut *t.screens.active });
                            self.placements.remove(&key);
                            if delete {
                                self.delete_if_unused(alloc, img.id);
                            }
                        }
                    }
                }

                command::Delete::Z { delete, z } => {
                    let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
                    for key in keys {
                        match self.placements.get(&key).unwrap().location {
                            Location::Pin(_) => {}

                            // Virtual placeholders cannot delete by z according
                            // to the spec.
                            Location::Virtual => continue,
                        }

                        if self.placements.get(&key).unwrap().z == z {
                            let image_id = key.image_id;
                            self.placements
                                .get(&key)
                                .unwrap()
                                .deinit(unsafe { &mut *t.screens.active });
                            self.placements.remove(&key);
                            if delete {
                                self.delete_if_unused(alloc, image_id);
                            }
                        }
                    }
                }

                #[allow(clippy::absurd_extreme_comparisons)]
                command::Delete::Range {
                    delete,
                    first,
                    last,
                } => {
                    if first <= 0 || last <= 0 {
                        // log.warn("delete range values must be greater than zero", .{});
                        break 'delete;
                    }
                    if first > last {
                        // log.warn("delete range 'x' ({}) must be less than or equal to 'y' ({})", .{ v.first, v.last });
                        break 'delete;
                    }

                    let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
                    for key in keys {
                        if key.image_id >= first || key.image_id <= last {
                            let image_id = key.image_id;
                            self.placements
                                .get(&key)
                                .unwrap()
                                .deinit(unsafe { &mut *t.screens.active });
                            self.placements.remove(&key);
                            if delete {
                                self.delete_if_unused(alloc, image_id);
                            }
                        }
                    }
                }

                // We don't support animation frames yet so they are successfully
                // deleted!
                command::Delete::AnimationFrames(_) => {}
            }
        }

        if self.placements.len() != placements_before || self.images.len() != images_before {
            self.mark_mutated();
        }
    }

    fn delete_by_id(
        &mut self,
        alloc: Allocator,
        s: &mut Screen,
        image_id: u32,
        placement_id: u32,
        delete_unused: bool,
    ) {
        // If no placement, we delete all placements with the ID
        if placement_id == 0 {
            // var it = self.placements.iterator();
            // while (it.next()) |entry| { ... }
            let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
            for key in keys {
                if key.image_id == image_id {
                    self.placements.get(&key).unwrap().deinit(s);
                    self.placements.remove(&key);
                }
            }
        } else {
            let key = PlacementKey {
                image_id,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: placement_id,
                },
            };
            if let Some(entry) = self.placements.get(&key) {
                entry.deinit(s);
                self.placements.remove(&key);
            }
        }

        // If this is specified, then we also delete the image
        // if it is no longer in use.
        if delete_unused {
            self.delete_if_unused(alloc, image_id);
        }
    }

    /// Delete an image if it is unused.
    fn delete_if_unused(&mut self, alloc: Allocator, image_id: u32) {
        // var it = self.placements.iterator();
        // while (it.next()) |kv| { if (kv.key_ptr.image_id == image_id) return; }
        for kv in self.placements.keys() {
            if kv.image_id == image_id {
                return;
            }
        }

        // If we get here, we can delete the image.
        // if (self.images.getEntry(image_id)) |entry| { ... }
        let _ = &alloc;
        if let Some(entry) = self.images.get(&image_id) {
            self.total_bytes -= entry.data.len();
            // entry.value_ptr.deinit(alloc); — dropping the removed Image frees data.
            self.images.remove(&image_id);
        }
    }

    /// Deletes all placements intersecting a screen point.
    //
    // PORT NOTE: the reference takes `filter_ctx: anytype` plus a comptime
    // `filter: ?fn (@TypeOf(filter_ctx), Placement) bool`. We collapse the
    // context+optional-fn into a single Rust closure `filter: F` where `F` is a
    // predicate over `Placement`; callers that pass no filter supply `|_| true`,
    // matching the `filter == null` "always keep" branch.
    fn delete_intersecting<F: Fn(&Placement) -> bool>(
        &mut self,
        alloc: Allocator,
        t: &mut Terminal,
        p: point::Point,
        delete_unused: bool,
        filter: F,
    ) {
        // Convert our target point to a pin for comparison.
        let target_pin = match unsafe { &(*t.screens.active).pages }.pin(p) {
            Some(p) => p,
            None => return,
        };

        let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
        for key in keys {
            let img = match self.image_by_id(key.image_id) {
                Some(img) => img,
                None => continue,
            };
            let rect = match self.placements.get(&key).unwrap().rect(&img, t) {
                Some(r) => r,
                None => continue,
            };
            if target_pin.is_between(rect.top_left, rect.bottom_right) {
                // if (filter) |f| if (!f(filter_ctx, entry.value_ptr.*)) continue;
                if !filter(self.placements.get(&key).unwrap()) {
                    continue;
                }
                self.placements
                    .get(&key)
                    .unwrap()
                    .deinit(unsafe { &mut *t.screens.active });
                self.placements.remove(&key);
                if delete_unused {
                    self.delete_if_unused(alloc, img.id);
                }
            }
        }
    }

    /// Evict image to make space. This will evict the oldest image,
    /// prioritizing transient images and unused images as recommended by the
    /// published Kitty spec.
    ///
    /// This will evict as many images as necessary to make space for
    /// req bytes.
    fn evict_image(&mut self, alloc: Allocator, req: usize) -> Result<bool, OutOfMemory> {
        assert(req <= self.total_limit);
        let _ = &alloc;

        // Ironically we allocate to evict. We should probably redesign the
        // data structures to avoid this but for now allocating a little
        // bit is fine compared to the megabytes we're looking to save.
        // const Candidate = struct { id: u32, time: std.time.Instant, used: bool };

        // var candidates: std.ArrayList(Candidate) = .empty;
        // defer candidates.deinit(alloc);
        let mut candidates: Vec<Candidate> = Vec::with_capacity(self.images.len());

        // var it = self.images.iterator();
        // while (it.next()) |kv| { ... }
        for img in self.images.values() {
            // This is a huge waste. See comment above about redesigning
            // our data structures to avoid this. Eviction should be very
            // rare though and we never have that many images/placements
            // so hopefully this will last a long time.
            let used = {
                let mut used = false;
                for p_kv in self.placements.keys() {
                    if p_kv.image_id == img.id {
                        used = true;
                        break;
                    }
                }
                used
            };

            // const transient = img.usage.transient;
            let transient = img.usage.transient;

            // candidates[i] = .{ ... };
            //
            // PORT NOTE (oracle `a65e11cc9`): the reference swapped the
            // `ArrayList` for a single `alloc.alloc(Candidate, count)` +
            // indexed writes, purely to cut allocation count. rustty's `Vec`
            // is pre-sized with `with_capacity(self.images.len())` below, which
            // is the same one-allocation shape without an unsafe uninit slice.
            candidates.push(Candidate {
                id: img.id,
                generation: img.generation,
                // Map images into four distinct blocks:
                // 0: transient, unused
                // 1: not transient, unused
                // 2: transient, used
                // 3: not transient, used
                block: (if transient { 0u8 } else { 1u8 }) + (if used { 2u8 } else { 0u8 }),
            });
        }

        // Sort
        // std.mem.sortUnstable(Candidate, candidates.items, {}, struct { fn lessThan(...) }.lessThan);
        candidates.sort_unstable_by(|lhs, rhs| {
            // PORT NOTE: the reference `lessThan` returns `bool`; we port it to a
            // total `Ordering` comparator. `lessThan(lhs, rhs)` true ⇒ `Less`.

            // If images mapped into different blocks, prioritize lower
            // numbered blocks.
            // if (lhs.block < rhs.block) return true;
            // if (lhs.block > rhs.block) return false;
            if lhs.block != rhs.block {
                return lhs.block.cmp(&rhs.block);
            }

            // If images mapped to the same block, compare generations.
            if lhs.generation == rhs.generation {
                // If the generation is the same, use the ID to prioritize
                // evicting "earlier" images.
                lhs.id.cmp(&rhs.id)
            } else {
                // If the generation is different, prioritize evicting images
                // from earlier generations.
                lhs.generation.cmp(&rhs.generation)
            }
        });

        // Evicting anything is a content mutation. This matters for the
        // `set_limit` path in particular, which doesn't otherwise mark it.
        //
        // PORT NOTE: Zig `defer if (any_evicted) self.markMutated();`. Rust has
        // no `defer`, so the flag is checked at BOTH success paths: the
        // `return Ok(true)` inside the loop and the trailing `Ok(false)`.
        let mut any_evicted = false;

        // They're in order of best to evict.
        let mut evicted: usize = 0;
        for c in &candidates {
            // Delete all the placements for this image and the image.
            let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
            for key in keys {
                if key.image_id == c.id {
                    self.placements.remove(&key);
                    any_evicted = true;
                }
            }

            // if (self.images.getEntry(c.id)) |entry| { ... }
            if let Some(entry) = self.images.get(&c.id) {
                // log.info("evicting image id={} bytes={}", .{ c.id, entry.value_ptr.data.len });

                evicted += entry.data.len();
                self.total_bytes -= entry.data.len();

                // entry.value_ptr.deinit(alloc); — dropping the removed Image frees data.
                self.images.remove(&c.id);
                any_evicted = true;

                if evicted > req {
                    if any_evicted {
                        self.mark_mutated();
                    }
                    return Ok(true);
                }
            }
        }

        if any_evicted {
            self.mark_mutated();
        }
        Ok(false)
    }
}

/// The allocation-failure error, mirroring Zig's `Allocator.Error`/OutOfMemory.
//
// PORT NOTE: the reference's fallible functions return `Allocator.Error!T`; in
// Rust this is `Result<T, OutOfMemory>`. Rust's `HashMap`/`Vec` own their
// storage and never surface OOM through these APIs, so this error is preserved
// for signature fidelity but never produced in practice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

/// The candidate type used during eviction (`evictImage`).
//
// PORT NOTE: the reference declares this as a function-local `const Candidate =
// struct {...}`. Rust has no local-to-method struct visible across closures,
// so it is hoisted to a private module-level struct with identical fields.
struct Candidate {
    id: u32,
    generation: u64,
    // Map images into four distinct blocks:
    // 0: transient, unused
    // 1: not transient, unused
    // 2: transient, used
    // 3: not transient, used
    //
    // PORT NOTE (oracle `a65e11cc9`): the reference field is `block: u2`. Rust
    // has no `u2`; `u8` is the narrowest carrier. The value is only ever
    // compared, never bit-packed, so the widening is unobservable — and the
    // construction below can only produce 0..=3.
    block: u8,
}

/// Every placement is uniquely identified by the image ID and the
/// placement ID. If an image ID isn't specified it is assumed to be 0.
/// Likewise, if a placement ID isn't specified it is assumed to be 0.
//
// PORT NOTE: the reference `placement_id` is a `packed struct { tag: enum(u1),
// id: u32 }`. The map only ever hashes/compares the key field-wise (it is never
// bit-cast), so a plain struct preserves all observable behavior. `Hash`/`Eq`
// give the `AutoHashMapUnmanaged` auto-context equality used by the lookups.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PlacementKey {
    pub image_id: u32,
    pub placement_id: PlacementId,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PlacementId {
    pub tag: PlacementIdTag,
    pub id: u32,
}

// tag: enum(u1) { internal, external }
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PlacementIdTag {
    Internal,
    External,
}

pub struct Placement {
    /// The location where this placement should be drawn.
    pub location: Location,

    /// Offset of the x/y from the top-left of the cell.
    pub x_offset: u32,
    pub y_offset: u32,

    /// Source rectangle for the image to pull from
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,

    /// The columns/rows this image occupies.
    pub columns: u32,
    pub rows: u32,

    /// The z-index for this placement.
    pub z: i32,
}

/// `pub const Location = union(enum) { pin, virtual }`.
pub enum Location {
    /// Exactly placed on a screen pin.
    //
    // PORT NOTE: the reference is `pin: *PageList.Pin` — a pointer to a *tracked*
    // pin owned by the pagelist (created via `pages.trackPin`). We hold the same
    // raw `*mut Pin`; `deinit` hands it back via `pages.untrackPin`.
    Pin(*mut PageList::Pin),

    /// Virtual placement (U=1) for unicode placeholders.
    Virtual,
}

impl Placement {
    // PORT NOTE: the reference declares per-field defaults (`x_offset: u32 = 0`,
    // etc.) so `.{ .location = ..., .columns = 10 }` is a valid literal. Rust has
    // no per-field defaults; the tests and `delete` code construct placements
    // with only a subset of fields set, so we provide a constructor mirroring the
    // common `.{ .location = ... }` literal (only `location` set, rest zero) plus
    // direct struct construction with `..Placement::with_location(loc)` where more
    // fields are set.
    pub fn with_location(location: Location) -> Placement {
        Placement {
            location,
            x_offset: 0,
            y_offset: 0,
            source_x: 0,
            source_y: 0,
            source_width: 0,
            source_height: 0,
            columns: 0,
            rows: 0,
            z: 0,
        }
    }

    pub fn deinit(&self, s: &mut Screen) {
        match self.location {
            // .pin => |p| s.pages.untrackPin(p),
            Location::Pin(p) => s.pages.untrack_pin(p),
            // .virtual => {},
            Location::Virtual => {}
        }
    }

    /// Returns the size of this placement's image in pixels,
    /// taking into account the source rectangle, specified
    /// rows/columns, and aspect ratio.
    pub fn pixel_size(&self, image: &Image, t: &Terminal) -> PixelSize {
        // Height / width of the image in px.
        let width = if self.source_width > 0 {
            self.source_width
        } else {
            image.width
        };
        let height = if self.source_height > 0 {
            self.source_height
        } else {
            image.height
        };

        // If we don't have any specified cols or rows then the placement
        // should be the native size of the image, and doesn't need to be
        // re-scaled.
        if self.columns == 0 && self.rows == 0 {
            return PixelSize { width, height };
        }

        // We calculate the size of a cell so that we can multiply
        // it by the specified cols/rows to get the correct px size.
        //
        // We assume that the width is divided evenly by the column
        // count and the height by the row count, because it should be.
        let cell_width: u32 = t.width_px / t.cols as u32;
        let cell_height: u32 = t.height_px / t.rows as u32;

        let width_f64: f64 = width as f64;
        let height_f64: f64 = height as f64;

        // If we have a specified cols AND rows then we calculate
        // the width and height from them directly, we don't need
        // to adjust for aspect ratio.
        if self.columns > 0 && self.rows > 0 {
            let calc_width = cell_width * self.columns;
            let calc_height = cell_height * self.rows;

            return PixelSize {
                width: calc_width,
                height: calc_height,
            };
        }

        // Either the columns or the rows were specified, but not both,
        // so we need to calculate the other one based on the aspect ratio.

        // If only the columns were specified, we determine
        // the height of the image based on the aspect ratio.
        if self.columns > 0 {
            let aspect = height_f64 / width_f64;
            let calc_width: u32 = cell_width * self.columns;
            let calc_height: u32 = ((calc_width as f64) * aspect).round() as u32;

            return PixelSize {
                width: calc_width,
                height: calc_height,
            };
        }

        // Otherwise, only the rows were specified, so we
        // determine the width based on the aspect ratio.
        {
            let aspect = width_f64 / height_f64;
            let calc_height: u32 = cell_height * self.rows;
            let calc_width: u32 = ((calc_height as f64) * aspect).round() as u32;

            PixelSize {
                width: calc_width,
                height: calc_height,
            }
        }
    }

    /// Returns the size in grid cells that this placement takes up.
    pub fn grid_size(&self, image: &Image, t: &Terminal) -> GridSize {
        // If we have a specified columns and rows then this is trivial.
        if self.columns > 0 && self.rows > 0 {
            return GridSize {
                cols: self.columns,
                rows: self.rows,
            };
        }

        // Otherwise we calculate the pixel size, divide by
        // cell size, and round up to the nearest integer.
        let calc_size = self.pixel_size(image, t);
        GridSize {
            // std.math.divCeil(u32, calc_size.width + self.x_offset, t.width_px / t.cols) catch 0,
            cols: div_ceil_u32(calc_size.width + self.x_offset, t.width_px / t.cols as u32)
                .unwrap_or(0),
            // std.math.divCeil(u32, calc_size.height + self.y_offset, t.height_px / t.rows) catch 0,
            rows: div_ceil_u32(
                calc_size.height + self.y_offset,
                t.height_px / t.rows as u32,
            )
            .unwrap_or(0),
        }
        // NOTE: Above `divCeil`s can only fail if the cell size is 0,
        //       in such a case it seems safe to return 0 for this.
    }

    /// Returns a selection of the entire rectangle this placement
    /// occupies within the screen. This can return null if the placement
    /// doesn't have an associated rect (i.e. a virtual placement).
    pub fn rect(&self, image: &Image, t: &Terminal) -> Option<Rect> {
        let grid_size = self.grid_size(image, t);
        let pin = match self.location {
            Location::Pin(p) => p,
            Location::Virtual => return None,
        };

        // var br = switch (pin.downOverflow(grid_size.rows - 1)) {
        //     .offset => |v| v,
        //     .overflow => |v| v.end,
        // };
        let pin_ref: &PageList::Pin = unsafe { &*pin };
        let mut br = match pin_ref.down_overflow((grid_size.rows - 1) as usize) {
            PageList::pin::PinOverflow::Offset(v) => v,
            PageList::pin::PinOverflow::Overflow { end, .. } => end,
        };
        br.x = core::cmp::min(
            // We need to sub one here because the x value is
            // one width already. So if the image is width "1"
            // then we add zero to X because X itself is width 1.
            pin_ref.x + (grid_size.cols - 1) as size::CellCountInt,
            t.cols - 1,
        );

        Some(Rect {
            top_left: *pin_ref,
            bottom_right: br,
        })
    }
}

/// Return type of `Placement::pixelSize`: `struct { width: u32, height: u32 }`.
pub struct PixelSize {
    pub width: u32,
    pub height: u32,
}

/// Return type of `Placement::gridSize`: `struct { cols: u32, rows: u32 }`.
pub struct GridSize {
    pub cols: u32,
    pub rows: u32,
}

/// `std.math.divCeil(u32, numerator, denominator)`: ceil division, returning
/// `None` (the reference's `error.DivisionByZero`, caught as `catch 0`) when the
/// denominator is zero.
fn div_ceil_u32(numerator: u32, denominator: u32) -> Option<u32> {
    if denominator == 0 {
        return None;
    }
    Some(numerator.div_ceil(denominator))
}

#[cfg(test)]
// PORT NOTE: the ported tests mirror the reference verbatim:
//   - `s.placements.get(.{...}) != null` → `s.placements.get(&...).is_some()`
//     (clippy `unnecessary_get_then_check` would prefer `contains_key`, but we
//     keep `get(...).is_some()` to mirror the reference `get(...) != null`).
//   - `var s: ImageStorage = .{}; s.total_limit = 5000;` → a `Default::default()`
//     followed by a field assignment (clippy `field_reassign_with_default`).
// Both forms are preserved per R1; the lints are allowed on the test module only.
#[allow(
    clippy::unnecessary_get_then_check,
    clippy::field_reassign_with_default
)]
mod tests {
    use super::*;
    use crate::terminal::Terminal;

    // PORT NOTE: the tests construct `terminal.Terminal.init(alloc, .{ .cols, .rows })`
    // relying on the reference `Terminal.Options` field defaults (`max_scrollback_bytes`,
    // `colors = .default`, `default_modes = .{}`, kitty limits). Rust `Options` has
    // no field defaults, so this helper supplies them verbatim, mirroring the
    // reference `Options` declaration (see terminal.rs's test `opts` helper).
    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> crate::terminal::Options {
        crate::terminal::Options {
            cols,
            rows,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            colors: crate::terminal::Colors::default(),
            default_modes: crate::modes::ModePacked::default(),
            default_cursor_style: crate::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: Limits::DIRECT,
        }
    }

    // Our pin for the placement
    // fn trackPin(t: *terminal.Terminal, pt: point.Coordinate) !*PageList.Pin {
    //     return try t.screens.active.pages.trackPin(t.screens.active.pages.pin(.{
    //         .active = pt,
    //     }).?);
    // }
    fn track_pin(
        t: &mut Terminal,
        pt: point::Coordinate,
    ) -> Result<*mut PageList::Pin, crate::pagelist::OutOfMemory> {
        let s = unsafe { &mut *t.screens.active };
        let pin = s.pages.pin(point::Point::Active(pt)).unwrap();
        s.pages.track_pin(pin)
    }

    // test "storage: add placement with zero placement id"
    #[test]
    fn add_placement_with_zero_placement_id() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 50,
                height: 50,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                width: 25,
                height: 25,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            0,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            0,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
            )),
        )
        .unwrap();

        assert_eq!(2usize, s.placements.len());
        assert_eq!(2usize, s.images.len());

        // verify the placement is what we expect
        assert!(s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::Internal,
                    id: 0
                },
            })
            .is_some());
        assert!(s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::Internal,
                    id: 1
                },
            })
            .is_some());

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete all placements and images"
    #[test]
    fn delete_all_placements_and_images() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();

        s.dirty = false;
        s.delete(alloc, &mut t, command::Delete::All(true));
        assert!(s.dirty);
        assert_eq!(0usize, s.images.len());
        assert_eq!(0usize, s.placements.len());
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete all placements and images preserves limit"
    #[test]
    fn delete_all_placements_and_images_preserves_limit() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.total_limit = 5000;
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();

        s.dirty = false;
        s.delete(alloc, &mut t, command::Delete::All(true));
        assert!(s.dirty);
        assert_eq!(0usize, s.images.len());
        assert_eq!(0usize, s.placements.len());
        assert_eq!(5000usize, s.total_limit);
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete all placements"
    #[test]
    fn delete_all_placements() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();

        s.dirty = false;
        s.delete(alloc, &mut t, command::Delete::All(false));
        assert!(s.dirty);
        assert_eq!(0usize, s.placements.len());
        assert_eq!(3usize, s.images.len());
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete all placements by image id"
    #[test]
    fn delete_all_placements_by_image_id() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Id {
                delete: false,
                image_id: 2,
                placement_id: 0,
            },
        );
        assert!(s.dirty);
        assert_eq!(1usize, s.placements.len());
        assert_eq!(3usize, s.images.len());
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete all placements by image id and unused images"
    #[test]
    fn delete_all_placements_by_image_id_and_unused_images() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Id {
                delete: true,
                image_id: 2,
                placement_id: 0,
            },
        );
        assert!(s.dirty);
        assert_eq!(1usize, s.placements.len());
        assert_eq!(2usize, s.images.len());
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete placement by specific id"
    #[test]
    fn delete_placement_by_specific_id() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Id {
                delete: true,
                image_id: 1,
                placement_id: 2,
            },
        );
        assert!(s.dirty);
        assert_eq!(2usize, s.placements.len());
        assert_eq!(3usize, s.images.len());
        assert_eq!(
            tracked + 2,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete intersecting cursor"
    #[test]
    fn delete_intersecting_cursor() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 100;
        t.height_px = 100;
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 50,
                height: 50,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                width: 25,
                height: 25,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
            )),
        )
        .unwrap();

        unsafe { (*t.screens.active).cursor_absolute(12, 12) };

        s.dirty = false;
        s.delete(alloc, &mut t, command::Delete::IntersectCursor(false));
        assert!(s.dirty);
        assert_eq!(1usize, s.placements.len());
        assert_eq!(2usize, s.images.len());
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        // verify the placement is what we expect
        assert!(s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 2
                },
            })
            .is_some());

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete intersecting cursor plus unused"
    #[test]
    fn delete_intersecting_cursor_plus_unused() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 100;
        t.height_px = 100;
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 50,
                height: 50,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                width: 25,
                height: 25,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
            )),
        )
        .unwrap();

        unsafe { (*t.screens.active).cursor_absolute(12, 12) };

        s.dirty = false;
        s.delete(alloc, &mut t, command::Delete::IntersectCursor(true));
        assert!(s.dirty);
        assert_eq!(1usize, s.placements.len());
        assert_eq!(2usize, s.images.len());
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        // verify the placement is what we expect
        assert!(s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 2
                },
            })
            .is_some());

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete intersecting cursor hits multiple"
    #[test]
    fn delete_intersecting_cursor_hits_multiple() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 100;
        t.height_px = 100;
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 50,
                height: 50,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                width: 25,
                height: 25,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
            )),
        )
        .unwrap();

        unsafe { (*t.screens.active).cursor_absolute(26, 26) };

        s.dirty = false;
        s.delete(alloc, &mut t, command::Delete::IntersectCursor(true));
        assert!(s.dirty);
        assert_eq!(0usize, s.placements.len());
        assert_eq!(1usize, s.images.len());
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete by column"
    #[test]
    fn delete_by_column() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 100;
        t.height_px = 100;
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 50,
                height: 50,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                width: 25,
                height: 25,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
            )),
        )
        .unwrap();

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Column {
                delete: false,
                x: 60,
            },
        );
        assert!(s.dirty);
        assert_eq!(1usize, s.placements.len());
        assert_eq!(2usize, s.images.len());
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        // verify the placement is what we expect
        assert!(s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1
                },
            })
            .is_some());

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete by column 1x1"
    #[test]
    fn delete_by_column_1x1() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 1,
                height: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            3,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 2, y: 0 }).unwrap(),
            )),
        )
        .unwrap();

        s.delete(
            alloc,
            &mut t,
            command::Delete::Column {
                delete: false,
                x: 2,
            },
        );
        assert_eq!(2usize, s.placements.len());
        assert_eq!(1usize, s.images.len());

        // verify the placement is what we expect
        assert!(s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1
                },
            })
            .is_some());
        assert!(s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 3
                },
            })
            .is_some());

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete by row"
    #[test]
    fn delete_by_row() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 100;
        t.height_px = 100;
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 50,
                height: 50,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                width: 25,
                height: 25,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
            )),
        )
        .unwrap();

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Row {
                delete: false,
                y: 60,
            },
        );
        assert!(s.dirty);
        assert_eq!(1usize, s.placements.len());
        assert_eq!(2usize, s.images.len());
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        // verify the placement is what we expect
        assert!(s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1
                },
            })
            .is_some());

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete by row 1x1"
    #[test]
    fn delete_by_row_1x1() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 1,
                height: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            3,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap(),
            )),
        )
        .unwrap();

        s.delete(
            alloc,
            &mut t,
            command::Delete::Row {
                delete: false,
                y: 2,
            },
        );
        assert_eq!(2usize, s.placements.len());
        assert_eq!(1usize, s.images.len());

        // verify the placement is what we expect
        assert!(s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1
                },
            })
            .is_some());
        assert!(s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 3
                },
            })
            .is_some());

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete images by range 1"
    #[test]
    fn delete_images_by_range_1() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        assert_eq!(3usize, s.images.len());
        assert_eq!(2usize, s.placements.len());

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Range {
                delete: false,
                first: 1,
                last: 2,
            },
        );
        assert!(s.dirty);
        assert_eq!(3usize, s.images.len());
        assert_eq!(0usize, s.placements.len());
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete images by range 2"
    #[test]
    fn delete_images_by_range_2() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        assert_eq!(3usize, s.images.len());
        assert_eq!(2usize, s.placements.len());

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Range {
                delete: true,
                first: 1,
                last: 2,
            },
        );
        assert!(s.dirty);
        assert_eq!(1usize, s.images.len());
        assert_eq!(0usize, s.placements.len());
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete images by range 3"
    #[test]
    fn delete_images_by_range_3() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        assert_eq!(3usize, s.images.len());
        assert_eq!(2usize, s.placements.len());

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Range {
                delete: false,
                first: 1,
                last: 1,
            },
        );
        assert!(s.dirty);
        assert_eq!(3usize, s.images.len());
        assert_eq!(0usize, s.placements.len());
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: delete images by range 4"
    #[test]
    fn delete_images_by_range_4() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        assert_eq!(3usize, s.images.len());
        assert_eq!(2usize, s.placements.len());

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Range {
                delete: true,
                first: 1,
                last: 1,
            },
        );
        assert!(s.dirty);
        assert_eq!(1usize, s.images.len());
        assert_eq!(0usize, s.placements.len());
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: aspect ratio calculation when only columns or rows specified"
    #[test]
    fn aspect_ratio_calculation_when_only_columns_or_rows_specified() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 1000; // 10 px per col
        t.height_px = 2000; // 20 px per row

        // Case 1: Only columns specified
        {
            let image = Image {
                id: 1,
                width: 16,
                height: 9,
                ..Image::default()
            };
            let placement = Placement {
                location: Location::Virtual,
                columns: 10,
                rows: 0,
                ..Placement::with_location(Location::Virtual)
            };

            // Image is 16x9, set to a width of 10 columns, at 10px per column
            // that's 100px width. 100px * (9 / 16) = 56.25, which should round
            // to a height of 56px.

            let calc_size = placement.pixel_size(&image, &t);
            assert_eq!(100u32, calc_size.width);
            assert_eq!(56u32, calc_size.height);
        }

        // Case 2: Only rows specified
        {
            let image = Image {
                id: 2,
                width: 16,
                height: 9,
                ..Image::default()
            };
            let placement = Placement {
                location: Location::Virtual,
                columns: 0,
                rows: 5,
                ..Placement::with_location(Location::Virtual)
            };

            // Image is 16x9, set to a height of 5 rows, at 20px per row that's
            // 100px height. 100px * (16 / 9) = 177.77..., which should round to
            // a width of 178px.

            let calc_size = placement.pixel_size(&image, &t);
            assert_eq!(178u32, calc_size.width);
            assert_eq!(100u32, calc_size.height);
        }

        t.deinit(alloc);
    }

    // test "storage: generation stamps on image add and replace"
    #[test]
    fn storage_generation_stamps_on_image_add_and_replace() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();

        // Fresh storage has generation zero (never mutated).
        assert_eq!(0u64, s.generation);

        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 1,
                height: 1,
                ..Image::default()
            },
        )
        .unwrap();
        let gen1 = s.generation;
        assert!(gen1 > 0);

        let img1 = s.image_by_id(1).unwrap();
        assert_eq!(gen1, img1.generation);

        // A second image gets a strictly greater stamp.
        s.add_image(
            alloc,
            Image {
                id: 2,
                width: 1,
                height: 1,
                ..Image::default()
            },
        )
        .unwrap();
        let gen2 = s.generation;
        assert!(gen2 > gen1);
        assert_eq!(gen2, s.image_by_id(2).unwrap().generation);

        // Retransmitting the same image ID (identical dimensions) gets a
        // fresh stamp: this is what makes same-sized retransmissions
        // detectable by renderers.
        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 1,
                height: 1,
                ..Image::default()
            },
        )
        .unwrap();
        let gen3 = s.generation;
        assert!(gen3 > gen2);
        assert_eq!(gen3, s.image_by_id(1).unwrap().generation);

        // Image 2 kept its stamp.
        assert_eq!(gen2, s.image_by_id(2).unwrap().generation);

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: generation bumps on placement and delete"
    #[test]
    fn storage_generation_bumps_on_placement_and_delete() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        let gen_add = s.generation;

        let pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
        s.add_placement(alloc, 1, 1, Placement::with_location(Location::Pin(pin)))
            .unwrap();
        let gen_place = s.generation;
        assert!(gen_place > gen_add);

        // Reads don't change the generation.
        let _ = s.image_by_id(1);
        let _ = s.image_by_number(1);
        assert_eq!(gen_place, s.generation);

        s.delete(alloc, &mut t, command::Delete::All(true));
        assert!(s.generation > gen_place);

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: generation bumps when setLimit evicts or disables"
    #[test]
    fn storage_generation_bumps_when_set_limit_evicts_or_disables() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();

        s.add_image(
            alloc,
            Image {
                id: 1,
                width: 1,
                height: 1,
                data: b"1234".to_vec(),
                ..Image::default()
            },
        )
        .unwrap();
        let gen_add = s.generation;

        // Lowering the limit evicts the image and must mark a mutation.
        s.dirty = false;
        s.set_limit(alloc, unsafe { &mut *t.screens.active }, 1)
            .unwrap();
        assert!(s.dirty);
        assert!(s.generation > gen_add);
        assert_eq!(0usize, s.images.len());
        let gen_evict = s.generation;

        // Disabling (limit=0) resets the storage and must mark a mutation.
        s.dirty = false;
        s.set_limit(alloc, unsafe { &mut *t.screens.active }, 0)
            .unwrap();
        assert!(s.dirty);
        assert!(s.generation > gen_evict);

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: imageByNumber returns most recently transmitted"
    #[test]
    fn storage_image_by_number_returns_most_recently_transmitted() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();

        // Two images sharing a number: the newest transmission wins,
        // regardless of insertion order or clock resolution.
        s.add_image(
            alloc,
            Image {
                id: 1,
                number: 7,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                number: 7,
                ..Image::default()
            },
        )
        .unwrap();
        assert_eq!(2u32, s.image_by_number(7).unwrap().id);

        // Retransmit the first: it becomes the newest.
        s.add_image(
            alloc,
            Image {
                id: 1,
                number: 7,
                ..Image::default()
            },
        )
        .unwrap();
        assert_eq!(1u32, s.image_by_number(7).unwrap().id);

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: nextGeneration is unique and monotonic"
    #[test]
    fn storage_next_generation_is_unique_and_monotonic() {
        let a = next_generation();
        let b = next_generation();
        assert!(b > a);
        assert!(a > 0);
    }

    // test "storage: no-op delete does not mark a mutation"
    #[test]
    fn storage_no_op_delete_does_not_mark_a_mutation() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();

        // A delete-all on an empty storage (this runs on every screen
        // clear) must not dirty the state or bump the generation.
        s.delete(alloc, &mut t, command::Delete::All(true));
        assert!(!s.dirty);
        assert_eq!(0u64, s.generation);

        // Same for a delete that matches nothing.
        s.add_image(
            alloc,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        let pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
        s.add_placement(alloc, 1, 1, Placement::with_location(Location::Pin(pin)))
            .unwrap();
        let gen = s.generation;
        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Id {
                delete: false,
                image_id: 42,
                placement_id: 0,
            },
        );
        assert!(!s.dirty);
        assert_eq!(gen, s.generation);

        // But a delete that removes something does mark a mutation.
        s.delete(
            alloc,
            &mut t,
            command::Delete::Id {
                delete: false,
                image_id: 1,
                placement_id: 0,
            },
        );
        assert!(s.dirty);
        assert!(s.generation > gen);

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    // test "storage: evict unused transient image"  (oracle a65e11cc9)
    #[test]
    fn storage_evict_unused_transient_image() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage {
            total_limit: 192,
            ..ImageStorage::default()
        };

        s.add_image(
            alloc,
            Image {
                id: 1,
                data: vec![b'*'; 64],
                usage: command::Usage { transient: false },
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 2,
                data: vec![b'*'; 64],
                usage: command::Usage { transient: true },
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            Image {
                id: 3,
                data: vec![b'*'; 64],
                usage: command::Usage { transient: true },
                ..Image::default()
            },
        )
        .unwrap();
        let pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
        s.add_placement(alloc, 2, 1, Placement::with_location(Location::Pin(pin)))
            .unwrap();

        let gen = s.generation;
        let result = s.evict_image(alloc, 32).unwrap();
        assert!(s.dirty);
        assert!(s.generation > gen);
        assert!(result);
        assert_eq!(2, s.images.len());
        assert!(s.images.contains_key(&1));
        assert!(s.images.contains_key(&2));
        assert!(!s.images.contains_key(&3));

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }

    /// NEGATIVE CONTROL for `a65e11cc9`'s re-keyed eviction order — the test
    /// UPSTREAM DOES NOT HAVE (`P3-W6 T5` Step 2 / area D §6).
    ///
    /// Upstream's only new test evicts one image and can be satisfied without
    /// the `used` term ever being consulted. This one pins the FULL block
    /// ordering by evicting every image one at a time and recording the order:
    ///
    ///   block 0: transient + unused   (evicted first)
    ///   block 1: not transient + unused
    ///   block 2: transient + used
    ///   block 3: not transient + used (evicted last)
    ///
    /// Without it, `block` could be computed and never consulted — or consulted
    /// with the two terms swapped — and every other test would still pass.
    #[test]
    fn storage_eviction_order_is_transient_then_unused() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        // Four images, one per block. IDs are assigned so that ID order is the
        // REVERSE of the expected eviction order: if `block` were ignored the
        // tie-break (generation, then id) would produce 1,2,3,4 and the
        // assertion below would fail.
        //
        //   id 1: not transient, used   -> block 3
        //   id 2: transient,     used   -> block 2
        //   id 3: not transient, unused -> block 1
        //   id 4: transient,     unused -> block 0
        let mut s: ImageStorage = ImageStorage {
            total_limit: 4 * 64,
            ..ImageStorage::default()
        };
        for (id, transient) in [(1u32, false), (2, true), (3, false), (4, true)] {
            s.add_image(
                alloc,
                Image {
                    id,
                    data: vec![b'*'; 64],
                    usage: command::Usage { transient },
                    ..Image::default()
                },
            )
            .unwrap();
        }
        // Only 1 and 2 are "used" (have a placement).
        for (i, id) in [1u32, 2].iter().enumerate() {
            let pin = track_pin(&mut t, point::Coordinate { x: 1, y: i as u32 }).unwrap();
            s.add_placement(alloc, *id, 1, Placement::with_location(Location::Pin(pin)))
                .unwrap();
        }

        // Evict one image at a time and record which one went.
        let mut order: Vec<u32> = Vec::new();
        for _ in 0..4 {
            let before: std::collections::BTreeSet<u32> = s.images.keys().copied().collect();
            assert!(s.evict_image(alloc, 1).unwrap());
            let after: std::collections::BTreeSet<u32> = s.images.keys().copied().collect();
            let gone: Vec<u32> = before.difference(&after).copied().collect();
            assert_eq!(
                1,
                gone.len(),
                "exactly one image should be evicted per call"
            );
            order.push(gone[0]);
        }

        assert_eq!(
            vec![4u32, 3, 2, 1],
            order,
            "eviction must go transient+unused, not-transient+unused, \
             transient+used, not-transient+used"
        );

        s.deinit(alloc, unsafe { &mut *t.screens.active });
        t.deinit(alloc);
    }
}
