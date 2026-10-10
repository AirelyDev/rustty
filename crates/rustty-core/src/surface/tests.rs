//! Tests for `surface.rs` and its `surface/` children. Rustty-only: `Surface.zig` has no
//! reference `test{}` block (see the G1b `core_surface` row).

use super::*;

// rustty-only (R13, UPSTREAM-DEFECTS.md UD59): the initial-size and
// size-limit arithmetic with values that come through the config loader.
// `Surface::init` cannot run without an apprt, so the tests drive the two
// helpers `recomputeInitialSize` and `init` call, with the parsed values
// (`DerivedConfig` copies `window-width` / `window-height` unchanged).
#[cfg(test)]
mod r13_window_size_overflow_tests {
    use super::*;

    fn load(text: &[u8]) -> configpkg::Config {
        let mut cfg = configpkg::Config::default_config();
        let mut iter = rustty_args::args::LineIterator::init(text);
        cfg.load_iter(&mut iter).unwrap();
        cfg.finalize().unwrap();
        assert!(cfg._diagnostics.empty(), "unexpected config diagnostics");
        cfg
    }

    fn size(cell_width: u32, cell_height: u32, padding: u32) -> rendererpkg::size::Size {
        rendererpkg::size::Size {
            screen: rendererpkg::size::ScreenSize {
                width: 800,
                height: 600,
            },
            cell: rendererpkg::size::CellSize {
                width: cell_width,
                height: cell_height,
            },
            padding: rendererpkg::size::Padding {
                top: padding,
                bottom: padding,
                right: padding,
                left: padding,
            },
        }
    }

