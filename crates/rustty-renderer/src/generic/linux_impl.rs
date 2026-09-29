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
/// reference logs (`log.warn`). UPDATED (task-14c-5): the `log` facade is wired and
/// this arm's site is now LIVE too — `rebuild_cells` binds the payload and emits
/// `log::warn!(target: LOG_SCOPE, "error building row y={y} err={err:?}")`
/// (`generic.zig:2627`), matching the `macos_impl` twin. The `#[derive(Debug)]` is
/// what the `{err:?}` rendering reads; the `#[allow(dead_code)]` STAYS because
/// rustc's `dead_code` lint does not count a `#[derive(Debug)]` field read as a use,
/// so the variant payloads would still be reported "never read" without it (same
/// reasoning as `generic.rs:3309`).
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

/// The `Frame` completion tail (`opengl::frame::CompletionFn`) — the OpenGL twin of
/// `macos_impl::metal_buffer_completed` (generic.rs). `Frame::complete` computes
/// `health` from `gl.errors.getError()` and hands it in, so — unlike Metal, which
/// inspects an `MTLCommandBuffer` here — this runs the reference's tail from the
/// present on (`opengl/Frame.zig:62-89` at `12752b2ac`, `2f0b65346`):
///
/// ```zig
/// const presented_frame: ?OpenGL.ExportedFrame = if (health == .healthy) frame: {
///     const frame = self.renderer.api.present(
///         self.target.*,
///         self.renderer.presentationHealth(),
///     ) catch |err| {
///         log.warn("failed to present render target: err={}", .{err});
///         break :frame null;
///     };
///     break :frame frame;
/// } else null;
///
/// // Sync after frame draw AND frame present GL calls.
/// gl.finish();
///
/// // At this point the ExportedFrame is finished and can be shared
/// if (presented_frame) |frame| {
///     self.renderer.pushFrame(frame);
///
///     // Notify the surface that it should redraw
///     _ = self.renderer.surface_mailbox.push(.redraw, .{ .forever = {} });
/// }
///
/// // Report the health to the renderer.
/// self.renderer.frameCompleted(health);
/// ```
//
// The log site's oracle is `renderer/opengl/Frame.zig`, whose scope is `.opengl`
// (Frame.zig:15), NOT `generic.zig`'s `.generic_renderer` — so it targets
// `crate::opengl::opengl_impl::LOG_SCOPE` even though the code lives here, exactly
// as the Metal twin targets `metal::frame::LOG_SCOPE`. It lives here at all because
// the port inlines `Frame.zig`'s completion tail into this trampoline.
//
// PORT NOTE (raw-ptr re-entry, `rust-mut-args-strongly-protected`): the renderer is
// recovered as a RAW `*mut Renderer` (never `&mut`) and deref'd HERE — inside
// `Frame::complete`, which runs inside `draw_frame_raw` while its `self_ptr` raw root
// is live. `present(&self)`, `push_frame(&self)`, the mailbox push and
// `frame_completed(&self)` each reborrow only `&` (narrow), matching the Metal
// lane's proven-clean shape.
//
// # Safety
// `renderer` must be a live, whole-`Renderer`-provenance `*mut ()` (the `self_ptr`
// `begin_frame` was handed); `target` a live `*mut Target` into that renderer's frame;
// called on the render thread inside `draw_frame_raw`, with `draw_mutex` held.
unsafe fn opengl_frame_completed(renderer: *mut (), target: *mut Target, health: crate::Health) {
    let renderer = renderer as *mut Renderer;

    // If the frame is healthy, draw it to an ExportedFrame
    // Then sync before exporting and pushing to the present queue.
    // The apprt pulls from this queue in its snapshot handler.
    let presented_frame: Option<ExportedFrame> = if health == crate::Health::Healthy {
        let presentation_health = unsafe { (*renderer).presentation_health() };
        match unsafe { (*renderer).api.present(&*target, presentation_health) } {
            Ok(frame) => Some(frame),
            Err(err) => {
                // log.warn("failed to present render target: err={}", .{err});
                log::warn!(
                    target: crate::opengl::opengl_impl::LOG_SCOPE,
                    "failed to present render target: err={err:?}"
                );
                None
            }
        }
    } else {
        None
    };

    // Sync after frame draw AND frame present GL calls.
    // gl.finish();
    crate::opengl::gl::finish();

    // At this point the ExportedFrame is finished and can be shared
    if let Some(frame) = presented_frame {
        // self.renderer.pushFrame(frame);
        unsafe { (*renderer).push_frame(frame) };

        // Notify the surface that it should redraw
        // _ = self.renderer.surface_mailbox.push(.redraw, .{ .forever = {} });
        //
        // PORT NOTE: reference defect (R13), UD362. This runs with the draw
        // mutex held; the `.forever` push is deferred until `draw_frame_raw`
        // releases it (`deferred_surface_pushes`).
        unsafe { (*renderer).defer_forever_push(SurfaceMessage::Redraw) };
    }

    // Report the health to the renderer.
    // self.renderer.frameCompleted(health);
    unsafe { (*renderer).frame_completed(health) };
}

/// `.forever` surface-mailbox pushes taken under the draw mutex, sent after it
/// is released (UD362).
pub struct DeferredSurfacePushes {
    mailbox: SurfaceMailbox,
    msgs: Vec<SurfaceMessage>,
}

impl DeferredSurfacePushes {
    /// Send every queued message with `.forever`, in order.
    pub fn push_all(self) {
        for msg in self.msgs {
            let _ = self.mailbox.push(msg, MailboxTimeout::Forever);
        }
    }
}

