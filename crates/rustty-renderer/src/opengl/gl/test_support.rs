//! RUSTTY-ONLY `#[cfg(test)]` scaffolding: stands a live GL context up for the
//! `opengl::gl` leaf tests. NOT a port — it has no ghostty analogue, for the same
//! reason [`sys::OffscreenContext`] has none (GDK owns context creation in the
//! reference, so ghostty's `pkg/opengl` has nothing to stand up).
//!
//! ONE CONTEXT PER TEST, mirroring `metal.rs`'s `MTLCreateSystemDefaultDevice()`
//! -per-test shape rather than a shared global one, so the leaf tests stay
//! independent of each other's GL state (a leaked binding in one test must not be
//! visible to the next).
//!
//! SERIALIZED, though — see [`gl_lock`]. Per-test contexts and a process-wide
//! lock are not in tension: the lock makes the contexts sequential in TIME, it
//! does not make them shared.

use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::opengl::gl::sys::{self, OffscreenContext};

/// The process-wide GL-test lock. Every test that stands up an
/// [`OffscreenContext`] MUST hold it for the context's whole lifetime.
///
/// TWO independent hazards make this mandatory, both of which the default
/// (thread-parallel) Rust test harness would otherwise hit as soon as a second GL
/// test exists — which is exactly what this task adds:
///
/// 1. **`eglTerminate` on a shared display.** `eglGetPlatformDisplay` hands
///    back the SAME `EGLDisplay` for the same platform + native display, and
///    `eglTerminate` marks EVERY resource on that display for deletion, so two
///    concurrent `OffscreenContext`s shared a display and the first one dropped
///    invalidated the other. P7-W7 T1 (DECISIONS B10) removed the terminate, as
///    the reference never terminates, so this hazard is gone.
/// 2. **`gl::load_with` wrote process-global statics.** P7-W7 T1 (DECISIONS B9)
///    replaced them with a per-thread table (`glad`), so this hazard is gone too.
///
/// The lock stays: every GL test still assumes it is alone on the driver (the
/// leak tests count live GL object names, the log-capture tests expect exactly
/// their own debug-callback records), and serializing keeps them independent.
///
/// `sys.rs`'s own `offscreen_egl_context_reaches_gl43_core` takes this lock
/// directly rather than going through [`offscreen_gl`], because it asserts on the
/// specific `load_gl` error it gets and so cannot use a helper that collapses
/// them into `None`.
pub(crate) fn gl_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    // A GL test that panics while holding the lock poisons it. Poisoning carries
    // no information here — the lock guards no invariant of the `()` it wraps,
    // only the ORDERING of the EGL/loader calls — and letting it turn every
    // subsequent GL test into an unwrap panic would report one failure as N.
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// RUSTTY-ONLY strict mode (audit W8-03): `RUSTTY_REQUIRE_GL=1` in the
/// environment turns every clean skip — [`offscreen_gl`]'s two arms here and
/// the sibling skip arms in `sys`, `opengl_impl` and `generic::linux_impl` —
/// into a hard test failure. Without it, a CI leg that is SUPPOSED to provide a
/// live GL context can go green after skipping all 11 live-GL tests, proving
/// nothing; the Ubuntu CI leg installs Mesa (llvmpipe, surfaceless EGL) and
/// exports this variable so those tests are EXECUTED evidence there. Unset (the
/// default, and every macOS build) preserves the clean-skip contract — the
/// `metal.rs` "no device ⇒ skip" precedent — byte-for-byte.
pub(crate) fn gl_required() -> bool {
    gl_required_from(std::env::var_os("RUSTTY_REQUIRE_GL").as_deref())
}

/// The env-independent core of [`gl_required`], split out so the unit tests
/// below can drive both arms WITHOUT mutating process-global environment
/// (`set_var` would race every concurrently running GL test that consults the
/// same variable under the parallel libtest harness).
fn gl_required_from(value: Option<&std::ffi::OsStr>) -> bool {
    value == Some(std::ffi::OsStr::new("1"))
}

/// The strict-mode pivot. EVERY clean-skip arm calls this immediately before
/// printing its `skipping <test>` line: a no-op by default, a panic under
/// `RUSTTY_REQUIRE_GL=1` — so in strict mode an unavailable context is a
/// FAILURE, never a silent green. `err` is the context/version error that
/// would have justified the skip, carried into the panic message so the
/// strict-mode failure is as diagnosable as the skip line it replaces.
#[track_caller]
pub(crate) fn require_gl_or_skip(test_name: &str, err: &dyn std::fmt::Display) {
    require_gl_or_skip_impl(gl_required(), test_name, err);
}

/// The env-independent core of [`require_gl_or_skip`] (same rationale as
/// [`gl_required_from`]: unit-testable without touching process env).
#[track_caller]
fn require_gl_or_skip_impl(required: bool, test_name: &str, err: &dyn std::fmt::Display) {
    assert!(
        !required,
        "GL context required but unavailable (RUSTTY_REQUIRE_GL=1): \
         {test_name} would have skipped: {err}"
    );
}

