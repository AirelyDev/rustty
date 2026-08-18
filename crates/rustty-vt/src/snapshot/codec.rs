//! Complete terminal snapshot encoding and restoration.
//!
//! PORTS: ghostty src/terminal/snapshot/snapshot.zig (imported by the
//! reference `main.zig` under the local name `codec`, hence this file name).
//!
//! PORT STATUS: partial (phase-4 wsnap-1) — `Continuation`, `EncodeOptions`,
//! and `DecodeOptions` are live. The codec drivers are BLOCKED on the
//! next-stage modules `snapshot/{grid,screen,terminal,history}.rs`
//! (assessment B clusters C3/C4) and land with them, END STATE unchanged:
//!
//!   * `EncodeError` / `DecodeError` / `DecodeExactError` (unions over the
//!     terminal/screen/history/checkpoint/continuation record error sets)
//!   * `encode` — envelope, TERMINAL, SCREEN×n (+PAGEs), CONTINUATION,
//!     READY, HISTORY×n (+PAGEs, newest-to-oldest), FINISH; buffers only one
//!     record payload at a time (f0fe788fc)
//!   * `Decoded` — the restored `Terminal` + owned continuation +
//!     per-screen advisory `history_rows` (`std.EnumMap(ScreenSet.Key, u64)`),
//!     with `deinit` / `toOwned`
//!   * `Decoder` — the incremental decoder, END STATE `init` → `ready` →
//!     `next` (e37865bed; state machine start/history/finished/failed with
//!     per-screen generation + width drop rules, `Progress {key, rows,
//!     remaining}`, strict wire validation with forgiving application;
//!     invalid-state reporting of cbc9f360b/7d9aaa297/58e92098a)
//!   * `decode` — the one-shot form over `Decoder`, stopping at FINISH and
//!     leaving trailing transport bytes unread (d37e1fe18: the format does
//!     NOT require EOF)
//!   * `decodeExact` — `decode` plus the strict trailing-data check
//!     (`error.TrailingData`) for bounded files and buffers
//!   * NO BLAKE3 anywhere (removed by 219173ab3)
//!   * the reference `io` parameter (`std.Io`) threaded through
//!     `decode`/`ready` — its Rust seam arrives with area C's TinyIo analogue
//!   * deferred upstream tests (assertion-for-assertion with C3/C4):
//!     "complete snapshot round trip with history and alternate screen",
//!     "complete snapshot restores a canonical Stream continuation",
//!     "complete snapshot validates continuation before writing",
//!     "complete snapshot waits for continuation tracking recovery",
//!     "complete snapshot preserves every supported continuation cut",
//!     "complete snapshot preserves Kitty virtual placeholders",
//!     "complete snapshot encoding streams from the current writer position",
//!     "complete snapshot rejects ordering and invalid markers",
//!     plus the incremental-decoder and robustness tests of
//!     e37865bed/cbc9f360b/7d9aaa297/58e92098a/f8ac0ca98 and the
//!     decode/decodeExact split tests of d37e1fe18
//!
//! Its golden (`complete-v1.hex`) is already checked in under `testdata/`.

// const std = @import("std");
// const build_options = @import("terminal_options");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const checkpoint = @import("checkpoint.zig");
// const continuation = @import("continuation.zig");
// const envelope = @import("envelope.zig");
// const test_fixture = @import("fixture.zig");
// const history = @import("history.zig"); — NEXT STAGE
// const page = @import("page.zig");
// const record = @import("record.zig");
// const screen = @import("screen.zig"); — NEXT STAGE
// const size = @import("../size.zig");
// const terminal = @import("terminal.zig"); — NEXT STAGE
// const Terminal = @import("../Terminal.zig");
// const TerminalStream = @import("../stream_terminal.zig").Stream;
// const terminal_kitty = @import("../kitty.zig");
// const TerminalPageList = @import("../PageList.zig");
// const TerminalScreen = @import("../Screen.zig");
// const TerminalScreenKey = @import("../ScreenSet.zig").Key;
use super::continuation;

/// Re-export continuation to make it a bit more ergonomic to reference.
// pub const Continuation = continuation.Value;
pub type Continuation<'a> = continuation::Value<'a>;

pub struct EncodeOptions<'a> {
    pub continuation: Continuation<'a>,
}

pub struct DecodeOptions {
    /// Largest non-ground continuation the decoder may allocate and return.
    /// Set this to zero when only ground-state snapshots are acceptable.
    pub max_continuation_bytes: usize,
}
