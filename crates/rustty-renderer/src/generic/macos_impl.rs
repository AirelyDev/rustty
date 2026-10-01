use core::mem;
use core::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use allocator_api2::alloc::Global;
// PORT NOTE: `Atlas` here is the FONT texture atlas (`rustty_font::atlas`),
// aliased `FontAtlas` so the plain name `Atlas` refers to the CellText vertex
// atlas enum (`crate::shaders::Atlas`) that the Chunk-3b cell fill assembles.
use rustty_font::atlas::{Atlas as FontAtlas, Format, OutOfMemory};
use rustty_font::glyph::{Constraint, ConstraintSize, RenderOptions};
use rustty_font::nerd_font_attributes::get_constraint;
use rustty_font::sprite::Sprite;
use rustty_font_shaper::shared_grid::RenderError;
use rustty_font_shaper::{
    collection, shape, Metrics, Presentation, Shaper, ShaperCache, SharedGrid, SPRITE_INDEX,
};
use rustty_render_types::WindowPaddingColor;
// PORT NOTE (Chunk 3c §B.2 COMPILE-BLOCKER): the bare name `OutOfMemory` in this
// module is `rustty_font::atlas::OutOfMemory` (imported above). `update_frame`'s
// sole propagated `?` is `terminal_state.update(...)`, whose error is the DISTINCT
// `rustty_vt::pagelist::OutOfMemory`; there is no `From` between them, so it is
// imported here under a distinct alias and returned directly so `?` needs no
// conversion.
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
use std::sync::Arc;

use core::ptr::addr_of_mut;
use std::ffi::{CStr, CString, OsStr};
// `prepBackgroundImage` opens the config path (byte-faithful `CString` bytes ->
// an `OsStr` path) via `std::os::unix::ffi::OsStrExt::from_bytes` (macos_impl is
// macOS-gated, so unix is guaranteed) — NEVER a lossy `String`.
use std::os::unix::ffi::OsStrExt;

use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandBuffer, MTLCommandBufferStatus};
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
use crate::message::{SearchMatch, SearchMatches};
use crate::metal::api::MTLPrimitiveType;
use crate::metal::buffer::{self, Buffer};
use crate::metal::frame::{CompletionContext, Frame};
use crate::metal::render_pass::{Attachment, AttachmentTarget, Draw, Step};
// The Metal-ABI POD serializers (§2): `FrameState`'s GPU buffers hold these
// byte-faithful types; `draw_frame` converts the ergonomic `crate::shaders`
// structs at the `sync` boundary.
use crate::latest_frame::{DrawMutex, LatestFrame};
use crate::metal::gpu_abi::{BgImageGpu, CellTextGpu, UniformsGpu};
use crate::metal::sampler::{self, Sampler};
use crate::metal::shaders::Shaders;
use crate::metal::target::{self, Target};
use crate::metal::texture::{self, Texture};
use crate::metal::Metal;
use crate::row::never_extend_bg;
use crate::shaders::{
    Atlas, BgImage, BgImageInfo, BgImageInfoFit, BgImageInfoPosition, CellBg, CellText,
    CellTextBools, Uniforms, UniformsBools, UniformsPaddingExtend,
};
use crate::shadertoy;
use crate::size::{
    AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, CellSize, GridSize,
    Padding, ScreenSize, Size, WindowColorspace,
};
use crate::state;
use crate::Overlay;
// W2: the `xev::Async` (Copy+Send mach-port wakeup) `loop_enter` forwards to the
// CVDisplayLink output callback — the reference `thr.draw_now` field. Not an
// apprt type (R4); `rustty-xev` is a leaf crate.
use rustty_xev::Async;

// const log = std.log.scoped(.generic_renderer);   (generic.zig:46)
//
// DECISION-logging.md §6r call shape 3: `rustty-renderer` sits BELOW
// `rustty-vt-c` in R4's layering, so restored sites call the `log` facade with
// `target: LOG_SCOPE` carrying the reference's `std.log.scoped` tag. The
// constant lives INSIDE `macos_impl` (not at file scope) because every restored
// site in this file is in this macOS-gated module; the `linux_impl` arm
// (`generic/linux_impl.rs`) is a later stage and gets its own.
const LOG_SCOPE: &str = "generic_renderer";

/// The number of buffers we use for double/triple buffering.
//
// PORT NOTE: reference `SwapChain.buf_count = GraphicsAPI.swap_chain_count`.
// Monomorphized to the concrete `Metal::SWAP_CHAIN_COUNT` (= 3, triple
// buffering). Used as the `[FrameState; SWAP_CHAIN_COUNT]` array length + the
// `frame_sema` permit count (GPU_RD_CHUNK3A_DESIGN §B.5).
const SWAP_CHAIN_COUNT: usize = Metal::SWAP_CHAIN_COUNT;

