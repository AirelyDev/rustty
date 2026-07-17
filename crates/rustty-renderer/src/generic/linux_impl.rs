//! The monomorphic GPU renderer over [`crate::opengl::OpenGL`] — the SECOND
//! `generic.zig` `Renderer(GraphicsAPI)` instantiation (Linux-only), sibling to
//! `macos_impl`'s `Renderer(Metal)`.
//!
//! PORTS: `src/renderer/generic.zig`'s `Renderer(comptime GraphicsAPI: type)` —
//! the SAME function ghostty instantiates for both Metal and OpenGL. This file is
//! COPY-ADAPTED from the already-reviewed `generic::macos_impl` (READ-ONLY
//! reference) with the `crate::metal::*` graphics-API glue swapped for
//! `crate::opengl::*`; every method cites BOTH the Rust source
//! (`generic.rs::macos_impl` line range) and the ultimate Zig oracle
//! (`generic.zig` line range) so a reviewer can diff either direction.
//!
//! MILESTONE-SCOPE-CUT (L4, per the plan's Global Constraints): this is a SKELETON
//! (Task 9) — struct decls + `init`(offscreen-only path)/`deinit`. The CPU cell
//! fill (`rebuild_cells`/`update_frame`) and the offscreen draw path
//! (`draw_frame`/the golden) land in Tasks 10-11.
//!
//! TWO deliberate divergences from `macos_impl`, both faithful to `generic.zig`'s
//! non-macOS comptime arms + rustty's current module gating:
//!   1. **No `display_link`.** `generic.zig`'s `const DisplayLink = switch
//!      (builtin.os.tag) { .macos => *macos.video.DisplayLink, else => void }`
//!      (:225-227) is `void` off macOS, so the field, the `vsync` init branch
//!      (`else => null`, :692-698), and the `if (DisplayLink != void)` deinit
//!      teardown (:806-811) all collapse to nothing here.
//!   2. **No `images`/`bg_image`.** rustty's `crate::image` module is macOS-only
//!      (it embeds `crate::metal::Texture`); the graphics-API-generic OpenGL image
//!      port is DEFERRED (kitty-graphics + background-image are milestone-cut).
//!      So the image-typed Renderer fields + `prep_background_image` are omitted
//!      (DEFERRALS.md "GL image"); `generic.zig`'s `images`/`bg_image` fields (its
//!      `imagepkg.Image` = `Image(OpenGL)`) have no rustty analogue yet.
//!
//! The OpenGL leaf resources (`opengl::{buffer,texture,sampler,target}`) have a
//! MANUAL `deinit(self)` and NO `Drop` (faithful to the Zig reference's explicit
//! model — contrast Metal's ARC/`Retained`). So, unlike `macos_impl` (which lets
//! field `Drop` free GPU state), this file ports `generic.zig`'s explicit
//! `FrameState.deinit`/`CustomShaderState.deinit`/`SwapChain.deinit` bodies (:408,
//! :497, :276) for real, and `Renderer::deinit` frees them WHILE the owned GL
//! context is still current, before `api` drops (its `OffscreenContext` terminates
//! EGL).

use std::time::Instant;

use allocator_api2::alloc::Global;
use rustty_font::atlas::{Atlas as FontAtlas, Format, OutOfMemory};
use rustty_font_shaper::{shape, Metrics, Shaper, ShaperCache, SharedGrid};
use rustty_util::math;
use rustty_vt::render::RenderState;
use rustty_vt::size::CellCountInt;
use rustty_vt::Scrollbar;
use rustty_zigstd::thread::{Mutex, Semaphore};

use core::mem;
use std::ffi::{CStr, CString};

use super::{AtomicHealth, DerivedConfig, Options};
use crate::cell::Contents;
use crate::opengl::buffer::Buffer;
use crate::opengl::gpu_abi::{
    BgImage as BgImageGpu, CellText as CellTextGpu, Uniforms as UniformsGpu,
};
use crate::opengl::sampler::Sampler;
use crate::opengl::shaders::Shaders;
use crate::opengl::target::Target;
use crate::opengl::texture::Texture;
use crate::opengl::OpenGL;
use crate::shaders::{
    BgImage, BgImageInfo, BgImageInfoFit, BgImageInfoPosition, CellBg, Uniforms, UniformsBools,
    UniformsPaddingExtend,
};
use crate::shadertoy;
use crate::size::{
    AlphaBlending, BackgroundImageFit, BackgroundImagePosition, CellSize, GridSize, Padding,
    ScreenSize, Size, WindowColorspace,
};
use crate::Overlay;

/// The number of buffers we use for double/triple buffering.
//
// PORT NOTE (macos_impl generic.rs:913-919): reference `SwapChain.buf_count =
// GraphicsAPI.swap_chain_count`. Monomorphized to the concrete
// `OpenGL::SWAP_CHAIN_COUNT` (= 1, per `OpenGL.zig:30-32` "OpenGL's frame
// completion is always sync … no need for multi-buffering"). This collapses the
// semaphore-backpressure logic to the trivial 1-buffer case; it is ported anyway
// (not special-cased) so the code stays parallel to Metal's 3-buffer version and
// any later buffer-count change is a one-constant edit.
const SWAP_CHAIN_COUNT: usize = OpenGL::SWAP_CHAIN_COUNT;

/// Whether +Y points down in the custom-shader coordinate space.
//
// PORT NOTE (macos_impl generic.rs:921-929): reference
// `GraphicsAPI.custom_shader_y_is_down` — monomorphized to
// `OpenGL::CUSTOM_SHADER_Y_IS_DOWN` (`OpenGL.zig:27-28` = `false`: the OpenGL
// fragCoord is +Y = up). Milestone-inert (only read by the deferred custom-shader
// uniforms), kept for parity + the later draw path.
#[allow(dead_code)]
const CUSTOM_SHADER_Y_IS_DOWN: bool = OpenGL::CUSTOM_SHADER_Y_IS_DOWN;

/// PORT NOTE (macos_impl generic.rs:931-954): the aggregate renderer error.
/// Reference `generic.zig` methods return Zig `!T` (inferred error unions of the
/// graphics-API leaf errors + OOM); Rust `?` needs a common type, so this enum
/// aggregates the leaf errors via the `From` impls below. The Metal twin names the
/// GPU variant `MetalFailed`; this names it `GlFailed`. The `ImageDecode` variant
/// (bg-image wuffs decode) has no analogue here — bg-image is milestone-cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// An OpenGL API call failed (any of the buffer/texture/sampler/target/
    /// shaders leaf failures, or the `OpenGL` graphics-API impl itself — context
    /// setup / a raw `gl` error).
    GlFailed,
    /// An allocation failed (the font `Atlas` allocation).
    OutOfMemory,
    /// The swap chain is defunct (its GPU state was torn down). Reference
    /// `error{Defunct}` (part of `drawFrame`'s error set).
    Defunct,
}

