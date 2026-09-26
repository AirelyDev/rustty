//! The monomorphic image render state over [`crate::metal::metal_impl::Metal`] —
//! the `Image(GraphicsAPI)` instantiation for the `.metal` renderer arm, sibling
//! to `image::linux_impl`'s OpenGL twin (see the `image.rs` dispatcher).
//!
//! PORTS: ghostty `src/renderer/image.zig` — the generic Kitty-graphics +
//! overlay image render state (`State`, `Placement`, `Id`, `ImageMap`, and the
//! per-image `Image` upload state machine).
//!
//! PORT STATUS (GPU R-d): FULL — the *container / dispatch /
//! lifecycle* surface plus the Kitty-graphics pixel path
//! (`RENDERER_KITTY_IMAGE_PATH_DESIGN.md`):
//!
//! - FULL: `State::{empty, deinit, upload, draw, overlay_update,
//!   kitty_requires_update, prep_image}`, all of `Image`'s state machine
//!   (`deinit`/`mark_for_unload`/`mark_for_replace`/`is_pending`/`has_texture`/
//!   `is_unloading`/`prep_for_upload`/`upload`/`get_pending`/`get_texture`/
//!   `get_pending_pointer`), and the pure types.
//! - FILLED (R-d): `Image::convert` (the `wuffs` swizzle, re-homed as a pure-Rust
//!   per-pixel RGBA_PREMUL loop — `wuffs` has no Rust home) and the four Kitty
//!   terminal-walk methods (`kitty_update`/`prep_kitty_placement`/
//!   `prep_kitty_virtual_placement`/`prep_kitty_image`). Each keeps its verbatim
//!   Zig body as a leading block comment (R1).
//!
//! The `image.zig` reference carries **0** `test{}` blocks (golden-image-gated);
//! the `#[test]`s at the bottom are rustty-only smoke tests (the empty-map
//! lifecycle + the offscreen APC-driven Kitty pixel-path test), mirroring the
//! metal-leaf smoke-test precedent.

use std::collections::HashMap;

use core::alloc::Layout;
use core::ptr::NonNull;

use allocator_api2::alloc::{AllocError, Allocator};

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const assert = @import("../quirks.zig").inlineAssert;
// const wuffs = @import("wuffs");   // SEAM dep (see `Image::convert`)
// const terminal = @import("../terminal/main.zig");
use rustty_util::quirks::inlineAssert as assert;

use rustty_vt::kitty::graphics_command::Format;
use rustty_vt::kitty::graphics_image::Image as KittyImage;
use rustty_vt::kitty::graphics_storage::{
    ImageStorage, Location, Placement as KittyStoragePlacement, PlacementKey,
};
use rustty_vt::kitty::graphics_unicode::{self, Placement as KittyUnicodePlacement};
use rustty_vt::pagelist::Pin as PageListPin;
use rustty_vt::point;
use rustty_vt::screen::Screen;
use rustty_vt::terminal::Terminal;

// const Renderer = @import("../renderer.zig").Renderer;
// const GraphicsAPI = Renderer.API;
// const Texture = GraphicsAPI.Texture;
// PORT NOTE (§2, MONOMORPHIC): `*GraphicsAPI` → `&Metal`, `GraphicsAPI.Texture` →
// `crate::metal::texture::Texture`, `GraphicsAPI.Pipeline` → `&Pipeline`,
// `*GraphicsAPI.RenderPass` → `&RenderPass`, `Buffer(shaders.Image)` →
// `Buffer<crate::shaders::Image>`.
use crate::metal::api::MTLPrimitiveType;
use crate::metal::buffer::Buffer;
use crate::metal::metal_impl::{ImageTextureFormat, Metal};
use crate::metal::pipeline::Pipeline;
use crate::metal::render_pass::{Draw, RenderPass, Step};
use crate::metal::texture::Texture;
use crate::shaders;
use crate::size::CellSize;

use crate::overlay::Overlay;

// const log = std.log.scoped(.renderer_image);
// PORT NOTE (task-14c-2): the `log` facade is wired for this crate
// (`Cargo.toml`'s `log = { workspace = true }`; DECISION-logging.md §6r call
// shape 3). ELEVEN of `image.zig`'s thirteen `log.warn` sites are RESTORED, each
// beneath its R1 comment. The other TWO are NOT restorable and stay comments with
// a PORT NOTE at the site: `image.zig:365` (`error deferring relative placement`,
// reference `ArrayList.append(alloc, …)` is fallible, `Vec::push` is not) and
// `image.zig:447` (`error tracking virtual origin`, reference
// `getOrPut(alloc, …)` is fallible, `HashMap::entry` is not). Both are inside row
// S117-F3's cited anchors, which is why that row is WAIVED rather than CLOSED —
// see `.port/parity-closure/ADJUDICATION.md#s117-f3`.
/// `image.zig:14`'s `const log = std.log.scoped(.renderer_image);`.
const LOG_SCOPE: &str = "renderer_image";

/// Generic image rendering state for the renderer. This stores all
/// images and their placements and exposes only a limited public API
/// for adding images and placements and drawing them.
pub struct State {
    /// The full image state for the renderer that specifies what images
    /// need to be uploaded, pruned, etc.
    pub images: ImageMap,

    /// The placements for the Kitty image protocol.
    pub kitty_placements: Vec<Placement>,

    /// The end index (exclusive) for placements that should be
    /// drawn below the background, below the text, etc.
    pub kitty_bg_end: u32,
    pub kitty_text_end: u32,

    /// True if there are any virtual placements. This needs to be known
    /// because virtual placements need to be recalculated more often
    /// on frame builds and are generally more expensive to handle.
    pub kitty_virtual: bool,

    /// Overlays
    pub overlay_placements: Vec<Placement>,
}

/// Where a placement is anchored on screen: a pin, plus cell
/// offsets from it for relative placements. The pin is the
/// placement's own pin for pin placements; for relative placements
/// it is the root of the parent chain and the offsets are the
/// accumulated chain offsets in cells.
//
// const Origin = struct {
//     pin: *const terminal.Pin,
//     horizontal_offset: i32 = 0,
//     vertical_offset: i32 = 0,
// };
//
// PORT NOTE: nested inside `State` upstream; a module-level sibling here (Rust
// cannot nest types inside impls). The Zig field defaults (`= 0`) are written
// explicitly at every construction site.
//
// SAFETY (SB discipline, assessment D hazard #1): `pin` is a raw alias into a
// TRACKED pin owned by the pagelist (`Location::Pin` carries `*mut Pin`). It is
// held only across the SHARED-read walk of `kitty_update` /
// `prep_kitty_placement` — no `&mut Screen`/`&mut PageList` interleaves before
// the last deref (the only screen mutation, `dirty = false`, happens after all
// origins are dead).
#[derive(Clone, Copy)]
struct Origin {
    pin: *const PageListPin,
    horizontal_offset: i32,
    vertical_offset: i32,
}

// The anonymous element struct of the Zig `pending_relative` list, named
// because Rust has no anonymous structs.
struct PendingRelative {
    image_id: u32,
    p: KittyStoragePlacement,
    root_key: PlacementKey,
    horizontal_offset: i32,
    vertical_offset: i32,
}

// The anonymous `struct { x: u32, y: u32 }` value type of the Zig
// `virtual_origins` map, named because Rust has no anonymous structs.
#[derive(Clone, Copy)]
struct VirtualOrigin {
    x: u32,
    y: u32,
}

impl State {
    // pub const empty: State = .{ ... };
    //
    // PORT NOTE: `image.zig` declares `empty` as a `const`, but `HashMap::new()`
    // is not `const`, so this is an associated `fn empty()` returning the same
    // value (behaviorally identical to `.empty`). `AutoHashMapUnmanaged.empty` /
    // `ArrayListUnmanaged.empty` → `HashMap::new()` / `Vec::new()`.
    pub fn empty() -> State {
        State {
            images: HashMap::new(),
            kitty_placements: Vec::new(),
            kitty_bg_end: 0,
            kitty_text_end: 0,
            kitty_virtual: false,
            overlay_placements: Vec::new(),
        }
    }

    pub fn deinit<A: Allocator + Copy>(&mut self, alloc: A) {
        {
            // var it = self.images.iterator();
            // while (it.next()) |kv| kv.value_ptr.image.deinit(alloc);
            // self.images.deinit(alloc);
            //
            // PORT NOTE: `mem::take` empties `self.images` and hands us the owned
            // map, so `Image::deinit` can take `self` by value (frees any owned
            // pixel data / releases the texture). `kitty_placements` /
            // `overlay_placements` are POD `Vec`s and free on drop.
            for entry in std::mem::take(&mut self.images).into_values() {
                entry.image.deinit(alloc);
            }
        }
        // self.kitty_placements.deinit(alloc);
        self.kitty_placements = Vec::new();
        // self.overlay_placements.deinit(alloc);
        self.overlay_placements = Vec::new();
    }

    /// Upload any images to the GPU that need to be uploaded,
    /// and remove any images that are no longer needed on the GPU.
    ///
    /// If any uploads fail, they are ignored. The return value
    /// can be used to detect if upload was a total success (true)
    /// or not (false).
    pub fn upload<A: Allocator + Copy>(&mut self, alloc: A, api: &Metal) -> bool {
        let mut success: bool = true;
        // var image_it = self.images.iterator();
        //
        // PORT NOTE: Zig mutates/removes entries mid-iteration via `removeByPtr`.
        // Rust's `HashMap` forbids remove-during-iteration, so we snapshot the
        // keys first (`Id: Copy`) and walk those; the semantics are identical (we
        // visit every entry once, unloading the unloading and uploading the
        // pending).
        let keys: Vec<Id> = self.images.keys().copied().collect();
        for id in keys {
            // const img = &kv.value_ptr.image;
            // if (img.isUnloading()) { img.deinit(alloc); removeByPtr; continue; }
            let unloading = self.images.get(&id).is_some_and(|e| e.image.is_unloading());
            if unloading {
                if let Some(entry) = self.images.remove(&id) {
                    entry.image.deinit(alloc);
                }
                continue;
            }

            // if (img.isPending()) { img.upload(alloc, api) catch { success = false; } }
            let pending = self.images.get(&id).is_some_and(|e| e.image.is_pending());
            if pending {
                let img = &mut self.images.get_mut(&id).unwrap().image;
                if let Err(err) = img.upload(alloc, api) {
                    // log.warn("error uploading image to GPU err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error uploading image to GPU err={err:?}");
                    success = false;
                }
            }
        }

        success
    }

    /// Draw the given named set of placements.
    ///
    /// Any placements that have non-uploaded images are ignored. Any
    /// graphics API errors during drawing are also ignored.
    //
    // PORT NOTE: the reference takes `self: *State` but performs no mutation, so
    // this is `&self` (the Chunk-3 `generic.zig` caller reads `&self.images`).
    pub fn draw(
        &self,
        api: &Metal,
        pipeline: &Pipeline,
        pass: &RenderPass,
        placement_type: DrawPlacements,
    ) {
        let placements: &[Placement] = match placement_type {
            DrawPlacements::KittyBelowBg => &self.kitty_placements[0..self.kitty_bg_end as usize],
            DrawPlacements::KittyBelowText => {
                &self.kitty_placements[self.kitty_bg_end as usize..self.kitty_text_end as usize]
            }
            DrawPlacements::KittyAboveText => {
                &self.kitty_placements[self.kitty_text_end as usize..]
            }
            DrawPlacements::Overlay => &self.overlay_placements[..],
        };

        for p in placements {
            // Look up the image
            let image = match self.images.get(&p.image_id) {
                Some(v) => v,
                None => {
                    // log.warn("image not found for placement image_id={}", .{p.image_id});
                    log::warn!(
                        target: LOG_SCOPE,
                        "image not found for placement image_id={:?}",
                        p.image_id
                    );
                    continue;
                }
            };

            // Get the texture
            let texture = match &image.image {
                Image::Ready(t) | Image::UnloadReady(t) => t,
                _ => {
                    // log.warn("image not ready for placement image_id={}", .{p.image_id});
                    log::warn!(
                        target: LOG_SCOPE,
                        "image not ready for placement image_id={:?}",
                        p.image_id
                    );
                    continue;
                }
            };

            // Create our vertex buffer, which is always exactly one item.
            // future(mitchellh): we can group rendering multiple instances of a single image
            let buf = match Buffer::<shaders::Image>::init_fill(
                api.image_buffer_options(),
                &[shaders::Image {
                    grid_pos: [p.x as f32, p.y as f32],
                    cell_offset: [p.cell_offset_x as f32, p.cell_offset_y as f32],
                    source_rect: [
                        p.source_x as f32,
                        p.source_y as f32,
                        p.source_width as f32,
                        p.source_height as f32,
                    ],
                    dest_size: [p.width as f32, p.height as f32],
                }],
            ) {
                Ok(b) => b,
                Err(err) => {
                    // log.warn("error creating image vertex buffer err={}", .{err});
                    log::warn!(
                        target: LOG_SCOPE,
                        "error creating image vertex buffer err={err:?}"
                    );
                    continue;
                }
            };
            // defer buf.deinit();  → `buf` drops at the end of this iteration.

            pass.step(Step {
                pipeline,
                uniforms: None,
                buffers: &[Some(buf.raw())],
                textures: &[Some(texture)],
                samplers: &[],
                draw: Draw {
                    kind: MTLPrimitiveType::TriangleStrip,
                    vertex_count: 4,
                    // Zig default `instance_count = 1` (the reference omits it).
                    instance_count: 1,
                },
            });
        }
    }

