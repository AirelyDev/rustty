// SPDX-License-Identifier: MPL-2.0
//   Copyright © 2024-2025 Chris Marchesi
//
// Ported to Rust for rustty from z2d 0.10.0 `src/internal/tess/Dasher.zig`
// (R3 vendored dep).
//
// The full dashed-line machinery (`init`/`applyOffset`/`step`/`reset`, plus the
// `dashes`/`offset`/`idx`/`on`/`remain` state) is ported here and consumed by
// the two dashed plotters (`dashed_plotter`, and the `dashedLineTo` branch of
// `polyline_plotter`). `dashes` is a borrowed `&[f64]` (the Zig `[]const f64`),
// tied to the caller-owned dash array (byte/scalar-slice fidelity — no String).

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

/// Used to keep track of dash state. Faithful port of the `Dasher` struct in
/// `Dasher.zig`. Borrows the caller-owned dash array for its lifetime.
#[derive(Debug, Clone, Copy)]
pub struct Dasher<'a> {
    pub dashes: &'a [f64],
    pub offset: f64,
    pub idx: usize,
    pub on: bool,
    pub remain: f64,
}

impl<'a> Dasher<'a> {
    /// Port of `Dasher.init`.
    pub fn init(dashes: &'a [f64], offset: f64) -> Dasher<'a> {
        let mut result = Dasher {
            dashes,
            offset,
            idx: 0,
            on: true,
            remain: dashes[0],
        };
        result.apply_offset();
        result
    }

    /// Port of `Dasher.applyOffset` (private).
    fn apply_offset(&mut self) {
        self.remain -= self.offset;
        while self.remain < 0.0 || self.remain > self.dashes[self.idx] {
            if self.remain < 0.0 {
                self.remain += self.dashes[self.idx];
                self.idx = if self.idx >= self.dashes.len() - 1 {
                    0
                } else {
                    self.idx + 1
                };
            } else {
                self.remain -= self.dashes[self.idx];
                self.idx = if self.idx == 0 {
                    self.dashes.len() - 1
                } else {
                    self.idx - 1
                };
            }
            self.on = !self.on;
        }
    }

    /// Port of `Dasher.reset`.
    pub fn reset(&mut self) {
        self.idx = 0;
        self.on = true;
        self.remain = self.dashes[0];
        self.apply_offset();
    }

    /// Port of `Dasher.step`. Returns true if a dash transition occurred.
    pub fn step(&mut self, len: f64) -> bool {
        let mut stepped = false;
        self.remain -= len;
        if self.remain <= 0.0 {
            stepped = true;
            self.on = !self.on;
            self.idx += 1;
            if self.idx >= self.dashes.len() {
                self.idx = 0;
            }
            self.remain = self.dashes[self.idx];
        }

        stepped
    }
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

    // Behavioral tests for the dash state machine (`Dasher.zig` has no inline
    // `init`/`step`/`applyOffset` tests). These pin the transition arithmetic
    // that `dashed_plotter` / `polyline_plotter::dashedLineTo` rely on.
    #[test]
    fn init_no_offset() {
        let dashes = [2.0, 2.0];
        let d = Dasher::init(&dashes, 0.0);
        assert_eq!(0, d.idx);
        assert!(d.on);
        assert_eq!(2.0, d.remain);
    }

    #[test]
    fn step_transitions() {
        let dashes = [2.0, 2.0];
        let mut d = Dasher::init(&dashes, 0.0);
        // Partial step within the first dash: no transition.
        assert!(!d.step(1.0));
        assert!(d.on);
        assert_eq!(1.0, d.remain);
        // Completing the first dash: transition to "off".
        assert!(d.step(1.0));
        assert!(!d.on);
        assert_eq!(1, d.idx);
        assert_eq!(2.0, d.remain);
        // Completing the gap: wrap idx back to 0, transition to "on".
        assert!(d.step(2.0));
        assert!(d.on);
        assert_eq!(0, d.idx);
        assert_eq!(2.0, d.remain);
    }

    #[test]
    fn init_with_offset_pushes_into_gap() {
        // Offset 3 on a [2,2] pattern advances past the first (len-2) dash and
        // 1 unit into the gap: idx=1, off, remain=1.
        let dashes = [2.0, 2.0];
        let d = Dasher::init(&dashes, 3.0);
        assert_eq!(1, d.idx);
        assert!(!d.on);
        assert_eq!(1.0, d.remain);
    }

    #[test]
    fn reset_restores_offset_state() {
        let dashes = [2.0, 2.0];
        let mut d = Dasher::init(&dashes, 3.0);
        // Advance the state, then reset — should recover the post-offset state.
        d.step(0.5);
        d.reset();
        assert_eq!(1, d.idx);
        assert!(!d.on);
        assert_eq!(1.0, d.remain);
    }
}
