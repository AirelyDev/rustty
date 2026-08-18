//! Keep track of the location of tabstops.
//!
//! This is implemented as a bit set. There is a preallocation segment that
//! is used for almost all screen sizes. Then there is a dynamically allocated
//! segment if the screen is larger than the preallocation amount.
//!
//! In reality, tabstops don't need to be the most performant in any metric.
//! This implementation tries to balance denser memory usage (by using a bitset)
//! and minimizing unnecessary allocations.

// const Tabstops = @This();
//
// const std = @import("std");
// const tripwire = @import("../tripwire.zig");
// const Allocator = std.mem.Allocator;
// const testing = std.testing;
// const assert = @import("../quirks.zig").inlineAssert;
//
// PORT NOTE: `tripwire` and `quirks.inlineAssert` live in `rustty-util` (the
// FIRM leaf crate). `std.mem.Allocator` is faithfully modeled by
// `allocator_api2::alloc::Allocator` (the same map used by PageList); callers
// pass `allocator_api2::alloc::Global` for the prior `std::alloc` behavior.
// (`ac4dc1205` dropped the `fastmem` import along with the alloc+copy+free
// resize path.)
use allocator_api2::alloc::{Allocator, Global};
use allocator_api2::boxed::Box;
use rustty_util::quirks::inlineAssert as assert;

/// Unit is the type we use per tabstop unit (see file docs).
type Unit = u8;
// const unit_bits = @bitSizeOf(Unit);
const UNIT_BITS: usize = core::mem::size_of::<Unit>() * 8;

/// The number of columns we preallocate for. This is kind of high which
/// costs us some memory, but this is more columns than my 6k monitor at
/// 12-point font size, so this should prevent allocation in almost all
/// real world scenarios for the price of wasting at most
/// (columns / sizeOf(Unit)) bytes.
const PREALLOC_COLUMNS: usize = 512;

/// The number of entries we need for our preallocation.
const PREALLOC_COUNT: usize = PREALLOC_COLUMNS / UNIT_BITS;

/// We precompute all the possible masks since we never use a huge bit size.
// const masks = blk: {
//     var res: [unit_bits]Unit = undefined;
//     for (res, 0..) |_, i| {
//         res[i] = @shlExact(@as(Unit, 1), @as(u3, @intCast(i)));
//     }
//
//     break :blk res;
// };
const MASKS: [Unit; UNIT_BITS] = {
    let mut res: [Unit; UNIT_BITS] = [0; UNIT_BITS];
    let mut i: usize = 0;
    while i < UNIT_BITS {
        res[i] = (1 as Unit) << (i as u32);
        i += 1;
    }
    res
};

/// The number of columns this tabstop is set to manage. Use resize()
/// to change this number.
// cols: usize = 0,
//
// /// Preallocated tab stops.
// prealloc_stops: [prealloc_count]Unit = @splat(0),
//
// /// Dynamically expanded stops above prealloc stops.
// dynamic_stops: []Unit = &[0]Unit{},
//
// PORT NOTE: the reference threads `alloc: Allocator` through `init`/`deinit`/
// `resize`. Here `Tabstops` carries the allocator type as a generic `A:
// Allocator` (defaulting to `Global`, the `std.alloc` map) and the instance in
// the field, so `set`/`unset`/`get`/`capacity`/`reset` need no allocator
// argument — matching the reference where those methods take no allocator.
// `dynamic_stops` is the heap slice; we model `[]Unit` as `Box<[Unit], A>`
// (empty box for the `&[0]Unit{}` default).
pub struct Tabstops<A: Allocator = Global> {
    /// The number of columns this tabstop is set to manage. Use resize()
    /// to change this number.
    cols: usize,

    /// Preallocated tab stops.
    prealloc_stops: [Unit; PREALLOC_COUNT],

    /// Dynamically expanded stops above prealloc stops.
    dynamic_stops: Box<[Unit], A>,
}

/// The allocation-failure error, mirroring Zig's `Allocator.Error` / OutOfMemory.
// PORT NOTE: the reference functions return `Allocator.Error!T`; in Rust this
// is `Result<T, OutOfMemory>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutOfMemory;