impl Renderer {
    // PORTS: generic.zig:658-819 `Renderer.init`; macos_impl generic.rs.
    //
    // The macOS `display_link`/vsync branch (generic.zig `else => null`) is the only
    // omission — see the module docs.
    //
    // P7-W7 T2 (`2f0b65346`): `api` is the reference's `OpenGL.init`, which owns a
    // surfaceless EGL context and releases it before returning, so this touches no
    // GL at all: the swap chain and the shaders are built lazily by the first
    // `drawFrame` on the render thread (`.swap_chain = null`, `.shaders = .uninit`,
    // `.reinitialize_shaders = true`). The port's two constructors (`init` over an
    // offscreen context the tests owned, `init_windowed` over GDK's borrowed GLArea
    // context) are one again.
    pub fn init(alloc: Global, options: Options) -> Result<Self, Error> {
        // Initialize our graphics API wrapper, this will prepare the
        // surface provided by the apprt and set up any API-specific
        // GPU resources.
        // var api = try GraphicsAPI.init(alloc, options);
        // errdefer api.deinit();
        let api = OpenGL::init(options.config.blending)?;

        // const has_custom_shaders = options.config.custom_shaders.value.items.len > 0;
        let has_custom_shaders = !options.config.custom_shaders.value.is_empty();

        // Create the font shaper.
        // Zig: var font_shaper = try font.Shaper.init(alloc, .{ .features = ... });
        let font_shaper = Shaper::init(shape::Options {
            features: options.config.font_features.clone(),
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

        // .cells = .{} (generic.zig:717) — an EMPTY Contents, sized by the first
        // `update_frame`/`rebuild_cells`.
        let cells = Contents::default();

        let mut result = Self {
            alloc,
            config: options.config,
            // .surface_mailbox = options.surface_mailbox, (generic.zig:704)
            surface_mailbox: options.surface_mailbox,
            deferred_surface_pushes: Default::default(),
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
            // Graphics API stuff
            api,
            swap_chain: None,
            has_custom_shaders,
            reinitialize_shaders: true,
            // Shaders are initialized lazily on the render thread.
            shaders: Shaders::uninit(),
            // ---- Zig struct-decl defaults (OMITTED by the 701-787 literal) ----
            latest_frame: Arc::new(LatestFrame::new()),
            visible: true,
            presentation_health: AtomicHealth::new(crate::Health::Healthy),
            display_realized: true,
            kitty_animation_clock: None,
            kitty_animation_next_ms: None,
            draw_mutex: DrawMutex::new(),
            custom_shader_focused_changed: false,
            cells_rebuilt: false,
            first_frame_time: None,
            last_frame_time: None,
            // macos_impl generic.rs:2274-2276. NOTE (R1): `generic.zig:189` declares
            // `bg_image_changed: bool = true`, but that field is WRITE-ONLY in the
            // oracle — the only other mention is a SHADOWING local in `changeConfig`
            // (:1881) — because `init` calls `prepBackgroundImage()` directly (:799)
            // rather than consulting it. The macOS twin therefore initializes it
            // `false`; follow the twin so the two arms cannot drift.
            images: image::State::empty(),
            bg_image: None,
            bg_image_changed: false,
            bg_image_buffer_modified: 0,
            health: AtomicHealth::new(crate::Health::Healthy),
            target_config_modified: 0,
            terminal_state: RenderState::empty(),
            terminal_state_frame_count: 0,
            overlay: None,
        };

        // Ensure our undefined values above are correctly initialized.
        result.update_font_grid_uniforms(); // (792)
        result.update_screen_size_uniforms(); // (793)
        result.update_bg_image_buffer(); // (794)
        result.prep_background_image()?; // (795) — macos_impl generic.rs:2297.

        Ok(result)
    }

    // PORTS: generic.zig:822-850 `Renderer.deinit` (`2f0b65346`); macos_impl generic.rs.
    //
    // This only deinitializes and frees CPU-side state
    // and does not free GPU resources like the swap chain and
    // shaders. Those are freed with `releaseGpuResources`.
    //
    // Off-macOS: the `if (DisplayLink != void)` teardown collapses to nothing (that
    // field does not exist here — see the module docs). The GPU objects (swap chain,
    // shaders, images, background image) were freed by `thread_exit` on the render
    // thread, whose context was current; the images' CPU state goes with them there
    // (`generic.zig:903-912`).
    pub fn deinit(&mut self) {
        // if (self.overlay) |*overlay| overlay.deinit(self.alloc);
        // Port: `Overlay::deinit(&mut self)` takes NO allocator arg.
        if let Some(overlay) = &mut self.overlay {
            overlay.deinit();
        }
        // self.terminal_state.deinit(self.alloc);
        self.terminal_state.deinit(self.alloc.into());
        // `search_selected_match`/`search_matches` arena teardown: the arena field
        // was collapsed (message.rs), so the owned `Option`s free via Rust `Drop`;
        // no explicit call.

        // self.latest_frame.deinit(global.io());
        self.latest_frame.deinit();

        // DisplayLink teardown — omitted (void off macOS).

        // self.cells.deinit(self.alloc); — takes NO allocator arg.
        self.cells.deinit();

        // self.font_shaper.deinit();
        self.font_shaper.deinit();
        // self.font_shaper_cache.deinit(self.alloc); — takes NO allocator arg.
        self.font_shaper_cache.deinit();

        // self.config.deinit();
        self.config.deinit();

        // self.api.deinit(); — the owned `OpenGL` (its `OffscreenContext`) releases
        // and destroys its EGL context on `Drop` when the `Renderer` value drops. A
        // `&mut self` `deinit` cannot move `api` out; documented. (`OpenGL::deinit`
        // consumes `self`, unreachable here — the field drop achieves the same.)
        //
        // self.* = undefined; — no-op (Rust ownership handles teardown).
    }

    /// The soonest animation wake this renderer needs, if any:
    /// custom shader animation wants continuous draw-only wakes
    /// at draw_interval_ms while active, and a running Kitty
    /// graphics animation wants an update wake when its next
    /// frame is due. The renderer thread drives its animation
    /// timer off this, re-querying after every wake.
    ///
    /// Must be called on the render thread.
    //
    // PORTS: `animationWake` (`aee7bf347`, REPLACES `hasAnimations`); the macos
    // twin carries the verbatim Zig quote — API-free code, copied verbatim.
    pub fn animation_wake(&self) -> Option<AnimationWake> {
        // Custom shaders animate by redrawing on a fixed cadence,
        // gated by configuration and focus.
        let shader_delay: Option<u64> = 'shader: {
            if !self.has_custom_shaders {
                break 'shader None;
            }
            break 'shader match self.config.custom_shader_animation {
                crate::thread::CustomShaderAnimation::False => None,
                crate::thread::CustomShaderAnimation::Always => Some(DRAW_INTERVAL_MS),
                crate::thread::CustomShaderAnimation::True => {
                    if self.focused {
                        Some(DRAW_INTERVAL_MS)
                    } else {
                        None
                    }
                }
            };
        };

        // Kitty animations tick during updateFrame; between
        // updates the deadline is absolute on the animation
        // clock, so a stream of draw wakes recomputing this
        // cannot starve it into the future.
        let kitty_delay: Option<u64> = 'kitty: {
            let next = match self.kitty_animation_next_ms {
                Some(next) => next,
                None => break 'kitty None,
            };
            let base = match self.kitty_animation_clock {
                Some(base) => base,
                None => break 'kitty None,
            };
            let now = Instant::now();
            let now_ms: u64 = now.duration_since(base).as_millis() as u64;
            // Never wake faster than the draw interval; an
            // overdue frame is picked up on the next wake.
            break 'kitty Some(core::cmp::max(
                next.saturating_sub(now_ms),
                DRAW_INTERVAL_MS,
            ));
        };

        // An update wake includes a draw, so it wins ties.
        if let Some(k) = kitty_delay {
            if shader_delay.is_none() || k <= shader_delay.unwrap() {
                return Some(AnimationWake {
                    delay_ms: k,
                    kind: AnimationWakeKind::Update,
                });
            }
        }

        if let Some(s) = shader_delay {
            return Some(AnimationWake {
                delay_ms: s,
                kind: AnimationWakeKind::Draw,
            });
        }

        None
    }

    // =======================================================================
    // L4 SWAP (first light): the renderer-thread lifecycle + mailbox-driven
    // setter surface that core's `renderer_seam::Thread<A>` forwards to through
    // `(*self.renderer).0`. Copy-adapted from `macos_impl` (generic.rs) with the
    // Metal leaf swapped for the OpenGL leaf and `DisplayLink` collapsed: on Linux
    // `generic.zig:39-42`'s `DisplayLink` is `void` (macOS-only CVDisplayLink), so
    // every `if (comptime DisplayLink != void)` block degenerates (GTK drives the
    // frame clock, not a CVDisplayLink). Each `@hasDecl(GraphicsAPI, …)` probe is
    // resolved against the concrete `OpenGL` leaf (OpenGL.zig), cited per method.
    // =======================================================================

    // P7-W7 T2 (`2f0b65346`): `surfaceInit` and `finalizeSurfaceInit` are gone
    // from `generic.zig` (and `OpenGL.zig`'s GTK `prepareContext(null)` arm with
    // them): the renderer owns its context and nothing prepares a GL context on the
    // main thread any more.

    /// Callback called by renderer.Thread when it begins.
    //
    // PORTS: generic.zig:880-886 `threadEnter` (`2f0b65346`: `self: *Self`).
    //
    //     pub fn threadEnter(self: *Self, surface: *apprt.Surface) !void {
    //         // If our API has to do things on thread enter, let it.
    //         if (@hasDecl(GraphicsAPI, "threadEnter")) {
    //             try self.api.threadEnter(surface);
    //         }
    //     }
    //
    // `OpenGL` declares `threadEnter` (OpenGL.zig:218-227): make the context
    // current on the render thread and load this thread's GL function table.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn thread_enter<S>(&mut self, rt_surface: *mut S) -> Result<(), Error> {
        let _ = rt_surface;
        // If our API has to do things on thread enter, let it.
        self.api.thread_enter()?;
        Ok(())
    }

    /// Callback called by renderer.Thread when it exits. Called on the
    /// render thread. Releases all GPU resources before the API tears down
    /// its context, since after this the GL context will be gone.
    //
    // PORTS: generic.zig:888-916 `threadExit` (`2f0b65346`).
    //
    //     pub fn threadExit(self: *Self) void {
    //         {
    //             self.draw_mutex.lockUncancelable(global.io());
    //             defer self.draw_mutex.unlock(global.io());
    //             ...
    //         }
    //         // If our API has to do things on thread exit, let it.
    //         if (@hasDecl(GraphicsAPI, "threadExit")) {
    //             self.api.threadExit();
    //         }
    //     }
    //
    // The locked block is [`Self::thread_exit_locked`]; this entry locks
    // `self.draw_mutex` around it for the single-threaded callers (the tests).
    // The render thread calls the two halves through `renderer_seam` with its own
    // lock handle (N437).
    pub fn thread_exit(&mut self) {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        self.thread_exit_locked();
        mutex.unlock();

        self.thread_exit_api();
    }

    /// The `draw_mutex` block of [`Self::thread_exit`]. The caller holds
    /// `draw_mutex`.
    pub fn thread_exit_locked(&mut self) {
        // Release swap chain and shaders.
        self.release_gpu_resources();

        // We don't release images in `releaseGpuResources`
        // since it can be called whenever the terminal is
        // occluded or unrealized, and we don't want to
        // reupload images every time that happens.
        // self.images.deinit(self.alloc); self.images = .empty;
        //
        // ORDER IS LOAD-BEARING (GL): this DELETES every Kitty/overlay image's GL
        // texture, so it must run here, while the render thread's context is still
        // current — the `api.threadExit()` below releases it.
        self.images.deinit(self.alloc);
        self.images = image::State::empty();

        // if (self.bg_image) |img| { img.deinit(self.alloc); self.bg_image = null; }
        if let Some(img) = self.bg_image.take() {
            img.deinit(self.alloc);
        }
    }

    /// The tail of [`Self::thread_exit`], after the `draw_mutex` block.
    pub fn thread_exit_api(&mut self) {
        // If our API has to do things on thread exit, let it.
        // (`OpenGL` declares `threadExit`, OpenGL.zig:229-235: release the context
        // and drop this thread's GL table.)
        self.api.thread_exit();
    }

    /// Run our loop start callback if the renderer cares.
    //
    // PORTS: generic.zig:897-916 `loopEnter`; macos_impl generic.rs:2274-2291.
    // `@hasDecl(OpenGL, "loopEnter")` is FALSE (OpenGL declares no `loopEnter`), so
    // there is no api call; and `if (comptime DisplayLink == void) return;` fires
    // immediately on Linux. The `draw_now: Async` arg (the reference `thr.draw_now`
    // fed to the CVDisplayLink output callback) is unused with no display link.
    pub fn loop_enter(&mut self, draw_now: rustty_xev::Async) -> Result<(), Error> {
        let _ = draw_now;
        // `if (comptime DisplayLink == void) return;` — Linux: DisplayLink is void.
        Ok(())
    }

    /// Run our loop end callback if the renderer cares.
    //
    // PORTS: generic.zig:919-933 `loopExit`; macos_impl generic.rs:2301-2305.
    // `@hasDecl(OpenGL, "loopExit")` is FALSE ⇒ no api call; `DisplayLink == void` ⇒
    // no link to stop. Faithful no-op.
    pub fn loop_exit(&self) {}

    /// PORTS: generic.zig:1001-1009 `setMacOSDisplayID`; macos_impl
    /// generic.rs:2325-2334. `if (comptime DisplayLink == void) return;` fires
    /// immediately on Linux — there is no display link to retarget. Kept because
    /// core's `Thread<A>` drains the `.macos_display_id` mailbox message
    /// unconditionally (the reference's own shape); a no-op here.
    ///
    /// PORT NOTE (`a177ba90a`): the signature tracks the macOS arm's — `&mut
    /// self` and the `draw_now` handle — even though neither is used here. The
    /// mailbox drain in `rustty-core`'s `renderer_seam.rs` is SHARED across
    /// platforms and calls this unconditionally, so the two arms must agree.
    /// They had diverged: the macOS arm took `(id, draw_now)` while this one
    /// still took `(id)`, which broke the Linux build outright.
    pub fn set_macos_display_id(
        &mut self,
        id: u32,
        draw_now: rustty_xev::Async,
    ) -> Result<(), Error> {
        let _ = (id, draw_now);
        Ok(())
    }

    /// PORTS: generic.zig:1017-1024 `hasVsync`; macos_impl generic.rs:2342-2347.
    /// `if (comptime DisplayLink == void) return false;` — Linux has no
    /// CVDisplayLink-managed vsync, so the renderer never self-manages the frame
    /// clock; core's `drawFrame` therefore always draws when asked.
    pub fn has_vsync(&self) -> bool {
        false
    }

    /// PORTS: generic.zig:1029-1048 `setFocus`; macos_impl generic.rs:2360-2372.
    /// Keeps the reference `assert(self.focused != focus)` + the custom-shader flag.
    /// The `if (comptime DisplayLink != void)` link start/stop block is elided on
    /// Linux (DisplayLink is void).
    pub fn set_focus(&mut self, focus: bool) -> Result<(), Error> {
        assert!(self.focused != focus);
        self.focused = focus;
        self.custom_shader_focused_changed = true;
        // `if (comptime DisplayLink != void) link: { … }` — Linux: no display link.
        Ok(())
    }

    /// Callback when the window is visible or occluded.
    ///
    /// Must be called on the render thread.
    //
    // PORTS: generic.zig:1174-1188 `setVisible` (`2f0b65346`).
    //
    //     pub fn setVisible(self: *Self, visible: bool) void {
    //         self.visible = visible;
    //         self.syncDisplayLink(null, null);
    //
    //         // When we're hidden, release our GPU resources.
    //         if (!visible) {
    //             self.draw_mutex.lockUncancelable(global.io());
    //             defer self.draw_mutex.unlock(global.io());
    //             self.releaseGpuResources();
    //         }
    //     }
    //
    // `syncDisplayLink` returns at once off macOS (`DisplayLink == void`). The
    // pin's `comptime !apprt.must_draw_from_app_thread` gate, which folded the
    // release away for GTK, is gone: the render thread owns the context now.
    //
    // N437: the render thread holds `draw_mutex` for the whole call (it takes it
    // through its own handle before forming this `&mut self`), so this is the
    // locked form: the reference's own lock around the release is the caller's.
    pub fn set_visible_locked(&mut self, visible: bool) -> DisplayLinkSync {
        self.visible = visible;

        // When we're hidden, release our GPU resources.
        if !visible {
            self.release_gpu_resources();
        }

        DisplayLinkSync
    }

    /// Release the GPU resources we hold while the surface is not
    /// visible. Today this is the swap chain (render targets, font
    /// atlas texture copies, cell buffers, custom shader textures),
    /// which makes up nearly all of a surface's GPU memory usage.
    /// The swap chain is rebuilt on the next `drawFrame`.
    ///
    /// Note that images are NOT released here since we don't want
    /// to reupload images every time the terminal is brought back
    /// from being occluded or unrealized.
    ///
    /// Caller must lock the draw mutex before calling this function.
    /// Resources that are already released are skipped.
    //
    // PORTS: generic.zig:1190-1214 `releaseGpuResources` (`2f0b65346`).
    //
    //     pub fn releaseGpuResources(self: *Self) void {
    //         if (self.swap_chain) |*sc| {
    //             // Waits for any in-flight frames to complete, then
    //             // frees all GPU resources.
    //             sc.deinit();
    //             self.swap_chain = null;
    //         }
    //
    //         // Release the shaders as well if we're unrealized.
    //         if (!self.display_realized) {
    //             self.shaders.deinit(self.alloc);
    //         }
    //     }
    //
    // Runs on the render thread with its context current (`thread_exit`,
    // `set_visible`, `renderCallback`'s unrealized arm).
    pub fn release_gpu_resources(&mut self) {
        if let Some(sc) = &mut self.swap_chain {
            // Waits for any in-flight frames to complete, then
            // frees all GPU resources.
            sc.deinit();
            self.swap_chain = None;
        }

        // Release the shaders as well if we're unrealized.
        if !self.display_realized {
            self.shaders.deinit();
        }
    }

    /// Mark the full screen as dirty so that we redraw everything.
    //
    // Zig: `pub inline fn markDirty(self: *Self) void { self.terminal_state.dirty
    // = .full; }`; macos_impl generic.rs:2397-2399.
    pub fn mark_dirty(&mut self) {
        self.terminal_state.dirty = Dirty::Full;
    }

    /// Set the new font grid. Must be called on the render thread.
    //
    // PORTS: generic.zig `setFontGrid`; macos_impl generic.rs:2410-2463. Fully
    // graphics-API-agnostic (no DisplayLink / Metal), so this is a straight
    // copy-adapt. `defer self.draw_mutex.unlock()`: the body is INFALLIBLE (no `?`),
    // so the single unlock at the natural end is reached on every non-panic path.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn set_font_grid(&mut self, grid: *mut SharedGrid) {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        self.set_font_grid_locked(grid);
        mutex.unlock();
    }

    /// The body of [`Self::set_font_grid`] between its `lock()` and the
    /// `defer unlock()`; the caller holds `draw_mutex` (N437: the render thread
    /// takes it through its own handle before forming this `&mut self`).
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn set_font_grid_locked(&mut self, grid: *mut SharedGrid) {
        // Update our grid.
        self.font_grid = grid;

        // Update all our textures so that they sync on the next frame. We can
        // modify this without a lock because the GPU does not touch this data.
        // A released swap chain is rebuilt with fresh frames that sync all
        // textures on first use.
        // if (self.swap_chain) |*sc| for (&sc.frames) |*frame| { ... };
        if let Some(sc) = &mut self.swap_chain {
            for frame in &mut sc.frames {
                frame.grayscale_modified = 0;
                frame.color_modified = 0;
            }
        }

        // Get our metrics from the grid. This doesn't require a lock because the
        // metrics are never recalculated.
        //
        // SAFETY: `grid` is the external `*mut SharedGrid` handle (not reborrowed
        // from `&mut self`); `(*grid).metrics` is a shared read through a raw pointer
        // with no `&mut` alias to `*grid` (§4.4 — the shared-grid handle).
        let metrics = unsafe { (*grid).metrics };
        self.grid_metrics = metrics;

        // Reset our shaper cache. If our font changed (not just the size) then the
        // cached data may be invalid, so we always clear the cache just in case.
        let font_shaper_cache = ShaperCache::init();
        self.font_shaper_cache.deinit();
        self.font_shaper_cache = font_shaper_cache;

        // Update cell size.
        self.size.cell = CellSize {
            width: metrics.cell_width,
            height: metrics.cell_height,
        };

        // Update relevant uniforms.
        self.update_font_grid_uniforms();

        // Force a full rebuild, because cached rows may still reference an outdated
        // atlas from the old grid and this can cause garbage to be rendered.
        self.mark_dirty();
    }

    /// Resize the screen.
    //
    // PORTS: generic.zig:2210-2226 `setScreenSize` (`2f0b65346`); macos_impl
    // generic.rs.
    //
    //     pub fn setScreenSize(self: *Self, size: renderer.Size) void {
    //         self.draw_mutex.lockUncancelable(global.io());
    //         defer self.draw_mutex.unlock(global.io());
    //
    //         self.size = size;
    //         self.updateScreenSizeUniforms();
    //
    //         // Some graphics APIs need to manually update their viewport,
    //         // like OpenGL. Do so here.
    //         if (@hasDecl(GraphicsAPI, "setViewport")) {
    //             self.api.setViewport(self.size.screen.width, self.size.screen.height);
    //         }
    //
    //         log.debug("screen size size={}", .{size});
    //     }
    pub fn set_screen_size(&mut self, size: Size) {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        self.set_screen_size_locked(size);
        mutex.unlock();
    }

    /// The body of [`Self::set_screen_size`]; the caller holds `draw_mutex`
    /// (N437). Runs on the render thread: `setViewport` is a GL call on its
    /// context.
    pub fn set_screen_size_locked(&mut self, size: Size) {
        self.size = size;
        self.update_screen_size_uniforms();

        // Some graphics APIs need to manually update their viewport,
        // like OpenGL. Do so here.
        self.api
            .set_viewport(self.size.screen.width, self.size.screen.height);

        // log.debug("screen size size={}", .{size});
        log::debug!(target: LOG_SCOPE, "screen size size={size:?}");
    }

    /// Update the configuration.
    //
    // PORTS: generic.zig:... `changeConfig`; macos_impl generic.rs:2501-2622.
    // By-value ownership transfer (`DerivedConfig` is non-`Copy`): the caller's
    // `Message::ChangeConfig` payload holds a `Box<DerivedConfig>`, so ownership is
    // moved in (the reference `v.impl.deinit()` collapses into the old-config drop).
    //
    // (RESOLVED, w2 F-012) The reference `bg_image_changed` branch — which calls
    // `prepBackgroundImage()` to decode a new bg-image path via `crate::image` — used
    // to be OMITTED here because `crate::image` was macOS-only. It is a twin pair
    // now, so the decode branch is LIVE below, exactly as in `macos_impl`.
    //
    // `defer self.draw_mutex.unlock()` FIDELITY: the ported `Mutex` is bare
    // lock/unlock with NO RAII guard, so `defer` is replicated by explicit unlock on
    // every early return (unlock-before-`?`) plus the unlock at the natural end.
    pub fn change_config(&mut self, config: DerivedConfig) -> Result<(), Error> {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        let result = self.change_config_locked(config);
        mutex.unlock();
        result
    }

    /// The body of [`Self::change_config`]; the caller holds `draw_mutex`
    /// (N437), so the reference's `defer self.draw_mutex.unlock()` is the
    /// caller's on every exit.
    pub fn change_config_locked(&mut self, config: DerivedConfig) -> Result<(), Error> {
        // We always redo the font shaper in case font features changed. Easier and
        // rare enough to not cause performance issues.
        {
            let font_shaper = Shaper::init(shape::Options {
                features: config.font_features.clone(),
            })?;
            self.font_shaper.deinit();
            self.font_shaper = font_shaper;
        }

        // Reset the shaper cache so info from the previous font isn't reused.
        let font_shaper_cache = ShaperCache::init();
        self.font_shaper_cache.deinit();
        self.font_shaper_cache = font_shaper_cache;

        // Set our new minimum contrast.
        self.uniforms.min_contrast = config.min_contrast;

        // Set our new color space and blending.
        self.uniforms.bools.use_display_p3 = config.colorspace == WindowColorspace::DisplayP3;
        self.uniforms.bools.use_linear_blending = config.blending.is_linear();
        self.uniforms.bools.use_linear_correction =
            config.blending == AlphaBlending::LinearCorrected;

        let bg_image_config_changed = self.config.bg_image_fit != config.bg_image_fit
            || self.config.bg_image_position != config.bg_image_position
            || self.config.bg_image_repeat != config.bg_image_repeat
            || self.config.bg_image_opacity != config.bg_image_opacity;

        // Zig: `if (self.config.bg_image) |old| if (config.bg_image) |new|
        //          !old.equal(new) else true else config.bg_image != null`.
        // `Path` derives `PartialEq`, so `old != new` == `!old.equal(new)`.
        // macos_impl generic.rs:2779-2784.
        let bg_image_changed = match (&self.config.bg_image, &config.bg_image) {
            (Some(old), Some(new)) => old != new,
            (Some(_), None) => true,
            (None, Some(_)) => true,
            (None, None) => false,
        };

        let old_blending = self.config.blending;
        let custom_shaders_changed = self.config.custom_shaders != config.custom_shaders;

        // `self.config.deinit(); self.config = config.*;` — replace the config
        // (moving the new one in), then `deinit` + drop the old one.
        let mut old = mem::replace(&mut self.config, config);
        old.deinit();

        // If our background image path changed, prepare the new bg image.
        // if (bg_image_changed) try self.prepBackgroundImage();
        //
        // `prep_background_image` decodes the new path (wuffs PNG/JPEG) into a
        // pending `bg_image`; only a DECODE error propagates (open/read failures
        // warn + no-op). unlock-before-`?` (the `Mutex` is guard-less, so
        // `defer draw_mutex.unlock()` is replicated on every early return).
        // macos_impl generic.rs:2805-2810.
        if bg_image_changed {
            self.prep_background_image()?;
        }

        // If our background image config changed, update the vertex buffer.
        if bg_image_config_changed {
            self.update_bg_image_buffer();
        }

        // Reset our viewport to force a rebuild, in case of a font change.
        self.mark_dirty();

        let blending_changed = old_blending != self.config.blending;

        if blending_changed {
            // We update our API's blending mode.
            self.api.blending = self.config.blending;
            // And indicate that we need to reinitialize our shaders.
            self.reinitialize_shaders = true;
            // And that our swap-chain targets need re-creation for the new blending
            // mode. Zig `+%=` -> `wrapping_add`.
            self.target_config_modified = self.target_config_modified.wrapping_add(1);
        }

        if custom_shaders_changed {
            self.reinitialize_shaders = true;
        }

        Ok(())
    }

    /// Call this any time the background image path changes.
    ///
    /// Caller must hold the draw mutex.
    //
    // PORTS: generic.zig:1746-1830 `prepBackgroundImage`; macos_impl
    // generic.rs:2844-2971. FULLY graphics-API-agnostic (file read + wuffs decode +
    // an `image::Pending` hand-off) — copy-adapted VERBATIM from the Metal twin; the
    // only reason it was cut here before is that `crate::image` did not compile on
    // Linux.
    fn prep_background_image(&mut self) -> Result<(), Error> {
        // Then we try to load the background image if we have a path.
        // if (self.config.bg_image) |p| load_background: { ... } else { ... }
        let Some(p) = self.config.bg_image.as_ref() else {
            // If we don't have a background image path, mark our
            // background image for unload if we currently have one.
            // if (self.bg_image) |*img| img.markForUnload();
            if let Some(img) = self.bg_image.as_mut() {
                img.mark_for_unload();
            }
            return Ok(());
        };

        // const path = switch (p) { .required, .optional => |slice| slice };
        //
        // Borrow discipline (same as the Metal twin): `p` borrows `self.config`
        // immutably, but the load block below mutates `self.bg_image`. Copy the
        // null-terminated path bytes into an OWNED `CString` here and drop the
        // `&self.config` borrow before touching `self.bg_image`. Purely mechanical;
        // no fidelity impact.
        let path: CString = p.slice().to_owned();

        // `self.alloc` is a Copy `Global` ZST; capture it so the `'load_background`
        // block reads it without holding a `&self` borrow across the later
        // `self.bg_image` mutation.
        let alloc = self.alloc;

        // 'load_background: { ... break :load_background; } — a labeled block whose
        // early exits (open-fail / read-fail / unknown-type) `break` with `None`
        // ("no change"); a DECODE error `?`-propagates OUT of the whole method
        // (the reference `try`), NOT a `break`.
        let pending: Option<image::Pending> = 'load_background: {
            // Open the file / Read it.
            //
            // The reference `openFileAbsolute` + `readToEndAlloc(self.alloc,
            // maxInt(u32))` (max size 4 GiB) are kept as TWO distinct steps here
            // (they were previously collapsed into one `std.fs.read`) because the
            // reference logs a DIFFERENT message for each — "error opening ..." vs
            // "error reading ..." — and R1 requires the message text verbatim. Both
            // failures map to `break :load_background` (a warn + no-op). The 4 GiB
            // cap is applied AFTER the read (an oversize file is treated as a read
            // failure, the reference's `error.StreamTooLong`), byte-faithfully over
            // the `CString` path bytes (never a lossy `String`).
            //
            // (task-14c-5: split to match `macos_impl`'s `generic.rs` twin, which
            // task-14c-1 split for the same R1 reason. Same two `break`s as before,
            // so decode and caller behaviour are unchanged.)
            //
            // `{s}` on the reference's `[]const u8` path prints the RAW bytes; the
            // closest Rust rendering is `String::from_utf8_lossy` over the same
            // bytes. The lossy conversion is confined to the log message — the path
            // the port actually opens stays the byte-faithful `OsStr`.
            let path_bytes = path.to_bytes();
            let path_display = String::from_utf8_lossy(path_bytes);
            // var file = std.fs.openFileAbsolute(path, .{}) catch |err| { log.warn(...); break; };
            #[cfg(not(windows))]
            let opened = std::fs::File::open(OsStr::from_bytes(path_bytes));
            // Windows: a path that is not WTF-8 is Zig's `error.InvalidWtf8` from
            // `openFileAbsolute`, which takes this same warn-and-break arm.
            #[cfg(windows)]
            let opened = match rustty_os::path::os_string_from_bytes(path_bytes) {
                Some(p) => std::fs::File::open(p),
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "InvalidWtf8",
                )),
            };
            let mut file = match opened {
                Ok(f) => f,
                Err(err) => {
                    // log.warn("error opening background image file \"{s}\": {}", .{ path, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "error opening background image file \"{path_display}\": {err:?}"
                    );
                    break 'load_background None;
                }
            };
            // Read it
            // const contents = file.readToEndAlloc(self.alloc, std.math.maxInt(u32))
            //     catch |err| { log.warn(...); break; };
            //   (`std.math.maxInt(u32), // Max size of 4 GiB, for now.`)
            // defer self.alloc.free(contents);
            let contents: Vec<u8> = {
                let mut buf: Vec<u8> = Vec::new();
                match std::io::Read::read_to_end(&mut file, &mut buf) {
                    Ok(_) if buf.len() <= u32::MAX as usize => buf,
                    // Over the reference's `maxInt(u32)` cap: the reference's
                    // `readToEndAlloc` fails with `error.StreamTooLong`. Zig's `{}`
                    // on an error value prints `error.` + the name, so the oracle's
                    // rendering of this arm is `...: error.StreamTooLong` — the
                    // prefix is part of the message text, not decoration.
                    Ok(_) => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "error reading background image file \"{path_display}\": error.StreamTooLong"
                        );
                        break 'load_background None;
                    }
                    Err(err) => {
                        log::warn!(
                            target: LOG_SCOPE,
                            "error reading background image file \"{path_display}\": {err:?}"
                        );
                        break 'load_background None;
                    }
                }
            };

            // Figure out what type it probably is.
            // const file_type = switch (FileType.detect(contents)) {
            //     .unknown => FileType.guessFromExtension(std.fs.path.extension(path)),
            //     else => |t| t,
            // };
            let file_type = match FileType::detect(&contents) {
                FileType::Unknown => FileType::guessFromExtension(path_extension(path.to_bytes())),
                t => t,
            };

            // Decode it if we know how.
            // const image_data = switch (file_type) {
            //     .png => try wuffs.png.decode(self.alloc, contents),
            //     .jpeg => try wuffs.jpeg.decode(self.alloc, contents),
            //     .unknown => { log.warn("Cannot determine file type ...", .{path}); break; },
            //     else => |f| { log.warn("Unsupported file type {} ...", .{ f, path }); break; },
            // };
            let image_data = match file_type {
                FileType::Png => rustty_wuffs::png::decode(alloc, &contents)?,
                FileType::Jpeg => rustty_wuffs::jpeg::decode(alloc, &contents)?,
                FileType::Unknown => {
                    // log.warn("Cannot determine file type for background image file \"{s}\"!", .{path});
                    log::warn!(
                        target: LOG_SCOPE,
                        "Cannot determine file type for background image file \"{path_display}\"!"
                    );
                    break 'load_background None;
                }
                f => {
                    // log.warn("Unsupported file type {} for background image file \"{s}\"!", .{ f, path });
                    log::warn!(
                        target: LOG_SCOPE,
                        "Unsupported file type {f:?} for background image file \"{path_display}\"!"
                    );
                    break 'load_background None;
                }
            };

            // const image: imagepkg.Image = .{ .pending = .{ .width, .height,
            //     .pixel_format = .rgba, .data = image_data.data.ptr } };
            //
            // Transfer OWNERSHIP of the decoded RGBA buffer to `Pending` (do NOT
            // free it here — `rustty-wuffs` allocated it over the SAME `Global`,
            // align 1, so `image::linux_impl`'s `free_data` `Layout::from_size_align(
            // len, 1)` frees it later without a layout mismatch). `contents` (the
            // encoded bytes) IS freed here, at the block's end (`Vec` drop == the
            // reference `defer self.alloc.free(contents)`).
            let (width, height, buf, _len) = image_data.into_raw();
            Some(image::Pending {
                width,
                height,
                pixel_format: image::PixelFormat::Rgba,
                data: buf.as_ptr(), // *mut u8 — freed later by Image::deinit / free_data
            })
        };

        // If we have an existing background image, replace it.
        // Otherwise, set this as our background image directly. A `None` pending
        // is the `break :load_background` case — leave `self.bg_image` unchanged.
        if let Some(pend) = pending {
            let img = image::Image::Pending(pend);
            // if (self.bg_image) |*img| { img.markForReplace(self.alloc, image); }
            // else { self.bg_image = image; }
            if let Some(existing) = self.bg_image.as_mut() {
                existing.mark_for_replace(self.alloc, img);
            } else {
                self.bg_image = Some(img);
            }
        }

        Ok(())
    }

    // PORTS: generic.zig:836-... `initShaders`; macos_impl generic.rs:5231-5272.
    fn init_shaders(&mut self) -> Result<(), Error> {
        // Load our custom shaders. The reference arena is subsumed by owned
        // `Vec<CString>`; the `catch |err| break :err &.{}` (any load error ⇒ no
        // custom shaders, never block the renderer) maps to `unwrap_or_else`.
        let custom_shaders: Vec<CString> =
            shadertoy::load_from_files(&self.config.custom_shaders, OpenGL::CUSTOM_SHADER_TARGET)
                .unwrap_or_else(|err| {
                    // log.warn("error loading custom shaders err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "error loading custom shaders err={err:?}");
                    Vec::new()
                });

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

    /// This is called by the GTK apprt after the surface is
    /// reinitialized (e.g. after the widget is re-realized following
    /// a display change or reparenting).
    //
    // PORTS: generic.zig:947-968 `displayRealized` (`2f0b65346`).
    //
    // ```zig
    // pub fn displayRealized(self: *Self) !void {
    //     // If our API has to do things on realize, let it.
    //     if (@hasDecl(GraphicsAPI, "displayRealized")) {
    //         self.api.displayRealized();
    //     }
    //
    //     // Lock the draw mutex so that we can safely update state.
    //     self.draw_mutex.lockUncancelable(global.io());
    //     defer self.draw_mutex.unlock(global.io());
    //
    //     // Mark the display as realized. ...
    //     self.display_realized = true;
    //     self.reinitialize_shaders = true;
    //     self.target_config_modified = 1;
    // }
    // ```
    //
    // `OpenGL` no longer declares `displayRealized` (its GTK `prepareContext(null)`
    // arm went with the GLArea), so the `@hasDecl` arm is statically false. The
    // lock is the caller's: the main thread calls this through `renderer_seam` with
    // its own `draw_mutex` handle (N437).
    //
    // PORT NOTE: the inferred `!void` has an empty error set now; the `Result`
    // stays so the core seam's `try` reads as the reference's.
    pub fn display_realized_locked(&mut self) -> Result<(), Error> {
        // Mark the display as realized. The render thread will lazily
        // rebuild the swap chain and shaders on the next `drawFrame`,
        // which is the right place for GL resource creation (it
        // guarantees a current context on the render thread).
        self.display_realized = true;
        self.reinitialize_shaders = true;
        self.target_config_modified = 1;
        Ok(())
    }

    /// This is called when the surface is being unrealized.
    /// This can happen because the surface is being closed but
    /// also when moving the window between displays or splitting.
    ///
    /// This runs on the main thread and only updates CPU-side state
    /// here; resource cleanup happens on the render thread via
    /// `releaseGpuResources`.
    //
    // PORTS: generic.zig:970-985 `displayUnrealized` (`2f0b65346`).
    //
    // ```zig
    // pub fn displayUnrealized(self: *Self) void {
    //     // Lock the draw mutex so that we can safely update state.
    //     self.draw_mutex.lockUncancelable(global.io());
    //     defer self.draw_mutex.unlock(global.io());
    //
    //     // Clearing `display_realized` ensures drawFrame doesn't attempt
    //     // to rebuild the swap chain or make any graphics API calls.
    //     // The actual GPU resource release is done by the render thread.
    //     self.display_realized = false;
    // }
    // ```
    //
    // The lock is the caller's (N437), as for `display_realized_locked`.
    pub fn display_unrealized_locked(&mut self) {
        // Clearing `display_realized` ensures drawFrame doesn't attempt
        // to rebuild the swap chain or make any graphics API calls.
        // The actual GPU resource release is done by the render thread.
        self.display_realized = false;
    }

    /// Push the latest completed frame, replacing if one previously
    /// existed. Called on the render thread.
    ///
    /// Has no effect for renderers that do not export frames
    /// (i.e. `ExportedFrame == void`).
    // pub fn pushFrame(self: *Self, frame: ExportedFrame) void {
    //     return self.latest_frame.push(global.io(), frame);
    // }   (generic.zig:1028-1034)
    pub fn push_frame(&self, frame: ExportedFrame) {
        self.latest_frame.push(frame);
    }

    /// Take the latest completed frame for the apprt to composite.
    /// Returns null if no frame is available. The caller takes
    /// ownership of the returned frame. Called on the main thread.
    ///
    /// Has no effect for renderers that do not export frames
    /// (i.e. `ExportedFrame == void`).
    // pub fn takeFrame(self: *Self) ?ExportedFrame {
    //     return self.latest_frame.take(global.io());
    // }   (generic.zig:1036-1044)
    //
    // PORT NOTE (N437): the main thread takes frames through
    // [`Self::latest_frame_handle`], which reaches the slot without touching the
    // renderer; this method is that handle's `take` for callers that own the
    // renderer outright (the tests).
    pub fn take_frame(&self) -> Option<ExportedFrame> {
        self.latest_frame.take()
    }

    /// A handle to the `latest_frame` slot for the apprt's main thread (N437).
    /// Taken once, before the render thread starts.
    pub fn latest_frame_handle(&self) -> Arc<LatestFrame<ExportedFrame>> {
        self.latest_frame.clone()
    }

    /// Called when the apprt reports a change in how well
    /// frames can be presented.
    // pub fn setPresentationHealth(self: *Self, health: Health) void {
    //     self.presentation_health.store(health, .seq_cst);
    // }   (generic.zig:1163-1167)
    pub fn set_presentation_health(&self, health: crate::Health) {
        self.presentation_health.store(health);
    }

    /// Returns how well frames can be presented.
    // pub fn presentationHealth(self: *Self) Health {
    //     return self.presentation_health.load(.seq_cst);
    // }   (generic.zig:1169-1172)
    pub fn presentation_health(&self) -> crate::Health {
        self.presentation_health.load()
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

            if let Err(err) = self.rebuild_row(
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
                log::warn!(target: LOG_SCOPE, "error building row y={y} err={err:?}");
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
                    if let Err(err) = self.add_preedit_cell(
                        *cp,
                        point::Coordinate {
                            x,
                            y: range.y as u32,
                        },
                        hoisted_foreground,
                    ) {
                        // log.warn("error building preedit cell, will be invalid
                        //           x={} y={}, err={}", .{ x, range.y, err });
                        log::warn!(
                            target: LOG_SCOPE,
                            "error building preedit cell, will be invalid x={} y={}, err={:?}",
                            x,
                            range.y,
                            err
                        );
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
                                // self.font_shaper_cache.put(self.alloc, run, new_cells) catch |err| {
                                //     log.warn("error caching font shaping results err={}", .{err});
                                // };
                                //
                                // LOG PORT NOTE (task-14c-5, N9 — Linux-side residual
                                // of the WAIVED row S116-F2): this `log.warn` is NOT
                                // restorable, exactly as in the Metal twin
                                // (task-14c-1). `ShaperCache::put`
                                // (`rustty-font-shaper/src/shaper/cache.rs:136`) is
                                // INFALLIBLE in rustty — it allocates through the
                                // infallible `Global`/`Box` path — so there is no
                                // `catch` arm to hang the warn on and emitting it
                                // unconditionally would put a diagnostic on the
                                // SUCCESS path. See ADJUDICATION.md#s116-f2.
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
                if let Err(err) = unsafe {
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
                    log::warn!(
                        target: LOG_SCOPE,
                        "error adding underline to cell, will be invalid x={x} y={y}, err={err:?}"
                    );
                }
            }

            if style.flags.overline {
                // SAFETY (§C.3): raw-self helper — see `self_raw`/`add_underline`.
                if let Err(err) =
                    unsafe { Renderer::add_overline(self_raw, x as CellCountInt, y, fg, alpha) }
                {
                    // log.warn("error adding overline to cell, will be invalid
                    //           x={} y={}, err={}", .{ x, y, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "error adding overline to cell, will be invalid x={x} y={y}, err={err:?}"
                    );
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
                                // self.font_shaper_cache.put(self.alloc, run, new_cells) catch |err| {
                                //     log.warn("error caching font shaping results err={}", .{err});
                                // };
                                //
                                // LOG PORT NOTE (task-14c-5, N9 — Linux-side residual
                                // of the WAIVED row S116-F2): this `log.warn` is NOT
                                // restorable, exactly as in the Metal twin
                                // (task-14c-1). `ShaperCache::put`
                                // (`rustty-font-shaper/src/shaper/cache.rs:136`) is
                                // INFALLIBLE in rustty — it allocates through the
                                // infallible `Global`/`Box` path — so there is no
                                // `catch` arm to hang the warn on and emitting it
                                // unconditionally would put a diagnostic on the
                                // SUCCESS path. See ADJUDICATION.md#s116-f2.
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
                        if let Err(err) = unsafe {
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
                            log::warn!(
                                target: LOG_SCOPE,
                                "error adding glyph to cell, will be invalid x={x} y={y}, err={err:?}"
                            );
                        }
                        shaper_cells_i += 1;
                    }
                }
            }

            // Finally, draw a strikethrough if necessary.
            if style.flags.strikethrough {
                // SAFETY (§C.3): raw-self helper — see `self_raw`/`add_underline`.
                if let Err(err) = unsafe {
                    Renderer::add_strikethrough(self_raw, x as CellCountInt, y, fg, alpha)
                } {
                    // log.warn("error adding strikethrough to cell, will be invalid
                    //           x={} y={}, err={}", .{ x, y, err });
                    log::warn!(
                        target: LOG_SCOPE,
                        "error adding strikethrough to cell, will be invalid x={x} y={y}, err={err:?}"
                    );
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
                    Err(err) => {
                        // log.warn("error rendering cursor glyph err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error rendering cursor glyph err={err:?}");
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
                        log::warn!(
                            target: LOG_SCOPE,
                            "failed to find lock symbol for cursor codepoint=0xF023"
                        );
                        return;
                    }
                    Err(err) => {
                        // log.warn("error rendering cursor glyph err={}", .{err});
                        log::warn!(target: LOG_SCOPE, "error rendering cursor glyph err={err:?}");
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
                log::warn!(
                    target: LOG_SCOPE,
                    "failed to find font for preedit codepoint={:X}",
                    cp.codepoint
                );
                return Ok(());
            }
            Err(err) => {
                // log.warn("error rendering preedit glyph err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error rendering preedit glyph err={err:?}");
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

    // fn rebuildOverlay(self, features: []const Overlay.Feature) Overlay.InitError!void
    //
    /// Build the overlay as configured. Returns null if there is no
    /// overlay currently configured.
    //
    // PORTS: generic.zig:2230-… `rebuildOverlay`; macos_impl generic.rs:4539-4622.
    // PURE CPU/z2d — no graphics-API surface at all (the `Overlay` is a z2d software
    // surface; the GPU only ever sees it later via `images.overlay_update`), so this
    // is a VERBATIM copy-adapt of the Metal twin.
    pub fn rebuild_overlay(
        &mut self,
        features: &[crate::overlay::Feature],
    ) -> Result<(), crate::overlay::InitError> {
        // const alloc = self.alloc;
        // (rustty `Overlay` drops the reference `alloc` arg — its z2d surface
        // owns its `Vec`; `Overlay::init`/`apply_features` take no `alloc`.)

        // If we have no features enabled, don't build an overlay.
        // If we had a previous overlay, deallocate it.
        if features.is_empty() {
            if let Some(old) = self.overlay.as_mut() {
                old.deinit();
                self.overlay = None;
            }

            return Ok(());
        }

        // If we had a previous overlay, clear it. Otherwise, init.
        'overlay: {
            if let Some(v) = self.overlay.as_mut() {
                'existing: {
                    // Verify that our overlay size matches our screen
                    // size as we know it now. If not, deinit and reinit.
                    // Note: these intCasts are always safe because z2d
                    // stores as i32 but we always init with a u32.
                    let width: u32 = v.surface.get_width() as u32;
                    let height: u32 = v.surface.get_height() as u32;
                    let term_size = self.size.terminal();
                    if width != term_size.width || height != term_size.height {
                        break 'existing;
                    }

                    // We also depend on cell size.
                    if v.cell_size.width != self.size.cell.width
                        || v.cell_size.height != self.size.cell.height
                    {
                        break 'existing;
                    }

                    // Everything matches, so we can just reset the surface
                    // and redraw.
                    v.reset();
                    break 'overlay;
                }
            }

            // If we reached this point we want to reset our overlay.
            if let Some(v) = self.overlay.as_mut() {
                v.deinit();
                self.overlay = None;
            }

            assert!(self.overlay.is_none());
            let new: Overlay = Overlay::init(self.size)?;
            self.overlay = Some(new);
        }

        // overlay.applyFeatures(alloc, &self.terminal_state, features);
        // SAFETY: `self.overlay` and `self.terminal_state` are DISJOINT fields; the
        // raw ptr detaches the immutable `terminal_state` read from the `&mut
        // self.overlay` borrow (a field-level reborrow does not retag
        // `terminal_state`).
        let terminal_state: *const RenderState = &self.terminal_state;
        self.overlay
            .as_mut()
            .unwrap()
            .apply_features(unsafe { &*terminal_state }, features);
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
    // HISTORY (both L4 scope cuts are now RESOLVED; nothing here is elided):
    //   1. RESOLVED (w2 F-005) — the Kitty-graphics image state
    //      (`self.images.kitty_requires_update`/`kitty_update`) is LIVE below;
    //      `crate::image` is a twin pair now, so `self.images` is a real field.
    //   2. RESOLVED (w2 F-020) — the inspector debug-overlay chain (`state.inspector`
    //      → `overlay_features` → `rebuild_overlay`, macos_impl generic.rs:4856-4873,
    //      :4959) and the overlay image upload (`self.images.overlay_update`,
    //      :5026-5028) are LIVE below.
    //   3. NOT A CUT — the macOS-only glass-background alpha zero-out (macos_impl
    //      generic.rs:5007-5021) is guarded by `if (comptime builtin.os.tag ==
    //      .macos)` in generic.zig; on Linux that branch is COMPILED OUT, so its
    //      absence here IS the faithful port.
    // Every other line is copy-adapted verbatim from `macos_impl`.
    pub fn update_frame(
        &mut self,
        state: &state::State,
        cursor_blink_visible: bool,
    ) -> Result<(), VtOutOfMemory> {
        let mutex = self.draw_mutex.clone();
        mutex.lock();
        let result = self.update_frame_locked(state, cursor_blink_visible);
        mutex.unlock();
        result
    }

    /// [`Self::update_frame`] with `draw_mutex` held by the caller for the WHOLE
    /// call.
    //
    // PORT NOTE (N437): the reference takes `draw_mutex` twice inside
    // `updateFrame` (around the Kitty image update and around the GPU-cell
    // rebuild) and touches the rest of the renderer unlocked, because only the
    // render thread writes those fields. In Rust the render thread's
    // whole-renderer `&mut` would then overlap the main thread's own locked
    // accesses (a foreign access to protected bytes under Stacked and Tree
    // Borrows), so the render thread takes the lock first, through its own
    // handle, and holds it across the whole update; the two inner lock pairs are
    // the caller's. Lock order: draw mutex, then the terminal-state mutex; no
    // path takes them the other way round.
    pub fn update_frame_locked(
        &mut self,
        state: &state::State,
        cursor_blink_visible: bool,
    ) -> Result<(), VtOutOfMemory> {
        // CoreText shaping accumulates objects for deferred release over
        // the course of a frame. Always flush those objects, including
        // when rebuilding the frame fails due to memory pressure.
        // defer self.font_shaper.endFrame();
        //
        // PORT NOTE (parity S115-F1): the reference's first statement is
        // this `defer`, so `endFrame` runs on every exit: the
        // synchronized-output early `return`, a failed `beginUpdate`, and
        // the normal end. The body is `update_frame_impl`; running
        // `end_frame` after it here is that defer. It also keeps the
        // reference's order at the normal end: the later-registered `defer
        // self.terminal_state.dirty = .false` (the impl's last statement)
        // runs before `endFrame`. CORRECTION: `end_frame` used to be a plain
        // statement at the end of the success path only.
        // UNVERIFIED on Linux: written on macOS, compile and suite pending on
        // rvm (NEW-FINDINGS N181).
        let result = self.update_frame_impl(state, cursor_blink_visible);
        self.font_shaper.end_frame();
        result
    }

    /// The body of [`Self::update_frame`], which runs
    /// `font_shaper.end_frame()` after it on every exit.
    fn update_frame_impl(
        &mut self,
        state: &state::State,
        cursor_blink_visible: bool,
    ) -> Result<(), VtOutOfMemory> {
        // We fully deinit and reset the terminal state every so often so that a
        // particularly large terminal state doesn't cause the renderer to hold on to
        // retained memory. Frame count is ~12 minutes at 120Hz.
        let max_terminal_state_frame_count = 100_000;
        if self.terminal_state_frame_count >= max_terminal_state_frame_count {
            self.terminal_state.deinit(self.alloc.into());
            self.terminal_state = RenderState::empty();
            self.terminal_state_frame_count = 0;
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
            overlay_features: Vec<crate::overlay::Feature>,
        }

        // The renderer-thread state carries its terminal behind a `*mut Terminal` (a
        // separate allocation, disjoint from `self`). Copy the stored raw pointer and
        // re-deref it FRESH at every use, matching the Zig `state.terminal.…` re-walk.
        let t = state.terminal; // *mut Terminal

        // Hand-rolled `defer state.unlockDemand()` (macos_impl §C.4/RISK-5): the
        // guard-less `Mutex` has no RAII drop, and the lock is held across THREE exits
        // — the sync-output early `return`, the `terminal_state.update(...)?`
        // propagation, and normal fall-through. This `Drop` fires on all three.
        //
        // (D4, oracle d34b54e9b: the guard now borrows the whole `State` because the
        // release path is `unlock_demand`, not a bare `mutex.unlock` — it must also
        // bump the handoff generation and wake parked yielders. A shared `&State` is
        // disjoint from the `&mut self` uses inside the block, exactly as the bare
        // `&Mutex` was.)
        struct Unlock<'a>(&'a state::State);
        impl Drop for Unlock<'_> {
            fn drop(&mut self) {
                // SAFETY: same as the `lock_demand` below — `state.mutex` is the live
                // shared render mutex, held by this frame.
                unsafe { self.0.unlock_demand() };
            }
        }

        // Update all our data as tightly as possible within the mutex.
        let mut critical: Critical = {
            // Lock while signaling demand so the IO parse thread can't starve us.
            // See renderer::State::lock_demand.
            //
            // SAFETY: `state.mutex` points at the live shared render-state mutex and
            // this frame does not already hold it.
            unsafe {
                state.lock_demand();
            }
            let _mutex_guard = Unlock(state);

            // If we're in a synchronized output state, we pause all rendering.
            if unsafe { (*t).modes.get(Mode::SynchronizedOutput) } {
                // log.debug("synchronized output started, skipping render", .{});
                log::debug!(target: LOG_SCOPE, "synchronized output started, skipping render");
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
            self.terminal_state
                .update(self.alloc.into(), unsafe { &mut *t })?;

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

            // Advance any running Kitty graphics animations to the
            // frame due now, and remember when the next frame is
            // due (as an absolute deadline, see animationWake) so
            // the renderer thread can schedule a wakeup for it.
            // This must happen before the dirty check below:
            // advancing a frame marks the image state dirty.
            //
            // (the macos twin carries the verbatim Zig quote — API-free code,
            // copied verbatim.)
            //
            // SAFETY (assessment D hazard, `animationTick` threading): the
            // tick MUTATES kitty storage (frame advance + dirty stamp) from
            // the render thread while the terminal is LOCKED (this whole
            // block runs under the renderer/terminal mutex, like the
            // `begin_update` above) — the IO thread cannot interleave. The
            // `&mut` is re-derived from the raw screen ptr for exactly this
            // statement (no borrow held across).
            self.kitty_animation_next_ms = 'next: {
                // Likely case: we have no kitty images, so do nothing.
                let active = unsafe { (*t).screens.active };
                if unsafe { &(*active).kitty_images }.images.is_empty() {
                    break 'next None;
                }

                let now = Instant::now();
                let base = match self.kitty_animation_clock {
                    Some(base) => base,
                    None => {
                        self.kitty_animation_clock = Some(now);
                        now
                    }
                };
                let now_ms: u64 = now.duration_since(base).as_millis() as u64;
                let delay = match unsafe { &mut (*active).kitty_images }.animation_tick(now_ms) {
                    Some(delay) => delay,
                    None => break 'next None,
                };
                break 'next Some(now_ms + delay);
            };

            // If we have Kitty graphics data, we enter a SLOW SLOW SLOW path.
            // We only do this if the Kitty image state is dirty meaning only if
            // it changes.
            //
            // If we have any virtual references, we must also rebuild our
            // kitty state on every frame because any cell change can move
            // an image.
            //
            // PORTS: generic.zig:1229-1248; macos_impl generic.rs:4813-4833.
            if self.images.kitty_requires_update(unsafe { &*t }) {
                // We need to grab the draw mutex since this updates
                // our image state that drawFrame uses.
                // (N437: held by the caller of `update_frame_locked`.)
                self.images.kitty_update(
                    self.alloc,
                    unsafe { &*t },
                    CellSize {
                        width: self.grid_metrics.cell_width,
                        height: self.grid_metrics.cell_height,
                    },
                );
            }

            // Get our OSC8 links we're hovering if we have a mouse.
            let links: CellSet = match state.mouse.point {
                None => CellSet::new(),
                Some(vp) => {
                    if !state.mouse.mods.equal(key::ctrl_or_super(Mods::default())) {
                        CellSet::new()
                    } else {
                        self.terminal_state
                            .link_cells(Global.into(), vp)
                            .unwrap_or_else(|err| {
                                // log.warn("error searching for OSC8 links err={}", .{err});
                                log::warn!(
                                    target: LOG_SCOPE,
                                    "error searching for OSC8 links err={err:?}"
                                );
                                CellSet::new()
                            })
                    }
                }
            };

            // PORTS: generic.zig:1269-1283; macos_impl generic.rs:4856-4873.
            //   const insp = state.inspector orelse break :overlay &.{};
            //   const renderer_info = insp.rendererInfo();
            //   break :overlay renderer_info.overlayFeatures(arena_alloc) catch &.{};
            //
            // R2 ACCESSOR-TRAIT SEAM: `state.inspector` is the renderer-owned
            // `*mut dyn InspectorOverlay` fat pointer (state.rs) — the acyclic
            // accessor trait (INV-7). The renderer reads the enabled overlay features
            // THROUGH the trait, never naming a `rustty-inspector` type. The
            // reference's `insp.rendererInfo()` navigation happens at STORE time
            // (core coerces the `Info` into the fat pointer), so here the one-method
            // trait call subsumes the whole chain. INV-1: this deref is inside the
            // `state.mutex`-guarded critical section; the raw pointer never escapes
            // the guard.
            let overlay_features: Vec<crate::overlay::Feature> = match state.inspector {
                None => Vec::new(),
                Some(insp) => unsafe { (*insp).overlay_features() },
            };

            Critical {
                links,
                mouse: state.mouse,
                preedit,
                scrollbar,
                overlay_features,
            }
        };

        // Outside the critical area we can update our links to contain our regex
        // results.
        // catch |err| log.warn("error searching for regex links err={}", .{err});
        if let Err(err) = self.config.links.render_cell_map(
            Global.into(),
            &mut critical.links,
            &self.terminal_state,
            state.mouse.point,
            state.mouse.mods,
        ) {
            log::warn!(target: LOG_SCOPE, "error searching for regex links err={err:?}");
        }

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
                // catch |err| {
                //     // Not a critical error, we just won't show highlights.
                //     log.warn("error updating search selected highlight err={}", .{err});
                // }
                if let Err(err) = self.terminal_state.update_highlights_flattened(
                    self.alloc.into(),
                    HighlightTag::SearchMatchSelected as u8,
                    core::slice::from_ref(&m.r#match),
                ) {
                    // Not a critical error, we just won't show highlights.
                    log::warn!(
                        target: LOG_SCOPE,
                        "error updating search selected highlight err={err:?}"
                    );
                }
            }
            if let Some(m) = &self.search_matches {
                // catch |err| {
                //     // Not a critical error, we just won't show highlights.
                //     log.warn("error updating search highlights err={}", .{err});
                // }
                if let Err(err) = self.terminal_state.update_highlights_flattened(
                    self.alloc.into(),
                    HighlightTag::SearchMatch as u8,
                    &m.matches,
                ) {
                    // Not a critical error, we just won't show highlights.
                    log::warn!(target: LOG_SCOPE, "error updating search highlights err={err:?}");
                }
            }
        }

        // From this point forward no more errors.

        // Rebuild the overlay image if we have one. We can do this
        // outside of any critical areas.
        // self.rebuildOverlay(critical.overlay_features) catch |err|
        //     log.warn("error rebuilding overlay surface err={}", .{err});
        //
        // PORTS: generic.zig:1356-1365; macos_impl generic.rs:4959.
        if let Err(err) = self.rebuild_overlay(&critical.overlay_features) {
            log::warn!(target: LOG_SCOPE, "error rebuilding overlay surface err={err:?}");
        }

        // Acquire the draw mutex for all remaining state updates.
        // (N437: held by the caller of `update_frame_locked`.)
        {
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
            // catch |err| {
            //     // This means we weren't able to allocate our buffer
            //     // to update the cells. In this case, we continue with
            //     // our old buffer (frozen contents) and log it.
            //     comptime assert(@TypeOf(err) == error{OutOfMemory});
            //     log.warn("error rebuilding GPU cells err={}", .{err});
            // }
            //
            // LOG PORT NOTE (task-14c-5, N9 — Linux-side residual of the WAIVED row
            // S115-F4): `rebuild_cells` is INFALLIBLE in rustty (it returns `()`;
            // its sole OOM source aborts via the `Global` allocator), so the
            // reference's "continue with frozen contents" catch path is unreachable
            // here and this `log.warn` is NOT restorable — emitting it
            // unconditionally would put a diagnostic on the SUCCESS path. Same
            // ruling as the Metal twin (task-14c-1, generic.rs:5412-5427). See
            // ADJUDICATION.md#s115-f4.

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

            // NOT A CUT (macOS glass background): macos_impl generic.rs:5007-5021 is
            // guarded by `if (comptime builtin.os.tag == .macos)` in generic.zig;
            // that branch is COMPILED OUT on Linux, so it is absent here by fidelity.

            // Prepare our overlay image for upload (or unload). This has to use our
            // general allocator since it modifies state that survives frames.
            // self.images.overlayUpdate(self.alloc, self.overlay) catch |err|
            //     log.warn("error updating overlay images err={}", .{err});
            //
            // PORTS: generic.zig:1416-1424; macos_impl generic.rs:5026-5028.
            // (`Option<&Overlay>` — the borrowed-ownership deviation documented on
            //  `image::linux_impl::State::overlay_update`.)
            if let Err(err) = self
                .images
                .overlay_update(self.alloc, self.overlay.as_ref())
            {
                log::warn!(target: LOG_SCOPE, "error updating overlay images err={err:?}");
            }

            // Update custom shader uniforms that depend on terminal state.
            // self.updateCustomShaderUniformsFromState();
            self.update_custom_shader_uniforms_from_state();
        }

        // Reset our dirty state after updating (the reference `defer
        // self.terminal_state.dirty = .false`, fired at fn scope exit — §C.4).
        self.terminal_state.dirty = Dirty::False;

        Ok(())
    }

    /// Update custom shader uniforms that depend on terminal state.
    ///
    /// This should be called in `updateFrame` when terminal state changes.
    //
    // PORTS: generic.zig `updateCustomShaderUniformsFromState`; copy-adapted verbatim
    // from macos_impl generic.rs:4926-5041 (widened to `pub` there for direct unit
    // tests; kept `pub` here for the same reason).
    pub fn update_custom_shader_uniforms_from_state(&mut self) {
        // We only need to do this if we have custom shaders.
        if !self.has_custom_shaders {
            return;
        }

        // Only update when terminal state is dirty.
        if self.terminal_state.dirty == Dirty::False {
            return;
        }

        let uniforms: &mut shadertoy::Uniforms = &mut self.custom_shader_uniforms;
        let colors: &render::Colors = &self.terminal_state.colors;

        // 256-color palette
        for (i, color) in colors.palette.iter().enumerate() {
            // S117-F1: `palette` is `[shadertoy::Vec4; 256]` (std140-forcing
            // newtype — see shadertoy.rs), hence the explicit wrap.
            uniforms.palette[i] = shadertoy::Vec4([
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                1.0,
            ]);
        }

        // Background color
        uniforms.background_color = [
            colors.background.r as f32 / 255.0,
            colors.background.g as f32 / 255.0,
            colors.background.b as f32 / 255.0,
            1.0,
        ];

        // Foreground color
        uniforms.foreground_color = [
            colors.foreground.r as f32 / 255.0,
            colors.foreground.g as f32 / 255.0,
            colors.foreground.b as f32 / 255.0,
            1.0,
        ];

        // Cursor color
        if let Some(cursor_color) = colors.cursor {
            uniforms.cursor_color = [
                cursor_color.r as f32 / 255.0,
                cursor_color.g as f32 / 255.0,
                cursor_color.b as f32 / 255.0,
                1.0,
            ];
        }

        // NOTE: the following could be optimized to follow a change in
        // config for a slight optimization however this is only 12 bytes
        // each being updated and likely isn't a cause for concern

        // Cursor text color
        //
        // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD57), for this
        // uniform and the two selection uniforms below. The reference reads
        // `.color.r/g/b` on a `?TerminalColor` whose other tags,
        // `cell-foreground` and `cell-background`, are documented values of these
        // options; with any `custom-shader` it panics in ReleaseSafe. A
        // cell-relative value names no single colour for a whole-surface uniform,
        // so the port writes each uniform only for a `.color` value and otherwise
        // leaves it unchanged, as the reference does when the option is unset.
        // The full rationale is at the twin site in `generic.rs`
        // (`update_custom_shader_uniforms_from_state`).
        if let Some(TerminalColor::Color(c)) = self.config.cursor_text {
            uniforms.cursor_text = [
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
                1.0,
            ];
        }

        // Selection background color
        if let Some(TerminalColor::Color(c)) = self.config.selection_background {
            uniforms.selection_background_color = [
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
                1.0,
            ];
        }

        // Selection foreground color
        if let Some(TerminalColor::Color(c)) = self.config.selection_foreground {
            uniforms.selection_foreground_color = [
                c.r as f32 / 255.0,
                c.g as f32 / 255.0,
                c.b as f32 / 255.0,
                1.0,
            ];
        }

        // Cursor visibility
        uniforms.cursor_visible = self.terminal_state.cursor.visible as i32;

        // Cursor style
        let cursor_style: cursor::Style =
            cursor::Style::from_terminal(self.terminal_state.cursor.visual_style);
        uniforms.previous_cursor_style = uniforms.current_cursor_style;
        uniforms.current_cursor_style = cursor_style as i32;
    }

    /// Update per-frame custom shader uniforms.
    ///
    /// This should be called exactly once per frame, inside `drawFrame`.
    //
    // PORTS: generic.zig `updateCustomShaderUniformsForFrame`; copy-adapted verbatim
    // from macos_impl generic.rs:5052-5169 (the sole `try` — `std.time.Instant.now()`
    // — is infallible in Rust ⇒ `()`). `CUSTOM_SHADER_Y_IS_DOWN` is `false` here
    // (`OpenGL.zig:27-28`; Metal's is `true`), so the flipped-Y branches are LIVE.
    pub fn update_custom_shader_uniforms_for_frame(&mut self) {
        // We only need to do this if we have custom shaders.
        if !self.has_custom_shaders {
            return;
        }

        let now = Instant::now();
        // defer self.last_frame_time = now; (set as the LAST statement — §E.6)
        let first_frame_time = match self.first_frame_time {
            Some(t) => t,
            None => {
                self.first_frame_time = Some(now);
                now
            }
        };
        let last_frame_time = self.last_frame_time.unwrap_or(now);

        let u: &mut shadertoy::Uniforms = &mut self.custom_shader_uniforms;

        let since_ns: f32 = now.duration_since(first_frame_time).as_nanos() as f32;
        u.time = since_ns / 1_000_000_000.0;

        let delta_ns: f32 = now.duration_since(last_frame_time).as_nanos() as f32;
        u.time_delta = delta_ns / 1_000_000_000.0;

        u.frame += 1;

        let screen = self.size.screen;
        let padding = self.size.padding;
        let cell = self.size.cell;

        u.resolution = [screen.width as f32, screen.height as f32, 1.0];
        u.channel_resolution[0] = [screen.width as f32, screen.height as f32, 1.0, 0.0];

        if let Some(cursor) = self.cells.get_cursor_glyph() {
            let cursor_width: f32 = cursor.glyph_size[0] as f32;
            let cursor_height: f32 = cursor.glyph_size[1] as f32;

            // Left edge of the cell the cursor is in.
            //
            // PORT NOTE (macos_impl §E.6): `cursor.grid_pos: [u16; 2]` but
            // `cell.{width,height}` / `padding.{left,top}` are `u32` — the reference
            // `grid_pos[0] * cell.width + padding.left` relies on Zig's implicit
            // widening. Rust has no implicit widening, so widen `grid_pos` to `u32`
            // BEFORE the `as f32`.
            //
            // PORT NOTE: reference defect (R13), UD76 (task R13-C2): saturating,
            // as in `generic.rs` (macOS).
            let mut pixel_x: f32 = (cursor.grid_pos[0] as u32)
                .saturating_mul(cell.width)
                .saturating_add(padding.left) as f32;
            // Top edge, relative to the top of the
            // screen, of the cell the cursor is in.
            let mut pixel_y: f32 = (cursor.grid_pos[1] as u32)
                .saturating_mul(cell.height)
                .saturating_add(padding.top) as f32;

            // If +Y is up in our shaders, we need to flip the coordinate
            // so that it's instead the top edge of the cell relative to
            // the *bottom* of the screen.
            if !CUSTOM_SHADER_Y_IS_DOWN {
                pixel_y = screen.height as f32 - pixel_y;
            }

            // Add the X bearing to get the -X (left) edge of the cursor.
            pixel_x += cursor.bearings[0] as f32;

            // How we deal with the Y bearing depends on which direction
            // is "up", since we want our final `pixel_y` value to be the
            // +Y edge of the cursor.
            if CUSTOM_SHADER_Y_IS_DOWN {
                // As a reminder, the Y bearing is the distance from the
                // bottom of the cell to the top of the glyph, so to get
                // the +Y edge we need to add the cell height, subtract
                // the Y bearing, and add the glyph height to get the +Y
                // (bottom) edge of the cursor.
                pixel_y += cell.height as f32;
                pixel_y -= cursor.bearings[1] as f32;
                pixel_y += cursor.glyph_size[1] as f32;
            } else {
                // If the Y direction is reversed though, we instead want
                // the *top* edge of the cursor, which means we just need
                // to subtract the cell height and add the Y bearing.
                pixel_y -= cell.height as f32;
                pixel_y += cursor.bearings[1] as f32;
            }

            // S117-F1: `current_cursor`/`previous_cursor` are `shadertoy::Vec4`
            // (the std140-forcing newtype — see shadertoy.rs), so this local is
            // typed the same for the comparisons/assignments below to compile.
            let new_cursor: shadertoy::Vec4 =
                shadertoy::Vec4([pixel_x, pixel_y, cursor_width, cursor_height]);
            let cursor_color: [f32; 4] = [
                cursor.color[0] as f32 / 255.0,
                cursor.color[1] as f32 / 255.0,
                cursor.color[2] as f32 / 255.0,
                cursor.color[3] as f32 / 255.0,
            ];

            let cursor_changed: bool =
                new_cursor != u.current_cursor || cursor_color != u.current_cursor_color;

            if cursor_changed {
                u.previous_cursor = u.current_cursor;
                u.previous_cursor_color = u.current_cursor_color;
                u.current_cursor = new_cursor;
                u.current_cursor_color = cursor_color;
                u.cursor_change_time = u.time;
            }
        }

        // Update focus uniforms
        u.focus = self.focused as i32;

        // If we need to update the time our focus state changed
        // then update it to our current frame time. This may not be
        // exactly correct since it is frame time, not exact focus
        // time, but focus time on its own isn't exactly correct anyways
        // since it comes async from a message.
        if self.custom_shader_focused_changed && self.focused {
            u.time_focus = u.time;
            self.custom_shader_focused_changed = false;
        }

        // defer self.last_frame_time = now; (Zig fn-scope-exit defer — §E.6)
        self.last_frame_time = Some(now);
    }

    /// Sync the atlas data to the given texture. This copies the bytes associated with
    /// the atlas to the given texture. If the atlas no longer fits into the texture,
    /// the texture will be resized.
    //
    // PORTS: generic.zig:1234-1257 `syncAtlasTexture`; macos_impl generic.rs:5291-5317.
    //
    // PORT NOTE (raw-ptr — the same MIRIHAR-proven shape as the Metal twin): the
    // reference is `syncAtlasTexture(self: *const Self, atlas: *const font.Atlas,
    // texture: *Texture)` — `self` and `texture` are RAW pointers. A `&self`/`&mut
    // texture` form is unsound because `texture` is `&mut frame.grayscale`, which
    // aliases INTO `self.swap_chain`. Ported as raw self + raw texture: it reborrows
    // only `self.api` (narrow, disjoint from `swap_chain`) and writes `*texture`.
    // `atlas` stays a real `&FontAtlas` — it lives in the `*mut SharedGrid` (a SEPARATE
    // allocation).
    //
    // DIVERGENCE from `macos_impl` (Metal `Texture` is ARC-`Retained`, dropped on
    // reassign): the OpenGL `Texture` has a MANUAL `deinit(self)` and no `Drop`, so the
    // old texture is freed EXPLICITLY before the reassignment (Zig `texture.*.deinit()`,
    // generic.zig:1240) rather than by an implicit ARC release.
    //
    // # Safety
    // `self_raw`/`texture` must be valid, well-aligned, whole-provenance pointers that
    // outlive the call; `texture` must not alias `atlas`.
    unsafe fn sync_atlas_texture(
        self_raw: *const Renderer,
        atlas: &FontAtlas,
        texture: *mut Texture,
    ) -> Result<(), Error> {
        // if (atlas.size > texture.width) {
        if atlas.size() as usize > unsafe { (*texture).width } {
            // Free our old texture / Reallocate.
            // texture.*.deinit();  texture.* = try self.api.initAtlasTexture(atlas);
            let new_texture = unsafe { (*self_raw).api.init_atlas_texture(atlas)? };
            // `mem::replace` + explicit `deinit` of the old value (manual GL delete —
            // the OpenGL leaf has no `Drop`, unlike Metal's ARC release).
            let old = unsafe { mem::replace(&mut *texture, new_texture) };
            old.deinit();
        }

        // try texture.replaceRegion(0, 0, atlas.size, atlas.size, atlas.data);
        unsafe {
            (*texture).replace_region(
                0,
                0,
                atlas.size() as usize,
                atlas.size() as usize,
                atlas.data(),
            )?;
        }
        Ok(())
    }

    // PORTS: generic.zig:1832-1846 `uploadBackgroundImage`; macos_impl
    // generic.rs:5561-5581.
    //
    // GL: `Image::deinit`/`Image::upload` touch real GL objects
    // (`glDeleteTextures`/`glTexImage2D`), so this must run with the owned context
    // current — it does: the sole caller is `draw_frame_raw`.
    fn upload_background_image(&mut self) -> Result<(), Error> {
        // Make sure our bg image is uploaded if it needs to be.
        // if (self.bg_image) |*bg| {
        if let Some(bg) = self.bg_image.as_mut() {
            // if (bg.isUnloading()) { bg.deinit(self.alloc); self.bg_image = null; return; }
            if bg.is_unloading() {
                // `Image::deinit` consumes `self` by value (the reference
                // `bg.deinit(self.alloc)`), so take the image OUT of the Option to
                // move it into `deinit`; then the Option is left `None`.
                let img = self.bg_image.take().unwrap();
                img.deinit(self.alloc);
                return Ok(());
            }
            // if (bg.isPending()) try bg.upload(self.alloc, &self.api);
            //   (`bg` borrows the `self.bg_image` field; `self.api` is a DISJOINT
            //    field, so the shared borrow is fine — same shape as the Metal twin.)
            if bg.is_pending() {
                bg.upload(self.alloc, &self.api)
                    .map_err(|_| Error::GlFailed)?;
            }
        }
        Ok(())
    }

    /// Callback from the graphics API when a frame is completed.
    //
    // PORTS: generic.zig:1259-1276 `frameCompleted`; macos_impl generic.rs:5351-5372.
    // `&self` narrowing (faithful): every op is `&self`-safe — `health.store`
    // (`AtomicHealth`), `surface_mailbox.push` (`&self` — the sink is behind an
    // `Arc`), and `swap_chain.release_frame`
    // (posts the semaphore).
    /// Queue a `.forever` surface-mailbox push until the draw mutex is
    /// released (UD362).
    pub fn defer_forever_push(&self, msg: SurfaceMessage) {
        self.deferred_surface_pushes.borrow_mut().push(msg);
    }

    /// Take the queued `.forever` pushes (under the draw mutex) together with a
    /// handle to the mailbox, so the caller can send them after the unlock
    /// without touching the renderer (UD362, N437).
    pub fn take_deferred_surface_pushes(&self) -> DeferredSurfacePushes {
        DeferredSurfacePushes {
            mailbox: self.surface_mailbox.clone(),
            msgs: core::mem::take(&mut *self.deferred_surface_pushes.borrow_mut()),
        }
    }

    pub fn frame_completed(&self, health: crate::Health) {
        // If our health value hasn't changed, then we do nothing.
        // if (self.health.load(.seq_cst) != health) {
        if self.health.load() != health {
            // self.health.store(health, .seq_cst);
            self.health.store(health);

            // Notify the surface so it can react (REAL — seam §B.2 resolved: core's
            // sink re-wraps this into `apprt::surface::Message::RendererHealth`; a
            // DETACHED offscreen mailbox still returns 0).
            // _ = self.surface_mailbox.push(.{ .renderer_health = health }, .{ .forever = {} });
            //
            // PORT NOTE: reference defect (R13), UD362. On OpenGL this runs
            // inside `drawFrameLocked` (the frame completes synchronously), so
            // the `.forever` push is deferred until the draw mutex is released.
            self.defer_forever_push(SurfaceMessage::RendererHealth(health));
        }

        // Always release our semaphore. The swap chain is
        // guaranteed to exist here: it is only torn down after
        // waiting for all in-flight frames to complete, and this
        // callback is what signals that completion.
        // self.swap_chain.?.releaseFrame();
        self.swap_chain.as_ref().unwrap().release_frame();
    }

    /// Draw the frame to the screen.
    ///
    /// If `sync` is true, this will synchronously block until the frame is finished
    /// drawing and has been presented. (For OpenGL `Frame::complete` ALWAYS blocks via
    /// `gl.finish()` regardless of `sync` — `opengl/Frame.zig:50-57`.)
    //
    // PORTS: generic.zig:1278-1466 `drawFrame`; macos_impl generic.rs:5408-6022.
    //
    // NO SCOPE CUTS REMAIN in this body (the L4 plan's Global-Constraints elisions are
    // all resolved). The encode path is the reference's, in the reference's order:
    // begin_frame → one render_pass { bg_image-or-bg_color, kitty_below_bg, cell_bg,
    // kitty_below_text, cell_text, kitty_above_text, overlay } → complete.
    //   * KITTY-IMAGE bands + the image upload sweep: w2 F-005 (`crate::image` is a
    //     twin pair now, so `images.upload`/`images.draw` run for real).
    //   * BACKGROUND IMAGE: w2 F-012 — `upload_background_image` runs before the pass
    //     and the `if (self.bg_image) |img| switch (img) { .ready => … } else { … }`
    //     arm replaces the unconditional `bg_color` step (generic.zig:1611-1628).
    //   * DEBUG OVERLAY: w2 F-020 — the guarded `images.draw(…, .overlay)` band
    //     (generic.zig:1681-1688) runs whenever `update_frame` built an overlay.
    //   * CUSTOM SHADERS (post-process): F-013 — when `has_custom_shaders` is set the
    //     base pass retargets to the custom-shader state's `back_texture` and the
    //     `shaders.post_pipelines` ping-pong chain runs afterwards, exactly as
    //     generic.zig:1691-1719.
    //
    // PORT NOTE (raw-ptr discipline, identical to the Metal twin / MIRIHAR-proven): the
    // whole body drives `self` through the SINGLE raw root `self_ptr` with NARROW field
    // reborrows, and re-derives the `frame` raw root FRESH via
    // `addr_of_mut!((*self_ptr).swap_chain.frames[idx])` after every whole-`&mut self`
    // call, because a whole `&mut *self_ptr` reborrow POPS a held `frame` tag.
    //
    // P7-W7 T2: `drawFrame` is split into its locked body, `drawFrameLocked`
    // (`72cf50855`, P7-W0 T1 on the Metal twin), so `syncDisplayLink` runs after
    // the mutex is released. Off macOS `syncDisplayLink` returns at once
    // (`DisplayLink == void`), so the split only changes where the lock ends.
    //
    // N437: `lock` is the caller's own `draw_mutex` handle (`crate::latest_frame`),
    // taken before the render thread started; the body forms references to the
    // renderer only while it is held.
    //
    /// # Safety
    /// `self_ptr` must be a valid, well-aligned, whole-`Renderer`-provenance pointer
    /// that outlives the call, and `lock` a handle to its `draw_mutex`.
    pub unsafe fn draw_frame_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        sync: bool,
    ) -> Result<(), Error> {
        // Everything that touches draw state happens under the draw
        // mutex. The display link is synced only after the mutex is
        // released; see `syncDisplayLink` for why it must never be
        // called with the draw mutex held.
        // const sync_display_link = locked: {
        //     self.draw_mutex.lockUncancelable(global.io());
        //     defer self.draw_mutex.unlock(global.io());
        //     break :locked try self.drawFrameLocked(sync);
        // };
        let (sync_display_link, deferred) = {
            lock.lock();
            struct UnlockGuard<'a>(&'a DrawMutex);
            impl Drop for UnlockGuard<'_> {
                fn drop(&mut self) {
                    self.0.unlock();
                }
            }
            let _unlock = UnlockGuard(lock);
            let result = unsafe { Self::draw_frame_locked(self_ptr, sync) };
            // UD362: take the frame completion's `.forever` pushes while the
            // lock is held; they are sent below, after the unlock.
            (result, unsafe {
                (*self_ptr).take_deferred_surface_pushes()
            })
        };

        // PORT NOTE: reference defect (R13), UD362. The `.redraw` and
        // `.renderer_health` pushes the reference makes inside the locked
        // frame completion go out here, with the draw mutex released, so a
        // full app mailbox cannot block the render thread while it holds the
        // mutex the apprt main thread (the mailbox's consumer) needs.
        deferred.push_all();
        let sync_display_link = sync_display_link?;

        // if (sync_display_link) self.syncDisplayLink(null, null);
        // (`if (comptime DisplayLink == void) return;` — nothing to sync off macOS.)
        let _ = sync_display_link;
        Ok(())
    }

    /// The body of `drawFrame`. Must be called with `draw_mutex` held.
    ///
    /// Returns true if the display link should be resynced once the
    /// draw mutex is released. This is only ever true on the no-redraw
    /// path, which a sync draw never takes, so the main thread's sync
    /// draws never touch the display link and `syncDisplayLink` stays
    /// on the render thread.
    //
    // generic.zig:1695-2003 `drawFrameLocked` (`72cf50855`, `2f0b65346`).
    //
    /// # Safety
    /// As [`draw_frame_raw`](Self::draw_frame_raw), and the caller holds
    /// `draw_mutex` for the whole call.
    // PORT NOTE: `&(&(*self_ptr).shaders.pipelines).x` spells out the reference the
    // pipelines `Undefined` derefs through (the rustc lint `dangerous_implicit_autorefs` rejects
    // the implicit one on a raw-pointer place); clippy calls that borrow needless.
    #[allow(clippy::needless_borrow)]
    unsafe fn draw_frame_locked(self_ptr: *mut Renderer, sync: bool) -> Result<bool, Error> {
        // Hand-rolled `defer`s as RAII guards (each holds the raw root, cleans up via a
        // NARROW field reborrow), dropping LIFO in the reference order:
        // draw_frame_end → scrollbar.
        struct ScrollbarGuard(*mut Renderer);
        impl Drop for ScrollbarGuard {
            fn drop(&mut self) {
                // defer if (self.scrollbar_dirty) { if (self.surface_mailbox.push(...,
                //     .instant) > 0) self.scrollbar_dirty = false; };
                // (REAL — seam §B.2 resolved: core's sink enqueues
                //  `apprt::surface::Message::Scrollbar`. A DETACHED mailbox — the
                //  offscreen golden/unit renderers — still returns 0, so the branch
                //  is never taken offscreen.)
                unsafe {
                    if (*self.0).scrollbar_dirty
                        && (*self.0).surface_mailbox.push(
                            SurfaceMessage::Scrollbar((*self.0).scrollbar),
                            MailboxTimeout::Instant,
                        ) > 0
                    {
                        (*self.0).scrollbar_dirty = false;
                    }
                }
            }
        }
        struct DrawEndGuard(*mut Renderer);
        impl Drop for DrawEndGuard {
            fn drop(&mut self) {
                // defer self.api.drawFrameEnd();
                unsafe { (*self.0).api.draw_frame_end() };
            }
        }
        // errdefer-only guard: releases the swap-chain semaphore ONLY on an early error
        // return after `next_frame`. Disarmed on the success path (the frame's
        // `frameCompleted` releases instead — else a double-release).
        struct ReleaseGuard {
            r: *mut Renderer,
            armed: bool,
        }
        impl Drop for ReleaseGuard {
            fn drop(&mut self) {
                // errdefer swap_chain.releaseFrame();
                // (the chain exists whenever the guard is armed — armed only
                // after `next_frame` on the established-Some chain.)
                if self.armed {
                    unsafe { (*self.r).swap_chain.as_ref().unwrap().release_frame() };
                }
            }
        }

        let _scrollbar = ScrollbarGuard(self_ptr);

        // self.api.drawFrameStart(); defer self.api.drawFrameEnd();
        unsafe { (*self_ptr).api.draw_frame_start() };
        let _draw_end = DrawEndGuard(self_ptr);

        // const surface_size = try self.api.surfaceSize();
        let surface_size = unsafe { (*self_ptr).api.surface_size() }?;

        // If either dimension is zero, drawing is absurd — return.
        if surface_size.width == 0 || surface_size.height == 0 {
            return Ok(false);
        }

        // If we have no graphics context we can't draw. This is
        // only the case while unrealized (GTK); displayRealized
        // rebuilds the swap chain.
        // if (!self.display_realized) return false;
        if !unsafe { (*self_ptr).display_realized } {
            return Ok(false);
        }

        // Get our swap chain, rebuilding it if it was released
        // while we were hidden. Rebuilding is deferred to draw
        // time because resource creation must happen somewhere
        // our graphics API allows it (OpenGL requires a current
        // context, which drawFrame guarantees).
        //
        // (raw-ptr discipline: instead of the reference's held `*SwapChain`
        // local, the chain is re-derived FRESH from `self_ptr` at every use
        // below — the same rule as `frame`; the macos twin carries the verbatim
        // Zig quote.)
        let swap_chain_rebuilt: bool = if unsafe { (*self_ptr).swap_chain.is_some() } {
            false
        } else {
            let sc = SwapChain::init(unsafe { &(*self_ptr).api }, unsafe {
                (*self_ptr).has_custom_shaders
            })?;
            unsafe { (*self_ptr).swap_chain = Some(sc) };
            true
        };

        let size_changed = unsafe {
            (*self_ptr).size.screen.width != surface_size.width
                || (*self_ptr).size.screen.height != surface_size.height
        };

        // Conditions under which we need to draw the frame.
        //
        // While any animation is in progress (a pending animation wake)
        // every draw must actually render.
        let needs_redraw = size_changed
            || swap_chain_rebuilt
            || unsafe { (*self_ptr).cells_rebuilt }
            || unsafe { (*self_ptr).animation_wake().is_some() }
            || sync;

        if !needs_redraw {
            // Ask our caller to resync the display link once the draw
            // mutex is released, because we can probably pause the
            // display link at this point.
            //
            // (`2f0b65346` deleted the `try self.api.presentLastTarget()` that
            // preceded this: the apprt keeps displaying the last exported frame.)
            return Ok(true);
        }
        // self.cells_rebuilt = false;
        unsafe { (*self_ptr).cells_rebuilt = false };

        // const frame = swap_chain.nextFrame(); errdefer swap_chain.releaseFrame();
        // (called for its side effects; the returned `&mut FrameState` is dropped
        // immediately and `frame` is re-derived fresh from `self_ptr` at each use.
        // The chain is Some here — established/rebuilt above; teardown is
        // excluded by the held draw_mutex.)
        let _ = unsafe { (*self_ptr).swap_chain.as_mut().unwrap().next_frame() };
        let mut release_guard = ReleaseGuard {
            r: self_ptr,
            armed: true,
        };
        let idx = unsafe { (*self_ptr).swap_chain.as_ref().unwrap().frame_index as usize };

        // If we need to reinitialize our shaders, do so.
        // if (self.reinitialize_shaders) { self.reinitialize_shaders = false;
        //     self.shaders.deinit(); try self.initShaders(); }
        if unsafe { (*self_ptr).reinitialize_shaders } {
            unsafe {
                (*self_ptr).reinitialize_shaders = false;
                // `drawFrame` deinits `self.shaders` DIRECTLY (generic.zig:1500), NOT via
                // `self.deinit_shaders()`.
                (*self_ptr).shaders.deinit();
                (*self_ptr).init_shaders()?; // whole-&mut self
            }
        }

        // Our shaders should not be defunct at this point.
        // assert(!self.shaders.defunct);
        assert!(!unsafe { (*self_ptr).shaders.defunct });

        // If we have custom shaders, make sure we have the
        // custom shader state in our frame state, otherwise
        // if we have a state but don't need it we remove it.
        // PORTS: generic.zig:1305-1324; macos_impl generic.rs:5588-5621.
        {
            let frame: *mut FrameState = unsafe {
                // Narrow projection through the `Option` (a fresh,
                // immediately-discarded reborrow of ONLY the `swap_chain`
                // field; the chain is Some — established above, teardown
                // excluded by the held draw_mutex).
                addr_of_mut!((*self_ptr).swap_chain.as_mut().unwrap().frames[idx])
            };
            // if (self.has_custom_shaders) {
            if unsafe { (*self_ptr).has_custom_shaders } {
                // if (frame.custom_shader_state == null) {
                //     frame.custom_shader_state = try .init(self.api);
                //     try frame.custom_shader_state.?.resize(self.api, w, h);
                // }
                if unsafe { (*frame).custom_shader_state.is_none() } {
                    unsafe {
                        (*frame).custom_shader_state =
                            Some(CustomShaderState::init(&(*self_ptr).api)?);
                        (*frame).custom_shader_state.as_mut().unwrap().resize(
                            &(*self_ptr).api,
                            surface_size.width as usize,
                            surface_size.height as usize,
                        )?;
                    }
                }
            }
            // } else if (frame.custom_shader_state) |*state| {
            //     state.deinit();
            //     frame.custom_shader_state = null;
            // }
            //
            // DIVERGENCE from `macos_impl` (which relies on field `Drop`): the OpenGL
            // leaf resources have a manual `deinit(self)` and no `Drop`, so the state
            // is `take`n and deinited EXPLICITLY (a bare `= None` would leak the GL
            // objects), matching `FrameState::deinit`'s model.
            else if let Some(state) = unsafe { (*frame).custom_shader_state.take() } {
                state.deinit();
            }
        }

        // If our stored size doesn't match the surface size we need to update it.
        // (Golden: size_changed=false ⇒ skipped. `update_screen_size_uniforms` is a
        //  whole-&mut self call, so no `frame` raw root is held across it.)
        if size_changed {
            unsafe {
                (*self_ptr).size.screen = ScreenSize {
                    width: surface_size.width,
                    height: surface_size.height,
                };
                (*self_ptr).update_screen_size_uniforms();
            }
        }

        // If this frame's target isn't the correct size / config, replace it.
        // if (frame.target.width != self.size.screen.width or … or
        //     frame.target_config_modified != self.target_config_modified) { … }
        {
            let frame: *mut FrameState = unsafe {
                // Narrow projection through the `Option` (a fresh,
                // immediately-discarded reborrow of ONLY the `swap_chain`
                // field; the chain is Some — established above, teardown
                // excluded by the held draw_mutex).
                addr_of_mut!((*self_ptr).swap_chain.as_mut().unwrap().frames[idx])
            };
            let need_resize = unsafe {
                (*frame).target.width != (*self_ptr).size.screen.width as usize
                    || (*frame).target.height != (*self_ptr).size.screen.height as usize
                    || (*frame).target_config_modified != (*self_ptr).target_config_modified
            };
            if need_resize {
                unsafe {
                    (*frame).resize(
                        &(*self_ptr).api,
                        (*self_ptr).size.screen.width as usize,
                        (*self_ptr).size.screen.height as usize,
                    )?;
                    (*frame).target_config_modified = (*self_ptr).target_config_modified;
                }
            }
        }

        // Upload images to the GPU as necessary.
        // _ = self.images.upload(self.alloc, &self.api);   (generic.zig:1548-1549)
        //   (narrow: &mut self.images + &self.api — disjoint fields.)
        //   macos_impl generic.rs:5899-5906.
        //
        // GL: this is where pending Kitty images become real `glTexImage2D` uploads
        // AND where unloaded ones are evicted (`glDeleteTextures`). Both need the
        // context current — it is, we are inside `draw_frame_raw`.
        let _ = unsafe {
            (*self_ptr)
                .images
                .upload((*self_ptr).alloc, &(*self_ptr).api)
        };

        // Make sure our background image is uploaded if it needs to be.
        // try self.uploadBackgroundImage();   (generic.zig:1551-1552)
        //   (whole-`&mut self` — macos_impl generic.rs:5911.)
        //
        // GL: this is where a pending bg image becomes a real `glTexImage2D` upload
        // and where an unloaded one is freed (`glDeleteTextures`); both need the
        // context current — it is, we are inside `draw_frame_raw`.
        unsafe { (*self_ptr).upload_background_image()? };

        // Update per-frame custom shader uniforms.
        // try self.updateCustomShaderUniformsForFrame();   (whole-&mut self)
        unsafe { (*self_ptr).update_custom_shader_uniforms_for_frame() };

        // Re-derive the frame raw root FRESH: the two whole-`&mut self` calls above
        // (`upload_background_image`, `update_custom_shader_uniforms_for_frame`)
        // popped any earlier frame tag.
        // (No whole-`&mut self` reborrows occur from here to `complete()`, so this
        // root survives to the end.)
        let frame: *mut FrameState = unsafe {
            // Narrow projection through the `Option` (a fresh,
            // immediately-discarded reborrow of ONLY the `swap_chain`
            // field; the chain is Some — established above, teardown
            // excluded by the held draw_mutex).
            addr_of_mut!((*self_ptr).swap_chain.as_mut().unwrap().frames[idx])
        };

        // Setup our frame data (the serialization boundary: ergonomic `crate::shaders`
        // structs → their OpenGL-ABI POD images via `opengl::gpu_abi`'s `From`).
        // try frame.uniforms.sync(&.{self.uniforms});
        unsafe {
            (*frame)
                .uniforms
                .sync(&[UniformsGpu::from(&(*self_ptr).uniforms)])?;
        }
        // try frame.cells_bg.sync(self.cells.bg_cells);  (CellBg is already ABI = [u8;4])
        unsafe {
            (*frame).cells_bg.sync(&(*self_ptr).cells.bg_cells)?;
        }
        // const fg_count = try frame.cells.syncFromArrayLists(self.cells.fg_rows);
        //
        // PORT NOTE (accepted deviation, same as the Metal twin): the reference's
        // `syncFromArrayLists` is a zero-copy memcpy of the `extern struct`s. Rust's
        // ergonomic `CellText` is NOT the packed-`u8` ABI (byte 29), so we FUSE the row
        // concatenation (order-preserving) with the `CellText → CellTextGpu` ABI
        // serialization into one transient `Vec`, then `sync` it (still reuses
        // `frame.cells`).
        let flat: Vec<CellTextGpu> = unsafe {
            (*self_ptr)
                .cells
                .fg_rows
                .iter()
                .flat_map(|row| row.iter().map(CellTextGpu::from))
                .collect()
        };
        unsafe { (*frame).cells.sync(&flat)? };
        let fg_count = flat.len();

        // If our background image buffer has changed, sync it. (Golden: modified stays 0
        // ⇒ skipped — the `bg_image_buffer` BUFFER is kept for parity even though the
        // bg-image DRAW is cut.)
        // if (frame.bg_image_buffer_modified != self.bg_image_buffer_modified) { … }
        if unsafe { (*frame).bg_image_buffer_modified != (*self_ptr).bg_image_buffer_modified } {
            unsafe {
                (*frame)
                    .bg_image_buffer
                    .sync(&[BgImageGpu::from(&(*self_ptr).bg_image_buffer)])?;
                (*frame).bg_image_buffer_modified = (*self_ptr).bg_image_buffer_modified;
            }
        }

        // If our font atlas changed, sync the texture data. Load-bearing: without it the
        // `cell_text` shader samples an empty texture → no glyphs. The `*mut SharedGrid`
        // is a SEPARATE allocation; the shared read lock is the reference
        // `lock.lockShared()`.
        {
            let grid = unsafe { &*(*self_ptr).font_grid };
            let modified = grid.atlas_grayscale().modified();
            if modified > unsafe { (*frame).grayscale_modified } {
                let _guard = grid.lock.read().unwrap();
                unsafe {
                    (*frame).grayscale_modified = grid.atlas_grayscale().modified();
                    Renderer::sync_atlas_texture(
                        self_ptr,
                        grid.atlas_grayscale(),
                        addr_of_mut!((*frame).grayscale),
                    )?;
                }
            }
        }
        {
            let grid = unsafe { &*(*self_ptr).font_grid };
            let modified = grid.atlas_color().modified();
            if modified > unsafe { (*frame).color_modified } {
                let _guard = grid.lock.read().unwrap();
                unsafe {
                    (*frame).color_modified = grid.atlas_color().modified();
                    Renderer::sync_atlas_texture(
                        self_ptr,
                        grid.atlas_color(),
                        addr_of_mut!((*frame).color),
                    )?;
                }
            }
        }

        // Get a frame context from the graphics API.
        // var frame_ctx = try self.api.beginFrame(self, &frame.target);
        // defer frame_ctx.complete(sync);
        //   — `self` is the type-erased `*mut ()` renderer root; the target is the raw
        //     `addr_of_mut!((*frame).target)`; the completion fn is the trampoline.
        //     (OpenGL `beginFrame` is infallible — `opengl/Frame.zig` has an empty error
        //     set — so no `?` here, unlike the Metal twin.)
        let frame_ctx: Frame = unsafe {
            (*self_ptr).api.begin_frame(
                self_ptr as *mut (),
                addr_of_mut!((*frame).target),
                opengl_frame_completed,
            )
        };

        {
            // var pass = frame_ctx.renderPass(&.{.{
            //     .target = if (frame.custom_shader_state) |state|
            //         .{ .texture = state.back_texture } else .{ .target = frame.target },
            //     .clear_color = .{ 0.0, 0.0, 0.0, 0.0 },
            // }});
            // defer pass.complete();
            //
            // PORT NOTE: OpenGL `RenderPass::step` takes `&mut self` (it bumps
            // `step_number`), unlike Metal's `&self`, so `pass` is `mut`. The
            // `attachments` slice is bound to a `let` so it outlives `pass` (the
            // `RenderPass` borrows it — E0716).
            let attach_target = match unsafe { (*frame).custom_shader_state.as_ref() } {
                Some(state) => AttachmentTarget::Texture(&state.back_texture),
                None => AttachmentTarget::Target(unsafe { &(*frame).target }),
            };
            let attachments = [Attachment {
                target: attach_target,
                clear_color: Some([0.0, 0.0, 0.0, 0.0]),
            }];
            let mut pass = frame_ctx.render_pass(&attachments);

            // First we draw our background image, if we have one.
            // The bg image shader also draws the main bg color.
            //
            // Otherwise, if we don't have a background image, we
            // draw the background color by itself in its own step.
            //
            // NOTE: We don't use the clear_color for this because that
            //       would require us to do color space conversion on the
            //       CPU-side. In the future when we have utilities for
            //       that we should remove this step and use clear_color.
            //
            // PORTS: generic.zig:1611-1628; macos_impl generic.rs:6069-6100.
            // if (self.bg_image) |img| switch (img) {
            //     .ready => |texture| pass.step(.{ .pipeline = bg_image, .uniforms,
            //         .buffers = &.{frame.bg_image_buffer.buffer},
            //         .textures = &.{texture}, .draw = {triangle, 3} }),
            //     else => {},
            // } else { pass.step(.{ .pipeline = bg_color, .uniforms, .buffers =
            //     &.{ null, frame.cells_bg.buffer }, .draw = {triangle, 3} }); }
            match unsafe { (*self_ptr).bg_image.as_ref() } {
                Some(image::Image::Ready(texture)) => pass.step(Step {
                    pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).bg_image },
                    uniforms: Some(unsafe { (*frame).uniforms.buffer }),
                    buffers: &[Some(unsafe { (*frame).bg_image_buffer.buffer })],
                    textures: &[Some(texture)],
                    samplers: &[],
                    draw: Draw {
                        kind: Primitive::Triangle,
                        vertex_count: 3,
                        instance_count: 1,
                    },
                }),
                Some(_) => {}
                None => {
                    pass.step(Step {
                        pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).bg_color },
                        uniforms: Some(unsafe { (*frame).uniforms.buffer }),
                        buffers: &[None, Some(unsafe { (*frame).cells_bg.buffer })],
                        textures: &[],
                        samplers: &[],
                        draw: Draw {
                            kind: Primitive::Triangle,
                            vertex_count: 3,
                            instance_count: 1,
                        },
                    });
                }
            }

            // Then we draw any kitty images that need
            // to be behind text AND cell backgrounds.
            // self.images.draw(&self.api, self.shaders.pipelines.image, &pass, .kitty_below_bg);
            //
            // PORTS: generic.zig:1628-1635; macos_impl generic.rs:6105-6113.
            // GL: `pass` is `&mut` (step bumps `step_number`) — see the module docs.
            unsafe {
                (*self_ptr).images.draw(
                    &(*self_ptr).api,
                    &(&(*self_ptr).shaders.pipelines).image,
                    &mut pass,
                    DrawPlacements::KittyBelowBg,
                );
            }

            // Opaque cell backgrounds.
            // pass.step(.{ .pipeline = cell_bg, .uniforms, .buffers = &.{ null,
            //     frame.cells_bg.buffer }, .draw = {triangle, 3} });
            pass.step(Step {
                pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).cell_bg },
                uniforms: Some(unsafe { (*frame).uniforms.buffer }),
                buffers: &[None, Some(unsafe { (*frame).cells_bg.buffer })],
                textures: &[],
                samplers: &[],
                draw: Draw {
                    kind: Primitive::Triangle,
                    vertex_count: 3,
                    instance_count: 1,
                },
            });

            // Kitty images between cell backgrounds and text.
            // self.images.draw(..., .kitty_below_text);
            //
            // PORTS: generic.zig:1645-1651; macos_impl generic.rs:6131-6139.
            unsafe {
                (*self_ptr).images.draw(
                    &(*self_ptr).api,
                    &(&(*self_ptr).shaders.pipelines).image,
                    &mut pass,
                    DrawPlacements::KittyBelowText,
                );
            }

            // Text.
            // pass.step(.{ .pipeline = cell_text, .uniforms, .buffers = &.{
            //     frame.cells.buffer, frame.cells_bg.buffer }, .textures = &.{
            //     frame.grayscale, frame.color }, .draw = {triangle_strip, 4, fg_count} });
            pass.step(Step {
                pipeline: unsafe { &(&(*self_ptr).shaders.pipelines).cell_text },
                uniforms: Some(unsafe { (*frame).uniforms.buffer }),
                buffers: &[
                    Some(unsafe { (*frame).cells.buffer }),
                    Some(unsafe { (*frame).cells_bg.buffer }),
                ],
                textures: &[
                    Some(unsafe { &(*frame).grayscale }),
                    Some(unsafe { &(*frame).color }),
                ],
                samplers: &[],
                draw: Draw {
                    kind: Primitive::TriangleStrip,
                    vertex_count: 4,
                    instance_count: fg_count,
                },
            });

            // Kitty images in front of text.
            // self.images.draw(..., .kitty_above_text);
            //
            // PORTS: generic.zig:1671-1677; macos_impl generic.rs:6166-6174.
            unsafe {
                (*self_ptr).images.draw(
                    &(*self_ptr).api,
                    &(&(*self_ptr).shaders.pipelines).image,
                    &mut pass,
                    DrawPlacements::KittyAboveText,
                );
            }

            // Debug overlay. We do this before any custom shader state
            // because our debug overlay is aligned with the grid.
            // if (self.overlay != null) self.images.draw(..., .overlay);
            //
            // PORTS: generic.zig:1681-1688; macos_impl generic.rs:6176-6188.
            if unsafe { (*self_ptr).overlay.is_some() } {
                unsafe {
                    (*self_ptr).images.draw(
                        &(*self_ptr).api,
                        &(&(*self_ptr).shaders.pipelines).image,
                        &mut pass,
                        DrawPlacements::Overlay,
                    );
                }
            }

            // defer pass.complete();
            pass.complete();
        }

        // If we have custom shaders, then we render them.
        // if (frame.custom_shader_state) |*state| { … }
        // PORTS: generic.zig:1691-1719; macos_impl generic.rs:5963-6023.
        if unsafe { (*frame).custom_shader_state.is_some() } {
            let state: *mut CustomShaderState =
                unsafe { (*frame).custom_shader_state.as_mut().unwrap() };
            // Sync our uniforms.
            // try state.uniforms.sync(&.{self.custom_shader_uniforms});
            //   (CustomShaderState.uniforms stays `Buffer<shadertoy::Uniforms>` — no
            //    ABI serializer; the ergonomic shadertoy uniforms sync directly.)
            unsafe {
                (*state)
                    .uniforms
                    .sync(&[(*self_ptr).custom_shader_uniforms])?;
            }

            // for (self.shaders.post_pipelines, 0..) |pipeline, i| {
            //   (the slice ref is hoisted so `[i]` does not autoref through the raw
            //    `self_ptr` — a narrow shared reborrow of `self.shaders.post_pipelines`,
            //    disjoint from the `state`/`frame` the loop touches.)
            let post_pipelines: &[crate::opengl::pipeline::Pipeline] =
                unsafe { &(*self_ptr).shaders.post_pipelines };
            let n = post_pipelines.len();
            for (i, pipeline) in post_pipelines.iter().enumerate() {
                // var pass = frame_ctx.renderPass(&.{.{
                //     .target = if (i < post_pipelines.len - 1)
                //         .{ .texture = state.front_texture } else .{ .target = frame.target },
                //     .clear_color = .{ 0.0, 0.0, 0.0, 0.0 },
                // }});
                // defer pass.complete();
                let target = if i < n - 1 {
                    AttachmentTarget::Texture(unsafe { &(*state).front_texture })
                } else {
                    AttachmentTarget::Target(unsafe { &(*frame).target })
                };
                let attachments = [Attachment {
                    target,
                    clear_color: Some([0.0, 0.0, 0.0, 0.0]),
                }];
                let mut pass = frame_ctx.render_pass(&attachments);

                // pass.step(.{ .pipeline = pipeline, .uniforms = state.uniforms.buffer,
                //     .textures = &.{state.back_texture}, .samplers = &.{state.sampler},
                //     .draw = .{ .type = .triangle, .vertex_count = 3 } });
                pass.step(Step {
                    pipeline,
                    uniforms: Some(unsafe { (*state).uniforms.buffer }),
                    buffers: &[],
                    textures: &[Some(unsafe { &(*state).back_texture })],
                    samplers: &[Some(unsafe { &(*state).sampler })],
                    draw: Draw {
                        kind: Primitive::Triangle,
                        vertex_count: 3,
                        instance_count: 1,
                    },
                });
                // defer pass.complete();
                pass.complete();
                // defer state.swap();
                unsafe { (*state).swap() };
            }
        }

        // Disarm the errdefer: from here we always reach `frame_ctx.complete`, whose
        // trampoline calls `frame_completed` → `release_frame`. Leaving it armed would
        // double-release.
        release_guard.armed = false;

        // defer frame_ctx.complete(sync); — the reference's LAST-registered defer; for
        // OpenGL it `gl.finish()`es (always blocking) then invokes `opengl_frame_completed`
        // directly. `frame` is NOT used after this.
        frame_ctx.complete(sync);

        Ok(false)
        // The two RAII guards drop here in the reference LIFO order:
        // draw_frame_end → scrollbar; the caller's unlock follows.
    }

    /// `&mut self` convenience wrapper over [`draw_frame_raw`], locking through
    /// the renderer's own handle. Sound for the exclusive single-threaded
    /// offscreen callers (the golden unit tests). Mirrors `macos_impl::draw_frame`.
    pub fn draw_frame(&mut self, sync: bool) -> Result<(), Error> {
        let mutex = self.draw_mutex.clone();
        let self_ptr: *mut Renderer = self;
        unsafe { Self::draw_frame_raw(self_ptr, &mutex, sync) }
    }
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

