//! Renderer backend selection.
//!
//! BREAK #3: the `Backend` enum is DEFINED HERE (ported from the reference
//! `src/renderer/backend.zig`) so that `rustty-renderer` imports it from
//! build-config rather than the reverse (R4). The reference file has no
//! `test {}` block.

use crate::os::{Target, WasmTarget};

/// Possible implementations, used for build options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Opengl,
    Metal,
    Webgl,
}

impl Backend {
    pub const fn default(target: Target, wasm_target: WasmTarget) -> Backend {
        if target.arch.is_wasm() {
            return match wasm_target {
                WasmTarget::Browser => Backend::Webgl,
            };
        }

        if target.os.is_darwin() {
            return Backend::Metal;
        }
        Backend::Opengl
    }
}
