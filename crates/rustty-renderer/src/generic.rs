//! The monomorphic GPU renderer (`GenericRenderer(Metal)`).
//!
//! PORTS: ghostty `src/renderer/generic.zig` — the `Renderer` struct decl, its
//! nested `SwapChain`/`FrameState`/`CustomShaderState`, and the `DerivedConfig`.
//!
//! Per `GPU_RD_TRAIT_DESIGN.md` §D + the committed monomorphic decision, the
//! reference `pub fn Renderer(comptime GraphicsAPI: type) type` is ported as a
//! CONCRETE `Renderer` over the concrete `Metal` leaf types (a set of one — no
//! `GraphicsAPI` trait hierarchy; OpenGL is Linux-deferred, WebGL wasm-deferred).
//!
//! PORT STATUS (Chunk 3a — struct + swap-chain/frame-state skeleton):
//!
//! - `DerivedConfig` (struct + `init<C>`/`deinit`) — the `config -> renderer`
//!   cycle is broken by (a) hoisting the 6 clean config value-types to
//!   `rustty-render-types` (Part A, landed), and (b) porting `DerivedConfig`
//!   generic over `C` (the `renderer/Thread.zig` `DerivedConfig::init<C>`
//!   precedent, `thread.rs`); the 9 non-hoistable fields (`TerminalColor`×7,
//!   `Path`, `RepeatablePath`) use inhabited default-constructible placeholders
//!   until `init<C>` and the golden gate land (GPU_RD_CHUNK3A_DESIGN §B.1).
//! - The `Renderer` struct (all fields, comments preserved) + nested
//!   `SwapChain`/`FrameState`/`CustomShaderState` (+ the semaphore-backpressure
//!   method set: `SwapChain::{init,deinit,next_frame,release_frame}` using
//!   `frame_sema.wait()`/`post()`; `FrameState::{init,resize}`;
//!   `CustomShaderState::{init,resize,swap}`).
//!
//! DEFERRED (the draw path — a later chunk): `updateFrame`/`rebuildCells`/
//! `drawFrame`/`uniforms` math + `initShaders`/`deinitShaders`/`updateFontGrid*`
//! are NOT declared here; `Renderer::{init,deinit}` are `unimplemented!()` seams
//! with the verbatim reference body preserved (R5). See DEFERRALS.md.
//!
//! DEAD-CODE (`-D warnings`): the reference `SwapChain`/`FrameState`/
//! `CustomShaderState` are Renderer-private (`const … = struct`) and their
//! methods are reachable only from the deferred `Renderer::init`/`drawFrame`, so
//! rustc would flag them "never used". They are WIDENED to `pub` (the
//! GPU_RD_CHUNK3A_DESIGN §B.4 sanctioned "make pub" route — pub items reachable
//! from the crate root are dead-code-exempt), which avoids per-item
//! `#[allow(dead_code)]`. The visibility widening is documented per type.

use allocator_api2::alloc::Global;
use core::sync::atomic::{AtomicI32, Ordering};
use std::ffi::CString;

use rustty_font_shaper::codepoint_resolver::StyleStatus;
use rustty_font_shaper::SharedGrid;
use rustty_render_types::{
    AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, FontShapingBreak,
    WindowColorspace, WindowPaddingColor,
};
use rustty_vt::color::RGB;
use rustty_vt::style::BoldColor;
use rustty_zigstd::arena_allocator::ArenaAllocator;

use crate::link;
use crate::size::Size;
use crate::Health;

// ===========================================================================
// DerivedConfig config-type placeholders (§B.1 — the 9 non-hoistable fields)
// ===========================================================================