/// Whether +Y points down in the custom-shader coordinate space.
//
// PORT NOTE (Chunk 3c §E.5): reference `GraphicsAPI.custom_shader_y_is_down`
// (Metal.zig `pub const custom_shader_y_is_down = true;`). Monomorphized to the
// concrete `Metal::CUSTOM_SHADER_Y_IS_DOWN` (the `SWAP_CHAIN_COUNT` const-collapse
// precedent above). `update_custom_shader_uniforms_for_frame` keeps BOTH reference
// `if` branches verbatim (R1 branch fidelity); the const-collapse keeps them,
// matching the comptime source (do NOT hand-drop the dead branch).
//
// `2f0b65346` makes it public on the renderer: "Whether +Y is down in the
// coordinate space of exported frames. Apprts use this to orient frames when
// presenting." (`generic.zig:95-97`).
pub const CUSTOM_SHADER_Y_IS_DOWN: bool = Metal::CUSTOM_SHADER_Y_IS_DOWN;

/// A finished frame ready to be consumed by the apprt in case that the
/// frame needs to be composited with UI elements by the graphical toolkit
/// manually.
//
// pub const ExportedFrame = if (@hasDecl(GraphicsAPI, "ExportedFrame"))
//     GraphicsAPI.ExportedFrame else void;   (generic.zig:93)
// Metal declares none: `void`, modelled as `()` (`crate::latest_frame`).
pub type ExportedFrame = ();

/// PORT NOTE: the aggregate renderer error. Reference `generic.zig` methods return
/// Zig `!T` (inferred error unions of the Metal leaf errors + OOM). Rust `?`
/// needs a common type, so this enum aggregates the leaf errors via the `From`
/// impls below (R1 cycle-break glue — the faithful mapping of Zig's error-set
/// union; the richer `RendererError` for the whole draw path is a later chunk).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A Metal API call failed (any of the buffer/texture/sampler/target
    /// leaf `MetalFailed`s).
    MetalFailed,
    /// An allocation failed (the font `Atlas` allocation).
    OutOfMemory,
    // PORT NOTE (`c4e16970a`): the `Defunct` variant is DELETED with the
    // swap chain's defunct flag — `nextFrame` is infallible and a released
    // chain is `swap_chain == null` (rebuilt in `drawFrame`).
    /// A background-image `wuffs` decode failed (`WuffsError`/`Overflow`).
    /// `prepBackgroundImage` propagates the wuffs `Error` via `try` (the
    /// reference `!void` error set includes the wuffs `Error`); OOM maps to
    /// `OutOfMemory`, the rest to this. Note: only the DECODE error propagates —
    /// the open/read failures `break :load_background` (a warn + no-op), never
    /// propagate (§3.3).
    ImageDecode,
}

// PORT NOTE (W3 E1): the reference error is an inferred error union that
// already flows out of `drawFrame`; core's `renderer_seam::RendererError`
// aliases this `Error` so the real variant propagates through
// `Surface::draw` / `InitError::Renderer`'s `Display` delegation. Those
// seams need `Display` + `std::error::Error`; the enum only derives `Debug`,
// so bridge via the `Debug` rendering (the leaf variant name is the message).
impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

// rustty_wuffs::Error {OutOfMemory, WuffsError, Overflow} -> the renderer Error.
// `prepBackgroundImage` returns `!void` whose error set includes the wuffs
// `Error` (via `try wuffs.png.decode(...)`); this preserves "the decode error
// propagates out of prepBackgroundImage" (§3.3). OOM reuses `OutOfMemory`;
// `WuffsError`/`Overflow` map to `ImageDecode`.
impl From<rustty_wuffs::Error> for Error {
    fn from(e: rustty_wuffs::Error) -> Self {
        match e {
            rustty_wuffs::Error::OutOfMemory => Error::OutOfMemory,
            rustty_wuffs::Error::WuffsError | rustty_wuffs::Error::Overflow => Error::ImageDecode,
        }
    }
}

