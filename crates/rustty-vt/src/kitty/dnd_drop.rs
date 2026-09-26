//! Kitty drag and drop protocol (OSC 72) state machine.
//!
//! rustty-vt kitty/dnd_drop
//!
//! PORTS: ghostty src/terminal/kitty/dnd_drop.zig
//!
//! PORT STATUS: ported (phase-5 B6 end state at 683d8db64: 50f69b883 +
//! af8d28a94's dnd_drop tweak; the stream_terminal/Terminal dispatch half of
//! af8d28a94 is a separate stream-owner change — see the PORT-NOTE seam on
//! `handle_command`).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// PORT NOTE (parity tail S7-3): every `alloc: Allocator` parameter is kept.
// The state and everything it holds are allocated through that allocator (the
// terminal's `gpa()`), fallibly: the containers are allocator_api2 vectors
// bound to it when the state is created, so each later `alloc` argument is
// checked against that binding in debug builds (all calls must use the
// allocator the state was created with, the reference's own contract).
// `(Allocator.Error || std.Io.Writer.Error)!T` is `std::io::Result<T>`, with
// `error.OutOfMemory` as `std::io::ErrorKind::OutOfMemory` ([`oom`]).
use crate::alloc::{Allocator, CopyVecExt, TryPushExt};
use allocator_api2::boxed::Box as ABox;
use allocator_api2::vec::Vec as AVec;

/// Bytes allocated through the state's allocator.
pub type Bytes = AVec<u8, Allocator>;

/// `*State` allocated through the terminal's allocator (`alloc.create(State)`).
pub type StateBox = ABox<State, Allocator>;

/// `error.OutOfMemory` in the `std::io::Result` error union.
fn oom() -> std::io::Error {
    std::io::Error::from(std::io::ErrorKind::OutOfMemory)
}

// const assert = @import("../../quirks.zig").inlineAssert;
// PORT NOTE: retargeted to `debug_assert!` at the call site (crate convention).
use crate::kitty::dnd_command as command;
use crate::kitty::dnd_response as response;
use crate::osc;

use command::{Metadata, Operation, Operations};

// const log = std.log.scoped(.kitty_dnd);
// The scope, carried through the `log` facade as `target: LOG_SCOPE`.
// CORRECTION (task S-1c): a PORT NOTE here said "Rustty has no logging facade
// wired into this crate", and the `log.debug` site was ported as a comment.
// The crate logs through the `log` facade, so the call is live.
const LOG_SCOPE: &str = "kitty_dnd";

/// Maximum accumulated size of a client-sent MIME list (the accepted
/// list of a `t=m` status update). Matches kitty's MIME_LIST_SIZE_CAP.
pub const MAX_MIME_LIST_BYTES: usize = 1024 * 1024;