/// PORT NOTE (DEFERRED CONFIG TYPE — inhabited placeholder): reference
/// `configpkg.Config.TerminalColor = union(enum) { color: Color,
/// cell_foreground, cell_background }`. `Color.toTerminalRGB()` returns
/// `rustty_vt::color::RGB` (`value_types.rs:1822,2021`), so hoisting
/// `TerminalColor` to `rustty-render-types` would drag `Color` -> `vt::color::RGB`
/// and force a `render-types -> vt` edge — a CYCLE (`vt -> render-types` already
/// exists, `rustty-vt/Cargo.toml`). It therefore takes the generic-`C` route
/// (`DerivedConfig::init<C>`, deferred): the value-carrying enum + the real
/// extraction land with the draw path, where the Chunk-4 offscreen golden render
/// reads these on the cursor/selection/search paths (GPU_RD_CHUNK3A_DESIGN §B.1
/// GOLDEN-GATE PROOF). Until then it is an inhabited, default-constructible
/// placeholder (the `thread::DerivedConfig` precedent).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TerminalColor;

/// PORT NOTE (DEFERRED CONFIG TYPE — inhabited placeholder): reference
/// `configpkg.Path` — a heap/arena string type over `rustty-args` +
/// `rustty-cfg-fmt` + `Allocator` (`path.rs`); hoisting it would add
/// `render-types -> args/cfg-fmt` edges and break the render-types leaf
/// invariant. Deferred as an inhabited placeholder (`bg_image` field); the real
/// `Path` clone lands with `DerivedConfig::init<C>` (GPU_RD_CHUNK3A_DESIGN §B.1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Path;

/// PORT NOTE (DEFERRED CONFIG TYPE — inhabited placeholder): reference
/// `configpkg.RepeatablePath` (the `custom_shaders` field) — same heap/arena
/// coupling as [`Path`]. Deferred as an inhabited placeholder; the real
/// `RepeatablePath` clone lands with `DerivedConfig::init<C>` (§B.1). Note that
/// `Renderer::init` reads `options.config.custom_shaders.value.items.len`
/// (generic.zig:667), so this placeholder is one reason `Renderer::init` itself
/// is deferred.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepeatablePath;

/// PORT: the inferred error set of [`DerivedConfig::init`] (`ArenaAllocator`
/// OOM + the config clones + `link::Set::from_config`'s `oni::Error`) — an
/// opaque growth-point marker until the config-coupled body lands (precedent:
/// `message::InitChangeConfigError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedConfigError;

// ===========================================================================
// DerivedConfig (the `renderer.Renderer.DerivedConfig`)
// ===========================================================================

/// The configuration for this renderer that is derived from the main
/// configuration. This must be exported so that we don't need to
/// pass around Config pointers which makes memory management a pain.
//
// PORT NOTE: reference `pub const DerivedConfig = struct` nested in
// `Renderer(GraphicsAPI)`. It names no Metal type, so it is NOT macOS-gated
// (the cross-platform `Message::ChangeConfig` payload — message.rs — points at
// it). Fields are `pub` (the reference accesses `config.<field>` from the
// renderer's methods; making them pub is faithful and keeps them dead-code-exempt
// until the draw path reads them). The 9 config-typed fields that cannot be named
// without cycling use the inhabited placeholders above (§B.1).
pub struct DerivedConfig {
    pub arena: ArenaAllocator,

    pub font_thicken: bool,
    pub font_thicken_strength: u8,
    // Zig `std.ArrayListUnmanaged([:0]const u8)` — a list of null-terminated
    // byte strings -> `Vec<CString>` (fidelity: byte strings, not lossy `String`).
    pub font_features: Vec<CString>,
    pub font_styles: StyleStatus,
    pub font_shaping_break: FontShapingBreak,
    pub cursor_color: Option<TerminalColor>,
    pub cursor_opacity: f64,
    pub cursor_text: Option<TerminalColor>,
    pub background: RGB,
    pub background_opacity: f64,
    pub background_opacity_cells: bool,
    pub foreground: RGB,
    pub selection_background: Option<TerminalColor>,
    pub selection_foreground: Option<TerminalColor>,
    pub search_background: TerminalColor,
    pub search_foreground: TerminalColor,
    pub search_selected_background: TerminalColor,
    pub search_selected_foreground: TerminalColor,
    pub bold_color: Option<BoldColor>,
    pub faint_opacity: u8,
    pub min_contrast: f32,
    pub padding_color: WindowPaddingColor,
    pub custom_shaders: RepeatablePath,
    pub bg_image: Option<Path>,
    pub bg_image_opacity: f32,
    pub bg_image_position: BackgroundImagePosition,
    pub bg_image_fit: BackgroundImageFit,
    pub bg_image_repeat: bool,
    pub links: link::Set,
    pub vsync: bool,
    pub colorspace: WindowColorspace,
    pub blending: AlphaBlending,
    pub background_blur: BackgroundBlur,
    pub scroll_to_bottom_on_output: bool,
}

