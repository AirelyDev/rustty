//! Kitty clipboard protocol (OSC 5522).
//!
//! This implements the protocol semantics on top of the raw OSC capture:
//! src/terminal/osc/parsers/kitty_clipboard_protocol.zig:
//!
//! The behavior here is modeled on the kitty reference implementation
//! (kitty/clipboard.py) rather than only the prose spec, since the two
//! disagree in places. Notable reference behaviors we reproduce:
//!
//!   * Malformed metadata (any record without '=', including an empty
//!     metadata section) and an unknown or missing `type` silently drop
//!     the request with no response.
//!   * All base64 (payloads and the `mime`, `name`, and `pw` metadata
//!     values) is strict RFC 4648 per the spec's "Encoding of payloads"
//!     section: characters outside the standard alphabet (including
//!     whitespace) and incorrect padding are rejected, never silently
//!     skipped. An invalid value on `wdata` or `walias` aborts an
//!     in-flight write with EINVAL; an invalid `read` is dropped with
//!     no response since reads have no error status.
//!   * Decoded metadata and MIME-list payloads must be valid UTF-8. An
//!     invalid value on `wdata` or `walias`, or a `walias` without a
//!     target MIME type, aborts an in-flight write with EINVAL.
//!   * The `wdata` payloads for one MIME type form a single base64
//!     stream split at arbitrary packet boundaries; only the
//!     concatenation must be correctly padded. Like kitty (which
//!     resets its streaming decoder on EOF), a packet ending exactly
//!     at terminal padding restarts the stream, so independently
//!     encoded chunks also work.
//!   * `mime`, `name`, and `pw` metadata values are base64-encoded UTF-8;
//!     everything else is verbatim. Unknown keys are ignored.
//!   * `id` is sanitized by stripping characters outside [a-zA-Z0-9-_+.]
//!     and truncating to 512 bytes, then echoed verbatim in every
//!     response packet (omitted when empty).
//!   * A `type=write` silently replaces any in-flight transaction. A
//!     commit (`type=wdata` without a MIME type) with no in-flight
//!     transaction is silently ignored.
//!   * Responses never send a payload section for an empty payload,
//!     except the targets ('.') listing DATA packet which is always sent.
//!
//! Specification: https://sw.kovidgoyal.net/kitty/clipboard/
//!
//! PORTS: ghostty src/terminal/kitty/clipboard.zig (B5 end state)
//!
//! PORT NOTE (dispatch seam): the OSC 5522 stream dispatch, the
//! `stream_terminal.zig` handler (`kittyClipboard*`), the `clipboard_read`
//! effect, and the Surface/termio halves land with the Dispatch stage; this
//! module is the complete VT-side state machinery they consume.

// PORT NOTE: the reference imports are
//   const oscpkg = @import("../osc.zig");
//   const protocol = @import("../osc/parsers/kitty_clipboard_protocol.zig");
//   const command = @import("clipboard_command.zig");
//   const write = @import("clipboard_write.zig");
//   const response = @import("clipboard_response.zig");
//   const grants = @import("clipboard_grants.zig");
use crate::kitty::clipboard_command as command;
use crate::kitty::clipboard_grants as grants;
use crate::kitty::clipboard_response as response;
use crate::kitty::clipboard_write as write;
use crate::osc as oscpkg;
use crate::osc::parsers::kitty_clipboard_protocol as protocol;

// pub const OSC = protocol.OSC;
pub use protocol::OSC;
// pub const Operation = protocol.Operation;
pub use protocol::Operation;
// pub const Status = protocol.Status;
pub use protocol::Status;
// pub const Terminator = oscpkg.Terminator;
pub use oscpkg::Terminator;

