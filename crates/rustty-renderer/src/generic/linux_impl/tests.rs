//! Unit tests for `generic::linux_impl`, the `Renderer(OpenGL)` arm of ghostty
//! src/renderer/generic.zig: the shared fixtures, the lifecycle, cell-fill, uniforms,
//! mailbox and restored-log tests. The golden and config/overlay tests live in
//! `tests/`.

use allocator_api2::alloc::Global;

use super::{Error, Renderer};
use crate::generic::{
    Color, DerivedConfig, Options, RepeatablePath, SurfaceMailbox, TerminalColor,
};
use crate::link;
// The bg-image buffer mapping asserted by `update_bg_image_buffer_maps_config`.
use crate::opengl::gl::test_support;
use crate::opengl::OpenGL;
use crate::shaders::{BgImageInfoFit, BgImageInfoPosition};
use crate::size::{
    AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, CellSize,
    FontShapingBreak, Padding, ScreenSize, Size, WindowColorspace,
};
use crate::state;
use rustty_font_shaper::codepoint_resolver::{CodepointResolver, DescriptorCache, StyleStatus};
use rustty_font_shaper::collection::{AddOptions, Collection, LoadOptions, SizeAdjustment};
use rustty_font_shaper::face::{
    DesiredSize, Face, Options as FaceOptions, FREETYPE_LOAD_FLAGS_DEFAULT,
};
use rustty_font_shaper::library::Library;
use rustty_font_shaper::SharedGrid;
use rustty_font_shaper::Style as FontStyle;
use rustty_render_types::WindowPaddingColor;
use rustty_vt::color;
use rustty_vt::kitty;
use rustty_vt::modes::ModePacked;
use rustty_vt::render::CellSet;
use rustty_vt::size::CellCountInt;
use rustty_vt::terminal::{Colors, Options as TermOptions, Terminal};
use rustty_zigstd::arena_allocator::ArenaAllocator;
use rustty_zigstd::thread::Mutex;

use crate::cursor;
use crate::shaders::Atlas;

// PORT NOTE: `derived_config_for_test`/`test_font_grid_and_size` are duplicated
// from `macos_impl::tests` (the established test-helper-duplication precedent —
// `opts(cols, rows)` is already duplicated cell.rs/cursor.rs/generic.rs). We
// duplicate rather than share to avoid coupling `linux_impl`'s tests to
// `macos_impl`'s internals (and `macos_impl` is not even compiled here).
fn derived_config_for_test() -> DerivedConfig {
    DerivedConfig {
        arena: ArenaAllocator::init(),
        font_thicken: false,
        font_thicken_strength: 0,
        font_features: Vec::new(),
        font_styles: StyleStatus::init_fill(true),
        font_shaping_break: FontShapingBreak::default(),
        cursor_color: Some(TerminalColor::Color(Color {
            r: 0xFF,
            g: 0,
            b: 0,
        })),
        cursor_opacity: 1.0,
        cursor_text: None,
        background: color::RGB { r: 0, g: 0, b: 0 },
        background_opacity: 1.0,
        background_opacity_cells: false,
        foreground: color::RGB {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        },
        selection_background: None,
        selection_foreground: None,
        search_background: TerminalColor::Color(Color {
            r: 0xFF,
            g: 0xE0,
            b: 0x82,
        }),
        search_foreground: TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
        search_selected_background: TerminalColor::Color(Color {
            r: 0xF2,
            g: 0xA5,
            b: 0x7E,
        }),
        search_selected_foreground: TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
        bold_color: None,
        faint_opacity: 255,
        min_contrast: 1.0,
        padding_color: WindowPaddingColor::Background,
        custom_shaders: RepeatablePath::default(),
        bg_image: None,
        bg_image_opacity: 1.0,
        bg_image_position: BackgroundImagePosition::TopLeft,
        bg_image_fit: BackgroundImageFit::Contain,
        bg_image_repeat: false,
        links: link::Set { links: Vec::new() },
        vsync: false,
        colorspace: WindowColorspace::Srgb,
        blending: AlphaBlending::Native,
        background_blur: BackgroundBlur::False,
        scroll_to_bottom_on_output: false,
        custom_shader_animation: crate::thread::CustomShaderAnimation::True,
    }
}

// Build a heap embedded-font `SharedGrid` + the pixel `Size` for a cols×rows
// grid (the `macos_impl::tests::test_font_grid_and_size` recipe, generic.rs:6283).
// The grid is boxed so the returned `*mut SharedGrid` stays valid for the
// renderer that borrows it (the renderer never owns its grid; in the app the
// `SharedGridSet` does). The test frees it after the renderer's `deinit`,
// with [`free_test_font_grid`] or [`deinit_test_renderer`]: it used to be
// leaked, which G-VALGRIND reported as 38 definite leaks (NEW-FINDINGS N411).
// `size.screen` is PINNED to the grid's pixel size.
fn test_font_grid_and_size(cols: CellCountInt, rows: CellCountInt) -> (*mut SharedGrid, Size) {
    let font_grid: *mut SharedGrid = {
        let lib = Library::init().unwrap();
        let mut c = Collection::init();
        c.load_options = Some(LoadOptions::new(lib));
        c.add(
            Face::init(
                lib,
                rustty_font::embedded::REGULAR,
                FaceOptions {
                    size: DesiredSize {
                        points: 12.0,
                        xdpi: 96,
                        ydpi: 96,
                    },
                    freetype_load_flags: FREETYPE_LOAD_FLAGS_DEFAULT,
                },
            )
            .unwrap(),
            AddOptions {
                style: FontStyle::Regular,
                fallback: false,
                size_adjustment: SizeAdjustment::None,
            },
        )
        .unwrap();
        let resolver = CodepointResolver {
            collection: c,
            styles: StyleStatus::init_fill(true),
            discover: None,
            codepoint_map: None,
            descriptor_cache: DescriptorCache::default(),
            sprite: None,
        };
        let grid = SharedGrid::init(Global, resolver).unwrap();
        Box::into_raw(Box::new(grid))
    };

    let grid_metrics = unsafe { (*font_grid).metrics };
    let cell = CellSize {
        width: grid_metrics.cell_width,
        height: grid_metrics.cell_height,
    };
    let size = Size {
        screen: ScreenSize {
            width: cell.width * cols as u32,
            height: cell.height * rows as u32,
        },
        cell,
        padding: Padding {
            top: 0,
            bottom: 0,
            right: 0,
            left: 0,
        },
    };
    (font_grid, size)
}

