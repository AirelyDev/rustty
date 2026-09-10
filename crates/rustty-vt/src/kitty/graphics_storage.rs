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
use crate::kitty::graphics_animation as animation;
use crate::kitty::graphics_command as command;
use crate::kitty::graphics_image;
use crate::kitty::graphics_image::{Image, Limits, LoadingImage, Rect};
use crate::kitty::graphics_pixel as pixel;
use crate::pagelist as PageList;
use crate::point;
use crate::screen::Screen;
use crate::size;
use crate::terminal::Terminal;
use allocator_api2::alloc::Global as Allocator;
use rustty_util::quirks::inlineAssert as assert;
use std::collections::HashMap;

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
        // PORT NOTE: `ensureUnusedCapacity` on a Rust `HashMap` is infallible
        // (it grows via the global allocator); the reserve keeps the shape.
        if !self.images.contains_key(&img.id) {
            self.images.reserve(1);
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

        // log.debug("addImage image={}", .{img.withoutData()});

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
            // the existing `Image` (its payload and animation) is dropped by
            // the `insert` below.
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
        let mut result: ScrollMargins = ScrollMargins {
            screen: s,
            restores: Vec::new(),
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
            log::warn!(target: LOG_SCOPE, "OOM adjusting image placement for scroll");
            //     continue;
            // };
            // PORT NOTE: Rust's `Vec::push` aborts on OOM, so the reference's
            // degraded-but-safe fallback arm is unreachable in this port.
            result.restores.push(Restore {
                pin,
                // SAFETY: shared read of the tracked pin, see above.
                x: unsafe { (*pin).x },
                // .y = @intCast(final_y),
                y: u32::try_from(final_y).unwrap(),
            });
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
    // PORT NOTE: `Allocator.Error!void` — the only reference failure is a
    // real allocation failure inside `rgbaFromFormat`, which aborts in this
    // port (see graphics_pixel.rs), so the port is infallible; the exec
    // callers' `catch` ENOMEM arms are carried as comments. `img: *Image` →
    // image id per the hazard-#3 note above.
    pub fn convert_image_to_rgba(&mut self, alloc: Allocator, image_id: u32) {
        let _ = &alloc;
        let (rgba, old_len) = {
            let img = self.images.get(&image_id).unwrap();
            if img.format == command::Format::Rgba {
                return;
            }
            // const old = img.data.bytes() orelse return;
            let old = match img.data.bytes() {
                Some(old) => old,
                None => return,
            };

            // const rgba = try pixel.rgbaFromFormat(alloc, img.format, old);
            (pixel::rgba_from_format(img.format, old), old.len())
        };
        {
            let img = self.images.get_mut(&image_id).unwrap();
            let new_len = rgba.len();
            // img.data.deinit(alloc); — the assignment below drops the old
            // payload.
            img.data = graphics_image::Data::Complete(rgba);
            img.format = command::Format::Rgba;
            self.total_bytes -= old_len;
            self.total_bytes += new_len;
        }
        self.mark_image_content_changed(image_id);
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
                // log.warn("failed to evict enough images for animation frame", .{});
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

    /// Clear placements intersecting the active screen, then reclaim every
    /// image with no remaining placement. Unlike protocol d=A, a terminal
    /// clear also reclaims images that were already unplaced.
    pub fn clear_screen(&mut self, alloc: Allocator, t: &mut Terminal) {
        let placements_before = self.placements.len();
        let images_before = self.images.len();

        // Delete unused placements and images. Orphaned relative
        // placements must be reaped before the image sweep so that
        // their images are reclaimable too.
        self.delete_visible_placements(alloc, t, true);
        let _ = self.remove_orphans(t.screens.active, None);
        // var image_it = self.images.iterator();
        // while (image_it.next()) |entry| { ... }
        let image_keys: Vec<u32> = self.images.keys().copied().collect();
        for k in image_keys {
            self.delete_if_unused(alloc, k);
        }

        // Mark mutated only if things changed.
        if self.placements.len() != placements_before || self.images.len() != images_before {
            self.mark_mutated();
        }
    }

    /// Delete placements, images.
    pub fn delete(&mut self, alloc: Allocator, t: &mut Terminal, cmd: command::DeleteAction) {
        // Deletes only ever remove placements/images, so comparing counts
        // before and after tells us whether anything actually changed.
        // Only then do we mark a mutation, so a delete that matches nothing
        // doesn't dirty the image state or bump the generation.
        //
        // PORT NOTE: Zig's `defer` runs on every return path; Rust has no
        // `defer`. The `match` is wrapped in a labeled block whose arm-local
        // early exits (`break 'arms`, the reference's `break :range` /
        // `break :newest` etc.) skip only the rest of that arm; the trailing
        // orphan sweep and the count comparison below always run, exactly as
        // upstream.
        let placements_before = self.placements.len();
        let images_before = self.images.len();

        'arms: {
            match cmd {
                command::DeleteAction::All(delete_images) => {
                    self.delete_visible_placements(alloc, t, delete_images)
                }

                command::DeleteAction::Id {
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

                command::DeleteAction::Newest {
                    delete,
                    image_number,
                    placement_id,
                } => {
                    // const img = self.imageByNumber(v.image_number) orelse break :newest;
                    let img = match self.image_by_number(image_number) {
                        Some(img) => img,
                        None => break 'arms,
                    };
                    self.delete_by_id(alloc, t.screens.active, img.id, placement_id, delete);
                }

                command::DeleteAction::IntersectCursor(delete_images) => {
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
                command::DeleteAction::IntersectCell { delete, x, y } => {
                    if x <= 0 || y <= 0 {
                        log::warn!(target: LOG_SCOPE, "delete intersect cell coords must be at least 1");
                        break 'arms;
                    }

                    // .x = std.math.cast(size.CellCountInt, v.x - 1) orelse break :intersect_cell,
                    let cx = match size::CellCountInt::try_from(x - 1) {
                        Ok(v) => v,
                        Err(_) => break 'arms,
                    };
                    // .y = std.math.cast(size.CellCountInt, v.y - 1) orelse break :intersect_cell,
                    // PORT NOTE: the reference casts `v.y - 1` to `size.CellCountInt`
                    // (returning on overflow); `point.Coordinate.y` is `u32` in this
                    // port, so we range-check against `CellCountInt` first, then widen.
                    let cy = match size::CellCountInt::try_from(y - 1) {
                        Ok(v) => v as u32,
                        Err(_) => break 'arms,
                    };
                    let p = point::Point::Active(point::Coordinate { x: cx, y: cy });
                    self.delete_intersecting(alloc, t, p, delete, |_p| true);
                }

                #[allow(clippy::absurd_extreme_comparisons)]
                command::DeleteAction::IntersectCellZ { delete, x, y, z } => {
                    if x <= 0 || y <= 0 {
                        log::warn!(target: LOG_SCOPE, "delete intersect cell coords must be at least 1");
                        break 'arms;
                    }

                    // .x = std.math.cast(size.CellCountInt, v.x - 1) orelse break :intersect_cell_z,
                    let cx = match size::CellCountInt::try_from(x - 1) {
                        Ok(v) => v,
                        Err(_) => break 'arms,
                    };
                    // .y = std.math.cast(size.CellCountInt, v.y - 1) orelse break :intersect_cell_z,
                    let cy = match size::CellCountInt::try_from(y - 1) {
                        Ok(v) => v as u32,
                        Err(_) => break 'arms,
                    };
                    let p = point::Point::Active(point::Coordinate { x: cx, y: cy });
                    // struct { fn filter(ctx: i32, p: Placement) bool { return p.z == ctx; } }.filter
                    self.delete_intersecting(alloc, t, p, delete, move |placement| {
                        placement.z == z
                    });
                }

                #[allow(clippy::absurd_extreme_comparisons)]
                command::DeleteAction::Column { delete, x } => {
                    if x <= 0 {
                        log::warn!(target: LOG_SCOPE, "delete column must be greater than zero");
                        break 'arms;
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
                            self.remove_placement(t.screens.active, &key);
                            if delete {
                                self.delete_if_unused(alloc, img.id);
                            }
                        }
                    }
                }

                #[allow(clippy::absurd_extreme_comparisons)]
                command::DeleteAction::Row { delete, y } => {
                    if y <= 0 {
                        log::warn!(target: LOG_SCOPE, "delete row must be greater than zero");
                        break 'arms;
                    }

                    // v.y is in active coords so we want to convert it to a pin
                    // so we can compare by page offsets.
                    let ty = match size::CellCountInt::try_from(y - 1) {
                        Ok(v) => v,
                        Err(_) => break 'arms,
                    };
                    let target_pin = match unsafe { &(*t.screens.active).pages }.pin(
                        point::Point::Active(point::Coordinate { x: 0, y: ty as u32 }),
                    ) {
                        Some(p) => p,
                        None => break 'arms,
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
                            self.remove_placement(t.screens.active, &key);
                            if delete {
                                self.delete_if_unused(alloc, img.id);
                            }
                        }
                    }
                }

                command::DeleteAction::Z { delete, z } => {
                    let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
                    for key in keys {
                        match self.placements.get(&key).unwrap().location {
                            // Relative placements carry their own z value and
                            // are matched by z deletes just like kitty does.
                            Location::Pin(_) | Location::Relative(_) => {}

                            // Virtual placeholders cannot delete by z according
                            // to the spec.
                            Location::Virtual => continue,
                        }

                        if self.placements.get(&key).unwrap().z == z {
                            let image_id = key.image_id;
                            self.remove_placement(t.screens.active, &key);
                            if delete {
                                self.delete_if_unused(alloc, image_id);
                            }
                        }
                    }
                }

                command::DeleteAction::Range {
                    delete,
                    first,
                    last,
                } => 'range: {
                    // Both bounds default to zero when omitted. An inverted range
                    // or a zero upper bound is not an error, it just selects
                    // nothing: image IDs are always non-zero.
                    if last == 0 || first > last {
                        break 'range;
                    }

                    // Remove matching placements in one pass.
                    let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
                    for key in keys {
                        if key.image_id < first || key.image_id > last {
                            continue;
                        }

                        self.remove_placement(t.screens.active, &key);
                    }

                    // Uppercase deletion also frees matching images that are now
                    // unused, including images that had no placements initially.
                    if !delete {
                        break 'range;
                    }
                    let image_keys: Vec<u32> = self.images.keys().copied().collect();
                    for image_id in image_keys {
                        if image_id >= first && image_id <= last {
                            self.delete_if_unused(alloc, image_id);
                        }
                    }
                }

                // .animation_frames => |v| self.deleteAnimationFrame(
                //     io,
                //     alloc,
                //     t.screens.active,
                //     v,
                // ),
                command::DeleteAction::AnimationFrames(v) => {
                    self.delete_animation_frame(alloc, t.screens.active, v)
                }
            }
        }

        // Deleting placements orphans any relative placements parented
        // to them (transitively). Their lifetime is tied to the parent
        // so they are removed as well, and an uppercase delete also
        // frees any image the cascade leaves without placements (the
        // per-branch deleteIfUnused calls above ran while the orphans
        // still counted as placements).
        //
        // PORT NOTE: the reference's `inline else => |v| v.delete` expands over
        // every remaining union arm; spelled out per-variant here.
        let delete_unused: bool = match cmd {
            command::DeleteAction::All(v) | command::DeleteAction::IntersectCursor(v) => v,
            command::DeleteAction::Id { delete, .. }
            | command::DeleteAction::Newest { delete, .. }
            | command::DeleteAction::IntersectCell { delete, .. }
            | command::DeleteAction::IntersectCellZ { delete, .. }
            | command::DeleteAction::Range { delete, .. }
            | command::DeleteAction::Column { delete, .. }
            | command::DeleteAction::Row { delete, .. }
            | command::DeleteAction::Z { delete, .. } => delete,
            command::DeleteAction::AnimationFrames(v) => v.delete,
        };
        let _ = self.remove_orphans(
            t.screens.active,
            if delete_unused { Some(alloc) } else { None },
        );

        if self.placements.len() != placements_before || self.images.len() != images_before {
            self.mark_mutated();
        }
    }

    /// Delete only non-virtual placements that intersect the active screen.
    /// The protocol defines d=a/A in terms of visible placements, and an
    /// uppercase delete only frees data for images selected by those
    /// placements.
    fn delete_visible_placements(
        &mut self,
        alloc: Allocator,
        t: &mut Terminal,
        delete_unused: bool,
    ) {
        // var it = self.placements.iterator();
        // while (it.next()) |entry| { ... }
        let keys: Vec<PlacementKey> = self.placements.keys().copied().collect();
        for key in keys {
            let pin = match self.placements.get(&key).unwrap().location {
                Location::Pin(pin) => pin,
                // Virtual placements are never selected by visible
                // deletes per the protocol. Relative placements are
                // removed with their parents instead (removeOrphans).
                Location::Virtual | Location::Relative(_) => continue,
            };
            // if (pin.garbage) continue;
            // SAFETY(raw-page-aliasing/graphics_storage): `pin` is a tracked
            // pin owned by the screen's pagelist; only shared reads are
            // performed and no `&mut` to the pin is live across them.
            if unsafe { (*pin).garbage } {
                continue;
            }

            // Placements anchored in the active area are necessarily visible.
            // Only compute their extent when the anchor is already in history
            // and the bottom edge may still intersect the active area.
            if unsafe { &(*t.screens.active).pages }
                .point_from_pin(point::Tag::Active, unsafe { *pin })
                .is_none()
            {
                let image = match self.image_by_id(key.image_id) {
                    Some(image) => image,
                    None => continue,
                };
                let rect = match self.placements.get(&key).unwrap().rect(&image, t) {
                    Some(rect) => rect,
                    None => continue,
                };
                if unsafe { &(*t.screens.active).pages }
                    .point_from_pin(point::Tag::Active, rect.bottom_right)
                    .is_none()
                {
                    continue;
                }
            }

            let image_id = key.image_id;
            self.remove_placement(t.screens.active, &key);
            if delete_unused {
                self.delete_if_unused(alloc, image_id);
            }
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
        let mut matched = placement_id == 0;

        // If no placement, we delete all placements with the ID
        if placement_id == 0 {
            self.remove_placements_by_image_id(s, image_id);
        } else {
            let key = PlacementKey {
                image_id,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: placement_id,
                },
            };
            if self.placements.contains_key(&key) {
                self.remove_placement(s, &key);
                matched = true;
            }
        }

        // A placement ID narrows the selection to that exact placement, so an
        // unmatched selector must not free otherwise-unreferenced image data.
        // https://sw.kovidgoyal.net/kitty/graphics-protocol/#deleting-images
        if delete_unused && matched {
            self.delete_if_unused(alloc, image_id);
        }
    }

    /// Delete an animation frame (d=f/F). Deletes never produce
    /// responses, so all failures are only logged. Kitty behaviors
    /// implemented here: on an image without extra frames a lowercase
    /// delete is a no-op while an uppercase delete removes the entire
    /// image, placements included; the frame number is clamped to the
    /// last frame and zero selects the root frame; deleting the root
    /// frame promotes frame 2 to be the new root.
    fn delete_animation_frame(
        &mut self,
        alloc: Allocator,
        s: *mut Screen,
        v: command::DeleteAnimationFrames,
    ) {
        let _ = &alloc;
        if v.image_id == 0 && v.image_number == 0 {
            log::warn!(target: LOG_SCOPE, "delete animation frames requires image id or number");
            return;
        }
        // const img = self.imagePtrByIdOrNumber(v.image_id, v.image_number) orelse { ... };
        let img_id = match self.image_ptr_by_id_or_number(v.image_id, v.image_number) {
            Some(img) => img.id,
            None => {
                log::warn!(
                    target: LOG_SCOPE,
                    "delete animation frames for unknown image id={} number={}",
                    v.image_id,
                    v.image_number
                );
                return;
            }
        };

        // const anim: *animation.Animation = anim: {
        //     if (img.animation) |anim| {
        //         if (anim.frames.items.len > 0) break :anim anim;
        //     }
        //     ...
        // };
        let has_extra_frames = {
            let img = self.images.get(&img_id).unwrap();
            img.animation
                .as_ref()
                .is_some_and(|anim| !anim.frames.is_empty())
        };
        if !has_extra_frames {
            // The image is not (or no longer) an animation. The
            // uppercase delete removes the entire image, even when it
            // still has placements.
            if !v.delete {
                return;
            }
            self.remove_placements_by_image_id(s, img_id);
            // const entry = self.images.getEntry(img.id).?;
            // self.total_bytes -= entry.value_ptr.storageSize();
            let size = self.images.get(&img_id).unwrap().storage_size();
            self.total_bytes -= size;
            // entry.value_ptr.deinit(alloc); — dropping the removed Image
            // frees its data and animation.
            self.images.remove(&img_id);
            return;
        }

        // Clamp the frame number: zero selects the root frame and
        // values past the end select the last frame.
        let count: u32 = {
            let img = self.images.get(&img_id).unwrap();
            img.animation.as_ref().unwrap().frame_count()
        };
        let mut number: u32 = core::cmp::min(v.frame, count);
        if number == 0 {
            number = 1;
        }

        if number == 1 {
            // Deleting the root frame promotes frame 2 to root. The
            // promoted frame's bytes stay reserved; only the old root
            // data is freed.
            let data_len = self.images.get(&img_id).unwrap().data.len();
            self.release_animation_bytes(data_len);
            let img = self.images.get_mut(&img_id).unwrap();
            // img.data.deinit(alloc); — the assignment below drops the old
            // root payload.
            // const promoted = anim.frames.orderedRemove(0);
            let promoted = img.animation.as_mut().unwrap().frames.remove(0);
            img.data = graphics_image::Data::Complete(promoted.data);
            img.animation.as_mut().unwrap().root_gap_ms = promoted.gap_ms;
        } else {
            // const removed = anim.frames.orderedRemove(number - 2);
            let removed = {
                let img = self.images.get_mut(&img_id).unwrap();
                img.animation
                    .as_mut()
                    .unwrap()
                    .frames
                    .remove((number - 2) as usize)
            };
            self.release_animation_bytes(removed.data.len());
            // alloc.free(removed.data); — `removed` drops here.
        }

        // Fix up the current frame.
        let removed_idx: u32 = if number == 1 { 0 } else { number - 2 };
        enum Mark {
            ContentChanged,
            Mutated,
        }
        let mark: Mark = {
            let img = self.images.get_mut(&img_id).unwrap();
            let anim = img.animation.as_mut().unwrap();
            // const remaining: u32 = @intCast(anim.frames.items.len);
            let remaining: u32 = u32::try_from(anim.frames.len()).unwrap();
            if anim.current_index > remaining {
                anim.current_index = remaining;
                anim.frame_shown_at_ms = None;
                // self.markImageContentChanged(io, img); return;
                Mark::ContentChanged
            } else if removed_idx == anim.current_index {
                anim.frame_shown_at_ms = None;
                Mark::ContentChanged
            } else {
                if removed_idx < anim.current_index {
                    anim.current_index -= 1;
                }
                Mark::Mutated
            }
        };
        match mark {
            Mark::ContentChanged => self.mark_image_content_changed(img_id),
            Mark::Mutated => self.mark_mutated(),
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

        // self.total_bytes -= entry.value_ptr.storageSize();
        self.total_bytes -= entry.storage_size();
        // entry.value_ptr.deinit(alloc); — dropping the removed Image frees
        // its data and animation.
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
                self.remove_placement(t.screens.active, &key);
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
        // Evicted placements can also orphan relative placements of
        // other images, which must be reaped along with them.
        //
        // PORT NOTE: Zig `defer if (self.images.count() != images_before) {
        // _ = self.removeOrphans(s, null); self.markMutated(io); };`. Rust has
        // no `defer`, so the count is checked at BOTH exits: the `return
        // false` inside the loop and the trailing `return true`.
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
                            let _ = self.remove_orphans(s, None);
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
                    self.remove_placement(s, &key);
                }
            }

            // const entry = self.images.getEntry(c.id).?;
            // const image_len = entry.value_ptr.storageSize();
            let image_len = self.images.get(&c.id).unwrap().storage_size();
            // log.info("evicting image id={} bytes={}", .{ c.id, image_len });

            evicted += image_len;
            self.total_bytes -= image_len;

            // entry.value_ptr.deinit(alloc); — dropping the removed Image frees
            // its data and animation.
            self.images.remove(&c.id);
        }

        if self.images.len() != images_before {
            let _ = self.remove_orphans(s, None);
            self.mark_mutated();
        }
        true
    }
}

