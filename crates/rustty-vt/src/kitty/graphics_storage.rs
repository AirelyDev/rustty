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
use crate::kitty::graphics_image;
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
/// ImageStorage.generation and Image.generation). This is global rather
/// than per-storage so that stamps are unique across every storage in
/// the process: two mutation events never produce the same value, even
/// across separate screens (main vs. alt), storage resets, or separate
/// terminals. This lets consumers use a generation value alone as a
/// cache key without any ambiguity.
///
/// Thread-safe because separate terminals may mutate their storages
/// from different threads. On single-threaded targets this lowers to
/// plain operations.
//
// The reference names the backing type `GenerationCounter` and documents it:
//
//   Backing implementation for the generation counter. We use a
//   lock-free atomic counter where we can, but not all targets support
//   64-bit atomic operations (e.g. 32-bit ARM Android), so we fall back
//   to a mutex-protected counter on those. This is a cold path (only
//   invoked on content mutations) so the mutex cost is irrelevant.
//
//   The pointer-width check is a conservative proxy for 64-bit atomic
//   support: every 64-bit target supports 64-bit atomics, while 32-bit
//   targets may not (per the compiler's atomic operand validation).
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

    /// Generation stamp of the last content mutation to this storage:
    /// any image transmit/replace, placement add, or delete of either.
    /// Zero means the storage has never been mutated (and is therefore
    /// empty).
    ///
    /// Unlike dirty, this is NOT updated by scrolling/resizing, so an
    /// unchanged generation means the placement set and all image data
    /// are identical; only placement geometry (pins) may have moved.
    /// Values come from a process-global monotonic counter, so a value
    /// observed from any storage never recurs for different content,
    /// even across screen switches or storage resets.
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

/// Identifies one exact pending image transmission. The generation is
/// assigned by this storage when the pending image is inserted, so a
/// later replacement of the same ID cannot consume stale payload bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingImage {
    pub id: u32,
    pub generation: u64,
}

impl PendingImage {
    /// Attach owned decoded bytes if this exact pending transmission is
    /// still resident. On true, storage owns `data`; on false, the caller
    /// retains ownership. Completion preserves the image generation (and
    /// therefore eviction age) while marking storage content as mutated.
    //
    // PORT NOTE: the reference takes `data: []const u8` and the caller frees it
    // on `false`. Rust ownership transfers the `Vec<u8>` either way: on `true`
    // it becomes the image payload; on `false` it is dropped (freed) here —
    // observably the caller's `defer alloc.free(data)`.
    pub fn complete(&self, storage: &mut ImageStorage, data: Vec<u8>) -> bool {
        // const img = storage.images.getPtr(self.id) orelse return false;
        let img = match storage.images.get_mut(&self.id) {
            Some(img) => img,
            None => return false,
        };
        if img.generation != self.generation {
            return false;
        }

        let expected_len = match img.data {
            graphics_image::Data::Complete(_) => return false,
            graphics_image::Data::Pending(len) => len,
        };
        if data.len() != expected_len {
            return false;
        }

        img.data = graphics_image::Data::Complete(data);
        storage.mark_mutated();
        true
    }
}

impl ImageStorage {
    // PORT NOTE (`s: *terminal.Screen` params, this range's pin-lifecycle
    // series): upstream threads the owning screen into
    // `deinit`/`setLimit`/`addImage`/`addPendingImage`/`addPlacement`/eviction
    // so replaced/evicted/pruned placements can release their tracked pins.
    // This storage lives INSIDE `Screen` (`screen.kitty_images`), so a
    // `&mut Screen` parameter would alias the `&mut self` receiver whenever the
    // caller's storage has not been moved out — instant Stacked-Borrows UB
    // (memory `rust-mut-args-strongly-protected`). The parameter is therefore a
    // RAW `*mut Screen`, following `delete`'s existing sanctioned shape, and is
    // dereferenced only inside narrow scopes (`Placement::deinit` →
    // `(*s).pages.untrack_pin`, garbage-pin reads) that never overlap a `&mut`
    // reconstruction of the whole screen.
    pub fn deinit(&mut self, alloc: Allocator, s: *mut Screen) {
        // if (self.loading) |loading| loading.destroy(alloc);
        // PORT NOTE: `LoadingImage.destroy` frees its data + the heap allocation;
        // dropping the owning `Box<LoadingImage>` does both.
        let _ = &alloc;
        self.loading = None;

        self.clear_placements(s);
        // self.placements.deinit(alloc);
        self.placements = PlacementMap::new();

        // var it = self.images.iterator();
        // while (it.next()) |kv| kv.value_ptr.deinit(alloc);
        // self.images.deinit(alloc);
        // PORT NOTE: `Image.deinit` frees its `data`; clearing the owning map
        // drops every `Image` (and its payload).
        self.images.clear();
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
    // PORT NOTE (`4371871bc`): eviction no longer allocates, so `setLimit` is
    // infallible (`!void` → `void`).
    pub fn set_limit(&mut self, alloc: Allocator, s: *mut Screen, limit: usize) {
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
            if !self.evict_image(alloc, s, req_bytes) {
                // log.warn("failed to evict enough images for required bytes", .{});
            }
        }

        self.total_limit = limit;
    }