impl DerivedConfig {
    // DEFERRED BODY (config-coupled — reads `config.@"cursor-color"` etc.; a
    // `renderer -> config` edge would cycle, G7 — the `Message::initChangeConfig`
    // precedent). Generic over `C` so core can name it; the concrete
    // instantiation (`C` = `rustty_config::Config`) binds higher up. The
    // reference body is preserved verbatim (R1); the real extraction — and the
    // real values for the 9 placeholder-typed fields (§B.1) — land with the draw
    // path. See GPU_RD_CHUNK3A_DESIGN §B.1 + DEFERRALS.md.
    //
    // pub fn init(
    //     alloc_gpa: Allocator,
    //     config: *const configpkg.Config,
    // ) !DerivedConfig {
    //     var arena = ArenaAllocator.init(alloc_gpa);
    //     errdefer arena.deinit();
    //     const alloc = arena.allocator();
    //
    //     // Copy our shaders
    //     const custom_shaders = try config.@"custom-shader".clone(alloc);
    //
    //     // Copy our background image
    //     const bg_image =
    //         if (config.@"background-image") |bg|
    //             try bg.clone(alloc)
    //         else
    //             null;
    //
    //     // Copy our font features
    //     const font_features = try config.@"font-feature".clone(alloc);
    //
    //     // Get our font styles
    //     var font_styles = font.CodepointResolver.StyleStatus.initFill(true);
    //     font_styles.set(.bold, config.@"font-style-bold" != .false);
    //     font_styles.set(.italic, config.@"font-style-italic" != .false);
    //     font_styles.set(.bold_italic, config.@"font-style-bold-italic" != .false);
    //
    //     // Our link configs
    //     const links = try link.Set.fromConfig(
    //         alloc,
    //         config.link.links.items,
    //     );
    //
    //     return .{
    //         .background_opacity = @max(0, @min(1, config.@"background-opacity")),
    //         .background_opacity_cells = config.@"background-opacity-cells",
    //         .font_thicken = config.@"font-thicken",
    //         .font_thicken_strength = config.@"font-thicken-strength",
    //         .font_features = font_features.list,
    //         .font_styles = font_styles,
    //         .font_shaping_break = config.@"font-shaping-break",
    //
    //         .cursor_color = config.@"cursor-color",
    //         .cursor_text = config.@"cursor-text",
    //         .cursor_opacity = @max(0, @min(1, config.@"cursor-opacity")),
    //
    //         .background = config.background.toTerminalRGB(),
    //         .foreground = config.foreground.toTerminalRGB(),
    //         .bold_color = if (config.@"bold-color") |b| b.toTerminal() else null,
    //         .faint_opacity = @intFromFloat(@ceil(config.@"faint-opacity" * 255)),
    //
    //         .min_contrast = @floatCast(config.@"minimum-contrast"),
    //         .padding_color = config.@"window-padding-color",
    //
    //         .selection_background = config.@"selection-background",
    //         .selection_foreground = config.@"selection-foreground",
    //         .search_background = config.@"search-background",
    //         .search_foreground = config.@"search-foreground",
    //         .search_selected_background = config.@"search-selected-background",
    //         .search_selected_foreground = config.@"search-selected-foreground",
    //
    //         .custom_shaders = custom_shaders,
    //         .bg_image = bg_image,
    //         .bg_image_opacity = config.@"background-image-opacity",
    //         .bg_image_position = config.@"background-image-position",
    //         .bg_image_fit = config.@"background-image-fit",
    //         .bg_image_repeat = config.@"background-image-repeat",
    //         .links = links,
    //         .vsync = config.@"window-vsync",
    //         .colorspace = config.@"window-colorspace",
    //         .blending = config.@"alpha-blending",
    //         .background_blur = config.@"background-blur",
    //         .scroll_to_bottom_on_output = config.@"scroll-to-bottom".output,
    //         .arena = arena,
    //     };
    // }
    #[allow(clippy::extra_unused_type_parameters)]
    pub fn init<C>(alloc_gpa: Global, config: &C) -> Result<DerivedConfig, DerivedConfigError> {
        let _ = (alloc_gpa, config);
        unimplemented!(
            "renderer.Renderer.DerivedConfig.init deferred (config-coupled placeholder; \
             the generic `C` binds `rustty_config::Config` in core); verbatim body above"
        )
    }

