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
//!   precedent, `thread.rs`); the 10 non-hoistable fields (`TerminalColor`×8,
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
// DerivedConfig config-type placeholders (§B.1 — the 10 non-hoistable fields)
// ===========================================================================

/// PORTS: `configpkg.Config.Color` (Config.zig:5413). Renderer-LOCAL (not hoisted
/// to render-types) so it can name `rustty_vt::color::RGB` — the renderer already
/// deps vt, so this creates NO cycle; hoisting to render-types WOULD cycle
/// (render-types has no vt edge, and vt->render-types exists).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    // pub fn toTerminalRGB(self: Color) terminal.color.RGB {
    //     return .{ .r = self.r, .g = self.g, .b = self.b };
    // }
    pub fn to_terminal_rgb(self) -> RGB {
        RGB {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }
}

/// PORTS: `configpkg.Config.TerminalColor` (Config.zig:5549), a
/// `union(enum) { color: Color, @"cell-foreground", @"cell-background" }`.
/// Renderer-local (see [`Color`]) so it can name `rustty_vt::color::RGB` without
/// cycling (§E). The Chunk-3b cell-fill destructures this directly on the
/// selection/search/cursor color paths (`rebuild_row`/`rebuild_cells`), so it is
/// promoted from the earlier unit-struct placeholder to the real value-carrying
/// enum NOW. `DerivedConfig::init<C>` (deferred, 3c) converts from
/// `rustty_config::Config`'s own `TerminalColor` via a `From` bound to the
/// concrete `C` in core (the generic-`C` seam); the reference config-parsing
/// surface (`TerminalColor.toTerminalRGB -> ?RGB`, `parseCLI`/`formatEntry` + its
/// `test{}`s) is NOT on the fill path and lands with `init<C>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalColor {
    Color(Color),
    CellForeground,
    CellBackground,
}

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
// until the draw path reads them). The `TerminalColor` config-typed fields now use
// the REAL renderer-local `TerminalColor` enum (promoted in Chunk 3b — the cell
// fill destructures it, GPU_RD_CHUNK3B_DESIGN §E); the two `Path`/`RepeatablePath`
// fields (`bg_image`/`custom_shaders`) still use the inhabited placeholders above
// (§B.1) because the fill never reads them.
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
    //   /// The mailbox for sending the surface messages. This is only valid
    //   /// once the thread has started and should not be used outside of the thread.
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
    use rustty_zigstd::thread::{Mutex, Semaphore};

    use super::{AtomicHealth, DerivedConfig, Options, TerminalColor};
    use crate::cell::{self, Contents, Key};
    use crate::cursor;
    use crate::image;
    use crate::message::{SearchMatch, SearchMatches};
    use crate::metal::buffer::{self, Buffer};
    use crate::metal::sampler::{self, Sampler};
    use crate::metal::shaders::Shaders;
    use crate::metal::target::{self, Target};
    use crate::metal::texture::{self, Texture};
    use crate::metal::Metal;
    use crate::row::never_extend_bg;
    use crate::shaders::{
        Atlas, BgImage, BgImageInfo, BgImageInfoFit, BgImageInfoPosition, CellBg, CellText,
        CellTextBools, Uniforms, UniformsPaddingExtend,
    };
    use crate::shadertoy;
    use crate::size::{
        BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, CellSize, GridSize, Padding,
        ScreenSize, Size,
    };
    use crate::state;
    use crate::Overlay;

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
    const CUSTOM_SHADER_Y_IS_DOWN: bool = Metal::CUSTOM_SHADER_Y_IS_DOWN;

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

    // =======================================================================
    // Chunk 3b — the CPU cell-fill (GPU_RD_CHUNK3B_DESIGN)
    // =======================================================================

    // const PreeditRange = struct {
    //     y: terminal.size.CellCountInt,
    //     x: [2]terminal.size.CellCountInt,
    //     cp_offset: usize,
    // };
    #[derive(Clone, Copy)]
    struct PreeditRange {
        y: CellCountInt,
        x: [CellCountInt; 2],
        cp_offset: usize,
    }

    /// The per-cell selection precedence used by [`Renderer::rebuild_row`].
    // PORT NOTE: the reference declares this as an anonymous enum
    // `const selected: enum { false, selection, search, search_selected }` inline in
    // the loop (ref :2752-2757). Rust has no anonymous enums, so it is hoisted to a
    // module-private named enum (the `False` variant renames the Zig `.false`). The
    // `SearchSelected` variant faithfully mirrors the reference `.search_selected`
    // (which happens to end with the enum name) — `#[allow(enum_variant_names)]`.
    #[derive(Clone, Copy, PartialEq, Eq)]
    #[allow(clippy::enum_variant_names)]
    enum Selected {
        False,
        Selection,
        Search,
        SearchSelected,
    }

    /// PORT: the un-caught error sources of `rebuild_row` (the reference `!void`).
    /// The only `try`s that are NOT `catch`-logged inline are `run_iter.next` (a
    /// `collection::Error`) and `font_shaper.shape` (the active shaper's `Error`,
    /// `coretext` on macOS). `rebuild_cells` catches this → `clear(y)` + log. (The
    /// `add_*` calls are `catch`-logged inline, mapping to `.unwrap_or_else`.)
    //
    // PORT NOTE: the wrapped payloads carry the faithful error value the reference
    // logs (`log.warn("... err={}", .{ err })` in `rebuildCells`, ref :2440). rustty
    // has no logging facade wired yet, so those `log.warn` sites are preserved as
    // comments and the payload is currently unread — `#[allow(dead_code)]` until the
    // logging facade lands, rather than dropping the faithful error data.
    #[derive(Debug)]
    #[allow(dead_code)]
    enum RebuildRowError {
        Collection(collection::Error),
        Shape(shape::coretext::Error),
    }

    impl From<collection::Error> for RebuildRowError {
        fn from(e: collection::Error) -> Self {
            RebuildRowError::Collection(e)
        }
    }
    impl From<shape::coretext::Error> for RebuildRowError {
        fn from(e: shape::coretext::Error) -> Self {
            RebuildRowError::Shape(e)
        }
    }

    impl Renderer {
        /// Convert the terminal state to GPU cells stored in CPU memory. These
        /// are then synced to the GPU in the next frame. This only updates CPU
        /// memory and doesn't touch the GPU.
        ///
        /// This requires the draw mutex.
        ///
        /// Dirty state on terminal state won't be reset by this.
        //
        // PORT NOTE: reference `fn rebuildCells(self, preedit, cursor_style_, links)
        // Allocator.Error!void`. The sole own error source (`self.cells.resize`)
        // aborts on OOM in rustty (`cell.rs::resize`'s Global-allocator PORT NOTE)
        // and the `rebuild_row` call is `catch`-logged inline, so this is infallible
        // → `()` (NOT a vacuous `Result`). WIDENED to `pub` for dead-code exemption
        // under `-D warnings` — reachable from the crate root; its real caller
        // (`updateFrame`) lands in Chunk 3c (GPU_RD_CHUNK3B_DESIGN §F).
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
                    let cc = &self.terminal_state.row_data[cvy as usize].cells[cvx as usize];
                    if cc.raw.has_styling() {
                        cc.style
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
                let cells_ptr: *const [render::Cell] =
                    self.terminal_state.row_data[y_usize].cells.as_slice();
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
                            TerminalColor::Color(color) => {
                                break 'cursor_color color.to_terminal_rgb()
                            }

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
                let cursor_ptr: *const render::Cursor =
                    unsafe { &(*self_raw).terminal_state.cursor };
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
            cells: *const [render::Cell],
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
            // (`cells_slice` is HEAP-backed — survives `&mut *self`; §C.3(2).)
            let cells_slice: &[render::Cell] = unsafe { &*cells };
            let cells_len = core::cmp::min(cells_slice.len(), self.cells.size.columns as usize);
            // PORT NOTE: the reference reads two SoA projections `cells.items(.raw)` /
            // `.items(.style)`. rustty's cells are AoS, so `cells_raw` is a fresh
            // row-local `Vec<page::Cell>` (the `cell.rs::raw_cells` precedent) — it does
            // NOT alias `self.terminal_state`, so it is a plain owned local (§D.5), and
            // `cells_slice[x].style` is the `.items(.style)` projection read inline.
            let cells_raw: Vec<page::Cell> = cells_slice.iter().map(|c| c.raw).collect();

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
                            !never_extend_bg(row, cells_slice, &palette, background);
                    } else if y == self.cells.size.rows - 1 {
                        self.uniforms.padding_extend.down =
                            !never_extend_bg(row, cells_slice, &palette, background);
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
                let managed_style: Style = cells_slice[x].style;

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
                        while (run.offset as usize
                            + shaper_cells_unwrapped[shaper_cells_i].x as usize)
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
                if shaper_cells.is_some()
                    && shaper_cells_i >= unsafe { &*shaper_cells.unwrap() }.len()
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
                                    &cells_raw,
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
            // PORT NOTE (§D.5): a plain safe `&[page::Cell]` — the AoS `cells_raw`
            // projection is a fresh row-local `Vec` in `rebuild_row` (you cannot
            // reinterpret `&[render::Cell]` as `&[page::Cell]`), so it does NOT alias
            // `self.terminal_state` and is NOT one of the §C.3 raw-pointer triad.
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

        // fn rebuildOverlay(self, features: []const Overlay.Feature) Overlay.InitError!void
        //
        // WIDENED to `pub` for dead-code exemption (its only non-test caller,
        // `updateFrame`, is deferred to 3c; GPU_RD_CHUNK3B_DESIGN §F).
        pub fn rebuild_overlay(
            &mut self,
            features: &[crate::overlay::Feature],
        ) -> Result<(), crate::overlay::InitError> {
            // const start = std.time.Instant.now() catch unreachable;
            // const start_micro = std.time.microTimestamp();
            // defer {
            //     const end = std.time.Instant.now() catch unreachable;
            //     log.warn(
            //         "[rebuildOverlay time] start_micro={} duration={}ns",
            //         .{ start_micro, end.since(start) / std.time.ns_per_us },
            //     );
            // }

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
    }

    // =======================================================================
    // Chunk 3c — the frame orchestrator + uniforms math (GPU_RD_CHUNK3C_DESIGN)
    // =======================================================================

    impl Renderer {
        /// Update the frame data.
        //
        // Zig: pub fn updateFrame(self, state: *renderer.State, cursor_blink_visible: bool)
        //      Allocator.Error!void
        // WIDENED to `pub` (dead-code-exempt entry point; also the crate's frame driver).
        // It is the SOLE caller of the already-ported `rebuild_cells` (:1364) /
        // `rebuild_overlay` (:1349), retiring their 3b "real caller lands in 3c"
        // placeholders. `state: *renderer.State` -> `&state::State` (Chunk 3c §B.2):
        // `update_frame` never mutates a `State` *field* — it only locks `state.mutex`
        // (`lock(&self)`) and mutates THROUGH the stored `*mut Terminal` (a disjoint
        // allocation), so a shared `&State` suffices. The ONLY propagated `?` is
        // `terminal_state.update`, whose error is `VtOutOfMemory`
        // (`rustty_vt::pagelist::OutOfMemory`, aliased — see the import PORT NOTE); Zig
        // `Allocator.Error!void` -> `Result<(), VtOutOfMemory>`.
        pub fn update_frame(
            &mut self,
            state: &state::State,
            cursor_blink_visible: bool,
        ) -> Result<(), VtOutOfMemory> {
            // const start = std.time.Instant.now() catch unreachable;
            // const start_micro = std.time.microTimestamp();
            // defer {
            //     const end = std.time.Instant.now() catch unreachable;
            //     log.warn(
            //         "[updateFrame time] start_micro={} duration={}ns",
            //         .{ start_micro, end.since(start) / std.time.ns_per_us },
            //     );
            // }

            // We fully deinit and reset the terminal state every so often
            // so that a particularly large terminal state doesn't cause
            // the renderer to hold on to retained memory.
            //
            // Frame count is ~12 minutes at 120Hz.
            let max_terminal_state_frame_count = 100_000;
            if self.terminal_state_frame_count >= max_terminal_state_frame_count {
                self.terminal_state.deinit(self.alloc);
                self.terminal_state = RenderState::empty();
            }
            self.terminal_state_frame_count += 1;

            // Create an arena for all our temporary allocations while rebuilding
            //
            // PORT NOTE (Chunk 3c §D.6): the reference opens `var arena =
            // ArenaAllocator.init(self.alloc)` and threads `arena_alloc` into
            // `preedit.clone`, `link_cells`, and `render_cell_map`. In rustty every one
            // of those owns its result as a `Global` `Vec`/`CellSet` freed by `Drop`
            // (the arena-collapse precedent — message.rs `SearchMatches`), so the
            // explicit `ArenaAllocator` is omitted and `Global` is passed where the
            // reference passes `arena_alloc` (nothing leaks — the former arena tenants
            // are now `Drop`-freed).
            // var arena = ArenaAllocator.init(self.alloc);
            // defer arena.deinit();
            // const arena_alloc = arena.allocator();

            // Data we extract out of the critical area.
            struct Critical {
                links: CellSet,
                // §C.5: written (`:1275 .mouse = state.mouse`) but never READ after the
                // block in the reference (post-block code reads `state.mouse.point/.mods`
                // directly, :1289/1290). Mirrored (R1) as a faithful write-only field.
                #[allow(dead_code)]
                mouse: state::Mouse,
                preedit: Option<state::Preedit>,
                scrollbar: Scrollbar,
                overlay_features: Vec<crate::overlay::Feature>,
            }

            // The renderer-thread state carries its terminal behind a `*mut Terminal`
            // (a separate allocation, disjoint from `self`). Copy the stored raw
            // pointer and re-deref it FRESH at every use (never cache a reference
            // across an interleaving `&mut`), matching the Zig `state.terminal.…`
            // re-walk (Chunk 3c §C.2).
            let t = state.terminal; // *mut Terminal

            // Hand-rolled `defer state.mutex.unlock()` (Chunk 3c §C.4/RISK-5): the
            // guard-less `rustty_zigstd::thread::Mutex` has no RAII drop, and the lock
            // is held across THREE exits — the sync-output early `return`, the
            // `terminal_state.update(...)?` propagation (the sole propagating `?`,
            // INSIDE the lock), and normal fall-through. This `Drop` over `&Mutex`
            // fires on all three. Sound: it borrows the mutex allocation behind
            // `state.mutex`, disjoint from `self`, so it does not clash with the
            // `&mut self.terminal_state` / `&mut *t` uses inside the block.
            struct Unlock<'a>(&'a Mutex);
            impl Drop for Unlock<'_> {
                fn drop(&mut self) {
                    self.0.unlock();
                }
            }

            // Update all our data as tightly as possible within the mutex.
            let mut critical: Critical = {
                // const start = try std.time.Instant.now();
                // const start_micro = std.time.microTimestamp();
                // defer {
                //     const end = std.time.Instant.now() catch unreachable;
                //     std.log.err("[updateFrame critical time] start={}\tduration={} us", .{ start_micro, end.since(start) / std.time.ns_per_us });
                // }

                unsafe {
                    (*state.mutex).lock();
                }
                let _mutex_guard = Unlock(unsafe { &*state.mutex });

                // If we're in a synchronized output state, we pause all rendering.
                if unsafe { (*t).modes.get(Mode::SynchronizedOutput) } {
                    // log.debug("synchronized output started, skipping render", .{});
                    return Ok(());
                }

                // If scroll-to-bottom on output is enabled, check if the final line
                // changed by comparing the bottom-right pin. If the node pointer or
                // y offset changed, new content was added to the screen.
                // Update this BEFORE we update our render state so we can
                // draw the new scrolled data immediately.
                if self.config.scroll_to_bottom_on_output {
                    'scroll: {
                        let active = unsafe { (*t).screens.active };
                        let br =
                            match unsafe { (*active).pages.get_bottom_right(point::Tag::Screen) } {
                                Some(br) => br,
                                None => break 'scroll,
                            };

                        // If the pin hasn't changed, then don't scroll.
                        if self.last_bottom_node == Some(br.node as usize)
                            && self.last_bottom_y == br.y
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

                // If our terminal state is dirty at all we need to redo
                // the viewport search.
                if self.terminal_state.dirty != Dirty::False {
                    unsafe {
                        (*t).flags.search_viewport_dirty = true;
                    }
                }

                // Get our scrollbar out of the terminal. We synchronize
                // the scrollbar read with frame data updates because this
                // naturally limits the number of calls to this method (it
                // can be expensive) and also makes it so we don't need another
                // cross-thread mailbox message within the IO path.
                //
                // (§C.1-#9: re-read `active` FRESH — do NOT reuse the scroll block's
                // `active` across the `&mut *t` update above.)
                let active = unsafe { (*t).screens.active };
                let scrollbar = unsafe { (*active).pages.scrollbar() };

                // Get our preedit state
                let preedit: Option<state::Preedit> = state.preedit.as_ref().map(|p| p.clone());

                // If we have Kitty graphics data, we enter a SLOW SLOW SLOW path.
                // We only do this if the Kitty image state is dirty meaning only if
                // it changes.
                //
                // If we have any virtual references, we must also rebuild our
                // kitty state on every frame because any cell change can move
                // an image.
                if self.images.kitty_requires_update(unsafe { &*t }) {
                    // We need to grab the draw mutex since this updates
                    // our image state that drawFrame uses.
                    self.draw_mutex.lock();
                    self.images.kitty_update(
                        self.alloc,
                        unsafe { &*t },
                        CellSize {
                            width: self.grid_metrics.cell_width,
                            height: self.grid_metrics.cell_height,
                        },
                    );
                    self.draw_mutex.unlock();
                }

                // Get our OSC8 links we're hovering if we have a mouse.
                // This requires terminal state because of URLs.
                let links: CellSet = match state.mouse.point {
                    // If our mouse isn't hovering, we have no links.
                    None => CellSet::new(),
                    Some(vp) => {
                        // If the right mods aren't pressed, then we can't match.
                        if !state.mouse.mods.equal(key::ctrl_or_super(Mods::default())) {
                            CellSet::new()
                        } else {
                            self.terminal_state
                                .link_cells(Global, vp)
                                .unwrap_or_else(|_err| {
                                    // log.warn("error searching for OSC8 links err={}", .{err});
                                    CellSet::new()
                                })
                        }
                    }
                };

                let overlay_features: Vec<crate::overlay::Feature> = match state.inspector {
                    None => Vec::new(),
                    // DEFERRED R5 SEAM (§C.7): `state.inspector` is the uninhabited
                    // `enum {}` placeholder (state.rs) ⇒ statically the `None` arm; the
                    // reference logic is kept verbatim as the unreachable arm.
                    //   const insp = state.inspector orelse break :overlay &.{};
                    //   const renderer_info = insp.rendererInfo();
                    //   break :overlay renderer_info.overlayFeatures(arena_alloc) catch &.{};
                    Some(_insp) => unimplemented!(
                        "inspector.rendererInfo().overlayFeatures — deferred \
                         (rustty-inspector stub); verbatim body above"
                    ),
                };

                Critical {
                    links,
                    mouse: state.mouse,
                    preedit,
                    scrollbar,
                    overlay_features,
                }
            };

            // Outside the critical area we can update our links to contain
            // our regex results.
            let _ = self.config.links.render_cell_map(
                Global,
                &mut critical.links,
                &self.terminal_state,
                state.mouse.point,
                state.mouse.mods,
            );
            // catch |err| log.warn("error searching for regex links err={}", .{err});

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

                // NOTE: The order below matters. Highlights added earlier
                // will take priority.

                if let Some(m) = &self.search_selected_match {
                    let _ = self.terminal_state.update_highlights_flattened(
                        self.alloc,
                        HighlightTag::SearchMatchSelected as u8,
                        core::slice::from_ref(&m.r#match),
                    );
                    // catch |err| {
                    //     // Not a critical error, we just won't show highlights.
                    //     log.warn("error updating search selected highlight err={}", .{err});
                    // }
                }

                if let Some(m) = &self.search_matches {
                    let _ = self.terminal_state.update_highlights_flattened(
                        self.alloc,
                        HighlightTag::SearchMatch as u8,
                        &m.matches,
                    );
                    // catch |err| {
                    //     // Not a critical error, we just won't show highlights.
                    //     log.warn("error updating search highlights err={}", .{err});
                    // }
                }
            }

            // From this point forward no more errors.
            // errdefer comptime unreachable;

            // Reset our dirty state after updating.
            // defer self.terminal_state.dirty = .false;
            //
            // PORT NOTE (§C.4/RISK-3): the reference `defer` fires at function scope
            // exit — i.e. AFTER `updateCustomShaderUniformsFromState` (:1418, which
            // GATES on `dirty != .false`) and `endFrame` (:1423). So the reset is the
            // LAST statement of this fn (after `end_frame`), NOT here; the post-region
            // is infallible so a plain final assignment is faithful.

            // Rebuild the overlay image if we have one. We can do this
            // outside of any critical areas.
            let _ = self.rebuild_overlay(&critical.overlay_features);
            // catch |err| log.warn("error rebuilding overlay surface err={}", .{err});

            // Acquire the draw mutex for all remaining state updates.
            {
                self.draw_mutex.lock();

                // Build our GPU cells.
                //
                // `critical.preedit` is MOVED into `rebuild_cells`, but
                // `critical.preedit != null` is read FIRST for `cursor::style` — so
                // compute the cursor style (reading `is_some()`) before the move.
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
                // (rebuild_cells is infallible in rustty — its sole OOM source aborts
                // via the Global allocator, so the reference's "continue with frozen
                // contents" catch path above is unreachable here; §3b §C.4.)

                // The scrollbar is only emitted during draws so we also
                // check the scrollbar cache here and update if needed.
                // This is pretty fast.
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

                // If we're on macOS and have glass styles, we remove
                // the background opacity because the glass effect handles
                // it.
                //
                // PORT NOTE (§C.1-#24): the reference guards this with `if (comptime
                // builtin.os.tag == .macos)`; the whole `macos_impl` module is already
                // `#[cfg(target_os = "macos")]`-gated, so the comptime guard collapses
                // to the bare `switch`.
                match self.config.background_blur {
                    BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear => {
                        self.uniforms.bg_color[3] = 0
                    }
                    _ => {}
                }

                // Prepare our overlay image for upload (or unload). This
                // has to use our general allocator since it modifies
                // state that survives frames.
                let _ = self
                    .images
                    .overlay_update(self.alloc, self.overlay.as_ref());
                // catch |err| log.warn("error updating overlay images err={}", .{err});

                // Update custom shader uniforms that depend on terminal state.
                self.update_custom_shader_uniforms_from_state();

                self.draw_mutex.unlock();
            }

            // Notify our shaper we're done for the frame. For some shapers,
            // such as CoreText, this triggers off-thread cleanup logic.
            self.font_shaper.end_frame();

            // Reset our dirty state after updating (the reference `defer
            // self.terminal_state.dirty = .false`, fired at fn scope exit — §C.4).
            self.terminal_state.dirty = Dirty::False;

            Ok(())
        }

        /// Update uniforms that are based on the font grid.
        ///
        /// Caller must hold the draw mutex.
        //
        // Zig: fn updateFontGridUniforms(self) void — widened to `pub` for dead-code
        // exemption (no 3c caller; its caller `setFontGrid`/`Renderer::init` land later).
        pub fn update_font_grid_uniforms(&mut self) {
            self.uniforms.cell_size = [
                self.grid_metrics.cell_width as f32,
                self.grid_metrics.cell_height as f32,
            ];
        }

        /// Update uniforms that are based on the screen size.
        ///
        /// Caller must hold the draw mutex.
        //
        // Zig: fn updateScreenSizeUniforms(self) void — `pub` (no 3c caller).
        // PARITY: the reference multiplies the left/top padding by `-1`
        // (`-1 * @as(f32, @floatFromInt(self.size.padding.left))`); the `-1 *` form is
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

            // Setup our uniforms
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
        ///
        /// This should be called if and when configs change that
        /// could affect the background image.
        ///
        /// Caller must hold the draw mutex.
        //
        // Zig: fn updateBgImageBuffer(self) void — `pub` (no 3c caller).
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
            // Signal that the buffer was modified.
            self.bg_image_buffer_modified = self.bg_image_buffer_modified.wrapping_add(1);
        }

        /// Update custom shader uniforms that depend on terminal state.
        ///
        /// This should be called in `updateFrame` when terminal state changes.
        //
        // Zig: fn updateCustomShaderUniformsFromState(self) void — `pub` (called by
        // `update_frame` + directly unit-tested).
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
                uniforms.palette[i] = [
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    1.0,
                ];
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
            // PORT NOTE (§E.3/RISK-1): the reference reads `cursor_text.color.r` on a
            // `?TerminalColor` — a raw Zig union `.color` field access that is
            // SAFETY-CHECKED as a TAG-ASSERT (panics on the `cell-foreground`/
            // `cell-background` specials). A plain `if let Some(TerminalColor::Color(c))`
            // would SILENTLY SKIP the specials (a third behavior matching neither the
            // reference panic nor the 3b special-resolution path), so the faithful port
            // destructures the `Option`, then TAG-ASSERTs with `_ => unreachable!()`.
            if let Some(cursor_text) = self.config.cursor_text {
                match cursor_text {
                    TerminalColor::Color(c) => {
                        uniforms.cursor_text = [
                            c.r as f32 / 255.0,
                            c.g as f32 / 255.0,
                            c.b as f32 / 255.0,
                            1.0,
                        ];
                    }
                    _ => unreachable!(),
                }
            }

            // Selection background color
            if let Some(selection_bg) = self.config.selection_background {
                match selection_bg {
                    TerminalColor::Color(c) => {
                        uniforms.selection_background_color = [
                            c.r as f32 / 255.0,
                            c.g as f32 / 255.0,
                            c.b as f32 / 255.0,
                            1.0,
                        ];
                    }
                    _ => unreachable!(),
                }
            }

            // Selection foreground color
            if let Some(selection_fg) = self.config.selection_foreground {
                match selection_fg {
                    TerminalColor::Color(c) => {
                        uniforms.selection_foreground_color = [
                            c.r as f32 / 255.0,
                            c.g as f32 / 255.0,
                            c.b as f32 / 255.0,
                            1.0,
                        ];
                    }
                    _ => unreachable!(),
                }
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
        // Zig: fn updateCustomShaderUniformsForFrame(self) !void — the sole `try`
        // (`std.time.Instant.now()`) is infallible in Rust ⇒ `()`. `pub` (no 3c caller —
        // its caller is `drawFrame`/Chunk 4; directly unit-tested here). It issues ZERO
        // GPU calls (self-contained uniform math over `self.size` +
        // `self.cells.get_cursor_glyph()` + timing), so it lands cleanly in 3c.
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
                // PORT NOTE (§E.6 IMPLEMENTER FLAG): `cursor.grid_pos: [u16; 2]` but
                // `cell.{width,height}` / `padding.{left,top}` are `u32` — the reference
                // `grid_pos[0] * cell.width + padding.left` relies on Zig's implicit
                // widening. Rust has no implicit widening, so widen `grid_pos` to `u32`
                // BEFORE the `as f32`.
                let mut pixel_x: f32 =
                    (cursor.grid_pos[0] as u32 * cell.width + padding.left) as f32;
                // Top edge, relative to the top of the
                // screen, of the cell the cursor is in.
                let mut pixel_y: f32 =
                    (cursor.grid_pos[1] as u32 * cell.height + padding.top) as f32;

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

                let new_cursor: [f32; 4] = [pixel_x, pixel_y, cursor_width, cursor_height];
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
    }

    // =======================================================================
    // Chunk 3b unit tests (macOS-gated, Metal-skip; GPU_RD_CHUNK3B_DESIGN §G)
    // =======================================================================
    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::generic::{Color, RepeatablePath};
        use crate::link;
        use crate::size::{
            AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, CellSize,
            FontShapingBreak, GridSize, Padding, ScreenSize, Size, WindowColorspace,
        };
        use rustty_font_shaper::codepoint_resolver::{
            CodepointResolver, DescriptorCache, StyleStatus,
        };
        use rustty_font_shaper::collection::{AddOptions, Collection, LoadOptions, SizeAdjustment};
        use rustty_font_shaper::face::{
            DesiredSize, Face, Options as FaceOptions, FREETYPE_LOAD_FLAGS_DEFAULT,
        };
        use rustty_font_shaper::library::Library;
        use rustty_font_shaper::Style as FontStyle;
        use rustty_vt::kitty;
        use rustty_vt::modes::ModePacked;
        use rustty_vt::terminal::{Colors, Options as TermOptions, Terminal};
        use rustty_zigstd::arena_allocator::ArenaAllocator;

        // PORT NOTE: reuse the exact `opts(cols, rows)` helper from the cell.rs /
        // cursor.rs test modules (the established test-helper duplication precedent).
        fn opts(cols: CellCountInt, rows: CellCountInt) -> TermOptions {
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

        // GPU_RD_CHUNK3B_DESIGN §E.2: the 3b test (and the Chunk-4 golden) construct a
        // `DerivedConfig` literal with fixed values; `DerivedConfig::init<C>` stays a
        // deferred config-parsing seam. The four search colors mirror the reference
        // config defaults (Config.zig:1116/1117/1131/1132); `cursor-color` defaults to
        // null in the reference but we set it red so `add_cursor` renders a
        // deterministic, visible color for the golden (GOLDEN-GATE PROOF, §E.2).
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
                custom_shaders: RepeatablePath,
                bg_image: None,
                bg_image_opacity: 1.0,
                bg_image_position: BackgroundImagePosition::TopLeft,
                bg_image_fit: BackgroundImageFit::Contain,
                bg_image_repeat: false,
                // VERIFIED (§E.2): `link::Set::empty()` does NOT exist; `Set { links }`
                // has a pub field, so construct the empty (Global-allocated) set directly.
                links: link::Set { links: Vec::new() },
                vsync: false,
                colorspace: WindowColorspace::Srgb,
                blending: AlphaBlending::Native,
                background_blur: BackgroundBlur::False,
                scroll_to_bottom_on_output: false,
            }
        }

        // GPU_RD_CHUNK3B_DESIGN §G.1: assemble an offscreen `Renderer` field-by-field
        // (its `init` is deferred — needs a live surface). Returns `None` (skip) on
        // hosts with no Metal device (the `metal.rs` precedent). This is the Chunk-4
        // golden harness MINUS `drawFrame`.
        fn test_renderer(cols: CellCountInt, rows: CellCountInt) -> Option<Renderer> {
            // Headless CI may lack a Metal device → skip cleanly.
            let api = match Metal::init_offscreen(AlphaBlending::Native, 256, 256) {
                Ok(m) => m,
                Err(_) => {
                    eprintln!("no Metal device; skipping");
                    return None;
                }
            };
            let shaders = api.init_shaders(&[]).expect("init_shaders");
            let swap_chain = SwapChain::init(&api, false).expect("swap_chain");

            // Real font grid over the embedded JetBrains Mono Regular (the
            // `shared_grid.rs::test_grid` recipe). Leaked so the `*mut SharedGrid`
            // stays valid for the test's scope.
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

            // grid.metrics is loaded under the grid lock during `SharedGrid::init`.
            let grid_metrics = unsafe { (*font_grid).metrics };
            let font_shaper = Shaper::init(shape::Options::default()).expect("shaper");

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

            let mut cells = Contents::default();
            cells.resize(GridSize {
                rows,
                columns: cols,
            });

            Some(Renderer {
                alloc: Global,
                draw_mutex: Mutex::new(),
                config: derived_config_for_test(),
                surface_mailbox: SurfaceMailbox,
                grid_metrics,
                size,
                focused: true,
                custom_shader_focused_changed: false,
                scrollbar: Scrollbar::ZERO,
                scrollbar_dirty: false,
                last_bottom_node: None,
                last_bottom_y: 0,
                search_matches: None,
                search_selected_match: None,
                search_matches_dirty: false,
                cells,
                cells_rebuilt: false,
                // The fill only writes grid_size/padding_extend/cursor_pos/cursor_color/
                // bg_color; a zeroed (valid POD) `Uniforms` suffices for the rest.
                uniforms: unsafe { core::mem::zeroed() },
                custom_shader_uniforms: unsafe { core::mem::zeroed() },
                first_frame_time: None,
                last_frame_time: None,
                font_grid,
                font_shaper,
                font_shaper_cache: ShaperCache::init(),
                images: image::State::empty(),
                bg_image: None,
                bg_image_changed: false,
                bg_image_buffer: unsafe { core::mem::zeroed() },
                bg_image_buffer_modified: 0,
                api,
                display_link: None,
                health: AtomicHealth::new(crate::Health::Healthy),
                swap_chain,
                target_config_modified: 0,
                reinitialize_shaders: false,
                has_custom_shaders: false,
                shaders,
                terminal_state: RenderState::empty(),
                terminal_state_frame_count: 0,
                overlay: None,
            })
        }

        #[test]
        fn rebuild_cells_fills_bg_fg_cursor() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return, // skip w/o Metal
            };

            // small terminal: print "Ab" on row 0, leave a visible block cursor.
            let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
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

            t.deinit(Global);
        }

        #[test]
        fn rebuild_overlay_builds_and_clears() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };

            // empty features → overlay stays None
            r.rebuild_overlay(&[]).unwrap();
            assert!(r.overlay.is_none());

            // A real feature builds the overlay at the terminal size. `rebuild_overlay`
            // sets `self.overlay = Some(new)` BEFORE it calls `apply_features`, whose
            // semantic-prompt render body is a DEFERRED R5 seam
            // (`overlay.rs::highlight_semantic_prompts`, `unimplemented!()`; the z2d
            // render leaves land with the Chunk-4 golden). So we drive the build path
            // under `catch_unwind`: the OVERLAY-BUILD control flow is what 3b proves;
            // the panic from the un-ported render leaf is expected and swallowed (the
            // hook is silenced to keep the test output clean). Post-catch, the overlay
            // is present regardless of whether the leaf panicked.
            let prev_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = r.rebuild_overlay(&[crate::overlay::Feature::SemanticPrompts]);
            }));
            std::panic::set_hook(prev_hook);
            assert!(r.overlay.is_some());

            // and clearing removes it
            r.rebuild_overlay(&[]).unwrap();
            assert!(r.overlay.is_none());
        }

        // ===================================================================
        // Chunk 3c unit tests (macOS-gated, Metal-skip; GPU_RD_CHUNK3C_DESIGN §F)
        // ===================================================================

        // §F.2(1) — the headline orchestrator test: build a live `state::State` +
        // `Terminal`, drive `update_frame` end-to-end, and assert the same
        // Contents/uniforms the direct `rebuild_cells` test checks, now reached
        // through the mutex/critical-section walk, plus the bg_color + scrollbar.
        #[test]
        fn update_frame_fills_cells_uniforms_scrollbar() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };

            let mut mutex = Mutex::new();
            let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
            {
                let mut s = t.vt_stream();
                s.next_slice(b"Ab");
                s.deinit();
            }
            let st = state::State {
                mutex: &mut mutex,
                terminal: &mut t,
                inspector: None,
                preedit: None,
                mouse: state::Mouse::default(),
            };
            r.update_frame(&st, true).unwrap();

            // Contents filled (same asserts as rebuild_cells_fills_bg_fg_cursor,
            // now driven end-to-end):
            let bg = r.terminal_state.colors.background;
            assert_eq!(*r.cells.bg_cell(0, 0), [bg.r, bg.g, bg.b, 0]);
            assert!(!r.cells.fg_rows.lists[1].is_empty());
            let cur = r.cells.get_cursor_glyph().expect("cursor present");
            assert!(cur.bools.is_cursor_glyph);
            assert_eq!(cur.grid_pos, [2, 0]);
            // written by rebuild_cells inside update_frame
            assert_eq!(r.uniforms.cursor_pos, [2, 0]);

            // bg_color uniform (§C.1-#23): opacity 1.0 → alpha 255.
            assert_eq!(r.uniforms.bg_color, [bg.r, bg.g, bg.b, 255]);

            // The scrollbar snapshot happened inside the critical section; on a
            // small non-scrolling screen the cache matches the terminal's
            // scrollbar (recomputed from the same PageList).
            let expected_scrollbar = unsafe { (*t.screens.active).pages.scrollbar() };
            assert!(r.scrollbar.eql(expected_scrollbar));

            t.deinit(Global);
        }

        // §F.2(2) — direct: `screen_size`/`grid_padding`/`projection_matrix`.
        // (`-1 *` mirrors `update_screen_size_uniforms`'s reference form.)
        #[test]
        #[allow(clippy::neg_multiply)]
        fn update_screen_size_uniforms_projection() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };
            r.update_screen_size_uniforms();

            assert_eq!(
                r.uniforms.screen_size,
                [r.size.screen.width as f32, r.size.screen.height as f32]
            );

            // The fixture has all-zero padding and screen = cols*cell x rows*cell,
            // so there is no leftover and the blank padding is all zero.
            assert_eq!(r.uniforms.grid_padding, [0.0, 0.0, 0.0, 0.0]);

            // Spot-check the projection matrix against `ortho2d` recomputed from
            // the fixture.
            let terminal_size = r.size.terminal();
            let expected = math::ortho2d(
                -1.0 * r.size.padding.left as f32,
                (terminal_size.width + r.size.padding.right) as f32,
                (terminal_size.height + r.size.padding.bottom) as f32,
                -1.0 * r.size.padding.top as f32,
            );
            assert_eq!(r.uniforms.projection_matrix, expected);
        }

        // §F.2(3) — direct: `cell_size`.
        #[test]
        fn update_font_grid_uniforms_cell_size() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };
            r.update_font_grid_uniforms();
            assert_eq!(
                r.uniforms.cell_size,
                [
                    r.grid_metrics.cell_width as f32,
                    r.grid_metrics.cell_height as f32
                ]
            );
        }

        // §F.2(4) — direct: config → bg-image-buffer mapping + the modified counter.
        #[test]
        fn update_bg_image_buffer_maps_config() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };
            r.config.bg_image_position = BackgroundImagePosition::CenterCenter;
            r.config.bg_image_fit = BackgroundImageFit::Cover;
            r.config.bg_image_repeat = true;
            r.update_bg_image_buffer();
            assert_eq!(r.bg_image_buffer.info.position, BgImageInfoPosition::Mc);
            assert_eq!(r.bg_image_buffer.info.fit, BgImageInfoFit::Cover);
            assert!(r.bg_image_buffer.info.repeat);
            assert_eq!(r.bg_image_buffer_modified, 1);
        }

        // §F.2(5) — direct: custom-shader-from-state palette/colors/cursor.
        #[test]
        fn update_custom_shader_uniforms_from_state_palette() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };
            r.has_custom_shaders = true;

            let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
            r.terminal_state.update(Global, &mut t).unwrap(); // dirty = Full
            r.update_custom_shader_uniforms_from_state();

            let bg = r.terminal_state.colors.background;
            let fg = r.terminal_state.colors.foreground;
            assert_eq!(
                r.custom_shader_uniforms.background_color,
                [
                    bg.r as f32 / 255.0,
                    bg.g as f32 / 255.0,
                    bg.b as f32 / 255.0,
                    1.0
                ]
            );
            assert_eq!(
                r.custom_shader_uniforms.foreground_color,
                [
                    fg.r as f32 / 255.0,
                    fg.g as f32 / 255.0,
                    fg.b as f32 / 255.0,
                    1.0
                ]
            );
            let p0 = r.terminal_state.colors.palette[0];
            assert_eq!(
                r.custom_shader_uniforms.palette[0],
                [
                    p0.r as f32 / 255.0,
                    p0.g as f32 / 255.0,
                    p0.b as f32 / 255.0,
                    1.0
                ]
            );
            assert_eq!(
                r.custom_shader_uniforms.cursor_visible,
                r.terminal_state.cursor.visible as i32
            );
            let expected_style =
                cursor::Style::from_terminal(r.terminal_state.cursor.visual_style) as i32;
            assert_eq!(
                r.custom_shader_uniforms.current_cursor_style,
                expected_style
            );

            t.deinit(Global);
        }

        // §F.2(6) — direct: per-frame timing/resolution/focus. Per §RISK-7 the
        // `Contents` is empty (no prior `rebuild_cells`) so `get_cursor_glyph()`
        // is `None` and the cursor block is skipped; assert only the timing/
        // resolution/focus fields.
        #[test]
        fn update_custom_shader_uniforms_for_frame_timing() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };
            r.has_custom_shaders = true;
            r.update_custom_shader_uniforms_for_frame();

            assert_eq!(r.custom_shader_uniforms.frame, 1);
            assert_eq!(
                r.custom_shader_uniforms.resolution,
                [r.size.screen.width as f32, r.size.screen.height as f32, 1.0]
            );
            assert_eq!(
                r.custom_shader_uniforms.channel_resolution[0],
                [
                    r.size.screen.width as f32,
                    r.size.screen.height as f32,
                    1.0,
                    0.0
                ]
            );
            assert_eq!(r.custom_shader_uniforms.focus, 1);
            assert!(r.first_frame_time.is_some());
            assert!(r.last_frame_time.is_some());
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos_impl::*;
