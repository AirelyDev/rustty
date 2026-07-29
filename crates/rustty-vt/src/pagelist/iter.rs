//! PageList iterators cluster.
//!
//! PORTS: ghostty src/terminal/PageList.zig — the `Direction` enum and the
//! prompt/cell/row/page iterators: `PromptIterator` (`next`, `nextRightDown`,
//! `nextLeftUp`) + `promptIterator`, `CellIterator` (`next`) + `cellIterator`,
//! `RowIterator` (`next`) + `rowIterator`, and `PageIterator` (`next`,
//! `nextDown`, `nextUp`, `Chunk`) + `pageIterator`.
//!
//! PORT STATUS: methods ported (vt-3c). This file ports the `Direction` enum,
//! the four iterator structs (`PromptIterator`, `CellIterator`, `RowIterator`,
//! `PageIterator` + `PageIterator::Chunk`) with all of their `next()` /
//! `nextRightDown` / `nextLeftUp` / `nextDown` / `nextUp` / `Chunk` methods, and
//! the per-`Pin` inline iterator constructors (`Pin::pageIterator`,
//! `Pin::rowIterator`, `Pin::cellIterator`, `Pin::promptIterator`) that the
//! struct iterators are built from. The four PageList-level wrapper functions
//! (`pageIterator`/`rowIterator`/`cellIterator`/`promptIterator` on `PageList`)
//! are DEFERRED — see the PORT NOTE at the bottom of this file — because they
//! depend on `getBottomRight` from the access cluster (`access.rs`), which is
//! genuinely unported at this point in the port. The 205-block test split lands
//! in the dedicated test phase.

// PORT NOTE: the reference iterator structs (`PromptIterator`, `CellIterator`,
// `RowIterator`, `PageIterator`) each expose a manual `next()` method that
// returns `?Pin`/`?Chunk` — they are NOT `std::iter::Iterator` impls. We keep
// the `next` name to mirror the reference API verbatim, so allow clippy's
// `should_implement_trait` lint for this module.
#![allow(clippy::should_implement_trait)]

use super::{Node, Pin};
use crate::page as pagepkg;
use rustty_render_types::term_size as size;

/// Direction that iterators can move.
///
/// PORTS: `pub const Direction = enum { left_up, right_down };`
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    LeftUp,
    RightDown,
}

pub struct PromptIterator {
    /// The pin that we are currently at. Also the starting pin when
    /// initializing.
    pub current: Option<Pin>,

    /// The pin to end at or null if we end when we can't traverse
    /// anymore.
    pub limit: Option<Pin>,

    /// The direction to do the traversal.
    pub direction: Direction,
}

impl PromptIterator {
    pub const EMPTY: PromptIterator = PromptIterator {
        current: None,
        limit: None,
        direction: Direction::LeftUp,
    };

    /// Return the next pin that represents the first row in a prompt.
    /// From here, you can find the prompt input, command output, etc.
    pub fn next(&mut self) -> Option<Pin> {
        match self.direction {
            Direction::LeftUp => self.next_left_up(),
            Direction::RightDown => self.next_right_down(),
        }
    }

