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
//! (HISTORY) This file started as an L4 SKELETON (Task 9 — struct decls +
//! `init`/`deinit`); the CPU cell fill (`rebuild_cells`/`update_frame`) and the
//! offscreen draw path (`draw_frame`/the golden) landed in Tasks 10-11, and wave 2
//! closed the remaining image-pipeline elisions (F-005 kitty graphics, F-012
//! background image, F-020 inspector overlay). There are no scope cuts left in the
//! update/draw path.
//!
//! ONE deliberate divergence from `macos_impl`, faithful to `generic.zig`'s
//! non-macOS comptime arms:
//!   1. **No `display_link`.** `generic.zig`'s `const DisplayLink = switch
//!      (builtin.os.tag) { .macos => *macos.video.DisplayLink, else => void }`
//!      (:225-227) is `void` off macOS, so the field, the `vsync` init branch
//!      (`else => null`, :692-698), and the `if (DisplayLink != void)` deinit
//!      teardown (:806-811) all collapse to nothing here.
//!
//! (RESOLVED, w2 F-005) A second divergence used to be listed here: "no
//! `images`/`bg_image`", because `crate::image` was macOS-only. `crate::image` is
//! now a monomorphic twin pair like this module is — `image::linux_impl` over
//! `crate::opengl` — so `images: image::State` is a real field and the Kitty
//! graphics upload/draw path is wired for real (`generic.zig:1229-1248, 1548-1549,
//! 1628-1679`).
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
use rustty_zigstd::thread::Semaphore;

use core::mem;
use core::ptr::addr_of_mut;
use std::ffi::{CStr, CString};
// `prepBackgroundImage` opens the config path (byte-faithful `CString` bytes ->
// an `OsStr` path) via `std::os::unix::ffi::OsStrExt::from_bytes` — NEVER a lossy
// `String`.
// CORRECTION (task W-2, N342): this said "(This module is Linux-gated, so unix is
// guaranteed.)"; the OpenGL arm also serves Windows now (`backend.zig:20-21`), where
// the path bytes are WTF-8 and are converted as Zig's `openFile` does
// (`rustty_os::path::os_string_from_bytes`, the same conversion `shadertoy.rs` uses).
#[cfg(not(windows))]
use std::ffi::OsStr;
#[cfg(not(windows))]
use std::os::unix::ffi::OsStrExt;

// `FileType::detect`/`guessFromExtension` — the background-image file-type sniff
// (`prepBackgroundImage`). Renderer deps rustty-util.
use rustty_util::file_type::FileType;

use super::{
    path_extension, AtomicHealth, DerivedConfig, MailboxTimeout, Options, SurfaceMailbox,
    SurfaceMessage, TerminalColor,
};
use crate::cell::{self, Contents, Key};
use crate::cursor;
use crate::image::{self, DrawPlacements};
use crate::latest_frame::{DrawMutex, LatestFrame};
use crate::opengl::buffer::Buffer;
use crate::opengl::frame::Frame;
use crate::opengl::gl::primitives::Primitive;
use crate::opengl::gpu_abi::{
    BgImage as BgImageGpu, CellText as CellTextGpu, Uniforms as UniformsGpu,
};
/// A finished frame ready to be consumed by the apprt in case that the
/// frame needs to be composited with UI elements by the graphical toolkit
/// manually.
//
// pub const ExportedFrame = if (@hasDecl(GraphicsAPI, "ExportedFrame"))
//     GraphicsAPI.ExportedFrame else void;   (generic.zig:93)
pub use crate::opengl::opengl_impl::ExportedFrame;
use crate::opengl::render_pass::{Attachment, AttachmentTarget, Draw, Step};
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
use std::sync::Arc;