// Free a grid built by `test_font_grid_and_size`: the grid's faces
// (`SharedGrid::deinit`), the box, then the FreeType library the collection
// was loaded with, in the order `shared_grid`'s own tests use. Call it only
// after every renderer that borrowed the grid has been deinited.
fn free_test_font_grid(font_grid: *mut SharedGrid) {
    // SAFETY: `font_grid` came from `Box::into_raw` in
    // `test_font_grid_and_size`, and its only borrower is gone.
    let mut grid = unsafe { Box::from_raw(font_grid) };
    let mut lib = grid
        .resolver
        .collection
        .load_options
        .as_ref()
        .expect("test_font_grid_and_size sets the load options")
        .library;
    grid.deinit();
    drop(grid);
    lib.deinit();
}

// `r.deinit()` for a renderer built on `test_font_grid_and_size`'s grid
// (every `golden_renderer`), then the grid itself.
fn deinit_test_renderer(r: &mut Renderer) {
    let font_grid = r.font_grid;
    // The render thread's `threadExit` frees the GPU objects while its context
    // is current (`2f0b65346`); the tests are that thread.
    r.thread_exit();
    r.deinit();
    free_test_font_grid(font_grid);
}

/// What the render thread does before its first draw (`threadEnter`, then
/// the first `.resize` message, whose `setScreenSize` sets the GL viewport).
fn enter_test_renderer(r: &mut Renderer) {
    r.thread_enter(core::ptr::null_mut::<()>())
        .expect("thread_enter");
    let size = r.size;
    r.set_screen_size(size);
}

/// L4 T9 KAT: the OpenGL `Renderer` init/deinit round-trip — one layer up from
/// Task 8's `opengl_init_offscreen_then_deinit_round_trips`. Stands up a real
/// embedded-font grid, builds the monomorphic `Renderer(OpenGL)` via
/// `Renderer::init` (which internally stands up the offscreen EGL/GL context),
/// then tears it down via `deinit` — asserting no panic/leak across the whole
/// FFI teardown (the swap-chain frames' GL objects + the shader pipelines are
/// freed while the context is current, then the context terminates).
///
/// Skips cleanly (never panics, never fabricates) when the host cannot provide
/// an OpenGL 4.3-core context — the Task-1 OPEN RISK path (the default virgl
/// driver here; runs for real under `LIBGL_ALWAYS_SOFTWARE=1` → llvmpipe 4.5).
///
/// Holds the process-wide [`test_support::gl_lock`] for the whole context
/// lifetime (see its docs), `_guard` declared FIRST so it drops LAST (after
/// `deinit` destroys the context), matching Task 8's field-order contract.
#[test]
fn linux_opengl_renderer_init_then_deinit_round_trips() {
    let _guard = test_support::gl_lock();
    let Some(device) =
        test_support::device_or_skip("linux_opengl_renderer_init_then_deinit_round_trips")
    else {
        return;
    };

    let (font_grid, size) = test_font_grid_and_size(10, 3);
    let opts = Options {
        device,
        config: derived_config_for_test(),
        font_grid,
        size,
        // Offscreen: no surface, hence no mailbox (push returns 0).
        surface_mailbox: SurfaceMailbox::DETACHED,
    };

    let mut r = match Renderer::init(Global, opts) {
        Ok(r) => r,
        // A GL/context failure here is the host being unable to stand up a
        // 4.3-core context (absent EGL / no surfaceless platform / the virgl
        // driver's `EGL_BAD_MATCH` / a version under the floor). All are a
        // CLEAN SKIP, mirroring Task 8's `init_offscreen` skip contract — never
        // a panic, never a fabricated renderer.
        Err(e) => {
            // W8-03 strict mode: under RUSTTY_REQUIRE_GL=1 the clean skip
            // becomes a hard failure (see test_support docs).
            test_support::require_gl_or_skip(
                "linux_opengl_renderer_init_then_deinit_round_trips",
                &e,
            );
            eprintln!(
                "no headless OpenGL 4.3 core context ({e}); skipping \
                 linux_opengl_renderer_init_then_deinit_round_trips"
            );
            return;
        }
    };

    // `init` touches no GL (`2f0b65346`): the swap chain and the shaders
    // are built lazily by the first draw on the render thread.
    assert!(r.swap_chain.is_none());
    assert!(r.shaders.defunct);
    assert!(r.reinitialize_shaders);
    enter_test_renderer(&mut r);

    // Fields the ctor + the `update_*` tail set (non-vacuous: proves
    // `init` ran the whole path, not just the struct literal).
    assert!(!r.has_custom_shaders);
    assert_eq!(
        r.uniforms.cell_size,
        [
            r.grid_metrics.cell_width as f32,
            r.grid_metrics.cell_height as f32
        ]
    );

    // The teardown must not panic/leak: leaf GL objects freed first (context
    // still current), then the context (via `api`'s drop after `deinit`).
    deinit_test_renderer(&mut r);
}