    /// Add an image to the storage. This will automatically free any existing
    /// image with the same ID. Prefer addPendingImage for pending data so the
    /// caller receives a completion token.
    // PORT NOTE: `Allocator.Error!void` → `Result<(), OutOfMemory>`.
    pub fn add_image(
        &mut self,
        alloc: Allocator,
        s: *mut Screen,
        img: Image,
    ) -> Result<(), OutOfMemory> {
        let new_len = img.data.len();

        // If the image itself is over the limit, then error immediately
        if new_len > self.total_limit {
            return Err(OutOfMemory);
        }

        // Credit an existing image's reservation before calculating the
        // replacement size. In particular, a completed live transmission
        // replacing pending snapshot metadata must be able to reuse the
        // reservation without evicting its own ID.
        let old_len = if let Some(old) = self.images.get(&img.id) {
            old.data.len()
        } else {
            0
        };
        assert(old_len <= self.total_bytes);

        // Reserve map capacity before evicting anything so allocation failure
        // cannot leave storage partially evicted. Existing keys need no room.
        // PORT NOTE: `ensureUnusedCapacity` on a Rust `HashMap` is infallible
        // (it grows via the global allocator); the reserve keeps the shape.
        if !self.images.contains_key(&img.id) {
            self.images.reserve(1);
        }

        // If this would put us over the limit, then evict.
        let total_bytes = self.total_bytes - old_len + new_len;
        if total_bytes > self.total_limit {
            let req_bytes = total_bytes - self.total_limit;
            // log.info("evicting images to make space for {} bytes", .{req_bytes});
            if !self.evict_image_except(alloc, s, req_bytes, Some(img.id)) {
                // log.warn("failed to evict enough images for required bytes", .{});
                return Err(OutOfMemory);
            }
        }

        // const gop = self.images.getOrPutAssumeCapacity(img.id);

        // log.debug("addImage image={}", .{img.withoutData()});

        // Retransmitting a specific image ID replaces the old image and all
        // of its placements, as required by the Kitty graphics protocol.
        // if (gop.found_existing) {
        //     self.removePlacementsByImageId(s, img.id);
        //     self.total_bytes -= gop.value_ptr.data.len();
        //     gop.value_ptr.deinit(alloc);
        // }
        let id = img.id;
        if self.images.contains_key(&id) {
            self.remove_placements_by_image_id(s, id);
            self.total_bytes -= self.images.get(&id).unwrap().data.len();
            // the existing `Image` (and its payload) is dropped by the
            // `insert` below.
        }
        let _ = &alloc;

        // gop.value_ptr.* = img;
        // gop.value_ptr.metadata.placement_count = 0;
        self.images.insert(id, img);
        self.images.get_mut(&id).unwrap().metadata.placement_count = 0;
        self.total_bytes += new_len;

        // Stamp the stored image with a fresh generation. This gives
        // every add/replace a unique stamp even when the same image ID
        // is retransmitted with identical dimensions, so consumers
        // (e.g. renderer texture caches) can detect content changes.
        self.mark_mutated();
        let generation = self.generation;
        self.images.get_mut(&id).unwrap().generation = generation;
        Ok(())
    }

    /// Add an image whose decoded payload bytes have not arrived yet.
    /// Returns the token required to complete this exact transmission.
    pub fn add_pending_image(
        &mut self,
        alloc: Allocator,
        s: *mut Screen,
        img: Image,
    ) -> Result<PendingImage, OutOfMemory> {
        assert(img.data.is_pending());
        // PORT NOTE: `img` moves into `addImage`; capture the id first (the
        // reference re-reads `img.id` from its by-value copy).
        let id = img.id;
        self.add_image(alloc, s, img)?;
        // const stored = self.images.get(img.id).?;
        let stored = self.images.get(&id).unwrap();
        Ok(PendingImage {
            id: stored.id,
            generation: stored.generation,
        })
    }

    /// Add a placement for a given image. The caller must verify in advance
    /// the image exists to prevent memory corruption. On success, storage
    /// owns `p`; on error, the caller retains ownership. The screen must own
    /// any tracked pin in `p` and in the placement it replaces.
    pub fn add_placement(
        &mut self,
        alloc: Allocator,
        s: *mut Screen,
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

        // Tracked pins are marked garbage when their underlying history is
        // pruned. Kitty removes placements once they scroll out of retained
        // history, so reclaim those placements before growing the map for a
        // new one. If allocation below fails, the sweep is still a content
        // mutation and must be visible to consumers.
        // const removed_garbage = self.removeGarbagePlacements(s);
        // errdefer if (removed_garbage) self.markMutated(io);
        // PORT NOTE: the `errdefer` fires only on the error return below; Rust
        // marks the mutation inline on that path.
        let removed_garbage = self.remove_garbage_placements(s);

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
        // if (gop.found_existing) {
        //     gop.value_ptr.deinit(s);
        // } else { ... placement_count overflow check ... }
        // gop.value_ptr.* = p;
        if let Some(existing) = self.placements.get(&key) {
            existing.deinit(s);
        } else {
            let img = self.images.get_mut(&image_id).unwrap();
            // img.metadata.placement_count = std.math.add(
            //     @TypeOf(img.metadata.placement_count),
            //     img.metadata.placement_count,
            //     1,
            // ) catch {
            //     self.placements.removeByPtr(gop.key_ptr);
            //     return error.OutOfMemory;
            // };
            // PORT NOTE: the counter is a `u30` upstream; the checked add is
            // bounded by `Metadata::PLACEMENT_COUNT_MAX` (`maxInt(u30)`). The
            // reference removes the freshly-created gop slot on overflow; here
            // nothing was inserted yet, so there is nothing to remove.
            match img.metadata.placement_count.checked_add(1) {
                Some(count) if count <= graphics_image::Metadata::PLACEMENT_COUNT_MAX => {
                    img.metadata.placement_count = count;
                }
                _ => {
                    if removed_garbage {
                        self.mark_mutated();
                    }
                    return Err(OutOfMemory);
                }
            }
        }
        self.placements.insert(key, p);

        self.mark_mutated();
        Ok(())
    }

