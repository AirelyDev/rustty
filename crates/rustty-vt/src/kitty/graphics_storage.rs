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
use crate::alloc::{ABox, AVec, Allocator, Bytes, TryPushExt};
use crate::kitty::graphics_animation as animation;
use crate::kitty::graphics_command as command;
use crate::kitty::graphics_image;
use crate::kitty::graphics_image::{Image, Limits, LoadingImage};
use crate::kitty::graphics_pixel as pixel;
use crate::pagelist as PageList;
use crate::point;
use crate::screen::Screen;
use crate::terminal::Terminal;
use rustty_util::quirks::inlineAssert as assert;
use std::hash::RandomState;

// The `delete`/eviction cluster and the placement types live in topic
// children; the placement types are re-exported so every public path is
// unchanged.
mod delete;
mod placement;

use placement::Restore;
pub use placement::{
    CellOffset, GridSize, Location, OutOfMemory, PixelSize, Placement, PlacementId, PlacementIdTag,
    PlacementKey, Relative, ScrollMargins, ScrollOp, SourceRect,
};

// const log = std.log.scoped(.kitty_gfx);
const LOG_SCOPE: &str = "kitty_gfx";

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
// `placements`. The Image value owns its pixel data (non-`Copy`), so the
// allocator-backed pointer map (`crate::hash_map::HashMapUnmanaged`, which is
// `K: Copy, V: Copy`) cannot store it. They are `hashbrown` maps over the
// allocator handle (parity tail S7-4), with std's `RandomState` hasher (what
// the `std::collections::HashMap` they replace used). An unmanaged `.{}` map
// holds no allocator: each map binds to the allocator of its first growing
// call (`bind_map`), and later calls debug-assert that it is the same one,
// the reference's contract. Both Zig and Rust hash maps are unordered; the
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

    /// This is the next automatically assigned image ID for images
    /// transmitted without an ID or number. We start mid-way through
    /// the u32 range to stay clear of the low IDs client programs
    /// typically pick. See next_image_id_for.
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
    // PORT NOTE (parity tail S7-4): `alloc.create(LoadingImage)` through the
    // storage's allocator.
    pub loading: Option<ABox<LoadingImage>>,

    /// The limits of what medium types are allowed for image loading.
    pub image_limits: Limits,

    /// The total bytes of image data that have been loaded and the limit.
    /// If the limit is reached, the oldest images will be evicted to make
    /// space. Unused images take priority.
    pub total_bytes: usize,
    pub total_limit: usize,
}

// const ImageMap = std.AutoHashMapUnmanaged(u32, Image);
pub type ImageMap = hashbrown::HashMap<u32, Image, RandomState, Allocator>;
// const PlacementMap = std.AutoHashMapUnmanaged(PlacementKey, Placement);
pub type PlacementMap = hashbrown::HashMap<PlacementKey, Placement, RandomState, Allocator>;
/// The placements map's iterator (`PlacementMap.Iterator`).
pub type PlacementMapIter<'a> = hashbrown::hash_map::Iter<'a, PlacementKey, Placement>;

/// `.{}`: an empty unmanaged map (no allocation; bound on first growth).
fn empty_map<K, V>() -> hashbrown::HashMap<K, V, RandomState, Allocator> {
    hashbrown::HashMap::with_hasher_in(RandomState::new(), crate::alloc::Global)
}

/// Bind an unmanaged map to the allocator a reference call site passes: while
/// it owns no allocation it is rebound to `alloc`; afterwards `alloc` is
/// debug-asserted to be the allocator it was bound to (see `alloc::bind`).
fn bind_map<K, V>(map: &mut hashbrown::HashMap<K, V, RandomState, Allocator>, alloc: Allocator) {
    if map.capacity() == 0 {
        if *map.allocator() != alloc {
            debug_assert!(map.is_empty());
            *map = hashbrown::HashMap::with_hasher_in(RandomState::new(), alloc);
        }
    } else {
        map.allocator().debug_assert_same(alloc);
    }
}

/// Maximum number of parent links in a relative placement chain,
/// matching the minimum specified in the spec and the actual
/// limit defined by Kitty at the time of authoring.
//
// PORT NOTE: `ImageStorage.parent_chain_limit` upstream; hoisted to module
// level (Rust associated consts can't be referenced from a nested item the
// way Zig decls can, and the module is the ImageStorage namespace here).
pub const PARENT_CHAIN_LIMIT: usize = 8;

/// `pub const ParentError = error{...}` on ImageStorage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentError {
    /// The parent image (P=) does not exist.
    ParentImageNotFound,
    /// The parent image exists but the requested placement (Q=, or
    /// any placement when Q is omitted) does not.
    ParentPlacementNotFound,
    /// The placement refers to itself as its own parent.
    SelfParent,
    /// The parent chain loops back to the placement being created.
    Cycle,
    /// The parent chain exceeds parent_chain_limit links.
    TooDeep,
    /// An ancestor in the chain no longer exists. This should not
    /// happen since removeOrphans keeps chains intact, but we
    /// check anyway rather than trusting the invariant.
    AncestorNotFound,
}

/// The result of resolving a relative placement's parent chain:
/// the chain's root placement (always pin or virtual, never
/// relative) and the total cell offset from the root's origin.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedChain {
    pub root_key: PlacementKey,
    pub root: Placement,
    pub horizontal_offset: i32,
    pub vertical_offset: i32,
}

/// PORT NOTE: the reference's `placeholderTarget` returns an anonymous
/// `?struct { key: PlacementKey, placement: Placement }`; named here because
/// Rust has no anonymous structs.
#[derive(Debug, Clone, Copy)]
pub struct PlaceholderTarget {
    pub key: PlacementKey,
    pub placement: Placement,
}