/// Returns the entry in the stops array that would contain this column.
// inline fn entry(col: usize) usize {
//     return col / unit_bits;
// }
#[inline]
fn entry(col: usize) -> usize {
    col / UNIT_BITS
}

// inline fn index(col: usize) usize {
//     return @mod(col, unit_bits);
// }
#[inline]
fn index(col: usize) -> usize {
    col % UNIT_BITS
}

impl Tabstops<Global> {
    // pub fn init(
    //     alloc: Allocator,
    //     cols: usize,
    //     interval: usize,
    // ) Allocator.Error!Tabstops {
    //     var res: Tabstops = .{};
    //     try res.resize(alloc, cols);
    //     res.reset(interval);
    //     return res;
    // }
    //
    // PORT NOTE: the default-constructed `Tabstops{}` (`.{}`) is `default()`
    // over `Global`. Threading a non-`Global` allocator would use
    // `init_in`/`new_in` analogues; the reference only ever uses the default
    // allocator path here.
    pub fn init(
        alloc: Global,
        cols: usize,
        interval: usize,
    ) -> Result<Tabstops<Global>, OutOfMemory> {
        let mut res: Tabstops<Global> = Tabstops::default();
        res.resize(alloc, cols)?;
        res.reset(interval);
        Ok(res)
    }
}

impl Default for Tabstops<Global> {
    fn default() -> Self {
        Tabstops {
            cols: 0,
            prealloc_stops: [0; PREALLOC_COUNT],
            // &[0]Unit{} — an empty heap slice (no allocation). An empty
            // `Vec` into a boxed slice allocates nothing (dangling pointer),
            // matching the reference's zero-length default slice.
            dynamic_stops: allocator_api2::vec::Vec::new_in(Global).into_boxed_slice(),
        }
    }
}

impl<A: Allocator> Tabstops<A> {
    // pub fn deinit(self: *Tabstops, alloc: Allocator) void {
    //     if (self.dynamic_stops.len > 0) alloc.free(self.dynamic_stops);
    //     self.* = undefined;
    // }
    //
    // PORT NOTE: the allocator instance owned by the `Box` frees the slice on
    // drop; we take `alloc` to mirror the reference signature (the freed slice
    // was allocated by this allocator). `self.* = undefined` has no Rust
    // analogue — the value is consumed by drop.
    pub fn deinit(self, _alloc: A) {
        // dropping `self` (and its `dynamic_stops` Box) frees via the allocator.
        let _ = self;
    }

    /// Set the tabstop at a certain column. The columns are 0-indexed.
    // pub fn set(self: *Tabstops, col: usize) void {
    pub fn set(&mut self, col: usize) {
        let i = entry(col);
        let idx = index(col);
        if i < PREALLOC_COUNT {
            self.prealloc_stops[i] |= MASKS[idx];
            return;
        }

        let dynamic_i = i - PREALLOC_COUNT;
        assert(dynamic_i < self.dynamic_stops.len());
        self.dynamic_stops[dynamic_i] |= MASKS[idx];
    }

    /// Unset the tabstop at a certain column. The columns are 0-indexed.
    // pub fn unset(self: *Tabstops, col: usize) void {
    pub fn unset(&mut self, col: usize) {
        let i = entry(col);
        let idx = index(col);
        if i < PREALLOC_COUNT {
            self.prealloc_stops[i] ^= MASKS[idx];
            return;
        }

        let dynamic_i = i - PREALLOC_COUNT;
        assert(dynamic_i < self.dynamic_stops.len());
        self.dynamic_stops[dynamic_i] ^= MASKS[idx];
    }

    /// Get the value of a tabstop at a specific column. The columns are 0-indexed.
    // pub fn get(self: Tabstops, col: usize) bool {
    pub fn get(&self, col: usize) -> bool {
        let i = entry(col);
        let idx = index(col);
        let mask = MASKS[idx];
        let unit = if i < PREALLOC_COUNT {
            self.prealloc_stops[i]
        } else {
            let dynamic_i = i - PREALLOC_COUNT;
            assert(dynamic_i < self.dynamic_stops.len());
            self.dynamic_stops[dynamic_i]
        };

        unit & mask == mask
    }

