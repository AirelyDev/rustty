//! The monomorphic GPU renderer (`GenericRenderer(Metal)`).
//!
//! PORTS: ghostty `src/renderer/generic.zig` — the `Renderer` struct decl, its
//! nested `SwapChain`/`FrameState`/`CustomShaderState`, and the `DerivedConfig`.
//!
//! Per `GPU_RD_TRAIT_DESIGN.md` §D + the committed monomorphic decision, the
//! reference `pub fn Renderer(comptime GraphicsAPI: type) type` is ported as a
//! CONCRETE `Renderer` over the concrete `Metal` leaf types (a set of one — no
//! `GraphicsAPI` trait hierarchy; OpenGL is Linux-deferred). The `.webgl` arm does
//! NOT come through here at all: the reference selects the bare `WebGL` type for it
//! (renderer.zig:41), a 2-line placeholder ported COMPLETE at `crate::webgl`.
//!
//! PORT STATUS (GPU R-d OFFSCREEN slice — Chunks 3a–4, DONE):
//!
//! - `DerivedConfig` (struct + `init<C>`/`deinit`) — the `config -> renderer`
//!   cycle is broken by (a) hoisting the 6 clean config value-types to
//!   `rustty-render-types` (Part A, landed), and (b) the `C`-generic seam (the
//!   `renderer/Thread.zig` `DerivedConfig::init<C>` precedent, `thread.rs`).
//!   `TerminalColor` is now a real renderer-local enum (Chunk 3b); `init<C>` is
//!   LANDED — the config coupling is resolved by a renderer-defined
//!   `RendererConfig` accessor trait impl'd IN `rustty-config` (the renderer
//!   names no config type; no config edge). `Path`/`RepeatablePath` stay
//!   inhabited placeholders (GPU_RD_CHUNK3A_DESIGN §B.1).
//! - The `Renderer` struct (all fields, comments preserved) + nested
//!   `SwapChain`/`FrameState`/`CustomShaderState` (+ the semaphore-backpressure
//!   method set: `SwapChain::{init,deinit,next_frame,release_frame}`;
//!   `FrameState::{init,resize}`; `CustomShaderState::{init,resize,swap}`).
//! - The full offscreen DRAW PATH: `rebuild_cells`/`rebuild_row`/`add_*` (the CPU
//!   cell-fill, Chunk 3b), `update_frame` + the uniforms math (Chunk 3c),
//!   `draw_frame`/`frame_completed`/`sync_atlas_texture`/`init_shaders`/
//!   `deinit_shaders` (Metal encode+submit -> IOSurface readback, Chunk 4).
//!   Golden-gated by `macos_impl::tests::golden_renders_cursor_fg_bg`. The
//!   raw-pointer aliasing (the shaper hook + the completion trampoline) is proven
//!   SB+TB-clean on the isolated Miri harness `.port/phase-2/mirihar-chunk3b/`
//!   (see MIRI_EXCEPTIONS.md).
//!
//! LANDED SINCE (each was formerly a deferred seam — now real, live-tested):
//! `DerivedConfig::init<C>`, `changeConfig`/`setScreenSize`/`setFontGrid` (resize +
//! config-swap), `Renderer::{init,deinit}` (offscreen device ctor + the
//! W-ATTACH host-view layer attach, `attach_host_view`), `prepBackgroundImage`/
//! custom-shader load (`init_shaders`), and the kitty-pixel path. The real
//! windowed layer-present path is RESOLVED (W-ATTACH, commit 96b0116) — there is
//! NO remaining windowed-render seam.
//!
//! DEFERRED (a later tier — still an `unimplemented!()` seam, verbatim body
//! preserved): only the inspector overlay-features seam
//! (`inspector.rendererInfo().overlayFeatures`, a `rustty-inspector` stub —
//! statically the `None` arm in the draw-frame overlay build). See DEFERRALS.md.
//!
//! DEAD-CODE (`-D warnings`): the reference `SwapChain`/`FrameState`/
//! `CustomShaderState` are Renderer-private (`const … = struct`). Some of their
//! methods are now reachable from the landed `draw_frame`; the rest are reachable
//! only from the deferred `Renderer::init`/`deinit`, so rustc would flag those
//! "never used". They are WIDENED to `pub` (the
//! GPU_RD_CHUNK3A_DESIGN §B.4 sanctioned "make pub" route — pub items reachable
//! from the crate root are dead-code-exempt), which avoids per-item
//! `#[allow(dead_code)]`. The visibility widening is documented per type.

use allocator_api2::alloc::Global;
use core::fmt;
use core::sync::atomic::{AtomicI32, Ordering};
// `CStr` is named by `Path::slice()` (the byte-faithful null-terminated path
// carrier); `CString` by the `Path::{Optional,Required}` arms + `font_features`.
use std::ffi::{CStr, CString};
use std::sync::Arc;

use rustty_font_shaper::codepoint_resolver::StyleStatus;
use rustty_font_shaper::SharedGrid;
use rustty_font_shaper::Style;
use rustty_render_types::{
    AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, FontShapingBreak,
    WindowColorspace, WindowPaddingColor,
};
use rustty_vt::color::RGB;
use rustty_vt::style::BoldColor;
use rustty_vt::Scrollbar;
use rustty_zigstd::arena_allocator::ArenaAllocator;

use crate::config::RendererConfig;
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

/// The renderer-local mirror of the reference `configpkg.Path`
/// (`union(enum) { required: [:0]const u8, optional: [:0]const u8 }`,
/// `config/path.zig:18`).
///
/// R4/G7: the renderer may NOT name `rustty_config::path::Path` (config already
/// deps renderer — the `Message::ChangeConfig` back-edge — so a `renderer ->
/// config` edge would cycle), so `DerivedConfig` carries this renderer-nameable
/// enum instead; `Config` maps its own `Path` into it via
/// `RendererConfig::bg_image` (config-side, acyclic — the exact pattern the
/// `background_image_{fit,position,repeat,opacity}` accessors already use).
///
/// Byte-faithful (MEMORY zig-byteslice-to-rust-rawbytes, paths especially): the
/// reference type is null-terminated (`[:0]const u8`) and its byte payload is
/// opened as an absolute file path, so `CString` is the faithful carrier — never a
/// lossy `String`. `prepBackgroundImage` consumes only the path bytes (`switch (p)
/// { .required, .optional => |slice| slice }` — both arms identical); the
/// optional/required distinction is carried purely for fidelity + `equal()` (the
/// `change_config` change-detect).
//
// PORT NOTE: no `Default` — the old placeholder derived it only as a ZST
// field-filler; the real enum has no meaningful default and the field type is
// `Option<Path>` (default `None`). `Clone` replaces the reference arena `clone`
// (`DerivedConfig::init`'s `try bg.clone(alloc)` collapses to `.clone()` — the
// `CString` owns its bytes, so the arena `dupeZ` is subsumed by owned `CString`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Path {
    /// No error if the file does not exist.
    // optional: [:0]const u8,
    Optional(CString),
    /// The file is required to exist.
    // required: [:0]const u8,
    Required(CString),
}

impl Path {
    /// Reference `Path.len` (`switch (self) { inline else => |path| path.len }`)
    /// — the byte length of either arm's slice.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        match self {
            Path::Optional(p) | Path::Required(p) => p.as_bytes().len(),
        }
    }

    /// Reference `Path.equal` (`std.meta.eql(self, other)`) — variant + bytes equal.
    pub fn equal(&self, other: &Path) -> bool {
        self == other
    }

    /// The raw null-terminated path bytes both `prepBackgroundImage` arms consume
    /// (`switch (p) { .required, .optional => |slice| slice }`).
    pub fn slice(&self) -> &CStr {
        match self {
            Path::Optional(p) | Path::Required(p) => p.as_c_str(),
        }
    }
}

/// Port of Zig `std.fs.path.extension` (over the POSIX `basename`) — the
/// dot-INCLUSIVE extension of the final path component, or `""`.
///
/// ADVERSARY F4: no port of `std.fs.path.extension` exists in `rustty-util`, and
/// `FileType::guessFromExtension` compares dot-INCLUSIVE bytes (`".png"`/`".jpg"`),
/// so `std::path::Path::extension` (which STRIPS the dot and mishandles a bare
/// leading-dot filename) is unfaithful and must NOT be used. This reproduces the
/// zigstd behavior exactly: the substring from the last `.` in the final component
/// (after the last `/`), `""` when there is no `.` in that component, and `""` when
/// the `.` is the component's first byte (a leading dot is not an extension).
//
// Reference `std.fs.path.extension`:
//   const filename = basename(path);
//   const index = mem.lastIndexOfScalar(u8, filename, '.') orelse return path[path.len..];
//   if (index == 0) return path[path.len..];
//   return filename[index..];
//
// Reference `std.fs.path.basenamePosix`:
//   if (path.len == 0) return &[_]u8{};
//   var end_index = path.len - 1;
//   while (path[end_index] == '/') { if (end_index == 0) return &[_]u8{}; end_index -= 1; }
//   var start_index = end_index; end_index += 1;
//   while (path[start_index] != '/') { if (start_index == 0) return path[0..end_index]; start_index -= 1; }
//   return path[start_index + 1 .. end_index];
pub fn path_extension(path: &[u8]) -> &[u8] {
    let filename = basename_posix(path);
    // const index = mem.lastIndexOfScalar(u8, filename, '.') orelse return "";
    let Some(index) = filename.iter().rposition(|&b| b == b'.') else {
        return &path[path.len()..];
    };
    // if (index == 0) return "";
    if index == 0 {
        return &path[path.len()..];
    }
    // return filename[index..];
    &filename[index..]
}

/// Port of Zig `std.fs.path.basenamePosix` — the final `/`-separated component
/// (trailing slashes stripped), or `""`.
fn basename_posix(path: &[u8]) -> &[u8] {
    // if (path.len == 0) return &[_]u8{};
    if path.is_empty() {
        return &path[path.len()..];
    }
    // var end_index = path.len - 1; while (path[end_index] == '/') { ... end_index -= 1; }
    let mut end_index = path.len() - 1;
    while path[end_index] == b'/' {
        if end_index == 0 {
            return &path[path.len()..];
        }
        end_index -= 1;
    }
    // var start_index = end_index; end_index += 1;
    let mut start_index = end_index;
    end_index += 1;
    // while (path[start_index] != '/') { if (start_index == 0) return path[0..end_index]; start_index -= 1; }
    while path[start_index] != b'/' {
        if start_index == 0 {
            return &path[0..end_index];
        }
        start_index -= 1;
    }
    // return path[start_index + 1 .. end_index];
    &path[start_index + 1..end_index]
}

/// Reference `configpkg.RepeatablePath` = `struct { value: ArrayListUnmanaged(Path) }`
/// (config/path.zig) — the `custom_shaders` config field.
///
/// R4/G7: renderer-local (the renderer may NOT name `rustty_config::path::RepeatablePath`
/// — config already deps renderer, so a `renderer -> config` edge would cycle). `Config`
/// maps its own `RepeatablePath` into this via `RendererConfig::custom_shaders`
/// (config-side, acyclic — the exact background-image `Path` pattern). Byte-faithful:
/// each element is the renderer-local [`Path`] (a `CString`, never a lossy `String`).
/// `load_from_files` reads the path bytes; `init_shaders` transpiles them into the
/// post-process pipelines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepeatablePath {
    // value: std.ArrayListUnmanaged(Path)
    pub value: Vec<Path>,
}

impl RepeatablePath {
    /// Reference `RepeatablePath.equal` (`len` then per-element `a.equal(b)`). The
    /// derived `PartialEq` on `Vec<Path>` is exactly this; kept for R1 parity and
    /// named-call fidelity (`change_config` change-detects via `!=`).
    pub fn equal(&self, other: &RepeatablePath) -> bool {
        self == other
    }
}

/// PORT NOTE: the inferred error set of [`DerivedConfig::init`] (`ArenaAllocator`
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
// fill destructures it, GPU_RD_CHUNK3B_DESIGN §E); `bg_image` now carries the REAL
// renderer-local `Path` enum (promoted in the background-image milestone — the
// `prepBackgroundImage` decode consumes it, RENDERER_BG_IMAGE_DESIGN §1).
// `custom_shaders` now carries the REAL renderer-local `RepeatablePath` (a
// `Vec<Path>`, promoted in the custom-shader milestone — `init_shaders` reads it to
// load + transpile + build the post-process pipelines).
pub struct DerivedConfig {
    pub arena: ArenaAllocator,

    pub font_thicken: bool,
    pub font_thicken_strength: u8,
    // Zig `std.ArrayListUnmanaged([:0]const u8)` — a list of null-terminated
    // byte strings -> `Vec<Vec<u8>>` (fidelity: RAW bytes, not a lossy `String`
    // and not a `CString`, which cannot represent the interior NUL a config
    // line may legally contain — see `RendererConfig::font_features`).
    pub font_features: Vec<Vec<u8>>,
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
    // PORT NOTE (`aee7bf347`): MOVED here from `renderer/Thread.zig`'s
    // `DerivedConfig` — the renderer computes its own animation wakes now
    // (`animation_wake`). The renderer-local `CustomShaderAnimation` mirror
    // stays declared in `crate::thread` (R4/G7 — see its PORT NOTE).
    pub custom_shader_animation: crate::thread::CustomShaderAnimation,
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
    //
    // PORT NOTE: the reference reads `config.@"…"` fields directly off `*const
    // configpkg.Config`; here the `renderer -> config` edge would cycle (R4/G7), so
    // the reads go through the renderer-defined [`RendererConfig`] accessor trait
    // (`crate::config`), impl-ed by `Config` IN rustty-config. `C` is bare-bounded
    // by `RendererConfig`; the concrete `C = rustty_config::Config` binds in core.
    // `bg_image` is now REAL (config `?Path` -> the renderer-local `Path` via
    // `RendererConfig::bg_image`, feeding `prepBackgroundImage` + wuffs decode);
    // `custom_shaders` is likewise REAL (config `RepeatablePath` -> the renderer-local
    // `RepeatablePath` via `RendererConfig::custom_shaders`, feeding `init_shaders`).
    pub fn init<C: RendererConfig>(
        alloc_gpa: Global,
        config: &C,
    ) -> Result<DerivedConfig, DerivedConfigError> {
        // var arena = ArenaAllocator.init(alloc_gpa);
        // errdefer arena.deinit();
        // const alloc = arena.allocator();
        //
        // PORT NOTE: `Global` is a ZST — the reference arena backs the config string
        // clones; here `custom_shaders`/`bg_image` own their `CString`s and
        // `font_features` is an owned `Vec`, so the arena owns nothing but is preserved
        // as a struct field for fidelity. `errdefer arena.deinit()` maps to the
        // `ArenaAllocator` `Drop` on the error path (the local `arena` is dropped if we
        // return `Err` before moving it into the struct).
        let arena = ArenaAllocator::init();
        let _ = alloc_gpa;

        // Copy our shaders
        //
        // Reference: `const custom_shaders = try config.@"custom-shader".clone(alloc);`.
        // The config `RepeatablePath` is read through the renderer-defined
        // `RendererConfig::custom_shaders` accessor (R4/G7 — the renderer may not name
        // `rustty_config::path::RepeatablePath`), which maps each element arm-for-arm
        // into the renderer-local `Path` (a `CString` over the same path bytes). The
        // accessor's per-element `CString` clone + `.collect()` IS the reference clone.
        let custom_shaders = config.custom_shaders();

        // Copy our background image
        //
        // Reference: `const bg_image = if (config.@"background-image") |bg| try
        // bg.clone(alloc) else null;`. The config `?Path` is read through the
        // renderer-defined `RendererConfig::bg_image` accessor (R4/G7 — the renderer
        // may not name `rustty_config::path::Path`), which maps arm-for-arm into the
        // renderer-local `Path` (§1). The arena `clone` collapses to the owned
        // `CString` inside `Path` (no separate `.clone()` needed — the accessor
        // already returns an owned value).
        let bg_image: Option<Path> = config.bg_image();

        // Copy our font features (clone -> owned Vec<CString>).
        let font_features = config.font_features();

        // Get our font styles
        let mut font_styles = StyleStatus::init_fill(true);
        font_styles.set(Style::Bold, config.font_style_bold());
        font_styles.set(Style::Italic, config.font_style_italic());
        font_styles.set(Style::BoldItalic, config.font_style_bold_italic());

        // Our link configs
        let links =
            link::Set::from_config(Global, config.links()).map_err(|_| DerivedConfigError)?;

        Ok(DerivedConfig {
            background_opacity: config.background_opacity().clamp(0.0, 1.0),
            background_opacity_cells: config.background_opacity_cells(),
            font_thicken: config.font_thicken(),
            font_thicken_strength: config.font_thicken_strength(),
            font_features,
            font_styles,
            font_shaping_break: config.font_shaping_break(),

            cursor_color: config.cursor_color(),
            cursor_text: config.cursor_text(),
            cursor_opacity: config.cursor_opacity().clamp(0.0, 1.0),

            background: config.background().to_terminal_rgb(),
            foreground: config.foreground().to_terminal_rgb(),
            bold_color: config.bold_color(),
            faint_opacity: (config.faint_opacity() * 255.0).ceil() as u8,

            min_contrast: config.minimum_contrast() as f32,
            padding_color: config.window_padding_color(),

            selection_background: config.selection_background(),
            selection_foreground: config.selection_foreground(),
            search_background: config.search_background(),
            search_foreground: config.search_foreground(),
            search_selected_background: config.search_selected_background(),
            search_selected_foreground: config.search_selected_foreground(),

            custom_shaders,
            bg_image,
            bg_image_opacity: config.background_image_opacity(),
            bg_image_position: config.background_image_position(),
            bg_image_fit: config.background_image_fit(),
            bg_image_repeat: config.background_image_repeat(),
            links,
            vsync: config.window_vsync(),
            colorspace: config.window_colorspace(),
            blending: config.alpha_blending(),
            background_blur: config.background_blur(),
            scroll_to_bottom_on_output: config.scroll_to_bottom_output(),
            // .custom_shader_animation = config.@"custom-shader-animation",
            custom_shader_animation: config.custom_shader_animation(),
            arena,
        })
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
// DerivedConfig::init<C> tests — cross-platform (no Metal), mock-`C` (§5).
// ===========================================================================
//
// The renderer CANNOT dep rustty-config even as a dev-dependency (that IS the
// cycle, R4). So `DerivedConfig::init<C>` is exercised through a `#[cfg(test)]`
// mock `C` impl-ing [`RendererConfig`], returning fixed values; the test asserts
// the extracted `DerivedConfig` fields (the config-side `impl RendererConfig for
// Config` is compile-verified separately by `cargo build -p rustty-config`).
#[cfg(test)]
mod derived_config_tests {
    use super::*;
    // The mock `C` (impl `RendererConfig`, fixed values) is shared with the
    // message.rs/thread.rs seam tests — see `crate::test_support::MockConfig`.
    use crate::test_support::MockConfig;

    #[test]
    fn derived_config_init_extracts_fields() {
        let cfg = MockConfig::default();
        let d = DerivedConfig::init(Global, &cfg).expect("init");

        assert!(!d.font_thicken);
        assert_eq!(d.font_thicken_strength, 0);
        assert!(d.font_features.is_empty());
        // `!= .false` => every style enabled.
        assert!(d.font_styles.get(Style::Bold));
        assert!(d.font_styles.get(Style::Italic));
        assert!(d.font_styles.get(Style::BoldItalic));

        // `@ceil(1.0 * 255)` => 255 (u8); `@floatCast(f64)` => f32.
        assert_eq!(d.faint_opacity, 255);
        assert_eq!(d.min_contrast, 1.0f32);

        // `@max(0, @min(1, x))` clamps.
        assert_eq!(d.background_opacity, 1.0);
        assert_eq!(d.cursor_opacity, 1.0);

        // `Color::to_terminal_rgb`.
        assert_eq!(d.background, RGB { r: 0, g: 0, b: 0 });
        assert_eq!(
            d.foreground,
            RGB {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF
            }
        );
        assert_eq!(
            d.cursor_color,
            Some(TerminalColor::Color(Color {
                r: 0xFF,
                g: 0,
                b: 0
            }))
        );
        assert_eq!(d.cursor_text, None);

        assert_eq!(d.blending, AlphaBlending::Native);
        assert_eq!(d.colorspace, WindowColorspace::Srgb);
        assert_eq!(d.padding_color, WindowPaddingColor::Background);
        assert!(!d.scroll_to_bottom_on_output);

        // `bg_image` unset by the base mock => `None`; the placeholder `links` empty.
        assert!(d.bg_image.is_none());
        assert!(d.links.links.is_empty());
    }

    // T2 (RENDERER_BG_IMAGE_DESIGN §4): the config `?Path` -> renderer-local `Path`
    // mapping flows through `DerivedConfig::init` byte-faithfully. The renderer
    // canNOT dep rustty-config, so a mock returning `Some(Path::Required(CString))`
    // stands in for the config-side accessor; assert the variant + bytes round-trip
    // and that `Path::equal` feeds the `change_config` change-detect semantics.
    #[test]
    fn derived_config_maps_bg_image_path() {
        use super::Path;

        // Some(Path::Required(..)) flows into DerivedConfig.bg_image unchanged.
        let cstr = CString::new(b"/tmp/bg.png".to_vec()).unwrap();
        let cfg = MockConfig {
            bg_image: Some(Path::Required(cstr.clone())),
            ..Default::default()
        };
        let d = DerivedConfig::init(Global, &cfg).expect("init");
        match &d.bg_image {
            Some(Path::Required(bytes)) => {
                // Byte-faithful: the exact path bytes (no NUL, no lossy re-encode).
                assert_eq!(bytes.as_bytes(), b"/tmp/bg.png");
                assert_eq!(bytes, &cstr);
            }
            other => panic!("expected Some(Path::Required), got {other:?}"),
        }

        // None flows through as None.
        let d_none = DerivedConfig::init(Global, &MockConfig::default()).expect("init");
        assert!(d_none.bg_image.is_none());

        // Path::equal / PartialEq semantics that drive `bg_image_changed`:
        let opt = Path::Optional(CString::new("a").unwrap());
        let req_a = Path::Required(CString::new("a").unwrap());
        let req_b = Path::Required(CString::new("b").unwrap());
        assert!(req_a.equal(&req_a)); // Some -> same
        assert!(!req_a.equal(&req_b)); // Some -> different bytes
        assert!(!req_a.equal(&opt)); // Some -> different variant (same bytes)
        assert_eq!(req_a.len(), 1); // the byte length of the arm's slice
    }

    // RENDERER_CUSTOM_SHADERS_DESIGN §4.3: the config `RepeatablePath` -> the
    // renderer-local `RepeatablePath` (a `Vec<Path>`) mapping flows through
    // `DerivedConfig::init` byte-faithfully. The renderer canNOT dep rustty-config, so
    // a mock returning `RepeatablePath { value: vec![Path::Required(..)] }` stands in
    // for the config-side accessor; assert the arm + bytes round-trip.
    #[test]
    fn derived_config_maps_custom_shaders() {
        use super::Path;
        use std::ffi::CString;

        let cfg = MockConfig {
            custom_shaders: vec![Path::Required(CString::new("/x/shader.glsl").unwrap())],
            ..Default::default()
        };
        let d = DerivedConfig::init(Global, &cfg).expect("init");
        assert_eq!(d.custom_shaders.value.len(), 1);
        match &d.custom_shaders.value[0] {
            Path::Required(p) => assert_eq!(p.to_bytes(), b"/x/shader.glsl"),
            other => panic!("wrong arm: {other:?}"),
        }

        // Default mock => empty list.
        let d0 = DerivedConfig::init(Global, &MockConfig::default()).expect("init");
        assert!(d0.custom_shaders.value.is_empty());
    }

    // path_extension (F4 — the `std.fs.path.extension` byte port that feeds
    // `FileType::guessFromExtension`): dot-INCLUSIVE final-component extension.
    #[test]
    fn path_extension_matches_zigstd() {
        use super::path_extension;
        // Normal extension (dot-inclusive, so it matches `guessFromExtension`'s ".png").
        assert_eq!(path_extension(b"/a/b/img.png"), b".png");
        assert_eq!(path_extension(b"photo.JPEG"), b".JPEG");
        // Last dot wins.
        assert_eq!(path_extension(b"a.tar.gz"), b".gz");
        // No dot in the final component -> "".
        assert_eq!(path_extension(b"/a/b/noext"), b"");
        assert_eq!(path_extension(b"/a.b/noext"), b""); // dot only in a parent dir
                                                        // A leading-dot bare filename is NOT an extension (index == 0).
        assert_eq!(path_extension(b".png"), b"");
        assert_eq!(path_extension(b"/a/b/.hidden"), b"");
        // Trailing slashes stripped by basename; empty stays empty.
        assert_eq!(path_extension(b"/a/b.png/"), b".png");
        assert_eq!(path_extension(b""), b"");
        assert_eq!(path_extension(b"/"), b"");
    }

    // T4 (RENDERER_BG_IMAGE_DESIGN §4 / R-2): the decoded buffer length is exactly
    // `width * height * 4` (the RGBA bpp). This is the invariant `image::Pending::len()`
    // (`w*h*bpp`) recomputes for the later `free_data` align-1 `Layout`, so the
    // transferred-pointer free provably matches the `rustty-wuffs` alloc (guards the
    // one genuine UB surface — a layout mismatch = a bad free). Host test (no Metal).
    #[test]
    fn decoded_bg_image_len_is_wh4() {
        use allocator_api2::alloc::Global;

        // A 2×3 solid-color PNG -> the decoded RGBA buffer must be 2*3*4 = 24 bytes.
        let png_bytes = encode_solid_png(2, 3, [0x30, 0x60, 0xC0]);
        let img = rustty_wuffs::png::decode(Global, &png_bytes).expect("decode");
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 3);
        // The load-bearing invariant: len == w*h*4 (== Pending::len(), the free layout).
        assert_eq!(
            img.bytes().len(),
            img.width as usize * img.height as usize * 4
        );
        // And the pixels are the decoded solid color (RGBA_NONPREMUL, opaque).
        assert_eq!(&img.bytes()[0..4], &[0x30, 0x60, 0xC0, 0xFF]);
    }

    /// Encode a `w`×`h` solid-RGB PNG (8-bit) for the bg-image tests (T3/T4). The
    /// reference `background-image` fixtures are not checked in; generate at test
    /// time via the pure-Rust `png` encoder (the rustty-wuffs/font stand-in).
    pub(super) fn encode_solid_png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
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
}

// ===========================================================================
// Surface mailbox seam (apprt.surface.Mailbox) — RESOLVED
// ===========================================================================
//
// PORT NOTE (surface-mailbox seam, GPU_RD_CHUNK3A_DESIGN §B.2 — RESOLVED):
// `apprt.surface.Mailbox` cannot be NAMED here — `rustty-apprt-api ->
// rustty-renderer` (apprt-api/Cargo.toml), so a `renderer -> apprt-api` edge
// would CYCLE (check_acyclic). The design's prescribed resolution (taken now that
// `drawFrame` has landed) is a RENDERER-LOCAL mailbox trait that core binds to the
// real `apprt.surface.Mailbox`: the renderer declares [`SurfaceMessage`] (the only
// two `apprt.surface.Message` variants `generic.zig` ever pushes) plus the
// [`SurfaceMailboxSink`] trait, and `rustty-core` implements the trait over its
// `surface::SurfaceMailbox<A>` and hands it in through [`Options`]. Same acyclic
// shape as the `RendererConfig` accessor trait (`crate::config`): the renderer
// never names an apprt type; the apprt-typed side lives in core.
//
// These items are SHARED by both `Renderer` monomorphizations (`macos_impl`'s
// `Renderer(Metal)` and `linux_impl`'s `Renderer(OpenGL)`) — `generic.zig`
// instantiates ONE generic function for both — so they live at the module level
// rather than being duplicated per platform arm.

/// The surface messages the renderer pushes onto the surface mailbox.
//
// PORT NOTE: the renderer's slice of `apprt.surface.Message` (src/apprt/
// surface.zig) — `generic.zig` constructs exactly two of its variants,
// `.{ .scrollbar = self.scrollbar }` (:1452-1454) and
// `.{ .renderer_health = health }` (:1735-1737). Core re-wraps these into the
// full `apprt::surface::Message` at the seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceMessage {
    /// The scrollbar state changed for the surface (generic.zig:1452-1454).
    Scrollbar(Scrollbar),