/// Process one OSC 72 command received from the client, writing any
/// responses to the writer. Returns the state change the embedder may
/// need to act on, if any.
///
/// PORT NOTE (seam): the stream_terminal dispatch that feeds this from the
/// OSC layer and the `Terminal.kitty_dnd` slot (af8d28a94's stream half) are
/// the stream owner's change; this function, `State`, and `Event` are the
/// complete pub surface it consumes.
///
/// PORT NOTE: `slot: *?*State` (heap pointer slot the function allocates
/// into / frees) → `&mut Option<StateBox>`; `v: osc.Command.KittyDndProtocol`
/// (a struct of borrowed slices) → `&osc::KittyDndProtocol` (self-owning).
pub fn handle_command<W: std::io::Write>(
    slot: &mut Option<StateBox>,
    alloc: Allocator,
    writer: &mut W,
    v: &osc::KittyDndProtocol,
) -> std::io::Result<Option<Event>> {
    let raw = match Metadata::parse(v.metadata) {
        Some(raw) => raw,
        None => {
            // log.debug("dropping malformed OSC 72 metadata", .{});
            log::debug!(target: LOG_SCOPE, "dropping malformed OSC 72 metadata");
            return Ok(None);
        }
    };

    // Chunk reassembly lives in the state, so before registration
    // each command stands alone. The only legitimately chunked
    // command before registration is t=a itself, which seeds the
    // reassembly on its first chunk below.
    let continuation = match slot.as_ref() {
        Some(state) => state.chunking.active,
        None => false,
    };
    let meta = match slot.as_mut() {
        Some(state) => state.chunking.apply(raw),
        None => raw,
    };
    let payload: &[u8] = v.payload.unwrap_or(b"");
    let t = match meta.r#type {
        Some(t) => t,
        None => return Ok(None),
    };

    match t {
        command::EventType::Register => {
            // x=1 declares the client's machine ID for remote drop
            // support. We don't support remote drop yet, so accept and ignore.
            if meta.cell_x == 1 {
                return Ok(None);
            }

            // Setup our state if we haven't already
            let state: &mut State = match slot {
                Some(state) => state,
                None => {
                    // const state = try State.create(alloc);
                    let state = slot.insert(State::create(alloc)?);
                    let _ = state.chunking.apply(raw);
                    state
                }
            };

            // Update the client ID on every registration
            state.drop.client_id = meta.client_id;

            return state.register(alloc, payload, continuation, meta.more);
        }

        command::EventType::Unregister => {
            // const state = slot.* orelse return null;
            // state.destroy(alloc);
            // slot.* = null;
            let Some(state) = slot.take() else {
                return Ok(None);
            };
            State::destroy(state, alloc);
            return Ok(Some(Event::Registration));
        }

        command::EventType::Status => {
            let state = match slot.as_mut() {
                Some(state) => state,
                None => return Ok(None),
            };
            return state.accept_status(alloc, meta, payload);
        }

        command::EventType::Request => {
            return data_request(slot.as_deref_mut(), alloc, writer, meta, v.terminator);
        }

        // Drag source control. Enabling (x=1, with an optional
        // machine ID payload) and disabling (x=2) offers are
        // accepted and ignored since the terminal never requests a
        // drag start. Offering a MIME list (x=0) for a new drag is
        // refused since drag-out is not implemented.
        command::EventType::Offer => {
            if meta.cell_x == 0 {
                refuse_drag_out(writer, meta, v.terminator)?;
            }
        }

        // Drag-out data and start commands. A conforming client
        // never sends these because the terminal never requests a
        // drag start, but refuse them properly if one does.
        command::EventType::Present | command::EventType::StartDrag => {
            refuse_drag_out(writer, meta, v.terminator)?;
        }

        // Responses to drag-out requests the terminal never makes.
        command::EventType::DragEvent
        | command::EventType::DragError
        | command::EventType::RemoteData => {}

        command::EventType::Query => {
            response::encode(
                writer,
                b"t=q",
                meta.client_id,
                b"",
                response::Encoding::Plain,
                v.terminator,
            )?;
        }

        // Only ever sent by the terminal. Ignore.
        command::EventType::Drop | command::EventType::RequestError => {}
    }

    Ok(None)
}

