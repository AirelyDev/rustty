//! rustty-vt kitty/graphics_exec
//!
//! PORTS: ghostty src/terminal/kitty/graphics_exec.zig
//!
//! PORT STATUS: ported

// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
//
// const Terminal = @import("../Terminal.zig");
// const command = @import("graphics_command.zig");
// const image = @import("graphics_image.zig");
// const Command = command.Command;
// const Response = command.Response;
// const LoadingImage = image.LoadingImage;
// const Image = image.Image;
// const ImageStorage = @import("graphics_storage.zig").ImageStorage;
use crate::kitty::graphics_animation as animation;
use crate::kitty::graphics_command as command;
use crate::kitty::graphics_command::{Command, Response};
use crate::kitty::graphics_image as image;
use crate::kitty::graphics_image::{FrameContext, Image, LoadingImage};
use crate::kitty::graphics_pixel as pixel;
use crate::kitty::graphics_storage::{Location, Placement};
use crate::terminal::Terminal;
use allocator_api2::alloc::Global as Allocator;
use rustty_util::quirks::inlineAssert as assert;

// const log = std.log.scoped(.kitty_gfx);

/// Execute a Kitty graphics command against the given terminal. This
/// will never fail, but the response may indicate an error and the
/// terminal state may not be updated to reflect the command. This will
/// never put the terminal in an unrecoverable state, however.
///
/// The allocator must be the same allocator that was used to build
/// the command.
pub fn execute(alloc: Allocator, terminal: &mut Terminal, cmd: &Command) -> Option<Response> {
    // If storage is disabled then we disable the full protocol. This means
    // we don't even respond to queries so the terminal completely acts as
    // if this feature is not supported.
    // if (!terminal.screens.active.kitty_images.enabled()) {
    if !unsafe { &(*terminal.screens.active).kitty_images }.enabled() {
        // log.debug("kitty graphics requested but disabled", .{});
        return None;
    }

    // log.debug("executing kitty graphics command: quiet={} control={}", .{
    //     cmd.quiet,
    //     cmd.control,
    // });

    // The quiet settings used to control the response. We have to make this
    // a var because in certain special cases (namely chunked transmissions)
    // this can change.
    let mut quiet = cmd.quiet;

    // The protocol makes i and I mutually exclusive for every action, so this
    // must happen before dispatch and before an action can mutate storage.
    // https://sw.kovidgoyal.net/kitty/graphics-protocol/#requesting-image-ids-from-the-terminal
    let identifiers = cmd.control.identifiers();
    if identifiers.image_id > 0 && identifiers.image_number > 0 {
        let resp: Response = Response {
            id: identifiers.image_id,
            image_number: identifiers.image_number,
            placement_id: identifiers.placement_id,
            message: b"EINVAL: image ID and number are mutually exclusive".to_vec(),
            ..Response::default()
        };
        // log.warn("erroneous kitty graphics response: {s}", .{resp.message});

        return match quiet {
            command::Quiet::No => Some(resp),
            command::Quiet::Ok => Some(resp),
            command::Quiet::Failures => None,
        };
    }

    // const resp_: ?Response = switch (cmd.control) {
    let resp_: Option<Response> = match &cmd.control {
        // .query => query(alloc, terminal, cmd),
        command::Control::Query(_) => Some(query(alloc, terminal, cmd)),
        // .display => display(alloc, terminal, cmd),
        command::Control::Display(_) => Some(display(alloc, terminal, cmd)),
        // .delete => delete(alloc, terminal, cmd),
        command::Control::Delete(_) => Some(delete(alloc, terminal, cmd)),

        // .transmit, .transmit_and_display, .transmit_animation_frame => resp: {
        command::Control::Transmit(_)
        | command::Control::TransmitAndDisplay { .. }
        | command::Control::TransmitAnimationFrame(_) => {
            // If we're transmitting, then our `q` setting value is complicated.
            // The `q` setting inherits the value from the starting command
            // unless `q` is set >= 1 on this command. If it is, then we save
            // that as the new `q` setting.
            // const storage = &terminal.screens.active.kitty_images;
            let storage = unsafe { &mut (*terminal.screens.active).kitty_images };
            // if (storage.loading) |loading| switch (cmd.quiet) {
            if let Some(loading) = storage.loading.as_mut() {
                match cmd.quiet {
                    // q=0 we use whatever the start command value is
                    // .no => quiet = loading.quiet,
                    command::Quiet::No => quiet = loading.quiet,

                    // q>=1 we use the new value, but we should already be set to it
                    // inline .ok, .failures => |tag| {
                    //     assert(quiet == tag);
                    //     loading.quiet = tag;
                    // },
                    tag @ (command::Quiet::Ok | command::Quiet::Failures) => {
                        assert(quiet == tag);
                        loading.quiet = tag;
                    }
                }
            }

            // break :resp switch (cmd.control) {
            //     .transmit_animation_frame => transmitAnimationFrame(...),
            //     else => transmit(...),
            // };
            Some(match &cmd.control {
                command::Control::TransmitAnimationFrame(_) => {
                    transmit_animation_frame(alloc, terminal, cmd)
                }
                _ => transmit(alloc, terminal, cmd),
            })
        }

        // .control_animation => controlAnimation(io, alloc, terminal, cmd),
        command::Control::ControlAnimation(_) => Some(control_animation(alloc, terminal, cmd)),

        // .compose_animation => composeAnimation(io, alloc, terminal, cmd),
        command::Control::ComposeAnimation(_) => Some(compose_animation(alloc, terminal, cmd)),
    };

    // Handle the quiet settings
    // if (resp_) |resp| {
    if let Some(resp) = resp_ {
        if !resp.ok() {
            // log.warn("erroneous kitty graphics response: {s}", .{resp.message});
        }

        return match quiet {
            command::Quiet::No => {
                if resp.empty() {
                    None
                } else {
                    Some(resp)
                }
            }
            command::Quiet::Ok => {
                if resp.ok() {
                    None
                } else {
                    Some(resp)
                }
            }
            command::Quiet::Failures => None,
        };
    }

    None
}

/// Execute a "query" command.
///
/// This command is used to attempt to load an image and respond with
/// success/error but does not persist any of the command to the terminal
/// state.
fn query(alloc: Allocator, terminal: &Terminal, cmd: &Command) -> Response {
    // const t = cmd.control.query;
    let t = match &cmd.control {
        command::Control::Query(t) => *t,
        // PORT NOTE: `query` is only invoked from the `.query` arm of `execute`,
        // so this is unreachable in the reference's tagged-union access.
        _ => unreachable!(),
    };

    // Query requires image ID. We can't actually send a response without
    // an image ID either but we return an error and this will be logged
    // downstream.
    if t.image_id == 0 {
        return Response {
            message: b"EINVAL: image ID required".to_vec(),
            ..Response::default()
        };
    }

    // Build a partial response to start
    let mut result: Response = Response {
        id: t.image_id,
        image_number: t.image_number,
        placement_id: t.placement_id,
        ..Response::default()
    };

    // A query must attempt a complete load, then discard the result without
    // changing image storage.
    // https://sw.kovidgoyal.net/kitty/graphics-protocol/#querying-support-and-available-transmission-mediums
    // const storage = &terminal.screens.active.kitty_images;
    let storage = unsafe { &(*terminal.screens.active).kitty_images };
    // var loading = LoadingImage.init(io, alloc, cmd, storage.image_limits) catch |err| {
    //     encodeError(&result, err);
    //     return result;
    // };
    // defer loading.deinit(alloc);
    let _ = &alloc;
    let mut loading = match LoadingImage::init(cmd, &storage.image_limits) {
        Ok(loading) => loading,
        Err(err) => {
            encode_error(&mut result, err);
            return result;
        }
    };

    // var img = loading.complete(alloc) catch |err| { ... };
    // img.deinit(alloc);
    // PORT NOTE: both the completed image and the loading state are owned
    // values dropped at scope end (the reference's `img.deinit` + deferred
    // `loading.deinit`).
    let img = match loading.complete() {
        Ok(img) => img,
        Err(err) => {
            encode_error(&mut result, err);
            return result;
        }
    };
    drop(img);

    result
}

/// Transmit image data.
///
/// This loads the image, validates it, and puts it into the terminal
/// screen storage. It does not display the image.
fn transmit(alloc: Allocator, terminal: &mut Terminal, cmd: &Command) -> Response {
    let t = cmd.transmission().unwrap();
    // const storage = &terminal.screens.active.kitty_images;
    let storage = unsafe { &(*terminal.screens.active).kitty_images };
    // var result: Response = if (storage.loading) |loading| loading: {
    //     if (loading.frame != null) return transmitAnimationFrame(...);
    //     break :loading loading.response;
    // } else .{ .id = ..., .image_number = ..., .placement_id = ... };
    let mut result: Response = if let Some(loading) = storage.loading.as_ref() {
        // Any transmit-like command received while a load is in progress
        // is a continuation chunk of that load, and the load completes
        // according to how it started. If an animation frame load (a=f)
        // is in progress, this chunk belongs to it even when the client
        // didn't repeat a=f on the chunk.
        if loading.frame.is_some() {
            return transmit_animation_frame(alloc, terminal, cmd);
        }

        loading.response.clone()
    } else {
        Response {
            id: t.image_id,
            image_number: t.image_number,
            placement_id: t.placement_id,
            ..Response::default()
        }
    };

    // const load = loadAndAddImage(alloc, terminal, cmd) catch |err| {
    //     encodeError(&result, err);
    //     return result;
    // };
    // errdefer load.image.deinit(alloc);
    // PORT NOTE: the reference's `errdefer load.image.deinit(alloc)` frees the
    // returned image if any subsequent step fails. In this port, every step
    // after `loadAndAddImage` is infallible (no early-return error path), so the
    // `errdefer` never fires; the owned `Image` is dropped at end of scope.
    let load = match load_and_add_image(alloc, terminal, cmd) {
        Ok(load) => load,
        Err(err) => {
            encode_error(&mut result, err);
            return result;
        }
    };

    // If we're also displaying, then do that now. This function does
    // both transmit and transmit and display. The display might also be
    // deferred if it is multi-chunk.
    // if (load.display) |d| {
    if let Some(d) = load.display {
        // assert(!load.more);
        assert(!load.more);
        // var d_copy = d;
        let mut d_copy = d;
        // d_copy.image_id = load.image.id;
        d_copy.image_id = load.image.id;
        // result = display(alloc, terminal, &.{
        //     .control = .{ .display = d_copy },
        //     .quiet = cmd.quiet,
        // });
        result = display(
            alloc,
            terminal,
            &Command {
                control: command::Control::Display(d_copy),
                quiet: cmd.quiet,
                data: Vec::new(),
            },
        );
    }

    // If there are more chunks expected we do not respond.
    if load.more {
        return Response::default();
    }

    // If the loaded image was assigned its ID automatically, not based
    // on a number or explicitly specified ID, then we don't respond.
    if load.image.metadata.implicit_id {
        return Response::default();
    }

    // After the image is added, set the ID in case it changed.
    // The resulting image number and placement ID never change.
    result.id = load.image.id;

    result
}

