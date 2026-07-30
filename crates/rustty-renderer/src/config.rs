//! The renderer's config-accessor surface.
//!
//! THE ACYCLIC CRUX (R4/G7): `rustty-config` DEPENDS ON `rustty-renderer`
//! (`rustty-config/Cargo.toml` — the `Message::ChangeConfig` back-edge), so the
//! renderer may NEVER name a `rustty_config` type. `DerivedConfig::init<C>`
//! (`generic.rs`) reads ~30 `config.*` fields; it is generic over a bare `C`, with
//! the concrete `C = rustty_config::Config` bound higher up (in core). This module
//! defines the accessor trait it reads through: [`RendererConfig`], whose EVERY
//! return type is renderer-nameable (the hoisted `rustty-render-types` value-types,
//! the renderer-local [`Color`]/[`TerminalColor`], `rustty_vt::style::BoldColor`,
//! `rustty_input::link::Link`, or std). `Config` impls this trait IN rustty-config
//! (orphan-rule OK — the impl lives in the type's home crate; acyclic — config
//! already deps renderer), so no `renderer -> config` edge is created
//! (`check_acyclic.sh` stays green).
//!
//! See `.port/phase-2/RENDERER_SURFACE_INIT_RESIZE_CONFIG_DESIGN.md` §1.

use std::ffi::CString;

use rustty_input::link::Link as InputLink;
use rustty_render_types::{
    AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, FontShapingBreak,
    WindowColorspace, WindowPaddingColor,
};
use rustty_vt::style::BoldColor;

use crate::generic::{Color, Path, RepeatablePath, TerminalColor};

/// The accessor surface `DerivedConfig::init<C>` reads to extract the renderer's
/// derived configuration WITHOUT naming any `rustty_config` type (R4/G7). The
/// concrete `C = rustty_config::Config` impls this IN rustty-config (acyclic:
/// config already deps renderer). Return types are all renderer-nameable
/// (render-types value-types, renderer-local `Color`/`TerminalColor`, vt
/// `BoldColor`, input `Link`, or std).
///
/// PORT NOTE: this replaces the reference's direct `config.@"…"` field reads
/// inside `DerivedConfig.init(config: *const configpkg.Config)`. Each accessor is
/// one reference config field; the reference's `init`-body clamps/casts stay in
/// `DerivedConfig::init` (so the arithmetic is faithful in the renderer body),
/// while transforms that would require naming a config-crate type (the
/// `FontStyle != .false` comparison, the `RepeatableString -> Vec<CString>` bridge,
/// the config-`TerminalColor -> renderer-TerminalColor` map, `BoldColor.toTerminal`)
/// are encapsulated in the config-side impl.
pub trait RendererConfig {
    // Scalars / flags (raw config values — the reference's clamps/casts stay in
    // DerivedConfig::init so the arithmetic is faithful in the renderer body).
    fn font_thicken(&self) -> bool;
    fn font_thicken_strength(&self) -> u8;
    fn background_opacity(&self) -> f64; // init clamps @max(0,@min(1,x))
    fn background_opacity_cells(&self) -> bool;
    fn cursor_opacity(&self) -> f64; // init clamps @max(0,@min(1,x))
    fn faint_opacity(&self) -> f64; // init: @intFromFloat(@ceil(x*255))
    fn minimum_contrast(&self) -> f64; // init: @floatCast -> f32
    fn background_image_opacity(&self) -> f32;
    fn background_image_repeat(&self) -> bool;
    fn window_vsync(&self) -> bool;
    fn scroll_to_bottom_output(&self) -> bool; // config.@"scroll-to-bottom".output

    // font-feature: config `RepeatableString` (Vec<Vec<u8>>) bridged to the
    // renderer's `Vec<CString>` (the [:0]const u8 null-termination). The clone
    // collapses into the owned Vec.
    fn font_features(&self) -> Vec<CString>;

