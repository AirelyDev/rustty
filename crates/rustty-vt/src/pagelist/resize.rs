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
//! PORT STATUS: stub (sub-batch 3 prep). Method bodies + tests land with the
//! MemoryPool port (see `mod.rs` PORT BLOCKER).

use super::PageList;

impl PageList {}
