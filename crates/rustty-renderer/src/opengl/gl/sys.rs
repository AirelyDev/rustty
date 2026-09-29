//! The raw GL entry-point loader and the version/debug-output gate.
//!
//! PORTS: the context-preparation half of ghostty `src/renderer/OpenGL.zig` —
//! `MIN_VERSION_MAJOR`/`MIN_VERSION_MINOR` (`OpenGL.zig:36-38`),
//! `glDebugMessageCallback` (`OpenGL.zig:72-130`) and `prepareContext`
//! (`OpenGL.zig:132-159`). The remainder of `OpenGL.zig` (the `GraphicsAPI`
//! surface proper: `init`/`deinit`/`surfaceInit`/`threadEnter`/…) is Task 8.
//!
//! [`prepare_context`] is deliberately a FREE function over a proc-address
//! closure — exactly like the reference's `fn prepareContext(getProcAddress:
//! anytype)` — so Task 8's `surfaceInit` (`OpenGL.zig:169-170`, the
//! `apprt.gtk => try prepareContext(null)` arm) reuses it unchanged.
//!
//! [`OffscreenContext`] is RUSTTY-ONLY and has NO ghostty analogue: ghostty never
//! creates its own GL context (GDK/GLArea owns context creation, which is why
//! `prepareContext(null)` loads from the already-current global context). It
//! exists solely so the L4 offscreen tests can stand a context up with no X11,
//! Wayland or GTK in the loop, standing in for what GDK does in the windowed
//! path. It is the Rust analogue of "whatever made a context current before
//! `prepareContext` ran", not a port of anything.
//!
//! P7-W7 T1 (pin `12752b2ac`): upstream now DOES create its own surfaceless
//! context (`OpenGL.init`, `2f0b65346`), over `pkg/opengl/egl.zig` (`fa2ec3a6a`),
//! and never terminates its display. This module now also carries that EGL
//! half — [`EglError`], [`Image`], the `EGL_MESA_image_dma_buf_export` entry
//! points, the `EGL_SURFACE_TYPE = 0` config request, no `eglTerminate`
//! (DECISIONS B10) — and [`OffscreenContext`] stands in for the renderer-owned
//! display + context until stage S2 merges it into `OpenGL::init`. The GL function
//! table [`prepare_context`] loads is the calling thread's (`glad`, DECISIONS B9).
//!
//! PORT NOTE (`const log = std.log.scoped(.opengl);`, `OpenGL.zig:34`): the
//! scoped logger is reproduced by the `log_*!` macros in the parent module. (The
//! original note's "this crate has no log facade and R3 permits no new dependency
//! for one" is false as of task-14c-1 — see the CORRECTED paragraph on those
//! macros in `gl.rs`; routing them through `log::` is the OpenGL 14c stage's
//! work.) They
//! emit `std.log`'s own wire format (`error(opengl): …`), so every reference call
//! site ports as a one-for-one textual match. Task 1 declared them privately here
//! (this was then the only file in the package that logged); Task 3 hoisted them
//! to `gl.rs` unchanged, because `Shader.zig`/`Program.zig` declare that same
//! scoped logger and the reference shares ONE `.opengl` scope across the package.

use std::ffi::{c_char, c_int, c_void, CStr};
use std::fmt;

use khronos_egl as egl;

use super::{draw, errors, glad, log_debug, log_err, log_info, log_warn};

/// PORTS: `OpenGL.zig:36-38`.
///
/// ```zig
/// /// We require at least OpenGL 4.3
/// pub const MIN_VERSION_MAJOR = 4;
/// pub const MIN_VERSION_MINOR = 3;
/// ```
pub const MIN_VERSION_MAJOR: u32 = 4;
/// See [`MIN_VERSION_MAJOR`].
pub const MIN_VERSION_MINOR: u32 = 3;

/// `EGL_PLATFORM_SURFACELESS_MESA` (`EGL_MESA_platform_surfaceless`).
///
/// PORT NOTE: not a ghostty constant — `khronos-egl` binds only core EGL, so the
/// extension enum is spelled out here. Value per the Khronos registry.
const PLATFORM_SURFACELESS_MESA: egl::Enum = 0x31DD;

