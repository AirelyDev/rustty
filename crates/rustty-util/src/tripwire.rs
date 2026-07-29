//! A library for injecting failures into Zig code for the express
//! purpose of testing error handling paths.
//!
//! Improper `errdefer` is one of the highest sources of bugs in Zig code.
//! Many `errdefer` points are hard to exercise in unit tests and rare
//! to encounter in production, so they often hide bugs. Worse, error
//! scenarios are most likely to put your code in an unexpected state
//! that can result in future assertion failures or memory safety issues.
//!
//! This module aims to solve this problem by providing a way to inject
//! errors at specific points in your code during unit tests, allowing you
//! to test every possible error path.
//!
//! # Usage
//!
//! To use this package, create a `tripwire.module` for each failable
//! function you want to test. The enum must be hand-curated to be the
//! set of fail points, and the error set comes directly from the function
//! itself.
//!
//! Pepper your function with `try tw.check` calls wherever you want to
//! have a testable failure point. You don't need every "try" to have
//! an associated tripwire check, only the ones you care about testing.
//! Usually, this is going to be the points where you want to test
//! errdefer logic above it.
//!
//! In unit tests, add `try tw.errorAlways` or related calls to
//! configure expected failures. Then, call your function. Finally, always
//! call `try tw.end(.reset)` to verify your expectations were met and
//! to reset the tripwire module for future tests.
//!
//! ```zig
//! const tw = tripwire.module(enum { alloc_buf, open_file }, myFunction);
//!
//! fn myFunction() tw.Error!void {
//!     try tw.check(.alloc_buf);
//!     const buf = try allocator.alloc(u8, 1024);
//!     errdefer allocator.free(buf);
//!
//!     try tw.check(.open_file);
//!     const file = try std.fs.cwd().openFile("foo.txt", .{});
//!     // ...
//! }
//!
//! test "myFunction fails on alloc" {
//!     tw.errorAlways(.alloc_buf, error.OutOfMemory);
//!     try std.testing.expectError(error.OutOfMemory, myFunction());
//!     try tw.end(.reset);
//! }
//! ```
//!
//! ## Transitive Function Calls
//!
//! To test transitive calls, there are two schools of thought:
//!
//!   1. Put a failure point above the transitive call in the caller
//!      and assume the child function error handling works correctly.
//!
//!   2. Create another tripwire module for the child function and
//!      trigger failures there. This is recommended if the child function
//!      can't really be called in isolation (e.g. its an auxiliary function
//!      to a public API).
//!
//! Either works, its situationally dependent which is better.
//!
//! (The usage example above is written in Zig and is marked `zig` so it is not
//! compiled as a Rust doctest.)

#![allow(non_snake_case)]

use std::collections::HashMap;
use std::hash::Hash;

// log scope: .tripwire
//
// (In the Zig reference: `const log = std.log.scoped(.tripwire);`. We route
// untripped warnings through `log::warn` with the same `tripwire` scope.)

// Future ideas:
//
//   - Assert that the errors are actually tripped. e.g. you set a
//     errorAlways on a point, and want to verify it was tripped.
//   - Assert that every point is covered by at least one test. We
//     can probably use global state for this.
//   - Error only after a certain number of calls to a point.
//   - Error only on a range of calls (first 5 times, 2-7th time, etc.)
//
// I don't want to implement these until they're actually needed by
// some part of our codebase, but I want to list them here in case they
// do become useful.

/// Trait implemented by the fail-point enum `P`. In the Zig reference `P` is
/// any `enum`; here we require the small bundle of capabilities the module
/// needs to use the enum as a map key and to log its tag name. Derive-free
/// enums satisfy this with `#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]`.
pub trait FailPointKey: Copy + Eq + Hash + std::fmt::Debug {}
impl<T: Copy + Eq + Hash + std::fmt::Debug> FailPointKey for T {}

/// Error returned by `end` when there were untripped error expectations.
///
/// In the Zig reference this is `error{UntrippedError}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UntrippedError;

