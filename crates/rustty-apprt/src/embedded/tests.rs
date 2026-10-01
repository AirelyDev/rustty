//! Unit tests for the embedded apprt backend (no reference analogue:
//! embedded.zig has 0 `test {}` blocks).

use super::inspector::key_to_imgui_key;
use super::*;

#[cfg(test)]
mod keymap_lazy_tests {
    use super::*;

    extern "C" fn stub_wakeup(_: *mut c_void) {}
    extern "C" fn stub_action(_: *mut App, _: TargetC, _: ActionC) -> bool {
        false
    }
    extern "C" fn stub_read_clipboard(
        _: *mut c_void,
        _: c_int,
        _: *mut ClipboardRequest,
        _: *const *const c_char,
        _: usize,
        _: bool,
    ) -> c_int {
        0
    }
    extern "C" fn stub_confirm_read_clipboard(
        _: *mut c_void,
        _: *const ClipboardConfirm,
        _: *mut ClipboardRequest,
        _: ClipboardRequestType,
    ) {
    }
    extern "C" fn stub_write_clipboard(
        _: *mut c_void,
        _: c_int,
        _: *const ClipboardContent,
        _: usize,
        _: bool,
    ) {
    }

    /// An `App` whose `core_app` is never dereferenced by anything these tests
    /// call. `keyboardLayout`/`reloadKeymap` touch only `self.keymap`.
    fn test_app() -> App {
        App {
            core_app: std::ptr::null_mut(),
            opts: Options {
                userdata: std::ptr::null_mut(),
                supports_selection_clipboard: false,
                wakeup: stub_wakeup,
                action: stub_action,
                read_clipboard: stub_read_clipboard,
                confirm_read_clipboard: stub_confirm_read_clipboard,
                write_clipboard: stub_write_clipboard,
                close_surface: None,
            },
            keymap: UnsafeCell::new(None),
            config: UnsafeCell::new(Config::default()),
        }
    }

    /// `reloadKeymap` on a never-initialized keymap does nothing and does NOT
    /// initialize it: "lazy initialization will pick up the current layout".
    #[test]
    fn reload_keymap_does_not_initialize() {
        let mut app = test_app();
        assert!(unsafe { &*app.keymap.get() }.is_none());
        app.reloadKeymap().unwrap();
        assert!(
            unsafe { &*app.keymap.get() }.is_none(),
            "reloadKeymap must not force the lazy init"
        );
    }

    /// `keyboardLayout` IS the initialization point. This is the positive
    /// control for the test above: it proves the keymap slot is reachable and
    /// does get filled, so `is_none()` there is a real observation and not an
    /// unreachable field.
    #[cfg(target_os = "macos")]
    #[test]
    fn keyboard_layout_initializes_lazily() {
        let app = test_app();
        assert!(unsafe { &*app.keymap.get() }.is_none());
        let _ = app.keyboardLayout();
        assert!(
            unsafe { &*app.keymap.get() }.is_some(),
            "keyboardLayout must lazily initialize the keymap"
        );
        // And a second call reuses it rather than rebuilding.
        let _ = app.keyboardLayout();
        assert!(unsafe { &*app.keymap.get() }.is_some());
    }
}

#[cfg(test)]
mod key_event_core_tests {
    use super::*;

    fn event(unshifted_codepoint: u32) -> KeyEvent<'static> {
        KeyEvent {
            action: key::Action::Press,
            mods: Mods::default(),
            consumed_mods: Mods::default(),
            keycode: 0,
            text: None,
            unshifted_codepoint,
            composing: false,
        }
    }

    /// S46-F2: `std.math.cast(u21, self.unshifted_codepoint) orelse 0`
    /// (`embedded.zig:116-119`). The largest `u21` passes through; one past it, and
    /// `u32::MAX`, become 0.
    #[test]
    fn key_event_core_zeroes_an_unshifted_codepoint_outside_u21() {
        let cp = |v| event(v).core().expect("core key event").unshifted_codepoint;
        assert_eq!(cp(u32::from('a')), u32::from('a'));
        assert_eq!(cp(0x1F_FFFF), 0x1F_FFFF);
        assert_eq!(cp(0x20_0000), 0);
        assert_eq!(cp(u32::MAX), 0);
    }
}

#[cfg(test)]
mod host_view_tests {
    use super::*;