/// The errors [`OffscreenContext::init`] and [`prepare_context`] can produce.
///
/// PORT NOTE: `prepareContext` returns Zig's inferred error set, whose only
/// explicitly-returned member is `error.OpenGLOutdated` (`OpenGL.zig:148`); the
/// rest bubble up from `gl.glad.load`/`gl.enable`. [`Error::OpenGLOutdated`] is
/// that member; the others are this module's EGL-side additions (the context
/// creation ghostty delegates to GDK).
#[derive(Debug)]
pub enum Error {
    /// `libEGL.so.1` could not be dlopen'd, or is too old (EGL < 1.5).
    EglUnavailable(String),
    /// An EGL call failed.
    Egl(egl::Error),
    /// The EGL implementation lacks an extension this module requires.
    MissingEglExtension(&'static str),
    /// No pbuffer-capable `EGL_OPENGL_BIT` config exists on this display, i.e.
    /// the EGL implementation offers no desktop-GL pixel format at all.
    NoConfig,
    /// PORTS: `OpenGL.zig:148` `return error.OpenGLOutdated;`.
    OpenGLOutdated { major: u32, minor: u32 },
    /// `glGetString(GL_VERSION)` was unavailable or unparseable, i.e. the version
    /// CHECK itself could not run.
    VersionUnavailable,
    /// A GL entry point that the loaded context's own version guarantees was not
    /// resolved by [`glad::load`].
    ///
    /// Distinct from [`Error::VersionUnavailable`] on purpose: this cannot happen
    /// on a conformant context at or above the [`MIN_VERSION_MAJOR`].
    /// [`MIN_VERSION_MINOR`] floor, so it indicates a broken loader/port rather
    /// than a host that merely lacks 4.3 — which is why the offscreen test FAILS
    /// on it instead of skipping.
    MissingGlEntryPoint(&'static str),
    /// A GL call reported an error.
    ///
    /// PORT NOTE: carried `u32` (a raw `glGetError` code) while Task 1 was the
    /// only landed slice of `pkg/opengl`; now that the real
    /// [`errors::Error`] set exists (Task 2, the `pkg/opengl/errors.zig` port),
    /// it carries that.
    Gl(errors::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EglUnavailable(why) => write!(f, "EGL unavailable: {why}"),
            Self::Egl(err) => write!(f, "EGL error: {err}"),
            Self::MissingEglExtension(ext) => write!(f, "missing EGL extension: {ext}"),
            Self::NoConfig => f.write_str("no pbuffer-capable EGL_OPENGL_BIT config"),
            Self::OpenGLOutdated { major, minor } => write!(
                f,
                "OpenGL {major}.{minor} is too old; rustty requires OpenGL \
                 {MIN_VERSION_MAJOR}.{MIN_VERSION_MINOR}"
            ),
            Self::VersionUnavailable => f.write_str("could not determine the OpenGL version"),
            Self::MissingGlEntryPoint(name) => {
                write!(f, "the GL context did not provide {name}")
            }
            Self::Gl(err) => write!(f, "OpenGL error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<egl::Error> for Error {
    fn from(err: egl::Error) -> Self {
        Self::Egl(err)
    }
}

impl From<errors::Error> for Error {
    fn from(err: errors::Error) -> Self {
        Self::Gl(err)
    }
}

/// A headless, surfaceless EGL/OpenGL context, current on the creating thread.
///
/// RUSTTY-ONLY — see the module docs: ghostty has no analogue because GDK owns
/// context creation in the GTK path.
///
/// P7-W7 T1: it is also the stand-in, until the renderer's `init` owns one
/// (stage S2), for the `egl_display`/`egl_context` pair ghostty's `OpenGL`
/// holds (`OpenGL.zig:45-46`). The DMA-BUF export ([`Image`],
/// `opengl::target::Target::export_dmabuf`) takes it where the reference takes
/// that pair.
pub struct OffscreenContext {
    /// The dlopen'd `libEGL.so.1`. Must outlive `display`/`context`, so it is
    /// owned here rather than in a global.
    egl: egl::DynamicInstance<egl::EGL1_5>,
    display: egl::Display,
    context: egl::Context,
    /// `eglExportDMABUFImageQueryMESA`, resolved once at [`Self::init`] as glad
    /// resolves it in `gladLoadEGL` (`vendor/glad/src/glad_egl.c:34-37`). `None`
    /// when `eglGetProcAddress` has no such entry point.
    export_dmabuf_query_mesa: Option<ExportDmabufImageQueryMesaFn>,
    /// `eglExportDMABUFImageMESA`; see `export_dmabuf_query_mesa`.
    export_dmabuf_mesa: Option<ExportDmabufImageMesaFn>,
}

impl OffscreenContext {
    /// Stands up an OpenGL [`MIN_VERSION_MAJOR`].[`MIN_VERSION_MINOR`] core
    /// context with no surface, current on the calling thread.
    ///
    /// The sequence is display -> initialize -> bind_api -> config -> context ->
    /// `MakeCurrent(NO_SURFACE)`.
    ///
    /// The context is requested as **core** profile at exactly
    /// [`MIN_VERSION_MAJOR`].[`MIN_VERSION_MINOR`]. EGL treats
    /// `EGL_CONTEXT_MAJOR_VERSION`/`MINOR_VERSION` as a MINIMUM, so a driver that
    /// can do better returns better; one that cannot fails `EGL_BAD_MATCH` here
    /// rather than at [`prepare_context`]'s version gate. Both are a clean
    /// `Err`, never a panic.
    pub fn init() -> Result<Self, Error> {
        // SAFETY: `load_required` dlopen's libEGL.so.1 and resolves the core
        // entry points. Unsafe only because it trusts the shared object to
        // actually be EGL.
        let egl = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }
            .map_err(|err| Error::EglUnavailable(err.to_string()))?;

        // A surfaceless display needs no X11/Wayland/GBM native handle, which is
        // the whole point: the offscreen tests must run with no display server
        // and no DRM node. It is an extension even under EGL 1.5, so check for it
        // rather than let `get_platform_display` fail opaquely.
        let client_extensions = egl
            .query_string(None, egl::EXTENSIONS)
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !has_extension(&client_extensions, "EGL_MESA_platform_surfaceless") {
            return Err(Error::MissingEglExtension("EGL_MESA_platform_surfaceless"));
        }

        // SAFETY: unsafe because the native-display pointer must be valid for the
        // named platform. `EGL_MESA_platform_surfaceless` — just confirmed
        // present — is defined to take `EGL_DEFAULT_DISPLAY` and no native handle
        // at all, which is precisely what is passed.
        let display = unsafe {
            egl.get_platform_display(
                PLATFORM_SURFACELESS_MESA,
                egl::DEFAULT_DISPLAY,
                &[egl::ATTRIB_NONE],
            )
        }?;

        let (egl_major, egl_minor) = egl.initialize(display)?;
        log_debug!("initialized EGL {egl_major}.{egl_minor}");

        // P7-W7 T1 (DECISIONS B10): a failure from here on does NOT terminate the
        // display. Ghostty's `OpenGL.init` never terminates it on any path
        // (`OpenGL.zig:113-115`, "Do not destroy the EGL display here as it is
        // shared across the entire process."); see `Drop`.
        let context = Self::create_context(&egl, display)?;

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

        let this = Self {
            egl,
            display,
            context,
            export_dmabuf_query_mesa,
            export_dmabuf_mesa,
        };

        // NO_SURFACE draw/read: legal for a core context via EGL 1.5 /
        // EGL_KHR_surfaceless_context. The offscreen path renders into an FBO it
        // owns (Task 11), never into a window-system surface.
        this.egl
            .make_current(this.display, None, None, Some(this.context))?;

        Ok(this)
    }

    /// The `bind_api -> config -> create_context` half of [`Self::init`], split
    /// out so that its callers have a single display-terminating error path.
    fn create_context(
        egl: &egl::DynamicInstance<egl::EGL1_5>,
        display: egl::Display,
    ) -> Result<egl::Context, Error> {
        // Desktop GL, not GLES: `MIN_VERSION_*` is a desktop-GL core version, and
        // the default bound API is EGL_OPENGL_ES_API.
        egl.bind_api(egl::OPENGL_API)?;

        let config = Self::choose_config(egl, display)?;

        let attributes = [
            egl::CONTEXT_MAJOR_VERSION,
            MIN_VERSION_MAJOR as egl::Int,
            egl::CONTEXT_MINOR_VERSION,
            MIN_VERSION_MINOR as egl::Int,
            egl::CONTEXT_OPENGL_PROFILE_MASK,
            egl::CONTEXT_OPENGL_CORE_PROFILE_BIT,
            egl::NONE,
        ];

        Ok(egl.create_context(display, config, None, &attributes)?)
    }

    /// Picks an `EGL_OPENGL_BIT` RGBA8 config, with no surface type required.
    ///
    /// PORTS (P7-W7 T1, DECISIONS B10): the `egl.Config.choose` attribute list of
    /// ghostty's `OpenGL.init` (`OpenGL.zig:63-80`, from `ab34b8b13`):
    ///
    /// ```zig
    /// // Choose a config. We need a config that is renderable with
    /// // OpenGL and a RGBA8 color buffer.
    /// const config = egl.Config.choose(display, &.{
    ///     // EGL_SURFACE_TYPE defaults to EGL_WINDOW_BIT even though
    ///     // we are rendering exclusively through surfaceless mode.
    ///     // This is no problem on Mesa but we need to specify this
    ///     // explicitly for proprietary Nvidia drivers.
    ///     egl.c.EGL_SURFACE_TYPE,    0,
    ///     egl.c.EGL_RENDERABLE_TYPE, egl.c.EGL_OPENGL_BIT,
    ///     egl.c.EGL_RED_SIZE,        8,
    ///     egl.c.EGL_GREEN_SIZE,      8,
    ///     egl.c.EGL_BLUE_SIZE,       8,
    ///     egl.c.EGL_ALPHA_SIZE,      8,
    /// }) catch |err| { ... };
    /// ```
    ///
    /// `EGL_SURFACE_TYPE = 0` requires no surface type bit at all, so every
    /// `EGL_OPENGL_BIT` config matches whatever surfaces it can back. The port
    /// previously asked for `EGL_PBUFFER_BIT`, which the measurement below shows
    /// also works on Mesa; `0` is the end state because a driver need not
    /// advertise pbuffers on a surfaceless display. The history is kept because
    /// the table still explains why the attribute cannot be left out.
    ///
    /// HISTORY (`EGL_SURFACE_TYPE = EGL_PBUFFER_BIT`): the attribute is REQUIRED,
    /// not decoration. `eglChooseConfig` DEFAULTS `EGL_SURFACE_TYPE` to
    /// `EGL_WINDOW_BIT` when it is left out (EGL 1.5 spec, table 3.4), so omitting
    /// it silently asks for a WINDOW-capable config on a WINDOWLESS platform and
    /// can only ever match zero configs.
    ///
    /// MEASURED on the L4 development host, same surfaceless display (config
    /// counts from `eglChooseConfig`):
    ///
    /// | attribute list | virgl | llvmpipe |
    /// |---|---|---|
    /// | `[RENDERABLE_TYPE, OPENGL_BIT]` (implies `WINDOW_BIT`) | 0 | 0 |
    /// | `[…, SURFACE_TYPE, WINDOW_BIT]` (the implied default, spelled out) | 0 | 0 |
    /// | `[…, SURFACE_TYPE, PBUFFER_BIT]` (this) | **35** | **105** |
    ///
    /// So the surfaceless platform exposes plenty of `EGL_OPENGL_BIT` configs; the
    /// zero is a property of the QUERY, not of the platform or of a driver. An
    /// earlier revision of this file concluded the opposite and carried an
    /// `EGL_NO_CONFIG_KHR` fallback to work around it; that fallback was never
    /// load-bearing and is gone, along with the hard dependency it baked in on the
    /// OPTIONAL `EGL_KHR_no_config_context` extension.
    ///
    /// PORT NOTE: the config selects the context's pixel format only — the context
    /// is made current with NO surface ([`Self::init`]), and the offscreen path
    /// renders into an FBO whose format it chooses itself. No surface is ever
    /// created.
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

    /// Resolves an OpenGL entry point, for [`glad::load`].
    ///
    /// `eglGetProcAddress` is the GTK/Wayland-era analogue of the
    /// `getProcAddress` argument the reference threads into `gl.glad.load`
    /// (`OpenGL.zig:133`).
    pub fn get_proc_address(&self, name: &str) -> *const c_void {
        match self.egl.get_proc_address(name) {
            Some(f) => f as *const c_void,
            None => std::ptr::null(),
        }
    }

    /// Make the context current on the calling thread.
    ///
    /// PORTS: `egl.zig:106-113` `Display.makeCurrent(null, null, context)`, as
    /// `OpenGL.threadEnter` calls it (`OpenGL.zig:224`).
    pub fn make_current(&self) -> Result<(), EglError> {
        self.egl
            .make_current(self.display, None, None, Some(self.context))
            .map_err(EglError::from)
    }

    /// Release whatever context is current on the calling thread.
    ///
    /// PORTS: `egl.zig:115-117` `Display.releaseCurrent`, as `OpenGL.threadExit`
    /// calls it (`OpenGL.zig:233`). The reference discards the result.
    pub fn release_current(&self) {
        let _ = self.egl.make_current(self.display, None, None, None);
    }

    /// Queries `EGL_VENDOR` or `EGL_EXTENSIONS` on the display.
    ///
    /// PORTS: `egl.zig:98-104` `Display.queryString`.
    pub fn query_string(&self, name: StringQuery) -> Option<String> {
        let name = match name {
            StringQuery::Vendor => egl::VENDOR,
            StringQuery::Extensions => egl::EXTENSIONS,
        };
        self.egl
            .query_string(Some(self.display), name)
            .ok()
            .map(|s| s.to_string_lossy().into_owned())
    }

    /// Whether the display offers `EGL_MESA_image_dma_buf_export` AND both of its
    /// entry points resolved: the precondition of [`Image::export_dmabuf_query`]
    /// and [`Image::export_dmabuf`]. RUSTTY-ONLY (the DMA-BUF KAT's skip probe).
    pub fn has_dmabuf_export(&self) -> bool {
        self.export_dmabuf_query_mesa.is_some()
            && self.export_dmabuf_mesa.is_some()
            && self
                .query_string(StringQuery::Extensions)
                .is_some_and(|exts| has_extension(&exts, "EGL_MESA_image_dma_buf_export"))
    }
}

impl Drop for OffscreenContext {
    fn drop(&mut self) {
        // Release the context before destroying it: a context that is still
        // current is only marked for deletion by eglDestroyContext.
        //
        // The shape of `OpenGL.deinit` (`OpenGL.zig:109-117`):
        //     self.egl_display.releaseCurrent();
        //     self.egl_context.destroy(self.egl_display) catch {};
        let _ = self.egl.make_current(self.display, None, None, None);
        let _ = self.egl.destroy_context(self.display, self.context);

        // The GL function table is NOT unloaded here, as the reference's
        // `deinit` does not unload it: `glad.unload()` belongs to `threadExit`
        // (`OpenGL.zig:234`, `OpenGL::thread_exit`), and this thread's table may
        // have been loaded from another context.

        // // Do not destroy the EGL display here as
        // // it is shared across the entire process.
        // // It will get automatically torn down by the OS.
        //
        // P7-W7 T1 (DECISIONS B10): this used to call `eglTerminate`. The display
        // `eglGetPlatformDisplay` returns is the SAME one for every caller in the
        // process, and terminating it marks every resource on it for deletion, so
        // one context's teardown invalidated every other context on the display
        // (the hazard `gtk_first_light.rs` recorded when a separate context shared
        // GDK's display). The reference never terminates; neither does this.
    }
}

/// `const StringQuery = enum(c.EGLint) { vendor, extensions }` (`egl.zig:98-101`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StringQuery {
    /// `EGL_VENDOR`.
    Vendor,
    /// `EGL_EXTENSIONS`.
    Extensions,
}

/// PORTS: `egl.zig:24-40` — the `Error` set every EGL wrapper returns.
///
/// PORT NOTE: `khronos-egl`'s `egl::Error` carries the same fourteen codes; this
/// type adds the reference's `Unknown` (an EGL call failed and `eglGetError`
/// reported `EGL_SUCCESS`, `egl.zig:63-66` `mustError`) and a Rust-only
/// [`EglError::MissingEntryPoint`] for an extension entry point
/// `eglGetProcAddress` did not resolve (glad would leave the pointer null and the
/// call would jump through it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EglError {
    NotInitialized,
    BadAccess,
    BadAlloc,
    BadAttribute,
    BadContext,
    BadConfig,
    BadCurrentSurface,
    BadDisplay,
    BadSurface,
    BadMatch,
    BadParameter,
    BadNativePixmap,
    BadNativeWindow,
    ContextLost,
    Unknown,
    /// RUSTTY-ONLY; see the type docs.
    MissingEntryPoint(&'static str),
}

impl From<egl::Error> for EglError {
    /// PORTS: `egl.zig:42-61` `getError`'s switch.
    fn from(err: egl::Error) -> Self {
        match err {
            egl::Error::NotInitialized => Self::NotInitialized,
            egl::Error::BadAccess => Self::BadAccess,
            egl::Error::BadAlloc => Self::BadAlloc,
            egl::Error::BadAttribute => Self::BadAttribute,
            egl::Error::BadContext => Self::BadContext,
            egl::Error::BadConfig => Self::BadConfig,
            egl::Error::BadCurrentSurface => Self::BadCurrentSurface,
            egl::Error::BadDisplay => Self::BadDisplay,
            egl::Error::BadSurface => Self::BadSurface,
            egl::Error::BadMatch => Self::BadMatch,
            egl::Error::BadParameter => Self::BadParameter,
            egl::Error::BadNativePixmap => Self::BadNativePixmap,
            egl::Error::BadNativeWindow => Self::BadNativeWindow,
            egl::Error::ContextLost => Self::ContextLost,
        }
    }
}

impl fmt::Display for EglError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEntryPoint(name) => write!(f, "EGL entry point {name} is unavailable"),
            other => write!(f, "{other:?}"),
        }
    }
}