/// The unrealize/realize cycle after `2f0b65346`: `displayUnrealized` and
/// `displayRealized` only flip flags (on the main thread, under the draw
/// mutex); the render thread's `renderCallback` releases the swap chain and
/// the shaders while unrealized (`Thread.zig:556-565`), no draw builds them
/// while unrealized (`drawFrameLocked`'s `if (!self.display_realized) return
/// false`), and the first draw after `displayRealized` rebuilds both
/// (`reinitialize_shaders = true`, `target_config_modified = 1`).
///
/// The test thread plays the render thread (`threadEnter`), calling the
/// `renderer_seam` entries with a separate `draw_mutex` handle as the render
/// and main threads do (N437).
#[test]
fn linux_opengl_display_unrealize_then_realize_releases_and_rebuilds_on_the_render_thread() {
    const TEST: &str =
        "linux_opengl_display_unrealize_then_realize_releases_and_rebuilds_on_the_render_thread";
    let _guard = test_support::gl_lock();

    let Some(mut r) = golden_renderer(10, 3) else {
        let _ = TEST;
        return;
    };
    let lock = r.draw_mutex.clone();
    let rp: *mut Renderer = &mut r;

    // The first draw builds the GPU resources.
    r.draw_frame(false).expect("first draw");
    assert!(r.swap_chain.is_some());
    assert!(!r.shaders.defunct);
    assert!(!r.reinitialize_shaders);

    // Unrealize (main thread): only the flag changes.
    unsafe { Renderer::display_unrealized_raw(rp, &lock) };
    assert!(!r.display_realized);
    assert!(
        r.swap_chain.is_some(),
        "unrealize leaves the release to the render thread"
    );

    // renderCallback (render thread): releases, and disarms.
    assert!(unsafe { Renderer::release_if_unrealized_raw(rp, &lock) });
    assert!(r.swap_chain.is_none());
    assert!(r.shaders.defunct, "unrealized: the shaders go too");

    // No draw while unrealized.
    r.draw_frame(true).expect("unrealized draw");
    assert!(r.swap_chain.is_none());

    // Realize (main thread): only flags.
    unsafe { Renderer::display_realized_raw(rp, &lock) }.expect("display_realized");
    assert!(r.display_realized);
    assert!(r.reinitialize_shaders);
    assert_eq!(r.target_config_modified, 1);
    assert!(r.swap_chain.is_none());
    assert!(!unsafe { Renderer::release_if_unrealized_raw(rp, &lock) });

    // The next draw rebuilds both.
    r.draw_frame(false).expect("draw after realize");
    assert!(r.swap_chain.is_some());
    assert!(!r.shaders.defunct);
    assert!(!r.reinitialize_shaders);

    // A second cycle proves the rebuilt resources are themselves re-tearable.
    unsafe { Renderer::display_unrealized_raw(rp, &lock) };
    assert!(unsafe { Renderer::release_if_unrealized_raw(rp, &lock) });
    assert!(r.swap_chain.is_none());

    deinit_test_renderer(&mut r);
}

/// `140feb86a`: `threadExit` marks the display unrealized before
/// `releaseGpuResources`, so the shaders are freed even for an apprt that never
/// unrealizes (macOS), and no later draw rebuilds resources after the render
/// thread is gone. Plays the render thread with its own lock handle (N437).
#[test]
fn linux_opengl_thread_exit_unrealizes_the_display() {
    let _guard = test_support::gl_lock();
    let Some(mut r) = golden_renderer(10, 3) else {
        return;
    };
    r.draw_frame(false).expect("first draw");
    assert!(r.display_realized);
    assert!(r.swap_chain.is_some());
    assert!(!r.shaders.defunct);

    // The `draw_mutex` block of `threadExit` (the API teardown is left to
    // `deinit_test_renderer`'s own `thread_exit`).
    let lock = r.draw_mutex.clone();
    lock.lock();
    r.thread_exit_locked();
    lock.unlock();
    assert!(!r.display_realized, "threadExit unrealizes the display");
    assert!(r.swap_chain.is_none());
    assert!(r.shaders.defunct, "the shaders are released on thread exit");

    // A later draw returns early and rebuilds nothing.
    r.draw_frame(true).expect("draw after thread exit");
    assert!(r.swap_chain.is_none());
    assert!(r.shaders.defunct);

    deinit_test_renderer(&mut r);
}

/// `setVisible(false)` releases the swap chain on the render thread
/// (`2f0b65346`: the pin's GTK arm skipped the release, because the GLArea
/// owned the context), but not the shaders while the display is realized
/// (`releaseGpuResources`).
#[test]
fn linux_opengl_set_visible_false_releases_the_swap_chain() {
    let _guard = test_support::gl_lock();
    let Some(mut r) = golden_renderer(10, 3) else {
        return;
    };
    let lock = r.draw_mutex.clone();
    let rp: *mut Renderer = &mut r;
    r.draw_frame(false).expect("first draw");
    assert!(r.swap_chain.is_some());

    unsafe { Renderer::set_visible_raw(rp, &lock, false) };
    assert!(!r.visible);
    assert!(r.swap_chain.is_none());
    assert!(!r.shaders.defunct, "realized: the shaders stay");

    unsafe { Renderer::set_visible_raw(rp, &lock, true) };
    r.draw_frame(false).expect("draw after visible");
    assert!(r.swap_chain.is_some());

    deinit_test_renderer(&mut r);
}