    const SCALE: apprt::ContentScale = apprt::ContentScale { x: 1.0, y: 1.0 };

    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-core, parity tail C-1): `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn initial_size_px_overflow_takes_the_skip_path() {
        // Positive control: 80x24 cells of 10x20 px, 2 px padding.
        let cfg = load(b"window-width = 80\nwindow-height = 24\n");
        assert_eq!(
            Some((80 * 10 + 4, 24 * 20 + 4)),
            initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 2), SCALE)
        );

        // The brief's trigger: 1000000000 * 20 overflows the `u32` product.
        let cfg = load(b"window-width = 80\nwindow-height = 1000000000\n");
        assert_eq!(
            1_000_000_000, cfg.window_height,
            "the value reaches the surface"
        );
        assert_eq!(
            None,
            initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 2), SCALE)
        );
        let cfg = load(b"window-width = 1000000000\nwindow-height = 24\n");
        assert_eq!(
            None,
            initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 2), SCALE)
        );

        // The product fits (214748364 * 20 = 4294967280) but `@floatFromInt` to
        // f32 rounds it to 2^32, which `@intFromFloat` into `u32` cannot hold.
        // No padding here, so a saturating conversion (4294967295) is not
        // caught by the padding addition instead.
        let cfg = load(b"window-width = 80\nwindow-height = 214748364\n");
        assert_eq!(
            None,
            initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 0), SCALE)
        );

        // The scaled size fits, and adding the padding overflows. (2147483 * 2000
        // = 4294966000; f32 steps by 256 there, so it rounds to 4294966016.)
        let cfg = load(b"window-width = 80\nwindow-height = 2147483\n");
        assert_eq!(
            Some((80 * 10 + 4, 4_294_966_016 + 4)),
            initial_size_px(
                cfg.window_width,
                cfg.window_height,
                size(10, 2000, 2),
                SCALE
            )
        );
        assert_eq!(
            None,
            initial_size_px(
                cfg.window_width,
                cfg.window_height,
                size(10, 2000, 1_000),
                SCALE
            )
        );
    }

    /// UD71 (R13, task R13-C): `Surface.zig:437-455` `scaledPadding` and the
    /// padding sums in `renderer/size.zig`. `window-padding-x = 4294967295` at
    /// 144 dpi is out of `u32` (the reference traps on the cast); `window-padding-y
    /// = 2147483648` at 72 dpi converts, but its top + bottom overflows `u32` in
    /// `subPadding` (the reference traps there). The port saturates both, and the
    /// grid keeps its one-cell minimum.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-core, parity tail C-1): `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn an_oversized_window_padding_saturates_and_leaves_a_one_cell_grid() {
        // Positive control: an ordinary padding is scaled by the dpi.
        let cfg = load(b"window-padding-x = 2\nwindow-padding-y = 3\n");
        let mut dc = DerivedConfig::init(Allocator, &cfg).unwrap();
        let pad = dc.scaledPadding(144.0, 144.0);
        assert_eq!((4, 4, 6, 6), (pad.left, pad.right, pad.top, pad.bottom));
        dc.deinit();

        let cfg = load(b"window-padding-x = 4294967295\nwindow-padding-y = 2147483648\n");
        assert_eq!(
            u32::MAX,
            cfg.window_padding_x.top_left,
            "the value reaches the config"
        );
        let mut dc = DerivedConfig::init(Allocator, &cfg).unwrap();
        let pad = dc.scaledPadding(144.0, 72.0);
        dc.deinit();
        assert_eq!(
            (u32::MAX, u32::MAX),
            (pad.left, pad.right),
            "saturated, not trapped"
        );
        assert_eq!(
            (1 << 31, 1 << 31),
            (pad.top, pad.bottom),
            "in range; the SUM is not"
        );

        let mut sz = size(10, 20, 0);
        sz.padding = pad;
        assert_eq!(
            rendererpkg::size::ScreenSize {
                width: 0,
                height: 0
            },
            sz.terminal(),
            "the padding leaves no room"
        );
        let grid = sz.grid();
        assert_eq!((1, 1), (grid.columns, grid.rows), "the grid keeps one cell");
        let blank = sz.screen.blank_padding(pad, grid, sz.cell);
        assert_eq!((0, 0), (blank.right, blank.bottom));

        // Surface.init's balancing path with the same explicit padding.
        sz.balance_padding(pad, configpkg::value_types::WindowPaddingBalance::True);
        assert_eq!((1, 1), (sz.grid().columns, sz.grid().rows));
    }

    /// UD95 (R13, task R13-D): `click-repeat-interval` above 4294 ms. The
    /// reference computes `config.@"click-repeat-interval" * 1_000_000` in `u32`
    /// (`Surface.zig:395`) and traps (ReleaseFast: 5000 -> 705032704 ns,
    /// measured). Driven through the config loader into a real `DerivedConfig`.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-core, parity tail C-1): `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn a_click_repeat_interval_above_u32_nanoseconds_is_widened_not_trapped() {
        // Positive control: an ordinary interval.
        let cfg = load(b"click-repeat-interval = 500\n");
        let mut dc = DerivedConfig::init(Allocator, &cfg).unwrap();
        assert_eq!(500_000_000, dc.mouse_interval);
        dc.deinit();

        for ms in [5000u32, u32::MAX] {
            let cfg = load(format!("click-repeat-interval = {ms}\n").as_bytes());
            assert_eq!(
                ms, cfg.click_repeat_interval,
                "the value reaches the config"
            );
            let mut dc = DerivedConfig::init(Allocator, &cfg).unwrap();
            assert_eq!(
                u64::from(ms) * 1_000_000,
                dc.mouse_interval,
                "{ms} ms is {ms}000000 ns, not a wrapped u32"
            );
            dc.deinit();
        }
    }

    /// `u32_from_ceil` on a NaN (task R13-C). The reference's `@as(u32,
    /// @intFromFloat(@ceil(width_f32 / scale.x)))` with a NaN content scale does
    /// not trap: it yields 0, so it would send an initial size of 0 plus the
    /// padding (measured, Zig 0.16.0, `final_width = 8` for 800 px and 4 px
    /// padding a side, Debug/ReleaseSafe/ReleaseFast, arm64 and x86_64 under
    /// Rosetta 2). The port skips instead; see `u32_from_ceil`.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-core, parity tail C-1): `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn initial_size_px_with_a_nan_content_scale_takes_the_skip_path() {
        let cfg = load(b"window-width = 80\nwindow-height = 24\n");
        // Positive control: the same config with a real scale.
        assert_eq!(
            Some((80 * 10 + 8, 24 * 20 + 8)),
            initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 4), SCALE)
        );
        for scale in [
            apprt::ContentScale {
                x: f32::NAN,
                y: 1.0,
            },
            apprt::ContentScale {
                x: 1.0,
                y: f32::NAN,
            },
        ] {
            assert_eq!(
                None,
                initial_size_px(cfg.window_width, cfg.window_height, size(10, 20, 4), scale),
                "a NaN content scale ({scale:?}) sends no initial size"
            );
        }
        assert_eq!(None, u32_from_ceil(f32::NAN));
        assert_eq!(Some(0), u32_from_ceil(-0.5), "ceil(-0.5) is -0, which is 0");
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-core, parity tail C-1): `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn size_limit_min_overflow_skips_the_action() {
        // `adjust-cell-width` adds an absolute `i32` to the face's cell width.
        let cfg = load(b"adjust-cell-width = 500000000\n");
        let cell_width = cfg
            .adjust_cell_width
            .expect("adjust-cell-width parsed")
            .apply_u32(10);
        assert_eq!(
            500_000_010, cell_width,
            "the modifier reaches the cell size"
        );
        assert_eq!(None, size_limit_min(cell_width, 20));

        // Positive control, and the largest cell that fits.
        assert_eq!(Some((100, 80)), size_limit_min(10, 20));
        assert_eq!(Some((4_294_967_290, 80)), size_limit_min(u32::MAX / 10, 20));
        assert_eq!(None, size_limit_min(10, u32::MAX / 4 + 1));
    }
}

// P7-W6 T1: CSI 8 t (714fe9b90). The arithmetic helper (R13, UD371) and the
// `handleMessage(.resize_window)` arm on a fabricated surface: the action is
// sent only when `vt-window-resize-allowed` is set, and an overflowing request
// returns without panicking.
#[cfg(test)]
mod resize_window_tests {
    use super::*;
    use crate::log_capture;
    use std::mem::MaybeUninit;
    use std::sync::Mutex;

    fn load(text: &[u8]) -> configpkg::Config {
        let mut cfg = configpkg::Config::default_config();
        let mut iter = rustty_args::args::LineIterator::init(text);
        cfg.load_iter(&mut iter).unwrap();
        cfg.finalize().unwrap();
        assert!(cfg._diagnostics.empty(), "unexpected config diagnostics");
        cfg
    }

    fn cell(width: u32, height: u32) -> rendererpkg::size::CellSize {
        rendererpkg::size::CellSize { width, height }
    }

    fn pad(p: u32) -> rendererpkg::size::Padding {
        rendererpkg::size::Padding {
            top: p,
            bottom: p,
            right: p,
            left: p,
        }
    }

    fn rw(rows: u16, columns: u16) -> terminal::stream::ResizeWindow {
        terminal::stream::ResizeWindow { rows, columns }
    }

    const SCALE: apprt::ContentScale = apprt::ContentScale { x: 1.0, y: 1.0 };

    // UD371 (R13): the reference's unchecked `u32` arithmetic traps in
    // ReleaseSafe for these inputs (measured, `.port/phase-7/probes/
    // p7w6t1_resize_window_overflow_probe.zig`); the port returns `None`.
    #[test]
    #[cfg_attr(
        miri,
        ignore = "MIRI EXCEPTION (rustty-core, parity tail C-1): `Config::finalize` calls the foreign function `getppid`, which Miri cannot execute; runs natively in cargo test --workspace (see MIRI_EXCEPTIONS.md)"
    )]
    fn ud371_resize_window_overflow_takes_the_skip_path() {
        // Positive controls: CSI 8;24;80 t on 10x20 px cells with 2 px
        // padding, the minimums, and zero (omitted) dimensions.
        assert_eq!(
            Some((80 * 10 + 4, 24 * 20 + 4)),
            resize_window_pts(rw(24, 80), cell(10, 20), pad(2), SCALE)
        );
        assert_eq!(
            Some((40 * 10 + 4, 10 * 20 + 4)),
            resize_window_pts(rw(1, 1), cell(10, 20), pad(2), SCALE),
            "requests below 40x10 cells are raised to it"
        );
        assert_eq!(
            Some((0, 30 * 20 + 4)),
            resize_window_pts(rw(30, 0), cell(10, 20), pad(2), SCALE)
        );
        assert_eq!(
            Some(((80 * 10 + 4) / 2, (24 * 20 + 4) / 2)),
            resize_window_pts(
                rw(24, 80),
                cell(10, 20),
                pad(2),
                apprt::ContentScale { x: 2.0, y: 2.0 }
            ),
            "pixels are converted to points"
        );

        // `adjust-cell-width = 65528` on a 10 px cell: 65535 * 65538 > u32.
        let cfg = load(b"adjust-cell-width = 65528\n");
        let cell_width = cfg
            .adjust_cell_width
            .expect("adjust-cell-width parsed")
            .apply_u32(10);
        assert_eq!(65_538, cell_width, "the modifier reaches the cell size");
        assert_eq!(
            None,
            resize_window_pts(rw(65535, 65535), cell(cell_width, 20), pad(2), SCALE)
        );
        // 65535 * 65537 is exactly u32::MAX: the padding sum overflows.
        assert_eq!(
            None,
            resize_window_pts(rw(24, 65535), cell(65_537, 20), pad(2), SCALE)
        );
        // No padding, the product fits, but the point size at scale 0.5 does
        // not (the reference's "integer part ... out of bounds").
        assert_eq!(
            None,
            resize_window_pts(
                rw(24, 65535),
                cell(65_536, 20),
                pad(0),
                apprt::ContentScale { x: 0.5, y: 1.0 }
            )
        );
        // A cell so wide that even the 40-column minimum overflows: a bare
        // CSI 8 t (both omitted) still succeeds, because the port does not
        // compute a dimension the program did not ask for; asking for a
        // width fails.
        let huge = load(b"adjust-cell-width = 2147483647\n")
            .adjust_cell_width
            .expect("adjust-cell-width parsed")
            .apply_u32(10);
        assert_eq!(2_147_483_657, huge);
        assert_eq!(
            Some((0, 0)),
            resize_window_pts(rw(0, 0), cell(huge, 20), pad(2), SCALE)
        );
        assert_eq!(
            Some((0, 24 * 20 + 4)),
            resize_window_pts(rw(24, 0), cell(huge, 20), pad(2), SCALE)
        );
        assert_eq!(
            None,
            resize_window_pts(rw(24, 1), cell(huge, 20), pad(2), SCALE)
        );
        // Heights overflow the same way.
        assert_eq!(
            None,
            resize_window_pts(rw(65535, 0), cell(10, 65_538), pad(2), SCALE)
        );
        // Fix round 1 (Ruling 55): padding only. `window-padding-x =
        // 2147483648` at scale 1 is 2^31 a side (fits `u32`), and the sum of
        // the two sides overflows with ordinary cells (probe case `padsum`).
        let mut derived = DerivedConfig::init(Allocator, &load(b"window-padding-x = 2147483648\n"))
            .expect("DerivedConfig::init");
        let scaled = derived.scaledPadding(72.0, 72.0);
        derived.deinit();
        drop(derived);
        assert_eq!(
            (1 << 31, 1 << 31),
            (scaled.left, scaled.right),
            "the config reaches the padding"
        );
        assert_eq!(
            None,
            resize_window_pts(rw(24, 80), cell(10, 20), scaled, SCALE)
        );
        assert_eq!(
            None,
            resize_window_pts(
                rw(24, 80),
                cell(10, 20),
                rendererpkg::size::Padding {
                    left: 1 << 31,
                    right: 1 << 31,
                    top: 0,
                    bottom: 0,
                },
                SCALE
            )
        );
        // The same padding with the width omitted: the height is unaffected.
        assert_eq!(
            Some((0, 24 * 20)),
            resize_window_pts(
                rw(24, 0),
                cell(10, 20),
                rendererpkg::size::Padding {
                    left: 1 << 31,
                    right: 1 << 31,
                    top: 0,
                    bottom: 0,
                },
                SCALE
            )
        );
    }

    struct TestApp {
        config: configpkg::Config,
        actions: Mutex<Vec<ResizeWindow>>,
    }

    impl apprt::App for TestApp {
        type Surface = TestRtSurface;

        fn perform_action<C: apprt::CoreSurfaceHandle>(
            &self,
            target: apprt::Target<'_, C>,
            action: apprt::Action<'_>,
        ) -> Result<bool, apprt::ApprtError> {
            assert!(matches!(target, apprt::Target::Surface(_)));
            match action {
                apprt::Action::ResizeWindow(v) => self.actions.lock().unwrap().push(v),
                other => panic!("unexpected action {:?}", other.key()),
            }
            Ok(true)
        }

        fn perform_ipc(
            &self,
            _target: apprt::ipc::Target<'_>,
            _action: apprt::ipc::Action<'_>,
        ) -> Result<bool, apprt::ipc::Errors> {
            unimplemented!("resize_window KAT: not driven")
        }

        fn wakeup(&self) {}

        fn config(&self) -> &configpkg::Config {
            &self.config
        }

        fn keyboard_layout(&self) -> rustty_input::keyboard::Layout {
            rustty_input::keyboard::Layout::Unknown
        }
    }

    struct TestRtSurface {
        core: *mut Surface<TestApp>,
        app: *mut TestApp,
        scale: apprt::ContentScale,
    }

    impl apprt::Surface for TestRtSurface {
        type App = TestApp;
        type Core = Surface<TestApp>;

        fn core(&self) -> *mut Surface<TestApp> {
            self.core
        }
        fn rt_app(&self) -> *mut TestApp {
            self.app
        }
        unsafe fn close(_self: *const Self, _process_alive: bool) {}
        fn deinit(&mut self) {}
        fn get_content_scale(&self) -> Result<apprt::ContentScale, apprt::ApprtError> {
            Ok(self.scale)
        }
        fn get_size(&self) -> Result<apprt::SurfaceSize, apprt::ApprtError> {
            unimplemented!("resize_window KAT: not driven")
        }
        fn get_title(&self) -> Option<&std::ffi::CStr> {
            None
        }
        fn supports_clipboard(&self, _clipboard_type: apprt::Clipboard) -> bool {
            unimplemented!("resize_window KAT: not driven")
        }
        fn clipboard_request(
            &self,
            _clipboard_type: apprt::Clipboard,
            _state: apprt::ClipboardRequest,
        ) -> Result<apprt::ClipboardReadResult, apprt::ApprtError> {
            unimplemented!("resize_window KAT: not driven")
        }
        fn set_clipboard(
            &self,
            _clipboard_type: apprt::Clipboard,
            _contents: &[apprt::ClipboardContent<'_>],
            _confirm: bool,
        ) -> Result<(), apprt::ApprtError> {
            unimplemented!("resize_window KAT: not driven")
        }
        fn get_cursor_pos(&self) -> Result<apprt::CursorPos, apprt::ApprtError> {
            unimplemented!("resize_window KAT: not driven")
        }
        fn default_termio_env(&self) -> Result<rustty_global::EnvMap, apprt::ApprtError> {
            unimplemented!("resize_window KAT: not driven")
        }
    }

    /// The `ResizeWindow` values a program's bytes parse to (the vt stream's
    /// CSI t arm; termio forwards each unchanged, see its own test).
    fn parse(bytes: &[u8]) -> Vec<terminal::stream::ResizeWindow> {
        struct H(Vec<terminal::stream::ResizeWindow>);
        impl terminal::stream::Handler for H {
            fn resize_window(&mut self, v: terminal::stream::ResizeWindow) {
                self.0.push(v);
            }
        }
        let mut s =
            terminal::stream::Stream::init(terminal::stream::Options::with_handler(H(Vec::new())));
        s.next_slice(bytes);
        std::mem::take(&mut s.handler.0)
    }

    /// Runs `bytes` through a fabricated surface (only `config`, `size`,
    /// `rt_app` and `rt_surface` written; the arm reads nothing else) and
    /// returns the recorded `resize_window` actions and log records.
    fn run(
        config_text: &[u8],
        cell: rendererpkg::size::CellSize,
        bytes: &[u8],
    ) -> (Vec<ResizeWindow>, Vec<(log::Level, String, String)>) {
        let mut full_config = configpkg::Config::default_config();
        let mut iter = rustty_args::args::LineIterator::init(config_text);
        full_config.load_iter(&mut iter).unwrap();
        assert!(full_config._diagnostics.empty());
        // No links => `DerivedConfig::init` never reaches oniguruma.
        full_config.link.links.clear();
        let derived = DerivedConfig::init(Allocator, &full_config).expect("DerivedConfig::init");

        let mut mu: Box<MaybeUninit<Surface<TestApp>>> = Box::new(MaybeUninit::uninit());
        let sp = mu.as_mut_ptr();
        let rt_app: *mut TestApp = Box::into_raw(Box::new(TestApp {
            config: full_config,
            actions: Mutex::new(Vec::new()),
        }));
        let rt_surface: *mut TestRtSurface = Box::into_raw(Box::new(TestRtSurface {
            core: sp,
            app: rt_app,
            scale: apprt::ContentScale { x: 1.0, y: 1.0 },
        }));
        unsafe {
            std::ptr::addr_of_mut!((*sp).config).write(derived);
            std::ptr::addr_of_mut!((*sp).rt_app).write(rt_app);
            std::ptr::addr_of_mut!((*sp).rt_surface).write(rt_surface);
            std::ptr::addr_of_mut!((*sp).size).write(rendererpkg::size::Size {
                screen: rendererpkg::size::ScreenSize {
                    width: 800,
                    height: 600,
                },
                cell,
                padding: pad(0),
            });
        }

        let sizes = parse(bytes);
        let cap = log_capture::start();
        for size in sizes {
            unsafe { Surface::handleMessage(sp, Message::ResizeWindow(size)) }
                .expect("handleMessage");
        }
        let messages = cap.messages();
        drop(cap);

        let actions = std::mem::take(&mut *unsafe { &*rt_app }.actions.lock().unwrap());
        unsafe {
            (*sp).config.deinit();
            std::ptr::drop_in_place(std::ptr::addr_of_mut!((*sp).config));
            drop(Box::from_raw(rt_surface));
            drop(Box::from_raw(rt_app));
        }
        (actions, messages)
    }

    #[test]
    fn csi_8_t_resizes_the_window_only_when_allowed() {
        // Disabled by default: logged, nothing performed.
        let (actions, messages) = run(b"", cell(10, 20), b"\x1b[8;24;80t");
        assert!(actions.is_empty());
        assert_eq!(
            vec![(
                log::Level::Info,
                "surface".to_string(),
                "resize_window requested, but disabled via config".to_string()
            )],
            messages
        );

        // Allowed: the grid size in points, with the configured padding
        // (`window-padding-x/y = 2` at 72 dpi × scale 1 → 2 px a side), the
        // 40x10 minimum, and a zero (omitted) dimension passed through.
        let (actions, messages) = run(
            b"vt-window-resize-allowed = true\nwindow-padding-x = 2\nwindow-padding-y = 2\n",
            cell(10, 20),
            b"\x1b[8;24;80t\x1b[8;1;1t\x1b[8;;100t\x1b[8;30;100;1t",
        );
        assert!(messages.is_empty(), "{messages:?}");
        assert_eq!(
            vec![
                ResizeWindow {
                    width: 80 * 10 + 4,
                    height: 24 * 20 + 4
                },
                ResizeWindow {
                    width: 40 * 10 + 4,
                    height: 10 * 20 + 4
                },
                ResizeWindow {
                    width: 100 * 10 + 4,
                    height: 0
                },
            ],
            actions
        );
    }

    // UD371 through the arm: the checked path returns without panicking and
    // sends nothing.
    #[test]
    fn ud371_csi_8_t_overflow_returns_without_panicking() {
        let (actions, messages) = run(
            b"vt-window-resize-allowed = true\n",
            cell(65_538, 20),
            b"\x1b[8;65535;65535t",
        );
        assert!(actions.is_empty());
        assert_eq!(
            vec![(
                log::Level::Warn,
                "surface".to_string(),
                "resize_window requested, but the size does not fit a u32, ignoring".to_string()
            )],
            messages
        );
    }
}

