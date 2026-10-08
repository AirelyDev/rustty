//! The OpenGL device context.
//!
//! This holds the EGL display and config used to create EGL
//! contexts, each corresponding to a surface, its renderer
//! object and its render thread.
//!
//! TODO: If there's a way to prefer certain devices like in
//! Vulkan or Metal we should do it here.
//!
//! PORTS: ghostty `src/renderer/opengl/Device.zig` (new in `40d5b860d`), P8-W6 T2. The
//! app-scoped half of the port's former per-renderer `OffscreenContext`
//! (DECISIONS P12): the dlopen'd libEGL, the display, the config and the
//! `EGL_MESA_image_dma_buf_export` entry points. The per-renderer EGL context
//! stays in [`super::gl::sys::OffscreenContext`] ("Contexts are surface-scoped
//! because a context can only be current on one thread at a time",
//! `OpenGL.zig:63-64`).
//!
//! PORT NOTE (`init(self: *Device, alloc)`): the reference initializes in place
//! into `App.device`, which `App.init` leaves `undefined` until then. Rust has no
//! `undefined`, so `init` writes into a `MaybeUninit<Device>` slot and leaves it
//! uninitialized on error (the core's `DeviceBox` owns that slot, N522). The
//! unused `alloc` parameter (`_ = alloc;`) is dropped, as everywhere in this crate.
//!
//! PORT NOTE (`egl.load()`): glad's `gladLoadEGL` fills process-global function
//! pointers. The port dlopens `libEGL.so.1` into a `khronos_egl::DynamicInstance`
//! owned by the device, which every EGL call of every renderer goes through, so it
//! lives exactly as long as the display and config it produced.
//!
//! PORT NOTE (sharing across render threads): every surface renderer reads the
//! device through a `*const Device` (`Options.device`) and calls EGL through it on
//! its own render thread. That is sound: EGL 1.5 requires display-level calls to
//! be thread-safe (eglMakeCurrent, eglCreateContext, eglCreateImage, ... on one
//! display from many threads), the device's fields are written once by `init`,
//! before any renderer exists, and never written again, and the function
//! pointers are plain code addresses. `Device` holds raw EGL handles and is
//! therefore neither `Send` nor `Sync`; no `unsafe impl` is added because only the
//! raw `*const Device` crosses threads (inside the renderer the core moves to its
//! render thread under its own `Send` wrapper).

use std::mem::MaybeUninit;

use khronos_egl as egl;

use super::gl::sys::{self, ExportDmabufImageMesaFn, ExportDmabufImageQueryMesaFn};
use super::gl::{log_info, log_warn};

/// `EGL_PLATFORM_SURFACELESS_MESA` (`EGL_MESA_platform_surfaceless`).
///
/// PORT NOTE: not a ghostty constant — `khronos-egl` binds only core EGL, so the
/// extension enum is spelled out here. Value per the Khronos registry.
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

/// The errors [`Device::init`] returns: the reference's inferred set of
/// `egl.load` / `Display.initPlatform` / `Config.choose`.
pub type Error = sys::Error;

/// The OpenGL device context (`Device.zig`'s `Device = @This()`).
pub struct Device {
    /// The dlopen'd `libEGL.so.1` (the reference's `egl.load()`); see the module
    /// PORT NOTE. Must outlive `display`/`config`, so it is owned here.
    pub(crate) egl: egl::DynamicInstance<egl::EGL1_5>,

    /// The EGL display.
    ///
    /// Since we use the default display, this is torn down
    /// automagically by the OS.
    pub display: egl::Display,

    /// The EGL config used to create surface renderer contexts. Chosen
    /// once here so that all contexts share compatible capabilities.
    pub config: egl::Config,