/// How the row operation behind a scrollMarginsBegin/end pair moves
/// tracked pins, which determines how much repositioning work the
/// placements need.
//
// PORT NOTE: nested `pub const ScrollOp` on `ImageStorage` upstream; hoisted
// to module level (the `Candidate` precedent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollOp {
    /// The operation shifts the entire active window down (it
    /// creates scrollback), so every placement anchored in the
    /// active area changes active position and must be restored.
    WindowShift,

    /// The operation moves rows in place within the scrolling
    /// region: pins anchored outside the region rows keep their
    /// active position without help, so only placements anchored
    /// inside the region rows need handling. This is the common
    /// case (e.g. a status bar image beside a scrolling pane
    /// costs almost nothing per scroll).
    InPlace,
}

/// State returned by scrollMarginsBegin: the final active-area
/// position every surviving placement pin must be restored to once
/// the scrolled rows have settled. Every begin must be paired with
/// exactly one end() call after the scroll's row operations
/// complete.
//
// PORT NOTE (hazard #1, phase-5 plan): the guard holds a raw `*mut Screen`
// AND raw `*mut Pin`s while the caller's `&mut Terminal` row operations run
// between begin and end — both are dereferenced ONLY inside `end`
// (`rust-mut-args-strongly-protected`). The pins are pool-allocated tracked
// pins whose addresses are stable until `untrack_pin` (they are NOT interior
// to the placements map), and the begin/end window performs no storage
// mutation, so no pin recorded here is untracked in between (the upstream
// implicit contract, written down). The exactly-once `end()` contract is
// enforced by ownership: `end(self)` consumes the guard and `Drop` performs
// the restore, so early-error paths (`?`) mirror the Zig `defer ... end()`.
// Proven on `.port/phase-5/mirihar-scroll-margins/` (with a Miri-caught
// foil).
#[must_use = "every scrollMarginsBegin must be paired with exactly one end() after the scroll's row operations complete"]
pub struct ScrollMargins {
    screen: *mut Screen,
    // restores: std.ArrayListUnmanaged(Restore) = .empty,
    restores: Vec<Restore>,
}

