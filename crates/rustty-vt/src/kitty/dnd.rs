//! Kitty drag and drop protocol (OSC 72).
//!
//! Specification: https://sw.kovidgoyal.net/kitty/dnd-protocol/
//!
//! The protocol lets a program running in the terminal participate in
//! native OS drag and drop. A client registers to accept drops (t=a);
//! the terminal then forwards native drag movement (t=m) and drops
//! (t=M) to it and serves the dropped data on request (t=r), instead
//! of the traditional behavior of pasting dropped paths or text.
//!
//! ## Divergences
//!
//! These will be fixed in the future:
//!
//!   * Dropped data is captured eagerly at drop time from a curated
//!     set of representations the embedder can serve (typically
//!     text/uri-list and text/plain), rather than fetched from the OS
//!     on demand. Consequently the native drag session concludes at
//!     drop time and the client's concluding operation (t=r with
//!     x=y=Y=0) only frees the held data, and kitty's 128-entry
//!     request queue and EMFILE overflow handling are unnecessary
//!     because requests are served synchronously in order.
//!   * Every client is treated as local: machine IDs (t=a:x=1) are
//!     accepted and ignored, responses never carry the X=1 remote
//!     marker, and remote file transfer requests (t=r with y or Y
//!     keys) are answered with EINVAL. A remote client (e.g. over
//!     ssh) can still receive text drops; only file-content transfer
//!     is unavailable.
//!   * The terminal never initiates drags (drag out): enabling and
//!     disabling offers (t=o:x=1, t=o:x=2) are accepted and ignored,
//!     and since the terminal never sends a drag start request a
//!     conforming client never offers a drag. Direct offers (t=o:x=0)
//!     and drag data/start commands (t=p, t=P) are refused with EPERM.
//!
//! These are on purpose forever:
//!
//!   * Responses echo the requesting command's terminator (ST or BEL)
//!     per ghostty convention; kitty always uses ST. Terminal-
//!     initiated events always use ST.
//!
//! rustty-vt kitty/dnd
//!
//! PORTS: ghostty src/terminal/kitty/dnd.zig
//!
//! PORT STATUS: ported (phase-5 B6 end state at 683d8db64: 7c845e8af +
//! 38746b8c1 + 50f69b883, doc supersede db2f8be59 folded in)

// const dnd_command = @import("dnd_command.zig");
// const dnd_response = @import("dnd_response.zig");
// const dnd_drop = @import("dnd_drop.zig");
// PORT NOTE: the submodules are declared in kitty/mod.rs (crate module
// tree); this aggregator re-exports the same surface the reference does.
use crate::kitty::dnd_command;
use crate::kitty::dnd_drop;
use crate::kitty::dnd_response;

pub use dnd_command::Chunking;
pub use dnd_command::EventType;
pub use dnd_command::Metadata;
pub use dnd_command::Operation;
pub use dnd_command::Operations;
pub use dnd_command::Request;

pub use dnd_response::encode;
pub use dnd_response::encode_error;
pub use dnd_response::Errno;
pub use dnd_response::RequestKeys;

pub use dnd_drop::State;
pub use dnd_drop::StateBox;
// pub const Item = dnd_drop.State.Item;
// pub const MoveEvent = dnd_drop.State.MoveEvent;
// PORT NOTE: `Item`/`MoveEvent` are nested in `State` upstream; hoisted to
// module level in dnd_drop.rs (Rust has no nested type namespaces).
pub use dnd_drop::Item;
pub use dnd_drop::MoveEvent;
// pub const max_mime_list_bytes = dnd_drop.max_mime_list_bytes;
pub use dnd_drop::handle_command;
pub use dnd_drop::Event;
pub use dnd_drop::MAX_MIME_LIST_BYTES;

// test {
//     _ = dnd_command;
//     _ = dnd_response;
//     _ = dnd_drop;
//     _ = @import("dnd_test.zig");
// }
// PORT NOTE: the submodules' tests are compiled via their `mod` declarations
// in kitty/mod.rs; `dnd_test.zig` is `kitty/dnd_test.rs`, declared there as
// `#[cfg(test)] mod dnd_test;`. The re-export block above forces every
// aggregated symbol to resolve, mirroring the reference `test` block's
// analysis guarantee.