/// Handle a t=r data request or drop conclusion from the client.
/// Requests from an unregistered client (no state) get the same
/// errors kitty sends from its zeroed drop state.
fn data_request<W: std::io::Write>(
    state: Option<&mut State>,
    alloc: Allocator,
    writer: &mut W,
    meta: Metadata,
    terminator: osc::Terminator,
) -> std::io::Result<Option<Event>> {
    // Responses echo the registration's client ID, matching kitty.
    let client_id = match state.as_ref() {
        Some(s) => s.drop.client_id,
        None => 0,
    };

    match command::Request::init(meta) {
        command::Request::Conclude(op) => {
            // The client is done with the drop: free the held data and
            // report the operation it performed. Kitty hands that to
            // the still-open OS drag session; ours ended at drop time
            // (see dnd.rs), so the embedder decides what to do with
            // it. A conclusion with no drop in progress is a no-op.
            let s = match state {
                Some(s) => s,
                None => return Ok(None),
            };
            let dropped = s.drop.dropped;
            s.reset_drop(alloc);
            return Ok(if dropped {
                Some(Event::concluded(op))
            } else {
                None
            });
        }

        command::Request::Mime(idx) => {
            let keys = response::RequestKeys {
                x: idx,
                ..Default::default()
            };
            // const items = (if (state) |s| s.drop.items else null) orelse { ... };
            let items = match state.as_ref().and_then(|s| s.drop.items.as_ref()) {
                Some(items) => items,
                None => {
                    response::encode_error(
                        writer,
                        response::ErrorKind::Drop,
                        keys,
                        client_id,
                        response::Errno::ENOENT,
                        b"no drop data available",
                        terminator,
                    )?;
                    return Ok(None);
                }
            };
            if idx < 1 || idx as usize > items.len() {
                response::encode_error(
                    writer,
                    response::ErrorKind::Drop,
                    keys,
                    client_id,
                    response::Errno::ENOENT,
                    b"drop data request index out of bounds",
                    terminator,
                )?;
                return Ok(None);
            }

            // var header_buf: [32]u8 = undefined;
            // const header = std.fmt.bufPrint(&header_buf, "t=r{f}", .{keys})
            //     catch unreachable;
            let header = format!("t=r{}", keys);

            // The data chunks followed by the empty end-of-data
            // message, which is how the client detects completion.
            // An empty item is just the end-of-data message alone;
            // clients treat a duplicate as a second completion.
            let item = &items[(idx - 1) as usize];
            if !item.data.is_empty() {
                response::encode(
                    writer,
                    header.as_bytes(),
                    client_id,
                    &item.data,
                    response::Encoding::Base64,
                    terminator,
                )?;
            }
            response::encode(
                writer,
                header.as_bytes(),
                client_id,
                b"",
                response::Encoding::Base64,
                terminator,
            )?;
            return Ok(None);
        }

        // Remote drop transfers (URI file contents and directory
        // handles). We never advertise remote support (no X=1
        // marker), so a conforming client never sends these.
        command::Request::Uri { mime_idx, uri_idx } => {
            response::encode_error(
                writer,
                response::ErrorKind::Drop,
                response::RequestKeys {
                    x: mime_idx,
                    y: uri_idx,
                    ..Default::default()
                },
                client_id,
                response::Errno::EINVAL,
                b"remote drop data is not supported",
                terminator,
            )?;
        }
        command::Request::Dir { handle, entry } => {
            response::encode_error(
                writer,
                response::ErrorKind::Drop,
                response::RequestKeys {
                    x: entry,
                    cap_y: handle,
                    ..Default::default()
                },
                client_id,
                response::Errno::EINVAL,
                b"remote drop data is not supported",
                terminator,
            )?;
        }
    }

    Ok(None)
}

/// Refuse a drag-out command with an error, since ghostty does not
/// implement the terminal side of client-initiated drags yet.
fn refuse_drag_out<W: std::io::Write>(
    writer: &mut W,
    meta: Metadata,
    terminator: osc::Terminator,
) -> std::io::Result<()> {
    response::encode_error(
        writer,
        response::ErrorKind::Drag,
        response::RequestKeys::default(),
        meta.client_id,
        response::Errno::EPERM,
        b"drag out is not supported by this terminal",
        terminator,
    )
}

/// A protocol state change an embedder may need to act on, returned by
/// `handle_command` and delivered through the stream handler's
/// `drag_and_drop` effect. This is a flat enum so it can cross a C API
/// unchanged; any details are read back from `Terminal.kitty_dnd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// The client registered (t=a), re-registered, or unregistered
    /// (t=A) to accept drops. An embedder may want to use this
    /// to setup the proper mime types to accept (e.g. on macOS)
    /// or not (unregistered).
    Registration,

    /// The client answered the drag currently over the terminal.
    /// `State::client_accepted` has the answer. Embedders can refresh the
    /// OS drag feedback immediately rather than on the next move.
    Acceptance,

    /// The client concluded a drop, performing no operation (it
    /// canceled), a copy, or a move. The held drop data has been freed.
    ConcludedNone,
    ConcludedCopy,
    ConcludedMove,
}

