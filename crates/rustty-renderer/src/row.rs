//! PORTS: ghostty src/renderer/row.zig
//!
//! PORT STATUS: done — `neverExtendBg`. row.zig has NO `test{}` block, so none is
//! ported (R1 satisfied); the reference file's `// TODO: Test neverExtendBg
//! function` comment is preserved verbatim below.

// const terminal = @import("../terminal/main.zig");
use rustty_vt::color;
use rustty_vt::page;
use rustty_vt::render;
use rustty_vt::style::Style;

// TODO: Test neverExtendBg function

/// Returns true if the row of this pin should never have its background
/// color extended for filling padding space in the renderer. This is
/// a set of heuristics that help making our padding look better.
//
// PORT NOTE: the reference takes two SoA projections — `cells: []const
// page.Cell` and `styles: []const Style` — obtained from the row's
// MultiArrayList via `.items(.raw)`/`.items(.style)`. rustty's render cells are
// AoS (`render::Cell` carrying `.raw` + `.style`), so this takes the single
// `&[render::Cell]` slice and reads `.raw`/`.style` inline (the same AoS
// adaptation `cell.rs::constraint_width` established). Behavior is identical.
pub fn never_extend_bg(
    row: page::Row,
    cells: &[render::Cell],
    palette: &color::Palette,
    default_background: color::RGB,
) -> bool {
    // Any semantic prompts should not have their background extended
    // because prompts often contain special formatting (such as
    // powerline) that looks bad when extended.
    match row.semantic_prompt() {
        page::SemanticPrompt::Prompt | page::SemanticPrompt::PromptContinuation => return true,
        page::SemanticPrompt::None => {}
    }

    // for (0.., cells) |x, *cell|
    for cell in cells.iter() {
        // If any cell has a default background color then we don't
        // extend because the default background color probably looks
        // good enough as an extension.
        match cell.raw.content_tag() {
            // If it is a background color cell, we check the color.
            page::ContentTag::BgColorPalette | page::ContentTag::BgColorRgb => {
                // const s: terminal.Style = if (cell.hasStyling()) styles[x] else .{};
                let s: Style = if cell.raw.has_styling() {
                    cell.style
                } else {
                    Style::default()
                };
                // const bg = s.bg(cell, palette) orelse return true;
                let bg = match s.bg(&cell.raw, palette) {
                    Some(v) => v,
                    None => return true,
                };
                // if (bg.eql(default_background)) return true;
                if bg.eql(default_background) {
                    return true;
                }
            }

            // If its a codepoint cell we can check the style.
            page::ContentTag::Codepoint | page::ContentTag::CodepointGrapheme => {
                // For codepoint containing, we also never extend bg
                // if any cell has a powerline glyph because these are
                // perfect-fit.
                match cell.raw.codepoint() {
                    // Powerline
                    0xE0B0..=0xE0C8 | 0xE0CA | 0xE0CC..=0xE0D2 | 0xE0D4 => return true,

                    _ => {}
                }

                // Never extend a cell that has a default background.
                // A default background is applied if there is no background
                // on the style or the explicitly set background
                // matches our default background.
                let s: Style = if cell.raw.has_styling() {
                    cell.style
                } else {
                    Style::default()
                };
                let bg = match s.bg(&cell.raw, palette) {
                    Some(v) => v,
                    None => return true,
                };
                if bg.eql(default_background) {
                    return true;
                }
            }
        }
    }

    false
}