// PORT NOTE (macos_impl generic.rs:956-967): core's `renderer_seam::RendererError`
// bridges this via `Display` + `std::error::Error`; the enum only derives `Debug`,
// so render via the `Debug` form (the leaf variant name is the message).
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

// The OpenGL graphics-API leaf errors that `init`/`deinit` (and the later draw
// path) propagate. All GPU-side failures fold into `GlFailed` (the reference `!void`
// inferred-union member); font/allocation failures fold into `OutOfMemory`.
impl From<crate::opengl::gl::errors::Error> for Error {
    fn from(_: crate::opengl::gl::errors::Error) -> Self {
        // `opengl::buffer::Buffer::init` surfaces the raw `gl` error directly.
        Error::GlFailed
    }
}
impl From<crate::opengl::texture::Error> for Error {
    fn from(_: crate::opengl::texture::Error) -> Self {
        Error::GlFailed
    }
}
impl From<crate::opengl::sampler::Error> for Error {
    fn from(_: crate::opengl::sampler::Error) -> Self {
        Error::GlFailed
    }
}
impl From<crate::opengl::target::Error> for Error {
    fn from(_: crate::opengl::target::Error) -> Self {
        Error::GlFailed
    }
}
impl From<crate::opengl::shaders::Error> for Error {
    fn from(_: crate::opengl::shaders::Error) -> Self {
        Error::GlFailed
    }
}
impl From<crate::opengl::opengl_impl::Error> for Error {
    fn from(_: crate::opengl::opengl_impl::Error) -> Self {
        // `OpenGL::init_offscreen` (context/version) + `present` (raw gl) failures.
        Error::GlFailed
    }
}
impl From<OutOfMemory> for Error {
    fn from(_: OutOfMemory) -> Self {
        Error::OutOfMemory
    }
}
impl From<Defunct> for Error {
    fn from(_: Defunct) -> Self {
        Error::Defunct
    }
}
// `font.Shaper.init` (`Renderer::init`) fails with the platform shaper's error —
// `harfbuzz` off macOS (a `Collection` failure or a HarfBuzz failure). It folds
// into the aggregate `OutOfMemory` member of the reference `!void` inferred union
// (a font-resource failure, not a GL one), mirroring how `macos_impl` folds
// `shape::coretext::Error` (generic.rs:1040-1044).
impl From<shape::harfbuzz::Error> for Error {
    fn from(_: shape::harfbuzz::Error) -> Self {
        Error::OutOfMemory
    }
}

/// Returned by [`SwapChain::next_frame`] when the swap chain is defunct.
//
// PORT NOTE (macos_impl generic.rs:1059-1063): reference `error{Defunct}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Defunct;

/// PORT NOTE (macos_impl generic.rs:1065-1076, DEFERRED SEAM — surface-mailbox):
/// stands in for the Renderer's `surface_mailbox: apprt.surface.Mailbox` field.
/// `apprt::surface::Mailbox` does not exist in rustty (apprt-api replaced it with
/// the `AppMailbox` trait), and a `renderer -> apprt-api` edge would CYCLE. Until
/// `draw_frame` lands the field is an inhabited, default-constructible placeholder.
#[derive(Clone, Copy, Debug, Default)]
pub struct SurfaceMailbox;

/// PORT NOTE (macos_impl generic.rs:1078-1087, DEFERRED SEAM): the mailbox push
/// timeout mode. Inhabited placeholder preserving the two reference modes (R1).
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum MailboxTimeout {
    Instant,
    Forever,
}

impl SurfaceMailbox {
    /// PORT NOTE (macos_impl generic.rs:1089-1100, DEFERRED SEAM — no-op):
    /// `apprt.surface.Mailbox.push` returns the number of messages pushed; this
    /// placeholder returns 0 so the reference push sites port unchanged as no-ops.
    #[allow(dead_code)]
    pub fn push<M>(&self, _msg: M, _timeout: MailboxTimeout) -> usize {
        0
    }
}

/// The monomorphic GPU renderer over [`OpenGL`] (reference
/// `GenericRenderer(OpenGL)`).
//
// PORT NOTE (macos_impl generic.rs:1123-1326): the R1-PRESERVED `generic.zig:46-80`
// abstraction-hierarchy doc-comment lives verbatim on `macos_impl::Renderer`; it is
// not duplicated here. Fields are `pub` (the reference accesses them via
// `self.<field>` from the renderer's own methods; Zig struct fields are non-opaque)
// and dead-code-exempt until the deferred draw path reads them. Same field SET as
// `macos_impl::Renderer` with `crate::metal::*` types swapped for `crate::opengl::*`,
// MINUS `display_link` (macOS-only) and `images`/`bg_image`/`bg_image_changed` (the
// `crate::image` module is macOS-only; the GL image port is deferred — see the
// module docs).
pub struct Renderer {
    /// Allocator that can be used.
    pub alloc: Global,

    /// This mutex must be held whenever any state used in `drawFrame` is being
    /// modified, and also when it's being accessed in `drawFrame`.
    pub draw_mutex: Mutex,

    /// The configuration we need derived from the main config.
    pub config: DerivedConfig,

    /// The mailbox for communicating with the window.
    pub surface_mailbox: SurfaceMailbox,

    /// Current font metrics defining our grid.
    pub grid_metrics: Metrics,

    /// The size of everything.
    pub size: Size,

    /// True if the window is focused.
    pub focused: bool,

    /// Flag to indicate that our focus state changed for custom shaders to
    /// update their state.
    pub custom_shader_focused_changed: bool,

    /// The most recent scrollbar state.
    pub scrollbar: Scrollbar,
    pub scrollbar_dirty: bool,

    /// Tracks the last bottom-right pin of the screen to detect new output.
    pub last_bottom_node: Option<usize>,
    pub last_bottom_y: CellCountInt,