    /// Update our overlay state. Null value deletes any existing overlay.
    //
    // PORT NOTE (ownership, R1): the reference takes `overlay_: ?Overlay` **by
    // value**, but `z2d.Surface` is a slice-header value — the by-value copy
    // *shares* the caller's pixel buffer and `overlayUpdate` never deinits it
    // (the renderer keeps `self.overlay` alive and frees it later). A Rust
    // by-value `Option<Overlay>` would MOVE and drop the surface here, freeing
    // the caller's buffer. To preserve the "shared, not owned" semantics we take
    // `Option<&Overlay>` (`prep_image` dupes the aliased buffer before the borrow
    // ends). The Chunk-3 caller passes `self.overlay.as_ref()`. This diverges
    // from `GPU_RD_P1_DESIGN.md` §5's imprecise `Option<Overlay>` in favor of R1.
    pub fn overlay_update<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        overlay_: Option<&Overlay>,
    ) -> Result<(), PrepImageError> {
        let overlay = match overlay_ {
            Some(o) => o,
            None => {
                // If we don't have an overlay, remove any existing one.
                if let Some(data) = self.images.get_mut(&Id::Overlay) {
                    data.image.mark_for_unload();
                }
                return Ok(());
            }
        };

        // Overlays are always considered new content, so we take a fresh
        // generation stamp to force replacing any existing one.
        let generation = rustty_vt::kitty::graphics::next_generation();

        // Ensure we have space for our overlay placement. Do this before
        // we upload our image so we don't have to deal with cleaning
        // that up.
        self.overlay_placements.clear(); // clearRetainingCapacity
        self.overlay_placements.reserve(1); // ensureUnusedCapacity(alloc, 1)

        // Setup our image.
        let pending = overlay.pending_image();
        self.prep_image(alloc, Id::Overlay, generation, pending)?;
        // errdefer comptime unreachable;  → the tail below is infallible.

        // Setup our placement
        self.overlay_placements.push(Placement {
            image_id: Id::Overlay,
            x: 0,
            y: 0,
            z: 0,
            width: pending.width,
            height: pending.height,
            cell_offset_x: 0,
            cell_offset_y: 0,
            source_x: 0,
            source_y: 0,
            source_width: pending.width,
            source_height: pending.height,
        });

        Ok(())
    }

    /// Returns true if the Kitty graphics state requires an update based
    /// on the terminal state and our internal state.
    ///
    /// This does not read/write state used by drawing.
    pub fn kitty_requires_update(&self, t: &Terminal) -> bool {
        // If the terminal kitty image state is dirty, we must update.
        //
        // SAFETY (§7 risk #2, intrusive-ptr memory model): `t.screens.active` is
        // a `*mut Screen` (the faithful raw-ptr field). We read its
        // `.kitty_images.dirty` (both `pub`), exactly as Zig does. The pointed-at
        // `Screen` is a distinct object from `self`, so this const read aliases
        // nothing we mutate.
        if unsafe { (*t.screens.active).kitty_images.dirty } {
            return true;
        }

        // If we have any virtual references, we must also rebuild our
        // kitty state on every frame because any cell change can move
        // an image. If the virtual placements were removed, this will
        // be set to false on the next update.
        if self.kitty_virtual {
            return true;
        }

        false
    }

    /// Update the Kitty graphics state from the terminal.
    ///
    /// This reads/writes state used by drawing.
    //
    // PORT NOTE (R-d, RENDERER_KITTY_IMAGE_PATH_DESIGN §1.1): the Kitty terminal-walk
    // (viewport pins, per-placement rects, z-sort, bg/text split). Exercised by the
    // offscreen APC-driven test. Verbatim Zig reference (also mapped inline below):
    //
    // pub fn kittyUpdate(
    //     self: *State,
    //     alloc: Allocator,
    //     t: *const terminal.Terminal,
    //     cell_size: CellSize,
    // ) void {
    //     const storage = &t.screens.active.kitty_images;
    //     defer storage.dirty = false;
    //     // We always clear our previous placements no matter what because
    //     // we rebuild them from scratch.
    //     self.kitty_placements.clearRetainingCapacity();
    //     self.kitty_virtual = false;
    //     // Go through our known images and if there are any that are no longer
    //     // in use then mark them to be freed.
    //     //
    //     // This never conflicts with the below because a placement can't
    //     // reference an image that doesn't exist.
    //     {
    //         var it = self.images.iterator();
    //         while (it.next()) |kv| {
    //             switch (kv.key_ptr.*) {
    //                 // We're only looking at Kitty images
    //                 .kitty => |id| if (storage.imageById(id) == null) {
    //                     kv.value_ptr.image.markForUnload();
    //                 },
    //                 .overlay => {},
    //             }
    //         }
    //     }
    //     // The top-left and bottom-right corners of our viewport in screen
    //     // points. This lets us determine offsets and containment of placements.
    //     const top = t.screens.active.pages.getTopLeft(.viewport);
    //     const bot = t.screens.active.pages.getBottomRight(.viewport).?;
    //     const top_y = t.screens.active.pages.pointFromPin(.screen, top).?.screen.y;
    //     const bot_y = t.screens.active.pages.pointFromPin(.screen, bot).?.screen.y;
    //     // Relative placements whose parent chain roots at a virtual
    //     // placement can only be positioned once the placeholder cells
    //     // have been scanned below, so they are collected here first.
    //     var pending_relative: std.ArrayListUnmanaged(struct { ... }) = .empty;
    //     defer pending_relative.deinit(alloc);
    //     // Go through the placements and ensure the image is
    //     // on the GPU or else is ready to be sent to the GPU.
    //     var it = storage.placements.iterator();
    //     while (it.next()) |kv| {
    //         const p = kv.value_ptr;
    //         // Special logic based on location
    //         const origin: Origin = switch (p.location) {
    //             .pin => |pin| .{ .pin = pin },
    //             .virtual => { self.kitty_virtual = true; continue; },
    //             .relative => |rel| origin: {
    //                 const chain = storage.resolveChain(rel) orelse continue;
    //                 switch (chain.root.location) {
    //                     .pin => |root_pin| break :origin .{
    //                         .pin = root_pin,
    //                         .horizontal_offset = chain.horizontal_offset,
    //                         .vertical_offset = chain.vertical_offset,
    //                     },
    //                     .virtual => {
    //                         self.kitty_virtual = true;
    //                         pending_relative.append(alloc, .{ ... }) catch |err| { ... };
    //                         continue;
    //                     },
    //                     // resolveChain roots are never relative.
    //                     .relative => unreachable,
    //                 }
    //             },
    //         };
    //         // Get the image for the placement
    //         const image = storage.imageById(kv.key_ptr.image_id) orelse {
    //             log.warn(
    //                 "missing image for placement, ignoring image_id={}",
    //                 .{kv.key_ptr.image_id},
    //             );
    //             continue;
    //         };
    //         self.prepKittyPlacement(alloc, t, top_y, bot_y, &image, p, origin) catch |err| {
    //             // For errors we log and continue. We try to place
    //             // other placements even if one fails.
    //             log.warn("error preparing kitty placement err={}", .{err});
    //         };
    //     }
    //     // If we have virtual placements then we need to scan for placeholders.
    //     if (self.kitty_virtual) {
    //         var virtual_origins: std.AutoHashMapUnmanaged(
    //             terminal.kitty.graphics.ImageStorage.PlacementKey,
    //             struct { x: u32, y: u32 },
    //         ) = .empty;
    //         defer virtual_origins.deinit(alloc);
    //         var v_it = terminal.kitty.graphics.unicode.placementIterator(top, bot);
    //         while (v_it.next()) |virtual_p| {
    //             self.prepKittyVirtualPlacement(alloc, t, &virtual_p, cell_size) catch |err| {
    //                 // For errors we log and continue. We try to place
    //                 // other placements even if one fails.
    //                 log.warn("error preparing kitty placement err={}", .{err});
    //             };
    //             // We need to track the origins of all the placeholders
    //             // when we have relative cells so that we can calculate
    //             // the proper offsets later.
    //             if (pending_relative.items.len > 0) fold: { ... }
    //         }
    //         // Position the relative placements rooted at virtual
    //         // placements now that the placeholder cells are known.
    //         for (pending_relative.items) |pr| { ... }
    //     }
    //     // Sort the placements by their Z value.
    //     std.mem.sortUnstable(
    //         Placement,
    //         self.kitty_placements.items,
    //         {},
    //         struct {
    //             fn lessThan(ctx: void, lhs: Placement, rhs: Placement) bool {
    //                 _ = ctx;
    //                 return lhs.z < rhs.z or
    //                     (lhs.z == rhs.z and lhs.image_id.zLessThan(rhs.image_id));
    //             }
    //         }.lessThan,
    //     );
    //     // Find our indices. The values are sorted by z so we can
    //     // find the first placement out of bounds to find the limits.
    //     const bg_limit = std.math.minInt(i32) / 2;
    //     var bg_end: ?u32 = null;
    //     var text_end: ?u32 = null;
    //     for (self.kitty_placements.items, 0..) |p, i| {
    //         if (bg_end == null and p.z >= bg_limit) bg_end = @intCast(i);
    //         if (text_end == null and p.z >= 0) text_end = @intCast(i);
    //     }
    //     // If we didn't see any images with a z > the bg limit,
    //     // then our bg end is the end of our placement list.
    //     self.kitty_bg_end = bg_end orelse @intCast(self.kitty_placements.items.len);
    //     // Same idea for the image_text_end.
    //     self.kitty_text_end = text_end orelse @intCast(self.kitty_placements.items.len);
    // }
    pub fn kitty_update<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        t: &Terminal,
        cell_size: CellSize,
    ) {
        // const storage = &t.screens.active.kitty_images;
        // defer storage.dirty = false;
        //
        // SAFETY (memory-model, §5): `t.screens.active` is the faithful raw `*mut
        // Screen` field. We read `kitty_images`/`pages` through the raw ptr (NOT a
        // `&mut` derived from `&Terminal`), exactly as `kitty_requires_update`
        // (image.rs) already does. `screen` is a COPY of the raw pointer (no borrow
        // retained). Every shared borrow below is SCOPED so it is dropped before the
        // final `dirty = false` write through the same raw ptr (Stacked-Borrows
        // clean: no shared ref live across the write).
        let screen: *mut Screen = t.screens.active;

        // We always clear our previous placements no matter what because
        // we rebuild them from scratch.
        self.kitty_placements.clear(); // clearRetainingCapacity
        self.kitty_virtual = false;

        // Go through our known images and if there are any that are no longer
        // in use then mark them to be freed.
        //
        // This never conflicts with the below because a placement can't
        // reference an image that doesn't exist.
        {
            // var it = self.images.iterator();
            // while (it.next()) |kv| switch (kv.key_ptr.*) {
            //     .kitty => |id| if (storage.imageById(id)) |image| {
            //         if (image.data.isPending()) markForUnload;
            //     } else markForUnload,
            //     .overlay => {},
            // };
            //
            // PORT NOTE: Zig mutates entries during iteration; Rust forbids a
            // `get_mut` while `keys()` borrows `self.images`, so we collect the
            // stale Kitty ids first (observably identical — no removal, order-free).
            let storage: &ImageStorage = unsafe { &(*screen).kitty_images };
            let stale: Vec<u32> = self
                .images
                .keys()
                .filter_map(|id| match id {
                    // We're only looking at Kitty images
                    Id::Kitty(kid) => match storage.image_by_id(*kid) {
                        // A pending source must not keep an older texture for
                        // the same ID renderable while its placements wait.
                        Some(image) if image.data.is_pending() => Some(*kid),
                        Some(_) => None,
                        None => Some(*kid),
                    },
                    _ => None,
                })
                .collect();
            for kid in stale {
                self.images
                    .get_mut(&Id::Kitty(kid))
                    .unwrap()
                    .image
                    .mark_for_unload();
            }
        }

        // The top-left and bottom-right corners of our viewport in screen
        // points. This lets us determine offsets and containment of placements.
        // const top = t.screens.active.pages.getTopLeft(.viewport);
        // const bot = t.screens.active.pages.getBottomRight(.viewport).?;
        // const top_y = t.screens.active.pages.pointFromPin(.screen, top).?.screen.y;
        // const bot_y = t.screens.active.pages.pointFromPin(.screen, bot).?.screen.y;
        let (top, bot, top_y, bot_y) = {
            let pages = unsafe { &(*screen).pages };
            let top = pages.get_top_left(point::Tag::Viewport);
            let bot = pages.get_bottom_right(point::Tag::Viewport).unwrap();
            let top_y = pages
                .point_from_pin(point::Tag::Screen, top)
                .unwrap()
                .coord()
                .y;
            let bot_y = pages
                .point_from_pin(point::Tag::Screen, bot)
                .unwrap()
                .coord()
                .y;
            (top, bot, top_y, bot_y)
        };

        // Relative placements whose parent chain roots at a virtual
        // placement can only be positioned once the placeholder cells
        // have been scanned below, so they are collected here first.
        //
        // var pending_relative: std.ArrayListUnmanaged(struct {
        //     image_id: u32,
        //     p: terminal.kitty.graphics.ImageStorage.Placement,
        //     root_key: terminal.kitty.graphics.ImageStorage.PlacementKey,
        //     horizontal_offset: i32,
        //     vertical_offset: i32,
        // }) = .empty;
        // defer pending_relative.deinit(alloc);
        let mut pending_relative: Vec<PendingRelative> = Vec::new();

        // Go through the placements and ensure the image is
        // on the GPU or else is ready to be sent to the GPU.
        {
            let storage: &ImageStorage = unsafe { &(*screen).kitty_images };
            // var it = storage.placements.iterator();
            // while (it.next()) |kv| { const p = kv.value_ptr; ... }
            for (key, p) in storage.placements.iter() {
                // Special logic based on location
                // const origin: Origin = switch (p.location) {
                let origin: Origin = match p.location {
                    // .pin => |pin| .{ .pin = pin },
                    Location::Pin(pin) => Origin {
                        pin: pin as *const PageListPin,
                        horizontal_offset: 0,
                        vertical_offset: 0,
                    },
                    Location::Virtual => {
                        // We need to mark virtual placements on our renderer so that
                        // we know to rebuild in more scenarios since cell changes can
                        // now trigger placement changes.
                        self.kitty_virtual = true;
                        // We also continue out because virtual placements are
                        // only triggered by the unicode placeholder, not by the
                        // placement itself.
                        continue;
                    }
                    // .relative => |rel| origin: { ... },
                    Location::Relative(rel) => {
                        // An unresolvable chain is never drawn. This only
                        // happens transiently (storage reaps orphans) or for
                        // chains re-parented too deep, which kitty doesn't
                        // draw either.
                        // const chain = storage.resolveChain(rel) orelse continue;
                        let chain = match storage.resolve_chain(rel) {
                            Some(chain) => chain,
                            None => continue,
                        };
                        match chain.root.location {
                            // Rooted at a pin: anchored at the root's pin,
                            // offset by the accumulated chain offsets.
                            // .pin => |root_pin| break :origin .{ ... },
                            Location::Pin(root_pin) => Origin {
                                pin: root_pin as *const PageListPin,
                                horizontal_offset: chain.horizontal_offset,
                                vertical_offset: chain.vertical_offset,
                            },

                            // Rooted at a virtual placement: positioned from
                            // the root's placeholder cells, which we only
                            // know after the placeholder scan below. The
                            // placeholders also move with cell changes so we
                            // must rebuild every frame, like virtuals.
                            Location::Virtual => {
                                self.kitty_virtual = true;
                                // pending_relative.append(alloc, .{ ... }) catch |err| {
                                //     log.warn("error deferring relative placement err={}", .{err});
                                // };
                                // PORT NOTE (task-14c-2, WAIVED — not restorable):
                                // `Vec::push` is infallible (OOM aborts through
                                // `Global`), so there is NO failure arm for this
                                // `log.warn` to live in. Hoisting it out of the arm
                                // would fire it on every SUCCESSFUL deferral — the
                                // production-bug shape this plan has already shipped
                                // once. Same adjudication as `generic.zig:1558`'s
                                // `error rebuilding GPU cells` (14c-1 §4).
                                // See ADJUDICATION.md#s117-f3.
                                pending_relative.push(PendingRelative {
                                    image_id: key.image_id,
                                    p: *p,
                                    root_key: chain.root_key,
                                    horizontal_offset: chain.horizontal_offset,
                                    vertical_offset: chain.vertical_offset,
                                });
                                continue;
                            }

                            // resolveChain roots are never relative.
                            // .relative => unreachable,
                            Location::Relative(_) => unreachable!(),
                        }
                    }
                };

                // Get the image for the placement
                // const image = storage.imageById(kv.key_ptr.image_id) orelse { ...; continue; };
                let image = match storage.image_by_id(key.image_id) {
                    Some(img) => img,
                    None => {
                        // log.warn(
                        //     "missing image for placement, ignoring image_id={}",
                        //     .{kv.key_ptr.image_id},
                        // );
                        log::warn!(
                            target: LOG_SCOPE,
                            "missing image for placement, ignoring image_id={}",
                            key.image_id
                        );
                        continue;
                    }
                };

                // self.prepKittyPlacement(alloc, t, top_y, bot_y, &image, p, origin) catch |err| {
                //     // For errors we log and continue. We try to place
                //     // other placements even if one fails.
                //     log.warn("error preparing kitty placement err={}", .{err});
                // };
                if let Err(err) =
                    self.prep_kitty_placement(alloc, t, top_y, bot_y, image, p, origin)
                {
                    // log.warn("error preparing kitty placement err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error preparing kitty placement err={err:?}");
                }
            }
        }

        // If we have virtual placements then we need to scan for placeholders.
        if self.kitty_virtual {
            // The minimum placeholder cell seen per virtual placement,
            // in viewport coordinates. This is the origin for relative
            // placements rooted at a virtual placement: kitty positions
            // those at the min-x/min-y of the parent's placeholder
            // cells. Only tracked when such placements exist.
            //
            // var virtual_origins: std.AutoHashMapUnmanaged(
            //     terminal.kitty.graphics.ImageStorage.PlacementKey,
            //     struct { x: u32, y: u32 },
            // ) = .empty;
            // defer virtual_origins.deinit(alloc);
            let mut virtual_origins: HashMap<PlacementKey, VirtualOrigin> = HashMap::new();

            // SAFETY: shared re-borrow of the storage through the raw screen
            // ptr for the fold/resolution reads below; no `&mut` to the screen
            // interleaves until the final `dirty = false` write.
            let storage: &ImageStorage = unsafe { &(*screen).kitty_images };

            // var v_it = terminal.kitty.graphics.unicode.placementIterator(top, bot);
            let mut v_it = graphics_unicode::placement_iterator(top, Some(bot));
            // while (v_it.next()) |virtual_p| { ... }
            while let Some(virtual_p) = v_it.next() {
                if let Err(err) = self.prep_kitty_virtual_placement(alloc, t, &virtual_p, cell_size)
                {
                    // For errors we log and continue. We try to place
                    // other placements even if one fails.
                    // log.warn("error preparing kitty placement err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error preparing kitty placement err={err:?}");
                }

                // We need to track the origins of all the placeholders
                // when we have relative cells so that we can calculate
                // the proper offsets later.
                // if (pending_relative.items.len > 0) fold: { ... }
                if !pending_relative.is_empty() {
                    'fold: {
                        // Find the target for this virtual placeholder.
                        // const target = storage.placeholderTarget(
                        //     virtual_p.image_id,
                        //     virtual_p.placement_id,
                        // ) orelse break :fold;
                        let target = match storage
                            .placeholder_target(virtual_p.image_id, virtual_p.placement_id)
                        {
                            Some(target) => target,
                            None => break 'fold,
                        };

                        // Get the actual viewport position for it.
                        // const vp = t.screens.active.pages.pointFromPin(
                        //     .viewport,
                        //     virtual_p.pin,
                        // ) orelse break :fold;
                        let vp = match unsafe { &(*screen).pages }
                            .point_from_pin(point::Tag::Viewport, virtual_p.pin)
                        {
                            Some(vp) => vp,
                            None => break 'fold,
                        };

                        // Add the origin for this target
                        // const gop = virtual_origins.getOrPut(alloc, target.key) catch |err| {
                        //     log.warn("error tracking virtual origin err={}", .{err});
                        //     break :fold;
                        // };
                        // PORT NOTE (task-14c-2, WAIVED — not restorable): the reference's
                        // `getOrPut` takes an allocator and can fail; `HashMap::entry` cannot
                        // (OOM aborts through `Global`), so there is no failure arm for the
                        // `log.warn` and no `break :fold` to reach. See
                        // ADJUDICATION.md#s117-f3.
                        // if (!gop.found_existing) {
                        //     gop.value_ptr.* = .{ .x = vp.viewport.x, .y = vp.viewport.y };
                        // } else {
                        //     gop.value_ptr.x = @min(gop.value_ptr.x, vp.viewport.x);
                        //     gop.value_ptr.y = @min(gop.value_ptr.y, vp.viewport.y);
                        // }
                        // (`vp.viewport.x` is the u16 cell coordinate; the Zig
                        // struct field is u32 — implicit widening, `as u32` here.)
                        match virtual_origins.entry(target.key) {
                            std::collections::hash_map::Entry::Vacant(e) => {
                                e.insert(VirtualOrigin {
                                    x: vp.coord().x as u32,
                                    y: vp.coord().y,
                                });
                            }
                            std::collections::hash_map::Entry::Occupied(mut e) => {
                                let v = e.get_mut();
                                v.x = v.x.min(vp.coord().x as u32);
                                v.y = v.y.min(vp.coord().y);
                            }
                        }
                    }
                }
            }

            // Position the relative placements rooted at virtual
            // placements now that the placeholder cells are known. A
            // root with no placeholders on screen leaves its relative
            // placements undrawn, matching kitty.
            // for (pending_relative.items) |pr| { ... }
            for pr in pending_relative.iter() {
                // const origin = virtual_origins.get(pr.root_key) orelse continue;
                let origin = match virtual_origins.get(&pr.root_key) {
                    Some(origin) => *origin,
                    None => continue,
                };
                // const image = storage.imageById(pr.image_id) orelse continue;
                let image = match storage.image_by_id(pr.image_id) {
                    Some(image) => image,
                    None => continue,
                };
                // if (image.data.isPending()) continue;
                if image.data.is_pending() {
                    continue;
                }

                // const grid = pr.p.gridSize(image, t);
                // if (grid.cols == 0 or grid.rows == 0) continue;
                let grid = pr.p.grid_size(image, t);
                if grid.cols == 0 || grid.rows == 0 {
                    continue;
                }

                // Viewport-relative signed position; cull placements
                // entirely outside the viewport.
                // const x: i64 = @as(i64, origin.x) + pr.horizontal_offset;
                // const y: i64 = @as(i64, origin.y) + pr.vertical_offset;
                // if (y >= t.rows or y + grid.rows - 1 < 0) continue;
                // if (x >= t.cols or x + grid.cols - 1 < 0) continue;
                let x: i64 = origin.x as i64 + pr.horizontal_offset as i64;
                let y: i64 = origin.y as i64 + pr.vertical_offset as i64;
                if y >= t.rows as i64 || y + grid.rows as i64 - 1 < 0 {
                    continue;
                }
                if x >= t.cols as i64 || x + grid.cols as i64 - 1 < 0 {
                    continue;
                }

                // self.appendKittyPlacement(
                //     alloc, t, &image, &pr.p,
                //     std.math.cast(i32, x) orelse continue,
                //     std.math.cast(i32, y) orelse continue,
                // ) catch |err| {
                //     log.warn("error preparing kitty placement err={}", .{err});
                // };
                let x = match i32::try_from(x) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let y = match i32::try_from(y) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Err(err) = self.append_kitty_placement(alloc, t, image, &pr.p, x, y) {
                    // log.warn("error preparing kitty placement err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error preparing kitty placement err={err:?}");
                }
            }
        }

        // Sort the placements by their Z value.
        // std.mem.sortUnstable(Placement, ..., lessThan);
        //
        // NOTE: mirrors Zig `sortUnstable` with a strict boolean `lessThan`; the
        // comparator returns `Less`/`Greater` only (never `Equal`), which is
        // acceptable for an UNSTABLE sort and matches Zig's boolean predicate
        // (equal elements may reorder either way — Zig's is unstable too).
        self.kitty_placements.sort_unstable_by(|lhs, rhs| {
            if lhs.z < rhs.z || (lhs.z == rhs.z && Id::z_less_than(lhs.image_id, rhs.image_id)) {
                core::cmp::Ordering::Less
            } else {
                core::cmp::Ordering::Greater
            }
        });

        // Find our indices. The values are sorted by z so we can
        // find the first placement out of bounds to find the limits.
        let bg_limit: i32 = i32::MIN / 2;
        let mut bg_end: Option<u32> = None;
        let mut text_end: Option<u32> = None;
        for (i, p) in self.kitty_placements.iter().enumerate() {
            if bg_end.is_none() && p.z >= bg_limit {
                bg_end = Some(i as u32);
            }
            if text_end.is_none() && p.z >= 0 {
                text_end = Some(i as u32);
            }
        }
        // If we didn't see any images with a z > the bg limit,
        // then our bg end is the end of our placement list.
        self.kitty_bg_end = bg_end.unwrap_or(self.kitty_placements.len() as u32);
        // Same idea for the image_text_end.
        self.kitty_text_end = text_end.unwrap_or(self.kitty_placements.len() as u32);

        // `defer storage.dirty = false;` — LAST, through the raw ptr, no live borrow.
        unsafe {
            (*screen).kitty_images.dirty = false;
        }
    }

    /// Get the viewport-relative position for this placement and add it
    /// to the placements list.
    //
    // PORT NOTE (R-d §1.2, reworked by `9490f7134`/`52190a5d8`/`c5a3c7e2e`):
    // signed/widened `gridSize` edge math over the caller-computed `Origin`;
    // called from `kitty_update`. Verbatim Zig reference (also mapped inline
    // below):
    //
    // fn prepKittyPlacement(
    //     self: *State,
    //     alloc: Allocator,
    //     t: *const terminal.Terminal,
    //     top_y: u32,
    //     bot_y: u32,
    //     image: *const terminal.kitty.graphics.Image,
    //     p: *const terminal.kitty.graphics.ImageStorage.Placement,
    //     origin: Origin,
    // ) PrepImageError!void {
    //     // Keep the native placement but do not create a renderer placement or
    //     // texture until the decoded bytes arrive.
    //     if (image.data.isPending()) return;
    //     // An origin whose tracked content was pruned has no position.
    //     if (origin.pin.garbage) return;
    //     // The size of the placement in grid cells. A zero size can
    //     // occur when pixel geometry is unavailable; nothing to place.
    //     const grid = p.gridSize(image.*, t);
    //     if (grid.cols == 0 or grid.rows == 0) return;
    //     // This is expensive but necessary.
    //     const origin_y = t.screens.active.pages.pointFromPin(
    //         .screen,
    //         origin.pin.*,
    //     ).?.screen.y;
    //     // The placement's edges in screen coordinates. Chain offsets
    //     // are signed: a relative placement can hang above or to the
    //     // left of its origin, so this math must be signed and widened.
    //     const img_top_y: i64 = @as(i64, origin_y) + origin.vertical_offset;
    //     const img_bot_y: i64 = img_top_y + grid.rows - 1;
    //     const img_left_x: i64 = @as(i64, origin.pin.x) + origin.horizontal_offset;
    //     const img_right_x: i64 = img_left_x + grid.cols - 1;
    //     // If the placement isn't within our viewport then skip it.
    //     if (img_top_y > bot_y or img_bot_y < top_y) return;
    //     if (img_left_x >= t.cols or img_right_x < 0) return;
    //     // Viewport-relative position. Offsets so extreme that the
    //     // position is unrepresentable have no renderable pixels.
    //     const y_pos = std.math.cast(i32, img_top_y - top_y) orelse return;
    //     const x_pos = std.math.cast(i32, img_left_x) orelse return;
    //     try self.appendKittyPlacement(alloc, t, image, p, x_pos, y_pos);
    // }
    #[allow(clippy::too_many_arguments)]
    // PORT NOTE: the explicit trailing `return` is the reference's; the port
    // mirrors it so the two read line-for-line.
    #[allow(clippy::needless_return)]
    fn prep_kitty_placement<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        t: &Terminal,
        top_y: u32,
        bot_y: u32,
        image: &KittyImage,
        p: &KittyStoragePlacement,
        origin: Origin,
    ) -> Result<(), PrepImageError> {
        // Keep the native placement but do not create a renderer placement or
        // texture until the decoded bytes arrive.
        // if (image.data.isPending()) return;
        if image.data.is_pending() {
            return Ok(());
        }

        // An origin whose tracked content was pruned has no position.
        // if (origin.pin.garbage) return;
        //
        // SAFETY: `origin.pin` aliases a live tracked pin (see `Origin`); only
        // shared reads of the screen are active.
        if unsafe { (*origin.pin).garbage } {
            return Ok(());
        }

        // The size of the placement in grid cells. A zero size can
        // occur when pixel geometry is unavailable; nothing to place.
        // const grid = p.gridSize(image.*, t);
        let grid = p.grid_size(image, t);
        if grid.cols == 0 || grid.rows == 0 {
            return Ok(());
        }

        // This is expensive but necessary.
        // const origin_y = t.screens.active.pages.pointFromPin(
        //     .screen,
        //     origin.pin.*,
        // ).?.screen.y;
        let pages = unsafe { &(*t.screens.active).pages };
        let origin_y = pages
            .point_from_pin(point::Tag::Screen, unsafe { *origin.pin })
            .unwrap()
            .coord()
            .y;

        // The placement's edges in screen coordinates. Chain offsets
        // are signed: a relative placement can hang above or to the
        // left of its origin, so this math must be signed and widened.
        let img_top_y: i64 = origin_y as i64 + origin.vertical_offset as i64;
        let img_bot_y: i64 = img_top_y + grid.rows as i64 - 1;
        let img_left_x: i64 = unsafe { (*origin.pin).x } as i64 + origin.horizontal_offset as i64;
        let img_right_x: i64 = img_left_x + grid.cols as i64 - 1;

        // If the placement isn't within our viewport then skip it.
        if img_top_y > bot_y as i64 || img_bot_y < top_y as i64 {
            return Ok(());
        }
        if img_left_x >= t.cols as i64 || img_right_x < 0 {
            return Ok(());
        }

        // Viewport-relative position. Offsets so extreme that the
        // position is unrepresentable have no renderable pixels.
        // const y_pos = std.math.cast(i32, img_top_y - top_y) orelse return;
        // const x_pos = std.math.cast(i32, img_left_x) orelse return;
        let y_pos = match i32::try_from(img_top_y - top_y as i64) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        let x_pos = match i32::try_from(img_left_x) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };

        // try self.appendKittyPlacement(alloc, t, image, p, x_pos, y_pos);
        return self.append_kitty_placement(alloc, t, image, p, x_pos, y_pos);
    }

    /// Compute the sizes for a native kitty placement positioned at the
    /// given viewport cell position, prepare its image for the GPU, and
    /// append it to the placements list.
    //
    // fn appendKittyPlacement(
    //     self: *State,
    //     alloc: Allocator,
    //     t: *const terminal.Terminal,
    //     image: *const terminal.kitty.graphics.Image,
    //     p: *const terminal.kitty.graphics.ImageStorage.Placement,
    //     x: i32,
    //     y: i32,
    // ) PrepImageError!void {
    #[allow(clippy::too_many_arguments)]
    fn append_kitty_placement<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        t: &Terminal,
        image: &KittyImage,
        p: &KittyStoragePlacement,
        x: i32,
        y: i32,
    ) -> Result<(), PrepImageError> {
        // We need to prep this image for upload if it isn't in the
        // cache OR it is in the cache but the transmit time doesn't
        // match meaning this image is different.
        // try self.prepKittyImage(alloc, image);
        self.prep_kitty_image(alloc, image)?;

        // Calculate the dimensions of our image, taking in to
        // account the rows / columns specified by the placement.
        // const dest_size = p.pixelSize(image.*, t);
        // const cell_offset = p.cellOffset(t);
        let dest_size = p.pixel_size(image, t);
        let cell_offset = p.cell_offset(t);

        // const source = p.sourceRect(image.*);
        let source = p.source_rect(image);

        // Accumulate the placement
        if dest_size.width > 0 && dest_size.height > 0 {
            // try self.kitty_placements.append(alloc, .{ ... });
            self.kitty_placements.push(Placement {
                image_id: Id::Kitty(image.id),
                x,
                y,
                z: p.z,
                width: dest_size.width,
                height: dest_size.height,
                cell_offset_x: cell_offset.x,
                cell_offset_y: cell_offset.y,
                source_x: source.x,
                source_y: source.y,
                source_width: source.width,
                source_height: source.height,
            });
        }
        Ok(())
    }

    // PORT NOTE (R-d §1.3): `unicode.placementIterator`/`renderPlacement` virtual walk;
    // called from `kitty_update`. Verbatim Zig reference (also mapped inline below):
    //
    // fn prepKittyVirtualPlacement(
    //     self: *State,
    //     alloc: Allocator,
    //     t: *const terminal.Terminal,
    //     p: *const terminal.kitty.graphics.unicode.Placement,
    //     cell_size: CellSize,
    // ) PrepImageError!void {
    //     const storage = &t.screens.active.kitty_images;
    //     const image = storage.imageById(p.image_id) orelse {
    //         log.warn(
    //             "missing image for virtual placement, ignoring image_id={}",
    //             .{p.image_id},
    //         );
    //         return;
    //     };
    //     const rp = p.renderPlacement(
    //         storage,
    //         &image,
    //         cell_size.width,
    //         cell_size.height,
    //     ) catch |err| {
    //         log.warn("error rendering virtual placement err={}", .{err});
    //         return;
    //     };
    //     // If our placement is zero sized then we don't do anything.
    //     if (rp.dest_width == 0 or rp.dest_height == 0) return;
    //     const viewport: terminal.point.Point = t.screens.active.pages.pointFromPin(
    //         .viewport,
    //         rp.top_left,
    //     ) orelse {
    //         // This is unreachable with virtual placements because we should
    //         // only ever be looking at virtual placements that are in our
    //         // viewport in the renderer and virtual placements only ever take
    //         // up one row.
    //         unreachable;
    //     };
    //     // Prepare the image for the GPU and store the placement.
    //     try self.prepKittyImage(alloc, &image);
    //     try self.kitty_placements.append(alloc, .{
    //         .image_id = .{ .kitty = image.id },
    //         .x = @intCast(rp.top_left.x),
    //         .y = @intCast(viewport.viewport.y),
    //         .z = -1,
    //         .width = rp.dest_width,
    //         .height = rp.dest_height,
    //         .cell_offset_x = rp.offset_x,
    //         .cell_offset_y = rp.offset_y,
    //         .source_x = rp.source_x,
    //         .source_y = rp.source_y,
    //         .source_width = rp.source_width,
    //         .source_height = rp.source_height,
    //     });
    // }
    fn prep_kitty_virtual_placement<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        t: &Terminal,
        p: &KittyUnicodePlacement,
        cell_size: CellSize,
    ) -> Result<(), PrepImageError> {
        // const storage = &t.screens.active.kitty_images;
        let storage = unsafe { &(*t.screens.active).kitty_images };
        // const image = storage.imageById(p.image_id) orelse { ...; return; };
        let image = match storage.image_by_id(p.image_id) {
            Some(img) => img,
            None => {
                // log.warn(
                //     "missing image for virtual placement, ignoring image_id={}",
                //     .{p.image_id},
                // );
                log::warn!(
                    target: LOG_SCOPE,
                    "missing image for virtual placement, ignoring image_id={}",
                    p.image_id
                );
                return Ok(());
            }
        };
        // if (image.data.isPending()) return;
        if image.data.is_pending() {
            return Ok(());
        }

        // const rp = p.renderPlacement(storage, &image, cell_size.width, cell_size.height)
        //     catch |err| { log.warn("error rendering virtual placement err={}", .{err}); return; };
        let rp = match p.render_placement(storage, image, cell_size.width, cell_size.height) {
            Ok(rp) => rp,
            Err(err) => {
                // log.warn("error rendering virtual placement err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error rendering virtual placement err={err:?}");
                return Ok(());
            }
        };

        // If our placement is zero sized then we don't do anything.
        if rp.dest_width == 0 || rp.dest_height == 0 {
            return Ok(());
        }

        // const viewport: terminal.point.Point = t.screens.active.pages.pointFromPin(
        //     .viewport, rp.top_left,
        // ) orelse {
        //     // This is unreachable with virtual placements because we should
        //     // only ever be looking at virtual placements that are in our
        //     // viewport in the renderer and virtual placements only ever take
        //     // up one row.
        //     unreachable;
        // };
        let viewport: point::Point = unsafe { &(*t.screens.active).pages }
            .point_from_pin(point::Tag::Viewport, rp.top_left)
            .expect("virtual placement in viewport");

        // Prepare the image for the GPU and store the placement.
        // try self.prepKittyImage(alloc, &image);
        self.prep_kitty_image(alloc, image)?;
        // try self.kitty_placements.append(alloc, .{ ... });
        self.kitty_placements.push(Placement {
            image_id: Id::Kitty(image.id),
            x: rp.top_left.x as i32,
            y: viewport.coord().y as i32, // viewport.viewport.y
            z: -1,
            width: rp.dest_width,
            height: rp.dest_height,
            cell_offset_x: rp.offset_x,
            cell_offset_y: rp.offset_y,
            source_x: rp.source_x,
            source_y: rp.source_y,
            source_width: rp.source_width,
            source_height: rp.source_height,
        });
        Ok(())
    }

    /// Prepare an image for upload to the GPU.
    fn prep_image<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        id: Id,
        generation: u64,
        pending: Pending,
    ) -> Result<(), PrepImageError> {
        // If this image exists and its generation is the same it is the
        // identical image so we don't need to send it to the GPU.
        //
        // PORT NOTE: Zig's `getOrPut` pre-inserts an (undefined) slot before the
        // dupe; we defer the insert instead. On dupe failure with `!found_existing`
        // Zig removes that slot — here there is no slot to remove (harmless no-op),
        // so `self.images.remove(id)` is elided.
        let found_existing = match self.images.get(&id) {
            Some(entry) => {
                if entry.generation == generation {
                    return Ok(());
                }
                true
            }
            None => false,
        };

        // Store it in the map
        // const new_image: Image = if (pending.convertCopy(alloc)) |v| v else |_| { ... };
        //
        // PORT NOTE (`169213cd2`): copying-to-owned and converting the pixel
        // format are FUSED into `convert_copy` (single pass, no intermediate
        // dupe buffer).
        let new_image: Image = match pending.convert_copy(alloc) {
            Ok(v) => v,
            Err(_) => {
                if !found_existing {
                    // If this is a new entry we can just remove it since it
                    // was never sent to the GPU. (No pre-inserted slot to remove.)
                    self.images.remove(&id);
                } else {
                    // If this was an existing entry, it is invalid and
                    // we must unload it.
                    self.images.get_mut(&id).unwrap().image.mark_for_unload();
                }

                return Err(PrepImageError::OutOfMemory);
            }
        };
        // Note: we don't need to errdefer free the data because it is
        // put into the map immediately below and our errdefer to
        // handle our map state will fix this up.
        if !found_existing {
            self.images.insert(
                id,
                ImageEntry {
                    image: new_image,
                    // Zig `.generation = 0`; overwritten below before any read.
                    generation: 0,
                },
            );
        } else {
            self.images
                .get_mut(&id)
                .unwrap()
                .image
                .mark_for_replace(alloc, new_image);
        }

        // If any error happens, we unload the image and it is invalid.
        // errdefer gop.value_ptr.image.markForUnload();
        //
        // gop.value_ptr.image.getPendingPointer().?.prepForUpload(alloc) catch |err| { ... };
        // (`b17abd96d`: prepForUpload moved onto `Image.Pending`.)
        let entry = self.images.get_mut(&id).unwrap();
        if let Err(err) = entry
            .image
            .get_pending_pointer()
            .unwrap()
            .prep_for_upload(alloc)
        {
            // log.warn("error preparing image for upload err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error preparing image for upload err={err:?}");
            entry.image.mark_for_unload();
            return Err(PrepImageError::ImageConversionError);
        }
        entry.generation = generation;
        Ok(())
    }

    /// Prepare the provided Kitty image for upload to the GPU by copying its
    /// data with our allocator and setting it to the pending state.
    //
    // PORT NOTE (R-d §1.4): called from the Kitty walk; the `.png => unreachable`
    // format switch feeds `prep_image`. Verbatim Zig reference (mapped inline below):
    //
    // fn prepKittyImage(
    //     self: *State,
    //     alloc: Allocator,
    //     image: *const terminal.kitty.graphics.Image,
    // ) PrepImageError!void {
    //     try self.prepImage(
    //         alloc,
    //         .{ .kitty = image.id },
    //         image.generation,
    //         .{
    //             .width = image.width,
    //             .height = image.height,
    //             .pixel_format = switch (image.format) {
    //                 .gray => .gray,
    //                 .gray_alpha => .gray_alpha,
    //                 .rgb => .rgb,
    //                 .rgba => .rgba,
    //                 .png => unreachable, // should be decoded by now
    //             },
    //             // constCasts are always gross but this one is safe is because
    //             // the data is only read from here and copied into its own
    //             // buffer.
    //             .data = @constCast(image.data.ptr),
    //         },
    //     );
    // }
    fn prep_kitty_image<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        image: &KittyImage,
    ) -> Result<(), PrepImageError> {
        // For animated images this is the current animation frame;
        // the image generation changes whenever the current frame
        // does, so the upload cache stays coherent.
        // const data = image.renderData().bytes() orelse unreachable;
        // (callers skip pending images, so the payload is always complete here)
        let data = image.render_data().bytes().unwrap();
        // try self.prepImage(alloc, .{ .kitty = image.id }, image.generation, .{ ... });
        self.prep_image(
            alloc,
            Id::Kitty(image.id),
            image.generation,
            Pending {
                width: image.width,
                height: image.height,
                pixel_format: match image.format {
                    Format::Gray => PixelFormat::Gray,
                    Format::GrayAlpha => PixelFormat::GrayAlpha,
                    Format::Rgb => PixelFormat::Rgb,
                    Format::Rgba => PixelFormat::Rgba,
                    Format::Png => unreachable!(), // should be decoded by now
                },
                // constCasts are always gross but this one is safe is because
                // the data is only read from here and copied into its own
                // buffer. (`image` borrows the stored image under the terminal
                // lock (N276), so the ptr is valid for the call.)
                // .data = @constCast(data.ptr),
                data: data.as_ptr() as *mut u8,
            },
        )
    }
}

