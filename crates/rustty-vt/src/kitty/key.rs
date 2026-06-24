//! Kitty keyboard protocol support.

// const std = @import("std");

/// Stack for the key flags. This implements the push/pop behavior
/// of the CSI > u and CSI < u sequences. We implement the stack as
/// fixed size to avoid heap allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagStack {
    flags: [Flags; Self::LEN],
    idx: u3,
}

impl FlagStack {
    const LEN: usize = 8;
}

impl Default for FlagStack {
    fn default() -> Self {
        FlagStack {
            flags: [Flags::DISABLED; Self::LEN],
            idx: u3::ZERO,
        }
    }
}

impl FlagStack {
    /// Return the current stack value
    pub fn current(self: FlagStack) -> Flags {
        self.flags[self.idx.get() as usize]
    }

    /// Perform the "set" operation as described in the spec for
    /// the CSI = u sequence.
    pub fn set(self: &mut FlagStack, mode: SetMode, v: Flags) {
        match mode {
            SetMode::Set => self.flags[self.idx.get() as usize] = v,
            SetMode::Or => {
                self.flags[self.idx.get() as usize] =
                    Flags::from_int(self.flags[self.idx.get() as usize].int() | v.int())
            }
            SetMode::Not => {
                self.flags[self.idx.get() as usize] =
                    Flags::from_int(self.flags[self.idx.get() as usize].int() & !v.int())
            }
        }
    }

    /// Push a new set of flags onto the stack. If the stack is full
    /// then the oldest entry is evicted.
    pub fn push(self: &mut FlagStack, flags: Flags) {
        // Overflow and wrap around if we're full, which evicts
        // the oldest entry.
        self.idx = self.idx.wrapping_add(u3::ONE);
        self.flags[self.idx.get() as usize] = flags;
    }

    /// Pop `n` entries from the stack. This will just wrap around
    /// if `n` is greater than the amount in the stack.
    pub fn pop(self: &mut FlagStack, n: usize) {
        // If n is more than our length then we just reset the stack.
        // This also avoids a DoS vector where a malicious client
        // could send a huge number of pop commands to waste cpu.
        if n >= self.flags.len() {
            self.idx = u3::ZERO;
            self.flags = [Flags::DISABLED; Self::LEN];
            return;
        }

        for _ in 0..n {
            self.flags[self.idx.get() as usize] = Flags::DISABLED;
            self.idx = self.idx.wrapping_sub(u3::ONE);
        }
    }
}

/// PORT NOTE: Zig's `u3` is a 3-bit integer with defined wrap-around
/// arithmetic (`+%`/`-%`). Rust has no native 3-bit integer, so this models
/// the same value range `0..=7` with masked wrapping add/sub, mirroring the
/// reference's `idx: u3` field and its `+%= 1` / `-%= 1` operations exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
struct u3(u8);

impl u3 {
    const ZERO: u3 = u3(0);
    const ONE: u3 = u3(1);
    const MASK: u8 = 0b111;

    fn get(self) -> u8 {
        self.0
    }

    fn wrapping_add(self, other: u3) -> u3 {
        u3(self.0.wrapping_add(other.0) & Self::MASK)
    }

    fn wrapping_sub(self, other: u3) -> u3 {
        u3(self.0.wrapping_sub(other.0) & Self::MASK)
    }
}

// Make sure we the overflow works as expected
#[cfg(test)]
#[test]
fn flag_stack_overflow() {
    let mut stack: FlagStack = FlagStack::default();
    stack.idx = u3((stack.flags.len() - 1) as u8);
    stack.idx = stack.idx.wrapping_add(u3::ONE);
    assert!(stack.idx == u3::ZERO);

    stack.idx = u3::ZERO;
    stack.idx = stack.idx.wrapping_sub(u3::ONE);
    assert!(stack.idx == u3((stack.flags.len() - 1) as u8));
}

/// The possible flags for the Kitty keyboard protocol.
///
/// PORT NOTE: Zig's `packed struct(u5)` with five `bool` fields in declaration
/// order. The backing integer assigns field 0 (`disambiguate`) to the least
/// significant bit, field 1 (`report_events`) to the next, and so on, which
/// `int()`/`from_int` reproduce bit-for-bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags {
    pub disambiguate: bool,
    pub report_events: bool,
    pub report_alternates: bool,
    pub report_all: bool,
    pub report_associated: bool,
}

impl Flags {
    /// Kitty keyboard protocol disabled (all flags off).
    pub const DISABLED: Flags = Flags {
        disambiguate: false,
        report_events: false,
        report_alternates: false,
        report_all: false,
        report_associated: false,
    };

    /// Sets all modes on.
    #[allow(non_upper_case_globals)]
    pub const r#true: Flags = Flags {
        disambiguate: true,
        report_events: true,
        report_alternates: true,
        report_all: true,
        report_associated: true,
    };

    pub fn int(self: Flags) -> u8 {
        (self.disambiguate as u8)
            | ((self.report_events as u8) << 1)
            | ((self.report_alternates as u8) << 2)
            | ((self.report_all as u8) << 3)
            | ((self.report_associated as u8) << 4)
    }

    /// PORT NOTE: `@bitCast` from the `u5` backing integer back to the packed
    /// struct. Only the low 5 bits are meaningful.
    pub fn from_int(v: u8) -> Flags {
        Flags {
            disambiguate: (v & 0b00001) != 0,
            report_events: (v & 0b00010) != 0,
            report_alternates: (v & 0b00100) != 0,
            report_all: (v & 0b01000) != 0,
            report_associated: (v & 0b10000) != 0,
        }
    }
}

// Its easy to get packed struct ordering wrong so this test checks.
#[cfg(test)]
#[test]
fn flags_int_ordering() {
    assert_eq!(
        0b1u8,
        Flags {
            disambiguate: true,
            ..Default::default()
        }
        .int(),
    );
    assert_eq!(
        0b10u8,
        Flags {
            report_events: true,
            ..Default::default()
        }
        .int(),
    );
}

/// The possible modes for setting the key flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetMode {
    Set,
    Or,
    Not,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_stack_push_pop() {
        let mut stack: FlagStack = FlagStack::default();
        stack.push(Flags {
            disambiguate: true,
            ..Default::default()
        });
        assert_eq!(
            Flags {
                disambiguate: true,
                ..Default::default()
            },
            stack.current(),
        );

        stack.pop(1);
        assert_eq!(Flags::default(), stack.current());
    }

    #[test]
    fn flag_stack_pop_big_number() {
        let mut stack: FlagStack = FlagStack::default();
        stack.pop(100);
        assert_eq!(Flags::default(), stack.current());
    }

    #[test]
    fn flag_stack_set() {
        let mut stack: FlagStack = FlagStack::default();
        stack.set(
            SetMode::Set,
            Flags {
                disambiguate: true,
                ..Default::default()
            },
        );
        assert_eq!(
            Flags {
                disambiguate: true,
                ..Default::default()
            },
            stack.current(),
        );

        stack.set(
            SetMode::Or,
            Flags {
                report_events: true,
                ..Default::default()
            },
        );
        assert_eq!(
            Flags {
                disambiguate: true,
                report_events: true,
                ..Default::default()
            },
            stack.current(),
        );

        stack.set(
            SetMode::Not,
            Flags {
                report_events: true,
                ..Default::default()
            },
        );
        assert_eq!(
            Flags {
                disambiguate: true,
                ..Default::default()
            },
            stack.current(),
        );
    }
}
