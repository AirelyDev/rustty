//! rustty-apprt::gtk::class::render_surface
//!
//! PORTS: ghostty `src/apprt/gtk/class/render_surface.zig` (461 loc, 0 `test{}`;
//! new at `12752b2ac`: `2f0b65346` plus the fractional-scaling series, R2 §3-4).
//! `GhosttyRenderSurface`, a plain `gtk.Widget` subclass that displays the
//! frames the renderer exports: a `GdkDmabufTexture` when the renderer's DMA-BUF
//! imports, a `GdkMemoryTexture` (CPU readback) when it does not.

use std::cell::{Cell, RefCell};

use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;

use rustty_renderer::generic::ExportedFrame;

use super::surface::CoreSurface;
use crate::gtk::scale;

// const log = std.log.scoped(.gtk_render_surface);
const LOG_SCOPE: &str = "gtk_render_surface";

glib::wrapper! {
    /// A widget that displays the rendered output of the Rustty renderer.
    ///
    /// In the past, Rustty relied on GTK's builtin `GLArea` to display the
    /// rendered output within the GTK-based UI, but this had numerous
    /// debilitating limitations.
    ///
    /// Its most significant limitation is that GTK's render lifecycle lives
    /// **exclusively on the main UI thread**, from initialization to drawing the
    /// rendered framebuffers. This is not only inefficient but also error-prone,
    /// as we offload our render work to a dedicated render thread which has
    /// to be careful not to access the renderer at the same time as the main
    /// thread. Initialization is also far more complex as we had to wait for
    /// GTK's OpenGL context to initialize, then we can proceed with initializing
    /// the renderer and then the core surface. (The GTK surface widget could not
    /// assume it always has a valid core surface, which is obviously absurd!)
    /// It was a messy, chicken-and-egg relationship that held us back a lot.
    ///
    /// With our custom `RenderSurface`, the renderer instead renders frames,
    /// exports them into DMABUFs, then pushes them into a queue that this widget
    /// can consume at its own pace on the main thread, whenever GTK calls for it
    /// to be snapshotted. This also allowed us to do triple-buffering and should
    /// greatly reduce tearing. Initialization is done on the render thread and
    /// always completes before the core surface is created. Furthermore, this
    /// surface is actually OpenGL-agnostic, since it operates exclusively on
    /// DMABUFs which can also be produced by other graphics APIs like Vulkan.
    pub struct RenderSurface(ObjectSubclass<imp::RenderSurface>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

pub mod imp {
    use super::*;

    #[derive(Default)]
    pub struct RenderSurface {
        /// The core surface, used to pull presents from the renderer.
        /// Set by the apprt Surface when it realizes the render surface.
        //
        // PORT NOTE: `?*CoreSurface` → a raw pointer (null for none); the widget
        // does not own it (the apprt `Surface` does, `surface.rs` `core_surface`).
        pub core_surface: Cell<Option<std::ptr::NonNull<CoreSurface>>>,

        /// The GDK Texture currently being displayed.
        pub texture: RefCell<Option<gdk::Texture>>,

        /// `notify::scale` handler id on `scale_surface`.
        //
        // PORT NOTE: `c_ulong` (0 for none) → `Option<SignalHandlerId>`.
        pub scale_notify_id: RefCell<Option<glib::SignalHandlerId>>,

        /// GdkSurface we connected; ref'd so a replacement cannot dangle the handler.
        pub scale_surface: RefCell<Option<gdk::Surface>>,

        /// One-shot so we log the first real allocation, not every size-allocate.
        pub scale_logged_size: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RenderSurface {
        // `.name = "GhosttyRenderSurface"` — the GType name is kept (the
        // `GhosttySurface` precedent; the templates name it).
        const NAME: &'static str = "GhosttyRenderSurface";
        type Type = super::RenderSurface;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for RenderSurface {
        fn signals() -> &'static [glib::subclass::Signal] {
            use std::sync::OnceLock;
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                // pub const resize = struct { ... defineSignal(name, Self, &.{ c_int, c_int }, void) };
                vec![glib::subclass::Signal::builder("resize")
                    .param_types([i32::static_type(), i32::static_type()])
                    .build()]
            })
        }

        fn dispose(&self) {
            // PORT NOTE (Rust-only): the reference relies on `unrealize` for the
            // handler; a disposed-but-never-unrealized widget would otherwise keep
            // its `notify::scale` connection on a live `GdkSurface`.
            self.obj().disconnect_scale_notify();
        }
    }

    impl WidgetImpl for RenderSurface {
        fn realize(&self) {
            // Call the parent realize.
            self.parent_realize();

            // Request a draw in case frames were produced before we
            // realized. The snapshot will pull from the renderer's queue.
            let obj = self.obj();
            obj.connect_scale_notify();
            obj.queue_draw();
        }

        fn unrealize(&self) {
            let obj = self.obj();
            obj.disconnect_scale_notify();

            // if (priv.texture) |tex| { tex.unref(); priv.texture = null; }
            self.texture.replace(None);

            self.parent_unrealize();
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let obj = self.obj();
            obj.connect_scale_notify();
            obj.emit_device_resize(width, height);

            self.parent_size_allocate(width, height, baseline);

            if !self.scale_logged_size.get() && width > 0 && height > 0 {
                obj.log_scale("allocated", width, height);
                self.scale_logged_size.set(true);
            }
        }

        fn snapshot(&self, snap: &gtk4::Snapshot) {
            self.obj().snapshot_frame(snap);
        }
    }
}

impl Default for RenderSurface {
    fn default() -> Self {
        glib::Object::new()
    }
}

/// TEST-ONLY fault injection (feature `render-surface-fault-inject`): while set,
/// the DMA-BUF texture is built with fourcc 0, which no display supports, so GDK
/// itself rejects it and the real `DmabufBuildFailed` path runs.
#[cfg(feature = "render-surface-fault-inject")]
pub static FORCE_DMABUF_IMPORT_FAILURE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// `rebuildTexture`'s error set: `error.DmabufBuildFailed` (the one the
/// snapshot handler switches on) plus the reference's allocator errors, which
/// the port's infallible allocation collapses.
#[derive(Debug)]
pub enum RebuildError {
    /// GTK could not import the DMA-BUF (`gdk_dmabuf_texture_builder_build`
    /// returned null).
    DmabufBuildFailed,
}

impl RenderSurface {
    /// The snapshot handler (`snapshot`, `render_surface.zig:149-212`).
    fn snapshot_frame(&self, snap: &gtk4::Snapshot) {
        let imp = self.imp();

        // Take the latest present from the renderer's queue, if any, and
        // build a texture from it. `take` closes any present that was
        // still unconsumed in the queue, but not the texture we're
        // currently displaying.
        if let Some(core) = imp.core_surface.get() {
            let core = core.as_ptr();
            // SAFETY: the apprt `Surface` sets `core_surface` to its live core
            // surface and clears it before freeing it; the latest-frame handle is
            // read raw (N437: no reference to the renderer is formed).
            let frame = unsafe {
                rustty_core::renderer_seam::Renderer::takeFrame(&raw const (*core).renderer)
            };
            if let Some(frame) = frame {
                // PORT NOTE: the reference's catch-all `else` arm (its
                // `error building texture from frame` warn, :171) takes only
                // `OutOfMemory` (the `Planes`/`MemoryHolder` `alloc.create`);
                // Rust aborts on OOM, so `RebuildError` has no other variant
                // (LOG-ABSENT-SITES.tsv).
                match self.rebuild_texture(frame) {
                    Ok(()) => {}
                    // GTK can't import our DMABUFs, e.g. due to an
                    // incompatible format modifier on multi-GPU setups.
                    // Report unhealthy presentation health so the
                    // renderer presents via CPU readback instead.
                    Err(RebuildError::DmabufBuildFailed) => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "failed to import dmabuf, asking renderer for CPU presentation"
                        );
                        // SAFETY: as above; `reportPresentationHealth` is raw-self.
                        unsafe {
                            CoreSurface::reportPresentationHealth(
                                core,
                                rustty_renderer::Health::Unhealthy,
                            )
                        };
                    }
                }
            }
        }

        // If we have a texture, draw it.
        let Some(texture) = imp.texture.borrow().clone() else {
            return;
        };

        let widget: &gtk4::Widget = self.upcast_ref();
        if widget.width() == 0 || widget.height() == 0 {
            return;
        }

        // Map one texture texel to one device pixel. Widget CSS size would
        // stretch a fractionally scaled buffer. Snap the surface-relative
        // origin so parent chrome cannot place the texture between pixels.
        let layout = scale::widget_layout(widget);
        let origin = layout.snap_origin();
        let scale = layout.scale as f32;
        let css_w = texture.width() as f32 / scale;
        let css_h = texture.height() as f32 / scale;

        // GTK, Metal, Vulkan, etc. all have their origin points in the
        // top left, but not OpenGL!
        let flip = !rustty_renderer::generic::CUSTOM_SHADER_Y_IS_DOWN;
        let bounds = texture_bounds(flip, origin, css_w, css_h);
        if flip {
            snap.save();
        }
        if flip {
            snap.translate(&graphene::Point::new(
                bounds.translate.0,
                bounds.translate.1,
            ));
            snap.scale(1.0, -1.0);
        }
        snap.append_texture(
            &texture,
            &graphene::Rect::new(bounds.x, bounds.y, css_w, css_h),
        );
        if flip {
            snap.restore();
        }
    }

    /// Return the size of this surface in device pixels. Used by the
    /// apprt surface to report the size to the renderer.
    pub fn device_size(&self) -> scale::DeviceSize {
        scale::widget_layout(self.upcast_ref()).size
    }

    fn emit_device_resize(&self, css_w: i32, css_h: i32) {
        let size = scale::widget_layout_for_size(self.upcast_ref(), css_w, css_h).size;
        if size.width == 0 || size.height == 0 {
            return;
        }
        // PORT NOTE (R7): `@intCast(size.width)` into `c_int` traps above
        // `i32::MAX`.
        let w = i32::try_from(size.width).expect("device width exceeds c_int");
        let h = i32::try_from(size.height).expect("device height exceeds c_int");
        self.emit_by_name::<()>("resize", &[&w, &h]);
    }

    fn connect_scale_notify(&self) {
        let Some(native) = self.native() else {
            return;
        };
        let Some(surface) = native.surface() else {
            return;
        };

        let imp = self.imp();
        if imp.scale_notify_id.borrow().is_some() {
            if imp.scale_surface.borrow().as_ref() == Some(&surface) {
                return;
            }
            self.disconnect_scale_notify();
        }

        // _ = surface.as(gobject.Object).ref(); priv.scale_surface = surface;
        imp.scale_surface.replace(Some(surface.clone()));
        imp.scale_logged_size.set(false);
        let weak = self.downgrade();
        let id = surface.connect_notify_local(Some("scale"), move |_surface, _pspec| {
            if let Some(this) = weak.upgrade() {
                this.on_scale_notify();
            }
        });
        imp.scale_notify_id.replace(Some(id));
        let widget: &gtk4::Widget = self.upcast_ref();
        self.log_scale("connected", widget.width(), widget.height());
    }

    fn disconnect_scale_notify(&self) {
        let imp = self.imp();
        let Some(id) = imp.scale_notify_id.take() else {
            return;
        };
        if let Some(surface) = imp.scale_surface.take() {
            surface.disconnect(id);
            // (`surface.unref()` — the `take()` drops our ref.)
        }
    }

    fn on_scale_notify(&self) {
        let widget: &gtk4::Widget = self.upcast_ref();
        self.log_scale("changed", widget.width(), widget.height());
        self.emit_device_resize(widget.width(), widget.height());
        widget.queue_draw();
    }

    fn log_scale(&self, event: &str, css_width: i32, css_height: i32) {
        let widget: &gtk4::Widget = self.upcast_ref();
        let Some(native) = widget.native() else {
            return;
        };
        let Some(surface) = native.surface() else {
            return;
        };
        let layout = scale::widget_layout_for_size(widget, css_width, css_height);

        log::debug!(
            target: LOG_SCOPE,
            "surface scale {event}: css={css_width}x{css_height} device={}x{} surface={} effective={} surface_factor={} widget_factor={}",
            layout.size.width,
            layout.size.height,
            surface.scale(),
            layout.scale,
            surface.scale_factor(),
            widget.scale_factor(),
        );
    }

    /// Set the core surface to pull presents from. Nothing will
    /// render when this is unset.
    pub fn set_core_surface(&self, core: *mut CoreSurface) {
        self.imp().core_surface.set(std::ptr::NonNull::new(core));
    }

    /// The texture the last snapshot displayed, for the live tests.
    pub fn texture(&self) -> Option<gdk::Texture> {
        self.imp().texture.borrow().clone()
    }

    /// Build a `GdkTexture` from the present and set it as our current
    /// texture, unrefing any previous texture.
    pub fn rebuild_texture(&self, frame: ExportedFrame) -> Result<(), RebuildError> {
        match frame {
            ExportedFrame::Dmabuf(dmabuf) => self.rebuild_dmabuf_texture(dmabuf),
            ExportedFrame::Memory(memory) => {
                self.rebuild_memory_texture(memory);
                Ok(())
            }
        }
    }

    /// Build a `GdkDmabufTexture` from the present and set it as our
    /// current texture, unrefing any previous texture.
    fn rebuild_dmabuf_texture(
        &self,
        frame: rustty_renderer::dmabuf::Dmabuf,
    ) -> Result<(), RebuildError> {
        let display = self.display();

        let mut builder = gdk::DmabufTextureBuilder::new()
            .set_display(&display)
            .set_width(frame.width)
            .set_height(frame.height)
            .set_fourcc({
                #[cfg(feature = "render-surface-fault-inject")]
                if FORCE_DMABUF_IMPORT_FAILURE.load(std::sync::atomic::Ordering::SeqCst) {
                    0
                } else {
                    frame.fourcc
                }
                #[cfg(not(feature = "render-surface-fault-inject"))]
                frame.fourcc
            })
            .set_modifier(frame.modifier)
            .set_premultiplied(frame.premultiplied)
            .set_n_planes(u32::from(frame.planes.count));

        for i in 0..usize::from(frame.planes.count) {
            let plane = u32::try_from(i).expect("plane index");
            let fd = frame.planes.fds[i]
                .as_ref()
                .map(std::os::fd::AsRawFd::as_raw_fd)
                .expect("a validated plane owns its fd");
            // SAFETY: the fd stays open until GDK releases the texture: the
            // `Planes` that own it are handed to GDK below and closed only by
            // `dmabuf_destroy`, or by us when the build fails.
            builder = unsafe { builder.set_fd(plane, fd) };
            // PORT NOTE (R7): `@intCast` of the driver's `c_int` offset/stride.
            builder = builder.set_offset(
                plane,
                u32::try_from(frame.planes.offsets[i]).expect("negative plane offset"),
            );
            builder = builder.set_stride(
                plane,
                u32::try_from(frame.planes.strides[i]).expect("negative plane stride"),
            );
        }

        // Build the texture. We retain ownership over the DMABUF FDs
        // until the asynchronous texture building process is complete,
        // so we have to make a heap copy of them, send them to GTK,
        // and then destroy them once successful.
        //
        // PORT NOTE: reference defect (R13), UD353 (the `render_surface.zig`
        // half): `alloc.create(Planes)` sits before its `errdefer`, so an
        // allocation failure there leaks every plane fd. Here the planes move
        // into a `Box` whose allocation cannot fail without aborting, and an
        // `OwnedFd` closes itself wherever it is dropped, so no path leaks.
        let planes: *mut rustty_renderer::dmabuf::Planes = Box::into_raw(Box::new(frame.planes));

        let mut err: *mut glib::ffi::GError = std::ptr::null_mut();
        // SAFETY: `builder` is a live `GdkDmabufTextureBuilder`; `planes` is a
        // leaked `Box` that `dmabuf_destroy` reclaims exactly once when GDK
        // releases a texture it built. GDK only takes the destroy notify on
        // success (`gdk_dmabuf_texture_builder_build` returns null without
        // calling it on failure; `render_surface_dmabuf_build_failure_skips_the_destroy_notify`
        // checks this), so the failure arm reclaims the box itself.
        let texture = unsafe {
            gdk::ffi::gdk_dmabuf_texture_builder_build(
                builder.as_ptr(),
                Some(dmabuf_destroy),
                planes.cast(),
                &mut err,
            )
        };
        if texture.is_null() {
            // errdefer planes.deinit(); errdefer alloc.destroy(planes);
            // SAFETY: GDK did not take the notify; this is the only owner.
            drop(unsafe { Box::from_raw(planes) });
            if !err.is_null() {
                // SAFETY: `err` is a GError GDK set; `from_glib_full` frees it.
                let err: glib::Error = unsafe { glib::translate::from_glib_full(err) };
                // log.warn("failed to build dmabuf err={s}", .{err.f_message orelse "(unknown)"});
                log::warn!(target: LOG_SCOPE, "failed to build dmabuf err={}", err.message());
            }
            return Err(RebuildError::DmabufBuildFailed);
        }

        // SAFETY: a non-null, transfer-full `GdkTexture`.
        let texture: gdk::Texture = unsafe { glib::translate::from_glib_full(texture) };

        // Swap out the old texture.
        self.imp().texture.replace(Some(texture));
        Ok(())
    }

    /// Build a `GdkMemoryTexture` from a CPU memory frame and set it as
    /// our current texture, unrefing any previous texture. This is the
    /// fallback path for drivers that can't export DMA-BUFs.
    //
    // PORT NOTE: the reference's `MemoryHolder` + `glib.Bytes.newWithFreeFunc` +
    // `memoryDestroy` hand GTK the pixels and free them when GTK releases the
    // `GBytes`; `glib::Bytes::from_owned` is that, with the `Vec` as the holder.
    // Its allocation failure arm (`frame.deinit()` before returning) collapses
    // with the infallible allocation.
    fn rebuild_memory_texture(&self, frame: rustty_renderer::opengl::opengl_impl::Memory) {
        let stride = usize::try_from(frame.width).expect("width") * 4; // stride
        let width = i32::try_from(frame.width).expect("texture width exceeds c_int");
        let height = i32::try_from(frame.height).expect("texture height exceeds c_int");

        // Holder for the pixel data backing a `GdkMemoryTexture`. GTK
        // ref-counts the `GBytes` we hand it and calls `memoryDestroy`
        // when it no longer needs the data, at which point we can return
        // the pixels to the allocator.
        //
        // PORT NOTE: `glib::Bytes::from_owned` is the reference's
        // `MemoryHolder` + `Bytes.newWithFreeFunc` + `memoryDestroy`: it moves
        // the `Vec` into a boxed holder whose free func (the destroy callback
        // for the `GBytes` backing a `GdkMemoryTexture`) drops it and returns
        // the pixels to the allocator. We own the pixels; make sure they don't
        // leak: they move into `Bytes` here, so every exit frees them.
        //
        // `GdkMemoryTexture` keeps a reference to the `GBytes` for as
        // long as it needs the pixel data, so we hand it our holder and
        // get the pixels freed via the destroy notify below.
        let bytes = glib::Bytes::from_owned(frame.pixels);
        let texture = gdk::MemoryTexture::new(
            width,
            height,
            gdk::MemoryFormat::R8g8b8a8Premultiplied,
            &bytes,
            stride,
        );

        // The texture holds its own reference to the bytes now.
        drop(bytes);

        // Swap out the old texture.
        self.imp().texture.replace(Some(texture.upcast()));
    }
}