/// Represents a single image placement on the grid.
/// A placement is a request to render an instance of an image.
#[derive(Clone, Copy)]
pub struct Placement {
    /// The image being rendered. This MUST be in the image map.
    pub image_id: Id,

    /// The grid x/y where this placement is located.
    pub x: i32,
    pub y: i32,
    pub z: i32,

    /// The width/height of the placed image.
    pub width: u32,
    pub height: u32,

    /// The offset in pixels from the top left of the cell.
    /// This is clamped to the size of a cell.
    pub cell_offset_x: u32,
    pub cell_offset_y: u32,

    /// The source rectangle of the placement.
    pub source_x: u32,
    pub source_y: u32,
    pub source_width: u32,
    pub source_height: u32,
}

// pub const DrawPlacements = enum { ... };
/// The named set of placements `draw` renders.
pub enum DrawPlacements {
    KittyBelowBg,
    KittyBelowText,
    KittyAboveText,
    Overlay,
}

// const PrepImageError = error{ OutOfMemory, ImageConversionError };
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PrepImageError {
    OutOfMemory,
    ImageConversionError,
}

/// Image identifier used to store and lookup images.
///
/// This is tagged by different image types to make it easier to
/// store different kinds of images in the same map without having
/// to worry about ID collisions.
//
// PORT NOTE: `union(enum) { kitty: u32, overlay }` → a plain enum (the `overlay`
// arm has no payload). `#[derive(..., Hash)]` makes it a `HashMap` key.
//
// PORT NOTE (task-14c-2): `Debug` is derived because `image.zig:122`/`:132`'s
// restored `log.warn("... image_id={}", .{p.image_id})` render this value. Zig's
// `{}` on a tagged union prints `<TypeName>{ .kitty = <n> }`; the derived `Debug`
// prints `Kitty(<n>)`. That rendering difference is the known/accepted one (the
// reference has no custom `format` method to port).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Id {
    /// Image sent to the terminal state via the kitty graphics protocol.
    /// The value is the ID assigned by the terminal.
    Kitty(u32),

    /// Debug overlay. This is always composited down to a single
    /// image for now. In the future we can support layers here if we want.
    Overlay,
}

