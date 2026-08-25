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
use crate::kitty::graphics_command as command;
use crate::kitty::graphics_command::{Command, Response};
use crate::kitty::graphics_image as image;
use crate::kitty::graphics_image::{Image, LoadingImage};
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

        // .transmit, .transmit_and_display => resp: {
        command::Control::Transmit(_) | command::Control::TransmitAndDisplay { .. } => {
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

            // break :resp transmit(alloc, terminal, cmd);
            Some(transmit(alloc, terminal, cmd))
        }

        // .transmit_animation_frame,
        // .control_animation,
        // .compose_animation,
        // => .{ .message = "ERROR: unimplemented action" },
        command::Control::TransmitAnimationFrame(_)
        | command::Control::ControlAnimation(_)
        | command::Control::ComposeAnimation(_) => Some(Response {
            message: b"ERROR: unimplemented action".to_vec(),
            ..Response::default()
        }),
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
    // var result: Response = if (storage.loading) |loading|
    //     loading.response
    // else
    //     .{ .id = ..., .image_number = ..., .placement_id = ... };
    let storage = unsafe { &(*terminal.screens.active).kitty_images };
    let mut result: Response = if let Some(loading) = storage.loading.as_ref() {
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
}
