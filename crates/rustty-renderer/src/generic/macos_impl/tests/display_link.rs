//! Tests for `generic::macos_impl` (ghostty src/renderer/generic.zig, the
//! `Renderer(Metal)` arm) — the display-link lifecycle (`syncDisplayLink`) and the
//! `72cf50855` display-link/draw-mutex tests.

use super::*;

// PORT-SIDE REGRESSION (`a177ba90a`, gap-close): the display-link lifecycle
// refactor. The reference added no `test {}` for it — every branch that
// succeeds needs a real CoreVideo display — but the two GUARD CLAUSES of
// `sync_display_link` and the new `visible` field are observable without one,
// and they are exactly what the pre-refactor port got wrong: it created the
// link EAGERLY in `init` (so `create_with_active_cg_displays` failing on a
// locked session aborted renderer init instead of falling back to
// event-driven rendering) and it never stored visibility (so focusing an
// occluded surface spun vsync back up).
mod display_link_sync {
    use super::*;

    /// `if (!self.config.vsync) return;` — vsync off never creates a link,
    /// even from the one call site that carries a `draw_now`.
    #[test]
    fn sync_without_vsync_never_creates_a_link() {
        let mut r = match test_renderer(10, 3) {
            Some(r) => r,
            None => return, // skip w/o Metal
        };
        r.config.vsync = false;
        let draw_now = Async::init().expect("xev Async");
        r.sync_display_link(None, Some(draw_now));
        assert!(r.display_link.is_none());
        r.thread_exit();
        r.deinit();
    }

    /// `const callback = draw_now orelse return;` — a sync with no
    /// `draw_now` (focus/visibility changes, and a display change that
    /// arrives before `loop_enter`) must not create a link, because the
    /// output callback would have no userdata to notify.
    #[test]
    fn sync_without_draw_now_never_creates_a_link() {
        let mut r = match test_renderer(10, 3) {
            Some(r) => r,
            None => return, // skip w/o Metal
        };
        r.config.vsync = true;
        r.sync_display_link(None, None);
        assert!(r.display_link.is_none());

        // A display-id sync is the "re-adds itself" path and is equally
        // barred while there is nothing to call back into.
        r.sync_display_link(Some(1), None);
        assert!(r.display_link.is_none());
        r.thread_exit();
        r.deinit();
    }

    /// `self.visible = visible;` — the field the pre-refactor port dropped.
    /// `syncDisplayLink` gates start/stop on `visible` (since `97f57edcc` +
    /// `6688aa072`: `visible and (cells_rebuilt or animationWake() != null)`;
    /// before them `visible and focused`), so losing it silently degraded to
    /// dirtiness-only (then focus-only) gating.
    #[test]
    fn set_visible_tracks_visibility_state() {
        let mut r = match test_renderer(10, 3) {
            Some(r) => r,
            None => return, // skip w/o Metal
        };
        assert!(r.visible, "init starts visible (.visible = true)");

        r.set_visible(false);
        assert!(!r.visible);

        // Focusing while hidden must not flip visibility back.
        r.focused = false;
        r.set_focus(true).unwrap();
        assert!(!r.visible);

        r.set_visible(true);
        assert!(r.visible);
        r.thread_exit();
        r.deinit();
    }