// PORT NOTE (rustty-only, CORE_RENDERER_CONFIG_INTEGRATION_DESIGN §4): these
// tests have NO Zig `test{}` counterpart — they exercise the core-side
// consequence of the non-windowing renderer `DerivedConfig` swap. Core deps
// BOTH `rustty-config` (for `Config`) AND `rustty-renderer`, and binds the
// concrete `C = rustty_config::Config`, so it is the FIRST place anywhere the
// real `impl RendererConfig for Config` (rustty-config's `renderer_config.rs`)
// executes: the renderer crate can only test `init<C>` with its `MockConfig`
// (it cannot dep `rustty-config` — that is the G7 cycle, R4). Both are headless
// (no Metal, no NSView, no thread); `Surface::init` itself stays untestable
// (it panics earlier at the `Renderer::surfaceInit` windowing seam, §3).
#[cfg(test)]
mod renderer_config_tests {
    use super::*;
    // The renderer-defined accessor trait (`config.<field>()`); the concrete
    // `impl RendererConfig for Config` lives in rustty-config.
    use rendererpkg::config::RendererConfig;

    // T1 — a real `DerivedConfig` derived from a real `Config`.
    #[test]
    // Miri cannot execute the oniguruma C FFI this test deliberately reaches
    // (`extern static OnigEncodingUTF8`, via `link::Set::from_config` regex
    // compilation): "unsupported operation", not a soundness verdict. Covered
    // natively by `cargo test --workspace` (CI HARD step 4) and by the
    // rustty-config/rustty-oniguruma ASan G6 lanes. See MIRI_EXCEPTIONS.md
    // ("rustty-core renderer_config_tests").
    #[cfg_attr(
        miri,
        ignore = "real Config compiles link regexes through oniguruma C FFI (extern static OnigEncodingUTF8) — un-executable under Miri; ASan + native lanes cover it"
    )]
    fn renderer_derived_config_from_real_config() {
        // OnceLock C-init guard (memory `parallel-cargo-test-exposes-c-init-races`):
        // `init<C>` reaches `link::Set::from_config`, which compiles the default
        // config's link regex through the non-thread-safe oniguruma C FFI.
        oni::testing::ensure_init().unwrap();
        let cfg = configpkg::Config::default_config();
        let d = rendererpkg::DerivedConfig::init(Allocator, &cfg).expect("init");
        // Assert the extracted fields track the `Config` defaults, proving the
        // config-side `impl RendererConfig for Config` accessors + `init<C>`'s
        // clamps/casts:
        assert_eq!(d.font_thicken, cfg.font_thicken()); // bool passthrough
        assert!((0.0..=1.0).contains(&d.background_opacity)); // init clamps [0,1]
        assert_eq!(d.faint_opacity, (cfg.faint_opacity() * 255.0).ceil() as u8);
        // frees `links`' onig handles (no `Drop`) — no leak.
        let mut d = d;
        d.deinit();
    }

    // T2 — a real `Message::ChangeConfig` end-to-end (no `Surface` needed).
    #[test]
    // Same Miri FFI limit as T1 above (oniguruma extern static via the real
    // `Config`); same native + ASan coverage. See MIRI_EXCEPTIONS.md.
    #[cfg_attr(
        miri,
        ignore = "real Config compiles link regexes through oniguruma C FFI (extern static OnigEncodingUTF8) — un-executable under Miri; ASan + native lanes cover it"
    )]
    fn init_change_config_from_real_config() {
        // OnceLock C-init guard (see T1).
        oni::testing::ensure_init().unwrap();
        let cfg = configpkg::Config::default_config();
        let msg =
            rendererpkg::Message::initChangeConfig(Allocator, &cfg).expect("initChangeConfig");
        match msg {
            // both `DerivedConfig`s (renderer + thread) built.
            rendererpkg::Message::ChangeConfig { .. } => {}
            _ => panic!("expected ChangeConfig"),
        }
        // frees `impl.links` onig handles then drops both boxes.
        msg.deinit();
    }
}

// rustty-only tests: lock the `isize_from_float` cast helper's contract.
//
// Task R13-P (`UPSTREAM-DEFECTS.md` UD62) REVERSED the two tests that used to
// live here, which pinned the reference's trap (a `debug_assert!` copy of
// `@intFromFloat`'s safety check). The trap is reachable from a `keybind` value
// and from a C-ABI embedder's scroll offset, so under R13 the port does not
// reproduce it.
#[cfg(test)]
mod isize_from_float_tests {
    use super::{isize_from_float, isize_from_float_saturating};

    #[test]
    fn isize_from_float_in_range_is_exact() {
        assert_eq!(isize_from_float(3.0), Some(3));
        assert_eq!(isize_from_float(-2.0), Some(-2));
        // `@intFromFloat` converts the integer part (truncation toward zero).
        assert_eq!(isize_from_float(0.9), Some(0));
        assert_eq!(isize_from_float(-1.9), Some(-1));
    }

    /// The values the reference traps on (measured in Debug and ReleaseSafe:
    /// infinities and finite values outside `isize`) have no `isize`, and the
    /// gesture call sites scroll nothing for them instead of crashing or
    /// looping to `isize::MAX` (`scroll_gesture_cast_tests` drives those).
    #[test]
    fn isize_from_float_reports_the_values_the_reference_traps_on() {
        assert_eq!(isize_from_float(f64::INFINITY), None);
        assert_eq!(isize_from_float(f64::NEG_INFINITY), None);
        assert_eq!(isize_from_float(1e300), None);
        assert_eq!(isize_from_float(-1e300), None);
        // The boundary: 2^63 has no `isize`, the largest f64 below it does.
        assert_eq!(isize_from_float(-(isize::MIN as f64)), None);
        assert_eq!(isize_from_float(isize::MIN as f64), Some(isize::MIN));
    }

    /// NaN is NOT a value the reference traps on: the safety check lets it
    /// through, and the result is target-dependent — `0` on arm64 and
    /// `isize::MIN` on x86_64, in every build mode (measured, task R13-P2).
    /// It has no `isize` either, so it is reported like the trapping values and
    /// each caller's arm decides what it means (task R13-P2 re-checked them:
    /// see `isize_from_float`).
    #[test]
    fn isize_from_float_reports_nan_whose_reference_value_is_target_dependent() {
        assert_eq!(isize_from_float(f64::NAN), None);
        assert_eq!(isize_from_float(-f64::NAN), None);
    }

    /// `scroll_page_fractional`'s arm: `keybind = …=scroll_page_fractional:1e30`
    /// means "scroll as far as it goes", and the viewport clamps it (the
    /// companion test below drives the clamp).
    #[test]
    fn isize_from_float_saturating_takes_the_end_in_each_direction() {
        assert_eq!(isize_from_float_saturating(12.0), 12);
        assert_eq!(isize_from_float_saturating(1e300), isize::MAX);
        assert_eq!(isize_from_float_saturating(f64::INFINITY), isize::MAX);
        assert_eq!(isize_from_float_saturating(-1e300), isize::MIN);
        assert_eq!(isize_from_float_saturating(f64::NEG_INFINITY), isize::MIN);
        // A NaN fraction has no direction, so it scrolls nothing.
        assert_eq!(isize_from_float_saturating(f64::NAN), 0);
    }
}

/// The other half of `scroll_page_fractional`'s handling: the saturated delta
/// this surface queues is safe at its destination. `scroll_viewport` takes the
/// magnitude with `unsigned_abs` (so `isize::MIN` is exact) and clamps to the
/// top or the active area, without a panic and without walking one row at a
/// time.
#[cfg(test)]
mod scroll_page_fractional_saturation_tests {
    use super::*;

