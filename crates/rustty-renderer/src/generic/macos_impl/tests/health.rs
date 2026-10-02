//! Tests for `generic::macos_impl` (ghostty src/renderer/generic.zig, the
//! `Renderer(Metal)` arm) — the UD362 Metal health-push deadlock tests
//! (`metal_buffer_completed` and `HealthReport`).

use super::*;

// UD362, Metal variants (P7-W7 T5a). These drive the production
// `frameCompleted` body (`frame_completed_on`, which the completion handler
// `metal_buffer_completed` runs over its context) with no GPU: the Metal
// callback plumbing around it only decides WHICH thread runs it (the Metal
// completion thread for an async frame, the drawing thread for a sync one).
mod metal_health_push_deadlocks {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    use super::*;
    use crate::generic::SurfaceMailboxSink;

    /// An app mailbox that starts FULL. `.instant` is refused while full;
    /// `.forever` waits (up to 3 s, then gives up and records it) for the
    /// main thread to drain it.
    struct FullMailbox {
        full: Mutex<bool>,
        cv: Condvar,
        timed_out: AtomicBool,
        delivered: Mutex<Vec<(SurfaceMessage, MailboxTimeout)>>,
    }

    impl FullMailbox {
        fn new() -> Arc<FullMailbox> {
            Arc::new(FullMailbox {
                full: Mutex::new(true),
                cv: Condvar::new(),
                timed_out: AtomicBool::new(false),
                delivered: Mutex::new(Vec::new()),
            })
        }

        /// The mailbox is full again.
        fn fill(&self) {
            *self.full.lock().unwrap() = true;
        }

        /// The main thread drains the mailbox: it has room again.
        fn drain(&self) {
            *self.full.lock().unwrap() = false;
            self.cv.notify_all();
        }

        fn delivered(&self) -> Vec<(SurfaceMessage, MailboxTimeout)> {
            self.delivered.lock().unwrap().clone()
        }
    }

    impl SurfaceMailboxSink for FullMailbox {
        fn push(&self, msg: SurfaceMessage, timeout: MailboxTimeout) -> usize {
            let full = self.full.lock().unwrap();
            if *full {
                if timeout == MailboxTimeout::Instant {
                    return 0;
                }
                let (full, res) = self
                    .cv
                    .wait_timeout_while(full, Duration::from_secs(3), |f| *f)
                    .unwrap();
                drop(full);
                if res.timed_out() {
                    self.timed_out.store(true, Ordering::SeqCst);
                    return 0;
                }
            } else {
                drop(full);
            }
            self.delivered.lock().unwrap().push((msg, timeout));
            1
        }
    }

    /// The completion context's cells, moved to the completion thread.
    struct CompletionCells {
        health: Arc<AtomicHealth>,
        frame_sema: Arc<Semaphore>,
        surface_mailbox: SurfaceMailbox,
        report: Arc<HealthReport>,
    }
    // SAFETY: the mailbox's sink is `FullMailbox` (Sync); the renderer
    // hands the same clones to the Metal completion thread.
    unsafe impl Send for CompletionCells {}

    /// UD362 (a): an async frame completes on the Metal completion thread
    /// with a health change, while every swap-chain frame is in flight and
    /// the app mailbox is full. The main thread's sync draw holds
    /// `draw_mutex` and waits in `nextFrame` for a frame slot; only once it
    /// has one can it finish the draw and drain the mailbox. With the
    /// reference's blocking `.forever` push (before `releaseFrame`) the two
    /// threads wait on each other. The port's `.instant` push is refused and
    /// the report stays owed; the frame is released.
    #[test]
    fn metal_async_frame_completed_never_blocks_on_a_full_mailbox() {
        let mailbox = FullMailbox::new();
        let health = Arc::new(AtomicHealth::new(crate::Health::Healthy));
        // Every swap-chain frame in flight: no permit left.
        let frame_sema = Arc::new(Semaphore::new());
        let report = Arc::new(HealthReport::new());
        let cells = CompletionCells {
            health: health.clone(),
            frame_sema: frame_sema.clone(),
            surface_mailbox: SurfaceMailbox::new(mailbox.clone()),
            report: report.clone(),
        };

        // The Metal completion thread.
        let completion = std::thread::spawn(move || {
            let c = cells;
            frame_completed_on(
                &c.health,
                &c.frame_sema,
                &c.surface_mailbox,
                &c.report,
                crate::Health::Unhealthy,
            );
            c
        });

        // The main thread's sync draw: `nextFrame` → `frame_sema.wait()`
        // under `draw_mutex`; with a slot it finishes, unlocks and drains.
        frame_sema.wait();
        mailbox.drain();
        let cells = completion.join().expect("completion thread");

        assert!(
            !mailbox.timed_out.load(Ordering::SeqCst),
            "the completion thread blocked in the `.forever` health push before \
             releasing the frame the mailbox's drainer was waiting for (UD362)"
        );
        // Refused while full: the frame's health is recorded, the report
        // owed, the surface still believes the old health.
        assert!(mailbox.delivered().is_empty());
        assert_eq!(health.load(), crate::Health::Unhealthy);
        assert_eq!(report.reported(), crate::Health::Healthy);
        assert!(report.pending());

        // The retried frame (the mailbox drained) reports it.
        frame_completed_on(
            &cells.health,
            &cells.frame_sema,
            &cells.surface_mailbox,
            &cells.report,
            crate::Health::Unhealthy,
        );
        assert_eq!(
            mailbox.delivered(),
            vec![(
                SurfaceMessage::RendererHealth(crate::Health::Unhealthy),
                MailboxTimeout::Instant
            )]
        );
        assert_eq!(report.reported(), crate::Health::Unhealthy);
        assert!(!report.pending());
    }

