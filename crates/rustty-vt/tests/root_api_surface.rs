//! Root VT-module public API surface — the `pub const` re-exports of
//! `ghostty src/terminal/main.zig` (forwarded by `src/lib_vt.zig`) as seen by an
//! EXTERNAL consumer of the crate.
//!
//! This is an INTEGRATION test on purpose: it can only name what `rustty_vt`
//! actually exports at its root, so it fails to compile if a root alias
//! regresses to a comment-only line. The crate's own unit tests could reach the
//! same types through `crate::module::Type` and would not notice.
//!
//! Covers `main.zig:58-59`:
//!   pub const TerminalStream = stream_terminal.Stream;
//!   pub const Stream = stream.Stream;
//!
//! `main.zig:60`'s `pub const StreamAction = stream.Action` has no counterpart:
//! the reference `Action` tagged union is modeled as the `stream::Handler` trait
//! plus payload structs (with the C-ABI union mirror in `rustty-vt-c`), so there
//! is no `stream::Action` item to alias. See the PORT NOTE in `lib.rs`.

/// `TerminalStream` must BE `stream_terminal::Stream`, not merely a lookalike:
/// the function body is an identity, so it only compiles if the two names
/// resolve to the same type.
#[allow(dead_code)]
fn terminal_stream_is_stream_terminal_stream<'a>(
    s: rustty_vt::stream_terminal::Stream<'a>,
) -> rustty_vt::TerminalStream<'a> {
    s
}

/// Likewise `Stream` must BE `stream::Stream`, for any handler.
#[allow(dead_code)]
fn stream_is_stream_stream<'a, H: rustty_vt::stream::Handler>(
    s: rustty_vt::stream::Stream<'a, H>,
) -> rustty_vt::Stream<'a, H> {
    s
}

#[test]
fn root_module_reexports_stream_and_terminal_stream() {
    assert_eq!(
        core::mem::size_of::<rustty_vt::stream_terminal::Stream<'static>>(),
        core::mem::size_of::<rustty_vt::TerminalStream<'static>>(),
    );
    assert_eq!(
        core::mem::size_of::<
            rustty_vt::stream::Stream<'static, rustty_vt::stream_terminal::Handler<'static>>,
        >(),
        core::mem::size_of::<
            rustty_vt::Stream<'static, rustty_vt::stream_terminal::Handler<'static>>,
        >(),
    );
}