/// RUSTTY-ONLY strict mode for DMA-BUF export (P7-W7 T1, R14): under
/// `RUSTTY_REQUIRE_DMABUF=1` a test that needs `EGL_MESA_image_dma_buf_export`
/// FAILS when the display does not offer it, instead of skipping. Separate from
/// [`gl_required`] because a host can have a perfectly good GL 4.3 context
/// (llvmpipe) and still no DMA-BUF export, so the GL lane cannot demand it.
pub(crate) fn dmabuf_required() -> bool {
    gl_required_from(std::env::var_os("RUSTTY_REQUIRE_DMABUF").as_deref())
}

/// The DMA-BUF strict-mode pivot: [`require_gl_or_skip`]'s twin over
/// [`dmabuf_required`].
#[track_caller]
pub(crate) fn require_dmabuf_or_skip(test_name: &str, err: &dyn std::fmt::Display) {
    require_dmabuf_or_skip_impl(dmabuf_required(), test_name, err);
}

/// The env-independent core of [`require_dmabuf_or_skip`].
#[track_caller]
fn require_dmabuf_or_skip_impl(required: bool, test_name: &str, err: &dyn std::fmt::Display) {
    assert!(
        !required,
        "DMA-BUF export required but unavailable (RUSTTY_REQUIRE_DMABUF=1): \
         {test_name} would have skipped: {err}"
    );
}

/// A live, GL-loaded context for one test, plus the [`gl_lock`] guard.
pub(crate) struct GlTestContext {
    /// FIELD ORDER IS LOAD-BEARING: Rust drops fields in declaration order, so
    /// the context is destroyed BEFORE the lock is released and the next test
    /// never overlaps it (see [`gl_lock`]).
    _ctx: OffscreenContext,
    _guard: MutexGuard<'static, ()>,
}

/// Stands up an [`OffscreenContext`], loads the GL entry points into it, and
/// holds [`gl_lock`] for as long as the returned value lives.
///
/// Returns `None` — after printing a `skipping <test_name>` line — when the host
/// cannot provide an OpenGL [`sys::MIN_VERSION_MAJOR`].[`sys::MIN_VERSION_MINOR`]
/// core context. That is the clean-skip contract `metal.rs` established for "no
/// Metal device", and it is NARROW on purpose: absent context, absent version, or
/// a version under the floor. Anything else out of `load_gl` means the PORT is
/// broken on a context EGL already promised is >= 4.3 core, so it panics rather
/// than letting a real regression report itself as a skip.
pub(crate) fn offscreen_gl(test_name: &str) -> Option<GlTestContext> {
    let guard = gl_lock();

    let ctx = match OffscreenContext::init() {
        Ok(ctx) => ctx,
        Err(err) => {
            require_gl_or_skip(test_name, &err);
            eprintln!(
                "no headless OpenGL {}.{} core context ({err}); skipping {test_name}",
                sys::MIN_VERSION_MAJOR,
                sys::MIN_VERSION_MINOR,
            );
            return None;
        }
    };

    match sys::load_gl(&ctx) {
        Ok(_) => {}
        Err(err @ (sys::Error::VersionUnavailable | sys::Error::OpenGLOutdated { .. })) => {
            require_gl_or_skip(test_name, &err);
            eprintln!(
                "no headless OpenGL {}.{} core context ({err}); skipping {test_name}",
                sys::MIN_VERSION_MAJOR,
                sys::MIN_VERSION_MINOR,
            );
            return None;
        }
        Err(err) => panic!(
            "prepare_context failed on a live OpenGL {}.{} core context: {err}",
            sys::MIN_VERSION_MAJOR,
            sys::MIN_VERSION_MINOR,
        ),
    }

    Some(GlTestContext {
        _ctx: ctx,
        _guard: guard,
    })
}

/// PROVING tests for the W8-03 strict mode: the pivot must be a no-op by
/// default and a panic under `RUSTTY_REQUIRE_GL=1`. They drive the
/// `*_from`/`*_impl` cores directly — never `std::env::set_var` — so they are
/// race-free under the parallel harness and require no GL context (they run
/// everywhere, including hosts where every live-GL test skips).
#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::{gl_required_from, require_dmabuf_or_skip_impl, require_gl_or_skip_impl};

    #[test]
    fn gl_required_only_when_env_is_exactly_one() {
        assert!(!gl_required_from(None));
        assert!(!gl_required_from(Some(OsStr::new(""))));
        assert!(!gl_required_from(Some(OsStr::new("0"))));
        assert!(!gl_required_from(Some(OsStr::new("true"))));
        assert!(gl_required_from(Some(OsStr::new("1"))));
    }

    #[test]
    fn skip_pivot_is_noop_when_gl_not_required() {
        // Must return, preserving the clean-skip contract.
        require_gl_or_skip_impl(false, "some_test", &"no context");
    }

    #[test]
    #[should_panic(expected = "GL context required but unavailable (RUSTTY_REQUIRE_GL=1)")]
    fn skip_pivot_panics_when_gl_required() {
        require_gl_or_skip_impl(true, "some_test", &"no context");
    }

    #[test]
    fn dmabuf_skip_pivot_is_noop_when_not_required() {
        require_dmabuf_or_skip_impl(false, "some_test", &"no export");
    }

    /// The negative control of the `RUSTTY_REQUIRE_DMABUF` arm (R5).
    #[test]
    #[should_panic(expected = "DMA-BUF export required but unavailable (RUSTTY_REQUIRE_DMABUF=1)")]
    fn dmabuf_skip_pivot_panics_when_required() {
        require_dmabuf_or_skip_impl(true, "some_test", &"no export");
    }
}
