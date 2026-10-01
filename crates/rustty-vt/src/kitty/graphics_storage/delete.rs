//! PORTS: ghostty src/terminal/kitty/graphics_storage.zig — the ImageStorage
//! `clearScreen` / `delete` cluster (`deleteVisiblePlacements`, `deleteById`,
//! `deleteAnimationFrame`, `deleteIfUnused`, `deleteIntersecting`) and image
//! eviction (`evictImage`, `evictImageExcept`).

use super::placement::Candidate;
use super::{
    ImageStorage, Location, Placement, PlacementId, PlacementIdTag, PlacementKey, LOG_SCOPE,
};
use crate::alloc::Allocator;
use crate::kitty::graphics_command as command;
use crate::kitty::graphics_image;
use crate::point;
use crate::screen::Screen;
use crate::size;
use crate::terminal::Terminal;
use rustty_util::quirks::inlineAssert as assert;

impl ImageStorage {
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
                        // (the image is borrowed: only its id and the placement rect
                        // are needed, and the storage is mutated below.)
                        let (img_id, rect) = {
                            let img = match self.image_by_id(key.image_id) {
                                Some(img) => img,
                                None => continue,
                            };
                            match self.placements.get(&key).unwrap().rect(img, t) {
                                Some(r) => (img.id, r),
                                None => continue,
                            }
                        };
                        if rect.top_left.x as u32 <= x && rect.bottom_right.x as u32 >= x {
                            self.remove_placement(t.screens.active, &key);
                            if delete {
                                self.delete_if_unused(alloc, img_id);
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
                        // (the image is borrowed: only its id and the placement rect
                        // are needed, and the storage is mutated below.)
                        let (img_id, rect) = {
                            let img = match self.image_by_id(key.image_id) {
                                Some(img) => img,
                                None => continue,
                            };
                            match self.placements.get(&key).unwrap().rect(img, t) {
                                Some(r) => (img.id, r),
                                None => continue,
                            }
                        };

                        // We need to copy our pin to ensure we are at least at
                        // the top-left x.
                        let mut target_pin_copy = target_pin;
                        target_pin_copy.x = rect.top_left.x;
                        if target_pin_copy.is_between(rect.top_left, rect.bottom_right) {
                            self.remove_placement(t.screens.active, &key);
                            if delete {
                                self.delete_if_unused(alloc, img_id);
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
                let rect = match self.placements.get(&key).unwrap().rect(image, t) {
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
    pub(super) fn delete_if_unused(&mut self, alloc: Allocator, image_id: u32) {
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
            // (the image is borrowed: only its id and the placement rect
            // are needed, and the storage is mutated below.)
            let (img_id, rect) = {
                let img = match self.image_by_id(key.image_id) {
                    Some(img) => img,
                    None => continue,
                };
                match self.placements.get(&key).unwrap().rect(img, t) {
                    Some(r) => (img.id, r),
                    None => continue,
                }
            };
            // if (rect.contains(target_pin)) { ... }
            if rect.contains(target_pin) {
                // if (filter) |f| if (!f(filter_ctx, entry.value_ptr.*)) continue;
                if !filter(self.placements.get(&key).unwrap()) {
                    continue;
                }
                self.remove_placement(t.screens.active, &key);
                if delete_unused {
                    self.delete_if_unused(alloc, img_id);
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
    pub(super) fn evict_image(&mut self, alloc: Allocator, s: *mut Screen, req: usize) -> bool {
        self.evict_image_except(alloc, s, req, None)
    }

    /// Evict images while preserving `exclude_id`, used when replacing an
    /// existing image whose old reservation has already been credited.
    pub(super) fn evict_image_except(
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
            log::info!(target: LOG_SCOPE, "evicting image id={} bytes={image_len}", c.id);

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
