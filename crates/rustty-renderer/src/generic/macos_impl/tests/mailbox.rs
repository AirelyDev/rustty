//! Tests for `generic::macos_impl` (ghostty src/renderer/generic.zig, the
//! `Renderer(Metal)` arm) — the F-014 surface-mailbox routing proofs.

use super::*;

// F-014 PROVING TESTS: the surface-mailbox seam (GPU_RD_CHUNK3A_DESIGN §B.2)
// is RESOLVED — the two `generic.zig` pushes actually reach the mailbox.
// Before the fix `SurfaceMailbox::push` was a hard-coded `0`, so BOTH of
// these tests observed an EMPTY sink and the scrollbar `defer` could never
// clear `scrollbar_dirty`.
mod surface_mailbox_routing {
    use std::sync::Arc;

    use super::*;
    use crate::generic::surface_mailbox_tests::RecordingSink;

    /// Drive one real frame: `update_frame` (which refreshes the scrollbar
    /// cache from the terminal and raises `scrollbar_dirty`) then
    /// `draw_frame(true)` (whose `defer` runs the scrollbar push). Returns
    /// the scrollbar value the push must carry.
    fn update_and_draw(r: &mut Renderer) -> Scrollbar {
        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"Ab");
            s.deinit();
        }
        let st = state::State {
            mutex: &mut mutex,
            terminal: &mut t,
            inspector: None,
            inspector_ptr: None,
            preedit: None,
            mouse: state::Mouse::default(),
            demand: Default::default(),
            handoff_gen: Default::default(),
            handoff_park: Default::default(),
        };
        r.update_frame(&st, true).unwrap();
        // Guard against a vacuous test: the fresh terminal's scrollbar must
        // differ from the `Scrollbar::ZERO` the renderer was built with, so
        // `update_frame` raised the dirty flag (generic.zig:1247-1250).
        assert!(
            r.scrollbar_dirty,
            "update_frame must have cached a fresh scrollbar (dirty)"
        );
        let scrollbar = r.scrollbar;
        r.draw_frame(true).unwrap();
        scrollbar
    }

    /// generic.zig:1725-1737 — `frameCompleted` pushes `.renderer_health`
    /// ONLY on a health TRANSITION (with `.instant` where the reference uses
    /// `.forever`: UD362, see `frame_completed_on`).
    #[test]
    fn frame_completed_pushes_renderer_health_on_transition_only() {
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // Metal-skip
        };
        let sink = Arc::new(RecordingSink::accepting());
        r.surface_mailbox = SurfaceMailbox::new(sink.clone());
        // A frame only completes once the swap chain exists; the renderer
        // builds it lazily on the first draw (`2f0b65346`), so build it here.
        r.swap_chain = Some(SwapChain::init(&r.api, false).expect("swap chain"));

        // `health` starts `.healthy`, so this IS a transition → one push.
        r.frame_completed(crate::Health::Unhealthy);
        // Identical health → `if (self.health.load(...) != health)` is false
        // → NO second push.
        r.frame_completed(crate::Health::Unhealthy);

        assert_eq!(
            sink.pushed(),
            vec![(
                SurfaceMessage::RendererHealth(crate::Health::Unhealthy),
                MailboxTimeout::Instant
            )]
        );
    }

    /// generic.zig:1449-1456 — the `drawFrame` `defer` pushes `.scrollbar`
    /// with `.instant` and clears `scrollbar_dirty` when the push lands.
    #[test]
    fn draw_frame_pushes_scrollbar_and_clears_dirty() {
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // Metal-skip
        };
        let sink = Arc::new(RecordingSink::accepting());
        r.surface_mailbox = SurfaceMailbox::new(sink.clone());

        let scrollbar = update_and_draw(&mut r);

        assert_eq!(
            sink.pushed(),
            vec![(
                SurfaceMessage::Scrollbar(scrollbar),
                MailboxTimeout::Instant
            )]
        );
        assert!(
            !r.scrollbar_dirty,
            "a landed scrollbar push must clear scrollbar_dirty"
        );
    }

    /// The other arm of the same `if (... > 0)`: "Fail instantly if the
    /// surface mailbox if full, we'll just get it on the next frame" —
    /// a refused push must KEEP `scrollbar_dirty` set.
    #[test]
    fn draw_frame_keeps_scrollbar_dirty_when_the_mailbox_is_full() {
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // Metal-skip
        };
        let sink = Arc::new(RecordingSink::refusing());
        r.surface_mailbox = SurfaceMailbox::new(sink.clone());

        let scrollbar = update_and_draw(&mut r);

        assert_eq!(
            sink.pushed(),
            vec![(
                SurfaceMessage::Scrollbar(scrollbar),
                MailboxTimeout::Instant
            )]
        );
        assert!(
            r.scrollbar_dirty,
            "a refused scrollbar push must keep scrollbar_dirty set"
        );
    }
}