impl From<buffer::Error> for Error {
    fn from(_: buffer::Error) -> Self {
        Error::MetalFailed
    }
}
impl From<texture::Error> for Error {
    fn from(_: texture::Error) -> Self {
        Error::MetalFailed
    }
}
impl From<sampler::Error> for Error {
    fn from(_: sampler::Error) -> Self {
        Error::MetalFailed
    }
}
impl From<target::Error> for Error {
    fn from(_: target::Error) -> Self {
        Error::MetalFailed
    }
}
impl From<OutOfMemory> for Error {
    fn from(_: OutOfMemory) -> Self {
        Error::OutOfMemory
    }
}
// The Metal graphics-API leaf errors `drawFrame` propagates: `Metal::{surface_size,
// present, present_last_target}` return `metal_impl::Error` (`NoMetalDevice`), and
// `Metal::begin_frame` returns `frame::Error` (`MetalFailed`). Both fold into the
// aggregate `MetalFailed` (the reference `!void` inferred-union member).
impl From<crate::metal::metal_impl::Error> for Error {
    fn from(_: crate::metal::metal_impl::Error) -> Self {
        Error::MetalFailed
    }
}
impl From<crate::metal::frame::Error> for Error {
    fn from(_: crate::metal::frame::Error) -> Self {
        Error::MetalFailed
    }
}
// `Metal::init_shaders` (via `Renderer::init_shaders`) returns
// `crate::metal::shaders::Error` (`MetalFailed`, or `OutOfMemory` from
// `createWithBytes`, N8).
impl From<crate::metal::shaders::Error> for Error {
    fn from(e: crate::metal::shaders::Error) -> Self {
        match e {
            crate::metal::shaders::Error::MetalFailed => Error::MetalFailed,
            crate::metal::shaders::Error::OutOfMemory => Error::OutOfMemory,
        }
    }
}
// `font.Shaper.init` (`changeConfig`, and the deferred `Renderer::init`) fails
// with the active shaper's error (`shape::ShaperError` — `coretext::Error` on
// the default `.coretext` backend: an atlas OOM or a font `Collection`
// failure; the HarfBuzz/noop shaper's on `-Dfont-backend=coretext_freetype`/
// `coretext_harfbuzz`/`coretext_noshape`, S-14). It folds into the aggregate
// `OutOfMemory` member of the reference `!void` inferred union (a
// font-resource failure, not a Metal one).
impl From<shape::ShaperError> for Error {
    fn from(_: shape::ShaperError) -> Self {
        Error::OutOfMemory
    }
}

// `Renderer::init`'s `DisplayLink::create_with_active_cg_displays()?` and
// `Renderer::loop_enter`'s `display_link.set_output_callback(draw_now)?`
// propagate the CVDisplayLink wrapper error (W2). The reference
// `createWithActiveCGDisplays` maps CoreVideo failure to `error.OutOfMemory`
// (its `Allocator.Error`); we mirror that by folding the sole
// `cvdisplaylink::Error::InvalidOperation` into the aggregate `OutOfMemory`
// member — keeping the reference `try … catch error.OutOfMemory` faithful.
impl From<crate::metal::cvdisplaylink::Error> for Error {
    fn from(_: crate::metal::cvdisplaylink::Error) -> Self {
        Error::OutOfMemory
    }
}

// PORT NOTE (`c4e16970a`): the `Defunct` marker type (reference
// `error{Defunct}`) is DELETED — `SwapChain::next_frame` is infallible.

// PORT NOTE (surface-mailbox seam RESOLVED — was a per-arm no-op placeholder):
// `SurfaceMailbox`/`MailboxTimeout`/`SurfaceMessage`/`SurfaceMailboxSink` are now
// SHARED module-level types (`super::`, one copy for both `Renderer(GraphicsAPI)`
// monomorphizations) and the mailbox is REAL: core attaches a
// `SurfaceMailboxSink` over `apprt.surface.Mailbox` through `Options`
// (GPU_RD_CHUNK3A_DESIGN §B.2). Imported at the `use super::{…}` line above.

// PORT NOTE (W2 — un-deferred): reference `const DisplayLink =
// switch (builtin.os.tag) { .macos => *macos.video.DisplayLink, else => void }`.
// This crate's `generic`/`metal` modules are macOS-only (`lib.rs:78,121`), so the
// `.macos` arm is the only one compiled and `DisplayLink` is ALWAYS the real
// CoreVideo wrapper — every `if (comptime DisplayLink == void) return;` guard in
// the methods below collapses to an unconditional non-return. The wrapper lives
// in `crate::metal::cvdisplaylink` (W2-T1); re-exported here so the field type
// `display_link: Option<DisplayLink>` (below) is unchanged.
pub use crate::metal::cvdisplaylink::DisplayLink;
use crate::metal::cvdisplaylink::DisplayLinkHandle;

/// PORT-ONLY (UD352): the outcome of the `syncDisplayLink` tail's reads,
/// applied later without touching the renderer. See
/// `Renderer::sync_display_link`.
#[derive(Clone, Copy, Debug)]
pub struct DisplayLinkSync {
    link: Option<DisplayLinkHandle>,
    should_run: bool,
}

impl DisplayLinkSync {
    /// The CoreVideo half of the `syncDisplayLink` tail. Must be called on
    /// the render thread (which owns and releases the link) and must NOT be
    /// called while holding `draw_mutex`: stopping a CVDisplayLink is a
    /// blocking join on CoreVideo's IO thread.
    pub fn apply(self) {
        #[cfg(test)]
        tests::on_sync_display_link();
        let Some(display_link) = self.link else {
            return;
        };
        // SAFETY: the link is released only by the render thread's teardown
        // (`Renderer::deinit`, after the thread is joined), and this runs on
        // the render thread, so the `DisplayLink` behind the handle is live.
        unsafe {
            if self.should_run {
                if !display_link.is_running() {
                    let _ = display_link.start(); // catch {}
                }
            } else {
                let _ = display_link.stop(); // catch {}
            }
        }
    }
}