impl Id {
    /// Z-ordering tie-breaker for images with the same z value.
    pub fn z_less_than(lhs: Id, rhs: Id) -> bool {
        // If our tags aren't the same, we sort by tag.
        if std::mem::discriminant(&lhs) != std::mem::discriminant(&rhs) {
            return match lhs {
                // Kitty images always sort before (lower z) non-kitty images.
                Id::Kitty(_) => true,
                Id::Overlay => false,
            };
        }

        match lhs {
            Id::Kitty(lhs_id) => {
                // rhs.kitty — the tags are equal, so `rhs` is also `Kitty`.
                let rhs_id = match rhs {
                    Id::Kitty(v) => v,
                    Id::Overlay => unreachable!(),
                };
                lhs_id < rhs_id
            }

            // No sensical ordering
            Id::Overlay => false,
        }
    }
}

/// The map used for storing images.
// pub const ImageMap = std.AutoHashMapUnmanaged(Id, struct { image, generation });
pub type ImageMap = HashMap<Id, ImageEntry>;

/// The `ImageMap` value: the image plus its terminal-image generation stamp.
pub struct ImageEntry {
    pub image: Image,

    /// The generation of the terminal image this was created from
    /// (see terminal.kitty.graphics.Image.generation). Used to detect
    /// staleness: a differing generation for the same ID means the
    /// contents changed and the texture must be replaced. Zero is
    /// never a valid stored generation so it marks "not yet uploaded".
    ///
    /// (In the port: `rustty_vt::kitty::graphics_image::Image::generation`.)
    pub generation: u64,
}