impl Event {
    /// The conclusion event for a performed operation.
    pub fn concluded(op: Operation) -> Event {
        match op {
            Operation::None => Event::ConcludedNone,
            Operation::Copy => Event::ConcludedCopy,
            Operation::Move => Event::ConcludedMove,
        }
    }
}

/// The per-terminal drop target state.
///
/// The primary entrypoint is `handle_command` which takes a
/// `&mut Option<Box<State>>` slot that it can heap allocate into when DnD
/// activates and free when it deactivates.
///
/// The normal lifecycle:
///
///   1. The stream handler feeds every OSC 72 command received from the
///      client to `handle_command`. The client registers (t=a), which
///      allocates the state into the slot and yields a `registration`
///      event so the embedder can register any declared MIME types
///      with the OS. Until then `handle_command` only answers stateless
///      commands (queries, error responses).
///   2. A native drag enters or moves over the terminal. When the slot
///      is non-null, the embedder calls `drag_move` with the pointer
///      position, the operations the drag source allows, and the MIME
///      types it can serve if dropped. This sends the client a t=m
///      move event; when the slot is null the embedder should handle
///      the drag as it would without the protocol.
///   3. The client answers with its acceptance (t=m:o=N), recorded by
///      `handle_command` which yields an `acceptance` event. The
///      embedder reads `client_accepted` then and on subsequent moves
///      to give the OS drag session its feedback.
///   4. The drag either leaves, and the embedder calls `drag_leave` to
///      send the t=m leave event, or drops: the embedder captures the
///      representations it advertised and calls `drag_drop`, which
///      copies and holds them and sends the client a t=M drop event. A
///      new drag entering before the client concludes discards the
///      held drop.
///   5. The client requests data (t=r:x=N), which `handle_command`
///      serves from the held copies, and then concludes the drop
///      (t=r:o=N), which frees them and yields a `concluded_*` event
///      naming the operation the client performed.
///   6. The client unregisters (t=A) and `handle_command` frees the
///      state, yielding a final `registration` event, or the terminal
///      is deinitialized and drops the slot.
///
/// All calls require the same synchronization as any other terminal
/// mutation.
///
/// PORT NOTE (parity tail S7-3): the reference's "all calls must use the
/// allocator the state was created with (the terminal's)" contract holds: the
/// state's containers are bound to that allocator (`create`), each call's
/// `alloc` is checked against it in debug builds, and `destroy`/`deinit` free
/// through it (the containers' Drop).
#[derive(Debug)]
pub struct State {
    /// Chunk reassembly for client commands. This is the only part of
    /// the state cleared by a terminal reset (RIS), matching kitty.
    pub chunking: command::Chunking,

    /// Drop target state for the registered client.
    pub drop: DropTarget,
}

/// PORT NOTE: nested `State.DropTarget` hoisted to a module-level type
/// (Rust has no nested type namespaces); same for `Item`, `Offered`, and
/// `MoveEvent`.
#[derive(Debug)]
pub struct DropTarget {
    /// Multiplexer client ID from registration, echoed in every
    /// drop-side message the terminal sends.
    pub client_id: u32,

    /// The MIME list the client registered with (the t=a payload),
    /// space-separated as received and accumulated across chunks.
    /// Only needed by embedders that must register types with the
    /// OS ahead of a drag; kitty frees it after doing so, we keep
    /// it so the `registration` event can be acted on from here.
    pub registered_mimes: Bytes,

    /// True while the pointer of a native drag is over the terminal.
    pub hovered: bool,

    /// True after the native drop until the client concludes it.
    pub dropped: bool,

    /// The client's response to the current drag, null until the
    /// client has responded. `none` means the client rejected it.
    pub accepted: Option<Operation>,