// PORT NOTE: reference `const HighlightTag = enum(u8) { search_match,
// search_match_selected }` (Renderer-private). Widened to `pub` (dead-code
// exempt); consumed by the deferred `rebuildCells`/`drawFrame` search path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum HighlightTag {
    SearchMatch,
    SearchMatchSelected,
}

/// The monomorphic GPU renderer over `Metal` (reference
/// `GenericRenderer(Metal)`).
//
// R1-PRESERVED reference doc-comment (`generic.zig:46-80`) on the
// `pub fn Renderer(comptime GraphicsAPI: type)` declaration. The monomorphic
// port collapses this comptime hierarchy onto the concrete `Metal` leaf types
// (`GraphicsAPI` -> `Metal`, `Target` -> `metal::Target`, `Frame`/`RenderPass`/
// `Step`/`Pipeline`/`Buffer`/`Texture` -> their `crate::metal::*` leaves); the
// documented abstraction hierarchy is preserved verbatim:
//
//   Create a renderer type with the provided graphics API wrapper.
//
//   The graphics API wrapper must provide the interface outlined below.
//   Specific details for the interfaces are documented on the existing
//   implementations (`Metal` and `OpenGL`).
//
//   Hierarchy of graphics abstractions:
//
//   [ GraphicsAPI ] - Responsible for configuring the runtime surface
//      |     |        and providing render `Target`s that draw to it,
//      |     |        as well as `Frame`s and `Pipeline`s.
//      |     V
//      | [ Target ] - Represents an abstract target for rendering, which
//      |              could be a surface directly but is also used as an
//      |              abstraction for off-screen frame buffers.
//      V
//   [ Frame ] - Represents the context for drawing a given frame,
//      |        provides `RenderPass`es for issuing draw commands
//      |        to, and reports the frame health when complete.
//      V
//   [ RenderPass ] - Represents a render pass in a frame, consisting of
//     :              one or more `Step`s applied to the same target(s),
//   [ Step ] - - - - each describing the input buffers and textures and
//     :              the vertex/fragment functions and geometry to use.
//     :_ _ _ _ _ _ _ _ _ _/
//     v
//   [ Pipeline ] - Describes a vertex and fragment function to be used
//                  for a `Step`; the `GraphicsAPI` is responsible for
//                  these and they should be constructed and cached
//                  ahead of time.
//
//   [ Buffer ] - An abstraction over a GPU buffer.
//
//   [ Texture ] - An abstraction over a GPU texture.
//
//   [ ExportedFrame ] - A finished frame ready to be consumed by the apprt
//                       in case that the frame needs to be composited with
//                       UI elements by the graphical toolkit manually.
//
// PORT NOTE: reference `pub fn Renderer(comptime GraphicsAPI: type) type {
// return struct { ... } }`. The `const Self = @This(); pub const API =
// GraphicsAPI;` + the `Target`/`Buffer`/`Sampler`/`Texture`/`RenderPass`/
// `shaderpkg`/`Shaders` comptime aliases collapse into the concrete `Metal`
// leaf types used directly below. Fields are `pub` (the reference accesses
// them via `self.<field>` from the renderer's own methods; making them pub is
// faithful — Zig struct fields are non-opaque — and keeps them dead-code-exempt
// until the deferred draw path reads them).
pub struct Renderer {
    /// Allocator that can be used
    //
    // Zig `alloc: std.mem.Allocator` -> `Global` (the ZST global allocator;
    // the renderer-wide `Allocator = Global` decision).
    pub alloc: Global,

    /// This mutex must be held whenever any state used in `drawFrame` is
    /// being modified, and also when it's being accessed in `drawFrame`.
    //
    // Zig `draw_mutex: std.Thread.Mutex = .{}` -> `rustty_zigstd::thread::Mutex`.
    //
    // PORT NOTE (N437): in its own allocation (`DrawMutex`, an `Arc` handle);
    // see `crate::latest_frame`. Every cross-thread entry locks it through the
    // caller's own handle before it forms a reference to the renderer.
    pub draw_mutex: DrawMutex,

    /// The configuration we need derived from the main config.
    pub config: DerivedConfig,

    /// The mailbox for communicating with the window.
    pub surface_mailbox: SurfaceMailbox,

    /// The latest exported frame ready to be consumed by an apprt
    /// who needs to manually composite the frame with UI elements.
    /// Previously exported frames are released when a new frame is
    /// exported and pushed onto the queue.
    ///
    /// Unused if the renderer does not need to export frames to
    /// present its rendered frame.
    //
    // `ExportedFrame` is `void` for Metal (`@hasDecl(Metal, "ExportedFrame")`
    // is false), so the slot is `LatestFrame<()>` and every push/take is a
    // no-op. Behind an `Arc` as on the OpenGL twin (N437).
    pub latest_frame: Arc<LatestFrame<()>>,