/// The state for a single image that is to be rendered.
//
// PORT NOTE: `union(enum)` → a Rust enum. The payload-carrying arms own their
// data (`Pending`'s raw byte buffer, `Texture`'s `Retained`). `Image` has no
// `Drop` impl — the raw `Pending::data` is freed only by the explicit
// `deinit`/`mark_for_*`/`upload` paths, faithful to Zig's manual union memory.
pub enum Image {
    /// The image data is pending upload to the GPU.
    ///
    /// This data is owned by this union so it must be freed once uploaded.
    Pending(Pending),

    /// This is the same as the pending states but there is
    /// a texture already allocated that we want to replace.
    Replace(Replace),

    /// The image is uploaded and ready to be used.
    Ready(Texture),

    /// The image isn't uploaded yet but is scheduled to be unloaded.
    UnloadPending(Pending),
    /// The image is uploaded and is scheduled to be unloaded.
    UnloadReady(Texture),
    /// The image is uploaded and scheduled to be replaced
    /// with new data, but it's also scheduled to be unloaded.
    UnloadReplace(Replace),
}

// pub const Replace = struct { texture: Texture, pending: Pending };
pub struct Replace {
    pub texture: Texture,
    pub pending: Pending,
}

/// Pending image data that needs to be uploaded to the GPU.
//
// PORT NOTE (zig-byteslice-to-rust-rawbytes + rust-mut-args memory): Zig
// `data: [*]u8` is a RAW pointer whose ownership is tracked by the enclosing
// `Image` variant (owned once `prep_image` dupes it; borrowed when it aliases a
// z2d/kitty buffer via `Overlay::pending_image`). The faithful port keeps it a
// raw `*mut u8` (NOT `Vec<u8>`) so the borrow-then-own duality and the overlay
// aliasing are preserved. `Pending` stays `Copy` (the pointer is copied, not the
// bytes) exactly like the Zig struct.
#[derive(Clone, Copy)]
pub struct Pending {
    pub height: u32,
    pub width: u32,
    pub pixel_format: PixelFormat,

    /// Data is always expected to be (width * height * bpp).
    pub data: *mut u8, // Zig `[*]u8`
}

impl Pending {
    // pub fn dataSlice(self: Pending) []u8 { return self.data[0..self.len()]; }
    //
    // PORT NOTE: Zig returns a mutable `[]u8`, but the only in-tree mutator is
    // `convert` (an R5 seam), so the FULL callers (dupe copy-source, texture
    // upload-source) need only `&[u8]`. Returning `&[u8]` also avoids clippy's
    // `mut_from_ref`. Faithful for the ported surface.
    pub fn data_slice(&self) -> &[u8] {
        // SAFETY: `data` points at `len()` valid bytes (owned after `prep_image`
        // dupes it, or aliasing a live z2d surface buffer for the overlay).
        unsafe { std::slice::from_raw_parts(self.data, self.len()) }
    }

    // pub fn len(self: Pending) usize { return self.width * self.height * bpp(); }
    #[allow(clippy::len_without_is_empty)] // faithful to Zig `Pending.len`; no `is_empty` in the reference
    pub fn len(&self) -> usize {
        self.width as usize * self.height as usize * self.pixel_format.bpp()
    }

    /// Converts the image data and replaces it with a format that can be uploaded to the GPU.
    /// If the data is already in a format that can be uploaded, this is a
    /// no-op.
    /// Use `convertCopy()` to convert and copy in a single-pass, such as for owning the data to upload.
    //
    // fn convertReplace(self: *Image.Pending, alloc: Allocator) wuffs.Error!void {
    //     // As things stand, we currently convert all images to RGBA before
    //     // uploading to the GPU. This just makes things easier. In the future
    //     // we may want to support other formats.
    //     if (self.pixel_format == .rgba) return;
    //     // If the pending data isn't RGBA we'll need to swizzle it.
    //     const data = self.dataSlice();
    //     const rgba = try switch (self.pixel_format) {
    //         .gray => wuffs.swizzle.gToRgba(alloc, data),
    //         .gray_alpha => wuffs.swizzle.gaToRgba(alloc, data),
    //         .rgb => wuffs.swizzle.rgbToRgba(alloc, data),
    //         .bgr => wuffs.swizzle.bgrToRgba(alloc, data),
    //         .rgba => unreachable,
    //         .bgra => wuffs.swizzle.bgraToRgba(alloc, data),
    //     };
    //     alloc.free(data);
    //     self.data = rgba.ptr;
    //     self.pixel_format = .rgba;
    // }
    fn convert_replace<A: Allocator>(&mut self, alloc: A) -> Result<(), WuffsError> {
        // As things stand, we currently convert all images to RGBA before
        // uploading to the GPU. This just makes things easier. In the future
        // we may want to support other formats.
        if self.pixel_format == PixelFormat::Rgba {
            return Ok(());
        }
        // If the pending data isn't RGBA we'll need to swizzle it.
        // (`.rgba` is unreachable in the swizzle: early-returned above.)
        let old_len = self.len();
        let rgba = swizzle_to_rgba(&alloc, self.data_slice(), self.pixel_format)?;
        // alloc.free(data);
        free_raw(&alloc, self.data, old_len);
        // self.data = rgba.ptr; self.pixel_format = .rgba;
        self.data = rgba;
        self.pixel_format = PixelFormat::Rgba;
        Ok(())
    }

    /// Converts the image data to a copy with a format that can be uploaded to the GPU.
    /// If the data is already owned, use `convertReplace()` to be a no-op for data already
    /// in a format that can be uploaded.
    //
    // fn convertCopy(self: *const Image.Pending, alloc: Allocator) wuffs.Error!Image {
    //     // As things stand, we currently convert all images to RGBA before
    //     // uploading to the GPU. This just makes things easier. In the future
    //     // we may want to support other formats.
    //     const data = self.dataSlice();
    //     const rgba = try switch (self.pixel_format) {
    //         .gray => wuffs.swizzle.gToRgba(alloc, data),
    //         .gray_alpha => wuffs.swizzle.gaToRgba(alloc, data),
    //         .rgb => wuffs.swizzle.rgbToRgba(alloc, data),
    //         .bgr => wuffs.swizzle.bgrToRgba(alloc, data),
    //         .rgba => alloc.dupe(u8, data),
    //         .bgra => wuffs.swizzle.bgraToRgba(alloc, data),
    //     };
    //     const result: Image = .{ .pending = .{
    //         .height = self.height,
    //         .width = self.width,
    //         .pixel_format = .rgba,
    //         .data = rgba.ptr,
    //     } };
    //     return result;
    // }
    fn convert_copy<A: Allocator>(&self, alloc: A) -> Result<Image, WuffsError> {
        // As things stand, we currently convert all images to RGBA before
        // uploading to the GPU. This just makes things easier. In the future
        // we may want to support other formats.
        // (`.rgba => alloc.dupe(u8, data)` is the `Rgba` memcpy arm of the
        // shared swizzle helper.)
        let rgba = swizzle_to_rgba(&alloc, self.data_slice(), self.pixel_format)?;
        Ok(Image::Pending(Pending {
            height: self.height,
            width: self.width,
            pixel_format: PixelFormat::Rgba,
            data: rgba,
        }))
    }

