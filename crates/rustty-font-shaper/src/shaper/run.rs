//! rustty-font-shaper::shape::run
//!
//! PORTS: ghostty `src/font/shaper/run.zig` (409 loc, 0 tests) — the `TextRun`
//! and the `RunIterator` that splits a terminal row into shapeable runs.

// const std = @import("std");
// const assert = @import("../../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const font = @import("../main.zig");
// const shape = @import("../shape.zig");
// const terminal = @import("../../terminal/main.zig");
// const autoHash = std.hash.autoHash;
// const Hasher = std.hash.Wyhash;
use allocator_api2::alloc::Allocator;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher as _};

use rustty_util::quirks::inlineAssert as assert;

use crate::collection;
use crate::shape;
use crate::shared_grid::SharedGrid;
// `crate::Style` is the FONT style enum (`font.Style`); the terminal cell style
// (`terminal.Style`) is a distinct struct — aliased `TermStyle` to disambiguate.
use crate::{Presentation, Style};
use rustty_vt::style::Style as TermStyle;

// PORT NOTE (hasher seam): the reference hashes runs with `std.hash.Wyhash`
// seeded 0 via `std.hash.autoHash`. This crate has no Wyhash; the run hash is an
// opaque, position-independent CACHE KEY that never crosses a parity boundary
// (it keys `shaper.Cache`; identical run content must hash identically, which
// any deterministic hasher satisfies), and `run.zig` has no test asserting a
// hash value. We therefore use the deterministic `DefaultHasher` (SipHash-1-3
// with fixed keys). A bit-exact Wyhash port would only be needed if a future
// parity gate compared raw hash values, which none does.
type Hasher = DefaultHasher;

fn hasher_init() -> Hasher {
    // std.hash.Wyhash.init(0)
    DefaultHasher::new()
}

fn auto_hash<T: Hash>(hasher: &mut Hasher, value: T) {
    value.hash(hasher);
}

/// The hook interface driven by `RunIterator` as it accumulates a run.
///
// PORT NOTE (DI seam): the reference field is `hooks: font.Shaper.RunIteratorHook`
// — the ACTIVE backend shaper's concrete hook (a struct holding `*Shaper`). The
// backend is chosen by `shape.zig`'s `switch`, so `run.zig` ↔ `shape.zig` ↔ the
// backend form a comptime-lazy cycle Zig resolves at analysis time. Rust has no
// lazy cross-module type cycle, so the hook is abstracted behind this trait
// (`RunIterator` is generic over `H: RunIteratorHook`); each backend's hook
// implements it. This adds nothing to the logic — it only breaks the type cycle
// (R1/R4 glue). The methods are infallible here: the reference's `!void` failure
// mode is OOM (`ArrayList.append`), which this crate handles by aborting on
// allocation failure (the crate-wide convention), so `try` sites collapse to
// direct calls (`prepare`/`finalize` are already inferred-empty-error-set in the
// reference and called without `try`).
pub trait RunIteratorHook {
    // pub fn prepare(self: *RunIteratorHook) !void
    fn prepare(&mut self);
    // pub fn addCodepoint(self: RunIteratorHook, cp: u32, cluster: u32) !void
    fn add_codepoint(&mut self, cp: u32, cluster: u32);
    // pub fn finalize(self: RunIteratorHook) !void
    fn finalize(&mut self);
}

/// A single text run. A text run is only valid for one Shaper instance and
/// until the next run is created. A text run never goes across multiple
/// rows in a terminal, so it is guaranteed to always be one line.
// pub const TextRun = struct { ... };
#[derive(Clone, Copy)]
pub struct TextRun {
    /// A unique hash for this run. This can be used to cache the shaping
    /// results. We don't provide a means to compare actual values if the
    /// hash is the same, so we should continue to improve this hash to
    /// lower the chance of hash collisions if they become a problem. If
    /// there are hash collisions, it would result in rendering issues but
    /// the core data would be correct.
    ///
    /// The hash is position-independent within the row by using relative
    /// cluster positions. This allows identical runs in different positions
    /// to share the same cache entry, improving cache efficiency.
    pub hash: u64,