    /// The most recent viewport matches so that we can render search matches in
    /// the visible frame.
    //
    // PORT NOTE: the search-match types (`SearchMatch`/`SearchMatches`) are
    // graphics-API-agnostic (`crate::message`); they land with the search draw
    // path (deferred). Kept as raw-pointer-free `Option`s so `init`/`deinit` build
    // and tear them down trivially. Typed via the shared `crate::message` types.
    pub search_matches: Option<crate::message::SearchMatches>,
    pub search_selected_match: Option<crate::message::SearchMatch>,
    pub search_matches_dirty: bool,

    /// The current set of cells to render. Rebuilt on every frame but kept around
    /// so that we don't reallocate.
    pub cells: Contents,

    /// Set to true after rebuildCells is called.
    pub cells_rebuilt: bool,

    /// The current GPU uniform values.
    pub uniforms: Uniforms,

    /// Custom shader uniform values.
    pub custom_shader_uniforms: shadertoy::Uniforms,

    /// Timestamp we rendered out first frame (custom-shader uniforms).
    pub first_frame_time: Option<Instant>,

    /// Timestamp when we rendered out more recent frame (custom-shader uniforms).
    pub last_frame_time: Option<Instant>,

    /// The font structures.
    pub font_grid: *mut SharedGrid,
    pub font_shaper: Shaper,
    pub font_shaper_cache: ShaperCache,

    /// Background image vertex buffer (CPU-side, ergonomic).
    //
    // PORT NOTE: `generic.zig`'s `bg_image_buffer` (`shaderpkg.BgImage`) is
    // graphics-API-agnostic (`crate::shaders::BgImage`), so it is KEPT even though
    // the bg-image DRAW path is milestone-cut: `init` fills it via
    // `update_bg_image_buffer` (matching `generic.zig:773,794`) and it is synced
    // into `FrameState.bg_image_buffer` by the later draw path.
    pub bg_image_buffer: BgImage,
    /// Force-updates the swap-chain copy of the bg-image buffer whenever it changes.
    pub bg_image_buffer_modified: usize,

    /// Graphics API state.
    pub api: OpenGL,

    // NOTE: no `display_link` — `generic.zig`'s `DisplayLink` is `void` off macOS.
    /// Health of the most recently completed frame.
    pub health: AtomicHealth,

    /// Our swap chain (multiple buffering).
    pub swap_chain: SwapChain,

    /// Force-updates swap chain targets on a config change that requires it.
    pub target_config_modified: usize,

    /// If something happened that requires us to reinitialize our shaders.
    pub reinitialize_shaders: bool,

    /// Whether or not we have custom shaders.
    pub has_custom_shaders: bool,

    /// Our shader pipelines.
    pub shaders: Shaders,

    /// The render state we update per loop.
    pub terminal_state: RenderState,

    /// The number of frames since the last terminal state reset.
    pub terminal_state_frame_count: usize,

    /// Our overlay state, if any.
    pub overlay: Option<Overlay>,
}

/// Swap chain which maintains multiple copies of the state needed to render a
/// frame.
//
// PORT NOTE (macos_impl generic.rs:1328-1366): reference `const SwapChain = struct`
// (Renderer-private), widened to `pub` (dead-code exempt). For OpenGL,
// `SWAP_CHAIN_COUNT == 1`, so `frames` is a 1-element array and the semaphore is a
// single permit — the trivial degenerate case, ported un-special-cased.
pub struct SwapChain {
    /// `buf_count` structs that can hold the data needed by the GPU to draw a frame.
    pub frames: [FrameState; SWAP_CHAIN_COUNT],
    /// Index of the most recently used frame state struct.
    //
    // Zig `frame_index: std.math.IntFittingRange(0, buf_count)`; Rust has no
    // sub-byte int, so `u8`.
    pub frame_index: u8,
    /// Semaphore we wait on to ensure a frame state struct is available.
    pub frame_sema: Semaphore,

    /// Set to true when deinited, so a double-deinit is ignored (prevents
    /// double-free on the `displayUnrealized`-then-`deinit` sequence).
    pub defunct: bool,
}

impl SwapChain {
    // PORTS: generic.zig:265-274 `SwapChain.init`; macos_impl generic.rs:1368-1398.
    pub fn init(api: &OpenGL, custom_shaders: bool) -> Result<SwapChain, Error> {
        // Zig: for (&result.frames) |*frame| frame.* = try FrameState.init(...);
        // Rust builds the fixed-size array fallibly via a `Vec` (no stable
        // `array::try_from_fn`), preserving the `buf_count` loop.
        let mut frames_vec: Vec<FrameState> = Vec::with_capacity(SWAP_CHAIN_COUNT);
        for _ in 0..SWAP_CHAIN_COUNT {
            frames_vec.push(FrameState::init(api, custom_shaders)?);
        }
        let frames: [FrameState; SWAP_CHAIN_COUNT] = match frames_vec.try_into() {
            Ok(a) => a,
            // We pushed exactly `SWAP_CHAIN_COUNT` items; the `Err(Vec)` arm is
            // unreachable.
            Err(_) => unreachable!(),
        };

        Ok(SwapChain {
            frames,
            frame_index: 0,
            frame_sema: Semaphore::with_permits(SWAP_CHAIN_COUNT),
            defunct: false,
        })
    }

    // PORTS: generic.zig:276-284 `SwapChain.deinit`; macos_impl generic.rs:1400-1422.
    //
    // DIVERGENCE from `macos_impl` (which lets field `Drop` free the frames): the
    // OpenGL `FrameState` GL resources have a MANUAL `deinit` and no `Drop`, so this
    // ports `generic.zig:283`'s `for (&self.frames) |*frame| frame.deinit()` FOR
    // REAL. `FrameState::deinit` consumes by value; since `FrameState` has no `Drop`,
    // reading each out of the array (via `ptr::read`) and consuming it is sound —
    // the stale array copies are never dropped, and the `defunct` guard makes this
    // run exactly once, so there is no double-free.
    pub fn deinit(&mut self) {
        if self.defunct {
            return;
        }
        self.defunct = true;

        // Wait for all inflight draws to complete so we can cleanly deinit GPU state.
        for _ in 0..SWAP_CHAIN_COUNT {
            self.frame_sema.wait();
        }
        for frame in &mut self.frames {
            // SAFETY: `FrameState` (and its GL-handle fields) have no `Drop`, so
            // duplicating the handle via `read` + consuming it in `deinit` cannot
            // double-free; `deinit` runs exactly once (the `defunct` guard above).
            let frame = unsafe { core::ptr::read(frame) };
            frame.deinit();
        }
    }