/// The reset mode for `end`: whether expectations are reset too.
///
/// In the Zig reference this is the inline `enum { reset, retain }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Reset,
    Retain,
}

/// A tripwire module that can be used to inject failures at specific points.
///
/// Outside of unit tests, this module is free and completely optimized away.
/// It takes up zero binary or runtime space and all function calls are
/// optimized out.
///
/// To use this module, add `check` (or related) calls prior to every
/// `try` operation that you want to be able to fail arbitrarily. Then,
/// in your unit tests, call the `error` family of functions to configure
/// when errors should be injected.
///
/// P is an enum type representing the failure points in the module.
/// E is the error set of possible errors that can be returned from the
/// failure points. You can use `anyerror` here but note you may have to
/// use `checkConstrained` to narrow down the error type when you call
/// it in your function (so your function can compile).
///
/// E may also be an error union type, in which case the error set of that
/// union is used as the error set for the tripwire module.
/// If E is a function, then the error set of the return value of that
/// function is used as the error set for the tripwire module.
//
// PORT NOTE: Zig's `module(P, E)` is a comptime function returning a type with
// type-static state (`var tripwires`). Rust has no per-generic-instantiation
// static, so we model the module as an instance value (`Module::new()`) holding
// the same state. The 4 reference tests construct a module per test, so the
// observable semantics are identical and there is no global mutable state to
// leak (Miri-clean).
pub struct Module<P: FailPointKey, E: Copy> {
    /// The configured tripwires for this module.
    tripwires: TripwireMap<P, E>,
}

/// The points this module can fail at. (Zig: `pub const FailPoint = P;`)
pub type FailPoint<P> = P;

type TripwireMap<P, E> = HashMap<P, Tripwire<E>>;

struct Tripwire<E: Copy> {
    /// Error to return when tripped
    err: E,

    /// The amount of times this tripwire has been reached. This
    /// is NOT the number of times it has tripped, since we may
    /// have mins for that.
    reached: usize,

    /// The minimum number of times this must be reached before
    /// tripping. After this point, it trips every time. This is
    /// a "before" check so if this is "1" then it'll trip the
    /// second time it's reached.
    min: usize,

    /// True if this has been tripped at least once.
    tripped: bool,
}

impl<P: FailPointKey, E: Copy> Module<P, E> {
    /// Whether our module is enabled or not. In the future we may
    /// want to make this a comptime parameter to the module.
    //
    // PORT NOTE: the Zig reference gates on `builtin.is_test`, which is true for
    // the WHOLE compilation unit when running tests (including dependency code).
    // Rust's `cfg!(test)` is only true inside the crate currently compiled with
    // `--test`, so a downstream crate's tripwire calls would see `enabled() ==
    // false` and never trip (this module would be dead weight cross-crate). The
    // faithful equivalent that propagates to dependency code is
    // `cfg!(debug_assertions)`: it is on for the `test`/`dev` profiles (where
    // unit tests run) and off for `release`, matching `builtin.is_test`'s effect
    // in practice. Consumers additionally `#[cfg(test)]`-gate their `check` call
    // sites, so production (non-test) builds never reach `check` regardless.
    pub const fn enabled() -> bool {
        cfg!(debug_assertions)
    }

    /// Construct a new tripwire module instance. (Zig models this as a type;
    /// see the PORT NOTE on `Module`.)
    pub fn new() -> Self {
        Module {
            tripwires: HashMap::new(),
        }
    }

    /// Check for a failure at the given failure point. These should
    /// be placed directly before the `try` operation that may fail.
    #[inline]
    pub fn check(&mut self, point: FailPoint<P>) -> Result<(), E> {
        if !Self::enabled() {
            return Ok(());
        }
        self.checkConstrained(point)
    }