    #[test]
    fn a_saturated_scroll_delta_is_clamped_by_the_viewport() {
        let mut term = terminal::terminal::Terminal::init(
            Allocator.into(),
            terminal::terminal::Options {
                cols: 10,
                rows: 4,
                max_scrollback_bytes: Some(10_000),
                max_scrollback_lines: None,
                colors: rustty_vt::terminal::Colors::default(),
                default_modes: rustty_vt::modes::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                default_xt_checksum: Default::default(),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .expect("Terminal::init");

        // Some scrollback to scroll through.
        for _ in 0..20 {
            term.print(u32::from('x')).unwrap();
            term.index().unwrap();
        }

        // `scroll_page_fractional:-1e30` -> isize::MIN; `:1e30` -> isize::MAX.
        term.scroll_viewport(terminal::terminal::ScrollViewport::Delta(isize::MIN));
        term.scroll_viewport(terminal::terminal::ScrollViewport::Delta(isize::MAX));
        term.scroll_viewport(terminal::terminal::ScrollViewport::Delta(0));

        term.deinit(Allocator.into());
    }
}

// rustty-only (task R13-P2, UPSTREAM-DEFECTS.md UD62): the three gesture float
// casts in `scrollCallback` — `Surface.zig:3520` (y), `:3529` (x, discrete) and
// `:3543` (x, precision). `Surface::init` cannot run without an apprt, so these
// drive `scroll_amount_y` / `scroll_amount_x`, the callback's own y and x blocks,
// with the mods decoded by `ScrollMods::from_int` as the C-ABI entry
// `rustty_surface_mouse_scroll` decodes them. Each test pins ONE cast: a value
// the reference traps on must take the no-scroll arm (a saturating cast would
// return `isize::MAX`/`MIN` here, which the callback's per-row loops would then
// walk), the pending offset must be left as it was, and an in-range offset must
// still scroll by the exact amount.
#[cfg(test)]
mod scroll_gesture_cast_tests {
    use super::*;

    const CELL: rendererpkg::size::CellSize = rendererpkg::size::CellSize {
        width: 10,
        height: 20,
    };

    /// `ScrollMods` as the C-ABI entry decodes the host's `int` (bit 0 is
    /// `precision`).
    fn mods(precision: bool) -> input_mouse::ScrollMods {
        input_mouse::ScrollMods::from_int(u8::from(precision))
    }

    /// Offsets the reference traps on at the cast (measured: 1e300 and inf).
    const TRAPPING: [f64; 4] = [1e300, -1e300, f64::INFINITY, f64::NEG_INFINITY];

    fn none() -> ScrollAmount {
        ScrollAmount::default()
    }

    #[test]
    fn y_gesture_cast_out_of_range_scrolls_nothing() {
        let multiplier = configpkg::value_types::MouseScrollMultiplier::default();
        let mut mouse = Mouse::default();

        // Positive control: 45 px of precision scroll over 20 px cells.
        assert_eq!(
            ScrollAmount { delta: 2 },
            scroll_amount_y(&mut mouse, CELL, multiplier, 45.0, mods(true))
        );
        assert_eq!(
            ScrollAmount { delta: -2 },
            scroll_amount_y(&mut mouse, CELL, multiplier, -45.0, mods(true))
        );

        for precision in [true, false] {
            for v in TRAPPING {
                mouse.pending_scroll_y = 5.0;
                assert_eq!(
                    none(),
                    scroll_amount_y(&mut mouse, CELL, multiplier, v, mods(precision)),
                    "yoff {v} (precision {precision}) scrolls nothing"
                );
                assert_eq!(
                    5.0, mouse.pending_scroll_y,
                    "yoff {v} (precision {precision}) leaves the pending offset alone"
                );
            }
        }

        // NaN (task R13-C): on the precision path a NaN offset makes `amount`
        // NaN, which the reference's `assert(@abs(amount) >= 1)` rejects; the
        // guard now runs ahead of that assert, so a NaN from the C ABI scrolls
        // nothing instead of failing the `debug_assert!` (an abort inside the
        // `extern "C"` entry point in debug and `release-safe` builds). The
        // discrete path is not driven with NaN: on Darwin `@min(nan, -1)` is
        // -1 in both trees (measured, Zig 0.16.0), so it is an ordinary tick.
        for v in [f64::NAN, -f64::NAN] {
            mouse.pending_scroll_y = 5.0;
            assert_eq!(
                none(),
                scroll_amount_y(&mut mouse, CELL, multiplier, v, mods(true)),
                "precision yoff NaN scrolls nothing"
            );
            assert!(
                mouse.pending_scroll_y == 5.0,
                "precision yoff NaN leaves the pending offset alone, got {}",
                mouse.pending_scroll_y
            );
        }

        // Not poisoned: the next ordinary scroll still adds to the kept 5 px.
        assert_eq!(
            ScrollAmount { delta: 2 },
            scroll_amount_y(&mut mouse, CELL, multiplier, 40.0, mods(true))
        );
    }

    #[test]
    fn x_discrete_gesture_cast_out_of_range_scrolls_nothing() {
        let mut mouse = Mouse::default();

        // Positive control: a discrete x offset is rounded, not accumulated.
        assert_eq!(
            ScrollAmount { delta: 3 },
            scroll_amount_x(&mut mouse, CELL, 2.6, mods(false))
        );
        assert_eq!(
            ScrollAmount { delta: -2 },
            scroll_amount_x(&mut mouse, CELL, -2.4, mods(false))
        );

        // 9.3e18 is just past `isize::MAX`. NaN does not trap in the reference
        // (0 on arm64, `isize::MIN` on x86_64, measured) and has no assert on
        // this path, so it is driven here too.
        for v in TRAPPING.into_iter().chain([9.3e18, f64::NAN]) {
            mouse.pending_scroll_x = 3.0;
            assert_eq!(
                none(),
                scroll_amount_x(&mut mouse, CELL, v, mods(false)),
                "discrete xoff {v} scrolls nothing"
            );
            assert_eq!(3.0, mouse.pending_scroll_x);
        }
    }

    #[test]
    fn x_precision_gesture_cast_out_of_range_scrolls_nothing() {
        // Positive control: 3 px pending + 25 px over 10 px cells.
        let mut mouse = Mouse {
            pending_scroll_x: 3.0,
            ..Mouse::default()
        };
        assert_eq!(
            ScrollAmount { delta: 2 },
            scroll_amount_x(&mut mouse, CELL, 25.0, mods(true))
        );

        // NaN (task R13-C): as on the y precision path, the guard runs ahead
        // of the `amount` assert a NaN would fail.
        for v in TRAPPING.into_iter().chain([f64::NAN, -f64::NAN]) {
            mouse.pending_scroll_x = 3.0;
            assert_eq!(
                none(),
                scroll_amount_x(&mut mouse, CELL, v, mods(true)),
                "precision xoff {v} scrolls nothing"
            );
            assert_eq!(
                3.0, mouse.pending_scroll_x,
                "precision xoff {v} leaves the pending offset alone"
            );
        }

        // Not poisoned.
        assert_eq!(
            ScrollAmount { delta: 2 },
            scroll_amount_x(&mut mouse, CELL, 25.0, mods(true))
        );
    }
}

// Task 6 inspector-lifecycle KAT. A full Surface-level activate->deactivate
// cannot be exercised in a unit test — `Surface::init` spawns the render/io
// threads and needs a live apprt `App` + pty — so this KAT proves the exact
// cross-crate seams `activateInspector` / `deactivateInspector` / the keyCallback
// inspector defer wire up, which ARE constructible without a Surface. The Surface
// control flow itself (the mutex/mailbox/queueIo sequence) is covered by the
// workspace build gate + the app shakedown (plan Task 18).
#[cfg(test)]
mod inspector_lifecycle_tests {
    use super::*;

    // The R2 acyclic overlay coercion `activateInspector` performs: an inspector's
    // `renderer_info` sub-object unsize-coerces to the renderer-owned
    // `*mut dyn InspectorOverlay` fat pointer, through which the renderer reads
    // overlay features (INV-7). Proves the type wiring compiles AND the call
    // crosses the seam end-to-end (a fresh inspector enables no overlays).
    #[test]
    fn inspector_renderer_info_coerces_to_overlay_and_reads() {
        let mut insp = inspectorpkg::Inspector::init(Allocator);

        // Exactly the coercion `activateInspector` stores onto renderer_state.inspector.
        let info_ref: &mut dyn rendererpkg::state::InspectorOverlay = &mut insp.gui.renderer_info;
        let info_ptr: *mut rendererpkg::state::Inspector = info_ref;

        // The renderer reads overlay features through the fat pointer.
        let features = unsafe { (*info_ptr).overlay_features() };
        assert!(features.is_empty());

        insp.deinit(Allocator);
    }

    // The keyCallback inspector defer records key events via `recordKeyEvent` with
    // the max_capacity=50 grow-then-evict policy (INV-9). Filling well past capacity
    // must never surface an error (the ring self-evicts) and never panic — driving
    // both the `resize` grow branch and the `delete_oldest` + re-append evict branch.
    #[test]
    fn record_key_event_grows_then_evicts_bounded() {
        let mut insp = inspectorpkg::Inspector::init(Allocator);

        for _ in 0..200 {
            insp.recordKeyEvent(Allocator, inspectorpkg::KeyEvent::default())
                .expect("recordKeyEvent self-evicts at capacity, never surfaces OutOfMemory");
        }

        insp.deinit(Allocator);
    }
}

// `freetype-load-flags` DI-bridge KAT (`fix-freetype-tmux` lane).
//
// The bug this pins: `font_shared_grid_set_derived_config` used to assign
// `face::FREETYPE_LOAD_FLAGS_DEFAULT` unconditionally, so a user's configured
// `freetype-load-flags` was parsed, accepted, and then silently discarded on
// every freetype-active build. The reference assigns
// `if (FreetypeLoadFlags != void) config.@"freetype-load-flags" else {}`
// (`SharedGridSet.zig:528`).
#[cfg(test)]
mod freetype_load_flags_tests {
    use super::*;

    /// A value where EVERY field is the opposite of
    /// `config::FreetypeLoadFlags::default()`, so any test below fails if the
    /// implementation substitutes the default for the configured value.
    fn all_flipped() -> configpkg::value_types::FreetypeLoadFlags {
        let d = configpkg::value_types::FreetypeLoadFlags::default();
        let flipped = configpkg::value_types::FreetypeLoadFlags {
            hinting: !d.hinting,
            force_autohint: !d.force_autohint,
            monochrome: !d.monochrome,
            autohint: !d.autohint,
            light: !d.light,
        };
        assert_ne!(flipped, d, "the fixture must be non-default");
        flipped
    }

    // The DI bridge threads all five fields; a hard-coded default fails here.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn from_config_threads_every_field() {
        let cfg = all_flipped();
        let got = freetype_load_flags_from_config(&cfg);

        assert_eq!(got.hinting, cfg.hinting);
        assert_eq!(got.force_autohint, cfg.force_autohint);
        assert_eq!(got.monochrome, cfg.monochrome);
        assert_eq!(got.autohint, cfg.autohint);
        assert_eq!(got.light, cfg.light);

        // And the whole value must differ from the shaper-tier default — the
        // exact discrimination the original bug lacked.
        assert_ne!(
            got,
            rustty_font_shaper::face::freetype::FreetypeLoadFlags::DEFAULT,
            "a non-default config must NOT map onto the default load flags"
        );
    }

    // The two tiers' stand-ins for the reference's single
    // `config.FreetypeLoadFlags` must stay field-identical: if a field is added
    // to one and not the other this stops compiling / stops round-tripping.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn default_round_trips_exactly() {
        let d = configpkg::value_types::FreetypeLoadFlags::default();
        assert_eq!(
            freetype_load_flags_from_config(&d),
            rustty_font_shaper::face::freetype::FreetypeLoadFlags::DEFAULT,
        );
    }

    // `SharedGridSet.Key.hash` feeds the load flags to the hasher
    // (`autoHash(hasher, self.freetype_load_flags)`); the port used to skip that
    // field outright, so two grids differing ONLY in `freetype-load-flags` hashed
    // identically and the second config silently reused the first's cached grid.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn load_flags_are_hash_distinguishing() {
        use core::hash::{Hash, Hasher};

        fn h(v: rustty_font_shaper::face::freetype::FreetypeLoadFlags) -> u64 {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            v.hash(&mut hasher);
            hasher.finish()
        }

        let d = rustty_font_shaper::face::freetype::FreetypeLoadFlags::DEFAULT;
        assert_ne!(
            h(d),
            h(freetype_load_flags_from_config(&all_flipped())),
            "two distinct load-flag configs must not collide on the same grid key"
        );
    }

    // The reference's `else {}` (void) arm: on the backends without FreeType
    // (the macOS default `coretext`, `coretext_harfbuzz`, `coretext_noshape`)
    // `hasFreetype()` is false, so the config value IS legitimately dropped and
    // the shaper type is `()`. There is nothing runtime to assert (a `#[test]`
    // comparing two unit values always succeeds — a worthless test), so the arm
    // is pinned at COMPILE time instead: the selected type must be zero-sized.
    // (S-14, N266: keyed on `hasFreetype()` via `build.rs`, not `target_os`.)
    #[cfg(all(target_os = "macos", not(rustty_font_has_freetype)))]
    const _: () = assert!(size_of::<rustty_font_shaper::face::FreetypeLoadFlags>() == 0);

    // THE ORIGINAL BUG, at the exact site that had it. Only compiles/runs on the
    // freetype-active backends (Linux `fontconfig_freetype`, and on macOS
    // `-Dfont-backend=coretext_freetype`/`freetype`), because elsewhere the
    // reference itself takes the `void` arm and there is nothing to thread.
    #[test]
    #[cfg(all(rustty_font_has_freetype, not(target_arch = "wasm32")))]
    fn derived_config_carries_the_configured_flags() {
        let config = configpkg::Config {
            freetype_load_flags: all_flipped(),
            ..Default::default()
        };

        let derived = font_shared_grid_set_derived_config(&config);
        assert_eq!(
            derived.freetype_load_flags,
            freetype_load_flags_from_config(&config.freetype_load_flags),
        );
        assert_ne!(
            derived.freetype_load_flags,
            rustty_font_shaper::face::FREETYPE_LOAD_FLAGS_DEFAULT,
            "regression: the configured freetype-load-flags were replaced by the default"
        );
    }
}

