//! The monomorphic GPU renderer (`GenericRenderer(Metal)`).
//!
//! PORTS: ghostty `src/renderer/generic.zig` — the `Renderer` struct decl, its
//! nested `SwapChain`/`FrameState`/`CustomShaderState`, and the `DerivedConfig`.
//!
//! Per `GPU_RD_TRAIT_DESIGN.md` §D + the committed monomorphic decision, the
//! reference `pub fn Renderer(comptime GraphicsAPI: type) type` is ported as a
//! CONCRETE `Renderer` over the concrete `Metal` leaf types (a set of one — no
//! `GraphicsAPI` trait hierarchy; OpenGL is Linux-deferred).
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
//!   Golden-gated by `macos_impl::tests::golden::golden_renders_cursor_fg_bg`. The
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
        let links = link::Set::from_config(Global.into(), config.links())
            .map_err(|_| DerivedConfigError)?;

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
        self.links.deinit(Global.into());
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
// surface.zig) — `generic.zig` constructs two of its variants,
// `.{ .scrollbar = self.scrollbar }` (:1452-1454) and
// `.{ .renderer_health = health }` (:1735-1737), and `opengl/Frame.zig`
// (`2f0b65346`) a third, `.redraw` (`Frame.zig:85` at `12752b2ac`). Core re-wraps
// these into the full `apprt::surface::Message` at the seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurfaceMessage {
    /// The scrollbar state changed for the surface (generic.zig:1452-1454).
    Scrollbar(Scrollbar),

    /// Health status change for the renderer (generic.zig:1735-1737).
    RendererHealth(Health),

    /// Renderer pushed a new frame, redraw this surface.
    Redraw,
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
    /// The device state that corresponds to the physical GPU
    /// we'd like to render to.
    //
    // P8-W6 T2 (`40d5b860d`): `device: *renderer.Device` (Options.zig:7-9), the
    // app-scoped render device the core `App` owns (`App.device`, a `DeviceBox`
    // allocation, N522) and frees after every surface. A raw `*const`: render
    // threads only read it, and a reference here would tie the renderer to a
    // borrow of the app (see `.port/phase-2/mirihar-device-box`).
    pub device: *const crate::Device,

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
mod macos_impl;

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
// PORT NOTE (task W-2, N342): Windows is the third `.opengl` target (`backend.zig:20-21`);
// its runtime is `.none`, so it builds the `apprt.embedded` arms of `OpenGL.zig`.
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
mod linux_impl;
#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "windows"))]
pub use linux_impl::*;