    /// Same as check but allows specifying a custom error type for the
    /// return value. This must be a subset of the module's Error type
    /// and will produce a runtime error if the configured tripwire
    /// error can't be cast to the ConstrainedError type.
    //
    // PORT NOTE: Zig narrows to `comptime ConstrainedError`; in Rust we express
    // the narrowing via `TryFrom<E>` for the constrained error `CE`, and panic
    // (a runtime error, matching Zig's behavior) if the configured error can't
    // be cast to `CE`. The plain `check` above uses the identity case where
    // `CE == E`.
    #[inline]
    pub fn checkConstrained<CE>(&mut self, point: FailPoint<P>) -> Result<(), CE>
    where
        CE: TryFrom<E>,
    {
        if !Self::enabled() {
            return Ok(());
        }
        let tripwire = match self.tripwires.get_mut(&point) {
            Some(t) => t,
            None => return Ok(()),
        };
        tripwire.reached += 1;
        if tripwire.reached <= tripwire.min {
            return Ok(());
        }
        tripwire.tripped = true;
        let err = tripwire.err;
        match CE::try_from(err) {
            Ok(ce) => Err(ce),
            Err(_) => panic!("tripwire error can't be cast to the ConstrainedError type"),
        }
    }

    /// Mark a failure point to always trip with the given error.
    pub fn errorAlways(&mut self, point: FailPoint<P>, err: E) {
        self.errorAfter(point, err, 0);
    }

    /// Mark a failure point to trip with the given error after
    /// the failure point is reached at least `min` times. A value of
    /// zero is equivalent to `errorAlways`.
    pub fn errorAfter(&mut self, point: FailPoint<P>, err: E, min: usize) {
        self.tripwires.insert(
            point,
            Tripwire {
                err,
                reached: 0,
                min,
                tripped: false,
            },
        );
    }

    /// Ends the tripwire session. This will raise an error if there
    /// were untripped error expectations. The reset mode specifies
    /// whether expectations are reset too. Expectations are always reset,
    /// even if this returns an error.
    pub fn end(&mut self, reset_mode: ResetMode) -> Result<(), UntrippedError> {
        let mut untripped: bool = false;
        for (key, value) in self.tripwires.iter() {
            if !value.tripped {
                log_warn_untripped(key);
                untripped = true;
            }
        }

        match reset_mode {
            ResetMode::Reset => self.reset(),
            ResetMode::Retain => {}
        }

        if untripped {
            return Err(UntrippedError);
        }
        Ok(())
    }

    /// Unset all the tripwires. You should usually call `end` instead.
    pub fn reset(&mut self) {
        self.tripwires = HashMap::new();
    }
}

impl<P: FailPointKey, E: Copy> Default for Module<P, E> {
    fn default() -> Self {
        Self::new()
    }
}

/// Construct a tripwire module for the given fail-point enum `P` and error
/// type `E`. (Zig: `tripwire.module(P, E)`.)
///
/// The error set used for failures at the failure points.
/// (Zig: `pub const Error`.) In Rust the error type is the type parameter `E`.
///
/// In the Zig reference `E` may be an error set, an error-union type, or a
/// function whose return-value error set is used. In Rust the error type is
/// passed directly as the type parameter `E`; the caller selects it explicitly.
pub fn module<P: FailPointKey, E: Copy>() -> Module<P, E> {
    Module::new()
}

/// Log a warning for an untripped failure point.
///
/// Zig: `log.warn("untripped point={s}", .{@tagName(entry.key)});`
fn log_warn_untripped<P: std::fmt::Debug>(key: &P) {
    // Use the Debug rendering as the tag name; for a fieldless enum this is the
    // variant name, matching Zig's `@tagName`.
    eprintln!("warning(tripwire): untripped point={:?}", key);
}

/// Our calling convention is inline if our tripwire module is
/// NOT enabled, so that all calls to `check` are optimized away.
//
// PORT NOTE: in the Zig reference this controls the `callconv` of `check` /
// `checkConstrained` (inline when not enabled). In Rust we mark those methods
// `#[inline]` and short-circuit on `!enabled()`, which the optimizer collapses
// to nothing in non-test builds.
#[allow(dead_code)]
fn callingConvention() -> bool {
    !Module::<DummyPoint, DummyError>::enabled()
}