    /// Current font metrics defining our grid.
    pub grid_metrics: Metrics,

    /// The size of everything.
    pub size: Size,

    /// True if the window is focused
    pub focused: bool,

    /// True if the window is visible.
    pub visible: bool,

    /// Flag to indicate that our focus state changed for custom
    /// shaders to update their state.
    pub custom_shader_focused_changed: bool,

    /// The most recent scrollbar state. We use this as a cache to
    /// determine if we need to notify the apprt that there was a
    /// scrollbar change.
    pub scrollbar: Scrollbar,
    pub scrollbar_dirty: bool,

    /// Tracks the last bottom-right pin of the screen to detect new output.
    /// When the final line changes (node or y differs), new content was added.
    /// Used for scroll-to-bottom on output feature.
    pub last_bottom_node: Option<usize>,
    pub last_bottom_y: CellCountInt,

    /// The most recent viewport matches so that we can render search
    /// matches in the visible frame. This is provided asynchronously
    /// from the search thread so we have the dirty flag to also note
    /// if we need to rebuild our cells to include search highlights.
    ///
    /// Note that the selections MAY BE INVALID (point to PageList nodes
    /// that do not exist anymore). These must be validated prior to use.
    pub search_matches: Option<SearchMatches>,
    pub search_selected_match: Option<SearchMatch>,
    pub search_matches_dirty: bool,

    /// The current set of cells to render. This is rebuilt on every frame
    /// but we keep this around so that we don't reallocate. Each set of
    /// cells goes into a separate shader.
    pub cells: Contents,

    /// Set to true after rebuildCells is called. This can be used
    /// to determine if any possible changes have been made to the
    /// cells for the draw call.
    //
    // PORT NOTE: reference defect (R13), UD352. The reference field is a plain
    // `bool`, written under `draw_mutex` (`rebuildCells` sets it, `drawFrame`
    // clears it) and read WITHOUT the mutex by `syncDisplayLink` on the render
    // thread while the main thread's CoreAnimation `drawFrame(sync)` can be
    // clearing it: a data race, UB in Rust. An `AtomicBool` (Relaxed; the
    // stores stay under the lock, which orders everything else) removes it.
    pub cells_rebuilt: AtomicBool,

    /// The current GPU uniform values.
    pub uniforms: Uniforms,

    /// Custom shader uniform values.
    pub custom_shader_uniforms: shadertoy::Uniforms,

    /// Timestamp we rendered out first frame.
    ///
    /// This is used when updating custom shader uniforms.
    pub first_frame_time: Option<Instant>,

    /// Timestamp when we rendered out more recent frame.
    ///
    /// This is used when updating custom shader uniforms.
    pub last_frame_time: Option<Instant>,

    /// The font structures.
    //
    // Zig `font_grid: *font.SharedGrid` -> `*mut SharedGrid` (raw handle to the
    // cross-surface shared grid; the `message.rs` `FontGrid.grid` precedent).
    pub font_grid: *mut SharedGrid,
    pub font_shaper: Shaper,
    pub font_shaper_cache: ShaperCache,

    /// The images that we may render.
    //
    // Zig `images: ImageState = .empty` (`ImageState = imagepkg.State`).
    pub images: image::State,

    /// Background image, if we have one.
    pub bg_image: Option<image::Image>,
    /// Set whenever the background image changes, signalling
    /// that the new background image needs to be uploaded to
    /// the GPU.
    ///
    /// This is initialized as true so that we load the image
    /// on renderer initialization, not just on config change.
    pub bg_image_changed: bool,
    /// Background image vertex buffer.
    pub bg_image_buffer: BgImage,
    /// This value is used to force-update the swap chain copy
    /// of the background image buffer whenever we change it.
    pub bg_image_buffer_modified: usize,

    /// Graphics API state.
    pub api: Metal,

    /// The CVDisplayLink used to drive the rendering loop in
    /// sync with the display. This is void on platforms that
    /// don't support a display link.
    pub display_link: Option<DisplayLink>,

    /// Health of the most recently completed frame.
    //
    // PORT NOTE (N440, P7-W7 T5a): behind an `Arc`, in its own allocation. The
    // frame completion handler runs on a Metal-owned thread without the draw
    // mutex; each in-flight frame's `CompletionContext` holds a clone, so the
    // handler's load/store touch no renderer byte.
    pub health: Arc<AtomicHealth>,

    /// The health reports to the surface: the health it was last told,
    /// whether a report is owed, and the paced retry wakeup.
    //
    // rustty-only (R13, UPSTREAM-DEFECTS.md UD362); see `HealthReport`.
    pub health_report: Arc<HealthReport>,

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

    /// This value is used to force-update swap chain targets in the
    /// event of a config change that requires it (such as blending mode).
    pub target_config_modified: usize,

    /// If something happened that requires us to reinitialize our shaders,
    /// this is set to true so that we can do that whenever possible.
    pub reinitialize_shaders: bool,