    /// True while a chunked t=m acceptance is being accumulated.
    pub accept_in_progress: bool,

    /// The client's accepted MIME list: space-separated while
    /// accumulating, converted to NUL-separated (with a trailing
    /// NUL) once complete, matching kitty's in-place conversion.
    pub accepted_mimes: Bytes,

    /// The MIME types of the current native drag, in the order
    /// that data request indices refer to.
    pub offered: Option<Offered>,

    /// The data captured at drop time, parallel to `offered`.
    pub items: Option<AVec<HeldItem, Allocator>>,
}

impl DropTarget {
    /// The `.{}` default, with its lists bound to `alloc` (no allocation).
    fn new_in(alloc: Allocator) -> DropTarget {
        DropTarget {
            client_id: 0,
            registered_mimes: Bytes::new_in(alloc),
            hovered: false,
            dropped: false,
            accepted: None,
            accept_in_progress: false,
            accepted_mimes: Bytes::new_in(alloc),
            offered: None,
            items: None,
        }
    }
}

/// A dropped representation held by the state: the `Item` copies `dragDrop`
/// makes with `alloc.dupe`.
#[derive(Debug)]
pub struct HeldItem {
    pub mime: Bytes,
    pub data: Bytes,
}

/// One dropped representation: a MIME type and its data.
///
/// PORT NOTE (fidelity): reference `mime`/`data: []const u8` — RAW bytes,
/// owned here (`Vec<u8>`, never a lossy String).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub mime: Vec<u8>,
    pub data: Vec<u8>,
}

/// The MIME list of the current drag plus the pre-joined move-event
/// payload ("mime1 mime2 " with a trailing space after every entry,
/// matching kitty) so per-move encoding is allocation-free.
///
/// PORT NOTE (SB): the reference stores `mimes: []const []const u8` as
/// slices INTO its own `payload` allocation — a self-referential borrow in
/// Rust. Ported as byte *ranges* into `payload` (same single allocation,
/// no held borrow), with the slices re-derived on access.
#[derive(Debug)]
pub struct Offered {
    mimes: AVec<core::ops::Range<usize>, Allocator>,
    payload: Bytes,
}

impl Offered {
    fn init(alloc: Allocator, mimes: &[&[u8]]) -> Result<Offered, std::io::Error> {
        // const copies = try alloc.alloc([]const u8, mimes.len);
        // errdefer alloc.free(copies);
        let mut ranges = AVec::new_in(alloc);
        ranges.try_reserve_exact(mimes.len()).map_err(|_| oom())?;

        let mut payload_len: usize = 0;
        for m in mimes {
            payload_len += m.len() + 1;
        }

        // const payload = try alloc.alloc(u8, payload_len);
        // errdefer alloc.free(payload);
        let mut payload = Bytes::new_in(alloc);
        payload.try_reserve_exact(payload_len).map_err(|_| oom())?;
        // Both reserved exactly above: the fills below cannot allocate.
        for m in mimes {
            let offset = payload.len();
            payload.extend_copy(m);
            payload.push(b' ');
            ranges.push(offset..offset + m.len());
        }

        Ok(Offered {
            mimes: ranges,
            payload,
        })
    }

    // fn deinit(self: *const Offered, alloc: Allocator) void
    // PORT NOTE: `deinit` is the implicit Drop of the two Vecs.

    fn eql(&self, mimes: &[&[u8]]) -> bool {
        if self.mimes.len() != mimes.len() {
            return false;
        }
        for (a, b) in self.mimes.iter().zip(mimes) {
            if &self.payload[a.clone()] != *b {
                return false;
            }
        }
        true
    }
}

/// A native drag position report from the embedder.
#[derive(Debug, Clone, Copy)]
pub struct MoveEvent {
    /// Grid cell under the pointer, zero-based from the top-left.
    pub cell_x: u32,
    pub cell_y: u32,

    /// Pointer position in pixels relative to the top-left of the
    /// terminal's content area.
    pub pixel_x: i32,
    pub pixel_y: i32,