    /// The offset in the row where this run started. This is added to the
    /// X position of the final shaped cells to get the absolute position
    /// in the row where they belong.
    pub offset: u16,

    /// The total number of cells produced by this run.
    pub cells: u16,

    /// The font grid that built this run.
    // grid: *font.SharedGrid,
    pub grid: *mut SharedGrid,

    /// The font index to use for the glyphs of this run.
    // font_index: font.Collection.Index,
    pub font_index: collection::Index,
}

/// RunIterator is an iterator that yields text runs.
// pub const RunIterator = struct {
//     hooks: font.Shaper.RunIteratorHook,
//     opts: shape.RunOptions,
//     i: usize = 0,
//     ...
// };
pub struct RunIterator<'a, H: RunIteratorHook> {
    pub hooks: H,
    pub opts: shape::RunOptions<'a>,
    pub i: usize,
}

/// Result of the `font_info:` labeled block in `next`.
struct FontInfo {
    idx: collection::Index,
    fallback: Option<u32>,
}

impl<'a, H: RunIteratorHook> RunIterator<'a, H> {
    // pub fn next(self: *RunIterator, alloc: Allocator) !?TextRun {
    pub fn next<A: Allocator + Copy>(
        &mut self,
        alloc: A,
    ) -> Result<Option<TextRun>, collection::Error> {
        // const slice = &self.opts.cells;
        // const cells: []const terminal.page.Cell = slice.items(.raw);
        // const graphemes: []const []const u21 = slice.items(.grapheme);
        // const styles: []const terminal.Style = slice.items(.style);
        //
        // PORT NOTE: the reference's cells are a `MultiArrayList` split into
        // parallel `.raw`/`.grapheme`/`.style` arrays; rustty's `RenderState`
        // cells are an AoS `render::Cell` carrying all three, so `cells[i]` /
        // `graphemes[i]` / `styles[i]` become `cells[i].raw` / `cells[i].grapheme`
        // / `cells[i].style`. `cells` is a `Copy` of the `&'a` slice, independent
        // of `self`, so `&mut self` helper calls below don't conflict.
        let cells = self.opts.cells;
        let grid = self.opts.grid;
        let selection = self.opts.selection;
        let cursor_x = self.opts.cursor_x;

        // Trim the right side of a row that might be empty
        let max: usize = 'max: {
            for i in 0..cells.len() {
                let rev_i = cells.len() - i - 1;
                if !cells[rev_i].raw.is_empty() {
                    break 'max rev_i + 1;
                }
            }

            0
        };

        // Invisible cells don't have any glyphs rendered,
        // so we explicitly skip them in the shaping process.
        while self.i < max
            && (cells[self.i].raw.has_styling() && cells[self.i].style.flags.invisible)
        {
            self.i += 1;
        }

        // We're over at the max
        if self.i >= max {
            return Ok(None);
        }

        // Track the font for our current run
        // var current_font: font.Collection.Index = .{};
        let mut current_font = collection::Index(0);

        // Allow the hook to prepare
        self.hooks.prepare();

        // Initialize our hash for this run.
        let mut hasher = hasher_init();

        // Let's get our style that we'll expect for the run.
        let style: TermStyle = if cells[self.i].raw.has_styling() {
            cells[self.i].style
        } else {
            TermStyle::default()
        };

        // Go through cell by cell and accumulate while we build our run.
        let mut j: usize = self.i;
        'run: while j < max {
            'cont: {
                // Use relative cluster positions (offset from run start) to make
                // the shaping cache position-independent. This ensures that runs
                // with identical content but different starting positions in the
                // row produce the same hash, enabling cache reuse.
                let cluster = j - self.i;
                // const cell: *const terminal.page.Cell = &cells[j];
                let cell = &cells[j].raw;

                // If we have a selection and we're at a boundary point, then
                // we break the run here.
                if let Some(bounds) = selection {
                    if j > self.i {
                        if bounds[0] > 0 && j == bounds[0] as usize {
                            break 'run;
                        }
                        if bounds[1] > 0 && j == bounds[1] as usize + 1 {
                            break 'run;
                        }
                    }
                }