/// Display a previously transmitted image.
fn display(alloc: Allocator, terminal: &mut Terminal, cmd: &Command) -> Response {
    let d = cmd.display().unwrap();

    // Display requires image ID or number.
    if d.image_id == 0 && d.image_number == 0 {
        return Response {
            message: b"EINVAL: image ID or number required".to_vec(),
            ..Response::default()
        };
    }

    // Build up our response
    let mut result: Response = Response {
        id: d.image_id,
        image_number: d.image_number,
        placement_id: d.placement_id,
        ..Response::default()
    };

    // A virtual placement (U=1) cannot also be a relative placement.
    // Kitty checks this before even looking up the image.
    if d.virtual_placement && d.parent_id > 0 {
        result.message = b"EINVAL: virtual placement cannot refer to a parent".to_vec();
        return result;
    }

    // Verify the requested image exists if we have an ID
    // const storage = &terminal.screens.active.kitty_images;
    let storage = unsafe { &mut (*terminal.screens.active).kitty_images };
    // const img_: ?Image = if (d.image_id != 0)
    //     storage.imageById(d.image_id)
    // else
    //     storage.imageByNumber(d.image_number);
    let img_: Option<Image> = if d.image_id != 0 {
        storage.image_by_id(d.image_id)
    } else {
        storage.image_by_number(d.image_number)
    };
    // const img = img_ orelse {
    //     result.message = "ENOENT: image not found";
    //     return result;
    // };
    let img = match img_ {
        Some(img) => img,
        None => {
            result.message = b"ENOENT: image not found".to_vec();
            return result;
        }
    };

    // Make sure our response has the image id in case we looked up by number
    result.id = img.id;

    // Location where the placement will go.
    // const location: ImageStorage.Placement.Location = location: {
    let location: Location = 'location: {
        // Virtual placements are not tracked
        // if (d.virtual_placement) break :location .virtual;
        if d.virtual_placement {
            break 'location Location::Virtual;
        }

        // No parent reference (P=): the placement is pinned to the
        // cursor. The cursor is always tracked but we don't want
        // this pin to move with the cursor.
        if d.parent_id == 0 {
            // const pin = terminal.screens.active.pages.trackPin(
            //     terminal.screens.active.cursor.page_pin.*,
            // ) catch |err| { ... };
            let active = unsafe { &mut *terminal.screens.active };
            let cursor_pin = unsafe { *active.cursor.page_pin };
            let pin = match active.pages.track_pin(cursor_pin) {
                Ok(pin) => pin,
                Err(_err) => {
                    // log.warn("failed to create pin for Kitty graphics err={}", .{err});
                    result.message = b"EINVAL: failed to prepare terminal state".to_vec();
                    return result;
                }
            };
            // break :location .{ .pin = pin };
            break 'location Location::Pin(pin);
        }

        // A parent reference makes this a relative placement: it is
        // positioned relative to the parent placement instead of the
        // cursor.

        // The key of the placement being created, when it is
        // addressable (an explicit placement ID). Needed for
        // self-parent and cycle detection.
        let child: Option<crate::kitty::graphics_storage::PlacementKey> = if d.placement_id > 0 {
            Some(crate::kitty::graphics_storage::PlacementKey {
                image_id: img.id,
                placement_id: crate::kitty::graphics_storage::PlacementId {
                    tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                    id: d.placement_id,
                },
            })
        } else {
            None
        };

        // SB: re-derive the storage reference from the raw base pointer here;
        // the `active` reference used by other paths must not be live across
        // this call (each alias stays scoped, per this file's PORT NOTEs).
        let storage = unsafe { &mut (*terminal.screens.active).kitty_images };
        let parent = match storage.resolve_parent(
            terminal.screens.active,
            child,
            d.parent_id,
            d.parent_placement_id,
        ) {
            Ok(parent) => parent,
            Err(err) => {
                use crate::kitty::graphics_storage::ParentError;
                result.message = match err {
                    ParentError::ParentImageNotFound => {
                        b"ENOPARENT: parent image not found".to_vec()
                    }
                    ParentError::ParentPlacementNotFound => {
                        b"ENOPARENT: parent placement not found".to_vec()
                    }
                    ParentError::SelfParent => {
                        b"EINVAL: placement cannot be its own parent".to_vec()
                    }
                    ParentError::Cycle => b"ECYCLE: parent chain creates a cycle".to_vec(),
                    ParentError::TooDeep => b"ETOODEEP: parent chain too deep".to_vec(),
                    ParentError::AncestorNotFound => {
                        b"ENOENT: parent chain ancestor not found".to_vec()
                    }
                };
                return result;
            }
        };

        break 'location Location::Relative(crate::kitty::graphics_storage::Relative {
            parent,
            horizontal_offset: d.horizontal_offset,
            vertical_offset: d.vertical_offset,
        });
    };

    // Add the placement
    // const p: ImageStorage.Placement = placement: { ... };
    let p: Placement = {
        let mut p: Placement = Placement {
            location,
            x_offset: d.x_offset,
            y_offset: d.y_offset,
            source_x: d.x,
            source_y: d.y,
            source_width: d.width,
            source_height: d.height,
            columns: d.columns,
            rows: d.rows,
            z: d.z,
        };

        let cell_offset = p.cell_offset(terminal);
        if terminal.width_px / terminal.cols as u32 > 0 {
            p.x_offset = cell_offset.x;
        }
        if terminal.height_px / terminal.rows as u32 > 0 {
            p.y_offset = cell_offset.y;
        }

        p
    };
    // storage.addPlacement(
    //     io,
    //     alloc,
    //     terminal.screens.active,
    //     img.id,
    //     result.placement_id,
    //     p,
    // ) catch |err| {
    //     p.deinit(terminal.screens.active);
    //     encodeError(&result, err);
    //     return result;
    // };
    // PORT NOTE: `addPlacement` borrows `storage` (a field of the active screen)
    // mutably; `p.deinit` on the error path borrows the same active screen. We
    // re-derive both raw references inside their own `unsafe` blocks so the
    // aliasing is scoped, mirroring the reference's `&...kitty_images` /
    // `terminal.screens.active` accesses. `Placement` is `Copy` (like the Zig
    // by-value struct), so the post-move reads below use the same `p`.
    //
    // SB: the `storage` reference taken at the top of `display` dies the
    // moment the cursor-pin block above takes its own
    // `&mut *terminal.screens.active` (Stacked Borrows pops the older tag;
    // Miri-caught through the vt-c kitty transmit KAT). Re-derive it from the
    // raw base pointer here so each alias stays scoped, exactly as the PORT
    // NOTE above prescribes; the reference's `storage` is a plain Zig pointer
    // with no such invalidation.
    let storage = unsafe { &mut (*terminal.screens.active).kitty_images };
    if let Err(err) = storage.add_placement(
        alloc,
        terminal.screens.active,
        img.id,
        result.placement_id,
        p,
    ) {
        // p.deinit(terminal.screens.active);
        p.deinit(terminal.screens.active);
        encode_error_storage(&mut result, err);
        return result;
    }

    // Apply cursor movement setting. This only applies to pin placements:
    // relative placements never move the cursor regardless of C=, just
    // like kitty.
    // switch (p.location) {
    match p.location {
        // .virtual, .relative => {},
        Location::Virtual | Location::Relative(_) => {}
        // .pin => |pin| switch (d.cursor_movement) {
        Location::Pin(pin) => match d.cursor_movement {
            // .none => {},
            command::CursorMovement::None => {}
            // .after => { ... },
            command::CursorMovement::After => {
                // We use terminal.index to properly handle scroll regions.
                // const screen = terminal.screens.active;
                // const size = p.gridSize(img, terminal);
                let size = p.grid_size(&img, terminal);
                // const target_x = @as(usize, pin.x) +| @as(usize, size.cols);
                // (R7: the `+|`s saturate by design for untrusted `size.cols`.)
                let pin_x = unsafe { (*pin).x };
                let target_x: usize = (pin_x as usize).saturating_add(size.cols as usize);
                // const wraps = target_x >= @as(usize, terminal.cols);
                let wraps = target_x >= terminal.cols as usize;
                // const requested_rows =
                //     (@as(usize, size.rows) -| 1) +| @intFromBool(wraps);
                let requested_rows: usize = (size.rows as usize)
                    .saturating_sub(1)
                    .saturating_add(usize::from(wraps));

                // The requested row count comes from the application and can
                // be as large as u32. Calling terminal.index once for every
                // row could therefore leave the terminal unresponsive for a
                // very long time.
                //
                // First allow enough calls to reach the bottom of the scroll
                // region. Each call after that scrolls the region by one row,
                // so limit those extra calls to one screen. This follows
                // Kitty's behavior while keeping the amount of work bounded.
                let region = terminal.scrolling_region;
                let (cursor_x, cursor_y) = unsafe {
                    (
                        (*terminal.screens.active).cursor.x,
                        (*terminal.screens.active).cursor.y,
                    )
                };
                let rows_before_scroll: usize = if cursor_y >= region.top
                    && cursor_y <= region.bottom
                    && cursor_x >= region.left
                    && cursor_x <= region.right
                {
                    (region.bottom - cursor_y) as usize
                } else {
                    0
                };
                let rows_to_move: usize = core::cmp::min(
                    requested_rows,
                    rows_before_scroll.saturating_add(terminal.rows as usize),
                );
                for _ in 0..rows_to_move {
                    if terminal.index().is_err() {
                        // log.warn("failed to move cursor: {}", .{err});
                        break;
                    }
                }

                // Kitty wraps once when the movement reaches the right edge;
                // otherwise it leaves the cursor immediately to the right of
                // the placement.
                // screen.cursor.pending_wrap = false;
                // screen.cursorHorizontalAbsolute(if (wraps) 0 else @intCast(target_x));
                let screen = unsafe { &mut *terminal.screens.active };
                screen.cursor.pending_wrap = false;
                screen.cursor_horizontal_absolute(if wraps {
                    0
                } else {
                    crate::size::CellCountInt::try_from(target_x).unwrap()
                });
            }
        },
    }

    result
}

/// Transmit animation frame data (a=f).
///
/// Frame data is loaded exactly like image data, including chunking
/// and every transmission medium, and once complete it is composed
/// into a frame of an existing image's animation.
fn transmit_animation_frame(alloc: Allocator, terminal: &mut Terminal, cmd: &Command) -> Response {
    // const storage = &terminal.screens.active.kitty_images;
    let storage = unsafe { &mut (*terminal.screens.active).kitty_images };

    // A chunk arriving while a load is in progress continues that load.
    if storage.loading.is_some() {
        // If this is the first frame, then we treat it like a normal image
        // because the first frame is just the image.
        if storage.loading.as_ref().unwrap().frame.is_none() {
            return transmit(alloc, terminal, cmd);
        }

        // Not the first frame, so continue loading similar to transmit
        // but this is for a subsequent frame.
        let loading = storage.loading.as_mut().unwrap();
        // var result = loading.response;
        let mut result = loading.response.clone();
        // loading.addData(alloc, cmd.data) catch |err| { ... };
        if let Err(err) = loading.addData(&cmd.data) {
            encode_error(&mut result, err);
            return result;
        }

        // If more chunks are expected we don't respond yet.
        let t = cmd.transmission().unwrap();
        if t.more_chunks {
            return Response::default();
        }

        // Final chunk: copy the loading state out and free the
        // pointer, mirroring loadAndAddImage.
        // var loading_copy = loading.*;
        // alloc.destroy(loading);
        // storage.loading = null;
        // defer loading_copy.deinit(alloc);
        let mut loading_copy = *storage.loading.take().unwrap();
        return complete_animation_frame(alloc, terminal, &mut loading_copy);
        // `loading_copy` drops here (the reference `defer deinit`).
    }

    // We are starting our load, either the first frame or a subsequent frame.
    // const f = cmd.control.transmit_animation_frame;
    let f = match &cmd.control {
        command::Control::TransmitAnimationFrame(f) => *f,
        // PORT NOTE: only invoked from the `.transmit_animation_frame` arms.
        _ => unreachable!(),
    };
    let t = f.transmission;
    let mut result: Response = Response {
        id: t.image_id,
        image_number: t.image_number,
        placement_id: t.placement_id,
        // Errors echo the client's frame number; on success this is
        // replaced with the resolved (possibly newly assigned) one.
        frame: f.edit_frame,
        ..Response::default()
    };

    // A frame can only be added to an existing image.
    if t.image_id == 0 && t.image_number == 0 {
        result.message = b"EINVAL: image ID or number required".to_vec();
        return result;
    }
    // const img = storage.imagePtrByIdOrNumber(t.image_id, t.image_number) orelse { ... };
    let (img_id, img_generation) =
        match storage.image_ptr_by_id_or_number(t.image_id, t.image_number) {
            Some(img) => (img.id, img.generation),
            None => {
                result.message = b"ENOENT: image not found".to_vec();
                return result;
            }
        };
    result.id = img_id;

    let mut loading = match LoadingImage::init(cmd, &storage.image_limits) {
        Ok(loading) => loading,
        Err(err) => {
            encode_error(&mut result, err);
            return result;
        }
    };
    loading.frame = Some(FrameContext {
        cmd: f,
        image_generation: img_generation,
    });
    loading.response.id = img_id;

    // Chunked: store the loading state and wait for the rest. The
    // frame parameters above are saved with it; continuation chunks
    // contribute only payload bytes.
    if t.more_chunks {
        // const loading_ptr = alloc.create(LoadingImage) catch |err| {
        //     loading.deinit(alloc);
        //     encodeError(&result, err);
        //     return result;
        // };
        // loading_ptr.* = loading;
        // storage.loading = loading_ptr;
        // PORT NOTE: `Box::new` aborts on OOM; the catch arm is unreachable.
        storage.loading = Some(Box::new(loading));
        return Response::default();
    }

    // defer loading.deinit(alloc); — `loading` drops at end of scope.
    complete_animation_frame(alloc, terminal, &mut loading)
}

