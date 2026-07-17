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
use rustty_font::glyph::{Constraint, ConstraintSize, RenderOptions};
use rustty_font::nerd_font_attributes::get_constraint;
use rustty_font::sprite::Sprite;
use rustty_font_shaper::shared_grid::RenderError;
use rustty_font_shaper::{
    collection, shape, Metrics, Presentation, Shaper, ShaperCache, SharedGrid, SPRITE_INDEX,
};
use rustty_render_types::WindowPaddingColor;
use rustty_util::math;
use rustty_vt::input::key;
use rustty_vt::input::key_mods::Mods;
use rustty_vt::modes::Mode;
use rustty_vt::pagelist::OutOfMemory as VtOutOfMemory;
use rustty_vt::render::{self, CellSet, Dirty, Highlight, RenderState};
use rustty_vt::size::CellCountInt;
use rustty_vt::style::{Fg, Style};
use rustty_vt::terminal::ScrollViewport;
use rustty_vt::Scrollbar;
use rustty_vt::{color, page, point, sgr};
use rustty_zigstd::thread::{Mutex, Semaphore};

use core::mem;
use std::ffi::{CStr, CString};

use super::{AtomicHealth, DerivedConfig, Options, TerminalColor};
use crate::cell::{self, Contents, Key};
use crate::cursor;
use crate::opengl::buffer::Buffer;
use crate::opengl::gpu_abi::{
    BgImage as BgImageGpu, CellText as CellTextGpu, Uniforms as UniformsGpu,
};
use crate::opengl::sampler::Sampler;
use crate::opengl::shaders::Shaders;
use crate::opengl::target::Target;
use crate::opengl::texture::Texture;
use crate::opengl::OpenGL;
use crate::row::never_extend_bg;
use crate::shaders::{
    Atlas, BgImage, BgImageInfo, BgImageInfoFit, BgImageInfoPosition, CellBg, CellText,
    CellTextBools, Uniforms, UniformsBools, UniformsPaddingExtend,
};
use crate::shadertoy;
use crate::size::{
    AlphaBlending, BackgroundImageFit, BackgroundImagePosition, CellSize, GridSize, Padding,
    ScreenSize, Size, WindowColorspace,
};
use crate::state;
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

// PORT NOTE (macos_impl generic.rs:1113-1121): reference `const HighlightTag =
// enum(u8) { search_match, search_match_selected }` (Renderer-private). Consumed by
// `rebuild_row`/`update_frame`'s search-highlight path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HighlightTag {
    SearchMatch,
    SearchMatchSelected,
}

// PORT NOTE (macos_impl generic.rs:2762-2772): reference `const PreeditRange = struct`
// — the CPU cell-fill preedit skip-range.
#[derive(Clone, Copy)]
struct PreeditRange {
    y: CellCountInt,
    x: [CellCountInt; 2],
    cp_offset: usize,
}

/// The per-cell selection precedence used by [`Renderer::rebuild_row`].
// PORT NOTE (macos_impl generic.rs:2774-2788): reference anonymous enum `const
// selected: enum { false, selection, search, search_selected }` (inline in the loop,
// generic.zig:2752-2757), hoisted to a module-private named enum (`False` renames
// `.false`). `SearchSelected` faithfully mirrors `.search_selected`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
enum Selected {
    False,
    Selection,
    Search,
    SearchSelected,
}

/// PORT NOTE (macos_impl generic.rs:2790-2817): the un-caught error sources of
/// `rebuild_row` (the reference `!void`). The only `try`s NOT `catch`-logged inline
/// are `run_iter.next` (a `collection::Error`) and `font_shaper.shape`. FAITHFUL
/// DIVERGENCE: `macos_impl` names the shaper arm `shape::coretext::Error` (CoreText
/// is the macOS shaper); the Linux backend is `harfbuzz::Shaper`
/// (`rustty_font_shaper::shape::Shaper` cfg-selects `harfbuzz` off macOS), so the arm
/// is `shape::harfbuzz::Error` here. The payloads carry the faithful error value the
/// reference logs (`log.warn`); rustty has no logging facade wired, so those sites
/// are comments and the payload is unread — `#[allow(dead_code)]` until logging lands.
#[derive(Debug)]
#[allow(dead_code)]
enum RebuildRowError {
    Collection(collection::Error),
    Shape(shape::harfbuzz::Error),
}

