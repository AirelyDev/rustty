// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/tess/Dasher.zig`
// (R3 vendored dep).
//
// SUBSET NOTE (rustty): only `validate` is ported — the dashed-line machinery
// (`init`/`applyOffset`/`step`/`reset`, plus the `dashes`/`offset`/`idx`/`on`/
// `remain` state) is not reached by the sprite subset (neither the polygon
// stroke nor the hairline stroke uses dashes; both guard on `validate` and
// `unimplemented!()` a valid dash array). Ported so the two plotters
// (`stroke_plotter`, `polyline_plotter`) share one faithful copy of the guard.

//! Used to keep track of dash state.

/// Port of `Dasher.validate`: true iff `dashes` is non-negative with at least
/// one positive entry.
pub fn validate(dashes: &[f64]) -> bool {
    let mut valid = false;
    for &d in dashes {
        if d < 0.0 {
            return false;
        }
        if d > 0.0 {
            valid = true;
        }
    }
    valid
}

#[cfg(test)]
mod tests {
    use super::*;

    // Port of `Dasher.zig` `test "validate"` (Dasher.zig:77-98). Promoting
    // `validate` to a named port takes on the R1 obligation to port its
    // reference `test{}` verbatim — seven cases, mirroring the Zig
    // `testing.expect` / `testing.expect(!...)` assertions.
    #[test]
    fn validate_test() {
        // Good (single)
        assert!(validate(&[1.0]));

        // Good (even)
        assert!(validate(&[1.0, 2.0]));

        // Good (odd)
        assert!(validate(&[1.0, 2.0, 3.0]));

        // Good (zero stops)
        assert!(validate(&[0.0, 2.0, 0.0]));

        // Bad (single zero)
        assert!(!validate(&[0.0]));

        // Bad (all zeroes)
        assert!(!validate(&[0.0, 0.0, 0.0]));

        // Bad (negative value)
        assert!(!validate(&[1.0, -2.0, 3.0]));
    }
}
