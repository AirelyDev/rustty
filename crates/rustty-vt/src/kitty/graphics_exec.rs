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

    // Attempt to load the image. If we cannot, then set an appropriate error.
    // const storage = &terminal.screens.active.kitty_images;
    let storage = unsafe { &(*terminal.screens.active).kitty_images };
    // var loading = LoadingImage.init(alloc, cmd, storage.image_limits) catch |err| {
    //     encodeError(&result, err);
    //     return result;
    // };
    let _ = &alloc;
    let loading = match LoadingImage::init(cmd, &storage.image_limits) {
        Ok(loading) => loading,
        Err(err) => {
            encode_error(&mut result, err);
            return result;
        }
    };
    // loading.deinit(alloc);
    drop(loading);

    result
}

/// Transmit image data.
///
/// This loads the image, validates it, and puts it into the terminal
/// screen storage. It does not display the image.
fn transmit(alloc: Allocator, terminal: &mut Terminal, cmd: &Command) -> Response {
    let t = cmd.transmission().unwrap();
    let mut result: Response = Response {
        id: t.image_id,
        image_number: t.image_number,
        placement_id: t.placement_id,
        ..Response::default()
    };
    if t.image_id > 0 && t.image_number > 0 {
        return Response {
            message: b"EINVAL: image ID and number are mutually exclusive".to_vec(),
            ..Response::default()
        };
    }

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
        if d.virtual_placement {
            if d.parent_id > 0 {
                result.message = b"EINVAL: virtual placement cannot refer to a parent".to_vec();
                return result;
            }

            // break :location .{ .virtual = {} };
            break 'location Location::Virtual;
        }

        // Track a new pin for our cursor. The cursor is always tracked but we
        // don't want this one to move with the cursor.
        // const pin = terminal.screens.active.pages.trackPin(
        //     terminal.screens.active.cursor.page_pin.*,
        // ) catch |err| {
        //     log.warn("failed to create pin for Kitty graphics err={}", .{err});
        //     result.message = "EINVAL: failed to prepare terminal state";
        //     return result;
        // };
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
    };

    // Add the placement
    // const p: ImageStorage.Placement = .{ ... };
    let p: Placement = Placement {
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
    // storage.addPlacement(
    //     alloc,
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
    // `terminal.screens.active` accesses.
    // PORT NOTE: `add_placement` consumes the owned `Placement` (stored in the
    // map); the reference re-reads `p` (a by-value copy) afterward for the
    // error-path `deinit`, the cursor-movement switch on `p.location`, and
    // `p.gridSize`. `Placement` has no allocations (its `Location` is a plain
    // pointer-or-unit union), so we snapshot a field-for-field copy `p_copy`
    // before the move and use it for all post-move reads.
    let p_copy: Placement = Placement {
        location: match &p.location {
            Location::Pin(pin) => Location::Pin(*pin),
            Location::Virtual => Location::Virtual,
        },
        x_offset: p.x_offset,
        y_offset: p.y_offset,
        source_x: p.source_x,
        source_y: p.source_y,
        source_width: p.source_width,
        source_height: p.source_height,
        columns: p.columns,
        rows: p.rows,
        z: p.z,
    };
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
        p_copy.deinit(terminal.screens.active);
        encode_error_storage(&mut result, err);
        return result;
    }

    // Apply cursor movement setting. This only applies to pin placements.
    // switch (p.location) {
    match p_copy.location {
        // .virtual => {},
        Location::Virtual => {}
        // .pin => |pin| switch (d.cursor_movement) {
        Location::Pin(pin) => match d.cursor_movement {
            // .none => {},
            command::CursorMovement::None => {}
            // .after => { ... },
            command::CursorMovement::After => {
                // We use terminal.index to properly handle scroll regions.
                // const size = p.gridSize(img, terminal);
                let size = p_copy.grid_size(&img, terminal);
                // Once the requested movement leaves the screen, its exact
                // position is undefined by the Kitty graphics protocol. Bound
                // the work so an untrusted row count can't make us spin.
                // const rows_to_move: usize = @min(
                //     @as(usize, size.rows),
                //     @as(usize, terminal.rows),
                // );
                let rows_to_move: usize =
                    core::cmp::min(size.rows as usize, terminal.rows as usize);
                for _ in 0..rows_to_move {
                    if terminal.index().is_err() {
                        // log.warn("failed to move cursor: {}", .{err});
                        break;
                    }
                }

                // terminal.setCursorPos(
                //     terminal.screens.active.cursor.y,
                //     @as(usize, pin.x) +| @as(usize, size.cols) +| 1,
                // );
                // (R7: the `+|`s saturate by design for untrusted `size.cols`.)
                let cursor_y = unsafe { (*terminal.screens.active).cursor.y };
                let pin_x = unsafe { (*pin).x };
                terminal.set_cursor_pos(
                    cursor_y as usize,
                    (pin_x as usize)
                        .saturating_add(size.cols as usize)
                        .saturating_add(1),
                );
            }
        },
    }

    result
}

/// Display a previously transmitted image.
fn delete(alloc: Allocator, terminal: &mut Terminal, cmd: &Command) -> Response {
    // const storage = &terminal.screens.active.kitty_images;
    // storage.delete(alloc, terminal, cmd.control.delete);
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
    storage.delete(alloc, terminal, d);
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
        // else try .init(alloc, cmd, storage.image_limits);
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

    // If the image has no ID, we assign one
    if loading.image.id == 0 {
        loading.image.id = storage.next_image_id;
        // storage.next_image_id +%= 1;
        storage.next_image_id = storage.next_image_id.wrapping_add(1);

        // If the image also has no number then its auto-ID is "implicit".
        // See the doc comment on Image.metadata.implicit_id for more detail.
        if loading.image.number == 0 {
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
    use crate::kitty::graphics_image::Limits;
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

        t.deinit(alloc);
    }
}
