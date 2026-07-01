// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/InternalError.zig`
// (R3 vendored dep).

/// InternalError represents an error with the internal state of the plotter.
/// These errors are wholly unexpected and should never be returned under
/// correct circumstances, so please report a bug if you encounter it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalError {
    InvalidState,
}
