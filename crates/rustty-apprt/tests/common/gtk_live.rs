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
use rustty_apprt::gtk::class::surface::{CoreSurface, Surface};
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

/// Stops and joins the render threads of the surfaces it watches when it drops,
/// whether the test passed or an assertion panicked (P7-W7 T6, N450).
///
/// Since `2f0b65346` the render thread draws with its own EGL context. If a KAT
/// exits while that thread is still running, the driver's exit-time teardown
/// runs underneath it. On rvm (Mesa llvmpipe) a KAT whose assertion panicked
/// crashed at exit with SIGSEGV inside `glReadPixels` (`OpenGL::present` →
/// `read_pixels`, on a `renderer` thread), after libtest had already reported
/// the failure. The render thread's `threadExit` releases its GPU resources and
/// its context on the way out.
///
/// The guard holds WEAK references. A KAT that closes its surfaces normally
/// ([`close_surface`]: the GObject finalize → `CoreSurface::deinit`, which joins
/// both threads) leaves it nothing to do. A surface still alive when the guard
/// drops, because the KAT keeps it or an assertion unwound past its close, gets
/// its render thread stopped and joined here. So does every surface in a
/// toplevel still open when the guard drops (e.g. the app-created window).
///
/// Declare the guard AFTER the core app and the apprt app, so that unwinding
/// drops it first, while the core app is still alive. While it waits for a
/// thread to exit, it drains the core-app mailbox. A render thread blocked on a
/// `.forever` push to a full queue (`frameCompleted`) can therefore still see
/// its stop. The drop never panics, because a panic during unwinding aborts.
///
/// A surface whose render thread the guard joined must not be finalized
/// afterwards, because `CoreSurface::deinit` expects `renderer_thr` to still be
/// set. The guard acts only at the end of a test, and nothing finalizes a
/// surface after that.
pub struct RenderThreadGuard {
    core_app: *mut CoreApp<gtk::App>,
    surfaces: Vec<glib::WeakRef<Surface>>,
}

impl RenderThreadGuard {
    /// `core_app` is the test's live boxed core app, and it outlives the guard.
    pub fn new<'a>(
        core_app: *mut CoreApp<gtk::App>,
        surfaces: impl IntoIterator<Item = &'a Surface>,
    ) -> Self {
        Self {
            core_app,
            surfaces: surfaces.into_iter().map(|s| s.downgrade()).collect(),
        }
    }

    /// Watch one more surface (e.g. one the product code built later).
    pub fn watch(&mut self, surface: &Surface) {
        self.surfaces.push(surface.downgrade());
    }
}

