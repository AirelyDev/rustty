//! RUSTTY-ONLY (P8-W6 T2, `40d5b860d`): Review Focus 4, "closing tabs while
//! other tabs render".
//!
//! With the app-scoped render device every surface renderer borrows
//! `App.device`. Two REAL surfaces (each with its renderer, render thread, io
//! thread and `/bin/cat` pty) are created on one `App`; one is destroyed while
//! the other keeps rendering; the survivor then draws a frame; and the App is
//! deinitialized. Nothing may use the device after it is freed, and nothing may
//! race: the drop-order probe (`super::drop_order_probe`) asserts the device is
//! freed after BOTH renderers.
//!
//! Real GPU: Metal on macOS, OpenGL (EGL) on Linux, llvmpipe on rvm. A host
//! without a render device skips through the R14 pivot
//! (`RUSTTY_REQUIRE_METAL` / `RUSTTY_REQUIRE_GL`). Miri cannot run render threads
//! or GPU calls, so the device-lifetime half of this focus is proven there by
//! `.port/phase-2/mirihar-device-box` and the null-device `App` tests; this test
//! is written to run unchanged under G-TSAN (plain threads, no timing
//! assumptions beyond bounded polling).

use std::mem::MaybeUninit;
#[cfg(not(target_os = "macos"))]
use std::time::{Duration, Instant};

use allocator_api2::alloc::Global as Allocator;
use rustty_apprt_api as apprt;
use rustty_apprt_api::Surface as _;
use rustty_config::Config;

use super::drop_order_probe::{self, Event};
use super::App;
use crate::surface::Surface;

/// The apprt app: accepts every action, needs nothing else.
struct DeviceApp {
    config: Config,
}

impl apprt::App for DeviceApp {
    type Surface = RtSurface;

    fn perform_action<C: apprt::CoreSurfaceHandle>(
        &self,
        _target: apprt::Target<'_, C>,
        _action: apprt::Action<'_>,
    ) -> Result<bool, apprt::ApprtError> {
        Ok(true)
    }

    fn perform_ipc(
        &self,
        _target: apprt::ipc::Target<'_>,
        _action: apprt::ipc::Action<'_>,
    ) -> Result<bool, apprt::ipc::Errors> {
        unimplemented!("Review Focus 4 test: not driven")
    }

    fn wakeup(&self) {}

    fn config(&self) -> &Config {
        &self.config
    }

    fn keyboard_layout(&self) -> rustty_input::keyboard::Layout {
        rustty_input::keyboard::Layout::Unknown
    }
}

/// The apprt surface, shaped like the embedded runtime's: it owns the core
/// surface's allocation, and its `deinit` removes itself from the app and then
/// deinitializes the core surface (`embedded.zig` `Surface.deinit`).
struct RtSurface {
    core: *mut Surface<DeviceApp>,
    core_app: *mut App<DeviceApp>,
    rt_app: *mut DeviceApp,
}

impl apprt::Surface for RtSurface {
    type App = DeviceApp;
    type Core = Surface<DeviceApp>;

