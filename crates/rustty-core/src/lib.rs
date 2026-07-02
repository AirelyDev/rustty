//! rustty-core
//!
//! PORTS: ghostty src/App.zig, Surface.zig, surface_mouse.zig + the high-level
//! wiring split from global.zig (break #1, additive glue). Depends on apprt-api, not apprt.
//! (src/Command.zig is RELOCATED to rustty-termio — break #13b, Command-in-core
//! would cycle; Surface reaches it as `rustty_termio::Command`.)
//!
//! PORT STATUS: stub (core chunk C0 landed: seam prep + the `type Core`
//! decision gate below; chunk C2 landed `surface_mouse` in full — done; chunk
//! C3 landed the `renderer_seam` dependency placeholders + the minimal
//! `app`/`surface` module skeletons they name; the full App/Surface ports
//! land C4+ per CORE_BLUEPRINT §6).
//!
//! Carved incrementally (R1: every line/comment/test{} ported as the crate is
//! carved, leaves-first, under the acyclicity gate per R4).

// PORT NOTE: App.zig — chunk C4 owns the full file; C3 landed the minimal
// `Mailbox<A>`/`Queue<A>`/`Message<A>` skeleton (renderer_seam::Thread<A>
// embeds `app::Mailbox<A>`, renderer/Thread.zig:89).
pub mod app;

// PORT NOTE (CORE_BLUEPRINT §5): core-local dependency placeholders for the
// Phase-3 GPU renderer + renderer thread — `Renderer`, `Thread<A>`,
// `Options<A>` (chunk C3).
pub mod renderer_seam;

// PORT NOTE: Surface.zig — chunks C5+ own the full file; C3 landed the
// `Surface<A>` shell + the `SurfaceMailbox<A>` (apprt.surface.Mailbox) field
// shape (renderer_seam::Options<A> names it).
pub mod surface;

// PORT NOTE: Surface.zig:38 `const SurfaceMouse = @import("surface_mouse.zig");`
// — the future surface.rs (C5+) reaches the module as `crate::surface_mouse`.
pub mod surface_mouse;

// C0 DECISION GATE (CORE_BLUEPRINT §1/§6): prototype the OPTION-1
// self-referential `type Core` bound — `impl<A> ... where A: apprt_api::App,
// A::Surface: apprt_api::Surface<App = A, Core = Surface<A>>` — in isolation,
// with a dummy backend `A`, so the trait solver's verdict is evidence before
// C4+ builds on it (fallback: option 2, an opaque `core_ptr()` void* cast).
// VERDICT: the solver ACCEPTS it — recorded in CORE_BLUEPRINT.md "## C0
// DECISION"; later chunks follow option 1. Kept as a `#[cfg(test)]` module
// (rustty-only compile+run evidence, NOT a port): it is replaced by the real
// `Surface<A>` impl blocks as C4/C5 land.
#[cfg(test)]
mod type_core_prototype {
    use rustty_apprt_api as apprt;
    // The trait must be in scope for `rt.core()`/`rt.rt_app()` method syntax
    // (renamed: `Surface` is what the stand-in struct below is named for).
    use rustty_apprt_api::Surface as ApprtSurface;