/// Complete a fully loaded animation frame: validate it against its
/// image and compose it into the image's animation.
fn complete_animation_frame(
    alloc: Allocator,
    terminal: &mut Terminal,
    loading: &mut LoadingImage,
) -> Response {
    // const storage = &terminal.screens.active.kitty_images;
    //
    // PORT NOTE (hazard #3, phase-5 plan): the reference holds `img: *Image`
    // (a map-interior pointer) across `reserveAnimationBytes` — which can
    // EVICT — and re-resolves it afterwards. The port resolves the image's
    // concrete id once and re-looks it up (`images.get_mut`) around every
    // storage mutation instead; the id is stable and never evicted by
    // `reserve_animation_bytes` (it excludes this image), so every re-lookup
    // is the reference's re-resolve made mandatory.
    let storage = unsafe { &mut (*terminal.screens.active).kitty_images };
    let mut result = loading.response.clone();
    let f = loading.frame.unwrap().cmd;

    // Re-resolve the image: it may have been deleted, evicted, or
    // replaced while the frame data was being transmitted. The saved
    // generation pins the exact image the load started against.
    let (img_id, img_generation, img_width, img_height, img_data_complete) =
        match storage.image_ptr_by_id_or_number(loading.image.id, loading.image.number) {
            Some(img) => (
                img.id,
                img.generation,
                img.width,
                img.height,
                img.data.bytes().is_some(),
            ),
            None => {
                result.message = b"ENOENT: image not found".to_vec();
                return result;
            }
        };
    if img_generation != loading.frame.unwrap().image_generation {
        result.message = b"ENOENT: image not found".to_vec();
        return result;
    }
    if !img_data_complete {
        result.message = b"EINVAL: image data incomplete".to_vec();
        return result;
    }

    // Finish decoding the frame data: decompression, PNG decoding,
    // and length validation all match image loading.
    // var frame_img = loading.complete(alloc) catch |err| { ... };
    // defer frame_img.deinit(alloc);
    let mut frame_img = match loading.complete() {
        Ok(img) => img,
        Err(err) => {
            encode_error(&mut result, err);
            return result;
        }
    };

    // The frame rectangle may not exceed the image's size. The x/y
    // offsets are deliberately not validated: composition clips,
    // matching Kitty.
    if frame_img.width > img_width || frame_img.height > img_height {
        result.message = b"EINVAL: frame dimensions exceed image".to_vec();
        return result;
    }

    // All composition happens in RGBA; convert both sides as needed.
    if frame_img.format != command::Format::Rgba {
        // const rgba = pixel.rgbaFromFormat(alloc, frame_img.format, frame_img.data.bytes().?)
        //     catch |err| { encodeError(&result, err); return result; };
        // PORT NOTE: infallible in this port (see graphics_pixel.rs).
        let rgba = pixel::rgba_from_format(frame_img.format, frame_img.data.bytes().unwrap());
        // frame_img.data.deinit(alloc); — the assignment drops the old payload.
        frame_img.data = image::Data::Complete(rgba);
        frame_img.format = command::Format::Rgba;
    }
    // storage.convertImageToRgba(io, alloc, img) catch |err| { ... };
    // PORT NOTE: infallible in this port (see graphics_storage.rs).
    storage.convert_image_to_rgba(alloc, img_id);

    // const anim = ensureAnimation(alloc, img) catch |err| { ... };
    // (re-borrowed per use below, see the hazard-#3 note above.)

    // Resolve the frame number: r in 1..count edits that existing
    // frame, anything else (including omitted) creates a new frame
    // appended at count+1. The resolved number is echoed in the
    // response so clients can learn assigned frame numbers.
    let count: u32 = ensure_animation(storage.images.get_mut(&img_id).unwrap()).frame_count();
    let number: u32 = {
        let r = f.edit_frame;
        if r == 0 || r > count + 1 {
            count + 1
        } else {
            r
        }
    };
    result.frame = number;

    // const src = frame_img.data.bytes().?;
    if number == count + 1 {
        // Creating a new frame. The gap defaults to 40ms when omitted
        // and a negative gap creates a gapless (never shown) frame.
        let gap: u32 = if f.gap_ms > 0 {
            u32::try_from(f.gap_ms).unwrap() // @intCast
        } else if f.gap_ms < 0 {
            0
        } else {
            animation::DEFAULT_GAP_MS
        };

        // The base canvas frame must exist before we reserve space.
        if f.create_frame > 0
            && storage
                .images
                .get(&img_id)
                .unwrap()
                .frame_data(f.create_frame)
                .is_none()
        {
            result.message = b"EINVAL: base frame not found".to_vec();
            return result;
        }

        // Reserve room for the new frame, evicting other images if
        // needed. Eviction can remove images (never this one), so we
        // re-resolve our pointer afterwards to be safe.
        let frame_len: usize = img_width as usize * img_height as usize * 4;
        let image_id = img_id;
        if storage
            .reserve_animation_bytes(alloc, terminal.screens.active, image_id, frame_len)
            .is_err()
        {
            result.message = b"ENOSPC: animation frame storage full".to_vec();
            return result;
        }
        // img = storage.imagePtrByIdOrNumber(image_id, 0).?; — every access
        // below re-looks the image up by id (see the hazard-#3 note).

        // const canvas = alloc.alloc(u8, frame_len) catch {
        //     storage.releaseAnimationBytes(frame_len);
        //     result.message = "ENOMEM: out of memory";
        //     return result;
        // };
        // PORT NOTE: `vec!` aborts on OOM; the release/ENOMEM arm is
        // unreachable in this port.
        let mut canvas = vec![0u8; frame_len];
        if f.create_frame > 0 {
            // @memcpy(canvas, img.frameData(f.create_frame).?);
            canvas.copy_from_slice(
                storage
                    .images
                    .get(&img_id)
                    .unwrap()
                    .frame_data(f.create_frame)
                    .unwrap(),
            );
        } else {
            pixel::fill_background(&mut canvas, f.background);
        }
        pixel::compose_rect(
            &mut canvas,
            img_width,
            img_height,
            frame_img.data.bytes().unwrap(),
            frame_img.width,
            frame_img.height,
            f.x,
            f.y,
            f.composition_mode,
        );

        // anim.frames.append(alloc, .{ .data = canvas, .gap_ms = gap }) catch {
        //     alloc.free(canvas);
        //     storage.releaseAnimationBytes(frame_len);
        //     result.message = "ENOMEM: out of memory";
        //     return result;
        // };
        // PORT NOTE: `Vec::push` aborts on OOM; the catch arm is unreachable.
        {
            let img = storage.images.get_mut(&img_id).unwrap();
            ensure_animation(img).frames.push(animation::Frame {
                data: canvas,
                gap_ms: gap,
            });
        }

        // A new frame never changes the displayed frame (playback
        // reaches it later), but the storage content changed.
        storage.mark_mutated();
    } else {
        // Editing an existing frame. A nonzero gap also updates the
        // frame's gap; the 40ms default doesn't apply to edits.
        //
        // Unlike frame creation there is no byte reservation here:
        // frames are always stored at full image size, so the edit
        // composes into the existing buffer in place and storage
        // usage cannot change. Kitty likewise exempts frame edits
        // from its quota check.
        if f.gap_ms != 0 {
            let img = storage.images.get_mut(&img_id).unwrap();
            ensure_animation(img).set_gap_at(
                number - 1,
                if f.gap_ms > 0 {
                    u32::try_from(f.gap_ms).unwrap()
                } else {
                    0
                },
            );
        }

        // The frame data is owned by this storage, so the const cast
        // is safe (same reasoning as the renderer's image uploads).
        // const dst = @constCast(img.frameData(number).?);
        // PORT NOTE: the const-cast is expressed directly as the mutable
        // frame projection `frame_data_mut`.
        {
            let img = storage.images.get_mut(&img_id).unwrap();
            let dst = frame_data_mut(img, number).unwrap();
            pixel::compose_rect(
                dst,
                img_width,
                img_height,
                frame_img.data.bytes().unwrap(),
                frame_img.width,
                frame_img.height,
                f.x,
                f.y,
                f.composition_mode,
            );
        }

        let is_current = {
            let img = storage.images.get(&img_id).unwrap();
            number - 1 == img.animation.as_ref().unwrap().current_index
        };
        if is_current {
            // The displayed pixels changed; restart the frame's gap
            // timer like Kitty's re-upload does.
            {
                let img = storage.images.get_mut(&img_id).unwrap();
                img.animation.as_mut().unwrap().frame_shown_at_ms = None;
            }
            storage.mark_image_content_changed(img_id);
        } else {
            storage.mark_mutated();
        }
    }

    result
    // `frame_img` drops here (the reference `defer frame_img.deinit(alloc)`).
}

/// Control animation playback (a=a).
///
/// Successful commands never respond. The only possible responses are
/// for a missing image or identifier. Invalid key values are silently
/// ignored, matching Kitty.
fn control_animation(alloc: Allocator, terminal: &mut Terminal, cmd: &Command) -> Response {
    // const a = cmd.control.control_animation;
    let a = match &cmd.control {
        command::Control::ControlAnimation(a) => *a,
        // PORT NOTE: only invoked from the `.control_animation` arm.
        _ => unreachable!(),
    };
    // const storage = &terminal.screens.active.kitty_images;
    let storage = unsafe { &mut (*terminal.screens.active).kitty_images };
    let _ = &alloc;

    let mut result: Response = Response {
        id: a.image_id,
        image_number: a.image_number,
        placement_id: a.placement_id,
        ..Response::default()
    };
    if a.image_id == 0 && a.image_number == 0 {
        result.message = b"EINVAL: image ID or number required".to_vec();
        return result;
    }
    let img_id = match storage.image_ptr_by_id_or_number(a.image_id, a.image_number) {
        Some(img) => img.id,
        None => {
            result.message = b"ENOENT: image not found".to_vec();
            return result;
        }
    };

    // const anim = ensureAnimation(alloc, img) catch |err| { ... };
    // PORT NOTE: infallible in this port (Box aborts on OOM); the `anim`
    // pointer is re-borrowed per key below (hazard #3: the mark_* calls
    // between the keys need `&mut storage`).
    ensure_animation(storage.images.get_mut(&img_id).unwrap());

    // The keys below are applied independently and in the same order as Kitty.

    // Set a frame's gap (r= together with z=). This is the only way
    // to give the root frame a gap, since it is created gapless.
    let applied = {
        let anim = storage
            .images
            .get_mut(&img_id)
            .unwrap()
            .animation
            .as_deref_mut()
            .unwrap();
        if a.frame != 0 && a.frame <= anim.frame_count() && a.gap_ms != 0 {
            anim.set_gap_at(
                a.frame - 1,
                if a.gap_ms > 0 {
                    u32::try_from(a.gap_ms).unwrap()
                } else {
                    0
                },
            );
            true
        } else {
            false
        }
    };
    if applied {
        storage.mark_mutated();
    }

    // Set the current frame (c=), the client-driven animation
    // primitive.
    let applied = {
        let anim = storage
            .images
            .get_mut(&img_id)
            .unwrap()
            .animation
            .as_deref_mut()
            .unwrap();
        if a.current_frame != 0
            && a.current_frame <= anim.frame_count()
            && a.current_frame - 1 != anim.current_index
        {
            anim.current_index = a.current_frame - 1;
            anim.frame_shown_at_ms = None;
            true
        } else {
            false
        }
    };
    if applied {
        storage.mark_image_content_changed(img_id);
    }

    // Set the playback state (s=). Any state change resets the loop
    // counter; leaving the stopped state restarts the gap timer.
    if a.action != command::AnimationAction::Invalid {
        {
            let anim = storage
                .images
                .get_mut(&img_id)
                .unwrap()
                .animation
                .as_deref_mut()
                .unwrap();
            let old = anim.state;
            anim.state = match a.action {
                command::AnimationAction::Invalid => unreachable!(),
                command::AnimationAction::Stop => animation::State::Stopped,
                command::AnimationAction::RunWait => animation::State::Loading,
                command::AnimationAction::Run => animation::State::Running,
            };
            if old == animation::State::Stopped && anim.state != animation::State::Stopped {
                anim.frame_shown_at_ms = None;
            }
            anim.current_loop = 0;
        }
        storage.mark_mutated();
    }

    // Set the loop count (v=), stored off by one per the protocol:
    // v=1 loops forever, v=n plays n-1 loops.
    if a.loops != 0 {
        {
            let anim = storage
                .images
                .get_mut(&img_id)
                .unwrap()
                .animation
                .as_deref_mut()
                .unwrap();
            anim.max_loops = a.loops - 1;
        }
        storage.mark_mutated();
    }

    // Successful animation control commands never respond.
    Response::default()
}

/// Compose a rectangle of pixels from one animation frame onto
/// another (a=c). Frame 1 (the root frame) always exists, so this also works
/// on images without any animation state.
fn compose_animation(alloc: Allocator, terminal: &mut Terminal, cmd: &Command) -> Response {
    // const c = cmd.control.compose_animation;
    let c = match &cmd.control {
        command::Control::ComposeAnimation(c) => *c,
        // PORT NOTE: only invoked from the `.compose_animation` arm.
        _ => unreachable!(),
    };
    // const storage = &terminal.screens.active.kitty_images;
    let storage = unsafe { &mut (*terminal.screens.active).kitty_images };

    let mut result: Response = Response {
        id: c.image_id,
        image_number: c.image_number,
        placement_id: c.placement_id,
        ..Response::default()
    };
    if c.image_id == 0 && c.image_number == 0 {
        result.message = b"EINVAL: image ID or number required".to_vec();
        return result;
    }
    let (img_id, img_width, img_height, img_data_complete) =
        match storage.image_ptr_by_id_or_number(c.image_id, c.image_number) {
            Some(img) => (img.id, img.width, img.height, img.data.bytes().is_some()),
            None => {
                result.message = b"ENOENT: image not found".to_vec();
                return result;
            }
        };
    result.id = img_id;
    if !img_data_complete {
        result.message = b"EINVAL: image data incomplete".to_vec();
        return result;
    }

    // Both frames must exist. Note that r is the source frame and c
    // the destination; the spec's reference table describes these
    // backwards (see AnimationFrameComposition).
    {
        let img = storage.images.get(&img_id).unwrap();
        if img.frame_data(c.source_frame).is_none() {
            result.message = b"ENOENT: source frame not found".to_vec();
            return result;
        }
        if img.frame_data(c.dest_frame).is_none() {
            result.message = b"ENOENT: destination frame not found".to_vec();
            return result;
        }
    }

    // Rectangle validation is done in u64 so untrusted 32-bit values
    // can't overflow. Out-of-bounds rectangles are errors here,
    // unlike a=f which clips.
    let width: u64 = if c.width > 0 {
        c.width as u64
    } else {
        img_width as u64
    };
    let height: u64 = if c.height > 0 {
        c.height as u64
    } else {
        img_height as u64
    };
    if c.x as u64 + width > img_width as u64 || c.y as u64 + height > img_height as u64 {
        result.message = b"EINVAL: destination rectangle out of bounds".to_vec();
        return result;
    }
    if c.left_edge as u64 + width > img_width as u64
        || c.top_edge as u64 + height > img_height as u64
    {
        result.message = b"EINVAL: source rectangle out of bounds".to_vec();
        return result;
    }

    // Composing a frame onto itself requires non-overlapping
    // rectangles.
    if c.source_frame == c.dest_frame {
        // const x_overlaps = @max(c.left_edge, c.x) < @min(c.left_edge, c.x) + width;
        // const y_overlaps = @max(c.top_edge, c.y) < @min(c.top_edge, c.y) + height;
        let x_overlaps = (core::cmp::max(c.left_edge, c.x) as u64)
            < core::cmp::min(c.left_edge, c.x) as u64 + width;
        let y_overlaps =
            (core::cmp::max(c.top_edge, c.y) as u64) < core::cmp::min(c.top_edge, c.y) as u64 + height;
        if x_overlaps && y_overlaps {
            result.message = b"EINVAL: source and destination rectangles overlap".to_vec();
            return result;
        }
    }

    // All composition happens in RGBA.
    // storage.convertImageToRgba(io, alloc, img) catch |err| { ... };
    // PORT NOTE: infallible in this port (see graphics_storage.rs).
    storage.convert_image_to_rgba(alloc, img_id);

    // The frame data is owned by this storage, so the const cast is
    // safe. The rectangles were validated disjoint above, so in-place
    // composition within one frame is well-defined.
    // const src = img.frameData(c.source_frame).?;
    // const dst = @constCast(img.frameData(c.dest_frame).?);
    {
        let img = storage.images.get_mut(&img_id).unwrap();
        // @intCast(width) / @intCast(height)
        let w = u32::try_from(width).unwrap();
        let h = u32::try_from(height).unwrap();
        if c.source_frame == c.dest_frame {
            // Same frame: the reference aliases dst == src; the port's
            // in-place variant carries the same math (graphics_pixel.rs).
            let canvas = frame_data_mut(img, c.dest_frame).unwrap();
            pixel::compose_canvas_rect_in_place(
                canvas,
                img_width,
                w,
                h,
                c.left_edge,
                c.top_edge,
                c.x,
                c.y,
                c.composition_mode,
            );
        } else {
            let (src, dst) = frame_data_pair(img, c.source_frame, c.dest_frame).unwrap();
            pixel::compose_canvas_rect(
                dst,
                src,
                img_width,
                w,
                h,
                c.left_edge,
                c.top_edge,
                c.x,
                c.y,
                c.composition_mode,
            );
        }
    }

    // If the destination is the displayed frame then the on-screen
    // content changed.
    // const current: u32 = if (img.animation) |anim| anim.current_index else 0;
    let current: u32 = {
        let img = storage.images.get(&img_id).unwrap();
        img.animation.as_ref().map_or(0, |anim| anim.current_index)
    };
    if c.dest_frame - 1 == current {
        storage.mark_image_content_changed(img_id);
    } else {
        storage.mark_mutated();
    }

    result
}

/// Get or lazily create the animation state for an image.
//
// PORT NOTE: `Allocator.Error!*animation.Animation` — `alloc.create` maps to
// `Box::default` (aborts on OOM), so the port is infallible; the callers'
// `catch encodeError` arms are carried as comments at each call site.
fn ensure_animation(img: &mut Image) -> &mut animation::Animation {
    // if (img.animation) |anim| return anim;
    // const anim = try alloc.create(animation.Animation);
    // anim.* = .{};
    // img.animation = anim;
    // return anim;
    if img.animation.is_none() {
        img.animation = Some(Box::default());
    }
    img.animation.as_deref_mut().unwrap()
}