impl From<collection::Error> for RebuildRowError {
    fn from(e: collection::Error) -> Self {
        RebuildRowError::Collection(e)
    }
}
impl From<shape::harfbuzz::Error> for RebuildRowError {
    fn from(e: shape::harfbuzz::Error) -> Self {
        RebuildRowError::Shape(e)
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

    pub fn rebuild_cells(
        &mut self,
        preedit: Option<state::Preedit>,
        cursor_style_: Option<cursor::Style>,
        links: &CellSet,
    ) {
        // const state: *terminal.RenderState = &self.terminal_state;
        //
        // SAFETY (the REAL invariant — NOT "disjoint fields", GPU_RD_CHUNK3B_DESIGN
        // §C.3(1)): no INLINE-self alias read through `state` is held live across a
        // nested `&mut *self` reborrow. Every inline scalar a post-`rebuild_row`
        // block needs (`colors.{foreground,background,cursor,palette}`,
        // `cursor.{viewport,cell}`, `rows/cols/dirty`) is Copy-hoisted to a local
        // BELOW, BEFORE the row loop's `&mut *self` sub-calls (remedy 1). The
        // per-row `dirty=false` write-back goes through a fresh `&mut
        // self.terminal_state` *before* `rebuild_row`, so it is never aliased with
        // a read. `page::Cell`/`Style`/`RGB`/`Colors`-scalars are `Copy`, so hoisted
        // values leave the `*state` region by copy. The pre-loop `self.cells.*` /
        // `self.uniforms.*` writes are FIELD-level `&mut` (they retag only that
        // field, not the whole `Renderer` allocation), so `state` survives them.
        // Registered in `.port/phase-2/MIRI_EXCEPTIONS.md` (adversary-only — the
        // Metal-gated 3b test cannot run Miri).
        let state: *const RenderState = &self.terminal_state;

        // const start = try std.time.Instant.now();
        // const start_micro = std.time.microTimestamp();
        // defer {
        //     const end = std.time.Instant.now() catch unreachable;
        //     // "[rebuildCells time] <START us>\t<TIME_TAKEN us>"
        //     std.log.warn("[rebuildCells time] {}\t{}", .{start_micro, end.since(start) / std.time.ns_per_us});
        // }

        let grid_size_diff = self.cells.size.rows != unsafe { (*state).rows }
            || self.cells.size.columns != unsafe { (*state).cols };

        if grid_size_diff {
            let mut new_size = self.cells.size;
            new_size.rows = unsafe { (*state).rows };
            new_size.columns = unsafe { (*state).cols };
            self.cells.resize(new_size);

            // Update our uniforms accordingly, otherwise
            // our background cells will be out of place.
            self.uniforms.grid_size = [new_size.columns, new_size.rows];
        }

        let rebuild = unsafe { (*state).dirty } == Dirty::Full || grid_size_diff;
        if rebuild {
            // If we are doing a full rebuild, then we clear the entire cell buffer.
            self.cells.reset();

            // We also reset our padding extension depending on the screen type
            match self.config.padding_color {
                WindowPaddingColor::Background => {}

                // For extension, assume we are extending in all directions.
                // For "extend" this may be disabled due to heuristics below.
                WindowPaddingColor::Extend | WindowPaddingColor::ExtendAlways => {
                    self.uniforms.padding_extend = UniformsPaddingExtend {
                        up: true,
                        down: true,
                        left: true,
                        right: true,
                    };
                }
            }
        }

        // From this point on we never fail. We produce some kind of
        // working terminal state, even if incorrect.
        // (reference `errdefer comptime unreachable;` — the Rust port is infallible
        // from here, hence `rebuild_cells` returns `()`.)

        // §C.3 REMEDY 1: Copy-hoist every inline `terminal_state` scalar the
        // cursor/preedit blocks read AFTER the row loop, BEFORE the loop.
        let hoisted_foreground = unsafe { (*state).colors.foreground };
        let hoisted_background = unsafe { (*state).colors.background };
        let hoisted_colors_cursor = unsafe { (*state).colors.cursor };
        let hoisted_palette = unsafe { (*state).colors.palette };
        let hoisted_cursor_cell = unsafe { (*state).cursor.cell };
        // `Viewport` is not `Copy`, so hoist its three scalar fields by value.
        let hoisted_cursor_vp: Option<(CellCountInt, CellCountInt, bool)> =
            unsafe { (*state).cursor.viewport.as_ref() }.map(|v| (v.x, v.y, v.wide_tail));
        let rows = unsafe { (*state).rows };
        let cols = unsafe { (*state).cols };

        // If our cell contents buffer is shorter than the screen viewport,
        // we render the rows that fit, starting from the bottom. If instead
        // the viewport is shorter than the cell contents buffer, we align
        // the top of the viewport with the top of the contents buffer.
        let row_len: usize = core::cmp::min(rows as usize, self.cells.size.rows as usize);

        // Determine our x/y range for preedit. We don't want to render anything
        // here because we will render the preedit separately.
        let preedit_range: Option<PreeditRange> = if let Some(preedit_v) = preedit.as_ref() {
            'preedit: {
                // We base the preedit on the position of the cursor in the
                // viewport. If the cursor isn't visible in the viewport we
                // don't show it.
                let (cvx, cvy, _) = match hoisted_cursor_vp {
                    Some(v) => v,
                    None => break 'preedit None,
                };

                // If our preedit row isn't dirty then we don't need the
                // preedit range. This also avoids an issue later where we
                // unconditionally add preedit cells when this is set.
                if !rebuild && !self.terminal_state.row_data[cvy as usize].dirty {
                    break 'preedit None;
                }

                let range = preedit_v.range(cvx, cols - 1);
                Some(PreeditRange {
                    y: cvy,
                    x: [range.start, range.end],
                    cp_offset: range.cp_offset,
                })
            }
        } else {
            None
        };

        // The SGR style AT the cursor cell (ref :2458-2465, distinct from the
        // VISUAL cursor `style` below), hoisted BEFORE the row loop (remedy 1:
        // `Style` is `Copy`, and `rebuild_row` never mutates `row_data`, so the
        // value is identical whether read here or in the cursor block). It feeds
        // the `cell-foreground`/`cell-background` cursor-color + cursor-text arms.
        let cursor_sgr_style: Style = match hoisted_cursor_vp {
            Some((cvx, cvy, _)) => {
                // PORT NOTE (reshape): read the SoA `raw`/`style` columns.
                let cc = &self.terminal_state.row_data[cvy as usize].cells;
                if cc.raw()[cvx as usize].has_styling() {
                    cc.style()[cvx as usize]
                } else {
                    Style::default()
                }
            }
            None => Style::default(),
        };

        for y_usize in 0..row_len {
            let y: CellCountInt = y_usize as CellCountInt;

            if !rebuild {
                // Only rebuild if we are doing a full rebuild or this row is dirty.
                if !self.terminal_state.row_data[y_usize].dirty {
                    continue;
                }

                // Clear the cells if the row is dirty
                self.cells.clear(y);
            }

            // Unmark the dirty state in our render state. (Fresh `&mut
            // self.terminal_state` write-back BEFORE `rebuild_row`, so it is not
            // aliased with any read — §C.3.)
            self.terminal_state.row_data[y_usize].dirty = false;

            // Get our row data from our state
            // (PORT NOTE: the reference projects the row's SoA columns once —
            // `state.row_data.slice()` + `.items(.raw/.cells/.dirty/.selection/
            // .highlights)`, ref :2364-2370; rustty's `row_data` is AoS, so we read
            // each column per-row here inside the loop.)
            let row: page::Row = self.terminal_state.row_data[y_usize].raw;
            let selection = self.terminal_state.row_data[y_usize].selection;
            // HEAP-backed raw slices (the `Vec` backing buffers of
            // `row_data[y].cells`/`highlights`) — SB-SAFE across `rebuild_row`'s
            // `&mut *self` reborrows (§C.3(2)): a reborrow retags the inline
            // `Renderer`, not these separate heap allocations.
            // PORT NOTE (reshape): a `*const CellList` (the reference passes
            // `cells: *std.MultiArrayList(RenderState.Cell)` into `rebuildRow`).
            let cells_ptr: *const render::CellList = &self.terminal_state.row_data[y_usize].cells;
            let highlights_ptr: *const [Highlight] =
                self.terminal_state.row_data[y_usize].highlights.as_slice();

            if let Err(_err) = self.rebuild_row(
                y,
                row,
                cells_ptr,
                preedit_range,
                selection,
                highlights_ptr,
                links,
            ) {
                // This should never happen except under exceptional
                // scenarios. In this case, we don't want to corrupt
                // our render state so just clear this row and keep
                // trying to finish it out.
                // log.warn("error building row y={} err={}", .{ y, err });
                self.cells.clear(y);
            }
        }

        // Setup our cursor rendering information.
        'cursor: {
            // Clear our cursor by default.
            self.cells.set_cursor(None, None);
            self.uniforms.cursor_pos = [u16::MAX, u16::MAX];

            // If the cursor isn't visible on the viewport, don't show
            // a cursor. Otherwise, get our cursor cell, because we may
            // need it for styling.
            let (cvx, cvy, _cv_wide_tail) = match hoisted_cursor_vp {
                Some(v) => v,
                None => break 'cursor,
            };
            let cursor_style: Style = cursor_sgr_style;

            // If we have preedit text, we don't setup a cursor
            if preedit.is_some() {
                break 'cursor;
            }

            // If there isn't a cursor visual style requested then
            // we don't render a cursor.
            let style = match cursor_style_ {
                Some(s) => s,
                None => break 'cursor,
            };

            // Determine the cursor color.
            let cursor_color: color::RGB = 'cursor_color: {
                // If an explicit cursor color was set by OSC 12, use that.
                if let Some(v) = hoisted_colors_cursor {
                    break 'cursor_color v;
                }

                // Use our configured color if specified
                if let Some(v) = self.config.cursor_color {
                    match v {
                        TerminalColor::Color(color) => break 'cursor_color color.to_terminal_rgb(),

                        TerminalColor::CellForeground | TerminalColor::CellBackground => {
                            let fg_style = cursor_style.fg(Fg {
                                default: hoisted_foreground,
                                palette: &hoisted_palette,
                                bold: self.config.bold_color,
                            });
                            let bg_style = cursor_style
                                .bg(&hoisted_cursor_cell, &hoisted_palette)
                                .unwrap_or(hoisted_background);

                            break 'cursor_color match v {
                                TerminalColor::Color(_) => unreachable!(),
                                TerminalColor::CellForeground => {
                                    if cursor_style.flags.inverse {
                                        bg_style
                                    } else {
                                        fg_style
                                    }
                                }
                                TerminalColor::CellBackground => {
                                    if cursor_style.flags.inverse {
                                        fg_style
                                    } else {
                                        bg_style
                                    }
                                }
                            };
                        }
                    }
                }