    /// INTEGRATION_PARITY §2.6 regression: the iOS arm maps `uiview` (the
    /// reference Metal.zig:99-102 `switch { .macos => v.nsview, .ios => v.uiview }`)
    /// — never the old null catch-all.
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    #[test]
    fn ios_platform_maps_uiview() {
        let marker = 0xA5A5usize as *mut c_void;
        let platform = Platform::IOS(IOS { uiview: marker });
        assert_eq!(marker, platform_host_view(&platform));
    }

    #[cfg(any(target_os = "macos", target_os = "ios"))]
    #[test]
    fn macos_platform_maps_nsview() {
        let marker = 0x5A5Ausize as *mut c_void;
        let platform = Platform::MacOS(MacOS { nsview: marker });
        assert_eq!(marker, platform_host_view(&platform));
    }

    /// INTEGRATION_PARITY §2.9 proof: on non-darwin `MacOS`/`IOS` are ZSTs and
    /// the helper compiles (and returns null — the platform never renders there).
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    #[test]
    fn non_darwin_returns_null() {
        assert!(platform_host_view(&Platform::MacOS(MacOS)).is_null());
        assert!(platform_host_view(&Platform::IOS(IOS)).is_null());
    }

    /// Off Darwin (Linux, FreeBSD, Windows) the embedded apprt has no platform a
    /// host can create a surface on: both tags are `error.UnsupportedPlatform`
    /// (`embedded.zig:410-428`, `MacOS`/`IOS` are `void` there), as in the
    /// reference. Task W-2 (V9): this is why no live embedded surface exists on
    /// Windows in either program.
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    #[test]
    fn non_darwin_platform_init_is_unsupported() {
        for tag in [1, 2] {
            // SAFETY: an all-zero `PlatformC` is two null view pointers.
            let c: PlatformC = unsafe { core::mem::zeroed() };
            assert!(
                matches!(
                    Platform::init(tag, c),
                    Err(PlatformError::UnsupportedPlatform)
                ),
                "tag {tag}"
            );
        }
        let c: PlatformC = unsafe { core::mem::zeroed() };
        assert!(matches!(
            Platform::init(3, c),
            Err(PlatformError::InvalidEnumTag)
        ));
    }
}

#[cfg(test)]
mod inspector_style_tests {
    use super::*;
    use std::sync::Mutex;