    /// The operations the drag source allows.
    pub operations: Operations,
}

impl State {
    /// The maximum number of dropped items. Embedders provide a small
    /// curated set of representations (see dnd.rs), so this is a
    /// generous bound that keeps the MIME list assembly on the stack.
    pub const MAX_ITEMS: usize = 16;

    /// Allocate a fresh state. Done by `handle_command` on registration.
    // fn create(alloc: Allocator) Allocator.Error!*State
    fn create(alloc: Allocator) -> std::io::Result<StateBox> {
        // const state = try alloc.create(State);
        // state.* = .{};
        StateBox::try_new_in(
            State {
                chunking: Default::default(),
                drop: DropTarget::new_in(alloc),
            },
            alloc,
        )
        .map_err(|_| oom())
    }

    /// Free the state and everything it holds.
    // pub fn destroy(self: *State, alloc: Allocator) void
    //
    // PORT NOTE: `deinit` (`freeDragData` and both lists' `deinit(alloc)`)
    // and `alloc.destroy(self)` are the Drop of the box and its containers,
    // which free through the allocator they were created with.
    //
    // PORT NOTE: an associated function taking the box (a method cannot take
    // an allocator_api2 `Box` as `self` on stable Rust).
    pub fn destroy(state: StateBox, alloc: Allocator) {
        state
            .drop
            .registered_mimes
            .allocator()
            .debug_assert_same(alloc);
        drop(state);
    }

    /// Iterate the MIME types the client registered with, in order.
    /// Empty when the client declared none, which is the common case.
    /// The list is only needed to register exotic types with the OS,
    /// such as macOS pasteboard stuff.
    ///
    /// PORT NOTE: `std.mem.tokenizeScalar` skips empty tokens — `split`
    /// + non-empty filter is the faithful equivalent.
    pub fn registered_mimes(&self) -> impl Iterator<Item = &[u8]> {
        self.drop
            .registered_mimes
            .split(|&b| b == b' ')
            .filter(|s| !s.is_empty())
    }

    /// Record one chunk of a registration's MIME list.
    ///
    /// `continuation` is true for every chunk but the first of a chunked
    /// registration. Returns the registration event once the list is complete.
    fn register(
        &mut self,
        alloc: Allocator,
        payload: &[u8],
        continuation: bool,
        more: bool,
    ) -> std::io::Result<Option<Event>> {
        let list = &mut self.drop.registered_mimes;
        list.allocator().debug_assert_same(alloc);
        if !continuation {
            // list.clearRetainingCapacity();
            list.clear();
        }

        // Matching kitty, an over-cap chunk is dropped and does not
        // complete the registration.
        if list.len() + payload.len() > MAX_MIME_LIST_BYTES {
            return Ok(None);
        }
        // try list.appendSlice(alloc, payload);
        list.try_extend_copy(payload).map_err(|_| oom())?;

        Ok(if more {
            None
        } else {
            Some(Event::Registration)
        })
    }

    /// The client's acceptance response for the drag currently over the
    /// terminal, for OS drag feedback. Null when the client hasn't
    /// responded yet (embedders should fall back to their default,
    /// typically copy) or `none` when the client rejected the drag.
    pub fn client_accepted(&self) -> Option<Operation> {
        if self.drop.accept_in_progress {
            return None;
        }
        self.drop.accepted
    }

    /// Free the per-drag data (offered MIME list and held drop items).
    fn free_drag_data(&mut self) {
        // PORT NOTE: the reference's explicit `offered.deinit(alloc)` /
        // per-item frees are the Drop of the taken values.
        if self.drop.offered.is_some() {
            self.drop.offered = None;
        }
        if self.drop.items.is_some() {
            self.drop.items = None;
        }
    }

