//! rustty-input input/config
//!
//! PORTS: ghostty src/input/config.zig
//!
//! PORT STATUS: done

/// Determines the macOS option key behavior. See the config
/// `macos-option-as-alt` for a lot more details.
//
// PORT NOTE: the reference is `enum(c_int)` with the field names `false`,
// `true`, `left`, `right`. `false`/`true` are Zig keywords used as enum tags;
// in Rust they become `False`/`True` PascalCase variants. The explicit `c_int`
// backing tag type is preserved via `#[repr(C)]` (the C-ABI integer), and the
// declaration order fixes the discriminants 0,1,2,3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
pub enum OptionAsAlt {
    False,
    True,
    Left,
    Right,
}