/// Mutable access to a frame's pixel buffer — the reference spells this
/// `@constCast(img.frameData(number).?)` ("the frame data is owned by this
/// storage, so the const cast is safe"); here the mutability is expressed
/// directly as a `&mut` projection mirroring `Image::frame_data`.
fn frame_data_mut(img: &mut Image, number: u32) -> Option<&mut [u8]> {
    match number {
        0 => None,
        1 => match &mut img.data {
            image::Data::Complete(data) => Some(data.as_mut_slice()),
            image::Data::Pending(_) => None,
        },
        _ => {
            let anim = img.animation.as_deref_mut()?;
            let idx = (number - 2) as usize;
            if idx >= anim.frames.len() {
                return None;
            }
            Some(&mut anim.frames[idx].data)
        }
    }
}

/// Split-borrow two DIFFERENT frames of one image: the source shared, the
/// destination mutable (the reference just aliases two pointers; the buffers
/// are distinct heap allocations, so the split borrows are always disjoint).
fn frame_data_pair(img: &mut Image, src_number: u32, dst_number: u32) -> Option<(&[u8], &mut [u8])> {
    debug_assert!(src_number != dst_number);
    if src_number == 0 || dst_number == 0 {
        return None;
    }
    if src_number == 1 {
        // Source is the base data; destination is an animation frame.
        let src: &[u8] = match &img.data {
            image::Data::Complete(data) => data.as_slice(),
            image::Data::Pending(_) => return None,
        };
        let anim = img.animation.as_deref_mut()?;
        let idx = (dst_number - 2) as usize;
        if idx >= anim.frames.len() {
            return None;
        }
        return Some((src, &mut anim.frames[idx].data));
    }
    if dst_number == 1 {
        // Destination is the base data; source is an animation frame.
        let anim = img.animation.as_deref()?;
        let idx = (src_number - 2) as usize;
        if idx >= anim.frames.len() {
            return None;
        }
        let src: &[u8] = &anim.frames[idx].data;
        let dst: &mut [u8] = match &mut img.data {
            image::Data::Complete(data) => data.as_mut_slice(),
            image::Data::Pending(_) => return None,
        };
        return Some((src, dst));
    }
    // Both are animation frames at distinct indices: split the Vec.
    let anim = img.animation.as_deref_mut()?;
    let si = (src_number - 2) as usize;
    let di = (dst_number - 2) as usize;
    if si >= anim.frames.len() || di >= anim.frames.len() {
        return None;
    }
    if si < di {
        let (head, tail) = anim.frames.split_at_mut(di);
        Some((&head[si].data, &mut tail[0].data))
    } else {
        let (head, tail) = anim.frames.split_at_mut(si);
        Some((&tail[0].data, &mut head[di].data))
    }
}

/// Display a previously transmitted image.
fn delete(alloc: Allocator, terminal: &mut Terminal, cmd: &Command) -> Response {
    // const storage = &terminal.screens.active.kitty_images;
    // PORT NOTE: `storage` is a field of the active screen and `delete` also
    // takes `terminal` mutably. The reference aliases `&...kitty_images` and
    // `terminal` simultaneously; in this port `ImageStorage::delete` is a free
    // method on the storage that re-derives the active screen via the terminal
    // it is handed, so we move the storage out, run delete, and move it back to
    // satisfy the borrow checker without changing behavior. The `kitty_images`
    // slot is restored before returning.
    let d = match &cmd.control {
        command::Control::Delete(d) => *d,
        // PORT NOTE: `delete` is only invoked from the `.delete` arm of `execute`.
        _ => unreachable!(),
    };
    let mut storage = core::mem::take(unsafe { &mut (*terminal.screens.active).kitty_images });

    // Every delete command aborts an incomplete chunked upload.
    // if (storage.loading) |loading| {
    //     loading.destroy(alloc);
    //     storage.loading = null;
    // }
    // PORT NOTE: dropping the boxed LoadingImage is `destroy`.
    if storage.loading.is_some() {
        storage.loading = None;
    }

    // Then perform the actual deletion request, too.
    storage.delete(alloc, terminal, d.action);
    unsafe {
        (*terminal.screens.active).kitty_images = storage;
    }

    // Delete never responds on success
    Response::default()
}

/// PORT NOTE: the reference returns an anonymous struct
/// `struct { image: Image, more: bool = false, display: ?command.Display = null }`
/// from `loadAndAddImage`. Rust has no anonymous structs, so we name it.
struct LoadAndAddImage {
    image: Image,
    more: bool,
    display: Option<command::Display>,
}

// PORT NOTE: the reference `loadAndAddImage` error set is the inferred union of
// `LoadingImage.init`/`addData`/`complete` (`Image.Error`) and `alloc.create`/
// `storage.addImage` (`Allocator.Error`). In this port `Image::Error` already
// carries `OutOfMemory`, and `addImage` returns the storage's `OutOfMemory`,
// which we fold into `Image::Error::OutOfMemory`. The combined fallible result
// is `Result<_, image::Error>` (== `EncodeableError`).
fn load_and_add_image(
    alloc: Allocator,
    terminal: &mut Terminal,
    cmd: &Command,
) -> Result<LoadAndAddImage, image::Error> {
    let t = cmd.transmission().unwrap();
    // const storage = &terminal.screens.active.kitty_images;
    let storage = unsafe { &mut (*terminal.screens.active).kitty_images };

    // Determine our image. This also handles chunking and early exit.
    // var loading: LoadingImage = if (storage.loading) |loading| loading: {
    let mut loading: LoadingImage = if storage.loading.is_some() {
        // Note: we do NOT want to call "cmd.toOwnedData" here because
        // we're _copying_ the data. We want the command data to be freed.
        // try loading.addData(alloc, cmd.data);
        let loading = storage.loading.as_mut().unwrap();
        loading.addData(&cmd.data)?;

        // If we have more then we're done
        // if (t.more_chunks) return .{ .image = loading.image, .more = true };
        if t.more_chunks {
            return Ok(LoadAndAddImage {
                image: loading.image.withoutData(),
                more: true,
                display: None,
            });
        }

        // We have no more chunks. We're going to be completing the
        // image so we want to destroy the pointer to the loading
        // image and copy it out.
        // defer {
        //     alloc.destroy(loading);
        //     storage.loading = null;
        // }
        //
        // break :loading loading.*;
        // PORT NOTE: the reference copies the loading image out of the heap
        // pointer then destroys the pointer (and nulls `storage.loading`).
        // `Option::take` moves the boxed `LoadingImage` out (nulling the slot)
        // and unboxing yields the owned value — the box is freed at scope end.
        *storage.loading.take().unwrap()
    } else {
        // Reusing a specific image ID deletes the old image and all of its
        // placements when the new transmission begins, not when it completes.
        if t.image_id > 0 {
            // storage.delete(io, alloc, terminal, .{ .id = .{
            //     .image_id = t.image_id,
            //     .delete = true,
            // } });
            // PORT NOTE: `delete` re-derives the active screen through the
            // terminal, so the storage is moved out and back around the call
            // (the same shape as `graphics_exec::delete` above).
            let mut moved =
                core::mem::take(unsafe { &mut (*terminal.screens.active).kitty_images });
            moved.delete(
                alloc,
                terminal,
                command::DeleteAction::Id {
                    image_id: t.image_id,
                    delete: true,
                    placement_id: 0,
                },
            );
            unsafe {
                (*terminal.screens.active).kitty_images = moved;
            }
        }

        // break :loading try .init(io, alloc, cmd, storage.image_limits);
        // SB: re-derive the storage reference after the delete above scoped
        // its own aliases.
        let storage = unsafe { &(*terminal.screens.active).kitty_images };
        LoadingImage::init(cmd, &storage.image_limits)?
    };

    // We only want to deinit on error. If we're chunking, then we don't
    // want to deinit at all. If we're not chunking, then we'll deinit
    // after we've copied the image out.
    // errdefer loading.deinit(alloc);
    // PORT NOTE: the `errdefer loading.deinit(alloc)` frees the loading state on
    // any error below. The owned `loading` value is dropped (freeing its data)
    // when an early `return Err(...)` unwinds out of this function, which is the
    // same effect; `mem::forget`-style hand-off only happens on the chunked
    // path below where the value is stored back into `storage.loading`.

    // SB: the `storage` reference taken at the top of this function was
    // invalidated by the retransmit-delete branch above (which moved the
    // storage out of the screen and back); re-derive it from the raw base
    // pointer so every use below has a live tag.
    let storage = unsafe { &mut (*terminal.screens.active).kitty_images };

    // If the image has no ID, we assign one
    if loading.image.id == 0 {
        // if (loading.image.number > 0) {
        //     loading.image.id = storage.nextImageId(.explicit);
        // } else {
        //     loading.image.id = storage.nextImageId(.implicit);
        //     loading.image.metadata.implicit_id = true;
        // }
        if loading.image.number > 0 {
            loading.image.id =
                storage.next_image_id_for(crate::kitty::graphics_storage::NextImageIdMode::Explicit);
        } else {
            loading.image.id =
                storage.next_image_id_for(crate::kitty::graphics_storage::NextImageIdMode::Implicit);
            loading.image.metadata.implicit_id = true;
        }
    }

    // If this is chunked, this is the beginning of a new chunked transmission.
    // (We checked for an in-progress chunk above.)
    if t.more_chunks {
        // We allocate the pointer on the heap because its rare and we
        // don't want to always pay the memory cost to keep it around.
        // const loading_ptr = try alloc.create(LoadingImage);
        // errdefer alloc.destroy(loading_ptr);
        // loading_ptr.* = loading;
        // storage.loading = loading_ptr;
        // return .{ .image = loading.image, .more = true };
        let image = loading.image.withoutData();
        storage.loading = Some(Box::new(loading));
        return Ok(LoadAndAddImage {
            image,
            more: true,
            display: None,
        });
    }

    // Dump the image data before it is decompressed
    // loading.debugDump() catch unreachable;

    // Validate and store our image
    // var img = try loading.complete(alloc);
    // errdefer img.deinit(alloc);
    // try storage.addImage(io, alloc, terminal.screens.active, img);
    // PORT NOTE: in the reference, `addImage` takes `img` by value (a struct copy
    // that shares the same `data` slice) and the caller keeps `img` to return
    // below — both the stored image and the returned `img` alias the same data.
    // Rust cannot share the owning `Vec<u8>`, so we store the full `img` (with
    // data, preserving `total_bytes` accounting) in the map and return a metadata
    // copy via `withoutData`. The only fields the caller reads from the returned
    // image are `id`/`implicit_id` (both preserved by `withoutData`); the data is
    // never read post-store, so this is observably identical.
    let img = loading.complete()?;
    let returned = img.withoutData();
    storage
        .add_image(alloc, terminal.screens.active, img)
        .map_err(|_| image::Error::OutOfMemory)?;

    // Get our display settings
    // const display_ = loading.display;
    let display_ = loading.display;

    // Ensure we deinit the loading state because we're done. The image
    // won't be deinit because of "complete" above.
    // loading.deinit(alloc);
    drop(loading);

    // return .{ .image = img, .display = display_ };
    Ok(LoadAndAddImage {
        image: returned,
        more: false,
        display: display_,
    })
}

// const EncodeableError = Image.Error || Allocator.Error;

/// Encode an error code into a message for a response.
// fn encodeError(r: *Response, err: EncodeableError) void {
fn encode_error(r: &mut Response, err: image::Error) {
    match err {
        // error.OutOfMemory => r.message = "ENOMEM: out of memory",
        image::Error::OutOfMemory => r.message = b"ENOMEM: out of memory".to_vec(),
        // error.InsufficientData => r.message = "ENODATA: insufficient data",
        image::Error::InsufficientData => r.message = b"ENODATA: insufficient data".to_vec(),
        // error.InvalidData => r.message = "EINVAL: invalid data",
        image::Error::InvalidData => r.message = b"EINVAL: invalid data".to_vec(),
        // error.DecompressionFailed => r.message = "EINVAL: decompression failed",
        image::Error::DecompressionFailed => r.message = b"EINVAL: decompression failed".to_vec(),
        // error.FilePathTooLong => r.message = "EINVAL: file path too long",
        image::Error::FilePathTooLong => r.message = b"EINVAL: file path too long".to_vec(),
        // error.TemporaryFileNotInTempDir => r.message = "EINVAL: temporary file not in temp dir",
        image::Error::TemporaryFileNotInTempDir => {
            r.message = b"EINVAL: temporary file not in temp dir".to_vec()
        }
        // error.TemporaryFileNotNamedCorrectly => r.message = "EINVAL: temporary file not named correctly",
        image::Error::TemporaryFileNotNamedCorrectly => {
            r.message = b"EINVAL: temporary file not named correctly".to_vec()
        }
        // error.UnsupportedFormat => r.message = "EINVAL: unsupported format",
        image::Error::UnsupportedFormat => r.message = b"EINVAL: unsupported format".to_vec(),
        // error.UnsupportedMedium => r.message = "EINVAL: unsupported medium",
        image::Error::UnsupportedMedium => r.message = b"EINVAL: unsupported medium".to_vec(),
        // error.UnsupportedDepth => r.message = "EINVAL: unsupported pixel depth",
        image::Error::UnsupportedDepth => r.message = b"EINVAL: unsupported pixel depth".to_vec(),
        // error.DimensionsRequired => r.message = "EINVAL: dimensions required",
        image::Error::DimensionsRequired => r.message = b"EINVAL: dimensions required".to_vec(),
        // error.DimensionsTooLarge => r.message = "EINVAL: dimensions too large",
        image::Error::DimensionsTooLarge => r.message = b"EINVAL: dimensions too large".to_vec(),
    }
}