/// A completed healthy frame is exported, pushed into `latest_frame` and
/// announced with `.redraw` (`opengl/Frame.zig:62-89`); the apprt takes it.
/// With the presentation unhealthy the frame is CPU memory.
#[test]
fn linux_opengl_frame_completion_pushes_the_frame_and_redraws() {
    let _guard = test_support::gl_lock();
    let Some(mut r) = golden_renderer(10, 3) else {
        return;
    };
    use crate::generic::surface_mailbox_tests::RecordingSink;
    use crate::generic::{MailboxTimeout, SurfaceMessage};
    let sink = std::sync::Arc::new(RecordingSink::accepting());
    r.surface_mailbox = SurfaceMailbox::new(sink.clone());
    let slot = r.latest_frame_handle();

    r.set_presentation_health(crate::Health::Unhealthy);
    r.draw_frame(true).expect("draw");
    assert!(
        sink.pushed()
            .contains(&(SurfaceMessage::Redraw, MailboxTimeout::Forever)),
        "pushed: {:?}",
        sink.pushed()
    );
    match slot.take().expect("a frame was pushed") {
        crate::opengl::opengl_impl::ExportedFrame::Memory(m) => {
            assert_eq!(
                (m.width, m.height),
                (r.size.screen.width, r.size.screen.height)
            );
            assert_eq!(m.pixels.len(), (m.width * m.height * 4) as usize);
        }
        #[cfg(unix)]
        crate::opengl::opengl_impl::ExportedFrame::Dmabuf(_) => {
            panic!("an unhealthy presentation exported a DMA-BUF")
        }
    }
    assert!(r.take_frame().is_none(), "take empties the slot");

    deinit_test_renderer(&mut r);
}

/// UD362: a full app mailbox must not deadlock the render thread against the
/// apprt main thread. The render thread completes a frame and pushes
/// `.redraw` with `.forever` into a full mailbox; the main thread (the
/// mailbox's only consumer) first calls `displayUnrealized`, which takes the
/// draw mutex, and only then drains. With the reference's order (the push
/// inside the locked frame completion) the two wait on each other; the test
/// mailbox gives up after 3 s and reports it. With the push after the
/// unlock, the main thread gets the mutex, drains, and the push lands.
#[test]
fn linux_opengl_full_mailbox_does_not_deadlock_display_unrealize() {
    use crate::generic::{MailboxTimeout, SurfaceMailboxSink, SurfaceMessage};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    let _guard = test_support::gl_lock();
    let Some(mut r) = golden_renderer(10, 3) else {
        return;
    };

    struct FullMailbox {
        full: Mutex<bool>,
        cv: Condvar,
        blocked: AtomicBool,
        timed_out: AtomicBool,
        delivered: Mutex<Vec<SurfaceMessage>>,
    }
    impl SurfaceMailboxSink for FullMailbox {
        fn push(&self, msg: SurfaceMessage, timeout: MailboxTimeout) -> usize {
            let full = self.full.lock().unwrap();
            if *full {
                if timeout == MailboxTimeout::Instant {
                    return 0;
                }
                // `.forever` on a full queue: wait for the consumer.
                self.blocked.store(true, Ordering::SeqCst);
                self.cv.notify_all();
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
            self.delivered.lock().unwrap().push(msg);
            1
        }
    }
    let mailbox = Arc::new(FullMailbox {
        full: Mutex::new(true),
        cv: Condvar::new(),
        blocked: AtomicBool::new(false),
        timed_out: AtomicBool::new(false),
        delivered: Mutex::new(Vec::new()),
    });
    r.surface_mailbox = SurfaceMailbox::new(mailbox.clone());
    r.set_presentation_health(crate::Health::Unhealthy);

    struct SendPtr(*mut Renderer);
    // SAFETY: the pointer is only used through the lock-first raw entries.
    unsafe impl Send for SendPtr {}
    let lock = r.draw_mutex.clone();
    let r_ptr: *mut Renderer = &mut r;

    // The apprt main thread.
    let main = std::thread::spawn({
        let mailbox = mailbox.clone();
        let lock = lock.clone();
        let p = SendPtr(r_ptr);
        move || {
            let p = p;
            let mut full = mailbox.full.lock().unwrap();
            while !mailbox.blocked.load(Ordering::SeqCst) {
                let (f, res) = mailbox
                    .cv
                    .wait_timeout(full, Duration::from_secs(10))
                    .unwrap();
                full = f;
                assert!(!res.timed_out(), "the render thread never pushed `.redraw`");
            }
            drop(full);
            // SAFETY: `p` is the live renderer; the entry locks first.
            unsafe { Renderer::display_unrealized_raw(p.0, &lock) };
            // Drain: the queue has room again.
            *mailbox.full.lock().unwrap() = false;
            mailbox.cv.notify_all();
        }
    });

    // The render thread (this test's thread, where the GL context is current).
    // SAFETY: `r_ptr` is the live renderer and `lock` its draw mutex.
    unsafe { Renderer::draw_frame_raw(r_ptr, &lock, true) }.expect("draw");
    main.join().expect("main thread");

    assert!(
        !mailbox.timed_out.load(Ordering::SeqCst),
        "the render thread blocked in a `.forever` push while holding the draw \
         mutex the main thread needed (UD362)"
    );
    assert!(
        mailbox
            .delivered
            .lock()
            .unwrap()
            .contains(&SurfaceMessage::Redraw),
        "`.redraw` was delivered once the main thread drained"
    );
    assert!(!r.display_realized, "the unrealize went through");

    deinit_test_renderer(&mut r);
}

// PORT NOTE: reuse the exact `opts(cols, rows)` recipe from `macos_impl::tests`
// (generic.rs:6066) — renamed `term_opts` to avoid shadowing the `Options`
// renderer struct that the other tests construct as a local `opts`.
fn term_opts(cols: CellCountInt, rows: CellCountInt) -> TermOptions {
    TermOptions {
        cols,
        rows,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        colors: Colors::default(),
        default_modes: ModePacked::default(),
        default_cursor_style: rustty_vt::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        kitty_image_storage_limit: 320 * 1000 * 1000,
        kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
    }
}

/// L4 T10 KAT: the CPU-side cell fill — mirrors `macos_impl::tests::
/// rebuild_cells_fills_bg_fg_cursor` (generic.rs:7178). Feeds "Ab" into a 10×3
/// terminal, updates the render state, calls [`Renderer::rebuild_cells`], and
/// asserts the CPU-side [`Contents`] buffers (bg cell, fg-glyph row, cursor
/// glyph) + the cursor uniform — NO GPU readback (that is Task 11's golden).
///
/// Uses [`Renderer::init`] (which stands up the offscreen EGL/GL context) as the
/// skip-probe; clean-skips (never panics/fabricates) when the host cannot give a
/// 4.3-core context — the Task-1 OPEN RISK path (default virgl here; runs for
/// real under `LIBGL_ALWAYS_SOFTWARE=1` → llvmpipe 4.5). Holds the process-wide
/// [`test_support::gl_lock`] for the whole context lifetime, `_guard` first so it
/// drops last (after `deinit`).
#[test]
fn linux_opengl_rebuild_cells_places_ab_and_cursor() {
    let _guard = test_support::gl_lock();
    let Some(device) =
        test_support::device_or_skip("linux_opengl_rebuild_cells_places_ab_and_cursor")
    else {
        return;
    };

    let (font_grid, size) = test_font_grid_and_size(10, 3);
    let opts = Options {
        device,
        config: derived_config_for_test(),
        font_grid,
        size,
        // Offscreen: no surface, hence no mailbox (push returns 0).
        surface_mailbox: SurfaceMailbox::DETACHED,
    };
    let mut r = match Renderer::init(Global, opts) {
        Ok(r) => r,
        Err(e) => {
            // W8-03 strict mode (see test_support docs).
            test_support::require_gl_or_skip("linux_opengl_rebuild_cells_places_ab_and_cursor", &e);
            eprintln!(
                "no headless OpenGL 4.3 core context ({e}); skipping \
                 linux_opengl_rebuild_cells_places_ab_and_cursor"
            );
            return;
        }
    };

    // small terminal: print "Ab" on row 0, leave a visible block cursor.
    let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Ab");
        s.deinit();
    }
    r.terminal_state.update(Global.into(), &mut t).unwrap();
    let cs = cursor::style(
        &r.terminal_state,
        cursor::StyleOptions {
            preedit: false,
            focused: true,
            blink_visible: true,
        },
    );
    r.rebuild_cells(None, cs, &CellSet::new());

    // (1) BG: the printed cell (0,0) has a bg entry; a default-bg cell → alpha 0.
    let bg = r.terminal_state.colors.background;
    assert_eq!(*r.cells.bg_cell(0, 0), [bg.r, bg.g, bg.b, 0]);

    // (2) FG glyph: row 0 fg list (index y+1 == 1) is non-empty (glyphs for A, b).
    assert!(!r.cells.fg_rows[1].is_empty());
    let a = r.cells.fg_rows[1][0];
    assert_eq!(a.grid_pos, [0, 0]);
    assert_eq!(a.atlas, Atlas::Grayscale);
    assert!(a.glyph_size[0] > 0 && a.glyph_size[1] > 0);

    // (3) CURSOR: a block cursor glyph is present, colored the fixture's red
    //     (config.cursor_color). After printing "Ab" the block cursor sits at
    //     column x=2 (the narrow empty cell), so BOTH the cursor uniform and the
    //     cursor glyph grid_pos are [2, 0] (ref :2529-2536).
    let cur = r.cells.get_cursor_glyph().expect("cursor present");
    assert!(cur.bools.is_cursor_glyph);
    assert_eq!(&cur.color[0..3], &[0xFF, 0, 0]);
    assert_eq!(cur.grid_pos, [2, 0]);
    assert_eq!(r.uniforms.cursor_pos, [2, 0]);

    deinit_test_renderer(&mut r);
    t.deinit(Global.into());
}