// const log = std.log.scoped(.generic_renderer);   (generic.zig:46)
//
// DECISION-logging.md §6r call shape 3: `rustty-renderer` sits BELOW `rustty-vt-c`
// in R4's layering, so restored sites call the `log` facade with `target:
// LOG_SCOPE` carrying the reference's `std.log.scoped` tag. This is the OpenGL twin
// of `generic.rs`'s `macos_impl::LOG_SCOPE` (`generic.rs:1190`); both twins port the
// SAME `src/renderer/generic.zig` and therefore carry the SAME scope. The constant
// is at FILE scope here because this whole file is the Linux-only arm.
//
// RESTORED (task-14c-5, finding N9). `generic.zig` has 33 live `log.*` calls at pin
// `20abdb50a`. SIX of them are macOS-only and have no Linux counterpart at all —
// `:991` (`displayLinkCallback`) and `:1169/:1177/:1183/:1188/:1190`
// (`syncDisplayLink`), both guarded by `DisplayLink != void`, which is `void` off
// macOS (see the module docs' divergence #1). Of the remaining 27, TWENTY-FOUR are
// live in this file. The three that are not are `:1558` (`error rebuilding GPU
// cells`) and `:2912`/`:3181` (`error caching font shaping results`, the same
// reference call in two places): their Rust counterparts are infallible, so there is
// no failure arm to hang the warn on. Each carries a PORT NOTE at its site; they are
// the Linux-side residuals of the WAIVED rows `S115-F4` and `S116-F2`
// (`ADJUDICATION.md#s115-f4`, `#s116-f2`), exactly as on the Metal side.
//
// One further emission in this file — `opengl_frame_completed`'s "Failed to present
// render target" — is NOT a `generic.zig` site: its oracle is `opengl/Frame.zig:65`
// and it therefore targets `opengl::opengl_impl::LOG_SCOPE`. See the note there.
const LOG_SCOPE: &str = "generic_renderer";

/// The number of buffers we use for double/triple buffering.
//
// PORT NOTE (macos_impl generic.rs:913-919): reference `SwapChain.buf_count =
// GraphicsAPI.swap_chain_count`. Monomorphized to the concrete
// `OpenGL::SWAP_CHAIN_COUNT` (= 3 since `2f0b65346`, `OpenGL.zig:31-33`:
// "Triple-buffering gives the GPU room to pipeline renders without having to wait
// on the apprt consuming previous frames"; it was 1 at the pin).
const SWAP_CHAIN_COUNT: usize = OpenGL::SWAP_CHAIN_COUNT;

/// Whether +Y points down in the custom-shader coordinate space.
//
// PORT NOTE (macos_impl generic.rs:921-929): reference
// `GraphicsAPI.custom_shader_y_is_down` — monomorphized to
// `OpenGL::CUSTOM_SHADER_Y_IS_DOWN` (`OpenGL.zig:27-28` = `false`: the OpenGL
// fragCoord is +Y = up). Read by `update_custom_shader_uniforms_for_frame`.
//
// `2f0b65346` makes it public on the renderer: "Whether +Y is down in the
// coordinate space of exported frames. Apprts use this to orient frames when
// presenting." (`generic.zig:95-97`). The GTK `RenderSurface` flips on it.
pub const CUSTOM_SHADER_Y_IS_DOWN: bool = OpenGL::CUSTOM_SHADER_Y_IS_DOWN;

/// PORT NOTE (macos_impl generic.rs:931-954): the aggregate renderer error.
/// Reference `generic.zig` methods return Zig `!T` (inferred error unions of the
/// graphics-API leaf errors + OOM); Rust `?` needs a common type, so this enum
/// aggregates the leaf errors via the `From` impls below. The Metal twin names the
/// GPU variant `MetalFailed`; this names it `GlFailed`. Every other variant —
/// including `ImageDecode` — is the SAME set as the Metal twin
/// (macos_impl generic.rs:1194-1216).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// An OpenGL API call failed (any of the buffer/texture/sampler/target/
    /// shaders leaf failures, or the `OpenGL` graphics-API impl itself — context
    /// setup / a raw `gl` error).
    GlFailed,
    /// An allocation failed (the font `Atlas` allocation).
    OutOfMemory,
    // PORT NOTE (`c4e16970a`): the `Defunct` variant is DELETED with the swap
    // chain's defunct flag — `nextFrame` is infallible and a released chain is
    // `swap_chain == null` (rebuilt in `drawFrame`).
    /// A background-image `wuffs` decode failed (`WuffsError`/`Overflow`).
    /// `prepBackgroundImage` propagates the wuffs `Error` via `try` (the
    /// reference `!void` error set includes the wuffs `Error`); OOM maps to
    /// `OutOfMemory`, the rest to this. Note: only the DECODE error propagates —
    /// the open/read failures `break :load_background` (a warn + no-op), never
    /// propagate. Twin of `macos_impl generic.rs:1209-1215`.
    ImageDecode,
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