impl std::error::Error for EglError {}

/// PORTS: `egl.zig:63-66`.
///
/// ```zig
/// pub fn mustError() Error {
///     try getError();
///     return error.Unknown;
/// }
/// ```
fn must_error(egl: &egl::DynamicInstance<egl::EGL1_5>) -> EglError {
    egl.get_error().map_or(EglError::Unknown, EglError::from)
}

/// `PFNEGLEXPORTDMABUFIMAGEQUERYMESAPROC` (`EGL_MESA_image_dma_buf_export`).
pub type ExportDmabufImageQueryMesaFn = unsafe extern "system" fn(
    dpy: *mut c_void,
    image: *mut c_void,
    fourcc: *mut c_int,
    num_planes: *mut c_int,
    modifiers: *mut u64,
) -> egl::Boolean;

/// `PFNEGLEXPORTDMABUFIMAGEMESAPROC` (`EGL_MESA_image_dma_buf_export`).
pub type ExportDmabufImageMesaFn = unsafe extern "system" fn(
    dpy: *mut c_void,
    image: *mut c_void,
    fds: *mut c_int,
    strides: *mut egl::Int,
    offsets: *mut egl::Int,
) -> egl::Boolean;

/// The most planes an exported DMA-BUF can have: the extension's "<num_planes>
/// must be at most 4", which is also `Dmabuf.max_planes` (asserted equal below
/// wherever `crate::dmabuf` exists; this module also builds for Windows).
pub const MAX_DMABUF_PLANES: usize = 4;

