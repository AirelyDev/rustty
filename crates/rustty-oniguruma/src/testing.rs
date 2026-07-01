//! rustty-oniguruma testing
//!
//! PORTS: ghostty pkg/oniguruma/testing.zig
//!
//! PORT STATUS: done.

use crate::errors::Error;
use crate::initpkg as init;
use crate::types::Encoding;
use std::sync::OnceLock;

// PORT NOTE (parallel-test correctness — DELIBERATE DIVERGENCE from the Zig
// reference): the reference `ensureInit` reads a `var initialized: bool = false`
// that it NEVER writes back, so `onig_initialize` runs on every call. That is
// harmless under Zig's `zig test`, which runs a file's tests SERIALLY — the
// repeated `onig_initialize` calls never overlap. Rust's `cargo test` runs
// `#[test]`s CONCURRENTLY across threads, and `onig_initialize` is NOT
// thread-safe: it initializes Oniguruma's process-global tables (encodings,
// default syntax, the shared char-class table) non-atomically. Two tests calling
// `ensure_init` at once therefore raced inside `onig_initialize`, corrupting the
// C heap — surfacing as a SIGTRAP or, when the clobbered bytes landed next to a
// terminal memory-pool arena node, as an `ArenaAllocator::usable`
// `data - buf_node_size()` underflow in an unrelated crate.
//
// The correct, faithful-to-INTENT behavior (the doc comment below is explicit:
// "ensure that the oni library is initialized" — a one-time init) is to run
// `onig_initialize` EXACTLY ONCE per process, thread-safely. `OnceLock` gives
// that: the initializer runs once, concurrent callers block until it completes,
// and the (Copy) result is cached and handed back to every caller.
static INIT: OnceLock<Result<(), Error>> = OnceLock::new();

/// Call this function before any other tests in this package to ensure that
/// the oni library is initialized. This should only be used for tests
/// and only when you're sure this is the ONLY way that oni is being
/// initialized.
///
/// This always only initializes the encodings the tests use.
pub fn ensure_init() -> Result<(), Error> {
    *INIT.get_or_init(|| init::init(&[Encoding::utf8()]))
}