// rustty_wuffs::Error {OutOfMemory, WuffsError, Overflow} -> the renderer Error.
// `prepBackgroundImage` returns `!void` whose error set includes the wuffs `Error`
// (via `try wuffs.png.decode(...)`), so the decode error PROPAGATES out of
// `prep_background_image` while the open/read failures only warn. OOM reuses
// `OutOfMemory`; `WuffsError`/`Overflow` map to `ImageDecode`. Twin of
// `macos_impl generic.rs:1231-1245`.
impl From<rustty_wuffs::Error> for Error {
    fn from(e: rustty_wuffs::Error) -> Self {
        match e {
            rustty_wuffs::Error::OutOfMemory => Error::OutOfMemory,
            rustty_wuffs::Error::WuffsError | rustty_wuffs::Error::Overflow => Error::ImageDecode,
        }
    }
}

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

// PORT NOTE (`c4e16970a`): the `Defunct` marker type (reference
// `error{Defunct}`) is DELETED — `SwapChain::next_frame` is infallible.

// PORT NOTE (surface-mailbox seam RESOLVED — was a per-arm no-op placeholder):
// `SurfaceMailbox`/`MailboxTimeout`/`SurfaceMessage`/`SurfaceMailboxSink` are now
// SHARED module-level types in `generic.rs` (`super::`) — `generic.zig`
// instantiates ONE `Renderer(GraphicsAPI)` for both Metal and OpenGL, so the seam
// has ONE copy rather than a per-arm duplicate — and the mailbox is REAL: core
// attaches a `SurfaceMailboxSink` over `apprt.surface.Mailbox` through `Options`
// (GPU_RD_CHUNK3A_DESIGN §B.2). Imported at the `use super::{…}` line above.

/// The monomorphic GPU renderer over [`OpenGL`] (reference
/// `GenericRenderer(OpenGL)`).
//
// PORT NOTE (macos_impl generic.rs:1123-1326): the R1-PRESERVED `generic.zig:46-80`
// abstraction-hierarchy doc-comment lives verbatim on `macos_impl::Renderer`; it is
// not duplicated here. Fields are `pub` (the reference accesses them via
// `self.<field>` from the renderer's own methods; Zig struct fields are non-opaque)
// and dead-code-exempt until the deferred draw path reads them. Same field SET as
// `macos_impl::Renderer` with `crate::metal::*` types swapped for `crate::opengl::*`,
// MINUS `display_link` (macOS-only — see the module docs). The image-typed fields
// (`images`/`bg_image`/`bg_image_changed`) ARE present: `crate::image` is a twin
// pair, so they carry `image::linux_impl`'s OpenGL types here.
pub struct Renderer {
    /// Allocator that can be used.
    pub alloc: Global,

    /// This mutex must be held whenever any state used in `drawFrame` is being
    /// modified, and also when it's being accessed in `drawFrame`.
    //
    // PORT NOTE (N437): the mutex lives in its own allocation (`DrawMutex`, an
    // `Arc` handle); see `crate::latest_frame`. Every cross-thread entry locks it
    // through the caller's own handle before it forms a reference to the renderer.
    pub draw_mutex: DrawMutex,

    /// The configuration we need derived from the main config.
    pub config: DerivedConfig,

    /// The mailbox for communicating with the window.
    pub surface_mailbox: SurfaceMailbox,

    /// Blocking (`.forever`) surface-mailbox pushes made while `draw_mutex` is
    /// held, to be sent once it is released.
    //
    // rustty-only (R13, UPSTREAM-DEFECTS.md UD362). The reference's frame
    // completion (`opengl/Frame.zig:83` `.redraw`, `generic.zig:2017`
    // `.renderer_health`) pushes with `.forever` inside `drawFrameLocked`, i.e.
    // with the draw mutex held. The app mailbox's only consumer is the apprt
    // main thread, which takes the same mutex in `displayRealized` /
    // `displayUnrealized`: a full mailbox then deadlocks both threads. The
    // pushes are queued here under the lock and `draw_frame_raw` sends them
    // after the unlock (as UD352 does for the display-link calls).
    pub deferred_surface_pushes: core::cell::RefCell<Vec<SurfaceMessage>>,

    /// The latest exported frame ready to be consumed by an apprt
    /// who needs to manually composite the frame with UI elements.
    /// Previously exported frames are released when a new frame is
    /// exported and pushed onto the queue.
    ///
    /// Unused if the renderer does not need to export frames to
    /// present its rendered frame.
    //
    // PORT NOTE (N437): behind an `Arc` so the apprt's main thread takes frames
    // through its own handle (`latest_frame_handle`) without touching the
    // renderer's bytes; see `crate::latest_frame`.
    pub latest_frame: Arc<LatestFrame<ExportedFrame>>,