    /// Resize this to support up to cols columns.
    // const resize_tw = tripwire.module(enum {
    //     dynamic_alloc,
    // }, resize);
    //
    // PORT NOTE: the Zig `tripwire.module(P, E)` is a comptime function
    // returning a type with type-static state; the Rust
    // `rustty_util::tripwire::Module<P, E>` is an INSTANCE holding the same
    // state (see its PORT NOTE). To share that state between `resize` (which
    // calls `check`) and the "resize alloc failure preserves state" test (which
    // calls `errorAlways`), we hold the module in a `#[cfg(test)]
    // thread_local!` and route access through the `cfg(test)` helper below. In
    // non-test builds the call site is `#[cfg(test)]`-gated, so production
    // `resize` never reaches `check`, mirroring the Zig `enabled =
    // builtin.is_test` gate.
    //
    // TODO: needs interval to set new tabstops
    // pub fn resize(
    //     self: *Tabstops,
    //     alloc: Allocator,
    //     cols: usize,
    // ) Allocator.Error!void {
    pub fn resize(&mut self, alloc: A, cols: usize) -> Result<(), OutOfMemory> {
        // const tw = resize_tw;

        // Do nothing if it fits.
        if cols <= PREALLOC_COLUMNS {
            self.cols = cols;
            return Ok(());
        }

        // Number of units needed beyond the preallocated columns.
        // const dynamic_count = std.math.divCeil(
        //     usize,
        //     cols - prealloc_columns,
        //     unit_bits,
        // ) catch unreachable;
        //
        // R7: `cols - prealloc_columns` cannot underflow (the `cols <=
        // PREALLOC_COLUMNS` early-return above guarantees it), asserted; the
        // reference `divCeil ... catch unreachable` only errors on a zero
        // divisor (`UNIT_BITS` is a nonzero constant), matching `div_ceil`.
        debug_assert!(cols > PREALLOC_COLUMNS);
        let dynamic_count = (cols - PREALLOC_COLUMNS).div_ceil(UNIT_BITS);
        if dynamic_count <= self.dynamic_stops.len() {
            self.cols = cols;
            return Ok(());
        }

        // try tw.check(.dynamic_alloc);
        #[cfg(test)]
        resize_tw_check(ResizeFp::DynamicAlloc)?;
        // const old_len = self.dynamic_stops.len;
        // self.dynamic_stops = try alloc.realloc(self.dynamic_stops, dynamic_count);
        // @memset(self.dynamic_stops[old_len..], 0);
        // self.cols = cols;
        //
        // PORT NOTE: `alloc.realloc(old, dynamic_count)` grows the slice in
        // place where possible, preserving the existing stops. The Rust model
        // moves the `Box` back into a `Vec` over the same allocator `A`,
        // grows it with `resize(dynamic_count, 0)` (realloc + the reference's
        // tail-only `@memset`), and re-boxes it.
        let old = core::mem::replace(
            &mut self.dynamic_stops,
            allocator_api2::vec::Vec::new_in(alloc).into_boxed_slice(),
        );
        let mut new_vec: allocator_api2::vec::Vec<Unit, A> = old.into_vec();
        new_vec.resize(dynamic_count, 0);
        self.dynamic_stops = new_vec.into_boxed_slice();
        self.cols = cols;
        Ok(())
    }

    /// Return the maximum number of columns this can support currently.
    // pub fn capacity(self: Tabstops) usize {
    //     return (prealloc_count + self.dynamic_stops.len) * unit_bits;
    // }
    pub fn capacity(&self) -> usize {
        (PREALLOC_COUNT + self.dynamic_stops.len()) * UNIT_BITS
    }