    // pub fn deinit(self: *DerivedConfig) void {
    //     const alloc = self.arena.allocator();
    //     self.links.deinit(alloc);
    //     self.arena.deinit();
    // }
    //
    // PORT NOTE: `link::Set` (link.rs) is allocated over `Global` (its landed
    // `Allocator = Global` decision), so `links.deinit` takes `Global`; the
    // reference `self.arena.deinit()` maps to the `ArenaAllocator` `Drop` impl,
    // which runs when the caller drops `self`.
    pub fn deinit(&mut self) {
        self.links.deinit(Global);
    }
}

// ===========================================================================
// Options (renderer/Options.zig, minus the apprt-typed fields)
// ===========================================================================

/// The options that are used to configure a renderer.
//
// PORT NOTE (ports `renderer/Options.zig`): three reference fields are
// apprt-typed and DEFERRED with the surface-mailbox seam (GPU_RD_CHUNK3A_DESIGN
// §B.2) — `surface_mailbox: apprt.surface.Mailbox`, `rt_surface: *apprt.Surface`,
// `thread: *renderer.Thread`. `rustty-apprt-api -> rustty-renderer`
// (apprt-api/Cargo.toml), so naming any apprt type here would CYCLE
// (check_acyclic). They land with `drawFrame`/`Renderer::init` routed through the
// renderer-local mailbox trait. The remaining three fields are platform-neutral.
pub struct Options {
    /// The derived configuration for this renderer implementation.
    pub config: DerivedConfig,

    /// The font grid that should be used along with the key for deref-ing.
    //
    // Zig `font_grid: *font.SharedGrid` -> `*mut SharedGrid` (the raw-pointer
    // handle to the shared grid; the `message.rs` `FontGrid.grid` precedent).
    pub font_grid: *mut SharedGrid,

    /// The size of everything.
    pub size: Size,
    // DEFERRED (surface-mailbox seam §B.2):
    //   /// The mailbox for sending the surface messages. ...
    //   surface_mailbox: apprt.surface.Mailbox,
    //   /// The apprt surface.
    //   rt_surface: *apprt.Surface,
    //   /// The renderer thread.
    //   thread: *renderer.Thread,
}

// ===========================================================================
// AtomicHealth (port of std.atomic.Value(Health))
// ===========================================================================

/// Atomic wrapper over the renderer [`Health`], the `health` field's type.
//
// PORT NOTE: reference `health: std.atomic.Value(Health) = .{ .raw = .healthy }`.
// `Health` is `enum(c_int)` (crate root, `#[repr(i32)]`), so `std.atomic.Value`
// is modeled as an `AtomicI32` holding the discriminant (matching the `c_int`
// backing). `drawFrame` (deferred) does `self.health.load(.seq_cst)` /
// `.store(x, .seq_cst)` (generic.zig:1730-1731) -> `Ordering::SeqCst`.
#[derive(Debug)]
pub struct AtomicHealth {
    raw: AtomicI32,
}

impl AtomicHealth {
    /// Zig `.{ .raw = .healthy }` — a value initialized to the given health.
    pub const fn new(health: Health) -> AtomicHealth {
        AtomicHealth {
            raw: AtomicI32::new(health as i32),
        }
    }