    /// Current font metrics defining our grid.
    pub grid_metrics: Metrics,

    /// The size of everything.
    pub size: Size,

    /// True if the window is focused.
    pub focused: bool,

    /// True if the window is visible.
    //
    // (`a177ba90a`; read only by `syncDisplayLink`, which is a no-op off macOS.
    // Added with P7-W7 T2 so `setVisible` is the reference's on both twins.)
    pub visible: bool,

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

    /// The images that we may render.
    //
    // Zig `images: ImageState = .empty` (`ImageState = imagepkg.State`);
    // macos_impl generic.rs:1496-1499; generic.zig:178-189.
    pub images: image::State,

    /// Background image, if we have one.
    pub bg_image: Option<image::Image>,
    /// Set whenever the background image changes, signalling
    /// that the new background image needs to be uploaded to
    /// the GPU.
    ///
    /// This is initialized as true so that we load the image
    /// on renderer initialization, not just on config change.
    //
    // NOTE (WI-3 scope): the field exists and is torn down/kept in lockstep with
    // the macOS twin, but the bg-image DECODE/UPLOAD/DRAW arms are WI-4's; until
    // they land `bg_image` stays `None` on Linux and the draw path takes the
    // oracle's `else` branch (bg_color in its own step, generic.zig:1619-1626) —
    // which is exactly what the oracle does with no background image.
    pub bg_image_changed: bool,

    /// Background image vertex buffer (CPU-side, ergonomic).
    //
    // PORT NOTE: `generic.zig`'s `bg_image_buffer` (`shaderpkg.BgImage`) is
    // graphics-API-agnostic (`crate::shaders::BgImage`): `init` fills it via
    // `update_bg_image_buffer` (matching `generic.zig:773,794`) and it is synced
    // into `FrameState.bg_image_buffer` by the draw path.
    pub bg_image_buffer: BgImage,
    /// Force-updates the swap-chain copy of the bg-image buffer whenever it changes.
    pub bg_image_buffer_modified: usize,

    /// Graphics API state.
    pub api: OpenGL,

    // NOTE: no `display_link` — `generic.zig`'s `DisplayLink` is `void` off macOS.
    /// Health of the most recently completed frame.
    pub health: AtomicHealth,

    /// Health of how well the apprt can present our frames.
    ///
    /// This is separate from `health` because a renderer
    /// can produce healthy frames that the apprt can't present.
    pub presentation_health: AtomicHealth,

    /// True when we have a graphics context that can create GPU
    /// resources. Creating any GPU resource while this is false is invalid.
    pub display_realized: bool,

    /// Our swap chain (multiple buffering). Null when it has
    /// been released, either because the surface is hidden
    /// (`releaseGpuResources`) or because the display is
    /// unrealized. Rebuilt on the next `drawFrame`.
    pub swap_chain: Option<SwapChain>,

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

    /// The overlay `overlay` replaced last frame, kept so the next rebuild can
    /// reuse its surface. Only the render thread touches it.
    //
    // rustty-only (R13, UPSTREAM-DEFECTS.md UD363): the reference rebuilds
    // `self.overlay` in place without the draw mutex while the macOS main
    // thread's sync draw reads it under the mutex. The port builds the next
    // overlay into a local (reusing this one) without the lock and swaps it
    // into `overlay` under the lock. Cost: while the overlay is on, two
    // terminal-sized overlay surfaces stay resident (`overlay` and this spare,
    // where the reference keeps one); it is released when the overlay is
    // turned off.
    pub overlay_spare: Option<Overlay>,

    /// The base timestamp for the Kitty graphics animation clock.
    /// Animation frame timing is expressed as milliseconds since
    /// this instant. Set on the first frame update that observes
    /// Kitty images.
    //
    // Zig `kitty_animation_clock: ?std.Io.Timestamp = null` -> `Option<Instant>`
    // (the `first_frame_time`/`last_frame_time` monotonic-clock precedent).
    pub kitty_animation_clock: Option<Instant>,

    /// When the next Kitty animation frame is due, in
    /// milliseconds on the animation clock, from the most recent
    /// frame update. Null when no running animation needs a
    /// wakeup.
    pub kitty_animation_next_ms: Option<u64>,
}

