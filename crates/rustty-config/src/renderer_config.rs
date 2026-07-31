//! `impl rustty_renderer::config::RendererConfig for Config`.
//!
//! THE ACYCLIC PROOF (R4/G7): `rustty-config` already DEPENDS ON `rustty-renderer`
//! (`Cargo.toml` — the `Message::ChangeConfig` back-edge), so this impl lives HERE,
//! in the config crate (the trait's method receiver's home crate — orphan rule OK),
//! NOT in the renderer (which may never name a `rustty_config` type). It adds no new
//! crate edge (`check_acyclic.sh` stays green) and lets `DerivedConfig::init<C>`
//! extract from a real `Config` end-to-end.
//!
//! Each accessor maps a `Config` field to the renderer-nameable return type declared
//! by [`rustty_renderer::config::RendererConfig`]: the hoisted `rustty-render-types`
//! value-types (returned directly — the field IS the hoisted type), std scalars, the
//! renderer-local `Color`/`TerminalColor` (mapped from the config's own), the vt
//! `BoldColor` (via `BoldColor::to_terminal`), and the input `Link` slice.
//!
//! NOTE: this impl is compile-verified by `cargo build -p rustty-config`; it is not
//! yet exercised by core (core carries its own `renderer_seam::DerivedConfig`
//! placeholder until the real `rustty_renderer::DerivedConfig` is wired in a later
//! integration step). See `.port/phase-2/RENDERER_SURFACE_INIT_RESIZE_CONFIG_DESIGN.md`
//! §1.2.

use std::ffi::CString;

use rustty_renderer::config::RendererConfig;
use rustty_renderer::generic::{Color as RColor, TerminalColor as RTerminalColor};
use rustty_renderer::thread::CustomShaderAnimation as RCustomShaderAnimation;

use rustty_render_types::{
    AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, FontShapingBreak,
    WindowColorspace, WindowPaddingColor,
};
use rustty_vt::style::BoldColor;

use crate::config_struct::Config;
use crate::value_types::{Color, CustomShaderAnimation, FontStyle, TerminalColor};

/// Map the config's own `Color` (`{r,g,b}: u8`) to the renderer-local `Color`.
fn map_color(c: Color) -> RColor {
    RColor {
        r: c.r,
        g: c.g,
        b: c.b,
    }
}

/// Map the config's own `TerminalColor` union to the renderer-local one.
fn map_terminal_color(t: TerminalColor) -> RTerminalColor {
    match t {
        TerminalColor::Color(c) => RTerminalColor::Color(map_color(c)),
        TerminalColor::CellForeground => RTerminalColor::CellForeground,
        TerminalColor::CellBackground => RTerminalColor::CellBackground,
    }
}

/// Map the config's own `CustomShaderAnimation` to the renderer-local mirror
/// (`rustty_renderer::thread::CustomShaderAnimation`). The exhaustive `match` is
/// the drift backstop: adding a config variant fails to compile until this map
/// (and the renderer-local mirror) are updated.
fn map_custom_shader_animation(a: CustomShaderAnimation) -> RCustomShaderAnimation {
    match a {
        CustomShaderAnimation::False => RCustomShaderAnimation::False,
        CustomShaderAnimation::True => RCustomShaderAnimation::True,
        CustomShaderAnimation::Always => RCustomShaderAnimation::Always,
    }
}

impl RendererConfig for Config {
    fn font_thicken(&self) -> bool {
        self.font_thicken
    }
    fn font_thicken_strength(&self) -> u8 {
        self.font_thicken_strength
    }
    fn background_opacity(&self) -> f64 {
        self.background_opacity
    }
    fn background_opacity_cells(&self) -> bool {
        self.background_opacity_cells
    }
    fn cursor_opacity(&self) -> f64 {
        self.cursor_opacity
    }
    fn faint_opacity(&self) -> f64 {
        self.faint_opacity
    }
    fn minimum_contrast(&self) -> f64 {
        self.minimum_contrast
    }
    fn background_image_opacity(&self) -> f32 {
        self.background_image_opacity
    }
    fn background_image_repeat(&self) -> bool {
        self.background_image_repeat
    }
    fn window_vsync(&self) -> bool {
        self.window_vsync
    }
    fn scroll_to_bottom_output(&self) -> bool {
        // config.@"scroll-to-bottom".output
        self.scroll_to_bottom.output
    }