    /// PORT-SIDE REGRESSION (`97f57edcc` + `6688aa072`, phase-6): the
    /// end-state predicate `should_run = visible and (cells_rebuilt or
    /// animationWake() != null)`. The link runs only while a rebuilt frame
    /// or an animation is pending, is parked (stopped) once idle, focus no
    /// longer participates, and a hidden surface never runs it. The
    /// reference added no `test {}` (it needs a real CoreVideo link), so
    /// this one creates the link for real and skips when
    /// `createWithActiveCGDisplays` fails (a headless/locked session).
    ///
    /// NEGATIVE CONTROL: restoring the pre-range `visible and focused`
    /// predicate fails the first assertion (the idle-but-focused surface
    /// keeps vsync spinning) or, with `focused == false`, the second (a
    /// dirty unfocused surface never starts it).
    #[test]
    fn sync_parks_the_link_while_idle_and_runs_it_while_dirty() {
        let mut r = match test_renderer(10, 3) {
            Some(r) => r,
            None => return, // skip w/o Metal
        };
        let running = |r: &Renderer| r.display_link.as_ref().map(|l| l.is_running());

        r.config.vsync = true;
        let draw_now = Async::init().expect("xev Async");
        r.sync_display_link(None, Some(draw_now));
        if r.display_link.is_none() {
            // No active display (headless / locked session): the
            // fallback-rendering arm, covered by the guard tests above.
            // R14: a skip here is visible, and a failure under
            // `RUSTTY_REQUIRE_DISPLAY=1`.
            r.thread_exit();
            r.deinit();
            crate::metal::test_support::require_display_or_skip(
                "sync_parks_the_link_while_idle_and_runs_it_while_dirty",
                &"createWithActiveCGDisplays failed (no active display)",
            );
            eprintln!(
                "no active display; skipping \
                 sync_parks_the_link_while_idle_and_runs_it_while_dirty"
            );
            return;
        }

        // Freshly created, visible, nothing rebuilt, no animation
        // (`test_renderer` has no custom shaders and no kitty clock):
        // idle -> parked, whatever the focus state.
        assert!(r.visible);
        assert!(!r.cells_rebuilt.load(Ordering::Relaxed));
        assert!(r.animation_wake().is_none());
        assert_eq!(running(&r), Some(false), "idle surface must park the link");

        // A rebuilt frame un-parks it, focused or not.
        r.cells_rebuilt.store(true, Ordering::Relaxed);
        r.sync_display_link(None, None);
        assert_eq!(running(&r), Some(true), "dirty surface must run the link");

        // `if (!display_link.isRunning()) start()` — a second sync while
        // already running is a no-op, not a double start.
        r.sync_display_link(None, None);
        assert_eq!(running(&r), Some(true));

        // Idle again (the draw consumed `cells_rebuilt`): parked.
        r.cells_rebuilt.store(false, Ordering::Relaxed);
        r.sync_display_link(None, None);
        assert_eq!(
            running(&r),
            Some(false),
            "idle surface must re-park the link"
        );

        // Non-visible windows never vsync, dirty or not.
        r.cells_rebuilt.store(true, Ordering::Relaxed);
        r.set_visible(false);
        assert_eq!(
            running(&r),
            Some(false),
            "hidden surface never runs the link"
        );

        // Visible again with the rebuilt frame still pending: runs.
        r.set_visible(true);
        assert_eq!(running(&r), Some(true));

        r.thread_exit();
        r.deinit();
    }

    /// UD362 fix round 3, gap (b): a no-redraw draw decides, under the
    /// lock, to park the display link; a health report refused before the
    /// link actually stops sees the link still running and skips its
    /// `draw_now` wake. After the draw the link is stopped and the report
    /// is still owed: `health_report_owed_raw` says so, which is what the
    /// render thread's `drawNowCallback` checks after its draw to arm the
    /// animation timer (so the report is not stranded), and the owed report
    /// is an animation wake for that timer.
    #[test]
    fn owed_report_refused_while_the_link_parks_is_not_stranded() {
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // skip w/o Metal
        };
        r.config.vsync = true;
        let draw_now = Async::init().expect("xev Async");
        r.sync_display_link(None, Some(draw_now));
        if r.display_link.is_none() {
            r.thread_exit();
            r.deinit();
            crate::metal::test_support::require_display_or_skip(
                "owed_report_refused_while_the_link_parks_is_not_stranded",
                &"createWithActiveCGDisplays failed (no active display)",
            );
            eprintln!(
                "no active display; skipping \
                 owed_report_refused_while_the_link_parks_is_not_stranded"
            );
            return;
        }
        r.health_report.arm(Some(draw_now));
        let running = |r: &Renderer| r.display_link.as_ref().map(|l| l.is_running());