// test "queueIo frees allocated writes in readonly mode" {
//
// (`957ed21d5`; the readonly discard arm's free is `msg.deinit()` at END
// STATE (`2f7fbadb0`) == the Rust drop — see the arm's PORT NOTE in
// `queueIo`.)
#[cfg(test)]
#[allow(non_snake_case)]
mod queue_io_readonly_tests {
    use super::*;
    use crate::app::test_runtime::TestApp;

    #[test]
    fn queueIo_frees_allocated_writes_in_readonly_mode() {
        // const testing = std.testing;
        //
        // const surface = try testing.allocator.create(Surface);
        // defer testing.allocator.destroy(surface);
        // surface.readonly = true;
        //
        // PORT NOTE: `testing.allocator.create(Surface)` returns UNINITIALIZED
        // memory and the test writes exactly ONE field — the established
        // `fabricateSurface`/`surface_visibility_kat` MaybeUninit fabrication
        // (field-projected raw-pointer writes; the untouched fields are never
        // read — the readonly discard arm returns before `self.io` — and
        // `MaybeUninit` never drops, which is `destroy`-without-`deinit`).
        let mut surface: Box<std::mem::MaybeUninit<Surface<TestApp>>> =
            Box::new(std::mem::MaybeUninit::uninit());
        let ptr = surface.as_mut_ptr();
        unsafe { std::ptr::addr_of_mut!((*ptr).readonly).write(true) };

        // queueIo must free allocated writes in read-only mode.
        // const data = try testing.allocator.dupe(u8, "\x1b]lGhostty\x1b\\");
        // surface.queueIo(.{ .write_alloc = .{
        //     .alloc = testing.allocator,
        //     .data = data,
        // } }, .unlocked);
        //
        // (R2: the test payload's product name; the free assertion is the
        // allocator leak check — Zig's `testing.allocator` there, the Miri
        // leak gate here. A leak would also re-appear as the `Box<[u8]>` not
        // dropping under any leak-checking run of this lane.)
        let data: Box<[u8]> = b"\x1b]lRustty\x1b\\".to_vec().into_boxed_slice();
        unsafe { &mut *ptr }.queueIo(
            termio::Message::WriteAlloc(Alloc { data }),
            termio::termio_impl::MutexState::Unlocked,
        );
    }
}

// S151-F1 production-path regression: `Surface::copySelectionToClipboards` is
// the ONLY place the `clipboard-codepoint-map` config option reaches a
// formatter (built with `codepoint_map: Some(config.clipboard_codepoint_map
// .map.list.clone())` a few lines above). The bug lived entirely inside
// `rustty_vt::formatter`'s internal `copy_options` helper, which every
// delegation hop (`ScreenFormatter` -> `PageListFormatter` -> `PageFormatter`)
// used and which hard-coded `codepoint_map: Some(Vec::new())` — so the config
// option compiled, parsed, and did nothing. `formatter.rs`'s own unit test
// (`codepoint_map_survives_the_screen_formatter_delegation`) proves the vt
// crate's delegation chain in isolation; this test proves the actual
// production call: a real `Config` with `clipboard-codepoint-map` set, run
// through the real `copySelectionToClipboards`, landing in the runtime's
// `setClipboard`.
//
// `Surface::init` itself is untestable (it spawns a live pty/renderer/io
// thread — see the `renderer_config_tests` module doc above and
// `surface_visibility_kat.rs`'s V21 note), so this follows that file's
// established fabrication recipe: a heap-pinned `Surface<TestApp>` in
// `MaybeUninit` with ONLY the fields `copySelectionToClipboards` reads
// written (`io.terminal`, `config`, `rt_surface`) — sound because the method
// never touches any other field, and `MaybeUninit` never drops fields that
// were never initialized. The three fields that WERE written (unlike, say,
// `surface_visibility_kat.rs`'s `bool`/pointer fields) own heap memory —
// `Terminal` and `DerivedConfig` both need their own explicit `deinit`, and
// `rt_surface` its own `Box::from_raw` + drop — so the test's teardown frees
// all three explicitly rather than relying on any implicit drop.
#[cfg(test)]
mod clipboard_codepoint_map_e2e {
    use super::*;
    use std::mem::MaybeUninit;
    use std::sync::Mutex;

    struct TestApp {
        config: configpkg::Config,
    }

    impl apprt::App for TestApp {
        type Surface = TestRtSurface;

        fn perform_action<C: apprt::CoreSurfaceHandle>(
            &self,
            _target: apprt::Target<'_, C>,
            _action: apprt::Action<'_>,
        ) -> Result<bool, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }

        fn perform_ipc(
            &self,
            _target: apprt::ipc::Target<'_>,
            _action: apprt::ipc::Action<'_>,
        ) -> Result<bool, apprt::ipc::Errors> {
            unimplemented!("S151-F1 KAT: not driven")
        }

        fn wakeup(&self) {}

        fn config(&self) -> &configpkg::Config {
            &self.config
        }

        fn keyboard_layout(&self) -> rustty_input::keyboard::Layout {
            rustty_input::keyboard::Layout::Unknown
        }
    }

    /// Captures every `setClipboard` call so the test can assert on the
    /// bytes the production formatter actually produced.
    struct TestRtSurface {
        core: *mut Surface<TestApp>,
        app: *mut TestApp,
        captured: Mutex<Vec<(Vec<u8>, Vec<u8>)>>,
    }

    impl apprt::Surface for TestRtSurface {
        type App = TestApp;
        type Core = Surface<TestApp>;

        fn core(&self) -> *mut Surface<TestApp> {
            self.core
        }
        fn rt_app(&self) -> *mut TestApp {
            self.app
        }
        unsafe fn close(_self: *const Self, _process_alive: bool) {}
        fn deinit(&mut self) {}
        fn get_content_scale(&self) -> Result<apprt::ContentScale, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }
        fn get_size(&self) -> Result<apprt::SurfaceSize, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }
        fn get_title(&self) -> Option<&std::ffi::CStr> {
            None
        }
        fn supports_clipboard(&self, _clipboard_type: apprt::Clipboard) -> bool {
            unimplemented!("S151-F1 KAT: not driven")
        }
        fn clipboard_request(
            &self,
            _clipboard_type: apprt::Clipboard,
            _state: apprt::ClipboardRequest,
        ) -> Result<apprt::ClipboardReadResult, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }
        fn set_clipboard(
            &self,
            _clipboard_type: apprt::Clipboard,
            contents: &[apprt::ClipboardContent<'_>],
            _confirm: bool,
        ) -> Result<(), apprt::ApprtError> {
            let mut captured = self.captured.lock().unwrap();
            for c in contents {
                captured.push((c.mime.to_vec(), c.data.to_vec()));
            }
            Ok(())
        }
        fn get_cursor_pos(&self) -> Result<apprt::CursorPos, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }
        fn default_termio_env(&self) -> Result<rustty_global::EnvMap, apprt::ApprtError> {
            unimplemented!("S151-F1 KAT: not driven")
        }
    }

    #[test]
    // Same Miri FFI limit as `renderer_config_tests` above (`DerivedConfig::
    // init` compiles the default `Config`'s link regex through the
    // non-thread-safe oniguruma C FFI, `extern static OnigEncodingUTF8`, which
    // Miri hard-errors on): "unsupported operation", not a soundness verdict.
    // Covered natively by `cargo test --workspace` (CI HARD step 4) and by
    // the rustty-config/rustty-oniguruma ASan G6 lanes. The raw-pointer
    // fixture logic this test exists to verify (the field-projected
    // `MaybeUninit<Surface>` writes and the post-copySelectionToClipboards
    // teardown ordering) has NO oniguruma dependency of its own — proven by
    // `clipboard_codepoint_map_fixture_is_miri_clean` below, which runs the
    // identical fixture and IS Miri-covered; see MIRI_EXCEPTIONS.md
    // ("rustty-core clipboard_codepoint_map_e2e").
    #[cfg_attr(
        miri,
        ignore = "DerivedConfig::init compiles the default config's link regex through oniguruma C FFI (extern static OnigEncodingUTF8) — un-executable under Miri; ASan + native lanes cover it; see clipboard_codepoint_map_fixture_is_miri_clean for the Miri-covered variant"
    )]
    fn clipboard_codepoint_map_reaches_copy_selection_to_clipboards() {
        run(configpkg::Config::default_config());
    }

    // Miri-covered twin of the test above: identical fixture and assertions,
    // but built on a `Config` with `link.links` cleared so `DerivedConfig::
    // init` never reaches oniguruma — the ONLY difference from the real
    // `Surface::init` path, and irrelevant to what this test verifies (the
    // codepoint-map plumbing and the raw-pointer fixture's Stacked-Borrows
    // soundness, neither of which touches links). This is what actually
    // exercises the fixture's `unsafe` under Miri, per S151-F1 review.
    #[test]
    fn clipboard_codepoint_map_fixture_is_miri_clean() {
        let mut config = configpkg::Config::default_config();
        config.link.links.clear();
        run(config);
    }

    fn run(mut full_config: configpkg::Config) {
        // `clipboard-codepoint-map` set through its real CLI parser exactly
        // the way the user's config file would (map 'a', 0x61, to 'Z', 0x5A).
        full_config
            .clipboard_codepoint_map
            .parse_cli(Some(b"U+0061=U+005A"))
            .unwrap();

        // The Surface-level `DerivedConfig` clones `clipboard_codepoint_map`
        // out of the real `Config` exactly as `Surface::init` does.
        let derived_config =
            DerivedConfig::init(Allocator, &full_config).expect("DerivedConfig::init");

        // A terminal with "ab" printed to it (same fixture shape as
        // `surface_visibility_kat.rs`'s `TestSurface::build`).
        let mut terminal = rustty_vt::terminal::Terminal::init(
            Allocator.into(),
            rustty_vt::terminal::Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                colors: rustty_vt::terminal::Colors::default(),
                default_modes: rustty_vt::modes::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                default_xt_checksum: Default::default(),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .expect("Terminal::init");
        {
            let mut s = terminal.vt_stream();
            s.next_slice(b"ab");
        }

        let mut mu: Box<MaybeUninit<Surface<TestApp>>> = Box::new(MaybeUninit::uninit());
        let sp = mu.as_mut_ptr();

        let mut rt_app = Box::new(TestApp {
            config: full_config,
        });
        let rt_surface = Box::new(TestRtSurface {
            core: sp,
            app: &raw mut *rt_app,
            captured: Mutex::new(Vec::new()),
        });

        // Field-projected raw-pointer writes: only the three fields
        // `copySelectionToClipboards` reads (`io.terminal`, `config`,
        // `rt_surface`) — see the module doc above.
        //
        // (N444: `io` is out of line; its box is written first, then the
        // terminal into the block by place.)
        unsafe {
            std::ptr::addr_of_mut!((*sp).io).write(ThreadSharedBox::new_uninit());
            std::ptr::addr_of_mut!((*(*sp).io.as_ptr()).terminal).write(terminal);
            std::ptr::addr_of_mut!((*sp).config).write(derived_config);
            std::ptr::addr_of_mut!((*sp).rt_surface).write(Box::into_raw(rt_surface));
        }

        let surface = unsafe { &mut *sp };
        let pages = unsafe { &(*(*surface.io.as_ptr()).terminal.screens.active).pages };
        let sel = terminal::selection::Selection::init(
            pages.get_top_left(terminal::point::Tag::Screen),
            pages
                .get_bottom_right(terminal::point::Tag::Screen)
                .unwrap(),
            false,
        );

        surface
            .copySelectionToClipboards(
                sel,
                &[apprt::Clipboard::Standard],
                binding::CopyToClipboard::Plain,
            )
            .expect("copySelectionToClipboards");

        // Capture the `rt_surface` pointer into a LOCAL now, before any
        // further write through `sp` — under Stacked Borrows, a write
        // through the parent raw pointer `sp` (the `io.terminal.deinit` /
        // `(*sp).config` writes in teardown below) pops the tag `surface`
        // (`&mut *sp`) was derived with, so reading `surface.rt_surface`
        // AFTER such a write is a use of an invalidated reference. Reading
        // the field now, while `surface`'s tag is still live, and using only
        // this raw-pointer local afterward avoids that.
        let rt_surface_ptr: *mut TestRtSurface = surface.rt_surface;

        let rt_surface_ref: &TestRtSurface = unsafe { &*rt_surface_ptr };
        let captured = rt_surface_ref.captured.lock().unwrap();
        assert_eq!(
            1,
            captured.len(),
            "expected exactly one text/plain clipboard write"
        );
        assert_eq!(b"text/plain".as_slice(), captured[0].0.as_slice());
        let text = String::from_utf8_lossy(&captured[0].1);
        assert_eq!(
            text.trim_end_matches('\0').trim_end(),
            "Zb",
            "clipboard-codepoint-map did not reach copySelectionToClipboards's output \
             (S151-F1 regression: the config option is silently inert)"
        );
        // MUST drop before freeing `rt_surface` below — it borrows through
        // `rt_surface_ref`, which points into the boxed `TestRtSurface` we
        // are about to `Box::from_raw` + drop.
        drop(captured);

        // Teardown: free everything we allocated above via raw-pointer
        // writes. `io.terminal` uses the manual `deinit` convention, not
        // `Drop` (matches `surface_visibility_kat.rs`'s `TestSurface::drop`).
        // `config` is a real `DerivedConfig`: its `.deinit()` frees ONLY the
        // link regexes' oniguruma C handles (`Regex` has no `Drop` of its
        // own — see `DerivedConfig::deinit` above, "the owned fields free on
        // drop"); the REST of its fields (`keybind`, `selection_word_chars`,
        // `title`, `font`, ...) are plain owned `Vec`/`String`/`Option` data
        // that only free through a normal Rust `Drop`, which `MaybeUninit`
        // never runs for an untouched-by-Drop field. So `.deinit()` alone
        // leaks them (caught by `cargo miri test ... --
        // clipboard_codepoint_map_fixture_is_miri_clean`, which reports a
        // leaked `Vec<u32>` and two leaked `Vec<u8>`s originating exactly at
        // `DerivedConfig::init`'s `keybind`/`selection_word_chars` clones);
        // `drop_in_place` after `.deinit()` runs that Drop safely (the FFI
        // `Regex.value` raw pointer was already freed and has no Drop impl
        // of its own, so this can't double-free it). Finally the raw-ptr
        // `rt_surface` Box and `rt_app`. `mu` itself never drops
        // (`MaybeUninit`; every field besides the three written above was
        // never initialized). All writes/reads below go through `sp` (the
        // raw pointer), never through `surface`, for the Stacked-Borrows
        // reason noted above.
        unsafe {
            (*(*sp).io.as_ptr()).terminal.deinit(Allocator.into());
            // (N444: then the io block's own allocation.)
            (*sp).io.dealloc();
            (*sp).config.deinit();
            std::ptr::drop_in_place(std::ptr::addr_of_mut!((*sp).config));
            drop(Box::from_raw(rt_surface_ptr));
        }
        drop(rt_app);
        let _ = &mu;
    }
}