    fn font_features(&self) -> Vec<Vec<u8>> {
        // config.@"font-feature".clone(alloc) -> `.list`
        //
        // PORT NOTE (fidelity): the reference item type is `[:0]const u8` and
        // the only consumer (`font.shape.Options.features` ->
        // `FeatureList.appendFromString`) reads it as a LENGTH-bounded slice,
        // never as a C string, so the RAW bytes are the faithful carrier.
        //
        // This previously bridged to `Vec<CString>` with a
        // `.expect("font-feature contains no interior NUL")`, justified by the
        // claim that `RepeatableString.list` holds non-NUL bytes. That claim is
        // FALSE: `RepeatableString::parse_cli` stores the config line's value
        // verbatim (`value.to_vec()`), so `font-feature = a<0x00>b` in a config
        // file lands in the list as `[97, 0, 98]` and aborted the process at
        // renderer init. The reference passes those same three bytes to
        // `appendFromString`, which rejects them as an unknown feature tag and
        // moves on. See `font_features_carry_interior_nul_bytes`.
        self.font_feature.list.clone()
    }

    fn font_style_bold(&self) -> bool {
        // config.@"font-style-bold" != .false
        self.font_style_bold != FontStyle::False
    }
    fn font_style_italic(&self) -> bool {
        self.font_style_italic != FontStyle::False
    }
    fn font_style_bold_italic(&self) -> bool {
        self.font_style_bold_italic != FontStyle::False
    }

    fn background(&self) -> RColor {
        map_color(self.background)
    }
    fn foreground(&self) -> RColor {
        map_color(self.foreground)
    }
    fn cursor_color(&self) -> Option<RTerminalColor> {
        self.cursor_color.map(map_terminal_color)
    }
    fn cursor_text(&self) -> Option<RTerminalColor> {
        self.cursor_text.map(map_terminal_color)
    }
    fn selection_background(&self) -> Option<RTerminalColor> {
        self.selection_background.map(map_terminal_color)
    }
    fn selection_foreground(&self) -> Option<RTerminalColor> {
        self.selection_foreground.map(map_terminal_color)
    }
    fn search_background(&self) -> RTerminalColor {
        map_terminal_color(self.search_background)
    }
    fn search_foreground(&self) -> RTerminalColor {
        map_terminal_color(self.search_foreground)
    }
    fn search_selected_background(&self) -> RTerminalColor {
        map_terminal_color(self.search_selected_background)
    }
    fn search_selected_foreground(&self) -> RTerminalColor {
        map_terminal_color(self.search_selected_foreground)
    }
    fn bold_color(&self) -> Option<BoldColor> {
        // if (config.@"bold-color") |b| b.toTerminal() else null
        self.bold_color.map(|b| b.to_terminal())
    }