    // font-style-{bold,italic,bold-italic}: config `FontStyle` enum; the
    // reference compares `!= .false`. The comparison is encapsulated in the impl
    // (FontStyle carries a name payload -> not renderer-nameable); accessor
    // returns the resulting bool.
    fn font_style_bold(&self) -> bool; // config.@"font-style-bold" != .false
    fn font_style_italic(&self) -> bool;
    fn font_style_bold_italic(&self) -> bool;

    // Colors — renderer-local Color/TerminalColor (config maps its own
    // Color/TerminalColor -> the renderer's; init calls Color::to_terminal_rgb).
    fn background(&self) -> Color; // init: .to_terminal_rgb()
    fn foreground(&self) -> Color; // init: .to_terminal_rgb()
    fn cursor_color(&self) -> Option<TerminalColor>;
    fn cursor_text(&self) -> Option<TerminalColor>;
    fn selection_background(&self) -> Option<TerminalColor>;
    fn selection_foreground(&self) -> Option<TerminalColor>;
    fn search_background(&self) -> TerminalColor;
    fn search_foreground(&self) -> TerminalColor;
    fn search_selected_background(&self) -> TerminalColor;
    fn search_selected_foreground(&self) -> TerminalColor;
    // bold-color: config maps `?BoldColor` via .toTerminal() -> vt BoldColor.
    fn bold_color(&self) -> Option<BoldColor>;

    // Hoisted render-types value-types (config re-exports each; its field IS the
    // hoisted type, so the accessor returns it directly, by value/Copy).
    fn font_shaping_break(&self) -> FontShapingBreak;
    fn window_padding_color(&self) -> WindowPaddingColor;
    fn background_image_position(&self) -> BackgroundImagePosition;
    fn background_image_fit(&self) -> BackgroundImageFit;

    // background-image: config `?Path` -> the renderer-local [`Path`] (§1.2). The
    // optional/required arm + the null-terminated path bytes are preserved
    // byte-faithfully (`CString`, never a lossy `String`); `None` means no
    // background image configured. This replaces the reference's direct
    // `config.@"background-image"` read (a `configpkg.Path`) — routed through the
    // accessor because the renderer may not name `rustty_config::path::Path` (R4/G7).
    fn bg_image(&self) -> Option<Path>;

    // custom-shader: config `RepeatablePath` -> the renderer-local [`RepeatablePath`]
    // (a `Vec<Path>`). Each element's optional/required arm + null-terminated path
    // bytes are preserved byte-faithfully (`CString`, never a lossy `String`). This
    // replaces the reference's `config.@"custom-shader".clone(alloc)` in
    // `DerivedConfig::init` — routed through the accessor because the renderer may not
    // name `rustty_config::path::RepeatablePath` (R4/G7).
    fn custom_shaders(&self) -> RepeatablePath;
    fn window_colorspace(&self) -> WindowColorspace;
    fn alpha_blending(&self) -> AlphaBlending;
    fn background_blur(&self) -> BackgroundBlur;

    // link.links.items -> `link::Set::from_config(alloc, &[InputLink])`.
    // Borrow tied to &self; renderer names InputLink (input is a dep).
    fn links(&self) -> &[InputLink];

    // custom-shader-animation: config `CustomShaderAnimation` mapped to the
    // renderer-local mirror (thread.rs). Feeds `thread::DerivedConfig::init` — the
    // draw/animation loop reads it to decide whether custom shaders animate.
    fn custom_shader_animation(&self) -> crate::thread::CustomShaderAnimation;

    // scrollback-compression: a plain `bool`, so the accessor returns the config
    // field directly. Feeds `thread::DerivedConfig::init` (WS7 D3, oracle
    // 0fb89f4ff) — the renderer thread's idle `Compression` scheduler reads it on
    // every wake and on every timer step, so toggling it at runtime takes effect
    // without restarting the surface.
    fn scrollback_compression(&self) -> bool;
}
