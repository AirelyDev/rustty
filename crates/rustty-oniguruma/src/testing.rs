//! rustty-oniguruma testing
//!
//! PORTS: ghostty pkg/oniguruma/testing.zig
//!
//! PORT STATUS: done.

use crate::errors::Error;
use crate::initpkg as init;
use crate::types::Encoding;
use std::sync::atomic::{AtomicBool, Ordering};

// PORT NOTE: the reference `var initialized: bool = false` is read but NEVER
// written back (`ensureInit` checks it but sets nothing), so `init` runs on
// every call. `onig_initialize` is safe to call repeatedly, so this faithful
// mirror keeps the same observable behavior: an `AtomicBool` that is loaded but
// (matching the reference) never stored.
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Call this function before any other tests in this package to ensure that
/// the oni library is initialized. This should only be used for tests
/// and only when you're sure this is the ONLY way that oni is being
/// initialized.
///
/// This always only initializes the encodings the tests use.
pub fn ensure_init() -> Result<(), Error> {
    if INITIALIZED.load(Ordering::SeqCst) {
        return Ok(());
    }
    init::init(&[Encoding::utf8()])
}