    /// `eglExportDMABUFImageQueryMESA`, resolved once at [`Device::init`] as glad
    /// resolves it in `gladLoadEGL` (`vendor/glad/src/glad_egl.c:34-37`). `None`
    /// when `eglGetProcAddress` has no such entry point. `eglGetProcAddress`
    /// results are display-independent, so they are app-scoped too.
    pub(crate) export_dmabuf_query_mesa: Option<ExportDmabufImageQueryMesaFn>,
    /// `eglExportDMABUFImageMESA`; see `export_dmabuf_query_mesa`.
    pub(crate) export_dmabuf_mesa: Option<ExportDmabufImageMesaFn>,
}

impl Device {
    // pub fn init(self: *Device, alloc: Allocator) !void {
    /// Initialize the device in place. On error `self_` stays uninitialized.
    pub fn init(self_: &mut MaybeUninit<Device>) -> Result<(), Error> {
        // try egl.load();
        //
        // SAFETY: `load_required` dlopen's libEGL.so.1 and resolves the core
        // entry points. Unsafe only because it trusts the shared object to
        // actually be EGL.
        let egl = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|err| Error::EglUnavailable(err.to_string()))?;

        // const display: *egl.Display = try .initPlatform(
        //     egl.c.EGL_PLATFORM_SURFACELESS_MESA,
        //     egl.c.EGL_DEFAULT_DISPLAY,
        //     null,
        // );
        //
        // P8-W6 T2 (DECISIONS P12): the port used to pre-check the client
        // extensions and fail before `eglGetPlatformDisplay`. It now follows the
        // reference's order: try the platform display.
        let display = sys::init_platform(&egl, PLATFORM_SURFACELESS_MESA, egl::DEFAULT_DISPLAY)?;

        // log.info("EGL vendor={s}", .{display.queryString(.vendor) orelse "(unknown)"});
        // log.info("EGL extensions={s}", .{display.queryString(.extensions) orelse "(unknown)"});
        {
            let query = |name| {
                egl.query_string(Some(display), name)
                    .ok()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "(unknown)".to_owned())
            };
            log_info!("EGL vendor={}", query(egl::VENDOR));
            log_info!("EGL extensions={}", query(egl::EXTENSIONS));
        }

        // Choose a config. We need a config that is renderable with
        // OpenGL and a RGBA8 color buffer.
        // const config = egl.Config.choose(display, &.{ ... }) catch |err| {
        //     log.warn("failed to choose config err={}", .{err});
        //     return err;
        // };
        let config = choose_config(&egl, display).inspect_err(|err| {
            log_warn!("failed to choose config err={err:?}");
        })?;

        // `pub fn getProcAddress(name)` (`egl.zig:20-22`) through glad's
        // `load_EGL_MESA_image_dma_buf_export` (`glad_egl.c:34-37`).
        //
        // SAFETY (both transmutes): `eglGetProcAddress` returned these for exactly
        // these names, and the fn types are the extension's C prototypes
        // (`EGL_MESA_image_dma_buf_export`; `extern "system"` is the EGL ABI).
        let export_dmabuf_query_mesa =
            egl.get_proc_address("eglExportDMABUFImageQueryMESA")
                .map(|f| unsafe {
                    std::mem::transmute::<extern "system" fn(), ExportDmabufImageQueryMesaFn>(f)
                });
        let export_dmabuf_mesa = egl
            .get_proc_address("eglExportDMABUFImageMESA")
            .map(|f| unsafe {
                std::mem::transmute::<extern "system" fn(), ExportDmabufImageMesaFn>(f)
            });

        self_.write(Device {
            egl,
            display,
            config,
            export_dmabuf_query_mesa,
            export_dmabuf_mesa,
        });
        Ok(())
    }

    // pub fn deinit(self: *Device) void {
    /// Tear the device down. The display is left alone (DECISIONS B10).
    pub fn deinit(&mut self) {
        // Do not destroy the EGL display here as
        // it is shared across the entire process.
        // It will get automatically torn down by the OS.
        //
        // (`self.* = undefined;` — the owner frees the allocation right after,
        // `rustty_core::app::DeviceBox::deinit`; dropping the device dlcloses its
        // libEGL handle, never `eglTerminate`.)
    }

    /// Resolves an OpenGL entry point, for [`super::gl::glad::load`]
    /// (`gl.egl.getProcAddress`, `egl.zig:20-22`).
    pub fn get_proc_address(&self, name: &str) -> *const std::ffi::c_void {
        match self.egl.get_proc_address(name) {
            Some(f) => f as *const std::ffi::c_void,
            None => std::ptr::null(),
        }
    }

    /// Queries `EGL_VENDOR` or `EGL_EXTENSIONS` on the display.
    ///
    /// PORTS: `egl.zig:116-123` `Display.queryString`.
    pub fn query_string(&self, name: sys::StringQuery) -> Option<String> {
        let name = match name {
            sys::StringQuery::Vendor => egl::VENDOR,
            sys::StringQuery::Extensions => egl::EXTENSIONS,
        };
        self.egl
            .query_string(Some(self.display), name)
            .ok()
            .map(|s| s.to_string_lossy().into_owned())
    }

    /// Whether the display offers `EGL_MESA_image_dma_buf_export` AND both of its
    /// entry points resolved: the precondition of [`sys::Image::export_dmabuf_query`]
    /// and [`sys::Image::export_dmabuf`]. RUSTTY-ONLY (the DMA-BUF KATs' skip probe).
    pub fn has_dmabuf_export(&self) -> bool {
        self.export_dmabuf_query_mesa.is_some()
            && self.export_dmabuf_mesa.is_some()
            && self
                .query_string(sys::StringQuery::Extensions)
                .is_some_and(|exts| sys::has_extension(&exts, "EGL_MESA_image_dma_buf_export"))
    }
}