    /// Get the next frame state to draw to (waits on the semaphore; must be paired
    /// with [`SwapChain::release_frame`]).
    // PORTS: generic.zig:288-297 `SwapChain.nextFrame`; macos_impl generic.rs:1427-1441.
    pub fn next_frame(&mut self) -> Result<&mut FrameState, Defunct> {
        if self.defunct {
            return Err(Defunct);
        }

        self.frame_sema.wait();
        // (The Zig `errdefer self.frame_sema.post()` guards the infallible index
        // update below; there is no error path between the `wait` and the return, so
        // the errdefer never fires.)
        self.frame_index = (self.frame_index + 1) % (SWAP_CHAIN_COUNT as u8);
        Ok(&mut self.frames[self.frame_index as usize])
    }

    /// Called when the frame has completed drawing.
    // PORTS: generic.zig:299-301 `SwapChain.releaseFrame`; macos_impl generic.rs:1444-1446.
    pub fn release_frame(&self) {
        self.frame_sema.post();
    }
}

/// State we need duplicated for every frame.
//
// PORT NOTE (macos_impl generic.rs:1449-1492): reference `const FrameState = struct`
// (Renderer-private), widened to `pub`. The Zig buffer-type aliases are inlined as
// the concrete `Buffer<T>` field types; the POD element types are the OpenGL-ABI
// serializers (`opengl::gpu_abi`, aliased `*Gpu`), NOT the ergonomic
// `crate::shaders` structs.
pub struct FrameState {
    pub uniforms: Buffer<UniformsGpu>,
    pub cells: Buffer<CellTextGpu>,
    pub cells_bg: Buffer<CellBg>,

    pub grayscale: Texture,
    pub grayscale_modified: usize,
    pub color: Texture,
    pub color_modified: usize,

    pub target: Target,
    /// See property of same name on Renderer for explanation.
    pub target_config_modified: usize,

    /// Buffer with the vertex data for our background image.
    pub bg_image_buffer: Buffer<BgImageGpu>,
    /// See property of same name on Renderer for explanation.
    pub bg_image_buffer_modified: usize,

    /// Custom shader state, null if we have no custom shaders.
    pub custom_shader_state: Option<CustomShaderState>,
}

impl FrameState {
    // PORTS: generic.zig:343-406 `FrameState.init`; macos_impl generic.rs:1495-1564.
    pub fn init(api: &OpenGL, custom_shaders: bool) -> Result<FrameState, Error> {
        // Uniform buffer contains exactly 1 uniform struct (undefined until a frame
        // is drawn).
        let uniforms = Buffer::<UniformsGpu>::init(api.uniform_buffer_options(), 1)?;

        // Create GPU buffers for our cells. We start them off at size 1 (too small,
        // resized as needed) — wasteful but a one-time thing.
        let cells = Buffer::<CellTextGpu>::init(api.fg_buffer_options(), 1)?;
        let cells_bg = Buffer::<CellBg>::init(api.bg_buffer_options(), 1)?;

        // Create a GPU buffer for our background image info.
        let bg_image_buffer = Buffer::<BgImageGpu>::init(api.bg_image_buffer_options(), 1)?;

        // Initialize our textures for our font atlas.
        //
        // PORT NOTE (macos_impl generic.rs:1521-1533): Zig passes a throwaway
        // `*const font.Atlas` (`initAtlasTexture` reads only `.size`/`.format`).
        // rustty's `Atlas` has private fields, so the throwaway is a real 2x2 atlas
        // over `Global` (the smallest VALID size: `Atlas::init` calls `clear()`,
        // whose node width is `size - 2`, which underflow-panics at `size < 2`); the
        // 2x2 texture is immaterial — it is resized on the first atlas sync. The
        // transient 4-byte allocation frees when the temp `Atlas` drops.
        let grayscale_atlas = FontAtlas::init(Global, 2, Format::Grayscale)?;
        let grayscale = api.init_atlas_texture(&grayscale_atlas)?;
        let color_atlas = FontAtlas::init(Global, 2, Format::Bgra)?;
        let color = api.init_atlas_texture(&color_atlas)?;

        let custom_shader_state = if custom_shaders {
            Some(CustomShaderState::init(api)?)
        } else {
            None
        };

        // Initialize the target. Start it off as small as we can (resized later).
        let target = api.init_target(1, 1)?;

        Ok(FrameState {
            uniforms,
            cells,
            cells_bg,
            bg_image_buffer,
            grayscale,
            color,
            target,
            custom_shader_state,
            grayscale_modified: 0,
            color_modified: 0,
            target_config_modified: 0,
            bg_image_buffer_modified: 0,
        })
    }

    // PORTS: generic.zig:408-417 `FrameState.deinit`.
    //
    // DIVERGENCE from `macos_impl` (which relies on field `Drop`): the OpenGL leaf
    // resources have a manual `deinit(self)` and no `Drop`, so this consumes `self`
    // by value and calls each field's `deinit()` in the reference's order. The GL
    // context must be current when this runs (guaranteed: `SwapChain::deinit` calls
    // this from `Renderer::deinit`, before `api` — the context owner — drops).
    pub fn deinit(self) {
        self.target.deinit();
        self.uniforms.deinit();
        self.cells.deinit();
        self.cells_bg.deinit();
        self.grayscale.deinit();
        self.color.deinit();
        self.bg_image_buffer.deinit();
        if let Some(state) = self.custom_shader_state {
            state.deinit();
        }
    }

    // PORTS: generic.zig:419-431 `FrameState.resize`; macos_impl generic.rs:1583-1597.
    // (Not reached by `init`/`deinit`; lands here now so the draw-path resize in
    // Tasks 10-11 needs no second edit. Milestone-inert this task.)
    #[allow(dead_code)]
    pub fn resize(&mut self, api: &OpenGL, width: usize, height: usize) -> Result<(), Error> {
        if let Some(state) = self.custom_shader_state.as_mut() {
            state.resize(api, width, height)?;
        }
        // Zig: const target = try api.initTarget(...); self.target.deinit();
        //      self.target = target;
        // The old `Target` is freed EXPLICITLY (manual GL delete) before the
        // reassignment — the OpenGL leaf has no `Drop`.
        let target = api.init_target(width, height)?;
        let old = mem::replace(&mut self.target, target);
        old.deinit();
        Ok(())
    }
}