    /// Prepare the pending image data for upload to the GPU.
    /// This doesn't need GPU access so is safe to call any time.
    //
    // fn prepForUpload(self: *Image.Pending, alloc: Allocator) wuffs.Error!void {
    //     try self.convertReplace(alloc);
    // }
    // (`b17abd96d`: restricted to `Image.Pending` — takes the Pending, not the union.)
    fn prep_for_upload<A: Allocator>(&mut self, alloc: A) -> Result<(), WuffsError> {
        self.convert_replace(alloc)
    }
}

/// The shared per-format swizzle body of `convertReplace`/`convertCopy`
/// (the `wuffs.swizzle.*ToRgba` calls, re-homed as pure-Rust per-pixel loops —
/// see the module PORT NOTE). Allocates and returns the RGBA buffer.
//
// PORT NOTE (dedup): the reference repeats the 6-arm switch in BOTH
// `convertReplace` and `convertCopy`; this helper hosts it once. The `Rgba`
// arm is `alloc.dupe(u8, data)` (only reachable from `convert_copy`;
// `convert_replace` early-returns first, matching its `.rgba => unreachable`).
//
// PORT NOTE (`86db30785`): the reference's gaToRgba swizzles
// `YA_NONPREMUL -> RGBA_NONPREMUL` (fixed upstream from the always-failing
// YA_PREMUL pair) — a straight channel expand with NO alpha math, exactly the
// GrayAlpha loop below (byte-identical to `rustty-wuffs`'s `ga_to_rgba` KAT).
fn swizzle_to_rgba<A: Allocator>(
    alloc: &A,
    src: &[u8],
    format: PixelFormat,
) -> Result<*mut u8, WuffsError> {
    let src_bpp = format.bpp();
    // Pixel count.
    let px = src.len() / src_bpp;
    // Overflow guard mirrors wuffs `error{Overflow}` on `src.len * 4 / src_bpp`.
    let out_len = px.checked_mul(4).ok_or(WuffsError::Overflow)?;

    // Allocate the RGBA buffer with the SAME allocator model as `dupe_bytes`
    // (align 1) so `free_data`/`Image::deinit` can release it identically.
    let dst_ptr = alloc_bytes(alloc, out_len).map_err(|_| WuffsError::OutOfMemory)?;
    // SAFETY: `dst_ptr` points at `out_len` freshly-allocated bytes.
    let dst = unsafe { core::slice::from_raw_parts_mut(dst_ptr, out_len) };

    match format {
        PixelFormat::Gray => {
            // Index-based per-pixel loop (faithful to the wuffs swizzle's
            // src/dst byte-offset arithmetic); `enumerate` would obscure the
            // parallel `src[i]`/`dst[o..]` addressing shared by all arms.
            #[allow(clippy::needless_range_loop)]
            for i in 0..px {
                let y = src[i];
                let o = i * 4;
                dst[o] = y;
                dst[o + 1] = y;
                dst[o + 2] = y;
                dst[o + 3] = 255;
            }
        }
        PixelFormat::GrayAlpha => {
            // YA_NONPREMUL -> RGBA_NONPREMUL: straight channel expand.
            for i in 0..px {
                let y = src[i * 2];
                let a = src[i * 2 + 1];
                let o = i * 4;
                dst[o] = y;
                dst[o + 1] = y;
                dst[o + 2] = y;
                dst[o + 3] = a;
            }
        }
        PixelFormat::Rgb => {
            for i in 0..px {
                let s = i * 3;
                let o = i * 4;
                dst[o] = src[s];
                dst[o + 1] = src[s + 1];
                dst[o + 2] = src[s + 2];
                dst[o + 3] = 255;
            }
        }
        PixelFormat::Bgr => {
            for i in 0..px {
                let s = i * 3;
                let o = i * 4;
                dst[o] = src[s + 2];
                dst[o + 1] = src[s + 1];
                dst[o + 2] = src[s];
                dst[o + 3] = 255;
            }
        }
        PixelFormat::Bgra => {
            // BGRA_PREMUL source: alpha already applied → channel swap only.
            for i in 0..px {
                let s = i * 4;
                let o = i * 4;
                dst[o] = src[s + 2];
                dst[o + 1] = src[s + 1];
                dst[o + 2] = src[s];
                dst[o + 3] = src[s + 3];
            }
        }
        // `.rgba => alloc.dupe(u8, data)` (convert_copy's arm).
        PixelFormat::Rgba => dst.copy_from_slice(src),
    }

    Ok(dst_ptr)
}

/// The pixel format of a `Pending` image's raw bytes.
// pub const PixelFormat = enum { gray, gray_alpha, rgb, bgr, rgba, bgra };
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelFormat {
    /// 1 byte per pixel grayscale.
    Gray,
    /// 2 bytes per pixel grayscale + alpha.
    GrayAlpha,
    /// 3 bytes per pixel RGB.
    Rgb,
    /// 3 bytes per pixel BGR.
    Bgr,
    /// 4 byte per pixel RGBA.
    Rgba,
    /// 4 byte per pixel BGRA.
    Bgra,
}

impl PixelFormat {
    /// Get bytes per pixel for this format.
    // pub inline fn bpp(self: PixelFormat) usize
    pub fn bpp(self) -> usize {
        match self {
            PixelFormat::Gray => 1,
            PixelFormat::GrayAlpha => 2,
            PixelFormat::Rgb => 3,
            PixelFormat::Bgr => 3,
            PixelFormat::Rgba => 4,
            PixelFormat::Bgra => 4,
        }
    }
}

impl Image {
    pub fn deinit<A: Allocator>(self, alloc: A) {
        match self {
            // .pending, .unload_pending => |p| alloc.free(p.dataSlice()),
            Image::Pending(p) | Image::UnloadPending(p) => free_data(alloc, p),

            // .replace, .unload_replace => |r| { alloc.free(r.pending.dataSlice()); r.texture.deinit(); },
            Image::Replace(r) | Image::UnloadReplace(r) => {
                free_data(alloc, r.pending);
                drop(r.texture); // r.texture.deinit() → ARC release via `Retained` Drop
            }

            // .ready, .unload_ready => |t| t.deinit(),
            Image::Ready(t) | Image::UnloadReady(t) => drop(t),
        }
    }

    /// Mark this image for unload whatever state it is in.
    pub fn mark_for_unload(&mut self) {
        // The already-unloading states are returned unchanged (no move needed).
        if matches!(
            self,
            Image::UnloadPending(_) | Image::UnloadReplace(_) | Image::UnloadReady(_)
        ) {
            return;
        }
        // Now `self` is ready/pending/replace; re-tag by moving the payload.
        map_in_place(self, |old| match old {
            Image::Ready(t) => Image::UnloadReady(t),
            Image::Pending(p) => Image::UnloadPending(p),
            Image::Replace(r) => Image::UnloadReplace(r),
            // Handled by the early return above.
            other => other,
        });
    }

    /// Mark the current image to be replaced with a pending one. This will
    /// attempt to update the existing texture if we have one, otherwise it
    /// will act like a new upload.
    pub fn mark_for_replace<A: Allocator>(&mut self, alloc: A, img: Image) {
        assert(img.is_pending());

        // If we have pending data right now, free it.
        if let Some(p) = self.get_pending() {
            free_data(alloc, p);
        }
        // `img.getPending().?` — a `Pending` (Copy), shared into the replace/pending.
        let new_pending = img.get_pending().unwrap();
        // If we have an existing texture, use it in the replace.
        // Otherwise we just become a pending image.
        //
        // PORT NOTE: `get_texture` returns a borrow for `draw`/`has_texture`;
        // here we must MOVE the owned `Texture` out of the old value — see
        // `into_texture` — because `Texture` is not `Copy`.
        map_in_place(self, |old| match old.into_texture() {
            Some(texture) => Image::Replace(Replace {
                texture,
                pending: new_pending,
            }),
            None => Image::Pending(new_pending),
        });
    }

    /// Returns true if this image is pending upload.
    pub fn is_pending(&self) -> bool {
        self.get_pending().is_some()
    }

    /// Returns true if this image has an associated texture.
    pub fn has_texture(&self) -> bool {
        self.get_texture().is_some()
    }

    /// Returns true if this image is marked for unload.
    pub fn is_unloading(&self) -> bool {
        match self {
            Image::UnloadPending(_) | Image::UnloadReplace(_) | Image::UnloadReady(_) => true,
            Image::Pending(_) | Image::Replace(_) | Image::Ready(_) => false,
        }
    }

    // PORT NOTE (`b17abd96d`): the union-level `convert`/`prepForUpload` moved
    // onto `Image.Pending` (`convert_replace`/`convert_copy`/`prep_for_upload`
    // above); `169213cd2` fused prep_image's copy+convert into `convert_copy`.

    /// Upload the pending image to the GPU and change the state of this
    /// image to ready.
    //
    // PORT NOTE (adversary fix, §4): FULL wiring above the swizzle seam. Its
    // DIRECT deps (`Texture::init`, `api.image_texture_options`) are landed.
    // The golden never calls it.
    pub fn upload<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        api: &Metal,
    ) -> Result<(), UploadError> {
        assert(self.is_pending());

        // Get our pending info
        // const p = self.getPendingPointer().?;
        //
        // No error recover is required after this call because it just
        // converts in place and is idempotent.
        // try p.prepForUpload(alloc);
        self.get_pending_pointer()
            .unwrap()
            .prep_for_upload(alloc)
            .map_err(UploadError::Wuffs)?;
        // (the `&mut` borrow ends here; re-read the now-converted pending BY
        // VALUE for the texture init — observably identical to the Zig `p`
        // pointer reuse, which reads the same converted fields.)
        let p = self.get_pending().unwrap();

        // Create our texture
        let texture = Texture::init(
            api.image_texture_options(ImageTextureFormat::Rgba, true),
            p.width as usize,
            p.height as usize,
            Some(p.data_slice()),
        )
        .map_err(|_| UploadError::UploadFailed)?;
        // errdefer comptime unreachable;  → the tail below is infallible.

        // Uploaded. We can now clear our data and change our state.
        //
        // NOTE: For the `replace` state, this will free the old texture.
        //       We don't currently actually replace the existing texture
        //       in-place but that is an optimization we can do later.
        // self.deinit(alloc); self.* = .{ .ready = texture };
        map_in_place(self, |old| {
            old.deinit(alloc);
            Image::Ready(texture)
        });
        Ok(())
    }

    /// Returns any pending image data for this image that requires upload.
    ///
    /// If there is no pending data to upload, returns null.
    fn get_pending(&self) -> Option<Pending> {
        match self {
            Image::Pending(p) | Image::UnloadPending(p) => Some(*p),
            Image::Replace(r) | Image::UnloadReplace(r) => Some(r.pending),
            _ => None,
        }
    }

    /// Returns the texture for this image.
    ///
    /// If there is no texture for it yet, returns null.
    //
    // PORT NOTE (adversary fix): Zig `getTexture` returns the `Texture` handle by
    // VALUE (a copy). The Rust `Texture` is not `Copy` (owns a `Retained`), so
    // this borrows for the read-only callers (`draw`/`has_texture`). The one
    // caller that must MOVE the texture (`mark_for_replace`) uses `into_texture`
    // (by value) instead. Used by `has_texture`.
    fn get_texture(&self) -> Option<&Texture> {
        match self {
            Image::Ready(t) | Image::UnloadReady(t) => Some(t),
            Image::Replace(r) | Image::UnloadReplace(r) => Some(&r.texture),
            _ => None,
        }
    }

    /// Consuming variant of `get_texture`: moves the owned `Texture` out of
    /// `self`, dropping the rest. Used by `mark_for_replace` (the reference's
    /// by-value `getTexture` return).
    fn into_texture(self) -> Option<Texture> {
        match self {
            Image::Ready(t) | Image::UnloadReady(t) => Some(t),
            Image::Replace(r) | Image::UnloadReplace(r) => Some(r.texture),
            _ => None,
        }
    }

    // Same as getPending but returns a pointer instead of a copy.
    //
    // PORT NOTE: used by the `convert` swizzle to mutate the pending pixel data
    // in place (Zig `getPendingPointer`).
    fn get_pending_pointer(&mut self) -> Option<&mut Pending> {
        match self {
            Image::Pending(p) => Some(p),
            Image::UnloadPending(p) => Some(p),
            Image::Replace(r) => Some(&mut r.pending),
            Image::UnloadReplace(r) => Some(&mut r.pending),
            _ => None,
        }
    }
}