// pub const Metadata = command.Metadata;
pub use command::Metadata;
// pub const Payload = command.Payload;
pub use command::Payload;
// pub const readPromptExempt = command.readPromptExempt;
pub use command::read_prompt_exempt;
// pub const max_id_len = command.max_id_len;
pub use command::max_id_len;
// pub const max_pw_len = command.max_pw_len;
pub use command::max_pw_len;
// pub const max_mime_len = command.max_mime_len;
pub use command::max_mime_len;
// pub const max_name_len = command.max_name_len;
pub use command::max_name_len;
// (rustty-only rider: `Metadata.parse`'s error type, flattened to module
// scope by the port; the reference reaches it as an inline error set.)
pub use command::ParseError;

// pub const Content = write.Content;
pub use write::Content;
// pub const WriteState = write.WriteState;
pub use write::WriteState;
// pub const max_write_size = write.max_write_size;
pub use write::max_write_size;
// pub const max_write_mimes = write.max_write_mimes;
pub use write::max_write_mimes;
// pub const max_write_aliases = write.max_write_aliases;
pub use write::max_write_aliases;
// (rustty-only riders: the write error sets and nested types, flattened to
// module scope by the port — the reference reaches them as inline error sets
// and `WriteState.Options`/`WriteState.Committed`.)
pub use write::{AliasError, CommitError, Committed, DataError};
pub use write::Options as WriteOptions;

// pub const Response = response.Response;
pub use response::Response;
// pub const ReadSuccess = response.ReadSuccess;
pub use response::ReadSuccess;
// pub const PasteEvent = response.PasteEvent;
pub use response::PasteEvent;
// pub const read_chunk_size = response.read_chunk_size;
pub use response::read_chunk_size;
// pub const max_read_mimes = response.max_read_mimes;
pub use response::max_read_mimes;
// pub const max_listing_mimes = response.max_listing_mimes;
pub use response::max_listing_mimes;
// pub const targets_mime = response.targets_mime;
pub use response::targets_mime;

// pub const Grants = grants.Grants;
pub use grants::Grants;
// pub const otp_len = grants.otp_len;
pub use grants::otp_len;
// pub const generateOtp = grants.generateOtp;
pub use grants::generate_otp;
// (rustty-only rider: the reference reaches this as `Grants.Direction`;
// flattened to module scope by the port.)
pub use grants::Direction;

// test {
//     @import("std").testing.refAllDecls(@This());
// }
// PORT NOTE: the reference's aggregator test is a `refAllDecls`
// compile-coverage touch; mirrored concretely below (kitty/mod.rs precedent)
// so a removed/renamed re-export surfaces as a test failure.
#[cfg(test)]
mod tests {
    #[test]
    fn ref_all_decls() {
        let _ = core::any::type_name::<super::OSC>();
        let _ = core::any::type_name::<super::Operation>();
        let _ = core::any::type_name::<super::Status>();
        let _ = core::any::type_name::<super::Terminator>();
        let _ = core::any::type_name::<super::Metadata>();
        let _ = core::any::type_name::<super::Payload>();
        let _ = super::read_prompt_exempt as fn(usize) -> bool;
        let _ = (
            super::max_id_len,
            super::max_pw_len,
            super::max_mime_len,
            super::max_name_len,
        );
        let _ = core::any::type_name::<super::Content<'_>>();
        let _ = core::any::type_name::<super::WriteState>();
        let _ = (
            super::max_write_size,
            super::max_write_mimes,
            super::max_write_aliases,
        );
        let _ = core::any::type_name::<super::Response<'_>>();
        let _ = core::any::type_name::<super::ReadSuccess<'_>>();
        let _ = core::any::type_name::<super::PasteEvent<'_>>();
        let _ = (
            super::read_chunk_size,
            super::max_read_mimes,
            super::max_listing_mimes,
            super::targets_mime,
        );
        let _ = core::any::type_name::<super::Grants>();
        let _ = super::otp_len;
        let _ = super::generate_otp
            as fn(crate::sys::Io) -> Result<[u8; super::otp_len], crate::sys::RandomSecureError>;
    }
}