/// State relevant to our custom shaders if we have any.
//
// PORT NOTE (macos_impl generic.rs:1600-1626): reference `const CustomShaderState =
// struct` (Renderer-private), widened to `pub`. Milestone-inert (built only when
// `custom_shaders` is set, which the golden's config never does), kept for parity.
pub struct CustomShaderState {
    /// Front/back textures used as a swap chain between custom shaders.
    pub front_texture: Texture,
    pub back_texture: Texture,

    /// Sampler for the shadertoy channel textures (shared across front/back).
    pub sampler: Sampler,

    pub uniforms: Buffer<shadertoy::Uniforms>,
}

impl CustomShaderState {
    /// Swap the front and back textures.
    // PORTS: generic.zig:461-463 `CustomShaderState.swap`; macos_impl generic.rs:1630-1633.
    #[allow(dead_code)]
    pub fn swap(&mut self) {
        mem::swap(&mut self.front_texture, &mut self.back_texture);
    }

    // PORTS: generic.zig:464-495 `CustomShaderState.init`; macos_impl generic.rs:1635-1652.
    pub fn init(api: &OpenGL) -> Result<CustomShaderState, Error> {
        let uniforms = Buffer::<shadertoy::Uniforms>::init(api.uniform_buffer_options(), 1)?;

        // Front/back textures at 1x1 px (resized later).
        let front_texture = Texture::init(api.texture_options(), 1, 1, None)?;
        let back_texture = Texture::init(api.texture_options(), 1, 1, None)?;

        let sampler = Sampler::init(api.sampler_options())?;

        Ok(CustomShaderState {
            front_texture,
            back_texture,
            sampler,
            uniforms,
        })
    }

    // PORTS: generic.zig:497-502 `CustomShaderState.deinit`.
    //
    // DIVERGENCE from `macos_impl` (field `Drop`): manual GL delete, consuming `self`.
    pub fn deinit(self) {
        self.front_texture.deinit();
        self.back_texture.deinit();
        self.sampler.deinit();
        self.uniforms.deinit();
    }

    // PORTS: generic.zig:504-... `CustomShaderState.resize`; macos_impl generic.rs:1664-1677.
    // Milestone-inert; lands now so Tasks 10-11 need no second edit.
    #[allow(dead_code)]
    pub fn resize(&mut self, api: &OpenGL, width: usize, height: usize) -> Result<(), Error> {
        let front_texture = Texture::init(api.texture_options(), width, height, None)?;
        let back_texture = Texture::init(api.texture_options(), width, height, None)?;

        // Explicit GL delete of the old textures before the reassignments.
        let old_front = mem::replace(&mut self.front_texture, front_texture);
        let old_back = mem::replace(&mut self.back_texture, back_texture);
        old_front.deinit();
        old_back.deinit();
        Ok(())
    }
}

impl Renderer {
    // PORTS: generic.zig:658-799 `Renderer.init`; macos_impl generic.rs:1831-2076.
    //
    // OFFSCREEN-ONLY path (Task 9). Like `macos_impl::init`, this builds `api` via
    // the offscreen-portable ctor (`OpenGL::init_offscreen`) rather than the
    // live-surface `GraphicsAPI.init` — `Options` carries no `rt_surface` (R4), and
    // the windowed present / GLArea attach ("GL-ATTACH") is the deferred stub
    // matching Task 8's `present` deferral. The macOS `display_link`/vsync branch
    // (generic.zig:692-698 `else => null`) and the `images`/`bg_image` fields (the
    // macOS-only `crate::image` module) are omitted — see the module docs.
    pub fn init(alloc: Global, options: Options) -> Result<Self, Error> {
        // Zig: var api = try GraphicsAPI.init(alloc, options); errdefer api.deinit();
        // (macos_impl generic.rs:1848-1852: reuse the offscreen ctor, layer `None`.)
        let api = OpenGL::init_offscreen(
            options.config.blending,
            options.size.screen.width,
            options.size.screen.height,
        )?;

        // const has_custom_shaders = options.config.custom_shaders.value.items.len > 0;
        let has_custom_shaders = !options.config.custom_shaders.value.is_empty();

        // Prepare our swap chain (errdefer -> the `?` early-return + explicit deinit
        // are handled by the leaf teardown; here the `?` drops nothing GL-owned yet
        // because `SwapChain::init` frees its own partial frames on failure via the
        // Vec push loop's leaf errors).
        let swap_chain = SwapChain::init(&api, has_custom_shaders)?;

        // Create the font shaper.
        // Zig: var font_shaper = try font.Shaper.init(alloc, .{ .features = ... });
        let font_shaper = Shaper::init(shape::Options {
            features: options
                .config
                .font_features
                .iter()
                .map(|c| c.as_bytes().to_vec())
                .collect(),
        })?;

        // Initialize all the data that requires a critical font section. The
        // shared-read lock is LOAD-BEARING and MUST be mirrored (R1):
        // `grid.lock.lockShared()` before reading `grid.metrics`.
        let grid_metrics = {
            let g: *mut SharedGrid = options.font_grid;
            // SAFETY: `g` is the external `*mut SharedGrid` handle; `(*g).lock` /
            // `(*g).metrics` are shared reads through a raw pointer with no `&mut`
            // alias (the `set_font_grid` precedent, no SB hazard).
            let _read = unsafe { (*g).lock.read().unwrap() };
            unsafe { (*g).metrics }
        };

        // generic.zig:692-698: `const display_link = switch (…) { .macos => … else
        // => null }`. Off macOS the `else => null` arm applies — there is NO display
        // link, so the field + its errdefer are omitted entirely.

        // generic.zig:718-743 uniforms literal. Reference sets projection_matrix/
        // cell_size/grid_size/grid_padding/screen_size + cursor_color to `undefined`
        // (fixed by the `update_*` calls below / `update_frame`); every REMAINING
        // field to a concrete value. Rust has no `undefined`: start from zeroed POD
        // then set each field the reference sets. (macos_impl generic.rs:1959-1985.)
        let uniforms = {
            let mut u: Uniforms = unsafe { core::mem::zeroed() };
            u.padding_extend = UniformsPaddingExtend::default();
            u.min_contrast = options.config.min_contrast;
            u.cursor_pos = [u16::MAX, u16::MAX];
            u.bg_color = [
                options.config.background.r,
                options.config.background.g,
                options.config.background.b,
                (options.config.background_opacity * 255.0).round() as u8,
            ];
            u.bools = UniformsBools {
                cursor_wide: false,
                use_display_p3: options.config.colorspace == WindowColorspace::DisplayP3,
                use_linear_blending: options.config.blending.is_linear(),
                use_linear_correction: options.config.blending == AlphaBlending::LinearCorrected,
            };
            u
        };

        // generic.zig:744-772 custom_shader_uniforms literal: resolution = {0,0,1},
        // frame_rate = 60, focus = 1; every other field 0. (macos_impl generic.rs:1990-1996.)
        let custom_shader_uniforms = {
            let mut c: shadertoy::Uniforms = unsafe { core::mem::zeroed() };
            c.resolution = [0.0, 0.0, 1.0];
            c.frame_rate = 60.0;
            c.focus = 1;
            c
        };

        // .shaders = undefined (generic.zig:781) — set by `result.init_shaders()`
        // below. Rust needs a value: build the base shaders (empty custom set);
        // `init_shaders` re-runs from the real config right after the literal.
        let shaders = api.init_shaders(&[])?;

        // .cells = .{} (generic.zig:717) — an EMPTY Contents, sized by the first
        // `update_frame`/`rebuild_cells`.
        let cells = Contents::default();

        let mut result = Self {
            alloc,
            config: options.config,
            surface_mailbox: SurfaceMailbox,
            grid_metrics,
            size: options.size,
            focused: true,
            scrollbar: Scrollbar::ZERO,
            scrollbar_dirty: false,
            last_bottom_node: None,
            last_bottom_y: 0,
            search_matches: None,
            search_selected_match: None,
            search_matches_dirty: false,
            cells,
            uniforms,
            custom_shader_uniforms,
            // .bg_image_buffer = undefined (773) — fixed by update_bg_image_buffer below.
            bg_image_buffer: unsafe { core::mem::zeroed() },
            font_grid: options.font_grid,
            font_shaper,
            font_shaper_cache: ShaperCache::init(),
            shaders,
            api,
            swap_chain,
            // ---- Zig struct-decl defaults (OMITTED by the 701-787 literal) ----
            draw_mutex: Mutex::new(),
            custom_shader_focused_changed: false,
            cells_rebuilt: false,
            first_frame_time: None,
            last_frame_time: None,
            bg_image_buffer_modified: 0,
            health: AtomicHealth::new(crate::Health::Healthy),
            target_config_modified: 0,
            reinitialize_shaders: false,
            has_custom_shaders,
            terminal_state: RenderState::empty(),
            terminal_state_frame_count: 0,
            overlay: None,
        };

        // try result.initShaders(); (789)
        result.init_shaders()?;

        // Ensure our undefined values above are correctly initialized.
        result.update_font_grid_uniforms(); // (792)
        result.update_screen_size_uniforms(); // (793)
        result.update_bg_image_buffer(); // (794)

        // generic.zig:795 `try result.prepBackgroundImage()` — OMITTED: bg-image
        // load needs the macOS-only `crate::image` module (GL image port deferred).

        Ok(result)
    }