    /// Remove pin-backed placements whose tracked content has been pruned.
    /// Virtual placements have no tracked screen location and are retained.
    fn remove_garbage_placements(&mut self, s: *mut Screen) -> bool {
        let mut removed = false;
        // var it = self.placements.iterator();
        // while (it.next()) |entry| { ... }
        let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
        for key in keys {
            let pin = match self.placements.get(&key).unwrap().location {
                Location::Pin(pin) => pin,
                Location::Virtual => continue,
            };
            // if (!pin.garbage) continue;
            // SAFETY(raw-page-aliasing/graphics_storage): `pin` is a tracked
            // pin owned by the screen's pagelist; no `&mut` to the pin or its
            // pagelist is live across this read.
            if !unsafe { (*pin).garbage } {
                continue;
            }

            self.placements.get(&key).unwrap().deinit(s);
            self.remove_placement_by_key(&key);
            removed = true;
        }

        removed
    }

    fn clear_placements(&mut self, s: *mut Screen) {
        // var it = self.placements.iterator();
        // while (it.next()) |entry| entry.value_ptr.deinit(s);
        for entry in self.placements.values() {
            entry.deinit(s);
        }
        self.placements.clear(); // clearRetainingCapacity
    }

    fn remove_placements_by_image_id(&mut self, s: *mut Screen, image_id: u32) {
        // var it = self.placements.iterator();
        // while (it.next()) |entry| { ... }
        let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
        for key in keys {
            if key.image_id != image_id {
                continue;
            }
            self.placements.get(&key).unwrap().deinit(s);
            self.remove_placement_by_key(&key);
        }
    }