#[cfg(unix)]
const _: () = assert!(MAX_DMABUF_PLANES == crate::dmabuf::MAX_PLANES);

/// PORTS: `egl.zig:267-271`.
///
/// ```zig
/// pub const DmabufQuery = struct {
///     fourcc: c_int,
///     num_planes: c_int,
///     modifier: u64,
/// };
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DmabufQuery {
    pub fourcc: c_int,
    pub num_planes: c_int,
    pub modifier: u64,
}

/// PORTS: `egl.zig:250-265` — `pub const ImageTarget = union(ImageTarget.Tag)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageTarget {
    /// OpenGL 2D Texture with the given name.
    Texture2d(u32),
    // Many other variants, add when needed
}

impl ImageTarget {
    /// `pub const Tag = enum(c.EGLenum) { texture_2d = c.EGL_GL_TEXTURE_2D }`.
    fn tag(self) -> egl::Enum {
        match self {
            Self::Texture2d(_) => egl::GL_TEXTURE_2D as egl::Enum,
        }
    }

    /// PORTS: `egl.zig:259-264` `toClientBuffer`.
    fn to_client_buffer(self) -> egl::ClientBuffer {
        match self {
            // Yes this really is that cursed
            //
            // SAFETY: for `EGL_GL_TEXTURE_2D` the "client buffer" IS the GL texture
            // name, passed as a pointer-sized integer (EGL_KHR_gl_texture_2D_image);
            // EGL never dereferences it.
            Self::Texture2d(v) => unsafe {
                egl::ClientBuffer::from_ptr(v as usize as egl::EGLClientBuffer)
            },
        }
    }
}

/// An `EGLImage`.
///
/// PORTS: `egl.zig:194-248` — `pub const Image = opaque { … }`.
///
/// PORT NOTE: the reference passes `display: *Display` to each method. The Rust
/// `khronos-egl` calls also need the dlopen'd instance, and the MESA entry points
/// are resolved per context, so the methods take the [`OffscreenContext`] that
/// owns all three.
#[derive(Debug)]
pub struct Image(egl::Image);

impl Image {
    /// PORTS: `egl.zig:195-210`.
    ///
    /// ```zig
    /// pub fn create(
    ///     display: *Display,
    ///     context: ?*Context,
    ///     target: ImageTarget,
    ///     attrs: ?[:c.EGL_NONE]const c.EGLAttrib,
    /// ) Error!*Image {
    ///     const image = c.eglCreateImage(
    ///         display,
    ///         context,
    ///         @intFromEnum(target),
    ///         target.toClientBuffer(),
    ///         if (attrs) |a| a.ptr else null,
    ///     ) orelse return mustError();
    ///
    ///     return @ptrCast(image);
    /// }
    /// ```
    ///
    /// PORT NOTE (`attrs`): `khronos-egl` requires the list to END with
    /// `EGL_NONE`, which the reference's sentinel-terminated slice also
    /// guarantees; pass `&[egl::ATTRIB_NONE]` for the reference's `&.{}`.
    pub fn create(
        ctx: &OffscreenContext,
        target: ImageTarget,
        attrs: &[egl::Attrib],
    ) -> Result<Image, EglError> {
        ctx.egl
            .create_image(
                ctx.display,
                ctx.context,
                target.tag(),
                target.to_client_buffer(),
                attrs,
            )
            .map(Image)
            .map_err(EglError::from)
    }

    /// PORTS: `egl.zig:212-216`.
    ///
    /// ```zig
    /// pub fn destroy(self: *Image, display: *Display) Error!void {
    ///     if (c.eglDestroyImage(display, self) != c.EGL_TRUE) {
    ///         return mustError();
    ///     }
    /// }
    /// ```
    pub fn destroy(self, ctx: &OffscreenContext) -> Result<(), EglError> {
        ctx.egl
            .destroy_image(ctx.display, self.0)
            .map_err(EglError::from)
    }

    /// PORTS: `egl.zig:218-230`.
    ///
    /// ```zig
    /// pub fn exportDmabufQuery(self: *Image, display: *Display) Error!DmabufQuery {
    ///     var query: DmabufQuery = undefined;
    ///     if (c.eglExportDMABUFImageQueryMESA(
    ///         display,
    ///         self,
    ///         &query.fourcc,
    ///         &query.num_planes,
    ///         &query.modifier,
    ///     ) != c.EGL_TRUE) {
    ///         return mustError();
    ///     }
    ///     return query;
    /// }
    /// ```
    ///
    /// R13 FIX: see [`export_dmabuf_query_with`] — the reference hands the driver
    /// ONE `u64` for `modifiers`, which the extension defines as "an array of at
    /// least <num_planes> elements".
    pub fn export_dmabuf_query(&self, ctx: &OffscreenContext) -> Result<DmabufQuery, EglError> {
        let f = ctx
            .export_dmabuf_query_mesa
            .ok_or(EglError::MissingEntryPoint("eglExportDMABUFImageQueryMESA"))?;
        export_dmabuf_query_with(f, ctx.display.as_ptr(), self.0.as_ptr())
            .map_err(|()| must_error(&ctx.egl))
    }

    /// PORTS: `egl.zig:231-247`.
    ///
    /// ```zig
    /// pub fn exportDmabuf(
    ///     self: *Image,
    ///     display: *Display,
    ///     fds: []c_int,
    ///     strides: []c_int,
    ///     offsets: []c_int,
    /// ) Error!void {
    ///     if (c.eglExportDMABUFImageMESA(
    ///         display,
    ///         self,
    ///         fds.ptr,
    ///         strides.ptr,
    ///         offsets.ptr,
    ///     ) != c.EGL_TRUE) {
    ///         return mustError();
    ///     }
    /// }
    /// ```
    ///
    /// PORT NOTE (slices → arrays): the extension writes one entry per plane and
    /// "<num_planes> must be at most 4", so the arrays are fixed at
    /// [`MAX_DMABUF_PLANES`] rather than trusting a slice length the C side never
    /// sees. The reference's only caller passes its 4-element `Planes` arrays.
    pub fn export_dmabuf(
        &self,
        ctx: &OffscreenContext,
        fds: &mut [c_int; MAX_DMABUF_PLANES],
        strides: &mut [c_int; MAX_DMABUF_PLANES],
        offsets: &mut [c_int; MAX_DMABUF_PLANES],
    ) -> Result<(), EglError> {
        let f = ctx
            .export_dmabuf_mesa
            .ok_or(EglError::MissingEntryPoint("eglExportDMABUFImageMESA"))?;
        // SAFETY: `f` is the resolved `eglExportDMABUFImageMESA`; the display and
        // image are live; each out-array holds the extension's maximum of 4
        // entries.
        let ok = unsafe {
            f(
                ctx.display.as_ptr(),
                self.0.as_ptr(),
                fds.as_mut_ptr(),
                strides.as_mut_ptr(),
                offsets.as_mut_ptr(),
            )
        };
        if ok != egl::TRUE {
            return Err(must_error(&ctx.egl));
        }
        Ok(())
    }
}