// Helper types only used to give `callingConvention` a concrete signature
// mirroring the Zig private fn; they carry no behavior.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum DummyPoint {}
#[derive(Clone, Copy)]
struct DummyError;

#[cfg(test)]
mod tests {
    use super::*;

    // Error set used by the anonymous test (Zig `anyerror` narrowed to the
    // single error actually injected). UntrippedError is distinct (see `end`).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum AnyError {
        OutOfMemory,
    }

    #[test]
    // WS18 release lane: this test injects a failure through a `rustty_util::tripwire`
    // fail point, and `tripwire::Module::enabled()` is `cfg!(debug_assertions)` (the
    // faithful stand-in for Zig's `builtin.is_test`; see its PORT NOTE). Without
    // `debug_assertions` the fail point never trips, so the test can only run in
    // debug — where it still runs and passes. NOT disabled to hide a failure.
    #[cfg_attr(not(debug_assertions), ignore)]
    fn test_anon_0() {
        let mut io: Module<Io, AnyError> = module();

        // Reset should work
        io.end(ResetMode::Reset).unwrap();

        // By default, its pass-through
        io.check(Io::Read).unwrap();

        // Always trip
        io.errorAlways(Io::Read, AnyError::OutOfMemory);
        assert!(matches!(io.check(Io::Read), Err(AnyError::OutOfMemory)));
        // Happens again
        assert!(matches!(io.check(Io::Read), Err(AnyError::OutOfMemory)));
        io.end(ResetMode::Reset).unwrap();
    }

    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    enum Io {
        Read,
        #[allow(dead_code)]
        Write,
    }

    #[test]
    fn module_as_error_set() {
        // const io = module(enum { read, write }, @TypeOf((struct {
        //     fn func() error{ Foo, Bar }!void { return error.Foo; }
        // }).func));
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        enum IoModErr {
            #[allow(dead_code)]
            Read,
            #[allow(dead_code)]
            Write,
        }
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[allow(dead_code)]
        enum FooBar {
            Foo,
            Bar,
        }
        fn func() -> Result<(), FooBar> {
            Err(FooBar::Foo)
        }
        let _ = func;

        let mut io: Module<IoModErr, FooBar> = module();
        io.end(ResetMode::Reset).unwrap();
    }

    #[test]
    // WS18 release lane: this test injects a failure through a `rustty_util::tripwire`
    // fail point, and `tripwire::Module::enabled()` is `cfg!(debug_assertions)` (the
    // faithful stand-in for Zig's `builtin.is_test`; see its PORT NOTE). Without
    // `debug_assertions` the fail point never trips, so the test can only run in
    // debug — where it still runs and passes. NOT disabled to hide a failure.
    #[cfg_attr(not(debug_assertions), ignore)]
    fn errorAfter() {
        let mut io: Module<Io, AnyError> = module();
        // Trip after 2 calls (on the 3rd call)
        io.errorAfter(Io::Read, AnyError::OutOfMemory, 2);

        // First two calls succeed
        io.check(Io::Read).unwrap();
        io.check(Io::Read).unwrap();

        // Third call and on trips
        assert!(matches!(io.check(Io::Read), Err(AnyError::OutOfMemory)));
        assert!(matches!(io.check(Io::Read), Err(AnyError::OutOfMemory)));

        io.end(ResetMode::Reset).unwrap();
    }

    #[test]
    fn errorAfter_untripped_error_if_min_not_reached() {
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
        enum ReadOnly {
            Read,
        }
        let mut io: Module<ReadOnly, AnyError> = module();
        io.errorAfter(ReadOnly::Read, AnyError::OutOfMemory, 2);
        // Only call once, not enough to trip
        io.check(ReadOnly::Read).unwrap();
        // end should fail because tripwire was set but never tripped
        assert!(matches!(io.end(ResetMode::Reset), Err(UntrippedError)));
    }
}
