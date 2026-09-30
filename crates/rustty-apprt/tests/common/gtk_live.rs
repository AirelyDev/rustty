//! Shared helpers for the headed GTK live KATs (P7-W7 T2).
//!
//! `2f0b65346` replaced the terminal's `GtkGLArea` with the `RenderSurface`
//! widget, which displays the frames the render thread exports as a
//! `GdkTexture`. The live KATs used to read GDK's GLArea framebuffer back with
//! `glReadPixels` on the main thread; nothing loads a GL function table there
//! any more, so they now inspect the texture the `RenderSurface` built in its
//! snapshot, or render a widget to a texture through its window's GSK renderer.
//! Included by `#[path]` from each test binary; every helper is test-only.
//!
//! On a Linux host without a desktop session (rvm), run these KATs through the
//! canonical launcher, `.port/tools/gtk_live_headless_sway.sh <cargo test ...>`.
//! It starts a TILING headless sway (pixman, its own socket), and its header
//! explains each part of the recipe (P7-W7 T6).
#![allow(dead_code)]

use gtk4::gdk;
use gtk4::prelude::*;

use rustty_apprt::gtk;
use rustty_apprt::gtk::class::render_surface::RenderSurface;
use rustty_core::app::App as CoreApp;

/// One main-loop iteration, then one core-app tick — what `App::run`'s loop does
/// (`application.zig` `run`: `ctx.iteration(true)` then `core_app.tick`). The
/// tick drains the core app's mailbox, which is how a render thread's `.redraw`
/// reaches its `RenderSurface` (`Surface.redraw` → `.render` → `queue_draw`); a
/// KAT that only iterates the GTK context never sees a second frame, and the
/// renderer's `.forever` push stalls once the queue fills.
pub fn iterate(ctx: &glib::MainContext, core_app: *mut CoreApp<gtk::App>, app: &gtk::App) {
    ctx.iteration(true);
    // SAFETY: `core_app` is the test's live boxed core app; the tick runs on the
    // main thread, as `App::run`'s does.
    if let Err(err) = unsafe { CoreApp::<gtk::App>::tick(core_app, app) } {
        panic!("core app tick failed: {err:?}");
    }
}

/// Stop and join a live surface's render thread, for a KAT that ends without
/// tearing its surfaces down (the out-of-scope termio `Subprocess::drop` gap,
/// `gtk_first_light.rs` section 6). Since `2f0b65346` the render thread draws
/// with its own EGL context; left running, it can still be inside libEGL when the
/// process exits and the driver's exit-time teardown runs, which crashes the test
/// binary after the test has passed. The render thread's `threadExit` releases
/// its GPU resources and its context on the way out.
///
/// # Safety
/// `core` is the widget's live core surface; the main thread calls this once.
pub unsafe fn stop_render_thread(core: *mut rustty_apprt::gtk::class::surface::CoreSurface) {
    unsafe {
        let _ = (*core).renderer_thread.stop.notify();
        if let Some(handle) = (*core).renderer_thr.take() {
            handle.join().expect("the render thread exits cleanly");
        }
    }
}

/// Stops and joins the render threads of `surfaces` when dropped, on the pass
/// path AND when an assertion panics (P7-W7 T6, N450).
///
/// A KAT that stops its render threads with a trailing [`stop_render_thread`]
/// skips that call when an assertion panics. libtest then catches the panic and
/// calls `exit`, and the render threads are still drawing when the driver's
/// exit-time teardown runs. On rvm (Mesa llvmpipe) that crashed the binary
/// with SIGSEGV inside `glReadPixels` (`OpenGL::present` → `read_pixels`,
/// on a `renderer` thread) after the test had already reported its failure.
///
/// Declare the guard AFTER the core app and the apprt app, so that unwinding
/// drops it first, while the core app is still alive. While it waits for a
/// thread to exit, it drains the core-app mailbox. A render thread blocked on a
/// `.forever` push to a full queue (`frameCompleted`) can therefore still see
/// its stop. The drop never panics, because a panic during unwinding aborts.
pub struct RenderThreadGuard {
    core_app: *mut CoreApp<gtk::App>,
    surfaces: Vec<rustty_apprt::gtk::class::surface::Surface>,
}

impl RenderThreadGuard {
    /// `core_app` is the test's live boxed core app, and it outlives the guard.
    pub fn new(
        core_app: *mut CoreApp<gtk::App>,
        surfaces: impl IntoIterator<Item = rustty_apprt::gtk::class::surface::Surface>,
    ) -> Self {
        Self {
            core_app,
            surfaces: surfaces.into_iter().collect(),
        }
    }
}