/// Destroy callback for `GdkDmabufTexture`. GDK calls this when the
/// texture is released; we close the FDs and free the holder.
unsafe extern "C" fn dmabuf_destroy(data: glib::ffi::gpointer) {
    if data.is_null() {
        return;
    }
    // SAFETY: `data` is the `Box<Planes>` `rebuild_dmabuf_texture` leaked; GDK
    // calls this once. Dropping it closes every plane fd.
    drop(unsafe { Box::from_raw(data.cast::<rustty_renderer::dmabuf::Planes>()) });
}

/// Where the texture is drawn, in the snapshot's coordinates after the flip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureBounds {
    /// The `translate` applied before the `scale(1, -1)` flip.
    pub translate: (f32, f32),
    /// The texture rectangle's origin.
    pub x: f32,
    pub y: f32,
}

/// The flip transform and texture origin of the snapshot.
///
/// The reference (`render_surface.zig:188-205`):
///
/// ```zig
/// if (flip) {
///     snap.translate(&.{ .f_x = 0, .f_y = css_h });
///     snap.scale(1, -1);
/// }
///
/// snap.appendTexture(texture, &.{
///     .f_origin = .{ .f_x = @floatCast(origin.x), .f_y = @floatCast(origin.y) },
///     .f_size = .{ .f_width = css_w, .f_height = css_h },
/// });
/// ```
///
/// PORT NOTE: reference defect (R13), UD357. The snapped origin is appended
/// INSIDE the flip, so the flip negates its y: the texture's top lands at
/// widget y `-origin.y` instead of `+origin.y`, and its device top at
/// `2D - round(D)` for a device origin `D`, a half pixel off at `D = n.75`
/// (measured on rvm: a seam row above and below full-block glyphs at 1.25x).
/// The port moves the snapped y into the flip's translation and appends the
/// texture at y 0 inside it, which puts the top at `+origin.y` as the snap
/// intends; x is not flipped and keeps the reference's origin.
pub fn texture_bounds(
    flip: bool,
    origin: scale::CssPoint,
    _css_w: f32,
    css_h: f32,
) -> TextureBounds {
    if flip {
        TextureBounds {
            translate: (0.0, origin.y as f32 + css_h),
            x: origin.x as f32,
            y: 0.0,
        }
    } else {
        TextureBounds {
            translate: (0.0, 0.0),
            x: origin.x as f32,
            y: origin.y as f32,
        }
    }
}