impl Drop for RenderThreadGuard {
    fn drop(&mut self) {
        // The watched surfaces that are still alive (a finalized one's
        // `CoreSurface::deinit` already joined its threads), plus every surface
        // in a toplevel still open. The second set covers the window that
        // `App::run`'s `activate` creates (see [`close_other_windows`]), when an
        // assertion unwinds before that window is closed.
        let mut live: Vec<Surface> = self.surfaces.iter().filter_map(|w| w.upgrade()).collect();
        let toplevels = gtk4::Window::toplevels();
        for i in 0..toplevels.n_items() {
            if let Some(widget) = toplevels
                .item(i)
                .and_then(|o| o.downcast::<gtk4::Widget>().ok())
            {
                collect_surfaces(&widget, &mut live);
            }
        }
        for surface in live {
            let Some(core) = surface.core() else {
                continue;
            };
            // SAFETY: `core` is the widget's live core surface (the widget is
            // alive, so it has not been finalized), and the main thread is the
            // only caller. `renderer_thr.take()` makes a second stop a no-op.
            unsafe {
                let _ = (*(*core).renderer_thread.as_ptr()).stop.notify();
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

/// The pid of a live surface's shell (`io.backend` → `Exec.subprocess.process`).
///
/// # Safety
/// `core` points at a live, initialized core surface whose shell was started.
pub unsafe fn shell_pid(core: *mut CoreSurface) -> libc::pid_t {
    match unsafe { &(*(*core).io.as_ptr()).backend } {
        rustty_termio::Backend::Exec(exec) => match exec.subprocess.process.as_ref() {
            Some(rustty_termio::exec::Process::ForkExec(cmd)) => {
                cmd.pid.expect("the shell was forked")
            }
            _ => panic!("the shell runs as a fork/exec child"),
        },
    }
}

/// Whether `pid` no longer exists: it was killed AND reaped (a zombie still
/// answers `kill(pid, 0)`).
pub fn process_gone(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    let rc = unsafe { libc::kill(pid, 0) };
    rc == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
}

/// Close a live surface the way the app does, and prove it closed. See
/// [`close_surfaces`].
pub fn close_surface(
    ctx: &glib::MainContext,
    window: impl IsA<gtk4::Window>,
    surface: Surface,
    shell_pid: libc::pid_t,
) {
    close_surfaces(ctx, window, vec![(surface, shell_pid)]);
}

/// Close the live surfaces of one window the way the app does, and prove they
/// closed (P7-W7 T6).
///
/// The caller has already dropped every other strong reference it held (the
/// surfaces' widgets included), and passes the last reference to each surface,
/// its shell's pid, and the window that hosts them. This destroys the window,
/// drops the surfaces, and pumps the main loop until every `Surface` GObject is
/// finalized. Each finalize runs `deleteSurface`, then `CoreSurface::deinit`
/// (stop and join the render and io threads, kill and reap the shell), then
/// frees the core surface without drop glue.
///
/// It asserts, for every surface, that the finalize happened (no leaked
/// reference) and that its shell is gone (killed and reaped). It also asserts
/// that the window and every Rustty widget in it (the `Ghostty*` GTypes:
/// `Window`, `Tab`, `SplitTree`, `SurfaceScrolledWindow`, `RenderSurface`,
/// `SearchOverlay`, ...) were finalized, so a reference leaked on any of them
/// (a template-callback closure that keeps its widget, NEW-FINDINGS N516) is
/// RED here whatever a leak checker's suppressions hide (P7-W25).
pub fn close_surfaces(
    ctx: &glib::MainContext,
    window: impl IsA<gtk4::Window>,
    surfaces: Vec<(Surface, libc::pid_t)>,
) {
    let watched: Vec<(glib::WeakRef<Surface>, libc::pid_t)> = surfaces
        .into_iter()
        .map(|(surface, pid)| (surface.downgrade(), pid))
        .collect();
    let mut widgets: Vec<(String, glib::WeakRef<gtk4::Widget>)> = Vec::new();
    collect_rustty_widgets(
        window.upcast_ref::<gtk4::Window>().upcast_ref(),
        &mut widgets,
    );
    let window_weak = window.upcast_ref::<gtk4::Window>().downgrade();
    let window_type = window.type_().name().to_owned();
    window.upcast_ref::<gtk4::Window>().destroy();
    drop(window);
    let alive = || {
        watched.iter().any(|(weak, _)| weak.upgrade().is_some())
            || widgets.iter().any(|(_, weak)| weak.upgrade().is_some())
            || window_weak.upgrade().is_some()
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while alive() && std::time::Instant::now() < deadline {
        ctx.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    for (i, (weak, pid)) in watched.iter().enumerate() {
        assert!(
            weak.upgrade().is_none(),
            "surface {i} was not finalized after its window was destroyed (a leaked reference)"
        );
        assert!(
            process_gone(*pid),
            "surface {i}'s shell (pid {pid}) outlived the surface's close"
        );
    }
    assert!(
        window_weak.upgrade().is_none(),
        "the window ({window_type}) was not finalized after it was destroyed (a leaked reference)"
    );
    for (type_name, weak) in &widgets {
        assert!(
            weak.upgrade().is_none(),
            "a {type_name} was not finalized after its window was destroyed (a leaked reference)"
        );
    }
}

/// Weak references to every Rustty widget in `widget`'s tree (itself
/// included): the GTypes the port registers under the reference's names,
/// `Ghostty*` (and the reference's `GhostttySurfaceScrolledWindow`).
fn collect_rustty_widgets(
    widget: &gtk4::Widget,
    out: &mut Vec<(String, glib::WeakRef<gtk4::Widget>)>,
) {
    let name = widget.type_().name();
    if name.starts_with("Ghostt") {
        out.push((name.to_owned(), widget.downgrade()));
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        collect_rustty_widgets(&c, out);
        child = c.next_sibling();
    }
}

/// Every `Surface` in `widget`'s tree (itself included).
fn collect_surfaces(widget: &gtk4::Widget, out: &mut Vec<Surface>) {
    if let Ok(surface) = widget.clone().downcast::<Surface>() {
        out.push(surface);
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        collect_surfaces(&c, out);
        child = c.next_sibling();
    }
}

/// Close every toplevel window still open, the way [`close_surfaces`] closes
/// the KAT's own, and assert that each `Surface` inside was finalized and its
/// shell reaped.
///
/// `App::run` emits `activate`, and its `new_window_thunk` pushes a
/// `NewWindow`. A KAT that runs the loop to a scheduled quit can therefore own
/// a second, app-created window with its own terminal. That surface holds a
/// font-grid reference, which `CoreApp::deinit` asserts is gone, and its
/// threads would otherwise still run at exit (P7-W7 T6).
pub fn close_other_windows(ctx: &glib::MainContext) {
    let mut watched: Vec<(glib::WeakRef<Surface>, Option<libc::pid_t>)> = Vec::new();
    let toplevels = gtk4::Window::toplevels();
    let windows: Vec<gtk4::Window> = (0..toplevels.n_items())
        .filter_map(|i| toplevels.item(i))
        .filter_map(|obj| obj.downcast::<gtk4::Window>().ok())
        .collect();
    drop(toplevels);
    for window in windows {
        let mut surfaces = Vec::new();
        collect_surfaces(window.upcast_ref::<gtk4::Widget>(), &mut surfaces);
        for surface in surfaces {
            // SAFETY: a live widget's published core surface, main thread.
            let pid = surface.core().map(|core| unsafe { shell_pid(core) });
            watched.push((surface.downgrade(), pid));
        }
        window.destroy();
    }
    let alive = || watched.iter().any(|(weak, _)| weak.upgrade().is_some());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while alive() && std::time::Instant::now() < deadline {
        ctx.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    for (i, (weak, pid)) in watched.iter().enumerate() {
        assert!(
            weak.upgrade().is_none(),
            "app-created surface {i} was not finalized after its window was destroyed"
        );
        if let Some(pid) = pid {
            assert!(
                process_gone(*pid),
                "app-created surface {i}'s shell (pid {pid}) outlived its close"
            );
        }
    }
    if !watched.is_empty() {
        eprintln!(
            "close_other_windows: closed {} app-created surface(s)",
            watched.len()
        );
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
    // By GType name: gtk4-rs exposes `gdk::DmabufTexture` on Linux only, and
    // GTK has the type on every platform (FreeBSD included).
    if texture.type_().name() == "GdkDmabufTexture" {
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
