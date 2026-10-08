//! P8-W6 T2 (`40d5b860d`, DECISIONS P10, N522): where the app-scoped render
//! `Device` lives.
//!
//! Upstream embeds the device BY VALUE in `App` (`App.zig:57-58`
//! `device: renderer.Device`) and hands every surface renderer `&app.device`
//! (`Surface.zig:564`), which the renderer reads on its render thread
//! (`Metal.zig` `self.device.device` / `.max_texture_size` /
//! `.default_storage_mode`; `OpenGL.zig` `self.device.display`). Zig pointers
//! carry no aliasing rule. In Rust the main thread keeps calling `&mut self`
//! methods on the core `App` while render threads run (`addSurface`,
//! `deleteSurface`, `updateConfig`, `deinit`, ...): each one is a PROTECTED
//! `&mut App` over every byte of the app, so a render-thread read of the
//! device bytes during the call is Stacked Borrows UB.
//!
//! The candidates (run by `run-all.sh` under SB and TB):
//!
//! - `inline_device`: the literal shape. The render thread reads the device
//!   through `&raw const (*app).device` while the main thread is inside
//!   `App::add_surface(&mut self)`. Expected: UB under SB.
//! - `boxed_device`: the port (`DeviceBox(*mut Device)`, `Box::into_raw`). The
//!   device is its own allocation, so `&mut App` covers only the box's pointer
//!   bytes. Expected: CLEAN under SB and TB.
//! - `boxed_device_lifecycle`: the port's whole lifetime: create the device in
//!   `App::init`, two surfaces whose render threads both read it, destroy one
//!   surface (join its thread), keep the other reading across further
//!   `&mut App` calls, then `App::deinit` (surfaces first, the device last,
//!   freed with `Box::from_raw`). Expected: CLEAN, and no leak.
//! - `boxed_device_null`: the `#[cfg(test)]` null-device constructor (P11):
//!   `deinit` skips the null box. Expected: CLEAN, no leak.
//! - `boxed_device_freed_before_surfaces` (negative control): `deinit` frees the
//!   device BEFORE joining the surviving render thread, i.e. the reference's
//!   ordering inverted. Expected: UB (use after free), proving the harness sees
//!   the lifetime the ordering protects.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::thread;

/// Stand-in for `rustty_renderer::Device`: the fields the render thread reads
/// (`max_texture_size`, `default_storage_mode`) and the handle it uses.
struct Device {
    handle: u64,
    default_storage_mode: u32,
    max_texture_size: u32,
}

impl Device {
    fn init() -> Device {
        Device {
            handle: 0xD3_71CE,
            default_storage_mode: 1,
            max_texture_size: 16384,
        }
    }

    /// What a renderer does with the device on its thread.
    fn read(&self) -> u64 {
        self.handle + u64::from(self.default_storage_mode) + u64::from(self.max_texture_size)
    }
}

/// A raw pointer the render thread owns (the core's `Send` wrappers).
#[derive(Clone, Copy)]
struct SendPtr(*const Device);
unsafe impl Send for SendPtr {}

/// Step flag the two threads hand off on (so the interleaving is fixed, not
/// left to the scheduler).
struct Step(AtomicU8);

impl Step {
    fn new() -> Arc<Step> {
        Arc::new(Step(AtomicU8::new(0)))
    }
    fn set(&self, v: u8) {
        self.0.store(v, Ordering::Release);
    }
    fn wait(&self, v: u8) {
        while self.0.load(Ordering::Acquire) != v {
            thread::yield_now();
        }
    }
}

// ---------------------------------------------------------------------------
// The literal shape: the device by value in `App`.
// ---------------------------------------------------------------------------

struct InlineApp {
    surfaces: Vec<u32>,
    device: Device,
}

impl InlineApp {
    /// `App.addSurface` (`&mut self`): the render thread reads the device while
    /// this protected reference is live.
    fn add_surface(&mut self, s: u32, step: &Step) {
        self.surfaces.push(s);
        step.set(1); // let the render thread read now
        step.wait(2); // ... and wait for it inside this call
    }
}

fn inline_device() {
    let app: *mut InlineApp = Box::into_raw(Box::new(InlineApp {
        surfaces: Vec::new(),
        device: Device::init(),
    }));
    // `Surface.zig:564` `.device = &app.device` (the port's
    // `lifecycle.rs` reads the field by place from the raw app pointer).
    let dev = SendPtr(unsafe { &raw const (*app).device });
    let step = Step::new();
    let s2 = step.clone();
    let render = thread::spawn(move || {
        let dev = dev;
        s2.wait(1);
        // SAFETY (model): the renderer reads its device on the render thread.
        let v = unsafe { (*dev.0).read() };
        s2.set(2);
        v
    });
    unsafe { (*app).add_surface(1, &step) };
    let v = render.join().unwrap();
    assert_ne!(v, 0);
    drop(unsafe { Box::from_raw(app) });
}

// ---------------------------------------------------------------------------
// The port: `DeviceBox(*mut Device)`.
// ---------------------------------------------------------------------------

/// `rustty_core::app::DeviceBox`: the device in its own allocation.
struct DeviceBox(*mut Device);

impl DeviceBox {
    fn init() -> DeviceBox {
        DeviceBox(Box::into_raw(Box::new(Device::init())))
    }
    /// The `#[cfg(test)]` constructor's null box.
    fn null() -> DeviceBox {
        DeviceBox(core::ptr::null_mut())
    }
    fn as_ptr(&self) -> *const Device {
        self.0
    }
    /// `App.deinit`'s `self.device.deinit()`: free it, once. A null box (the
    /// test constructor) is skipped.
    unsafe fn deinit(&mut self) {
        if !self.0.is_null() {
            drop(unsafe { Box::from_raw(self.0) });
            self.0 = core::ptr::null_mut();
        }
    }
}