// GPU_RD_CHUNK4_DESIGN §3.2 / this file's Task-11 milestone: the GOLDEN harness =
// the offscreen `Renderer(OpenGL)` with its offscreen size PINNED to the grid's
// pixel size (`size.screen`), so `draw_frame`'s `surface_size == self.size.screen`
// ⇒ `size_changed == false` (projection 1:1; cell (x,y) at pixel
// [x*cw..(x+1)*cw) × [y*ch..(y+1)*ch)). Mirrors `macos_impl::tests::golden_renderer`
// (generic.rs:6357) — probe with `OpenGL::init_offscreen` FIRST, THEN `Renderer::init`.
//
// GL-CONTEXT SKIP (Task-1 OPEN RISK): if the host cannot stand up a 4.3-core
// context, `init_offscreen` fails ⇒ clean skip (returns None), mirroring the Metal
// "no device ⇒ skip" precedent. The probe context is dropped BEFORE
// `Renderer::init` stands up its own; the CALLER holds `test_support::gl_lock`,
// so the two are SEQUENTIAL.
fn golden_renderer(cols: CellCountInt, rows: CellCountInt) -> Option<Renderer> {
    if let Err(e) = OpenGL::init_offscreen(AlphaBlending::Native, 1, 1) {
        // W8-03 strict mode: this ONE arm covers every golden test — they
        // all probe through here (see test_support docs).
        test_support::require_gl_or_skip("golden_renderer probe", &e);
        eprintln!("no headless OpenGL 4.3 core context; skipping golden");
        return None;
    }

    let (font_grid, size) = test_font_grid_and_size(cols, rows);
    let opts = Options {
        device: test_support::device().expect("the test device (GL probed above)"),
        config: derived_config_for_test(),
        font_grid,
        size,
        // Offscreen: no surface, hence no mailbox (push returns 0).
        surface_mailbox: SurfaceMailbox::DETACHED,
    };
    let mut r = Renderer::init(Global, opts).expect("Renderer::init (GL probed above)");
    enter_test_renderer(&mut r);
    Some(r)
}