/// The `wuffs.Error` set from the (unported) `wuffs` package (pixel swizzle):
/// `wuffs.Error = Allocator.Error || error{ WuffsError, Overflow }`.
//
// PORT NOTE (§3.3): the nominal set is carried in full for R1 type-fidelity to
// Zig's `wuffs.Error`. In the pure-Rust `convert` re-home the `WuffsError`
// (swizzler-`prepare` status) variant is unreachable — there is no
// `pixel_swizzler__prepare` call — and `Overflow` doubles as the defensive
// `checked_mul` guard on `px * 4`; the only genuinely-reachable failure is the
// allocation (`OutOfMemory`).
#[derive(Debug)]
pub enum WuffsError {
    OutOfMemory,
    WuffsError,
    Overflow,
}

/// `Image::upload`'s error set: `wuffs.Error || error{UploadFailed}`.
#[derive(Debug)]
pub enum UploadError {
    /// `wuffs.Error` — see [`WuffsError`] (the `convert` swizzle).
    Wuffs(WuffsError),
    /// Texture creation failed, usually a GPU memory issue.
    UploadFailed,
}

/// Replace `*self` by mapping its current value through `f`.
//
// PORT NOTE: Zig freely does `self.* = switch (self.*) {...}`, moving union
// payloads (e.g. a `Texture` handle) out of and back into `self`. Rust has no
// by-value move through `&mut` without a sentinel, so this uses the standard
// read/replace/write take-pattern (cf. `take_mut::take`). `f` is a total,
// non-panicking match in every call site, so the transient duplicate on the
// stack is never observed.
fn map_in_place<T>(slot: &mut T, f: impl FnOnce(T) -> T) {
    // SAFETY: `slot` is a valid `&mut T`. `read` moves the value out (ownership
    // transferred to `old`); `write` stores the mapped value back without
    // dropping the (moved-from) original. `f` does not panic.
    unsafe {
        let old = core::ptr::read(slot);
        let new = f(old);
        core::ptr::write(slot, new);
    }
}

/// Allocate `len` uninitialized bytes with `align = 1`, the layout `dupe_bytes`
/// and `free_data`/`free_raw` all agree on (so any buffer allocated here is
/// releasable by `free_data`/`free_raw`/`Image::deinit` identically). This is
/// the single allocation primitive backing `swizzle_to_rgba` (and through it
/// `Pending::convert_copy`/`convert_replace`) so the `Layout` logic lives in
/// one place.
fn alloc_bytes<A: Allocator>(alloc: A, len: usize) -> Result<*mut u8, AllocError> {
    let layout = Layout::from_size_align(len, 1).map_err(|_| AllocError)?;
    let ptr = alloc.allocate(layout)?;
    Ok(ptr.cast::<u8>().as_ptr())
}

/// Deallocate a raw `len`-byte buffer allocated by `alloc_bytes` (align 1). Used
/// by `Pending::convert_replace` to free the pre-swizzle source buffer
/// (`alloc.free(data)`).
fn free_raw<A: Allocator>(alloc: A, data: *mut u8, len: usize) {
    let layout = Layout::from_size_align(len, 1).expect("valid layout (matched a prior alloc)");
    if let Some(ptr) = NonNull::new(data) {
        // SAFETY: `ptr`/`layout` were produced by `alloc_bytes` via the same
        // allocator; the caller owns the buffer.
        unsafe {
            alloc.deallocate(ptr, layout);
        }
    }
}

// PORT NOTE (`169213cd2`): `dupe_bytes` (the `alloc.dupe` shim) was DELETED —
// its only consumer, `prep_image`'s copy-then-convert, fused into
// `Pending::convert_copy` (the `Rgba` memcpy arm of `swizzle_to_rgba`).

/// `alloc.free(p.dataSlice())` — deallocate a `Pending`'s owned buffer.
//
// PORT NOTE (§7 risk #3): the `Layout` is `(len(), align 1)`, matching the layout
// `alloc_bytes` allocated with; the *same* allocator instance must be used across
// alloc/free (the Zig invariant).
fn free_data<A: Allocator>(alloc: A, p: Pending) {
    let layout = Layout::from_size_align(p.len(), 1).expect("valid layout (matched a prior alloc)");
    if let Some(ptr) = NonNull::new(p.data) {
        // SAFETY: `ptr`/`layout` were produced by `alloc_bytes` via the same
        // allocator; `p` owns the buffer (only owned `Pending`s reach `deinit`).
        unsafe {
            alloc.deallocate(ptr, layout);
        }
    }
}

