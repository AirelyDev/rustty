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
//! PORT STATUS: complete (phase-4 wsnap-2 + wsnap-3, assessment B clusters
//! C1–C4) — the scaffold (envelope, io, record framing, checkpoints,
//! continuation), the page/style/hyperlink record codecs, the COMPLETE grid
//! codec (cluster C3: 8-byte cells with blank elision, per-row variable cell
//! width, bulk encode/decode, batched grapheme suffixes, per-row wide-pair
//! resolution), the framed PAGE encode/decode/Decoder, the HISTORY record,
//! the SCREEN and TERMINAL records, and the codec drivers in `codec.rs`
//! (encode/decode/decodeExact + the incremental Decoder) are all live.

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
pub mod screen;
// pub const style = @import("style.zig");
pub mod style;
// pub const terminal = @import("terminal.zig");
pub mod terminal;

// const codec = @import("snapshot.zig");
mod codec;
// pub const EncodeError = codec.EncodeError;
pub use codec::EncodeError;
// pub const DecodeError = codec.DecodeError;
pub use codec::DecodeError;
// pub const DecodeExactError = codec.DecodeExactError;
pub use codec::Continuation;
pub use codec::DecodeExactError;
pub use codec::DecodeOptions;
pub use codec::EncodeOptions;
// pub const Decoded = codec.Decoded;
pub use codec::Decoded;
// pub const Decoder = codec.Decoder;
pub use codec::Decoder;
// PORT NOTE: `Decoder.Progress` is a nested public type in the reference; the
// port hoisted it to the module (see codec.rs), so it is re-exported here for
// the C-API tier. `HistoryRows` is the `Decoded.history_rows` field type (the
// reference's public-std `std.EnumMap(TerminalScreenKey, u64)`).
pub use codec::HistoryRows;
pub use codec::Progress;
// pub const encode = codec.encode;
pub use codec::encode;
// pub const decode = codec.decode;
pub use codec::decode;
// pub const decodeExact = codec.decodeExact;
pub use codec::decode_exact;

// PORT NOTE: `io.zig` and `fixture.zig` are internal siblings (main.zig does
// not re-export them); `io` also carries the crate's `std.Io` stand-in (see
// its PORT NOTE). It is PUBLIC (unlike the reference sibling) because the
// stand-in traits appear in the public codec signatures (`&mut dyn
// io::Reader` / `&mut dyn io::Writer` — the reference's `*std.Io.Reader` /
// `*std.Io.Writer`, which ARE public Zig std types): the C-API tier
// (rustty-vt-c Bundle S) implements them over the `RusttyReader` /
// `RusttyWriter` callback pairs.
#[cfg(test)]
pub(crate) mod fixture;
pub mod io;

// test { @import("std").testing.refAllDecls(@This()); }
// PORT NOTE: no analogue needed — Rust compiles and lints every declared
// module unconditionally.

