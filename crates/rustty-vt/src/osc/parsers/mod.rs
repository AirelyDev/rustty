//! rustty-vt osc/parsers
//!
//! PORTS: ghostty src/terminal/osc/parsers.zig
//!
//! PORT STATUS: ported — aggregator of the per-command OSC parsers. Each
//! submodule holds the faithful `parse` implementation plus the public payload
//! types referenced by the OSC parent `Command`, with the per-parser `test {}`
//! blocks ported alongside.

// PORT NOTE: the reference is a pure aggregator: `pub const X = @import(...)`
// for each per-command parser submodule, followed by a `refAllDecls` root test.
// The `pub const change_window_icon`/etc. imports are mirrored as `pub mod`
// declarations below. Submodule names match the reference 1:1.
pub mod change_window_icon;
pub mod change_window_title;
pub mod clipboard_operation;
pub mod color;
pub mod context_signal;
pub mod hyperlink;
pub mod iterm2;
pub mod kitty_clipboard_protocol;
pub mod kitty_color;
pub mod kitty_desktop_notification;
pub mod kitty_dnd_protocol;
pub mod kitty_text_sizing;
pub mod mouse_shape;
pub mod osc9;
pub mod program_status;
pub mod report_pwd;
pub mod rxvt_extension;
pub mod semantic_prompt;
pub mod unknown;

// test {
//     std.testing.refAllDecls(@This());
// }
// PORT NOTE: the reference root `test { std.testing.refAllDecls(@This()); }` is
// a reflection smoke test that forces every declaration in the aggregator (i.e.
// every per-command parser submodule) to be analyzed. Rust compiles and
// type-checks every `pub mod` item unconditionally, so the structural guarantee
// is automatic; we mirror it concretely by touching each submodule's public
// `parse` entry point so a regression (a removed/renamed parser) surfaces as a
// test failure rather than only a compile error.
#[cfg(test)]
mod tests {
    #[test]
    fn ref_all_decls() {
        // Function-pointer references to each per-command parser's public
        // `parse`. Naming each one forces the symbol to exist with the expected
        // shape, mirroring `refAllDecls` over the aggregator's submodules.
        let _ = super::change_window_icon::parse as fn(_, _) -> _;
        let _ = super::change_window_title::parse as fn(_, _) -> _;
        let _ = super::clipboard_operation::parse as fn(_, _) -> _;
        let _ = super::color::parse as fn(_, _) -> _;
        let _ = super::context_signal::parse as fn(_, _) -> _;
        let _ = super::hyperlink::parse as fn(_, _) -> _;
        let _ = super::iterm2::parse as fn(_, _) -> _;
        let _ = super::kitty_clipboard_protocol::parse as fn(_, _) -> _;
        let _ = super::kitty_color::parse as fn(_, _) -> _;
        let _ = super::kitty_dnd_protocol::parse as fn(_, _) -> _;
        let _ = super::kitty_desktop_notification::parse as fn(_, _) -> _;
        let _ = super::program_status::parse as fn(_, _) -> _;
        let _ = super::kitty_text_sizing::parse as fn(_, _) -> _;
        let _ = super::mouse_shape::parse as fn(_, _) -> _;
        let _ = super::osc9::parse as fn(_, _) -> _;
        let _ = super::report_pwd::parse as fn(_, _) -> _;
        let _ = super::rxvt_extension::parse as fn(_, _) -> _;
        let _ = super::semantic_prompt::parse as fn(_, _) -> _;
        let _ = super::unknown::parse as fn(_, _) -> _;
    }
}
