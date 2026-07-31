// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2026 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.12.1 `src/internal/path_fx/shared.zig`
// (R3 vendored dep). `dumpNodes` is deliberately not ported (upstream marks it
// "For testing only. TODO: remove eventually.").

use super::input_set;
use super::output_set::{Contour, OutputSet};

pub fn noop_contour(out: &mut OutputSet, in_: &input_set::Contour) {
    if in_.segments.is_empty() {
        return;
    }

    let mut result = Contour::default();

    if in_.closed {
        for seg in &in_.segments {
            result.plot(seg.p0);
        }

        result.close();
    } else {
        result.plot(in_.segments[0].p0);
        result.plot(in_.segments[0].p1);
        if in_.segments.len() > 1 {
            for seg in &in_.segments[1..] {
                result.plot(seg.p1);
            }
        }
    }

    out.contours.push(result);
}