/// PORT NOTE: tripwire fail-point module standing in for the reference
/// tests' `std.testing.FailingAllocator` (the crate's established
/// failure-injection seam; see `stream_continuation.rs`'s `TRACKER_TW`
/// PORT NOTE). One point, `Alloc`, sits at the subsystem's fallible
/// decode allocations. Unlike `FailingAllocator`, which sees every
/// allocation, this seam is OPT-IN: the instrumented sites are
///
///   * `continuation::decode_alloc` — owned continuation bytes;
///   * `hyperlink::decode_alloc` — owned hyperlink bytes;
///   * `hyperlink::decode_page_string` — page string-allocator strings;
///   * `terminal::decode_payload` — tab stops, pwd and title;
///   * `terminal::decode` — `Terminal.init`, the alternate screen,
///     `set_pwd` and `set_title`;
///   * `screen::decode` — the PageList builder, each decoded page, and
///     the cursor's tracked pin;
///   * `history::decode_page` — each history page.
///
/// Twelve `tw::check` sites in all.
///
/// The transactionality sweep in `codec.rs` derives its bound from a
/// baseline run's reach count and carries a FLOOR on that count. A site is
/// protected by that floor only if one of the sweep's FIXTURES REACHES IT —
/// the floor counts reaches, so a site nothing reaches contributes nothing to
/// count and its loss is invisible. That is not a theoretical caveat: when
/// this note first claimed, unqualified, that "dropping a site from this list
/// turns that test red", `hyperlink::decode_page_string` was reached by
/// neither fixture, and deleting its check left the WHOLE CRATE SUITE green
/// (3039 passed, 0 failed). The sweep's third fixture — a terminal carrying an
/// explicit hyperlink — exists to reach it.
///
/// **Verified, not asserted**: each of the twelve sites was disabled
/// individually and the crate suite re-run; all twelve turn it red. Redo that
/// sweep if you add a site, and add a fixture that reaches it if none does.
///
/// **The floor is the only thing in that sweep doing the work.** Of the three
/// assertions the test carries, `allocation_count > 0` is subsumed by the floor
/// (the floor is a larger lower bound), and the per-index "the fail point must
/// actually have TRIPPED" assertion catches no mutation on its own — it exists
/// to stop the bound drifting above reality, which is a different failure from
/// a site going missing. If you weaken or remove the floor, this seam stops
/// being checked; everything else is scaffolding around it.
///
/// **Reach is per-FIXTURE and badly asymmetric, so "a site is in the list" and
/// "a site is protected" are different facts.** Measured, by neutralising every
/// other site and reading the baseline counts back:
///
///   * `terminal::decode_alloc_check` is reached THREE times by every fixture
///     (tab stops, pwd, title). The complete golden's total before this seam was
///     widened was exactly 3 — i.e. that one site was the ONLY one the first
///     fixture reached, and it alone was load-bearing.
///   * `continuation::decode_alloc` was reached only by the continuation
///     fixture (total 4 = 3 + 1).
///   * `hyperlink::decode_alloc` was reached by NEITHER. Under the old
///     two-fixture sweep it was held up entirely by `hyperlink.rs`'s own
///     `decode_allocation_failure` test.
///
/// So the pair that could be deleted in silence was
/// `continuation::decode_alloc` + `hyperlink::decode_alloc`, and the loss was
/// never order-independent: which sites are losable depends on which fixture
/// reaches what, not on how many sites there are. The third fixture (the
/// explicit-hyperlink terminal) is what makes all twelve floor-protected today
/// — both hyperlink sites fail the floor now, not just their own test.
///
/// What the floor still cannot see, and neither can the reference's own test:
/// a site that is still REACHED but no longer PROPAGATES the failure. The
/// count is unchanged, and `saw_out_of_memory` is satisfied by any other
/// site. Asserting `Err` at every index would catch it here, but the reference
/// deliberately allows `Ok` (`hyperlink::decode_page_string`'s own caller
/// DEGRADES to `Ok(0)` rather than propagating), so that assertion would be a
/// divergence, not a strengthening.
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

    thread_local! {
        /// Number of times any fail point in this module has been REACHED
        /// since the last `reset_reached`, armed or not.
        ///
        /// PORT NOTE: stands in for `std.testing.FailingAllocator.alloc_index`,
        /// which the reference's transactionality test reads to learn how many
        /// allocations a clean decode performs before sweeping them. The
        /// tripwire module itself keeps a per-point `reached` counter but does
        /// not expose it, and it is a ported reference file — so the counter
        /// lives here, in this crate-local stand-in, rather than diverging
        /// `rustty-util::tripwire` from its oracle.
        pub(crate) static SNAPSHOT_TW_REACHED: core::cell::Cell<usize> =
            const { core::cell::Cell::new(0) };
    }

    /// Reset the reach counter and return its previous value.
    pub(crate) fn take_reached() -> usize {
        SNAPSHOT_TW_REACHED.with(|c| c.replace(0))
    }

    #[inline]
    pub(crate) fn check(point: SnapshotFp) -> Result<(), OutOfMemory> {
        SNAPSHOT_TW_REACHED.with(|c| c.set(c.get() + 1));
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
