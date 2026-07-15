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
    pub dirty: bool,

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
            let image_limits = self.image_limits;
            self.deinit(alloc, s);
            *self = ImageStorage {
                image_limits,
                ..ImageStorage::default()
            };
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

        self.dirty = true;
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

        self.dirty = true;
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
                if newest.is_none()
                    || instant_order(kv.transmit_time, newest.as_ref().unwrap().transmit_time)
                        == std::cmp::Ordering::Greater
                {
                    newest = Some(kv.clone());
                }
            }
        }

        newest
    }

    /// Delete placements, images.
    pub fn delete(&mut self, alloc: Allocator, t: &mut Terminal, cmd: command::Delete) {
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

                self.dirty = true;
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
                    None => return,
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
                    return;
                }

                // .x = std.math.cast(size.CellCountInt, v.x - 1) orelse break :intersect_cell,
                let cx = match size::CellCountInt::try_from(x - 1) {
                    Ok(v) => v,
                    Err(_) => return,
                };
                // .y = std.math.cast(size.CellCountInt, v.y - 1) orelse break :intersect_cell,
                // PORT NOTE: the reference casts `v.y - 1` to `size.CellCountInt`
                // (returning on overflow); `point.Coordinate.y` is `u32` in this
                // port, so we range-check against `CellCountInt` first, then widen.
                let cy = match size::CellCountInt::try_from(y - 1) {
                    Ok(v) => v as u32,
                    Err(_) => return,
                };
                let p = point::Point::Active(point::Coordinate { x: cx, y: cy });
                self.delete_intersecting(alloc, t, p, delete, |_p| true);
            }

            #[allow(clippy::absurd_extreme_comparisons)]
            command::Delete::IntersectCellZ { delete, x, y, z } => {
                if x <= 0 || y <= 0 {
                    // log.warn("delete intersect cell coords must be at least 1", .{});
                    return;
                }

                // .x = std.math.cast(size.CellCountInt, v.x - 1) orelse break :intersect_cell_z,
                let cx = match size::CellCountInt::try_from(x - 1) {
                    Ok(v) => v,
                    Err(_) => return,
                };
                // .y = std.math.cast(size.CellCountInt, v.y - 1) orelse break :intersect_cell_z,
                let cy = match size::CellCountInt::try_from(y - 1) {
                    Ok(v) => v as u32,
                    Err(_) => return,
                };
                let p = point::Point::Active(point::Coordinate { x: cx, y: cy });
                // struct { fn filter(ctx: i32, p: Placement) bool { return p.z == ctx; } }.filter
                self.delete_intersecting(alloc, t, p, delete, move |placement| placement.z == z);
            }

            #[allow(clippy::absurd_extreme_comparisons)]
            command::Delete::Column { delete, x } => {
                if x <= 0 {
                    // log.warn("delete column must be greater than zero", .{});
                    return;
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

                // Mark dirty to force redraw
                self.dirty = true;
            }

            #[allow(clippy::absurd_extreme_comparisons)]
            command::Delete::Row { delete, y } => {
                if y <= 0 {
                    // log.warn("delete row must be greater than zero", .{});
                    return;
                }

                // v.y is in active coords so we want to convert it to a pin
                // so we can compare by page offsets.
                let ty = match size::CellCountInt::try_from(y - 1) {
                    Ok(v) => v,
                    Err(_) => return,
                };
                let target_pin = match unsafe { &(*t.screens.active).pages }.pin(
                    point::Point::Active(point::Coordinate { x: 0, y: ty as u32 }),
                ) {
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

                // Mark dirty to force redraw
                self.dirty = true;
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

                // Mark dirty to force redraw
                self.dirty = true;
            }

            #[allow(clippy::absurd_extreme_comparisons)]
            command::Delete::Range {
                delete,
                first,
                last,
            } => {
                if first <= 0 || last <= 0 {
                    // log.warn("delete range values must be greater than zero", .{});
                    return;
                }
                if first > last {
                    // log.warn("delete range 'x' ({}) must be less than or equal to 'y' ({})", .{ v.first, v.last });
                    return;
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

                // Mark dirty to force redraw
                self.dirty = true;
            }

            // We don't support animation frames yet so they are successfully
            // deleted!
            command::Delete::AnimationFrames(_) => {}
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

        // Mark dirty to force redraw
        self.dirty = true;
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

        // Mark dirty to force redraw
        self.dirty = true;
    }

    /// Evict image to make space. This will evict the oldest image,
    /// prioritizing unused images first, as recommended by the published
    /// Kitty spec.
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
        let mut candidates: Vec<Candidate> = Vec::new();

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

            // try candidates.append(alloc, .{ ... });
            candidates.push(Candidate {
                id: img.id,
                time: img.transmit_time,
                used,
            });
        }

        // Sort
        // std.mem.sortUnstable(Candidate, candidates.items, {}, struct { fn lessThan(...) }.lessThan);
        candidates.sort_unstable_by(|lhs, rhs| {
            // PORT NOTE: the reference `lessThan` returns `bool`; we port it to a
            // total `Ordering` comparator. `lessThan(lhs, rhs)` true ⇒ `Less`.

            // If they're usage matches, then its based on time.
            if lhs.used == rhs.used {
                return match instant_order(lhs.time, rhs.time) {
                    std::cmp::Ordering::Less => std::cmp::Ordering::Less,
                    std::cmp::Ordering::Greater => std::cmp::Ordering::Greater,
                    std::cmp::Ordering::Equal => lhs.id.cmp(&rhs.id),
                };
            }

            // If not used, then its a better candidate
            // return !lhs.used;  (true ⇒ Less)
            if !lhs.used {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });

        // They're in order of best to evict.
        let mut evicted: usize = 0;
        for c in &candidates {
            // Delete all the placements for this image and the image.
            let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
            for key in keys {
                if key.image_id == c.id {
                    self.placements.remove(&key);
                }
            }

            // if (self.images.getEntry(c.id)) |entry| { ... }
            if let Some(entry) = self.images.get(&c.id) {
                // log.info("evicting image id={} bytes={}", .{ c.id, entry.value_ptr.data.len });

                evicted += entry.data.len();
                self.total_bytes -= entry.data.len();

                // entry.value_ptr.deinit(alloc); — dropping the removed Image frees data.
                self.images.remove(&c.id);

                if evicted > req {
                    return Ok(true);
                }
            }
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
    // PORT NOTE: `time: std.time.Instant`. `Image.transmit_time` is modeled as
    // `Option<Instant>` (see graphics_image), so we carry the same `Option`.
    time: Option<std::time::Instant>,
    used: bool,
}

/// `std.time.Instant.order(self, other)` over the `Option<Instant>` modeling of
/// `transmit_time`. Mirrors the reference comparison where `transmit_time` is a
/// monotonic `Instant`; `None` (the "not yet completed", reference `undefined`)
/// orders consistently and deterministically below any `Some`.
fn instant_order(
    a: Option<std::time::Instant>,
    b: Option<std::time::Instant>,
) -> std::cmp::Ordering {
    a.cmp(&b)
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
    // relying on the reference `Terminal.Options` field defaults (`max_scrollback`,
    // `colors = .default`, `default_modes = .{}`, kitty limits). Rust `Options` has
    // no field defaults, so this helper supplies them verbatim, mirroring the
    // reference `Options` declaration (see terminal.rs's test `opts` helper).
    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> crate::terminal::Options {
        crate::terminal::Options {
            cols,
            rows,
            max_scrollback: 10_000,
            colors: crate::terminal::Colors::default(),
            default_modes: crate::modes::ModePacked::default(),
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
}
