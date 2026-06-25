//! rustty-vt input/config
//!
//! PORTS: ghostty src/input/config.zig
//!
//! PORT STATUS: done
//
// PORT NOTE: `input/config.zig` re-exports a small set of config types used by
// the input layer. The only type the VT input slice needs is `OptionAsAlt`
// (consumed by `key_encode`); the rest of the apprt-layer config lives in the
// `rustty-config` crate (an apprt/config seam, not VT). `OptionAsAlt` is
// reproduced here so the VT key encoder has no apprt-config dependency.

/// Determines the macOS option key behavior. See the config
/// `macos-option-as-alt` for a lot more details.
///
/// PORT NOTE: `enum(c_int)` — backed by `c_int` so it can be used as-is for the
/// embedding API. The reference field `true`/`false` are Rust keywords, so they
/// are rendered `r#true`/`r#false`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[allow(non_camel_case_types)]
pub enum OptionAsAlt {
    #[default]
    r#false,
    r#true,
    left,
    right,
}
