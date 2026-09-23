//! Fonts that can be embedded with Ghostty. Note they are only actually
//! embedded in the binary if they are referenced by the code, so fonts
//! used for tests will not result in the final binary being larger.
//!
//! Be careful to ensure that any fonts you embed are licensed for
//! redistribution and include their license as necessary.

// PORT NOTE: the reference uses `@embedFile("<name>")` which loads the file
// bytes at compile time. In Rust this is `include_bytes!("<path>")`, which
// yields a `&'static [u8; N]`. The reference `@embedFile` of a `[:0]const u8`
// for the non-`res/` fonts (`variable`, `variable_italic`, `symbols_nerd_font`,
// `regular`, `bold`, `italic`, `bold_italic`) refers to build-time *anonymous
// module imports* the Zig build graph injects (lazy dependencies such as
// `nerd_fonts_symbols_only`), NOT files that live in the `font/` source tree.
// They are not consumed by any test ported into this crate (the glyf/sfnt/head
// tests use only `julia_mono`, `cozette`, and `jetbrains_mono`), so they are
// ported as documented declarations deferred until the build-dependency seam
// that supplies those binaries is ported. The `res/`-relative fonts are faithful
// source-tree fixtures and are copied into this crate's `res/` and embedded via
// `include_bytes!`.

/// Default fonts that we prefer for Ghostty.
// PORT NOTE: the reference `variable = @embedFile("jetbrains_mono_variable")` is a
// build-anonymous import of `fonts/variable/JetBrainsMono[wght].ttf` from the
// `jetbrains_mono` build dependency (JetBrains Mono 2.304, SIL OFL — redistributable
// per R3), not a `font/` source-tree file. It is required by the freetype face's
// "face name" test, so the exact dependency file is vendored into this crate's
// `res/` and embedded here. (`variable_italic` remains deferred; no ported test
// uses it.)
pub const VARIABLE: &[u8] = include_bytes!("../res/JetBrainsMono-Variable.ttf");

// PORT NOTE: the reference `variable_italic = @embedFile("jetbrains_mono_variable_italic")`
// is a build-anonymous import of `fonts/variable/JetBrainsMono-Italic[wght].ttf`
// from the `jetbrains_mono` build dependency (JetBrains Mono 2.304, SIL OFL —
// redistributable per R3), not a `font/` source-tree file. It is required by the
// `SharedGridSet` `collection` builder (the italic + bold-italic built-in fallback
// faces), so the exact dependency file is vendored into this crate's `res/` and
// embedded here.
pub const VARIABLE_ITALIC: &[u8] = include_bytes!("../res/JetBrainsMono-Variable-Italic.ttf");

/// Symbols-only nerd font.
// PORT NOTE: the reference `symbols_nerd_font = @embedFile("nerd_fonts_symbols_only")`
// is a build-anonymous import of `SymbolsNerdFont-Regular.ttf` from the
// `nerd_fonts_symbols_only` build dependency (NerdFontsSymbolsOnly 3.4.0,
// https://deps.files.ghostty.org/NerdFontsSymbolsOnly-3.4.0.tar.gz — the Symbols
// Nerd Font is MIT-licensed and redistributable per R3), not a `font/` source-tree
// file. It is required by the Collection "adjusted sizes" test (the symbol-face
// `ex_height`→`cap_height`→`lineHeight` fallback), so the exact dependency file is
// vendored into this crate's `res/` and embedded here.
pub const SYMBOLS_NERD_FONT: &[u8] = include_bytes!("../res/SymbolsNerdFont-Regular.ttf");

/// Static jetbrains mono faces, currently unused.
// PORT NOTE: the reference `regular = @embedFile("jetbrains_mono_regular")` is a
// build-anonymous import of `fonts/ttf/JetBrainsMono-Regular.ttf` from the
// `jetbrains_mono` build dependency (JetBrains Mono 2.304, SIL OFL —
// redistributable per R3), not a `font/` source-tree file. It is required by the
// CoreText face's "in-memory" test, so the exact dependency file is vendored into
// this crate's `res/` and embedded here.
pub const REGULAR: &[u8] = include_bytes!("../res/JetBrainsMono-Regular.ttf");
// PORT NOTE: `bold`/`italic`/`bold_italic` are the same dependency's
// `fonts/ttf/JetBrainsMono-{Bold,Italic,BoldItalic}.ttf`
// (`SharedDeps.zig:623-635`), vendored byte-for-byte from the pinned
// `JetBrainsMono-2.304` package (S65-F8). CORRECTION (2026-09-23): this used to
// be a DEFERRED comment, "(`bold`/`italic`/`bold_italic` remain deferred; no
// ported test uses them.)". As in the reference, a const that nothing
// references is not linked into a binary.
pub const BOLD: &[u8] = include_bytes!("../res/JetBrainsMono-Bold.ttf");
pub const ITALIC: &[u8] = include_bytes!("../res/JetBrainsMono-Italic.ttf");
pub const BOLD_ITALIC: &[u8] = include_bytes!("../res/JetBrainsMono-BoldItalic.ttf");

