//! rustty-vt focus
//!
//! PORTS: ghostty src/terminal/focus.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const lib = @import("lib.zig");
// `lib.Enum` for the `.zig` target is a plain enum with the keys in order;
// ported directly as a Rust enum.
//
// PORT NOTE: the reference `encode` takes a `*std.Io.Writer` and returns
// `std.Io.Writer.Error!void`. We port it to take any `core::fmt::Write` sink and
// return `core::fmt::Result`, which models the same "append formatted bytes, may
// fail to write" contract (matching the pattern used elsewhere in this crate,
// e.g. size_report / device_attributes).

use core::fmt::Write;

/// Maximum number of bytes that `encode` will write. Any users of this
/// should be resilient to this changing, so this is always a specific
/// value (e.g. we don't add unnecessary padding).
pub const MAX_ENCODE_SIZE: usize = 3;

/// A focus event that can be reported to the application running in the
/// terminal when focus reporting mode (mode 1004) is enabled.
// PORT NOTE: `Event = lib.Enum(lib.target, &.{...})`. For the `.zig` target this
// is a plain enum with the keys in order; ported as a Rust enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Gained,
    Lost,
}

/// Encode a focus in/out report (CSI I / CSI O).
pub fn encode<W: Write>(writer: &mut W, event: Event) -> core::fmt::Result {
    writer.write_str(match event {
        Event::Gained => "\x1B[I",
        Event::Lost => "\x1B[O",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_focus_gained() {
        let mut buf = String::new();
        encode(&mut buf, Event::Gained).unwrap();
        assert_eq!("\x1B[I", buf);
    }

    #[test]
    fn encode_focus_lost() {
        let mut buf = String::new();
        encode(&mut buf, Event::Lost).unwrap();
        assert_eq!("\x1B[O", buf);
    }
}
