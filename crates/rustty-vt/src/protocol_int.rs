//! The UD141 wrapper the glyph APC and kitty graphics integer parsers run
//! before Zig's `std.fmt.parseInt` (`rustty_zigstd::fmt`).
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
/// `lh`) and kitty graphics key values read `1_0` as 10. Neither protocol
/// allows it (their specs define plain decimal or hex integers), and Ghostty
/// has since taken its own position: `73768913b` moved every OSC and colour
/// site to `lib.parseInt` (`rustty_lib::parse_int`), which refuses `_`,
/// because accepting it "makes us diverge from other terminals". These two
/// parsers are still on `std.fmt.parseInt` at 12752b2ac, so each calls this
/// first and takes its existing failure path. Only `_` is refused: the sign
/// handling (`+10`, `-0`) stays `parseInt`'s. The tmux control-mode parsers
/// (N100) stay faithful: their input is tmux's own generated output, not a
/// protocol programs write against.
pub(crate) fn reject_digit_separators(buf: &[u8]) -> Result<(), ParseIntError> {
    // if (std.mem.indexOfScalar(u8, value, '_')) |_| break :value key.default();
    if buf.contains(&b'_') {
        return Err(ParseIntError::InvalidCharacter);
    }
    Ok(())
}