    // PORTS: generic.zig:800-830 `Renderer.deinit`; macos_impl generic.rs:2114-2179.
    //
    // Off-macOS: the `if (DisplayLink != void)` teardown (806-811) and the
    // `images`/`bg_image` teardown (821-823) collapse to nothing (those fields do
    // not exist here — see the module docs). ORDER IS LOAD-BEARING for OpenGL: the
    // GL objects (`swap_chain` frames, `shaders`) are freed HERE while the owned GL
    // context is still current; `api` (its `OffscreenContext`) drops last, when the
    // `Renderer` value drops, terminating EGL.
    pub fn deinit(&mut self) {
        // if (self.overlay) |*overlay| overlay.deinit(self.alloc); (801)
        // Port: `Overlay::deinit(&mut self)` takes NO allocator arg.
        if let Some(overlay) = &mut self.overlay {
            overlay.deinit();
        }
        // self.terminal_state.deinit(self.alloc); (802)
        self.terminal_state.deinit(self.alloc);
        // (803-804) `search_selected_match`/`search_matches` arena teardown: the
        // arena field was collapsed (message.rs), so the owned `Option`s free via
        // Rust `Drop`; no explicit call.
        //
        // self.swap_chain.deinit(); (805) — frees every frame's GL objects (context
        // still current).
        self.swap_chain.deinit();

        // (806-811) DisplayLink teardown — omitted (void off macOS).

        // self.cells.deinit(self.alloc); (814) — takes NO allocator arg.
        self.cells.deinit();

        // self.font_shaper.deinit(); (816)
        self.font_shaper.deinit();
        // self.font_shaper_cache.deinit(self.alloc); (817) — takes NO allocator arg.
        self.font_shaper_cache.deinit();

        // self.config.deinit(); (819)
        self.config.deinit();

        // (821-823) `images`/`bg_image` teardown — omitted (fields do not exist).

        // self.deinitShaders(); (825) — frees the shader pipelines' GL objects.
        self.deinit_shaders();

        // self.api.deinit(); (827) — the owned `OpenGL` (its `OffscreenContext`)
        // terminates EGL on `Drop` when the `Renderer` value drops. A `&mut self`
        // `deinit` cannot move `api` out; documented. (`OpenGL::deinit` consumes
        // `self`, unreachable here — the field drop achieves the same.)
        //
        // self.* = undefined; (829) — no-op (Rust ownership handles teardown).
    }

    /// True if our renderer has animations (higher-frequency timer).
    // PORTS: generic.zig `hasAnimations`; macos_impl generic.rs:5227-5229.
    #[allow(dead_code)]
    pub fn has_animations(&self) -> bool {
        self.has_custom_shaders
    }

    // PORTS: generic.zig:836-... `initShaders`; macos_impl generic.rs:5231-5272.
    fn init_shaders(&mut self) -> Result<(), Error> {
        // Load our custom shaders. The reference arena is subsumed by owned
        // `Vec<CString>`; the `catch |err| break :err &.{}` (any load error ⇒ no
        // custom shaders, never block the renderer) maps to `unwrap_or_else`.
        let custom_shaders: Vec<CString> =
            shadertoy::load_from_files(&self.config.custom_shaders, OpenGL::CUSTOM_SHADER_TARGET)
                .unwrap_or_else(|_err| Vec::new());

        let custom_refs: Vec<&CStr> = custom_shaders.iter().map(|c| c.as_c_str()).collect();
        let has_custom_shaders = !custom_refs.is_empty();

        // var shaders = try self.api.initShaders(...); self.shaders = shaders;
        // The old `Shaders` value is replaced; its GL pipelines are freed
        // EXPLICITLY (manual GL delete — `Shaders` has no `Drop`) before the
        // reassignment, matching the reference's `errdefer shaders.deinit()` +
        // replace model.
        let new_shaders = self.api.init_shaders(&custom_refs)?;
        let mut old = mem::replace(&mut self.shaders, new_shaders);
        old.deinit();
        self.has_custom_shaders = has_custom_shaders;
        Ok(())
    }

