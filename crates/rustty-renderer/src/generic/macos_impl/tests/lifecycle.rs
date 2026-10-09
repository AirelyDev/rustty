//! Tests for `generic::macos_impl` (ghostty src/renderer/generic.zig, the
//! `Renderer(Metal)` arm) — the restored `prepBackgroundImage` log sites, the windowed
//! lifecycle (`init`/`deinit` + host-view attach) and the offscreen resize.

use super::*;

// ===================================================================
// task-14c-1 (`LOG` class, rows S115-F4 / S116-F2): the restored
// `std.log.scoped(.generic_renderer)` diagnostics actually EMIT.
//
// Three tests, one per independently-breakable shape (the task brief's
// own list), all on `prepBackgroundImage`'s three `break :load_background`
// arms — the only `generic.zig` diagnostics reachable from a unit test
// without a live terminal frame:
//
//   1. a plain single-argument `log.warn` (`Cannot determine file type
//      ...`),
//   2. a site whose preserved Zig comment was ELIDED with `...`
//      (`Unsupported file type {} ...`) — the arguments exist only in the
//      oracle, so this test fails for anything restored from the comment,
//   3. a site that was standing in with `eprintln!` (`error opening
//      background image file ...`), which an embedder's `log::Log` backend
//      could never see.
//
// Each asserts the FULL captured record vector — level, target and exact
// message text — so deleting the emission fails the test. Metal-skip on a
// headless host, matching every other test in this module.
mod restored_log_sites {
    use super::*;