    /// Clear the per-drag state while preserving the registration,
    /// mirroring kitty's reset_drop. Called when a new drag enters and
    /// when a drop concludes.
    fn reset_drop(&mut self, alloc: Allocator) {
        self.free_drag_data();
        // self.drop.accepted_mimes.clearAndFree(alloc);
        self.drop
            .accepted_mimes
            .allocator()
            .debug_assert_same(alloc);
        self.drop.accepted_mimes = Bytes::new_in(alloc);
        self.drop.hovered = false;
        self.drop.dropped = false;
        self.drop.accepted = None;
        self.drop.accept_in_progress = false;
    }

    /// Handle a t=m acceptance status update from the client, mirroring
    /// kitty's drop_set_status.
    fn accept_status(
        &mut self,
        alloc: Allocator,
        meta: Metadata,
        payload: &[u8],
    ) -> std::io::Result<Option<Event>> {
        let d = &mut self.drop;
        d.accepted_mimes.allocator().debug_assert_same(alloc);
        if !d.accept_in_progress {
            // d.accepted_mimes.clearRetainingCapacity();
            d.accepted_mimes.clear();
            d.accept_in_progress = true;
            d.accepted = Some(Operation::from_protocol(meta.operation));
        }

        if !payload.is_empty() {
            // Matching kitty, an over-cap list stops accumulating and
            // never finalizes, leaving the acceptance unanswered.
            if d.accepted_mimes.len() + payload.len() > MAX_MIME_LIST_BYTES {
                return Ok(None);
            }
            // try d.accepted_mimes.appendSlice(alloc, payload);
            d.accepted_mimes
                .try_extend_copy(payload)
                .map_err(|_| oom())?;
        }

        if meta.more {
            return Ok(None);
        }
        d.accept_in_progress = false;
        if !d.accepted_mimes.is_empty() {
            for c in d.accepted_mimes.iter_mut() {
                if *c == b' ' {
                    *c = 0;
                }
            }
            // try d.accepted_mimes.append(alloc, 0);
            d.accepted_mimes.try_push(0).map_err(|_| oom())?;
        }
        Ok(Some(Event::Acceptance))
    }

    /// Report a native drag moving over the terminal, sending a t=m
    /// move event to the client. `mimes` is the list of MIME types the
    /// terminal can provide for this drag, in the order data request
    /// indices will refer to.
    pub fn drag_move<W: std::io::Write>(
        &mut self,
        alloc: Allocator,
        writer: &mut W,
        ev: MoveEvent,
        mimes: &[&[u8]],
    ) -> std::io::Result<()> {
        self.move_event(alloc, writer, ev, mimes, false)
    }

    /// Report a native drop onto the terminal. The items' data is
    /// copied and held so the client's data requests can be served; it
    /// is freed when the client concludes the drop, a new drag enters,
    /// or the client unregisters.
    ///
    /// Sends a t=M drop event listing the items' MIME types.
    pub fn drag_drop<W: std::io::Write>(
        &mut self,
        alloc: Allocator,
        writer: &mut W,
        ev: MoveEvent,
        items: &[Item],
    ) -> std::io::Result<()> {
        // Copy the items so they can be served after this call returns.
        // Items beyond the cap are dropped so the held list always
        // matches the advertised MIME list.
        let accepted_items = &items[0..items.len().min(Self::MAX_ITEMS)];
        // const copies = try alloc.alloc(Item, accepted_items.len);
        // errdefer alloc.free(copies);
        // errdefer for (copies[0..copied]) |item| { ... };
        //
        // PORT NOTE: the `errdefer`s are the Drop of `copies` (and of each
        // copied `HeldItem`) on the `?` paths, freeing through `alloc`.
        let mut copies: AVec<HeldItem, Allocator> = AVec::new_in(alloc);
        copies
            .try_reserve_exact(accepted_items.len())
            .map_err(|_| oom())?;
        for item in accepted_items {
            // const mime = try alloc.dupe(u8, item.mime);
            // errdefer alloc.free(mime);
            // const data = try alloc.dupe(u8, item.data);
            let mime = dupe_in(alloc, &item.mime)?;
            let data = dupe_in(alloc, &item.data)?;
            // Reserved exactly above: this cannot allocate.
            copies.push(HeldItem { mime, data });
        }

        // The move handling below resets per-drag state when this drop
        // arrives without a preceding move, so the items are attached
        // after it runs. Collect the MIME list first.
        // var mimes_buf: [max_items][]const u8 = undefined;
        let mut mimes_buf: [&[u8]; Self::MAX_ITEMS] = [&[]; Self::MAX_ITEMS];
        let mimes = &mut mimes_buf[0..copies.len()];
        for (m, item) in mimes.iter_mut().zip(copies.iter()) {
            *m = &item.mime;
        }

        self.move_event(alloc, writer, ev, mimes, true)?;

        debug_assert!(self.drop.items.is_none());
        self.drop.items = Some(copies);
        Ok(())
    }