// ===========================================================================
// N437: the cross-thread entries.
//
// Every renderer call the render thread makes (`renderer_seam::Thread`), and
// every one the apprt's main thread makes while the render thread runs
// (`displayRealized`/`displayUnrealized`), goes through one of these. Each takes
// the caller's own `draw_mutex` handle and forms a reference to the renderer only
// while holding it, so a whole-renderer `&mut` never overlaps another thread's
// access (Stacked and Tree Borrows; `.port/phase-2/mirihar-n437/`). Where the
// reference runs an entry unlocked (`setFocus`, `setVisible`'s field write,
// `animationWake`, `hasVsync`, `loopEnter`, `setMacOSDisplayID`, the search-match
// writes), it only ever races the main thread's draw on macOS; the port takes the
// lock rather than keep a data race, and runs the display link's CoreVideo calls
// after the unlock exactly as `drawFrame` does (UD352, `72cf50855`).
// ===========================================================================
impl Renderer {
    /// Runs `f` on the renderer with `lock` held.
    ///
    /// # Safety
    /// `self_ptr` is the live renderer and `lock` a handle to its `draw_mutex`;
    /// no other thread may form a reference to the renderer, or touch its bytes,
    /// without holding that mutex.
    pub unsafe fn with_locked<T>(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        f: impl FnOnce(&mut Renderer) -> T,
    ) -> T {
        lock.lock();
        struct UnlockGuard<'a>(&'a DrawMutex);
        impl Drop for UnlockGuard<'_> {
            fn drop(&mut self) {
                self.0.unlock();
            }
        }
        let _unlock = UnlockGuard(lock);
        // SAFETY: the caller's contract; the mutex excludes every other access.
        f(unsafe { &mut *self_ptr })
    }

    /// `threadEnter` under the lock.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn thread_enter_raw<S>(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        rt_surface: *mut S,
    ) -> Result<(), Error> {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.thread_enter(rt_surface)) }
    }

    /// `threadExit`: its `draw_mutex` block, then the api's `threadExit`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn thread_exit_raw(self_ptr: *mut Renderer, lock: &DrawMutex) {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.thread_exit_locked()) };
        unsafe { Self::with_locked(self_ptr, lock, |r| r.thread_exit_api()) };
    }

    /// `loopEnter`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn loop_enter_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        draw_now: rustty_xev::Async,
    ) -> Result<(), Error> {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.loop_enter(draw_now)) }
    }

    /// `loopExit`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn loop_exit_raw(self_ptr: *mut Renderer, lock: &DrawMutex) {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.loop_exit()) }
    }

    /// `setMacOSDisplayID`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn set_macos_display_id_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        id: u32,
        draw_now: rustty_xev::Async,
    ) -> Result<(), Error> {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.set_macos_display_id(id, draw_now)) }
    }

    /// `hasVsync`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn has_vsync_raw(self_ptr: *mut Renderer, lock: &DrawMutex) -> bool {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.has_vsync()) }
    }

    /// `setFocus`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn set_focus_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        focus: bool,
    ) -> Result<(), Error> {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.set_focus(focus)) }
    }

    /// `setVisible`: the locked body, then the display-link sync after the unlock.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn set_visible_raw(self_ptr: *mut Renderer, lock: &DrawMutex, visible: bool) {
        let sync = unsafe { Self::with_locked(self_ptr, lock, |r| r.set_visible_locked(visible)) };
        sync.apply();
    }

    /// `animationWake`.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn animation_wake_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
    ) -> Option<AnimationWake> {
        unsafe { Self::with_locked(self_ptr, lock, |r| r.animation_wake()) }
    }

    /// `updateFrame`, with the lock held across the whole update (see
    /// [`Self::update_frame_locked`]).
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn update_frame_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
        state: &state::State,
        cursor_blink_visible: bool,
    ) -> Result<(), VtOutOfMemory> {
        unsafe {
            Self::with_locked(self_ptr, lock, |r| {
                r.update_frame_locked(state, cursor_blink_visible)
            })
        }
    }

    /// `displayRealized`, from the apprt's main thread.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn display_realized_raw(
        self_ptr: *mut Renderer,
        lock: &DrawMutex,
    ) -> Result<(), Error> {
        // If our API has to do things on realize, let it. (`OpenGL` no longer
        // declares `displayRealized`; see `display_realized_locked`.)
        //
        // Lock the draw mutex so that we can safely update state.
        unsafe { Self::with_locked(self_ptr, lock, |r| r.display_realized_locked()) }
    }

    /// `displayUnrealized`, from the apprt's main thread.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn display_unrealized_raw(self_ptr: *mut Renderer, lock: &DrawMutex) {
        // Lock the draw mutex so that we can safely update state.
        unsafe { Self::with_locked(self_ptr, lock, |r| r.display_unrealized_locked()) }
    }

    /// `renderCallback`'s unrealized arm (`Thread.zig:556-565` at `12752b2ac`):
    /// if the display is now unrealized, release the GPU resources on the render
    /// thread and report `true` so the callback disarms.
    //
    //     if (!t.renderer.display_realized) {
    //         t.renderer.draw_mutex.lockUncancelable(global.io());
    //         defer t.renderer.draw_mutex.unlock(global.io());
    //
    //         t.renderer.releaseGpuResources();
    //         return .disarm;
    //     }
    //
    // PORT NOTE (N437): the reference reads `display_realized` before it locks,
    // racing the main thread's `displayRealized`/`displayUnrealized` write; the
    // port reads it under the lock it takes for the release anyway.
    ///
    /// # Safety
    /// As [`Self::with_locked`].
    pub unsafe fn release_if_unrealized_raw(self_ptr: *mut Renderer, lock: &DrawMutex) -> bool {
        unsafe {
            Self::with_locked(self_ptr, lock, |r| {
                if r.display_realized {
                    return false;
                }
                r.release_gpu_resources();
                true
            })
        }
    }
}

