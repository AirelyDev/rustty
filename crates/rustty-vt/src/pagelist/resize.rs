//! PageList resize + reflow cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `Resize` struct (+ `Cursor`),
//! `resize`, `resizeCols`, the `ReflowCursor` helper (`reflowRow`, `writeCell`,
//! `moveLastRowToNewPage`, `increaseCapacity`, `bottom`, `cursorForward`,
//! `cursorScroll`, `cursorNewPage`, `cursorScrollOrNewPage`, `cursorAbsolute`,
//! `countTrailingEmptyCells`, `copyRowMetadata`), `resizeWithoutReflow`,
//! `resizeWithoutReflowGrowCols`, `trailingBlankLines`, and
//! `trimTrailingBlankRows` cluster.
//!
//! PORT STATUS: methods ported (vt-3c). The `Resize` struct (+ `Cursor`) and
//! `trailingBlankLines` (which only needs the already-ported page/row/cell
//! accessors `Cell.hasTextAny`) land here fully ported. The rest of the cluster
//! — `resize`, `resizeCols`, the entire `ReflowCursor` helper, both
//! `resizeWithoutReflow`/`resizeWithoutReflowGrowCols`, and
//! `trimTrailingBlankRows` — is DEFERRED via a documented seam (see the PORT
//! NOTE at the bottom of this file): their bodies are driven by siblings that
//! are genuinely unported at this point in the port (the pin / iter / access /
//! erase clusters AND the page-level managed-memory API). Deferring them keeps
//! the crate compiling cumulatively; they will be re-homed fully ported (NOT
//! stubbed) when those siblings land. The 205-block test split lands in the
//! dedicated test phase.

use super::PageList;
use crate::page::Cell;
use rustty_render_types::term_size as size;

/// `PageList.Resize`
//
// PORT NOTE: the reference `Resize` is a plain options struct with all-default
// fields. It ports to a Rust struct; the `?size.CellCountInt` optionals become
// `Option<size::CellCountInt>`, the optional cursor becomes `Option<Cursor>`,
// and `reflow` keeps its `true` default. A `Default` impl reproduces the
// reference field defaults (`cols = null`, `rows = null`, `reflow = true`,
// `cursor = null`).
pub struct Resize {
    /// The new cols/cells of the screen.
    pub cols: Option<size::CellCountInt>,
    pub rows: Option<size::CellCountInt>,

    /// Whether to reflow the text. If this is false then the text will
    /// be truncated if the new size is smaller than the old size.
    pub reflow: bool,

    /// Set this to the current cursor position in the active area. Some
    /// resize/reflow behavior depends on the cursor position.
    pub cursor: Option<Cursor>,
}

impl Default for Resize {
    fn default() -> Resize {
        Resize {
            cols: None,
            rows: None,
            reflow: true,
            cursor: None,
        }
    }
}

/// `PageList.Resize.Cursor`
#[derive(Clone, Copy)]
pub struct Cursor {
    pub x: size::CellCountInt,
    pub y: size::CellCountInt,

    /// When set, this pin preserves right-side blank cells up to the cursor
    /// during reflow.
    pub pin: Option<*mut super::Pin>,
}

impl PageList {
    /// Returns the number of trailing blank lines, not to exceed max. Max
    /// is used to limit our traversal in the case of large scrollback.
    #[allow(dead_code)]
    fn trailing_blank_lines(&self, max: size::CellCountInt) -> size::CellCountInt {
        let mut count: size::CellCountInt = 0;

        // Go through our pages backwards since we're counting trailing blanks.
        let mut it = self.pages.last;
        while !it.is_null() {
            let page = it;
            let len = unsafe { (*page).data.size.rows };
            let base = unsafe { (*page).data.memory_ptr() };
            let rows = unsafe { (*page).data.rows.ptr(base) };
            for i in 0..len {
                let rev_i = len - i - 1;
                let row = unsafe { &*rows.add(rev_i as usize) };
                let cells = unsafe {
                    core::slice::from_raw_parts(
                        row.cells().ptr(base),
                        (*page).data.size.cols as usize,
                    )
                };

                // If the row has any text then we're done.
                if Cell::has_text_any(cells) {
                    return count;
                }

                // Inc count, if we're beyond max then we're done.
                count += 1;
                if count >= max {
                    return count;
                }
            }

            it = unsafe { (*page).prev };
        }

        count
    }
}