/// The body of [`Image::export_dmabuf_query`] over an explicit entry point, so the
/// modifier-array fix is testable without a driver. `Err(())` means the call
/// returned `EGL_FALSE`; the caller turns that into `mustError`.
///
/// R13 FIX (`.port/parity-closure/UPSTREAM-DEFECTS.md` UD355): the reference
/// passes `&query.modifier`, the address of ONE `u64`, as `modifiers`.
/// `EGL_MESA_image_dma_buf_export` says "<modifiers> must be an array of at least
/// <num_planes> elements", and `<num_planes>` may be up to 4 — so for a 2-, 3- or
/// 4-plane export (a compressed or multi-plane layout the driver chose) the driver
/// writes 8, 16 or 24 bytes past the end of the reference's stack struct. Here the
/// driver writes into a [`MAX_DMABUF_PLANES`]-element array, and the query keeps
/// the first element (the extension requires every element to hold the same
/// value). No bound is checked before the call: the extension caps
/// `<num_planes>` at 4, and a larger or smaller count is still rejected after it
/// by `Target::export_dmabuf`, as in the reference.
pub(crate) fn export_dmabuf_query_with(
    f: ExportDmabufImageQueryMesaFn,
    display: *mut c_void,
    image: *mut c_void,
) -> Result<DmabufQuery, ()> {
    let mut fourcc: c_int = 0;
    let mut num_planes: c_int = 0;
    let mut modifiers = [0u64; MAX_DMABUF_PLANES];
    // SAFETY: `f` is the resolved `eglExportDMABUFImageQueryMESA` (or a test
    // double with its signature); `fourcc`/`num_planes` are single live ints and
    // `modifiers` holds the extension's maximum of 4 planes.
    let ok = unsafe {
        f(
            display,
            image,
            &mut fourcc,
            &mut num_planes,
            modifiers.as_mut_ptr(),
        )
    };
    if ok != egl::TRUE {
        return Err(());
    }
    Ok(DmabufQuery {
        fourcc,
        num_planes,
        modifier: modifiers[0],
    })
}

/// Loads the GL entry points from `ctx` and applies the reference's version gate
/// and debug-output setup. Returns the loaded `(major, minor)`.
///
/// This is the [`OffscreenContext`] flavour of [`prepare_context`], i.e. the
/// `glad::load(|s| egl.get_proc_address(s))` binding of the reference's
/// `getProcAddress` parameter (`OpenGL.zig:133`). Ghostty's own GTK arm passes
/// `null` instead (`OpenGL.zig:170`) because GDK has already made a context
/// current; Task 8 binds that arm to [`prepare_context`] directly.
pub fn load_gl(ctx: &OffscreenContext) -> Result<(u32, u32), Error> {
    prepare_context(|symbol| ctx.get_proc_address(symbol))
}

/// GL-ATTACH — the WINDOWED flavour of [`prepare_context`], i.e. the reference's
/// `apprt.gtk => try prepareContext(null)` arm (`OpenGL.zig:169-170`, reached from
/// `surfaceInit`/`displayRealized`). Ghostty passes `null` because GDK has already
/// made its GLArea context CURRENT before this runs; glad then loads GL from the
/// platform's default loader against that current context. The Rust `gl` crate has
/// no null-loader equivalent, so we bind `getProcAddress` to `eglGetProcAddress`,
/// which — per EGL 1.5, and as libglvnd guarantees — resolves the (vendor-neutral)
/// GL dispatch entry points that route to whatever context is current on the
/// calling thread. It creates NO display and NO context (GDK owns those); it only
/// dlopen's `libEGL.so.1` for its `eglGetProcAddress`.
///
/// The `DynamicInstance` is leaked (`Box::leak`) so `libEGL.so.1` stays mapped for
/// the process lifetime — the loaded GL pointers live in libglvnd's dispatch (kept
/// alive by GDK) but keeping libEGL resident is the belt-and-braces cheap option
/// (one small one-time leak per realize; the GL function table itself is per
/// thread and replaced on every load, `glad::load`).
///
/// # Errors
/// [`Error::EglUnavailable`] if `libEGL.so.1` cannot be loaded, or any
/// [`prepare_context`] error (version-gate / GL failure) against the current
/// context.
pub fn prepare_context_windowed() -> Result<(u32, u32), Error> {
    // SAFETY: `load_required` dlopen's libEGL.so.1 and resolves the core entry
    // points; unsafe only because it trusts the shared object to be EGL.
    let egl = unsafe { egl::DynamicInstance::<egl::EGL1_5>::load_required() }
        .map_err(|err| Error::EglUnavailable(err.to_string()))?;
    let egl: &'static egl::DynamicInstance<egl::EGL1_5> = Box::leak(Box::new(egl));

    prepare_context(|symbol| match egl.get_proc_address(symbol) {
        Some(f) => f as *const c_void,
        None => std::ptr::null(),
    })
}