// P7-W4 T1 (Review Focus 5): the modifyOtherKeys-2 legacy encoder series
// (upstream `4406cea3e`, `636a2f35b`, `587e08f3f`) end to end through the
// Surface key path. MOK2 is enabled the way a program such as vim does it,
// by writing `CSI > 4 ; 2 m` to the terminal, and each key is then encoded by
// `Surface::encodeKey` -> `encodeKeyOpts` (`Options::from_terminal`) ->
// `key_encode::encode`, the bytes `keyCallback` queues to the pty.
//
// `Surface::init` is untestable here (it spawns a live pty/renderer/io
// thread; see `clipboard_codepoint_map_e2e` above), so this uses the same
// fabrication recipe: a heap-pinned `Surface<TestApp>` in `MaybeUninit` with
// ONLY the fields `encodeKey` reads written: `renderer_state.mutex`,
// `io.terminal` and `config` (the macOS arm of `encodeKeyOpts` reads
// `config.macos_option_as_alt`, set here so the `mouse`/`rt_app` detection
// fallback is never reached). `keyCallback` itself is not driven: before it
// reaches `encodeKey` it walks bindings, the inspector, the renderer and the
// apprt, none of which exist in this fixture.
#[cfg(test)]
mod mok2_key_path_e2e {
    use super::*;
    use crate::app::test_runtime::TestApp;
    use std::mem::MaybeUninit;

    fn encode(surface: &mut Surface<TestApp>, event: KeyEvent) -> Vec<u8> {
        match surface.encodeKey(&event, None).expect("encodeKey") {
            Some(req) => req.slice().to_vec(),
            None => Vec::new(),
        }
    }

    #[test]
    fn surface_mok2_keypad_alt_ctrl_end_to_end() {
        let mut full_config = configpkg::Config::default_config();
        // Keep `DerivedConfig::init` off the oniguruma FFI (the
        // `clipboard_codepoint_map_fixture_is_miri_clean` precedent); links
        // are irrelevant to key encoding.
        full_config.link.links.clear();
        let mut derived_config =
            DerivedConfig::init(Allocator, &full_config).expect("DerivedConfig::init");
        derived_config.macos_option_as_alt = Some(OptionAsAlt::True);

        let mut terminal = rustty_vt::terminal::Terminal::init(
            Allocator.into(),
            rustty_vt::terminal::Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                colors: rustty_vt::terminal::Colors::default(),
                default_modes: rustty_vt::modes::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                default_xt_checksum: Default::default(),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .expect("Terminal::init");
        {
            // modifyOtherKeys 2 (xterm `CSI > 4 ; 2 m`), as vim sends it.
            let mut s = terminal.vt_stream();
            s.next_slice(b"\x1b[>4;2m");
        }
        assert!(terminal.flags.modify_other_keys_2);

        let mutex: *mut rustty_zigstd::thread::Mutex =
            Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));

        let mut mu: Box<MaybeUninit<Surface<TestApp>>> = Box::new(MaybeUninit::uninit());
        let sp = mu.as_mut_ptr();
        // (N444: the out-of-line blocks' boxes first, then their fields.)
        unsafe {
            std::ptr::addr_of_mut!((*sp).renderer_state).write(ThreadSharedBox::new_uninit());
            std::ptr::addr_of_mut!((*sp).io).write(ThreadSharedBox::new_uninit());
            std::ptr::addr_of_mut!((*(*sp).renderer_state.as_ptr()).mutex).write(mutex);
            std::ptr::addr_of_mut!((*(*sp).io.as_ptr()).terminal).write(terminal);
            std::ptr::addr_of_mut!((*sp).config).write(derived_config);
        }

        {
            let surface = unsafe { &mut *sp };

            // Keypad 1 keeps its numeric output under MOK2 (`636a2f35b`).
            let out = encode(
                surface,
                KeyEvent {
                    key: Key::Numpad1,
                    mods: Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    utf8: b"1".to_vec(),
                    ..Default::default()
                },
            );
            assert_eq!(out, b"1");

            // Alt + a non-ASCII key prefixes the complete UTF-8 encoding of
            // the unshifted codepoint (`587e08f3f`). The event carries no
            // UTF-8 text (a frontend that supplies only the physical key's
            // unshifted codepoint): with text, the MOK2 block claims alt+é
            // first and sends `CSI 27 ; 3 ; 233 ~`, upstream too, since
            // `587e08f3f` leaves MOK2 unchanged.
            let out = encode(
                surface,
                KeyEvent {
                    key: Key::Unidentified,
                    mods: Mods {
                        alt: true,
                        ..Default::default()
                    },
                    unshifted_codepoint: 'é' as u21,
                    ..Default::default()
                },
            );
            assert_eq!(out, b"\x1b\xc3\xa9");

            // Ctrl + char uses the MOK2 CSI 27 encoding rather than the C0
            // byte (`4406cea3e`).
            let out = encode(
                surface,
                KeyEvent {
                    key: Key::KeyP,
                    mods: Mods {
                        ctrl: true,
                        ..Default::default()
                    },
                    utf8: b"p".to_vec(),
                    ..Default::default()
                },
            );
            assert_eq!(out, b"\x1b[27;5;112~");
        }

        // Teardown through `sp` only (the `clipboard_codepoint_map_e2e`
        // Stacked-Borrows note); `mu` never drops its uninitialized fields.
        unsafe {
            (*(*sp).io.as_ptr()).terminal.deinit(Allocator.into());
            (*sp).io.dealloc();
            (*sp).renderer_state.dealloc();
            (*sp).config.deinit();
            std::ptr::drop_in_place(std::ptr::addr_of_mut!((*sp).config));
            drop(Box::from_raw(mutex));
        }
        let _ = &mu;
    }
}

// ===========================================================================
// task 14d-1 (S41-F1, S42-F2): the restored `Surface.zig` diagnostics.
//
// `Surface::init` is untestable here (it spawns a live pty/renderer/io
// thread — see `renderer_config_tests`' module doc), so these follow this
// file's established fabrication recipe: a heap-pinned `Surface<TestApp>` in
// `MaybeUninit` with ONLY the fields the method under test reads written.
// Every method driven below returns before touching any other field, which
// is exactly why these four sites were chosen: they need `config` (and, for
// the key-table one, the `Default` `keyboard`) and nothing else.
//
// Four tests for 100 restored sites, chosen for shapes that break
// INDEPENDENTLY: a policy-gate `info` that must NOT fire on the allowed
// path, a second `info` in the SAME function reached only after the gate
// passes, a `debug` on an early return in a different function, and a
// `{s}`-interpolated `debug` (the `[]const u8` -> `String::from_utf8_lossy`
// conversion, which no other test here would catch). Assertions are
// full-vector `assert_eq!` on the captured records.
// ===========================================================================
#[cfg(test)]
mod diagnostics_tests {
    use super::*;
    use crate::app::test_runtime::TestApp;
    use crate::log_capture;
    use std::mem::MaybeUninit;

    /// A `Surface<TestApp>` with only `config` (and `keyboard`) live.
    ///
    /// Returns the `MaybeUninit` box so the caller keeps it alive; the
    /// `DerivedConfig` is deinit-ed + dropped in `teardown` below, matching
    /// `clipboard_codepoint_map_e2e`'s teardown convention (`.deinit()` frees
    /// the oniguruma handles, `drop_in_place` runs the ordinary `Drop` for
    /// the owned `Vec`/`String` fields that `MaybeUninit` would otherwise
    /// leak).
    fn fabricate(
        mutate: impl FnOnce(&mut configpkg::Config),
    ) -> Box<MaybeUninit<Surface<TestApp>>> {
        let mut full_config = configpkg::Config::default_config();
        // No links => `DerivedConfig::init` never reaches oniguruma, so these
        // tests are Miri-clean (the `clipboard_codepoint_map_fixture_is_miri_
        // clean` precedent).
        full_config.link.links.clear();
        mutate(&mut full_config);
        let derived = DerivedConfig::init(Allocator, &full_config).expect("DerivedConfig::init");

        let mut mu: Box<MaybeUninit<Surface<TestApp>>> = Box::new(MaybeUninit::uninit());
        let sp = mu.as_mut_ptr();
        unsafe {
            std::ptr::addr_of_mut!((*sp).config).write(derived);
            std::ptr::addr_of_mut!((*sp).keyboard).write(Keyboard::default());
        }
        mu
    }