// PORT NOTE (DEFERRED SEAM): the bulk of the resize/reflow cluster belongs to
// this file's owner doc header, but its bodies are driven by siblings that are
// genuinely unported at this point in the port. Deferring them keeps the crate
// compiling cumulatively. Each is deferred to the cluster that owns its missing
// siblings, where it lands fully ported — NOT stubbed.
//
// The genuinely-unported siblings these methods require:
//
//   pin cluster (`pin.rs`):
//     * `pin` (point->pin lookup)
//     * `trackPin` / `untrackPin`
//     * `pointFromPin`
//     * `pinIsActive`
//     * `Pin.before`
//     * `Pin.rowAndCell`
//     * `Pin.rowIterator` / `Pin.cells`
//   iter cluster (`iter.rs`):
//     * `rowIterator` / `pageIterator`
//     * `PageIterator.Chunk`
//     * `Direction`
//   access cluster (`access.rs`):
//     * `getBottomRight`
//   erase cluster (`erase.rs`):
//     * `erasePage`
//   page-level managed-memory API (`page.rs`, not yet ported in that file):
//     * `Page.cloneRowFrom`
//     * `Page.getCells`
//     * `Page.setGraphemes` / `Page.lookupGrapheme` / `Page.graphemeCount` /
//       `Page.graphemeCapacity` / `Page.grapheme_alloc`
//     * `Page.setHyperlink` / `Page.lookupHyperlink` / `Page.hyperlinkCount` /
//       `Page.hyperlinkCapacity` / `Page.hyperlink_set` / `Page.string_alloc`
//     * `Page.styles.addWithId`
//
// Deferred members (re-homed fully ported when those siblings land):
//
//   * `resize`                      — needs `pinIsActive`, plus `resizeCols`
//                                     and `resizeWithoutReflow` below.
//   * `resizeCols`                  — needs `pin`, `trackPin`, `untrackPin`,
//                                     `pointFromPin`, `pinIsActive`,
//                                     `Pin.before`, `Pin.rowIterator`,
//                                     `rowIterator`, and `ReflowCursor`.
//   * `ReflowCursor` (struct)       — the entire reflow helper, including
//     and ALL of its methods:         `init`, `reflowRow`, `writeCell`,
//                                     `moveLastRowToNewPage`,
//                                     `increaseCapacity`, `bottom`,
//                                     `cursorForward`, `cursorScroll`,
//                                     `cursorNewPage`, `cursorScrollOrNewPage`,
//                                     `cursorAbsolute`,
//                                     `countTrailingEmptyCells`, and
//                                     `copyRowMetadata`. Driven by the
//                                     page-level managed-memory API above
//                                     (`cloneRowFrom`, `setGraphemes`,
//                                     `setHyperlink`, `styles.addWithId`, etc.).
//   * `resizeWithoutReflow`         — needs `pageIterator`,
//                                     `resizeWithoutReflowGrowCols`,
//                                     `trimTrailingBlankRows`, `pinIsActive`.
//   * `resizeWithoutReflowGrowCols` — needs `pageIterator`/`Chunk`,
//                                     `Page.getCells`, `Page.cloneRowFrom`.
//   * `trimTrailingBlankRows`       — needs `getBottomRight`,
//                                     `Pin.rowIterator`, `Pin.cells`,
//                                     `erasePage`.
//
// `trailingBlankLines` (which only needs the already-ported `Cell.hasTextAny`)
// and the `Resize` struct (+ `Cursor`) are ported above. None of the deferred
// members has a ported caller in any cluster landed so far, so deferring them
// does not affect any method ported here.