/// Prepares the provided GL context, loading it with the `gl` bindings.
///
/// PORTS: `OpenGL.zig:132-159`.
///
/// ```zig
/// /// Prepares the provided GL context, loading it with glad.
/// fn prepareContext(getProcAddress: anytype) !void {
/// ```
///
/// PORT NOTE: the reference returns `void` and discards the version after the
/// gate; this returns `(major, minor)` so the offscreen test can assert on it
/// without re-querying (and so a caller can log it). Behaviourally identical.
pub fn prepare_context(
    get_proc_address: impl FnMut(&'static str) -> *const c_void,
) -> Result<(u32, u32), Error> {
    // const version = try gl.glad.load(getProcAddress);
    // const major = gl.glad.versionMajor(@intCast(version));
    // const minor = gl.glad.versionMinor(@intCast(version));
    //
    // PORT NOTE: glad's loader RETURNS the version it detected, so the reference
    // gets `major`/`minor` straight out of `load`. `glad::load` (the generated
    // struct's `load_with`) returns `()`, so the version is queried separately,
    // immediately after.
    //
    // P7-W7 T1 (DECISIONS B9): this fills the CALLING thread's table, as glad's
    // `threadlocal var context` is filled, instead of the `gl` crate's
    // process-global one.
    glad::load(get_proc_address);

    let (major, minor) = gl_version()?;

    // errdefer gl.glad.unload();
    //
    // PORT NOTE: the `errdefer` sits AFTER the version read in the reference, so
    // a failed read (glad's `load` failing) leaves the table as the load left it;
    // the same holds here. Every later error path is funnelled through
    // `prepare_loaded_context` so the unload runs exactly once, here.
    prepare_loaded_context(major, minor).inspect_err(|_| glad::unload())
}

/// The part of [`prepare_context`] after the version read, split out so that each
/// of its error paths runs the reference's `errdefer gl.glad.unload();` in the
/// caller.
fn prepare_loaded_context(major: u32, minor: u32) -> Result<(u32, u32), Error> {
    // log.info("loaded OpenGL {}.{}", .{ major, minor });
    log_info!("loaded OpenGL {major}.{minor}");

    // Need to check version before trying to enable it
    if version_is_outdated(major, minor) {
        // log.warn(
        //     "OpenGL version is too old. Ghostty requires OpenGL {d}.{d}",
        //     .{ MIN_VERSION_MAJOR, MIN_VERSION_MINOR },
        // );
        // R2: "Ghostty" -> "rustty".
        log_warn!(
            "OpenGL version is too old. rustty requires OpenGL \
             {MIN_VERSION_MAJOR}.{MIN_VERSION_MINOR}"
        );
        // return error.OpenGLOutdated;
        return Err(Error::OpenGLOutdated { major, minor });
    }

    // Enable debug output for the context.
    // try gl.enable(gl.c.GL_DEBUG_OUTPUT);
    //
    // PORT NOTE: `gl.enable` IS `pkg/opengl/draw.zig`'s `enable`, which Task 2
    // ported — so this now calls the real thing. Task 1 carried a private
    // stand-in here (it needed `enable` before `draw.rs` existed) and left a note
    // asking Tasks 2-4 to delete it and route here; done. The stand-in's
    // `is_loaded()` guards on `glEnable`/`glGetError` went with it, which loses
    // nothing: both are core GL 1.0, the reference force-unwraps both glad slots
    // (`.?`), and `draw::enable` reproduces that force-unwrap exactly (the `gl`
    // crate panics on an unloaded entry point). Any context that got far enough to
    // answer `glGetString(GL_VERSION)` above has them.
    draw::enable(gl::DEBUG_OUTPUT)?;

    // Register our debug message callback with the OpenGL context.
    // gl.glad.context.DebugMessageCallback.?(glDebugMessageCallback, null);
    //
    // PORT NOTE: the reference's `.?` force-unwraps the glad slot, i.e. it
    // PANICS if a >= 4.3 context somehow lacks glDebugMessageCallback (which
    // core 4.3 guarantees). The offscreen tests must never panic on a hostile
    // driver — the plan's clean-skip rule — so the missing slot is an `Err`
    // here instead. Unreachable on any conformant 4.3 context either way.
    if !glad::context(|gl| gl.DebugMessageCallback.is_loaded()) {
        return Err(Error::MissingGlEntryPoint("glDebugMessageCallback"));
    }
    // SAFETY: the callback is an `extern "system"` fn with the GLDEBUGPROC
    // signature and a null user_param, which it ignores.
    glad::context(|gl| unsafe {
        gl.DebugMessageCallback(Some(gl_debug_message_callback), std::ptr::null())
    });

    // Enable SRGB framebuffer for linear blending support.
    // try gl.enable(gl.c.GL_FRAMEBUFFER_SRGB);
    draw::enable(gl::FRAMEBUFFER_SRGB)?;

    Ok((major, minor))
}

/// Whether a loaded `(major, minor)` is below the
/// [`MIN_VERSION_MAJOR`].[`MIN_VERSION_MINOR`] floor.
///
/// PORTS: the `if` condition of `OpenGL.zig:145-147`.
///
/// ```zig
/// if (major < MIN_VERSION_MAJOR or
///     (major == MIN_VERSION_MAJOR and minor < MIN_VERSION_MINOR))
/// ```
///
/// PORT NOTE: the reference spells this inline in `prepareContext`. Lifting it to
/// a named predicate is behaviour-preserving and makes the gate unit-testable
/// with no GL context — which matters here, because `OffscreenContext` asks EGL
/// for >= 4.3 up front, so EGL guarantees the version and the outdated branch is
/// unreachable through [`load_gl`]. It is NOT unreachable for Task 8's
/// `apprt.gtk` arm, where GDK creates the context and picks the version, and this
/// gate is the only thing standing between an old driver and a wall of GL errors.
fn version_is_outdated(major: u32, minor: u32) -> bool {
    major < MIN_VERSION_MAJOR || (major == MIN_VERSION_MAJOR && minor < MIN_VERSION_MINOR)
}

/// Reads `GL_VERSION` and parses out `(major, minor)`.
///
/// PORT NOTE: stands in for `gl.glad.versionMajor`/`versionMinor` over the value
/// `gl.glad.load` returns (`OpenGL.zig:133-135`) — see [`prepare_context`]. The
/// `GL_VERSION` string is parsed rather than `glGetIntegerv(GL_MAJOR_VERSION)`
/// queried because the latter is itself 3.0+, so it cannot report that a context
/// is too old — which is precisely what the gate below it needs to know. The
/// leading `<major>.<minor>` form is mandated for ALL GL versions by the spec.
fn gl_version() -> Result<(u32, u32), Error> {
    // A driver too broken to even resolve glGetString would make the `gl`
    // crate's missing-entry-point stub PANIC; the plan's clean-skip rule forbids
    // that, so the slot is checked before the call.
    if !glad::context(|gl| gl.GetString.is_loaded()) {
        return Err(Error::VersionUnavailable);
    }
    // SAFETY: `GetString` was just confirmed loaded, and `GL_VERSION` returns a
    // static, NUL-terminated string owned by the driver for the context's life.
    let version = glad::context(|gl| unsafe { gl.GetString(gl::VERSION) });
    if version.is_null() {
        return Err(Error::VersionUnavailable);
    }
    let version = unsafe { CStr::from_ptr(version.cast::<c_char>()) }.to_string_lossy();

    parse_gl_version(&version).ok_or(Error::VersionUnavailable)
}

/// Splits `<major>.<minor>` off the front of a `GL_VERSION` string.
fn parse_gl_version(version: &str) -> Option<(u32, u32)> {
    // "The GL_VERSION string begins with a version number of the form
    // major.minor, optionally followed by `.release`, then a space and
    // vendor-specific text."
    let number = version.split_whitespace().next()?;
    let mut parts = number.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Whether `extensions` (a space-separated EGL extension string) contains `name`.
///
/// Split on spaces rather than substring-matched, so that e.g.
/// `EGL_KHR_platform_gbm` cannot satisfy a query for `EGL_KHR_platform_gb`.
fn has_extension(extensions: &str, name: &str) -> bool {
    extensions.split(' ').any(|ext| ext == name)
}

/// PORTS: `OpenGL.zig:72-130`.
///
/// PORT NOTE: `gl_debug_proc_callconv` (`OpenGL.zig:62-70`) computes the calling
/// convention by reflecting through glad's `GLDEBUGPROC` typedef. Its reference
/// doc:
/// 32-bit windows cross-compilation breaks with `.c` for some reason, so...
///
/// The `gl` crate's `types::GLDEBUGPROC`
/// pins that same convention as `extern "system"`, which resolves to the identical
/// ABI on every target, so the reflection has no analogue and is not needed.
extern "system" fn gl_debug_message_callback(
    src: gl::types::GLenum,
    typ: gl::types::GLenum,
    id: gl::types::GLuint,
    severity: gl::types::GLenum,
    len: gl::types::GLsizei,
    msg: *const gl::types::GLchar,
    user_param: *mut c_void,
) {
    // _ = user_param;
    let _ = user_param;

    let src_str: &str = match src {
        gl::DEBUG_SOURCE_API => "OpenGL API",
        gl::DEBUG_SOURCE_WINDOW_SYSTEM => "Window System",
        gl::DEBUG_SOURCE_SHADER_COMPILER => "Shader Compiler",
        gl::DEBUG_SOURCE_THIRD_PARTY => "Third Party",
        gl::DEBUG_SOURCE_APPLICATION => "User",
        gl::DEBUG_SOURCE_OTHER => "Other",
        _ => "Unknown",
    };

    let typ_str: &str = match typ {
        gl::DEBUG_TYPE_ERROR => "Error",
        gl::DEBUG_TYPE_DEPRECATED_BEHAVIOR => "Deprecated Behavior",
        gl::DEBUG_TYPE_UNDEFINED_BEHAVIOR => "Undefined Behavior",
        gl::DEBUG_TYPE_PORTABILITY => "Portability Issue",
        gl::DEBUG_TYPE_PERFORMANCE => "Performance Issue",
        gl::DEBUG_TYPE_MARKER => "Marker",
        gl::DEBUG_TYPE_PUSH_GROUP => "Group Push",
        gl::DEBUG_TYPE_POP_GROUP => "Group Pop",
        gl::DEBUG_TYPE_OTHER => "Other",
        _ => "Unknown",
    };

    // const msg_str = msg[0..@intCast(len)];
    //
    // PORT NOTE: Zig slices the raw pointer; `len` is the driver-reported byte
    // length, exclusive of any NUL. A negative `len` is out of spec, but this is
    // a callback INTO us from a C driver, so it is clamped rather than trusted.
    let msg_str = if msg.is_null() || len < 0 {
        String::new()
    } else {
        // SAFETY: the driver guarantees `msg` points to at least `len` bytes for
        // the duration of this call. GLchar is c_char, so the cast is a no-op on
        // the byte representation.
        let bytes = unsafe { std::slice::from_raw_parts(msg.cast::<u8>(), len as usize) };
        String::from_utf8_lossy(bytes).into_owned()
    };

    match severity {
        gl::DEBUG_SEVERITY_HIGH => log_err!("[{id}] ({src_str}: {typ_str}) {msg_str}"),
        gl::DEBUG_SEVERITY_MEDIUM => log_warn!("[{id}] ({src_str}: {typ_str}) {msg_str}"),
        gl::DEBUG_SEVERITY_LOW => log_info!("[{id}] ({src_str}: {typ_str}) {msg_str}"),
        gl::DEBUG_SEVERITY_NOTIFICATION => log_debug!("[{id}] ({src_str}: {typ_str}) {msg_str}"),
        _ => log_warn!("UNKNOWN SEVERITY [{id}] ({src_str}: {typ_str}) {msg_str}"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use super::{
        has_extension, load_gl, parse_gl_version, version_is_outdated, Error, OffscreenContext,
        MIN_VERSION_MAJOR, MIN_VERSION_MINOR,
    };

    /// Proves a headless GL [`MIN_VERSION_MAJOR`].[`MIN_VERSION_MINOR`] core
    /// context stands up with no display server, and that the loaded entry
    /// points answer `glGetString(GL_VERSION)`.
    ///
    /// SKIPS CLEANLY (prints + returns, never panics) when no such context can be
    /// made — the same contract as `metal.rs`'s "no Metal device" skip. A host
    /// whose GL ceiling is below 4.3 core is a real and expected configuration
    /// (see the plan's OPEN RISK); it is not a test failure, and it must never be
    /// papered over with a fabricated context.
    ///
    /// The skip is deliberately NARROW — absent context, absent version, absent
    /// 4.3 — and a failure of the ported `prepare_context` behaviour on a context
    /// EGL already promised is >= 4.3 core FAILS. See the match arms below.
    ///
    /// EMPIRICAL NOTE (measured 2026-07-17 on the L4 development host: aarch64
    /// VM, Mesa 26.0.3, `/dev/dri/renderD128` present and accelerated) — READ
    /// THIS BEFORE DEBUGGING AN `EGL_BAD_MATCH` SKIP.
    ///
    /// **`LIBGL_ALWAYS_SOFTWARE=1` makes this test really run, and pass.** Under
    /// it the host reports `GL_VERSION = 4.5 (Core Profile) Mesa 26.0.3`,
    /// `GL_RENDERER = llvmpipe (LLVM 22.1.1, 128 bits)` — comfortably over the
    /// 4.3 gate. That is the switch to reach for in CI and for the Task 11
    /// offscreen golden; it is deliberately NOT forced here, so that a host with
    /// a real core-profile driver exercises the real driver.
    ///
    /// Without it the default driver is virgl (ANGLE-on-Metal passthrough), which
    /// reports `GL_VERSION = 2.1 Mesa 26.0.3`, `GL_RENDERER = virgl (ANGLE
    /// (Apple, Apple M4 Pro, OpenGL 4.1 Metal - 90.5))`, and `glxinfo -B` puts
    /// `Max core profile version: 0.0` / `Max compat profile version: 2.1`. Every
    /// core-profile request from 3.2 to 4.6 fails `EGL_BAD_MATCH` there, across
    /// ALL of the surfaceless, GBM-on-`renderD128` and X11 platforms — including
    /// over a REAL, pbuffer-capable `EGL_OPENGL_BIT` config (re-measured after
    /// [`OffscreenContext::choose_config`] was corrected: 35 such configs exist on
    /// virgl's surfaceless display, and 4.3 core over the first still fails
    /// `EGL_BAD_MATCH` 0x3009). So config selection is NOT the constraint on
    /// virgl: it exposes no desktop core profile at all. Another EGL platform will
    /// not lift that ceiling; a different driver is what lifts it.
    #[test]
    fn offscreen_egl_context_reaches_gl43_core() {
        // Every GL test in this crate holds this lock for its context's whole
        // lifetime; see `test_support::gl_lock` for the rationale (P7-W7 T1 removed
        // the two hazards it was introduced for, `eglTerminate` on the shared
        // display and the process-global `gl::load_with` table).
        //
        // This test takes the lock DIRECTLY instead of going through
        // `test_support::offscreen_gl` because it asserts on WHICH `load_gl`
        // error it got (the narrow-skip arms below), which that helper collapses
        // into `None`.
        let _gl_lock = test_support::gl_lock();

        let ctx = match OffscreenContext::init() {
            Ok(ctx) => ctx,
            Err(err) => {
                // W8-03 strict mode: under RUSTTY_REQUIRE_GL=1 an unavailable
                // context is a FAILURE, not a skip (see test_support docs).
                test_support::require_gl_or_skip("offscreen_egl_context_reaches_gl43_core", &err);
                eprintln!(
                    "no headless OpenGL {MIN_VERSION_MAJOR}.{MIN_VERSION_MINOR} core context \
                     ({err}); skipping offscreen_egl_context_reaches_gl43_core"
                );
                return;
            }
        };

        // The skip licence is NARROW: the brief permits it only when "context
        // creation or the version check fails". `prepare_context` can fail for
        // reasons that are NEITHER — and those reasons are exactly the ported
        // behaviour this task exists to prove (`enable(GL_DEBUG_OUTPUT)`,
        // `glDebugMessageCallback` registration, `enable(GL_FRAMEBUFFER_SRGB)`).
        // Skipping on those would let a real regression in the port turn the
        // suite green via "skipping", disabling the only test in the task that
        // asserts GL behaviour. So they FAIL loudly instead.
        let (major, minor) = match load_gl(&ctx) {
            Ok(version) => version,
            // The version check itself could not run, or ran and found the host
            // below the floor. Both ARE "the version check failing": a host whose
            // GL ceiling is under 4.3 core is a real, expected configuration (the
            // plan's OPEN RISK), not a defect. This — not the assert below — is
            // the gate that a sub-4.3 host actually trips.
            Err(err @ (Error::VersionUnavailable | Error::OpenGLOutdated { .. })) => {
                // W8-03 strict mode: same pivot as the context-creation arm.
                test_support::require_gl_or_skip("offscreen_egl_context_reaches_gl43_core", &err);
                eprintln!(
                    "no headless OpenGL {MIN_VERSION_MAJOR}.{MIN_VERSION_MINOR} core context \
                     ({err}); skipping offscreen_egl_context_reaches_gl43_core"
                );
                return;
            }
            // `Error::Gl` (a real GL error out of the ported `enable` calls) and
            // `Error::MissingGlEntryPoint` (an entry point a >= 4.3 core context
            // guarantees). EGL already promised this context is >= 4.3 core, so
            // neither is an absent-4.3 host — both mean the port is broken.
            Err(err) => panic!(
                "prepare_context failed on a live OpenGL \
                 {MIN_VERSION_MAJOR}.{MIN_VERSION_MINOR} core context: {err}"
            ),
        };

        // Belt-and-braces on `load_gl`'s contract rather than the live gate: a
        // sub-4.3 host returns `Err(OpenGLOutdated)` above and never reaches here.
        // It exists so that a future refactor which drops the gate out of
        // `prepare_context` fails this test rather than silently passing it.
        assert!(
            major > MIN_VERSION_MAJOR || (major == MIN_VERSION_MAJOR && minor >= MIN_VERSION_MINOR),
            "load_gl returned {major}.{minor}, below the \
             {MIN_VERSION_MAJOR}.{MIN_VERSION_MINOR} gate it is supposed to enforce"
        );
    }

    /// The `GL_VERSION` parse behind [`prepare_context`]'s version gate
    /// (`OpenGL.zig:141-149`). Runs everywhere — no GL context required — so the
    /// gate itself stays covered on hosts where the context test skips.
    #[test]
    fn parses_gl_version_strings() {
        // The real strings from the hosts this was developed against.
        assert_eq!(parse_gl_version("2.1 Mesa 26.0.3"), Some((2, 1)));
        assert_eq!(
            parse_gl_version("4.6 (Core Profile) Mesa 26.0.3"),
            Some((4, 6))
        );
        // The optional `.release` field.
        assert_eq!(parse_gl_version("4.3.0 NVIDIA 550.54.14"), Some((4, 3)));
        // Malformed input is reported, never panicked on or silently zeroed.
        assert_eq!(parse_gl_version(""), None);
        assert_eq!(parse_gl_version("garbage"), None);
    }

    /// The version gate itself (`OpenGL.zig:145-147`). Runs everywhere — no GL
    /// context required — which is the only coverage the OUTDATED direction gets:
    /// `OffscreenContext` requests >= 4.3 from EGL, so it can never observe an
    /// outdated context. Task 8's GDK-created context can.
    #[test]
    fn gates_opengl_versions_below_4_3() {
        // Below the floor on the major.
        assert!(version_is_outdated(2, 1));
        assert!(version_is_outdated(3, 3));
        // Same major, below on the minor — the boundary the `and` arm exists for.
        assert!(version_is_outdated(4, 0));
        assert!(version_is_outdated(4, 2));
        // Exactly the floor is NOT outdated.
        assert!(!version_is_outdated(4, 3));
        // Above the floor, either component.
        assert!(!version_is_outdated(4, 6));
        assert!(!version_is_outdated(5, 0));
        // A higher major with a lower minor must not trip the minor comparison.
        assert!(!version_is_outdated(5, 1));
    }

    /// Guards the EGL extension probe against substring false-positives.
    #[test]
    fn matches_egl_extensions_by_whole_token() {
        let exts = "EGL_KHR_no_config_context EGL_MESA_platform_surfaceless";
        assert!(has_extension(exts, "EGL_KHR_no_config_context"));
        assert!(has_extension(exts, "EGL_MESA_platform_surfaceless"));
        // A prefix of a present extension must NOT match.
        assert!(!has_extension(exts, "EGL_KHR_no_config"));
        assert!(!has_extension(exts, "EGL_KHR_platform_gbm"));
    }

    /// The modifier every plane of the test double's export reports.
    const MODIFIER: u64 = 0x0100_0000_0000_0005;
    /// A value no write of the double produces, for the guard word.
    const CANARY: u64 = 0xDEAD_BEEF_DEAD_BEEF;

    /// A spec-conforming `eglExportDMABUFImageQueryMESA` for a 2-plane image:
    /// `<modifiers> must be an array of at least <num_planes> elements` and
    /// every element holds the same value, so it writes `modifiers[0..2]`.
    unsafe extern "system" fn two_plane_query(
        _dpy: *mut std::ffi::c_void,
        _image: *mut std::ffi::c_void,
        fourcc: *mut std::ffi::c_int,
        num_planes: *mut std::ffi::c_int,
        modifiers: *mut u64,
    ) -> khronos_egl::Boolean {
        // SAFETY: the extension's contract, which the caller must honour.
        unsafe {
            *fourcc = i32::from_le_bytes(*b"AB24");
            *num_planes = 2;
            for i in 0..2 {
                *modifiers.add(i) = MODIFIER;
            }
        }
        khronos_egl::TRUE
    }

    /// UD355 fail-first, on the reference's call shape: `exportDmabufQuery`
    /// passes `&query.modifier`, ONE `u64`. A conforming driver exporting two
    /// planes writes the second modifier into whatever follows it. The guard word
    /// after the slot is in the same allocation here (so the demonstration is
    /// defined behaviour); in the reference it is whatever sits after the
    /// `DmabufQuery` on the stack.
    #[test]
    fn dmabuf_query_single_modifier_slot_is_overrun_by_a_two_plane_export() {
        #[repr(C)]
        struct UpstreamSlot {
            modifier: u64,
            after: u64,
        }
        let mut slot = UpstreamSlot {
            modifier: 0,
            after: CANARY,
        };
        let (mut fourcc, mut num_planes) = (0, 0);
        // SAFETY: the pointer covers the whole struct, so the double's second
        // write lands in `after`, inside the allocation.
        let ok = unsafe {
            two_plane_query(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut fourcc,
                &mut num_planes,
                (&raw mut slot).cast::<u64>(),
            )
        };
        assert_eq!(ok, khronos_egl::TRUE);
        assert_eq!(slot.modifier, MODIFIER);
        assert_eq!(
            slot.after, MODIFIER,
            "the second plane's modifier landed past the single slot"
        );
    }

    /// UD355, the port: the query hands the driver room for every plane the
    /// extension allows, and reports the (shared) modifier.
    #[test]
    fn dmabuf_query_gives_the_driver_a_modifier_slot_per_plane() {
        let query = super::export_dmabuf_query_with(
            two_plane_query,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
        .expect("the double returns EGL_TRUE");
        assert_eq!(
            query,
            super::DmabufQuery {
                fourcc: i32::from_le_bytes(*b"AB24"),
                num_planes: 2,
                modifier: MODIFIER,
            }
        );
    }

    /// `EGL_FALSE` is reported as `Err(())`, which the wrapper turns into
    /// `mustError`.
    #[test]
    fn dmabuf_query_reports_egl_false() {
        unsafe extern "system" fn failing(
            _: *mut std::ffi::c_void,
            _: *mut std::ffi::c_void,
            _: *mut std::ffi::c_int,
            _: *mut std::ffi::c_int,
            _: *mut u64,
        ) -> khronos_egl::Boolean {
            khronos_egl::FALSE
        }
        assert_eq!(
            super::export_dmabuf_query_with(failing, std::ptr::null_mut(), std::ptr::null_mut()),
            Err(())
        );
    }
}