// PORT NOTE (rustty-only smoke tests, NOT ghostty parity tests): `image.zig`
// carries 0 `test{}` blocks (golden-image-gated). These two prove the FULL
// container path over an empty map + `None` overlay compiles and runs without
// panic; the binding gate for `draw`/`upload` over real placements is the
// Chunk-4 offscreen golden. Skipped cleanly on hosts with no Metal device.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::metal::metal_impl::Metal;
    use crate::size::AlphaBlending;
    use rustty_vt::alloc::Global;

    #[test]
    fn image_empty_lifecycle() {
        let alloc = Global;
        // Need a live `Metal` for `upload`'s signature; the empty-map loop body
        // never touches it. Skip cleanly on headless CI.
        let metal = match Metal::init_offscreen(AlphaBlending::Native, 4, 4) {
            Ok(m) => m,
            Err(e) => {
                // F5 strict-mode pivot (metal::test_support).
                crate::metal::test_support::require_metal_or_skip(
                    "image_empty_lifecycle",
                    &format!("{e:?}"),
                );
                eprintln!("no Metal device available; skipping image_empty_lifecycle");
                return;
            }
        };

        let mut s = State::empty();
        // FULL container path over the empty map / `None` overlay:
        assert!(s.upload(alloc, &metal));
        s.overlay_update(alloc, None).unwrap();
        assert!(s.images.is_empty());
        assert!(s.overlay_placements.is_empty());
        s.deinit(alloc);
    }

    #[test]
    fn id_z_less_than_ordering() {
        // kitty always sorts before overlay (different tags)
        assert!(Id::z_less_than(Id::Kitty(0), Id::Overlay));
        assert!(!Id::z_less_than(Id::Overlay, Id::Kitty(0)));
        // same-tag kitty sorts by id
        assert!(Id::z_less_than(Id::Kitty(1), Id::Kitty(2)));
        assert!(!Id::z_less_than(Id::Kitty(2), Id::Kitty(1)));
        // overlay has no sensical ordering
        assert!(!Id::z_less_than(Id::Overlay, Id::Overlay));
    }

    /// A same-id image whose generation has NOT advanced must be treated as
    /// identical and left alone; a same-id image with a NEW generation must
    /// replace the stored one.
    ///
    /// PORT NOTE (P3-W8 T8 Step 2, oracle `bdc0b6c19`): the generation stamps
    /// that replaced `transmit_time` are ported (`next_generation`, the
    /// `ImageEntry.generation` field and `prep_image`'s early return) but had no
    /// dedicated staleness test — the kitty renderer tests only ever exercise
    /// the fresh path, so a `prep_image` with the comparison deleted would have
    /// passed the whole suite.
    #[test]
    fn prep_image_ignores_a_stale_generation() {
        let alloc = Global;
        let mut s = State::empty();
        let id = Id::Kitty(7);

        // 2x1 RGBA.
        let first = [0x11u8; 8];
        let second = [0x22u8; 8];
        let mk = |buf: &[u8; 8]| Pending {
            width: 2,
            height: 1,
            pixel_format: PixelFormat::Rgba,
            data: buf.as_ptr() as *mut u8,
        };

        s.prep_image(alloc, id, 1, mk(&first)).unwrap();
        assert_eq!(s.images.get(&id).unwrap().generation, 1);
        assert_eq!(
            s.images
                .get(&id)
                .unwrap()
                .image
                .get_pending()
                .unwrap()
                .data_slice(),
            &first[..]
        );

        // Same generation, different bytes: rejected as identical, the stored
        // image keeps the FIRST payload.
        s.prep_image(alloc, id, 1, mk(&second)).unwrap();
        assert_eq!(s.images.get(&id).unwrap().generation, 1);
        assert_eq!(
            s.images
                .get(&id)
                .unwrap()
                .image
                .get_pending()
                .unwrap()
                .data_slice(),
            &first[..],
            "a stale (unchanged) generation must not replace the stored image"
        );

        // A new generation replaces it.
        s.prep_image(alloc, id, 2, mk(&second)).unwrap();
        assert_eq!(s.images.get(&id).unwrap().generation, 2);
        assert_eq!(
            s.images
                .get(&id)
                .unwrap()
                .image
                .get_pending()
                .unwrap()
                .data_slice(),
            &second[..],
            "a fresh generation must replace the stored image"
        );

        s.deinit(alloc);
    }

    #[test]
    fn pixel_format_bpp() {
        assert_eq!(PixelFormat::Gray.bpp(), 1);
        assert_eq!(PixelFormat::GrayAlpha.bpp(), 2);
        assert_eq!(PixelFormat::Rgb.bpp(), 3);
        assert_eq!(PixelFormat::Bgr.bpp(), 3);
        assert_eq!(PixelFormat::Rgba.bpp(), 4);
        assert_eq!(PixelFormat::Bgra.bpp(), 4);
    }

    // PORT NOTE: the tests construct `terminal.Terminal.init(io, alloc, .{ .rows,
    // .cols })` relying on the reference `Terminal.Options` field defaults; this
    // helper supplies them verbatim (the graphics_storage.rs / graphics_exec.rs
    // test-`opts` shape).
    fn opts(
        cols: rustty_vt::size::CellCountInt,
        rows: rustty_vt::size::CellCountInt,
    ) -> rustty_vt::terminal::Options {
        rustty_vt::terminal::Options {
            cols,
            rows,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            colors: rustty_vt::terminal::Colors::default(),
            default_modes: rustty_vt::modes::ModePacked::default(),
            default_cursor_style: rustty_vt::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
        }
    }

    // test "kitty renderer ignores pending payloads and removes replaced placements"
    #[test]
    fn kitty_renderer_ignores_pending_payloads_and_removes_replaced_placements() {
        use rustty_vt::kitty::graphics_image::Data;

        let alloc = Global;

        let mut t = rustty_vt::terminal::Terminal::init(alloc, opts(3, 3)).unwrap();
        t.width_px = 30;
        t.height_px = 30;

        let mut state: State = State::empty();

        // const storage = &t.screens.active.kitty_images;
        // (re-derived from the raw screen pointer per access, per the storage's
        // `s: *mut Screen` PORT NOTE.)
        let sp = t.screens.active;
        let tracked = unsafe { &(*sp).pages }.count_tracked_pins();
        let pending = unsafe { &mut (*sp).kitty_images }
            .add_pending_image(
                alloc,
                sp,
                KittyImage {
                    id: 1,
                    width: 1,
                    height: 1,
                    format: Format::Rgba,
                    data: Data::Pending(4),
                    ..KittyImage::default()
                },
            )
            .unwrap();
        // const pin = try t.screens.active.pages.trackPin(
        //     t.screens.active.cursor.page_pin.*,
        // );
        let pin = {
            let cursor_pin = unsafe { *(*sp).cursor.page_pin };
            unsafe { &mut (*sp).pages }.track_pin(cursor_pin).unwrap()
        };
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                1,
                KittyStoragePlacement {
                    columns: 1,
                    rows: 1,
                    ..KittyStoragePlacement::with_location(Location::Pin(pin))
                },
            )
            .unwrap();

        state.kitty_update(
            alloc,
            &t,
            CellSize {
                width: 10,
                height: 10,
            },
        );
        assert_eq!(1usize, unsafe { &(*sp).kitty_images }.placements.len());
        assert_eq!(0usize, state.kitty_placements.len());
        // `.get(..).is_none()`, not `!contains_key(..)`: the reference asserts
        // `state.images.get(...) == null`, and the port mirrors that.
        #[allow(clippy::unnecessary_get_then_check)]
        {
            assert!(state.images.get(&Id::Kitty(1)).is_none());
        }

        // const pixels = try alloc.dupe(u8, "rgba");
        let pixels = b"rgba".to_vec();
        assert!(pending.complete(
            unsafe { &mut (*sp).kitty_images },
            rustty_vt::alloc::heap_bytes(&pixels)
        ));
        state.kitty_update(
            alloc,
            &t,
            CellSize {
                width: 10,
                height: 10,
            },
        );
        assert_eq!(1usize, state.kitty_placements.len());
        assert_eq!(
            pending.generation,
            state.images.get(&Id::Kitty(1)).unwrap().generation
        );

        // A newer pending replacement deletes the native placement and marks the
        // copied texture data for unload.
        let _ = unsafe { &mut (*sp).kitty_images }
            .add_pending_image(
                alloc,
                sp,
                KittyImage {
                    id: 1,
                    width: 1,
                    height: 1,
                    format: Format::Rgba,
                    data: Data::Pending(4),
                    ..KittyImage::default()
                },
            )
            .unwrap();
        state.kitty_update(
            alloc,
            &t,
            CellSize {
                width: 10,
                height: 10,
            },
        );
        assert_eq!(0usize, unsafe { &(*sp).kitty_images }.placements.len());
        assert_eq!(tracked, unsafe { &(*sp).pages }.count_tracked_pins());
        assert_eq!(0usize, state.kitty_placements.len());
        assert!(state
            .images
            .get(&Id::Kitty(1))
            .unwrap()
            .image
            .is_unloading());

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "kitty renderer uses the intersected source rectangle"
    #[test]
    fn kitty_renderer_uses_the_intersected_source_rectangle() {
        use rustty_vt::kitty::graphics_image::Data;

        let alloc = Global;

        let mut t = rustty_vt::terminal::Terminal::init(alloc, opts(3, 3)).unwrap();
        t.width_px = 30;
        t.height_px = 30;

        let mut state: State = State::empty();

        // const storage = &t.screens.active.kitty_images;
        let sp = t.screens.active;
        // const pixels = try alloc.alloc(u8, 4 * 3 * 3);
        // @memset(pixels, 0);
        let pixels = vec![0u8; 4 * 3 * 3];
        unsafe { &mut (*sp).kitty_images }
            .add_image(
                alloc,
                sp,
                KittyImage {
                    id: 1,
                    width: 4,
                    height: 3,
                    format: Format::Rgb,
                    data: Data::Complete(rustty_vt::alloc::heap_bytes(&pixels)),
                    ..KittyImage::default()
                },
            )
            .unwrap();
        // const pin = try t.screens.active.pages.trackPin(
        //     t.screens.active.cursor.page_pin.*,
        // );
        let pin = {
            let cursor_pin = unsafe { *(*sp).cursor.page_pin };
            unsafe { &mut (*sp).pages }.track_pin(cursor_pin).unwrap()
        };
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                1,
                KittyStoragePlacement {
                    source_x: 3,
                    source_y: 1,
                    ..KittyStoragePlacement::with_location(Location::Pin(pin))
                },
            )
            .unwrap();

        state.kitty_update(
            alloc,
            &t,
            CellSize {
                width: 10,
                height: 10,
            },
        );
        assert_eq!(1usize, state.kitty_placements.len());

        let placement = state.kitty_placements[0];
        assert_eq!(1u32, placement.width);
        assert_eq!(2u32, placement.height);
        assert_eq!(3u32, placement.source_x);
        assert_eq!(1u32, placement.source_y);
        assert_eq!(1u32, placement.source_width);
        assert_eq!(2u32, placement.source_height);

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "kitty renderer positions relative placements from the parent pin"
    #[test]
    fn kitty_renderer_positions_relative_placements_from_the_parent_pin() {
        use rustty_vt::kitty::graphics_image::Data;
        use rustty_vt::kitty::graphics_storage::{PlacementId, PlacementIdTag, Relative};

        let alloc = Global;

        let mut t = rustty_vt::terminal::Terminal::init(alloc, opts(10, 10)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut state: State = State::empty();

        let sp = t.screens.active;
        let pixels = vec![0u8; 3];
        unsafe { &mut (*sp).kitty_images }
            .add_image(
                alloc,
                sp,
                KittyImage {
                    id: 1,
                    width: 1,
                    height: 1,
                    format: Format::Rgb,
                    data: Data::Complete(rustty_vt::alloc::heap_bytes(&pixels)),
                    ..KittyImage::default()
                },
            )
            .unwrap();

        // Parent at (2, 1).
        // const pin = try t.screens.active.pages.trackPin(
        //     t.screens.active.pages.pin(.{ .active = .{ .x = 2, .y = 1 } }).?,
        // );
        let pin = {
            let p = unsafe { &(*sp).pages }
                .pin(point::Point::Active(point::Coordinate { x: 2, y: 1 }))
                .unwrap();
            unsafe { &mut (*sp).pages }.track_pin(p).unwrap()
        };
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                1,
                KittyStoragePlacement {
                    columns: 1,
                    rows: 1,
                    ..KittyStoragePlacement::with_location(Location::Pin(pin))
                },
            )
            .unwrap();

        // Child offset (3, 2) cells from the parent.
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                2,
                KittyStoragePlacement {
                    columns: 1,
                    rows: 1,
                    z: 1,
                    ..KittyStoragePlacement::with_location(Location::Relative(Relative {
                        parent: PlacementKey {
                            image_id: 1,
                            placement_id: PlacementId {
                                tag: PlacementIdTag::External,
                                id: 1,
                            },
                        },
                        horizontal_offset: 3,
                        vertical_offset: 2,
                    }))
                },
            )
            .unwrap();

        state.kitty_update(
            alloc,
            &t,
            CellSize {
                width: 10,
                height: 10,
            },
        );
        assert_eq!(2usize, state.kitty_placements.len());

        // Sorted by z: parent (z=0) first, child (z=1) second.
        let parent = state.kitty_placements[0];
        assert_eq!(2i32, parent.x);
        assert_eq!(1i32, parent.y);
        let child = state.kitty_placements[1];
        assert_eq!(5i32, child.x);
        assert_eq!(3i32, child.y);

        // Pin-rooted relative placements don't force per-frame rebuilds.
        assert!(!state.kitty_virtual);

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "kitty renderer relative placement with negative offsets"
    #[test]
    fn kitty_renderer_relative_placement_with_negative_offsets() {
        use rustty_vt::kitty::graphics_image::Data;
        use rustty_vt::kitty::graphics_storage::{PlacementId, PlacementIdTag, Relative};

        let alloc = Global;

        let mut t = rustty_vt::terminal::Terminal::init(alloc, opts(10, 10)).unwrap();
        t.width_px = 100;
        t.height_px = 100;

        let mut state: State = State::empty();

        let sp = t.screens.active;
        let pixels = vec![0u8; 3];
        unsafe { &mut (*sp).kitty_images }
            .add_image(
                alloc,
                sp,
                KittyImage {
                    id: 1,
                    width: 1,
                    height: 1,
                    format: Format::Rgb,
                    data: Data::Complete(rustty_vt::alloc::heap_bytes(&pixels)),
                    ..KittyImage::default()
                },
            )
            .unwrap();

        let pin = {
            let p = unsafe { &(*sp).pages }
                .pin(point::Point::Active(point::Coordinate { x: 2, y: 2 }))
                .unwrap();
            unsafe { &mut (*sp).pages }.track_pin(p).unwrap()
        };
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                1,
                KittyStoragePlacement {
                    columns: 1,
                    rows: 1,
                    ..KittyStoragePlacement::with_location(Location::Pin(pin))
                },
            )
            .unwrap();

        // Hangs up and to the left of the viewport but still has visible
        // cells at (0, 0), so it must be kept with a negative position.
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                2,
                KittyStoragePlacement {
                    columns: 2,
                    rows: 2,
                    z: 1,
                    ..KittyStoragePlacement::with_location(Location::Relative(Relative {
                        parent: PlacementKey {
                            image_id: 1,
                            placement_id: PlacementId {
                                tag: PlacementIdTag::External,
                                id: 1,
                            },
                        },
                        horizontal_offset: -3,
                        vertical_offset: -3,
                    }))
                },
            )
            .unwrap();

        // Entirely off the left edge of the screen: culled.
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                3,
                KittyStoragePlacement {
                    columns: 2,
                    rows: 2,
                    z: 2,
                    ..KittyStoragePlacement::with_location(Location::Relative(Relative {
                        parent: PlacementKey {
                            image_id: 1,
                            placement_id: PlacementId {
                                tag: PlacementIdTag::External,
                                id: 1,
                            },
                        },
                        horizontal_offset: -9,
                        vertical_offset: 0,
                    }))
                },
            )
            .unwrap();

        state.kitty_update(
            alloc,
            &t,
            CellSize {
                width: 10,
                height: 10,
            },
        );
        assert_eq!(2usize, state.kitty_placements.len());
        let child = state.kitty_placements[1];
        assert_eq!(-1i32, child.x);
        assert_eq!(-1i32, child.y);

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "kitty renderer positions relative placements from virtual parent placeholders"
    #[test]
    fn kitty_renderer_positions_relative_placements_from_virtual_parent_placeholders() {
        use rustty_vt::kitty::graphics_image::Data;
        use rustty_vt::kitty::graphics_storage::{PlacementId, PlacementIdTag, Relative};
        use rustty_vt::sgr::Attribute;

        let alloc = Global;

        let mut t = rustty_vt::terminal::Terminal::init(alloc, opts(5, 5)).unwrap();
        t.width_px = 50;
        t.height_px = 50;
        t.modes.set(rustty_vt::modes::Mode::GraphemeCluster, true);

        let mut state: State = State::empty();

        let sp = t.screens.active;
        let pixels = vec![0u8; 10 * 10 * 3];
        unsafe { &mut (*sp).kitty_images }
            .add_image(
                alloc,
                sp,
                KittyImage {
                    id: 1,
                    width: 10,
                    height: 10,
                    format: Format::Rgb,
                    data: Data::Complete(rustty_vt::alloc::heap_bytes(&pixels)),
                    ..KittyImage::default()
                },
            )
            .unwrap();
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                1,
                KittyStoragePlacement::with_location(Location::Virtual),
            )
            .unwrap();

        // Placeholder cells for the virtual placement at (1, 1) and
        // (3, 2): the minimum cell (1, 1) is the anchor for relative
        // placements rooted at the virtual placement.
        t.set_attribute(Attribute::Color256Fg(1)).unwrap();
        t.set_attribute(Attribute::Color256Underline(1)).unwrap();
        unsafe { (*sp).cursor_absolute(1, 1) };
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();
        unsafe { (*sp).cursor_absolute(3, 2) };
        t.print_string("\u{10EEEE}\u{0305}\u{0305}".as_bytes())
            .unwrap();

        // Child of the virtual placement, offset (1, 2) cells.
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                2,
                KittyStoragePlacement {
                    columns: 1,
                    rows: 1,
                    z: 5,
                    ..KittyStoragePlacement::with_location(Location::Relative(Relative {
                        parent: PlacementKey {
                            image_id: 1,
                            placement_id: PlacementId {
                                tag: PlacementIdTag::External,
                                id: 1,
                            },
                        },
                        horizontal_offset: 1,
                        vertical_offset: 2,
                    }))
                },
            )
            .unwrap();

        state.kitty_update(
            alloc,
            &t,
            CellSize {
                width: 10,
                height: 10,
            },
        );
        assert!(state.kitty_virtual);

        // Two placeholder runs (z=-1) plus the child (z=5), sorted by z.
        assert_eq!(3usize, state.kitty_placements.len());
        let child = state.kitty_placements[2];
        assert_eq!(5i32, child.z);
        assert_eq!(2i32, child.x);
        assert_eq!(3i32, child.y);

        state.deinit(alloc);
        t.deinit(alloc);
    }

    // test "kitty renderer uploads the current animation frame"
    #[test]
    fn kitty_renderer_uploads_the_current_animation_frame() {
        use rustty_vt::kitty::graphics_animation::{Animation, Frame};
        use rustty_vt::kitty::graphics_image::Data;

        let alloc = Global;

        let mut t = rustty_vt::terminal::Terminal::init(alloc, opts(3, 3)).unwrap();
        t.width_px = 30;
        t.height_px = 30;

        let mut state: State = State::empty();

        let sp = t.screens.active;
        unsafe { &mut (*sp).kitty_images }
            .add_image(
                alloc,
                sp,
                KittyImage {
                    id: 1,
                    width: 1,
                    height: 1,
                    format: Format::Rgba,
                    // .data = .{ .complete = try alloc.dupe(u8, &.{ 255, 0, 0, 255 }) },
                    data: Data::Complete(rustty_vt::alloc::heap_bytes(&[255, 0, 0, 255])),
                    ..KittyImage::default()
                },
            )
            .unwrap();
        let pin = {
            let cursor_pin = unsafe { *(*sp).cursor.page_pin };
            unsafe { &mut (*sp).pages }.track_pin(cursor_pin).unwrap()
        };
        unsafe { &mut (*sp).kitty_images }
            .add_placement(
                alloc,
                sp,
                1,
                1,
                KittyStoragePlacement {
                    columns: 1,
                    rows: 1,
                    ..KittyStoragePlacement::with_location(Location::Pin(pin))
                },
            )
            .unwrap();

        state.kitty_update(
            alloc,
            &t,
            CellSize {
                width: 10,
                height: 10,
            },
        );
        let gen1 = state.images.get(&Id::Kitty(1)).unwrap().generation;
        assert_eq!(
            &[255u8, 0, 0, 255][..],
            state
                .images
                .get(&Id::Kitty(1))
                .unwrap()
                .image
                .get_pending()
                .unwrap()
                .data_slice(),
        );

        // Attach an animation and make its extra frame current, the way
        // an animation tick would.
        // const img = storage.images.getPtr(1).?;
        // const anim = try alloc.create(terminal.kitty.graphics.Animation);
        // anim.* = .{};
        // img.animation = anim;
        // try anim.frames.append(alloc, .{ .data = ..., .gap_ms = 40 });
        // anim.current_index = 1;
        // storage.markImageContentChanged(io, img);
        {
            let mut anim = rustty_vt::alloc::heap_box(Animation::default());
            anim.frames.push(Frame {
                data: rustty_vt::alloc::heap_bytes(&[0, 0, 255, 255]),
                gap_ms: 40,
            });
            anim.current_index = 1;
            let img = unsafe { &mut (*sp).kitty_images }
                .images
                .get_mut(&1)
                .unwrap();
            img.animation = Some(anim);
        }
        unsafe { &mut (*sp).kitty_images }.mark_image_content_changed(1);

        // The renderer must pick up the frame's pixels under a fresh
        // generation.
        state.kitty_update(
            alloc,
            &t,
            CellSize {
                width: 10,
                height: 10,
            },
        );
        let entry = state.images.get(&Id::Kitty(1)).unwrap();
        assert!(entry.generation > gen1);
        assert_eq!(
            &[0u8, 0, 255, 255][..],
            entry.image.get_pending().unwrap().data_slice(),
        );

        state.deinit(alloc);
        t.deinit(alloc);
    }
}