    pub fn next_right_down(&mut self) -> Option<Pin> {
        // Start at our current pin. If we have no current it means
        // we reached the end and we're done.
        let start: Pin = match &self.current {
            Some(p) => p.pin_copy(),
            None => return None,
        };

        // We need to traverse downwards and look for prompts.
        let mut current: Option<Pin> = Some(start);
        while let Some(p) = current {
            // Check our limit.
            let at_limit = if let Some(limit) = &self.limit {
                limit.eql(p.pin_copy())
            } else {
                false
            };

            let rac = p.row_and_cell();
            match unsafe { (*rac.row).semantic_prompt() } {
                // This row isn't a prompt. Keep looking.
                pagepkg::SemanticPrompt::None => {
                    if at_limit {
                        break;
                    }
                }

                // This is a prompt line or continuation line. In either
                // case we consider the first line the prompt, and then
                // skip over any remaining prompt lines. This handles the
                // case where scrollback pruned the prompt.
                pagepkg::SemanticPrompt::Prompt | pagepkg::SemanticPrompt::PromptContinuation => {
                    // If we're at our limit just return this prompt.
                    if at_limit {
                        self.current = None;
                        return Some(p.left(p.x as usize));
                    }

                    // Skip over any continuation lines that follow this prompt,
                    // up to our limit.
                    let mut end_pin = p.pin_copy();
                    while let Some(next_pin) = end_pin.down(1) {
                        match unsafe { (*next_pin.row_and_cell().row).semantic_prompt() } {
                            pagepkg::SemanticPrompt::PromptContinuation => {
                                if let Some(limit) = &self.limit {
                                    if limit.eql(next_pin.pin_copy()) {
                                        break;
                                    }
                                }
                            }

                            pagepkg::SemanticPrompt::Prompt | pagepkg::SemanticPrompt::None => {
                                self.current = Some(next_pin);
                                return Some(p.left(p.x as usize));
                            }
                        }
                        end_pin = next_pin;
                    }

                    self.current = None;
                    return Some(p.left(p.x as usize));
                }
            }

            current = p.down(1);
        }

        self.current = None;
        None
    }

    pub fn next_left_up(&mut self) -> Option<Pin> {
        // Start at our current pin. If we have no current it means
        // we reached the end and we're done.
        let start: Pin = match &self.current {
            Some(p) => p.pin_copy(),
            None => return None,
        };

        // We need to traverse upwards and look for prompts.
        let mut current: Option<Pin> = Some(start);
        while let Some(p) = current {
            // Check our limit.
            let at_limit = if let Some(limit) = &self.limit {
                limit.eql(p.pin_copy())
            } else {
                false
            };

            let rac = p.row_and_cell();
            match unsafe { (*rac.row).semantic_prompt() } {
                // This row isn't a prompt. Keep looking.
                pagepkg::SemanticPrompt::None => {
                    if at_limit {
                        break;
                    }
                }

                // This is a prompt line.
                pagepkg::SemanticPrompt::Prompt => {
                    self.current = if at_limit { None } else { p.up(1) };
                    return Some(p.left(p.x as usize));
                }

                // If this is a prompt continuation, then we continue
                // looking for the start of the prompt OR a non-prompt
                // line, whichever is first. The non-prompt line is to handle
                // poorly behaved programs or scrollback that's been cut-off.
                pagepkg::SemanticPrompt::PromptContinuation => {
                    // If we're at our limit just return this continuation as prompt.
                    if at_limit {
                        self.current = None;
                        return Some(p.left(p.x as usize));
                    }

                    let mut end_pin = p.pin_copy();
                    while let Some(prior) = end_pin.up(1) {
                        if let Some(limit) = &self.limit {
                            if limit.eql(prior.pin_copy()) {
                                break;
                            }
                        }

                        match unsafe { (*prior.row_and_cell().row).semantic_prompt() } {
                            // No prompt. That means our last pin is good!
                            pagepkg::SemanticPrompt::None => {
                                self.current = Some(prior);
                                return Some(end_pin.left(end_pin.x as usize));
                            }

                            // Prompt continuation, keep looking.
                            pagepkg::SemanticPrompt::PromptContinuation => {}

                            // Prompt! Found it!
                            pagepkg::SemanticPrompt::Prompt => {
                                self.current = prior.up(1);
                                return Some(prior.left(prior.x as usize));
                            }
                        }
                        end_pin = prior;
                    }

                    // No prior rows, trimmed scrollback probably.
                    self.current = None;
                    return Some(p.left(p.x as usize));
                }
            }

            current = p.up(1);
        }

        self.current = None;
        None
    }
}