// rustty-only (R13, UPSTREAM-DEFECTS.md UD57): the OpenGL twin of
// `macos_impl`'s `update_custom_shader_uniforms_from_state_cell_relative_colors`.
// Cell-relative `cursor-text` / `selection-*` values leave their uniforms
// unchanged (the reference panics); a `.color` still writes.
#[test]
fn update_custom_shader_uniforms_from_state_cell_relative_colors() {
    let _guard = test_support::gl_lock();

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // GL-context skip
    };
    r.has_custom_shaders = true;

    let sentinel = [0.25, 0.5, 0.75, 1.0];
    r.custom_shader_uniforms.cursor_text = sentinel;
    r.custom_shader_uniforms.selection_background_color = sentinel;
    r.custom_shader_uniforms.selection_foreground_color = sentinel;
    r.custom_shader_uniforms.foreground_color = sentinel;
    // Sentinels for the two uniforms written AFTER the three colour
    // blocks (see the macOS twin): neither is a value the function can
    // produce, so they pin that it ran to its end.
    r.custom_shader_uniforms.cursor_visible = -7;
    r.custom_shader_uniforms.current_cursor_style = -7;
    r.config.cursor_text = Some(TerminalColor::CellForeground);
    r.config.selection_background = Some(TerminalColor::CellForeground);
    r.config.selection_foreground = Some(TerminalColor::CellBackground);

    let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
    r.terminal_state.update(Global.into(), &mut t).unwrap(); // dirty = Full
    r.update_custom_shader_uniforms_from_state();

    // Positive reach: the update ran past its early returns.
    let fg = r.terminal_state.colors.foreground;
    assert_eq!(
        r.custom_shader_uniforms.foreground_color,
        [
            fg.r as f32 / 255.0,
            fg.g as f32 / 255.0,
            fg.b as f32 / 255.0,
            1.0
        ]
    );
    assert_eq!(r.custom_shader_uniforms.cursor_text, sentinel);
    assert_eq!(
        r.custom_shader_uniforms.selection_background_color,
        sentinel
    );
    assert_eq!(
        r.custom_shader_uniforms.selection_foreground_color,
        sentinel
    );

    // Positive reach PAST the three colour blocks (see the macOS twin).
    assert_eq!(
        r.custom_shader_uniforms.cursor_visible,
        r.terminal_state.cursor.visible as i32
    );
    assert_ne!(r.custom_shader_uniforms.current_cursor_style, -7);

    // A `.color` value is still written.
    let red = Color { r: 255, g: 0, b: 0 };
    r.config.cursor_text = Some(TerminalColor::Color(red));
    r.config.selection_background = Some(TerminalColor::Color(red));
    r.config.selection_foreground = Some(TerminalColor::Color(red));
    r.update_custom_shader_uniforms_from_state();
    assert_eq!(r.custom_shader_uniforms.cursor_text, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        r.custom_shader_uniforms.selection_background_color,
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        r.custom_shader_uniforms.selection_foreground_color,
        [1.0, 0.0, 0.0, 1.0]
    );

    t.deinit(Global.into());
    deinit_test_renderer(&mut r);
}

// UD76 (R13, task R13-C2): the OpenGL twin of `macos_impl`'s
// `custom_shader_cursor_uniform_saturates_with_a_saturated_padding`. A
// saturated padding (UD71) with the cursor off the first column and row
// overflowed the reference's unchecked `grid_pos * cell + padding`.
#[test]
fn custom_shader_cursor_uniform_saturates_with_a_saturated_padding() {
    let _guard = test_support::gl_lock();

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // GL-context skip
    };
    r.has_custom_shaders = true;
    r.size.padding.left = u32::MAX;
    r.size.padding.top = u32::MAX;
    // `golden_renderer` leaves the cell contents unsized (the frame's
    // `rebuild_cells` sizes them); a cursor needs rows.
    r.cells.resize(rustty_render_types::GridSize {
        rows: 3,
        columns: 10,
    });
    let glyph = crate::shaders::CellText {
        atlas: Atlas::Grayscale,
        bools: crate::shaders::CellTextBools {
            is_cursor_glyph: true,
            ..Default::default()
        },
        grid_pos: [3, 2],
        color: [0, 0, 0, 255],
        ..Default::default()
    };
    r.cells.set_cursor(Some(glyph), Some(cursor::Style::Block));
    assert_eq!(Some(glyph), r.cells.get_cursor_glyph(), "precondition");

    r.update_custom_shader_uniforms_for_frame();

    // +Y is up in OpenGL: y is measured from the bottom of the screen.
    let c = r.custom_shader_uniforms.current_cursor.0;
    let h = r.size.screen.height as f32;
    assert_eq!(u32::MAX as f32, c[0], "x saturates: {c:?}");
    assert!(c[1] <= h - u32::MAX as f32, "y saturates: {c:?}");
    deinit_test_renderer(&mut r);
}

// F-014 PROVING TESTS (the OpenGL twin of
// `macos_impl::tests::surface_mailbox_routing`): the surface-mailbox seam
// (GPU_RD_CHUNK3A_DESIGN §B.2) is RESOLVED — `frameCompleted`'s
// `.renderer_health` push and `drawFrame`'s `.scrollbar` push actually reach the
// mailbox. Before the fix `SurfaceMailbox::push` was a hard-coded `0`, so both
// sinks stayed EMPTY and `scrollbar_dirty` could never be cleared (the reported
// "Linux GTK: renderer scrollbar updates are discarded by a no-op mailbox").
mod surface_mailbox_routing {
    use std::sync::Arc;

    // `use super::*` picks up the enclosing `tests` module's helpers +
    // imports (`golden_renderer`, `term_opts`, `Renderer`, `Global`, `Mutex`,
    // `Terminal`, `state`, `test_support`); the seam types live one level
    // further up (`crate::generic`) and are named explicitly.
    use super::*;
    use crate::generic::surface_mailbox_tests::RecordingSink;
    use crate::generic::{MailboxTimeout, SurfaceMailbox, SurfaceMessage};
    use rustty_vt::Scrollbar;