    /// Unset all tabstops and then reset the initial tabstops to the given
    /// interval. An interval of 0 sets no tabstops.
    // pub fn reset(self: *Tabstops, interval: usize) void {
    pub fn reset(&mut self, interval: usize) {
        // @memset(&self.prealloc_stops, 0);
        self.prealloc_stops.fill(0);
        // @memset(self.dynamic_stops, 0);
        self.dynamic_stops.fill(0);

        // if (interval == 0 or self.cols <= 1) return;
        // PORT NOTE (parity 3d08161b5): `self.cols - 1` underflowed (Zig: safety
        // panic; Rust: debug panic / release wrap) for zero/one-column grids.
        if interval == 0 || self.cols <= 1 {
            return;
        }

        let mut i: usize = interval;
        while i < self.cols - 1 {
            self.set(i);
            i += interval;
        }
    }
}

// PORT NOTE: tripwire fail-point enum + thread-local module for `resize`,
// mirroring the reference `resize_tw = tripwire.module(enum { dynamic_alloc },
// resize)`. Only compiled in test builds; see the PORT NOTE on `resize`.
//
// `pub(crate)` (not file-private) so `terminal.rs`'s
// `terminal_resize_preserves_tabstops_on_allocation_failure` test can arm this
// fail point: `Terminal::resize` -> `Tabstops::init` -> `Tabstops::resize`
// reaches it, which is exactly the path upstream 91f0cf67d's FailingAllocator
// test exercises.
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum ResizeFp {
    DynamicAlloc,
}

#[cfg(test)]
thread_local! {
    pub(crate) static RESIZE_TW:
        core::cell::RefCell<rustty_util::tripwire::Module<ResizeFp, OutOfMemory>> =
        core::cell::RefCell::new(rustty_util::tripwire::module());
}

// `try tw.check(point)` for the `resize` tripwire module.
#[cfg(test)]
#[inline]
fn resize_tw_check(point: ResizeFp) -> Result<(), OutOfMemory> {
    RESIZE_TW.with(|tw| tw.borrow_mut().check(point))
}

/// `resize_tw.errorAlways(point, error.OutOfMemory)` — cross-module accessor so
/// `terminal.rs`'s tests can arm this module's fail points.
#[cfg(test)]
pub(crate) fn resize_tw_error_always(point: ResizeFp) {
    RESIZE_TW.with(|tw| tw.borrow_mut().errorAlways(point, OutOfMemory));
}