    /// Minimal stand-in for core's future `Surface<A>` (chunk C5): generic
    /// over the backend exactly as the real struct will be (CORE_BLUEPRINT §1).
    struct CoreSurface<A: apprt::App + 'static> {
        /// `rt_surface: *apprt.runtime.Surface` — the field the real struct
        /// carries; here it makes the prototype's genericity load-bearing.
        rt_surface: *mut A::Surface,
    }

    /// The OPTION-1 impl-block constraint, verbatim shape from §1:
    /// `A::Surface: Surface<App = A, Core = CoreSurface<A>>` — a
    /// SELF-REFERENTIAL bound (the associated type on `A`'s surface names a
    /// type generic over `A` under that very bound).
    impl<A> CoreSurface<A>
    where
        A: apprt::App + 'static,
        A::Surface: apprt::Surface<App = A, Core = CoreSurface<A>>,
    {
        /// The `rt_surface.core()` edge core needs at App.zig:140/193/231/285/
        /// 477/502/511 — TYPED, no void* cast (what option 1 buys over
        /// option 2).
        fn core_of(rt: &A::Surface) -> *mut CoreSurface<A> {
            rt.core()
        }

        /// The `rt_surface.rtApp()` edge (App.zig ×2) under the same bound.
        fn rt_app_of(rt: &A::Surface) -> *mut A {
            rt.rt_app()
        }
    }

    // ---- dummy backend `A` (rustty-only test glue, NOT a port of
    // apprt/none.zig — CORE_BLUEPRINT §7 keeps none.zig for the apprt
    // milestone; method bodies that the test never drives are
    // `unimplemented!()`) ----

    struct TestApp;

    struct TestSurface {
        core: *mut CoreSurface<TestApp>,
        app: *mut TestApp,
    }

    impl apprt::App for TestApp {
        type Surface = TestSurface;

        fn perform_action<C: apprt::CoreSurfaceHandle>(
            &self,
            _target: apprt::Target<'_, C>,
            _action: apprt::Action<'_>,
        ) -> Result<bool, apprt::ApprtError> {
            unimplemented!("dummy backend: not driven by the prototype test")
        }

        fn perform_ipc(
            &self,
            _target: rustty_apprt_api::ipc::Target<'_>,
            _action: rustty_apprt_api::ipc::Action<'_>,
        ) -> Result<bool, rustty_apprt_api::ipc::Errors> {
            unimplemented!("dummy backend: not driven by the prototype test")
        }

        fn wakeup(&self) {}

        fn config(&self) -> &rustty_config::Config {
            unimplemented!("dummy backend: not driven by the prototype test")
        }

        fn keyboard_layout(&self) -> rustty_input::keyboard::Layout {
            rustty_input::keyboard::Layout::Unknown
        }
    }

    impl apprt::Surface for TestSurface {
        type App = TestApp;
        type Core = CoreSurface<TestApp>;

        fn core(&self) -> *mut CoreSurface<TestApp> {
            self.core
        }

        fn rt_app(&self) -> *mut TestApp {
            self.app
        }

        fn close(&self, _process_alive: bool) {}

        fn get_content_scale(&self) -> Result<apprt::ContentScale, apprt::ApprtError> {
            unimplemented!("dummy backend: not driven by the prototype test")
        }

        fn get_size(&self) -> Result<apprt::SurfaceSize, apprt::ApprtError> {
            unimplemented!("dummy backend: not driven by the prototype test")
        }

        fn get_title(&self) -> Option<&std::ffi::CStr> {
            None
        }

        fn supports_clipboard(&self, _clipboard_type: apprt::Clipboard) -> bool {
            unimplemented!("dummy backend: not driven by the prototype test")
        }

        fn clipboard_request(
            &self,
            _clipboard_type: apprt::Clipboard,
            _state: apprt::ClipboardRequest,
        ) -> Result<bool, apprt::ApprtError> {
            unimplemented!("dummy backend: not driven by the prototype test")
        }

        fn set_clipboard(
            &self,
            _clipboard_type: apprt::Clipboard,
            _contents: &[apprt::ClipboardContent<'_>],
            _confirm: bool,
        ) -> Result<(), apprt::ApprtError> {
            unimplemented!("dummy backend: not driven by the prototype test")
        }

        fn get_cursor_pos(&self) -> Result<apprt::CursorPos, apprt::ApprtError> {
            unimplemented!("dummy backend: not driven by the prototype test")
        }

        fn default_termio_env(
            &self,
        ) -> Result<std::collections::HashMap<String, String>, apprt::ApprtError> {
            unimplemented!("dummy backend: not driven by the prototype test")
        }
    }

    // rustty-only (C0 decision evidence): instantiating the generic block at
    // `A = TestApp` forces the solver to normalize the recursive equality
    // `<TestApp::Surface as Surface>::Core == CoreSurface<TestApp>` and prove
    // the self-referential where-clause; the calls then round-trip real
    // pointers through the typed edges.
    #[test]
    fn option1_self_referential_core_bound() {
        let mut app = TestApp;
        let mut core = CoreSurface::<TestApp> {
            rt_surface: std::ptr::null_mut(),
        };
        let mut surf = TestSurface {
            core: &mut core,
            app: &mut app,
        };
        // Wire the back-pointer the way core's init will (§1 field shape).
        core.rt_surface = &mut surf;

        let core_ptr = CoreSurface::<TestApp>::core_of(&surf);
        assert_eq!(core_ptr, std::ptr::addr_of_mut!(core));

        let app_ptr = CoreSurface::<TestApp>::rt_app_of(&surf);
        assert_eq!(app_ptr, std::ptr::addr_of_mut!(app));

        assert_eq!(core.rt_surface, std::ptr::addr_of_mut!(surf));
    }
}