    fn core(&self) -> *mut Surface<DeviceApp> {
        self.core
    }
    fn rt_app(&self) -> *mut DeviceApp {
        self.rt_app
    }
    unsafe fn close(_self: *const Self, _process_alive: bool) {}
    fn deinit(&mut self) {
        // self.app.core_app.deleteSurface(self);
        let app = self.core_app;
        let self_ptr = core::ptr::from_mut(self);
        unsafe { (*app).deleteSurface(self_ptr) };
        // self.core_surface.deinit();
        unsafe { (*self.core).deinit() };
    }
    fn get_content_scale(&self) -> Result<apprt::ContentScale, apprt::ApprtError> {
        Ok(apprt::ContentScale { x: 1.0, y: 1.0 })
    }
    fn get_size(&self) -> Result<apprt::SurfaceSize, apprt::ApprtError> {
        Ok(apprt::SurfaceSize {
            width: 320,
            height: 200,
        })
    }
    fn get_title(&self) -> Option<&std::ffi::CStr> {
        None
    }
    fn supports_clipboard(&self, _clipboard_type: apprt::Clipboard) -> bool {
        false
    }
    fn clipboard_request(
        &self,
        _clipboard_type: apprt::Clipboard,
        _state: apprt::ClipboardRequest,
    ) -> Result<apprt::ClipboardReadResult, apprt::ApprtError> {
        Err(apprt::ApprtError)
    }
    fn set_clipboard(
        &self,
        _clipboard_type: apprt::Clipboard,
        _contents: &[apprt::ClipboardContent<'_>],
        _confirm: bool,
    ) -> Result<(), apprt::ApprtError> {
        Ok(())
    }
    fn get_cursor_pos(&self) -> Result<apprt::CursorPos, apprt::ApprtError> {
        Ok(apprt::CursorPos { x: 0.0, y: 0.0 })
    }
    fn default_termio_env(&self) -> Result<rustty_global::EnvMap, apprt::ApprtError> {
        Ok(rustty_global::EnvMap::new())
    }
}

/// One live surface: the apprt surface box and the core surface allocation.
struct Live {
    rt: *mut RtSurface,
    core: *mut Surface<DeviceApp>,
}

/// `embedded.zig` `Surface.init`: add to the app, then initialize the core
/// surface in place (its pty runs `/bin/cat`, which prints nothing).
unsafe fn new_surface(app: *mut App<DeviceApp>, rt_app: *mut DeviceApp, config: &Config) -> Live {
    let core: *mut Surface<DeviceApp> =
        Box::into_raw(Box::new(MaybeUninit::<Surface<DeviceApp>>::uninit())).cast();
    let rt = Box::into_raw(Box::new(RtSurface {
        core,
        core_app: app,
        rt_app,
    }));
    // try app.core_app.addSurface(self);
    unsafe { (*app).addSurface(rt) };
    // try self.core_surface.init(alloc, &config, app.core_app, app, self);
    let res = unsafe { Surface::init(core, Allocator, config, app, rt_app, rt) };
    if let Err(err) = res {
        panic!("Surface::init failed on a live render device: {err:?}");
    }
    Live { rt, core }
}

/// Free a surface's two allocations after its `deinit` ran (the core surface
/// without drop glue, as every owner frees it: `Surface::deinit`'s contract).
unsafe fn free_surface(live: Live) {
    unsafe {
        drop(Box::from_raw(
            live.core.cast::<MaybeUninit<Surface<DeviceApp>>>(),
        ));
        drop(Box::from_raw(live.rt));
    }
}

/// Draw a frame on `live`, and observe it.
///
/// macOS: `Surface.draw` (`drawFrame(true)` on the calling thread, which the
/// macOS host does during a resize; "Renderers are required to support
/// `drawFrame` being called from the main thread"). OpenGL: the render thread
/// owns the context, so a resize queues a render and the test waits until that
/// thread exports a frame (`takeFrame`, the GTK `RenderSurface` path).
unsafe fn draw_one_frame(app: *mut App<DeviceApp>, rt_app: &DeviceApp, live: &Live) {
    // A resize marks the renderer dirty and wakes its thread.
    unsafe {
        (*live.core)
            .sizeCallback(apprt::SurfaceSize {
                width: 330,
                height: 210,
            })
            .expect("sizeCallback");
    }
    #[cfg(target_os = "macos")]
    unsafe {
        Surface::draw(live.core).expect("Surface::draw on the surviving surface");
        let _ = (app, rt_app);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            // Keep the app mailbox drained (renderer health reports).
            unsafe { App::tick(app, rt_app) }.expect("App::tick");
            let frame = unsafe {
                crate::renderer_seam::Renderer::takeFrame((*live.core).renderer.as_ptr())
            };
            if let Some(frame) = frame {
                frame.deinit();
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the surviving surface never rendered a frame"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// Review Focus 4: two surfaces on one App, destroy one while the other
/// renders, draw a frame on the survivor, then deinit the App. The device
/// outlives both renderers.
#[test]
#[cfg_attr(
    miri,
    ignore = "real render device, render/io threads and a pty: foreign to Miri (the device lifetime is proven by .port/phase-2/mirihar-device-box and the null-device App tests)"
)]
fn two_surfaces_share_the_device_destroy_one_draw_the_other_then_deinit() {
    const TEST: &str = "two_surfaces_share_the_device_destroy_one_draw_the_other_then_deinit";
    let _ = drop_order_probe::take();
    // The entrypoint's global state (`rustty_init` / `main`): `Surface.init` reads
    // `global_state.resources_dir`. Idempotent across tests.
    let _ = crate::STATE.set(crate::GlobalState::new((), Default::default()));

    let mut config = Config::default_config();
    config.command = Some(rustty_config::command::Command::Direct(vec![
        b"/bin/cat".to_vec()
    ]));
    let mut rt_app = Box::new(DeviceApp {
        config: Config::default_config(),
    });
    let rt_app_ptr: *mut DeviceApp = &raw mut *rt_app;

    let app: *mut App<DeviceApp> = match App::<DeviceApp>::create(Allocator) {
        Ok(app) => Box::into_raw(app),
        Err(err) => {
            super::test_runtime::require_device_or_skip(TEST, &err);
            return;
        }
    };
    let device = unsafe { (*app).device.as_ptr() };
    assert!(!device.is_null());

    // Two tabs, both rendering through the app's one device.
    let a = unsafe { new_surface(app, rt_app_ptr, &config) };
    let b = unsafe { new_surface(app, rt_app_ptr, &config) };
    assert_eq!(unsafe { (*app).surfaces.len() }, 2);

    // Close tab A while B's render thread is live.
    unsafe {
        (*a.rt).deinit();
        free_surface(a);
    }
    assert_eq!(unsafe { (*app).surfaces.len() }, 1);
    assert_eq!(
        drop_order_probe::take(),
        vec![Event::Renderer],
        "closing one surface frees its renderer only, never the shared device"
    );
    assert_eq!(unsafe { (*app).device.as_ptr() }, device);

    // B still draws through the shared device.
    unsafe { draw_one_frame(app, &rt_app, &b) };

    // Quit: `App.deinit` deinitializes B (through its apprt surface), then
    // frees the device.
    let b_alloc = Live {
        rt: b.rt,
        core: b.core,
    };
    unsafe { Box::from_raw(app).destroy() };
    assert_eq!(
        drop_order_probe::take(),
        vec![Event::Renderer, Event::Device],
        "the render device must outlive every renderer that borrows it"
    );
    unsafe { free_surface(b_alloc) };
    drop(rt_app);
}