    /// Whether or not we have custom shaders.
    pub has_custom_shaders: bool,

    /// Our shader pipelines.
    pub shaders: Shaders,

    /// The render state we update per loop.
    //
    // Zig `terminal_state: terminal.RenderState = .empty`.
    pub terminal_state: RenderState,

    /// The number of frames since the last terminal state reset.
    /// We reset the terminal state after ~100,000 frames (about 10 to
    /// 15 minutes at 120Hz) to prevent wasted memory buildup from
    /// a large screen.
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

/// Swap chain which maintains multiple copies of the state needed to
/// render a frame, so that we can start building the next frame while
/// the previous frame is still being processed on the GPU.
//
// PORT NOTE: reference `const SwapChain = struct` (Renderer-private). Widened
// to `pub` so the staged-port methods (their callers `Renderer::init`/`deinit`/
// `drawFrame` land later) are dead-code-exempt under `-D warnings` without
// per-item `#[allow(dead_code)]` (GPU_RD_CHUNK3A_DESIGN §B.4).
pub struct SwapChain {
    // The count of buffers we use for double/triple buffering.
    // If this is one then we don't do any double+ buffering at all.
    // This is comptime because there isn't a good reason to change
    // this at runtime and there is a lot of complexity to support it.
    //
    // (Zig `const buf_count = GraphicsAPI.swap_chain_count` -> the module
    //  const `SWAP_CHAIN_COUNT` above.)
    /// `buf_count` structs that can hold the
    /// data needed by the GPU to draw a frame.
    pub frames: [FrameState; SWAP_CHAIN_COUNT],
    /// Index of the most recently used frame state struct.
    //
    // Zig `frame_index: std.math.IntFittingRange(0, buf_count) = 0` — the
    // smallest int fitting `0..=3` is `u2`; Rust has no sub-byte int, so `u8`.
    pub frame_index: u8,
    /// Semaphore that we wait on to make sure we have an available
    /// frame state struct so we can start working on a new frame.
    //
    // Zig `frame_sema: std.Thread.Semaphore = .{ .permits = buf_count }` ->
    // `Semaphore::with_permits(SWAP_CHAIN_COUNT)` (set in `init`).
    //
    // PORT NOTE (N440, P7-W7 T5a): behind an `Arc`, in its own allocation: each
    // in-flight frame's `CompletionContext` holds a clone and `releaseFrame`s
    // through it from the completion handler, which runs without the draw
    // mutex and must touch no renderer byte. A released chain's semaphore lives
    // until the last in-flight completion drops its clone.
    pub frame_sema: Arc<Semaphore>,
    // PORT NOTE (`c4e16970a`): the `defunct` flag is DELETED — the owner's
    // `swap_chain: ?SwapChain` (`Option`) now models torn-down state, so a
    // released/unrealized chain simply doesn't exist.
}

impl SwapChain {
    pub fn init(api: &Metal, custom_shaders: bool) -> Result<SwapChain, Error> {
        // Zig: var result: SwapChain = .{ .frames = undefined };
        //      // Initialize all of our frame state.
        //      for (&result.frames) |*frame| {
        //          frame.* = try FrameState.init(api, custom_shaders);
        //      }
        //      return result;
        //
        // Rust builds the fixed-size `[FrameState; SWAP_CHAIN_COUNT]` fallibly
        // (no stable `array::try_from_fn`) via a `Vec` we move into the array,
        // preserving the `buf_count` loop.
        let mut frames_vec: Vec<FrameState> = Vec::with_capacity(SWAP_CHAIN_COUNT);
        for _ in 0..SWAP_CHAIN_COUNT {
            frames_vec.push(FrameState::init(api, custom_shaders)?);
        }
        let frames: [FrameState; SWAP_CHAIN_COUNT] = match frames_vec.try_into() {
            Ok(a) => a,
            // We pushed exactly `SWAP_CHAIN_COUNT` items, so the length always
            // matches; the `Err(Vec)` arm is unreachable.
            Err(_) => unreachable!(),
        };

        Ok(SwapChain {
            frames,
            // Zig field defaults (relied on by `.{ .frames = undefined }`):
            frame_index: 0,
            frame_sema: Arc::new(Semaphore::with_permits(SWAP_CHAIN_COUNT)),
        })
    }

    pub fn deinit(&mut self) {
        // Wait for all of our inflight draws to complete
        // so that we can cleanly deinit our GPU state.
        // Zig: for (0..buf_count) |_| self.frame_sema.wait();
        for _ in 0..SWAP_CHAIN_COUNT {
            self.frame_sema.wait();
        }
        // Zig: for (&self.frames) |*frame| frame.deinit();
        //
        // The `FrameState` GPU resources are `Retained`/`CFRetained`-owned and
        // release via `Drop` (the buffer/texture/sampler/target precedent — none
        // expose a manual `deinit`). They release when the owning `SwapChain`
        // (its `frames` array) is dropped, which the caller (`Renderer::deinit`)
        // does immediately after this returns. The explicit per-frame release is
        // therefore the field `Drop`, not a manual call.
    }

