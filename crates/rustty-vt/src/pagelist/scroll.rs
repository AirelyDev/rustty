//! PageList scroll cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `Scroll` union, `scroll`,
//! `scrollPrompt`, and `scrollClear` cluster.
//!
//! PORT STATUS: methods ported (vt-3c). `scroll`, `scrollPrompt`, and
//! `scrollClear`, plus the `Scroll` union, all land here fully ported. The
//! 205-block test split lands in the dedicated test phase.

use super::{PageList, Pin, Viewport};
use crate::page::{Row, SemanticPrompt};
use crate::pagelist::iter::Direction;
use crate::pagelist::pin::PinOverflow;
use crate::point;
use rustty_render_types::term_size as size;

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
    /// Scroll the viewport. This will never create new scrollback, allocate
    /// pages, etc. This can only be used to move the viewport within the
    /// previously allocated pages.
    pub fn scroll(&mut self, behavior: Scroll) {
        // PORT NOTE: `defer self.assertIntegrity()` — reproduced by running the
        // body in `scroll_impl` (whose every `return` is an exit of this
        // function) and calling `assert_integrity` once after it returns. The
        // recursive `.row -> .delta_row` call below routes back through this
        // public `scroll`, preserving the reference's assert-on-every-exit
        // (including post-recursion) `defer` semantics.
        self.scroll_impl(behavior);
        self.assert_integrity();
    }

    fn scroll_impl(&mut self, behavior: Scroll) {
        // Special case no-scrollback mode to never allow scrolling.
        if self.explicit_max_size == 0 {
            self.viewport = Viewport::Active;
            return;
        }

        match behavior {
            Scroll::Active => self.viewport = Viewport::Active,
            Scroll::Top => self.viewport = Viewport::Top,
            Scroll::Pin(p) => {
                if self.pin_is_active(p.pin_copy()) {
                    self.viewport = Viewport::Active;
                    return;
                } else if self.pin_is_top(p.pin_copy()) {
                    self.viewport = Viewport::Top;
                    return;
                }

                unsafe { *self.viewport_pin = p };
                self.viewport = Viewport::Pin;
                self.viewport_pin_row_offset = None; // invalidate cache
            }
            Scroll::Row(n) => 'row: {
                // If we're at the top, pin the top.
                if n == 0 {
                    self.viewport = Viewport::Top;
                    break 'row;
                }

                // If we're below the top of the active area, pin the active area.
                if n >= self.total_rows - self.rows as usize {
                    self.viewport = Viewport::Active;
                    break 'row;
                }

                // See if there are any other faster paths we can take.
                match self.viewport {
                    Viewport::Top | Viewport::Active => {}
                    Viewport::Pin => {
                        if let Some(v) = self.viewport_pin_row_offset {
                            // If we have a pin and we already calculated a row offset,
                            // then we can efficiently calculate the delta and move
                            // that much from that pin.
                            let delta: isize = {
                                let n_isize: isize = n as isize;
                                let v_isize: isize = v as isize;
                                n_isize - v_isize
                            };
                            self.scroll(Scroll::DeltaRow(delta));
                            return;
                        }
                    }
                }

                // We have an accurate row offset so store it to prevent
                // calculating this again.
                self.viewport_pin_row_offset = Some(n);
                self.viewport = Viewport::Pin;

                // Slow path, we've just got to traverse the linked list and
                // get to our row. As a slight speedup, let's pick the traversal
                // that's likely faster based on our absolute row and total rows.
                let midpoint = self.total_rows / 2;
                if n < midpoint {
                    // Iterate forward from the first node.
                    let mut node_it = self.pages.first;
                    // PORT NOTE (parity 5fa37f2b7): `rem` stays `usize` so long
                    // scrollback histories don't bail to `.active`; the
                    // `CellCountInt` cast happens only at the pin `y`
                    // assignment, where `rem` is already < the page's rows.
                    let mut rem: usize = n;
                    while !node_it.is_null() {
                        let node = node_it;
                        if rem < unsafe { super::Node::rows(node) } as usize {
                            unsafe {
                                *self.viewport_pin = Pin {
                                    node,
                                    y: match size::CellCountInt::try_from(rem) {
                                        Ok(v) => v,
                                        Err(_) => {
                                            self.viewport = Viewport::Active;
                                            break 'row;
                                        }
                                    },
                                    x: 0,
                                    garbage: false,
                                };
                            }
                            break 'row;
                        }

                        rem -= unsafe { super::Node::rows(node) } as usize;
                        node_it = unsafe { (*node).next };
                    }
                } else {
                    // Iterate backwards from the last node.
                    let mut node_it = self.pages.last;
                    // PORT NOTE (parity 5fa37f2b7): as above, `rem` stays
                    // `usize`; the cast happens at the `y` assignment where
                    // `node.rows - rem` fits `CellCountInt` by construction.
                    let mut rem: usize = self.total_rows - n;
                    while !node_it.is_null() {
                        let node = node_it;
                        if rem <= unsafe { super::Node::rows(node) } as usize {
                            unsafe {
                                *self.viewport_pin = Pin {
                                    node,
                                    y: match size::CellCountInt::try_from(
                                        super::Node::rows(node) as usize - rem,
                                    ) {
                                        Ok(v) => v,
                                        Err(_) => {
                                            self.viewport = Viewport::Active;
                                            break 'row;
                                        }
                                    },
                                    x: 0,
                                    garbage: false,
                                };
                            }
                            break 'row;
                        }

                        rem -= unsafe { super::Node::rows(node) } as usize;
                        node_it = unsafe { (*node).prev };
                    }
                }

                // If we reached here, then we couldn't find the offset.
                // This feels impossible? Just clamp to active, screw it lol.
                self.viewport = Viewport::Active;
            }
            Scroll::DeltaPrompt(n) => self.scroll_prompt(n),
            Scroll::DeltaRow(n) => 'delta_row: {
                match self.viewport {
                    // If we're at the top and we're scrolling backwards,
                    // we don't have to do anything, because there's nowhere to go.
                    Viewport::Top => {
                        if n <= 0 {
                            break 'delta_row;
                        }
                    }

                    // If we're at active and we're scrolling forwards, we don't
                    // have to do anything because it'll result in staying in
                    // the active.
                    Viewport::Active => {
                        if n >= 0 {
                            break 'delta_row;
                        }
                    }

                    // If we're already a pin type, then we can fast-path our
                    // delta by simply moving the pin. This has the added benefit
                    // that we can update our row offset cache efficiently, too.
                    Viewport::Pin => match n.cmp(&0) {
                        core::cmp::Ordering::Equal => break 'delta_row,

                        core::cmp::Ordering::Less => {
                            match unsafe { (*self.viewport_pin).up_overflow((-n) as usize) } {
                                PinOverflow::Offset(new_pin) => {
                                    unsafe { *self.viewport_pin = new_pin };
                                    if let Some(v) = self.viewport_pin_row_offset.as_mut() {
                                        *v -= (-n) as usize;
                                    }
                                    break 'delta_row;
                                }

                                // If we overflow up we're at the top.
                                PinOverflow::Overflow { .. } => {
                                    self.viewport = Viewport::Top;
                                    break 'delta_row;
                                }
                            }
                        }

                        core::cmp::Ordering::Greater => {
                            match unsafe { (*self.viewport_pin).down_overflow(n as usize) } {
                                // If we offset its a valid pin but we still have to
                                // check if we're in the active area.
                                PinOverflow::Offset(new_pin) => {
                                    if self.pin_is_active(new_pin.pin_copy()) {
                                        self.viewport = Viewport::Active;
                                    } else {
                                        unsafe { *self.viewport_pin = new_pin };
                                        if let Some(v) = self.viewport_pin_row_offset.as_mut() {
                                            *v += n as usize;
                                        }
                                    }
                                    break 'delta_row;
                                }

                                // If we overflow down we're at active.
                                PinOverflow::Overflow { .. } => {
                                    self.viewport = Viewport::Active;
                                    break 'delta_row;
                                }
                            }
                        }
                    },
                }

                // Slow path: we have to calculate the new pin by moving
                // from our viewport.
                let top = self.get_top_left(point::Tag::Viewport);
                let p: Pin = if n < 0 {
                    match top.up_overflow((-n) as usize) {
                        PinOverflow::Offset(v) => v,
                        PinOverflow::Overflow { end, .. } => end,
                    }
                } else {
                    match top.down_overflow(n as usize) {
                        PinOverflow::Offset(v) => v,
                        PinOverflow::Overflow { end, .. } => end,
                    }
                };

                // If we are still within the active area, then we pin the
                // viewport to active. This isn't EXACTLY the same behavior as
                // other scrolling because normally when you scroll the viewport
                // is pinned to _that row_ even if new scrollback is created.
                // But in a terminal when you get to the bottom and back into the
                // active area, you usually expect that the viewport will now
                // follow the active area.
                if self.pin_is_active(p.pin_copy()) {
                    self.viewport = Viewport::Active;
                    return;
                }

                // If we're at the top, then just set the top. This is a lot
                // more efficient everywhere. We must check this after the
                // active check above because we prefer active if they overlap.
                if self.pin_is_top(p.pin_copy()) {
                    self.viewport = Viewport::Top;
                    return;
                }

                // Pin is not active so we need to track it.
                unsafe { *self.viewport_pin = p };
                self.viewport = Viewport::Pin;
                self.viewport_pin_row_offset = None; // invalidate cache
            }
        }
    }

    /// Jump the viewport forwards (positive) or backwards (negative) a set number of
    /// prompts (delta).
    fn scroll_prompt(&mut self, delta: isize) {
        // If we aren't jumping any prompts then we don't need to do anything.
        if delta == 0 {
            return;
        }
        let delta_start: usize = (if delta > 0 { delta } else { -delta }) as usize;
        let mut delta_rem: usize = delta_start;

        // We start at the row before or after our viewport depending on the
        // delta so that we don't land back on our current viewport.
        let start_pin = 'start: {
            let tl = self.get_top_left(point::Tag::Viewport);

            // If we're moving up we can just move the viewport up because
            // promptIterator handles jumpting to the start of prompts.
            if delta <= 0 {
                match tl.up(1) {
                    Some(v) => break 'start v,
                    None => return,
                }
            }

            // If we're moving down and we're presently at some kind of
            // prompt, we need to skip all the continuation lines because
            // promptIterator can't know if we're cutoff or continuing.
            let mut adjusted: Pin = match tl.down(1) {
                Some(v) => v,
                None => return,
            };
            if unsafe { (*tl.row_and_cell().row).semantic_prompt() } != SemanticPrompt::None {
                'skip: {
                    while unsafe { (*adjusted.row_and_cell().row).semantic_prompt() }
                        == SemanticPrompt::PromptContinuation
                    {
                        adjusted = match adjusted.down(1) {
                            Some(v) => v,
                            None => break 'skip,
                        };
                    }
                }
            }

            break 'start adjusted;
        };

        // Go through prompts delta times
        let mut it = start_pin.prompt_iterator(
            if delta > 0 {
                Direction::RightDown
            } else {
                Direction::LeftUp
            },
            None,
        );
        let mut prompt_pin: Option<Pin> = None;
        while let Some(next) = it.next() {
            prompt_pin = Some(next);
            delta_rem -= 1;
            if delta_rem == 0 {
                break;
            }
        }

        // If we found a prompt, we move to it. If the prompt is in the active
        // area we keep our viewport as active because we can't scroll DOWN
        // into the active area. Otherwise, we scroll up to the pin.
        if let Some(p) = prompt_pin {
            if self.pin_is_active(p.pin_copy()) {
                self.viewport = Viewport::Active;
            } else {
                unsafe { *self.viewport_pin = p };
                self.viewport = Viewport::Pin;
                self.viewport_pin_row_offset = None; // invalidate cache
            }
        }
    }

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
                let base = unsafe { (*super::Node::page(page)).memory_ptr() };
                let rows: *const Row = unsafe { (*super::Node::page(page)).rows.ptr(base) };
                let page_rows = unsafe { super::Node::rows(page) };
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
