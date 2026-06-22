//! PageList scroll cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `Scroll` union, `scroll`,
//! `scrollPrompt`, and `scrollClear` cluster.
//!
//! PORT STATUS: methods ported (vt-3c). `scrollClear` (which only needs `grow`
//! and the ported page/row/cell accessors) and the `Scroll` union land here
//! fully ported. `scroll` and `scrollPrompt` are DEFERRED to the pin/iter
//! clusters via a documented seam (see the PORT NOTE at the bottom of this
//! file): their entire bodies are driven by the not-yet-ported pin-cluster
//! navigation methods (`pinIsActive`, `pinIsTop`, `Pin.up`/`down`/`upOverflow`/
//! `downOverflow`, `Pin.rowAndCell`) and the iter-cluster `Pin.promptIterator`
//! (+ `Direction`/`PromptIterator`). Deferring them keeps the crate compiling
//! cumulatively; they will be re-homed fully ported (not stubbed) when those
//! clusters land. The 205-block test split lands in the dedicated test phase.

use super::PageList;
use crate::page::Row;

/// Scroll behaviors.
//
// PORT NOTE: the reference `Scroll` is a `union(enum)`. The `active`/`top`
// variants carry no payload; `row`/`delta_row`/`delta_prompt`/`pin` carry a
// value. It ports to a Rust enum with the same variants and payload types
// (`usize`/`isize`/`Pin`).
pub enum Scroll {
    /// Scroll to the active area. This is also sometimes referred to as
    /// the "bottom" of the screen. This makes it so that the end of the
    /// screen is fully visible since the active area is the bottom
    /// rows/cols of the screen.
    Active,

    /// Scroll to the top of the screen, which is the farthest back in
    /// the scrollback history.
    Top,

    /// Scroll to the given absolute row from the top. A value of zero
    /// is the top row. This row will be the first visible row in the viewport.
    /// Scrolling into or below the active area will clamp to the active area.
    Row(usize),

    /// Scroll up (negative) or down (positive) by the given number of
    /// rows. This is clamped to the "top" and "active" top left.
    DeltaRow(isize),

    /// Jump forwards (positive) or backwards (negative) a set number of
    /// prompts. If the absolute value is greater than the number of prompts
    /// in either direction, jump to the furthest prompt in that direction.
    DeltaPrompt(isize),

    /// Scroll directly to a specific pin in the page. This will be set
    /// as the top left of the viewport (ignoring the pin x value).
    Pin(super::Pin),
}

impl PageList {
    /// Clear the screen by scrolling written contents up into the scrollback.
    /// This will not update the viewport.
    pub fn scroll_clear(&mut self) -> Result<(), super::OutOfMemory> {
        // PORT NOTE: `defer self.assertIntegrity()` — reproduced by calling
        // `assert_integrity` before the (single) exit point below.

        // Go through the active area backwards to find the first non-empty
        // row. We use this to determine how many rows to scroll up.
        let non_empty: usize = 'non_empty: {
            let mut page = self.pages.last;
            let mut n: usize = 0;
            loop {
                let base = unsafe { (*page).data.memory_ptr() };
                let rows: *const Row = unsafe { (*page).data.rows.ptr(base) };
                let page_rows = unsafe { (*page).data.size.rows };
                for i in 0..page_rows {
                    let rev_i = page_rows - i - 1;
                    let row = unsafe { &*rows.add(rev_i as usize) };
                    let cells = unsafe {
                        core::slice::from_raw_parts(row.cells().ptr(base), self.cols as usize)
                    };
                    for cell in cells {
                        if !cell.is_empty() {
                            break 'non_empty self.rows as usize - n;
                        }
                    }

                    n += 1;
                    if n > self.rows as usize {
                        break 'non_empty 0;
                    }
                }

                page = unsafe { (*page).prev };
                if page.is_null() {
                    break 'non_empty 0;
                }
            }
        };

        // Scroll
        for _ in 0..non_empty {
            let _ = self.grow()?;
        }

        self.assert_integrity();
        Ok(())
    }
}

// PORT NOTE (DEFERRED SEAM): `scroll` and `scrollPrompt` belong to this file's
// owner doc header, but their entire bodies are driven by pin-cluster /
// iter-cluster siblings that are genuinely unported at this point in the port:
//
//   * `pinIsActive` / `pinIsTop`        (pin cluster — `pin.rs`)
//   * `Pin.up` / `Pin.down`             (pin cluster — `pin.rs`)
//   * `Pin.upOverflow` / `Pin.downOverflow` (pin cluster — `pin.rs`)
//   * `Pin.rowAndCell`                  (pin cluster — `pin.rs`)
//   * `Pin.promptIterator` + `Direction` / `PromptIterator` (iter cluster —
//     `iter.rs`)
//
// `scroll`/`scrollPrompt` have NO ported caller in any cluster landed so far,
// so deferring them keeps the crate compiling cumulatively. They are deferred
// to the pin/iter clusters (which own those siblings), where they land fully
// ported — NOT stubbed. The `Scroll` union and `scrollClear` (which only need
// the already-ported `grow` + page/row/cell accessors) are ported above.
