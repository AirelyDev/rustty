//! rustty-oniguruma region
//!
//! PORTS: ghostty pkg/oniguruma/region.zig
//!
//! PORT STATUS: done. `Region` is the C `OnigRegion` (`re_registers`) layout —
//! the match-group range table Oniguruma fills in during a search.

use oniguruma_sys as c;
use std::os::raw::c_int;

/// The match-group range table Oniguruma fills in during a search.
// PORT NOTE: the reference `Region = extern struct { … }` is layout-compatible
// with the C `OnigRegion`; we keep `#[repr(C)]` with the identical field order
// so `&mut Region as *mut OnigRegion` (the `@ptrCast(self)` in the reference) is
// sound. `history_root` is the C `OnigCaptureTreeNode*` (TODO in the reference:
// convert to a Rust type).
#[repr(C)]
pub struct Region {
    pub allocated: c_int,
    pub num_regs: c_int,
    pub beg: *mut c_int,
    pub end: *mut c_int,
    pub history_root: *mut c::OnigCaptureTreeNode, // TODO: convert to Rust
}

impl Default for Region {
    // PORT NOTE: the reference default is `.{}` — zeroed counts + null slices.
    fn default() -> Self {
        Region {
            allocated: 0,
            num_regs: 0,
            beg: std::ptr::null_mut(),
            end: std::ptr::null_mut(),
            history_root: std::ptr::null_mut(),
        }
    }
}

impl Region {
    pub fn deinit(&mut self) {
        // We never free ourself because allocation of Region in the Zig
        // bindings is handled by the Zig program.
        // PORT NOTE: `free_self = 0` — free the internal `beg`/`end` buffers but
        // NOT the `Region` value itself (it is owned by the caller / this Rust
        // value, not the C allocator).
        unsafe { c::onig_region_free(self as *mut Region as *mut c::OnigRegion, 0) }
    }

    /// Count the number of matches
    pub fn count(&self) -> usize {
        self.num_regs as usize
    }

    /// Iterate over the matched ranges.
    pub fn iterator(&self) -> Iterator<'_> {
        Iterator { region: self, i: 0 }
    }

    pub fn starts(&self) -> &[c_int] {
        if self.num_regs == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.beg, self.num_regs as usize) }
    }

    pub fn ends(&self) -> &[c_int] {
        if self.num_regs == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.end, self.num_regs as usize) }
    }
}

/// Iterates the `[begin, end)` byte ranges of each match group.
// PORT NOTE: the reference `Region.Iterator` yields `[2]usize` per group.
pub struct Iterator<'a> {
    region: &'a Region,
    i: usize,
}

impl Iterator<'_> {
    /// The next range
    // PORT NOTE: the reference `next` returns `?[2]usize` and `defer self.i += 1`.
    // `should_implement_trait` is suppressed at the module level: this is the
    // reference's fallible-style `next`, not `std::iter::Iterator`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<[usize; 2]> {
        if self.i >= self.region.num_regs as usize {
            return None;
        }
        let out = unsafe {
            [
                *self.region.beg.add(self.i) as usize,
                *self.region.end.add(self.i) as usize,
            ]
        };
        self.i += 1;
        Some(out)
    }
}