// FFI-heavy (raw GL calls behind the leaves + the `*mut SharedGrid` handle) —
// Miri-excluded like the Metal lane (add to MIRI_EXCEPTIONS.md under `linux_impl`).
#[cfg(test)]
mod tests {
    use allocator_api2::alloc::Global;

    use super::{Error, Renderer};
    use crate::generic::{
        Color, DerivedConfig, Options, RepeatablePath, SurfaceMailbox, TerminalColor,
    };
    use crate::link;
    // The bg-image buffer mapping asserted by `update_bg_image_buffer_maps_config`.
    use crate::opengl::gl::test_support;
    use crate::opengl::OpenGL;
    use crate::shaders::{BgImageInfoFit, BgImageInfoPosition};
    use crate::size::{
        AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, CellSize,
        FontShapingBreak, Padding, ScreenSize, Size, WindowColorspace,
    };
    use crate::state;
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
    use rustty_zigstd::thread::Mutex;

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
            custom_shader_animation: crate::thread::CustomShaderAnimation::True,
        }
    }

    // Build a heap embedded-font `SharedGrid` + the pixel `Size` for a cols×rows
    // grid (the `macos_impl::tests::test_font_grid_and_size` recipe, generic.rs:6283).
    // The grid is boxed so the returned `*mut SharedGrid` stays valid for the
    // renderer that borrows it (the renderer never owns its grid; in the app the
    // `SharedGridSet` does). The test frees it after the renderer's `deinit`,
    // with [`free_test_font_grid`] or [`deinit_test_renderer`]: it used to be
    // leaked, which G-VALGRIND reported as 38 definite leaks (NEW-FINDINGS N411).
    // `size.screen` is PINNED to the grid's pixel size.
    fn test_font_grid_and_size(cols: CellCountInt, rows: CellCountInt) -> (*mut SharedGrid, Size) {
        let font_grid: *mut SharedGrid = {
            let lib = Library::init().unwrap();
            let mut c = Collection::init();
            c.load_options = Some(LoadOptions::new(lib));
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
            Box::into_raw(Box::new(grid))
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

    // Free a grid built by `test_font_grid_and_size`: the grid's faces
    // (`SharedGrid::deinit`), the box, then the FreeType library the collection
    // was loaded with, in the order `shared_grid`'s own tests use. Call it only
    // after every renderer that borrowed the grid has been deinited.
    fn free_test_font_grid(font_grid: *mut SharedGrid) {
        // SAFETY: `font_grid` came from `Box::into_raw` in
        // `test_font_grid_and_size`, and its only borrower is gone.
        let mut grid = unsafe { Box::from_raw(font_grid) };
        let mut lib = grid
            .resolver
            .collection
            .load_options
            .as_ref()
            .expect("test_font_grid_and_size sets the load options")
            .library;
        grid.deinit();
        drop(grid);
        lib.deinit();
    }

    // `r.deinit()` for a renderer built on `test_font_grid_and_size`'s grid
    // (every `golden_renderer`), then the grid itself.
    fn deinit_test_renderer(r: &mut Renderer) {
        let font_grid = r.font_grid;
        // The render thread's `threadExit` frees the GPU objects while its context
        // is current (`2f0b65346`); the tests are that thread.
        r.thread_exit();
        r.deinit();
        free_test_font_grid(font_grid);
    }

    /// What the render thread does before its first draw (`threadEnter`, then
    /// the first `.resize` message, whose `setScreenSize` sets the GL viewport).
    fn enter_test_renderer(r: &mut Renderer) {
        r.thread_enter(core::ptr::null_mut::<()>())
            .expect("thread_enter");
        let size = r.size;
        r.set_screen_size(size);
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
    /// lifetime (see its docs), `_guard` declared FIRST so it drops LAST (after
    /// `deinit` destroys the context), matching Task 8's field-order contract.
    #[test]
    fn linux_opengl_renderer_init_then_deinit_round_trips() {
        let _guard = test_support::gl_lock();

        let (font_grid, size) = test_font_grid_and_size(10, 3);
        let opts = Options {
            config: derived_config_for_test(),
            font_grid,
            size,
            // Offscreen: no surface, hence no mailbox (push returns 0).
            surface_mailbox: SurfaceMailbox::DETACHED,
        };

        let mut r = match Renderer::init(Global, opts) {
            Ok(r) => r,
            // A GL/context failure here is the host being unable to stand up a
            // 4.3-core context (absent EGL / no surfaceless platform / the virgl
            // driver's `EGL_BAD_MATCH` / a version under the floor). All are a
            // CLEAN SKIP, mirroring Task 8's `init_offscreen` skip contract — never
            // a panic, never a fabricated renderer.
            Err(e) => {
                // W8-03 strict mode: under RUSTTY_REQUIRE_GL=1 the clean skip
                // becomes a hard failure (see test_support docs).
                test_support::require_gl_or_skip(
                    "linux_opengl_renderer_init_then_deinit_round_trips",
                    &e,
                );
                eprintln!(
                    "no headless OpenGL 4.3 core context ({e}); skipping \
                     linux_opengl_renderer_init_then_deinit_round_trips"
                );
                return;
            }
        };

        // `init` touches no GL (`2f0b65346`): the swap chain and the shaders
        // are built lazily by the first draw on the render thread.
        assert!(r.swap_chain.is_none());
        assert!(r.shaders.defunct);
        assert!(r.reinitialize_shaders);
        enter_test_renderer(&mut r);

        // Fields the ctor + the `update_*` tail set (non-vacuous: proves
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
        deinit_test_renderer(&mut r);
    }

    /// The unrealize/realize cycle after `2f0b65346`: `displayUnrealized` and
    /// `displayRealized` only flip flags (on the main thread, under the draw
    /// mutex); the render thread's `renderCallback` releases the swap chain and
    /// the shaders while unrealized (`Thread.zig:556-565`), no draw builds them
    /// while unrealized (`drawFrameLocked`'s `if (!self.display_realized) return
    /// false`), and the first draw after `displayRealized` rebuilds both
    /// (`reinitialize_shaders = true`, `target_config_modified = 1`).
    ///
    /// The test thread plays the render thread (`threadEnter`), calling the
    /// `renderer_seam` entries with a separate `draw_mutex` handle as the render
    /// and main threads do (N437).
    #[test]
    fn linux_opengl_display_unrealize_then_realize_releases_and_rebuilds_on_the_render_thread() {
        const TEST: &str =
            "linux_opengl_display_unrealize_then_realize_releases_and_rebuilds_on_the_render_thread";
        let _guard = test_support::gl_lock();

        let Some(mut r) = golden_renderer(10, 3) else {
            let _ = TEST;
            return;
        };
        let lock = r.draw_mutex.clone();
        let rp: *mut Renderer = &mut r;

        // The first draw builds the GPU resources.
        r.draw_frame(false).expect("first draw");
        assert!(r.swap_chain.is_some());
        assert!(!r.shaders.defunct);
        assert!(!r.reinitialize_shaders);

        // Unrealize (main thread): only the flag changes.
        unsafe { Renderer::display_unrealized_raw(rp, &lock) };
        assert!(!r.display_realized);
        assert!(
            r.swap_chain.is_some(),
            "unrealize leaves the release to the render thread"
        );

        // renderCallback (render thread): releases, and disarms.
        assert!(unsafe { Renderer::release_if_unrealized_raw(rp, &lock) });
        assert!(r.swap_chain.is_none());
        assert!(r.shaders.defunct, "unrealized: the shaders go too");

        // No draw while unrealized.
        r.draw_frame(true).expect("unrealized draw");
        assert!(r.swap_chain.is_none());

        // Realize (main thread): only flags.
        unsafe { Renderer::display_realized_raw(rp, &lock) }.expect("display_realized");
        assert!(r.display_realized);
        assert!(r.reinitialize_shaders);
        assert_eq!(r.target_config_modified, 1);
        assert!(r.swap_chain.is_none());
        assert!(!unsafe { Renderer::release_if_unrealized_raw(rp, &lock) });

        // The next draw rebuilds both.
        r.draw_frame(false).expect("draw after realize");
        assert!(r.swap_chain.is_some());
        assert!(!r.shaders.defunct);
        assert!(!r.reinitialize_shaders);

        // A second cycle proves the rebuilt resources are themselves re-tearable.
        unsafe { Renderer::display_unrealized_raw(rp, &lock) };
        assert!(unsafe { Renderer::release_if_unrealized_raw(rp, &lock) });
        assert!(r.swap_chain.is_none());

        deinit_test_renderer(&mut r);
    }

    /// `setVisible(false)` releases the swap chain on the render thread
    /// (`2f0b65346`: the pin's GTK arm skipped the release, because the GLArea
    /// owned the context), but not the shaders while the display is realized
    /// (`releaseGpuResources`).
    #[test]
    fn linux_opengl_set_visible_false_releases_the_swap_chain() {
        let _guard = test_support::gl_lock();
        let Some(mut r) = golden_renderer(10, 3) else {
            return;
        };
        let lock = r.draw_mutex.clone();
        let rp: *mut Renderer = &mut r;
        r.draw_frame(false).expect("first draw");
        assert!(r.swap_chain.is_some());

        unsafe { Renderer::set_visible_raw(rp, &lock, false) };
        assert!(!r.visible);
        assert!(r.swap_chain.is_none());
        assert!(!r.shaders.defunct, "realized: the shaders stay");

        unsafe { Renderer::set_visible_raw(rp, &lock, true) };
        r.draw_frame(false).expect("draw after visible");
        assert!(r.swap_chain.is_some());

        deinit_test_renderer(&mut r);
    }

    /// A completed healthy frame is exported, pushed into `latest_frame` and
    /// announced with `.redraw` (`opengl/Frame.zig:62-89`); the apprt takes it.
    /// With the presentation unhealthy the frame is CPU memory.
    #[test]
    fn linux_opengl_frame_completion_pushes_the_frame_and_redraws() {
        let _guard = test_support::gl_lock();
        let Some(mut r) = golden_renderer(10, 3) else {
            return;
        };
        use crate::generic::surface_mailbox_tests::RecordingSink;
        use crate::generic::{MailboxTimeout, SurfaceMessage};
        let sink = std::sync::Arc::new(RecordingSink::accepting());
        r.surface_mailbox = SurfaceMailbox::new(sink.clone());
        let slot = r.latest_frame_handle();

        r.set_presentation_health(crate::Health::Unhealthy);
        r.draw_frame(true).expect("draw");
        assert!(
            sink.pushed()
                .contains(&(SurfaceMessage::Redraw, MailboxTimeout::Forever)),
            "pushed: {:?}",
            sink.pushed()
        );
        match slot.take().expect("a frame was pushed") {
            crate::opengl::opengl_impl::ExportedFrame::Memory(m) => {
                assert_eq!(
                    (m.width, m.height),
                    (r.size.screen.width, r.size.screen.height)
                );
                assert_eq!(m.pixels.len(), (m.width * m.height * 4) as usize);
            }
            #[cfg(unix)]
            crate::opengl::opengl_impl::ExportedFrame::Dmabuf(_) => {
                panic!("an unhealthy presentation exported a DMA-BUF")
            }
        }
        assert!(r.take_frame().is_none(), "take empties the slot");

        deinit_test_renderer(&mut r);
    }

    /// UD362: a full app mailbox must not deadlock the render thread against the
    /// apprt main thread. The render thread completes a frame and pushes
    /// `.redraw` with `.forever` into a full mailbox; the main thread (the
    /// mailbox's only consumer) first calls `displayUnrealized`, which takes the
    /// draw mutex, and only then drains. With the reference's order (the push
    /// inside the locked frame completion) the two wait on each other; the test
    /// mailbox gives up after 3 s and reports it. With the push after the
    /// unlock, the main thread gets the mutex, drains, and the push lands.
    #[test]
    fn linux_opengl_full_mailbox_does_not_deadlock_display_unrealize() {
        use crate::generic::{MailboxTimeout, SurfaceMailboxSink, SurfaceMessage};
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Condvar, Mutex};
        use std::time::Duration;

        let _guard = test_support::gl_lock();
        let Some(mut r) = golden_renderer(10, 3) else {
            return;
        };

        struct FullMailbox {
            full: Mutex<bool>,
            cv: Condvar,
            blocked: AtomicBool,
            timed_out: AtomicBool,
            delivered: Mutex<Vec<SurfaceMessage>>,
        }
        impl SurfaceMailboxSink for FullMailbox {
            fn push(&self, msg: SurfaceMessage, timeout: MailboxTimeout) -> usize {
                let full = self.full.lock().unwrap();
                if *full {
                    if timeout == MailboxTimeout::Instant {
                        return 0;
                    }
                    // `.forever` on a full queue: wait for the consumer.
                    self.blocked.store(true, Ordering::SeqCst);
                    self.cv.notify_all();
                    let (full, res) = self
                        .cv
                        .wait_timeout_while(full, Duration::from_secs(3), |f| *f)
                        .unwrap();
                    drop(full);
                    if res.timed_out() {
                        self.timed_out.store(true, Ordering::SeqCst);
                        return 0;
                    }
                } else {
                    drop(full);
                }
                self.delivered.lock().unwrap().push(msg);
                1
            }
        }
        let mailbox = Arc::new(FullMailbox {
            full: Mutex::new(true),
            cv: Condvar::new(),
            blocked: AtomicBool::new(false),
            timed_out: AtomicBool::new(false),
            delivered: Mutex::new(Vec::new()),
        });
        r.surface_mailbox = SurfaceMailbox::new(mailbox.clone());
        r.set_presentation_health(crate::Health::Unhealthy);

        struct SendPtr(*mut Renderer);
        // SAFETY: the pointer is only used through the lock-first raw entries.
        unsafe impl Send for SendPtr {}
        let lock = r.draw_mutex.clone();
        let r_ptr: *mut Renderer = &mut r;

        // The apprt main thread.
        let main = std::thread::spawn({
            let mailbox = mailbox.clone();
            let lock = lock.clone();
            let p = SendPtr(r_ptr);
            move || {
                let p = p;
                let mut full = mailbox.full.lock().unwrap();
                while !mailbox.blocked.load(Ordering::SeqCst) {
                    let (f, res) = mailbox
                        .cv
                        .wait_timeout(full, Duration::from_secs(10))
                        .unwrap();
                    full = f;
                    assert!(!res.timed_out(), "the render thread never pushed `.redraw`");
                }
                drop(full);
                // SAFETY: `p` is the live renderer; the entry locks first.
                unsafe { Renderer::display_unrealized_raw(p.0, &lock) };
                // Drain: the queue has room again.
                *mailbox.full.lock().unwrap() = false;
                mailbox.cv.notify_all();
            }
        });

        // The render thread (this test's thread, where the GL context is current).
        // SAFETY: `r_ptr` is the live renderer and `lock` its draw mutex.
        unsafe { Renderer::draw_frame_raw(r_ptr, &lock, true) }.expect("draw");
        main.join().expect("main thread");

        assert!(
            !mailbox.timed_out.load(Ordering::SeqCst),
            "the render thread blocked in a `.forever` push while holding the draw \
             mutex the main thread needed (UD362)"
        );
        assert!(
            mailbox
                .delivered
                .lock()
                .unwrap()
                .contains(&SurfaceMessage::Redraw),
            "`.redraw` was delivered once the main thread drained"
        );
        assert!(!r.display_realized, "the unrealize went through");

        deinit_test_renderer(&mut r);
    }

    // PORT NOTE: reuse the exact `opts(cols, rows)` recipe from `macos_impl::tests`
    // (generic.rs:6066) — renamed `term_opts` to avoid shadowing the `Options`
    // renderer struct that the other tests construct as a local `opts`.
    fn term_opts(cols: CellCountInt, rows: CellCountInt) -> TermOptions {
        TermOptions {
            cols,
            rows,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            colors: Colors::default(),
            default_modes: ModePacked::default(),
            default_cursor_style: rustty_vt::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
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
            // Offscreen: no surface, hence no mailbox (push returns 0).
            surface_mailbox: SurfaceMailbox::DETACHED,
        };
        let mut r = match Renderer::init(Global, opts) {
            Ok(r) => r,
            Err(e) => {
                // W8-03 strict mode (see test_support docs).
                test_support::require_gl_or_skip(
                    "linux_opengl_rebuild_cells_places_ab_and_cursor",
                    &e,
                );
                eprintln!(
                    "no headless OpenGL 4.3 core context ({e}); skipping \
                     linux_opengl_rebuild_cells_places_ab_and_cursor"
                );
                return;
            }
        };

        // small terminal: print "Ab" on row 0, leave a visible block cursor.
        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"Ab");
            s.deinit();
        }
        r.terminal_state.update(Global.into(), &mut t).unwrap();
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
        assert!(!r.cells.fg_rows[1].is_empty());
        let a = r.cells.fg_rows[1][0];
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

        deinit_test_renderer(&mut r);
        t.deinit(Global.into());
    }

    // GPU_RD_CHUNK4_DESIGN §3.2 / this file's Task-11 milestone: the GOLDEN harness =
    // the offscreen `Renderer(OpenGL)` with its offscreen size PINNED to the grid's
    // pixel size (`size.screen`), so `draw_frame`'s `surface_size == self.size.screen`
    // ⇒ `size_changed == false` (projection 1:1; cell (x,y) at pixel
    // [x*cw..(x+1)*cw) × [y*ch..(y+1)*ch)). Mirrors `macos_impl::tests::golden_renderer`
    // (generic.rs:6357) — probe with `OpenGL::init_offscreen` FIRST, THEN `Renderer::init`.
    //
    // GL-CONTEXT SKIP (Task-1 OPEN RISK): if the host cannot stand up a 4.3-core
    // context, `init_offscreen` fails ⇒ clean skip (returns None), mirroring the Metal
    // "no device ⇒ skip" precedent. The probe context is dropped BEFORE
    // `Renderer::init` stands up its own; the CALLER holds `test_support::gl_lock`,
    // so the two are SEQUENTIAL.
    fn golden_renderer(cols: CellCountInt, rows: CellCountInt) -> Option<Renderer> {
        if let Err(e) = OpenGL::init_offscreen(AlphaBlending::Native, 1, 1) {
            // W8-03 strict mode: this ONE arm covers every golden test — they
            // all probe through here (see test_support docs).
            test_support::require_gl_or_skip("golden_renderer probe", &e);
            eprintln!("no headless OpenGL 4.3 core context; skipping golden");
            return None;
        }

        let (font_grid, size) = test_font_grid_and_size(cols, rows);
        let opts = Options {
            config: derived_config_for_test(),
            font_grid,
            size,
            // Offscreen: no surface, hence no mailbox (push returns 0).
            surface_mailbox: SurfaceMailbox::DETACHED,
        };
        let mut r = Renderer::init(Global, opts).expect("Renderer::init (GL probed above)");
        enter_test_renderer(&mut r);
        Some(r)
    }

    // rustty-only (R13, UPSTREAM-DEFECTS.md UD57): the OpenGL twin of
    // `macos_impl`'s `update_custom_shader_uniforms_from_state_cell_relative_colors`.
    // Cell-relative `cursor-text` / `selection-*` values leave their uniforms
    // unchanged (the reference panics); a `.color` still writes.
    #[test]
    fn update_custom_shader_uniforms_from_state_cell_relative_colors() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };
        r.has_custom_shaders = true;

        let sentinel = [0.25, 0.5, 0.75, 1.0];
        r.custom_shader_uniforms.cursor_text = sentinel;
        r.custom_shader_uniforms.selection_background_color = sentinel;
        r.custom_shader_uniforms.selection_foreground_color = sentinel;
        r.custom_shader_uniforms.foreground_color = sentinel;
        // Sentinels for the two uniforms written AFTER the three colour
        // blocks (see the macOS twin): neither is a value the function can
        // produce, so they pin that it ran to its end.
        r.custom_shader_uniforms.cursor_visible = -7;
        r.custom_shader_uniforms.current_cursor_style = -7;
        r.config.cursor_text = Some(TerminalColor::CellForeground);
        r.config.selection_background = Some(TerminalColor::CellForeground);
        r.config.selection_foreground = Some(TerminalColor::CellBackground);

        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
        r.terminal_state.update(Global.into(), &mut t).unwrap(); // dirty = Full
        r.update_custom_shader_uniforms_from_state();

        // Positive reach: the update ran past its early returns.
        let fg = r.terminal_state.colors.foreground;
        assert_eq!(
            r.custom_shader_uniforms.foreground_color,
            [
                fg.r as f32 / 255.0,
                fg.g as f32 / 255.0,
                fg.b as f32 / 255.0,
                1.0
            ]
        );
        assert_eq!(r.custom_shader_uniforms.cursor_text, sentinel);
        assert_eq!(
            r.custom_shader_uniforms.selection_background_color,
            sentinel
        );
        assert_eq!(
            r.custom_shader_uniforms.selection_foreground_color,
            sentinel
        );

        // Positive reach PAST the three colour blocks (see the macOS twin).
        assert_eq!(
            r.custom_shader_uniforms.cursor_visible,
            r.terminal_state.cursor.visible as i32
        );
        assert_ne!(r.custom_shader_uniforms.current_cursor_style, -7);

        // A `.color` value is still written.
        let red = Color { r: 255, g: 0, b: 0 };
        r.config.cursor_text = Some(TerminalColor::Color(red));
        r.config.selection_background = Some(TerminalColor::Color(red));
        r.config.selection_foreground = Some(TerminalColor::Color(red));
        r.update_custom_shader_uniforms_from_state();
        assert_eq!(r.custom_shader_uniforms.cursor_text, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            r.custom_shader_uniforms.selection_background_color,
            [1.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            r.custom_shader_uniforms.selection_foreground_color,
            [1.0, 0.0, 0.0, 1.0]
        );

        t.deinit(Global.into());
        deinit_test_renderer(&mut r);
    }

    // UD76 (R13, task R13-C2): the OpenGL twin of `macos_impl`'s
    // `custom_shader_cursor_uniform_saturates_with_a_saturated_padding`. A
    // saturated padding (UD71) with the cursor off the first column and row
    // overflowed the reference's unchecked `grid_pos * cell + padding`.
    #[test]
    fn custom_shader_cursor_uniform_saturates_with_a_saturated_padding() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };
        r.has_custom_shaders = true;
        r.size.padding.left = u32::MAX;
        r.size.padding.top = u32::MAX;
        // `golden_renderer` leaves the cell contents unsized (the frame's
        // `rebuild_cells` sizes them); a cursor needs rows.
        r.cells.resize(rustty_render_types::GridSize {
            rows: 3,
            columns: 10,
        });
        let glyph = crate::shaders::CellText {
            atlas: Atlas::Grayscale,
            bools: crate::shaders::CellTextBools {
                is_cursor_glyph: true,
                ..Default::default()
            },
            grid_pos: [3, 2],
            color: [0, 0, 0, 255],
            ..Default::default()
        };
        r.cells.set_cursor(Some(glyph), Some(cursor::Style::Block));
        assert_eq!(Some(glyph), r.cells.get_cursor_glyph(), "precondition");

        r.update_custom_shader_uniforms_for_frame();

        // +Y is up in OpenGL: y is measured from the bottom of the screen.
        let c = r.custom_shader_uniforms.current_cursor.0;
        let h = r.size.screen.height as f32;
        assert_eq!(u32::MAX as f32, c[0], "x saturates: {c:?}");
        assert!(c[1] <= h - u32::MAX as f32, "y saturates: {c:?}");
        deinit_test_renderer(&mut r);
    }

    // F-014 PROVING TESTS (the OpenGL twin of
    // `macos_impl::tests::surface_mailbox_routing`): the surface-mailbox seam
    // (GPU_RD_CHUNK3A_DESIGN §B.2) is RESOLVED — `frameCompleted`'s
    // `.renderer_health` push and `drawFrame`'s `.scrollbar` push actually reach the
    // mailbox. Before the fix `SurfaceMailbox::push` was a hard-coded `0`, so both
    // sinks stayed EMPTY and `scrollbar_dirty` could never be cleared (the reported
    // "Linux GTK: renderer scrollbar updates are discarded by a no-op mailbox").
    mod surface_mailbox_routing {
        use std::sync::Arc;

        // `use super::*` picks up the enclosing `tests` module's helpers +
        // imports (`golden_renderer`, `term_opts`, `Renderer`, `Global`, `Mutex`,
        // `Terminal`, `state`, `test_support`); the seam types live one level
        // further up (`crate::generic`) and are named explicitly.
        use super::*;
        use crate::generic::surface_mailbox_tests::RecordingSink;
        use crate::generic::{MailboxTimeout, SurfaceMailbox, SurfaceMessage};
        use rustty_vt::Scrollbar;

        /// Drive one real frame: `update_frame` (refreshes the scrollbar cache from
        /// the terminal and raises `scrollbar_dirty`) then `draw_frame(true)` (whose
        /// `defer` runs the scrollbar push). Returns the pushed scrollbar value.
        fn update_and_draw(r: &mut Renderer) -> Scrollbar {
            let mut mutex = Mutex::new();
            let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
            {
                let mut s = t.vt_stream();
                s.next_slice(b"Ab");
                s.deinit();
            }
            let st = state::State {
                mutex: &mut mutex,
                terminal: &mut t,
                inspector: None,
                inspector_ptr: None,
                preedit: None,
                mouse: state::Mouse::default(),
                demand: Default::default(),
                handoff_gen: Default::default(),
                handoff_park: Default::default(),
            };
            r.update_frame(&st, true).unwrap();
            // Guard against a vacuous test: the fresh terminal's scrollbar differs
            // from the `Scrollbar::ZERO` the renderer was built with, so
            // `update_frame` raised the dirty flag.
            assert!(
                r.scrollbar_dirty,
                "update_frame must have cached a fresh scrollbar (dirty)"
            );
            let scrollbar = r.scrollbar;
            r.draw_frame(true).unwrap();
            // The frame is drawn; the terminal is this helper's to free (it was
            // leaked, NEW-FINDINGS N411).
            drop(st);
            t.deinit(Global.into());
            scrollbar
        }

        /// generic.zig:1725-1737 — `frameCompleted` pushes `.renderer_health` with
        /// `.forever` ONLY on a health TRANSITION.
        #[test]
        fn frame_completed_pushes_renderer_health_on_transition_only() {
            let _guard = test_support::gl_lock();

            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // GL-context skip
            };
            let sink = Arc::new(RecordingSink::accepting());
            r.surface_mailbox = SurfaceMailbox::new(sink.clone());
            // A frame only completes once the swap chain exists; the renderer
            // builds it lazily on the first draw (`2f0b65346`), so build it here.
            r.swap_chain =
                Some(crate::generic::SwapChain::init(&r.api, false).expect("swap chain"));

            // `health` starts `.healthy`, so this IS a transition → one push.
            r.frame_completed(crate::Health::Unhealthy);
            // Identical health → `if (self.health.load(...) != health)` is false.
            r.frame_completed(crate::Health::Unhealthy);
            // UD362: the `.forever` push waits for the draw mutex's release,
            // which `draw_frame_raw` does by sending the deferred pushes.
            assert!(sink.pushed().is_empty(), "nothing is sent under the lock");
            r.take_deferred_surface_pushes().push_all();

            assert_eq!(
                sink.pushed(),
                vec![(
                    SurfaceMessage::RendererHealth(crate::Health::Unhealthy),
                    MailboxTimeout::Forever
                )]
            );

            deinit_test_renderer(&mut r);
        }

        /// generic.zig:1449-1456 — the `drawFrame` `defer` pushes `.scrollbar` with
        /// `.instant` and clears `scrollbar_dirty` when the push lands. This is the
        /// exact hop the GTK scrollbar adjustment depends on.
        #[test]
        fn draw_frame_pushes_scrollbar_and_clears_dirty() {
            let _guard = test_support::gl_lock();

            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // GL-context skip
            };
            let sink = Arc::new(RecordingSink::accepting());
            r.surface_mailbox = SurfaceMailbox::new(sink.clone());

            let scrollbar = update_and_draw(&mut r);

            // The scrollbar `defer` runs at the end of `drawFrameLocked`, under
            // the lock; the completed frame's `.redraw` (`Frame.zig:83`) is sent
            // after the unlock (UD362), so it lands second.
            assert_eq!(
                sink.pushed(),
                vec![
                    (
                        SurfaceMessage::Scrollbar(scrollbar),
                        MailboxTimeout::Instant
                    ),
                    (SurfaceMessage::Redraw, MailboxTimeout::Forever),
                ]
            );
            assert!(
                !r.scrollbar_dirty,
                "a landed scrollbar push must clear scrollbar_dirty"
            );

            deinit_test_renderer(&mut r);
        }

        /// The other arm of the same `if (... > 0)`: "Fail instantly if the surface
        /// mailbox if full, we'll just get it on the next frame" — a refused push
        /// must KEEP `scrollbar_dirty` set.
        #[test]
        fn draw_frame_keeps_scrollbar_dirty_when_the_mailbox_is_full() {
            let _guard = test_support::gl_lock();

            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // GL-context skip
            };
            let sink = Arc::new(RecordingSink::refusing());
            r.surface_mailbox = SurfaceMailbox::new(sink.clone());

            let scrollbar = update_and_draw(&mut r);

            // (the frame's `.redraw` is refused too; `2f0b65346`. It is sent
            // after the unlock, UD362.)
            assert_eq!(
                sink.pushed(),
                vec![
                    (
                        SurfaceMessage::Scrollbar(scrollbar),
                        MailboxTimeout::Instant
                    ),
                    (SurfaceMessage::Redraw, MailboxTimeout::Forever),
                ]
            );
            assert!(
                r.scrollbar_dirty,
                "a refused scrollbar push must keep scrollbar_dirty set"
            );

            deinit_test_renderer(&mut r);
        }
    }

    // Sample the RGBA byte 4-tuple at pixel (x, y) in a tight `w*h*4` readback
    // (`Target::read_rgba` returns `[R,G,B,A]` per pixel, rows tight). Returns
    // (r,g,b,a). The OpenGL twin of `macos_impl::tests::px_at` (generic.rs:6453),
    // which samples BGRA — the byte order differs because the two backends read back
    // in their native order, but the STRUCTURAL assertions are the same.
    fn px_at(px: &[u8], w: usize, x: usize, y: usize) -> (u8, u8, u8, u8) {
        let i = (y * w + x) * 4;
        (px[i], px[i + 1], px[i + 2], px[i + 3])
    }

    /// L4 T11 MILESTONE GATE: render a deterministic terminal Screen OFFSCREEN into a
    /// GL FBO (via the monomorphic `Renderer(OpenGL)`'s `draw_frame`), read the pixels
    /// back with `Target::read_rgba`, and assert the STRUCTURE the port is responsible
    /// for (background color region, foreground glyph presence, block cursor at grid
    /// (2,0)). The OpenGL twin of `macos_impl::tests::golden_renders_cursor_fg_bg`
    /// (generic.rs:6520) — SAME input ("Ab" on 10×3), SAME structural facts.
    ///
    /// NOT A CROSS-API BYTE COMPARISON: this deliberately does NOT compare GL bytes
    /// against the Metal golden's bytes. They are different rendering pipelines
    /// (llvmpipe software rasterizer here vs. Metal there) and are NOT expected to
    /// produce identical pixels — only the same STRUCTURAL result. The thresholds are
    /// the Metal golden's own THRESHOLD PHILOSOPHY (structural, lenient enough to
    /// survive AA/rounding differences across GPU backends — generic.rs:6478-6490),
    /// applied fresh to the GL readback, never a literal byte-diff.
    ///
    /// RUNS FOR REAL only under `LIBGL_ALWAYS_SOFTWARE=1` (llvmpipe ⇒ GL 4.5 core on
    /// this host); CLEAN-SKIPS (never panics, never fabricates a golden) on the
    /// default virgl driver, which exposes no desktop core profile (Task-1 finding).
    /// Holds the process-wide `test_support::gl_lock` for the whole context lifetime
    /// (see its docs); `_guard` declared FIRST so it drops LAST (after `deinit`
    /// destroys the context).
    #[test]
    fn golden_renders_cursor_fg_bg() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        // "Ab" on a 10×3 grid ⇒ a block cursor at grid (2,0) (the empty cell after
        // "Ab"; focused + blink_visible ⇒ block). Verified by the T10 test.
        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"Ab");
            s.deinit();
        }
        let st = state::State {
            mutex: &mut mutex,
            terminal: &mut t,
            inspector: None,
            inspector_ptr: None,
            preedit: None,
            mouse: state::Mouse::default(),
            // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
            demand: Default::default(),
            handoff_gen: Default::default(),
            handoff_park: Default::default(),
        };
        // Drive the full pipeline through update_frame: fills cells + uniforms;
        // rebuild_cells sets dirty. cursor_blink_visible=true ⇒ visible block cursor.
        r.update_frame(&st, true).unwrap();
        // sync ⇒ the frame is drawn + `gl.finish()` lands the pixels + the completion
        // trampoline runs.
        r.draw_frame(true).unwrap();

        // Read back the FBO pixels of the frame just drawn (RGBA, rows tight).
        let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
        let px = r.swap_chain.as_ref().unwrap().frames[idx]
            .target
            .read_rgba();
        // llvmpipe's pixels, declared defined for memcheck (a no-op natively). Its
        // blending multiplies the destination by (1 - source alpha) = 0, and memcheck
        // cannot tell that product is defined, so it marks read-back pixels that hold
        // their expected values undefined (NEW-FINDINGS N412; the C program
        // `.port/tools/valgrind-llvmpipe-probe/probe.c` gets the same report with no
        // Rustty code). Only these bytes are exempted: every other value this test
        // branches on stays checked (review L-6 F2).
        rustty_zigstd::valgrind::memcheck::make_mem_defined(&px);

        let w = r.size.screen.width as usize;
        let h = r.size.screen.height as usize;
        let cw = r.grid_metrics.cell_width as usize;
        let ch = r.grid_metrics.cell_height as usize;
        assert_eq!(px.len(), w * h * 4);

        // GL Y-FLIP (the one real divergence from the Metal golden): the projection is a
        // top-left-origin `ortho2d` (generic.zig `updateScreenSizeUniforms`) so grid row 0
        // renders to the TOP of the FBO — but OpenGL's `glReadPixels` reads BOTTOM-UP
        // (origin bottom-left), so a top-down grid pixel row `y` lands at readback row
        // `h-1-y`. (Metal's IOSurface readback is already top-down, so its golden samples
        // `y` directly.) `sample(x, y)` samples the terminal-grid pixel (x, y).
        let sample = |x: usize, y: usize| px_at(&px, w, x, h - 1 - y);

        // (A) CURSOR CELL — solid red block at grid (2,0). Sample the cell interior
        //     center. Can only pass if `cell_text` ran with `is_cursor_glyph` set +
        //     `cursor_color` red reached the shader ⇒ proves the packed `bools` byte
        //     (bit 1) AND the uniform serialization.
        let (rr, g, b, _a) = sample(2 * cw + cw / 2, ch / 2);
        assert!(rr > 128, "cursor cell not bright-red (R={rr})");
        assert!(
            rr > g && rr > b,
            "cursor cell not red-dominant (R={rr} G={g} B={b})"
        );

        // (B) BACKGROUND — config bg is black (0,0,0), opacity 1.0 ⇒ bg_color =
        //     [0,0,0,255]. Sample an empty cell far from glyphs/cursor (grid (8,2),
        //     bottom row): cell_bg is transparent there (alpha 0) ⇒ the opaque
        //     full-screen bg_color layer shows through. The RGB check ALONE is VACUOUS
        //     (config bg RGB == clear RGB == 0), so we ALSO assert the ALPHA: the
        //     bg_color step writes an opaque layer (a=255) whereas the render-pass clear
        //     leaves a=0 ⇒ `ba > 128` can only pass if the full-screen bg_color step ran
        //     AND `bg_color.a = 255` serialized (offset 128 + 3).
        let (br, bg2, bb, ba) = sample(8 * cw + cw / 2, 2 * ch + ch / 2);
        assert!(
            br < 96 && bg2 < 96 && bb < 96,
            "background not dark (R={br} G={bg2} B={bb})"
        );
        assert!(
            ba > 128,
            "bg_color step absent / bg alpha not opaque (A={ba})"
        );

        // (C) FOREGROUND GLYPH present in row 0 — "A" at grid (0,0) is white. Scan the
        //     cell for a bright mark (antialiased glyph ⇒ max luminance over the cell).
        //     Proves the atlas sync + `cell_text` glyph sampling.
        let mut maxlum = 0u32;
        for yy in 0..ch {
            for xx in 0..cw {
                let (r3, g3, b3, _) = sample(xx, yy);
                maxlum = maxlum.max((r3 as u32 + g3 as u32 + b3 as u32) / 3);
            }
        }
        assert!(
            maxlum > 128,
            "no fg glyph rendered in cell (0,0) (maxlum={maxlum})"
        );

        deinit_test_renderer(&mut r);
        t.deinit(Global.into());
    }

    /// Build the `state::State` the two Kitty tests drive `update_frame` with.
    /// (Split out so the two tests cannot drift apart; the golden above keeps its
    /// own inline literal, untouched.)
    fn kitty_state(mutex: &mut Mutex, terminal: &mut Terminal) -> state::State {
        state::State {
            mutex,
            terminal,
            inspector: None,
            inspector_ptr: None,
            preedit: None,
            mouse: state::Mouse::default(),
            // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
            demand: Default::default(),
            handoff_gen: Default::default(),
            handoff_park: Default::default(),
        }
    }

    /// F-005 PROVING TEST — the OpenGL twin of `macos_impl::tests::
    /// kitty_image_pixel_path` (generic.rs:7498). Drives a real `Terminal` with a
    /// Kitty-graphics transmit+display APC, runs `update_frame` (which enters
    /// `image::State::kitty_update` through the `kitty_requires_update` guard) then
    /// `draw_frame` (which uploads the pending image to the GPU), and asserts the
    /// STATE the path produces.
    ///
    /// BEFORE THIS WAVE the whole path was absent on Linux: `linux_impl` had no
    /// `images` field, so a transmitted Kitty image was parsed and stored by the vt
    /// layer and then silently dropped by the renderer (F-005).
    ///
    /// The assertions (a)-(d) are ported VERBATIM from the macOS twin — which
    /// asserts STATE, not pixels — plus one GL-ONLY EXTRA: a pixel readback proving
    /// the `.kitty_below_text` band actually reached the framebuffer. There is no
    /// macOS-twin analogue for the extra; it follows the structural-threshold
    /// philosophy of `golden_renders_cursor_fg_bg` (a region/color assertion with
    /// tolerance, never a byte-diff against Metal).
    ///
    /// Live-GL: clean-skips via `golden_renderer`, hard-fails under
    /// `RUSTTY_REQUIRE_GL=1`. `_guard` first so it drops last (after `deinit`).
    #[test]
    fn kitty_image_pixel_path() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        // Transmit AND display (a=T) a 1x2 direct RGB image (f=24), id 1, all
        // pixels 0xFF (`////////` = 6 bytes of 0xFF). a=T places it at the
        // cursor → a pin placement, marking storage dirty.
        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
        // The vt `Terminal` starts with `width_px`/`height_px == 0` (the port
        // sets them only through the surface). Kitty `pixel_size` divides by the
        // cell size (`width_px / cols`), so pin the pixel dims to the renderer's
        // cell metrics — exactly what a real surface resize would do — else the
        // placement's dest size is 0 and it is (correctly) culled.
        t.width_px = r.grid_metrics.cell_width * 10;
        t.height_px = r.grid_metrics.cell_height * 3;
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b_Ga=T,f=24,i=1,s=1,v=2,c=1,r=1;////////\x1b\\");
            s.deinit();
        }

        // Sanity: the image + a placement now live in storage, dirty is set.
        let storage = unsafe { &(*t.screens.active).kitty_images };
        assert!(storage.image_by_id(1).is_some(), "image not in storage");
        assert!(!storage.placements.is_empty(), "no placement in storage");
        assert!(storage.dirty, "storage not marked dirty by the APC");

        // Drive the renderer. `terminal_state.update` snapshots the terminal;
        // `update_frame` → `kitty_requires_update` (dirty) == true → `kitty_update`.
        r.terminal_state.update(Global.into(), &mut t).unwrap();
        {
            let st = kitty_state(&mut mutex, &mut t);
            r.update_frame(&st, true).unwrap();
        }

        // (a) the terminal-walk + containment math ran → a placement was computed.
        assert!(
            !r.images.kitty_placements.is_empty(),
            "kitty_update computed no placements"
        );
        // (b) `prep_kitty_image` queued the image (pending) or it is already up.
        let entry = r
            .images
            .images
            .get(&crate::image::Id::Kitty(1))
            .expect("no Id::Kitty(1) image entry after kitty_update");
        assert!(
            entry.image.has_texture() || entry.image.is_pending(),
            "image neither pending nor uploaded after kitty_update"
        );
        // (c) `kitty_update`'s tail cleared `storage.dirty` (through the raw ptr).
        assert!(
            !unsafe { &(*t.screens.active).kitty_images }.dirty,
            "kitty_update did not clear storage.dirty"
        );

        // `draw_frame` uploads pending images → `convert` (rgb→rgba) +
        // `Texture::init` run → the image is Ready with a GL texture.
        r.draw_frame(true).unwrap();
        assert!(
            r.images
                .images
                .get(&crate::image::Id::Kitty(1))
                .expect("image entry vanished")
                .image
                .has_texture(),
            "image has no GPU texture after draw_frame (convert/upload did not run)"
        );

        // EXTRA (GL-only, no macOS-twin analogue): the image was not merely uploaded
        // — the `.kitty_below_text` band actually DREW it. The placement is a 1-cell
        // wide, 1-row tall, all-0xFF (white) image at the cursor, i.e. grid (0,0) on
        // an otherwise black background. Sample the cell center and require a bright
        // pixel. Structural threshold (>128 on all three channels), never a byte
        // comparison against Metal — llvmpipe and Metal are different rasterizers.
        // Y-FLIP as in `golden_renders_cursor_fg_bg`: `glReadPixels` is bottom-up.
        let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
        let px = r.swap_chain.as_ref().unwrap().frames[idx]
            .target
            .read_rgba();
        let w = r.size.screen.width as usize;
        let h = r.size.screen.height as usize;
        let cw = r.grid_metrics.cell_width as usize;
        let ch = r.grid_metrics.cell_height as usize;
        assert_eq!(px.len(), w * h * 4);
        let (ir, ig, ib, _ia) = px_at(&px, w, cw / 2, h - 1 - ch / 2);
        assert!(
            ir > 128 && ig > 128 && ib > 128,
            "the kitty image was uploaded but never drawn: cell (0,0) is \
             (R={ir} G={ig} B={ib}), expected the all-0xFF white image"
        );

        deinit_test_renderer(&mut r);
        t.deinit(Global.into());
    }

    /// F-005 PROVING TEST (eviction half) — GL-ONLY, no macOS-twin analogue.
    ///
    /// Covers the third leg of the oracle's image lifecycle: upload → draw →
    /// **evict**. After a drawn image exists, the Kitty delete-all APC (`a=d`) makes
    /// the vt layer drop the placement/image; the next `update_frame` re-walks the
    /// terminal and `kitty_update`'s prune marks the orphaned renderer-side image for
    /// unload, and the next `draw_frame`'s upload sweep removes it from the map and
    /// deletes its GL texture (`image.zig:73-78`, `State::upload`'s `isUnloading`
    /// arm). Without the twin's explicit `Texture::deinit` this would leak a GL
    /// texture per transmitted image for the life of the surface.
    ///
    /// Live-GL: clean-skips via `golden_renderer`, hard-fails under
    /// `RUSTTY_REQUIRE_GL=1`.
    #[test]
    fn kitty_eviction_removes_image() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
        t.width_px = r.grid_metrics.cell_width * 10;
        t.height_px = r.grid_metrics.cell_height * 3;
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b_Ga=T,f=24,i=1,s=1,v=2,c=1,r=1;////////\x1b\\");
            s.deinit();
        }

        // Frame 1: transmit + place + upload.
        r.terminal_state.update(Global.into(), &mut t).unwrap();
        {
            let st = kitty_state(&mut mutex, &mut t);
            r.update_frame(&st, true).unwrap();
        }
        r.draw_frame(true).unwrap();
        assert!(
            r.images
                .images
                .get(&crate::image::Id::Kitty(1))
                .is_some_and(|e| e.image.has_texture()),
            "precondition: the image must be uploaded before we evict it"
        );

        // Delete everything — `d=A` (capital) is "all placements AND the image
        // data" (`command::Delete::All(true)`); the lowercase `d=a` default would
        // drop only the placements and KEEP the image in storage, which is not an
        // eviction. Then re-drive the renderer.
        {
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b_Ga=d,d=A\x1b\\");
            s.deinit();
        }
        r.terminal_state.update(Global.into(), &mut t).unwrap();
        {
            let st = kitty_state(&mut mutex, &mut t);
            r.update_frame(&st, true).unwrap();
        }
        // The placement is gone from the renderer's own list right away.
        assert!(
            r.images.kitty_placements.is_empty(),
            "kitty_update kept a placement for a deleted image"
        );
        assert!(
            match r.images.images.get(&crate::image::Id::Kitty(1)) {
                None => true,
                Some(e) => e.image.is_unloading(),
            },
            "the orphaned image was not marked for unload"
        );

        // The upload sweep inside `draw_frame` performs the eviction (and the GL
        // texture delete).
        r.draw_frame(true).unwrap();
        assert!(
            r.images.images.is_empty(),
            "the unloaded image was not evicted from the renderer image map"
        );

        deinit_test_renderer(&mut r);
        t.deinit(Global.into());
    }

    // Shared driver for the custom-shader goldens (F-013): write the given shadertoy
    // body to a nanos-suffixed temp file, point the config at it, `init_shaders`
    // (load + transpile GLSL + build the post pipelines + flip `has_custom_shaders`),
    // then drive one "Ab" frame through `update_frame` + `draw_frame`. Returns the
    // renderer, the terminal, and the readback of the frame just drawn (RGBA, rows
    // tight, bottom-up per `glReadPixels`). Returns None on the GL-context skip.
    //
    // The OpenGL twin of `macos_impl::tests::golden_renders_custom_shader`
    // (generic.rs:7042); CALLER holds `test_support::gl_lock`.
    fn drive_custom_shader_frame(shader_body: &[u8]) -> Option<(Renderer, Terminal, Vec<u8>)> {
        use std::ffi::CString;

        let mut r = golden_renderer(10, 3)?;

        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rustty_gl_shader_{nanos}.glsl"));
        std::fs::write(&path, shader_body).unwrap();

        // Point the config at the on-disk shader (required arm), byte-faithful
        // path -> CString (mirrors the macOS custom-shader golden).
        let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();
        r.config.custom_shaders = RepeatablePath {
            value: vec![crate::generic::Path::Required(cpath)],
        };

        // Load + transpile + build the post pipelines + flip has_custom_shaders.
        r.init_shaders().expect("init_shaders");
        assert!(r.has_custom_shaders, "custom shaders not activated");
        assert!(
            !r.shaders.post_pipelines.is_empty(),
            "post pipelines not built"
        );

        // Drive one frame: "Ab" on 10×3 ⇒ block cursor at grid (2,0) (the T10/T11
        // fixture), visible glyphs + a red cursor cell.
        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"Ab");
            s.deinit();
        }
        let st = state::State {
            mutex: &mut mutex,
            terminal: &mut t,
            inspector: None,
            inspector_ptr: None,
            preedit: None,
            mouse: state::Mouse::default(),
            // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
            demand: Default::default(),
            handoff_gen: Default::default(),
            handoff_park: Default::default(),
        };
        r.update_frame(&st, true).unwrap();
        r.draw_frame(true).unwrap();

        // The draw_frame setup block lazily created the per-frame CustomShaderState
        // (the config had no shaders at `Renderer::init`, so `FrameState::init` did
        // not — this is the F-013 wiring, absent before the fix).
        let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
        assert!(
            r.swap_chain.as_ref().unwrap().frames[idx]
                .custom_shader_state
                .is_some(),
            "custom_shader_state not created by draw_frame"
        );

        let px = r.swap_chain.as_ref().unwrap().frames[idx]
            .target
            .read_rgba();
        let _ = std::fs::remove_file(&path);
        Some((r, t, px))
    }

    /// F-013 golden 1/2 (the macOS `golden_renders_custom_shader` twin): a
    /// PASS-THROUGH shadertoy post-process shader preserves the cells — the base
    /// pass retargets to `back_texture` and the post pass blits it to
    /// `frame.target`. Same robustness contract as the macOS twin (non-black AND
    /// non-uniform — an empty-pipeline black frame fails the first, a uniform fill
    /// fails the second; NOT an exact byte-diff, since the pass-through samples
    /// through a GL sampler and sub-LSB filtering may perturb bytes) PLUS the
    /// structural cursor fact: the red cursor cell survives the identity shader.
    ///
    /// RUNS FOR REAL only under `LIBGL_ALWAYS_SOFTWARE=1` (llvmpipe); CLEAN-SKIPS
    /// otherwise. Holds `test_support::gl_lock`; `_guard` first so it drops last.
    #[test]
    fn golden_renders_custom_shader_passthrough() {
        let _guard = test_support::gl_lock();

        let Some((mut r, mut t, px)) = drive_custom_shader_frame(
            b"void mainImage(out vec4 fragColor, in vec2 fragCoord){\n\
              fragColor = texture(iChannel0, fragCoord / iResolution.xy);\n}\n",
        ) else {
            return; // GL-context skip
        };
        // `px` is `drive_custom_shader_frame`'s `read_rgba()`, returned as is.
        // llvmpipe's pixels, declared defined for memcheck (a no-op natively). Its
        // blending multiplies the destination by (1 - source alpha) = 0, and memcheck
        // cannot tell that product is defined, so it marks read-back pixels that hold
        // their expected values undefined (NEW-FINDINGS N412; the C program
        // `.port/tools/valgrind-llvmpipe-probe/probe.c` gets the same report with no
        // Rustty code). Only these bytes are exempted: every other value this test
        // branches on stays checked (review L-6 F2).
        rustty_zigstd::valgrind::memcheck::make_mem_defined(&px);

        let w = r.size.screen.width as usize;
        let h = r.size.screen.height as usize;
        let cw = r.grid_metrics.cell_width as usize;
        let ch = r.grid_metrics.cell_height as usize;
        assert_eq!(px.len(), w * h * 4);
        let sample = |x: usize, y: usize| px_at(&px, w, x, h - 1 - y);

        // Non-black: some pixel has a bright channel (the white "Ab" glyphs /
        // red cursor made it through the pass-through).
        let non_black = px
            .chunks_exact(4)
            .any(|p| p[0] > 64 || p[1] > 64 || p[2] > 64);
        assert!(
            non_black,
            "cells not visible through the pass-through shader"
        );
        // Not uniform: at least one pixel differs from the first (glyph vs bg).
        let first = &px[0..4];
        let non_uniform = px.chunks_exact(4).any(|p| p != first);
        assert!(
            non_uniform,
            "frame is a uniform fill (post-process did not preserve the image)"
        );
        // Structural: the block cursor at grid (2,0) is still bright-red-dominant
        // (identity shader ⇒ the T11 golden's cursor fact holds).
        let (rr, g, b, _a) = sample(2 * cw + cw / 2, ch / 2);
        assert!(rr > 128, "cursor cell not bright-red (R={rr})");
        assert!(
            rr > g && rr > b,
            "cursor cell not red-dominant (R={rr} G={g} B={b})"
        );

        deinit_test_renderer(&mut r);
        t.deinit(Global.into());
    }

    /// F-013 golden 2/2, the BEHAVIORAL proof the post-process pass actually
    /// transforms pixels (rustty-added; ghostty has no oracle test here): a
    /// color-INVERTING shader turns the dark background bright and the red cursor
    /// cyan. Before the F-013 wiring the base pass rendered straight to
    /// `frame.target` and the post loop never ran, so the background stayed dark —
    /// this test fails without the fix and passes with it.
    ///
    /// RUNS FOR REAL only under `LIBGL_ALWAYS_SOFTWARE=1` (llvmpipe); CLEAN-SKIPS
    /// otherwise. Holds `test_support::gl_lock`; `_guard` first so it drops last.
    #[test]
    fn golden_custom_shader_inverts_colors() {
        let _guard = test_support::gl_lock();

        let Some((mut r, mut t, px)) = drive_custom_shader_frame(
            b"void mainImage(out vec4 fragColor, in vec2 fragCoord){\n\
              vec4 c = texture(iChannel0, fragCoord / iResolution.xy);\n\
              fragColor = vec4(1.0 - c.rgb, 1.0);\n}\n",
        ) else {
            return; // GL-context skip
        };

        let w = r.size.screen.width as usize;
        let h = r.size.screen.height as usize;
        let cw = r.grid_metrics.cell_width as usize;
        let ch = r.grid_metrics.cell_height as usize;
        assert_eq!(px.len(), w * h * 4);
        let sample = |x: usize, y: usize| px_at(&px, w, x, h - 1 - y);

        // The empty background cell (8,2) was near-black (T11 golden fact B);
        // inverted it must be BRIGHT on every channel. Without the post pass this
        // samples the un-transformed dark background and fails.
        let (br, bg2, bb, _ba) = sample(8 * cw + cw / 2, 2 * ch + ch / 2);
        assert!(
            br > 128 && bg2 > 128 && bb > 128,
            "background not inverted to bright (R={br} G={bg2} B={bb})"
        );

        // The red cursor cell (2,0) inverts to cyan: green/blue dominant over red.
        let (rr, g, b, _a) = sample(2 * cw + cw / 2, ch / 2);
        assert!(
            g > rr && b > rr,
            "cursor cell not inverted to cyan-dominant (R={rr} G={g} B={b})"
        );

        deinit_test_renderer(&mut r);
        t.deinit(Global.into());
    }

    // ===================================================================
    // F-012 — background image (w2 WI-4)
    // ===================================================================

    /// Encode a `w`×`h` solid-RGB PNG (8-bit) for the bg-image golden. The reference
    /// `background-image` fixtures are not checked in, so generate one at test time
    /// via the pure-Rust `png` encoder. (Test-helper duplication precedent — the
    /// Metal twin has the same helper at `generic.rs:6842`; `linux_impl`'s tests
    /// deliberately do not reach into `macos_impl`, which is not even compiled here.)
    fn encode_solid_png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().expect("png header");
            let mut data = Vec::with_capacity((w * h * 3) as usize);
            for _ in 0..(w * h) {
                data.extend_from_slice(&rgb);
            }
            writer.write_image_data(&data).expect("png data");
            writer.finish().expect("png finish");
        }
        out
    }

    /// F-012 PROVING TEST — the OpenGL twin of `macos_impl::tests::
    /// golden_renders_background_image` (generic.rs:7277). Point the config at a real
    /// PNG on disk, run `prep_background_image` (open + `FileType` sniff + wuffs
    /// decode → `bg_image = Some(Pending)`) and `update_bg_image_buffer`
    /// (opacity/fit → `BgImageGpu`), then `draw_frame` — whose
    /// `upload_background_image` turns the pending image into a `Ready(Texture)` and
    /// whose base pass takes the `bg_image` step INSTEAD of the `bg_color` step. Read
    /// the FBO back and assert the sampled pixel is the DECODED image color, distinct
    /// from the plain black `bg_color` the negative control shows.
    ///
    /// BEFORE THIS WAVE `background-image` was accepted by the config and its vertex
    /// buffer was synced, but nothing on Linux ever decoded, uploaded, or drew it
    /// (F-012) — this test could not even be written, because `bg_image` was
    /// permanently `None`.
    ///
    /// DETERMINISM PINS (mirroring the cursor golden): `AlphaBlending::Native`,
    /// `colorspace = Srgb`, and `fit = Stretch` so the solid image fills the whole
    /// screen ⇒ every pixel is the image color (trivial sampling).
    ///
    /// Live-GL: clean-skips via `golden_renderer`, hard-fails under
    /// `RUSTTY_REQUIRE_GL=1`. `_guard` first so it drops last (after `deinit`).
    #[test]
    fn golden_renders_background_image() {
        use std::ffi::CString;

        let _guard = test_support::gl_lock();

        // The distinct bg-image color (blue-dominant), unambiguously != black.
        const IMG_RGB: [u8; 3] = [0x30, 0x60, 0xC0];

        // Build a golden renderer, optionally with a bg image at `path`, draw one
        // frame, and return the center-pixel RGBA of the bottom row.
        fn draw_center(with_bg: Option<&std::path::Path>) -> Option<(u8, u8, u8, u8)> {
            let mut r = golden_renderer(10, 3)?;

            if let Some(p) = with_bg {
                // Point the config at the on-disk PNG (required arm) + stretch it.
                let bytes = p.as_os_str().as_encoded_bytes().to_vec();
                r.config.bg_image =
                    Some(crate::generic::Path::Required(CString::new(bytes).unwrap()));
                r.config.bg_image_fit = BackgroundImageFit::Stretch;
                r.config.bg_image_opacity = 1.0;
                // Decode + build the vertex/opacity buffer (the two calls
                // `change_config` would make on a bg-image change).
                r.prep_background_image().expect("prep_background_image");
                r.update_bg_image_buffer();
                // The decode must have produced a pending bg image.
                assert!(
                    matches!(r.bg_image, Some(crate::image::Image::Pending(_))),
                    "prep did not stage a pending bg image"
                );
            }

            // An (empty) terminal drives update_frame/draw_frame (as the cursor
            // golden does); the bg-image step does not depend on cell content.
            let mut mutex = Mutex::new();
            let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
            {
                let st = kitty_state(&mut mutex, &mut t);
                r.update_frame(&st, true).unwrap();
            }
            r.draw_frame(true).unwrap();

            if with_bg.is_some() {
                // After the frame, the pending image was uploaded to the GPU.
                assert!(
                    matches!(r.bg_image, Some(crate::image::Image::Ready(_))),
                    "bg image not uploaded to a Ready texture"
                );
            }

            let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
            let px = r.swap_chain.as_ref().unwrap().frames[idx]
                .target
                .read_rgba();
            let w = r.size.screen.width as usize;
            let h = r.size.screen.height as usize;
            let cw = r.grid_metrics.cell_width as usize;
            let ch = r.grid_metrics.cell_height as usize;
            // Sample the center of an empty cell on the bottom row (grid (5,2)).
            // Y-FLIP: `glReadPixels` is bottom-up (see `golden_renders_cursor_fg_bg`).
            let out = px_at(&px, w, 5 * cw + cw / 2, h - 1 - (2 * ch + ch / 2));
            deinit_test_renderer(&mut r);
            t.deinit(Global.into());
            Some(out)
        }

        // Write the solid bg-image PNG to a temp file.
        let png_bytes = encode_solid_png(2, 2, IMG_RGB);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rustty_gl_bg_{nanos}.png"));
        std::fs::write(&path, &png_bytes).unwrap();

        // (bg image) — sample should be the decoded image color.
        let bg = match draw_center(Some(&path)) {
            Some(v) => v,
            None => {
                let _ = std::fs::remove_file(&path);
                return; // GL-context skip
            }
        };
        // (negative control) — no bg image ⇒ the plain black bg_color.
        let none = draw_center(None).expect("GL probed above");
        let _ = std::fs::remove_file(&path);

        // `read_rgba` returns (R, G, B, A). The image is blue-dominant
        // (0x30,0x60,0xC0): B ≈ 0xC0 (>128), R ≈ 0x30 (<128), B > R, opaque.
        // Structural thresholds, never a byte-diff against the Metal golden.
        let (br, bg2, bb, ba) = bg;
        assert!(bb > 128, "bg-image blue channel too low (B={bb})");
        assert!(br < 128, "bg-image red channel too high (R={br})");
        assert!(bb > br, "bg-image not blue-dominant (B={bb} R={br})");
        assert!(ba > 128, "bg-image not opaque (A={ba})");

        // UD354 (DECISIONS M5): under `native` blending the image's sRGB bytes
        // must reach the target unchanged (within rounding), not encoded a second
        // time. The reference's always-sRGB render target turns #3060C0 into
        // #78A5E1, which the structural checks above still accept.
        for (got, want, channel) in [
            (br, IMG_RGB[0], 'R'),
            (bg2, IMG_RGB[1], 'G'),
            (bb, IMG_RGB[2], 'B'),
        ] {
            assert!(
                got.abs_diff(want) <= 2,
                "bg-image {channel}={got} is not the image's {want} (sRGB encoded twice?)"
            );
        }

        // The negative control is dark (black bg_color) — proves the bg-image step
        // (not the bg_color fallback) produced the color above.
        let (nr, ng, nb, _na) = none;
        assert!(
            nr < 96 && ng < 96 && nb < 96,
            "negative control not dark (R={nr} G={ng} B={nb})"
        );
        assert!(
            bb > nb + 64,
            "bg-image blue not distinct from bg_color (bg B={bb} vs none B={nb})"
        );
    }

    /// F-012 — the CPU half: config → bg-image vertex/uniform buffer mapping + the
    /// `modified` counter. The OpenGL twin of `macos_impl::tests::
    /// update_bg_image_buffer_maps_config` (generic.rs:7766). This code was already
    /// live on Linux before the wave, but it had NO Linux-side test; it is the
    /// fit/position/repeat/opacity coverage the GPU-side sampling (the shader's job)
    /// rests on.
    #[test]
    fn update_bg_image_buffer_maps_config() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };
        // DIVERGENCE from the Metal twin's assertion (`== 1`): the macOS test builds
        // its renderer through a test-only ctor, whereas `golden_renderer` goes
        // through the REAL `Renderer::init`, which already called
        // `update_bg_image_buffer` once (generic.zig:794). So assert the INCREMENT,
        // which is the actual invariant the per-frame sync gate reads.
        let before = r.bg_image_buffer_modified;
        r.config.bg_image_position = BackgroundImagePosition::CenterCenter;
        r.config.bg_image_fit = BackgroundImageFit::Cover;
        r.config.bg_image_repeat = true;
        r.update_bg_image_buffer();
        assert_eq!(r.bg_image_buffer.info.position, BgImageInfoPosition::Mc);
        assert_eq!(r.bg_image_buffer.info.fit, BgImageInfoFit::Cover);
        assert!(r.bg_image_buffer.info.repeat);
        assert_eq!(r.bg_image_buffer_modified, before + 1);

        deinit_test_renderer(&mut r);
    }

    /// F-012 — the "no path" arm of `prep_background_image` (oracle
    /// `generic.zig:1825-1829`): with `config.bg_image == None` an existing image is
    /// marked for unload, and the next `draw_frame`'s `upload_background_image`
    /// FREES it (`bg_image = None`). This is the config-cleared path, which the
    /// golden above does not exercise.
    #[test]
    fn clearing_bg_image_config_unloads_the_image() {
        use std::ffi::CString;

        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        let png_bytes = encode_solid_png(2, 2, [0x30, 0x60, 0xC0]);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rustty_gl_bg_clear_{nanos}.png"));
        std::fs::write(&path, &png_bytes).unwrap();

        r.config.bg_image = Some(crate::generic::Path::Required(
            CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap(),
        ));
        r.prep_background_image().expect("prep_background_image");
        assert!(
            matches!(r.bg_image, Some(crate::image::Image::Pending(_))),
            "prep did not stage a pending bg image"
        );

        // Now clear the path and re-prep: `markForUnload` on the pending image.
        r.config.bg_image = None;
        r.prep_background_image().expect("prep (cleared)");
        assert!(
            r.bg_image.as_ref().is_some_and(|i| i.is_unloading()),
            "clearing the config did not mark the bg image for unload"
        );

        // The frame's `upload_background_image` frees it.
        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
        {
            let st = kitty_state(&mut mutex, &mut t);
            r.update_frame(&st, true).unwrap();
        }
        r.draw_frame(true).unwrap();
        assert!(
            r.bg_image.is_none(),
            "upload_background_image did not free the unloading bg image"
        );

        let _ = std::fs::remove_file(&path);
        deinit_test_renderer(&mut r);
        t.deinit(Global.into());
    }

    /// F-012 — `change_config`'s `bg_image_changed` branch (oracle
    /// `generic.zig:1878-1897`): a config whose `bg_image` path DIFFERS from the
    /// current one must run `prep_background_image`, and an identical path must not
    /// re-decode. This is the runtime (`ChangeConfig` message) entry point, distinct
    /// from the `init` entry the golden drives.
    #[test]
    fn change_config_reprepares_the_bg_image() {
        use std::ffi::CString;

        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        let png_bytes = encode_solid_png(2, 2, [0x30, 0x60, 0xC0]);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rustty_gl_bg_cfg_{nanos}.png"));
        std::fs::write(&path, &png_bytes).unwrap();
        let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();

        // Precondition: the renderer was built with no bg image.
        assert!(r.bg_image.is_none());

        // None -> Some(path) IS a change ⇒ decode.
        let mut cfg = derived_config_for_test();
        cfg.bg_image = Some(crate::generic::Path::Required(cpath.clone()));
        r.change_config(cfg).expect("change_config");
        assert!(
            matches!(r.bg_image, Some(crate::image::Image::Pending(_))),
            "change_config did not decode the new bg-image path"
        );

        // Upload it so the state machine has a TEXTURE — that is what makes the
        // next two arms distinguishable (`mark_for_replace` on a textured image
        // yields `Replace{texture, pending}`; on an untextured one it is just
        // `Pending` again, which would make this test vacuous).
        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
        {
            let st = kitty_state(&mut mutex, &mut t);
            r.update_frame(&st, true).unwrap();
        }
        r.draw_frame(true).unwrap();
        assert!(
            matches!(r.bg_image, Some(crate::image::Image::Ready(_))),
            "the bg image did not upload"
        );

        // Some(path) -> Some(SAME path) is NOT a change ⇒ `prep_background_image`
        // must NOT run: the image stays plain `Ready`.
        let mut same = derived_config_for_test();
        same.bg_image = Some(crate::generic::Path::Required(cpath));
        r.change_config(same).expect("change_config (same path)");
        assert!(
            matches!(r.bg_image, Some(crate::image::Image::Ready(_))),
            "an unchanged bg-image path re-entered prep_background_image"
        );

        // Some(A) -> Some(B) IS a change ⇒ re-decode + `mark_for_replace`, which on a
        // textured image is the `Replace{texture, pending}` shape (pending AND
        // textured at once).
        let path2 = std::env::temp_dir().join(format!("rustty_gl_bg_cfg2_{nanos}.png"));
        std::fs::write(&path2, encode_solid_png(2, 2, [0xC0, 0x20, 0x20])).unwrap();
        let mut other = derived_config_for_test();
        other.bg_image = Some(crate::generic::Path::Required(
            CString::new(path2.as_os_str().as_encoded_bytes().to_vec()).unwrap(),
        ));
        r.change_config(other).expect("change_config (new path)");
        let bg = r.bg_image.as_ref().expect("bg image vanished");
        assert!(
            bg.is_pending() && bg.has_texture(),
            "a changed bg-image path did not re-enter prep_background_image"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&path2);
        deinit_test_renderer(&mut r);
        t.deinit(Global.into());
    }

    /// F-012 — the wuffs-error → `Error::ImageDecode` mapping the `?` in
    /// `prep_background_image` propagates. PURE (no GL, no renderer): runs on every
    /// host, including this macOS dev box's cross-compile lanes' Linux twin builds.
    #[test]
    fn wuffs_error_maps_to_image_decode() {
        assert_eq!(
            Error::from(rustty_wuffs::Error::OutOfMemory),
            Error::OutOfMemory
        );
        assert_eq!(
            Error::from(rustty_wuffs::Error::WuffsError),
            Error::ImageDecode
        );
        assert_eq!(
            Error::from(rustty_wuffs::Error::Overflow),
            Error::ImageDecode
        );
    }

    // ===================================================================
    // F-020 — inspector renderer overlay (w2 WI-5)
    // ===================================================================

    /// A stand-in for the inspector's `widgets::renderer::Info`, which is the real
    /// `InspectorOverlay` impl. The renderer may never name a `rustty-inspector`
    /// type (R4 acyclicity), so the seam is exercised through a test-local impl —
    /// exactly the shape core coerces the real `Info` into.
    struct FakeInspector(Vec<crate::overlay::Feature>);
    impl state::InspectorOverlay for FakeInspector {
        fn overlay_features(&self) -> Vec<crate::overlay::Feature> {
            self.0.clone()
        }
    }

    /// F-020 PROVING TEST — the inspector's live renderer-overlay controls now have
    /// an OpenGL consumer. With a `state.inspector` attached and a feature enabled,
    /// `update_frame` must (1) read the features through the `InspectorOverlay`
    /// seam, (2) `rebuild_overlay` a z2d surface, and (3) `images.overlay_update`
    /// stage it as an `Id::Overlay` placement; `draw_frame` must then upload it and
    /// take the `.overlay` draw band.
    ///
    /// BEFORE THIS WAVE all four stages were milestone-scope-cut on Linux: the
    /// inspector checkbox wrote a feature nobody read (F-020).
    ///
    /// The negative control (`inspector: None`) proves the assertions are not
    /// vacuous — `overlay` stays `None` and no overlay placement is staged.
    ///
    /// Live-GL: clean-skips via `golden_renderer`, hard-fails under
    /// `RUSTTY_REQUIRE_GL=1`.
    #[test]
    fn overlay_draws_on_inspector_features() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
        {
            // OSC 133;A marks row 0 as a shell PROMPT — the row state
            // `Overlay::highlight_semantic_prompts` looks for. It draws a
            // `min(5, cell_width)`-wide bar down the LEFT edge of the prompt rows,
            // which is what the pixel assertion below samples.
            let mut s = t.vt_stream();
            s.next_slice(b"\x1b]133;A\x1b\\Ab");
            s.deinit();
        }

        // (negative control) no inspector ⇒ no features ⇒ no overlay at all.
        {
            let st = kitty_state(&mut mutex, &mut t);
            r.update_frame(&st, true).unwrap();
        }
        assert!(
            r.overlay.is_none(),
            "an overlay was built with no inspector attached"
        );
        assert!(
            r.images.overlay_placements.is_empty(),
            "an overlay placement was staged with no inspector attached"
        );
        // Guard against a vacuous pixel comparison later: the prompt row must
        // actually be marked, else the overlay would legitimately draw nothing.
        assert_ne!(
            r.terminal_state.row_data[0].raw.semantic_prompt(),
            rustty_vt::page::SemanticPrompt::None,
            "OSC 133;A did not mark row 0 as a prompt"
        );
        // Baseline frame WITHOUT the overlay.
        r.draw_frame(true).unwrap();
        let before = {
            let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
            r.swap_chain.as_ref().unwrap().frames[idx]
                .target
                .read_rgba()
        };

        // (positive) attach an inspector reporting one enabled feature.
        let mut fake = FakeInspector(vec![crate::overlay::Feature::SemanticPrompts]);
        {
            let mut st = kitty_state(&mut mutex, &mut t);
            st.inspector = Some(&mut fake as *mut dyn state::InspectorOverlay);
            r.update_frame(&st, true).unwrap();
        }

        // (1)+(2) the features reached `rebuild_overlay`, which built a surface at
        // the terminal size.
        let overlay = r
            .overlay
            .as_ref()
            .expect("rebuild_overlay built no overlay");
        let term = r.size.terminal();
        assert_eq!(overlay.surface.get_width() as u32, term.width);
        assert_eq!(overlay.surface.get_height() as u32, term.height);

        // (3) `images.overlay_update` staged the `Id::Overlay` image + placement.
        assert_eq!(
            r.images.overlay_placements.len(),
            1,
            "overlay_update staged no overlay placement"
        );
        assert!(
            r.images.images.contains_key(&crate::image::Id::Overlay),
            "overlay_update staged no Id::Overlay image"
        );

        // `draw_frame`'s upload sweep turns it into a real GL texture, and the
        // `.overlay` band draws it (the band is guarded on `self.overlay.is_some()`,
        // which now holds).
        r.draw_frame(true).unwrap();
        assert!(
            r.images
                .images
                .get(&crate::image::Id::Overlay)
                .expect("overlay image entry vanished")
                .image
                .has_texture(),
            "the overlay image was never uploaded to a GL texture"
        );

        // (4) the `.overlay` DRAW band actually reached the framebuffer. Sample the
        // left-edge prompt bar (`highlight_semantic_prompts` draws it at pixel x
        // 0..min(5, cell_width) of the prompt rows) and require it to differ from the
        // overlay-off baseline. Structural (a difference, with a margin), never a
        // byte-diff against Metal — llvmpipe is a different rasterizer.
        // Y-FLIP as in `golden_renders_cursor_fg_bg`: `glReadPixels` is bottom-up.
        let after = {
            let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
            r.swap_chain.as_ref().unwrap().frames[idx]
                .target
                .read_rgba()
        };
        let w = r.size.screen.width as usize;
        let h = r.size.screen.height as usize;
        let ch = r.grid_metrics.cell_height as usize;
        assert_eq!(before.len(), after.len());
        let y = h - 1 - ch / 2; // vertical center of grid row 0
        let (b0, b1, b2, _) = px_at(&before, w, 2, y);
        let (a0, a1, a2, _) = px_at(&after, w, 2, y);
        let delta = (a0 as i32 - b0 as i32).abs()
            + (a1 as i32 - b1 as i32).abs()
            + (a2 as i32 - b2 as i32).abs();
        assert!(
            delta > 16,
            "the overlay was uploaded but never drawn: prompt-bar pixel is \
             ({a0},{a1},{a2}) with the overlay vs ({b0},{b1},{b2}) without \
             (delta={delta})"
        );

        deinit_test_renderer(&mut r);
        t.deinit(Global.into());
    }

    /// F-020 — the direct `rebuild_overlay` lifecycle, twin of `macos_impl::tests::
    /// rebuild_overlay_builds_and_clears` (generic.rs:7632): empty features build
    /// nothing, a real feature builds an overlay at the terminal size, and clearing
    /// tears it down again. Needs a `Renderer`, hence live-GL clean-skip.
    #[test]
    fn rebuild_overlay_builds_and_clears() {
        let _guard = test_support::gl_lock();

        let mut r = match golden_renderer(10, 3) {
            Some(r) => r,
            None => return, // GL-context skip
        };

        // empty features → overlay stays None
        r.rebuild_overlay(&[]).unwrap();
        assert!(r.overlay.is_none());

        // A real feature builds the overlay at the terminal size.
        r.rebuild_overlay(&[crate::overlay::Feature::SemanticPrompts])
            .unwrap();
        assert!(r.overlay.is_some());

        // A second call with the SAME size takes the `reset()` fast path (oracle
        // `'existing`), not a rebuild — the overlay survives.
        r.rebuild_overlay(&[crate::overlay::Feature::SemanticPrompts])
            .unwrap();
        assert!(r.overlay.is_some());

        // and clearing removes it
        r.rebuild_overlay(&[]).unwrap();
        assert!(r.overlay.is_none());

        deinit_test_renderer(&mut r);
    }

    // ===================================================================
    // task-14c-5 (finding N9) — the RESTORED diagnostics of this Linux twin
    // ===================================================================

    /// Proof that `generic.zig`'s `log.*` family is live in the OpenGL arm, not
    /// only in the Metal one. Four shapes that break INDEPENDENTLY:
    ///
    /// 1. a warn carrying only the (byte-lossy) path — `generic.zig:1995`;
    /// 2. a warn carrying an extra formatted argument — `generic.zig:2002`;
    /// 3. the SUCCESS path of the same function, which must emit NOTHING (this
    ///    project has shipped a diagnostic-on-the-success-path bug once already,
    ///    and three sites in this file were deliberately left unrestored for
    ///    exactly that reason — see the `LOG PORT NOTE`s);
    /// 4. a DEBUG-level site in a different function — `generic.zig:2137`.
    ///
    /// All four need a live `Renderer`, hence a GL context: they clean-skip through
    /// `golden_renderer` and HARD-FAIL under `RUSTTY_REQUIRE_GL=1`.
    mod restored_log_sites {
        use super::*;
        use crate::log_capture::CapturedRecord;
        use std::ffi::CString;

        /// Write `bytes` to a uniquely-named temp file with the given extension and
        /// return (path, the `CString` the config holds, the `{s}` rendering the
        /// oracle prints for it).
        fn temp_bg_file(
            tag: &str,
            ext: &str,
            bytes: &[u8],
        ) -> (std::path::PathBuf, CString, String) {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("rustty_gl_bg_{tag}_{nanos}.{ext}"));
            std::fs::write(&path, bytes).unwrap();
            let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();
            let display = String::from_utf8_lossy(cpath.to_bytes()).into_owned();
            (path, cpath, display)
        }

        /// Run `prep_background_image` over `cpath` under capture and return every
        /// record it emitted on this thread.
        fn prep_under_capture(r: &mut Renderer, cpath: CString) -> Vec<CapturedRecord> {
            r.config.bg_image = Some(crate::generic::Path::Required(cpath));
            let cap = crate::log_capture::start();
            let res = r.prep_background_image();
            let records = cap.messages();
            drop(cap);
            res.expect("prep_background_image must not error on a break-arm path");
            records
        }

        /// `generic.zig:1995` — neither the magic bytes nor the extension identify
        /// the file, so the load block breaks with the "Cannot determine" warn.
        #[test]
        fn undetectable_file_type_warns_once() {
            let _guard = test_support::gl_lock();
            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // GL-context skip
            };

            let (path, cpath, display) =
                temp_bg_file("undetectable", "xyz", b"not an image at all");
            let records = prep_under_capture(&mut r, cpath);
            let _ = std::fs::remove_file(&path);

            assert_eq!(
                records,
                vec![(
                    log::Level::Warn,
                    "generic_renderer".to_string(),
                    format!("Cannot determine file type for background image file \"{display}\"!"),
                )]
            );
            // The break arm leaves the renderer's bg image untouched.
            assert!(r.bg_image.is_none());
            deinit_test_renderer(&mut r);
        }

        /// `generic.zig:2002` — GIF magic bytes are DETECTED, but the port decodes
        /// only PNG and JPEG, so this is the `else => |f|` arm and its message
        /// carries the detected type as a second argument. A restore that dropped
        /// that argument passes test 1 above and fails this one.
        #[test]
        fn unsupported_file_type_warns_with_the_detected_type() {
            let _guard = test_support::gl_lock();
            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // GL-context skip
            };

            // GIF87a magic — `FileType::detect` returns `Gif`, which the decode
            // switch does not handle.
            let (path, cpath, display) =
                temp_bg_file("unsupported", "gif", b"GIF87a\x01\x00\x01\x00\x00\x00\x00");
            let records = prep_under_capture(&mut r, cpath);
            let _ = std::fs::remove_file(&path);

            assert_eq!(
                records,
                vec![(
                    log::Level::Warn,
                    "generic_renderer".to_string(),
                    format!("Unsupported file type Gif for background image file \"{display}\"!"),
                )]
            );
            assert!(r.bg_image.is_none());
            deinit_test_renderer(&mut r);
        }

        /// The success path of the very same function must be SILENT. This is the
        /// guard against the shape this project shipped once — a warning hoisted
        /// out of its failure arm, firing on every successful call.
        #[test]
        fn a_successful_decode_emits_nothing() {
            let _guard = test_support::gl_lock();
            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // GL-context skip
            };

            let png = encode_solid_png(2, 2, [0x30, 0x60, 0xC0]);
            let (path, cpath, _display) = temp_bg_file("ok", "png", &png);
            let records = prep_under_capture(&mut r, cpath);
            let _ = std::fs::remove_file(&path);

            assert_eq!(records, Vec::<CapturedRecord>::new());
            // and it really did the work, so the silence is not the silence of a
            // function that never ran.
            assert!(matches!(r.bg_image, Some(crate::image::Image::Pending(_))));
            deinit_test_renderer(&mut r);
        }

        /// `generic.zig:2137` — a DEBUG-level site in a different function. Level
        /// and scope are asserted, so a restore that used `log::warn!` or the wrong
        /// target fails here even though the message text is right.
        #[test]
        fn set_screen_size_emits_one_debug_record() {
            let _guard = test_support::gl_lock();
            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // GL-context skip
            };

            let mut size = r.size;
            size.padding = Padding {
                top: 1,
                bottom: 2,
                left: 3,
                right: 4,
            };

            let cap = crate::log_capture::start();
            r.set_screen_size(size);
            let records = cap.messages();
            drop(cap);

            assert_eq!(
                records,
                vec![(
                    log::Level::Debug,
                    "generic_renderer".to_string(),
                    format!("screen size size={size:?}"),
                )]
            );
            assert_eq!(r.size.padding, size.padding);
            deinit_test_renderer(&mut r);
        }
    }
}