        // A frame, then the link running (a rebuilt frame un-parks it).
        r.draw_frame(true).expect("first draw");
        r.cells_rebuilt.store(true, Ordering::Relaxed);
        r.sync_display_link(None, None);
        assert_eq!(running(&r), Some(true));
        r.cells_rebuilt.store(false, Ordering::Relaxed);

        // Between the no-redraw draw's unlock and its resync, a completion
        // is refused: its wake sees the link still running and skips.
        let skipped = std::rc::Rc::new(std::cell::Cell::new(None));
        let skipped_in = skipped.clone();
        BETWEEN_DRAW_UNLOCK_AND_SYNC.with(|h| {
            *h.borrow_mut() = Some(Box::new(move |this: *mut Renderer| {
                let report: &HealthReport = unsafe { &(*this).health_report };
                let refusing = SurfaceMailbox::new(std::sync::Arc::new(
                    crate::generic::surface_mailbox_tests::RecordingSink::refusing(),
                ));
                let health = AtomicHealth::new(crate::Health::Healthy);
                let sema = Semaphore::new();
                frame_completed_on(&health, &sema, &refusing, report, crate::Health::Unhealthy);
                skipped_in.set(Some(report.pending() && !report.wake()));
            }))
        });
        r.draw_frame(false).expect("no-redraw draw");
        BETWEEN_DRAW_UNLOCK_AND_SYNC.with(|h| *h.borrow_mut() = None);

        assert_eq!(
            skipped.get(),
            Some(true),
            "the refusal did not happen while the link was still running"
        );
        // The link is parked now, and the report still owed.
        assert_eq!(running(&r), Some(false));
        let self_ptr: *mut Renderer = &mut r;
        let lock = r.draw_mutex.clone();
        assert!(
            unsafe { Renderer::health_report_owed_raw(self_ptr, &lock) },
            "drawNowCallback would not see the owed report"
        );
        assert_eq!(
            r.animation_wake(),
            Some(AnimationWake {
                delay_ms: DRAW_INTERVAL_MS,
                kind: AnimationWakeKind::Draw
            })
        );

        r.health_report.arm(None);
        r.thread_exit();
        r.deinit();
    }
}

mod display_link_draw_mutex {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::time::Duration;

    struct SendRenderer(*mut Renderer);
    // SAFETY: the pointee outlives the thread (joined inside the hook), and
    // `draw_frame_raw` serialises its field accesses on `draw_mutex`.
    unsafe impl Send for SendRenderer {}

    /// The `apply` test double: counts the resyncs and records whether
    /// `draw_mutex` was held at any of them.
    fn sync_probe(
        rp: *mut Renderer,
        held_at_sync: std::sync::Arc<AtomicBool>,
        sync_calls: std::sync::Arc<AtomicUsize>,
    ) -> SyncHook {
        Box::new(move || {
            sync_calls.fetch_add(1, Ordering::SeqCst);
            // SAFETY: a shared borrow of the `Sync` mutex field only.
            let m = unsafe { &(*rp).draw_mutex };
            if m.try_lock() {
                m.unlock();
            } else {
                held_at_sync.store(true, Ordering::SeqCst);
            }
        })
    }

    /// Clears both hooks even when an assertion unwinds.
    struct ClearHooks;
    impl Drop for ClearHooks {
        fn drop(&mut self) {
            ON_SYNC_DISPLAY_LINK.with(|h| *h.borrow_mut() = None);
            BETWEEN_DRAW_UNLOCK_AND_SYNC.with(|h| *h.borrow_mut() = None);
        }
    }