    /// Report the native drag leaving the terminal, sending the t=m
    /// leave event (x=-1, y=-1).
    ///
    /// Ignored after a drop: some toolkits emit a leave notification
    /// for the drop itself, and the held data must survive until the
    /// client concludes.
    pub fn drag_leave<W: std::io::Write>(
        &mut self,
        alloc: Allocator,
        writer: &mut W,
    ) -> std::io::Result<()> {
        self.drop
            .accepted_mimes
            .allocator()
            .debug_assert_same(alloc);
        if self.drop.dropped {
            return Ok(());
        }
        let hovered = self.drop.hovered;
        self.drop.hovered = false;
        if self.drop.offered.is_some() {
            // offered.deinit(alloc);
            self.drop.offered = None;
        }

        // Only a client that saw the drag enter gets the leave event,
        // matching kitty which notifies hovered windows only.
        if !hovered {
            return Ok(());
        }

        response::encode(
            writer,
            b"t=m:x=-1:y=-1",
            self.drop.client_id,
            b"",
            response::Encoding::Plain,
            osc::Terminator::St,
        )
    }

    /// Shared implementation of move and drop events, mirroring kitty's
    /// drop_move_on_child.
    fn move_event<W: std::io::Write>(
        &mut self,
        alloc: Allocator,
        writer: &mut W,
        ev: MoveEvent,
        mimes: &[&[u8]],
        is_drop: bool,
    ) -> std::io::Result<()> {
        if !self.drop.hovered {
            self.reset_drop(alloc);
            self.drop.hovered = true;
        }
        if is_drop {
            self.drop.dropped = true;
            self.drop.hovered = false;
        }

        // (Re)build the offered MIME list when it changed.
        let rebuild = match &self.drop.offered {
            None => true,
            Some(offered) => !offered.eql(mimes),
        };
        if rebuild {
            self.drop.offered = None;
            // self.drop.offered = try Offered.init(alloc, mimes);
            self.drop.offered = Some(Offered::init(alloc, mimes)?);
        }

        // var header_buf: [96]u8 = undefined;
        // const header = std.fmt.bufPrint(&header_buf, ...) catch unreachable;
        let header = format!(
            "t={}:x={}:y={}:X={}:Y={}:o={}",
            if is_drop { 'M' } else { 'm' },
            ev.cell_x,
            ev.cell_y,
            ev.pixel_x,
            ev.pixel_y,
            ev.operations.protocol_value(),
        );

        // The MIME list is sent with every move event, matching kitty
        // (the spec suggests only the first, but kitty always sends it
        // and clients depend on that).
        response::encode(
            writer,
            header.as_bytes(),
            self.drop.client_id,
            &self.drop.offered.as_ref().unwrap().payload,
            response::Encoding::Plain,
            osc::Terminator::St,
        )
    }
}

/// `alloc.dupe(u8, src)` into the state's allocator, `error.OutOfMemory` as
/// [`oom`].
fn dupe_in(alloc: Allocator, src: &[u8]) -> std::io::Result<Bytes> {
    let mut v = Bytes::new_in(alloc);
    v.try_extend_copy(src).map_err(|_| oom())?;
    Ok(v)
}