    /// Health status change for the renderer (generic.zig:1735-1737).
    RendererHealth(Health),
}

/// The push timeout mode for [`SurfaceMailbox::push`].
//
// PORT NOTE: `App.Mailbox.Queue.Timeout`. The reference uses `.instant` at
// `drawFrame` (:1454) and `.{ .forever = {} }` at `frameCompleted` (:1735) — the
// only two modes the renderer ever asks for, so the renderer-local enum carries
// exactly those two (core maps them onto
// `rustty_datastruct::blocking_queue::Timeout`, whose third `Ns` mode no renderer
// call site uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailboxTimeout {
    Instant,
    Forever,
}

/// The core-supplied sink behind [`SurfaceMailbox`] — the renderer's half of
/// `apprt.surface.Mailbox`.
//
// PORT NOTE: one method, mirroring the reference's single
// `Mailbox.push(msg, timeout) App.Mailbox.Queue.Size` entry point (the reference
// pushes both message kinds through the SAME call). No `Send`/`Sync` bound: the
// `Renderer` itself is not `Send` (it holds `*mut SharedGrid` and graphics-API
// handles) and is moved onto the render thread through a raw `*mut Renderer`
// (renderer_seam `Thread<A>`), exactly as the reference's raw `*Renderer` field.
pub trait SurfaceMailboxSink {
    /// `apprt.surface.Mailbox.push(msg, timeout)` — returns the number of
    /// messages pushed (0 means the queue refused it).
    fn push(&self, msg: SurfaceMessage, timeout: MailboxTimeout) -> usize;
}

/// The mailbox for communicating with the window (`apprt.surface.Mailbox`).
//
// PORT NOTE: the reference field is a by-value `apprt.surface.Mailbox` struct
// (`{ surface: *Surface, app: App.Mailbox }`) which is always present. Rust
// models it as a handle that may be DETACHED, because the renderer crate's own
// offscreen entry points (the golden/unit-test `Renderer::init`, which has no
// surface at all) have no mailbox to attach — a detached push returns 0, which is
// exactly the "queue refused it" answer the reference's own `.instant` push can
// give, so both push sites stay verbatim.
#[derive(Clone, Default)]
pub struct SurfaceMailbox {
    sink: Option<Arc<dyn SurfaceMailboxSink>>,
}

impl SurfaceMailbox {
    /// An unattached mailbox: every push is a no-op returning 0.
    pub const DETACHED: SurfaceMailbox = SurfaceMailbox { sink: None };

    /// Attach a core-supplied sink (`renderer_seam::Options.surface_mailbox`).
    pub fn new(sink: Arc<dyn SurfaceMailboxSink>) -> SurfaceMailbox {
        SurfaceMailbox { sink: Some(sink) }
    }

    /// Whether a sink is attached (a detached mailbox always pushes 0).
    pub fn is_attached(&self) -> bool {
        self.sink.is_some()
    }

    /// `self.surface_mailbox.push(msg, timeout)` — the number of messages pushed.
    pub fn push(&self, msg: SurfaceMessage, timeout: MailboxTimeout) -> usize {
        match &self.sink {
            Some(sink) => sink.push(msg, timeout),
            None => 0,
        }
    }
}

// `dyn SurfaceMailboxSink` is not `Debug`, so `Debug` is hand-written (the
// `Renderer`'s other handles print the same way).
impl fmt::Debug for SurfaceMailbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SurfaceMailbox")
            .field("attached", &self.is_attached())
            .finish()
    }
}

#[cfg(test)]
pub(crate) mod surface_mailbox_tests {
    use std::sync::{Arc, Mutex};

    use super::{MailboxTimeout, SurfaceMailbox, SurfaceMailboxSink, SurfaceMessage};
    use crate::Health;
    use rustty_vt::Scrollbar;

    /// A recording [`SurfaceMailboxSink`] — stands in for core's real
    /// `apprt.surface.Mailbox` adapter (`renderer_seam::SurfaceMailboxAdapter`).
    ///
    /// `accept` is what `push` returns, i.e. the reference's
    /// `App.Mailbox.Queue.Size`: `1` = the message was enqueued, `0` = the queue
    /// refused it (a full queue on an `.instant` push — the case `drawFrame`'s
    /// scrollbar `defer` explicitly handles by KEEPING `scrollbar_dirty`).
    pub(crate) struct RecordingSink {
        // `Mutex` (not `RefCell`) so the sink is `Send + Sync` — the production
        // sink is shared with the render thread through the `Renderer`.
        pushed: Mutex<Vec<(SurfaceMessage, MailboxTimeout)>>,
        accept: usize,
    }

    impl RecordingSink {
        /// A sink whose queue always accepts (`push` returns 1).
        pub(crate) fn accepting() -> RecordingSink {
            RecordingSink {
                pushed: Mutex::new(Vec::new()),
                accept: 1,
            }
        }

        /// A sink whose queue is FULL (`push` returns 0).
        pub(crate) fn refusing() -> RecordingSink {
            RecordingSink {
                pushed: Mutex::new(Vec::new()),
                accept: 0,
            }
        }

        pub(crate) fn pushed(&self) -> Vec<(SurfaceMessage, MailboxTimeout)> {
            self.pushed.lock().unwrap().clone()
        }
    }

    impl SurfaceMailboxSink for RecordingSink {
        fn push(&self, msg: SurfaceMessage, timeout: MailboxTimeout) -> usize {
            self.pushed.lock().unwrap().push((msg, timeout));
            self.accept
        }
    }

    fn scrollbar() -> Scrollbar {
        Scrollbar {
            total: 120,
            offset: 7,
            len: 24,
        }
    }

    /// A DETACHED mailbox (offscreen renderers — no surface) swallows every push
    /// and reports 0, exactly like the pre-resolution placeholder did, so the
    /// goldens keep observing no mailbox.
    #[test]
    fn detached_mailbox_pushes_nothing() {
        let mailbox = SurfaceMailbox::DETACHED;
        assert!(!mailbox.is_attached());
        assert_eq!(
            mailbox.push(
                SurfaceMessage::Scrollbar(scrollbar()),
                MailboxTimeout::Instant
            ),
            0
        );
        assert_eq!(
            mailbox.push(
                SurfaceMessage::RendererHealth(Health::Unhealthy),
                MailboxTimeout::Forever
            ),
            0
        );
        assert!(!SurfaceMailbox::default().is_attached());
    }

    /// An ATTACHED mailbox routes both reference message kinds — and both
    /// reference timeout modes — through to the core-supplied sink, and returns
    /// the sink's push count verbatim (the `> 0` test at generic.zig:1454).
    #[test]
    fn attached_mailbox_routes_pushes_to_the_sink() {
        let sink = Arc::new(RecordingSink::accepting());
        let mailbox = SurfaceMailbox::new(sink.clone());
        assert!(mailbox.is_attached());

        assert_eq!(
            mailbox.push(
                SurfaceMessage::Scrollbar(scrollbar()),
                MailboxTimeout::Instant
            ),
            1
        );
        assert_eq!(
            mailbox.push(
                SurfaceMessage::RendererHealth(Health::Unhealthy),
                MailboxTimeout::Forever
            ),
            1
        );
        assert_eq!(
            sink.pushed(),
            vec![
                (
                    SurfaceMessage::Scrollbar(scrollbar()),
                    MailboxTimeout::Instant
                ),
                (
                    SurfaceMessage::RendererHealth(Health::Unhealthy),
                    MailboxTimeout::Forever
                ),
            ]
        );

        // A FULL queue reports 0 — the `.instant` refusal the scrollbar `defer`
        // must survive.
        let full = Arc::new(RecordingSink::refusing());
        assert_eq!(
            SurfaceMailbox::new(full.clone()).push(
                SurfaceMessage::Scrollbar(scrollbar()),
                MailboxTimeout::Instant
            ),
            0
        );
        assert_eq!(full.pushed().len(), 1);
    }
}

// ===========================================================================
// Options (renderer/Options.zig, minus the apprt-typed fields)
// ===========================================================================