/// Picks an `EGL_OPENGL_BIT` RGBA8 config, with no surface type required
/// (`Device.zig:45-57`).
///
/// `EGL_SURFACE_TYPE = 0` requires no surface type bit at all, so every
/// `EGL_OPENGL_BIT` config matches whatever surfaces it can back. The attribute
/// is REQUIRED, not decoration: `eglChooseConfig` DEFAULTS `EGL_SURFACE_TYPE` to
/// `EGL_WINDOW_BIT` when it is left out (EGL 1.5 spec, table 3.4), so omitting it
/// silently asks for a WINDOW-capable config on a WINDOWLESS platform and can only
/// ever match zero configs.
///
/// MEASURED on the L4 development host, same surfaceless display (config counts
/// from `eglChooseConfig`):
///
/// | attribute list | virgl | llvmpipe |
/// |---|---|---|
/// | `[RENDERABLE_TYPE, OPENGL_BIT]` (implies `WINDOW_BIT`) | 0 | 0 |
/// | `[…, SURFACE_TYPE, WINDOW_BIT]` (the implied default, spelled out) | 0 | 0 |
/// | `[…, SURFACE_TYPE, PBUFFER_BIT]` | **35** | **105** |
///
/// `0` (the reference's value, `ab34b8b13`) is the end state because a driver
/// need not advertise pbuffers on a surfaceless display.
///
/// PORT NOTE: the config selects the contexts' pixel format only — each context
/// is made current with NO surface, and the renderer renders into an FBO whose
/// format it chooses itself. No surface is ever created.
fn choose_config(
    egl: &egl::DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
) -> Result<egl::Config, Error> {
    let attributes = [
        egl::SURFACE_TYPE,
        0,
        egl::RENDERABLE_TYPE,
        egl::OPENGL_BIT,
        egl::RED_SIZE,
        8,
        egl::GREEN_SIZE,
        8,
        egl::BLUE_SIZE,
        8,
        egl::ALPHA_SIZE,
        8,
        egl::NONE,
    ];
    egl.choose_first_config(display, &attributes)?
        .ok_or(Error::NoConfig)
}