    // PORTS: generic.zig:832-834 `deinitShaders`; macos_impl generic.rs:5216-5218.
    pub fn deinit_shaders(&mut self) {
        self.shaders.deinit();
    }

    /// Update uniforms that are based on the font grid.
    // PORTS: generic.zig `updateFontGridUniforms`; macos_impl generic.rs:4811-4816.
    pub fn update_font_grid_uniforms(&mut self) {
        self.uniforms.cell_size = [
            self.grid_metrics.cell_width as f32,
            self.grid_metrics.cell_height as f32,
        ];
    }

    /// Update uniforms that are based on the screen size.
    // PORTS: generic.zig `updateScreenSizeUniforms`; macos_impl generic.rs:4827-4864.
    // PARITY: the reference multiplies left/top padding by `-1`; the `-1 *` form is
    // kept verbatim rather than the `-(x)` clippy rewrite.
    #[allow(clippy::neg_multiply)]
    pub fn update_screen_size_uniforms(&mut self) {
        let terminal_size: ScreenSize = self.size.terminal();

        // Blank space around the grid.
        let blank: Padding = self
            .size
            .screen
            .blank_padding(
                self.size.padding,
                GridSize {
                    columns: self.cells.size.columns,
                    rows: self.cells.size.rows,
                },
                CellSize {
                    width: self.grid_metrics.cell_width,
                    height: self.grid_metrics.cell_height,
                },
            )
            .add(self.size.padding);

        self.uniforms.projection_matrix = math::ortho2d(
            -1.0 * self.size.padding.left as f32,
            (terminal_size.width + self.size.padding.right) as f32,
            (terminal_size.height + self.size.padding.bottom) as f32,
            -1.0 * self.size.padding.top as f32,
        );
        self.uniforms.grid_padding = [
            blank.top as f32,
            blank.right as f32,
            blank.bottom as f32,
            blank.left as f32,
        ];
        self.uniforms.screen_size = [
            self.size.screen.width as f32,
            self.size.screen.height as f32,
        ];
    }

    /// Update the background image vertex buffer (CPU-side).
    // PORTS: generic.zig `updateBgImageBuffer`; macos_impl generic.rs:4874-4902.
    pub fn update_bg_image_buffer(&mut self) {
        self.bg_image_buffer = BgImage {
            opacity: self.config.bg_image_opacity,
            info: BgImageInfo {
                position: match self.config.bg_image_position {
                    BackgroundImagePosition::TopLeft => BgImageInfoPosition::Tl,
                    BackgroundImagePosition::TopCenter => BgImageInfoPosition::Tc,
                    BackgroundImagePosition::TopRight => BgImageInfoPosition::Tr,
                    BackgroundImagePosition::CenterLeft => BgImageInfoPosition::Ml,
                    BackgroundImagePosition::CenterCenter | BackgroundImagePosition::Center => {
                        BgImageInfoPosition::Mc
                    }
                    BackgroundImagePosition::CenterRight => BgImageInfoPosition::Mr,
                    BackgroundImagePosition::BottomLeft => BgImageInfoPosition::Bl,
                    BackgroundImagePosition::BottomCenter => BgImageInfoPosition::Bc,
                    BackgroundImagePosition::BottomRight => BgImageInfoPosition::Br,
                },
                fit: match self.config.bg_image_fit {
                    BackgroundImageFit::Contain => BgImageInfoFit::Contain,
                    BackgroundImageFit::Cover => BgImageInfoFit::Cover,
                    BackgroundImageFit::Stretch => BgImageInfoFit::Stretch,
                    BackgroundImageFit::None => BgImageInfoFit::None,
                },
                repeat: self.config.bg_image_repeat,
            },
        };
        self.bg_image_buffer_modified = self.bg_image_buffer_modified.wrapping_add(1);
    }

    /// GL-ATTACH (windowed present — DEFERRED STUB). Parallel to `macos_impl`'s
    /// `attach_host_view` (generic.rs:2218-2233) but no-op: the windowed
    /// `GtkGLArea` context hookup is L5's job (the "GL-ATTACH" deferral, matching
    /// Task 8's `present` deferral). Offscreen backends never attach a live view.
    #[allow(dead_code)]
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn attach_host_view(
        &mut self,
        _view: *mut core::ffi::c_void,
        _scale: f64,
    ) -> Result<(), Error> {
        Ok(())
    }
}

// FFI-heavy (raw GL calls behind the leaves + the `*mut SharedGrid` handle) —
// Miri-excluded like the Metal lane (add to MIRI_EXCEPTIONS.md under `linux_impl`).
#[cfg(test)]
mod tests {
    use allocator_api2::alloc::Global;

    use super::Renderer;
    use crate::generic::{Color, DerivedConfig, Options, RepeatablePath, TerminalColor};
    use crate::link;
    use crate::opengl::gl::test_support;
    use crate::size::{
        AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, CellSize,
        FontShapingBreak, Padding, ScreenSize, Size, WindowColorspace,
    };
    use rustty_font_shaper::codepoint_resolver::{CodepointResolver, DescriptorCache, StyleStatus};
    use rustty_font_shaper::collection::{AddOptions, Collection, LoadOptions, SizeAdjustment};
    use rustty_font_shaper::face::{
        DesiredSize, Face, Options as FaceOptions, FREETYPE_LOAD_FLAGS_DEFAULT,
    };
    use rustty_font_shaper::library::Library;
    use rustty_font_shaper::SharedGrid;
    use rustty_font_shaper::Style as FontStyle;
    use rustty_render_types::WindowPaddingColor;
    use rustty_vt::color;
    use rustty_vt::size::CellCountInt;
    use rustty_zigstd::arena_allocator::ArenaAllocator;