    // fn removePlacementByPtr(self: *ImageStorage, key: *PlacementKey) void
    // PORT NOTE: Zig removes by entry pointer during iteration; the collect-
    // keys-first shape removes by key value (observably identical, see the
    // hash-map PORT NOTE on the struct).
    fn remove_placement_by_key(&mut self, key: &PlacementKey) {
        let img = self.images.get_mut(&key.image_id).unwrap();
        assert(img.metadata.placement_count > 0);
        img.metadata.placement_count -= 1;
        self.placements.remove(key);
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
        // before and after tells us whether anything actually changed.
        // Only then do we mark a mutation. This matters because a
        // delete-all runs on every screen clear (e.g. `ESC [ 2 J`), and
        // we don't want empty clears to dirty the image state or bump
        // the generation.
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
                        self.placements.get(&key).unwrap().deinit(t.screens.active);
                        self.remove_placement_by_key(&key);
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
                    self.delete_by_id(alloc, t.screens.active, img.id, placement_id, delete);
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
                            self.placements.get(&key).unwrap().deinit(t.screens.active);
                            self.remove_placement_by_key(&key);
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
                            self.placements.get(&key).unwrap().deinit(t.screens.active);
                            self.remove_placement_by_key(&key);
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
                            self.placements.get(&key).unwrap().deinit(t.screens.active);
                            self.remove_placement_by_key(&key);
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
                        if key.image_id >= first && key.image_id <= last {
                            let image_id = key.image_id;
                            self.placements.get(&key).unwrap().deinit(t.screens.active);
                            self.remove_placement_by_key(&key);
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
        s: *mut Screen,
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
                    self.remove_placement_by_key(&key);
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
                self.remove_placement_by_key(&key);
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
        // const entry = self.images.getEntry(image_id) orelse return;
        // if (entry.value_ptr.metadata.placement_count > 0) return;
        let _ = &alloc;
        let entry = match self.images.get(&image_id) {
            Some(entry) => entry,
            None => return,
        };
        if entry.metadata.placement_count > 0 {
            return;
        }

        self.total_bytes -= entry.data.len();
        // entry.value_ptr.deinit(alloc); — dropping the removed Image frees data.
        self.images.remove(&image_id);
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
            // if (rect.contains(target_pin)) { ... }
            if rect.contains(target_pin) {
                // if (filter) |f| if (!f(filter_ctx, entry.value_ptr.*)) continue;
                if !filter(self.placements.get(&key).unwrap()) {
                    continue;
                }
                self.placements.get(&key).unwrap().deinit(t.screens.active);
                self.remove_placement_by_key(&key);
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
    fn evict_image(&mut self, alloc: Allocator, s: *mut Screen, req: usize) -> bool {
        self.evict_image_except(alloc, s, req, None)
    }

    /// Evict images while preserving `exclude_id`, used when replacing an
    /// existing image whose old reservation has already been credited.
    fn evict_image_except(
        &mut self,
        alloc: Allocator,
        s: *mut Screen,
        req: usize,
        exclude_id: Option<u32>,
    ) -> bool {
        assert(req <= self.total_limit);
        let _ = &alloc;

        // Evicting anything is a content mutation. This matters for the
        // setLimit path in particular, which doesn't otherwise mark it.
        //
        // PORT NOTE: Zig `defer if (self.images.count() != images_before)
        // self.markMutated(io);`. Rust has no `defer`, so the count is checked
        // at BOTH exits: the `return false` inside the loop and the trailing
        // `return true`.
        let images_before = self.images.len();

        let mut evicted: usize = 0;
        while evicted < req {
            // const c = candidate: { ... };
            let c = {
                let mut best: Option<Candidate> = None;
                // var it = self.images.iterator();
                for img in self.images.values() {
                    // if (exclude_id != null and img.id == exclude_id.?) continue;
                    if exclude_id.is_some() && img.id == exclude_id.unwrap() {
                        continue;
                    }

                    let current = Candidate::init(img);
                    if best.is_none() || current.less_than(best.unwrap()) {
                        best = Some(current);
                    }
                }
                // break :candidate best orelse return false;
                match best {
                    Some(best) => best,
                    None => {
                        if self.images.len() != images_before {
                            self.mark_mutated();
                        }
                        return false;
                    }
                }
            };

            // var p_it = self.placements.iterator();
            // while (p_it.next()) |entry| { ... }
            let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
            for key in keys {
                if key.image_id == c.id {
                    self.placements.get(&key).unwrap().deinit(s);
                    self.remove_placement_by_key(&key);
                }
            }

            // const entry = self.images.getEntry(c.id).?;
            let image_len = self.images.get(&c.id).unwrap().data.len();
            // log.info("evicting image id={} bytes={}", .{ c.id, image_len });

            evicted += image_len;
            self.total_bytes -= image_len;

            // entry.value_ptr.deinit(alloc); — dropping the removed Image frees data.
            self.images.remove(&c.id);
        }

        if self.images.len() != images_before {
            self.mark_mutated();
        }
        true
    }
}

/// The allocation-failure error, mirroring Zig's `Allocator.Error`/OutOfMemory.
//
// PORT NOTE: the reference's fallible functions return `Allocator.Error!T`; in
// Rust this is `Result<T, OutOfMemory>`. Rust's `HashMap`/`Vec` own their
// storage and never surface OOM through these APIs; the reachable producers
// are `add_image` (eviction cannot free enough) and `add_placement` (the u30
// placement-count overflow, `error.OutOfMemory` upstream).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

/// The candidate type used during eviction (`evictImageExcept`).
//
// PORT NOTE: the reference declares this as a function-local `const Candidate =
// struct {...}`. Rust has no local-to-method struct visible across closures,
// so it is hoisted to a private module-level struct with identical fields.
#[derive(Clone, Copy)]
struct Candidate {
    id: u32,
    generation: u64,

    // Lower values are evicted first:
    // 0: transient, unused
    // 1: not transient, unused
    // 2: transient, used
    // 3: not transient, used
    //
    // PORT NOTE: the reference field is `priority: u2`. Rust has no `u2`; `u8`
    // is the narrowest carrier. The value is only ever compared, never
    // bit-packed, so the widening is unobservable — and `init` can only
    // produce 0..=3.
    priority: u8,
}

impl Candidate {
    fn init(img: &Image) -> Candidate {
        Candidate {
            id: img.id,
            generation: img.generation,
            priority: (if img.metadata.transient { 0u8 } else { 1u8 })
                + (if img.metadata.placement_count > 0 {
                    2u8
                } else {
                    0u8
                }),
        }
    }

    fn less_than(self, rhs: Candidate) -> bool {
        if self.priority != rhs.priority {
            return self.priority < rhs.priority;
        }
        if self.generation != rhs.generation {
            return self.generation < rhs.generation;
        }
        self.id < rhs.id
    }
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

    pub fn deinit(&self, s: *mut Screen) {
        match self.location {
            // .pin => |p| s.pages.untrackPin(p),
            // SAFETY(raw-page-aliasing/graphics_storage): `s` is the screen
            // owning this placement's tracked pin (the caller's contract, per
            // the `s: *mut Screen` PORT NOTE on `ImageStorage`); the `&mut`
            // projection of its `pages` field is scoped to this one call and
            // never overlaps a live borrow of `kitty_images`.
            Location::Pin(p) => unsafe { (*s).pages.untrack_pin(p) },
            // .virtual => {},
            Location::Virtual => {}
        }
    }

    /// Multiply two protocol-controlled values without allowing them to
    /// wrap. Placement geometry is exposed as u32, so values larger than
    /// that are represented by the largest possible value.
    // fn saturatingMul(lhs: u32, rhs: u32) u32 {
    //     return std.math.mul(u32, lhs, rhs) catch std.math.maxInt(u32);
    // }
    fn saturating_mul(lhs: u32, rhs: u32) -> u32 {
        lhs.saturating_mul(rhs)
    }

    /// Scale a dimension by an aspect ratio and round to the nearest
    /// integer. The u64 intermediate can hold the product of two u32s as
    /// well as the rounding adjustment.
    fn scale_dimension(value: u32, numerator: u32, denominator: u32) -> u32 {
        if denominator == 0 {
            return 0;
        }

        // const rounded = (@as(u64, value) * @as(u64, numerator) +
        //     @as(u64, denominator) / 2) / @as(u64, denominator);
        let rounded =
            (value as u64 * numerator as u64 + denominator as u64 / 2) / denominator as u64;
        // return std.math.cast(u32, rounded) orelse std.math.maxInt(u32);
        u32::try_from(rounded).unwrap_or(u32::MAX)
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

        // If we have a specified cols AND rows then we calculate
        // the width and height from them directly, we don't need
        // to adjust for aspect ratio.
        if self.columns > 0 && self.rows > 0 {
            let calc_width = Self::saturating_mul(cell_width, self.columns);
            let calc_height = Self::saturating_mul(cell_height, self.rows);

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
            let calc_width = Self::saturating_mul(cell_width, self.columns);
            let calc_height = Self::scale_dimension(calc_width, height, width);

            return PixelSize {
                width: calc_width,
                height: calc_height,
            };
        }

        // Otherwise, only the rows were specified, so we
        // determine the width based on the aspect ratio.
        {
            let calc_height = Self::saturating_mul(cell_height, self.rows);
            let calc_width = Self::scale_dimension(calc_height, width, height);

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
        // PORT NOTE (R7): the numerators are `+|` upstream (`e5840bb9b` — the
        // offsets are protocol-controlled and MAY saturate by design; the
        // untrusted-dimensions test drives them at `maxInt(u32)`), so this is
        // `saturating_add` without a no-overflow debug_assert.
        let calc_size = self.pixel_size(image, t);
        GridSize {
            // std.math.divCeil(u32, calc_size.width +| self.x_offset, t.width_px / t.cols) catch 0,
            cols: div_ceil_u32(
                calc_size.width.saturating_add(self.x_offset),
                t.width_px / t.cols as u32,
            )
            .unwrap_or(0),
            // std.math.divCeil(u32, calc_size.height +| self.y_offset, t.height_px / t.rows) catch 0,
            rows: div_ceil_u32(
                calc_size.height.saturating_add(self.y_offset),
                t.height_px / t.rows as u32,
            )
            .unwrap_or(0),
        }
        // NOTE: Above `divCeil`s can only fail if the cell size is 0,
        //       in such a case it seems safe to return 0 for this.
    }

    /// Returns a selection of the entire rectangle this placement
    /// occupies within the screen. This can return null for a virtual
    /// placement or when unavailable pixel geometry makes it empty.
    pub fn rect(&self, image: &Image, t: &Terminal) -> Option<Rect> {
        let grid_size = self.grid_size(image, t);
        let pin = match self.location {
            Location::Pin(p) => p,
            Location::Virtual => return None,
        };
        // if (pin.garbage) return null;
        // SAFETY(raw-page-aliasing/graphics_storage): `pin` is a tracked pin
        // owned by the screen's pagelist; only shared reads are performed and
        // no `&mut` to the pin is live across them.
        let pin_ref: &PageList::Pin = unsafe { &*pin };
        if pin_ref.garbage {
            return None;
        }

        // A zero pixel-sized placement can produce a zero grid size when
        // pixel geometry is unavailable. It occupies no rectangle.
        if grid_size.cols == 0 || grid_size.rows == 0 {
            return None;
        }

        // var br = switch (pin.downOverflow(grid_size.rows - 1)) {
        //     .offset => |v| v,
        //     .overflow => |v| v.end,
        // };
        let mut br = match pin_ref.down_overflow((grid_size.rows - 1) as usize) {
            PageList::pin::PinOverflow::Offset(v) => v,
            PageList::pin::PinOverflow::Overflow { end, .. } => end,
        };
        // br.x = @intCast(@min(
        //     @as(u32, pin.x) +| (grid_size.cols - 1),
        //     @as(u32, t.cols) - 1,
        // ));
        br.x = size::CellCountInt::try_from(core::cmp::min(
            // We need to sub one here because the x value is
            // one width already. So if the image is width "1"
            // then we add zero to X because X itself is width 1.
            (pin_ref.x as u32).saturating_add(grid_size.cols - 1),
            t.cols as u32 - 1,
        ))
        .unwrap();

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
    use crate::kitty::graphics_image::{Data, Metadata};
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
            t.screens.active,
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
            t.screens.active,
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
            t.screens.active,
            1,
            0,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 25, y: 25 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
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
            t.screens.active,
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
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
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
            t.screens.active,
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
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
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
            t.screens.active,
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
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
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
            t.screens.active,
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
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
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
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
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
            t.screens.active,
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
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
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
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            3,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        assert_eq!(3usize, s.images.len());
        assert_eq!(3usize, s.placements.len());

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Range {
                delete: false,
                first: 2,
                last: 2,
            },
        );
        assert!(s.dirty);
        assert_eq!(3usize, s.images.len());
        assert_eq!(2usize, s.placements.len());
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
                image_id: 3,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1
                },
            })
            .is_some());
        assert_eq!(
            tracked + 2,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            3,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        assert_eq!(3usize, s.images.len());
        assert_eq!(3usize, s.placements.len());

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::Delete::Range {
                delete: true,
                first: 2,
                last: 2,
            },
        );
        assert!(s.dirty);
        assert_eq!(2usize, s.images.len());
        assert_eq!(2usize, s.placements.len());
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
                image_id: 3,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1
                },
            })
            .is_some());
        assert_eq!(
            tracked + 2,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
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
            t.screens.active,
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
            t.screens.active,
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

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        let gen_add = s.generation;

        let pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(pin)),
        )
        .unwrap();
        let gen_place = s.generation;
        assert!(gen_place > gen_add);

        // Reads don't change the generation.
        let _ = s.image_by_id(1);
        let _ = s.image_by_number(1);
        assert_eq!(gen_place, s.generation);

        s.delete(alloc, &mut t, command::Delete::All(true));
        assert!(s.generation > gen_place);

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                width: 1,
                height: 1,
                data: Data::Complete(b"1234".to_vec()),
                ..Image::default()
            },
        )
        .unwrap();
        let gen_add = s.generation;

        // Lowering the limit evicts the image and must mark a mutation.
        s.dirty = false;
        s.set_limit(alloc, t.screens.active, 1);
        assert!(s.dirty);
        assert!(s.generation > gen_add);
        assert_eq!(0usize, s.images.len());
        let gen_evict = s.generation;

        // Disabling (limit=0) resets the storage and must mark a mutation.
        s.dirty = false;
        s.set_limit(alloc, t.screens.active, 0);
        assert!(s.dirty);
        assert!(s.generation > gen_evict);

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                number: 7,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
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
            t.screens.active,
            Image {
                id: 1,
                number: 7,
                ..Image::default()
            },
        )
        .unwrap();
        assert_eq!(1u32, s.image_by_number(7).unwrap().id);

        s.deinit(alloc, t.screens.active);
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
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        let pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(pin)),
        )
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

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: evicts images in priority order"
    // (supersedes a65e11cc9's "storage: evict unused transient image")
    #[test]
    fn storage_evicts_images_in_priority_order() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage {
            total_limit: 192,
            ..ImageStorage::default()
        };

        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                data: Data::Complete(vec![b'*'; 64]),
                metadata: Metadata {
                    transient: false,
                    ..Metadata::default()
                },
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                data: Data::Complete(vec![b'*'; 64]),
                metadata: Metadata {
                    transient: true,
                    ..Metadata::default()
                },
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                data: Data::Complete(vec![b'*'; 64]),
                metadata: Metadata {
                    transient: true,
                    ..Metadata::default()
                },
                ..Image::default()
            },
        )
        .unwrap();
        let pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            2,
            1,
            Placement::with_location(Location::Pin(pin)),
        )
        .unwrap();

        let gen = s.generation;
        let result = s.evict_image(alloc, t.screens.active, 96);
        assert!(s.dirty);
        assert!(s.generation > gen);
        assert!(result);
        assert_eq!(1, s.images.len());
        assert!(!s.images.contains_key(&1));
        assert!(s.images.contains_key(&2));
        assert!(!s.images.contains_key(&3));

        s.deinit(alloc, t.screens.active);
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
                t.screens.active,
                Image {
                    id,
                    data: Data::Complete(vec![b'*'; 64]),
                    metadata: Metadata {
                        transient,
                        ..Metadata::default()
                    },
                    ..Image::default()
                },
            )
            .unwrap();
        }
        // Only 1 and 2 are "used" (have a placement).
        for (i, id) in [1u32, 2].iter().enumerate() {
            let pin = track_pin(&mut t, point::Coordinate { x: 1, y: i as u32 }).unwrap();
            s.add_placement(
                alloc,
                t.screens.active,
                *id,
                1,
                Placement::with_location(Location::Pin(pin)),
            )
            .unwrap();
        }

        // Evict one image at a time and record which one went.
        let mut order: Vec<u32> = Vec::new();
        for _ in 0..4 {
            let before: std::collections::BTreeSet<u32> = s.images.keys().copied().collect();
            assert!(s.evict_image(alloc, t.screens.active, 1));
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

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: replacing placement releases tracked pin"
    #[test]
    fn storage_replacing_placement_releases_tracked_pin() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();

        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        let replacement_pin = track_pin(&mut t, point::Coordinate { x: 2, y: 2 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(replacement_pin)),
        )
        .unwrap();
        assert_eq!(1usize, s.placements.len());
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );
        // try testing.expectEqual(replacement_pin, s.placements.get(...).?.location.pin);
        match s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1,
                },
            })
            .unwrap()
            .location
        {
            Location::Pin(p) => assert_eq!(replacement_pin, p),
            Location::Virtual => panic!("expected pin location"),
        }

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: adding placement reclaims garbage placements"
    #[test]
    fn storage_adding_placement_reclaims_garbage_placements() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();

        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();
        let old_pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            0,
            Placement::with_location(Location::Pin(old_pin)),
        )
        .unwrap();
        // old_pin.garbage = true;
        // SAFETY: `old_pin` is a live tracked pin owned by the pagelist.
        unsafe { (*old_pin).garbage = true };

        let new_pin = track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            0,
            Placement::with_location(Location::Pin(new_pin)),
        )
        .unwrap();

        assert_eq!(1usize, s.placements.len());
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );
        match s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::Internal,
                    id: 1,
                },
            })
            .unwrap()
            .location
        {
            Location::Pin(p) => assert_eq!(new_pin, p),
            Location::Virtual => panic!("expected pin location"),
        }

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: placement count limit permits replacement"
    #[test]
    fn storage_placement_count_limit_permits_replacement() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Virtual),
        )
        .unwrap();

        // const img = s.images.getPtr(1).?;
        // img.metadata.placement_count = std.math.maxInt(@TypeOf(img.metadata.placement_count));
        let img = s.images.get_mut(&1).unwrap();
        img.metadata.placement_count = Metadata::PLACEMENT_COUNT_MAX;
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Virtual),
        )
        .unwrap();
        assert_eq!(
            Err(OutOfMemory),
            s.add_placement(
                alloc,
                t.screens.active,
                1,
                2,
                Placement::with_location(Location::Virtual),
            )
        );
        assert_eq!(1usize, s.placements.len());

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: delete intersecting cursor checks interior row column"
    #[test]
    fn storage_delete_intersecting_cursor_checks_interior_row_column() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 10,
                height: 10,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 20, y: 0 }).unwrap(),
            )),
        )
        .unwrap();

        // This is inside the right placement and on an interior row shared by
        // both placements, but it is outside the left placement's columns.
        unsafe { (*t.screens.active).cursor_absolute(21, 5) };
        s.delete(alloc, &mut t, command::Delete::IntersectCursor(false));

        assert_eq!(1usize, s.placements.len());
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

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: delete intersecting cell checks interior row column"
    #[test]
    fn storage_delete_intersecting_cell_checks_interior_row_column() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(100, 100)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 10,
                height: 10,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 20, y: 0 }).unwrap(),
            )),
        )
        .unwrap();

        // Protocol coordinates are one-based, so this targets grid cell (21, 5).
        s.delete(
            alloc,
            &mut t,
            command::Delete::IntersectCell {
                delete: false,
                x: 22,
                y: 6,
            },
        );

        assert_eq!(1usize, s.placements.len());
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

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: placement geometry handles untrusted dimensions"
    #[test]
    fn storage_placement_geometry_handles_untrusted_dimensions() {
        let alloc = allocator_api2::alloc::Global;
        let max = u32::MAX;

        let mut t = Terminal::init(alloc, opts(2, 2)).unwrap();
        t.width_px = max;
        t.height_px = max;

        // Test helper: the reference passes the image as an anonymous literal
        // `.{ .width = 1, .height = 1 }`.
        let image_1x1 = Image {
            width: 1,
            height: 1,
            ..Image::default()
        };

        // Cell dimensions multiplied by protocol-controlled row and column
        // counts saturate instead of panicking or wrapping.
        {
            let placement: Placement = Placement {
                columns: 3,
                rows: 3,
                ..Placement::with_location(Location::Virtual)
            };
            let actual = placement.pixel_size(&image_1x1, &t);
            assert_eq!(max, actual.width);
            assert_eq!(max, actual.height);
        }

        // Aspect-ratio scaling also saturates when the derived dimension does
        // not fit in the public u32 geometry type.
        {
            let placement: Placement = Placement {
                columns: 3,
                source_height: max,
                ..Placement::with_location(Location::Virtual)
            };
            let actual = placement.pixel_size(&image_1x1, &t);
            assert_eq!(max, actual.width);
            assert_eq!(max, actual.height);
        }
        {
            let placement: Placement = Placement {
                rows: 3,
                source_width: max,
                ..Placement::with_location(Location::Virtual)
            };
            let actual = placement.pixel_size(&image_1x1, &t);
            assert_eq!(max, actual.width);
            assert_eq!(max, actual.height);
        }

        // Pixel offsets are protocol-controlled too. Include them without
        // allowing the grid-size numerator to wrap.
        t.width_px = 2;
        t.height_px = 2;
        {
            let placement: Placement = Placement {
                x_offset: max,
                y_offset: max,
                ..Placement::with_location(Location::Virtual)
            };
            let actual = placement.grid_size(&image_1x1, &t);
            assert_eq!(max, actual.cols);
            assert_eq!(max, actual.rows);
        }

        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        // defer t.screens.active.pages.untrackPin(pin); — explicit below.

        // Explicit maximum dimensions must clamp the rectangle to the terminal
        // without overflowing its horizontal extent.
        {
            let placement: Placement = Placement {
                columns: max,
                rows: 1,
                ..Placement::with_location(Location::Pin(pin))
            };
            let rect = placement.rect(&image_1x1, &t).unwrap();
            assert_eq!(1 as size::CellCountInt, rect.bottom_right.x);
        }

        // A garbage pin represents content that has been pruned from retained
        // history. Its fallback location must not make the placement visible.
        // SAFETY: `pin` is a live tracked pin owned by the pagelist.
        unsafe { (*pin).garbage = true };
        {
            let placement: Placement = Placement {
                columns: 1,
                rows: 1,
                ..Placement::with_location(Location::Pin(pin))
            };
            assert!(placement.rect(&image_1x1, &t).is_none());
        }
        unsafe { (*pin).garbage = false };

        // Terminals can temporarily have no pixel geometry. A placement whose
        // computed grid is empty has no rectangle, so rect must not subtract one
        // from a zero row count.
        t.width_px = 0;
        t.height_px = 0;
        {
            let placement: Placement = Placement {
                columns: 1,
                ..Placement::with_location(Location::Pin(pin))
            };
            let actual = placement.grid_size(&image_1x1, &t);
            assert_eq!(0u32, actual.cols);
            assert_eq!(0u32, actual.rows);
            assert!(placement.rect(&image_1x1, &t).is_none());
        }

        // (deferred) t.screens.active.pages.untrackPin(pin);
        unsafe { (*t.screens.active).pages.untrack_pin(pin) };
        t.deinit(alloc);
    }

    // test "storage: eviction releases placement pins"
    #[test]
    fn storage_eviction_releases_placement_pins() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage {
            total_limit: 128,
            ..ImageStorage::default()
        };

        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                data: Data::Complete(vec![b'*'; 64]),
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                data: Data::Complete(vec![b'*'; 64]),
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        assert_eq!(
            tracked + 2,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        // Adding a third image evicts the oldest equally-ranked image and its
        // placement. The newer image's placement and tracked pin remain intact.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                data: Data::Complete(vec![b'*'; 64]),
                ..Image::default()
            },
        )
        .unwrap();
        assert!(!s.images.contains_key(&1));
        assert!(s.images.contains_key(&2));
        assert!(s.images.contains_key(&3));
        assert_eq!(1usize, s.placements.len());
        assert!(s.placements.contains_key(&PlacementKey {
            image_id: 2,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1
            },
        }));
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: pending image completes once and preserves age"
    #[test]
    fn storage_pending_image_completes_once_and_preserves_age() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage {
            total_limit: 16,
            ..ImageStorage::default()
        };

        let pending = s
            .add_pending_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    number: 7,
                    width: 2,
                    height: 2,
                    format: command::Format::Rgba,
                    data: Data::Pending(16),
                    ..Image::default()
                },
            )
            .unwrap();
        assert_eq!(16usize, s.total_bytes);
        assert_eq!(1u32, s.image_by_number(7).unwrap().id);
        assert!(s.image_by_id(1).unwrap().data.is_pending());

        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        assert_eq!(1usize, s.placements.len());

        // const wrong = try alloc.dupe(u8, "short");
        // const wrong_completed = pending.complete(&s, io, wrong);
        // try testing.expect(!wrong_completed);
        // defer alloc.free(wrong); — the Vec drops inside `complete` on false.
        let wrong = b"short".to_vec();
        let wrong_completed = pending.complete(&mut s, wrong);
        assert!(!wrong_completed);

        let storage_generation = s.generation;
        s.dirty = false;
        let pixels = vec![b'*'; 16];
        assert!(pending.complete(&mut s, pixels.clone()));
        assert!(s.dirty);
        assert!(s.generation > storage_generation);
        assert_eq!(pending.generation, s.image_by_id(1).unwrap().generation);
        assert_eq!(16usize, s.total_bytes);
        // try testing.expectEqualSlices(u8, pixels, s.imageById(1).?.data.bytes().?);
        let stored = s.image_by_id(1).unwrap();
        assert_eq!(pixels.as_slice(), stored.data.bytes().unwrap());

        let duplicate = vec![b'!'; 16];
        let duplicate_completed = pending.complete(&mut s, duplicate);
        assert!(!duplicate_completed);

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: stale pending completion loses to delete replacement and eviction"
    #[test]
    fn storage_stale_pending_completion_loses_to_delete_replacement_and_eviction() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage {
            total_limit: 8,
            ..ImageStorage::default()
        };

        let deleted = s
            .add_pending_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    data: Data::Pending(4),
                    ..Image::default()
                },
            )
            .unwrap();
        s.delete(
            alloc,
            &mut t,
            command::Delete::Id {
                delete: true,
                image_id: 1,
                placement_id: 0,
            },
        );
        let deleted_data = b"gone".to_vec();
        let deleted_completed = deleted.complete(&mut s, deleted_data);
        assert!(!deleted_completed);

        let replaced = s
            .add_pending_image(
                alloc,
                t.screens.active,
                Image {
                    id: 2,
                    data: Data::Pending(4),
                    ..Image::default()
                },
            )
            .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                data: Data::Complete(b"live".to_vec()),
                ..Image::default()
            },
        )
        .unwrap();
        let replaced_data = b"late".to_vec();
        let replaced_completed = replaced.complete(&mut s, replaced_data);
        assert!(!replaced_completed);
        // try testing.expectEqualStrings("live", s.imageById(2).?.data.bytes().?);
        let live = s.image_by_id(2).unwrap();
        assert_eq!(b"live".as_slice(), live.data.bytes().unwrap());

        let evicted = s
            .add_pending_image(
                alloc,
                t.screens.active,
                Image {
                    id: 3,
                    data: Data::Pending(4),
                    ..Image::default()
                },
            )
            .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 4,
                data: Data::Complete(b"12345678".to_vec()),
                ..Image::default()
            },
        )
        .unwrap();
        assert!(s.image_by_id(3).is_none());
        let evicted_data = b"late".to_vec();
        let evicted_completed = evicted.complete(&mut s, evicted_data);
        assert!(!evicted_completed);

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: replacement reuses pending reservation and removes placements"
    #[test]
    fn storage_replacement_reuses_pending_reservation_and_removes_placements() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage {
            total_limit: 12,
            ..ImageStorage::default()
        };
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        let pending = s
            .add_pending_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    width: 2,
                    height: 1,
                    format: command::Format::Rgba,
                    data: Data::Pending(8),
                    ..Image::default()
                },
            )
            .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                data: Data::Complete(b"keep".to_vec()),
                ..Image::default()
            },
        )
        .unwrap();

        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 2,
                height: 1,
                format: command::Format::Rgba,
                data: Data::Complete(b"12345678".to_vec()),
                ..Image::default()
            },
        )
        .unwrap();
        assert!(s.images.contains_key(&1));
        assert!(s.images.contains_key(&2));
        assert_eq!(0usize, s.placements.len());
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );
        // try testing.expectEqual(@as(u30, 0), s.imageById(1).?.metadata.placement_count);
        assert_eq!(0u32, s.image_by_id(1).unwrap().metadata.placement_count);
        assert_eq!(12usize, s.total_bytes);

        let stale = b"snapshot".to_vec();
        let stale_completed = pending.complete(&mut s, stale);
        assert!(!stale_completed);

        // Growing the replacement requires eviction, but the replacement ID is
        // excluded. The other image supplies the needed bytes.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                data: Data::Complete(b"1234567890".to_vec()),
                ..Image::default()
            },
        )
        .unwrap();
        assert!(s.images.contains_key(&1));
        assert!(!s.images.contains_key(&2));
        assert_eq!(0usize, s.placements.len());
        assert_eq!(10usize, s.total_bytes);

        s.delete(
            alloc,
            &mut t,
            command::Delete::Id {
                delete: true,
                image_id: 1,
                placement_id: 0,
            },
        );
        assert_eq!(0usize, s.images.len());
        assert_eq!(0usize, s.placements.len());

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: pending images share exact eviction ordering"
    #[test]
    fn storage_pending_images_share_exact_eviction_ordering() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage {
            total_limit: 192,
            ..ImageStorage::default()
        };

        let _ = s
            .add_pending_image(
                alloc,
                t.screens.active,
                Image {
                    id: 1,
                    data: Data::Pending(64),
                    metadata: Metadata {
                        transient: false,
                        ..Metadata::default()
                    },
                    ..Image::default()
                },
            )
            .unwrap();
        let _ = s
            .add_pending_image(
                alloc,
                t.screens.active,
                Image {
                    id: 2,
                    data: Data::Pending(64),
                    metadata: Metadata {
                        transient: true,
                        ..Metadata::default()
                    },
                    ..Image::default()
                },
            )
            .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                data: Data::Complete(vec![b'*'; 64]),
                metadata: Metadata {
                    transient: true,
                    ..Metadata::default()
                },
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            2,
            1,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();

        // ID 3 is transient and unused, so one exact-size eviction is enough.
        assert!(s.evict_image(alloc, t.screens.active, 64));
        assert!(s.images.contains_key(&1));
        assert!(s.images.contains_key(&2));
        assert!(!s.images.contains_key(&3));
        assert_eq!(128usize, s.total_bytes);

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }
}
