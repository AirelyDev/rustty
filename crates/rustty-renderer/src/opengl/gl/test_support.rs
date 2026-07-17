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
/// 1. **`eglTerminate` on a shared display.** `OffscreenContext::drop`'s CAVEAT
///    (`sys.rs`) spells it out: `eglGetPlatformDisplay` hands back the SAME
///    `EGLDisplay` for the same platform + native display, and `eglTerminate`
///    marks EVERY resource on that display for deletion. So two concurrent
///    `OffscreenContext`s share a display and the first one dropped invalidates
///    the other. Serializing means the terminate always lands after the other
///    test is done with the display, and the next `eglInitialize` re-initializes
///    it — which is legal and is the sequence this lock produces.
/// 2. **`gl::load_with` writes process-global statics.** The `gl` crate's entry
///    points are `static mut` slots (glad's `glad.context` analogue, which
///    ghostty likewise has exactly one of); two threads calling `load_with`
///    concurrently is a data race regardless of both writing the same values.
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

/// A live, GL-loaded context for one test, plus the [`gl_lock`] guard.
pub(crate) struct GlTestContext {
    /// FIELD ORDER IS LOAD-BEARING: Rust drops fields in declaration order, so
    /// the context (and its `eglTerminate`) must be destroyed BEFORE the lock is
    /// released, or the next test can start on a display that is about to be
    /// terminated out from under it.
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