    /// Get the next frame state to draw to. This will wait on the
    /// semaphore to ensure that the frame is available. This must
    /// always be paired with a call to releaseFrame.
    //
    // pub fn nextFrame(self: *SwapChain) *FrameState {
    // (`c4e16970a`: infallible — the defunct check is gone; the Zig
    // `errdefer` was deleted with it.)
    pub fn next_frame(&mut self) -> &mut FrameState {
        // Zig: self.frame_sema.wait();
        self.frame_sema.wait();
        self.frame_index = (self.frame_index + 1) % (SWAP_CHAIN_COUNT as u8);
        &mut self.frames[self.frame_index as usize]
    }

    /// This should be called when the frame has completed drawing.
    pub fn release_frame(&self) {
        self.frame_sema.post();
    }
}

/// State we need duplicated for every frame. Any state that could be
/// in a data race between the GPU and CPU while a frame is being drawn
/// should be in this struct.
///
/// While a draw is in-process, we "lock" the state (via a semaphore)
/// and prevent the CPU from updating the state until our graphics API
/// reports that the frame is complete.
///
/// This is used to implement double/triple buffering.
//
// PORT NOTE: reference `const FrameState = struct` (Renderer-private), widened
// to `pub` (dead-code exempt; §B.4). The Zig buffer-type aliases
// (`UniformBuffer = Buffer(shaderpkg.Uniforms)`, `CellBgBuffer = Buffer(...CellBg)`,
// `CellTextBuffer = Buffer(...CellText)`, `BgImageBuffer = Buffer(...BgImage)`)
// are inlined as the concrete `Buffer<T>` field types below.
pub struct FrameState {
    // §2.6: the GPU buffers hold the Metal-ABI POD serializer types (byte-
    // faithful to what the `.metal` shaders read), NOT the ergonomic
    // `crate::shaders` structs. `cells_bg` is UNCHANGED — `CellBg = [u8;4]` is
    // already the exact `uchar4` ABI.
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
    ///
    /// TODO: Make this an optional and only create it
    ///       if we actually have a background image.
    pub bg_image_buffer: Buffer<BgImageGpu>,
    /// See property of same name on Renderer for explanation.
    pub bg_image_buffer_modified: usize,

    /// Custom shader state, this is null if we have no custom shaders.
    pub custom_shader_state: Option<CustomShaderState>,
}

impl FrameState {
    pub fn init(api: &Metal, custom_shaders: bool) -> Result<FrameState, Error> {
        // Uniform buffer contains exactly 1 uniform struct. The
        // uniform data will be undefined so this must be set before
        // a frame is drawn.
        // (Zig `errdefer uniforms.deinit()` — the `Retained` releases on the
        //  `?` early-return `Drop`; no manual errdefer needed. Same for each
        //  resource below.)
        let uniforms = Buffer::<UniformsGpu>::init(api.uniform_buffer_options(), 1)?;

        // Create GPU buffers for our cells.
        //
        // We start them off with a size of 1, which will of course be
        // too small, but they will be resized as needed. This is a bit
        // wasteful but since it's a one-time thing it's not really a
        // huge concern.
        let cells = Buffer::<CellTextGpu>::init(api.fg_buffer_options(), 1)?;
        let cells_bg = Buffer::<CellBg>::init(api.bg_buffer_options(), 1)?;

        // Create a GPU buffer for our background image info.
        let bg_image_buffer = Buffer::<BgImageGpu>::init(api.bg_image_buffer_options(), 1)?;

        // Initialize our textures for our font atlas.
        //
        // As with the buffers above, we start these off as small
        // as possible since they'll inevitably be resized anyway.
        //
        // PORT NOTE: Zig passes a throwaway `*const font.Atlas`
        // (`&.{ .data = undefined, .size = 1, .format = .grayscale }`) —
        // `initAtlasTexture` reads only `.size`/`.format`. rustty's `Atlas` has
        // private fields (only `Atlas::init(alloc, size, format)` is public), so
        // the throwaway is realized as a real atlas over `Global`. CORRECTION
        // (Chunk 3b — first caller to exercise this path via `SwapChain::init`):
        // the reference's `size = 1` is a raw STRUCT LITERAL that never runs
        // `clear()`, but rustty's `Atlas::init` calls `clear()`, whose initial
        // node width is `self.size - 2` (atlas.rs) — which UNDERFLOW-panics for a
        // real atlas at `size < 2`. So the throwaway uses the smallest VALID real
        // size, 2 (`init_atlas_texture` reads only `.size()`/`.format()`, so the
        // 2x2 texture is immaterial — it is resized on the first atlas sync); the
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

        // Initialize the target. Just as with the other resources,
        // start it off as small as we can since it'll be resized.
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
            // Zig field defaults (`= 0`):
            grayscale_modified: 0,
            color_modified: 0,
            target_config_modified: 0,
            bg_image_buffer_modified: 0,
        })
    }

    // pub fn deinit(self: *FrameState) void {
    //     self.target.deinit();
    //     self.uniforms.deinit();
    //     self.cells.deinit();
    //     self.cells_bg.deinit();
    //     self.grayscale.deinit();
    //     self.color.deinit();
    //     self.bg_image_buffer.deinit();
    //     if (self.custom_shader_state) |*state| state.deinit();
    // }
    //
    // PORT NOTE: every field is a `Retained`/`CFRetained`-owned GPU resource
    // (or a nested `CustomShaderState` of the same) whose Zig `deinit` maps to
    // Rust `Drop` (the buffer/texture/sampler/target precedent — none expose a
    // manual `deinit`). Dropping a `FrameState` releases all fields, so the
    // reference `deinit` is the automatic field `Drop`; no explicit method.

    pub fn resize(&mut self, api: &Metal, width: usize, height: usize) -> Result<(), Error> {
        // Zig: if (self.custom_shader_state) |*state| { try state.resize(...); }
        if let Some(state) = self.custom_shader_state.as_mut() {
            state.resize(api, width, height)?;
        }
        // Zig: const target = try api.initTarget(width, height);
        //      self.target.deinit();
        //      self.target = target;
        //
        // The reassignment drops the old `Target` (its `Retained`/`CFRetained`
        // release) — the reference `self.target.deinit()`.
        let target = api.init_target(width, height)?;
        self.target = target;
        Ok(())
    }
}