/// The widget-space y of the texture's top edge for [`texture_bounds`]: the
/// flip maps texture-space y to `translate.1 - y`.
pub fn texture_top_in_widget(flip: bool, bounds: TextureBounds, css_h: f32) -> f32 {
    if flip {
        // The rectangle spans [y, y + css_h] before the flip; after it,
        // [translate - y - css_h, translate - y].
        bounds.translate.1 - bounds.y - css_h
    } else {
        bounds.y
    }
}

#[cfg(test)]
mod tests {
    use super::{texture_bounds, texture_top_in_widget, TextureBounds};
    use crate::gtk::scale::{snap_offset, CssPoint};

    /// UD357: at 1.25x a widget whose surface origin is 47 CSS px down sits at
    /// device 58.75; the snap moves it by +0.2 CSS px to device 59. The flipped
    /// OpenGL texture must land there too. The reference's transform puts its
    /// top at `-0.2` (device 58.5, a half pixel off); `render_surface_zig_bounds`
    /// is that transform, kept as the fail-first witness.
    #[test]
    fn flipped_texture_lands_on_the_snapped_origin() {
        let scale = 1.25;
        let origin = CssPoint {
            x: snap_offset(47.0, scale),
            y: snap_offset(47.0, scale),
        };
        let css_h = 400.0 / scale as f32;

        let top = texture_top_in_widget(true, texture_bounds(true, origin, 0.0, css_h), css_h);
        let device_top = (47.0 + f64::from(top)) * scale;
        assert!(
            (device_top - device_top.round()).abs() < 1e-4,
            "flipped texture top at device {device_top}"
        );

        // The reference's transform (the pre-fix port): half a device pixel off.
        let reference = render_surface_zig_bounds(origin, css_h);
        let top = texture_top_in_widget(true, reference, css_h);
        let device_top = (47.0 + f64::from(top)) * scale;
        assert!(
            ((device_top - device_top.floor()) - 0.5).abs() < 1e-4,
            "reference transform lands at device {device_top}"
        );

        // Unflipped (Metal-style): the origin is the reference's.
        let b = texture_bounds(false, origin, 0.0, css_h);
        assert_eq!(texture_top_in_widget(false, b, css_h), origin.y as f32);
    }

    /// `render_surface.zig:193-205` verbatim: translate(0, css_h), scale(1, -1),
    /// then the texture at the snapped origin.
    fn render_surface_zig_bounds(origin: CssPoint, css_h: f32) -> TextureBounds {
        TextureBounds {
            translate: (0.0, css_h),
            x: origin.x as f32,
            y: origin.y as f32,
        }
    }
}
