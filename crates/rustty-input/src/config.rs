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

// `OptionAsAlt` is used as the `macos-option-as-alt` config field type
// (`?OptionAsAlt`). The reference's `Config.changed` resolves its `deepEqual`
// via the `.@"enum"` arm (`old == new`). The orphan rule requires this
// `DeepEqual` impl to live in `OptionAsAlt`'s home crate (rustty-input).
impl rustty_datastruct::comparison::DeepEqual for OptionAsAlt {
    fn deep_equal(&self, other: &Self) -> bool {
        self == other
    }
}