                hoisted_foreground
            };

            // self.addCursor(&state.cursor, style, cursor_color); (§D.6)
            // SAFETY (§C.3/§D.6): BOTH `self` and `cursor_ptr` are raw, exactly as
            // the reference `addCursor(self: *Self, cursor_state: *const ...)`.
            // Taking `add_cursor` raw means its receiver does NOT retag the whole
            // `Renderer` and pop `cursor_ptr` (the `*const` alias into inline
            // `self.terminal_state.cursor`) before the body derefs it — the SB UB
            // the Miri harness proved for a `&mut self` receiver.
            let self_raw: *mut Renderer = self;
            let cursor_ptr: *const render::Cursor = unsafe { &(*self_raw).terminal_state.cursor };
            unsafe { Renderer::add_cursor(self_raw, cursor_ptr, style, cursor_color) };

            // If the cursor is visible then we set our uniforms.
            if style == cursor::Style::Block {
                let wide = hoisted_cursor_cell.wide();

                self.uniforms.cursor_pos = [
                    // If we are a spacer tail of a wide cell, our cursor needs
                    // to move back one cell. The saturate is to ensure we don't
                    // overflow but this shouldn't happen with well-formed input.
                    match wide {
                        page::Wide::Narrow | page::Wide::SpacerHead | page::Wide::Wide => cvx,
                        page::Wide::SpacerTail => cvx.saturating_sub(1),
                    },
                    cvy,
                ];

                self.uniforms.bools.cursor_wide = match wide {
                    page::Wide::Narrow | page::Wide::SpacerHead => false,
                    page::Wide::Wide | page::Wide::SpacerTail => true,
                };

                let uniform_color: color::RGB = if let Some(txt) = self.config.cursor_text {
                    'blk: {
                        // If cursor-text is set, then compute the correct color.
                        // Otherwise, use the background color.
                        if let TerminalColor::Color(color) = txt {
                            // Use the color set by cursor-text, if any.
                            break 'blk color.to_terminal_rgb();
                        }

                        let fg_style = cursor_style.fg(Fg {
                            default: hoisted_foreground,
                            palette: &hoisted_palette,
                            bold: self.config.bold_color,
                        });
                        let bg_style = cursor_style
                            .bg(&hoisted_cursor_cell, &hoisted_palette)
                            .unwrap_or(hoisted_background);

                        break 'blk match txt {
                            // If the cell is reversed, use the opposite cell color instead.
                            TerminalColor::CellForeground => {
                                if cursor_style.flags.inverse {
                                    bg_style
                                } else {
                                    fg_style
                                }
                            }
                            TerminalColor::CellBackground => {
                                if cursor_style.flags.inverse {
                                    fg_style
                                } else {
                                    bg_style
                                }
                            }
                            _ => unreachable!(),
                        };
                    }
                } else {
                    hoisted_background
                };

                self.uniforms.cursor_color =
                    [uniform_color.r, uniform_color.g, uniform_color.b, 255];
            }
        }

        // Setup our preedit text.
        if let Some(preedit_v) = preedit.as_ref() {
            'preedit: {
                let range = match preedit_range {
                    Some(r) => r,
                    None => break 'preedit,
                };
                let mut x = range.x[0];
                for cp in preedit_v.codepoints[range.cp_offset..].iter() {
                    if let Err(_err) = self.add_preedit_cell(
                        *cp,
                        point::Coordinate {
                            x,
                            y: range.y as u32,
                        },
                        hoisted_foreground,
                    ) {
                        // log.warn("error building preedit cell, will be invalid
                        //           x={} y={}, err={}", .{ x, range.y, err });
                    }

                    x += if cp.wide { 2 } else { 1 };
                }
            }
        }

        // Update that our cells rebuilt
        self.cells_rebuilt = true;

        // Log some things
        // log.debug("rebuildCells complete cached_runs={}", .{
        //     self.font_shaper_cache.count(),
        // });
    }

    // fn rebuildRow(self, y, row, cells: *MultiArrayList(Cell), preedit_range,
    //               selection, highlights: *const ArrayList(Highlight), links) !void
    // PORT NOTE (reshape): `cells: *MultiArrayList(Cell)` -> `*const CellList`.
    //
    // SAFETY (§C.3(2,3)): `cells`/`highlights` are HEAP-backed raw ptrs (the `Vec`
    // backing buffers of `row_data[y]`), SB-SAFE across this method's sub-calls
    // (a different allocation from the `Renderer`). The one INLINE-self leg is the
    // shaper hook: `run_iter`'s `RunIteratorHook { shaper: *mut Shaper }` aliases
    // inline `self.font_shaper` and must stay live across the whole run loop.
    //
    // A `&mut self`-receiver `self.font_shaper.shape(...)` (a SIBLING reborrow of
    // `font_shaper`) or `self.add_glyph(...)` (a WHOLE-`Renderer` `&mut *self`
    // receiver reborrow, which covers `font_shaper`) POPS the hook's tag —
    // Undefined Behavior under BOTH Stacked and Tree Borrows, permanently (the tag
    // is not restored when the transient borrow ends). This was proven on an
    // isolated, non-FFI Miri harness reproducing this exact borrow structure. So
    // the loop is kept SB/TB-clean (the `self_raw` established below) by:
    //   (a) `run_iterator` returns an iterator holding only raw ptrs (the hook + a
    //       synthetic-lifetime `&[render::Cell]` from `cells`), borrowing no `self`;
    //   (b) `shape` is invoked THROUGH the hook's own raw ptr
    //       `run_iter.hooks.shaper` — a CHILD reborrow of the hook, which does not
    //       pop it — never `self.font_shaper.shape`;
    //   (c) the `add_*` helpers are RAW-self associated fns called via `self_raw`,
    //       using only NARROW field access (never a whole `&mut *self_raw`); and
    //   (d) direct `self.field` access in the loop body (`self.cells.bg_cell`,
    //       `self.uniforms`, `self.font_shaper_cache`) is disjoint from
    //       `font_shaper`, so it retags a different location and leaves the hook.
    // (GPU_RD_CHUNK3B_DESIGN §C.3 remedy 2; the Metal-gated test cannot run Miri,
    // so this is registered in MIRI_EXCEPTIONS.md — but, unlike an FFI exception,
    // the borrow structure IS SB/TB-clean by construction, harness-proven.)
    // The `shaper_cells` raw ptr is a heap slice into the shaper `cell_buf` / cache
    // Box (§C.3(3)): captured immediately after `shape`/`get`, consumed within the
    // same run's glyph loop, `put` COPIES (does not free) it, and `run_iter.next`
    // nulls it before the next `shape` overwrites the buffer.
    // (`too_many_arguments`: faithful to the reference `rebuildRow` arg list.)
    #[allow(clippy::too_many_arguments)]
    fn rebuild_row(
        &mut self,
        y: CellCountInt,
        row: page::Row,
        cells: *const render::CellList,
        preedit_range: Option<PreeditRange>,
        selection: Option<[CellCountInt; 2]>,
        highlights: *const [Highlight],
        links: &CellSet,
    ) -> Result<(), RebuildRowError> {
        // const state = &self.terminal_state;
        let state: *const RenderState = &self.terminal_state;

        // §C.3 REMEDY 1: hoist the inline `terminal_state` scalars the per-cell
        // color resolution reads, BEFORE the shape loop's `&mut *self` sub-calls.
        let palette = unsafe { (*state).colors.palette };
        let foreground = unsafe { (*state).colors.foreground };
        let background = unsafe { (*state).colors.background };
        let cols = unsafe { (*state).cols };

        // If our viewport is wider than our cell contents buffer,
        // we still only process cells up to the width of the buffer.
        // (`cells_ref`/its columns are HEAP-backed — the `Vec` backing buffers
        // of `row_data[y].cells` — so they survive `&mut *self`; §C.3(2).)
        let cells_ref: &render::CellList = unsafe { &*cells };
        let cells_slice: render::CellSlice = cells_ref.slice();
        // PORT NOTE (reshape): the reference reads two SoA projections
        // `cells.items(.raw)` / `.items(.style)`; rustty's cells are the SoA
        // `CellList`, so `cells_raw` IS the `raw` column (`cells_slice.raw`, a
        // borrow into `row_data`'s heap — a separate allocation from `Renderer`,
        // not one of the §C.3 raw-pointer triad) and `cells_slice.style[x]` is
        // the `.items(.style)` projection.
        let cells_raw: &[page::Cell] = cells_slice.raw;
        let cells_len = core::cmp::min(cells_raw.len(), self.cells.size.columns as usize);

        // On primary screen, we still apply vertical padding
        // extension under certain conditions we feel are safe.
        //
        // This helps make some scenarios look better while
        // avoiding scenarios we know do NOT look good.
        match self.config.padding_color {
            // These already have the correct values set above.
            WindowPaddingColor::Background | WindowPaddingColor::ExtendAlways => {}

            // Apply heuristics for padding extension.
            WindowPaddingColor::Extend => {
                if y == 0 {
                    self.uniforms.padding_extend.up =
                        !never_extend_bg(row, cells_raw, cells_slice.style, &palette, background);
                } else if y == self.cells.size.rows - 1 {
                    self.uniforms.padding_extend.down =
                        !never_extend_bg(row, cells_raw, cells_slice.style, &palette, background);
                }
            }
        }

        // Iterator of runs for shaping.
        let mut run_iter_opts: shape::RunOptions = shape::RunOptions {
            grid: self.font_grid,
            cells: cells_slice,
            selection,

            // We want to do font shaping as long as the cursor is
            // visible on this viewport.
            cursor_x: 'cursor_x: {
                let vp = match unsafe { (*state).cursor.viewport.as_ref() } {
                    Some(vp) => vp,
                    None => break 'cursor_x None,
                };
                if vp.y != y {
                    break 'cursor_x None;
                }
                Some(vp.x as usize)
            },
        };
        run_iter_opts.apply_break_config(shape::FontShapingBreak {
            cursor: self.config.font_shaping_break.cursor,
        });
        // Raw `*mut Renderer` used for the interleaved cell-fill helpers below
        // (§C.3 shaper-hook remedy): `run_iter` holds a `RunIteratorHook`'s raw
        // `*mut Shaper` alias into `self.font_shaper` that MUST stay live across the
        // whole run loop, but each `self.add_glyph(...)`/`add_underline(...)` call
        // and each `self.font_shaper.shape(...)` would, as a `&mut self` reborrow,
        // pop that hook tag (Miri-proven UB, SB + TB). So: (a) the `add_*` helpers
        // are taken RAW via `self_raw` (narrow field access only — no whole
        // `&mut *self_raw`), and (b) `shape` is invoked through the hook's OWN raw
        // pointer `run_iter.hooks.shaper` (a child reborrow of the hook, which does
        // not pop it), NOT through `self.font_shaper` (a sibling reborrow, which
        // does). Narrow `self.field` access (`self.cells.bg_cell`, `self.uniforms`,
        // `self.font_shaper_cache`) is disjoint from `font_shaper` and stays safe.
        let self_raw: *mut Renderer = self;
        let mut run_iter = self.font_shaper.run_iterator(run_iter_opts);
        let mut shaper_run: Option<shape::TextRun> = run_iter.next(self.alloc)?;
        let mut shaper_cells: Option<*const [shape::Cell]> = None;
        let mut shaper_cells_i: usize = 0;

        'cells: for x in 0..cells_len {
            let cell: &page::Cell = &cells_raw[x];
            let managed_style: Style = cells_slice.style[x];

            // If this cell falls within our preedit range then we
            // skip this because preedits are setup separately.
            if let Some(range) = preedit_range {
                'preedit: {
                    // We're not on the preedit line, no actions necessary.
                    if range.y != y {
                        break 'preedit;
                    }
                    // We're before the preedit range, no actions necessary.
                    if (x as CellCountInt) < range.x[0] {
                        break 'preedit;
                    }
                    // We're in the preedit range, skip this cell.
                    if (x as CellCountInt) <= range.x[1] {
                        continue 'cells;
                    }
                    // After exiting the preedit range we need to catch
                    // the run position up because of the missed cells.
                    // In all other cases, no action is necessary.
                    if (x as CellCountInt) != range.x[1] + 1 {
                        break 'preedit;
                    }

                    // Step the run iterator until we find a run that ends
                    // after the current cell, which will be the soonest run
                    // that might contain glyphs for our cell.
                    while let Some(run) = shaper_run {
                        if run.offset as usize + run.cells as usize > x {
                            break;
                        }
                        shaper_run = run_iter.next(self.alloc)?;
                        shaper_cells = None;
                        shaper_cells_i = 0;
                    }

                    let run = match shaper_run {
                        Some(r) => r,
                        None => break 'preedit,
                    };

                    // If we haven't shaped this run, do so now.
                    if shaper_cells.is_none() {
                        shaper_cells = match self.font_shaper_cache.get(&run) {
                            // Try to read the cells from the shaping cache if we can.
                            Some(c) => Some(c as *const [shape::Cell]),
                            None => {
                                // Otherwise we have to shape them.
                                // SAFETY (§C.3 shaper-hook remedy): shape THROUGH the
                                // hook's own raw `*mut Shaper` (`run_iter.hooks.
                                // shaper`), a child reborrow that does NOT pop the
                                // hook — a `self.font_shaper.shape(...)` sibling
                                // reborrow WOULD pop it (Miri-proven UB). Same Shaper
                                // object, byte-identical to the reference `self.
                                // font_shaper.shape(run)`.
                                let new_cells = unsafe { (*run_iter.hooks.shaper).shape(run) }?;
                                let new_ptr = new_cells as *const [shape::Cell];

                                // Try to cache them. If caching fails for any reason we
                                // continue because it is just a performance optimization,
                                // not a correctness issue.
                                self.font_shaper_cache.put(&run, unsafe { &*new_ptr });

                                // The cells we get from direct shaping are always owned
                                // by the shaper and valid until the next shaping call so
                                // we can safely use them.
                                Some(new_ptr)
                            }
                        };
                    }

                    // Advance our index until we reach or pass
                    // our current x position in the shaper cells.
                    let shaper_cells_unwrapped = unsafe { &*shaper_cells.unwrap() };
                    while (run.offset as usize + shaper_cells_unwrapped[shaper_cells_i].x as usize)
                        < x
                    {
                        shaper_cells_i += 1;
                    }
                }
            }

            let wide = cell.wide();
            let style: Style = if cell.has_styling() {
                managed_style
            } else {
                Style::default()
            };

            // True if this cell is selected
            let selected: Selected = 'selected: {
                // Order below matters for precedence.

                // Selection should take the highest precedence.
                let x_compare: usize = if wide == page::Wide::SpacerTail {
                    x.saturating_sub(1)
                } else {
                    x
                };
                if let Some(sel) = selection {
                    if x_compare >= sel[0] as usize && x_compare <= sel[1] as usize {
                        break 'selected Selected::Selection;
                    }
                }

                // If we're highlighted, then we're selected. In the
                // future we want to use a different style for this
                // but this to get started.
                for hl in unsafe { &*highlights }.iter() {
                    if x_compare >= hl.range[0] as usize && x_compare <= hl.range[1] as usize {
                        let tag: HighlightTag = match hl.tag {
                            0 => HighlightTag::SearchMatch,
                            1 => HighlightTag::SearchMatchSelected,
                            other => unreachable!("invalid HighlightTag {other}"),
                        };
                        break 'selected match tag {
                            HighlightTag::SearchMatch => Selected::Search,
                            HighlightTag::SearchMatchSelected => Selected::SearchSelected,
                        };
                    }
                }

                Selected::False
            };

            // The `_style` suffixed values are the colors based on
            // the cell style (SGR), before applying any additional
            // configuration, inversions, selections, etc.
            let bg_style: Option<color::RGB> = style.bg(cell, &palette);
            let fg_style: color::RGB = style.fg(Fg {
                default: foreground,
                palette: &palette,
                bold: self.config.bold_color,
            });

            // The final background color for the cell.
            let bg: Option<color::RGB> = match selected {
                // If we have an explicit selection background color
                // specified in the config, use that.
                //
                // If no configuration, then our selection background
                // is our foreground color.
                Selected::Selection => match self.config.selection_background {
                    Some(v) => match v {
                        TerminalColor::Color(color) => Some(color.to_terminal_rgb()),
                        TerminalColor::CellForeground => {
                            if style.flags.inverse {
                                bg_style
                            } else {
                                Some(fg_style)
                            }
                        }
                        TerminalColor::CellBackground => {
                            if style.flags.inverse {
                                Some(fg_style)
                            } else {
                                bg_style
                            }
                        }
                    },
                    None => Some(foreground),
                },

                Selected::Search => match self.config.search_background {
                    TerminalColor::Color(color) => Some(color.to_terminal_rgb()),
                    TerminalColor::CellForeground => {
                        if style.flags.inverse {
                            bg_style
                        } else {
                            Some(fg_style)
                        }
                    }
                    TerminalColor::CellBackground => {
                        if style.flags.inverse {
                            Some(fg_style)
                        } else {
                            bg_style
                        }
                    }
                },

                Selected::SearchSelected => match self.config.search_selected_background {
                    TerminalColor::Color(color) => Some(color.to_terminal_rgb()),
                    TerminalColor::CellForeground => {
                        if style.flags.inverse {
                            bg_style
                        } else {
                            Some(fg_style)
                        }
                    }
                    TerminalColor::CellBackground => {
                        if style.flags.inverse {
                            Some(fg_style)
                        } else {
                            bg_style
                        }
                    }
                },

                // Not selected
                Selected::False => {
                    if style.flags.inverse != cell::is_covering(cell.codepoint()) {
                        // Two cases cause us to invert (use the fg color as the bg)
                        // - The "inverse" style flag.
                        // - A "covering" glyph; we use fg for bg in that
                        //   case to help make sure that padding extension
                        //   works correctly.
                        //
                        // If one of these is true (but not the other)
                        // then we use the fg style color for the bg.
                        Some(fg_style)
                    } else {
                        // Otherwise they cancel out.
                        bg_style
                    }
                }
            };

            let fg: color::RGB = {
                // Our happy-path non-selection background color
                // is our style or our configured defaults.
                let final_bg = bg_style.unwrap_or(background);

                // Whether we need to use the bg color as our fg color:
                // - Cell is selected, inverted, and set to cell-foreground
                // - Cell is selected, not inverted, and set to cell-background
                // - Cell is inverted and not selected
                match selected {
                    Selected::Selection => match self.config.selection_foreground {
                        Some(v) => match v {
                            TerminalColor::Color(color) => color.to_terminal_rgb(),
                            TerminalColor::CellForeground => {
                                if style.flags.inverse {
                                    final_bg
                                } else {
                                    fg_style
                                }
                            }
                            TerminalColor::CellBackground => {
                                if style.flags.inverse {
                                    fg_style
                                } else {
                                    final_bg
                                }
                            }
                        },
                        None => background,
                    },

                    Selected::Search => match self.config.search_foreground {
                        TerminalColor::Color(color) => color.to_terminal_rgb(),
                        TerminalColor::CellForeground => {
                            if style.flags.inverse {
                                final_bg
                            } else {
                                fg_style
                            }
                        }
                        TerminalColor::CellBackground => {
                            if style.flags.inverse {
                                fg_style
                            } else {
                                final_bg
                            }
                        }
                    },

                    Selected::SearchSelected => match self.config.search_selected_foreground {
                        TerminalColor::Color(color) => color.to_terminal_rgb(),
                        TerminalColor::CellForeground => {
                            if style.flags.inverse {
                                final_bg
                            } else {
                                fg_style
                            }
                        }
                        TerminalColor::CellBackground => {
                            if style.flags.inverse {
                                fg_style
                            } else {
                                final_bg
                            }
                        }
                    },

                    Selected::False => {
                        if style.flags.inverse {
                            final_bg
                        } else {
                            fg_style
                        }
                    }
                }
            };

            // Foreground alpha for this cell.
            let alpha: u8 = if style.flags.faint {
                self.config.faint_opacity
            } else {
                255
            };

            // Set the cell's background color.
            {
                let rgb = bg.unwrap_or(background);

                // Determine our background alpha. If we have transparency configured
                // then this is dynamic depending on some situations. This is all
                // in an attempt to make transparency look the best for various
                // situations. See inline comments.
                let bg_alpha: u8 = 'bg_alpha: {
                    let default: u8 = 255;

                    // Cells that are selected should be fully opaque.
                    if selected != Selected::False {
                        break 'bg_alpha default;
                    }

                    // Cells that are reversed should be fully opaque.
                    if style.flags.inverse {
                        break 'bg_alpha default;
                    }

                    // If the user requested to have opacity on all cells, apply it.
                    if self.config.background_opacity_cells && bg_style.is_some() {
                        let mut opacity: f64 = default as f64;
                        opacity *= self.config.background_opacity;
                        break 'bg_alpha opacity as u8;
                    }

                    // Cells that have an explicit bg color should be fully opaque.
                    if bg_style.is_some() {
                        break 'bg_alpha default;
                    }

                    // Otherwise, we won't draw the bg for this cell,
                    // we'll let the already-drawn background color
                    // show through.
                    0
                };

                *self.cells.bg_cell(y as usize, x) = [rgb.r, rgb.g, rgb.b, bg_alpha];
            }

            // If the invisible flag is set on this cell then we
            // don't need to render any foreground elements, so
            // we just skip all glyphs with this x coordinate.
            //
            // NOTE: This behavior matches xterm. Some other terminal
            // emulators, e.g. Alacritty, still render text decorations
            // and only make the text itself invisible. The decision
            // has been made here to match xterm's behavior for this.
            if style.flags.invisible {
                continue;
            }

            // Give links a single underline, unless they already have
            // an underline, in which case use a double underline to
            // distinguish them.
            let underline: sgr::Underline = 'underline: {
                if links.contains(&point::Coordinate {
                    x: x as CellCountInt,
                    y: y as u32,
                }) {
                    break 'underline if style.flags.underline == sgr::Underline::Single {
                        sgr::Underline::Double
                    } else {
                        sgr::Underline::Single
                    };
                }
                style.flags.underline
            };

            // We draw underlines first so that they layer underneath text.
            // This improves readability when a colored underline is used
            // which intersects parts of the text (descenders).
            if underline != sgr::Underline::None {
                // SAFETY (§C.3): raw-self helper — see `self_raw`/`add_underline`.
                if let Err(_err) = unsafe {
                    Renderer::add_underline(
                        self_raw,
                        x as CellCountInt,
                        y,
                        underline,
                        style.underline_color(&palette).unwrap_or(fg),
                        alpha,
                    )
                } {
                    // log.warn("error adding underline to cell, will be invalid
                    //           x={} y={}, err={}", .{ x, y, err });
                }
            }

            if style.flags.overline {
                // SAFETY (§C.3): raw-self helper — see `self_raw`/`add_underline`.
                if let Err(_err) =
                    unsafe { Renderer::add_overline(self_raw, x as CellCountInt, y, fg, alpha) }
                {
                    // log.warn("error adding overline to cell, will be invalid
                    //           x={} y={}, err={}", .{ x, y, err });
                }
            }

            // If we're at or past the end of our shaper run then
            // we need to get the next run from the run iterator.
            if shaper_cells.is_some() && shaper_cells_i >= unsafe { &*shaper_cells.unwrap() }.len()
            {
                shaper_run = run_iter.next(self.alloc)?;
                shaper_cells = None;
                shaper_cells_i = 0;
            }

            if let Some(run) = shaper_run {
                'glyphs: {
                    // If we haven't shaped this run yet, do so.
                    if shaper_cells.is_none() {
                        shaper_cells = match self.font_shaper_cache.get(&run) {
                            // Try to read the cells from the shaping cache if we can.
                            Some(c) => Some(c as *const [shape::Cell]),
                            None => {
                                // Otherwise we have to shape them.
                                // SAFETY (§C.3 shaper-hook remedy): shape THROUGH the
                                // hook's own raw `*mut Shaper` (`run_iter.hooks.
                                // shaper`), a child reborrow that does NOT pop the
                                // hook — a `self.font_shaper.shape(...)` sibling
                                // reborrow WOULD pop it (Miri-proven UB). Same Shaper
                                // object, byte-identical to the reference `self.
                                // font_shaper.shape(run)`.
                                let new_cells = unsafe { (*run_iter.hooks.shaper).shape(run) }?;
                                let new_ptr = new_cells as *const [shape::Cell];

                                // Try to cache them. If caching fails for any reason we
                                // continue because it is just a performance optimization,
                                // not a correctness issue.
                                self.font_shaper_cache.put(&run, unsafe { &*new_ptr });

                                // The cells we get from direct shaping are always owned
                                // by the shaper and valid until the next shaping call so
                                // we can safely use them.
                                Some(new_ptr)
                            }
                        };
                    }

                    let shaped_cells: &[shape::Cell] = match shaper_cells {
                        Some(p) => unsafe { &*p },
                        None => break 'glyphs,
                    };

                    // If there are no shaper cells for this run, ignore it.
                    // This can occur for runs of empty cells, and is fine.
                    if shaped_cells.is_empty() {
                        break 'glyphs;
                    }

                    // If we encounter a shaper cell to the left of the current
                    // cell then we have some problems. This logic relies on x
                    // position monotonically increasing.
                    assert!(run.offset as usize + shaped_cells[shaper_cells_i].x as usize >= x);

                    // NOTE: An assumption is made here that a single cell will never
                    // be present in more than one shaper run. If that assumption is
                    // violated, this logic breaks.

                    while shaper_cells_i < shaped_cells.len()
                        && run.offset as usize + shaped_cells[shaper_cells_i].x as usize == x
                    {
                        // SAFETY (§C.3): raw-self helper — see `self_raw`/`add_glyph`.
                        if let Err(_err) = unsafe {
                            Renderer::add_glyph(
                                self_raw,
                                x as CellCountInt,
                                y,
                                cols as usize,
                                cells_raw,
                                shaped_cells[shaper_cells_i],
                                shaper_run.unwrap(),
                                fg,
                                alpha,
                            )
                        } {
                            // log.warn("error adding glyph to cell, will be invalid
                            //           x={} y={}, err={}", .{ x, y, err });
                        }
                        shaper_cells_i += 1;
                    }
                }
            }

            // Finally, draw a strikethrough if necessary.
            if style.flags.strikethrough {
                // SAFETY (§C.3): raw-self helper — see `self_raw`/`add_underline`.
                if let Err(_err) = unsafe {
                    Renderer::add_strikethrough(self_raw, x as CellCountInt, y, fg, alpha)
                } {
                    // log.warn("error adding strikethrough to cell, will be invalid
                    //           x={} y={}, err={}", .{ x, y, err });
                }
            }
        }

        Ok(())
    }

    /// Add an underline decoration to the specified cell
    // PORT NOTE (§C.3 aliasing remedy): taken RAW (`self_raw: *mut Renderer`),
    // NOT `&mut self`, mirroring the reference `addUnderline(self: *Self, ...)`.
    // It is called from `rebuild_row`'s shaper run loop while the
    // `RunIteratorHook`'s raw `*mut Shaper` alias into `self.font_shaper` is still
    // live; a `&mut self` receiver reborrow would retag the whole `Renderer`
    // (font_shaper included) and pop that hook tag — Undefined Behavior under both
    // Stacked and Tree Borrows (Miri-proven). Taken raw with only NARROW field
    // access (`(*self_raw).field`, never a whole `&mut *self_raw`), it leaves the
    // hook tag intact. See the §C.3 SAFETY block on `rebuild_row`.
    unsafe fn add_underline(
        self_raw: *mut Renderer,
        x: CellCountInt,
        y: CellCountInt,
        style: sgr::Underline,
        color: color::RGB,
        alpha: u8,
    ) -> Result<(), RenderError> {
        let sprite: Sprite = match style {
            sgr::Underline::None => unreachable!(),
            sgr::Underline::Single => Sprite::Underline,
            sgr::Underline::Double => Sprite::UnderlineDouble,
            sgr::Underline::Dotted => Sprite::UnderlineDotted,
            sgr::Underline::Dashed => Sprite::UnderlineDashed,
            sgr::Underline::Curly => Sprite::UnderlineCurly,
        };

        // SAFETY: `self.font_grid` is the shared `*mut SharedGrid` handle (the
        // `Options.font_grid` decision); the grid owns an internal `RwLock`, so
        // the `&mut` through the raw ptr is the reference `self.font_grid.
        // renderGlyph` exactly. `(*self_raw).{font_grid,alloc,grid_metrics}` are
        // NARROW reads of disjoint fields (never a whole-`self` reborrow).
        let render = unsafe {
            (*(*self_raw).font_grid).render_glyph(
                (*self_raw).alloc,
                SPRITE_INDEX,
                sprite as u32,
                RenderOptions {
                    cell_width: Some(1),
                    grid_metrics: (*self_raw).grid_metrics,
                    ..RenderOptions::default()
                },
            )
        }?;

        // SAFETY: narrow `&mut (*self_raw).cells` (the `Contents` field), disjoint
        // from `self.font_shaper` — does not pop the caller's live shaper hook.
        unsafe {
            (*self_raw).cells.add(
                Key::Underline,
                CellText {
                    atlas: Atlas::Grayscale,
                    grid_pos: [x, y],
                    color: [color.r, color.g, color.b, alpha],
                    glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                    glyph_size: [render.glyph.width, render.glyph.height],
                    bearings: [render.glyph.offset_x as i16, render.glyph.offset_y as i16],
                    ..Default::default()
                },
            );
        }
        Ok(())
    }

    /// Add a overline decoration to the specified cell
    // PORT NOTE: raw-self (`self_raw: *mut Renderer`) — see `add_underline` (§C.3):
    // called from the shaper run loop, so a `&mut self` receiver would pop the live
    // hook. Narrow field access only.
    unsafe fn add_overline(
        self_raw: *mut Renderer,
        x: CellCountInt,
        y: CellCountInt,
        color: color::RGB,
        alpha: u8,
    ) -> Result<(), RenderError> {
        // SAFETY: see `add_underline` — narrow `(*self_raw).{font_grid,alloc,
        // grid_metrics}` reads; the grid handle owns its own `RwLock`.
        let render = unsafe {
            (*(*self_raw).font_grid).render_glyph(
                (*self_raw).alloc,
                SPRITE_INDEX,
                Sprite::Overline as u32,
                RenderOptions {
                    cell_width: Some(1),
                    grid_metrics: (*self_raw).grid_metrics,
                    ..RenderOptions::default()
                },
            )
        }?;

        // SAFETY: narrow `&mut (*self_raw).cells`, disjoint from `self.font_shaper`.
        unsafe {
            (*self_raw).cells.add(
                Key::Overline,
                CellText {
                    atlas: Atlas::Grayscale,
                    grid_pos: [x, y],
                    color: [color.r, color.g, color.b, alpha],
                    glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                    glyph_size: [render.glyph.width, render.glyph.height],
                    bearings: [render.glyph.offset_x as i16, render.glyph.offset_y as i16],
                    ..Default::default()
                },
            );
        }
        Ok(())
    }

    /// Add a strikethrough decoration to the specified cell
    // PORT NOTE: raw-self (`self_raw: *mut Renderer`) — see `add_underline` (§C.3):
    // called from the shaper run loop, so a `&mut self` receiver would pop the live
    // hook. Narrow field access only.
    unsafe fn add_strikethrough(
        self_raw: *mut Renderer,
        x: CellCountInt,
        y: CellCountInt,
        color: color::RGB,
        alpha: u8,
    ) -> Result<(), RenderError> {
        // SAFETY: see `add_underline` — narrow `(*self_raw).{font_grid,alloc,
        // grid_metrics}` reads; the grid handle owns its own `RwLock`.
        let render = unsafe {
            (*(*self_raw).font_grid).render_glyph(
                (*self_raw).alloc,
                SPRITE_INDEX,
                Sprite::Strikethrough as u32,
                RenderOptions {
                    cell_width: Some(1),
                    grid_metrics: (*self_raw).grid_metrics,
                    ..RenderOptions::default()
                },
            )
        }?;

        // SAFETY: narrow `&mut (*self_raw).cells`, disjoint from `self.font_shaper`.
        unsafe {
            (*self_raw).cells.add(
                Key::Strikethrough,
                CellText {
                    atlas: Atlas::Grayscale,
                    grid_pos: [x, y],
                    color: [color.r, color.g, color.b, alpha],
                    glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                    glyph_size: [render.glyph.width, render.glyph.height],
                    bearings: [render.glyph.offset_x as i16, render.glyph.offset_y as i16],
                    ..Default::default()
                },
            );
        }
        Ok(())
    }

    // Add a glyph to the specified cell.
    // PORT NOTE: raw-self (`self_raw: *mut Renderer`), mirroring the reference
    // `addGlyph(self: *Self, ...)` — see `add_underline` (§C.3). It is called from
    // the shaper run loop, so a `&mut self` receiver reborrow would pop the live
    // `RunIteratorHook` alias into `self.font_shaper` (Miri-proven UB, SB + TB).
    // Narrow field access only. (`too_many_arguments`: faithful to the reference
    // `addGlyph` arg list.)
    #[allow(clippy::too_many_arguments)]
    unsafe fn add_glyph(
        self_raw: *mut Renderer,
        x: CellCountInt,
        y: CellCountInt,
        cols: usize,
        // PORT NOTE (reshape, §D.5): a plain safe `&[page::Cell]` — the SoA
        // `raw` column projection (`cells_slice.raw`) in `rebuild_row`. It is a
        // shared borrow into `row_data`'s heap (a separate allocation from
        // `Renderer` and never mutated here), so it is NOT one of the §C.3
        // raw-pointer triad and coexists with the narrow `&mut self` field
        // access below. This mirrors the reference passing `cells.items(.raw)`.
        cell_raws: &[page::Cell],
        shaper_cell: shape::Cell,
        shaper_run: shape::TextRun,
        color: color::RGB,
        alpha: u8,
    ) -> Result<(), RenderError> {
        let cell = cell_raws[x as usize];
        let cp = cell.codepoint();

        // Render
        // SAFETY: see `add_underline` — narrow `(*self_raw).{font_grid,alloc,
        // grid_metrics,config}` reads; the grid handle owns its own `RwLock`.
        let render = unsafe {
            (*(*self_raw).font_grid).render_glyph(
                (*self_raw).alloc,
                shaper_run.font_index,
                shaper_cell.glyph_index,
                RenderOptions {
                    grid_metrics: (*self_raw).grid_metrics,
                    thicken: (*self_raw).config.font_thicken,
                    thicken_strength: (*self_raw).config.font_thicken_strength,
                    cell_width: Some(cell.grid_width()),
                    // If there's no Nerd Font constraint for this codepoint
                    // then, if it's a symbol, we constrain it to fit inside
                    // its cell(s), we don't modify the alignment at all.
                    constraint: get_constraint(cp).unwrap_or_else(|| {
                        if cell::is_symbol(cp) {
                            Constraint {
                                size: ConstraintSize::Fit,
                                ..Constraint::none
                            }
                        } else {
                            Constraint::none
                        }
                    }),
                    constraint_width: cell::constraint_width(cell_raws, x as usize, cols),
                },
            )
        }?;

        // If the glyph is 0 width or height, it will be invisible
        // when drawn, so don't bother adding it to the buffer.
        if render.glyph.width == 0 || render.glyph.height == 0 {
            return Ok(());
        }

        // SAFETY: narrow `&mut (*self_raw).cells`, disjoint from `self.font_shaper`.
        unsafe {
            (*self_raw).cells.add(
                Key::Text,
                CellText {
                    atlas: match render.presentation {
                        Presentation::Emoji => Atlas::Color,
                        Presentation::Text => Atlas::Grayscale,
                    },
                    bools: CellTextBools {
                        no_min_contrast: cell::no_min_contrast(cp),
                        ..Default::default()
                    },
                    grid_pos: [x, y],
                    color: [color.r, color.g, color.b, alpha],
                    glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                    glyph_size: [render.glyph.width, render.glyph.height],
                    bearings: [
                        (render.glyph.offset_x + shaper_cell.x_offset as i32) as i16,
                        (render.glyph.offset_y + shaper_cell.y_offset as i32) as i16,
                    ],
                },
            );
        }
        Ok(())
    }

    // fn addCursor(self, cursor_state: *const RenderState.Cursor,
    //              cursor_style: renderer.CursorStyle, cursor_color: RGB) void
    // PORT NOTE (§C.3/§D.6): BOTH `self` and `cursor_state` are RAW, exactly as in
    // the reference `addCursor(self: *Self, cursor_state: *const RenderState.Cursor,
    // ...)` (ref :3226). The caller's `&st.cursor` aliases INLINE
    // `self.terminal_state.cursor`; taking `self` as `&mut self` here would retag
    // the whole `Renderer` on the receiver reborrow and POP that `cursor_state`
    // alias BEFORE this body dereferences it — Undefined Behavior under Stacked
    // Borrows (Miri-proven). Taken raw, `self` and `cursor_state` coexist as two raw
    // pointers, exactly as the reference. All `self` access below is NARROW
    // (`(*self_raw).field`), never a whole `&mut *self_raw`.
    unsafe fn add_cursor(
        self_raw: *mut Renderer,
        cursor_state: *const render::Cursor,
        cursor_style: cursor::Style,
        cursor_color: color::RGB,
    ) {
        // const cursor_vp = cursor_state.viewport orelse return;
        // (`Viewport` is not `Copy`; copy out its three scalars by value.)
        let (vp_x, vp_y, vp_wide_tail) = match unsafe { (*cursor_state).viewport.as_ref() } {
            Some(v) => (v.x, v.y, v.wide_tail),
            None => return,
        };
        let cell_wide = unsafe { (*cursor_state).cell.wide() };

        // Add the cursor. We render the cursor over the wide character if
        // we're on the wide character tail.
        let (wide, x): (bool, CellCountInt) = if !vp_wide_tail {
            // The cursor goes over the screen cursor position.
            (cell_wide == page::Wide::Wide, vp_x)
        } else {
            // If we're part of a wide character, we move the cursor back
            // to the actual character.
            (true, vp_x - 1)
        };

        // SAFETY: narrow reads of `(*self_raw).{focused,config}` (see §C.3).
        let alpha: u8 = if !unsafe { (*self_raw).focused } {
            255
        } else {
            (255.0 * unsafe { (*self_raw).config.cursor_opacity }).ceil() as u8
        };

        let render = match cursor_style {
            cursor::Style::Block
            | cursor::Style::BlockHollow
            | cursor::Style::Bar
            | cursor::Style::Underline => {
                let sprite: Sprite = match cursor_style {
                    cursor::Style::Block => Sprite::CursorRect,
                    cursor::Style::BlockHollow => Sprite::CursorHollowRect,
                    cursor::Style::Bar => Sprite::CursorBar,
                    cursor::Style::Underline => Sprite::CursorUnderline,
                    cursor::Style::Lock => unreachable!(),
                };

                // SAFETY: see `add_underline` — narrow `(*self_raw).{font_grid,
                // alloc,grid_metrics}` reads; the grid handle owns its own `RwLock`.
                match unsafe {
                    (*(*self_raw).font_grid).render_glyph(
                        (*self_raw).alloc,
                        SPRITE_INDEX,
                        sprite as u32,
                        RenderOptions {
                            cell_width: Some(if wide { 2 } else { 1 }),
                            grid_metrics: (*self_raw).grid_metrics,
                            ..RenderOptions::default()
                        },
                    )
                } {
                    Ok(r) => r,
                    Err(_err) => {
                        // log.warn("error rendering cursor glyph err={}", .{err});
                        return;
                    }
                }
            }

            cursor::Style::Lock => {
                // SAFETY: see `add_underline` — narrow `(*self_raw).{font_grid,
                // alloc,grid_metrics}` reads; the grid handle owns its own `RwLock`.
                match unsafe {
                    (*(*self_raw).font_grid).render_codepoint(
                        (*self_raw).alloc,
                        0xF023, // lock symbol
                        rustty_font_shaper::Style::Regular,
                        Some(Presentation::Text),
                        RenderOptions {
                            cell_width: Some(if wide { 2 } else { 1 }),
                            grid_metrics: (*self_raw).grid_metrics,
                            ..RenderOptions::default()
                        },
                    )
                } {
                    Ok(Some(r)) => r,
                    Ok(None) => {
                        // This should never happen because we embed nerd
                        // fonts so we just log and return instead of fallback.
                        // log.warn("failed to find lock symbol for cursor
                        //           codepoint=0xF023", .{});
                        return;
                    }
                    Err(_err) => {
                        // log.warn("error rendering cursor glyph err={}", .{err});
                        return;
                    }
                }
            }
        };

        // SAFETY: narrow `&mut (*self_raw).cells`, disjoint from `self.font_shaper`.
        unsafe {
            (*self_raw).cells.set_cursor(
                Some(CellText {
                    atlas: Atlas::Grayscale,
                    bools: CellTextBools {
                        is_cursor_glyph: true,
                        ..Default::default()
                    },
                    grid_pos: [x, vp_y],
                    color: [cursor_color.r, cursor_color.g, cursor_color.b, alpha],
                    glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                    glyph_size: [render.glyph.width, render.glyph.height],
                    bearings: [render.glyph.offset_x as i16, render.glyph.offset_y as i16],
                }),
                Some(cursor_style),
            );
        }
    }

    // fn addPreeditCell(self, cp: State.Preedit.Codepoint, coord: terminal.Coordinate,
    //                   screen_fg: RGB) !void
    fn add_preedit_cell(
        &mut self,
        cp: state::Codepoint,
        coord: point::Coordinate,
        screen_fg: color::RGB,
    ) -> Result<(), RenderError> {
        // Render the glyph for our preedit text
        // SAFETY: see `add_underline` — `self.font_grid` raw-ptr deref.
        let render = match unsafe { &mut *self.font_grid }.render_codepoint(
            self.alloc,
            cp.codepoint,
            rustty_font_shaper::Style::Regular,
            Some(Presentation::Text),
            RenderOptions {
                grid_metrics: self.grid_metrics,
                ..RenderOptions::default()
            },
        ) {
            Ok(Some(r)) => r,
            Ok(None) => {
                // log.warn("failed to find font for preedit codepoint={X}",
                //          .{cp.codepoint});
                return Ok(());
            }
            Err(_err) => {
                // log.warn("error rendering preedit glyph err={}", .{err});
                return Ok(());
            }
        };

        // Add our text
        self.cells.add(
            Key::Text,
            CellText {
                atlas: Atlas::Grayscale,
                grid_pos: [coord.x, coord.y as u16],
                color: [screen_fg.r, screen_fg.g, screen_fg.b, 255],
                glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
                glyph_size: [render.glyph.width, render.glyph.height],
                bearings: [render.glyph.offset_x as i16, render.glyph.offset_y as i16],
                ..Default::default()
            },
        );

        // Add underline
        // SAFETY (§C.3): `add_underline` is a raw-self helper; `add_preedit_cell`
        // holds no live inline-self alias (no shaper hook), so a fresh `self as
        // *mut Renderer` with narrow field access inside is sound.
        let self_raw: *mut Renderer = self;
        unsafe {
            Renderer::add_underline(
                self_raw,
                coord.x,
                coord.y as u16,
                sgr::Underline::Single,
                screen_fg,
                255,
            )
        }?;
        if cp.wide && coord.x < self.cells.size.columns - 1 {
            unsafe {
                Renderer::add_underline(
                    self_raw,
                    coord.x + 1,
                    coord.y as u16,
                    sgr::Underline::Single,
                    screen_fg,
                    255,
                )
            }?;
        }
        Ok(())
    }

    /// Update the frame data.
    //
    // PORTS: generic.zig:1123-… `updateFrame`; macos_impl generic.rs:4424-4803.
    // Zig `pub fn updateFrame(self, state: *renderer.State, cursor_blink_visible:
    // bool) Allocator.Error!void`; `state: *renderer.State` → `&state::State` (never
    // mutates a `State` field — only locks `state.mutex` + mutates THROUGH the stored
    // `*mut Terminal`). The sole propagated `?` is `terminal_state.update` →
    // `VtOutOfMemory`.
    //
    // MILESTONE-SCOPE-CUT (L4 plan Global Constraints — bg_color/cell_bg/cell_text
    // only): FOUR reference branches are ELIDED, each cited (never silently dropped):
    //   1. Kitty-graphics image state (`self.images.kitty_requires_update`/
    //      `kitty_update`, macos_impl generic.rs:4584-4597) — `linux_impl` has no
    //      `images` field (the `crate::image` port is macOS-only; kitty is deferred).
    //   2. Inspector debug-overlay (`state.inspector` → `overlay_features` →
    //      `rebuild_overlay`, macos_impl generic.rs:4619-4636,4717) + the overlay
    //      image upload (`self.images.overlay_update`, :4783-4785) — deferred.
    //   3. Custom-shader uniforms (`self.update_custom_shader_uniforms_from_state`,
    //      macos_impl generic.rs:4789) — custom shaders are deferred.
    //   4. The macOS-only glass-background alpha zero-out (macos_impl generic.rs:
    //      4773-4778) is guarded by `if (comptime builtin.os.tag == .macos)` in
    //      generic.zig; on Linux that branch is COMPILED OUT, so it is absent here.
    // Every other line is copy-adapted verbatim from `macos_impl`.
    pub fn update_frame(
        &mut self,
        state: &state::State,
        cursor_blink_visible: bool,
    ) -> Result<(), VtOutOfMemory> {
        // We fully deinit and reset the terminal state every so often so that a
        // particularly large terminal state doesn't cause the renderer to hold on to
        // retained memory. Frame count is ~12 minutes at 120Hz.
        let max_terminal_state_frame_count = 100_000;
        if self.terminal_state_frame_count >= max_terminal_state_frame_count {
            self.terminal_state.deinit(self.alloc);
            self.terminal_state = RenderState::empty();
        }
        self.terminal_state_frame_count += 1;

        // Data we extract out of the critical area.
        struct Critical {
            links: CellSet,
            // macos_impl §C.5: written but never READ after the block (post-block code
            // reads `state.mouse.point/.mods` directly). Mirrored (R1) write-only.
            #[allow(dead_code)]
            mouse: state::Mouse,
            preedit: Option<state::Preedit>,
            scrollbar: Scrollbar,
        }

        // The renderer-thread state carries its terminal behind a `*mut Terminal` (a
        // separate allocation, disjoint from `self`). Copy the stored raw pointer and
        // re-deref it FRESH at every use, matching the Zig `state.terminal.…` re-walk.
        let t = state.terminal; // *mut Terminal

        // Hand-rolled `defer state.mutex.unlock()` (macos_impl §C.4/RISK-5): the
        // guard-less `Mutex` has no RAII drop, and the lock is held across THREE exits
        // — the sync-output early `return`, the `terminal_state.update(...)?`
        // propagation, and normal fall-through. This `Drop` fires on all three.
        struct Unlock<'a>(&'a Mutex);
        impl Drop for Unlock<'_> {
            fn drop(&mut self) {
                self.0.unlock();
            }
        }

        // Update all our data as tightly as possible within the mutex.
        let mut critical: Critical = {
            unsafe {
                (*state.mutex).lock();
            }
            let _mutex_guard = Unlock(unsafe { &*state.mutex });

            // If we're in a synchronized output state, we pause all rendering.
            if unsafe { (*t).modes.get(Mode::SynchronizedOutput) } {
                return Ok(());
            }

            // If scroll-to-bottom on output is enabled, check if the final line
            // changed by comparing the bottom-right pin. Update this BEFORE we update
            // our render state so we can draw the new scrolled data immediately.
            if self.config.scroll_to_bottom_on_output {
                'scroll: {
                    let active = unsafe { (*t).screens.active };
                    let br = match unsafe { (*active).pages.get_bottom_right(point::Tag::Screen) } {
                        Some(br) => br,
                        None => break 'scroll,
                    };

                    // If the pin hasn't changed, then don't scroll.
                    if self.last_bottom_node == Some(br.node as usize) && self.last_bottom_y == br.y
                    {
                        break 'scroll;
                    }

                    // Update tracked pin state for next frame
                    self.last_bottom_node = Some(br.node as usize);
                    self.last_bottom_y = br.y;

                    // Scroll
                    unsafe {
                        (*t).scroll_viewport(ScrollViewport::Bottom);
                    }
                }
            }

            // Update our terminal state
            self.terminal_state.update(self.alloc, unsafe { &mut *t })?;

            // If our terminal state is dirty at all we need to redo the viewport search.
            if self.terminal_state.dirty != Dirty::False {
                unsafe {
                    (*t).flags.search_viewport_dirty = true;
                }
            }

            // Get our scrollbar out of the terminal (re-read `active` FRESH after the
            // `&mut *t` update above).
            let active = unsafe { (*t).screens.active };
            let scrollbar = unsafe { (*active).pages.scrollbar() };

            // Get our preedit state
            let preedit: Option<state::Preedit> = state.preedit.as_ref().map(|p| p.clone());

            // MILESTONE-SCOPE-CUT (kitty graphics): macos_impl generic.rs:4584-4597 —
            // `self.images.kitty_requires_update`/`kitty_update`. No `images` field.

            // Get our OSC8 links we're hovering if we have a mouse.
            let links: CellSet = match state.mouse.point {
                None => CellSet::new(),
                Some(vp) => {
                    if !state.mouse.mods.equal(key::ctrl_or_super(Mods::default())) {
                        CellSet::new()
                    } else {
                        self.terminal_state
                            .link_cells(Global, vp)
                            .unwrap_or_else(|_err| CellSet::new())
                    }
                }
            };

            // MILESTONE-SCOPE-CUT (inspector debug overlay): macos_impl generic.rs:
            // 4619-4636 — `state.inspector` → `overlay_features`. Overlay is deferred.

            Critical {
                links,
                mouse: state.mouse,
                preedit,
                scrollbar,
            }
        };

        // Outside the critical area we can update our links to contain our regex
        // results.
        let _ = self.config.links.render_cell_map(
            Global,
            &mut critical.links,
            &self.terminal_state,
            state.mouse.point,
            state.mouse.mods,
        );

        // Clear our highlight state and update.
        if self.search_matches_dirty || self.terminal_state.dirty != Dirty::False {
            self.search_matches_dirty = false;

            // Clear the prior highlights
            let mut any_dirty: bool = false;
            for row in self.terminal_state.row_data.iter_mut() {
                if !row.highlights.is_empty() {
                    row.highlights.clear();
                    row.dirty = true;
                    any_dirty = true;
                }
            }
            if any_dirty && self.terminal_state.dirty == Dirty::False {
                self.terminal_state.dirty = Dirty::Partial;
            }

            // NOTE: The order below matters. Highlights added earlier take priority.
            if let Some(m) = &self.search_selected_match {
                let _ = self.terminal_state.update_highlights_flattened(
                    self.alloc,
                    HighlightTag::SearchMatchSelected as u8,
                    core::slice::from_ref(&m.r#match),
                );
            }
            if let Some(m) = &self.search_matches {
                let _ = self.terminal_state.update_highlights_flattened(
                    self.alloc,
                    HighlightTag::SearchMatch as u8,
                    &m.matches,
                );
            }
        }

        // From this point forward no more errors.

        // MILESTONE-SCOPE-CUT (inspector debug overlay): macos_impl generic.rs:4717 —
        // `self.rebuild_overlay(&critical.overlay_features)`. Overlay is deferred, so
        // `rebuild_overlay` is not ported for this milestone.

        // Acquire the draw mutex for all remaining state updates.
        {
            self.draw_mutex.lock();

            // Build our GPU cells. `critical.preedit` is MOVED into `rebuild_cells`,
            // but `critical.preedit.is_some()` is read FIRST for `cursor::style`.
            let cs = cursor::style(
                &self.terminal_state,
                cursor::StyleOptions {
                    preedit: critical.preedit.is_some(),
                    focused: self.focused,
                    blink_visible: cursor_blink_visible,
                },
            );
            self.rebuild_cells(critical.preedit, cs, &critical.links);

            // The scrollbar is only emitted during draws so we also check the
            // scrollbar cache here and update if needed.
            if !self.scrollbar.eql(critical.scrollbar) {
                self.scrollbar = critical.scrollbar;
                self.scrollbar_dirty = true;
            }

            // Update our background color
            self.uniforms.bg_color = [
                self.terminal_state.colors.background.r,
                self.terminal_state.colors.background.g,
                self.terminal_state.colors.background.b,
                (self.config.background_opacity * 255.0).round() as u8,
            ];

            // MILESTONE-SCOPE-CUT (macOS glass background): macos_impl generic.rs:
            // 4773-4778 is guarded by `if (comptime builtin.os.tag == .macos)` in
            // generic.zig; that branch is compiled out on Linux, so it is absent.

            // MILESTONE-SCOPE-CUT (overlay image upload): macos_impl generic.rs:
            // 4783-4785 — `self.images.overlay_update`. No `images` field.

            // MILESTONE-SCOPE-CUT (custom-shader uniforms): macos_impl generic.rs:4789
            // — `self.update_custom_shader_uniforms_from_state()`. Deferred.

            self.draw_mutex.unlock();
        }

        // Notify our shaper we're done for the frame.
        self.font_shaper.end_frame();

        // Reset our dirty state after updating (the reference `defer
        // self.terminal_state.dirty = .false`, fired at fn scope exit — §C.4).
        self.terminal_state.dirty = Dirty::False;

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
    use rustty_vt::kitty;
    use rustty_vt::modes::ModePacked;
    use rustty_vt::render::CellSet;
    use rustty_vt::size::CellCountInt;
    use rustty_vt::terminal::{Colors, Options as TermOptions, Terminal};
    use rustty_zigstd::arena_allocator::ArenaAllocator;

    use crate::cursor;
    use crate::shaders::Atlas;

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

    // PORT NOTE: reuse the exact `opts(cols, rows)` recipe from `macos_impl::tests`
    // (generic.rs:6066) — renamed `term_opts` to avoid shadowing the `Options`
    // renderer struct that the other tests construct as a local `opts`.
    fn term_opts(cols: CellCountInt, rows: CellCountInt) -> TermOptions {
        TermOptions {
            cols,
            rows,
            max_scrollback: 10_000,
            colors: Colors::default(),
            default_modes: ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    /// L4 T10 KAT: the CPU-side cell fill — mirrors `macos_impl::tests::
    /// rebuild_cells_fills_bg_fg_cursor` (generic.rs:7178). Feeds "Ab" into a 10×3
    /// terminal, updates the render state, calls [`Renderer::rebuild_cells`], and
    /// asserts the CPU-side [`Contents`] buffers (bg cell, fg-glyph row, cursor
    /// glyph) + the cursor uniform — NO GPU readback (that is Task 11's golden).
    ///
    /// Uses [`Renderer::init`] (which stands up the offscreen EGL/GL context) as the
    /// skip-probe; clean-skips (never panics/fabricates) when the host cannot give a
    /// 4.3-core context — the Task-1 OPEN RISK path (default virgl here; runs for
    /// real under `LIBGL_ALWAYS_SOFTWARE=1` → llvmpipe 4.5). Holds the process-wide
    /// [`test_support::gl_lock`] for the whole context lifetime, `_guard` first so it
    /// drops last (after `deinit`).
    #[test]
    fn linux_opengl_rebuild_cells_places_ab_and_cursor() {
        let _guard = test_support::gl_lock();

        let (font_grid, size) = test_font_grid_and_size(10, 3);
        let opts = Options {
            config: derived_config_for_test(),
            font_grid,
            size,
        };
        let mut r = match Renderer::init(Global, opts) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "no headless OpenGL 4.3 core context ({e}); skipping \
                     linux_opengl_rebuild_cells_places_ab_and_cursor"
                );
                return;
            }
        };

        // small terminal: print "Ab" on row 0, leave a visible block cursor.
        let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"Ab");
            s.deinit();
        }
        r.terminal_state.update(Global, &mut t).unwrap();
        let cs = cursor::style(
            &r.terminal_state,
            cursor::StyleOptions {
                preedit: false,
                focused: true,
                blink_visible: true,
            },
        );
        r.rebuild_cells(None, cs, &CellSet::new());

        // (1) BG: the printed cell (0,0) has a bg entry; a default-bg cell → alpha 0.
        let bg = r.terminal_state.colors.background;
        assert_eq!(*r.cells.bg_cell(0, 0), [bg.r, bg.g, bg.b, 0]);

        // (2) FG glyph: row 0 fg list (index y+1 == 1) is non-empty (glyphs for A, b).
        assert!(!r.cells.fg_rows.lists[1].is_empty());
        let a = r.cells.fg_rows.lists[1][0];
        assert_eq!(a.grid_pos, [0, 0]);
        assert_eq!(a.atlas, Atlas::Grayscale);
        assert!(a.glyph_size[0] > 0 && a.glyph_size[1] > 0);

        // (3) CURSOR: a block cursor glyph is present, colored the fixture's red
        //     (config.cursor_color). After printing "Ab" the block cursor sits at
        //     column x=2 (the narrow empty cell), so BOTH the cursor uniform and the
        //     cursor glyph grid_pos are [2, 0] (ref :2529-2536).
        let cur = r.cells.get_cursor_glyph().expect("cursor present");
        assert!(cur.bools.is_cursor_glyph);
        assert_eq!(&cur.color[0..3], &[0xFF, 0, 0]);
        assert_eq!(cur.grid_pos, [2, 0]);
        assert_eq!(r.uniforms.cursor_pos, [2, 0]);

        r.deinit();
        t.deinit(Global);
    }
}