/// `resize_tw.end(mode)` — cross-module accessor (see above).
#[cfg(test)]
pub(crate) fn resize_tw_end(
    mode: rustty_util::tripwire::ResetMode,
) -> Result<(), rustty_util::tripwire::UntrippedError> {
    RESIZE_TW.with(|tw| tw.borrow_mut().end(mode))
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;

    // test "Tabstops: basic"
    #[test]
    fn tabstops_basic() {
        // var t: Tabstops = .{};
        // defer t.deinit(testing.allocator);
        let mut t: Tabstops = Tabstops::default();
        // try testing.expectEqual(@as(usize, 0), entry(4));
        assert_eq!(0usize, entry(4));
        // try testing.expectEqual(@as(usize, 1), entry(8));
        assert_eq!(1usize, entry(8));
        // try testing.expectEqual(@as(usize, 0), index(0));
        assert_eq!(0usize, index(0));
        // try testing.expectEqual(@as(usize, 1), index(1));
        assert_eq!(1usize, index(1));
        // try testing.expectEqual(@as(usize, 1), index(9));
        assert_eq!(1usize, index(9));

        // try testing.expectEqual(@as(Unit, 0b00001000), masks[3]);
        assert_eq!(0b00001000 as Unit, MASKS[3]);
        // try testing.expectEqual(@as(Unit, 0b00010000), masks[4]);
        assert_eq!(0b00010000 as Unit, MASKS[4]);

        // try testing.expect(!t.get(4));
        assert!(!t.get(4));
        // t.set(4);
        t.set(4);
        // try testing.expect(t.get(4));
        assert!(t.get(4));
        // try testing.expect(!t.get(3));
        assert!(!t.get(3));

        // t.reset(0);
        t.reset(0);
        // try testing.expect(!t.get(4));
        assert!(!t.get(4));

        // t.set(4);
        t.set(4);
        // try testing.expect(t.get(4));
        assert!(t.get(4));
        // t.unset(4);
        t.unset(4);
        // try testing.expect(!t.get(4));
        assert!(!t.get(4));

        t.deinit(Global);
    }

    // test "Tabstops: resize to existing capacity does not allocate"
    //
    // PORT NOTE: the reference proves "no second allocation" with a
    // `FixedBufferAllocator` sized exactly `prealloc_count` Units (`ac4dc1205`
    // shrank the backing from `prealloc_columns` Units to match the new
    // unit-sized dynamic storage) -- the first
    // resize consumes the whole buffer, so a second allocating resize would fail
    // with OutOfMemory. rustty's tabstops allocate through the generic `A`
    // parameter and the module already carries a tripwire fail point at exactly
    // the allocation site (`ResizeFp::DynamicAlloc`), so we arm THAT instead: it
    // asserts the same property (the second resize must not reach the allocator)
    // more directly, and without depending on a fixed-buffer allocator's
    // exhaustion semantics matching Zig's.
    #[test]
    fn tabstops_resize_to_existing_capacity_does_not_allocate() {
        use rustty_util::tripwire::ResetMode;
        let mut t: Tabstops = Tabstops::default();
        let cap = t.capacity();

        // First resize: grows the dynamic buffer to exactly `cap` units.
        t.resize(Global, cap * 2).unwrap();

        // Second resize to the SAME width must be satisfied by the buffer that
        // already exists. Arm the allocation fail point: if `resize` reaches it,
        // this returns Err and the unwrap below panics.
        resize_tw_error_always(ResizeFp::DynamicAlloc);

        // The resize must SUCCEED -- it is satisfied by the buffer that already
        // exists. Before `5caf20e3e` the capacity check was `<` rather than
        // `<=`, so an exactly-sized buffer was treated as insufficient, this
        // call reached the (armed) allocation point, and the unwrap panicked.
        t.resize(Global, cap * 2).unwrap();

        // ...and the armed fail point must never have been REACHED. `end`
        // returns UntrippedError for exactly that case: armed, never fired.
        // An `Ok(())` here would mean the allocator WAS called -- i.e. the
        // redundant allocation is back -- so the error is the passing outcome.
        assert!(
            resize_tw_end(ResetMode::Reset).is_err(),
            "resize to an already-sufficient width reached the allocator"
        );

        t.deinit(Global);
    }

    // test "Tabstops: dynamic allocations"
    #[test]
    fn tabstops_dynamic_allocations() {
        // var t: Tabstops = .{};
        // defer t.deinit(testing.allocator);
        let mut t: Tabstops = Tabstops::default();

        // Grow by less than one unit to verify the allocation rounds up.
        // const cap = t.capacity();
        let cap = t.capacity();
        // try t.resize(testing.allocator, cap + 5);
        t.resize(Global, cap + 5).unwrap();
        // try testing.expectEqual(cap + unit_bits, t.capacity());
        assert_eq!(cap + UNIT_BITS, t.capacity());

        // Set something that was out of range of the first
        // t.set(cap + 4);
        t.set(cap + 4);
        // try testing.expect(t.get(cap + 4));
        assert!(t.get(cap + 4));
        // try testing.expect(!t.get(cap + 3));
        assert!(!t.get(cap + 3));

        // Growing again preserves existing stops and clears the new unit.
        // try t.resize(testing.allocator, cap + unit_bits + 1);
        t.resize(Global, cap + UNIT_BITS + 1).unwrap();
        // try testing.expect(t.get(cap + 4));
        assert!(t.get(cap + 4));
        // try testing.expect(!t.get(cap + unit_bits));
        assert!(!t.get(cap + UNIT_BITS));

        // Prealloc still works
        // try testing.expect(!t.get(5));
        assert!(!t.get(5));

        t.deinit(Global);
    }

    // test "Tabstops: interval"
    #[test]
    fn tabstops_interval() {
        // var t: Tabstops = try init(testing.allocator, 80, 4);
        // defer t.deinit(testing.allocator);
        let t: Tabstops = Tabstops::init(Global, 80, 4).unwrap();
        // try testing.expect(!t.get(0));
        assert!(!t.get(0));
        // try testing.expect(t.get(4));
        assert!(t.get(4));
        // try testing.expect(!t.get(5));
        assert!(!t.get(5));
        // try testing.expect(t.get(8));
        assert!(t.get(8));

        t.deinit(Global);
    }

    // test "Tabstops: interval with zero columns"
    // PORT NOTE (parity 3d08161b5): test hunk of the upstream empty-tabstop-range
    // fix; init with zero columns used to underflow in `reset`.
    #[test]
    fn tabstops_interval_with_zero_columns() {
        // var t: Tabstops = try init(testing.allocator, 0, 8);
        // defer t.deinit(testing.allocator);
        let t: Tabstops = Tabstops::init(Global, 0, 8).unwrap();

        // try testing.expectEqual(@as(usize, 0), t.cols);
        assert_eq!(0usize, t.cols);

        t.deinit(Global);
    }

    // test "Tabstops: count on 80"
    #[test]
    fn tabstops_count_on_80() {
        // https://superuser.com/questions/710019/why-there-are-11-tabstops-on-a-80-column-console

        // var t: Tabstops = try init(testing.allocator, 80, 8);
        // defer t.deinit(testing.allocator);
        let t: Tabstops = Tabstops::init(Global, 80, 8).unwrap();

        // Count the tabstops
        // const count: usize = count: {
        //     var v: usize = 0;
        //     var i: usize = 0;
        //     while (i < 80) : (i += 1) {
        //         if (t.get(i)) {
        //             v += 1;
        //         }
        //     }
        //
        //     break :count v;
        // };
        let count: usize = {
            let mut v: usize = 0;
            let mut i: usize = 0;
            while i < 80 {
                if t.get(i) {
                    v += 1;
                }
                i += 1;
            }

            v
        };

        // try testing.expectEqual(@as(usize, 9), count);
        assert_eq!(9usize, count);

        t.deinit(Global);
    }

    // test "Tabstops: resize alloc failure preserves state"
    #[test]
    // WS18 release lane: this test injects a failure through a `rustty_util::tripwire`
    // fail point, and `tripwire::Module::enabled()` is `cfg!(debug_assertions)` (the
    // faithful stand-in for Zig's `builtin.is_test`; see its PORT NOTE). Without
    // `debug_assertions` the fail point never trips, so the test can only run in
    // debug — where it still runs and passes. NOT disabled to hide a failure.
    #[cfg_attr(not(debug_assertions), ignore)]
    fn tabstops_resize_alloc_failure_preserves_state() {
        use rustty_util::tripwire::ResetMode;

        // This test verifies that if resize() fails during allocation,
        // the original cols value is preserved (not corrupted).
        // var t: Tabstops = try init(testing.allocator, 80, 8);
        // defer t.deinit(testing.allocator);
        let mut t: Tabstops = Tabstops::init(Global, 80, 8).unwrap();

        // const original_cols = t.cols;
        let original_cols = t.cols;

        // Trigger allocation failure when resizing beyond prealloc
        // resize_tw.errorAlways(.dynamic_alloc, error.OutOfMemory);
        resize_tw_error_always(ResizeFp::DynamicAlloc);
        // const result = t.resize(testing.allocator, prealloc_columns * 2);
        let result = t.resize(Global, PREALLOC_COLUMNS * 2);
        // try testing.expectError(error.OutOfMemory, result);
        assert_eq!(Err(OutOfMemory), result);
        // try resize_tw.end(.reset);
        resize_tw_end(ResetMode::Reset).unwrap();

        // cols should be unchanged after failed resize
        // try testing.expectEqual(original_cols, t.cols);
        assert_eq!(original_cols, t.cols);

        t.deinit(Global);
    }
}