    /// `72cf50855` (#14150). The render thread (this thread) draws on
    /// the no-redraw path, which resyncs the display link; a stand-in
    /// main thread then takes `draw_mutex` through `draw_frame(sync =
    /// true)`, as the CoreAnimation display callback does. The resync
    /// must never run while `draw_mutex` is held, and the main thread's
    /// sync draw must get through before it. The probe is installed on
    /// both threads with one shared counter, so the main thread's sync
    /// draw is shown never to resync.
    ///
    /// NEGATIVE CONTROL: the pre-fix body (`sync_display_link` called
    /// inside the locked region) sets `held_at_sync`, and the main
    /// thread's draw never runs at the gap.
    #[test]
    fn draw_frame_does_not_sync_display_link_under_draw_mutex() {
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // Metal-skip
        };
        // One real frame, so the next unsynced draw has nothing new to
        // render: no size change, the swap chain exists, the rebuilt
        // cells are consumed, and there is no animation.
        r.draw_frame(true).expect("first draw");
        assert!(!r.cells_rebuilt.load(Ordering::Relaxed));
        assert!(r.animation_wake().is_none());

        let held_at_sync = std::sync::Arc::new(AtomicBool::new(false));
        let sync_calls = std::sync::Arc::new(AtomicUsize::new(0));
        // `Some(ok)` once the stand-in main thread's sync draw finished
        // at the gap; `None` if it never ran or timed out.
        let main_draw: std::rc::Rc<std::cell::Cell<Option<bool>>> = Default::default();

        let rp: *mut Renderer = &raw mut r;
        let _clear = ClearHooks;
        let probe = sync_probe(rp, held_at_sync.clone(), sync_calls.clone());
        ON_SYNC_DISPLAY_LINK.with(|h| *h.borrow_mut() = Some(probe));
        let stuck: std::rc::Rc<std::cell::RefCell<Option<std::thread::JoinHandle<()>>>> =
            Default::default();
        {
            let main_draw = main_draw.clone();
            let stuck = stuck.clone();
            let held_at_sync = held_at_sync.clone();
            let sync_calls = sync_calls.clone();
            let hook: DrawHook = Box::new(move |this: *mut Renderer| {
                let p = SendRenderer(this);
                let (held, calls) = (held_at_sync.clone(), sync_calls.clone());
                let (tx, rx) = std::sync::mpsc::channel();
                let main = std::thread::spawn(move || {
                    let p = p;
                    // The same probe on the stand-in main thread.
                    let probe = sync_probe(p.0, held, calls);
                    ON_SYNC_DISPLAY_LINK.with(|h| *h.borrow_mut() = Some(probe));
                    // SAFETY: see `SendRenderer`; this thread parks in
                    // `recv_timeout` below and touches no field meanwhile.
                    let lock = unsafe { (*p.0).draw_mutex.clone() };
                    let ok = unsafe { Renderer::draw_frame_raw(p.0, &lock, true) }.is_ok();
                    let _ = tx.send(ok);
                });
                match rx.recv_timeout(Duration::from_secs(10)) {
                    Ok(ok) => {
                        main.join().expect("main-thread draw panicked");
                        main_draw.set(Some(ok));
                    }
                    // The mutex is still held: join the thread once the
                    // draw below has released it, and fail.
                    Err(_) => {
                        main_draw.set(None);
                        *stuck.borrow_mut() = Some(main);
                    }
                }
            });
            BETWEEN_DRAW_UNLOCK_AND_SYNC.with(|h| *h.borrow_mut() = Some(hook));
        }

        // SAFETY: `rp` is the whole local renderer; no reference is live.
        let lock = unsafe { (*rp).draw_mutex.clone() };
        unsafe { Renderer::draw_frame_raw(rp, &lock, false) }.expect("no-redraw draw");
        drop(_clear);
        if let Some(main) = stuck.borrow_mut().take() {
            main.join().expect("main-thread draw panicked");
        }

        assert_eq!(
            1,
            sync_calls.load(Ordering::SeqCst),
            "the no-redraw path resyncs the display link once, the sync draw never"
        );
        assert!(
            !held_at_sync.load(Ordering::SeqCst),
            "sync_display_link ran while draw_mutex was held"
        );
        assert_eq!(
            Some(true),
            main_draw.get(),
            "a sync draw must take draw_mutex between the unlock and the resync"
        );
        r.thread_exit();
        r.deinit();
    }
}