/// A surface: its render thread (the renderer's device read loop).
struct Surface {
    stop: Arc<Step>,
    render: Option<thread::JoinHandle<u64>>,
}

impl Surface {
    /// `Surface.init` → `Renderer.init(.{ .device = &app.device })` → the
    /// render thread, which reads the device until it is stopped.
    fn init(dev: *const Device) -> Surface {
        let dev = SendPtr(dev);
        let stop = Step::new();
        let s2 = stop.clone();
        let render = thread::spawn(move || {
            let dev = dev;
            let mut acc = 0u64;
            // Read at least once, then until told to stop.
            loop {
                // SAFETY (model): the device outlives every renderer.
                acc = acc.wrapping_add(unsafe { (*dev.0).read() });
                if s2.0.load(Ordering::Acquire) == 1 {
                    break;
                }
                thread::yield_now();
            }
            acc
        });
        Surface {
            stop,
            render: Some(render),
        }
    }

    /// `Surface.deinit`: stop and join the render thread (the renderer is
    /// destroyed after its thread).
    fn deinit(&mut self) {
        self.stop.set(1);
        if let Some(t) = self.render.take() {
            assert_ne!(t.join().unwrap(), 0);
        }
    }
}

struct BoxedApp {
    surfaces: Vec<Surface>,
    device: DeviceBox,
}

impl BoxedApp {
    /// `App.init`: the device is created at the reference's point.
    fn init() -> BoxedApp {
        BoxedApp {
            surfaces: Vec::new(),
            device: DeviceBox::init(),
        }
    }
    fn init_null_device() -> BoxedApp {
        BoxedApp {
            surfaces: Vec::new(),
            device: DeviceBox::null(),
        }
    }
    fn add_surface(&mut self, s: Surface) {
        self.surfaces.push(s);
        // Keep the protected `&mut self` live across render-thread reads.
        for _ in 0..8 {
            thread::yield_now();
        }
    }
    fn delete_surface(&mut self, i: usize) {
        let mut s = self.surfaces.remove(i);
        s.deinit();
    }
    /// `App.deinit`: the surfaces, then the device ("This must happen after all
    /// surfaces are gone since their renderers borrow it.").
    fn deinit(&mut self) {
        for s in self.surfaces.iter_mut() {
            s.deinit();
        }
        self.surfaces.clear();
        unsafe { self.device.deinit() };
    }
    /// The inverted order (negative control).
    fn deinit_device_first(&mut self) {
        unsafe { self.device.deinit() };
        // The surviving render thread is still drawing.
        for _ in 0..64 {
            thread::yield_now();
        }
        for s in self.surfaces.iter_mut() {
            s.deinit();
        }
        self.surfaces.clear();
    }
}

fn boxed_device() {
    let app: *mut BoxedApp = Box::into_raw(Box::new(BoxedApp::init()));
    let dev = SendPtr(unsafe { (*app).device.as_ptr() });
    let step = Step::new();
    let s2 = step.clone();
    let render = thread::spawn(move || {
        let dev = dev;
        s2.wait(1);
        let v = unsafe { (*dev.0).read() };
        s2.set(2);
        v
    });
    // The same protected `&mut App` window as `inline_device`.
    unsafe {
        let this: &mut BoxedApp = &mut *app;
        this.surfaces.push(Surface {
            stop: Step::new(),
            render: None,
        });
        step.set(1);
        step.wait(2);
    }
    assert_ne!(render.join().unwrap(), 0);
    unsafe {
        (*app).deinit();
        drop(Box::from_raw(app));
    }
}

fn boxed_device_lifecycle() {
    let app: *mut BoxedApp = Box::into_raw(Box::new(BoxedApp::init()));
    // Two surfaces on one App, both rendering through the shared device.
    let dev_a = unsafe { (*app).device.as_ptr() };
    let a = Surface::init(dev_a);
    unsafe { (*app).add_surface(a) };
    let dev_b = unsafe { (*app).device.as_ptr() };
    let b = Surface::init(dev_b);
    unsafe { (*app).add_surface(b) };
    // Close one tab while the other keeps drawing.
    unsafe { (*app).delete_surface(0) };
    // More main-thread `&mut App` work while the survivor renders.
    unsafe { (*app).add_surface(Surface::init((*app).device.as_ptr())) };
    unsafe { (*app).delete_surface(1) };
    // Quit: surfaces first, the device last.
    unsafe {
        (*app).deinit();
        assert!((*app).device.0.is_null());
        drop(Box::from_raw(app));
    }
}

fn boxed_device_null() {
    let mut app = BoxedApp::init_null_device();
    assert!(app.device.as_ptr().is_null());
    app.deinit();
}

fn boxed_device_freed_before_surfaces() {
    let app: *mut BoxedApp = Box::into_raw(Box::new(BoxedApp::init()));
    let s = Surface::init(unsafe { (*app).device.as_ptr() });
    unsafe { (*app).add_surface(s) };
    unsafe {
        (*app).deinit_device_first();
        drop(Box::from_raw(app));
    }
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_default();
    match which.as_str() {
        "inline_device" => inline_device(),
        "boxed_device" => boxed_device(),
        "boxed_device_lifecycle" => boxed_device_lifecycle(),
        "boxed_device_null" => boxed_device_null(),
        "boxed_device_freed_before_surfaces" => boxed_device_freed_before_surfaces(),
        other => panic!("unknown candidate {other:?}"),
    }
}