/// PORT NOTE: the reference's `nextImageId(mode: enum { explicit, implicit })`
/// anonymous parameter enum, named because Rust has no anonymous enums.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextImageIdMode {
    Explicit,
    Implicit,
}

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
            images: empty_map(),
            placements: empty_map(),
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
    // on `false`. Rust ownership transfers the bytes either way: on `true`
    // they become the image payload; on `false` they are dropped (freed
    // through their allocator) here — observably the caller's
    // `defer alloc.free(data)`.
    pub fn complete(&self, storage: &mut ImageStorage, data: Bytes) -> bool {
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
        if let Some(loading) = self.loading.take() {
            LoadingImage::destroy(loading, alloc);
        }

        self.clear_placements(s);
        // self.placements.deinit(alloc);
        if self.placements.capacity() != 0 {
            self.placements.allocator().debug_assert_same(alloc);
        }
        self.placements = empty_map();

        // var it = self.images.iterator();
        // while (it.next()) |kv| kv.value_ptr.deinit(alloc);
        // self.images.deinit(alloc);
        for img in self.images.values_mut() {
            img.deinit(alloc);
        }
        if self.images.capacity() != 0 {
            self.images.allocator().debug_assert_same(alloc);
        }
        self.images = empty_map();
    }

    /// Kitty image protocol is enabled if we have a non-zero limit.
    pub fn enabled(&self) -> bool {
        self.total_limit != 0
    }

    /// Record a content mutation: marks the storage dirty and assigns a
    /// fresh generation stamp. Must be called by anything that changes
    /// the set of images or placements (or image contents), including
    /// the animation command handlers in graphics_exec.zig.
    ///
    /// Do NOT call this for geometry-only events (scrolling, resizing, screen
    /// switches); those must set only the dirty flag directly. Bumping the
    /// generation for geometry changes would break the contract that an
    /// unchanged generation means unchanged contents.
    //
    // PORT NOTE: `pub` since the animation exec paths call it directly
    // (matching the head `pub fn markMutated`).
    pub fn mark_mutated(&mut self) {
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
            log::info!(target: LOG_SCOPE, "evicting images to lower limit, evicting={req_bytes}");
            if !self.evict_image(alloc, s, req_bytes) {
                log::warn!(target: LOG_SCOPE, "failed to evict enough images for required bytes");
            }
        }

        self.total_limit = limit;
    }

    /// Returns the next ID to automatically assign to an image that
    /// was transmitted without an explicit ID (i=). The result is
    /// never zero (zero means "no ID") and never an ID currently in
    /// use, so an automatic ID never replaces an existing image.
    //
    // PORT NOTE: the reference is `nextImageId(self, mode: enum { explicit,
    // implicit })` with an anonymous parameter enum; Rust needs the named
    // `NextImageIdMode`, and the method is `next_image_id_for` because the
    // `next_image_id` name is taken by the field.
    pub fn next_image_id_for(&mut self, mode: NextImageIdMode) -> u32 {
        // Starting ID depends on mode.
        let mut id: u32 = match mode {
            // Yeah, starting from 1 is wasteful. This matches what Kitty
            // does. In the future we can probably cache a low-water-mark
            // based on delete behavior.
            NextImageIdMode::Explicit => 1,
            NextImageIdMode::Implicit => self.next_image_id,
        };

        // Go through all possible images. We have +2 because we can
        // exceed the total by 1 (new ID).
        let count: usize = self.images.len();
        for _ in 0..count + 2 {
            if id != 0 && !self.images.contains_key(&id) {
                break;
            }
            id = id.wrapping_add(1);
        }

        // Zero is never allowed.
        if id == 0 {
            id = 1;
        }

        // Keep track of our next image id for implicit to speed that up.
        if mode == NextImageIdMode::Implicit {
            self.next_image_id = id.wrapping_add(1);
        }

        id
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
            old.storage_size()
        } else {
            0
        };
        assert(old_len <= self.total_bytes);

        // Reserve map capacity before evicting anything so allocation failure
        // cannot leave storage partially evicted. Existing keys need no room.
        // try self.images.ensureUnusedCapacity(alloc, 1);
        if !self.images.contains_key(&img.id) {
            bind_map(&mut self.images, alloc);
            self.images.try_reserve(1).map_err(|_| OutOfMemory)?;
        }

        // If this would put us over the limit, then evict.
        let total_bytes = self.total_bytes - old_len + new_len;
        if total_bytes > self.total_limit {
            let req_bytes = total_bytes - self.total_limit;
            log::info!(target: LOG_SCOPE, "evicting images to make space for {req_bytes} bytes");
            if !self.evict_image_except(alloc, s, req_bytes, Some(img.id)) {
                log::warn!(target: LOG_SCOPE, "failed to evict enough images for required bytes");
                return Err(OutOfMemory);
            }
        }

        // const gop = self.images.getOrPutAssumeCapacity(img.id);

        log::debug!(target: LOG_SCOPE, "addImage image={:?}", img.withoutData());

        // if (gop.found_existing) {
        //     self.removePlacementsByImageId(s, img.id);
        //     _ = self.removeOrphans(s, null);
        //     self.total_bytes -= gop.value_ptr.storageSize();
        //     gop.value_ptr.deinit(alloc);
        // }
        let id = img.id;
        if self.images.contains_key(&id) {
            // Retransmitting a specific image ID replaces the old image and all
            // of its placements, as required by the Kitty graphics protocol.
            self.remove_placements_by_image_id(s, id);

            // Relative placements parented to the removed placements go too.
            let _ = self.remove_orphans(s, None);

            // Replacing an image drops its animation frames with it,
            // implementing the protocol rule that retransmitting the
            // base image resets the animation.
            self.total_bytes -= self.images.get(&id).unwrap().storage_size();
            // gop.value_ptr.deinit(alloc);
            self.images.get_mut(&id).unwrap().deinit(alloc);
        }

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
        // assert(self.images.get(image_id) != null);
        assert(self.images.contains_key(&image_id));
        log::debug!(
            target: LOG_SCOPE,
            "placement image_id={image_id} placement_id={placement_id} placement={p:?}\n"
        );

        // When we add a placement, take this opportunity to reap garbage.
        // Garbage are placements that disappeared off scrollback (were
        // pruned). We don't clear Kitty state in the hot path so we do
        // this opportunistically here.
        self.reap_garbage_placements(s);

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
        // PORT NOTE (parity tail S7-4): `getOrPut`'s growth is the fallible
        // `try_reserve(1)`, made before anything else changes, as the
        // reference's `try` is.
        if !self.placements.contains_key(&key) {
            bind_map(&mut self.placements, alloc);
            self.placements.try_reserve(1).map_err(|_| OutOfMemory)?;
        }
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
                    return Err(OutOfMemory);
                }
            }
        }
        self.placements.insert(key, p);

        // This always mutates
        self.mark_mutated();
        Ok(())
    }

    /// Adjust placements for a scroll of the terminal's scrolling region
    /// by `delta` rows (negative moves content up). This implements the
    /// Kitty graphics protocol behavior for scrolls bounded by margins:
    /// placements entirely inside the scrolling region move with the content
    /// and are clipped when the move would extend them past a margin (deleted
    /// when fully clipped), while every other placement (straddling a margin,
    /// outside the region, or in the scrollback) must not move at all.
    ///
    /// This is split into two phases around the row operation because
    /// the various scroll implementations move tracked pins in
    /// inconsistent ways (some shift the active area so every pin moves,
    /// some rotate rows in place and adjust only pins inside the
    /// region). We record the desired final position of every placement
    /// up front in the returned state, whose end() repositions the pins
    /// once the rows have settled, making the result independent of the
    /// path taken. Every call must be paired with exactly one end().
    ///
    /// Callers must skip this entirely when the scrolling region is the
    /// full screen: a full-screen scroll moves placements with their
    /// anchored rows (possibly into the scrollback) via pin tracking,
    /// which already matches kitty's marginless scroll behavior.
    //
    // PORT NOTE (hazard #1, phase-5 plan — the ScrollMargins guard): the
    // reference takes `t: *terminal.Terminal` while `self` lives INSIDE
    // `t.screens.active` (`screen.kitty_images`) — the raw-`*mut` argument
    // follows `rust-mut-args-strongly-protected` / this file's `s: *mut
    // Screen` discipline: `t`/`s` are dereferenced only in narrow scopes
    // (field reads, `grid_size(&*t)`, `Placement::deinit(s)`) that never
    // overlap a `&mut` reconstruction of the terminal or screen. The
    // Zig iterate-with-removal over `placements` is the sanctioned
    // collect-keys-first shape. Proven on the isolated Miri harness at
    // `.port/phase-5/mirihar-scroll-margins/`.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn scroll_margins_begin(
        &mut self,
        t: *mut Terminal,
        delta: isize,
        op: ScrollOp,
    ) -> ScrollMargins {
        // const s: *terminal.Screen = t.screens.active;
        // SAFETY(raw-page-aliasing/graphics_storage): `t` is the terminal
        // owning this storage's screen (the caller's contract); only field
        // reads are performed through it in this fn and no `&mut Terminal`
        // is reconstructed while `&mut self` is live.
        let s: *mut Screen = unsafe { (*t).screens.active };
        // assert(self == &s.kitty_images);
        assert(core::ptr::eq(self as *const ImageStorage, unsafe {
            &raw const (*s).kitty_images
        }));
        // (`restores` grows through the screen's allocator, `s.alloc`.)
        let mut result: ScrollMargins = ScrollMargins {
            screen: s,
            restores: AVec::new_in(unsafe { (*s).alloc }),
        };

        // const top: i64 = t.scrolling_region.top;
        // const bottom: i64 = t.scrolling_region.bottom;
        let top: i64 = unsafe { (*t).scrolling_region.top } as i64;
        let bottom: i64 = unsafe { (*t).scrolling_region.bottom } as i64;

        let mut mutated = false;
        // var it = self.placements.iterator();
        // while (it.next()) |entry| { ... }
        let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
        'placements: for key in keys {
            // const p: *Placement = entry.value_ptr;

            // Virtual placements follow their placeholder cells and
            // relative placements follow their parents, so neither is
            // ever adjusted by scrolls, matching kitty.
            let pin: *mut PageList::Pin = match self.placements.get(&key).unwrap().location {
                Location::Pin(pin) => pin,
                Location::Virtual | Location::Relative(_) => continue,
            };

            // Pruned placements are reaped by reapGarbagePlacements.
            // SAFETY(raw-page-aliasing/graphics_storage): `pin` is a tracked
            // pin owned by the screen's pagelist; only shared reads are
            // performed and no `&mut` to the pin is live across them.
            if unsafe { (*pin).garbage } {
                continue;
            }

            // Placements anchored outside the active area (scrollback)
            // are never touched by a scroll within the margins.
            // SAFETY: see the `s`/`pin` contracts above; `point_from_pin`
            // takes `&self.pages` and the pin by value.
            let coord = match unsafe { (*s).pages.point_from_pin(point::Tag::Active, *pin) } {
                Some(pt) => pt.coord(),
                None => continue,
            };
            let y: i64 = coord.y as i64;

            // An in-place operation doesn't touch pins anchored outside
            // the region rows, and a placement anchored there can never
            // be entirely inside the region, so it needs no work at all.
            if op == ScrollOp::InPlace && (y < top || y > bottom) {
                continue;
            }

            // The final position defaults to unmoved; restoring the pin
            // to this exact spot afterwards is what stops the row
            // operations from dragging the placement around.
            let mut final_y: i64 = y;

            'inside: {
                // const image = self.images.get(entry.key_ptr.image_id) orelse break :inside;
                let image = match self.images.get(&key.image_id) {
                    Some(image) => image,
                    None => break 'inside,
                };
                // const grid = p.gridSize(image, t);
                // SAFETY: narrow shared reborrow of the terminal for the
                // geometry read; no `&mut` to the terminal is live.
                let grid = self
                    .placements
                    .get(&key)
                    .unwrap()
                    .grid_size(image, unsafe { &*t });

                // Unknown geometry (e.g. pixel sizes not yet reported)
                // means we can't know the extent, so we can't consider
                // the placement contained.
                if grid.rows == 0 || grid.cols == 0 {
                    break 'inside;
                }

                // Only placements entirely within the scrolling region
                // move. Kitty only has top/bottom margins so it only
                // checks rows; the column check is our generalization
                // for left/right margins and is trivially true for
                // full-width regions.
                if y < top || y + grid.rows as i64 - 1 > bottom {
                    break 'inside;
                }
                let x: i64 = coord.x as i64;
                // if (x < t.scrolling_region.left or
                //     @min(x + grid.cols - 1, @as(i64, t.cols) - 1) > t.scrolling_region.right) break :inside;
                if x < unsafe { (*t).scrolling_region.left } as i64
                    || core::cmp::min(x + grid.cols as i64 - 1, unsafe { (*t).cols } as i64 - 1)
                        > unsafe { (*t).scrolling_region.right } as i64
                {
                    break 'inside;
                }

                let new_y: i64 = y + delta as i64;
                let rows: i64 = grid.rows as i64;
                let top_clip: i64 = core::cmp::max(0, top - new_y);
                let bottom_clip: i64 = core::cmp::max(0, new_y + rows - 1 - bottom);
                let visible: bool = 'clip: {
                    if top_clip >= rows || bottom_clip >= rows {
                        break 'clip false;
                    }

                    if top_clip > 0 {
                        // if (!p.clipTop(image, t, @intCast(top_clip), grid.rows)) break :clip false;
                        if !self.placements.get_mut(&key).unwrap().clip_top(
                            image,
                            unsafe { &*t },
                            u32::try_from(top_clip).unwrap(),
                            grid.rows,
                        ) {
                            break 'clip false;
                        }
                        mutated = true;
                        final_y = top;
                        break 'clip true;
                    }

                    if bottom_clip > 0 {
                        // if (!p.clipBottom(image, t, @intCast(bottom_clip), grid.rows)) break :clip false;
                        if !self.placements.get_mut(&key).unwrap().clip_bottom(
                            image,
                            unsafe { &*t },
                            u32::try_from(bottom_clip).unwrap(),
                            grid.rows,
                        ) {
                            break 'clip false;
                        }
                        mutated = true;
                    }

                    final_y = new_y;
                    true
                };

                // Scrolled or clipped entirely out of the region: the
                // placement is deleted, like kitty. The image itself is
                // retained for future placements.
                if !visible {
                    // self.removePlacement(s, entry);
                    self.remove_placement(s, &key);
                    mutated = true;
                    continue 'placements;
                }
            }

            // result.restores.append(s.alloc, .{ ... }) catch {
            //     // OOM: this placement is left to the whims of the
            //     // generic pin tracking. Degraded, but safe.
            //     log.warn("OOM adjusting image placement for scroll", .{});
            //     continue;
            // };
            // PORT NOTE: `try_push` is the fallible append through the screen's
            // allocator (parity tail S7-4 retired the `SCROLL_MARGINS_TW`
            // stand-in).
            // CORRECTION (parity S190-F2): this was a bare `Vec::push`, which
            // aborts on OOM, and the log call was commented out.
            let restore = Restore {
                pin,
                // SAFETY: shared read of the tracked pin, see above.
                x: unsafe { (*pin).x },
                // .y = @intCast(final_y),
                y: u32::try_from(final_y).unwrap(),
            };
            if result.restores.try_push(restore).is_err() {
                // OOM: this placement is left to the whims of the
                // generic pin tracking. Degraded, but safe.
                log::warn!(target: LOG_SCOPE, "OOM adjusting image placement for scroll");
                continue 'placements;
            }
        }

        if mutated {
            // Placements deleted by clipping may orphan relative placements.
            // Orphans have no pins so this never touches the restores.
            let _ = self.remove_orphans(s, None);
            self.mark_mutated();
        }
        result
    }

    /// Reap pin-backed placements whose tracked content has been pruned
    /// from history, along with any relative placements orphaned by
    /// that, marking the content mutation.
    ///
    /// Virtual and relative placements have no tracked screen location and
    /// are never pruned themselves. Kitty removes placements once they
    /// scroll out of retained history.
    fn reap_garbage_placements(&mut self, s: *mut Screen) {
        let mut removed = false;
        // var it = self.placements.iterator();
        // while (it.next()) |entry| { ... }
        let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
        for key in keys {
            let pin = match self.placements.get(&key).unwrap().location {
                Location::Pin(pin) => pin,
                Location::Virtual | Location::Relative(_) => continue,
            };
            // if (!pin.garbage) continue;
            // SAFETY(raw-page-aliasing/graphics_storage): `pin` is a tracked
            // pin owned by the screen's pagelist; no `&mut` to the pin or its
            // pagelist is live across this read.
            if !unsafe { (*pin).garbage } {
                continue;
            }

            self.remove_placement(s, &key);
            removed = true;
        }
        if !removed {
            return;
        }

        // If we removed a placement, then also remove any orphan
        // children if this was a parent.
        let _ = self.remove_orphans(s, None);

        self.mark_mutated();
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
            self.remove_placement(s, &key);
        }
    }

    /// Remove a placement by its map entry, releasing any tracked pin
    /// and keeping the image's placement count in sync. The entry must
    /// point into the placements map (via iteration or getEntry).
    //
    // PORT NOTE: the reference takes `entry: PlacementMap.Entry` (a pointer
    // pair into the map) and does `entry.value_ptr.deinit(s)` +
    // `removeByPtr(entry.key_ptr)`. The collect-keys-first shape removes by
    // key value (observably identical, see the hash-map PORT NOTE on the
    // struct); the value deinit happens here so every call site keeps the
    // deinit+decrement+remove trio atomic, exactly like upstream.
    fn remove_placement(&mut self, s: *mut Screen, key: &PlacementKey) {
        self.placements.get(key).unwrap().deinit(s);
        let img = self.images.get_mut(&key.image_id).unwrap();
        assert(img.metadata.placement_count > 0);
        img.metadata.placement_count -= 1;
        self.placements.remove(key);
    }

    /// Remove relative placements whose parent placement no longer
    /// exists, keeping a relative placement's lifetime tied to its
    /// parent chain. Chains can be several links deep, so this loops
    /// until a pass removes nothing to take out entire orphaned
    /// subtrees. Returns true if anything was removed.
    ///
    /// If `delete_unused` is non-null, an image left without placements
    /// by the reap is freed using it. This matches uppercase delete
    /// semantics; every other removal path retains image data and
    /// passes null.
    ///
    /// This must be called, outside of any placements iteration, after
    /// every operation that removes placements. Kitty gets the same
    /// effect lazily by dropping unresolvable placements while drawing;
    /// we do it eagerly because our renderer never mutates terminal
    /// state.
    //
    // PORT NOTE: `delete_unused: ?Allocator` — the option is the switch, the
    // allocator payload is a ZST here.
    fn remove_orphans(&mut self, s: *mut Screen, delete_unused: Option<Allocator>) -> bool {
        let mut removed_any = false;
        let mut removed = true;
        while removed {
            removed = false;
            // var it = self.placements.iterator();
            // while (it.next()) |entry| { ... } — collect-keys-first, the
            // sanctioned no-removeByPtr-during-iteration shape.
            let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
            for key in keys {
                let rel = match self.placements.get(&key).unwrap().location {
                    Location::Relative(rel) => rel,
                    Location::Pin(_) | Location::Virtual => continue,
                };
                if self.placements.contains_key(&rel.parent) {
                    continue;
                }

                // Parent is gone, remove this placement.
                let image_id = key.image_id;
                self.remove_placement(s, &key);
                removed = true;
                removed_any = true;

                if let Some(alloc) = delete_unused {
                    self.delete_if_unused(alloc, image_id);
                }
            }
        }

        removed_any
    }

    /// Resolve and validate the parent reference (P=/Q=) of a relative
    /// placement, returning the concrete key of the parent placement.
    /// `child` is the key of the placement being created when it is
    /// addressable (an explicit placement ID); null for placements that
    /// will receive a fresh internal ID, which nothing can refer to yet.
    ///
    /// Checks: the parent must exist, the placement must not
    /// parent itself, and the resulting ancestor chain must be acyclic
    /// and within parent_chain_limit.
    pub fn resolve_parent(
        &mut self,
        s: *mut Screen,
        child: Option<PlacementKey>,
        parent_image_id: u32,
        parent_placement_id: u32,
    ) -> Result<PlacementKey, ParentError> {
        // Reap placements whose content has been pruned from history
        // before resolving: a pruned parent must be reported as missing,
        // not accepted and then immediately reaped by addPlacement's own
        // sweep, which would store an orphan.
        self.reap_garbage_placements(s);

        // If the parent doesn't exist we're already failed.
        if !self.images.contains_key(&parent_image_id) {
            return Err(ParentError::ParentImageNotFound);
        }

        // Find the parent
        let parent: PlacementKey = 'parent: {
            // An explicit parent placement ID (Q=) selects exactly that
            // placement.
            if parent_placement_id > 0 {
                let key: PlacementKey = PlacementKey {
                    image_id: parent_image_id,
                    placement_id: PlacementId {
                        tag: PlacementIdTag::External,
                        id: parent_placement_id,
                    },
                };
                if !self.placements.contains_key(&key) {
                    return Err(ParentError::ParentPlacementNotFound);
                }
                break 'parent key;
            }

            // No Q: pick a placement of the parent image. Kitty picks
            // the oldest surviving placement; our placement map doesn't
            // track creation order so we pick by PlacementId.preferredOver
            // instead. This is unspecified so any behavior here is fine.
            // In practice the parent image has a single placement, and
            // clients use Q when it doesn't.
            let best: PlacementId = {
                let mut best: Option<PlacementId> = None;
                // var it = self.placements.keyIterator();
                for key in self.placements.keys() {
                    if key.image_id != parent_image_id {
                        continue;
                    }
                    let id = key.placement_id;
                    let b = match best {
                        Some(b) => b,
                        None => {
                            best = Some(id);
                            continue;
                        }
                    };
                    if id.preferred_over(b) {
                        best = Some(id);
                    }
                }
                match best {
                    Some(best) => best,
                    None => return Err(ParentError::ParentPlacementNotFound),
                }
            };

            PlacementKey {
                image_id: parent_image_id,
                placement_id: best,
            }
        };

        // A placement cannot be its own parent. This must be checked
        // before the chain walk so it reports EINVAL, not ECYCLE.
        if let Some(c) = child {
            if parent.eql(c) {
                return Err(ParentError::SelfParent);
            }
        }

        // Walk the would-be ancestor chain. `depth` counts parent links,
        // starting at one for the link we are about to create.
        let mut depth: usize = 1;
        let mut key = parent;
        loop {
            if let Some(c) = child {
                if key.eql(c) {
                    return Err(ParentError::Cycle);
                }
            }
            let p = match self.placements.get(&key) {
                Some(p) => p,
                None => return Err(ParentError::AncestorNotFound),
            };
            let rel = match p.location {
                Location::Relative(rel) => rel,
                // A pin or virtual placement is the chain root.
                Location::Pin(_) | Location::Virtual => break,
            };
            if depth >= PARENT_CHAIN_LIMIT {
                return Err(ParentError::TooDeep);
            }
            depth += 1;
            key = rel.parent;
        }

        Ok(parent)
    }

    /// Resolve a relative placement's parent chain to its root,
    /// accumulating the cell offsets (H=/V=) of every link on the way.
    /// The placement's position is the root's origin plus the accumulated
    /// offsets.
    ///
    /// Returns null when the chain is broken or too deep. Neither can
    /// normally happen for stored placements (resolveParent validates
    /// chains and removeOrphans removes broken ones), with one kitty
    /// quirk: replacing an ancestor placement can deepen the chains
    /// below it beyond parent_chain_limit after the fact.
    pub fn resolve_chain(&self, rel: Relative) -> Option<ResolvedChain> {
        let mut horizontal = rel.horizontal_offset;
        let mut vertical = rel.vertical_offset;
        let mut key = rel.parent;
        let mut depth: usize = 1;
        loop {
            let p = self.placements.get(&key)?;
            match p.location {
                Location::Relative(parent_rel) => {
                    if depth >= PARENT_CHAIN_LIMIT {
                        return None;
                    }
                    depth += 1;
                    horizontal = horizontal.saturating_add(parent_rel.horizontal_offset);
                    vertical = vertical.saturating_add(parent_rel.vertical_offset);
                    key = parent_rel.parent;
                }

                Location::Pin(_) | Location::Virtual => {
                    return Some(ResolvedChain {
                        root_key: key,
                        root: *p,
                        horizontal_offset: horizontal,
                        vertical_offset: vertical,
                    })
                }
            }
        }
    }

    /// Returns the placement that a unicode placeholder cell referencing
    /// this image/placement ID pair targets, or null if there is none. A
    /// zero placement ID targets one of the image's virtual placements,
    /// chosen by PlacementId.preferredOver so the choice is stable (an
    /// image rarely has more than one). This is the shared lookup used
    /// both for sizing placeholder runs and for positioning relative
    /// placements whose chain roots at a virtual placement.
    pub fn placeholder_target(
        &self,
        image_id: u32,
        placement_id: u32,
    ) -> Option<PlaceholderTarget> {
        if placement_id > 0 {
            let key: PlacementKey = PlacementKey {
                image_id,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: placement_id,
                },
            };
            let p = self.placements.get(&key)?;
            return Some(PlaceholderTarget { key, placement: *p });
        }

        let mut best: Option<PlacementKey> = None;
        // var it = self.placements.iterator();
        for (key, value) in self.placements.iter() {
            if key.image_id != image_id {
                continue;
            }
            if !matches!(value.location, Location::Virtual) {
                continue;
            }
            let b = match best {
                Some(b) => b,
                None => {
                    best = Some(*key);
                    continue;
                }
            };
            if key.placement_id.preferred_over(b.placement_id) {
                best = Some(*key);
            }
        }

        let key = best?;
        Some(PlaceholderTarget {
            key,
            placement: *self.placements.get(&key).unwrap(),
        })
    }

    /// Get an image by its ID. If the image doesn't exist, null is returned.
    //
    // PORT NOTE (parity tail S7-4): the reference returns `?Image` by value, a
    // shallow copy sharing the stored pixel data. The port returns a borrow of
    // the stored image: a copy would duplicate the pixel data, an allocation
    // (through the storage's, possibly the embedder's, allocator) the
    // reference never makes.
    pub fn image_by_id(&self, image_id: u32) -> Option<&Image> {
        self.images.get(&image_id)
    }

    /// Get an image by its number. If the image doesn't exist, return null.
    // PORT NOTE (parity tail S7-4): a borrow, as `image_by_id`.
    pub fn image_by_number(&self, image_number: u32) -> Option<&Image> {
        let mut newest: Option<&Image> = None;

        // var it = self.images.iterator();
        // while (it.next()) |kv| { ... }
        // PORT NOTE: the reference nests `if (number == ...) { if (newest == null
        // or ...) {` — we preserve that nested form per R1 (clippy's
        // `collapsible_if` would merge them).
        #[allow(clippy::collapsible_if)]
        for kv in self.images.values() {
            if kv.number == image_number {
                if newest.is_none() || kv.generation > newest.unwrap().generation {
                    newest = Some(kv);
                }
            }
        }

        newest
    }

    /// Get a mutable pointer to a stored image, by ID or (newest by)
    /// number, following the protocol's id/number addressing. Used by
    /// the animation commands, which mutate images in place. The
    /// pointer is invalidated by any operation that adds or removes
    /// images.
    //
    // PORT NOTE (hazard #3, phase-5 plan): the reference returns `?*Image` — a
    // raw pointer INTO `self.images` that exec then holds across storage
    // mutators which can evict (invalidation upstream is a documented
    // contract). The port returns a shared borrow; mutating callers resolve
    // the image's concrete `id` from it and re-look it up per mutation
    // (`images.get_mut`), which is the sanctioned collect/re-lookup shape and
    // observably identical (ids are unique and stable).
    pub fn image_ptr_by_id_or_number(&self, image_id: u32, image_number: u32) -> Option<&Image> {
        if image_id != 0 {
            return self.images.get(&image_id);
        }

        let mut newest: Option<&Image> = None;
        // var it = self.images.iterator();
        for kv in self.images.values() {
            if kv.number != image_number {
                continue;
            }
            if newest.is_none() || kv.generation > newest.unwrap().generation {
                newest = Some(kv);
            }
        }

        newest
    }

    /// Record that the displayed content of an image changed without
    /// the image being re-added: marks the storage mutated and stamps
    /// the image with the fresh generation so consumers (e.g. the
    /// renderer's texture cache) replace what they hold. Used when an
    /// animation changes which frame is current or edits the current
    /// frame's pixels.
    //
    // PORT NOTE (hazard #3): the reference takes `img: *Image` aliasing
    // `&mut self`; the port takes the image id and re-looks it up (the image
    // is present at every call site — upstream derefs the pointer
    // unconditionally).
    pub fn mark_image_content_changed(&mut self, image_id: u32) {
        self.mark_mutated();
        let generation = self.generation;
        self.images.get_mut(&image_id).unwrap().generation = generation;
    }

    /// Convert a stored image's base data to RGBA in place, adjusting
    /// byte accounting. All animation composition happens in RGBA;
    /// this is called before the first composition into an image.
    ///
    /// The pixels are unchanged visually but the stored representation
    /// changed, so the image is stamped with a fresh generation.
    //
    // PORT NOTE: `Allocator.Error!void` → `Result<(), OutOfMemory>`; the one
    // failure is `rgbaFromFormat`'s allocation. `img: *Image` → image id per
    // the hazard-#3 note above.
    // CORRECTION (parity S190-F2): this was infallible because
    // `rgba_from_format` was.
    pub fn convert_image_to_rgba(
        &mut self,
        alloc: Allocator,
        image_id: u32,
    ) -> Result<(), OutOfMemory> {
        let (rgba, old_len) = {
            let img = self.images.get(&image_id).unwrap();
            if img.format == command::Format::Rgba {
                return Ok(());
            }
            // const old = img.data.bytes() orelse return;
            let old = match img.data.bytes() {
                Some(old) => old,
                None => return Ok(()),
            };

            // const rgba = try pixel.rgbaFromFormat(alloc, img.format, old);
            (pixel::rgba_from_format(alloc, img.format, old)?, old.len())
        };
        {
            let img = self.images.get_mut(&image_id).unwrap();
            let new_len = rgba.len();
            img.data.deinit(alloc);
            img.data = graphics_image::Data::Complete(rgba);
            img.format = command::Format::Rgba;
            self.total_bytes -= old_len;
            self.total_bytes += new_len;
        }
        self.mark_image_content_changed(image_id);
        Ok(())
    }

    /// Reserve `bytes` of storage for animation frame data belonging
    /// to `image_id`, evicting other images if needed, mirroring how
    /// image transmission reserves space.
    ///
    /// Errors if the space cannot be made available. On success the caller
    /// owns the reservation and must either attach the frame data to the
    /// image or call releaseAnimationBytes.
    pub fn reserve_animation_bytes(
        &mut self,
        alloc: Allocator,
        s: *mut Screen,
        image_id: u32,
        bytes: usize,
    ) -> Result<(), OutOfMemory> {
        if bytes > self.total_limit {
            return Err(OutOfMemory);
        }

        let total_bytes = self.total_bytes + bytes;
        if total_bytes > self.total_limit {
            let req_bytes = total_bytes - self.total_limit;
            // Excess this large cannot be recovered by evicting other
            // images (evictImageExcept also requires it).
            if req_bytes > self.total_limit {
                return Err(OutOfMemory);
            }
            log::info!(target: LOG_SCOPE, "evicting images for animation frame, evicting={req_bytes}");
            if !self.evict_image_except(alloc, s, req_bytes, Some(image_id)) {
                log::warn!(target: LOG_SCOPE, "failed to evict enough images for animation frame");
                return Err(OutOfMemory);
            }
        }

        self.total_bytes += bytes;
        Ok(())
    }

    /// Release a reservation made by reserveAnimationBytes, or credit
    /// bytes freed by deleting animation frame data.
    pub fn release_animation_bytes(&mut self, bytes: usize) {
        assert(bytes <= self.total_bytes);
        self.total_bytes -= bytes;
    }

    /// Advance every running animation to the frame that should be
    /// displayed at `now_ms` and report when the next frame change is
    /// due, as a delay in milliseconds relative to `now_ms`. Null
    /// means no running animation needs a future tick.
    ///
    /// `now_ms` is a monotonic timestamp on a clock of the caller's
    /// choosing. The same clock must be used for every call. The
    /// caller is expected to be the renderer, ticking once per frame
    /// build and scheduling a wakeup for the returned delay.
    ///
    /// PORT NOTE (hazard #4, phase-5 plan): upstream drives this from the
    /// RENDER thread (`renderer/generic.zig:1388`) while exec mutates the
    /// same storage from the io/stream side — safe only under the
    /// renderer/terminal mutex. The rustty renderer half is a later wave
    /// (W7); when it lands, the tick call MUST sit behind the same lock
    /// discipline as every other storage access.
    pub fn animation_tick(&mut self, now_ms: u64) -> Option<u64> {
        let mut min_delay: Option<u64> = None;

        // var it = self.images.iterator();
        //
        // PORT NOTE (hazard #3): the reference mutates `img`/`anim` through
        // map-interior pointers while calling `self.markImageContentChanged`
        // (a self-alias). Collect-ids-first + per-id re-lookup; the mark is
        // deferred to after the per-image borrow ends (it only stamps
        // dirty/generation, so the reordering is unobservable).
        let ids: Vec<u32> = self.images.keys().copied().collect();
        for id in ids {
            let mut content_changed = false;
            let mut delay: Option<u64> = None;
            {
                let img = self.images.get_mut(&id).unwrap();

                // The gates below mirror Kitty's image_is_animatable.

                let placement_count = img.metadata.placement_count;
                let data_pending = img.data.is_pending();

                // No animation state was ever attached (plain image).
                let anim = match img.animation.as_deref_mut() {
                    Some(anim) => anim,
                    None => continue,
                };

                // Stopped is the initial state of every animation: frames
                // then only change client-driven (a=a c=N), never by time.
                if anim.state == animation::State::Stopped {
                    continue;
                }

                // Only the root frame exists; there is nothing to advance
                // to yet even in the running state.
                if anim.frames.is_empty() {
                    continue;
                }

                // Unplaced images don't animate. This is our simpler
                // approximation of Kitty's "is actually drawn" visibility
                // gate; it is what stops an image that was transmitted but
                // never placed from waking the renderer forever.
                if placement_count == 0 {
                    continue;
                }

                // The base pixel data hasn't arrived yet (e.g. an image
                // restored from a snapshot); nothing can be displayed.
                if data_pending {
                    continue;
                }

                // A zero total duration means every frame is gapless and
                // no frame can ever be displayed, so the animation can
                // never advance. This also guards the gapless-skip loop
                // below from never terminating.
                if anim.duration_ms() == 0 {
                    continue;
                }

                // A finite loop budget (a=a v=N) that ran out on an
                // earlier tick froze playback on the last frame for good.
                if anim.max_loops > 0 && anim.current_loop >= anim.max_loops {
                    continue;
                }

                let shown_at: u64 = 'shown_at: {
                    // First tick since playback started (or since the
                    // current frame changed through another path, e.g.
                    // a=a c=N): the frame is considered shown as of now,
                    // and its gap starts counting from here.
                    let at = match anim.frame_shown_at_ms {
                        Some(at) => at,
                        None => break 'shown_at now_ms,
                    };

                    // A timestamp from the future means the caller's clock
                    // restarted; re-anchor rather than stalling until the
                    // old timestamp comes around again.
                    if at > now_ms {
                        break 'shown_at now_ms;
                    }

                    at
                };
                anim.frame_shown_at_ms = Some(shown_at);

                // The current frame is replaced once its gap has elapsed.
                // We advance at most one displayed frame per tick with no
                // catch-up, exactly like Kitty: if ticks lag behind the
                // gaps, the animation slows down rather than skipping.
                let mut next_at: u64 =
                    shown_at.saturating_add(anim.gap_at(anim.current_index) as u64);
                if now_ms >= next_at {
                    'advance: {
                        // Walk forward to the next displayable frame. This is
                        // a loop only because gapless (gap=0) frames are never
                        // displayed and are stepped over; the durationMs gate
                        // above guarantees a displayable frame exists.
                        let count: u32 = anim.frame_count();
                        let mut idx = anim.current_index;
                        loop {
                            let next = (idx + 1) % count;
                            if next == 0 {
                                // Wrapping past the last frame back to the
                                // root. A loading-state (a=a s=2) animation
                                // refuses the wrap: it parks on the last frame
                                // awaiting more frames from the client.
                                if anim.state == animation::State::Loading {
                                    break 'advance;
                                }

                                // Each wrap completes a loop; a finite budget
                                // that just ran out parks on the last frame.
                                anim.current_loop += 1;
                                if anim.max_loops > 0 && anim.current_loop >= anim.max_loops {
                                    break 'advance;
                                }
                            }
                            idx = next;
                            if anim.gap_at(idx) != 0 {
                                break;
                            }
                        }

                        // Show the chosen frame: restart its gap timer and
                        // stamp a fresh generation so consumers (the renderer
                        // texture cache, the C API) pick up the new pixels.
                        anim.current_index = idx;
                        anim.frame_shown_at_ms = Some(now_ms);
                        // self.markImageContentChanged(io, img); — deferred
                        // below, see the collect-ids-first PORT NOTE.
                        content_changed = true;
                        next_at = now_ms.saturating_add(anim.gap_at(idx) as u64);
                    }
                }

                // Schedule the next tick. A parked animation left next_at
                // in the past and so never schedules one; it is woken by
                // its trigger instead (a new frame arriving, or an a=a
                // command changing the state).
                if next_at > now_ms {
                    delay = Some(next_at - now_ms);
                }
            }
            if content_changed {
                self.mark_image_content_changed(id);
            }
            if let Some(d) = delay {
                min_delay = Some(match min_delay {
                    Some(m) => core::cmp::min(m, d),
                    None => d,
                });
            }
        }

        min_delay
    }
}

#[cfg(test)]
mod tests;