    /// Drive one real frame: `update_frame` (refreshes the scrollbar cache from
    /// the terminal and raises `scrollbar_dirty`) then `draw_frame(true)` (whose
    /// `defer` runs the scrollbar push). Returns the pushed scrollbar value.
    fn update_and_draw(r: &mut Renderer) -> Scrollbar {
        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
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
        // Guard against a vacuous test: the fresh terminal's scrollbar differs
        // from the `Scrollbar::ZERO` the renderer was built with, so
        // `update_frame` raised the dirty flag.
        assert!(
            r.scrollbar_dirty,
            "update_frame must have cached a fresh scrollbar (dirty)"
        );
        let scrollbar = r.scrollbar;
        r.draw_frame(true).unwrap();
        // The frame is drawn; the terminal is this helper's to free (it was
        // leaked, NEW-FINDINGS N411).
        drop(st);
        t.deinit(Global.into());
        scrollbar
    }

    /// generic.zig:1725-1737 — `frameCompleted` pushes `.renderer_health` with
    /// `.forever` ONLY on a health TRANSITION.
    #[test]
    fn frame_completed_pushes_renderer_health_on_transition_only() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };
        let sink = Arc::new(RecordingSink::accepting());
        r.surface_mailbox = SurfaceMailbox::new(sink.clone());
        // A frame only completes once the swap chain exists; the renderer
        // builds it lazily on the first draw (`2f0b65346`), so build it here.
        r.swap_chain = Some(crate::generic::SwapChain::init(&r.api, false).expect("swap chain"));

        // `health` starts `.healthy`, so this IS a transition → one push.
        r.frame_completed(crate::Health::Unhealthy);
        // Identical health → `if (self.health.load(...) != health)` is false.
        r.frame_completed(crate::Health::Unhealthy);
        // UD362: the `.forever` push waits for the draw mutex's release,
        // which `draw_frame_raw` does by sending the deferred pushes.
        assert!(sink.pushed().is_empty(), "nothing is sent under the lock");
        r.take_deferred_surface_pushes().push_all();

        assert_eq!(
            sink.pushed(),
            vec![(
                SurfaceMessage::RendererHealth(crate::Health::Unhealthy),
                MailboxTimeout::Forever
            )]
        );

        deinit_test_renderer(&mut r);
    }

    /// generic.zig:1449-1456 — the `drawFrame` `defer` pushes `.scrollbar` with
    /// `.instant` and clears `scrollbar_dirty` when the push lands. This is the
    /// exact hop the GTK scrollbar adjustment depends on.
    #[test]
    fn draw_frame_pushes_scrollbar_and_clears_dirty() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };
        let sink = Arc::new(RecordingSink::accepting());
        r.surface_mailbox = SurfaceMailbox::new(sink.clone());

        let scrollbar = update_and_draw(&mut r);

        // The scrollbar `defer` runs at the end of `drawFrameLocked`, under
        // the lock; the completed frame's `.redraw` (`Frame.zig:83`) is sent
        // after the unlock (UD362), so it lands second.
        assert_eq!(
            sink.pushed(),
            vec![
                (
                    SurfaceMessage::Scrollbar(scrollbar),
                    MailboxTimeout::Instant
                ),
                (SurfaceMessage::Redraw, MailboxTimeout::Forever),
            ]
        );
        assert!(
            !r.scrollbar_dirty,
            "a landed scrollbar push must clear scrollbar_dirty"
        );

        deinit_test_renderer(&mut r);
    }

    /// The other arm of the same `if (... > 0)`: "Fail instantly if the surface
    /// mailbox if full, we'll just get it on the next frame" — a refused push
    /// must KEEP `scrollbar_dirty` set.
    #[test]
    fn draw_frame_keeps_scrollbar_dirty_when_the_mailbox_is_full() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };
        let sink = Arc::new(RecordingSink::refusing());
        r.surface_mailbox = SurfaceMailbox::new(sink.clone());

        let scrollbar = update_and_draw(&mut r);

        // (the frame's `.redraw` is refused too; `2f0b65346`. It is sent
        // after the unlock, UD362.)
        assert_eq!(
            sink.pushed(),
            vec![
                (
                    SurfaceMessage::Scrollbar(scrollbar),
                    MailboxTimeout::Instant
                ),
                (SurfaceMessage::Redraw, MailboxTimeout::Forever),
            ]
        );
        assert!(
            r.scrollbar_dirty,
            "a refused scrollbar push must keep scrollbar_dirty set"
        );

        deinit_test_renderer(&mut r);
    }
}

// Sample the RGBA byte 4-tuple at pixel (x, y) in a tight `w*h*4` readback
// (`Target::read_rgba` returns `[R,G,B,A]` per pixel, rows tight). Returns
// (r,g,b,a). The OpenGL twin of `macos_impl::tests::px_at` (generic.rs:6453),
// which samples BGRA — the byte order differs because the two backends read back
// in their native order, but the STRUCTURAL assertions are the same.
fn px_at(px: &[u8], w: usize, x: usize, y: usize) -> (u8, u8, u8, u8) {
    let i = (y * w + x) * 4;
    (px[i], px[i + 1], px[i + 2], px[i + 3])
}

/// Build the `state::State` the two Kitty tests drive `update_frame` with.
/// (Split out so the two tests cannot drift apart; the golden above keeps its
/// own inline literal, untouched.)
fn kitty_state(mutex: &mut Mutex, terminal: &mut Terminal) -> state::State {
    state::State {
        mutex,
        terminal,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: state::Mouse::default(),
        // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    }
}

/// Encode a `w`×`h` solid-RGB PNG (8-bit) for the bg-image golden. The reference
/// `background-image` fixtures are not checked in, so generate one at test time
/// via the pure-Rust `png` encoder. (Test-helper duplication precedent — the
/// Metal twin has the same helper at `generic.rs:6842`; `linux_impl`'s tests
/// deliberately do not reach into `macos_impl`, which is not even compiled here.)
fn encode_solid_png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().expect("png header");
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..(w * h) {
            data.extend_from_slice(&rgb);
        }
        writer.write_image_data(&data).expect("png data");
        writer.finish().expect("png finish");
    }
    out
}

