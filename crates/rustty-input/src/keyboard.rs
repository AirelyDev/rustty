//! rustty-input input/keyboard
//!
//! PORTS: ghostty src/input/keyboard.zig
//!
//! PORT STATUS: done
//
// PORT NOTE: the reference imports
//   const std = @import("std");
//   const OptionAsAlt = @import("config.zig").OptionAsAlt;
// `std.mem.eql(u8, ...)` maps to byte-slice equality; `OptionAsAlt` maps to the
// ported `config` module below.

use crate::config::OptionAsAlt;

/// Keyboard layouts.
///
/// These aren't heavily used in rustty and having a fully comprehensive
/// list is not important. We only need to distinguish between a few
/// different layouts for some nice-to-have features, such as setting a default
/// value for "macos-option-as-alt".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    // Unknown, unmapped layout. rustty should not make any assumptions
    // about the layout of the keyboard.
    Unknown,

    // The remaining should be fairly self-explanatory:
    UsStandard,
    UsInternational,
}

impl Layout {
    /// Map an Apple keyboard layout ID to a value in this enum. The layout
    /// ID can be retrieved using Carbon's TIKeyboardLayoutGetInputSourceProperty
    /// function.
    ///
    /// Even though our layout supports "unknown", we return null if we don't
    /// recognize the layout ID so callers can detect this scenario.
    pub fn map_apple_id(id: &[u8]) -> Option<Layout> {
        if id == b"com.apple.keylayout.US" {
            Some(Layout::UsStandard)
        } else if id == b"com.apple.keylayout.USInternational" {
            Some(Layout::UsInternational)
        } else {
            None
        }
    }

    /// Returns the default macos-option-as-alt value for this layout.
    ///
    /// We apply some heuristics to change the default based on the keyboard
    /// layout if "macos-option-as-alt" is unset. We do this because on some
    /// keyboard layouts such as US standard layouts, users generally expect
    /// an input such as option-b to map to alt-b but macOS by default will
    /// convert it to the codepoint "∫".
    ///
    /// This behavior however is desired on international layout where the
    /// option key is used for important, regularly used inputs.
    pub fn detect_option_as_alt(self) -> OptionAsAlt {
        match self {
            // On US standard, the option key is typically used as alt
            // and not as a modifier for other codepoints. For example,
            // option-B = ∫ but usually the user wants alt-B.
            Layout::UsStandard | Layout::UsInternational => OptionAsAlt::True,

            Layout::Unknown => OptionAsAlt::False,
        }
    }
}
