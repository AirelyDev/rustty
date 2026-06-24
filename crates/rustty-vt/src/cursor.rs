//! rustty-vt cursor
//!
//! PORTS: ghostty src/terminal/cursor.zig
//!
//! PORT STATUS: done

/// The visual style of the cursor. Whether or not it blinks
/// is determined by mode 12 (modes.zig). This mode is synchronized
/// with CSI q, the same as xterm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Bar,       // DECSCUSR 5, 6
    Block,     // DECSCUSR 1, 2
    Underline, // DECSCUSR 3, 4

    /// The cursor styles below aren't known by DESCUSR and are custom
    /// implemented in Ghostty. They are reported as some standard style
    /// if requested, though.
    /// Hollow block cursor. This is a block cursor with the center empty.
    /// Reported as DECSCUSR 1 or 2 (block).
    BlockHollow,
}