mod config_overlay;
mod golden;

// ===================================================================
// task-14c-5 (finding N9) — the RESTORED diagnostics of this Linux twin
// ===================================================================

/// Proof that `generic.zig`'s `log.*` family is live in the OpenGL arm, not
/// only in the Metal one. Four shapes that break INDEPENDENTLY:
///
/// 1. a warn carrying only the (byte-lossy) path — `generic.zig:1995`;
/// 2. a warn carrying an extra formatted argument — `generic.zig:2002`;
/// 3. the SUCCESS path of the same function, which must emit NOTHING (this
///    project has shipped a diagnostic-on-the-success-path bug once already,
///    and three sites in this file were deliberately left unrestored for
///    exactly that reason — see the `LOG PORT NOTE`s);
/// 4. a DEBUG-level site in a different function — `generic.zig:2137`.
///
/// All four need a live `Renderer`, hence a GL context: they clean-skip through
/// `golden_renderer` and HARD-FAIL under `RUSTTY_REQUIRE_GL=1`.
mod restored_log_sites {
    use super::*;
    use crate::log_capture::CapturedRecord;
    use std::ffi::CString;

    /// Write `bytes` to a uniquely-named temp file with the given extension and
    /// return (path, the `CString` the config holds, the `{s}` rendering the
    /// oracle prints for it).
    fn temp_bg_file(tag: &str, ext: &str, bytes: &[u8]) -> (std::path::PathBuf, CString, String) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rustty_gl_bg_{tag}_{nanos}.{ext}"));
        std::fs::write(&path, bytes).unwrap();
        let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();
        let display = String::from_utf8_lossy(cpath.to_bytes()).into_owned();
        (path, cpath, display)
    }

    /// Run `prep_background_image` over `cpath` under capture and return every
    /// record it emitted on this thread.
    fn prep_under_capture(r: &mut Renderer, cpath: CString) -> Vec<CapturedRecord> {
        r.config.bg_image = Some(crate::generic::Path::Required(cpath));
        let cap = crate::log_capture::start();
        let res = r.prep_background_image();
        let records = cap.messages();
        drop(cap);
        res.expect("prep_background_image must not error on a break-arm path");
        records
    }

    /// `generic.zig:1995` — neither the magic bytes nor the extension identify
    /// the file, so the load block breaks with the "Cannot determine" warn.
    #[test]
    fn undetectable_file_type_warns_once() {
        let _guard = test_support::gl_lock();
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        let (path, cpath, display) = temp_bg_file("undetectable", "xyz", b"not an image at all");
        let records = prep_under_capture(&mut r, cpath);
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            records,
            vec![(
                log::Level::Warn,
                "generic_renderer".to_string(),
                format!("Cannot determine file type for background image file \"{display}\"!"),
            )]
        );
        // The break arm leaves the renderer's bg image untouched.
        assert!(r.bg_image.is_none());
        deinit_test_renderer(&mut r);
    }

    /// `generic.zig:2002` — GIF magic bytes are DETECTED, but the port decodes
    /// only PNG and JPEG, so this is the `else => |f|` arm and its message
    /// carries the detected type as a second argument. A restore that dropped
    /// that argument passes test 1 above and fails this one.
    #[test]
    fn unsupported_file_type_warns_with_the_detected_type() {
        let _guard = test_support::gl_lock();
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        // GIF87a magic — `FileType::detect` returns `Gif`, which the decode
        // switch does not handle.
        let (path, cpath, display) =
            temp_bg_file("unsupported", "gif", b"GIF87a\x01\x00\x01\x00\x00\x00\x00");
        let records = prep_under_capture(&mut r, cpath);
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            records,
            vec![(
                log::Level::Warn,
                "generic_renderer".to_string(),
                format!("Unsupported file type Gif for background image file \"{display}\"!"),
            )]
        );
        assert!(r.bg_image.is_none());
        deinit_test_renderer(&mut r);
    }

    /// The success path of the very same function must be SILENT. This is the
    /// guard against the shape this project shipped once — a warning hoisted
    /// out of its failure arm, firing on every successful call.
    #[test]
    fn a_successful_decode_emits_nothing() {
        let _guard = test_support::gl_lock();
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        let png = encode_solid_png(2, 2, [0x30, 0x60, 0xC0]);
        let (path, cpath, _display) = temp_bg_file("ok", "png", &png);
        let records = prep_under_capture(&mut r, cpath);
        let _ = std::fs::remove_file(&path);

        assert_eq!(records, Vec::<CapturedRecord>::new());
        // and it really did the work, so the silence is not the silence of a
        // function that never ran.
        assert!(matches!(r.bg_image, Some(crate::image::Image::Pending(_))));
        deinit_test_renderer(&mut r);
    }

    /// `generic.zig:2137` — a DEBUG-level site in a different function. Level
    /// and scope are asserted, so a restore that used `log::warn!` or the wrong
    /// target fails here even though the message text is right.
    #[test]
    fn set_screen_size_emits_one_debug_record() {
        let _guard = test_support::gl_lock();
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        let mut size = r.size;
        size.padding = Padding {
            top: 1,
            bottom: 2,
            left: 3,
            right: 4,
        };

        let cap = crate::log_capture::start();
        r.set_screen_size(size);
        let records = cap.messages();
        drop(cap);

        assert_eq!(
            records,
            vec![(
                log::Level::Debug,
                "generic_renderer".to_string(),
                format!("screen size size={size:?}"),
            )]
        );
        assert_eq!(r.size.padding, size.padding);
        deinit_test_renderer(&mut r);
    }
}