    /// UD362 (b): a sync draw completes its frame on the main thread
    /// itself, the app mailbox's only drainer. A health change must not
    /// block it on its own full mailbox; the refused report stays owed and
    /// a later frame delivers it, once.
    #[test]
    fn metal_sync_frame_completed_never_blocks_the_mailbox_drainer() {
        let mailbox = FullMailbox::new();
        let surface_mailbox = SurfaceMailbox::new(mailbox.clone());
        let health = AtomicHealth::new(crate::Health::Healthy);
        let frame_sema = Semaphore::new();
        let report = HealthReport::new();

        frame_completed_on(
            &health,
            &frame_sema,
            &surface_mailbox,
            &report,
            crate::Health::Unhealthy,
        );
        assert!(
            !mailbox.timed_out.load(Ordering::SeqCst),
            "the main thread blocked in a `.forever` push into the mailbox \
             only it drains (UD362)"
        );
        // The frame was released all the same.
        assert!(frame_sema.timed_wait(0).is_ok());
        assert!(mailbox.delivered().is_empty());
        assert!(report.pending());

        // The main thread drains; the next frame reports the transition.
        mailbox.drain();
        frame_completed_on(
            &health,
            &frame_sema,
            &surface_mailbox,
            &report,
            crate::Health::Unhealthy,
        );
        // Identical health: no second report.
        frame_completed_on(
            &health,
            &frame_sema,
            &surface_mailbox,
            &report,
            crate::Health::Unhealthy,
        );
        assert_eq!(
            mailbox.delivered(),
            vec![(
                SurfaceMessage::RendererHealth(crate::Health::Unhealthy),
                MailboxTimeout::Instant
            )]
        );
        assert!(!report.pending());
        assert!(frame_sema.timed_wait(0).is_ok());
        assert!(frame_sema.timed_wait(0).is_ok());
        assert!(frame_sema.timed_wait(0).is_err());
    }

    /// Round 2 (Minor 1): the fix-round-1 withdrawal race. A completes
    /// Unhealthy and is refused; B completes Healthy before A's report
    /// lands; then the mailbox takes reports again. The surface must end
    /// up believing the last frame's health, and never be told a health
    /// that was not the one it should switch to. With the round-1
    /// withdrawal (compare-exchange back to the previous health) the cell
    /// could end at a value the surface never heard; with `reported` the
    /// surface's view is tracked on its own.
    #[test]
    fn metal_health_report_never_restores_an_undelivered_health() {
        let mailbox = FullMailbox::new();
        let surface_mailbox = SurfaceMailbox::new(mailbox.clone());
        let health = AtomicHealth::new(crate::Health::Healthy);
        let frame_sema = Semaphore::new();
        let report = HealthReport::new();
        let complete = |h| frame_completed_on(&health, &frame_sema, &surface_mailbox, &report, h);

        // A: Unhealthy, refused (the mailbox is full).
        complete(crate::Health::Unhealthy);
        assert!(report.pending());
        // B: Healthy, before A's report landed: the surface already
        // believes Healthy, so nothing is owed any more.
        complete(crate::Health::Healthy);
        assert!(!report.pending());
        assert_eq!(report.reported(), crate::Health::Healthy);
        assert_eq!(health.load(), crate::Health::Healthy);
        // The mailbox drains; another Healthy frame reports nothing.
        mailbox.drain();
        complete(crate::Health::Healthy);
        assert!(mailbox.delivered().is_empty());

        // The same race the other way round: Unhealthy delivered, then a
        // refused Healthy, then Unhealthy again: nothing owed, one report.
        complete(crate::Health::Unhealthy);
        mailbox.fill();
        complete(crate::Health::Healthy);
        assert!(report.pending());
        complete(crate::Health::Unhealthy);
        assert!(!report.pending());
        assert_eq!(report.reported(), crate::Health::Unhealthy);
        assert_eq!(
            mailbox.delivered(),
            vec![(
                SurfaceMessage::RendererHealth(crate::Health::Unhealthy),
                MailboxTimeout::Instant
            )]
        );
        for _ in 0..6 {
            assert!(frame_sema.timed_wait(0).is_ok());
        }
    }