    /// Dear ImGui's "current context" is a plain global (`GImGui`), and the
    /// default `cargo test` harness runs each `#[test]` on its own thread.
    /// Serialize every test that creates/destroys a context — same discipline
    /// as `dcimgui-sys`'s own `IMGUI_CONTEXT_TEST_LOCK`.
    static IMGUI_CONTEXT_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Known-answer test for `Inspector::updateContentScale` (oracle
    /// `embedded.zig:1111-1126`): the active ImGui style must be REPLACED with
    /// a default-constructed style scaled by `x`.
    ///
    /// Known answers are Dear ImGui 1.92.5's own `ImGuiStyle::ImGuiStyle()`
    /// defaults (`vendor/imgui/imgui.cpp`) run through `ScaleAllSizes`, which
    /// is `ImTrunc(value * scale_factor)`:
    ///   * `WindowPadding`  (8, 8)   -> (16, 16)
    ///   * `ScrollbarSize`   14.0    -> 28.0
    ///   * `GrabMinSize`     12.0    -> 24.0
    ///
    /// `updateContentScale` never dereferences `self.surface`, so the test
    /// constructs an `Inspector` with a null surface back-pointer.
    #[test]
    fn update_content_scale_rescales_active_imgui_style() {
        let _guard = IMGUI_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // SAFETY: single-threaded under the lock above; every pointer handed
        // to Dear ImGui is a live local, and the context is destroyed below.
        unsafe {
            let ig_ctx = dcimgui_sys::ImGui_CreateContext(core::ptr::null_mut());
            assert!(!ig_ctx.is_null(), "ImGui_CreateContext(NULL) returned null");

            // Dear ImGui persists window geometry to `io.IniFilename` (default
            // `"imgui.ini"`) on the next frame/shutdown; null it out so this
            // test never writes a stray file into the crate root (same
            // discipline as `dcimgui-sys`'s `metal_offscreen` KAT).
            let io = dcimgui_sys::ImGui_GetIO();
            (*io).IniFilename = core::ptr::null();

            let mut inspector = Inspector {
                surface: core::ptr::null_mut(),
                ig_ctx,
                backend: None,
                content_scale: 1.0,
                instant: None,
            };

            // Baseline: the fresh context carries the UNSCALED defaults.
            let style = dcimgui_sys::ImGui_GetStyle();
            assert!(!style.is_null(), "ImGui_GetStyle() returned null");
            assert_eq!((*style).WindowPadding.x, 8.0, "baseline WindowPadding.x");
            assert_eq!((*style).ScrollbarSize, 14.0, "baseline ScrollbarSize");
            assert_eq!((*style).GrabMinSize, 12.0, "baseline GrabMinSize");

            inspector.updateContentScale(2.0, 2.0);

            // The pure-Rust effect: the cached scale used by cursorPosCallback.
            assert_eq!(inspector.content_scale, 2.0, "cached content_scale");

            // The dcimgui effect: the ACTIVE style is the scaled default style.
            let style = dcimgui_sys::ImGui_GetStyle();
            assert_eq!((*style).WindowPadding.x, 16.0, "scaled WindowPadding.x");
            assert_eq!((*style).WindowPadding.y, 16.0, "scaled WindowPadding.y");
            assert_eq!((*style).ScrollbarSize, 28.0, "scaled ScrollbarSize");
            assert_eq!((*style).GrabMinSize, 24.0, "scaled GrabMinSize");

            dcimgui_sys::ImGui_DestroyContext(ig_ctx);
        }
    }

    /// Scaling is computed from a FRESH default style each call (the oracle
    /// default-constructs `style` every time), so calling twice with the same
    /// factor is idempotent — it must NOT compound to 4x.
    #[test]
    fn update_content_scale_is_idempotent_not_compounding() {
        let _guard = IMGUI_CONTEXT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // SAFETY: as above.
        unsafe {
            let ig_ctx = dcimgui_sys::ImGui_CreateContext(core::ptr::null_mut());
            assert!(!ig_ctx.is_null(), "ImGui_CreateContext(NULL) returned null");

            // See the sibling test above: null `IniFilename` so this test
            // never writes a stray `imgui.ini` into the crate root.
            let io = dcimgui_sys::ImGui_GetIO();
            (*io).IniFilename = core::ptr::null();

            let mut inspector = Inspector {
                surface: core::ptr::null_mut(),
                ig_ctx,
                backend: None,
                content_scale: 1.0,
                instant: None,
            };

            inspector.updateContentScale(2.0, 2.0);
            inspector.updateContentScale(2.0, 2.0);

            let style = dcimgui_sys::ImGui_GetStyle();
            assert_eq!(
                (*style).WindowPadding.x,
                16.0,
                "WindowPadding.x must stay 16 (not compound to 32)"
            );

            dcimgui_sys::ImGui_DestroyContext(ig_ctx);
        }
    }
}

// UD27 (PORT NOTE: reference defect (R13), at `key_to_imgui_key`): keypad Page
// Down reaches the inspector as Page Down. The reference maps it to Page Up.
#[cfg(test)]
mod imgui_key_tests {
    use super::*;

    #[test]
    fn numpad_page_down_maps_to_page_down() {
        use key::Key;
        assert_eq!(
            key_to_imgui_key(Key::NumpadPageDown),
            Some(dcimgui_sys::ImGuiKey_PageDown as c_int)
        );
        assert_eq!(
            key_to_imgui_key(Key::NumpadPageDown),
            key_to_imgui_key(Key::PageDown)
        );
        assert_eq!(
            key_to_imgui_key(Key::NumpadPageUp),
            Some(dcimgui_sys::ImGuiKey_PageUp as c_int)
        );
        assert_ne!(
            key_to_imgui_key(Key::NumpadPageDown),
            key_to_imgui_key(Key::NumpadPageUp)
        );
    }

    // UD27 (2): keypad Begin is unmapped (the reference sends it as Tab).
    #[test]
    fn numpad_begin_is_unmapped() {
        use key::Key;
        assert_eq!(key_to_imgui_key(Key::NumpadBegin), None);
        assert_ne!(
            key_to_imgui_key(Key::Tab),
            None,
            "Tab itself is still mapped"
        );
    }
}