// const Restore = struct { pin: *PageList.Pin, x: size.CellCountInt, y: u32 };
struct Restore {
    pin: *mut PageList::Pin,
    x: size::CellCountInt,
    y: u32,
}

impl ScrollMargins {
    /// Reposition every recorded placement pin to its post-scroll
    /// position and release the state. Must be called exactly
    /// once, after the scroll's row operations complete.
    //
    // PORT NOTE: consuming `self`; the body lives in `Drop` so that Zig's
    // `defer state.end()` on error paths is mirrored by the guard simply
    // dropping.
    pub fn end(self) {}
}

impl Drop for ScrollMargins {
    fn drop(&mut self) {
        for restore in &self.restores {
            // restore.pin.* = self.screen.pages.pin(.{ .active = .{
            //     .x = restore.x,
            //     .y = restore.y,
            // } }) orelse continue;
            //
            // SAFETY(raw-page-aliasing/graphics_storage): `screen` is the
            // screen owning every recorded tracked pin (scroll_margins_begin's
            // contract); the shared `pages` projection for the lookup ends
            // before the raw pin write, and the write targets a pool-owned
            // tracked pin with no live reference to it (the caller's row
            // operations have completed by the time the guard drops).
            let new_pin = match unsafe {
                (*self.screen)
                    .pages
                    .pin(point::Point::Active(point::Coordinate {
                        x: restore.x,
                        y: restore.y,
                    }))
            } {
                Some(p) => p,
                None => continue,
            };
            unsafe { *restore.pin = new_pin };
        }
        // self.restores.deinit(self.screen.alloc); — the owning Vec drops.
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

impl PlacementKey {
    // pub fn eql(self: PlacementKey, other: PlacementKey) bool {
    //     return std.meta.eql(self, other);
    // }
    pub fn eql(self, other: PlacementKey) -> bool {
        self == other
    }
}

/// Internal placement IDs are assigned by us for placements created
/// without an explicit ID (p=0); external IDs are client-specified.
/// The two are separate namespaces, hence the tag.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct PlacementId {
    pub tag: PlacementIdTag,
    pub id: u32,
}

impl PlacementId {
    /// Deterministic preference order used when an operation must
    /// pick a single placement out of several (kitty uses creation
    /// order, which our map doesn't track): external IDs win over
    /// internal ones, and lower IDs win within a tag.
    pub fn preferred_over(self, other: PlacementId) -> bool {
        if self.tag != other.tag {
            return self.tag == PlacementIdTag::External;
        }
        self.id < other.id
    }
}

// tag: enum(u1) { internal, external }
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PlacementIdTag {
    Internal,
    External,
}

// PORT NOTE: `Copy` matches the reference, which copies `Placement` values
// out of the map freely (`entry.value_ptr.*`, `resolveChain`'s `root`,
// `placeholderTarget`); the pin variant carries a raw pointer, which copies
// like the Zig `*Pin`.
#[derive(Debug, Clone, Copy)]
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

/// `pub const Location = union(enum) { pin, virtual, relative }`.
#[derive(Debug, Clone, Copy)]
pub enum Location {
    /// Exactly placed on a screen pin.
    //
    // PORT NOTE: the reference is `pin: *PageList.Pin` — a pointer to a *tracked*
    // pin owned by the pagelist (created via `pages.trackPin`). We hold the same
    // raw `*mut Pin`; `deinit` hands it back via `pages.untrackPin`.
    Pin(*mut PageList::Pin),

    /// Virtual placement (U=1) for unicode placeholders.
    Virtual,

    /// Placed relative to a parent placement (P=/Q=). The
    /// placement has no screen position of its own; it is
    /// positioned at render time by resolving the parent chain
    /// to its root (see resolveChain) and therefore follows the
    /// parent through scrolls automatically. Its lifetime is
    /// tied to the parent: removing any ancestor removes it
    /// too (see removeOrphans).
    Relative(Relative),
}

/// PORT NOTE: nested as `Placement.Relative` upstream; a module-level sibling
/// here (Rust cannot nest types inside structs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relative {
    /// The parent placement. Resolved to a concrete key when
    /// the placement is created, so the parent is guaranteed
    /// to exist at that point.
    pub parent: PlacementKey,

    /// Cell offsets from the parent's origin (H=/V=).
    pub horizontal_offset: i32,
    pub vertical_offset: i32,
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