    fn font_shaping_break(&self) -> FontShapingBreak {
        self.font_shaping_break
    }
    fn window_padding_color(&self) -> WindowPaddingColor {
        self.window_padding_color
    }
    fn background_image_position(&self) -> BackgroundImagePosition {
        self.background_image_position
    }
    fn background_image_fit(&self) -> BackgroundImageFit {
        self.background_image_fit
    }
    fn bg_image(&self) -> Option<rustty_renderer::generic::Path> {
        use rustty_renderer::generic::Path as RPath;
        // config `self.background_image` is `Option<crate::path::Path>`
        // (`enum { Optional(Vec<u8>), Required(Vec<u8>) }`, path.rs). Map arm for
        // arm into the renderer-local `Path` (a `CString` over the same bytes). The
        // parsed bytes carry no interior NUL (`Path::parse` dupeZ'd them), so
        // `CString::new` cannot fail on well-formed config — but map the `Result`
        // defensively: an interior NUL => treat as no bg image, matching the
        // reference's "path won't open" degradation. Byte-faithful (never a lossy
        // `String`).
        match self.background_image.as_ref()? {
            crate::path::Path::Optional(b) => CString::new(b.clone()).ok().map(RPath::Optional),
            crate::path::Path::Required(b) => CString::new(b.clone()).ok().map(RPath::Required),
        }
    }
    fn custom_shaders(&self) -> rustty_renderer::generic::RepeatablePath {
        use rustty_renderer::generic::{Path as RPath, RepeatablePath as RRepeatablePath};
        // config `self.custom_shader` is `RepeatablePath { value: Vec<crate::path::Path> }`
        // (path.rs). Map each arm into the renderer-local `Path` (a `CString` over the
        // same bytes). The parsed bytes carry no interior NUL (`Path::parse` dupeZ'd
        // them), so `CString::new` cannot fail on well-formed config; map defensively
        // (an interior NUL => drop that entry, matching the reference "path won't open"
        // degradation). Byte-faithful (never a lossy `String`). This IS the reference
        // `config.@"custom-shader".clone(alloc)` — the per-element clone is the collect.
        let value = self
            .custom_shader
            .value
            .iter()
            .filter_map(|p| match p {
                crate::path::Path::Optional(b) => CString::new(b.clone()).ok().map(RPath::Optional),
                crate::path::Path::Required(b) => CString::new(b.clone()).ok().map(RPath::Required),
            })
            .collect();
        RRepeatablePath { value }
    }
    fn window_colorspace(&self) -> WindowColorspace {
        self.window_colorspace
    }
    fn alpha_blending(&self) -> AlphaBlending {
        self.alpha_blending
    }
    fn background_blur(&self) -> BackgroundBlur {
        self.background_blur
    }

    fn links(&self) -> &[rustty_input::link::Link] {
        // config.link.links.items
        &self.link.links
    }

    fn custom_shader_animation(&self) -> RCustomShaderAnimation {
        // config.@"custom-shader-animation"
        map_custom_shader_animation(self.custom_shader_animation)
    }

    fn scrollback_compression(&self) -> bool {
        // config.@"scrollback-compression" (WS7 D3, oracle 0fb89f4ff)
        self.scrollback_compression
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_struct::Config;

    /// A config line's value is stored VERBATIM by `RepeatableString::parse_cli`,
    /// so a literal 0x00 byte in the file reaches `font-feature`'s list. The
    /// reference item type is `[:0]const u8`, read by LENGTH by its only
    /// consumer, so those bytes must be carried through unchanged — not
    /// funnelled through a `CString`, which cannot represent them and used to
    /// abort the process here at renderer init.
    ///
    /// Feeds the actual malformed config TEXT rather than poking the list, so
    /// the whole line-parse path is under test.
    #[test]
    fn font_features_carry_interior_nul_bytes() {
        let mut c = Config::default();
        let text: Vec<u8> = b"font-feature = liga\x00junk\nfont-feature = dlig\n".to_vec();
        let mut iter = rustty_args::args::LineIterator::init(&text);
        c.load_iter(&mut iter).unwrap();

        let feats = c.font_features();
        assert_eq!(feats.len(), 2);
        // Byte-faithful: the NUL is INTERIOR and preserved, not a terminator.
        // Truncating here would silently ENABLE `liga`, which the reference
        // (`FeatureList.appendFromString` over the full 9-byte slice) does not.
        assert_eq!(feats[0].as_slice(), b"liga\x00junk" as &[u8]);
        assert_eq!(feats[1].as_slice(), b"dlig" as &[u8]);
    }
}
