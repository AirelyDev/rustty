//! RUSTTY-ONLY `#[cfg(test)]` scaffolding: the strict-mode pivot for every
//! "no Metal device ⇒ skip" arm in this crate. NOT a port — it has no ghostty
//! analogue.
//!
//! # Why this exists
//!
//! `.port/phase-2/GPU_RD_BLUEPRINT.md:21-24` records project risk **F5**: the
//! Metal-backed tests (goldens, resource leaves, draw plumbing, the windowed
//! lifecycle KAT, and the restored-diagnostics tests) all skip cleanly when
//! `MTLCreateSystemDefaultDevice()` returns nothing, so a CI leg that is SUPPOSED
//! to have a GPU can go **green after executing none of them**. The blueprint's
//! mitigation is explicit — pin a known-Metal runner AND make the job fail if the
//! tests skip — and its `:392-395` claimed that was "guarded in `ci.sh`". It was
//! not: `grep -in metal .port/tools/ci.sh` returned nothing. F5's mitigation had
//! never been implemented, so the only assurance those tests were a gate rather
//! than a hope was an unenforced property of the GitHub runner image.
//!
//! This module implements it, deliberately as a near-verbatim sibling of the
//! OpenGL precedent [`crate::opengl::gl::test_support`] (audit W8-03,
//! `RUSTTY_REQUIRE_GL`) — same env variable shape, same split-core structure, same
//! `#[track_caller]` pivot, same proving tests. Keep the two in step.
//!
//! # The contract
//!
//! - **Default (unset):** unchanged. Every skip arm prints its `skipping <test>`
//!   line and returns, exactly as before. A developer without a GPU, and every
//!   non-macOS build, is unaffected.
//! - **`RUSTTY_REQUIRE_METAL=1`:** every skip arm PANICS instead, naming the test
//!   and the device error. The macOS CI leg exports it
//!   (`.github/workflows/ci.yml`), so on that leg the Metal tests are EXECUTED
//!   evidence, and a runner image that silently loses its Metal device turns the
//!   job red instead of green.

/// RUSTTY-ONLY strict mode (GPU_RD_BLUEPRINT risk F5): `RUSTTY_REQUIRE_METAL=1`
/// in the environment turns every clean "no Metal device" skip in this crate
/// into a hard test failure.
pub(crate) fn metal_required() -> bool {
    metal_required_from(std::env::var_os("RUSTTY_REQUIRE_METAL").as_deref())
}

/// The env-independent core of [`metal_required`], split out so the unit tests
/// below can drive both arms WITHOUT mutating process-global environment
/// (`set_var` would race every concurrently running Metal test that consults the
/// same variable under the parallel libtest harness).
fn metal_required_from(value: Option<&std::ffi::OsStr>) -> bool {
    value == Some(std::ffi::OsStr::new("1"))
}

/// The strict-mode pivot. EVERY clean-skip arm calls this immediately before
/// printing its `skipping <test>` line: a no-op by default, a panic under
/// `RUSTTY_REQUIRE_METAL=1` — so in strict mode an unavailable device is a
/// FAILURE, never a silent green. `err` is the device error that would have
/// justified the skip, carried into the panic message so the strict-mode failure
/// is as diagnosable as the skip line it replaces.
#[track_caller]
pub(crate) fn require_metal_or_skip(test_name: &str, err: &dyn std::fmt::Display) {
    require_metal_or_skip_impl(metal_required(), test_name, err);
}

/// The env-independent core of [`require_metal_or_skip`] (same rationale as
/// [`metal_required_from`]: unit-testable without touching process env).
#[track_caller]
fn require_metal_or_skip_impl(required: bool, test_name: &str, err: &dyn std::fmt::Display) {
    assert!(
        !required,
        "Metal device required but unavailable (RUSTTY_REQUIRE_METAL=1): \
         {test_name} would have skipped: {err}"
    );
}

/// R14 sibling of [`metal_required`] for the one live-display dependency in this
/// crate's macOS tests: a CoreVideo display link (`createWithActiveCGDisplays`)
/// needs an active display, which a headless or locked session lacks.
/// `RUSTTY_REQUIRE_DISPLAY=1` turns that skip into a failure. It is a separate
/// variable from `RUSTTY_REQUIRE_METAL` because a machine can have a Metal
/// device and no active display (a locked session), and because whether a CI
/// runner has an active display is not established: CI does NOT set it yet.
pub(crate) fn display_required() -> bool {
    metal_required_from(std::env::var_os("RUSTTY_REQUIRE_DISPLAY").as_deref())
}

/// The display pivot: a no-op by default (the caller prints its skip line and
/// returns), a panic under `RUSTTY_REQUIRE_DISPLAY=1`.
#[track_caller]
pub(crate) fn require_display_or_skip(test_name: &str, err: &dyn std::fmt::Display) {
    require_display_or_skip_impl(display_required(), test_name, err);
}

#[track_caller]
fn require_display_or_skip_impl(required: bool, test_name: &str, err: &dyn std::fmt::Display) {
    assert!(
        !required,
        "active display required but unavailable (RUSTTY_REQUIRE_DISPLAY=1): \
         {test_name} would have skipped: {err}"
    );
}

/// PROVING tests for the F5 strict mode: the pivot must be a no-op by default
/// and a panic under `RUSTTY_REQUIRE_METAL=1`. They drive the `*_from`/`*_impl`
/// cores directly — never `std::env::set_var` — so they are race-free under the
/// parallel harness and require no Metal device (they run on every macOS host,
/// including one where every live-Metal test skips).
#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::{metal_required_from, require_display_or_skip_impl, require_metal_or_skip_impl};

    #[test]
    fn metal_required_only_when_env_is_exactly_one() {
        assert!(!metal_required_from(None));
        assert!(!metal_required_from(Some(OsStr::new(""))));
        assert!(!metal_required_from(Some(OsStr::new("0"))));
        assert!(!metal_required_from(Some(OsStr::new("true"))));
        assert!(metal_required_from(Some(OsStr::new("1"))));
    }

    #[test]
    fn skip_pivot_is_noop_when_metal_not_required() {
        // Must return, preserving the clean-skip contract.
        require_metal_or_skip_impl(false, "some_test", &"no device");
    }

    #[test]
    #[should_panic(expected = "Metal device required but unavailable (RUSTTY_REQUIRE_METAL=1)")]
    fn skip_pivot_panics_when_metal_required() {
        require_metal_or_skip_impl(true, "some_test", &"no device");
    }

    #[test]
    fn display_pivot_is_noop_when_not_required() {
        require_display_or_skip_impl(false, "some_test", &"no active display");
    }

    #[test]
    #[should_panic(expected = "active display required but unavailable (RUSTTY_REQUIRE_DISPLAY=1)")]
    fn display_pivot_panics_when_required() {
        require_display_or_skip_impl(true, "some_test", &"no active display");
    }
}