    /// Builds a golden renderer whose bg-image config points at `bytes`
    /// written to a temp file with `ext`, runs `prep_background_image`
    /// with the capture logger installed, and returns
    /// `(records, path_display)`. `None` ⇒ no Metal device, skip.
    fn prep_with_bg_file(
        ext: &str,
        bytes: &[u8],
    ) -> Option<(Vec<crate::log_capture::CapturedRecord>, String)> {
        let mut r = golden_renderer(4, 2)?;

        let path = std::env::temp_dir().join(format!(
            "rustty-renderer-log-{}-{:?}.{ext}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, bytes).expect("write fixture");

        let path_bytes = path.as_os_str().as_encoded_bytes().to_vec();
        let path_display = String::from_utf8(path_bytes.clone()).unwrap();
        r.config.bg_image = Some(crate::generic::Path::Required(
            CString::new(path_bytes).unwrap(),
        ));

        let cap = crate::log_capture::start();
        r.prep_background_image().expect("prep_background_image");
        let records = cap.messages();
        drop(cap);

        let _ = std::fs::remove_file(&path);
        deinit_test_renderer(&mut r);
        Some((records, path_display))
    }

    // Shape 1 — a plain single-argument `log.warn` on a reachable path.
    // oracle `generic.zig:1995`:
    //   log.warn(
    //       "Cannot determine file type for background image file \"{s}\"!",
    //       .{path},
    //   );
    #[test]
    fn unknown_file_type_warns() {
        // Junk bytes + an extension `guessFromExtension` does not know ⇒
        // `FileType.detect` and the extension guess both say `.unknown`.
        let Some((records, path_display)) = prep_with_bg_file("xyz", b"not an image at all") else {
            // `golden_renderer` already ran the F5 strict-mode pivot; on a
            // host without a device we say so rather than skipping silently.
            eprintln!("no Metal device; skipping unknown_file_type_warns");
            return;
        };

        assert_eq!(
            records,
            vec![(
                log::Level::Warn,
                "generic_renderer".to_string(),
                format!("Cannot determine file type for background image file \"{path_display}\"!"),
            )],
        );
    }

    // Shape 2 — a site restored from the ORACLE, not from the elided
    // comment. `generic.rs`'s preserved comment reads
    //   `else => |f| { log.warn("Unsupported file type {} ...", .{ f, path }); break; },`
    // whose message text is truncated at `...`. The real oracle line
    // (`generic.zig:2002`) is
    //   log.warn(
    //       "Unsupported file type {} for background image file \"{s}\"!",
    //       .{ f, path },
    //   );
    // A restoration made from the comment emits `Unsupported file type {} ...`
    // and fails here.
    #[test]
    fn unsupported_file_type_warns_with_the_full_oracle_message() {
        // `GIF89a` magic ⇒ `FileType::detect` == `Gif`: a KNOWN type that
        // `prepBackgroundImage` has no decoder for (only png/jpeg), i.e.
        // the reference's `else => |f|` prong.
        let Some((records, path_display)) =
            prep_with_bg_file("gif", b"GIF89a\x01\x00\x01\x00\x00\x00\x00;")
        else {
            // `golden_renderer` already ran the F5 strict-mode pivot.
            eprintln!(
                "no Metal device; skipping \
                 unsupported_file_type_warns_with_the_full_oracle_message"
            );
            return;
        };

        assert_eq!(
            records,
            vec![(
                log::Level::Warn,
                "generic_renderer".to_string(),
                format!("Unsupported file type Gif for background image file \"{path_display}\"!"),
            )],
        );
    }

    // Shape 3 — a site that was an `eprintln!` stand-in. Before this stage
    // the open failure printed
    //   `warning(renderer): error opening/reading background image file {:?}`
    // to stderr: not a record any `log::Log` backend could filter or route,
    // and not the oracle's text either (the port had merged the reference's
    // distinct open/read messages into one invented string).
    // oracle `generic.zig:1960`:
    //   log.warn(
    //       "error opening background image file \"{s}\": {}",
    //       .{ path, err },
    //   );
    #[test]
    fn missing_background_image_file_warns_through_the_facade() {
        let Some(mut r) = golden_renderer(4, 2) else {
            // `golden_renderer` already ran the F5 strict-mode pivot.
            eprintln!(
                "no Metal device; skipping \
                 missing_background_image_file_warns_through_the_facade"
            );
            return;
        };

        let path = std::env::temp_dir().join(format!(
            "rustty-renderer-log-missing-{}-{:?}.png",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_file(&path);

        let path_bytes = path.as_os_str().as_encoded_bytes().to_vec();
        let path_display = String::from_utf8(path_bytes.clone()).unwrap();
        r.config.bg_image = Some(crate::generic::Path::Required(
            CString::new(path_bytes).unwrap(),
        ));

        // The exact `{err:?}` rendering the site will produce, obtained the
        // same way the site obtains it — so the assertion stays EXACT
        // without hard-coding a platform errno string.
        let expected_err = format!(
            "{:?}",
            std::fs::File::open(&path).expect_err("fixture path must not exist")
        );

        let cap = crate::log_capture::start();
        r.prep_background_image().expect("prep_background_image");
        let records = cap.messages();
        drop(cap);

        assert_eq!(
            records,
            vec![(
                log::Level::Warn,
                "generic_renderer".to_string(),
                format!("error opening background image file \"{path_display}\": {expected_err}"),
            )],
        );
        deinit_test_renderer(&mut r);
    }
}

// W1-7 KAT: the `Renderer` windowed lifecycle round-trip. Exercises the
// apprt-free surface handshake (`surface_init`/`finalize_surface_init` are
// FAITHFUL no-ops for the Metal monomorphization — `@hasDecl(GraphicsAPI,
// "surfaceInit"/"finalizeSurfaceInit")` is FALSE, Metal declares only
// `loopEnter`), then the now-filled `Renderer::init` (offscreen-portable device
// ctor, layer `None`; DEFERRALS.md #3) → `deinit` (verbatim teardown). `deinit`
// must not panic or leak (ASan covers UB across the objc/Metal FFI). Probe-skips
// on a host with no Metal device, matching the W0 KAT. FFI ⇒ Miri-excluded (R6).
//
// Placed in a `windowed_lifecycle` submodule so `.port/phase-2/MIRI_EXCEPTIONS.md`
// can name `generic::windowed_lifecycle::windowed_renderer_init_then_deinit_round_trips`.
mod windowed_lifecycle {
    use objc2::runtime::{AnyObject, NSObject};

    use super::*;

    #[test]
    fn windowed_renderer_init_then_deinit_round_trips() {
        // Probe: skip cleanly on hosts with no Metal device (headless CI).
        if let Err(e) = crate::metal::test_support::offscreen_metal(AlphaBlending::Native, 1, 1) {
            // F5 strict-mode pivot (metal::test_support).
            crate::metal::test_support::require_metal_or_skip(
                "windowed_renderer_init_then_deinit_round_trips",
                &format!("{e:?}"),
            );
            eprintln!("no Metal device; skipping windowed_lifecycle");
            return;
        }

        // A real objc object — the `*mut S` handed to the generic surface hooks.
        // We use a bare `NSObject` (from the objc2 base crate — no AppKit
        // framework, which the lib test binary does not link, and which R4
        // forbids depending on) rather than a live NSView: the hooks IGNORE the
        // pointer (Metal declares neither `@hasDecl` hook — they are faithful
        // no-ops), and the design permits "any `*mut S`". A real objc object
        // still keeps the KAT honest about the bare generic `<S>` (`S = NSObject`,
        // erased to `*mut AnyObject`).
        let obj: objc2::rc::Retained<NSObject> = NSObject::new();
        // `&*obj` (&NSObject) → `*mut AnyObject`. The hooks never deref it, so a
        // shared-ref-derived cross-type `*mut` is sound (no aliasing write occurs).
        let view_ptr: *mut AnyObject = &*obj as *const NSObject as *mut AnyObject;

        // P7-W7 T2: the `surfaceInit` half of the old handshake is gone
        // (`2f0b65346`); `threadEnter` stays a faithful no-op for Metal.
        let (font_grid, size) = test_font_grid_and_size(10, 3);
        let opts = Options {
            device: crate::metal::test_support::device()
                .expect("the test device (Metal probed above)"),
            config: derived_config_for_test(),
            font_grid,
            size,
            // Offscreen: no surface, hence no mailbox (push returns 0).
            surface_mailbox: SurfaceMailbox::DETACHED,
        };
        // SAFETY: `opts.device` is the process's test device (created once, never
        // freed) and `opts.font_grid` the boxed test grid, freed only after the
        // renderer's `deinit`, so both outlive `r`.
        let mut r =
            unsafe { Renderer::init(Global, opts) }.expect("Renderer::init (Metal probed above)");

        r.thread_enter(view_ptr).expect("thread_enter (no-op)");

        // The windowed teardown must not panic/leak (N503: the grid too).
        deinit_test_renderer(&mut r);
    }
}

// PORT NOTE (rustty-only, additive — no reference `test{}`): proves the
// OFFSCREEN seam M-3 setter actually reaches the drawn surface, which is the
// whole reason it exists. `set_offscreen_size` writes a private field; what
// an embedder needs is for the NEXT frame to be that size. So: draw once at
// the constructed size, resize, draw again, and read the swap-chain target
// (the IOSurface an embedder composites) plus `size.screen` back out. Without
// the setter both stay pinned to `init_offscreen`'s size forever.
mod offscreen_resize {
    use super::*;

    /// Drive one real frame, as the golden tests do.
    fn draw_one_frame(r: &mut Renderer) {
        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
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
        r.draw_frame(true).unwrap();
        // N503: the reference's `defer t.deinit(alloc)`.
        t.deinit(Global.into());
    }

    /// The target an embedder reads (`swap_chain.frames[frame_index].target`).
    fn target_size(r: &Renderer) -> (usize, usize) {
        let chain = r.swap_chain.as_ref().expect("a swap chain after a frame");
        let frame = &chain.frames[chain.frame_index as usize];
        (frame.target.width, frame.target.height)
    }

    #[test]
    fn set_offscreen_size_resizes_the_next_frames_surface() {
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // Metal-skip
        };

        draw_one_frame(&mut r);
        let built = r.size.screen;
        // `golden_renderer` pins `offscreen_size == size.screen`, so the
        // first frame drew at the constructed size — the state an embedder
        // used to be stuck in.
        assert_eq!(
            target_size(&r),
            (built.width as usize, built.height as usize),
            "the first frame must draw at the constructed offscreen size"
        );

        // A size no cell quantisation could have produced by itself, so a
        // pass cannot be an accident of the grid.
        let grown = ScreenSize {
            width: built.width + 37,
            height: built.height + 11,
        };
        r.set_offscreen_size(grown);
        assert_eq!(
            r.api.surface_size().unwrap(),
            crate::metal::metal_impl::SurfaceSize {
                width: grown.width,
                height: grown.height,
            },
            "the setter must move what `draw_frame` reads as the surface size"
        );

        draw_one_frame(&mut r);
        // `draw_frame`'s `size_changed` branch wrote `size.screen`...
        assert_eq!(r.size.screen, grown, "draw_frame must adopt the new size");
        // ...and `FrameState::resize` replaced the target behind it.
        assert_eq!(
            target_size(&r),
            (grown.width as usize, grown.height as usize),
            "the swap-chain target must follow the new offscreen size"
        );

        deinit_test_renderer(&mut r);
    }

    /// `set_screen_size` (the reference's `setScreenSize`) owns
    /// `size.padding`; `set_offscreen_size` owns the surface. They compose —
    /// neither clobbers the other — which is what an embedder relies on when
    /// it resizes the surface and the grid together.
    #[test]
    fn set_offscreen_size_and_set_screen_size_compose() {
        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // Metal-skip
        };

        let padding = Padding {
            top: 3,
            bottom: 4,
            left: 5,
            right: 6,
        };
        let mut size = r.size;
        size.padding = padding;
        r.set_screen_size(size);

        let grown = ScreenSize {
            width: r.size.screen.width + 37,
            height: r.size.screen.height + 11,
        };
        r.set_offscreen_size(grown);
        draw_one_frame(&mut r);

        assert_eq!(r.size.screen, grown, "the surface resize must land");
        assert_eq!(
            r.size.padding, padding,
            "and must not discard the padding `set_screen_size` set"
        );

        deinit_test_renderer(&mut r);
    }
}

/// `140feb86a`: `threadExit` marks the display unrealized before
/// `releaseGpuResources`. The macOS apprt never unrealizes, so without it each
/// closed surface leaked its render pipelines and shader library; it also
/// stops any later draw.
#[test]
fn thread_exit_unrealizes_the_display() {
    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // Metal-skip
    };
    r.draw_frame(false).expect("first draw");
    assert!(r.display_realized);
    assert!(r.swap_chain.is_some());
    assert!(!r.shaders.defunct);

    r.thread_exit();
    assert!(!r.display_realized, "threadExit unrealizes the display");
    assert!(r.swap_chain.is_none());
    assert!(r.shaders.defunct, "the shaders are released on thread exit");

    // A later draw returns early and rebuilds nothing.
    r.draw_frame(true).expect("draw after thread exit");
    assert!(r.swap_chain.is_none());
    assert!(r.shaders.defunct);

    deinit_test_renderer(&mut r);
}