                // If we're a spacer, then we ignore it
                match cell.wide() {
                    rustty_vt::page::Wide::Narrow | rustty_vt::page::Wide::Wide => {}
                    rustty_vt::page::Wide::SpacerHead | rustty_vt::page::Wide::SpacerTail => {
                        break 'cont;
                    }
                }

                // If our cell attributes are changing, then we split the run.
                // This prevents a single glyph for ">=" to be rendered with
                // one color when the two components have different styling.
                if j > self.i {
                    'style: {
                        let prev_cell = cells[j - 1].raw;

                        // If the prev cell and this cell are both plain
                        // codepoints then we check if they are commonly "bad"
                        // ligatures and spit the run if they are.
                        if prev_cell.content_tag() == rustty_vt::page::ContentTag::Codepoint
                            && cell.content_tag() == rustty_vt::page::ContentTag::Codepoint
                        {
                            let prev_cp = prev_cell.codepoint();
                            match prev_cp {
                                // fl, fi
                                cp if cp == 'f' as u32 => {
                                    let cp = cell.codepoint();
                                    if cp == 'l' as u32 || cp == 'i' as u32 {
                                        break 'run;
                                    }
                                }

                                // st
                                cp if cp == 's' as u32 => {
                                    let cp = cell.codepoint();
                                    if cp == 't' as u32 {
                                        break 'run;
                                    }
                                }

                                _ => {}
                            }
                        }

                        // If the style is exactly the change then fast path out.
                        if prev_cell.style_id() == cell.style_id() {
                            break 'style;
                        }

                        // The style is different. We allow differing background
                        // styles but any other change results in a new run.
                        let c1 = comparable_style(style);
                        let c2 = comparable_style(if cell.has_styling() {
                            cells[j].style
                        } else {
                            TermStyle::default()
                        });
                        if !c1.eql(&c2) {
                            break 'run;
                        }
                    }
                }

                // Text runs break when font styles change so we need to get
                // the proper style.
                let font_style: Style = 'fstyle: {
                    if style.flags.bold {
                        if style.flags.italic {
                            break 'fstyle Style::BoldItalic;
                        }
                        break 'fstyle Style::Bold;
                    }

                    if style.flags.italic {
                        break 'fstyle Style::Italic;
                    }
                    break 'fstyle Style::Regular;
                };

                // Determine the presentation format for this glyph.
                let presentation: Option<Presentation> = if cell.has_grapheme() {
                    'p: {
                        // We only check the FIRST codepoint because I believe the
                        // presentation format must be directly adjacent to the codepoint.
                        let cps = &cells[j].grapheme;
                        assert(!cps.is_empty());
                        if cps[0] == 0xFE0E {
                            break 'p Some(Presentation::Text);
                        }
                        if cps[0] == 0xFE0F {
                            break 'p Some(Presentation::Emoji);
                        }
                        None
                    }
                } else {
                    // If we're not a grapheme, our individual char could be
                    // an emoji so we want to check if we expect emoji presentation.
                    // The font grid indexForCodepoint we use below will do this
                    // automatically.
                    None
                };

                // If our cursor is on this line then we break the run around the
                // cursor. This means that any row with a cursor has at least
                // three breaks: before, exactly the cursor, and after.
                //
                // We do not break a cell that is exactly the grapheme. If there
                // are cells following that contain joiners, we allow those to
                // break. This creates an effect where hovering over an emoji
                // such as a skin-tone emoji is fine, but hovering over the
                // joiners will show the joiners allowing you to modify the
                // emoji.
                if !cell.has_grapheme() {
                    if let Some(cursor_x) = cursor_x {
                        // Exactly: self.i is the cursor and we iterated once. This
                        // means that we started exactly at the cursor and did at
                        // exactly one iteration. Why exactly one? Because we may
                        // start at our cursor but do many if our cursor is exactly
                        // on an emoji.
                        if self.i == cursor_x && j == self.i + 1 {
                            break 'run;
                        }

                        // Before: up to and not including the cursor. This means
                        // that we started before the cursor (self.i < cursor_x)
                        // and j is now at the cursor meaning we haven't yet processed
                        // the cursor.
                        if self.i < cursor_x && j == cursor_x {
                            assert(j > 0);
                            break 'run;
                        }

                        // After: after the cursor. We don't need to do anything
                        // special, we just let the run complete.
                    }
                }

                // We need to find a font that supports this character. If
                // there are additional zero-width codepoints (to form a single
                // grapheme, i.e. combining characters), we need to find a font
                // that supports all of them.
                let font_info: FontInfo = 'font_info: {
                    // If we find a font that supports this entire grapheme
                    // then we use that.
                    if let Some(idx) = self.index_for_cell(
                        alloc,
                        cell,
                        &cells[j].grapheme,
                        font_style,
                        presentation,
                    )? {
                        break 'font_info FontInfo {
                            idx,
                            fallback: None,
                        };
                    }

                    // Otherwise we need a fallback character. Prefer the
                    // official replacement character.
                    if let Some(idx) = unsafe {
                        (*grid).get_index(
                            alloc,
                            0xFFFD, // replacement char
                            font_style,
                            presentation,
                        )?
                    } {
                        break 'font_info FontInfo {
                            idx,
                            fallback: Some(0xFFFD),
                        };
                    }

                    // Fallback to space
                    if let Some(idx) =
                        unsafe { (*grid).get_index(alloc, ' ' as u32, font_style, presentation)? }
                    {
                        break 'font_info FontInfo {
                            idx,
                            fallback: Some(' ' as u32),
                        };
                    }

                    // We can't render at all. This is a bug, we should always
                    // have a font that can render a space.
                    unreachable!();
                };

                //log.warn("char={x} info={}", .{ cell.char, font_info });
                if j == self.i {
                    current_font = font_info.idx;
                }

                // If our fonts are not equal, then we're done with our run.
                if font_info.idx != current_font {
                    break 'run;
                }

                // If we're a fallback character, add that and continue; we
                // don't want to add the entire grapheme.
                if let Some(cp) = font_info.fallback {
                    self.add_codepoint(&mut hasher, cp, cluster as u32);
                    break 'cont;
                }

                // If we're a Kitty unicode placeholder then we add a blank.
                if cell.codepoint() == rustty_vt::kitty::graphics_unicode::PLACEHOLDER {
                    self.add_codepoint(&mut hasher, ' ' as u32, cluster as u32);
                    break 'cont;
                }

                // Add all the codepoints for our grapheme
                self.add_codepoint(
                    &mut hasher,
                    if cell.codepoint() == 0 {
                        ' ' as u32
                    } else {
                        cell.codepoint()
                    },
                    cluster as u32,
                );
                if cell.has_grapheme() {
                    for &cp in &cells[j].grapheme {
                        // Do not send presentation modifiers
                        if cp == 0xFE0E || cp == 0xFE0F {
                            continue;
                        }
                        self.add_codepoint(&mut hasher, cp, cluster as u32);
                    }
                }
            }

            // `while (...) : (j += 1)` — the continuation runs after each
            // iteration, including on `continue` (`break 'cont`).
            j += 1;
        }

        // Finalize our buffer
        self.hooks.finalize();

        // Add our length to the hash as an additional mechanism to avoid collisions
        auto_hash(&mut hasher, j - self.i);

        // Add our font index
        // autoHash(&hasher, current_font);
        auto_hash(&mut hasher, current_font.int());

        // Move our cursor. Must defer since we use self.i below.
        // defer self.i = j;
        let result = TextRun {
            hash: hasher.finish(),
            offset: self.i as u16,
            cells: (j - self.i) as u16,
            grid: self.opts.grid,
            font_index: current_font,
        };
        self.i = j;

        Ok(Some(result))
    }

    // fn addCodepoint(self: *RunIterator, hasher: anytype, cp: u32, cluster: u32) !void {
    fn add_codepoint(&mut self, hasher: &mut Hasher, cp: u32, cluster: u32) {
        auto_hash(hasher, cp);
        auto_hash(hasher, cluster);
        self.hooks.add_codepoint(cp, cluster);
    }

    /// Find a font index that supports the grapheme for the given cell,
    /// or null if no such font exists.
    ///
    /// This is used to find a font that supports the entire grapheme.
    /// We look for fonts that support each individual codepoint and then
    /// find the common font amongst all candidates.
    // fn indexForCell(self, alloc, cell, graphemes, style, presentation) !?font.Collection.Index {
    fn index_for_cell<A: Allocator + Copy>(
        &mut self,
        alloc: A,
        cell: &rustty_vt::page::Cell,
        graphemes: &[u32],
        style: Style,
        presentation: Option<Presentation>,
    ) -> Result<Option<collection::Index>, collection::Error> {
        let grid = self.opts.grid;

        if cell.is_empty()
            || cell.codepoint() == 0
            || cell.codepoint() == rustty_vt::kitty::graphics_unicode::PLACEHOLDER
        {
            return unsafe { (*grid).get_index(alloc, ' ' as u32, style, presentation) };
        }

        // Get the font index for the primary codepoint.
        let primary_cp: u32 = cell.codepoint();
        let primary = match unsafe { (*grid).get_index(alloc, primary_cp, style, presentation)? } {
            Some(idx) => idx,
            None => return Ok(None),
        };

        // Easy, and common: we aren't a multi-codepoint grapheme, so
        // we just return whatever index for the cell codepoint.
        if !cell.has_grapheme() {
            return Ok(Some(primary));
        }

        // If this is a grapheme, we need to find a font that supports
        // all of the codepoints in the grapheme.
        // var candidates = try ArrayList(Index).initCapacity(alloc, graphemes.len + 1);
        // defer candidates.deinit(alloc);
        let mut candidates: Vec<collection::Index> = Vec::with_capacity(graphemes.len() + 1);
        // candidates.appendAssumeCapacity(primary);
        candidates.push(primary);

        for &cp in graphemes {
            // Ignore Emoji ZWJs
            if cp == 0xFE0E || cp == 0xFE0F || cp == 0x200D {
                continue;
            }

            // Find a font that supports this codepoint. If none support this
            // then the whole grapheme can't be rendered so we return null.
            //
            // We explicitly do not require the additional grapheme components
            // to support the base presentation, since it is common for emoji
            // fonts to support the base emoji with emoji presentation but not
            // certain ZWJ-combined characters like the male and female signs.
            let idx = match unsafe { (*grid).get_index(alloc, cp, style, None)? } {
                Some(idx) => idx,
                None => return Ok(None),
            };
            candidates.push(idx);
        }

        // We need to find a candidate that has ALL of our codepoints
        for &idx in &candidates {
            if !unsafe { (*grid).has_codepoint(idx, primary_cp, presentation) } {
                continue;
            }
            // PORT NOTE: Zig `for (...) |cp| { ... } else { return idx; }` — the
            // `else` runs only if the inner loop completes without `break`.
            let mut all = true;
            for &cp in graphemes {
                // Ignore Emoji ZWJs
                if cp == 0xFE0E || cp == 0xFE0F || cp == 0x200D {
                    continue;
                }
                if !unsafe { (*grid).has_codepoint(idx, cp, None) } {
                    all = false;
                    break;
                }
            }
            if all {
                // If the while completed, then we have a candidate that
                // supports all of our codepoints.
                return Ok(Some(idx));
            }
        }

        Ok(None)
    }
}

/// Returns a style that when compared must be identical for a run to
/// continue.
// fn comparableStyle(style: terminal.Style) terminal.Style {
fn comparable_style(style: TermStyle) -> TermStyle {
    let mut s = style;

    // We allow background colors to differ because we'll just paint the
    // cell background whatever the style is, and wherever the glyph
    // lands on top of it will be the color of the glyph.
    s.bg_color = rustty_vt::style::Color::None;

    s
}
