//! The UD141 wrapper the terminal's wire-protocol integer parsers run before
//! Zig's `std.fmt.parseInt` / `parseUnsigned` (`rustty_zigstd::fmt`).
//!
//! PORT NOTE (NEW-FINDINGS N120/N150, task tail-s9): this module was
//! `zig_parse_int.rs`, rustty-vt's own port of `std.fmt.parseInt`. That port is
//! now the one in `rustty_zigstd::fmt` (its arbitrary-width core moved there,
//! with its tests); what stays here is the rustty-vt-specific `_` filter, which
//! is reference behaviour the std function does not have.

use rustty_zigstd::fmt::ParseIntError;

/// Refuses Zig's `_` digit separator in a wire-protocol integer, before
/// `rustty_zigstd::fmt::parse_int`/`parse_int_type`/`parse_unsigned` runs.
///
/// PORT NOTE: reference defect (R13, extended 2026-09-23), UD141. Zig's
/// `std.fmt.parseInt`/`parseUnsigned` accept interior `_` separators (a Zig
/// source-literal feature), so the reference's glyph APC (`cp`, `upm`, `aw`,
/// `lh`), kitty graphics key values, OSC 4/104 color indices, OSC 66 text
/// sizing values, the OSC 133 exit code and the OSC 9;1 / 9;4 numbers all read
/// `1_0` as 10. None of those protocols allows it (their specs define plain
/// decimal or hex integers), and Ghostty states its own position in a sibling
/// parser: "Zig's integer parser allows '_', we don't"
/// (`osc/parsers/kitty_desktop_notification.zig:210-211`, which checks
/// `std.mem.indexOfScalar(u8, value, '_')` and takes the key's failure path),
/// and `osc/parsers/context_signal.zig:165-167` accepts only digits. So each
/// of those sites calls this first and takes its existing failure path, as the
/// OSC 99 parser does. Only `_` is refused: the OSC 99 filter leaves the sign
/// handling (`+10`, `-0`) to `parseInt`, so the port keeps it too. The tmux
/// control-mode parsers (N100) stay faithful: their input is tmux's own
/// generated output, not a protocol programs write against.
pub(crate) fn reject_digit_separators(buf: &[u8]) -> Result<(), ParseIntError> {
    // if (std.mem.indexOfScalar(u8, value, '_')) |_| break :value key.default();
    if buf.contains(&b'_') {
        return Err(ParseIntError::InvalidCharacter);
    }
    Ok(())
}