/// State relevant to our custom shaders if we have any.
//
// PORT NOTE: reference `const CustomShaderState = struct` (Renderer-private),
// widened to `pub` (dead-code exempt; §B.4). The Zig `const UniformBuffer =
// Buffer(shadertoy.Uniforms)` alias is inlined as the concrete field type.
pub struct CustomShaderState {
    /// When we have a custom shader state, we maintain a front
    /// and back texture which we use as a swap chain to render
    /// between when multiple custom shaders are defined.
    pub front_texture: Texture,
    pub back_texture: Texture,

    /// Shadertoy uses a sampler for accessing the various channel
    /// textures. In Metal, we need to explicitly create these since
    /// the glslang-to-msl compiler doesn't do it for us (as we
    /// normally would in hand-written MSL). To keep it clean and
    /// consistent, we just force all rendering APIs to provide an
    /// explicit sampler.
    ///
    /// Samplers are immutable and describe sampling properties so
    /// we can share the sampler across front/back textures (although
    /// we only need it for the source texture at a time, we don't
    /// need to "swap" it).
    pub sampler: Sampler,

    pub uniforms: Buffer<shadertoy::Uniforms>,
}

impl CustomShaderState {
    /// Swap the front and back textures.
    pub fn swap(&mut self) {
        // Zig: std.mem.swap(Texture, &self.front_texture, &self.back_texture);
        mem::swap(&mut self.front_texture, &mut self.back_texture);
    }

    pub fn init(api: &Metal) -> Result<CustomShaderState, Error> {
        // Create a GPU buffer to hold our uniforms.
        let uniforms = Buffer::<shadertoy::Uniforms>::init(api.uniform_buffer_options(), 1)?;

        // Initialize the front and back textures at 1x1 px, this
        // is slightly wasteful but it's only done once so whatever.
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

    // pub fn deinit(self: *CustomShaderState) void {
    //     self.front_texture.deinit();
    //     self.back_texture.deinit();
    //     self.sampler.deinit();
    //     self.uniforms.deinit();
    // }
    //
    // PORT NOTE: as with `FrameState`, every field is a `Retained`-owned GPU
    // resource whose Zig `deinit` maps to Rust `Drop`; no explicit method.

    pub fn resize(&mut self, api: &Metal, width: usize, height: usize) -> Result<(), Error> {
        // Zig `@intCast(width)`/`@intCast(height)` — `Texture::init` takes
        // `usize`, so no narrowing cast is needed here.
        let front_texture = Texture::init(api.texture_options(), width, height, None)?;
        let back_texture = Texture::init(api.texture_options(), width, height, None)?;

        // Zig: self.front_texture.deinit(); self.back_texture.deinit();
        //      self.front_texture = front_texture; self.back_texture = back_texture;
        //
        // The reassignments drop the old textures (`Retained` release).
        self.front_texture = front_texture;
        self.back_texture = back_texture;
        Ok(())
    }
}

mod cells;
mod cross_thread;
mod draw;
mod frame;
mod lifecycle;

pub use draw::HealthReport;

// =======================================================================
// Chunk 3b unit tests (macOS-gated, Metal-skip; GPU_RD_CHUNK3B_DESIGN §G)
// =======================================================================
#[cfg(test)]
mod tests;