    /// Round 2 (Minor 1), concurrent: two completion threads (the Metal
    /// completion thread and a main-thread sync draw) report alternating
    /// healths into a mailbox that refuses every other push. Whatever the
    /// interleaving, the last health the surface heard is `reported`, and
    /// once a final frame completes with room, it is that frame's health.
    #[test]
    fn metal_health_report_concurrent_completions_stay_consistent() {
        struct FlakyMailbox {
            n: std::sync::atomic::AtomicUsize,
            delivered: Mutex<Vec<crate::Health>>,
        }
        impl SurfaceMailboxSink for FlakyMailbox {
            fn push(&self, msg: SurfaceMessage, _t: MailboxTimeout) -> usize {
                if self.n.fetch_add(1, Ordering::SeqCst).is_multiple_of(2) {
                    return 0;
                }
                let SurfaceMessage::RendererHealth(h) = msg else {
                    panic!("unexpected message");
                };
                self.delivered.lock().unwrap().push(h);
                1
            }
        }
        let sink = Arc::new(FlakyMailbox {
            n: std::sync::atomic::AtomicUsize::new(0),
            delivered: Mutex::new(Vec::new()),
        });
        let health = Arc::new(AtomicHealth::new(crate::Health::Healthy));
        let frame_sema = Arc::new(Semaphore::new());
        let report = Arc::new(HealthReport::new());
        let threads: Vec<_> = (0..2)
            .map(|t| {
                let cells = CompletionCells {
                    health: health.clone(),
                    frame_sema: frame_sema.clone(),
                    surface_mailbox: SurfaceMailbox::new(sink.clone()),
                    report: report.clone(),
                };
                std::thread::spawn(move || {
                    let c = cells;
                    for i in 0..200 {
                        let h = if (i + t) % 2 == 0 {
                            crate::Health::Healthy
                        } else {
                            crate::Health::Unhealthy
                        };
                        frame_completed_on(
                            &c.health,
                            &c.frame_sema,
                            &c.surface_mailbox,
                            &c.report,
                            h,
                        );
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let heard = |sink: &FlakyMailbox| {
            *sink
                .delivered
                .lock()
                .unwrap()
                .last()
                .unwrap_or(&crate::Health::Healthy)
        };
        assert_eq!(heard(&sink), report.reported());

        // A final frame, retried until the flaky mailbox takes it.
        let mailbox = SurfaceMailbox::new(sink.clone());
        for _ in 0..2 {
            frame_completed_on(
                &health,
                &frame_sema,
                &mailbox,
                &report,
                crate::Health::Unhealthy,
            );
        }
        assert!(!report.pending());
        assert_eq!(report.reported(), crate::Health::Unhealthy);
        assert_eq!(heard(&sink), crate::Health::Unhealthy);
        // No report ever repeats the health the surface already had.
        let d = sink.delivered.lock().unwrap().clone();
        let mut prev = crate::Health::Healthy;
        for h in d {
            assert_ne!(h, prev, "a report repeated the surface's health");
            prev = h;
        }
    }

    /// A frame on an offscreen renderer (a DETACHED mailbox: nothing to
    /// deliver to) owes no report.
    #[test]
    fn metal_sync_frame_completed_detached_mailbox_keeps_health() {
        let health = AtomicHealth::new(crate::Health::Healthy);
        let frame_sema = Semaphore::new();
        let report = HealthReport::new();
        frame_completed_on(
            &health,
            &frame_sema,
            &SurfaceMailbox::DETACHED,
            &report,
            crate::Health::Unhealthy,
        );
        assert_eq!(health.load(), crate::Health::Unhealthy);
        assert!(!report.pending());
        assert!(frame_sema.timed_wait(0).is_ok());
    }

    /// The retry wakes the render thread when a report BECOMES owed, and
    /// only then: a further refusal while it is owed does not notify (the
    /// animation timer paces those), and after the report settles the next
    /// refusal notifies again at once, however soon (fix round 3: a timed
    /// pacing could skip that wake and strand the report). Observed by a
    /// kqueue loop waiting on `draw_now`, as the render thread's
    /// `drawNowCallback` does.
    #[test]
    fn metal_health_report_wakes_draw_now_when_a_report_becomes_owed() {
        let mut draw_now = Async::init().expect("Async::init");
        let report = HealthReport::new();
        let mailbox = FullMailbox::new();
        let surface_mailbox = SurfaceMailbox::new(mailbox.clone());
        let health = AtomicHealth::new(crate::Health::Healthy);
        let frame_sema = Semaphore::new();
        let complete = |h| frame_completed_on(&health, &frame_sema, &surface_mailbox, &report, h);
        // Disarmed: nothing to notify.
        assert!(!report.wake());
        report.arm(Some(draw_now));

        let mut lp = rustty_xev::Loop::init(rustty_xev::Options::default()).expect("loop");
        let mut woke = false;
        let mut c_wait = rustty_xev::Completion::default();
        let cb = |ud: *mut bool,
                  _l: *mut rustty_xev::Loop,
                  _c: *mut rustty_xev::Completion,
                  r: Result<(), rustty_xev::WaitError>| {
            r.unwrap();
            unsafe { *ud = true };
            rustty_xev::CallbackAction::Rearm
        };
        unsafe {
            draw_now.wait(
                &mut lp as *mut rustty_xev::Loop,
                core::ptr::addr_of_mut!(c_wait),
                core::ptr::addr_of_mut!(woke),
                cb,
            );
        }
        let woken = |lp: &mut rustty_xev::Loop, woke: *mut bool| {
            lp.run(rustty_xev::RunMode::NoWait).expect("run");
            unsafe { core::mem::replace(&mut *woke, false) }
        };

        // Becomes owed: notified.
        complete(crate::Health::Unhealthy);
        assert!(report.pending());
        assert!(
            woken(&mut lp, &raw mut woke),
            "the owed report did not wake"
        );
        // Still owed: not again.
        complete(crate::Health::Unhealthy);
        assert!(
            !woken(&mut lp, &raw mut woke),
            "a still-owed report woke again"
        );
        // Settles (the mailbox drains), then a new refusal right away:
        // notified again, with no interval in between.
        mailbox.drain();
        complete(crate::Health::Unhealthy);
        assert!(!report.pending());
        mailbox.fill();
        complete(crate::Health::Healthy);
        assert!(report.pending());
        assert!(
            woken(&mut lp, &raw mut woke),
            "a newly owed report did not wake"
        );

        report.arm(None);
        assert!(!report.wake());
        for _ in 0..4 {
            assert!(frame_sema.timed_wait(0).is_ok());
        }
        lp.deinit();
        draw_now.deinit();
    }

    /// An owed report is an animation wake: every draw renders a frame
    /// (whose completion retries it) even when nothing else changed, and a
    /// render-thread draw asks for the display-link resync; without one,
    /// that draw takes the no-redraw path.
    #[test]
    fn metal_health_report_owed_forces_frames() {
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // Metal-skip
        };
        // A sync draw builds the swap chain and clears `cells_rebuilt`.
        r.draw_frame(true).expect("first draw");
        let index = |r: &Renderer| r.swap_chain.as_ref().unwrap().frame_index;
        let before = index(&r);
        // Nothing changed: the async draw does not render.
        r.draw_frame(false).expect("no-redraw draw");
        assert_eq!(index(&r), before, "an idle draw rendered a frame");
        assert_eq!(r.animation_wake(), None);
        // A report is owed: an animation wake, and draws render.
        r.health_report.set_pending_for_test();
        assert_eq!(
            r.animation_wake(),
            Some(AnimationWake {
                delay_ms: DRAW_INTERVAL_MS,
                kind: AnimationWakeKind::Draw
            })
        );
        r.draw_frame(false).expect("retry draw");
        let after = index(&r);
        assert_ne!(after, before, "an owed report did not force a frame");
        // The detached mailbox settles it on that frame's completion, so
        // the next draw is idle again.
        let self_ptr: *mut Renderer = &mut r;
        let lock = r.draw_mutex.clone();
        // Wait for the async completion (all frames released).
        unsafe {
            Renderer::with_locked(self_ptr, &lock, |r| {
                let chain = r.swap_chain.as_ref().unwrap();
                for _ in 0..SWAP_CHAIN_COUNT {
                    chain.frame_sema.wait();
                }
                for _ in 0..SWAP_CHAIN_COUNT {
                    chain.frame_sema.post();
                }
            })
        };
        assert!(!r.health_report.pending());
        r.draw_frame(false).expect("idle draw");
        assert_eq!(index(&r), after);

        deinit_test_renderer(&mut r);
    }
}