pub struct CellIterator {
    pub row_it: RowIterator,
    pub cell: Option<Pin>,
}

impl CellIterator {
    pub fn next(&mut self) -> Option<Pin> {
        let cell = match &self.cell {
            Some(c) => c.pin_copy(),
            None => return None,
        };

        match self.row_it.page_it.direction {
            Direction::RightDown => {
                if (cell.x as usize) + 1 < unsafe { super::Node::cols(cell.node) } as usize {
                    // We still have cells in this row, increase x.
                    let mut copy = cell.pin_copy();
                    copy.x += 1;
                    self.cell = Some(copy);
                } else {
                    // We need to move to the next row.
                    self.cell = self.row_it.next();
                }
            }

            Direction::LeftUp => {
                if cell.x > 0 {
                    // We still have cells in this row, decrease x.
                    let mut copy = cell.pin_copy();
                    copy.x -= 1;
                    self.cell = Some(copy);
                } else {
                    // We need to move to the previous row and last col
                    if let Some(next_cell) = self.row_it.next() {
                        let mut copy = next_cell.pin_copy();
                        copy.x = unsafe { super::Node::cols(next_cell.node) } - 1;
                        self.cell = Some(copy);
                    } else {
                        self.cell = None;
                    }
                }
            }
        }

        Some(cell)
    }
}

pub struct RowIterator {
    pub page_it: PageIterator,
    pub chunk: Option<Chunk>,
    pub offset: size::CellCountInt,
}

impl RowIterator {
    pub fn next(&mut self) -> Option<Pin> {
        let chunk = self.chunk?;
        let row: Pin = Pin {
            node: chunk.node,
            y: self.offset,
            x: 0,
            garbage: false,
        };

        match self.page_it.direction {
            Direction::RightDown => {
                // Increase our offset in the chunk
                self.offset += 1;

                // If we are beyond the chunk end, we need to move to the next chunk.
                if self.offset >= chunk.end {
                    self.chunk = self.page_it.next();
                    if let Some(c) = self.chunk {
                        self.offset = c.start;
                    }
                }
            }

            Direction::LeftUp => {
                // If we are at the start of the chunk, we need to move to the
                // previous chunk.
                if self.offset == 0 {
                    self.chunk = self.page_it.next();
                    if let Some(c) = self.chunk {
                        self.offset = c.end - 1;
                    }
                } else {
                    // If we're at the start of the chunk and its a non-zero
                    // offset then we've reached a limit.
                    if self.offset == chunk.start {
                        self.chunk = None;
                    } else {
                        self.offset -= 1;
                    }
                }
            }
        }

        Some(row)
    }
}

pub struct PageIterator {
    pub row: Option<Pin>,
    pub limit: Limit,
    pub direction: Direction,
}

/// PORTS: `const Limit = union(enum) { none, count: usize, row: Pin };`
pub enum Limit {
    None,
    Count(usize),
    Row(Pin),
}

impl PageIterator {
    pub fn next(&mut self) -> Option<Chunk> {
        match self.direction {
            Direction::LeftUp => self.next_up(),
            Direction::RightDown => self.next_down(),
        }
    }