    fn teardown(mut mu: Box<MaybeUninit<Surface<TestApp>>>) {
        let sp = mu.as_mut_ptr();
        unsafe {
            (*sp).config.deinit();
            std::ptr::drop_in_place(std::ptr::addr_of_mut!((*sp).config));
            std::ptr::drop_in_place(std::ptr::addr_of_mut!((*sp).keyboard));
        }
    }

    /// `Surface.zig:2194` — the `clipboard-write = deny` gate. Both arms are
    /// asserted: the denied one emits exactly the reference's line, and the
    /// ALLOWED one does not emit it at all. A diagnostic hoisted above the
    /// `if` would fail the second half — the success-path bug class.
    #[test]
    fn clipboard_write_deny_logs_once_and_the_allowed_path_does_not() {
        let mut mu = fabricate(|c| {
            c.clipboard_write = configpkg::value_types::ClipboardAccess::Deny;
        });
        let cap = log_capture::start();
        unsafe { &mut *mu.as_mut_ptr() }
            .clipboardWrite(b"aGk=", apprt::Clipboard::Standard)
            .expect("clipboardWrite");
        let denied = cap.messages();
        drop(cap);
        teardown(mu);

        assert_eq!(
            vec![(
                log::Level::Info,
                "surface".to_string(),
                "application attempted to write clipboard, but 'clipboard-write' is set to deny"
                    .to_string()
            )],
            denied
        );

        // The inverse arm — policy allows — is asserted by the next test:
        // with `clipboard_write = Ask` this same function runs PAST this
        // gate and the deny line is absent from its captured vector. It is
        // split out rather than done here because the allowed path with a
        // VALID payload continues into `set_clipboard`, which needs a live
        // `rt_surface` this fixture deliberately does not fabricate.
    }

    /// `Surface.zig:2206` — the invalid-base64 line, in the SAME function,
    /// reached only when the deny gate above did NOT fire. Proves the two
    /// sites are independent and that the deny line is genuinely gated.
    #[test]
    fn clipboard_write_invalid_base64_logs_and_the_deny_line_does_not() {
        let mut mu = fabricate(|c| {
            c.clipboard_write = configpkg::value_types::ClipboardAccess::Ask;
        });
        let cap = log_capture::start();
        unsafe { &mut *mu.as_mut_ptr() }
            .clipboardWrite(b"!!!not base64!!!", apprt::Clipboard::Standard)
            .expect("clipboardWrite");
        let messages = cap.messages();
        drop(cap);
        teardown(mu);

        assert_eq!(
            vec![(
                log::Level::Info,
                "surface".to_string(),
                "application sent invalid base64 data for OSC 52".to_string()
            )],
            messages
        );
    }

    /// `Surface.zig:980` — `handleMessage(.set_title)` short-circuits when a
    /// static title is configured. Different function, different level
    /// (`debug`), and an early return: a regression that dropped the
    /// early-return branch would still typecheck.
    #[test]
    fn set_title_with_a_static_config_title_logs_and_returns() {
        let mut mu = fabricate(|c| {
            c.title = Some(b"static".to_vec());
        });
        let cap = log_capture::start();
        unsafe {
            Surface::handleMessage(
                mu.as_mut_ptr(),
                Message::SetTitle({
                    let mut t = [0u8; 256];
                    t[0] = b'x';
                    t
                }),
            )
        }
        .expect("handleMessage");
        let messages = cap.messages();
        drop(cap);
        teardown(mu);

        assert_eq!(
            vec![(
                log::Level::Debug,
                "surface".to_string(),
                "ignoring title change request since static title is set via config".to_string()
            )],
            messages
        );
    }

    /// `Surface.zig:5521` — `key table not found: {s}`. The ONLY test here
    /// with an interpolated argument, and deliberately so: `{s}` on a
    /// `[]const u8` becomes `String::from_utf8_lossy`, a conversion the
    /// compiler cannot check and no other assertion in this module would
    /// catch. A missing table is the reference's own miss path.
    #[test]
    fn activate_key_table_logs_the_missing_table_name() {
        // The default config ships no key tables, so "nav" is a genuine
        // miss without touching the map at all.
        let mut mu = fabricate(|_c| {});
        let cap = log_capture::start();
        let performed = unsafe {
            Surface::performBindingAction(
                mu.as_mut_ptr(),
                binding::Action::ActivateKeyTable(b"nav".to_vec()),
            )
        }
        .expect("performBindingAction");
        let messages = cap.messages();
        drop(cap);
        teardown(mu);

        assert!(!performed);
        assert_eq!(
            vec![(
                log::Level::Debug,
                "surface".to_string(),
                "key table not found: nav".to_string()
            )],
            messages
        );
    }
}

// Parity S42-F1: `completeClipboardPasteEvent` queues its Kitty paste event
// with `.locked` while it still holds the renderer mutex (`Surface.zig`'s
// `defer ... unlock` runs after `queueIo`). The case that shows it is a FULL
// IO mailbox: `Mailbox.send` then unlocks the renderer mutex so the writer
// thread can drain, blocks, and relocks it. Fabricated with the
// `MaybeUninit<Surface>` recipe (only the fields this path reads are written).
#[cfg(test)]
mod paste_event_mutex_tests {
    use super::*;
    use crate::app::test_runtime::TestApp;
    use std::mem::MaybeUninit;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    #[cfg_attr(miri, ignore = "spawns a thread and waits on real time")]
    fn paste_event_queues_while_holding_the_renderer_mutex_on_a_full_mailbox() {
        let mut terminal = rustty_vt::terminal::Terminal::init(
            Allocator.into(),
            rustty_vt::terminal::Options {
                cols: 80,
                rows: 24,
                max_scrollback_bytes: Some(1000),
                max_scrollback_lines: None,
                colors: rustty_vt::terminal::Colors::default(),
                default_modes: rustty_vt::modes::ModePacked::default(),
                default_cursor_style: rustty_vt::screen::CursorStyle::Block,
                default_cursor_blink: Some(false),
                default_xt_checksum: Default::default(),
                kitty_image_storage_limit: 0,
                kitty_image_loading_limits: rustty_vt::kitty::graphics_image::Limits::DIRECT,
            },
        )
        .expect("Terminal::init");
        terminal
            .modes
            .set(rustty_vt::modes::Mode::KittyPasteEvents, true);

        let mut mailbox = termio::mailbox::Mailbox::initSPSC().expect("mailbox");
        let queue: *const termio::mailbox::Queue = match &mut mailbox {
            termio::mailbox::Mailbox::Spsc { queue, .. } => {
                // Fill the queue so the paste event takes the slow path.
                while queue
                    .push_or_return(
                        termio::Message::WriteAlloc(Alloc {
                            data: b"filler".to_vec().into_boxed_slice(),
                        }),
                        rustty_datastruct::blocking_queue::Timeout::Instant,
                    )
                    .is_ok()
                {}
                &**queue
            }
        };

        let renderer_mutex = Box::into_raw(Box::new(rustty_zigstd::thread::Mutex::new()));

        let mut mu: Box<MaybeUninit<Surface<TestApp>>> = Box::new(MaybeUninit::uninit());
        let sp = mu.as_mut_ptr();
        unsafe {
            std::ptr::addr_of_mut!((*sp).readonly).write(false);
            // (N444: the out-of-line blocks' boxes first, then their fields.)
            std::ptr::addr_of_mut!((*sp).renderer_state).write(ThreadSharedBox::new_uninit());
            std::ptr::addr_of_mut!((*sp).io).write(ThreadSharedBox::new_uninit());
            std::ptr::addr_of_mut!((*(*sp).renderer_state.as_ptr()).mutex).write(renderer_mutex);
            std::ptr::addr_of_mut!((*(*sp).io.as_ptr()).renderer_state)
                .write((*sp).renderer_state.as_ptr());
            std::ptr::addr_of_mut!((*(*sp).io.as_ptr()).terminal).write(terminal);
            std::ptr::addr_of_mut!(
                (*(*sp).io.as_ptr())
                    .terminal_stream
                    .handler
                    .kitty_clipboard_grants
            )
            .write(rustty_vt::kitty::clipboard::Grants::default());
            std::ptr::addr_of_mut!((*(*sp).io.as_ptr()).mailbox).write(mailbox);
        }

        // The writer's wakeup: `Mailbox.send` notifies it on the slow path right
        // before it releases the renderer mutex and blocks. Taken by pointer into
        // the surface's own mailbox, as the real writer thread waits on it
        // (`termio/Thread.zig`'s `.spsc => |*v| v`).
        // CORRECTION (task W-2, N342): this copied the Async out ("a `Copy`
        // handle"). On IOCP (Windows) an `xev.Async` is a value whose waiter is
        // registered in the copy it is called on (N343), so a copy's `wait` is
        // never woken by the mailbox's `notify`.
        let wakeup: *const rustty_xev::Async = unsafe {
            match &(*(*sp).io.as_ptr()).mailbox {
                termio::mailbox::Mailbox::Spsc { wakeup, .. } => wakeup,
            }
        };

        // The writer-thread stand-in: once the sender is parked on the full
        // queue it must be able to take the renderer mutex (the mailbox
        // released it), then it drains until the paste event arrives.
        //
        // It waits for the mailbox's wakeup, as the real writer thread does,
        // so it never tests the mutex before the sender has locked it; then
        // it polls `try_lock` against a deadline (no fixed sleep, so a slow
        // host cannot make it look too early; review-T-H item 3). The queue
        // stays full until this thread pops, so the sender cannot finish
        // before this thread has looked.
        //
        // Every wait is bounded (review-T-H2 blocking 2): a 10 s timer is armed
        // on the same loop as the wakeup wait, so `run(Once)` returns even if
        // the sender parks without notifying. On that timeout the drainer
        // skips the mutex poll but still drains, which releases the parked
        // sender; nothing is asserted on this thread (a panic here would leave
        // the sender parked forever). The main thread asserts `woke` after
        // `join`.
        struct SendPtr<T>(*const T);
        unsafe impl<T> Send for SendPtr<T> {}
        let q = SendPtr(queue);
        let m = SendPtr(renderer_mutex as *const rustty_zigstd::thread::Mutex);
        let w = SendPtr(wakeup);
        let done = Arc::new(AtomicBool::new(false));
        let done2 = done.clone();
        let notified = Arc::new(AtomicBool::new(false));
        let notified2 = notified.clone();
        let drainer = std::thread::spawn(move || {
            let (q, m, w) = (q, m, w);
            let mut loop_ = rustty_xev::Loop::init(rustty_xev::Options::default()).unwrap();
            let mut woke = false;
            let mut timed_out = false;
            let mut c_wait = rustty_xev::Completion::default();
            let mut c_timer = rustty_xev::Completion::default();
            let cb = |ud: *mut bool,
                      _l: *mut rustty_xev::Loop,
                      _c: *mut rustty_xev::Completion,
                      r: core::result::Result<(), rustty_xev::WaitError>| {
                if r.is_ok() {
                    unsafe { *ud = true };
                }
                rustty_xev::CallbackAction::Disarm
            };
            let timer_cb =
                |ud: *mut bool,
                 _l: *mut rustty_xev::Loop,
                 _c: *mut rustty_xev::Completion,
                 _r: core::result::Result<(), rustty_xev::TimerRunError>| {
                    unsafe { *ud = true };
                    rustty_xev::CallbackAction::Disarm
                };
            // SAFETY: `loop_`, `c_wait`, `c_timer`, `woke` and `timed_out`
            // outlive both completions: `run(Once)` returns after the first
            // of them fires, and the loop is deinitialized before any of them
            // goes out of scope, so the other never fires later.
            let timer = rustty_xev::Timer::init().unwrap();
            unsafe {
                (*w.0).wait(
                    &mut loop_ as *mut rustty_xev::Loop,
                    std::ptr::addr_of_mut!(c_wait),
                    std::ptr::addr_of_mut!(woke),
                    cb,
                );
                timer.run(
                    &mut loop_ as *mut rustty_xev::Loop,
                    std::ptr::addr_of_mut!(c_timer),
                    10_000,
                    std::ptr::addr_of_mut!(timed_out),
                    timer_cb,
                );
            }
            // One `run(Once)` is not enough on Linux: libxev's epoll backend
            // counts the timer's submission as a completion (`tick`: "These
            // operations happen synchronously"), so the first tick can return
            // before either callback ran (measured on the Linux VM: it returned
            // after 12 us with neither set). Tick until one of them fires; the
            // 10 s timer bounds the loop.
            let mut ran = Ok(());
            while ran.is_ok() && !woke && !timed_out {
                ran = loop_.run(rustty_xev::RunMode::Once);
            }
            let woke = woke && ran.is_ok();

            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut took_mutex_while_parked = false;
            while woke && std::time::Instant::now() < deadline {
                if unsafe { (*m.0).try_lock() } {
                    unsafe { (*m.0).unlock() };
                    took_mutex_while_parked = true;
                    break;
                }
                std::thread::yield_now();
            }
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut event: Option<Vec<u8>> = None;
            while event.is_none() && std::time::Instant::now() < deadline {
                while let Some(msg) = unsafe { (*q.0).pop() } {
                    if let termio::Message::WriteAlloc(a) = msg {
                        if &*a.data != b"filler" {
                            event = Some(a.data.to_vec());
                        }
                    }
                }
                if done2.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::yield_now();
            }
            // N382 (task W-5b): on IOCP the Async keeps pointing at `c_wait`
            // and `loop_` after its wait completed (libxev `AsyncIOCP` sets
            // `waiter` in `wait` and reads it in every `notify`), and the main
            // thread notifies once more after the paste. Both live on this
            // thread's stack, so they must outlive that last notify: wait for
            // it before deinitializing the loop and returning. CORRECTION:
            // the loop was deinitialized right after `run`, and a notify after
            // this thread exited read its freed stack (0xC0000005 on Windows).
            while !notified2.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
            loop_.deinit();
            (woke, timed_out, took_mutex_while_parked, event)
        });

        let surface = unsafe { &mut *sp };
        let result = surface
            .completeClipboardPasteEvent(apprt::Clipboard::Standard, &[b"text/plain".as_slice()]);
        done.store(true, Ordering::SeqCst);
        // If the sender never took the slow path, the drainer is still
        // waiting for the wakeup; release it so the test fails instead of
        // hanging.
        let _ = unsafe { (*wakeup).notify() };
        // The drainer's loop and wait completion may be released now (N382).
        notified.store(true, Ordering::SeqCst);
        let (woke, timed_out, took_mutex_while_parked, event) = drainer.join().expect("drainer");
        let unlocked_after = unsafe { (*renderer_mutex).try_lock() };

        // Teardown.
        unsafe {
            (*(*sp).io.as_ptr()).terminal.deinit(Allocator.into());
            std::ptr::drop_in_place(std::ptr::addr_of_mut!(
                (*(*sp).io.as_ptr())
                    .terminal_stream
                    .handler
                    .kitty_clipboard_grants
            ));
            std::ptr::read(std::ptr::addr_of!((*(*sp).io.as_ptr()).mailbox)).deinit();
            (*sp).io.dealloc();
            (*sp).renderer_state.dealloc();
            drop(Box::from_raw(renderer_mutex));
        }

        assert!(
            woke,
            "the mailbox never signalled its wakeup (timed out after 10 s: {timed_out})"
        );
        assert_eq!(result.ok(), Some(true), "the paste event was not produced");
        let event = event.expect("the paste event never reached the IO mailbox");
        assert!(
            event.starts_with(b"\x1b]5522;"),
            "unexpected paste event bytes: {:?}",
            String::from_utf8_lossy(&event)
        );
        assert!(
            took_mutex_while_parked,
            "the renderer mutex was not free while the sender waited on the full queue"
        );
        assert!(
            unlocked_after,
            "completeClipboardPasteEvent returned with the renderer mutex still locked"
        );
    }
}