    // The raw `*mut Screen` arg is the established raw-page-aliasing
    // discipline (memory `rust-mut-args-strongly-protected`); the deref is
    // covered by the SAFETY contract below (the rustty-xev precedent for this
    // allow).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn deinit(&self, s: *mut Screen) {
        match self.location {
            // .pin => |p| s.pages.untrackPin(p),
            // SAFETY(raw-page-aliasing/graphics_storage): `s` is the screen
            // owning this placement's tracked pin (the caller's contract, per
            // the `s: *mut Screen` PORT NOTE on `ImageStorage`); the `&mut`
            // projection of its `pages` field is scoped to this one call and
            // never overlaps a live borrow of `kitty_images`.
            Location::Pin(p) => unsafe { (*s).pages.untrack_pin(p) },
            // .virtual, .relative => {},
            Location::Virtual | Location::Relative(_) => {}
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

    /// Returns the requested source rectangle intersected with the image.
    /// A zero width or height requests the full corresponding image
    /// dimension before intersection, as defined by the Kitty protocol.
    pub fn source_rect(&self, image: &Image) -> SourceRect {
        let x = core::cmp::min(self.source_x, image.width);
        let y = core::cmp::min(self.source_y, image.height);
        SourceRect {
            x,
            y,
            width: core::cmp::min(
                if self.source_width > 0 {
                    self.source_width
                } else {
                    image.width
                },
                image.width - x,
            ),
            height: core::cmp::min(
                if self.source_height > 0 {
                    self.source_height
                } else {
                    image.height
                },
                image.height - y,
            ),
        }
    }

    /// Returns the placement offset clamped within the first cell. Pixel
    /// geometry may temporarily be unavailable, in which case there is no
    /// valid offset within the cell.
    pub fn cell_offset(&self, t: &Terminal) -> CellOffset {
        let cell_width: u32 = t.width_px / t.cols as u32;
        let cell_height: u32 = t.height_px / t.rows as u32;

        CellOffset {
            x: if cell_width > 0 {
                core::cmp::min(self.x_offset, cell_width - 1)
            } else {
                0
            },
            y: if cell_height > 0 {
                core::cmp::min(self.y_offset, cell_height - 1)
            } else {
                0
            },
        }
    }

    /// Returns the size of this placement's image in pixels,
    /// taking into account the source rectangle, specified
    /// rows/columns, and aspect ratio.
    pub fn pixel_size(&self, image: &Image, t: &Terminal) -> PixelSize {
        let source = self.source_rect(image);
        let width = source.width;
        let height = source.height;

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
        let cell_offset = self.cell_offset(t);

        // If we have a specified cols AND rows then we calculate
        // the width and height from them directly, we don't need
        // to adjust for aspect ratio.
        if self.columns > 0 && self.rows > 0 {
            let calc_width = Self::saturating_mul(cell_width, self.columns);
            let calc_height = Self::saturating_mul(cell_height, self.rows);

            return PixelSize {
                width: calc_width.saturating_sub(cell_offset.x),
                height: calc_height.saturating_sub(cell_offset.y),
            };
        }

        // Either the columns or the rows were specified, but not both,
        // so we need to calculate the other one based on the aspect ratio.

        // If only the columns were specified, we determine
        // the height of the image based on the aspect ratio.
        if self.columns > 0 {
            let calc_width =
                Self::saturating_mul(cell_width, self.columns).saturating_sub(cell_offset.x);
            let calc_height = Self::scale_dimension(calc_width, height, width);

            return PixelSize {
                width: calc_width,
                height: calc_height,
            };
        }

        // Otherwise, only the rows were specified, so we
        // determine the width based on the aspect ratio.
        {
            let calc_height =
                Self::saturating_mul(cell_height, self.rows).saturating_sub(cell_offset.y);
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
        // offsets are protocol-controlled and MAY saturate by design), so this
        // is `saturating_add` without a no-overflow debug_assert; `c5a3c7e2e`
        // clamps the offsets to the cell first (`cellOffset`).
        let calc_size = self.pixel_size(image, t);
        let cell_offset = self.cell_offset(t);
        GridSize {
            // std.math.divCeil(u32, calc_size.width +| cell_offset.x, t.width_px / t.cols) catch 0,
            cols: div_ceil_u32(
                calc_size.width.saturating_add(cell_offset.x),
                t.width_px / t.cols as u32,
            )
            .unwrap_or(0),
            // std.math.divCeil(u32, calc_size.height +| cell_offset.y, t.height_px / t.rows) catch 0,
            rows: div_ceil_u32(
                calc_size.height.saturating_add(cell_offset.y),
                t.height_px / t.rows as u32,
            )
            .unwrap_or(0),
        }
        // NOTE: Above `divCeil`s can only fail if the cell size is 0,
        //       in such a case it seems safe to return 0 for this.
    }

    /// Permanently clip `count` rows off the top of this placement,
    /// which currently spans `span` grid rows, by shrinking the
    /// source rectangle (the requested source rect is materialized
    /// into explicit values in the process). Used when a scroll
    /// within margins pushes the placement past the top margin.
    /// Requires 0 < count < span. Returns false if no visible
    /// source pixels would remain, in which case the placement is
    /// unmodified and should be deleted.
    fn clip_top(&mut self, image: &Image, t: &Terminal, count: u32, span: u32) -> bool {
        assert(count > 0 && count < span);
        let src = self.source_rect(image);
        let crop: u32 = if self.rows > 0 {
            // Scaled placement: each grid row shows an equal
            // fraction of the source.
            // @intCast(@as(u64, src.height) * count / span)
            u32::try_from(src.height as u64 * count as u64 / span as u64).unwrap()
        } else {
            // Native size: each grid row shows one cell height of
            // source pixels (kitty does the same).
            Self::saturating_mul(t.height_px / t.rows as u32, count)
        };
        if crop >= src.height {
            return false;
        }
        self.source_x = src.x;
        self.source_y = src.y + crop;
        self.source_width = src.width;
        self.source_height = src.height - crop;
        if self.rows > 0 {
            self.rows -= count;
        }
        true
    }

    /// clipTop, but clipping `count` rows off the bottom.
    fn clip_bottom(&mut self, image: &Image, t: &Terminal, count: u32, span: u32) -> bool {
        assert(count > 0 && count < span);
        let src = self.source_rect(image);
        // An empty source can never be clipped smaller. This also
        // guarantees we never store an explicit zero source height,
        // which would be reinterpreted as "full image".
        if src.height == 0 {
            return false;
        }
        if self.rows > 0 {
            // const crop: u32 = @intCast(@as(u64, src.height) * count / span);
            let crop: u32 = u32::try_from(src.height as u64 * count as u64 / span as u64).unwrap();
            if crop >= src.height {
                return false;
            }
            self.source_height = src.height - crop;
            self.rows -= count;
        } else {
            // The remaining rows can show this many source pixels,
            // with the first cell losing cellOffset pixels to the
            // placement's y offset.
            let visible = Self::saturating_mul(t.height_px / t.rows as u32, span - count);
            let offset = self.cell_offset(t).y;
            if visible <= offset {
                return false;
            }
            self.source_height = core::cmp::min(src.height, visible - offset);
        }
        self.source_x = src.x;
        self.source_y = src.y;
        self.source_width = src.width;
        true
    }

    /// Returns a selection of the entire rectangle this placement
    /// occupies within the screen. This can return null for a virtual
    /// placement or when unavailable pixel geometry makes it empty.
    ///
    /// Relative placements also return null: they have no screen
    /// position of their own, so geometric queries (delete by
    /// intersection, row, column, etc.) never match them directly.
    /// They are instead removed when their parent chain is removed.
    pub fn rect(&self, image: &Image, t: &Terminal) -> Option<Rect> {
        let grid_size = self.grid_size(image, t);
        let pin = match self.location {
            Location::Pin(p) => p,
            Location::Virtual | Location::Relative(_) => return None,
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

/// `pub const SourceRect = struct { x, y, width, height: u32 }` on `Placement`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Return type of `Placement::cellOffset`: `struct { x: u32, y: u32 }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellOffset {
    pub x: u32,
    pub y: u32,
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

    // test "storage: delete all visible placements and matching images"
    // (upstream `f8b40a023` DELETED the pin-era "storage: delete all placements
    // and images" test and replaced it with this one: d=A only frees images
    // selected by a matching visible placement, so the unplaced image 3
    // survives.)
    #[test]
    fn delete_all_visible_placements_and_matching_images() {
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
        s.delete(alloc, &mut t, command::DeleteAction::All(true));
        assert!(s.dirty);
        assert_eq!(1usize, s.images.len());
        assert!(s.images.contains_key(&3));
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
        s.delete(alloc, &mut t, command::DeleteAction::All(true));
        assert!(s.dirty);
        assert_eq!(1usize, s.images.len());
        assert!(s.images.contains_key(&3));
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
        s.delete(alloc, &mut t, command::DeleteAction::All(false));
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
            command::DeleteAction::Id {
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
            command::DeleteAction::Id {
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
            command::DeleteAction::Id {
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
        s.delete(alloc, &mut t, command::DeleteAction::IntersectCursor(false));
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
        s.delete(alloc, &mut t, command::DeleteAction::IntersectCursor(true));
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
        s.delete(alloc, &mut t, command::DeleteAction::IntersectCursor(true));
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
            command::DeleteAction::Column {
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
            command::DeleteAction::Column {
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
            command::DeleteAction::Row {
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
            command::DeleteAction::Row {
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

    // S190-F1: `delete`'s `DeleteAction::Column { x: 0, .. }` arm logs
    // `log.warn("delete column must be greater than zero", .{})`
    // (graphics_storage.zig:1294-1297) — the reference's 1-indexed column
    // rejects a literal 0.
    #[test]
    fn delete_column_zero_logs_and_is_a_no_op() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();
        let mut s: ImageStorage = ImageStorage::default();

        let cap = crate::log_capture::start();
        s.delete(
            alloc,
            &mut t,
            command::DeleteAction::Column {
                delete: false,
                x: 0,
            },
        );
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Warn
                    && target == "kitty_gfx"
                    && *msg == "delete column must be greater than zero"),
            "expected the delete-column-must-be-positive diagnostic, got: {messages:?}"
        );
        drop(cap);
        assert!(!s.dirty);

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
            command::DeleteAction::Range {
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
            command::DeleteAction::Range {
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
            command::DeleteAction::Range {
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
            command::DeleteAction::Range {
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

        s.delete(alloc, &mut t, command::DeleteAction::All(true));
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
        s.delete(alloc, &mut t, command::DeleteAction::All(true));
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
            command::DeleteAction::Id {
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
            command::DeleteAction::Id {
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
            Location::Virtual | Location::Relative(_) => panic!("expected pin location"),
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
            Location::Virtual | Location::Relative(_) => panic!("expected pin location"),
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
        s.delete(alloc, &mut t, command::DeleteAction::IntersectCursor(false));

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
            command::DeleteAction::IntersectCell {
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

        // Pixel offsets are protocol-controlled too. Clamp them to the cell
        // before including them in grid geometry.
        t.width_px = 2;
        t.height_px = 2;
        {
            let placement: Placement = Placement {
                x_offset: max,
                y_offset: max,
                ..Placement::with_location(Location::Virtual)
            };
            let actual = placement.grid_size(&image_1x1, &t);
            assert_eq!(1u32, actual.cols);
            assert_eq!(1u32, actual.rows);
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
            command::DeleteAction::Id {
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
            command::DeleteAction::Id {
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
    // test "storage: delete all visible placements preserves scrollback"
    #[test]
    fn delete_all_visible_placements_preserves_scrollback() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(
            alloc,
            crate::terminal::Options {
                max_scrollback_bytes: Some(4096),
                ..opts(2, 2)
            },
        )
        .unwrap();
        t.width_px = 2;
        t.height_px = 2;

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
        t.scroll_up(1).unwrap();

        let history_key: PlacementKey = PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1,
            },
        };
        {
            let pin = match s.placements.get(&history_key).unwrap().location {
                Location::Pin(pin) => pin,
                _ => unreachable!(),
            };
            assert!(unsafe { &(*t.screens.active).pages }
                .point_from_pin(point::Tag::Active, unsafe { *pin })
                .is_none());
        }

        s.add_placement(
            alloc,
            t.screens.active,
            1,
            2,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap(),
            )),
        )
        .unwrap();

        s.delete(alloc, &mut t, command::DeleteAction::All(true));
        assert_eq!(1usize, s.placements.len());
        assert!(s.placements.contains_key(&history_key));
        assert_eq!(1usize, s.images.len());
        assert!(s.images.contains_key(&1));

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: delete all includes placements spanning into active area"
    #[test]
    fn delete_all_includes_placements_spanning_into_active_area() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(
            alloc,
            crate::terminal::Options {
                max_scrollback_bytes: Some(4096),
                ..opts(2, 2)
            },
        )
        .unwrap();
        t.width_px = 2;
        t.height_px = 2;

        let mut s: ImageStorage = ImageStorage::default();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 1,
                height: 2,
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
        t.scroll_up(1).unwrap();

        let placement = *s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 1,
                },
            })
            .unwrap();
        {
            let pin = match placement.location {
                Location::Pin(pin) => pin,
                _ => unreachable!(),
            };
            assert!(unsafe { &(*t.screens.active).pages }
                .point_from_pin(point::Tag::Active, unsafe { *pin })
                .is_none());
        }
        let rect = placement.rect(&s.image_by_id(1).unwrap(), &t).unwrap();
        assert!(unsafe { &(*t.screens.active).pages }
            .point_from_pin(point::Tag::Active, rect.bottom_right)
            .is_some());

        s.delete(alloc, &mut t, command::DeleteAction::All(true));
        assert_eq!(0usize, s.placements.len());
        assert_eq!(0usize, s.images.len());

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: uppercase id delete preserves image when placement does not match"
    #[test]
    fn storage_uppercase_id_delete_preserves_image_when_placement_does_not_match() {
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

        s.dirty = false;
        let generation = s.generation;
        s.delete(
            alloc,
            &mut t,
            command::DeleteAction::Id {
                delete: true,
                image_id: 1,
                placement_id: 7,
            },
        );

        assert!(s.image_by_id(1).is_some());
        assert!(!s.dirty);
        assert_eq!(generation, s.generation);

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: uppercase id delete frees image after placement matches"
    #[test]
    fn storage_uppercase_id_delete_frees_image_after_placement_matches() {
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
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            9,
            Placement::with_location(Location::Pin(
                track_pin(&mut t, point::Coordinate { x: 1, y: 1 }).unwrap(),
            )),
        )
        .unwrap();

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::DeleteAction::Id {
                delete: true,
                image_id: 1,
                placement_id: 9,
            },
        );

        assert_eq!(0usize, s.placements.len());
        assert_eq!(0usize, s.images.len());
        assert!(s.dirty);
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: uppercase id delete without placement frees unplaced image"
    #[test]
    fn storage_uppercase_id_delete_without_placement_frees_unplaced_image() {
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

        s.dirty = false;
        s.delete(
            alloc,
            &mut t,
            command::DeleteAction::Id {
                delete: true,
                image_id: 1,
                placement_id: 0,
            },
        );

        assert_eq!(0usize, s.images.len());
        assert!(s.dirty);

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: uppercase range deletes unplaced image data"
    #[test]
    fn storage_uppercase_range_deletes_unplaced_image_data() {
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

        s.delete(
            alloc,
            &mut t,
            command::DeleteAction::Range {
                delete: true,
                // Zero is the default lower bound when x is omitted.
                first: 0,
                last: 2,
            },
        );
        assert_eq!(1usize, s.images.len());
        assert!(s.images.contains_key(&3));

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: delete images by empty range"
    #[test]
    fn delete_images_by_empty_range() {
        // Ranges that select nothing are a silent no-op, matching kitty, which
        // performs no validation and filters with `x <= image_id <= y`.
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

        // Inverted range.
        s.delete(
            alloc,
            &mut t,
            command::DeleteAction::Range {
                delete: true,
                first: 5,
                last: 4,
            },
        );
        assert_eq!(2usize, s.images.len());
        assert_eq!(2usize, s.placements.len());

        // Upper bound omitted, so it defaults to zero.
        s.delete(
            alloc,
            &mut t,
            command::DeleteAction::Range {
                delete: true,
                first: 5,
                last: 0,
            },
        );
        assert_eq!(2usize, s.images.len());
        assert_eq!(2usize, s.placements.len());

        // Both bounds omitted. Image IDs are never zero so this matches nothing.
        s.delete(
            alloc,
            &mut t,
            command::DeleteAction::Range {
                delete: true,
                first: 0,
                last: 0,
            },
        );
        assert_eq!(2usize, s.images.len());
        assert_eq!(2usize, s.placements.len());

        // Both placements survive, so both of their pins are still tracked.
        assert_eq!(
            tracked + 2,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: erase display preserves scrollback and reclaims unplaced images"
    #[test]
    fn storage_erase_display_preserves_scrollback_and_reclaims_unplaced_images() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(
            alloc,
            crate::terminal::Options {
                max_scrollback_bytes: Some(4096),
                ..opts(2, 2)
            },
        )
        .unwrap();
        t.width_px = 2;
        t.height_px = 2;

        // const s = &t.screens.active.kitty_images;
        // PORT NOTE: this test drives the TERMINAL's own storage (so
        // `t.eraseDisplay` reaches it); every access re-derives the reference
        // from the raw screen pointer so no alias survives a `&mut t` call.
        let sp = t.screens.active;
        unsafe { &mut (*sp).kitty_images }
            .add_image(
                alloc,
                sp,
                Image {
                    id: 1,
                    width: 1,
                    height: 1,
                    ..Image::default()
                },
            )
            .unwrap();
        {
            let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
            unsafe { &mut (*sp).kitty_images }
                .add_placement(
                    alloc,
                    sp,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        t.scroll_up(1).unwrap();

        let history_key: PlacementKey = PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1,
            },
        };
        {
            let s = unsafe { &(*sp).kitty_images };
            let pin = match s.placements.get(&history_key).unwrap().location {
                Location::Pin(pin) => pin,
                _ => unreachable!(),
            };
            assert!(unsafe { &(*sp).pages }
                .point_from_pin(point::Tag::Active, unsafe { *pin })
                .is_none());
        }

        unsafe { &mut (*sp).kitty_images }
            .add_image(
                alloc,
                sp,
                Image {
                    id: 2,
                    width: 1,
                    height: 1,
                    ..Image::default()
                },
            )
            .unwrap();
        {
            let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
            unsafe { &mut (*sp).kitty_images }
                .add_placement(
                    alloc,
                    sp,
                    2,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        unsafe { &mut (*sp).kitty_images }
            .add_image(
                alloc,
                sp,
                Image {
                    id: 3,
                    width: 1,
                    height: 1,
                    ..Image::default()
                },
            )
            .unwrap();

        t.erase_display(crate::csi::EraseDisplay::Complete, false);
        {
            let s = unsafe { &(*sp).kitty_images };
            assert_eq!(1usize, s.placements.len());
            assert!(s.placements.contains_key(&history_key));
            assert_eq!(1usize, s.images.len());
            assert!(s.images.contains_key(&1));
            assert!(!s.images.contains_key(&2));
            assert!(!s.images.contains_key(&3));
        }

        t.deinit(alloc);
    }

    // test "storage: cell offsets stay within explicit destination rectangle"
    #[test]
    fn storage_cell_offsets_stay_within_explicit_destination_rectangle() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        t.width_px = 50; // 10 px per col
        t.height_px = 100; // 20 px per row

        // Explicit columns and rows describe the far cell boundaries. Offsets
        // move the near edge inward without moving those far edges.
        let placement: Placement = Placement {
            x_offset: 3,
            y_offset: 4,
            columns: 2,
            rows: 1,
            ..Placement::with_location(Location::Virtual)
        };
        let image_4x3 = Image {
            width: 4,
            height: 3,
            ..Image::default()
        };
        let actual = placement.pixel_size(&image_4x3, &t);
        assert_eq!(17u32, actual.width);
        assert_eq!(16u32, actual.height);
        assert_eq!(2u32, placement.grid_size(&image_4x3, &t).cols);
        assert_eq!(1u32, placement.grid_size(&image_4x3, &t).rows);

        t.deinit(alloc);
    }

    // test "storage: cell offsets clamp to cell bounds"
    #[test]
    fn storage_cell_offsets_clamp_to_cell_bounds() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        t.width_px = 50; // 10 px per col
        t.height_px = 100; // 20 px per row

        let placement: Placement = Placement {
            x_offset: u32::MAX,
            y_offset: u32::MAX,
            columns: 1,
            rows: 1,
            ..Placement::with_location(Location::Virtual)
        };
        let offset = placement.cell_offset(&t);
        assert_eq!(9u32, offset.x);
        assert_eq!(19u32, offset.y);

        // Even hostile offsets leave one pixel inside the requested cell.
        let image_1x1 = Image {
            width: 1,
            height: 1,
            ..Image::default()
        };
        let actual = placement.pixel_size(&image_1x1, &t);
        assert_eq!(1u32, actual.width);
        assert_eq!(1u32, actual.height);

        t.deinit(alloc);
    }

    // test "storage: default source rectangle is intersected before sizing"
    #[test]
    fn storage_default_source_rectangle_is_intersected_before_sizing() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        let placement: Placement = Placement {
            source_x: 3,
            source_y: 1,
            ..Placement::with_location(Location::Virtual)
        };
        let actual = placement.pixel_size(
            &Image {
                width: 4,
                height: 3,
                ..Image::default()
            },
            &t,
        );
        assert_eq!(1u32, actual.width);
        assert_eq!(2u32, actual.height);

        t.deinit(alloc);
    }

    // test "storage: explicit source rectangle is intersected before sizing"
    #[test]
    fn storage_explicit_source_rectangle_is_intersected_before_sizing() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        // The requested 8x8 rectangle intersects this 10x10 image as 2x3.
        // With a two-column destination, the clipped 2:3 aspect ratio produces
        // a 20x30 pixel destination.
        let placement: Placement = Placement {
            source_x: 8,
            source_y: 7,
            source_width: 8,
            source_height: 8,
            columns: 2,
            ..Placement::with_location(Location::Virtual)
        };
        let actual = placement.pixel_size(
            &Image {
                width: 10,
                height: 10,
                ..Image::default()
            },
            &t,
        );
        assert_eq!(20u32, actual.width);
        assert_eq!(30u32, actual.height);

        t.deinit(alloc);
    }

    // test "storage: nextImageId number matches Kitty get_free_client_id"
    #[test]
    fn storage_next_image_id_number_matches_kitty_get_free_client_id() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();

        // Empty storage returns 1, as does Kitty.
        assert_eq!(1u32, s.next_image_id_for(NextImageIdMode::Explicit));

        // Contiguous IDs from 1: the next ID after them.
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
        assert_eq!(4u32, s.next_image_id_for(NextImageIdMode::Explicit));

        // The first gap is filled, even when higher IDs exist.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 100,
                ..Image::default()
            },
        )
        .unwrap();
        s.delete(
            alloc,
            &mut t,
            command::DeleteAction::Id {
                image_id: 2,
                delete: true,
                placement_id: 0,
            },
        );
        assert_eq!(2u32, s.next_image_id_for(NextImageIdMode::Explicit));

        // 1 is reused as soon as it is free.
        s.delete(
            alloc,
            &mut t,
            command::DeleteAction::Id {
                image_id: 1,
                delete: true,
                placement_id: 0,
            },
        );
        assert_eq!(1u32, s.next_image_id_for(NextImageIdMode::Explicit));

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: nextImageId implicit skips in-use ids and zero"
    #[test]
    fn storage_next_image_id_implicit_skips_in_use_ids_and_zero() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(3, 3)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();

        // A client image already sits on the counter's first two values.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2147483647,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2147483648,
                ..Image::default()
            },
        )
        .unwrap();
        assert_eq!(
            2147483649u32,
            s.next_image_id_for(NextImageIdMode::Implicit)
        );
        assert_eq!(
            2147483650u32,
            s.next_image_id_for(NextImageIdMode::Implicit)
        );

        // Wrapping skips zero, which means "no ID" protocol-wide.
        s.next_image_id = u32::MAX;
        assert_eq!(u32::MAX, s.next_image_id_for(NextImageIdMode::Implicit));
        assert_eq!(1u32, s.next_image_id_for(NextImageIdMode::Implicit));

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: resolveChain accumulates offsets to the pin root"
    #[test]
    fn storage_resolve_chain_accumulates_offsets_to_the_pin_root() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

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
            Placement::with_location(Location::Relative(Relative {
                parent: PlacementKey {
                    image_id: 1,
                    placement_id: PlacementId {
                        tag: PlacementIdTag::External,
                        id: 1,
                    },
                },
                horizontal_offset: 2,
                vertical_offset: 1,
            })),
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            3,
            Placement::with_location(Location::Relative(Relative {
                parent: PlacementKey {
                    image_id: 1,
                    placement_id: PlacementId {
                        tag: PlacementIdTag::External,
                        id: 2,
                    },
                },
                horizontal_offset: -1,
                vertical_offset: 4,
            })),
        )
        .unwrap();

        let grandchild = *s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 3,
                },
            })
            .unwrap();
        let rel = match grandchild.location {
            Location::Relative(rel) => rel,
            _ => unreachable!(),
        };
        let chain = s.resolve_chain(rel).unwrap();
        assert!(chain.root_key.eql(PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1,
            },
        }));
        assert!(matches!(chain.root.location, Location::Pin(_)));
        assert_eq!(1i32, chain.horizontal_offset);
        assert_eq!(5i32, chain.vertical_offset);

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: resolveChain finds virtual roots"
    #[test]
    fn storage_resolve_chain_finds_virtual_roots() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

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
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            2,
            Placement::with_location(Location::Relative(Relative {
                parent: PlacementKey {
                    image_id: 1,
                    placement_id: PlacementId {
                        tag: PlacementIdTag::External,
                        id: 1,
                    },
                },
                horizontal_offset: 1,
                vertical_offset: 0,
            })),
        )
        .unwrap();

        let child = *s
            .placements
            .get(&PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 2,
                },
            })
            .unwrap();
        let rel = match child.location {
            Location::Relative(rel) => rel,
            _ => unreachable!(),
        };
        let chain = s.resolve_chain(rel).unwrap();
        assert!(matches!(chain.root.location, Location::Virtual));
        assert!(chain.root_key.eql(PlacementKey {
            image_id: 1,
            placement_id: PlacementId {
                tag: PlacementIdTag::External,
                id: 1,
            },
        }));
        assert_eq!(1i32, chain.horizontal_offset);
        assert_eq!(0i32, chain.vertical_offset);

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: eviction removes orphaned relative placements"
    #[test]
    fn storage_eviction_removes_orphaned_relative_placements() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        let mut s: ImageStorage = ImageStorage::default();
        s.total_limit = 8;

        // Image 1 holds most of the byte budget and has a pin placement.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 2,
                height: 1,
                data: Data::Complete(vec![0, 0, 0, 0, 0, 0]),
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

        // Image 2's placement is relative to image 1's placement.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 2,
                ..Image::default()
            },
        )
        .unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            2,
            1,
            Placement::with_location(Location::Relative(Relative {
                parent: PlacementKey {
                    image_id: 1,
                    placement_id: PlacementId {
                        tag: PlacementIdTag::External,
                        id: 1,
                    },
                },
                horizontal_offset: 0,
                vertical_offset: 0,
            })),
        )
        .unwrap();

        // Adding image 3 exceeds the limit and evicts image 1 (oldest),
        // removing its placement, which orphans image 2's placement.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 3,
                width: 2,
                height: 1,
                data: Data::Complete(vec![0, 0, 0, 0, 0, 0]),
                ..Image::default()
            },
        )
        .unwrap();

        assert!(s.image_by_id(1).is_none());
        assert_eq!(0usize, s.placements.len());
        assert!(s.image_by_id(2).is_some());

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: placeholderTarget lookup"
    #[test]
    fn storage_placeholder_target_lookup() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

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
            5,
            Placement::with_location(Location::Virtual),
        )
        .unwrap();

        // Exact external ID match.
        {
            let target = s.placeholder_target(1, 5).unwrap();
            let expected: PlacementKey = PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 5,
                },
            };
            assert_eq!(expected, target.key);
            assert!(matches!(target.placement.location, Location::Virtual));
        }

        // Zero placement ID falls back to the image's virtual placement.
        {
            let target = s.placeholder_target(1, 0).unwrap();
            let expected: PlacementKey = PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 5,
                },
            };
            assert_eq!(expected, target.key);
        }

        assert!(s.placeholder_target(1, 9).is_none());
        assert!(s.placeholder_target(2, 0).is_none());

        // With multiple virtual placements, the zero-ID fallback picks
        // deterministically by PlacementId.preferredOver: lowest external.
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            3,
            Placement::with_location(Location::Virtual),
        )
        .unwrap();
        {
            let expected: PlacementKey = PlacementKey {
                image_id: 1,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id: 3,
                },
            };
            assert_eq!(expected, s.placeholder_target(1, 0).unwrap().key);
        }

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    /// Test helper: the reference reads
    /// `t.screens.active.pages.pointFromPin(.active, p.location.pin.*)` for a
    /// placement fetched with `storage.placements.get(...)`.
    fn external_placement_coord(t: &Terminal, image_id: u32, id: u32) -> Option<point::Coordinate> {
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let p = storage
            .placements
            .get(&PlacementKey {
                image_id,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id,
                },
            })
            .unwrap();
        let pin = match p.location {
            Location::Pin(pin) => pin,
            Location::Virtual | Location::Relative(_) => unreachable!(),
        };
        // SAFETY: `pin` is a live tracked pin owned by the pagelist.
        unsafe {
            (*t.screens.active)
                .pages
                .point_from_pin(point::Tag::Active, *pin)
        }
        .map(|pt| pt.coord())
    }

    /// Test helper: fetch the external placement (copy) for assertions on its
    /// source rectangle / rows / columns fields.
    fn external_placement(t: &Terminal, image_id: u32, id: u32) -> Placement {
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let p = storage
            .placements
            .get(&PlacementKey {
                image_id,
                placement_id: PlacementId {
                    tag: PlacementIdTag::External,
                    id,
                },
            })
            .unwrap();
        Placement {
            location: Location::Virtual, // location is asserted via the coord helper
            ..*p
        }
    }

    // test "storage: scroll margins placement inside region scrolls and clips at top"
    #[test]
    fn scroll_margins_placement_inside_region_scrolls_and_clips_at_top() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        // 2-row image (10x10px cells) inside the region (rows 1-4).
        // const storage: *ImageStorage = &t.screens.active.kitty_images;
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
                    alloc,
                    t.screens.active,
                    Image {
                        id: 1,
                        width: 10,
                        height: 20,
                        ..Image::default()
                    },
                )
                .unwrap();
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        t.set_top_and_bottom_margin(2, 5);
        t.set_cursor_pos(5, 1);

        // Entirely inside: moves up with the content.
        t.index().unwrap();
        {
            assert_eq!(
                point::Coordinate { x: 0, y: 1 },
                external_placement_coord(&t, 1, 1).unwrap()
            );
        }

        // Would extend above the top margin: anchored at the top margin
        // with the top cell row clipped off the source.
        t.index().unwrap();
        {
            assert_eq!(
                point::Coordinate { x: 0, y: 1 },
                external_placement_coord(&t, 1, 1).unwrap()
            );
            let p = external_placement(&t, 1, 1);
            assert_eq!(10u32, p.source_y);
            assert_eq!(10u32, p.source_height);
        }

        // Fully clipped: deleted.
        t.index().unwrap();
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            assert_eq!(0usize, storage.placements.len());
            // The image itself is retained.
            assert_eq!(1usize, storage.images.len());
        }

        t.deinit(alloc);
    }

    // test "storage: scroll margins placement straddling region does not move"
    #[test]
    fn scroll_margins_placement_straddling_region_does_not_move() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        // 3-row image anchored at the top of a 2-row region: not entirely
        // within the region, so region scrolls must not move it. This
        // exercises the scrollback-creating path (region top at row 0).
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
                    alloc,
                    t.screens.active,
                    Image {
                        id: 1,
                        width: 10,
                        height: 30,
                        ..Image::default()
                    },
                )
                .unwrap();
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        t.set_top_and_bottom_margin(1, 2);
        t.set_cursor_pos(2, 1);

        t.index().unwrap();
        assert_eq!(
            point::Coordinate { x: 0, y: 0 },
            external_placement_coord(&t, 1, 1).unwrap()
        );
        let p = external_placement(&t, 1, 1);
        assert_eq!(0u32, p.source_y);
        assert_eq!(0u32, p.source_height);

        t.deinit(alloc);
    }

    // test "storage: scroll margins placement below region does not move"
    #[test]
    fn scroll_margins_placement_below_region_does_not_move() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        // A "status bar" image below the bottom margin must stay put while
        // the region above it scrolls (scrollback-creating path).
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 5 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
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
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        t.set_top_and_bottom_margin(1, 4);
        t.set_cursor_pos(4, 1);

        t.index().unwrap();
        t.index().unwrap();
        assert_eq!(
            point::Coordinate { x: 0, y: 5 },
            external_placement_coord(&t, 1, 1).unwrap()
        );

        t.deinit(alloc);
    }

    // test "storage: scroll margins reverse index clips at bottom"
    #[test]
    fn scroll_margins_reverse_index_clips_at_bottom() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        // 2-row image at the bottom of the region (rows 1-4).
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 3 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
                    alloc,
                    t.screens.active,
                    Image {
                        id: 1,
                        width: 10,
                        height: 20,
                        ..Image::default()
                    },
                )
                .unwrap();
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        t.set_top_and_bottom_margin(2, 5);
        t.set_cursor_pos(2, 1);

        // Moves down, with the bottom cell row clipped off the source.
        t.reverse_index();
        {
            assert_eq!(
                point::Coordinate { x: 0, y: 4 },
                external_placement_coord(&t, 1, 1).unwrap()
            );
            let p = external_placement(&t, 1, 1);
            assert_eq!(0u32, p.source_y);
            assert_eq!(10u32, p.source_height);
        }

        // Fully clipped: deleted.
        t.reverse_index();
        assert_eq!(
            0usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );

        t.deinit(alloc);
    }

    // test "storage: scroll margins scaled placement clips proportionally"
    #[test]
    fn scroll_margins_scaled_placement_clips_proportionally() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        // A 40px-tall image scaled into 2 rows, inside a 2-row region.
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 1 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
                    alloc,
                    t.screens.active,
                    Image {
                        id: 1,
                        width: 10,
                        height: 40,
                        ..Image::default()
                    },
                )
                .unwrap();
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement {
                        columns: 1,
                        rows: 2,
                        ..Placement::with_location(Location::Pin(pin))
                    },
                )
                .unwrap();
        }
        t.set_top_and_bottom_margin(2, 3);
        t.set_cursor_pos(3, 1);

        t.index().unwrap();
        assert_eq!(
            point::Coordinate { x: 0, y: 1 },
            external_placement_coord(&t, 1, 1).unwrap()
        );
        let p = external_placement(&t, 1, 1);
        assert_eq!(20u32, p.source_y);
        assert_eq!(20u32, p.source_height);
        assert_eq!(1u32, p.rows);
        assert_eq!(1u32, p.columns);

        t.deinit(alloc);
    }

    // test "storage: scroll margins left/right margins respect columns"
    #[test]
    fn scroll_margins_left_right_margins_respect_columns() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        // One placement outside the left/right margin columns, one inside.
        let pin1 = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
        let pin2 = track_pin(&mut t, point::Coordinate { x: 2, y: 2 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
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
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin1)),
                )
                .unwrap();
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    2,
                    Placement::with_location(Location::Pin(pin2)),
                )
                .unwrap();
        }

        t.modes
            .set(crate::modes::Mode::EnableLeftAndRightMargin, true);
        t.set_left_and_right_margin(2, 8);

        t.scroll_up(1).unwrap();

        // Outside the margin columns: stays.
        assert_eq!(
            point::Coordinate { x: 0, y: 2 },
            external_placement_coord(&t, 1, 1).unwrap()
        );

        // Inside: moves up.
        assert_eq!(
            point::Coordinate { x: 2, y: 1 },
            external_placement_coord(&t, 1, 2).unwrap()
        );

        t.deinit(alloc);
    }

    // test "storage: scroll margins insert/delete lines do not move placements"
    #[test]
    fn scroll_margins_insert_delete_lines_do_not_move_placements() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        // Kitty does not scroll images for IL/DL, only for IND/RI/SU/SD.
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
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
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        t.set_top_and_bottom_margin(2, 5);
        t.set_cursor_pos(2, 1);

        t.delete_lines(1);
        assert_eq!(
            point::Coordinate { x: 0, y: 2 },
            external_placement_coord(&t, 1, 1).unwrap()
        );

        t.insert_lines(1);
        assert_eq!(
            point::Coordinate { x: 0, y: 2 },
            external_placement_coord(&t, 1, 1).unwrap()
        );

        t.deinit(alloc);
    }

    // test "storage: scroll without margins moves placement into scrollback"
    #[test]
    fn scroll_without_margins_moves_placement_into_scrollback() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        // No margins: the placement follows its anchored row into the
        // scrollback via pin tracking (kitty's marginless behavior).
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
                    alloc,
                    t.screens.active,
                    Image {
                        id: 1,
                        width: 10,
                        height: 20,
                        ..Image::default()
                    },
                )
                .unwrap();
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        t.set_cursor_pos(6, 1);

        t.index().unwrap();
        // try testing.expect(t.screens.active.pages.pointFromPin(.active, ...) == null);
        assert!(external_placement_coord(&t, 1, 1).is_none());
        // const pt = t.screens.active.pages.pointFromPin(.screen, p.location.pin.*).?;
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            let p = storage
                .placements
                .get(&PlacementKey {
                    image_id: 1,
                    placement_id: PlacementId {
                        tag: PlacementIdTag::External,
                        id: 1,
                    },
                })
                .unwrap();
            let pin = match p.location {
                Location::Pin(pin) => pin,
                Location::Virtual | Location::Relative(_) => unreachable!(),
            };
            // SAFETY: `pin` is a live tracked pin owned by the pagelist.
            let pt = unsafe {
                (*t.screens.active)
                    .pages
                    .point_from_pin(point::Tag::Screen, *pin)
            }
            .unwrap();
            assert_eq!(point::Coordinate { x: 0, y: 0 }, pt.coord());
        }

        t.deinit(alloc);
    }

    // test "storage: scroll margins large scroll deletes inside placement"
    #[test]
    fn scroll_margins_large_scroll_deletes_inside_placement() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
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
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        t.set_top_and_bottom_margin(2, 5);

        t.scroll_up(10).unwrap();
        assert_eq!(
            0usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );

        t.deinit(alloc);
    }

    // test "storage: scroll margins straddling placement pin inside region restored"
    #[test]
    fn scroll_margins_straddling_placement_pin_inside_region_restored() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        // 2-row image anchored on the bottom margin row, extending below
        // the region: not entirely inside, so it must not move even though
        // the region scroll moves every tracked pin within the region rows.
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 4 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
                    alloc,
                    t.screens.active,
                    Image {
                        id: 1,
                        width: 10,
                        height: 20,
                        ..Image::default()
                    },
                )
                .unwrap();
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        t.set_top_and_bottom_margin(2, 5);
        t.set_cursor_pos(5, 1);

        t.index().unwrap();
        assert_eq!(
            point::Coordinate { x: 0, y: 4 },
            external_placement_coord(&t, 1, 1).unwrap()
        );
        let p = external_placement(&t, 1, 1);
        assert_eq!(0u32, p.source_y);
        assert_eq!(0u32, p.source_height);

        t.deinit(alloc);
    }

    // test "storage: scroll margins multi-line scroll up with scrollback"
    #[test]
    fn scroll_margins_multi_line_scroll_up_with_scrollback() {
        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 6)).unwrap();
        t.width_px = 100;
        t.height_px = 60;

        // 1-row image inside a region whose top is the screen top, so the
        // scroll pushes text into the scrollback (window shift) while the
        // image moves within the region.
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 2 }).unwrap();
        {
            let storage = unsafe { &mut (*t.screens.active).kitty_images };
            storage
                .add_image(
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
            storage
                .add_placement(
                    alloc,
                    t.screens.active,
                    1,
                    1,
                    Placement::with_location(Location::Pin(pin)),
                )
                .unwrap();
        }
        t.set_top_and_bottom_margin(1, 4);

        t.scroll_up(2).unwrap();
        {
            assert_eq!(
                point::Coordinate { x: 0, y: 0 },
                external_placement_coord(&t, 1, 1).unwrap()
            );
        }

        // Another two rows scrolls it out of the region: deleted.
        t.scroll_up(2).unwrap();
        assert_eq!(
            0usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );

        t.deinit(alloc);
    }

    // test "storage: animation tick advances and schedules"
    #[test]
    fn animation_tick_advances_and_schedules() {
        use crate::kitty::graphics_animation::{Animation, Frame, State};

        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut s: ImageStorage = ImageStorage::default();

        // A running 1x1 RGBA image whose animation has one extra frame.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 1,
                height: 1,
                format: command::Format::Rgba,
                data: Data::Complete(vec![255, 0, 0, 255]),
                ..Image::default()
            },
        )
        .unwrap();
        // const img = s.images.getPtr(1).?;
        // const anim = try alloc.create(animation.Animation);
        // anim.* = .{ .state = .running };
        // img.animation = anim;
        // try anim.frames.append(alloc, .{ .data = ..., .gap_ms = 40 });
        {
            let img = s.images.get_mut(&1).unwrap();
            img.animation = Some(Box::new(Animation {
                state: State::Running,
                ..Animation::default()
            }));
            img.animation.as_mut().unwrap().frames.push(Frame {
                data: vec![0, 0, 255, 255],
                gap_ms: 40,
            });
        }

        // Without a placement the animation doesn't advance (our
        // approximation of Kitty's visibility gate).
        assert!(s.animation_tick(0).is_none());
        assert_eq!(
            0u32,
            s.images
                .get(&1)
                .unwrap()
                .animation
                .as_ref()
                .unwrap()
                .current_index
        );

        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            0,
            Placement::with_location(Location::Pin(pin)),
        )
        .unwrap();
        let gen1 = s.images.get(&1).unwrap().generation;
        s.dirty = false;

        // First tick: the gapless root frame is due immediately and is
        // skipped over to frame 2, which is due again in its 40ms gap.
        assert_eq!(Some(40u64), s.animation_tick(0));
        assert_eq!(
            1u32,
            s.images
                .get(&1)
                .unwrap()
                .animation
                .as_ref()
                .unwrap()
                .current_index
        );
        assert!(s.images.get(&1).unwrap().generation > gen1);
        assert!(s.dirty);

        // Nothing due yet: no advance, and the delay counts down.
        let gen2 = s.images.get(&1).unwrap().generation;
        assert_eq!(Some(30u64), s.animation_tick(10));
        assert_eq!(gen2, s.images.get(&1).unwrap().generation);

        // Wrapping is fine with an infinite loop budget: the gapless
        // root is skipped and frame 2 is shown again.
        assert_eq!(Some(40u64), s.animation_tick(40));
        {
            let anim = s.images.get(&1).unwrap().animation.as_ref().unwrap();
            assert_eq!(1u32, anim.current_index);
            assert_eq!(1u32, anim.current_loop);
        }

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: animation tick loading state parks on last frame"
    #[test]
    fn animation_tick_loading_state_parks_on_last_frame() {
        use crate::kitty::graphics_animation::{Animation, Frame, State};

        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut s: ImageStorage = ImageStorage::default();

        // A placed 1x1 RGBA image in the loading state (a=a s=2) whose
        // animation has one extra frame.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 1,
                height: 1,
                format: command::Format::Rgba,
                data: Data::Complete(vec![255, 0, 0, 255]),
                ..Image::default()
            },
        )
        .unwrap();
        {
            let img = s.images.get_mut(&1).unwrap();
            img.animation = Some(Box::new(Animation {
                state: State::Loading,
                ..Animation::default()
            }));
            img.animation.as_mut().unwrap().frames.push(Frame {
                data: vec![0, 0, 255, 255],
                gap_ms: 40,
            });
        }
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            0,
            Placement::with_location(Location::Pin(pin)),
        )
        .unwrap();

        // Reach the last frame, then park: no wakeup is scheduled while
        // waiting for more frames and the loop counter stays untouched.
        assert_eq!(Some(40u64), s.animation_tick(0));
        assert_eq!(
            1u32,
            s.images
                .get(&1)
                .unwrap()
                .animation
                .as_ref()
                .unwrap()
                .current_index
        );
        assert!(s.animation_tick(100).is_none());
        {
            let anim = s.images.get(&1).unwrap().animation.as_ref().unwrap();
            assert_eq!(1u32, anim.current_index);
            assert_eq!(0u32, anim.current_loop);
        }

        // A new frame arriving un-parks playback.
        s.images
            .get_mut(&1)
            .unwrap()
            .animation
            .as_mut()
            .unwrap()
            .frames
            .push(Frame {
                data: vec![0, 255, 0, 255],
                gap_ms: 25,
            });
        assert_eq!(Some(25u64), s.animation_tick(150));
        assert_eq!(
            2u32,
            s.images
                .get(&1)
                .unwrap()
                .animation
                .as_ref()
                .unwrap()
                .current_index
        );

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: animation tick exhausts loop budget"
    #[test]
    fn animation_tick_exhausts_loop_budget() {
        use crate::kitty::graphics_animation::{Animation, Frame, State};

        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut s: ImageStorage = ImageStorage::default();

        // A placed, running 1x1 RGBA image with a gapped root frame, one
        // extra frame, and a one-loop budget (a=a v=2).
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 1,
                height: 1,
                format: command::Format::Rgba,
                data: Data::Complete(vec![255, 0, 0, 255]),
                ..Image::default()
            },
        )
        .unwrap();
        {
            let img = s.images.get_mut(&1).unwrap();
            img.animation = Some(Box::new(Animation {
                state: State::Running,
                root_gap_ms: 10,
                max_loops: 1,
                ..Animation::default()
            }));
            img.animation.as_mut().unwrap().frames.push(Frame {
                data: vec![0, 0, 255, 255],
                gap_ms: 40,
            });
        }
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            0,
            Placement::with_location(Location::Pin(pin)),
        )
        .unwrap();

        // Root shows for 10ms, frame 2 for 40ms, then the wrap exhausts
        // the budget and playback freezes on the last frame for good.
        assert_eq!(Some(10u64), s.animation_tick(0));
        assert_eq!(
            0u32,
            s.images
                .get(&1)
                .unwrap()
                .animation
                .as_ref()
                .unwrap()
                .current_index
        );
        assert_eq!(Some(40u64), s.animation_tick(10));
        assert_eq!(
            1u32,
            s.images
                .get(&1)
                .unwrap()
                .animation
                .as_ref()
                .unwrap()
                .current_index
        );
        assert!(s.animation_tick(50).is_none());
        assert_eq!(
            1u32,
            s.images
                .get(&1)
                .unwrap()
                .animation
                .as_ref()
                .unwrap()
                .current_index
        );
        assert!(s.animation_tick(500).is_none());

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: animation tick ignores ineligible animations"
    #[test]
    fn animation_tick_ignores_ineligible_animations() {
        use crate::kitty::graphics_animation::{Animation, Frame, State};

        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut s: ImageStorage = ImageStorage::default();

        // A placed 1x1 RGBA image with one extra frame, in the default
        // stopped state.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 1,
                height: 1,
                format: command::Format::Rgba,
                data: Data::Complete(vec![255, 0, 0, 255]),
                ..Image::default()
            },
        )
        .unwrap();
        {
            let img = s.images.get_mut(&1).unwrap();
            img.animation = Some(Box::new(Animation::default()));
            img.animation.as_mut().unwrap().frames.push(Frame {
                data: vec![0, 0, 255, 255],
                gap_ms: 40,
            });
        }
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            0,
            Placement::with_location(Location::Pin(pin)),
        )
        .unwrap();

        // Stopped (the default) never advances.
        assert!(s.animation_tick(0).is_none());

        // An all-gapless animation can never advance either.
        {
            let anim = s.images.get_mut(&1).unwrap().animation.as_mut().unwrap();
            anim.state = State::Running;
            anim.frames[0].gap_ms = 0;
        }
        assert!(s.animation_tick(0).is_none());
        assert_eq!(
            0u32,
            s.images
                .get(&1)
                .unwrap()
                .animation
                .as_ref()
                .unwrap()
                .current_index
        );

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }

    // test "storage: animation tick re-anchors a restarted clock"
    #[test]
    fn animation_tick_re_anchors_a_restarted_clock() {
        use crate::kitty::graphics_animation::{Animation, Frame, State};

        let alloc = allocator_api2::alloc::Global;
        let mut t = Terminal::init(alloc, opts(10, 10)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut s: ImageStorage = ImageStorage::default();

        // A placed, running 1x1 RGBA image displaying its extra frame,
        // with a shown-at timestamp far ahead of the tick clock.
        s.add_image(
            alloc,
            t.screens.active,
            Image {
                id: 1,
                width: 1,
                height: 1,
                format: command::Format::Rgba,
                data: Data::Complete(vec![255, 0, 0, 255]),
                ..Image::default()
            },
        )
        .unwrap();
        {
            let img = s.images.get_mut(&1).unwrap();
            img.animation = Some(Box::new(Animation {
                state: State::Running,
                current_index: 1,
                frame_shown_at_ms: Some(1000),
                ..Animation::default()
            }));
            img.animation.as_mut().unwrap().frames.push(Frame {
                data: vec![0, 0, 255, 255],
                gap_ms: 40,
            });
        }
        let pin = track_pin(&mut t, point::Coordinate { x: 0, y: 0 }).unwrap();
        s.add_placement(
            alloc,
            t.screens.active,
            1,
            0,
            Placement::with_location(Location::Pin(pin)),
        )
        .unwrap();

        // A timestamp in the future relative to now means the caller's
        // clock restarted; the animation must not stall until the old
        // timestamp comes around again.
        assert_eq!(Some(40u64), s.animation_tick(5));
        assert_eq!(
            Some(5u64),
            s.images
                .get(&1)
                .unwrap()
                .animation
                .as_ref()
                .unwrap()
                .frame_shown_at_ms
        );

        s.deinit(alloc, t.screens.active);
        t.deinit(alloc);
    }
}