    fn next_down(&mut self) -> Option<Chunk> {
        // Get our current row location
        let row = match &self.row {
            Some(r) => r.pin_copy(),
            None => return None,
        };

        match &mut self.limit {
            Limit::None => {
                // If we have no limit, then we consume this entire page. Our
                // next row is the next page.
                self.row = {
                    let next_page = unsafe { (*row.node).next };
                    if next_page.is_null() {
                        None
                    } else {
                        Some(Pin {
                            node: next_page,
                            y: 0,
                            x: 0,
                            garbage: false,
                        })
                    }
                };

                Some(Chunk {
                    node: row.node,
                    start: row.y,
                    end: unsafe { super::Node::rows(row.node) },
                })
            }

            Limit::Count(limit) => {
                assert!(*limit > 0); // should be handled already
                let len = core::cmp::min(
                    unsafe { super::Node::rows(row.node) } - row.y,
                    *limit as size::CellCountInt,
                );
                if (len as usize) > *limit {
                    self.row = row.down(len as usize);
                    *limit -= len as usize;
                } else {
                    self.row = None;
                }

                Some(Chunk {
                    node: row.node,
                    start: row.y,
                    end: row.y + len,
                })
            }

            Limit::Row(limit_row) => {
                let limit_row_node = limit_row.node;
                let limit_row_y = limit_row.y;
                // If this is not the same page as our limit then we
                // can consume the entire page.
                if !core::ptr::eq(limit_row_node, row.node) {
                    self.row = {
                        let next_page = unsafe { (*row.node).next };
                        if next_page.is_null() {
                            None
                        } else {
                            Some(Pin {
                                node: next_page,
                                y: 0,
                                x: 0,
                                garbage: false,
                            })
                        }
                    };

                    return Some(Chunk {
                        node: row.node,
                        start: row.y,
                        end: unsafe { super::Node::rows(row.node) },
                    });
                }

                // If this is the same page then we only consume up to
                // the limit row.
                self.row = None;
                if row.y > limit_row_y {
                    return None;
                }
                Some(Chunk {
                    node: row.node,
                    start: row.y,
                    end: limit_row_y + 1,
                })
            }
        }
    }

