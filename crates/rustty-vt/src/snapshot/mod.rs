//! Terminal snapshot binary representation and codecs.
//!
//! This is NOT a full transport-ready format to implement generic replay
//! software such as multiplexers, recorders (e.g. asciinema), etc. The goal
//! of this package is to provide a documented, binary-compatible representation
//! for a terminal state.
//!
//! We call this a "snapshot." The snapshot is purposely laid out in a way
//! that prioritizes making a terminal functional as quickly as possible.
//! To do that, it sends the active terminal state followed by a READY record,
//! then complete history.
//!
//! READY denotes that enough state has been sent down to render the
//! terminal and reconstruct its unfinished VT Stream state.
//!
//! After READY, we send history pages (scrollback). Finally, the
//! snapshot ends with an empty FINISH marker.
//!
//! ## Snapshot Format
//!
//! This documents snapshot format 1. Version 1 is the work-in-progress
//! format that we intended to continue to break until we can promise
//! binary compatibility.
//!
//! A snapshot is one envelope followed by a sequence of records. The envelope
//! occurs once at byte zero. Every record is independently framed as a fixed
//! header followed by the number of payload bytes declared by that header.
//!
//! ```text
//! +------------------+
//! | Envelope         |
//! +------------------+
//! | Record 1 header  |
//! +------------------+
//! | Record 1 payload |
//! +------------------+
//! | Record 2 header  |
//! +------------------+
//! | Record 2 payload |
//! +------------------+
//! | ...              |
//! +------------------+
//! ```
//!
//! Record groups have a strict order:
//!
//! ```text
//! +----------------------------------------+
//! | TERMINAL                               |
//! +----------------------------------------+
//! | SCREEN * terminal.screen_count         |
//! | PAGE * each screen.page_count          |
//! +----------------------------------------+
//! | CONTINUATION                           |
//! +----------------------------------------+
//! | READY                                  |
//! +----------------------------------------+
//! | HISTORY * terminal.screen_count        |
//! | PAGE * each history.page_count         |
//! +----------------------------------------+
//! | FINISH                                 |
//! +----------------------------------------+
//! ```
//!
//! The SCREEN and HISTORY sequence groups may each appear in any key order.
//! Each key must occur exactly once in each group and must identify one of the
//! screens declared by TERMINAL. SCREEN contains the complete pages needed to
//! restore each active area. HISTORY contains the older complete pages for its
//! screen in newest-to-oldest order so they can be prepended as they arrive.
//! Every SCREEN has one corresponding HISTORY, even when its history page count
//! is zero. FINISH terminates the snapshot. Bytes after FINISH belong to the
//! containing transport and are not consumed by snapshot decoding.
//!
//! CONTINUATION contains the bytes required to bring the terminal's
//! VT parser/stream up to the same state, or no bytes if it should be
//! in the ground state.
//!
//! READY and FINISH are empty marker records. READY separates the renderable
//! active state and continuation from history; FINISH terminates the complete
//! snapshot. Every record, including both markers, is independently protected
//! by CRC32C. Each SCREEN declares its complete logical history extent, allowing
//! a client to size its scrollbar at READY even though older PAGE records arrive
//! afterward.
//!
//! ## Encoding
//!
//! Encode a complete snapshot into any writer:
//!
//! ```zig
//! var output: std.Io.Writer.Allocating = .init(alloc);
//! defer output.deinit();
//!
//! try snapshot.encode(alloc, &output.writer, &terminal, .{
//!     .continuation = .ground,
//! });
//!
//! const bytes = output.written();
//! ```
//!
//! Encoding begins at the writer's current position, so unrelated bytes may
//! precede the snapshot. The encoder buffers only the current record payload
//! to calculate its length and CRC32C; completed records stream immediately.
//! Buffering is an encoder implementation detail, not a requirement of the
//! wire format.
//!
//! A failure may leave prior complete records, or a partial record if the
//! destination itself fails. Such a prefix has no FINISH marker and
//! cannot be restored as a complete snapshot.
//!
//! Each record type usually exposes an `encode` function that encodes
//! a complete record, such as `screen.encode`.
//!
//! ## Decoding
//!
//! `snapshot.decode` consumes exactly one snapshot through FINISH and leaves
//! any following bytes unread. This permits multiple snapshots or live protocol
//! data to share a stream without waiting for the peer to close it. Transports
//! that deliver live PTY data before history finishes must multiplex that data
//! outside this ordered snapshot record sequence.
//!
//! ```zig
//! var decoded = try snapshot.decode(alloc, io, &reader, .{
//!     .max_continuation_bytes = 1024 * 1024,
//! });
//! defer decoded.deinit(alloc);
//! var terminal = decoded.toOwned();
//! defer terminal.deinit(alloc);
//! ```
//!
//! Use `snapshot.decodeExact` for a bounded file or buffer that must contain
//! only one snapshot. It preserves the stricter end-of-file check, which may
//! block when used with a live stream.
//!
//! `snapshot.Decoder` decodes the same stream incrementally so the terminal
//! becomes usable at READY, before history has arrived. Each `next` call
//! applies one history page to the by-then live terminal and returning null
//! validates FINISH:
//!
//! ```zig
//! var decoder: snapshot.Decoder = .init(&reader);
//! var decoded = try decoder.ready(alloc, io, .{
//!     .max_continuation_bytes = 1024 * 1024,
//! });
//! defer decoded.deinit(alloc);
//!
//! var terminal = decoded.toOwned();
//! defer terminal.deinit(alloc);
//!
//! // Render, replay the continuation, process input...
//! while (try decoder.next(alloc, &terminal)) |progress| {
//!     _ = progress; // Scrollback grew.
//! }
//! ```
//!
//! PORTS: ghostty src/terminal/snapshot/main.zig (the Zig code examples above
//! document the reference API; the port's live surface is the module list
//! below).
//!
//! PORT STATUS: partial (phase-4 wsnap-2, assessment B clusters C1+C2+C3 and
//! the history half of C4) — the scaffold (envelope, io, record framing,
//! checkpoints, continuation), the page/style/hyperlink record codecs, the
//! COMPLETE grid codec (cluster C3: 8-byte cells with blank elision, per-row
//! variable cell width, bulk encode/decode, batched grapheme suffixes,
//! per-row wide-pair resolution), the complete framed PAGE
//! encode/decode/Decoder, and the HISTORY record are live. `screen` and
//! `terminal` (the rest of cluster C4) plus the codec drivers in `codec.rs`
//! are the next stage; their files drop in beside these modules (see the
//! commented re-exports below and the PORT STATUS notes in `history.rs` /
//! `codec.rs`).