impl Drop for RenderThreadGuard {
    fn drop(&mut self) {
        for surface in &self.surfaces {
            let Some(core) = surface.core() else {
                continue;
            };
            // SAFETY: `core` is the widget's live core surface (the KATs never
            // finalize one), and the main thread is the only caller.
            // `renderer_thr.take()` makes a second stop a no-op.
            unsafe {
                let _ = (*core).renderer_thread.stop.notify();
                let Some(handle) = (*core).renderer_thr.take() else {
                    continue;
                };
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                while !handle.is_finished() && std::time::Instant::now() < deadline {
                    // SAFETY: the core app outlives the guard (see the type docs).
                    while (*self.core_app).mailbox.pop().is_some() {}
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                if handle.is_finished() {
                    if handle.join().is_err() {
                        eprintln!("RenderThreadGuard: a render thread panicked");
                    }
                } else {
                    eprintln!("RenderThreadGuard: a render thread did not stop within 10 s");
                }
            }
        }
    }
}

/// Find the first descendant of `widget` (itself included) of type `T`. The
/// template children are private; the KATs walk the tree by type.
pub fn find_widget<T: IsA<gtk4::Widget>>(widget: &gtk4::Widget) -> Option<T> {
    if let Ok(found) = widget.clone().downcast::<T>() {
        return Some(found);
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        if let Some(found) = find_widget::<T>(&c) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

/// The terminal `RenderSurface` inside a `Surface` widget tree.
pub fn find_render_surface(widget: &gtk4::Widget) -> Option<RenderSurface> {
    find_widget::<RenderSurface>(widget)
}

/// A texture's pixels as tight straight-alpha RGBA rows (`width * 4` bytes
/// each), top row first, as GDK stores texture rows.
pub fn texture_rgba(texture: &gdk::Texture) -> (usize, usize, Vec<u8>) {
    let mut downloader = gdk::TextureDownloader::new(texture);
    downloader.set_format(gdk::MemoryFormat::R8g8b8a8);
    let (bytes, stride) = downloader.download_bytes();
    let w = usize::try_from(texture.width()).expect("texture width");
    let h = usize::try_from(texture.height()).expect("texture height");
    let mut out = Vec::with_capacity(w * h * 4);
    for row in 0..h {
        let start = row * stride;
        out.extend_from_slice(&bytes[start..start + w * 4]);
    }
    (w, h, out)
}

/// Render `widget` to a texture through its window's GSK renderer, the way GTK
/// composites it (a GLArea contributes its GL texture node). `None` until the
/// widget has a size and a realized native.
pub fn render_widget_rgba(widget: &gtk4::Widget) -> Option<(usize, usize, Vec<u8>)> {
    let (w, h) = (widget.width(), widget.height());
    if w <= 0 || h <= 0 {
        return None;
    }
    let native = widget.native()?;
    let renderer = native.renderer()?;
    let paintable = gtk4::WidgetPaintable::new(Some(widget));
    let snapshot = gtk4::Snapshot::new();
    paintable.snapshot(&snapshot, f64::from(w), f64::from(h));
    let node = snapshot.to_node()?;
    let texture = renderer.render_texture(&node, None);
    Some(texture_rgba(&texture))
}

/// Count the bytes that are not zero.
pub fn nonzero_bytes(rgba: &[u8]) -> usize {
    rgba.iter().filter(|&&b| b != 0).count()
}

/// Whether the texture is a DMA-BUF import (`GdkDmabufTexture`) or CPU memory
/// (`GdkMemoryTexture`).
pub fn texture_kind(texture: &gdk::Texture) -> &'static str {
    if texture.is::<gdk::DmabufTexture>() {
        "dmabuf"
    } else if texture.is::<gdk::MemoryTexture>() {
        "memory"
    } else {
        "other"
    }
}

/// Write a binary PPM (P6) from tight RGBA rows (alpha dropped), top row first.
pub fn write_ppm(path: &str, rgba: &[u8], w: usize, h: usize) {
    use std::io::Write as _;
    let mut out = Vec::with_capacity(w * h * 3 + 32);
    write!(out, "P6\n{w} {h}\n255\n").expect("ppm header");
    for px in rgba.chunks_exact(4) {
        out.extend_from_slice(&px[..3]);
    }
    std::fs::write(path, out).expect("write the ppm");
}