    /// Zig `self.health.load(.seq_cst)`.
    pub fn load(&self) -> Health {
        // `Health` discriminants: `Healthy = 0`, `Unhealthy = 1` (the only two).
        match self.raw.load(Ordering::SeqCst) {
            0 => Health::Healthy,
            _ => Health::Unhealthy,
        }
    }

    /// Zig `self.health.store(health, .seq_cst)`.
    pub fn store(&self, health: Health) {
        self.raw.store(health as i32, Ordering::SeqCst);
    }
}

// ===========================================================================
// The monomorphic GPU renderer + its swap chain (macOS / Metal).
// ===========================================================================
//
// These types name the concrete `Metal` leaf types, so they are macOS-gated
// (like `metal`/`image`/`overlay`). Grouped in one `#[cfg]` module (with a glob
// re-export) so the cfg boundary + the macOS-only imports live in one place.
#[cfg(target_os = "macos")]
mod macos_impl {
    use core::mem;
    use std::time::Instant;

    use allocator_api2::alloc::Global;
    use rustty_font::atlas::{Atlas, Format, OutOfMemory};
    use rustty_font_shaper::{Metrics, Shaper, ShaperCache, SharedGrid};
    use rustty_vt::render::RenderState;
    use rustty_vt::size::CellCountInt;
    use rustty_vt::Scrollbar;
    use rustty_zigstd::thread::{Mutex, Semaphore};

    use super::{AtomicHealth, DerivedConfig, Options};
    use crate::cell::Contents;
    use crate::image;
    use crate::message::{SearchMatch, SearchMatches};
    use crate::metal::buffer::{self, Buffer};
    use crate::metal::sampler::{self, Sampler};
    use crate::metal::shaders::Shaders;
    use crate::metal::target::{self, Target};
    use crate::metal::texture::{self, Texture};
    use crate::metal::Metal;
    use crate::shaders::{BgImage, CellBg, CellText, Uniforms};
    use crate::shadertoy;
    use crate::size::Size;
    use crate::Overlay;

    /// The number of buffers we use for double/triple buffering.
    //
    // PORT NOTE: reference `SwapChain.buf_count = GraphicsAPI.swap_chain_count`.
    // Monomorphized to the concrete `Metal::SWAP_CHAIN_COUNT` (= 3, triple
    // buffering). Used as the `[FrameState; SWAP_CHAIN_COUNT]` array length + the
    // `frame_sema` permit count (GPU_RD_CHUNK3A_DESIGN §B.5).
    const SWAP_CHAIN_COUNT: usize = Metal::SWAP_CHAIN_COUNT;

    /// PORT: the aggregate renderer error. Reference `generic.zig` methods return
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

    /// Returned by [`SwapChain::next_frame`] when the swap chain is defunct.
    //
    // PORT NOTE: reference `error{Defunct}`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Defunct;

    /// PORT NOTE (DEFERRED SEAM — surface-mailbox): stands in for the Renderer's
    /// `surface_mailbox: apprt.surface.Mailbox` field. `apprt::surface::Mailbox`
    /// does not exist in rustty (apprt-api replaced the reference struct with the
    /// `AppMailbox` trait, apprt-api/src/surface.rs), and `rustty-apprt-api ->
    /// rustty-renderer` (apprt-api/Cargo.toml) so a `renderer -> apprt-api` edge
    /// would CYCLE (check_acyclic). `generic.zig` pushes exactly two messages onto
    /// it — `.scrollbar` (:1454) and `.renderer_health` (:1735), both inside the
    /// deferred `drawFrame`. Until `drawFrame` lands the field is an inhabited,
    /// default-constructible placeholder; then it routes through a renderer-local
    /// mailbox trait bound to `AppMailbox` in core (GPU_RD_CHUNK3A_DESIGN §B.2).
    #[derive(Clone, Copy, Debug, Default)]
    pub struct SurfaceMailbox;