/// The options that are used to configure a renderer.
//
// PORT NOTE (ports `renderer/Options.zig`): three reference fields are
// apprt-typed. `surface_mailbox: apprt.surface.Mailbox` is now CARRIED (as the
// renderer-local [`SurfaceMailbox`] handle — the §B.2 seam is resolved above);
// `rt_surface: *apprt.Surface` and `thread: *renderer.Thread` are still DEFERRED
// and stay in core's `renderer_seam::Options<A>`, since `rustty-apprt-api ->
// rustty-renderer` (apprt-api/Cargo.toml) means naming an apprt type here would
// CYCLE (check_acyclic) and the real renderer never reads either of them.
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

    /// The mailbox for sending the surface messages. This is only valid
    /// once the thread has started and should not be used outside of the thread.
    //
    // PORT NOTE: `surface_mailbox: apprt.surface.Mailbox` — the renderer-local
    // handle (seam §B.2, resolved). Offscreen callers pass
    // [`SurfaceMailbox::DETACHED`].
    pub surface_mailbox: SurfaceMailbox,
    // DEFERRED (surface-mailbox seam §B.2):
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
        AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, CellSize,
        GridSize, Padding, ScreenSize, Size, WindowColorspace,
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
    const CUSTOM_SHADER_Y_IS_DOWN: bool = Metal::CUSTOM_SHADER_Y_IS_DOWN;

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
                rustty_wuffs::Error::WuffsError | rustty_wuffs::Error::Overflow => {
                    Error::ImageDecode
                }
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
    // `crate::metal::shaders::Error` (`MetalFailed`).
    impl From<crate::metal::shaders::Error> for Error {
        fn from(_: crate::metal::shaders::Error) -> Self {
            Error::MetalFailed
        }
    }
    // `font.Shaper.init` (`changeConfig`, and the deferred `Renderer::init`) fails
    // with the platform shaper's error (`shape::coretext::Error` — an atlas OOM or a
    // font `Collection` failure). It folds into the aggregate `OutOfMemory` member of
    // the reference `!void` inferred union (a font-resource failure, not a Metal one).
    impl From<shape::coretext::Error> for Error {
        fn from(_: shape::coretext::Error) -> Self {
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
        pub frame_sema: Semaphore,
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
                frame_sema: Semaphore::with_permits(SWAP_CHAIN_COUNT),
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
            // Initialize our graphics API wrapper, this will prepare the
            // surface provided by the apprt and set up any API-specific
            // GPU resources.
            //
            // var api = try GraphicsAPI.init(alloc, options);   (generic.zig:662)
            // errdefer api.deinit();
            //
            // W1-7 DIVERGENCE (R4/apprt-free, DEFERRALS.md #3): `Options` carries no
            // `rt_surface` (it stays in `renderer_seam::Options<A>` in core so
            // rustty-renderer names no apprt type — R4), so `api` is built via the
            // offscreen-portable device ctor (`init_offscreen`, layer `None`) instead
            // of the live-surface `GraphicsAPI.init`. The on-screen layer attach is
            // supplied by the windowing host via `Metal::init` (W1-2) at the
            // renderer_seam wiring (W3) — behaviorally identical to ghostty across the
            // `init` -> host-attach sequence. This is the "reuse `init_offscreen`'s
            // shared body, do NOT duplicate" directive at the `Renderer` level.
            let api = Metal::init_offscreen(
                options.config.blending,
                options.size.screen.width,
                options.size.screen.height,
            )?;

            // const has_custom_shaders = options.config.custom_shaders.value.items.len > 0;
            let has_custom_shaders = !options.config.custom_shaders.value.is_empty();

            // Prepare our swap chain
            let swap_chain = SwapChain::init(&api, has_custom_shaders)?; // errdefer -> Drop

            // Create the font shaper.
            //
            // Zig: `var font_shaper = try font.Shaper.init(alloc, .{
            //          .features = options.config.font_features.items });`
            // The `[:0]const u8` items -> the shaper's `Vec<Vec<u8>>` features
            // (the RAW bytes, matching the Zig `.len`-based content; the
            // `change_config` precedent, generic.zig).
            let font_shaper = Shaper::init(shape::Options {
                features: options.config.font_features.clone(),
            })?; // errdefer -> Drop

            // Initialize all the data that requires a critical font section.
            //
            // const font_critical: struct { metrics: font.Metrics } = font_critical: {
            //     const grid: *font.SharedGrid = options.font_grid;
            //     grid.lock.lockShared();
            //     defer grid.lock.unlockShared();
            //     break :font_critical .{ .metrics = grid.metrics };
            // };
            //
            // The shared-read lock is LOAD-BEARING and MUST be mirrored (R1): the
            // reference takes `grid.lock.lockShared()` before reading `grid.metrics`.
            // `SharedGrid` exposes `pub lock: RwLock<()>` with `.read()` (every
            // `lockShared` site in the port is `self.lock.read().unwrap()`).
            let grid_metrics = {
                // const grid: *font.SharedGrid = options.font_grid;
                let g: *mut SharedGrid = options.font_grid;
                // grid.lock.lockShared(); defer grid.lock.unlockShared();
                //
                // SAFETY: `g` is the external `*mut SharedGrid` handle; `(*g).lock` /
                // `(*g).metrics` are shared reads through a raw pointer with no `&mut`
                // alias — the `set_font_grid` (§4.4) precedent, no SB hazard.
                let _read = unsafe { (*g).lock.read().unwrap() };
                unsafe { (*g).metrics }
            };

            // PORT NOTE (`a177ba90a`): `init` no longer creates the display link.
            // The reference DELETED its eager `createWithActiveCGDisplays()` +
            // `errdefer ... release()` pair here, because a locked macOS session can
            // temporarily have no active displays and that error path aborted renderer
            // init (reported as OutOfMemory, no less). Creation is now LAZY, inside
            // `sync_display_link` below, which tolerates failure by falling back to
            // event-driven rendering without vsync and retrying on the next display
            // update. The field starts `None`; the owning-`DisplayLinkErrdefer` RAII
            // guard this replaced is gone with the fallible call it guarded.

            // generic.zig:718-743 uniforms literal. The reference sets
            // projection_matrix/cell_size/grid_size/grid_padding/screen_size (and
            // cursor_color) to `undefined` (fixed by the `update_*` calls below /
            // `update_frame`), and every REMAINING field to a concrete value. Rust has
            // no `undefined`: start from zeroed POD (the `undefined` fields) then set
            // each field the reference sets. NB (R1, corrects a design-doc note):
            // `update_font_grid_uniforms`/`update_screen_size_uniforms` do NOT
            // re-apply `min_contrast`/`cursor_pos`/`bg_color`/`bools`, so these MUST be
            // set here to match the reference first-frame state.
            let uniforms = {
                let mut u: Uniforms = unsafe { core::mem::zeroed() };
                // .padding_extend = .{},
                u.padding_extend = UniformsPaddingExtend::default();
                // .min_contrast = options.config.min_contrast,
                u.min_contrast = options.config.min_contrast;
                // .cursor_pos = .{ std.math.maxInt(u16), std.math.maxInt(u16) },
                u.cursor_pos = [u16::MAX, u16::MAX];
                // .bg_color = .{ bg.r, bg.g, bg.b, @intFromFloat(@round(bg_opacity * 255.0)) },
                // Note that if we're on macOS with glass effects we'll disable
                // background opacity but we handle that in updateFrame.
                u.bg_color = [
                    options.config.background.r,
                    options.config.background.g,
                    options.config.background.b,
                    (options.config.background_opacity * 255.0).round() as u8,
                ];
                // .bools = .{ ... },
                u.bools = UniformsBools {
                    cursor_wide: false,
                    use_display_p3: options.config.colorspace == WindowColorspace::DisplayP3,
                    use_linear_blending: options.config.blending.is_linear(),
                    use_linear_correction: options.config.blending
                        == AlphaBlending::LinearCorrected,
                };
                u
            };

            // generic.zig:744-772 custom_shader_uniforms literal: resolution = {0,0,1},
            // frame_rate = 60, focus = 1 (assume focused initially); every other field
            // is 0 / @splat(0). Zeroed POD + the three non-zero fields.
            let custom_shader_uniforms = {
                let mut c: shadertoy::Uniforms = unsafe { core::mem::zeroed() };
                c.resolution = [0.0, 0.0, 1.0];
                c.frame_rate = 60.0; // not currently updated
                c.focus = 1; // assume focused initially
                c
            };

            // .shaders = undefined,  (generic.zig:781) — set by `result.init_shaders()`
            // below (which reads `result.config.custom_shaders`). Rust needs a value:
            // build the base shaders (empty custom set); `init_shaders` re-runs from
            // the real config right after the literal (the golden-harness precedent).
            let shaders = api.init_shaders(&[])?; // errdefer -> Drop

            // .cells = .{},  (generic.zig:717) — an EMPTY Contents. The first
            // `update_frame`/`rebuild_cells` resizes it to the terminal grid size
            // (generic.rs `rebuild_cells`: `if grid_size_diff { self.cells.resize(..) }`).
            let cells = Contents::default();

            let mut result = Self {
                // ---- generic.zig:701-787 (spelled out in the reference literal) ----
                alloc,
                config: options.config,
                // .surface_mailbox = options.surface_mailbox,  (704)
                surface_mailbox: options.surface_mailbox,
                grid_metrics, // font_critical.metrics
                size: options.size,
                focused: true,
                // .visible = true,  (a177ba90a)
                visible: true,
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
                // .bg_image_buffer = undefined,  (773) — fixed by update_bg_image_buffer below.
                bg_image_buffer: unsafe { core::mem::zeroed() },
                font_grid: options.font_grid,
                font_shaper,
                font_shaper_cache: ShaperCache::init(),
                shaders,
                api,
                swap_chain: Some(swap_chain),
                // ---- Zig struct-decl defaults (OMITTED by 701-787; per golden_renderer) ----
                // PORT NOTE (`a177ba90a`): the reference DROPPED `.display_link =
                // display_link` from the 701-787 literal when creation moved into
                // `sync_display_link`, so the field now takes its struct-decl default
                // (generic.zig:205 `display_link: ?DisplayLink = null`) and joins this
                // section.
                display_link: None,
                display_realized: true,
                kitty_animation_clock: None,
                kitty_animation_next_ms: None,
                draw_mutex: Mutex::new(),
                custom_shader_focused_changed: false,
                cells_rebuilt: false,
                first_frame_time: None,
                last_frame_time: None,
                images: image::State::empty(),
                bg_image: None,
                bg_image_changed: false,
                bg_image_buffer_modified: 0,
                health: AtomicHealth::new(crate::Health::Healthy),
                target_config_modified: 0,
                reinitialize_shaders: false,
                // = options.config.custom_shaders != empty (665). Also re-set by
                // init_shaders below (from the loaded custom-shader set) — kept here so
                // the field is initialized before that call, faithful to the reference.
                has_custom_shaders,
                terminal_state: RenderState::empty(),
                terminal_state_frame_count: 0,
                overlay: None,
            };

            // try result.initShaders();   (789)
            result.init_shaders()?;

            // Ensure our undefined values above are correctly initialized.
            result.update_font_grid_uniforms(); // (792)
            result.update_screen_size_uniforms(); // (793)
            result.update_bg_image_buffer(); // (794)
            result.prep_background_image()?; // (795)

            // return result;
            Ok(result)
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
            // if (self.overlay) |*overlay| overlay.deinit(self.alloc);   (801)
            // Port: `Overlay::deinit(&mut self)` takes NO allocator arg (overlay.rs:176).
            if let Some(overlay) = &mut self.overlay {
                overlay.deinit();
            }
            // self.terminal_state.deinit(self.alloc);   (802)
            self.terminal_state.deinit(self.alloc);
            // if (self.search_selected_match) |*m| m.arena.deinit();   (803)
            // if (self.search_matches) |*m| m.arena.deinit();   (804)
            // Port: `SearchMatch`/`SearchMatches` DROPPED the reference `arena`
            // field (message.rs — "same arena collapse"); the owned `Flattened` /
            // `Vec<Flattened>` free via Rust `Drop` when `self.search_*` drops. No
            // explicit `arena.deinit()` exists to call (faithful — the arena teardown
            // is the automatic Option drop).
            //
            // if (self.swap_chain) |*sc| sc.deinit();   (805)
            if let Some(sc) = &mut self.swap_chain {
                sc.deinit();
            }

            // generic.zig:807-813:
            //   if (DisplayLink != void) {
            //       if (self.display_link) |display_link| {
            //           display_link.stop() catch {};
            //           display_link.release();
            //       }
            //   }
            // macOS-only crate ⇒ `DisplayLink != void` is always true (no comptime
            // guard). `stop() catch {}` = ignore the stop error (link may never have
            // started / its view is gone); then `release()` the +1 ref from `init`.
            if let Some(display_link) = self.display_link.as_ref() {
                let _ = display_link.stop(); // catch {}
                display_link.release();
            }

            // self.cells.deinit(self.alloc);   (814)
            // Port: `Contents::deinit(&mut self)` takes NO allocator arg (cell.rs:131).
            self.cells.deinit();

            // self.font_shaper.deinit();   (816)
            self.font_shaper.deinit();
            // self.font_shaper_cache.deinit(self.alloc);   (817)
            // Port: `ShaperCache::deinit(&mut self)` takes NO allocator arg (cache.rs:112).
            self.font_shaper_cache.deinit();

            // self.config.deinit();   (819)
            self.config.deinit();

            // self.images.deinit(self.alloc);   (821)
            self.images.deinit(self.alloc);

            // if (self.bg_image) |img| img.deinit(self.alloc);   (823)
            // Port: `image::Image::deinit(self, alloc)` consumes `self` by value, so
            // move the `Image` out of the `Option` with `.take()` (image.rs:1283).
            if let Some(img) = self.bg_image.take() {
                img.deinit(self.alloc);
            }

            // self.deinitShaders();   (825)
            self.deinit_shaders();

            // self.api.deinit();   (827) -> the `Metal` `Drop` releases queue/device/
            // layer (ARC). An explicit `deinit` is unnecessary AND impossible here (a
            // `&mut self` `deinit` cannot move `api` out); documented.
            //
            // self.* = undefined;   (829) -> no-op (Rust drop/ownership handles teardown).
        }

        // generic.zig:864-869 `surfaceInit`. `@hasDecl(GraphicsAPI, "surfaceInit")` is
        // FALSE for `Metal` (it declares only `loopEnter`) ⇒ the probe's else-arm ⇒
        // a no-op. `rt_surface` is the apprt surface the hook is generic over (never
        // named by rustty-renderer, R4); Metal ignores it. The bare unbounded `<S>` is
        // exactly faithful — the reference `@hasDecl` probe likewise never touches
        // `surface` for the Metal monomorphization.
        //
        // pub fn surfaceInit(surface: *apprt.Surface) !void {
        //     if (@hasDecl(GraphicsAPI, "surfaceInit"))
        //         try GraphicsAPI.surfaceInit(surface);
        // }
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub fn surface_init<S>(rt_surface: *mut S) -> Result<(), Error> {
            let _ = rt_surface;
            Ok(())
        }

        // generic.zig:873-878 `finalizeSurfaceInit`. `@hasDecl(GraphicsAPI,
        // "finalizeSurfaceInit")` is FALSE for `Metal` ⇒ no-op (see `surface_init`).
        //
        // pub fn finalizeSurfaceInit(self: *Self, surface: *apprt.Surface) !void {
        //     if (@hasDecl(GraphicsAPI, "finalizeSurfaceInit"))
        //         try self.api.finalizeSurfaceInit(surface);
        // }
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub fn finalize_surface_init<S>(&mut self, rt_surface: *mut S) -> Result<(), Error> {
            let _ = rt_surface;
            Ok(())
        }

        /// W-ATTACH (macos-swift-app tier): attach the (offscreen-built) graphics
        /// API to a live host NSView, so frames present to the window instead of a
        /// dropped offscreen `IOSurface`. Core calls this from `Surface::init`
        /// (right after `finalizeSurfaceInit`, before the render thread spawns)
        /// with the raw view pointer it reads from `rt_surface.host_view()` — the
        /// renderer never names an apprt/app-kit type (R4). No-op on a null view
        /// (headless / non-hosted backends).
        #[cfg(target_os = "macos")]
        pub fn attach_host_view(
            &mut self,
            view: *mut core::ffi::c_void,
            scale: f64,
        ) -> Result<(), Error> {
            if view.is_null() {
                return Ok(());
            }
            // SAFETY: `view` is the live NSView the apprt surface was created with
            // (host contract); reinterpret the type-erased pointer as `&AnyObject`
            // for the objc2 layer-hosting messages inside `Metal::attach_view`.
            let obj = unsafe { &*(view as *const objc2::runtime::AnyObject) };
            self.api.attach_view(obj, scale)?;
            Ok(())
        }

        /// Non-macOS: no windowed Metal host-attach (offscreen/other backends).
        #[cfg(not(target_os = "macos"))]
        pub fn attach_host_view(
            &mut self,
            _view: *mut core::ffi::c_void,
            _scale: f64,
        ) -> Result<(), Error> {
            Ok(())
        }

        // generic.zig:881-886 threadEnter. `@hasDecl(Metal,"threadEnter")` is FALSE
        // (Metal declares only loopEnter) ⇒ no api call; `surface` is the apprt
        // surface the hook is generic over (never named, R4).
        //   pub fn threadEnter(self, surface: *apprt.Surface) !void {
        //       if (@hasDecl(GraphicsAPI, "threadEnter")) try self.api.threadEnter(surface);
        //   }
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub fn thread_enter<S>(&self, rt_surface: *mut S) -> Result<(), Error> {
            let _ = rt_surface;
            Ok(())
        }

        // generic.zig:889-894 threadExit. `@hasDecl(Metal,"threadExit")` FALSE ⇒ no-op.
        //   pub fn threadExit(self) void {
        //       if (@hasDecl(GraphicsAPI, "threadExit")) self.api.threadExit();
        //   }
        pub fn thread_exit(&self) {}

        // generic.zig:891-903 loopEnter. Adapted for R4: the reference takes
        // `thr: *renderer.Thread` and reads `&thr.draw_now`; we cannot name
        // `renderer.Thread` (R4), so the caller passes the `draw_now: xev::Async`
        // field directly (Copy+Send handle — NOT an apprt type).
        //   pub fn loopEnter(self, thr: *renderer.Thread) !void {
        //       if (@hasDecl(GraphicsAPI, "loopEnter")) self.api.loopEnter();
        //       if (comptime DisplayLink == void) return;      // (macOS: never)
        //       self.syncDisplayLink(null, &thr.draw_now);
        //   }
        pub fn loop_enter(&mut self, draw_now: Async) -> Result<(), Error> {
            // Metal DECLARES loopEnter ⇒ this hook fires. It installs the
            // IOSurfaceLayer display callback (W5). container_of needs WHOLE-Renderer
            // provenance (`draw_frame_raw` derefs all of Renderer), so hand the leaf a
            // raw `*mut Metal` derived from the whole-`Renderer` `self_ptr` — NOT a
            // narrowing `&mut self.api` reborrow. See `W5_DESIGN §2.4`.
            let self_ptr: *mut Renderer = self;
            let api_ptr: *mut Metal = unsafe { &raw mut (*self_ptr).api };
            unsafe { Metal::loop_enter(api_ptr) };
            // `if (comptime DisplayLink == void) return;` — macOS: DisplayLink is
            // never void; no early return.
            //
            // `self.syncDisplayLink(null, &thr.draw_now)` (a177ba90a): this is the
            // one call site that can CREATE the link, because it is the only one
            // holding the `draw_now` the output callback needs.
            self.sync_display_link(None, Some(draw_now));
            Ok(())
        }

        // generic.zig:919-933 loopExit. `@hasDecl(Metal,"loopExit")` FALSE ⇒ no api
        // call ⇒ FULLY runnable (unlike loopEnter). Stop the link; error ignored.
        //   pub fn loopExit(self) void {
        //       if (@hasDecl(GraphicsAPI, "loopExit")) self.api.loopExit();
        //       if (comptime DisplayLink == void) return;
        //       const display_link = self.display_link orelse return;
        //       display_link.stop() catch {};
        //   }
        pub fn loop_exit(&self) {
            if let Some(display_link) = self.display_link.as_ref() {
                let _ = display_link.stop(); // catch {}
            }
        }

        // generic.zig:986-994 displayLinkCallback. The body is INLINED into the
        // `cvdisplaylink::output_callback` trampoline (W2-T1) because a CoreVideo
        // `extern "C"` callback cannot be a `Self` method and must run without a
        // `Renderer` receiver. Kept as an R1 doc-anchor here; the live copy is in
        // cvdisplaylink.rs.
        //   fn displayLinkCallback(_: *macos.video.DisplayLink, ud: ?*xev.Async) void {
        //       const draw_now = ud orelse return;
        //       draw_now.notify() catch |err| log.err("error notifying draw_now err={}", .{err});
        //   }

        // generic.zig:988-996 setMacOSDisplayID. FULLY runnable.
        //   pub fn setMacOSDisplayID(
        //       self: *Self,
        //       id: u32,
        //       draw_now: *xev.Async,
        //   ) !void {
        //       if (comptime DisplayLink == void) return;
        //       self.syncDisplayLink(id, draw_now);
        //   }
        //
        // PORT NOTE (`a177ba90a`): `&self` -> `&mut self` and the new `draw_now`
        // parameter, both consequences of routing through `sync_display_link`: a
        // display change is now allowed to CREATE the link (that is the "when a
        // display becomes available it re-adds itself" half of the commit), which
        // needs both the mutable field and the output-callback userdata. The R4
        // adaptation of `*xev.Async` matches `loop_enter`'s: the Copy+Send handle
        // by value.
        pub fn set_macos_display_id(&mut self, id: u32, draw_now: Async) -> Result<(), Error> {
            self.sync_display_link(Some(id), Some(draw_now));
            Ok(())
        }

        // generic.zig:1017-1024 hasVsync. FULLY runnable.
        //   pub fn hasVsync(self) bool {
        //       if (comptime DisplayLink == void) return false;
        //       const display_link = self.display_link orelse return false;
        //       return display_link.isRunning();
        //   }
        pub fn has_vsync(&self) -> bool {
            match self.display_link.as_ref() {
                Some(display_link) => display_link.is_running(),
                None => false,
            }
        }

        // generic.zig:1090-1101 setFocus. FULLY runnable. NB: keeps the reference
        // `assert(self.focused != focus)` + the custom-shader flag + the sync.
        //   pub fn setFocus(self, focus: bool) !void {
        //       assert(self.focused != focus);
        //       self.focused = focus;
        //
        //       // Flag that we need to update our custom shaders
        //       self.custom_shader_focused_changed = true;
        //
        //       self.syncDisplayLink(null, null);
        //   }
        pub fn set_focus(&mut self, focus: bool) -> Result<(), Error> {
            assert!(self.focused != focus);
            self.focused = focus;

            // Flag that we need to update our custom shaders
            self.custom_shader_focused_changed = true;

            // PORT NOTE (`a177ba90a`): the inlined `if (focus) start() else stop()`
            // moved into `sync_display_link`, which gates on visibility TOO —
            // focusing an occluded surface no longer spins vsync back up.
            self.sync_display_link(None, None);
            Ok(())
        }

        /// Create or update the display link and match it to the current
        /// surface state.
        // generic.zig:1121-1153 syncDisplayLink (NEW in `a177ba90a`).
        //   fn syncDisplayLink(
        //       self: *Self,
        //       display_id: ?u32,
        //       draw_now: ?*xev.Async,
        //   ) void {
        //       if (comptime DisplayLink == void) return;
        //
        //       const display_link = self.display_link orelse display_link: {
        //           if (!self.config.vsync) return;
        //           const callback = draw_now orelse return;
        //           const result = macos.video.DisplayLink.createWithActiveCGDisplays() catch |err| {
        //               // A locked macOS session can temporarily have no active
        //               // displays. Rendering can continue without vsync and a
        //               // later display update will retry this method.
        //               log.warn("error creating display link; using fallback rendering err={}", .{err});
        //               return;
        //           };
        //           result.setOutputCallback(
        //               xev.Async,
        //               &displayLinkCallback,
        //               callback,
        //           ) catch |err| {
        //               log.warn("error configuring display link err={}", .{err});
        //               result.release();
        //               return;
        //           };
        //
        //           self.display_link = result;
        //           log.info("created display link", .{});
        //           break :display_link result;
        //       };
        //
        //       if (display_id) |id| {
        //           log.info("updating display link display id={}", .{id});
        //           display_link.setCurrentCGDisplay(id) catch |err| {
        //               log.warn("error setting display link display id err={}", .{err});
        //           };
        //       }
        //
        //       const should_run =
        //           // Non-visible windows never vsync
        //           self.visible and
        //           // Only vsync if we have cell changes or animation
        //           (self.cells_rebuilt or self.animationWake() != null);
        //
        //       if (should_run) {
        //           if (!display_link.isRunning()) {
        //               display_link.start() catch {};
        //           }
        //       } else {
        //           display_link.stop() catch {};
        //       }
        //   }
        //
        // PORT NOTE (`97f57edcc` + `6688aa072`, phase-6): the start/stop predicate
        // is no longer `visible and focused`. `97f57edcc` (vsync unfocused
        // surfaces while dirty) dropped `focused` — an unfocused-but-visible
        // surface with pending cell changes must still be presented on vsync —
        // and `6688aa072` (park DisplayLink while idle) gates the link on
        // `cells_rebuilt or animationWake() != null`, so an idle surface parks
        // the link between frames; `updateFrame`'s tail and `drawFrame`'s
        // no-redraw arm resync it. `set_focus` still calls this (the reference
        // kept the call) — it now only matters through `animationWake`'s
        // focus-gated custom-shader cadence.
        //
        // PORT NOTE: the reference's `orelse ... break :display_link result` labelled
        // block yields a `DisplayLink` VALUE (a `*DisplayLink` pointer in Zig, so the
        // copy and `self.display_link` denote the same CoreVideo object). Rust's
        // `DisplayLink` is a NonNull wrapper that owns the boxed callback userdata and
        // is NOT `Copy`, so the port keeps the link in the field throughout and
        // re-borrows it after the creation arm — same object, no aliasing copy. The
        // creation arm therefore assigns `self.display_link` and falls through rather
        // than yielding a value.
        //
        // `if (comptime DisplayLink == void) return;` — macOS: DisplayLink is never
        // void; no early return.
        fn sync_display_link(&mut self, display_id: Option<u32>, draw_now: Option<Async>) {
            if self.display_link.is_none() {
                // if (!self.config.vsync) return;
                if !self.config.vsync {
                    return;
                }
                // const callback = draw_now orelse return;
                let Some(callback) = draw_now else {
                    return;
                };
                // A locked macOS session can temporarily have no active
                // displays. Rendering can continue without vsync and a
                // later display update will retry this method.
                let mut result = match DisplayLink::create_with_active_cg_displays() {
                    Ok(r) => r,
                    Err(err) => {
                        // log.warn("error creating display link; using fallback rendering err={}", err);
                        log::warn!(
                            target: LOG_SCOPE,
                            "error creating display link; using fallback rendering err={err:?}"
                        );
                        return;
                    }
                };
                if let Err(err) = result.set_output_callback(callback) {
                    // log.warn("error configuring display link err={}", err);
                    log::warn!(target: LOG_SCOPE, "error configuring display link err={err:?}");
                    result.release();
                    return;
                }

                self.display_link = Some(result);
                // log.info("created display link", .{});
                log::info!(target: LOG_SCOPE, "created display link");
            }

            // The `orelse` arm above either returned or assigned, so the link is
            // present here — the re-borrow standing in for the reference's copy.
            let Some(display_link) = self.display_link.as_ref() else {
                return;
            };

            // if (display_id) |id| { ... }
            if let Some(id) = display_id {
                // log.info("updating display link display id={}", id);
                log::info!(target: LOG_SCOPE, "updating display link display id={id}");
                if let Err(err) = display_link.set_current_cg_display(id) {
                    // log.warn("error setting display link display id err={}", err);
                    log::warn!(target: LOG_SCOPE, "error setting display link display id err={err:?}");
                }
            }

            let should_run =
                // Non-visible windows never vsync
                self.visible &&
                // Only vsync if we have cell changes or animation
                (self.cells_rebuilt || self.animation_wake().is_some());

            if should_run {
                if !display_link.is_running() {
                    let _ = display_link.start(); // catch {}
                }
            } else {
                let _ = display_link.stop(); // catch {}
            }
        }

        /// Callback when the window is visible or occluded.
        ///
        /// Must be called on the render thread.
        // generic.zig:1103-1119 setVisible. FULLY runnable.
        //   pub fn setVisible(self: *Self, visible: bool) void {
        //       self.visible = visible;
        //       self.syncDisplayLink(null, null);
        //
        //       // When we're hidden, release our GPU resources if GPU
        //       // operations are allowed from this thread. Apprts where
        //       // they aren't (GTK owns the OpenGL context on the app
        //       // thread) call `releaseGpuResources` at the appropriate
        //       // time instead.
        //       if (comptime !apprt.must_draw_from_app_thread) {
        //           if (!visible) self.releaseGpuResources();
        //       }
        //   }
        //
        // (`&mut self` now: `releaseGpuResources` tears down the swap chain.)
        pub fn set_visible(&mut self, visible: bool) {
            // PORT NOTE (`a177ba90a`): the start/stop pair that used to be inlined
            // here moved into `sync_display_link`, and the visibility it decided on
            // is now STORED so every other sync (focus change, display change,
            // lazy creation) can consult it.
            self.visible = visible;
            self.sync_display_link(None, None);

            // When we're hidden, release our GPU resources if GPU
            // operations are allowed from this thread. Apprts where
            // they aren't (GTK owns the OpenGL context on the app
            // thread) call `releaseGpuResources` at the appropriate
            // time instead.
            //
            // PORT NOTE (`c4e16970a`, monomorphic fold): this Metal twin serves
            // the macOS/embedded apprt, whose `must_draw_from_app_thread` is
            // FALSE (`rustty_apprt_api::App::MUST_DRAW_FROM_APP_THREAD`
            // default; the GTK/OpenGL twin folds the `true` arm and skips the
            // release). The `comptime !apprt.must_draw_from_app_thread` guard
            // is therefore statically taken here.
            if !visible {
                self.release_gpu_resources();
            }
        }

        /// Release the GPU resources we hold while the surface is not
        /// visible. Today this is the swap chain (render targets, font
        /// atlas texture copies, cell buffers, custom shader textures),
        /// which makes up nearly all of a surface's GPU memory usage;
        /// a hidden surface doesn't draw, so it doesn't need it. The
        /// swap chain is rebuilt on the next `drawFrame`.
        ///
        /// This is safe to call in any state; resources that are
        /// already released are skipped.
        ///
        /// For OpenGL this must be called on the app thread with the
        /// GL context current (the same requirement as
        /// `displayUnrealized`). Other APIs may call this from the
        /// render thread; see `apprt.must_draw_from_app_thread`.
        //
        // pub fn releaseGpuResources(self: *Self) void {
        //     self.draw_mutex.lockUncancelable(global.io());
        //     defer self.draw_mutex.unlock(global.io());
        //     if (self.swap_chain) |*sc| {
        //         sc.deinit();
        //         self.swap_chain = null;
        //         if (comptime @hasDecl(GraphicsAPI, "gpuResourcesReleased")) {
        //             self.api.gpuResourcesReleased();
        //         }
        //     }
        // }
        //
        // SAFETY (assessment D hazard #3): `SwapChain::deinit` WAITS on
        // `frame_sema` for every in-flight frame before freeing, and
        // `frame_completed` (the only cross-thread reader of `swap_chain`)
        // posts that semaphore as its LAST touch of the chain — so the
        // `.unwrap()` deref there can never observe the teardown. The
        // draw_mutex serializes this against `drawFrame`.
        pub fn release_gpu_resources(&mut self) {
            self.draw_mutex.lock();
            // `defer self.draw_mutex.unlock()`: INFALLIBLE body (no `?`) => the
            // end unlock is reached on every non-panic path (the
            // `set_font_grid` precedent).

            if let Some(sc) = &mut self.swap_chain {
                // Waits for any in-flight frames to complete, then
                // frees all GPU resources.
                sc.deinit();
                self.swap_chain = None;

                // Let the API drop any references it holds to swap
                // chain resources (e.g. OpenGL's last presented
                // target).
                //
                // (`@hasDecl(GraphicsAPI, "gpuResourcesReleased")` — the Metal
                // leaf does NOT declare it, so the comptime arm folds to
                // nothing on this twin; the OpenGL twin calls it.)
            }

            self.draw_mutex.unlock();
        }

        /// Mark the full screen as dirty so that we redraw everything.
        //
        // Zig: pub inline fn markDirty(self: *Self) void {
        //          self.terminal_state.dirty = .full;
        //      }
        pub fn mark_dirty(&mut self) {
            self.terminal_state.dirty = Dirty::Full;
        }

        /// Set the new font grid.
        ///
        /// Must be called on the render thread.
        //
        // Zig: pub fn setFontGrid(self: *Self, grid: *font.SharedGrid) void
        // The reference takes a safe `*font.SharedGrid` arg it dereferences; keep the
        // faithful safe `pub fn` signature (the raw deref is the external shared-grid
        // handle, §4.4 — not the `rust-mut-args-strongly-protected` pattern).
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub fn set_font_grid(&mut self, grid: *mut SharedGrid) {
            self.draw_mutex.lock();
            // `defer self.draw_mutex.unlock()`: this body is INFALLIBLE (no `?`), so
            // the single unlock at the natural end is reached on every non-panic path
            // — faithful to the reference `defer`, which (Zig, no unwind) likewise
            // runs only on the normal return.

            // Update our grid
            self.font_grid = grid;

            // Update all our textures so that they sync on the next frame.
            // We can modify this without a lock because the GPU does not
            // touch this data. A released swap chain is rebuilt with
            // fresh frames that sync all textures on first use.
            // if (self.swap_chain) |*sc| for (&sc.frames) |*frame| { ... };
            if let Some(sc) = &mut self.swap_chain {
                for frame in &mut sc.frames {
                    frame.grayscale_modified = 0;
                    frame.color_modified = 0;
                }
            }

            // Get our metrics from the grid. This doesn't require a lock because
            // the metrics are never recalculated.
            //
            // SAFETY: `grid` is the external `*mut SharedGrid` handle (not reborrowed
            // from `&mut self`); `(*grid).metrics` is a shared read through a raw
            // pointer with no `&mut` alias to `*grid` — not the
            // `rust-mut-args-strongly-protected` pattern, no SB hazard (§4.4).
            let metrics = unsafe { (*grid).metrics };
            self.grid_metrics = metrics;

            // Reset our shaper cache. If our font changed (not just the size) then
            // the data in the shaper cache may be invalid and cannot be used, so we
            // always clear the cache just in case.
            //
            // Zig: `self.font_shaper_cache.deinit(self.alloc)` — the ported
            // `ShaperCache::deinit(&mut self)` takes NO allocator arg (cache.rs:112).
            let font_shaper_cache = ShaperCache::init();
            self.font_shaper_cache.deinit();
            self.font_shaper_cache = font_shaper_cache;

            // Update cell size.
            self.size.cell = CellSize {
                width: metrics.cell_width,
                height: metrics.cell_height,
            };

            // Update relevant uniforms
            self.update_font_grid_uniforms();

            // Force a full rebuild, because cached rows may still reference
            // an outdated atlas from the old grid and this can cause garbage
            // to be rendered.
            self.mark_dirty();

            self.draw_mutex.unlock();
        }

        /// Resize the screen.
        //
        // Zig: pub fn setScreenSize(self: *Self, size: renderer.Size) void
        pub fn set_screen_size(&mut self, size: Size) {
            self.draw_mutex.lock();
            // `defer self.draw_mutex.unlock()`: INFALLIBLE body (no `?`) => the end
            // unlock is reached on every non-panic path (as in `set_font_grid`).

            // We only actually need the padding from this,
            // everything else is derived elsewhere.
            self.size.padding = size.padding;

            self.update_screen_size_uniforms();

            // log.debug("screen size size={}", .{size});
            log::debug!(target: LOG_SCOPE, "screen size size={size:?}");

            self.draw_mutex.unlock();
        }

        /// PORT NOTE (rustty-only glue, additive): no reference counterpart — see
        /// `Metal::set_offscreen_size`, which this forwards to. Ghostty's renderer
        /// never sizes its own surface: `Metal.surfaceSize` reads the attached
        /// layer's bounds, so the windowing system resizes it. rustty's OFFSCREEN
        /// slice has no layer (`init_offscreen`, layer `None`), so an embedder that
        /// composites the finished IOSurface itself needs this to make its surface
        /// follow the pane. Purely additive: nothing in the reference lane calls it,
        /// and on-screen (`layer.is_some()`) it has no effect at all.
        ///
        /// Separate from [`Self::set_screen_size`] on purpose, and *not* folded into
        /// it: that one is the reference's `setScreenSize`, which owns only
        /// `size.padding` (plus the uniforms derived from it), while `size.screen`
        /// belongs to `draw_frame`, which re-derives it from `api.surface_size()` and
        /// resizes the swap-chain target in the same breath. So the two compose:
        /// this call changes what the *next* `draw_frame` will see, and a padding
        /// update from the embedder's terminal size is still `set_screen_size`'s job.
        ///
        /// Takes `draw_mutex`, as `set_screen_size` does, so the write cannot land
        /// midway through a concurrent `draw_frame`'s size handling.
        pub fn set_offscreen_size(&mut self, size: ScreenSize) {
            self.draw_mutex.lock();
            // `defer self.draw_mutex.unlock()`: INFALLIBLE body (no `?`) => the end
            // unlock is reached on every non-panic path (as in `set_screen_size`).

            self.api.set_offscreen_size(size.width, size.height);

            log::debug!(target: LOG_SCOPE, "offscreen size size={size:?}");

            self.draw_mutex.unlock();
        }

        // pub fn changeConfig(self: *Self, config: *DerivedConfig) !void
        //
        // SIGNATURE PORT NOTE (§4.3): the reference does a shallow `self.config = config.*`
        // (a Zig move-by-copy) and relies on the caller's later `Message.deinit`. In
        // Rust `DerivedConfig` is non-`Copy` (owns `ArenaAllocator`/`Vec`/`link::Set`),
        // so this is ported as by-value ownership transfer; the `Message::ChangeConfig`
        // payload holds `Box<DerivedConfig>`, so the caller passes ownership (the
        // reference `v.impl.deinit()` collapses into the old-config drop).
        //
        // `defer self.draw_mutex.unlock()` FIDELITY: the reference releases the mutex
        // on EVERY exit — including the `try font.Shaper.init(...)` error return. The
        // ported `Mutex` is bare lock/unlock with NO RAII guard; a scope-guard holding
        // `&self.draw_mutex` would conflict with the body's `&mut self` calls
        // (`update_bg_image_buffer`/`mark_dirty`/field writes) once those reborrow
        // `self` mutably. So `defer` is replicated by explicit unlock on every early
        // return (unlock-before-`?`) plus the unlock at the natural end.
        /// Update the configuration.
        pub fn change_config(&mut self, config: DerivedConfig) -> Result<(), Error> {
            self.draw_mutex.lock();

            // We always redo the font shaper in case font features changed. We
            // could check to see if there was an actual config change but this is
            // easier and rare enough to not cause performance issues.
            {
                // Zig: `var font_shaper = try font.Shaper.init(self.alloc, .{
                //          .features = config.font_features.items });`
                // The `[:0]const u8` items -> the shaper's `Vec<Vec<u8>>` features
                // (the RAW bytes, matching the Zig `.len`-based content).
                // unlock-before-`?` (defer-equivalent).
                let font_shaper = match Shaper::init(shape::Options {
                    features: config.font_features.clone(),
                }) {
                    Ok(s) => s,
                    Err(e) => {
                        self.draw_mutex.unlock();
                        return Err(e.into());
                    }
                };
                // errdefer font_shaper.deinit();
                self.font_shaper.deinit();
                self.font_shaper = font_shaper;
            }

            // We also need to reset the shaper cache so shaper info
            // from the previous font isn't reused for the new font.
            //
            // Zig: `self.font_shaper_cache.deinit(self.alloc)` — NO arg (cache.rs:112).
            let font_shaper_cache = ShaperCache::init();
            self.font_shaper_cache.deinit();
            self.font_shaper_cache = font_shaper_cache;

            // Set our new minimum contrast
            self.uniforms.min_contrast = config.min_contrast;

            // Set our new color space and blending
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
            // `Path` derives `PartialEq`, so `old != new` == `!old.equal(new)`. Now
            // that `bg_image` is a real `Option<Path>` this genuinely change-detects
            // (None->Some, Some->different, Some->same) and gates `prepBackgroundImage`.
            let bg_image_changed = match (&self.config.bg_image, &config.bg_image) {
                (Some(old), Some(new)) => old != new,
                (Some(_), None) => true,
                (None, Some(_)) => true,
                (None, None) => false,
            };

            let old_blending = self.config.blending;
            // Zig: `!self.config.custom_shaders.equal(config.custom_shaders)`.
            // The derived `PartialEq` on `RepeatablePath` (`Vec<Path>` — length +
            // per-element variant + `CString` bytes) reproduces the reference `equal`
            // exactly, so `!=` is `!equal(...)`.
            let custom_shaders_changed = self.config.custom_shaders != config.custom_shaders;

            // Zig: `self.config.deinit(); self.config = config.*;` -> replace the
            // config (moving the new one in), then `deinit` + drop the old one.
            let mut old = mem::replace(&mut self.config, config);
            old.deinit();

            // If our background image path changed, prepare the new bg image.
            // if (bg_image_changed) try self.prepBackgroundImage();
            //
            // `prep_background_image` decodes the new path (wuffs PNG/JPEG) into a
            // pending `bg_image`; only a DECODE error propagates (open/read failures
            // warn + no-op — §3.3). unlock-before-`?` (the `Mutex` is guard-less, so
            // `defer draw_mutex.unlock()` is replicated on every early return).
            if bg_image_changed {
                if let Err(e) = self.prep_background_image() {
                    self.draw_mutex.unlock();
                    return Err(e);
                }
            }

            // If our background image config changed, update the vertex buffer.
            if bg_image_config_changed {
                self.update_bg_image_buffer();
            }

            // Reset our viewport to force a rebuild, in case of a font change.
            self.mark_dirty();

            // FIDELITY (§4.3): the reference reads `config.blending` here (the local
            // param), which — after the move into `self.config` above — equals
            // `self.config.blending`; the Rust `mem::replace` moved `config`, so the
            // trailing reads use `self.config` (== the new config), the same values.
            let blending_changed = old_blending != self.config.blending;

            if blending_changed {
                // We update our API's blending mode.
                self.api.blending = self.config.blending;
                // And indicate that we need to reinitialize our shaders.
                self.reinitialize_shaders = true;
                // And indicate that our swap chain targets need to
                // be re-created to account for the new blending mode.
                // Zig `+%=` -> `wrapping_add`.
                self.target_config_modified = self.target_config_modified.wrapping_add(1);
            }

            if custom_shaders_changed {
                self.reinitialize_shaders = true;
            }

            self.draw_mutex.unlock();
            Ok(())
        }

        /// Call this any time the background image path changes.
        ///
        /// Caller must hold the draw mutex.
        // fn prepBackgroundImage(self: *Self) !void {  (ref generic.zig:1747)
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
            // ADVERSARY F6 — borrow discipline: `p` borrows `self.config` immutably,
            // but the load block below mutates `self.bg_image` (`mark_for_replace`/the
            // `self.bg_image = ...` set). Copy the null-terminated path bytes into an
            // OWNED `CString` here and drop the `&self.config` borrow before touching
            // `self.bg_image`. Purely mechanical; no fidelity impact — the reference's
            // `path` is a plain slice into config memory, but Rust's aliasing rules
            // require the early copy.
            let path: CString = p.slice().to_owned();

            // `self.alloc` is a Copy `Global` ZST; capture it so the `'load_background`
            // block reads it without holding a `&self` borrow across the later
            // `self.bg_image` mutation.
            let alloc = self.alloc;

            // 'load_background: { ... break :load_background; } — a labeled block whose
            // early exits (open-fail / read-fail / unknown-type) `break` with `None`
            // ("no change"); a DECODE error `?`-propagates OUT of the whole method
            // (the reference `try`), NOT a `break` (§3.3 asymmetry).
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
                // `{s}` on the reference's `[]const u8` path prints the RAW bytes; the
                // closest Rust rendering is `String::from_utf8_lossy` over the same
                // bytes. The lossy conversion is confined to the log message — the path
                // the port actually opens stays the byte-faithful `OsStr`.
                let path_bytes = path.to_bytes();
                let path_display = String::from_utf8_lossy(path_bytes);
                // var file = std.fs.openFileAbsolute(path, .{}) catch |err| { log.warn(...); break; };
                let mut file = match std::fs::File::open(OsStr::from_bytes(path_bytes)) {
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
                // const contents = file.readToEndAlloc(self.alloc, std.math.maxInt(u32)) catch |err| { log.warn(...); break; };
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
                    FileType::Unknown => {
                        FileType::guessFromExtension(path_extension(path.to_bytes()))
                    }
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
                // align 1, so `image.rs::free_data`'s `Layout::from_size_align(len, 1)`
                // frees it later without a layout mismatch — §3.2/R-2). `contents`
                // (the encoded bytes) IS freed here, at the block's end (`Vec` drop ==
                // the reference `defer self.alloc.free(contents)`).
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
                let image = image::Image::Pending(pend);
                // if (self.bg_image) |*img| { img.markForReplace(self.alloc, image); }
                // else { self.bg_image = image; }
                if let Some(img) = self.bg_image.as_mut() {
                    img.mark_for_replace(self.alloc, image);
                } else {
                    self.bg_image = Some(image);
                }
            }

            Ok(())
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

    /// PORT NOTE: the un-caught error sources of `rebuild_row` (the reference `!void`).
    /// The only `try`s that are NOT `catch`-logged inline are `run_iter.next` (a
    /// `collection::Error`) and `font_shaper.shape` (the active shaper's `Error`,
    /// `coretext` on macOS). `rebuild_cells` catches this → `clear(y)` + log. (The
    /// `add_*` calls are `catch`-logged inline, mapping to `.unwrap_or_else`.)
    //
    // PORT NOTE: the wrapped payloads carry the faithful error value the reference
    // logs (`log.warn("error building row y={} err={}", .{ y, err })` in
    // `rebuildCells`, oracle `generic.zig:2627`). That site is now LIVE — the payload
    // reaches the restored `log::warn!` through its `{err:?}` Debug rendering. The
    // `#[allow(dead_code)]` stays because rustc's `dead_code` lint does not count a
    // `#[derive(Debug)]` field read as a use, so the variant payloads would still be
    // reported "never read" without it.
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
                let cells_ptr: *const render::CellList =
                    &self.terminal_state.row_data[y_usize].cells;
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
                        self.uniforms.padding_extend.up = !never_extend_bg(
                            row,
                            cells_raw,
                            cells_slice.style,
                            &palette,
                            background,
                        );
                    } else if y == self.cells.size.rows - 1 {
                        self.uniforms.padding_extend.down = !never_extend_bg(
                            row,
                            cells_raw,
                            cells_slice.style,
                            &palette,
                            background,
                        );
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
                                    // LOG PORT NOTE (task-14c-1, row S116-F2): this
                                    // `log.warn` is NOT restorable. `ShaperCache::put`
                                    // (`rustty-font-shaper/src/shaper/cache.rs:136`) is
                                    // INFALLIBLE in rustty — it allocates through the
                                    // infallible `Global`/`Box` path, so there is no
                                    // `catch` arm to hang the warn on and emitting it
                                    // unconditionally would put a diagnostic on the
                                    // SUCCESS path. Same shape as the `graphics_storage`
                                    // OOM waiver (ADJUDICATION.md#s190-f2). S116-F2 is
                                    // therefore WAIVED, not CLOSED.
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
                                    // self.font_shaper_cache.put(self.alloc, run, new_cells) catch |err| {
                                    //     log.warn("error caching font shaping results err={}", .{err});
                                    // };
                                    //
                                    // LOG PORT NOTE (task-14c-1, row S116-F2): this
                                    // `log.warn` is NOT restorable. `ShaperCache::put`
                                    // (`rustty-font-shaper/src/shaper/cache.rs:136`) is
                                    // INFALLIBLE in rustty — it allocates through the
                                    // infallible `Global`/`Box` path, so there is no
                                    // `catch` arm to hang the warn on and emitting it
                                    // unconditionally would put a diagnostic on the
                                    // SUCCESS path. Same shape as the `graphics_storage`
                                    // OOM waiver (ADJUDICATION.md#s190-f2). S116-F2 is
                                    // therefore WAIVED, not CLOSED.
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
                self.terminal_state_frame_count = 0;
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

            // Hand-rolled `defer state.unlockDemand()` (Chunk 3c §C.4/RISK-5): the
            // guard-less `rustty_zigstd::thread::Mutex` has no RAII drop, and the lock
            // is held across THREE exits — the sync-output early `return`, the
            // `terminal_state.update(...)?` propagation (the sole propagating `?`,
            // INSIDE the lock), and normal fall-through. This `Drop` fires on all
            // three. Sound: it borrows `state` SHAREDLY (and through it the mutex
            // allocation), disjoint from `self`, so it does not clash with the
            // `&mut self.terminal_state` / `&mut *t` uses inside the block.
            //
            // (D4, oracle d34b54e9b: the guard borrows the whole `&State` rather than
            // just `&Mutex` because the release path is now `unlock_demand`, which
            // must also bump the handoff generation and wake parked yielders.)
            struct Unlock<'a>(&'a state::State);
            impl Drop for Unlock<'_> {
                fn drop(&mut self) {
                    // SAFETY: as for the `lock_demand` below — `state.mutex` is the
                    // live shared render mutex, held by this frame.
                    unsafe { self.0.unlock_demand() };
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

                // Lock while signaling demand so the IO parse thread can't starve
                // us. See renderer::State::lock_demand.
                //
                // SAFETY: `state.mutex` points at the live shared render-state mutex
                // and this frame does not already hold it.
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

                // Begin the update of our terminal state. Work that
                // doesn't require terminal access (e.g. style
                // denormalization) is deferred to the end_update call
                // outside of this critical section, keeping our lock
                // hold time as short as possible.
                self.terminal_state
                    .begin_update(self.alloc, unsafe { &mut *t })?;

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

                // Advance any running Kitty graphics animations to the
                // frame due now, and remember when the next frame is
                // due (as an absolute deadline, see animationWake) so
                // the renderer thread can schedule a wakeup for it.
                // This must happen before the dirty check below:
                // advancing a frame marks the image state dirty.
                //
                // self.kitty_animation_next_ms = next: {
                //     // Likely case: we have no kitty images, so do nothing.
                //     const storage = &state.terminal.screens.active.kitty_images;
                //     if (storage.images.count() == 0) break :next null;
                //
                //     const now: std.Io.Timestamp = .now(global.io(), .awake);
                //     const base = self.kitty_animation_clock orelse base: {
                //         self.kitty_animation_clock = now;
                //         break :base now;
                //     };
                //     const now_ms: u64 = @intCast(@divTrunc(
                //         base.durationTo(now).nanoseconds,
                //         std.time.ns_per_ms,
                //     ));
                //     const delay = storage.animationTick(
                //         global.io(),
                //         now_ms,
                //     ) orelse break :next null;
                //     break :next now_ms + delay;
                // };
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
                    let delay = match unsafe { &mut (*active).kitty_images }.animation_tick(now_ms)
                    {
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

                let overlay_features: Vec<crate::overlay::Feature> = match state.inspector {
                    //   const insp = state.inspector orelse break :overlay &.{};
                    //   const renderer_info = insp.rendererInfo();
                    //   break :overlay renderer_info.overlayFeatures(arena_alloc) catch &.{};
                    //
                    // R2 ACCESSOR-TRAIT SEAM (was DEFERRED, Task 5): `state.inspector`
                    // is now the renderer-owned `*mut dyn InspectorOverlay` fat pointer
                    // (state.rs) — the acyclic accessor-trait (INV-7), the exact shape
                    // `RendererConfig` uses for the config edge. The renderer reads the
                    // enabled overlay features THROUGH the trait, never naming a
                    // `rustty-inspector` type. The reference's `insp.rendererInfo()`
                    // navigation happens at STORE time (core coerces the `Info` into the
                    // fat pointer), so here the one-method trait call subsumes the whole
                    // chain. INV-1: this deref is inside the `state.mutex`-guarded
                    // critical section; the raw pointer never escapes the guard.
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

            // Outside the critical area, complete the update we began
            // within it. This must be done before anything reads the
            // render state (e.g. rebuild_cells).
            self.terminal_state.end_update();

            // Outside the critical area we can update our links to contain
            // our regex results.
            // catch |err| log.warn("error searching for regex links err={}", .{err});
            if let Err(err) = self.config.links.render_cell_map(
                Global,
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

                // NOTE: The order below matters. Highlights added earlier
                // will take priority.

                if let Some(m) = &self.search_selected_match {
                    // catch |err| {
                    //     // Not a critical error, we just won't show highlights.
                    //     log.warn("error updating search selected highlight err={}", .{err});
                    // }
                    if let Err(err) = self.terminal_state.update_highlights_flattened(
                        self.alloc,
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
                        self.alloc,
                        HighlightTag::SearchMatch as u8,
                        &m.matches,
                    ) {
                        // Not a critical error, we just won't show highlights.
                        log::warn!(target: LOG_SCOPE, "error updating search highlights err={err:?}");
                    }
                }
            }

            // From this point forward no more errors.
            // errdefer comptime unreachable;

            // Reset our dirty state after updating.
            // defer self.terminal_state.dirty = .false;
            //
            // PORT NOTE (§C.4/RISK-3): the reference `defer` fires at function scope
            // exit — i.e. AFTER `updateCustomShaderUniformsFromState` (which GATES on
            // `dirty != .false`). So the reset is the LAST statement of this body,
            // NOT here; the post-region is infallible so a plain final assignment is
            // faithful. CORRECTION (parity S115-F1): this used to say the reset runs
            // after `endFrame`; the `endFrame` defer is registered FIRST, so it runs
            // LAST, after this reset (in `update_frame`, after this body returns).

            // Rebuild the overlay image if we have one. We can do this
            // outside of any critical areas.
            // catch |err| log.warn("error rebuilding overlay surface err={}", .{err});
            if let Err(err) = self.rebuild_overlay(&critical.overlay_features) {
                log::warn!(target: LOG_SCOPE, "error rebuilding overlay surface err={err:?}");
            }

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
                //
                // LOG PORT NOTE (task-14c-1, row S115-F4): consequently the
                // `log.warn` above is NOT restorable — there is no `catch` arm to
                // hang it on, and emitting it unconditionally would put a
                // diagnostic on the SUCCESS path. S115-F4 is therefore WAIVED, not
                // CLOSED; every other anchor it cites is live.

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
                // catch |err| log.warn("error updating overlay images err={}", .{err});
                if let Err(err) = self
                    .images
                    .overlay_update(self.alloc, self.overlay.as_ref())
                {
                    log::warn!(target: LOG_SCOPE, "error updating overlay images err={err:?}");
                }

                // Update custom shader uniforms that depend on terminal state.
                self.update_custom_shader_uniforms_from_state();

                self.draw_mutex.unlock();
            }

            // Start the display link now that the rebuilt frame is ready.
            // (`6688aa072`: the link is parked while idle, so the rebuild that
            // set `cells_rebuilt` is what un-parks it; runs before the deferred
            // `font_shaper.endFrame()`, which `update_frame` runs after this body,
            // as in the reference.)
            self.sync_display_link(None, None);

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
            // `.color.r/g/b` on a `?TerminalColor`, a union whose other tags are
            // `cell-foreground` and `cell-background`. Those are documented values
            // of `cursor-text`, `selection-foreground` and `selection-background`,
            // and the `cursor-invert-fg-bg` / `selection-invert-fg-bg`
            // compatibility keys set them. With any `custom-shader`, the access
            // panics in ReleaseSafe and is undefined behaviour in ReleaseFast. A
            // cell-relative value names no single colour for a whole-surface
            // uniform (the reference's own `TerminalColor.toTerminalRGB` returns
            // null for it), and the cell-path resolution needs a cell. So the port
            // writes each uniform only for a `.color` value and otherwise leaves
            // it unchanged, which is what the reference does when the option is
            // unset. The platform twin is `generic/linux_impl.rs`.
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
                //
                // PORT NOTE: reference defect (R13), UD76 (task R13-C2): the
                // product and the padding sum saturate. A saturated
                // `padding.left`/`top` (UD71) overflows the reference's
                // unchecked `u32` sum for any cursor off the first column or
                // row until the terminal is resized to fit it.
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
    }

    // =======================================================================
    // Chunk 4 — the offscreen draw path (GPU_RD_CHUNK4_DESIGN)
    // =======================================================================

    /// `= Frame.zig` `bufferCompleted` (the `@fieldParentPtr` / container_of
    /// re-entry). Matches `crate::metal::frame::CompletionFn`.
    //
    // PORT NOTE ([FIX-2] / `rust-mut-args-strongly-protected` / vtc-effects-trampoline-
    // seam-d3): the renderer pointer is recovered as a RAW `*mut Renderer` (NOT
    // `&mut`) and deref'd HERE — inside `Frame::complete`, which runs inside
    // `draw_frame` while `draw_frame`'s `&mut self` (the raw root `self_ptr`) is still
    // live on the stack. A `&mut` reborrow held across this callback would be UB. This
    // exact structure (`&mut api.present` + `&self frame_completed` fired while the
    // outer `self_ptr` + `frame` raw roots are live) is PROVEN clean under Stacked
    // Borrows AND Tree Borrows on `.port/phase-2/mirihar-chunk3b` (`drawframe_shipped`).
    unsafe fn metal_buffer_completed(
        ctx: &CompletionContext,
        buffer: &ProtocolObject<dyn MTLCommandBuffer>,
    ) {
        // const buffer = objc.Object.fromId(buffer_id);
        // Get our command buffer status to pass back to the generic renderer.
        // const status = buffer.getProperty(mtl.MTLCommandBufferStatus, "status");
        let status = buffer.status();
        // const health: Health = switch (status) { .@"error" => .unhealthy, else => .healthy };
        let health = match status {
            MTLCommandBufferStatus::Error => crate::Health::Unhealthy,
            _ => crate::Health::Healthy,
        };
        // container_of: the type-erased renderer pointer back to *mut Renderer.
        let renderer = ctx.renderer as *mut Renderer;
        // If the frame is healthy, present it.
        if health == crate::Health::Healthy {
            // block.renderer.api.present(block.target.*, block.sync) catch |err|
            //     log.err("Failed to present render target: err={}", .{err});   (Frame.zig:82-88)
            // RESTORED (task-14c-2, S118-F4). This site's oracle is
            // `renderer/metal/Frame.zig:82-88`, whose scope is `.metal`, NOT
            // `generic.zig`'s `.generic_renderer` — so it targets
            // `crate::metal::frame::LOG_SCOPE` even though the code lives here. It lives
            // here at all because the port inlines `Frame.zig`'s `bufferCompleted` into
            // this completion trampoline. We consume the now-fallible Result faithfully
            // (the layer path can fail; W1-3). `&mut api` reborrow through the raw ptr;
            // `&*ctx.target` is the `*mut Target` back to `&Target`.
            if let Err(e) = unsafe { (*renderer).api.present(&*ctx.target, ctx.sync) } {
                // log.err("Failed to present render target: err={}", .{err});
                log::error!(
                    target: crate::metal::frame::LOG_SCOPE,
                    "Failed to present render target: err={e:?}"
                );
            }
        }
        // block.renderer.frameCompleted(health);
        unsafe { (*renderer).frame_completed(health) };
    }

    impl Renderer {
        // fn deinitShaders(self: *Self) void { self.shaders.deinit(self.alloc); }
        //
        // PORT NOTE (dead-code / `-D warnings`, GPU_RD_CHUNK4_DESIGN §6.4 following the
        // GPU_RD_CHUNK3A_DESIGN §B.4 "make pub, ZERO #[allow(dead_code)]" precedent):
        // `deinit_shaders`'s SOLE reference is `Renderer::deinit` (ref generic.zig
        // :825), which stays the `unimplemented!()` seam (§1). `draw_frame`'s reinit
        // branch calls `self.shaders.deinit()` DIRECTLY (ref :1500), NOT
        // `self.deinit_shaders()` — so it has no live caller in Chunk 4 and a private
        // `fn` would trip `-D warnings`. Widened to `pub fn` (a pub method on the
        // re-exported `Renderer` is reachable ⇒ dead-code-exempt) until
        // `Renderer::deinit` un-defers. Zig `alloc` param dissolves (`Shaders::deinit`
        // is objc2/Vec-managed).
        pub fn deinit_shaders(&mut self) {
            self.shaders.deinit();
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
        // PORT NOTE (`aee7bf347`): REPLACES `hasAnimations` (deleted upstream);
        // `pub` — the renderer thread's `armAnimationTimer` (core,
        // `renderer_seam::Thread`) reaches it cross-crate.
        //
        // pub fn animationWake(self: *const Self) ?AnimationWake {
        //     // Custom shaders animate by redrawing on a fixed cadence,
        //     // gated by configuration and focus.
        //     const shader_delay: ?u64 = shader: {
        //         if (!self.has_custom_shaders) break :shader null;
        //         break :shader switch (self.config.custom_shader_animation) {
        //             .false => null,
        //             .always => draw_interval_ms,
        //             .true => if (self.focused) draw_interval_ms else null,
        //         };
        //     };
        //
        //     // Kitty animations tick during updateFrame; between
        //     // updates the deadline is absolute on the animation
        //     // clock, so a stream of draw wakes recomputing this
        //     // cannot starve it into the future.
        //     const kitty_delay: ?u64 = kitty: {
        //         const next = self.kitty_animation_next_ms orelse break :kitty null;
        //         const base = self.kitty_animation_clock orelse break :kitty null;
        //         const now: std.Io.Timestamp = .now(global.io(), .awake);
        //         const now_ms: u64 = @intCast(@divTrunc(
        //             base.durationTo(now).nanoseconds,
        //             std.time.ns_per_ms,
        //         ));
        //         // Never wake faster than the draw interval; an
        //         // overdue frame is picked up on the next wake.
        //         break :kitty @max(next -| now_ms, draw_interval_ms);
        //     };
        //
        //     // An update wake includes a draw, so it wins ties.
        //     if (kitty_delay) |k| {
        //         if (shader_delay == null or k <= shader_delay.?) {
        //             return .{ .delay_ms = k, .kind = .update };
        //         }
        //     }
        //
        //     if (shader_delay) |s| return .{ .delay_ms = s, .kind = .draw };
        //
        //     return null;
        // }
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
                // (`base.durationTo(now).nanoseconds / ns_per_ms` — a monotonic
                // clock, so the duration is never negative.)
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

        fn init_shaders(&mut self) -> Result<(), Error> {
            // var arena = ArenaAllocator.init(self.alloc);
            // defer arena.deinit();
            // const arena_alloc = arena.allocator();
            //
            // Load our custom shaders
            // const custom_shaders: []const [:0]const u8 = shadertoy.loadFromFiles(
            //     arena_alloc,
            //     self.config.custom_shaders,
            //     GraphicsAPI.custom_shader_target,
            // ) catch |err| err: {
            //     log.warn("error loading custom shaders err={}", .{err});
            //     break :err &.{};
            // };
            //
            // The reference arena is subsumed by owned `Vec<CString>` (dropped after
            // `init_shaders` returns — `init_post_pipeline` copies the sources into an
            // `NSString`). The `catch |err| break :err &.{}` (any load error ⇒ no
            // custom shaders, never block the renderer) maps to `unwrap_or_else`.
            let custom_shaders: Vec<CString> = shadertoy::load_from_files(
                &self.config.custom_shaders,
                Metal::CUSTOM_SHADER_TARGET,
            )
            .unwrap_or_else(|err| {
                // log.warn("error loading custom shaders err={}", .{err});
                log::warn!(target: LOG_SCOPE, "error loading custom shaders err={err:?}");
                Vec::new()
            });

            // GraphicsAPI.initShaders takes `[]const [:0]const u8`; build the borrow slice.
            let custom_refs: Vec<&CStr> = custom_shaders.iter().map(|c| c.as_c_str()).collect();

            // const has_custom_shaders = custom_shaders.len > 0;
            let has_custom_shaders = !custom_refs.is_empty();

            // var shaders = try self.api.initShaders(self.alloc, custom_shaders);
            // errdefer shaders.deinit(self.alloc);  → the `?` early-return `Drop`.
            // self.shaders = shaders;
            self.shaders = self.api.init_shaders(&custom_refs)?;
            // self.has_custom_shaders = has_custom_shaders;
            self.has_custom_shaders = has_custom_shaders;
            Ok(())
        }

        /// Sync the atlas data to the given texture. This copies the bytes
        /// associated with the atlas to the given texture. If the atlas no
        /// longer fits into the texture, the texture will be resized.
        //
        // PORT NOTE (raw-ptr — MIRIHAR-PROVEN deviation from GPU_RD_CHUNK4_DESIGN
        // §6.3's literal signature): the reference is `syncAtlasTexture(self: *const
        // Self, atlas: *const font.Atlas, texture: *Texture)` — `self` and `texture`
        // are RAW pointers. The design §6.3's Rust signature `fn sync_atlas_texture(
        // &self, atlas: &Atlas, texture: &mut Texture)` is UNSOUND: `texture` is `&mut
        // frame.grayscale`, which aliases INTO `self.swap_chain`, so a `&self` receiver
        // FREEZES the texture bytes and the subsequent `*texture` write pops that frozen
        // tag — UB under BOTH Stacked Borrows AND Tree Borrows (PROVEN on
        // `.port/phase-2/mirihar-chunk3b`'s `atlas_self_isolated` candidate; the raw-self
        // form is proven clean by `drawframe_shipped`). Ported as raw self + raw texture
        // (the faithful Zig shape): it reborrows only `self.api` (narrow, disjoint from
        // `swap_chain`) and writes `*texture`. `atlas` stays a real `&FontAtlas` — it
        // lives in the `*mut SharedGrid` (a SEPARATE allocation, no aliasing with self).
        unsafe fn sync_atlas_texture(
            self_raw: *const Renderer,
            atlas: &FontAtlas,
            texture: *mut Texture,
        ) -> Result<(), Error> {
            // if (atlas.size > texture.width) {
            if atlas.size() as usize > unsafe { (*texture).width } {
                // Free our old texture / Reallocate. The reassignment drops the old
                // `Texture` (Retained release) — the reference `texture.*.deinit()`.
                // texture.* = try self.api.initAtlasTexture(atlas);
                unsafe {
                    *texture = (*self_raw).api.init_atlas_texture(atlas)?;
                }
            }

            // try texture.replaceRegion(0, 0, atlas.size, atlas.size, atlas.data);
            unsafe {
                (*texture).replace_region(
                    0,
                    0,
                    atlas.size() as usize,
                    atlas.size() as usize,
                    atlas.data(),
                );
            }
            Ok(())
        }

        // fn uploadBackgroundImage(self: *Self) !void
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
                if bg.is_pending() {
                    bg.upload(self.alloc, &self.api)
                        .map_err(|_| Error::MetalFailed)?;
                }
            }
            Ok(())
        }

        /// Callback from the graphics API when a frame is completed.
        //
        // PORT NOTE (`&self` narrowing, faithful): reference `pub fn frameCompleted(
        // self: *Self, health)`. Every op it performs is `&self`-safe — `health.store`
        // (AtomicHealth, &self), the `surface_mailbox.push` (`&self` — the sink is
        // behind an `Arc`), and `swap_chain.release_frame` (&self, posts the
        // semaphore). Narrowing `*Self →
        // &self` is faithful (no mutation of a non-atomic field) AND makes the
        // trampoline's reborrow a SHARED reborrow (SB/TB-cleaner; frame is unused after
        // `complete()`, so the whole-`&self` reborrow is clean — mirihar).
        pub fn frame_completed(&self, health: crate::Health) {
            // If our health value hasn't changed, then we do nothing. We don't
            // do a cmpxchg here because strict atomicity isn't important.
            // if (self.health.load(.seq_cst) != health) {
            if self.health.load() != health {
                // self.health.store(health, .seq_cst);
                self.health.store(health);

                // Our health value changed, so we notify the surface so that it
                // can do something about it.
                // _ = self.surface_mailbox.push(.{ .renderer_health = health }, .{ .forever = {} });
                //   — REAL (seam §B.2 resolved): core's sink re-wraps this into
                //     `apprt::surface::Message::RendererHealth` and enqueues it.
                //     Detached (offscreen) mailboxes still return 0.
                let _ = self.surface_mailbox.push(
                    SurfaceMessage::RendererHealth(health),
                    MailboxTimeout::Forever,
                );
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
        /// If `sync` is true, this will synchronously block until
        /// the frame is finished drawing and has been presented.
        //
        // PORT NOTE (raw-ptr discipline, GPU_RD_CHUNK4_DESIGN §5.2 / MIRIHAR-PROVEN):
        // the whole body drives `self` through the SINGLE raw root `self_ptr = self`
        // with NARROW field reborrows (`(*self_ptr).field`), and re-derives the frame
        // raw root `frame` FRESH via `addr_of_mut!((*self_ptr).swap_chain.frames[idx])`
        // AFTER every whole-`&mut self` call (`upload_background_image`,
        // `update_custom_shader_uniforms_for_frame`, `update_screen_size_uniforms`,
        // `init_shaders`) — because a whole `&mut *self_ptr` reborrow POPS a held
        // `frame` tag (H1: SB UB, PROVEN by the `drawframe_naive` mirihar candidate).
        // The trampoline (`Frame::complete`) re-enters through the type-erased
        // `self_ptr` (H2). The full structure is clean under SB AND TB
        // (`drawframe_shipped`). `swap_chain.frames` is a fixed ARRAY (place-projected
        // `[idx]`, no `Index` autoref).
        //
        // W5 (`W5_DESIGN §2.3`): the raw-self ENTRY. Faithful to Zig
        // `drawFrame(self: *Renderer)` (a raw-pointer receiver — no protector). BOTH
        // concurrent production callers invoke THIS through a raw `*mut Renderer`:
        // (a) the render thread's `renderer.Thread` owner (above rustty-renderer; it
        // holds `renderer: *mut Renderer`, faithful to Zig's raw `*Renderer` field),
        // and (b) the main-thread display callback (metal_impl `display_callback`). No
        // whole-`Renderer` `&mut` protector ever spans the call, so the two concurrent
        // calls never alias a protected reference; the `draw_mutex` (locked below)
        // serialises the field accesses. Proven on mirihar-chunk3b
        // (`displaycb_concurrent`); the `displaycb_naive` UB control shows the split is
        // load-bearing (a protected `&mut self` receiver racing this raw draw is UB).
        //
        /// # Safety
        /// `self_ptr` must be a valid, well-aligned, whole-`Renderer`-provenance
        /// pointer, and outlive the call. Concurrent calls are permitted (serialised
        /// by `draw_mutex`).
        pub unsafe fn draw_frame_raw(self_ptr: *mut Renderer, sync: bool) -> Result<(), Error> {
            // const start = std.time.Instant.now() catch unreachable;
            // const start_micro = std.time.microTimestamp();
            // defer {
            //     const end = std.time.Instant.now() catch unreachable;
            //     log.warn(
            //         "[drawFrame time] start_micro={} duration={}ns",
            //         .{ start_micro, end.since(start) / std.time.ns_per_us },
            //     );
            // }

            // We hold a the draw mutex to prevent changes to any
            // data we access while we're in the middle of drawing.
            // self.draw_mutex.lock();
            // defer self.draw_mutex.unlock();
            // (`self_ptr` is now the raw-self receiver; W5_DESIGN §2.3.)
            unsafe { (*self_ptr).draw_mutex.lock() };

            // Hand-rolled `defer`s as RAII guards, each holding the raw root `self_ptr`
            // and performing its cleanup via a NARROW field reborrow on `drop` (so no
            // guard holds a live `&`/`&mut` across the body). Declared so they drop
            // LIFO in the reference order: `draw_frame_end` → scrollbar → unlock (Rust
            // drops locals in reverse declaration order). `frame_ctx.complete(sync)` is
            // the reference's LAST-registered `defer` (fires FIRST); it is called
            // explicitly at the end of the body, before these guards drop.
            struct UnlockGuard(*mut Renderer);
            impl Drop for UnlockGuard {
                fn drop(&mut self) {
                    // self.draw_mutex.unlock();
                    unsafe { (*self.0).draw_mutex.unlock() };
                }
            }
            struct ScrollbarGuard(*mut Renderer);
            impl Drop for ScrollbarGuard {
                fn drop(&mut self) {
                    // After the graphics API is complete (so we defer) we want to
                    // update our scrollbar state.
                    // defer if (self.scrollbar_dirty) {
                    //     // Fail instantly if the surface mailbox if full, we'll just
                    //     // get it on the next frame.
                    //     if (self.surface_mailbox.push(.{ .scrollbar = self.scrollbar },
                    //         .instant) > 0) self.scrollbar_dirty = false;
                    // };
                    // (REAL, seam §B.2 resolved: core's sink enqueues
                    //  `apprt::surface::Message::Scrollbar`. A DETACHED mailbox —
                    //  the offscreen golden/unit renderers, which have no surface —
                    //  still returns 0, so the golden observes no mailbox.)
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
            // errdefer-only guard: releases the swap-chain semaphore ONLY on an early
            // error return after `next_frame`. On the success path it is disarmed (the
            // frame's `frameCompleted` releases instead — else a double-release).
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

            let _unlock = UnlockGuard(self_ptr);
            let _scrollbar = ScrollbarGuard(self_ptr);

            // Let our graphics API do any bookkeeping, etc.
            // that it needs to do before / after `drawFrame`.
            // self.api.drawFrameStart();
            // defer self.api.drawFrameEnd();
            unsafe { (*self_ptr).api.draw_frame_start() };
            let _draw_end = DrawEndGuard(self_ptr);

            // Retrieve the most up-to-date surface size from the Graphics API
            // const surface_size = try self.api.surfaceSize();
            let surface_size = unsafe { (*self_ptr).api.surface_size() }?;

            // If either of our surface dimensions is zero
            // then drawing is absurd, so we just return.
            if surface_size.width == 0 || surface_size.height == 0 {
                return Ok(());
            }

            // If we have no graphics context we can't draw. This is
            // only the case while unrealized (GTK); displayRealized
            // rebuilds the swap chain.
            // if (!self.display_realized) return;
            if !unsafe { (*self_ptr).display_realized } {
                return Ok(());
            }

            // Get our swap chain, rebuilding it if it was released
            // while we were hidden. Rebuilding is deferred to draw
            // time because resource creation must happen somewhere
            // our graphics API allows it (OpenGL requires a current
            // context, which drawFrame guarantees).
            //
            // const swap_chain: *SwapChain, const swap_chain_rebuilt: bool =
            //     if (self.swap_chain) |*sc| .{ sc, false } else rebuild: {
            //         self.swap_chain = try SwapChain.init(
            //             self.api,
            //             self.has_custom_shaders,
            //         );
            //         break :rebuild .{ &self.swap_chain.?, true };
            //     };
            //
            // (raw-ptr discipline: instead of the reference's held `*SwapChain`
            // local, the chain is re-derived FRESH from `self_ptr` at every use
            // below — the same rule as `frame`; §5.2.)
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

            // Conditions under which we need to draw the frame, otherwise we
            // don't need to since the previous frame should be identical.
            //
            // While any animation is in progress (a pending animation wake)
            // every draw must actually render.
            let needs_redraw = size_changed
                || swap_chain_rebuilt
                || unsafe { (*self_ptr).cells_rebuilt }
                || unsafe { (*self_ptr).animation_wake().is_some() }
                || sync;

            if !needs_redraw {
                // We still need to present the last target again, because the
                // apprt may be swapping buffers and display an outdated frame
                // if we don't draw something new.
                // try self.api.presentLastTarget();
                unsafe { (*self_ptr).api.present_last_target() }?;

                // Resync the display link because we can probably pause
                // the display link at this point.
                // self.syncDisplayLink(null, null);
                unsafe { (*self_ptr).sync_display_link(None, None) };
                return Ok(());
            }
            // self.cells_rebuilt = false;
            unsafe { (*self_ptr).cells_rebuilt = false };

            // Wait for a frame to be available.
            // const frame = swap_chain.nextFrame();
            // errdefer swap_chain.releaseFrame();
            //
            // `next_frame` is called for its side effects (semaphore wait + index
            // bump); the returned `&mut FrameState` is dropped
            // immediately and `frame` is re-derived FRESH from `self_ptr` at each use
            // (§5.2 — a held frame raw root is popped by any whole-`&mut self`
            // reborrow). `frame_index` now holds the chosen index. The chain is
            // Some here (established/rebuilt above; teardown is excluded by the
            // held draw_mutex).
            let _ = unsafe { (*self_ptr).swap_chain.as_mut().unwrap().next_frame() };
            let mut release_guard = ReleaseGuard {
                r: self_ptr,
                armed: true,
            };
            let idx = unsafe { (*self_ptr).swap_chain.as_ref().unwrap().frame_index as usize };
            // log.debug("drawing frame index={}", .{swap_chain.frame_index});

            // If we need to reinitialize our shaders, do so.
            // if (self.reinitialize_shaders) {
            //     self.reinitialize_shaders = false;
            //     self.shaders.deinit(self.alloc);
            //     try self.initShaders();
            // }
            if unsafe { (*self_ptr).reinitialize_shaders } {
                unsafe {
                    (*self_ptr).reinitialize_shaders = false;
                    // `drawFrame` deinits `self.shaders` DIRECTLY (ref :1500), NOT via
                    // `self.deinit_shaders()` — keeping `deinit_shaders` referenced only
                    // by the deferred `Renderer::deinit` (§6.4).
                    (*self_ptr).shaders.deinit();
                    (*self_ptr).init_shaders()?; // whole-&mut self
                }
            }

            // Our shaders should not be defunct at this point.
            // assert(!self.shaders.defunct);
            assert!(!unsafe { (*self_ptr).shaders.defunct });

            // If we have custom shaders, make sure we have the custom shader state in
            // our frame state, otherwise if we have a state but don't need it we remove
            // it. (Golden: has_custom_shaders=false + custom_shader_state=None ⇒ no-op.)
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
                else if unsafe { (*frame).custom_shader_state.is_some() } {
                    // `state.deinit()` is the `CustomShaderState` field `Drop`; nulling
                    // the Option drops it.
                    unsafe { (*frame).custom_shader_state = None };
                }
            }

            // If our stored size doesn't match the surface size we need to update it.
            // if (size_changed) {
            //     self.size.screen = .{ .width = w, .height = h };
            //     self.updateScreenSizeUniforms();
            // }
            // (Golden: size_changed=false ⇒ skipped. `update_screen_size_uniforms` is a
            //  whole-&mut self call, so no `frame` raw root is held across this.)
            if size_changed {
                unsafe {
                    (*self_ptr).size.screen = ScreenSize {
                        width: surface_size.width,
                        height: surface_size.height,
                    };
                    (*self_ptr).update_screen_size_uniforms();
                }
            }

            // If this frame's target isn't the correct size, or the target config has
            // changed (such as when the blending mode changes), remove it and replace
            // it with a new one with the right values.
            // if (frame.target.width != self.size.screen.width or
            //     frame.target.height != self.size.screen.height or
            //     frame.target_config_modified != self.target_config_modified) {
            //     try frame.resize(self.api, self.size.screen.width, self.size.screen.height);
            //     frame.target_config_modified = self.target_config_modified;
            // }
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
            // _ = self.images.upload(self.alloc, &self.api);
            //   (narrow: &mut self.images + &self.api — disjoint fields.)
            let _ = unsafe {
                (*self_ptr)
                    .images
                    .upload((*self_ptr).alloc, &(*self_ptr).api)
            };

            // Upload the background image to the GPU as necessary.
            // try self.uploadBackgroundImage();      (whole-&mut self)
            unsafe { (*self_ptr).upload_background_image()? };

            // Update per-frame custom shader uniforms.
            // try self.updateCustomShaderUniformsForFrame();   (whole-&mut self)
            unsafe { (*self_ptr).update_custom_shader_uniforms_for_frame() };

            // Re-derive the frame raw root FRESH: the two whole-`&mut self` calls above
            // (`upload_background_image`, `update_custom_shader_uniforms_for_frame`)
            // popped any earlier frame tag. From here to `complete()` there are NO
            // whole-`&mut self` reborrows (only narrow field reborrows + raw
            // `self_ptr`/`frame`), so this root survives to the end.
            let frame: *mut FrameState = unsafe {
                // Narrow projection through the `Option` (a fresh,
                // immediately-discarded reborrow of ONLY the `swap_chain`
                // field; the chain is Some — established above, teardown
                // excluded by the held draw_mutex).
                addr_of_mut!((*self_ptr).swap_chain.as_mut().unwrap().frames[idx])
            };

            // Setup our frame data. The serialization boundary (§2): the ergonomic
            // `crate::shaders` structs are converted to their Metal-ABI POD images.
            // try frame.uniforms.sync(&.{self.uniforms});
            unsafe {
                (*frame)
                    .uniforms
                    .sync(&[UniformsGpu::from(&(*self_ptr).uniforms)])?;
            }
            // try frame.cells_bg.sync(self.cells.bg_cells);  (CellBg is already ABI)
            unsafe {
                (*frame).cells_bg.sync(&(*self_ptr).cells.bg_cells)?;
            }
            // const fg_count = try frame.cells.syncFromArrayLists(self.cells.fg_rows);
            //
            // PORT NOTE (§6.2, accepted deviation — OPEN RISK #4): the reference's
            // `syncFromArrayLists` is a zero-copy memcpy of the `extern struct`s. Rust's
            // ergonomic `CellText` in-memory layout is NOT the packed-`u8` ABI (byte 29),
            // so we FUSE the row concatenation (order-preserving) with the `CellText ->
            // CellTextGpu` ABI serialization into one transient `Vec` (NOT a GPU-buffer
            // realloc — `sync` still reuses `frame.cells`). The one transient `Vec` per
            // frame deviates from the reference's "keep this around so we don't
            // reallocate" intent; forced by the layout mismatch, fine for the offscreen
            // golden. Documented; the repr(C)-pack-in-place alternative (§2.0) would
            // remove it at the cost of 3b/3c churn.
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

            // If our background image buffer has changed, sync it. (Golden: modified
            // stays 0 ⇒ skipped.)
            // if (frame.bg_image_buffer_modified != self.bg_image_buffer_modified) {
            //     try frame.bg_image_buffer.sync(&.{self.bg_image_buffer});
            //     frame.bg_image_buffer_modified = self.bg_image_buffer_modified;
            // }
            if unsafe { (*frame).bg_image_buffer_modified != (*self_ptr).bg_image_buffer_modified }
            {
                unsafe {
                    (*frame)
                        .bg_image_buffer
                        .sync(&[BgImageGpu::from(&(*self_ptr).bg_image_buffer)])?;
                    (*frame).bg_image_buffer_modified = (*self_ptr).bg_image_buffer_modified;
                }
            }

            // If our font atlas changed, sync the texture data. Load-bearing: without
            // it the `cell_text` shader samples an empty texture → no glyphs. The
            // `*mut SharedGrid` is a SEPARATE allocation (`&*self.font_grid`); the
            // shared read lock is the reference `lock.lockShared()`. `sync_atlas_texture`
            // takes raw self + raw texture (see its PORT NOTE — the texture aliases into
            // `self.swap_chain`).
            // texture: {
            //     const modified = self.font_grid.atlas_grayscale.modified.load(.monotonic);
            //     if (modified <= frame.grayscale_modified) break :texture;
            //     self.font_grid.lock.lockShared();
            //     defer self.font_grid.lock.unlockShared();
            //     frame.grayscale_modified = self.font_grid.atlas_grayscale.modified.load(.monotonic);
            //     try self.syncAtlasTexture(&self.font_grid.atlas_grayscale, &frame.grayscale);
            // }
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
            // texture: {
            //     const modified = self.font_grid.atlas_color.modified.load(.monotonic);
            //     if (modified <= frame.color_modified) break :texture;
            //     self.font_grid.lock.lockShared();
            //     defer self.font_grid.lock.unlockShared();
            //     frame.color_modified = self.font_grid.atlas_color.modified.load(.monotonic);
            //     try self.syncAtlasTexture(&self.font_grid.atlas_color, &frame.color);
            // }
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
            //   — `self` is the type-erased `*mut ()` renderer root; the target is the
            //     raw `addr_of_mut!((*frame).target)`; the completion fn is the
            //     trampoline. `begin_frame` is `&self.api` (narrow).
            let mut frame_ctx: Frame = unsafe {
                (*self_ptr).api.begin_frame(
                    self_ptr as *mut (),
                    addr_of_mut!((*frame).target),
                    metal_buffer_completed,
                )
            }?;

            {
                // var pass = frame_ctx.renderPass(&.{.{
                //     .target = if (frame.custom_shader_state) |state|
                //         .{ .texture = state.back_texture } else .{ .target = frame.target },
                //     .clear_color = .{ 0.0, 0.0, 0.0, 0.0 },
                // }});
                // defer pass.complete();
                let attach_target = match unsafe { (*frame).custom_shader_state.as_ref() } {
                    Some(state) => AttachmentTarget::Texture(&state.back_texture),
                    None => AttachmentTarget::Target(unsafe { &(*frame).target }),
                };
                let pass = frame_ctx.render_pass(&[Attachment {
                    target: attach_target,
                    clear_color: Some([0.0, 0.0, 0.0, 0.0]),
                }]);

                // First we draw our background image, if we have one. The bg image
                // shader also draws the main bg color. Otherwise, if we don't have a
                // background image, we draw the background color by itself in its own
                // step.
                //
                // NOTE: We don't use the clear_color for this because that would require
                //       us to do color space conversion on the CPU-side. In the future
                //       when we have utilities for that we should remove this step and
                //       use clear_color.
                // if (self.bg_image) |img| switch (img) {
                //     .ready => |texture| pass.step(.{ .pipeline = bg_image, ... }),
                //     else => {},
                // } else { pass.step(.{ .pipeline = bg_color, ... }); }
                match unsafe { (*self_ptr).bg_image.as_ref() } {
                    Some(image::Image::Ready(texture)) => pass.step(Step {
                        pipeline: unsafe { &(*self_ptr).shaders.pipelines.bg_image },
                        uniforms: Some(unsafe { (*frame).uniforms.raw() }),
                        buffers: &[Some(unsafe { (*frame).bg_image_buffer.raw() })],
                        textures: &[Some(texture)],
                        samplers: &[],
                        draw: Draw {
                            kind: MTLPrimitiveType::Triangle,
                            vertex_count: 3,
                            instance_count: 1,
                        },
                    }),
                    Some(_) => {}
                    None => {
                        pass.step(Step {
                            pipeline: unsafe { &(*self_ptr).shaders.pipelines.bg_color },
                            uniforms: Some(unsafe { (*frame).uniforms.raw() }),
                            buffers: &[None, Some(unsafe { (*frame).cells_bg.raw() })],
                            textures: &[],
                            samplers: &[],
                            draw: Draw {
                                kind: MTLPrimitiveType::Triangle,
                                vertex_count: 3,
                                instance_count: 1,
                            },
                        });
                    }
                }

                // Then we draw any kitty images that need to be behind text AND cell
                // backgrounds.
                // self.images.draw(&self.api, self.shaders.pipelines.image, &pass, .kitty_below_bg);
                unsafe {
                    (*self_ptr).images.draw(
                        &(*self_ptr).api,
                        &(*self_ptr).shaders.pipelines.image,
                        &pass,
                        DrawPlacements::KittyBelowBg,
                    );
                }

                // Then we draw any opaque cell backgrounds.
                // pass.step(.{ .pipeline = cell_bg, .uniforms, .buffers = &.{ null, frame.cells_bg.buffer }, .draw = {triangle, 3} });
                pass.step(Step {
                    pipeline: unsafe { &(*self_ptr).shaders.pipelines.cell_bg },
                    uniforms: Some(unsafe { (*frame).uniforms.raw() }),
                    buffers: &[None, Some(unsafe { (*frame).cells_bg.raw() })],
                    textures: &[],
                    samplers: &[],
                    draw: Draw {
                        kind: MTLPrimitiveType::Triangle,
                        vertex_count: 3,
                        instance_count: 1,
                    },
                });

                // Kitty images between cell backgrounds and text.
                // self.images.draw(..., .kitty_below_text);
                unsafe {
                    (*self_ptr).images.draw(
                        &(*self_ptr).api,
                        &(*self_ptr).shaders.pipelines.image,
                        &pass,
                        DrawPlacements::KittyBelowText,
                    );
                }

                // Text.
                // pass.step(.{ .pipeline = cell_text, .uniforms,
                //     .buffers = &.{ frame.cells.buffer, frame.cells_bg.buffer },
                //     .textures = &.{ frame.grayscale, frame.color },
                //     .draw = .{ .type = .triangle_strip, .vertex_count = 4, .instance_count = fg_count } });
                pass.step(Step {
                    pipeline: unsafe { &(*self_ptr).shaders.pipelines.cell_text },
                    uniforms: Some(unsafe { (*frame).uniforms.raw() }),
                    buffers: &[
                        Some(unsafe { (*frame).cells.raw() }),
                        Some(unsafe { (*frame).cells_bg.raw() }),
                    ],
                    textures: &[
                        Some(unsafe { &(*frame).grayscale }),
                        Some(unsafe { &(*frame).color }),
                    ],
                    samplers: &[],
                    draw: Draw {
                        kind: MTLPrimitiveType::TriangleStrip,
                        vertex_count: 4,
                        instance_count: fg_count,
                    },
                });

                // Kitty images in front of text.
                // self.images.draw(..., .kitty_above_text);
                unsafe {
                    (*self_ptr).images.draw(
                        &(*self_ptr).api,
                        &(*self_ptr).shaders.pipelines.image,
                        &pass,
                        DrawPlacements::KittyAboveText,
                    );
                }

                // Debug overlay. We do this before any custom shader state
                // because our debug overlay is aligned with the grid.
                // if (self.overlay != null) self.images.draw(..., .overlay);
                if unsafe { (*self_ptr).overlay.is_some() } {
                    unsafe {
                        (*self_ptr).images.draw(
                            &(*self_ptr).api,
                            &(*self_ptr).shaders.pipelines.image,
                            &pass,
                            DrawPlacements::Overlay,
                        );
                    }
                }

                // defer pass.complete();
                pass.complete();
            }

            // If we have custom shaders, then we render them. (Golden:
            // custom_shader_state=None ⇒ skipped.)
            // if (frame.custom_shader_state) |*state| {
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
                let post_pipelines: &[crate::metal::pipeline::Pipeline] =
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
                    let pass = frame_ctx.render_pass(&[Attachment {
                        target,
                        clear_color: Some([0.0, 0.0, 0.0, 0.0]),
                    }]);

                    // pass.step(.{ .pipeline = pipeline, .uniforms = state.uniforms.buffer,
                    //     .textures = &.{state.back_texture}, .samplers = &.{state.sampler},
                    //     .draw = .{ .type = .triangle, .vertex_count = 3 } });
                    pass.step(Step {
                        pipeline,
                        uniforms: Some(unsafe { (*state).uniforms.raw() }),
                        buffers: &[],
                        textures: &[Some(unsafe { &(*state).back_texture })],
                        samplers: &[Some(unsafe { &(*state).sampler })],
                        draw: Draw {
                            kind: MTLPrimitiveType::Triangle,
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
            // trampoline calls `frame_completed` → `release_frame` (releasing the
            // semaphore). Leaving the guard armed would double-release.
            release_guard.armed = false;

            // defer frame_ctx.complete(sync);  — the reference's LAST-registered defer;
            // on the sync path it commits → waitUntilCompleted → invokes
            // `metal_buffer_completed` directly (H2). `frame` is NOT used after this.
            frame_ctx.complete(sync);

            Ok(())
            // The three RAII guards drop here in the reference LIFO order:
            // draw_frame_end → scrollbar → unlock.
        }

        /// `&mut self` convenience wrapper over [`draw_frame_raw`]. Sound **only** for
        /// EXCLUSIVE single-threaded callers — i.e. the offscreen unit tests, where no
        /// display link / render thread / CA `display` can fire concurrently. Every
        /// concurrent production path uses `draw_frame_raw`; no `&mut self` draw entry
        /// is reachable from the render thread or the main-thread present path
        /// (`W5_DESIGN §2.5`). Adding a concurrent caller of THIS wrapper reintroduces
        /// the protected-`&mut` UB the split exists to prevent — the `displaycb_naive`
        /// mirihar control fails for exactly that shape.
        pub fn draw_frame(&mut self, sync: bool) -> Result<(), Error> {
            let self_ptr: *mut Renderer = self;
            unsafe { Self::draw_frame_raw(self_ptr, sync) }
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
                custom_shaders: RepeatablePath::default(),
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
                custom_shader_animation: crate::thread::CustomShaderAnimation::True,
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
                Err(e) => {
                    // F5 strict-mode pivot (metal::test_support): a no-op by
                    // default, a panic under `RUSTTY_REQUIRE_METAL=1`.
                    crate::metal::test_support::require_metal_or_skip(
                        "test_renderer",
                        &format!("{e:?}"),
                    );
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
                surface_mailbox: SurfaceMailbox::DETACHED,
                grid_metrics,
                size,
                focused: true,
                visible: true,
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
                display_realized: true,
                swap_chain: Some(swap_chain),
                kitty_animation_clock: None,
                kitty_animation_next_ms: None,
                target_config_modified: 0,
                reinitialize_shaders: false,
                has_custom_shaders: false,
                shaders,
                terminal_state: RenderState::empty(),
                terminal_state_frame_count: 0,
                overlay: None,
            })
        }

        // Build a leaked embedded-font `SharedGrid` + the pixel `Size` for a
        // `cols`×`rows` grid. Factored (W1-7 de-dup) so `golden_renderer` and the
        // windowed-lifecycle KAT share the identical grid recipe. Metal-independent
        // (the caller does its own Metal probe). The grid is leaked so the returned
        // `*mut SharedGrid` outlives the test scope. The `size.screen` is PINNED to the
        // grid's pixel size so that (in `golden_renderer`) `init_offscreen`'s
        // `offscreen_size == size.screen` ⇒ `draw_frame`'s `size_changed == false`
        // (§3.2: projection 1:1; cell (x,y) at pixel [x*cw..(x+1)*cw) × [y*ch..(y+1)*ch)).
        fn test_font_grid_and_size(
            cols: CellCountInt,
            rows: CellCountInt,
        ) -> (*mut SharedGrid, Size) {
            // Real font grid over the embedded JetBrains Mono Regular. Leaked so the
            // `*mut SharedGrid` stays valid for the test's scope.
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

        // GPU_RD_CHUNK4_DESIGN §3.2/§6.5: the GOLDEN harness = `test_renderer` with the
        // offscreen size PINNED to the grid's pixel size (`size.screen`), so
        // `draw_frame`'s `surface_size == self.size.screen` ⇒ `size_changed == false`
        // (no in-`draw_frame` resize / `update_screen_size_uniforms` surprise;
        // projection 1:1; cell (x,y) lands at pixel [x*cw..(x+1)*cw) × [y*ch..(y+1)*ch)
        // — trivial pixel math). So we build the font grid + metrics + `size` FIRST,
        // THEN call `init_offscreen(blending, size.screen.width, size.screen.height)`.
        fn golden_renderer(cols: CellCountInt, rows: CellCountInt) -> Option<Renderer> {
            // Probe: skip cleanly on hosts with no Metal device (headless CI), BEFORE
            // the (Metal-independent) font-grid build. `init_offscreen` returns
            // `Err(NoMetalDevice)` on a headless host (the `metal.rs` / `test_renderer`
            // precedent).
            if let Err(e) = Metal::init_offscreen(AlphaBlending::Native, 1, 1) {
                // F5 strict-mode pivot (metal::test_support).
                crate::metal::test_support::require_metal_or_skip(
                    "golden_renderer",
                    &format!("{e:?}"),
                );
                eprintln!("no Metal device; skipping golden");
                return None;
            }

            let (font_grid, size) = test_font_grid_and_size(cols, rows);

            // DE-DUP (W1-7): delegate to the now-filled `Renderer::init` instead of
            // hand-building the field literal. `golden_renderer` PINS
            // `offscreen_size == size.screen` (§3.2) — EXACTLY what `Renderer::init`
            // builds (`init_offscreen(config.blending, size.screen.width,
            // size.screen.height)`), and `derived_config_for_test()` uses
            // `blending = Native` (⇒ `BGRA8Unorm`, no store-time sRGB re-encode),
            // `colorspace = Srgb`, and EMPTY `custom_shaders`, so the delegated
            // `api`/`shaders`/`swap_chain`/`uniforms` are field-for-field identical to
            // the former hand-built literal (bg_color/bools/min_contrast are overwritten
            // by `update_frame` before `draw_frame`; all are no-op-equal for this
            // config anyway). `init` also runs `update_font_grid_uniforms` +
            // `update_screen_size_uniforms` (fixing the projection/cell uniforms — else
            // every vertex collapses to the origin) + `update_bg_image_buffer` +
            // `prep_background_image` (a no-op with `bg_image = None`). `cells` starts
            // empty and is sized by the first `update_frame`/`rebuild_cells`. Metal was
            // probed above, so `init` succeeds — the failure path is an `.expect`.
            let opts = Options {
                config: derived_config_for_test(),
                font_grid,
                size,
                // Offscreen: no surface, hence no mailbox (push returns 0).
                surface_mailbox: SurfaceMailbox::DETACHED,
            };
            Some(Renderer::init(Global, opts).expect("Renderer::init (Metal probed above)"))
        }

        // ===================================================================
        // task-14c-1 (`LOG` class, rows S115-F4 / S116-F2): the restored
        // `std.log.scoped(.generic_renderer)` diagnostics actually EMIT.
        //
        // Three tests, one per independently-breakable shape (the task brief's
        // own list), all on `prepBackgroundImage`'s three `break :load_background`
        // arms — the only `generic.zig` diagnostics reachable from a unit test
        // without a live terminal frame:
        //
        //   1. a plain single-argument `log.warn` (`Cannot determine file type
        //      ...`),
        //   2. a site whose preserved Zig comment was ELIDED with `...`
        //      (`Unsupported file type {} ...`) — the arguments exist only in the
        //      oracle, so this test fails for anything restored from the comment,
        //   3. a site that was standing in with `eprintln!` (`error opening
        //      background image file ...`), which an embedder's `log::Log` backend
        //      could never see.
        //
        // Each asserts the FULL captured record vector — level, target and exact
        // message text — so deleting the emission fails the test. Metal-skip on a
        // headless host, matching every other test in this module.
        mod restored_log_sites {
            use super::*;

            /// Builds a golden renderer whose bg-image config points at `bytes`
            /// written to a temp file with `ext`, runs `prep_background_image`
            /// with the capture logger installed, and returns
            /// `(records, path_display)`. `None` ⇒ no Metal device, skip.
            fn prep_with_bg_file(
                ext: &str,
                bytes: &[u8],
            ) -> Option<(Vec<crate::log_capture::CapturedRecord>, String)> {
                let mut r = golden_renderer(4, 2)?;

                let path = std::env::temp_dir().join(format!(
                    "rustty-renderer-log-{}-{:?}.{ext}",
                    std::process::id(),
                    std::thread::current().id()
                ));
                std::fs::write(&path, bytes).expect("write fixture");

                let path_bytes = path.as_os_str().as_encoded_bytes().to_vec();
                let path_display = String::from_utf8(path_bytes.clone()).unwrap();
                r.config.bg_image = Some(crate::generic::Path::Required(
                    CString::new(path_bytes).unwrap(),
                ));

                let cap = crate::log_capture::start();
                r.prep_background_image().expect("prep_background_image");
                let records = cap.messages();
                drop(cap);

                let _ = std::fs::remove_file(&path);
                Some((records, path_display))
            }

            // Shape 1 — a plain single-argument `log.warn` on a reachable path.
            // oracle `generic.zig:1995`:
            //   log.warn(
            //       "Cannot determine file type for background image file \"{s}\"!",
            //       .{path},
            //   );
            #[test]
            fn unknown_file_type_warns() {
                // Junk bytes + an extension `guessFromExtension` does not know ⇒
                // `FileType.detect` and the extension guess both say `.unknown`.
                let Some((records, path_display)) =
                    prep_with_bg_file("xyz", b"not an image at all")
                else {
                    // `golden_renderer` already ran the F5 strict-mode pivot; on a
                    // host without a device we say so rather than skipping silently.
                    eprintln!("no Metal device; skipping unknown_file_type_warns");
                    return;
                };

                assert_eq!(
                    records,
                    vec![(
                        log::Level::Warn,
                        "generic_renderer".to_string(),
                        format!(
                            "Cannot determine file type for background image file \"{path_display}\"!"
                        ),
                    )],
                );
            }

            // Shape 2 — a site restored from the ORACLE, not from the elided
            // comment. `generic.rs`'s preserved comment reads
            //   `else => |f| { log.warn("Unsupported file type {} ...", .{ f, path }); break; },`
            // whose message text is truncated at `...`. The real oracle line
            // (`generic.zig:2002`) is
            //   log.warn(
            //       "Unsupported file type {} for background image file \"{s}\"!",
            //       .{ f, path },
            //   );
            // A restoration made from the comment emits `Unsupported file type {} ...`
            // and fails here.
            #[test]
            fn unsupported_file_type_warns_with_the_full_oracle_message() {
                // `GIF89a` magic ⇒ `FileType::detect` == `Gif`: a KNOWN type that
                // `prepBackgroundImage` has no decoder for (only png/jpeg), i.e.
                // the reference's `else => |f|` prong.
                let Some((records, path_display)) =
                    prep_with_bg_file("gif", b"GIF89a\x01\x00\x01\x00\x00\x00\x00;")
                else {
                    // `golden_renderer` already ran the F5 strict-mode pivot.
                    eprintln!(
                        "no Metal device; skipping \
                         unsupported_file_type_warns_with_the_full_oracle_message"
                    );
                    return;
                };

                assert_eq!(
                    records,
                    vec![(
                        log::Level::Warn,
                        "generic_renderer".to_string(),
                        format!(
                            "Unsupported file type Gif for background image file \"{path_display}\"!"
                        ),
                    )],
                );
            }

            // Shape 3 — a site that was an `eprintln!` stand-in. Before this stage
            // the open failure printed
            //   `warning(renderer): error opening/reading background image file {:?}`
            // to stderr: not a record any `log::Log` backend could filter or route,
            // and not the oracle's text either (the port had merged the reference's
            // distinct open/read messages into one invented string).
            // oracle `generic.zig:1960`:
            //   log.warn(
            //       "error opening background image file \"{s}\": {}",
            //       .{ path, err },
            //   );
            #[test]
            fn missing_background_image_file_warns_through_the_facade() {
                let Some(mut r) = golden_renderer(4, 2) else {
                    // `golden_renderer` already ran the F5 strict-mode pivot.
                    eprintln!(
                        "no Metal device; skipping \
                         missing_background_image_file_warns_through_the_facade"
                    );
                    return;
                };

                let path = std::env::temp_dir().join(format!(
                    "rustty-renderer-log-missing-{}-{:?}.png",
                    std::process::id(),
                    std::thread::current().id()
                ));
                let _ = std::fs::remove_file(&path);

                let path_bytes = path.as_os_str().as_encoded_bytes().to_vec();
                let path_display = String::from_utf8(path_bytes.clone()).unwrap();
                r.config.bg_image = Some(crate::generic::Path::Required(
                    CString::new(path_bytes).unwrap(),
                ));

                // The exact `{err:?}` rendering the site will produce, obtained the
                // same way the site obtains it — so the assertion stays EXACT
                // without hard-coding a platform errno string.
                let expected_err = format!(
                    "{:?}",
                    std::fs::File::open(&path).expect_err("fixture path must not exist")
                );

                let cap = crate::log_capture::start();
                r.prep_background_image().expect("prep_background_image");
                let records = cap.messages();
                drop(cap);

                assert_eq!(
                    records,
                    vec![(
                        log::Level::Warn,
                        "generic_renderer".to_string(),
                        format!(
                            "error opening background image file \"{path_display}\": {expected_err}"
                        ),
                    )],
                );
            }
        }

        // W1-7 KAT: the `Renderer` windowed lifecycle round-trip. Exercises the
        // apprt-free surface handshake (`surface_init`/`finalize_surface_init` are
        // FAITHFUL no-ops for the Metal monomorphization — `@hasDecl(GraphicsAPI,
        // "surfaceInit"/"finalizeSurfaceInit")` is FALSE, Metal declares only
        // `loopEnter`), then the now-filled `Renderer::init` (offscreen-portable device
        // ctor, layer `None`; DEFERRALS.md #3) → `deinit` (verbatim teardown). `deinit`
        // must not panic or leak (ASan covers UB across the objc/Metal FFI). Probe-skips
        // on a host with no Metal device, matching the W0 KAT. FFI ⇒ Miri-excluded (R6).
        //
        // Placed in a `windowed_lifecycle` submodule so `.port/phase-2/MIRI_EXCEPTIONS.md`
        // can name `generic::windowed_lifecycle::windowed_renderer_init_then_deinit_round_trips`.
        mod windowed_lifecycle {
            use objc2::runtime::{AnyObject, NSObject};

            use super::*;

            #[test]
            fn windowed_renderer_init_then_deinit_round_trips() {
                // Probe: skip cleanly on hosts with no Metal device (headless CI).
                if let Err(e) = Metal::init_offscreen(AlphaBlending::Native, 1, 1) {
                    // F5 strict-mode pivot (metal::test_support).
                    crate::metal::test_support::require_metal_or_skip(
                        "windowed_renderer_init_then_deinit_round_trips",
                        &format!("{e:?}"),
                    );
                    eprintln!("no Metal device; skipping windowed_lifecycle");
                    return;
                }

                // A real objc object — the `*mut S` handed to the generic surface hooks.
                // We use a bare `NSObject` (from the objc2 base crate — no AppKit
                // framework, which the lib test binary does not link, and which R4
                // forbids depending on) rather than a live NSView: the hooks IGNORE the
                // pointer (Metal declares neither `@hasDecl` hook — they are faithful
                // no-ops), and the design permits "any `*mut S`". A real objc object
                // still keeps the KAT honest about the bare generic `<S>` (`S = NSObject`,
                // erased to `*mut AnyObject`).
                let obj: objc2::rc::Retained<NSObject> = NSObject::new();
                // `&*obj` (&NSObject) → `*mut AnyObject`. The hooks never deref it, so a
                // shared-ref-derived cross-type `*mut` is sound (no aliasing write occurs).
                let view_ptr: *mut AnyObject = &*obj as *const NSObject as *mut AnyObject;

                // The apprt-free surface handshake — faithful no-ops for Metal.
                Renderer::surface_init(view_ptr).expect("surface_init (no-op)");

                let (font_grid, size) = test_font_grid_and_size(10, 3);
                let opts = Options {
                    config: derived_config_for_test(),
                    font_grid,
                    size,
                    // Offscreen: no surface, hence no mailbox (push returns 0).
                    surface_mailbox: SurfaceMailbox::DETACHED,
                };
                let mut r =
                    Renderer::init(Global, opts).expect("Renderer::init (Metal probed above)");

                // The other half of the handshake (still a no-op for Metal).
                r.finalize_surface_init(view_ptr)
                    .expect("finalize_surface_init (no-op)");

                // The windowed teardown must not panic/leak.
                r.deinit();
            }
        }

        // PORT NOTE (rustty-only, additive — no reference `test{}`): proves the
        // OFFSCREEN seam M-3 setter actually reaches the drawn surface, which is the
        // whole reason it exists. `set_offscreen_size` writes a private field; what
        // an embedder needs is for the NEXT frame to be that size. So: draw once at
        // the constructed size, resize, draw again, and read the swap-chain target
        // (the IOSurface an embedder composites) plus `size.screen` back out. Without
        // the setter both stay pinned to `init_offscreen`'s size forever.
        mod offscreen_resize {
            use super::*;

            /// Drive one real frame, as the golden tests do.
            fn draw_one_frame(r: &mut Renderer) {
                let mut mutex = Mutex::new();
                let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
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
                r.draw_frame(true).unwrap();
            }

            /// The target an embedder reads (`swap_chain.frames[frame_index].target`).
            fn target_size(r: &Renderer) -> (usize, usize) {
                let chain = r.swap_chain.as_ref().expect("a swap chain after a frame");
                let frame = &chain.frames[chain.frame_index as usize];
                (frame.target.width, frame.target.height)
            }

            #[test]
            fn set_offscreen_size_resizes_the_next_frames_surface() {
                let mut r = match golden_renderer(10, 3) {
                    Some(r) => r,
                    None => return, // Metal-skip
                };

                draw_one_frame(&mut r);
                let built = r.size.screen;
                // `golden_renderer` pins `offscreen_size == size.screen`, so the
                // first frame drew at the constructed size — the state an embedder
                // used to be stuck in.
                assert_eq!(
                    target_size(&r),
                    (built.width as usize, built.height as usize),
                    "the first frame must draw at the constructed offscreen size"
                );

                // A size no cell quantisation could have produced by itself, so a
                // pass cannot be an accident of the grid.
                let grown = ScreenSize {
                    width: built.width + 37,
                    height: built.height + 11,
                };
                r.set_offscreen_size(grown);
                assert_eq!(
                    r.api.surface_size().unwrap(),
                    crate::metal::metal_impl::SurfaceSize {
                        width: grown.width,
                        height: grown.height,
                    },
                    "the setter must move what `draw_frame` reads as the surface size"
                );

                draw_one_frame(&mut r);
                // `draw_frame`'s `size_changed` branch wrote `size.screen`...
                assert_eq!(r.size.screen, grown, "draw_frame must adopt the new size");
                // ...and `FrameState::resize` replaced the target behind it.
                assert_eq!(
                    target_size(&r),
                    (grown.width as usize, grown.height as usize),
                    "the swap-chain target must follow the new offscreen size"
                );

                r.deinit();
            }

            /// `set_screen_size` (the reference's `setScreenSize`) owns
            /// `size.padding`; `set_offscreen_size` owns the surface. They compose —
            /// neither clobbers the other — which is what an embedder relies on when
            /// it resizes the surface and the grid together.
            #[test]
            fn set_offscreen_size_and_set_screen_size_compose() {
                let mut r = match golden_renderer(10, 3) {
                    Some(r) => r,
                    None => return, // Metal-skip
                };

                let padding = Padding {
                    top: 3,
                    bottom: 4,
                    left: 5,
                    right: 6,
                };
                let mut size = r.size;
                size.padding = padding;
                r.set_screen_size(size);

                let grown = ScreenSize {
                    width: r.size.screen.width + 37,
                    height: r.size.screen.height + 11,
                };
                r.set_offscreen_size(grown);
                draw_one_frame(&mut r);

                assert_eq!(r.size.screen, grown, "the surface resize must land");
                assert_eq!(
                    r.size.padding, padding,
                    "and must not discard the padding `set_screen_size` set"
                );

                r.deinit();
            }
        }

        // F-014 PROVING TESTS: the surface-mailbox seam (GPU_RD_CHUNK3A_DESIGN §B.2)
        // is RESOLVED — the two `generic.zig` pushes actually reach the mailbox.
        // Before the fix `SurfaceMailbox::push` was a hard-coded `0`, so BOTH of
        // these tests observed an EMPTY sink and the scrollbar `defer` could never
        // clear `scrollbar_dirty`.
        mod surface_mailbox_routing {
            use std::sync::Arc;

            use super::*;
            use crate::generic::surface_mailbox_tests::RecordingSink;

            /// Drive one real frame: `update_frame` (which refreshes the scrollbar
            /// cache from the terminal and raises `scrollbar_dirty`) then
            /// `draw_frame(true)` (whose `defer` runs the scrollbar push). Returns
            /// the scrollbar value the push must carry.
            fn update_and_draw(r: &mut Renderer) -> Scrollbar {
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
                    inspector_ptr: None,
                    preedit: None,
                    mouse: state::Mouse::default(),
                    demand: Default::default(),
                    handoff_gen: Default::default(),
                    handoff_park: Default::default(),
                };
                r.update_frame(&st, true).unwrap();
                // Guard against a vacuous test: the fresh terminal's scrollbar must
                // differ from the `Scrollbar::ZERO` the renderer was built with, so
                // `update_frame` raised the dirty flag (generic.zig:1247-1250).
                assert!(
                    r.scrollbar_dirty,
                    "update_frame must have cached a fresh scrollbar (dirty)"
                );
                let scrollbar = r.scrollbar;
                r.draw_frame(true).unwrap();
                scrollbar
            }

            /// generic.zig:1725-1737 — `frameCompleted` pushes `.renderer_health`
            /// with `.forever` ONLY on a health TRANSITION.
            #[test]
            fn frame_completed_pushes_renderer_health_on_transition_only() {
                let mut r = match golden_renderer(10, 3) {
                    Some(r) => r,
                    None => return, // Metal-skip
                };
                let sink = Arc::new(RecordingSink::accepting());
                r.surface_mailbox = SurfaceMailbox::new(sink.clone());

                // `health` starts `.healthy`, so this IS a transition → one push.
                r.frame_completed(crate::Health::Unhealthy);
                // Identical health → `if (self.health.load(...) != health)` is false
                // → NO second push.
                r.frame_completed(crate::Health::Unhealthy);

                assert_eq!(
                    sink.pushed(),
                    vec![(
                        SurfaceMessage::RendererHealth(crate::Health::Unhealthy),
                        MailboxTimeout::Forever
                    )]
                );
            }

            /// generic.zig:1449-1456 — the `drawFrame` `defer` pushes `.scrollbar`
            /// with `.instant` and clears `scrollbar_dirty` when the push lands.
            #[test]
            fn draw_frame_pushes_scrollbar_and_clears_dirty() {
                let mut r = match golden_renderer(10, 3) {
                    Some(r) => r,
                    None => return, // Metal-skip
                };
                let sink = Arc::new(RecordingSink::accepting());
                r.surface_mailbox = SurfaceMailbox::new(sink.clone());

                let scrollbar = update_and_draw(&mut r);

                assert_eq!(
                    sink.pushed(),
                    vec![(
                        SurfaceMessage::Scrollbar(scrollbar),
                        MailboxTimeout::Instant
                    )]
                );
                assert!(
                    !r.scrollbar_dirty,
                    "a landed scrollbar push must clear scrollbar_dirty"
                );
            }

            /// The other arm of the same `if (... > 0)`: "Fail instantly if the
            /// surface mailbox if full, we'll just get it on the next frame" —
            /// a refused push must KEEP `scrollbar_dirty` set.
            #[test]
            fn draw_frame_keeps_scrollbar_dirty_when_the_mailbox_is_full() {
                let mut r = match golden_renderer(10, 3) {
                    Some(r) => r,
                    None => return, // Metal-skip
                };
                let sink = Arc::new(RecordingSink::refusing());
                r.surface_mailbox = SurfaceMailbox::new(sink.clone());

                let scrollbar = update_and_draw(&mut r);

                assert_eq!(
                    sink.pushed(),
                    vec![(
                        SurfaceMessage::Scrollbar(scrollbar),
                        MailboxTimeout::Instant
                    )]
                );
                assert!(
                    r.scrollbar_dirty,
                    "a refused scrollbar push must keep scrollbar_dirty set"
                );
            }
        }

        // Sample the BGRA byte 4-tuple at pixel (x, y) in a tight `w*h*4` readback
        // (`read_bgra` returns `[B,G,R,A]` per pixel, rows tight). Returns (b,g,r,a).
        fn px_at(px: &[u8], w: usize, x: usize, y: usize) -> (u8, u8, u8, u8) {
            let i = (y * w + x) * 4;
            (px[i], px[i + 1], px[i + 2], px[i + 3])
        }

        // Encode a `w`×`h` solid-RGB PNG for the bg-image golden (T3) — the reference
        // `background-image` fixture is not checked in, so generate it at test time via
        // the pure-Rust `png` encoder (test-helper duplication precedent — `opts`).
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

        // GPU_RD_CHUNK4_DESIGN §3 — the R-d contract's FIRST real GPU-output assertion:
        // render a deterministic terminal state OFFSCREEN (Metal → IOSurface), read the
        // pixels back, and assert the STRUCTURE the port is responsible for (§3.1: not an
        // exact-PNG gate — that would be flaky across FreeType/metal-compiler versions —
        // but structural asserts with thresholds lenient enough to survive the P3
        // conversion / premultiply / BGRA8Unorm rounding, yet non-vacuous). macOS +
        // `#[cfg(test)]`; Metal-skip on a headless host.
        //
        // DETERMINISM PINS (§3.2 / OPEN RISK #5, load-bearing): `AlphaBlending::Native`
        // (⇒ `BGRA8Unorm`, no store-time sRGB re-encode), `colorspace = Srgb` (⇒
        // `use_display_p3 = false`), and `offscreen_size == size.screen` (⇒
        // `size_changed == false`). If a future change flips blending to linear the
        // target becomes `_sRGB` and the readback semantics shift.
        // W3 §5 (bounded-pump Surface::draw KAT). `Surface::draw` (core) forwards
        // one line to `drawFrame(true)` -> the real `draw_frame(true)` ("Renderers
        // are required to support `drawFrame` being called from the main thread",
        // Surface.zig:884). A full core `Surface::draw` unit test needs the whole
        // apprt mock + a live windowing host (the windowing-integration milestone),
        // so this bounded N-frame pump lands at the renderer's offscreen harness and
        // proves the main-thread-present leg is safe to pump repeatedly + leaves the
        // renderer Healthy.
        //
        // Coverage boundary: this does NOT drive the `threadMain` event loop or any
        // callback-driven draw — that path is proven by the xev-stubbed mirihar
        // `renderthread_shipped` candidate + `cargo build` type-checking the real
        // `threadMain`/callbacks against xev/renderer signatures.
        #[test]
        fn draw_frame_bounded_pump() {
            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // Metal-skip
            };
            // bounded — never a live loop.
            for _ in 0..8 {
                r.draw_frame(true).expect("main-thread draw_frame(true)");
                assert_eq!(r.health.load(), crate::Health::Healthy);
            }
            // deinit path clean.
            r.deinit();
        }

        #[test]
        fn golden_renders_cursor_fg_bg() {
            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // Metal-skip
            };

            // "Ab" on a 10×3 grid ⇒ a block cursor at grid (2,0) (the empty cell after
            // "Ab"; focused + blink_visible ⇒ block). Verified by the 3b test.
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
                inspector_ptr: None,
                preedit: None,
                mouse: state::Mouse::default(),
                // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
                demand: Default::default(),
                handoff_gen: Default::default(),
                handoff_park: Default::default(),
            };
            // Drive the full pipeline through update_frame (3c): fills cells + uniforms;
            // rebuild_cells sets dirty. cursor_blink_visible=true ⇒ visible block cursor.
            r.update_frame(&st, true).unwrap();
            // sync ⇒ commit + waitUntilCompleted + the completion trampoline runs.
            r.draw_frame(true).unwrap();

            // Read back the IOSurface pixels of the frame just drawn (all fields pub;
            // read_bgra handles bytesPerRow).
            let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
            let px = r.swap_chain.as_ref().unwrap().frames[idx]
                .target
                .read_bgra();

            let w = r.size.screen.width as usize;
            let cw = r.grid_metrics.cell_width as usize;
            let ch = r.grid_metrics.cell_height as usize;
            assert_eq!(px.len(), w * (r.size.screen.height as usize) * 4);

            // (A) CURSOR CELL — solid red block at grid (2,0). Sample the cell interior
            //     center. Can only pass if `cell_text` ran with `is_cursor_glyph` set +
            //     `cursor_color` red reached the shader ⇒ proves the packed `bools` byte
            //     (bit 1) AND the uniform serialization.
            let (b, g, rr, _a) = px_at(&px, w, 2 * cw + cw / 2, ch / 2);
            assert!(rr > 128, "cursor cell not bright-red (R={rr})");
            assert!(
                rr > g && rr > b,
                "cursor cell not red-dominant (R={rr} G={g} B={b})"
            );

            // (B) BACKGROUND — config bg is black (0,0,0), opacity 1.0 ⇒ bg_color =
            //     [0,0,0,255]. Sample an empty cell far from glyphs/cursor (grid (8,2),
            //     bottom row): cell_bg is transparent there (alpha 0) ⇒ the opaque
            //     full-screen bg_color layer shows through. The RGB check ALONE is
            //     VACUOUS here (config bg RGB == clear RGB == 0; bg_color_fragment
            //     premultiplies rgb*a ⇒ also RGB 0), so we ALSO assert the ALPHA: the
            //     bg_color step writes an opaque layer (a=255) whereas the render-pass
            //     clear leaves a=0 ⇒ `ba > 128` can only pass if the full-screen bg_color
            //     step ran AND `bg_color.a = 255` serialized (byte 131 = offset 128 + 3).
            let (bb, bg2, br, ba) = px_at(&px, w, 8 * cw + cw / 2, 2 * ch + ch / 2);
            assert!(
                br < 96 && bg2 < 96 && bb < 96,
                "background not dark (R={br} G={bg2} B={bb})"
            );
            assert!(
                ba > 128,
                "bg_color step absent / bg alpha not opaque (A={ba})"
            );

            // (C) FOREGROUND GLYPH present in row 0 — "A" at grid (0,0) is white. Scan
            //     the cell for a bright mark (antialiased glyph ⇒ max luminance over the
            //     cell). Proves the atlas sync + `cell_text` glyph sampling.
            let mut maxlum = 0u32;
            for yy in 0..ch {
                for xx in 0..cw {
                    let (b3, g3, r3, _) = px_at(&px, w, xx, yy);
                    maxlum = maxlum.max((r3 as u32 + g3 as u32 + b3 as u32) / 3);
                }
            }
            assert!(
                maxlum > 128,
                "no fg glyph rendered in cell (0,0) (maxlum={maxlum})"
            );

            t.deinit(Global);
        }

        // W5 HEADLESS KAT (`W5_DESIGN §5`): prove the display-callback trampoline
        // end-to-end with NO window server / live render thread. Build a `Renderer`
        // with a real (headless) `IOSurfaceLayer`, run the REAL `Metal::loop_enter`
        // (container_of + `set_display_callback`), fire the layer's ObjC `display`
        // method directly (exactly the message Core Animation sends on the main thread
        // during a `needsDisplayOnBoundsChange` commit), and assert a full sync frame
        // ran through `display_callback` → `draw_frame_raw`.
        //
        // Single-threaded, so it proves the FUNCTIONAL leg only — it exercises ZERO
        // cross-thread aliasing. The concurrency soundness (§2.5's raw-self re-route)
        // is covered SOLELY by the `mirihar-chunk3b` `displaycb_concurrent`/
        // `displaycb_naive` candidates (§6); neither artifact substitutes for the other.
        #[test]
        fn loop_enter_display_callback_draws_a_frame() {
            use crate::metal::iosurface_layer::IOSurfaceLayer;
            use objc2::msg_send;
            use objc2_core_foundation::{CGPoint, CGRect, CGSize};

            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // Metal-skip
            };

            // Give the terminal some content so the draw is non-trivial (mirrors golden).
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
                inspector_ptr: None,
                preedit: None,
                mouse: state::Mouse::default(),
                // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
                demand: Default::default(),
                handoff_gen: Default::default(),
                handoff_park: Default::default(),
            };
            r.update_frame(&st, true).unwrap();

            // Inject a headless IOSurfaceLayer (offscreen ctor leaves layer = None). All
            // fields are pub. `IOSurfaceLayer::init()` is window-server-free (W0 tests).
            let layer = IOSurfaceLayer::init().expect("IOSurfaceLayer::init");
            // Size it so surface_size == size.screen (scale 1 ⇒ size_changed == false ⇒
            // the frame draws the existing cells rather than taking the resize path).
            layer.as_ca_layer().setContentsScale(1.0);
            layer.as_ca_layer().setBounds(CGRect::new(
                CGPoint::new(0.0, 0.0),
                CGSize::new(r.size.screen.width as f64, r.size.screen.height as f64),
            ));
            r.api.layer = Some(layer);

            // Install the display callback via the REAL seam (container_of +
            // set_display_callback). `loop_enter` needs a whole-`Renderer`-provenance
            // `*mut Metal` (§2.4).
            let self_ptr: *mut Renderer = &mut r;
            let api_ptr: *mut Metal = unsafe { &raw mut (*self_ptr).api };
            unsafe { Metal::loop_enter(api_ptr) };

            // Arm an observable: draw_frame(true) drives the sync completion trampoline,
            // which sets health = Healthy. Pre-set Unhealthy so a no-op / early-return
            // fails loudly.
            r.health.store(crate::Health::Unhealthy);

            // Fire the ObjC `display` method — exactly what Core Animation sends on the
            // main thread during a needsDisplayOnBoundsChange commit. This drives
            // display_callback → draw_frame_raw(self_ptr, true).
            let disp_layer = r.api.layer.as_ref().unwrap();
            let _: () = unsafe { msg_send![&*disp_layer.layer, display] };

            // The trampoline ran a full sync frame: completion set health = Healthy …
            assert_eq!(
                r.health.load(),
                crate::Health::Healthy,
                "display → display_callback → draw_frame_raw must run a full frame"
            );
            // … and present(sync) set the layer contents to the drawn IOSurface.
            let contents: Option<objc2::rc::Retained<objc2::runtime::AnyObject>> =
                unsafe { r.api.layer.as_ref().unwrap().layer.contents() };
            assert!(
                contents.is_some(),
                "sync present must set the layer contents"
            );

            r.deinit();
            t.deinit(Global);
        }

        // A minimal ObjC test-double for the host NSView. `Metal::init` (the on-screen
        // constructor) attaches its `IOSurfaceLayer` by sending the layer-hosting
        // selectors `setLayer:` / `setWantsLayer:` / `setClipsToBounds:` to a
        // type-erased `&AnyObject` (R4 — never names an app-kit type), then the W1-6
        // handshake reads `[view layer]` back. This double responds to EXACTLY those
        // four selectors — storing the `setLayer:` id and returning it from `layer` —
        // so `Metal::init`'s present-path msg_sends run byte-identically to a real
        // NSView, while the object is a PLAIN `NSObject` subclass (define_class!, the
        // W0 `IOSurfaceLayer` precedent).
        //
        // WHY a double instead of a real `[NSView new]` (which the W1 design sketched):
        // the lib test binary links only QuartzCore + Metal — `NSView`/AppKit is NOT
        // loaded (R4 forbids the `objc2-app-kit` dep), so `class!(NSView)` would panic;
        // and even after `dlopen`-ing AppKit, `NSView` mutation (`setWantsLayer:` …)
        // enforces MAIN-THREAD use and THROWS an ObjC exception on cargo's parallel
        // test worker threads (an uncatchable foreign exception → `SIGABRT`), which no
        // `dispatch_sync` can rescue (the harness main thread runs no runloop). A plain
        // `NSObject` subclass has no AppKit main-thread constraint, so the KAT is robust
        // under the default parallel `cargo test` — the faithful headless test vehicle
        // (the visible NSWindow is W4). It exercises the SAME `Metal::init` code path;
        // only the receiver of the (side-effect-free-for-Metal) attach selectors is the
        // double rather than a live view.
        use objc2::DefinedClass as _;
        struct MockHostViewIvars {
            layer: core::cell::Cell<*mut objc2::runtime::AnyObject>,
        }
        objc2::define_class!(
            // SAFETY: NSObject has no subclassing requirements; no Drop impl.
            #[unsafe(super(objc2::runtime::NSObject))]
            #[name = "RusttyMockHostView"]
            #[ivars = MockHostViewIvars]
            struct MockHostView;

            impl MockHostView {
                // `[view setLayer: l]` — store the CALayer id (arrives as an object ptr).
                #[unsafe(method(setLayer:))]
                fn set_layer(&self, layer: *mut objc2::runtime::AnyObject) {
                    self.ivars().layer.set(layer);
                }
                // `[view layer]` — return the stored layer (the W1-6 handshake reads it).
                #[unsafe(method(layer))]
                fn layer(&self) -> *mut objc2::runtime::AnyObject {
                    self.ivars().layer.get()
                }
                // The remaining two attach selectors are no-ops for a layer double.
                #[unsafe(method(setWantsLayer:))]
                fn set_wants_layer(&self, _wants: bool) {}
                #[unsafe(method(setClipsToBounds:))]
                fn set_clips_to_bounds(&self, _clips: bool) {}
            }
        );

        // W1-6 CULMINATING KAT — FIRST PIXELS on-screen. Render one real terminal
        // frame OFFSCREEN (the proven golden path), then present it through the REAL
        // windowed present path: `Metal::init(view)` (the on-screen constructor —
        // creates an `IOSurfaceLayer`, attaches it to the host view as a layer-hosting
        // view) → `present(target, sync)` → `IOSurfaceLayer::set_surface_sync` →
        // `CALayer.setContents(surface)`. Then assert (a) the drawn pixels are genuine
        // terminal red (inherits the offscreen golden gate's pixel parity via the same
        // `px_at` cursor-cell probe), (b) the layer's `contents` IS the exact IOSurface
        // we drew (FIRST PIXELS), and (c) the view hosts our layer. The visible
        // NSWindow is W4 — this KAT is HEADLESS-friendly (no WindowServer / run loop):
        // the host view is a plain-`NSObject` layer-host double (see `MockHostView`).
        //
        // R4/headless: `rustty-renderer` MUST NOT depend on `objc2-app-kit` (no
        // compile-time app-kit edge, `check_acyclic.sh`). The `MockHostView` double
        // needs no AppKit at all — it responds to the four layer-host selectors
        // `Metal::init` sends, and is main-thread-safe under parallel `cargo test`.
        // Skips first on no-Metal (via `golden_renderer`). FFI/objc2 ⇒ Miri-excluded,
        // ASan-eligible (R6). SB/TB: `present` takes `&Target` from `r.swap_chain`
        // (shared) while `r` is not mutably borrowed across the call — no `&mut` alias.
        #[test]
        fn golden_presents_frame_on_screen() {
            use core::ffi::c_void;

            use objc2::msg_send;
            use objc2::rc::Retained;
            use objc2::runtime::AnyObject;
            use objc2::AnyThread;
            use objc2_io_surface::IOSurfaceRef;
            use objc2_quartz_core::CALayer;

            // --- (1) DRAW one real terminal frame OFFSCREEN (the proven golden path).
            //         Probe-skips on a host with no Metal device.
            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // Metal-skip
            };

            // "Ab" on a 10×3 grid ⇒ a block cursor at grid (2,0) (the empty cell after
            // "Ab") — identical to the offscreen cursor golden.
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

            let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;

            // --- (2) CONFIRM real pixels (reuse the offscreen golden assert). The
            //         cursor cell (grid (2,0)) interior center is a solid red block ⇒
            //         genuine terminal pixels, not a blank/uninitialized surface.
            let px = r.swap_chain.as_ref().unwrap().frames[idx]
                .target
                .read_bgra();
            let w = r.size.screen.width as usize;
            let cw = r.grid_metrics.cell_width as usize;
            let ch = r.grid_metrics.cell_height as usize;
            let (b, g, rr, _a) = px_at(&px, w, 2 * cw + cw / 2, ch / 2);
            assert!(
                rr > 128 && rr > g && rr > b,
                "no real terminal pixels to present (R={rr} G={g} B={b})"
            );

            // --- (3) PRESENT on-screen via the REAL windowed path. FIRST PIXELS.
            // Build the layer-host double (a plain NSObject subclass — no AppKit).
            let view: Retained<MockHostView> = {
                let this = MockHostView::alloc().set_ivars(MockHostViewIvars {
                    layer: core::cell::Cell::new(core::ptr::null_mut()),
                });
                unsafe { msg_send![super(this), init] }
            };
            // `&*view` (`&MockHostView`) deref-coerces to `&AnyObject` (via
            // MockHostView → NSObject → AnyObject) at the `Metal::init` call site.
            let view_any: &AnyObject = &view;

            // `present` takes `&mut self` ⇒ `let mut onscreen`.
            let mut onscreen = Metal::init(view_any, 1.0, AlphaBlending::Native)
                .expect("Metal::init (Metal probed above)");

            // present(sync=true) → set_surface_sync → CALayer.setContents(surface).
            onscreen
                .present(&r.swap_chain.as_ref().unwrap().frames[idx].target, true)
                .expect("present");

            // --- (4) ASSERT the handshake.
            let layer = onscreen
                .layer
                .as_ref()
                .expect("on-screen Metal has a layer");
            // (a) the layer's `contents` IS the exact IOSurface we drew. `contents()`
            //     is an `unsafe fn` (objc2-quartz-core `CALayer.rs`).
            let contents: Retained<AnyObject> =
                unsafe { layer.as_ca_layer().contents() }.expect("layer contents set by present");
            let want_surface = (&*r.swap_chain.as_ref().unwrap().frames[idx].target.surface
                as *const IOSurfaceRef)
                .cast::<c_void>();
            let got_surface = Retained::as_ptr(&contents) as *const c_void;
            assert_eq!(
                got_surface, want_surface,
                "layer contents must be the presented IOSurface (FIRST PIXELS)"
            );
            // (b) the view hosts our layer (Metal::init's setLayer: stored it in the
            //     double; `[view layer]` returns that stored id).
            let hosted: *mut AnyObject = unsafe { msg_send![&*view, layer] };
            let want_layer = (layer.as_ca_layer() as *const CALayer).cast::<c_void>();
            assert_eq!(
                hosted as *const c_void, want_layer,
                "view must host our IOSurfaceLayer"
            );

            t.deinit(Global);
        }

        // T3 (RENDERER_BG_IMAGE_DESIGN §4) — the offscreen proof that the whole
        // background-image pipeline executes end-to-end: point the config at a real
        // PNG on disk, `prep_background_image` (open + FileType sniff + wuffs decode ->
        // `bg_image = Some(Pending)`), `update_bg_image_buffer` (opacity/fit ->
        // `BgImageGpu`), `draw_frame` (`upload_background_image` -> `Ready(texture)` +
        // the full-screen bg_image render step), then read back the IOSurface and
        // assert the sampled pixels are the DECODED image color — distinct from the
        // plain `bg_color` (black) the negative control shows. macOS/Metal-gated;
        // Metal-skip on a headless host (same pattern as the cursor golden).
        //
        // DETERMINISM PINS (mirroring the cursor golden): `AlphaBlending::Native`
        // (BGRA8Unorm, no store-time sRGB re-encode), `colorspace = Srgb`,
        // `offscreen == size.screen`. `fit = Stretch` so the solid image fills the
        // whole screen ⇒ every pixel is the image color (trivial sampling).
        #[test]
        fn golden_renders_background_image() {
            use std::ffi::CString;

            // The distinct bg-image color (blue-dominant), unambiguously != black.
            const IMG_RGB: [u8; 3] = [0x30, 0x60, 0xC0];

            // Helper: build a golden renderer, optionally with a bg image at `path`,
            // draw one frame, and return the center-pixel BGRA of the bottom row.
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
                        matches!(r.bg_image, Some(image::Image::Pending(_))),
                        "prep did not stage a pending bg image"
                    );
                }

                // An (empty) terminal drives update_frame/draw_frame (as the cursor
                // golden does); the bg-image step does not depend on cell content.
                let mut mutex = Mutex::new();
                let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
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

                if with_bg.is_some() {
                    // After the frame, the pending image was uploaded to the GPU.
                    assert!(
                        matches!(r.bg_image, Some(image::Image::Ready(_))),
                        "bg image not uploaded to a Ready texture"
                    );
                }

                let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
                let px = r.swap_chain.as_ref().unwrap().frames[idx]
                    .target
                    .read_bgra();
                let w = r.size.screen.width as usize;
                let cw = r.grid_metrics.cell_width as usize;
                let ch = r.grid_metrics.cell_height as usize;
                // Sample the center of an empty cell on the bottom row (grid (5,2)).
                let out = px_at(&px, w, 5 * cw + cw / 2, 2 * ch + ch / 2);
                t.deinit(Global);
                Some(out)
            }

            // Write the solid bg-image PNG to a temp file.
            let png_bytes = encode_solid_png(2, 2, IMG_RGB);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("rustty_bg_{nanos}.png"));
            std::fs::write(&path, &png_bytes).unwrap();

            // (bg image) — sample should be the decoded image color.
            let bg = match draw_center(Some(&path)) {
                Some(v) => v,
                None => {
                    let _ = std::fs::remove_file(&path);
                    return; // Metal-skip
                }
            };
            // (negative control) — no bg image ⇒ the plain black bg_color.
            let none = draw_center(None).expect("Metal probed above");
            let _ = std::fs::remove_file(&path);

            // read_bgra returns (B, G, R, A). The image is blue-dominant (0x30,0x60,0xC0):
            //   B ≈ 0xC0 (>128), R ≈ 0x30 (<128), B > R, and the layer is opaque.
            let (bb, _bg2, br, ba) = bg;
            assert!(bb > 128, "bg-image blue channel too low (B={bb})");
            assert!(br < 128, "bg-image red channel too high (R={br})");
            assert!(bb > br, "bg-image not blue-dominant (B={bb} R={br})");
            assert!(ba > 128, "bg-image layer not opaque (A={ba})");

            // The negative control is dark (black bg_color) — proves the bg-image step
            // (not the bg_color fallback) produced the color above.
            let (nb, ng, nr, _na) = none;
            assert!(
                nb < 96 && ng < 96 && nr < 96,
                "negative control not dark (B={nb} G={ng} R={nr})"
            );
            assert!(
                bb > nb + 64,
                "bg-image blue not distinct from bg_color (bg B={bb} vs none B={nb})"
            );
        }

        // RENDERER_CUSTOM_SHADERS_DESIGN §4.2 — the offscreen end-to-end proof that the
        // custom-shader (shadertoy) post-process path executes. Point the config at an
        // on-disk pass-through shader, run `init_shaders` (load + transpile MSL + build
        // the post pipelines + flip `has_custom_shaders`), then drive one frame with
        // visible text. The Chunk-4 `draw_frame` passes (retarget the cell pass to
        // `back_texture`, then the ping-pong post loop blitting back to `frame.target`)
        // run for the FIRST time here. Assert the per-frame `CustomShaderState` was
        // created, the post pipelines were built, and the pass-through preserved the
        // cells (non-black AND not a uniform fill — a black frame is the empty-
        // `post_pipelines` failure mode in OPEN RISK R1). macOS/Metal-gated; Metal-skip.
        //
        // Assertion strategy (§4.2): the "recognizable" contract is the robustness
        // check — a non-black pixel AND a non-uniform frame — NOT an exact baseline
        // diff (the pass-through samples `texture(iChannel0, gl_FragCoord/iResolution)`
        // through a Metal sampler, so sub-LSB filtering can perturb an exact match while
        // leaving the image recognizable). Both halves still fail the real regressions:
        // an empty-pipeline black frame fails non-black; a uniform fill fails non-uniform.
        #[test]
        fn golden_renders_custom_shader() {
            use std::ffi::CString;
            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // Metal-skip
            };

            // Write the pass-through shadertoy body to a nanos-suffixed temp file
            // (mirrors the bg-image golden).
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("rustty_shader_{nanos}.glsl"));
            std::fs::write(
                &path,
                b"void mainImage(out vec4 fragColor, in vec2 fragCoord){\n\
                  fragColor = texture(iChannel0, fragCoord / iResolution.xy);\n}\n",
            )
            .unwrap();

            // Point the config at the on-disk pass-through shader (required arm),
            // byte-faithful path -> CString (mirrors the bg-image golden).
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

            // Drive one frame with a non-empty terminal (write visible text so the cell
            // pass produces a recognizable, non-black image the pass-through preserves).
            let mut mutex = Mutex::new();
            let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
            t.print_string(b"HELLO").unwrap();
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

            // The draw_frame branch lazily created the per-frame CustomShaderState.
            let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
            assert!(
                r.swap_chain.as_ref().unwrap().frames[idx]
                    .custom_shader_state
                    .is_some(),
                "custom_shader_state not created by draw_frame"
            );

            // Pass-through = identity: the cells survive to frame.target.
            let px = r.swap_chain.as_ref().unwrap().frames[idx]
                .target
                .read_bgra();
            // Non-black: some pixel has a bright channel (the white "HELLO" glyphs).
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

            t.deinit(Global);
            let _ = std::fs::remove_file(&path);
        }

        // RENDERER_KITTY_IMAGE_PATH_DESIGN §4 — the offscreen APC-driven proof that
        // the 5 filled Kitty pixel-path seams (`kitty_update` +
        // `prep_kitty_placement`/`prep_kitty_virtual_placement`/`prep_kitty_image` +
        // `Image::convert`) execute end-to-end: drive a real `Terminal` with a
        // Kitty-graphics transmit+display APC, run `update_frame` (which enters
        // `kitty_update` via the `kitty_requires_update` guard) then `draw_frame`
        // (which uploads pending images to the GPU), and assert the STATE the path
        // produces. macOS/Metal-gated; Metal-skip like the golden.
        #[test]
        fn kitty_image_pixel_path() {
            let mut r = match golden_renderer(10, 3) {
                Some(r) => r,
                None => return, // Metal-skip
            };

            // Transmit AND display (a=T) a 1x2 direct RGB image (f=24), id 1, all
            // pixels 0xFF (`////////` = 6 bytes of 0xFF). a=T places it at the
            // cursor → a pin placement, marking storage dirty.
            let mut mutex = Mutex::new();
            let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
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
            r.terminal_state.update(Global, &mut t).unwrap();
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
            // `Texture::init` run → the image is Ready with a texture.
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

            t.deinit(Global);
        }

        // PORT-SIDE REGRESSION (`a177ba90a`, gap-close): the display-link lifecycle
        // refactor. The reference added no `test {}` for it — every branch that
        // succeeds needs a real CoreVideo display — but the two GUARD CLAUSES of
        // `sync_display_link` and the new `visible` field are observable without one,
        // and they are exactly what the pre-refactor port got wrong: it created the
        // link EAGERLY in `init` (so `create_with_active_cg_displays` failing on a
        // locked session aborted renderer init instead of falling back to
        // event-driven rendering) and it never stored visibility (so focusing an
        // occluded surface spun vsync back up).
        mod display_link_sync {
            use super::*;

            /// `if (!self.config.vsync) return;` — vsync off never creates a link,
            /// even from the one call site that carries a `draw_now`.
            #[test]
            fn sync_without_vsync_never_creates_a_link() {
                let mut r = match test_renderer(10, 3) {
                    Some(r) => r,
                    None => return, // skip w/o Metal
                };
                r.config.vsync = false;
                let draw_now = Async::init().expect("xev Async");
                r.sync_display_link(None, Some(draw_now));
                assert!(r.display_link.is_none());
                r.deinit();
            }

            /// `const callback = draw_now orelse return;` — a sync with no
            /// `draw_now` (focus/visibility changes, and a display change that
            /// arrives before `loop_enter`) must not create a link, because the
            /// output callback would have no userdata to notify.
            #[test]
            fn sync_without_draw_now_never_creates_a_link() {
                let mut r = match test_renderer(10, 3) {
                    Some(r) => r,
                    None => return, // skip w/o Metal
                };
                r.config.vsync = true;
                r.sync_display_link(None, None);
                assert!(r.display_link.is_none());

                // A display-id sync is the "re-adds itself" path and is equally
                // barred while there is nothing to call back into.
                r.sync_display_link(Some(1), None);
                assert!(r.display_link.is_none());
                r.deinit();
            }

            /// `self.visible = visible;` — the field the pre-refactor port dropped.
            /// `syncDisplayLink` gates start/stop on `visible` (since `97f57edcc` +
            /// `6688aa072`: `visible and (cells_rebuilt or animationWake() != null)`;
            /// before them `visible and focused`), so losing it silently degraded to
            /// dirtiness-only (then focus-only) gating.
            #[test]
            fn set_visible_tracks_visibility_state() {
                let mut r = match test_renderer(10, 3) {
                    Some(r) => r,
                    None => return, // skip w/o Metal
                };
                assert!(r.visible, "init starts visible (.visible = true)");

                r.set_visible(false);
                assert!(!r.visible);

                // Focusing while hidden must not flip visibility back.
                r.focused = false;
                r.set_focus(true).unwrap();
                assert!(!r.visible);

                r.set_visible(true);
                assert!(r.visible);
                r.deinit();
            }

            /// PORT-SIDE REGRESSION (`97f57edcc` + `6688aa072`, phase-6): the
            /// end-state predicate `should_run = visible and (cells_rebuilt or
            /// animationWake() != null)`. The link runs only while a rebuilt frame
            /// or an animation is pending, is parked (stopped) once idle, focus no
            /// longer participates, and a hidden surface never runs it. The
            /// reference added no `test {}` (it needs a real CoreVideo link), so
            /// this one creates the link for real and skips when
            /// `createWithActiveCGDisplays` fails (a headless/locked session).
            ///
            /// NEGATIVE CONTROL: restoring the pre-range `visible and focused`
            /// predicate fails the first assertion (the idle-but-focused surface
            /// keeps vsync spinning) or, with `focused == false`, the second (a
            /// dirty unfocused surface never starts it).
            #[test]
            fn sync_parks_the_link_while_idle_and_runs_it_while_dirty() {
                let mut r = match test_renderer(10, 3) {
                    Some(r) => r,
                    None => return, // skip w/o Metal
                };
                let running = |r: &Renderer| r.display_link.as_ref().map(|l| l.is_running());

                r.config.vsync = true;
                let draw_now = Async::init().expect("xev Async");
                r.sync_display_link(None, Some(draw_now));
                if r.display_link.is_none() {
                    // No active display (headless / locked session): the
                    // fallback-rendering arm, covered by the guard tests above.
                    // R14: a skip here is visible, and a failure under
                    // `RUSTTY_REQUIRE_DISPLAY=1`.
                    r.deinit();
                    crate::metal::test_support::require_display_or_skip(
                        "sync_parks_the_link_while_idle_and_runs_it_while_dirty",
                        &"createWithActiveCGDisplays failed (no active display)",
                    );
                    eprintln!(
                        "no active display; skipping \
                         sync_parks_the_link_while_idle_and_runs_it_while_dirty"
                    );
                    return;
                }

                // Freshly created, visible, nothing rebuilt, no animation
                // (`test_renderer` has no custom shaders and no kitty clock):
                // idle -> parked, whatever the focus state.
                assert!(r.visible);
                assert!(!r.cells_rebuilt);
                assert!(r.animation_wake().is_none());
                assert_eq!(running(&r), Some(false), "idle surface must park the link");

                // A rebuilt frame un-parks it, focused or not.
                r.cells_rebuilt = true;
                r.sync_display_link(None, None);
                assert_eq!(running(&r), Some(true), "dirty surface must run the link");

                // `if (!display_link.isRunning()) start()` — a second sync while
                // already running is a no-op, not a double start.
                r.sync_display_link(None, None);
                assert_eq!(running(&r), Some(true));

                // Idle again (the draw consumed `cells_rebuilt`): parked.
                r.cells_rebuilt = false;
                r.sync_display_link(None, None);
                assert_eq!(
                    running(&r),
                    Some(false),
                    "idle surface must re-park the link"
                );

                // Non-visible windows never vsync, dirty or not.
                r.cells_rebuilt = true;
                r.set_visible(false);
                assert_eq!(
                    running(&r),
                    Some(false),
                    "hidden surface never runs the link"
                );

                // Visible again with the rebuilt frame still pending: runs.
                r.set_visible(true);
                assert_eq!(running(&r), Some(true));

                r.deinit();
            }
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
            // semantic-prompt render body now draws for real
            // (`overlay.rs::highlight_semantic_prompts` — the z2d hairline fill landed),
            // so we drive the build path directly. Post-build, the overlay is present.
            r.rebuild_overlay(&[crate::overlay::Feature::SemanticPrompts])
                .unwrap();
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
                inspector_ptr: None,
                preedit: None,
                mouse: state::Mouse::default(),
                // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
                demand: Default::default(),
                handoff_gen: Default::default(),
                handoff_park: Default::default(),
            };
            r.update_frame(&st, true).unwrap();

            // Contents filled (same asserts as rebuild_cells_fills_bg_fg_cursor,
            // now driven end-to-end):
            let bg = r.terminal_state.colors.background;
            assert_eq!(*r.cells.bg_cell(0, 0), [bg.r, bg.g, bg.b, 0]);
            assert!(!r.cells.fg_rows[1].is_empty());
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

        // Parity S115-F1: `defer self.font_shaper.endFrame()` is updateFrame's
        // first statement, so a frame skipped for synchronized output still
        // hands the shaper's CoreText objects to the release thread.
        #[test]
        fn update_frame_ends_the_shaper_frame_on_the_sync_output_return() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };

            // Shape a row outside `update_frame`, so the shaper's release pool
            // holds this frame's CoreText objects.
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
            assert!(r.font_shaper.release_pool_len() > 0);
            assert_ne!(r.terminal_state.dirty, Dirty::False);

            t.modes.set(Mode::SynchronizedOutput, true);
            let mut mutex = Mutex::new();
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

            // The early return was taken: the dirty reset at the end of the
            // body did not run (it is registered after that return in the
            // reference too).
            assert_ne!(r.terminal_state.dirty, Dirty::False);
            // ...and the shaper's frame was still ended.
            assert_eq!(0, r.font_shaper.release_pool_len());

            t.deinit(Global);
        }

        // 9e6e2ea96 — renderer: reset terminal state cleanup counter. The frame
        // counter must restart whenever the retained render state is cleared;
        // otherwise every subsequent frame deinitializes and rebuilds the full
        // state (perf collapse after ~12 minutes at 120Hz).
        #[test]
        fn update_frame_resets_terminal_state_frame_count() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };

            let mut mutex = Mutex::new();
            let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
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

            // Drive the counter to the cleanup threshold (the ~12-minutes-at-120Hz
            // mark). This frame performs the SCHEDULED deinit+rebuild...
            r.terminal_state_frame_count = 100_000;
            r.update_frame(&st, true).unwrap();
            // ...and must reset the counter (0, then +1 for this frame). Without
            // the reset the counter stays past the threshold forever and every
            // subsequent frame hits the deinit+rebuild branch.
            assert_eq!(r.terminal_state_frame_count, 1);
            r.update_frame(&st, true).unwrap();
            assert_eq!(r.terminal_state_frame_count, 2);

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
        // rustty-only (R13, UPSTREAM-DEFECTS.md UD57): `cursor-text`,
        // `selection-background` and `selection-foreground` set to
        // `cell-foreground` / `cell-background` leave their custom-shader
        // uniforms unchanged (the reference panics); a `.color` still writes.
        // The OpenGL twin is in `generic/linux_impl.rs`.
        #[test]
        fn update_custom_shader_uniforms_from_state_cell_relative_colors() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };
            r.has_custom_shaders = true;

            let sentinel = [0.25, 0.5, 0.75, 1.0];
            r.custom_shader_uniforms.cursor_text = sentinel;
            r.custom_shader_uniforms.selection_background_color = sentinel;
            r.custom_shader_uniforms.selection_foreground_color = sentinel;
            r.custom_shader_uniforms.foreground_color = sentinel;
            // Sentinels for the two uniforms written AFTER the three colour
            // blocks. Neither is a value the function can produce, so they
            // pin that the function ran to its end rather than returning
            // early out of a colour block.
            r.custom_shader_uniforms.cursor_visible = -7;
            r.custom_shader_uniforms.current_cursor_style = -7;
            r.config.cursor_text = Some(crate::generic::TerminalColor::CellForeground);
            r.config.selection_background = Some(crate::generic::TerminalColor::CellForeground);
            r.config.selection_foreground = Some(crate::generic::TerminalColor::CellBackground);

            let mut t = Terminal::init(Global, opts(10, 3)).unwrap();
            r.terminal_state.update(Global, &mut t).unwrap(); // dirty = Full
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

            // Positive reach PAST the three colour blocks: everything the
            // function writes after them is written. A skipped uniform must
            // not cost the rest of the update.
            assert_eq!(
                r.custom_shader_uniforms.cursor_visible,
                r.terminal_state.cursor.visible as i32
            );
            assert_ne!(r.custom_shader_uniforms.current_cursor_style, -7);

            // A `.color` value is still written.
            let red = crate::generic::Color { r: 255, g: 0, b: 0 };
            r.config.cursor_text = Some(crate::generic::TerminalColor::Color(red));
            r.config.selection_background = Some(crate::generic::TerminalColor::Color(red));
            r.config.selection_foreground = Some(crate::generic::TerminalColor::Color(red));
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

            t.deinit(Global);
        }

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

        // UD76 (R13, task R13-C2): a saturated padding (UD71, stored by
        // `contentScaleCallback` before the IO thread resizes the grid) with the
        // cursor off the first column and row. The reference's unchecked
        // `grid_pos * cell + padding` overflows `u32`; the port saturates, so the
        // cursor uniform is the saturated edge.
        #[test]
        fn custom_shader_cursor_uniform_saturates_with_a_saturated_padding() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };
            r.has_custom_shaders = true;
            r.size.padding.left = u32::MAX;
            r.size.padding.top = u32::MAX;
            let cursor = crate::shaders::CellText {
                atlas: crate::shaders::Atlas::Grayscale,
                bools: crate::shaders::CellTextBools {
                    is_cursor_glyph: true,
                    ..Default::default()
                },
                grid_pos: [3, 2],
                color: [0, 0, 0, 255],
                ..Default::default()
            };
            r.cells
                .set_cursor(Some(cursor), Some(crate::cursor::Style::Block));
            assert_eq!(Some(cursor), r.cells.get_cursor_glyph(), "precondition");

            r.update_custom_shader_uniforms_for_frame();

            let c = r.custom_shader_uniforms.current_cursor.0;
            assert_eq!(u32::MAX as f32, c[0], "x saturates: {c:?}");
            assert!(c[1] >= u32::MAX as f32, "y saturates: {c:?}");
        }

        // RESIZE (§5): `set_screen_size` stores the new padding + recomputes the
        // screen-size uniforms. Offscreen-testable directly.
        #[test]
        #[allow(clippy::neg_multiply)]
        fn set_screen_size_updates_padding_and_uniforms() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };
            let mut size = r.size;
            size.padding = Padding {
                top: 4,
                bottom: 4,
                left: 4,
                right: 4,
            };
            r.set_screen_size(size);
            assert_eq!(
                r.size.padding,
                Padding {
                    top: 4,
                    bottom: 4,
                    left: 4,
                    right: 4,
                }
            );
            // The projection is recomputed with the new padding
            // (`-1 * padding.left/top`, mirroring `update_screen_size_uniforms`).
            let ts = r.size.terminal();
            let expected = math::ortho2d(
                -1.0 * r.size.padding.left as f32,
                (ts.width + r.size.padding.right) as f32,
                (ts.height + r.size.padding.bottom) as f32,
                -1.0 * r.size.padding.top as f32,
            );
            assert_eq!(r.uniforms.projection_matrix, expected);
        }

        // RESIZE (§5): `set_font_grid` swaps the grid handle, resets each frame's
        // atlas-sync counters, recomputes the cell size + font-grid uniforms, and
        // marks the screen dirty.
        #[test]
        fn set_font_grid_swaps_grid_and_resets_modified() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };
            // Dirty a frame's modified counters, then confirm the reset.
            r.swap_chain.as_mut().unwrap().frames[0].grayscale_modified = 7;
            r.swap_chain.as_mut().unwrap().frames[0].color_modified = 5;
            let grid = r.font_grid; // reuse the same live grid handle
            r.set_font_grid(grid);
            assert_eq!(
                r.swap_chain.as_ref().unwrap().frames[0].grayscale_modified,
                0
            );
            assert_eq!(r.swap_chain.as_ref().unwrap().frames[0].color_modified, 0);
            let m = unsafe { (*grid).metrics };
            assert_eq!(
                r.size.cell,
                CellSize {
                    width: m.cell_width,
                    height: m.cell_height,
                }
            );
            assert_eq!(
                r.uniforms.cell_size,
                [m.cell_width as f32, m.cell_height as f32]
            );
            assert_eq!(r.terminal_state.dirty, Dirty::Full); // mark_dirty
        }

        // CONFIG-SWAP (§5): `change_config` reinits the shaper, updates the
        // blending/contrast uniforms, bumps `target_config_modified`, and marks
        // dirty. `bg_image`/`custom_shaders` stay placeholders so the deferred
        // `prep_background_image` seam is unreachable (`bg_image_changed == false`).
        #[test]
        fn change_config_updates_blending_uniforms() {
            let mut r = match test_renderer(10, 3) {
                Some(r) => r,
                None => return,
            };
            let mut d = derived_config_for_test();
            d.blending = AlphaBlending::LinearCorrected;
            d.min_contrast = 2.0;
            r.change_config(d).expect("change_config");
            assert!(r.uniforms.bools.use_linear_blending);
            assert!(r.uniforms.bools.use_linear_correction);
            assert_eq!(r.uniforms.min_contrast, 2.0);
            assert!(r.reinitialize_shaders); // blending changed
            assert_eq!(r.target_config_modified, 1); // +%= 1
            assert_eq!(r.terminal_state.dirty, Dirty::Full);
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos_impl::*;

// L4 (Linux/FreeBSD): the SECOND `generic.zig` `Renderer(GraphicsAPI)` instantiation,
// over `crate::opengl::OpenGL` — the sibling to `macos_impl`'s `Renderer(Metal)`.
// ADDITIVE (R5): not one byte of `macos_impl` changes; the two arms are mutually
// exclusive by target. Copy-adapted from `macos_impl` (READ-ONLY reference) against
// the same `generic.zig` oracle. (docs/superpowers/plans/2026-07-17-L4-opengl-renderer.md)
//
// PORT NOTE (W2D/F-003): `linux_impl` is the `GenericRenderer(OpenGL)` arm
// (`renderer.zig:38` `.opengl`) — it serves linux AND freebsd since W2D, because
// `backend.zig:20-21` returns `.opengl` for every native target that is not Darwin
// and FreeBSD is a real windowed target at the pin (`apprt/runtime.zig:18`). The
// module keeps its `linux_impl` NAME deliberately: renaming it would churn a file
// several concurrent lanes are editing for zero fidelity gain (the oracle has no
// per-OS name here at all — it is one comptime instantiation).
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux_impl;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub use linux_impl::*;