/// PORT NOTE: the storage's `add_placement` returns the storage-local
/// `OutOfMemory` type (the reference's `Allocator.Error`); fold it into the
/// `EncodeableError` mapping (only `OutOfMemory` is reachable here).
fn encode_error_storage(r: &mut Response, _err: crate::kitty::graphics_storage::OutOfMemory) {
    encode_error(r, image::Error::OutOfMemory);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kitty::graphics_command as command;
    use crate::kitty::graphics_image::{Data, Limits};
    use crate::size;

    // PORT NOTE: the tests construct `Terminal.init(alloc, .{ .rows, .cols })`
    // relying on the reference `Terminal.Options` field defaults. Rust `Options`
    // has no field defaults, so this helper supplies them verbatim, mirroring the
    // reference `Options` declaration (see terminal.rs / graphics_storage.rs).
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

    // test "kittygfx more chunks with q=1"
    #[test]
    fn kittygfx_more_chunks_with_q_1() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Initial chunk has q=1
        {
            let cmd =
                command::Parser::parse_string(b"a=T,f=24,t=d,i=1,s=1,v=2,c=10,r=1,m=1,q=1;////")
                    .unwrap();
            let resp = execute(alloc, &mut t, &cmd);
            assert!(resp.is_none());
        }

        // Subsequent chunk has no q but should respect initial
        {
            let cmd = command::Parser::parse_string(b"m=0;////").unwrap();
            let resp = execute(alloc, &mut t, &cmd);
            assert!(resp.is_none());
        }

        t.deinit(alloc);
    }

    // test "kittygfx more chunks with q=0"
    #[test]
    fn kittygfx_more_chunks_with_q_0() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Initial chunk has q=0
        {
            let cmd =
                command::Parser::parse_string(b"a=t,f=24,t=d,s=1,v=2,c=10,r=1,m=1,i=1,q=0;////")
                    .unwrap();
            let resp = execute(alloc, &mut t, &cmd);
            assert!(resp.is_none());
        }

        // Subsequent chunk has no q so should respond OK
        {
            let cmd = command::Parser::parse_string(b"m=0;////").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }

        t.deinit(alloc);
    }

    // test "kittygfx more chunks with chunk increasing q"
    #[test]
    fn kittygfx_more_chunks_with_chunk_increasing_q() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Initial chunk has q=0
        {
            let cmd =
                command::Parser::parse_string(b"a=t,f=24,t=d,s=1,v=2,c=10,r=1,m=1,i=1,q=0;////")
                    .unwrap();
            let resp = execute(alloc, &mut t, &cmd);
            assert!(resp.is_none());
        }

        // Subsequent chunk sets q=1 so should not respond
        {
            let cmd = command::Parser::parse_string(b"m=0,q=1;////").unwrap();
            let resp = execute(alloc, &mut t, &cmd);
            assert!(resp.is_none());
        }

        t.deinit(alloc);
    }

    // test "kittygfx default format is rgba"
    #[test]
    fn kittygfx_default_format_is_rgba() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        let cmd =
            command::Parser::parse_string(b"a=t,t=d,i=1,s=1,v=2,c=10,r=1;///////////").unwrap();
        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(resp.ok());

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let img = storage.image_by_id(1).unwrap();
        assert_eq!(command::Format::Rgba, img.format);

        t.deinit(alloc);
    }

    // test "kittygfx test valid u32 (expect invalid image ID)"
    #[test]
    fn kittygfx_test_valid_u32_expect_invalid_image_id() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        let cmd = command::Parser::parse_string(b"a=p,i=4294967295").unwrap();
        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(!resp.ok());
        assert_eq!(resp.message, b"ENOENT: image not found");

        t.deinit(alloc);
    }

    // test "kittygfx test valid i32 (expect invalid image ID)"
    #[test]
    fn kittygfx_test_valid_i32_expect_invalid_image_id() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        let cmd = command::Parser::parse_string(b"a=p,i=1,z=-2147483648").unwrap();
        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(!resp.ok());
        assert_eq!(resp.message, b"ENOENT: image not found");

        t.deinit(alloc);
    }

    // test "kittygfx no response with no image ID or number"
    #[test]
    fn kittygfx_no_response_with_no_image_id_or_number() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        {
            let cmd =
                command::Parser::parse_string(b"a=t,f=24,t=d,s=1,v=2,c=10,r=1,i=0,I=0;////////")
                    .unwrap();
            let resp = execute(alloc, &mut t, &cmd);
            assert!(resp.is_none());
        }

        t.deinit(alloc);
    }

    // test "kittygfx no response with no image ID or number load and display"
    #[test]
    fn kittygfx_no_response_with_no_image_id_or_number_load_and_display() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        {
            let cmd =
                command::Parser::parse_string(b"a=T,f=24,t=d,s=1,v=2,c=10,r=1,i=0,I=0;////////")
                    .unwrap();
            let resp = execute(alloc, &mut t, &cmd);
            assert!(resp.is_none());
        }

        t.deinit(alloc);
    }

    // test "kittygfx retransmit same id gets fresh image generation"
    #[test]
    fn kittygfx_retransmit_same_id_gets_fresh_image_generation() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Transmit a 1x2 RGB image with id=1.
        {
            let cmd = command::Parser::parse_string(b"a=t,t=d,f=24,i=1,s=1,v=2;////////").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let gen1 = storage.image_by_id(1).unwrap().generation;
        assert!(gen1 > 0);
        assert_eq!(gen1, storage.generation);

        // Retransmit the same id with identical dimensions/length. The
        // (width, height, format, len) tuple is identical, so only the
        // generation can reveal that the contents were replaced.
        {
            let cmd = command::Parser::parse_string(b"a=t,t=d,f=24,i=1,s=1,v=2;AAAAAAAA").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let gen2 = storage.image_by_id(1).unwrap().generation;
        assert!(gen2 > gen1);
        assert_eq!(gen2, storage.generation);

        t.deinit(alloc);
    }

    // test "kittygfx retransmit same id removes existing placements"
    #[test]
    fn kittygfx_retransmit_same_id_removes_existing_placements() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        // Transmit and display an image, then add anonymous and named placements.
        // Multiple anonymous a=p placements for one image are explicitly valid.
        {
            let cmd =
                command::Parser::parse_string(b"a=T,t=d,f=24,i=1,s=1,v=2,C=1;////////").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,C=1").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,p=7,C=1").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            assert_eq!(3usize, storage.placements.len());
        }
        assert_eq!(
            tracked + 3,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        // Retransmitting replaces the image and must delete every old placement.
        // Plain a=t creates no replacement placement of its own.
        {
            let cmd = command::Parser::parse_string(b"a=t,t=d,f=24,i=1,s=1,v=2;AAAAAAAA").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            assert_eq!(0usize, storage.placements.len());
        }
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        // a=T creates one new placement after deleting the previous image and
        // placements, so repeated redraws remain bounded at one placement.
        {
            let cmd =
                command::Parser::parse_string(b"a=T,t=d,f=24,i=1,s=1,v=2,C=1;////////").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        {
            let cmd =
                command::Parser::parse_string(b"a=T,t=d,f=24,i=1,s=1,v=2,C=1;AAAAAAAA").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            assert_eq!(1usize, storage.placements.len());
        }
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        t.deinit(alloc);
    }

    // test "kittygfx delete then retransmit same id gets fresh generation"
    #[test]
    fn kittygfx_delete_then_retransmit_same_id_gets_fresh_generation() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Transmit and display, then delete everything (including image
        // data), then retransmit the same ID.
        {
            let cmd = command::Parser::parse_string(b"a=T,t=d,f=24,i=1,s=1,v=2,c=1,r=1;////////")
                .unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        let gen1 = unsafe { &(*t.screens.active).kitty_images }
            .image_by_id(1)
            .unwrap()
            .generation;

        {
            let cmd = command::Parser::parse_string(b"a=d,d=A").unwrap();
            let resp = execute(alloc, &mut t, &cmd);
            assert!(resp.is_none());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            assert!(storage.image_by_id(1).is_none());
        }
        let gen_delete = unsafe { &(*t.screens.active).kitty_images }.generation;
        assert!(gen_delete > gen1);

        {
            let cmd = command::Parser::parse_string(b"a=t,t=d,f=24,i=1,s=1,v=2;////////").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        let gen2 = unsafe { &(*t.screens.active).kitty_images }
            .image_by_id(1)
            .unwrap()
            .generation;
        assert!(gen2 > gen1);
        assert!(gen2 > gen_delete);

        t.deinit(alloc);
    }

    // test "kittygfx placement bounds cursor movement for untrusted dimensions"
    #[test]
    fn kittygfx_placement_bounds_cursor_movement_for_untrusted_dimensions() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        let cmd = command::Parser::parse_string(
            b"a=T,t=d,f=24,i=1,s=1,v=1,c=4294967295,r=4294967295;////",
        )
        .unwrap();

        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(resp.ok());
        assert_eq!(
            1usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );
        // Reaching the bottom takes four rows, then scrolling is capped to one
        // screen (five rows), matching Kitty's bounded screen-scroll behavior.
        assert_eq!(
            10usize,
            unsafe { &mut (*t.screens.active).pages }.scrollbar().total
        );

        t.deinit(alloc);
    }
    // test "kittygfx query validates image data"
    #[test]
    fn kittygfx_query_validates_image_data() {
        let alloc = allocator_api2::alloc::Global;

        let mut terminal = Terminal::init(alloc, opts(5, 5)).unwrap();

        let cmd: Command = Command {
            control: command::Control::Query(command::Transmission {
                format: command::Format::Rgb,
                width: 1,
                height: 1,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            // A 1x1 RGB image requires three bytes.
            data: vec![0, 0],
        };

        let resp = execute(alloc, &mut terminal, &cmd).unwrap();
        assert!(!resp.ok());
        assert_eq!(31u32, resp.id);
        assert_eq!(b"EINVAL: invalid data".as_slice(), resp.message.as_slice());
        assert_eq!(
            0usize,
            unsafe { &(*terminal.screens.active).kitty_images }
                .images
                .len()
        );

        terminal.deinit(alloc);
    }

    // test "kittygfx valid query does not replace or store image"
    #[test]
    fn kittygfx_valid_query_does_not_replace_or_store_image() {
        let alloc = allocator_api2::alloc::Global;

        let mut terminal = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Store a red pixel under the same ID used by the query.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=31;/wAA").unwrap();
            assert!(execute(alloc, &mut terminal, &cmd).unwrap().ok());
        }

        // Successfully validate a black pixel without replacing the red one.
        let cmd: Command = Command {
            control: command::Control::Query(command::Transmission {
                format: command::Format::Rgb,
                width: 1,
                height: 1,
                image_id: 31,
                ..Default::default()
            }),
            quiet: command::Quiet::No,
            data: vec![0, 0, 0],
        };

        let resp = execute(alloc, &mut terminal, &cmd).unwrap();
        assert!(resp.ok());
        {
            let storage = unsafe { &(*terminal.screens.active).kitty_images };
            assert_eq!(1usize, storage.images.len());
            let img = storage.image_by_id(31).unwrap();
            let Data::Complete(bytes) = &img.data else {
                unreachable!()
            };
            assert_eq!([255u8, 0, 0].as_slice(), bytes.as_slice());
        }

        terminal.deinit(alloc);
    }

    // test "kittygfx image id and number are mutually exclusive for every action"
    #[test]
    fn kittygfx_image_id_and_number_are_mutually_exclusive_for_every_action() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        let inputs: [&[u8]; 8] = [
            b"a=q,f=24,s=1,v=1,i=1,I=2,p=3;AAAA",
            b"a=t,f=24,s=1,v=1,i=1,I=2,p=3;AAAA",
            b"a=T,f=24,s=1,v=1,i=1,I=2,p=3;AAAA",
            b"a=p,i=1,I=2,p=3",
            b"a=d,d=a,i=1,I=2,p=3",
            b"a=f,i=1,I=2,p=3;AAAA",
            b"a=a,i=1,I=2,p=3",
            b"a=c,i=1,I=2,p=3",
        ];

        for input in inputs {
            let cmd = command::Parser::parse_string(input).unwrap();

            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(1u32, resp.id);
            assert_eq!(2u32, resp.image_number);
            assert_eq!(3u32, resp.placement_id);
            assert_eq!(
                b"EINVAL: image ID and number are mutually exclusive".as_slice(),
                resp.message.as_slice()
            );

            let mut buf: Vec<u8> = Vec::new();
            resp.encode(&mut buf).unwrap();
            assert_eq!(
                b"\x1b_Gi=1,I=2,p=3;EINVAL: image ID and number are mutually exclusive\x1b\\"
                    .as_slice(),
                buf.as_slice()
            );
        }

        assert_eq!(
            0usize,
            unsafe { &(*t.screens.active).kitty_images }.images.len()
        );
        assert_eq!(
            0usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );

        t.deinit(alloc);
    }

    // test "kittygfx conflicting identifiers are rejected before mutation"
    #[test]
    fn kittygfx_conflicting_identifiers_are_rejected_before_mutation() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Store an image so a buggy put would succeed and mutate placement state.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Directly constructed commands must receive the same validation as
        // commands produced by the parser.
        {
            let cmd: Command = Command {
                control: command::Control::Display(command::Display {
                    image_id: 1,
                    image_number: 2,
                    placement_id: 7,
                    cursor_movement: command::CursorMovement::None,
                    ..Default::default()
                }),
                quiet: command::Quiet::No,
                data: Vec::new(),
            };
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(1u32, resp.id);
            assert_eq!(2u32, resp.image_number);
            assert_eq!(7u32, resp.placement_id);
            assert_eq!(
                0usize,
                unsafe { &(*t.screens.active).kitty_images }
                    .placements
                    .len()
            );
        }

        // Add a real placement so a buggy delete would remove it.
        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,p=7,C=1").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
            assert_eq!(
                1usize,
                unsafe { &(*t.screens.active).kitty_images }
                    .placements
                    .len()
            );
        }

        {
            let cmd = command::Parser::parse_string(b"a=d,d=i,i=1,I=2,p=7").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(
                1usize,
                unsafe { &(*t.screens.active).kitty_images }
                    .placements
                    .len()
            );
        }

        // q=2 suppresses the required error response but not the validation.
        {
            let cmd = command::Parser::parse_string(b"a=d,d=i,i=1,I=2,p=7,q=2").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
            assert_eq!(
                1usize,
                unsafe { &(*t.screens.active).kitty_images }
                    .placements
                    .len()
            );
        }

        t.deinit(alloc);
    }

    // test "kittygfx chunked success response uses initial identifiers"
    #[test]
    fn kittygfx_chunked_success_response_uses_initial_identifiers() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=2,I=93,p=7,m=1;AAAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }

        {
            let cmd = command::Parser::parse_string(b"m=0;AAAA").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();

            assert!(resp.ok());
            assert_eq!(1u32, resp.id);
            assert_eq!(93u32, resp.image_number);
            assert_eq!(7u32, resp.placement_id);

            let mut buf: Vec<u8> = Vec::new();
            resp.encode(&mut buf).unwrap();
            assert_eq!(b"\x1b_Gi=1,I=93,p=7;OK\x1b\\".as_slice(), buf.as_slice());
        }

        t.deinit(alloc);
    }

    // test "kittygfx chunked error response uses initial identifiers"
    #[test]
    fn kittygfx_chunked_error_response_uses_initial_identifiers() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=41,p=7,m=1;AA==").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }

        {
            let cmd = command::Parser::parse_string(b"m=0;AA==").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();

            assert!(!resp.ok());
            assert_eq!(41u32, resp.id);
            assert_eq!(7u32, resp.placement_id);
            assert_eq!(b"EINVAL: invalid data".as_slice(), resp.message.as_slice());
        }

        t.deinit(alloc);
    }

    // test "kittygfx delete aborts chunked image load"
    #[test]
    fn kittygfx_delete_aborts_chunked_image_load() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Begin a two-chunk RGB image, then interrupt it with a delete.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=2,i=1,m=1;AAAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        assert!(unsafe { &(*t.screens.active).kitty_images }.loading.is_some());

        {
            let cmd = command::Parser::parse_string(b"a=d").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        assert!(unsafe { &(*t.screens.active).kitty_images }.loading.is_none());

        // A fresh chunked upload must start from empty state after the delete.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=2,i=1,m=1;AAAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        {
            let cmd = command::Parser::parse_string(b"m=0;AAAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        assert_eq!(
            6usize,
            unsafe { &(*t.screens.active).kitty_images }
                .image_by_id(1)
                .unwrap()
                .data
                .len()
        );

        t.deinit(alloc);
    }

    // test "kittygfx uppercase id delete preserves image when placement does not match"
    #[test]
    fn kittygfx_uppercase_id_delete_preserves_image_when_placement_does_not_match() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Store an unplaced 1x1 RGB image.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;AAAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Uppercase deletion may free data only if the named placement matched.
        {
            let cmd = command::Parser::parse_string(b"a=d,d=I,i=1,p=7").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }

        assert!(unsafe { &(*t.screens.active).kitty_images }
            .image_by_id(1)
            .is_some());

        t.deinit(alloc);
    }

    // test "kittygfx retransmit same id removes image on first chunk"
    #[test]
    fn kittygfx_retransmit_same_id_removes_image_on_first_chunk() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        let tracked = unsafe { &(*t.screens.active).pages }.count_tracked_pins();

        // Store and display the old image.
        {
            let cmd =
                command::Parser::parse_string(b"a=T,t=d,f=24,i=1,s=1,v=2,C=1;////////").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        assert!(unsafe { &(*t.screens.active).kitty_images }
            .image_by_id(1)
            .is_some());
        assert_eq!(
            1usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );
        assert_eq!(
            tracked + 1,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        // Starting a replacement for the same explicit ID removes the old image
        // and placements before the replacement's final chunk arrives.
        {
            let cmd = command::Parser::parse_string(b"a=t,t=d,f=24,i=1,s=1,v=2,m=1;AAAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        assert!(unsafe { &(*t.screens.active).kitty_images }.loading.is_some());
        assert!(unsafe { &(*t.screens.active).kitty_images }
            .image_by_id(1)
            .is_none());
        assert_eq!(
            0usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );
        assert_eq!(
            tracked,
            unsafe { &(*t.screens.active).pages }.count_tracked_pins()
        );

        t.deinit(alloc);
    }

    // test "kittygfx display clamps cell offsets"
    #[test]
    fn kittygfx_display_clamps_cell_offsets() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        t.width_px = 50; // 10 px per col
        t.height_px = 100; // 20 px per row

        let cmd =
            command::Parser::parse_string(b"a=T,t=d,f=24,i=1,s=1,v=1,c=2,r=1,X=99,Y=99,C=1;AAAA")
                .unwrap();

        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(resp.ok());

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let placement = storage.placements.values().next().unwrap();
        assert_eq!(9u32, placement.x_offset);
        assert_eq!(19u32, placement.y_offset);

        let actual = placement.pixel_size(&storage.image_by_id(1).unwrap(), &t);
        assert_eq!(11u32, actual.width);
        assert_eq!(1u32, actual.height);

        t.deinit(alloc);
    }

    // test "kittygfx placement moves cursor past a tall image"
    #[test]
    fn kittygfx_placement_moves_cursor_past_a_tall_image() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Load a one-pixel RGB image, then place it over the full width and eight
        // rows. Eight rows are taller than the screen but still within Kitty's
        // scroll budget.
        {
            let cmd = command::Parser::parse_string(b"a=t,t=d,f=24,i=1,s=1,v=1;////").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,p=1,c=5,r=8").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }

        // A subsequent placement begins immediately after the first one instead
        // of overlapping it at the old one-screen movement cap.
        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,p=2,C=1").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let first = *storage
            .placements
            .get(&crate::kitty::graphics_storage::PlacementKey {
                image_id: 1,
                placement_id: crate::kitty::graphics_storage::PlacementId {
                    tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                    id: 1,
                },
            })
            .unwrap();
        let second = *storage
            .placements
            .get(&crate::kitty::graphics_storage::PlacementKey {
                image_id: 1,
                placement_id: crate::kitty::graphics_storage::PlacementId {
                    tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                    id: 2,
                },
            })
            .unwrap();
        let first_pin = match first.location {
            Location::Pin(pin) => pin,
            Location::Virtual | Location::Relative(_) => unreachable!(),
        };
        let second_pin = match second.location {
            Location::Pin(pin) => pin,
            Location::Virtual | Location::Relative(_) => unreachable!(),
        };
        let first_y = unsafe { &(*t.screens.active).pages }
            .point_from_pin(crate::point::Tag::Screen, unsafe { *first_pin })
            .unwrap()
            .coord()
            .y;
        let second_y = unsafe { &(*t.screens.active).pages }
            .point_from_pin(crate::point::Tag::Screen, unsafe { *second_pin })
            .unwrap()
            .coord()
            .y;
        assert_eq!(first_y + 8, second_y);

        t.deinit(alloc);
    }

    // test "kittygfx unknown format responds with EINVAL"
    #[test]
    fn kittygfx_unknown_format_responds_with_einval() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        let cmd = command::Parser::parse_string(b"a=t,f=42,t=d,i=31,s=1,v=1;AAAA").unwrap();

        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(!resp.ok());
        assert_eq!(31u32, resp.id);
        assert_eq!(
            b"EINVAL: unsupported format".as_slice(),
            resp.message.as_slice()
        );
        assert_eq!(
            0usize,
            unsafe { &(*t.screens.active).kitty_images }.images.len()
        );

        t.deinit(alloc);
    }

    // test "kittygfx unknown format on query responds with EINVAL"
    #[test]
    fn kittygfx_unknown_format_on_query_responds_with_einval() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        let cmd = command::Parser::parse_string(b"a=q,f=42,t=d,i=31,s=1,v=1;AAAA").unwrap();

        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(!resp.ok());
        assert_eq!(31u32, resp.id);
        assert_eq!(
            b"EINVAL: unsupported format".as_slice(),
            resp.message.as_slice()
        );

        t.deinit(alloc);
    }

    // test "kittygfx zero format is rgba"
    #[test]
    fn kittygfx_zero_format_is_rgba() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Kitty treats f=0 as an absent f, i.e. RGBA.
        let cmd =
            command::Parser::parse_string(b"a=t,f=0,t=d,i=1,s=1,v=2,c=10,r=1;///////////").unwrap();

        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(resp.ok());

        let img = unsafe { &(*t.screens.active).kitty_images }
            .image_by_id(1)
            .unwrap();
        assert_eq!(command::Format::Rgba, img.format);

        t.deinit(alloc);
    }

    // test "kittygfx unknown format with q=3 has no response"
    #[test]
    fn kittygfx_unknown_format_with_q_3_has_no_response() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // q above 2 is out of range in the spec but Kitty suppresses
        // everything for anything above 1, so we must still parse it.
        let cmd = command::Parser::parse_string(b"a=t,f=42,t=d,i=31,q=3,s=1,v=1;AAAA").unwrap();

        assert!(execute(alloc, &mut t, &cmd).is_none());

        t.deinit(alloc);
    }

    // test "kittygfx out of range display keys are tolerated"
    #[test]
    fn kittygfx_out_of_range_display_keys_are_tolerated() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        {
            let cmd =
                command::Parser::parse_string(b"a=T,f=24,t=d,i=31,s=1,v=1,C=2,U=2;AAAA").unwrap();

            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
            assert_eq!(31u32, resp.id);
        }

        // U=2 must behave like U=1, so the placement is virtual.
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let entry = storage.placements.values().next().unwrap();
        assert!(matches!(entry.location, Location::Virtual));

        t.deinit(alloc);
    }

    // test "kittygfx number-based transmission assigns smallest free id"
    #[test]
    fn kittygfx_number_based_transmission_assigns_smallest_free_id() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Empty storage: the first free ID is 1, as in Kitty.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,I=42;/wAA").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
            assert_eq!(1u32, resp.id);
            assert_eq!(42u32, resp.image_number);
        }

        // Occupy ID 2 explicitly; the next number gets 3.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=2;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,I=43;/wAA").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
            assert_eq!(3u32, resp.id);
        }

        // Deleting ID 1 opens a gap that the next number fills.
        {
            let cmd = command::Parser::parse_string(b"a=d,d=I,i=1").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,I=44;/wAA").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
            assert_eq!(1u32, resp.id);
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        assert_eq!(3usize, storage.images.len());
        assert_eq!(44u32, storage.image_by_id(1).unwrap().number);
        assert_eq!(0u32, storage.image_by_id(2).unwrap().number);
        assert_eq!(43u32, storage.image_by_id(3).unwrap().number);

        t.deinit(alloc);
    }

    // test "kittygfx number-based id assignment does not replace client image"
    #[test]
    fn kittygfx_number_based_id_assignment_does_not_replace_client_image() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // A client stores and places a red pixel on the ID the old wrapping
        // counter would assign first.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=2147483647;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=p,i=2147483647,C=1").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // A number-based transmission must not collide with it.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,I=42;AAD/").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
            assert_eq!(1u32, resp.id);
        }

        // The client's image and placement are untouched.
        let storage = unsafe { &(*t.screens.active).kitty_images };
        assert_eq!(2usize, storage.images.len());
        assert_eq!(1usize, storage.placements.len());
        {
            let img = storage.image_by_id(2147483647).unwrap();
            let Data::Complete(bytes) = &img.data else {
                unreachable!()
            };
            assert_eq!([255u8, 0, 0].as_slice(), bytes.as_slice());
        }

        t.deinit(alloc);
    }

    // test "kittygfx implicit id assignment does not replace client image"
    #[test]
    fn kittygfx_implicit_id_assignment_does_not_replace_client_image() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // A client stores and places a red pixel on the ID the implicit
        // counter assigns first.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=2147483647;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=p,i=2147483647,C=1").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Transmit without an ID or number: no response, and the counter
        // skips over the in-use ID instead of replacing that image.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1;AAD/").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        assert_eq!(2usize, storage.images.len());
        assert_eq!(1usize, storage.placements.len());
        {
            let img = storage.image_by_id(2147483647).unwrap();
            let Data::Complete(bytes) = &img.data else {
                unreachable!()
            };
            assert_eq!([255u8, 0, 0].as_slice(), bytes.as_slice());
        }
        let implicit = storage.image_by_id(2147483648).unwrap();
        assert!(implicit.metadata.implicit_id);
        {
            let Data::Complete(bytes) = &implicit.data else {
                unreachable!()
            };
            assert_eq!([0u8, 0, 255].as_slice(), bytes.as_slice());
        }

        t.deinit(alloc);
    }

    // test "kittygfx relative placement with missing parent image"
    #[test]
    fn kittygfx_relative_placement_with_missing_parent_image() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;AAAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // The parent image does not exist: the placement must be rejected
        // with ENOPARENT, nothing may be created, and the cursor must not
        // move (a relative placement never moves the cursor, and a failed
        // one certainly must not).
        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,p=1,P=42,Q=1,H=2,V=2").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(
                b"ENOPARENT: parent image not found".as_slice(),
                resp.message.as_slice()
            );
        }

        assert_eq!(
            0usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );
        assert_eq!(0, unsafe { (*t.screens.active).cursor.x });
        assert_eq!(0, unsafe { (*t.screens.active).cursor.y });

        t.deinit(alloc);
    }

    // test "kittygfx relative placement with missing parent placement"
    #[test]
    fn kittygfx_relative_placement_with_missing_parent_placement() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=t,f=24,s=1,v=1,i=2;AAAA".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // The parent image exists but has no placements at all.
        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,P=2").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(
                b"ENOPARENT: parent placement not found".as_slice(),
                resp.message.as_slice()
            );
        }

        // The parent image has a placement, but not the requested one.
        {
            let cmd = command::Parser::parse_string(b"a=p,i=2,p=1,C=1").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,P=2,Q=9").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(
                b"ENOPARENT: parent placement not found".as_slice(),
                resp.message.as_slice()
            );
        }

        assert_eq!(
            1usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );

        t.deinit(alloc);
    }

    // test "kittygfx relative placement cannot parent itself"
    #[test]
    fn kittygfx_relative_placement_cannot_parent_itself() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=p,i=1,p=1,C=1".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Explicitly via Q, and implicitly when the Q=0 fallback selects
        // the placement being replaced.
        for input in [
            b"a=p,i=1,p=1,P=1,Q=1".as_slice(),
            b"a=p,i=1,p=1,P=1".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(
                b"EINVAL: placement cannot be its own parent".as_slice(),
                resp.message.as_slice()
            );
        }

        // The original placement must be untouched.
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let p = storage
            .placements
            .get(&crate::kitty::graphics_storage::PlacementKey {
                image_id: 1,
                placement_id: crate::kitty::graphics_storage::PlacementId {
                    tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                    id: 1,
                },
            })
            .unwrap();
        assert!(matches!(p.location, Location::Pin(_)));

        t.deinit(alloc);
    }

    // test "kittygfx relative placement cycle"
    #[test]
    fn kittygfx_relative_placement_cycle() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=p,i=1,p=1,C=1".as_slice(),
            b"a=p,i=1,p=2,P=1,Q=1".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Replacing placement 1 with a parent of placement 2 would create
        // the cycle 1 -> 2 -> 1.
        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,p=1,P=1,Q=2").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(
                b"ECYCLE: parent chain creates a cycle".as_slice(),
                resp.message.as_slice()
            );
        }

        // The original placement must be untouched, keeping the stored
        // chains acyclic.
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let p = storage
            .placements
            .get(&crate::kitty::graphics_storage::PlacementKey {
                image_id: 1,
                placement_id: crate::kitty::graphics_storage::PlacementId {
                    tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                    id: 1,
                },
            })
            .unwrap();
        assert!(matches!(p.location, Location::Pin(_)));

        t.deinit(alloc);
    }

    // test "kittygfx relative placement chain depth limit"
    #[test]
    fn kittygfx_relative_placement_chain_depth_limit() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=p,i=1,p=1,C=1".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Chains of up to parent_chain_limit (8) links must work: these
        // placements form the chain 9 -> 8 -> ... -> 2 -> 1.
        for id in 2..10 {
            let input = format!("a=p,i=1,p={},P=1,Q={}", id, id - 1);
            let cmd = command::Parser::parse_string(input.as_bytes()).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // One more link exceeds the limit.
        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,p=10,P=1,Q=9").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(
                b"ETOODEEP: parent chain too deep".as_slice(),
                resp.message.as_slice()
            );
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        assert_eq!(9usize, storage.placements.len());

        t.deinit(alloc);
    }

    // test "kittygfx relative placement does not move cursor"
    #[test]
    fn kittygfx_relative_placement_does_not_move_cursor() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // C is left at its default (move after) on the relative placement
        // but it must never move the cursor.
        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=p,i=1,p=1,C=1".as_slice(),
            b"a=p,i=1,p=2,P=1,Q=1,H=3,V=2,c=2,r=2".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        assert_eq!(0, unsafe { (*t.screens.active).cursor.x });
        assert_eq!(0, unsafe { (*t.screens.active).cursor.y });

        // The stored placement carries the parent link and offsets.
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let p = storage
            .placements
            .get(&crate::kitty::graphics_storage::PlacementKey {
                image_id: 1,
                placement_id: crate::kitty::graphics_storage::PlacementId {
                    tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                    id: 2,
                },
            })
            .unwrap();
        let rel = match p.location {
            Location::Relative(rel) => rel,
            _ => unreachable!(),
        };
        assert!(rel.parent.eql(crate::kitty::graphics_storage::PlacementKey {
            image_id: 1,
            placement_id: crate::kitty::graphics_storage::PlacementId {
                tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                id: 1,
            },
        }));
        assert_eq!(3i32, rel.horizontal_offset);
        assert_eq!(2i32, rel.vertical_offset);

        t.deinit(alloc);
    }

    // test "kittygfx virtual placement with parent rejected before image lookup"
    #[test]
    fn kittygfx_virtual_placement_with_parent_rejected_before_image_lookup() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Kitty rejects U=1 + P= before checking that the image exists,
        // so this must not be ENOENT.
        let cmd = command::Parser::parse_string(b"a=p,i=42,U=1,P=1").unwrap();
        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(!resp.ok());
        assert_eq!(
            b"EINVAL: virtual placement cannot refer to a parent".as_slice(),
            resp.message.as_slice()
        );

        t.deinit(alloc);
    }

    // test "kittygfx deleting a parent deletes relative placements transitively"
    #[test]
    fn kittygfx_deleting_a_parent_deletes_relative_placements_transitively() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Root (pin) <- child <- grandchild
        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=p,i=1,p=1,C=1".as_slice(),
            b"a=p,i=1,p=2,P=1,Q=1".as_slice(),
            b"a=p,i=1,p=3,P=1,Q=2".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        assert_eq!(
            3usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );

        // Deleting the middle placement takes the grandchild with it but
        // leaves the root.
        {
            let cmd = command::Parser::parse_string(b"a=d,d=i,i=1,p=2").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            assert_eq!(1usize, storage.placements.len());
            assert!(storage
                .placements
                .contains_key(&crate::kitty::graphics_storage::PlacementKey {
                    image_id: 1,
                    placement_id: crate::kitty::graphics_storage::PlacementId {
                        tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                        id: 1,
                    },
                }));
        }

        // Deleting the root removes the remaining placement, and the
        // image itself is retained (lowercase delete).
        {
            let cmd = command::Parser::parse_string(b"a=d,d=i,i=1,p=1").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            assert_eq!(0usize, storage.placements.len());
            assert!(storage.image_by_id(1).is_some());
        }

        t.deinit(alloc);
    }

    // test "kittygfx retransmitting parent image deletes relative placements"
    #[test]
    fn kittygfx_retransmitting_parent_image_deletes_relative_placements() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Image 2's placement is relative to image 1's placement.
        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=t,f=24,s=1,v=1,i=2;AAAA".as_slice(),
            b"a=p,i=1,p=1,C=1".as_slice(),
            b"a=p,i=2,p=1,P=1,Q=1".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        assert_eq!(
            2usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );

        // Retransmitting image 1 removes its placements, which orphans
        // and removes image 2's placement too. Image 2 is left without
        // any placement so it is freed as well: retransmission deletes
        // with uppercase semantics, and kitty also removes an image whose
        // last placement died from a broken parent chain.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;AAAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            assert_eq!(0usize, storage.placements.len());
            assert!(storage.image_by_id(2).is_none());
        }

        t.deinit(alloc);
    }

    // test "kittygfx relative placement parent fallback picks lowest external id"
    #[test]
    fn kittygfx_relative_placement_parent_fallback_picks_lowest_external_id() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=t,f=24,s=1,v=1,i=2;AAAA".as_slice(),
            b"a=p,i=1,p=7,C=1".as_slice(),
            b"a=p,i=1,p=3,C=1".as_slice(),
            b"a=p,i=2,P=1".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let rel = storage
            .placements
            .iter()
            .find_map(|(key, value)| {
                if key.image_id == 2 {
                    match value.location {
                        Location::Relative(rel) => Some(rel),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .expect("PlacementNotFound");
        assert!(rel.parent.eql(crate::kitty::graphics_storage::PlacementKey {
            image_id: 1,
            placement_id: crate::kitty::graphics_storage::PlacementId {
                tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                id: 3,
            },
        }));

        t.deinit(alloc);
    }

    // test "kittygfx placements created after a full reset are retained"
    #[test]
    fn kittygfx_placements_created_after_a_full_reset_are_retained() {
        // Regression test: Screen.reset reuses the cursor's tracked pin
        // but PageList.reset marked it garbage. Placements copy the cursor
        // pin, so every placement created after a reset was born garbage
        // and silently swept by the next placement command.
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(80, 24)).unwrap();

        t.full_reset();

        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=p,i=1,p=1,C=1".as_slice(),
            b"a=p,i=1,p=2,C=1".as_slice(),
            b"a=p,i=1,p=3,P=1,Q=2".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        assert_eq!(
            3usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );

        t.deinit(alloc);
    }

    // test "kittygfx relative placement with pruned parent is ENOPARENT"
    #[test]
    fn kittygfx_relative_placement_with_pruned_parent_is_enoparent() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=p,i=1,p=1,C=1".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // The parent's tracked content is pruned from history but the
        // placement hasn't been swept yet. The put must reap it and answer
        // ENOPARENT rather than storing an orphan against it.
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            let parent = storage
                .placements
                .get(&crate::kitty::graphics_storage::PlacementKey {
                    image_id: 1,
                    placement_id: crate::kitty::graphics_storage::PlacementId {
                        tag: crate::kitty::graphics_storage::PlacementIdTag::External,
                        id: 1,
                    },
                })
                .unwrap();
            let pin = match parent.location {
                Location::Pin(pin) => pin,
                _ => unreachable!(),
            };
            // parent.location.pin.garbage = true;
            // SAFETY: `pin` is a live tracked pin owned by the pagelist.
            unsafe { (*pin).garbage = true };
        }

        {
            let cmd = command::Parser::parse_string(b"a=p,i=1,p=2,P=1,Q=1").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(
                b"ENOPARENT: parent placement not found".as_slice(),
                resp.message.as_slice()
            );
        }
        assert_eq!(
            0usize,
            unsafe { &(*t.screens.active).kitty_images }
                .placements
                .len()
        );

        t.deinit(alloc);
    }

    // test "kittygfx uppercase delete frees image of cascaded placements"
    #[test]
    fn kittygfx_uppercase_delete_frees_image_of_cascaded_placements() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        for input in [
            b"a=t,f=24,s=1,v=1,i=1;AAAA".as_slice(),
            b"a=p,i=1,p=1,C=1".as_slice(),
            b"a=p,i=1,p=2,P=1,Q=1".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // The uppercase delete only matches placement 1. The cascade
        // removes placement 2, leaving the image without placements, so
        // the image must be freed too.
        {
            let cmd = command::Parser::parse_string(b"a=d,d=I,i=1,p=1").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            assert_eq!(0usize, storage.placements.len());
            assert!(storage.image_by_id(1).is_none());
        }

        t.deinit(alloc);
    }

    // test "kittygfx animation: new frame with default gap responds with frame number"
    #[test]
    fn kittygfx_animation_new_frame_with_default_gap_responds_with_frame_number() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Transmit a 1x1 RGB blue frame ("AAD/" is 0000FF).
        let cmd = command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1;AAD/").unwrap();
        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(resp.ok());
        assert_eq!(1u32, resp.id);
        assert_eq!(2u32, resp.frame);

        // var buf: [128]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        // try resp.encode(&writer);
        let mut buf: Vec<u8> = Vec::new();
        resp.encode(&mut buf).unwrap();
        assert_eq!(b"\x1b_Gi=1,r=2;OK\x1b\\".as_slice(), buf.as_slice());

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let img = storage.image_ptr_by_id_or_number(1, 0).unwrap();
        assert_eq!(command::Format::Rgba, img.format);
        assert_eq!(&[255, 0, 0, 255], img.data.bytes().unwrap());

        let anim = img.animation.as_ref().unwrap();
        assert_eq!(2u32, anim.frame_count());
        assert_eq!(0u32, anim.root_gap_ms);
        assert_eq!(40u32, anim.frames[0].gap_ms);
        assert_eq!(&[0, 0, 255, 255], anim.frames[0].data.as_slice());

        // The displayed frame is still the root: renderData is the base.
        assert_eq!(&[255, 0, 0, 255], img.render_data().bytes().unwrap());

        // Frame bytes count against the storage total.
        assert_eq!(8usize, storage.total_bytes);

        t.deinit(alloc);
    }

    // test "kittygfx animation: frame gap normalization on create"
    #[test]
    fn kittygfx_animation_frame_gap_normalization_on_create() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        for input in [
            b"a=f,i=1,f=24,s=1,v=1,z=100;AAD/".as_slice(),
            b"a=f,i=1,f=24,s=1,v=1,z=-5;AAD/".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let anim = storage
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .animation
            .as_ref()
            .unwrap();
        assert_eq!(100u32, anim.frames[0].gap_ms);
        assert_eq!(0u32, anim.frames[1].gap_ms);

        t.deinit(alloc);
    }

    // test "kittygfx animation: background fill and offset composition"
    #[test]
    fn kittygfx_animation_background_fill_and_offset_composition() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // 2x1 RGB white image ("////////" is FFFFFF FFFFFF).
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=2,v=1,i=1;////////").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // New frame: 1x1 RGB white transmitted at x=1 over an opaque red
        // background (Y=4278190335 is 0xff0000ff, R in the MSB).
        {
            let cmd =
                command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1,x=1,Y=4278190335;////")
                    .unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let anim = storage
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .animation
            .as_ref()
            .unwrap();
        assert_eq!(
            &[255, 0, 0, 255, 255, 255, 255, 255],
            anim.frames[0].data.as_slice()
        );

        t.deinit(alloc);
    }

    // test "kittygfx animation: create from base frame with overwrite"
    #[test]
    fn kittygfx_animation_create_from_base_frame_with_overwrite() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Frame 2 based on the root (c=1) with a semi-transparent blue
        // pixel ("AAD/gA==" is 0000FF80) in overwrite mode: the source
        // replaces the canvas including alpha.
        {
            let cmd =
                command::Parser::parse_string(b"a=f,i=1,f=32,s=1,v=1,c=1,X=1;AAD/gA==").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let anim = storage
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .animation
            .as_ref()
            .unwrap();
        assert_eq!(&[0, 0, 255, 128], anim.frames[0].data.as_slice());

        t.deinit(alloc);
    }

    // test "kittygfx animation: alpha blend composes over base frame"
    #[test]
    fn kittygfx_animation_alpha_blend_composes_over_base_frame() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Blend a fully opaque blue pixel over the root: alpha blending
        // an opaque source is equivalent to a copy.
        {
            let cmd = command::Parser::parse_string(b"a=f,i=1,f=32,s=1,v=1,c=1;AAD//w==").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Blend a fully transparent pixel over the root: the canvas is
        // unchanged (still the red base).
        {
            let cmd = command::Parser::parse_string(b"a=f,i=1,f=32,s=1,v=1,c=1;AAD/AA==").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let anim = storage
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .animation
            .as_ref()
            .unwrap();
        assert_eq!(&[0, 0, 255, 255], anim.frames[0].data.as_slice());
        assert_eq!(&[255, 0, 0, 255], anim.frames[1].data.as_slice());

        t.deinit(alloc);
    }

    // test "kittygfx animation: edit root frame bumps generation"
    #[test]
    fn kittygfx_animation_edit_root_frame_bumps_generation() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        let gen1 = unsafe { &(*t.screens.active).kitty_images }
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .generation;

        // r=1 edits the root frame in place. The root is the displayed
        // frame so the image generation must change.
        let cmd = command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1,r=1;AAD/").unwrap();
        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(resp.ok());
        assert_eq!(1u32, resp.frame);

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let img = storage.image_ptr_by_id_or_number(1, 0).unwrap();
        assert_eq!(&[0, 0, 255, 255], img.data.bytes().unwrap());
        assert!(img.generation > gen1);

        t.deinit(alloc);
    }

    // test "kittygfx animation: edit frame gap without data change"
    #[test]
    fn kittygfx_animation_edit_frame_gap_without_data_change() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Create frame 2, then edit it with z=-1: gap becomes gapless.
        // Editing with z=0 leaves the gap unchanged (no 40ms default).
        {
            let cmd = command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1,z=77;AAD/").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1,r=2;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            let anim = storage
                .image_ptr_by_id_or_number(1, 0)
                .unwrap()
                .animation
                .as_ref()
                .unwrap();
            assert_eq!(77u32, anim.frames[0].gap_ms);
        }

        {
            let cmd =
                command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1,r=2,z=-1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            let anim = storage
                .image_ptr_by_id_or_number(1, 0)
                .unwrap()
                .animation
                .as_ref()
                .unwrap();
            assert_eq!(0u32, anim.frames[0].gap_ms);
        }

        t.deinit(alloc);
    }

    // test "kittygfx animation: frame errors"
    #[test]
    fn kittygfx_animation_frame_errors() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        let cases: [(&[u8], &[u8]); 4] = [
            // Unknown image.
            (b"a=f,i=9,f=24,s=1,v=1;AAD/", b"ENOENT: image not found"),
            // Frame bigger than the image.
            (
                b"a=f,i=1,f=24,s=2,v=1;////////",
                b"EINVAL: frame dimensions exceed image",
            ),
            // Nonexistent base frame.
            (
                b"a=f,i=1,f=24,s=1,v=1,c=5;AAD/",
                b"EINVAL: base frame not found",
            ),
            // Insufficient data for the declared rectangle.
            (b"a=f,i=1,f=24,s=1,v=1;AA==", b"ENODATA: insufficient data"),
        ];
        for (input, message) in cases {
            let cmd = command::Parser::parse_string(input).unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(message, resp.message.as_slice());
        }

        // None of the failures may have created a frame.
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let img = storage.image_ptr_by_id_or_number(1, 0).unwrap();
        if let Some(anim) = &img.animation {
            assert_eq!(0usize, anim.frames.len());
        }

        t.deinit(alloc);
    }

    // test "kittygfx animation: excess frame data is truncated"
    #[test]
    fn kittygfx_animation_excess_frame_data_is_truncated() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Six bytes for a 1x1 RGB frame: Kitty tolerates the excess.
        let cmd = command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1;AAD/////").unwrap();
        assert!(execute(alloc, &mut t, &cmd).unwrap().ok());

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let anim = storage
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .animation
            .as_ref()
            .unwrap();
        assert_eq!(&[0, 0, 255, 255], anim.frames[0].data.as_slice());

        t.deinit(alloc);
    }

    // test "kittygfx animation: chunked frame transmission"
    #[test]
    fn kittygfx_animation_chunked_frame_transmission() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // 2x1 RGB white image.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=2,v=1,i=1;////////").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Transmit a 2x1 frame in two chunks of one pixel each. The
        // chunks repeat a=f per the protocol. No response until the
        // final chunk.
        {
            let cmd =
                command::Parser::parse_string(b"a=f,i=1,f=24,s=2,v=1,z=60,m=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        assert!(unsafe { &(*t.screens.active).kitty_images }.loading.is_some());
        {
            let cmd = command::Parser::parse_string(b"a=f,m=0;AAD/").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
            assert_eq!(1u32, resp.id);
            assert_eq!(2u32, resp.frame);
        }
        assert!(unsafe { &(*t.screens.active).kitty_images }.loading.is_none());

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let anim = storage
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .animation
            .as_ref()
            .unwrap();
        assert_eq!(60u32, anim.frames[0].gap_ms);
        assert_eq!(
            &[255, 0, 0, 255, 0, 0, 255, 255],
            anim.frames[0].data.as_slice()
        );

        t.deinit(alloc);
    }

    // test "kittygfx animation: chunked frame continuation without a=f"
    #[test]
    fn kittygfx_animation_chunked_frame_continuation_without_a_f() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=2,v=1,i=1;////////").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // The spec requires chunks to repeat a=f, but a bare continuation
        // (which parses as a transmit action) must still continue the
        // frame load rather than turn into an image transmission.
        {
            let cmd = command::Parser::parse_string(b"a=f,i=1,f=24,s=2,v=1,m=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        {
            let cmd = command::Parser::parse_string(b"m=0;AAD/").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
            assert_eq!(2u32, resp.frame);
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let anim = storage
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .animation
            .as_ref()
            .unwrap();
        assert_eq!(1usize, anim.frames.len());

        t.deinit(alloc);
    }

    // test "kittygfx animation: control command sets gap current state and loops"
    #[test]
    fn kittygfx_animation_control_command_sets_gap_current_state_and_loops() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1;AAD/").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        let gen1 = unsafe { &(*t.screens.active).kitty_images }
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .generation;

        // One command can set several things: the root frame's gap
        // (r=1,z=50), the current frame (c=2), the state (s=3, running),
        // and the loop count (v=3, meaning two loops). Successful a=a
        // commands never respond.
        let cmd = command::Parser::parse_string(b"a=a,i=1,r=1,z=50,c=2,s=3,v=3").unwrap();
        assert!(execute(alloc, &mut t, &cmd).is_none());

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let img = storage.image_ptr_by_id_or_number(1, 0).unwrap();
        let anim = img.animation.as_ref().unwrap();
        assert_eq!(50u32, anim.root_gap_ms);
        assert_eq!(1u32, anim.current_index);
        assert_eq!(animation::State::Running, anim.state);
        assert_eq!(2u32, anim.max_loops);

        // Changing the current frame changes the displayed content.
        assert!(img.generation > gen1);
        assert_eq!(&[0, 0, 255, 255], img.render_data().bytes().unwrap());

        t.deinit(alloc);
    }

    // test "kittygfx animation: control command ignores invalid values"
    #[test]
    fn kittygfx_animation_control_command_ignores_invalid_values() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Out-of-range frame numbers and s values are silently ignored.
        let cmd = command::Parser::parse_string(b"a=a,i=1,r=9,z=50,c=9,s=7").unwrap();
        assert!(execute(alloc, &mut t, &cmd).is_none());

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let anim = storage
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .animation
            .as_ref()
            .unwrap();
        assert_eq!(0u32, anim.root_gap_ms);
        assert_eq!(0u32, anim.current_index);
        assert_eq!(animation::State::Stopped, anim.state);

        t.deinit(alloc);
    }

    // test "kittygfx animation: control command missing image responds ENOENT"
    #[test]
    fn kittygfx_animation_control_command_missing_image_responds_enoent() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        let cmd = command::Parser::parse_string(b"a=a,i=9,s=3").unwrap();
        let resp = execute(alloc, &mut t, &cmd).unwrap();
        assert!(!resp.ok());
        assert_eq!(b"ENOENT: image not found".as_slice(), resp.message.as_slice());

        t.deinit(alloc);
    }

    // test "kittygfx animation: compose frames"
    #[test]
    fn kittygfx_animation_compose_frames() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // 2x1 RGB image: red then blue ("/wAAAAD/" is FF0000 0000FF).
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=2,v=1,i=1;/wAAAAD/").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        // Frame 2 starts as a transparent canvas with a white pixel at
        // x=0 ("////" is FFFFFF).
        {
            let cmd = command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1;////").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Compose the root's pixel at (1,0) onto frame 2 at (0,0):
        // r=1 is the source frame, c=2 the destination, X/Y the source
        // offsets, x/y the destination offsets.
        {
            let cmd = command::Parser::parse_string(b"a=c,i=1,r=1,c=2,w=1,h=1,X=1,x=0").unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(resp.ok());
            assert_eq!(0u32, resp.frame);
        }

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let anim = storage
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .animation
            .as_ref()
            .unwrap();
        assert_eq!(
            &[0, 0, 255, 255, 0, 0, 0, 0],
            anim.frames[0].data.as_slice()
        );

        t.deinit(alloc);
    }

    // test "kittygfx animation: compose current frame bumps generation"
    #[test]
    fn kittygfx_animation_compose_current_frame_bumps_generation() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // 2x1 RGB white image; the root frame is the displayed frame.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=2,v=1,i=1;////////").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        let gen1 = unsafe { &(*t.screens.active).kitty_images }
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .generation;

        // a=c works on images without animation state: both frames are
        // the root. Copy pixel 0 onto pixel 1.
        let cmd = command::Parser::parse_string(b"a=c,i=1,r=1,c=1,w=1,h=1,x=1").unwrap();
        assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        let img_generation = unsafe { &(*t.screens.active).kitty_images }
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .generation;
        assert!(img_generation > gen1);

        t.deinit(alloc);
    }

    // test "kittygfx animation: compose errors"
    #[test]
    fn kittygfx_animation_compose_errors() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        let cases: [(&[u8], &[u8]); 6] = [
            (b"a=c,i=9,r=1,c=1", b"ENOENT: image not found"),
            (b"a=c,i=1,r=2,c=1", b"ENOENT: source frame not found"),
            (b"a=c,i=1,r=1,c=2", b"ENOENT: destination frame not found"),
            // 1x1 image: any offset pushes the rect out of bounds.
            (
                b"a=c,i=1,r=1,c=1,x=1",
                b"EINVAL: destination rectangle out of bounds",
            ),
            (
                b"a=c,i=1,r=1,c=1,X=1",
                b"EINVAL: source rectangle out of bounds",
            ),
            // Same frame, same (whole-image) rect: overlap.
            (
                b"a=c,i=1,r=1,c=1",
                b"EINVAL: source and destination rectangles overlap",
            ),
        ];
        for (input, message) in cases {
            let cmd = command::Parser::parse_string(input).unwrap();
            let resp = execute(alloc, &mut t, &cmd).unwrap();
            assert!(!resp.ok());
            assert_eq!(message, resp.message.as_slice());
        }

        t.deinit(alloc);
    }

    // test "kittygfx animation: delete frame promotes root and fixes current"
    #[test]
    fn kittygfx_animation_delete_frame_promotes_root_and_fixes_current() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        // Frames 2 and 3: blue and white.
        for input in [
            b"a=f,i=1,f=24,s=1,v=1,z=10;AAD/".as_slice(),
            b"a=f,i=1,f=24,s=1,v=1,z=20;////".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        let total_before = unsafe { &(*t.screens.active).kitty_images }.total_bytes;

        // Delete the root frame (r omitted selects it): frame 2 becomes
        // the new root, including its gap. Deletes never respond.
        {
            let cmd = command::Parser::parse_string(b"a=d,d=f,i=1").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            let img = storage.image_ptr_by_id_or_number(1, 0).unwrap();
            let anim = img.animation.as_ref().unwrap();
            assert_eq!(2u32, anim.frame_count());
            assert_eq!(10u32, anim.root_gap_ms);
            assert_eq!(&[0, 0, 255, 255], img.data.bytes().unwrap());
            assert_eq!(total_before - 4, storage.total_bytes);
        }

        // Delete a frame number past the end: clamps to the last frame.
        {
            let cmd = command::Parser::parse_string(b"a=d,d=f,i=1,r=9").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            let anim = storage
                .image_ptr_by_id_or_number(1, 0)
                .unwrap()
                .animation
                .as_ref()
                .unwrap();
            assert_eq!(1u32, anim.frame_count());
            assert_eq!(total_before - 8, storage.total_bytes);
        }

        // Down to one frame: d=f is now a no-op and the image survives.
        {
            let cmd = command::Parser::parse_string(b"a=d,d=f,i=1").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        assert!(unsafe { &(*t.screens.active).kitty_images }
            .image_by_id(1)
            .is_some());

        t.deinit(alloc);
    }

    // test "kittygfx animation: uppercase frame delete removes non-animated image"
    #[test]
    fn kittygfx_animation_uppercase_frame_delete_removes_non_animated_image() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();

        // Transmit and place: unlike d=I, an uppercase frame delete on a
        // non-animated image removes it even though it is placed.
        {
            let cmd = command::Parser::parse_string(b"a=T,f=24,s=1,v=1,i=1,C=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        assert_eq!(
            1usize,
            unsafe { &(*t.screens.active).kitty_images }.placements.len()
        );

        {
            let cmd = command::Parser::parse_string(b"a=d,d=F,i=1").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        let storage = unsafe { &(*t.screens.active).kitty_images };
        assert!(storage.image_by_id(1).is_none());
        assert_eq!(0usize, storage.placements.len());
        assert_eq!(0usize, storage.total_bytes);

        t.deinit(alloc);
    }

    // test "kittygfx animation: deleting current frame refreshes display"
    #[test]
    fn kittygfx_animation_deleting_current_frame_refreshes_display() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        for input in [
            b"a=f,i=1,f=24,s=1,v=1;AAD/".as_slice(),
            b"a=f,i=1,f=24,s=1,v=1;////".as_slice(),
            b"a=a,i=1,c=3".as_slice(),
        ] {
            let cmd = command::Parser::parse_string(input).unwrap();
            let _ = execute(alloc, &mut t, &cmd);
        }

        let gen1 = {
            let storage = unsafe { &(*t.screens.active).kitty_images };
            let img = storage.image_ptr_by_id_or_number(1, 0).unwrap();
            let anim = img.animation.as_ref().unwrap();
            assert_eq!(2u32, anim.current_index);
            img.generation
        };

        // Deleting the last frame (the current one) clamps the index and
        // refreshes the display.
        {
            let cmd = command::Parser::parse_string(b"a=d,d=f,i=1,r=3").unwrap();
            assert!(execute(alloc, &mut t, &cmd).is_none());
        }
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let img = storage.image_ptr_by_id_or_number(1, 0).unwrap();
        let anim = img.animation.as_ref().unwrap();
        assert_eq!(1u32, anim.current_index);
        assert!(img.generation > gen1);
        assert_eq!(&[0, 0, 255, 255], img.render_data().bytes().unwrap());

        t.deinit(alloc);
    }

    // test "kittygfx animation: retransmitting base image resets animation"
    #[test]
    fn kittygfx_animation_retransmitting_base_image_resets_animation() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1;AAD/").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        assert_eq!(
            8usize,
            unsafe { &(*t.screens.active).kitty_images }.total_bytes
        );

        // Retransmit the base image: the animation is dropped and the
        // frame bytes are credited back.
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        let storage = unsafe { &(*t.screens.active).kitty_images };
        let img = storage.image_ptr_by_id_or_number(1, 0).unwrap();
        assert!(img.animation.is_none());
        assert_eq!(3usize, storage.total_bytes);

        t.deinit(alloc);
    }

    // test "kittygfx animation: control negative gap makes frame gapless"
    #[test]
    fn kittygfx_animation_control_negative_gap_makes_frame_gapless() {
        let alloc = allocator_api2::alloc::Global;

        let mut t = Terminal::init(alloc, opts(5, 5)).unwrap();
        {
            let cmd = command::Parser::parse_string(b"a=t,f=24,s=1,v=1,i=1;/wAA").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }
        {
            let cmd = command::Parser::parse_string(b"a=f,i=1,f=24,s=1,v=1;AAD/").unwrap();
            assert!(execute(alloc, &mut t, &cmd).unwrap().ok());
        }

        let cmd = command::Parser::parse_string(b"a=a,i=1,r=2,z=-1").unwrap();
        assert!(execute(alloc, &mut t, &cmd).is_none());

        let storage = unsafe { &(*t.screens.active).kitty_images };
        let anim = storage
            .image_ptr_by_id_or_number(1, 0)
            .unwrap()
            .animation
            .as_ref()
            .unwrap();
        assert_eq!(0u32, anim.frames[0].gap_ms);

        t.deinit(alloc);
    }
}