// test "promptClickRelativeRow" {
//
// (`5170b06f6`; the helper is `mouse_input::prompt_click_relative_row`.)
#[cfg(test)]
#[allow(non_snake_case)]
mod prompt_click_relative_row_tests {
    use super::*;
    use rustty_vt::alloc::Global;
    use rustty_vt::point;

    #[test]
    fn promptClickRelativeRow() {
        // const testing = std.testing;
        // const Case = struct {
        //     layout: enum { no_history, offset, cross_page, prompt_above_viewport },
        //     click_y: u32,
        //     expected: u32,
        // };
        #[derive(PartialEq, Eq)]
        enum Layout {
            NoHistory,
            Offset,
            CrossPage,
            PromptAboveViewport,
        }
        struct Case {
            layout: Layout,
            click_y: u32,
            expected: u32,
        }
        // const cases = [_]Case{
        //     .{ .layout = .no_history, .click_y = 1, .expected = 1 },
        //     .{ .layout = .offset, .click_y = 3, .expected = 3 },
        //     .{ .layout = .cross_page, .click_y = 3, .expected = 3 },
        //     .{ .layout = .prompt_above_viewport, .click_y = 0, .expected = 3 },
        // };
        let cases = [
            Case {
                layout: Layout::NoHistory,
                click_y: 1,
                expected: 1,
            },
            Case {
                layout: Layout::Offset,
                click_y: 3,
                expected: 3,
            },
            Case {
                layout: Layout::CrossPage,
                click_y: 3,
                expected: 3,
            },
            Case {
                layout: Layout::PromptAboveViewport,
                click_y: 0,
                expected: 3,
            },
        ];
        // for (cases) |case| {
        for case in cases {
            // var pages = try terminal.PageList.init(testing.allocator, .{
            //     .cols = 80,
            //     .rows = 5,
            // });
            // defer pages.deinit();
            let mut pages =
                terminal::pagelist::PageList::init(Global, terminal::pagelist::Options::new(80, 5))
                    .unwrap();
            // const scroll_rows: u32 = switch (case.layout) {
            //     .no_history => 0,
            //     .offset, .prompt_above_viewport => 10,
            //     .cross_page => pages.pages.first.?.capacity().rows - 2,
            // };
            let scroll_rows: u32 = match case.layout {
                Layout::NoHistory => 0,
                Layout::Offset | Layout::PromptAboveViewport => 10,
                Layout::CrossPage => {
                    u32::from(unsafe { terminal::pagelist::Node::capacity(pages.pages.first) }.rows)
                        - 2
                }
            };
            // for (0..scroll_rows) |_| _ = try pages.grow();
            for _ in 0..scroll_rows {
                pages.grow().unwrap();
            }

            // const prompt_y = if (case.layout == .prompt_above_viewport)
            //     scroll_rows - 2
            // else
            //     scroll_rows + 1;
            let prompt_y = if case.layout == Layout::PromptAboveViewport {
                scroll_rows - 2
            } else {
                scroll_rows + 1
            };
            // const prompt = pages.pin(.{ .screen = .{ .y = prompt_y } }).?;
            let prompt = pages
                .pin(point::Point::Screen(point::Coordinate {
                    x: 0,
                    y: prompt_y,
                }))
                .unwrap();
            // const click = pages.pin(.{ .viewport = .{ .x = 7, .y = case.click_y } }).?;
            let click = pages
                .pin(point::Point::Viewport(point::Coordinate {
                    x: 7,
                    y: case.click_y,
                }))
                .unwrap();
            // try testing.expectEqual(case.layout == .cross_page, prompt.node != click.node);
            assert_eq!(
                case.layout == Layout::CrossPage,
                !std::ptr::eq(prompt.node, click.node)
            );
            // try testing.expectEqual(
            //     case.layout == .prompt_above_viewport,
            //     pages.pointFromPin(.viewport, prompt) == null,
            // );
            assert_eq!(
                case.layout == Layout::PromptAboveViewport,
                pages.point_from_pin(point::Tag::Viewport, prompt).is_none()
            );
            // try testing.expectEqual(case.expected, promptClickRelativeRow(&pages, prompt, click));
            assert_eq!(
                case.expected,
                mouse_input::prompt_click_relative_row(&pages, prompt, click)
            );

            pages.deinit();
        }
    }
}

// rustty-only (P8-W9 FX4, NEW-FINDINGS N540 item 1): Oniguruma's first-use
// init. `onig_new` -> `onig_reg_init` reads the process-global `onig_inited`
// and, if it is unset, runs `onig_initialize` itself, with no lock
// (regcomp.c:7667/7769). The reference runs `oni.init` once, single-threaded,
// at startup (`global.zig`), before any surface derives its config; Rust's
// test harness runs `DerivedConfig::init` (which compiles every configured
// link regex through `Link::oni_regex`) on many threads with no such global
// init, beside tests that call `oni::testing::ensure_init`. G-TSAN reported
// exactly that pair (`onig_reg_init` read vs `onig_initialize` write). Every
// regex compile must run after the one thread-safe init (memory
// `parallel-cargo-test-exposes-c-init-races`).
//
// The race is on the FIRST init of the process, so this test is a regression
// test only when it runs in a fresh process: run it alone under G-TSAN
// (`--exact`) to reproduce; in the full binary another test may have
// initialized Oniguruma first, and the test then still passes.
#[cfg(test)]
mod oniguruma_first_use_init_tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    #[cfg_attr(
        miri,
        ignore = "DerivedConfig::init compiles the default config's link regex through oniguruma C FFI (extern static OnigEncodingUTF8) — un-executable under Miri; the TSan and native lanes cover it"
    )]
    fn concurrent_derived_config_link_regexes_and_ensure_init_do_not_race() {
        const THREADS: usize = 8;
        let barrier = Arc::new(Barrier::new(THREADS));
        let handles: Vec<_> = (0..THREADS)
            .map(|i| {
                let barrier = Arc::clone(&barrier);
                std::thread::spawn(move || {
                    let config = configpkg::Config::default_config();
                    assert!(
                        !config.link.links.is_empty(),
                        "the default config has a link regex to compile"
                    );
                    barrier.wait();
                    if i % 2 == 0 {
                        oni::testing::ensure_init().unwrap();
                        0
                    } else {
                        let mut derived =
                            DerivedConfig::init(Allocator, &config).expect("DerivedConfig::init");
                        assert_eq!(derived.links.len(), config.link.links.len());
                        derived.deinit();
                        1
                    }
                })
            })
            .collect();
        let mut derived_inits = 0;
        for h in handles {
            derived_inits += h.join().expect("thread");
        }
        // Every odd thread really compiled the link regexes concurrently with
        // the even threads' init (N540).
        assert_eq!(derived_inits, THREADS / 2);
    }
}