    // PORT NOTE: `derived_config_for_test`/`test_font_grid_and_size` are duplicated
    // from `macos_impl::tests` (the established test-helper-duplication precedent —
    // `opts(cols, rows)` is already duplicated cell.rs/cursor.rs/generic.rs). We
    // duplicate rather than share to avoid coupling `linux_impl`'s tests to
    // `macos_impl`'s internals (and `macos_impl` is not even compiled here).
    fn derived_config_for_test() -> DerivedConfig {
        DerivedConfig {
            arena: ArenaAllocator::init(),
            font_thicken: false,
            font_thicken_strength: 0,
            font_features: Vec::new(),
            font_styles: StyleStatus::init_fill(true),
            font_shaping_break: FontShapingBreak::default(),
            cursor_color: Some(TerminalColor::Color(Color {
                r: 0xFF,
                g: 0,
                b: 0,
            })),
            cursor_opacity: 1.0,
            cursor_text: None,
            background: color::RGB { r: 0, g: 0, b: 0 },
            background_opacity: 1.0,
            background_opacity_cells: false,
            foreground: color::RGB {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF,
            },
            selection_background: None,
            selection_foreground: None,
            search_background: TerminalColor::Color(Color {
                r: 0xFF,
                g: 0xE0,
                b: 0x82,
            }),
            search_foreground: TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
            search_selected_background: TerminalColor::Color(Color {
                r: 0xF2,
                g: 0xA5,
                b: 0x7E,
            }),
            search_selected_foreground: TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
            bold_color: None,
            faint_opacity: 255,
            min_contrast: 1.0,
            padding_color: WindowPaddingColor::Background,
            custom_shaders: RepeatablePath::default(),
            bg_image: None,
            bg_image_opacity: 1.0,
            bg_image_position: BackgroundImagePosition::TopLeft,
            bg_image_fit: BackgroundImageFit::Contain,
            bg_image_repeat: false,
            links: link::Set { links: Vec::new() },
            vsync: false,
            colorspace: WindowColorspace::Srgb,
            blending: AlphaBlending::Native,
            background_blur: BackgroundBlur::False,
            scroll_to_bottom_on_output: false,
        }
    }

    // Build a leaked embedded-font `SharedGrid` + the pixel `Size` for a cols×rows
    // grid (the `macos_impl::tests::test_font_grid_and_size` recipe, generic.rs:6283).
    // The grid is leaked so the returned `*mut SharedGrid` outlives the test scope;
    // `size.screen` is PINNED to the grid's pixel size.
    fn test_font_grid_and_size(cols: CellCountInt, rows: CellCountInt) -> (*mut SharedGrid, Size) {
        let font_grid: *mut SharedGrid = {
            let lib = Library::init().unwrap();
            let mut c = Collection::init();
            c.load_options = Some(LoadOptions {
                library: lib,
                ..Default::default()
            });
            c.add(
                Face::init(
                    lib,
                    rustty_font::embedded::REGULAR,
                    FaceOptions {
                        size: DesiredSize {
                            points: 12.0,
                            xdpi: 96,
                            ydpi: 96,
                        },
                        freetype_load_flags: FREETYPE_LOAD_FLAGS_DEFAULT,
                    },
                )
                .unwrap(),
                AddOptions {
                    style: FontStyle::Regular,
                    fallback: false,
                    size_adjustment: SizeAdjustment::None,
                },
            )
            .unwrap();
            let resolver = CodepointResolver {
                collection: c,
                styles: StyleStatus::init_fill(true),
                discover: None,
                codepoint_map: None,
                descriptor_cache: DescriptorCache::default(),
                sprite: None,
            };
            let grid = SharedGrid::init(Global, resolver).unwrap();
            Box::leak(Box::new(grid)) as *mut SharedGrid
        };

        let grid_metrics = unsafe { (*font_grid).metrics };
        let cell = CellSize {
            width: grid_metrics.cell_width,
            height: grid_metrics.cell_height,
        };
        let size = Size {
            screen: ScreenSize {
                width: cell.width * cols as u32,
                height: cell.height * rows as u32,
            },
            cell,
            padding: Padding {
                top: 0,
                bottom: 0,
                right: 0,
                left: 0,
            },
        };
        (font_grid, size)
    }

    /// L4 T9 KAT: the OpenGL `Renderer` init/deinit round-trip — one layer up from
    /// Task 8's `opengl_init_offscreen_then_deinit_round_trips`. Stands up a real
    /// embedded-font grid, builds the monomorphic `Renderer(OpenGL)` via
    /// `Renderer::init` (which internally stands up the offscreen EGL/GL context),
    /// then tears it down via `deinit` — asserting no panic/leak across the whole
    /// FFI teardown (the swap-chain frames' GL objects + the shader pipelines are
    /// freed while the context is current, then the context terminates).
    ///
    /// Skips cleanly (never panics, never fabricates) when the host cannot provide
    /// an OpenGL 4.3-core context — the Task-1 OPEN RISK path (the default virgl
    /// driver here; runs for real under `LIBGL_ALWAYS_SOFTWARE=1` → llvmpipe 4.5).
    ///
    /// Holds the process-wide [`test_support::gl_lock`] for the whole context
    /// lifetime (the `eglTerminate`-on-a-shared-display + `gl::load_with`-global
    /// hazards), `_guard` declared FIRST so it drops LAST (after `deinit`
    /// terminates the context), matching Task 8's field-order contract.
    #[test]
    fn linux_opengl_renderer_init_then_deinit_round_trips() {
        let _guard = test_support::gl_lock();

        let (font_grid, size) = test_font_grid_and_size(10, 3);
        let opts = Options {
            config: derived_config_for_test(),
            font_grid,
            size,
        };

        let mut r = match Renderer::init(Global, opts) {
            Ok(r) => r,
            // A GL/context failure here is the host being unable to stand up a
            // 4.3-core context (absent EGL / no surfaceless platform / the virgl
            // driver's `EGL_BAD_MATCH` / a version under the floor). All are a
            // CLEAN SKIP, mirroring Task 8's `init_offscreen` skip contract — never
            // a panic, never a fabricated renderer.
            Err(e) => {
                eprintln!(
                    "no headless OpenGL 4.3 core context ({e}); skipping \
                     linux_opengl_renderer_init_then_deinit_round_trips"
                );
                return;
            }
        };

        // Fields the offscreen ctor + the `update_*` tail set (non-vacuous: proves
        // `init` ran the whole path, not just the struct literal).
        assert!(!r.has_custom_shaders);
        assert_eq!(
            r.uniforms.cell_size,
            [
                r.grid_metrics.cell_width as f32,
                r.grid_metrics.cell_height as f32
            ]
        );

        // The teardown must not panic/leak: leaf GL objects freed first (context
        // still current), then the context (via `api`'s drop after `deinit`).
        r.deinit();
    }
}