/// The cadence of continuous (draw-only) animation wakes,
/// i.e. 120fps, and the floor for any animation wake delay.
//
// PORT NOTE (`aee7bf347`): reference `pub const draw_interval_ms: u64 = 8;`
// nested inside `Renderer`; a module-level sibling here (SCREAMING_CASE per
// Rust const convention, the crate-wide precedent).
pub const DRAW_INTERVAL_MS: u64 = 8;

/// A point in the future when the renderer needs to be driven
/// again to keep animating, and what kind of drive it needs.
//
// PORT NOTE: reference `pub const AnimationWake = struct { delay_ms, kind }`
// nested inside `Renderer` (with `Kind` nested inside IT); module-level
// siblings here (Rust cannot nest types inside impls), `Kind` named
// `AnimationWakeKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationWake {
    /// Delay in milliseconds until the wake is due.
    pub delay_ms: u64,
    pub kind: AnimationWakeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationWakeKind {
    /// A redraw alone suffices, no updateFrame. Much cheaper
    /// than `update`.
    Draw,

    /// Frame data must be updated first: updateFrame, then draw.
    Update,
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
    // PORT NOTE (`c4e16970a`): the `defunct` flag is DELETED — the owner's
    // `swap_chain: ?SwapChain` (`Option`) now models torn-down state. The
    // exactly-once contract the GL per-frame `ptr::read` deinit relies on is
    // carried by the callers: every `sc.deinit()` is immediately followed by
    // `swap_chain = None` (release/unrealize) or by dropping the Renderer
    // (`FrameState` has no `Drop`, so the stale copy frees nothing).
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
        })
    }

    // PORTS: generic.zig:276-284 `SwapChain.deinit`; macos_impl generic.rs:1400-1422.
    //
    // DIVERGENCE from `macos_impl` (which lets field `Drop` free the frames): the
    // OpenGL `FrameState` GL resources have a MANUAL `deinit` and no `Drop`, so this
    // ports `generic.zig:283`'s `for (&self.frames) |*frame| frame.deinit()` FOR
    // REAL. `FrameState::deinit` consumes by value; since `FrameState` has no `Drop`,
    // reading each out of the array (via `ptr::read`) and consuming it is sound —
    // the stale array copies are never dropped, and the callers' Option
    // discipline (see the struct PORT NOTE) makes this run exactly once, so
    // there is no double-free.
    pub fn deinit(&mut self) {
        // Wait for all inflight draws to complete so we can cleanly deinit GPU state.
        for _ in 0..SWAP_CHAIN_COUNT {
            self.frame_sema.wait();
        }
        for frame in &mut self.frames {
            // SAFETY: `FrameState` (and its GL-handle fields) have no `Drop`, so
            // duplicating the handle via `read` + consuming it in `deinit` cannot
            // double-free; `deinit` runs exactly once (the callers' Option
            // discipline).
            let frame = unsafe { core::ptr::read(frame) };
            frame.deinit();
        }
    }

    /// Get the next frame state to draw to (waits on the semaphore; must be paired
    /// with [`SwapChain::release_frame`]).
    // PORTS: `SwapChain.nextFrame` (`c4e16970a`: infallible — the defunct check
    // is gone; the Zig `errdefer` was deleted with it).
    pub fn next_frame(&mut self) -> &mut FrameState {
        self.frame_sema.wait();
        self.frame_index = (self.frame_index + 1) % (SWAP_CHAIN_COUNT as u8);
        &mut self.frames[self.frame_index as usize]
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
// struct` (Renderer-private), widened to `pub`. Built when `custom_shaders` is set
// (at init, or lazily by `draw_frame_raw`'s setup block after a config change).
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

/// PORT-ONLY (UD352): the outcome of the `syncDisplayLink` tail's reads, applied
/// once `draw_mutex` is released. Off macOS `DisplayLink` is `void` and
/// `syncDisplayLink` returns at once, so this is empty and `apply` does nothing;
/// it exists so `renderer_seam` drives both twins the same way.
#[derive(Clone, Copy, Debug)]
pub struct DisplayLinkSync;

impl DisplayLinkSync {
    /// `if (comptime DisplayLink == void) return;`
    pub fn apply(self) {}
}

mod cells;
mod cross_thread;
mod draw;
mod frame;
mod lifecycle;

pub use draw::DeferredSurfacePushes;

// FFI-heavy (raw GL calls behind the leaves + the `*mut SharedGrid` handle) —
// Miri-excluded like the Metal lane (add to MIRI_EXCEPTIONS.md under `linux_impl`).
#[cfg(test)]
mod tests;