    fn next_up(&mut self) -> Option<Chunk> {
        // Get our current row location
        let row = match &self.row {
            Some(r) => r.pin_copy(),
            None => return None,
        };

        match &mut self.limit {
            Limit::None => {
                // If we have no limit, then we consume this entire page. Our
                // next row is the next page.
                self.row = {
                    let next_page = unsafe { (*row.node).prev };
                    if next_page.is_null() {
                        None
                    } else {
                        Some(Pin {
                            node: next_page,
                            y: unsafe { super::Node::rows(next_page) } - 1,
                            x: 0,
                            garbage: false,
                        })
                    }
                };

                Some(Chunk {
                    node: row.node,
                    start: 0,
                    end: row.y + 1,
                })
            }

            Limit::Count(limit) => {
                assert!(*limit > 0); // should be handled already
                let len = core::cmp::min(row.y, *limit as size::CellCountInt);
                if (len as usize) > *limit {
                    self.row = row.up(len as usize);
                    *limit -= len as usize;
                } else {
                    self.row = None;
                }

                Some(Chunk {
                    node: row.node,
                    start: row.y - len,
                    end: row.y - 1,
                })
            }

            Limit::Row(limit_row) => {
                let limit_row_node = limit_row.node;
                let limit_row_y = limit_row.y;
                // If this is not the same page as our limit then we
                // can consume the entire page.
                if !core::ptr::eq(limit_row_node, row.node) {
                    self.row = {
                        let next_page = unsafe { (*row.node).prev };
                        if next_page.is_null() {
                            None
                        } else {
                            Some(Pin {
                                node: next_page,
                                y: unsafe { super::Node::rows(next_page) } - 1,
                                x: 0,
                                garbage: false,
                            })
                        }
                    };

                    return Some(Chunk {
                        node: row.node,
                        start: 0,
                        end: row.y + 1,
                    });
                }

                // If this is the same page then we only consume up to
                // the limit row.
                self.row = None;
                if row.y < limit_row_y {
                    return None;
                }
                Some(Chunk {
                    node: row.node,
                    start: limit_row_y,
                    end: row.y + 1,
                })
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct Chunk {
    pub node: *mut Node,

    /// Start y index (inclusive) of this chunk in the page.
    pub start: size::CellCountInt,

    /// End y index (exclusive) of this chunk in the page.
    pub end: size::CellCountInt,
}

impl Chunk {
    pub fn rows(self) -> *mut [pagepkg::Row] {
        let rows_ptr = unsafe { (*super::Node::page(self.node)).rows.ptr((*super::Node::page(self.node)).memory_ptr()) };
        core::ptr::slice_from_raw_parts_mut(
            unsafe { rows_ptr.add(self.start as usize) },
            (self.end - self.start) as usize,
        )
    }

    /// Returns true if this chunk represents every row in the page.
    pub fn full_page(self) -> bool {
        self.start == 0 && self.end == unsafe { super::Node::rows(self.node) }
    }

    /// Returns true if this chunk overlaps with the given other chunk
    /// in any way.
    pub fn overlaps(self, other: Chunk) -> bool {
        if !core::ptr::eq(self.node, other.node) {
            return false;
        }
        if self.end <= other.start {
            return false;
        }
        if self.start >= other.end {
            return false;
        }
        true
    }
}

impl Pin {
    /// Iterators. These are the same as PageList iterator funcs but operate
    /// on pins rather than points. This is MUCH more efficient than calling
    /// pointFromPin and building up the iterator from points.
    ///
    /// The limit pin is inclusive.
    #[inline]
    pub fn page_iterator(&self, direction: Direction, limit: Option<Pin>) -> PageIterator {
        if cfg!(debug_assertions) {
            if let Some(ref l) = limit {
                // Check the order according to the iteration direction.
                match direction {
                    Direction::RightDown => {
                        assert!(self.eql(l.pin_copy()) || self.before(l.pin_copy()))
                    }
                    Direction::LeftUp => {
                        assert!(self.eql(l.pin_copy()) || l.before(self.pin_copy()))
                    }
                }
            }
        }

        PageIterator {
            row: Some(self.pin_copy()),
            limit: if let Some(p) = limit {
                Limit::Row(p)
            } else {
                Limit::None
            },
            direction,
        }
    }

    #[inline]
    pub fn row_iterator(&self, direction: Direction, limit: Option<Pin>) -> RowIterator {
        let mut page_it = self.page_iterator(direction, limit);
        let chunk = match page_it.next() {
            Some(c) => c,
            None => {
                return RowIterator {
                    page_it,
                    chunk: None,
                    offset: 0,
                }
            }
        };
        RowIterator {
            offset: match direction {
                Direction::RightDown => chunk.start,
                Direction::LeftUp => chunk.end - 1,
            },
            page_it,
            chunk: Some(chunk),
        }
    }

    #[inline]
    pub fn cell_iterator(&self, direction: Direction, limit: Option<Pin>) -> CellIterator {
        let mut row_it = self.row_iterator(direction, limit);
        let mut cell = match row_it.next() {
            Some(c) => c,
            None => return CellIterator { row_it, cell: None },
        };
        cell.x = self.x;
        CellIterator {
            row_it,
            cell: Some(cell),
        }
    }

    #[inline]
    pub fn prompt_iterator(&self, direction: Direction, limit: Option<Pin>) -> PromptIterator {
        PromptIterator {
            current: Some(self.pin_copy()),
            limit,
            direction,
        }
    }
}

// PORT NOTE (DEFERRED-SEAM RESOLUTION): the four PageList-level iterator wrapper
// functions — `pageIterator`, `rowIterator`, `cellIterator`, `promptIterator` on
// `*const PageList` — belong to this file's owner doc header. Their bodies are
// driven by `getBottomRight`, which is owned by the access cluster (`access.rs`):
//
//   const bl_pin = if (bl_pt) |pt| self.pin(pt).? else
//       self.getBottomRight(tl_pt) orelse return .{...};
//
// `getBottomRight` now exists in `access.rs`, so the wrappers land there fully
// ported — NOT stubbed. The per-`Pin` inline iterators above (which the wrappers
// delegate to via `tl_pin.pageIterator(...)` etc.) ARE ported here, since they
// only depend on already-ported pin-cluster siblings (`eql`, `before`) plus the
// iterator structs in this file.
