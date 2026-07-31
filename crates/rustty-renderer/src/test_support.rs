//! Shared `#[cfg(test)]` mock `C` for the `RendererConfig`-bounded seams.
//!
//! The renderer CANNOT dep `rustty-config` (that IS the R4 cycle), so
//! `DerivedConfig::init<C>`, `thread::DerivedConfig::init<C>`, and
//! `Message::initChangeConfig<C>` are all exercised through this one mock `C`
//! impl-ing [`RendererConfig`] with fixed values (the config-side
//! `impl RendererConfig for Config` is compile-verified separately by
//! `cargo build -p rustty-config`). Centralized here so the three test sites
//! (generic.rs, message.rs, thread.rs) share one 41-method impl instead of
//! triplicating it.
//!
//! The fixed values mirror the `derived_config_for_test` oracle used by the
//! macOS draw-path tests; they exercise the non-trivial transforms:
//! `font-style != .false` (all `true`), the `@ceil(x*255)` cast
//! (`faint_opacity` -> 255), the `@floatCast` (min_contrast f64 -> f32), and
//! `Color::to_terminal_rgb`. `bg_image`/`custom_shaders`/`links` are
//! configurable so the Path/config-mapping + link-free tests can flow a
//! `Some(Path::Required(..))` / a `vec![Path::Required(..)]` / a live
//! `oni::Regex`-backed link through the seams (the base mock defaults them to
//! `None`/empty). `custom_shader_animation` is fixed to `True` (the config
//! default `.true`).

use rustty_input::link::Link as InputLink;
use rustty_render_types::{
    AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, FontShapingBreak,
    WindowColorspace, WindowPaddingColor,
};
use rustty_vt::style::BoldColor;

use crate::config::RendererConfig;
use crate::generic::{Color, Path, RepeatablePath, TerminalColor};
use crate::thread::CustomShaderAnimation;

pub(crate) struct MockConfig {
    pub(crate) bg_image: Option<Path>,
    pub(crate) custom_shaders: Vec<Path>,
    pub(crate) links: Vec<InputLink>,
    /// Configurable (defaults to the config default `true`, WS7 D3) so the
    /// `thread::DerivedConfig` extraction test can assert BOTH arms.
    pub(crate) scrollback_compression: bool,
}

// PORT NOTE: MANUAL `Default` — `scrollback_compression`'s config default is
// `true`, so `#[derive(Default)]` (which would give `false`) would make the mock
// disagree with the real `Config::default()`.
impl Default for MockConfig {
    fn default() -> MockConfig {
        MockConfig {
            bg_image: None,
            custom_shaders: Vec::new(),
            links: Vec::new(),
            scrollback_compression: true,
        }
    }
}

impl RendererConfig for MockConfig {
    fn font_thicken(&self) -> bool {
        false
    }
    fn font_thicken_strength(&self) -> u8 {
        0
    }
    fn background_opacity(&self) -> f64 {
        1.0
    }
    fn background_opacity_cells(&self) -> bool {
        false
    }
    fn cursor_opacity(&self) -> f64 {
        1.0
    }
    fn faint_opacity(&self) -> f64 {
        1.0
    }
    fn minimum_contrast(&self) -> f64 {
        1.0
    }
    fn background_image_opacity(&self) -> f32 {
        1.0
    }
    fn background_image_repeat(&self) -> bool {
        false
    }
    fn window_vsync(&self) -> bool {
        false
    }
    fn scroll_to_bottom_output(&self) -> bool {
        false
    }
    fn font_features(&self) -> Vec<Vec<u8>> {
        Vec::new()
    }
    fn font_style_bold(&self) -> bool {
        true
    }
    fn font_style_italic(&self) -> bool {
        true
    }
    fn font_style_bold_italic(&self) -> bool {
        true
    }
    fn background(&self) -> Color {
        Color { r: 0, g: 0, b: 0 }
    }
    fn foreground(&self) -> Color {
        Color {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        }
    }
    fn cursor_color(&self) -> Option<TerminalColor> {
        Some(TerminalColor::Color(Color {
            r: 0xFF,
            g: 0,
            b: 0,
        }))
    }
    fn cursor_text(&self) -> Option<TerminalColor> {
        None
    }
    fn selection_background(&self) -> Option<TerminalColor> {
        None
    }
    fn selection_foreground(&self) -> Option<TerminalColor> {
        None
    }
    fn search_background(&self) -> TerminalColor {
        TerminalColor::Color(Color {
            r: 0xFF,
            g: 0xE0,
            b: 0x82,
        })
    }
    fn search_foreground(&self) -> TerminalColor {
        TerminalColor::Color(Color { r: 0, g: 0, b: 0 })
    }
    fn search_selected_background(&self) -> TerminalColor {
        TerminalColor::Color(Color {
            r: 0xF2,
            g: 0xA5,
            b: 0x7E,
        })
    }
    fn search_selected_foreground(&self) -> TerminalColor {
        TerminalColor::Color(Color { r: 0, g: 0, b: 0 })
    }
    fn bold_color(&self) -> Option<BoldColor> {
        None
    }
    fn font_shaping_break(&self) -> FontShapingBreak {
        FontShapingBreak::default()
    }
    fn window_padding_color(&self) -> WindowPaddingColor {
        WindowPaddingColor::Background
    }
    fn background_image_position(&self) -> BackgroundImagePosition {
        BackgroundImagePosition::TopLeft
    }
    fn background_image_fit(&self) -> BackgroundImageFit {
        BackgroundImageFit::Contain
    }
    fn bg_image(&self) -> Option<Path> {
        // Configurable (defaults to `None`); the Path-mapping test overrides it
        // to `Some(Path::Required(CString::new(..)))`.
        self.bg_image.clone()
    }
    fn custom_shaders(&self) -> RepeatablePath {
        // Configurable (defaults to empty); the mapping test overrides it to a
        // `vec![Path::Required(CString::new(..))]`.
        RepeatablePath {
            value: self.custom_shaders.clone(),
        }
    }
    fn window_colorspace(&self) -> WindowColorspace {
        WindowColorspace::Srgb
    }
    fn alpha_blending(&self) -> AlphaBlending {
        AlphaBlending::Native
    }
    fn background_blur(&self) -> BackgroundBlur {
        BackgroundBlur::False
    }
    fn links(&self) -> &[InputLink] {
        // Configurable (defaults to empty); the leak/free test overrides it with a
        // real `InputLink` whose `oni_regex()` succeeds so `DerivedConfig::init`
        // allocates a live `OnigRegex` handle.
        &self.links
    }
    fn custom_shader_animation(&self) -> CustomShaderAnimation {
        // config default `.true`.
        CustomShaderAnimation::True
    }
    fn scrollback_compression(&self) -> bool {
        // Configurable (defaults to the config default `true`).
        self.scrollback_compression
    }
}