/// Emoji fonts
pub const EMOJI: &[u8] = include_bytes!("../res/NotoColorEmoji.ttf");
pub const EMOJI_TEXT: &[u8] = include_bytes!("../res/NotoEmoji-Regular.ttf");

// Fonts below are ONLY used for testing.

/// Fonts with general properties
pub const ARABIC: &[u8] = include_bytes!("../res/KawkabMono-Regular.ttf");

/// A font for testing which is patched with nerd font symbols.
pub const TEST_NERD_FONT: &[u8] = include_bytes!("../res/JetBrainsMonoNerdFont-Regular.ttf");

/// Specific font families below:
pub const CODE_NEW_ROMAN: &[u8] = include_bytes!("../res/CodeNewRoman-Regular.otf");
pub const INCONSOLATA: &[u8] = include_bytes!("../res/Inconsolata-Regular.ttf");
pub const GEIST_MONO: &[u8] = include_bytes!("../res/GeistMono-Regular.ttf");
pub const JETBRAINS_MONO: &[u8] = include_bytes!("../res/JetBrainsMonoNoNF-Regular.ttf");
pub const JULIA_MONO: &[u8] = include_bytes!("../res/JuliaMono-Regular.ttf");

/// Cozette is a unique font because it embeds some emoji characters
/// but has a text presentation.
pub const COZETTE: &[u8] = include_bytes!("../res/CozetteVector.ttf");

/// Monaspace has weird ligature behaviors we want to test in our shapers
/// so we embed it here.
pub const MONASPACE_NEON: &[u8] = include_bytes!("../res/MonaspaceNeon-Regular.otf");

/// Terminus TTF is a scalable font with bitmap glyphs at various sizes.
pub const TERMINUS_TTF: &[u8] = include_bytes!("../res/TerminusTTF-Regular.ttf");

/// Spleen is a monospaced bitmap font available in multiple formats.
/// Used for testing bitmap font support across different file formats.
pub const SPLEEN_BDF: &[u8] = include_bytes!("../res/spleen-8x16.bdf");
pub const SPLEEN_PCF: &[u8] = include_bytes!("../res/spleen-8x16.pcf");
pub const SPLEEN_OTB: &[u8] = include_bytes!("../res/spleen-8x16.otb");

#[cfg(test)]
mod tests {
    use super::*;

    /// `fsSelection` of the font's `OS/2` table (OpenType spec: bit 0 ITALIC,
    /// bit 5 BOLD, bit 6 REGULAR), found through the sfnt table directory.
    fn fs_selection(font: &[u8]) -> u16 {
        let be16 = |o: usize| u16::from_be_bytes([font[o], font[o + 1]]);
        let be32 = |o: usize| u32::from_be_bytes([font[o], font[o + 1], font[o + 2], font[o + 3]]);
        let num_tables = usize::from(be16(4));
        for i in 0..num_tables {
            let rec = 12 + 16 * i;
            if &font[rec..rec + 4] == b"OS/2" {
                let off = usize::try_from(be32(rec + 8)).unwrap();
                return be16(off + 62);
            }
        }
        panic!("no OS/2 table");
    }

    /// `embedded.zig:15-19` exports the four static JetBrains Mono faces,
    /// which `SharedDeps.zig:623-635` imports from the `jetbrains_mono`
    /// dependency's `fonts/ttf/` (S65-F8). Each must be the face its name
    /// says, not a copy of another.
    #[test]
    fn static_jetbrains_mono_faces_have_their_styles() {
        const ITALIC_BIT: u16 = 1 << 0;
        const BOLD_BIT: u16 = 1 << 5;
        const REGULAR_BIT: u16 = 1 << 6;
        let style = |f: &[u8]| fs_selection(f) & (ITALIC_BIT | BOLD_BIT | REGULAR_BIT);
        assert_eq!(style(REGULAR), REGULAR_BIT);
        assert_eq!(style(BOLD), BOLD_BIT);
        assert_eq!(style(ITALIC), ITALIC_BIT);
        assert_eq!(style(BOLD_ITALIC), BOLD_BIT | ITALIC_BIT);
    }
}