    /// PORT NOTE (DEFERRED SEAM — macOS display link): reference
    /// `const DisplayLink = switch (builtin.os.tag) { .macos => *macos.video.DisplayLink,
    /// else => void }`, field `display_link: ?DisplayLink = null`. The CVDisplayLink
    /// loop driver (`createWithActiveCGDisplays`/`stop`/`release`) lands with
    /// `loopEnter`; this is an inhabited macOS placeholder for the field type.
    #[derive(Debug)]
    pub struct DisplayLink {
        _seam: (),
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
        pub draw_mutex: Mutex,

        /// The configuration we need derived from the main config.
        pub config: DerivedConfig,

        /// The mailbox for communicating with the window.
        pub surface_mailbox: SurfaceMailbox,

        /// Current font metrics defining our grid.
        pub grid_metrics: Metrics,

        /// The size of everything.
        pub size: Size,

        /// True if the window is focused
        pub focused: bool,

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
        pub cells_rebuilt: bool,

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
        pub health: AtomicHealth,

        /// Our swap chain (multiple buffering)
        pub swap_chain: SwapChain,

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
        pub frame_sema: Semaphore,

        /// Set to true when deinited, if you try to deinit a defunct
        /// swap chain it will just be ignored, to prevent double-free.
        ///
        /// This is required because of `displayUnrealized`, since it
        /// `deinits` the swapchain, which leads to a double-free if
        /// the renderer is deinited after that.
        pub defunct: bool,
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
                frame_sema: Semaphore::with_permits(SWAP_CHAIN_COUNT),
                defunct: false,
            })
        }

        pub fn deinit(&mut self) {
            // Zig: if (self.defunct) return;
            //      self.defunct = true;
            if self.defunct {
                return;
            }
            self.defunct = true;

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
        pub fn next_frame(&mut self) -> Result<&mut FrameState, Defunct> {
            // Zig: if (self.defunct) return error.Defunct;
            if self.defunct {
                return Err(Defunct);
            }

            // Zig: self.frame_sema.wait();
            //      errdefer self.frame_sema.post();
            self.frame_sema.wait();
            // (The Zig `errdefer post()` guards the index update + slice index
            // below; both are infallible in Rust — there is no error path between
            // the `wait` and the return — so the errdefer never fires.)
            self.frame_index = (self.frame_index + 1) % (SWAP_CHAIN_COUNT as u8);
            Ok(&mut self.frames[self.frame_index as usize])
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
        pub uniforms: Buffer<Uniforms>,
        pub cells: Buffer<CellText>,
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
        pub bg_image_buffer: Buffer<BgImage>,
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
            let uniforms = Buffer::<Uniforms>::init(api.uniform_buffer_options(), 1)?;

            // Create GPU buffers for our cells.
            //
            // We start them off with a size of 1, which will of course be
            // too small, but they will be resized as needed. This is a bit
            // wasteful but since it's a one-time thing it's not really a
            // huge concern.
            let cells = Buffer::<CellText>::init(api.fg_buffer_options(), 1)?;
            let cells_bg = Buffer::<CellBg>::init(api.bg_buffer_options(), 1)?;

            // Create a GPU buffer for our background image info.
            let bg_image_buffer = Buffer::<BgImage>::init(api.bg_image_buffer_options(), 1)?;

            // Initialize our textures for our font atlas.
            //
            // As with the buffers above, we start these off as small
            // as possible since they'll inevitably be resized anyway.
            //
            // PORT NOTE: Zig passes a throwaway `*const font.Atlas`
            // (`&.{ .data = undefined, .size = 1, .format = .grayscale }`) —
            // `initAtlasTexture` reads only `.size`/`.format`. rustty's `Atlas` has
            // private fields (only `Atlas::init(alloc, size, format)` is public), so
            // the throwaway is realized as a real 1x1 atlas over `Global`; the
            // transient 1-byte allocation is immaterial (the texture takes only
            // size=1/format) and frees when the temp `Atlas` drops.
            let grayscale_atlas = Atlas::init(Global, 1, Format::Grayscale)?;
            let grayscale = api.init_atlas_texture(&grayscale_atlas)?;
            let color_atlas = Atlas::init(Global, 1, Format::Bgra)?;
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

    impl Renderer {
        // DEFERRED SEAM (the draw path — a later chunk). `Renderer::init` needs
        // `GraphicsAPI::init(options)` over a LIVE surface (the apprt/windowing-host
        // seam — `Metal::init` is likewise deferred, `metal_impl.rs`), reads
        // `options.config.custom_shaders` (a deferred `RepeatablePath` placeholder,
        // §B.1), constructs `font.Shaper.init`, and calls the deferred CPU methods
        // (`initShaders`/`updateFontGridUniforms`/`updateScreenSizeUniforms`/
        // `updateBgImageBuffer`/`prepBackgroundImage`). The reference body is
        // preserved verbatim (R1/R5); it lands — un-commented — with those methods.
        //
        // pub fn init(alloc: Allocator, options: renderer.Options) !Self {
        //     // Initialize our graphics API wrapper, this will prepare the
        //     // surface provided by the apprt and set up any API-specific
        //     // GPU resources.
        //     var api = try GraphicsAPI.init(alloc, options);
        //     errdefer api.deinit();
        //
        //     const has_custom_shaders = options.config.custom_shaders.value.items.len > 0;
        //
        //     // Prepare our swap chain
        //     var swap_chain = try SwapChain.init(
        //         api,
        //         has_custom_shaders,
        //     );
        //     errdefer swap_chain.deinit();
        //
        //     // Create the font shaper.
        //     var font_shaper = try font.Shaper.init(alloc, .{
        //         .features = options.config.font_features.items,
        //     });
        //     errdefer font_shaper.deinit();
        //
        //     // Initialize all the data that requires a critical font section.
        //     const font_critical: struct {
        //         metrics: font.Metrics,
        //     } = font_critical: {
        //         const grid: *font.SharedGrid = options.font_grid;
        //         grid.lock.lockShared();
        //         defer grid.lock.unlockShared();
        //         break :font_critical .{
        //             .metrics = grid.metrics,
        //         };
        //     };
        //
        //     const display_link: ?DisplayLink = switch (builtin.os.tag) {
        //         .macos => if (options.config.vsync)
        //             try macos.video.DisplayLink.createWithActiveCGDisplays()
        //         else
        //             null,
        //         else => null,
        //     };
        //     errdefer if (display_link) |v| v.release();
        //
        //     var result: Self = .{
        //         .alloc = alloc,
        //         .config = options.config,
        //         .surface_mailbox = options.surface_mailbox,
        //         .grid_metrics = font_critical.metrics,
        //         .size = options.size,
        //         .focused = true,
        //         .scrollbar = .zero,
        //         .scrollbar_dirty = false,
        //         .last_bottom_node = null,
        //         .last_bottom_y = 0,
        //         .search_matches = null,
        //         .search_selected_match = null,
        //         .search_matches_dirty = false,
        //
        //         // Render state
        //         .cells = .{},
        //         .uniforms = .{
        //             .projection_matrix = undefined,
        //             .cell_size = undefined,
        //             .grid_size = undefined,
        //             .grid_padding = undefined,
        //             .screen_size = undefined,
        //             .padding_extend = .{},
        //             .min_contrast = options.config.min_contrast,
        //             .cursor_pos = .{ std.math.maxInt(u16), std.math.maxInt(u16) },
        //             .cursor_color = undefined,
        //             .bg_color = .{
        //                 options.config.background.r,
        //                 options.config.background.g,
        //                 options.config.background.b,
        //                 // Note that if we're on macOS with glass effects
        //                 // we'll disable background opacity but we handle
        //                 // that in updateFrame.
        //                 @intFromFloat(@round(options.config.background_opacity * 255.0)),
        //             },
        //             .bools = .{
        //                 .cursor_wide = false,
        //                 .use_display_p3 = options.config.colorspace == .@"display-p3",
        //                 .use_linear_blending = options.config.blending.isLinear(),
        //                 .use_linear_correction = options.config.blending == .@"linear-corrected",
        //             },
        //         },
        //         .custom_shader_uniforms = .{
        //             .resolution = .{ 0, 0, 1 },
        //             .time = 0,
        //             .time_delta = 0,
        //             .frame_rate = 60, // not currently updated
        //             .frame = 0,
        //             .channel_time = @splat(@splat(0)), // not currently updated
        //             .channel_resolution = @splat(@splat(0)),
        //             .mouse = @splat(0), // not currently updated
        //             .date = @splat(0), // not currently updated
        //             .sample_rate = 0, // N/A, we don't have any audio
        //             .current_cursor = @splat(0),
        //             .previous_cursor = @splat(0),
        //             .current_cursor_color = @splat(0),
        //             .previous_cursor_color = @splat(0),
        //             .current_cursor_style = 0,
        //             .previous_cursor_style = 0,
        //             .cursor_visible = 0,
        //             .cursor_change_time = 0,
        //             .time_focus = 0,
        //             .focus = 1, // assume focused initially
        //             .palette = @splat(@splat(0)),
        //             .background_color = @splat(0),
        //             .foreground_color = @splat(0),
        //             .cursor_color = @splat(0),
        //             .cursor_text = @splat(0),
        //             .selection_background_color = @splat(0),
        //             .selection_foreground_color = @splat(0),
        //         },
        //         .bg_image_buffer = undefined,
        //
        //         // Fonts
        //         .font_grid = options.font_grid,
        //         .font_shaper = font_shaper,
        //         .font_shaper_cache = font.ShaperCache.init(),
        //
        //         // Shaders (initialized below)
        //         .shaders = undefined,
        //
        //         // Graphics API stuff
        //         .api = api,
        //         .swap_chain = swap_chain,
        //         .display_link = display_link,
        //     };
        //
        //     try result.initShaders();
        //
        //     // Ensure our undefined values above are correctly initialized.
        //     result.updateFontGridUniforms();
        //     result.updateScreenSizeUniforms();
        //     result.updateBgImageBuffer();
        //     try result.prepBackgroundImage();
        //
        //     return result;
        // }
        pub fn init(alloc: Global, options: Options) -> Result<Self, Error> {
            let _ = (alloc, options);
            unimplemented!(
                "renderer.Renderer.init deferred (Chunk 3a is the struct + swap-chain/\
                 frame-state skeleton). The full init — GraphicsAPI::init over a live \
                 surface, font.Shaper.init, initShaders, the uniform setup + \
                 updateFontGridUniforms/updateScreenSizeUniforms/updateBgImageBuffer/\
                 prepBackgroundImage — lands with the draw path; verbatim body above"
            )
        }

        // DEFERRED SEAM (the draw path). `deinit` tears down the deferred draw-path
        // state (`terminal_state`/`images`/`shaders`/`font_shaper` etc.) and calls
        // the not-yet-declared `deinitShaders`; the reference body is preserved
        // verbatim (R1/R5) and lands with those methods.
        //
        // pub fn deinit(self: *Self) void {
        //     if (self.overlay) |*overlay| overlay.deinit(self.alloc);
        //     self.terminal_state.deinit(self.alloc);
        //     if (self.search_selected_match) |*m| m.arena.deinit();
        //     if (self.search_matches) |*m| m.arena.deinit();
        //     self.swap_chain.deinit();
        //
        //     if (DisplayLink != void) {
        //         if (self.display_link) |display_link| {
        //             display_link.stop() catch {};
        //             display_link.release();
        //         }
        //     }
        //
        //     self.cells.deinit(self.alloc);
        //
        //     self.font_shaper.deinit();
        //     self.font_shaper_cache.deinit(self.alloc);
        //
        //     self.config.deinit();
        //
        //     self.images.deinit(self.alloc);
        //
        //     if (self.bg_image) |img| img.deinit(self.alloc);
        //
        //     self.deinitShaders();
        //
        //     self.api.deinit();
        //
        //     self.* = undefined;
        // }
        pub fn deinit(&mut self) {
            unimplemented!(
                "renderer.Renderer.deinit deferred (verbatim body above; the draw-path \
                 teardown — deinitShaders/api.deinit/font_shaper.deinit/terminal_state \
                 etc. — lands with those methods)"
            )
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos_impl::*;