// pub const checkpoint = @import("checkpoint.zig");
pub mod checkpoint;
// pub const continuation = @import("continuation.zig");
pub mod continuation;
// pub const envelope = @import("envelope.zig");
pub mod envelope;
// pub const grid = @import("grid.zig");
pub mod grid;
// pub const history = @import("history.zig");
pub mod history;
// pub const hyperlink = @import("hyperlink.zig");
pub mod hyperlink;
// pub const page = @import("page.zig");
pub mod page;
// pub const record = @import("record.zig");
pub mod record;
// pub const screen = @import("screen.zig");
// PORT NOTE (wsnap-2 boundary): `snapshot/screen.rs` is next-stage (cluster
// C4; goldens `testdata/screen-header-v1.hex` /
// `testdata/screen-saved-cursor-v1.hex` are already checked in). It also
// unblocks the deferred history test (see history.rs PORT STATUS).
// pub const style = @import("style.zig");
pub mod style;
// pub const terminal = @import("terminal.zig");
// PORT NOTE (wsnap-2 boundary): `snapshot/terminal.rs` is next-stage (cluster
// C4; golden `testdata/terminal-header-v1.hex` is already checked in).

// const codec = @import("snapshot.zig");
mod codec;
// pub const EncodeError = codec.EncodeError; — next-stage (see codec.rs)
// pub const DecodeError = codec.DecodeError; — next-stage (see codec.rs)
// pub const DecodeExactError = codec.DecodeExactError; — next-stage (see codec.rs)
pub use codec::Continuation;
pub use codec::DecodeOptions;
pub use codec::EncodeOptions;
// pub const Decoded = codec.Decoded; — next-stage (see codec.rs)
// pub const Decoder = codec.Decoder; — next-stage (see codec.rs)
// pub const encode = codec.encode; — next-stage (see codec.rs)
// pub const decode = codec.decode; — next-stage (see codec.rs)
// pub const decodeExact = codec.decodeExact; — next-stage (see codec.rs)

// PORT NOTE: `io.zig` and `fixture.zig` are internal siblings (main.zig does
// not re-export them); `io` also carries the crate's `std.Io` stand-in (see
// its PORT NOTE) and is crate-visible so the next-stage modules and the
// benchmark/C-API tiers can reuse it.
pub(crate) mod io;
#[cfg(test)]
pub(crate) mod fixture;

// test { @import("std").testing.refAllDecls(@This()); }
// PORT NOTE: no analogue needed — Rust compiles and lints every declared
// module unconditionally.

/// PORT NOTE: tripwire fail-point module standing in for the reference
/// tests' `std.testing.FailingAllocator` (the crate's established
/// failure-injection seam; see `stream_continuation.rs`'s `TRACKER_TW`
/// PORT NOTE). One point, `Alloc`, sits at the subsystem's fallible
/// decode allocations (`continuation::decode`, `hyperlink::decode`).
#[cfg(test)]
pub(crate) mod tw {
    use core::cell::RefCell;

    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    pub(crate) enum SnapshotFp {
        Alloc,
    }

    /// The tripwire's error value (Zig `Allocator.Error`).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct OutOfMemory;

    thread_local! {
        pub(crate) static SNAPSHOT_TW: RefCell<
            rustty_util::tripwire::Module<SnapshotFp, OutOfMemory>,
        > = RefCell::new(rustty_util::tripwire::module());
    }

    #[inline]
    pub(crate) fn check(point: SnapshotFp) -> Result<(), OutOfMemory> {
        SNAPSHOT_TW.with(|tw| tw.borrow_mut().check(point))
    }

    /// Arm the allocation fail point (`FailingAllocator{ .fail_index = 0 }`).
    pub(crate) fn error_always(point: SnapshotFp) {
        SNAPSHOT_TW.with(|tw| tw.borrow_mut().errorAlways(point, OutOfMemory));
    }

    /// Arm the allocation fail point after `min` successful checks
    /// (`FailingAllocator{ .fail_index = min }`).
    pub(crate) fn error_after(point: SnapshotFp, min: usize) {
        SNAPSHOT_TW.with(|tw| tw.borrow_mut().errorAfter(point, OutOfMemory, min));
    }

    /// Disarm the fail point; errors with `UntrippedError` if it never fired.
    pub(crate) fn end(
        mode: rustty_util::tripwire::ResetMode,
    ) -> Result<(), rustty_util::tripwire::UntrippedError> {
        SNAPSHOT_TW.with(|tw| tw.borrow_mut().end(mode))
    }
}
